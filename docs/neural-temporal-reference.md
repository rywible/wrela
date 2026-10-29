# Reference-relative diagnosis redirects temporal work to source-routed decoding

2026-10-29 UTC. Parent `d2626b669b4b5d298e1e60f4347f2d820ef54a47`.

**Decision: keep the temporal shortcuts retired and change the next architecture
experiment.** Their local image changes include both improvements and real
boundary regressions. The existing source-separated conventional decoder reduces
error on the same affected boundary pixels by **95.88%**, but its measured
cost is too high to run everywhere. The next bet is selective expensive decoding
at source-known mixed-visibility boundaries, with a cheap interior path.

The first joint assessment, combining new reference-relative quality with the
pinned previous runtime/residency measurements, finishes in
**12.26 minutes**. The decision, after the existing
source-control comparison, is at **18.68 minutes**.
There are **zero optimization followups** in this diagnostic block: it replays
previously retired variants. No model fitting or new reference algorithm.
[G1–G7](neural-rendering-goals.md) remain unchanged and unmet.

## Resolve the actual uncertainty before changing representation

The [previous experiment](neural-temporal-identity.md) found small, mixed mean
analytic changes alongside large local clearing differences. Parent-image
agreement could reject an optimization but could not establish which image
was closer to source shading. This block uses existing targets to answer that
quality question, rather than adding another approximate shortcut.

Three native GPU capture invocations use the unchanged reconstruction tool on the
AMD Pop!_OS host: clearing shot 0, 192×108, fixed simulation time, 240 warmup frames,
phase64 sampler, seed 17. Each invocation builds once and captures 256/1024 samples,
an independent 1024-sample repeat, and all four temporal scales. Total invocation
wall time is 682.91 seconds, including compilation; this is not GPU frame time.
The isolated variants are the original identity and stationary-texel implementations,
copied from their preserved source archives and restored in finally.

The current baseline's three reference buffers and four temporal outputs are
**byte-identical** to completed run `1793058183-330807687`. Both candidate
replays also preserve all three reference buffers exactly. Source comparison
finds unchanged capture/view/temporal baseline logic; the active shading changes
are factored leaf/card functions and the retained conservative cloud support path.
The raw matches provide execution evidence beyond the source inspection.

That supports reusing completed run `1793059465-54690141` for this fixed
shot: 16,384-sample coarse, 65,536-sample fine, and independent 65,536-sample repeat.
Those high-sample buffers are hash-checked, **not rerendered in this block**.
Their whole-image coarse/fine and fine-repeat FLIP discrepancies are 0.002115
and 0.000730; the lower-half coarse/fine discrepancy remains 0.003121.
They integrate production shading, not independent light transport. No new
worst-region, motion or scene-population reference guarantee is inferred.

## Matched clearing quality

The fixed viewing transform and established FLIP/LPIPS/PSNR implementation are
reused unchanged. Values below use the 65k seed-17 reference.

| Method | Scale | Mean FLIP | LPIPS | Display PSNR dB | Changed-region pixels |
|---|---|---|---|---|---|
| baseline | 1 | 0.07329275 | 0.05454957 | 33.11025 | 0 |
| baseline | 2 | 0.09648763 | 0.15780549 | 28.90034 | 0 |
| baseline | 3 | 0.14225547 | 0.21669061 | 25.50779 | 0 |
| baseline | 4 | 0.18331479 | 0.32078129 | 22.50369 | 0 |
| identity | 1 | 0.07324273 | 0.05438436 | 33.11283 | 28 |
| identity | 2 | 0.09642785 | 0.15758242 | 28.90647 | 40 |
| identity | 3 | 0.14230070 | 0.21715757 | 25.50869 | 27 |
| identity | 4 | 0.18326764 | 0.32094866 | 22.50266 | 26 |
| stationary-texel | 1 | 0.07329599 | 0.05458139 | 33.10860 | 10 |
| stationary-texel | 2 | 0.09648538 | 0.15776645 | 28.89911 | 16 |
| stationary-texel | 3 | 0.14225928 | 0.21658324 | 25.50783 | 4 |
| stationary-texel | 4 | 0.18330483 | 0.32092035 | 22.50367 | 8 |

For each shortcut and each scale, the sign of its FLIP change is the same against
all three existing reference estimates. Identity slightly improves FLIP at
1×/2×/4× and worsens it at 3×; stationary-texel improves it at 2×/4× and worsens
it at 1×/3×. Other metrics disagree at some operating points. These tiny changes
are not a confidence-qualified quality win.

The changed-region mask is defined by maximum RGB difference above 2/255 in the
named displayed-linear view. It is a **post-hoc diagnostic**, not a predeclared
detail ROI or semantic silhouette acceptance test. In the clearing it contains
28/40/27/26 pixels for identity and 10/16/4/8 for stationary-texel. The largest
named-view differences reach 0.166748 and 0.029636 respectively. At 4×, identity's
affected-region mean absolute error rises from 0.035583 to 0.037965 despite its
slightly lower whole-image FLIP.

This fixed 192×108 shot does not directly resolve every changed pixel in the
previous dynamic 1080p clearing capture. It establishes that local source-relative
improvements and regressions coexist, without treating the parent as truth.

## The moving-boundary tradeoff hidden by global averages

No new analytic captures are needed. Reuse the six cases, four scales and 32 cold
frames from the preceding block, with byte-identical independent truth and
visibility masks. For each shortcut, inspect pixels above the same 2/255
disagreement threshold. The boundary subset includes fractional foreground
coverage or an adjacent coverage change. Counts below are pixel-frames across
the complete set, not independent statistical samples.

