# Checked native field programs: retain direct execution

2026-10-30 UTC. **Continue the direct source-program architecture; retire the
source-buffer cache follow-up.** Four source-matched Last Green comparisons show
**5.74–7.71% lower complete GPU frame-span sums** over 660 frames. Distance work
falls from **2.15–2.17 seconds to 64.0–65.1 milliseconds**, a **33.2–33.8×**
reduction. Movement's worst GPU frames fall from **127.74–134.27 ms to
44.38–54.54 ms**. Two comparisons against a separately optimized conventional
control also favor the retained path by **5.67–7.63%**.

This is a conventional rendering result from Wrela's inspectable source
programs. It adds **210,112 requested live GPU bytes** at 134 shapes and makes
**no physical-memory saving claim**. Numeric output is not bitwise identical:
15 of 47,775,744 checked grid values differ, at most 2.384185791015625e-7, with
no sign changes. All selected paired images differ by at most 1/255 per RGB
channel, including visible boundaries; alpha matches.

First rendered quality/cost assessment: **34.56 minutes**. The complete
assessment with the resolved field discrepancy and repeats: **40.07 minutes**.
Architecture decision: **49.998 minutes**, after **one** optimization
follow-up. No training, no new shaded-reference family, and no second
optimization. Frozen models and G1–G7 are unchanged and unmet.

## What source knowledge buys

[Distance](../engine/distance.wrela) already selects no-shape, 256-, 384- or
512-operation interpreters from exact bin/slab source information.
The new [direct evaluator](../engine/direct_fields.wrela) recognizes 16 observed
operation layouts and executes straight-line scalar/native-integer expressions.

Selection checks the entire opcode/register graph and the integer-half selectors.
Every numeric material/geometry constant remains a runtime parameter. Unknown
graphs and table-bearing programs remain on the existing interpreter. A slab
uses the direct kernel only when every shape its bins list is recognized.
Transforms, sphere rejection, ground columns, capsules, bin order, kept regions,
update cadence and downstream shading are unchanged.

The inspection capture contained 68 shapes, all without external constant tables.
The production route later held 134 shapes: 133 recognized and one unfamiliar.
The unknown-program fallback remains resident. The dictionary is an observed-layout
specialization, not a universal compiler or an unseen-scene generalization claim.

A literal expansion initially produced a 432 KiB shader and failed the pinned
toolchain's 256 KiB pipeline limit. Source tapes encode integers as exact
high/low 16-bit float pairs. After checking those selectors, the generator fuses
666 adjacent pairs into native u32 operations, retaining exact high/low values
where scalar operations use them. The resulting additional WGSL pipeline is
**170,969 bytes**. The sources and failed expansion are preserved.

This lowering follows the pinned toolchain's scalar, packed-integer, conversion
and vector operation semantics; those primary source/API extracts are in the
evidence bundle. Floating-point compiler behavior still differs at 15 grid
values, so this is deliberately not described as bitwise exact.

## Fair comparison and complete cost

Solo Pop!_OS, AMD RADV RENOIR PCI 1002:164c, Ryzen 5 5500U, Mesa 26.1.6,
Chrome 154.0.8037.57, kernel 7.1.5-76070105, Wrela 0.9.0/compiler
de9b81e52a7747870dfbd5223cb8aeb41e584d23. Clocks were not locked; per-run
conditions are retained. Output 1920×1080, unchanged Quality scale 0.667,
one worker, fresh browser profiles, paced mode and two frames in flight.

Reuse the [established route](neural-distance-capacity.md): held initial World,
frame-defined time/camera, vertical cut at 240, return at 300, 96 m movement
during 360–539, then settle. Background production jobs continue. Match
camera, full source-arrival census and player/loaded-instance records; reject
mismatches. Counts do not constitute hashes of every numeric source byte.

Each executable renders seven periodic captures at frames
299/359/419/479/539/599/659. All builds finish before the serial AMD timing queue.
No compilation, image scoring or toolchain queries overlap GPU timing/capture.
Sixteen production runs yield 112 periodic PNGs. The field diagnostic has its
own seven PNGs and is excluded from production timing.

Both methods receive the same original source tapes, constants, transforms,
bounds, bins and visibility information. The control includes all prior
ground-column, source-slab, interpreter-capacity and foliage improvements.
The cache follow-up is applied separately to both control and candidate; the
retained direct path is also compared against that optimized conventional
control. There is no input privilege for a learned or direct method.

