# Overview

Rust tool to summarize Detailed Azure bill

## Subcommands

* in folder ./bill_analysis ( cd bill_analysis )
  * cargo run -- -h

### Command bill-summary

* Reads multiple Azure billing CSV's and print some sumary info regarding reservations, and unused reservations.

### Command (default bill breakdown and comparisons)

* Get cost total for all subscriptions containing a '7'

       cd bill_analysis
       cargo run --release --
       cargo run --release -- --bill-path "./csv_data/202411"
       # current bill - older bill
       cargo run --release -- --bill-prev-subtract-path ./csv_data/*202410*.csv
       #
       cargo run --release -- --subscription "torpedo7" --bill-prev-subtract-path ./csv_data/*202409*.csv
       cargo run --release -- --resource-group dev-tui-rg --meter-category ".*" --name-regex ".*"
       #
       ./bill_analysis.rs --bill-path ./csv_data/Detail_Enrollment_70785102_202405_en.csv resource-price --subscription ".*7$" # Prod only
       #
       ./bill_analysis.rs --bill-path ./csv_data/Detail_Enrollment_70785102_202405_en.csv resource-price --subscription ".*7.*Non-Prod"
       # All VM's in Nonprod subscription
       cargo run --release --   --meter-category "Virtual Machines" --subscription "non-"
       # No command
       ./bill_analysis.rs  --subscription "Torpedo7" --bill-path ./csv_data/Detail_Enrollment_70785102_202405_en.csv
       # Find AKS RG's cost breakdown
       ./bill_analysis.rs --bill-path ./csv_data/Detail_Enrollment_70785102_202404_en.csv --resource-group="^MC"
       # Remove all previous month entries - only view new
       ./bill_analysis.rs --bill-path ./csv_data/Detail_Enrollment_70785102_202410_en.csv --bill-prev-subtract-path ./csv_data/Detail_Enrollment_70785102_202409_en.csv --resource-group ".*"
       #

### Command resource-price

* ```disk-csv-savings``` Takes csv or txt file of disk names and does lookup in latest bill printing the cost for each disk.

       cargo run -- disk-csv-savings --diskfile ../Azuredisks-Jira-PPD-29997.txt

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
  * Resources not coverd by reservations - compute & sql
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