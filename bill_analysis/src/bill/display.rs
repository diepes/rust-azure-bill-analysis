use colored::Colorize;

use super::bills::Bills;
use super::billsummary;
use crate::bill::billsummary::{CostSource, CostTotal};
use crate::bill::costtype::CostType;
use crate::cmd_parse::GlobalOpts;
use crate::f64_to_currency;

/// Display cost summary.
/// can also subtract previous_bill

pub fn display_cost_by_filter(
    name_r: Option<String>,
    rg_r: Option<String>,
    sub_r: Option<String>,
    cat_r: Option<String>,
    region_r: Option<String>,
    reservation_r: Option<String>,
    tag_summarize: Option<String>,
    tag_filter: Option<String>,
    // file_or_folder: PathBuf,
    latest_bill: Bills,
    previous_bill: Option<Bills>,
    // global_opts.case_sensitive: bool,
    global_opts: &GlobalOpts,
) {
    println!();
    println!("Filter Azure name_r:{name_r:?}, rg_r:{rg_r:?}, sub_r:{sub_r:?}, cat_r:{cat_r:?}, tag_r:{tag_filter:?}, tag_s:{tag_summarize:?}, region_r:{region_r:?}, reservation_r:{reservation_r:?}.\n");
    // now that we have latest_bill and disks, lookup disk cost in latest_bill
    // and print the cost
    let cur = latest_bill.get_billing_currency();
    let s_name = name_r.unwrap_or("".to_string());
    let s_rg = rg_r.unwrap_or("".to_string());
    let s_sub = sub_r.unwrap_or("".to_string());
    let s_cat = cat_r.unwrap_or("".to_string());
    let s_region = region_r.unwrap_or("any".to_string()); // allow for capture of empty region
    let s_reservation = reservation_r.unwrap_or("".to_string());
    let s_tag_s = tag_summarize.clone().unwrap_or("".to_string());
    let s_tag_r = tag_filter.unwrap_or("".to_string());
    let mut display_date = latest_bill.file_short_name.clone();

    let mut bill_summary = latest_bill.cost_by_any_summary(
        &s_name,
        &s_rg,
        &s_sub,
        &s_cat,
        &s_region,
        &s_reservation,
        &s_tag_s,
        &s_tag_r,
        global_opts,
    );
    let mut total_cost = bill_summary.filtered_cost_total;
    // If we got a previous bill calculate summary and subtract.
    if let Some(prev_bill) = previous_bill {
        display_date = format!(
            "{display_date} - {prev_date}",
            display_date = display_date,
            prev_date = prev_bill.file_short_name
        );
        let prev_bill_summary = prev_bill.cost_by_any_summary(
            &s_name,
            &s_rg,
            &s_sub,
            &s_cat,
            &s_region,
            &s_reservation,
            &s_tag_s,
            &s_tag_r,
            global_opts,
        );
        total_cost -= prev_bill_summary.filtered_cost_total;
        // merge negative values from prev_bill_details into bill_details Hashmap
        // key (CostType, "resource_name")
        for (prev_key, prev_cost_total) in &prev_bill_summary.per_type {
            bill_summary
                .per_type
                .entry(prev_key.clone())
                .and_modify(|cost_total| {
                    cost_total.cost -= prev_cost_total.cost;
                    cost_total.cost_unreserved -= prev_cost_total.cost_unreserved;
                    cost_total.source = CostSource::Combined;
                })
                .or_insert(CostTotal {
                    cost: -prev_cost_total.cost,
                    cost_unreserved: -prev_cost_total.cost_unreserved,
                    source: CostSource::Secondary,
                });
        }
        // merge negative values from prev_details into details HashSet
        bill_summary.details.extend(prev_bill_summary.details);
    }

    if !s_name.is_empty() {
        println!(" details: len={}", bill_summary.details.len());
        for d in bill_summary.details.iter() {
            println!(" details: {:?}", d);
        }
    }

    // print Subscription bill details
    println!("## Subscription bill details {} '{}'", s_sub, display_date);
    print_summary(&bill_summary, &cur, CostType::Subscription, global_opts);
    println!();
    // print ResourceGroup bill details
    println!("## ResourceGroup bill details {} '{}'", s_rg, display_date);
    print_summary(&bill_summary, &cur, CostType::ResourceGroup, global_opts);
    println!();
    // print Resource bill details
    if !s_name.is_empty() {
        print_summary(&bill_summary, &cur, CostType::ResourceName, global_opts);
    }

    // print Category bill details
    if !s_cat.is_empty() {
        print_summary(&bill_summary, &cur, CostType::MeterCategory, global_opts);
        println!()
    }

    // print Reservation bill details
    if !s_reservation.is_empty() {
        print_summary(&bill_summary, &cur, CostType::Reservation, global_opts);
        println!()
    }

    // print Tag bill details
    if !s_tag_s.is_empty() {
        println!("## Tag details {} '{}'", s_tag_s, display_date);
        print_summary(&bill_summary, &cur, CostType::Tag, global_opts);
        println!();
    }

    println!(
        "Total cost {cur} {total_cost}  date:'{display_date}' Region:'{s_region}'",
        cur = cur,
        total_cost = f64_to_currency(total_cost, 2).bold(),
        display_date = display_date,
        s_region = s_region,
    );

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
    //bill_details: &'a std::collections::HashMap<(CostType, String), CostTotal>,
    bill_details: &'a billsummary::SummaryData,
    cost_type: &CostType,
) -> (f64, f64, i32, Vec<(f64, f64, &'a str, CostSource)>) {
    let mut total = 0.0;
    let mut total_unreserved = 0.0;
    let mut cnt = 0;
    // create Vec from HashMap for specific CostType
    let mut bill_details_sorted: Vec<(f64, f64, &str, CostSource)> = bill_details
        .per_type
        .iter()
        .filter_map(|((grp, name), cost)| {
            if grp == cost_type {
                total += cost.cost;
                total_unreserved += cost.cost_unreserved;
                cnt += 1;
                // return some or none
                Some((cost.cost, cost.cost_unreserved, name.as_str(), cost.source))
            } else {
                None
            }
        })
        .collect();
    // sort Vec by cost
    bill_details_sorted
        .sort_by(|(a, _resa, _na, _srca), (b, _resb, _nb, _srcb)| a.partial_cmp(b).unwrap());
    //
    (total, total_unreserved, cnt, bill_details_sorted)
}

/// print_summary for Subscription, ResourceGroup, ResourceName, MeterCategory
fn print_summary(
    bill_summary: &billsummary::SummaryData,
    cur: &str,
    cost_type: CostType,
    global_opts: &GlobalOpts,
) {
    let (total, total_unreserved, cnt, bill_details_sorted) =
        sort_calc_total(bill_summary, &cost_type);
    let mut cnt_skip = 0;

    let color_legend = if global_opts.bill_prev_subtract_path.is_none() {
        "".to_string()
    } else {
        format!(
            "Legend: cost colour's {red} {green} {blue} {yellow}",
            red = "Red=Original(New)".red(),
            green = "Green=Previous(Gone)".green(),
            blue = "Blue=Combined(Changed)".blue(),
            yellow = "Yellow=ReservationSavings".yellow(),
        )
    };
    // print sorted Vec by cost
    for (cost, cost_unreserved, name, source) in bill_details_sorted.iter() {
        let currency = f64_to_currency(*cost, 2);
        let _cur_unreserved = f64_to_currency(*cost_unreserved, 2);
        let savings = *cost_unreserved - *cost;
        let cur_savings = f64_to_currency(savings, 2);
        let part1 = format!("{cur} {currency:>11}");
        let part2 = if savings > 1.0 { format!("+{cur_savings:>9}") } else { "".to_string() };

        let color_cost = match source {
            CostSource::Original => format!("{} {:>11}",part1.red(),part2.yellow()),
            CostSource::Secondary => format!("{} {:>11}",part1.green(),part2.yellow()),
            CostSource::Combined => format!("{} {:>11}",part1.blue(),part2.yellow()),
        };
        if *cost > global_opts.cost_min_display || *cost < -global_opts.cost_min_display {
            println!(
                " bill_details: '{color_cost}' :: {t_short}:'{name}'",
                t_short = cost_type.as_short(),
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
            "     Total #{cnt} {cost_type} filtered cost {cur} {total} unreserved {cur} {total_unreserved}",
            cost_type = cost_type.as_str(),
            cur = cur,
            // total = (total as i64).to_formatted_string(&Locale::en).bold(),
            total = f64_to_currency(total, 2).bold(),
            total_unreserved = f64_to_currency(total_unreserved, 2).bold(),
        );
        println!("     {color_legend}");
    }
}
