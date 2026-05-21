#!/usr/bin/env bash

cd bill_analysis
cargo run --release --bin bill_analysis_mcp -- --data-dir ./csv-data --no-auth

echo "# Exit $0"