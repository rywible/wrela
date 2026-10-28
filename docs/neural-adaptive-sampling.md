# Source-guided sampling: retire this scattered-sample path

**Decision: retire.** Geometry-balanced sampling loses aggregate spatial and
motion quality, takes 52.1% more complete GPU time, and saves no requested memory
against the informed regular sampler. The uniform scattered control also loses.
There is no new neural or rendering advantage. G1–G7 and the frozen models are
unchanged; no training ran.

The first complete comparison took 7.95 minutes.
The decision followed at 20.46 minutes
(2026-10-28T06:03:21.000267+00:00), within the two-hour limit. Zero follow-up optimizations.
Existing analytic references and cases were reused; the capture tool repeated
those same targets and masks, which matched exactly. No new reference method.

## What changed and what each method knows

This follows the [retired clump visibility representation](neural-shaded-appearance.md#clump-visibility-tiles-exact-silhouettes-insufficient-surface-information).
It is a G1 full-stack sampling diagnostic, not another G4 appearance result.
All methods retain one colour observation per low-resolution cell per frame.
At 4x linear reduction this is 1/16 of the output colour-sample count.
The regular control uses the established 16-frame jitter schedule and fixed tent.

The new uniform sampler generates four jittered candidates, one in each 2x2
subcell, and selects one. The balanced sampler queries the source geometry at
those same four positions. If both surface classes occur, it chooses foreground
or background with equal probability, then uniformly chooses within that class.
It stores a relative importance weight of class count/2; single-class cells use
weight one. Only the selected position evaluates source RGB. Selection sees
slat period, width, offset and speed, never colours, checker frequency, the
integrated reference, reference coverage or future samples. Conventional methods
receive the same geometry/visibility information; there is no learned candidate.

The existing 20-byte record holds half RGB, half importance, f32 x/y position and
f32 depth. The former motion slots now mean actual sample position, and the
former semantic-alpha lane means importance. The custom gather infers the two
known layer velocities from depth and the supplied speed. Diagnostic filenames
and old field labels still say motion; this semantic change is explicit in the
archived metadata. It is restricted to the existing two-layer analytic fixture.

Both scattered variants use the same gather: two layers, up to sixteen causal
frames, conservative cell bounds for actual positions, current source visibility,
positive tent weights times importance, normalization, and nearest current-record
fallback. The radius rule is unchanged. At 1x the sampler returns the regular
position with weight one. All 256 candidate 1x RGBA frames match the control.
The selector is recomputed by colour, motion and retention passes; all costs are
included. No additional image or history allocation is hidden.

The discrete four-candidate importance algebra is correct, but that does not
make the normalized temporal filter an unbiased pixel integral. Nor does knowing
the source class guarantee that this random schedule retains useful samples.

## Quality on the existing clips

Cases 1, 6, 13 and 23, 32 cold-start frames each, 192x108 output, all four scales.
The following table averages the four clip aggregates at 4x. PSNR is the mean of
clip PSNR values; it is not recomputed from pooled MSE. These means are diagnostic,
not G1's median/tail acceptance statistics.

| Method | Mean FLIP | Mean LPIPS | Mean clip PSNR dB | Opaque temporal MAE | Newly exposed error |
| --- | ---: | ---: | ---: | ---: | ---: |
| Regular fixed filter | 0.24324395 | 0.25324590 | 16.369425 | 0.02631498 | 0.09197359 |
| Uniform scattered | 0.26362102 | 0.31779534 | 15.505457 | 0.03571487 | 0.10341624 |
| Geometry-balanced scattered | 0.26540504 | 0.31781175 | 15.555711 | 0.03682783 | 0.10694341 |

Balanced versus regular: FLIP +9.1%, LPIPS +25.5%, mean clip PSNR -0.814 dB,
opaque correspondence error +40.0%, and newly exposed error +16.3%.
Every case's mean FLIP worsens at 4x. There are exceptions in other metrics:
case 1 LPIPS improves from 0.197783 to 0.188849, and the mean of each clip's worst
frame FLIP improves from 0.507462 to 0.454179. These do not outweigh the joint
quality, runtime and memory result; this is not a claim of regression everywhere.

| Scale | Regular FLIP | Uniform FLIP | Balanced FLIP |
| --- | ---: | ---: | ---: |
| 1x | 0.09620063 | 0.09620063 | 0.09620063 |
| 2x | 0.13385482 | 0.14648063 | 0.14778765 |
| 3x | 0.19165672 | 0.20712854 | 0.20949058 |
| 4x | 0.24324395 | 0.26362102 | 0.26540504 |

The established evaluator keeps exposure one, x/(1+x), the sRGB view, LDR FLIP
at 67 ppd and AlexNet LPIPS v0.1. Temporal error subtracts reference error before
comparing valid opaque correspondences. Reveals and unmatched/mixed samples stay
separate. No semantic-alpha value is called geometric coverage: this diagnostic
does not measure G3 silhouette precision/trails or establish G4 shaded quality.
All raw images, frame scores and masks are preserved, including failed cases.

## Complete AMD cost and memory

AMD RADV RENOIR PCI 1002:164c, Ryzen 5 5500U, Mesa 26.1.6,
Chrome 154.0.8037.57, Pop!_OS. Wrela 0.9.0/compiler
`de9b81e52a7747870dfbd5223cb8aeb41e584d23`; source parent `b8e8a036`
plus exact per-variant archives. Solo, sequential AMD runs; no simultaneous
training or CPU scoring. Timing uses case 1 at 4x and 1920x1080 output.

| Method | Complete GPU median/p95 ms | CPU call p99 ms | CPU start-to-completion p95/p99 ms |
| --- | ---: | ---: | ---: |
| Regular fixed filter | 21.97056/22.26636 | 0.6307 | 94.0165/94.7492 |
| Uniform scattered | 31.57704/31.72848 | 0.6772 | 132.2184/133.4341 |
| Geometry-balanced scattered | 33.42048/33.57992 | 0.5611 | 139.3224/140.3150 |

Saturated GPU spans use 32 warm-up plus 240 measured frames. Completion has a
separate 300-frame run, discarding 60 warm-up frames; it includes browser/driver
notification and in-flight queue time. These are single-run measurements, not
repeatability or sustained G6 certification. Even the regular control fails the
8.33 ms GPU budget before adding a network.

Complete fixture spans include guide queries and selection, colour/depth/motion,
retention, neighbourhood work, all history gathering, updates, display and
compositing. Each uses four draws, two dispatches and 1,296 upload bytes/frame.
This analytic fixture has no wood geometry or primitive sorting; the preceding
G4 comparisons charged those costs separately. No isolated decoder time is used
as a whole-engine rendering claim.

The temporal-control pass takes 20.829/30.474/32.064 ms for regular/uniform/
balanced; it accounts for approximately 95–97% of the complete spans. Extra
geometry selection is not the only problem: uniform scattering already raises
cost by 43.7%, with worse quality. A new model cannot fix that measured cost by
making only its own arithmetic cheaper.

| Method | Requested live/peak bytes | DRM VRAM/GTT resident bytes | Browser-group PSS bytes |
| --- | ---: | ---: | ---: |
| Regular fixed filter | 89,493,544/97,787,944 | 165,724,160/18,706,432 | 575,934,464 |
| Uniform scattered | 89,493,544/97,787,944 | 165,650,432/18,706,432 | 573,566,976 |
| Geometry-balanced scattered | 89,493,544/97,787,944 | 174,571,520/18,804,736 | 556,996,608 |

All-requested totals count the 20-byte raw history, all scratch, inactive fixture
resources, fallback inputs, screen/output, upload and uniform buffers. Live and
peak requested allocation are identical across methods; all-method current
resource accounting reports 80,871,464 bytes before the wider observer's extras.
DRM and PSS are actual whole-client resident snapshots, including browser/driver
state. They are neither asset-attributed appearance residency nor physical
transient peaks, and overlapping UMA pools must not be added. There is no
measured 16x memory saving or G4 claim.

## Verification, retirement and the next bet

The regular control reproduces 1,536 prior image/reference/mask files exactly.
All candidate references/masks match. Independent CPU checks cover
419,840 selected records, exact importance and source depth, and
8,192 reconstructed pixels across both candidates and four scales.
The exhaustive replay does not reuse shader search bounds; its worst named-view
error is 0.00024147. Wrong-position and removed-importance
negative controls fail where expected. Sixteen enumerated class masks verify the
discrete importance identity. Established reference/reset machinery was reused.

The first source-colour audit incorrectly assumed round-to-nearest half conversion.
Actual GPU source values differ by one adjacent half code for some constants.
That failed script and log are preserved. The repaired check permits one code
for this source conversion only; reconstruction replay uses actual captured RGB.
No rendered metric, model or goal threshold changed.

All experimental source variants, raw clips, diagnostic probes, full builds,
allocation observations and the failed audit are archived. The runtime is
restored byte-for-byte to the parent; the new module is removed. No broad suite
was rerun for unchanged restored code. No second sampling sweep or fit is needed
to decide this implementation is uncompetitive. This does not rule out all
adaptive sampling or all temporally structured schedules.

**Next architecture bet: a persistent cache in surface coordinates.** Keep the
regular sampler fixed. Insert only new raw observations into bounded surface
cells, retaining actual positions, age and surface identity; reconstruct from
small local support. Count collisions, expiry, visibility, insertion, fallback
and every retained byte. Compare the conventional version first, on these same
clips and references, with equal source information. This differs from the
previous current-output index, which rebuilt all sixteen frames every tick.
The experiment must prove a useful quality/runtime/memory tradeoff before any
network is trained to route or weight that cache. It is a hypothesis, not a win.

Existing method citations and frozen model provenance are in
[the reconstruction note](neural-kernel-reconstruction.md#primary-sources).
This sampler and explicit-position gather are original experimental code; no
external code, pretrained weights or new reference assets were imported.

Evidence on Pop!_OS: `build/neural-evidence/adaptive-sampling-v1`.
Manifest SHA-256: `0647b783a3aa74993eaef0a3d50b3ad2170b6270340535bc8fbf9ebb041e6855`.
