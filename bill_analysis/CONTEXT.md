# bill_analysis — Project Context

## Purpose

A Rust CLI tool that parses **Azure "Detailed" cost export CSVs** (amortized/enrollment format) and reports spend, reservation savings, and cost breakdowns. Supports filtering by resource name, resource group, subscription, meter category, location, reservation benefit, and tags. Can diff two billing periods (latest minus previous) and report cost changes.

## Domain Glossary

| Term | Meaning |
|---|---|
| **BillEntry** | One row from an Azure Detailed CSV — a single charge line for a resource on a given date |
| **Bills** | Collection of `BillEntry` rows parsed from one CSV file, with pre-computed totals |
| **BillingCurrency** | Currency code found in the CSV (e.g. `NZD`) |
| **EffectivePrice / cost** | Actual billed amount after reservations/discounts |
| **UnitPrice** | Per-unit list price before negotiated discounts |
| **TotalUsedSavings** | Cost saved by applied reservation benefits |
| **TotalUnusedSavings** | Cost of reservation capacity that went unused |
| **NoReservation cost** | What the bill would have been without any reservations (`effective + used_savings + unused_savings`) |
| **MeterCategory** | Azure service category (e.g. `Virtual Machines`, `Azure App Service`) |
| **MeterSubCategory** | Finer-grained service type within a category |
| **MeterName** | Specific meter within a sub-category (e.g. `Intra-Region Ingress`) |
| **ResourceGroup** | Azure resource group name |
| **Subscription** | Azure subscription name or ID |
| **Region / Location** | Azure region (e.g. `australiaeast`) |
| **Reservation / Benefit** | Azure Reserved Instance or Savings Plan — `benefitName` in the CSV |
| **Tag** | Azure resource tag key-value pair |
| **AzDisk** | An unattached Azure managed disk (from a separate disk inventory CSV or `.txt`) |
| **SummaryData** | Aggregated filter result: cost totals keyed by `(CostType, name)` plus reservation detail |
| **CostType** | Dimension used to group costs: `ResourceName`, `ResourceGroup`, `Subscription`, `MeterCategory`, `MeterSubCategory`, `Tag`, `Reservation`, `Region` |
| **CostSource** | Indicates which bill a cost entry came from: `Original` (latest), `Secondary` (previous, shown as negative), `Combined` (appears in both) |
| **file_short_name** | Date portion extracted from the billing CSV filename (format `_YYYYMM_`) |
| **BillFilter** | Compiled set of regex filters (name, RG, subscription, category, location, reservation, tag, invoice section) constructed from CLI args; encodes the empty-string=match-all convention |
| **merge_summaries** | Pure function that subtracts a previous `SummaryData` from the latest one and tags each entry with its `CostSource` |
| **PreparedRow** | Display-ready row produced by `prepare_rows` — carries NZD cost, USD cost, name, colour label, and `CostSource`; internal to the display module |
| **FilterOpts** | Subset of options relevant to filtering (`case_sensitive`); passed to `BillFilter::new()` |
| **DisplayOpts** | Subset of options relevant to rendering (`cost_min_display`, `tag_list`, `debug`); passed to display functions |
| **MCP** | Model Context Protocol — a standard for exposing tools to LLMs over HTTP |
| **MCP tool** | A named function the LLM can invoke via MCP (e.g. `get_monthly_cost`) |
| **BillCache** | Lazy in-memory cache mapping `YearMonth → Bills`; populated on first access, retained for the server lifetime |
| **YearMonth** | A `(u32, u32)` year/month pair used as the cache key in `BillCache` |
| **Entra** | Microsoft Entra ID (formerly Azure AD) — the identity provider used to authenticate users of the MCP server |
| **App Registration** | The Entra application record that defines OAuth client credentials, redirect URIs, and App Role declarations for this MCP server |
| **OAuth Proxy** | The role the MCP server plays: it exposes `/authorize`, `/callback`, and `/.well-known/oauth-authorization-server` so MCP clients can drive the Auth Code flow, then passes the resulting Entra JWT through to the caller |
| **App Role** | A named permission declared on the App Registration and assigned to users/groups in Entra; appears as a `roles` claim in the JWT. The single defined role is `BillingViewer` — required to call any MCP tool. _Avoid_: Entra group, permission scope |
| **CallerIdentity** | The authenticated user's identity extracted from the validated Entra JWT — at minimum UPN (`upn`/`preferred_username`) and OID (`oid`); used for audit logging and future authorization checks |
| **PKCE State** | Short-lived in-memory record keyed by OAuth `state` parameter, correlating a `/authorize` request to its `/callback`; holds the PKCE `code_verifier` and the MCP client's `redirect_uri` |
| **JwksCache** | In-memory cache of Entra's public signing keys, used to validate JWT signatures; refreshed automatically on key-not-found |
| **`--no-auth` flag** | Explicit CLI opt-out that allows the MCP server to start without Entra config (for local dev/testing); absent this flag, missing Entra env vars cause the server to refuse to start |

