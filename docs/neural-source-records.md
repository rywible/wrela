# Source records and shared visibility queries reduce conventional reconstruction cost

2026-10-29 UTC. Parent `9d6e3638afafacfe9c624c8144b886adc686d62a`.

**Decision: continue compact eight-byte colour/tag records with shared class-filtered
queries as an opt-in conventional diagnostic. Keep the equally informed full-record
control. Retire naive compact storage and the tag-only operating point.**

The final public-selector build takes **9.62540 ms** for a complete
1080p 4× nonlinear fixture frame, versus 12.66036 ms for the original
twenty-byte record path and 9.96280 ms for twenty-byte records with
the same shared masks. These are individual runs; alternating paired results below
provide the stronger timing comparison. Compact versus full-mask requested live
storage falls by **24,883,188 B** (27.04%).

All six original clips remain byte-exact, including shaded boundaries. A separate
cut test is **not entirely byte-exact**: nine colour components across three scale-3
frames differ by at most 0.000244140625 linear RGB. It is preserved and scored below.
No claim of universal numerical identity follows from the original clips.

First quality/runtime/residency assessment: **7.13 minutes**.
Architecture decision after exactly two optimization followups: **34.13 minutes**.
Final checks confirm the restricted retention decision. G1–G7 and eight frozen
artifacts are unchanged; no training or new reference algorithm.

## Source information and the fair control

The existing analytic source has two geometric classes, vertical slats with
row-invariant class membership, and constant foreground velocity within each causal
window. Observed alpha tags identify foreground and background. Depth is 0.5 or
0.25; vertical motion is zero. Horizontal motion is recovered from the declared
source speed using the same RGBA16F rounding and output-pixel conversion as the
original motion target.

A twenty-byte observation holds eight bytes of half colour/tag plus twelve bytes
of motion/depth. The compact record retains the eight colour/tag bytes and all
sixteen observation phases. The input colour, motion and depth textures remain
allocated and rendered; their costs are charged. This is source-specific geometry
compression, not neural compression or fewer colour samples.

The first optimization checks shared motion validity once per output layer and
uses observed tags for class rejection. It is applied equally to full and compact
records. The second retains one observed foreground bit per input column and
history slot, then filters shared horizontal query weights before colour loads.
Because class membership is row-invariant, nonzero weights certify membership.
Fallback tag checks remain. Both record layouts receive identical masks, queries,
motion predicates, light metadata and update work.

Logical mask content at 1080p 4× is 960 B. GPU binding alignment makes its actual
allocation **4,096 B**; that padded storage is counted. No target, future colour,
new source RGB sample or learned parameter enters decoding. The unrestricted
direct-source control receives authored appearance and charges its source queries;
its extra colour access is explicit and does not meet G1's sample reduction by
merely having lower error.

## Complete timing and residency

Solo Pop!_OS AMD RADV RENOIR PCI1002:164c / Ryzen5 5500U, Mesa26.1.6,
Chrome154.0.8037.57, Wrela0.9.0/compiler
`de9b81e52a7747870dfbd5223cb8aeb41e584d23`.
Each timing point is 1920×1080, 4×, one worker, 120 frames, zero warmup.
Case77 includes the declared illumination change at frame16; case1 is flat.
All GPU captures run sequentially. CPU scoring starts after that queue.

