# Shared source grids: retire the mixed-field interpolation

**Retire the owner-local trilinear base-field representation.** Source guards preserve shaded quality by falling back almost everywhere. Removing the guard fails quality and still misses the fastest conventional control's timing limit. Restore the parent renderer and retain the complete failed experiment.

The first complete assessment, including preparation and the strengthened direct-field control, finished **13.48 minutes after the observed implementation start**, or **22.48 minutes including an explicit nine-minute allowance for initial inspection**. That allowance is an accounting estimate, not a precisely observed turn-start timestamp. One candidate followup; no training or new references. G1–G7 remain unchanged and unmet.

## Representation and fair controls

The preceding [changing-edit experiment](neural-changing-edits.md) stores authored-base normal-weight sums separately for each leaf. This experiment replaces those per-card sums **and** the compact per-card response cache with one table per source clump, shared across all ten layouts.

Each table covers the owner's normalized bounding cube. Four or eight cells per axis use five or nine vertices. A vertex stores the other clumps' unnormalized weighted normal G and scalar weight W in f32. The owner's contribution is evaluated exactly. Authored clump metadata is stored with the table so GPU inference does not reconstruct the host-side tree.

Each cell checks the 19 additional edge, face and center locations of a 3×3×3 probe pattern. Interpolation is rejected if any probe exceeds 0.0015 in normal-vector distance or 0.0025 in depth. This is an **empirical source check, not a certified continuum bound**. It uses source evaluations alone, with no shaded reference or learned labels.

Unmoved owners use interpolation plus exact current-minus-authored contributions from changed clumps. Moved owners and rejected cells evaluate the full current source. The source still supplies exact ownership, positions, cutouts, visibility, material laws and wood. The table, guard data, authored metadata, temporary construction buffers and full source fallback are all counted.

The tables occupy **111,936 / 694,848 B** for four/eight cells per axis, independent of the ten layouts. Guard construction additionally requests **22,528 / 180,224 B** of temporary storage. These payload numbers alone do not establish G4's resident-memory ratio.

Controls are the retained native Oct12 updater, compact f32 updater, stabilized direct field and existing bounded-raw direct field. All receive the same useful source, edit dependencies, visibility and sampling information. The initial steady comparison omitted the raw direct-field variant; it was added before the complete decision. Native and compact update controls already used bounded-raw sums. No speed claim rests on the weaker stabilized direct-field comparison.

## Rendered quality and the failed followup

Ten layouts contain 26,560 cards each and share the 44-clump crown. The same five authored-base edit states, views 133/141, four lights, AA4 and 320×128 mosaic are reused. Quality scores use the unchanged 30 converged references on three backgrounds: **90 rows per method**, covering seeds 7/101/251 and all five states. Each executable batch captures all 400 combinations for the ten layouts.

| Candidate vs fastest native | Pass / rows | Worst PSNR loss dB | Worst FLIP ratio | Worst LPIPS ratio |
| --- | ---: | ---: | ---: | ---: |
| 4-cell-axis guarded grid | 90 / 90 | 0.004234 | 1.000302 | 1.013739 |
| 8-cell-axis guarded grid | 90 / 90 | 0.004464 | 1.000378 | 1.013642 |
| 8-cell-axis, guard removed | 49 / 90 | 0.522061 | 1.045158 | 1.420107 |


Both guarded grids pass all 90 rows against every recorded control. The single candidate followup removes interpolation rejection from the finer grid while retaining exact evaluation for moved owners. It passes only **49/90** against each control. This is a failed candidate, not a valid low-cost approximation.

The decisive source diagnostic reads the actual GPU guard table and classifies all **265,600 authored leaf queries** at the base state:

| Grid | Accepted cells / total | Accepted leaf queries / total |
| --- | ---: | ---: |
| Four cells per axis | 0 / 2,816 | **0 / 265,600** |
| Eight cells per axis | 497 / 22,528 | **3,506 / 265,600 (1.320%)** |

These are all authored queries, not the frequency of visible shading invocations. Moved-owner fallback can only reduce acceptance during edits. The coarse grid's quality pass validates full fallback, not successful interpolation. The finer grid scarcely uses its table. Removing fallback exposes substantial shaded errors from interpolating already-combined source contributions.

All **3,600 captured images** are finite and retain exactly the bounded-raw control's alpha. Therefore the existing silhouette errors remain; this does not meet G3. All **800 preparation-each-frame images** are byte-identical to their prepared-cache counterparts. **528 prior baseline images** remain byte-identical. No additional reference work was needed.

## Complete rendering, preparation and actual memory

