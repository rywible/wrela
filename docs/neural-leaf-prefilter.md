# Prefiltered leaf coverage: change the representation

2026-10-29. Parent `7e7b854ad59a3c6d1983c0f99029fb5842be3f0f`. **Change the representation.**
The final prefilter improves 4× PSNR by 2.96 dB and lowers the single-run complete
GPU median from 25.01 to 24.07 ms, but 3× LPIPS rises 7.8%, beyond the unchanged 5%
quality tolerance. No new runtime improvement is retained. The renderer is restored
exactly to the parent, including its previously retained packed coverage certificate.

Complete joint assessment including the strengthened source-count control: **19.81
minutes**. Decision: **30.24 minutes**, after exactly two targeted
optimization followups. No training or new reference algorithm. G1–G7 and all eight
frozen artifacts are unchanged and the goals remain unmet.
The initial candidate assessment completed at 12.23 minutes; the conservative 19.81-minute
figure includes the stronger equal-source control.

## Hypothesis and equally informed controls

[The source leaf atlas](../engine/plant.wrela) contains 16 patterns of 128×128 texels.
Clearing cooks six leaves per pattern, and its current kinds admit all six.
The control already uses original four-sample coverage, source-derived tight
outlines and packed certificates that skip proven-empty reads.

The candidate extends the existing 512×512 Rgba8 atlas to eight mip levels, stopping at
4×4 so each pattern still has its own final texel. Level 0 and all original data
remain available; no duplicate source atlas is introduced. The additional levels
cost 349,504 bytes. Texture sampling clamps within each pattern using the coarsest
participating level, preventing adjacent patterns from bleeding into the result.

Only an explicitly supplied full source leaf count enables integrated coverage.
Partial counts use the original filter; generic callers default to disabled.
Centre coverage and derivatives remain unconditional. Footprints below 2 texels use
the original supported four-sample filter, 2..4 blend, and footprints above 4 use a
single trilinear mip read. The original empty certificate bounds only the original
four reads; it never suppresses coverage from the wider mip kernel. Shadow coverage
and all other level 0 tissue reads remain unchanged.

Both methods receive the same source atlas, source code/count, geometry, visibility,
wind, camera and sampling information. Followup 1 explicitly supplies the same source
count to a conventional four-sample control. That control can omit leaf-ID rejection
when all leaves are admitted; it changes neither offsets nor filter weights and
allocates no mips. Its seven HDR buffers are exact, but the measured full frame is
25.05988 ms versus 25.01468 ms for the retained renderer. This small single-run difference
does not establish a slowdown; it provides no evidence of a useful speed improvement.
The existing renderer remains the strongest measured conventional control.

|Trial|Question answered|
|---|---|
|Initial integrated|Does one isotropic mip lookup buy a useful complete tradeoff? Runtime and 4× improve, but 2× quality fails.|
|Followup 1 count-control|Does the same source-count knowledge speed up the original four-read filter? Exact images, no useful measured gain.|
|Followup 2 matched|Does matching filter spread to the original sample offsets repair the regression? It repairs 2×, but 3× LPIPS fails.|

The final correction derives mip width from both source-coordinate derivatives.
The original four offsets are ±X/4±Y/4 in atlas texels. With phase-averaged bilinear
filtering, matching covariance traces gives N²=1+(|X|²+|Y|²)/8. Trilinear mixing
interpolates squared widths between adjacent levels, rather than logarithmic width.
The complete derivation is preserved in filter-width-derivation.md in the evidence.
It matches an average trace, not the original anisotropic or discrete filter.
Quantization, pattern boundaries, depth/compositing and temporal history remain
nonlinear. The rendered measurements decide acceptance; this derivation is not a
proof of image equivalence. No material-moment fitting or neural training occurred.

## Complete rendering cost and actual residency

AMD RADV RENOIR PCI1002:164c / Ryzen 5 5500U / Mesa 26.1.6 / Chrome 154.0.8037.57.
Wrela 0.9.0 compiler `de9b81e52a7747870dfbd5223cb8aeb41e584d23`.
Full clearing 1920×1080 with production 2× reconstruction, one worker,
600 warmup/60 measured frames, fixed workload and no pacing/adaptive workload.
Separate paced timing and allocation replays use each same executable.
No thermal/power lock. No repeated timing campaign was justified after quality failed.

|Measurement|Baseline|Integrated|Count control|Matched|
|---|---:|---:|---:|---:|
|Complete GPU median, ms|25.01468|24.02292|25.05988|24.07140|
|Complete GPU p95, ms|26.64068|25.71348|26.78648|25.57060|
|Vegetation depth, ms|3.59452|2.53888|3.67080|2.68652|
|Vegetation shade, ms|3.83216|3.87156|3.82092|3.81452|
|CPU frame-call median, ms|1.69000|1.72000|1.67000|1.64500|
|Separate paced GPU p50, ms|24.93720|23.99994|25.00518|24.14384|
|Separate paced GPU p95, ms|26.71962|25.68623|26.90454|25.73287|
|Separate paced GPU p99, ms|27.34257|26.44026|27.92134|26.93761|
|All requested live bytes|405,332,792|405,682,296|405,332,792|405,682,296|
|All requested peak bytes|582,510,964|582,860,468|582,510,964|582,860,468|
|Observed DRM VRAM bytes|552,017,920|552,038,400|551,993,344|552,038,400|
|Observed DRM GTT bytes|20,180,992|20,197,376|20,180,992|20,279,296|
|Observed browser PSS bytes|844,873,728|850,541,568|807,154,688|846,270,464|

