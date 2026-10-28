# Source coverage and per-surface radiance

**Decision: continue this factorization as an optional conventional diagnostic
baseline.** Keep geometric coverage explicit and estimate colour separately for
each surface. A causal same-surface fallback improves missing-support pixels.
No neural model is trained or promoted, and this does not establish G4 viability.
The broader appearance representation still needs to change: the frozen compact
model's shaded silhouette/material failures remain unresolved.

First complete quality/runtime/memory assessment: 6.96
minutes. Decision: 31.62 minutes, 2026-10-28T14:49:01.859861+00:00.
One targeted follow-up; no second optimization, training, material-moment fitting,
new reference method or new reference cases. All G1–G7 remain unchanged and unmet.
Integration checks completed afterward; they are not a rendering gain.

## Equal source information and the representation

All variants receive the same authored slat period, width, offset and speed,
the same sixteen-frame 20-byte packed observation history, the same jitter
sequence, motion/depth and current fallback. Exactly one colour point per
low-resolution cell per frame is retained. No source RGB, checker frequency,
reference image, reference coverage mask or future sample reaches the decoder.
The conventional baseline is the strongest retained regular source filter,
including its existing shared jitter preparation. Frozen learned controls stay
unchanged; giving a learned method exclusive geometric information would be unfair.

The new decoder intersects each unit output pixel with source slat intervals
(at most two because these fixture periods exceed one), producing geometric
foreground/background area. This implementation is separate from the reference's
periodic antiderivative and never calls its colour integral or coverage routine.
It then estimates each contributing surface's radiance from compatible retained
observations and combines the estimates using geometric area. This computation
is charged to the GPU span. It is a source-aware full-stack diagnostic, not a
fixed-input upscaler or a general 3D visibility solution.

The first variant rejects samples no longer visible in the current frame.
The second retains those observations for their own surface's colour estimate.
Neither averages foreground and background radiance into one normalized cloud.
Each uses the existing tent support, motion/depth checks and nearest-current
colour when a required surface has no support. That final fallback can borrow
the wrong surface's colour and corrupt an otherwise correct geometric edge.

An independent replay found that in case1/frame1/4x, wrong-surface fallback
pixels account for 52.06% of named-view colour error. A causal replay preview
reduced mean error .09514→.05505 by searching compatible observations. This
justified the sole follow-up: only when a required surface has zero tent support,
search a 3x3 neighbourhood in each retained frame, newest first; select the
closest compatible sample in the first frame with a match. If none exists,
retain the original current fallback. No extra colour queries, buffers or cache.
This is a local radiance extrapolation, not evidence that an unobserved surface
can always be reconstructed.

## Rendered quality, including boundaries

Four existing public cases (1,6,13,23), 32 cold-start frames, 192x108 output,
1x–4x linear reduction. The unchanged evaluator uses exposure1, x/(1+x), sRGB,
LDR FLIP at67ppd, calibrated AlexNet LPIPS v0.1 and reference-subtracted temporal
error with valid opaque correspondences. Reveals remain separate. Means average
clip aggregates; PSNR is mean clip PSNR, not pooled MSE. These public diagnostic
means are not G1 median/worst-decile gates or sealed validation.

| Method | 4x mean FLIP | LPIPS | Mean clip PSNR dB | Opaque temporal MAE |
| --- | ---: | ---: | ---: | ---: |
| Regular source control | 0.24324395 | 0.25324590 | 16.36942 | 0.02631498 |
| Separate radiance, visibility rejection | 0.19215613 | 0.20210962 | 17.37593 | 0.01805052 |
| Separate radiance, retained observations | 0.19035943 | 0.20037096 | 17.50127 | 0.01501501 |
| Same-surface fallback (selected) | 0.17217554 | 0.19584559 | 18.10427 | 0.01482404 |

The selected path lowers 4x FLIP 29.22%, LPIPS22.67% and opaque temporal error
43.67%, with +1.73485dB mean clip PSNR against regular. The fallback improves
FLIP another9.55% over the unfilled factorization but costs more GPU time.
It is a measured quality/cost choice, not dominance over every alternative.
All sixteen case/scale mean-FLIP values improve against regular; that does not
prove every frame, category, scene or metric improves.

