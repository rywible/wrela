# Direct source demand cache: lower startup peak, retire the current cache

2026-10-30 UTC. **Retire the fixed-slot synchronous demand cache.** The executable
AMD path passes the established source-ID shaded gates and reduces cold/edit
GPU-memory peaks, but barely reduces steady source-ID storage, raises browser
PSS, and adds work when the camera requests new views. Runtime is restored to
e5a56797. G1–G7 and all frozen models are unchanged and unmet.

The first joint assessment completed in **40.91 minutes**.
There were exactly two targeted followups, zero training cycles, and no new
reference-generation scheme. The final decision was at 91.20
minutes. This is a rendering experiment with preserved negative results, not
a claim that infrastructure completion improves rendering.

## Executed comparison

Same 12-kind clearing, 2,485 instances, 1920×1080 output and normal internal 2×
rendering, one worker, 600 warmup +60 measured frames. AMD RADV RENOIR
(PCI1002:164c), Ryzen 5 5500U, Mesa 26.1.6, Chrome 154.0.8037.57,
Wrela 0.9.0/compiler de9b81e52a7747870dfbd5223cb8aeb41e584d23.
All work was solo on Pop!_OS; one quiet GPU queue, no concurrent builds or scoring.
Thermal/power clocks were not locked. Fresh repeat runs record conditions.

The CPU uses the renderer's source instances, frustum, LOD and view functions
to request views before visibility and drawing. It conservatively includes
volume bounds and ignores HZB rejection. Both conventional BC and source-ID
cache variants receive identical useful source information and requests.
The controls are the retained full exact BC-page and source-ID renderers from
[streamed construction](neural-streamed-bake.md), with the same source-edit and
camera hooks. Neither control is denied Wrela's source information.

One cache holds 1,024 physical view slots. New views are rasterized from source
geometry in batches of at most 16 per kind, encoded and made available in the
same frame. There is no full appearance fallback. BC uses the existing exact
encoders and known-destination texture copies; source IDs use the existing
palette codec and field/material reconstruction. Source-ID pixels are verified
after encoding; capacity/encoding overflow fails explicitly.

This is bounded experimental coverage. A larger demanded set would assert.
No claim of general production readiness or an unrestricted cache guarantee.

## First failure and the two followups

The initial 256² scratch remapped original atlas coordinates. Three of 24 stress
checks failed the unchanged 0.25 dB PSNR-loss gate (worst 0.277290 dB); all 12 native
checks passed. The initial complete quality/runtime/memory assessment and
source are preserved.

1. **Preserve the original 1024² raster coordinates.** Both formats render only
   requested views at their original locations. All24 stress checks and 12
   native checks then pass. The larger scratch cost is counted.
2. **Release scratch after 120 idle frames.** Misses allocate it again, so both
   cuts and source changes pay the real allocation/rebuild cost. Rendering and
   codec math are unchanged. Final settled screenshots versus followup1 are
   recorded in final-assessment.json; no third optimization was attempted.

The first control compilation exposed an experimental module left beside the
restored control sources. It was preserved as a failed compile, then excluded
from the control build and restored afterward. No failed compile is counted
as a renderer result.

## Shaded quality and silhouettes

Reuse completed references: 24 stress comparisons across scales/ROIs/reference
convergence variants, plus 12 production native comparisons. Worst source-ID
comparison is PSNR -0.221716dB relative to BC, FLIP ratio
1.042725, LPIPS ratio 1.023648. All pass the
unchanged -0.25dB / 5% gates.

Native leaf-class coverage precision 99.99930272%, recall
100.00000000%; symmetric boundary p99 0.000000px,
maximum 2.828427px. Leaf-class includes impostor wood but
excludes separately drawn near wood. Static checks do not establish G3.

A separate **single executable, single initialized scene** checked 15 camera
poses, including elevations and forced-impostor stress, on each first visible
frame. GPU-selected early/late impostor and volume requests had zero missing
routes. Peak CPU demand 824 slots;
cumulative misses 1405. This checks
admission across changes/eviction, not temporal image quality. Ordinary motion
and source-edit runs also complete with source-ID verification errors 0.

BC cache screenshots are compared with BC controls, but no broad new BC cache
reference qualification is claimed. Because the complete tradeoff is rejected,
the experiment does not expand reference infrastructure or train a model.

## Complete memory

