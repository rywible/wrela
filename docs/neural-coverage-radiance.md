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
