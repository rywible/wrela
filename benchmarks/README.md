# Wrela Benchmark Harness

This benchmark harness measures compiled Wrela runtime performance across five suites:

- `micro` (low-level primitives and hot loops)
- `meso` (scheduler/mailbox/pool behavior models)
- `macro` (domain-level pipelines)
- `field_engine` (semantic hard-scene field-engine pack for phase-11 repetition, thin-feature, local-frame, radiance/media, and opaque-pessimization regressions)
- `linux` (optional Linux-only pack, non-blocking in v1)

## Manifests

Each suite has a `bench.toml` manifest:

- `benchmarks/micro/bench.toml`
- `benchmarks/meso/bench.toml`
- `benchmarks/macro/bench.toml`
- `benchmarks/field_engine/bench.toml`
- `benchmarks/linux/bench.toml`

Contract rules:

- Scenario test names must end with `_ops_<N>` where `<N>` matches `ops`.
- Scenarios run with deterministic checksum assertions in the test body.

## Profiles

`wrela perf` and `wrela perfcmp` support:

- `--profile=smoke` (2 warmup, 6 measured)
- `--profile=standard` (3 warmup, 10 measured, default)
- `--profile=deep` (5 warmup, 18 measured)

Per-scenario overrides are available for `perfcmp`:

- `--warmup-pairs=<N>`
- `--measure-pairs=<N>`
- `--min-effect-pct=<pct>`
- `--confidence=<pct>`

## Run Commands

Single-suite perf baseline:

```bash
cargo run -p wrela -- perf benchmarks/micro --profile=standard --runs=5
```

Field-engine baseline on CPU:

```bash
cargo run -p wrela -- perf benchmarks/field_engine --profile=standard --runs=5 --query-backend=cpu
```

Field-engine baseline on WGSL:

```bash
cargo run -p wrela -- perf benchmarks/field_engine --profile=standard --runs=5 --query-backend=wgsl
```

Paired comparison (baseline vs candidate refs):

```bash
cargo run -p wrela -- perfcmp benchmarks/macro \
  --profile=standard \
  --baseline-ref=origin/main \
  --candidate-ref=HEAD
```

Linux optional pack:

```bash
cargo run -p wrela -- perfcmp benchmarks/linux --profile=standard
```

On non-Linux hosts, optional Linux suite is skipped and reported as non-blocking.

## Statistical Method

Per scenario, `perfcmp` uses:

- paired interleaved baseline/candidate runs
- warmup pair discard
- median paired delta
- bootstrap percentile CI (`10_000` resamples)
- default practical threshold `2.0%`

Classification:

- `win`: CI low > `+min_effect_pct`
- `regression`: CI high < `-min_effect_pct`
- `no_signal`: otherwise

Stability:

- CV limit: micro `<= 2.5%`, meso/macro `<= 5.0%`
- IQR/median runtime `<= 0.15`
- scenario `min_runtime_ms` can force unstable status if medians are too short

## Gating

- `smoke`: correctness only (no perf classification gating)
- `standard`: fail on critical regressions
- `deep`: fail on any stable regression, unstable critical scenarios, or unstable ratio > 20%
- optional suites are non-blocking

## Artifacts

Primary reports:

- JSON: `.artifacts/perf/perfcmp-report.json`
- Markdown: `.artifacts/perf/perfcmp-report.md`

Suite baselines:

- `.artifacts/perf/baselines/<suite>-<profile>-<ref>.json`

Diagnostics:

- metrics: `.artifacts/perf/metrics/*.json`
- report includes key metric deltas and host metadata
