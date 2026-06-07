use serde::Serialize;
use std::collections::HashMap;

use crate::bills::Bills;

// ---------------------------------------------------------------------------
// resource_type extraction
// ---------------------------------------------------------------------------

/// Extract the Azure resource type from an ARM resource ID.
///
/// ARM resource IDs follow the pattern:
/// `/subscriptions/{id}/resourceGroups/{rg}/providers/{Namespace}/{type}/{name}`
///
/// Returns `{namespace}/{type}` lowercased, e.g. `microsoft.compute/disks`.
/// Returns an empty string if the resource_id doesn't contain a `providers/` segment.
pub fn extract_resource_type(resource_id: &str) -> String {
    let lower = resource_id.to_lowercase();
    // Find "providers/" and take the two path segments after it.
    if let Some(pos) = lower.find("/providers/") {
        let after = &lower[pos + "/providers/".len()..];
        let parts: Vec<&str> = after.splitn(4, '/').collect();
        if parts.len() >= 2 {
            return format!("{}/{}", parts[0], parts[1]);
        }
    }
    String::new()
}

// ---------------------------------------------------------------------------
// CostQuery — existing aggregated-cost query
// ---------------------------------------------------------------------------

/// Filters for [`query_cost`]. All string fields are case-insensitive regex
/// patterns; an empty string means "match all".
#[derive(Default)]
pub struct CostQuery {
    pub rg_filter: String,
    pub name_filter: String,
    pub tag_filter: String,
    /// When `Some`, only entries whose `date` field equals this ISO date string
    /// (`YYYY-MM-DD`) are included.
    pub date_filter: Option<String>,
}

/// Aggregated cost result returned by [`query_cost`].
pub struct CostSummary {
    pub cost_usd: f64,
    pub row_count: usize,
    /// Top-10 contributors by cost, descending.
    pub top_contributors: Vec<Contributor>,
}

/// One entry in [`CostSummary::top_contributors`].
#[derive(Serialize)]
pub struct Contributor {
    pub name: String,
    pub cost_usd: f64,
    pub row_count: usize,
}

/// Compile a non-empty pattern into a case-insensitive regex, or return `None`
/// for an empty string (meaning "match all").
pub(crate) fn compile_filter(pattern: &str) -> Result<Option<regex::Regex>, String> {
    if pattern.is_empty() {
        return Ok(None);
    }
    regex::Regex::new(&format!("(?i){pattern}"))
        .map(Some)
        .map_err(|e| format!("Invalid filter regex '{pattern}': {e}"))
}

/// Round to 2 decimal places for JSON output.
pub fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// Compute total USD cost across all matching bill entries.
///
/// All three string filters are **case-insensitive regexes** — plain strings
/// like `"prod"` or `"ingenie"` work as substring matches.
///
/// When `name_filter` is set, `top_contributors` are keyed by `resource_name`;
/// otherwise by `resource_group`. At most 10 contributors are returned, sorted
/// by cost descending.
pub fn query_cost(bills: &Bills, query: &CostQuery) -> Result<CostSummary, String> {
    use std::time::Instant;

    let t = Instant::now();
    let rg_re = compile_filter(&query.rg_filter)?;
    let name_re = compile_filter(&query.name_filter)?;
    let tag_re = compile_filter(&query.tag_filter)?;
    let group_by_name = name_re.is_some();

    let mut total_usd = 0.0f64;
    let mut row_count = 0usize;
    let mut by_key: HashMap<String, (f64, usize)> = HashMap::new();

    for entry in &bills.bills {
        if let Some(date) = &query.date_filter
            && &entry.date != date
        {
            continue;
        }
        if let Some(re) = &rg_re
            && !re.is_match(&entry.resource_group)
        {
            continue;
        }
        if let Some(re) = &name_re
            && !re.is_match(&entry.resource_name)
        {
            continue;
        }
        if let Some(re) = &tag_re
            && !re.is_match(&entry.tags.value)
        {
            continue;
        }

        let cost = entry.cost_usd.0;
        total_usd += cost;
        row_count += 1;

        let key = if group_by_name {
            entry.resource_name.clone()
        } else {
            entry.resource_group.clone()
        };
        let e = by_key.entry(key).or_insert((0.0, 0));
        e.0 += cost;
        e.1 += 1;
    }

    let mut entries: Vec<(String, f64, usize)> =
        by_key.into_iter().map(|(k, (c, n))| (k, c, n)).collect();
    entries.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    entries.truncate(10);

    log::debug!(
        "[query_cost] {} rows in {:.1}ms",
        row_count,
        t.elapsed().as_secs_f64() * 1000.0
    );

    Ok(CostSummary {
        cost_usd: total_usd,
        row_count,
        top_contributors: entries
            .into_iter()
            .map(|(name, cost_usd, row_count)| Contributor {
                name,
                cost_usd: round2(cost_usd),
                row_count,
            })
            .collect(),
    })
}

