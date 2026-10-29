# Shape-support reuse does not pay for sunlight queries

2026-10-29 UTC. Parent `2b6f47c4ad3dce08f0c46cac225dc1886036c79c`.

**Decision: retire all three sunlight variants.** They add no requested storage,
but all cost more than the retained view-support renderer. The best attempted
subset costs **26.37524 ms complete / 4.97780 ms clouds**, versus
**26.04872 / 4.68700 ms** baseline. Maximum channel error is 2/255 in the inspected shaded images. No joint quality/memory/runtime advantage is demonstrated.

First joint assessment: **4.73 minutes**.
Decision after exactly two targeted optimizations: **14.45 minutes**.
Tracked runtime is restored byte-for-byte to the parent. The preceding
[view-support win](neural-cloud-support.md) remains intact. G1–G7 and frozen
models are unchanged; no new training or reference infrastructure.

## Question and fair baseline

The baseline already contains the 256 KiB shape-support table and its measured
view-ray acceleration. Comparing to the older, slower renderer would wrongly
credit sunlight reuse with the previous improvement.

Each candidate reuses that same table to certify empty sunlight-density queries.
The eight original light intervals, weights, positions and accumulation order
remain unchanged. The first two queries still use detailed density; the other
six use the original base-density evaluator. No optical-depth approximation,
new table, alternate light, omitted wood/visibility/compositing, or privileged
source information is introduced. The conventional baseline sees the same
complete source textures, weather, scene and sampling sequence.

The first version checks all eight queries. Followup1 checks only indices2–7,
guided by near-sample selectivity. Followup2 checks only indices4–7 to test whether
the most distant subset can earn its overhead. All remaining queries retain
the original evaluator and fallback.

## Actual query selectivity

A temporary counter build follows the original light queries at eight real
cloud phases,600/604/608/612/616/620/624/628. It still checks original density at
every proposed rejection. The 4,926,936 light queries equal eight times the
preceding source diagnostic's615,867 positive view samples exactly.

| Query group | Eligible after cheap weather tests | Certified empty | Rejection share of eligible |
|---|---:|---:|---:|
| All eight | 4,266,423 | 731,785 | 17.1522% |
| Near two | 1,231,717 | 134 | 0.0109% |
| Far six | 3,034,706 | 731,651 | 24.1095% |

There are **zero observed false rejections**. Across all light queries, including
cheap weather exits, the table removes only **14.853%**.
The near queries originate close to already occupied view points, explaining
their very low rejection rate. This differs materially from the view-query
population where support rejected47.159% of all queries.

Counters replace cloud colour; their timings/images are not rendering evidence.
The finite audit does not prove all possible inputs safe. Existing bound/negative
control evidence is reused from the pinned preceding experiment rather than
rerunning unchanged machinery.

## Complete rendered comparison

1920×1080 clearing, one worker,60frames after600warmup. Identical fixed resolution
and recurring probe work prevent adaptive scheduling from removing work in one
comparator. All source textures, fallback data, wood, visibility, temporal
processing and compositing remain. The executable is reused for paced capture
and allocation observation; builds, GPU measurements and CPU scoring are sequential.

| Method | Complete GPU median / p95 ms | Cloud median / p95 ms | Paced complete p95 / p99 ms |
|---|---:|---:|---:|
| Retained view support | 26.04872 / 27.77300 | 4.68700 / 5.19472 | 27.65298 / 28.79247 |
| All eight light queries | 26.71472 / 28.89400 | 5.38516 / 6.09120 | 28.45048 / 29.23467 |
| Far six light queries | 26.53576 / 28.04832 | 5.12176 / 5.75236 | 28.38363 / 29.23905 |
| Distant four light queries | 26.37524 / 28.09772 | 4.97780 / 5.57404 | 28.03521 / 28.90495 |

All three increase cloud medians and complete medians. These are unpaired
development points: no population confidence interval or universal dominance is
claimed. Repeating losers in a larger paired tournament would not change the
decision supported by this block.

| Method | Requested live / peak B | Resident VRAM / GTT B | Browser PSS B |
|---|---:|---:|---:|
| Retained view support | 405,267,256 / 585,383,112 | 551,907,328 / 20,099,072 | 802,246,656 |
| All eight light queries | 405,267,256 / 582,445,428 | 551,907,328 / 20,180,992 | 808,064,000 |
| Far six light queries | 405,267,256 / 582,445,428 | 551,911,424 / 20,262,912 | 798,598,144 |
| Distant four light queries | 405,267,256 / 585,383,112 | 551,907,328 / 20,099,072 | 805,586,944 |

Requested allocations include scratch, caches, original textures, fallback data
and the complete renderer. All methods have identical live requested storage.
DRM values are deduplicated whole-client snapshots; PSS includes CPU-side
compiler/runtime memory. These are not per-cache physical peaks. Do not sum
overlapping UMA pools or attribute browser-memory variation to cache bytes.

The existing bound-construction events are recorded in every run. Construction,
layout and source invalidation are unchanged. The preceding explicit noise-rebuild
penalty still applies; this experiment introduces no new update resource and does
not requalify changing-weather/light performance.

## Shaded quality and limitations

| Method | Max channel error, byte | Mean RGB byte error | Pixels above two bytes | Independent replay max byte |
|---|---:|---:|---:|---:|
| Retained view support | 0 | 0.000000000 | 0 | 1 |
| All eight light queries | 2 | 0.000016236 | 0 | 0 |
| Far six light queries | 2 | 0.000016397 | 0 | 1 |
| Distant four light queries | 2 | 0.000016397 | 0 | 1 |

These are differences from the retained sampler, not independently converged
transport truth. Maximum two-byte candidate differences exceed the baseline's
one-byte replay variation in this sample; their cause is unattributed. No exact
parity claim is made. The cost failure does not justify another quality-repair
project or new references for these retired implementations.

Image-derived contrast-band error and opaque final alpha do not independently
establish cloud silhouette correctness. Motion, edited lighting, broad scenes
and G1–G7 remain unqualified. No GPU speedup is inferred from rejection counts.

## Reproduction and next architecture bet

Evidence on Pop!_OS:
`build/neural-evidence/cloud-light-support-v1`.
All failed variants retain exact source archives, compiled executables, profiles,
images, actual-residency captures and commands. Original-query counters and
restoration checks are included. The prior source-support manifest and model
plan are pinned dependencies.

Solo Pop!_OS AMD Ryzen5 5500U / RADV RENOIR PCI1002:164c, Mesa26.1.6,
Chrome154.0.8037.57, Wrela0.9.0 compiler de9b81e52a7747870dfbd5223cb8aeb41e584d23.
One quiet GPU queue, CPU scoring afterward, no power/thermal lock or presentation
claim. No unchanged-engine test audit.

The point-support certificate stays on view rays. Next return to the
[measured reconstruction-observation ambiguity](neural-capacity.md):
preserve visible-surface identity, source-local sampling coordinates and separate
surface colour/history before a compact residual. Require an equally informed,
constructive conventional decoder to improve held-out cold/reveal images at
measured complete cost before more training. Merely adding unique IDs and
eliminating equal-input collisions would not demonstrate useful information.

Manifest: `843735e7936ad4c3283b2f7aa0bbbe23dd7a9d76732a73b18114c18d8ddfd156`.
