/// This module contains summary data structures derived from bills.
/// see bill/calc/summary.rs for actual implementation.
///
use crate::bill::costtype::CostType;
pub struct CostTotal {
    pub cost: f64,
    pub source: CostSource,
    pub cost_unreserved: f64,
}
pub struct ReservationInfo<'a> {
    pub cost_full: f64,    // unreserved cost
    pub cost_savings: f64, // unreseved cost - actual cost
    pub hr_total: f64,
    pub hr_saving: f64,
    pub cost_unused: f64, // cost of unused reservation
    // set of strings
    pub reservation_names: std::collections::HashSet<&'a str>,
    pub vm_names_reserved: Vec<&'a str>,
    pub vm_names_not_reserved: Vec<&'a str>,
    pub meter_category: String,
}
// set copy trait for CostTotal
#[derive(Clone, Copy)]
pub enum CostSource {
    Original,
    Secondary,
    Combined,
}
pub struct SummaryData<'a> {
    // Created by "pub fn cost_by_any_summary() in bills.rs"
    // Subtract old in pub fn display_cost_by_filter() in display.rs
    // Totals per CostType & Name e.g. (RG, Test01)
    pub per_type: std::collections::HashMap<(CostType, String), CostTotal>,
    // bill_details record cost per filter category e.g. name_regex, rg_regex, subs_regex, meter_category
    pub details: std::collections::HashSet<String>,
    pub filtered_cost_total: f64,
    pub reservations: std::collections::HashMap<(String, u8), ReservationInfo<'a>>, // flex type, day of month
}
impl<'a> Default for SummaryData<'a> {
    fn default() -> SummaryData<'a> {
        SummaryData {
            per_type: std::collections::HashMap::new(),
            details: std::collections::HashSet::new(),
            filtered_cost_total: 0.0,
            reservations: std::collections::HashMap::new(),
        }
    }
}
