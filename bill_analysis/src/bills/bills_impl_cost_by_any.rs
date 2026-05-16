use crate::bills::bill_filter::BillFilter;
use crate::bills::Bills;

use crate::bills::cost_type_enum::CostType;
// use crate::bills::ReservationInfo;
use crate::bills::bills_sum_data::SummaryData;
use crate::money::{Nzd, Usd};
// use crate::RESERVATION_SUMMARY;

impl Bills {
    // function cost_by_any
    // takes name_regex, rg_regex, subs_regex, meter_category, tag_regex and returns total where all match,
    //   if empty str in input it is skiped for filter.
    // returns total_filtered_cost,
    //         set of filtered resource groups,
    //     and HashMap of filtered cost per category(each category total - total filtered cost)
    pub fn cost_by_any_summary(
        &self,
        filter: &BillFilter,
    ) -> SummaryData<'_> {
        // collect set of resource groups in set rgs
        let mut summary_data = SummaryData::default();
        // bill_details record cost per filter category e.g. name_regex, rg_regex, subs_regex, meter_category
        // per_type
        // iter through bills, get total and update new bill_details for each category.
        let filtered_total = self.bills.iter().fold((Nzd::default(), Usd::default()), |acc, bill| {
            let mut flag_match = true;
            if !filter.name.is_empty() && !filter.re_name.is_match(&bill.resource_name) {
                flag_match = false; // if filter set and no match skip
            } else if !filter.resource_group.is_empty() && !filter.re_resource_group.is_match(&bill.resource_group) {
                flag_match = false;
            } else if !filter.subscription.is_empty() && !filter.re_subscription.is_match(&bill.subscription_name) {
                flag_match = false;
            } else if !filter.meter_category.is_empty() && !filter.re_meter_category.is_match(&bill.meter_category) {
                flag_match = false;
            // Check tags hashmap for match
            } else if !filter.tag_filter.is_empty() && !filter.re_tag_filter.is_match(&bill.tags.value) {
                flag_match = false;
            } else if !filter.reservation.is_empty() && !filter.re_reservation.is_match(&bill.benefit_name)
            {
                flag_match = false;
            } else if !filter.invoice_section.is_empty()
                && !filter.re_invoice_section.is_match(&bill.invoice_section)
            {
                flag_match = false;
            } else if match (
                filter.location.as_str(),
                filter.location.len(),
                filter.re_location.is_match(&bill.resource_location),
                bill.resource_location.len(),
            ) {
                ("any" , _, _    , _) => false, // any(default) any region ok, leave flag_match unchanged
                ("all" , _, _    , _) => false, // any(default) any region ok, leave flag_match unchanged
                ("none", _, _    , 1..) => true, // empty region. if value set for resource_location, set to false to skip
                (_, _, true, _) => false, // if location_regex set and match, leave unchanged
                //(_    , 1.., _  , 0) => false, // if location_regex set(len>0) and resource_location not set , set to false to skip
                (_     , _, false, _) => true, // if location_regex set and no match, set to false to skip
            } {
                flag_match = false;
            }
            if flag_match {
                // if flag_match still true (no filter above excluded this bill) add to summary_data
                // record cost against resource_name, resource_group, subscription_name, meter_category, tag
                let cost_unreserved = bill.unit_price * bill.quantity;
                // do some sanity checks / assert's
                if bill.meter_name == "RoundingAdjustment" {
                    assert!( bill.effective_price < 2.0,
                        "RoundingAdjustment cost too high ${} ResName:'{}' date:{} RG:'{}' line_csv:{}",
                        bill.effective_price, bill.resource_name, bill.date, bill.resource_group, bill.line_number_csv,
                    );
                } else if bill.meter_name == "Unassigned" { // MeterName: "Unassigned" software market place purchases
                    // should be zero cost, look at actual unit cost x quantity
                    assert_eq!(
                        bill.effective_price * bill.quantity, 0.0,
                        "Unassigned cost not zero ${} cost:${} ResName:'{}' date:{} RG:'{}' line_csv:{}",
                        bill.effective_price, bill.cost, bill.resource_name, bill.date, bill.resource_group, bill.line_number_csv,
                    );
                } else {
                    // Note: effective_price is in the pricing currency; cost is in the billing
                    // currency (NZD). With FX conversion these are not equal, so no assertion here.
                };

                summary_data.accumulate(
                    CostType::ResourceName,
                    bill.resource_name.clone(),
                    bill.cost,
                    bill.cost_usd,
                    cost_unreserved,
                );

                summary_data.accumulate(
                    CostType::ResourceGroup,
                    bill.resource_group.clone(),
                    bill.cost,
                    bill.cost_usd,
                    cost_unreserved,
                );

                summary_data.accumulate(
                    CostType::Subscription,
                    bill.subscription_name.clone(),
                    bill.cost,
                    bill.cost_usd,
                    cost_unreserved,
                );

                summary_data.accumulate(
                    CostType::MeterCategory,
                    bill.meter_category.clone(),
                    bill.cost,
                    bill.cost_usd,
                    cost_unreserved,
                );

                summary_data.accumulate(
                    CostType::Reservation,
                    bill.benefit_name.clone(),
                    bill.cost,
                    bill.cost_usd,
                    cost_unreserved,
                );

                let region = if bill.resource_location.is_empty() { "none" } else { &bill.resource_location };
                summary_data.accumulate(
                    CostType::Region,
                    region.to_string(),
                    bill.cost,
                    bill.cost_usd,
                    cost_unreserved,
                );

                let section = if !bill.invoice_section.is_empty() {
                    bill.invoice_section.clone()
                } else if !bill.meter_sub_category.is_empty() {
                    format!("({})", bill.meter_sub_category)
                } else {
                    "none".to_string()
                };
                summary_data.accumulate(
                    CostType::InvoiceSection,
                    section,
                    bill.cost,
                    bill.cost_usd,
                    cost_unreserved,
                );

                // add bill_details for tags, using the matched tag and value
                if !filter.tag_summarise.is_empty() {
                    let tag_summarize_lowercase = if filter.case_sensitive {
                        filter.tag_summarise.to_string()
                    } else {
                        filter.tag_summarise.to_lowercase()
                    };
                    let tag_key = if bill.tags.kv.contains_key(&tag_summarize_lowercase) {
                        let v = bill.tags.kv.get(&tag_summarize_lowercase).unwrap();
                        format!("tag:{}={}", v.1, v.0)
                    } else {
                        "tag:none".to_string()
                    };
                    summary_data.accumulate(
                        CostType::Tag,
                        tag_key,
                        bill.cost,
                        bill.cost_usd,
                        cost_unreserved,
                    );
                } // end tag_summarise

                summary_data.accumulate(
                    CostType::MeterSubCategory,
                    format!("{}__{}", bill.meter_category, bill.meter_sub_category),
                    bill.cost,
                    bill.cost_usd,
                    cost_unreserved,
                );
                // TODO: Add RESERVATION SUMMARY, struct added to Bills
                // if RESERVATION_SUMMARY
                //     .iter()
                //     // check if unit_price > 0.0 to filter SQL Licence and storage at zero cost
                //     .any(|(k,v)| {
                //         *k == bill.meter_category &&
                //         bill.unit_price > 0.0 &&
                //         !v.iter().any(|rule| bill.meter_sub_category.contains(rule) )
                //     })
                //     {
                //     // add to reservation summary
                //     let savings = cost_unreserved - bill.cost;
                //     if savings < -0.0001 && bill.charge_type != "UnusedReservation" {
                //         println!(
                //             "Over charge cost > unitprice*quantity:{} Name:{} RG:{} cost_unreserverd:{}, ChargeType:{}, LineCSV:{}",
                //             savings,
                //             bill.resource_name,
                //             bill.resource_name,
                //             cost_unreserved,
                //             bill.charge_type,
                //             bill.line_number_csv,
                //         );
                //     };
                //     // assert!(bill.reservation_name != "", "No reservation name meter_category:{}, ChargeType:{}, LineCSV:{}",
                //     //     bill.meter_category,
                //     //     bill.charge_type,
                //     //     bill.line_number_csv,
                //     // );
                //     summary_data
                //         .reservations
                //         .entry((
                //             // TODO: make meter_sub_category complex, add MeterCategory, MeterSubCategory, MeterName and MeterRegion
                //             format!("MC:{}__MSubC:{}",bill.meter_category,bill.meter_sub_category), // flex type e.g. "Dav4/Dasv4 Series"
                //             bill.date[3..5].parse().expect(
                //                 format!("Invalid date expected fmt mm/dd/yyyy {}", bill.date)
                //                     .as_str(),
                //             ),
                //         ))
                //         .and_modify(|e| {
                //             e.cost_full += cost_unreserved;
                //             e.cost_savings += savings;
                //             e.hr_saving += if savings > 0.01 { bill.quantity } else { 0.0 };
                //             e.hr_total += bill.quantity;
                //             if bill.pricing_model == "Reservation" {
                //                 e.cost_unused += if bill.charge_type == "UnusedReservation" {
                //                     bill.cost
                //                 } else { 0.00 };
                //                 e.reservation_names.insert(&bill.reservation_name);
                //                 e.vm_names_reserved.push(&bill.resource_name);
                //             } else {
                //                 e.vm_names_not_reserved.push(&bill.resource_name);
                //             }
                //         })
                //         .or_insert(ReservationInfo {
                //             cost_full: cost_unreserved,
                //             cost_savings: savings,
                //             hr_total: bill.quantity,
                //             hr_saving: if savings > 0.01 { bill.quantity } else { 0.0 },
                //             cost_unused: if bill.charge_type == "UnusedReservation" {
                //                 bill.cost
                //             } else { 0.00 },
                //             reservation_names: if bill.reservation_name != "" { let mut rn = HashSet::<&str>::new(); rn.insert(&bill.reservation_name); rn } else { HashSet::new() },
                //             vm_names_reserved: if bill.pricing_model == "Reservation" { vec![&bill.resource_name] } else { Vec::new() },
                //             vm_names_not_reserved: if bill.pricing_model != "Reservation" { vec![&bill.resource_name] } else { Vec::new() },
                //             meter_category: bill.meter_category.clone(),
                //         });
                // }
                summary_data.details.insert(format!(
                    "{rg}_____{rn}_____{mc}",
                    rg = bill.resource_group.clone(),
                    mc = bill.meter_category.clone(),
                    rn = bill.resource_name.clone(),
                ));
                (acc.0 + bill.cost, acc.1 + bill.cost_usd)
            } else {
                acc
            }
        }); // end loop through bill entries
        //
        // bill_details should have same cost total for each category
        summary_data.filtered_cost_total = filtered_total.0;
        summary_data.filtered_cost_total_usd = filtered_total.1;
        summary_data
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::bills::bill_filter::BillFilter;
    use crate::bills::bills_sum_data::{CostSource, CostTotal};
    use crate::bills::cost_type_enum::CostType;
    use crate::cmd_parse::FilterOpts;
    use crate::money::{Nzd, Usd};

