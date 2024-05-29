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
        Commands::BillSummary(args) => {
            println!("Running BillSummary command {:?}", args);
            bill_analysis::calc_bill_summary(args.billpath);
        }
        Commands::ResourcePrice(args) => {
            println!("Running '--resource-price' command with args: {:?}", args);
            if let Some(resource_group) = args.resource_group {
                println!("Resource group: {:?}", resource_group);
                bill_analysis::calc_resource_group_cost(
                    &resource_group,
                    app.global_opts.bill_path.unwrap(),
                );
            } else if 
                let Some(subscription) = args.subscription {
                println!("Subscription: {:?}", subscription);
                bill_analysis::calc_subscription_cost(
                    &subscription,
                    app.global_opts.bill_path.unwrap(),
                );
            
            }
        }
        Commands::DiskCsvSavings(args) => {
                bill_analysis::calc_disks_cost(args.diskfile, app.global_opts.bill_path.unwrap());
        }
    }
}
