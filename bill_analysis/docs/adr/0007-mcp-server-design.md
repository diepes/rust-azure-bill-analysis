# ADR-0007: MCP Server Design

## Status

Accepted

## Context

We want to expose Azure billing cost data to LLMs via the Model Context Protocol (MCP), so that agents can query spend for specific resources or resource groups as part of automated cost reporting workflows.

The existing `bill_analysis` CLI binary is short-lived (run-and-exit). An MCP server is a daemon that must stay running and handle repeated requests, so it warrants a separate binary.

## Decision

### Separate binary

A new binary `bill_analysis_mcp` is added to the same Cargo workspace. It shares all library code (`Bills`, `BillEntry`, `BillFilter`, `cost_by_any_summary`, etc.) via the existing `lib.rs` public surface.

### Transport: Streamable HTTP (2025 MCP spec)

The server exposes a single `POST /mcp` endpoint using the Streamable HTTP transport defined in the 2025 MCP specification. This is what modern MCP clients (Claude Desktop, VS Code Copilot, Cursor) expect. Responses are plain JSON for all cost tool calls (no streaming body needed).

Implementation uses **axum** rather than the `rmcp` crate. Axum gives us full control of the request lifecycle, which is important for logging performance data (CSV load time, parse time, query time) when handling billing files that may exceed 500 MB.

### Bill data loading: lazy stateful cache

The server maintains a `Arc<RwLock<HashMap<YearMonth, Bills>>>` cache. On first request for a given month, the CSV is loaded and parsed; the result is stored in the cache. Subsequent requests for the same month are served from memory. CSV load and parse duration is logged on every cache miss.

### Bill file discovery

The `--data-dir` CLI flag specifies where to look for billing CSVs. Defaults to `./csv_data`. The existing `find_files` logic (date-shorthand resolution, subfolder scanning) is reused unchanged.

### MCP tools exposed

| Tool | Parameters | Returns |
|---|---|---|
| `list_available_months` | _(none)_ | Array of `"YYYY-MM"` strings |
| `get_monthly_cost` | `month: "YYYY-MM"`, `resource_group?: string`, `resource_name?: string` | Cost summary (see below) |
| `get_daily_cost` | `date: "YYYY-MM-DD"`, `resource_group?: string`, `resource_name?: string` | Cost summary (see below) |
| `search_resources` | `month: "YYYY-MM"`, `resource_type_filter?: string`, `meter_category_filter?: string`, `subscription_filter?: string`, `rg_filter?: string`, `name_filter?: string`, `tag_filter?: string`, `limit?: number` | Array of resource rows with cost (see below) |

**Filter behaviour:** `resource_group` and `resource_name` are case-insensitive substring matches against `BillEntry::resource_group` and `BillEntry::resource_name` respectively. When a filter is specified, only matching rows are included. The tool description documents this for the LLM.

**Response shape (cost summary):**

```json
{
  "cost_usd": 1234.56,
  "row_count": 892,
  "period": "2026-04",
  "matched_resources": [
    { "name": "prod-eastus-rg", "cost_usd": 800.00, "row_count": 500 },
    { "name": "prod-westus-rg", "cost_usd": 434.56, "row_count": 392 }
  ]
}
```

When no filter is specified, `matched_resources` contains the top-10 contributors (by cost) so the LLM has context for follow-up questions. When a filter is active, `matched_resources` lists every distinct matched resource/RG with its individual cost and row count.

**Response shape (`search_resources`):**

```json
{
  "total_resources": 42,
  "total_cost_usd": 950.30,
  "resources": [
    {
      "resource_name": "prod-sql-01",
      "resource_group": "rg-prod-eastus",
      "resource_type": "microsoft.sql/servers",
      "meter_category": "SQL Database",
      "subscription_name": "ingenie-prod",
      "total_cost_usd": 310.50,
      "cost_usd_formatted": "$310.50"
    }
  ]
}
```

Results are sorted by `total_cost_usd` descending. `total_resources` reflects the untruncated count — if it exceeds `limit` (default 50, max 200), the LLM knows results were truncated. All string filters are case-insensitive regex substring matches. `resource_type` is extracted from the ARM `resource_id` path as `{namespace}/{type}` (see `ResourceType` in CONTEXT.md).

**Currency:** all costs are returned in USD (`costInUsd` column). USD is preferred over billing currency (NZD) because it is unaffected by exchange rate fluctuations between billing periods.

**Timezone:** the Azure Detailed CSV `date` column is a calendar date only (`MM/DD/YYYY`, no time component). No timezone conversion is required for daily queries.

## Alternatives considered

- **`--mcp` flag on the existing binary:** rejected because the CLI is designed to be short-lived, and mixing a daemon mode into it complicates both the CLI UX and process lifecycle management.
- **`rmcp` crate:** rejected in favour of axum to retain full control of the request lifecycle for performance logging.
- **Stateless (load per request):** rejected because LLM agents frequently ask follow-up questions about the same month; paying a 2–5 s CSV parse cost on every call would be unacceptable.
- **SSE transport:** the older two-endpoint SSE variant of MCP is superseded by Streamable HTTP in the 2025 spec.

## Consequences

- A new `bin/bill_analysis_mcp.rs` (or `src/bin/mcp.rs`) is added to the workspace.
- `axum`, `tokio`, and `serde_json` are added as dependencies (scoped to the new binary where possible).
- The `BillFilter` and `cost_by_any_summary` APIs must remain accessible from `lib.rs`.
- Performance metrics (CSV load ms, parse ms, query ms, cache hit/miss) are logged to stderr on every request.
