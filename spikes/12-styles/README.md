# Spike 12: styles

*A measurement spike for the flagship's art direction, which is still open (D-103). It's throwaway code, not the start of the engine.*

## The question

**What do realistic, anime/cel and painterly looks cost in a pure ray-marched field renderer, and does a stylized look make the hard cases (foliage, close-ups) cheaper?**

A field renderer gives stylization inputs that a triangle renderer has to approximate in screen space:
- the exact distance from each ray to every silhouette it passes (its closest approach)
- curvature, from a few extra field samples
- thickness and occlusion, from field samples along the normal
- a stable surface in object space to anchor brush strokes on

The owner hasn't chosen an art direction. This spike prices the options; it doesn't pick one.

## The scene

One scene, four looks. A small forest clearing under a sun at 38° elevation, from the wide shot's left:
- **terrain:** a heightfield, flat in the clearing and rising into hills
- **287 simple trees** on a jittered 8 m grid: a trunk, two branches and five canopy clumps each. Spike 03 does real foliage; these are stand-ins.
- **a stone tower** with masonry courses, a door, arrow slits and a crenellated top, plus a few boulders
- **the wolf** from `experiments/agent-authoring/wolf/creature.wgsl`, copied into `wolf.wgsl` (renamed functions, and a switch for its noise; see Layout)

**Cameras:**

| Camera | What it tests |
|---|---|
| `wide` | The establishing shot from the clearing's edge at eye height: the wolf at ~8 m, the tower, the forest wall, sky |
| `close` | The wolf at 1.8 m (12% of the pixels) with the forest close behind: the close-up hard case (spike 02 lost 2.2× there) |
| `canopy` | The forest edge, looking toward the sun, with backlit foliage over a quarter of the frame and its shadows below: the foliage hard case |

**Content levels.** A stylized art direction lets content drop detail that only realism needs. The spike measures that saving separately from the shading:
- **detailed:** canopy clumps displaced by two octaves of noise (leaf clusters); the wolf's fur and ruff noise; mortar grooves cut into the tower.
- **simple:** canopy clumps with one low-frequency octave (soft clumps); no noise on the wolf; masonry painted in shading only.

## The looks

| Variant | Content | Shading | Lines or strokes | Role |
|---|---|---|---|---|
| `real` | detailed | physically based: soft shadows, field AO, GGX, leaf translucency, fog | none | **the baseline** |
| `cel` | detailed | quantized light, stylized terminator, cool shadow tint, rim light, hard shadows | **field-native outlines** + curvature crease lines | anime |
| `cel_ss` | detailed | same as `cel` | screen-space depth/normal edges | anime, the usual way |
| `paint` | detailed | painted value bands, warm light and cool shadow | **object-space brush strokes**, overshooting silhouettes | painterly |
| `paint_kuw` | detailed | `real`, then an anisotropic Kuwahara filter | screen-space | painterly, the usual way |
| `real_simple` | simple | as `real` | none | content savings alone |
| `cel_simple` | simple | as `cel` | as `cel` | anime with stylized content |
| `paint_simple` | simple | as `paint`, canopy as soft clumps with painted, thickness-based shading | as `paint` | **stylized foliage** |

## Kill criteria, written before measuring

**A style's extra cost** is its whole-frame GPU time minus `real`'s, on the same camera, at native 1920×1080 on the M4, from timestamp queries under sustained load (median of 90 back-to-back frames after 30 warm-up frames).

1. **Budget.** Per style, the worst of the three cameras:

   | Extra cost over `real` | Verdict |
   |---|---|
   | ≤ 1.0 ms (the post slice) | **Pass** |
   | 1.0–2.0 ms | **Inconclusive** |
   | > 2.0 ms | **Fail**: use the fallback |

   The task's rule is "fits in the post slice, plus any shading changes inside the systems' own slices". This scene doesn't separate the systems (vegetation, world geometry, creatures and lighting share one trace), so the verdict charges the **whole** extra cost to the post slice. That's the conservative reading. The extra is also reported split by pass, so the owner can apply the looser one.

   The post slice is already allocated to temporal accumulation, upscaling, tone mapping and UI (`spikes/README.md`). A style that uses it all leaves nothing for those; the README for the budget will have to say where that comes from.

