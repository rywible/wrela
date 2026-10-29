# Actual source edits: a conventional rendering win from Wrela's source algebra

Continue **source-derived invariant caching for repeated localized edits**, with a full-update fallback for one-off changes. Retain native half precision and compact f32; retire compact half because its measured residency is worse than compact f32. These are bounded, opt-in experimental paths, not a generic edit system.

The [previous comparison](neural-field-lanes.md) found a field advantage only when replacing unchanged response caches. Actual in-place edits change that result: an informed native full updater costs **2.92860 ms**, beating direct field evaluation's **3.10864 ms**. Reusing the source's invariant terms then reduces complete edited frames to **2.20608 ms native** or **2.59572 ms compact**.

G1–G7 remain unchanged and unmet. The frozen neural models were not trained or altered; no neural, world-leading or G4 compression claim follows.

## Scope and fairness

First joint assessment completed in **23.09 minutes**; architecture decision in **62.31 minutes**. Exactly two candidate optimizations: invariant f32 sums, then half storage. A fixed full-recomputation arithmetic control was added to the first hypothesis after candidate measurements: without it, savings could incorrectly be attributed to caching instead of cheaper arithmetic. No third candidate search.

Ten predeclared layouts of 26,560 cards on the same 44-clump crown; all ten render each frame into a 320×128 mosaic. Translate clump 0 by **(1.0, −0.6, 0.5)** and reverse it. Radius, squash, source area, owner ranges, card count, wood, material laws and camera bounds stay fixed, with runtime checks.

Every method receives the same source plant, ownership, visibility, material, changed-clump identity and phase. Native and compact caches update existing buffers in place. There is no forced double allocation and no learned-method privilege.

Frames alternate view 133 / light 1 / edited source and view 141 / light 2 / base source. Native quality captures cover both views and all four lights for all ten layouts: **960 RGBA captures across twelve methods**. Scoring uses three predeclared view/light/source states and three backgrounds: **90 rows per method**. Light 3 at view 141 gives a second edited state.

The only truth extension answers the previous block's actual-edit uncertainty: **20 edited cases, 40 AA32/AA64 captures**, all converged under existing thresholds. Reuse ten completed base cases. The generator's algorithms and metric definitions did not change; temporary source-constructor substitutions are archived and removed from the ordinary fixture.

## What Wrela makes reusable

For an unmoved card at position p, write the soft union as:

- W(p) = sum of exp(−k d_i(p)).
- G(p) = sum of exp(−k d_i(p)) n_i(p).
- Surface response uses log(W)/k and normalize(G).

With only clump c moving, the source proves that all other owners' card positions and all other clump terms stay fixed. Store the four components of the sums excluding c; add c's current contribution on an edit. Cards owned by c change their query positions, so they still evaluate the complete field. Immutable sums avoid temporal subtraction and accumulated drift.

This is a useful consequence of having executable content and known dependencies. It is our own algebra applied to the existing plant field, not a novelty claim or an imported implementation.

The f32 cache adds **16 bytes/card = 4,249,600 B** for this population. Half storage adds **8 bytes/card = 2,124,800 B**. Source bounds check finite exponential ranges; half storage additionally checks a conservative sum bound below 60,000. Geometry, visibility and depth ordering are unchanged.

Both full-update controls also use source-compiled ownership/placement. The stronger raw control evaluates all 44 terms using the same bounded exponential, normalization and log arithmetic; it gets the same useful source information without the invariant table. Stable full-update controls remain numerical references.

## Shaded quality and silhouette errors

| Method | Pass / rows vs fastest native | Worst PSNR loss dB | Worst FLIP / LPIPS ratio |
| --- | ---: | ---: | ---: |
| Original native full update | 90 / 90 | 0.000017 | 1.000008 / 1.000036 |
| Original compact full update | 90 / 90 | 0.001367 | 1.000743 / 1.002370 |
| Direct source field | 90 / 90 | 0.001367 | 1.000743 / 1.002370 |
| Stale native (invalid) | 30 / 90 | 7.215419 | 1.810675 / 7.195352 |
| Compiled stable native full update | 90 / 90 | 0.000017 | 1.000008 / 1.000036 |
| Compiled stable compact full update | 90 / 90 | 0.001367 | 1.000743 / 1.002370 |
| Compiled raw native full update | 90 / 90 | 0.000000 | 1.000000 / 1.000000 |
| Compiled raw compact full update | 90 / 90 | 0.001366 | 1.000742 / 1.002366 |
| Native f32 invariant | 90 / 90 | 0.000019 | 1.000004 / 1.000017 |
| Compact f32 invariant | 90 / 90 | 0.001366 | 1.000742 / 1.002366 |
| Native half invariant | 90 / 90 | 0.065428 | 1.004304 / 1.019608 |
| Compact half invariant (retired) | 90 / 90 | 0.066226 | 1.004625 / 1.020954 |


