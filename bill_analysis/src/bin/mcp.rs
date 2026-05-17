//! bill_analysis_mcp — MCP server exposing Azure billing data via Streamable HTTP.
//!
//! Implements the 2025 MCP spec (Streamable HTTP transport):
//!   POST /mcp  — JSON-RPC 2.0 requests (requires BillingViewer App Role unless --no-auth)
//!   GET  /mcp  — Health check
//!
//! OAuth 2.0 Authorization Code + PKCE proxy against Microsoft Entra ID:
//!   GET  /.well-known/oauth-authorization-server  — OAuth metadata discovery
//!   GET  /authorize                               — Redirect browser to Entra
//!   GET  /callback                                — Receive Entra code, issue server code
//!   POST /token                                   — Exchange server code for Entra access token
//!
//! Usage: bill_analysis_mcp --data-dir <path> [--port <port>] [--host <host>] [--no-auth]
//! Config: ENTRA_TENANT_ID, ENTRA_CLIENT_ID, ENTRA_CLIENT_SECRET,
//!         MCP_PUBLIC_URL, MCP_CALLBACK_URL (optional; .env or env)

use axum::{
    Router,
    extract::{Query, Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
    Form, Json,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use bill_analysis::{bills::Bills, cmd_parse::FilterOpts, find_files};
use clap::Parser;
use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Instant};
use tokio::sync::RwLock;

// ---------------------------------------------------------------------------
// CLI args
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(name = "bill_analysis_mcp", about = "MCP server for Azure billing cost data")]
struct Args {
    /// Directory containing billing CSV subfolders
    #[arg(long)]
    data_dir: PathBuf,

    /// TCP port to listen on. Defaults to the port in MCP_PUBLIC_URL, or 3000.
    #[arg(long)]
    port: Option<u16>,

    /// Host address to bind to. Defaults to the host in MCP_PUBLIC_URL, or 127.0.0.1.
    #[arg(long)]
    host: Option<String>,

    /// Disable authentication (skip Entra config; all callers are trusted).
    /// Without this flag the server refuses to start if Entra env vars are missing.
    #[arg(long)]
    no_auth: bool,
}

// ---------------------------------------------------------------------------
// Shared state
// ---------------------------------------------------------------------------

type BillCache = Arc<RwLock<HashMap<(u32, u32), Arc<Bills>>>>;

// ---------------------------------------------------------------------------
// Entra OAuth configuration
// ---------------------------------------------------------------------------

/// Entra OAuth 2.0 configuration loaded from .env / environment variables.
#[derive(Clone)]
struct EntraConfig {
    tenant_id: String,
    client_id: String,
    client_secret: String,
    /// Base URL used for both public and callback purposes.
    url: String,
}

impl EntraConfig {
    fn redirect_uri(&self) -> String {
        format!("{}/callback", self.url)
    }
    fn authorize_url(&self) -> String {
        format!("https://login.microsoftonline.com/{}/oauth2/v2.0/authorize", self.tenant_id)
    }
    fn token_url(&self) -> String {
        format!("https://login.microsoftonline.com/{}/oauth2/v2.0/token", self.tenant_id)
    }
    fn jwks_url(&self) -> String {
        format!("https://login.microsoftonline.com/{}/discovery/v2.0/keys", self.tenant_id)
    }
    fn issuer(&self) -> String {
        format!("https://login.microsoftonline.com/{}/v2.0", self.tenant_id)
    }
    fn oidc_discovery_url(&self) -> String {
        format!(
            "https://login.microsoftonline.com/{}/.well-known/openid-configuration",
            self.tenant_id
        )
    }
}

fn load_entra_config() -> Option<EntraConfig> {
    let tenant_id    = std::env::var("ENTRA_TENANT_ID").ok().filter(|s| !s.is_empty())?;
    let client_id    = std::env::var("ENTRA_CLIENT_ID").ok().filter(|s| !s.is_empty())?;
    let client_secret = std::env::var("ENTRA_CLIENT_SECRET").ok().filter(|s| !s.is_empty())?;
    let url          = std::env::var("MCP_URL").ok().filter(|s| !s.is_empty())?; // Single URL

    Some(EntraConfig { tenant_id, client_id, client_secret, url })
}

// ---------------------------------------------------------------------------
// OAuth flow state types
// ---------------------------------------------------------------------------

/// Per-authorize-request state, keyed by the server-generated OAuth `state` parameter.
/// Stored between GET /authorize and GET /callback, pruned after 10 minutes.
struct PkceFlowState {
    code_challenge: String,
    code_challenge_method: String,
    /// The MCP client's redirect_uri — where we redirect after a successful callback.
    client_redirect_uri: String,
    /// The client's original `state` value, echoed back in the redirect.
    client_state: String,
    created_at: Instant,
}

/// Short-lived entry mapping a server-issued authorization code to an Entra access token.
/// Consumed once in POST /token; pruned after 5 minutes.
#[allow(dead_code)]
struct TempCodeEntry {
    access_token: String,
    code_challenge: String,
    code_challenge_method: String,
    created_at: Instant,
}

/// Cached Entra JSON Web Key Set, refreshed on key-not-found.
struct JwksCacheEntry {
    keys: Vec<Value>,
}

type PkceStateStore = Arc<RwLock<HashMap<String, PkceFlowState>>>;
type TempCodeStore  = Arc<RwLock<HashMap<String, TempCodeEntry>>>;
type JwksCache      = Arc<RwLock<Option<JwksCacheEntry>>>;

