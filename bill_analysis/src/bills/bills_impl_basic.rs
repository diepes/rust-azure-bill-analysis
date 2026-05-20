use crate::bills::Bills;
use crate::bills::bill_entry::BillEntry;
use crate::bills::summary::Summary;
use crate::money::{Nzd, Usd};
// use crate::bills::bills_struct::Bills;
use std::collections::HashMap;

// use summary::Summary;

impl Bills {
    pub fn remove(&mut self, other: Bills) {
        // ToDo: Probaly faulty, only drops matching values.
        //       Move logic into summary to use both bills.
        // create HashMap from other.bills to use as lookup from self.bills
        let b2: HashMap<&BillEntry, ()> = HashMap::from_iter(other.bills.iter().map(|b| (b, ())));
        // retain only the bills that are not in other.bills(b2) using hash lookup
        self.bills.retain(|x| !b2.contains_key(x));
    }

    pub fn calc_all_totals(&mut self) {
        let mut total_cost = Nzd::default();
        let mut total_cost_usd = Usd::default();
        let mut total_no_reservation = Usd::default();
        let mut total_effective = Usd::default();
        let mut total_savings_used = Usd::default();
        let mut total_savings_un_used = Usd::default();
        let mut total_savings_meter_category_map: HashMap<String, (Usd, Usd)> = HashMap::new();
        // Loop over all bills.
        for bill in &self.bills {
            total_cost += bill.cost;
            total_cost_usd += bill.cost_usd;
            total_no_reservation += Usd(bill.unit_price * bill.quantity);
            total_effective += Usd(bill.effective_price * bill.quantity);

            if !bill.reservation_name.is_empty() && bill.charge_type == "Usage" {
                total_savings_used += Usd((bill.unit_price - bill.effective_price) * bill.quantity);
                let entry = total_savings_meter_category_map
                    .entry(bill.meter_category.clone())
                    .or_insert((Usd::default(), Usd::default()));
                entry.0 += Usd((bill.unit_price - bill.effective_price) * bill.quantity);
            } else if bill.charge_type == "UnusedSavingsPlan"
                || bill.charge_type == "UnusedReservation"
            {
                total_savings_un_used += Usd(bill.effective_price * bill.quantity);
                let entry = total_savings_meter_category_map
                    .entry(bill.meter_category.clone())
                    .or_insert((Usd::default(), Usd::default()));
                entry.1 += Usd(bill.effective_price * bill.quantity);
            } else {
                // Reservation purchases and other non-usage charge types are
                // excluded from savings calculations.
                debug_assert!(
                    bill.reservation_name.is_empty()
                        || bill.charge_type == "Purchase"
                        || bill.charge_type == "Refund",
                    "calc savings_all_categories - Unexpected reservation_name:'{res}' charge_type:'{t}' category:'{cat}'",
                    res = bill.reservation_name,
                    t = bill.charge_type,
                    cat = bill.meter_category,
                );
            }
        }
        self.summary = Summary {
            total_cost,
            total_cost_usd,
            exchange_rate: if total_cost_usd.amount() != 0.0 {
                total_cost.amount() / total_cost_usd.amount()
            } else {
                0.0
            },
            total_no_reservation,
            total_effective,
            total_savings_used,
            total_savings_un_used,
            total_savings_meter_category_map,
        }
    }

    pub fn total_no_reservation(&self) -> Usd {
        self.bills.iter().fold(Usd::default(), |acc, bill| {
            acc + Usd(bill.unit_price * bill.quantity)
        })
    }
    pub fn total_effective(&self) -> Usd {
        self.bills.iter().fold(Usd::default(), |acc, bill| {
            acc + Usd(bill.effective_price * bill.quantity)
        })
    }
    // Function to calculte the total savings
    // https://learn.microsoft.com/en-us/azure/cost-management-billing/reservations/calculate-ea-reservations-savings
    pub fn total_used_savings(&self) -> Usd {
        self.bills.iter().fold(Usd::default(), |acc, bill| {
            if !bill.reservation_name.is_empty() && bill.charge_type == "Usage" {
                acc + Usd((bill.unit_price - bill.effective_price) * bill.quantity)
            } else {
                acc
            }
        })
    }
    pub fn total_unused_savings(&self) -> Usd {
        self.bills.iter().fold(Usd::default(), |acc, bill| {
            if bill.charge_type == "UnusedSavingsPlan" || bill.charge_type == "UnusedReservation" {
                acc + Usd(bill.effective_price * bill.quantity)
            } else {
                // skip and check assertions
                // Purchase and Refund charge types are non-usage charges, skip them.
                assert!(
                    bill.charge_type == "Usage"
                        || bill.charge_type == "RoundingAdjustment"
                        || bill.charge_type == "Purchase"
                        || bill.charge_type == "Refund",
                    "Unexpected charge_type '{}'",
                    bill.charge_type
                );
                acc
            }
        })
    }
    // Function to calculte the savings for meter_category
    // benefit_name != "" && charge_type == "Usage" && meter_category == Input then sum the (unit_price - effective_price) * quantity for each bill
    pub fn savings(&self, meter_category: &str) -> Usd {
        self.bills.iter().fold(Usd::default(), |acc, bill| {
            if !bill.benefit_name.is_empty()
                && bill.charge_type == "Usage"
                && bill.meter_category == meter_category
            {
                acc + Usd((bill.unit_price - bill.effective_price) * bill.quantity)
            } else {
                acc
            }
        })
    }
    pub fn savings_all_categories(&self) -> HashMap<&str, (Usd, Usd)> {
        let mut savings_map: HashMap<&str, (Usd, Usd)> = HashMap::new();
        for bill in &self.bills {
            if !bill.reservation_name.is_empty() && bill.charge_type == "Usage" {
                let entry = savings_map
                    .entry(&bill.meter_category)
                    .or_insert((Usd::default(), Usd::default()));
                entry.0 += Usd((bill.unit_price - bill.effective_price) * bill.quantity);
            } else if bill.charge_type == "UnusedSavingsPlan"
                || bill.charge_type == "UnusedReservation"
            {
                let entry = savings_map
                    .entry(&bill.meter_category)
                    .or_insert((Usd::default(), Usd::default()));
                entry.1 += Usd(bill.effective_price * bill.quantity);
            } else {
                // Reservation purchases and other non-usage charge types are
                // excluded from savings calculations.
                debug_assert!(
                    bill.reservation_name.is_empty()
                        || bill.charge_type == "Purchase"
                        || bill.charge_type == "Refund",
                    "calc savings_all_categories - Unexpected reservation_name:'{res}' charge_type:'{t}' category:'{cat}'",
                    res = bill.reservation_name,
                    t = bill.charge_type,
                    cat = bill.meter_category,
                );
            }
        }
        savings_map
    }
    // filter cost for specific resource e.g. disk
    pub fn cost_by_resource_name(&self, resource_name: &str) -> Nzd {
        self.bills.iter().fold(Nzd::default(), |acc, bill| {
            if bill.resource_name == resource_name {
                acc + bill.cost
            } else {
                acc
            }
        })
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

    /// Merge another `Bills` into `self`, appending all entries and recalculating totals.
    /// Used when combining multiple part CSVs from a single blob export into one dataset.
    pub fn extend_with(&mut self, other: Bills) {
        self.bills.extend(other.bills);
        self.tag_names.extend(other.tag_names);
        if self.billing_currency.is_none() {
            self.billing_currency = other.billing_currency;
        }
        self.calc_all_totals();
    }
}
