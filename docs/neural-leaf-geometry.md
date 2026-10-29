# Shared leaf geometry: exact captures, no useful complete-frame saving

2026-10-29. Parent `e16e414e368a00fce42bb723cc1f8ccb2be53b50`. **Retire all three variants.**
Sharing transformed card centres and wind evaluation improves the measured shading
subtotal, but preparation consumes almost all that saving. The final variant still
adds resident storage without a demonstrated complete-frame advantage.
Runtime is restored exactly to the parent. No new rendering win or G1–G7 pass.

First full shaded-quality/runtime/residency assessment: **17.40
minutes**. Decision: **33.30 minutes**, after exactly two targeted
optimization followups. The clock conservatively starts at the preceding checkpoint
commit, including inspection and intervening time. No training or new reference
algorithm; all eight frozen artifacts and G1–G7 are unchanged.

## Source sharing and a fair control

[The card vertex program](../engine/vegetation.wrela) evaluates an instance's source
transform and [wind field](../engine/plant.wrela) at its card centre for each corner.
The depth and shading passes both run that vertex program. We test one common
record per selected card, reused across both draws. The control is the existing
production renderer with the same cards, instances, order, visibility, wind, camera,
samples and shading. No learned method receives extra information.

GPU prefixes read the actual indirect draw counts. An indirect preparation dispatch
rebuilds records every frame, for both early and late visibility phases. Each phase
has a fixed 262,144-record cap; overflow uses the original vertex program. Source
buffers and fallback resources remain resident and are included. Records never
persist across frames, so a held camera does not hide a cache-invalidation scheme.
Current wind, camera and source/instance state are read during every preparation.

| Variant | Record | Routing |
|---|---|---|
|shared|32B: swayed centre, halfwidth, seed, keep, card and instance identifiers|Per-vertex prepared/overflow branch|
|specialized, followup1|Same32B|Admit whole instances; separate indirect prepared and original-fallback draws|
|compact, followup2|16B: swayed centre and source size|Same separate draws; recover metadata from original indices/marks|

The compact form exploits the actual source rule
[by_card](../engine/lod.wrela): it returns a whole or empty share according to whether
size is positive, while fade_packed preserves the upper anchor bits. The halfwidth
uses the original multiplication order. No quantized positions, approximate wind,
removed leaves or weaker coverage are introduced. Remaining corner projection
and all fragment math are unchanged. This is our source factoring, with no new
external rendering implementation.

## Complete browser runtime and actual residency

Same AMD RADV RENOIR PCI1002:164c / Ryzen5 5500U / Mesa26.1.6 / Chrome154.0.8037.57.
Wrela0.9.0, compiler `de9b81e52a7747870dfbd5223cb8aeb41e584d23`.
Full clearing at1920×1080 with production2× reconstruction, one worker,
600warmup and60measured frames, adaptive workload/pacing disabled. Separate paced
timing and allocation/residency replays use each same executable. No thermal/power
lock. These single-run values do not prove a stable sub-percent speed difference.

|Measurement|Baseline|Shared|Specialized|Compact|
|---|---:|---:|---:|---:|
|Complete GPU median,ms|25.37440|25.35220|25.38672|25.43744|
|Complete GPU p95,ms|26.99232|26.97316|27.17864|27.19632|
|Card preparation early,ms|0.00000|0.11780|0.12860|0.11336|
|Card preparation late,ms|0.00000|0.02284|0.03324|0.03320|
|Vegetation prepass,ms|3.99084|3.97016|3.96600|4.01292|
|Vegetation shading,ms|3.81880|3.68264|3.68128|3.67388|
|CPU frame-call median,ms|1.66000|1.78000|1.89500|1.89000|
|Separate paced GPU p50,ms|25.51074|25.48664|25.41498|25.63788|
|Separate paced GPU p95,ms|27.13426|27.16562|27.21829|27.22235|
|Separate paced GPU p99,ms|28.07861|27.98740|28.26046|27.97636|
|All requested live bytes|405,267,256|422,046,520|422,051,400|413,662,760|
|All requested peak bytes|582,445,428|582,445,620|582,445,780|582,445,716|
|Observed DRM VRAM bytes|552,001,536|568,799,232|568,762,368|560,455,680|
|Observed DRM GTT bytes|20,197,376|20,279,296|20,197,376|20,279,296|
|Observed browser PSS bytes|838,801,408|855,055,360|860,054,528|863,187,968|

Complete frame span includes preparation, prefix/argument work, visibility/culling,
both vegetation phases, wood, lighting, clouds, temporal reconstruction, updates
and composition. Existing ordering and any other decode/sort work are included;
this is production rendering, not the specialized G4 appearance fixture.
CPU frame-call time is not GPU completion or presentation latency.