// ---------------------------------------------------------------------------
// Authenticated caller identity
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct CallerIdentity {
    oid: String,
    upn: String,
    roles: Vec<String>,
}

// ---------------------------------------------------------------------------
// Shared state
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct AppState {
    cache: BillCache,
    data_dir: PathBuf,
    /// None when --no-auth is set; all MCP callers are trusted.
    entra: Option<EntraConfig>,
    pkce_store: PkceStateStore,
    temp_codes: TempCodeStore,
    jwks_cache: JwksCache,
}

// ---------------------------------------------------------------------------
// JSON-RPC 2.0 types
// ---------------------------------------------------------------------------

#[derive(Deserialize, Debug)]
struct RpcRequest {
    #[allow(dead_code)]
    jsonrpc: String,
    method: String,
    params: Option<Value>,
    id: Option<Value>,
}

#[derive(Serialize)]
struct RpcResponse {
    jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<RpcError>,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<Value>,
}

#[derive(Serialize)]
struct RpcError {
    code: i32,
    message: String,
}

impl RpcResponse {
    fn ok(id: Option<Value>, result: Value) -> Self {
        Self { jsonrpc: "2.0", result: Some(result), error: None, id }
    }
    fn err(id: Option<Value>, code: i32, message: String) -> Self {
        Self { jsonrpc: "2.0", result: None, error: Some(RpcError { code, message }), id }
    }
}

// ---------------------------------------------------------------------------
// GET /mcp — health / liveness probe
// ---------------------------------------------------------------------------

async fn mcp_get_handler(State(state): State<AppState>) -> impl IntoResponse {
    let auth = if state.entra.is_some() { "entra" } else { "disabled" };
    Json(json!({ "status": "ok", "auth": auth }))
}

// ---------------------------------------------------------------------------
// HTTP request logging middleware
// ---------------------------------------------------------------------------

async fn log_request(req: Request, next: Next) -> Response {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let is_probe = method == axum::http::Method::GET && uri.path() == "/mcp";
    let start = Instant::now();
    let resp = next.run(req).await;
    if !is_probe {
        eprintln!(
            "[http] {} {} → {} in {:.1}ms",
            method,
            uri,
            resp.status(),
            start.elapsed().as_secs_f64() * 1000.0
        );
    }
    resp
}

// ---------------------------------------------------------------------------
// OAuth 2.0 — discovery, authorize, callback, token
// ---------------------------------------------------------------------------

async fn oauth_metadata_handler(State(state): State<AppState>) -> impl IntoResponse {
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
    });
    eprintln!("[oauth] authorization-server metadata: {body}");
    Json(body).into_response()
}

/// RFC 9728 — OAuth 2.0 Protected Resource Metadata.
/// Served at `/.well-known/oauth-protected-resource` (and the `/mcp` path variant).
/// Tells OAuth clients which authorization server protects this resource so they
/// know where to fetch AS metadata (`/.well-known/oauth-authorization-server`).
async fn oauth_protected_resource_handler(State(state): State<AppState>) -> impl IntoResponse {
    let entra = match &state.entra {
        Some(e) => e,
        None => return (StatusCode::NOT_FOUND, "auth disabled").into_response(),
    };
    let base = &entra.url;
    let body = json!({
        "resource": format!("{}/mcp", base),
        "authorization_servers": [base],
    });
    eprintln!("[oauth] protected-resource: {body}");
    Json(body).into_response()
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct AuthorizeParams {
    response_type: Option<String>,
    client_id: Option<String>,
    redirect_uri: String,
    state: Option<String>,
    code_challenge: String,
    code_challenge_method: Option<String>,
    scope: Option<String>,
}

async fn authorize_handler(
    State(state): State<AppState>,
    Query(params): Query<AuthorizeParams>,
) -> Response {
    eprintln!("[oauth] /authorize called with client_id={:?}, redirect_uri={}", params.client_id, params.redirect_uri);
    
    let entra = match &state.entra {
        Some(e) => e.clone(),
        None => return (StatusCode::NOT_FOUND, "auth disabled").into_response(),
    };

    let method = params.code_challenge_method.as_deref().unwrap_or("S256");
    if method != "S256" {
        eprintln!("[oauth] FAIL /authorize unsupported code_challenge_method={}", method);
        return (StatusCode::BAD_REQUEST, "only S256 code_challenge_method supported")
            .into_response();
    }

    let server_state = random_string(32);
    let client_state = params.state.unwrap_or_default();
    let scope = params.scope.as_deref().unwrap_or("openid profile email");

    {
        let mut store = state.pkce_store.write().await;
        // Prune stale entries (>10 min)
        store.retain(|_, v| v.created_at.elapsed().as_secs() < 600);
        store.insert(
            server_state.clone(),
            PkceFlowState {
                code_challenge: params.code_challenge,
                code_challenge_method: method.to_string(),
                client_redirect_uri: params.redirect_uri,
                client_state,
                created_at: Instant::now(),
            },
        );
    }

    let target = format!(
        "{}?response_type=code&client_id={}&redirect_uri={}&state={}&scope={}&prompt=select_account",
        entra.authorize_url(),
        url_encode(&entra.client_id),
        url_encode(&entra.redirect_uri()),
        url_encode(&server_state),
        url_encode(scope),
    );
    eprintln!("[oauth] /authorize redirecting to Entra: {}", target);
    Redirect::temporary(&target).into_response()
}

#[derive(Deserialize)]
struct CallbackParams {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

async fn callback_handler(
    State(state): State<AppState>,
    Query(params): Query<CallbackParams>,
) -> Response {
    let entra = match &state.entra {
        Some(e) => e.clone(),
        None => return (StatusCode::NOT_FOUND, "auth disabled").into_response(),
    };

    if let Some(err) = &params.error {
        let desc = params.error_description.as_deref().unwrap_or("");
        eprintln!("[auth] Entra authorize error={err} description={desc}");
        return (StatusCode::BAD_GATEWAY, format!("Entra error: {err}")).into_response();
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
                eprintln!("[auth] FAIL callback unknown or expired state={server_state}");
                return (StatusCode::BAD_REQUEST, "unknown or expired state").into_response();
            }
        }
    };

    // Exchange code with Entra (confidential client, no PKCE on Entra side)
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
                eprintln!("[auth] FAIL token exchange parse error: {e}");
                return (StatusCode::BAD_GATEWAY, "token exchange failed").into_response();
            }
        },
        Err(e) => {
            eprintln!("[auth] FAIL token exchange request error: {e}");
            return (StatusCode::BAD_GATEWAY, "token exchange failed").into_response();
        }
    };

    let access_token = match token_json["access_token"].as_str() {
        Some(t) => t.to_string(),
        None => {
            let err = token_json["error"].as_str().unwrap_or("unknown");
            let desc = token_json["error_description"].as_str().unwrap_or("");
            eprintln!("[auth] FAIL token exchange entra_error={err} desc={desc}");
            return (StatusCode::BAD_GATEWAY, "token exchange failed").into_response();
        }
    };

    // Issue a short-lived server code
    let server_code = random_string(40);
    {
        let mut codes = state.temp_codes.write().await;
        // Prune stale entries (>5 min)
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
    Redirect::temporary(&redirect).into_response()
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct TokenRequest {
    grant_type: Option<String>,
    code: String,
    code_verifier: String,
    redirect_uri: Option<String>,
    client_id: Option<String>,
}

async fn token_handler(
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
                eprintln!("[auth] FAIL token exchange unknown or expired code");
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error":"invalid_grant","error_description":"unknown or expired code"})),
                )
                    .into_response();
            }
        }
    };

    // Validate PKCE: SHA256(code_verifier) == code_challenge
    let computed = URL_SAFE_NO_PAD.encode(Sha256::digest(req.code_verifier.as_bytes()));
    if computed != entry.code_challenge {
        eprintln!("[auth] FAIL PKCE verification failed");
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
    /// Entra v2 access tokens use `preferred_username`; v1 tokens may use `upn`.
    #[serde(alias = "upn", alias = "unique_name", default)]
    preferred_username: String,
    #[serde(default)]
    roles: Vec<String>,
}

