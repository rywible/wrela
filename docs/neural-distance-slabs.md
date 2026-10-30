# Distance update slabs: retain exact source selection

2026-10-30 UTC. **Change the architecture: retain direct slab-level kernel
selection and retire per-bin GPU routing.** Four balanced pairs on the controlled
Last Green route reduce complete GPU frame-span sums by
**18.16–19.79%** over all 660 frames
and **39.48–42.37%** across the
180-frame movement window. Browser elapsed time falls as well. Memory and tails
remain material tradeoffs, recorded below. This is a conventional rendering
result. G1–G7 remain unchanged and unmet; all seven model artifacts stay frozen.

First joint assessment: **31.88 minutes**.
One targeted optimization followup, zero training, no new reference generator.
Decision: 58.27 minutes. Preserve the failed initial routing
idea, unmatched stock walking comparison, verification-wrapper failure and all
memory/timing outliers.

## The exact source information used

[Distance](../engine/distance.wrela) already bins capsules and shape tapes around
the camera. The authored scene eventually supplies 134 shapes (at most 368
operations) and thousands of capsules, but only 0–45 of 576 spatial bins contain
shapes in the observed route. A global nonempty shape list unnecessarily sends
shape-free update slabs through the general interpreter.

The CPU now marks each of eight physical z slabs that intersects a shape-bearing
bin. The mapping uses the same toroidal integer coordinates as the volume.
A marked slab keeps the original general kernel; an unmarked slab uses the
original ground/capsule kernel. Every bin and source-version change rebuilds the
flags. Kept regions, volume validity, the update schedule, shading and geometry
are unchanged.

The new metadata is eight u32 flags: **32 logical CPU bytes**. There are no new
GPU buffers, textures or fallback representations. The complete emitted WGSL
shader set is **byte-identical to the parent**. Actual residency is measured,
not inferred from this small metadata count.

## Why the finer router was rejected

The initial candidate split every slab into two GPU dispatches, with disjoint
per-bin guards and a shape flag in existing bin headers. Its corrected complete
comparison moved 47,099.92 → 46,856.48 ms over 660 frames, only 0.52%; the central motion
window moved 5,623.95 → 5,569.97 ms. The general kernel still consumed most of its
previous time. Its 47,775,744 same-frame field comparisons and five GPU tests pass,
so performance, not a demonstrated field-quality failure, motivates the change.

The one optimization followup tested skipping the general dispatch when an entire
physical update slab has no shapes. Both contenders received exactly the same
source-derived slab flags: a direct selector and the finer bin router.

The finer router's complete-route result versus the direct selector changes sign:
0.38% faster in forward order and 0.58% slower in reverse order. One finer-router
run also differs in its source-arrival census and images. No consistent added
benefit justifies its extra shader paths and packed header format. Retain the
direct selector and the original shaders.

## A matched production-renderer workload

Solo Pop!_OS: AMD RADV RENOIR PCI 1002:164c, Ryzen 5 5500U, Mesa 26.1.6,
Chrome 154.0.8037.57, Wrela 0.9.0/compiler
de9b81e52a7747870dfbd5223cb8aeb41e584d23. Output 1920×1080; the game's
unchanged Quality scale 0.667. One worker, one quiet serial GPU queue;
no concurrent image scoring, training or builds. Clocks were not locked;
per-run conditions are retained.

The first stock walking runs had different tick/source/camera records and image
differences up to 214/255 per channel. They cannot establish matched quality or
moving-scene speed. The initial stock shot also has a large first-run startup
difference; no causal cache attribution or rendering win is claimed from it.

The corrected diagnostic renders the same initial World snapshot and a camera
defined by the frame number. Background simulation still runs. The route starts
near the gate, cuts vertically 96 m at frame 240, returns at 300, travels 96 m forward
during 360–539, and settles. Frame-defined render time removes simulation-state
drift from drawing. All four final pairs have identical camera, player/loaded
instance records and complete shape/capsule arrival censuses.

These are authored Last Green sources and the complete rendering pipeline,
including visibility, geometry, wood, decoding, distance/probe updates and
compositing. This controlled render path is not ordinary interactive gameplay,
independent object-motion qualification, or independently converged transport
truth. No scene density or quality setting is reduced for the candidate.

Each variant compiles once; its executable is reused for all browser runs.
Each run captures seven periodic images in one initialized scene. The initial
separate final image duplicates the final periodic pose and is not counted
again in the 28 final paired image comparisons. Capture costs are included
equally. No second reference infrastructure was built.

## Complete runtime

The full GPU frame span includes gaps between submissions; it is not a sum of
selected pass medians or an assertion of uninterrupted GPU occupancy. CPU calls,
browser intervals and elapsed time are retained separately. Four fresh browser
pairs alternate order. Values below are observations, not confidence intervals.

Reported frame windows are inclusive: all 0–659, startup 0–119, vertical cut
239–298, return 299–358, movement 359–538, and settled 600–659. The movement
window includes the frame before motion and excludes its last frame; the exact
camera movement is 360–539. These same windows apply to every variant.

| Pair | Full 660-frame GPU reduction | Movement 180-frame GPU reduction | Whole-route browser elapsed Δ s | End PSS Δ MB | End VRAM Δ MB |
|---|---:|---:|---:|---:|---:|
| 1 | 18.16% | 39.55% | -7.840 | 4.95 | -8.41 |
| 2 | 18.58% | 39.48% | -7.906 | 168.90 | 0.06 |
| 3 | 18.61% | 39.65% | -8.323 | 7.83 | 8.83 |
| 4 | 19.79% | 42.37% | -8.591 | 84.58 | 8.35 |

