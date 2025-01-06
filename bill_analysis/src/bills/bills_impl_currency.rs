
// use std::collections::{HashMap, HashSet};
// use crate::bills::bill_entry::BillEntry;
use crate::bills::bills_struct::Bills;
use std::error::Error;

impl Bills {
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