The original and source-compiled stable full controls are byte-identical in all 80 captures for each renderer. Every valid method passes all 90 rows against all seven valid conventional controls. The stale negative passes only the 30 base-state rows and fails all 60 edited rows; its speed is invalid.

The native half candidate's worst LPIPS ratio against the strongest native control is below 1.020; worst PSNR loss is below 0.066 dB. F32 differences are much smaller. Ratios use the unchanged 1e−4 baseline floor; limits remain 1.05 for FLIP/LPIPS and 0.25 dB PSNR loss. No metric was relaxed.

Coverage is exact to the conventional control in the scored states. Absolute AA4 silhouette errors against converged truth remain: maximum coverage MAE **0.007154**, maximum per-case p99 coverage error **0.115259**, minimum soft IoU **0.965618**, and worst per-case p99 boundary distance **3.139277 pixels**. Preserving these errors does not satisfy G3.

The initial controls restore the original source exactly in all 120 base/reversal captures; all 120 edited captures change RGB, with observable coverage changes. The negative remains exactly stale in all 80 captures. Every final browser mosaic agrees with native captures within the established one-byte display tolerance.

## Complete frame cost and memory

| Method | GPU median / p95 ms | Requested live / peak B | Resident VRAM / GTT B |
| --- | ---: | ---: | ---: |
| Original native full update | 3.49828 / 3.53948 | 10,393,292 / 10,557,132 | 26,476,544 / 10,199,040 |
| Original compact full update | 4.04652 / 4.08260 | 3,823,724 / 3,987,564 | 17,784,832 / 10,133,504 |
| Direct source field | 3.10864 / 3.14004 | 1,698,932 / 1,862,772 | 18,051,072 / 10,133,504 |
| Stale native (invalid) | 1.80536 / 1.84696 | 10,393,292 / 10,557,132 | 26,480,640 / 10,035,200 |
| Compiled stable native full update | 3.01088 / 3.04548 | 10,394,700 / 10,558,540 | 26,300,416 / 10,199,040 |
| Compiled stable compact full update | 3.54872 / 3.58508 | 3,823,724 / 3,987,564 | 17,793,024 / 10,199,040 |
| Compiled raw native full update | 2.92860 / 2.97140 | 10,394,700 / 10,558,540 | 26,300,416 / 10,035,200 |
| Compiled raw compact full update | 3.46316 / 3.49920 | 3,823,724 / 3,987,564 | 17,793,024 / 10,133,504 |
| Native f32 invariant | 2.22144 / 2.32080 | 14,644,300 / 14,808,140 | 30,756,864 / 10,199,040 |
| Compact f32 invariant | 2.59572 / 2.63144 | 8,073,324 / 8,237,164 | 22,249,472 / 10,035,200 |
| Native half invariant | 2.20608 / 2.31236 | 12,519,500 / 12,683,340 | 26,505,216 / 10,199,040 |
| Compact half invariant (retired) | 2.59176 / 2.62652 | 5,948,524 / 6,112,364 | 26,386,432 / 9,936,896 |


Saturated GPU measurements include the source update, placement/decoding, visibility and hardware depth ordering, wood, shading, scratch, copies and compositing. No separate software sort is needed by these hardware-depth paths. Static initialization is excluded from these prepared-edit spans and charged separately below.

Requested live/peak counts include caches, scratch, fallback/dummy bindings, host rings and output banks, without subtracting common resources. Existing sample shading uses 1 KiB workgroup storage and 64 KiB global staging; the new update kernels add no shared workgroup arrays. The compiled native control's extra hierarchy nodes are counted.

Physical memory comes from deduplicated device/client DRM and browser-process PSS. These are whole-client resident snapshots, not per-asset attribution or physical peaks. The UMA pools overlap and must not be summed.

Compact half is rejected despite passing quality and halving requested invariant storage: a fresh observer finds **26,386,432 B resident VRAM**, versus **22,052,864 B** for compact f32 under the same observer. Both report 3,749,888 B GTT in that recheck. The unexplained driver/compiler allocation penalty is measured; it is not attributed to scratch without evidence.

