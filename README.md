# Overview

Rust tool to summarize Detailed Azure bill

## Subcommands

* in folder ./bill_analysis ( cd bill_analysis )
  * cargo run -- -h

### Command bill-summary

* Reads multiple Azure billing CSV's and print some sumary info regarding reservations, and unused reservations.

### Command resource-price

* Takes csv or txt file of disk names and does lookup in latest bill printing the cost for each disk.

       cargo run -- resource-price --diskfile ../Azuredisks-Jira-PPD-29997.txt

* Get cost total for all subscriptions containing a '7'

       cargo build --release --manifest-path bill_analysis/Cargo.toml
       #
       ./bill_analysis.rs --subscription "Torpedo7" 
       ./bill_analysis.rs --bill-path ./csv_data/Detail_Enrollment_70785102_202405_en.csv resource-price --subscription ".*7$" # Prod only
       ./bill_analysis.rs --bill-path ./csv_data/Detail_Enrollment_70785102_202405_en.csv resource-price --subscription ".*7.*Non-Prod"
       # No command
       ./bill_analysis.rs  --subscription "Torpedo7" --bill_path --bill-path ./csv_data/Detail_Enrollment_70785102_202405_en.csv
       # Find AKS RG's cost breakdown
       ./bill_analysis.rs --bill-path ./csv_data/Detail_Enrollment_70785102_202404_en.csv --resource-group="^MC"
       # Remove all previous month entries - only view new
       ./bill_analysis.rs --bill-path ./csv_data/Detail_Enrollment_70785102_202405_en.csv --bill-prev-subtract-path ./csv_data/Detail_Enrollment_70785102_202404_en.csv --resource-group ".*"

* RG summary

       cargo run -- resource-price --


## Run in watch/debug mode

* Install cargo-watch if you haven't already. You can do this by running the following command in your terminal:
watch

       cargo install cargo-watch
* Once cargo-watch is installed, you can run your project in watch mode with the following command:

      cargo watch -x run

## Speedup mmap read

* src/bill.rs replaced file read with mmap, no speedup
  * when run with --release 6s drop to 0.6s even for normal file read.
