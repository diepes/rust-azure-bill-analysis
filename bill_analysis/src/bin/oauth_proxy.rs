//! OAuth 2.0 Authorization Code + PKCE proxy against Microsoft Entra ID.
//!
//! This module owns all Entra/OAuth state, types, and handlers so that mcp.rs
//! can stay focused on MCP JSON-RPC logic.

use axum::{
    Form, Json,
    extract::{Query, Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use bill_analysis::bills::repository::BillRepository;
use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, sync::Arc, time::Instant};
use tokio::sync::{RwLock, watch};

// ---------------------------------------------------------------------------
// Entra OAuth configuration
// ---------------------------------------------------------------------------

/// Entra OAuth 2.0 configuration loaded from .env / environment variables.
#[derive(Clone)]
pub struct EntraConfig {
    pub tenant_id: String,
    pub client_id: String,
    pub client_secret: String,
    /// Base URL used for both public and callback purposes.
    pub url: String,
}

impl EntraConfig {
    pub fn redirect_uri(&self) -> String {
        format!("{}/callback", self.url)
    }
    pub fn authorize_url(&self) -> String {
        format!(
            "https://login.microsoftonline.com/{}/oauth2/v2.0/authorize",
            self.tenant_id
        )
    }
    pub fn token_url(&self) -> String {
        format!(
            "https://login.microsoftonline.com/{}/oauth2/v2.0/token",
            self.tenant_id
        )
    }
    pub fn jwks_url(&self) -> String {
        format!(
            "https://login.microsoftonline.com/{}/discovery/v2.0/keys",
            self.tenant_id
        )
    }
    pub fn issuer_v2(&self) -> String {
        format!("https://login.microsoftonline.com/{}/v2.0", self.tenant_id)
    }
    pub fn issuer_v1(&self) -> String {
        format!("https://sts.windows.net/{}/", self.tenant_id)
    }
    pub fn oidc_discovery_url(&self) -> String {
        format!(
            "https://login.microsoftonline.com/{}/.well-known/openid-configuration",
            self.tenant_id
        )
    }
}

pub fn load_entra_config() -> Option<EntraConfig> {
    let tenant_id = std::env::var("ENTRA_TENANT_ID")
        .ok()
        .filter(|s| !s.is_empty())?;
    let client_id = std::env::var("ENTRA_CLIENT_ID")
        .ok()
        .filter(|s| !s.is_empty())?;
    let client_secret = std::env::var("ENTRA_CLIENT_SECRET")
        .ok()
        .filter(|s| !s.is_empty())?;
    let url = std::env::var("MCP_URL").ok().filter(|s| !s.is_empty())?;

    Some(EntraConfig {
        tenant_id,
        client_id,
        client_secret,
        url,
    })
}

// ---------------------------------------------------------------------------
// OAuth flow state types
// ---------------------------------------------------------------------------

/// Per-authorize-request state, keyed by the server-generated OAuth `state` parameter.
/// Stored between GET /authorize and GET /callback, pruned after 10 minutes.
pub struct PkceFlowState {
    pub code_challenge: String,
    pub code_challenge_method: String,
    /// The MCP client's redirect_uri — where we redirect after a successful callback.
    pub client_redirect_uri: String,
    /// The client's original `state` value, echoed back in the redirect.
    pub client_state: String,
    pub created_at: Instant,
}

/// Result of a completed (or failed) OAuth flow, stored server-side so the
/// MCP client can poll `/mcp/auth/wait?state=<client_state>` instead of
/// relying on a local callback server that may have closed.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum AuthFlowResult {
    /// Browser has been redirected to Entra; waiting for the user.
    Pending,
    /// User authenticated successfully; MCP client should exchange this code at /token.
    Complete { code: String, redirect_uri: String },
    /// Entra (or our server) returned an error.
    Error {
        error: String,
        error_description: String,
    },
}

/// Map from client_state → flow result.  Entries are created by
/// `authorize_handler` and resolved (or failed) by `callback_handler`.
pub type AuthResultStore = Arc<RwLock<HashMap<String, AuthFlowResult>>>;

// ---------------------------------------------------------------------------
// Server-managed session flow (no LLM callback server required)
// ---------------------------------------------------------------------------

/// Result broadcast to a long-polling `POST /mcp` connection when a
/// server-managed OAuth flow (started via `/auth/start`) completes.
#[derive(Clone, Debug)]
pub enum PendingAuthResult {
    Success,
    Failure { error: String, description: String },
}

/// Validated identity stored after a successful server-managed auth flow.
/// Keyed by `session_id` which doubles as the bearer token.
#[derive(Clone, Debug)]
pub struct SessionInfo {
    pub upn: String,
    #[allow(dead_code)]
    pub oid: String,
    #[allow(dead_code)]
    pub roles: Vec<String>,
    pub created_at: Instant,
}

/// Active server-managed sessions: session_id → identity.
pub type SessionStore = Arc<RwLock<HashMap<String, SessionInfo>>>;
/// Pending server-managed auth flows: session_id → watch sender.
/// The `POST /mcp` long-poll handler holds a receiver and awaits a value.
pub type PendingSessionStore =
    Arc<RwLock<HashMap<String, watch::Sender<Option<PendingAuthResult>>>>>;

/// Consumed once in POST /token; pruned after 5 minutes.
#[allow(dead_code)]
pub struct TempCodeEntry {
    pub access_token: String,
    pub code_challenge: String,
    pub code_challenge_method: String,
    pub created_at: Instant,
}

