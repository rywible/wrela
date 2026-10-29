# Dense source-coordinate radiance charts do not earn their cost

2026-10-29 UTC. Parent `a829ee617639aba33dfa86d3f90fee45e1b0e7b4`.

**Decision: retire this representation for this workload.** Against the strong
surface-separated raw-history control, the chart lowers GPU cost but raises
4x FLIP error by 25.34%, temporal error by 139.86%, and requested live storage by
24.49%. It loses 2.15 dB mean clip PSNR. Direct source integration is faster,
smaller and substantially more accurate in this source-accessible diagnostic.

The first complete quality/runtime/residency assessment finished in
**30.91 minutes**, including implementation.
Decision at **33.13 minutes**. No follow-up optimization:
reducing storage would not resolve the rendered-quality deficit, and there is no
joint advantage against the strongest practical source control.

All candidate source, captures, compiled profiles and failed measurements remain.
Tracked runtime is restored exactly to the parent. [G1–G7](neural-rendering-goals.md)
and all frozen models are unchanged. This is a reconstruction experiment, not a
new G4 result or a production rendering win.

## Representation and source information

The candidate accumulates actual rendered RGB into separate foreground and
background charts in stable source coordinates. Both charts are RGBA16F and
double buffered. Each texel stores weighted RGB sum and mass. A four-cell gather
accepts current observations from the same surface using an output-space tent
of radius 0.8 pixels; prior sum and mass decay by 0.9 each frame.

The moving foreground uses an integer-coordinate ring, padded by divisor+1
pixels on each side. Reused slots reject world coordinates outside the previous
window. Previous chart reads use the identical source texel, avoiding repeated
history resampling. Only final foreground composition interpolates neighbouring
source texels. Background history stays fixed in world coordinates. Geometry-only
coverage composites the two normalized radiances. Missing mass searches the
current 3x3 input neighbourhood for the same surface, then uses the declared
nearest-current fallback if none exists. Reset clears history.

This is a dense radiance sum, unlike the previously retired
[fixed-slot sample cache](neural-surface-cache.md). It does not retain exact sample
positions, separate lighting states or material subregions within a surface.
The dynamic-light case exposes the consequence of accumulating shaded radiance.

The chart, fast history and raw-factor receive identical geometric period,
width, offset and speed, one original RGB sample per low-resolution cell, and
the renderer's depth/motion information. No truth image, future frame, source RGB
or checker frequency is fed to their reconstruction. Raw-factor retains sixteen
frames; the chart has causal exponential history without that hard window.
They see the same incoming stream but do not retain identical observations.

The strongest unrestricted conventional control receives the full authored
material regions and shading formula and evaluates visible-area centroids.
Those source facts are available in Wrela; the chart's choice not to consume them
cannot justify omitting that baseline. Its additional source evaluations are
charged to complete runtime. Direct integration is an approximation to the
independently integrated reference, not the reference implementation itself.

## Quality, edges and lighting changes

Six public development cases: 1, 6, 13, 23, 74 and 77. Each has 32 cold-start frames
at 192x108 and all four reduction factors. The last two use production leaf shading;
77 includes the existing illumination drop to 35% at frame 16. This is an analytic
two-layer diagnostic with horizontal motion, not arbitrary 3D foliage.

The existing evaluator is unchanged: exposure one, x/(1+x), sRGB, LDR FLIP at
67 pixels/degree and AlexNet LPIPS v0.1. Temporal error subtracts the reference
error before opaque-surface correspondence; reveal errors remain separately
recorded. Every paired frame has identical reference and visibility hashes.

Values below average the six clip aggregates. PSNR is mean clip PSNR, not pooled
MSE; the nearly exact flat cases make the direct-source average particularly high.
Boundary error is RGB MAE in the named linear view on the existing mixed-coverage
mask. A one-pixel-expanded boundary mask, cold-frame and later-frame metrics are
also recorded. Final alpha is opaque; none of these certify G3 silhouettes.

