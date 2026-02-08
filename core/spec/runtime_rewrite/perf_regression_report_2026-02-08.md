# Runtime Rewrite Perf Regression Budget Report

Date: 2026-02-08
Budget Rule: `current_p95 <= baseline_p95 * 1.20` unless waived.

## Harness

```sh
./core/spec/runtime_rewrite/perf_harness.sh baseline
./core/spec/runtime_rewrite/perf_harness.sh current
```

## Baseline vs Current

| Scenario | Baseline p95 (s) | Current p95 (s) | Delta | Budget | Decision |
| --- | --- | --- | --- | --- | --- |
| `pool_queue_mpsc_multi_producer` | 0.07 | 0.07 | 0.00% | <= 20% | PASS |
| `native_pool_backpressure_config_smoke` | 0.30 | 0.32 | 6.67% | <= 20% | PASS |

## Waiver Format

Use this exact template when accepting a regression above budget:

```md
### PERF WAIVER
- Scenario:
- Baseline p95:
- Current p95:
- Delta (%):
- Root cause:
- Why accepted:
- Owner:
- Expiration date:
- Follow-up issue:
```

## Functional Re-Validation After Perf Runs

| Command | Result |
| --- | --- |
| `cargo test -p wrela --test codegen native_pool_policy_matrix_smoke -- --exact` | PASS |
| `cargo test -p wrela --test thin_core_snapshot` | PASS |
| `./target/debug/wrela test` | PASS |