/// Cached Entra JSON Web Key Set, refreshed on key-not-found.
pub struct JwksCacheEntry {
    pub keys: Vec<Value>,
}

pub type PkceStateStore = Arc<RwLock<HashMap<String, PkceFlowState>>>;
pub type TempCodeStore = Arc<RwLock<HashMap<String, TempCodeEntry>>>;
pub type JwksCache = Arc<RwLock<Option<JwksCacheEntry>>>;

// ---------------------------------------------------------------------------
// Authenticated caller identity
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct CallerIdentity {
    pub oid: String,
    pub upn: String,
    pub roles: Vec<String>,
}

// ---------------------------------------------------------------------------
// Shared state (owned here so OAuth handlers and MCP handlers share one type)
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct AppState {
    pub repo: Arc<BillRepository>,
    /// None when --no-auth is set; all MCP callers are trusted.
    pub entra: Option<EntraConfig>,
    /// When true, JWT is still validated but the BillingViewer App Role is not required.
    pub no_role_check: bool,
    pub pkce_store: PkceStateStore,
    pub temp_codes: TempCodeStore,
    pub jwks_cache: JwksCache,
    /// Legacy: server-side auth flow results for the old long-poll endpoint.
    pub auth_results: AuthResultStore,
    /// Server-managed sessions: session_id → validated identity.
    pub sessions: SessionStore,
    /// Pending server-managed auth flows waiting for the browser to complete.
    pub pending_sessions: PendingSessionStore,
}

