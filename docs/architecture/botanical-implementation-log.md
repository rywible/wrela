# Botanical implementation evidence

This is a working implementation record for `botanical-growth-rendering-plan.md`. The acceptance gates remain separate from code completion.

## Baseline on the target M4 Air

Frozen source: `output/vegetation-benchmarks/1790175424453-20832/source`.
1080p MSAA, nine-tree gameplay, 2 s warmup + 15 s bounded queue: 336 matched frames, GPU p50 42.07 ms / p95 44.96 ms / p99 46.20 ms, measured completed throughput 21.71 fps, max GPU allocation 249,948,996 bytes. This is an attribution trial, not the required sustained acceptance run.

The albedo diagnostic took p50 3.15 ms / p95 5.05 ms but removes lighting, sky and shadow work; it is not an isolated foliage measurement. Matched individual shading ablations preserved in the subsequent frozen benchmark sources: removing indirect probes did not improve the workload; removing direct response, sky reflection or sun transport reduced some cost but did not resolve the bottleneck. All such runs are explicitly diagnostic.

## Implemented so far

- Reusable vegetation fixture, frozen-source benchmark, hardware/power metadata, matched GPU frame identity and bounded queue throughput; separate thin-depth interval. Pass timestamps overlap on this tiled GPU and are never summed.
- Versioned developmental source for lodgepole pine and paper birch; permanent shoot/bud IDs, immutable synchronous growth, seasonal light grid and canopy neighbors, bounded source/sink resource accounting, dormancy, foliage cohorts, secondary growth, pruning/damage/environment event replay, serializable continuation checkpoints.
- Source transactions expose growth scrubbing, resources, neighbor shade, shoot history and pruning in Studio.
- Continuous wood rings remove hidden internal caps, carry surface coordinates through the packed/cooked pipeline, preserve wind/source identities, and add restrained flare/collars. Junctions overlap; they are not a watertight surface union.
- Structural tests cover replay, continuation, iteration independence, resources, shade/drought response and persistent pruning. A 32-seed × 4-environment × 4-stage study per species is being generated. Species growth rates remain uncalibrated.

## Outstanding gates

Performance targets, calibrated species distributions, visual approval, aggregate/cell representations, full temporal/coverage qualification, incremental world persistence, and the sustained streaming acceptance run are not yet passed. No diagnostic lighting ablation is a production optimization.

## Exact shader specialization and sustained result

The compiler-derived plain/foliage/bark kernels partially evaluate only features proved absent by the current material and lighting state; unsupported edits fall back to the full kernel. Fixed 1080p MSAA comparison preserved in `output/vegetation-material-review/1790178249308-26395/source/output/browser-1790178249437-26402/comparison.json`: 6,220,800 RGB channels, two channels differed by one 8-bit level; RMS 0.000567 levels. This is display-encoded equivalence evidence, not a new foliage fidelity measurement.

Three one-minute native 1080p temporal trials reached GPU p95 6.29/6.23/6.16 ms and 142.0/140.2/141.1 completed fps. Their revision-1 CPU timings included editor replay and are not comparable with the corrected game-update benchmark.

The required-duration **nine-tree attribution** run (not the complete forest/character/streaming acceptance scene) is frozen at `output/vegetation-benchmarks/1790178356080-27036/source`. Five-minute warmup + 30-minute moving-camera run, native 1080p temporal, bounded queue 4, 223,728 matched frames, no GPU errors/dropped samples:

- Whole run: GPU p50 5.90 / p95 7.34 / p99 7.93 ms; 124.28 completed fps.
- Final ten minutes: GPU p50 6.16 / p95 7.54 / p99 8.26 ms; **116 completed fps**. Render CPU p95 5.60 ms, scene preparation p95 0.50 ms. GPU allocation maximum 225,444,488 bytes.
- Battery operation: 51% at start, 25% at end. macOS reported no recorded thermal/performance warning. Concurrent authoring/type checks/growth studies were running on the shared machine, so this is not an isolated renderer-only thermal comparison. The run does not establish a specific cause of the slowdown.
- **FAIL** primary p95 GPU, final-ten-minute throughput, and total CPU targets. Full-panel, plugged-in, and paced-presentation modes remain unqualified. Nothing here establishes the million-occurrence gate.

## Growth, surface, and persistence implementation

