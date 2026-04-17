# Wrela Benchmark Harness

The benchmark harness now focuses on the world-language surface that remains in the repo:

- `micro`: low-level primitives and hot loops.
- `field_engine`: authored field/scene query cases for repetition, thin features, local frames, mixed-solver dense-oracle closure, radiance/media, opaque-pessimization regressions, and a collision-heavy transition proxy lane for the closure protocol.
- `collision_perf`: collision-focused point occupancy, ray-cast, overlap, sweep, and TOI workload coverage with a dedicated CPU-oracle 1080p120 closure companion.
- `realtime_presentation`: presentation-oriented scene-shape benchmarks for dense constructive geometry, repetition-heavy layouts, a dedicated repeat-aware solver proof lane, thin-stack aliasing, relaxed exact-torus solver coverage, transformed primitive galleries, mixed opaque/conservative scenes, media/radiance scenes, cache-stress motion paths, and camera-motion temporal-reuse / clipmap-churn coverage. This suite remains the rendering subsystem lane for debugging and regression hunting.
- `whole_frame`: the canonical representative lane for fixed-1920x1080, 120 FPS closure work. Each scenario charges both resident presentation and representative collision work in the same suite and emits presentation, collision, and joined whole-frame reports.
- `1080p120` closure profiles: fixed 1920x1080, 120 FPS protocol manifests for the representative WGSL-resident whole-frame lane plus the companion CPU-oracle lane. `wrela perf --profile=1080p120` automatically selects the companion `1080p120_closure.toml` file when it exists and reports the closure stories explicitly.
  The default `1080p120` run is now truth-first: the canonical measurement lane is `whole_frame`, the baseline JSON includes `presentation_reports`, `collision_reports`, and `whole_frame_reports`, and expensive comparison payloads stay behind `--why-not-120`.

If you need the junior-friendly walkthrough for reading plans, report dumps, closure output,
and parity checks, start with [docs/perf/acceleration_playbook.md](../docs/perf/acceleration_playbook.md).
For the production phase-48 lanes, also use
[docs/perf/gpu_resident_framegraph_playbook.md](../docs/perf/gpu_resident_framegraph_playbook.md)
and
[docs/perf/collision_gpu_batch_playbook.md](../docs/perf/collision_gpu_batch_playbook.md).
If you are specifically working on the resident framegraph or collision batch lanes, use the
specialized playbooks alongside it:

- [GPU Resident Framegraph Playbook](../docs/perf/gpu_resident_framegraph_playbook.md)
- [Collision GPU Batch Playbook](../docs/perf/collision_gpu_batch_playbook.md)

## Repo Surface

From the repo root, use `just` for the named repo workflows:

```bash
just perf-smoke
just perf-closure
just baseline-devloop
```

Lane mapping:

- `just perf-smoke` => `wrela perf --profile=smoke`
- `just perf-closure` => `wrela perf --profile=1080p120`

The raw `cargo run -p wrela -- perf ...` commands below are the underlying `wrela` invocations
for subsystem debugging, ad-hoc comparisons, and flag variations.

## Manifests

Each suite has a `bench.toml` manifest for the default microbench lane, and the closure profiles use a companion `1080p120_closure.toml`:

- `benchmarks/micro/bench.toml`
- `benchmarks/field_engine/bench.toml`
- `benchmarks/collision_perf/bench.toml`
- `benchmarks/realtime_presentation/bench.toml`
- `benchmarks/whole_frame/bench.toml`
- `benchmarks/field_engine/1080p120_closure.toml`
- `benchmarks/collision_perf/1080p120_closure.toml`
- `benchmarks/realtime_presentation/1080p120_closure.toml`
- `benchmarks/whole_frame/1080p120_closure.toml`

Scenario test names must end with `_ops_<N>` where `<N>` matches `ops`, and scenarios should use deterministic checksum assertions in the test body.

## Underlying Wrela Perf Commands

