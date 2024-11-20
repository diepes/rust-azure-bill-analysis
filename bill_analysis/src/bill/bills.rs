use crate::bill::billentry::BillEntry;
use crate::bill::billsummary::{CostSource, CostTotal};
use crate::bill::costtype::CostType;
use regex::{Regex, RegexBuilder};
use std::collections::{HashMap, HashSet};
use std::error::Error;

use super::billsummary::SummaryData;

pub struct Bills {
    pub bills: Vec<BillEntry>,
    pub billing_currency: Option<String>,
    pub tag_names: HashSet<String>,
    pub file_name: String,
    pub file_short_name: String,
}
impl Default for Bills {
    fn default() -> Self {
        Self {
            bills: Vec::new(),
            billing_currency: None,
            tag_names: HashSet::new(),
            file_name: "NotSet".to_string(),
            file_short_name: "NotSet".to_string(),
        }
    }
}
impl Bills {
    pub fn remove(&mut self, other: Bills) {
        // ToDo: Probaly faulty, only drops matching values.
        //       Move logic into summary to use both bills.
        // create HashMap from other.bills to use as lookup from self.bills
        let b2: HashMap<&BillEntry, ()> = HashMap::from_iter(other.bills.iter().map(|b| (b, ())));
        // retain only the bills that are not in other.bills(b2) using hash lookup
        self.bills.retain(|x| !b2.contains_key(x));
    }

