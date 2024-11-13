use regex::Regex;
use serde::Deserialize;
use std::error::Error;
use std::fs::File;
use std::hash::Hash;
use std::path::{Path, PathBuf};

// 1brc speedup
use std::time::Instant;
// pub mod calc;
use crate::bill::bills::Bills;
use crate::bill::tags::Tags;

//struct to hold bill data for Azure detailed Enrollment csv parsed file
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
#[allow(unused)]
pub struct BillEntry {
    // SubscriptionId
    pub subscription_id: String,
    pub subscription_name: String,
    pub date: String,
    pub product: String,
    pub meter_id: String,
    pub meter_category: String,     // e.g. "Virtual Network"
    pub meter_sub_category: String, // e.g. "Peering"
    pub meter_name: String,         // e.g. "Intra-Region Ingress"
    pub quantity: f64,
    pub effective_price: f64,
    pub cost: f64,
    // BillingCurrency
    pub billing_currency: String,
    // UnitPrice,TotalUsedSavings,TotalUnused
    pub unit_price: f64,
    pub reservation_name: String,
    pub resource_id: String,
    pub resource_name: String,
    pub resource_group: String,
    // PlanName,ChargeType,Frequency
    pub publisher_name: String,
    pub plan_name: String,
    pub charge_type: String,
    pub frequency: String,
    // benefitId,benefitName
    #[serde(rename = "benefitId")]
    pub benefit_id: String,
    #[serde(rename = "benefitName")]
    pub benefit_name: String,
    pub tags: Tags,
}

macro_rules! lowercase_all_strings {
    ($struct:ident, $($field:ident),*) => {
        impl $struct {
            fn lowercase_all_strings(&mut self) {
                $(
                    self.$field = self.$field.to_lowercase();
                )*
            }
        }
    };
}
// Apply the macro to specify which fields are subject to lowercasing
lowercase_all_strings!(BillEntry, subscription_name, resource_group, resource_name);

impl BillEntry {
    // Function to parse the CSV file and return a vector of BillEntry structs
    pub fn parse_csv(
        file_path: &PathBuf,
        global_opts: &crate::GlobalOpts,
    ) -> Result<Bills, Box<dyn Error>> {
        let start = Instant::now();
        let file = File::open(Path::new(file_path))?;
        // 2024-06-23 tested mmap for faster read, no difference for 200k lines
        //let mmap = unsafe { memmap::MmapOptions::new().map(&file).unwrap() };
        //let mut reader = csv::Reader::from_reader(mmap.as_ref());
        let mut reader = csv::Reader::from_reader(file);
        let mut bills = Bills::default();
        // set file name
        bills.file_name = file_path
            .clone()
            .into_os_string()
            .into_string()
            .expect("Could not convert path to string ?");
        bills.file_short_name = extract_date_from_file_name(&bills.file_name);
        let mut lines = 0;
        for result in reader.deserialize() {
            let mut bill: BillEntry = result?;
            if !global_opts.case_sensitive {
                bill.lowercase_all_strings();
            }
            // handle empty RG - probably purchase
            if bill.resource_group.is_empty() {
                bill.resource_group = format!(
                    "BUY_{}_{}_{}",
                    bill.publisher_name.replace(" ", "-"),
                    bill.plan_name.replace(" ", "-"),
                    bill.charge_type
                );
            }
            bills.tag_names.extend(bill.tags.kv.keys().cloned());
            bills.push(bill);
            lines += 1;
        }
        bills.set_billing_currency()?;
        println!(
            "parse_csv {lines} lines in {:.3}s",
            start.elapsed().as_secs_f64()
        );

        Ok(bills)
    }
}
fn extract_date_from_file_name(file_path: &str) -> String {
    // Define the regex pattern to match a date of the format _YYYYMM_
    let re = Regex::new(r"_(\d{6})_").unwrap();

    // Attempt to find a match for the date
    if let Some(caps) = re.captures(file_path) {
        caps[1].to_string() // Return the matched date (group 1)
    } else {
        file_path.to_string() // Return None if no match is found
    }
}

// Implement Eq for BillEntry
impl Eq for BillEntry {}
impl PartialEq for BillEntry {
    // Implement PartialEq for BillEntry by comparing some fields
    fn eq(&self, other: &Self) -> bool {
        self.subscription_id == other.subscription_id
            // && self.subscription_name == other.subscription_name
            // && self.product == other.product
            // meter settings changed 2024-05 
            // && self.meter_id == other.meter_id
            // && self.meter_category == other.meter_category
            // && self.meter_sub_category == other.meter_sub_category
            // && self.meter_name == other.meter_name
            // && self.quantity == other.quantity
            // && self.billing_currency == other.billing_currency
            && self.resource_id == other.resource_id
            // && self.resource_name == other.resource_name
            && self.resource_group == other.resource_group
    }
}
// Implement Hash for BillEntry
impl Hash for BillEntry {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.subscription_id.hash(state);
        // self.subscription_name.hash(state);
        // self.product.hash(state);
        // self.meter_id.hash(state);
        // self.meter_category.hash(state);
        // self.meter_sub_category.hash(state);
        // self.meter_name.hash(state);
        // self.quantity.hash(state);
        // self.billing_currency.hash(state);
        self.resource_id.hash(state);
        // self.resource_name.hash(state);
        // self.resource_group.hash(state);
    }
}

#[cfg(test)]
mod tests {
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
