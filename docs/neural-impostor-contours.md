# Retire contour impostors and projected-corner caches

2026-10-30. **Decision: retire the tested two-corner contour geometry and persistent
corner caches.** The source-derived contours pass the established quality checks
but cost **1.03–2.65% more total GPU time than indexed quads**. Sharing their
projected corners closes most of that gap, at **13,551,288 B** additional live
allocation, but the valid comparison remains **0.26% slower**.

The last followup removes the contours and caches only the original quad corners.
It saves **0.29–0.51% total GPU time** in two source-matched comparisons, but adds
**10,500,224 requested live bytes** and **10.03–27.28 MB observed GPU residency**.
Full-route browser elapsed improves in only one pair. This small GPU-only benefit
does not justify promoting the cache. Indexed quads alone show mixed, small total
changes and are not promoted either. The production renderer remains unchanged.

First paired assessment: **18.14 minutes**.
First complete quality/runtime/residency assessment: **27.86 minutes**.
Explicit decision: **46.92 minutes**. Exactly two targeted followups.
Solo Pop!_OS AMD; frozen models, zero training, unchanged G1–G7 and completed
references. No G4 win, physical-memory saving or frontier achievement is claimed.

## Representation, controls and source knowledge

Parent 082bdebfa607a42629097a06e2f9851d8eafc24b has the retained runtime from
3708b775's [exact-depth impostor path](neural-impostor-depth-reuse.md).
All methods receive the same source fields, geometry, atlases, species identities,
camera, lighting, visibility, LOD marks, sampling and reconstruction. No reference
image or privileged learned input enters a candidate.

Three arms were declared before measurement:

- **parent:** retained six submitted vertices, two original billboard triangles.
- **indexed:** four unique vertices and six indices for the identical triangles.
  This separates ordinary vertex reuse from any source-contour benefit.
- **contour:** six unique vertices and twelve indices, clipping the two corners
  opposite the original diagonal into four triangles. The cuts are conservative
  bounds of all four potentially sampled atlas views.

Each source view records the original rectangle plus an eight-direction support
bound. The support includes the entire occupied 4×4 compression block and a filter
margin, rather than assuming compressed alpha is zero at every zero source texel.
The original rectangle stays unchanged. For each selected instance, map every
view's support vertices back through its actual 2×2 projection into the original
quad coordinates, take their union, and round the two cuts outward. A near-singular
projection keeps the whole quad. Source-based cutting is disabled for learned coverage.

Two 16-bit cuts fit the existing otherwise-unused far mark lane. Source identity
and the LOD mark remain intact. Near cards and volumes keep their original draws.
The new boundary vertices interpolate the original wind-deformed edges, preserving
the two original triangle mappings in ideal arithmetic. The two passes use the same
geometry. Raster/interpolation differences remain measurable; no bit-exact claim.

All per-view metadata, index buffers, construction, growth, updates, fallback data
and both visibility phases count. Explicit live authoring edits were not separately
replayed for these rejected prototypes.

## Two targeted followups

**1. cached contours.** Initial contour preparation costs only about 0.065 ms
per settled frame, but its prepass rises from 7.64 to 8.14–8.15 ms. Cache the four
original projected corners and packed cuts once per live selected impostor, then
share them between prepass and shading. The cache record is 80 B, with capacity
and growth room for the entire impostor range in each phase, including unused
capacity. It does not store all material/shading attributes. Camera and wind become
explicit selection inputs. This reduces the prepass to about7.36 ms, with 0.088 ms
preparation, but increases actual/requested memory and does not win complete cost.

**2. cache-only quads.** Remove the contour geometry and source-support metadata.
The ordinary indexed quad shares only four projected corners, 64 B per allocated
cache slot. Its prepass falls to 7.28 ms versus7.63–7.64 ms, preparation costs 0.074 ms,
and shading stays about 3.39 ms. The small complete GPU improvement comes with
10.50 MB extra requested live allocation and additional observed residency.
No third optimization is attempted.

