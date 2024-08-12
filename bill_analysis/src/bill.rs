use regex::Regex;
use serde::Deserialize;
use serde::Deserializer; // used for custom tags deserialization
use std::collections::HashMap;
use std::error::Error;
use std::fs::File;
use std::hash::Hash;
use std::path::{Path, PathBuf};

// 1brc speedup
use std::time::Instant;

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
    resource_group: String,
    // PlanName,ChargeType,Frequency
    plan_name: String,
    charge_type: String,
    frequency: String,
    // benefitId,benefitName
    #[serde(rename = "benefitId")]
    benefit_id: String,
    #[serde(rename = "benefitName")]
    benefit_name: String,
    tags: Tags,
}

impl BillEntry {
    // Function to parse the CSV file and return a vector of BillEntry structs
    pub fn parse_csv(file_path: &PathBuf) -> Result<Bills, Box<dyn Error>> {
        let start = Instant::now();
        let file = File::open(Path::new(file_path))?;
        // 2024-06-23 tested mmap for faster read, no difference for 200k lines
        //let mmap = unsafe { memmap::MmapOptions::new().map(&file).unwrap() };
        //let mut reader = csv::Reader::from_reader(mmap.as_ref());
        let mut reader = csv::Reader::from_reader(file);
        let mut bills = Bills::default();
        let mut lines = 0;
        for result in reader.deserialize() {
            let bill: BillEntry = result?;
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
    pub fn remove(&mut self, other: Bills) {
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
            if bill.resource_name == resource_name {
                acc + bill.cost
            } else {
                acc
            }
        })
    }
    // cost_by_resource_name_regex
    // returns the total cost of all bills in the resource_name and a set of all resource names matched.
    pub fn cost_by_resource_name_regex(
        &self,
        resource_regex: &str,
    ) -> (f64, std::collections::HashSet<String>) {
        let re_res = Regex::new(resource_regex).unwrap();
        // collect set of resource groups in set rgs
        let mut res_details = std::collections::HashSet::new();
        let bill = self.bills.iter().fold(0.0, |acc, bill| {
            if re_res.is_match(&bill.resource_name) {
                res_details.insert(format!(
                    "{}___{}",
                    bill.resource_group.clone(),
                    bill.resource_name.clone(),
                ));
                acc + bill.cost
            } else {
                acc
            }
        });
        (bill, res_details)
    }

    // function cost_by_any
    // takes name_regex, rg_regex, subs_regex, meter_category and returns total where all match,
    //   if empty str in input it is skiped for filter.
    // returns total_filtered_cost,
    //         set of filtered resource groups,
    //     and HashMap of filtered cost per category(each category total - total filtered cost)
    pub fn cost_by_any(
        &self,
        name_regex: &str,
        rg_regex: &str,
        subs_regex: &str,
        meter_category: &str,
    ) -> (
        f64,
        std::collections::HashSet<String>,
        std::collections::HashMap<(CostType, String), f64>,
    ) {
        let re_name = Regex::new(name_regex).unwrap();
        let re_rg = Regex::new(rg_regex).unwrap();
        let re_subs = Regex::new(subs_regex).unwrap();
        let re_type = Regex::new(meter_category).unwrap();
        // collect set of resource groups in set rgs
        let mut res_details = std::collections::HashSet::new();
        // filtered_bill_details record cost per filter category e.g. name_regex, rg_regex, subs_regex, meter_category
        let mut filtered_bill_details = std::collections::HashMap::new();

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
            }
            if flag_match {
                // if all match
                // record cost against resource_name, resource_group, subscription_name, meter_category
                filtered_bill_details
                    .entry((CostType::ResourceName, bill.resource_name.clone()))
                    .and_modify(|e| *e += bill.cost)
                    .or_insert(bill.cost);

                filtered_bill_details
                    .entry((CostType::ResourceGroup, bill.resource_group.clone()))
                    .and_modify(|e| *e += bill.cost)
                    .or_insert(bill.cost);

                filtered_bill_details
                    .entry((CostType::Subscription, bill.subscription_name.clone()))
                    .and_modify(|e| *e += bill.cost)
                    .or_insert(bill.cost);

                filtered_bill_details
                    .entry((CostType::MeterCategory, bill.meter_category.clone()))
                    .and_modify(|e| *e += bill.cost)
                    .or_insert(bill.cost);

                res_details.insert(format!(
                    "{rg}___{rn}",
                    rg = bill.resource_group.clone(),
                    rn = bill.resource_name.clone(),
                ));
                acc + bill.cost
            } else {
                acc
            }
        });
        // filtered_bill_details should have same cost total for each category
        (filtered_total, res_details, filtered_bill_details)
    }

    pub fn cost_by_resource_group(
        &self,
        resource_group: &str,
    ) -> (f64, std::collections::HashSet<String>) {
        let re_rg = Regex::new(resource_group).unwrap();
        // collect set of resource groups in set rgs
        let mut rgs = std::collections::HashSet::new();
        let bill = self.bills.iter().fold(0.0, |acc, bill| {
            if re_rg.is_match(&bill.resource_group) {
                rgs.insert(bill.resource_group.clone());
                acc + bill.cost
            } else {
                acc
            }
        });
        (bill, rgs)
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

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum CostType {
    ResourceName,
    ResourceGroup,
    Subscription,
    MeterCategory,
}
impl CostType {
    pub fn as_str(&self) -> &str {
        match self {
            CostType::ResourceName => "ResourceName",
            CostType::ResourceGroup => "ResourceGroup",
            CostType::Subscription => "Subscription",
            CostType::MeterCategory => "MeterCategory",
        }
    }
    // short name 3 char
    pub fn as_short(&self) -> &str {
        match self {
            CostType::ResourceName => "Res",
            CostType::ResourceGroup => "Rg",
            CostType::Subscription => "Sub",
            CostType::MeterCategory => "Meter",
        }
    }
}

// Tag data deserialized from the CSV file
#[derive(Debug,)]
pub struct Tags {
    kv: HashMap<String,String>
}

// Implement Deserialize for Tags, Vec<Tag>
impl<'de> Deserialize<'de> for Tags {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Deserialize the input into a string
        // e.g. '"JenkinsManagedTag": "ManagedByAzureVMAgents","JenkinsTemplateTag": "build-agent-azure"'
        let s = String::deserialize(deserializer)?;
        
        // Initialize a HashMap to hold the parsed tags
        let mut kv = HashMap::new();

        // Split the string by commas to separate each key-value pair
        for part in s.split(',') {
            // Split each pair by the colon to separate key and value
            let mut iter = part.split(':');
            if let (Some(key), Some(value)) = (iter.next(), iter.next()) {
                // Trim quotes and whitespace and insert into the HashMap
                kv.insert(key.trim_matches('"').trim().to_string(), value.trim_matches('"').trim().to_string());
            }
        }

        // Return the Tags struct with the populated HashMap
        Ok(Tags { kv })
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