Continuous wood rings and branch-aligned material coordinates now survive cooking and worker transfer. Needles have curved tapered triangular sections in the explicit source path. Wind lighting rotates smooth normals by the actual triangle deformation. Paper birch has leaf dimensions and a distinct procedural lenticel/sheet surface. These are implementations awaiting final specimen approval, not claims of calibrated botany or watertight junction unions.

Growth/prune preparation can use the existing bounded compiler worker pool; authoritative checkpoints feed meshing without a second developmental replay. Studio history inspection also runs in a worker. The runtime integration test grows, prunes, saves, reloads, continues, compares the realized tree, and verifies failed edits leave state unchanged. Reloads release obsolete generated growth realizations rather than accumulating aliases. Saves remain sparse: untouched population occurrences do not retain full graphs.

The earlier 1,024-case developmental sweep at `output/botanical-growth/1790179634138` had no invalid attachments/resources but **159 capacity-limited cases**. Preserve it as a failed model experiment. Nearby foliage had been omitted from light queries and assimilation did not scale with retained organ count. Revised local optical density derives from species organ area, nearby occlusion participates, and short shoots retain a leaf complement while lateral branching frequency depends on order. The revised sweep at `output/botanical-growth/1790180415908` has **1,024 cases, zero structural violations, zero capacity limits, maximum 4,818 shoots**. Both are uncalibrated structural studies; neither proves species distributions. Subsequent study commands now freeze their source automatically.

Grown pine uses four deterministic needle-field variants and independent full/half-retained cohort layers. Broadleaf and explicit needle losses honor cohort retention. Texture arrays prevent cross-module mip leakage, keep all filtering levels, and use the existing packed vertex stride. Array layer bytes participate in cooking, CPU ownership, worker transfers, and incremental GPU uploads. Dedicated upload tests exercise budgets smaller than a row and one texel.

## Crown experiment: rejected first candidate

An explicit `withVegetationCrowns` compilation path produces relightable coverage/orientation fields, source-organ ownership, exact payload byte counts, wind-domain metadata, and independent camera/light draw ranges. It is deliberately a **candidate**; the automatic selector rejects it, preserves source fallback, and checks light footprint and transform validity. View switches reset render history via draw-range identity. This is not yet branch-level mixed representation selection or forest-cell aggregation.

The first 64-view, 64-pixel field (32-pixel occupied extent) failed 11 of 12 held-out direction/size coverage cases; relative area losses were commonly 6–9%, and local coverage RMSE reached 0.099. Evidence: `output/vegetation-frontier/1790180488622-32167/source/output/browser-1790180500842-32170/frontier.json`. At the smallest footprint, some 16×/32× source references did not converge sufficiently either. The next experiment increases field resolution and reference sampling to 32×/64×. No crown has been qualified for radiance, shadows, motion, or production use.

## Current remaining work

CPU attribution and further hot-state optimization; calibrated species/reference dossiers and image review; exact 3D shoot-reference comparisons; a passing crown/cluster product; canonical forest-cell hierarchy and bounded streaming with complete fallbacks; growth transitions/local invalidation/competition across boundaries; the playable route and full acceptance scene/population/resolution/power-state matrix. The full plan is **not complete**.

The working tree contains concurrent unrelated rendering/water/authoring changes. Whole-workspace checks have intermittently failed in those files; rerun the required check on the final integrated source. Botanical worker/runtime/cooking/motion/coverage tests are recorded separately and do not waive that gate.

## Rendering improvements retained at the shape-first pivot

The later CPU profile exposed repeated typed-array callbacks, corner allocations in rigid bounds, and material packing during batch identity checks. Scalar comparisons, affine center/extents bounds, cached default creature data, and conservative source-material signatures reduced short-trial render CPU p95 to 1.2–1.3 ms. Constant-index specialization of the nine SH coefficients reduced short native-1080p control trials to GPU p95 5.64/5.57/5.64 ms and 170.1/171.1/169.6 completed fps (`output/vegetation-benchmarks/1790181922410-35683/source`). The fixed-image generic/specialized comparison changed six of 6,220,800 RGB channels by one 8-bit level, maximum one, RMS 0.000982 levels (`output/vegetation-material-review/1790182111025-36036/source`). These were short control trials; the prior failed thermal gate remains open.

