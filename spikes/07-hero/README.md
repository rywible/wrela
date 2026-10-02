# Spike 07: the hero creature

*One of spikes 03–08: the hardest cases for a pure ray-marched renderer. Throwaway code, not the start of the engine.*

## The question

**Can a hero creature fill the screen, with fur and believable joints, inside the creatures slice (2.5 ms at 1080p)? If not, where's the limit?**

Spike 02 found close-ups are ray marching's worst case: its grazer cost 5.5 ms at 63% screen coverage, about 8.7 ms extrapolated to full screen (an estimate), with no fur and rigid parts. This spike takes a harder creature (the agent-authored wolf from `experiments/agent-authoring/wolf/`), makes it walk, gives it volumetric fur, and pushes coverage to 100%.

## Kill criteria, written before measuring

The budget slice is **creatures: 2.5 ms of GPU time at native 1920×1080** on the reference device (MacBook Air M4, 8-core GPU, Chrome, WebGPU), from GPU timestamps under sustained load (`spikes/README.md`).

**Creature time** is the marginal cost of the wolf, as in spike 02:

> creatures = `bin_screen` + `bin_light` + (`trace` with the wolf − `trace` of the same view without it)

It covers primary visibility, fur, shading, ambient occlusion and marched sun shadows on everything (ground included). It doesn't include the ground and sky themselves.

**1. The verdict,** for the hero configuration (the joint method chosen by criterion 3, plus volumetric fur with footprint LOD) at **100% coverage**, 1080p:

| Creature time | Verdict |
|---|---|
| ≤ 2.5 ms | **Pass** |
| 2.5–5 ms | **Inconclusive:** optimization or reallocating budget might save it |
| > 5 ms | **Fail:** use the fallback |

**2. Where the limit is:** the coverage at which each configuration crosses 2.5 ms and 5 ms, at 1080p and at 960×540 internal resolution (the usual lever for a temporal upscaler, which this spike doesn't implement, so it claims no upscaled quality).

**3. Joints.** Two methods, measured while walking, at the elbows, hocks and neck:
- **Rigid:** parts rigid in their bone's frame, joined by smooth unions (spike 02's approach).
- **Warp:** a bend warp per joint whose Lipschitz factor is known each frame, so the march stays safe by dividing the step by it.

The warp is **worth it** if it looks visibly better at those joints in the screenshots *and* costs ≤ 15% more creature time than rigid at 100% coverage. Its Lipschitz factor must stay ≤ 2 through the walk cycle, and it must pass criterion 4. Otherwise rigid is the hero configuration.

