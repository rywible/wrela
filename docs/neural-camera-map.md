# Camera reprojection map: retire the arithmetic shortcut

2026-10-29. Parent `8442e2728e359a15c077b233c91a7d7808f34b5c`. **Retire this implementation.**
The per-frame map has no demonstrated joint advantage over the retained production
renderer. One measured complete GPU median rises 0.658%, requested resident
allocation is unchanged, and rendered-quality effects are small and mixed.
This is insufficient evidence to promote it; it is not proof of a stable slowdown.

The first complete quality/runtime/residency assessment took
**9.71 minutes**; the decision took
**16.36 minutes**. Zero optimization followups were used.
No training or reference-algorithm expansion. G1–G7 and all eight frozen artifacts
remain unchanged. Runtime is restored exactly to the parent.

## Question and same-information comparison

Can the engine's declared camera structure eliminate per-pixel normalization and
world reconstruction without harming the rendered result or complete cost?
[Camera](../engine/view.wrela) declares a reversed infinite perspective projection
and a basis. In real arithmetic, the world point multiplied by reversed depth is
near times the unnormalized view direction plus depth times the eye.
The candidate composes this basis with the previous view projection on the CPU
once per frame. Static-surface reprojection uses the resulting homogeneous map.
Explicit/deforming motion and sky keep their established paths.

This identity does not imply floating-point equality: normalization, dot products,
matrix composition and division change rounding and can change discrete history
sampling. The implementation was compared to rendered references, not accepted
from algebra alone. The same camera, geometry, motion, histories, source
information and samples are available to both methods. The baseline includes the
retained [source-region specialization](neural-motion-regions.md).

## Complete cost and resident observations

Pop!_OS, AMD RADV RENOIR PCI 1002:164c, Ryzen 5 5500U, Mesa 26.1.6;
Chrome 154.0.8037.57; Wrela 0.9.0 compiler
`de9b81e52a7747870dfbd5223cb8aeb41e584d23`. Full clearing at
1920×1080, production 2× reconstruction, one worker, 600 warmup frames and
60 measured frames. Adaptive workload and pacing are disabled for the saturated
run. The separate paced run and allocation replay use the same build.
No power/thermal lock; these are single-run comparisons, not confidence intervals.

| Measurement | Baseline | Homogeneous map |
|---|---:|---:|
| Complete GPU median, ms | 25.37972 | 25.54684 |
| Complete GPU p95, ms | 26.99724 | 27.00160 |
| Temporal pass median, ms | 4.31520 | 4.36144 |
| CPU frame-call median, ms | 1.79500 | 1.79500 |
| Separate paced GPU p50, ms | 25.39132 | 25.46176 |
| Separate paced GPU p95, ms | 26.98113 | 27.25374 |
| Separate paced GPU p99, ms | 27.94528 | 28.37738 |
| Requested live bytes | 405,267,256 | 405,267,256 |
| Requested peak bytes | 582,445,428 | 582,445,428 |
| Observed DRM VRAM bytes | 551,911,424 | 551,931,904 |
| Observed DRM GTT bytes | 20,180,992 | 20,180,992 |
| Observed browser PSS bytes | 815,462,400 | 841,116,672 |

Complete GPU span includes actual vegetation depth/shading, culling, wood,
updates, clouds, lighting, temporal work, bloom and composition. No decode/sort
work is excluded if present in this production path. This is not the specialized
G4 representation fixture. CPU frame-call time is not presentation latency.
Requested totals include all resources, histories, caches and scratch; the observer
does not subtract unused fixture allocations. Physical figures are deduplicated
whole-client snapshots after warmup, not physical peaks or per-asset attribution.
UMA pools must not be added together. There is no memory win.

## Shaded quality and boundaries

Two executable analytic batches each capture six clips (1, 6, 13, 23, 74, 77),
four scales and 32 cold-start frames: **1,536 output frames** total.
Capture time is 24.13 + 24.44 seconds, excluding full-scene runs and CPU scoring.
Every target and visibility mask matches. The fresh baseline's 768 frames match
the established baseline exactly, allowing score reuse with pinned hashes.
Only 24/768 candidate frames match exactly; all candidate clips receive fresh scores.

