# Distance fields: retain exact source specialization

2026-10-30 UTC. **Continue this architecture and retain the source-specialized
distance kernels.** Across the final four BC/source-ID camera-motion pairs,
complete GPU cost falls by 3.24 to 3.28×.
The speedup is specific to the measured clearing, whose distance source contains
ground and no shape tapes. Capsule behavior and the general shape fallback are
separately tested. This is a conventional rendering win; G1–G7 remain unchanged
and unmet, and all seven model artifacts remain frozen.

The first complete quality/runtime/memory assessment took
**29.52 minutes**. One targeted optimization followed,
with no training or new reference-generation scheme. The decision was recorded
at 70.78 minutes. Failed hypotheses, compiler attempts,
deliberate test corruption and all measurement outliers are preserved.

## What Wrela's source information buys

The renderer knows whether the current source contains shape tapes. Its original
distance shader nevertheless includes the general field interpreter, with a
512-float workspace in emitted WGSL. We now choose an exact capsule/ground
kernel when the source's shape list is empty; any shape tape selects the general
kernel. This choice uses runtime source truth, not a learned prediction, camera
reference or hidden input.

Terrain height and normal depend only on horizontal position. A first pass
computes them once per active column; all 64 vertical voxels reuse the values.
The 192×192 vec2 scratch costs **294,912 requested bytes** and is included in
memory. Coordinates, kept regions, update order, capsule geometry, visibility,
volume reads and general shape semantics remain unchanged.

The [previous block](neural-source-demand-runtime.md) proposed preserving valid
regions across interrupted updates. That scheduling change was not implemented.
This block instead isolates source specialization and exact reuse of terrain
samples. The existing interrupted-cook restart behavior remains.

## Failed first hypothesis and fair control

Column reuse alone passed the 36 shaded checks, but full GPU cost for the initial
60-frame pan only moved from 4261.15 to 4230.03 ms. Its terrain-column pass took
0.93 ms while the general volume pass still took 2929.97 ms. Repeated terrain
queries were therefore not the consequential remaining bottleneck.

The one optimization followup removed the unused shape interpreter from the
no-shape source class. Both the direct-ground conventional kernel and the
column variant received this exact same source classification. Two fresh
forward/reverse comparisons per method measured:

| Method | Complete pan GPU, 60-frame sum ms | Distance work, sum ms |
|---|---:|---:|
| Original general kernel | 4279.43 to 4287.06 | 2975.68 to 2984.07 |
| Source-specialized direct ground | 1326.56 to 1328.73 | 17.40 to 17.60 |
| Source-specialized shared columns | 1315.71 to 1318.97 | 6.98 to 6.98 |

Columns add a smaller **0.73 to 0.82%**
complete-pan improvement over the equally informed specialized direct control.
The large improvement comes from source specialization. The WGSL workspace
motivated the experiment; no ISA register/spill counters were collected, so a
specific hardware mechanism is not established.

## Complete runtime and memory

Same 12-kind clearing, 2,485 instances, 1920×1080 output, normal internal 2×
rendering, one worker, 600 warmup +60 final measured frames. AMD RADV RENOIR
(PCI1002:164c), Ryzen 5 5500U, Mesa 26.1.6, Chrome 154.0.8037.57,
Wrela 0.9.0/compiler de9b81e52a7747870dfbd5223cb8aeb41e584d23.
All work was solo on Pop!_OS. One serial GPU queue; no concurrent image scoring, training or builds.
Clocks were not locked; per-run conditions are retained.

The final 24 fresh browsers run forward/reverse pairs for normal, source-edit
and camera-motion cases in both retained full-store BC and source-ID formats.
Controls use the same source geometry, fields, sampling, camera and rendering.
Full GPU spans include decoding, sorting, visibility, wood, updates, compositing
and intervening submission gaps. CPU calls and browser intervals are separate.

The edit changes all 12 source seeds at frame 300. The motion hook cuts up 96 m at
frame 300, returns at 360 and pans through a 60-frame loop at 420–479. The clearing
itself has no capsule/shape solids; a dedicated test changes a capsule source
version and radius. Do not infer general shape-scene speed from this scene.

Each range below is the two candidate-minus-original observations. Positive
differences are regressions. Startup covers 120 frames; other columns 60 frames.

| Scenario / format | Startup GPU Δ ms | Cut/edit GPU Δ ms | Pan GPU Δ ms | End PSS Δ MB | End VRAM Δ MB |
|---|---:|---:|---:|---:|---:|
| normal / bc | -762.12 to -685.21 | -4.77 to -4.18 | -1.65 to -0.25 | -2.54 to 1.46 | -22.02 to -13.57 |
| normal / id | -757.67 to -697.10 | -0.94 to 2.11 | -4.65 to -4.43 | -1.27 to 1.13 | -13.68 to -13.57 |
| edit / bc | -757.29 to -616.32 | -1.45 to 7.94 | -1.79 to 0.90 | 0.80 to 3.50 | -22.09 to -22.01 |
| edit / id | -735.69 to -562.73 | -3.98 to 1.82 | -0.69 to -0.56 | -2.50 to 8.56 | -13.16 to -13.10 |
| motion / bc | -725.43 to -673.07 | -735.61 to -723.95 | -2986.13 to -2954.36 | 12.08 to 18.64 | -13.21 to -13.20 |
| motion / id | -744.36 to -697.74 | -721.33 to -717.85 | -2966.94 to -2948.21 | 15.12 to 20.50 | -21.58 to -13.07 |

In the motion window, worst complete GPU frame falls from
122.51 to 124.06 ms to
25.29 to 25.69 ms.
Final settled GPU p95 across all candidate scenarios is
26.49 to 28.45 ms.
This does not establish a 60 fps budget or an improvement in all steady frames.

