# Single accumulate Helper for CostType Aggregation

`cost_by_any_summary` accumulated costs into the `per_type` HashMap using a copy-pasted `entry().and_modify().or_insert(CostTotal{…})` block for each of the 9 `CostType` variants. We replace all 9 copies with a single `accumulate(map, key, bill)` helper.

The duplication was the root cause of the `cost_usd` omission bug: the field was added to `CostTotal` but the `and_modify` block for `Combined` entries was only partially updated. A single helper means any field added to `CostTotal` is handled once. Adding a new `CostType` dimension in future requires one call to `accumulate`, not a new copy of the block.
