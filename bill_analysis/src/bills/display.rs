use std::collections::HashSet;

use colored::Colorize;

use crate::bills::bill_filter::BillFilter;
use crate::bills::Bills;
// use super::bills_sum_data;
use crate::bills::bills_sum_data::{CostSource, SummaryData};
use crate::bills::cost_type_enum::CostType;
use crate::cmd_parse::GlobalOpts;
use crate::f64_to_currency;

/// Display cost summary.
/// can also subtract previous_bill

pub fn display_cost_by_filter(
    filter: &BillFilter,
    // file_or_folder: PathBuf,
    latest_bill: Bills,
    previous_bill: Option<Bills>,
    // global_opts.case_sensitive: bool,
    global_opts: &GlobalOpts,
) {
    println!();
    println!(
        "Filter Azure name:{}, rg:{}, sub:{}, cat:{}, tag_filter:{}, tag_summarise:{}, location:{}, reservation:{}, invoice_section:{}.\n",
        filter.name, filter.resource_group, filter.subscription, filter.meter_category,
        filter.tag_filter, filter.tag_summarise, filter.location, filter.reservation,
        filter.invoice_section,
    );
    // now that we have latest_bill and disks, lookup disk cost in latest_bill
    // and print the cost
    let cur = latest_bill.get_billing_currency();
    let mut display_date = latest_bill.file_short_name.clone();

    let mut bill_summary = latest_bill.cost_by_any_summary(filter);
    let mut total_cost = bill_summary.filtered_cost_total;
    let mut total_cost_usd = bill_summary.filtered_cost_total_usd;
    // If we got a previous bill calculate summary and subtract.
    if let Some(prev_bill) = previous_bill {
        display_date = format!(
            "{display_date} - {prev_date}",
            display_date = display_date,
            prev_date = prev_bill.file_short_name
        );
        let prev_bill_summary = prev_bill.cost_by_any_summary(filter);
        bill_summary.merge_summaries(&prev_bill_summary);
        total_cost = bill_summary.filtered_cost_total;
        total_cost_usd = bill_summary.filtered_cost_total_usd;
    }

    if !filter.name.is_empty() {
        println!("## Name details: len={}", bill_summary.details.len());
        // https://vscode.dev/github/diepes/rust-azure-bill-analysis/blob/main/bill_analysis/src/bills/bills_impl_cost_by_any.rs#L322
        println!("## details: {{resource_group}}_____{{resource_name}}_____{{meter_category}}");
        for d in bill_summary.details.iter() {
            println!(" details: {:?}", d);
        }
        println!();
    }

    // print Region bill details
    println!("## Location bill details {} '{}'", filter.location, display_date);
    print_summary(&bill_summary, &cur, CostType::Region, global_opts);
    println!();

    // print Invoice Section bill details (only when filter specified)
    if !filter.invoice_section.is_empty() {
        println!("## Invoice Section bill details '{}' '{}'", filter.invoice_section, display_date);
        print_summary(&bill_summary, &cur, CostType::InvoiceSection, global_opts);
        println!();
    }

    // print Subscription bill details
    println!("## Subscription bill details {} '{}'", filter.subscription, display_date);
    print_summary(&bill_summary, &cur, CostType::Subscription, global_opts);
    println!();
    // print ResourceGroup bill details
    println!("## ResourceGroup bill details {} '{}'", filter.resource_group, display_date);
    print_summary(&bill_summary, &cur, CostType::ResourceGroup, global_opts);
    println!();
    // print Resource bill details
    if !filter.name.is_empty() {
        println!("## ResourceName bill details {} '{}'", filter.resource_group, display_date);
        print_summary(&bill_summary, &cur, CostType::ResourceName, global_opts);
    }

    // print MeterSubCategory bill details
    if !filter.meter_category.is_empty() {
        println!(
            "## MeterSubCategory bill details {} '{}'",
            filter.resource_group, display_date
        );
        print_summary(&bill_summary, &cur, CostType::MeterSubCategory, global_opts);
        println!()
    }
    // print MeterCategory bill details
    if !filter.meter_category.is_empty() {
        println!("## MeterCategory bill details {} '{}'", filter.resource_group, display_date);
        print_summary(&bill_summary, &cur, CostType::MeterCategory, global_opts);
        println!()
    }

    // print Tag bill details
    if !filter.tag_summarise.is_empty() {
        println!("## Tag details {} '{}'", filter.tag_summarise, display_date);
        print_summary(&bill_summary, &cur, CostType::Tag, global_opts);
        println!();
    }

    println!(
        "Total cost excl. GST {total_cost}  ({total_cost_usd})  date:'{display_date}' Region:'{location}'",
        total_cost = format!("{total_cost}").bold(),
        total_cost_usd = format!("{total_cost_usd}").bold(),
        display_date = display_date,
        location = filter.location,
    );

    if global_opts.tag_list {
        println!();
        println!(
            "Tags: {}\n{:?}",
            latest_bill.tag_names.len(),
            latest_bill.tag_names
        );
    }

    // print Reservation bill details
    if !filter.reservation.is_empty() {
        print_summary(&bill_summary, &cur, CostType::Reservation, global_opts);
        println!();

        println!();
        println!("Reservations:");
        let mut unique_key = HashSet::new();
        for (key, _day) in bill_summary.reservations.keys() {
            unique_key.insert(key.clone());
        }
        for key in unique_key {
            println!("{} '{}'", "Reservation key:".blue(), key.blue());
            let mut res_cost_savings = 0.0;
            let mut res_cost_unused = 0.0;
            let mut res_cost_full = 0.0;
            let mut res_compare_days = "".to_string();
            for day in 1..=31 {
                if let Some(reservation) = bill_summary.reservations.get_mut(&(key.clone(), day)) {
                    // get reservation names convert to vec and sort them
                    let mut rn = reservation
                        .reservation_names
                        .iter()
                        .map(|s| *s)
                        .collect::<Vec<&str>>();
                    rn.sort();
                    let rn = rn.join(", ").blue();
                    // get vm names and sort them
                    reservation.vm_names_reserved.sort();
                    let rvmr = reservation.vm_names_reserved.join(", ").green();
                    // get vm names not reserved and sort them
                    reservation.vm_names_not_reserved.sort();
                    let rvmnr = reservation.vm_names_not_reserved.join(", ").red();
                    let res_compare_days_new = format!("{rn}{rvmr}{rvmnr}");
                    if res_compare_days_new != res_compare_days {
                        // only print if different
                        res_compare_days = res_compare_days_new;
                        println!(
                            "Res: Day:{d} Save:{rcs:.2} Unused:{rcu:.2} FullCost:{cf:.2} , key:'{key}'\n     ResName:[{rn}]\n     VMsRes:[{rvmr}]\n     VMsNotRes:[{rvmnr}]",
                            d = day,
                            rcs = reservation.cost_savings,
                            rcu = reservation.cost_unused,
                            cf = reservation.cost_full,
                            key = key.red(),
                        );
                    };
                    res_cost_savings += reservation.cost_savings;
                    res_cost_unused += reservation.cost_unused;
                    res_cost_full += reservation.cost_full;
                }
            }
            println!(
                "    Month Total: Save:{rcs:.2} Unused:{rcu:.2} FullCost:{cf:.2} Saving:{saving_pct} key:'{key}' ",
                rcs = res_cost_savings,
                rcu = res_cost_unused,
                cf = res_cost_full,
                key = key.red(),
                saving_pct = format!(
                    "{:.0}%",
                    res_cost_savings / (res_cost_full + res_cost_unused) * 100.0
                )
                .green(),
            );
        }
        // if *day == 1 || true {
        //     println!(
        //         "  Reservation benefit: {} - day {} - {}",
        //         res_name, day, reservation.meter_category
        //     );
        //     println!("    Cost: {}", f64_to_currency(reservation.cost_full, 2));
        //     println!(
        //         "    Savings: {}",
        //         f64_to_currency(reservation.cost_savings, 2)
        //     );
        //     println!("    Hours: {}", reservation.hr_total);
        //     println!("    Savings Hours: {}", reservation.hr_saving);
        //     println!("    Unused reservation: Day:{} {}", day, reservation.cost_unused);
        //     println!(
        //         "    Reservations: {}",
        //         reservation
        //             .reservation_names
        //             .iter()
        //             .map(|s| s.as_str())
        //             .collect::<Vec<&str>>()
        //             .join(", "),
        //     );
        //     println!("    VM Names: #{}", reservation.vm_names.len());
        //     println!(" Reservation # {}", bill_summary.reservations.len());
        // }
        // }
    }
}