Requested totals include all fallback data, caches, scratch, executable rings,
staging and outputs. Physical figures are actual deduplicated whole-client
snapshots after warmup, not physical peaks or per-asset attribution. UMA pools
must not be summed. The old startup allocation peak dominates the measured
requested peak; that does not erase the candidate's higher steady allocation.
No memory or startup improvement is claimed.

The combined vegetation labels remain about7.8ms. Their entire time cannot be
attributed to card vertices: they also contain grass, far crowns and fragment work.
The observed reduction in a subtotal does not count as a rendering win when
preparation offsets it.

## Shaded quality, boundaries and limits

Each variant uses one executable to batch the established three reference checks
and four temporal scales at192×108. All **21 buffers** from the three candidate
runs match the already completed baseline exactly, including scene-linear HDR.
All four scales therefore have identical FLIP, LPIPS, PSNR and reference-contrast
band diagnostics. The prior baseline's source files are checked against this
block's unchanged parent, and the existing16k/65k/independent65k references are
reused with their provenance hashes.

- shared: full1080p frame max RGB difference 0 byte levels; pixels above2 levels 0; repeat max 0 levels in 0 components.
- specialized: full1080p frame max RGB difference 1 byte levels; pixels above2 levels 0; repeat max 1 levels in 2 components.
- compact: full1080p frame max RGB difference 1 byte levels; pixels above2 levels 0; repeat max 1 levels in 2 components.


HDR captures run on the AMD native path; full-frame cost/residency and final
display comparisons run in Chrome on the same AMD adapter. This does not make
a native timing a browser timing. Established exposure-1 Reinhard viewing,
FLIP67pixels/degree, LPIPSAlexNetv0.1 and sRGB PSNR remain unchanged:
[definitions and citations](citations.md#neural-reconstruction-instrument).

Exact observed images include their silhouettes, but a fixed shot does not establish
G3's semantic strand/coverage, moving-camera, reveal or stale-history gates. The
image-derived contrast band is a diagnostic, not semantic silhouette truth.
Per-frame preparation/update cost is included in the production replay;
a separate dynamic camera/wind quality campaign was not run because the
architecture already lacks a useful static joint advantage. No broad equivalence
claim is made. The existing shaded reference is production quadrature, not
independent converged light transport.

## Decision and the next consequential question

Retire the shared geometry cache after both followups. Keep the earlier retained
source-region specialization and other parent improvements unchanged.
The record compression works as designed, and observed quality stays exact,
but additional resident data and complexity have not bought a useful complete
rendering improvement. No extra tuning or repeated timing campaign is justified
by the present result. This does not rule out a different workload where repeated
card transforms dominate.

Source-compiled coverage and work selection for vegetation fragments. First separate card, grass and far-crown cost within the existing production workload: the aggregate7.8ms label does not identify a bottleneck. Then test a source-informed reduction of the dominant work with unchanged visibility/sample information and complete preparation/memory/update accounting.

This next investigation is a cost attribution step needed to select a substantive
architecture experiment, not itself a rendering win. It must retain a whole-frame
baseline and must not infer removable cost from aggregate labels.

## Preserved failures, checks and replay

The initial source called Slots.len, an unavailable API. The type check rejected it
before rendering; the failed source/log is retained. Passing capacity explicitly
repaired it. The first preparation-script writer also failed JavaScript parsing
before any command executed; that failure is recorded. All subsequent changed-code
format/type checks passed, and all candidates executed on AMD.

No new test framework or reference method was added. The restored runtime is the
exact parent, so no repeated unchanged-engine suite is run. Candidate-specific
dynamic correctness testing would be necessary before promotion, which did not occur.

Evidence root on Pop!_OS:
`/home/ryanwible/worktrees/wrela-neural-frontier/build/neural-evidence/leaf-geometry-v1`.

It preserves each source/executable, complete profiles, requested allocations,
actual DRM/PSS, HDR captures and scored comparisons, failed sources, candidate
patch and restoration record. The external task-state scripts are archived too.

Replay in a fresh checkout/root: init_leaf_geometry.py, baseline profile,
prepare_leaf_geometry.py plus the recorded capacity-argument repair, then profile
and capture/score. Apply the specialized and compact preparation scripts in order,
each followed by the same profile and batched capture. Absolute paths in scripts
must be redirected; never replay over sealed evidence. The source archives are
authoritative for each measured implementation.

Manifest SHA-256: `50207e59bf0c42c20c255eabc1b4fad86ca0c2dd7c96f3dd950462927b292c44`.
Only this block's benchmark append is staged; inherited edits remain unstaged.
