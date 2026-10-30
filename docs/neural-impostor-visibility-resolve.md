# Retire the production impostor visibility resolve

2026-10-30. **Decision: retire this visibility-record/fullscreen material resolve.**
It passes all 36 established shaded-reference checks, but gives no repeatable
complete rendering benefit. Total GPU changes are **−0.27% / +0.83%**; the
movement window is **1.17–1.47% slower**. Requested live memory rises
**14,745,600 B**, and final observed DRM VRAM rises **15,757,312 /
24,735,744 B**. The small shading saving does not justify the extra state.

The production runtime remains 3708b775560ce765976d7052023590193eb0a988.
Parent 88198c436284c871d9c590d0c077d2cec86b0078 includes later evidence-only commits.
No rendering improvement is attributed to the prototype, its infrastructure,
successful execution, or quality pass.

First paired assessment: **36.75 minutes**.
First complete quality/runtime/residency assessment: **46.50 minutes**.
Decision: **48.02 minutes**. **Zero optimization followups.**
The initial joint result is sufficient to reject this tradeoff; two is a limit,
not a quota. Solo Pop!_OS AMD, frozen models, zero training, unchanged G1–G7.

## What was compared

All three methods receive the same source instances, fields, geometry, material
representations, light, wind, LOD, visibility, sampling choices and reconstruction.
No candidate receives reference radiance or privileged learned inputs.

- **Parent:** retained exact-depth impostor prepass and geometric material shading.
- **Tagged control:** same shading, but far prepass split into a separate two-target
  pass that also captures a 16-byte visibility record per internal scene pixel.
- **Resolve:** same capture, with flat impostor material shading performed by one
  fullscreen triangle per visibility phase. Near cards, wood limbs, and volumes
  retain their original geometric shading.

The control separates the cost of recording visibility from eliminating the second
flat-impostor geometry pass. It is the retained practical conventional engine
baseline, strengthened by earlier measured experiments, not a claim to be the
globally optimal renderer.

The RGBA32Float record contains a signed winning instance ID (sign selects early
or late visibility phase), 16-bit source qx, 16-bit qy plus the two-bit selected view,
and the original 32-bit raster depth. Integer values are stored numerically and
exactly; CPU code asserts the instance range is below 2^24. No arbitrary NaN payload
is bitcast through the float texture. The selected view is captured from the
original coverage decision, preserving its LOD/sampling choice.

Resolve loads the original instance, reconstructs its material coordinates and
undeformed source position, and uses the original material/light functions. Wind
already affected raster coverage, interpolation and captured depth. Resolve writes
that captured depth with Equal testing, so nearer geometry continues to win.
Phase and ID-range guards reject other records. The early record pass clears old
records and the original near-card/grass IDs, which are zero; the late pass keeps
prior records. Both phases and all original material alternatives are implemented.

The existing [virtual geometry visibility resolve](../engine/virtual.wrela) is
the in-engine precedent; visibility buffers are established work, cited in
[citations.md](citations.md) (Burns and Hunt 2013; Schied and Dachsbacher 2015).
The experiment tests applying that architecture to production impostors, not
algorithmic novelty.

## Complete runtime and residency

Ryzen 5 5500U, AMD RADV RENOIR PCI 1002:164c, Mesa 26.1.6,
Chrome 154.0.8037.57, kernel 7.1.5-76070105, Wrela 0.9.0/compiler
de9b81e52a7747870dfbd5223cb8aeb41e584d23. No power/thermal lock.

One prebuilt executable per arm; quiet serial order parent/tagged/resolve then
resolve/tagged/parent. Each run is 660 frames at 1920×1080 output, 0.667 internal
scale, one worker, pacing enabled and two frames in flight, with seven periodic
captures. All six runs complete on AMD. Source census, controlled camera and
player streams match in all four comparisons; parent controls are shared, so
those comparisons are not four independent paired samples. No runs excluded.

The rendered World snapshot and frame time are controlled while background
simulation continues. Camera rises 96 m at frames 240–299, returns, moves 96 m during frames 360–539,
then settles. This is not ordinary gameplay or independent object motion.
All startup, source arrivals/updates, visibility, wood, decoding, sorting if any,
fields, light, reconstruction, compositing and GPU submission gaps are included.
Explicit authoring edits were not separately replayed for this retired prototype.
CPU/browser time is reported separately, not added to GPU time and not presented
as input-to-photon latency.

| Candidate, round | Total GPU change | Movement GPU change | Settled GPU p50/p95/p99, ms | Full-route browser elapsed change, ms |
| --- | ---: | ---: | ---: | ---: |
| tagged, 1 | +0.03% | +0.60% | 37.034/39.273/40.112 | +59.275 |
| tagged, 2 | +1.56% | +0.24% | 37.120/39.375/40.205 | +707.830 |
| resolve, 1 | -0.27% | +1.47% | 36.532/38.655/39.701 | +804.855 |
| resolve, 2 | +0.83% | +1.17% | 36.652/38.722/40.654 | +372.725 |

Parent settled GPU p50 is 36.586/36.703 ms, p95 is 39.027/39.312 ms,
and p99 is 39.759/40.297 ms. Resolve does not provide a consistent tail improvement.
Full-route browser elapsed is 372.7–804.9 ms longer for resolve. Raw JSON records
all windows, CPU quantiles, browser intervals and startup/update maxima.

