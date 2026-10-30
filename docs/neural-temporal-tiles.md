# Fused temporal tiles: retire after one follow-up

2026-10-30 UTC. **Retire this fused tiled resolver; keep the current production
resolver.** The initial candidate raises complete GPU cost **7.20–8.33%**.
A shorter half-float conversion recovers part of the loss, but still raises
complete cost **4.01–4.67%** and settled cost **5.16–5.69%**. It removes
**14,745,584 requested live bytes**, about 2.08% of the whole renderer.
The follow-up's end VRAM falls 15.77–16.03 MB in two repeats; process memory
moves in both directions. That tradeoff and the shaded-quality results do not
justify retaining the architecture.

First paired-image/runtime/residency assessment: **20.11 minutes**.
Completed-reference and boundary assessment: **40.69 minutes**.
Decision: **41.24 minutes** after one optimization follow-up.
No second optimization, training or new reference family. Models and G1–G7 stay
unchanged and unmet. All experimental runtime changes are restored; only this
record and its benchmark entry are retained.

## Question and implementation

Could knowledge of the exact sampling footprint eliminate global statistics
textures and their bandwidth? The existing [Shared resolver](../engine/frame.wrela)
first writes scene-sized means and spreads, then resolves output pixels.
The candidate cooperatively loads an input footprint for each 8×8 output tile,
calculates the same 3×3 moments in workgroup storage, and calls the existing
[found/blended operators](../engine/temporal.wrela).

An 11×11 color cache and two 9×9 statistics caches occupy **4,528 workgroup bytes**.
All padding lanes reach barriers; bounds checks happen afterward. Statistics
are explicitly rounded to half floats to preserve the intended intermediate
precision. Storage-capable output histories are required. The original resolver
remains as a fallback for input dimensions larger than output, including its
small initial textures; all fallback allocations count.

Both methods get identical scene color, depth, motion, history, camera and
source information. The source-bounded zero-motion path remains in the strongest
control. Last Green has unbounded motion and uses its general path. The candidate
also uses the general path. No truth, future frame or privileged learned input
is supplied.

The pinned toolchain expands Half2 encode/decode into software bit operations.
The sole follow-up replaces common finite round trips with direct mantissa
rounding and a subnormal path, retaining the general conversion for large and
nonfinite values. It intends the same round-to-nearest, ties-to-even values.
Generated shader size actually grows **92,086 → 132,482 bytes**, despite the
faster common execution path. Identical mismatch counts in the temporal fixture
are not proof of universal numeric equivalence.

## Complete runtime and batched execution

Parent **54bdc66264c0888a907c1312e7c121669f6de1b3**, including retained direct field specialization.
Solo Pop!_OS AMD RADV RENOIR PCI 1002:164c / Ryzen 5 5500U, Mesa 26.1.6,
Chrome 154.0.8037.57, kernel 7.1.5-76070105, Wrela 0.9.0/compiler
de9b81e52a7747870dfbd5223cb8aeb41e584d23. Clocks are not locked; run conditions
are recorded.

Reuse the [established held-world route](neural-distance-capacity.md):
1920×1080, scale 0.667, one worker, vertical cut/return, 96 m movement,
then settle. Every 660-frame executable run produces seven captures
at 299/359/419/479/539/599/659. Candidates are built ahead of a quiet serial
GPU queue; builds, scoring and toolchain queries do not overlap measurements.

Ten production runs produce **70 captures**. All compared camera, source-census
and player/instance records match; none is excluded. Counts do not hash every
source byte. Whole GPU spans include updates, decoding, visibility, sorting,
wood and compositing, including startup spikes. CPU calls, browser intervals
and loading records remain in the bundle. Diagnostic/reference work is outside
production timing.

