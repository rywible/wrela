# Merge Gate

The merge gate for Phase 0/5 is the `wrela matrix` lane.

A change is gate-passing when all of these are true:

1. `cargo test --workspace` passes.
2. `wrela test language/spec/spec.wr` passes.
3. `wrela perf` produces a baseline bundle under `.artifacts/matrix/perf-baseline.json`.
4. If `--perf-gate` is set, perf regression checks pass under `--perf-max-regression-pct`.
5. If KPI thresholds are set, KPI gate checks pass:
   - `--kpi-check-fallback-max`
   - `--kpi-check-batch-min`
   - `--kpi-scheduler-p99-improve-min-pct`
   - `--kpi-rewrite-overhead-max-pct`
   - `--kpi-actor-throughput-improve-min-pct`
   - `--kpi-queue-age-p99-max-regress-pct`
   - `--kpi-starvation-violations-max`
   - `--kpi-scheduler-throughput-improve-min-pct`
   - `--kpi-scheduler-loop-p99-max-regress-pct`
   - `--kpi-scheduler-local-hit-min`

## Required Evidence

CI must upload `.artifacts/matrix/**` so reviewers can inspect:

- step-by-step command logs,
- matrix JSON bundle metadata,
- KPI table (`perf_summary`) and threshold snapshot (`kpi_thresholds`),
- generated perf baseline used in the run.

## Local Repro

```bash
cargo run -p wrela -- matrix --runs=1
```

Optional perf gate check:

```bash
cargo run -p wrela -- matrix --runs=1 --perf-gate=.artifacts/perf/baseline.json --perf-max-regression-pct=5
cargo run -p wrela -- matrix --runs=1 --perf-gate=.artifacts/perf/baseline.json --perf-max-regression-pct=5 --kpi-check-fallback-max=0.20 --kpi-check-batch-min=6 --kpi-scheduler-p99-improve-min-pct=10 --kpi-rewrite-overhead-max-pct=5
cargo run -p wrela -- matrix --runs=1 --perf-gate=.artifacts/perf/baseline.json --perf-max-regression-pct=5 --kpi-actor-throughput-improve-min-pct=0 --kpi-queue-age-p99-max-regress-pct=10 --kpi-starvation-violations-max=0
cargo run -p wrela -- matrix --runs=1 --perf-gate=.artifacts/perf/baseline.json --perf-max-regression-pct=5 --kpi-scheduler-throughput-improve-min-pct=0 --kpi-scheduler-loop-p99-max-regress-pct=20 --kpi-scheduler-local-hit-min=0.25
```
