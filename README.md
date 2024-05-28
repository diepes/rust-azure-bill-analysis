# Overview

Rust tool to summarize Detailed Azure bill

## Subcommands

* in folder ./bill_analysis
  * cargo run -- -h

### Command bill-summary

* Reads multiple Azure billing CSV's and print some sumary info regarding reservations, and unused reservations.

### Command resource-price

* Takes csv or txt file of disk names and does lookup in latest bill printing the cost for each disk.

       cargo run -- resource-price --diskfile ../Azuredisks-Jira-PPD-29997.txt

* Get cost total for all subscriptions containing a '7'

       cargo run -- --bill-path ./csv_data/Detail_Enrollment_70785102_202403_en.csv resource-price --subscription ".*7.*"

* RG summary

       cargo run -- resource-price --


## Run in watch/debug mode

* Install cargo-watch if you haven't already. You can do this by running the following command in your terminal:
watch

       cargo install cargo-watch
* Once cargo-watch is installed, you can run your project in watch mode with the following command:

      cargo watch -x run
