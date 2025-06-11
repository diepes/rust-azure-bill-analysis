// bill/mod.rs

pub mod bill_entry;
pub mod bills_impl_basic;
pub mod bills_impl_cost_by_any;
pub mod bills_impl_cost_by_sub;
pub mod bills_impl_currency;
pub mod bills_sum_data;
pub mod cost_type_enum;
pub mod display;
pub mod summary;
pub mod tags;
// use crate::bills::bills_struct::Bills;

use crate::bills::bill_entry::extract_date_from_file_name;
use crate::bills::bill_entry::BillEntry;
use std::collections::{HashSet, HashMap};
use std::error::Error;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::Instant;

pub struct Bills {
    pub bills: Vec<BillEntry>,
    pub billing_currency: Option<String>,
    pub tag_names: HashSet<String>,
    pub file_name: String,
    pub file_short_name: String,
    pub summary: summary::Summary,
}



impl Bills {
    // Function to parse the CSV file and return a vector of BillEntry structs
    pub fn parse_csv(
        &mut self,
        file_path: &PathBuf,
        global_opts: &crate::GlobalOpts,
    ) -> Result<(), Box<dyn Error>>
    {
        let start = Instant::now();
        let file = File::open(Path::new(file_path))?;
        // 2024-06-23 tested mmap for faster read, no difference for 200k lines
        //let mmap = unsafe { memmap::MmapOptions::new().map(&file).unwrap() };
        //let mut reader = csv::Reader::from_reader(mmap.as_ref());
        let mut reader = csv::Reader::from_reader(file);
        // set file name
        self.file_name = file_path
            .clone()
            .into_os_string()
            .into_string()
            .expect("Could not convert path to string ?");
        self.file_short_name = extract_date_from_file_name(&self.file_name);
        let mut line_number: usize = 0;
        for (line, result) in reader.deserialize().enumerate() {
            line_number = line + 2; // 1-based line number + header
            if result.is_err() {
                println!(
                    "Error parsing line #{} in file {}",
                    line_number,
                    file_path.display()
                );
            }
            let mut bill: BillEntry = result?;
            if !global_opts.case_sensitive {
                bill.lowercase_all_strings();
            }
            // handle empty RG - probably purchase
            // PLAN:{pn}__ChargeTYPE:{ct}__CSV:{ln}__
            // pn=bill.plan_name.replace(' ', "-"),
            // ct=bill.charge_type,
            // ln=line_number,
            if bill.resource_group.is_empty() {
                bill.resource_group = format!(
                    "EMPTY_RG__PUBL:{pubn}__MCat:{mc}__MSubCat:{msc}",
                    pubn = bill.publisher_name.replace(' ', "-"),
                    mc = bill.meter_category.replace(' ', "_"),
                    msc = bill.meter_sub_category.replace(' ', "_"),
                );
            }
            bill.line_number_csv = line_number;
            // record global tags
            self.tag_names.extend(bill.tags.kv.keys().cloned());
            self.push(bill);
        }
        self.set_billing_currency()?;
        println!(
            "parse_csv {line_number} lines in {:.3}s",
            start.elapsed().as_secs_f64()
        );
        self.calc_all_totals(); // Ensure calc_all_totals has the correct lifetime constraints

        Ok(())
    }
}

impl Default for Bills {
    fn default() -> Self {
        Self {
            bills: Vec::new(),
            billing_currency: None,
            tag_names: HashSet::new(),
            file_name: "NotSet".to_string(),
            file_short_name: "NotSet".to_string(),
            summary: summary::Summary {
                total_cost: 0.0,
                total_no_reservation: 0.0,
                total_effective: 0.0,
                total_savings_used: 0.0,
                total_savings_un_used: 0.0,
                total_savings_meter_category_map: HashMap::new(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::bills::bill_entry::BillEntry;
    use crate::cmd_parse::GlobalOpts;
    use std::path::PathBuf;

    // use super::*;

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
