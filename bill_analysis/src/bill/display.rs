use super::bills::Bills;
use crate::bill::costtype::CostType;
use crate::cmd_parse::GlobalOpts;

/// Display cost summary.
/// can also subtract previous_bill

pub fn display_cost_by_filter(
    name_r: Option<String>,
    rg_r: Option<String>,
    sub_r: Option<String>,
    cat_r: Option<String>,
    tag_summarize: Option<String>,
    tag_filter: Option<String>,
    // file_or_folder: PathBuf,
    latest_bill: Bills,
    previous_bill: Option<Bills>,
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

    let (bill_cost, mut details, mut bill_details) = latest_bill.cost_by_any(
        &s_name,
        &s_rg,
        &s_sub,
        &s_cat,
        &s_tag_s,
        &s_tag_r,
        &global_opts,
    );
    let mut total_cost = bill_cost;
    // If we got a previous bill calculate summary and subtract.
    if let Some(prev_bill) = previous_bill {
        let (prev_bill_cost, prev_details, prev_bill_details) = prev_bill.cost_by_any(
            &s_name,
            &s_rg,
            &s_sub,
            &s_cat,
            &s_tag_s,
            &s_tag_r,
            &global_opts,
        );
        total_cost -= prev_bill_cost;
        // merge negative values from prev_bill_details into bill_details Hashmap
        // key (CostType, "resource_name")
        for (key, value) in &prev_bill_details {
            *bill_details.entry(key.clone()).or_insert(0.0) -= value;
        }
        // merge negative values from prev_details into details HashSet
        details.extend(prev_details);
    }

    if s_name.len() > 0 {
        println!(" details: len={}", details.len());
        for d in details.iter() {
            println!(" details: {:?}", d);
        }
    }

    // print Subscription bill details
    println!("## Subscription bill details {}", s_sub);
    print_summary(&bill_details, &cur, CostType::Subscription, global_opts);
    println!();
    // print ResourceGroup bill details
    println!("## ResourceGroup bill details");
    print_summary(&bill_details, &cur, CostType::ResourceGroup, global_opts);
    println!();
    // print Resource bill details
    if s_name.len() > 0 {
        print_summary(&bill_details, &cur, CostType::ResourceName, global_opts);
    }

    // print Category bill details
    if s_cat.len() > 0 {
        print_summary(&bill_details, &cur, CostType::MeterCategory, &global_opts);
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

fn sort_calc_total<'a>(
    bill_details: &'a std::collections::HashMap<(CostType, String), f64>,
    cost_type: &CostType,
) -> (f64, i32, Vec<(f64, &'a str)>) {
    let mut total = 0.0;
    let mut cnt = 0;
    // create Vec from HashMap for specific CostType
    let mut bill_details_sorted: Vec<(f64, &str)> = bill_details
        .iter()
        .filter_map(|((grp, name), cost)| {
            if grp == cost_type {
                total += cost;
                cnt += 1;
                // return some or none
                Some((*cost, name.as_str()))
            } else {
                None
            }
        })
        .collect();
    // sort Vec by cost
    bill_details_sorted.sort_by(|(a, _na), (b, _nb)| a.partial_cmp(b).unwrap());
    (total, cnt, bill_details_sorted)
}

/// print_summary for Subscription, ResourceGroup, ResourceName, MeterCategory
fn print_summary(
    bill_details: &std::collections::HashMap<(CostType, String), f64>,
    cur: &str,
    cost_type: CostType,
    global_opts: &GlobalOpts,
) {
    let (total, cnt, bill_details_sorted) = sort_calc_total(&bill_details, &cost_type);
    let mut cnt_skip = 0;
    for (cost, name) in bill_details_sorted.iter() {
        if *cost > global_opts.cost_min_display || *cost < -global_opts.cost_min_display {
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
