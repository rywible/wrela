# V1 Baseline Manifest

This tracked manifest records baseline command expectations for V2 readiness.

## Commands

1. `cargo run -q -p wrela -- --help`
2. `cargo run -q -p wrela -- test apps/ledger-lite`
3. `cargo run -q -p wrela -- check language/spec/spec.wr`
4. `cargo run -q -p wrela -- perf benchmarks/micro --profile=smoke --runs=1`
5. `cargo run -q -p wrela -- perf benchmarks/meso --profile=smoke --runs=1`

## Artifact Location

Raw captures are written to `.artifacts/baselines/v1/` via
`scripts/governance/capture_v1_baseline.sh`.

Because `.artifacts/` is ignored, this manifest is the tracked baseline contract.