| Method | 4x FLIP | LPIPS | Mean clip PSNR dB | Opaque temporal MAE | Boundary RGB MAE |
|---|---:|---:|---:|---:|---:|
| Dense source chart | 0.21352427 | 0.27271014 | 17.47500 | 0.03267479 | 0.07140083 |
| Fast source-aware history | 0.31920107 | 0.58548077 | 16.12856 | 0.01618023 | 0.11375316 |
| Surface-separated raw history | 0.17036091 | 0.26061900 | 19.62543 | 0.01362244 | 0.04458283 |
| Direct source integration | 0.00171129 | 0.00000340 | 82.76788 | 0.00001351 | 0.00008078 |

| Scale | Dense chart FLIP | Fast history FLIP | Raw history FLIP | Direct source FLIP |
|---|---:|---:|---:|---:|
| 1x | 0.07555068 | 0.20004899 | 0.07408727 | 0.00171129 |
| 2x | 0.11151523 | 0.25517424 | 0.10312453 | 0.00171129 |
| 3x | 0.16907628 | 0.29483389 | 0.13961758 | 0.00171129 |
| 4x | 0.21352427 | 0.31920107 | 0.17036091 | 0.00171129 |

The chart improves spatial quality over fast history but costs more and has
worse temporal error. Raw history preserves more useful observations: at 4x,
chart boundary error is about 60% higher. The chart's sparse support and fallback
also leave cold-start error: mean FLIP 0.39543 versus raw history 0.35606.

On case 77 at 4x, chart FLIP changes from 0.15873 at frame 15 to 0.48284 at
frame 16 and is still 0.20470 at frame 31. Direct source is 0.00276 at the light
change and 0.00273 at frame 31. The source-coordinate chart avoids geometric
history resampling but still preserves obsolete lighting. At 1x, the same event
raises chart FLIP from 0.03529 to 0.49317, so sparse sampling alone cannot explain
that failure. No extra fitting or material-moment training was undertaken.

## Complete AMD runtime and residency

Pop!_OS, Ryzen 5 5500U, AMD RADV RENOIR PCI 1002:164c, Mesa 26.1.6,
Chrome 154.0.8037.57; Wrela 0.9.0, compiler
`de9b81e52a7747870dfbd5223cb8aeb41e584d23`.

Timing is 1920x1080 at 4x reduction, one worker, 120 frames after 60 warm-up.
Both flat case 1 and nonlinear leaf case 74 are timed. The initial flat-only
profile was insufficient to judge a shading cache; timing the existing nonlinear
case resolved that uncertainty without expanding references.

| Method | Case | Saturated complete GPU median / p95 ms | Paced GPU median / p95 / p99 ms | CPU call median / p95 ms |
|---|---:|---:|---:|---:|
| Dense source chart | 1 | 4.97408 / 5.31792 | 5.27550 / 5.56232 / 5.76544 | 0.33000 / 0.46500 |
| Fast source-aware history | 1 | 2.09980 / 2.34744 | 2.69330 / 2.91200 / 2.96950 | 0.42000 / 0.55500 |
| Surface-separated raw history | 1 | 10.79672 / 12.24508 | 11.09970 / 12.50169 / 13.36157 | 0.39500 / 0.56500 |
| Direct source integration | 1 | 1.79612 / 1.91176 | 2.45113 / 2.75905 / 2.80841 | 0.37500 / 0.44000 |
| Dense source chart | 74 | 4.68804 / 4.97712 | 5.35425 / 5.58780 / 6.06081 | 0.43500 / 0.57500 |
| Fast source-aware history | 74 | 1.78524 / 2.07484 | 1.99494 / 2.24656 / 2.35413 | 0.41500 / 0.58500 |
| Surface-separated raw history | 74 | 17.88788 / 22.60352 | 17.93494 / 22.63473 / 22.79733 | 0.43000 / 0.56500 |
| Direct source integration | 74 | 2.25320 / 2.30296 | 2.87992 / 3.31838 / 3.43705 | 0.37000 / 0.43500 |

Complete span includes original sampling, depth/motion work, history updates,
all decoding/fallback work, coverage and output composition, and display.
The chart's flat-case update alone costs 2.45360 ms; composition costs 1.67272 ms.
This fixture has no wood or primitive sorting. These numbers must not replace
the completed full-foliage G4 measurements, which count those stages.

