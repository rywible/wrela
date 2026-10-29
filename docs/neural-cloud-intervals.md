# Cloud interval certificates preserve inspected shading but lose on complete cost

2026-10-29 UTC. Parent `ccd785d1fc03c403c9afeeef4f3d5dd96d920901`.

**Decision: retire all three interval candidates.** The best observed complete
median is 26.48944 ms, versus 26.10764 ms for the retained point-support renderer.
All add memory. Final image differences are at most one channel byte, within
the observed baseline replay variation; that does not turn a cost regression
into a rendering win.

First joint assessment: **12.56 minutes**.
Retirement: **21.99 minutes**, after exactly two optimization
followups. Runtime is restored exactly to parent. All source variants, profiles,
images, residency captures and unsuccessful results remain. Frozen models and
[G1–G7](neural-rendering-goals.md) are unchanged.

## Consequential opportunity measurement

The [retained point certificate](neural-cloud-support.md) removes 47.159% of
original view-density queries. This experiment asks whether one certificate can
remove several consecutive scheduled samples and amortize its own cost.
It preserves original sample locations, integration weights, source textures,
lighting, opacity termination and all fallback work.

Two counter builds inspect the same eight existing phases, 600/604/.../628.
Both reproduce **25,166,123** original scheduled queries. Complete four-sample
groups already certified empty by the point path cover 76.753% of those queries.
However, staying within compatible source cells sharply reduces that opportunity:

| Shape-support cell grid | Samples in compatible four-sample groups | Samples in compatible all-empty groups |
|---|---:|---:|
| 32³ | 3.097% | 2.612% |
| 16³ | 30.635% | 25.069% |
| 8³ | 52.837% | 43.102% |

Compatibility also requires one coarse weather cell. These counts are optimistic:
the interval's wider extrema can fail to prove a group empty even when every
point certificate succeeds. They justify testing a hierarchy, not a speed claim.

## Representation and two targeted followups

The first candidate aggregates the existing 32³ shape-support texture into 16³
extrema. A separate 64² weather texture stores maximum cover/top and minimum/maximum
bottom over 16×16 source-texel blocks plus the filtering halo. Each RGBA16F table
uses 32,768 bytes, so the initial hierarchy adds 65,536 requested bytes.

For each aligned group of four original samples, the ray computes the final
sample's source coordinate. A certificate requires compatible shape and weather
cells and bounds all intervening coordinates. It derives a conservative cloud
profile upper bound from weather and shape extrema. If zero cannot be certified,
the original point-support path evaluates every original sample. No interpolated
optical depth, altered sunlight intervals or radiance approximation is introduced.

Follow-up 1 coarsens the shape level to 8³, guided by the opportunity counts.
Its shape table uses 4,096 bytes, so both new tables total 36,864 bytes.
The looser bound gains spatial coherence but can certify fewer groups within it.

Follow-up 2 keeps 8³ and tests weather-only zero conditions before requiring
shape-cell agreement. A weather-only proof does not need that extra constraint.
The same revision widens endpoint height padding from 0.001 m to 8 m: the source's
planet-scale height subtraction warrants the margin already used by the earlier
[weather-interval study](neural-cloud-bounds.md). That is a numerical correction,
not another optimization attempt. No false rejection was observed in the initial
audit, but its tiny padding was not sufficient evidence for general safety.

Every weather-band write invalidates the weather table; the next march rebuilds
it before use. Existing noise invalidation rebuilds both shape-support levels.
Original weather/noise/fallback textures remain resident. All methods receive
the same source and scene knowledge; the baseline already includes the earlier
point-support improvement.

## Actual certificate effectiveness

The initial and final diagnostic builds evaluate the original detailed source
field at **every** scheduled point, including points the interval proposes to
skip. They retain original opacity termination and report any positive density
inside a rejected group. Counters replace cloud colour, so their timing and
images are not renderer results.

| Candidate | Original queries | Certificate attempts | Certified skipped samples | Share of queries | Positive-density false skips |
|---|---:|---:|---:|---:|---:|
| 16³ | 25,166,123 | 5,987,817 | 3,797,268 | 15.089% | 0 |
| 8³, weather first | 25,166,123 | 5,987,817 | 5,643,580 | 22.425% | 0 |

The final candidate removes only about 0.94 scheduled samples per certificate
attempt on average. Setup, cell checks and table reads still execute on failed
attempts, while many original samples were already cheap point-path exits.
This explains why high empty-run counts alone were insufficient. It is not an
exact attribution of GPU cost or a formal proof over all possible source inputs.

## Complete renderer cost and residency

Pop!_OS, AMD RADV RENOIR PCI 1002:164c / Ryzen 5 5500U, Mesa 26.1.6,
Chrome 154.0.8037.57, Wrela 0.9.0/compiler
`de9b81e52a7747870dfbd5223cb8aeb41e584d23`.
1920×1080 clearing, one worker, 60 measured frames after 600 warm-up.

