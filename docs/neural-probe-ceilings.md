# Source terrain ceilings: retire after the production comparison

2026-10-29 UTC. Parent: 61c534f4cddba9b58c23177826cb3e386e9d686b.

**Decision: retire all three terrain-ceiling accelerators.** The small fixtures
preserve every sampled lighting word and shaded image byte, with modest nominal
speed gains. The full clearing does not reproduce those gains: probe-update median
is 1.80284 ms for the unchanged baseline versus 1.86372, 1.85272 and 1.88600 ms
for the three candidates. Memory is unchanged or larger. There is no new joint
rendering win, G4 memory result, or G5/G6 qualification.

The first complete image/runtime/residency assessment took **23.16 minutes**.
The explicit decision took **51.65 minutes**, after exactly two targeted followups.
There was no training or new transport-reference infrastructure. Failed source,
executables, images, timing and memory observations remain archived. Tracked
runtime is restored byte-for-byte to the parent; only evidence documentation is retained.

## Question, source information and measured work

The preceding [nonlinear radiance experiment](neural-leaf-lighting.md) found cheap
direct source shading stronger than uniform interpolation. This block first
measured an existing expensive production operation: repeated terrain queries in
[probe tracing](../engine/probes.wrela). The original closed-room test costs
2.27312 ms for probe updates and 4.16536 ms for the complete 320×180 frame.

The experiment asks whether existing [terrain culling bounds](../engine/terrain.wrela)
can safely eliminate these queries. Both baseline and candidates receive identical
terrain, density, solid fields, current lighting, source functions and ray sequences.
Neither receives reference images or future state. Preparation, cache updates,
scratch and fallback are charged. This is an internal source-based optimization,
not a new neural model or a novel external algorithm.

- Initial candidate: each ray reduces existing terrain side bounds to a global
  ceiling, omits height queries above it, and ends upward rays above all source
  volumes. Actual ground is still evaluated where leaf scattering or solid bounce
  needs it. No new buffer is allocated.
- Followup 1: summarize that ceiling once after terrain cooking; retain four bytes
  per clipmap. This tests whether removing the per-ray reduction changes the result.
- Followup 2: derive a reach-local ceiling for each active probe column, shared over
  layers and rays. This avoids distant clearing mountains inflating a global bound.
  It adds 5,120 bytes of requested scratch; preparation remains inside probe cost.

The bounds are intended to conservatively cover the terrain interpolation. Each
changed representation passed its corresponding GPU test: 393,216 sample queries
across three terrain-edit/camera states, including negative/elevated terrain,
clipmap boundaries, morphs and out-of-range clamping. These are sampled validation,
not a formal continuum proof. The measured implementation is in each source archive.

## Rendered quality and silhouette preservation

Reuse the existing closed-room/box/open-ground probe fixture. Give the ground a
non-black material so indirect lighting differences are visible; add a smooth hilly
case using the same room geometry. Both use the full Frame renderer. Each executable
reports 54 positions with six float32 lighting words each and captures the final
shaded RGBA frame. All methods settle for 900 frames and measure the next 60.
Sizes are 320×180 and 1920×1080.

For the initial and cached candidates, all four configurations are byte-exact in
RGBA, including silhouettes/alpha, and exact in **1,296 lighting words per variant**.
The allocation-observer replays also preserve those images and lighting words.
The local candidate was taken directly to the consequential full-scene comparison;
it did not warrant another fixture sweep after failing there.

The full 1920×1080 clearing's final frame is byte-exact for local versus baseline.
The initial and cached variants differ from baseline by **one channel byte by one
unit in one pixel**, out of 8,294,400 channel bytes. Alpha is exact. Independent
replays of the baseline, cached and local executables also show this one-unit
variation, so it is within observed repeat variability; it is not evidence of a
meaningful quality difference.

This establishes parity with the existing sampler at the inspected outputs.
There is no independent converged light-transport reference, no moving sequence
qualification and no claim that the baseline lighting is physically correct.
No FLIP/LPIPS score is manufactured from near-identical byte outputs.

## Complete production cost

A default clearing profile initially appeared slightly faster: 25.83044 versus
25.65540 ms for baseline/cached. It dispatched **zero probe updates** in measured
frames 600–659; none occurred after the initial bake through frame 659.
Those numbers cannot establish a probe acceleration.

To measure the actual proposed work, set measured adaptive-cost scheduling and
adaptive pacing off identically for all methods, keeping fixed resolution and the
code-estimated work scheduler. All 60 measured frames then dispatch probe updates.
All other clearing rendering remains enabled: terrain, wood/vegetation, visibility,
sorting/culling where used, lighting, temporal work and composition. The temporary
app flags and all engine changes are restored after measurement.

