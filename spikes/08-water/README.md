# Spike 08: water, ray-marched

*A measurement spike for the flagship's forest lake. It's throwaway code, not the start of the engine.*

## The question

Can water with **true reflections and refraction** be ray-marched inside the water slice of the frame budget: **1.0 ms at 1920×1080 on the M4, with water covering ~30% of the screen?** If not, where's the limit: at what coverage, reflection resolution and reflection length does it break?

"True" means secondary rays marched through the same fields as the rest of the world, not screen-space tricks: a reflection ray from every water pixel (or every 2×2 or 4×4 block of them) that can find trees, the tower, rocks, terrain or sky, and a refraction ray that finds the lake bed through absorbing water.

## The scene

A forest lake fed by a stream that runs over rocks and through a small waterfall (a 1.5 m drop).

- **Water:** a lake at y = 0 and a stream whose level follows the channel and steps down at the fall. The surface is a bandlimited field: summed directional waves plus noise, with each component faded by the pixel footprint projected along its own direction. Wind patches ("cat's paws") modulate the ripples, so parts of the lake are mirror-calm and parts are ruffled.
- **The world (fields, no triangles):**
  - an analytic heightfield terrain: lake basin with a shallow shelf, shore, hills, an escarpment where the stream falls, a carved gully;
  - a conifer-and-birch forest on a jittered 4.5 m grid (each tree confined to its cell, so a ray walks cells and evaluates one tree at a time);
  - a stone tower with battlements and a stair turret on the far shore;
  - boulders along the stream, at the waterfall's lip and in the shallows.
- **Simple trees are fine:** trunks plus canopy blobs and cones. 03-forest does the real foliage.
- **Two views:**
  - `shore`: from the south shore at eye height (1.7 m), afternoon sun.
  - `sunset`: low across the water (0.6 m), with a low sun (13°) just above the far treeline, so its glint is on the water.

## Kill criteria, written before measuring

The **water slice** is the GPU time of the passes that exist only because of water:

> water = `water_surface` + `reflect` + `refract` + `composite`