A controlled alternating vegetation/no-vegetation trial before the SH change had matched-tick marginal GPU p95 3.015/2.884/2.884 ms, failing the 2 ms requirement (`output/vegetation-marginal/1790181799546-35270/source`). An earlier uncontrolled removal activated a different sun path and is invalid for attribution. The four-cluster crown experiment failed 6/12 static coverage cases, both with flat fields and with the optional depth experiment (`output/vegetation-frontier/1790181129745-33919/source` and `1790181223799-34072/source`). Neither is a production-qualified representation. The 32-layer thin-coverage diagnostic still shows meaningful bias (`output/thin-coverage/1790182277862-36682/source`); no thin-detail accuracy gate was closed.

## User-approved pivot: controlled pine architecture

The user rejected the developmental model's tree proportions and sparse crowns and approved direct control of morphology. The resource/light model remains available under **Experimental ecological growth** in Studio; it is no longer the production specimen strategy. Crown/cell expansion and ecological modeling are parked until a convincing specimen earns continuation.

Implemented an optional `conifer.architecture` source with trunk → main limb → secondary branch → leafy twig hierarchy. Existing sources retain their old generator. The new pine exposes twig pairs, twig length, supporting limb thickness and the start of foliage, alongside existing crown/whorl/taper controls. Coherent crown lobes, branch pitch and twig inclination break flat repeated tiers. Maturity advances a height front and a deterministic branch extension schedule with stable identities; old primary attachments remain in place within sampled-curve precision. Pruning and manual bends preserve sibling identities and descendant attachment. This is art-directed development, with no claim of calibrated age, carbon accounting, ecological competition, automatic regrowth or smooth animation.

Studio's **Use shaped pine** action is an undoable source replacement, explicitly resetting the incompatible old branch edits and prune IDs. The original lookdev specimen remains the control. The shaped specimen and review project are separate deliverables, not an automatic migration of saved trees.

The first capture was fuller but too flat/tiered (`output/vegetation-lookdev/1790183127754-38669/source`). The second introduces more branch pitch and twig volume. Current branch/tree/silhouette/grove captures: `output/vegetation-lookdev/1790183435598-39928/source/output/browser-1790183435753-39933`. Three seeds × three maturity stages: `output/vegetation-lookdev/1790183627304-40459/source/output/browser-1790183627495-40474`. Original-source captures through the same contemporary renderer: `output/vegetation-lookdev/1790183627304-40459/source/output/browser-1790183687279-40628`.

[Visual comparison, seed/stage gallery, and evidence](../research/pine-architecture-2026-09-23/index.html). Full-tree and grove cameras/lighting are matched; branch cameras track their differently oriented limbs. Needle dimensions and the maturity mapping differ intentionally. Source branch isolation is a review-only triangle filter, never a benchmark optimization. The new tree is a visual prototype pending user approval; fine-needle grain, branch junctions and canopy lighting are still visible limitations.

Short 15-second, one-trial native-1080p temporal tests, 3-second warmup, same frozen renderer (`output/vegetation-benchmarks/1790183505258-40164/source`):

| Scene | GPU p50 / p95 / p99 (ms) | Completed fps | Total CPU p95 (ms) | Max GPU bytes |
| --- | --- | ---: | ---: | ---: |
| Shaped grove | 10.88 / 13.30 / 14.35 | 78.25 | 3.0 | 254,885,937 |
| Original control | 8.00 / 11.34 / 12.52 | 102.65 | 2.6 | 228,966,776 |

Both are slower than earlier control trials; current system/power/concurrency conditions do not establish an isolated cause. Different submitted-frame counts also traverse different tick spans. These are exploratory workload results, not a clean matched-tick marginal comparison or a sustained qualification. The fuller grove costs more and **fails the performance target**. Its review captures contain 955,330 triangles versus 624,734 for the original grove at that camera. Do not equate source needle counts with explicit needle geometry; the filtered twig realization is complete, but the expensive whole-tree explicit reference can exceed its declared geometry budget.

Verification: 28 relevant tests pass, including nine seed/stage cooked round trips, finite motion attributes, bounded amplification, stable attachments, edits/pruning, legacy botanical regressions, and the existing growth save/reload path. TypeScript, browser TypeScript and workspace boundaries passed. The required full `bun run check` remains red on shared-workspace formatting/import/lint issues; scoped checks for the changed botanical files pass. This closes neither visual approval nor the full botanical/forest implementation plan.