// ---------------------------------------------------------------------------
// ResourceSearchQuery — per-resource detailed search
// ---------------------------------------------------------------------------

/// Filters for [`search_resources`]. All string fields are case-insensitive
/// regex patterns; an empty string means "match all".
#[derive(Default)]
pub struct ResourceSearchQuery {
    pub rg_filter: String,
    pub name_filter: String,
    pub tag_filter: String,
    pub meter_category_filter: String,
    pub subscription_filter: String,
    /// Matched against the resource type extracted from `resource_id`,
    /// e.g. `"publicipaddresses"` or `"microsoft.compute/disks"`.
    pub resource_type_filter: String,
    /// Maximum number of results to return (default 50).
    pub limit: Option<usize>,
}

/// One resource row in [`ResourceSearchResult`].
#[derive(Serialize)]
pub struct ResourceRow {
    pub resource_name: String,
    pub resource_group: String,
    pub subscription_name: String,
    pub meter_category: String,
    pub resource_type: String,
    pub total_cost_usd: f64,
    pub charge_rows: usize,
}

/// Result returned by [`search_resources`].
pub struct ResourceSearchResult {
    /// Total unique resources matched (before limit).
    pub total_resources: usize,
    /// Summed cost of all matched resources (before limit).
    pub total_cost_usd: f64,
    /// Up to `limit` resources, sorted by `total_cost_usd` descending.
    pub resources: Vec<ResourceRow>,
}

