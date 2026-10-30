# Source-bound two-phase visibility: retain the wind envelope, challenge its attribution

2026-10-30. **Decision: continue this composite architecture on the experimental
branch**, retaining the analytic wind envelope and finer depth-pyramid query.
On two matched AMD runs, complete GPU time falls **1.77–2.76%**, movement GPU time
falls **2.60–3.40%**, and browser route elapsed falls **577–687 ms**.
All **36 established shaded-reference checks pass**. Requested live memory grows
**137,280 bytes**; observed physical residency is mixed against the parent.
CPU movement p95 worsens. This is a bounded GPU/route-throughput improvement,
not a general latency, physical-memory or G4 representation victory.

First joint quality/runtime/residency assessment: **30.17 minutes**.
Final envelope qualification: **55.64 minutes**. Decision: **62.33 minutes**.
Exactly **two optimization followups**, no training, no reference expansion.
G1–G7 and all frozen models remain unchanged and unmet.

## Comparison and mechanism

Parent 1ef44ec6b86f800bdc837591a527a42ab21fd992 contains the retained exact-depth renderer
from 3708b775560ce765976d7052023590193eb0a988 and subsequent evidence-only commits.
The initial hypothesis of temporal visibility reuse already exists in the engine:
previous-frame HZB selects early draws, then a current-frame HZB rechecks deferred
instances. This block tests more useful conservative bounds within that path.

All arms receive identical source fields, instances, wind, camera, materials,
visibility, lighting, sampling and reconstruction. Both learned and conventional
methods could use the same metadata. No reference pixels are runtime inputs.

- **Parent:** original sphere bounds and coarse HZB query.
- **Quad:** four actual wind-deformed billboard corners replace the sphere only
  for pure flat-impostor LOD. Mixed, near and volume LOD keep the sphere.
- **Combined:** try the original sphere first, then those source corners.
- **Refined**, followup 1: if the source rectangle's coarse query fails, try that
  rectangle two mip levels finer. Empty pixels outside the useful rectangle can
  otherwise prevent rejection. Stop at the first non-occluding cell.
- **Envelope**, followup 2, retained: use the same refined query, but enclose the
  undeformed source billboard in a world-space box expanded by an analytic wind
  bound. Eight cheap corners replace four full wind-field evaluations.

The useful source rectangle is the union of the same four atlas views and filter
margin used by [far_quad](../engine/impostor.wrela). The envelope follows
[plant::sway](../engine/plant.wrela): gust lies in [0.25, 1.4], felt is at most 1.4,
absolute rocking at most 1.15 and flutter at most 0.35. Orthogonal horizontal
components are bounded by 2.09 and 0.35, so their norm is less than 2.13.
Expansion is 2.13 × length(direction) × abs(strength) × h²/(1+h), plus a roundoff
margin, using maximum nonnegative source height. Negative strength and nonunit
direction are covered. The box encloses rasterized triangle interiors as well
as vertices. Near-plane crossings stay visible; projection uses outward pixel
and depth margins. Same-frame reveal remains active.

[Wood](../engine/wood.wrela) packs original LOD fields, bake midpoint/extent and
256 source-view rectangles into one 260-vec4 table per kind. This preserves the
existing selection storage-binding count. The original LOD buffer remains and
both are charged: 33 kinds add 137,280 requested live bytes. Packing happens
with write_lods, including its initialization and update work. Both production
stages supply the same wind used for drawing; older callers that omit wind keep
the sphere path. Shadows retain their existing path. Source geometry/materials
and the original fallback data remain resident.

This extends existing hierarchical Z/two-phase methods, credited in
[citations.md](citations.md) (Greene, Kass and Miller; Haar and Aaltonen).
No algorithmic novelty is claimed.

## Complete rendering cost

Ryzen 5 5500U, AMD RADV RENOIR PCI 1002:164c, Mesa 26.1.6,
Chrome 154.0.8037.57, kernel 7.1.5-76070105; Wrela 0.9.0/compiler
de9b81e52a7747870dfbd5223cb8aeb41e584d23. No thermal/power lock.

