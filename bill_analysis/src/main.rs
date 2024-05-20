use bill_analysis::az_disk::{AzDisk, AzDisks};
use bill_analysis::bill::{Bill, Bills};
use std::path;

fn main() {
    println!("Hello, world!! Calculating Azure savings form Amortized charges csv export.\n");
    let folder = "csv_data";
    let files =
        bill_analysis::find_files::in_folder(&folder, r"Detail_Enrollment_70785102_.*_en.csv");
    println!("Found {:?} csv files.", files.len());
    for csv_file_name in files {
        // combine folder and csv_file_name into file_path
        let file_path = format!("{}/{}", folder, csv_file_name);
        let bills: Bills = Bill::parse_csv(&file_path)
            .expect(&format!("Error parsing the file {}", csv_file_name));
        println!();
        println!(
            "Read {len:?} records from '{f_name}'",
            len = bills.len(),
            f_name = csv_file_name.split(path::is_separator).last().unwrap(),
        );
        //println!("{:?}", bills[0]);
        let cur = bills.get_billing_currency();
        println!(
            "Total no_reservation {:.2} {cur}  -  Total effective {:.2} {cur}  = {savings:.2} {cur} Savings/month {save_percent:.1}% . [Unused Savings: {unused:.2} {cur}]",
            bills.total_no_reservation(),
            bills.total_effective(),
            savings = bills.total_no_reservation() - bills.total_effective(),
            save_percent = (bills.total_no_reservation() - bills.total_effective()) / bills.total_no_reservation() * 100.0,
            unused = bills.total_unused_savings(),
        );
        print!("Total Used Savings {:.2} {cur}", bills.total_used_savings());
        let category = "Virtual Machines";
        print!("Savings '{category}' {:.2} {cur}", bills.savings(category));
        let category = "Azure App Service";
        print!("Savings '{category}' {:.2} {cur}", bills.savings(category));
        println!();
    }
}
