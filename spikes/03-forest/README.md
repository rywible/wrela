# Spike 03: a ray-marched forest

*A measurement spike for the flagship's first priority, a beautiful living forest. It's throwaway code, not the start of the engine.*

## The question

**Can a ray-marched forest look beautiful and fit the vegetation slice (4.0 ms at 1080p), with its light inside the lighting slice (3.5 ms)?** No triangles: one compute kernel finds what each pixel sees, a second lights it, a third accumulates it over time.

A dense forest is the worst case for depth complexity: a ray at eye height can pass trunks, grass and dozens of leaf clusters before it stops, and a shadow ray from the floor has to find the gaps in the canopy.

## Kill criteria, written before measuring

All at native 1920×1080 on the primary device (MacBook Air M4, 8-core GPU, D-096), Chrome stable, from GPU timestamps under sustained load (30 warm-up frames, then the median of 90 back-to-back frames). The verdict rule is the one in [`../README.md`](../README.md).

1. **Vegetation ≤ 4.0 ms** in each of the three scenes. Vegetation is the *marginal* cost of grass, plants and trees in primary visibility:

   > vegetation = `trace` (full forest) − `trace` (terrain only)

   - **≤ 4.0 ms:** pass. **4.0–8.0 ms:** inconclusive. **> 8.0 ms:** fail. The worst scene decides.
2. **Lighting ≤ 3.5 ms:** the whole `light` pass, which casts the sun-shadow rays through the canopy (dappled light, leaf self-shadowing), estimates sky occlusion, and shades every layer. **≤ 3.5 ms** pass, **≤ 7.0 ms** inconclusive, **> 7.0 ms** fail. The worst scene decides.
3. **Correct, not just fast.** Each scene's fast path is compared with a brute-force reference of the same content:
   - **The reference:** explicit leaves and grass blades at every distance (no volumes, no statistical grass), the same analytic intersections, step caps raised ×8 or more, 96 jittered samples per pixel, and shadow rays that test explicit leaves against a sun direction sampled over the sun's disc.
   - **The comparison** is between images *converged over 64 jittered frames* (static camera, frozen time), because the fast path is designed to be resolved by temporal accumulation and a one-sample forest aliases by construction.
   - **Thresholds:** mean difference **≤ 2/255** and **≤ 2%** of pixels off by more than 8/255. Rays that end by hitting a step cap must be **≤ 0.1%** of pixels.
   - The reference's own noise floor is measured by comparing its two independent halves. If that floor is above the thresholds, criterion 3 is reported as *not measurable*, not as a pass.
   - These thresholds are looser than spike 02's (0.5/255, 0.5%) on purpose: there, both images were opaque surfaces at one sample; here, the fast path replaces sub-pixel leaves by volumes whose shading model differs from the leaves'. The single-frame difference and a shimmer metric are reported too, without thresholds.