Initial preparation had two script errors, followed by one failed indexed build
whose edit had accidentally changed the baking vertex function. The corrected
edit is scoped to the far billboard function. Failed scripts/source/build/logs
are preserved; no failed executable was measured. All successful builders and
qualification runs restore temporary source edits.

## Complete AMD rendering cost

Ryzen 5 5500U; AMD RADV RENOIR PCI 1002:164c; Mesa 26.1.6;
Chrome 154.0.8037.57; kernel 7.1.5-76070105; Wrela 0.9.0/compiler
de9b81e52a7747870dfbd5223cb8aeb41e584d23. No power/thermal lock.

The established Last Green fixture runs 660 frames at 1920×1080 output, unchanged
0.667 internal scale, one worker, pacing enabled and two frames in flight.
One executable provides seven periodic route captures. Prebuilt candidates run
in one quiet serial AMD queue; builds and scoring do not overlap GPU measurements.
Both orders are retained.

The rendered World snapshot and frame time are controlled while background
simulation runs. Camera height jumps 96 m at 240–299, returns at 300, moves 96 m during
360–539, then settles. It is not ordinary gameplay or independent object motion.

Complete GPU spans include decoding, preparation, visibility, wood, source updates,
fields, lighting, reconstruction, compositing and submission gaps. CPU/browser
intervals remain separate and are not input-to-photon latency. All startup and
update spikes remain in the raw data and total.

| Candidate / control, round | Source match | Total GPU change | Settled GPU p50 control/candidate, ms | Requested live delta, B |
| --- | --- | ---: | ---: | ---: |
| indexed / parent, 1 | yes | -0.65% | 36.765 / 36.636 | +24 |
| indexed / parent, 2 | yes | +0.20% | 36.685 / 36.526 | +24 |
| contour / indexed, 1 | yes | +2.65% | 36.636 / 37.393 | +426,008 |
| contour / indexed, 2 | yes | +1.03% | 36.526 / 37.510 | +426,008 |
| cached / indexed, 3 | **no; excluded** | +0.72% | 36.870 / 36.554 | +2,463,288 |
| cached / indexed, 4 | yes | +0.26% | 37.081 / 36.610 | +13,551,288 |
| quadcache / indexed, 5 | yes | -0.51% | 36.604 / 36.423 | +10,500,224 |
| quadcache / indexed, 6 | **no; excluded** | +0.24% | 36.750 / 36.351 | +7,513,856 |
| quadcache / indexed, 7 | yes | -0.29% | 36.565 / 36.211 | +10,500,224 |

| Candidate, round | Movement GPU change | Movement CPU p95 control/candidate, ms | Settled browser p95 control/candidate, ms | Full-route browser elapsed delta, ms |
| --- | ---: | ---: | ---: | ---: |
| indexed, 1 | -0.36% | 169.858 / 145.791 | 40.364 / 40.087 | +28.985 |
| indexed, 2 | -0.04% | 153.480 / 142.697 | 40.423 / 40.261 | -18.985 |
| contour, 1 | +1.70% | 145.791 / 145.010 | 40.087 / 40.799 | +436.840 |
| contour, 2 | +1.41% | 142.697 / 145.710 | 40.261 / 41.163 | +218.725 |
| cached, 4 | +0.13% | 142.732 / 138.685 | 41.158 / 40.093 | -81.435 |
| quadcache, 5 | -0.66% | 147.325 / 140.378 | 40.045 / 40.093 | +101.750 |
| quadcache, 7 | +0.16% | 144.848 / 143.908 | 40.312 / 39.739 | -246.205 |

Every window's GPU p50/p95/p99/max, CPU quantiles, browser intervals, timestamps
and labels remain in assessment JSON. Two small paired GPU gains do not establish
a sustained presentation or confidence-qualified global speedup.

Pairs 3 and 6 are retained/excluded independently of timing: census record 9 has
shapes 62/capsules 0 in the indexed control and shapes 62/capsules 5907 in its candidate;
the remaining census fields agree. Pair 7 reuses the same executables to replace
the cache-only quad's unmatched reverse comparison. This is not a new optimization.
The cached-contour retirement has one source-matched comparison. All seven primary
valid comparisons match every recorded census, camera and player-frame record.
Initial controls are shared between comparisons; they are not independent replications.

