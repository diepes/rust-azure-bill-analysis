use bill_analysis::bill::{Bill, Bills};
use std::path;

fn main() {
    println!("Hello, world!! Calculating Azure savings form Amortized charges csv export.");
    // let csv_file_name = "tests/azure_test_data_01.csv";
    for date in &[
        "202306", "202307", "202308", "202309", "202310", "202311", "202312", "202401", "202402",
        "202403", "202404",
    ] {
        let csv_file_name = format!(
            "/Users/324157/Documents/TWG-Azure/Cost/Detail_Enrollment_70785102_{date}_en.csv"
        );
        let bills: Bills = Bill::parse_csv(&csv_file_name).unwrap();
        println!();
        println!(
            "{date}: Num of records in csv {len:?}  '{f_name}'",
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
        print!("Savings '{category}' {:.2} {cur}", bills.savings(&category));
        let category = "Azure App Service";
        print!("Savings '{category}' {:.2} {cur}", bills.savings(&category));
        println!();
    }
}
