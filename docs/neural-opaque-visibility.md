# Early opaque terrain visibility: retire the extra depth work

2026-10-30 UTC. **Retire all four experimental paths and retain the current
renderer.** Full ground shading before foliage raises complete GPU time
**3.30–3.37%**. A provisional ground-depth pass saves some foliage work but
does not establish a repeatable complete benefit. Restricting it to outer
terrain rings still makes settled rendering **0.80–1.06% slower** and movement
**0.07–0.40% slower**, with unchanged requested live storage.

The separate reduced pass has mixed full-route results **+0.30% / −1.20%**.
Its favorable run gains 412.03 ms during startup, then loses 31.20 ms afterward;
the other loses 41.10 ms after startup.

The final merged-pass follow-up lowers full-route totals only **0.157–0.199%**,
while movement is **0.012–0.120% slower** and settled rendering **0.915–1.693%
slower**. After startup its changes are +30.04 / −2.70 ms, not a consistent saving.
End VRAM rises **16.43–24.63 MB**, and end PSS rises **7.44 / 137.49 MB**.
These measurements remain in the record; the small whole-route reduction
does not justify the worse settled cost and residency. Startup is included,
not discarded to construct a preferred total.

First paired-image/runtime/residency assessment: **9.88 minutes**.
First completed current-parent shaded comparison: **38.21 minutes**.
Final merged-pass shaded comparison: **52.92 minutes**.
Decision: **52.93 minutes**, after **two** optimization
follow-ups. No third optimization, training or new reference generation.
G1–G7 and frozen models remain unchanged and unmet.

## What was tested

The [frame pipeline](../engine/frame.wrela) currently draws vegetation depth,
vegetation shading, then opaque ground. This can run foliage coverage and
lighting behind ground that later hides it. Both methods have the same source
fields, terrain triangles, material data, visibility, camera, wind, sampling,
geometry density, source updates and reconstruction. No method gets reference
radiance or future information.

- **ground-first:** move the existing fully shaded terrain draw before
  vegetation and move the target clears with their first writers. This pays
  ground shading even where foreground foliage later covers it; equal-depth
  ties can change precedence.
- **depth-first:** draw original ground triangles into existing IDs/depth first.
  Fragment depth is one representable f32 step farther than the raster depth,
  clamped to zero. The original later ground shading retains its strict
  Greater comparison. The provisional pass writes ID zero. It introduces
  neither a replacement mesh nor a new texture.
- **far-depth, follow-up 1:** omit the two innermost clipmap levels from that
  provisional draw through the existing indirect buffer. All original terrain
  shading and foliage remain. A subset of original opaque triangles gives a
  weaker certificate. No fitted coarse surface is substituted.
- **merged-depth, follow-up 2:** put exactly the same reduced ground draw inside
  the existing vegetation prepass. This removes the intermediate pass store/load
  boundary. All **190 generated GPU shaders are byte-identical as a multiset**
  to far-depth; geometry, buffers and sampling are unchanged. It tests pass
  overhead rather than a new numeric approximation.

The certificate updates every frame from the same current mesh and camera.
It does not cache stale visibility. The original source and all fallback work
stay resident. Clear/keep operations preserve the existing targets; no new
GPU allocation is requested by the algorithm.

The initial cost breakdown selected the first follow-up. Full early shading
raises the route's terrain subtotal from **2.44 ms to 5.15–5.21 ms per frame**.
Full provisional depth costs about **0.54–0.64 ms**, while saving only about
**0.15 ms** of settled foliage work. Selecting outer rings reduces that pass
to **0.503–0.505 ms** in the settled view, insufficient to repay it. This is
a rejected implementation of an exact-source visibility idea, not evidence
that all visibility reuse is unhelpful.

## Complete AMD route and comparison controls

Parent **d345ba1bdb3e35fa5a6606b8dd6394b1a102d733**, including the retained native field programs.
Solo Pop!_OS, Ryzen 5 5500U / AMD RADV RENOIR PCI 1002:164c, Mesa 26.1.6,
Chrome 154.0.8037.57, kernel 7.1.5-76070105, Wrela 0.9.0/compiler
de9b81e52a7747870dfbd5223cb8aeb41e584d23. No locked clocks or thermal claim.

Reuse the [established held-world route](neural-distance-capacity.md):
1920×1080, scale 0.667, one worker, cut at 240, return at 300,
96 m movement during 360–539, then settle. Background production jobs run.
Every 660-frame executable supplies seven captures at
299/359/419/479/539/599/659. Candidates are compiled before a quiet serial AMD
queue; scoring, compilation and toolchain queries do not overlap GPU runs.