    // use super::*;

    static FILTER_OPTS: FilterOpts = FilterOpts { case_sensitive: true };

    #[test]
    fn test_cost_by_resource_name() {
        let file_name: PathBuf = PathBuf::from("tests/azure_test_data_01.csv");
        let mut bills = crate::bills::Bills::default();
        let result = bills.parse_csv(&file_name, &FILTER_OPTS);
        assert!(
            result.is_ok(),
            "!Error parsing the file:'{file_name:?}'\nERR:{}",
            result.err().unwrap()
        );
        let cost = bills.cost_by_resource_name("NLSYDWAVAP01P-OSdisk-00_ide_0_869850_GXMD_40cfb0");
        assert_eq!(cost, Nzd(0.002785917));
    }
    #[test]
    fn test_parse_csv() {
        let file_name: PathBuf = PathBuf::from("tests/azure_test_data_01.csv");
        let mut bills = crate::bills::Bills::default();
        let result = bills.parse_csv(&file_name, &FILTER_OPTS);
        assert!(
            result.is_ok(),
            "!Error parsing the file:'{file_name:?}'\nERR:{}",
            result.err().unwrap()
        );
        assert_eq!(bills.bills.len(), 8);
        let first_bill = &bills.bills[0];
        assert_eq!(
            first_bill.subscription_id, "fc123456-7890-1234-5678-901234567890",
            "subscription_id mismatch"
        );
        assert_eq!(
            first_bill.subscription_name, "TstNl",
            "subscription_name mismatch"
        );
        assert_eq!(first_bill.date, "2024-03-08", "date mismatch");
        assert_eq!(
            first_bill.product, "TestVirtNet-Intra-Region",
            "product mismatch"
        );
        assert_eq!(
            first_bill.meter_id, "59bc01e3-test-4b9f-bacf-35e696aad6d4",
            "meter_id mismatch"
        );
        assert_eq!(
            first_bill.meter_name, "Intra-Region Ingress",
            "meter_name mismatch"
        );
        assert_eq!(first_bill.quantity, 0.194368534, "quantity mismatch");
        assert_eq!(first_bill.cost, Nzd(0.003025655), "cost mismatch");
    }

