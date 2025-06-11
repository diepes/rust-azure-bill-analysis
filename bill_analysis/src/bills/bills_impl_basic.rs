use crate::bills::bill_entry::BillEntry;
use crate::bills::summary::Summary;
use crate::bills::Bills;
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

    pub fn calc_all_totals(&mut self)
    {
        let mut total_cost = 0.0;
        let mut total_no_reservation = 0.0;
        let mut total_effective = 0.0;
        let mut total_savings_used = 0.0;
        let mut total_savings_un_used = 0.0;
        let mut total_savings_meter_category_map: HashMap<String, (f64, f64)> = HashMap::new();
        // Loop over all bills.
        for bill in &self.bills {
            total_cost += bill.cost;
            total_no_reservation += bill.unit_price * bill.quantity;
            total_effective += bill.effective_price * bill.quantity;

            //if !bill.meter_category.is_empty() && bill.charge_type == "Usage" {
            if !bill.reservation_name.is_empty() && bill.charge_type == "Usage" {
                total_savings_used += (bill.unit_price - bill.effective_price) * bill.quantity;
                let entry = total_savings_meter_category_map
                    .entry(bill.meter_category.clone())
                    .or_insert((0.0, 0.0));
                entry.0 += (bill.unit_price - bill.effective_price) * bill.quantity;
            } else if bill.charge_type == "UnusedSavingsPlan"
                || bill.charge_type == "UnusedReservation"
            {
                total_savings_un_used += bill.effective_price * bill.quantity;
                let entry = total_savings_meter_category_map
                    .entry(bill.meter_category.clone())
                    .or_insert((0.0, 0.0));
                entry.1 += (bill.effective_price) * bill.quantity;
            } else {
                // assert reservation_name is empty
                assert!(
                        bill.reservation_name.is_empty(),
                        "calc savings_all_categories - Unexpected reservation_name:'{res}' charge_type:'{t}' category:'{cat}'",
                        res = bill.reservation_name,
                        t = bill.charge_type,
                        cat = bill.meter_category,
                    );
            }
        }
        self.summary = Summary {
            total_cost,
            total_no_reservation,
            total_effective,
            total_savings_used,
            total_savings_un_used,
            total_savings_meter_category_map,
        }
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
            } else {
                acc
            }
        })
    }
    pub fn total_unused_savings(&self) -> f64 {
        // In the billing data there is a charge_type "UnusedReservation" for every "date" and reservation.
        self.bills.iter().fold(0.0, |acc, bill| {
            // bill.charge_type != "Usage"
            if bill.charge_type == "UnusedSavingsPlan" || bill.charge_type == "UnusedReservation" {
                acc + bill.effective_price * bill.quantity
            } else {
                // skip and check assertions
                // Asset to catch unknown charge_type, we did find "Purchase" at $0, ignore.
                if !(bill.effective_price == 0.0 && bill.unit_price == 0.0) {
                    // assert bill.charge_type one of "Usage", "RoundingAdjustment"
                    assert!(
                        bill.charge_type == "Usage" || bill.charge_type == "RoundingAdjustment",
                        "Unexpected charge_type '{}'",
                        bill.charge_type
                    );
                };
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
    // calculate savings and unused savings for all categories
    pub fn savings_all_categories(&self) -> HashMap<&str, (f64, f64)> {
        let mut savings_map: HashMap<&str, (f64, f64)> = HashMap::new();
        for bill in &self.bills {
            //if !bill.meter_category.is_empty() && bill.charge_type == "Usage" {
            if !bill.reservation_name.is_empty() && bill.charge_type == "Usage" {
                let entry = savings_map
                    .entry(&bill.meter_category)
                    .or_insert((0.0, 0.0));
                entry.0 += (bill.unit_price - bill.effective_price) * bill.quantity;
            } else if bill.charge_type == "UnusedSavingsPlan"
                || bill.charge_type == "UnusedReservation"
            {
                let entry = savings_map
                    .entry(&bill.meter_category)
                    .or_insert((0.0, 0.0));
                entry.1 += (bill.effective_price) * bill.quantity;
            } else {
                // assert reservation_name is empty
                assert!(
                    bill.reservation_name.is_empty(),
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

    pub fn len(&self) -> usize {
        self.bills.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bills.is_empty()
    }

    pub fn push(&mut self, bill: BillEntry) {
        self.bills.push(bill);
    }
}
