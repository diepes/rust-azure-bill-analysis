use bill_analysis::bill::{Bill, Bills};

fn main() {
    println!("Hello, world!! Calculating Azure savings form Amortized charges csv export.");
    // let csv_file_name = "../Detail_Enrollment_70785102_202404_en.csv";
    // let csv_file_name = "tests/azure_test_data_01.csv";
    //let csv_file_name = "/Users/324157/Documents/TWG-Azure/Cost/Detail_Enrollment_70785102_202402_en.csv";
    let csv_file_name = "/Users/324157/Documents/TWG-Azure/Cost/Detail_Enrollment_70785102_202403_en.csv";
    //let csv_file_name = "/Users/324157/Documents/TWG-Azure/Cost/Detail_Enrollment_70785102_202404_en.csv";
    let bills: Bills = Bill::parse_csv(&csv_file_name).unwrap();
    println!("Num of records in csv {:?}", bills.len());
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
    println!("Total Used Savings {:.2} {cur}", bills.total_used_savings());
    let category = "Virtual Machines";
    println!("Savings '{category}' {:.2} {cur}", bills.savings(&category));
    let category = "Azure App Service";
    println!("Savings '{category}' {:.2} {cur}", bills.savings(&category));
}
