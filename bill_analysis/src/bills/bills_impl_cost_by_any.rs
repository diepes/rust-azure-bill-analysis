use crate::bills::Bills;

use crate::bills::bills_sum_data::{CostSource, CostTotal};
use crate::bills::cost_type_enum::CostType;
// use crate::bills::ReservationInfo;
use crate::bills::bills_sum_data::SummaryData;
use crate::money::{Nzd, Usd};
// use crate::RESERVATION_SUMMARY;
use regex::RegexBuilder; // lib.rs static RESERVATION_SUMMARY

impl Bills {
    // function cost_by_any
    // takes name_regex, rg_regex, subs_regex, meter_category, tag_regex and returns total where all match,
    //   if empty str in input it is skiped for filter.
    // returns total_filtered_cost,
    //         set of filtered resource groups,
    //     and HashMap of filtered cost per category(each category total - total filtered cost)
    pub fn cost_by_any_summary(
        &self,
        name_regex: &str,
        rg_regex: &str,
        subs_regex: &str,
        meter_category: &str,
        location_regex: &str,
        reservation_regex: &str,
        tag_summarise: &str,
        tag_filter: &str,
        invoice_section_regex: &str,
        global_opts: &crate::GlobalOpts,
    ) -> SummaryData<'_> {
        let re_name = RegexBuilder::new(name_regex)
            .case_insensitive(!global_opts.case_sensitive)
            .build()
            .expect("Invalid regex for name");
        let re_rg = RegexBuilder::new(rg_regex)
            .case_insensitive(!global_opts.case_sensitive)
            .build()
            .expect("Invalid regex for resource group");
        let re_subs = RegexBuilder::new(subs_regex)
            .case_insensitive(!global_opts.case_sensitive)
            .build()
            .expect("Invalid regex for subscription");
        let re_type = RegexBuilder::new(meter_category)
            .case_insensitive(!global_opts.case_sensitive)
            .build()
            .expect("Invalid regex for meter category");
        let re_location = RegexBuilder::new(location_regex)
            .case_insensitive(!global_opts.case_sensitive)
            .build()
            .expect("Invalid regex for location/region");
        let re_reservation = RegexBuilder::new(reservation_regex)
            .case_insensitive(!global_opts.case_sensitive)
            .build()
            .expect("Invalid regex for reservation");
        let re_tag = RegexBuilder::new(tag_filter)
            .case_insensitive(!global_opts.case_sensitive)
            .build()
            .expect("Invalid regex for tag");
        let re_invoice_section = RegexBuilder::new(invoice_section_regex)
            .case_insensitive(!global_opts.case_sensitive)
            .build()
            .expect("Invalid regex for invoice section");
        // collect set of resource groups in set rgs
        let mut summary_data = SummaryData::default();
        // bill_details record cost per filter category e.g. name_regex, rg_regex, subs_regex, meter_category
        // per_type
        // iter through bills, get total and update new bill_details for each category.
        let filtered_total = self.bills.iter().fold((Nzd::default(), Usd::default()), |acc, bill| {
            let mut flag_match = true;
            if !name_regex.is_empty() && !re_name.is_match(&bill.resource_name) {
                flag_match = false; // if filter set and no match skip
            } else if !rg_regex.is_empty() && !re_rg.is_match(&bill.resource_group) {
                flag_match = false;
            } else if !subs_regex.is_empty() && !re_subs.is_match(&bill.subscription_name) {
                flag_match = false;
            } else if !meter_category.is_empty() && !re_type.is_match(&bill.meter_category) {
                flag_match = false;
            // Check tags hashmap for match
            } else if !tag_filter.is_empty() && !re_tag.is_match(&bill.tags.value) {
                flag_match = false;
            } else if !reservation_regex.is_empty() && !re_reservation.is_match(&bill.benefit_name)
            {
                flag_match = false;
            } else if !invoice_section_regex.is_empty()
                && !re_invoice_section.is_match(&bill.invoice_section)
            {
                flag_match = false;
            } else if match (
                location_regex,
                location_regex.len(),
                re_location.is_match(&bill.resource_location),
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

                summary_data
                    .per_type
                    .entry((CostType::ResourceName, bill.resource_name.clone()))
                    .and_modify(|e| {
                        e.cost += bill.cost;
                        e.cost_usd += bill.cost_usd;
                        e.cost_unreserved += cost_unreserved;
                    })
                    .or_insert(CostTotal {
                        cost: bill.cost,
                        cost_usd: bill.cost_usd,
                        cost_unreserved: cost_unreserved,
                        source: CostSource::Original,
                    });

                summary_data
                    .per_type
                    .entry((CostType::ResourceGroup, bill.resource_group.clone()))
                    .and_modify(|e| {
                        e.cost += bill.cost;
                        e.cost_usd += bill.cost_usd;
                        e.cost_unreserved += cost_unreserved;
                    })
                    .or_insert(CostTotal {
                        cost: bill.cost,
                        cost_usd: bill.cost_usd,
                        cost_unreserved: cost_unreserved,
                        source: CostSource::Original,
                    });

                summary_data
                    .per_type
                    .entry((CostType::Subscription, bill.subscription_name.clone()))
                    .and_modify(|e| {
                        e.cost += bill.cost;
                        e.cost_usd += bill.cost_usd;
                        e.cost_unreserved += cost_unreserved;
                    })
                    .or_insert(CostTotal {
                        cost: bill.cost,
                        cost_usd: bill.cost_usd,
                        cost_unreserved: cost_unreserved,
                        source: CostSource::Original,
                    });

                summary_data
                    .per_type
                    .entry((CostType::MeterCategory, bill.meter_category.clone()))
                    .and_modify(|e| {
                        e.cost += bill.cost;
                        e.cost_usd += bill.cost_usd;
                        e.cost_unreserved += cost_unreserved;
                    })
                    .or_insert(CostTotal {
                        cost: bill.cost,
                        cost_usd: bill.cost_usd,
                        cost_unreserved: cost_unreserved,
                        source: CostSource::Original,
                    });

                summary_data
                    .per_type
                    .entry((CostType::Reservation, bill.benefit_name.clone()))
                    .and_modify(|e| {
                        e.cost += bill.cost;
                        e.cost_usd += bill.cost_usd;
                        e.cost_unreserved += cost_unreserved;
                    })
                    .or_insert(CostTotal {
                        cost: bill.cost,
                        cost_usd: bill.cost_usd,
                        cost_unreserved: cost_unreserved,
                        source: CostSource::Original,
                    });

                let region = if bill.resource_location.is_empty() { "none" } else { &bill.resource_location };
                summary_data
                    .per_type
                    .entry((CostType::Region, region.to_string()))
                    .and_modify(|e| {
                        e.cost += bill.cost;
                        e.cost_usd += bill.cost_usd;
                        e.cost_unreserved += cost_unreserved;
                    })
                    .or_insert(CostTotal {
                        cost: bill.cost,
                        cost_usd: bill.cost_usd,
                        cost_unreserved: cost_unreserved,
                        source: CostSource::Original,
                    });

                let section = if !bill.invoice_section.is_empty() {
                    bill.invoice_section.clone()
                } else if !bill.meter_sub_category.is_empty() {
                    format!("({})", bill.meter_sub_category)
                } else {
                    "none".to_string()
                };
                summary_data
                    .per_type
                    .entry((CostType::InvoiceSection, section.to_string()))
                    .and_modify(|e| {
                        e.cost += bill.cost;
                        e.cost_usd += bill.cost_usd;
                        e.cost_unreserved += cost_unreserved;
                    })
                    .or_insert(CostTotal {
                        cost: bill.cost,
                        cost_usd: bill.cost_usd,
                        cost_unreserved: cost_unreserved,
                        source: CostSource::Original,
                    });

                // add bill_details for tags, using the matched tag and value
                if !tag_summarise.is_empty() {
                    let tag_summarize_lowercase = if global_opts.case_sensitive {
                        tag_summarise.to_string()
                    } else {
                        tag_summarise.to_lowercase()
                    };
                    if bill.tags.kv.contains_key(&tag_summarize_lowercase) {
                        // from lowercase tag_summarise get the value and original key(Original case)
                        let v = bill.tags.kv.get(&tag_summarize_lowercase).unwrap();
                        summary_data
                            .per_type
                            .entry((CostType::Tag, format!("tag:{}={}", v.1, v.0)))
                            .and_modify(|e| {
                                e.cost += bill.cost;
                        e.cost_usd += bill.cost_usd;
                                e.cost_unreserved += cost_unreserved;
                            })
                            .or_insert(CostTotal {
                                cost: bill.cost,
                        cost_usd: bill.cost_usd,
                                cost_unreserved: cost_unreserved,
                                source: CostSource::Original,
                            });
                    } else {
                        //no tag found
                        summary_data
                            .per_type
                            .entry((CostType::Tag, "tag:none".to_string()))
                            .and_modify(|e| {
                                e.cost += bill.cost;
                        e.cost_usd += bill.cost_usd;
                                e.cost_unreserved += cost_unreserved;
                            })
                            .or_insert(CostTotal {
                                cost: bill.cost,
                        cost_usd: bill.cost_usd,
                                cost_unreserved: cost_unreserved,
                                source: CostSource::Original,
                            });
                    }
                } // end tag_summarise

                    summary_data
                        .per_type
                        .entry((CostType::MeterSubCategory, format!("{}__{}", bill.meter_category, bill.meter_sub_category)))
                        .and_modify(|e| {
                            e.cost += bill.cost;
                        e.cost_usd += bill.cost_usd;
                            e.cost_unreserved += cost_unreserved;
                        })
                        .or_insert(CostTotal {
                            cost: bill.cost,
                        cost_usd: bill.cost_usd,
                            cost_unreserved: cost_unreserved,
                            source: CostSource::Original,
                        });

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

    use crate::bills::bills_sum_data::{CostSource, CostTotal};
    use crate::bills::cost_type_enum::CostType;
    use crate::cmd_parse::GlobalOpts;
    use crate::money::{Nzd, Usd};

    // use super::*;

    static GLOBAL_OPTS: GlobalOpts = crate::GlobalOpts {
        debug: false,
        bill_path: None,
        bill_prev_subtract_path: None,
        cost_min_display: 10.0,
        case_sensitive: true,
        tag_list: false,
    };

    #[test]
    fn test_cost_by_resource_name() {
        let global_opts = &GLOBAL_OPTS;
        let file_name: PathBuf = PathBuf::from("tests/azure_test_data_01.csv");
        let mut bills = crate::bills::Bills::default();
        let result = bills.parse_csv(&file_name, global_opts);
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
        let global_opts = &GLOBAL_OPTS;
        let file_name: PathBuf = PathBuf::from("tests/azure_test_data_01.csv");
        let mut bills = crate::bills::Bills::default();
        let result = bills.parse_csv(&file_name, global_opts);
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
        assert_eq!(first_bill.date, "03/08/2024", "date mismatch");
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

    /// Verify that when two bills are compared (latest − previous) the NZD and USD
    /// deltas are both computed correctly for each CostSource scenario:
    ///   Combined  — present in both bills → delta = latest − previous
    ///   Original  — only in latest        → full latest cost
    ///   Secondary — only in previous      → negated previous cost
    #[test]
    fn test_compare_bills_nzd_usd_deltas() {
        let global_opts = &GLOBAL_OPTS;
        let latest_path = PathBuf::from("tests/azure_test_nzd_usd_latest.csv");
        let prev_path = PathBuf::from("tests/azure_test_nzd_usd_prev.csv");

        let mut latest_bills = crate::bills::Bills::default();
        latest_bills
            .parse_csv(&latest_path, global_opts)
            .expect("latest CSV parse failed");

        let mut prev_bills = crate::bills::Bills::default();
        prev_bills
            .parse_csv(&prev_path, global_opts)
            .expect("prev CSV parse failed");

        let no_filter = "";
        let latest_summary = latest_bills.cost_by_any_summary(
            no_filter, no_filter, no_filter, no_filter, no_filter, no_filter, no_filter,
            no_filter, no_filter, global_opts,
        );
        let prev_summary = prev_bills.cost_by_any_summary(
            no_filter, no_filter, no_filter, no_filter, no_filter, no_filter, no_filter,
            no_filter, no_filter, global_opts,
        );

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
