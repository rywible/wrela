# Source-sized shape interpreters: keep capacities, retire pruning

2026-10-30 UTC. **Continue source-sized interpreter capacities; retire spatial
pruning and active-source compaction.** Three clean, source-matched Last Green
comparisons reduce complete GPU frame-span sums by **4.22–4.67%** over 660 frames,
and **14.34–15.20%** in the movement window. Movement's worst GPU frames fall
from **223.13–225.69 ms to 133.39–134.79 ms**. This is an incremental conventional
runtime result on top of [source-selected slabs](neural-distance-slabs.md).

First matched joint assessment: **18.58 minutes**.
Decision: **31.63 minutes**, after two targeted optimization
followups. No training; all seven model artifacts remain frozen. G1–G7 are
unchanged and unmet. The selected path adds **1,672,320 requested live bytes**
in these matched runs. It makes **no physical-memory saving claim**.

## What Wrela's source enables

The pinned std::stage::interpret reserves an N-float workspace and runs a
tape's actual operations. The engine's general shape type allows 512 operations;
observed Last Green shapes use at most 368. Source programs already expose their
length and a checked resize operation. No learned prediction is needed to know
which capacity holds an unchanged program.

[Distance](../engine/distance.wrela) now records the maximum source-program
length touching each physical z slab. A slab with no shapes keeps its previous
ground/capsule kernel. Otherwise it selects a 256-, 384-, or original 512-operation
interpreter. The smaller copies preserve the original operations, constants,
transforms and sphere bounds. Programs too large for a copy leave an unused empty
slot there; the source-derived slab maximum prevents that slot being evaluated.

The original full programs remain resident for the fallback. Both compact arrays,
their conversions/uploads, CPU sources, driver caches and all other passes are
charged. This representation duplicates programs; it is not a compressed
appearance model. The same eight CPU slab words now hold lengths instead of
presence flags. No update cadence, kept region, validity rule, visibility,
geometry or quality setting changes.

The pinned interpreter/resize API text is preserved in the evidence bundle.
A smaller static workspace is the implementation change; register occupancy or
spill traffic was not measured, so neither is claimed as the hardware mechanism.

## Fair workload and measurement limits

Solo Pop!_OS, RADV RENOIR PCI 1002:164c, Ryzen 5 5500U, Mesa 26.1.6,
Chrome 154.0.8037.57, Wrela 0.9.0/compiler
de9b81e52a7747870dfbd5223cb8aeb41e584d23. Kernel and per-run conditions are retained;
clocks were not locked. Output 1920×1080, unchanged Quality scale 0.667,
one worker. Reuse the previous controlled Last Green fixture and completed
measurement machinery; generate no new reference family.

The fixture renders the initial World snapshot with frame-defined render time
and camera while background simulation continues. It cuts vertically 96 m at
frame 240, returns at 300, moves 96 m forward during 360–539, then settles.
The three selected comparisons have identical camera, source-arrival census
and player/loaded-instance records. All variants receive the same authored fields
and bin information. The control is the prior source-specialized slab renderer.

Reported windows, inclusive: all 0–659; startup 0–119; cut 239–298; return
299–358; movement 359–538; settled 600–659. The movement window includes the
frame before motion and excludes its last frame, equally for every method.

Each variant compiles once and reuses its executable for fresh browser runs.
Seven periodic images come from each initialized run; the duplicate final pose
is not counted again. Capture costs apply equally. GPU timings use the complete
frame span, including submission gaps, with CPU calls and browser intervals
reported separately. The rendering includes decoding, sorting/visibility,
wood, updates and compositing. Actual residency is sampled throughout every run.
Image scoring and builds run outside GPU measurements.

These are controlled production-renderer measurements, not ordinary interactive
gameplay, independent object-motion qualification or converged light-transport
truth. Equivalent source rendering does not establish G1/G3 reference gates.

## Complete runtime and resident memory

Pairs 3 and 4 provide clean forward/reverse comparisons; pair 5 confirms the result.
Values are observations, not confidence intervals.

| Pair | Full GPU reduction | Movement GPU reduction | Browser elapsed Δ s | End PSS Δ MB | End VRAM Δ MB |
|---|---:|---:|---:|---:|---:|
| 3 | 4.30% | 15.03% | -1.126 | -1.95 | 10.44 |
| 4 | 4.22% | 14.34% | -0.780 | -78.35 | 2.42 |
| 5 | 4.67% | 15.20% | -1.444 | -72.86 | 2.47 |

Distance work over 660 frames falls from 4.17–4.20 s to 2.165–2.180 s.
The movement-window worst-frame reduction is substantial, but 133–135 ms still
misses the frame budget. Settled rendering remains around 42 ms. Startup still
contains multi-second GPU spans; these are included in the complete-route result.
CPU improvement is not established: movement CPU p95 ranges
138.82–150.33 ms versus 141.93–145.17 ms for these controls.

All requested resources include copies, scratch, caches, fallback programs and
output. Whole-browser PSS includes CPU/driver allocations. **VRAM, GTT and PSS
overlap on UMA; do not sum them.** Sampled maxima can miss shorter peaks;
sampling counts, gaps and raw process/DRM data are retained.