    /// Verify `cost_by_any_summary` accumulates NZD and USD correctly into `per_type`
    /// for each CostType dimension, and that `filtered_cost_total` matches the sum.
    ///
    /// Uses the two-row NZD/USD fixture (latest) so expected values are known exactly.
    #[test]
    fn test_cost_by_any_summary_per_type_nzd_usd() {
        let path = PathBuf::from("tests/azure_test_nzd_usd_latest.csv");
        let mut bills = crate::bills::Bills::default();
        bills.parse_csv(&path, &FILTER_OPTS).expect("parse failed");

        let filter = BillFilter::new(None, None, None, None, None, None, None, None, None, &FILTER_OPTS)
            .expect("valid test filter");
        let summary = bills.cost_by_any_summary(&filter);

        // Both rows have MeterCategory=Compute → aggregate across both
        let mc_key = (CostType::MeterCategory, "Compute".to_string());
        let mc = summary.per_type.get(&mc_key).expect("MeterCategory Compute missing");
        assert_eq!(mc.cost, Nzd(150.0), "MeterCategory NZD total");
        assert_eq!(mc.cost_usd, Usd(90.0), "MeterCategory USD total");

        // Individual ResourceGroup entries
        let rg1_key = (CostType::ResourceGroup, "rg-delta-test".to_string());
        let rg1 = summary.per_type.get(&rg1_key).expect("rg-delta-test missing");
        assert_eq!(rg1.cost, Nzd(100.0), "rg-delta-test NZD");
        assert_eq!(rg1.cost_usd, Usd(60.0), "rg-delta-test USD");

        let rg2_key = (CostType::ResourceGroup, "rg-new-only".to_string());
        let rg2 = summary.per_type.get(&rg2_key).expect("rg-new-only missing");
        assert_eq!(rg2.cost, Nzd(50.0), "rg-new-only NZD");
        assert_eq!(rg2.cost_usd, Usd(30.0), "rg-new-only USD");

        // filtered_cost_total must equal sum of all rows
        assert_eq!(summary.filtered_cost_total, Nzd(150.0), "filtered NZD total");
        assert_eq!(summary.filtered_cost_total_usd, Usd(90.0), "filtered USD total");
    }