| Scale | Regular FLIP | Visible radiance | All retained radiance | Same-surface fallback |
| --- | ---: | ---: | ---: | ---: |
| 1x | 0.09620063 | 0.05481804 | 0.05635493 | 0.05428683 |
| 2x | 0.13385482 | 0.09175009 | 0.09170266 | 0.08432950 |
| 3x | 0.19165672 | 0.15004408 | 0.14864739 | 0.13477913 |
| 4x | 0.24324395 | 0.19215613 | 0.19035943 | 0.17217554 |

Boundary colour is also scored on the existing reference pixels whose geometric
coverage is strictly between zero and one. This diagnostic uses scene-linear
RGB after x/(1+x), averages frames then clips, and includes cold frames. It is
not a new reference or a substitute for silhouette precision/recall.

| Method | 4x mixed-coverage colour MAE | 4x newly exposed error |
| --- | ---: | ---: |
| Regular source control | 0.09537262 | 0.09197359 |
| Separate radiance, visibility rejection | 0.05551014 | 0.07512288 |
| Separate radiance, retained observations | 0.05210041 | 0.07492705 |
| Same-surface fallback (selected) | 0.03739491 | 0.07490307 |

Independent double-precision coverage integration agrees with the existing
reference masks within 0.00000477
in the audited frames. Actual RGB on those boundaries still depends on radiance
support; geometric accuracy alone is not a shaded silhouette pass. No G3
strand precision/recall, trail p99 or reveal-time guarantee is inferred.

## Complete AMD cost and physical memory observations

Pop!_OS, AMD RADV RENOIR PCI1002:164c, Ryzen5 5500U, Mesa26.1.6,
Chrome154.0.8037.57; Wrela0.9.0/compiler
`de9b81e52a7747870dfbd5223cb8aeb41e584d23`. Parent `6a102762` plus
archived variants. GPU runs are sequential, with no concurrent scoring/training.
Timing: 1920x1080, 4x, case1; 32 warm-up and240 measured frames. Separate
completion/memory run:300 paced frames, discarding60.

| Method | GPU median/p95 ms | CPU call p99 ms | Start-to-completion p95/p99 ms |
| --- | ---: | ---: | ---: |
| Regular source control | 18.49432/18.80188 | 0.6150 | 80.0780/80.6648 |
| Separate radiance, visibility rejection | 15.13820/15.41620 | 0.6172 | 66.5923/67.0664 |
| Separate radiance, retained observations | 14.19108/14.40576 | 0.7402 | 48.3995/63.4078 |
| Same-surface fallback (selected) | 15.97144/18.13852 | 0.6350 | 68.2859/73.9566 |

Sampling, guide/depth/motion, retention, decoding, source coverage, visibility
and support checks, fallback search, updates, display and compositing are counted.
This analytic fixture has no wood or primitive sorting. Their costs remain in
[the existing shaded G4 experiments](neural-shaded-appearance.md), and must be
measured again when this representation is tested there. This comparison does
not replace that end-to-end shaded assessment.

Selected GPU median is13.64% lower than regular, while its p95 advantage is
smaller (18.13852 vs18.80188ms). The unfilled variant is faster at14.19108ms.
Completion includes queueing and notification; it is not GPU time. Startup
jitter preparation occurs on resize and is excluded from steady-state warm-up
statistics; requested allocation peaks include startup resources. Cuts were
checked for correctness, not latency tails. These short runs do not certify
repeatable speedup, GPU p99,20-minute stability, latency or G6's8.33ms budget.

| Method | Requested live/peak B | DRM resident VRAM/GTT B | Browser-group PSS B |
| --- | ---: | ---: | ---: |
| Regular source control | 89,493,784/97,788,184 | 174,653,440/18,706,432 | 570,808,320 |
| Separate radiance, visibility rejection | 89,493,784/97,788,184 | 178,114,560/18,706,432 | 539,528,192 |
| Separate radiance, retained observations | 89,493,784/97,788,184 | 177,983,488/18,706,432 | 571,096,064 |
| Same-surface fallback (selected) | 89,493,784/97,788,184 | 174,583,808/18,706,432 | 558,850,048 |

All requested allocations include history, inactive fixture resources, output,
scratch, upload/uniform buffers, jitter preparation and fallback data. The
factorization adds no GPU allocation. Physical DRM/PSS are actual whole-client
snapshots, not asset-attributed memory or transient physical peaks; overlapping
UMA pools must not be added. No memory compression or16x appearance claim.