The tool produces two runs: saturated GPU summaries in `profile.json`,
and a paced replay whose timestamps are retained in
`profile-build/results/timings.json`. Their distributions are reported separately;
no paced p99 is presented as part of the saturated distribution.

These are sequential, unpaired development measurements, with no enforced
thermal/power lock. No universal speedup or sustained G6 claim. CPU calls do not
measure GPU completion or presentation. Cold initialization occurs before the
steady timed window; cut correctness and cut-time tails are not qualified for
the retired chart.

Each compiled executable is reused for a separate 300-frame allocation run.
The table includes requested allocations and deduplicated actual DRM residency
snapshots, plus browser-process-group PSS.

| Method | Case | Requested live / peak bytes | Resident VRAM / GTT bytes | Browser PSS bytes |
|---|---:|---:|---:|---:|
| Dense source chart | 1 | 114,549,584 / 122,843,984 | 164,511,744 / 18,427,904 | 595,096,576 |
| Fast source-aware history | 1 | 48,021,616 / 56,316,016 | 95,125,504 / 18,427,904 | 549,952,512 |
| Surface-separated raw history | 1 | 92,013,116 / 100,307,516 | 145,031,168 / 18,427,904 | 549,509,120 |
| Direct source integration | 1 | 25,212,072 / 33,506,472 | 69,849,088 / 18,427,904 | 596,107,264 |
| Dense source chart | 74 | 114,549,584 / 122,843,984 | 164,511,744 / 18,427,904 | 595,924,992 |
| Fast source-aware history | 74 | 48,021,616 / 56,316,016 | 95,125,504 / 18,427,904 | 588,408,832 |
| Surface-separated raw history | 74 | 92,013,116 / 100,307,516 | 145,031,168 / 18,427,904 | 573,957,120 |
| Direct source integration | 74 | 25,212,072 / 33,506,472 | 60,928,000 / 18,427,904 | 594,403,328 |

All caches, scratch, fallback inputs, inactive fixture resources, output and
upload allocations are counted. Four common dummy chart textures were present
in every method in this experimental build. Physical snapshots include driver
and browser state; they are neither per-representation physical peaks nor
proof of a 16x appearance reduction. Overlapping UMA pools must not be summed.

## Batching, verification and preserved evidence

Four capture executable runs produced **3,072 reconstructed frames** and their
paired references/masks in **97.89 seconds** total, excluding timing,
allocation and CPU scoring. Each build serves six cases and four scales with
fresh state per case/scale. This reports throughput; it is not a controlled
speedup claim against the prior unbatched path.

Eight complete profiles and eight residency captures ran in one AMD queue.
CPU quality scoring started only after that queue finished. The changed module
passed formatting/type checks and all captures/profiles executed. Existing
metric controls and reference-hash checks were reused. No broad unchanged test
suite, new reference algorithm, neural training or subagent was used.

Evidence on Pop!_OS: `build/neural-evidence/source-chart-v1`.
The manifest pins the initial source archive, compiled shaders, command logs,
all 24 external capture directories, metric code, allocation observer, dependency
manifests and eight frozen artifacts. Runtime restoration is checked directly
against the parent. Failed results remain reproducible.

Manifest: `e33978e38675f190552e95e8246eb7034ac271a458af3be3a0ba5af8245a08ac`.

## Next architecture bet

**Use source-derived interval certificates to skip runs of scheduled cloud view
samples.** The retained [point-support result](neural-cloud-support.md) already
shows that 47.159% of its actual view queries are certifiably empty. First measure
whether those empty queries form useful contiguous runs and whether repeated
bound checks cost enough to justify interval traversal. Any candidate must keep
the original sample positions/integration and charge certificate construction,
updates, traversal and fallback data.

This follows the measured source-support advantage rather than investing further
in dense shaded-RGB history. The [sunlight reuse failure](neural-cloud-light-support.md)
also remains relevant: low selectivity can make an otherwise valid certificate
unprofitable. No interval speedup is claimed before the comparison.
