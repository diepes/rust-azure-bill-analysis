//! bill_analysis_mcp — MCP server exposing Azure billing data via Streamable HTTP.
//!
//! Implements the 2025 MCP spec (Streamable HTTP transport): a single `POST /mcp` endpoint
//! that accepts JSON-RPC 2.0 requests and returns JSON responses.
//!
//! Usage: bill_analysis_mcp [--data-dir <path>] [--port <port>] [--host <host>]
//!
//! Defaults: data-dir = ./csv_data, port = 3000, host = 127.0.0.1

use axum::{
    Router,
    extract::State,
    response::{IntoResponse, Response},
    routing::post,
    Json,
};
use bill_analysis::{bills::Bills, cmd_parse::FilterOpts, find_files};
use clap::Parser;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Instant};
use tokio::sync::RwLock;

// ---------------------------------------------------------------------------
// CLI args
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(name = "bill_analysis_mcp", about = "MCP server for Azure billing cost data")]
struct Args {
    /// Directory containing billing CSV subfolders (default: ./csv_data)
    #[arg(long, default_value = "./csv_data")]
    data_dir: PathBuf,

    /// TCP port to listen on (default: 3000)
    #[arg(long, default_value_t = 3000)]
    port: u16,

    /// Host address to bind to (default: 127.0.0.1)
    #[arg(long, default_value = "127.0.0.1")]
    host: String,
}

// ---------------------------------------------------------------------------
// Shared state
// ---------------------------------------------------------------------------

type BillCache = Arc<RwLock<HashMap<(u32, u32), Arc<Bills>>>>;

#[derive(Clone)]
struct AppState {
    cache: BillCache,
    data_dir: PathBuf,
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
// MCP request handler
// ---------------------------------------------------------------------------

async fn mcp_handler(State(state): State<AppState>, Json(req): Json<RpcRequest>) -> Response {
    let start = Instant::now();
    let method = req.method.as_str();
    eprintln!("[mcp] → {method}");

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
    RpcResponse::ok(
        req.id.clone(),
        json!({
            "protocolVersion": "2025-03-26",
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
        if let Some(date) = date_filter {
            if entry.date != date {
                continue;
            }
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
    let args = Args::parse();

    let state = AppState {
        cache: Arc::new(RwLock::new(HashMap::new())),
        data_dir: args.data_dir.clone(),
    };

    let app = Router::new().route("/mcp", post(mcp_handler)).with_state(state);

    let addr = format!("{}:{}", args.host, args.port);
    eprintln!("[bill_analysis_mcp] listening on http://{addr}/mcp");
    eprintln!("[bill_analysis_mcp] data-dir: {:?}", args.data_dir);

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|e| panic!("Failed to bind {addr}: {e}"));
    axum::serve(listener, app)
        .await
        .unwrap_or_else(|e| panic!("Server error: {e}"));
}