```bash
cargo run -p wrela -- perf benchmarks/micro --profile=standard --runs=5
cargo run -p wrela -- perf benchmarks/field_engine --profile=standard --runs=5 --query-backend=cpu
cargo run -p wrela -- perf benchmarks/collision_perf --profile=standard --runs=5 --query-backend=cpu
cargo run -p wrela -- perf benchmarks/field_engine --profile=standard --runs=5 --query-backend=wgsl
cargo run -p wrela -- perf benchmarks/realtime_presentation --profile=standard --runs=5 --query-backend=cpu
cargo run -p wrela -- perf benchmarks/whole_frame --profile=standard --runs=5 --query-backend=wgsl
```

Paired comparison:

```bash
cargo run -p wrela -- perfcmp benchmarks/field_engine \
  --profile=standard \
  --baseline-ref=origin/main \
  --candidate-ref=HEAD
cargo run -p wrela -- perfcmp benchmarks/collision_perf \
  --profile=standard \
  --baseline-ref=origin/main \
  --candidate-ref=HEAD
cargo run -p wrela -- perfcmp benchmarks/realtime_presentation \
  --profile=standard \
  --baseline-ref=origin/main \
  --candidate-ref=HEAD
cargo run -p wrela -- perfcmp benchmarks/whole_frame \
  --profile=standard \
  --baseline-ref=origin/main \
  --candidate-ref=HEAD
```

For the current rendering diagnostic mode, use `cargo run -p wrela -- presentation-debug <path>`.
That is the quickest way to inspect pass-level rendering output when `presentation-plan` is not
enough.

For the canonical measurement lane, use `cargo run -p wrela -- perf benchmarks/whole_frame --profile=1080p120`.
That must complete, write the baseline JSON, emit `presentation_reports`, `collision_reports`, and `whole_frame_reports`, and report both per-scenario steady-state FPS and the suite median FPS for the fixed 1920x1080 closure protocol.

For closure failure analysis, use `cargo run -p wrela -- perf <suite-root> --profile=1080p120 --why-not-120`.
That keeps the closure verdict and FPS reporting, but also enables the more expensive WGSL workgroup sweep and hybrid-vs-dense-only comparison payloads so you can investigate the remaining bottleneck.

## Profiles

- `--profile=smoke`: correctness-oriented short run.
- `--profile=standard`: default baseline profile.
- `--profile=deep`: longer regression-hunting profile.

Per-scenario overrides are available for `perfcmp` with `--warmup-pairs`, `--measure-pairs`, `--min-effect-pct`, and `--confidence`.

## Realtime Presentation Scenarios

The `realtime_presentation` suite keeps the checks deterministic by combining fixed query grids with checksum-style assertions while also attaching a canonical multi-frame named-view probe per scenario in `bench.toml`. `wrela perf benchmarks/realtime_presentation ...` now derives its scenario runtime lane from presentation frame-cost reports rather than the raw query-fixture wall clock alone, records those presentation probes in the baseline JSON under `presentation_reports`, and prints `presentation-scenario` / `presentation-pass` lines with the quality tier, internal resolution scale history, steady-state FPS, bottleneck pass, acceleration artifacts, and per-pass work/cost breakdown.

The suite keeps backend selection explicit so benchmark lanes do not silently drift with `auto`. The canonical `1080p120` closure lane now lives in `whole_frame`; `realtime_presentation` remains the presentation subsystem probe, while the CPU oracle remains the companion reference in `compiler/tests/presentation_exec.rs`, the CPU-specific closure profile, and the collision closure lane. In default `1080p120` mode the rendering lane is measurement-first only; `--why-not-120` is the explicit opt-in for workgroup and dense-only diagnostics.

- `presentation_dense_constructive_geometry`: dense constructive solids built from lofts, sweeps, bends, and unions. This stresses candidate selection, hit resolution, and normal stability on heavily composed geometry.
- `presentation_repetition_heavy_scene`: repetition-heavy structure built from nested linear repetition and instancing. This measures repeat identity, instance stability, and traversal behavior in tiled layouts.
- `presentation_repeat_linear_solver_scene`: direct axis-aligned `repeat_linear` evidence lane. This keeps dense-parity and repeat-identity checks in the benchmark fixture while the phase-37 CPU solver regression and presentation report counters prove repeat-aware traversal reduction and skipped repeated cells on the supported subset.
- `presentation_thin_stack_alias_prone`: thin stacked layers and near-touching surfaces. This exercises alias-prone rays, epsilon sensitivity, and shallow-angle normal consistency.
- `presentation_media_radiance_scene`: radiance- and media-enabled presentation content. This covers surface sampling, radiance lookup, medium evaluation, and the frame path for volumetric scenes.

