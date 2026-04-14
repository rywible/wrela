# Wrela Benchmark Harness

The benchmark harness now focuses on the world-language surface that remains in the repo:

- `micro`: low-level primitives and hot loops.
- `field_engine`: authored field/scene query cases for repetition, thin features, local frames, mixed-solver dense-oracle closure, radiance/media, opaque-pessimization regressions, and a collision-heavy transition proxy lane for the closure protocol.
- `realtime_presentation`: presentation-oriented scene-shape benchmarks for dense constructive geometry, repetition-heavy layouts, a dedicated repeat-aware solver proof lane, thin-stack aliasing, relaxed exact-torus solver coverage, transformed primitive galleries, mixed opaque/conservative scenes, media/radiance scenes, cache-stress motion paths, and camera-motion temporal-reuse / clipmap-churn coverage. The explicit `1080p120` closure lane now adds fixed scenarios for each of those representative stresses.
- `1080p120` closure profiles: fixed 1920x1080, 120 FPS protocol manifests for the frame and collision lanes. `wrela perf --profile=1080p120` automatically selects the companion `1080p120_closure.toml` file when it exists.

## Manifests

Each suite has a `bench.toml` manifest for the default microbench lane, and the closure profiles use a companion `1080p120_closure.toml`:

- `benchmarks/micro/bench.toml`
- `benchmarks/field_engine/bench.toml`
- `benchmarks/realtime_presentation/bench.toml`
- `benchmarks/field_engine/1080p120_closure.toml`
- `benchmarks/realtime_presentation/1080p120_closure.toml`

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
- `presentation_repeat_linear_solver_scene`: direct axis-aligned `repeat_linear` evidence lane. This keeps dense-parity and repeat-identity checks in the benchmark fixture while the phase-37 CPU solver regression and presentation report counters prove repeat-aware traversal reduction and skipped repeated cells on the supported subset.
- `presentation_thin_stack_alias_prone`: thin stacked layers and near-touching surfaces. This exercises alias-prone rays, epsilon sensitivity, and shallow-angle normal consistency.
- `presentation_media_radiance_scene`: radiance- and media-enabled presentation content. This covers surface sampling, radiance lookup, medium evaluation, and the frame path for volumetric scenes.

The `1080p120_closure.toml` protocol files define the fixed closure lane. Their scenario ids are prefixed with `closure_1080p120_` so they stay visually distinct from the microbench scenes, and their view definitions use `realtime_quality(target_fps = 120)` with fixed 1920x1080 framing.

Closure lane coverage currently includes:

- `closure_1080p120_dense_constructive`: candidate selection and hit-resolution stress on composed geometry.
- `closure_1080p120_repetition_heavy`: repeat identity and instance-stability stress on tiled repetition layouts.
- `closure_1080p120_thin_stack_grazing`: alias-prone, shallow-angle ray coverage for thin-stack geometry.
- `closure_1080p120_media_radiance`: radiance and medium evaluation coverage for volumetric scenes.
- `closure_1080p120_transformed_primitive_gallery`: translated primitive-gallery coverage intended to keep transformed primitive hits and normals stable under the fixed closure protocol.
- `closure_1080p120_mixed_opaque_conservative`: support-bounded guard plus conservative-field mix intended to stress opaque/conservative interplay in the representative presentation lane.
- `closure_1080p120_cache_stress_motion_path`: repeat-heavy motion-path coverage intended to churn cache/acceleration reuse under fixed camera-relative movement.
- `closure_1080p120_camera_motion_temporal_reuse_clipmap_churn`: camera-motion coverage intended to exercise temporal reuse and clipmap-like view churn in the canonical presentation lane.

## Field Engine Closure Scenarios

The `field_engine` closure manifest is the collision-side companion to the presentation closure lane. It keeps the benchmark protocol fixed while stressing the semantic query substrate that collision depends on, including repeat identity, dense-oracle mixed solving, and transition-like probe motion.

- `closure_1080p120_repetition_identity_stability`: repeat identity and authored-instance stability for the collision baseline lane.
- `closure_1080p120_mixed_solver_dense_oracle`: mixed solver behavior anchored to the dense CPU oracle under the fixed closure protocol.
- `closure_1080p120_collision_heavy_transition`: transition-like probe motion across mixed support/repetition structure, used as the representative collision-heavy closure case for non-regression tracking.
- `closure_1080p120_transformed_primitive_gallery`: transformed/deformed primitive coverage for the closure lane’s gallery-style authored geometry family.
- `closure_1080p120_mixed_opaque_conservative`: mixed opaque/conservative coverage so closure keeps the authored opaque support path and the broader conservative scene in the same protocol.

## Artifacts

- JSON report: `.artifacts/perf/perfcmp-report.json`
- Markdown report: `.artifacts/perf/perfcmp-report.md`
- Suite baselines: `.artifacts/perf/baselines/<suite>-<profile>-<ref>.json`
- Metrics: `.artifacts/perf/metrics/*.json`
