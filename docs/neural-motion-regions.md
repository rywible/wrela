# Source motion bounds pay when they select a specialized shader

2026-10-29. Parent `3b37241f66ce8812626d3836ca91b80535a7329b`.

**Continue the source-region specialization; retire the two preceding variants.**
Five alternating production comparisons give a complete-GPU median across pairs
of **26.09468 → 25.34972 ms**
(2.85% lower). Requested live allocation remains **405,267,256 B**.
This is a conventional production-renderer improvement, not a new learned model,
upscaler quality improvement, G4 result or G1–G7 pass.

First joint quality/runtime/residency assessment: **5.54 minutes**.
Architecture decision after exactly two followups: **27.87 minutes**;
the paired, boundary and HDR checks subsequently confirm retention. No training or
new reference algorithm. All eight frozen artifacts and G1–G7 remain unchanged.

## The source fact and how it is used

The clearing's Scene.motion implementation has one writer: the fawn. Its existing
moves_in rectangle encloses the projected posed bounds with slack and a screen
margin. A new Scene.motion_bounded contract explicitly states that this rectangle
contains every nonzero motion texel, including deformation. The default is false.
The clearing opts in because of that single writer. Adding another writer requires
updating the bound or withdrawing the declaration; a colour tag alone is not proof.

The shared temporal resolver expands the rectangle by two input pixels at the
output scale, rounds outward to integer output boundaries and clips to the target.
It draws the central rectangle with the original decoder and up to four disjoint
surrounding rectangles with a zero-motion specialization. That specialization
removes current motion loads and allows dependent code to simplify at compile time.
It still uses the original depth, camera reprojection, old history, colour clipping,
old-motion handling, sampling and blending. It does not substitute identity
reprojection or a cheaper history filter.

The generic path remains available to all scenes. Gathered/native fixtures do not
silently acquire a source assumption. All compared paths have the same input
textures, source geometry, scene density, quality settings and frozen models.
The dynamic-bound comparator receives the same source declaration as the selected
method, but applies it inside each fragment. This isolates the value of choosing
a specialized program before the draw rather than claiming privileged inputs for
a learned method. There is no neural contribution in this experiment.

## Complete production cost and actual residency

Pop!_OS AMD RADV RENOIR PCI1002:164c, Ryzen5 5500U, Mesa26.1.6,
Chrome154.0.8037.57, Wrela0.9.0/compiler
`de9b81e52a7747870dfbd5223cb8aeb41e584d23`.
Full clearing output1920×1080, production2× input scale, one worker,60 measured
frames after600warmup; adaptive pacing and measured-work planning disabled.
All captures and profiles run in one sequential AMD queue. These full frames
include wood, vegetation visibility/prepass, shading, clouds, probes, compositing,
temporal reconstruction and postprocessing. CPU submission cost is retained too.

| Variant | Full saturated median / p95 ms | Temporal median / p95 ms | Paced full median / p95 / p99 ms |
|---|---|---|---|
| baseline | 26.07804 / 27.88792 | 5.04136 / 5.09236 | 26.20184 / 27.75045 / 28.49225 |
| shared-motion | 26.32804 / 27.95460 | 5.35796 / 5.41600 | 26.41580 / 28.08413 / 29.21388 |
| source-bounds | 26.57744 / 28.29480 | 5.58956 / 5.67588 | 26.64468 / 28.39512 / 29.37046 |
| source-regions | 25.43100 / 26.85512 | 4.30072 / 4.35144 | 25.46968 / 27.17669 / 27.87659 |

The distributions are separate: a saturated median is not combined with a paced
tail. No thermal/power lock,20-minute deadline study or presentation claim.
Cold events remain in the full build/timing logs; the table is explicitly warm.
The construction of source bounds and the extra draw submissions are included.

| Variant | Requested live / peak B | Resident VRAM / GTT B | Browser PSS B | CPU frame-call median / p95 ms |
|---|---|---|---|---|
| baseline | 405,267,256 / 585,383,112 | 551,931,904 / 20,180,992 | 854,802,432 | 1.70000 / 1.95500 |
| shared-motion | 405,267,256 / 585,383,112 | 551,944,192 / 20,279,296 | 848,704,512 | 1.68000 / 1.94000 |
| source-bounds | 405,267,256 / 582,445,428 | 551,972,864 / 20,180,992 | 809,184,256 | 1.70500 / 1.92000 |
| source-regions | 405,267,256 / 582,445,428 | 551,997,440 / 20,099,072 | 854,730,752 | 1.72000 / 1.98000 |

Each residency point is a separate660-frame replay of the same compiled build.
All requested resources, histories, scratch, common/fallback data and output count.
Actual DRM observations are deduplicated by device/client and PSS covers the browser
process group. They are whole-client snapshots, not per-asset physical peaks; do not
sum UMA pools. Startup peak and residency variations do not establish a memory win.
The source-region method allocates no new textures or histories.

