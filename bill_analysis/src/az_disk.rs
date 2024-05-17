use csv::{Reader, ReaderBuilder, StringRecord};
use serde::Deserialize;
use std::error::Error;
use std::fs::File;
use std::io::{self, BufRead};
use std::path::Path;

//struct to hold bill data for Azure detailed Enrollment csv parsed file
#[derive(Debug, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
#[allow(unused)]
pub struct AzDisk {
    // SubscriptionId
    name: String,
    #[serde(rename = "STORAGE TYPE")]
    storage_type: String,
    #[serde(rename = "SIZE (GIB)")]
    size_gb: usize,
    owner: String,
    #[serde(rename = "RESOURCE GROUP")]
    resource_group: String,
    location: String,
}

impl AzDisk {}

pub struct AzDisks {
    disks: Vec<AzDisk>,
}
impl AzDisks {
    fn default() -> Self {
        Self { disks: Vec::new() }
    }
    pub fn len(&self) -> usize {
        self.disks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.disks.is_empty()
    }

    fn push(&mut self, disk: AzDisk) {
        self.disks.push(disk);
    }

    /*
    Function to parse the CSV file and return a vector of Bill structs
    */
    pub fn parse_csv(file_path: &str) -> Result<AzDisks, Box<dyn Error>> {
        // Create a new Bills instance
        let mut az_disks = AzDisks::default();

        // Open the CSV file
        let file = File::open(file_path)?;
        let mut rdr = ReaderBuilder::new().from_reader(file);

        // Iterate over each record
        for result in rdr.deserialize() {
            // Read the record
            let record: AzDisk = result?;

            // Push the record to the Bills instance
            az_disks.push(record);
        }

        // Return the Bills instance
        Ok(az_disks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_disk_csv() {
        let file_name = "tests/azure_test_disks_01.csv";
        // Test file path
        let file_path = &file_name;

        // Parse the CSV file
        let result = AzDisks::parse_csv(file_path);

        // Assert that parsing was successful
        assert!(
            result.is_ok(),
            "!Error parsing the file:'{file_name}'\nERR:{}",
            result.err().unwrap()
        );

        // Get the parsed bills
        let disks = result.unwrap().disks;

        // Assert that the number of bills is correct
        assert_eq!(disks.len(), 10);

        // Assert the values of the first bill
        let first_disk = &disks[0];
        assert_eq!(
            first_disk.name, "aaabbbcccdisk-Template-Template-554433dd-7722-4411-9966-eedd6622ee5e",
            "name mismatch"
        );
        assert_eq!(
            first_disk.storage_type, "Standard SSD LRS",
            "storage_type mismatch"
        );
        assert_eq!(first_disk.size_gb, 50, "size_gb mismatch");
        assert_eq!(first_disk.owner, "-", "owner mismatch");
        assert_eq!(
            first_disk.resource_group, "nonprd-vnet-rg",
            "resource_group mismatch"
        );

        assert_eq!(first_disk.location, "Australia East", "location mismatch");
    }
}
