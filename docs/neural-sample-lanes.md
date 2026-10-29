# Exact sample lanes: a stronger conventional rendering baseline

**Continue the exact sixteen-lane AA4 source renderer with 64 KiB pixel staging.** Five alternating paired AMD runs reduce complete GPU time by **29.62% for native cached cards** and **27.74% for the compact exact response cache**, with byte-identical images. Both conventional methods receive the optimization. CPU improvement is inconclusive. This is a measured rendering win in the bounded fixture, not neural superiority, G4 completion or world-leading evidence.

First assessment: 16.82 minutes. Architecture decision: 26.43 minutes. Verification: 36.76 minutes. Exactly two optimization follow-ups; zero training or new reference captures. [G1–G7](neural-rendering-goals.md) and frozen models remain unchanged.

## Why this experiment followed the failed representation

The [uniform response packing experiment](neural-response-statistics.md) showed that even small normal errors can fail rendered quality. Inspection then found a different cost: the existing source resolve shades all sixteen AA4 samples serially in one invocation. Native sample shading accounts for about 1.33 ms of a 2.57 ms complete frame.

Wrela exposes the exact source visibility, material evaluation and sampling order in one codebase. We can change execution while keeping the numerical contract: identical visible surface IDs, identical per-sample material calls and half-float rounding, and the original y-major sixteen-sample sum. The strongest practical native cached-card control and the smaller exact response-cache control both get that treatment; neither is denied useful source information.

This uses the existing ten predeclared leaf layouts of one 44-clump crown, 26,560 cards per layout, two difficult scored views/lights, three backgrounds and 20 completed converged references. Every measured frame renders all ten 64×64 crowns into a 320×128 mosaic. It is not G4's fixed-canopy tenfold density workload.

## Initial measurement and two follow-ups

Four or sixteen invocations per output pixel shade independent samples into a 1 MiB global buffer, shared across layouts. A gather reads them in the original order. Every initial candidate is byte-identical to its serial parent. Sixteen lanes are faster, but the gather costs about 0.29 ms, and compact-cache observed residency rises by roughly 8 MB for the additional 1 MiB allocation.

1. **Reduce within the workgroup.** A 64-invocation workgroup shades four pixels. Its sixteen lanes per pixel place exact radiance in 1 KiB of shared storage. The pixel leader sums in the original order and writes four uncontended atomic words. A small copy reconstructs the existing vec4 output. Global staging falls to 64 KiB; gather cost falls to roughly 0.046 ms. All 160 follow-up images remain byte-identical.
2. **Suppress identical leaf samples.** Equal leaf IDs within one pixel can share exact radiance only when tissue is unresolved; wood, distinct IDs and resolved leaves remain independent. Shared key and owner records avoid the earlier failed private-array design. All 160 outputs remain exact, but native GPU cost rises from 1.821 to 1.980 ms and compact from 2.378 to 2.554 ms. Coordination costs exceed savings. This implementation is retired and preserved; no third optimization was attempted.

| Method | Saturated GPU median / p95 ms | Requested live / peak B | Resident VRAM / GTT B |
| --- | ---: | ---: | ---: |
| Native serial | 2.56848 / 2.60780 | 10,327,756 / 10,491,596 | 26,271,744 / 9,936,896 |
| Native, four lanes / 1 MiB staging | 2.40740 / 2.48444 | 11,376,332 / 11,540,172 | 26,738,688 / 10,100,736 |
| Native, sixteen lanes / 1 MiB staging | 2.18240 / 2.26760 | 11,376,332 / 11,540,172 | 26,218,496 / 10,100,736 |
| Native, sixteen lanes / 64 KiB staging | 1.82084 / 1.88512 | 10,393,292 / 10,557,132 | 26,480,640 / 10,199,040 |
| Native, shared duplicate suppression (retired) | 1.97984 / 2.03584 | 10,393,292 / 10,557,132 | 26,542,080 / 9,936,896 |
| Compact exact cache, serial | 3.29456 / 3.33116 | 3,758,188 / 3,922,028 | 18,038,784 / 10,199,040 |
| Compact, four lanes / 1 MiB staging | 2.95376 / 3.00976 | 4,806,764 / 4,970,604 | 26,361,856 / 10,035,200 |
| Compact, sixteen lanes / 1 MiB staging | 2.68100 / 2.76824 | 4,806,764 / 4,970,604 | 26,361,856 / 10,100,736 |
| Compact, sixteen lanes / 64 KiB staging | 2.37780 / 2.40796 | 3,823,724 / 3,987,564 | 17,776,640 / 10,100,736 |
| Compact, shared duplicate suppression (retired) | 2.55376 / 2.58196 | 3,823,724 / 3,987,564 | 18,235,392 / 10,100,736 |