| Pair / candidate vs parent | Complete GPU Δ | Movement Δ | Settled Δ | Browser route Δ s | Max paired RGB /255 |
|---|---:|---:|---:|---:|---:|
| 1 / stored | +0.36% | -0.99% | -1.06% | -0.150 | 1 |
| 1 / tiled | +8.33% | +9.56% | +8.38% | +2.317 | 22 |
| 2 / stored | -0.22% | -0.41% | -0.34% | -0.251 | 1 |
| 2 / tiled | +7.20% | +9.94% | +8.93% | +1.805 | 22 |
| 3 / tiled-half | +4.01% | +5.72% | +5.16% | +1.492 | 22 |
| 4 / tiled-half | +4.67% | +6.23% | +5.69% | +1.217 | 22 |

The storage-only control is essentially flat in complete GPU time.
Initial tiled temporal work is **9.81–9.82 ms per settled frame**, versus
**6.32–6.55 ms** for the original. The follow-up lowers it to **8.55 ms**,
still above its matched controls' **6.32–6.39 ms**.
Follow-up settled complete GPU p50 is **38.51–38.54 ms**, controls
**36.49–36.50 ms**. Movement CPU p95 is mixed:
146.05 → 147.54 ms and 142.58 → 140.05 ms.
There is no CPU latency improvement claim.

## Resident memory, including scratch and fallbacks

Amounts are decimal MB. Requested columns show live / peak; physical columns
show sampled maximum / final. **Do not add VRAM, GTT and PSS on this UMA device.**
The observer samples the browser process group and DRM allocations, with a
requested 50 ms sleep plus read overhead. Sample gaps/counts are retained.
Sampled maxima are not guaranteed physical peaks. Runtime shaders, caches,
fallbacks, outputs and scratch remain charged.

| Run | Requested live / peak | VRAM max / end | GTT max / end | PSS max / end |
|---|---:|---:|---:|---:|
| pair1-parent | 708.08 / 903.34 | 1056.61 / 896.02 | 22.58 / 22.54 | 4263.69 / 1097.10 |
| pair1-stored | 708.08 / 908.35 | 1084.41 / 921.50 | 22.80 / 22.77 | 4264.91 / 1156.75 |
| pair1-tiled | 693.34 / 881.92 | 1057.26 / 922.35 | 23.06 / 23.04 | 4281.84 / 1093.71 |
| pair2-parent | 708.08 / 906.44 | 1059.45 / 887.55 | 22.90 / 22.86 | 4276.96 / 1090.61 |
| pair2-stored | 708.08 / 908.35 | 1076.09 / 921.43 | 22.86 / 22.84 | 4254.96 / 1093.13 |
| pair2-tiled | 693.34 / 899.83 | 1060.13 / 905.71 | 22.68 / 22.63 | 4263.70 / 1093.06 |
| pair3-parent | 708.08 / 906.44 | 1076.21 / 913.15 | 22.83 / 22.79 | 4308.73 / 1095.48 |
| pair3-tiled-half | 693.34 / 888.59 | 1040.57 / 897.38 | 23.12 / 23.10 | 4295.10 / 1104.38 |
| pair4-parent | 708.08 / 908.35 | 1076.16 / 913.26 | 22.52 / 22.46 | 4284.33 / 1093.30 |
| pair4-tiled-half | 693.34 / 891.70 | 1060.20 / 897.23 | 22.75 / 22.71 | 4223.34 / 1090.90 |

Initial tiled end VRAM increases 18.15–26.33 MB even with fewer requested bytes.
The follow-up's end VRAM decreases 15.77–16.03 MB, but end PSS changes
+8.89 and −2.40 MB. Its sampled maximum PSS falls 13.63–60.99 MB.
These observations do not establish a stable whole-process memory saving
or the G4 appearance-representation ratio.

## Shaded quality and silhouette-adjacent errors

Reuse the completed focused and native shaded references, their coarse/fine
and independent-repeat controls, locked exposure/Reinhard/sRGB transform,
FLIP at 67 pixels/degree and LPIPS AlexNet v0.1. References are never regenerated
from the candidate. A small adapter routes the established capture Session's
Shared type through the actual measured Tiled operator; capture/reference logic
is unchanged. Both qualification builds contain a Tiled GPU shader byte-identical
to the measured follow-up shader (SHA-256 recorded in the bundle).