Fresh observations of the cleaned retained paired builds:

| Retained build | Requested live / peak B | Resident VRAM / GTT B | Browser PSS B |
| --- | ---: | ---: | ---: |
| Native half invariant | 12,519,500 / 12,683,340 | 26,304,512 / 10,100,736 | 508,695,552 |
| Compact f32 invariant | 8,073,324 / 8,237,164 | 22,319,104 / 10,199,040 | 513,030,144 |


Five alternating pairs validate the final implementations:

- Compiled raw native full update versus Original native full update: **16.167% lower complete GPU time**, geometric paired-log 95% interval **16.089–16.250%**; CPU result inconclusive.
- Native half invariant versus Compiled raw native full update: **25.403% lower complete GPU time**, geometric paired-log 95% interval **24.324–25.833%**; CPU result inconclusive.
- Compact f32 invariant versus Compiled raw native full update: **11.669% lower complete GPU time**, geometric paired-log 95% interval **11.611–11.739%**; CPU result inconclusive.
- Compact f32 invariant versus Compiled raw compact full update: **25.345% lower complete GPU time**, geometric paired-log 95% interval **25.220–25.404%**; CPU result inconclusive.

The intervals enumerate all 3,125 resamples of five paired log ratios and apply only to these runs/workload. CPU improvement is not established. Compact f32 is also compared directly with the fastest native control, so its combined runtime/memory advantage does not depend on choosing the slower compact baseline.

| Method | Paced GPU p50 / p95 / p99 ms | CPU call p50 / p95 / p99 ms | CPU-start → GPU-complete p50 / p95 / p99 ms | Browser PSS B |
| --- | ---: | ---: | ---: | ---: |
| Original native full update | 4.049 / 4.391 / 4.479 | 1.820 / 2.012 / 2.202 | 8.245 / 8.870 / 9.069 | 502,561,792 |
| Original compact full update | 4.689 / 5.166 / 5.251 | 1.895 / 2.310 / 2.480 | 8.852 / 10.086 / 10.478 | 508,619,776 |
| Direct source field | 3.699 / 4.302 / 4.502 | 1.910 / 2.366 / 2.472 | 8.097 / 8.913 / 9.291 | 508,395,520 |
| Stale native (invalid) | 2.435 / 2.766 / 2.839 | 1.555 / 2.125 / 2.356 | 5.848 / 6.906 / 7.294 | 503,048,192 |
| Compiled stable native full update | 3.567 / 3.992 / 4.117 | 2.040 / 2.551 / 2.666 | 8.595 / 9.482 / 9.920 | 509,813,760 |
| Compiled stable compact full update | 4.143 / 4.478 / 4.530 | 1.970 / 2.261 / 2.423 | 8.472 / 9.136 / 9.723 | 510,955,520 |
| Compiled raw native full update | 3.584 / 3.857 / 3.971 | 1.850 / 2.420 / 2.528 | 7.478 / 8.620 / 9.011 | 510,571,520 |
| Compiled raw compact full update | 4.081 / 4.477 / 4.581 | 1.772 / 2.365 / 2.406 | 8.125 / 8.821 / 9.038 | 512,395,264 |
| Native f32 invariant | 2.625 / 2.909 / 2.962 | 1.825 / 2.261 / 2.428 | 7.017 / 7.880 / 8.078 | 510,035,968 |
| Compact f32 invariant | 2.827 / 3.521 / 3.702 | 1.803 / 2.387 / 2.563 | 6.938 / 7.660 / 8.590 | 490,698,752 |
| Native half invariant | 2.663 / 2.956 / 3.019 | 1.928 / 2.295 / 2.395 | 6.885 / 7.855 / 8.254 | 510,672,896 |
| Compact half invariant (retired) | 3.133 / 3.447 / 3.492 | 1.908 / 2.310 / 2.546 | 7.160 / 7.730 / 8.086 | 492,299,264 |


Following [profiling semantics](toolchain.md), saturated spans and paced timestamps are separate observations. CPU-start-to-GPU-completion includes queue and browser/driver notification, not presentation latency. Do not add pass medians.

## Preparation changes the choice

The edit target must be known before cooking invariant sums. A separate diagnostic rebuilds them in place before **every** actual source transition, then performs the update and entire frame. Both final frames are byte-identical to their prepared-cache counterparts.

