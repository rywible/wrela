# Wrela Benchmark Harness

The benchmark harness now focuses on the world-language surface that remains in the repo:

- `micro`: low-level primitives and hot loops.
- `field_engine`: authored field/scene query cases for repetition, thin features, local frames, mixed-solver dense-oracle closure, radiance/media, and opaque-pessimization regressions.
- `realtime_presentation`: presentation-oriented scene-shape benchmarks for dense constructive geometry, repetition-heavy layouts, thin-stack aliasing, and media/radiance scenes.

## Manifests

Each suite has a `bench.toml` manifest:

- `benchmarks/micro/bench.toml`
- `benchmarks/field_engine/bench.toml`
- `benchmarks/realtime_presentation/bench.toml`

Scenario test names must end with `_ops_<N>` where `<N>` matches `ops`, and scenarios should use deterministic checksum assertions in the test body.

## Run Commands

```bash
cargo run -p wrela -- perf benchmarks/micro --profile=standard --runs=5
cargo run -p wrela -- perf benchmarks/field_engine --profile=standard --runs=5 --query-backend=cpu
cargo run -p wrela -- perf benchmarks/field_engine --profile=standard --runs=5 --query-backend=wgsl
cargo run -p wrela -- perf benchmarks/realtime_presentation --profile=standard --runs=5 --query-backend=cpu
```

Paired comparison:

```bash
cargo run -p wrela -- perfcmp benchmarks/field_engine \
  --profile=standard \
  --baseline-ref=origin/main \
  --candidate-ref=HEAD
cargo run -p wrela -- perfcmp benchmarks/realtime_presentation \
  --profile=standard \
  --baseline-ref=origin/main \
  --candidate-ref=HEAD
```

## Profiles

- `--profile=smoke`: correctness-oriented short run.
- `--profile=standard`: default baseline profile.
- `--profile=deep`: longer regression-hunting profile.

Per-scenario overrides are available for `perfcmp` with `--warmup-pairs`, `--measure-pairs`, `--min-effect-pct`, and `--confidence`.

## Realtime Presentation Scenarios

The `realtime_presentation` suite keeps the checks deterministic by combining fixed query grids with checksum-style assertions while also attaching a canonical multi-frame named-view probe per scenario in `bench.toml`. `wrela perf benchmarks/realtime_presentation ...` now derives its scenario runtime lane from presentation frame-cost reports rather than the raw query-fixture wall clock alone, records those presentation probes in the baseline JSON under `presentation_reports`, and prints `presentation-scenario` / `presentation-pass` lines with the quality tier, internal resolution scale history, bottleneck pass, acceleration artifacts, and per-pass work/cost breakdown.

The scene queries in this suite are pinned to `dispatch_backend_cpu()` so the benchmark lane measures one stable execution backend rather than whatever `auto` resolves to on a given machine. The presentation probe is likewise collected with the CPU backend today. Use the CPU perf lane for these complex representative scenes until the remaining WGSL shader-validation gaps are closed; CPU/WGSL parity stays covered by `compiler/tests/presentation_exec.rs`.

- `presentation_dense_constructive_geometry`: dense constructive solids built from lofts, sweeps, bends, and unions. This stresses candidate selection, hit resolution, and normal stability on heavily composed geometry.
- `presentation_repetition_heavy_scene`: repetition-heavy structure built from nested linear repetition and instancing. This measures repeat identity, instance stability, and traversal behavior in tiled layouts.
- `presentation_thin_stack_alias_prone`: thin stacked layers and near-touching surfaces. This exercises alias-prone rays, epsilon sensitivity, and shallow-angle normal consistency.
- `presentation_media_radiance_scene`: radiance- and media-enabled presentation content. This covers surface sampling, radiance lookup, medium evaluation, and the frame path for volumetric scenes.

## Artifacts

- JSON report: `.artifacts/perf/perfcmp-report.json`
- Markdown report: `.artifacts/perf/perfcmp-report.md`
- Suite baselines: `.artifacts/perf/baselines/<suite>-<profile>-<ref>.json`
- Metrics: `.artifacts/perf/metrics/*.json`
