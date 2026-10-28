# Persistent surface cache: retire fixed slots, retain shared jitter preparation

**Decision: retire the oldest-first fixed-slot surface cache.** Preparing the
sixteen existing jitter phases is retained in the conventional fixed-filter
source decoder. It preserves rendered output and reduces observed complete GPU
time from 21.96008 to 18.49648 ms. That is a conventional component improvement,
not a neural rendering win or a G1–G7 pass.

First complete comparison: 7.47 minutes.
Decision: 30.86 minutes (2026-10-28T06:47:29.107721+00:00). Exactly two
follow-ups: two→four cache slots, then shared jitter preparation on both the
cache and the informed regular control. No training, material-moment fitting,
reference method expansion or new cases. Frozen models and goals are unchanged.

## Representation and equal source information

The regular sample locations and one colour observation per low-resolution cell
per frame stay fixed. The cache retains those actual observations in two source
layers. Each surface cell is two output pixels wide (one at divisor one), with
two or four slots. Its 16-byte record contains half RGBA, the original low-pixel
index and the absolute frame stamp plus one; zero means empty. Position is
reconstructed from that index, the original jitter and the known source motion.
Colours are never repeatedly resampled or inferred from source appearance.

The horizontal cache is a ring wider than the viewport, with two cells of padding.
Each layer has a separate grid. New observations replace the oldest slot in their
cell. Only current observations are inserted; the previous sixteen frames are not
reindexed. Stored age rejects entries older than the sixteen-frame window; cuts
clear the state. The gather searches nearby surface cells, reconstructs actual
positions, tests current source visibility and applies the same normalized tent
and nearest-current fallback. World-coordinate aliases cannot contribute unless
the reconstructed position lies in support.

Input points are spaced at least one bin apart, and the ring exceeds their span.
Thus one invocation writes each layer/cell in a frame; atomics provide indexed
storage, not a lock-free multi-writer protocol. Independent replay confirms this
for every observation in the audited clips. This invariant would need a different
insertion scheme for general projected 3D samples; no general-purpose cache claim.

All methods receive the same slat period, width, offset and speed. No source RGB,
checker frequency, reference colour, integrated reference coverage or future
frame reaches reconstruction. The cache uses the exact supplied velocity; the
old control also receives it but retains quantized motion from the renderer.
At 3x these can differ slightly; at the measured 4x the fixture velocities are
exactly representable. This is a full-stack source-aware diagnostic, not a fixed-
input vendor tournament. Four public clips do not constitute sealed validation.

## Rendered quality and retained information

Cases 1, 6, 13 and 23; 32 cold-start frames; 192x108 output; all four scales.
Means average four clip aggregates. PSNR is the mean of clip PSNR, not pooled MSE.
The unchanged evaluator uses exposure one, x/(1+x), sRGB, LDR FLIP at 67 ppd,
AlexNet LPIPS v0.1 and reference-subtracted opaque correspondence error. Reveals
remain separate. Semantic alpha is not treated as geometric silhouette coverage.

| Method | 4x mean FLIP | LPIPS | Mean clip PSNR dB | Opaque temporal MAE |
| --- | ---: | ---: | ---: | ---: |
| Regular | 0.24324395 | 0.25324590 | 16.369425 | 0.02631498 |
| Cache, two slots | 0.26806494 | 0.27641784 | 15.424325 | 0.04426342 |
| Cache, four slots | 0.24439923 | 0.25562920 | 16.331590 | 0.02778977 |
| Regular + jitter table | 0.24324395 | 0.25324590 | 16.369425 | 0.02631498 |
| Cache, four slots + table | 0.24439923 | 0.25562920 | 16.331590 | 0.02778977 |

The two-slot cache raises 4x FLIP by 10.20% and opaque temporal error by 68.21%.
In case 1 it retains 13,014 of 20,736 possible live observations at the final
frame, and evicts 25,218 still-live records across the clip. Four slots retain
20,196 and reduce those live evictions to 3,078. Case 23 similarly improves from
13,068 to 20,277 retained observations. This explains why testing capacity was
consequential; it is not proof that all missing observations would be useful.

Four slots recover most 4x spatial quality, but still raise FLIP 0.475%, LPIPS
0.941% and temporal error 5.60% against the equally prepared regular control.
Lower scales regress more severely:

| Scale | Regular/table FLIP | Two-slot FLIP | Four-slot/table FLIP |
| --- | ---: | ---: | ---: |
| 1x | 0.09620063 | 0.12838844 | 0.11015668 |
| 2x | 0.13385482 | 0.23457110 | 0.18376364 |
| 3x | 0.19165672 | 0.26949380 | 0.21340849 |
| 4x | 0.24324395 | 0.26806494 | 0.24439923 |

At 2x, four-slot FLIP is 37.28% worse. Capacity is fixed per output-space area,
so it discards a much greater fraction of the denser observations at lower
reduction factors. Source identity alone does not preserve sufficient sampling
information. No G1/G3 silhouette, trail, reveal-time or human-test pass is inferred.
G4's compact appearance model is unchanged and remains unqualified.

## Complete AMD cost and physical observations

Pop!_OS, AMD RADV RENOIR PCI 1002:164c, Ryzen 5 5500U, Mesa 26.1.6,
Chrome 154.0.8037.57; Wrela 0.9.0/compiler
`de9b81e52a7747870dfbd5223cb8aeb41e584d23`. Parent `b3332131` plus exact
archived variants. Sequential runs on AMD; no overlapping CPU scoring/training.
Timing is 1920x1080, case 1, 4x. GPU: 32 warm-up and 240 measured frames.
Separate completion/memory run: 300 frames, discarding the first 60.

