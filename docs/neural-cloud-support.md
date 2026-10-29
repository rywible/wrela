# Source-support bounds reduce complete cloud rendering cost

2026-10-29 UTC. Parent `e42f2d1633eef8cffaa7411e6de4b8da6d19c58a`. Solo Pop!_OS AMD measurements.

**Decision: continue and retain 32³ source support.** Final code reduces complete
GPU time by **5.70%** and cloud time by
**24.28%** across three alternating run pairs.
It adds **256 KiB** of requested storage and preserves the inspected shaded output
within one channel byte. This is a conventional win for one measured workload,
not a G4 memory reduction or G1–G7 qualification.

First joint assessment: **6.45 minutes**.
Architecture choice after two optimization followups: **14.32 minutes**.
Final validation: **36.35 minutes**.
Frozen goals/models unchanged; no training or new transport-reference infrastructure.

## Representation and fair source information

[The preceding workload diagnostic](neural-cloud-bounds.md) found 46.413% of view
queries reached shape thresholding before returning zero. Weather bounds and
simple compaction had not demonstrated a useful complete-cost improvement.

The new 32³ RGBA16 table stores maximum shape red, minimum green and maximum green
over each 4×4×4 source-texel block, including a one-texel filtering halo. Extrema of
the original half texels remain exactly representable. It uses 262,144 bytes;
the original 128³ shape texture, detail texture and every fallback remain resident.

At each view-density query, exact current weather and height combine with the
bounds to upper-bound coverage. Only certified zero-density queries are removed.
Remaining queries use the original evaluator, sample locations, sunlight
quadrature and compositing. Wind uses source-coordinate lookup. Weather/light
changes do not invalidate this shape-only table; cooking noise does.

Both methods receive the same source textures, complete scene, visibility,
weather, light, camera and sampling sequence. No reference image or privileged
learned input participates. The benefit comes from the field's support and
thresholds, while preserving its radiance evaluation.

## Initial comparison and two followups

1920×1080 clearing; one worker; 60 measured frames after 600 warmup.
Equal fixed resolution and recurring probe work (measured:false, pace:false)
prevent adaptive scheduling from removing work in one comparator. Wood,
visibility, temporal processing and compositing remain included.

| Method | Complete GPU median ms | Cloud median / p95 ms | Requested live B | Max channel error, byte |
|---|---:|---:|---:|---:|
| Unchanged source | 27.63648 | 6.17160 / 6.88972 | 405,005,112 | 0 |
| 32³ support (256 KiB) | 26.13076 | 4.70964 / 5.11352 | 405,267,256 | 1 |
| 64³ support (2 MiB) | 26.45800 | 5.08248 / 5.70528 | 407,102,264 | 0 |
| 32³ plus early coverage check | 26.12860 | 4.63876 / 5.27520 | 405,267,256 | 1 |

These initial values are unpaired. The 64³ table is slower and uses eight times
the cache. The extra coverage check has essentially identical complete cost,
slightly greater total cloud time and worse cloud p95. Neither followup is
retained. Failed source, executables and measurements remain in the bundle.

## Final paired timing

After the minimum-thickness correctness repair below, final code was remeasured
in three alternating baseline/candidate pairs. Complete GPU medians across runs:
**27.74020 → 26.15880 ms**.
Cloud medians:
**6.25456 → 4.73592 ms**.

The paired complete-cost saving is **5.70%**;
enumerated paired-log bootstrap 95% interval:
**5.37–5.93%**.
Cloud interval:
**23.62–24.77%**.
These intervals resample only three run pairs, not a scene population.
The earlier prototype's five-pair result is preserved separately, not pooled.
CPU frame-call comparison: **inconclusive**; host-call time is not GPU
completion or presentation latency.

The final paced replay of each executable provides these tails; they are not
pooled p99 estimates across every run:

| Method | Complete GPU p50 / p95 / p99 ms | Cloud p50 / p95 / p99 ms |
|---|---:|---:|
| baseline | 27.84158 / 29.61088 / 30.41396 | 6.36486 / 7.18129 / 7.46191 |
| candidate | 26.28088 / 27.82283 / 28.59330 | 4.81578 / 5.22001 / 5.60141 |

## Quality and changed-bound checks

Counter-only preflight evaluates original density at each original view query.
Across **25,166,123 queries**, the table certifies
**11,867,986** empty: **47.159%** of all
queries, **74.774%** after cheap weather rejection.
There are **zero observed false rejections**. Counts agree with the preceding
original workload diagnostic. Counter images/timings are not rendering evidence.

