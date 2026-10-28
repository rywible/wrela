# Compact material statistics: retire uniform per-card packing for G4

**Decision: retire this representation for G4 and restore the retained renderer.** No packed format passes every rendered-quality comparison. The best uses 28.27% fewer total requested live bytes than the exact response cache, but is 27.97% slower than native cached cards, has no demonstrated steady physical-residency advantage over the exact response cache, and costs more to refresh. Construction improvements are not a rendering win.

Initial assessment: **27.14 minutes**. Explicit decision: **37.77 minutes**. Exactly two follow-ups, zero training, zero new reference captures. [G1–G7](neural-rendering-goals.md) remain unchanged and unmet.

## Fair experiment

The authored [leaf material](../engine/impostor.wrela) consumes depth only through attenuation exp(-depth / 2.4). The experiment temporarily exposes that statistic directly while preserving the public depth wrapper. The conventional control receives the same simplification: Oct32 normal plus f32 attenuation, eight bytes per card. It is byte-identical to the original normal/depth cache in all 80 captures.

Candidates uniformly quantize the two existing octahedral normal coordinates and attenuation into 32 or 16 bits. This is our own quantizer, not a learned model. [Source card construction](../engine/plant.wrela) supplies every method with the same crown, exact layout, wood, cutouts, visibility and material laws. Native full-card caching is the fastest practical conventional control; the eight-byte response cache is the smaller exact control.

Ten predeclared seeds share one 44-clump crown, with 26,560 cards per layout. Every frame renders all ten 64×64 crowns into a 320×128 mosaic, alternating views 133/141 and lights 1/2. Quality reuses 20 converged shaded references on three backgrounds: 60 comparisons. This is ten layouts, **not** G4's tenfold density test at fixed canopy bounds and area.

## Results and two follow-ups

The initial 10+10+12-bit format passes 56/60 comparisons against every exact control; worst LPIPS ratio is 1.1873 versus the exact response cache. The 16-bit formats pass 3/60 and 0/60. Coverage is exact; shaded appearance fails.

1. **Precision reallocation:** 12 bits per normal coordinate and 8 attenuation bits improve the result to 59/60. Seed 101, view 133, light 1, dark background still fails LPIPS: 1.07023× the exact response cache, 1.07131× native, versus the unchanged 1.05× limit. Worst PSNR loss is 0.07376 dB and worst FLIP ratio 1.00522. No further precision search.
2. **Bounded construction:** both exact and compact caches use 4,096-card chunks with reusable staging. All 160 images match their parents byte-for-byte. Candidate requested peak falls from 4,820,644 to 2,859,628 B; observed resident VRAM falls from 26,427,392 to 18,034,688 B. This repairs construction memory but preserves the quality failure. The exact driver allocation cause is unproved.

| Method | Quality passes / 60 | Saturated GPU median / p95 ms | Requested live / peak B | Resident VRAM / GTT B |
| --- | ---: | ---: | ---: | ---: |
| Native exact cards | 60 | 2.56868 / 2.60796 | 10,327,756 / 10,491,596 | 26,533,888 / 10,100,736 |
| Exact normal/depth cache | 60 | 3.29756 / 3.33376 | 3,758,188 / 3,922,028 | 18,038,784 / 10,297,344 |
| Exact normal/attenuation cache | 60 | 3.27896 / 3.32620 | 3,758,188 / 3,922,028 | 17,768,448 / 10,035,200 |
| 32 bits: 10+10 normal, 12 attenuation | 56 | 3.28480 / 3.32040 | 2,695,788 / 4,820,644 | 26,521,600 / 10,035,200 |
| 16 bits: 6+6 normal, 4 attenuation | 3 | 3.28128 / 3.32544 | 2,164,588 / 4,289,444 | 17,768,448 / 10,199,040 |
| 16 bits: 5+5 normal, 6 attenuation | 0 | 3.27852 / 3.32440 | 2,164,588 / 4,289,444 | 18,231,296 / 10,035,200 |
| 32 bits: 12+12 normal, 8 attenuation | 59 | 3.29120 / 3.32296 | 2,695,788 / 4,820,644 | 26,427,392 / 10,297,344 |
| Exact attenuation, chunked construction | 60 | 3.28172 / 3.32548 | 3,758,188 / 3,922,028 | 17,768,448 / 10,330,112 |
| 32 bits: 12+12+8, chunked construction | 59 | 3.28712 / 3.33744 | 2,695,788 / 2,859,628 | 18,034,688 / 10,231,808 |


