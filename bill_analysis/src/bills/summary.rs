use crate::bills::Bills;
use crate::cmd_parse::FilterOpts;
use crate::find_files;
use crate::money::{Nzd, Usd};
use std::collections::HashMap;
use std::path::Path;

pub struct Summary {
    pub total_cost: Nzd,
    pub total_cost_usd: Usd,
    pub exchange_rate: f64,       // pricing currency (USD) → billing currency (NZD)
    pub total_no_reservation: Usd,
    pub total_effective: Usd,
    pub total_savings_used: Usd,
    pub total_savings_un_used: Usd,
    pub total_savings_meter_category_map: HashMap<String, (Usd, Usd)>,
}

impl Bills {
    pub fn summary(&mut self, folder: &Path, filter_opts: &FilterOpts, debug: bool) {
        println!("Hello, world!! Calculating Azure savings form Amortized charges csv export.\n");
        let (path, files) =
            find_files::in_folder(folder, r"Detail_Enrollment_70785102_.*_en.csv", debug);
        println!("Found {:?} csv files.", files.len());
        // Collect file paths first to avoid borrowing self across loop iterations
        let file_paths: Vec<_> = files
            .into_iter()
            .map(|csv_file_name| path.join(csv_file_name))
            .collect();
        for file_path in file_paths {
            self.parse_csv(&file_path, filter_opts)
                .unwrap_or_else(|_| panic!("Error parsing the file '{:?}'", file_path));
            println!();
            println!(
                "Read {len:?} records from '{f_name}'",
                f_name = file_path.file_name().unwrap().to_str().unwrap(),
                len = self.len(),
            );
            //println!("{:?}", bills[0]);
        let no_res = self.total_no_reservation();
            let effective = self.total_effective();
            let savings = no_res - effective;
            let save_percent = if no_res.amount() != 0.0 {
                savings.amount() / no_res.amount() * 100.0
            } else {
                0.0
            };
            println!(
                "Total no_reservation {no_res}  -  Total effective {effective}  = {savings} Savings/month {save_percent:.1}% . [Unused Savings: {unused}]",
                unused = self.total_unused_savings(),
            );
            print!("Total Used Savings {}", self.total_used_savings());
            let meter_category = "Virtual Machines";
            print!("Savings '{meter_category}' {}", self.savings(meter_category));
            let meter_category = "Azure App Service";
            print!("Savings '{meter_category}' {}", self.savings(meter_category));
            println!();
        }
    }
}