Across 24 clip/scale comparisons, worst FLIP increase is 0.01818%,
worst LPIPS increase 0.04690%, worst PSNR change
-0.001947 dB, worst opaque temporal-residual increase 0.03419%,
and worst mixed-coverage boundary MAE increase 0.03379%.
These fit the numerical matched-quality tolerances on this development set.
They do not establish G3 silhouette/strand/trail gates, camera-motion generality,
or a meaningful quality improvement.

The full 1080p final display frame changes at 1,333 pixels by more than 2 byte
levels, with a maximum channel difference of 104. Both methods' repeat captures
differ by only one byte in one component. This motivated the existing shaded HDR
comparison; a large changed pixel alone says nothing about which image is better.

The established clearing capture batches four temporal scales plus the existing
256/1024/independent-1024 reference checks. All three reference buffers exactly
match the completed prior capture. Quality is scored against the already completed
16k/65k/independent-65k references from
[the reference diagnosis](neural-temporal-reference.md); no new high-sample
reference is generated. Against the 65k estimate:

| Scale | Baseline FLIP | Map FLIP | Baseline LPIPS | Map LPIPS | PSNR change dB |
|---|---:|---:|---:|---:|---:|
| 1× | 0.073293 | 0.073241 | 0.054550 | 0.054455 | +0.004865 |
| 2× | 0.096488 | 0.096435 | 0.157805 | 0.157666 | +0.006357 |
| 3× | 0.142255 | 0.142298 | 0.216691 | 0.217206 | +0.000965 |
| 4× | 0.183315 | 0.183278 | 0.320781 | 0.320895 | -0.000981 |

At 4×, 18 pixels change by more than 2/255 in the named view; nine improve and
nine worsen. Their mean absolute reference error rises from 0.029111 to 0.033294.
This is a post-hoc changed-region diagnostic, not a predeclared silhouette gate.
All three reference estimates and all per-scale deltas are preserved. The fixed
shot's quadrature reference is not an independent light-transport solution.

Use the established exposure-1 Reinhard view, FLIP at 67 pixels/degree,
LPIPS AlexNet v0.1 and PSNR after sRGB encoding. Metric definitions and upstream
citations remain in [the citation record](citations.md#neural-reconstruction-instrument).
No new external algorithm or implementation is imported.

## Decision and next architecture bet

Retire the homogeneous map and restore all three changed runtime files exactly.
There is no demonstrated cost reduction to compensate for added numerical
complexity, and no quality or memory advance. A paired timing campaign could
resolve the small timing difference, but would not establish an advantage on the
current evidence. Neither followup slot is spent on speculative projection tuning.

Reuse source-derived per-frame leaf geometry across the production vegetation depth and shading passes. The same card vertex program currently runs in both. Determine whether avoiding repeated source/instance/wind work can beat the existing draws after charging preparation, scratch, bandwidth, and changed-camera/wind updates. No new RGB information or weakened coverage is allowed.

This next bet targets duplicated work in the measured vegetation passes, which
together take about 7.8 ms here. Their combined time is not an estimate of removable
vertex cost; the first experiment must isolate that uncertainty and be rejected
if preparation or memory outweighs the saving. Conventional methods receive the
same source program and metadata. This is a proposed experiment, not a claim.

## Preserved evidence and replay

Evidence root on Pop!_OS:
`/home/ryanwible/worktrees/wrela-neural-frontier/build/neural-evidence/camera-map-v1`.

Both executable/source snapshots, complete profiles, allocation events and DRM/PSS
observations, analytic captures and metric references, candidate patch, production
HDR captures and all scoring rows are preserved. Failed results remain available.

External scripts live in the task state directory and are archived in the evidence.
The run_camera_map.py script prepares, checks and batches the two analytic variants,
serializes full-scene profiles/residency and the established HDR capture, then scores
analytic results. score_camera_map_hdr.py homogeneous finishes the reference-relative
comparison. Redirect the recorded absolute R/S/E paths to a fresh checkout/root
before replay; never overwrite sealed evidence. document_camera_map.py records the
decision and restores runtime. finalize_camera_map.py pins artifacts and toolchain.

Changed-code format/type checks passed; the candidate executed on AMD.
Retirement restores the exact already-tested parent runtime, so no repeated
unchanged-engine test suite is run. A first documentation-tool invocation failed
JavaScript parsing before any command executed; no rendering or runtime was affected.
Only this block's benchmark append is staged; inherited benchmark edits remain unstaged.

Manifest SHA-256: `a81e892f627c0e228fa50f798d9e369c8ec64d9a6c7dab308aeb86ea079c8425`.