async fn get_jwks(entra: &EntraConfig, cache: &JwksCache, kid: &str) -> Option<Vec<Value>> {
    {
        let lock = cache.read().await;
        if let Some(entry) = lock.as_ref()
            && entry.keys.iter().any(|k| k["kid"] == kid) {
                return Some(entry.keys.clone());
            }
    }
    // Refresh
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

async fn validate_jwt(
    token: &str,
    entra: &EntraConfig,
    jwks_cache: &JwksCache,
) -> Result<CallerIdentity, String> {
    // Decode header to get kid
    let header = jsonwebtoken::decode_header(token).map_err(|e| format!("bad header: {e}"))?;
    let kid = header.kid.as_deref().unwrap_or("").to_string();

    let keys = get_jwks(entra, jwks_cache, &kid)
        .await
        .ok_or_else(|| "failed to fetch JWKS".to_string())?;

    let jwk = keys.iter().find(|k| k["kid"] == kid).ok_or_else(|| "kid not found in JWKS".to_string())?;

    let n = jwk["n"].as_str().ok_or("missing n")?;
    let e = jwk["e"].as_str().ok_or("missing e")?;
    let decoding_key = DecodingKey::from_rsa_components(n, e).map_err(|e| format!("bad key: {e}"))?;

    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_audience(&[&entra.client_id]);
    validation.set_issuer(&[&entra.issuer()]);

    let token_data = jsonwebtoken::decode::<EntraClaims>(token, &decoding_key, &validation)
        .map_err(|e| format!("{e}"))?;

    Ok(CallerIdentity {
        oid: token_data.claims.oid,
        upn: token_data.claims.preferred_username,
        roles: token_data.claims.roles,
    })
}

fn unauthorized_response(public_url: &str, description: &str) -> Response {
    let resource_metadata =
        format!("{public_url}/.well-known/oauth-protected-resource");
    (
        StatusCode::UNAUTHORIZED,
        [
            (
                "WWW-Authenticate",
                format!(
                    "Bearer realm=\"{public_url}\", \
                     resource_metadata=\"{resource_metadata}\""
                ),
            ),
            ("Content-Type", "text/plain".to_string()),
        ],
        description.to_string(),
    )
        .into_response()
}

async fn require_auth(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Response {
    let entra = match &state.entra {
        Some(e) => e.clone(),
        None => return next.run(req).await, // --no-auth
    };

    let auth_header = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let token = if let Some(t) = auth_header.strip_prefix("Bearer ") {
        t.to_string()
    } else {
        eprintln!("[auth] FAIL missing Authorization header");
        return unauthorized_response(&entra.url, "missing Authorization header");
    };

    match validate_jwt(&token, &entra, &state.jwks_cache).await {
        Ok(caller) => {
            if !caller.roles.contains(&"BillingViewer".to_string()) {
                eprintln!(
                    "[auth] FAIL missing_role oid={} upn={} roles={:?}",
                    caller.oid, caller.upn, caller.roles
                );
                return (
                    StatusCode::FORBIDDEN,
                    "missing required App Role: BillingViewer",
                )
                    .into_response();
            }
            eprintln!(
                "[auth] OK oid={} upn={} roles={:?}",
                caller.oid, caller.upn, caller.roles
            );
            next.run(req).await
        }
        Err(reason) => {
            eprintln!("[auth] FAIL token validation: {reason}");
            unauthorized_response(&entra.url, &format!("invalid token: {reason}"))
        }
    }
}

async fn startup_validate_entra(entra: &EntraConfig, bind_port: u16) {
    eprintln!("[startup] Validating Entra configuration ...");

    // Warn if callback URL port doesn't match the actual bind port.
    if let Some(url_port) = entra.url.split(':').nth(1)
        .and_then(|p| p.trim_end_matches('/').parse::<u16>().ok())
        && url_port != bind_port {
            eprintln!(
                "[startup] ⚠ WARNING: MCP_CALLBACK_URL port ({url_port}) does not match \
                 --port ({bind_port}). The callback URL will be unreachable.\n\
                 \x20 Fix: set MCP_CALLBACK_URL=http://localhost:{bind_port} or run with --port {url_port}"
            );
        }

    // 1. OIDC discovery — proves tenant_id is valid, warms JWKS metadata
    let client = reqwest::Client::new();
    match client.get(entra.oidc_discovery_url()).send().await {
        Ok(r) if r.status().is_success() => {
            eprintln!("[startup] ✓ Entra OIDC discovery OK (tenant_id valid)");
        }
        Ok(r) => {
            eprintln!("[startup] ✗ Entra OIDC discovery returned {}: check ENTRA_TENANT_ID", r.status());
        }
        Err(e) => {
            eprintln!("[startup] ✗ Entra OIDC discovery request failed: {e}");
        }
    }

    // 2. Client credentials probe — validates client_id + client_secret
    let probe = client
        .post(entra.token_url())
        .form(&[
            ("grant_type", "client_credentials"),
            ("client_id", &entra.client_id),
            ("client_secret", &entra.client_secret),
            ("scope", &format!("{}/.default", entra.client_id)),
        ])
        .send()
        .await;

    match probe {
        Ok(r) => {
            let body: Value = r.json().await.unwrap_or_default();
            if body["error"].is_null() {
                eprintln!("[startup] ✓ Entra client credentials probe OK (client_id/secret valid)");
            } else {
                let err = body["error"].as_str().unwrap_or("?");
                let desc = body["error_description"].as_str().unwrap_or("");
                eprintln!("[startup] ✗ Client credentials probe error={err}: {desc}");
                eprintln!("[startup]   → check ENTRA_CLIENT_ID and ENTRA_CLIENT_SECRET");
            }
        }
        Err(e) => {
            eprintln!("[startup] ✗ Client credentials probe request failed: {e}");
        }
    }

    // 3. Redirect URI — advisory only (can't validate without Graph API perms)
    eprintln!(
        "[startup] ℹ Callback URL (must be registered in app registration): {}",
        entra.redirect_uri()
    );
    eprintln!(
        "[startup] ℹ MCP OAuth metadata base URL (must be reachable by MCP clients): {}",
        entra.url
    );
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse `host` and `port` from a URL like `http://localhost:8091`.
/// Maps `localhost` → `127.0.0.1` for binding. Returns `None` if unparseable.
fn parse_bind_addr_from_url(url: &str) -> Option<(String, u16)> {
    // Strip scheme
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    // Take only the authority (before any path)
    let authority = rest.split('/').next()?;
    // Split host:port
    let (host, port_str) = authority.rsplit_once(':')?;
    let port: u16 = port_str.parse().ok()?;
    let host = if host == "localhost" { "127.0.0.1" } else { host }.to_string();
    Some((host, port))
}

fn random_string(len: usize) -> String {
    let mut rng = rand::thread_rng();
    (0..len).map(|_| char::from(rng.gen_range(b'a'..=b'z'))).collect()
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
// MCP request handler
// ---------------------------------------------------------------------------

async fn mcp_handler(State(state): State<AppState>, Json(req): Json<RpcRequest>) -> Response {
    let start = Instant::now();
    let method = req.method.as_str();
    if let Some(params) = &req.params {
        eprintln!("[mcp] → {method} {}", params);
    } else {
        eprintln!("[mcp] → {method}");
    }

    // Notifications have no `id` — acknowledge but send no body.
    if req.id.is_none() {
        eprintln!("[mcp] ← notification in {:.1}ms", start.elapsed().as_secs_f64() * 1000.0);
        return axum::http::StatusCode::ACCEPTED.into_response();
    }

    let resp = match method {
        "initialize" => handle_initialize(&req),
        "ping" => RpcResponse::ok(req.id.clone(), json!({})),
        "tools/list" => handle_tools_list(&req),
        "tools/call" => handle_tools_call(&req, &state).await,
        _ => RpcResponse::err(req.id.clone(), -32601, format!("Method not found: {method}")),
    };

    eprintln!("[mcp] ← {method} in {:.1}ms", start.elapsed().as_secs_f64() * 1000.0);
    Json(resp).into_response()
}

// ---------------------------------------------------------------------------
// initialize
// ---------------------------------------------------------------------------

fn handle_initialize(req: &RpcRequest) -> RpcResponse {
    // Echo the client's requested protocol version so we stay compatible as
    // the MCP spec evolves, falling back to the last known version.
    let protocol_version = req
        .params
        .as_ref()
        .and_then(|p| p.get("protocolVersion"))
        .and_then(|v| v.as_str())
        .unwrap_or("2025-11-25");

    RpcResponse::ok(
        req.id.clone(),
        json!({
            "protocolVersion": protocol_version,
            "capabilities": { "tools": {} },
            "serverInfo": {
                "name": "bill_analysis_mcp",
                "version": env!("CARGO_PKG_VERSION")
            }
        }),
    )
}

// ---------------------------------------------------------------------------
// tools/list
// ---------------------------------------------------------------------------

fn handle_tools_list(req: &RpcRequest) -> RpcResponse {
    RpcResponse::ok(
        req.id.clone(),
        json!({
            "tools": [
                {
                    "name": "list_available_months",
                    "description": "List all billing months available in the data directory. Returns an array of YYYY-MM strings.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {}
                    }
                },
                {
                    "name": "get_monthly_cost",
                    "description": "Get the total Azure cost in USD for a given billing month. Optionally filter by resource group or resource name using a case-insensitive substring match — e.g. resource_group='prod' will match 'my-prod-eastus-rg'. Returns the total cost, row count, and top contributors.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "month": {
                                "type": "string",
                                "description": "Billing month in YYYY-MM format, e.g. '2026-04'."
                            },
                            "resource_group": {
                                "type": "string",
                                "description": "Case-insensitive substring to match against resource group names. Omit to include all resource groups."
                            },
                            "resource_name": {
                                "type": "string",
                                "description": "Case-insensitive substring to match against resource names. Omit to include all resources."
                            }
                        },
                        "required": ["month"]
                    }
                },
                {
                    "name": "get_daily_cost",
                    "description": "Get the total Azure cost in USD for a specific calendar date. The billing CSV uses UTC calendar dates. Optionally filter by resource group or resource name (case-insensitive substring match).",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "date": {
                                "type": "string",
                                "description": "Calendar date in YYYY-MM-DD format, e.g. '2026-04-07'."
                            },
                            "resource_group": {
                                "type": "string",
                                "description": "Case-insensitive substring to match against resource group names."
                            },
                            "resource_name": {
                                "type": "string",
                                "description": "Case-insensitive substring to match against resource names."
                            }
                        },
                        "required": ["date"]
                    }
                }
            ]
        }),
    )
}

