pub mod az_disk;
pub mod blob_source;
pub mod bills;
pub mod money;
use bills::Bills;
use colored::Colorize;
pub mod cmd_parse;
pub mod find_files;
pub use bills::repository::BillRepository;
use std::{
    f64,
    path::{Path, PathBuf},
};

use cmd_parse::FilterOpts;

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
pub fn calc_subscription_cost(
    subscription: &str,
    file_or_folder: &Path,
    filter_opts: &FilterOpts,
    debug: bool,
) {
    println!("Calculating Azure subscription:\"{subscription}\" cost from csv export.\n");
    let (latest_bill, bill_file_name) = load_latest_bill(file_or_folder, filter_opts, debug);
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

fn load_latest_bill(
    file_or_folder: &Path,
    filter_opts: &FilterOpts,
    debug: bool,
) -> (Bills, String) {
    let resolved = find_files::resolve_date_shorthand(file_or_folder);
    let file_or_folder = resolved.as_path();
    let file_bill: PathBuf = if file_or_folder.is_file() {
        file_or_folder.to_path_buf()
    } else {
        let (path, files) = find_files::in_folder(file_or_folder, r".*Detail.*\.csv$", debug);
        path.join(files.last().expect("No files found"))
    };
    println!("Loading bill from '{:?}'", file_bill);
    let mut latest_bill: Bills = Bills::default();
    latest_bill
        .parse_csv(&file_bill, filter_opts)
        .unwrap_or_else(|_| panic!("Error parsing the file '{:?}'", file_bill));
    //
    (
        latest_bill,
        file_bill.file_name().unwrap().to_str().unwrap().to_string(),
    )
}

pub fn calc_disks_cost(
    file_disk: PathBuf,
    file_or_folder: &Path,
    filter_opts: &FilterOpts,
    debug: bool,
) {
    println!("Calculating Azure disk cost from csv export.\n");
    let disks = az_disk::AzDisks::parse(&file_disk)
        .unwrap_or_else(|_| panic!("Error parsing the file '{:?}'", file_disk));
    let (latest_bill, file_name_bill) = load_latest_bill(file_or_folder, filter_opts, debug);
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

pub fn load_bill(file_or_folder: &Path, filter_opts: &FilterOpts, debug: bool) -> (Bills, String) {
    let (latest_bill, file_name) = load_latest_bill(file_or_folder, filter_opts, debug);
    (latest_bill, file_name)
}

/// Like `load_bill`, but falls back to Azure Blob Storage when the path cannot
/// be resolved to a local CSV file.
///
/// The blob fallback is active only when all three `AZ_BILLING_BLOB_*` env vars
/// are set and the path looks like a date shorthand (`YYYY-MM` or `YYYYMM`).
pub async fn load_bill_async(
    file_or_folder: &Path,
    filter_opts: &FilterOpts,
    debug: bool,
) -> (Bills, String) {
    // Attempt to resolve to a local file first.
    let resolved = find_files::resolve_date_shorthand(file_or_folder);
    if resolved.exists() {
        return load_bill(&resolved, filter_opts, debug);
    }

    // No local file — try blob storage if configured and path is a date shorthand.
    if let Some(cfg) = blob_source::BlobSourceConfig::from_env() {
        // Parse from the resolved value so bare month shorthand (e.g. "03") works,
        // even when it maps to a year-month string only during resolution.
        if let Some((year, month)) = find_files::parse_year_month_path(&resolved) {
            let month_str = format!("{year}-{month:02}");
            log::info!("[bill] no local file for {month_str}, trying blob storage");
            let blob = blob_source::BlobSource::new(cfg)
                .unwrap_or_else(|e| panic!("Failed to create blob source: {e}"));
            let bills = blob
                .load_bills_for_month(year, month, filter_opts)
                .await
                .unwrap_or_else(|e| {
                    panic!("Failed to load bill from blob for {month_str}: {e}")
                });
            return (bills, month_str);
        }
    }

    // Fall back to the sync path — will surface the original error if the path is invalid.
    load_bill(file_or_folder, filter_opts, debug)
}

pub fn display_total_cost_summary(bills: &Bills, description: &str) {
    println!(
        "\n===  Displaying Azure cost summary.  {description} {} ===",
        bills.file_short_name
    );
    let t_cost_nzd = &bills.summary.total_cost;
    let t_cost_usd = &bills.summary.total_cost_usd;
    let exchange_rate = bills.summary.exchange_rate;
    let gst_rate = 0.15_f64;
    let tax_nzd = t_cost_nzd.amount() * gst_rate;
    let total_incl_tax = t_cost_nzd.amount() * (1.0 + gst_rate);
    let t_sav_used = bills.total_used_savings();
    let t_sav_unused = bills.total_unused_savings();
    println!(
        "Total cost {t_cost_nzd}  ({t_cost_usd})  [savings approx.: res_save {t_sav_used} + res_unused {t_sav_unused}]",
        t_cost_nzd = format!("{t_cost_nzd}").red().bold(),
        t_cost_usd = format!("{t_cost_usd}").bold(),
        t_sav_used = format!("{t_sav_used}").yellow(),
        t_sav_unused = format!("{t_sav_unused}").on_red(),
    );
    println!(
        "  GST (15%)  NZ$ {tax}  →  Total incl. GST  NZ$ {total_incl}",
        tax = f64_to_currency(tax_nzd, 2).yellow(),
        total_incl = f64_to_currency(total_incl_tax, 2).red().bold(),
    );
    if exchange_rate > 0.0 {
        println!(
            "  Exchange rate  1 USD = {rate:.10} NZD  (derived from costInBillingCurrency / costInUsd)",
            rate = exchange_rate,
        );
    }
    // TODO: print filtered total cost

    // print details of the savings
    let savings_all = bills.savings_all_categories();
    let mut total_savings = crate::money::Usd::default();
    let mut total_unused_savings = crate::money::Usd::default();
    for meter_category in savings_all.keys() {
        let (savings, unused_savings) = savings_all[meter_category];
        total_savings += savings;
        total_unused_savings += unused_savings;
        if savings.amount().abs() < 0.01 && unused_savings.amount().abs() < 0.01 {
            continue;
        }
        println!(
            "  Savings by meter_category:{meter_category:>32} {savings:>14} and Unused {unused_savings}",
            meter_category = format!("'{}'", meter_category),
            savings = format!("{savings}").yellow(),
            unused_savings = format!("{unused_savings}").red(),
        );
    }
    println!(
        "  Savings Total {total_savings} Unused {total_unused_savings}",
        total_savings = format!("{total_savings}").yellow(),
        total_unused_savings = format!("{total_unused_savings}").red(),
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

#[cfg(test)]
mod tests {
    use super::BillRepository;

    #[test]
    fn bill_repository_accessible_from_crate_root() {
        // BillRepository must be re-exported at the crate root so both the CLI
        // and external consumers can use `bill_analysis::BillRepository` without
        // knowing the internal module path.
        let _: fn(_, _) -> BillRepository = BillRepository::new;
    }
}