All GPU allocations are observed, including sorting, visibility, wood, volumes,
scratch, cache routing, unused allocations and output. CPU source copies and
browser/driver caches remain in whole-process PSS. The table's bytes are
requested allocations; VRAM/PSS are actual observations. UMA pools overlap:
**do not sum VRAM, GTT and PSS**. Sample maxima can miss short-lived peaks;
sample counts and largest sampling gaps are retained. These are not per-asset
physical-memory attributions.

| Method | Requested live B | Requested peak B | Sampled VRAM maximum MB | End VRAM MB | End PSS MB |
|---|---:|---:|---:|---:|---:|
| parent-bc | 383,312,696 | 475,022,248 | 617.63 | 522.58 | 876.52 |
| idle-bc | 380,179,324 | 430,064,776 | 559.33 | 518.35 | 926.56 |
| parent-id | 373,069,104 | 460,157,068 | 607.25 | 507.11 | 879.08 |
| idle-id | 372,839,340 | 414,262,500 | 539.43 | 506.90 | 893.05 |
| parent-update-bc | 411,065,772 | 499,604,916 | 660.82 | 577.02 | 865.25 |
| update-bc | 407,932,400 | 430,278,984 | 602.13 | 560.16 | 902.83 |
| parent-update-id | 400,771,796 | 479,485,372 | 650.05 | 553.03 | 859.35 |
| update-id | 400,592,416 | 414,476,712 | 582.16 | 552.84 | 896.72 |

At unchanged-source steady state the ID cache removes only 229,764 requested bytes versus the
full ID store; BC removes 3,133,372 B. The cache therefore does not turn the
existing source-ID representation into a G4-scale memory reduction. Cold and
source-edit GPU peaks improve, but steady process memory must also count.

## Complete rendering cost

The full GPU timestamp span includes decoding, sorting, visibility, wood,
updates and compositing, including submission gaps within that span. CPU calls,
browser frame intervals and startup/update windows are reported separately.
Two fresh browsers per method/scenario run in forward then reverse order:
24 runs total. No concurrent CPU scoring or GPU experiments.

Each entry below is the range of the two paired candidate-minus-control
observations. Positive cost/memory differences are regressions. Cut/edit and pan
columns are 60-frame windows; on unchanged-camera runs those are ordinary
rendering windows. Startup is 120 frames. Full frame samples, labels, CPU maxima,
browser intervals and images are in final-assessment.json.

| Candidate | Startup GPU Δ ms | Cut/edit GPU Δ ms | Pan-window GPU Δ ms | End PSS Δ MB | End VRAM Δ MB |
|---|---:|---:|---:|---:|---:|
| idle-bc | 431.52 to 467.31 | -1.09 to 1.11 | -7.36 to -5.18 | 30.33 to 38.78 | 4.10 to 4.16 |
| idle-id | 402.54 to 434.79 | -1.74 to 2.54 | -2.46 to 2.20 | 22.60 to 24.82 | -0.33 to -0.32 |
| update-bc | 434.47 to 435.32 | -55.25 to -47.63 | -4.94 to -2.84 | 25.74 to 28.89 | -16.88 to -16.87 |
| update-id | 372.16 to 383.73 | -70.84 to -61.56 | -1.05 to 2.88 | 23.62 to 25.88 | -0.19 to -0.19 |
| motion-bc | 364.06 to 398.76 | 4.11 to 36.58 | 52.71 to 116.25 | 31.36 to 34.23 | 4.19 to 4.26 |
| motion-id | 332.77 to 384.98 | 26.10 to 41.38 | 59.86 to 77.22 | 23.65 to 113.40 | -0.27 to -0.26 |

Two outliers are also preserved: the reverse-order BC return window adds
193.73 ms of complete GPU span, and one ID motion run ends 113.40 MB higher in
PSS (the other 23.65 MB). These have no established causal attribution and are
not discarded. The returned-camera window itself causes no new cache misses
in these logs. The static-camera scratch-lifetime comparison is byte-exact
for BC; ID differs by one byte-level step in one RGB channel of one pixel,
with exact alpha.

Fresh settled complete-frame timings:

