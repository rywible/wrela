# Source-shaped near-card partitions: retire the extra geometry

2026-10-30 UTC. **Retire this two-rectangle representation; keep the current
one-rectangle renderer.** Its extra geometry and vertex work do not establish
a repeatable complete advantage. Requested live storage rises **1,707,008 B**,
including the retained original geometry fallback. The first implementation
fell back on every production pattern; that failure is preserved and corrected
before interpreting the representation.

The corrected candidate's first pairs are **−2.731% / +0.666%** complete GPU.
The favorable pair also has lower costs in unaffected fog, temporal and terrain
passes. It is retained, not excluded or credited entirely to the geometry.
Two further balanced pairs use the same binaries to resolve this inconsistency:
**pair5 GPU+0.524%, movement+0.923%, settled+0.589%; pair6 GPU+1.600%, movement+0.764%, settled+0.414%**. No additional optimization is hidden in these repetitions.

Initial joint route assessment: **22.87 min**.
Initial completed shaded assessment: **32.56 min**,
but subsequently identified as fallback-only.
Corrected representation's first completed shaded comparison:
**51.25 min**. Decision:
**58.74 min**, after **one** optimization follow-up.
No training or new truth images. [G1–G7](neural-rendering-goals.md) stay unchanged
and unmet; frozen weights are hash-checked and archived.

## Representation, fairness and failure diagnosis

The production [card outline](../engine/plant.wrela) is a four-vertex oriented
rectangle, chosen from16 orientations around nonzero atlas texels and their
bilinear support. The candidate splits its longer axis at the midpoint and
bounds support in each half. It retains the original coverage filter, leaf IDs,
shading, wind, visibility, source updates, sorting, wood and compositing.

Only nearest card LOD uses the new path. Cards narrower than64 render pixels
use the original quad plus a degenerate second quad. The extra invocations still
cost; the gate does not remove their submission. Other LODs and shadows keep the
original geometry. Runtime expands each box by two projected pixels plus a small
numeric margin and clips it to the original outline.

The methods receive the same useful source data. Nothing learns from truth
captures or receives privileged geometry. Both original index data and the
original source atlas remain resident. The candidate adds512 B of bounds and
1,706,496 B of indices. Startup cooking and all temporary/driver allocation are
charged, including the duplicated per-output scan imposed by Slots ownership.

**Initial failure:** any occupied atlas-edge texel caused a full-rectangle
fallback. An actual AMD readback of the production six-leaf atlas found all16
patterns took this path. Thus the initial36 passing shaded checks demonstrate
only fallback compatibility, not trimmed geometry quality. A diagnosis was
necessary to interpret the result; it did not expand the reference system.

**Follow-up1, clamped:** intersect the original quad with each edge texel's
bilinear support and the sampler's unbounded clamped edge strip. Interior
texels retain their conservative projected bounds. Four half-plane clips keep
the source support inside the original quad. This removes the blanket fallback
while retaining the same64px gate, filter and two-pixel padding.

Read back the actual cooked bounds for all16 production patterns. The table
shows aggregate geometric area with equal pattern representation. Submitted
area counts the overlap twice; union area counts it once. These are neither
hardware fragment counts nor a weighted visible-scene estimate.

| Card width px | Submitted / original area | Union / original area | Patterns with less submitted area | Patterns with less union area |
|---|---:|---:|---:|---:|
| 64 | 1.062851 | 0.995046 | 0 / 16 | 4 / 16 |
| 128 | 1.019638 | 0.986363 | 2 / 16 | 9 / 16 |
| 256 | 0.995822 | 0.979420 | 8 / 16 | 11 / 16 |
| infinite | 0.969033 | 0.969033 | 16 / 16 | 16 / 16 |

At64px every pattern submits more area than the original quad. Even removing
all pixel padding leaves little possible saving. This is a poor trade for
doubling near-card vertices/triangles. The [LOD policy](../engine/lod.wrela)
normally targets about12px cards, so the64px gate is mainly a close-up path.
No claim is made that every scored view exercises trimmed close-up geometry.

## Complete AMD rendering and resident memory