    /// Verify that the rg_regex filter includes matching rows and excludes non-matching rows.
    /// Both `per_type` and `filtered_cost_total` must reflect the filtered subset only.
    #[test]
    fn test_cost_by_any_summary_rg_filter() {
        let path = PathBuf::from("tests/azure_test_nzd_usd_latest.csv");
        let mut bills = crate::bills::Bills::default();
        bills.parse_csv(&path, &FILTER_OPTS).expect("parse failed");

        let filter = BillFilter::new(None, Some("rg-delta-test".to_string()), None, None, None, None, None, None, None, &FILTER_OPTS)
            .expect("valid test filter");
        let summary = bills.cost_by_any_summary(&filter);

        // Matching RG present with full row cost
        let rg_key = (CostType::ResourceGroup, "rg-delta-test".to_string());
        let rg = summary.per_type.get(&rg_key).expect("rg-delta-test missing after filter");
        assert_eq!(rg.cost, Nzd(100.0), "filtered rg NZD");
        assert_eq!(rg.cost_usd, Usd(60.0), "filtered rg USD");

        // Non-matching RG must be absent
        let excluded_key = (CostType::ResourceGroup, "rg-new-only".to_string());
        assert!(
            summary.per_type.get(&excluded_key).is_none(),
            "rg-new-only should be excluded by rg filter"
        );

        // Totals reflect filtered row only
        assert_eq!(summary.filtered_cost_total, Nzd(100.0), "filtered NZD total");
        assert_eq!(summary.filtered_cost_total_usd, Usd(60.0), "filtered USD total");
    }

