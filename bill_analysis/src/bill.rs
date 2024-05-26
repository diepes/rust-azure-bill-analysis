use csv::Reader;
use serde::Deserialize;
use std::error::Error;
use std::fs::File;
use std::path::{Path, PathBuf};

//struct to hold bill data for Azure detailed Enrollment csv parsed file
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(unused)]
pub struct BillEntry {
    // SubscriptionId
    subscription_id: String,
    subscription_name: String,
    date: String,
    product: String,
    meter_id: String,
    meter_category: String,     // e.g. "Virtual Network"
    meter_sub_category: String, // e.g. "Peering"
    meter_name: String,         // e.g. "Intra-Region Ingress"
    quantity: f64,
    effective_price: f64,
    cost: f64,
    // BillingCurrency
    billing_currency: String,
    // UnitPrice,TotalUsedSavings,TotalUnused
    unit_price: f64,
    reservation_name: String,
    resource_id: String,
    resource_name: String,
    // PlanName,ChargeType,Frequency
    plan_name: String,
    charge_type: String,
    frequency: String,
    // benefitId,benefitName
    #[serde(rename = "benefitId")]
    benefit_id: String,
    #[serde(rename = "benefitName")]
    benefit_name: String,
}

impl BillEntry {
    // Function to parse the CSV file and return a vector of BillEntry structs
    pub fn parse_csv(file_path: &PathBuf) -> Result<Bills, Box<dyn Error>> {
        let file = File::open(Path::new(file_path))?;
        let mut reader = Reader::from_reader(file);

        let mut bills = Bills::default();

        for result in reader.deserialize() {
            let bill: BillEntry = result?;
            bills.push(bill);
        }
        bills.set_billing_currency()?;

        Ok(bills)
    }
}

pub struct Bills {
    bills: Vec<BillEntry>,
    billing_currency: Option<String>,
}
impl Bills {
    fn default() -> Self {
        Self {
            bills: Vec::new(),
            billing_currency: None,
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
    // benefit_name != "" && charge_type == "Usage" then sum the (unit_price - effective_price) * quantity for each bill
    // https://learn.microsoft.com/en-us/azure/cost-management-billing/reservations/calculate-ea-reservations-savings
    pub fn total_used_savings(&self) -> f64 {
        self.bills.iter().fold(0.0, |acc, bill| {
            // if bill.benefit_name != "" && bill.charge_type == "Usage" {
            if !bill.reservation_name.is_empty() && bill.charge_type == "Usage" {
                acc + (bill.unit_price - bill.effective_price) * bill.quantity
            } else {
                acc
            }
        })
    }
    pub fn total_unused_savings(&self) -> f64 {
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
    pub fn cost_by_resource_name(&self, resource_name: &str) -> f64 {
        self.bills.iter().fold(0.0, |acc, bill| {
            // bill.subscription_name == resource_group &&
            if  bill.resource_name == resource_name {
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

    fn push(&mut self, bill: BillEntry) {
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
    use super::*;

    #[test]
    fn test_cost_by_resource_name() {
        let file_name: PathBuf = PathBuf::from("tests/azure_test_data_01.csv");
        let result = BillEntry::parse_csv(&file_name);
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
        let file_name: PathBuf = PathBuf::from("tests/azure_test_data_01.csv");
        // Test file path
        let file_path = &file_name;

        // Parse the CSV file
        let result = BillEntry::parse_csv(file_path);

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
