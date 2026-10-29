# Source-certified leaf coverage: retain the packed guarded path

2026-10-29. Parent `d5f7274e07ebc7d4364e5689a05fdf1718953cb3`. **Continue this source-certificate
architecture.** Five alternating full-workload pairs reduce complete GPU median
from **25.32668 to 24.89280 ms**
(1.71% lower, ratio of medians). The retained path adds **65,536 live bytes**.
All observed HDR outputs match the control exactly. This is a conventional
rendering improvement, not a neural advantage or a G1–G7 pass.

First joint shaded-quality/runtime/residency assessment: **29.16
minutes**; decision **57.72 minutes**. Exactly two optimization
followups, no training and no new reference algorithm. G1–G7 and all eight frozen
artifacts are unchanged.

## Attribution selected the experiment

The production vegetation labels combined about 7.8 ms, without identifying which
draws dominated. The existing label API measures passes, so a temporary diagnostic
split the unchanged draws into separate passes with the same targets/depth/order.
Complete GPU was 25.34324 ms versus 25.46076 ms for the control, requested allocation
was identical, and the final 1080p frame differed by at most one byte in two
components. The small whole-frame difference is not a speed claim.

|Depth work|Median ms|
|---|---:|
|Near cards|2.91844|
|Grass|0.93736|
|Far crowns|0.14704|

Shading medians were cards 1.36576, grass 1.24596, limbs 0.60688, stone 0.43952,
far crowns 0.18516 ms. The instrumentation was archived and restored before the
optimization; infrastructure progress is not counted as a rendering win.

## Same source and filter, fewer proven-empty reads

The control already uses [source-generated leaf patterns and tight card outlines](../engine/plant.wrela).
Both methods receive identical atlas, cards, geometry, source parameters, camera,
wind, visibility, leaf-count and sampling information. No primitive is removed,
no sample pattern changes, and the original texture/filter remains resident.

[The coverage shader](../engine/vegetation.wrela) always reads the original centre
sample and computes its derivatives. Magnified coverage follows the original
sharp filter. For minified coverage, it normally reads four offset samples.
A table can skip those four only when their entire bilinear footprint is certified
zero; the original sharp contribution is still preserved. Unknown regions use
the original four samples. Shadow coverage remains on the original path.

Each original 2×2-texel cell has a lower bound on Chebyshev distance to a nonzero
source texel. Cooking scans 18×18 texels and clamps the bound to 8 texels; any unscanned
texel is farther away. The runtime bounds both the cell offset and four sample
offsets, plus a full texel of bilinear support and a rounding margin. Pattern
clamping cannot enlarge the offset. Four half-texel integer distances fit exactly
in one 32-bit word: a 128×128 R32Uint table, 64 KiB total.

|Trial|Consequential question|
|---|---|
|Initial support|Can an empty-source certificate save enough of the four reads? 256 KiB R32Float table.|
|Followup 1 packed|Can exact four-per-word packing reduce cache cost and resident storage? 64 KiB.|
|Followup 2 guarded|Can the mandatory centre read avoid querying the table when its coverage is nonzero?|
|Qualified retained path|Same final algorithm, with explicit atlas-lifetime contract and a focused GPU check; no third optimization.|

The [Wood constructor](../engine/wood.wrela) defaults fixed_leaf_atlas to false:
generic callers get the original shader and no support allocation. The opt-in also
requires the standard 512×512 atlas. Clearing opts in because its atlas is fixed;
remake_woods reconstructs Wood and the table after source edits. A caller must
reconstruct Wood after editing an opted-in atlas. Camera/wind/instance changes
do not require rebuilding a table defined solely over atlas texels.
This is an explicit caller contract, not automatic mutation tracking.

This source factoring is our implementation; it imports no new external rendering
method or learned weights.

## Complete cost and actual resident memory

AMD RADV RENOIR PCI1002:164c, Ryzen 5 5500U, Mesa 26.1.6, Chrome 154.0.8037.57.
Wrela 0.9.0 compiler `de9b81e52a7747870dfbd5223cb8aeb41e584d23`.
Clearing 1920×1080, production 2× reconstruction, one worker, 600 warmup and 60 measured
frames, adaptive workload and pacing disabled. Separate paced timing and allocation
replays use the same executable. No thermal/power lock.