| Method | Case | Saturated complete median / p95 ms | Paced median / p95 / p99 ms |
|---|---|---|---|
| raw | 77 | 12.53432 / 13.90284 | 12.62262 / 13.67438 / 16.06946 |
| raw-flat | 1 | 11.17520 / 13.13740 | 11.22736 / 13.32142 / 15.68393 |
| cheap | 77 | 2.04908 / 3.21623 | 2.54930 / 3.31977 / 5.88199 |
| cheap-flat | 1 | 2.08156 / 3.26444 | 2.64783 / 3.23619 / 4.72406 |
| direct | 77 | 3.57576 / 5.18612 | 4.11673 / 5.12762 / 10.09768 |
| direct-flat | 1 | 1.81400 / 2.92524 | 1.87920 / 2.81040 / 6.36983 |
| source-records | 77 | 12.85628 / 13.58088 | 12.90180 / 13.57217 / 16.89411 |
| source-records-flat | 1 | 11.31560 / 13.59980 | 11.37818 / 13.13702 / 15.49170 |
| raw-tags | 77 | 12.60616 / 13.28956 | 12.64294 / 13.70362 / 16.22696 |
| raw-tags-flat | 1 | 11.22412 / 13.11436 | 11.29144 / 13.30460 / 15.38592 |
| source-records-tags | 77 | 12.27984 / 12.90408 | 12.29428 / 12.85849 / 15.60838 |
| source-records-tags-flat | 1 | 10.92688 / 12.28664 | 10.94662 / 12.80694 / 13.74825 |
| raw-masks | 77 | 9.94572 / 10.41752 | 10.10080 / 10.79198 / 13.26351 |
| raw-masks-flat | 1 | 8.70664 / 9.93976 | 8.95064 / 10.43601 / 12.50256 |
| source-records-masks | 77 | 9.61108 / 10.12268 | 9.62224 / 10.15063 / 12.40206 |
| source-records-masks-flat | 1 | 8.36740 / 9.60144 | 8.42794 / 9.63473 / 11.24974 |
| raw-layout0 | 77 | 12.66036 / 13.94544 | 12.55228 / 13.89512 / 15.82885 |
| raw-layout0-flat | 1 | 11.17480 / 13.13240 | 11.21166 / 13.44496 / 15.40822 |
| raw-layout1 | 77 | 9.96280 / 10.47296 | 9.99002 / 10.46942 / 13.42045 |
| raw-layout1-flat | 1 | 8.72740 / 9.97624 | 8.83492 / 10.43863 / 13.35944 |
| raw-layout2 | 77 | 9.62540 / 10.12228 | 9.69142 / 10.52675 / 12.45657 |
| raw-layout2-flat | 1 | 8.37900 / 9.63148 | 8.52336 / 10.24647 / 11.49562 |

The first sixteen points are development variants. The final six use public
selectors with the retained source. Saturated and paced distributions stay
separate. These cold runs are not sustained presentation guarantees. No thermal
or power lock is claimed. CPU frame-call timings are retained in JSON and are not
GPU completion measurements.

All fixture work counts: sample shading, colour packing, mask updates, motion
metadata, query preparation, sixteen-frame history, reconstruction, fallbacks and
display. This is a complete analytic fixture, **not** a production clearing frame
including wood, 3D sorting, general visibility and production compositing.

| Method | Requested live / peak B | Actual resident VRAM / GTT B | Browser PSS B |
|---|---|---|---|
| raw | 92,013,092 / 100,307,492 | 145,031,168 / 18,427,904 | 593,796,096 |
| raw-flat | 92,013,092 / 100,307,492 | 145,031,168 / 18,427,904 | 594,494,464 |
| cheap | 48,021,592 / 56,315,992 | 86,204,416 / 18,427,904 | 597,464,064 |
| cheap-flat | 48,021,592 / 56,315,992 | 95,125,504 / 18,427,904 | 597,360,640 |
| direct | 25,212,048 / 33,506,448 | 69,849,088 / 18,427,904 | 548,795,392 |
| direct-flat | 25,212,048 / 33,506,448 | 69,849,088 / 18,427,904 | 568,810,496 |
| source-records | 67,129,904 / 75,424,304 | 120,139,776 / 18,427,904 | 596,091,904 |
| source-records-flat | 67,129,904 / 75,424,304 | 120,139,776 / 18,427,904 | 552,525,824 |
| raw-tags | 92,013,092 / 100,307,492 | 145,031,168 / 18,427,904 | 595,217,408 |
| raw-tags-flat | 92,013,092 / 100,307,492 | 145,031,168 / 18,427,904 | 594,910,208 |
| source-records-tags | 67,129,904 / 75,424,304 | 120,139,776 / 18,427,904 | 596,758,528 |
| source-records-tags-flat | 67,129,904 / 75,424,304 | 120,139,776 / 18,427,904 | 601,206,784 |
| raw-masks | 92,017,188 / 100,311,588 | 145,047,552 / 18,427,904 | 597,164,032 |
| raw-masks-flat | 92,017,188 / 100,311,588 | 145,047,552 / 18,427,904 | 596,438,016 |
| source-records-masks | 67,134,000 / 75,428,400 | 120,074,240 / 18,427,904 | 600,554,496 |
| source-records-masks-flat | 67,134,000 / 75,428,400 | 120,139,776 / 18,427,904 | 600,726,528 |
| raw-layout0 | 92,013,096 / 100,307,496 | 145,031,168 / 18,427,904 | 601,168,896 |
| raw-layout0-flat | 92,013,096 / 100,307,496 | 145,031,168 / 18,427,904 | 554,917,888 |
| raw-layout1 | 92,017,188 / 100,311,588 | 145,047,552 / 18,427,904 | 601,534,464 |
| raw-layout1-flat | 92,017,188 / 100,311,588 | 145,047,552 / 18,427,904 | 601,650,176 |
| raw-layout2 | 67,134,000 / 75,428,400 | 120,139,776 / 18,427,904 | 601,269,248 |
| raw-layout2-flat | 67,134,000 / 75,428,400 | 120,139,776 / 18,427,904 | 555,199,488 |

