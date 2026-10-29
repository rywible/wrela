# Changing source edits: immutable base sums and a faster experiment loop

Continue immutable authored-base field sums for **repeated changing and simultaneous translations**. Retain native Oct12 and compact f32; keep full recomputation for one-off edits. Retire compact Oct12 because actual GPU residency is worse, and retire target-specific rebuilding as an operating point. These are bounded experimental fixtures, not a general edit system.

The source-aware conventional win survives changing targets: the fastest informed full updater costs **2.93024 ms**, versus **2.21216 ms native Oct12** and **2.60140 ms compact f32**. Preparation and memory prevent a general win: with preparation charged on every update, the candidates cost **3.28980 / 3.65432 ms**. G1–G7 remain unchanged and unmet; no training or neural advantage in this block.

## Scope, decision and fairness

First joint assessment completed in **33.13 minutes**; explicit architecture decision in **63.31 minutes**. Exactly two candidate followups: immutable f32 base sums, then a 12-byte direction/magnitude/weight encoding. Batched capture is a subsequent workflow change, not a third rendering optimization.

Ten layouts of 26,560 cards share the same 44-clump crown. All ten render each 320×128 frame. Five authored-base states are base, clump 0 translated (1, −0.6, 0.5), clump 3 translated (−0.7, −0.4, 0.8), clump 7 translated (0.5, −0.8, −0.6), and all three translations. These are absolute snapshots from the authored base, not cumulative arbitrary edits. Radius, squash, source areas, owner ranges, counts, wood, material laws and bounds stay fixed.

The phase cycles modulo five, view alternates 133/141 and light cycles every two frames. The resulting 40-frame cycle decouples source phase from view and illumination. Every control receives the same source, compiled ownership/placement, changed-clump list, visibility and sampling information. The raw full updater uses the same bounded exponential arithmetic as the candidate; direct field evaluation pays its node updates. No input advantage is withheld from conventional methods.

Quality uses predeclared layouts 0/5/8 (seeds 7/101/251), all five phases, two view/light cases and three backgrounds: **90 rows per method**. Ten methods produce **1,760 original RGBA captures**. Phase four additionally covers the other seven layouts to verify complete browser mosaics. Only a blocking edited-truth uncertainty justified references: **21 new cases / 42 AA32+AA64 captures**, with nine completed cases reused. All 30 converge under unchanged thresholds. Temporary source-constructor substitutions are removed.

## Why immutable source sums help

For each fixed card query p, store the authored source's W = sum(exp(−k d_i(p))) and G = sum(exp(−k d_i(p)) n_i(p)). The current source response follows from log(W)/k and normalize(G).

For an unmoved owner, add each changed clump's current contribution and subtract its authored contribution from the immutable base sum. Cards belonging to a moved clump change their query positions and use the complete current field. This exploits known source dependencies without accumulating edits into history. The authored query is regenerated directly from source parameters; subtracting a current offset to recover it would introduce an extra rounding step.

F32 uses **16 bytes/card = 4,249,600 B** of base sums. Oct12 uses Oct32 direction plus f32 magnitude and weight: **12 bytes/card = 3,187,200 B**. Source bounds guard finite exponential ranges; the normalization guard is unchanged. These source identities are exact algebra, but finite-precision subtraction and Oct32 quantization are measured approximations. This is our own application of the existing field, not an external novelty claim.

## Rendered quality and silhouettes