The complete GPU frame span includes submission gaps and all rendering:
decoding, sorting/visibility, wood, source updates and compositing. CPU frame
calls, browser intervals, opening-to-first-frame and opening-to-last-frame-begin
are also retained. First-frame load in the selected comparisons is about
1.14–1.22 seconds, with no consistent load-time improvement. Runtime shader
and driver caches are charged through sampled process/DRM residency. Toolchain
builds and diagnostic reference dispatches are outside production frame timing.

| Pair / control | Full GPU reduction | Movement GPU reduction | Distance total ms | Movement worst GPU ms | Browser route Δ s |
|---|---:|---:|---:|---:|---:|
| 1 / parent | 7.71% | 19.69% | 2163.24 → 64.20 | 134.27 → 44.38 | -1.820 |
| 3 / parent | 5.74% | 19.71% | 2173.05 → 64.30 | 127.74 → 47.82 | -0.942 |
| 4 / parent | 5.98% | 19.65% | 2150.06 → 63.99 | 129.50 → 54.42 | -1.529 |
| 5 / parent | 6.43% | 19.93% | 2158.53 → 65.09 | 129.39 → 54.54 | -1.570 |
| 5 / cached-parent | 5.67% | 19.54% | 2156.98 → 65.09 | 129.33 → 54.54 | -1.611 |
| 6 / cached-parent | 7.63% | 19.45% | 2166.62 → 65.21 | 128.56 → 53.94 | -1.584 |

Pairs 1, 3, 4 and 5 are the original-control comparisons. The two cached-control
comparisons share the direct5 run with one original comparison. These are
observations, not independent statistical trials or confidence intervals.
The matched original comparisons contain 28 paired views; the two supplemental
comparisons contain 14, with seven candidate views reused.

CPU movement p95 does not consistently improve: original controls
138.35–148.44 ms, matched direct runs 140.22–142.42 ms. Settled GPU p50 is about
36.44–36.62 ms. Startup multi-second spans remain included. These gaps still
prevent the broader frame-budget claims.

## Actual residency

All original full tapes and compact fallbacks remain allocated. Each direct
shape adds 1,568 bytes (program ID, parameters, transform, sphere and alignment):
134 × 1,568 = 210,112 requested live bytes. The complete allocation observer
includes output, scratch, caches and other resources; process-group PSS includes
CPU/driver allocations.

All amounts below are decimal MB. Requested columns are live / peak;
physical columns are sampled maximum / final. **VRAM, GTT and PSS overlap on
UMA and must not be summed.** Sampling requests a 50 ms sleep plus reading
overhead; gaps and sample counts are retained. Sampled maxima are not physical
peak guarantees.

| Run | Requested live / peak MB | VRAM max / end MB | GTT max / end MB | PSS max / end MB |
|---|---:|---:|---:|---:|
| pair1-parent | 707.87 / 892.42 | 1080.71 / 929.05 | 22.75 / 22.71 | 4322.43 / 1091.98 |
| pair1-direct | 708.08 / 906.44 | 1067.79 / 913.24 | 22.68 / 22.63 | 4267.42 / 1093.06 |
| pair3-parent | 707.87 / 903.65 | 1083.57 / 920.58 | 23.12 / 23.10 | 4252.38 / 1090.13 |
| pair3-direct | 708.08 / 897.11 | 1056.54 / 929.42 | 22.75 / 22.71 | 4246.10 / 1093.52 |
| pair4-parent | 707.87 / 903.65 | 1092.48 / 928.96 | 23.06 / 23.02 | 4228.02 / 1093.13 |
| pair4-direct | 708.08 / 892.57 | 1073.74 / 921.73 | 22.91 / 22.87 | 4327.79 / 1101.22 |
| pair5-parent | 707.87 / 906.29 | 1083.55 / 920.51 | 22.65 / 22.63 | 4256.91 / 1090.11 |
| pair5-direct | 708.08 / 892.57 | 1064.98 / 929.64 | 22.96 / 22.92 | 4252.47 / 1227.55 |
| pair5-cached-parent | 707.87 / 895.05 | 1072.33 / 945.81 | 22.67 / 22.63 | 4274.75 / 1091.61 |
| pair6-cached-parent | 707.87 / 910.07 | 1083.49 / 920.48 | 22.60 / 22.54 | 4288.35 / 1139.20 |
| pair6-direct | 708.08 / 903.98 | 1064.99 / 946.81 | 22.74 / 22.71 | 4242.65 / 1094.00 |

Across the four original-control pairs, final PSS increases by **1.08–137.44 MB**;
the 137.44 MB observation is retained, not discarded as an outlier. Sampled
maximum PSS changes by **-55.01 to +99.78 MB**. End VRAM varies in both directions.
Against cached controls, end PSS changes by -45.20 and +135.94 MB. These data do
not isolate a causal physical-memory change or establish a memory saving.
The runtime advantage justifies continuing this limited architecture, while
physical residency remains a measured gap.