// ---------------------------------------------------------------------------
// tools/call dispatch
// ---------------------------------------------------------------------------

async fn handle_tools_call(req: &RpcRequest, state: &AppState) -> RpcResponse {
    let params = match req.params.as_ref().and_then(|p| p.as_object()) {
        Some(p) => p,
        None => return RpcResponse::err(req.id.clone(), -32602, "Missing params".into()),
    };
    let tool_name = match params.get("name").and_then(|v| v.as_str()) {
        Some(n) => n,
        None => return RpcResponse::err(req.id.clone(), -32602, "Missing tool name".into()),
    };
    let args = params.get("arguments").and_then(|v| v.as_object());

    let result = match tool_name {
        "list_available_months" => tool_list_available_months(state).await,
        "get_monthly_cost" => tool_get_monthly_cost(args, state).await,
        "get_daily_cost" => tool_get_daily_cost(args, state).await,
        _ => Err(format!("Unknown tool: {tool_name}")),
    };

    match result {
        Ok(text) => RpcResponse::ok(
            req.id.clone(),
            json!({ "content": [{ "type": "text", "text": text }] }),
        ),
        Err(e) => RpcResponse::err(req.id.clone(), -32000, e),
    }
}

// ---------------------------------------------------------------------------
// Tool: list_available_months
// ---------------------------------------------------------------------------