Five prebuilt variants, a quiet serial GPU queue, **19 runs and 133 route PNGs**.
Each 660-frame executable run captures seven views, at 1920×1080 output,
0.667 internal scale, one worker, pacing and two frames in flight. No builds,
toolchain queries or scoring overlap GPU timing or reference capture.
The final six-run order is parent/refined/envelope, envelope/refined/parent.

The controlled World snapshot, time and camera keep rendering comparable while
background simulation continues. Camera rises 96 m at frames 240–299, returns,
moves 96 m during 360–539, then settles. Complete GPU spans include all startup,
source loading/updates, decoding, sorting where present, visibility, wood,
lighting, reconstruction, compositing and submission gaps. CPU frame calls and
browser elapsed/intervals remain separate. This is not input-to-photon latency.
Explicit authoring edits and independent object-motion sequences are not
separately qualified in this block.

| Variant, round | Total GPU | Movement GPU | Settled GPU p50/p95/p99, ms | Browser route elapsed delta, ms |
| --- | ---: | ---: | ---: | ---: |
| quad, 2 | -0.46% | -1.54% | 36.671/39.214/40.131 | -28.135 |
| combined, 2 | -0.75% | -1.46% | 36.788/39.165/40.267 | -358.025 |
| combined, 3 | -0.01% | -1.39% | 36.640/39.075/40.302 | +285.285 |
| refined, 4 | -2.39% | -2.87% | 36.595/38.923/39.820 | -641.400 |
| refined, 5 | -1.77% | -2.92% | 36.527/38.733/40.213 | -565.015 |
| refined, 6 | -2.65% | -3.54% | 36.604/38.992/40.231 | -495.885 |
| refined, 7 | -1.79% | -3.31% | 36.475/39.035/39.917 | -613.040 |
| envelope, 6 | -2.76% | -2.60% | 36.574/38.732/39.949 | -577.415 |
| envelope, 7 | -1.77% | -3.40% | 36.559/38.883/40.142 | -686.985 |

Rounds 6–7 parent settled GPU p50 is 36.592/36.713 ms, p95 39.015/39.256 ms,
p99 40.374/40.205 ms. Envelope's settled p50 remains about 36.56 ms: this is
far from a G6 frame budget. Settled vegetation prepass falls from 7.643–7.660 to
7.455–7.464 ms/frame; late visibility work increases from 0.0425–0.0427 to
0.0583–0.0592 ms/frame, and is included. The route gain is larger than the
settled-frame gain.

**CPU movement p95 regresses:** 138.998 → 156.491 ms in round 6 and
139.613 → 145.285 ms in round 7. Whole-route CPU p95 is mixed:
165.874 → 167.200 ms and 168.182 → 167.230 ms. Browser route elapsed improves,
but a latency-tail win is not established. Raw results include all windows,
maxima, browser intervals and per-label times.

Refined and envelope are essentially tied in total GPU: envelope changes
−0.118% / +0.018% against refined in the direct shared-run comparisons.
Movement changes +0.970% / −0.091%. Envelope is chosen for avoiding full wind
evaluation and its lower observed final VRAM in both direct comparisons, while
retaining quality. This is not an isolated proof of the shader mechanism.

## Requested and physical memory

| Memory, bytes, rounds 6–7 | Parent | Refined | Envelope |
| --- | ---: | ---: | ---: |
| Requested live | 707,871,892 | 708,009,172 | 708,009,172 |
| Requested peak | 892,417,892–906,289,244 | 895,189,532 | 895,189,532–903,792,164 |
| Final DRM VRAM | 928,886,784–928,960,512 | 954,122,240–954,142,720 | 920,588,288–928,985,088 |
| Final DRM GTT | 22,708,224–23,101,440 | 22,855,680–22,953,984 | 22,708,224–23,019,520 |
| Final process-group PSS | 1,091,260,416–1,092,310,016 | 1,089,599,488–1,091,542,016 | 1,076,531,200–1,096,944,640 |
| Sampled maximum DRM VRAM | 1,089,646,592–1,092,493,312 | 1,063,940,096–1,080,713,216 | 1,083,568,128–1,089,638,400 |
| Sampled maximum DRM GTT | 22,740,992–23,142,400 | 22,908,928–22,994,944 | 23,052,288–31,019,008 |
| Sampled maximum process-group PSS | 4,236,183,552–4,236,485,632 | 4,208,813,056–4,277,629,952 | 4,233,340,928–4,341,979,136 |

