# Source-field viability with exact sample lanes

**Change the G4 architecture.** Recomputing the source field after visibility remains too expensive for steady rendering against the upgraded conventional caches. Both sharing follow-ups preserve quality but lose on complete cost. All experimental code is removed; the renderer is restored exactly to parent 44d67436854930a4ede32b499e6d8939153a395b.

Direct exact field evaluation does win the narrower full-response-replacement diagnostic. This motivates an actual source-edit/invalidation experiment; it does not establish a G4 win or prove that a practical selective cache would lose.

First joint assessment including replacement: **15.08 minutes**. Decision: **28.43 minutes**. Verification: 36.45 minutes. Exactly two follow-ups; no training, material-moment fitting or new reference captures. [G1–G7](neural-rendering-goals.md) remain unchanged.

## Equal inputs and scope

Every method uses the [retained sixteen-lane executor](neural-sample-lanes.md), ordered AA4 sum, 1 KiB workgroup radiance and 64 KiB global staging. Native cards and compact cached responses receive identical source, material, visibility and sampling information. Cached controls keep their strongest measured direct path when optional sharing loses.

The exact field evaluates the authored 44-clump crown at each visible leaf's source position. The previously validated cutoff-5 alternative evaluates nearby contributions exactly and a 7,392-byte quadratic aggregate for the far contributions.

