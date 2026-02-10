# Matrix Command

`wrela matrix` runs the Phase 0/5 validation matrix and writes an evidence bundle.

## What It Runs

1. `cargo test --workspace`
2. `wrela test language/spec/spec.wr`
3. `wrela perf --runs=<N> --baseline-out=.artifacts/matrix/perf-baseline.json language/spec/spec.wr`

If `--perf-gate=PATH` is provided, step 3 also receives:

- `--perf-gate=PATH`
- `--perf-max-regression-pct=<N>` (default `5`)
- optional KPI thresholds:
  - `--kpi-check-fallback-max=<N>`
  - `--kpi-check-batch-min=<N>`
  - `--kpi-scheduler-p99-improve-min-pct=<N>`
  - `--kpi-rewrite-overhead-max-pct=<N>`
  - `--kpi-actor-throughput-improve-min-pct=<N>`
  - `--kpi-queue-age-p99-max-regress-pct=<N>`
  - `--kpi-starvation-violations-max=<N>`
  - `--kpi-scheduler-throughput-improve-min-pct=<N>`
  - `--kpi-scheduler-loop-p99-max-regress-pct=<N>`
  - `--kpi-scheduler-local-hit-min=<N>`

## Usage

```bash
cargo run -p wrela -- matrix
cargo run -p wrela -- matrix --runs=1
cargo run -p wrela -- matrix --runs=1 --perf-gate=.artifacts/perf/baseline.json --perf-max-regression-pct=5
cargo run -p wrela -- matrix --runs=1 --perf-gate=.artifacts/perf/baseline.json --perf-max-regression-pct=5 --kpi-check-fallback-max=0.20 --kpi-check-batch-min=6 --kpi-scheduler-p99-improve-min-pct=10 --kpi-rewrite-overhead-max-pct=5
cargo run -p wrela -- matrix --runs=1 --perf-gate=.artifacts/perf/baseline.json --perf-max-regression-pct=5 --kpi-actor-throughput-improve-min-pct=0 --kpi-queue-age-p99-max-regress-pct=10 --kpi-starvation-violations-max=0
cargo run -p wrela -- matrix --runs=1 --perf-gate=.artifacts/perf/baseline.json --perf-max-regression-pct=5 --kpi-scheduler-throughput-improve-min-pct=0 --kpi-scheduler-loop-p99-max-regress-pct=20 --kpi-scheduler-local-hit-min=0.25
```

## Evidence Output

The command writes:

- `.artifacts/matrix/matrix-<timestamp>.json`
- `.artifacts/matrix/matrix-latest.json`
- per-step logs: `.artifacts/matrix/NN-<step>.stdout.log` and `.artifacts/matrix/NN-<step>.stderr.log`
- perf baseline: `.artifacts/matrix/perf-baseline.json`

Bundle fields include `success`, `exit_code`, `steps[]`, command args, duration, and log paths.
When available, the bundle also includes a KPI table under `perf_summary` and active KPI thresholds under `kpi_thresholds`.

## CI Notes

Use the matrix workflow to enforce merge-gate checks and publish `.artifacts/matrix` as a build artifact for traceability.
