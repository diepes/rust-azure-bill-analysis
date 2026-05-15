# BillFilter Struct Replaces 10-Parameter Function

`cost_by_any_summary` originally took 10 positional parameters (9 filter strings + `GlobalOpts`). We replace the 9 filter strings with a single `BillFilter` struct that holds the compiled regexes and encodes the shared conventions (empty string = match all, `"any"` location alias).

## Considered Options

**Keep 10 parameters.** Simple but every new filter dimension changes every call site and every test. The parameter list is already longer than can be verified at a glance.

**Use a builder.** More discoverable but adds boilerplate for a struct that is always fully constructed from CLI args at one call site.

**BillFilter struct (chosen).** Named fields are self-documenting, all call sites construct it the same way, and `BillFilter::new()` is the single place that encodes the empty-string and alias conventions.

## Consequences

The `case_sensitive` flag moves into `BillFilter::new()` since it affects regex construction. `GlobalOpts` is no longer passed into `cost_by_any_summary` directly.
