# DB Policy Intent DSL

`WRE-498` introduces a policy intent DSL contract plus a deterministic runtime compiler.

## Intent Shape

Intent fields (all required):

- `survivability`
- `latency_target_ms`
- `residency_scope`
- `cost_tier`
- `budget_class`

Reference DSL surface lives at:

- `language/packages/db/policy/dsl.wr`

Runtime compile types live at:

- `runtime/src/db/autopilot/compiler.rs`

## Deterministic Compile Contract

`compile_policy_intent(spec)` performs:

1. Normalization:
   - trims `policy_id`
   - trims + lowercases each residency region
   - stable sort of residency scope
2. Validation:
   - empty/invalid/duplicate residency region typed errors
   - required non-zero latency target
3. Contradiction detection (typed):
   - `SurvivabilityExceedsResidency`
   - `LatencyTooAggressiveForSurvivability`
   - `LatencyTooAggressiveForCostTier`
   - `BudgetConflictsWithCostTier`

Successful compile emits:

- deterministic `policy_hash` (`fnv64`)
- `explain` metadata with stable canonical material and schema version

Canonical material format:

`v=1|policy_id=<...>|survivability=<...>|latency_target_ms=<...>|residency_scope=<...>|cost_tier=<...>|budget_class=<...>`

## Diagnostics

Contradictions return:

- typed `PolicyContradictionCode`
- clear `reason` string suitable for user diagnostics/logging

Example rejection:

- `survivability intent TwoRegionFailure requires at least 3 residency regions, got 2`

## Tests

Coverage lives in:

- `runtime/tests/db_policy_compiler.rs`

Scenarios:

- deterministic compile output
- contradictory policy rejection with typed code + reason
- explain metadata/hash stability across equivalent inputs

Run:

`cargo test -p wrela_runtime --test db_policy_compiler -- --nocapture`
