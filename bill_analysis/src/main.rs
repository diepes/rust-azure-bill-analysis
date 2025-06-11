// use bill_analysis::az_disk::{AzDisk, AzDisks};
// use bill_analysis::bill::{BillEntry, Bills};
//use bill_analysis::cmd_parse::App;
use bill_analysis::bills;
use bill_analysis::cmd_parse::Commands;
use clap::Parser; // Add this line to import the `Parser` trait from the `clap` crate
// use bill_analysis::calc_bill_summary; // Import the function if it exists

fn main() {
    let timer_run = std::time::Instant::now();
    let app = bill_analysis::cmd_parse::App::parse();
    let debug: bool = if app.global_opts.debug {
        println!("Debug mode activated {:?}", app.command);
        true
    } else {
        //println!("Debug mode not activated {:?}", app.command);
        false
    };
    match app.command {
        Some(Commands::BillSummary(args)) => {
            println!("Running BillSummary command {:?}", args);
            // bill_analysis::calc_bill_summary(&bill_path, &app.global_opts);
            let bill_path = app.global_opts.bill_path.clone().unwrap();
            let (mut latest_bill, _file_name) = bill_analysis::load_bill(&bill_path, &app.global_opts);
            latest_bill.summary(&bill_path, &app.global_opts);
        }
        Some(Commands::DiskCsvSavings(args)) => {
            bill_analysis::calc_disks_cost(
                args.diskfile,
                &app.global_opts
                    .bill_path
                    .clone()
                    .expect("Bill path not set ?"),
                &app.global_opts,
            );
        }
        None => {
            if debug {
                println!("No command specified #1 {:?}", app);
                println!("No command specified #2 {:?}", app.name_regex);
            }
            // Read latest_bill from file_name csv file.
            let bill_path = app.global_opts.bill_path.clone().unwrap();
            let (latest_bill, file_name) = bill_analysis::load_bill(&bill_path, &app.global_opts);
            println!("Loaded latest bill from '{:?}'", file_name);
            bill_analysis::display_total_cost_summary(
                &latest_bill,
                "Latest bill",
                &app.global_opts,
            );
            // If set read previous bill and subtract it from latest bill
            let previous_bill: Option<bills::Bills> = if let Some(
                ref bill_prev_subtract_path,
            ) =
                app.global_opts.bill_prev_subtract_path
            {
                let (prev_bill, prev_file_name) =
                    bill_analysis::load_bill(bill_prev_subtract_path, &app.global_opts);
                if prev_bill.get_billing_currency() != latest_bill.get_billing_currency() {
                    panic!("Currency mismatch between bills");
                }
                println!(
                    "Removing previous bill from latest bill '{:?}' (Filter matching resource ID's)",
                    prev_file_name
                );
                bill_analysis::display_total_cost_summary(
                    &prev_bill,
                    "Previous bill",
                    &app.global_opts,
                );
                // latest_bill.remove(prev_bill);
                // bill_analysis::display_total_cost_summary(
                //     &latest_bill,
                //     "Latest bill - Previous bill (Id's matched)",
                //     &app.global_opts,
                // );
                Some(prev_bill)
            } else {
                None
            };
            // Display latest_bill ( - previous bill if set)
            // using regex filters if set
            bill_analysis::bills::display::display_cost_by_filter(
                app.name_regex,
                app.resource_group,
                app.subscription,
                app.meter_category,
                app.location,
                app.reservation,
                app.tag_summarise,
                app.tag_filter,
                latest_bill,
                previous_bill,
                &app.global_opts,
            )
        }
    }
    println!(
        "Total time to run: {:.3}s",
        timer_run.elapsed().as_secs_f64()
    );
}
