# Spike 10: materials from field quantities

*A measurement spike for the flagship's hard materials. It's throwaway code, not the start of the engine.*

## The question

A field gives some quantities almost for free, or for a few extra evaluations:

- **thickness**, by marching *inward* from a surface point
- **curvature**, from the derived gradient and Hessian (here: the Laplacian, from a five-tap stencil)
- **occlusion**, from distance samples along the normal
- **the pixel footprint**, which with the noise's declared bandlimits gives the normal's variance inside a pixel

**Can those quantities give convincing hard materials, at a cost that fits the slice of the system each material belongs to?** Six materials, on the reference device (MacBook Air M4, 8-core GPU, Chrome, WebGPU), at 1920×1080:

| # | Material | Field quantity it leans on | System (slice) |
|---|---|---|---|
| 1 | **Skin and hide with subsurface scattering:** backlit ears glowing, thin membranes, soft falloff | thickness along the light | creatures (2.5 ms) |
| 2 | **Translucent leaves,** sun through a leaf from either side | thickness along the light | vegetation (4.0 ms) |
| 3 | **Eyes:** refraction through a wet cornea, iris depth, specular highlight | the cornea's field normal | creatures (2.5 ms) |
| 4 | **Wet surfaces:** darker albedo, sharper specular, puddles in concave areas | curvature at puddle scale | world geometry (3.0 ms) |
| 5 | **Layered stone:** moss on up-facing occluded areas, weathering in crevices, edge wear | normal, occlusion, curvature | world geometry (3.0 ms) |
| 6 | **Specular anti-aliasing:** roughness widened by the normal's variance in the pixel footprint (Toksvig/LEAN-style) | footprint, bandlimits, curvature | world geometry (3.0 ms) |

## Kill criteria, written before measuring

**A material's extra cost** is the GPU time of the shading pass with the material on, minus the same pass with *plain shading* on the same pixels, in the same scene, from the same camera. Plain shading is: field normal, base albedo, Lambert diffuse plus GGX specular from the sun with a marched soft shadow, hemispherical sky light and an analytic sky reflection. The visibility pass is identical in both and isn't counted. Times are medians of 90 frames after 30 warm-up frames, from timestamp queries, at native 1920×1080.

1. **Cost,** at the material's representative screen coverage:

   | Extra cost | Verdict |
   |---|---|
   | ≤ 25% of its system's slice | **Pass** |
   | ≤ 50% | **Inconclusive** |
   | > 50% | **Fail:** use the fallback |

   | Material | Slice | Pass ≤ | Inconclusive ≤ | Representative coverage |
   |---|---|---|---|---|
   | Skin and hide | 2.5 ms | 0.625 ms | 1.25 ms | **25%** of the screen (spike 02's creature close-up was 22%) |
   | Eyes | 2.5 ms | 0.625 ms | 1.25 ms | **2%** (two eyes in a creature close-up); also reported at 25%, an extreme close-up |
   | Leaves | 4.0 ms | 1.0 ms | 2.0 ms | **50%** (foliage in a forest frame) |
   | Wet | 3.0 ms | 0.75 ms | 1.5 ms | **50%** (ground, rock and walls in a forest frame) |
   | Layered stone and moss | 3.0 ms | 0.75 ms | 1.5 ms | **50%** |
   | Specular anti-aliasing | 3.0 ms | 0.75 ms | 1.5 ms | **50%** |

   - The representative coverages are this spike's judgment, set before measuring. Each material is also swept over coverage, so a reader who prefers another coverage can read it off the curve.
   - The value at the representative coverage is interpolated linearly between the two nearest measured coverages.
   - **Lighting is the worst case for translucency:** a low golden-hour sun *behind* the subject, so most visible skin and leaf pixels are backlit, and only backlit pixels march inward.

2. **Correct, not just fast.** Each material's fast path is compared with a brute-force reference of the same content, on the final tonemapped 8-bit image:
   - **on the material's own pixels:** mean |difference| **≤ 2/255** and **≤ 2%** of them off by more than 8/255;
   - **primary step-cap hits ≤ 0.1%** of the material's pixels.

   A material that misses this is at best **inconclusive** whatever its cost. These thresholds are per material's own pixels, so they're stricter than spike 02's full-frame 0.5/255 and 0.5% at low coverage and about equal at 25% coverage. They're this spike's judgment.

3. **Specular anti-aliasing has its own correctness test.** Against a 16-sample-per-pixel supersampled reference of the unfiltered surface, under slow camera motion (about a quarter of a pixel per frame):
   - **shimmer:** ~~the mean frame-to-frame difference in excess of the reference's own~~ *(changed after measuring, see below)* the flicker must drop by **≥ 50%** against footprint-filtered normals without roughness widening (D-077's plan of record);
   - **accuracy:** the mean |difference| from the supersampled reference must not get worse.

   **Changed after measuring:** the "excess over the reference" metric couldn't work. The 16-sample reference's own sampling noise and sharper detail make its frame-to-frame difference *larger* than every filtered variant's (1.86 against 1.12/255 on the tower), so the excess is negative for all of them. It's replaced by **flicker**, the mean |I(t) − ½(I(t−1) + I(t+1))|, which cancels the change that's linear in time, as slow motion of a stable image is, and keeps sparkle that pops on and off. The ≥ 50% rule is unchanged.