## Architecture

```
main.rs  (bill_analysis CLI)
  └─ cmd_parse::App (clap CLI)
       ├─ GlobalOpts  (--bill-path, --bill-prev-subtract-path, --cost-min-display, --case-sensitive, --debug, --tag-list)
       ├─ Filters     (--name-regex, --resource-group, --subscription, --meter-category, --location, --reservation, --tag-filter, --tag-summarise)
       └─ Commands
            ├─ (default)        → load_bill → display_total_cost_summary → display_cost_by_filter
            ├─ BillSummary      → Bills::summary  (multi-file enrollment format)
            └─ DiskCsvSavings   → AzDisks::parse + cost_by_resource_name per disk

src/bin/mcp.rs  (bill_analysis_mcp MCP server)
  └─ axum POST /mcp  (Streamable HTTP, 2025 MCP spec)
       ├─ BillCache (Arc<RwLock<HashMap<YearMonth, Bills>>>)
       └─ Tools
            ├─ list_available_months  → ["YYYY-MM", ...]
            ├─ get_monthly_cost       → cost summary in USD
            └─ get_daily_cost         → cost summary in USD
```

## Module Map

```
src/
├── main.rs                        Entry point — CLI dispatch
├── lib.rs                         Public API surface
├── cmd_parse.rs                   clap CLI structs (App, GlobalOpts, Commands)
├── find_files.rs                  Regex-based file discovery in a folder
├── az_disk.rs                     AzDisk / AzDisks — disk inventory parser (CSV or TXT)
├── bin/
│   └── mcp.rs                     MCP server binary (bill_analysis_mcp) — axum, Streamable HTTP
└── bills/
    ├── bills.rs (mod)             Bills struct + parse_csv entry point
    ├── bill_entry.rs              BillEntry — single CSV row; serde PascalCase deserialise
    ├── bills_impl_basic.rs        push, len, calc_all_totals
    ├── bills_impl_cost_by_any.rs  cost_by_any_summary() — main filter+aggregation engine
    ├── bills_impl_cost_by_sub.rs  cost_by_subscription(), cost_by_resource_name()
    ├── bills_impl_currency.rs     get/set_billing_currency()
    ├── bills_sum_data.rs          SummaryData, CostTotal, CostSource, ReservationInfo
    ├── cost_type_enum.rs          CostType enum
    ├── display.rs                 display_cost_by_filter(), print_summary() — coloured terminal output
    ├── summary.rs                 Summary struct + Bills::summary() (multi-month BillSummary command)
    └── tags.rs                    Tags — serde deserialiser for Azure tag key-value pairs
```

## Key Data Flow

```
Azure Detailed CSV
  → Bills::parse_csv()
      → Vec<BillEntry>              (one row per charge line, tags parsed, RG normalised)
        → calc_all_totals()         → Summary (effective, savings used/unused, per-meter-category)
          → cost_by_any_summary()   → SummaryData (filtered, grouped by CostType)
            → display_cost_by_filter()  → coloured terminal output
```

**Previous-bill diff:** a second `SummaryData` is computed for the previous bill, costs are negated, and the two maps are merged — entries only in the previous bill appear as `CostSource::Secondary` (green), entries in both as `CostSource::Combined` (blue), new entries as `CostSource::Original` (red).

**Reservation detail:** per `(benefit_name, day_of_month)` — tracks `cost_full`, `cost_savings`, `cost_unused`, VM names reserved vs. not reserved.

## CLI Usage Patterns

```bash
# Default: show cost summary + filtered breakdown for latest bill in ./csv_data/
bill_analysis

# Diff two months
bill_analysis --bill-path ./csv_data/202405 --bill-prev-subtract-path ./csv_data/202404

# Filter by resource group regex
bill_analysis -r "prod-.*"

# Filter by meter category
bill_analysis -m "Virtual Machines"

# Summarise costs by tag value
bill_analysis -t "environment"

# Look up disk costs from an unattached-disk export
bill_analysis disk-csv-savings -d ./Azuredisks-Unattached.csv

# Multi-month CSV output (uses run-multi-month.sh)
./run-multi-month.sh
```

## Test Data

```
tests/
├── azure_test_data_01.csv    8-row BillEntry sample
├── azure_test_disks_01.csv   10-row AzDisk CSV sample
└── azure_test_disks_02.txt   5-row AzDisk newline-delimited sample
```

