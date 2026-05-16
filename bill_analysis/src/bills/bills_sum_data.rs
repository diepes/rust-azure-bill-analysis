/// This module contains summary data structures derived from bills.
/// see bill/calc/summary.rs for actual implementation.
///
use crate::bills::cost_type_enum::CostType;
use crate::money::{Nzd, Usd};

pub struct CostTotal {
    pub cost: Nzd,
    pub cost_usd: Usd,
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
#[derive(Debug, Clone, Copy)]
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
    pub filtered_cost_total: Nzd,
    pub filtered_cost_total_usd: Usd,
    pub reservations: std::collections::HashMap<(String, u8), ReservationInfo<'a>>, // flex type, day of month
}
impl<'a> SummaryData<'a> {
    /// Accumulate a single bill row's cost into `per_type` under the given key.
    /// On first insertion the source is `Original`; on subsequent rows the costs are summed.
    pub fn accumulate(
        &mut self,
        cost_type: CostType,
        key: String,
        cost: crate::money::Nzd,
        cost_usd: crate::money::Usd,
        cost_unreserved: f64,
    ) {
        self.per_type
            .entry((cost_type, key))
            .and_modify(|e| {
                e.cost += cost;
                e.cost_usd += cost_usd;
                e.cost_unreserved += cost_unreserved;
            })
            .or_insert(CostTotal {
                cost,
                cost_usd,
                cost_unreserved,
                source: CostSource::Original,
            });
    }

    /// Subtract `prev` from `self` in place, producing a diff view.
    ///
    /// Entries present in both → `Combined` with delta costs.
    /// Entries only in `prev`  → `Secondary` with negated costs (gone/removed).
    /// Entries only in `self`  → unchanged `Original` (new/added).
    /// Also subtracts the filtered cost totals and merges details sets.
    pub fn merge_summaries(&mut self, prev: &SummaryData) {
        self.filtered_cost_total -= prev.filtered_cost_total;
        self.filtered_cost_total_usd -= prev.filtered_cost_total_usd;
        for (prev_key, prev_cost) in &prev.per_type {
            self.per_type
                .entry(prev_key.clone())
                .and_modify(|e| {
                    e.cost -= prev_cost.cost;
                    e.cost_usd -= prev_cost.cost_usd;
                    e.cost_unreserved -= prev_cost.cost_unreserved;
                    e.source = CostSource::Combined;
                })
                .or_insert(CostTotal {
                    cost: -prev_cost.cost,
                    cost_usd: -prev_cost.cost_usd,
                    cost_unreserved: -prev_cost.cost_unreserved,
                    source: CostSource::Secondary,
                });
        }
        self.details.extend(prev.details.iter().cloned());
    }
}

impl<'a> Default for SummaryData<'a> {
    fn default() -> SummaryData<'a> {
        SummaryData {
            per_type: std::collections::HashMap::new(),
            details: std::collections::HashSet::new(),
            filtered_cost_total: Nzd::default(),
            filtered_cost_total_usd: Usd::default(),
            reservations: std::collections::HashMap::new(),
        }
    }
}
