# Retire regular coverage tiles for distant impostors

2026-10-30. **Decision: retire this representation.** A 4×4 subdivision can skip
source-empty parts of a crown billboard, but its extra geometry costs more than
it saves on this AMD route. The best of three implementations remains
**5.40–5.61% slower in complete GPU span totals**, adds **55,339,168 requested live
GPU bytes**, and raises final observed GPU residency by **63.48–71.48 MB**.
The existing exact-depth six-vertex billboards remain the runtime.

The final variant passes **36/36 existing shaded development checks**.
Quality alone does not rescue the runtime/memory tradeoff. No rendering improvement,
memory win, neural advantage or G1–G7 achievement is claimed.

First paired image/runtime/memory assessment: 18.27 minutes.
First complete assessment, including existing shaded references and coverage:
50.07 minutes. Decision: 50.07 minutes.
Exactly two optimization followups; no third attempt. Solo Pop!_OS AMD, no training,
frozen models and unchanged G1–G7. All failed results are preserved.

## Tested representation and fair control

The baseline is commit 3708b775's [exact-depth impostor path](neural-impostor-depth-reuse.md),
including all earlier retained optimizations. Both methods receive identical source
geometry, atlases, fields, lights, cameras, visibility, sampling and reconstruction.
The new coverage data is derived from those shared source inputs, never a reference
image or privileged learned input.

During the existing per-view atlas-bounds scan, record an 8×8 occupancy mask.
Each bit covers an 8×8-texel region. A projected billboard tile is discarded
only if all four possible atlas views find no touched occupied region, expanding
queries by a texel for bilinear filtering and boundary roundoff.
Learned coverage disables this source-based rejection. No model is trained.

Each billboard becomes sixteen tiles, 96 submitted vertices instead of six.
Its geometry interpolates the **original two wind-deformed triangles**; it does
not evaluate a new wind surface at each interior point. View coordinates and
other attributes retain the original affine mappings in ideal arithmetic.
Raster/interpolation differences remain observable; no bit-exact claim is made.

The bounds record changes from a rectangle to rectangle-plus-mask and updates
its GPU accounting. All startup construction, streaming updates, buffer growth, scratch, fallback
data and common resources are charged. Explicit live authoring edits were not
separately replayed for this rejected prototype.

1. **Initial, tiles4:** recompute tile coverage and corner transforms in each
   vertex invocation. One source-matched pair costs 23.21% more total GPU time.
   The other pair has an early source-arrival mismatch and is excluded from
   matched claims, preserved in full.
2. **Followup one, mask4:** compute the mask once per live impostor and store
   it in the existing otherwise-unused far mark lane. Near and volume marks
   stay untouched. This adds no new mask buffer, but remains 13.09–13.58% slower
   against fresh controls. Requested live bytes increase by 212,992.
3. **Followup two, cached4:** prepare original corners and affine attributes
   once and share them between both passes. Camera and wind become explicit
   selection inputs. Cache allocation covers the impostor capacity with growth
   room; only live prefixes are prepared. Unused capacity is fully charged.
   The extra cache reduces the prototype's cost, but still loses to the baseline.

The first build failed because the new bounds record lacked the required Plain
trait for its existing completion fence. The error, source and corrected build
are retained. All runtime/fixture edits are restored after each experiment.
Only this report and its benchmark record are retained in the repository.

## Complete AMD rendering cost

Ryzen 5 5500U, AMD RADV RENOIR PCI 1002:164c, Mesa 26.1.6, Chrome 154.0.8037.57,
kernel 7.1.5-76070105, Wrela 0.9.0/compiler
de9b81e52a7747870dfbd5223cb8aeb41e584d23. No clock or thermal lock.

Fresh browsers, 660 frames, 1920×1080 output, unchanged 0.667 internal scale,
one worker, pacing enabled, two frames in flight, adaptation disabled.
Candidates are built before one quiet serial GPU queue; no concurrent builds
or image scoring. Each executable supplies seven periodic route captures.
Both execution orders are retained.

The existing fixture renders an initial World snapshot with frame-defined time
while background simulation runs. Camera height jumps 96 m at 240–299, returns 300,
moves 96 m during 360–539, then settles. Source census, camera and player-frame
records match for the five primary pairs. This is not ordinary interactive
gameplay or an independent object-motion test.

Control/candidate settled medians are milliseconds. Total GPU spans cover all
recorded work, including decoding, selection, preparation, visibility, wood,
updates, lighting, reconstruction and compositing, plus submission gaps.
CPU and browser intervals remain separate; they are not added to overlapping
GPU work or called input-to-photon latency.

| Variant, pair | Source records match | Total GPU change | Settled GPU p50 control/candidate | Requested live delta,B |
| --- | --- | ---: | ---: | ---: |
| tiles4, 1 | **no; excluded** | +21.92% | 36.654 / 49.765 | +9,188,992 |
| tiles4, 2 | yes | +23.21% | 36.526 / 49.784 | +212,992 |
| mask4, 3 | yes | +13.58% | 36.717 / 44.884 | +212,992 |
| mask4, 4 | yes | +13.09% | 36.951 / 44.901 | +212,992 |
| cached4, 5 | yes | +5.61% | 36.780 / 40.301 | +55,339,168 |
| cached4, 6 | yes | +5.40% | 36.703 / 40.050 | +55,339,168 |