2. **Correct, not just fast** (spike 02's thresholds). Each fast variant is compared with a brute-force reference render of the same content and style: half-size steps with the strict Lipschitz bound, a twentieth of the hit tolerance, step caps of thousands, and half-size shadow steps.
   - mean |difference| **≤ 0.5/255**
   - **≤ 0.5%** of pixels off by more than 8/255
   - rays that end on the step cap **≤ 0.1%** of covered pixels

   A fast number with a wrong image doesn't count.

3. **Temporal coherence** (this spike's threshold, set before measuring). Under camera motion at 2 px/frame, a stylization's flicker must not exceed `real`'s by more than **0.5/255** mean warp error, or **0.5 percentage points** of pixels over 8/255. Flicker is measured as below.

4. **Stylized foliage** has no kill criterion: it's a saving. It counts as **meaningful** if `paint_simple` or `real_simple` cuts the `canopy` view's frame time by **≥ 25%** against `real`.

Also reported: each style's whole-frame cost, the split by pass, all of it at 960×540 too, and paced 60 Hz runs.

## The fallbacks

- **Field-native outlines over budget:** screen-space depth/normal edge detection (`cel_ss`), or lines from a half-resolution pass.
- **Object-space strokes over budget or incoherent:** a screen-space filter (`paint_kuw`), or strokes baked into the material's albedo at cook time (loses the stroke's view-dependent overshoot).
- **No stylized look fits:** stylize at 960×540 behind a temporal upscaler, or go realistic. Neither is implemented here.

## Method

Everything is compute shaders on one WebGPU device, at 1920×1080 and 960×540. Each pass has its own timestamp pair. Per frame:

1. **`trace`:** sphere-traces the scene per pixel and writes a G-buffer: hit distance, normal (oct-encoded), material, step count, and the **closest-approach record** used for lines and stroke overshoot.
   - **Terrain:** a heightfield traced with a directional Lipschitz bound (spike 02's trick). The slope bound comes from the zone the ray is in: 0.11 inside 24 m, 0.18 inside 100 m, 0.37 beyond, each measured from the terrain function plus 10%. One marcher steps by the smaller of the terrain's safe step and the objects' distance. Above the tallest object (14 m over the terrain) nothing is evaluated.
   - **Trees:** a jittered 8 m grid, turned 30° with odd rows offset by half a cell so the trees don't line up, one tree per cell, read from a storage buffer. A step evaluates only the current cell's tree. It's clamped to the cell's exit, or to the cell's precomputed clearance from every other tree, whichever is longer (both are safe). Each tree has bounds: a sphere around the canopy, a capsule around the trunk and branches.
   - **The canopy's displacement** is traced in two phases (spike 02): the base clumps minus the displacement's amplitude, then the displaced field within that shell, with the step shrunk by a Lipschitz estimate. The fast path trusts 0.35 of the strict bound, which is a hypothesis; the reference uses the strict one. Each octave fades with the pixel footprint (D-077's filtering).
   - **The wolf** is the authored field with part-group bounds added: torso, neck, head, forelegs, hindlegs and tail each sit in a box, and a group is skipped when its box is at least the running union plus its blend radius. That leaves the smooth union unchanged (`smin(a, b, k) = a` when `b ≥ a + k`). The fur gets a two-phase shell. The reference evaluates `wolf_field` as authored, so a wrong box would show there.
   - **The hit tolerance is a tenth of a pixel's footprint.** A quarter failed the reference check, as in spike 02.
2. **`light`:**
   - **The sun:** a soft shadow march. The penumbra uses the closest-approach estimate between samples, and steps are denser where the penumbra term is active. Bounds may stand in for distances only beyond `t / SOFT`, where they can't change the penumbra. Occluder detail is filtered to the penumbra's width, since finer detail can't show in a shadow and the displaced canopy's "distance" is far from Euclidean.
   - **AO and thickness:** four-tap field AO along the normal, at AO scale (no fur, coarse canopy), and a one-tap thickness for leaf translucency.
   - **Leaves facing away from the sun** get two field samples toward the sun instead of a shadow march: they receive no direct light, only what comes through their clump.
   - **Cel:** shadows are thresholded, so the march stops as soon as the penumbra estimate is below the threshold. Cel also gets curvature: a tetrahedral Laplacian at the line's scale, five evaluations.
3. **`shade`:** the style. Writes display-linear colour.
4. **Post:** only what the style needs, either screen-space edges or the Kuwahara chain (structure tensor, two blur passes, filter).

**GPU safety** (spikes/README.md). Every pass is its own submission. The trace, light and Kuwahara passes run as top and bottom halves, each its own submission, and a pass's time is the sum of its halves. The brute-force references render in 240×128 tiles, one tile of one pass per submission, waiting in between. Pipelines compile two at a time.

### Field-native outlines

While a ray marches, it tracks its distance to the scene in pixels, `d / (t × pixel angle)`. Between samples, a two-sphere closest-approach estimate (Quílez's, generalized to any step) refines it, used only for steps no longer than the previous distance. A longer step (the terrain's directional one) can make the estimate invent a narrow waist between nearly tangent spheres.

An **approach** is committed once the ray has clearly left the surface: its sampled distance rises past `max(2W + 2, 2 × min + 2)` px before it hits anything. A ray closing on its own hit never commits. Within `W + 1` px of a surface, every bound is replaced by exact evaluation, so a bounding sphere or the displaced shell's inner bound can never draw a line. The result:
- lines sit **outside** silhouettes, over whatever is behind, with a width of `W` px and analytic anti-aliasing (the distance is continuous)
- lines appear where one part passes in front of another (a leg over the body) as long as the gap opens past the leave distance, and they fade where parts touch
- there's no screen-space search and no depth buffer: a few registers and flops per step, plus the exact-evaluation margin

**Inner lines** come from curvature: the Laplacian above marks concave creases (where smooth unions join, and ground contact). Convex ridges on stone get a light edge.

**The screen-space baseline** has two versions:
- `sobel`: a Laplacian of inverse depth (zero on planes, so sloped ground stays clean) plus normal differences, sampled `W/2` px apart. Constant cost; the usual game technique.
- `disk`: the exact-width analogue of the field-native definition: a pixel is a line if a much nearer pixel lies within `W` px. Cost grows with `W²`.

Line quality is compared on line-only renders (white surfaces, black lines) against the field-native lines of the brute-force reference, which are exact. That reference shares the field-native method's definition of a line, which favours it. The comparison reports the false and missed line pixels so the definitional part is visible.

### Object-space brush strokes

Strokes are a **solid texture anchored in object space**: world space for the static scene, the wolf's frame for the wolf.
- **Cells and strokes:** space is cut into cells, and each cell holds a stroke: an ellipsoid at a jittered centre, 1.05 cells long along a direction in the surface's tangent plane, 0.55 across, 0.45 through. A stroke exists only if its centre lies within 0.4 cells of the surface (first order, from the point's distance and the normal), so strokes hug surfaces. Each stroke carries a value offset and a hue offset.
- **Compositing:** covering strokes are combined by a soft maximum over their priorities, with feathered edges. It reads as a mosaic of overlapping dabs, but it stays continuous, so a sub-pixel change in the surface point can't flip a pixel to another stroke. A hard "highest priority wins" failed the reference check and would alias in motion.
- **Stroke direction** comes from the field's gradient: per material, a fixed axis projected into the tangent plane (horizontal courses on stone, along the trunk, along the wolf's body), or a random angle per stroke (foliage). On the wolf, the frame uses the fur-free body's gradient (four extra body evaluations), because the fur's noisy normal smears the strokes.
- **Stroke size is held in screen space:** the cell size follows the pixel footprint (12 px at 1080p). Two octaves are blended across the middle of each octave with Bénard et al.'s contrast-preserving blend (*Dynamic Solid Textures for Real-Time Coherent Stylization*, 2009), so strokes don't pop as distance changes.
- **Strokes overshoot silhouettes.** For a pixel within 1.5 strokes of a silhouette, the closest-approach record gives the point near the surface. If a stroke there reaches the ray, the pixel takes that stroke, so edges break up the way a brush's do. It costs a gradient (four evaluations) and a stroke lookup on those pixels only.
- **Value bands:** the strokes perturb bands that come from the same light as `real` (shadow, AO, thickness), mapped to a per-material palette with warm light and cool shadow.

**The screen-space baseline** is the anisotropic Kuwahara filter with polynomial sector weights (Kyprianidis et al. 2009; Kyprianidis, Semmo, Kang and Döllner 2010), radius 6 px at 1080p, on the `real` image. The filter pass stages each 16×16 tile and its 12 px apron in workgroup memory, and visits only each row's span of the ellipse.

### Stylized foliage

`paint_simple` draws the canopy as soft clumps: one low-frequency octave of displacement, painted value bands, and the stroke overshoot at silhouettes, which gives the clumps a soft, brushed edge. That's a surface with soft edges, not a volume march; a real volume would cost more, not less.

### Temporal coherence (flicker)

The camera strafes sideways at a fixed speed, given in pixels per frame of motion at the wolf's distance. For each pair of consecutive frames, frame B is warped into frame A using B's hit distances and A's camera, and compared:
- a pixel counts only if A sees the same surface there (A's hit distance within 2%), so disocclusions are skipped
- sky pixels are skipped
- A is sampled bilinearly, so the measure includes resampling blur; that floor is the same for every style and shows in `real`

The result is the mean warp error (/255) and the share of pixels over 8/255, over six frame pairs. View-dependent shading (speculars, rims, lines that slide along the surface behind a silhouette) counts as flicker here even when it's correct; `real` sets the floor for that.

### What is measured

Frames in this scene cost 38–150 ms at 1080p (the baseline isn't in budget; see Results), so the matrix is trimmed. A run still takes 4–5 minutes on this machine, over the ~3 minutes asked for:

| Camera | 1920×1080 | 960×540 |
|---|---|---|
| `wide` | real, cel, cel_ss, paint, paint_kuw | real, cel, paint |
| `close` | the same, plus real_simple, cel_simple | real, cel, paint |
| `canopy` | all eight | real, cel, paint, paint_simple |

Every configuration gets 30 warm-up frames, then 90 back to back.

**Sweeps:**
- **Outline width** `W` ∈ {1, 2, 4, 6} px on `wide`: line error against exact lines for the three methods. Timed for the passes that change: the edges pass (`sobel`, `disk`) at every width, and the trace with and without line tracking (at W = 6; W = 2 is in the main matrix).
- **Stroke size** ∈ {32, 16, 12, 8, 4} px on `wide` (density goes as its inverse square): the shading pass's time and flicker at 2 px/frame. Also the overshoot's own cost on every camera.
- **Camera speed** ∈ {0.5, 2, 8} px/frame: flicker for real, cel, cel_ss, paint, paint_kuw and paint_simple, on `wide` and `canopy`.
- **Paced:** real, cel and paint on `wide` at 60 Hz by busy-wait, at both resolutions (90 frames, the first 30 dropped).

At 960×540, line widths, stroke sizes and the Kuwahara radius are halved, so the look is the same. Nothing upscales; 960×540 is the cost a temporal upscaler would start from, not its quality.

### Changes made after exploratory measurements, before the final run

The criteria didn't change.
1. **The hit tolerance** went from ¼ to ⅒ px: ¼ failed the reference check on silhouettes (0.5–0.9% of pixels over 8/255).
2. **Soft shadows:**
   - The estimator first produced banded false shadows: Quílez's formula assumes each step equals the previous distance, and these steps are clamped. It now uses the general two-sphere form.
   - Bounds inside the penumbra zone, and the canopy's unfiltered detail, made the penumbra differ from the reference (up to 1.9% of pixels over 8/255). Hence exact evaluation within `t / SOFT`, penumbra-scale filtering and denser steps there.
3. **Lines:**
   - The closest-approach estimate drew false line speckles on the ground, so it's now limited to sphere-tracing-like steps, and the leave decision uses sampled distances only.
   - A three-sample parabolic refinement of each line pixel was tried and removed: on the displaced canopy it pulled approaches under the line width (8% of pixels over 8/255).
4. **Strokes:** the hard mosaic became a soft maximum (see above). Stroke thickness went from 0.75 to 0.45 cells, and the overshoot reach from 2 to 1.5 strokes. The wolf's strokes use the fur-free normal.
5. **Speed, the same for every look:** the wolf's part-group bounds, the terrain's slope zones, the per-cell clearance, AO at AO scale, two-sample translucency for back-facing leaves, and the Lipschitz trust from 0.5 to 0.35 (the reference check still passes). Without them, a close-up frame was ~120 ms in one submission.
6. **The Kuwahara filter** was rewritten twice (vector accumulators, then workgroup memory and row spans): 42 → ~34 ms at 1080p.
7. **The `disk` baseline** first judged "much nearer" by a flat depth gap. It over-drew ground receding at grazing angles (IoU 0.29 at W = 2), so it now predicts each neighbour's depth from the centre pixel's plane, as a careful screen-space implementation would (IoU 0.69). The `sobel` baseline's line-only view also stopped including normal creases, which the silhouette reference doesn't contain.
8. **Step caps** went from 256 to 400 (primary) and 96 to 160 (shadow): 0.12% of the close-up's covered pixels hit the primary cap in a trial run.
9. **A discarded warm-up configuration** before the matrix: the first configuration after compiling read high.

## Layout

| File | What |
|---|---|
| `common.wgsl` | Frame uniform, noise, SDF helpers (from `experiments/agent-authoring/fieldview`), G-buffer packing |
| `wolf.wgsl` | The wolf, copied from `experiments/agent-authoring/wolf/creature.wgsl`. Changes: `field`/`albedo` renamed `wolf_field`/`wolf_albedo`, and its shape noise goes through `wfbm`/`wnoise`, which return 0 for simple content. |
| `scene.wgsl` | Terrain, trees, tower, rocks, the wolf's placement; the scene field and its bounds |
| `trace.wgsl` | Primary visibility, the closest-approach record, the G-buffer |
| `light.wgsl` | Shadows, AO, thickness, curvature |
| `shade.wgsl` | The three shading styles, sky, outlines, strokes |
| `post.wgsl` | Screen-space edges, the Kuwahara chain, the flicker measure, the blit |
| `main.js` | Harness: the scene's placement and trees, pipelines, timing, sweeps, the reference comparison, flicker, screenshots |
| `results/` | The final run's JSON, `console.log`, and JPEG copies of the key screenshots (PNGs are ignored by git): `<camera>-<look>`, `*-diff` (red over 8/255, yellow over 2/255 against the reference), `*-heat` (primary steps), `lines-*` (line-only crops), `wide-paint-s*` (stroke sizes), `wide-cel-W*` (line widths), `*-540` |

## Running it

```bash
python3 spikes/serve.py 8417
```

```bash
spikes/headless.sh 12-styles '#run' 1200
```

A full run takes 4–5 minutes on this machine and holds the GPU lock throughout. `#quick` renders real, cel and paint on every camera once, times five frames of each and saves screenshots (~10 s). Options go after an `=`, for example `#quick=looks:cel,paint;cams:close;ref:1` (`ref:1` adds the reference comparison and a diff image, `dbg:1` the line-only view). Open <http://localhost:8417/12-styles/> for a button.

## Results (2026-10-01, indicative)

**The timings are contaminated, so treat every number in ms as indicative.**
- **Setup:** the MacBook Air M4 (8-core GPU, 16 GB), Chrome 154 headless via `spikes/headless.sh`.
- **The GPU lock held,** so no other spike drew at the same time. But the fanless machine had been under heavy GPU load from many spikes all evening, and it throttled:
  - **between runs:** the same `close/real` frame measured 120 ms in one full run and 90 ms in the next
  - **within a run:** identical trace pipelines measured as the 1st, 3rd and 5th configuration of a camera drifted by up to 17% (53 → 56 → 62 ms)
- **Which runs:**
  - Final: `results/run-2026-10-02T03-06-35-900Z.json`, 252 s.
  - Shown in brackets: an earlier full run with the same code except the `disk` baseline's edge test, 288 s. It isn't kept in the repo.
- **Deterministic results:** correctness, line quality and flicker came out identical in both runs.
- **Quiet-machine run:** every timing here needs one.

### Verdict

| Criterion | Measured (1080p) | Verdict |
|---|---|---|
| 1. Budget, cel (field-native lines + creases) | extra over `real`: wide +0.3, close **+5.3**, canopy +2.7 ms [+5.8, +8.8, +1.8] | **Fail** (worst camera > 2 ms), indicative |
| 1. Budget, cel with screen-space edges | −2.6, −1.6, −0.7 ms [−2.4, −7.5, −1.3]: *cheaper* than realistic | **Pass** |
| 1. Budget, painterly (object-space strokes) | +4.0, **+28.4**, +4.4 ms [+6.9, +9.7, +4.1] | **Fail** |
| 1. Budget, painterly via Kuwahara | +33.9, **+55.8**, +41.7 ms [+38.5, +32.8, +40.1] | **Fail** |
| 2. Correct (≤ 0.5/255 mean, ≤ 0.5% over 8/255, caps ≤ 0.1%) | `real` 0.10–0.25 mean, 0.10–0.18%, caps ≤ 0.004%. Kuwahara 0.22–0.32%. **Cel 0.57–1.07%, cel_ss 0.42–0.58%, paint 0.44–0.66%**, means all ≤ 0.45 | **Pass** for realistic and Kuwahara. **Fail** on the share over 8/255 for the quantized looks (below). |
| 3. Flicker at 2 px/frame (excess over `real` ≤ 0.5/255 and ≤ 0.5 pp) | Kuwahara +0.06/255, −0.1 pp. Cel +0.99, +3.9 pp. Cel with screen-space edges +0.92, +3.1 pp. Paint +0.43, +1.3 pp. Stylized foliage +0.30, +0.9 pp (wide; canopy similar) | **Pass** only for Kuwahara |
| 4. Stylized foliage saves ≥ 25% of the `canopy` frame | `paint_simple` −10.0 ms of 48.6 (−21%) [−9.0 of 47.3, −19%]; `real_simple` −16% [−21%] | **Not meaningful** by the threshold set beforehand: a real but smaller saving |

At 960×540 the extras are smaller: cel −0.5 to +1.8 ms, paint +0.7 to +3.1 ms. Those would pass or be inconclusive, but the verdicts are set at 1080p.

**The baseline itself is far over budget:** `real` is 38 / 90 / 49 ms at 1080p (wide / close / canopy), 2–5× the whole 16.7 ms frame, and 9.8 / 30.9 / 12.6 ms at 960×540. This scene's trees, tower and wolf aren't optimized the way spikes 03–07 optimize theirs. The extras above are absolute ms. The extras that come from extra field evaluations (creases, line exactness, overshoot) would shrink with a cheaper field; that's a hypothesis.

### Where the extra time goes

Pass-level differences at 1080p, final run [earlier run]:

| Component | wide | close | canopy |
|---|---|---|---|
| Field-native outline tracking (trace: `cel` − `cel_ss`) | +1.6 [+4.7] | +0.7 [+6.5] | +2.4 [+1.9] |
| Curvature crease lines (light: `cel` − `cel_ss`) | +2.4 [+4.3] | +7.1 [+10.7] | +2.3 [+2.6] |
| Thresholded cel shadows (light: `cel_ss` − `real`) | **−3.7** [−4.2] | **−4.3** [−7.7] | **−3.8** [−4.1] |
| Screen-space Sobel edges pass | 1.1 [1.2] | 1.2 [1.4] | 1.2 [1.2] |
| Brush strokes + overshoot (shade: `paint` − `real`) | +3.8 [+4.8] | +9.4 [+9.9] | +3.1 [+3.1] |
| Overshoot's exact distances (trace: `paint` − `real`) | +2.2 [+4.1] | +14.8 [+3.5] | +3.9 [+3.5] |
| Kuwahara chain (structure, 2 blurs, filter) | 33.9 [40.0] | 41.9 [43.9] | 41.6 [40.0] |

- **Cel's thresholded shadows pay for its outlines.** A shadow march that stops once the penumbra estimate falls below the threshold saves 4–8 ms, more than the field-native outlines cost (1–6 ms). What tips cel over budget is the **curvature crease lines**: five full-scene field evaluations per pixel, 2–11 ms, most in the close-up, where the wolf's field is expensive. Cel with field-native outlines and *no* curvature creases would land near or below the realistic cost; that's a hypothesis, not measured as its own configuration.
- **Brush strokes cost about the same at every density** (below). The cost is per pixel (cell lookups), not per stroke. The overshoot adds 1.6–4.5 ms of shading, plus trace time: it needs exact distances out to 18 px from every surface, so bounds and displacement shells are skipped less. That trace term is noisy here (+2 to +15 ms).
- **Kuwahara is the most expensive thing measured,** 34–44 ms at 1080p. At 960×540 it would be ~9–11 ms if it scales with pixel count; that's an estimate, not measured. That's after staging tiles in workgroup memory and visiting only each ellipse's row spans. A better-optimized implementation might be 2–4× faster (a hypothesis), and it would still fail.

### Frame times

GPU ms: median of 90 back-to-back frames after 30 warm-up, final run. Brackets are the earlier run's extra over `real`.

| Look | wide | close | canopy |
|---|---|---|---|
| real | 38.0 [46.5] | 90.0 [119.9] | 48.6 [47.3] |
| cel | 38.3, +0.3 [+5.8] | 95.3, +5.3 [+8.8] | 51.3, +2.7 [+1.8] |
| cel_ss | 35.4, −2.6 [−2.4] | 88.4, −1.6 [−7.5] | 47.9, −0.7 [−1.3] |
| paint | 42.0, +4.0 [+6.9] | 118.4, +28.4 [+9.7] | 53.0, +4.4 [+4.1] |
| paint_kuw | 71.9, +33.9 [+38.5] | 145.8, +55.8 [+32.8] | 90.3, +41.7 [+40.1] |
| real_simple | – | 67.4, −22.5 [−53.5] | 41.0, −7.7 [−9.8] |
| cel_simple | – | 74.1, −15.9 [−48.9] | 39.8, −8.8 [−9.4] |
| paint_simple | – | – | 38.7, −10.0 [−9.0] |
| **960×540** real | 9.8 | 30.9 | 12.6 |
| **960×540** cel | 9.4, −0.5 | 32.6, +1.7 | 14.4, +1.8 |
| **960×540** paint | 10.5, +0.7 | 34.0, +3.1 | 13.9, +1.2 |
| **960×540** paint_simple | – | – | 10.6, −2.1 |

- **p95** is within 1–5% of the median except where the GPU throttled mid-measurement (close/paint: median 118, p95 141). Per-pass p95 is in the JSON.
- **The largest single submission** (half of the close-up's trace) was 50 ms, under the ~100 ms rule.
- **Paced at 60 Hz** (busy-wait, `wide`): every 1080p frame missed 16.7 ms (medians 50–54 ms). At 960×540 none did, with medians real 12.6, cel 14.8, paint 14.2 ms. Pacing let the clocks drop: cel's 9.4 ms unpaced frame took 14.8 paced. Spike 01 saw the same.

**Simple content saves the most in the close-up:** without the wolf's fur and ruff noise, the close-up frame is 25–45% cheaper. The rays take the same number of steps (20 per wolf pixel); each evaluation is cheaper. In the canopy view, simple clumps save 16–21% of the frame, and steps per leaf pixel drop from 41 to 30.

### Lines: quality against cost

Line-only renders on `wide`, compared with the exact lines of the brute-force reference: IoU of the line masks and mean coverage error over the union of line pixels.

| W (px) | Field-native IoU / error | Sobel IoU / error, cost | Disk IoU / error, cost |
|---|---|---|---|
| 1 | 0.83 / 0.09 | 0.36 / 0.50, 1.25 ms | 0.59 / 0.31, 1.44 ms |
| 2 | **0.91 / 0.07** | 0.39 / 0.49, 1.18 ms | 0.69 / 0.27, 2.49 ms |
| 4 | 0.94 / 0.05 | 0.37 / 0.55, 1.18 ms | 0.62 / 0.35, 8.39 ms |
| 6 | 0.96 / 0.04 | 0.37 / 0.58, 1.25 ms | 0.56 / 0.42, 16.4 ms |

- **Field-native lines track the exact ones closely at every width,** and their cost doesn't grow with width. In the same sweep the trace measured 28.2 ms without line tracking and 28.5 ms with it at W = 6. In-frame comparisons put line tracking at 0.7–6.5 ms (table above).
- **The reference shares the field-native method's definition of a line,** which favours it. The screen-space methods' errors include definitional differences, for example drawing a line where two surfaces nearly touch.
- **Sobel** straddles edges and aliases. **Disk** is the closest screen-space analogue, but it's aliased (whole-pixel distances), and its cost grows with W²: 16 ms at W = 6.
- `results/lines-*.jpg` crops the same region for each method.

### Brush strokes: density, overshoot and flicker

Stroke cell size, on `wide`. Cost is the shading pass alone. Flicker is at 2 px/frame.

| Stroke size (px) | 4 | 8 | 12 | 16 | 32 |
|---|---|---|---|---|---|
| Shading pass, ms | 5.4 | 5.8 | 6.0 | 6.6 | 7.9 |
| Flicker, /255 and share over 8/255 | 1.18, 3.3% | 0.84, 2.4% | 0.74, 2.2% | 0.71, 2.2% | 0.68, 2.2% |

- **Cost doesn't fall with density.** The cost is per pixel, and the spread here is measurement drift: the earlier run measured the same sweep flat at 5.8–6.6 ms.
- **Small strokes flicker more** (they alias). Above 12 px the flicker floor is ~0.7/255. That floor comes from the overshoot (view-dependent by design) and the quantized value bands, not from strokes sliding.
- **The overshoot's shading cost:** with it versus without, 6.0 vs 4.3 ms (wide), 11.9 vs 7.4 (close), 4.9 vs 3.3 (canopy).
- `results/wide-paint-s*.jpg` shows each size.

### Flicker

Mean warp error /255 and the share of pixels over 8/255, warping frame B into frame A. Disoccluded and sky pixels are skipped (~70% of pixels count).

| Look | wide 0.5 px | wide 2 px | wide 8 px | canopy 0.5 px | canopy 2 px | canopy 8 px |
|---|---|---|---|---|---|---|
| real | 0.22, 0.6% | 0.31, 0.9% | 0.27, 0.8% | 0.13, 0.3% | 0.26, 0.8% | 0.22, 0.7% |
| cel | 0.85, 3.1% | 1.30, 4.8% | 1.37, 4.4% | 0.51, 1.4% | 1.10, 4.2% | 1.14, 3.8% |
| cel_ss | 0.81, 2.6% | 1.23, 4.0% | 1.26, 3.4% | 0.49, 1.3% | 1.05, 4.3% | 1.10, 3.3% |
| paint | 0.50, 1.1% | 0.74, 2.2% | 0.89, 2.1% | 0.42, 0.8% | 0.80, 2.2% | 0.87, 2.2% |
| paint_kuw | 0.27, 0.6% | 0.36, 0.8% | 0.36, 0.8% | 0.16, 0.3% | 0.27, 0.6% | 0.30, 0.7% |
| paint_simple | 0.39, 0.8% | 0.61, 1.8% | 0.80, 1.8% | 0.37, 0.7% | 0.68, 1.8% | 0.77, 1.9% |

- **Outlines dominate cel's flicker, and field-native and screen-space lines flicker equally** (1.30 vs 1.23). A line belongs to a silhouette, not to the surface behind it, so it slides across that surface with parallax. This measure counts that as flicker, and it is inherent to outlines, not a fault of either method.
- **Object-space strokes don't swim:** they stay on the surface. Their flicker comes from aliasing at dab edges, the view-dependent overshoot and the quantized bands. Kuwahara flickers less because it smooths, at 34–44 ms. A screen-space stroke texture wasn't measured.
- **Flicker grows from 0.5 to 2 px/frame and then levels off.**

### Correctness

| | wide | close | canopy |
|---|---|---|---|
| real | 0.13, 0.17% | 0.25, 0.18% | 0.14, 0.17% |
| cel | 0.24, 0.79% | 0.39, 1.03% | 0.19, 0.57% |
| cel_ss | 0.25, 0.51% | 0.42, 0.58% | 0.20, 0.42% |
| paint | 0.27, 0.60% | 0.45, 0.66% | 0.19, 0.44% |
| paint_kuw | 0.15, 0.29% | 0.28, 0.32% | 0.14, 0.22% |
| real_simple | – | 0.21, 0.10% | 0.10, 0.10% |
| cel_simple | – | 0.34, 1.07% | 0.17, 0.60% |
| paint_simple | – | – | 0.25, 0.54% |

Mean |difference| /255 and the share of pixels over 8/255, fast against the brute-force reference.
- **Step caps:** primary caps hit ≤ 0.004% of covered pixels. Shadow caps hit 0–0.19% of shadow marches, the most in the canopy's soft shadows; the hard cel shadows almost never hit them. The references hit ≤ 3 primary caps and no shadow caps.
- **The realistic baseline passes everywhere.** It needed a tenth-of-a-pixel hit tolerance, and the penumbra estimator changes listed under Method.
- **The quantized looks fail the over-8/255 share** while their means pass. They amplify sub-threshold differences: a 2/255 change in a soft-shadow estimate moves a cel terminator or a painted value band, and a sub-pixel change in a line's closest-approach estimate moves its anti-aliased edge (the diff images, `results/*-diff.jpg`).
- **Kuwahara passes,** because it smooths the same differences away.
- **The reference shares the estimators by design** (same soft-shadow formula, same quantization), so what fails is the fast path's sampling as seen through quantization. Whether a style-aware threshold is right is the owner's call. The criteria haven't changed.
- **What the reference also checks:** that the wolf's part-group bounds are exact (it evaluates the authored `wolf_field`), and that 0.35 of the strict displacement Lipschitz bound is safe for this canopy. Both hypotheses held here.

### How it looks

The looks' strongest shots are `results/wide-cel.jpg`, `canopy-paint_simple.jpg`, `wide-paint.jpg`, `close-paint_kuw.jpg` and `close-cel_simple.jpg`.

- **Realistic** is competent and plain: an early-2010s tech demo. The canopies read as lumpy "broccoli": displaced blobs with AO and soft shadows, but no leaves. The ground is flat green, the haze greys the forest, and the backlit canopy looks glossy. The wolf holds up. These trees are stand-ins, and spike 03 is the place to judge realistic foliage.
- **Cel** is the most attractive look here. Field-native outlines are clean, constant-width and correctly broken where parts touch. The thresholded shadows and rim light read as storybook anime.
  - **Simple content suits it best** (`close-cel_simple.jpg`): smooth canopy clumps with outlines look like hand-drawn background trees, and the fur-free wolf gets a clean terminator.
  - **Detailed content in cel** gets blotchy canopies and a noisy terminator on the furred wolf. That's the fur's millimetre normal noise, quantized.
- **Painterly (object-space strokes)** is convincing on the world. The tower, canopies and ground read as gouache, and the overshoot gives silhouettes a brushed, broken edge that screen-space filters don't give (`wide-paint.jpg`, `canopy-paint_simple.jpg`).
  - **On the wolf it's weaker:** dabby, with fuzz floating off the legs.
  - **At 4 px the strokes read as dither,** and at 32 px as blotches. 12–16 px works at 1080p.
- **Kuwahara** gives the furred wolf a good oil-paint look (`close-paint_kuw.jpg`). On the scene it mostly smears detail into flat patches.
- **960×540** (`wide-cel-540.jpg`, `wide-paint-540.jpg`, blit-stretched): both stylized looks hold up at the lower resolution. Flat colour and lines hide resolution, which is a known argument for stylization. The realistic look wasn't rendered at 960×540 for comparison, so this is an impression.

### What surprised me

1. **A stylized look made this scene's hard case cheaper, but through content, not shading.** Dropping fur noise and leaf-cluster detail saved 16–45% of the frame. The cel and painterly shading itself always cost more, except for cel's thresholded shadows.
2. **Cel's hard shadows pay for its outlines.** The expensive part of the anime look is the curvature creases, not the outlines.
3. **Object-space strokes cost the same at any density,** and still flicker more than Kuwahara by this measure. "Anchored in object space" removes swimming, not aliasing.
4. **Field-native outlines need exact distances near the surface.** A bounding sphere or a displacement shell's inner bound would draw a line around the bound. So the march has to evaluate the exact field within W + 1 px of every surface, and for the painterly overshoot within 18 px. In compiler terms, it's a fact scoped to "within r of the surface", like D-092's.
5. **Quílez's closest-approach estimate assumes each step equals the previous distance.** With clamped or scaled steps (cell exits, Lipschitz shrink, a directional terrain bound) it invents narrow waists: false banded shadows, then false line speckles. It needs the general two-sphere form, used only for sphere-tracing-like steps.
6. **A parabolic refinement of line distances made things worse:** the displaced canopy's "distance" isn't Euclidean enough to fit.
7. **The authored wolf was most of the close-up's cost,** at 12% of its pixels: one monolithic smooth union with about 15 noise calls per evaluation. Part-group bounds that are exact by the smooth union's own algebra cut the close-up frame by about a quarter in a quick test (122 → 92 ms).

### What carries over to a hybrid renderer

Other spikes point to a hybrid: rasterize what's big on screen, and use fields for lighting, distances, small instances and cooking. The techniques here split by what they need from the field.

**They need only the field at the visible surface,** so they carry over to rasterized surfaces with the field evaluated per pixel (spike 01's shading path):
- **curvature crease lines** (five evaluations per pixel)
- **thickness and AO**
- **object-space stroke anchoring:** a rasterizer interpolates the rest-space position, which is all the solid stroke texture needs; stroke orientation comes from the gradient that per-pixel shading already computes
- **cel's thresholded shadows,** with their early-out, as long as shadows stay field-traced

**They need the ray's path in front of the hit:**
- **field-native outlines**
- **the strokes' silhouette overshoot**

A rasterizer doesn't march that path. Two ways they could survive, both untested:
- **A silhouette pass:** find candidate pixels near a rasterized object's edge in screen space (a dilation of its ID or depth), then march only that object's field over its depth interval for those pixels. The cost goes with silhouette length × line width, not screen area.
- **Mesh silhouettes:** extracted meshes have exact silhouette edges, and toon renderers draw those (an inverted hull, or edge extraction). Width control is then a mesh technique, not a field one.

**What changes:**
- **The content saving.** Here, dropping fur and foliage detail saves march steps. With rasterization it saves triangles and extraction instead, which spikes 01 and 07 price.
- **Unchanged:** the screen-space baselines (Sobel edges, Kuwahara) don't care how the surface was produced.

### Caveats

- **Timing:** contaminated by throttling (above). The baseline is far over budget, so the extras sit on an unrepresentative frame. Only the M4 in Chrome was tested.
- **Run time:** 4–5 minutes, over the ~3 asked for, because the baseline frames are slow.
- **No anti-aliasing anywhere** except the field-native lines' analytic edge. No TAA, no upscaler. 960×540 is a cost, not a quality.
- **The scene is static:** no wind, no animation. The flicker measure covers camera motion only.
- **Stand-ins:** the trees (spike 03 has real foliage) and the scene's own terrain.
- **Limits of the line comparison:** the reference shares the field method's definition of a line, and outline flicker counts inherent parallax.
- **"Stylized foliage" here is a soft-edged surface,** not a volume.
- **Which numbers are hypotheses:** every claim about what an optimized implementation or a cheaper scene would cost.

### What this means for the art direction

Nothing here is recorded in `decisions.md`; that's the owner's call.

- **Cel with field-native outlines** is the cheapest stylized look to make beautiful, and fields do something here that screen space can't: exact, controllable line width with analytic anti-aliasing, at roughly 0.9 IoU against exact lines, against 0.4–0.7 for screen-space methods. It fits a 1 ms post slice only without curvature creases. Creases would need a cheaper source (fewer evaluations, or the AO samples reused); that's a hypothesis.
- **Painterly** looks right on landscapes and architecture. Per-pixel solid strokes cost 3–9 ms of shading at 1080p, independent of density; the next thing to try is shading strokes at a lower rate or baking them.
- **Realism** costs the most in exactly the cases stylization simplifies: fur and leaf detail. Whichever look is chosen, simple content saves more than any shading choice costs.

## Quiet-machine rerun (2026-10-02)

Run: `results/run-2026-10-02T04-28-42-984Z.json` (227 s; serial, after a cool-down, Chrome 154 headless, alone on the GPU), against the README's `results/run-2026-10-02T03-06-35-900Z.json`. Triples are wide / close / canopy, GPU ms at 1080p unless marked; bold marks a look's worst camera. Correctness, flicker, line quality and step counts are identical in the two JSONs; only timings moved.

| Metric | README value | Quiet value |
|---|---|---|
| `real` frame: 1080p; 960×540 | 38.0 / 90.0 / 48.6; 9.8 / 30.9 / 12.6 | 38.0 / 86.5 / 42.3; 9.8 / 23.4 / 11.5 |
| Extra over `real`: cel | +0.3 / **+5.3** / +2.7 | +0.3 / **+4.0** / +0.5 |
| Extra: cel_ss | −2.6 / −1.6 / −0.7 | −2.6 / −3.9 / −2.8 |
| Extra: paint | +4.0 / **+28.4** / +4.4 | +4.0 / **+8.3** / +2.6 |
| Extra: paint_kuw | +33.9 / **+55.8** / +41.7 | +33.8 / +35.1 / **+35.8** |
| Extra at 960×540: cel; paint | −0.5 / +1.7 / +1.8; +0.7 / +3.1 / +1.2 | −0.4 / +0.2 / −0.3; +0.6 / +1.6 / +0.3 |
| Simple content, close: real_simple; cel_simple | −22.5 (−25%); −15.9 (−18%) | −31.0 (−36%); −28.6 (−33%) |
| Simple content, canopy: real_simple; cel_simple; paint_simple | −7.7 (−16%); −8.8 (−18%); −10.0 (−21%) | −9.4 (−22%); −10.2 (−24%); −8.3 (−20%) |
| Line IoU on `wide`, W = 1 / 2 / 4 / 6: field-native; Sobel; disk | 0.83 / 0.91 / 0.94 / 0.96; 0.36 / 0.39 / 0.37 / 0.37; 0.59 / 0.69 / 0.62 / 0.56 | 0.825 / 0.905 / 0.943 / 0.955; 0.361 / 0.392 / 0.374 / 0.368; 0.589 / 0.692 / 0.619 / 0.557 |
| Flicker excess over `real`, `wide`, 2 px/frame (/255, pp): cel; cel_ss; paint; paint_kuw; paint_simple | +0.99, +3.9; +0.92, +3.1; +0.43, +1.3; +0.06, −0.1; +0.30, +0.9 | +0.99, +3.9; +0.92, +3.1; +0.43, +1.3; +0.06, −0.1; +0.30, +0.9 |
| Correctness, share over 8/255 (every mean ≤ 0.45/255) | real 0.10–0.18%, paint_kuw 0.22–0.32%, cel 0.57–1.07%, cel_ss 0.42–0.58%, paint 0.44–0.66%; primary caps ≤ 0.004% | the same ranges; primary caps ≤ 0.004% of covered pixels, shadow caps ≤ 0.19% of marches |

**Verdict changes: none.** By the README's own criteria cel (worst +4.0 ms, close), paint (+8.3, close) and Kuwahara (+35.8, canopy) still fail the 2 ms budget, cel_ss still passes, correctness and flicker are unchanged, and neither real_simple (−22%) nor paint_simple (−20%) reaches the 25% canopy saving; the margins moved, most of all paint's close-up extra (+28.4 → +8.3 ms).
