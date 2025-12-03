# reservation_plan

Azure reservation planning tool written in Rust

Retrieves Azure reservations using the Azure CLI and displays monthly distribution analysis.

## Usage

       cd reservation_plan
       cargo run --release

## Command-line Options

* `-h, --help` - Show help message
* `-f, --force, --refresh` - Force refresh from Azure (bypass cache)
* `--show-expired-reservations` - Include expired reservations in output

## Features

* Fetches all active Azure reservations via Azure CLI
* Caches results in `cache_reservations_YYYYMM.json` for the current month
* Filters out expired and cancelled reservations by default
* Displays comprehensive summary statistics:
  * Total reservations and quantity (flex units)
  * Breakdown by resource type (e.g., VirtualMachines, PostgreSQL, etc.)
  * Breakdown by term (1 Year / 3 Years)
  * Monthly expiry distribution starting from current month
* Color-coded monthly distribution:
  * Green: below-average units (good months to add new reservations)
  * Red: above-average units (months to avoid adding more)
* Shows both overall totals and 3-year reservation distribution
* Per-service monthly breakdown with color-coded hotspots

## Output

The tool displays:
1. A detailed table of all active reservations
2. Summary statistics grouped by resource type, VM SKU, and term
3. Monthly expiry distribution table with color coding
4. Per-service monthly breakdown showing units/reservations (e.g., "10u/5r" = 10 units / 5 reservations)

### Example Output

![Reservation Plan Output](Screenshot_2025-12-03.png)

**Legend:**
- Green cells: Below average (good months to add new reservations)
- Red cells: Above average (avoid adding more in these months)
- Format: `XXu/Yr` = XX units / Y reservations

## Requirements

* Azure CLI must be installed and authenticated
* Rust toolchain (cargo)