impl AppState {
    pub fn new(repo: Arc<BillRepository>, entra: Option<EntraConfig>, no_role_check: bool) -> Self {
        Self {
            repo,
            entra,
            no_role_check,
            pkce_store: Arc::new(RwLock::new(HashMap::new())),
            temp_codes: Arc::new(RwLock::new(HashMap::new())),
            jwks_cache: Arc::new(RwLock::new(None)),
            auth_results: Arc::new(RwLock::new(HashMap::new())),
            sessions: Arc::new(RwLock::new(HashMap::new())),
            pending_sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

// ---------------------------------------------------------------------------
// OAuth 2.0 — discovery, authorize, callback, token
// ---------------------------------------------------------------------------

pub async fn oauth_metadata_handler(State(state): State<AppState>) -> impl IntoResponse {
    let entra = match &state.entra {
        Some(e) => e,
        None => return (StatusCode::NOT_FOUND, "auth disabled").into_response(),
    };
    let base = &entra.url;
    let body = json!({
        "issuer": base,
        "authorization_endpoint": format!("{}/authorize", base),
        "token_endpoint": format!("{}/token", base),
        "response_types_supported": ["code"],
        "grant_types_supported": ["authorization_code"],
        "code_challenge_methods_supported": ["S256"],
        "scopes_supported": ["openid", "profile"],
        "registration_endpoint": format!("{}/register", base),
        "auth_wait_endpoint": format!("{}/mcp/auth/wait", base),
    });
    log::debug!("  [oauth] authorization-server metadata: {body}");
    Json(body).into_response()
}

/// RFC 7591 — Dynamic Client Registration.
/// Stateless: any valid registration request gets the server's own client_id echoed back.
pub async fn register_handler(
    State(state): State<AppState>,
    body: axum::extract::Json<Value>,
) -> Response {
    let entra = match &state.entra {
        Some(e) => e,
        None => return (StatusCode::NOT_FOUND, "auth disabled").into_response(),
    };
    let redirect_uris = body.get("redirect_uris").cloned().unwrap_or(json!([]));
    log::debug!(
        "  [oauth] /register client_name={:?} redirect_uris={redirect_uris}",
        body.get("client_name")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
    );
    Json(json!({
        "client_id": entra.client_id,
        "client_id_issued_at": 0,
        "redirect_uris": redirect_uris,
        "grant_types": ["authorization_code"],
        "response_types": ["code"],
        "token_endpoint_auth_method": "none",
    }))
    .into_response()
}

/// RFC 9728 — OAuth 2.0 Protected Resource Metadata.
pub async fn oauth_protected_resource_handler(State(state): State<AppState>) -> impl IntoResponse {
    let entra = match &state.entra {
        Some(e) => e,
        None => return (StatusCode::NOT_FOUND, "auth disabled").into_response(),
    };
    let base = &entra.url;
    let body = json!({
        "resource": format!("{}/mcp", base),
        "authorization_servers": [base],
    });
    log::debug!("  [oauth] protected-resource: {body}");
    Json(body).into_response()
}

#[derive(Deserialize)]
#[allow(dead_code)]
pub struct AuthorizeParams {
    pub response_type: Option<String>,
    pub client_id: Option<String>,
    pub redirect_uri: String,
    pub state: Option<String>,
    pub code_challenge: String,
    pub code_challenge_method: Option<String>,
    pub scope: Option<String>,
}

pub async fn authorize_handler(
    State(state): State<AppState>,
    Query(params): Query<AuthorizeParams>,
) -> Response {
    log::debug!(
        "  [oauth] /authorize called with client_id={:?}, redirect_uri={}",
        params.client_id,
        params.redirect_uri
    );

    let entra = match &state.entra {
        Some(e) => e.clone(),
        None => return (StatusCode::NOT_FOUND, "auth disabled").into_response(),
    };

    let method = params.code_challenge_method.as_deref().unwrap_or("S256");
    if method != "S256" {
        log::warn!(
            "  [oauth] FAIL /authorize unsupported code_challenge_method={}",
            method
        );
        return (
            StatusCode::BAD_REQUEST,
            "only S256 code_challenge_method supported",
        )
            .into_response();
    }

    let server_state = random_string(32);
    let default_scope = "openid profile".to_string();
    let raw_scope = params
        .scope
        .filter(|s| !s.is_empty())
        .unwrap_or(default_scope.clone());
    let scope: String = {
        let filtered: Vec<&str> = raw_scope
            .split_whitespace()
            .filter(|tok| matches!(*tok, "openid" | "profile"))
            .collect();
        if filtered.is_empty() {
            default_scope
        } else {
            filtered.join(" ")
        }
    };
    log::debug!("  [oauth] /authorize using scope: {scope:?}");

    let client_state_key = {
        let mut store = state.pkce_store.write().await;
        store.retain(|_, v| v.created_at.elapsed().as_secs() < 600);
        let client_state = params.state.clone().unwrap_or_default();
        store.insert(
            server_state.clone(),
            PkceFlowState {
                code_challenge: params.code_challenge,
                code_challenge_method: method.to_string(),
                client_redirect_uri: params.redirect_uri,
                client_state: client_state.clone(),
                created_at: Instant::now(),
            },
        );
        client_state
    };

    // Pre-register a Pending entry so the long-poll endpoint can see the flow.
    {
        let mut results = state.auth_results.write().await;
        results.retain(|_, _| true); // prune old entries lazily via poll timeout
        results.insert(client_state_key, AuthFlowResult::Pending);
    }

    let target = format!(
        "{}?response_type=code&client_id={}&redirect_uri={}&state={}&scope={}&prompt=select_account",
        entra.authorize_url(),
        url_encode(&entra.client_id),
        url_encode(&entra.redirect_uri()),
        url_encode(&server_state),
        url_encode(&scope),
    );
    log::debug!("  [oauth] /authorize redirecting to Entra: {}", target);
    Redirect::temporary(&target).into_response()
}

// ---------------------------------------------------------------------------
// Server-managed auth start: GET /auth/start?session=<session_id>
// ---------------------------------------------------------------------------

/// The LLM receives this URL in the 401 error body, opens it in the browser,
/// and the server handles the full PKCE dance on the user's behalf.
/// On completion, the pending `POST /mcp` connection is unblocked.
#[derive(Deserialize)]
pub struct AuthStartParams {
    pub session: String,
}

pub async fn auth_start_handler(
    State(state): State<AppState>,
    Query(params): Query<AuthStartParams>,
) -> Response {
    let entra = match &state.entra {
        Some(e) => e.clone(),
        None => return (StatusCode::NOT_FOUND, "auth disabled").into_response(),
    };

    log::info!("  [oauth] /auth/start session={}", params.session);

    // Register this session in pkce_store with an empty client_redirect_uri
    // to mark it as server-managed (no CLI callback server involved).
    {
        let mut store = state.pkce_store.write().await;
        store.retain(|_, v| v.created_at.elapsed().as_secs() < 600);
        store.insert(
            params.session.clone(), // session_id IS the server_state sent to Entra
            PkceFlowState {
                code_challenge: String::new(),
                code_challenge_method: String::new(),
                client_redirect_uri: String::new(), // empty = server-managed
                client_state: params.session.clone(),
                created_at: Instant::now(),
            },
        );
    }

    // Redirect browser directly to Entra.  As a confidential client we use
    // client_secret for the token exchange so no PKCE code_challenge is needed.
    let target = format!(
        "{}?response_type=code&client_id={}&redirect_uri={}&state={}&scope={}&prompt=select_account",
        entra.authorize_url(),
        url_encode(&entra.client_id),
        url_encode(&entra.redirect_uri()),
        url_encode(&params.session),
        url_encode("openid profile"),
    );
    log::debug!("  [oauth] /auth/start redirecting to Entra: {}", target);
    Redirect::temporary(&target).into_response()
}

#[derive(Deserialize)]
pub struct CallbackParams {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
    pub error_subcode: Option<String>,
    pub error_description: Option<String>,
}

pub async fn callback_handler(
    State(state): State<AppState>,
    Query(params): Query<CallbackParams>,
) -> Response {
    let entra = match &state.entra {
        Some(e) => e.clone(),
        None => return (StatusCode::NOT_FOUND, "auth disabled").into_response(),
    };

    if let Some(err) = &params.error {
        let desc = params.error_description.as_deref().unwrap_or("");
        let subcode = params.error_subcode.as_deref().unwrap_or("");
        log::warn!("  [oauth] Entra authorize error={err} subcode={subcode} description={desc}");

        let callback_url = if let Some(server_state) = &params.state {
            let client_redirect = {
                let mut store = state.pkce_store.write().await;
                store
                    .remove(server_state)
                    .map(|e| (e.client_redirect_uri, e.client_state))
            };
            if let Some((redirect_uri, client_state)) = client_redirect {
                if redirect_uri.is_empty() {
                    // Server-managed flow: notify the waiting POST /mcp long-poll directly.
                    let pending = state.pending_sessions.read().await;
                    if let Some(tx) = pending.get(&client_state) {
                        tx.send(Some(PendingAuthResult::Failure {
                            error: err.clone(),
                            description: desc.to_string(),
                        }))
                        .ok();
                    }
                    let html = build_auth_error_page(err, desc, None);
                    return (
                        StatusCode::OK,
                        [("Content-Type", "text/html; charset=utf-8")],
                        html,
                    )
                        .into_response();
                }

                // CLI-managed flow: resolve long-poll waiter and redirect to client.
                if !client_state.is_empty() {
                    let mut results = state.auth_results.write().await;
                    results.insert(
                        client_state.clone(),
                        AuthFlowResult::Error {
                            error: err.clone(),
                            error_description: desc.to_string(),
                        },
                    );
                }
                let mut url = format!(
                    "{}?error={}&error_description={}",
                    redirect_uri,
                    url_encode(err),
                    url_encode(desc),
                );
                if !client_state.is_empty() {
                    url.push_str(&format!("&state={}", url_encode(&client_state)));
                }

                // Try to notify the CLI's local callback server directly.
                let notify_url = url.clone();
                tokio::spawn(async move {
                    match reqwest::Client::new()
                        .get(&notify_url)
                        .timeout(std::time::Duration::from_secs(5))
                        .send()
                        .await
                    {
                        Ok(r) => {
                            log::debug!("  [oauth] notified CLI callback: HTTP {}", r.status())
                        }
                        Err(e) => log::warn!("  [oauth] CLI callback notification failed: {e}"),
                    }
                });

                Some(url)
            } else {
                None
            }
        } else {
            None
        };

        let html = build_auth_error_page(err, desc, callback_url.as_deref());
        return (
            StatusCode::OK,
            [("Content-Type", "text/html; charset=utf-8")],
            html,
        )
            .into_response();
    }

    let entra_code = match &params.code {
        Some(c) => c.clone(),
        None => return (StatusCode::BAD_REQUEST, "missing code").into_response(),
    };
    let server_state = match &params.state {
        Some(s) => s.clone(),
        None => return (StatusCode::BAD_REQUEST, "missing state").into_response(),
    };

    let pkce_entry = {
        let mut store = state.pkce_store.write().await;
        match store.remove(&server_state) {
            Some(e) => e,
            None => {
                log::warn!("  [oauth] FAIL callback unknown or expired state={server_state}");
                return (StatusCode::BAD_REQUEST, "unknown or expired state").into_response();
            }
        }
    };

    let client = reqwest::Client::new();
    let resp = client
        .post(entra.token_url())
        .form(&[
            ("client_id", entra.client_id.as_str()),
            ("client_secret", entra.client_secret.as_str()),
            ("grant_type", "authorization_code"),
            ("code", &entra_code),
            ("redirect_uri", &entra.redirect_uri()),
        ])
        .send()
        .await;

    let token_json: Value = match resp {
        Ok(r) => match r.json().await {
            Ok(v) => v,
            Err(e) => {
                log::warn!("  [oauth] FAIL token exchange parse error: {e}");
                return (StatusCode::BAD_GATEWAY, "token exchange failed").into_response();
            }
        },
        Err(e) => {
            log::warn!("  [oauth] FAIL token exchange request error: {e}");
            return (StatusCode::BAD_GATEWAY, "token exchange failed").into_response();
        }
    };

    let access_token = match token_json["id_token"].as_str() {
        Some(t) => t.to_string(),
        None => {
            let err = token_json["error"].as_str().unwrap_or("unknown");
            let desc = token_json["error_description"].as_str().unwrap_or("");
            log::warn!("  [oauth] FAIL token exchange entra_error={err} desc={desc}");
            // Notify pending server-managed session of failure.
            if pkce_entry.client_redirect_uri.is_empty() {
                let pending = state.pending_sessions.read().await;
                if let Some(tx) = pending.get(&pkce_entry.client_state) {
                    tx.send(Some(PendingAuthResult::Failure {
                        error: err.to_string(),
                        description: desc.to_string(),
                    }))
                    .ok();
                }
                let html = build_auth_error_page(err, desc, None);
                return (
                    StatusCode::OK,
                    [("Content-Type", "text/html; charset=utf-8")],
                    html,
                )
                    .into_response();
            }
            return (StatusCode::BAD_GATEWAY, "token exchange failed").into_response();
        }
    };

    // Server-managed flow: validate the id_token now, store the session, and
    // wake the waiting POST /mcp connection.  No code issuance needed.
    if pkce_entry.client_redirect_uri.is_empty() {
        let session_id = pkce_entry.client_state.clone();
        match validate_jwt(&access_token, &entra, &state.jwks_cache).await {
            Ok(caller) => {
                if !state.no_role_check && !caller.roles.contains(&"BillingViewer".to_string()) {
                    log::warn!(
                        "  [oauth] FAIL missing_role oid={} upn={} session={session_id}",
                        caller.oid,
                        caller.upn
                    );
                    let pending = state.pending_sessions.read().await;
                    if let Some(tx) = pending.get(&session_id) {
                        tx.send(Some(PendingAuthResult::Failure {
                            error: "forbidden".to_string(),
                            description: "Missing required App Role: BillingViewer".to_string(),
                        }))
                        .ok();
                    }
                    let html = build_auth_error_page(
                        "forbidden",
                        "Missing required App Role: BillingViewer",
                        None,
                    );
                    return (
                        StatusCode::OK,
                        [("Content-Type", "text/html; charset=utf-8")],
                        html,
                    )
                        .into_response();
                }
                log::info!(
                    "  [oauth] session authenticated oid={} upn={} session={session_id}",
                    caller.oid,
                    caller.upn
                );
                {
                    let mut sessions = state.sessions.write().await;
                    sessions.retain(|_, v| v.created_at.elapsed().as_secs() < 86400);
                    sessions.insert(
                        session_id.clone(),
                        SessionInfo {
                            upn: caller.upn,
                            oid: caller.oid,
                            roles: caller.roles,
                            created_at: Instant::now(),
                        },
                    );
                }
                {
                    let pending = state.pending_sessions.read().await;
                    if let Some(tx) = pending.get(&session_id) {
                        tx.send(Some(PendingAuthResult::Success)).ok();
                    }
                }
                let html = "<html><body style='font-family:sans-serif;text-align:center;padding:60px'>\
                    <h2>✅ Authentication successful</h2>\
                    <p>You can close this tab and return to your conversation.</p>\
                    </body></html>";
                return (
                    StatusCode::OK,
                    [("Content-Type", "text/html; charset=utf-8")],
                    html,
                )
                    .into_response();
            }
            Err(reason) => {
                log::warn!("  [oauth] FAIL JWT validation for session={session_id}: {reason}");
                let pending = state.pending_sessions.read().await;
                if let Some(tx) = pending.get(&session_id) {
                    tx.send(Some(PendingAuthResult::Failure {
                        error: "invalid_token".to_string(),
                        description: reason.clone(),
                    }))
                    .ok();
                }
                let html = build_auth_error_page("invalid_token", &reason, None);
                return (
                    StatusCode::OK,
                    [("Content-Type", "text/html; charset=utf-8")],
                    html,
                )
                    .into_response();
            }
        }
    }

    let server_code = random_string(40);
    {
        let mut codes = state.temp_codes.write().await;
        codes.retain(|_, v| v.created_at.elapsed().as_secs() < 300);
        codes.insert(
            server_code.clone(),
            TempCodeEntry {
                access_token,
                code_challenge: pkce_entry.code_challenge,
                code_challenge_method: pkce_entry.code_challenge_method,
                created_at: Instant::now(),
            },
        );
    }

    let redirect = format!(
        "{}?code={}&state={}",
        pkce_entry.client_redirect_uri,
        url_encode(&server_code),
        url_encode(&pkce_entry.client_state),
    );

    // Resolve the long-poll waiter with the success code.
    if !pkce_entry.client_state.is_empty() {
        let mut results = state.auth_results.write().await;
        results.insert(
            pkce_entry.client_state.clone(),
            AuthFlowResult::Complete {
                code: server_code,
                redirect_uri: pkce_entry.client_redirect_uri.clone(),
            },
        );
    }

    Redirect::temporary(&redirect).into_response()
}

// ---------------------------------------------------------------------------
// Long-poll: GET /mcp/auth/wait?state=<client_state>
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct AuthWaitParams {
    pub state: String,
}

/// Long-polls until the OAuth flow for `state` resolves or 90 s elapse.
/// Returns JSON matching the `AuthFlowResult` shape:
///   `{"status":"pending"}` on timeout
///   `{"status":"complete","code":"...","redirect_uri":"..."}`
///   `{"status":"error","error":"...","error_description":"..."}`
pub async fn auth_wait_handler(
    State(state): State<AppState>,
    Query(params): Query<AuthWaitParams>,
) -> Response {
    if state.entra.is_none() {
        return (StatusCode::NOT_FOUND, "auth disabled").into_response();
    }

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(90);

    loop {
        {
            let results = state.auth_results.read().await;
            if let Some(result) = results.get(&params.state) {
                match result {
                    AuthFlowResult::Pending => {} // keep waiting
                    resolved => {
                        log::debug!(
                            "  [oauth] auth_wait resolved state={} result={resolved:?}",
                            params.state
                        );
                        return Json(resolved.clone()).into_response();
                    }
                }
            } else {
                // Unknown state — likely expired or never started.
                return (
                    StatusCode::NOT_FOUND,
                    Json(json!({"status": "not_found", "error": "unknown or expired state"})),
                )
                    .into_response();
            }
        }
        if tokio::time::Instant::now() >= deadline {
            return Json(json!({"status": "pending", "message": "timeout waiting for auth"}))
                .into_response();
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
}

#[derive(Deserialize)]
#[allow(dead_code)]
pub struct TokenRequest {
    pub grant_type: Option<String>,
    pub code: String,
    pub code_verifier: String,
    pub redirect_uri: Option<String>,
    pub client_id: Option<String>,
}

pub async fn token_handler(
    State(state): State<AppState>,
    Form(req): Form<TokenRequest>,
) -> Response {
    if state.entra.is_none() {
        return (StatusCode::NOT_FOUND, "auth disabled").into_response();
    }

    let entry = {
        let mut codes = state.temp_codes.write().await;
        match codes.remove(&req.code) {
            Some(e) => e,
            None => {
                log::warn!("  [oauth] FAIL token exchange unknown or expired code");
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error":"invalid_grant","error_description":"unknown or expired code"})),
                )
                    .into_response();
            }
        }
    };

    let computed = URL_SAFE_NO_PAD.encode(Sha256::digest(req.code_verifier.as_bytes()));
    if computed != entry.code_challenge {
        log::warn!("  [oauth] FAIL PKCE verification failed");
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":"invalid_grant","error_description":"PKCE verification failed"})),
        )
            .into_response();
    }

    Json(json!({
        "access_token": entry.access_token,
        "token_type": "Bearer",
        "expires_in": 3600,
    }))
    .into_response()
}

// ---------------------------------------------------------------------------
// JWT validation and auth middleware
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct EntraClaims {
    oid: String,
    #[serde(alias = "upn", alias = "unique_name", default)]
    preferred_username: String,
    #[serde(default)]
    roles: Vec<String>,
}

async fn get_jwks(entra: &EntraConfig, cache: &JwksCache, kid: &str) -> Option<Vec<Value>> {
    {
        let lock = cache.read().await;
        if let Some(entry) = lock.as_ref()
            && entry.keys.iter().any(|k| k["kid"] == kid)
        {
            return Some(entry.keys.clone());
        }
    }
    let client = reqwest::Client::new();
    let resp = client.get(entra.jwks_url()).send().await.ok()?;
    let body: Value = resp.json().await.ok()?;
    let keys: Vec<Value> = body["keys"].as_array()?.clone();
    {
        let mut lock = cache.write().await;
        *lock = Some(JwksCacheEntry { keys: keys.clone() });
    }
    Some(keys)
}

pub async fn validate_jwt(
    token: &str,
    entra: &EntraConfig,
    jwks_cache: &JwksCache,
) -> Result<CallerIdentity, String> {
    let header = jsonwebtoken::decode_header(token).map_err(|e| format!("bad header: {e}"))?;
    let kid = header.kid.as_deref().unwrap_or("").to_string();

    let keys = get_jwks(entra, jwks_cache, &kid)
        .await
        .ok_or_else(|| "failed to fetch JWKS".to_string())?;

    let jwk = keys
        .iter()
        .find(|k| k["kid"] == kid)
        .ok_or_else(|| "kid not found in JWKS".to_string())?;

    let n = jwk["n"].as_str().ok_or("missing n")?;
    let e = jwk["e"].as_str().ok_or("missing e")?;
    let decoding_key =
        DecodingKey::from_rsa_components(n, e).map_err(|e| format!("bad key: {e}"))?;

    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_audience(&[&entra.client_id]);
    validation.set_issuer(&[&entra.issuer_v2(), &entra.issuer_v1()]);

    let token_data = jsonwebtoken::decode::<EntraClaims>(token, &decoding_key, &validation)
        .map_err(|e| format!("{e}"))?;

    Ok(CallerIdentity {
        oid: token_data.claims.oid,
        upn: token_data.claims.preferred_username,
        roles: token_data.claims.roles,
    })
}

pub fn unauthorized_response(public_url: &str, _client_id: &str, description: &str) -> Response {
    let resource_metadata = format!("{public_url}/.well-known/oauth-protected-resource");
    let www_authenticate = format!(
        "Bearer realm=\"{public_url}\", resource_metadata=\"{resource_metadata}\", scope=\"openid profile\""
    );
    log::debug!("  [oauth] 401 response with WWW-Authenticate: {www_authenticate}");
    (
        StatusCode::UNAUTHORIZED,
        [
            ("WWW-Authenticate", www_authenticate),
            ("Content-Type", "application/json".to_string()),
        ],
        serde_json::to_string(&json!({
            "jsonrpc": "2.0",
            "id": null,
            "error": { "code": -32001, "message": format!("Unauthorized: {description}") }
        }))
        .unwrap(),
    )
        .into_response()
}

pub async fn require_auth(State(state): State<AppState>, mut req: Request, next: Next) -> Response {
    let entra = match &state.entra {
        Some(e) => e.clone(),
        None => return next.run(req).await,
    };

    let auth_header = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    // ── No Authorization header ──────────────────────────────────────────────
    // Generate a new server-managed session, return 401 immediately with the
    // auth URL.  The LLM should open it, then retry with Bearer <session_id>.
    let token = if let Some(t) = auth_header.strip_prefix("Bearer ") {
        t.to_string()
    } else {
        let session_id = random_string(32);
        let auth_url = format!("{}/auth/start?session={}", entra.url, session_id);
        log::info!("  [oauth] new session={session_id} — returning auth URL to LLM");
        {
            let (tx, _) = watch::channel::<Option<PendingAuthResult>>(None);
            let mut pending = state.pending_sessions.write().await;
            pending.retain(|_, tx| tx.receiver_count() > 0); // prune abandoned sessions
            pending.insert(session_id.clone(), tx);
        }
        // Do NOT include WWW-Authenticate here — that header triggers the Copilot
        // CLI to launch its own parallel PKCE flow (with its own local callback
        // server), which races with our server-managed long-poll flow and causes the
        // LLM to hang.  The body already contains everything the LLM needs.
        return (
            StatusCode::UNAUTHORIZED,
            [("Content-Type", "application/json".to_string())],
            serde_json::to_string(&json!({
                "jsonrpc": "2.0",
                "id": null,
                "error": {
                    "code": -32001,
                    "message": "Authentication required. Open the auth_url in your browser, then retry with Authorization: Bearer <session_id>.",
                    "data": {
                        "auth_url": auth_url,
                        "session_id": session_id,
                    }
                }
            })).unwrap(),
        ).into_response();
    };

    // ── Bearer token is a pending session_id → long-poll ─────────────────────
    // The LLM retries immediately after receiving the 401 above; we hold this
    // connection open until the browser auth completes (up to 5 minutes).
    let maybe_rx = {
        let pending = state.pending_sessions.read().await;
        pending.get(&token).map(|tx| tx.subscribe())
    };
    if let Some(mut rx) = maybe_rx {
        log::info!("  [oauth] long-poll started for session={}", token);
        let result = tokio::time::timeout(std::time::Duration::from_secs(300), async {
            loop {
                if rx.borrow().is_some() {
                    break;
                }
                if rx.changed().await.is_err() {
                    break;
                }
            }
            rx.borrow().clone()
        })
        .await;

        // Clean up the pending entry regardless of outcome.
        state.pending_sessions.write().await.remove(&token);

        return match result {
            Ok(Some(PendingAuthResult::Success)) => {
                log::info!(
                    "  [oauth] long-poll resolved: auth success session={}",
                    token
                );
                (
                    StatusCode::OK,
                    [("Content-Type", "application/json")],
                    serde_json::to_string(&json!({
                        "jsonrpc": "2.0",
                        "id": null,
                        "result": {
                            "_mcp_auth": "complete",
                            "session_token": token,
                            "message": "Authentication successful. Retry your request with Authorization: Bearer <session_token>."
                        }
                    })).unwrap(),
                ).into_response()
            }
            Ok(Some(PendingAuthResult::Failure { error, description })) => {
                log::warn!(
                    "  [oauth] long-poll resolved: auth failure={error} session={}",
                    token
                );
                let msg = if description.is_empty() {
                    format!("Authorization failed: {error}")
                } else {
                    format!("Authorization failed: {error} — {description}")
                };
                (
                    StatusCode::OK,
                    [("Content-Type", "application/json")],
                    serde_json::to_string(&json!({
                        "jsonrpc": "2.0",
                        "id": null,
                        "error": { "code": -32001, "message": msg }
                    }))
                    .unwrap(),
                )
                    .into_response()
            }
            _ => {
                log::warn!("  [oauth] long-poll timed out for session={}", token);
                (
                    StatusCode::OK,
                    [("Content-Type", "application/json")],
                    serde_json::to_string(&json!({
                        "jsonrpc": "2.0",
                        "id": null,
                        "error": { "code": -32001, "message": "Authentication timed out. Please retry." }
                    })).unwrap(),
                ).into_response()
            }
        };
    }

    // ── Bearer token is a validated session_id → proceed ─────────────────────
    let maybe_identity = {
        let sessions = state.sessions.read().await;
        sessions.get(&token).map(|info| CallerIdentity {
            upn: info.upn.clone(),
            oid: info.oid.clone(),
            roles: info.roles.clone(),
        })
    };
    if let Some(identity) = maybe_identity {
        log::debug!(
            "  [oauth] OK session upn={} session={}",
            identity.upn,
            token
        );
        req.extensions_mut().insert(identity);
        return next.run(req).await;
    }

    // ── Bearer token is a JWT (existing PKCE flow) → validate ────────────────
    match validate_jwt(&token, &entra, &state.jwks_cache).await {
        Ok(caller) => {
            if !state.no_role_check && !caller.roles.contains(&"BillingViewer".to_string()) {
                log::warn!(
                    "  [oauth] FAIL missing_role oid={} upn={} roles={:?}",
                    caller.oid,
                    caller.upn,
                    caller.roles
                );
                return (
                    StatusCode::FORBIDDEN,
                    [("Content-Type", "application/json")],
                    serde_json::to_string(&json!({
                        "jsonrpc": "2.0",
                        "id": null,
                        "error": { "code": -32001, "message": "Forbidden: missing required App Role: BillingViewer" }
                    }))
                    .unwrap(),
                )
                    .into_response();
            }
            log::debug!(
                "  [oauth] OK jwt oid={} upn={} roles={:?}",
                caller.oid,
                caller.upn,
                caller.roles
            );
            req.extensions_mut().insert(caller);
            next.run(req).await
        }
        Err(reason) => {
            log::warn!("  [oauth] FAIL token validation: {reason}");
            unauthorized_response(
                &entra.url,
                &entra.client_id,
                &format!("invalid token: {reason}"),
            )
        }
    }
}

// ---------------------------------------------------------------------------
// Startup Entra validation
// ---------------------------------------------------------------------------

pub async fn startup_validate_entra(entra: &EntraConfig, bind_port: u16) {
    log::info!("[startup] Validating Entra configuration ...");

    if let Some(url_port) = entra
        .url
        .split(':')
        .nth(1)
        .and_then(|p| p.trim_end_matches('/').parse::<u16>().ok())
        && url_port != bind_port
    {
        log::warn!(
            "[startup] ⚠ MCP_CALLBACK_URL port ({url_port}) does not match \
                 --port ({bind_port}). The callback URL will be unreachable.\n\
                 \x20 Fix: set MCP_CALLBACK_URL=http://localhost:{bind_port} or run with --port {url_port}"
        );
    }

    let client = reqwest::Client::new();
    match client.get(entra.oidc_discovery_url()).send().await {
        Ok(r) if r.status().is_success() => {
            log::info!("[startup] ✓ Entra OIDC discovery OK (tenant_id valid)");
        }
        Ok(r) => {
            log::error!(
                "[startup] ✗ Entra OIDC discovery returned {}: check ENTRA_TENANT_ID",
                r.status()
            );
        }
        Err(e) => {
            log::error!("[startup] ✗ Entra OIDC discovery request failed: {e}");
        }
    }

    let probe = client
        .post(entra.token_url())
        .form(&[
            ("grant_type", "client_credentials"),
            ("client_id", &entra.client_id),
            ("client_secret", &entra.client_secret),
            ("scope", &format!("api://{}/.default", entra.client_id)),
        ])
        .send()
        .await;

    match probe {
        Ok(r) => {
            let body: Value = r.json().await.unwrap_or_default();
            if body["error"].is_null() {
                log::info!(
                    "[startup] ✓ Entra client credentials probe OK (client_id/secret valid)"
                );
            } else {
                let err = body["error"].as_str().unwrap_or("?");
                let desc = body["error_description"].as_str().unwrap_or("");
                log::error!("[startup] ✗ Client credentials probe error={err}: {desc}");
                log::info!("[startup]   → check ENTRA_CLIENT_ID and ENTRA_CLIENT_SECRET");
            }
        }
        Err(e) => {
            log::error!("[startup] ✗ Client credentials probe request failed: {e}");
        }
    }

    log::info!(
        "[startup] ℹ Callback URL (must be registered in app registration): {}",
        entra.redirect_uri()
    );
    log::info!(
        "[startup] ℹ MCP OAuth metadata base URL (must be reachable by MCP clients): {}",
        entra.url
    );
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Render an HTML error page shown in the browser when Entra returns an error
/// during the OAuth callback.  If `callback_url` is Some, the page uses
/// `fetch()` to attempt notifying the MCP client's local callback server, then
/// redirects.  If the client's local server is already gone the page stays
/// visible so the user sees a clear message instead of ERR_CONNECTION_REFUSED.
fn build_auth_error_page(error: &str, description: &str, callback_url: Option<&str>) -> String {
    let notify_script = match callback_url {
        Some(url) => format!(
            r#"
    fetch({url:?})
      .then(() => {{ window.location.replace({url:?}); }})
      .catch(() => {{
        document.getElementById('status').textContent =
          'The authorisation client could not be reached. You can close this tab and check your CLI for the error.';
      }});
"#,
            url = url
        ),
        None => String::new(),
    };

    let desc_line = if description.is_empty() {
        String::new()
    } else {
        format!("<p class=\"desc\">{}</p>", html_escape(description))
    };

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>Authorisation failed</title>
  <style>
    body {{ font-family: system-ui, sans-serif; max-width: 480px; margin: 10vh auto; padding: 0 1rem; color: #222; }}
    h1   {{ color: #c0392b; font-size: 1.4rem; }}
    .code {{ font-family: monospace; background: #f4f4f4; padding: 2px 6px; border-radius: 3px; }}
    .desc {{ color: #555; font-size: .9rem; }}
    #status {{ margin-top: 1rem; color: #555; font-size: .9rem; }}
  </style>
</head>
<body>
  <h1>Authorisation failed</h1>
  <p>Entra returned error: <span class="code">{error}</span></p>
  {desc_line}
  <p id="status">Notifying your CLI&hellip;</p>
  <script>{notify_script}</script>
</body>
</html>"#,
        error = html_escape(error),
        desc_line = desc_line,
        notify_script = notify_script,
    )
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Parse `host` and `port` from a URL like `http://localhost:8091`.
/// Maps `localhost` → `127.0.0.1` for binding. Returns `None` if unparseable.
pub fn parse_bind_addr_from_url(url: &str) -> Option<(String, u16)> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let authority = rest.split('/').next()?;
    let (host, port_str) = authority.rsplit_once(':')?;
    let port: u16 = port_str.parse().ok()?;
    let host = if host == "localhost" {
        "127.0.0.1"
    } else {
        host
    }
    .to_string();
    Some((host, port))
}

fn random_string(len: usize) -> String {
    let mut rng = rand::thread_rng();
    (0..len)
        .map(|_| char::from(rng.gen_range(b'a'..=b'z')))
        .collect()
}

/// Percent-encode non-unreserved URI characters.
fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- parse_bind_addr_from_url ---

    #[test]
    fn parse_bind_addr_localhost() {
        assert_eq!(
            parse_bind_addr_from_url("http://localhost:3000"),
            Some(("127.0.0.1".to_string(), 3000))
        );
    }

    #[test]
    fn parse_bind_addr_custom_host() {
        assert_eq!(
            parse_bind_addr_from_url("https://example.com:8443/mcp"),
            Some(("example.com".to_string(), 8443))
        );
    }

    #[test]
    fn parse_bind_addr_no_port() {
        assert_eq!(parse_bind_addr_from_url("http://localhost"), None);
    }

    // --- validate_jwt rejects garbage ---

    #[tokio::test]
    async fn validate_jwt_rejects_garbage_token() {
        let entra = EntraConfig {
            tenant_id: "test-tenant".to_string(),
            client_id: "test-client".to_string(),
            client_secret: "secret".to_string(),
            url: "http://localhost:3000".to_string(),
        };
        let jwks_cache: JwksCache = Arc::new(RwLock::new(None));
        let result = validate_jwt("not.a.valid.jwt.token", &entra, &jwks_cache).await;
        assert!(result.is_err(), "garbage token should be rejected");
        let err = result.unwrap_err();
        assert!(!err.is_empty(), "error message should not be empty");
    }

    #[tokio::test]
    async fn validate_jwt_rejects_empty_string() {
        let entra = EntraConfig {
            tenant_id: "t".to_string(),
            client_id: "c".to_string(),
            client_secret: "s".to_string(),
            url: "http://localhost:3000".to_string(),
        };
        let cache: JwksCache = Arc::new(RwLock::new(None));
        assert!(validate_jwt("", &entra, &cache).await.is_err());
    }
}
