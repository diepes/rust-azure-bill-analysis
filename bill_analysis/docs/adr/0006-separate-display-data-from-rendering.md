# Separate Display Data Preparation from Rendering

`print_summary` and `display_cost_by_filter` in `display.rs` expressed all logic as `println!` side effects — colour selection, sorting, legend text, and subtotals were only observable via stdout. We separate:

- `prepare_rows(summary, cost_type, opts) -> Vec<PreparedRow>` — pure function returning sorted rows with pre-assigned colour labels and both NZD and USD amounts
- `legend_text(is_comparison: bool) -> String` — pure function for the colour legend
- `print_summary` — thin adapter that calls the above and iterates to `println!`

The colour logic bug (`CostSource` vs `cost < 0.0` precedence) was found by reading the code, not by a failing test — because there was no test surface for colour selection. The extracted pure functions are the intended test surface for colour, sorting, and legend correctness going forward.

## Consequences

`PreparedRow` becomes a new type in `display.rs` (or `bills_sum_data.rs`). It is not part of the public library interface — it is internal to the display module.