/// Search for individual resources, returning one aggregated row per unique
/// `(resource_name, resource_group)` combination. Supports filtering by
/// `meter_category`, `subscription`, and `resource_type` in addition to the
/// filters available on [`query_cost`].
pub fn search_resources(
    bills: &Bills,
    query: &ResourceSearchQuery,
) -> Result<ResourceSearchResult, String> {
    use std::time::Instant;

    let t = Instant::now();
    let rg_re = compile_filter(&query.rg_filter)?;
    let name_re = compile_filter(&query.name_filter)?;
    let tag_re = compile_filter(&query.tag_filter)?;
    let cat_re = compile_filter(&query.meter_category_filter)?;
    let sub_re = compile_filter(&query.subscription_filter)?;
    let type_re = compile_filter(&query.resource_type_filter)?;
    let limit = query.limit.unwrap_or(50);

    // Key: (resource_name, resource_group) → (subscription_name, meter_category, resource_type, cost, rows)
    #[derive(Default)]
    struct Acc {
        subscription_name: String,
        meter_category: String,
        resource_type: String,
        cost: f64,
        rows: usize,
    }
    let mut by_resource: HashMap<(String, String), Acc> = HashMap::new();

    for entry in &bills.bills {
        if let Some(re) = &rg_re
            && !re.is_match(&entry.resource_group)
        {
            continue;
        }
        if let Some(re) = &name_re
            && !re.is_match(&entry.resource_name)
        {
            continue;
        }
        if let Some(re) = &tag_re
            && !re.is_match(&entry.tags.value)
        {
            continue;
        }
        if let Some(re) = &cat_re
            && !re.is_match(&entry.meter_category)
        {
            continue;
        }
        if let Some(re) = &sub_re
            && !re.is_match(&entry.subscription_name)
        {
            continue;
        }

        let rtype = extract_resource_type(&entry.resource_id);
        if let Some(re) = &type_re
            && !re.is_match(&rtype)
        {
            continue;
        }

        let key = (entry.resource_name.clone(), entry.resource_group.clone());
        let acc = by_resource.entry(key).or_default();
        acc.cost += entry.cost_usd.0;
        acc.rows += 1;
        if acc.subscription_name.is_empty() {
            acc.subscription_name = entry.subscription_name.clone();
        }
        if acc.meter_category.is_empty() {
            acc.meter_category = entry.meter_category.clone();
        }
        if acc.resource_type.is_empty() {
            acc.resource_type = rtype;
        }
    }

    let total_resources = by_resource.len();
    let total_cost_usd: f64 = by_resource.values().map(|a| a.cost).sum();

    let mut rows: Vec<ResourceRow> = by_resource
        .into_iter()
        .map(|((resource_name, resource_group), acc)| ResourceRow {
            resource_name,
            resource_group,
            subscription_name: acc.subscription_name,
            meter_category: acc.meter_category,
            resource_type: acc.resource_type,
            total_cost_usd: round2(acc.cost),
            charge_rows: acc.rows,
        })
        .collect();

    rows.sort_by(|a, b| {
        b.total_cost_usd
            .partial_cmp(&a.total_cost_usd)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    rows.truncate(limit);

    log::debug!(
        "[search_resources] {} resources ({} unique) in {:.1}ms",
        total_resources,
        rows.len(),
        t.elapsed().as_secs_f64() * 1000.0
    );

    Ok(ResourceSearchResult {
        total_resources,
        total_cost_usd: round2(total_cost_usd),
        resources: rows,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bills::bill_entry::BillEntry;
    use crate::money::Usd;

    fn make_entry(resource_group: &str, resource_name: &str, cost: f64, date: &str) -> BillEntry {
        BillEntry {
            resource_group: resource_group.to_string(),
            resource_name: resource_name.to_string(),
            cost_usd: Usd(cost),
            date: date.to_string(),
            ..BillEntry::default()
        }
    }

    fn make_bills(entries: Vec<BillEntry>) -> Bills {
        let mut b = Bills::default();
        b.bills = entries;
        b
    }

    #[test]
    fn unfiltered_sums_all_rows() {
        let bills = make_bills(vec![
            make_entry("rg-a", "vm-1", 10.0, "2026-04-01"),
            make_entry("rg-b", "vm-2", 20.0, "2026-04-01"),
            make_entry("rg-a", "vm-3", 5.0, "2026-04-02"),
        ]);
        let r = query_cost(&bills, &CostQuery::default()).unwrap();
        assert_eq!(r.row_count, 3);
        assert!((r.cost_usd - 35.0).abs() < 0.001);
    }

    #[test]
    fn rg_filter_excludes_non_matching() {
        let bills = make_bills(vec![
            make_entry("my-prod-rg", "vm-1", 10.0, "2026-04-01"),
            make_entry("dev-rg", "vm-2", 99.0, "2026-04-01"),
            make_entry("prod-east", "vm-3", 5.0, "2026-04-01"),
        ]);
        let r = query_cost(
            &bills,
            &CostQuery {
                rg_filter: "prod".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(r.row_count, 2);
        assert!((r.cost_usd - 15.0).abs() < 0.001);
    }

    #[test]
    fn name_filter_groups_by_resource_name() {
        let bills = make_bills(vec![
            make_entry("rg-a", "sql-prod-1", 10.0, "2026-04-01"),
            make_entry("rg-b", "sql-prod-2", 20.0, "2026-04-01"),
            make_entry("rg-a", "vm-other", 5.0, "2026-04-01"),
        ]);
        let r = query_cost(
            &bills,
            &CostQuery {
                name_filter: "sql".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(r.row_count, 2);
        assert!((r.cost_usd - 30.0).abs() < 0.001);
        assert!(r.top_contributors.iter().any(|c| c.name == "sql-prod-1"));
        assert!(r.top_contributors.iter().any(|c| c.name == "sql-prod-2"));
    }

    #[test]
    fn date_filter_excludes_other_dates() {
        let bills = make_bills(vec![
            make_entry("rg-a", "vm-1", 10.0, "2026-04-01"),
            make_entry("rg-a", "vm-2", 20.0, "2026-04-02"),
            make_entry("rg-a", "vm-3", 5.0, "2026-04-01"),
        ]);
        let r = query_cost(
            &bills,
            &CostQuery {
                date_filter: Some("2026-04-01".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(r.row_count, 2);
        assert!((r.cost_usd - 15.0).abs() < 0.001);
    }

    #[test]
    fn combined_rg_and_date_filter() {
        let bills = make_bills(vec![
            make_entry("prod-rg", "vm-1", 10.0, "2026-04-01"),
            make_entry("dev-rg", "vm-2", 20.0, "2026-04-01"),
            make_entry("prod-rg", "vm-3", 5.0, "2026-04-02"),
        ]);
        let r = query_cost(
            &bills,
            &CostQuery {
                rg_filter: "prod".into(),
                date_filter: Some("2026-04-01".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(r.row_count, 1);
        assert!((r.cost_usd - 10.0).abs() < 0.001);
    }

    #[test]
    fn top_contributors_truncated_to_10() {
        let entries: Vec<BillEntry> = (0..15)
            .map(|i| make_entry(&format!("rg-{i:02}"), "vm", i as f64, "2026-04-01"))
            .collect();
        let bills = make_bills(entries);
        let r = query_cost(&bills, &CostQuery::default()).unwrap();
        assert_eq!(r.top_contributors.len(), 10);
        assert_eq!(r.top_contributors[0].name, "rg-14");
    }

    #[test]
    fn no_match_returns_zero() {
        let bills = make_bills(vec![make_entry("rg-a", "vm-1", 10.0, "2026-04-01")]);
        let r = query_cost(
            &bills,
            &CostQuery {
                rg_filter: "nonexistent".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(r.row_count, 0);
        assert_eq!(r.cost_usd, 0.0);
        assert!(r.top_contributors.is_empty());
    }

    #[test]
    fn round2_basic() {
        assert_eq!(round2(1.234), 1.23);
        assert_eq!(round2(1.235), 1.24);
        assert_eq!(round2(0.0), 0.0);
        assert_eq!(round2(100.0), 100.0);
    }

    // -----------------------------------------------------------------------
    // extract_resource_type
    // -----------------------------------------------------------------------

    #[test]
    fn extract_resource_type_disk() {
        let id =
            "/subscriptions/abc/resourceGroups/rg-prod/providers/Microsoft.Compute/disks/my-disk";
        assert_eq!(extract_resource_type(id), "microsoft.compute/disks");
    }

    #[test]
    fn extract_resource_type_public_ip() {
        let id = "/subscriptions/abc/resourceGroups/rg/providers/Microsoft.Network/publicIPAddresses/my-ip";
        assert_eq!(
            extract_resource_type(id),
            "microsoft.network/publicipaddresses"
        );
    }

    #[test]
    fn extract_resource_type_empty_for_no_providers() {
        assert_eq!(extract_resource_type(""), "");
        assert_eq!(extract_resource_type("/subscriptions/abc"), "");
    }

    // -----------------------------------------------------------------------
    // search_resources
    // -----------------------------------------------------------------------

    fn make_rich_entry(
        rg: &str,
        name: &str,
        cost: f64,
        meter_category: &str,
        resource_id: &str,
        subscription_name: &str,
    ) -> BillEntry {
        BillEntry {
            resource_group: rg.to_string(),
            resource_name: name.to_string(),
            cost_usd: Usd(cost),
            meter_category: meter_category.to_string(),
            resource_id: resource_id.to_string(),
            subscription_name: subscription_name.to_string(),
            date: "2026-04-01".to_string(),
            ..BillEntry::default()
        }
    }

    #[test]
    fn search_resources_returns_unique_resources() {
        let ip_id =
            "/subscriptions/s/resourceGroups/rg/providers/Microsoft.Network/publicIPAddresses/ip1";
        let disk_id = "/subscriptions/s/resourceGroups/rg/providers/Microsoft.Compute/disks/d1";
        let bills = make_bills(vec![
            make_rich_entry("rg-net", "ip-1", 5.0, "Virtual Network", ip_id, "sub-a"),
            make_rich_entry("rg-net", "ip-1", 3.0, "Virtual Network", ip_id, "sub-a"),
            make_rich_entry("rg-compute", "disk-1", 20.0, "Storage", disk_id, "sub-a"),
        ]);
        let r = search_resources(&bills, &ResourceSearchQuery::default()).unwrap();
        assert_eq!(r.total_resources, 2);
        assert!((r.total_cost_usd - 28.0).abs() < 0.01);
        // ip-1 has two charge rows aggregated
        let ip = r
            .resources
            .iter()
            .find(|x| x.resource_name == "ip-1")
            .unwrap();
        assert_eq!(ip.charge_rows, 2);
        assert!((ip.total_cost_usd - 8.0).abs() < 0.01);
    }

    #[test]
    fn search_resources_meter_category_filter() {
        let disk_id = "/subscriptions/s/resourceGroups/rg/providers/Microsoft.Compute/disks/d1";
        let bills = make_bills(vec![
            make_rich_entry("rg-net", "ip-1", 5.0, "Virtual Network", "", "sub-a"),
            make_rich_entry("rg-compute", "disk-1", 20.0, "Storage", disk_id, "sub-a"),
        ]);
        let r = search_resources(
            &bills,
            &ResourceSearchQuery {
                meter_category_filter: "storage".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(r.total_resources, 1);
        assert_eq!(r.resources[0].resource_name, "disk-1");
        assert_eq!(r.resources[0].resource_type, "microsoft.compute/disks");
    }

    #[test]
    fn search_resources_resource_type_filter() {
        let ip_id =
            "/subscriptions/s/resourceGroups/rg/providers/Microsoft.Network/publicIPAddresses/ip1";
        let vm_id =
            "/subscriptions/s/resourceGroups/rg/providers/Microsoft.Compute/virtualMachines/vm1";
        let bills = make_bills(vec![
            make_rich_entry("rg-net", "my-ip", 5.0, "Virtual Network", ip_id, "sub-a"),
            make_rich_entry(
                "rg-compute",
                "my-vm",
                50.0,
                "Virtual Machines",
                vm_id,
                "sub-a",
            ),
        ]);
        let r = search_resources(
            &bills,
            &ResourceSearchQuery {
                resource_type_filter: "publicipaddresses".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(r.total_resources, 1);
        assert_eq!(r.resources[0].resource_name, "my-ip");
    }

    #[test]
    fn search_resources_sorted_by_cost_descending() {
        let bills = make_bills(vec![
            make_rich_entry("rg", "cheap", 1.0, "cat", "", "sub"),
            make_rich_entry("rg", "expensive", 100.0, "cat", "", "sub"),
            make_rich_entry("rg", "mid", 10.0, "cat", "", "sub"),
        ]);
        let r = search_resources(&bills, &ResourceSearchQuery::default()).unwrap();
        assert_eq!(r.resources[0].resource_name, "expensive");
        assert_eq!(r.resources[1].resource_name, "mid");
        assert_eq!(r.resources[2].resource_name, "cheap");
    }

    #[test]
    fn search_resources_limit_truncates_results() {
        let entries: Vec<BillEntry> = (0..10)
            .map(|i| make_rich_entry("rg", &format!("res-{i}"), i as f64, "cat", "", "sub"))
            .collect();
        let bills = make_bills(entries);
        let r = search_resources(
            &bills,
            &ResourceSearchQuery {
                limit: Some(3),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(r.total_resources, 10);
        assert_eq!(r.resources.len(), 3);
    }
}