Initial full images differ in zero to two channel bytes, each by one; independent
replays also show single-byte variation. Final paired images differ in
**2 channel bytes**, maximum
**1**. Contrast-band diagnostics are recorded, but
an image-derived band and opaque output alpha are not semantic cloud silhouette
truth. This is sampler parity in an inspected scene, not independently converged
transport or broad motion/G3 qualification.

The retained [GPU test](../engine/tests/sky-support/main.wrela) checks 131,072
queries over three changing synthetic shape fields and one dedicated boundary
state. It includes spikes, filter boundaries, negative/wrapped coordinates,
varying weather and the exact original evaluator. It passes; a deliberately
reject-all bound fails.

A counterexample exposed a cutoff error when a custom column base exceeds the
capped top: the original evaluator still clamps thickness to one metre.
At weather (1,1,1), shape (1,1), height 4016.5 m above cloud base, the first
bound rejected positive density. The saved failing test contains 32,768 such
rejections. The repaired upper limit is max(top, bottom + 1); the same test then
has zero false rejections. This is outside cooked weather's ≤65 m base offset,
but is covered without changing density behavior. Failure and repair are preserved.
This was a correctness repair, not a third optimization. Finite tests are not a
universal floating-point proof.

## Residency and update cost

All requested allocations include source textures, scratch, caches, fallbacks
and the complete renderer. DRM values are deduplicated whole-client snapshots;
browser PSS includes CPU compiler/runtime memory. They are not isolated per-cache
residency or physical peaks. Do not sum overlapping UMA pools.

| Run | Requested live / peak B | Resident VRAM / GTT B | Browser PSS B |
|---|---:|---:|---:|
| Unchanged source | 405,005,112 / 582,183,284 | 551,976,960 / 20,099,072 | 807,483,392 |
| 32³ support (256 KiB) | 405,267,256 / 585,383,112 | 552,001,536 / 20,180,992 | 845,575,168 |
| 64³ support (2 MiB) | 407,102,264 / 584,280,436 | 552,001,536 / 20,180,992 | 794,575,872 |
| 32³ plus early coverage check | 405,267,256 / 582,445,428 | 551,997,440 / 20,082,688 | 807,042,048 |
| baseline-update | 405,005,112 / 582,183,284 | 551,976,960 / 20,180,992 | 804,996,096 |
| support-32-update | 405,267,256 / 582,445,428 | 551,997,440 / 20,262,912 | 852,608,000 |

Initial cold table construction costs **2.58444 ms**.
Both full-render update runs invoke the supported noise-slice recook at frame620.
The candidate invalidates and rebuilds its bound in that frame:

| Method | Complete update frame ms | Noise generation ms | Support rebuild ms | Cloud ms |
|---|---:|---:|---:|---:|
| baseline-update | 34.08588 | 6.86492 | 0.00000 | 5.70904 |
| support-32-update | 34.96984 | 6.88524 | 1.99020 | 4.51772 |

The recook regenerates identical authored noise: it exercises the real invalidation
and cost path. Changed contents are covered separately by the synthetic GPU test.
It does not establish a complete changing-weather/light/motion quality suite.
Both update runs include full image and memory measurements. Update spikes,
startup and steady cost must be considered together; no G6 budget is met.

## Reproduction and next bet

[Implementation](../engine/sky.wrela), [test package](../engine/tests/sky-support/wrela.toml).
Run `wrela test engine/tests/sky-support shape_support_covers`.
Pop!_OS evidence: `build/neural-evidence/cloud-shape-support-v1`.
Source archives and exact compiled runs are authoritative; commands, scripts,
raw counters, timings, residency, failures and replay instructions are included.
The initializer also uses three profile/allocation/assessment templates from the
pinned preceding cloud-weather-bounds-v1/external-scripts.tar.gz dependency.

Wrela0.9.0 compiler de9b81e52a7747870dfbd5223cb8aeb41e584d23;
Chrome154.0.8037.57; Mesa26.1.6; Ryzen5 5500U / RADV RENOIR PCI1002:164c.
One quiet AMD queue and CPU scoring afterward; no power/thermal lock or display
claim. No unchanged-engine test audit.

Next: reuse the same certificate for sunlight-density queries while preserving
light integration and fallback. Measure selectivity and complete cost before
extending to ray intervals. Do not revive the failed approximate optical-depth
cache. G1–G7 remain unchanged and unmet.

Manifest: `50aa27439f64740c2165d2bdd288555e8d59b1166b1e77bed4374711170124ab`.