Memory observations include all requested GPU allocations, scratch, sorting,
wood, fallback resources and output. Whole-browser PSS includes CPU source copies
and driver/browser caches. **VRAM, GTT and PSS overlap on UMA; do not sum them.**
The runtime adds 294,912 requested bytes. Actual residency varies; any PSS increase
is charged, and no total physical-memory saving is claimed. In the earlier three-method motion pairs,
selected-column end PSS was 16.12–21.25 MB above the original and VRAM about
13.20 MB lower. The final pairs and primary sampled runs are retained separately;
residency differences should not be assumed stable or attributed to one resource.
Sample maxima can miss short peaks; counts and largest sampling gaps are retained.

| Primary run | Requested live B | Requested peak B | Sampled max VRAM MB | End VRAM MB | End PSS MB |
|---|---:|---:|---:|---:|---:|
| parent-id | 373,069,104 | 460,157,068 | 607.61 | 507.47 | 884.79 |
| parent-bc | 383,312,696 | 475,022,248 | 617.69 | 522.65 | 877.46 |
| parent-motion-id | 373,069,104 | 460,157,068 | 607.25 | 506.69 | 852.37 |
| parent-motion-bc | 383,312,696 | 475,022,248 | 617.63 | 522.16 | 871.41 |
| parent-update-id | 400,771,796 | 479,485,372 | 650.12 | 553.10 | 864.00 |
| parent-update-bc | 411,065,772 | 499,604,916 | 660.81 | 577.02 | 865.38 |
| specialized-id | 373,364,016 | 460,451,980 | 593.68 | 493.55 | 891.46 |
| specialized-bc | 383,607,608 | 475,317,160 | 587.24 | 500.57 | 887.30 |
| motion-specialized-columns-id | 373,364,016 | 459,672,268 | 582.00 | 485.09 | 879.80 |
| motion-specialized-bc | 383,607,608 | 475,317,160 | 595.63 | 508.96 | 887.76 |
| update-specialized-id | 401,066,708 | 479,780,284 | 608.26 | 531.02 | 883.27 |
| update-specialized-bc | 411,360,684 | 499,899,828 | 638.79 | 555.00 | 888.56 |

## Quality and proportional verification

All **24 stress and 12 native shaded comparisons pass** against the completed
references. Worst source-ID versus exact BC comparison: PSNR
-0.221775 dB, FLIP ratio
1.042732, LPIPS ratio
1.023648. The unchanged gates are −0.25 dB and 5%.
These inherited source-ID/BC differences are not attributed to this field change.

The single-executable, single-initialized-scene field oracle compares original
and selected kernels against the same frame's terrain/source inputs through
660 frames of startup, cuts, return and pan. **26,247,168 active-slab voxel
comparisons are bit-exact**, including kept voxels. Diagnostic extra volume and
counters are excluded from production timing/memory. Render size 320×180 reduces
unrelated oracle cost; the field remains 192×64×192.

All five distance GPU tests pass: sphere, shape tapes, probe hits, contact
shadows and the added capsule-only source-edit check. Deliberately removing
capsule contribution from the specialized kernel makes that new test fail;
the exact candidate is restored afterward. This qualifies the conditional fast
path without re-auditing unrelated rendering machinery.

Native leaf-class coverage precision is
99.99930272%, recall
100.00000000%; symmetric boundary p99 is
0.000000 px and maximum
2.828427 px. This class includes
impostor wood and excludes separately drawn near wood.

All eight focused/native temporal HDR outputs are bit-exact against the retained
source-ID controls. The separate coverage-reference color output differs in 47
float values, maximum 0.00018704, with exact alpha. Final browser screenshots
are exact in 4 of 12 pairs; the largest channel difference is 6/255 and at most one
pixel per comparison exceeds 2/255. Alpha is exact in every pair. These small
nonzero differences are preserved without an established causal attribution.
Full capture and screenshot deltas are in final-assessment.json. Reference metrics,
coverage details and convergence remain in specialized-id-native-assessment.json.
The focused/native capture batches each compile once and still initialize per
output; only the field oracle shares one initialized scene. No broad new
temporal-image suite or G3 claim is made.

## Decision and next bet

Retain this exact optimization: its large measured update/motion runtime benefit
justifies the small explicit scratch and observed process-memory tradeoff while
preserving field values and passing the shaded gates. This does not advance G4's
16× complete appearance-memory target by itself.

The next architecture-level bet is **specialize bounded field programs from
their actual source operators and stack requirements**, then share those exact
programs where distance, visibility and lighting need them. The current general
shape fallback still carries interpreter cost. Measure that class against an
equally informed conventional program, charge compile/cache/residency cost, and
require the same quality/runtime/memory decision before expanding it.

## Evidence and replay

Pop!_OS evidence:
 /home/ryanwible/worktrees/wrela-neural-frontier/build/neural-evidence/distance-column-factor-v1

Parent be8323043f4565c2018c1c728fb4539a2881812f.
Manifest SHA-256: 0010291fde89325591896ad3af8eeb2b8cf9303755c6e85ded9da69a913ca6ce.

The bundle contains the failed initial hypothesis and compiler attempt, both
specialized controls, raw timings, requested/resident memory, source archives,
executables, commands, scripts, frozen hashes, oracle and mutation-test failure.
[Distance implementation](../engine/distance.wrela),
[GPU tests](../engine/tests/distance/main.wrela),
[retained conventional appearance controls](neural-streamed-bake.md) and
[metric citations](citations.md) define the methods. No external implementation,
model weights, assets or hardware results were imported.