## Shaded quality and silhouettes

The initial **contour** variant passes **36/36 existing shaded development checks**:
24 forced-impostor stress checks and 12 native-resolution checks. Reuse the completed
256/1024/independent 1024 quadrature references, unchanged whole-image/crown regions
and four reconstruction scales. No new reference infrastructure or target.

Unchanged gates: FLIP/LPIPS error ratio ≤1.05, PSNR delta ≥−0.25 dB.
Worst results: FLIP **1.042344**, LPIPS
**1.021909**, PSNR **-0.209353 dB**.
Viewing: exposure-1 Reinhard, sRGB AlexNet LPIPS v0.1, LDR-FLIP at 67 pixels/degree,
without resizing. These ratios use the same completed target, not independent
transport truth or a claim of improved fidelity.

Initial contour native leaf coverage has precision **99.999303%**,
recall **100.000000%**, symmetric boundary p99
**0.000000 px**, maximum **2.828427 px**.
Coverage union MAE is 0.000023392; 213 pixels differ by
more than 2/255 coverage. The class includes impostor wood and excludes separate
near wood. It is not per-instance/strand truth or the full G3 motion suite.

Across 49 matched paired route views (some initial controls reused), maximum RGB
difference is 33/255 and at most
1397 pixels per view differ by more than 2/255.
Indexed-only and cache-only quads differ from their matched controls by at most 1/255.
Display alpha agrees, but is not silhouette truth.

The runtime/memory-rejected cache followups were not put through another full
reference qualification. Paired route images are not a claim that those variants
passed all 36 checks. The first complete joint assessment belongs to initial contours.
Their recorded cost/memory tradeoffs are sufficient to reject them without retraining
or spending further reference work on an unpromoted implementation.

## Actual and requested memory

Requested totals include common resources, all caches, scratch, unused capacity
and fallback data, plus startup/update peaks. Physical measurements deduplicate
DRM clients and sample browser process-group PSS throughout and at completion.

**indexed against parent, matched comparisons only; bytes.**

| Observation | Control range | Candidate range |
| --- | ---: | ---: |
| Requested live | 707,871,892–707,871,892 | 707,871,916–707,871,916 |
| Requested peak | 903,654,884–907,751,252 | 892,417,916–910,065,092 |
| Final DRM VRAM | 911,675,392–920,596,480 | 928,944,128–945,799,168 |
| Final DRM GTT | 22,773,760–22,790,144 | 22,708,224–22,790,144 |
| Final process-group PSS | 1,088,601,088–1,090,408,448 | 1,097,753,600–1,098,372,096 |
| Sampled maximum DRM VRAM | 1,083,502,592–1,083,576,320 | 1,080,647,680–1,100,795,904 |
| Sampled maximum DRM GTT | 22,806,528–22,814,720 | 22,749,184–22,843,392 |
| Sampled maximum process-group PSS | 4,256,297,984–4,266,345,472 | 4,265,221,120–4,330,728,448 |

**contour against indexed, matched comparisons only; bytes.**

| Observation | Control range | Candidate range |
| --- | ---: | ---: |
| Requested live | 707,871,916–707,871,916 | 708,297,924–708,297,924 |
| Requested peak | 892,417,916–910,065,092 | 895,478,316–910,491,132 |
| Final DRM VRAM | 928,944,128–945,799,168 | 928,968,704–954,089,472 |
| Final DRM GTT | 22,708,224–22,790,144 | 22,708,224–22,790,144 |
| Final process-group PSS | 1,097,753,600–1,098,372,096 | 1,125,639,168–1,129,248,768 |
| Sampled maximum DRM VRAM | 1,080,647,680–1,100,795,904 | 1,072,336,896–1,083,514,880 |
| Sampled maximum DRM GTT | 22,749,184–22,843,392 | 22,769,664–22,806,528 |
| Sampled maximum process-group PSS | 4,265,221,120–4,330,728,448 | 4,235,175,936–4,316,939,264 |