The opaque `scene` pass is not charged to water, even though water clips it (primary rays stop at the water surface, so the lake bed isn't shaded twice). That makes the figure conservative. The marginal cost, frame with water minus the same frame with a dry lake bed, is reported too.

1. **Budget** (spikes/README.md's verdict rule), at native 1080p on the M4, GPU timestamps under sustained load, at 30% water coverage (interpolated from the coverage sweep in the `shore` view):

   | Water slice | Verdict |
   |---|---|
   | ≤ 1.0 ms | **Pass** |
   | 1.0–2.0 ms | **Inconclusive:** optimization or reallocating budget might save it |
   | > 2.0 ms | **Fail:** use the fallback |

2. **Which configuration counts:** the cheapest one whose image is *correct*. Correct means, against a brute-force reference of the same content (below), over water pixels:
   - mean |difference| ≤ 1.0/255, and
   - ≤ 1% of water pixels off by more than 8/255, and
   - step-cap hits on ≤ 0.1% of water pixels' secondary rays.

   Reduced-resolution and proxy reflections are measured and reported with their own quality numbers. If they're cheaper but miss the bar, they're reported as "with visible quality loss" and the verdict says so; they don't count toward a pass on their own.

3. **The limit:** for each configuration, the water coverage at which the water slice reaches 1.0 ms and 2.0 ms (linear fit to the coverage sweep).

**Also reported, not pass/fail:**
- The cost per water pixel of reflection rays and refraction rays (ns per pixel), at full and half resolution.
- The whole frame at a 960×540 internal resolution (all passes at half resolution per axis). No upscaler is implemented, so no upscaled quality is claimed.
- A 60 Hz paced run (busy-wait).
- Filtering error: the fast path (one sample per pixel, footprint-filtered waves) against a 9-sample supersampled reference with a third of the footprint. Water aliasing (glint sparkle, ripple shimmer) is a beauty problem, but neither spike 01 nor 02 anti-aliases, so this is reported, not a criterion.
- The `sunset` view, which is harder for reflections: grazing rays travel across the lake into the far forest.

## The fallback if it fails

Screen-space reflections plus a reflection probe (a cube map of the lake surroundings, updated rarely), with refraction taken from the already-rendered scene: the opaque pass renders the lake bed, and water samples that colour and depth with a normal-based offset. That's what shipped games do. It's not implemented here.

## Method

Six compute passes per frame, each with its own timestamps. The water slice is every pass except `scene`.

| Pass | Resolution | What |
|---|---|---|
| `water_surface` | full | Where each ray meets the water: the lake's mean plane, or the stream (a heightfield march with a scoped slope bound, steep only in the chute; the bare level first, the ripples only within their amplitude of it). Writes the hit distance. |
| `scene` | full | The opaque world, with primary rays clipped at the water. Soft sun shadows. Writes colour and the visible-water mask. Not charged to water. |
| `water_gbuf` | full | For visible water only: the hit refined by two Newton steps on the wave field, then the filtered normal and the slope variance filtering removed. Written once (16 bytes per water pixel) and read by the passes below. |
| `reflect` | 1, ½ or ¼ | The reflected ray, marched through the world up to a maximum length; shaded without shadow rays; sky beyond. At reduced resolution each texel intersects and filters the water at its own (wider) footprint. |
| `refract` | 1 or ½ | The refracted ray, marched to the bed or rocks through absorbing water; the bed lit through the water with procedural caustics; depth fog along the view path. |
| `composite` | full | Fresnel, upsampling of reflection and refraction (bilinear over valid water texels), foam, sun glint with roughness from the filtered-out waves (Toksvig-style), a sun shadow ray only where glint or foam needs it, aerial perspective, tonemapping. |

**The world, as the tracer sees it:**
- **Terrain** is cooked once from its analytic field into a 2048² float heightmap over 300 m (14.6 cm texels, bilinear). It's marched with a directional slope bound (spike 02). The bound is a scoped fact (D-092): 0.72 almost everywhere, 1.6 in two boxes around the stream's valley and the escarpment. A probe re-checks both on every run, on the heightmap's bilinear interpolant (1M points). This is an assumption about the engine's representation, which is spike 04's subject. In an early version, marching the analytic field directly cost 9.3 ms for reflections and 4.3 ms for refraction, against 5.9 and 2.0 ms once the terrain was cooked (one indicative run each).
- **Trees** sit one per 4.5 m cell, confined to it. Terrain and trees are traced in one walk along the ray: cell by cell, near to far, with the terrain march advanced through each cell before the cell's tree is marched. Both stop at the first hit. Blocks of 8×8 cells whose tallest tree the ray passes above are skipped whole.
- **Rocks:** per-ray masks from bound spheres, as in spike 02. **Tower:** a bounding cylinder.
- **Noise** (leaf and rock displacement, stone, ripples, caustics, foam, clouds) is the same lattice value noise as the analytic version, cooked into tileable textures (96³ for 3D, 512² with its gradient for 2D) and sampled with hardware filtering: one fetch instead of 4–8 hashed lattice points. Its slope bound (3.7 per unit of frequency) is the probe's measured maximum.
- **Sun shadows** are cast by the trees' base shapes (tiers and crowns without leaf displacement). These are true distance bounds, so a low sun's ray crosses the forest within its step budget.

**Filtering.** Each wave component's amplitude is faded by `1 − smoothstep(0.25, 0.5, footprint × frequency)`. The footprint is projected along the wave's own direction: a pixel's footprint on the water is stretched along the view by 1/|d.y|, or by the stream's tilted surface in the chute. Filtering skips the trigonometry for faded-out components. The slope variance removed by filtering becomes roughness, which widens the glint. Reduced-resolution passes filter at their own footprint, so their normals are bandlimited for their own sample rate.

**Other details:**
- **Reflection length:** a ray that reaches the maximum length without a hit returns a far fallback: a dark treeline colour if it's still below canopy height, sky otherwise.
- **Proxy reflections:** the same trace against a coarse version of the world: trees as plain cones and ellipsoids, a plain cylinder for the tower, smooth rocks, no displacement.
- **The reference (`ref`):** the same content and shading, with:
  - every march at half step, a twentieth of the hit tolerance, and step caps of 2,000–3,000;
  - full-resolution reflection and refraction, with a reflection length of 400 m;
  - the water surface found by a dense march down through the wave slab, instead of the mean plane and Newton steps.
  - It renders in 96² tiles, one awaited submission each (spikes/README.md's GPU rules); the longest submission is logged.
- **The supersampled reference (`ss`):** `ref` with 3×3 stratified samples per pixel and a third of the footprint, averaged after tonemapping.
- **Stats** come from a counting variant of each pass (atomics): water pixels, steps per ray, cap hits, what reflection rays hit.

**Timing:**
- 30 warm-up frames, then 90 back-to-back frames with timestamps; medians and p95.
- Sweeps time *water-only frames*: one whole frame, then frames of the five water passes only. The scene pass's output (colour and the visible-water mask) doesn't change for a fixed view, and skipping it keeps the run short.
- The main configurations are also timed in whole frames, for the frame total and the marginal cost against a dry lake bed.
- Each configuration is measured in two interleaved rounds, and the round with the lower median is reported, both rounds listed. This is because the shared, fanless machine drifted by tens of percent during development.

### Sweeps

| Parameter | Values |
|---|---|
| Water coverage | `shore` view tilted from looking slightly up to looking down: ~10% to ~70% |
| Reflection resolution | 1, ½, ¼ per axis |
| Reflection length | 25, 50, 100, 200 m |
| Reflection scene | full, proxy |
| Wave detail | low (6 waves), mid (12 waves + 2 noise octaves), high (24 waves + 4 octaves) |
| Refraction resolution | 1, ½ |
| Internal resolution | 1920×1080, 960×540 |

## Layout

| File | What |
|---|---|
| `world.wgsl` | Noise, terrain, the stream's course and level, the lake's shape, tree placement rules |
| `scene.wgsl` | Bindings, sky, trees, tower, rocks, tracing, shading |
| `water.wgsl` | Waves, filtering, the water intersection, foam, absorption, caustics |
| `passes.wgsl` | The six passes |
| `place.wgsl` | Cooking, once: the heightmap, the noise tables, the tree grid and block tops, rock heights; the slope probe |
| `blit.wgsl` | Copies the frame to the canvas (not measured) |
| `main.js` | Harness: pipelines, views, timing, sweeps, the reference comparison |
| `results/` | Raw JSON from each run, console log, screenshots (PNG ignored by git; JPEG copies kept) |

## Running it

```bash
python3 spikes/serve.py 8417
spikes/headless.sh 08-water '#run' 600
```

`#quick` renders each view once and saves screenshots (~10 s). Development modes: `#perf:<configs>` times configurations in both views; `#refcheck:<view>,<configs>` compares them with the reference and saves diff images.

## Results (2026-10-01, indicative)

**Setup:**
- MacBook Air M4 (8-core GPU), headless Chrome 154 via `headless.sh`. The run is `results/run-2026-10-02T02-18-52-071Z.json` (161 s).
- **All timings are indicative.** Other spikes used the GPU between my runs, and the fanless machine drifted by tens of percent during the session. Each number is the better of two interleaved rounds, and both rounds are in the JSON; they differ by up to ~30%. The final measurement on a quiet machine is still to come. The verdict below is far enough from the thresholds that this doesn't change it.
- Correctness and stats don't depend on timing.
- **GPU safety:** the longest single submission in the run was 54 ms (a whole fast frame). References render in 96² tiles, one awaited submission each; the longest tile seen during development took 15 ms.

### Verdict

| Kill criterion | Measured | Verdict |
|---|---|---|
| 1. Water slice ≤ 1.0 ms at 30% coverage, 1080p (linear fit to the `shore` sweep) | **full 14.0 ms**; reflection at ½ 6.0; reflection and refraction at ½ (`r2q2`) 5.7; reflection at ¼ with refraction at ½ (`r4q2`) 3.6 | **Fail** for every configuration (> 2.0 ms) |
| 2. Correct against the reference (water pixels: mean ≤ 1.0/255, ≤ 1% over 8/255; caps ≤ 0.1%) | **No timed configuration passes.** The closest, `full`: shore 0.78 and 2.1%, sunset 0.49 and 1.4%; reflection rays capped on 0.41% and 0.35% of water pixels. | Even the full path misses the bar (see *Quality*) |
| 3. The limit: where the slice reaches 1.0 ms | Even the cheapest, `r4q2`, costs 2.75 ms at the lowest coverage measured (22%). By its cost per water pixel (5.9 ns), 1.0 ms buys **~8% coverage**. `full` (20.6 ns per pixel) buys **~2%**. Both are extrapolations (estimates). | |

**So no:** water with true marched reflections doesn't fit 1.0 ms at 30% coverage. It's 14× over at full quality, and 3.6× over with quarter-resolution reflections that no longer match the reference. **Use the fallback** (screen-space reflections, a probe, refraction from the rendered scene), or a hybrid (below).

### Where the time goes

`shore`, `full`, 40% coverage (835K water pixels), water-only frames:

| Pass | ms | ns per water pixel | Notes |
|---|---|---|---|
| `water_surface` | 0.26 | — | every pixel; the lake plane and the stream march |
| `water_gbuf` | 1.31 | 1.6 | two Newton steps and the filtered normal: 12 waves and 2 noise octaves, three evaluations |
| `reflect` | **12.8** | **15.4** | 41 steps per ray (27 object, 14 terrain), 15 tree cells; hits: trees 50%, sky 25%, terrain 14%, tower 9% |
| `refract` | 0.98 | 1.2 | 11 steps per ray; every ray reaches the bed (the lake is ≤ 3 m deep) |
| `composite` | 0.46 | 0.55 | glint shadow rays on 1.2% of water pixels (sunset: 5.7%) |
| **water slice** | **15.9** | 19.1 | sunset: 14.2 |

- **Reflection is almost all tracing.**
  - Tracing nothing (sky only) leaves 0.52 ms of fixed cost; leaving hits unshaded saves only ~0.6 ms.
  - The heat map (`results/shore-heat-reflect.jpg`) shows the cost: every water pixel that reflects the forest wall is at the hot end. Water reflecting the tower or the sky is cheap.
  - A reflection ray into a forest costs what a primary ray into it costs: it grazes crown after crown, which is spike 02's silhouette worst case. The brute-force reference takes 110 steps per reflection ray.
- **Even with no rays at all,** the per-pixel water work (`water_surface` + `water_gbuf` + `composite`) is **2.0 ms at 40% coverage**: twice the slice. The bandlimited wave sum is evaluated three times per water pixel, and wave detail moves it directly (`water_gbuf` costs 0.66 ms with 6 waves, 1.31 with 12 waves + 2 octaves, 2.36 with 24 waves + 4 octaves).
- **Whole frames:**

  | View | Frame | Scene | Water passes | Dry lake bed | Marginal cost of water |
  |---|---|---|---|---|---|
  | `shore` | 32.2 | 14.2 | 18.0 | 16.7 | 15.5 |
  | `sunset` | 30.2 | 13.9 | 16.5 | 16.8 | 13.4 |

  - In ms, `full` configuration.
  - With `r2q2`, the marginal cost is **4.1 ms**: less than its water passes (7.1), because clipping primary rays at the water makes the scene pass ~3 ms cheaper than shading a dry bed.
- **Paced at 60 Hz:** median frame 38.3 ms (`full`) and 28.0 ms (`r2q2`); 150 of 150 frames over 16.7 ms. The scene pass alone is ~14 ms, which is the forest's cost (03-forest's subject).
- **At 960×540 internal:**

  | View | Config | Water slice (ms) | Of which reflect | Scene | Frame |
  |---|---|---|---|---|---|
  | `shore` | `full` | 5.3 | 4.3 | 4.7 | 10.4 |
  | `shore` | `r2q2` (480×270 reflections) | 2.1 | | | |
  | `sunset` | `r2q2` | 2.2 | | | |

  No upscaler is implemented, so no upscaled quality is claimed.

### The sweeps

All at 1080p. Water-slice ms, with water-pixel quality against the reference of the same content (mean/255 and share over 8/255).

| Configuration | shore ms | shore quality | sunset ms | sunset quality |
|---|---|---|---|---|
| `full`: reflection and refraction at 1, 200 m | 15.9 | 0.78, 2.1% | 14.2 | 0.49, 1.4% |
| reflection ½ | 6.9 | 7.6, 34% | 6.6 | 2.4, 10% |
| reflection ½, refraction ½ (`r2q2`) | 6.6 | 7.8, 35% | 6.2 | 2.4, 10% |
| reflection ¼, refraction ½ (`r4q2`) | 3.9 | 9.6, 42% | 3.9 | 3.7, 15% |
| proxy world, full resolution | 8.5 | 4.6, 21% | 7.5 | 1.9, 6.7% |
| proxy world, `r2q2` | 4.5 | 8.6, 38% | 4.2 | 2.9, 12% |
| reflection length 25 m | 4.6 | 28, 82% | 3.9 | 19, 82% |
| length 50 m | 9.0 | 13, 37% | 7.7 | 11, 35% |
| length 100 m | 14.7 | 1.5, 3.9% | 13.0 | 1.7, 4.5% |
| low waves (6, no noise), vs its own reference | 15.1 | 0.85, 2.2% | 14.4 | 0.47, 1.4% |
| high waves (24 + 4 octaves), vs its own reference | 18.2 | 0.83, 2.1% | 18.9 | 0.44, 1.3% |
| one Newton step (quality only) | — | 1.5, 4.9% | — | 0.63, 1.9% |
| hit tolerance 0.1 footprint (quality only) | — | 0.40, 1.0% | — | 0.27, 0.7% |

**Reading the sweeps:**
- **Reduced resolution** cuts reflection time 3.3× at ½ and 9.8× at ¼, not 4× and 16×. Each texel intersects and filters the water at its own footprint, and its rays are less coherent: 18.5 ns per ray at ½ and 25 ns at ¼, against 15.4 at full.
  - Visually, ½ is plausible but blocky at reflected edges (2×2 upsampling).
  - It also differs from full resolution by design. Its normals are bandlimited for the coarser rate, so its reflections are less distorted, which accounts for most of its 34% over 8/255. Compare `results/shore-full.jpg` and `shore-r2q2.jpg`.
- **The proxy world** (no leaf displacement, plain cones and cylinders) cuts object steps per ray from 27 to 9.4 and reflection time 2.4×. Its errors are visible: smoother, wrongly shaped reflected trees.
- **Reflections can't be cut short.** The lake reflects the forest 50–150 m away. At 25 m a ray falls back to a flat treeline colour and the image is wrong (`results/shore-len25.jpg`); even 100 m misses 4% of water pixels.
- **The tightest fast path, a 0.1-footprint tolerance,** meets the image bar in the sunset view and lands exactly on it in the shore view (1.0%). It wasn't timed, and it costs more than `full`.

**Coverage sweep** (`shore`, the camera tilted from +8° to −28°):

| Coverage | 22% | 34% | 45% | 56% | 69% | 80% | 78% |
|---|---|---|---|---|---|---|---|
| `full` ms | 9.6 | 15.0 | 21.0 | 21.6 | 23.9 | 27.1 | 22.7 |
| `r2q2` ms | 4.3 | 5.9 | 8.7 | 9.7 | 11.7 | 13.6 | 11.5 |
| `r4q2` ms | 2.8 | 3.9 | 5.2 | 6.0 | 7.3 | 8.3 | 7.4 |

- Cost isn't linear in coverage. The expensive water is the far, grazing water that reflects the forest wall, and it's in view at every tilt.
- Tilting down adds near water that reflects the sky, which is cheap: steps per reflection ray fall from 48 to 20 across the sweep.
- So the linear fits (in the JSON) have large intercepts. Their crossings at 1.0 and 2.0 ms come out negative and mean nothing; the "limit" row above uses cost per pixel instead.

### Quality

- **Fast against reference.** No configuration passes the pre-registered bar. The closest, `full`, passes on mean (0.78 and 0.49) and fails on share over 8/255 (2.1% and 1.4%) and on caps (0.4%).
  - The residual is speckle in the reflected forest (`results/shore-full-diff.jpg`). A wavy mirror maps each pixel to a distant point, so sub-pixel differences in hit tolerance and step placement move reflected silhouettes and flip the shading of displaced leaves.
  - A diagnostic split it roughly in half. In the smooth proxy world, fast against a proxy reference is 0.53 and 1.6%.
  - A tolerance of 0.1 footprints, instead of 0.25, nearly closes it (above).
- **Single-sample aliasing.** Against a 9-sample supersampled reference, the fast path differs by 6.2/255 with **30%** of water pixels over 8/255 in the `shore` view (sunset: 1.4 and 5.5%). The brute-force reference is just as far from it (6.2, 30%).
  - So this is aliasing of one sample per pixel, not an approximation of the fast path. Footprint-filtering the waves and widening the glint by the filtered variance don't stop reflected detail from aliasing.
  - In motion this would shimmer. It needs temporal accumulation, which isn't implemented.
- **Correctness bugs that the comparison caught,** fixed before the final run:
  1. The reflection cone was widened by the water's roughness, and the hit tolerance scales with the cone. Reflected trees were fattened (12.5/255, 41% over 8), and reflections looked 3× cheaper than they are.
  2. Shadow rays that ran out of steps counted as lit. At sunset that drew a sun-glint path on water the trees actually shade. Shadows are now cast by the trees' base shapes with a larger budget: 62 capped shadow rays per frame at sunset, 0 on the shore.

### How it looks

Judged from the screenshots, honestly.

- **`shore`** (`results/shore-full.jpg`): it reads as a calm forest lake on a clear afternoon.
  - The stone tower and spruces are mirrored in the water and broken up by ripples, with calm and ruffled patches.
  - Near the shore you see the bed through clear green-brown water, with soft caustics and sunken leaves.
  - On the far shore, a small cascade falls between boulders into a foam pool.
  - The water is the best thing in it. The world around it is clearly "simple trees": blobby birches, a clean CG look, no ambient occlusion or bounce light.
- **`sunset`** (`results/sunset-full.jpg`): backlit golden-hour silhouettes of spruces and the tower, the sun just above the treeline, a glint path on the water and the trees' reflections. It's the more beautiful of the two. The sky is a little washed out: the ACES curve desaturates its bright orange.
- **Weak spots:**
  - The waterfall is small and simple: a streaked white chute with no spray or mist.
  - The stream mouth's foam is faint.
  - There's no lapping at the shoreline.
  - The reflections shimmer (above).
- **Half-resolution reflections** look acceptable in a still image, but reflected edges are blocky. Quarter resolution is visibly smeary.

### Other findings

- **Scoped slope bounds work (D-092).** The cooked terrain is marched with L = 0.72 almost everywhere and 1.6 inside two boxes around the stream and escarpment. The probe found maxima of 0.66 and 1.44. A single global bound would be 1.6.
- **The probe caught two wrong assumptions:**
  - My first terrain had a slope of 9.6 where I'd declared 0.6. The stream valley carved an unbounded trench far upstream.
  - The noise slope is 3.69 per unit of frequency, not the 2.5 I assumed (spike 02 assumed 3.0).
- **Walking terrain and trees together** (instead of terrain first) cut steps per reflection ray from 18 to 12 in an exploratory run, but not reflection time (3.5 → 3.8 ms; that run still had the fattening cone bug). Time per ray tracks object steps near crowns, not step count in open space.
- **Refraction is the cheap half.** In a shallow lake it costs ~1.2 ns per water pixel at full resolution and 0.59 ms at half (40% coverage), because refracted rays go steeply down to a nearby bed.
- **Pipelines:** six per frame, plus the blit. A cold compile of one configuration takes 161 ms, with `reflect` the longest at 65 ms. Cooking the world (heightmap, noise tables, trees, probe) takes 32 ms.

### What carries over to a hybrid renderer

This applies if big things are rasterized and fields are used for lighting, reflections, distance and cooking (the coordinator's summary of spikes 03–07).

- **Reflection rays traced against fields from a rasterized water surface:** the `reflect` numbers carry over almost unchanged. They depend on the world the rays march, not on how the water was found.
  - The lesson is that a reflection ray costs a primary ray against the same representation.
  - If the forest is rasterized for primary visibility because marching it is too expensive, reflection rays must march something much cheaper: a distance- and footprint-dependent LOD or cooked proxy.
  - The proxy here was still 1.9 ms at `r2q2` resolution, so the hit count must shrink too. The obvious candidate is screen-space reflections first, with field-traced rays only for SSR misses (off-screen and occluded content). That's untested.
- **What changes:**
  - Rasterizing the surface (or a plane with a normal map) removes `water_surface` and the Newton steps.
  - The per-pixel wave sum would move into the fragment shader at the same cost (~1.3 ms at 40%) unless the wave field is cooked into a mip-mapped texture. An FFT-style animated tile, filtered by hardware mip-mapping instead of per-wave fades, is a hypothesis to test.
  - Refraction from the rasterized scene (the bed drawn once and sampled) replaces the marched refraction. The ~3 ms the clipped scene pass saved here suggests the bed shouldn't be shaded twice in any architecture.
- **Findings that carry over as they stand:**
  - the reflection cone uses the pixel footprint, not roughness, for hit tolerance;
  - a shadow ray that runs out of steps must not count as lit;
  - reflection length must reach the far shore;
  - reduced-resolution reflections should be filtered at their own rate;
  - the glint's Toksvig widening;
  - single-sample reflections shimmer, so temporal accumulation is needed.

### Caveats

- **Timings are indicative** (above). The quiet-machine run is still to come.
- **One device and browser:** an M4 with headless Chrome 154.
- **The world's representation is my assumption:**
  - a cooked heightmap, cooked noise, and trees confined to grid cells;
  - shadows cast by base shapes;
  - reflections shaded without shadow rays, with the ground under trees darkened by a canopy term instead.
  - Reflection cost scales with whatever 03-forest and 04-vista settle on. Real foliage would probably cost more per reflection ray (a hypothesis).
- **The reference shares those content definitions,** so it checks the marching and filtering approximations, not the representation.
- **Coverage:** the 30% figure is interpolated from a single view's sweep. The `sunset` view (39%) agrees with it.
- **No upscaling or temporal accumulation is implemented.** Reduced-resolution and aliasing results would change with them (untested).
- **Changes made after exploratory measurements, before the final run** (the kill criteria didn't change):
  1. The terrain was cooked into a heightmap and the noise into textures (analytic evaluation per step was the dominant cost).
  2. A G-buffer pass was added: the wave field was being evaluated 5–6 times per water pixel.
  3. Terrain and trees became one walk; block skipping was added.
  4. Sweeps time water-only frames, in two rounds.
  5. The correctness fixes above.
  6. The supersampled reference uses 9 samples, not 16, for run time and the GPU-safety rules.
  7. The `sunset` sun was raised from 6° to 13°, so its glint isn't hidden by the trees.

### What this means for the design

Nothing here is recorded in `decisions.md`; that's the owner's call.

- **The water slice can't hold marched reflections of a forest.** The fallback named above stands.
- **Marched refraction in shallow water is affordable** relative to reflection, but the per-pixel wave work (~2 ms at 40% coverage) already exceeds the slice. Water needs a cooked wave field whatever traces its secondary rays (hypothesis).
- **For the language (D-050-clean):**
  - Scoped Lipschitz facts (D-092) earned their keep.
  - Footprint-faded noise (D-077) worked as specified, but it doesn't remove the aliasing of what a wavy mirror reflects.
  - Every declared fact I guessed (terrain slope, noise slope) was wrong until a probe measured it. That argues for debug-build sampling of `@assume` (D-077) being on by default.

## Quiet-machine rerun (2026-10-02)

Run: `results/run-2026-10-02T04-00-03-916Z.json` (serial, after a cool-down, headless Chrome 154; 131 s). Its two timing rounds agree to within one 65.5 µs timestamp step (they were up to ~31% apart before). Stats and image comparisons are identical to the earlier run's.

| Metric (1080p unless noted) | README value | Quiet value |
|---|---|---|
| Water slice at 30%, `full` (ms) | 14.0 | 12.8 |
| Water slice at 30%, reflection ½ (`r2`) / `r2q2` (ms) | 6.0 / 5.7 | 5.7 / 5.3 |
| Water slice at 30%, reflection ¼ (`r4q2`) (ms) | 3.6 | 3.3 |
| `reflect`, `shore` `full`, 40%: ms; ns per ray at 1 / ½ / ¼ | 12.8; 15.4 / 18.5 / 25 | 12.6; 15.1 / 18.5 / 25.0 |
| Whole frame, `full`, `shore` / `sunset` (ms) | 32.2 / 30.2 | 27.9 / 25.6 |
| Whole frame, `full`, `shore`, 960×540 (ms) | 10.4 | 8.0 |
| Correctness, `full`, `shore`: mean/255, share over 8/255 | 0.78, 2.1% | 0.78, 2.1% |
| Correctness, `full`, `sunset`: mean/255, share over 8/255 | 0.49, 1.4% | 0.49, 1.4% |
| Reflection-ray caps, `full`, `shore` / `sunset` | 0.41% / 0.35% | 0.41% / 0.35% |
| Criterion 3: coverage 1.0 ms buys, `r4q2` / `full` (estimate) | ~8% / ~2% | ~8% / ~2% |

**Verdict:** none changes. Every configuration is still over 2.0 ms at 30% (Fail), none meets the correctness bar, and the limit estimates stand. Only the margins move: `full` is 12.8× the slice, not 14×, and `r4q2` is 3.3×, not 3.6×.