async fn tool_list_available_months(state: &AppState) -> Result<String, String> {
    let months = find_files::list_bill_months(&state.data_dir);
    Ok(serde_json::to_string_pretty(&json!({ "months": months })).unwrap())
}

// ---------------------------------------------------------------------------
// Tool: get_monthly_cost
// ---------------------------------------------------------------------------

async fn tool_get_monthly_cost(
    args: Option<&serde_json::Map<String, Value>>,
    state: &AppState,
) -> Result<String, String> {
    let args = args.ok_or_else(|| "Missing arguments".to_string())?;
    let month = args
        .get("month")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Missing required argument 'month'".to_string())?;
    let rg_filter = args.get("resource_group").and_then(|v| v.as_str()).unwrap_or("");
    let name_filter = args.get("resource_name").and_then(|v| v.as_str()).unwrap_or("");

    let (year, mon) = parse_year_month(month)?;
    let bills = load_or_cache(state, year, mon, month).await?;
    let result = compute_cost(&bills, rg_filter, name_filter, None);

    Ok(serde_json::to_string_pretty(&json!({
        "cost_usd": round2(result.cost_usd),
        "row_count": result.row_count,
        "period": month,
        "matched_resources": result.matched_resources,
    }))
    .unwrap())
}

// ---------------------------------------------------------------------------
// Tool: get_daily_cost
// ---------------------------------------------------------------------------