The winning path adds exactly 65,536 B of requested global storage, plus 1,024 B of workgroup storage. The table counts all observed requested buffers/textures, scratch, caches, fallback data, host rings and output banks. No common-resource subtraction is used. Actual driver residency rejects the large staging option for the compact cache; the 64 KiB version returns near its original resident level.

## Paired performance and complete costs

Five alternating pairs compare the retained implementation against each original serial build. Native median paired saving is 29.6196%; the geometric paired-log bootstrap 95% interval is **28.6226–29.8305%**. Compact saving is 27.7395%, interval **27.6959–27.8667%**. All 3,125 resamples of the five pairs are enumerated. These intervals are conditional on this workload and these runs, not universal population/hardware guarantees. The CPU comparison is inconclusive for both.

Complete spans include source placement and decoding, visibility/depth ordering, wood, sample shading, scratch writes/reads, view/light changes, copies and final compositing. These hardware-depth controls need no separate sort. Pass medians are not summed to claim total cost.

| Method | Paced GPU p95 / p99 ms | CPU start → GPU completion p95 / p99 ms | Browser group PSS MB |
| --- | ---: | ---: | ---: |
| Native serial | 3.445 / 3.565 | 7.926 / 8.214 | 497.492 |
| Native, four lanes / 1 MiB staging | 3.323 / 3.386 | 7.550 / 7.788 | 499.306 |
| Native, sixteen lanes / 1 MiB staging | 3.052 / 3.086 | 7.670 / 7.935 | 500.908 |
| Native, sixteen lanes / 64 KiB staging | 2.955 / 3.138 | 8.560 / 8.919 | 500.736 |
| Native, shared duplicate suppression (retired) | 2.738 / 2.850 | 7.751 / 7.969 | 500.402 |
| Compact exact cache, serial | 4.437 / 4.676 | 9.591 / 9.963 | 501.794 |
| Compact, four lanes / 1 MiB staging | 3.981 / 4.038 | 8.025 / 8.178 | 506.016 |
| Compact, sixteen lanes / 1 MiB staging | 4.063 / 4.113 | 9.110 / 9.299 | 505.639 |
| Compact, sixteen lanes / 64 KiB staging | 3.196 / 3.236 | 7.728 / 7.930 | 504.417 |
| Compact, shared duplicate suppression (retired) | 3.933 / 3.980 | 7.996 / 8.553 | 505.111 |


Per [toolchain semantics](toolchain.md), profile.json reports saturated GPU spans; profile-build/results/timings.json reports a subsequent paced run. CPU-start-to-GPU-completion includes browser/driver notification and is not input-to-photon. Measurements use 240 frames after 60 warmup frames, one worker; this is not sustained G6 qualification.

DRM observations deduplicate PCI/client identifiers; VRAM/GTT are whole-client end-of-run resident snapshots. PSS covers the browser process group. Do not sum overlapping UMA pools, call requested peaks physical peaks, or infer per-asset physical attribution.

Fresh observations of the cleaned retained paired builds give native 10,393,292 B requested live / 10,557,132 B peak, 26,542,080 B resident VRAM / 10,199,040 B GTT, 502,646,784 B PSS. Compact gives 3,823,724 / 3,987,564 B, 17,780,736 / 10,199,040 B, 507,560,960 B PSS. No hidden resident regression appears at the scale seen with large staging; no 16× memory claim follows.

## Full cache replacement every frame

The source cache is fully reconstructed and replaced before every complete population frame, with the previous cache live during construction. Native and compact controls use the same replacement policy as their optimized counterpart. All four final browser frames remain byte-identical to steady parents.

