# AAA priorities: implementation and retained evidence

This iteration implements the six cross-domain priorities through the existing authoring, compiler, runtime and Studio layers. It does **not** establish AAA artistic acceptance or world-leading performance. The rendered failures below are part of the result.

## Implemented tracks

| Priority | Working implementation | Boundary |
| --- | --- | --- |
| Shared integration scene and budgets | Alpine crossing with explicit portable/balanced output, CPU, GPU, resident-memory and installed-product budgets; persistent-renderer captures with frame-matched timestamps | Development targets; one Apple Metal adapter is not a hardware compatibility matrix |
| Compiler quality/performance frontier | Complementary geometric relief, residual normal bands and unresolved slope variance; optional rest-coordinate streams; matched radiance, silhouette, coverage, temporal-change and cost evidence | Finite reference and sampled cameras, with explicit uncertainty; no global optimum or complete fidelity certificate |
| Reusable construction | Parameterized masonry, arches, weathering history, debris, plant communities, biped anatomy and fitted continuous mantle; geological profiles and protected openings | Procedural recipes remain visibly unfinished; held-out invariants do not imply artistic approval |
| Agent alternatives | Shared inspect/propose/compare/adopt workflow across eight domains; source constraints, stale-candidate protection, undoable adoption, matched renders and request-to-complete latency | Recorded time includes real compilation/upload/render work; accepted variants, token cost and manual intervention remain unknown |
| Diffuse GI | Optional progressively built static irradiance cache, compiled triangle BVH, receiver visibility, Studio control and independent CPU reference | One diffuse geometry bounce, constant authored sky surrogate, no moving/transmissive transport or hardware ray tracing |
| Second composition | River-bend shelter authored through normal transactions over the same construction system; saved operations and actual compiled collision checks | Route clearance is checked, but this is not a finished playable level |

Thin foliage now compiles semantic needles/leaves to filtered occupancy plus curved carriers, separately from optical transmission. A cheap masked depth pass precedes expensive foliage shading. Atmosphere work adds ozone extinction and separates spherical lighting from the camera-resolved cloud product. The architecture and commands are documented in [the development loop](../architecture/aaa-authoring-loop.md).

## Eight-domain image review

[Gallery](../../output/authoring-lookdev/1790118404143-41411/index.html) · [manifest and observations](../../output/authoring-lookdev/1790118404143-41411/report.json)

All eight suites complete, with **130 PNGs** and retained motion evidence, from source fingerprint `598664393a8fa9859e08679cd9a180b66a4fc4565d6e7ded19795d72e9b26bfb`. Representative beauty, detail, silhouette and motion-contact images were inspected. All eight categories remain `needs-work`.

The mantle no longer separates into rectangular panels during the sampled walk. Architecture has dimensioned openings, jointed gate leaves and deposited rubble. Stone shows actual relief and residual detail. Canopies preserve substantially more overlapping coverage. Remaining failures include primitive faces, insufficient cloth folds, repetitive bark fluting, scalloped rock boundaries, synthetic ground/banks, foliage stipple, soft cloud contours and unfinished water response.

The world traversal completes all **24** settled frames with collision ready, zero blocked frames and observed streaming activation. Four initial frames are incomplete; measured total settling is **1.264 seconds**. The sampled GPU median/p95 is **14.16/30.61 ms** at 1024×768, with peak owned GPU allocation **234,649,321 bytes**. These spaced captures are not sustained FPS. [Raw traversal report](../../output/authoring-lookdev/1790118404143-41411/source/output/browser-1790118475256-41684/world-traversal.json).

## Lighting and compiler evidence

The shared variant workflow completes **24 alternatives across all eight domains**, with baseline plus three alternatives and three matched views per domain: **96 images**, no browser errors. Observed request-to-first-complete-frame ranges are 94–108 ms for vegetation, 459–485 ms for geology, 557–572 ms for performance, 1.17–1.35 s for creatures, 5.83–6.15 s for assemblies, 5.91–6.89 s for materials, 21.29–27.24 s for worlds and 21.08–22.42 s for environment variants. These are actual single-run observations with possible warm process caches, not latency guarantees or estimates of agent token cost. [Matched alternatives and timings](../../output/lookdev-snapshots/1790117334046-37285/source/output/browser-1790117334181-37295/index.html).

The final bounded-GI numerical gate passes: the closed box has **zero** measured leakage; maximum CPU/GPU discrepancy is **0.000129**, and rendered rebase discrepancy is **0.000123**. Against the independent one-bounce reference, radiance RMS is **0.01013** in the box and **0.00454** in the outdoor specimen. Builds take approximately **1.43/1.33 seconds** in this run. Probe interpolation remains an approximation and guarded interpolation has measurable GPU cost. [GI images and raw report](../../output/lookdev-snapshots/1790117006674-34204/source/output/browser-1790117006786-34222/gi-lookdev.json).

The hardware relief parity probe compares **1,728** cases with zero gate failures: maximum band discrepancy `9.14e-6`, residual-height discrepancy `3.24e-7 m`, slope-variance discrepancy `1.46e-6`. [Raw parity evidence](../../output/lookdev-snapshots/1790115406258-29686/source/output/browser-1790115426389-29778/surface-relief-parity.json).

The measured representation frontier includes five distance views, **20** image/HDR pairs and **30** frame-tagged GPU samples per candidate. Automatic selection has linear-radiance RMS **0.000735**, maximum silhouette displacement **1 pixel**, and temporal error-change RMS **0.001162** against the finite near reference. Timing ranges overlap, so **both candidates remain on the measured frontier**. Neither a universal winner nor an ideal-field fidelity bound is claimed. [Interactive comparison and raw data](../../output/lookdev-snapshots/1790117006674-34204/source/output/browser-1790117039602-34458/index.html).