**4. Correct, not just fast** (spike 02's rule). Each fast path is compared with a brute-force reference of the same content: all parts every step (no tile or per-ray masks), half-size steps, a hit tolerance of 1/20 pixel, step caps in the thousands.
- **Geometry** (surface-shaded fur, one sample per pixel at the pixel centre, against the reference at the same sample): mean difference **≤ 0.5/255**, **≤ 0.5%** of pixels off by more than 8/255, and rays ending on the step cap **≤ 0.1%** of creature pixels.
- **Volumetric fur** can't be judged at one sample per pixel: strands are thinner than a pixel. Its reference also takes **16 jittered samples per pixel** with full-detail strands and 4× the volume steps. The LOD'd fast path passes if its mean difference from that reference is **no worse than the brute-force reference's own one-sample render** (same detail, fine steps, pixel centre) against it. That is, footprint LOD must not add error beyond what one sample per pixel already has. The share over 8/255 is reported without a threshold: one-sample fur isn't expected to meet 0.5%, and temporal accumulation (not built here) is the usual answer.

**5. Hybrid numbers.** The same wolf as a rasterized skinned mesh (extracted from the field, linear-blend skinned, per-pixel shaded with the same material; shadows from a shadow map per D-086), with and without 16 alpha-blended fur shells. Creature time there = shadow map + mesh + shells. These numbers aren't pass/fail; they size the fallback.

**6. Looks.** Screenshots of every scene, judged honestly for beauty and not just correctness.

## Fallback if it fails

**Rasterize creatures above a screen-size threshold** (the hybrid: "rasterize heroes, trace crowds") and keep pure tracing for small and distant creatures. The spike reports the threshold where tracing's cost crosses the raster path's.

## Method

- **The creature:** the agent-authored wolf's field, re-rigged as 22 parts on a 22-bone skeleton, each part rigid in one bone's frame (spike 02's posing). The millimetre fbm "fur noise" the author put in the field is left out of the marched surface: the volumetric fur replaces it.
- **The walk:** a lateral-sequence walk (0.9 s cycle, duty factor 0.6), with two-bone IK for each leg, a lowered and bobbing head, and a swaying tail. The wolf walks forward and the camera follows it.
- **Bend warp:** a polar warp around the joint's hinge axis. A point's angle around the axis is remapped smoothly between the parent bone's wedge (identity) and the child's (rotated by the joint angle); radius and the axis coordinate are unchanged. Its Jacobian is diagonal in cylindrical coordinates, so its Lipschitz factor is exactly the largest angular stretch, computed per joint per frame.
- **Fur:** a shell from 6 mm inside the authored surface to 12 mm outside it.
  - **Marching:** the ray sphere-traces to the shell's outer envelope, then steps through it, compositing a strand density front to back.
  - **Shading:** Kajiya-Kay with two shifted lobes.
  - **Strands:** a 2D array texture (one layer per height in the shell), sampled triplanar in rest space at the strand's root, leaning along a comb direction.
  - **LOD by pixel footprint:**
    - the strand texture is filtered over the footprint across the comb and over each step's sweep along it (anisotropic; added after the first measurements showed aliasing);
    - the volume step is no finer than ¾ of the footprint;
    - beyond ~3 mm per pixel, there's no volume at all, only surface Kajiya-Kay shading.
- **Binning:** per 8×8 tile and per light-grid cell, a part mask from each part's oriented bounding box (computed at startup by sampling the field). Per ray, exact ray intervals against those boxes give the per-ray part mask and march interval.
- **Raster hybrid:** the rest-pose field extracted by surface nets at 5 mm, skin weights from part distances, linear-blend skinning, per-vertex baked AO, a 2048² shadow map.

## Sweep

| Parameter | Values |
|---|---|
| Screen coverage (camera distance found by search, side-on to the flank) | 1, 3, 10, 25, 50, 75, 100% (8.8 m, 5.0 m, 2.7 m, 1.74 m, 1.13 m, 0.74 m, 0.23 m) |
| Joints | rigid, warp |
| Fur | surface shading only (`s`), volumetric with LOD (`v`) |
| Resolution | 1920×1080 (all four); 960×540 (rigid `s`, rigid `v`, warp `v`) |
| Raster | mesh, mesh + 16 shells |
| Fur knobs (100%, warp) | LOD off, 6 / 24 volume steps, no AO, no shadows, field every 3rd volume step, surface only, surface without shadows or AO |

The 1% and 3% points were added after the first measurements showed every traced configuration was over the slice at 10%. The kill criteria didn't change.

## Layout

| File | What |
|---|---|
| `wolf.js` | Skeleton, parts, walk cycle and IK, bend-warp parameters and Lipschitz factors, the per-frame pose buffer |
| `common.wgsl` | Frame uniform, pose buffer layout, noise, SDF helpers, sky, ground, Kajiya-Kay fur lighting |
| `wolf.wgsl` | The wolf's parts in rest space, adapted from `creature.wgsl`, plus its albedo, coat direction, coat length and bare regions |
| `posed.wgsl` | The posed field: rigid parts, the bend warp, the fold over a part mask, box bounds and ray intervals |
| `bin.wgsl` | `bin_screen` (8×8 tiles), `bin_light` (64² sun columns), `bin_volume` (AO cells) |
| `bounds.wgsl` | Startup part boxes by Lipschitz-conservative sampling; the per-frame refit of the warped joint regions' boxes |
| `trace.wgsl` | The kernel: ground, wolf, fur shell, shadows, AO, shading; the reference, stats and heat-map variants |
| `extract.wgsl` | Surface-nets extraction of the rest pose for the raster path, with skin weights and baked AO, albedo and coat |
| `raster.wgsl`, `raster.js` | The raster hybrid: shadow map, ground, skinned mesh, fur shells |
| `main.js` | Harness: pipelines, banded submission, scenes, coverage search, timing, the reference, screenshots |
| `run.js` | The `#run` sequence and its summary |
| `results/` | Raw JSON from each run, `console.log`, JPEG copies of the key screenshots (PNGs are ignored) |

## Running it

```bash
spikes/headless.sh 07-hero '#run' 900
```

- `#quick` (~5 s once the pipeline cache is warm) renders a few scenes and saves screenshots.
- `#probe` times the heaviest frame and the fur reference.
- **GPU safety rules:**
  - Every traced frame is split into row bands, each one queue submission, sized by a timed probe frame to stay near 30 ms.
  - References run one 8-pixel row at a time, at most 120 pixels wide, each awaited.
  - Extraction runs in z slabs and the startup bounds one job at a time, each awaited.
  - Pipelines compile three at a time.
  - The worst single submission seen in the final run was ~70 ms of wall clock, which includes queue latency.

## Results (2026-10-02)

**Setup:**
- MacBook Air M4 (8-core GPU), Chrome 154 headless through `spikes/headless.sh`, WebGPU with `timestamp-query`.
- Final run: `results/run-2026-10-02T02-06-11-109Z.json`, 207 s.
- **Every timing here is indicative.** Other spikes were queued on the same GPU lock and the desktop was in use.
  - Two earlier full runs of the same measuring code, minutes apart, are kept: `run-…01-44-15-158Z.json` and `run-…01-50-37-242Z.json`. They predate the last two shading fixes (the skin colour and the volume step size).
  - At 100% coverage, rigid fur measured 81.2, 104.1 and 88.9 ms across the three runs, so run-to-run variation is roughly ±15%.
- **Times are medians** of creature GPU time under sustained load, from timestamps. Creature time = refit + bins + (trace − trace of the same view without the wolf).

### Verdict

| Criterion | Measured (1080p unless stated) | Verdict |
|---|---|---|
| 1. Hero configuration at 100% coverage ≤ 2.5 ms | **Rigid joints + volumetric fur: 88.9 ms** (p95 114). That's 36× the slice. 960×540: 23.2 ms. | **Fail:** use the fallback |
| 2. Where the limit is | Every traced configuration is already over 2.5 ms at **1% coverage** (a wolf 8.8 m away): rigid fur 3.5 ms, surface only 3.1 ms. It crosses 5 ms at ~1.3% (fur) and ~1.9% (surface). At 960×540, rigid fur crosses 2.5 ms at ~1.8% and 5 ms at ~4.8%. | Pure tracing fits only distant creatures |
| 3. Joints: warp worth it? | **Lipschitz factor:** max 1.91 through the cycle (≤ 2 ✓). **Correctness:** passes criterion 4 ✓. **Looks:** visibly better at the hock, subtle at the elbow, no visible difference at the neck. **Cost:** +33% with fur and +25% surface-only at 100% (the bar is ≤ 15%). | **Not worth it** by the rule, so the hero configuration is rigid |
| 4. Correctness: geometry | 100%: mean 0.039/255 (rigid), 0.045 (warp); 0.0003% and 0.03% of pixels over 8/255. Elbow and hock close-ups: ≤ 0.056 and ≤ 0.004%. Step-cap hits ≤ 0.08% of creature pixels. | **Pass** |
| 4. Correctness: fur | Fast against the 16-sample, 192-step reference: mean **1.48**/255, 0.006% of pixels over 8/255. The one-sample reference against it: **0.026**. | **Fail as written** (see "Fur" below) |
| 5. Hybrid numbers | **Raster mesh** (shadow map + mesh): 1.6–3.3 ms, crossing 2.5 ms at ~26% coverage. At 960×540: 0.85–1.44 ms. **With 16 fur shells:** 8.7–20.4 ms (960×540: 6.6–11.8). | The fallback fits for an unfurred hero; furred heroes don't fit either way |

**So the answer is no.** With these techniques, a furred hero filling the screen costs ~90 ms traced, against a 2.5 ms slice. The limit for pure tracing is about 1% screen coverage at 1080p (~2% at 960×540). Even the fallback only fits without volumetric fur: shells add 7–17 ms.

### Cost against coverage

Creature GPU ms, median. Coverage is the share of pixels whose creature opacity is ≥ ½.

| Coverage | Rigid, surface | Warp, surface | Rigid, fur | Warp, fur | Raster mesh | Raster + 16 shells |
|---|---|---|---|---|---|---|
| 1% | 3.1 | 4.6 | 3.5 | 5.0 | 1.6 | 8.7 |
| 3% | 7.3 | 9.0 | 12.3 | 17.5 | 1.8 | 10.4 |
| 10% | 16.1 | 21.6 | 33.8 | 47.4 | 2.1 | 13.2 |
| 25% | 33.9 | 43.6 | 68.2 | 87.3 | 2.5 | 16.2 |
| 50% | 50.5 | 65.9 | 105.1 | 133.2 | 2.9 | 19.5 |
| 75% | 64.3 | 84.9 | 126.9 | 169.8 | 3.3 | 20.4 |
| 100% | 55.8 | 70.0 | 88.9 | 118.0 | 3.3 | 16.9 |
| **960×540** | | | | | | |
| 1% | 1.8 | | 1.9 | 2.6 | 0.85 | 6.6 |
| 10% | 6.1 | | 9.8 | 11.9 | 1.1 | 9.4 |
| 50% | 16.3 | | 38.2 | 50.2 | 1.4 | 11.8 |
| 100% | 16.4 | | 23.2 | 27.0 | 1.2 | 8.0 |

- **Cost peaks at 75%, not 100%.** The 100% view is a side-on macro of the flank (0.23 m away): few parts, no head, few silhouettes. The 75% view still holds legs, the belly line and the shoulder. Coverage alone doesn't predict cost.
- **At 1%, most of the 3.5 ms isn't the wolf's own 19K pixels.** With the wolf out of view, the same kernels cost only 0.13–0.33 ms more than the ground alone, so it isn't kernel overhead. It's the shadow marches and ground AO around the wolf, and divergence on silhouette rays (a hypothesis: not profiled further).
- **60 Hz pacing** (warp fur at 10%): at 1080p a median of 46.9 ms, all 80 frames over 16.7 ms. At 960×540 a median of 12.6 ms, maximum 16.0 ms, none over.

### Where the time goes

Per pixel at 100% coverage, rigid fur: **146 part evaluations.** That breaks down as about 6–9 march steps, 15.6 volume steps in the fur, 7.5 shadow steps, 2 AO samples over ~10 parts, and 5 evaluations for the shading frame, all at 5–8 parts per evaluation. Spike 02's grazer fill was about 27 part evaluations per pixel at 1.9 parts each: 4.2 ns per covered pixel, against 43 ns here.

What removing each piece saves, at 100% (warp, fur, 115.7 ms in the same run):

| Knob | Creature ms | Saving |
|---|---|---|
| No sun shadows | 76.2 | 39.5 |
| No AO | 79.0 | 36.7 |
| Surface fur only (no volume) | 69.9 | 45.8 |
| Surface only, no shadows, no AO | 23.9 | 91.9 |
| Field evaluated every 3rd volume step | 101.0 | 14.7 |
| 6 volume steps instead of 12 | 95.1 | 20.6 |
| 24 volume steps | 156.4 | −40.6 |
| No LOD (no anisotropic filtering) | 99.9 | 15.8, but aliases (below) |

- **Parts per evaluation, not steps, is the main multiplier.**
  - The agent gave the wolf blend radii of 5–8 cm.
  - Dropping a part from a ray's mask is exact only beyond its blend radius, so each part's box is inflated by that much (plus the fur shell).
  - That puts 5–8 parts into every evaluation near the body, against spike 02's 1.9.
- **Per-step interval masks helped:** −35% part evaluations against per-ray masks, plus −0.6 steps per pixel from a 10 cm lookahead.
- **Over-relaxation didn't help** (1.4 and 1.8 were the same or worse), as in spike 02.
- **The march itself takes ~9 steps per pixel** on the surface (spike 02: 4–7). It starts at box entries 10 cm or more from the surface, and most of a body is seen at oblique incidence.
- **Shadows and AO marched through the field cost as much as the fur volume.** Spike 01's shadow map cost 0.33 ms here for the mesh.

### Joints: rigid against bend warp

The warp is a polar remap of the angle around the joint's hinge axis: the parent wedge is identity, the child wedge is rotated by the joint angle, with smoothstep gaps between them. Its Lipschitz factor is exactly 1 + 1.5·|θ|/(width of the closing gap), computed per joint per frame. Steps are divided by it only while the joint region's box is active along the ray.

| Joint | Range over the walk | Warp L, max | Looks (`elbow-*`, `hock-*`, `neck-*` screenshots, at maximum flexion) |
|---|---|---|---|
| Elbow | −26° to +34° | 1.36 | Rigid has a slightly rounder ball-joint bump at the back; the warp is smoother. Subtle. |
| Hock | −43° to +27° | 1.91 | Rigid shows a crease where the gaskin and metatarsus "sausages" meet at the front of the hock; the warp gives a smooth concave fillet. **Visibly better.** |
| Neck (base) | 15° to 20° lowered | 1.17 | No visible difference at this bend. |

- **Correctness:** the warped field passes the reference check (elbow and hock close-ups, 100% view), and step-cap hits stay ≤ 0.08% of creature pixels.
- **Bounds:** a bent region has no fixed bounding box. Its two boxes are refit every frame by sampling the warped field on a 32³ grid in the parent bone's frame, which costs **0.07–0.13 ms**.
- **Cost:** the warp's 25–33% comes mostly from shrinking steps (L up to 1.9) over the whole joint-region box, which covers the whole hind leg. The rear half of `hero-heat-v.jpg` shows it.
- **Verdict:** rigid parts are fine at walk amplitudes except the hock, so a warp might be worth it only for strongly flexed joints, and a narrower warp region would cost less (untested).

### Fur

- **The model:** a shell from 6 mm under the authored surface to 12 mm above it. Strands are a 16-layer, 512² texture over a 12.8 cm tile (13K strands in clumps, agouti-banded), looked up triplanar at the strand's root and leaning 24° along a comb direction. Underfur is analytic. Lighting is Kajiya-Kay with two shifted lobes, plus self-shadowing toward the root.
  - **Short coats** (face, legs) are an even pile.
  - **Eyes, nose and claws** are bare.
- **LOD by footprint:**
  1. strand filtering;
  2. a volume step no finer than ¾ of the footprint;
  3. no volume at all beyond 3 mm per pixel (at about 5 m), where only surface Kajiya-Kay shading remains.
- **The central difficulty: leaning strands need fine steps.**
  - Each volume step moves the lookup by LEAN × dt along the comb: 3.3 mm per step at 12 steps.
  - Strands are 0.5–0.9 mm wide, so the fast path steps over them and aliases into blotches: 2.5/255 mean, 2.6% of pixels over 8/255 without filtering.
  - **Anisotropic hardware filtering along the comb** (`textureSampleGrad` over each step's sweep) brings it to 1.48 mean and 0.006% over 8/255, at about 16 ms.
  - Strands then read as soft streaks rather than crisp hairs.
  - 24 steps gives 1.15 mean, for +41 ms.
- **The reference** takes 192 steps (a sweep below a strand's width), 16 jittered samples and every part, on a 240×136 crop.
  - **The criterion was wrong.** It assumed one sample per pixel would be the error floor. At this magnification the strands blur along the comb even in the reference, so one sample is already within 0.026 of sixteen.
  - **What the error is:** the fast path's difference is volume integration (12 steps against 192) and filtering, not sampling.
  - **Verdict:** it fails criterion 4 as written. In absolute terms it's 1.5/255.
- **Fur step caps:** 0.8% of creature pixels at 100% coverage hit the volume-step cap (36). They continue marching, so there are no holes, but this is an approximation.

### The raster hybrid

- **The mesh:** extracted at 5 mm (110K vertices, 220K triangles, 0 holes) in 44 ms of GPU at startup.
- **Skinning:** four bones per vertex from part distances. AO, albedo and coat are baked per vertex; the mesh pass evaluates albedo per pixel, like the tracer.
- **The mesh is cheap:** 1.6–3.3 ms at 1080p. It costs more than spike 01's close-up (1.05 ms) because albedo is evaluated per pixel and the fur's Kajiya-Kay comb direction uses noise.
- **The shells aren't:** 16 shells add 7–17 ms.
  - There's no mesh LOD, so even at 1% coverage that's 3.5M triangles (spike 01: creatures are triangle-bound).
  - The shells show the classic stacked-layer striping at grazing angles and no silhouette fuzz, because no fins were built.
- **Fallback threshold:** in this implementation the raster mesh is cheaper than tracing at every measured coverage (1.6 against 3.1 ms at 1%). So for a single creature, the screen-size threshold where tracing wins is below 1%. Spike 02 found tracing wins on instance count (crowds), not on single creatures.
- **Unexplained:** the raster ground pass (a fullscreen fragment pass that writes depth) takes 18 ms, against 0.8 ms for the same shading in the compute kernel. It isn't part of creature time, but it suggests the raster numbers may be pessimistic. Not investigated.

### Looks, honestly

- **At hero and mid distances** (`hero-rigid-v.jpg`, `cov10-warp-v.jpg`, `cov50-warp-v.jpg`), the traced wolf reads as a furred grey wolf:
  - soft strand fuzz on the back, mane, belly line and tail;
  - a grizzled saddle and a coat that flows toward the tail;
  - soft shadows and contact AO.
  - The silhouettes are clearly better than the raster shells'.
- **The face, legs and paws** have a short coat and read as smooth suede, close to plastic. The ears look plasticky, and the muzzle is the agent's boxy one.
- **At 100% coverage** (`cov100-warp-v.jpg`, 23 cm from the flank), the coat is soft streaks with no individual guard hairs. It's believable as out-of-focus fur, not as a hero macro.
- **The walk reads correctly:** lateral sequence, folding carpus in swing, flexing hock. It's simple, and feet slide slightly at full reach (IK reach up to 1.03).
- **Overall:** a good placeholder to mid-tier creature, not hero quality.
  - There's no anti-aliasing and no temporal accumulation (silhouettes alias, as in spike 02).
  - The creature is the agent-authored placeholder of D-095.

### Other costs

- **Pipelines:** 17 trace variants, plus bins, bounds, refit, raster and blit. Cold creation took 10.6 s compiling three at a time (one module, specialized by override constants). Each scene needs 5 (rigid: three bins, trace, blit) or 8 (warp adds three refit passes).
- **Memory:** strand texture 10.7 MB; pose buffer 7.7 KB per frame; tile, light and AO bins under 0.2 MB; raster mesh 7.5 MB plus a 16 MB shadow map.
- **CPU:** posing, IK and the warp parameters take ~0.1 ms per frame in JS.
- **Startup:** part boxes by sampling, 18 jobs at 80³ cells: 18 ms of GPU.

### What surprised me

1. **Blend radii set the cost.** The 5–8 cm smooth unions that make the agent's wolf look organic also make every part's bound 5–8 cm fatter, and that multiplies every evaluation.
2. **Shadows and AO** marched through the field cost as much as the volumetric fur at full coverage.
3. **Volumetric fur's step count follows the strands' lean,** not the shell's thickness. Without anisotropic filtering, a 12-step shell can't avoid aliasing.
4. **My first fur "reference" was itself aliased** (48 steps). It only showed up because 1 and 16 samples agreed suspiciously well.
5. **Cost peaked at 75% coverage,** not 100%.

### Deviations from the protocol, and caveats

- **Frames:** configurations whose frame is over ~17 ms got ≥ 0.5 s of warm-up (4–30 frames) and ≥ 1.5 s timed (≥ 20 frames), not 30 + 90, to keep the run near 3.5 minutes. Light configurations got the full 30 + 90.
- **Banding:** each frame is split into 1–16 row-band submissions (GPU safety rules). Trace time is the sum of the bands, so short gaps between bands aren't counted.
- **References:** joint references cover a 960×544 crop and the fur reference a 240×136 crop.
- **Timestamps** are quantized to ~65.5 µs.
- **Coverage** is measured at one walk phase, with spread checked over four (±2%).
- **Not built:** anti-aliasing, temporal accumulation, mesh LOD for the raster path, fins, a strand-level silhouette model.
- **Not tested:** the M4 against other devices, Safari, Firefox.
- **The STEP_SCALE of 0.9** assumes the noise-free wolf field has L ≤ 1.1. That's a hypothesis, which the reference check supports for this content.

### What this means for the design

Nothing here is recorded in `decisions.md`; that's the owner's call. These are hypotheses for it.

- **Hero creatures filling the screen with fur don't fit a 2.5 ms slice by pure tracing,** and not by an optimization margin (36×). This supports choosing by screen size (D-001's allowance; spike 02's suggestion): trace distant creatures, rasterize close ones.
- **Even the raster path doesn't afford volumetric fur at that slice.** A furred hero needs a cheaper fur representation, such as baked fur shading with silhouette fins, or a bigger creature slice in close-ups. Both untested.
- **Content and language:**
  - Blend radii and part decomposition are a performance contract, not just looks.
  - A compiler that derives part bounds (D-080) could also report each part's inflation, so authors see the cost of a soft blend.
- **Bend warps with a known Lipschitz factor work:** correct, L ≤ 2 for a walk. Per-frame bounds refit is cheap.
  - They cost 25–33% with this region size and pay off only at strongly flexed joints.
  - Their facts are naturally scoped to a region (D-092).
- **Field-marched shadows and AO for creatures are expensive at close range,** which supports D-086's shadow maps for deforming creatures.

## Quiet-machine rerun (2026-10-02)

Run file: `results/run-2026-10-02T04-03-46-737Z.json` (197 s): serial, after a cool-down, Chrome 154 headless, alone on the GPU. Times are creature GPU ms, medians, at 1 / 10 / 50 / 100% coverage, at 1080p and then at 960×540 ("540p"). At these points they moved by at most 4.6% from the README's run (one 65.5 µs timestamp tick, on the 540p mesh at 50%), and by at most 1.8% at 1080p. The correctness numbers are identical.

| Metric | README value | Quiet value |
|---|---|---|
| Rigid, surface only | 3.1 / 16.1 / 50.5 / 55.8; 540p 1.8 / 6.1 / 16.3 / 16.4 | 3.1 / 16.1 / 50.9 / 55.8; 540p 1.7 / 6.0 / 16.3 / 16.4 |
| Rigid, fur (the hero) | 3.5 / 33.8 / 105.1 / 88.9; 540p 1.9 / 9.8 / 38.2 / 23.2 | 3.5 / 34.0 / 105.3 / 88.9; 540p 1.9 / 9.8 / 37.6 / 23.3 |
| Warp, fur | 5.0 / 47.4 / 133.2 / 118.0; 540p 2.6 / 11.9 / 50.2 / 27.0 | 5.0 / 46.5 / 133.8 / 118.4; 540p 2.7 / 12.1 / 49.9 / 27.5 |
| Raster mesh | 1.6 / 2.1 / 2.9 / 3.3; 540p 0.85 / 1.1 / 1.4 / 1.2 | 1.6 / 2.1 / 2.9 / 3.3; 540p 0.85 / 1.1 / 1.4 / 1.2 |
| Raster + 16 shells | 8.7 / 13.2 / 19.5 / 16.9; 540p 6.6 / 9.4 / 11.8 / 8.0 | 8.7 / 13.0 / 19.6 / 16.9; 540p 6.7 / 9.5 / 11.7 / 7.9 |
| Warp overhead at 100%, 1080p: fur / surface | +33% / +25% | +33% / +26% (ratios 1.331 / 1.259) |
| Coverage at crossing: rigid fur / surface 5 ms; 540p rigid fur 2.5 / 5 ms; raster mesh 2.5 ms | ~1.3% / ~1.9%; ~1.8% / ~4.8%; ~26% | 1.3% / 1.9%; 1.8% / 4.7%; 23.5% |
| Geometry, rigid / warp at 100%: mean /255; share over 8/255; step caps, all views | 0.039 / 0.045; 0.0003% / 0.03%; ≤ 0.08% | 0.039 / 0.045; 0.0003% / 0.03%; ≤ 0.08% |
| Fur, mean /255: fast vs the 16-sample reference; one-sample reference vs it | 1.48 (0.006% over 8/255); 0.026 | 1.48 (0.006% over 8/255); 0.026 |

**Verdicts: none change** under the README's criteria. The hero is still 88.9 ms at 100% (Fail); every traced configuration is still over 2.5 ms at 1%; the warp still costs +33% against the ≤ 15% bar; geometry passes and fur fails as written; only the raster mesh's 2.5 ms crossing moves (~26% to ~24%), which changes no verdict.