Parent 0d6fb49ce1131f5556c51080e3e70f180f408c06. Ryzen5 5500U / AMD RADV RENOIR PCI1002:164c,
Mesa26.1.6, Chrome154.0.8037.57, kernel7.1.5-76070105,
Wrela0.9.0/compiler de9b81e52a7747870dfbd5223cb8aeb41e584d23.
Serial GPU execution on Pop!_OS; no locked-clock or thermal-stability claim.

The established held-world Last Green route is1920×1080 at render scale0.667:
cut at240, return300,96m movement360–539, settle600–659. Each660-frame
executable supplies seven images at299/359/419/479/539/599/659.
**12 runs / 84 captures**. Candidates are built before the
serial GPU queue. Builds, scoring and toolchain queries do not overlap captures.
Source-census, camera and player records are matched for comparisons; they do
not constitute a hash of every numeric source byte.

| Pair / method | Source records match | Full GPU Δ | Movement Δ | Settled Δ | Browser interval Δ ms | Max RGB /255 |
|---|---|---:|---:|---:|---:|---:|
| 1 / split | True | +1.730% | +1.380% | -0.609% | +517.10 | 1 |
| 2 / split | True | -0.173% | +0.706% | +0.239% | +31.17 | 0 |
| 3 / clamped | True | -2.731% | -3.887% | -2.345% | -678.63 | 1 |
| 4 / clamped | True | +0.666% | +1.652% | -0.167% | +145.44 | 1 |
| 5 / clamped | True | +0.524% | +0.923% | +0.589% | +24.50 | 1 |
| 6 / clamped | True | +1.600% | +0.764% | +0.414% | +433.14 | 1 |

Full GPU spans include startup, decoding, sorting, selection, visibility, wood,
updates, lighting and compositing. Labels are diagnostic; their averages are
not summed as a substitute for the whole. CPU calls and browser intervals are
recorded separately in the assessment; no input-to-photon claim.

All figures below are decimal MB. Requested live/peak comes from the existing
allocation observer; residency comes from the whole Chrome process group via
DRM fdinfo and PSS, sampled throughout startup and the route. Caches, scratch,
fallback and driver allocations are included. **UMA pools overlap: do not add
VRAM, GTT and PSS.** Sampled maxima can miss short physical peaks.

| Run | Requested live / peak | VRAM max / end | GTT max / end | PSS max / end |
|---|---:|---:|---:|---:|
| pair1-parent | 708.08 / 895.20 | 1064.99 / 938.40 | 31.25 / 22.94 | 4273.56 / 1088.18 |
| pair1-split | 709.79 / 850.00 | 1010.74 / 947.14 | 22.91 / 22.87 | 4248.81 / 1247.86 |
| pair2-parent | 708.08 / 892.57 | 1073.32 / 921.73 | 22.98 / 22.94 | 4296.19 / 1230.17 |
| pair2-split | 709.79 / 898.82 | 1091.96 / 947.10 | 22.81 / 22.79 | 4279.09 / 1110.22 |
| pair3-parent | 699.11 / 902.34 | 1081.34 / 906.35 | 22.74 / 22.71 | 4258.24 / 1085.98 |
| pair3-clamped | 700.81 / 905.95 | 1100.00 / 907.18 | 22.83 / 22.79 | 4260.61 / 1115.98 |
| pair4-parent | 708.08 / 906.44 | 1068.19 / 904.86 | 22.81 / 22.79 | 4283.71 / 1087.52 |
| pair4-clamped | 709.79 / 916.28 | 1076.30 / 904.62 | 22.82 / 22.79 | 4223.94 / 1241.18 |
| pair5-parent | 708.08 / 895.20 | 1056.53 / 921.62 | 22.59 / 22.54 | 4278.83 / 1090.96 |
| pair5-clamped | 709.79 / 896.91 | 1064.99 / 947.12 | 22.99 / 22.95 | 4242.49 / 1237.53 |
| pair6-parent | 708.08 / 903.81 | 1076.17 / 904.44 | 22.60 / 22.54 | 4220.49 / 1224.94 |
| pair6-clamped | 709.79 / 908.15 | 1084.67 / 921.74 | 22.83 / 22.79 | 4250.53 / 1106.47 |

