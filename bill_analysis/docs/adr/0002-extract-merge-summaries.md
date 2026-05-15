# Extract merge_summaries as a Pure Function

The bill-diff logic (subtract a previous `SummaryData` from the latest one, tagging entries as `CostSource::Original`, `Secondary`, or `Combined`) was originally inlined inside `display_cost_by_filter` alongside `println!` calls. We extract it into a standalone pure function `merge_summaries(latest: SummaryData, previous: SummaryData) -> SummaryData` so it can be tested directly without running the display pipeline.

This is where the bug that zeroed USD deltas for `Combined` entries lived — it was only caught by manually replicating the logic in a test. The extracted function is the intended test surface for all future diff-correctness tests.
