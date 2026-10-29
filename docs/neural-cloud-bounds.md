# Cloud weather bounds preserve shading but do not improve complete cost

2026-10-29 UTC. Parent60e2fbecf3020fc5429d690efbbb420e03adc42d.

**Decision: retire all three weather-bound accelerators.** The single-level,
hierarchical and row-shared versions preserve the inspected image exactly or
within one byte, but their measured cloud medians are 6.24152, 6.39500 and
6.52288 ms versus 6.22624 ms for the unchanged source. All add memory. No useful
quality/runtime/residency advantage is demonstrated.

First joint assessment: **11.85 minutes**. Retirement decision: **35.79 minutes**,
after exactly **two** targeted optimizations. Additional bounded workload diagnostics
explain the next experiment; they are not more optimized candidates or rendering wins.
No training/new transport references. Runtime is restored exactly to the parent,
with all failed source, builds, outputs and measurements preserved. G1–G7 are unchanged.

## Representation and fair source information

The [previous optical-depth cache](neural-cloud-cache.md) failed shaded quality.
This block keeps the original density and lighting evaluator and the original
sample locations. A source-derived bound can only remove a ray interval whose
samples satisfy the field's early-zero conditions.

The first version stores a 64×64 table of maxima over the weather texture's
16×16-texel tiles, including a one-texel filtering halo. It takes 32,768 requested
bytes. Each originally marked ray cell queries the overlapping bound tiles;
up to nine lookups test whether the cell is clear or above all local cloud tops.

Followup1 builds four nested maximum tables at256²,128²,64²,32², totaling696,320 B.
The finest tile covers four source texels; coarser tiles combine their four children.
Each interval selects a level covering its box with at most four table reads.
This tests tighter bounds and less lookup overhead.

Followup2 shares a24-bit empty-cell mask across a horizontal tile row:16 rays at
full dome size,8 at half size. Every row has one elevation. The endpoint box,
expanded by an angular curvature allowance and floating-point margin, encloses
all rays. A per-frame preparation pass writes the masks before the original march.
It adds40,960 B at full size; all preparation stays within the measured cloud stage.

Both baseline and candidates receive the same weather, shape/detail fields, lights,
camera, ray sequence, wind and complete scene. Bounds never sample rendered truth.
Fallback remains the original evaluator. Original textures and all other scene
resources stay resident. Weather edits invalidate the bound tables; wind is handled
through source-coordinate lookup. No cheaper shadow or omitted rendering pass is
hidden in the candidates.

## Bounds and rendered verification

The weather maximum is a direct bound on convex bilinear interpolation of its
source texels. The implementation uses conservative coordinate padding and an
eight-metre margin for the planetary-height calculation. Large/out-of-domain
queries fall back to an unconstrained bound. This is an analytic construction with
sampled numerical verification, not a formal floating-point proof for every input.

The single-level and hierarchical GPU tests each inspect98,304 intervals and
1,671,168 points across three synthetic weather states. They include narrow spikes,
clear patches, filtering boundaries, negative coordinates and wrapping. Maximum
observed excess over the stored bound is zero; no certified interval contains a
sample violating the source's early-zero conditions.

The row test examines98,304 row/cell candidates across both dome scales and three
weather states. It verifies every ray at nine points in each of16,152 certified
intervals. No false-empty sample is found. Both the interval and row tests have
negative controls: deliberately invalid bounds/masks make the tests fail.
Positive and negative builds/logs are preserved separately.

The complete1920×1080 final frame is exact for hierarchy and row sharing.
The single-level version differs in one RGB channel byte by one unit, also seen
in its independent replay. Alpha is exact for all. Contrast-band checks find no
difference beyond that byte. Screen alpha is not independent cloud silhouette
truth, and a final still does not establish motion/event quality.

These checks compare against the original renderer, not independently converged
transport. There is no G1/G3/G5 quality qualification. The static comparison is
already sufficient to reject the measured operating points; no expanded reference
infrastructure or repeated audit of unchanged engine machinery is justified.

## Complete runtime and actual memory

Solo Pop!_OS, Ryzen5 5500U, AMD RADV RENOIR PCI1002:164c, Mesa26.1.6,
Chrome154.0.8037.57, Wrela0.9.0/compiler
de9b81e52a7747870dfbd5223cb8aeb41e584d23. One worker,1920×1080,
60 measured frames after600warmup. Equal fixed resolution and scheduled probe work
disable measured adaptive scheduling/pacing identically. All other rendering is enabled.

| Method | Complete GPU median / p95 ms | Cloud median / p95 ms | Paced complete p95 / p99 ms |
|---|---:|---:|---:|
| Unchanged source | 27.67216 / 29.44744 | 6.22624 / 7.07352 | 29.47345 / 30.31655 |
| Single-level intervals | 27.76088 / 29.46692 | 6.24152 / 7.03040 | 29.50551 / 30.37371 |
| Weather hierarchy | 27.92436 / 29.55908 | 6.39500 / 7.11812 | 29.71243 / 30.72155 |
| Shared row masks | 28.00516 / 29.62692 | 6.52288 / 7.25752 | 29.67519 / 30.70949 |

The complete span includes vegetation/wood, visibility, sorting/culling where used,
terrain, probes, clouds, temporal reconstruction and composition. Row-mask generation
is included in clouds. Bound generation is separately labelled during startup.

