# Temporal shortcuts fail the joint quality and cost decision

2026-10-29 UTC. Parent `388ca647b7d7530c0942b02986c206c68033fe74`.

**Decision: retire all three shortcuts and restore the parent runtime.**
The exact-centre variant reproduces all 768 analytic output frames exactly and
stays within one channel byte in the inspected full clearing image, but complete
GPU median rises from 26.14876 to 26.42412 ms (1.05%) and the temporal
pass from 5.06712 to 5.34344 ms (5.45%). The two faster variants change
localized full-scene pixels by up to 98 and 37 bytes. Their tiny, mixed analytic
metric changes do not establish a quality improvement. All retain the same live
storage. None demonstrates a joint rendering advantage.

First joint full-scene plus analytic assessment: **8.64 minutes**.
Retirement: **25.56 minutes**, after exactly two targeted
followups. G1–G7 and all frozen models remain unchanged. No training or new
reference algorithm was added.

## Question and variants

The [previous cloud study](neural-cloud-intervals.md) identified an approximately
5 ms temporal stage. In [the temporal implementation](../engine/temporal.wrela),
static surfaces reconstruct world positions and reproject them into the previous
camera even when the two view-projection matrices are equal. This experiment asks
whether source-known frame invariants can remove that work safely.

The initial **identity** candidate substitutes current pixel and depth for the
previous pixel and expected depth only when the matrices compare equal, after
the existing explicit-motion and deformation branches. Moving-camera and explicit
motion paths retain the original arithmetic.

Followup 1, **stationary-texel**, restores all original pixel/depth arithmetic.
When the camera is held, the surface has no explicit motion/deformation, and the
original reprojection is within 1/1024 pixel of the current texel centre, it reuses
the RGB from the existing history-tag load instead of the five-tap Catmull-Rom
gather. Other positions keep the original filtered gather.

Followup 2, **exact-texel**, keeps the original reprojection and only collapses
the gather where `last - 0.5 == floor(last - 0.5)`. The existing filter's
weights are then 0, 1, 0, 0. It shares the tag texel's colour; every non-centred
position retains the original gather. No approximate epsilon, new cache,
additional input or learned decoder is introduced.

Equal source formulas do not guarantee equal finite-precision execution. History
depth validation and vacancy tests select discrete neighborhoods from reprojected
coordinates, and accumulated/filter/clipped colour can propagate differences.
The observations motivate that explanation, but this block does **not** isolate
which operation causes each changed pixel. It also does not show that every
changed clearing pixel is worse than independently shaded truth.

## Complete rendering cost and memory

Pop!_OS, AMD RADV RENOIR PCI 1002:164c / Ryzen 5 5500U, Mesa 26.1.6,
Chrome 154.0.8037.57, Wrela 0.9.0/compiler
`de9b81e52a7747870dfbd5223cb8aeb41e584d23`.
1920×1080 clearing, one worker, 60 measured frames after 600 warmup.
Resolution and recurring probe work are fixed across methods. Complete cost
includes wood, vegetation, visibility, atmospheric work, updates, reconstruction
and compositing. No candidate gets privileged source information.

| Method | Saturated complete median / p95 ms | Temporal median / p95 ms | Paced complete median / p95 / p99 ms |
|---|---|---|---|
| baseline | 26.14876 / 27.72596 | 5.06712 / 5.10440 | 26.25062 / 27.94348 / 28.60047 |
| identity | 25.93984 / 27.63688 | 4.92956 / 4.97460 | 26.06708 / 27.74863 / 28.46054 |
| stationary-texel | 25.72540 / 27.33424 | 4.72864 / 4.78412 | 25.82518 / 27.46657 / 28.23155 |
| exact-texel | 26.42412 / 27.94176 | 5.34344 / 5.39744 | 26.42780 / 28.04409 / 28.94458 |

Saturated statistics come from `profile.json`; paced statistics come from
the separate `profile-build/results/timings.json` run. They are kept separate.
These are unpaired development comparisons without a power/thermal lock.
The smaller medians of the first two variants are not confidence-qualified
speedups. No paired confirmation is warranted while quality remains unresolved;
the last variant already loses on measured cost.

| Method | Requested live / peak B | Actual resident VRAM / GTT B | Browser PSS B |
|---|---|---|---|
| baseline | 405,267,256 / 585,383,112 | 551,936,000 / 20,099,072 | 843,008,000 |
| identity | 405,267,256 / 582,445,428 | 551,936,000 / 20,361,216 | 822,095,872 |
| stationary-texel | 405,267,256 / 582,445,428 | 551,981,056 / 20,361,216 | 801,421,312 |
| exact-texel | 405,267,256 / 582,445,428 | 551,907,328 / 20,099,072 | 801,632,256 |