After exposing the selected path as `--decoder raw-factor`, final GPU median/
p95 is 15.95700/18.09632ms;
requested live/peak 89,493,784/97,788,184B.
DRM VRAM/GTT is 174,452,736/18,706,432B,
browser PSS 557,279,232B, completion p95/p99
67.7293/74.6664ms.
Exact final source and browser/driver evidence are preserved.

## Verification and preserved failures

The original regular control reproduces all1,536 prior image/reference/mask
files exactly. Every variant's targets and masks are byte-identical. Separate
single-frame snapshots reproduce ordinary sequence images exactly at all scales.
Independent CPU replay uses actual captured raw observations, exhaustive5x5
candidate enumeration and a different coverage integral. It checks512 pixels
per case/frame/scale; maximum named-view RGB error is
0.00024190 for the initial variants
and 0.00024190 for the fallback.
Replacing integrated coverage with binary centre coverage is a failing negative.

Three audit failures are retained, not hidden: single-frame warmup0 is rejected
by the existing tool; display captures force alpha1 whereas raw observation
alpha contains semantic tags; and nearest-sample floor selection at exact
integer boundaries differs with GPU arithmetic. The corrected replay excludes
source-coordinate floor boundaries within1e-4 and records their fraction
(max 0.500000); it
keeps the original RGB tolerance. Whole-image quality scores still include every
pixel. This bounded replay is not full-frame numerical equivalence. No renderer
or reference was changed to make the audit pass.

Final integration reproduces all2,688 selected/control/frozen-model
image/reference/mask files exactly: four selected clips plus the conventional
control and both frozen raw-kernel models. Cut/reset matches a fresh start for
two frames at every scale; ignoring reset changes pixels in every scale.
Source/type and workspace allow-list checks pass. No unchanged broad test suite
is repeated. `raw-factor` is explicit and fixture-only; original `raw-source`,
learned/indexed paths and ordinary renderer behaviour remain unchanged. Trained
raw weights and specialization are rejected for this new conventional mode.

## Next architecture-level bet

Keep explicit source visibility and coverage, and represent radiance per surface
in source coordinates with a hierarchy that retains detail where rendered error
requires it. First test that separation on the completed shaded G4 references
against the equally informed conventional source renderer, including wood,
visibility, sorting/updates, decoding, compositing and all resident storage.
The shortest consequential next comparison uses the existing two views,
four lights and three backgrounds; no new reference infrastructure or training
is needed to decide whether surface identity fixes the compact model's failures.

The improvement here is conventional and limited to a two-layer analytic
fixture. It is evidence for the factorization, not proof of a portable renderer
or a neural advantage. The frozen compact appearance model remains unqualified;
G1–G7 stay active and unchanged.