The `1080p120_closure.toml` protocol files define fixed closure lanes. Their scenario ids are prefixed with `closure_1080p120_` so they stay visually distinct from the microbench scenes, and their view definitions use `realtime_quality(target_fps = 120)` with fixed 1920x1080 framing. The canonical representative lane is `whole_frame`, while `realtime_presentation` and `collision_perf` remain subsystem suites.

Closure lane coverage currently includes:

- `closure_1080p120_dense_constructive`: candidate selection and hit-resolution stress on composed geometry.
- `closure_1080p120_repetition_heavy`: repeat identity and instance-stability stress on tiled repetition layouts.
- `closure_1080p120_thin_stack_grazing`: alias-prone, shallow-angle ray coverage for thin-stack geometry.
- `closure_1080p120_media_radiance`: radiance and medium evaluation coverage for volumetric scenes.
- `closure_1080p120_transformed_primitive_gallery`: translated primitive-gallery coverage intended to keep transformed primitive hits and normals stable under the fixed closure protocol.
- `closure_1080p120_mixed_opaque_conservative`: support-bounded guard plus conservative-field mix intended to stress opaque/conservative interplay in the representative presentation lane.
- `closure_1080p120_cache_stress_motion_path`: repeat-heavy motion-path coverage intended to churn cache/acceleration reuse under fixed camera-relative movement.
- `closure_1080p120_camera_motion_temporal_reuse_clipmap_churn`: camera-motion coverage intended to exercise temporal reuse and clipmap-like view churn in the canonical presentation lane.

## Collision Perf Closure Scenarios

The `collision_perf` closure manifest is the dedicated collision-side companion to the presentation closure lane. It keeps the benchmark protocol fixed while stressing point occupancy, dense ray casts, overlap bursts, repeated sweeps, and TOI transition reuse under the fixed 1080p120 protocol.

If you need to interpret the closure output or compare CPU oracle versus WGSL resident lanes,
read the specialized playbooks before changing the benchmark manifests.

- `closure_1080p120_point_occupancy_burst`: many point occupancy probes across the canonical collision scene.
- `closure_1080p120_dense_ray_casts`: dense ray-cast coverage for the collision throughput lane.
- `closure_1080p120_overlap_burst`: overlap-heavy burst coverage around the canonical collision cluster.
- `closure_1080p120_repeated_sweeps`: repeated sweep-like probes through static clutter.
- `closure_1080p120_toi_transition_reuse`: transition-scoped TOI-style reuse coverage for the collision closure lane.

## Whole Frame Closure Scenarios

The `whole_frame` closure manifest is the canonical 1080p120 lane. Each scenario pairs a fixed presentation view with a representative collision workload so the closure report charges the combined frame instead of sampling the subsystems independently.

- `closure_1080p120_dense_constructive_dense_ray_casts`: dense constructive presentation plus dense ray casts.
- `closure_1080p120_repetition_heavy_repeated_sweeps`: repetition-heavy presentation plus repeated sweeps.
- `closure_1080p120_thin_stack_point_occupancy`: thin-stack grazing presentation plus point occupancy bursts.
- `closure_1080p120_media_radiance_overlap_burst`: media/radiance presentation plus overlap bursts.
- `closure_1080p120_camera_motion_toi_reuse`: camera-motion temporal-reuse / clipmap-churn presentation plus TOI transition reuse.

## Artifacts

- JSON report: `.artifacts/perf/perfcmp-report.json`
- Markdown report: `.artifacts/perf/perfcmp-report.md`
- Suite baselines: `.artifacts/perf/baselines/<suite>-<profile>-<ref>.json`
- Metrics: `.artifacts/perf/metrics/*.json`
