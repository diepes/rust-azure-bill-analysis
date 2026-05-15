pub mod az_disk;
pub mod bills;
pub mod money;
use bills::Bills;
use colored::Colorize;
pub mod cmd_parse;
pub mod find_files;
use std::{
    f64,
    path::{Path, PathBuf},
};

use cmd_parse::GlobalOpts;

// use once_cell::sync::Lazy;
// static RESERVATION_SUMMARY: Lazy<HashMap<&'static str, Vec<&'static str>>> = Lazy::new(|| {
//     let mut m = HashMap::new();
//     // k = meter_category to include, v = meter_sub_category to exclude
//     m.insert("Virtual Machines", vec![]);
//     m.insert("SQL Managed Instance", vec!["Storage"]);
//     m.insert("Azure App Service", vec![]);
//     m
// });

// function calc_subscription_cost
pub fn calc_subscription_cost(subscription: &str, file_or_folder: &Path, global_opts: &GlobalOpts) {
    println!("Calculating Azure subscription:\"{subscription}\" cost from csv export.\n");
    let (latest_bill, bill_file_name) = load_latest_bill(file_or_folder, global_opts);
    println!();
    // now that we have latest_bill and disks, lookup disk cost in latest_bill
    // and print the cost
    let mut total_cost = money::Nzd::default();
    let (sub_cost, subs) = latest_bill.cost_by_subscription(subscription);
    println!("cost {sub_cost} - subscription: '{subscription:?}' ");
    total_cost += sub_cost;
    println!("    from file '{:?}'", bill_file_name);
    println!("Total cost {total_cost} subs:{:?}", subs);
}

fn load_latest_bill(file_or_folder: &Path, global_opts: &GlobalOpts) -> (Bills, String) {
    let resolved = find_files::resolve_date_shorthand(file_or_folder);
    let file_or_folder = resolved.as_path();
    let file_bill: PathBuf = if file_or_folder.is_file() {
        file_or_folder.to_path_buf()
    } else {
        let (path, files) = find_files::in_folder(
            file_or_folder,
            r".*Detail.*\.csv$",
            global_opts,
        );
        path.join(files.last().expect("No files found"))
    };
    println!("Loading bill from '{:?}'", file_bill);
    let mut latest_bill: Bills = Bills::default();
    latest_bill
        .parse_csv(&file_bill, global_opts)
        .expect(&format!("Error parsing the file '{:?}'", file_bill));
    //
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
    let mut total_cost = money::Nzd::default();
    for disk in &disks.disks {
        let disk_cost = latest_bill.cost_by_resource_name(&disk.name);
        println!("cost {disk_cost} - disk: {:?} ", disk.name);
        total_cost += disk_cost;
    }
    println!("    from file '{:?}'", file_name_bill);
    println!("Total cost {total_cost}");
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
    let t_cost_nzd = &bills.summary.total_cost;
    let t_cost_usd = &bills.summary.total_cost_usd;
    let t_sav_used = bills.total_used_savings();
    let c_sav_used = f64_to_currency(t_sav_used, 2);
    let t_sav_unused = bills.total_unused_savings();
    let c_sav_unused = f64_to_currency(t_sav_unused, 2);
    println!(
        "Total cost {t_cost_nzd}  ({t_cost_usd})  [savings approx. USD: res_save US$ {c_sav_used} + res_unused US$ {c_sav_unused}]",
        t_cost_nzd = format!("{t_cost_nzd}").red().bold(),
        t_cost_usd = format!("{t_cost_usd}").bold(),
        c_sav_unused = c_sav_unused.on_red(),
        c_sav_used = c_sav_used.yellow(),
    );
    // TODO: print filtered total cost

    // print details of the savings
    let savings_all = bills.savings_all_categories();
    // let categorys = ["Virtual Machines","Azure App Service", "SQL Managed Instance"] ;
    let mut total_savings = 0.0;
    let mut total_unused_savings = 0.0;
    for meter_category in savings_all.keys() {
        // let savings = f64_to_currency(bills.savings(meter_category), 2);
        let (savings, unused_savings) = savings_all[meter_category];
        total_savings += savings;
        total_unused_savings += unused_savings;
        let c_savings = f64_to_currency(savings, 2);
        let c_unused_savings = f64_to_currency(unused_savings, 2);
        if savings.abs() < 0.01 && unused_savings.abs() < 0.01 {
            continue;
        }
        println!(
            "  Savings by meter_category:{meter_category:>32} US$ {savings:>10} and Unused US$ {unused_savings:<8}",
            meter_category = format!("'{}'", meter_category),
            savings = c_savings.yellow(),
            unused_savings = c_unused_savings.red(),
        );
    }
    println!(
        "  Savings Total US$ {total_savings} Unused {total_unused_savings}",
        total_savings = f64_to_currency(total_savings, 2).yellow(),
        total_unused_savings = f64_to_currency(total_unused_savings, 2).red(),
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