4. **Transitions keep density.** At each level-of-detail boundary, the two adjacent representations of an isolated tree, each forced on and converged, must give the same foliage coverage (mean opacity over the tree's screen footprint) within **5%**.
5. **Beauty** has no number. The screenshots are judged by eye and the judgement is written down, including what's ugly.

The scenes must hold: **≥ 200 trees within 300 m** of the floor camera, and forest to **≥ 1 km** from the camera above the canopy.

## Fallback if it fails

- **Triangles up close, volumes far away,** as UE5's Nanite Foliage does: spike 01's route for the near leaves and grass, with this spike's far volumes kept.
- **Or a stylized look of soft volumetric clumps:** the middle and far representations only. This spike measures that variant directly (`clumps` below), so the fallback's cost is known.

## The world

One shared world field, three cameras:

| Scene | Camera | What it shows |
|---|---|---|
| `floor` | 1.7 m above the ground in a clearing, looking into the forest | grass, ferns, leaf litter, trunks, crowns, sky through gaps |
| `up` | under a broadleaf at the clearing's edge, looking up toward the sun | backlit translucent leaves, branches, sun through the canopy |
| `above` | 60 m above the ground, looking across the forest | crowns to the horizon, meadows, haze |

- **Terrain:** gently rolling analytic hills (a few sines), traced as a heightfield with a directional Lipschitz bound (spike 02's method).
- **Trees:** domain repetition on a 6 m grid (as built; the plan said 10 m, see *Changes* below). Each cell holds a canopy tree and an understory tree, each with hashed existence (occupancy × a forest mask with the clearing and meadows), position, species (broadleaf or conifer, in mixed stands), size, colour and leaf-grid orientation. Crowns may overhang their cell by up to 3.5 m.
  - **Broadleaf:** a tapered trunk with a root flare, four branches, and a crown made of seven foliage clumps inside an ellipsoid; leaves sit mostly in each clump's outer shell.
  - **Conifer:** a trunk, and a cone of lopsided whorl tiers of needle sprays.
- **Leaves:** a periodic 16×16×16 tile of leaf cells (22 cm for broadleaves, 10 cm for conifers), in two interleaved grids offset by half a cell. Each cell has six leaves with hashed position, orientation, size and tint. A cell's leaves exist where a hash falls below the crown's density profile times a smooth gap field (holes and ragged edges at the 0.5–2 m scale).
- **Floor:** grass blades on a 15 cm grid (two per cell by default) where the clearing and meadows allow, ferns on a 1.3 m grid near the forest's edge and in patches under the trees, and a litter-and-moss ground colour.
- **Wind:** cheap per-cell domain warps. Crowns sway as a whole, leaves flutter within their cells, and grass blades lean with a travelling gust. Every warp is bounded, and the bounds include the warp.

## The method

### Multi-scale foliage

Each crown is drawn at one of three levels, chosen per crown and per pixel by the pixel's footprint where the ray enters the crown:

1. **Explicit leaves (near).** A 3D DDA walks the crown's leaf cells, and each cell's leaves are intersected analytically (ray against a planar leaf shape). The first hit ends the walk. No sphere tracing: leaves are thin, and thin things are sphere tracing's worst case.
2. **Volumetric texture (middle),** after Neyret and Decaudin (EGSR 2004). The same leaf tile is baked into a mip-mapped 3D texture of leaf-area density, density-weighted normal and tint. The ray marches the crown with steps of two texels at the mip the footprint selects, and composites front to back.
3. **Density volume (far).** A homogeneous ellipsoid or cone, integrated analytically along the ray (Beer–Lambert), lit with a foliage phase function.

**Transitions.** Between levels, each crown picks one representation stochastically, with a probability that ramps across the boundary, and temporal accumulation blends the result. The densities are *calibrated* so that each level's mean transmittance matches the one below it:
- volumetric texture against explicit leaves: one factor per mip level and per band of ray elevation, from random rays through the tile (computed on the page at start-up);
- density volume against volumetric texture: one extinction per species, from random parallel rays through sample crowns.

Criterion 4 checks the result in the renderer itself.

### Passes

| Pass | What | Slice |
|---|---|---|
| `trace` | Terrain, the grass shell, ferns, then a slab walk over the tree grid near to far. It writes a two-layer G-buffer: the nearest opaque surface (ground, blade, frond, bark, explicit leaf) and one composited foliage layer in front of it (colour, opacity, normal, representative depth). | vegetation (minus terrain) |
| `light` | A sun-shadow ray from each layer's point, cone-traced through the crowns (trunks are opaque; crowns use the volumetric texture at the mip the sun's cone selects, so dapples blur as they should). Sky occlusion from nearby crowns as analytic spheres. Two-sided leaf shading with translucency, a foliage phase function, sky, aerial perspective, tone mapping. | lighting |
| `taa` | Reprojection through the G-buffer's depth, 3×3 YCoCg neighbourhood clamp, exponential blend. Sub-pixel jitter (Halton 2, 3) feeds it. | temporal accumulation |

### Measurements

- **Per pass:** GPU timestamps, 30 warm-up frames, then 90 back to back; medians and p95. Timestamps are quantized to ~65.5 µs.
- **Resolutions:** native 1920×1080 and an internal 960×540 (same passes at half size, bilinear blit; no upscaler is implemented, so no upscaled quality is claimed).
- **Paced:** 180 frames submitted at 60 Hz by busy-wait, with the first 30 dropped.
- **Where the steps go:** counters for terrain steps, tree cells tested, crowns entered per level, leaf cells walked, leaves tested, volume samples, grass and fern cells, blades and fronds tested, shadow work, and cap hits. Heat maps of primary and shadow work.
- **Shimmer:** the mean frame-to-frame difference of the displayed image over 17 frames (16 differences, after 16 settling frames) with a static camera, still wind and sub-pixel jitter, with and without TAA.

### Sweeps

- **Tree density:** occupancy 0.35, 0.6, 0.85 (default) and 1.0 of the cells the forest mask allows, in `floor` and `above`. The tree count within 300 m is reported for each.
- **Grass density:** 1, 2 (default) and 3 blades per 15 cm cell, in `floor`.
- **View distance:** 250 m, 500 m, 1 km, 2 km (default) and 4 km, in `above`.
- **Representation:** `auto` (default), `no-explicit` (volumetric texture up close) and `clumps` (density volumes only, the stylized fallback), in `floor` and `up`.
- **Added during the work:** the explicit-cell budget (12, 32, unbounded) in `floor` and `up`, and volume steps per crown (10, 16) in `floor` and `above`.

## Layout

| File | What |
|---|---|
| `world.js` | The JS mirror of the world (hash, terrain, masks, trees, crown envelopes), the leaf tiles and their packed table, the 3D-texture bake, and both density calibrations |
| `common.wgsl` | The frame uniform, hash, terrain, masks, intersections, G-buffer packing |
| `foliage.wgsl` | Trees (`tree_lite`, `tree_full`), crown envelopes, the grid walk, the explicit-leaf walk, volume transmittance, bark |
| `map.wgsl` | Cooks the per-cell tree attributes into a table at start-up |
| `trace.wgsl` | Primary visibility: terrain, grass, ferns, trees, the three foliage levels |
| `light.wgsl` | Sun shadows through the canopy, sky occlusion, shading, sky and fog |
| `post.wgsl` | TAA, accumulation, image differences, coverage |
| `blit.wgsl` | Presentation (not measured) |
| `main.js` | Harness: pipelines, scenes, timing, correctness, sweeps, transitions |
| `results/` | Raw JSON from each run, and screenshots (PNG ignored; JPEG copies of the key ones kept) |

## Running it

```bash
python3 spikes/serve.py 8417
```

```bash
spikes/headless.sh 03-forest '#run' 900
```

- `#quick` (~10 s) renders each scene with TAA settled, a heat map and the work counters, then stops.
- `#probe` (~30 s) times each scene with one feature switched off at a time: the cost breakdown.
- `#trans` runs criterion 4's transition test alone.

Every submission is kept short (GPU safety rules in [`../README.md`](../README.md)): trace and light are submitted as two half-frame dispatches each, and the reference is rendered in 240×135 tiles, one awaited submission each.

## Results (2026-10-01)

**Setup:**
- MacBook Air M4 (8-core GPU), Chrome stable 154 headless, through `spikes/headless.sh`.
- The final run is `results/run-2026-10-02T03-01-49-827Z.json` (242 s, under the GPU lock, so no other page used the GPU during it).
- **Timings are indicative only.** Other spikes ran heavy GPU work between my runs, and this Air has no fan:
  - The trivial TAA pass measured 0.92 ms here, against 0.79 ms in a cooler run.
  - The same `trace` pass measured 73 ms and 88 ms in two configurations of this run that differ only in the light pass.
  - Read every number as ±20%. The verdicts below don't depend on that margin.
- Medians of 90 frames after 30 warm-up frames for each scene's main configurations. Secondary configurations and sweeps use 30 frames after 10 warm-up frames.

### Verdict

| Criterion | Measured (1080p) | At 960×540 | Verdict |
|---|---|---|---|
| 1. Vegetation ≤ 4.0 ms | floor **68.3**, up **74.8**, above **48.5** ms | 12.7, 20.3, 11.3 ms | **Fail** (19× the slice in the worst scene; 5× at 540p) |
| 2. Lighting ≤ 3.5 ms | floor **22.5**, up **20.4**, above **23.3** ms (shadow rays are 19–21 ms of it) | 6.2, 5.9, 6.6 ms | **Fail** (6.7×; 1.9× at 540p, which would be inconclusive) |
| 3. Matches the reference | converged fast vs reference: mean **7.0 / 10.7 / 8.3** /255, **30 / 26 / 50%** of pixels over 8/255. The reference's own noise floor is 1.6–2.8/255 with 8–12% over 8/255. | | **Not measurable** by the rule written beforehand: the floor is above the thresholds. The fast path is still clearly **different from the reference**, by 3–7× the floor. |
| 3b. Step caps ≤ 0.1% of pixels | 0.85% (floor), 0 (up), 0.30% (above). All are terrain-march caps on grazing horizon rays; the vegetation walks never capped. | | **Fail** (the terrain, not the vegetation) |
| 4. Transitions keep coverage within 5% | explicit → volume: broadleaf −1.8%, conifer −0.2%. Volume → far: broadleaf **+5.4%**, conifer +1.9%. | | **Pass** except broadleaf volume → far, a **marginal fail** |
| Content | 6,437 canopy trees (plus 2,698 understory) within 300 m of the floor camera; forest drawn to 2 km (4 km in the sweep) | | Holds |
| 60 fps | Paced at 60 Hz: median frame 109 ms; all 150 frames over 16.7 ms | | Fail |

**So no.** A fully ray-marched forest with this design costs 80–97 ms per 1080p frame on the M4: vegetation 12–19× its slice, lighting 6–7×. Even at 540p, vegetation is 3–5× its slice. The cheapest representation measured, density-volume crowns only (the stylized fallback), still costs 27 ms of vegetation in the floor scene, because the grass, the trunks and the walk through the tree grid remain.

### How it looks

Judged by eye from `results/*-taa.jpg` (the displayed image after TAA settles) and the reference crops:
- **`up` is the best of the three, and close to beautiful.** It shows dense, backlit, translucent leaves, the sun glowing through gaps, branch forks and clump structure (`up-taa.jpg`). The weak points:
  - soft, out-of-focus-looking patches where the explicit-leaf budget runs out and the crown interior continues as volume;
  - plain grey trunks;
  - trees too evenly sized.
- **`floor` is pleasant but stylized, not beautiful** (`floor-taa.jpg`).
  - The near grass and ferns are convincing: backlit blade tips and a tree shadow on the clearing.
  - The forest edge reads as a stage set. Conifers are dense dark spires with visibly layered tiers, a few trunk lines show through the thin crown tops, and broadleaf crowns are soft cauliflowers on tall bare poles.
  - The forest's interior is too bright and flat. No dappled light reads at this distance.
- **`above` is a believable forest to a hazy horizon, but toy-like up close** (`above-taa.jpg`).
  - Broadleaf crowns look like broccoli with a yellowish cast, and ground shows between them where a dense forest's canopy would close.
  - The conifers and the aerial perspective work.
- **The reference is far more beautiful than the fast path** in all three scenes (`*-crop-ref.jpg` against `*-crop-converged.jpg`). With explicit leaves everywhere and explicit leaf shadows sampled over the sun's disc, the floor scene becomes a deep, dark, misty forest with soft textured conifers, and `above` looks almost photographic. The gap between the two is the volumetric representations' look, not their density:
  - The density is calibrated: coverage across transitions holds within 2%, except broadleaf volume → far at 5.4%.
  - The look is not. Within the trees at a transition, the colour differs by a mean of 9–20/255.
- **The stylized fallback, as built, is not usable:** density-volume crowns alone look like translucent ghost discs (`floor-clumps.jpg`, `up-clumps.jpg`). It would need its own art direction (opaque, textured clumps).

### Where the time goes

Per pixel, from the counters (1080p, final run):

| Scene | Terrain steps | Tree slabs / cells | Trees tested | Leaf cells / leaves tested | Volume samples | Grass cells / blades | Shadow cells / volume samples |
|---|---|---|---|---|---|---|---|
| floor | 20.6 | 8.0 / 47.5 | 2.6 | 15.1 / 17.3 | 7.2 | 13.5 / 26.9 | 39.4 / 3.9 |
| up | 0 | 5.5 / 27.7 | 6.4 | 42.4 / 36.3 | 6.8 | 0 | 17.8 / 6.9 |
| above | 28.4 | 10.3 / 61.2 | 5.6 | 1.9 / 2.9 | 9.6 | 0 | 25.6 / 3.9 |

The cost breakdown, from `#probe` on an earlier configuration (explicit budget 24, explicit range by leaf width; indicative), switching one feature off at a time in `floor`:

| Component | Share of the trace pass | Note |
|---|---|---|
| Grass and ferns | ≈ 15 of 42 ms | Explicit blades out to 8–15 m, ferns to 23–46 m |
| Understory trees | ≈ 3.5 ms | Plus ≈ 3.5 ms in the light pass |
| The tree walk, trunks and volumes | the rest | Rays at eye level cross 8 slabs and 50 cells of the tree grid |
| Shadow rays | ≈ 12 of 17 ms of the light pass | 40 tree cells and 4 volume samples per shadow ray |

**Heat maps** (`*-heat.jpg`, `floor-heat-shadow.jpg`):
- **Primary work concentrates in a few places.** It's highest on crown silhouettes and grazing chords through crowns, and in the empty space inside the conifers' bounding cones above their apexes. The forest band at the horizon and the grazing grass shell at the forest's edge are also hot.
- **Shadow work is high across the whole ground,** because every ground pixel's shadow ray climbs through about 36 m of canopy.

**Explicit leaves are the most expensive representation per covered pixel,** and the budget is the main lever:

| Explicit cells per crown before the rest is volume | floor vegetation | up vegetation |
|---|---|---|
| 12 | 45.5 ms | 39.1 ms |
| 32 (default) | 68.3 ms | 74.8 ms |
| unbounded (1,024) | 99.2 ms | 80.5 ms |
| none (volumetric texture from the first cell) | 30.3 ms | 16.9 ms |
| none (density volumes only) | 27.1 ms | 9.2 ms |

### Sweeps (1080p vegetation, ms)

| Sweep | Points |
|---|---|
| Tree occupancy, floor (canopy trees within 300 m) | 0.35: 62.8 (2,710) · 0.6: 66.8 (4,532) · **0.85: 68.3 (6,437)** · 1.0: 72.7 (7,562) |
| Tree occupancy, above | 0.35: 40.5 · 0.6: 44.4 · **0.85: 48.5** · 1.0: 49.6 |
| Grass blades per cell, floor | 1: 65.7 · **2: 68.3** · 3: 75.0 |
| View distance, above | 250 m: 27.1 · 500 m: 36.0 · 1 km: 44.1 · **2 km: 48.5** · 4 km: 51.6 |
| Volume steps per crown | floor 10 → 16: 68.3 → 72.7 · above: 48.5 → 48.0 |

- **Density barely matters:** 2.8× the trees cost 16% more. A denser forest stops rays sooner, and a sparser one lets them travel further through the grid.
- **View distance matters up to about 1 km,** then flattens, because the far crowns are analytic and rays die in the canopy.

### Other measurements

- **TAA:**
  - Cost: 0.9 ms at 1080p, 0.3 ms at 540p.
  - Shimmer (mean frame-to-frame difference, static camera, still wind, jittered): 11.9 → 0.94 (floor), 6.7 → 0.53 (up) and 4.8 → 0.39 (above) per 255. That's a **12.5× reduction**; pixels over 8/255 frame to frame drop from 19–35% to 0.4–1.8%.
  - The stochastic level choices rely on it: a single frame differs from the reference by 9–12/255.
- **Cooked tree attributes:**
  - Evaluating each grid cell's existence, species and ground height inline, instead of from the cooked table (the same functions, computed once at start-up), costs +12% on the trace pass and +41% on the light pass (floor, 540p: 15.9 vs 14.2 ms and 8.8 vs 6.2 ms).
  - The table is 72 MB (two trees per cell, ±4.6 km).
- **Memory:** leaf table 1.25 MB (packed to 12 bytes per leaf), two 3D leaf textures with mips 36.6 MB, the tree table 72 MB. The G-buffer is 24 bytes per pixel.
- **Calibration** (from the run's JSON):
  - The volume needs factors of 0.29–0.69 on the baked leaf-area density to match explicit leaves' transmittance. The factor depends on the ray's elevation: conifer sprays block vertical rays 2.3× more than horizontal ones.
  - The factors change by at most 12% between cluster densities 0.7 and 0.4 (0.30–0.69 against 0.29–0.61), so one set serves the whole profile reasonably.
  - Far extinction: 0.28–0.29 m⁻¹ (broadleaf) and 0.44–0.52 m⁻¹ (conifer), depending on view elevation.
- **The reference** costs about 180–610 ms per full 1080p sample (an estimate from the quarter frame it covered), so a beautiful explicit forest costs 2–7× the fast path per sample, before anti-aliasing.
- **Pipelines:**
  - 4 per scene (trace, light, TAA, blit), plus the table build once at start-up.
  - Cold compile time **wasn't measured.** An attempt salted the source with an unused constant, which the compiler drops, so the browser's cache served it (7 ms); the harness no longer reports it. The individual override variants took 0.3–0.5 s each to create.

### Surprises

1. **Shading, not density, made the levels disagree.**
   - The first volumetric levels used a wrapped-diffuse lobe around the crown envelope's normal. Their density matched the explicit leaves within 2%, but they looked like rim-lit plastic: converged error 11–12/255 against the reference.
   - Shading the volume with the explicit leaves' own thin-leaf model, averaged over the species' leaf orientations, brought that to 7–11/255.
   - A level of detail for foliage has to preserve the *shading statistics*, not just the opacity.
2. **Explicit leaves were memory-bound.**
   - A float table read about 3.7 KB per pixel in `up` (≈7 GB per frame).
   - Packing to 12 bytes per leaf cut that scene's trace pass by about a quarter.
3. **Most explicit cells are empty.**
   - 85–95% of the leaf cells a ray walks hold no leaves, mostly between the crown's bound and its clumps.
   - Sphere tracing the crown envelope to the entry point (its Lipschitz bound is a fact the content can declare), together with a cell budget, a volume continuation and denser crowns, cut leaf cells per pixel in `up` from 98 to 42.
4. **One leaf grid shows comb-like gaps** along cell faces seen edge-on. Two grids offset by half a cell fixed it; a per-tree rotation of the grid didn't.
5. **Splitting the trace kernel didn't help.** A trees-only and a floor-only kernel sum to the merged kernel's time, so there was no occupancy penalty to win back.
6. **Grass is a third of the floor scene.** Thin explicit blades at grazing angles walk 30+ cells per pixel before a hit.
7. **540p is 3.7–5.4× cheaper than 1080p** for 4× fewer pixels. The hypothesis is that short frames don't trigger the same throttling; it's untested.
8. **The fanless Air throttles under sustained load** from back-to-back runs. Final measurements need a cold start.

### Changes made after exploratory measurements, before the final run

The kill criteria didn't change. The method did, in these ways; each change was made to fix a look problem or a cost problem seen in an earlier run:

1. **Grid:** 10 m → 6 m, with a second, understory tree per cell. On 10 m the forest read as lollipop trees on a plantation.
2. **Crowns:** sinusoidal lumps → seven clumps; conifer tiers made lopsided, with a softer sawtooth. The lumps rendered as smooth wavy sheets.
3. **Leaf tile:** 8³ cells of five leaves (leaf-area density ≈ 0.55 m²/m³; crowns were see-through) → 16³ cells, two grids, six leaves per cell per grid, a gap field. The leaf table was packed to 12 bytes per leaf.
4. **Explicit range:** an explicit-cell budget (32) per crown, with a volume continuation, and the envelope entry skip. The explicit → volume threshold moved from the leaf's width (k0 = 1) to the leaf cell (k0 = 0.25 cells), which takes explicit conifers out to 29–58 m.
5. **Shading:** volumes shaded with the averaged thin-leaf model (above). Statistical grass shaded as averaged blades instead of tinted ground.
6. **Far calibration:** it now emulates the shader's own march, per view elevation. It had matched a fine-step volume, and the far level came out 8% too opaque.
7. **Cooking:** per-cell tree attributes cooked into a table.
8. **GPU safety:** half-frame submissions, a tiled reference (240×135, awaited) and sequential shader compiles, after the 2026-10-01 incident. The reference covers the frame's central quarter at 96 samples (2 × 48), so that it's affordable.

### Caveats

- **One device and one browser:** the M4 and Chrome 154. Timings are contaminated, as above.
- **The reference shares the lighting model's approximations:** the sky occlusion from the nine nearest crowns as spheres, the constant bounce light and no terrain self-shadowing. Criterion 3 compares representations, not physical truth.
- **The reference covers the frame's central quarter.**
- **Brighter sunlit foliage in the fast path** is the largest remaining error in `up` (`up-crop-error.jpg`). The hypothesis is that coarse shadow-volume steps (six per crown) bias the sun's transmittance upwards: for a heterogeneous crown, the mean of exp(−τ̂) exceeds exp(−mean τ̂). It's untested.
- **The far calibration ignores the per-tree tilt of the leaf grid,** which is a likely cause of the residual +5.4%. That's a hypothesis.
- **Sky occlusion and bounce light are crude stand-ins** (spike 05's question). There are no god rays and no terrain shadows.
- **"What a compiler would emit" doesn't apply** to most of this code: the grid walk, the levels of detail, the calibration and the passes are engine code. The leaf and blade intersections, envelopes and per-cell functions are field code.

### What this means for the design

Nothing here is recorded in `decisions.md`; that's the owner's call.

- **Pure ray marching fails for a dense forest on the M4,** by about an order of magnitude at 1080p and 3–5× at 540p. The named fallback applies.
- **What carries over to a hybrid** (rasterize what's big on screen; fields for lighting, distance and small instances):
  - **The far density volume carries over** as it is. It's analytic, almost free per crown, calibrated to keep coverage, and it agrees with the mid level within 2–5%.
  - **Cone-traced canopy transmittance carries over as the lighting path.** It traces the prefiltered leaf density at the sun cone's mip, which gives soft dapples and leaf self-shadowing that rasterized near leaves could use too. Its cost (≈ 20 ms at 1080p here) says it should run at reduced resolution or into a transmittance map, not per full-resolution pixel.
  - **Calibration and statistical shading carry over.** Matching mean transmittance across representations, and shading a level with the average of the explicit model, is what lets rasterized leaves, traced volumes and impostors agree in density and look.
  - **Cooking carries over:** per-cell tables and packed leaf tiles. The leaf tile is already realizable as instanced quads.
  - **Explicit traced leaves and blades don't carry over.** They were the dominant cost per covered pixel and belong to raster up close.
  - **The mid-distance volumetric texture is the weakest piece.** It cost about as much per crown as explicit leaves, blurred, and needed the shading fix to look right. Next to rasterized near foliage, mid-distance trees are better served by raster LOD or impostors, with the volumetric texture kept as the shadow and visibility medium.
- **What the language would need,** as hypotheses: an exact ray–planar-shape intersection as a derived interpretation of thin primitives, Lipschitz facts on envelopes (for the empty-space skip), memoizable per-cell functions (cooking), and a prefiltered statistical interpretation (a mip-mapped density with calibration). None is engine-specific (D-050).

## Quiet-machine rerun (2026-10-02)

`results/run-2026-10-02T04-23-45-689Z.json` (205 s): serial, after a cool-down, Chrome 154 headless, alone on the GPU, same parameters as the run above. Its timings are 5–28% lower than the contended run's, and three move by more than the ±20% assumed above (`up` vegetation at 1080p, `floor` vegetation and lighting at 540p). The image metrics, counters and step caps are deterministic and came out identical.

| Metric (ms unless noted; floor / up / above) | README value | Quiet value |
|---|---|---|
| Vegetation, 1080p | 68.3 / 74.8 / 48.5 | 61.1 / 59.8 / 41.7 |
| Vegetation, 540p | 12.7 / 20.3 / 11.3 | 9.2 / 17.0 / 9.8 |
| Lighting, 1080p (shadow rays) | 22.5 / 20.4 / 23.3 (19–21) | 20.3 / 16.9 / 20.3 (15.8–18.8) |
| Lighting, 540p | 6.2 / 5.9 / 6.6 | 4.7 / 4.85 / 5.6 |
| Volumetric near (`no-explicit`) vegetation, floor / up | 30.3 / 16.9 | 28.9 / 15.6 |
| Clumps vegetation, floor / up | 27.1 / 9.2 | 25.2 / 8.5 |
| Leaf budget 12 / 1,024 cells: floor; up | 45.5 / 99.2; 39.1 / 80.5 | 41.1 / 89.8; 34.7 / 74.8 |
| Shimmer raw → TAA (/255) | 11.9 → 0.94, 6.7 → 0.53, 4.8 → 0.39 (12.5×) | Same (12.7×, 12.7×, 12.2× per scene) |
| Transitions: coverage change, broadleaf 0–1 / 1–2, conifer 0–1 / 1–2 | −1.8 / +5.4%, −0.2 / +1.9% | Identical |
| Converged vs reference (/255; share over 8/255); reference noise floor | 7.0 / 10.7 / 8.3; 30 / 26 / 50%; 1.6–2.8 | Identical |
| Step caps (share of pixels) | 0.85 / 0 / 0.30% | Identical (in `above`, 9 tree-slab caps fall within the 0.30%) |

**Verdicts:** none change under the criteria above. Vegetation still fails (the worst scene is now `floor` at 61.1 ms, 15× the slice, not `up` at 19×; 4.2× at 540p) and lighting still fails (5.8×; 1.6× at 540p, still in the inconclusive band). Criteria 3, 3b and 4 are unchanged, and 60 fps still fails (paced median 86.6 ms, all 150 frames over 16.7 ms).
