// use bill_analysis::az_disk::{AzDisk, AzDisks};
// use bill_analysis::bill::{BillEntry, Bills};
//use bill_analysis::cmd_parse::App;
use bill_analysis::bills;
use bill_analysis::cmd_parse::{Commands, DisplayOpts, FilterOpts};
use clap::Parser; // Add this line to import the `Parser` trait from the `clap` crate
// use bill_analysis::calc_bill_summary; // Import the function if it exists

fn main() {
    let timer_run = std::time::Instant::now();
    let app = bill_analysis::cmd_parse::App::parse();
    let debug: bool = if app.global_opts.debug {
        println!("Debug mode activated {:?}", app.command);
        true
    } else {
        false
    };
    let filter_opts = FilterOpts { case_sensitive: app.global_opts.case_sensitive };
    let display_opts = DisplayOpts {
        cost_min_display: app.global_opts.cost_min_display,
        tag_list: app.global_opts.tag_list,
        debug,
    };
    match app.command {
        Some(Commands::BillSummary(args)) => {
            println!("Running BillSummary command {:?}", args);
            let bill_path = app.global_opts.bill_path.clone()
                .unwrap_or_else(|| std::path::PathBuf::from(bill_analysis::find_files::last_month_shorthand()));
            let (mut latest_bill, _file_name) =
                bill_analysis::load_bill(&bill_path, &filter_opts, debug);
            latest_bill.summary(&bill_path, &filter_opts, debug);
        }
        Some(Commands::DiskCsvSavings(args)) => {
            bill_analysis::calc_disks_cost(
                args.diskfile,
                &app.global_opts
                    .bill_path
                    .clone()
                    .unwrap_or_else(|| std::path::PathBuf::from(bill_analysis::find_files::last_month_shorthand())),
                &filter_opts,
                debug,
            );
        }
        None => {
            if debug {
                println!("No command specified #1 {:?}", app);
                println!("No command specified #2 {:?}", app.name_regex);
            }
            let bill_path = app.global_opts.bill_path.clone()
                .unwrap_or_else(|| {
                    let default = bill_analysis::find_files::last_month_shorthand();
                    println!("No --bill-path specified, defaulting to last month: {default}");
                    std::path::PathBuf::from(default)
                });
            let (latest_bill, file_name) = bill_analysis::load_bill(&bill_path, &filter_opts, debug);
            println!("Loaded latest bill from '{:?}'", file_name);
            bill_analysis::display_total_cost_summary(
                &latest_bill,
                "Latest bill",
            );
            // If set read previous bill and subtract it from latest bill
            let previous_bill: Option<bills::Bills> = if let Some(ref bill_prev_subtract_path) =
                app.global_opts.bill_prev_subtract_path
            {
                let (prev_bill, prev_file_name) =
                    bill_analysis::load_bill(bill_prev_subtract_path, &filter_opts, debug);
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
                );
                Some(prev_bill)
            } else {
                None
            };
            // Display latest_bill ( - previous bill if set)
            // using regex filters if set
            let filter = bill_analysis::bills::BillFilter::new(
                app.name_regex,
                app.resource_group,
                app.subscription,
                app.meter_category,
                app.location,
                app.reservation,
                app.tag_summarise,
                app.tag_filter,
                app.invoice_section,
                &filter_opts,
            )
            .unwrap_or_else(|e| {
                eprintln!("Error: invalid regex in filter: {e}");
                std::process::exit(1);
            });
            bill_analysis::bills::display::display_cost_by_filter(
                &filter,
                latest_bill,
                previous_bill,
                &display_opts,
            )
        }
    }
    println!(
        "Total time to run: {:.3}s",
        timer_run.elapsed().as_secs_f64()
    );
}
