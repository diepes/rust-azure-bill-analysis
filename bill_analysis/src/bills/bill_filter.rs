use crate::cmd_parse::FilterOpts;
use regex::{Regex, RegexBuilder};

/// `BillFilter` groups all per-query filter parameters that narrow down which
/// bill rows are included in a `cost_by_any_summary` result.
///
/// Empty string means "match all" for every field except `location`,
/// which defaults to `"any"` (all regions).
///
/// Regex patterns are compiled once at construction time. `new()` returns
/// `Err` immediately for any invalid regex, so the caller catches bad input
/// before processing any bill rows.
#[derive(Debug)]
pub struct BillFilter {
    // Pattern strings — kept for `{filter:?}` debug output and display.
    pub name: String,
    pub resource_group: String,
    pub subscription: String,
    pub meter_category: String,
    /// Location/region filter. Special values: `"any"` or `"all"` = match all;
    /// `"none"` = match rows with no location set. Defaults to `"any"`.
    pub location: String,
    pub reservation: String,
    /// Tag key used for grouping output rows (not a regex — plain key name).
    pub tag_summarise: String,
    pub tag_filter: String,
    pub invoice_section: String,
    // Pre-compiled regexes for all pattern fields (not tag_summarise).
    pub(crate) re_name: Regex,
    pub(crate) re_resource_group: Regex,
    pub(crate) re_subscription: Regex,
    pub(crate) re_meter_category: Regex,
    pub(crate) re_location: Regex,
    pub(crate) re_reservation: Regex,
    pub(crate) re_tag_filter: Regex,
    pub(crate) re_invoice_section: Regex,
    /// Whether tag key lookups use exact case (`true`) or lowercase (`false`).
    pub(crate) case_sensitive: bool,
}

impl BillFilter {
    /// Construct from the `Option<String>` values that come directly off the
    /// CLI args. `None` maps to `""` for all fields except `location`, which
    /// maps to `"any"` (the "match all regions" sentinel).
    ///
    /// `filter_opts.case_sensitive` controls whether regexes are compiled
    /// case-sensitively; the value is also stored on the struct for later use
    /// (e.g. tag key lookup).
    ///
    /// Returns `Err` if any pattern is not a valid regex expression.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: Option<String>,
        resource_group: Option<String>,
        subscription: Option<String>,
        meter_category: Option<String>,
        location: Option<String>,
        reservation: Option<String>,
        tag_summarise: Option<String>,
        tag_filter: Option<String>,
        invoice_section: Option<String>,
        filter_opts: &FilterOpts,
    ) -> Result<Self, regex::Error> {
        let name = name.unwrap_or_default();
        let resource_group = resource_group.unwrap_or_default();
        let subscription = subscription.unwrap_or_default();
        let meter_category = meter_category.unwrap_or_default();
        let location = location.unwrap_or_else(|| "any".to_string());
        let reservation = reservation.unwrap_or_default();
        let tag_summarise = tag_summarise.unwrap_or_default();
        let tag_filter = tag_filter.unwrap_or_default();
        let invoice_section = invoice_section.unwrap_or_default();

        let ci = !filter_opts.case_sensitive;

        let build_re_with_case =
            |pattern: &str| RegexBuilder::new(pattern).case_insensitive(ci).build();

        Ok(BillFilter {
            re_name: build_re_with_case(&name)?,
            re_resource_group: build_re_with_case(&resource_group)?,
            re_subscription: build_re_with_case(&subscription)?,
            re_meter_category: build_re_with_case(&meter_category)?,
            re_location: build_re_with_case(&location)?,
            re_reservation: build_re_with_case(&reservation)?,
            re_tag_filter: build_re_with_case(&tag_filter)?,
            re_invoice_section: build_re_with_case(&invoice_section)?,
            case_sensitive: filter_opts.case_sensitive,
            name,
            resource_group,
            subscription,
            meter_category,
            location,
            reservation,
            tag_summarise,
            tag_filter,
            invoice_section,
        })
    }
}