Across 660 frames the distance work falls from
13.02–13.83 s to
4.18–4.21 s.
The shape-free vertical-cut window's worst GPU frame falls from
208.34–209.64 ms to
45.87–46.11 ms.

This does **not** fix shape-containing update hitches: the movement window still
has worst frames of 225.04–233.58 ms.
Settled rendering is effectively unchanged, around 42 ms per GPU frame.
CPU improvement is not established: movement CPU p95 ranges
133.69–176.70 ms versus
139.00–144.57 ms for controls.
Startup has multi-second GPU frame spans and large CPU spikes.
G6's 60 fps milestone and 120 fps target remain unmet.

The fourth control is slower than the earlier controls, including roughly 0.8 s
more distance work in the movement window. It is preserved without a specific
causal attribution. The first three pairs independently show 18.16–18.61%
complete-route reduction; the larger fourth observation is not needed to select
the architecture.

## Complete memory

All requested resources include scratch, caches, sorting/visibility, wood,
fallback data and output. Whole-browser PSS includes CPU sources and driver
caches. **VRAM, GTT and PSS overlap on UMA; do not sum them.** Samples are actual
observations, but maxima can miss shorter peaks. Sampling counts/gaps and raw
per-process data are retained.

| Run | Requested live / peak B | Sampled max VRAM MB | End VRAM MB | Sampled max PSS MB | End PSS MB |
|---|---:|---:|---:|---:|---:|
| pair1-parent | 706,199,572 / 893,841,692 | 1078.55 | 943.74 | 4240.01 | 1031.47 |
| pair1-direct | 706,199,572 / 908,854,508 | 1089.80 | 935.33 | 4277.82 | 1036.42 |
| pair2-parent | 706,199,572 / 906,983,700 | 1098.60 | 935.40 | 4276.49 | 996.18 |
| pair2-direct | 706,199,572 / 908,854,508 | 1081.83 | 935.45 | 4250.62 | 1165.08 |
| pair3-parent | 706,199,572 / 903,085,448 | 1098.24 | 926.53 | 4194.24 | 1165.11 |
| pair3-direct | 706,199,572 / 891,207,332 | 1087.00 | 935.36 | 4238.98 | 1172.94 |
| pair4-parent | 706,199,572 / 893,736,608 | 1078.62 | 935.40 | 4279.81 | 1028.11 |
| pair4-direct | 706,199,572 / 905,078,684 | 1089.79 | 943.76 | 4328.20 | 1112.68 |

The observed end-PSS increases of 168.90 MB and 84.58 MB remain charged alongside
the smaller 4.95 MB and 7.83 MB observations. The extra two pairs were run specifically
because the first large increase was unresolved. Its cause is not established,
and this result makes **no physical-memory saving claim**. No new requested GPU
resource follows from the selector; observed allocation-lifetime and residency
variation is still part of the complete comparison.

## Rendered quality, boundaries and verification

All **28 paired full-image captures** differ by at most
2/255 in any RGB channel, including pixels on
visible boundaries; no pixel exceeds 2/255. Alpha matches throughout. This is
parent-renderer equivalence, not new G1/G3 silhouette precision/recall evidence.

The selected same-frame oracle compares **48,070,656 active-slab
voxel values bit for bit** against the original general kernel on the same terrain,
capsules and shape tapes. It covers startup source arrivals, cuts, return,
toroidal movement and kept regions. The rendered oracle viewport is 320×180 but
the distance grid remains 192×64×192. Reference volume/counter overhead is
diagnostic and excluded from production runtime/memory.

All five existing [distance GPU tests](../engine/tests/distance/main.wrela) pass,
including shape tapes, capsule source edits, probes and contact shadows.
Deliberately suppressing the slab flags causes the test's GPU bounds check to
reject selecting a capsule kernel for shape-tagged indices. The first wrapper
incorrectly demanded the later numerical assertion; its failed expectation,
mutated source and actual rejection are preserved. Qualification accepted the
specific bounds failure without rerunning unchanged tests. Production source
was restored after the negative check.

No material training or repeated reference generation was warranted. The parent
GPU programs are unchanged, all selected field values match, and final paired
images remain within the observed two-byte-level limit.

## Decision and next architecture-level bet

Keep the direct selector for its repeatable complete rendering-time reduction
with exact field values, while charging the resident-memory increase and retaining
the original general path. Retire both per-bin GPU-routing variants. This is a
runtime tradeoff, not G4's 16× appearance-memory compression or a solved G6 budget.

The next bet is **smaller exact shape programs for occupied regions**, using
source-field bounds and program requirements to reduce the remaining interpreter
work. Compare against an equally informed conventional pruned/compiled program,
charge source-update work, caches and fallback memory, and qualify worst-frame
cost as well as throughput. The remaining 4.2 s of distance work and 220 ms
shape-update hitches make that question consequential; no such optimization is
claimed here.

## Evidence and replay

Pop!_OS:
 /home/ryanwible/worktrees/wrela-neural-frontier/build/neural-evidence/shape-bin-routing-v1

Parent 71deb5fc8d8bc035a981420ee8935456827d5e79.
Manifest SHA-256: 595042400ec00b4617fad17b7eebdd1bb12fc2ef5fd53975baa05f03f91f5a21.

The bundle preserves all variants, stock-run mismatches, exact source hooks,
executables, commands, raw captures, timings, resident/requested memory, conditions,
field oracles, negative-test failure, scripts and frozen hashes.
[Prior source specialization](neural-distance-specialization.md),
[distance implementation](../engine/distance.wrela),
[toolchain measurement semantics](toolchain.md) and
[metric/source policy](citations.md) define inherited methods.
No external code, asset, learned weights or hardware result was imported.
