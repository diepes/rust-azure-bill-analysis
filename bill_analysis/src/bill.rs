use serde::Deserialize;
use std::error::Error;
use csv::Reader;
use std::fs::File;
use std::path::Path;

//struct to hold bill data for Azure detailed Enrollment csv parsed file
#[derive(Debug, Deserialize)]
pub struct Bill {
    subscription_id: String,
    subscription_name: String,
    date: String,
    product: String,
    meter_id: String,
    meter_name: String,
    quantity: f64,
    cost: f64,
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
