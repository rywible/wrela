# Neural capacity: change the representation, retain a conventional win

**Retire the widened mixed-colour polynomial model as an operating point.** Its modest rendered gains do not justify its cost. Keep the exact conventional source-history bypass and batched sequence capture. The next neural experiment should change what information is retained and how surfaces are represented before spending more training on this head.

First joint assessment: **26.07 minutes**. Explicit architecture decision: **35.75 minutes**. One targeted candidate followup, plus one cold-observation diagnostic; no further training after the first comparison. G1–G7 remain unchanged and unmet.

## Controlled capacity experiment

The input-resolution encoder grows from **48→16→16→12 (1,260 weights)** to **48→64→64→12 (8,076 weights)**. Its 48 causal features, overlapping bilinear RGB polynomial head, source-geometry history connection and 13-weight confidence gate remain the same. There is no new colour reference, future frame, case identity or absolute image position in inference.

Initialization duplicates each hidden unit four times, divides outgoing weights among copies and adds seeded zero-sum outgoing perturbations. This preserves the initial mathematical function while breaking training symmetry. A double-precision rollout verifies that identity, and four finite-difference directions check the changed packing. The already-audited recurrent graph and scorer are reused.

The wider network uses the same recorded joint-training recipe as the frozen small control: cases 0/3/5/7, all four scales, 32 cold frames, 256 updates, seed 3, view MSE plus opaque temporal MAE coefficient 1, Adam spatial/gate rates 0.001/0.01 decaying to one tenth, gradient norm cap 1. Old weights are immutable. The wider fit took **183.47 CPU seconds**, with 0 clipped steps. Final-epoch training objective changes from 0.0205470 to 0.0203418, about 1%; this is not the rendering decision.

New model: capacity64-1793247221-725123221. Packed spatial weights: **16,152 B**, versus 2,520 B small; gate data remains 64 B. Wider model SHA-256: 5f9487df39baa103c069ccda9322bab3bcd2ebbc9332af0483b2f9d3fc0f3eb3; gate: acf2e44e07dc57ea90295b5a9a045eff01b131a9db29481bf23b39ebd72061c9. These are experimental artifacts, not shipped/promoted weights.

The wider implementation initially exceeded the compiler's 256 KiB pipeline shader limit: 273 KiB, then two 263 KiB failures. All diagnostics and sources are preserved. Separating entries alone did not fix it. Explicit input/hidden vector math plus a looped output layer fits; its actual GPU cost is charged. No threshold, model shape or weight change was used to evade the failure.

## Held-out rendered quality

Eight cases 1/6/8/10/13/18/21/23 are excluded from fitting. They include signed motion speeds 1–32 pixels/frame and changed colours/geometry, but remain public development cases from one analytic family. All four scales are scored over 32 continuous cold-start frames at 192×108. Truth and visibility hashes match each paired baseline exactly.

| Case | Wider 4× FLIP | FLIP change vs small | LPIPS change | PSNR change dB | Temporal ratio | Reveal ratio |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 0.341413 | -1.375% | -0.139% | -0.0003 | 1.0167 | 1.0178 |
| 6 | 0.424351 | -1.272% | -0.049% | -0.0644 | 0.9752 | 1.0017 |
| 8 | 0.491254 | -1.425% | +0.563% | +0.0015 | 1.0077 | 0.9979 |
| 10 | 0.444370 | -1.383% | +1.348% | +0.1677 | 0.9752 | 0.9878 |
| 13 | 0.322667 | +1.165% | -1.624% | -0.0423 | 0.9724 | 0.9996 |
| 18 | 0.531681 | -0.231% | -2.521% | +0.1217 | 0.9905 | 0.9673 |
| 21 | 0.444653 | -0.353% | -1.717% | +0.0065 | 1.0049 | 0.9927 |
| 23 | 0.429940 | +0.148% | -2.690% | +0.0402 | 0.9787 | 0.9989 |


At 4×, the median paired FLIP improvement versus the small neural control is **0.813%**, LPIPS **0.882%**, and temporal error **1.543%**. FLIP regresses on cases 13/23; motion regresses on 1/8/21. Median absolute FLIP is **0.437155**, far from G1's target. All-scale, reveal, mixed/changed visibility and worst-frame data remain in the scored reports.

Fixed raw-source history often has much better spatial detail, at greater runtime; the wider model is temporally smoother in several cases, not uniformly better. Production temporal, the frozen small model, the older learned-patch/constant-history hybrid, the untrained source-history control and fixed raw-source control are all retained. The older constant-history hybrid is explicitly **not** called conventional because it contains learned spatial weights.

