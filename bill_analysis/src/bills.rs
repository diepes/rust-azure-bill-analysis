// bill/mod.rs

pub mod bill_filter;
pub use bill_filter::BillFilter;

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

use crate::bills::bill_entry::BillEntry;
use crate::bills::bill_entry::extract_date_from_file_name;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fs::File;
use std::io::Read;
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
        filter_opts: &crate::cmd_parse::FilterOpts,
    ) -> Result<(), Box<dyn Error>> {
        let file = File::open(Path::new(file_path))?;
        self.file_name = file_path
            .clone()
            .into_os_string()
            .into_string()
            .expect("Could not convert path to string ?");
        self.file_short_name = extract_date_from_file_name(&self.file_name);
        let short_name = self.file_short_name.clone();
        self.parse_csv_from_reader(file, &short_name, filter_opts)
    }

    /// Parse CSV from any `Read` source (e.g. in-memory bytes from blob storage).
    /// `source_name` is used as `file_short_name` and for progress logging.
    pub fn parse_csv_from_reader<R: Read>(
        &mut self,
        reader: R,
        source_name: &str,
        filter_opts: &crate::cmd_parse::FilterOpts,
    ) -> Result<(), Box<dyn Error>> {
        let start = Instant::now();
        let mut reader = csv::Reader::from_reader(reader);
        self.file_name = source_name.to_string();
        self.file_short_name = source_name.to_string();
        let mut line_number: usize = 0;
        for (line, result) in reader.deserialize().enumerate() {
            line_number = line + 2; // 1-based line number + header
            if result.is_err() {
                println!(
                    "Error parsing line #{} in file {}",
                    line_number,
                    source_name
                );
            }
            let mut bill: BillEntry = result?;
            // New format omits ResourceName — derive it from the last segment of resourceId
            if bill.resource_name.is_empty() && !bill.resource_id.is_empty() {
                bill.resource_name = bill
                    .resource_id
                    .rsplit('/')
                    .next()
                    .unwrap_or("")
                    .to_string();
            }
            if !filter_opts.case_sensitive {
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
                total_cost: crate::money::Nzd::default(),
                total_cost_usd: crate::money::Usd::default(),
                exchange_rate: 0.0,
                total_no_reservation: crate::money::Usd::default(),
                total_effective: crate::money::Usd::default(),
                total_savings_used: crate::money::Usd::default(),
                total_savings_un_used: crate::money::Usd::default(),
                total_savings_meter_category_map: HashMap::new(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::cmd_parse::FilterOpts;
    use crate::money::Nzd;
    use std::path::PathBuf;

    static FILTER_OPTS: FilterOpts = FilterOpts {
        case_sensitive: true,
    };

    #[test]
    fn test_cost_by_resource_name() {
        let file_name: PathBuf = PathBuf::from("tests/azure_test_data_01.csv");
        let mut bills = super::Bills::default();
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
        let mut bills = super::Bills::default();
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
