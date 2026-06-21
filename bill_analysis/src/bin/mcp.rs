//! bill_analysis_mcp — MCP server exposing Azure billing data via Streamable HTTP.
//!
//! Implements the 2025 MCP spec (Streamable HTTP transport):
//!   POST /mcp  — JSON-RPC 2.0 requests (requires BillingViewer App Role unless --no-auth)
//!   GET  /mcp  — Health check
//!
//! OAuth 2.0 Authorization Code + PKCE proxy against Microsoft Entra ID is
//! implemented in the `oauth_proxy` sub-module.
//!
//! Usage: bill_analysis_mcp --data-dir <path> [--port <port>] [--host <host>] [--no-auth]
//! Config: ENTRA_TENANT_ID, ENTRA_CLIENT_ID, ENTRA_CLIENT_SECRET, MCP_URL (or .env)

mod oauth_proxy;

use axum::{
    Json, Router,
    extract::Request,
    extract::State,
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use bill_analysis::{
    bills::{
        cost_query::{CostQuery, ResourceSearchQuery, query_cost, round2, search_resources},
        repository::BillRepository,
    },
    blob_source::{BlobSource, BlobSourceConfig},
};
use clap::Parser;
use oauth_proxy::{
    AppState, CallerIdentity, auth_start_handler, auth_wait_handler, authorize_handler,
    callback_handler, load_entra_config, oauth_metadata_handler, oauth_protected_resource_handler,
    parse_bind_addr_from_url, register_handler, require_auth, startup_validate_entra,
    token_handler,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc, time::Instant};
use tracing_subscriber::prelude::*;

// ---------------------------------------------------------------------------
// CLI args
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(
    name = "bill_analysis_mcp",
    about = "MCP server for Azure billing cost data"
)]
struct Args {
    /// Directory containing billing CSV subfolders
    #[arg(long)]
    data_dir: PathBuf,

    /// TCP port to listen on. Defaults to the port in MCP_URL, or 3000.
    #[arg(long)]
    port: Option<u16>,

    /// Host address to bind to. Defaults to the host in MCP_URL, or 127.0.0.1.
    #[arg(long)]
    host: Option<String>,

    /// Disable authentication (skip Entra config; all callers are trusted).
    /// Without this flag the server refuses to start if Entra env vars are missing.
    #[arg(long)]
    no_auth: bool,

    /// Skip the BillingViewer App Role check. JWT is still validated.
    /// Useful for testing the OAuth flow without admin-assigned App Roles.
    #[arg(long)]
    no_role_check: bool,
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
        Self {
            jsonrpc: "2.0",
            result: Some(result),
            error: None,
            id,
        }
    }
    fn err(id: Option<Value>, code: i32, message: String) -> Self {
        Self {
            jsonrpc: "2.0",
            result: None,
            error: Some(RpcError { code, message }),
            id,
        }
    }
}

// ---------------------------------------------------------------------------
// GET /mcp — health / liveness probe
// ---------------------------------------------------------------------------

async fn mcp_get_handler(State(state): State<AppState>) -> impl IntoResponse {
    let auth = if state.entra.is_some() {
        "entra"
    } else {
        "disabled"
    };
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
    if !is_probe {
        log::debug!("[http] {} {}", method, uri);
    }
    let resp = next.run(req).await;
    if !is_probe {
        log::debug!(
            "[http]   → {} in {:.1}ms",
            resp.status(),
            start.elapsed().as_secs_f64() * 1000.0
        );
    }
    resp
}

// ---------------------------------------------------------------------------
// MCP request handler
// ---------------------------------------------------------------------------