4. **Looks.** Screenshots of every scene, judged honestly. Beauty is part of the question, so this is reported, not scored.

## The fallback, if a material fails

- **Thickness:** bake local thickness (per brick or per vertex) at load, as games did before fields (Barré-Brisebois and Bouchard 2011), and keep only the transmission formula per pixel.
- **Curvature and occlusion masks:** bake them into the world's bricks with the far-field cache (D-077 already allows baking channels beyond a distance), and look them up.
- **Eyes:** parallax-offset the iris on the outer surface instead of refracting.
- **Specular anti-aliasing:** temporal accumulation alone (the 1.0 ms TAA slice) with a fixed roughness floor.

## Scenes

| Scene | What it shows |
|---|---|
| **Gallery** | One simple field object per material on a forest floor, under a low golden-hour sun from behind and to one side: a fennec-eared bust and a bat-wing membrane (skin), a 48-leaf shrub (leaves), a big eye in a lidded socket (eyes), a stone slab with hollows (wet), a mossy boulder (layered), a hammered glossy sphere (specular AA). |
| **Creature head** | A close-up of the wolf's head and neck from `experiments/agent-authoring/wolf/creature.wgsl` (copied, trimmed to head, neck and torso), sun behind the head: hide translucency (the ears), refractive eyes. |
| **Tower wall** | A mossy masonry tower rising from damp ground, golden sun raking across the stones: layered stone, wet stone and puddles, specular anti-aliasing. |

## Method

Pure ray marching, as in spike 02: no triangles. Two compute passes per frame, each with its own timestamps:

1. **`visibility`:** sphere-trace the scene's objects; the ground is a heightfield traced with spike 02's directional Lipschitz bound. Writes hit distance, material id and step count. The hit tolerance is a quarter of a pixel's footprint.
2. **`shade`:** per pixel, the field normal (four-tap tetrahedral gradient, step tied to the footprint), base attributes, sun soft shadow, sky light, then the material. Every material is a pipeline-overridable switch, so each variant compiles to its own specialized kernel and plain shading carries none of the material code.

The shrub's 48 leaves sit in a static uniform grid (8 cm cells; each lists the parts within 5 cm of it, so the field stays a bound). Without it, every field evaluation looped over all leaves.

### The materials