| Method | GPU median/p95 ms | CPU call p99 ms | Start-to-completion p95/p99 ms |
| --- | ---: | ---: | ---: |
| Regular | 21.96008/22.28536 | 0.6611 | 94.3252/95.0395 |
| Cache, two slots | 14.46528/14.76664 | 1.0013 | 47.6627/62.2420 |
| Cache, four slots | 24.05660/24.79944 | 0.9913 | 94.0788/100.6526 |
| Regular + jitter table | 18.49892/18.80336 | 0.6041 | 80.1860/81.0240 |
| Cache, four slots + table | 14.83664/16.26272 | 0.9311 | 52.5809/65.1897 |

All source sampling, guide/depth/motion work, insertion, replacement, age/visibility
checks, fallback, remaining neighbourhood work, updates, display and compositing
are included. This analytic fixture has no wood or primitive sorting; the earlier
G4 experiments accounted for those. Cold clear and jitter initialization execute
at reset/resize; warm-up excludes their startup latency from steady-state spans.
Cut correctness is measured, but cut-latency tails are not certified.

Preparing the sixteen jitter phases removes repeated Halton loops when decoding
sample positions. The same optimization improves regular 21.96008→18.49892 ms
and four-slot cache 24.05660→14.83664 ms. The prepared cache remains slower than
the 8.33 ms budget and loses quality. Completion includes notification and queued
work; it is not a substitute for GPU span. Single runs and short clips do not
establish a sustained G6 or repeatable percentage-speedup claim.

| Method | Requested live/peak bytes | DRM resident VRAM/GTT bytes | Browser-group PSS bytes |
| --- | ---: | ---: | ---: |
| Regular | 89,493,548/97,787,948 | 174,510,080/18,804,736 | 560,507,904 |
| Cache, two slots | 81,391,420/89,685,820 | 169,947,136/18,706,432 | 545,968,128 |
| Cache, four slots | 114,761,276/123,055,676 | 199,917,568/18,706,432 | 575,351,808 |
| Regular + jitter table | 89,493,804/97,788,204 | 174,583,808/18,706,432 | 574,715,904 |
| Cache, four slots + table | 114,761,532/123,055,932 | 203,259,904/18,706,432 | 574,060,544 |

At 1080p, the two-slot cache occupies 33,369,856 bytes; four slots require
66,739,712, versus 41,472,000 for the original raw history. All-requested totals
include inactive fixture resources, scratch, fallback inputs, output/screen,
uniform and upload resources. Prepared four-slot cache uses 28.23% more live
requested memory than prepared regular. No hidden predecoded image or learned
payload is omitted. Actual DRM/PSS snapshots include browser/driver state; they
are not asset-attributed appearance residency or transient physical peaks.
Overlapping UMA pools must not be added. There is no 16x memory claim.

## Focused checks and retained implementation

Both capacities were independently replayed from actual exported observations
for cases 1 and 23 across all four scales. Cache records, timestamps, oldest-slot
replacement, RGBA and layer identity match exactly; no same-frame key collisions
occur. Each audit compares 512 reconstructed pixels per case/scale against an
exhaustive 5x5-cell CPU gather, with maximum named-view error below 0.000234.
Wrong-velocity and ignored-expiry negatives expose errors. Instrumented captures
match ordinary captures, so observation export does not alter rendering.

Preparing jitter preserves all 1,024 paired RGBA frames exactly. Eight prepared
cache snapshots match the previous audited state byte-for-byte. All sixteen
jitter entries agree with independent CPU arithmetic. A cut/reset matches a
fresh start for both frames at every scale; the no-reset negative changes
99,944–124,339 channels per scale. Unchanged reference and mask checks are reused.

Two implementation failures are preserved: the initial large clear exceeded the
GPU workgroup-count limit and was repaired by clearing 32 words per invocation;
a hyphen in a timing fixture package name failed before GPU execution. Neither
failure is omitted or counted as a rendering result. The clear repair preceded
the first complete comparison and did not change filtering or storage capacity.

The final code removes the cache and diagnostic tool modifications. Only the
fixed, non-indexed `raw-source` decoder prepares jitter; learned and indexed paths
retain their prior evaluation. The inactive model-context buffer is reused,
adding 240 bytes to that fixed decoder and no allocation to ordinary rendering.
One 16-thread initialization dispatch runs on resize. No model weights change.

Final cleanup verification reproduces 1,152 image/reference/mask files exactly:
128 fixed-control frames and 128 frames for each of the two frozen learned
controls, each with its reference and mask. Final retained complete GPU median/
p95 is 18.49648/18.80460 ms,
with 89,493,784/97,788,184 live/peak requested bytes.
Its DRM VRAM/GTT snapshot is 174,587,904/18,706,432
bytes and browser-group PSS 572,350,464 bytes. Source/type and allow-list
checks accompany the new module; no broad unchanged test suite is repeated.

**Next architecture bet: separate geometric coverage from per-surface radiance.**
The cache still treats visibility-biased observations as a single normalized RGB
cloud. Test a conventional factorized reconstruction using the same available
source geometry and footprint information, charging every coverage query and
radiance-history operation under the unchanged colour-sample budget. Use the
existing clips and references. A learned residual is justified only after the
rendered comparison identifies what the conventional representation cannot retain.
This is an untested hypothesis, not a promised G1 or G4 win.

Existing sampling and reconstruction citations are in
[the reconstruction note](neural-kernel-reconstruction.md#primary-sources).
The bounded cache/replacement and shared-jitter implementation are original code;
no external code, weights or new assets were imported.

Evidence: `build/neural-evidence/surface-cache-v1` on Pop!_OS.
Manifest SHA-256: `474d249e9b36b99fc35a0d353f93697214222189bb4a3d5a4d1a139369a2d094`.
