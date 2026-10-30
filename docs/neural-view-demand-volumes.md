# View demand and sparse volumes: retire the retrofit

2026-10-30 UTC. **Decision: retire this sparse-volume retrofit from the runtime;
change the next experiment to construction directly into a bounded working set.**
The executable AMD path compresses the shared volume records by 9.07x and the
source-ID candidate passes all 36 established shaded checks. The final schedule
saves 2,361,452 requested steady bytes, but does not establish lower actual
resident memory or a frame-time advantage. Building dense data first still pays
the startup high-water mark. Extra decoder and lifecycle complexity is not
justified by this result. The source and failures remain replayable.

First actual quality–memory–runtime assessment: **40.38
minutes**, under the two-hour limit. Decision: 67.76 minutes.
Exactly two targeted optimization followups, solo on Pop!_OS AMD, zero training.
G1–G7 remain unchanged and unmet; all frozen model files remain unchanged.
The retained streamed construction and source-ID renderer from fec85d60 remain.

## Questions and sequence

The initial question was whether a fixed budget of demanded view tiles can beat
full appearance atlases. One initialized executable collected 15 poses: three
authored cameras at four height offsets, then three forced-impostor cases.
The batch took 229.89 seconds, including its build and setup; there is no
matched unbatched timing claim. It counted all four view samples for every actually selected
early/late impostor, giving both conventional BC and source-ID controls identical demand.

Nonempty ordinary poses need 105–502 views. Exact source-ID payload plus routing
is 0.142–0.638 MB; the practical BC counterpart is 0.513–2.099 MB.
The opening needs 397 views: 507,560 ID bytes versus 1,991,680 BC bytes.
Forced cases need 590–822 views and 0.851–1.291 MB of ID payload/routing.
The union is 1,383 views, 2,124,700 ID bytes. Raw u16 ID tiles instead require
3,252,224 bytes in the opening and up to 4.11 MB in ordinary poses.

**These are demand/storage estimates, not an implemented cache or rendering win.**
They omit cache misses, regeneration, eviction, scratch and fallback. BC uses
the same source information, same demanded views and its practical padded
palette plus maps; no privileged learned inputs.

The existing two dense volume atlases alone occupy 2,654,208 bytes. That exceeds
the 1,148,928-byte 16x budget against the current full BC impostors plus these
volumes, even if all view data were free. This is only a partial appearance
accounting bound, not the complete G4 denominator. A demand-limited BC control
would make that target harder.

Of 165,888 voxels, only 21,803 are nonzero; the depth atlas has three zero
channels. Followup one therefore tested shared sparse volumes in both BC and ID
renderers: 4x4x4 occupancy masks, rank lookup and three half2 words per nonzero
voxel. The 292,740-byte result includes 31,104 bytes of brick metadata.
The GPU verifier compares every voxel's stored half bits before activation.
Filtering is manual trilinear interpolation; lossless voxel storage does not
promise identical filtering or rendered pixels.

The first candidate overlapped its 2,021,760-byte construction scratch with
appearance-page construction. Its actual sampled GPU peak increased about
8.4 MB. Followup two delayed volume compression until page activation.
Dense fallback remains until verification and the final copy fence.
This removed the extra high-water mark but did not turn compression into a
resident-memory advantage. No third candidate optimization was attempted.

## Complete memory observations

All-resource requested bytes include geometry, common targets, scratch, caches,
metadata and fallback. Sampled DRM VRAM and browser-process-group PSS are decimal
MB, observed through startup and all 660 frames. Update runs rebuild all twelve
individuals at frame 300 using seed offset 101. Every memory control below was
replayed in this block. The initial volume runs preserve the rejected overlap;
scheduled is the final candidate.

| Run | Requested steady bytes | Requested peak bytes | Sampled VRAM max MB | Sampled PSS max MB |
| --- | ---: | ---: | ---: | ---: |
| parent-bc | 383,312,696 | 475,022,248 | 617.628 | 4555.970 |
| volume-bc | 380,951,244 | 475,314,992 | 626.127 | 4620.048 |
| scheduled-bc | 380,951,244 | 475,022,252 | 617.628 | 4579.346 |
| parent-id | 373,069,104 | 460,157,068 | 607.318 | 4602.964 |
| volume-id | 370,707,652 | 462,178,832 | 615.682 | 4603.128 |
| scheduled-id | 370,707,652 | 460,157,072 | 607.248 | 4567.846 |
| parent-update-bc | 411,065,772 | 499,604,916 | 660.820 | 4565.504 |
| update-volume-bc | 408,702,832 | 499,604,920 | 660.840 | 4494.738 |
| parent-update-id | 400,771,796 | 479,485,372 | 637.071 | 4544.643 |
| update-volume-id | 398,408,856 | 479,485,376 | 633.643 | 4513.875 |

