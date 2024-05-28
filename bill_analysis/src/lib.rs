pub mod az_disk;
pub mod bill;
pub mod cmd_parse;
pub mod find_files;
use std::path::PathBuf;

pub fn calc_resource_group_cost(resource_group: &str, file_or_folder: PathBuf) {
    println!("Hello, world!! Calculating Azure rg:{resource_group} cost from csv export.\n");
    let (latest_bill, _file_name) = load_latest_bill(file_or_folder);
    println!();
    // now that we have latest_bill and disks, lookup disk cost in latest_bill
    // and print the cost
    let cur = latest_bill.get_billing_currency();
    let mut total_cost: f64 = 0.0;
    let (rg_cost, rgs) = latest_bill.cost_by_resource_group(resource_group);
    println!("cost {cur} {rg_cost:7.2} - rg: '{resource_group:?}' ");
    total_cost += rg_cost;
    println!("Total cost {cur} {total_cost:.2} rg's:{:?}", rgs);
}
// function calc_subscription_cost
pub fn calc_subscription_cost(subscription: &str, file_or_folder: PathBuf) {
    println!("Hello, world!! Calculating Azure subscription:{subscription} cost from csv export.\n");
    let (latest_bill, bill_file_name) = load_latest_bill(file_or_folder);
    println!();
    // now that we have latest_bill and disks, lookup disk cost in latest_bill
    // and print the cost
    let cur = latest_bill.get_billing_currency();
    let mut total_cost: f64 = 0.0;
    let (sub_cost, subs) = latest_bill.cost_by_subscription(subscription);
    println!("cost {cur} {sub_cost:7.2} - subscription: '{subscription:?}' ");
    total_cost += sub_cost;
    println!("    from file '{:?}'", bill_file_name);
    println!("Total cost {cur} {total_cost:.2} subs:{:?}", subs);
}

fn load_latest_bill(file_or_folder: PathBuf) -> (bill::Bills, String) {
    let file_bill: PathBuf;
    if file_or_folder.is_file() {
        file_bill = file_or_folder;
    } else {
        let files = find_files::in_folder(&file_or_folder, r"Detail_Enrollment_70785102_.*_en.csv");
        file_bill = file_or_folder.join(files.last().unwrap());
    }
    let latest_bill = bill::BillEntry::parse_csv(&file_bill)
        .expect(&format!("Error parsing the file '{:?}'", file_bill));
    (latest_bill, file_bill.file_name().unwrap().to_str().unwrap().to_string())
}

pub fn calc_disks_cost(file_disk: PathBuf, file_or_folder: PathBuf) {
    println!("Hello, world!! Calculating Azure disk cost from csv export.\n");
    let disks = az_disk::AzDisks::parse(&file_disk)
        .expect(&format!("Error parsing the file '{:?}'", file_disk));
    let (latest_bill, file_name_bill) = load_latest_bill(file_or_folder);
    println!();
    println!(
        "Read {len_disk:?} records from '{f_disk}' and {len_bill:?} records from '{f_bill}'",
        len_disk = disks.len(),
        f_disk = file_disk.file_name().unwrap().to_str().unwrap(),
        len_bill = latest_bill.len(),
        f_bill = file_name_bill,
    );
    // now that we have latest_bill and disks, lookup disk cost in latest_bill
    // and print the cost
    let cur = latest_bill.get_billing_currency();
    let mut total_cost: f64 = 0.0;
    for disk in &disks.disks {
        let disk_cost = latest_bill.cost_by_resource_name(&disk.name);
        println!("cost {cur} {disk_cost:7.2} - disk: {:?} ", disk.name);
        total_cost += disk_cost;
    }
    println!("Total cost {cur} {total_cost:.2}");
}

pub fn calc_bill_summary(folder: PathBuf) {
    println!("Hello, world!! Calculating Azure savings form Amortized charges csv export.\n");
    //let folder = app.global_opts.billpath.unwrap();
    let files = find_files::in_folder(&folder, r"Detail_Enrollment_70785102_.*_en.csv");
    println!("Found {:?} csv files.", files.len());
    for csv_file_name in files {
        // combine folder and csv_file_name into file_path
        //let file_path = format!("{:?}/{}", folder, csv_file_name);
        let file_path = folder.join(csv_file_name);
        let bills = bill::BillEntry::parse_csv(&file_path)
            .expect(&format!("Error parsing the file '{:?}'", file_path));
        println!();
        println!(
            "Read {len:?} records from '{f_name}'",
            len = bills.len(),
            f_name = file_path.file_name().unwrap().to_str().unwrap(),
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