| Method | Saturated GPU median / p95 ms | Paced GPU p95 / p99 ms | Requested live / peak B | Resident VRAM / GTT B |
| --- | ---: | ---: | ---: | ---: |
| Native serial | 4.50252 / 4.58688 | 4.970 / 5.080 | 10,327,756 / 18,826,996 | 26,640,384 / 10,231,808 |
| Native, sixteen lanes / 64 KiB staging | 3.74344 / 3.82464 | 4.000 / 4.266 | 10,393,292 / 18,892,532 | 26,451,968 / 10,395,648 |
| Compact exact cache, serial | 5.00860 / 5.05288 | 5.564 / 5.713 | 3,758,188 / 5,883,044 | 35,016,704 / 10,330,112 |
| Compact, sixteen lanes / 64 KiB staging | 4.11744 / 4.16928 | 4.650 / 4.770 | 3,823,724 / 5,948,580 | 34,660,352 / 10,395,648 |


Native complete GPU cost improves from 4.503 to 3.743 ms, compact from 5.009 to 4.117 ms. These are descriptive refresh measurements, not additional paired confidence intervals. They measure complete invalidation/reconstruction cost for unchanged source, not actual edited geometry, incremental invalidation or temporal source-edit correctness. Raw completion tails, startup observations, counters and residency are in refresh-assessment.json.

## Quality and retained implementation

All **640 candidate RGBA captures** match their respective serial controls exactly. All candidates pass **60/60** scored comparisons against both controls. The ten variants preserve 800 captures; the two controls also match 160 prior captures exactly. This preserves the original silhouettes and existing AA4 errors; it does not improve or complete G3.

The retained [source renderer](../studio/appearance/runtime/source_control.wrela) uses sixteen lanes only for AA4. Other AA counts keep the original serial path. The [runtime](../studio/appearance/runtime/main.wrela) allocates the 64 KiB staging only for that active source mode. [Settings](../studio/appearance/runtime/sample_lanes.wrela) retain a serial diagnostic control and an opt-in full-refresh measurement switch. The large global staging and duplicate-suppression implementations are removed from live code, with complete archives retained.

Verification compares 32 cleaned AA4 outputs/resource snapshots, eight AA2 fallback pairs, and eight frozen default renderer pairs. All are exact. The ordinary mode 2/AA2 fixture, single-layout default, frame loop and frozen payload are restored. Existing format/type checks pass; no unrelated tests or new reference infrastructure were added. Engine and reconstruction code are unchanged.

Preserved setup failures: a missing metadata key before implementation, a generated function header that included the compute annotation, and an attempted over-domain dispatch for a shared-memory kernel. Repairs use the sealed model hash, correct header extraction, and explicit group dispatch. They did not alter an experimental hypothesis or permit extra optimizations.

## Next architecture-level bet and limits

Use this stronger executor to remeasure exact source-field and compact source-residual evaluation. Their earlier cost tradeoffs were measured with serial sample shading; the new execution architecture may change which representation is viable. Compare them against these **upgraded conventional caches**, including all scratch and refresh costs. No further material-moment training is justified without a specific rendered failure.

This block does not establish neural advantage, 16× resident appearance reduction, unseen-family generalization, 8–512 px behavior, temporal/disocclusion quality, fixed-canopy density scaling or sustained G6 budgets. G1–G7 remain unmet.

## Evidence

Solo Pop!_OS AMD Radeon Graphics RADV RENOIR PCI 1002:164c, Ryzen 5 5500U, Mesa 26.1.6, Chrome 154.0.8037.57. Wrela 0.9.0 compiler de9b81e52a7747870dfbd5223cb8aeb41e584d23. No subagents, training, new truth captures, RTX or purchases.

Evidence directory: /home/ryanwible/worktrees/wrela-neural-frontier/build/neural-evidence/g4-sample-lanes-v1.
Manifest SHA-256: `26d712d7066dccb7081b66a8654fdb4511717a0589d8ed90bc1261d8f4a226c6`.

The bundle includes all ten variants, four refresh builds, paired samples, native and browser outputs, allocation/completion/DRM/PSS observations, retired and retained sources, failures, checks and replay scripts. Parent response-statistic evidence is anchored by 66085f3d8ca1fcb849a789cd530da1296327245a087bfbd580e91746d918c733; reused population references by 38eb09675763f99ed58c6c3136f127a3f0f6000bbb259cefeea51d3374a9a3e3. Our own scheduling implementation uses the existing material and metric [citations](citations.md#neural-reconstruction-instrument); no outside shader code was copied or algorithmic novelty claimed.