Pop!_OS, Ryzen 5 5500U, AMD RADV RENOIR (PCI 1002:164c), Mesa 26.1.6,
Chrome 154.0.8037.57, Wrela 0.9.0/compiler
de9b81e52a7747870dfbd5223cb8aeb41e584d23. One worker, 1920×1080,
60 measured frames after 600 warmup. Saturated GPU replay and paced execution
are reported separately.

| Method | Complete GPU median / p95 ms | Probe median / p95 ms | Paced complete p95 / p99 ms |
|---|---:|---:|---:|
| Unchanged baseline | 27.66524 / 29.55736 | 1.80284 / 3.00688 | 29.44713 / 30.42032 |
| Per-ray global ceiling | 27.97544 / 29.57108 | 1.86372 / 3.00028 | 29.52476 / 30.11182 |
| Cached global ceiling | 27.71396 / 29.42020 | 1.85272 / 3.02020 | 29.45787 / 30.24935 |
| Per-column local ceiling | 27.69688 / 29.44572 | 1.88600 / 2.93348 | 29.62742 / 30.14961 |

The probe label includes candidate bound preparation and tracing. Complete GPU span
includes every timestamped rendering stage, not just the candidate kernel.
These are unpaired runs without a power/thermal lock. Small tail differences are
mixed; none is a confidence-qualified production advantage. No extra paired sweep
was justified after all three candidate medians failed to improve probe work.

| Method | Host call median / p95 ms | Invocation interval median / p95 ms | Paced probe p95 / p99 ms | Cumulative cold probe-bake GPU ms |
|---|---:|---:|---:|---:|
| Unchanged baseline | 1.710 / 2.020 | 29.665 / 57.905 | 3.00217 / 3.08610 | 634.70416 |
| Per-ray global ceiling | 1.730 / 1.935 | 29.720 / 57.100 | 3.01740 / 3.05254 | 653.78448 |
| Cached global ceiling | 1.760 / 2.005 | 29.420 / 57.170 | 2.97424 / 3.09175 | 649.30952 |
| Per-column local ceiling | 1.780 / 2.200 | 29.850 / 56.815 | 2.93364 / 2.98173 | 569.52708 |

Host call duration and invocation intervals are not CPU-to-GPU completion or
presentation latency. Cold values sum measured probe-bake GPU work during warmup;
they are not wall-clock readiness times. The lower local cold sum is an interesting
unpaired observation, but it does not establish a repeated startup win and does not
offset failed steady-state cost for this objective.

## Actual resident memory

The observer reuses each exact compiled production executable and takes its snapshot
after 660 paced frames. Requested live/peak cover all instrumented buffer/texture
allocations, including scratch, caches, inactive resources and fallbacks. Actual
DRM residency is deduplicated by client/device; PSS covers the browser process tree.

| Method | Requested live / peak B | Resident VRAM / GTT B | Browser PSS B |
|---|---:|---:|---:|
| Unchanged baseline | 405,005,112 / 582,183,284 | 551,976,960 / 20,180,992 | 808,082,432 |
| Per-ray global ceiling | 405,005,112 / 585,120,968 | 551,936,000 / 20,099,072 | 813,744,128 |
| Cached global ceiling | 405,005,116 / 585,120,972 | 551,936,000 / 20,099,072 | 853,451,776 |
| Per-column local ceiling | 405,010,232 / 582,188,404 | 552,001,536 / 19,902,464 | 803,638,272 |

These are whole-scene snapshots, not physical peaks or per-probe attribution.
Do not sum VRAM, GTT and PSS on UMA. The four-byte and 5,120-byte increases
match the intended caches. Different allocation peaks and process residency do not
establish savings; no candidate reduces the requested working set.

## Initial fixture result and why it did not transfer

| Terrain / size | Baseline full / probe ms | Initial ceiling full / probe ms | Cached ceiling full / probe ms |
|---|---:|---:|---:|
| Flat / 320 | 4.16176 / 2.25776 | 4.06832 / 2.14972 | 4.11228 / 2.18924 |
| Flat / 1920 | 26.99228 / 2.29776 | 26.82440 / 2.16012 | 26.81528 / 2.18012 |
| Hilly / 320 | 4.22004 / 2.28120 | 4.16988 / 2.23780 | 4.19584 / 2.26400 |
| Hilly / 1920 | 26.82428 / 2.32940 | 26.72492 / 2.24068 | 26.75204 / 2.26200 |

The initial probe medians improve nominally by about 2–6%, while full-frame gains
are smaller. The cached summary does not improve those results consistently.
Clearing terrain includes mountains around 900–1,400 m, so a global bound is a weak
empty-space test for local rays. Local bounds add their own preparation/load cost.
The measurements support retiring this implementation; they do not prove that all
source-derived empty-space acceleration is ineffective.

