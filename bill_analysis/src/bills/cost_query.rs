use serde::Serialize;

use crate::bills::Bills;

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
    use std::collections::HashMap;
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
        if let Some(date) = &query.date_filter {
            if &entry.date != date {
                continue;
            }
        }
        if let Some(re) = &rg_re {
            if !re.is_match(&entry.resource_group) {
                continue;
            }
        }
        if let Some(re) = &name_re {
            if !re.is_match(&entry.resource_name) {
                continue;
            }
        }
        if let Some(re) = &tag_re {
            if !re.is_match(&entry.tags.value) {
                continue;
            }
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
            &CostQuery { rg_filter: "prod".into(), ..Default::default() },
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
            &CostQuery { name_filter: "sql".into(), ..Default::default() },
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
            &CostQuery { date_filter: Some("2026-04-01".into()), ..Default::default() },
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
            &CostQuery { rg_filter: "nonexistent".into(), ..Default::default() },
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
}