async fn mcp_handler(
    State(state): State<AppState>,
    caller: Option<axum::Extension<CallerIdentity>>,
    Json(req): Json<RpcRequest>,
) -> Response {
    let start = Instant::now();
    let method = req.method.as_str();
    if let Some(params) = &req.params {
        log::debug!("[mcp] → {method} {}", params);
    } else {
        log::debug!("[mcp] → {method}");
    }

    // Notifications have no `id` — acknowledge but send no body.
    if req.id.is_none() {
        log::debug!(
            "[mcp] ← notification in {:.1}ms",
            start.elapsed().as_secs_f64() * 1000.0
        );
        return StatusCode::ACCEPTED.into_response();
    }

    // Extract tool name before dispatch so we can include it in the audit log.
    let tool_name = if method == "tools/call" {
        req.params
            .as_ref()
            .and_then(|p| p.as_object())
            .and_then(|p| p.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string()
    } else {
        String::new()
    };

    let resp = match method {
        "initialize" => handle_initialize(&req),
        "ping" => RpcResponse::ok(req.id.clone(), json!({})),
        "tools/list" => handle_tools_list(&req),
        "tools/call" => handle_tools_call(&req, &state).await,
        _ => RpcResponse::err(
            req.id.clone(),
            -32601,
            format!("Method not found: {method}"),
        ),
    };

    let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;

    if !tool_name.is_empty() {
        let upn = caller
            .as_ref()
            .map(|ext| ext.upn.as_str())
            .unwrap_or("anonymous");
        let resp_json = serde_json::to_string(&resp).unwrap_or_default();
        let bytes = resp_json.len();
        tracing::info!(upn, tool = tool_name, bytes, run_msec = elapsed_ms as u64,);
        return axum::response::Json(resp).into_response();
    }

    log::debug!("[mcp] ← {method} in {elapsed_ms:.1}ms");
    Json(resp).into_response()
}

// ---------------------------------------------------------------------------
// initialize
// ---------------------------------------------------------------------------

fn handle_initialize(req: &RpcRequest) -> RpcResponse {
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
                    "description": "Get the total Azure cost in USD for a given billing month. All filters are case-insensitive regexes — plain strings match as substrings, but anchors, alternation (prod|staging), and wildcards (ingenie.*) are all valid. Returns the total cost, row count, and top contributors.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "month": {
                                "type": "string",
                                "description": "Billing month in YYYY-MM format, e.g. '2026-04'."
                            },
                            "resource_group": {
                                "type": "string",
                                "description": "Case-insensitive regex matched against resource group names. Plain strings work as substring filters, e.g. 'prod' matches 'my-prod-eastus-rg'. Omit to include all resource groups."
                            },
                            "resource_name": {
                                "type": "string",
                                "description": "Case-insensitive regex matched against resource names. Supports alternation, e.g. 'ingenie|eroad'. Omit to include all resources."
                            },
                            "tag_filter": {
                                "type": "string",
                                "description": "Case-insensitive regex matched against the full tag string, e.g. 'environment.*prod' matches resources tagged environment=prod. Tag string format: '\"Key\": \"Value\",\"Key2\": \"Value2\"'. Omit to include all resources regardless of tags."
                            }
                        },
                        "required": ["month"]
                    }
                },
                {
                    "name": "get_daily_cost",
                    "description": "Get the total Azure cost in USD for a specific calendar date. All filters are case-insensitive regexes — plain strings match as substrings, anchors and alternation are also valid. The billing CSV uses UTC calendar dates.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "date": {
                                "type": "string",
                                "description": "Calendar date in YYYY-MM-DD format, e.g. '2026-04-07'."
                            },
                            "resource_group": {
                                "type": "string",
                                "description": "Case-insensitive regex matched against resource group names. Omit to include all resource groups."
                            },
                            "resource_name": {
                                "type": "string",
                                "description": "Case-insensitive regex matched against resource names. Supports alternation, e.g. 'ingenie|eroad'. Omit to include all resources."
                            },
                            "tag_filter": {
                                "type": "string",
                                "description": "Case-insensitive regex matched against the full tag string, e.g. 'environment.*prod'. Omit to include all resources regardless of tags."
                            }
                        },
                        "required": ["date"]
                    }
                },
                {
                    "name": "search_resources",
                    "description": "Search for individual Azure resources and their costs for a billing month. Returns one row per unique resource (resource_name + resource_group) with cost, meter category, and Azure resource type (extracted from the ARM resource ID). Use meter_category or resource_type to find all resources of a specific kind (e.g. all Public IPs, all Disks). Results are capped at 50 by default, sorted by cost descending. The response includes total_resources and total_cost_usd for the full matched set (before the limit).",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "month": {
                                "type": "string",
                                "description": "Billing month in YYYY-MM format, e.g. '2026-04'."
                            },
                            "resource_group": {
                                "type": "string",
                                "description": "Case-insensitive regex matched against resource group names. Omit to include all resource groups."
                            },
                            "resource_name": {
                                "type": "string",
                                "description": "Case-insensitive regex matched against resource names. Supports alternation, e.g. 'ingenie|eroad'. Omit to include all resources."
                            },
                            "meter_category": {
                                "type": "string",
                                "description": "Case-insensitive regex matched against the Azure meter category, e.g. 'Virtual Machines', 'Storage', 'Virtual Network'. Omit to include all categories."
                            },
                            "subscription": {
                                "type": "string",
                                "description": "Case-insensitive regex matched against subscription names. Supports alternation, e.g. 'prod|staging'. Omit to include all subscriptions."
                            },
                            "resource_type": {
                                "type": "string",
                                "description": "Case-insensitive regex matched against the Azure resource type extracted from the ARM resource ID (e.g. 'publicipaddresses', 'disks', 'microsoft.compute/virtualmachines'). Omit to include all resource types."
                            },
                            "tag_filter": {
                                "type": "string",
                                "description": "Case-insensitive regex matched against the full tag string, e.g. 'environment.*prod'. Omit to include all resources regardless of tags."
                            },
                            "limit": {
                                "type": "integer",
                                "description": "Maximum number of resources to return (default 50, max 200). Results are sorted by cost descending."
                            }
                        },
                        "required": ["month"]
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
        "search_resources" => tool_search_resources(args, state).await,
        _ => Err(format!("Unknown tool: {tool_name}")),
    };

    match result {
        Ok(text) => RpcResponse::ok(
            req.id.clone(),
            json!({ "content": [{ "type": "text", "text": text }] }),
        ),
        Err(e) => {
            log::error!("[mcp] tool '{}' error: {}", tool_name, e);
            RpcResponse::err(req.id.clone(), -32000, e)
        }
    }
}

