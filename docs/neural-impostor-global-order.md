# Retire per-frame global impostor ordering on AMD

2026-10-30. **Decision: retire both tested global-order variants.** Ordering all
flat impostors across species saves a little prepass time, but sorting, gathering
and shading cost more. The 16-bit version increases complete GPU span totals
**1.57–2.06%** and live requested GPU allocation **1,642,500 B**. The cheaper
single-pass version is **0.88–2.46% slower**, with **1,314,372 B** more allocation.
No prototype runtime is retained. The existing exact-depth six-vertex billboards remain.

The first complete image/runtime/memory assessment finished in **22.81 minutes**;
the decision followed at **33.89 minutes**. One targeted optimization,
below the limit of two. No training, no new references, unchanged G1–G7 and frozen models.
This is negative evidence, not a rendering win, memory win or neural advantage.

## Representation and fair comparison

Parent 0d87387a0e23e7066fd231b9f9b1dd4d97beecc9 has the runtime from
3708b775's [exact-depth impostor change](neural-impostor-depth-reuse.md).
Both methods receive the same fields, source geometry, atlas data, source identity,
camera, visibility, lighting, sampling, LOD marks and reconstruction. Neither
candidate receives privileged reference data. Material decoding and shading are unchanged.

Each instance already identifies its species, and the pooled atlas bindings work
across species. The candidate globally orders the contiguous impostor way of
the selection buffers, then emits one combined indirect draw. Cards and volumes
retain their original lists. Both early and late visibility phases are included.

The initial radix16 implementation uses two stable 8-bit radix passes, 16-bit
near-first logarithmic depth keys, and a hole sentinel for unused list capacity.
It adapts the existing [particle radix implementation](../engine/particles.wrela),
already attributed to Satish, Harris and Garland in [citations](citations.md).
Its block-total scan handles arbitrary capacity. Live counts are clamped to each
list's room. Gather moves source instance and both LOD mark words together;
unused entries cannot be drawn. Scratch includes capacity and growth room,
is shared sequentially between phases, and participates in memory accounting.

The bucket8 followup uses one stable pass over 255 depth bins from 1 to 4096 m,
with a separate hole bin. It scatters only indices and removes the unused output-key
buffer. It answers the measured problem: can a cheaper ordering stage repay the
small prepass saving? It cannot on this route. No second optimization is spent on
this approach; the next experiment changes coverage geometry.

The first build failed on four ownership errors while selecting scratch buffers.
The failure, source archive and correction are preserved. All actual runs completed
on AMD, and builders/qualification restored their temporary source edits exactly.

## Complete rendering cost

Ryzen 5 5500U; AMD RADV RENOIR PCI 1002:164c; Mesa 26.1.6;
Chrome 154.0.8037.57; kernel 7.1.5-76070105; Wrela 0.9.0/compiler
de9b81e52a7747870dfbd5223cb8aeb41e584d23. Clocks and thermals are not locked.

The established controlled Last Green route runs 660 frames at 1920×1080 output,
unchanged 0.667 internal scale, one worker, pacing enabled and two frames in flight.
One prebuilt executable supplies seven periodic captures. Builds and scoring
occur outside the serial GPU measurement queue; both orders are represented.
The route holds the initial rendered World snapshot with frame-defined time while
background simulation continues. Camera height jumps 96 m during 240–299,
returns at 300, moves 96 m during 360–539, then settles. This is not ordinary
gameplay or an independent object-motion test.

Total GPU span includes decoding, sorting, gathering, visibility, wood, source
updates, fields, lighting, reconstruction, compositing and submission gaps.
CPU/browser intervals are separate, not added to overlapping GPU time or called
input-to-photon latency. Explicit live authoring edits were not separately replayed.