**Sixteen production runs / 112 captures.** Match full source-census, camera
and player/instance records. These counts do not hash every numeric source
byte. Two primary full-depth comparisons have different early capsule arrivals;
their two cross-candidate comparisons are also excluded. All are preserved.

| Pair / variant vs parent | Source records match | Full GPU Δ | Movement Δ | Settled Δ | Max RGB /255 |
|---|---|---:|---:|---:|---:|
| 1 / ground-first | yes | +3.295% | +6.565% | +4.738% | 2 |
| 1 / depth-first | yes | +0.579% | +0.759% | +1.711% | 1 |
| 2 / ground-first | yes | +3.372% | +6.255% | +4.670% | 2 |
| 2 / depth-first | **no; excluded** | +0.719% | +0.922% | +1.242% | 24 |
| 3 / far-depth | yes | +0.299% | +0.403% | +0.805% | 1 |
| 3 / depth-first | **no; excluded** | -0.167% | +0.084% | +1.141% | 24 |
| 4 / far-depth | yes | -1.200% | +0.070% | +1.060% | 1 |
| 4 / depth-first | yes | -0.833% | +0.413% | +1.237% | 1 |
| 5 / merged-depth | yes | -0.199% | +0.120% | +1.693% | 1 |
| 6 / merged-depth | yes | -0.157% | +0.012% | +0.915% | 1 |

Every complete GPU span includes decoding, selection, visibility, sorting,
wood, source updates, lighting, compositing, startup and submission gaps.
CPU frame-call and browser timing are recorded separately. The merged-depth
movement CPU p95 is mixed: **142.89 → 145.73 ms** and **143.57 → 139.24 ms**.
Its route browser intervals change **+207.36 / −588.30 ms**, also mixed.
No CPU or presentation-latency advantage is claimed. Reference/diagnostic
captures are separate from production timing.

Full-depth repetitions remain unfavorable in settled/movement windows even
when a cold-start fluctuation improves a full-route total. The failed
ground-first path receives no additional shaded qualification because its
repeatable cost regression already excludes retention. The complete shaded
comparison below concerns the final merged-depth follow-up and actual current parent.
The preceding reduced-depth variant also passes all 36 current-parent checks;
its direct HDR comparisons are preserved separately. Both candidate qualification\nframe sources match the measured variant exactly, and their changed depth shader\nis byte-identical to the production-timed shader.

## Actual residency

Amounts are decimal MB. Requested values are live / peak. Physical values
are sampled maximum / final; **VRAM, GTT and PSS overlap on UMA and must not
be added**. The observer covers the browser process group and deduplicated
DRM clients, including scratch, caches, shader/driver state and fallback data.
Sampled maxima are not guaranteed physical peaks. Sampling asks for a 50 ms
sleep plus observation overhead; actual counts/gaps are in the records.

| Run | Requested live / peak | VRAM max / end | GTT max / end | PSS max / end |
|---|---:|---:|---:|---:|
| pair5-parent | 708.08 / 908.35 | 1076.16 / 913.25 | 22.50 / 22.46 | 4310.52 / 1089.84 |
| pair5-merged-depth | 708.08 / 895.20 | 1073.38 / 937.88 | 22.57 / 22.53 | 4257.65 / 1097.27 |
| pair6-parent | 708.08 / 897.11 | 1090.45 / 921.55 | 22.72 / 22.71 | 4292.04 / 1096.00 |
| pair6-merged-depth | 708.08 / 895.20 | 1064.99 / 937.98 | 22.75 / 22.71 | 4214.99 / 1233.49 |

The merged pass has **zero requested live-byte change** in both matched pairs.
End VRAM changes **+24.63 / +16.43 MB**, while end PSS changes
**+7.44 / +137.49 MB**. The large candidate PSS observation is retained.
Requested peaks also vary with update/resource lifetimes. There is no new
appearance-memory representation and no G4 compression claim.
All initial variants' memory records, including source-mismatched rows, remain
in the raw assessment rather than being relabeled memory savings.

## Shaded output and silhouette-adjacent errors

Completed shaded targets are reused unchanged: focused 192×108 forced-impostor
shot 3 and native 1920×1080 normal-LOD shot 0, 1×–4× reconstruction, whole/crown
regions and coarse/fine/independent-repeat controls. Viewing remains
exposure-1 Reinhard/sRGB, FLIP67pixels/degree and LPIPS AlexNet v0.1.

