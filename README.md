# Azure Bill Analysis - Rust Tools

This repository contains Rust-based tools for Azure cost analysis:

## Tools

### 1. [bill_analysis](README-bill_analysis.md)

CLI tool that analyzes detailed Azure billing CSV files — summarizes costs, tracks reservations, and compares billing periods.

**Quick start:**
```bash
cd bill_analysis
cargo run --release -- -h
```

[View full documentation →](README-bill_analysis.md)

### 2. bill_analysis_mcp

MCP server that exposes Azure billing data to LLMs via the [Model Context Protocol](https://modelcontextprotocol.io/). Supports queries by month, resource group, resource type, meter category, subscription, and tags.

**Quick start:**
```bash
cd bill_analysis
# --no-auth
cargo run --bin bill_analysis_mcp --  --data-dir ./csv_data --port 8091
```

See `bill_analysis/CONTEXT.md` for MCP tool reference and OAuth setup.

### 3. [reservation_plan](README-reservation_plan.md)

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
