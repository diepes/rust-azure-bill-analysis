# bill_analysis

Azure billing CSV analysis tool written in Rust

Diagram - [Flow Diagram](flowchartTD.md)

## Usage

* in folder ./bill_analysis ( cd bill_analysis )
  * cargo run -- -h

## Subcommands

### Command bill-summary

* Reads multiple Azure billing CSV's and print some summary info regarding reservations, and unused reservations.

### Command (default bill breakdown and comparisons)

* Get cost total for all subscriptions containing a '7'

       cd bill_analysis
       cargo run --release --
       cargo run --release -- --bill-path "./csv_data/202511"
       # current bill - older bill
       cargo run --release -- --bill-prev-subtract-path ./csv_data/*202510*.csv
       #
       cargo run --release -- --subscription "torpedo7" --bill-prev-subtract-path ./csv_data/*202502*.csv
       cargo run --release -- --resource-group dev-tui-rg --meter-category ".*" --name-regex ".*"
       #
       #
       # All VM's in Nonprod subscription
       cargo run --release --   --meter-category "Virtual Machines" --subscription "non-"
       # No command
       ./bill_analysis.rs  --subscription "Torpedo7" --bill-path ./csv_data/Detail_Enrollment_70785102_202405_en.csv
       # Find AKS RG's cost breakdown
       ./bill_analysis.rs --bill-path ./csv_data/Detail_Enrollment_70785102_202404_en.csv --resource-group="^MC"
       # Remove all previous month entries - only view new
       ./bill_analysis.rs --bill-path ./csv_data/Detail_Enrollment_70785102_202410_en.csv --bill-prev-subtract-path ./csv_data/Detail_Enrollment_70785102_202409_en.csv --resource-group ".*"
       # Filter tags and display details for specific tag
       cargo run --release -- --tag-filter "hours_of_operation=24\*7|24x7" --tag-summarise "hours_of_operation"

### Command resource-price

* ```disk-csv-savings``` Takes csv or txt file of disk names and does lookup in latest bill printing the cost for each disk.

       cargo run -- disk-csv-savings --diskfile ../Azuredisks-Jira-PPD-29997.txt

## MCP Server (bill_analysis_mcp)

The `bill_analysis_mcp` binary exposes billing data to LLMs via the [Model Context Protocol](https://modelcontextprotocol.io/) (Streamable HTTP, 2025 spec).

```bash
# Local dev (no auth)
cargo run --bin bill_analysis_mcp -- --no-auth --data-dir ./csv_data

# Production (Entra OAuth)
ENTRA_TENANT_ID=... ENTRA_CLIENT_ID=... ENTRA_CLIENT_SECRET=... \
MCP_PUBLIC_URL=http://localhost:8091 \
cargo run --bin bill_analysis_mcp -- --data-dir ./csv_data
```

### MCP Tools

| Tool | What it does |
|---|---|
| `list_available_months` | Lists months with billing data available |
| `get_monthly_cost` | Cost summary for a month, with optional RG/name filter |
| `get_daily_cost` | Cost summary for a single day, with optional RG/name filter |
| `search_resources` | Find resources by type, category, subscription, name, or tag — returns per-resource rows sorted by cost |

### Example LLM queries enabled

```
search_resources(month="2026-05", resource_type_filter="publicipaddresses")
search_resources(month="2026-05", meter_category_filter="Virtual Machines", subscription_filter="prod")
get_monthly_cost(month="2026-05", resource_group="ingenie")
```

See `CONTEXT.md` for the full OAuth setup, environment variables, and troubleshooting guide.

## Run in watch/debug mode

* Install cargo-watch if you haven't already. You can do this by running the following command in your terminal:
watch

       cargo install cargo-watch
* Once cargo-watch is installed, you can run your project in watch mode with the following command:

      cargo watch -x run

## Speedup mmap read

* src/bill.rs replaced file read with mmap, no speedup
  * when run with --release 6s drop to 0.6s even for normal file read.

## ToDo

* Error out if partial csv file name match is not unique
* Better Reservation reporting/analysis
  * Reservation under utilization
  * Resources not covered by reservations - compute & sql
  * Reservation fields
    * ResourceId = "/providers/Microsoft.Capacity/reservationOrders/79fd6a5d-938c-4ac1-bbe9-c9ad8cb08ccf/reservations/"
    * ResourceName = ""
    * ReservationName / ReservationId / benefitName
    * ChargeType = ["Usage", "Purchase"]
    * Quantity(Hr) x EffectivePrice = Cost = 0 if reserved
    * Quantity(Hr) x UnitPrice => What_the_cost_would_have_been
    * MeterName
    * MeterCategory = ["Virtual Machines", "SQL Managed Instance", "Azure App Service"]
    * MeterSubCategory = ["Dv2/DSv2 Series","Managed Instance General Purpose - Compute Gen5", "Premium Plan"]
    * Date = "dd/mm/yyyy" (Day for the bill entry, 32 for the month)
    * One entry per day per VM for Quantity = 24(Hr)
    * 2025-03 bill l:106258 ProductOrderName:"Compute savings plan, 1 Year" pricingModel:"SavingsPlan"
