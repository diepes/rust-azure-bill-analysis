use bill_analysis::az_disk::{AzDisk, AzDisks};
use bill_analysis::bill::{BillEntry, Bills};
use bill_analysis::cmd_parse::App;
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
        Commands::DiskPrice(args) => {
            println!("Running DiskPrice command with diskpath: {:?}", args);
            bill_analysis::calc_disk_cost(args.diskfile, app.global_opts.billpath.unwrap());
        }
    }
}