Each allocation/residency result comes from a separate 300-frame replay of the same
compiled build. Requested accounting includes all live resources and scratch,
histories, masks, query tables, output, fallback and inactive common buffers.
The tiny default-path increase from newly present inactive buffers is disclosed.
Actual DRM snapshots are deduplicated by device/client; PSS covers the browser
process group. These whole-client observations are not per-asset physical peaks.
Do not sum UMA pools.

Three alternating pairs for each comparison separate the shared-query gain
(layout1 versus layout0) from compact storage (layout2 versus layout1).

- **layout1-vs-0**: baseline/candidate median across pairs 12.51420/9.94780 ms; changes -20.881%, -20.462%, -20.587%. Tool status: **faster**.

- **layout2-vs-1**: baseline/candidate median across pairs 9.95620/9.61596 ms; changes -3.476%, -3.154%, -3.448%. Tool status: **faster**.

- **layout1-vs-0-flat**: baseline/candidate median across pairs 11.19960/8.71700 ms; changes -21.963%, -22.167%, -22.581%. Tool status: **faster**.

- **layout2-vs-1-flat**: baseline/candidate median across pairs 8.70624/8.36636 ms; changes -3.731%, -4.198%, -3.798%. Tool status: **faster**.

These are small descriptive paired comparisons on this AMD device, not broad
hardware confidence bounds or a presentation-rate claim.

## Shaded quality, including boundaries and cuts

Eight experimental batches each contain cases1,23,74,75,76,77, four scales and
32 cold frames at192×108: **6,144 frames** total. Every corresponding output,
target and visibility mask is byte-exact to its prior control. The 192 case/scale
quality rows are therefore reused from the sealed
[radiance evolution evidence](neural-radiance-evolution.md), with provenance hashes;
LPIPS/FLIP were not needlessly rerun. Raw variants use the same rebased control.

Existing metrics use exposure1 Reinhard mapping, FLIP at67 pixels/degree,
LPIPS AlexNet0.1 and sRGB PSNR. Equal-case 4× means:

| Method | FLIP | LPIPS | PSNR dB | Opaque temporal residual | Mixed-boundary RGB MAE |
|---|---|---|---|---|---|
| raw | 0.116796 | 0.168024 | 23.203731 | 0.008370 | 0.031399 |
| cheap | 0.240314 | 0.460591 | 19.096829 | 0.010193 | 0.081968 |
| direct | 0.002261 | 0.000005 | 75.858187 | 0.000026 | 0.000113 |

The selected compact and full-mask paths have exactly the raw row on these clips.
Boundary RGB errors and observed coverage are reported; they do not certify
general 3D silhouette geometry.

