# Winter rendering overhead, material reuse, and scalability

Implemented and measured on Apple M4 / 16 GB, Chrome hardware WebGPU/Metal, native 1920×1080, balanced profile. The current scene takes **6.23 ms median / 7.14 ms p95 GPU time in sustained rendering**, completing approximately **129 render frames/second**. At a normal 60 Hz cadence it still reports approximately 13–14 ms. These are different load/presentation regimes; the difference is not a twofold code optimization.

The earlier interpretation that a 13–14 ms paced GPU interval established the scene's intrinsic cost ceiling was too pessimistic. Scheduling and power management are plausible contributors, but clocks were not captured, so their individual contributions remain unproven. Four-frame bursts amortize presentation/readback and keep work available to the GPU. They establish render throughput, not a demonstrated 120 Hz game or a universal sub-8-ms guarantee.

## Production changes

- Native-resolution temporal reconstruction writes both HDR history and the final display color from the existing tiled compute pass. This removes the separate display pass and HDR reread. It requests optional BGRA storage support when available; reduced-resolution rendering and unsupported devices retain the compute-plus-display path. The raster fusion experiment remains an explicit control and is not the default.
- Material signatures share packing within a frame. Identical materials no longer rebuild large float arrays and string keys for each tree. Skinned/water/deformed surfaces skip unused batching-signature work. The cache is frame-local, so in-place authoring edits cannot leave a stale signature.
- GPU timing excludes atmosphere work when cached products are reused. An empty timestamped compute pass previously produced a stale interval: one live sample claimed 756 ms while display pacing remained near 60 Hz. The stationary-camera regression now reports zero atmosphere work and a maximum GPU interval of 14.94 ms over 91 frames. Corrected live-loop maximum was 14.81 ms in that check.
- Frame duration covers the earliest active pass beginning through the latest ending. Raw overlapping intervals are available for inspection; they are not additive exclusive costs. The readback ring grows from 3 to 16 slots, and dropped timing counts are exposed. Benchmarks require complete frame attribution. Measured bundles are now saved beside future browser evidence.

The isolated 1080p reconstruction test measured **1.60–1.70 ms** for fused compute presentation versus **1.77–1.98 ms** for the separate compute/display path. Raster fusion was slower at 2.29–2.45 ms. The matched sustained whole-scene comparison put fused medians at 6.16–6.23 ms versus 6.42–6.49 ms for the two-pass control. This is a modest GPU saving.

In an alternating CPU batching microbenchmark, 400 identical-material surfaces fell from 1.61 to 0.18 ms, and 1,600 from 6.67 to 0.69 ms, with identical batches. That microbenchmark uses Bun rather than Chrome. In the browser, the foliage ×4 scene fell from an initial approximately 10.9 ms submission median to approximately 7.2 ms in the later three-trial paced run; this sequential comparison is less controlled than the isolated batching test.

## Where the GPU work goes

Repeated-pass probes in sustained four-frame bursts estimate the following incremental costs. For each trajectory frame, subtract the one-pass condition from the four-pass condition and divide by three, then take the median across three trials.

| Work | Estimated incremental cost |
|---|---:|
| Opaque surfaces and sky | 2.23 ms |
| Water shading/transport pass | 1.86 ms |
| Temporal reconstruction and display | 1.57 ms |
| Shadow rendering | 0.17 ms |

These are approximate marginal costs, not an exact additive hardware capture. The water probe uses less-equal depth in both diagnostic conditions so repeated water still executes shading. Repeating passes at 60 Hz gave misleadingly flat or even decreasing times; sustained-load probes made the cost differences visible. A Metal System Trace attempt crashed while saving and is not used as evidence.

## Compiler material-data experiment

The compiler can partially evaluate the existing periodic material-noise field into a bounded lattice. Two RGBA32 texels store each cell's eight original corner samples. The shader preserves the same interpolation and footprint filtering, including world-origin rebasing; samples outside the cached region use the original evaluator. Color, lighting, layer blending and animated coordinates remain dynamic. This is an exact-field noise product within floating-point rounding, not a complete surface-material atlas.

The default experiment covers a 64³-cell region and costs 8 MiB. Its 2,048 GPU probes include 921 hits and 1,127 fallback samples, with maximum absolute difference 1.19e-7. Three scene-linear image comparisons had maximum absolute difference 0.000122 and relative RMS at most 0.000204%.

