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


## Replacement experiment: reject two visibility slabs

**Decision: retire the two-slab radiance mixture.** This subsequent experiment tests
the replacement proposed above, with two targeted follow-ups after its initial result.
It does not reopen the first block's optimization budget. The first block concluded
within 67 minutes; this replacement's plan was recorded at 16:29:38 UTC and its decision
at 16:46:09 UTC on 2026-10-27. Zero training steps and zero new references.
G1–G7 remain unchanged and unmet.

The frozen 1,897-primitive payload is unchanged, SHA-256
dc329e4b4c4eaa8ad0bbbb93eeea90cb2a94f4a0f01b284487f344d4ee2ff71c.
The conventional source-calibrated payload is unchanged, SHA-256
065549765511b660a115519d86c2d307bf04c5d56bc2536f17d60d80bb111114.
Both receive the same source geometry, materials, field support, authored wood
triangles, lighting and four quarter-pixel sample locations. The strongest ordered
calibrated control and established BC/depth control remain in the comparison.
These are fitted explicit optical primitives, not an MLP runtime decoder.

The initial replacement removes global CPU sorting and supersampled image targets.
Each primitive averages four opacity samples, converts the result to optical
thickness, and additively accumulates thickness-weighted shaded radiance in two
depth slabs. Far then near slabs resolve over point-sampled source wood. Support
quads expand to cover the four sample positions. Within each slab, colors are
mixtures rather than ordered per-fragment compositing.