Envelope final VRAM versus parent is **+98,304 / −8,372,224 B**; process-group
PSS is **+4,634,624 / −14,729,216 B**. No consistent physical-memory saving is
claimed against the parent. Versus refined, final VRAM is lower by
25,157,632 / 33,533,952 B, but PSS changes +7,345,152 / −15,010,816 B.
The allocation/cache/spill cause has not been established.

All observed GPU allocations, scratch, caches and fallback data are included.
DRM VRAM, GTT and process-group PSS overlap on this UMA machine: never sum them.
Samples request a 50 ms cadence; actual gaps are preserved (up to about 294 ms
in the final runs). Sampled maxima are not true physical peaks.

## Shaded quality, silhouettes and fair exclusions

Reuse the completed focused 192×108 forced-impostor references and native
1920×1080 normal-LOD references. Four temporal scales, three existing quadrature
controls and whole/crown regions give 36 shaded checks.
Both initial combined and final envelope pass **36/36**. Refined has matched
route image checks, not a separate full 36-check qualification.

Final worst FLIP ratio **1.042755837**, LPIPS ratio
**1.023757834**, PSNR change **-0.222027 dB** against the
older completed controls; gates stay 1.05/1.05/−0.25 dB. Exposure 1, Reinhard,
display sRGB, AlexNet LPIPS v0.1, LDR FLIP 67 pixels/degree, no resizing.
Candidate integrated RGB is diagnostic and never reference truth.
These are development gates, not independent transport truth or full held-out G3.

Native leaf-class coverage precision is **0.999993027204**, recall
**1.0**, symmetric boundary p99 **0.0 pixels**
and maximum **2.828427 pixels**.
Union coverage MAE is 0.000022278467; 214 pixels differ by more
than 2/255 and 28 by more than 0.1. This class includes impostor wood and
excludes near wood; it is not strand/branch/instance or motion ground truth.

All nine source-matched primary comparisons have camera/player equality and at
most 1/255 RGB channel difference in the seven paired route images.
Three of twelve primary comparisons are excluded and preserved:
quad round 1, combined round 1 and quad round 3. In source census record 9,
candidate capsules=5907 versus parent capsules=0; other census records match.
Their requested-live deltas (−8,838,720 / −2,376,000 / −2,376,000 B respectively)
are not memory wins. All mismatched images/timings remain in evidence.
Parent controls are shared within rounds; direct envelope/refined comparisons
reuse runs and are not additional independent samples.

## Verification, decision and next architecture bet

The focused GPU test covers nine independent synthetic wall/opening/near-plane/
disabled/offscreen/odd-size cases and 1,024 actual wind-field samples against the
analytic bound. It passes. Deliberately disabling occlusion makes its first
case fail; the mutation and failure log are preserved, and restoration is exact.
The existing wood-merge test passes with the default older caller path.
Only changed files are formatted and the established workspace allow check is
run. No broad unchanged renderer test suite is repeated.

**Retain envelope plus refined query on this experimental branch.** Retire the
quad-only, unrefined combined and full-wind refined alternatives as final choices.
All variant sources, builds, observations and exclusions remain reproducible.
The decision weighs a modest complete-GPU/route-time improvement against a small
known requested-memory increase and unresolved CPU-tail costs. It does not
establish a Pareto improvement in every measure.

**Next block must challenge the composite with the original sphere bounds plus
the same finer HZB query, without the source table.** That simpler conventional
control has not been measured; it could explain most of the gain and eliminate
metadata/complexity. Current results therefore do not isolate Wrela's source
knowledge as the cause of the speedup. Include CPU movement tails and a
low-occlusion view before making a broader default-quality/performance claim.
No third optimization is added to this block.

Evidence on Pop!_OS: build/neural-evidence/impostor-source-cull-v1.
Manifest SHA-256: 89163f26e016ad2b9a7f0394c5500b9efcaaee24de8cb241643caf74199a357e. The source archive, retained runtime patch,
all 19 raw runs, 133 images, completed references and dependency hashes, frozen
models, quality scores, memory observations, scripts and test failure are sealed.
The inherited unrelated benchmark diff remains unstaged.
