# Merge Gate

The merge gate for Phase 0/5 is the `wrela matrix` lane.

A change is gate-passing when all of these are true:

1. `cargo test --workspace` passes.
2. `wrela test language/spec/spec.wr` passes.
3. `wrela perf` produces a baseline bundle under `.artifacts/matrix/perf-baseline.json`.
4. If `--perf-gate` is set, perf regression checks pass under `--perf-max-regression-pct`.

## Required Evidence

CI must upload `.artifacts/matrix/**` so reviewers can inspect:

- step-by-step command logs,
- matrix JSON bundle metadata,
- generated perf baseline used in the run.

## Local Repro

```bash
cargo run -p wrela -- matrix --runs=1
```

Optional perf gate check:

```bash
cargo run -p wrela -- matrix --runs=1 --perf-gate=.artifacts/perf/baseline.json --perf-max-regression-pct=5
```