1. **Translucency (skin, hide, membranes, leaves).** A medium inside the surface with per-channel extinction σt, scattering σs and a Henyey–Greenstein phase g, plus a diffuse-transmission lobe with its own extinction. Only backlit pixels (n·l < 0) transmit.
   - **Fast path, the inward march:** one field sample at the shaded point, then N samples inward along the light (N = 4 by default). The first sample matters: the hit sits up to the hit tolerance *outside* the surface, which for a 0.5 mm leaf is a large share of its thickness, so the chord starts where f says the surface is. Each step is |f|/|∇f| plus a growing increment, so the march crosses the far side; a secant on f places the exit, and the secant's slope gives the far face's cosine to the light for free. Two details turned out to matter:
     - **|∇f|:** bound fields are often scaled (the leaf is 0.7× an SDF, the wolf 0.9×), so slopes are divided by |∇f|, which the normal's four taps already give (|Σ kᵢ fᵢ| / 4h).
     - **The tent:** along the light, f is tent-shaped, first the distance to the entry face, then to the far face. A secant from a sample still on the entry branch crosses the peak and gets the far face's slope wrong. So the march checks which branch its inside sample is on, and if it's the wrong one, spends a sample just inside the exit predicted by mirroring the entry.
   - **Fast path, the light's way out:** a *transmittance* shadow ray from just past the exit. Where it meets a translucent part (another leaf, the other ear), it measures that part's chord with the same inward march, adds the length and carries on; an opaque part, a thick crossing or 0.25% transmission stops it.
   - **Fast path, the integrals:** single scattering along the view ray has a closed form if the surface is locally a slab: σs·p(θ)·s·(e^(−σt·c) − e^(−σt·s))/(σt·(s − c)), with c the chord and s = c·|n·l|/(n·v). Diffuse transmission is e^(−σm·c)·cos_in·(1 − F)/π.
   - **Reference:** integrate single scattering along the inward view ray numerically, 16 samples at quantiles of e^(−σmin·u) (importance-sampled). Each sample marches toward the sun through the hit part with fine steps (1/40 of the fast path's first step inside), bisecting every boundary it crosses and taking the true normal where the light entered. Then one fine march per pixel, from where the light entered, through everything else; non-translucent parts block it. No slab, no secant, no mirroring.
2. **Eyes.** The eyeball is a sphere with a corneal bulge. On the cornea: Fresnel-weighted sun highlight (GGX, α = 0.03) and sky reflection, then refraction (η = 1.376) through the field normal and an exact hit on the iris dome (a paraboloid: one quadratic), shaded with the iris relief's normal. Plain shading paints the iris onto the outer surface. **Reference:** the refracted ray marches the true domed, ridged iris with fine steps, then bisects. The only difference is the fibre relief's own parallax (1.2% of the eye's radius).
3. **Wet.** Porous darkening, a water film (roughness toward 0.13 on stone, 0.55 on soil), water's F0. Puddles where the Laplacian at puddle scale (a four-tap tetrahedral stencil of radius r, plus f(p)) is concave and the normal faces up; inside a puddle the normal flattens to up and roughness drops to 0.02. **Reference:** the same Laplacian from 64 samples on the same sphere.
4. **Layered stone.** Occlusion from four samples along the normal (log-spaced, 1.5–12 cm); curvature at 4 cm; up-facing from the normal. Moss on up-facing, occluded, concave and shaded areas, broken into patches by filtered noise; grime in crevices; lighter worn edges; lichen spots on exposed faces. The occlusion also dims the sky light. **Reference:** the same occlusion integral with 32 samples, and the same curvature from 64 samples.
5. **Specular anti-aliasing.** Surface detail is a displacement of filtered noise on the shading normal (D-077: octaves fade above Nyquist for the footprint). The faded octaves' slope variance, from the noise's declared mean-square gradient (E|∇n|² = 1.32 per unit frequency, measured once offline for this noise) times amplitude² and frequency², is added to roughness². So is the base surface's curvature times the footprint (from the normal's own four taps plus f(p): one extra evaluation). **Reference:** 4×4 jittered sub-pixel rays per pixel, each marched and shaded with the unfiltered surface, averaged before tonemapping; rendered twice with different jitter to measure its own noise.

### Timing and GPU safety

- **Interleaved:** a view's variants are timed together, 30 warm-up frames across them, then three rounds of 30 back-to-back frames each, medians over the 90. Slow drift (clocks, heat on a fanless laptop) cancels in their differences. Only the shading pass is timed for material comparisons; whole frames are timed with every material on.
- **No submission over ~100 ms** (spikes/README's GPU rules): every pass goes in row bands, one submission each, sized per view by a calibration run so the heaviest band takes about 35 ms; timed pass times sum their bands. References render in adaptive tiles, awaited one at a time, and every reference march has an evaluation budget. The final run's longest timed submission was **32 ms**, its longest reference tile **43 ms**.
- Pipelines compile four at a time.

### Correctness

Each fast variant is grabbed as 8-bit display values and compared with its reference over the full frame and over its own material's pixels (ids from the visibility pass). Step caps are counted for primary rays, inward marches and shadow rays. A full-frame reference (half steps and a fifth of the tolerance for visibility and shadows, plus every material's reference) is compared too, at both resolutions.

### What the compiler would derive, and what's hand-written

The field functions, gradients, the Laplacian stencil, |∇f| and the bandlimit variance are what the compiler would derive from a field (D-077's bandlimits, D-012's derivatives). Here, gradients and the Laplacian are finite differences, and the detail noise has a hand-written analytic gradient; a derived (forward-mode) Hessian may cost more or less. Everything else, the materials, the marches and the lights, is engine code. None of it is engine-specific language (D-050): an inward march, a Laplacian and a footprint make sense in any program that renders a field.

### Changes made after the first measurements

The criteria didn't change except criterion 3's metric (above). The fast paths and references did, and each change is listed here because each one moved the numbers:

1. **Three reference bugs, each of which made the fast path look worse than it was**, found by splitting the error by term and writing the chord and cosine out raw:
   - the reference's per-pixel evaluation budget could run out in the dense shrub, after which every remaining sample transmitted nothing;
   - a hit just outside the surface made the reference take the *front* face as the light's entry (cosine 0);
   - its outside steps (half the thinnest part's thickness, so no part is stepped over) overshot the entry and didn't count the overshoot, shortening every chord by up to 0.25 mm.
2. **The fast path gained** the sample at the shaded point, the |∇f| scaling, the tent check and the transmittance ray (the first version's shadow ray treated every other leaf as opaque, which the reference rightly disagreed with: stacked leaves transmit).
3. **The eye** went from a flat iris plane at half the dome's height (12–18% of eye pixels off by more than 8/255, from parallax on the fine fibres) to the exact dome.
4. **The reference** went from 64 samples marching the whole scene each to the design above, because the first one took 20–37 s per frame with single tiles over a second, which broke the GPU rules.
5. **Timing** went from consecutive to interleaved, and passes were split into bands, after a run's numbers drifted 8–17% late in the run and one shading submission reached 110 ms.
6. **Coverage** is counted per material (the layered material only covers stone and mortar, not the ground).
7. **Looks:** the tower's sun, moss and lichen, and the hide's glow, were tuned between runs.

## Sweeps

| Parameter | Values | Where |
|---|---|---|
| **Screen coverage** of each material | four camera distances per material (three for the eye) | skin: head; leaves: shrub; eyes: gallery eye; wet, layered, spec AA: tower |
| **Inward-march sample count** N | 1, 2, 4, 8, 16 (default 4) | skin on the head; leaves on the shrub (N = 16 checked for correctness only) |
| **Footprint scale** | 0.5, 1, 2, 4 × the pixel footprint used for filtering and variance | spec AA on the wet tower and the gallery |
| **Resolution** | native 1920×1080, and 960×540 internal | every scene (coverage sweeps at 1080p only, for run time) |

Also: a 60 Hz paced run (busy-wait, 90 frames after 30) of the head and the tower with every material on.

## Layout

| File | What |
|---|---|
| `common.wgsl` | Frame uniform, noise with analytic gradients, footprint-filtered fbm and its variance, SDF helpers, sky, environment BRDF, GGX, tonemap |
| `wolf.wgsl` | The wolf's head, neck and torso, copied from `experiments/agent-authoring/wolf/creature.wgsl` (noisy terms are skipped where provably irrelevant; the field is unchanged) |
| `scene_gallery.wgsl`, `scene_head.wgsl`, `scene_tower.wgsl` | Each scene's field, ids, base surfaces, part masks and material hooks |
| `render.wgsl` | Visibility, shading, the materials, their references, the supersampled reference, stats |
| `blit.wgsl` | Copies the frame to the canvas |
| `main.js` | Harness: the shrub's data and grid, pipelines, banded and interleaved timing, sweeps, comparisons, screenshots |
| `results/` | `run-*.json` and JPEG copies of key screenshots (PNGs are ignored) |

## Running it

```bash
python3 spikes/serve.py 8417
spikes/headless.sh 10-materials '#run' 900
```

`#run` measures everything (about 3.8 minutes of GPU time on a cool machine, over the ~3-minute target), saves `results/run-<timestamp>.json` and the screenshots, then `results/DONE`. `#quick` renders three scenes once (about 10 s). Development options: `#quick&shots=scene:view:variant,...`, `&time=1` to time them, `&cmp=scene:view:fast:ref` to compare.

## Results (2026-10-02)

**Setup:** MacBook Air M4 (8-core GPU), Chrome 154 headless (Metal), through `spikes/headless.sh`, which gives one page the GPU at a time. The run is `results/run-2026-10-02T03-40-03-172Z.json`. Earlier development runs were deleted: their references had the bugs listed under "Changes".

**These timings are indicative, not final.** The machine had been under hours of load from nine other spikes, and the reference device is fanless. The previous run, 7 minutes earlier, measured the same unchanged plain-shading passes 19–35% slower. The interleaving keeps each material's *difference* consistent within a run, but the final numbers should come from a run on a cool, quiet machine.

### Verdict

| Material | Extra at representative coverage | Share of slice | Cost | Correct (own pixels) | **Overall** |
|---|---|---|---|---|---|
| Skin and hide | **+7.5 ms** at 25% | 301% | fail | mean 0.46/255, 3.8% over 8 (head); 0.77, 4.2% (gallery) | **Fail** |
| Eyes | **+0.32 ms** at 2% (+0.60 ms at 25%) | 13% | pass | mean 0.20–0.26/255, ≤ 0.2% over 8 | **Pass** |
| Leaves | **+75 ms** at 50% (extrapolated from 36%) | 1890% | fail | close up: 2.0/255, 4.5% over 8; at gallery distance: 15.8/255, 51% | **Fail** |
| Wet | **+1.4 ms** at 50% (interpolated, mostly ground); **~3.7 ms** on stone (an estimate from the tower view) | 46%; ~120% on stone | inconclusive; fail on stone | mean ≤ 0.09/255, ≤ 0.3% over 8 | **Inconclusive on soil, fail on stone** |
| Layered stone | **+10.9 ms** at 50% | 362% | fail | 1.24/255, 3.5% over 8 (tower); 0.87, 1.8% (gallery boulder) | **Fail** |
| Specular AA | **+0.73 ms** at 50% (extrapolated); +1.25 ms at 72% on the tower | 24% | pass, barely | flicker 2% lower than filtering alone (needs ≥ 50%); accuracy worse on the tower | **Not demonstrated:** footprint filtering does the work |

Primary step caps: 0 in the gallery, 0.01% of non-sky pixels on the head, ~0 on the tower. Every material passes that rule.

**The short answer: the quantities work, the cost doesn't, in this content.** Every one of them is accurate enough to build convincing materials on (wet and eyes match their references almost exactly; skin and leaves come within 0.5 and 2/255 on average). But each costs a handful of extra field evaluations per pixel, and *these* fields cost about a nanosecond per evaluation per pixel at 1080p. That's ~2 ms for every full-screen evaluation, and the slices don't have room.

### What the cost is made of

At each scene's main view, 1080p. Evaluations are counted by the kernel; ns per evaluation is the measured extra time divided by them.

| Material | Extra field evaluations per material pixel | Extra | ns per extra evaluation | To pass at its representative coverage, an evaluation must cost (estimate) |
|---|---|---|---|---|
| Layered (tower, 69%) | **9:** 4 occlusion, 4 + 1 curvature | +12.1 ms | 0.94 | ≤ 0.08 ns, **12× cheaper** |
| Wet (tower, 72%) | **5:** curvature at puddle scale | +5.3 ms | 0.71 | ≤ 0.14 ns, **5× cheaper** |
| Spec AA (tower, 72%) | **1:** f(p) for the curvature term | +1.25 ms | 0.83 | ≤ 0.72 ns, about what it costs |
| Skin (head, 29%; 28% backlit) | **5 inward** per backlit pixel, plus a transmittance ray (6.9 extra shadow steps per ray, and its crossings' inward marches) | +8.1 ms, of which the inward march is +2.8 ms and the ray +5.2 ms | 1.1 | march alone: ≤ 0.25 ns, **4× cheaper**; and the ray has to go |
| Leaves (shrub close-up, 21%) | **5 inward** (one leaf's field only) plus a transmittance ray through the shrub | +34 to +37 ms (two measurements), of which the inward march is **+1.0 ms** | 0.5 for the leaf's own field | march alone: ≤ 0.2 ns, 2.5× cheaper; the ray needs a different design |
| Eyes | **0:** a refraction, one quadratic, two relief lookups | +0.3 ms at 2% | — | already passes |

- **The thickness itself is cheap; the light's way out is not.** For leaves, the inward march through the one leaf that was hit (the field's part mask, as in spike 02) costs 1.0 ms at 21% coverage. The transmittance ray, which has to find and cross every other leaf between the far face and the sun, costs more than 30 times that in a dense shrub. That ray is sun visibility for back faces: in an engine it's the lighting slice's job, and a shadow map can't see through leaves unless it's an opacity or deep shadow map.
- **Per-evaluation cost is the lever,** and it's set by the content. The wolf is an agent-authored field with fbm fur, ruff and mane noise; the tower's stones carry fbm displacement. Spike 02's posed grazer evaluated 1.7 parts per step. Even plain shading and visibility are far over budget here: the head's visibility pass alone is 19 ms (the creature slice is 2.5), the tower's plain shading 24 ms. That matches what the other spikes found: a traced, detailed creature filling the screen costs many times its slice.
- **What would change the verdicts** (hypotheses, untested here): evaluate the material queries on a cheaper version of the field. Occlusion at 1.5–12 cm and curvature at 4–30 cm don't need millimetre noise, so octaves above the query's scale can be dropped, which is the same bandlimit fact D-077 uses for the footprint (an estimate: 30–50% cheaper on the tower, not 12×). Or read them from a cooked distance cache or baked masks, the fallback, which turns each evaluation into a texture fetch.

### Coverage sweeps (1080p, extra ms on the shading pass)

| Material | Coverage → extra |
|---|---|
| Skin (head) | 4% → 1.2 · 13% → 3.6 · 26% → 7.9 · 43% → 14.0 |
| Leaves (shrub) | 1.6% → 5.3 · 5.9% → 12.3 · 21% → 37.4 · 36% → 56.8 |
| Eyes (gallery eye) | 1.7% → 0.3 · 7% → 0.3 · 31% → 0.7 |
| Wet (tower; % of wet pixels) | 53% → 2.0 · 77% → 2.9 · 74% → 6.3 · 84% → 6.9 |
| Layered (tower; % of stone) | 13% → 3.0 · 21% → 4.7 · 71% → 15.3 · 82% → 18.1 |
| Spec AA (tower; % non-sky) | 53% → 0.9 · 77% → 0.7 · 74% → 1.8 · 84% → 3.3 |

- **Cost per covered pixel depends on what's under it.** The two far tower views are mostly ground, where a field evaluation is a cheap heightfield; the near ones are mostly wall. That's why wet costs 2.9 ms at 77% coverage and 6.3 ms at 74%. The representative-coverage rule interpolates among the ground-heavy views and gives wet +1.4 ms (inconclusive); the tower's own main view gives about +3.7 ms at 50% (an estimate, fail).
- **Spec AA's sweep is noisy** (1–3 ms swings between nearby views); its one evaluation per pixel predicts about 0.9 ms at 50%.
- **Leaves don't scale linearly with coverage:** the distant views pay a long transmittance ray per small leaf.

### Correctness

On the material's own pixels, against its reference (1080p):

| Scene | Material | Pixels | Mean \|diff\| /255 | Over 8/255 |
|---|---|---|---|---|
| Head | Skin (hide) | 608 K | 0.46 | 3.8% |
| Gallery | Skin (bust, membrane, socket) | 50 K | 0.77 | 4.2% |
| Shrub close-up | Leaves | 436 K | 2.03 | 4.5% |
| Gallery (distant shrub) | Leaves | 8 K | 15.8 | 51% |
| Gallery | Eye | 1.6 K | 0.26 | 0.19% |
| Eye close-up | Eye | 646 K | 0.24 | 0.11% |
| Head | Eyes | 1.1 K | 0.20 | 0% |
| Gallery | Wet slab | 40 K | 0.09 | 0.28% |
| Tower | Wet | 1.5 M | 0.006 | 0.01% |
| Gallery | Layered boulder | 39 K | 0.87 | 1.8% |
| Tower | Layered | 1.4 M | 1.24 | 3.5% |

Whole frames against the full reference (brute-force visibility and shadows too): gallery 0.32/255 and 0.76% over 8 at 1080p (0.44, 1.1% at 540p); head 0.23, 1.3% (0.30, 1.4%); tower 1.36, 4.0% (1.81, 5.2%). The tower's full-frame error is the layered material's.

- **Skin and leaves miss 2% by a little, and more samples don't fix it** (below). The rest is the slab assumption for single scattering, mirrored far faces where the tent check can't place a sample, and thin convex edges.
- **Thin convex ridges glow.** A medium whose red mean free path is several millimetres really does pass light through the thin edge of a convex surface, so the muzzle's top ridge shows a faint red line, in the reference as well. Real tissue spreads that light out (diffusion); this model doesn't.
- **Leaves at gallery distance don't work** (51% off): the leaf is 0.5 mm thick and the hit tolerance there (a quarter pixel at 5.4 m) is about 0.9 mm, so the hit point says little about where the leaf is, and the fast path and reference both guess. Distant foliage needs an aggregate model; that's spike 03's ground.
- **Layered misses 2%** because four occlusion samples and a four-tap Laplacian put the moss and grime masks' edges in slightly different places than 32 and 64 samples do. The moss isn't wrong-looking, it's differently placed.

### Inward-march samples

| N | Head skin: extra ms | mean /255 | over 8 | Shrub leaves: extra ms | mean /255 | over 8 |
|---|---|---|---|---|---|---|
| 1 | +3.9 | 0.94 | 5.9% | +19.2 | 16.0 | 69% |
| 2 | +5.4 | 0.73 | 4.5% | +27.1 | 7.8 | 47% |
| **4** | +8.1 | 0.46 | 3.8% | +33.9 | 2.03 | 4.5% |
| 8 | +7.7 | 0.36 | 3.2% | +33.6 | 2.03 | 4.4% |
| 16 | +8.4 | 0.36 | 3.2% | not timed | 2.03 | 4.4% |

Times include the transmittance ray (whose crossings use the same N, so fewer samples also make the ray cheaper). Without the ray, N = 4 costs +2.8 ms on the head and +1.0 ms on the shrub. **Leaves need N = 4** (the tent check needs a sample to spend); skin converges by N = 8. Neither reaches 2% at any N.

### Specular anti-aliasing

Wet tower and gallery, slow sideways camera motion (0.8 mm per frame, about a quarter of a pixel at the wall), non-sky pixels:

| Variant | Tower flicker /255 | Tower vs reference /255 | Gallery flicker | Gallery vs reference |
|---|---|---|---|---|
| Unfiltered detail (naive) | 2.14 | 3.13 | 0.45 | 3.15 |
| Footprint-filtered (D-077) | 0.55 | 3.41 | 0.28 | 1.99 |
| Filtered + roughness widening | 0.54 | 4.34 | 0.28 | 2.00 |
| … at footprint × 0.5 / 2 / 4 | 0.74 / 0.46 / 0.41 | 3.52 / 5.29 / 6.37 | 0.34 / 0.23 / 0.18 | 2.47 / 2.68 / 3.53 |
| Supersampled reference (16 spp) | 0.95 | its own noise: 0.75 | 0.26 | 0.65 |

- **Filtering the detail octaves by footprint removes the shimmer:** flicker drops 75% on the tower and 38% in the gallery against the naive surface.
- **Widening roughness by the faded octaves' variance adds almost nothing to stability** (2% and 0%), so it fails criterion 3. Its job is the look at distance (highlights keep their energy instead of turning too glossy), not stability. On the tower it's also *less* accurate than filtering alone: brighter, probably because this spike's crude blurred-sky reflection isn't energy-consistent across roughness (a hypothesis).
- **The reference is too noisy to referee differences below about 0.75/255,** and its flicker exceeds the filtered variants'. A stronger test needs 64+ samples per pixel or temporal accumulation in the reference.
- **Footprint scale** trades flicker for blur as expected. Cost barely moves: every material on the tower costs +19.0, 19.6, 18.6 and 17.8 ms at 0.5, 1, 2 and 4× (coarser footprints evaluate fewer octaves).

### Resolution, whole frames and pacing

| Scene | Visibility | Plain shading | All materials | Whole frame (all on) | Same at 960×540 |
|---|---|---|---|---|---|
| Gallery | 6.3 ms | 16.6 | 21.2 (+4.6) | 27.5 | 7.7 (materials +1.4) |
| Head | 19.3 | 5.6 | 14.3 (+8.7) | 33.6 | 9.4 (+2.8) |
| Tower | 14.0 | 23.7 | 42.3 (+18.6) | 56.3 | 14.0 (+4.7) |

- **At 960×540 the materials cost about a quarter to a third** of their 1080p cost, as expected for per-pixel work.
- **60 Hz paced:** head 45.7 ms median (p95 54.5), tower 63.2 ms (p95 82.2); all 90 frames over 16.7 ms. These scenes don't fit a frame even without materials: the fields are too expensive to trace and shade at 1080p as written.

### Other costs

- **Pipelines:** 34 (gallery), 22 (head) and 19 (tower) variants, compiled cold four at a time in 3.4, 1.7 and 1.3 s. A real material system would compile far fewer: most of these are measurement variants.
- **Memory:** a 33 MB visibility buffer (RGBA32F; a real G-buffer would be smaller) and a 16 MB RGBA16F frame. The shrub's leaves, twigs and grid take 0.1 MB.
- **Run time:** the full run took 3.8 minutes of GPU time, over the ~3-minute target, because these scenes' frames are 25–60 ms and every measurement is 120 of them.

### How it looks

Honestly, in order of strength:

- **Backlit leaves are the best thing here:** yellow-green glow, darker veins and midrib where the leaf is thicker, and translucent shadows where one leaf lies over another (`results/cover-leaf3-all.jpg`). Front-lit, they're flat plastic green.
- **The eyes read as eyes:** refraction, iris depth and a limbal ring that shifts correctly as the view changes (`results/cover-eye3-all.jpg`). There's no visible sun glint in these views, because the sun is behind the eye.
- **The wet slab and wet tower** look like rain: darkened stone, a water film's sheen, puddles that reflect the sky (`results/tower-all.jpg`).
- **The tower** is pleasant but clean-CG: warm raking light, moss in damp joints and near the base, edge wear, some lichen (`results/tower-dry.jpg`). The masonry is too regular, and the lichen reads a little like stains.
- **Skin** is the weakest. The fennec ears and bat wing glow convincingly in the gallery. On the wolf, the ear glow is subtle and uniform, the faint red ridge lines are visible, and the wolf itself looks like grey plastic: its fur is geometric noise, not fur.
- **The scenes are hazy golden-hour plains** with a painted treeline, not forests. Good enough to judge materials; not the flagship's look.

### What surprised me

1. **The references were wrong three times before the fast path was.** Each bug pushed the comparison against the fast path, and only splitting the error by term (single scattering against diffuse, then chord against cosine against entry offset) found them. A brute-force reference is code too; it needs the same scrutiny.
2. **The hit point isn't on the surface.** A quarter-pixel hit tolerance is a large share of a thin part's thickness, so an inward march has to measure where the surface is before it starts. The same issue would arise with rasterized meshes (below).
3. **Fields are rarely unit-gradient,** and a march that reads slopes as cosines has to divide by |∇f|, which the normal's taps give for free.
4. **For thin parts, visibility of the light dominates,** not thickness: the transmittance ray costs more than 30× the inward march in a dense shrub.
5. **Roughness widening doesn't stop shimmer here; filtering does.**
6. **Cost per covered pixel depends on the field under it,** which makes "cost at coverage" a weaker criterion than "evaluations per pixel × cost per evaluation".
7. **The GPU rules shaped the method.** The first reference took 37 s per frame and a single 1-row tile ran 1.5 s. Banding, budgets and calibration were needed before anything could be measured safely.

### Caveats

- **Indicative timings** (above): the final run should be repeated on a cool, quiet machine.
- **Content-specific:** the per-evaluation cost (0.7–1.1 ns) is this content's. A cheaper or cached field changes every cost verdict, and the eval counts are what transfer.
- **Finite differences stand in for derived derivatives.** A compiler-derived Hessian could cost more or less than the five-tap stencil.
- **The translucency model is single scattering plus a diffuse-transmission lobe,** not diffusion. Its references check the model's numerics, not that skin looks like skin.
- **One browser, one device:** Chrome 154 on the M4. Not Safari, not Firefox, not the secondary devices.
- **The environment lighting is a crude analytic sky,** with no reflections of the scene and no sky occlusion beyond the layered material's own.

### What this means for the design

Nothing here is recorded in `decisions.md`; that's the owner's call.

- **Field-derived material quantities are accurate and cheap to *write*:** the masks, thickness and refraction all came from a few queries, with no textures and no UVs, and every material but specular AA's widening did what it was for.
- **Their cost is (extra evaluations per pixel) × (cost per evaluation),** and with analytic, noise-rich fields the second factor is too high. That points at a field-query cache, or baked masks per brick, for anything shaded at full screen: occlusion and curvature at centimetre scales are low-frequency and cache well (a hypothesis). Thickness of thin parts needs the field itself.
- **Bandlimit facts (D-077) earn their keep twice:** they stop shimmer, and they could tell a query at a coarse scale which octaves to skip (untested).
- **Sun visibility for back faces belongs to the lighting slice,** and needs a representation that sees through leaves (opacity or deep shadow maps, or cone tracing through a translucency-aware cache).
- **Eyes and wet are ready;** specular AA's widening isn't needed for stability.

### If the surface is rasterized

The other spikes point to a hybrid: rasterize what's large on screen, and use fields for lighting, distance, small instances and cooking. **Every material here survives that,** because each one only queries the field at the shaded point; none needs the primary ray to have been marched.
- **Thickness:** works as is. The shaded point then comes from a mesh, which sits off the field's zero set by the mesh's error (millimetres up close, centimetres at coarse LODs), exactly the hit-tolerance problem found here. The sample at the shaded point already handles it, as long as the mesh error is smaller than the thin parts (it isn't for leaves on a coarse mesh).
- **Curvature:** the puddle-scale and moss-scale Laplacians still need field taps. Spec AA's curvature term could come from screen-space derivatives of the normal (as in Kaplanyan et al. 2016), saving its one evaluation.
- **Occlusion:** unchanged; it doesn't care how the surface was found.
- **Footprint:** from screen derivatives of position instead of distance × pixel angle; the bandlimit variance is unchanged.
- **Eyes:** unchanged; the cornea's normal comes from the field at the rasterized point.
- **What changes is the budget, not the method.** Rasterizing removes the visibility pass, the biggest single cost here (19 ms on the head). The materials' own costs stay the same, so they become a larger share of a smaller frame. The transmittance ray would become a lookup in whatever the lighting system provides, which must see through leaves.

## Quiet-machine rerun (2026-10-02)

Run: `results/run-2026-10-02T04-18-43-963Z.json` (serial, after a cool-down, Chrome 154 headless, alone on the GPU). Errors are own-pixel mean /255 and share over 8/255.

| Metric | README value | Quiet value |
|---|---|---|
| Skin: extra at 25%, share; error (head; gallery) | +7.5 ms, 301%; 0.46, 3.8%; 0.77, 4.2% | +6.66 ms, 267%; 0.46, 3.8%; 0.77, 4.2% |
| Eyes: extra at 2% (at 25%), share; error (gallery; close-up; head) | +0.32 ms (+0.60), 13%; 0.26, 0.19%; 0.24, 0.11%; 0.20, 0% | −0.06 ms (+0.36), −2%; 0.26, 0.19%; 0.24, 0.11%; 0.20, 0% |
| Leaves: extra at 50% (extrapolated), share; error (close-up; gallery) | +75 ms, 1890%; 2.03, 4.5%; 15.8, 51% | +67.8 ms, 1696%; 2.03, 4.5%; 15.8, 51% |
| Wet: extra at 50% (extrapolated), share; on stone; error (gallery; tower) | +1.4 ms, 46%; ~3.7 ms, ~120% (estimate); 0.09, 0.28%; 0.006, 0.01% | +1.05 ms, 35%; stone estimate not in the JSON (its tower main view: +5.24 ms at 72%, so ~3.6 ms, ~121% scaled the same way); 0.09, 0.28%; 0.006, 0.01% |
| Layered: extra at 50%, share; error (tower; gallery) | +10.9 ms, 362%; 1.24, 3.5%; 0.87, 1.8% | +8.83 ms, 294%; 1.24, 3.5%; 0.87, 1.8% |
| Spec AA: extra at 50% (extrapolated), share; tower main view at 72% | +0.73 ms, 24%; +1.25 ms | +0.26 ms, 9%; +1.25 ms |
| Spec AA flicker /255, filtered → widened (tower; gallery) | 0.55 → 0.54, 2% lower; 0.28 → 0.28, 0% | 0.555 → 0.544, 2.0% lower; 0.282 → 0.281, 0.1% |
| Spec AA vs 16-spp reference /255, filtered → widened (tower; gallery) | 3.41 → 4.34; 1.99 → 2.00 | 3.41 → 4.34; 1.99 → 2.00 |
| Visibility pass (gallery; head; tower) | 6.3; 19.3; 14.0 ms | 6.29; 19.27; 13.96 ms |
| Primary step caps (gallery; head; tower) | 0; 0.01%; ~0 | 0; 103 px, 0.01%; 3 px |

- Correctness and flicker match to every printed digit (the rendering is deterministic), and main-view timings match within one or two timestamp ticks (65.5 µs). Only the coverage sweep, run late, moved: 0–24% lower for skin, leaves, wet and layered, which sets the representative-coverage values. The eye's 2% point is one tick below zero, so its sign is noise.
- **Verdicts: none change** under the README's criteria. Skin, leaves and layered still fail; eyes still pass; wet stays inconclusive on soil (35%) and fails on stone; spec AA's cost now passes clearly (9%), but widening still cuts flicker by 2%, not ≥ 50%, so it stays not demonstrated.