| Terrain / width | Method | Requested live / peak B | Resident VRAM / GTT B | Browser PSS B |
|---|---|---:|---:|---:|
| Flat / 320 | Baseline | 213,942,848 / 246,056,604 | 287,657,984 / 10,477,568 | 597,524,480 |
| Flat / 320 | Initial | 213,942,848 / 246,056,604 | 287,662,080 / 10,477,568 | 597,839,872 |
| Flat / 320 | Cached | 213,942,852 / 246,056,608 | 287,674,368 / 10,477,568 | 599,844,864 |
| Flat / 1920 | Baseline | 399,393,136 / 431,506,892 | 504,496,128 / 19,066,880 | 621,379,584 |
| Flat / 1920 | Initial | 399,393,136 / 431,506,892 | 495,570,944 / 19,066,880 | 676,169,728 |
| Flat / 1920 | Cached | 399,393,140 / 431,506,896 | 495,583,232 / 19,066,880 | 675,828,736 |
| Hilly / 320 | Baseline | 213,942,848 / 246,056,604 | 287,657,984 / 10,477,568 | 598,064,128 |
| Hilly / 320 | Initial | 213,942,848 / 246,056,604 | 287,592,448 / 10,711,040 | 602,869,760 |
| Hilly / 320 | Cached | 213,942,852 / 246,056,608 | 287,600,640 / 10,477,568 | 602,513,408 |
| Hilly / 1920 | Baseline | 399,393,136 / 431,506,892 | 504,496,128 / 19,066,880 | 621,775,872 |
| Hilly / 1920 | Initial | 399,393,136 / 431,506,892 | 495,509,504 / 19,066,880 | 675,391,488 |
| Hilly / 1920 | Cached | 399,393,140 / 431,506,896 | 504,438,784 / 19,066,880 | 621,217,792 |

Fixture memory snapshots occur after 960 paced frames. Complete requested live
storage is unchanged for the first candidate and increases by four bytes for the
cached variant. Process/driver variation explains why a smaller observed snapshot
alone is insufficient to claim a representation memory win. Full fixture p95/p99,
host timings and all raw allocation observations remain in decision.json.

## Decision and next architecture bet

Retire terrain ceilings and stop tuning after the two allowed followups. Restore
engine, content, studio and examples exactly to the parent. Remove only the temporary
test's owned source files; preserve the test, failed candidates and outputs in the
evidence archive. No repeated audit of unchanged engine machinery is required.

The next useful question is the cost of repeated irradiance/visibility and shadow
evaluation inside volumetric scattering. First measure that contribution with a
controlled full-render ablation; only then select a source-radiance cache or learned
residual with an equally informed direct baseline. Keep geometry, cheap terms and
visibility exact where practical. A cache/model is justified by measured expensive
work and an error budget, not by its use of neural networks.

The default clearing's lack of recurring probe updates is a separate dynamic-lighting
response gap to understand before making G5 claims. Fixing or diagnosing it should
use the same refresh workload and image sequence for every architecture.
[G1–G7](neural-rendering-goals.md) and all previously frozen models remain unchanged
and unmet.

## Evidence and replay

Evidence on Pop!_OS:
`/home/ryanwible/worktrees/wrela-neural-frontier/build/neural-evidence/probe-free-space-v1`.
Scripts: `/home/ryanwible/.local/state/wrela-neural-frontier`, archived with dependencies.
Manifest SHA-256: `56cd426ca9e904e1547b0fe10faaffa8b22a650f8fcbc43ec13494671605b4e7`.

- plan.json records the start, deadline, parent, equal inputs and two followups.
- timing/ stores each fixture source archive, compiled profile, 54-point lighting
  capture, shaded image and allocation replay. There are 12 fixture profiles.
- production-clearing/ preserves the two default profiles with absent update work.
- production-fixed/ preserves the four decisive profiles, exact temporary app
  source, complete GPU/host timing and resident-memory observations.
- decision.json joins quality, memory, complete cost, limitations and the decision.
- retired-local-source.tar.gz keeps the last candidate and GPU tests;
  parent-source.tar.gz and restored-source.json identify the retained runtime.
- initial-parser-failed.log preserves the parser failure caused by reading the
  truncated console tail. The corrected parser reads the complete results/log.txt.
  No rendered result was discarded or recaptured to repair that parser.

The fixture batches all 54 lighting queries plus the shaded frame in each executable.
The allocation observer replays that executable rather than recompiling it. Separate
source cases/resolutions still require separate profiles; no fictitious single-build
claim is made. Candidate preparation precedes a serial quiet AMD queue; CPU scoring
does not overlap timed rendering. Replay in a fresh path from the archived source
and pinned toolchain, never by mutating sealed evidence.