The full scene keeps fixed resolution and recurring probe work. Wood, vegetation,
visibility, atmospheric work, temporal reconstruction, updates and compositing
remain included. Each compiled build is reused for a 660-frame allocation replay.
One quiet AMD queue precedes CPU assessment; no overlapping fitting or scoring.

| Method | Saturated complete median / p95 ms | Cloud median / p95 ms | Paced complete median / p95 / p99 ms |
|---|---:|---:|---:|
| Retained point support | 26.10764 / 27.72920 | 4.71184 / 5.29908 | 26.14840 / 27.83971 / 28.60865 |
| 16³ interval support | 26.50504 / 28.33456 | 5.17724 / 5.76912 | 26.53658 / 28.42587 / 29.25039 |
| 8³ interval support | 26.48944 / 28.27852 | 5.11784 / 5.59212 | 26.47350 / 28.36021 / 29.24918 |
| 8³, weather first | 26.49236 / 28.15040 | 5.10400 / 5.54188 | 26.44224 / 28.11386 / 28.95360 |

Saturated summaries come from `profile.json`; paced timestamps come from the
separate replay's `profile-build/results/timings.json`. Their distributions
are kept separate. These are unpaired development points with no power/thermal
lock; no confidence-qualified small speedup, sustained G6 or presentation claim.

| Method | Requested live / peak B | Actual resident VRAM / GTT B | Browser PSS B |
|---|---:|---:|---:|
| Retained point support | 405,267,256 / 585,383,112 | 551,972,864 / 20,180,992 | 824,098,816 |
| 16³ interval support | 405,332,792 / 582,510,964 | 551,993,344 / 20,180,992 | 802,854,912 |
| 8³ interval support | 405,304,120 / 585,419,976 | 551,956,480 / 20,180,992 | 843,213,824 |
| 8³, weather first | 405,304,120 / 582,482,292 | 551,927,808 / 20,262,912 | 803,040,256 |

All requested resources include scratch, caches, source/fallback data and common
renderer allocations. The lower peak in some runs is not a memory win: live
requested storage grows by the new table sizes, while allocation overlap varies.
DRM snapshots are deduplicated by device/client; PSS covers the browser process
group. These are whole-scene observations, not per-cache physical peaks. Do not
sum overlapping UMA pools or attribute small driver variations to individual tables.

New-table construction is observed at frame 18, outside steady timing:

| Method | Shape hierarchy construction ms | Weather table construction ms |
|---|---:|---:|
| Retained point support | 0.00000 | 0.00000 |
| 16³ interval support | 0.01504 | 0.45504 |
| 8³ interval support | 0.01788 | 0.54444 |
| 8³, weather first | 0.02072 | 0.58860 |

Changed-weather refresh-frame tails were not separately timed. Each invalidation
requires reconstruction before use, so this unresolved update cost cannot establish
a win for a candidate already losing steady cost. No production update or G4
qualification is claimed; further update infrastructure is not justified for
these retired variants.

## Shaded image comparison and proportional verification

| Method | Maximum channel error, byte | Mean RGB byte error | Pixels above two bytes | Independent replay max byte |
|---|---:|---:|---:|---:|
| Retained point support | 0 | 0.0000000000 | 0 | 1 |
| 16³ interval support | 1 | 0.0000003215 | 0 | 1 |
| 8³ interval support | 1 | 0.0000001608 | 0 | 0 |
| 8³, weather first | 1 | 0.0000001608 | 0 | 0 |

These compare against the retained sampler, not independently converged transport.
All compared final alpha values are exact. Existing image-derived contrast-band
diagnostics are recorded; neither final opaque alpha nor that band establishes
semantic cloud silhouette correctness. Motion and broad weather/light changes
remain unqualified. No new reference algorithm was added.

The changed source passes formatting/type checks and all complete GPU runs.
The two actual-query audits found zero false skips. Existing point-support tests
are reused as pinned evidence; no broad unchanged-engine audit or new general
interval test suite was built for a failed architecture. Candidate source and
all restoration records are preserved.

## Evidence and next architecture bet

Pop!_OS evidence: `build/neural-evidence/cloud-interval-v1`.
It contains the parent, three candidate source archives and compiled executables,
four complete profiles/residency captures, two opportunity counters, two original-
query audits, raw images/timestamps, scripts and pinned dependencies.
Manifest: `c91a9fe67074f87e249a4a40d547bd3354f8051424c3c4a9a9f4052475b740f7`.

**Next test frame-invariant specialization in temporal reconstruction.** The
current full-scene temporal stage costs about 5.05 ms. The source still reconstructs
world positions and projects them back through matrices for static camera samples.
Test the stationary-camera identity for pixel and depth while retaining explicit
motion, deformation and moving-camera paths. Use the existing shaded/motion checks,
charge the full frame, and require a rendered quality/runtime/memory result before
promotion. This is a new hypothesis, not a claimed speedup or reason to resume
neural fitting.