// ---------------------------------------------------------------------------
// Tool: list_available_months
// ---------------------------------------------------------------------------

async fn tool_list_available_months(state: &AppState) -> Result<String, String> {
    let months = state.repo.list_months_including_blob().await;
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
    let rg_filter = args
        .get("resource_group")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let name_filter = args
        .get("resource_name")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let tag_filter = args
        .get("tag_filter")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let (year, mon) = parse_year_month(month)?;
    let bills = state.repo.get(year, mon).await?;
    let result = query_cost(
        &bills,
        &CostQuery {
            rg_filter: rg_filter.to_string(),
            name_filter: name_filter.to_string(),
            tag_filter: tag_filter.to_string(),
            date_filter: None,
        },
    )?;

    Ok(serde_json::to_string_pretty(&json!({
        "cost_usd": round2(result.cost_usd),
        "row_count": result.row_count,
        "period": month,
        "top_contributors": result.top_contributors,
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
    let rg_filter = args
        .get("resource_group")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let name_filter = args
        .get("resource_name")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let tag_filter = args
        .get("tag_filter")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let (year, mon, _day) = parse_date(date_str)?;
    let bills = state.repo.get(year, mon).await?;
    let result = query_cost(
        &bills,
        &CostQuery {
            rg_filter: rg_filter.to_string(),
            name_filter: name_filter.to_string(),
            tag_filter: tag_filter.to_string(),
            date_filter: Some(date_str.to_string()),
        },
    )?;

    Ok(serde_json::to_string_pretty(&json!({
        "cost_usd": round2(result.cost_usd),
        "row_count": result.row_count,
        "date": date_str,
        "top_contributors": result.top_contributors,
    }))
    .unwrap())
}

// ---------------------------------------------------------------------------
// Tool: search_resources
// ---------------------------------------------------------------------------

async fn tool_search_resources(
    args: Option<&serde_json::Map<String, Value>>,
    state: &AppState,
) -> Result<String, String> {
    let args = args.ok_or_else(|| "Missing arguments".to_string())?;
    let month = args
        .get("month")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Missing required argument 'month'".to_string())?;

    let rg_filter = args
        .get("resource_group")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let name_filter = args
        .get("resource_name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let tag_filter = args
        .get("tag_filter")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let meter_category_filter = args
        .get("meter_category")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let subscription_filter = args
        .get("subscription")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let resource_type_filter = args
        .get("resource_type")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .map(|n| (n as usize).min(200));

    let (year, mon) = parse_year_month(month)?;
    let bills = state.repo.get(year, mon).await?;
    let result = search_resources(
        &bills,
        &ResourceSearchQuery {
            rg_filter,
            name_filter,
            tag_filter,
            meter_category_filter,
            subscription_filter,
            resource_type_filter,
            limit,
        },
    )?;

    Ok(serde_json::to_string_pretty(&json!({
        "period": month,
        "total_resources": result.total_resources,
        "total_cost_usd": result.total_cost_usd,
        "resources": result.resources,
    }))
    .unwrap())
}

// ---------------------------------------------------------------------------
// Parse helpers
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
    let year: u32 = parts[0]
        .parse()
        .map_err(|_| format!("Invalid year in '{}'", s))?;
    let mon: u32 = parts[1]
        .parse()
        .map_err(|_| format!("Invalid month in '{}'", s))?;
    let day: u32 = parts[2]
        .parse()
        .map_err(|_| format!("Invalid day in '{}'", s))?;
    Ok((year, mon, day))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(parse_date("2026-04").is_err()); // only 2 parts
        assert!(parse_date("20260407").is_err()); // no separators
        assert!(parse_date("abc-def-ghi").is_err());
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    // Load .env: try CWD first, then the project root two levels above the binary
    // (i.e. <project_root>/target/release/<binary> → <project_root>/.env).
    let env_loaded: Option<std::path::PathBuf> = dotenvy::dotenv().ok().or_else(|| {
        let project_root = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent()?.parent()?.parent().map(|p| p.to_path_buf()))?;
        let env_path = project_root.join(".env");
        dotenvy::from_path(&env_path).ok().map(|_| env_path)
    });

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("bill_analysis=info")),
        )
        .with(tracing_subscriber::fmt::layer().json().with_timer(
            tracing_subscriber::fmt::time::ChronoLocal::new("%Y%m%d-%Hh%M%:z".to_string()),
        ))
        .init();

    match &env_loaded {
        Some(path) => log::info!("[env] loaded .env from {}", path.display()),
        None => log::debug!("[env] no .env file found in CWD or project root"),
    }

    let args = Args::parse();

    let entra = if args.no_auth {
        log::warn!("[bill_analysis_mcp] --no-auth set, all callers trusted");
        None
    } else {
        match load_entra_config() {
            Some(cfg) => Some(cfg),
            None => {
                log::error!(
                    "[bill_analysis_mcp] missing required env vars \
                     (ENTRA_TENANT_ID, ENTRA_CLIENT_ID, ENTRA_CLIENT_SECRET, MCP_URL).\n\
                     Copy .env.example to .env and fill in the values, or pass --no-auth to \
                     disable authentication."
                );
                std::process::exit(1);
            }
        }
    };

    let mcp_url = std::env::var("MCP_URL").ok().filter(|s| !s.is_empty());
    let url_addr = entra
        .as_ref()
        .and_then(|e| parse_bind_addr_from_url(&e.url))
        .or_else(|| mcp_url.as_ref().and_then(|u| parse_bind_addr_from_url(u)));
    let bind_host = args.host.unwrap_or_else(|| {
        url_addr
            .as_ref()
            .map(|(h, _)| h.clone())
            .unwrap_or_else(|| "127.0.0.1".to_string())
    });
    let bind_port = args
        .port
        .unwrap_or_else(|| url_addr.as_ref().map(|(_, p)| *p).unwrap_or(3000));

    if let Some(ref cfg) = entra {
        startup_validate_entra(cfg, bind_port).await;
    }

    let blob_source = if let Some(cfg) = BlobSourceConfig::from_env() {
        log::debug!(
            "[mcp] blob source configured — container '{}', prefix '{}'",
            cfg.container_name,
            cfg.prefix
        );
        // Extract storage account name early (before cfg is moved)
        let account_name = cfg
            .blob_service_url
            .strip_prefix("https://")
            .and_then(|s| s.split('.').next())
            .unwrap_or("<account-name>")
            .to_string();

        match BlobSource::new(cfg) {
            Ok(source) => {
                match source.list_date_range_folders().await {
                    Ok(folders) => {
                        log::debug!(
                            "[mcp] blob source connected; {} date-range folder(s):",
                            folders.len()
                        );
                        for f in &folders {
                            log::debug!("[mcp]   {f}");
                        }
                    }
                    Err(e) => {
                        log::error!(
                            "[mcp] ✗ blob connectivity check failed for storage account '{}': {}",
                            account_name,
                            e
                        );
                        log::error!(
                            "[mcp] ℹ if using DefaultAzureCredential, ensure you are authenticated:"
                        );
                        let tenant_id = std::env::var("ENTRA_TENANT_ID")
                            .unwrap_or_else(|_| "<TenantID>".to_string());
                        log::error!("[mcp]   az login --tenant {}", tenant_id);
                        std::process::exit(1);
                    }
                }
                Some(Arc::new(source))
            }
            Err(e) => {
                log::error!("[mcp] ✗ blob client init failed: {e}");
                std::process::exit(1);
            }
        }
    } else {
        None
    };

    let no_role_check = args.no_role_check;
    if no_role_check && !args.no_auth {
        log::warn!("[bill_analysis_mcp] --no-role-check set, BillingViewer App Role not enforced");
    }

    let state = AppState::new(
        Arc::new(BillRepository::new(args.data_dir.clone(), blob_source)),
        entra,
        no_role_check,
    );

    let app = Router::new()
        .route(
            "/mcp",
            post(mcp_handler)
                .route_layer(middleware::from_fn_with_state(state.clone(), require_auth))
                .get(mcp_get_handler),
        )
        .route(
            "/.well-known/oauth-authorization-server",
            get(oauth_metadata_handler),
        )
        .route(
            "/.well-known/oauth-authorization-server/mcp",
            get(oauth_metadata_handler),
        )
        .route(
            "/.well-known/openid-configuration/mcp",
            get(oauth_metadata_handler),
        )
        .route(
            "/.well-known/oauth-protected-resource",
            get(oauth_protected_resource_handler),
        )
        .route(
            "/.well-known/oauth-protected-resource/mcp",
            get(oauth_protected_resource_handler),
        )
        .route(
            "/mcp/.well-known/oauth-protected-resource",
            get(oauth_protected_resource_handler),
        )
        .route("/authorize", get(authorize_handler))
        .route("/auth/start", get(auth_start_handler))
        .route("/callback", get(callback_handler))
        .route("/token", post(token_handler))
        .route("/register", post(register_handler))
        .route("/mcp/auth/wait", get(auth_wait_handler))
        .with_state(state)
        .layer(middleware::from_fn(log_request));

    let addr = format!("{bind_host}:{bind_port}");
    log::info!("[bill_analysis_mcp] listening on http://{addr}/mcp");
    log::info!("[bill_analysis_mcp] data-dir: {:?}", args.data_dir);

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|e| panic!("Failed to bind {addr}: {e}"));
    axum::serve(listener, app)
        .await
        .unwrap_or_else(|e| panic!("Server error: {e}"));
}
