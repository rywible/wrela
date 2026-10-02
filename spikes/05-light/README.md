# Spike 05: light from fields

*A measurement spike for the flagship's lighting slice. It's throwaway code, not the start of the engine.*

## The question

Can the world's fields give **soft sun shadows, sky occlusion and one bounce of diffuse light** inside the lighting slice of the frame budget, **3.5 ms at 1920×1080** on the reference device (MacBook Air M4, 8-core GPU, Chrome, WebGPU)? That includes **indoors**, where distance-field global illumination tends to leak light through walls.

The renderer is pure ray marching: no triangles anywhere. The same field that's traced for visibility is traced for light.

## Scenes

| Scene | What it tests |
|---|---|
| **Forest floor** under a canopy | Dappled sunlight through gaps in the canopy, sky light filtered by foliage, green bounce. Simple trees: trunks and canopies of displaced blobs (spike 03 does real foliage). |
| **Inside a stone tower** | Sunlight through deep windows onto the floor, bounce light filling the room, and *no light leaking through the 0.9 m walls* while the outside of those walls is in full sun. This is the hard case. |
| **Open landscape at golden hour** | Long soft shadows from a low sun, sky occlusion in valleys, distances to ~2 km. |

## Kill criteria, written before measuring

**Lighting time** is the GPU time of every lighting pass at native 1920×1080, from timestamp queries under sustained load (median of 90 back-to-back frames after 30 warm-up frames):

> lighting = sun shadows + sky occlusion + temporal accumulation + probe tracing + probe update + probe gather + composite

