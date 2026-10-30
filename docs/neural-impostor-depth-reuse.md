# Reuse the impostor prepass's exact depth

2026-10-30. **Decision: continue the existing impostor representation with exact-depth
shading.** Four source-matched Pop!_OS AMD comparisons reduce complete 660-frame
GPU span totals by 5.40–6.09%.
Settled full-GPU median falls from 42.10–42.21 ms
to 36.59–36.84 ms.
All 36 existing shaded development checks pass. This is a conventional rendering
win, not a neural-model improvement, physical-memory reduction or G1–G7 achievement.

No new buffers, caches, fallback data or learned inputs. Models and G1–G7 unchanged;
no training, solo Pop!_OS work. First paired image/runtime/memory assessment:
27.40 minutes. First complete assessment including
reused shaded references and coverage: 50.26 minutes;
decision: 50.26 minutes. Exactly two optimization followups,
both retired. All failures and unfavorable measurements are preserved.

## Why the existing visibility information helps

[Wood's flat impostor prepass](../engine/wood.wrela) writes the winning crown's
ID and planar depth. Shading previously moved its billboard toward the eye and
tested GreaterEqual, then rejected losing IDs in the fragment shader.
The selected change draws the same unshifted plane in both passes and tests Equal.
The GPU can reject uncovered and hidden quad pixels before shading.
The ID check remains for coincident surfaces. Actual-depth volume crowns keep
their existing path; plain, compressed, paged and source-identity impostors use
the same flat-prepass rule.

Wrela already expresses the useful contract: visibility for this planar
surface was resolved in the preceding cut. This exploits that information
without changing geometry, atlases, volumes, lights, sampling or reconstruction.
It is not a new general depth-testing algorithm.

Both methods receive identical fields, source geometry, camera, visibility,
sampling and updates. Last Green's control is parent 264bfb08, including prior
retained conventional wins; its BC impostors have neural reading disabled.
Clearing quality checks use its retained source-identity path against the
completed equally informed conventional control/reference captures.
No reference-derived data enters runtime. Measured paths are BC Last Green and
source-identity clearing; other formats and hardware remain unqualified.

## Bounded experiments

The draw-removal diagnostics are incorrect images, **not quality-matched wins**.
Settled full GPU p50: parent 42.084 ms; without trees 28.669; without grass 41.824;
without near cards 42.073; without far crowns 29.218; without volumes 42.024.
Distant impostors dominate. Near-card, grass and volume-cache directions were
rejected before implementation.

The selected change reduces settled vegetation shading from about 8.95 to 3.43 ms.
That label also includes other work. Its prepass remains about 7.65 ms.

1. **Relaxed depth test:** same unshifted plane with GreaterEqual. Only 0.36–0.99%
   total GPU improvement versus parent, same 62/255 maximum pixel difference.
   It loses most of Equal's benefit. Retired.
2. **Local front-to-back ordering:** stable 32-instance groups within kind/way
   lists, preserving instance/mark pairs; 256 bytes declared workgroup scratch,
   no global buffers. No measured prepass reduction. Extra total GPU savings
   versus exact-depth alone: 0.64–1.01%; final PSS rises 7.17–95.48 MB and VRAM
   8.56–16.80 MB. These observations do not establish useful overdraw reduction.
   Retired. The failed first build and corrected executable are preserved.

No third optimization. Runtime contains neither sorting nor diagnostic hooks.

## Complete rendering cost

Ryzen 5 5500U, AMD RADV RENOIR PCI 1002:164c, Mesa 26.1.6, Chrome 154.0.8037.57,
kernel 7.1.5-76070105, Wrela 0.9.0/compiler
de9b81e52a7747870dfbd5223cb8aeb41e584d23. No clock/thermal lock.

Each fresh browser runs 660 frames, 1920×1080 output, unchanged 0.667 internal
quality scale, one worker, pacing enabled, two frames in flight, adaptation
disabled. One quiet serial GPU queue; no concurrent builds or image scoring.
Executables are reused, seven periodic images each, controls and candidates
in both orders. All four selected pairs match source census, controlled camera
and player-frame records; performance does not select the pairs.

The fixture renders the initial World snapshot at frame-defined time while
background simulation continues. Camera height jumps 96m at 240–299, returns 300,
moves 96m during 360–539, then settles. This exercises streaming and rendering,
not ordinary interactive gameplay or independent object motion.

The table gives control/candidate milliseconds. GPU spans cover minimum timestamp
start through maximum end, including submission gaps and all recorded renderer
work: decoding, selection/visibility, wood, updates, lighting, reconstruction and
compositing. CPU/browser timings are separate and not added to overlapping GPU
work. Browser intervals are not input-to-photon latency.

| Pair | Settled GPU p50 | Settled p95 | Settled p99 | Total GPU reduction | Browser elapsed delta,ms |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1 | 42.164 / 36.841 | 44.462 / 38.917 | 45.171 / 40.243 | 6.09% | -1759.3 |
| 2 | 42.102 / 36.617 | 44.494 / 38.909 | 45.077 / 39.892 | 5.83% | -2028.4 |
| 5 | 42.168 / 36.782 | 44.594 / 38.944 | 45.934 / 39.784 | 5.40% | -2282.3 |
| 6 | 42.212 / 36.592 | 44.777 / 38.842 | 45.699 / 40.168 | 5.95% | -1706.8 |

Settled CPU median: 6.62–6.89 ms
control, 6.56–6.83 ms candidate.
Browser interval median:
43.34–43.50 ms and
37.80–37.93 ms.
Movement-window total GPU improves
4.89–5.47%.
All windows, maxima, labels and raw frames remain in evidence.
Startup includes large compilation/update gaps; no cold-start tail win claimed.

## Shaded quality and silhouettes

The 28 selected paired browser images are **not bit-exact**. Maximum RGB difference
is 62/255 in a localized return-view area; at most 83 pixels in any capture
differ by more than 2/255. Output alpha is exact, but display alpha is not a
silhouette measurement. Removing the forward shift can alter raster/interpolation
or intervening occlusion decisions; the cause of every changed pixel is not isolated.

Existing completed coarse 256, fine 1024 and independent 1024 quadrature references
are reused. Forced-impostor stress: four scales, three references, whole/crown
regions, 24 checks. Production 1920×1080: 12 checks plus integrated raw leaf-class
coverage. No new reference scheme or candidate-derived quality target.

All 36 pass unchanged development gates: FLIP/LPIPS ratio ≤ 1.05, PSNR delta ≥ −0.25 dB.
Worst across all checks: FLIP ratio 1.042756,
LPIPS ratio 1.023758, PSNR delta −0.222027 dB.

| Production scale | FLIP error ratio | LPIPS error ratio | PSNR delta,dB |
| --- | ---: | ---: | ---: |
| 1× | 1.000532 | 1.000809 | -0.002671 |
| 2× | 1.000159 | 1.000046 | -0.000353 |
| 3× | 1.000099 | 0.999914 | -0.000174 |
| 4× | 1.000044 | 0.999831 | -0.000076 |

Ratios are candidate/control error against the same target. Exposure 1 Reinhard,
sRGB AlexNet LPIPS v0.1, LDR-FLIP 67 pixels/degree, no resizing.
Existing crown rows 0–87 (stress), 0–879 (native) unchanged.

At 50% leaf coverage: precision 99.999303%,
recall 100.000000%; symmetric boundary distance
p99 0.000000 px, maximum 2.828427 px.
Coverage MAE over the union: 0.000022278;
214 pixels differ by more than 2/255 coverage.
This class includes impostor wood but excludes separately drawn near wood.
It is not per-strand/branch/instance truth, full G3 motion or independent transport
truth. Reference convergence and every metric row are retained.
Small favorable score changes are not fidelity wins.

## Requested and actual memory

Requested allocations include common assets, scratch, caches, fallback data,
startup and updates. The selected runtime adds no resource. Three selected
pairs have identical requested live bytes; pair 2 finishes 6,462,720 bytes lower.
Allocations vary despite matching recorded source arrivals; this is not
attributed to the six binding changes.

| Bytes | Control range | Candidate range |
| --- | ---: | ---: |
| Requested live | 705,358,612–707,871,892 | 698,895,892–707,871,892 |
| Requested peak | 892,417,892–906,289,244 | 890,950,556–906,289,244 |
| Final DRM VRAM | 903,303,168–945,803,264 | 920,584,192–931,098,624 |
| Final DRM GTT | 22,626,304–23,101,440 | 22,790,144–23,101,440 |
| Final browser PSS | 1,074,736,128–1,223,694,336 | 1,033,200,640–1,136,576,512 |
| Sampled maximum DRM VRAM | 1,072,328,704–1,083,564,032 | 1,060,737,024–1,092,489,216 |
| Sampled maximum DRM GTT | 22,679,552–23,117,824 | 22,806,528–23,134,208 |
| Sampled maximum browser PSS | 4,230,060,032–4,293,554,176 | 4,241,260,544–4,300,441,600 |

Final paired PSS changes range −103.02 MB to +61.84 MB; VRAM changes have both signs.
**No physical-memory saving is established.** Raw DRM fdinfo, smaps, allocation
traces and sampled maxima are retained. VRAM/GTT/PSS overlap on UMA: never sum
them. Sampling has a 50 ms sleep plus collection time and can miss shorter peaks;
sampled maxima are not true physical peaks. Requested peaks remain separate.

## Next architecture bet and evidence

Target the remaining roughly 7.65 ms impostor prepass with source-derived
conservative coverage geometry, so empty billboard regions need less fragment
work. Charge geometry, metadata, build/update scratch and fallbacks; provide the
same source-coverage information to conventional and learned alternatives.
The failed local sort argues against assuming small-batch ordering solves it.

Evidence: build/neural-evidence/foliage-shared-work-v1 on Pop!_OS.
All ablations, failed build, retired variants, source/build archives, controls,
captures, timestamps, residency, reference runs and replay scripts are preserved.
One executable supplies multiple route captures; the established reference
helper still initializes GPU assets per output. Infrastructure progress is not
counted as a rendering improvement.

References: [unchanged goals](neural-rendering-goals.md),
[previous control](neural-distance-capacity.md),
[completed stress references](neural-appearance-projection.md),
[native/coverage protocol](neural-source-identity-atlas.md),
[measurement rules](bench.md), [metric sources](citations.md#neural-reconstruction-instrument).
Own engine implementation; no external code, weights or assets imported.

Evidence manifest SHA-256: eafa4d807db8d4e4ab4094c44f1afd8e27865f83a1d49942bf855497576e4f6c.
