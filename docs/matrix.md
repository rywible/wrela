# Matrix Command

`wrela matrix` runs the Phase 0/5 validation matrix and writes an evidence bundle.

## What It Runs

1. `cargo test --workspace`
2. `wrela test language/spec/spec.wr`
3. `wrela perf --runs=<N> --baseline-out=.artifacts/matrix/perf-baseline.json language/spec/spec.wr`

If `--perf-gate=PATH` is provided, step 3 also receives:

- `--perf-gate=PATH`
- `--perf-max-regression-pct=<N>` (default `5`)

## Usage

```bash
cargo run -p wrela -- matrix
cargo run -p wrela -- matrix --runs=1
cargo run -p wrela -- matrix --runs=1 --perf-gate=.artifacts/perf/baseline.json --perf-max-regression-pct=5
```

## Evidence Output

The command writes:

- `.artifacts/matrix/matrix-<timestamp>.json`
- `.artifacts/matrix/matrix-latest.json`
- per-step logs: `.artifacts/matrix/NN-<step>.stdout.log` and `.artifacts/matrix/NN-<step>.stderr.log`
- perf baseline: `.artifacts/matrix/perf-baseline.json`

Bundle fields include `success`, `exit_code`, `steps[]`, command args, duration, and log paths.

## CI Notes

Use the matrix workflow to enforce merge-gate checks and publish `.artifacts/matrix` as a build artifact for traceability.