| Run | Full GPU p50 / p95 / p99 ms | CPU p95 ms |
|---|---:|---:|
| repeat1-parent-bc | 25.076 / 26.527 / 27.535 | 2.014 |
| repeat1-idle-bc | 24.983 / 26.565 / 27.621 | 1.902 |
| repeat1-parent-id | 25.132 / 26.606 / 27.600 | 2.076 |
| repeat1-idle-id | 25.125 / 26.651 / 27.711 | 1.849 |
| repeat2-idle-id | 25.150 / 26.605 / 27.641 | 2.038 |
| repeat2-parent-id | 25.153 / 26.693 / 27.703 | 1.947 |
| repeat2-idle-bc | 25.053 / 26.670 / 27.622 | 1.897 |
| repeat2-parent-bc | 25.045 / 26.594 / 27.679 | 1.967 |
| repeat1-parent-update-bc | 26.169 / 28.264 / 29.059 | 1.962 |
| repeat1-update-bc | 26.048 / 28.096 / 29.177 | 1.955 |
| repeat1-parent-update-id | 26.170 / 28.223 / 29.361 | 1.970 |
| repeat1-update-id | 26.210 / 27.990 / 29.560 | 1.927 |
| repeat2-update-id | 26.246 / 28.262 / 29.331 | 2.048 |
| repeat2-parent-update-id | 26.251 / 28.245 / 29.448 | 2.031 |
| repeat2-update-bc | 26.077 / 28.556 / 29.076 | 1.977 |
| repeat2-parent-update-bc | 26.099 / 28.042 / 29.104 | 1.981 |
| repeat1-motion-parent-bc | 26.323 / 28.119 / 28.755 | 2.057 |
| repeat1-motion-bc | 26.344 / 28.743 / 29.468 | 2.086 |
| repeat1-motion-parent-id | 26.374 / 28.578 / 28.920 | 2.108 |
| repeat1-motion-id | 26.405 / 28.330 / 29.001 | 1.931 |
| repeat2-motion-id | 26.470 / 28.333 / 29.191 | 1.841 |
| repeat2-motion-parent-id | 26.443 / 28.540 / 28.811 | 2.016 |
| repeat2-motion-bc | 26.425 / 28.351 / 28.891 | 2.089 |
| repeat2-motion-parent-bc | 26.361 / 28.428 / 29.151 | 2.001 |

The source-edit hook replaces all 12 seeds at frame300. The camera hook cuts
up 96 m at frame300, returns at 360, then traverses a 60-frame loop at 420–479.
Source, movement, lighting and downstream rendering are matched in controls.
Motion cost is not inferred from a static cache-hit benchmark.

## Decision and next architecture-level bet

The startup-memory benefit is real within the sampled/requested evidence.
It does not justify this cache's negligible steady ID saving, added browser
residency and recurring new-view work. **Retire this implementation**, preserve
its direct-construction evidence and keep the previous full-store renderers.
Do not harden an architecture after its complete tradeoff has failed.

A larger conventional target emerged from the same experiment: the unchanged
distance-field update kernels consume **2.96–2.97 seconds of 4.26 seconds** of
complete GPU spans across the conventional 60-frame pan, about 69.5–69.7%.
Wrela already reuses a completed toroidal volume, but
[Distance.follow](../engine/distance.wrela) discards the kept region and
restarts all 8 slabs if the camera moves while an earlier cook remains unfinished.

The next bet is **explicit validity for field regions across interrupted
updates**: retain completed world/source-valid regions, evaluate only newly
exposed or invalid regions, and charge metadata, source changes and visibility.
Source code and field bounds can inform this exact conventional scheduling.
The label measurement identifies a consequential target; it does not prove
that all distance cost is redundant or that the proposed change is correct.
The next experiment must demonstrate the quality–memory–runtime result.

## Evidence and replay

Pop!_OS evidence:
 /home/ryanwible/worktrees/wrela-neural-frontier/build/neural-evidence/source-demand-runtime-v1

Parent e5a56797c582fb231e95da7d4893fa4d6417b07a.
Manifest SHA-256: 927c22caee11d50c49733ab735f29fb9fadbfab76cb9e39ae2a03dbcc7a079b9.

The bundle preserves initial failures, both followups, retired Wrela sources,
compiled executables, commands, raw captures, scores, complete timings,
allocation/residency samples, scripts, frozen hashes and replay instructions.
Prior [source-ID evidence](neural-source-identity-atlas.md),
[exact BC control](neural-sparse-appearance.md),
[previous demand census](neural-view-demand-volumes.md), and
[metric citations](citations.md) define inherited methods and completed truth.
No external implementation, weights or training assets were imported.
