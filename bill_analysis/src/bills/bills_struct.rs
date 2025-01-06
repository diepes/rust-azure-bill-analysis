use crate::bills::bill_entry::BillEntry;
// use crate::bills::bills_impl_basic::Bills;
use std::collections::HashSet;
pub struct Bills {
    pub bills: Vec<BillEntry>,
    pub billing_currency: Option<String>,
    pub tag_names: HashSet<String>,
    pub file_name: String,
    pub file_short_name: String,
}
impl Default for Bills {
    fn default() -> Self {
        Self {
            bills: Vec::new(),
            billing_currency: None,
            tag_names: HashSet::new(),
            file_name: "NotSet".to_string(),
            file_short_name: "NotSet".to_string(),
        }
    }
}