async fn tool_get_daily_cost(
    args: Option<&serde_json::Map<String, Value>>,
    state: &AppState,
) -> Result<String, String> {
    let args = args.ok_or_else(|| "Missing arguments".to_string())?;
    let date_str = args
        .get("date")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Missing required argument 'date'".to_string())?;
    let rg_filter = args.get("resource_group").and_then(|v| v.as_str()).unwrap_or("");
    let name_filter = args.get("resource_name").and_then(|v| v.as_str()).unwrap_or("");

    let (year, mon, _day) = parse_date(date_str)?;
    let month_str = format!("{:04}-{:02}", year, mon);

    let bills = load_or_cache(state, year, mon, &month_str).await?;
    let result = compute_cost(&bills, rg_filter, name_filter, Some(date_str));

    Ok(serde_json::to_string_pretty(&json!({
        "cost_usd": round2(result.cost_usd),
        "row_count": result.row_count,
        "date": date_str,
        "matched_resources": result.matched_resources,
    }))
    .unwrap())
}

// ---------------------------------------------------------------------------
// Cost computation — iterates bill entries directly for performance
// ---------------------------------------------------------------------------

struct CostResult {
    cost_usd: f64,
    row_count: usize,
    matched_resources: Vec<ResourceEntry>,
}

#[derive(Serialize)]
struct ResourceEntry {
    name: String,
    cost_usd: f64,
    row_count: usize,
}

/// Compute total USD cost across all matching bill entries.
///
/// Filters are case-insensitive substring matches. All strings were lowercased
/// on ingest (bills are always parsed with `case_sensitive = false`), so we
/// just lowercase the filter inputs and use `contains`.
///
/// `date_filter` — when `Some`, only entries whose `date` field equals the
/// ISO date string (`YYYY-MM-DD`) are included.
///
/// When `name_filter` is set, `matched_resources` lists individual resources
/// (ResourceName); otherwise it lists resource groups (ResourceGroup), top-10
/// by cost.
fn compute_cost(
    bills: &Bills,
    rg_filter: &str,
    name_filter: &str,
    date_filter: Option<&str>,
) -> CostResult {
    let t = Instant::now();
    let rg_lower = rg_filter.to_lowercase();
    let name_lower = name_filter.to_lowercase();
    let group_by_name = !name_lower.is_empty();

    let mut total_usd = 0.0f64;
    let mut row_count = 0usize;
    let mut by_key: HashMap<String, (f64, usize)> = HashMap::new();

    for entry in &bills.bills {
        if let Some(date) = date_filter
            && entry.date != date {
                continue;
            }
        if !rg_lower.is_empty() && !entry.resource_group.contains(rg_lower.as_str()) {
            continue;
        }
        if !name_lower.is_empty() && !entry.resource_name.contains(name_lower.as_str()) {
            continue;
        }

        let cost = entry.cost_usd.0;
        total_usd += cost;
        row_count += 1;

        let key = if group_by_name {
            entry.resource_name.clone()
        } else {
            entry.resource_group.clone()
        };
        let e = by_key.entry(key).or_insert((0.0, 0));
        e.0 += cost;
        e.1 += 1;
    }

    // Sort by cost descending, keep top 10
    let mut entries: Vec<(String, f64, usize)> =
        by_key.into_iter().map(|(k, (c, n))| (k, c, n)).collect();
    entries.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    entries.truncate(10);

    let matched_resources = entries
        .into_iter()
        .map(|(name, cost_usd, row_count)| ResourceEntry {
            name,
            cost_usd: round2(cost_usd),
            row_count,
        })
        .collect();

    eprintln!(
        "[mcp] compute_cost {} rows in {:.1}ms",
        row_count,
        t.elapsed().as_secs_f64() * 1000.0
    );
    CostResult { cost_usd: total_usd, row_count, matched_resources }
}

// ---------------------------------------------------------------------------
// Cache helpers
// ---------------------------------------------------------------------------

fn parse_year_month(s: &str) -> Result<(u32, u32), String> {
    let mut parts = s.splitn(2, '-');
    let year: u32 = parts
        .next()
        .and_then(|p| p.parse().ok())
        .ok_or_else(|| format!("Invalid month format '{}', expected YYYY-MM", s))?;
    let mon: u32 = parts
        .next()
        .and_then(|p| p.parse().ok())
        .ok_or_else(|| format!("Invalid month format '{}', expected YYYY-MM", s))?;
    Ok((year, mon))
}

fn parse_date(s: &str) -> Result<(u32, u32, u32), String> {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 3 {
        return Err(format!("Invalid date format '{}', expected YYYY-MM-DD", s));
    }
    let year: u32 =
        parts[0].parse().map_err(|_| format!("Invalid year in '{}'", s))?;
    let mon: u32 =
        parts[1].parse().map_err(|_| format!("Invalid month in '{}'", s))?;
    let day: u32 =
        parts[2].parse().map_err(|_| format!("Invalid day in '{}'", s))?;
    Ok((year, mon, day))
}