The focused fixture is 192×108, forced-impostor shot 3; the native fixture is
1920×1080, normal production LOD shot 0. Both cover 1×–4× and whole/crown regions.
There are **36 / 36** passing development comparisons using the
unchanged G4 tolerances: PSNR loss ≤0.25 dB and FLIP/LPIPS increase ≤5%.

| Family | Passing checks | FLIP ratio | LPIPS ratio | PSNR Δ dB |
|---|---:|---:|---:|---:|
| focused | 24 / 24 | 1.0004–1.0391 | 0.9653–1.0247 | -0.216 to +0.070 |
| native | 12 / 12 | 0.9858–0.9974 | 0.9997–1.0013 | -0.002 to +0.001 |

A reference-derived four-pixel band around the existing native coverage
threshold measures shaded RGB near silhouettes. This diagnostic was added
after the paired-image mismatch to interpret it; it is not a new acceptance
gate or independently recovered candidate silhouette. It reuses existing data.

| Scale | Boundary pixels | RGB MAE ratio | PSNR Δ dB | p99 RGB error, baseline → candidate |
|---|---:|---:|---:|---:|
| 1× | 90649 | 1.0002 | +0.001 | 0.1240 → 0.1239 |
| 2× | 90649 | 1.0000 | +0.001 | 0.2464 → 0.2463 |
| 3× | 90649 | 1.0000 | +0.001 | 0.2887 → 0.2887 |
| 4× | 90649 | 0.9999 | +0.001 | 0.3031 → 0.3031 |

The integrated source-coverage diagnostic has precision **0.99999**,
recall **1.00000**, symmetric boundary p99 **0.000 px**
and maximum **2.828 px** against the fine reference.
This is leaf-class coverage, including impostor wood but excluding separately
drawn near wood. It does not run the temporal reconstruction and therefore cannot
prove that the changed resolver preserves silhouettes. The shaded boundary
diagnostic above covers that changed output, but neither substitutes for G3's
strand/instance and motion tests.

The strict same-input temporal fixture fails for both tiled variants:
**113,979 / 165,676** pixels are not bitwise equal. Seven cases × four scales,
odd 97×61 output, jitter, rigid/deforming motion and the invalid-bound negative
control are preserved. Production paired RGB differs by up to **22/255**.
The exact numerical cause is unresolved; do not attribute all image differences
to half conversion. No claim of equivalent rendered output is made.

## Decision, failures and next bet

Retire the initial and optimized tiled paths. The only repeatable improvement
is requested storage; the complete runtime loses and reference quality does not
supply a compensating consistent gain. No second optimization is necessary to
justify that decision. The storage-only control is also not retained.

Preflight type/dispatch and fixture-selection errors, the corrected edge-origin
bug, strict fixture failures, all captures and all timings remain archived.
Verification is proportional: existing temporal cases and shaded captures,
restoration against parent bytes, and the documentation allowlist. No unchanged
distance machinery or broad GPU suite is re-audited.

The next architecture-level bet is **source-certified visibility and temporal
correspondence regions that avoid work altogether**, with edit/disocclusion
invalidation proved and charged. Both conventional and learned alternatives
must receive the same certificate. Prior approximate stationary-camera shortcuts
already failed; this is not permission to repeat those heuristics or treat
unchanged pixels as proof of unchanged radiance.

Evidence: build/neural-evidence/temporal-tiles-v1 on Pop!_OS.
Manifest SHA-256: a17befe2ece5881ab71f0f7a146642643c7adf36480c5512897ae5fcc90ba689.
Replay starts from the recorded parent and preserved scripts/sources, in a fresh
evidence directory with a reset two-hour clock. Build candidates before queueing,
run queue_temporal_tiles_initial.py then queue_temporal_tiles_half.py, and use
summarize_temporal_tiles.py for the joint assessment. Existing reference files
are pinned by hashes. Failed sources are never overwritten.

Provenance: the existing Wrela resolver and pinned public Half2/Shared APIs and
lowered WGSL, preserved in the bundle. No third-party code, assets or model
weights were introduced. The [benchmark charter](neural-rendering-goals.md)
remains the authority for claims.