The compact payload itself is 1,062,400 B versus 2,124,800 B. Total requested live reduction is only 1.394×. Its 18.035 MB resident VRAM is comparable to the exact attenuation control's 17.768 MB, not a 16× physical reduction.

Complete spans include placement/decoding, visibility/depth ordering, wood, scratch clearing, view/light updates, copies and compositing. Hardware depth ordering needs no additional sort. Static cooking is excluded from steady timings and measured through full replacement below. All observed buffers/textures, scratch, caches, fallback/shared data, host rings and output banks count; no common-resource subtraction.

| Method | Paced GPU p95 / p99 ms | CPU start → GPU completion p95 / p99 ms | Browser group PSS MB |
| --- | ---: | ---: | ---: |
| Native exact cards | 3.398 / 3.474 | 7.815 / 8.068 | 495.489 |
| Exact normal/depth cache | 4.137 / 4.217 | 8.886 / 9.214 | 501.994 |
| Exact normal/attenuation cache | 4.146 / 4.241 | 8.731 / 9.001 | 497.376 |
| 32 bits: 10+10 normal, 12 attenuation | 4.495 / 4.751 | 8.520 / 8.704 | 501.232 |
| 16 bits: 6+6 normal, 4 attenuation | 4.184 / 4.266 | 8.800 / 9.235 | 500.816 |
| 16 bits: 5+5 normal, 6 attenuation | 4.124 / 4.233 | 7.822 / 8.232 | 501.148 |
| 32 bits: 12+12 normal, 8 attenuation | 4.603 / 4.709 | 8.781 / 8.926 | 501.626 |
| Exact attenuation, chunked construction | 4.186 / 4.250 | 8.840 / 9.025 | 502.671 |
| 32 bits: 12+12+8, chunked construction | 4.206 / 4.270 | 8.727 / 9.111 | 503.753 |


Per [toolchain semantics](toolchain.md), profile.json contains saturated GPU spans; profile-build/results/timings.json contains a subsequent paced run. CPU-start-to-completion includes browser/driver notification, not input-to-photon. These descriptive 240-frame runs follow 60 warmup frames. No small speed gain, paired confidence interval or sustained G6 claim is made.

DRM pools deduplicate PCI/client identifiers. VRAM/GTT are whole-client resident observations; PSS covers the browser process group. Do not add UMA pools or claim per-asset physical attribution. Physical readings are end-of-run resident snapshots, not a time-resolved physical-memory peak; the API allocation observer separately records requested peaks.

## Full replacement and rendering

Every frame reconstructs all ten caches, with the old cache live during replacement. Native cards receive the same replacement semantics. Exact and compact paths have the same source information; both receive chunking. GPU spans include refresh, rendering, copies and gaps rather than summing pass medians.

| Method | Saturated GPU median / p95 ms | Paced GPU p95 / p99 ms | Requested live / peak B | Resident VRAM / GTT B |
| --- | ---: | ---: | ---: | ---: |
| Native exact cards | 4.50912 / 4.59828 | 4.921 / 5.031 | 10,327,756 / 18,826,996 | 26,443,776 / 10,395,648 |
| Exact normal/attenuation cache | 5.01492 / 5.05820 | 5.537 / 5.654 | 3,758,188 / 5,883,044 | 34,652,160 / 10,395,648 |
| Exact attenuation, chunked construction | 6.78132 / 7.19024 | 7.814 / 8.066 | 3,758,188 / 5,948,580 | 34,902,016 / 12,066,816 |
| 32 bits: 12+12+8, chunked construction | 7.11080 / 7.49876 | 8.355 / 9.499 | 2,695,788 / 3,856,532 | 26,259,456 / 11,902,976 |


