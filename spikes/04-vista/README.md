# Spike 04: the vista

*A measurement spike for spikes 03–08 (see [../README.md](../README.md)). It's throwaway code, not the start of the engine.*

## The question

Can a large world's field be **cooked on the device into a cache** and **ray-marched to the horizon**, inside the world-geometry slice (**3.0 ms at 1080p**), **while the player moves**?

- **The world** is one field program: about 8×8 km of mountains, valleys, cliffs and rock outcrops with detail down to ~10 cm near the camera, plus a few stone towers and ruins built by CSG. There's simple aerial perspective and fog, which belongs to the 1.0 ms sky slice and is reported separately.
- **The technique:** cook the field on the GPU into a sparse, multi-resolution cache around the camera (a clipmap of distance-field bricks, finer near the camera), march the cache instead of the program, and re-cook incrementally as the camera moves.
- **Compared against:** marching the analytic field directly with no cache, and (terrain only) a rasterized heightfield clipmap.

## Kill criteria, written before measuring

All times are GPU timestamps under sustained load (30 warm-up frames, then 90 back-to-back), medians unless stated, at 1920×1080 on the MacBook Air M4.

**World geometry** = the cache update pass (classify, allocate, cook) + the geometry pass (primary visibility, normal and material into a G-buffer). Lighting (sun shadows, ambient occlusion; spike 05's slice) and sky/fog (the sky slice) are separate passes, timed and reported separately.

1. **Budget** (the shared verdict rule). For the best cache configuration that also passes criterion 2, the worst of the three views:
   - **Static views** (hillside, tower top): median world geometry.
   - **Moving** (flyover at 60 m/s, re-cooking as it goes): **p95** of world geometry, because hitches are what moving costs.

   | Result | Verdict |
   |---|---|
   | ≤ 3.0 ms | **Pass** |
   | ≤ 6.0 ms | **Inconclusive** |
   | > 6.0 ms | **Fail:** use the fallback |
2. **Correct, not just fast.** Against a brute-force reference of the same field (the analytic field, plain sphere tracing with half-size steps, a fifth of the hit tolerance, step caps in the thousands, the same pixel centres, the same lighting and sky passes): mean difference **≤ 0.5/255**, **≤ 0.5%** of pixels off by more than 8/255, and rays that hit the step cap **≤ 0.1%** of pixels. These are spike 02's thresholds. A configuration that fails doesn't count for criterion 1.
3. **Memory:** the cache (brick atlas, page tables, job lists) stays **≤ 512 MB**, a third of the 1.5 GB tab.
4. **Sky and aerial perspective ≤ 1.0 ms** at 1080p (its own slice).
5. **Precision:** the cache path shows no f32 precision artifacts with the camera 11.6 km from the coordinate origin (an 8×8 km world whose origin is a corner): its image differs from the same view at the origin by no more than criterion 2's thresholds.

Reported without a threshold, because there's no basis for one yet: cooking throughput (bricks per ms, ms per frame at 10 and 60 m/s), the cost of a teleport (a full re-cook), horizon shimmer, and the analytic path's precision.

## The fallback, if it fails

**Rasterized terrain clipmap meshes extracted from the field, with ray marching only for near detail.** The raster clipmap measured here (terrain only) is the start of that fallback, so its cost is measured alongside.

## The method, as built

*The method section was written before measuring. It now describes what was built; items marked (†) changed during development, and "Changes made during development" below lists them with reasons. The kill criteria above didn't change.*

### The world field (`field.wgsl`)

One function of position in metres, with a pixel footprint for filtering (D-077's bandlimits). The views and landmarks sit in an 8×8 km square; the noise itself is unbounded.
- **Terrain:** an eroded fBm heightfield (value noise with derivative damping), 15 octaves from 2 km down to 12 cm wavelength (†), with a large-scale mask that separates lowland valleys from mountains, and flattened valley floors. Heights run from 14 to 1,700 m.
  - Its distance is `(y − h) · K` with `K = 1/√(1 + G²)`, from an **assumed** slope bound G = 3 (†). That's an `@assume(lipschitz)`-style fact.
  - **Checked by sampling** 200,000 points: 0.3% of the terrain is steeper than 3 (p99.9 is 3.4, the maximum 5.4). So the fact is violated on 0.3% of the ground. The reference marches with half steps, which covers a slope of 6.
- **Rock outcrops and cliffs:** boulders (48 m cells, 1.2–5 m) and crags (240 m cells, 16–40 m, mountains only). They are rounded boxes with 3D fBm displacement, under an assumed displacement slope of 0.8.
  - **Placement:** by hash. Each rock lives inside its own cell, and the field evaluates the 2×2 block of cells nearest to a point (†).
  - **Scatter cooked once (†):** each cell's base height is computed once by `place_rocks` (30,880 cells in 0.33 ms) and read from a buffer, as an engine places instances at load.
- **Ruins:** five structures built by CSG: an intact round tower (the tower-top view), a broken keep and its curtain wall, and two damaged towers. They have crenellations, windows, arches, floors and ragged breaks.
- **Filtering (†):** octaves fade between 8 and 4 pixel footprints per wavelength. The cook passes half a cell as its footprint, so the cache is prefiltered between 4 and 2 cells.
- **Normals (†)** come from the closest component only: the terrain from its height gradient, rocks and ruins by tetrahedral differences.

### The cache (`cook.wgsl`, `cache.wgsl`, `main.js`)

- **A clipmap of L levels,** each a cube of N³ bricks centred on the camera, stored toroidally. A brick is 8³ cells, kept as 9³ r16float samples so trilinear filtering never reads a neighbour.
  - **Main configuration:** N = 64, finest cell 12.5 cm (finest brick 1 m), 10 levels to 8 km (coarsest brick 512 m).
  - **A level is used only within its cube shrunk by one brick,** so the slab being replaced is never sampled.
- **Sparse:** a page table (one u32 per brick) marks each brick empty (storing the field at its centre) or allocated (pointing into a 3D atlas).
- **Cooking on the GPU,** with no readback:
  1. `classify`: the field's bound at the brick centre proves most bricks empty. The rest are candidates.
  2. `refine` (†): 27 threads per candidate evaluate a 3³ grid. Every point of the brick is within 0.43 of a brick of a sample, against 0.87 for the centre alone, so more bricks are proven empty.
  3. `allocate`: bricks that need a slot pop one from a free stack.
  4. `cook`: one workgroup per brick evaluates 729 samples.
     - **Normalized samples (†):** each sample is divided by the gradient magnitude of the cooked samples (from workgroup memory). That's a first-order distance, up to 1/K = 3.2 times the bound on gentle ground. It's an estimate, not a bound.
     - **The `hybrid` variant needs a bound,** so it runs on a strict cook.
  - Small `args` dispatches turn counters into indirect dispatch sizes.
- **Moving:** a level whose cube must move shifts by whole bricks; only the entering slab is classified and cooked.
- **Precision:** the march is camera-relative (level origins come from the CPU in f64); cook positions come from integer brick coordinates.
- **GPU safety:** a teleport (a full re-cook) runs as 144–176 awaited submits, each under about 8 ms wall (one, in a session's first teleport, took 78 ms). WebGPU zero-fills a new atlas lazily, which takes 50–90 ms wall, so that fill gets its own submit.

### Geometry variants (`march.wgsl`, `raster.wgsl`, `defer.wgsl`)

| Variant | Visibility | Normal |
|---|---|---|
| `analytic` | sphere-traces the field, with directional bounds (spike 02's heightfield bound; cell exits for the rocks) and a secant at terrain hits | the field, per pixel |
| `cache` | marches the cache: trilinear distance in allocated bricks, with over-relaxation 1.6 (†); skips to the exit of empty bricks | the cache |
| `cache_fn` | as `cache` | the field, per pixel (D-089) |
| `hybrid` | marches the strict cache as a **conservative bound** (cached distance minus a per-level error bound δ = √3·cell + omitted amplitude), and switches to the field near the surface. Exact under the field's facts. | the field, per pixel |
| `raster` | terrain only: a rasterized heightfield clipmap (geometry clipmaps with geomorphing), 32×32-quad blocks culled on the CPU, back faces culled (†). Levels are M×M quads with M = 8N, so its grid spacing equals the cache's cell. | a cooked normal texture |
| `raster_dn` (†) | as `raster` | the field, per pixel, in a deferred compute pass after rasterization |
| `raster_fn` (†) | as `raster` | the field, per pixel, in the fragment shader (pays for overdraw) |

- **Shared passes:** every variant writes the same G-buffer, so lighting and sky are shared.
  - **`light.wgsl`** is spike 05's slice, reported separately. It computes materials, the sun with a soft shadow marched through the cache, and sky ambient with cache ambient occlusion. It always uses the cache.
  - **`sky.wgsl`** has an analytic sky, aerial perspective (height-attenuated Rayleigh and Mie) and a fade over the last 15% of the view distance.
- **Capped rays:** a ray that runs out of steps counts as a hit only if it ended within 8 tolerances of the surface. Either way it's counted as a cap.

### Views

1. **Hillside:** eye height (1.7 m) on a hill 150 m up, looking across a valley to the main massif (1,700 m), 2–3 km away.
2. **Tower top:** standing 1.7 m inside the parapet of a 21 m tower on a hilltop, at a crenel, looking out and slightly down.
3. **Flyover:** 760 m up, 60 m/s, heading for the massif, re-cooking as it goes.
4. **Riding:** the hillside camera moving along the ground at 10 m/s.

### Correctness, precision, shimmer

- **Reference (as written):** the analytic field with half steps, a hit tolerance of 0.05 px instead of 0.25 px, and step caps of 6,000, at the same pixel centres, with the same lighting and sky passes.
  - **The hybrid's own reference:** the same, rendered with the strict cook's lighting. The lighting reads the cache, so a different cache changes the image.
- **Added after the first measurements (†), as extra measures, not criteria:**
  - a **same-tolerance reference** (brute-force steps, 0.25 px tolerance);
  - **depth agreement:** the share of pixels whose depth differs from the reference's by more than 1% and by more than 0.1%.
- **Precision:** rendering the same view with the camera 1.6–1.7 km (as placed), 5.5–7 km, 11–13 km and 46–47 km from the shader's origin. The world is shifted by multiples of the coarsest brick, so the content is identical and only f32 rounding differs. Each image is compared with the first.
- **Horizon shimmer:** consecutive frames of a moving camera, compared where the surface is more than 2 km away. The flyover sequence is placed so that level 7 (128 m bricks) re-centres between frames 5 and 6.

## The sweep

As run (†):
- **View distance:** 1, 4 and 8 km. 2 km was dropped to keep the run near 3 minutes.
- **Finest cell:** 6.25, 12.5 and 25 cm.
- **Clipmap resolution N:** 16, 32 and 64. N must be a power of two (the toroidal wrap is a mask), so 96 became 16.
- **Raster density M:** 256, 512 and 1,024 quads per level side.
- **Camera speed:** 0, 10, 30, 60 and 120 m/s.
- **Internal resolution:** 1920×1080 and 960×540. There's no upscaler, so there's no claim about upscaled quality.

## Layout

| File | What |
|---|---|
| `field.wgsl` | The world field: terrain, rocks, ruins, filtering, bounds, normals |
| `cache.wgsl` | The clipmap lookup (shared by the march, the lighting and the deferred pass) |
| `cook.wgsl` | `classify`, `refine`, `allocate`, `cook`, the rock scatter, and the heightfield cook for the raster path |
| `march.wgsl` | The marched geometry variants |
| `raster.wgsl`, `defer.wgsl` | The heightfield clipmap's shaders, and the deferred per-pixel field normal |
| `light.wgsl`, `sky.wgsl` | Lighting (spike 05's slice), sky and aerial perspective |
| `tools.wgsl` | Probing the field and the overview map (harness only) |
| `main.js` | Harness: the clipmap managers, views, timing, the reference comparisons |
| `results/` | Raw JSON from each run, and JPEG copies of key screenshots (PNGs are ignored) |

## Running it

```bash
python3 spikes/serve.py 8417
spikes/headless.sh 04-vista '#run' 1800
```

- **`#run`** measures everything (about 230 s when the lock is free) and saves `results/run-<timestamp>.json`, PNG screenshots and `DONE`.
- **`#quick`** renders each view and variant once (about 10 s).
- **Development modes:**
  - `#map` renders an overview of the terrain and slope statistics.
  - `#stats=hillside` prints steps, timings and reference differences for one view.
  - `#depth=hillside` writes depth-difference images.
  - `#teleports` checks the longest teleport submit for each sweep configuration.

**GPU safety (spikes/README.md):**
- Teleports and heightfield re-cooks are chunked.
- Every untimed render is banded into submits of about 40 ms.
- A variant whose geometry pass would exceed 40 ms is measured in timed bands for 3 frames, not back to back. The JSON marks those `tiled`.

## Results (2026-10-02)

**Setup:**
- MacBook Air M4 (8-core GPU), Chrome 154 headless via `spikes/headless.sh`, which holds a GPU lock, so one spike page runs at a time.
- The final run is `results/run-2026-10-02T02-57-50-612Z.json` (227 s). Earlier full runs are kept: `02-13-36` had a timestamp bug in frame totals; `02-21-36` and `02-31-50` are complete, but had fewer raster variants.
- **All times are indicative.** Under the lock no other page used the GPU, but the machine had run the other spikes back to back for hours. Two complete runs of the same code measured the same passes 15–35% apart (the hillside cache march: 26.5 and 30.6 ms). Treat absolute numbers as ±30%. Ratios within a run were stable: raster against marching the terrain cache was 17× in one run and 19× in another.
- GPU times are medians of 90 back-to-back frames after 30 warm-up frames (60 + 20 in the sweeps), except the `tiled` ones. Timestamps are quantized to about 65 µs.

### Verdict

| Kill criterion | Measured | Verdict |
|---|---|---|
| 1. World geometry ≤ 3.0 ms at 1080p (worst view; p95 when moving) | **No cache configuration passes criterion 2, so by the rule none counts.** Ignoring correctness, the pure cache march costs **30.6 ms** on the hillside (10× the slice), 21.6 ms on the tower, and **13.7 ms p95** on the flyover at 60 m/s. | **Fail** at 1080p, by 4.6–10×. Use the fallback. |
| (half resolution, for information) | 960×540: hillside 9.6 ms, tower 5.7 ms, flyover p95 3.4 ms | Would be Fail / Inconclusive / Inconclusive |
| 2. Matches the brute-force reference: ≤ 0.5/255 mean, ≤ 0.5% of pixels over 8/255, ≤ 0.1% capped | Pure cache: 3.1 / 14.1% (hillside), 5.9 / 15.5% (tower), 2.2 / 9.1% (flyover). Caps ≤ 0.0002%. **The exact analytic march fails too:** 2.2 / 7.9% on the hillside. | **Fail** for every path; see "Correctness" for why the criterion can't separate geometry error here |
| 3. Cache memory ≤ 512 MB | **368 MB** allocated (296–310 MB used) at N = 64, 8 km, 12.5 cm | **Pass** |
| 4. Sky and aerial perspective ≤ 1.0 ms | **0.39 ms** at 1080p, 0.07 ms at 540p | **Pass** |
| 5. Cache shows no precision artifacts at 11.6 km | 0 pixels over 8/255 at 11–13 km and at 46–47 km, max 7/255 | **Pass** |

**The fallback is triggered.** The raster heightfield clipmap draws the same terrain in **1.4–1.6 ms** at 1080p: 1.8–2.1 M triangles, 19× cheaper than marching the terrain-only cache. Its correctness against the reference is about the same as the cache's (below).

### Where the time goes

GPU ms at 1920×1080 (960×540 in brackets). World geometry = the geometry pass, plus the cache update for cache variants. `tiled` variants are too heavy to run back to back safely. Lighting is spike 05's slice and isn't in world geometry.

| View | `cache` | `cache_fn` | `analytic` (tiled) | `hybrid` (tiled) | Lighting | Sky |
|---|---|---|---|---|---|---|
| Hillside | **30.6** p95 34.1 (9.6) | 40.7 tiled (14.2) | 660 (161) | 680 (161) | 14.9 | 0.39 |
| Tower top | **21.6** p95 23.0 (5.7) | 30.7 (8.5) | 300 | 241 | 10.9 | 0.39 |
| Flyover, 60 m/s | **12.3** p95 13.7 (2.75, p95 3.4) | 26.3 (5.8) | 169 | 142 | 14.7 | 0.39 |
| Riding, 10 m/s | **34.5** p95 37.3 | 40.2 tiled | | | 15.7 | 0.39 |

- **The cache makes marching 14–22× cheaper than the field:** 31 against 660 ms on the hillside. A field evaluation costs about 2.8 ns at full occupancy: 15 octaves, rocks and ruins. A cache step costs about 0.2 ns (an estimate, from steps × pixels ÷ time).
- **It's still far from 3 ms.** The cost tracks how much of the screen is near, grazing ground. Half the hillside's pixels hit level 0, the 12.5 cm cells within 31 m.
- **Per-pixel field normals (`cache_fn`) add 9–14 ms at 1080p.**
- **The exact hybrid saves almost nothing over the analytic march** (680 against 660 ms on the hillside). Grazing rays spend most of their steps inside the cache's error band (δ is 2–3 cells), where the field must be evaluated: 61 field evaluations per pixel against the analytic's 111.
- **Ablations (hillside, 1080p):**
  - without over-relaxation: 32.0 ms (+5%);
  - with the strict, un-normalized cook: 43.8 ms (+43%);
  - before the bitmask toroidal wrap and the level carried along the ray: about 87 ms, in an earlier run.
- **Paced at 60 Hz** (busy-wait, flyover, cache):
  - 1080p: frame median 31.1 ms; 150 of 150 frames over 16.7 ms.
  - 540p: median 13.6 ms; 3 of 150 frames over 16.7 ms (lighting included).

### Why marching costs this much: steps per pixel

| View | Variant | Steps/px | of which empty bricks | Field evaluations/px | Capped | Sky |
|---|---|---|---|---|---|---|
| Hillside | `cache` | 70.8 | 41.6 | 0 | 0.0002% | 6.0% |
| Hillside | `analytic` | 110.6 | | 110.6 | 0.002% | 5.9% |
| Hillside | `hybrid` | 235.6 | 45.3 | 61.3 | 0.003% | 5.9% |
| Hillside | reference | 254.7 | | 254.7 | 0 | 6.0% |
| Tower | `cache` | 44.3 | 25.6 | 0 | 0 | 2.2% |
| Flyover | `cache` | 26.3 | 16.5 | 0 | 0 | 1.5% |

- **60% of the cache march's steps cross empty bricks,** about two per level per ray.
  - **Skipping to the exit of a coarser level's empty brick was tried:** that level works as an implicit octree. It removed few steps near the ground, where coarse bricks are rarely empty, made each step dearer, and cost 20% more time. It was reverted.
- **The terrain's bound drives the rest.** With K = 0.32, a strict cook steps a third of the vertical gap above flat ground. Normalizing at cook time took the march from 43.8 to 30.6 ms.

### Cooking

**Teleports** (a full re-cook of all 10 levels, 2.6 M bricks classified):

| View | Bricks cooked | Candidates | Proven empty by the 3³ test | Classify ms | Cook ms | GPU total ms | Bricks per cook-ms | Used / allocated MB |
|---|---|---|---|---|---|---|---|---|
| Hillside | 193,976 | 257,128 | 63,152 | 22.3 | 274 | 297 | 707 | 296 / 368 |
| Tower top | 204,150 | 255,619 | 51,469 | 22.8 | 308 | 332 | 663 | 310 / 368 |
| Flyover | 105,489 | 139,360 | 33,871 | 12.4 | 130 | 143 | 809 | 173 / 368 |

**Moving** (flyover path, cache, 1080p; cook = classify + refine + allocate + cook):

| Speed (m/s) | World median | p95 | max | Cook p95 | Cook max | Bricks/frame p95 | max | Bricks per cook-ms |
|---|---|---|---|---|---|---|---|---|
| 0 | 10.9 | 11.1 | 12.3 | 0 | 0 | 0 | 0 | |
| 10 | 11.3 | 12.5 | 12.7 | 0.07 | 0.66 | 0 | 290 | 316 |
| 30 | 11.0 | 12.6 | 12.8 | 0.20 | 1.38 | 37 | 616 | 271 |
| 60 | 11.9 | 12.8 | 14.2 | 0.72 | 2.16 | 335 | 973 | 330 |
| 120 | 11.5 | 13.3 | 15.3 | 1.51 | 2.95 | 636 | 1,336 | 350 |

- **Most frames cook nothing;** the cost comes in spikes, when several levels shift in one frame.
- **Riding at 10 m/s near the ground costs more than flying at 60 m/s:** cook p95 1.5 ms, max 2.7 ms, up to 1,166 bricks. The finest levels shift often there, vertically as well as horizontally.
- **Cooking keeps up at every speed tested:** no level lagged.
- **Throughput:** about 700–800 bricks per ms in teleports, 270–370 in per-frame batches. Each brick is 729 field evaluations, so 0.5 G evaluations per second in teleports.
- **Spikes cost a lot of the slice.** At 60 m/s the p95 cook is 0.7 ms (a quarter of the 3 ms slice), and the worst frame is 2.2 ms. Spreading the work over frames, with a budget the code supports but the run didn't use, would trade those spikes for lag in the coarse levels (untested).

### Memory

- **The main cache:** 368 MB allocated (MiB throughout): the atlas is 342 MB of it, the page tables 10 MB, the job lists 16 MB. 296–310 MB is in use near the ground.
- **The allocation band is thick:** about 4–5 bricks per column per level, because the terrain's bound is `0.32 × (y − h)`. A brick near the surface can only be proven empty from about 1.4 bricks away.
  - The 3³ refinement removed 20–25% of candidates.
  - A tighter, local Lipschitz fact would thin the band further (a hypothesis).
- **The raster clipmap:** 27 MB at M = 512 (8 MB at 256, 97 MB at 1,024).

### Correctness

Against the brute-force reference (the criterion as written), lit images:

| View | Variant | Mean /255 | Pixels over 8/255 | Depth off by > 1% | Depth off by > 0.1% |
|---|---|---|---|---|---|
| Hillside | `analytic` | 2.19 | 7.9% | 4.7% | 82% |
| Hillside | `cache` | 3.11 | 14.1% | 0.53% | 23% |
| Hillside | `cache_fn` | 1.54 | 5.2% | 0.53% | 23% |
| Hillside | `hybrid` (own reference) | 2.48 | 9.4% | 4.7% | 82% |
| Tower top | `analytic` | 0.74 | 2.2% | 0.62% | 41% |
| Tower top | `cache` | 5.89 | 15.5% | **7.1%** | 31% |
| Tower top | `cache_fn` | 3.57 | 6.0% | 7.1% | 31% |
| Tower top | `hybrid` | 0.71 | 1.7% | 0.62% | 41% |
| Flyover | `analytic` | 1.31 | 4.0% | 0.22% | 49% |
| Flyover | `cache` | 2.22 | 9.1% | 0.11% | 3.9% |
| Flyover | `cache_fn` | 0.83 | 2.3% | 0.11% | 3.9% |

Against the **same-tolerance** reference (added): `analytic` 0.64 / 0.88%, `cache` 3.70 / 17.1%, `cache_fn` 2.77 / 10.9% (hillside).

- **The written criterion can't separate geometry error for this content.**
  - **The exact analytic march fails it,** and the same-tolerance reference shows why: most of the difference is the reference's 5× tighter hit tolerance. A ray stops on a 0.25 px shell instead of a 0.05 px one, which moves the shaded point by about a quarter of a pixel.
  - **Fractal terrain magnifies that.** It has slope variation at every scale down to the filter's cutoff, so a quarter-pixel shift changes the normal enough to move 2–8% of pixels by more than 8/255.
  - **At grazing angles** the shell also lengthens the hit distance: 4.7% of hillside pixels move by more than 1% in depth.
  - The thresholds stay as written; the result is reported as a fail.
- **What does separate them is depth near architecture.**
  - **The pure cache rounds the tower's merlons and crenels,** which are 2 m from the eye in 12.5 cm cells (`results/tower-cache.jpg` against `tower-ref`): 7.1% of pixels are off by more than 1% in depth.
  - **On terrain, the cache's depth agrees with the reference better than the analytic march does** (0.53% against 4.7%). That's probably because its normalized distance makes the tolerance shell thinner (untested).
- **Shading is where the pure cache loses most.** Normals from 12.5 cm–64 m cells are visibly blurrier; `cache_fn` (field normals) halves the error.
- **Step caps aren't a problem:** at most 0.003% of pixels.

### The raster comparison (terrain only)

Terrain-only content (no rocks or ruins); hillside view (flyover in brackets); 1080p GPU ms; 1.8 M (2.1 M) triangles after culling.

| Variant | World geometry | p95 | 540p | Lit vs reference | Depth > 1% |
|---|---|---|---|---|---|
| `raster` (normal texture) | **1.57** (1.51) | 1.77 (1.57) | 1.05 (1.25) | 2.80 / 12.6% | 0.59% |
| `raster`, no back-face culling | 1.83 (1.64) | 2.03 (1.97) | | | |
| `raster_dn`: deferred field normal | 13.0 (10.7); the normal pass alone is 11.5 (9.2) | 14.1 (11.5) | 3.7 (3.4) | **1.44 / 4.5%** | 0.59% |
| `raster_fn`: forward field normal | 24.5 (19.7) | 25.8 (21.7) | 8.1 (7.4) | | |
| `cache_t`: marched cache | 29.4 (10.3) | 32.3 (11.4) | 7.9 (2.6) | 3.20 / 14.2% | 0.59% |
| `analytic_t` (tiled) | 360 | | | 2.20 / 8.0% | 4.8% |

- **Raster is 19× cheaper than marching the same terrain on the hillside,** and 7× on the flyover. Its correctness is the same as the cache's at equal cell size.
- **Its moving cost is negligible:** the heightfield re-cook is 0.07 ms median and 0.13 ms p95 at 60 m/s. A full re-cook takes 13–14 ms (in chunks) at M = 512.
- **Density sweep** (raster only, 1080p):

  | M | Triangles drawn | Hillside ms | Flyover ms | Memory MB |
  |---|---|---|---|---|
  | 256 | 0.59–0.68 M | 0.66 | 0.66 | 8 |
  | 512 | 1.8–2.1 M | 1.44 | 1.51 | 27 |
  | 1,024 | 5.5–6.6 M | 3.60 | 4.13 | 97 |

  At M = 512 a triangle is 6–12 pixels across at distance (an estimate from the level geometry), the same as the cache's cells.
- **Per-pixel field shading is the expensive part,** not visibility.
  - **Deferred, once per pixel,** the terrain's normal from 4 height evaluations costs 9–12 ms at 1080p and 2–3 ms at 540p. That's 3–4× the whole world slice.
  - **In the fragment shader it costs twice that:** overdraw, and the coarse levels' `discard`, defeat hidden-surface removal.
  - **The normal texture is cheap but blurry near the camera:** 12.5 cm per texel, 12.6% against 4.5% of pixels over 8/255.

### Sweeps

Cache, hillside, 1080p. Depth is compared with the main reference's depth; lit differences are confounded, because the lighting reads each configuration's own cache.

| Sweep | Value | Levels | World ms | Bricks | Used / allocated MB | Teleport GPU ms | Depth > 1% / > 0.1% | Lit (confounded) |
|---|---|---|---|---|---|---|---|---|
| View distance | 1 km | 7 | 26.6 | 129,040 | 198 / 257 | 248 | | |
| View distance | 4 km | 9 | 31.1 | 172,148 | 263 / 331 | 307 | | |
| View distance | 8 km | 10 | 30.5 | 193,976 | 296 / 368 | 337 | 0.53% / 23% (main) | |
| Finest cell | 6.25 cm | 11 | 34.2 | 211,784 | 323 / 404 | 356 | 0.39% / 13% | 3.9 / 23% |
| Finest cell | 25 cm | 9 | 28.1 | 176,111 | 268 / 331 | 282 | 1.6% / 37% | 4.3 / 23% |
| N | 16 | 12 | 19.9 | 14,623 | 22 / 30 | 26 | 3.1% / 48% | 12.7 / 65% |
| N | 32 | 11 | 24.1 | 53,452 | 80 / 103 | 89 | 0.9% / 30% | 6.4 / 31% |

- **View distance barely matters:** 26.6 ms at 1 km, 30.5 ms at 8 km. The near field dominates. Memory and teleport time grow by about 40%.
- **The finest cell** trades near accuracy for cost: 6.25 cm costs +12% time and +10% memory.
- **N is the lever for distant quality.** It sets cells per pixel everywhere beyond level 0.
  - **N = 16** is 35% cheaper, but blobby (`results/sweep-N16-cache.jpg`), with 3% of pixels off by more than 1% in depth.
  - **Memory scales with N²:** 30, 103 and 368 MB.
  - **N = 128 wasn't run:** its estimated memory, 1.2–1.5 GB (about 4× N = 64), breaks criterion 3.

### Precision

| View | Camera from origin | `analytic` mean / over 8 / max | `cache` mean / over 8 / max |
|---|---|---|---|
| Hillside | 7.0, 12.8, 47.5 km | 0 / 0 / ≤ 5 | 0 / 0 / ≤ 7 |
| Tower top | 5.5, 11.1 km | 0.016 / 0.012% / 185 | 0.002 / 0 / 5 |
| Tower top | 45.8 km | 0.042 / 0.054% / 185 | 0.009 / 0 / 7 |

- **No visible precision artifacts up to 47 km** for either path, at this content's scale (12 cm finest octave, nearest surfaces 1–3 m away).
- **The analytic path's few differing pixels** are on the merlons' edges, 2 m from the eye.
- **The cache is immune by construction:** camera-relative marching, integer-grid cooking.
- **Caveat:** the shift captures the rounding of ray positions, not rounding inside the noise's lattice arithmetic at large coordinates. That's estimated to be of the same order, ulp(|p|): 4 mm at 47 km. Content finer than about 1 cm, or eyes closer than about 0.5 m, would show artifacts sooner (an estimate).

### Horizon shimmer

Mean change per frame, and the share of far pixels (over 2 km) changing by more than 8/255:

| Sequence | `cache` | `cache_fn` | `analytic` |
|---|---|---|---|
| Riding, 10 m/s | 0.049 / 0.04% | 0.059 / 0.05% | 0.052 / 0.06% |
| Flyover, 60 m/s | 0.22 / 0.20%; the level-7 re-centring frame 0.30 / **0.26%** | 0.39 / 0.77%, re-centring frame 0.81% | 0.42 / 0.93%, flat |

- **Far-field stability is good, and the pure cache is the most stable.** Its prefilter at cell scale removes detail the analytic path still shows.
- **The level-7 re-centring is visible in the numbers** as a 30% bump in changed far pixels on that frame (0.20% → 0.26%). That's a small pop, not a flash.
- **Most of the flyover's change is parallax,** so these are upper bounds on instability.
- **Not tested:** pops when levels lag a fast camera, since no level lagged.

### How it looks

Key screenshots, as JPEG in `results/`: `hillside-cache_fn`, `tower-ref`, `tower-cache`, `flyover-cache_fn`, `hillside-terrain-raster`, `hillside-terrain-raster_dn`, `hillside-levels`, `hillside-steps-cache`, and the `-diff` images.

- **The flyover is the best of it:** side-lit relief, green alpine meadows and pale rock faces in low sun. It reads as a landscape.
- **The hillside is pleasant but not AAA.**
  - The massif is a pale, low-detail wall of snow and rock, and two dark crags on it read as blemishes.
  - The meadow is a bumpy green blanket with no grass or stones (vegetation is spike 03's job).
  - Haze is subtle; distant layers separate less than they should.
- **The tower top frames the valley well,** but the masonry is flat CG tiling with no relief or weathering.
  - **In the pure cache the merlons melt:** rounded corners and a wavy crenel sill (`tower-cache.jpg`). Near architecture can't come from a 12.5 cm cache.
- **Pure-cache shading is soft everywhere,** because its normals are as coarse as its cells. Field normals (`cache_fn`, `raster_dn`) bring back the crisp relief, at 9–14 ms.
- **Overall:** the cache and the field can draw a convincing vista. The beautiful forest-and-mountain look the flagship needs isn't shown here, and wasn't attempted in full: no vegetation, simple materials, one light.

### Changes made during development, before the final run

The criteria didn't change. In order:
1. **Terrain.**
   - Ridged first octaves were dropped: their fold made the derivative damping discontinuous (a slope of 27,000).
   - The assumed slope bound went from 2 to 3. At 2, 5.8% of the terrain violated it; at 3, 0.3% does.
   - Mountain amplitudes were reduced to make that true.
2. **Rocks.**
   - Each point now evaluates the 2×2 nearest cells, and the scatter's base heights are cooked once. The single-cell border bound drew a grid in normals and occlusion, and recomputing base heights per evaluation doubled the field's cost.
   - Rocks are sunk to the lowest ground under them.
3. **Normals** come from the closest component only. The rocks' cheap bounds had leaked into finite differences as rings.
4. **The cook gained `refine`** (the 3³ test, −24% bricks) and **gradient-normalized samples** (−30% march time).
5. **March cost:** the toroidal wrap became a bitmask, the level is carried along the ray, and over-relaxation 1.6 was added. With item 4 they took the hillside cache march from about 87 to about 26 ms in that session (development runs).
6. **Bandlimits** fade at 8→4 footprints instead of 4→2. The old filter made single-sample shading too sensitive to sub-pixel hit differences.
7. **Capped rays** near the surface count as hits.
8. **GPU safety, after the desktop reset:** chunked teleports, banded renders, tiled measurement of heavy variants, batched pipeline creation.
   - **After the final run,** teleports were split further (eighths → sixteenths) and the atlas's lazy zero-fill moved to its own submit. In the final run, two sweep teleports had one chunk of 102 and 111 ms wall, because the fill landed in them. Teleport times are sums of chunks, so the results are unaffected.
9. **Tried and reverted:**
   - hierarchical empty-brick skipping (slower);
   - secant iterations at terrain hits (diverged without a bracket).
10. **Measurement fixes:**
    - The first runs undercounted steps, because a banding bug skipped rows in the statistics pass.
    - Frame totals used a timestamp from an empty pass, which Chrome doesn't write; they're now sums of passes.
11. **Added correctness measures:** the same-tolerance reference and depth agreement (see "Correctness").
12. **Raster, at the coordinator's request:** back-face culling, the forward and deferred field-normal variants, the density sweep and triangle counts.
13. **Not measured:** cold pipeline creation. The salt constant is unused and the compiler strips it, so the "cold" numbers (111 ms for the scene's 9 pipelines) are cache hits. All 53 pipelines were created in 213 ms, warm.

### What this means for the design

Nothing is recorded in `decisions.md`; that's the owner's call.

- **Marching a world cache as primary visibility doesn't fit the world slice at native 1080p on the M4.**
  - It's 4–10× over (12–31 ms against 3 ms), and the near field dominates.
  - At 960×540 it's 2.8–9.6 ms, which is only inconclusive for open views.
  - This is consistent with the town spike's 3× over on a different world.
- **The fallback works for terrain:** a rasterized heightfield clipmap cooked from the field costs 1.5 ms at 1080p, re-cooks for 0.1 ms per frame while moving, and uses 27 MB.
  - **A heightfield can't hold** rocks with overhangs, crags or ruins; those need extracted meshes (spike 01's path). Their cost here is unmeasured.
- **Per-pixel field shading of fBm terrain is too expensive** to be the default for the world: 9–12 ms at 1080p deferred.
  - Terrain shading inputs (normals, material weights) must be cooked into textures. The field's per-pixel role is near detail, at a bounded screen share.
  - This qualifies D-089's per-pixel default, which was measured on creatures.
- **Cooking on device is cheap and keeps up:**
  - about 0.3 s of GPU for a full 8 km cache;
  - under 1 ms p95 per frame at 60 m/s, 1.5 ms at 120 m/s;
  - spikes up to 3 ms, which want spreading over frames.
- **For the language** (hypotheses):
  - **Lipschitz facts drive memory and steps.** A global slope bound of 3 makes the brick band 3× thicker than an exact distance would. Scoped or local facts (D-092), or a declared "exact distance" kind, would pay off directly.
  - **Bandlimits matter at 1 sample per pixel.** Fading at 4–8 footprints was needed to keep shading stable.
  - **Scattered instances want a cooked placement step,** not re-evaluation per query.
  - **Nothing here needs engine knowledge in the compiler (D-050):** bounds, bandlimits and cooking are general field facts.

### What carries over to the hybrid architecture

The likely architecture rasterizes what's big on screen and uses fields for lighting, distance, small instances and cooking.

**Carries over:**
- **The rasterized terrain clipmap:** 1.4–1.6 ms at 1080p with M = 512, 0.66 ms at M = 256; 0.1 ms per frame of re-cooking.
- **The cook and clipmap machinery:**
  - the GPU classify, refine and cook pipeline at about 700 bricks/ms;
  - incremental re-centring under 1 ms p95 at 60 m/s;
  - chunked teleports;
  - camera-relative lookup, immune to f32 range;
  - about 370 MB for an 8 km, N = 64 cache, or about 100 MB at N = 32.
  
  A lighting cache could likely be coarser than one used for visibility, since shadows and occlusion tolerate coarse cells (a hypothesis).
- **The finding that shading must be cooked** (9–12 ms for per-pixel field normals on terrain).
- **The Lipschitz-band and bandlimit lessons.**

**What changes:**
- **Primary visibility leaves the field.** The 12–31 ms march disappears from the world slice; near architecture becomes extracted meshes, because the cache melts it.
- **The cost moves to lighting.** This spike's cache-marched shadow (one soft shadow ray of about 45–53 steps) plus 4 occlusion taps already costs 8–16 ms at 1080p, against spike 05's 3.5 ms slice. Lighting from the cache needs half resolution or temporal amortization, and that's now the cache's real test (untested here).
- **Cooking must also feed rasterization:** heightfield tiles, extracted meshes, and normal and material textures. Mesh extraction for rocks and ruins at world scale is unmeasured.

## Quiet-machine rerun (2026-10-02)

Run file: `results/run-2026-10-02T04-08-35-318Z.json` (221 s): serial, after a cool-down, Chrome 154 headless, alone on the GPU. No pass ran slower than in the README's final run: passes measured back to back in both runs were 5–30% faster, tiled ones 0–5%. GPU ms at 1080p, 540p in brackets; the variants are in the order `cache` / `cache_fn` / `analytic` / `hybrid`, and `analytic` and `hybrid` are tiled.

| Metric | README value | Quiet value |
|---|---|---|
| Hillside, world geometry | 30.6 p95 34.1 / 40.7 (tiled) / 660 / 680 (9.6 / 14.2 / 161 / 161) | 26.3 p95 26.7 / 38.5 / 660 / 653 (7.0 / 9.9 / 159 / 154) |
| Tower top, world geometry | 21.6 p95 23.0 / 30.7 / 300 / 241 (5.7 / 8.5 / not reported) | 16.1 p95 16.2 / 24.2 / 285 / 230 (4.2 / 6.1 / not in the JSON) |
| Flyover at 60 m/s, world geometry | 12.3 p95 13.7 / 26.3 / 169 / 142 (2.75 p95 3.4 / 5.8 / not reported) | 9.8 p95 10.4 / 20.3 / 162 / 140 (2.56 p95 3.08 / 5.0 / not in the JSON) |
| Terrain only, `raster` / `cache_t`: hillside; flyover | 1.57 / 29.4 (19×); 1.51 / 10.3 (7×) | 1.31 / 24.8 (19×); 1.38 / 8.7 (6.3×) |
| Cooking per frame, p95 / max: 60 m/s; 120 m/s | 0.72 / 2.16; 1.51 / 2.95 | 0.66 / 1.77; 1.25 / 2.29 |
| Sky and aerial perspective | 0.39 (0.07) | 0.33 (0.07) |
| Cache memory, allocated (used) | 368 MB (296–310) | 368 MB (296–310) |
| Correctness, `cache` mean / pixels over 8/255: hillside, tower, flyover | 3.1 / 14.1%, 5.9 / 15.5%, 2.2 / 9.1%; caps ≤ 0.0002% | the same; every variant is within 0.001/255 and 0.01% of pixels of the README's final run |
| Precision, hillside `cache` max /255 at 7.0, 12.8, 47.5 km | ≤ 7 | 12, 7, 7; still 0 pixels over 8/255 at 11–13 and 46–47 km |

**No verdict changes** under the criteria above: criterion 1 is still Fail at 1080p (no configuration passes criterion 2, and the pure cache's worst view, the hillside at 26.3 ms, is 8.8× the slice rather than 10×); the 540p row is still Fail / Inconclusive / Inconclusive (7.0, 4.2 and p95 3.08 ms, the last within two timestamp ticks of Pass); criterion 2 fails, criteria 3–5 pass, and the fallback stays triggered.
