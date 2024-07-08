// use bill_analysis::az_disk::{AzDisk, AzDisks};
// use bill_analysis::bill::{BillEntry, Bills};
//use bill_analysis::cmd_parse::App;
use bill_analysis::cmd_parse::Commands;
use clap::Parser; // Add this line to import the `Parser` trait from the `clap` crate

fn main() {
    let app = bill_analysis::cmd_parse::App::parse();
    if app.global_opts.debug {
        println!("Debug mode activated {:?}", app.command);
    } else {
        println!("Debug mode not activated {:?}", app.command);
    }
    match app.command {
        Some(Commands::BillSummary(args)) => {
            println!("Running BillSummary command {:?}", args);
            bill_analysis::calc_bill_summary(args.billpath);
        }
        Some(Commands::ResourcePrice(args)) => {
            println!("Running '--resource-price' command with args: {:?}", args);
            if let Some(resource_group) = args.resource_group {
                println!("Resource group: {:?}", resource_group);
                bill_analysis::calc_resource_group_cost(
                    &resource_group,
                    app.global_opts.bill_path.unwrap(),
                );
            } else if let Some(subscription) = args.subscription {
                println!("Subscription: {:?}", subscription);
                bill_analysis::calc_subscription_cost(
                    &subscription,
                    app.global_opts.bill_path.unwrap(),
                );
            } else if let Some(name_regex) = args.name_regex {
                println!("Resource name: {:?}", name_regex);
                bill_analysis::cost_by_resource_name_regex(
                    &name_regex,
                    app.global_opts.bill_path.unwrap(),
                );
            }
        }
        Some(Commands::DiskCsvSavings(args)) => {
            bill_analysis::calc_disks_cost(args.diskfile, app.global_opts.bill_path.unwrap());
        }
        None => {
            println!("No command specified #1 {:?}", app);
            println!("No command specified #2 {:?}", app.name_regex);
            // Read latest_bill from file_name csv file.
            let (mut latest_bill, file_name) =
                bill_analysis::load_bill(app.global_opts.bill_path.unwrap());
            println!("Loaded latest bill from '{:?}'", file_name);
            bill_analysis::display_total_cost_summary(&latest_bill, "Latest bill");
            // If set read previous bill and subtract it from latest bill
            if let Some(bill_prev_subtract_path) = app.global_opts.bill_prev_subtract_path {
                let (prev_bill, prev_file_name) = bill_analysis::load_bill(bill_prev_subtract_path);
                if prev_bill.get_billing_currency() != latest_bill.get_billing_currency() {
                    panic!("Currency mismatch between bills");
                }
                println!(
                    "Removing previous bill from latest bill '{:?}' (Filter matching resource ID's)",
                    prev_file_name
                );
                bill_analysis::display_total_cost_summary(&prev_bill, "Previous bill");
                latest_bill.remove(prev_bill);
                bill_analysis::display_total_cost_summary(&latest_bill, "Latest bill - Previous bill (Id's matched)");
            }
            // Display latest_bill ( - previous bill if set)
            // using regex filters if set
            bill_analysis::display_cost_by_filter(
                app.name_regex,
                app.resource_group,
                app.subscription,
                app.meter_category,
                latest_bill,
                app.global_opts.cost_min_display,
            )
        }
    }
}
