use regex::Regex;
use serde::Deserialize;
use std::hash::Hash;

// 1brc speedup
// pub mod calc;
use crate::bills::tags::Tags;

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
    pub meter_region: String,
    pub quantity: f64,
    pub effective_price: f64, // blended rate across tiers, actual rate, could be 0 for reservations
    pub cost: f64,
    // BillingCurrency
    pub billing_currency: String,
    // UnitPrice,TotalUsedSavings,TotalUnused
    pub unit_price: f64, // per-unit price at time of billing inc. negotiated discounts
    pub reservation_name: String,
    pub resource_id: String,
    pub resource_name: String,
    pub resource_group: String,
    pub resource_location: String,
    // PlanName,ChargeType,Frequency
    pub publisher_name: String,
    pub plan_name: String,
    pub charge_type: String,
    pub frequency: String,
    pub pricing_model: String,
    // benefitId,benefitName
    #[serde(rename = "benefitId")]
    pub benefit_id: String,
    #[serde(rename = "benefitName")]
    pub benefit_name: String,
    pub tags: Tags,
    #[serde(skip_deserializing)]
    pub line_number_csv: usize, // line number in csv file, added for debugging
}

// Apply the macro to specify which fields are subject to lowercasing
macro_rules! lowercase_all_strings {
    ($struct:ident, $($field:ident),*) => {
        impl $struct {
            pub fn lowercase_all_strings(&mut self) {
                $(
                    self.$field = self.$field.to_lowercase();
                )*
            }
        }
    };
}
lowercase_all_strings!(
    BillEntry,
    subscription_name,
    resource_group,
    resource_name,
    tags
);
//

pub fn extract_date_from_file_name(file_path: &str) -> String {
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