Primary visibility (the G-buffer trace) isn't lighting: it belongs to the geometry and vegetation slices. It's reported separately. Load-time work (cooking a lighting volume, placing probes, baking the fallback's probes) is reported separately too; anything that would have to be redone every frame for a moving world counts against the slice.

1. **Budget** (spikes/README's verdict rule), per scene:

   | Lighting time | Verdict |
   |---|---|
   | ≤ 3.5 ms | **Pass** |
   | 3.5–7 ms | **Inconclusive** |
   | > 7 ms | **Fail**: use the fallback |

2. **Correct, not just fast.** The configuration judged against the budget must also match a **path-traced reference** of the same scene and the same light transport (sun disc, sky, one diffuse bounce), rendered in the same page with hundreds of samples per pixel:
   - mean |difference| **≤ 3/255**, and
   - **≤ 10%** of pixels off by more than 8/255,

   on the final tonemapped 8-bit image. These thresholds are looser than spike 02's (0.5/255 and 0.5%), on purpose: spike 02 checked geometry against geometry, which can be exact. Every real-time GI method approximates the integral, and the question is whether the approximation is close enough to look right. They are this spike's judgment, set before measuring; the owner may prefer others. The reference's own noise floor is measured (two independent halves of its samples, compared) and reported next to every number.

3. **No leaks indoors.** In the tower, over interior pixels that aren't directly sunlit, the real-time image's mean linear luminance must be **≤ 1.25×** the reference's. A leak makes walls glow; this catches it. A deliberately leaky variant (probe gather without visibility) is run to show the check detects leaks.

4. **Which configuration is judged:** per scene, the **cheapest swept configuration that meets criterion 2** (and 3, indoors). If none does, the scene fails on correctness whatever its time.

5. **Looks.** Screenshots of every scene, real-time and reference side by side, judged honestly. Beauty is part of the question, so this is reported, not scored.

*Changed during development, before the final runs (see "Changes made during development"):* criterion 2 is evaluated on a uniform 1-in-16 sample of the pixels (the reference's indirect grid), because a per-pixel reference of the whole image converged enough to judge was not affordable. Criterion 4 is reported twice, once for the technique under test (per-frame lighting) and once for the fallback (baked probes), because the sweep includes fallback configurations and they shouldn't make the technique pass.

## The fallback, if it fails

Bake irradiance probes from the fields on the device at load ("cook on device"), and trace only sun shadows per frame. The spike measures the fallback too: it's the same pipeline with the probes baked once with many rays and never updated, sky light taken from the probes, and only the sun traced per pixel.

## Method

Everything is compute shaders on one WebGPU device. The lighting passes take a G-buffer (depth, normal, albedo, material) and nothing else. Per frame:

1. **Primary** (not lighting): sphere-trace the scene's analytic field per pixel. Terrain is a heightfield traced with a directional Lipschitz bound (spike 02's trick), objects are sphere-traced, and one marcher steps by the smaller of the two safe steps. Two submissions (top and bottom half).
2. **Sun: distance-field cone tracing** per pixel (full or half resolution). One ray toward the sun's centre; the cone's half-angle is the sun disc's angular radius. The penumbra tracks the smallest `h / (t·tan θ)` along the ray, *including negative values inside occluders*, so it straddles the geometric shadow edge as an area light's does (Quílez's and Aaltonen's improved soft shadows). Terrain sun shadows come from a cooked **shadow-height map** (below), so the ray only has to look for objects, and it stops as soon as it is higher than the object layer above everything toward the sun.
3. **Sky light**, three ways (a sweep):
   - *cones:* K distance-field cones per pixel, cosine-distributed, each a 1/K share of the hemisphere, length L, **one-sided** (a cone whose axis touches a surface counts as blocked, however thin the surface), divided by what the tangent plane alone would block. Rotated per pixel per frame, half resolution, accumulated temporally.
   - *probes:* sky irradiance from the probe cache (next item).
   - *probes × cone AO:* probe sky and bounce, multiplied by the visibility of K short cones (length 1–8 m) for contact occlusion, as DDGI + DFAO engines do. This double-counts some occlusion, as theirs does.
4. **One diffuse bounce: a probe cache traced with distance-field visibility** (DDGI-style):
   - Probes on a regular grid. At load each probe is pushed out of nearby geometry along the field's gradient (the field makes relocation trivial); probes still inside, or too far from any surface to be interpolated (> 2.1 cells), are switched off.
   - Each frame every active probe traces R rays (a spherical Fibonacci set under a random rotation per frame). At a hit, outgoing radiance is `albedo/π × (sun, with a cone-traced shadow, + sky)`, where the hit's sky light comes from a second probe set that stores *sky-only* irradiance, fed by the escaped rays. That is exactly one bounce, with no multi-bounce feedback, so it matches the reference's light transport.
   - Irradiance in 6×6 octahedral maps, distance moments in 14×14, as in DDGI. Updates blend with history.
   - The gather weights the 8 surrounding probes by trilinear position, a backface term and a Chebyshev visibility test on the distance moments. It runs at half resolution and is upsampled (or at full resolution, or inline in the composite: sweep points).
5. **Temporal accumulation:** sky (or AO) is reprojected and blended with history (history length H, depth-checked disocclusion). Probes blend with the same H.
6. **Composite:** `albedo/π × (sun + sky + bounce)`, fog, tonemap. One set of depth- and normal-aware upsampling weights serves every half-resolution input.

**What lighting rays trace** (a sweep): the **analytic** field (the authored one, as primary visibility uses), the **cooked** field, or a **hybrid**: analytic for the first stretch of each ray (2.5 fine voxels for sun rays, 1 for sky cones), cooked beyond. The cooked field is made on device at load from the analytic one:
- a **height map** of the terrain with its gradient (2D, so terrain keeps the directional step), 0.125–4 m texels;
- a **two-level clipmap of the objects' distances** (3D, `r16float`): 0.1 / 0.5 m (tower), 0.25 / 1 m (forest), 4 / 16 m (landscape);
- a **shadow-height map** for the current sun: for every height-map texel, the height a point must clear to see the sun over the terrain, and the distance to the occluder (which sets the penumbra). Re-cooked when the sun moves.

**Reference:** a progressive path tracer in the same page on the analytic field, from the same primary hits, with the same light transport. It has two estimators:
- **Sun at every pixel:** next-event estimation over the disc with hard shadow rays, stratified (R2).
- **Sky + bounce on a 1-in-16 pixel grid:** one cosine-sampled ray that adds sky radiance if it escapes or, if it hits, the hit's direct sun and sky (one more disc sample, one more sky ray), times its albedo.

Rays are sphere-traced with half-size steps, a twentieth of the real-time hit tolerance, a doubled terrain slope bound and step caps of 2000. In empty space the marcher may step by the cooked clipmap's distance minus a provable margin (trilinear interpolation of a Lipschitz-L field is within L·√3·voxel of it); hits are only ever decided by the analytic field, so this changes the cost, not the answer. Samples alternate between two accumulators, so the reference's own noise is measured. For display, the grid's indirect light is reconstructed at the other pixels with the same depth/normal-aware filter (the side-by-side images); **the metrics use only the grid pixels**, where the reference is a direct estimate.

**Correctness numbers** follow spike 02, on the grid pixels of the final tonemapped image: mean |difference| out of 255 and the share of pixels whose largest channel difference exceeds 8/255. Also reported:
- an **unbiased RMS error**, mean over pixels of (x − A)(x − B) where A and B are the reference's independent halves, which is free of the reference's noise in expectation;
- the **sun term at every pixel** (sun-only views compared);
- **step-cap hits** for every kind of ray;
- in the tower, the **leak ratio**: mean luminance of interior pixels the sun doesn't reach, real-time ÷ reference.

**GPU safety** (spikes/README): one pass per submission, the primary and sun passes in two bands, the reference in tiles of one sample pair (adapting toward 40 ms) with `onSubmittedWorkDone` between submissions, at most four pipelines compiling at once. The longest submissions in the final run were the landscape's primary halves (~65 ms each); every reference tile was under 45 ms.

## Sweeps (as run)

| Parameter | Values |
|---|---|
| Sky cones: count K | 1, 2, 4 (8 in the previous run) |
| Sky cones: length L | short, base, long: forest 4 / 12 / 32 m, tower 2 / 6 / 16 m, landscape 40 / 160 / 600 m |
| Sky source | per-pixel cones; probes; probes × short cone AO (2 / 1 / 8 m) |
| Cache resolution | probe spacing fine / base / coarse (forest 2 / 3 / 4 m, tower 0.5 / 0.75 / 1.5 m, landscape — / 48 / 64 m); rays per probe 16 (in combinations), 32, 64 |
| Temporal history H | 1, 16, 64 |
| Probe visibility | DDGI (backface + Chebyshev + relocation); none, all probes on (tower) |
| Lighting field | hybrid (base), all cooked, all analytic (forest, tower); sun near-zone 0; terrain sun shadows marched instead of mapped (landscape) |
| Resolution | sun full / half; 1920×1080 and 960×540 |
| Combinations | `fast`: all cooked, half-res sun, probes × AO (K = 2), R = 16; `fast_fullsun`: same with full-res sun |
| Fallback | `fallback`: probes baked at load (48 frames × 64 rays, analytic), sky from probes, hybrid sun; `fallback_fast`: same with cooked half-res sun |
| Drift check | `base_again`: base, measured again at the end of each scene's sweep |

Base is: hybrid field, full-res sun, K = 4 cones at half resolution with base length, probes at base spacing with R = 32, H = 16, gather at half resolution.

## Layout

| File | What |
|---|---|
| `common.wgsl` | Frame uniform, hashing and noise, sky, octahedral maps, tonemap |
| `scenes.wgsl` | The three scene fields, materials, cooked-field lookups, the marchers (primary, cone, probe ray, reference) |
| `light.wgsl` | Every per-frame pass, cooking (height map, shadow-height map, clipmap), probe placement, debug views |
| `ref.wgsl` | The path-traced reference (sun and grid estimators) and its resolve |
| `blit.wgsl` | Copies the frame to the canvas (not measured) |
| `main.js` | Harness: scenes, pipelines, sweeps, timing, reference, metrics, screenshots |
| `results/` | Run JSON, JPEG copies of key screenshots (PNGs are git-ignored) |

## Running it

```bash
python3 spikes/serve.py 8417
spikes/headless.sh 05-light '#run' 900
```

- **`#run`** takes about 4½ minutes, longer than the ~3-minute target. Many swept configurations are deliberately far over budget (the analytic field is 24–77 ms of lighting per frame), each is measured with the protocol's 120 frames, and the references take ~30 s at 1080p.
- **`#quick`** (~20 s) renders each scene with a 2-pair reference and the base and `fast` configurations.
- **`#look=forest,tower&cfg=fast&debug=albedo,normal,sunvis`** is a development view: one scene and configuration, its components and debug views, no metrics.

## Results (2026-10-01)

**Setup:**
- The reference device: MacBook Air M4 (8-core GPU), headless Chrome 154 stable, WebGPU, `r16float` clipmaps.
- The final run is `results/run-2026-10-02T02-47-40-876Z.json`; an earlier full run, `run-2026-10-02T02-36-40-072Z.json`, used the same code except for the sweep list and the verdict format, and agrees.
- Other spikes were being built on the machine. The GPU lock in `headless.sh` meant no other page shared the GPU during these runs, but clocks and thermals weren't controlled, and timings moved 10–20% between the two runs.
- **All timings here are indicative, not final.**

### Verdict

Lighting time is the median at 1920×1080. "Meets bar" is criteria 2 and 3.

| Scene | Technique (per-frame lighting): cheapest config meeting the bar | Verdict | Fallback (baked probes): cheapest meeting the bar | Verdict |
|---|---|---|---|---|
| Forest | `fast_fullsun`, **11.3 ms** (`fast`, 8.0 ms, misses: 11.95% of pixels > 8/255; in the earlier run 10.01%) | **Fail** (> 7 ms) | `fallback_fast`, **3.0 ms** (3.5 ms in the earlier run) | **Pass**, at the edge |
| Tower | none | **Fail on correctness** | none | **Fail on correctness** |
| Landscape | `fast`, **5.0 ms** | **Inconclusive** | `fallback_fast`, **2.4 ms** | **Pass** |

- **The technique as written doesn't fit the slice.** At its base configuration it costs 13.7 ms (tower) to 32 ms (forest).
- **The cheapest per-frame combination that is close to right costs 5–8 ms at 1080p.** That's all-cooked fields, a half-resolution sun, and probe sky with contact cones.
- **At 960×540,** the same combination is **2.4–4.8 ms** (below). That's the lever a temporal upscaler would use, but this spike doesn't implement upscaling, so it doesn't claim upscaled quality.
- **The fallback fits outdoors at 1080p:** 2.4–3.0 ms, with the sun traced per frame at half resolution and probes baked at load.
- **Indoors nothing meets the bar,** for two reasons, below.

### Where lighting time goes

GPU ms per pass at 1080p (final run). Every configuration listed gathers the probes at half resolution in its own pass; "probe update" is the irradiance and distance-moment passes together.

| Scene, config | Sun | Sky / AO | Temporal | Probe trace | Probe update | Gather | Composite | **Lighting** |
|---|---|---|---|---|---|---|---|---|
| Forest, base | 10.03 | 11.93 | 0.52 | 6.36 | 1.05 | 0.66 | 1.57 | **32.3** |
| Forest, all analytic | 27.00 | 26.87 | 0.33 | 20.12 | 0.79 | 0.46 | 1.25 | **76.9** |
| Forest, all cooked | 4.33 | 5.37 | 0.46 | 4.52 | 1.05 | 0.59 | 1.57 | **18.1** |
| Forest, `fast` | 1.51 | 0.98 | 0.39 | 2.16 | 0.59 | 0.59 | 1.57 | **8.0** |
| Forest, `fallback_fast` | 1.31 | — | — | — | — | 0.52 | 1.25 | **3.0** |
| Tower, base | 3.28 | 5.90 | 0.46 | 1.38 | 0.72 | 0.59 | 1.64 | **13.7** |
| Tower, `fast` | 0.92 | 1.18 | 0.39 | 0.79 | 0.46 | 0.66 | 1.83 | **6.2** |
| Tower, `fallback_fast` | 0.79 | — | — | — | — | 0.59 | 1.57 | **3.0** |
| Landscape, base | 15.01 | 9.50 | 0.33 | 3.74 | 0.65 | 0.46 | 0.98 | **30.9** |
| Landscape, `fast` | 1.11 | 0.33 | 0.26 | 1.51 | 0.46 | 0.46 | 1.05 | **5.0** |
| Landscape, `fallback_fast` | 1.05 | — | — | — | — | 0.39 | 0.92 | **2.4** |

What drives these costs:

- **Cost is rays × steps × cost per step.**
  - An analytic step costs **0.5–1.2 ns**, a cooked step about **0.2 ns**: from the sun passes, forest 27.0 ms for 0.53 M rays × 42.5 steps against 4.3 ms for 0.53 M × 38.9.
  - The forest's field is roughly 1000 operations a step (an estimate from its code: four tree cells, twelve-blob crowns, value noise), and real foliage (spike 03) will cost more. **Lighting rays can't trace the authored field; they need a cooked one.**
- **The hybrid's analytic near zone costs a lot for little.**
  - Sun rays, forest: 10.0 ms hybrid against 4.3 ms cooked, for 2.16% against 2.19% of pixels over 8/255 in the sun term.
  - Sky cones: forest 10.0 against 5.4 ms (earlier run), no measurable quality change.
- **Full-resolution sun is the biggest single cost.** At half resolution with depth- and normal-aware upsampling it is about 3× cheaper, for up to 0.2 more points of sun pixels over 8/255.
- **The fixed passes add up to a floor of ~1.8–2.9 ms at 1080p:** composite plus gather (bandwidth-bound) plus the temporal pass. In `fast` they are a third to a half of the total.
- **Primary visibility is not lighting, but it isn't in budget either:** forest 97 ms, tower 16 ms, landscape 134 ms at 1080p (20 / 4 / 38 ms at 540p). This agrees with spikes 02–04 that pure ray-marched primary visibility over a rich world is out of reach.

### Correctness

Final image on the grid pixels (129,600 at 1080p). "Floor" is the reference's own noise: half A against half B, each half the samples. The full reference is about half as noisy as either half.

| Scene, config | Mean \|diff\| /255 | > 8/255 | Unbiased RMSE /255 | Sun term > 8/255 (every pixel) | Leak ratio |
|---|---|---|---|---|---|
| Forest floor (A vs B) | 3.20 | 16.9% | — | 0.59% | |
| Forest, base (cone sky) | 6.01 | 35.7% | 8.1 | 2.16% | |
| Forest, sky from probes | 2.65 | 7.9% | 3.3 | 2.16% | |
| Forest, probes × AO | 2.50 | 9.0% | 3.3 | 2.16% | |
| Forest, `fast` | 2.81 | 12.0% | 3.9 | 2.37% | |
| Forest, `fallback_fast` | 2.69 | 9.5% | 3.7 | 2.37% | |
| Forest, all analytic | 5.58 | 33.0% | 7.3 | 0.81% | |
| Tower floor (A vs B) | 10.98 | 58.6% | — | 0.06% | |
| Tower, base | 18.48 | 76.4% | 22.4 | 0.75% | 1.00 |
| Tower, sky from probes | 8.85 | 48.2% | 11.7 | 0.75% | 1.13 |
| Tower, `fast` | 9.79 | 46.3% | 15.9 | 0.93% | 1.04 |
| Tower, `fallback` | 8.33 | 44.1% | 11.1 | 0.75% | 1.13 |
| Tower, probe visibility off | 16.63 | 73.8% | 20.4 | 0.75% | 0.90 |
| Tower, short sky cones (2 m) | 135.9 | 98.2% | 146 | 0.75% | **41.9** |
| Landscape floor (A vs B) | 1.12 | 0.7% | — | 0.61% | |
| Landscape, base (cone sky) | 11.11 | 59.2% | 14.8 | 0.99% | |
| Landscape, sky from probes | 2.08 | 1.5% | 3.3 | 0.99% | |
| Landscape, `fast` | 2.68 | 5.0% | 5.2 | 1.55% | |
| Landscape, `fallback` | 1.44 | 1.2% | 2.7 | 0.99% | |
| Landscape, terrain sun marched, no map | 12.22 | 59.4% | 16.4 | **8.72%** | |
| Landscape, all analytic (earlier run) | 14.52 | 59.9% | 19.8 | **30.5%** | |

Per component, base configuration (sun at every pixel, sky and bounce on the grid):

| Scene | Sun: mean, > 8 | Sky (cones): mean, > 8 | Bounce (probes): mean, > 8 |
|---|---|---|---|
| Forest | 0.81, 2.2% | 13.4, 79% | 1.65, 1.8% |
| Tower | 0.67, 0.8% | 20.2, 80% | 6.63, 31% |
| Landscape | 0.30, 1.0% | 33.5, 67% | 1.31, 1.3% |

**Step caps:**
- Sun rays: under 0.4% in every configuration except the landscape's marched terrain (1.2%) and analytic sun (2.9%, earlier run).
- Probe rays: under 2.2% (landscape, 2.5 km rays).
- Sky cones: none (0.1% at K = 8 in the earlier run).
- Primary rays: 0.1% (forest) or less.
- Reference rays: none in the sampled tiles.

What the correctness numbers say:

1. **Sun shadows from fields are accurate,** and all three fields agree within a point or two. Dapples, trunk and pillar shadows land where the path tracer puts them; 0.4–2.4% of pixels differ by more than 8/255, mostly along penumbrae. The cooked field costs 0–0.5 points against the analytic field; half resolution costs 0.1–0.2 more.
2. **Per-pixel cone-traced sky light is the weak link, and it is biased, not noisy.**
   - The one-sided cone estimator with wide cones over-occludes at long range: open landscape slopes come out nearly black in the sky term (`results/landscape-1080p-base-sky.jpg` against `landscape-1080p-ref-sky.jpg`).
   - Short cones under-occlude, and indoors that is the leak: 2 m cones in a 7 m room see "open sky" everywhere, so the interior glows (leak ratio 42, `results/tower-1080p-L_short.jpg`).
   - No length works for all scenes, and more cones (K = 8) don't help.
3. **Probes carry sky and bounce light well outdoors.**
   - Sky from probes meets the bar in the forest (2.65 / 7.9%) and the landscape (2.08 / 1.5%).
   - The bounce component is within ~1.3–1.7/255.
   - Short cones layered on probes (contact AO) keep it within the bar. In the landscape's all-cooked combinations the AO is effectively absent: a cooked cone starts 1.5 voxels out (6 m) and the AO length is 8 m, so `fast` there is probe light alone.
4. **Indoors fails for two reasons:**
   - **The reference is too noisy to judge against this bar.** At 166 spp per grid pixel its own noise is ~5/255 mean, above the 3/255 bar, so criterion 2 can't be met by any real-time result here. Small bright sources (the window, the sun patch) are the worst case for a path tracer.
   - **The real-time result is genuinely off.** The noise-free RMSE is 11–16/255 for the best tower configurations against 3.3–3.9 outdoors. Probes at 0.75 m miss the strong local bounce around the sun patch (the pillar base glows in the reference, `results/tower-1080p-base-bounce-compare.jpg`), and the interior reads a little flatter than the reference.
5. **Probes don't leak through the walls.**
   - With DDGI-style visibility the interior leak ratio is 0.83–1.21 in every configuration, including 1.5 m spacing (wider than the 0.9 m wall). The exceptions are the sky-cone length extremes: 42 for 2 m cones, and 0.52 (too dark) for 16 m cones.
   - The deliberately naive gather (no visibility, every probe on) made the interior *darker* (0.90), because probes buried in the walls trace black.
   - The leak that did happen came from short sky cones (point 2).
6. **Golden-hour terrain shadows need the cooked shadow-height map.** Marching the terrain toward a 7° sun costs 25.5 ms against 15.0 (hybrid with map) or 3.5 (cooked with map), and is *less* accurate: 8.7% of pixels over 8/255 against 1.0%. Rays run out of steps along the slopes, and the analytic version hit 30.5%. The map costs 34 ms of GPU to cook for a 2048² terrain (16 bands), once per sun change.
7. **Temporal history barely matters at steady state** (static camera, 120 frames): forest RMSE 8.5 / 8.1 / 8.0 for H = 1 / 16 / 64. H = 1 flickers; the static comparison can't see that. Temporal costs 0.3–0.5 ms whatever H is.
8. **Cache resolution is a cost lever more than a quality lever here.**
   - Forest probe tracing costs 2.7 / 6.4 / 18.6 ms at 4 / 3 / 2 m spacing, and 11.3 ms at 64 rays.
   - Quality moves by less than the reference noise outdoors.
   - Indoors, 0.5 m spacing improves the RMSE from 22.4 to 21.3 for 3× the probe cost; coarse 1.5 m worsens it to 25.2.

### Looks

Real-time and reference side by side: `results/<scene>-1080p-fast-compare.jpg`, `-fallback_fast-compare.jpg`, `-base-compare.jpg`. Full frames: `<scene>-1080p-fast.jpg` and `<scene>-1080p-ref.jpg`.

- **Tower: the most convincing scene.**
  - A shaft of sun across flagstones carries the pillar's shadow; the deep window glows; the cantilevered stairs spiral overhead in bounce light.
  - The real-time interior is a little darker and flatter than the reference, and lacks the reference's glow around the sun patch.
  - It doesn't look wrong, just less rich.
- **Forest: correct but not beautiful.**
  - Sun patches and shadows match the path tracer, and sky through the canopy gaps reads right.
  - But the trees are blob canopies on bare trunks, the haze is grey, and nothing glows: no leaf translucency, no light shafts. It looks like a lit greybox.
  - Real foliage (spike 03), translucency (spike 10) and fog lighting would decide whether this can be beautiful; this lighting doesn't stand in their way.
- **Landscape: pleasant, but `fast` and the fallback lose the hallmark of golden hour.**
  - A warm valley with layered haze and lit slopes.
  - **The cooked configurations drop the long conifer shadows** on the near slope (compare `landscape-1080p-fast.jpg` with `landscape-1080p-ref.jpg`). Trees 2–3 m across barely exist in a 4 m clipmap, so their shadows vanish.
  - The pixel metric hardly notices (1.6% of pixels), but the eye does.
  - The hybrid and analytic sun keep them. A finer clipmap level near the camera, or cooking thin objects conservatively, would be needed.

None of the scenes anti-alias, so mortar lines and distant trunks shimmer. That is primary visibility, not lighting.

### Other costs

- **Cooking at load** (GPU ms):
  - height maps 0.2–5 ms;
  - clipmaps 0.5–8 ms (forest fine level, 4.2 M voxels: 1.9–8.4 ms across runs);
  - landscape shadow-height map 34 ms;
  - probe placement under 1 ms.
  - Re-cooking for a moving camera would only touch the slabs it scrolls into (not measured). **Animated content (swaying trees, creatures) would need re-cooking or separate handling** (D-086's shadow maps for deforming creatures); a full forest clipmap re-cook every frame (~2–8 ms) doesn't fit.
- **Baking the fallback's probes:** 48 frames × 64 analytic rays per probe took 1.9 s of GPU (forest), 0.19 s (tower) and 1.5 s (landscape). It would have to be redone when the sun moves or the world changes, which is the fallback's weakness for a day cycle.
- **Memory (allocated for the largest case):**
  - G-buffer and lighting targets 150 MB at 1080p (with ping-pong);
  - probe atlases 96 MB and ray buffer 32 MB, for 16,384 probes;
  - clipmaps 15 MB;
  - height and shadow-height maps 96 MB (the landscape's 2048² `rgba32float` and `rg32float`; half-floats would do);
  - reference accumulators 127 MB (reference only).
- **Pacing at 60 Hz (busy-wait, lighting passes only):** the judged configurations stayed under 16.7 ms (forest 2 of 150 frames over), but **took 1.2–2.8× longer than back to back**, as in spikes 01 and 02: landscape `fast` 14.0 ms paced against 5.0. The OS lowers GPU clocks at that load.
- **The reference cost far more than the real-time path.** One sample pair took 0.24–1.4 s for the sun at every pixel and 0.1–0.45 s for the indirect grid, depending on the scene, at 70–250 steps per indirect ray. Its budget was 6–12 s per scene at 1080p.

### Caveats

- **Device and browser:** one M4 and one browser. Timings are indicative; the final quiet run decides them.
- **Clocks and thermals:** timings drifted 10–20% between runs and within a scene's sweep (`base_again` against base), and pacing changes clocks a lot. The MacBook Air is fanless; a 4½-minute run warms it.
- **The quality bar is this spike's,** and indoors it is below what this reference can resolve. The unbiased RMSE is the more trustworthy indoor number.
- **One bounce only,** in both the real-time path and the reference. Real interiors get noticeably brighter with more bounces, which the probes could add by feedback (DDGI's default); that wasn't tested against a matching reference.
- **Static scenes and camera** during measurement. Reprojection and disocclusion run but aren't exercised, and probe lag and temporal ghosting under motion aren't measured.
- **The content is simple:** blob trees, a stylized tower, a noise landscape. Real foliage is costlier per step for primary and for cooking, not for cooked lighting rays.
- **"What the compiler would emit" doesn't apply here:** this is engine code (cooking, probes, passes). Only the field functions are what the language would generate.

### Changes made during development, before the final runs

The criteria didn't change; how criterion 2 is evaluated and how criterion 4 is reported did (noted under the criteria).

1. **Wide sky cones became one-sided.** The two-sided penumbra estimate (right for the sun) read a cone through a thin ceiling as half open, lighting the tower's interior.
2. **Lighting rays moved from the analytic field to cooked fields,** after the first measurements put analytic lighting at 20–30 ms. The terrain height map, the objects-only clipmap and the shadow-height map were added for that reason, and so were the hybrid mode, the half-resolution sun and gather, and the shared upsampling weights.
3. **The reference was restructured twice.** After the 2026-10-01 GPU freeze, whole-frame dispatches (up to 6 s) became tiles of one sample pair. Then, because a converged per-pixel reference of the whole frame would take minutes per scene, the sun moved to every pixel and sky + bounce to a 1-in-16 grid with reconstruction for display, and the unbiased RMSE was added.
4. **A bug made a false surface:** above the terrain's maximum height, the cheap lower bound on the terrain gap was treated as a surface. That put a floor at y = 0.85 m in the tower, one at 0.7 m in the forest, and speckles in the landscape's sky. It was fixed before any reported measurement.
5. **Instances read their base height from the cooked height map** (landscape trees and boulders) instead of re-evaluating five octaves of terrain at every sample. The primary pass, the lighting rays and the reference all use the same lookup, so they still see one field.
6. **Configurations were added after seeing results:** probes × AO, `fast`, `fast_fullsun` and `fallback_fast`. Criterion 4 picks the cheapest configuration that meets the bar, so adding configurations can only find a cheaper one. The fallback configurations are judged separately.

### What this means for the design

Nothing here is recorded in `decisions.md`; that's the owner's call.

- **Fields can give correct sun shadows, sky light and one bounce outdoors, near budget, and the fallback fits.** The per-frame technique needs a cooked lighting field, not the authored one. It is inconclusive in the landscape (5.0 ms) and fails in the forest (8.0 ms just misses the bar, 11.3 ms meets it). Probes baked at load with a per-frame half-resolution sun pass at 2.4–3.0 ms, but only for a static sun and world.
- **Per-pixel distance-field cone tracing is the wrong tool for sky light at range.** Use probes for sky and bounce, and cones only for short-range contact occlusion.
- **Indoors is unresolved, not leaking.** DDGI-style visibility over field-traced probes doesn't leak through thick walls. But accuracy around strongly lit patches needs finer or adaptive probes (or screen-space or per-pixel final gather near the patch), and judging it needs a better indoor reference (portal or patch importance sampling).
- **What the language and engine would need (hypotheses):**
  - cooking a field into a clipmap, a height map and a shadow-height map is a general operation, not an engine rule, so it passes D-050's test;
  - instance placement data (base heights) belongs in cooked data, not re-evaluated per sample;
  - thin objects need conservative cooking, or a finer level, to keep their shadows.

**If primary visibility is rasterized** (the hybrid the other spikes point to), most of this carries over unchanged. The lighting passes only read a G-buffer of depth, normal and albedo, so every lighting cost and accuracy number above applies to a rasterized G-buffer as measured. That includes sun cones at full or half resolution, probe tracing and gather, sky probes with contact cones, the temporal pass, the composite, and the conclusion that lighting rays must trace cooked fields (~0.2 ns a step) rather than the authored one (0.5–1.2 ns). It also includes the shadow-height map for low suns, the leak behaviour, and the half-resolution lever.

What would change:
- **Self-shadowing:** ray origins come from triangles, not the field's zero set. They sit up to the extraction tolerance off the cooked or analytic surface, so the normal offset must grow past that, or contact shadows show acne or gaps (Lumen's mesh-SDF mismatch). The hybrid near zone would evaluate the field at a mesh surface, so it would need the same care.
- **The budget gets easier:** the 80–135 ms ray-marched primary leaves the frame, and the lighting slice can be judged as a whole.
- **Dynamic content:** rasterized deforming creatures (D-086) won't be in the cooked field, so they need shadow maps or proxy shapes for sun and probe rays.
- **Anti-aliasing:** temporal anti-aliasing of the rasterized G-buffer would make the half-resolution lighting passes and their upsampling the natural default, with this spike's 540p numbers (`fast` 2.4–4.8 ms, fallback under 1 ms) as the indicative cost.

## Quiet-machine rerun (2026-10-02)

Run file: `results/run-2026-10-02T04-13-47-881Z.json`: serial, after a cool-down, headless Chrome 154, alone on the GPU. Its canary (a fixed kernel timed before each configuration) read 4.1–4.2 ms apart from three blips (up to 5.2 ms), against up to 7.3 ms in the final run above, and `base_again` matched base within 0.4%. Values are forest / tower / landscape at 1920×1080 unless noted.

| Metric | README value | Quiet value |
|---|---|---|
| Lighting ms, base | 32.3 / 13.7 / 30.9 | 19.2 / 9.2 / 22.7 |
| Lighting ms, `fast` | 8.0 / 6.2 / 5.0 | 5.7 / 4.0 / 4.3 |
| Lighting ms, `fast_fullsun` (forest's judged config) | 11.3 | 7.4 |
| Lighting ms, `fallback_fast` | 3.0 / 3.0 / 2.4 | 2.6 / 2.2 / 2.2 |
| Lighting ms, `fast` at 960×540 | 2.4–4.8 (range only) | 3.1 / 1.6 / 2.5 |
| Forest and landscape correctness, every 1080p row (mean, > 8/255, unbiased RMSE, sun term) | as tabled above, e.g. forest `fast` 2.81, 12.0%, 3.9 | identical to the last digit: each reference drew the same sample pairs as before (the 16-pair indirect minimum, fixed seeds) |
| Tower floor (A vs B): mean /255, > 8/255 | 10.98, 58.6% (166 spp) | 8.52, 49.1% (268 spp) |
| Tower `fast`: mean, > 8/255, unbiased RMSE | 9.79, 46.3%, 15.9 | 8.95, 40.8%, 15.9 |
| Tower `fallback`: mean, > 8/255, unbiased RMSE | 8.33, 44.1%, 11.1 | 7.33, 36.9%, 10.95 |
| Tower leak ratio: `fast`, `fallback`, visibility off, 2 m cones; range of the other 1080p configs except 16 m cones | 1.04, 1.13, 0.90, 41.9; 0.83–1.21 | 1.05, 1.15, 0.92, 42.5; 0.84–1.22 |

**Verdicts: none change** under criteria 1–4. Forest: `fast` (5.7 ms) still misses criterion 2 (11.95% of pixels over 8/255), so `fast_fullsun` is judged and stays **Fail** at 7.4 ms (fastest of 90 frames 7.27 ms, past the 7 ms line); `fallback_fast` stays **Pass** at 2.6 ms, no longer at the edge. Landscape: `fast` stays **Inconclusive** at 4.3 ms (fastest frame 4.13 ms, over 3.5); `fallback_fast` stays **Pass** at 2.2 ms. Tower: still fails on correctness (best mean 7.33/255 against 3, with the reference's own half-vs-half noise at 8.5/255); on time alone `fast` (4.0 ms) would be Inconclusive and `fallback_fast` (2.2 ms) a Pass. No per-frame configuration reaches 3.5 ms at 1080p in any scene.
