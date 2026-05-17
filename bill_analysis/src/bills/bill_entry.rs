use regex::Regex;
use serde::{Deserialize, Deserializer};
use std::hash::Hash;

use crate::bills::tags::Tags;
use crate::money::{Nzd, Usd};

//struct to hold bill data for Azure detailed Enrollment csv parsed file
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(unused)]
pub struct BillEntry {
    #[serde(alias = "SubscriptionId")]
    pub subscription_id: String,
    #[serde(alias = "SubscriptionName")]
    pub subscription_name: String,
    #[serde(alias = "Date", deserialize_with = "deserialize_date")]
    pub date: String,
    #[serde(alias = "Product")]
    pub product: String,
    #[serde(alias = "MeterId")]
    pub meter_id: String,
    #[serde(alias = "MeterCategory")]
    pub meter_category: String,
    #[serde(alias = "MeterSubCategory")]
    pub meter_sub_category: String,
    #[serde(alias = "MeterName")]
    pub meter_name: String,
    #[serde(alias = "MeterRegion")]
    pub meter_region: String,
    #[serde(alias = "Quantity")]
    pub quantity: f64,
    #[serde(alias = "EffectivePrice")]
    pub effective_price: f64,
    // Old format: "Cost", new format: "costInBillingCurrency" (NZD)
    #[serde(rename = "costInBillingCurrency", alias = "Cost")]
    pub cost: Nzd,
    // USD cost — not present in old format test data, defaults to 0
    #[serde(default, rename = "costInUsd")]
    pub cost_usd: Usd,
    // PAYG (list-price) costs — for savings calculations
    #[serde(default, rename = "paygCostInBillingCurrency")]
    pub payg_cost_nzd: Nzd,
    #[serde(default, rename = "paygCostInUsd")]
    pub payg_cost_usd: Usd,
    #[serde(alias = "BillingCurrency")]
    pub billing_currency: String,
    #[serde(alias = "UnitPrice")]
    pub unit_price: f64,
    #[serde(alias = "ReservationName")]
    pub reservation_name: String,
    #[serde(alias = "ResourceId")]
    pub resource_id: String,
    // Not present in new format — populated from resource_id after parse
    #[serde(default, alias = "ResourceName")]
    pub resource_name: String,
    // Old format: "ResourceGroup", new format: "resourceGroupName"
    #[serde(rename = "resourceGroupName", alias = "ResourceGroup")]
    pub resource_group: String,
    #[serde(alias = "ResourceLocation")]
    pub resource_location: String,
    // Not present in old format test data
    #[serde(default, rename = "invoiceSectionName")]
    pub invoice_section: String,
    #[serde(alias = "PublisherName")]
    pub publisher_name: String,
    // Not present in new format
    #[serde(default, alias = "PlanName")]
    pub plan_name: String,
    #[serde(alias = "ChargeType")]
    pub charge_type: String,
    #[serde(alias = "Frequency")]
    pub frequency: String,
    #[serde(alias = "PricingModel")]
    pub pricing_model: String,
    // Already camelCase in both old and new formats
    pub benefit_id: String,
    pub benefit_name: String,
    #[serde(alias = "Tags")]
    pub tags: Tags,
    #[serde(skip_deserializing)]
    pub line_number_csv: usize,
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

/// Convert a CSV date value to ISO 8601 (YYYY-MM-DD).
///
/// The Azure Detailed CSV stores dates as `MM/DD/YYYY`. We normalise to
/// `YYYY-MM-DD` on ingest so the rest of the code never sees the raw format.
/// Any value that doesn't match the expected pattern is kept as-is.
fn deserialize_date<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let s = String::deserialize(d)?;
    // Fast path: already ISO (YYYY-MM-DD)
    if s.len() == 10 && s.as_bytes().get(4) == Some(&b'-') {
        return Ok(s);
    }
    // Convert MM/DD/YYYY → YYYY-MM-DD
    if s.len() == 10 && s.as_bytes().get(2) == Some(&b'/') && s.as_bytes().get(5) == Some(&b'/') {
        let mm = &s[0..2];
        let dd = &s[3..5];
        let yyyy = &s[6..10];
        return Ok(format!("{yyyy}-{mm}-{dd}"));
    }
    Ok(s)
}

pub fn extract_date_from_file_name(file_path: &str) -> String {
    // Prefer the parent directory name if it starts with a date pattern (YYYY-MM or YYYYMM).
    // Folder names like "2026-04_G156087700" are set by the user and reflect the real billing
    // month, whereas the CSV file name may contain a different period code (e.g. "202605").
    if let Some(parent) = std::path::Path::new(file_path).parent()
        && let Some(dir_name) = parent.file_name().and_then(|n| n.to_str()) {
            let re_folder = Regex::new(r"^(\d{4}-\d{2}|\d{6})").unwrap();
            if let Some(caps) = re_folder.captures(dir_name) {
                return caps[1].to_string();
            }
        }
    // Fall back: extract _YYYYMM_ from the file name itself
    let re = Regex::new(r"_(\d{6})_").unwrap();
    if let Some(caps) = re.captures(file_path) {
        caps[1].to_string()
    } else {
        file_path.to_string()
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
    use crate::cmd_parse::FilterOpts;
    use crate::money::Nzd;
    use std::path::PathBuf;

    static FILTER_OPTS: FilterOpts = FilterOpts { case_sensitive: true };

    #[test]
    fn test_cost_by_resource_name() {
        let file_name: PathBuf = PathBuf::from("tests/azure_test_data_01.csv");
        let mut bills = crate::bills::Bills::default();
        let result = bills.parse_csv(&file_name, &FILTER_OPTS);
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
        let file_name: PathBuf = PathBuf::from("tests/azure_test_data_01.csv");
        let mut bills = crate::bills::Bills::default();
        let result = bills.parse_csv(&file_name, &FILTER_OPTS);
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
        assert_eq!(first_bill.date, "2024-03-08", "date mismatch");
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
}