async fn load_or_cache(
    state: &AppState,
    year: u32,
    mon: u32,
    month_str: &str,
) -> Result<Arc<Bills>, String> {
    // Fast path: read lock
    {
        let cache = state.cache.read().await;
        if let Some(bills) = cache.get(&(year, mon)) {
            eprintln!("[mcp] cache HIT  {month_str}");
            return Ok(Arc::clone(bills));
        }
    }

    // Cache miss: locate and parse the CSV
    eprintln!("[mcp] cache MISS {month_str} — loading...");
    let csv_path = find_files::find_bill_csv(&state.data_dir, month_str)
        .ok_or_else(|| {
            format!(
                "No billing file found for '{}' in {:?}",
                month_str, state.data_dir
            )
        })?;

    let load_start = Instant::now();
    let mut bills = Bills::default();
    let filter_opts = FilterOpts { case_sensitive: false };
    bills
        .parse_csv(&csv_path, &filter_opts)
        .map_err(|e| format!("Failed to parse '{:?}': {}", csv_path, e))?;
    eprintln!(
        "[mcp] loaded {month_str} ({} rows) in {:.3}s",
        bills.len(),
        load_start.elapsed().as_secs_f64()
    );

    let bills = Arc::new(bills);
    {
        let mut cache = state.cache.write().await;
        cache.insert((year, mon), Arc::clone(&bills));
    }
    Ok(bills)
}

// ---------------------------------------------------------------------------
// Utilities
// ---------------------------------------------------------------------------