Ten predeclared leaf-layout seeds, 26,560 cards per layout, one crown, two difficult scored views/lights and three backgrounds. Every frame renders all ten 64×64 crowns to a 320×128 mosaic. Reuse the [population experiment's](neural-source-population.md) 20 completed converged references. This is not the fixed-canopy tenfold-density workload or unseen-family qualification.

## Shaded quality and silhouettes

All **800 RGBA captures** match their corresponding representation parents: 320 initial captures match archived serial-source parents; 480 follow-up captures match the current initial methods. This does not claim every method's RGB matches native cards bit-for-bit.

Every method passes **60/60** scored comparisons against both caches. Follow-ups inherit metrics only after complete RGBA byte comparison. Silhouette/coverage stays unchanged; all compared alpha values are exact. The original AA4 silhouette errors remain, so this does not complete G3. Worst per-case differences against native:

| Method | Worst PSNR loss dB | Worst FLIP ratio | Worst LPIPS ratio | Passing rows |
| --- | ---: | ---: | ---: | ---: |
| Native cards | 0.000000 | 1.000000 | 1.000000 | 60 / 60 |
| Compact exact cache | 0.000791 | 1.000307 | 1.002521 | 60 / 60 |
| Exact source field | 0.000791 | 1.000307 | 1.002521 | 60 / 60 |
| Cutoff-5 quadratic field | 0.002971 | 1.001635 | 1.048305 | 60 / 60 |


Every browser mosaic agrees with its native GPU tiles within the existing one-byte display tolerance. No reference or metric implementation changed.

## Complete steady cost and memory

| Method | Saturated GPU median / p95 ms | Requested live / peak B | Resident VRAM / GTT B |
| --- | ---: | ---: | ---: |
| Native cards | 1.80384 / 1.83880 | 10,393,292 / 10,557,132 | 26,476,544 / 10,035,200 |
| Compact exact cache | 2.37644 / 2.40916 | 3,823,724 / 3,987,564 | 18,235,392 / 10,100,736 |
| Exact source field | 3.10472 / 3.13224 | 1,698,932 / 1,862,772 | 18,239,488 / 10,035,200 |
| Cutoff-5 quadratic field | 2.98664 / 3.01860 | 1,706,316 / 1,870,156 | 17,788,928 / 10,199,040 |
| Exact field + per-pixel sharing | 3.28492 / 3.31896 | 1,698,932 / 1,862,772 | 18,239,488 / 10,035,200 |
| Quadratic field + per-pixel sharing | 3.15544 / 3.19516 | 1,706,316 / 1,870,156 | 18,055,168 / 10,199,040 |
| Native + visible queue | 2.25852 / 2.76272 | 10,818,256 / 10,982,096 | 26,505,216 / 9,936,896 |
| Compact cache + visible queue | 2.72324 / 2.82112 | 4,248,688 / 4,412,528 | 17,805,312 / 10,035,200 |
| Exact field + visible queue | 3.13168 / 3.19260 | 2,123,896 / 2,287,736 | 17,805,312 / 10,035,200 |
| Quadratic field + visible queue | 3.11540 / 3.15968 | 2,131,280 / 2,295,120 | 17,813,504 / 9,936,896 |


Earlier serial population measurements were 6.01572 ms for exact field and 5.44260 ms for quadratic field; the new executor gives 3.10472 and 2.98664 ms. These separate-block observations are not paired percentage estimates.

The best field candidate still costs **1.656× native** and **1.257× compact-cache** median GPU time. Requested live memory is 1,706,316 B versus compact caching's 3,823,724 B, but observed whole-client residency is close to compact caching. Requested-buffer savings do not establish 16× physical appearance compression.

### Follow-ups and rejection

1. **Shared same-leaf evaluation inside one pixel.** Equal leaf IDs reuse exact radiance only for unresolved tissue. Wood and resolved samples remain independent. Shared owner/value records use 2 KiB plus 256 B keys. All 160 images are exact, but exact field slows 3.10472→3.28492 ms and quadratic 2.98664→3.15544 ms. Retire this path.
2. **Compact unique-visible-leaf queue.** Clear marks, atomically append each visible leaf once, shade the dense queue, then load its exact half-rounded radiance. No history survives. A counter, marks, queue and two radiance words per card add **424,964 B**, reused across layouts. Wood and resolved detail retain sample shading. All 320 images are exact, but every method slows: exact field 3.13168 ms and quadratic 3.11540 ms. Retire the queue. Both cached controls also receive this option and reject it.

The extra passes erase the reduced field work. No third optimization was attempted. All failed performance results and source archives are preserved.

| Method | Paced GPU p95 / p99 ms | CPU start → GPU completion p95 / p99 ms | Browser PSS MB |
| --- | ---: | ---: | ---: |
| Native cards | 2.643 / 2.734 | 7.365 / 7.612 | 503.492 |
| Compact exact cache | 3.710 / 3.814 | 9.297 / 9.493 | 509.822 |
| Exact source field | 3.997 / 4.070 | 8.885 / 9.405 | 508.263 |
| Cutoff-5 quadratic field | 3.919 / 4.045 | 9.202 / 10.051 | 511.488 |
| Exact field + per-pixel sharing | 4.226 / 4.352 | 8.781 / 8.933 | 510.150 |
| Quadratic field + per-pixel sharing | 4.118 / 4.366 | 8.888 / 9.187 | 509.982 |
| Native + visible queue | 3.153 / 3.209 | 8.012 / 8.454 | 514.791 |
| Compact cache + visible queue | 3.693 / 3.811 | 8.681 / 9.344 | 497.115 |
| Exact field + visible queue | 4.121 / 4.266 | 9.966 / 10.320 | 495.152 |
| Quadratic field + visible queue | 4.478 / 4.583 | 9.836 / 10.045 | 517.383 |


Following [toolchain profiling semantics](toolchain.md), profile.json contains saturated GPU spans; separate timings.json gives paced timestamps. CPU-start-to-completion includes queued work and browser/driver notification, not input-to-photon. Do not sum pass medians.

Complete spans include procedural placement/decoding, hardware visibility/depth ordering, wood, field/material shading, every new mark/queue/cache operation, view/light changes, copies and compositing. Hardware depth requires no separate sort. Static response construction is excluded from steady timing and measured below.

Requested live/peak counts observed buffers/textures, scratch, caches, fallback data, host rings and output banks without common-resource subtraction. Workgroup storage is separately stated. DRM deduplicates device/client IDs; PSS covers the browser group. Physical observations are whole-client end-of-run snapshots, not per-asset attribution or physical peaks. Do not add overlapping UMA pools.

## Full response replacement every frame

All ten methods also replace their response data before every complete frame, with old cache data live during construction. Exact field has no response table: replacing its one-element dummy buffer submits no refresh compute. Quadratic field rebuilds its coefficients. All ten final browser frames match steady parents exactly.

| Method | Saturated GPU median / p95 ms | Paced GPU p95 / p99 ms | Requested live / peak B | Resident VRAM / GTT B |
| --- | ---: | ---: | ---: | ---: |
| Native cards | 3.73940 / 3.82720 | 4.374 / 4.555 | 10,393,292 / 18,892,532 | 26,660,864 / 10,231,808 |
| Compact exact cache | 4.10316 / 4.15164 | 4.707 / 4.825 | 3,823,724 / 5,948,580 | 35,123,200 / 10,395,648 |
| Exact source field | 3.11124 / 3.13584 | 3.776 / 3.996 | 1,698,932 / 1,862,772 | 18,071,552 / 10,199,040 |
| Cutoff-5 quadratic field | 3.10740 / 3.15060 | 4.322 / 4.466 | 1,706,316 / 1,870,156 | 18,354,176 / 10,297,344 |
| Exact field + per-pixel sharing | 3.28368 / 3.31788 | 3.930 / 4.030 | 1,698,932 / 1,862,772 | 17,874,944 / 10,231,808 |
| Quadratic field + per-pixel sharing | 3.27916 / 3.31852 | 3.992 / 4.218 | 1,706,316 / 1,870,156 | 18,087,936 / 10,035,200 |
| Native + visible queue | 4.21988 / 4.37772 | 4.943 / 5.101 | 10,818,256 / 19,317,496 | 26,701,824 / 10,133,504 |
| Compact cache + visible queue | 4.46916 / 4.53408 | 5.033 / 5.165 | 4,248,688 / 6,373,544 | 34,951,168 / 10,297,344 |
| Exact field + visible queue | 3.13812 / 3.21628 | 3.835 / 4.082 | 2,123,896 / 2,287,736 | 17,879,040 / 10,133,504 |
| Quadratic field + visible queue | 3.23644 / 3.28312 | 3.862 / 3.954 | 2,131,280 / 2,295,120 | 17,981,440 / 10,133,504 |


Five alternating pairs verify the exact field against both controls under this replacement policy:

- Exact field versus native cards: **17.020% lower complete GPU time**, geometric paired-log 95% interval **16.943–17.151%**; CPU result inconclusive.
- Exact field versus compact exact cache: **24.326% lower complete GPU time**, geometric paired-log 95% interval **24.274–24.523%**; CPU result inconclusive.

The intervals enumerate all 3,125 resamples of five paired log ratios, conditional on these runs/workload. Neither supports CPU improvement. Startup, completion tails, counters and physical observations remain in refresh-assessment.json.

**This rebuilds unchanged source.** It does not measure actual changed clump/wood geometry, validate edited-source correspondence, or compare selective invalidation. Forced full replacement cannot establish that fields outperform the strongest practical cache under ordinary edits. Keep this result separate from steady rendering.

## Decision and next architecture bet

Change the G4 candidate toward **persistent response caching with source-aware invalidation**. Use dependencies, versions and conservative change bounds to identify work; retain direct field evaluation for missing/invalid responses. Give conventional controls the same information.

Test actual source edits against full replacement, selective conventional updates and direct field evaluation, charging metadata, fallback, all update spikes and actual residency. The replacement result demonstrates why that missing experiment matters. Reuse the existing reference generator for edited snapshots if needed; no broad reference expansion or new material-moment training is justified.

G4's 16× resident appearance reduction and ≤1.1× p95/p99 cost remain unproved. No unseen-family, 8–512 px, temporal/reveal, fixed-canopy density or sustained G6 qualification. No neural advantage or world-leading claim.

## Verification and evidence

All ten native/browser variants execute; all 800 parent-image comparisons, ten replacement-frame comparisons and 60 scored rows per method pass. Queue format/type checks pass with only inherited Camera padding warning. After rejection, every runtime, engine and reconstruction file matches parent HEAD exactly; reuse the unchanged parent default/AA2 checks.

Preserved setup failures: an overly strict assertion expected a refresh-compute label for the uncached field. Its timing run had completed; recovery finished observation/archive without repeating it. The queue allocation initially used an unimported short constructor name; qualification repaired it before GPU execution. Neither changed the hypothesis or follow-up limit.

Solo Pop!_OS AMD RADV RENOIR PCI 1002:164c, Ryzen 5 5500U, Mesa 26.1.6, Chrome 154.0.8037.57; Wrela 0.9.0 compiler de9b81e52a7747870dfbd5223cb8aeb41e584d23. Profiles: 240 frames after 60 warmup, one worker; allocation/completion: 300 paced frames. Sequential GPU runs without CPU scoring overlap. No subagents, RTX or purchases.

Evidence: /home/ryanwible/worktrees/wrela-neural-frontier/build/neural-evidence/g4-field-lanes-v1.
Manifest SHA-256: `cd936529f0af25926f3c1465f2e46395282c909105f466afec8395f9ecefc79f`.

All variants, replacements, pairs, captures, residency, failures, restored sources, checks and replay scripts are preserved. Parent manifest: 26d712d7066dccb7081b66a8654fdb4511717a0589d8ed90bc1261d8f4a226c6. Population/reference manifest: 38eb09675763f99ed58c6c3136f127a3f0f6000bbb259cefeea51d3374a9a3e3. Existing [material and metric sources](citations.md#neural-reconstruction-instrument) apply. Scheduling/queue code is our own; no novelty claim or outside implementation imported.