| Method | Saturated median / p95 ms | Paced p95 / p99 ms | Requested live / peak B | Resident VRAM / GTT B | Browser PSS B |
| --- | ---: | ---: | ---: | ---: | ---: |
| Native immutable Oct12 | 2.24624 / 2.36788 | 3.04277 / 3.10952 | 13,581,900 / 13,745,740 | 26,501,120 / 10,199,040 | 512,014,336 |
| Compact immutable f32 | 2.59420 / 2.72716 | 3.95355 / 4.05676 | 8,073,324 / 8,237,164 | 22,052,864 / 10,035,200 | 492,166,144 |
| Direct stabilized field | 3.12112 / 3.15124 | 4.45830 / 4.57109 | 1,698,932 / 1,862,772 | 17,788,928 / 10,100,736 | 511,159,296 |
| Direct bounded-raw field | 3.05524 / 3.09360 | 4.42856 / 4.55481 | 1,698,932 / 1,862,772 | 18,046,976 / 10,035,200 | 512,728,064 |
| 4-cell-axis guarded grid | 3.06944 / 3.09704 | 3.97715 / 4.03246 | 1,810,860 / 1,974,700 | 17,797,120 / 10,100,736 | 492,165,120 |
| 8-cell-axis guarded grid | 3.07724 / 3.10504 | 4.06527 / 4.22658 | 2,393,772 / 2,574,052 | 17,793,024 / 10,035,200 | 492,418,048 |
| 8-cell-axis, guard removed | 2.53540 / 2.68880 | 3.46317 / 3.57360 | 2,393,772 / 2,574,052 | 17,793,024 / 10,199,040 | 494,153,728 |
| 4-cell-axis, preparation each frame | 3.48252 / 3.51880 | 4.14308 / 4.25920 | 1,810,860 / 1,974,700 | 18,006,016 / 10,297,344 | 495,052,800 |
| 8-cell-axis, preparation each frame | 5.13500 / 5.18372 | 5.88046 / 6.00398 | 2,393,772 / 2,574,052 | 18,010,112 / 10,231,808 | 496,363,520 |


Complete spans include source updates, placement/decoding, hardware visibility/depth ordering, wood, shading, scratch, copies and compositing. These paths need no separate software sort. The steady rows exclude initial table construction; preparation rows charge reconstruction of the table and guards on every source transition before rendering the full frame. Preparation kernels alone have median labels of 0.40432 / 2.05168 ms, but the complete preparation-plus-render spans above govern the decision. Do not sum pass medians.

Against native Oct12's 2.24624 ms, guarded grids cost about **37% more**. Even the invalid guard-free candidate costs **2.53540 ms**, about **12.9% more**, and its paced p95/p99 are **1.138× / 1.149×** native. Both guarded candidates also exceed G4's 1.1× p95/p99 ceiling. No second optimization experiment is justified by this result.

Against the 3.05524 ms bounded-raw direct field, guarded grids have slightly higher saturated medians and consume additional requested memory. Their paced tails are lower in these unpaired runs; those observations are retained, not discarded or treated as statistically resolved. No small-speed-win or dominance claim is made. The direct field already avoids both per-card caches without a grid.

Whole-client resident VRAM is approximately 17.8 MB for the grids and direct fields, versus 26.5 MB for native and 22.1 MB for compact caching. The apparent large reduction in requested representation bytes therefore does **not** establish a 16× physical-residency reduction. Geometry storage also differs between native and procedural controls, so this is not the identical-common-buffer attribution required for a full G4 claim.

Requested live/peak includes all resources, scratch, caches, dummy/fallback bindings, host rings and output banks. Sample shading also uses 1 KiB workgroup storage; the grid adds no shared arrays. DRM deduplicates device/client IDs, and PSS covers the browser group. These are whole-client resident snapshots, not attributed appearance bytes or physical peaks. Do not sum overlapping UMA pools.

Timing uses 240 measured frames after 60 warmup, one worker, sequential AMD browser runs. Paced GPU tails come from the profiler's separate paced run; CPU and completion observations come from the 300-frame allocation run. No CPU scoring or other experiment overlapped timed GPU measurements. There is no enforced thermal/power lock or sustained G6 claim. CPU-call and CPU-start-to-GPU-completion p50/p95/p99 remain in decision.json; completion includes queue/notification, not presentation.

## Decision, retained state and next bet

Retire this uniform mixed-field grid and its runtime code. The experiment identifies a representation problem: source components combine into a normal/weight field whose interpolation is too sensitive for the tested tables. A dense table mostly triggers fallback; a permissive table damages rendered quality. This does not prove all interpolation or source compression impossible.

Return to the planned neural block: preserve **separate surface detail and causal source visibility observations**, first comparing an equally informed conventional reconstruction. For future G4 work, preserve source components or material-sensitive transitions before interpolating, rather than allocating a larger uniform grid around the same combined response. Do not spend another block tuning these grid sizes or guard thresholds.

Engine, content and studio files are restored byte-for-byte to parent **f67feece7391a89315978c20be992b2e10786f93**. The experimental module is removed. Each measured source/build, diagnostic, failed candidate and initial typecheck failure is archived. All measured builds execute successfully; only the inherited Camera layout warning remains. Because the retained renderer is exactly restored and prior baseline images match, no redundant rebuild of unchanged code is added.

The initial compiler failures were invocation ownership for storage writes and calling a potentially panicking host tree constructor from GPU code. Own-slot kernels and explicit authored metadata resolve them. Before the first capture, a guard-table prefix offset and variant-selection regular expression were corrected. Per-method source archives are authoritative for replay.

No new rendering win, neural promotion, generic-3D, unseen-family, 8–512 px, fixed-area tenfold density, G3 motion/reveal, or G6 deployment claim follows. G1–G7 and frozen models remain unchanged.

## Evidence

Evidence: /home/ryanwible/worktrees/wrela-neural-frontier/build/neural-evidence/g4-base-grid-v1. Manifest SHA-256: `140aca3467f2daf6cbe657a1fd5ad7bf8fe8fb39d0d1d888291a9c688f875e4b`.

Solo Pop!_OS AMD RADV RENOIR PCI 1002:164c, Ryzen 5 5500U, Mesa 26.1.6, Chrome 154.0.8037.57; Wrela 0.9.0/compiler de9b81e52a7747870dfbd5223cb8aeb41e584d23. No subagents, purchases, training or new reference captures. Existing [metric citations](citations.md#neural-reconstruction-instrument) and [profiling semantics](toolchain.md) apply. This experiment implements ordinary trilinear interpolation directly; no external code or new algorithm claim.