| Candidate | Prepared edit frame ms | Preparation + edit + rendering ms | Preparation diagnostic p95 ms | Predicted reuse threshold |
| --- | ---: | ---: | ---: | ---: |
| Compact f32 invariant | 2.59572 | 3.82876 | 3.88492 | 2 updates |
| Native half invariant | 2.20608 | 3.46548 | 3.54996 | 2 updates |


These costs make full recomputation faster for a one-off edit. The two-update reuse thresholds are a simple cost model from separate saturated medians, not measured short bursts or arbitrary edit-target changes. Pipeline compilation is warm. Startup observations, preparation labels, full paced tails, counters and physical memory remain in preparation-assessment.json.

No automatic reuse policy is implemented. Callers should not assume these cache costs apply when another clump moves, several clumps change, or card query positions change. In those cases the invariant must be rebuilt or the full updater used.

## Decision, retained code and next bet

Retain the source-compiled conventional updater, native half invariant path, and compact f32 invariant path. Native f32 remains a precision control. Remove compact half response code and guard against selecting it; preserve its measured source and artifacts.

The ordinary authored content and frozen rendering configuration have edits disabled. Opt-in modes in content/great-tree/edit.wrela are: 0 stale negative, 1 original full update, 2 source-compiled stable full update, 3 f32 invariant, 4 native-only half invariant, 5 source-compiled raw full update. Phase is selected by illumination parity in this experiment; benchmark setters explicitly select the recorded view/light schedule. This fixture supports the recorded bounded translation, not general asset edits.

Next architecture bet: compile source dependencies into reusable response statistics with an explicit preparation, reuse and invalidation policy. The next consequential comparison should change which clump moves and which card query positions change, charge rebuilding/fallback, and give conventional controls identical information. Do not train another material-moment model until an observed rendered error justifies it.

The compact winner offers a modest measured joint advantage over native full recomputation during repeated edits, far from G4's **16×** resident appearance reduction. No independent-motion/reveal G3, 8–512 px, unseen-family, fixed-canopy tenfold-density or sustained G6 qualification. Ten layouts are not tenfold density.

## Verification and evidence

After removing the failed compact-half path, **32 edited captures**, **8 comparisons of 64 versus two alternating edits**, and **8 frozen-default captures** match both image and resource-count controls exactly. Fresh retained builds are profiled and observed separately. Formatting and type checks pass with the inherited Camera padding warning. Reuse unchanged AA2 checks; no broad suite.

The initial CPU dispatch wrappers incorrectly passed slices where shader binding required GPU buffer handles. This failed before execution; archived failure and repair preserve the evidence. Public wrappers now take GPU handles and expose slices only at shader binding. No failed timing or quality result was discarded.

The original repetition check compared 64 edits with an unconverted initial response. Native half differed only at its first light-0 capture (maximum linear difference 0.00605392456), with exact alpha/resources; the other three lights matched. That failed check is preserved. Comparing 64 versus two edits puts both images in the same response representation and is byte-exact. This demonstrates no accumulated drift for this schedule, while retaining the one-time rounding change as an unresolved G3 qualification issue.

Solo Pop!_OS AMD RADV RENOIR PCI 1002:164c, Ryzen 5 5500U, Mesa 26.1.6, Chrome 154.0.8037.57; Wrela 0.9.0 compiler de9b81e52a7747870dfbd5223cb8aeb41e584d23. Timing: 240 frames after 60 warmup, one worker; allocations/completion: 300 paced frames. GPU runs are sequential, with no CPU metric scoring overlap. No training, subagents, RTX or purchases.

Evidence directory: /home/ryanwible/worktrees/wrela-neural-frontier/build/neural-evidence/g4-source-edit-v1.
Manifest SHA-256: `c1bd9136294e1c43d86400bff808c89e0bb27045dfc442a214a0cd344a4fb4ed`.

All twelve methods, failures, images, references/convergence, paired samples, preparation costs, requested allocations, DRM/PSS and replay sources are preserved. Parent field-lane manifest: cd936529f0af25926f3c1465f2e46395282c909105f466afec8395f9ecefc79f. Reused population/reference manifest: 38eb09675763f99ed58c6c3136f127a3f0f6000bbb259cefeea51d3374a9a3e3. Existing [material/metric citations](citations.md#neural-reconstruction-instrument) apply. No new outside algorithm or implementation imported.