| Method | Pass / rows vs fastest native | Worst PSNR loss dB | Worst FLIP / LPIPS ratio |
| --- | ---: | ---: | ---: |
| Native full raw update | 90 / 90 | 0.000000 | 1.000000 / 1.000000 |
| Compact full raw update | 90 / 90 | 0.000240 | 1.000306 / 1.001817 |
| Direct source field | 90 / 90 | 0.000240 | 1.000306 / 1.001818 |
| Native target-specific rebuild | 90 / 90 | 0.000008 | 1.000003 / 1.000019 |
| Compact target-specific rebuild | 90 / 90 | 0.000240 | 1.000306 / 1.001817 |
| Stale native (invalid) | 18 / 90 | 10.120604 | 2.703491 / 8.288316 |
| Native immutable f32 | 90 / 90 | 0.000012 | 1.000003 / 1.000014 |
| Compact immutable f32 | 90 / 90 | 0.000240 | 1.000306 / 1.001817 |
| Native immutable Oct12 | 90 / 90 | 0.012425 | 1.003525 / 1.000389 |
| Compact immutable Oct12 (retired) | 90 / 90 | 0.012433 | 1.003527 / 1.001817 |


Every valid method passes all 90 rows against all five conventional controls. Stale native passes only the 18 base rows and fails all 72 edited rows. Unchanged gates are at most 0.25 dB PSNR loss and 1.05 FLIP/LPIPS ratios with a 1e−4 floor.

All 1,584 valid-method original captures have finite RGBA and exact native-control alpha. Scored absolute AA4 errors remain: maximum coverage MAE **0.007324**, maximum per-case p99 coverage error **0.111511**, minimum soft IoU **0.965966**, and worst per-case p99 boundary distance **2.944996 pixels**. Preserving these errors does not meet G3. Final browser mosaics match native tiles within the established one-byte display tolerance.

## Complete cost and actual memory

| Method | GPU median / p95 ms | Requested live / peak B | Resident VRAM / GTT B |
| --- | ---: | ---: | ---: |
| Native full raw update | 2.93024 / 2.98128 | 10,394,700 / 10,558,540 | 26,300,416 / 10,199,040 |
| Compact full raw update | 3.47028 / 3.50928 | 3,823,724 / 3,987,564 | 18,259,968 / 10,297,344 |
| Direct source field | 3.11240 / 3.14348 | 1,698,932 / 1,862,772 | 17,784,832 / 10,199,040 |
| Native target-specific rebuild | 3.56492 / 3.64740 | 14,644,300 / 14,808,140 | 30,756,864 / 10,133,504 |
| Compact target-specific rebuild | 3.90148 / 3.94776 | 8,073,324 / 8,237,164 | 22,253,568 / 10,297,344 |
| Stale native (invalid) | 1.86212 / 1.98672 | 10,394,700 / 10,558,540 | 26,476,544 / 10,035,200 |
| Native immutable f32 | 2.25176 / 2.30528 | 14,644,300 / 14,808,140 | 30,560,256 / 10,035,200 |
| Compact immutable f32 | 2.60140 / 2.72984 | 8,073,324 / 8,237,164 | 22,249,472 / 10,199,040 |
| Native immutable Oct12 | 2.21216 / 2.30952 | 13,581,900 / 13,745,740 | 26,763,264 / 10,035,200 |
| Compact immutable Oct12 (retired) | 2.59712 / 2.74012 | 7,010,924 / 7,174,764 | 26,189,824 / 10,199,040 |


Saturated spans include all current-source updates, decoding/placement, visibility and hardware depth ordering, wood, shading, scratch, copies and compositing. These paths need no separate software sort. Initial immutable-cache preparation is excluded here and measured below. Target-specific rebuilding is included in its own full-frame spans.

Requested live/peak covers caches, scratch, fallback/dummy bindings, host rings and outputs without subtracting common resources. Sample shading also uses 1 KiB workgroup storage and 64 KiB global staging; the new update kernels add no shared arrays. Actual DRM device/client residency and browser PSS are whole-client snapshots, not per-asset attribution or physical peaks. Do not sum overlapping UMA pools.

Fresh retained-build observations:

| Retained build | Requested live / peak B | Resident VRAM / GTT B | Browser PSS B |
| --- | ---: | ---: | ---: |
| Native immutable Oct12 | 13,581,900 / 13,745,740 | 26,308,608 / 10,100,736 | 512,250,880 |
| Compact immutable f32 | 8,073,324 / 8,237,164 | 22,249,472 / 9,936,896 | 492,789,760 |