Conventional source-history and raw-source controls receive the same useful source geometry, motion/visibility knowledge and causal renderer observations. The raw method stores more causal samples and pays for them. No analytic source colours or integrated reference coverage reach these methods. This is the full-stack geometry-aware track, not a fixed-input SDK tournament. No extra source formula is used to handicap the learned comparison.

Wider GPU replay against the calibrated CPU implementation has worst mean named-view error 7.64e−7 and maximum 0.000365. Its cut at frame 16 agrees exactly with a fresh case at the same simulation time on frames 16/17 at all scales. Those checks establish execution/reset behavior, not complete G3 trail or reveal recovery.

## Complete rendering cost and actual residency

All timings are at 4× linear reconstruction. The complete analytic renderer includes low-resolution samples, encoder, history/geometry queries, decoding and diagnostic output/composition. Reference and training-observation passes are excluded because they are offline measurement work. All fixture resources, including unused production buffers, are counted in requested memory.

| Method / width | Complete GPU median / p95 ms | Requested live / peak B | Resident VRAM / GTT B | Browser PSS B |
| --- | ---: | ---: | ---: | ---: |
| Wider neural / 1920 | 13.58652 / 14.14452 | 56,364,404 / 64,658,804 | 103,436,288 / 19,001,344 | 573,840,384 |
| Wider neural / 3840 | 53.86832 / 57.10364 | 224,326,004 / 257,503,604 | 318,009,344 / 43,884,544 | 704,864,256 |
| Frozen small neural / 1920 | 3.22668 / 3.41228 | 56,323,508 / 64,617,908 | 94,515,200 / 18,509,824 | 589,079,552 |
| Frozen small neural / 3840 | 12.45480 / 13.02804 | 224,285,108 / 257,462,708 | 351,571,968 / 43,393,024 | 667,756,544 |
| Original untrained source-history / 1920 | 3.15624 / 3.36560 | 56,315,920 / 64,610,320 | 94,507,008 / 18,427,904 | 543,590,400 |
| Original untrained source-history / 3840 | 12.14584 / 12.59902 | 224,277,520 / 257,455,120 | 351,563,776 / 43,311,104 | 733,302,784 |
| Fixed raw-source history / 1920 | 18.49556 / 18.81872 | 89,493,804 / 97,788,204 | 127,705,088 / 18,427,904 | 587,612,160 |
| Fixed raw-source history / 3840 | 73.69632 / 75.24704 | 356,988,204 / 390,165,804 | 450,736,128 / 43,311,104 | 682,889,216 |
| Production temporal / 1920 | 6.68184 / 6.78400 | 48,021,584 / 56,315,984 | 95,133,696 / 18,427,904 | 588,565,504 |
| Production temporal / 3840 | 26.47468 / 26.82112 | 191,099,984 / 224,277,584 | 284,823,552 / 43,311,104 | 684,032,000 |
| Direct conventional source-history / 1920 | 2.02632 / 2.27716 | 48,021,584 / 56,315,984 | 95,125,504 / 18,427,904 | 543,860,736 |
| Direct conventional source-history / 3840 | 7.87388 / 8.18004 | 191,099,984 / 224,277,584 | 284,819,456 / 43,311,104 | 688,799,744 |


The wider model costs **13.58652 / 53.86832 ms** at 1080p/4K, versus **3.22668 / 12.45480 ms** for small. Its 1080p encoder alone costs 11.10804 ms. Requested allocations grow by only 40,896 B, but physical behavior is not predicted by weight bytes: wider VRAM is higher at 1080p and lower in the single 4K snapshot. No cause or compression win is inferred from that allocation variability.

Timing uses 120 measured frames after 60 warmup, one worker, sequential AMD Chrome runs. Saturated spans and paced p50/p95/p99 are preserved separately, as are CPU calls, initialization/warmup observations, counters and loaded bytes. Do not add pass medians. DRM is deduplicated by device/client and PSS covers the browser group. These are whole-client resident snapshots, not per-asset attribution or physical peaks; overlapping UMA pools must not be summed.

## A better conventional control

The untrained source-history control originally built 64-byte neural cells and extracted unused neural features. The followup replaces that work with four current-colour texture loads, the same software bilinear order, the same view roundtrip, and the identical source-geometry history validity and blending rule. It adds no input and learns nothing.

**All 1,024 rendered images are byte-identical**, along with all truth/mask files. Cut/reset comparisons are exact. Complete cost drops from **3.15624 to 2.02632 ms at 1080p**, and **12.14584 to 7.87388 ms at 4K**. Requested live/peak drops by **8,294,336 / 33,177,536 B** at those sizes. This is a real cost/memory improvement for the same imperfect image, not G1 qualification.