| Shortcut | Affected pixel-frames | Affected MAE baseline / candidate | Affected boundary pixel-frames | Boundary MAE baseline / candidate | Boundary change |
|---|---|---|---|---|---|
| identity | 9891 | 0.20077062 / 0.19092570 | 4294 | 0.27226601 / 0.28604795 | +5.062% |
| stationary-texel | 7973 | 0.21244065 / 0.20715446 | 3465 | 0.30263531 / 0.31591222 | +4.387% |

Identity improves 5,181 affected pixel-frames and worsens 4,710; stationary-texel
improves 4,156 and worsens 3,817. The pooled affected-region mean improves, while
the affected-boundary mean worsens. Every per-case/scale row is retained; this
post-hoc pooled diagnostic does not replace the charter's equal-category gates.

The strongest changes concentrate in case 23: source slats move **32 output
pixels per frame**. At scale 4, pixel (2,43), frame 6, identity removes a
baseline mean absolute error of 0.471292 on a fully revealed background pixel.
At frame 23, the same pixel has foreground coverage 0.3046875 and identity
increases error from 0.050675 to 0.402048. This is a concrete example of helping
a reveal while harming a mixed-coverage pixel.

The [production temporal path](../engine/temporal.wrela) stores one colour and
material tag per history pixel; deformation acceptance also reads the previous
tag. These facts motivate preserving separate surface contributions. They do not
by themselves isolate the numerical operation responsible for each failure.

## Existing source-aware controls change the next experiment

Re-score the same affected mask from identity against existing captures in
[the source-chart study](neural-source-chart.md), first requiring identical truth
and visibility buffers. This is a new regional comparison of old measurements.

| Existing path | Whole-image MAE | Affected-region MAE | Affected-boundary MAE |
|---|---|---|---|
| production | 0.10038745 | 0.20077062 | 0.27226601 |
| identity | 0.10038122 | 0.19092570 | 0.28604795 |
| raw-factor | 0.03998043 | 0.01301397 | 0.01120557 |
| fastsource-history | 0.08929414 | 0.07620303 | 0.09447826 |
| source-direct | 0.00006106 | 0.00002074 | 0.00003421 |

The raw-factor decoder keeps source-separated colour observations and combines
them using source-derived coverage. On the 4,294 affected boundary pixel-frames,
its MAE is 0.011206 versus production's 0.272266. The cheap source-aware history
path reaches 0.094478; it leaves a substantial gap. The unrestricted direct-source
control is almost exact, but it evaluates authored radiance and its source-query
cost still counts. It cannot waive the sixteen-times-fewer-colour-samples goal
or serve as a fixed-input upscaler comparison.

The same earlier study measured complete 1080p/4× analytic-fixture GPU medians
of 10.79672 / 17.88788 ms for raw-factor on flat / nonlinear-leaf cases, versus
2.09980 / 1.78524 ms for the cheap source-history path and 1.79612 / 2.25320 ms
for direct source. Requested live storage was 92,013,116 / 48,021,616 /
25,212,072 B respectively; actual whole-client residency is in the pinned study.
These are **earlier measurements on different fixtures**, not fresh timings or
a speed ratio against the full clearing below. They identify the quality/cost
tradeoff worth addressing; selective routing has not been measured yet.

## Runtime and memory still rule out promotion

The prior full-render observations for the exact replayed temporal sources are:

| Earlier full-clearing path | Complete median ms | Requested live B | Actual resident VRAM / GTT B |
|---|---|---|---|
| baseline | 26.14876 | 405,267,256 | 551,936,000 / 20,099,072 |
| identity | 25.93984 | 405,267,256 | 551,936,000 / 20,361,216 |
| stationary-texel | 25.72540 | 405,267,256 | 551,981,056 / 20,361,216 |
| exact-texel | 26.42412 | 405,267,256 | 551,907,328 / 20,099,072 |

All use the same 405,267,256 live requested bytes; histories, scratch, caches,
fallbacks, wood, visibility, updates and compositing are included. Prior PSS and
transient peaks remain in the pinned evidence. The exact-texel variant preserved
analytic quality but was slower; its retirement is unchanged and it is not
recaptured here. No fresh runtime, memory saving, paired speedup or presentation
claim comes from this quality-only diagnostic. The small unpaired timing
improvements cannot compensate for the demonstrated local quality tradeoff.

## Next architecture test and evidence

Use authored geometry to identify mixed-visibility boundaries at runtime. Route
those pixels to source-separated raw-factor decoding and use the cheap observed-
radiance path in interiors. Compare against both complete conventional paths with
the same useful source information. Retain direct source as the explicitly
unrestricted control. **Never route on these disagreement masks, reference
images, or reference-derived errors.**

Count every history buffer, feature pass, routing pass, source query, update and
compositing step; sharing must be real, not assumed. Dense source charts and fixed
small surface caches already failed earlier experiments, so this tests allocation
of decoder work, not another dense chart. Start with existing batched cases and
require joint quality, runtime and residency before promotion. This remains an
untested architecture hypothesis.

Pop!_OS evidence: `build/neural-evidence/temporal-reference-v1`.
Manifest: `e155ca3621d4b6d03853ee4af64dcbee4318e0f4fdc259e85ceeb90506fe8949`.
It pins three new captures, the earlier high/low reference files, source archives,
region measurements, scripts and dependencies. Frozen files remain unchanged.
Runtime is restored exactly to parent; inherited benchmark edits remain unstaged.
No unchanged engine tests, additional training or reference framework were added.