This belongs to the general family of bounded-storage commutative transparency
approximations discussed by [McGuire and Bavoil (2013)](https://www.jcgt.org/published/0002/02/09/).
It is our own optical-depth/slab experiment, not a reproduction or novelty claim.

The two follow-ups answer separate measured questions:

1. **Local front anchor:** replace the global split with a per-pixel nearest contributing
   primitive depth, using the source shader's 2.4 m attenuation length for the front
   slab. An extra visibility pass recovers only 0.502 dB while raising paced GPU
   p95 from 0.376 to 0.558 ms.
2. **Independent coverage samples:** keep the original global split, but accumulate
   four independent optical thicknesses before averaging final opacities. This retains
   overlap correlations lost by per-primitive averaging. An extra 32,760 live scratch
   bytes recovers only 0.101 dB. Point-sampled wood and slab color mixtures remain.

No third follow-up is taken. All failed outputs, the local-front compile failure,
exact source/builds and measurements are retained.

### Rendered quality and silhouettes

Same completed AA64 references, two views, four lights, three backgrounds and metric
preprocessing as above; no crop, threshold or acceptance change.

| Path | Mean PSNR dB | Mean FLIP | Mean LPIPS | BC quality gates passed |
|---|---:|---:|---:|---:|
| BC atlas + depth reprojection | 30.68527 | 0.062373 | 0.037719 | reference baseline |
| Conventional calibrated, ordered AA2 | 39.73958 | 0.013921 | 0.002116 | 22/24 |
| Frozen fit, ordered AA2 | 40.12768 | 0.012348 | 0.001889 | 22/24 |
| Frozen fit, global two slabs | 32.84992 | 0.047489 | 0.018105 | 17/24 |
| Conventional calibration, two slabs | 32.71395 | 0.048277 | 0.018490 | 17/24 |
| Follow-up 1: local front | 33.35180 | 0.043010 | 0.016403 | 18/24 |
| Follow-up 2: four coverage samples | 32.95052 | 0.047441 | 0.017916 | 17/24 |

Global slabs have 9.58× the ordered fit's LPIPS; even the best local-front result
passes only 18/24 public cases. The conventional slab control receives identical
runtime inputs and costs, and tracks the fitted one closely. Its ordered version
remains substantially stronger.

Coverage MAE rises from 0.008832 for the ordered fit to 0.013472 for global slabs,
0.013470 with the local front, and 0.012288 with independent coverage samples.
Worst per-image coverage-error p99 rises from 0.113129 to 0.307728, 0.307730 and
0.316566 respectively. Worst silhouette boundary p99 remains 1.414 pixels for
all four; that metric alone hides substantial fractional-coverage damage.
Per-case precision/recall, soft IoU and shaded tails remain in the evidence.

![Ordered and two-slab appearance comparisons](figures/neural-visibility-layers.png)

### Complete cost and actual residency

Same Ryzen 5 5500U, hardware AMD RADV RENOIR, Mesa 26.1.6, Chrome
154.0.8037.57, pinned Wrela 0.9.0 / compiler
de9b81e52a7747870dfbd5223cb8aeb41e584d23.
One isolated 240-frame run after 60 warmup frames per path, at 64×64, one worker.
Captures, profiling and scoring run sequentially on Pop!_OS.

GPU spans include decoding, wood, visibility, layer accumulation/resolution and screen
compositing. CPU calls include view/light updates and command recording. No global CPU
sort or index upload remains. These paths write 784 uniform bytes per frame; the
local-front path writes 1,296 bytes. Initialization is outside steady-state time,
but its peak allocation and retained upload capacity are counted.

| Path | Paced GPU p95 / p99 ms | CPU call p99 ms | Saturated GPU p95 ms | CPU start to GPU completion p95 / p99 ms |
|---|---:|---:|---:|---:|
| Frozen fit, global two slabs | 0.375642 / 0.376428 | 0.553050 | 0.09548 | 4.220215 / 4.452451 |
| Conventional calibration, two slabs | 0.375042 / 0.375649 | 0.538050 | 0.11424 | 4.365491 / 4.726450 |
| Follow-up 1: local front | 0.558454 / 0.559950 | 0.702200 | 0.16740 | 4.490247 / 4.605481 |
| Follow-up 2: four coverage samples | 0.390446 / 0.395628 | 0.538550 | 0.10452 | 4.204834 / 4.305229 |

Ordered fitted AA2 has paced GPU p95/p99 0.314113/0.316113 ms, CPU p99 0.973050 ms
and saturated GPU p95 0.09808 ms. BC has paced GPU p95/p99 0.103642/0.104649 ms
and CPU p99 0.370000 ms. CPU sorting removal reduces CPU work, but no replacement
establishes a complete GPU improvement.

Completion uses the existing observer of submissions and completion notifications;
every submission within each CPU frame is accounted for. This includes browser
scheduling and driver notification latency, and is not input-to-photon timing.
Ordered AA2 completion p95/p99 was 4.340100/4.527881 ms; BC was 1.906018/3.812336 ms.
Small single-run differences are not a robust wall-time ranking.

| Path | Engine appearance + scratch bytes | All live requested GPU bytes | Peak requested GPU bytes | DRM VRAM / GTT MiB | Browser group PSS MiB |
|---|---:|---:|---:|---:|---:|
| Frozen fit, global two slabs | 190,088 | 599,688 | 616,072 | 16.582 / 9.289 | 454.899 |
| Conventional calibration, two slabs | 190,088 | 599,688 | 616,072 | 16.582 / 9.289 | 454.088 |
| Follow-up 1: local front | 190,088 | 599,688 | 616,072 | 16.590 / 9.383 | 462.865 |
| Follow-up 2: four coverage samples | 222,848 | 632,448 | 648,832 | 16.586 / 9.289 | 455.003 |

The engine column excludes only the identical 65,536-byte output. Full requested
allocations include that output, depth/coverage/radiance targets, wood, fallback/dummy
resources, uniform/upload rings and screen storage. Only a dummy sort-index entry
remains. No BC fallback atlas is retained. Global/local slabs save 16.9% against
the measured ordered fit's 721,852 requested live GPU bytes.

Against BC's 4,341,876 bytes, complete requested allocations save only **7.240×**
(global/local) or **6.865×** (four coverage samples). The engine-only number appears
to exceed 16× for global slabs, illustrating why it cannot substitute for residency.
Actual DRM residency and whole-browser PSS include retained driver/browser storage
not exposed by WebGPU descriptors. These are separate UMA observations: do not sum
PSS, VRAM and GTT or attribute shared host overhead to one asset. The final-default
116-byte helper difference remains as documented above.

### What the failures identify

Post-measurement attribution uses conservative bounding boxes of the already exported
source wood triangles. No new reference or changed quality gate. Inside these boxes
can include foliage: not all their error is established as wood-caused.

Outside projected wood bounds, four independent opacity values reproduce ordered
AA2 coverage within **0.0001531 mean absolute error**. This is agreement with the
four-sample control, not with perfect coverage. Yet display squared error against
the shaded reference rises from 0.9217 to 2.9194 on average over cases in that region.
Within conservative wood bounds, coverage differs by 0.048656 and display squared
error rises from 0.6505 to 5.0003. Raw masks and per-case values are retained.

Compact coverage accumulation is useful, but does not retain joint depth, shaded
color and wood/leaf visibility. More material-moment fitting would not repair this
renderer's lost ordering. Reject the mixture on quality, full storage and complete
runtime together; infrastructure progress is not a rendering win.

### Next architecture-level bet

Preserve **ordered subpixel visibility including wood**, while eliminating global CPU
sorting and full supersampled radiance targets. The shortest discriminating prototype
is tile-local ordered evaluation at the same four sample positions, with bounded lists,
explicit overflow/fallback accounting and both frozen fitted and conventional
source-calibrated inputs. Recover the ordered control's images before its CPU/storage
savings count. Reuse these references and metrics.

If transmittance reconstruction becomes the demonstrated bottleneck, test a small
source-conditioned transmittance model against conventional moment reconstruction
receiving the same moments, depths and coverage. [Tsopouridis, Vasilakis and Fudos,
Neural Moment Transparency (2024)](https://cgrg.eu/publications/neural-moment-transparency)
is prior work on learned fragment transmittance from accumulated moment statistics.
This is a possible model target, not an implemented result or imported performance
claim. Wrela's fields and source visibility can supply exact ordered supervision and
support bounds to both methods.

### Preservation and proportional verification

The runtime keeps rejected methods as diagnostic modes 4–6; its default remains
the frozen ordered AA2 renderer. Format/type checks pass; a final native capture
reproduces all eight measured AA2 outputs byte for byte. Completed reference and
response checks are reused. No full-suite/PR-readiness claim.

Evidence on Pop!_OS: build/neural-evidence/g4-visibility-layers-v1 in the worktree.
Each variant retains actual source/payload, compiled build, raw images, profile,
completion/allocation records and raw process/DRM records. The manifest references
unchanged prior comparison/reference manifests and binds exact final source/scripts.
Manifest SHA-256: `814030755e130a9fd7a600643fbcef0f303cb6db94d4108f4916cbe7af44ef9e`.


## Ordered GPU execution: quality recovered, cost rejected

**Decision: change the appearance representation. Retire tile-local compute execution;
do not promote the GPU-global-sort alternative.** Keep the original frozen, CPU-sorted
AA2 renderer as the diagnostic control. This block follows the rejected two-slab
representation and uses exactly two targeted optimizations, zero training steps and
zero new shaded references. G1–G7 are unchanged and unmet.

The initial prototype assigns an 8×8 pixel tile to each 64-lane workgroup. It projects
the frozen primitive stream, culls support bounds, compacts a 512-entry local list,
sorts by float32 centre depth and descending primitive ID, and cooperatively evaluates
four ordered subpixel contributions. Pixel colors stay in registers until their
four-sample resolve. Source wood uses the existing 128×128 color/depth raster targets;
these remain resident and counted. This tests ordered GPU execution, not a claim
that all supersampled storage has been removed.

The tile's declared workgroup-local storage is 10,244 bytes; 64 groups are dispatched.
Count above 512 produces invalid negative coverage, making the result ineligible:
there is no hidden dropped-leaf path or loaded conventional fallback. None of the
eight captures overflows, for either fitted or calibrated parameters. This remains
a bounded diagnostic, not a general production fallback.

Both methods receive the same source support, materials, wood, camera, lights and
sample positions. The conventionally calibrated source primitives retain their
previously selected parameters. The fitted and calibrated hashes remain unchanged
from the earlier sections. No new fitting advantage is introduced by the runtime.

### Quality recovery and two consequential follow-ups

| Path | Mean PSNR dB | Mean FLIP | Mean LPIPS | Cases matching its prior ordered control | BC quality gates passed |
|---|---:|---:|---:|---:|---:|
| Frozen fit, tile-local | 40.120118 | 0.012470 | 0.001883 | 24/24 | 22/24 |
| Calibrated, tile-local | 39.735748 | 0.013968 | 0.002108 | 24/24 | 22/24 |
| Follow-up 1: projection cache | 40.120118 | 0.012470 | 0.001883 | 24/24 | 22/24 |
| Follow-up 2: GPU sort + raster | 40.127682 | 0.012348 | 0.001889 | 24/24 | 22/24 |
| Calibrated, GPU sort + raster | 39.739577 | 0.013921 | 0.002116 | 24/24 | 22/24 |

The original fitted AA2 control is 40.127682 dB / 0.012348 FLIP / 0.001889 LPIPS;
the calibrated control is 39.739577 dB / 0.013921 FLIP / 0.002116 LPIPS.
Tile integration uses f32 accumulation instead of rounding every blend through an
RGBA16F attachment, so its pixels are close rather than identical. Every shaded
case stays within the existing relative quality tolerances against its corresponding
ordered control. The fitted tile path's coverage MAE is 0.008820 versus 0.008832;
silhouette precision/recall are identical and worst boundary p99 remains 1.414 pixels.
The same two BC-relative sky-only cases still fail. This is no G4 quality completion.

1. **Project/shade once:** the first measurement's GPU p95 is 1.586 ms. Test whether
   repeating projection in every tile causes that regression by caching all 1,897
   projected/shaded records once per frame. The complete 121,408-byte cache and its
   dispatch are counted. Quality is retained, but GPU p95 remains 1.297 ms, saturated
   p95 0.537 ms, and total requested GPU allocations rise to 836,064 bytes.
2. **GPU sorting plus hardware raster:** replace tile integration with one global
   2,048-entry GPU sort followed by the established wood/leaf raster and four-sample
   resolve. The source count is asserted at most 2,048; sort indices are stored and
   counted. Its single workgroup declares 16,384 bytes of local storage. It reproduces
   all eight fitted and all eight calibrated images **byte for byte**, yet costs more
   GPU work in the alternating comparison below.

The GPU sort uses a standard bitonic network; see [Batcher (1968)](https://doi.org/10.1145/1468075.1468121).
This Wrela implementation and its rendering integration are our own, with no imported
implementation or claimed novelty. No third optimization is taken.

### Complete runtime, including the misleading first paced result

Same Ryzen 5 5500U / hardware AMD RADV RENOIR, Mesa 26.1.6, Chrome 154.0.8037.57,
Wrela 0.9.0 / compiler de9b81e52a7747870dfbd5223cb8aeb41e584d23.
Each initial run has 240 frames after 60 warmups, one worker, 64×64 output.
All builds, scoring and GPU measurements run sequentially on Pop!_OS.

| Path | Paced GPU p95 / p99 ms | CPU call p99 ms | Saturated GPU p95 ms | CPU start to completion p95 / p99 ms |
|---|---:|---:|---:|---:|
| Frozen fit, tile-local | 1.586317 / 1.655851 | 0.573050 | 0.71412 | 4.340356 / 4.500298 |
| Calibrated, tile-local | 1.587522 / 1.643885 | 0.510000 | 0.72608 | 4.340332 / 4.531252 |
| Follow-up 1: projection cache | 1.296984 / 1.430936 | 0.500250 | 0.53696 | 6.070813 / 6.339993 |
| Follow-up 2: GPU sort + raster | 0.224052 / 0.260471 | 0.444150 | 0.22880 | 4.401636 / 4.547219 |
| Calibrated, GPU sort + raster | 0.915528 / 0.916725 | 0.533050 | 0.22452 | 4.385754 / 4.539114 |

These GPU spans include decoding/projection, sorting, visibility, source wood, ordered
leaf composition, resolve and final display composite. CPU calls include view/light
updates and uploads/recording; no new path does a CPU global sort or index upload.
Initialization is outside steady-state timing, with peak/resident allocation counted.
Completion includes every frame submission, plus browser scheduling and completion
notification latency. It is not input-to-photon time.

The first fitted GPU-sort run misleadingly suggests a paced improvement over the old
CPU-sort p95 of 0.314113 ms: it reports 0.224052 ms. However, saturated performance
worsens, and the calibrated GPU-sort run reports 0.915528 ms despite the same algorithm.
This contradiction triggered a proportional verification run, not a new optimization:
three alternating pairs against the archived CPU-sort build, in B,A,A,B,B,A order.

| Alternating comparison | Original CPU sort | GPU sort + raster |
|---|---:|---:|
| Median of saturated GPU medians | 0.078240 ms | 0.214040 ms |
| Saturated GPU p95, three runs | 0.08504 / 0.08100 / 0.09324 ms | 0.23452 / 0.23552 / 0.23092 ms |
| Median CPU frame calls | 0.680000 ms | 0.380000 ms |
| Final pair paced GPU p95 / p99 | 0.313842 / 0.315178 ms | 0.914434 / 0.916959 ms |
| Final pair CPU p95 / p99 | 0.950750 / 1.043300 ms | 0.450250 / 0.538550 ms |

The established paired estimator reports **43.38% lower CPU time but 173.57% higher
saturated GPU time**. Every pair has the same direction. GPU cost is 2.74×, not a win.

Read-only 100 ms power snapshots retain the unchanged automatic policy, reported
shader clocks from 400–1,800 MHz and temperatures 35–43°C. These are not synchronized
per-frame clock measurements, so they do not prove a specific spike's cause.
No clock or power setting was changed. Preserve the initially favorable paced result
alongside the contradictory repeats; do not cherry-pick it.

### Resident storage and scratch

| Path | Engine appearance/scratch bytes | All requested live / peak GPU bytes | DRM VRAM / GTT MiB | Browser group PSS MiB |
|---|---:|---:|---:|---:|
| Frozen fit, tile-local | 305,056 | 714,656 / 731,040 | 16.555 / 9.289 | 449.797 |
| Calibrated, tile-local | 305,056 | 714,656 / 731,040 | 16.555 / 9.195 | 449.311 |
| Follow-up 1: projection cache | 426,464 | 836,064 / 852,448 | 16.566 / 9.398 | 455.268 |
| Follow-up 2: GPU sort + raster | 312,440 | 722,040 / 738,424 | 16.570 / 9.383 | 457.114 |
| Calibrated, GPU sort + raster | 312,440 | 722,040 / 738,424 | 16.824 / 9.383 | 456.738 |

The engine column excludes only the identical 65,536-byte output. Complete requested
allocations include it, wood, sort indices, projected cache, all image/depth targets,
fallback/dummy data, retained upload/uniform rings and screen storage. The initial tile
path saves only 1.0% against the old fitted AA2 allocation of 721,852 bytes; caching
increases storage by 15.8%. The GPU-sort variant has essentially the same storage.
All remain far from 16× against the strongest same-source conventional control.

GPU workgroup-local arrays are separate from persistent WebGPU buffer/texture bytes.
Their declared sizes above affect occupancy; driver alignment and concurrent residency
are not inferred by multiplying the size by every dispatched group. Physical DRM
residency and browser PSS snapshots capture exposed retained driver/client allocations,
including retained spill storage and caches where reported. Transient physical peaks and
exact per-asset attribution are not established by these snapshots. On this UMA
device these columns overlap: never sum PSS, VRAM and GTT.

The restored default adds 72 dummy GPU bytes relative to the parent checkpoint.
Together with the already documented 116-byte helper difference, that is 188 bytes
above the original measured fitted AA2 build. The GPU-sort table counts the actual
722,040 bytes, without subtracting these differences.

### Next representation bet and preservation

The useful result is that ordered subpixel visibility restores the shaded image.
The failed bet is that moving this explicit 1,897-primitive stream's sorting and
compositing to compute delivers the required joint quality/runtime/storage gain.
No further optimization of this execution path is justified by these results.

Next test a **small source-conditioned representation of joint visibility and shaded
response**, replacing the explicit primitive stream. Keep source support and authored
wood, and model the rendered contribution rather than independent averaged material
moments. A deterministic low-rank/basis fit must receive the identical source-derived
features, sample positions and supervision. The frozen ordered fitted and calibrated
renderers remain controls; the completed shaded references still judge actual quality.
This is an architecture hypothesis, not a learned rendering result. No new model was
trained in this block.

Exact source/payload, compiled builds, all five variants, quality/tails, complete runtime,
DRM/PSS, allocation descriptors, three paired runs and power records are preserved in
build/neural-evidence/g4-tile-ordered-v1 on Pop!_OS. The paired profiler writes into the
compared build's results directory: its new baseline outputs were preserved separately,
and the four overwritten original result files were restored from existing copies to
their sealed-manifest hashes. No original evidence was lost; replay now copies the
baseline first.

The runtime preserves rejected diagnostics as modes 7–9 and restores mode 2 as default.
Focused formatting/type checks pass, and all eight restored default images are
byte-identical to the original measured AA2. No full-suite or PR-readiness claim.
Manifest SHA-256: `19998e152994f78f42b6e63c1b2e23668b66485cad005e5077c303677d6e604e`.


## Joint shaded light field: retire image/camera factorization

**Decision: retire this tested representation for G4.** A compact field of image position
and camera direction loses too much geometry and costs too much to decode. The previous
frozen ordered model remains the default. G1–G7 are unchanged and unmet.

This follow-on block tested an actual AMD renderer, not material-moment accuracy. It
fits complete premultiplied shaded RGB and coverage, including authored wood, into
six bilinear factor planes over image x/y and camera gx/gy. Complementary plane pairs
are multiplied. Signed 4-bit planes feed either a two-layer 32-wide neural decoder or
a conventional tensor/basis decoder with no hidden network. Both get the same coordinates,
ten-term lighting basis, source captures, quantization method, rendered loss and
2,048-step budget. Conventional C8 has twice the feature channels of C4; it avoids
claiming an advantage over only a deliberately small control. Our implementation is
informed by the general plane-factorization idea in
[Cao and Johnson, HexPlane, CVPR 2023](https://openaccess.thecvf.com/content/CVPR2023/html/Cao_HexPlane_A_Fast_Representation_for_Dynamic_Scenes_CVPR_2023_paper.html);
our dimensions are image/camera coordinates, not their space-time volume.

The old AA64 shaded references are unchanged. The existing source-card/wood renderer
provided AA8 **training targets**, not new reference truth: 128 training poses and 16
validation poses, four lights each. The two public reference views are excluded from
training. All three fits use the same shaded display loss across the three established
backgrounds, foreground weighting and coverage loss; no further material-moment training
was performed. Fixed endpoints are retained without selecting a checkpoint on the public
references. The previous BC, calibrated optical and source-card controls still have the
source geometry/material/visibility information. The new conventional tensor fits receive
exactly the same teacher data as the neural fit.

| Fit | Mean PSNR dB ↑ | Mean FLIP ↓ | Mean LPIPS ↓ | Complete GPU p95/p99 ms | Asset/decoder bytes, output excluded | All live/peak requested GPU bytes |
|---|---:|---:|---:|---:|---:|---:|
| neural-c4 | 20.168 | 0.20498 | 0.38626 | 1.773/1.803 | 21,000 | 430,600/446,984 |
| tensor-c4 | 21.268 | 0.22610 | 0.42865 | 1.234/1.241 | 17,952 | 427,552/443,936 |
| tensor-c8 | 21.384 | 0.24533 | 0.41750 | 1.434/1.471 | 35,592 | 445,192/461,576 |
| neural-c4-a8 | 23.619 | 0.15422 | 0.22198 | 2.060/2.160 | 16,520 | 426,120/442,504 |
| tensor-c4-a8 | 23.082 | 0.17706 | 0.24028 | 1.228/1.242 | 13,472 | 423,072/439,456 |
| tensor-c8-a8 | 23.354 | 0.15845 | 0.21425 | 1.263/1.374 | 26,632 | 436,232/452,616 |

The common runtime allocation overhead above the asset is 409,600 bytes: output,
upload/uniform rings and screen target. This is measured live allocation, not an estimate
from the model file. Conventional half-weight buffers include two alignment bytes.
Neither path retains source geometry, an exact wood fallback, a hidden view atlas or a
sort cache: wood and visibility are part of the approximation, and their lost detail
counts against quality. GPU timing includes every plane lookup, unpack, decoder,
coverage evaluation and final composite; CPU timing includes view/light updates and
command recording. There is no runtime sorting. Build-time fitting and source capture
are recorded separately.

| Fit | CPU frame p99 ms | Saturated GPU p95 ms | CPU start through final GPU completion p95/p99 ms |
|---|---:|---:|---:|
| neural-c4 | 0.451 | 0.864 | 4.836/5.023 |
| tensor-c4 | 0.431 | 0.291 | 4.385/4.682 |
| tensor-c8 | 0.461 | 0.480 | 4.330/4.431 |
| neural-c4-a8 | 0.524 | 0.865 | 6.264/6.585 |
| tensor-c4-a8 | 0.470 | 0.278 | 4.295/4.402 |
| tensor-c8-a8 | 0.355 | 0.485 | 4.225/4.406 |

Completion timings cover every submission belonging to the frame and include browser/driver
notification latency; they are not input-to-photon. Saturated timing is reported separately
from paced timing because power state affects this AMD iGPU. No power policy was changed.

For context, the preserved ordered model scored **40.128 dB / 0.01235 FLIP / 0.00189
LPIPS** with prior measured GPU p95/p99 **0.314/0.316 ms**, and the calibrated conventional
optical control scored **39.740 dB / 0.01392 / 0.00212**. These are reused measurements on
the same AMD machine, not a newly paired timing experiment. The quality failure is large
enough that another timing audit cannot change this decision. Every new fit fails all
24 joint numeric quality cases against the existing BC control, which itself averages
30.685 dB.

### One targeted follow-up, then stop

The initial 16×16 angular lattice was underconstrained: the even-column training poses
touched only 128 of its 256 camera-plane coefficients. The first follow-up coarsened
angular planes to 8×8, making all 64 coefficients reachable from training. Image
resolution, channels, decoder, quantization, loss, data and step budget stayed fixed.
Both neural and conventional fits received this change.

The neural public mean rose from 20.168 to **23.619 dB**. Its separate 16-pose validation
mean was 23.874 dB against the AA8 teacher; conventional C8 reached 24.065 dB on that
same validation set. Fixing coefficient support helps, but does not recover geometry:
the best neural field has mean coverage error **0.11075**, worst symmetric
boundary p99 **8.64 pixels**, and worst thresholded silhouette precision/recall
**71.19%/83.56%**. The previous ordered result was 1.41 pixels with 97.80%/98.54%.
These static silhouette metrics do not establish the motion requirements of G3/G4.

Against conventional C8 at the coarser angular resolution, the neural field has +0.265 dB,
2.67% lower FLIP and **3.61% worse LPIPS** on public means. It meets all three relative
numeric quality limits in only **9/24** cases. Its payload is 1.61× smaller, but complete
live GPU allocations fall only **2.32%**, while measured GPU p95 is **63.0% higher**.
This is not a useful combined quality/memory/runtime advantage, let alone G4's 16×
resident-memory requirement with ≤10% runtime regression. The second allowed optimization
slot is unused. This rejects the tested compact implementation and fit budget; it is
not a proof that every image/camera factorization or longer fit must fail. A larger or
more expensive representation would still have to meet the same combined G4 gates.

![Unretouched source, ordered, neural and conventional outputs; fixed display transform, 4× nearest-pixel enlargement](figures/neural-joint-field.png)

### Resident memory, scope and verification

All six runs retain full resource create/destroy logs, live/peak requested GPU bytes,
CPU-to-last-GPU-completion timings, and actual Linux DRM resident-memory/browser PSS
snapshots. Those snapshots expose approximately 16.5–16.8 MiB of resident VRAM, 9.3–9.6
MiB of resident GTT and 442–446 MiB of browser-group PSS. They are separate, potentially
overlapping UMA views and **must not be summed**. They include reported retained
driver/client allocations and caches; they do not establish transient physical peaks
or exact per-asset driver attribution. The two final compared builds each report 30,474,240 used WASM bytes and a 1 GiB virtual
reservation; the reservation is not a physical residency measurement. There is no declared
workgroup scratch, although
thread-private arrays may spill. This uncertainty cannot rescue the failed quality gate.

This remains one static tree, one 64-pixel footprint and four known controlled
illuminations. There is no unseen-light, wind, full transport, 8–512-pixel or asset-family
claim. A neural output is one fitted integrated pixel, not a point sample proved to
integrate correctly at other footprints. Capturing additional views was necessary to
test generalization of the new image/camera representation; no new convergence or
reference system was built.

Packed-payload GPU decoding agrees with independent CPU evaluation in all **48** new
view/light cases, maximum absolute RGBA error **2.98e-7** (declared tolerance 5e-5).
Changed tooling and the new package pass format/type checks. Restoring the frozen
default reproduces all eight original outputs byte for byte. Frozen primitive
SHA-256 remains dc329e4b4c4eaa8ad0bbbb93eeea90cb2a94f4a0f01b284487f344d4ee2ff71c.
No full-suite or PR-readiness claim is made.

**Next architecture-level bet:** compile authored clumps and geometry into local
coordinates, explicit support and ordering; retain exact wood, then learn only local
shaded visibility/residual response. Give conventional local fits the identical source
and sampling knowledge. This uses Wrela's source to constrain the representation instead
of asking a global image field to relearn projection and silhouette. It is a proposed
experiment, not a demonstrated advantage.

All six failed fits, full checkpoints, quantized payloads, training logs, 576 teacher
captures, independent decoder checks, raw reference-comparison outputs, builds, timings,
allocation records, physical-memory snapshots and the initial syntax failure are kept
under build/neural-evidence/g4-joint-field-v1 on Pop!_OS. The runnable rejected fixture
is studio/appearance/joint; appearance joint-runtime [FIRST COUNT] captures it.
The first complete comparison and the follow-up decision finished within the block's
two-hour deadline. Manifest SHA-256: `d412e61a93664caa3724d00759759a626d93d4d90027affd462af81410ef40fa`.


## Source anchors: preserve order, change local appearance

**Decision: change the representation; retain ordered rendering and exact wood.** Do not
promote any of these 88-element models. A fixed quota of two elements per authored clump
loses to ordinary spatial grouping, both before and after equal-budget shaded-image fitting.
No neural-specific advantage or G4 pass is demonstrated.

The previous image/camera field lost projection and silhouette. This block instead keeps
the established 5D joint position/pattern optical elements, production shading before
blending, per-view depth sorting, four-point spatial integration and exact authored wood.
It reduces foliage from 1,897 elements to 88. The two initial conventional controls use
the identical 26,560 source cards, cutout-pattern moments, material information and
covariance/mass aggregation. One reproduces source ownership across the 44 authored clumps
and places two elements in each; the other spatially groups all cards into 88 elements.
Both get eight fixed grouping iterations. The source ownership arithmetic is checked
against every card's normalized authored shell radius. No image labels select these
initial controls.

These are explicit optical representations, not MLP decoders. The second follow-up fits
their numbers by differentiating shaded rendering; it does not turn them into a runtime
neural network. Source definitions are in [plant.wrela](../engine/plant.wrela) and
[great-tree/tree.wrela](../content/great-tree/tree.wrela). The source-only aggregation and
rendering reuse the previously verified optical primitive machinery.

| Variant | Mean PSNR dB ↑ | Mean FLIP ↓ | Mean LPIPS ↓ | Complete GPU p95/p99 ms | CPU start to final GPU completion p95/p99 ms | Numeric quality passes vs BC |
|---|---:|---:|---:|---:|---:|---:|
| source-88 | 28.426 | 0.08018 | 0.11304 | 0.211/0.212 | 4.510/4.659 | 1/24 |
| global-88 | 29.966 | 0.06103 | 0.06585 | 0.205/0.206 | 4.210/4.427 | 8/24 |
| source-88-square | 28.649 | 0.07861 | 0.08598 | 0.211/0.211 | 1.952/3.595 | 3/24 |
| global-88-square | 30.161 | 0.06027 | 0.04453 | 0.205/0.205 | 4.176/4.448 | 11/24 |
| source-88-square-fit | 31.895 | 0.04479 | 0.05424 | 0.212/0.213 | 4.380/4.613 | 8/24 |
| global-88-square-fit | 32.424 | 0.04013 | 0.04164 | 0.210/0.211 | 4.170/4.268 | 13/24 |

All six use 4,928 bytes of optical parameters, 1,408 serialized wood bytes and 32 common
metadata bytes: **6,368 packed bytes**, 16.91× smaller than the frozen best's 107,672 bytes.
That is not a resident-memory or matched-quality win. All six actually allocate
**613,500 live / 629,884 peak requested GPU bytes**. Excluding the identical 65,536-byte
output, engine appearance and scratch total 203,900 bytes: 196,608 bytes are the AA2
colour/depth attachments; the remainder includes optical data, wood, sorting indices and
inactive helper resources. Host rings, uniform storage, screen target and output account
for the remaining common 409,600 bytes. Full resource lifetime logs and CPU/WASM memory
are retained; no hidden source-card cache or fallback atlas supplies the compact output.

The preserved best measured 40.128 dB, 0.01235 FLIP and 0.00189 LPIPS, with prior GPU
p95/p99 0.314/0.316ms and 721,852 live requested GPU bytes. These same-machine results
are reused, not newly paired timing estimates. The compact spatial fit is faster and uses
about 15% less total live GPU allocation, but is **7.70 dB worse**. It also fails 11 of 24
numeric quality comparisons against BC, whose mean PSNR is 30.685dB. Smaller/faster at
lower quality does not meet G4.

### Exactly two targeted experiments

1. **Sharper optical profile, no training.** Replace the Gaussian optical density
   exp(−q/2) with a normalized exp(−q²/(4π)), retaining every source-derived element,
   its nominal covariance, integrated mass, bounds, material and draw order.
   The integral over q is π rather than 2, giving the amplitude factor 2/π;
   the existing Gaussian's finite-support normalization is also included.
   At the retained q≤18 cutoff, integrated mass agrees to about 2e-9 relative, while
   the sharper kernel's finite-support covariance is 0.1112% larger. The initial
   plan's “same covariance” means nominal untruncated covariance; the numerical
   clarification is preserved in the evidence. Spatial grouping improves by only
   0.195dB, although LPIPS falls 32.4%. Both controls receive the same profile.
2. **Direct shaded-image fitting at the same capacity.** Fit both initializations for
   2,048 steps, with the same seed, sampling sequence, constraints, optimizer rates,
   loss and fixed endpoint. Each step samples one of the existing 128 AA8 teacher
   poses, one of four lights and 1,024 pixels, rendered at the same four spatial
   points as the AMD path. The loss is display RGB over the three established
   backgrounds plus coverage, with foreground weighting. There is **no material-moment
   objective**. The public AA64 test views are excluded from training. The frozen
   1,897-element model and reference captures are unchanged.

The direct fit raises spatial grouping from 30.161 to **32.424dB**, and source grouping
from 28.649 to **31.895dB**. On the separate 16-pose AA8 teacher validation set, the final
means are 31.445 and 31.028dB respectively. The endpoint is not chosen using public
reference scores. Both optimization slots are now exhausted.

### What still fails

The best compact fit has coverage MAE **0.02360**, versus **0.00883** for the frozen best.
Worst symmetric silhouette boundary p99 is **2.86 pixels**, versus **1.41 pixels**.
Inside mostly opaque reference pixels, conditional foreground display-colour MSE is
0.001256 versus 0.000170 for the frozen model—about 7.4× higher. This interior diagnostic
does not mask or replace any pixel in the full-image scores. Both visible shape and
local shaded response need more expressive models; another global opacity adjustment
cannot address the colour error.

![Reference, frozen ordered model, fitted source groups and fitted spatial groups; same display transform and 4× nearest enlargement](figures/neural-source-anchors.png)

**Next bet:** retain spatial anchors, explicit ordering and exact wood, but replace one
constant material response per broad element with a shared local opacity/radiance decoder
conditioned on local geometry and source-field features. Give conventional local
functions the same features. Use authored clumps as conditioning information rather than
forcing every clump to receive the same capacity. That proposal remains unproven.

### Verification, residency and provenance

The initial strict CPU/GPU maximum-RGBA discrepancy limit of 0.002 failed at a wood edge;
the failed check and log are preserved. The CPU training approximation uses the existing
triangle query and f32 optical accumulation, whereas the GPU uses raster edge rules and
f16 target blending. The discrepancy is quantified, not silently treated as byte parity:
the initial maximum RGBA difference was 0.02357, but displayed-image discrepancy was
at most 0.00809% of the actual GPU reference MSE. After fitting, the corresponding maximum
fraction is 0.01375%. Both are below the declared 1% surrogate-error limit; final claims
use actual AMD output. The new differentiable shading agrees with the independent
material equation, and eight parameter groups pass smooth finite-difference checks.
No additional reference or capture system was built to eliminate a difference too small
to affect this decision.

The complete runtime measurements include decoding, projection, sorting/index uploads,
visibility, wood, integration, view/light updates and final compositing. Completion timing
also includes browser/driver notification and is not input-to-photon. Paced/saturated
timings, CPU tails and all allocations are preserved separately. Actual DRM resident-memory
and browser PSS snapshots are retained; they include exposed retained driver/client
allocations and caches, but not proven transient physical peaks or per-asset attribution.
Overlapping UMA counters must not be added.

For the final spatial fit specifically, CPU frame p99 is 0.544 ms and saturated GPU p95
is 0.0549 ms. Its actual client snapshots report 17,305,600 resident VRAM bytes,
9,740,288 resident GTT bytes and 476,049,408 browser-group PSS bytes, separately.
WASM reports 30,474,240 used bytes and a 1 GiB virtual reservation; virtual reservation
is not physical residency. These figures retain host/driver overhead rather than
equating the 6,368-byte payload with memory consumed by the complete renderer.


This is still one static tree, four known controlled lights and a 64-pixel footprint.
No wind, unseen-light, transport, 8–512-pixel, new-individual or full-family claim is made.
The format/type checks pass and the restored default reproduces all eight original
outputs byte for byte. G1–G7 remain unchanged and unmet; no full-suite/PR-readiness claim
is made.

Evidence lives at build/neural-evidence/g4-source-anchors-v1 on Pop!_OS: all six variants,
both fitted checkpoints, grouping ownership, training curves, teacher dependencies,
wood training caches, failed and successful checks, source builds, raw GPU pixels,
metrics, allocation/completion logs, physical-residency snapshots and the decision.
First measurements and both follow-ups finished within the two-hour block.
Manifest SHA-256: `bd5636383b51a19ff6f258290ad5608294e9112b58b5518bbb7880c9a36b01dd`.


## Local response decoder: retired

**Decision: retire the tested 88-element representation with shared quadratic local
opacity/radiance corrections for G4.** Keep the frozen 1,897-element renderer as the quality
control. A neural decoder improves compact-base PSNR by only 0.285 dB, reduces FLIP 2.21%,
worsens LPIPS 1.07%, and leaves silhouette errors essentially unchanged. Its paced GPU
p95 is 0.715 ms versus 0.210 ms for the compact base. These are complete-render measurements,
not decoder-only timing. No new model is promoted.

This block started 2026-10-27 at 18:48:56 UTC. The first three complete comparisons finished
at 19:06:03; the decision was recorded at 19:11:53, within the two-hour limit. One targeted
follow-up was used. A second execution optimization would not resolve the measured
visibility/quality gap, so its budget was left unused.

### Frozen inputs and equally informed controls

The preceding [source-anchor experiment](#source-anchors-preserve-order-change-local-appearance)
identified missing shaded color detail in its strongest compact 88-element model.
Freeze all 88 positions, projected covariance parameters, masses, base colors, normals
and crown depths. Preserve exact authored wood, CPU depth sorting and 2x2 pixel sampling.

All three heads receive the same 17 context values: center, unit source normal, crown
depth, view direction, projected ellipse scales/correlation and four trainable half
coordinates per element. Color also receives the same known light direction and sun
presence; opacity is light independent. Each predicts six coefficients for a local
quadratic basis, separately correcting log optical density and three log-radiance
channels. Corrections are bounded. Zero heads reproduce the compact GPU images exactly.

- Neural: width 12 shared stem, opacity head and width 12 color branch; 732 half weights.
- Conventional affine: 18 context terms and five light terms; 1,728 half weights.
- Conventional quadratic: 171 context terms and the same light basis; 16,416 half weights.

Each also stores the same 704-byte per-element coordinate array and 4,928-byte frozen
geometry/base array. Packed totals including common parameters and serialized wood are
8,536/10,528/39,904 bytes. None has privileged source features or targets. These bounded
controls supplement the existing engine BC atlas, compact spatial model and frozen
quality model; they are not a claim to exhaust all conventional methods.

Fit each head for exactly 2,048 steps, 256 sampled pixels/step and four spatial samples,
with identical sampled view/light/pixel sequences and the same shaded-display plus
coverage loss. Use the existing 128 AA8 training poses and 16 separate validation poses;
exclude both public test views. Select the fixed endpoint. No material-moment loss,
geometry update, new teacher capture or reference-infrastructure expansion was used.
Source derivation remains [engine/plant.wrela](../engine/plant.wrela), the authored
[great-tree fields](../content/great-tree), and the completed references documented
[above](#reused-shaded-references-and-proportional-verification).

### Rendered result and full execution cost

Means cover the same 24 shaded cases: two public views, four controlled lights and three
backgrounds against the completed AA64 source. Actual AMD GPU images decide quality.

| Model | PSNR dB | FLIP | LPIPS | Paced GPU p95/p99 ms | All live requested GPU bytes |
| --- | ---: | ---: | ---: | ---: | ---: |
| BC atlas control (reused) | 30.685 | 0.06237 | 0.03772 | 0.104/0.105 | 4,341,876 |
| Frozen 1,897 elements (reused) | 40.128 | 0.01235 | 0.00189 | 0.314/0.316 | 721,852 |
| Compact 88 base (reused) | 32.424 | 0.04013 | 0.04164 | 0.210/0.211 | 613,500 |
| neural12 | 32.708 | 0.03924 | 0.04209 | 0.715/0.716 | 615,668 |
| linear | 32.796 | 0.03949 | 0.04217 | 1.343/1.405 | 617,660 |
| quadratic | 33.009 | 0.03804 | 0.04097 | 4.650/4.708 | 647,036 |

The neural head passes only 13/24 numeric quality cases against the BC atlas, unchanged
from the compact base. The affine control reaches 32.796 dB; the quadratic control reaches
33.009 dB and beats the neural head on all three mean metrics. Both still fail the required
quality frontier. Neural remains 7.419 dB below the frozen model, with 3.18x its FLIP and
22.28x its LPIPS. There is no useful neural rendering advantage demonstrated here.

Neural mean coverage error is 0.023997 and worst boundary p99 is 2.864 pixels, compared with
0.023601/2.863 for the compact base and 0.008832/1.414 for the frozen model. High-frequency
coverage, holes and shading remain visibly smoothed. A low-order local correction
does not recover the missing visibility structure in this tested representation.

![Source, frozen quality control, compact base and both local decoders](figures/neural-local-response.png)

The image shows two shaded cases and coverage, enlarged without interpolation.
The runtime includes view/light updates, CPU sorting, index uploads, exact wood,
decoder execution, visibility, integration and final compositing. Profiles use 240
measured frames after 60 warmup,64x64, one Chrome worker. AMD RADV RENOIR/1002:164c,
Ryzen 5 5500U, Mesa 26.1.6, Chrome 154.0.8037.57; Wrela 0.9.0 compiler
de9b81e52a7747870dfbd5223cb8aeb41e584d23. All work ran sequentially on Pop!_OS.

### One execution follow-up, no promotion

Evaluate the unchanged decoder once per primitive in a compute pass, store six float4
coefficients, and read them in rasterization. This adds an 8,448-byte cache replacing an
existing 4-byte inactive buffer, plus the measured dispatch/update costs. Apply this
identically to all three heads; no retraining. It produced no saturated GPU improvement.
Paced results also do not establish a useful win; do not infer small speed ratios across
these unpaired runs. Prior AMD paired experiments already documented clock sensitivity.

| Head | Original saturated GPU p95 ms | Cached saturated GPU p95 ms | Cached paced GPU p95/p99 ms | Cached all live requested GPU bytes |
| --- | ---: | ---: | ---: | ---: |
| neural12 | 0.17152 | 0.18480 | 0.772/0.773 | 624,112 |
| linear | 0.45756 | 0.46276 | 1.299/1.363 | 626,104 |
| quadratic | 3.78684 | 3.83204 | 5.098/5.231 | 655,480 |

All neural cached images are byte-identical. A strict all-model byte-parity check failed:
moving the conventional arithmetic between shader stages changes rounding, with a
maximum RGBA difference 0.000122071. Preserve that failure. Cached variants have their own
full quality scores; the largest display discrepancy is 7.62e-9 of reference-error MSE.
This is not an exact-equivalence claim.

| Variant | CPU frame p99 ms | CPU start to final GPU completion p95/p99 ms | Engine appearance plus scratch bytes | All requested GPU peak bytes |
| --- | ---: | ---: | ---: | ---: |
| neural12 | 0.516 | 4.470/4.584 | 206,068 | 632,052 |
| linear | 0.507 | 4.305/4.548 | 208,060 | 634,044 |
| quadratic | 0.500 | 7.712/8.021 | 237,436 | 663,420 |
| neural12-cache | 0.661 | 4.441/4.669 | 214,512 | 640,496 |
| linear-cache | 0.563 | 5.977/6.333 | 216,504 | 642,488 |
| quadratic-cache | 0.544 | 7.432/7.847 | 245,880 | 671,864 |

Completion includes browser/driver notification, not input-to-photon latency. Engine
appearance/scratch excludes the identical 65,536-byte output and host-managed rings;
all-requested columns include output, upload rings, uniform rings, screen allocations,
inactive/fallback resources and decoded caches. Initialization is included in peak
memory and excluded from steady-state runtime.

### Residency is the second architectural constraint

For neural, 206,068 engine appearance/scratch bytes include 196,608 bytes of AA2 color/depth
targets,7,096 model bytes,1,760 live wood bytes,352 sort-index bytes and 252 inactive bytes.
All live requested allocations total 615,668, with 632,052 peak. This is only 7.05x below the
BC control's 4,341,876 live bytes, before any quality matching requirement is satisfied.
The optimistic engine-only ratio is 15.27x; it omits differing host upload rings and is
not an acceptable complete G4 residency result.

Against this measured BC control, 196,608 bytes of supersampling targets alone nearly
consume an optimistic 16x engine-only allowance of 196,615.25 bytes. Wood, indices and the
decoder still need storage. This bound concerns this particular control and allocation
scheme, not all possible conventional representations.

Actual physical snapshots from each isolated browser run are preserved separately:

| Variant | DRM resident VRAM bytes | DRM resident GTT bytes | Browser process-group PSS bytes |
| --- | ---: | ---: | ---: |
| neural12 | 17,367,040 | 9,740,288 | 480,005,120 |
| linear | 27,254,784 | 9,838,592 | 480,647,168 |
| quadratic | 27,242,496 | 10,149,888 | 480,657,408 |
| neural12-cache | 17,641,472 | 9,838,592 | 484,811,776 |
| linear-cache | 27,246,592 | 9,854,976 | 485,233,664 |
| quadratic-cache | 27,525,120 | 10,133,504 | 486,026,240 |

Requested bytes are not physical residency. DRM/PSS include retained driver/client
allocations, compiled code and caches where exposed; they do not prove transient
physical peaks or per-asset attribution. These UMA views overlap and must not be added.
No physical 16x saving is established.

### Next bet, verification and preserved evidence

Change visibility representation: source-derived sparse patches with direct filtering
over the output-pixel footprint and ordered wood/leaf visibility. Preserve fine coverage
explicitly, then model lower-frequency radiance separately. Start with conventional
patch compression using the same field/source information; add a learned model only if
it improves the joint quality, runtime and residency frontier. This is a proposed next
experiment, not a demonstrated rendering win.

Focused checks cover new decoder gradients, zero-head recovery and light-independent
opacity. The trained CPU surrogate's largest display discrepancy is 0.01415% of actual
GPU reference-error MSE; actual GPU outputs remain authoritative. Format/type checks
pass. Restoring KIND0/CACHEDfalse reproduces all eight frozen images and native resource
counts exactly. No full-suite or PR-readiness claim is made.

G1-G7 remain unchanged and unmet. Scope is still one static tree at 64 pixels with four
known analytic lights: no wind, unseen-light, new-individual, family, 8-512-pixel or full
transport claim. Evidence at build/neural-evidence/g4-local-response-v1 on Pop!_OS retains
all six measured variants, checkpoints, optimizer curves, raw images, complete timing,
allocation and physical-residency records, the failed parity check and the decision.
Replay archives include the Wrela sources and external fitting/evaluation scripts.
Manifest SHA-256: `ce4b84b4b62bd5d5a5f2b969d8bd62f265e1842fc79ecc37ad0bb816ba94e05b`.


## Fine visibility patches: change wood sampling

**Decision: change the wood/leaf visibility representation.** Retain the frozen fine
patches and their AA2 quality path. Direct per-patch filtering loses too much quality;
streaming all four complete visibility samples recovers quality but costs too much.
Neither is promoted. No new G4 rendering architecture win is established.

Two fixture corrections are retained: exact indexed wood drawing, already used in the
production engine, and bounded conventional-atlas uploads. They improve the comparison's
execution path; they are not a new neural advantage or an engine-wide speed claim.

This block began 2026-10-27 at 19:18:23 UTC. Initial complete measurements finished at
19:24:49; the decision was recorded at 19:42:52. Exactly two follow-ups were used.
No training, new references or changes to G1-G7.

### Frozen source knowledge and the consequential questions

Reuse all 1,897 fine optical patches of the frozen best model, and the equally informed
conventional source-calibrated patches. Positions, covariance, opacity and material
parameters stay unchanged. Both retain the authored wood and receive identical filtering
and execution changes. The 88-element neural decoder remains retired.

The first path samples each patch's opacity four times across an output pixel, then
blends the mean in depth order. It uses 64x64 color/depth targets instead of 128x128,
saving 147,456 target bytes. Its wood remains point sampled. This deliberately tests an
approximation: averaging each opacity before composing does not preserve every subpixel
visibility correlation.

Follow-up 1 renders all four jittered subpixel views, including wood and ordered leaves,
through the same 64x64 targets, accumulating linear RGBA in the existing output buffer.
Sort and upload once per frame. All four draws/passes and updates count. This preserves
visibility sampling but can differ slightly from the AA2 rasterizer's subpixel precision.

Follow-up 2 uses the existing [limb_corner/limb_point source geometry](../engine/plant.wrela)
in an indexed instanced wood draw. Each limb's 1,008 triangle indices address 225 shared
points; triangles, normals, curve samples and order are unchanged. The 4,032-byte index
buffer replaces a 4-byte inactive buffer. The production [wood renderer](../engine/wood.wrela)
already uses this approach; the diagnostic's expanded-vertex path was less efficient.
Both the frozen and conventional models receive the correction.

The BC atlas keeps every texel and shader equation. Initializing its buffers in 128 KiB
chunks reduces the observed retained upload ring from 1 MiB to 256 KiB, matching the
candidate's ring size. All eight BC images are byte-identical to the prior control.
The older fixture differs by 188 inactive bytes; report the ring change directly rather
than attributing that unrelated difference to the upload correction.

### Shaded quality, including silhouettes

The same completed AA64 source references, two public views, four analytic lights and
three backgrounds supply the 24 shaded cases. The table includes every measured variant.

| Variant | PSNR dB | FLIP | LPIPS | Mean coverage error | Worst boundary p99 px |
| --- | ---: | ---: | ---: | ---: | ---: |
| bc-chunk | 30.685 | 0.06237 | 0.03772 | 0.01678 | 2.038 |
| frozen-aa2-control | 40.128 | 0.01235 | 0.00189 | 0.00883 | 1.414 |
| frozen-aa2-indexed | 40.128 | 0.01235 | 0.00189 | 0.00883 | 1.414 |
| calibrated-aa2-indexed | 39.740 | 0.01392 | 0.00212 | 0.00935 | 1.414 |
| frozen-direct4 | 35.042 | 0.01789 | 0.01158 | 0.01351 | 1.414 |
| calibrated-direct4 | 34.858 | 0.01989 | 0.01191 | 0.01433 | 1.414 |
| frozen-direct4-indexed | 35.042 | 0.01789 | 0.01158 | 0.01351 | 1.414 |
| frozen-stream4 | 40.163 | 0.01234 | 0.00190 | 0.00882 | 1.414 |
| calibrated-stream4 | 39.774 | 0.01391 | 0.00213 | 0.00933 | 1.414 |
| frozen-stream4-indexed | 40.163 | 0.01234 | 0.00190 | 0.00882 | 1.414 |
| calibrated-stream4-indexed | 39.774 | 0.01391 | 0.00213 | 0.00933 | 1.414 |

Direct filtering reaches 35.042 dB versus 40.128 dB for frozen AA2. Streaming recovers
40.163 dB; the small difference is not a trained improvement. Both source-calibrated
controls show the same qualitative tradeoff. Each patch variant passes only 22/24
numeric quality cases against the BC atlas; no full G4 quality pass is established.

Using the existing source-wood sample masks on the neutral-gray background, 93.91% of
direct filtering's extra displayed MSE against frozen AA2 lies within one pixel of projected wood. That region covers
12.00% of the image. The mask includes foliage in front of and behind wood, so it does
not prove that every masked error comes from wood alone. It does identify joint wood/
leaf visibility as the next specific target. Remaining errors outside the mask still
count in every whole-image quality score.

![Completed reference, frozen fine patches, direct filtering and streamed visibility](figures/neural-footprint-visibility.png)

### Complete rendering cost and residency

Same Pop!_OS AMD RADV RENOIR/1002:164c, Ryzen 5 5500U, Mesa 26.1.6,
Chrome 154.0.8037.57; Wrela 0.9.0 compiler
de9b81e52a7747870dfbd5223cb8aeb41e584d23. One worker, 64x64 output, 60 warmup and
240 measured frames. Solo, sequential builds/captures/profiles/scoring.

GPU timing includes decoding, wood, visibility, integration and final compositing.
CPU timing includes view/light changes, sorting, uploads and command recording.
CPU-start-to-final-GPU completion includes browser/driver notification; it is not
input-to-photon latency. Initialization contributes to peak memory, not steady-state time.

| Variant | Paced GPU p95/p99 ms | CPU frame p99 ms | CPU to final GPU p95/p99 ms | Engine appearance/scratch bytes | All requested live/peak GPU bytes |
| --- | ---: | ---: | ---: | ---: | ---: |
| bc-chunk | 0.105/0.105 | 0.404 | 4.495/4.832 | 3,146,032 | 3,555,632/6,701,376 |
| frozen-aa2-control | 0.317/0.318 | 0.983 | 5.306/5.610 | 312,440 | 722,040/738,424 |
| frozen-aa2-indexed | 0.270/0.272 | 1.053 | 4.936/5.191 | 316,468 | 726,068/742,452 |
| calibrated-aa2-indexed | 0.272/0.274 | 1.001 | 4.231/4.326 | 316,468 | 726,068/742,452 |
| frozen-direct4 | 0.309/0.315 | 0.970 | 4.771/4.851 | 164,984 | 574,584/590,968 |
| calibrated-direct4 | 0.309/0.312 | 1.037 | 4.655/4.883 | 164,984 | 574,584/590,968 |
| frozen-direct4-indexed | 0.265/0.269 | 1.016 | 4.800/5.000 | 169,012 | 578,612/594,996 |
| frozen-stream4 | 1.125/1.128 | 1.021 | 4.861/5.089 | 164,984 | 574,584/590,968 |
| calibrated-stream4 | 1.121/1.123 | 1.043 | 5.051/5.186 | 164,984 | 574,584/590,968 |
| frozen-stream4-indexed | 0.948/0.950 | 1.075 | 4.921/5.187 | 169,012 | 578,612/594,996 |
| calibrated-stream4-indexed | 0.950/0.951 | 1.047 | 4.766/4.995 | 169,012 | 578,612/594,996 |

The indexed stream uses 169,012 engine appearance/scratch bytes, versus 316,468 for
indexed AA2. Yet its complete paced GPU p95 is 0.948 ms versus 0.270 ms; BC is 0.105 ms.
The engine-only ratio against BC is 18.61x, but it excludes the output and host-managed
rings/screen. Including every requested allocation reduces the ratio to 6.15x.
Even with identical-sized common rings, exclusions must remain explicit; this is not
proof of the complete G4 residency target.

Three paired runs of indexed versus expanded wood drawing on the unchanged AA2 path
use order B,A,A,B,B,A and read-only power/temperature sampling. Median saturated full-GPU
time falls from 0.07840 to 0.06836 ms: -12.76%, with individual pair changes from -13.01%
to -12.64%. CPU timing is inconclusive. Standalone paced p95/p99 falls from
0.317/0.318 to 0.270/0.272 ms, at 4,028 extra live index bytes. Retain this fixture
correction, without claiming a CPU or whole-engine throughput improvement. The paired
baseline is a private copy; archived measurements were not overwritten.

Actual physical residency snapshots are separate from requested buffer/texture sizes:

| Variant | DRM resident VRAM bytes | DRM resident GTT bytes | Browser-group PSS bytes |
| --- | ---: | ---: | ---: |
| bc-chunk | 25,747,456 | 9,756,672 | 486,687,744 |
| frozen-aa2-control | 17,301,504 | 9,854,976 | 477,434,880 |
| frozen-aa2-indexed | 17,367,040 | 9,838,592 | 477,849,600 |
| calibrated-aa2-indexed | 17,387,520 | 9,838,592 | 477,125,632 |
| frozen-direct4 | 17,367,040 | 9,838,592 | 478,234,624 |
| calibrated-direct4 | 17,367,040 | 9,854,976 | 477,139,968 |
| frozen-direct4-indexed | 17,633,280 | 9,838,592 | 478,456,832 |
| frozen-stream4 | 17,371,136 | 9,838,592 | 479,877,120 |
| calibrated-stream4 | 17,371,136 | 9,838,592 | 479,728,640 |
| frozen-stream4-indexed | 17,371,136 | 9,838,592 | 479,994,880 |
| calibrated-stream4-indexed | 17,367,040 | 9,854,976 | 480,855,040 |

These overlapping UMA counters must not be added. They include retained driver/client
storage and caches where exposed, but do not isolate per-asset residency or prove
transient physical peaks. No physical 16x saving is established. Raw allocation records
include scratch, indices, inactive/fallback data, output, staging, upload/uniform rings
and screen resources; the physical snapshots and all completion records are retained.

### Next architecture-level bet and verification

Keep fine foliage detail. Test a source-derived analytic or sparse wood coverage/depth
representation that filters the pixel footprint directly, while preserving partial
wood/leaf occlusion. It must avoid four complete repeated wood draws, pass the whole
shaded image and silhouettes, and use the same authored curve/source information for
its conventional control. This is the next hypothesis, not an achieved rendering win.

No material-moment or other training was performed. Fifty-six implementation-control
images prove that indexed topology and bounded BC uploads preserve the corresponding
outputs byte for byte. The promoted default keeps the original model and AA2 quality;
all eight images remain identical, and the extra 4,028 live index bytes are verified.
Format/type checks pass. An initial helper-borrow compile failure is retained. No full
suite or PR-readiness claim is made.

Evidence: build/neural-evidence/g4-footprint-visibility-v1 on Pop!_OS. All eleven variants,
the two follow-ups, paired timing and power samples, source/build archives, raw images,
allocation/DRM/PSS records, failed compile log and decision are preserved.
This remains one static tree at 64 pixels with known lights. No family, unseen-light,
motion or 8-512-pixel claim; G1-G7 remain unchanged and unmet.
Manifest SHA-256: `62aeb743a297c6734bb92f9894bbd17c47a5871fb4881e537e89a823f4c651c5`.


## Source-curve wood visibility: retire the two-group path

**Decision: retire this experimental analytic-cone/two-group renderer for G4.**
The current compact appearance representation still has no demonstrated joint
quality, runtime and memory advantage. Keep the frozen fine-patch AA2 renderer as
the quality control; change how visibility is represented before further fitting.

The best new path uses 196,208 engine appearance/scratch bytes instead of 316,468,
but complete requested live GPU allocations fall only from 726,068 to 605,808 bytes
(16.56%, or 1.20x). It loses 0.435 dB against the frozen renderer, increases FLIP
9.97% and LPIPS 13.30%, and has paced full-GPU p95/p99 of 1.315/1.356 ms versus
0.270/0.272 ms. The equally informed conventional AA2 control is also stronger than
a BC-only comparison suggests: the candidate passes only 10/24 numeric quality cases
against it. This is a failed combined comparison, not a rendering win.

### Fixed inputs and bounded experiment

Solo on Pop!_OS. Work started 2026-10-27 19:48:18 UTC; the first complete assessment
was recorded at 20:10:56 UTC, and the final decision at 20:14:53 UTC. Both are within
the two-hour limit. Exactly two targeted follow-ups followed the first measurement.
No new references, material-moment fitting, other training or model-selection sweep.

The 1,897-patch best model remained frozen. The source-calibrated conventional
1,897-patch model receives the same authored geometry, source materials, camera,
lighting, sorting, footprint samples and new wood implementation. Its AA2 path is
the strongest previously measured conventional compact control in this fixture;
the lower-quality BC control is retained separately. Existing source-AA8 results
remain the higher-cost source-quality control. None is denied useful source inputs.

The new representation uses 264 closed circular frusta, one per existing source
wood ring interval: 12 intervals per limb, 22 limbs. Endpoints and radii are derived
from the [authored ring geometry](../engine/plant.wrela) and occupy 4,224 half-packed
bytes appended to either unchanged patch payload. Circular frusta approximate the
original angled polygonal rings; they are not exact source geometry. Conservative
screen tiles cull cones; four ray samples find nearest wood, depth and radial shading
normal. Sparse storage admits at most 512 wood-covered pixels and rejects overflow.

Leaf fragments blend into two 64x64 RGBA16F targets, conditioned on samples that hit
wood or remain open. This preserves partial wood occlusion but averages leaf
transmittance within each group before compositing. It does not retain independent
visibility samples. The complete output remains four-channel float32.

A nondeployable CPU diagnostic using already-completed exact source-wood samples
scores 39.545 dB; replacing that wood with quantized cones scores 39.428 dB.
The AMD cone path scores 39.428 dB as well. Thus exact wood alone would not recover
the frozen renderer's 40.128 dB. GPU-versus-CPU display discrepancy is less than
0.09% of reference MSE in these diagnostic cases; raw RGBA agreement is not exact.
This oracle is quality diagnosis only, with no runtime or memory advantage claimed.

### Two follow-ups and rendered quality

1. **Reuse the idle target away from wood.** In pixels where all four wood samples
   miss, split foliage samples into left/right pairs instead of averaging all four.
   This follows the measured error: 67.7% of excess neutral-background display MSE
   lies outside a one-pixel-dilated source-wood mask. No new allocation or fitting.
   Frozen quality improves 0.265 dB, but still misses tolerance.
2. **Share cone geometry work.** Project cones once per workgroup into shared
   camera-space records and reject individual rays by conservative bounds before
   intersection. Preserve the first follow-up's filtering. Separate profiles show
   substantially lower saturated GPU time, but it remains over six times the AA2
   control's saturated p95. Paced tails still fail. No precise causal speedup or
   promotion claim is made from separate, unpaired profiles.

All quality rows use the completed AA64 shaded references: two public views,
four known analytic lights and three backgrounds. FLIP uses the established 67-pixel-
per-degree LDR pipeline; LPIPS uses the pinned AlexNet configuration. Coverage and
boundary errors are measured against the same source images.

| Variant | PSNR dB | FLIP | LPIPS | Coverage MAE | Worst boundary p99 px |
| --- | ---: | ---: | ---: | ---: | ---: |
| bc-chunk | 30.685 | 0.06237 | 0.037719 | 0.016777 | 2.038 |
| frozen-aa2-indexed | 40.128 | 0.01235 | 0.001889 | 0.008832 | 1.414 |
| calibrated-aa2-indexed | 39.740 | 0.01392 | 0.002116 | 0.009348 | 1.414 |
| frozen-cones | 39.428 | 0.01457 | 0.002298 | 0.010020 | 1.414 |
| calibrated-cones | 38.939 | 0.01666 | 0.002662 | 0.010877 | 1.414 |
| frozen-open-split | 39.693 | 0.01358 | 0.002141 | 0.009403 | 1.414 |
| calibrated-open-split | 39.245 | 0.01549 | 0.002464 | 0.010133 | 1.414 |
| frozen-cached-cones | 39.693 | 0.01358 | 0.002141 | 0.009403 | 1.414 |
| calibrated-cached-cones | 39.245 | 0.01549 | 0.002464 | 0.010133 | 1.414 |

The final candidate's mean quality is close to conventional AA2 (-0.047 dB,
FLIP -2.45%, LPIPS +1.15%), yet only 10/24 individual cases satisfy all three
numeric tolerances. Against frozen AA2 it passes 1/24; against the lower-quality BC
control it passes 22/24. Mean scores and favorable BC ratios do not establish G4.
The frozen model is 0.448 dB better than the calibrated model on the same final cone
path, but this quality difference does not overcome its cost or prove a neural
architecture advantage.

![Source, frozen and conventional AA2, initial and final cone comparisons](figures/neural-wood-visibility.png)

Images are nearest-neighbor enlargements of actual 64-pixel captures. The final row
shows absolute display error multiplied by ten. Numerical evaluation uses float
captures, not these quantized illustrations.

### Complete rendering cost and memory

Same AMD RADV RENOIR/PCI 1002:164c, Ryzen 5 5500U, Mesa 26.1.6,
Chrome 154.0.8037.57; Wrela 0.9.0 compiler
de9b81e52a7747870dfbd5223cb8aeb41e584d23. One worker, 60 warmup and 240 measured
frames. View/light updates alternate every frame. Builds, captures, profiles and
metric scoring run sequentially.

GPU spans include cone decoding/projection/intersection, leaf decoding and shading,
visibility blending, integration and final display composite. CPU frame time includes
sorting, index/parameter uploads and command recording. CPU-start-to-final-GPU
completion also includes browser/driver notification; it is not input-to-photon.
Initialization is excluded from steady-state time but included in allocation peaks.

| Variant | Paced GPU p95/p99 ms | Saturated GPU p95 ms | CPU frame p99 ms | CPU to final GPU p95/p99 ms | Engine appearance/scratch bytes | All requested live/peak GPU bytes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| bc-chunk | 0.105/0.105 | 0.027 | 0.404 | 4.495/4.832 | 3,146,032 | 3,555,632/6,701,376 |
| frozen-aa2-indexed | 0.270/0.272 | 0.071 | 1.053 | 4.936/5.191 | 316,468 | 726,068/742,452 |
| calibrated-aa2-indexed | 0.272/0.274 | 0.071 | 1.001 | 4.231/4.326 | 316,468 | 726,068/742,452 |
| frozen-cones | 1.847/1.922 | 0.968 | 1.078 | 4.851/5.133 | 196,208 | 605,808/622,192 |
| calibrated-cones | 1.846/1.906 | 0.968 | 1.005 | 5.740/5.959 | 196,208 | 605,808/622,192 |
| frozen-open-split | 1.313/1.639 | 0.969 | 1.007 | 6.456/6.659 | 196,208 | 605,808/622,192 |
| calibrated-open-split | 1.990/2.189 | 0.968 | 1.057 | 5.761/5.956 | 196,208 | 605,808/622,192 |
| frozen-cached-cones | 1.315/1.356 | 0.442 | 1.080 | 4.801/4.985 | 196,208 | 605,808/622,192 |
| calibrated-cached-cones | 1.293/1.369 | 0.441 | 1.019 | 4.821/4.973 | 196,208 | 605,808/622,192 |

The AA2 and BC rows reuse sealed controls from the preceding block; they are not
new paired runs. The large runtime failure plus quality failure is sufficient to
reject the candidate, without repeatedly profiling unchanged controls.

All requested bytes count weights, cone geometry, indices, scratch, maps, inactive/
fallback resources, output, staging, upload/uniform rings and screen allocations.
The engine-only column excludes 409,600 identically sized bytes: 65,536 output,
262,144 upload ring, 65,536 uniform ring and 16,384 screen. Its 16.03x ratio against
BC becomes 5.87x when all requested allocations are included. Neither establishes
the G4 target, especially against the stronger source-calibrated control.

Shared workgroup storage is also explicit: the initial/split path declares 1,060
bytes for its cone list; the cached path declares 14,884 bytes, including 13,824
geometry bytes. These are per-workgroup shader storage, not persistent buffers.
Register spills, compiler caches and driver storage cannot be assigned to this asset
from requested-buffer counts. Actual resident snapshots follow:

| Variant | DRM resident VRAM bytes | DRM resident GTT bytes | Browser-group PSS bytes |
| --- | ---: | ---: | ---: |
| bc-chunk | 25,747,456 | 9,756,672 | 486,687,744 |
| frozen-aa2-indexed | 17,367,040 | 9,838,592 | 477,849,600 |
| calibrated-aa2-indexed | 17,387,520 | 9,838,592 | 477,125,632 |
| frozen-cones | 17,637,376 | 9,838,592 | 483,670,016 |
| calibrated-cones | 17,375,232 | 9,854,976 | 483,975,168 |
| frozen-open-split | 17,653,760 | 9,953,280 | 483,088,384 |
| calibrated-open-split | 17,375,232 | 9,854,976 | 483,990,528 |
| frozen-cached-cones | 17,371,136 | 9,838,592 | 480,525,312 |
| calibrated-cached-cones | 17,637,376 | 9,838,592 | 481,431,552 |

These UMA counters overlap and must not be added. They expose whole-client physical
residency including retained driver storage where reported, not per-asset attribution
or transient physical peaks. No physical 16x reduction is demonstrated. All raw
allocation, completion, DRM and PSS records are retained.

### Next architecture bet and checks

Preserve independent subpixel coverage, depth and order through compositing. Use
hardware indexed source wood as the control, and investigate compressing visibility
records or sharing appearance work after visibility is known. Wrela's source fields
can guide which samples and surfaces deserve work; the next representation must
preserve their occlusion, rather than merge visibility groups early. A learned
radiance model is only justified by a specific remaining shaded error.

The retired diagnostic is disabled by default. The original model hash and all eight
frozen outputs remain byte-identical, with unchanged native resource counts.
A forced one-record capacity makes all pixels in all eight captures explicitly invalid:
no dropped wood or implicit fallback is accepted. Cached geometry changes individual
RGB values by less than 0.000003; all 16 before/after image pairs and actual scores are
retained without claiming byte identity. Format/type checks pass. The initial cached
shader parse failure and corrected reporting-bound check are preserved. No full-suite or PR-readiness claim is made.

Evidence: build/neural-evidence/g4-wood-visibility-v1 on Pop!_OS. Six measured
candidate/control variants, both follow-ups, CPU diagnostics, failed and successful
sources, builds, float images and measurements are preserved. Existing truth and
prior control manifests are dependencies; no reference infrastructure was expanded.
One static tree at 64 pixels cannot establish family, motion, unseen-light or
8-512-pixel behavior. G1-G7 remain unchanged and unmet.
Manifest SHA-256: `d67ea7938d2cc74c2834b2c49eca4d11124572149ee378e908e9eae3612a7139`.
