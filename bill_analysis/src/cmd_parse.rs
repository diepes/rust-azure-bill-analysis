// https://rust-cli-recommendations.sunshowers.io/handling-arguments.html

use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

/// Here's my app!
#[derive(Debug, Parser)]
#[clap(name = "my-app", version)]
pub struct App {
    #[clap(flatten)]
    pub global_opts: GlobalOpts,
    // Commands to run
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    BillSummary(BillSummaryArgs),
    ResourcePrice(ResourcePriceArgs),
    // /// Number of times to greet
    // #[arg(short, long, default_value_t = 1)]
    // pub count: u8,
}
#[derive(Debug, Args)]
pub struct BillSummaryArgs {
    // /// Path to find Azure bill's csv files
    #[arg(short, long, default_value = "./csv_data/")]
    pub billpath: PathBuf,
    // // a list of other write args
}
#[derive(Debug, Args)]
pub struct ResourcePriceArgs {
    /// Path to find Azure disk's csv files
    #[arg(short, long, default_value = "../Azuredisks-Unattached-20240517.csv")]
    pub diskfile: PathBuf,
    #[arg(short, long)]
    pub resource_group: Option<String>,
    #[arg(short, long)]
    pub subscription: Option<String>,
}

#[derive(Debug, Args)]
pub struct GlobalOpts {
    /// Activate debug mode
    #[arg(short, long)]
    pub debug: bool,
    #[arg(short, long, default_value = "csv_data")]
    pub bill_path: Option<PathBuf>,
}