Five alternating pairs use the same full workload. GPU changes:
**-2.704%, -2.735%, -3.324%, -2.987%, -2.344%**.
Tool status: **faster**. CPU frame-call medians across pairs are
1.665→1.720ms; paired median change+3.303%, tool status **inconclusive**.
This small submission-cost increase remains part of the result. Every individual
run is preserved in paired/result.json. CPU frame-call time is not frame completion.
These repeats support this device/workload result, not a cross-scene or cross-GPU
confidence statement.

## Quality and why the focused checks matter

Four batched analytical captures contain six cases1,6,13,23,74,77, four scales and
32 cold frames each:3,072 output frames. Each candidate's768 frames, targets and
visibility masks are byte-exact to the fresh baseline. The baseline is also exact
to the previously scored temporal-identity control, so existing shaded FLIP, LPIPS,
PSNR and temporal metrics are reused with hashes. No metric is rerun for identical
images. These full-screen motion bounds mostly exercise the generic path; they
alone would not validate the source-region specialization.

The complete production final frame is exact for shared-motion and source-regions.
The dynamic-bound version differs in one byte, maximum1; its repeat replay and the
baseline replay also differ by one byte. No reference-relative improvement is
claimed from this variation. Boundary-band differences and repeat observations are
preserved in the joint assessments.

The retained GPU fixture tests seven motion-region configurations at scales1–4:
interior, top/left clipping, bottom/right clipping, full target, offscreen, empty,
and a thin fractional region. Actual input motion includes rigid and deforming
tags. Both resolvers receive identical colour, depth, motion and history textures.
A shader compares their unquantized RGBA16F values before encoding the result.
All **161,280 pixel comparisons** are exact. A deliberately empty bound while motion
is present must change more than100 pixels; the test passes this negative control.
This is an equivalence/contract test, not a new G3 motion benchmark.

The existing clearing HDR batch is replayed at the same fixed tick, shot0,
192×108,240warmup, phase64/seed17/cells2. All **seven HDR buffers** match the completed
temporal-reference batch exactly: original coarse/fine/independent references plus
temporal1×/2×/3×/4× outputs. The established reference uncertainty is unchanged.
This resolves cross-scale production HDR equivalence without a new reference method.
It remains a fixed-pose result, not broad animated-visibility qualification.

## The two failed approaches

Initial shared-motion certification read a3×3 neighbourhood of the actual motion
target alongside existing colour moments. An exact negative-integer encoding in
the existing class-mask channel marked all-zero motion; no new allocation.
Quality was exact, but complete cost rose26.07804→26.32804ms and temporal cost
5.04136→5.35796ms. Building the certificate cost more than it saved.

Followup1 used the source bound directly as a per-fragment condition, avoiding
certificate construction. Full cost still rose to26.57744ms, temporal5.58956ms.
Followup2 moved the decision to CPU draw partitioning and compiled separate shader
paths:25.43100ms full and4.30072ms temporal in the initial selected run.
The evidence supports specialization over these dynamic checks; it does not
isolate a particular instruction, register-pressure or memory bottleneck.

Failed preparation syntax, invalid generated-field placement, incorrect vertex
API usage and the initial test annotation are archived with their diagnostics.
They produced no rendering result. All successful variants' complete sources,
builds, commands, captures and allocation observations are retained. Exactly two
optimization followups; additional work is focused verification, not more tuning.

## Remaining gap and next bet

This small full-renderer win moves toward G6, but the complete frame is still
around25ms, far from8.33ms. Quality and representation memory have not improved,
and the G4 compact appearance model remains unqualified. Other scenes keep the
generic path until they can make the motion-source guarantee. The bounded
clearing workload and fixed HDR shot do not prove every pose or future writer.

The next architecture bet is source-declared reconstruction regions that select
whole specialized operators. First determine whether the larger no-explicit-motion
region can share more of its remaining work while preserving camera reprojection
and history filtering; the earlier stationary-identity shortcut remains rejected.
Require a production comparison and bounded scratch before claiming a win.

Evidence: build/neural-evidence/motion-certificate-v1 on Pop!_OS.
Manifest: `3e17c376c548caa77706d95d2205ab5b1a1eeaa1fcf78396640fb2f405e950ad`.

After sealing, the final commit removes one empty line at the end of the focused
test fixture to satisfy the Git whitespace check. This is the only source difference
from the retained archive; executable text is unchanged. Fixture SHA256:
archived `83eb3550b1795a2854c8318dd1d79ba925c08a211e2eb82a990075320984bc51`,
committed `405c6f6919ed30ddd6787e2c12655c8d367ca9b84b052bcaefab3371f02bfa2f`.
