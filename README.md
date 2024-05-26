# Overview

Rust tool to summarize Detailed Azure bill

## Subcommands
 * in folder ./bill_analysis
   * cargo run -- -h

### Command bill-summary
 * Reads multiple Azure billing CSV's and print some sumary info regarding reservations, and unused reservations.


### Command disk-price
 * Takes csv or txt file of disk names and does lookup in latest bill printing the cost for each disk.
 

## Run in watch/debug mode

* Install cargo-watch if you haven't already. You can do this by running the following command in your terminal:
watch

       cargo install cargo-watch
* Once cargo-watch is installed, you can run your project in watch mode with the following command:

      cargo watch -x run