Compact Oct12 is rejected despite passing quality and reducing requested storage. A fresh recheck finds **26,386,432 B VRAM**, versus **22,249,472 B** for compact f32. GTT is 10,035,200 / 10,100,736 B; PSS is 491,659,264 / 491,523,072 B. The cause is unattributed, not asserted to be shader scratch. Native Oct12 avoids native f32's larger observed allocation.

Five alternating pairs per comparison validate the cleaned implementations:

- Native immutable Oct12 versus Native full raw update: **24.342% lower complete GPU time**, geometric paired-log 95% interval **23.766–24.576%**; CPU result inconclusive.
- Compact immutable f32 versus Native full raw update: **11.575% lower complete GPU time**, geometric paired-log 95% interval **11.337–11.672%**; CPU result inconclusive.
- Compact immutable f32 versus Compact full raw update: **24.977% lower complete GPU time**, geometric paired-log 95% interval **24.910–25.186%**; CPU result inconclusive.

The intervals enumerate 3,125 resamples of five paired log ratios and are conditional on these runs/workload. CPU improvement is not established. Comparing compact f32 directly with the fastest native full updater avoids a favorable but slower baseline.

| Method | CPU call p50 / p95 / p99 ms | CPU-start → GPU-complete p50 / p95 / p99 ms | Browser PSS B |
| --- | ---: | ---: | ---: |
| Native full raw update | 1.787 / 2.215 / 2.414 | 7.515 / 8.675 / 8.951 | 510,039,040 |
| Compact full raw update | 1.940 / 2.351 / 2.571 | 8.453 / 9.932 / 10.194 | 514,871,296 |
| Direct source field | 1.838 / 2.281 / 2.476 | 7.847 / 9.749 / 10.048 | 511,581,184 |
| Native target-specific rebuild | 1.740 / 2.261 / 2.410 | 8.260 / 9.246 / 9.718 | 512,626,688 |
| Compact target-specific rebuild | 1.987 / 2.371 / 2.543 | 8.710 / 9.602 / 9.925 | 494,601,216 |
| Stale native (invalid) | 1.325 / 1.716 / 1.861 | 5.870 / 6.586 / 6.756 | 508,603,392 |
| Native immutable f32 | 1.888 / 2.356 / 2.485 | 7.165 / 7.760 / 7.924 | 511,755,264 |
| Compact immutable f32 | 1.958 / 2.313 / 2.530 | 7.345 / 8.146 / 8.511 | 492,237,824 |
| Native immutable Oct12 | 1.375 / 1.881 / 1.953 | 6.438 / 7.075 / 7.446 | 491,009,024 |
| Compact immutable Oct12 (retired) | 1.810 / 2.165 / 2.474 | 7.368 / 8.060 / 9.054 | 491,738,112 |


CPU-start to GPU-complete includes queue and browser/driver notification, not presentation. Saturated GPU spans and paced observations are separate. Initial profile.json and assessment.json are preserved. The comparison tool overwrote raw paced files inside the two compared controls' profile-build/results directories; those now contain the last paired run. Initial raw paced GPU tails for those controls are not claimed reproducible from those overwritten files. Pair samples remain in each result.json; allocation-run retains separate completion/memory observations. Future queues must copy baseline executable directories before comparison.

## Preparation and reuse

Rebuilding the authored-base cache before every transition, then performing updates and the full frame, costs **3.28980 ms native Oct12 (p95 3.36648)** and **3.65432 ms compact f32 (p95 3.79560)**. Both final frames are byte-identical to prepared-cache counterparts. Pipelines are warm; these are not cold compiler costs.

Full recomputation wins for one-off changes. A simple cost model predicts two reused updates for native Oct12 versus native full update, two for compact f32 versus compact full update, and four for compact f32 versus the fastest native update. These are estimates from separate saturated medians, not measured burst lengths. Preparation-assessment.json preserves initialization, label, paced-tail, allocation and physical observations.

