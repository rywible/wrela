# Shaded appearance: first G4 viability decision

**Decision: change the representation.** Preserve the frozen fit and the renderer as an
experimental control. Stop spending training cycles on material-moment error. Neither
of the two permitted optimizations meets quality, runtime and resident-memory requirements
together. [G1–G7](neural-rendering-goals.md) are unchanged and remain unmet.

Work began 2026-10-27 15:04:56 UTC, with a 17:04:56 UTC deadline, solo on Pop!_OS.
The architecture decision was reached within the two-hour block; final archival/check time
is recorded in the evidence. No training steps or new reference captures occurred in this block.

The useful result is **shading visible material components before mixing them**, combined
with source-derived compact geometry. Relative to the full BC atlas, the trained AA2 path
has 80.20% lower mean FLIP, 94.99% lower LPIPS and +9.442 dB mean PSNR on this diagnostic.
That is not a large demonstrated learning advantage: the existing conventional
source-calibrated primitive control, rendered identically, is only 0.388 dB behind,
The trained parameters reduce mean FLIP by 11.30% and LPIPS by 10.73% versus that control. Both take essentially the same storage and execute the same algorithm.
All 24 trained-vs-calibrated comparisons meet the unchanged numeric quality tolerances.

![Shaded quality and retained silhouette failure](figures/neural-g4-viability.png)

## What was actually compared

One Great Tree, seed 7, at 64×64 output pixels; two previously published views
(stored anchor 103 and one off-grid direction), four fixed illuminations and three backgrounds:
24 shaded comparisons, not 24 independent scenes. Static hard-cut source cards and authored
wood, production leaf response with analytic sky, flat source bark. There is no wind,
scene shadow transport, fog, temporal reconstruction, camera-cut test or unseen asset here.
AA1 is one spatial sample per pixel; AA2 is 2×2; AA8 is 8×8.