Existing method/evaluator sources are cited in
[the reconstruction note](neural-kernel-reconstruction.md#primary-sources) and
[the citation register](citations.md#neural-reconstruction-instrument).
The interval intersection, factorized gather and fallback are original Wrela
code; no external implementation, model weights or assets were imported.

Evidence: `build/neural-evidence/coverage-radiance-v1` on Pop!_OS.
Manifest SHA-256: `1376a43a3faef8f1e258cb31ae9aa68547e1aea8b776b134d774e420072a2bc5`.

## Shared support tables: retain exact output with lower complete GPU cost

**Decision: retain shared support tables and masks in the explicit raw-factor
diagnostic.** Three alternating pairs reduce complete 1080p GPU time from
15.95852 to 10.77440 ms, a 32.52% median saving, with identical tested images.
Requested live memory rises by 2,519,280 bytes, from 89,493,784 to 92,013,064.
This is a conventional execution improvement, not a neural win or a G4 pass.

First complete comparison: 3.31 minutes. Decision:
19.16 minutes, 2026-10-28T16:34:49.839417+00:00. Exactly two targeted
follow-ups, no training, material-moment fitting or new reference cases.
The frozen models and G1–G7 thresholds remain unchanged and unmet.

### Share source-derived work without changing available information

The original factorization repeats coordinate bounds, motion correspondence and
tent-support work per output pixel, surface and history frame. The selected path
shares those calculations per column/surface/history age and per row/history age.
All RGB still comes from the same retained point observations. Both the original
and selected conventional methods have the same source geometry, visibility,
motion, sampling knowledge and history; no reference colour or extra sample is
introduced.

The first variant stores four bounds per axis query: low/high for tent support
and low/high for the existing fallback's 3×3 search. The roundoff margin uses the
maximum image extent and foreground displacement, conservatively covering the
original pixel-dependent bound. Acceptance and final weight calculations remain
unchanged. All 512 output frames match exactly, but complete GPU time only falls
15.96936→15.69360 ms while storage increases 1,259,520 bytes.

The first follow-up adds three scalar tent weights per axis query. It predicts
stored motion by packing the authored speed to half precision. This is incorrect
for the measured AMD render target at case 6, 3×: actual stored foreground motion
is 0.87451171875 pixels/frame, while the prediction is 0.875244140625.
An existing raw-history snapshot establishes the discrepancy directly.
Thirty-one output frames change, with maximum RGB difference 0.0009765625.
The small metric change does not make the failed equality assumption acceptable;
this variant and its observations are preserved and rejected.

The second and final follow-up reads actual encoded depth/motion observations.
This analytic fixture's vertical slats have row-invariant geometry and motion,
so each history age scans the first source row until it finds both surfaces or
reaches the end. It stores foreground motion and two surface-presence bits.
This source-specific fact is not a general 3D assumption.

The shared tables use that observed motion, and each column/row stores a mask
of history ages with positive support. The pixel intersects the two masks and
visits set bits from lowest to highest age. Positive contributions retain the
original age/y/x order, compatibility checks, normalization and causal same-surface
fallback. Missing surfaces contribute no unsupported samples. This removes work
for history frames that cannot contribute, beyond merely replacing arithmetic
with table reads.

### Shaded output, boundaries and temporal quality

The existing four public cases 1, 6, 13 and 23, each 32 cold-start frames at
192×108 and all four scales, remain the quality comparison. The selected masks
match all 512 RGBA frames and all 1,024 reference/mask files byte-for-byte.
Thus the existing spatial, mixed-boundary and temporal scores are unchanged:

| Scale | Mean FLIP | LPIPS | Mean clip PSNR dB | Opaque temporal MAE |
| --- | ---: | ---: | ---: | ---: |
| 1× | 0.05428683 | 0.02647699 | 27.03800 | 0.00617403 |
| 2× | 0.08432950 | 0.05266853 | 22.75674 | 0.00951335 |
| 3× | 0.13477913 | 0.12168463 | 19.67669 | 0.01315467 |
| 4× | 0.17217554 | 0.19584559 | 18.10427 | 0.01482404 |

Quality scores are reused only after exact image/reference parity. The changed
case in the rejected weight variant was rescored with the established evaluator.
No unchanged metric machinery was repeatedly audited. These public mean scores
are not the G1 median/worst-decile gates; geometric coverage is not a new G3
precision/recall proof. In particular, 4× mean FLIP remains 0.17218.

### Complete AMD runtime and resident memory

RADV RENOIR PCI1002:164c, Ryzen 5 5500U, Mesa 26.1.6, Chrome 154.0.8037.57,
Pop!_OS; Wrela 0.9.0/compiler de9b81e52a7747870dfbd5223cb8aeb41e584d23.
Parent 5c61df01 plus archived variants. Timing is 1920×1080, 4×, case 1:
32 warm-up plus 240 measured frames. Separate completion/memory runs use
300 paced frames, discarding 60. GPU runs are sequential; fitting/scoring does
not overlap them.

| Method | GPU median/p95 ms | CPU call p99 ms | Start-to-completion p95/p99 ms |
| --- | ---: | ---: | ---: |
| Original factorization | 15.96936/18.17032 | 0.6261 | 67.7839/75.2589 |
| Shared bounds | 15.69360/17.47324 | 0.6500 | 64.8693/71.8246 |
| Predicted-motion weights (rejected) | 15.45744/17.03120 | 0.6992 | 56.3141/67.2709 |
| Observed-motion support masks | 10.77908/12.04344 | 0.6988 | 16.4323/19.9065 |

The complete frame includes sample/guide generation, retention, surface-motion
scan, table construction, support masks, gather, fallback, updates and display/
compositing. The two added preparations cost about 0.2555 ms combined in the
selected initial run. This fixture has no wood or primitive sorting; their G4
costs remain measured in the shaded appearance report. No transfer of this speedup
to the full tree renderer is claimed.

Three alternating baseline/candidate pairs show savings of
32.589%, 32.517% and 32.458%. The exact bootstrap over the 27 resamples of those
three paired run-median savings gives a descriptive 95% interval of
32.471–32.574%.
This small fixed-workload sample does not establish cross-scene or sustained
performance. CPU changes are inconclusive. Do not interpret completion time,
which includes queueing and browser notification, as GPU time or photon latency.

| Method | Requested live/peak B | Actual DRM VRAM/GTT B | Browser-group PSS B |
| --- | ---: | ---: | ---: |
| Original factorization | 89,493,784/97,788,184 | 174,583,808/18,706,432 | 543,005,696 |
| Shared bounds | 90,753,304/99,047,704 | 182,980,608/18,706,432 | 558,830,592 |
| Predicted-motion weights (rejected) | 92,012,824/100,307,224 | 186,445,824/18,706,432 | 573,922,304 |
| Observed-motion support masks | 92,013,064/100,307,464 | 186,388,480/18,706,432 | 558,789,632 |

The selected two tables occupy 32×16×(2×width+height) bytes: 2,519,040 at 1080p.
The 16-record observed-motion table replaces a 16-byte inactive buffer, net
240 bytes. No additional RGB history or output feature map is allocated.
All requested totals include history, output, scratch, upload/uniform buffers,
inactive fixture resources, fallback and caches. The new work applies only to
explicit raw-factor; ordinary, raw-source and frozen learned paths retain their
previous allocation logic.

DRM/PSS are actual whole-client resident snapshots, including resident driver/
cache overhead, not per-asset physical attribution or transient physical peaks.
UMA pools may overlap and must not be added. No resident-memory compression is
demonstrated. The runtime gain costs extra storage.

After removing the rejected branches, final retained GPU median/p95 is
10.77568/12.12456 ms,
requested live/peak 92,013,064/100,307,464 bytes,
DRM VRAM/GTT 186,318,848/18,706,432 bytes,
and browser-group PSS 542,895,104 bytes. Final completion p95/p99 is
15.4966/16.8737 ms.

### Verification, failures and architecture decision

The final simplified implementation reproduces all 2,688 selected/conventional/
frozen-model image, reference and mask files exactly. Cut/reset matches a fresh
start for two frames at every scale; the established no-reset negative changes
pixels. Source/type checking passes. No broad unchanged test suite is repeated.
The initial span binding incorrectly applied an extra mutable marker; its compiler
failure and source are preserved. The correction changes no experiment semantics.

All experiment sources, rejected weight assumptions, raw observations, rendered
files, physical memory observations, timing samples and final sources are sealed.
The result supports retaining this execution strategy in the existing diagnostic.
It does not qualify the compact G4 appearance representation, which remains retired.

**Remaining gap:** complete GPU time still exceeds G6's 8.33 ms frame budget and
is far above its 1080p reconstruction allowance. No 20-minute tail/latency,
unseen-family, richness-scaling or 16× appearance-residency claim is established.
Exact image parity is measured at 192×108; cost is measured at 1920×1080.

**Next architecture bet:** share loaded observations and radiance accumulation
across output pixels with a cooperative tiled gather. Preserve exact coverage,
surface identity and observed motion; charge shared storage, history, fallback
and compositing. First compare an informed conventional implementation. A learned
residual needs a specific rendered error and a complete cost advantage to justify it.

Implementation sources: [axis support](../studio/reconstruct/edges/factor_queries.wrela),
[observed surface motion](../studio/reconstruct/edges/factor_surface_motion.wrela)
and [factorized gather](../studio/reconstruct/edges/coverage_radiance.wrela).
Existing evaluator and method citations above remain applicable. Original Wrela
code; no external code, weights or assets added.

Evidence: build/neural-evidence/factor-query-tables-v1 on Pop!_OS.
Manifest SHA-256: `2080ba5dd69ffe0d45e5b9bdd68cc35fe93ef41d3fab97ce807a155e6ba6c2c7`.
