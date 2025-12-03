# Azure Bill Analysis - Rust Tools

This repository contains two Rust-based tools for Azure cost analysis:

## Tools

### 1. [bill_analysis](README-bill_analysis.md)

Analyzes detailed Azure billing CSV files to summarize costs, track reservations, and compare billing periods.

**Quick start:**
```bash
cd bill_analysis
cargo run --release -- -h
```

[View full documentation →](README-bill_analysis.md)

### 2. [reservation_plan](README-reservation_plan.md)

Retrieves Azure reservations via Azure CLI and displays monthly expiry distribution with color-coded hotspot analysis.

**Quick start:**
```bash
cd reservation_plan
cargo run --release
```

[View full documentation →](README-reservation_plan.md)

## Requirements

* Rust toolchain (cargo)
* Azure CLI (for reservation_plan)