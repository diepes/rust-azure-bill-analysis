use crate::bills::Bills;
use crate::cmd_parse::GlobalOpts;
use crate::find_files;
use std::collections::HashMap;
use std::path::Path;

pub struct Summary {
    pub total_cost: f64,
    pub total_no_reservation: f64,
    pub total_effective: f64,
    pub total_savings_used: f64,
    pub total_savings_un_used: f64,
    pub total_savings_meter_category_map: HashMap<String, (f64, f64)>,
}

impl Bills {
    pub fn summary(&mut self, folder: &Path, global_opts: &GlobalOpts) {
        println!("Hello, world!! Calculating Azure savings form Amortized charges csv export.\n");
        //let folder = app.global_opts.billpath.unwrap();
        //TODO: file re_pattern should be commandline override arg.
        let (path, files) =
            find_files::in_folder(folder, r"Detail_Enrollment_70785102_.*_en.csv", global_opts);
        println!("Found {:?} csv files.", files.len());
        // Collect file paths first to avoid borrowing self across loop iterations
        let file_paths: Vec<_> = files.into_iter().map(|csv_file_name| path.join(csv_file_name)).collect();
        for file_path in file_paths {
            self.parse_csv(&file_path, global_opts)
                .expect(&format!("Error parsing the file '{:?}'", file_path));
            println!();
            println!(
                "Read {len:?} records from '{f_name}'",
                f_name = file_path.file_name().unwrap().to_str().unwrap(),
                len = self.len(),
            );
            //println!("{:?}", bills[0]);
            let cur = self.get_billing_currency();
            println!(
                "Total no_reservation {:.2} {cur}  -  Total effective {:.2} {cur}  = {savings:.2} {cur} Savings/month {save_percent:.1}% . [Unused Savings: {unused:.2} {cur}]",
                self.total_no_reservation(),
                self.total_effective(),
                savings = self.total_no_reservation() - self.total_effective(),
                save_percent = (self.total_no_reservation() - self.total_effective()) / self.total_no_reservation() * 100.0,
                unused = self.total_unused_savings(),
            );
            print!("Total Used Savings {:.2} {cur}", self.total_used_savings());
            let meter_category = "Virtual Machines";
            print!(
                "Savings '{meter_category}' {:.2} {cur}",
                self.savings(meter_category)
            );
            let meter_category = "Azure App Service";
            print!(
                "Savings '{meter_category}' {:.2} {cur}",
                self.savings(meter_category)
            );
            println!();
        }
    }
}
