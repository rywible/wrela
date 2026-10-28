# Population sharing: change the G4 representation

Decision on 2026-10-28: **change representation for the G4 pursuit**. A source-compiled far-field table can serve ten different leaf layouts, but the quality-preserving version remains slower than exact caching. Retain cutoff 5 as a bounded diagnostic; retire the lower-cutoff recipes for broader-layout quality and remove the GPU-cooked owner-table optimization because of actual resident memory. G1–G7 and the frozen optical/raw models are unchanged and unmet.

The first complete assessment took 23.19 minutes; the decision was recorded at 44.48 minutes, within the two-hour block. There were exactly two optimization follow-ups, zero training steps, and no reference-renderer changes.

## Workload and fair controls

Pop!_OS, AMD RADV RENOIR PCI 1002:164c, Ryzen 5 5500U; Wrela 0.9.0 compiler de9b81e52a7747870dfbd5223cb8aeb41e584d23. Device, host, build hashes and commands are preserved per run.

The predeclared seeds are 7, 13, 29, 43, 73, 101, 151, 199, 251 and 307. Each changes the 26,560 leaf-card placements over the same authored 44-clump crown. All methods share wood, cutout programs, source clumps/owner ranges and visibility scratch. Native cards and exact response caches correctly contain all ten layouts. Full field and compiled aggregate responses share one field representation. Ordinary instances of one individual already share cached data in the engine; that is not credited as a new win.

Each frame renders all ten 64 px crowns, reusing one visibility workspace, then copies and composites all ten outputs into 320×128. Placement/decoding, depth ordering/visibility, wood, response evaluation, view/light updates, copies, scratch and compositing are charged. Static source cooking is outside steady timing; its retained allocations and observed peaks are counted. Dynamic crown edits remain unmeasured. This is **not** G4's 10× primitive-density test at fixed canopy bounds/area.

Two difficult view/light pairs, 133/1 and 141/2, were tested for all seeds against the existing shaded renderer at AA 32/AA 64 and three backgrounds. The 18 new cases required 36 captures; seed 7 reused completed references. All 20 cases passed the existing radiance/coverage convergence gates. No new reference machinery was built.

## Joint results