    pub fn total_no_reservation(&self) -> f64 {
        self.bills
            .iter()
            .fold(0.0, |acc, bill| acc + bill.unit_price * bill.quantity)
    }
    pub fn total_effective(&self) -> f64 {
        self.bills
            .iter()
            .fold(0.0, |acc, bill| acc + bill.effective_price * bill.quantity)
    }
    // Function to calculte the total savings
    // https://learn.microsoft.com/en-us/azure/cost-management-billing/reservations/calculate-ea-reservations-savings
    pub fn total_used_savings(&self) -> f64 {
        self.bills.iter().fold(0.0, |acc, bill| {
            if !bill.reservation_name.is_empty() && bill.charge_type == "Usage" {
                acc + (bill.unit_price - bill.effective_price) * bill.quantity
                // assert_eq!(bill.pricing_model, "Reservation", "pricing_model mismatch");
            } else {
                acc
            }
        })
    }
    pub fn total_unused_savings(&self) -> f64 {
        // In the billing data there is a charge_type "UnusedReservation" for every "date" and reservation.
        self.bills.iter().fold(0.0, |acc, bill| {
            if bill.charge_type == "UnusedSavingsPlan" || bill.charge_type == "UnusedReservation" {
                acc + bill.effective_price * bill.quantity
            } else {
                acc
            }
        })
    }
    // Function to calculte the savings for meter_category
    // benefit_name != "" && charge_type == "Usage" && meter_category == Input then sum the (unit_price - effective_price) * quantity for each bill
    pub fn savings(&self, meter_category: &str) -> f64 {
        self.bills.iter().fold(0.0, |acc, bill| {
            if !bill.benefit_name.is_empty()
                && bill.charge_type == "Usage"
                && bill.meter_category == meter_category
            {
                acc + (bill.unit_price - bill.effective_price) * bill.quantity
            } else {
                acc
            }
        })
    }
    // filter cost for specific resource e.g. disk
    pub fn cost_by_resource_name(&self, resource_name: &str) -> f64 {
        self.bills.iter().fold(0.0, |acc, bill| {
            // bill.subscription_name == resource_group &&
            if bill.resource_name == resource_name {
                acc + bill.cost
            } else {
                acc
            }
        })
    }

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
        region_regex: &str,
        tag_summarize: &str,
        tag_filter: &str,
        global_opts: &crate::GlobalOpts,
    ) -> SummaryData {
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
        let re_region = RegexBuilder::new(region_regex)
            .case_insensitive(!global_opts.case_sensitive)
            .build()
            .expect("Invalid regex for region");
        let re_tag = RegexBuilder::new(tag_filter)
            .case_insensitive(!global_opts.case_sensitive)
            .build()
            .expect("Invalid regex for tag");
        // collect set of resource groups in set rgs
        let mut summary_data = SummaryData::default();
        // bill_details record cost per filter category e.g. name_regex, rg_regex, subs_regex, meter_category
        // per_type
        // iter through bills, get total and update new bill_details for each category.
        let filtered_total = self.bills.iter().fold(0.0, |acc, bill| {
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
            } else if match (
                region_regex,
                re_region.is_match(&bill.meter_region),
                bill.meter_region.len(),
            ) {
                ("any", _, _) => false,
                ("", _, 1..) => true,
                (_, true, _) => false,
                (_, false, _) => true,
            } {
                flag_match = false;
            }
            if flag_match {
                // if all match
                // record cost against resource_name, resource_group, subscription_name, meter_category, tag
                summary_data
                    .per_type
                    .entry((CostType::ResourceName, bill.resource_name.clone()))
                    .and_modify(|e| {
                        e.cost += bill.cost;
                        // e.source = CostSource::Original;
                    })
                    .or_insert(CostTotal {
                        cost: bill.cost,
                        source: CostSource::Original,
                    });

                summary_data
                    .per_type
                    .entry((CostType::ResourceGroup, bill.resource_group.clone()))
                    .and_modify(|e| {
                        e.cost += bill.cost;
                    })
                    .or_insert(CostTotal {
                        cost: bill.cost,
                        source: CostSource::Original,
                    });

                summary_data
                    .per_type
                    .entry((CostType::Subscription, bill.subscription_name.clone()))
                    .and_modify(|e| {
                        e.cost += bill.cost;
                    })
                    .or_insert(CostTotal {
                        cost: bill.cost,
                        source: CostSource::Original,
                    });

                summary_data
                    .per_type
                    .entry((CostType::MeterCategory, bill.meter_category.clone()))
                    .and_modify(|e| {
                        e.cost += bill.cost;
                    })
                    .or_insert(CostTotal {
                        cost: bill.cost,
                        source: CostSource::Original,
                    });

                // add bill_details for tags, using the matched tag and value
                if !tag_summarize.is_empty() {
                    let tag_summarize_lowercase = if global_opts.case_sensitive {
                        tag_filter.to_string()
                    } else {
                        tag_summarize.to_lowercase()
                    };
                    if bill.tags.kv.contains_key(&tag_summarize_lowercase) {
                        // from lowercase tag_summarize get the value and original key(Original case)
                        let v = bill.tags.kv.get(&tag_summarize_lowercase).unwrap();
                        summary_data
                            .per_type
                            .entry((CostType::Tag, format!("tag:{}={}", v.1, v.0)))
                            .and_modify(|e| {
                                e.cost += bill.cost;
                            })
                            .or_insert(CostTotal {
                                cost: bill.cost,
                                source: CostSource::Original,
                            });
                    } else {
                        //no tag found
                        summary_data
                            .per_type
                            .entry((CostType::Tag, "tag:none".to_string()))
                            .and_modify(|e| {
                                e.cost += bill.cost;
                            })
                            .or_insert(CostTotal {
                                cost: bill.cost,
                                source: CostSource::Original,
                            });
                    }
                }

                summary_data.details.insert(format!(
                    "{rg}_____{rn}_____{mc}",
                    rg = bill.resource_group.clone(),
                    mc = bill.meter_category.clone(),
                    rn = bill.resource_name.clone(),
                ));
                acc + bill.cost
            } else {
                acc
            }
        });
        // bill_details should have same cost total for each category
        summary_data.filtered_cost_total = filtered_total;
        summary_data
    }

    /// Similar to cost_by_resource_group, for cost_by_subscription
    /// returns the total cost of all bills in the subscription and a set of all subscription names matched.
    pub fn cost_by_subscription(
        &self,
        subscription_name: &str,
    ) -> (f64, std::collections::HashSet<String>) {
        let re_subs = Regex::new(subscription_name).unwrap();
        // collect set of resource groups in set rgs
        let mut subs = std::collections::HashSet::new();
        let bill = self.bills.iter().fold(0.0, |acc, bill| {
            if re_subs.is_match(&bill.subscription_name) {
                subs.insert(bill.subscription_name.clone());
                acc + bill.cost
            } else {
                acc
            }
        });
        (bill, subs)
    }

    pub fn len(&self) -> usize {
        self.bills.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bills.is_empty()
    }

    pub fn push(&mut self, bill: BillEntry) {
        self.bills.push(bill);
    }

    // Function to get the BillingCurrency by ensuring all BillingCurrency fields are the same and saving the value in Option<billing_currency>
    pub fn set_billing_currency(&mut self) -> Result<String, Box<dyn Error>> {
        if self.billing_currency.is_some() {
            Ok(self.billing_currency.as_ref().unwrap().clone())
        } else {
            let currency = &self.bills[0].billing_currency;
            for bill in &self.bills {
                if &bill.billing_currency != currency {
                    return Err("Billing Currency mismatch".into());
                }
            }
            self.billing_currency = Some(currency.clone());
            Ok(currency.clone())
        }
    }

    pub fn get_billing_currency(&self) -> String {
        self.billing_currency.as_ref().unwrap().clone()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::cmd_parse::GlobalOpts;

    use super::*;

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
        let result = BillEntry::parse_csv(&file_name, &global_opts);
        // Assert that parsing was successful
        assert!(
            result.is_ok(),
            "!Error parsing the file:'{file_name:?}'\nERR:{}",
            result.err().unwrap()
        );
        // Get the parsed bills
        let bills = result.unwrap();

        // Test the cost_by_resource_name function
        let cost = bills.cost_by_resource_name("NLSYDWAVAP01P-OSdisk-00_ide_0_869850_GXMD_40cfb0");
        assert_eq!(cost, 0.002785917);
    }
    #[test]
    fn test_parse_csv() {
        let global_opts = &GLOBAL_OPTS;
        let file_name: PathBuf = PathBuf::from("tests/azure_test_data_01.csv");
        // Test file path
        let file_path = &file_name;

        // Parse the CSV file
        let result = BillEntry::parse_csv(file_path, &global_opts);

        // Assert that parsing was successful
        assert!(
            result.is_ok(),
            "!Error parsing the file:'{file_name:?}'\nERR:{}",
            result.err().unwrap()
        );

        // Get the parsed bills
        let bills = result.unwrap().bills;

        // Assert that the number of bills is correct
        assert_eq!(bills.len(), 8);

        // Assert the values of the first bill
        let first_bill = &bills[0];
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
        assert_eq!(first_bill.quantity, (0.194368534), "quantity mismatch");
        assert_eq!(first_bill.cost, (0.003025655), "cost mismatch");
    }
}