The 1,897 fitted optical primitives are explicit optimized half-precision parameters,
**not an MLP or a neural decoder**. Total packed representation is 107,672 bytes,
including common values and wood. The primitive payload remains
`dc329e4b4c4eaa8ad0bbbb93eeea90cb2a94f4a0f01b284487f344d4ee2ff71c`.
The [prior continuation](neural-source-primitives.md#fixed-longer-optimization-with-exact-continuation)
is frozen.

All methods receive the same authored source information. No target image, current-view
ground truth or future sample is supplied to an inference path. Conventional controls are:

- The full source-integrated 16×16-view atlas: two BC7 layers plus an independent-depth BC5
  layer, 3 MiB, decoded-material bilinear filtering, four-view interpolation and the established
  one-step depth reprojection. Its raw GPU output agrees with the previously checked CPU
  decoder to the recorded 2e-5 maximum tolerance. A first implementation omitted reprojection;
  its off-grid parity failure and all resulting artifacts are preserved as `bc-no-parallax`.
- The existing source-derived 1,897-primitive model with the previously selected **two global
  calibration parameters**. This is not an uncalibrated strawman: its earlier fixed 49-candidate
  calibration used stored development views. No new calibration occurs here. It receives
  exactly the same pre-blend shading, wood renderer, sorting, AA2, output and composite
  as the fitted model. All 107,672 packed bytes are retained, and its active weight hash
  is recorded separately from the frozen trained weights.
- The original conventional source-card and wood renderer, with the same source material
  equations and 8×8 sampling. Cached cards, leaf-pattern texture, depth, scratch and output
  are counted. Source fields cook the cards at initialization; decoding/drawing them and
  wood, visibility, shading, integration and compositing occur in every measured frame.

These are the strongest applicable existing controls tested in this block, not proof of an
optimal conventional renderer. In particular, the BC atlas averages material roles: crediting
the learned path with its entire improvement over that atlas would confound representation,
shading order and learning. The calibrated and direct-source controls expose that confound.

## Two optimization experiments, then a decision

1. **Shade before blending, AA1.** Keep all primitive parameters and sorting unchanged.
   Shade each leaf primitive and shade wood with its bark response, then blend premultiplied
   radiance into one RGBA16F target. This replaces three material targets. Coverage is
   bitwise identical to the initial GPU path in all eight view/light outputs. Numeric
   quality passes versus BC rise from 13/24 to 22/24, but the two remaining cases regress
   as far as 11.558 dB. Runtime does not recover the BC budget.
2. **Four spatial samples, AA2.** Test the demonstrated silhouette/sampling failure.
   Mean quality improves strongly and the worst boundary error falls from 2.236 to 1.414
   pixels. The same two cases still fail. Scratch grows, and complete allocation reduction
   versus BC is only 6.015×. There was no third candidate optimization.

Mean displayed-image metrics, using the existing preprocessing and pinned evaluators:

| Path | PSNR dB ↑ | FLIP ↓ | LPIPS ↓ |
|---|---:|---:|---:|
| BC atlas + depth reprojection | 30.685 | 0.06237 | 0.037719 |
| Frozen model: material moments, AA1 | 30.406 | 0.05049 | 0.035133 |
| Experiment 1: shade then blend, AA1 | 33.431 | 0.02107 | 0.017743 |
| Experiment 2: shade then blend, AA2 | 40.128 | 0.01235 | 0.001889 |
| Conventional calibrated primitives, AA2 | 39.740 | 0.01392 | 0.002116 |
| Original source cards + wood, AA8 | 48.044 | 0.00651 | 0.000301 |

The final trained AA2 path still fails stored view 103, sky-only illumination, against
gray and blue backgrounds. PSNR losses versus BC are **3.530 and 5.980 dB**;
LPIPS ratios are **1.314× and 2.309×**. The excellent mean does not erase these failures.
The conventional source AA8 control has higher mean quality than either compact primitive
path; it is slower and larger. Neither conventional control is silently weakened to
manufacture a learned win.

Coverage is scored before tone mapping. Precision and recall use coverage ≥0.5;
boundary error is the symmetric 99th-percentile distance between binary boundaries.
The table gives the worst view for precision, recall and boundary error:

| Path | Mean coverage MAE ↓ | Precision ↑ | Recall ↑ | Boundary p99, pixels ↓ |
|---|---:|---:|---:|---:|
| BC atlas + depth reprojection | 0.01678 | 97.89% | 93.82% | 2.038 |
| Frozen model: material moments, AA1 | 0.01788 | 99.13% | 97.09% | 2.236 |
| Experiment 1: shade then blend, AA1 | 0.01788 | 99.13% | 97.09% | 2.236 |
| Experiment 2: shade then blend, AA2 | 0.00883 | 97.80% | 98.54% | 1.414 |
| Conventional calibrated primitives, AA2 | 0.00935 | 97.81% | 98.80% | 1.414 |
| Original source cards + wood, AA8 | 0.00429 | 99.49% | 99.57% | 1.000 |

This is a static silhouette diagnostic, not achievement of G3's temporal, disocclusion or
thin-feature acceptance tests.

## Complete rendering cost on the AMD machine

Ryzen 5 5500U, hardware AMD Radeon Graphics / RADV RENOIR, Mesa 26.1.6,
Chrome 154.0.8037.57, pinned Wrela 0.9.0. Each profile has 60 warmup and 240 measured
frames, one worker, 64×64 output. View changes every frame and illumination every two frames,
forcing model updates and sorting. Captures, profiles and scoring run sequentially.
Raw profiles, device descriptions, compiled shaders, programs and commands are retained.

The candidate frame includes half decoding, stable CPU depth sort and index upload,
procedural wood vertex evaluation, optical visibility, blending, integration and final
tone-mapped screen composite. It uploads 8,196 bytes/frame including uniforms, versus
96 bytes for BC. The direct source control has no global sort. The calibrated model
executes the same algorithm as the trained model.

All times below are milliseconds:

| Path | Saturated GPU p95 | Paced GPU p95 / p99 | CPU frame p99 | CPU start → GPU complete p95 / p99 |
|---|---:|---:|---:|---:|
| BC atlas + depth reprojection | 0.0269 | 0.1036 / 0.1046 | 0.370 | 1.906 / 3.812 |
| Frozen model: material moments, AA1 | 0.0758 | 0.2892 / 0.2899 | 1.020 | 4.985 / 5.215 |
| Experiment 1: shade then blend, AA1 | 0.0736 | 0.2915 / 0.2920 | 0.983 | 5.276 / 5.513 |
| Experiment 2: shade then blend, AA2 | 0.0981 | 0.3141 / 0.3161 | 0.973 | 4.340 / 4.528 |
| Conventional calibrated primitives, AA2 | 0.0855 | 0.3125 / 0.3149 | 0.862 | 4.858 / 5.131 |
| Original source cards + wood, AA8 | 1.0488 | 1.6842 / 1.7184 | 0.598 | 3.900 / 4.148 |

GPU spans cover the registered passes, including final compositing. They **do not by
themselves include a queue upload before the first timestamp**. CPU frame time includes
sorting, command recording and submission, but is not GPU completion time. Therefore a
separate observer follows the existing queue-completion promises from CPU frame start
through completion of all that frame's queued work. It adds no waits, includes uploads
and compositing, and excludes post-run image readback. That wall-clock column also includes
browser scheduling and driver notification, and varies between runs; it is not an
input-to-photon measurement or a basis for claiming the trained model faster than calibration.
Instrumentation is confined to copied evidence builds. The ordinary GPU/CPU profiles are
unmodified builds.

The trained AA2 path is 3.031× / 3.021× BC's paced GPU p95 / p99; saturated GPU p95 is
3.649× BC. Its observed complete-work p95 is 2.277× BC. G4's ≤1.1× limit is not met.
Paced frame intervals near 16.7 ms reflect the 60 Hz schedule and are not substituted for
rendering latency. Initialization/source cooking is outside steady-state frame timing;
its allocation peak is included below. No G6 whole-scene or G7 build-time claim is made.

## Resident memory, including scratch and caches

Packed model size is not residency. The engine counter misses executor uniform/upload
rings and the offscreen screen target. A measurement-only observer records **every**
WebGPU buffer and texture creation/destruction, including retained upload capacity,
scratch, output, dummy/fallback resources and cache backing storage. Shader/pipeline
and driver allocation bytes are not exposed by that interface, so actual DRM residency
and whole-browser proportional resident memory are recorded separately.

| Path | Engine appearance/scratch bytes¹ | All live requested GPU bytes² | Peak requested GPU bytes² |
|---|---:|---:|---:|
| BC atlas + depth reprojection | 3,145,844 | 4,341,876 | 7,487,620 |
| Frozen model: material moments, AA1 | 230,316 | 639,916 | 656,300 |
| Experiment 1: shade then blend, AA1 | 164,796 | 574,396 | 590,780 |
| Experiment 2: shade then blend, AA2 | 312,252 | 721,852 | 738,236 |
| Conventional calibrated primitives, AA2 | 312,368 | 721,968 | 738,352 |
| Original source cards + wood, AA8 | 5,046,136 | 5,455,736 | 5,472,120 |

¹ Excludes the identical 65,536-byte output buffer only; includes wood, sorting indices,
depth and image targets. ² Includes that output, screen, upload/uniform rings and all other
observed buffers/textures. No fallback atlas is loaded by the compact paths. Inactive
counterpart payloads are removed from each build. The source-control helper added 116
dummy bytes to the later calibrated build and final checked-in default; this is explicitly
counted for calibration and the final native check, not normalized away. The tables retain
the exact originally measured AA2 build; final default images are byte-identical.

The BC upload ring retains 1,048,576 bytes after initialization; compact/source paths
retain 262,144 bytes. Every path retains a 65,536-byte uniform ring and 16,384-byte
offscreen screen. The three-target path initially appears to save 13.659× by the engine
counter, and the one-target AA1 path 19.089×, but the latter is only **7.559×** with complete
requested allocations. Final AA2 is **6.015×** versus BC and **1.00016×** versus conventional
calibration. Neither is 16×.

Actual post-run residency snapshots, MiB, from the same isolated browser process groups:

| Path | DRM resident VRAM | DRM resident GTT | Browser group PSS |
|---|---:|---:|---:|
| BC atlas + depth reprojection | 24.55 | 9.20 | 464.48 |
| Frozen model: material moments, AA1 | 16.57 | 9.38 | 455.31 |
| Experiment 1: shade then blend, AA1 | 16.57 | 9.40 | 456.44 |
| Experiment 2: shade then blend, AA2 | 16.56 | 9.30 | 456.80 |
| Conventional calibrated primitives, AA2 | 16.82 | 9.38 | 456.54 |
| Original source cards + wood, AA8 | 16.51 | 9.40 | 456.91 |

DRM clients are deduplicated by device/client ID. These counters include driver/client
allocations and retained caches; PSS includes CPU/WASM data, sort scratch, code and browser
caches. The machine uses shared memory: **do not add PSS, VRAM and GTT columns** as if they
were disjoint per-asset ownership. The single-instance fixture's shared host overhead
cannot establish amortized multi-instance residency. WASM committed/used and reserved
values, full allocation descriptors and raw `/proc` records are retained. This is enough
to reject a 16× total-residency claim here, not a claim about every production scene.

## Next architecture-level bet

Keep exact authored wood and material roles. Replace globally sorted, point-sampled
appearance splats with **source-compiled visibility layers and pixel-footprint coverage**.
Use source geometry, field support and known visibility to bound occupied regions and
resolve thin silhouettes without allocating a supersampled image for every asset.
A learned component should predict a compact residual only where the deterministic
source-conditioned renderer has a demonstrated rendered error. Give the conventional
control those same layers, coverage and visibility inputs.

The shortest next discriminating experiment is one fixed-footprint layer/coverage
prototype on these same eight view/light images, against the frozen trained and calibrated
controls, with one scratch pool, full update/sort/composite accounting and a small batch
of instances. First resolve the two sky-only silhouette failures and count amortized
resident bytes. Train nothing until that measured comparison identifies a useful residual.

The remaining acceptance gaps are unseen views/individuals, motion, scale range 8–512 pixels,
richness scaling, shared-memory scaling and whole-scene integration. They are not needed
to decide against the current configuration: it already fails the joint public quality,
memory and cost test. This work does not retire neural rendering or change G1–G7.

## Reused shaded references and proportional verification

The completed shaded references shade each visible source fragment before exact Q20
integration. Existing 16→32 sampling failed all eight cases; the completed 32→64
comparison passed all eight fixed RGB/coverage tolerances. This block uses those AA64
references unchanged. Material-average controls expose nonlinear shading, normal
renormalization, material quantization and leaf/wood role loss; they do not isolate only
the mathematical averaging operation.

The production `leaf_response` extraction has the previously completed 4,096-case GPU
byte-parity check against the original expression
(`a2b14264783b3078b2d8901b92be0ce771a39eb9483e5bf4b185604271a24c25`),
an independent float64 equation check (maximum 6.57e-6), and a transmission-negative control.
Existing exact pixel integration and GPU/CPU material-response checks are reused.
No repeated reference audit or new reference captures were needed.

Preprocessing is engine tone mapping at exposure 0.62 followed by sRGB, applied after
premultiplied linear compositing over each background. FLIP uses LDR 67 pixels/degree,
RGB inputs; LPIPS uses AlexNet v0.1, sRGB [-1,1], without resizing or cropping.
Mean PSNR is the arithmetic mean of case PSNR values, not PSNR of pooled MSE.
Pinned implementations and primary sources are in [citations](citations.md).
The established probes and all executable GPU captures/profiles are the substantive checks;
the final frozen checkout is checked for formatting/types and byte-identical output.
No full-suite or PR-readiness claim is made.

## Evidence and replay

The checked-in `studio/appearance/runtime` defaults to the frozen trained AA2 diagnostic.
Its actual half words, common values and wood are included as Wrela constants so a clean
checkout can build it. This is not enabled in the production renderer.

On Pop!_OS from the worktree:

```bash
wrela tool appearance -- runtime
wrela profile studio/appearance/runtime --frames 240 --warmup 60 --workers 1 --size 64x64 --chrome --json
```

The remote evidence root is
`/home/ryanwible/worktrees/wrela-neural-frontier/build/neural-evidence/g4-viability-v1`.
Each variant retains its exact `runtime-source.tar.gz`, compiled profile build, raw pixels,
profiles, individual scores, allocation/DRM/PSS and completion records. Frozen full BC
and candidate data live in `frozen/`. External scripts under
`/home/ryanwible/.local/state/wrela-neural-frontier/` are archived with the evidence;
they are not repository Python code. Restoring a variant archive restores its actual data
and configuration; save the checked-in version before doing so. The calibrated baseline's
active weight SHA is in its `record.json`.

The old bad projection baseline, initial build/capture failures, failed material-moment
path, failed two targeted optimizations and every quality tail remain preserved.
A manifest binds this evidence, its reference dependency and exact source/script archives.
Evidence digest: `44cc42b8607d0781958bbd4409c360cd7ab8a7288d5a8797dfa091d4f8ef46b6`.