Public-selector validation captures another 1,152 frames in six batches.
Normal cases74/77 contribute768 frames, all exact to the earlier raw control.
A declared cut from77 to0 at frame16 contributes128 fresh generic frames and
128 frames per specialized selector. Full-mask is exact; compact has125/128 exact
frames, with nine colour components different in frames23,25,31 at scale3.
Targets and visibility masks are exact throughout. The three changed frames
prevent blanket metric reuse, so both generic and compact cut clips were freshly
scored after the GPU queue. The error's arithmetic origin is not established.

| Cut scale | Baseline / compact FLIP | Absolute FLIP change | Baseline / compact LPIPS | Absolute LPIPS change |
|---|---|---|---|---|
| 1 | 0.065722595 / 0.065722595 | +0 | 0.036490615 / 0.036490615 | +0 |
| 2 | 0.160004886 / 0.160004886 | +0 | 0.173163011 / 0.173163011 | +0 |
| 3 | 0.151194775 / 0.151194770 | -4.19095159e-09 | 0.363204021 / 0.363204021 | +2.91038305e-10 |
| 4 | 0.193801298 / 0.193801298 | +0 | 0.325207674 / 0.325207674 | +0 |

Detailed temporal and transition metrics remain in both cut score files; no G3
qualification is inferred from this narrow cut test.

A separate GPU contract check compares recovered colour, motion and depth against
the actual current input textures: **5,667,840 records, zero maximum error in all
four channels**. It also leaves all768 diagnostic rendered frames unchanged.
A deliberate1%wrong motion factor fails, with maximum motion error0.002499998
over59,040 records. This validates the original six-clip geometry contract,
not every possible source, cut or dimensional configuration.

## Failed attempts and implementation checks

The initial compact path saved24,883,188 requested bytes but was slower:
12.85628 versus12.53432 ms on case77. Storage reduction alone was not accepted as
a rendering win. Followup1, the tag predicate, lowered compact cost to12.27984 ms
versus12.60616 for the equally simplified full record. Followup2, shared masks,
lowered both to9.61108/9.94572 ms before final selector confirmation. There is no
third optimization in this block.

Preserved failures include initial source statement syntax, obsolete tag-helper
arguments, an unaligned mask binding, and a geometry export incorrectly using an
image protocol that requires alpha1. The mask repair pads frame slices to256-byte
alignment; the geometry exporter uses the existing finite-record protocol.
Neither partial failed run supplies a rendering measurement. The cut exactness
failure and original verification script are also preserved.

Retained controls are explicit: --source-records0 keeps the original decoder,
1 enables full records with shared masks, and2 enables compact records with the
same masks. They require continuous analytic raw-factor captures; models, learned
gates, raw-model uploads and training exports are excluded. Default behavior stays0.
Source cuts must reset history; CLI and runtime reject stale cross-source history.
The selected geometry guard was temporary observation-only code and is archived,
not carried in timed builds. Formatting/type checking and the focused GPU/replay
checks validate the change; unchanged engine tests are not repeatedly audited.

## Remaining gap and next architecture bet

The compact path retains the raw method's4×FLIP0.116796 and LPIPS0.168024, with
about67.1MB requested live storage and roughly9–10ms complete cost. Direct source
is about3.58ms/25.2MB with FLIP0.002261 on these authored examples, but consumes
additional appearance information and queries. A meaningful conventional
improvement has been demonstrated; a world-leading upscaler, G4 compact appearance
win and production deployment have not.

The next bet is **source-compiled reconstruction for coherent surface tiles**:
reuse colour sums and filter weights across output pixels, retain all sixteen
temporal phases, and bound intermediate scratch by tile. The source code supplies
the conditions under which sharing is valid. The next consequential test must
include production visibility and an equally informed conventional control.
Dense intermediates, hidden extra colour queries or analytic-only speedups cannot
stand in for that comparison.

Evidence: build/neural-evidence/source-records-v1 on Pop!_OS. Sources for every
variant, executable builds, commands, captures, scores, timing/residency observations,
failed results, dependencies and orchestration scripts are pinned.
Manifest: `dac261d373a8672d6d12045413b1f7a2462e37628b88ab180d1e2ee17c6e7eef`.
