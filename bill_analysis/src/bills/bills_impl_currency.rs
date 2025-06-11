// use std::collections::{HashMap, HashSet};
// use crate::bills::bill_entry::BillEntry;
use crate::bills::Bills;
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
        let mut cur = self.billing_currency.as_ref().unwrap().clone();
        if cur == "USD" {
            cur = "US$".to_string();
        }
        if cur == "NZD" {
            cur = "NZ$".to_string();
        }
        if cur == "AUD" {
            cur = "AU$".to_string();
        }
        cur
    }
}
