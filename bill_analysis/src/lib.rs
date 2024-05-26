pub mod az_disk;
pub mod bill;
pub mod cmd_parse;
pub mod find_files;
use std::path::PathBuf;

pub fn calc_disk_cost(file_disk: PathBuf, folder: PathBuf) {
    println!("Hello, world!! Calculating Azure disk cost from csv export.\n");
    let disks = az_disk::AzDisks::parse_csv(&file_disk)
        .expect(&format!("Error parsing the file '{:?}'", file_disk));
    let files = find_files::in_folder(&folder, r"Detail_Enrollment_70785102_.*_en.csv");
    let file_path = folder.join(files.last().unwrap());
    let latest_bill = bill::BillEntry::parse_csv(&file_path)
        .expect(&format!("Error parsing the file '{:?}'", file_path));
    println!();
    println!(
        "Read {len_disk:?} records from '{f_disk}' and {len_bill:?} records from '{f_bill}'",
        len_disk = disks.len(),
        f_disk = file_disk.file_name().unwrap().to_str().unwrap(),
        len_bill = latest_bill.len(),
        f_bill = file_path.file_name().unwrap().to_str().unwrap(),
    );
    // now that we have latest_bill and disks, lookup disk cost in latest_bill
    // and print the cost
    let cur = latest_bill.get_billing_currency();
    for disk in &disks.disks {
        //let cost = latest_bill.cost(&disk.resource_group, &disk.resource_name);
        println!("Cost");
    }
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