| Method | Host call median / p95 ms | Invocation interval median / p95 ms | Cold bound generation ms |
|---|---:|---:|---:|
| Unchanged source | 1.820 / 2.175 | 29.285 / 57.595 | 0.00000 |
| Single-level intervals | 1.655 / 1.970 | 29.545 / 56.790 | 0.49644 |
| Weather hierarchy | 1.665 / 1.970 | 29.485 / 57.295 | 0.48336 |
| Shared row masks | 1.670 / 1.860 | 30.090 / 58.855 | 0.48896 |

Cold table generation must be paid again after weather invalidation. No actual
changing-weather refresh-frame or moving-light quality benchmark is claimed.
Host calls and invocation intervals are not CPU-to-GPU completion or presentation.
These unpaired results have no power/thermal lock or confidence-qualified small
difference; the first version is close to baseline, without a demonstrated benefit.

| Method | Requested live / peak B | Resident VRAM / GTT B | Browser PSS B |
|---|---:|---:|---:|
| Unchanged source | 405,005,112 / 585,120,968 | 551,976,960 / 20,180,992 | 805,894,144 |
| Single-level intervals | 405,037,880 / 582,216,052 | 551,927,808 / 20,099,072 | 805,415,936 |
| Weather hierarchy | 405,701,432 / 585,817,288 | 551,968,768 / 20,099,072 | 849,928,192 |
| Shared row masks | 405,742,392 / 582,920,564 | 551,989,248 / 20,099,072 | 834,298,880 |

Requested counts include caches, scratch, inactive resources and fallback data.
Actual DRM residency is deduplicated by client/device; PSS covers the browser tree
after660pacedframes. These are whole-scene snapshots, not physical peaks or
per-appearance attribution. Do not sum UMA pools. Driver allocation reuse means
the small new tables do not produce a reliable separate residency delta; the
requested working set grows, and no memory reduction is established.

## Workload evidence that changes the next decision

After retirement, two temporary diagnostic variants inspect eight cloud phases
(600,604,...,628) of the unchanged source evaluator. Counters replace cloud colour,
so their images and profile-harness timings are invalid for rendering comparison.
The compiler can remove unused lighting work. The diagnostics measure source
query counts, not GPU time, utilization or a promised speedup.

Of all phase slots,17.640% reach the fine march after the existing rest/horizon/weather
gates. The two independent diagnostics agree exactly on total view-query and
nonzero-density counts in every phase. Source classifications sum to every query:

| Source outcome | Fraction of fine view queries |
|---|---:|
| Weather cover/height rejects | 36.932% |
| Shape-dependent billow height rejects | 7.635% |
| Shape-noise threshold yields zero | 46.413% |
| Detail erosion yields zero | 6.573% |
| Nonzero density | 2.447% |

A simple estimate sums64×the largest per-ray query count in each64-entry group.
Stable compaction using the existing weather-mask eligibility increases these work
units by2.744% for view queries and6.954% for occupied samples. Bucketing by predicted
marked-cell sample count changes them by-0.160% and+9.880%, respectively. These
predictors are available before the fine march; no future image or completed-ray
oracle is used to choose a bucket.

This estimate does not model every nested branch, wave scheduling or texture
locality. It nevertheless supplies no evidence that a simple active-ray queue will
help this workload. The current spatial ordering already keeps nearby rays together.
Do not build a compaction system solely because many rays are inactive.

The next bet is a preflight of **local shape-noise support bounds**, because46.413%
of fine queries become zero at that threshold. Test actual selectivity and cost
before another renderer integration. Query fractions alone are not causal GPU-cost
fractions; tighter bounds may still be too expensive. Keep authored shading exact
and require a complete quality/runtime/memory result before promotion.

## Evidence and replay

Pop!_OS:
`/home/ryanwible/worktrees/wrela-neural-frontier/build/neural-evidence/cloud-weather-bounds-v1`.
Manifest SHA-256: `7b3b13e06de6e2d00c03272d76bf140dcb4f1662ea5d62b37cf84a2529af0281`.

plan.json records scope and two followups. baseline/, weather-cells/,
weather-hierarchy/ and weather-row-masks/ contain source archives, commands,
compiled profiles, full-resolution images and allocation replays. decision.json
joins every assessment and workload result; retirement.json preserves the decision
before diagnostics. parent-source.tar.gz and retired-row-source.tar.gz identify
retained/failed implementations.

guard-positive-build/, guard-negative-build/, row-guard-positive-build/ and
row-guard-negative-build/ distinguish the deliberate negative controls from valid
tests. guard-test/ also contains the successful hierarchical test; row-guard-test/'s
last build is the deliberate negative, while its source is the valid test.
The original source tests and full logs remain archived.

workload-diagnostic/ and workload-reasons/ preserve counter source, executable,
commands and raw aggregate logs. workload-summary.json records per-phase counts,
classification totals, exact cross-run agreement and the work-unit estimate's limits.
external-scripts.tar.gz includes scripts and Wrela diagnostic sources.

Pre-run repairs are limited to an unsupported complement token in the row shader
and missing statement line breaks in the counter helper. Nonempty failure logs are
preserved. All restored runtime files match the parent; no model or goal changed.
Each profile batches its frames; allocation capture reuses the same executable.
CPU scoring follows the quiet AMD queue. Neither batching nor test infrastructure
is presented as a rendering win.