**It remains off by default in both the host and renderer.** In three sustained native-resolution trials, cached/uncached medians were approximately 6.23–6.29 / 6.29 ms and p95 approximately 7.14–7.21 / 7.14 ms. That does not establish a useful win for 8 MiB. The compiler product, GPU consumer, domain fallback, and validation remain available for workloads where reuse might earn its memory cost. The successful default material reuse is the CPU packing cache above.

## Richer workload results

Three rotated trials per workload, 91 warm-up and 91 measured frames per trial: **273 measured frames per condition, 2,730 total**, with no dropped samples. Each condition retains the authored trajectory, native resolution and temporal reconstruction. Every surface must be rendered or explicitly culled.

| Workload | GPU median ms | GPU p95 ms | CPU submission median ms | Completed render frames/s |
|---|---:|---:|---:|---:|
| Winter baseline | 6.23 | 7.14 | 2.80 | 128.7 |
| Foliage ×2 | 6.75 | 7.41 | 4.00 | 119.0 |
| Foliage ×4 | 7.01 | 7.86 | 7.10 | 104.3 |
| Characters ×4 | 6.42 | 7.27 | 2.90 | 125.6 |
| Characters ×8 | 6.55 | 7.41 | 3.10 | 121.9 |
| 4 point lights | 7.80 | 8.65 | 3.10 | 105.9 |
| 8 point lights | 9.57 | 10.22 | 3.10 | 90.9 |
| 512 moving debris pieces | 6.36 | 7.21 | 5.40 | 119.0 |
| Two material layers | 6.68 | 7.73 | 3.20 | 119.1 |
| Combined workload | 11.53 | 12.26 | 11.70 | 65.9 |

The combined workload contains four times the foliage and characters, eight point lights, 512 moving opaque debris pieces, and two material layers. It reaches approximately 356,496 visible camera triangles and 148 reported draws versus 198,068 and 115 for the baseline. Baseline owned GPU memory is approximately 175.4 MB.

The sustained data identifies **lighting as the strongest tested GPU scaling pressure**, while many surfaces and debris increase CPU submission cost. The combined workload has little margin at approximately 66 completed render frames/second. GPU and CPU time overlap and must not be added as if they were sequential stages.

A separate live baseline advances the current simulation, extracts the scene and renders it every frame. In the final scalability run, CPU p50/p95 was 4.4/4.7 ms, including approximately 0.4 ms simulation and 0.2 ms extraction at median; normal presentation cadence was approximately 60 Hz. This adds evidence beyond pre-evaluated render packets, but excludes application UI, networking and unimplemented gameplay.

These are controlled scaling tests of current content. The additional actors reuse the current rabbit's posed packets and the foliage uses existing geometry. Debris is opaque instanced geometry; alpha particle overdraw and dense cutout foliage are not represented. More detailed characters, streaming, complex gameplay and other hardware still require representative content tests. This result does not certify AAA scalability.

## Validation and reproduction

- 242 unit tests passed; zero failures. TypeScript checks for both configurations and workspace boundary checks passed.
- 21 GPU temporal cases across reference compute, raster control and fused storage cover stable history, reactive water, identity changes, disocclusion, resets, offscreen history and neighborhood clipping. Existing sky-irradiance and shadow-domain GPU checks pass.
- Three matched full-resolution display captures differ by at most one 8-bit code value per channel; RMS channel differences are below 0.15 code values. HDR history behavior remains matched.
- Compiler lattice CPU tests, GPU probes and scene comparisons pass. Workload tests verify counts, unique identities, source preservation and animated debris.

```sh
bun run perf:render
bun run perf:render:paced
bun run perf:reconstruction
bun run perf:passes
bun run verify:material-cache
bun tools/rendering-compiler/static-timing-check.ts
bun tools/rendering-compiler/temporal-check.ts
```

The `perf:render`, `perf:reconstruction` and `perf:passes` scripts explicitly use sustained four-frame bursts; `perf:render:paced` measures normal presentation cadence. Interpret their results separately. Full summaries and raw-evidence paths are recorded in [the accompanying JSON](winter-rendering-scalability-2026-09-21.json). Each cited browser run has a source-stability manifest. Earlier exploratory runs and the failed Metal trace are preserved under `output/rendering-priorities` and are not substituted for the final measurements.