GPU numbers below are complete **saturated** frame spans, 240 frames after 60 warmup. Separately recorded paced timing appears in the next table, following [the toolchain's profiling contract](toolchain.md). MB means decimal megabytes. Requested allocation totals include shared buffers, textures, host rings, scratch, caches, dummy/fallback resources and the final output bank.

| Method | Quality rows | GPU median / p95 ms | Requested live / peak MB |
| --- | ---: | ---: | ---: |
| Native cached cards | 60/60 | 2.582 / 2.617 | 10.328 / 10.492 |
| Compact exact response cache | 60/60 | 3.297 / 3.334 | 3.758 / 3.922 |
| Exact full field | 60/60 | 6.016 / 6.049 | 1.633 / 1.797 |
| Compact quadratic, cutoff 4 | 59/60 | 4.677 / 4.723 | 1.641 / 1.805 |
| Regional log, cutoff 2 | 55/60 | 4.088 / 4.119 | 1.656 / 1.820 |
| Compact quadratic, cutoff 5 | 60/60 | 5.443 / 5.534 | 1.641 / 1.805 |
| Exact cache + owner table (retired) | 60/60 | 3.094 / 3.134 | 3.785 / 3.949 |
| Exact field + owner table (retired) | 60/60 | 5.816 / 5.854 | 1.660 / 1.824 |
| Cutoff 5 + owner table (retired) | 60/60 | 5.223 / 5.303 | 1.667 / 1.831 |

Cutoff 4 fails seed 151/view 133/light 1/blue background: LPIPS ratio 1.0613 versus native exact. Regional cutoff 2 fails five rows across four seeds, reaching 1.1593. Both failures are retained and supersede their earlier seed 7-only quality evidence.

Follow-up 1 increases the compact table's exact-near cutoff 4→5. It preserves the 7,392-byte coefficient table and passes 60/60 population comparisons against all exact controls. It also passes 36/36 prior converged comparisons; six overlap, giving 90 distinct shaded comparisons. The worst population LPIPS ratio is 1.04831 versus native exact, close to the 1.05 limit; these are validation cases, not a new generalization guarantee.

For cutoff 5, population mean PSNR is 39.15457 dB, mean FLIP 0.01393377, and LPIPS 0.00146310. All candidate alpha images match the compact exact control. Against AA 64 truth, coverage MAE averages 0.0068507; worst boundary p99 is 3.124 px, worst half-coverage precision 0.982183, and recall 0.992045. Preserving the exact control's silhouette does not remove its finite-AA error.

Five alternating pairs confirm cutoff 5 is 9.63% faster than full-field evaluation (geometric 95% interval 9.564–9.735%, conditional on the five pairs). CPU improvement is inconclusive. Its total requested allocation is 2.290× smaller than the compact exact cache, but its GPU time is 1.651× as large; versus native cached cards it is 2.108× as slow. This is a measured tradeoff, **not a G4 win**.

## Actual residency rejects the second optimization

| Method | Resident VRAM / GTT MB | Browser group PSS MB | Paced GPU p95 / p99 ms | CPU start → GPU completion p95 / p99 ms |
| --- | ---: | ---: | ---: | ---: |
| Native cached cards | 26.272 / 9.937 | 494.960 | 3.429 / 3.480 | 7.561 / 7.879 |
| Compact exact response cache | 18.231 / 10.199 | 499.398 | 4.187 / 4.251 | 8.710 / 8.917 |
| Exact full field | 18.039 / 10.297 | 495.593 | 7.010 / 7.099 | 11.750 / 12.003 |
| Compact quadratic, cutoff 4 | 18.047 / 10.297 | 498.064 | 5.647 / 5.725 | 10.310 / 10.836 |
| Regional log, cutoff 2 | 17.977 / 10.134 | 500.758 | 5.071 / 5.151 | 9.900 / 10.135 |
| Compact quadratic, cutoff 5 | 18.047 / 10.297 | 503.462 | 6.483 / 6.582 | 11.035 / 11.303 |
| Exact cache + owner table (retired) | 101.040 / 10.199 | 501.783 | 4.009 / 4.157 | 9.239 / 9.699 |
| Exact field + owner table (retired) | 101.233 / 10.297 | 501.050 | 6.716 / 6.795 | 11.215 / 11.469 |
| Cutoff 5 + owner table (retired) | 101.044 / 10.035 | 505.511 | 6.199 / 6.292 | 10.470 / 10.788 |

These are deduplicated per-client DRM observations and whole-browser-group PSS. They are not per-asset physical attribution; do not add UMA VRAM/GTT/PSS pools or substitute requested resource bytes for physical residency.

Follow-up 2 compiles exact leaf-owner membership into a shared 26,560-byte table, enabling it equally for exact cache, exact field and cutoff 5. All 240 images are byte-exact to their parents. The exact cache gains 6.04% in five paired GPU runs (geometric 95% interval 6.033–6.231%), but every variant adds more than 80 MB of actual resident VRAM. The increase is in the primary DRM client, not duplicate descriptors. WebGPU resource counters show only the 26.6 KB table; the remaining driver/compiled-shader allocation has not been isolated. Calling it proven shader scratch would overstate the evidence.

**Retire this GPU-cooked owner-table implementation.** Its code was removed from the retained runtime; complete sources, binaries, outputs and failed resident results remain archived. A fresh run of the rebuilt no-owner cutoff 5 candidate returns to 18.043 MB resident VRAM and 10.297 MB GTT, with 1,640,780 B requested live. This recovery makes the joint decision independent of the unproved allocation cause.

## Retained result and next bet

The executable population diagnostic is retained with single-layout defaults. Existing mode 2/AA 2, frozen payload and frame loop are restored; the opt-in mode 3 compact recipe now uses cutoff 5, degree 2, log false, regions 1. Twenty final native images and resource-count snapshots match the measured results, the browser mosaic matches independently captured tiles within one 8-bit unit, and existing format/type checks pass. The rejected owner-table code is absent.

The next architecture bet is **source-compiled compact per-leaf response residuals with cheap decoding**, using exact authored structure as the predictor. Compile static ownership/structure offline; spend approximation only on residual response that matters in shaded images. Compare it with the strong exact cache immediately, including actual driver residency. No further material-moment training is justified by this block.

Unseen families, wind/temporal behavior, 8–512 px footprints, dynamic source edits, G4 density scaling and G6 sustained budgets remain open. One crown field with ten layouts is not world-leading evidence.

## Evidence and replay

Sealed evidence: `/home/ryanwible/worktrees/wrela-neural-frontier/build/neural-evidence/g4-source-population-v1`.
Manifest SHA256: `38eb09675763f99ed58c6c3136f127a3f0f6000bbb259cefeea51d3374a9a3e3`.

The bundle includes the predeclared plan, all 14 variants, references/convergence, full images and profiles, all-allocation observations, DRM/PSS, paired samples, failures, the rejected source archive, retained-source checks and replay scripts. See `decision.json`, `initial-comparison.json`, `comparison.json`, `owner-residency-rejection.json`, `retained-checks.json` and `replay.md`. Previous source-tail evidence is anchored by its manifest. The complete goal contract remains [G1–G7](neural-rendering-goals.md); metric and source citations remain in [citations](citations.md).
