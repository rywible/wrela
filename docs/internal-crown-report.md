# Internal Crown Report

Date: 2026-02-09
Project: Wrela 4-Track Straight-Shot (Internal Crown)

## Evidence Sources

- Matrix bundle: `/Users/ryanwible/projects/wrela/.artifacts/matrix/matrix-latest.json`
- Final comparison: `/Users/ryanwible/projects/wrela/.artifacts/perf/final-comparison.json`
- Merge gate policy: `/Users/ryanwible/projects/wrela/docs/merge-gate.md`

## Final Gate Outcome

- Matrix result: `success=true`, `exit_code=0`
- Steps green:
1. `cargo test --workspace`
2. `wrela test language/spec/spec.wr`
3. `wrela perf --runs=1 --baseline-out=.artifacts/matrix/perf-baseline.json language/spec/spec.wr`

## Performance Summary

From `final-comparison.json`:

1. `abi_call_heavy`: Wrela vs Rust `+40.53%`, vs C `+56.08%` (pass)
2. `map_hot_lookup`: Wrela vs Rust `+62.85%`, vs C `+81.93%` (pass)
3. `field_lookup`: Wrela vs Rust `+93.62%`, vs C `+87.73%` (pass)
4. `actor_queue_throughput`: Wrela vs Rust `+342.07%`, vs C `-21.28%` (not passing C comparator)
5. `scheduler_dispatch`: Wrela vs Rust `+1041.52%`, vs C `+337.61%` (pass)

Gate rollup:

- workloads passing: `4`
- regression violations vs baseline: `0`
- `passes_project_gate=true`

## Merge Checklist

- [x] Compiler + runtime workspace tests green
- [x] Spec tests green
- [x] Perf harness emits reproducible baseline in matrix artifacts
- [x] Rewrite mining/admission/rulepack pipeline integrated (`compiler/mir/rewrite.rs`)
- [x] CheckIR extraction + scalar/batch evaluator integrated (`compiler/hir/checkir.rs`)
- [x] EffectIR annihilation/reconstruction integrated (`compiler/mir/effect_ir.rs`)
- [x] Matrix CI workflow present (`.github/workflows/matrix.yml`)
- [x] Evidence bundle and logs persisted under `.artifacts/matrix`

## Known Limitations

- Matrix currently enforces one-shot lane success; policy for mandatory 3 consecutive green runs is documented, but branch protection enforcement is handled in VCS settings.
- Clippy has pre-existing warning debt across runtime/compiler modules; not required for gate pass in this project lane.
