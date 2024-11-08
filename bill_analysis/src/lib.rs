pub mod az_disk;
pub mod bill;
use bill::billentry::BillEntry;
use bill::bills::Bills;
use bill::costtype::CostType;
pub mod cmd_parse;
pub mod find_files;
use std::path::PathBuf;

use cmd_parse::GlobalOpts;

pub fn calc_resource_group_cost(
    resource_group: &str,
    file_or_folder: &PathBuf,
    global_opts: &GlobalOpts,
) {
    println!("Calculating Azure rg:\"{resource_group}\" cost from csv export.\n");
    let (latest_bill, _file_name) = load_latest_bill(&file_or_folder, &global_opts);
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
pub fn calc_subscription_cost(
    subscription: &str,
    file_or_folder: &PathBuf,
    global_opts: &GlobalOpts,
) {
    println!("Calculating Azure subscription:\"{subscription}\" cost from csv export.\n");
    let (latest_bill, bill_file_name) = load_latest_bill(&file_or_folder, &global_opts);
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

fn load_latest_bill(file_or_folder: &PathBuf, global_opts: &GlobalOpts) -> (Bills, String) {
    let file_bill: PathBuf;
    if file_or_folder.is_file() {
        file_bill = file_or_folder.clone();
    } else {
        let files = find_files::in_folder(&file_or_folder, r"Detail_Enrollment_70785102_.*_en.csv");
        file_bill = file_or_folder.join(files.last().unwrap());
    }
    let latest_bill = BillEntry::parse_csv(&file_bill, &global_opts)
        .expect(&format!("Error parsing the file '{:?}'", file_bill));
    (
        latest_bill,
        file_bill.file_name().unwrap().to_str().unwrap().to_string(),
    )
}

pub fn calc_disks_cost(file_disk: PathBuf, file_or_folder: PathBuf, global_opts: &GlobalOpts) {
    println!("Calculating Azure disk cost from csv export.\n");
    let disks = az_disk::AzDisks::parse(&file_disk)
        .expect(&format!("Error parsing the file '{:?}'", file_disk));
    let (latest_bill, file_name_bill) = load_latest_bill(&file_or_folder, &global_opts);
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

pub fn cost_by_resource_name_regex(
    name_regex: &str,
    file_or_folder: &PathBuf,
    global_opts: &GlobalOpts,
) {
    println!("Calculating Azure cost from csv export regex \"{name_regex}\".\n");
    let (latest_bill, _file_name) = load_latest_bill(&file_or_folder, &global_opts);
    println!();
    // now that we have latest_bill and disks, lookup disk cost in latest_bill
    // and print the cost
    let cur = latest_bill.get_billing_currency();
    let mut total_cost: f64 = 0.0;
    let (item_cost, details) = latest_bill.cost_by_resource_name_regex(name_regex);
    println!("cost {cur} {item_cost:7.2} - regex:'{name_regex:?}' ");
    total_cost += item_cost;
    if details.len() < 4 {
        println!(" details: {:?}", details);
    } else {
        println!(" details: len={}", details.len());
        for d in details.iter() {
            println!(" details: {:?}", d);
        }
    }
    println!("Total cost {cur} {total_cost:.2}");
}

pub fn load_bill(file_or_folder: &PathBuf, global_opts: &GlobalOpts) -> (Bills, String) {
    let (latest_bill, file_name) = load_latest_bill(&file_or_folder, &global_opts);
    (latest_bill, file_name)
}

pub fn display_total_cost_summary(
    bills: &Bills,
    description: &str,
    _global_opts: &GlobalOpts,
) {
    println!("\n===  Displaying Azure cost summary.  {description} ===");
    let cur = bills.get_billing_currency();
    println!("Total cost {cur} {t_cost:.2}, no_reservation {cur} {t_no_reservation:.2}, Unused Savings {cur} {t_unused_savings:.2}, Used Savings {cur} {t_used_savings:.2}",
        t_cost = bills.total_effective(),
        t_no_reservation = bills.total_no_reservation(),
        t_unused_savings = bills.total_unused_savings(),
        t_used_savings = bills.total_used_savings()
    );
    let category = "Virtual Machines";
    println!(
        "Savings '{category}' {cur} {savings:.2}",
        category = category,
        cur = cur,
        savings = bills.savings(category)
    );
    let category = "Azure App Service";
    println!(
        "Savings '{category}' {cur} {savings:.2}",
        category = category,
        cur = cur,
        savings = bills.savings(category)
    );
    println!();
}

/// Display cost summary.
pub fn display_cost_by_filter(
    name_r: Option<String>,
    rg_r: Option<String>,
    sub_r: Option<String>,
    cat_r: Option<String>,
    tag_summarize: Option<String>,
    tag_filter: Option<String>,
    // file_or_folder: PathBuf,
    latest_bill: Bills,
    // global_opts.case_sensitive: bool,
    global_opts: &GlobalOpts,
) {
    println!();
    println!("Filter Azure name_r:{name_r:?}, rg_r:{rg_r:?}, sub_r:{sub_r:?}, cat_r:{cat_r:?}, tag_r:{tag_filter:?}, tag_s:{tag_summarize:?}.\n");
    // now that we have latest_bill and disks, lookup disk cost in latest_bill
    // and print the cost
    let cur = latest_bill.get_billing_currency();
    let s_name = name_r.unwrap_or("".to_string());
    let s_rg = rg_r.unwrap_or("".to_string());
    let s_sub = sub_r.unwrap_or("".to_string());
    let s_cat = cat_r.unwrap_or("".to_string());
    let s_tag_s = tag_summarize.clone().unwrap_or("".to_string());
    let s_tag_r = tag_filter.unwrap_or("".to_string());

    let (total_cost, details, bill_details) = latest_bill.cost_by_any(
        &s_name,
        &s_rg,
        &s_sub,
        &s_cat,
        &s_tag_s,
        &s_tag_r,
        &global_opts,
    );

    if s_name.len() > 0 {
        println!(" details: len={}", details.len());
        for d in details.iter() {
            println!(" details: {:?}", d);
        }
    }

    // print Subscription bill details
    println!("## Subscription bill details {}", s_sub);
    print_summary(
        &bill_details,
        &cur,
        CostType::Subscription,
        global_opts,
    );
    println!();
    // print ResourceGroup bill details
    println!("## ResourceGroup bill details");
    print_summary(
        &bill_details,
        &cur,
        CostType::ResourceGroup,
        global_opts,
    );
    println!();
    // print Resource bill details
    if s_name.len() > 0 {
        print_summary(
            &bill_details,
            &cur,
            CostType::ResourceName,
            global_opts,
        );
    }

    // print Category bill details
    if s_cat.len() > 0 {
        print_summary(
            &bill_details,
            &cur,
            CostType::MeterCategory,
            &global_opts,
        );
        println!()
    }

    // print Tag bill details
    if s_tag_s.len() > 0 {
        println!("## Tag details");
        print_summary(&bill_details, &cur, CostType::Tag, &global_opts);
        println!();
    }

    println!("Total cost {cur} {total_cost:.2}");

    if global_opts.tag_list {
        println!();
        println!(
            "Tags: {}\n{:?}",
            latest_bill.tag_names.len(),
            latest_bill.tag_names
        );
    }
}

/// print_summary for Subscription, ResourceGroup, ResourceName, MeterCategory
fn print_summary(
    bill_details: &std::collections::HashMap<(CostType, String), f64>,
    cur: &str,
    cost_type: CostType,
    global_opts: &GlobalOpts,
) {
    let mut total = 0.0;
    let mut cnt = 0;
    // create Vec from HashMap for specific CostType
    let mut bill_details_sorted: Vec<(f64, &str)> = bill_details
        .iter()
        .filter_map(|((grp, name), cost)| {
            if *grp == cost_type {
                total += cost;
                cnt += 1;
                //if *cost > cost_min_display {
                Some((*cost, name.as_str()))
                // }
            } else {
                None
            }
        })
        .collect();
    // sort Vec by cost
    bill_details_sorted.sort_by(|(a, _na), (b, _nb)| a.partial_cmp(b).unwrap());
    let mut cnt_skip = 0;
    for (cost, name) in bill_details_sorted.iter() {
        if *cost > global_opts.cost_min_display {
            println!(
                " bill_details: '{cur} {cost:9.2}' :: {t_short}:'{name}'",
                t_short = cost_type.as_short()
            );
        } else {
            cnt_skip += 1;
        }
    }
    if cnt_skip > 0 {
        println!(
            " bill_details: skipped {cnt_skip} with cost below < '{cur} {cost_min_display:.2}' Type::{t_short}",
            t_short = cost_type.as_short(),
            cost_min_display = global_opts.cost_min_display,
        );
    }
    if cnt > 0 {
        println!(
            "     Total #{cnt} {cost_type} cost {cur} {total:.2}",
            cost_type = cost_type.as_str(),
            cur = cur,
            total = total,
        );
    }
}