Each compiled build has a separate 660-frame allocation replay. Requested
accounting includes all scratch, caches, histories, source/fallback data and common
renderer allocations. All methods use **405,267,256 live requested bytes**.
Peak differences reflect allocation overlap; they are not a storage saving.
DRM observations are deduplicated by device/client and PSS covers the browser
process group. They are actual whole-scene resident snapshots, not per-component
physical peaks. Overlapping UMA pools must not be added together. Neither smaller
PSS nor small driver variation establishes a candidate memory advantage.

## Shaded quality, boundaries and temporal evidence

| Method | Max channel byte difference | Mean RGB byte difference | Pixels above two bytes | Independent replay max byte |
|---|---|---|---|---|
| baseline | 0 | 0.0000000000 | 0 | 1 |
| identity | 98 | 0.0354004308 | 3001 | 1 |
| stationary-texel | 37 | 0.0308097029 | 2149 | 0 |
| exact-texel | 1 | 0.0000001608 | 0 | 0 |

Full-scene comparison uses the parent's final shaded image, not independently
converged transport. All compared final alpha values are exact. The existing
image-derived contrast band records mean differences of
0.118367 and
0.086307 bytes for the two
approximate shortcuts. Final opaque alpha and an image-derived band do not certify
semantic silhouette accuracy. Baseline independent replay variation is at most
one channel byte.

The analytic batch reuses independent shaded references and visibility masks for
cases 1, 6, 13, 23, 74 and 77, at 192×108, scales 1–4, 32 cold-start frames.
Matched-reference and mask checks precede FLIP, LPIPS, displayed-linear error,
PSNR, temporal correspondence, visibility-change and newly-exposed error scoring.

| Candidate | Largest linear RGBA difference | Exact frames / 768 | Mean relative FLIP change at scales 1 / 2 / 3 / 4, percent |
|---|---|---|---|
| identity | 2.439453125 | 24 | -0.004008 / +0.001090 / +0.000814 / +0.000404 |
| stationary-texel | 0.653564453125 | 24 | +0.001099 / -0.000965 / -0.002342 / -0.000250 |
| exact-texel | 0.0 | 768 | +0.000000 / +0.000000 / +0.000000 / +0.000000 |

The first two variants are exact only on the first frame of each case/scale;
their largest HDR changes are substantial despite nearly unchanged global averages.
The exact-centre candidate preserves all 768 RGBA32F outputs exactly. Its
perceptual, temporal and visibility-region scores therefore equal the baseline.
Raw per-case/scale results, including failures, remain in each joint assessment.

This public analytic set and one full-scene image do not qualify broad motion,
production silhouettes, G4 or G1–G7. The local clearing changes are an uncertainty
to resolve before adopting altered temporal arithmetic, not a reason to call
the baseline itself ground truth.

## Batching, verification and evidence

Four executable capture runs produce **3,072 output frames** across 24 cases;
each run batches six cases and four scales. Their total recorded capture wall time
is **95.27 seconds**, including their tool invocation/build work but
excluding full-scene profiles, allocation replays and CPU scoring. This is an
observed workflow duration, not a measured speedup over an unbatched control.

Each candidate follows the single AMD queue: complete profile and allocation
replay, then batched analytic capture, then CPU scoring. A result determines the
next targeted variant. Source archives, compiled builds, raw images, timings,
residency observations and metric outputs are retained. Formatting and the initial
type check succeeded; all candidate GPU runs completed. Existing generic temporal
tests and paired runtime repetitions were conditional on promotion and were not
run for these failed candidates. The unchanged engine was not re-audited.

Pop!_OS evidence: `build/neural-evidence/temporal-identity-v1`.
Manifest: `0502a8e71784f0a2c4ea5baa329cda50996f0097a17b845292ea72e5aacbe290`.
The manifest pins internal evidence, all four external capture roots, scripts,
dependencies and eight frozen files. Runtime restoration is verified against
the parent. The inherited benchmark edits remain unstaged.

## Next architecture-level bet

**Represent temporal correspondence explicitly in discrete source/surface
coordinates.** Approximate camera identities and approximate texel centres are
too weak a basis for dropping work when discrete history decisions can change.

Before implementing that representation, resolve the demonstrated uncertainty:
use the existing [static clearing capture](../studio/reconstruct/clearing/main.wrela)
and phase64 reference path to compare baseline and identity at a matched shot,
including the regions with large differences. Reuse completed references only
when source, pose, resolution, lighting and sampling conditions match; otherwise
reuse the existing executable to obtain the needed capture. The shortest useful
experiment decides whether the changed correspondence improves or harms shaded
quality and whether it is worth addressing at all. It requires no new training,
new reference framework or further tuning of these three retired shortcuts.
