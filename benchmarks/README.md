# Wrela Benchmark Harness

The benchmark harness now focuses on the world-language surface that remains in the repo:

- `micro`: low-level primitives and hot loops.
- `field_engine`: authored field/scene query cases for repetition, thin features, local frames, radiance/media, and opaque-pessimization regressions.

## Manifests

Each suite has a `bench.toml` manifest:

- `benchmarks/micro/bench.toml`
- `benchmarks/field_engine/bench.toml`

Scenario test names must end with `_ops_<N>` where `<N>` matches `ops`, and scenarios should use deterministic checksum assertions in the test body.

## Run Commands

```bash
cargo run -p wrela -- perf benchmarks/micro --profile=standard --runs=5
cargo run -p wrela -- perf benchmarks/field_engine --profile=standard --runs=5 --query-backend=cpu
cargo run -p wrela -- perf benchmarks/field_engine --profile=standard --runs=5 --query-backend=wgsl
```

Paired comparison:

```bash
cargo run -p wrela -- perfcmp benchmarks/field_engine \
  --profile=standard \
  --baseline-ref=origin/main \
  --candidate-ref=HEAD
```

## Profiles

- `--profile=smoke`: correctness-oriented short run.
- `--profile=standard`: default baseline profile.
- `--profile=deep`: longer regression-hunting profile.

Per-scenario overrides are available for `perfcmp` with `--warmup-pairs`, `--measure-pairs`, `--min-effect-pct`, and `--confidence`.

## Artifacts

- JSON report: `.artifacts/perf/perfcmp-report.json`
- Markdown report: `.artifacts/perf/perfcmp-report.md`
- Suite baselines: `.artifacts/perf/baselines/<suite>-<profile>-<ref>.json`
- Metrics: `.artifacts/perf/metrics/*.json`