fn sort_calc_total<'a>(
    bill_details: &'a SummaryData,
    cost_type: &CostType,
) -> (f64, f64, i32, Vec<(f64, &'a str, CostSource)>) {
    let mut total = 0.0;
    let mut total_usd = 0.0;
    let mut cnt = 0;
    let mut bill_details_sorted: Vec<(f64, &str, CostSource)> = bill_details
        .per_type
        .iter()
        .filter_map(|((grp, name), cost)| {
            if grp == cost_type {
                total += cost.cost.amount();
                total_usd += cost.cost_usd.amount();
                cnt += 1;
                Some((cost.cost.amount(), name.as_str(), cost.source))
            } else {
                None
            }
        })
        .collect();
    bill_details_sorted
        .sort_by(|(a, _na, _srca), (b, _nb, _srcb)| a.partial_cmp(b).unwrap());
    (total, total_usd, cnt, bill_details_sorted)
}

/// print_summary for Subscription, ResourceGroup, ResourceName, MeterCategory
/// called for each summary section to print the cost details
fn print_summary(
    bill_summary: &SummaryData,
    cur: &str,
    cost_type: CostType,
    global_opts: &GlobalOpts,
) {
    let (total, total_usd, cnt, bill_details_sorted) =
        sort_calc_total(bill_summary, &cost_type);
    let mut cnt_skip = 0;

    let color_legend = if global_opts.bill_prev_subtract_path.is_none() {
        format!("Legend: {cyan}", cyan = "Cyan=Credit/Refund(negative)".cyan())
    } else {
        format!(
            "Legend: cost colour's {red} {green} {blue} {cyan}",
            red = "Red=New(only in latest)".red(),
            green = "Green=Saving(gone or reduced)".green(),
            blue = "Blue=Increased".blue(),
            cyan = "Cyan=Credit/Refund(negative)".cyan(),
        )
    };
    // print sorted Vec by cost
    for (cost, name, source) in bill_details_sorted.iter() {
        let currency = f64_to_currency(*cost, 2);
        let part1 = format!("{cur} {currency:>11}");

        let color_cost = match source {
            // Original: new item in latest bill. Negative = actual credit/refund.
            CostSource::Original => {
                if *cost < 0.0 { part1.cyan().to_string() } else { part1.red().to_string() }
            }
            // Secondary: only in previous bill (gone) — always a saving, show green.
            CostSource::Secondary => part1.green().to_string(),
            // Combined: in both bills. Negative = cost went down (green), positive = went up (blue).
            CostSource::Combined => {
                if *cost < 0.0 { part1.green().to_string() } else { part1.blue().to_string() }
            }
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
        let total_colored = if total < 0.0 {
            // Negative net total = overall saving (comparison) or credit (single bill) — green
            f64_to_currency(total, 2).green().bold().to_string()
        } else {
            f64_to_currency(total, 2).red().bold().to_string()
        };
        println!(
            "     Total #{cnt} {cost_type} filtered cost {cur} {total_colored}  (US$ {total_usd})",
            cost_type = cost_type.as_str(),
            cur = cur,
            total_usd = f64_to_currency(total_usd, 2).bold(),
        );
        println!("     {color_legend}");
    }
}