| Variant, pair | Source match | Total GPU change | Settled GPU p50 control/candidate, ms | Requested live delta, B |
| --- | --- | ---: | ---: | ---: |
| radix16, 1 | yes | +1.57% | 36.670 / 37.106 | +1,642,500 |
| radix16, 2 | yes | +2.06% | 36.846 / 37.194 | +1,642,500 |
| bucket8, 3 | yes | +2.46% | 36.630 / 36.908 | +1,314,372 |
| bucket8, 4 | **no; excluded** | +1.66% | 36.773 / 36.994 | -1,258,680 |
| bucket8, 5 | yes | +0.88% | 36.709 / 36.958 | +1,314,372 |

For radix16, settled prepass means fall from about 7.67 to 7.25 ms, while sorting
and compaction cost 0.63 ms and vegetation shading rises about 0.08 ms.
For bucket8, prepass means fall from about 7.65 to 7.30 ms; ordering costs
0.51 ms, with roughly 0.07 ms more shading. These are diagnostic subsets, not
substitutes for the complete measured cost. Startup cost is included in totals;
the movement and settled windows also lose.

| Variant, pair | Movement GPU change | Movement CPU p95 control/candidate, ms | Settled browser p95 control/candidate, ms | Full-route browser elapsed delta, ms |
| --- | ---: | ---: | ---: | ---: |
| radix16, 1 | +1.30% | 141.217 / 146.979 | 40.697 / 40.848 | +696.070 |
| radix16, 2 | +2.15% | 146.074 / 145.975 | 40.722 / 41.135 | +746.045 |
| bucket8, 3 | +1.28% | 139.567 / 147.951 | 40.582 / 41.001 | +507.435 |
| bucket8, 5 | +0.94% | 147.551 / 141.918 | 40.596 / 40.758 | +345.785 |

All per-window GPU p50/p95/p99/maxima, CPU quantiles, browser intervals, labels,
startup and update spikes remain in the assessment JSON and raw results.

Pair4 is preserved and excluded independently of timing: census record9 in the
control has shapes62/capsules0, while bucket8 has shapes62/capsules5907; bins30,
allbins576, longest368, fit128=8 and fit256=53 otherwise agree. The same executables
were rerun in reverse order as pair5. Pairs1,2,3,5 match every recorded source,
camera and player-frame record. This replacement is not another optimization.

## Shaded quality and silhouettes

The initial radix16 version passes **36/36 established shaded development checks**:
24 forced-impostor stress checks and 12 native-resolution checks, using the completed
256/1024/independent 1024 quadrature references, unchanged whole-image/crown regions,
and four reconstruction scales. No reference expansion.

Unchanged gates are FLIP/LPIPS error ratio ≤1.05 and PSNR delta ≥−0.25 dB.
Worst results: FLIP **1.042756**, LPIPS
**1.023758**, PSNR **-0.222027 dB**.
Viewing is exposure-1 Reinhard, sRGB AlexNet LPIPS v0.1, LDR-FLIP at 67 pixels/degree,
without resizing. These are error ratios against a common completed target,
not an independent transport-accuracy or quality-improvement claim.

Native integrated leaf-class coverage for radix16 has precision
**99.999303%**, recall **100.000000%**,
symmetric boundary p99 **0.000000 px**, maximum
**2.828427 px**. Coverage union MAE is
0.000022278; 214 pixels differ by more than
2/255 coverage. The class includes impostor wood and excludes separate near wood.
It is not per-instance/strand truth or a complete G3 motion test.

Across 28 source-matched paired route views, the largest RGB delta is
4/255 and at most
4 pixels per view differ by more than 2/255.
Output alpha agrees, but display alpha is not silhouette truth.
The bucket8 matched views differ by at most
1/255.
The runtime-rejected bucket8 variant was not sent through another full reference
qualification: its paired captures are not a claim that it passed all 36 checks.
The first complete joint assessment is radix16's. This limitation does not prevent
retiring a followup that increases both total runtime and live allocation.

## Actual resident memory

