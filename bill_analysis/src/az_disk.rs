use csv::ReaderBuilder;
use serde::Deserialize;
use std::error::Error;
use std::fs::File;
use std::io::BufRead;
use std::path::{Path, PathBuf}; // Import the BufRead trait

//struct to hold bill data for Azure detailed Enrollment csv parsed file
#[derive(Debug, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
#[allow(unused)]
pub struct AzDisk {
    // SubscriptionId
    pub name: String,
    // most other fields are optional as we might just load names from tst file
    #[serde(rename = "STORAGE TYPE")]
    pub storage_type: Option<String>,
    #[serde(rename = "SIZE (GIB)")]
    pub size_gb: Option<usize>,
    pub owner: Option<String>,
    #[serde(rename = "RESOURCE GROUP")]
    pub resource_group: Option<String>,
    pub location: Option<String>,
}

impl AzDisk {}

pub struct AzDisks {
    pub disks: Vec<AzDisk>,
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
    /**
     * Function to parse the file and return a vector of AzDisk structs
     **/
    pub fn parse(file_disk: &PathBuf) -> Result<AzDisks, Box<dyn Error>> {
        let disks = if file_disk.extension().unwrap() == "csv" {
            Self::parse_csv(&file_disk)
                .expect(&format!("Error parsing the csv file '{:?}'", file_disk))
        } else {
            Self::parse_txt(&file_disk)
                .expect(&format!("Error parsing the txt file '{:?}'", file_disk))
        };
        Ok(disks)
    }
    /**
    Function to parse the CSV file and return a vector of AzDisk structs
    */
    fn parse_csv(file_path: &PathBuf) -> Result<AzDisks, Box<dyn Error>> {
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
    fn parse_txt(file_path: &PathBuf) -> Result<AzDisks, Box<dyn Error>> {
        // Create a new Bills instance
        let mut az_disks = AzDisks::default();

        // Open the new line delimite txt file with only names
        let file = File::open(file_path)?;
        let rdr = std::io::BufReader::new(file);
        // create new disk record for each line
        for line in rdr.lines() {
            let record = AzDisk {
                name: line?,
                storage_type: None,
                size_gb: None,
                owner: None,
                resource_group: None,
                location: None,
            };
            // Push the record to the Bills instance
            az_disks.push(record);
        }
        Ok(az_disks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_disk_csv() {
        let file_name: PathBuf = PathBuf::from("tests/azure_test_disks_01.csv");
        // Test file path
        let file_path = &file_name;

        // Parse the CSV file
        let result = AzDisks::parse(file_path);

        // Assert that parsing was successful
        assert!(
            result.is_ok(),
            "!Error parsing the file:'{file_name:?}'\nERR:{}",
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
            first_disk.storage_type,
            Some("Standard SSD LRS".into()),
            "storage_type mismatch"
        );
        assert_eq!(first_disk.size_gb, Some(50), "size_gb mismatch");
        assert_eq!(first_disk.owner, Some("-".into()), "owner mismatch");
        assert_eq!(
            first_disk.resource_group,
            Some("nonprd-vnet-rg".into()),
            "resource_group mismatch"
        );

        assert_eq!(
            first_disk.location,
            Some("Australia East".into()),
            "location mismatch"
        );
    }
    #[test]
    fn test_parse_txt() {
        let file_name: PathBuf = PathBuf::from("tests/azure_test_disks_02.txt");
        // Test file path
        let file_path = &file_name;

        // Parse the CSV file
        let result = AzDisks::parse(file_path);

        // Assert that parsing was successful
        assert!(
            result.is_ok(),
            "!Error parsing the txt file:'{file_name:?}'\nERR:{}",
            result.err().unwrap()
        );

        // Get the parsed bills
        let disks = result.unwrap().disks;

        // Assert that the number of bills is correct
        assert_eq!(disks.len(), 5);

        // Assert the values of the first bill
        let first_disk = &disks[0];
        assert_eq!(first_disk.name, "y-prd-xint-datadsk-01-0", "name mismatch");
        assert_eq!(first_disk.storage_type, None, "storage_type mismatch");
        assert_eq!(first_disk.size_gb, None, "size_gb mismatch");
        assert_eq!(first_disk.owner, None, "owner mismatch");
        assert_eq!(first_disk.resource_group, None, "resource_group mismatch");

        assert_eq!(first_disk.location, None, "location mismatch");
    }
}
