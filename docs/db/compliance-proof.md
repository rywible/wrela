# Compliance Proof Contract

`runtime/src/db/autopilot/compliance.rs` emits deterministic compliance artifacts.

## Cost Guardrail Decision

Input:

- `estimated_monthly_cost_cents`
- `max_monthly_budget_cents`
- `hard_stop_ratio_bps`

Output:

- `action`: `Allow | ReduceFanout | FreezeChanges`
- `budget_utilization_bps`
- `reason`

Fail-closed behavior:

- zero budget produces `FreezeChanges`.
- utilization above hard stop produces `FreezeChanges`.

## Residency Compliance Proof

Input:

- `policy_id`
- `placements`: `(shard, target_region)` rows
- residency policy rules

Output:

- `all_allowed`
- rows with:
  - `allowed`
  - `token` (`RESIDENCY_EGRESS_DENY` or `RESIDENCY_EGRESS_POLICY_UNSAT`)
  - `reason`

Fail-closed behavior:

- any deny or unsat row sets `all_allowed=false`.

## Verification

```bash
cargo test -p wrela_runtime compliance::tests -- --nocapture
cargo test -p wrela_runtime read_slo_controller::tests -- --nocapture
```
