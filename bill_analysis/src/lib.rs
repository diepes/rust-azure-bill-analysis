pub mod az_disk;
pub mod bill;
use bill::billentry::BillEntry;
use bill::bills::Bills;
use colored::Colorize;
pub mod cmd_parse;
pub mod find_files;
use std::path::{Path, PathBuf};

use cmd_parse::GlobalOpts;

// function calc_subscription_cost
pub fn calc_subscription_cost(subscription: &str, file_or_folder: &Path, global_opts: &GlobalOpts) {
    println!("Calculating Azure subscription:\"{subscription}\" cost from csv export.\n");
    let (latest_bill, bill_file_name) = load_latest_bill(file_or_folder, global_opts);
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

fn load_latest_bill(file_or_folder: &Path, global_opts: &GlobalOpts) -> (Bills, String) {
    let file_bill: PathBuf = if file_or_folder.is_file() {
        file_or_folder.to_path_buf()
    } else {
        let (path, files) = find_files::in_folder(
            file_or_folder,
            r"Detail_Enrollment_70785102_.*_en.csv",
            global_opts,
        );
        path.join(files.last().unwrap())
    };
    println!("Loading bill from '{:?}'", file_bill);
    let latest_bill = BillEntry::parse_csv(&file_bill, global_opts)
        .expect(&format!("Error parsing the file '{:?}'", file_bill));
    (
        latest_bill,
        file_bill.file_name().unwrap().to_str().unwrap().to_string(),
    )
}

pub fn calc_disks_cost(file_disk: PathBuf, file_or_folder: &Path, global_opts: &GlobalOpts) {
    println!("Calculating Azure disk cost from csv export.\n");
    let disks = az_disk::AzDisks::parse(&file_disk)
        .expect(&format!("Error parsing the file '{:?}'", file_disk));
    let (latest_bill, file_name_bill) = load_latest_bill(file_or_folder, global_opts);
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
    println!("    from file '{:?}'", file_name_bill);
    println!("Total cost {cur} {total_cost:.2}");
}

pub fn load_bill(file_or_folder: &Path, global_opts: &GlobalOpts) -> (Bills, String) {
    let (latest_bill, file_name) = load_latest_bill(file_or_folder, global_opts);
    (latest_bill, file_name)
}

pub fn display_total_cost_summary(bills: &Bills, description: &str, _global_opts: &GlobalOpts) {
    println!(
        "\n===  Displaying Azure cost summary.  {description} {} ===",
        bills.file_short_name
    );
    let cur = bills.get_billing_currency();
    println!("Total cost {cur} {t_cost}, no_reservation {cur} {t_no_reservation}, Unused Savings {cur} {t_unused_savings}, Used Savings {cur} {t_used_savings}",
        t_cost = f64_to_currency(bills.total_effective(),2),
        t_no_reservation = f64_to_currency(bills.total_no_reservation(),2),
        t_unused_savings = f64_to_currency(bills.total_unused_savings(), 2).on_red(),
        t_used_savings = f64_to_currency(bills.total_used_savings(), 2).on_green(),
    );
    let category = "Virtual Machines";
    println!(
        "Savings '{category}' {cur} {savings}",
        category = category,
        cur = cur,
        savings = f64_to_currency(bills.savings(category), 2),
    );
    let category = "Azure App Service";
    println!(
        "Savings '{category}' {cur} {savings}",
        category = category,
        cur = cur,
        savings = f64_to_currency(bills.savings(category), 2),
    );
    println!();
}

fn f64_to_currency(value: f64, decimal_places: usize) -> String {
    // Format to the specified number of decimal places
    let formatted_value = format!("{:.*}", decimal_places, value.abs()); // Use absolute value for formatting

    // Split integer and decimal parts
    let parts: Vec<&str> = formatted_value.split('.').collect();
    let integer_part = parts[0];
    let decimal_part = if parts.len() > 1 { parts[1] } else { "" };

    // Insert commas into the integer part
    let mut formatted_integer = String::new();
    for (i, c) in integer_part.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            formatted_integer.push(',');
        }
        formatted_integer.push(c);
    }

    // Reverse back to correct order
    let formatted_integer: String = formatted_integer.chars().rev().collect();

    // Pad the decimal part to the specified number of decimal places
    let padded_decimal = format!("{:0<width$}", decimal_part, width = decimal_places);

    // Combine integer and decimal parts
    let result = if decimal_places > 0 {
        format!("{}.{}", formatted_integer, padded_decimal)
    } else {
        formatted_integer
    };

    // Add back the negative sign if the original value was negative
    if value < 0.0 {
        format!("-{}", result)
    } else {
        result
    }
}