## Quality, failures and proportional verification

The strict same-frame bitwise oracle failed and remains a failed result.
A reporting version then reused the original 512-operation kernel, source
arrays, bins, columns and full 192×64×192 distance grid. Across 162 comparison
dispatches it checks **47,775,744 values**, including source arrivals, cuts,
return, movement and kept regions. Final readback accounts for every dispatch:
15 unequal values, maximum error **2^-22**, none above 1e-5, and zero signed-distance
sign changes. Diagnostic output is 320×180 but the distance grid is full size.

Selected source-matched RGB images are within 1/255 at every pixel. This checks
visible boundaries against the existing renderer; it is not new silhouette
precision/recall against geometric truth, a converged transport reference, or
full G1/G3/G4 qualification. The completed Clearing references contain no shape
tapes, so rerunning them would not validate this changed path and was omitted.

Original pairs 2 and 6 differ in an early capsule arrival (5,907 versus zero).
The parent/cached-parent comparison in round 6 has that same mismatch.
All three comparisons are preserved and excluded from rendering-advantage claims.
Their maximum RGB difference reaches 24/255.

The sole optimization follow-up retains GPU source buffers over camera-only
grid movement and recompiles/uploads on explicit source changes. Its 188 control
and 189 direct GPU shaders are byte-identical to their respective originals.
Compared with the uncached direct path, it is **0.72–1.34% slower** over the full
GPU route and **0.35–0.46% slower** in movement. Round 6 changes pixels by up to
**21/255** despite matching census/camera/player records. The underlying source
or resource lifetime cause is unresolved. Retire this cache; no third variant
or extra reference system was built to rescue it. The conventional cache
control still provides the stronger-input-fairness comparisons above.

All five existing [distance GPU tests](../engine/tests/distance/main.wrela) pass.
Forcing an unknown graph into direct program 1 makes the independent analytic
cuboid test fail by **4 m**. Its mutation, failure and exact restoration are saved.
Final cleanup removes diagnostic prints and an unused helper and formats the two
runtime files. A final build verifies all **189 GPU shaders are byte-identical**
to the measured retained candidate. No unrelated suite is rerun.

The bundle also preserves the private-field export error, two five-minute
export-fixture timeouts, native-host usage error, 62-versus-68 export-count
assumption, oversized shader and unsigned-selector literal error. None is a
rendering win.

## Decision and next architecture-level bet

**Continue checked native field execution. Retire source-buffer caching.**
The measured quality and full-renderer speed support the direct path despite
its small requested allocation increase and variable physical residency. The
7,349-line generated layout catalog is a prototype with finite coverage,
not the desired long-term authoring interface.

Next bet: make native source-field evaluation a reusable build-time capability
for visibility and temporal correspondence, with an explicit interpreter fallback
for unfamiliar graphs. Give conventional reconstruction and any neural model the
same source-derived correspondence and validity information. This route's
remaining steady labels include approximately **7.44 ms/frame for vegetation's
prepass** and **6.33 ms/frame for temporal reconstruction**; they offer larger
remaining targets than this now-small distance update. No gain for that future
architecture is claimed here.

## Evidence and replay

Pop!_OS evidence:
 /home/ryanwible/worktrees/wrela-neural-frontier/build/neural-evidence/direct-field-programs-v1

Parent d1cabd7648ca6dd54156a03c49ef2a7a3d6a74c6.
Manifest SHA-256: 29f97c0494fde7d16077eae4b8ef2a7a2b5a0275f04c279f3bfe2c89d49e9d92.

The bundle contains exact exported programs, opcode/API sources, generated
variants, failed sources/logs, executables, raw timings, load and residency data,
all images, field-oracle readbacks, tests/mutation, frozen goals/models, scripts
and final source patch. Scripts live outside the Wrela-only repository and are
copied into the sealed bundle. Regeneration starts from source-programs.json and
opcodes.json, then prepare_direct_field_candidate.py, pack_direct_field_integers.py
and repair_direct_field_types.py; retained cleanup removes diagnostics and formats.
Use a fresh evidence directory and update fixed paths before replay.

[Interpreter capacities](neural-distance-capacity.md),
[previous retained occlusion result](neural-sphere-refinement.md),
[measurement semantics](toolchain.md) and [source policy](citations.md)
supply inherited methods. Packed-integer helpers follow the pinned Wrela toolchain sources preserved in
the bundle. No third-party weights, assets or external hardware comparison were
imported.
