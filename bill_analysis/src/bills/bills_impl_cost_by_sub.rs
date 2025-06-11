//use std::collections::{HashMap, HashSet};
//use crate::bills::bill_entry::BillEntry;
use crate::bills::Bills;
use regex::Regex;

impl Bills {
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
}