The final thin-field probe uses **13 image cases** and nine coarse-footprint cases with 65,536 source salts each. For eight overlapping half-pixel sprays, independent-mask coverage is **0.3793** against **0.4088** reference coverage; correlated hardware alpha-to-coverage retains only **0.1546**. The production coarse sampler has maximum CPU/GPU discrepancy `1.03e-7`, adjacent-level error `5.97e-8`, and maximum observed mean-coverage bias **0.003663**. Single-layer noise, aggregate bias and temporal convergence remain open. [Raw thin-field report](../../output/lookdev-snapshots/1790117683430-38033/source/output/browser-1790117743594-38298/thin-coverage-lookdev.json).

Compiled cloud noise is slower for isolated random queries, but faster in the actual coherent 512×384 cloud kernel: **1.409 vs 3.670 ms** for thin cover and **1.671 vs 4.719 ms** for overcast. HDR RMS against the analytic-noise path is **0.001859/0.000287**, with maximum **0.019684/0.007935**. The 1,149,984-byte field remains optional; this evidence supports its default on this adapter, not a universal policy. Exact-phase periodicity passes. Stratified marching reduces visible bands but introduces obvious grain, so cloud artistic acceptance still fails. [Retained cloud measurements](cloud-noise-measured-2026-09-22.json) and [image review](../../output/lookdev-snapshots/1790117683430-38033/source/output/browser-1790117724868-38212/environment-lookdev.json).

## Retained failures and next acceptance work

The final runtime optimization removes repeated pinned-artifact accounting without changing geometry or budgets. On the identical 498-surface CPU profile, extraction median/p95 falls from **466.235/519.761 ms** to **2.217/2.547 ms**. Exact membership totals have bounded reuse and invalidate on resident changes. Relief GPU reference streams shrink from 92 to **24 bytes per vertex**, making total relief vertex storage **116 rather than 184 bytes** while ordinary geometry adds no stream. [Reproduction and measurements](../../output/world-memory/extraction-profile.md).

The held-out river-bend composition renders all three views at **640×480** through the standard scene lookdev tool, using the same ordinary authored project and compiler. The ruins remain block-like and the ground/cloud treatment is unfinished. This preview is not a pass at the 720p/1080p runtime budgets. [Second-composition report](../../output/lookdev-snapshots/1790118735023-42321/source/output/browser-1790118735296-42330/lookdev.json).

### Target-resolution budget results

Every completed camera retains 48 frame-matched samples. These are static views on Apple Metal 3, not simulation-inclusive gameplay FPS. Neither profile passes the complete contract.

| Composition/profile | Capture | GPU p95 across completed cameras | CPU p95 | Memory/readiness |
| --- | --- | --- | --- | --- |
| [Primary, portable](../../output/lookdev-snapshots/1790118735023-42321/source/output/browser-1790118857570-42630/alpine-slice-review.json) | 3/3 at 1280×720 output, 960×540 internal | 20.25–22.35 ms; meets 26 ms | 10.2–10.6 ms; misses 5 ms | Peak 127.90 MiB; fits 128 MiB |
| [Primary, balanced](../../output/lookdev-snapshots/1790118735023-42321/source/output/browser-1790118781382-42461/alpine-slice-review.json) | 3/3 at native 1920×1080 | 19.86–22.09 ms; misses 12 ms | 9.3–10.4 ms; misses 4 ms | Peak 245.29 MiB; fits 256 MiB |
| [River bend, portable](../../output/lookdev-snapshots/1790118580383-42060/source/output/browser-1790118580598-42071/alpine-slice-review.json) | 1/3 | Approach 21.69 ms | Approach 10.6 ms | Shelter camera refuses geometry at the 128 MiB limit |
| [River bend, balanced](../../output/lookdev-snapshots/1790118580383-42060/source/output/browser-1790118641770-42178/alpine-slice-review.json) | 1/3 | Approach 21.50 ms | Approach 10.3 ms | Shelter camera refuses geometry at the 256 MiB limit |

The same portable river-bend approach previously measured CPU p95 **370.4 ms** before cache accounting was fixed. The final **10.6 ms** includes extraction and render submission. Its remaining draw-submission cost, shelter residency and balanced GPU cost are explicit next bottlenecks. The fixed budgets were preserved; incomplete views were not counted as passes or hidden with missing measurements.

The real Studio verification passes **12** domain edit/undo/redo checks, exact save/reopen, source-preserving performance scrubbing, and the **Bounce lighting** button through completed baking and disabling. There are zero captured browser errors on Apple Metal 3. [Studio verification](../../output/lookdev-snapshots/1790118240206-40891/source/output/browser-1790118240345-40902/domain-studio-verification.json).

Final source validation: **890 tests pass, zero fail**, 1,415,855 assertions across 188 files using `bun test ./packages ./apps ./tools`. Both TypeScript targets, dependency boundaries and formatting checks pass; 98 existing non-null-assertion warnings remain. Studio, standalone player, worker and offline snapshot build successfully with 14 assets. The final eight-domain capture suite completes without failed categories; its separate artistic verdict remains `needs-work` throughout.

Earlier leaked-light images, missing coverage, upload failures and incomplete captures remain in their original source snapshots. Corrections include receiver-to-probe visibility, interpolation at probe planes, incremental coverage uploads, independent sample masks, continuous garment topology and valid shared-region variant operations. Full art acceptance still requires scene-specific refinement and repeated review in motion. Ordinary-hardware budgets, broad GI transport, realistic canopy appearance and authored player experience remain open requirements.