Sampling sleeps 50 ms plus read time; counts and maximum gaps are in the
assessment. Sampled maxima can miss brief peaks. On this UMA device VRAM, GTT
and PSS can overlap and must not be added. GTT and end snapshots are preserved.
Browser PSS still peaks around 4.5–4.6 GB. One ID edit replay sampled a
3.428 MB lower GPU peak, while the BC edit peak was essentially flat; these
favorable and neutral results are retained. No total physical-memory win is claimed.

The 9.07x ratio concerns only these two volumes. Giving sparse volumes equally
to both methods leaves 16,021,380 BC and 5,777,788 ID bytes for impostors plus
volumes: only 2.77x, before other appearance costs. It is not the 16x G4 result.

A stronger scalar-depth conventional control was considered. A single-channel
R16Float depth texture would reduce dense volume storage to 1,658,880 bytes;
the sparse ratio would be 5.67x. This is an unmeasured bound, not a benchmark.
Pinned Wrela 0.9.0 restricts Texture3d to Storage formats; R16Float is not Storage
and 3D textures have no TexelTarget buffer-copy implementation. R32Float is
storage-capable but unfiltered in the pinned API. Saved toolchain declarations
document the constraint; see [toolchain gaps](toolchain-gaps.md).
A sliced 2D texture could provide another conventional implementation; it was
not executed. We do not claim global conventional optimality. This limitation
cannot rescue
a candidate that failed to demonstrate a resident-memory win against the
existing executable controls.

## Complete runtime, including construction and edits

AMD RADV RENOIR (1002:164c), Ryzen 5 5500U, Mesa 26.1.6,
Chrome 154.0.8037.57, Linux 7.1.5-76070105,
Wrela 0.9.0/compiler de9b81e52a7747870dfbd5223cb8aeb41e584d23.
1920x1080 output, normal internal 2x reconstruction, one worker.
Application adaptive measurement and pacing are disabled. No thermal/power lock.

Fresh browsers run the precompiled controls/candidates in forward then reverse
order. Each has 600 warmup plus 60 steady frames. Complete GPU span includes
decoding, sorting, visibility, wood, lighting, compositing and other rendering:
earliest pass start to latest pass end. CPU calls and browser intervals are
separate. Timed runs omit continuous residency sampling; no builds or scoring
run concurrently. Timestamp observer allocations add 4,096 steady bytes.

Startup sums cover frames 0–119; browser elapsed covers starts 0 to 120.
This charges construction but excludes browser launch and pre-first-frame work.

| Fresh normal run | Full GPU p50 / p95 / p99 ms | CPU p99 ms | Browser interval p99 ms | First120 GPU sum / browser elapsed ms |
| --- | ---: | ---: | ---: | ---: |
| repeat1-parent-bc | 24.982 / 26.753 / 27.517 | 1.995 | 29.615 | 10420.6 / 12354.9 |
| repeat1-volume-bc | 25.084 / 26.765 / 27.715 | 2.074 | 29.542 | 10719.4 / 12707.1 |
| repeat1-parent-id | 25.203 / 26.722 / 27.621 | 2.194 | 29.992 | 10544.3 / 12475.1 |
| repeat1-volume-id | 25.124 / 26.796 / 27.681 | 2.095 | 29.552 | 10953.3 / 12958.3 |
| repeat2-volume-id | 25.160 / 26.676 / 27.737 | 1.992 | 30.067 | 10852.1 / 12797.3 |
| repeat2-parent-id | 25.204 / 26.706 / 28.018 | 2.077 | 30.050 | 10511.1 / 12442.6 |
| repeat2-volume-bc | 25.067 / 26.622 / 27.628 | 1.994 | 28.662 | 10909.3 / 12929.3 |
| repeat2-parent-bc | 25.019 / 26.632 / 27.550 | 2.033 | 29.535 | 10454.7 / 12359.7 |

For source edits the window is frames 299–358, with elapsed from start 299 to
359. All twelve changed sources are re-encoded and verified, including volumes.
Old resources, new construction and fallback count together.

| Fresh edit run | GPU sum ms | Browser elapsed ms | Worst GPU / CPU frame ms |
| --- | ---: | ---: | ---: |
| repeat1-parent-update-bc | 2337.705 | 2520.475 | 151.902 / 144.905 |
| repeat1-update-volume-bc | 2337.337 | 2517.635 | 151.149 / 144.435 |
| repeat1-parent-update-id | 2346.425 | 2533.095 | 151.023 / 145.270 |
| repeat1-update-volume-id | 2352.030 | 2537.675 | 151.012 / 145.300 |
| repeat2-update-volume-id | 2351.665 | 2537.300 | 151.809 / 143.940 |
| repeat2-parent-update-id | 2343.038 | 2529.100 | 150.282 / 145.315 |
| repeat2-update-volume-bc | 2340.488 | 2522.275 | 151.275 / 144.870 |
| repeat2-parent-update-bc | 2334.528 | 2518.155 | 150.710 / 145.300 |

Fresh paired comparisons below include resident end snapshots. “volume” names
refer to the final serialized candidate; changed-source comparisons use matched
changed-source controls. Vegetation includes both prepass and shading.

