use std::collections::HashSet;

use colored::Colorize;

use crate::bills::Bills;
use crate::bills::bill_filter::BillFilter;
// use super::bills_sum_data;
use crate::bills::bills_sum_data::{CostSource, SummaryData};
use crate::bills::cost_type_enum::CostType;
use crate::cmd_parse::DisplayOpts;
use crate::f64_to_currency;

// ── Display data types ────────────────────────────────────────────────────────

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum RowColour {
    Red,
    Green,
    Blue,
    Cyan,
}

#[derive(Debug)]
pub struct PreparedRow {
    pub cost: f64,
    pub name: String,
    pub source: CostSource,
    pub colour: RowColour,
    /// true when cost exceeds the display threshold
    pub visible: bool,
}

#[derive(Debug)]
pub struct PreparedSummary {
    pub rows: Vec<PreparedRow>,
    pub total: f64,
    pub total_usd: f64,
    pub skipped_count: usize,
}

// ── Pure helper functions (no I/O) ────────────────────────────────────────────

/// Assigns a display colour and visibility flag to each row; returns totals.
pub(crate) fn prepare_rows(
    bill_summary: &SummaryData,
    cost_type: CostType,
    display_opts: &DisplayOpts,
) -> PreparedSummary {
    let (total, total_usd, _cnt, sorted) = sort_calc_total(bill_summary, &cost_type);
    let mut skipped_count = 0usize;
    let rows: Vec<PreparedRow> = sorted
        .into_iter()
        .map(|(cost, name, source)| {
            let colour = match source {
                CostSource::Original => {
                    if cost < 0.0 {
                        RowColour::Cyan
                    } else {
                        RowColour::Red
                    }
                }
                CostSource::Secondary => RowColour::Green,
                CostSource::Combined => {
                    if cost < 0.0 {
                        RowColour::Green
                    } else {
                        RowColour::Blue
                    }
                }
            };
            let visible =
                cost > display_opts.cost_min_display || cost < -display_opts.cost_min_display;
            if !visible {
                skipped_count += 1;
            }
            PreparedRow {
                cost,
                name: name.to_string(),
                source,
                colour,
                visible,
            }
        })
        .collect();
    PreparedSummary {
        rows,
        total,
        total_usd,
        skipped_count,
    }
}

/// Returns the legend line for a summary block (pure, no I/O).
pub(crate) fn legend_text(is_comparison: bool) -> String {
    if !is_comparison {
        format!(
            "Legend: {cyan}",
            cyan = "Cyan=Credit/Refund(negative)".cyan()
        )
    } else {
        format!(
            "Legend: cost colour's {red} {green} {blue} {cyan}",
            red = "Red=New(only in latest)".red(),
            green = "Green=Saving(gone or reduced)".green(),
            blue = "Blue=Increased".blue(),
            cyan = "Cyan=Credit/Refund(negative)".cyan(),
        )
    }
}