    /// Invariant: sum of all (ResourceGroup, *) values in `per_type` must equal
    /// `filtered_cost_total` and `filtered_cost_total_usd`.
    #[test]
    fn test_filtered_totals_equal_per_type_rg_sum() {
        let path = PathBuf::from("tests/azure_test_nzd_usd_latest.csv");
        let mut bills = crate::bills::Bills::default();
        bills.parse_csv(&path, &FILTER_OPTS).expect("parse failed");

        let filter = BillFilter::new(None, None, None, None, None, None, None, None, None, &FILTER_OPTS)
            .expect("valid test filter");
        let summary = bills.cost_by_any_summary(&filter);

        let (rg_nzd_sum, rg_usd_sum) = summary
            .per_type
            .iter()
            .filter(|((ct, _), _)| *ct == CostType::ResourceGroup)
            .fold((Nzd(0.0), Usd(0.0)), |(n, u), (_, v)| {
                (n + v.cost, u + v.cost_usd)
            });

        assert_eq!(rg_nzd_sum, summary.filtered_cost_total, "RG NZD sum == filtered total");
        assert_eq!(rg_usd_sum, summary.filtered_cost_total_usd, "RG USD sum == filtered total USD");
    }

    /// Verify that when two bills are compared (latest − previous) the NZD and USD
    /// deltas are both computed correctly for each CostSource scenario:
    ///   Combined  — present in both bills → delta = latest − previous
    ///   Original  — only in latest        → full latest cost
    ///   Secondary — only in previous      → negated previous cost
    #[test]
    fn test_compare_bills_nzd_usd_deltas() {
        let latest_path = PathBuf::from("tests/azure_test_nzd_usd_latest.csv");
        let prev_path = PathBuf::from("tests/azure_test_nzd_usd_prev.csv");

        let mut latest_bills = crate::bills::Bills::default();
        latest_bills
            .parse_csv(&latest_path, &FILTER_OPTS)
            .expect("latest CSV parse failed");

        let mut prev_bills = crate::bills::Bills::default();
        prev_bills
            .parse_csv(&prev_path, &FILTER_OPTS)
            .expect("prev CSV parse failed");

        let filter = BillFilter::new(None, None, None, None, None, None, None, None, None, &FILTER_OPTS)
            .expect("valid test filter");
        let latest_summary = latest_bills.cost_by_any_summary(&filter);
        let prev_summary = prev_bills.cost_by_any_summary(&filter);

        // Simulate the merge/subtract from display.rs
        let mut merged = latest_summary.per_type;
        for (prev_key, prev_cost) in &prev_summary.per_type {
            merged
                .entry(prev_key.clone())
                .and_modify(|e| {
                    e.cost -= prev_cost.cost;
                    e.cost_usd -= prev_cost.cost_usd;
                    e.source = CostSource::Combined;
                })
                .or_insert(CostTotal {
                    cost: -prev_cost.cost,
                    cost_usd: -prev_cost.cost_usd,
                    cost_unreserved: -prev_cost.cost_unreserved,
                    source: CostSource::Secondary,
                });
        }

        let rg_key = (CostType::ResourceGroup, "rg-delta-test".to_string());
        let delta = merged.get(&rg_key).expect("rg-delta-test not found");
        assert!(
            matches!(delta.source, CostSource::Combined),
            "rg-delta-test should be Combined"
        );
        assert_eq!(delta.cost, Nzd(20.0), "NZD delta should be 20.0");
        assert_eq!(delta.cost_usd, Usd(12.0), "USD delta should be 12.0");

        // rg-new-only: only in latest → Original, full cost
        let new_key = (CostType::ResourceGroup, "rg-new-only".to_string());
        let new_item = merged.get(&new_key).expect("rg-new-only not found");
        assert!(
            matches!(new_item.source, CostSource::Original),
            "rg-new-only should be Original"
        );
        assert_eq!(new_item.cost, Nzd(50.0), "NZD should be 50.0");
        assert_eq!(new_item.cost_usd, Usd(30.0), "USD should be 30.0");

        // rg-gone-only: only in previous → Secondary, negated
        let gone_key = (CostType::ResourceGroup, "rg-gone-only".to_string());
        let gone_item = merged.get(&gone_key).expect("rg-gone-only not found");
        assert!(
            matches!(gone_item.source, CostSource::Secondary),
            "rg-gone-only should be Secondary"
        );
        assert_eq!(gone_item.cost, Nzd(-40.0), "NZD should be -40.0");
        assert_eq!(gone_item.cost_usd, Usd(-24.0), "USD should be -24.0");
    }
}
