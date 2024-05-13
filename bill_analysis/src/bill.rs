use serde::Deserialize;
use std::error::Error;
use csv::Reader;
use std::fs::File;
use std::path::Path;

//struct to hold bill data for Azure detailed Enrollment csv parsed file
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Bill {
    // SubscriptionId
    subscription_id: String,
    subscription_name: String,
    date: String,
    product: String,
    meter_id: String,
    meter_category: String, // e.g. "Virtual Network"
    meter_sub_category: String, // e.g. "Peering"
    meter_name: String, // e.g. "Intra-Region Ingress"
    quantity: f64,
    effective_price: f64,
    cost: f64,
    // UnitPrice,TotalUsedSavings,TotalUnused
    unit_price: f64,
}

impl Bill {
    // Function to parse the CSV file and return a vector of Bill structs
    pub fn parse_csv(file_path: &str) -> Result<Vec<Bill>, Box<dyn Error>> {
        let file = File::open(Path::new(file_path))?;
        let mut reader = Reader::from_reader(file);

        let mut bills = Vec::new();

        for result in reader.deserialize() {
            let bill: Bill = result?;
            bills.push(bill);
        }

        Ok(bills)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_csv() {
        let file_name = "tests/azure_test_data_01.csv";
        // Test file path
        let file_path = &file_name;

        // Parse the CSV file
        let result = Bill::parse_csv(file_path);

        // Assert that parsing was successful
        assert!(result.is_ok(),"!Error parsing the file:'{file_name}'\nERR:{}", result.err().unwrap());

        // Get the parsed bills
        let bills = result.unwrap();

        // Assert that the number of bills is correct
        assert_eq!(bills.len(), 8);

        // Assert the values of the first bill
        let first_bill = &bills[0];
        assert_eq!(first_bill.subscription_id, "fc123456-7890-1234-5678-901234567890","subscription_id mismatch");
        assert_eq!(first_bill.subscription_name, "TstNl", "subscription_name mismatch");
        assert_eq!(first_bill.date, "03/08/2024", "date mismatch");
        assert_eq!(first_bill.product, "TestVirtNet-Intra-Region", "product mismatch");
        assert_eq!(first_bill.meter_id, "59bc01e3-test-4b9f-bacf-35e696aad6d4", "meter_id mismatch");

        assert_eq!(first_bill.meter_name, "Intra-Region Ingress", "meter_name mismatch");
        assert_eq!(first_bill.quantity, (0.194368534), "quantity mismatch");
        assert_eq!(first_bill.cost, (0.003025655), "cost mismatch");

    }
}