Five alternating pairs confirm the cleaned-up conventional strategy against its original executable:

- 1920 output width: **34.582% lower complete GPU time**, geometric paired-log 95% interval **34.075–35.014%**; CPU comparison inconclusive.
- 3840 output width: **35.266% lower complete GPU time**, geometric paired-log 95% interval **35.045–35.320%**; CPU comparison faster.

Intervals enumerate all 3,125 resamples of five paired log ratios and apply only to these runs. Baseline executable directories are copied before comparison, preserving original raw measurements.

The wider runtime code is then removed; its sources and trained artifacts remain archived. Fresh final retained builds give:

| Clean retained build | GPU median / p95 ms | Requested live / peak B | Resident VRAM / GTT B | Browser PSS B |
| --- | ---: | ---: | ---: | ---: |
| 1920 | 2.09824 / 2.34388 | 48,021,584 / 56,315,984 | 95,125,504 / 18,427,904 | 543,171,584 |
| 3840 | 7.86376 / 8.42936 | 191,099,984 / 224,277,584 | 318,382,080 / 43,311,104 | 667,675,648 |


The public runtime selector rejects later model/gate uploads and training-input exports once this conventional path is selected. Continuous captures select it automatically only with source geometry, positive history, no spatial/gate model and no training export. Production and learned paths retain their original execution.

## What capacity cannot fix with these observations

Using existing cold-frame captures, group pixels with equal four-cell encoder features/colours and polynomial query context. Restrict groups to the same case and output row: x/4 coordinates are binary-exact, and every grouped GPU prediction is identical. The history share is verified zero.

**48.30% of cold pixels belong to groups whose reference RGB varies by more than 1/255 in the named view.** The conditional minimum MSE for equal predictions within these groups is **0.00908441**, about **15.16%** of the wider model's observed cold MSE. This bounds only part of its error; it does not explain the remaining error or prove all larger networks futile.

One witness is case 18, row 67, columns 112 and 172: equal observation keys and equal output, but sharply different reference colours. A deterministic network receiving the same relevant observations cannot choose both reference answers. Additional causal observations or source information can change that constraint.

The first unrestricted grouping showed 0.00021535 prediction spread from cross-row query-rounding ambiguity. It is preserved and excluded from the bound. This is a first-frame named-view MSE diagnosis, **not a FLIP/G1 bound**, a full temporal impossibility result or a claim about all source-aware renderers.

## Workflow, retained code and next bet

The reconstruct tool now accepts **--shots 1,6,8,10,13,18,21,23** for continuous analytic captures. One executable build serves all cases; each case/scale starts with independent state. The old single-case path remains. Wide capture produced all 1,024 scored images from one build in 104.26 seconds including observation exports. No new reconstruction capture speedup ratio is claimed; the earlier 7.9× measurement applied to matched appearance captures.

After removing widened inference, **32 conventional, 32 small-neural and eight production images** plus their targets/masks match archived outputs exactly. Format and tool/runtime type checks pass. Frozen model/goals hashes remain exact; engine, content and appearance code are unchanged. The inherited benchmark edits remain unstaged.

**Decision: change the neural observation/output representation.** Preserve causal surface-separated detail and make useful source visibility/sampling information explicit. First test whether that removes the measured ambiguity against an equally informed conventional control; then train residual or routing decisions. Reuse the prior direct-history, source-geometry, confidence, recurrent and raw-kernel failure evidence. The fast small encoder is an execution foundation, not a qualified renderer.

Alternate back to a bounded G4/source-aware compressed base-field experiment next, then return to the neural representation question. Keep the two-hour first assessment, at most two candidate followups, one quiet AMD queue and immutable old models. No additional material-moment training is justified here.

Evidence: /home/ryanwible/worktrees/wrela-neural-frontier/build/neural-evidence/neural-capacity-v1.
Manifest SHA-256: `8d640fd5b9ea1d4ffabbc83693c36606a332be80b8821114a484299440cc8fd8`.
Solo Pop!_OS AMD RADV RENOIR PCI1002:164c, Ryzen5 5500U, Mesa26.1.6, Chrome154.0.8037.57; Wrela0.9.0/compilerde9b81e52a7747870dfbd5223cb8aeb41e584d23. Existing [metric/autodifferentiation sources](citations.md#neural-reconstruction-instrument) and [profiling semantics](toolchain.md) apply. No new external code or algorithm is imported.

G1–G7 remain unchanged/unmet. No sealed-suite, human-preference, generic 3D, full G3, production-game/G6 or world-leading claim follows from these diagnostic fixtures.
