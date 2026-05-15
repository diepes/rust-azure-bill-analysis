# Split GlobalOpts into FilterOpts and DisplayOpts

`GlobalOpts` originally bundled CLI path resolution, filter configuration, and display configuration in one flat struct threaded through every module. We split it:

- `FilterOpts { case_sensitive }` — passed to `BillFilter::new()`
- `DisplayOpts { cost_min_display, tag_list, debug }` — passed to display functions
- Path fields (`bill_path`, `bill_prev_subtract_path`) stay in the top-level `App` struct and never leave `main.rs`

This follows naturally from ADR-0004: once `BillFilter` absorbs the filter strings, `case_sensitive` is the only filter-relevant field left in `GlobalOpts`, making the split obvious. The benefit is that modules declare exactly what they depend on — `cost_by_any_summary` no longer imports display configuration, and tests no longer construct irrelevant path fields.