/// Display cost summary.
/// can also subtract previous_bill
pub fn display_cost_by_filter(
    filter: &BillFilter,
    // file_or_folder: PathBuf,
    latest_bill: Bills,
    previous_bill: Option<Bills>,
    display_opts: &DisplayOpts,
) {
    println!();
    println!(
        "Filter Azure name:{}, rg:{}, sub:{}, cat:{}, tag_filter:{}, tag_summarise:{}, location:{}, reservation:{}, invoice_section:{}.\n",
        filter.name,
        filter.resource_group,
        filter.subscription,
        filter.meter_category,
        filter.tag_filter,
        filter.tag_summarise,
        filter.location,
        filter.reservation,
        filter.invoice_section,
    );
    // now that we have latest_bill and disks, lookup disk cost in latest_bill
    // and print the cost
    let cur = latest_bill.get_billing_currency();
    let mut display_date = latest_bill.file_short_name.clone();

    let is_comparison = previous_bill.is_some();
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
    println!(
        "## Location bill details {} '{}'",
        filter.location, display_date
    );
    print_summary(
        &bill_summary,
        &cur,
        CostType::Region,
        display_opts,
        is_comparison,
    );
    println!();

    // print Invoice Section bill details (only when filter specified)
    if !filter.invoice_section.is_empty() {
        println!(
            "## Invoice Section bill details '{}' '{}'",
            filter.invoice_section, display_date
        );
        print_summary(
            &bill_summary,
            &cur,
            CostType::InvoiceSection,
            display_opts,
            is_comparison,
        );
        println!();
    }

    // print Subscription bill details
    println!(
        "## Subscription bill details {} '{}'",
        filter.subscription, display_date
    );
    print_summary(
        &bill_summary,
        &cur,
        CostType::Subscription,
        display_opts,
        is_comparison,
    );
    println!();
    // print ResourceGroup bill details
    println!(
        "## ResourceGroup bill details {} '{}'",
        filter.resource_group, display_date
    );
    print_summary(
        &bill_summary,
        &cur,
        CostType::ResourceGroup,
        display_opts,
        is_comparison,
    );
    println!();
    // print Resource bill details
    if !filter.name.is_empty() {
        println!(
            "## ResourceName bill details {} '{}'",
            filter.resource_group, display_date
        );
        print_summary(
            &bill_summary,
            &cur,
            CostType::ResourceName,
            display_opts,
            is_comparison,
        );
    }

    // print MeterSubCategory bill details
    if !filter.meter_category.is_empty() {
        println!(
            "## MeterSubCategory bill details {} '{}'",
            filter.resource_group, display_date
        );
        print_summary(
            &bill_summary,
            &cur,
            CostType::MeterSubCategory,
            display_opts,
            is_comparison,
        );
        println!()
    }
    // print MeterCategory bill details
    if !filter.meter_category.is_empty() {
        println!(
            "## MeterCategory bill details {} '{}'",
            filter.resource_group, display_date
        );
        print_summary(
            &bill_summary,
            &cur,
            CostType::MeterCategory,
            display_opts,
            is_comparison,
        );
        println!()
    }

    // print Tag bill details
    if !filter.tag_summarise.is_empty() {
        println!("## Tag details {} '{}'", filter.tag_summarise, display_date);
        print_summary(
            &bill_summary,
            &cur,
            CostType::Tag,
            display_opts,
            is_comparison,
        );
        println!();
    }

    println!(
        "Total cost excl. GST {total_cost}  ({total_cost_usd})  date:'{display_date}' Region:'{location}'",
        total_cost = format!("{total_cost}").bold(),
        total_cost_usd = format!("{total_cost_usd}").bold(),
        display_date = display_date,
        location = filter.location,
    );

    if display_opts.tag_list {
        println!();
        println!(
            "Tags: {}\n{:?}",
            latest_bill.tag_names.len(),
            latest_bill.tag_names
        );
    }

    // print Reservation bill details
    if !filter.reservation.is_empty() {
        print_summary(
            &bill_summary,
            &cur,
            CostType::Reservation,
            display_opts,
            is_comparison,
        );
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
                        .copied()
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
    bill_details_sorted.sort_by(|(a, _na, _srca), (b, _nb, _srcb)| a.partial_cmp(b).unwrap());
    (total, total_usd, cnt, bill_details_sorted)
}

/// Thin printing adapter: calls prepare_rows + legend_text, then iterates to println!.
fn print_summary(
    bill_summary: &SummaryData,
    cur: &str,
    cost_type: CostType,
    display_opts: &DisplayOpts,
    is_comparison: bool,
) {
    let prepared = prepare_rows(bill_summary, cost_type, display_opts);

    for row in prepared.rows.iter().filter(|r| r.visible) {
        let currency = f64_to_currency(row.cost, 2);
        let part1 = format!("{cur} {currency:>11}");
        let color_cost = match row.colour {
            RowColour::Red => part1.red().to_string(),
            RowColour::Green => part1.green().to_string(),
            RowColour::Blue => part1.blue().to_string(),
            RowColour::Cyan => part1.cyan().to_string(),
        };
        println!(
            " bill_details: '{color_cost}' :: {t_short}:'{name}'",
            t_short = cost_type.as_short(),
            name = row.name,
        );
    }

    if prepared.skipped_count > 0 {
        println!(
            " bill_details: skipped {cnt_skip} with cost below < '{cur} {cost_min_display:.2}' Type::{t_short}",
            cnt_skip = prepared.skipped_count,
            t_short = cost_type.as_short(),
            cost_min_display = display_opts.cost_min_display,
        );
    }

    let total_count = prepared.rows.len();
    if total_count > 0 {
        let total_colored = if prepared.total < 0.0 {
            f64_to_currency(prepared.total, 2)
                .green()
                .bold()
                .to_string()
        } else {
            f64_to_currency(prepared.total, 2).red().bold().to_string()
        };
        println!(
            "     Total #{total_count} {cost_type} filtered cost {cur} {total_colored}  (US$ {total_usd})",
            cost_type = cost_type.as_str(),
            cur = cur,
            total_usd = f64_to_currency(prepared.total_usd, 2).bold(),
        );
        println!("     {}", legend_text(is_comparison));
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bills::bills_sum_data::CostTotal;
    use crate::cmd_parse::DisplayOpts;
    use crate::money::{Nzd, Usd};

    fn make_summary(entries: &[(&str, CostType, f64, CostSource)]) -> SummaryData<'static> {
        let mut s = SummaryData::default();
        for (name, ct, cost, source) in entries {
            s.per_type.insert(
                (ct.clone(), name.to_string()),
                CostTotal {
                    cost: Nzd(*cost),
                    cost_usd: Usd(*cost),
                    cost_unreserved: 0.0,
                    source: *source,
                },
            );
        }
        s
    }

    #[test]
    fn test_legend_text_single_bill() {
        let text = legend_text(false);
        assert!(text.contains("Cyan=Credit/Refund(negative)"));
        assert!(!text.contains("Red="));
    }

    #[test]
    fn test_legend_text_comparison() {
        let text = legend_text(true);
        assert!(text.contains("Red=New"));
        assert!(text.contains("Green=Saving"));
        assert!(text.contains("Blue=Increased"));
        assert!(text.contains("Cyan=Credit/Refund"));
    }

    #[test]
    fn test_prepare_rows_colours() {
        let summary = make_summary(&[
            (
                "new-item",
                CostType::ResourceGroup,
                50.0,
                CostSource::Original,
            ),
            (
                "credit",
                CostType::ResourceGroup,
                -10.0,
                CostSource::Original,
            ),
            (
                "gone-item",
                CostType::ResourceGroup,
                30.0,
                CostSource::Secondary,
            ),
            (
                "increased",
                CostType::ResourceGroup,
                20.0,
                CostSource::Combined,
            ),
            (
                "decreased",
                CostType::ResourceGroup,
                -15.0,
                CostSource::Combined,
            ),
        ]);
        let opts = DisplayOpts {
            cost_min_display: 0.0,
            tag_list: false,
            debug: false,
        };
        let prepared = prepare_rows(&summary, CostType::ResourceGroup, &opts);

        let find = |name: &str| prepared.rows.iter().find(|r| r.name == name).unwrap();
        assert_eq!(find("new-item").colour, RowColour::Red);
        assert_eq!(find("credit").colour, RowColour::Cyan);
        assert_eq!(find("gone-item").colour, RowColour::Green);
        assert_eq!(find("increased").colour, RowColour::Blue);
        assert_eq!(find("decreased").colour, RowColour::Green);
    }

    #[test]
    fn test_prepare_rows_visibility_threshold() {
        let summary = make_summary(&[
            ("big", CostType::ResourceGroup, 100.0, CostSource::Original),
            ("small", CostType::ResourceGroup, 5.0, CostSource::Original),
        ]);
        let opts = DisplayOpts {
            cost_min_display: 10.0,
            tag_list: false,
            debug: false,
        };
        let prepared = prepare_rows(&summary, CostType::ResourceGroup, &opts);

        let big = prepared.rows.iter().find(|r| r.name == "big").unwrap();
        let small = prepared.rows.iter().find(|r| r.name == "small").unwrap();
        assert!(big.visible);
        assert!(!small.visible);
        assert_eq!(prepared.skipped_count, 1);
    }
}