The compact candidate is 41.79% slower than direct exact response caching and 57.70% slower than native cards under this workload. Chunking slows exact-cache refresh too. It is a memory/refresh tradeoff. All four final refreshed browser images match their steady parents exactly.

CPU-start-to-completion p95/p99: native 9.920/10.181 ms; exact response 10.441/10.840; chunked exact 13.016/13.235; chunked compact 12.160/12.781. Single browser-open-to-first-observed-render-completion samples range from 333.58 to 375.63 ms, including loading, compilation and queued initialization; they are retained, not called isolated cooking latency.

This measures full reconstruction after invalidation, not actual edited-source or animated-geometry correctness, incremental invalidation or an in-place update optimization.

## Verification and retained state

All 560 candidate/attenuation-control alpha captures match the compact exact control. Across the 20 scored cases, mean coverage MAE is 0.006851, worst boundary p99 distance 3.124 px, minimum half-coverage precision 98.218% and recall 99.205%. These existing AA4 limitations are not G3 success.

The original controls preserve all 160 previous images after the temporary shared API change. Nine variants yield 720 RGBA captures; browser mosaics match native tiles within one 8-bit unit. Existing format/type checks pass. Frozen models and goals retain their hashes. No unchanged reference audit or unrelated tests were added.

All experimental engine/runtime code, including the material API extension, is removed. Engine and studio files match the parent commit byte-for-byte. Every candidate's source/build remains archived. Preserved failures include a collision with a historical sealed experiment name, recovered by restoring original file coverage and its external script, and an optional startup summary's incorrect assumption that frame zero rendered. The corrected summary records the first observed completed render frame; no measurements changed.

## Next architecture-level bet

**Change the unit of work to source-certified adaptive shading.** Use authored fields, derivatives, stable IDs and material sensitivity to bound where visible samples can share expensive field/lighting evaluation. Preserve exact coverage and use exact fallback for sensitive normals/glints. A neural residual earns a place only through this same measured comparison.

This is untested; earlier uniform sharing failures remain failures. The smallest next experiment is one difficult completed view/light case with a predeclared rendered-error bound and exact fallback. Measure fallback rate, all resident data and complete cost against both native cards and exact response caching. Retire it if bound/fallback overhead consumes the savings. No further material-moment training is justified here.

Unseen families, 8–512 px footprints, temporal/disocclusion behavior, source-edit correctness, G4 density scaling and sustained G6 remain open.

## Evidence

Pop!_OS AMD RADV RENOIR PCI 1002:164c, Ryzen 5 5500U, Mesa 26.1.6, Chrome 154.0.8037.57; Wrela 0.9.0 compiler de9b81e52a7747870dfbd5223cb8aeb41e584d23. Solo; no subagents, RTX, purchases or training.

Evidence: /home/ryanwible/worktrees/wrela-neural-frontier/build/neural-evidence/g4-response-statistic-v1.
Manifest SHA-256: `66085f3d8ca1fcb849a789cd530da1296327245a087bfbd580e91746d918c733`.

The bundle preserves the plan, nine variants, four full-refresh builds, raw captures/profiles, allocations/DRM/PSS, decisions, failures, restored-source verification and replay scripts. Reused references are anchored by manifest 38eb09675763f99ed58c6c3136f127a3f0f6000bbb259cefeea51d3374a9a3e3. The earlier g4-source-response-v1 is a different sealed study. Established metric sources: [citations](citations.md#neural-reconstruction-instrument).