Retain both immutable operating points, native f32 as a precision control, full raw fallback, and the low-memory direct-field control. Mode 2 target-specific rebuild remains only as a measurement control; compact Oct12 response code is removed and selection guarded. All ordinary edit flags are disabled and frozen source/data/config restored.

## Batched captures: measured workflow improvement

The new command is:

`wrela tool appearance -- runtime-batch PHASES LAYOUTS VIEW_A VIEW_B [HISTORY]`

One build and initialization emits all requested source phases, layouts, two views and four lights, with per-image resource counts. Runtime limits reject unsupported phase/layout ranges. Optional history performs bounded source transitions for drift checks. The same readback implementation serves legacy captures; timed rendering and reference generation are unchanged.

A matched 32-image local comparison takes **30.573 s across eight legacy builds** versus **3.877 s in one batch**, an observed **7.89× workflow speedup**. All images and resource counts are exact. Larger retained batches produce 400 images each in 6.071 / 6.381 s. This is one local workflow comparison, not a rendering or statistical throughput claim.

The retained batch verifies **352 existing images/resources exactly**. Another **80 images after 40 permuted source transitions** match fresh batch outputs. Eight default captures match the parent in images and resource counts. Formatting/typechecks pass with the inherited Camera warning. Existing unchanged AA2 checks are reused.

Prepare candidates and their source snapshots ahead of one serialized AMD measurement queue. Capture all cases per executable, then score away from timed profiles. Do not overlap GPU measurements with builds, training or CPU scoring; more queued candidates do not remove the need for a fair joint decision.

## Next architecture-level bets

The next work block returns to **neural reconstruction quality**. Use the fast input-resolution patch encoder as an execution foundation, not a qualified model. First isolate capacity with a controlled increase over its 1,260 weights while fixing inputs/output representation and evaluation. Reuse the recorded direct-history, source-geometry, learned-confidence, recurrent/joint-training and raw-kernel failures. Test whole-case held-out continuous motion, reveals and cuts, spatial quality, full AMD runtime and actual residency. Freeze old models; two-hour first assessment and at most two targeted followups, then continue/change/retire.

Alternate bounded neural blocks with source-aware conventional G4 work. The next G4 question is whether source-bounded compressed base-field evaluation can replace per-card sums, keeping changed-term corrections and full fallback and giving controls identical source information. Per-card cache memory remains the obstacle; do not spend another block shaving it indefinitely before testing neural capacity.

No 16× resident appearance compression, independent G3 motion/reveal, 8–512 px, unseen-family, tenfold fixed-canopy density or sustained G6 qualification. Ten layouts are not tenfold density. G1–G7 and all frozen optical/reconstruction models remain unchanged.

## Evidence

Solo Pop!_OS AMD RADV RENOIR PCI 1002:164c, Ryzen 5 5500U, Mesa 26.1.6, Chrome 154.0.8037.57; Wrela 0.9.0 compiler de9b81e52a7747870dfbd5223cb8aeb41e584d23. Timing uses 240 frames after 60 warmup, one worker; memory/completion uses 300 paced frames. No training, subagents, RTX or purchases in this block.

Evidence: /home/ryanwible/worktrees/wrela-neural-frontier/build/neural-evidence/g4-changing-edits-v1.
Manifest SHA-256: `6046addafe6c0e8f799e68fbd4509290255df0373ae9ac04030d72cab67f245d`.
All ten candidates, including failed operating points, reference convergence, preparation costs, physical observations and replay sources remain preserved. Parent source-edit manifest c1bd9136294e1c43d86400bff808c89e0bb27045dfc442a214a0cd344a4fb4ed; population/reference manifest 38eb09675763f99ed58c6c3136f127a3f0f6000bbb259cefeea51d3374a9a3e3. Existing [metric citations](citations.md#neural-reconstruction-instrument) and [profiling semantics](toolchain.md) apply; no new external algorithm or code imported.