No physical-memory saving is established. All unfavorable and variable
residency observations remain in the record.

## Shaded image quality and silhouettes

Reuse completed focused192×108 forced-impostor and native1920×1080 normal-LOD
references at1×–4×. Current-parent controls come from the preceding
[opaque visibility block](neural-opaque-visibility.md); runtime source is
unchanged between those controls and this parent. Their current-parent metric
fields, rather than the legacy control fields, are the comparator.

Exposure1 Reinhard/sRGB, FLIP67pixels/degree, LPIPS AlexNetv0.1. Necessary
development tolerances remain FLIP/LPIPS≤1.05 with1e−4 floor and PSNR≥−0.25dB.
The corrected candidate passes **36/36**.

| Family | Passing | FLIP ratio | LPIPS ratio | PSNR Δ dB |
|---|---:|---:|---:|---:|
| focused | 24 / 24 | 1.000000–1.000000 | 1.000000–1.000000 | +0.000000 to +0.000000 |
| native | 12 / 12 | 1.000000–1.000000 | 1.000000–1.000000 | +0.000000 to +0.000000 |

**8/8** paired HDR outputs have equal RGBA float values.
The focused forced-impostor control does not exercise near-card geometry.
Unchanged output in normal views cannot prove the64px close-up gate was active.
Do not promote these checks to general close-up or G3 qualification.

Use the existing fine source-coverage mask's four-pixel boundary band for shaded
RGB error. This is not branch/strand/instance segmentation or motion truth.

| Scale | Boundary RGB MAE ratio | Boundary PSNR Δ dB | Candidate p99 RGB error |
|---|---:|---:|---:|
| 1× | 1.000000 | +0.000000 | 0.124047 |
| 2× | 1.000000 | +0.000000 | 0.246388 |
| 3× | 1.000000 | +0.000000 | 0.288701 |
| 4× | 1.000000 | +0.000000 | 0.303109 |

Integrated candidate coverage is a diagnostic only, never new RGB truth. Its
metrics and reference convergence are archived in the qualified assessment.
No additional close-up reference system is built: the complete cost and source
area results already reject retention on this workload.

## Decision, verification and next bet

Retire both variants. One follow-up was enough to correct the blanket fallback
and expose the weak area trade. No second optimization is justified by these
measurements. Restore runtime bytes to the parent, preserve all failures and
keep the existing conventional wins.

Verification is limited to real AMD executions, existing shaded controls,
the necessary cooked-area diagnostic, source restoration and the report
allowlist. Preserve the compiler rejections, rejected diagnostic host arguments,
and two rejected duplicate launches. A user-created status fork had continued
the serial qualification and confirmation queue. Original-chat duplicate attempts
stopped at directory checks before compilation/GPU work. The fork stopped writes
before this chat sealed the record. Archived capture sources are checked against
the intended variant; all full measurement results remain intact.

Next architecture-level bet: **derive a lifetime plan from Wrela's explicit
frame pipeline and reuse compatible transient attachments after their last
read**. Keep sampling, arithmetic and pass order unchanged; measure whether
the reduced allocation survives driver residency and whole-frame cost. The
previous tiled temporal experiment reduced storage but added compute cost;
lifetime reuse asks whether storage can be removed without that extra work.
This is an untested hypothesis, not a rendering win.

Evidence: build/neural-evidence/card-partitions-v1 on Pop!_OS.
Manifest SHA-256: 2127e6aa5428e3b566a15f40aba719d6003ebebf6b4f71261dba1918144d6bd5. Archived scripts, source, commands, outputs,
current controls, frozen weights and reference hashes provide replay provenance.

Sources: the engine's [plant atlas](../engine/plant.wrela),
[card rendering](../engine/vegetation.wrela), [wood pipeline](../engine/wood.wrela),
[frame targets](../engine/frame.wrela), [toolchain measurement semantics](toolchain.md)
and [benchmark charter](neural-rendering-goals.md). No external code or model.