**cached against indexed, matched comparisons only; bytes.**

| Observation | Control range | Candidate range |
| --- | ---: | ---: |
| Requested live | 707,871,916–707,871,916 | 721,423,204–721,423,204 |
| Requested peak | 906,289,268–906,289,268 | 917,022,404–917,022,404 |
| Final DRM VRAM | 920,653,824–920,653,824 | 933,392,384–933,392,384 |
| Final DRM GTT | 22,609,920–22,609,920 | 22,626,304–22,626,304 |
| Final process-group PSS | 1,093,747,712–1,093,747,712 | 1,229,545,472–1,229,545,472 |
| Sampled maximum DRM VRAM | 1,075,187,712–1,075,187,712 | 1,100,365,824–1,100,365,824 |
| Sampled maximum DRM GTT | 22,642,688–22,642,688 | 22,642,688–22,642,688 |
| Sampled maximum process-group PSS | 4,272,612,352–4,272,612,352 | 4,303,218,688–4,303,218,688 |

**quadcache against indexed, matched comparisons only; bytes.**

| Observation | Control range | Candidate range |
| --- | ---: | ---: |
| Requested live | 707,871,916–707,871,916 | 718,372,140–718,372,140 |
| Requested peak | 896,957,292–906,184,184 | 913,010,964–916,786,788 |
| Final DRM VRAM | 920,567,808–920,596,480 | 930,594,816–947,871,744 |
| Final DRM GTT | 22,708,224–22,773,760 | 22,790,144–22,872,064 |
| Final process-group PSS | 1,084,798,976–1,141,699,584 | 1,121,960,960–1,126,416,384 |
| Sampled maximum DRM VRAM | 1,072,336,896–1,083,572,224 | 1,091,960,832–1,091,964,928 |
| Sampled maximum DRM GTT | 22,740,992–22,806,528 | 22,822,912–22,888,448 |
| Sampled maximum process-group PSS | 4,235,091,968–4,339,636,224 | 4,232,088,576–4,294,522,880 |

VRAM, GTT and PSS overlap on UMA and are never summed. Sampling has gaps; sampled
maxima are not true physical peaks. Physical allocations vary between fresh
browsers and drivers. No cause is asserted for the difference between requested
and resident deltas, and no physical-memory reduction is claimed.

## Decision and next architecture bet

**Retire these two-corner contours and persistent corner caches.** The contour
quality is adequate on the existing cases, but added work defeats the uncached
geometry. Caching buys too little complete runtime improvement for its memory.
Leave the retained renderer, models and goal thresholds unchanged. This does not
rule out every possible contour representation or cache policy.

Next, adapt the existing [virtual-geometry visibility resolve](../engine/virtual.wrela)
to production impostors. Use each prepass pixel's winning source identity to
recover and shade its surface, instead of submitting tree geometry for shading
again. Test full resolve cost, source-coordinate/derivative fidelity, memory,
both visibility phases and existing quality gates against the retained path.
This is an unmeasured architecture bet using an existing engine technique;
its external attributions remain in [citations](citations.md). It is not another
promised speedup or an excuse to change the reference.

## Evidence and replay

Evidence root on Pop!_OS:
 /home/ryanwible/worktrees/wrela-neural-frontier/build/neural-evidence/impostor-contours-v1

Manifest SHA-256: c3ce470dd3c70cd1abcb67b120aa8b658222992276bf27a3c1a16262a065f98b

The bundle preserves 16 runs/112 periodic PNGs, all four candidate sources and
builds plus the retained baseline, failed preparation/builds, both source mismatches,
all timings/allocations/residency observations, the 36-check initial qualification,
frozen inputs and replay scripts. Retained-runtime.patch is empty.
Only this report, the benchmark appendix and docs allow entry are committed.
One allow check and diff verification suffice; unchanged renderer tests are not rerun.
