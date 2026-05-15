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

## Architecture

```
main.rs
  └─ cmd_parse::App (clap CLI)
       ├─ GlobalOpts  (--bill-path, --bill-prev-subtract-path, --cost-min-display, --case-sensitive, --debug, --tag-list)
       ├─ Filters     (--name-regex, --resource-group, --subscription, --meter-category, --location, --reservation, --tag-filter, --tag-summarise)
       └─ Commands
            ├─ (default)        → load_bill → display_total_cost_summary → display_cost_by_filter
            ├─ BillSummary      → Bills::summary  (multi-file enrollment format)
            └─ DiskCsvSavings   → AzDisks::parse + cost_by_resource_name per disk
```

## Module Map

```
src/
├── main.rs                        Entry point — CLI dispatch
├── lib.rs                         Public API surface
├── cmd_parse.rs                   clap CLI structs (App, GlobalOpts, Commands)
├── find_files.rs                  Regex-based file discovery in a folder
├── az_disk.rs                     AzDisk / AzDisks — disk inventory parser (CSV or TXT)
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

## Notable Conventions

- **Case folding:** all `resource_name`, `resource_group`, `subscription_name`, and tag values are lowercased on ingest unless `--case-sensitive` is set. All regex filters are case-insensitive by default.
- **Empty ResourceGroup:** purchase entries with no RG are assigned a synthetic name: `EMPTY_RG__PUBL:{publisher}__MCat:{category}__MSubCat:{sub_category}`.
- **`cost_min_display`:** entries with `|cost| < cost_min_display` (default `10.00`) are counted but not printed individually.
- **Bill file naming:** the tool expects filenames matching `.*Detailed.*.csv` and extracts the `_YYYYMM_` date token as the display label.
- **BillPath shorthand:** if `--bill-path` (or `--bill-prev-subtract-path`) starts with `YYYY-MM` or `YYYYMM`, it is treated as a date shorthand and resolved under the hardcoded `csv_data/` base directory — first as a subdirectory prefix, then as a CSV file prefix, trying both date formats. Any other value is used as a literal path. See `docs/adr/0001-date-shorthand-bill-path.md`.