/// Round to 2 decimal places for JSON output.
fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use bill_analysis::bills::bill_entry::BillEntry;
    use bill_analysis::money::Usd;

    fn make_entry(resource_group: &str, resource_name: &str, cost: f64, date: &str) -> BillEntry {
        BillEntry {
            resource_group: resource_group.to_string(),
            resource_name: resource_name.to_string(),
            cost_usd: Usd(cost),
            date: date.to_string(), // ISO YYYY-MM-DD (as normalised on ingest)
            ..BillEntry::default()
        }
    }

    fn make_bills(entries: Vec<BillEntry>) -> Bills {
        let mut b = Bills::default();
        b.bills = entries;
        b
    }

    // --- parse_year_month ---

    #[test]
    fn parse_year_month_valid() {
        assert_eq!(parse_year_month("2026-04").unwrap(), (2026, 4));
        assert_eq!(parse_year_month("2025-12").unwrap(), (2025, 12));
    }

    #[test]
    fn parse_year_month_invalid() {
        assert!(parse_year_month("2026").is_err());
        assert!(parse_year_month("abcd-ef").is_err());
        assert!(parse_year_month("").is_err());
    }

    // --- parse_date ---

    #[test]
    fn parse_date_valid() {
        assert_eq!(parse_date("2026-04-07").unwrap(), (2026, 4, 7));
        assert_eq!(parse_date("2025-01-31").unwrap(), (2025, 1, 31));
    }

    #[test]
    fn parse_date_invalid() {
        assert!(parse_date("2026-04").is_err());   // only 2 parts
        assert!(parse_date("20260407").is_err());   // no separators
        assert!(parse_date("abc-def-ghi").is_err());
    }

    // --- round2 ---

    #[test]
    fn round2_basic() {
        assert_eq!(round2(1.234), 1.23);
        assert_eq!(round2(1.235), 1.24);
        assert_eq!(round2(0.0), 0.0);
        assert_eq!(round2(100.0), 100.0);
    }

    // --- compute_cost ---

    #[test]
    fn compute_cost_no_filter_totals_all_rows() {
        let bills = make_bills(vec![
            make_entry("rg-a", "vm-1", 10.0, "2026-04-01"),
            make_entry("rg-b", "vm-2", 20.0, "2026-04-01"),
            make_entry("rg-a", "vm-3", 5.0,  "2026-04-02"),
        ]);
        let r = compute_cost(&bills, "", "", None);
        assert_eq!(r.row_count, 3);
        assert!((r.cost_usd - 35.0).abs() < 0.001);
    }

    #[test]
    fn compute_cost_rg_filter_substring_match() {
        let bills = make_bills(vec![
            make_entry("my-prod-rg", "vm-1", 10.0, "2026-04-01"),
            make_entry("dev-rg",     "vm-2", 99.0, "2026-04-01"),
            make_entry("prod-east",  "vm-3",  5.0, "2026-04-01"),
        ]);
        // "prod" should match "my-prod-rg" and "prod-east" but not "dev-rg"
        let r = compute_cost(&bills, "prod", "", None);
        assert_eq!(r.row_count, 2);
        assert!((r.cost_usd - 15.0).abs() < 0.001);
    }

    #[test]
    fn compute_cost_name_filter_groups_by_resource_name() {
        let bills = make_bills(vec![
            make_entry("rg-a", "sql-prod-1", 10.0, "2026-04-01"),
            make_entry("rg-b", "sql-prod-2", 20.0, "2026-04-01"),
            make_entry("rg-a", "vm-other",    5.0, "2026-04-01"),
        ]);
        let r = compute_cost(&bills, "", "sql", None);
        assert_eq!(r.row_count, 2);
        assert!((r.cost_usd - 30.0).abs() < 0.001);
        // matched_resources should be keyed by resource_name, not rg
        assert!(r.matched_resources.iter().any(|e| e.name == "sql-prod-1"));
        assert!(r.matched_resources.iter().any(|e| e.name == "sql-prod-2"));
    }

    #[test]
    fn compute_cost_date_filter() {
        let bills = make_bills(vec![
            make_entry("rg-a", "vm-1", 10.0, "2026-04-01"),
            make_entry("rg-a", "vm-2", 20.0, "2026-04-02"),
            make_entry("rg-a", "vm-3",  5.0, "2026-04-01"),
        ]);
        let r = compute_cost(&bills, "", "", Some("2026-04-01"));
        assert_eq!(r.row_count, 2);
        assert!((r.cost_usd - 15.0).abs() < 0.001);
    }

    #[test]
    fn compute_cost_combined_rg_and_date_filter() {
        let bills = make_bills(vec![
            make_entry("prod-rg", "vm-1", 10.0, "2026-04-01"),
            make_entry("dev-rg",  "vm-2", 20.0, "2026-04-01"),
            make_entry("prod-rg", "vm-3",  5.0, "2026-04-02"),
        ]);
        let r = compute_cost(&bills, "prod", "", Some("2026-04-01"));
        assert_eq!(r.row_count, 1);
        assert!((r.cost_usd - 10.0).abs() < 0.001);
    }

    #[test]
    fn compute_cost_top10_truncation() {
        // 15 distinct resource groups — result should only return 10
        let entries: Vec<BillEntry> = (0..15)
            .map(|i| make_entry(&format!("rg-{i:02}"), "vm", i as f64, "2026-04-01"))
            .collect();
        let bills = make_bills(entries);
        let r = compute_cost(&bills, "", "", None);
        assert_eq!(r.matched_resources.len(), 10);
        // top entry should be the most expensive (rg-14 = $14)
        assert_eq!(r.matched_resources[0].name, "rg-14");
    }

    #[test]
    fn compute_cost_no_match_returns_zero() {
        let bills = make_bills(vec![
            make_entry("rg-a", "vm-1", 10.0, "2026-04-01"),
        ]);
        let r = compute_cost(&bills, "nonexistent", "", None);
        assert_eq!(r.row_count, 0);
        assert_eq!(r.cost_usd, 0.0);
        assert!(r.matched_resources.is_empty());
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    // Load .env file from current directory (advisory — no-op if absent)
    dotenvy::dotenv().ok();

    let args = Args::parse();

    // Resolve Entra config unless --no-auth was set
    let entra: Option<EntraConfig> = if args.no_auth {
        eprintln!("[bill_analysis_mcp] WARNING: --no-auth set, all callers trusted");
        None
    } else {
        match load_entra_config() {
            Some(cfg) => Some(cfg),
            None => {
                eprintln!(
                    "[bill_analysis_mcp] ERROR: missing required env vars \
                     (ENTRA_TENANT_ID, ENTRA_CLIENT_ID, ENTRA_CLIENT_SECRET, MCP_PUBLIC_URL).\n\
                     Copy .env.example to .env and fill in the values, or pass --no-auth to \
                     disable authentication.\n\
                     Optional for containerized MCP clients: set MCP_CALLBACK_URL separately \
                     (e.g. MCP_PUBLIC_URL=http://host.docker.internal:8091 and \
                     MCP_CALLBACK_URL=http://localhost:8091)."
                );
                std::process::exit(1);
            }
        }
    };

    // Resolve bind host/port: CLI flag > MCP_URL env var > hardcoded defaults.
    // This keeps local bind behavior sane when MCP_URL points at
    // `host.docker.internal` for container clients.
    let mcp_url = std::env::var("MCP_URL").ok().filter(|s| !s.is_empty());
    let url_addr = entra.as_ref().and_then(|e| parse_bind_addr_from_url(&e.url))
        .or_else(|| mcp_url.as_ref().and_then(|u| parse_bind_addr_from_url(u)));
    let bind_host = args.host
        .unwrap_or_else(|| url_addr.as_ref().map(|(h, _)| h.clone()).unwrap_or_else(|| "127.0.0.1".to_string()));
    let bind_port = args.port
        .unwrap_or_else(|| url_addr.as_ref().map(|(_, p)| *p).unwrap_or(3000));

    // Validate Entra app registration at startup
    if let Some(ref cfg) = entra {
        startup_validate_entra(cfg, bind_port).await;
    }

    let state = AppState {
        cache: Arc::new(RwLock::new(HashMap::new())),
        data_dir: args.data_dir.clone(),
        entra,
        pkce_store: Arc::new(RwLock::new(HashMap::new())),
        temp_codes: Arc::new(RwLock::new(HashMap::new())),
        jwks_cache: Arc::new(RwLock::new(None)),
    };

    let app = Router::new()
        .route(
            "/mcp",
            post(mcp_handler)
                .route_layer(middleware::from_fn_with_state(state.clone(), require_auth))
                .get(mcp_get_handler),
        )
        .route("/.well-known/oauth-authorization-server", get(oauth_metadata_handler))
        // Path-based variants probed by MCP clients (RFC 8414 §3.1 / RFC 9728 §5)
        .route("/.well-known/oauth-authorization-server/mcp", get(oauth_metadata_handler))
        .route("/.well-known/openid-configuration/mcp", get(oauth_metadata_handler))
        // RFC 9728 — Protected Resource Metadata (primary + path variants)
        .route("/.well-known/oauth-protected-resource", get(oauth_protected_resource_handler))
        .route("/.well-known/oauth-protected-resource/mcp", get(oauth_protected_resource_handler))
        .route("/mcp/.well-known/oauth-protected-resource", get(oauth_protected_resource_handler))
        .route("/authorize", get(authorize_handler))
        .route("/callback", get(callback_handler))
        .route("/token", post(token_handler))
        .with_state(state)
        .layer(middleware::from_fn(log_request));

    let addr = format!("{bind_host}:{bind_port}");
    eprintln!("[bill_analysis_mcp] listening on http://{addr}/mcp");
    eprintln!("[bill_analysis_mcp] data-dir: {:?}", args.data_dir);

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|e| panic!("Failed to bind {addr}: {e}"));
    axum::serve(listener, app)
        .await
        .unwrap_or_else(|e| panic!("Server error: {e}"));
}