Requested totals include all buffers/textures, common resources, unused capacity,
scratch, caches and fallback data, with startup/update peaks. Physical observations
deduplicate DRM clients and sample browser process-group smaps PSS during the run
and at completion. Final and sampled maxima are reported separately.

**radix16, source-matched pairs only; bytes.**

| Observation | Control range | Candidate range |
| --- | ---: | ---: |
| Requested live | 707,871,892–707,871,892 | 709,514,392–709,514,392 |
| Requested peak | 892,417,892–898,828,076 | 898,009,644–904,707,260 |
| Final DRM VRAM | 928,890,880–928,956,416 | 920,629,248–954,249,216 |
| Final DRM GTT | 22,872,064–23,019,520 | 22,773,760–23,019,520 |
| Final process-group PSS | 1,087,092,736–1,089,299,456 | 1,098,759,168–1,247,655,936 |
| Sampled maximum DRM VRAM | 1,072,324,608–1,072,791,552 | 1,056,137,216–1,083,559,936 |
| Sampled maximum DRM GTT | 23,052,288–31,182,848 | 22,822,912–23,048,192 |
| Sampled maximum process-group PSS | 4,258,050,048–4,323,091,456 | 4,273,967,104–4,295,971,840 |

**bucket8, source-matched pairs only; bytes.**

| Observation | Control range | Candidate range |
| --- | ---: | ---: |
| Requested live | 707,871,892–707,871,892 | 709,186,264–709,186,264 |
| Requested peak | 896,957,268–906,289,244 | 895,894,420–909,036,428 |
| Final DRM VRAM | 911,572,992–920,596,480 | 920,621,056–929,071,104 |
| Final DRM GTT | 22,528,000–22,626,304 | 22,708,224–22,855,680 |
| Final process-group PSS | 1,082,688,512–1,091,438,592 | 1,133,998,080–1,241,070,592 |
| Sampled maximum DRM VRAM | 1,072,316,416–1,083,502,592 | 1,081,303,040–1,092,530,176 |
| Sampled maximum DRM GTT | 22,560,768–22,659,072 | 22,724,608–22,872,064 |
| Sampled maximum process-group PSS | 4,260,838,400–4,271,306,752 | 4,285,378,560–4,317,407,232 |

VRAM, GTT and PSS overlap on this UMA machine and are never summed.
Sampling has gaps; sampled maxima are not true physical peaks. Variability in
browser/driver allocations prevents a physical-memory saving claim. Both tested
representations add live requested allocations, and neither is retained.

## Decision and next bet

**Retire per-frame global radix ordering for this workload.** No frame-time or
resident-memory advantage is demonstrated. Quality is adequate in the qualified
version, but that does not rescue the joint tradeoff. This does not prove every
possible ordering or reuse strategy is bad; it closes these measured candidates.

Next, change coverage geometry: derive conservative crown contours from the shared
source atlas/fields, with a small vertex budget instead of the retired
[4×4 regular tiling](neural-impostor-coverage-tiles.md). Test whether a few contour
edges remove enough empty billboard area to outweigh their vertices and updates.
Preserve the original wind-deformed triangle mapping and silhouette gates. This is
an unmeasured architecture bet, not an expected or claimed speedup.

## Evidence and replay

Evidence root on Pop!_OS:
 /home/ryanwible/worktrees/wrela-neural-frontier/build/neural-evidence/impostor-global-order-v1

Manifest SHA-256: 375b8c0614ff17552c744c1106d4f7e2c7176ace3c4808a103fb93179db52fce

The manifest preserves all ten 660-frame runs (70 periodic PNGs), both candidate
sources/builds, the build failure, source mismatch, all timing/allocation/residency
records, completed-reference dependencies, qualification output and replay scripts.
Prototype sources live in this evidence bundle; the tracked runtime remains the
parent exactly. One docs allow check and diff validation suffice for this retirement;
unchanged renderer tests are not rerun.