For the final variant, settled GPU p95 rises from
38.87–39.29 ms to
42.38–42.42 ms;
p99 rises from 39.56–40.33 ms
to 43.12–43.18 ms.
The vegetation prepass is about 8.96 ms versus 7.65 ms, and vegetation shading
about 4.87 ms versus 3.43 ms. These labels are subsets of the complete cost.
Total browser elapsed rises 1.60–1.98 s across 660 frames.
Every CPU quantile, browser interval, update/startup maximum and label is retained.

## Shaded quality and silhouettes

The final cached variant reuses completed 256/1024/independent 1024 quadrature
references: 24 forced-impostor stress checks and 12 native 1920×1080 checks, with
unchanged whole-image and crown regions and four reconstruction scales.
No new quality target or reference infrastructure.

Unchanged gates: FLIP/LPIPS error ratio ≤ 1.05 and PSNR delta ≥ −0.25 dB.
Result: 36/36 pass. Worst ratios/delta across all checks:
FLIP 1.043258, LPIPS 1.020996,
PSNR -0.228229 dB. Ratios compare candidate/control error against
the same completed target. Viewing is exposure-1 Reinhard, sRGB AlexNet LPIPS
v0.1, LDR-FLIP at 67 pixels/degree, without resizing.

The 35 source-matched paired route images are not bit-exact:
maximum RGB difference 43/255 and at most
2606 pixels per capture differ by more than 2/255.
Output alpha agrees, but that display alpha is not silhouette truth.

The existing integrated native leaf-class coverage check gives precision
99.999303%, recall 100.000000%,
symmetric boundary distance p99=0.000000 px and
maximum=2.828427 px. Coverage MAE over the union is
0.000024406; 216 pixels differ by more than
2/255 coverage. This class includes impostor wood, excludes separate near wood,
and is not per-strand/branch/instance truth or a complete G3 motion test.
Reference convergence and all rows remain available. Neither production
quadrature references nor favorable score noise establish independent transport
accuracy or a fidelity improvement.

## Actual and requested memory

All sizes below are bytes for the final matched pairs. Requested allocations
include common resources, caches, scratch, unused cache capacity, fallback data
and all startup/update peaks. Physical observations are deduplicated DRM fdinfo
and browser process-group smaps PSS, sampled throughout the runs and at completion.

| Observation | Control range | Final tiled range |
| --- | ---: | ---: |
| Requested live | 707,871,892–707,871,892 | 763,211,060–763,211,060 |
| Requested peak | 892,417,892–908,835,384 | 943,696,020–947,440,924 |
| Final DRM VRAM | 912,183,296–920,702,976 | 983,666,688–984,186,880 |
| Final DRM GTT | 22,855,680–22,872,064 | 22,708,224–22,708,224 |
| Final process-group PSS | 1,093,736,448–1,227,115,520 | 1,130,810,368–1,234,566,144 |
| Sampled maximum DRM VRAM | 1,063,931,904–1,084,096,512 | 1,118,855,168–1,127,247,872 |
| Sampled maximum DRM GTT | 22,888,448–22,904,832 | 22,740,992–31,019,008 |
| Sampled maximum process-group PSS | 4,254,958,592–4,256,320,512 | 4,263,313,408–4,295,849,984 |

Final VRAM increases in both pairs; PSS changes by +140.83 MB and −96.31 MB.
No physical-memory saving is established. VRAM, GTT and PSS overlap on UMA and
must not be summed. A 50 ms sleep plus collection time separates samples; shorter
peaks can be missed. Sampled maxima are not true physical peaks, and requested
allocation peaks remain separate.

## Next architecture-level bet

Keep six-vertex billboards and change the far-instance list into a **global
front-to-back stream across kinds**. Far shaders already read each instance's
kind from shared tables, so kind-grouped draw order is not a material requirement.
The earlier 32-instance sort could not reorder across chunks or kind boundaries.
A global stream could reduce overdraw without this experiment's geometry
amplification; sorting, compaction, scratch and updates must all be measured.
This is the next hypothesis, not a demonstrated win.

Evidence: build/neural-evidence/impostor-coverage-tiles-v1 on Pop!_OS.
Source/build archives, failed build, all six pairs including the mismatch,
raw timestamps/images/memory, completed-reference reuse and replay scripts
are preserved. No prototype runtime remains installed.

References: [retained baseline](neural-impostor-depth-reuse.md),
[unchanged goals](neural-rendering-goals.md),
[stress references](neural-appearance-projection.md),
[native and coverage protocol](neural-source-identity-atlas.md),
[measurement rules](bench.md), [metric sources](citations.md#neural-reconstruction-instrument).
Own implementation; no external code, weights or assets imported.

Evidence manifest SHA-256: a965a757c8b9f709614202628030c2471e4b5348d632cd70a4934ca78f1e8d74.