The inherited saved control captures predate retained engine changes. Once that
uncertainty was identified, the **current parent was recaptured against the same
fixed references**, with identical inputs/protocol. No new truth was generated.
The table compares candidate error with the fresh current-parent error, not with
the older saved renderer. Earlier comparisons remain as provenance in the raw files.

Unchanged necessary development tolerances: FLIP and LPIPS ratios ≤1.05
(floor1e−4), PSNR delta ≥−0.25dB. **36/36 pass.**

| Family | Passing | FLIP ratio | LPIPS ratio | PSNR Δ dB |
|---|---:|---:|---:|---:|
| focused | 24 / 24 | 1.000000–1.000000 | 1.000000–1.000000 | +0.000000 to +0.000000 |
| native | 12 / 12 | 1.000000–1.000000 | 1.000000–1.000000 | +0.000000 to +0.000000 |

Direct current-parent/candidate HDR comparison:

| Capture / scale | All RGBA exact | Max HDR RGB difference | Max displayed RGB difference | Pixels above2/255 |
|---|---|---:|---:|---:|
| focused / 1× | True | 0 | 0 | 0 |
| focused / 2× | True | 0 | 0 | 0 |
| focused / 3× | True | 0 | 0 | 0 |
| focused / 4× | True | 0 | 0 | 0 |
| native / 1× | True | 0 | 0 | 0 |
| native / 2× | True | 0 | 0 | 0 |
| native / 3× | True | 0 | 0 | 0 |
| native / 4× | True | 0 | 0 | 0 |

Reuse the fine reference's leaf-class coverage to select the existing four-pixel
boundary band. This measures shaded RGB near silhouettes; it is not candidate
segmentation, instance/strand precision-recall, or a full G3 motion gate.

| Scale | Boundary RGB MAE ratio | PSNR Δ dB | p99 RGB error, parent → candidate |
|---|---:|---:|---:|
| 1× | 1.000000 | +0.000000 | 0.124047 → 0.124047 |
| 2× | 1.000000 | +0.000000 | 0.246388 → 0.246388 |
| 3× | 1.000000 | +0.000000 | 0.288701 → 0.288701 |
| 4× | 1.000000 | +0.000000 | 0.303109 → 0.303109 |

Integrated native leaf-class coverage has precision **0.999993**,
recall **1.000000**, boundary p99 **0.000px**,
maximum **2.828px**, versus the completed fine reference.
It includes impostor wood and excludes separately drawn near wood. This existing
diagnostic and the production quadrature reference do not establish independent
transport truth or all G3 silhouette/reveal/trail requirements.

## Decision and next architecture-level bet

Retire early shaded ground, full provisional depth, reduced-ring depth and merged depth.
Preserve all results and restore runtime bytes to the parent. The two allowed
optimizations are complete. Quality passing cannot turn worse settled rendering,
zero allocation saving and higher observed final residency into a useful joint win.

Verification stays proportional: execute actual AMD variants, reuse completed
references, refresh the necessary current-parent controls, verify source
restoration and the report allowlist. No unchanged distance tests or broad GPU
suite are rerun. The excluded source-arrival runs and unfavorable physical-memory
observations are preserved.

Next bet: **source-shaped near-leaf coverage geometry**, selected by projected
footprint so extra vertices are paid only when they avoid enough alpha-filter
work. Preserve the source filter and a conservative fallback at uncertain edges.
This is a hypothesis, not an implemented saving. Earlier isotropic filters,
constant-region shortcuts, shared-card caches and regular distant-impostor
tiles already failed their joint decisions; their records must constrain the
new representation rather than be repeated unchanged.

Evidence on Pop!_OS: build/neural-evidence/opaque-visibility-v1.
Manifest SHA-256: e0bc6a2e0e4f70a686af794ed89faa3fb0b5dc315c86fd61e4d134beddd1e9bf.
Archived scripts and replay.md preserve parent/candidate builds, all commands,
source snapshots, measured output, references by hash, frozen models and failures.
Use a fresh evidence directory when replaying.

Provenance: original [terrain clipmap](../engine/terrain.wrela),
[frame pipeline](../engine/frame.wrela), pinned Wrela GPU semantics and
[benchmark charter](neural-rendering-goals.md). No third-party code, assets,
weights or reference algorithm was introduced.
