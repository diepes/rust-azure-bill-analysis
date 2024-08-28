// https://rust-cli-recommendations.sunshowers.io/handling-arguments.html

use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

/// Here's my app!
#[derive(Debug, Parser)]
#[clap(name = "bill-analysis", version)]
pub struct App {
    #[clap(flatten)]
    pub global_opts: GlobalOpts,
    // Commands to run
    #[command(subcommand)]
    pub command: Option<Commands>,
    /// regex find to filter on specific resource name, if not using diskfile option.'$'
    #[arg(short, long)]
    pub name_regex: Option<String>,
    /// regex find to filter on resource group terminate with '$'
    #[arg(short, long)]
    pub resource_group: Option<String>,
    /// regex find to filter on subscriptions terminate with '$'
    #[arg(short, long)]
    pub subscription: Option<String>,
    /// regex find to filter on meter category terminate with '$'
    #[arg(short, long)]
    pub meter_category: Option<String>,
    /// regex find to filter on tag's terminate regex with '$'
    #[arg(short, long)]
    pub tag: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    BillSummary(BillSummaryArgs),
    ResourcePrice(ResourcePriceArgs),
    DiskCsvSavings(DiskCsvSavingsArgs),
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
    /// regex find to filter on specific resource name, if not using diskfile option.'$'
    #[arg(short, long)]
    pub name_regex: Option<String>,
    /// regex find to filter on resource group terminate with '$'
    #[arg(short, long)]
    pub resource_group: Option<String>,
    /// regex find to filter on subscriptions terminate with '$'
    #[arg(short, long)]
    pub subscription: Option<String>,
}
#[derive(Debug, Args)]
pub struct DiskCsvSavingsArgs {
    /// Path to find Azure disk's csv files
    #[arg(short, long, default_value = "../Azuredisks-Unattached-20240517.csv")]
    pub diskfile: PathBuf,
}
#[derive(Debug, Args)]
pub struct GlobalOpts {
    /// Activate debug mode
    #[arg(short, long)]
    pub debug: bool,
    #[arg(short, long, default_value = "csv_data")]
    pub bill_path: Option<PathBuf>,
    #[arg(long, default_value = None)]
    pub bill_prev_subtract_path: Option<PathBuf>,
    #[arg(short, long, default_value = "10.00")]
    pub cost_min_display: f64,
    #[arg(long, default_value = "false")]
    pub case_sensitive: bool,
}