In the settled window, parent vegetation prepass is 7.649–7.651 ms/frame and
vegetation shading 3.432–3.435 ms. Resolve's near prepass plus new source prepass
is 7.992–8.010 ms. Its remaining geometric shading plus source resolve is
3.040–3.044 ms. The roughly 0.4 ms shading saving is consumed by record capture.
The source resolve itself costs 2.295–2.296 ms/frame. No isolated label is called
a rendering win.

Both new arms add 14,745,600 requested live bytes: 16 bytes × 1280 × 720.
All resource allocations, scratch, capacity, caches and fallback data are counted.

| Memory observation, bytes | Parent range | Tagged range | Resolve range |
| --- | ---: | ---: | ---: |
| Requested live | 707,871,892–707,871,892 | 722,617,492–722,617,492 | 722,617,492–722,617,492 |
| Requested peak | 906,289,244–908,818,520 | 922,939,892–922,939,892 | 918,400,548–922,939,924 |
| Final DRM VRAM | 911,622,144–928,972,800 | 927,457,280–944,787,456 | 936,357,888–944,730,112 |
| Final DRM GTT | 22,937,600–23,035,904 | 22,708,224–22,708,224 | 22,544,384–22,790,144 |
| Final process-group PSS | 1,088,553,984–1,093,758,976 | 1,098,201,088–1,122,710,528 | 1,123,066,880–1,140,678,656 |
| Sampled maximum DRM VRAM | 1,075,175,424–1,091,907,584 | 1,099,288,576–1,107,759,104 | 1,099,304,960–1,116,651,520 |
| Sampled maximum DRM GTT | 22,978,560–23,068,672 | 22,740,992–22,740,992 | 22,577,152–22,831,104 |
| Sampled maximum process-group PSS | 4,205,017,088–4,241,494,016 | 4,270,592,000–4,358,776,832 | 4,247,884,800–4,261,064,704 |

Resolve's final process-group PSS rises 34,512,896–46,919,680 B.
DRM VRAM/GTT and PSS overlap on this UMA machine: never sum them.
Physical samples are collected at a requested 50 ms interval with actual collection
gaps recorded; sampled maxima are not true physical peaks. Tagged's mixed physical
VRAM deltas are not a memory saving when its requested live state increases.

## Shaded quality and silhouettes

Reuse the completed focused 192×108 forced-impostor references and the native
1920×1080 normal-LOD references. Three existing reference quadratures, four
temporal scales, and whole-image/crown-band scores provide 36 checks. No new
reference infrastructure or candidate-generated RGB truth.

**36/36 pass.** Worst FLIP ratio
**1.042761866**, worst LPIPS ratio **1.023593757**,
worst PSNR change **-0.221313dB**. Gates remain 1.05/1.05/−0.25 dB.
Exposure 1, Reinhard, display sRGB, AlexNet LPIPS v0.1 and LDR FLIP 67 pixels/degree,
without resizing. These are the established shaded controls, not independent
light-transport truth or the full held-out G3 suite.

Raw leaf-class coverage against the completed fine native control has precision
99.999303%, recall 100%, boundary p99=0 pixels and maximum 2.828427 pixels.
Union coverage MAE is 0.0000222785; 214 pixels differ by more than 2/255.
The complete coverage comparisons remain in the qualified JSON. The class
includes impostor wood and excludes near wood.
This is an existing silhouette diagnostic, not proof of all G3 motion gates.

Across the 14 paired route views per candidate, tagged differs by at most 1/255
in an RGB channel and resolve by at most 12/255. Quantized-coordinate reconstruction
is not bit exact. Full image differences and the separate reference scores remain
available, including the earlier retained depth change's differences from the
older references.

## Failures, preservation and next bet

Before GPU execution, prototype builds exposed generated-method placement,
argument binding and view-selection edit errors, the eight-storage-buffer and
sixteen-texture stage limits, and an unsupported RGBA32Uint format in the pinned
standard library. All failed sources/builds/logs are preserved. A preliminary
MRT compile probe rejected drawing a one-output shader into a two-target pass;
it never ran on GPU. A failed repair-script attempt is preserved too.
No failed executable was measured. Successful builders and reference qualification
restore every temporary source edit.

The record layout and depth-test approach were revised before the initial queue.
They are implementation repairs, not post-measurement optimization experiments.
No followups, retraining or reference expansion were performed after the result.

**Next architecture-level hypothesis:** source-aware temporal visibility reuse
with explicit invalidation for source edits, wind and camera/disocclusion changes.
The larger opportunity here is the roughly 7.6 ms prepass. Any test must give the
conventional control identical identity/motion/invalidation knowledge, charge
history and validation/fallback work, and measure first-reveal and silhouette
errors. A low-reuse scene must expose the overhead. This remains unimplemented;
there is no claimed saving or G1–G7 completion.

Evidence: build/neural-evidence/impostor-visibility-resolve-v1 on Pop!_OS.
Manifest SHA-256: 645ad34cb647655884c503f0495055a02768719dd4b181b806b9697e3d677436. Exact source archives, builds, six raw runs,
42 route PNGs, references/dependency hashes, scores, all memory observations,
frozen artifacts and replay scripts are preserved. Runtime restoration is exact.