The complete frame includes ordering, visibility, wood, coverage, shading, updates,
clouds, reconstruction and composition. All scratch, caches, original/fallback data,
staging and outputs are included. CPU frame-call is not GPU completion or presentation.
DRM/PSS are deduplicated whole-client snapshots after warmup, not physical peaks
or per-asset attribution; do not sum UMA pools. PSS variation is not a memory saving.

Mip generation's seven startup dispatches total 0.44370 ms
initially and 0.44625 ms in the final variant. These and the
higher requested peak are preserved. Recreating the atlas regenerates its mips;
no source-edit storm or sustained presentation campaign was qualified. Per-frame
scene updates remain in the workload.

## Fixed shaded references and quality decision

Each candidate uses one executable to batch three reference checks and four
temporal scales at 192×108. Scores use the already completed 16k/65k/independent65k
shaded references, with provenance hashes verified. The parent source is checked
against the prior qualified capture before reuse.

Crucially, the candidate's own converged-looking outputs are **not** substituted
for the fixed target. Its three low-reference checks change because the coverage
filter itself changes. Their differences are recorded; no new target is generated
to excuse them. Existing targets integrate production shading/filtering, not
independently converged light transport or an independent semantic silhouette oracle.
[Metric definitions and primary citations](citations.md#neural-reconstruction-instrument)
remain unchanged.

The table below uses the 65k reference. Ratios above1 are worse. The unchanged
necessary tolerances are FLIP/LPIPS≤1.05 and PSNR delta≥−0.25 dB, with ratio floor 1e−4.
Passing these static numbers would still not establish G3 or a complete G4 pass.

|Variant|Scale|FLIP ratio|LPIPS ratio|PSNR delta, dB|Static tolerance|
|---|---:|---:|---:|---:|---|
|integrated|1×|0.978998|0.988753|-0.124597|within|
|integrated|2×|1.083487|0.996104|-0.701972|FAIL|
|integrated|3×|0.978546|1.041978|+0.558593|within|
|integrated|4×|0.873501|0.925988|+2.751303|within|
|matched|1×|1.012565|1.032070|-0.224391|within|
|matched|2×|0.980451|1.012941|+0.121063|within|
|matched|3×|0.897464|1.077972|+1.235087|FAIL|
|matched|4×|0.847257|0.938161|+2.956573|within|

The initial 2× failures and final 3× failure recur against all three fixed references.
The matched 3× result improves FLIP and PSNR while worsening LPIPS: metrics disagree,
so an improvement in one cannot erase the failed criterion. No human preference
or semantic silhouette claim is inferred from that disagreement.

- integrated: full 1080p max difference 135 byte levels; 153,974 pixels above 2 levels; contrast-band mean difference 2.148266 byte levels.
- count-control: full 1080p max difference 1 byte levels; 0 pixels above 2 levels; contrast-band mean difference 0.000001 byte levels.
- matched: full 1080p max difference 104 byte levels; 92,969 pixels above 2 levels; contrast-band mean difference 1.297699 byte levels.

Full 1080p differences locate changed output; without an independent truth at that
size they do not establish which output is better. The image-derived contrast band
is a diagnostic, not semantic silhouette truth. Native HDR scores and Chrome
cost/residency are distinct AMD paths. Posthoc changed-region metrics are retained
as diagnostics and are not tuned masks or acceptance criteria.

## Explicit decision and next consequential experiment

Change the representation. Preserve the measured 4× quality opportunity, but retire
both isotropic implementations from the active renderer. The count-aware four-read
control is also removed because it supplies no demonstrated joint advantage.
Runtime is restored exactly to 7e7b854a; the earlier retained certificate remains.

The next bet is to preserve the directional footprint and sampling phase instead
of reducing them to a single isotropic width. This is a hypothesis suggested by the
failed joint result, not a proven explanation for the 3× LPIPS increase. Any next
candidate must compare against both the retained four-read path and this preserved
isotropic evidence, on the same useful source information and all scales.
No extra tuning was performed after the two allowed followups.

## Evidence, checks and correction

All changed-code type/format checks passed. Executables, full profiles, actual
allocation/residency, all HDR outputs, scores, rejected source snapshots/patches,
the filter-width derivation and diagnostic image panel are preserved. All GPU jobs
were serial; CPU scoring followed captures. No broad unchanged-engine suite was
rerun after restoring the exact parent.

A prior report called its eight-leaf GPU test source the production source.
Inspection confirms clearing uses six. The prior guard actually tested eight and
three leaves; its raw source/results and sealed evidence remain unchanged.
The prior report's annotation is corrected in this checkpoint.

Evidence on Pop!_OS: `/home/ryanwible/worktrees/wrela-neural-frontier/build/neural-evidence/leaf-prefilter-v1`.
External scripts are archived; replay.md explains the sequence and fixed references.
Use fresh roots; never overwrite sealed evidence.

Manifest SHA-256: `b312f7e524727a6fdb54719da03f93deb5c972832fedc2f3789e2b56c63f6ec3`.
Inherited benchmark edits remain unstaged.