| Pair / candidate | GPU median change | Full GPU p95 / p99 ratio | Vegetation p95 / p99 ratio | End VRAM delta MB | End PSS delta MB |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1 volume-bc | +0.409% | 1.0004 / 1.0072 | 1.0073 / 1.0114 | +0.164 | +102.106 |
| 1 volume-id | -0.316% | 1.0028 / 1.0022 | 1.0138 / 1.0249 | +0.086 | +31.150 |
| 1 update-volume-bc | +0.755% | 1.0018 / 0.9978 | 1.0093 / 1.0366 | +0.131 | +20.929 |
| 1 update-volume-id | +0.177% | 0.9919 / 1.0020 | 1.0176 / 1.0255 | +0.061 | +28.994 |
| 2 volume-bc | +0.190% | 0.9996 / 1.0028 | 1.0067 / 0.9949 | +0.094 | +21.840 |
| 2 volume-id | -0.174% | 0.9989 / 0.9899 | 1.0117 / 0.9971 | +0.086 | +30.084 |
| 2 update-volume-bc | -0.091% | 0.9785 / 0.9971 | 1.0075 / 0.9867 | +0.131 | +19.725 |
| 2 update-volume-id | +0.643% | 0.9976 / 1.0095 | 1.0156 / 1.0124 | +0.066 | +35.120 |

Across the normal-scene pairs, the candidate adds 299–455 ms of summed GPU
work and 352–570 ms of browser elapsed over the first 120 frames. Endpoint
PSS rises 21.840–102.106 MB; the edit pairs rise 19.725–35.120 MB. GPU end
residency differs by only +0.061 to +0.164 MB across all pairs.

These results do not establish a frame-rate advantage. Small steady costs alone
would be acceptable for substantial memory savings; those savings were not
observed. Startup/edit costs and unfavorable residency remain part of the
decision, rather than being hidden behind the 9.07x payload ratio.

## Shaded quality and silhouettes

The source-ID candidate passes all **24 established stress checks and 12 native
1080p checks**, reusing completed coarse/fine/independent references. No new
reference targets or model training. Worst comparison across these checks:
PSNR -0.221011 dB relative to BC, FLIP ratio
1.042650, LPIPS ratio
1.023691; within the unchanged -0.25 dB / 5% gates.

The normal leaf-class coverage metrics match the retained ID result: precision
99.999303%, recall 100%, symmetric boundary p99 0 pixels, maximum 2.828427 pixels.
This class includes impostor wood but excludes separately drawn near wood.
Static development-scene evidence is not G3, broad motion, held-out individuals
or a complete G4 pass. BC candidate screens and real-edit screens are compared
with their corresponding controls, but have no new broad reference qualification.

Native HDR differences, profile/repeat screen deltas and coverage comparisons
are preserved in final-assessment.json. Manual filtering is not asserted
byte-identical to hardware filtering. The serialized followup changes host
ordering only; its profile and edit screens are also retained.

## Decision and next architecture-level bet

**Retire this retrofit.** Keep its exact format and executable evidence as a
research result; restore runtime files to fec85d60. Do not harden its general
API or run further optimization tests after the negative decision. The queued
request predicate omitted volume_requested; it does not affect the measured
clearing order (page request, then volume request, settle until ready), but must
be fixed before any reuse on an already paged Wood. It is not shipping.

The next consequential experiment is **direct source-to-cache construction**:
make only demanded view tiles into fixed, reusable allocations instead of
creating full dense assets before compressing them. Apply the same visibility,
sampling and deterministic source knowledge to BC and ID controls. Charge cold
misses, camera transitions, source edits and exact fallback. Reject it unless
actual residency falls at acceptable complete frame cost and silhouette quality.
The demand census justifies trying this; it does not establish that it will win.

This block deliberately skipped extra volume export and corruption suites once
the measured tradeoff failed: existing per-voxel verification and rendered
checks support the decision, and no candidate runtime is retained. Failed
demand/typecheck attempts, the initial high-memory build and a queue refusal
to repeat an already completed memory run are preserved. The refusal launched
no duplicate GPU work; the queue resumed at its first missing experiment.
Workspace allow and diff checks verify the documentation-only retained change.

Evidence on Pop!_OS:
 /home/ryanwible/worktrees/wrela-neural-frontier/build/neural-evidence/view-demand-cache-v1.
Parent: fec85d6027eed9cc9f7a99deff2a0e29a8967ea3.
Manifest SHA-256: 2d82d9546ea68fe18e8a7d781e41bc7de3ed6e16966e5a9554d92b7725d6111c.

The bundle holds the retired Wrela source and patch, compiled executables,
commands, raw images, timing/allocation/residency records, failed attempts,
scripts, unchanged frozen weights and replay instructions.
Prior [streamed construction](neural-streamed-bake.md),
[source-ID representation](neural-source-identity-atlas.md),
[exact BC control](neural-sparse-appearance.md), and
[metric citations](citations.md) define inherited controls.
No external algorithm implementation or weights were copied.