## MCP OAuth Setup & Troubleshooting

### How the OAuth proxy works

The MCP server acts as its own OAuth Authorization Server (AS), proxying behind Microsoft Entra ID:

```
MCP client → /authorize (MCP server) → Entra /authorize → user login
            ← /callback (Entra code)  ← browser redirect
MCP client → /token (MCP server)     → validates PKCE, returns Entra access token
MCP client → /mcp (Bearer token)     → validate_jwt + BillingViewer role check
```

### MCP 2025 spec requirements (all must be met)

1. **`issuer` must equal the fetch URL** (RFC 9207 §2) — `/.well-known/oauth-authorization-server` must return `"issuer": "http://localhost:8091"`, not Entra's issuer. Compliant clients abort if these differ.
2. **`registration_endpoint` must be present** — MCP clients use RFC 7591 dynamic client registration to obtain a `client_id` before calling `/authorize`. Without it the client never proceeds.
3. **Scope must target the app itself** — use `{client_id}/.default openid profile email` (not `api://{client_id}` which requires an Application ID URI configured in Azure). The GUID-based `/.default` scope always works.

### Azure App Registration requirements

- App Role `BillingViewer` defined with `Allowed member types: Users/Groups,Applications`, `Value: BillingViewer`
- Redirect URI `http://localhost:8091/callback` registered as a **Web** redirect URI
- **Admin consent must be granted** — App Roles cannot be user-consented by default. An Azure AD admin must grant consent once:
  ```
  https://login.microsoftonline.com/{TENANT_ID}/adminconsent?client_id={CLIENT_ID}
  ```
  Or via Portal: **Enterprise Applications → app → Permissions → Grant admin consent for [tenant]**
- Users must be **assigned the `BillingViewer` App Role** in Portal: **Enterprise Applications → app → Users and Groups → Add user/group**

### JWT validation

- `aud` must equal `entra.client_id` (the app's own GUID)
- Issuer accepted: both v2 (`login.microsoftonline.com/.../v2.0`) and v1 (`sts.windows.net/.../`)
- `roles` claim must contain `"BillingViewer"` — if missing, user authenticated successfully but gets `403 Forbidden`
- If `roles` is empty after successful login: the user has not been assigned the App Role in Azure

### Environment variables

| Variable | Example | Purpose |
|---|---|---|
| `ENTRA_TENANT_ID` | `6eedd2c9-...` | Azure AD tenant |
| `ENTRA_CLIENT_ID` | `d803de61-...` | App registration client ID |
| `ENTRA_CLIENT_SECRET` | `...` | Client secret for token exchange |
| `MCP_PUBLIC_URL` | `http://localhost:8091` | Base URL the MCP server is reachable at (used in well-known metadata) |
| `MCP_CALLBACK_URL` | `http://localhost:8091/callback` | Must match redirect URI registered in Azure |

### Debugging checklist

- `[oauth] 401 missing Authorization header` — normal on first connect; client should then read `/.well-known` endpoints
- Client reads `/.well-known` but never calls `/authorize` → check `issuer` equals `MCP_PUBLIC_URL`, check `registration_endpoint` is present in metadata
- `Entra error: invalid_resource` → scope uses `api://` prefix but Application ID URI not configured; switch to `{client_id}/.default`
- Browser shows "Need admin approval" → admin consent not yet granted (see above)
- `[oauth] FAIL missing_role` → user authenticated but `BillingViewer` App Role not assigned to them in Azure
- `[oauth] OK oid=... upn=... roles=[BillingViewer]` → fully authenticated and authorised ✓

## Notable Conventions

- **Case folding:** all `resource_name`, `resource_group`, `subscription_name`, and tag values are lowercased on ingest unless `--case-sensitive` is set. All regex filters are case-insensitive by default.
- **Empty ResourceGroup:** purchase entries with no RG are assigned a synthetic name: `EMPTY_RG__PUBL:{publisher}__MCat:{category}__MSubCat:{sub_category}`.
- **`cost_min_display`:** entries with `|cost| < cost_min_display` (default `10.00`) are counted but not printed individually.
- **Bill file naming:** the tool expects filenames matching `.*Detailed.*.csv` and extracts the `_YYYYMM_` date token as the display label.
- **BillPath shorthand:** if `--bill-path` (or `--bill-prev-subtract-path`) starts with `YYYY-MM` or `YYYYMM`, it is treated as a date shorthand and resolved under the hardcoded `csv_data/` base directory — first as a subdirectory prefix, then as a CSV file prefix, trying both date formats. Any other value is used as a literal path. See `docs/adr/0001-date-shorthand-bill-path.md`.