| Run | Requested live / peak B | Max VRAM MB | End VRAM MB | Max PSS MB | End PSS MB |
|---|---:|---:|---:|---:|---:|
| pair3-parent | 706,199,572 / 893,736,608 | 1078.62 | 935.36 | 4321.57 | 1095.24 |
| pair4-parent | 706,199,572 / 908,854,508 | 1089.86 | 926.47 | 4241.10 | 1168.54 |
| pair3-capacity | 707,871,892 / 893,059,016 | 1063.94 | 945.80 | 4293.96 | 1093.28 |
| pair4-capacity | 707,871,892 / 895,052,252 | 1080.72 | 928.90 | 4258.67 | 1090.19 |
| pair5-parent | 706,199,572 / 902,444,324 | 1098.25 | 926.53 | 4240.65 | 1166.65 |
| pair5-capacity | 707,871,892 / 903,654,804 | 1083.50 | 928.99 | 4223.85 | 1093.80 |

The compact arrays add 1.67 MB of requested live data. End VRAM rises
2.42–10.44 MB in these matched runs, while end PSS falls 1.95–78.35 MB.
Other preserved runs show PSS increases. These observations do not isolate a
causal physical-memory change, and no resident-memory saving is claimed.

## The two rejected followups

1. **Per-bin spatial pruning.** Transform each bin's bounds into local space,
   expand for transform rounding, and use the pinned source-field interval pruner.
   Store one pruned program per shape/bin, then select the same capacities.
   Two clean matched pairs produce 2.166–2.173 s of distance work, versus
   2.165–2.180 s for capacity selection: no consistent additional benefit.
   Small complete-route differences include startup variation. End PSS rises
   60.06–133.47 MB against controls. Retire it; its field-domain machinery and
   repeated preparation do not earn their cost here. It is not promoted as
   independently exact: only the selected architecture gets the full field oracle.

2. **Active original-program compaction.** Use existing bins to upload only
   referenced original shapes, deduplicate them and remap their indices;
   remove pruning. Source-matched comparisons reduce full GPU sums by
   4.63% and 5.69% versus their controls, but add 47.63 MB and 192.16 MB of end PSS.
   Requested allocation and VRAM are lower in these observations; those overlapping
   pools do not cancel the PSS increase. Its small additional speed does not justify
   this observed residency tradeoff over the simpler capacity selector.
   Retire it rather than start a third optimization to explain or repair it.

All sources, executable runs, memory observations and failed comparisons remain.
No result is deleted because it makes an approach look worse.

## Mismatches and verification

Initial pair 1's baseline overlapped a CPU toolchain-context query. It is preserved
but excluded from the primary timing claim. Initial pair 2 and later capacity
pair 6 differ at one early capsule arrival; their image differences reach 24/255.
They do not qualify source-matched image equivalence or primary rendering gains.

Both nominal active-compaction pairs had the same arrival ambiguity. Their four
runs form exactly two source-census groups, with one control and one candidate
in each. Matching solely by exact source/camera/player records swaps the partners:
parent 5 with active 6; parent 6 with active 5. Every run is used once, both
measurement orders remain, and all original comparisons are preserved.
No timing or memory value selects a partner. The resulting 14 paired images
are within 2/255, but the memory tradeoff still rejects that architecture.

For the retained capacity selector, **21 paired images** differ by at most
**1/255 per RGB channel**, including visible boundary pixels; alpha matches.
This is parent-renderer equivalence, not new silhouette precision/recall evidence.

The same-frame oracle compares **47,775,744 active-slab
values bit for bit** against the original 512-operation general kernel.
There are zero differing values and zero maximum error. It covers startup,
source arrivals, camera cuts, return, toroidal movement and kept regions.
The oracle renders at 320×180 while retaining the full 192×64×192 distance grid;
diagnostic reference-volume/counter cost is excluded from production measurements.
Every comparison dispatch is accounted for by the final readback count.

All five existing [distance GPU tests](../engine/tests/distance/main.wrela) pass.
Deliberately replacing fitting compact programs with empty programs fails the
analytic cuboid-distance check. The negative source, failure and restoration
record are preserved. No repeated audit of unchanged rendering machinery was needed.

## Decision and next architecture-level bet

Keep source-sized interpreter capacities for repeatable whole-renderer savings
and materially smaller update hitches, accepting the extra requested memory.
Retire per-bin pruning and active compaction. This does not achieve G4 appearance
compression, G5 transport accuracy/cost, or G6's complete-frame budget.

The next larger opportunity is **sharing source-derived foliage visibility
between the depth prepass and shaded pass**. Existing matched settled timings
attribute about 8.95 ms/frame to vegetation and 7.65 ms/frame to its prepass,
versus about 42.1 ms for the complete GPU span. Investigate the repeated work,
then compare a shared-visibility representation against an equally informed
conventional path, counting intermediate memory, updates and silhouette errors.
No such gain is claimed in this block.

## Evidence and replay

Pop!_OS:
 /home/ryanwible/worktrees/wrela-neural-frontier/build/neural-evidence/shape-capacity-v1

Parent 3c197161ae3267b03b46e8a7bf397b6a0607f5c6.
Manifest SHA-256: bc8b2c6dd3a61c5b671e8a17a322c51a84b3270769207676f0252250f67970df.

The sealed bundle includes plans, all variants and hooks, executables, commands,
captures, complete timings, requested/actual memory, source-census mismatches,
negative failure, exact-field oracle, scripts and frozen goal/model hashes.
[Previous slab selection](neural-distance-slabs.md),
[measurement semantics](toolchain.md) and [metric/source policy](citations.md)
define inherited methods. No external code, weights, asset or hardware result
was imported.
