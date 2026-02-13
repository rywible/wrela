# Planner Cost Seed (WRE-590 / WRE-600)

This seed introduces a deterministic planner cost model and extends it with
versioned stats snapshots, refresh policy triggers, persistence roundtrip
coverage, and planner-lane baseline drift gating in
`runtime/src/db/planner/mod.rs`.

## Contract

- `StatsSnapshot` captures persisted planner stats:
  - `version: u32`
  - `histogram_buckets: Vec<HistogramBucket>`
  - `cardinality_estimate: u64`
- `PlannerStats` is the explain input model:
  - `snapshot: StatsSnapshot`
  - `selectivity: u32` (basis points, `0..=10_000`)
  - `index_available: bool`
  - `stats_stale: bool`
- `PlanKind` supports two candidate plans:
  - `IndexLookup`
  - `FullScan`
- `ExplainOutput` is stable and explicit:
  - `chosen_plan: PlanKind`
  - `costs: PlanCosts` containing both candidate costs
  - `stats_version: u32`
  - `stats_stale: bool`
  - `explain_schema_version: u16` (contract version marker)
  - `decision_basis: DecisionBasis` (currently `CostModelV1`)

## Planner Lane Baseline Registry

- `PlanBaselineRegistry` stores deterministic baseline entries keyed by query
  fingerprint (`u64`):
  - `PlanBaseline.kind: PlanKind`
  - `PlanBaseline.latency_ns: u64`
- Registry operations are explicit and deterministic:
  - `upsert(query_fingerprint, baseline)`
  - `get(query_fingerprint) -> Option<PlanBaseline>`

## Drift Gate Evaluation

- `evaluate_drift_gate` compares an observed planner outcome against registry
  baseline state for a query fingerprint.
- Inputs:
  - `PlanBaselineRegistry`
  - `query_fingerprint: u64`
  - `DriftObservation { kind, latency_ns }`
  - `DriftGatePolicy { max_latency_drift_bps }` (`100 = 1%`)
- Output is typed and deterministic:
  - `Ok(())` when plan kind matches and latency drift stays within policy.
  - `Err(DriftGateFailure::MissingBaseline { query_fingerprint })` when no
    baseline exists.
  - `Err(DriftGateFailure::PlanKindChanged { baseline, observed })` when plan
    kind diverges.
  - `Err(DriftGateFailure::LatencyDriftExceeded { baseline_latency_ns,
    observed_latency_ns, allowed_max_latency_ns })` when observed latency
    exceeds allowed drift.
- Allowed max latency is computed with integer arithmetic and ceiling semantics:
  - `baseline + ceil(baseline * max_latency_drift_bps / 10_000)`

## Deterministic Cost Rules

- Full scan cost: `cardinality_estimate * FULL_SCAN_ROW_COST`.
- Index lookup cost:
  - `u64::MAX` when `index_available == false`.
  - Otherwise: `INDEX_LOOKUP_SEEK_COST + estimated_rows * INDEX_LOOKUP_ROW_COST`.
- `estimated_rows` uses integer arithmetic with round-up:
  - `ceil(cardinality_estimate * selectivity / 10_000)`
  - minimum `1` when `cardinality_estimate > 0`
  - `0` when `cardinality_estimate == 0`
- `selectivity` is bounded to `10_000` before estimation.

## Refresh Policy

- `RefreshPolicy` declares a deterministic staleness trigger:
  - `staleness_threshold: u64` (`updates_since_refresh >= threshold` triggers refresh)
- Refresh can also be forced on-demand (`on_demand_refresh == true`), which takes
  precedence over threshold checks.
- `refresh_snapshot` is deterministic:
  - trigger decision via `should_refresh`
  - snapshot version is monotonic (at least previous version + 1)
  - `updates_since_refresh` resets to `0` after a refresh
- `is_snapshot_stale` provides a stable marker used to surface staleness in explain output.

## Persistence Codec

- `encode_snapshot` and `decode_snapshot` provide byte roundtrip for `StatsSnapshot`.
- Wire layout:
  - magic (`WRST`)
  - codec version (`u16`, currently `1`)
  - snapshot version (`u32`)
  - cardinality estimate (`u64`)
  - histogram bucket count (`u32`)
  - repeated buckets: `upper_bound(u64)`, `row_count(u64)`
- Decode performs deterministic validation:
  - exact magic/version match
  - non-zero snapshot version
  - bounded bucket count
  - strictly increasing histogram upper bounds
  - no trailing bytes

## Selection and Tie-Break

- Lower cost wins.
- Exact tie is resolved deterministically in favor of `IndexLookup`.

## Seed Test Coverage

- Index chosen when selective and available.
- Full scan chosen when index is unavailable.
- Refresh trigger behavior at threshold and on-demand override.
- Snapshot persistence roundtrip and decode validation failures.
- Explain output remains stable while including schema-version and
  decision-basis fields.
- Drift gate failures are deterministic and typed:
  - missing baseline
  - plan kind changed
  - latency drift exceeded

## Verification

```bash
cargo test -p wrela_runtime planner::tests -- --nocapture
```