|Measurement|Baseline|Support|Packed|Guarded|Qualified|
|---|---:|---:|---:|---:|---:|
|Complete GPU median, ms|25.46076|25.28736|25.06196|24.94188|25.01736|
|Complete GPU p95, ms|27.06388|26.96172|26.65472|26.51876|26.65324|
|Vegetation depth, ms|4.03652|3.96980|3.65992|3.59564|3.59452|
|Vegetation shade, ms|3.82264|3.81624|3.82084|3.81964|3.82224|
|CPU frame-call median, ms|1.73000|1.78000|1.81000|1.69500|1.66000|
|Separate paced GPU p50, ms|25.44296|25.48262|25.06730|25.11630|25.07566|
|Separate paced GPU p95, ms|27.01260|27.08719|26.82218|26.48542|26.68694|
|Separate paced GPU p99, ms|27.94612|27.74542|27.49439|27.61500|27.71198|
|All requested live bytes|405,267,256|405,529,400|405,332,792|405,332,792|405,332,792|
|All requested peak bytes|582,445,428|582,707,572|582,510,964|582,510,964|582,510,964|
|Observed DRM VRAM bytes|551,931,904|552,017,920|551,993,344|551,993,344|551,993,344|
|Observed DRM GTT bytes|20,082,688|20,180,992|20,279,296|20,180,992|20,180,992|
|Observed browser PSS bytes|848,997,376|843,317,248|801,412,096|800,209,920|805,156,864|

Complete GPU span includes decoding/preparation, ordering, visibility, both vegetation
phases, wood, updates, lighting, clouds, reconstruction and composition. All scratch,
caches, fallback data and outputs are charged. CPU frame-call is not GPU completion
or presentation latency. Physical values are deduplicated whole-client snapshots
after warmup, not per-asset attribution or physical peaks; do not add UMA pools.
The PSS variation does not establish a host-memory saving.

The table cooks once during startup (and would recook when an opted-in Wood is
reconstructed): initial 0.98616 ms,
packed 1.86884 ms, guarded 1.85236 ms,
qualified 1.85252 ms. These costs and the higher requested
peak are recorded, not hidden in a steady label. Full source-edit storms and
presentation deadlines were not qualified.

Five alternating comparisons use the final qualified source and original complete
control. GPU paired changes: -1.504%, -1.916%, -1.808%, -1.851%, -1.070%.
The median paired change is -1.808%; tool status faster.
CPU frame-call medians are 1.69000 and
1.70000 ms; status
inconclusive. Do not infer CPU acceleration from GPU savings.

## Shaded quality and certificate verification

Each trial and final qualification batches the established three reference checks
and four temporal scales in one executable. All **28 HDR buffers** match the
completed parent exactly. Consequently FLIP, LPIPS, PSNR and contrast-band errors
are unchanged against the completed 16k/65k/independent 65k references. Reference
provenance and unchanged parent source are checked before reuse.

All four 1080p comparison images differ from the control by at most one byte level,
with no pixels above two levels; independent replay differences are recorded.
The fixed shot includes silhouettes but is not the complete G3 semantic boundary,
moving-camera or reveal suite. The image-derived contrast band is only a diagnostic.
The shaded reference uses production quadrature, not independent converged transport.
Native HDR quality and Chrome runtime/residency are distinct AMD measurements.
[Metric definitions and primary citations](citations.md#neural-reconstruction-instrument)
are unchanged.

[The focused GPU test](../engine/tests/leaf-support/main.wrela) evaluates
**2,097,152 queries** over 16 patterns, six footprint sizes, slanted footprints,
clamped pattern edges and cell-boundary perturbations. It uses the 8-leaf
source, a rebuilt 3-leaf source, synthetic sparse/border coverage and a full source.
All **876,837 certificates** are valid: zero omitted nonzero samples. Inflating each
source bound by just one texel yields **33,887 detectable errors** through the same
production query.
These counts describe test coverage, not the production skip rate.

The initial shader check found unavailable Texels indexing and scalar .r access;
the repaired API use preserves the algorithm. The focused test initially used
read-only textures for compute writes, then a repair missed an import. Failed
sources/logs are preserved. The final targeted test passes 1/1; changed production
type checks pass. No broad unchanged-engine suite was rerun.

## Decision, limits and next architecture bet

Retain the guarded packed certificate for explicitly fixed source atlases.
The initial float-table and unguarded variants are superseded; temporary attribution
passes are removed. The evidence supports a modest complete conventional rendering
win at small additional memory, not a 16× appearance-memory win or a world-leading
renderer. G1–G7 remain unchanged and unmet.

Source-derived prefiltered coverage for strongly minified cards: test whether a compact integrated representation can replace the four original coverage reads, retaining an exact source fallback around uncertain silhouettes. Compare with the strengthened conventional path using the same source, filter and visibility information; train only if rendered evidence identifies a useful residual.

Evidence on Pop!_OS:
`/home/ryanwible/worktrees/wrela-neural-frontier/build/neural-evidence/vegetation-work-v1`. Source/build archives, complete profiles and timing tails, actual
residency, all captures/metrics, failures, guard output, alternating pairs and
external replay scripts are preserved. See replay.md there. Use fresh roots when
replaying; do not overwrite sealed evidence.

Manifest SHA-256: `d42480953bab68f5713b42df5c05c608c73868bb58724faa4a54212187f928fb`.
Inherited benchmark edits remain unstaged.

Annotation correction: clearing uses six leaves per source pattern. The prior focused
GPU guard used eight-leaf and three-leaf sources; those were adversarial source
variants, not the production leaf count. Its raw results and sealed evidence are unchanged.
