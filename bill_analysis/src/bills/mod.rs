// bill/mod.rs

pub mod bill_entry;
pub mod bills_impl_basic;
pub mod bills_impl_cost_by_any;
pub mod bills_impl_cost_by_sub;
pub mod bills_impl_currency;
pub mod bills_struct;
pub mod bills_sum_data;
pub mod cost_type_enum;
pub mod display;
pub mod summary;
pub mod tags;

use crate::bills::bills_struct::Bills;

// use super::bills::bill_sum_data::{ReservationInfo, SummaryData};
//use crate::RESERVATION_SUMMARY; // lib.rs static RESERVATION_SUMMARY

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
