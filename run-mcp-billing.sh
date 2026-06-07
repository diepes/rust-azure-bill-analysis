#!/usr/bin/env bash


# Note: ai-run.sh launches billing with config from /Users/pietersmit/github/pieter-notes/AI/mcp-config.json
#       "local-start": "~/github/rust-azure-bill-analysis/bill_analysis/target/release/bill_analysis_mcp --data-dir ~/Documents/Azure-billing/ --port 8091",


cd bill_analysis
cargo run --release --bin bill_analysis_mcp -- --data-dir ~/Documents/Azure-billing/ --port 8091

# --no-auth

echo "# Exit $0"
