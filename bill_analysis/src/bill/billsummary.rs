/// This module contains summary data structures derived from bills.
use crate::bill::costtype::CostType;
pub struct CostTotal {
    pub cost: f64,
    pub source: CostSource,
}

// set copy trait for CostTotal
#[derive(Clone, Copy)]
pub enum CostSource {
    Original,
    Secondary,
    Combined,
}

pub struct SummaryData {
    // Created by "pub fn cost_by_any_summary() in bills.rs"
    // Subtract old in pub fn display_cost_by_filter() in display.rs
    // Totals per CostType & Name e.g. (RG, Test01)
    pub per_type: std::collections::HashMap<(CostType, String), CostTotal>,
    // bill_details record cost per filter category e.g. name_regex, rg_regex, subs_regex, meter_category
    pub details: std::collections::HashSet<String>,
    pub filtered_cost_total: f64,
}
impl SummaryData {
    pub fn new() -> SummaryData {
        SummaryData {
            per_type: std::collections::HashMap::new(),
            details: std::collections::HashSet::new(),
            filtered_cost_total: 0.0,
        }
    }
}
