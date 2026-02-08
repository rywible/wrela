# Runtime Rewrite Migration Guide And Final Cutover Checklist

Date: 2026-02-08

## Architecture Diff (Before vs After)

| Area | Before | After |
| --- | --- | --- |
| Runtime role | Rust mixed primitives + policy drift risk | Rust strict kernel waist (primitives only) |
| Policy behavior | Could leak into runtime helpers | Lives in Wrela stdlib/policy layer |
| ABI guardrails | Snapshot parity only | Snapshot parity + forbidden policy symbol classes |
| Validation | Ad hoc command runs | Baseline + perf harness + cutover checklist |

## Contributor Upgrade Checklist

- Read `core/spec/runtime_rewrite/kernel_waist_contract.md`.
- Run baseline commands once on your machine.
- Confirm your change stays in the correct track playbook.
- Update snapshot intentionally if ABI/intrinsics change.
- Attach AC evidence with command results before requesting merge.

## Rollback / Recovery If Parity Breaks

1. Stop merge train for runtime rewrite branch.
2. Revert the offending change set (single issue scope only).
3. Re-run:
   - `cargo test -p wrela --test codegen`
   - `cargo test -p wrela --test thin_core_snapshot`
   - `./target/debug/wrela test`
4. Compare perf harness outputs to baseline to ensure no secondary regression.
5. Re-open failed issue with root cause and follow-up plan.

## Final Merge Cutover Checklist (Binary Pass/Fail)

- [x] `cargo test -p wrela --tests` passes.
- [x] `cargo test -p wrela_runtime --lib` passes.
- [x] `./target/debug/wrela test` passes.
- [x] `cargo test -p wrela --test thin_core_snapshot` passes.
- [x] `cargo test -p wrela --test codegen native_pool_policy_matrix_smoke -- --exact` passes.
- [x] `./core/spec/runtime_rewrite/perf_harness.sh current` completes and budget decisions are recorded.
- [x] No unwaived p95 regression above 20% in `perf_regression_report_2026-02-08.md`.
- [ ] All issue AC evidence comments are posted and include command/results summary.

Cutover is approved only if every checkbox is true.

## Cutover Execution Evidence (2026-02-08)

| Command | Result |
| --- | --- |
| `cargo test -p wrela --tests` | PASS |
| `cargo test -p wrela_runtime --lib` | PASS |
| `./target/debug/wrela test` | PASS (40 passed, 0 failed) |
| `cargo test -p wrela --test thin_core_snapshot` | PASS |
| `cargo test -p wrela --test codegen native_pool_policy_matrix_smoke -- --exact` | PASS |
| `./core/spec/runtime_rewrite/perf_harness.sh current` | PASS |
