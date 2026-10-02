# Spike 11: hair and cloth

*A measurement spike for the flagship's moving things. It's throwaway code, not the start of the engine.*

## The question

**Can thin things that deform freely work in a field world, both simulated and drawn within budget?** The things: long hair, a hero's cape, banners on a tower, NPC clothing.

This may be pure fields' hardest case:
- A sheet that moves arbitrarily has no cheap distance bound. Its exact distance is the distance to whatever triangles or patches the simulation produced, and nothing cheaper is known to bound it.
- Warping space to deform a rest shape breaks the bound (spike 02's first finding: a traced field can't be skinned).
- A thin sheet is the worst case for sphere tracing: a ray that passes near it but misses takes steps no longer than its distance to the sheet.
- Hair is thinner still: strands far below a pixel.

The spike finds where the limits are, by building the pure-field versions and the hybrid fallback on the same simulation and timing both.

## Kill criteria, written before measuring

Budgets come from `spikes/README.md` (a hypothesis, not a decision). Cloth and hair live inside the **creatures-and-moving-things slice, 2.5 ms at 1080p.** They get:

| | Budget (native 1920×1080, M4) |
|---|---|
| **Draw:** cloth and hair together | **≤ 1.5 ms** |
| **Simulate:** cloth and hair together | **≤ 0.5 ms of GPU** |

**The scene that's judged (`courtyard`):**
- one hero with a cape and long hair, in the foreground
- 20 NPCs with simple clothing: robes and tunics, walking
- 10 banners in the wind on a stone tower: 6 hanging from the wall, 4 flags on poles

**Verdict, per technique** (both budgets must hold):

| Result | Verdict |
|---|---|
| Within budget | **Pass** |
| Up to 2× (draw ≤ 3.0 ms and sim ≤ 1.0 ms) | **Inconclusive** |
| Over 2× | **Fail** |

**What's counted.** Measured from GPU timestamps under sustained load (30 warm-up frames, then the median of 90 back-to-back frames), as spikes 01 and 02 do.
- **Sim** is every simulation pass: cloth and hair.
- **Draw** is the *marginal* cost of having cloth and hair, as in spike 02: every non-sim pass of the variant (building bounds, children strands, hair volume, binning, shadow map, depth copy, rasterization, tracing) minus the same passes in the `base` variant, which draws the same bodies, tower and ground with no cloth or hair. It includes their shadows on the world and the world's shadows on them.

**Correct, not just fast** (protocol rule 3). Every fast path is compared against a brute-force reference render of the same content, from the same simulation state. The thresholds are spike 02's, adopted before measuring:
- mean difference **≤ 0.5/255**
- **≤ 0.5%** of pixels off by more than 8/255
- step-cap hits **≤ 0.1%** of cloth pixels

A fast number with a wrong image doesn't count.

**Looks.** Screenshots of every scene and technique, judged by eye. Beauty is part of the question, so this is reported honestly, not scored.

## The fallback, if pure fields fail

**Simulated meshes, rasterized and composited by depth** (the `raster` technique below). The world is still ray-marched; cloth triangles and hair ribbons are rasterized over it against the marched depth, with a shadow map for cloth.

## Method

The design below is what was measured. Several choices were made after exploratory measurements; the criteria didn't change. They're listed under "Changes made while building", with the reasons.

### The world (the `base` variant)
Everything that isn't cloth or hair is ray-marched in one compute kernel, spike 02's way:
- A courtyard: flagstone ground, a stone tower with battlements, a sky with clouds.
- 21 bodies made of rigid parts: 14 round cones each, smooth-unioned, posed on the CPU per frame. The hero stands; the NPCs walk in circles.
- Scatter binning into screen tiles (8×8 pixels) and a 256² light grid for shadows: count, prefix sum, fill, so lists never overflow. Items that cover many cells are binned by a whole workgroup.
- Soft marched sun shadows from the bodies and the tower.

### Simulation (one GPU pass for cloth, one for hair)
- **Position-based dynamics:** Verlet integration and Jacobi constraint projection, with 8 substeps per 60 Hz frame and 1 iteration per substep ("small steps", Macklin et al. 2019). Collisions are resolved once per substep.
- **Constraints:** stretch, shear and bending distance constraints on a particle grid. Long-range tethers to the pinned particles stop a sheet stretching under its own weight. Robes are cut wider than the shoulder ring they're pinned to, with jittered rest lengths, so they fold.
- **One workgroup per sheet** (and per batch of 16 hair guides). Constraints never cross a group, so every substep and iteration runs inside one dispatch, separated by storage barriers, with no per-iteration dispatch overhead.
- **Pins** follow the bodies: the cape along the shoulders, robe and tunic tubes round the shoulders, banners along a rod, flags along a pole. Hair roots (two particles per guide, so they keep a direction) follow the head.
- **Wind** with gusts. Cloth feels it through its normal, hair through drag. Hanging banners are sheltered by the wall and get less of it than the flags do.
- **Collision** against the character's own body (its round cones), the tower and the ground. Hair collides with the head, neck, shoulders and torso, inflated so it rests on top of the cape.
- **Not simulated:** self-collision, cloth against cloth, hair against the cape's actual particles.

### Cloth, drawn three ways
1. **`field`: cloth as a field built from the simulation.**
   - Each sheet is cut into patches of P×P quads. P = 1 by default; P = 2 is a sweep point.
   - Every frame, each patch gets a bound from its particles: an axis-aligned box intersected with a slab along the patch's mean normal. The triangles are convex hulls of particles, so the bound contains them.
   - Inside a patch, the field is the exact distance to its 2P² triangles minus half the cloth's thickness (4 mm). That's an exact distance, so sphere tracing on it is safe.
   - Patches are scatter-binned. Each ray sphere-traces the patches in its tile, each within its own bound interval.
   - Shadow rays march the same field, through the light grid's lists, as hard shadows. The field's classic soft-penumbra estimate is a measured variant.
2. **`tri`: the same patches, intersected analytically.** Same binning and bounds, but each candidate patch's triangles are intersected directly, with no stepping. This isn't a field, but it's pure compute, and it shows what the field formulation costs over the triangles it's built from.
3. **`raster`: the fallback, raster first.**
   - A depth prepass rasterizes the cloth (and hair ribbons).
   - The marched world is then traced with that depth as a bound on every march, so nothing behind the meshes is marched or shaded.
   - A cheap fullscreen pass writes the world's depth where the world is nearer. The meshes are then shaded once per visible pixel, with an equal depth test and no discard.
   - A 2048² shadow map of the cloth gives cloth's shadows on everything. Cloth fragments march the world's field for the world's shadows on them.

### Hair, drawn three ways
The simulation moves 128 guide strands of 16 particles. Every frame, a compute pass (one workgroup per render strand) generates N render strands from them: interpolated toward a neighbouring guide, offset into clumps, and Catmull-Rom subdivided to 30 segments. N is the strand count swept. Each strand is a wisp of radius 0.6 mm × 2048/N, tapered, so the hair's total coverage stays the same as N changes.
1. **`volume`: hair as a flow-aligned density field.**
   - The render strands are splatted into a 64×96×64 grid (about 8 mm voxels) that holds extinction and summed strand direction.
   - The trace marches it, front to back, with Marschner-style R/TT/TRT longitudinal lobes along the stored direction (Kajiya–Kay diffuse).
   - Strand-scale detail comes from noise stretched along the flow.
   - A coarse distance-to-hair grid skips empty space.
2. **`strands`: hair as strands.**
   - Segments are scatter-binned into screen tiles and intersected analytically: closest approach between the ray and the segment.
   - Coverage comes from the pixel's footprint, so strands far below a pixel stay smooth.
   - A pixel keeps the 4 nearest hits and composites them front to back. Layers beyond 4 keep their exact total transmittance; their colour is approximated by the last kept layer's.
3. **`raster` hair:** the same segments as camera-facing ribbons in the raster-first pipeline. Opaque, no anti-aliasing.

**Hair shadows come from a density grid in every technique.** A 32×48×32 transmittance grid, marched toward the sun once per frame, gives self-shadowing. The head, neck, torso and tower occlude it too. Shadow rays from the world, cloth and bodies march the density for hair's shadow on them. Production strand renderers do the same (deep opacity maps or voxelized density). So the techniques differ only in primary visibility.
- **Volume hair** uses the 64×96×64 grid.
- **Strand and raster hair** use the grid only for shadows, at 32×48×32 (16 mm voxels), which costs less to build.
- **The transmittance lookup is biased one cell toward the sun.** Without the bias it bands along the cell layers, the classic deep-shadow-map artifact.

### Timing
- GPU timestamps per pass: 30 warm-up frames, then 90 back-to-back frames; medians and p95.
- **Render passes overlap the compute pass before them on this GPU.** A render pass's begin timestamp is taken while the trace is still running, so raw pass durations don't add up: the raster frame's passes summed to 9.6 ms against a 6.0 ms frame span. So:
  - each pass is charged its serialized share, end − max(begin, previous end);
  - verdicts use the frame span, first begin to last end.
- **Draw is marginal:** (frame span − sim) − base frame span, where the base is the mean of the two base measurements bracketing the technique. Clocks drifted by up to 40% within one set, so a single base before the set wasn't enough.
- **Sim** is the two simulation passes.
- **A 60 Hz paced run** (busy-wait) of the judged view, for the pure-field and raster techniques.

### Reference renders
- **Same content and state:** the same simulation state and the same shading model, including the hair light grid (its own error is measured separately, below).
- **No binning:** every ray tests every character, every sheet (through per-sheet bounds) and every strand (through per-strand bounds).
- **Brute-force parameters:** half-size steps, a tenth of the hit tolerance, step caps in the thousands, a quarter-voxel step through hair volume with no skipping, and 16 strand layers instead of 4.
- **Tiled:** rendered in 320×64 tiles, one submission each (GPU safety rules).
- **Matched to the technique:** the reference for `field` cloth is the same thickened field; for `tri` and `raster` cloth it's the exact triangles. The `raster` comparison includes the shadow map's error and the unanti-aliased ribbons'.
- **Reported alongside:** the floor (the base against its own reference: errors of the world itself, the same in every technique), and the error over only the pixels cloth or hair change.

## Sweeps

All at the `courtyard` view, native 1080p, unless noted.
- **Cloth resolution:** half, default and double particles per sheet axis. The cape is 13×14, 24×26 and 47×51 particles; all cloth is 4.6K, 15.4K and 58.8K particles.
- **Patch size** for the traced cloth: 1×1 and 2×2 quads, courtyard and hero views.
- **Strand count:** 512, 2,048 (default) and 8,192 render strands from the same 128 guides, courtyard and hero views.
- **Clothed NPCs:** 0, 10, 20 (default) and 40.
- **Solver settings:** substeps × iterations of 8×1 (default), 4×5, 4×3, 4×1 and 2×3.
- **Resolution:** every view and main technique at 1920×1080 and at a 960×540 internal resolution. No upscaler is implemented, so no upscaled quality is claimed.
- **Views:** `courtyard` (judged), `hero` (cape and hair fill the screen) and `banners` (looking up the tower, grazing views of the sheets).

## Layout

| File | What |
|---|---|
| `scene.js` | Content: bodies and their walk cycle, sheets and their constraints, hair guides and render-strand parameters, pins, cameras |
| `common.wgsl` | Frame uniform, buffer layouts, noise, primitives, ray intersections, sky |
| `world.wgsl` | The world's fields, cloth and hair lookups, shadows and shading (shared by the trace and the raster fallback) |
| `sim.wgsl` | The PBD solver: one workgroup per sheet or hair batch |
| `build.wgsl` | Cloth normals and patch bounds |
| `hair.wgsl` | Render strands, density splat, empty-space and light grids |
| `bin.wgsl` | Scatter binning and the prefix sum |
| `trace.wgsl` | The marched frame: world, bodies, cloth (field or triangles), hair (volume or strands), shadows |
| `raster.wgsl` | The fallback: shadow map, depth prepass, world depth, cloth triangles, hair ribbons |
| `main.js` | Harness: pipelines, content, timing, sweeps, references, screenshots |
| `results/` | Raw JSON from each run, JPEG screenshots (PNGs are ignored by git) |

## Running it

```bash
python3 spikes/serve.py 8417
```

```bash
spikes/headless.sh 11-hair-cloth '#run' 600
```

- **`#run`** takes about 2 minutes on this machine and saves `results/run-<timestamp>.json`, screenshots and diff images.
- **`#quick`** renders every view in three techniques, in about 3 s.
- **Development modes:** `#time=view:tech,...`, `#prof=view:tech,...` (every kernel in its own timed pass), `#qual=view:tech,...` (references and diffs) and `#simlook=8x1,4x5,...`. Options go after a `|` in the hash, for example `'#time=courtyard:field|cloth=2&P=2'`.

## Results (2026-10-01, indicative)

**Setup:**
- MacBook Air M4 (8-core GPU), Chrome 154 headless via `spikes/headless.sh`, page visible.
- The final run is `results/run-2026-10-02T03-28-53-462Z.json`. It took 124 s.

**These timings are contaminated and only indicative.** Nine other spikes were being built on the same machine at the same time. The GPU lock serialized the runs, but the fanless M4 was thermally loaded, and clocks drifted by up to 40% within a single run: the courtyard base went from 3.54 to 5.05 ms within one bracketed set. Marginal costs moved by about ±0.5 ms between four full runs of the same or near-identical code. Where it matters, the range over those four runs is given (two earlier runs used soft field shadows, and three had no base bracketing). Only the final run's JSON is kept: each run cleared `results/` first, so the earlier runs' numbers come from the logs of the session that wrote this. The final timing should be redone on a quiet machine. Correctness numbers are deterministic: the last two runs (same code) gave identical ones.

### Verdict (judged view: `courtyard`, native 1920×1080)

Budget: draw ≤ 1.5 ms and sim ≤ 0.5 ms; up to 2× is inconclusive.

| Technique | Draw, final run (range over 4 runs) | Sim | Verdict | Correct? (mean, % > 8/255) |
|---|---|---|---|---|
| **Pure fields:** cloth field + hair volume | **2.03 ms** (2.03–2.23) | 0.39 ms | **Inconclusive** (1.4× budget) | **Yes:** 0.25/255, 0.38% |
| Cloth field + hair strands | 2.85 ms (2.85–3.47) | 0.39 ms | Inconclusive to fail | No, narrowly: 0.27/255, 0.51% |
| Traced triangles + hair strands | 3.47 ms (2.49–3.47) | 0.39 ms | Inconclusive (fail in the final run) | Yes: 0.22/255, 0.43% |
| **Fallback:** rasterized meshes, raster first | **1.38 ms** (1.18–1.70) | 0.39 ms | **Pass** (inconclusive in one run) | No: 0.41/255, 1.05% (unanti-aliased hair, shadow map) |

- **Sim passes** in every configuration: 0.33 ms for 15.4K cloth particles and 0.07 ms for 2,048 hair particles (0.33–0.46 ms in total over the four runs).
- **The world's own floor** (the base against its own reference) is 0.29/255 and 0.43%. Most of the 0.5% allowance is spent before cloth and hair are added.
- **No technique hit a step cap.** Cloth caps were 0 in every view.

**Other views and resolution** (draw ms, final run, with the range over runs):

| View | Field + volume | Field + strands | Triangles + strands | Raster |
|---|---|---|---|---|
| `hero` 1080p (cape and hair fill the screen) | 6.06 (3.9–6.1) **fail** | 6.49 (5.2–6.5) fail | 4.65 (4.3–5.9) fail | **0.33 (0.3–1.7) pass** |
| `banners` 1080p (grazing sheets) | 1.05 pass | 0.85 pass | 0.72 pass | 1.25 pass |
| `courtyard` 960×540 | 1.54 (1.1–1.7) | 2.82 | 2.62 | 1.31 |
| `hero` 960×540 | 1.57 (1.1–1.6) | 3.34 | 2.82 | 1.11 |

**60 Hz pacing** (busy-wait, judged view): the GPU frame stretches to a median of 14.7 ms for both pure fields and raster, against 6.1 and 6.8 ms sustained, because the OS lowers clocks. 0 of 150 frames went over 16.7 ms. As in spikes 01 and 02, pacing shows the clocks the OS picks, not the headroom.

**So:**
- **Pure fields are viable for cloth and hair at distance and mid-range,** but not within budget at native 1080p in the judged scene. They come in at about 1.4× the draw budget: inconclusive. At 960×540 they pass or nearly pass.
- **They fail in close-ups:** 4–6 ms when the cape and hair fill the screen.
- **The fallback passes at both distances,** and it's cheapest exactly where pure fields are weakest: the close-up. Its correctness failures (unanti-aliased hair ribbons, shadow-map self-shadowing) are the usual raster ones, which TAA, MSAA and better shadow filtering address (a hypothesis here: none of those was implemented).

### Where the time goes (courtyard, 1080p, final run, bracketed; ranges over runs)

| Component | Draw (marginal) | Notes |
|---|---|---|
| Cloth as a field (P = 1, hard shadows) | 2.23 ms (1.05–2.23 over runs) | 0.56 ms without cloth's shadows: **shadows are most of it** |
| Cloth as a field, soft-estimate shadows | 1.11 ms (1.11–2.10 over runs) | Hard and soft are indistinguishable within this noise at P = 1; they differ at P = 2 (see the patch sweep) |
| Cloth as traced triangles | 0.89 ms (0.59–1.11) | 0.20 ms without cloth's shadows |
| Cloth rasterized | 0.30 ms (0.30–0.85) | Shadow map below the timestamp quantum. The trace skips what the cloth covers. |
| Hair as a volume (2,048 strands) | 0.89 ms (0.89–1.51) | Of which 0.59–0.79 ms builds the 64×96×64 grid every frame |
| Hair as strands | 2.20 ms (1.97–2.56) | 1,147 segment candidates per hair pixel at this distance |
| Hair rasterized (ribbons) | 1.41 ms (1.38–1.64) | 0.46–0.59 ms is the depth prepass of 61K ribbons |

- **Raw render-pass durations overlap.** In the final run's courtyard raster frame, the shading pass's raw duration is 5.5 ms; its serialized share is 1.2 ms (see Timing).
- **Building the hair grids:**
  - coarse (32×48×32), for shadows only: 0.26–0.33 ms
  - fine (64×96×64), for volume hair: 0.59–0.79 ms
  - The fine grid's cost is splat atomics (0.2 ms), the resolve (0.14 ms) and the light march (0.13–0.16 ms).
- **Binning** is 0.07–0.26 ms in every configuration, after big items were spread across workgroups. A one-thread-per-item version cost 1.8 ms in the close-up.

### How small the safe steps get (cloth as a field)

| View | Steps per patch march | Steps per cloth pixel | Marches that miss | Step sizes | Smallest step | Worst pixel |
|---|---|---|---|---|---|---|
| `courtyard` | 1.34 | 2.75 | 13% | 53% 1–4 mm, 45% 4–16 mm, 2% 16–64 mm | 1.1 mm | 46 steps |
| `hero` | 1.79 | 3.77 | 21% | **22% under 1 mm**, 57% 1–4 mm, 20% 4–16 mm | **0.37 mm** | 104 steps |
| `banners` (grazing) | 1.17 | 1.70 | 11% | 18% 1–4 mm, 75% 4–16 mm | 2.0 mm | 45 steps |

- **The steps are tiny, as expected for a 4 mm-thin sheet,** but there are very few of them. The patch's bound (box ∩ slab) clips each march to a sliver a few millimetres thick, so a ray rarely takes more than two steps per patch.
- **The cost is in evaluating the field, not in stepping.** Each step is the exact distance to every triangle in the patch, and each pixel tests about two patches.
- **The pathology the brief predicted is real, but it lives in shadows and bigger patches.** With 2×2-quad patches, a shadow ray leaving a robe at a grazing angle creeps along the sheet. Hard shadows then cost 7.6 ms against 6.3 ms soft (courtyard, cloth only), and 11.9 against 10.2 ms in the close-up.

### Sweeps (courtyard, 1080p, final run; draw ms)

**Patch size, cloth only:**

| Patch | Field | Field, soft shadows | Triangles | Steps per march |
|---|---|---|---|---|
| 1×1 quad (default) | 2.23 | 1.11 | 0.89 | 1.34 |
| 2×2 quads | **7.64** | 6.26 | 0.59 | 2.32 |

- **Single-quad patches make the field 3–5× cheaper.** Each step evaluates 2 triangles instead of 8, inside a tighter bound.
- **Triangles don't care about patch size** (within noise).
- The hero view at 2×2 costs 11.9 ms for field cloth.

**Cloth resolution, cloth only:**

| Particles (cape) | Sim, cloth | Field | Triangles | Raster |
|---|---|---|---|---|
| 4.6K (13×14) | 0.13 | 1.05 | 0.26 | 0.00 |
| 15.4K (24×26), default | 0.33 | 2.23 | 0.89 | 0.30 |
| 58.8K (47×51) | **2.03** | 4.39 | 2.56 | 0.59 |

- **The sim is cheap until a sheet outgrows a workgroup.** At double resolution, each of 256 threads handles about 9 particles per step, and cloth sim jumps 6× to 2 ms: 4× the sim budget.
- **Default resolution** (15K particles across 31 sheets, about 400 per sheet) **is the practical limit** for this solver structure on this device.

**Strand count, hair only** (from the same 128 simulated guides; wisp radius scaled so coverage is constant):

| Render strands | Volume, courtyard / hero | Strands, courtyard / hero | Raster, courtyard / hero | Strand candidates per hair pixel, hero |
|---|---|---|---|---|
| 512 | 0.69 / 2.33 | 1.21 / 1.87 | 0.56 / 0.59 | 101 |
| 2,048 | 0.89 / (n/a) | 2.20 / (n/a) | 1.41 / (n/a) | 291 (courtyard: 1,147) |
| 8,192 | 1.84 / 4.03 | **6.16 / 12.29** | 4.03 / 4.42 | 1,130 |

- **Traced strands scale with strands per pixel.** Strands per pixel *grow* with distance: the courtyard's distant hero has 4× the candidates per hair pixel of the close-up. So strands need level of detail: fewer, thicker wisps at distance, or a switch to the volume.
- **The volume's cost is mostly its per-frame build.** The build scales with strand count (splat atomics); the march doesn't.
- **Rasterized ribbons scale with ribbon count** through the depth prepass: 2.5 ms for 245K ribbons in the close-up.

**Clothed NPCs (full techniques):**

| NPCs | Sim | Field + volume | Triangles + strands | Raster |
|---|---|---|---|---|
| 0 | 0.26 | 1.57 | 3.44 | 1.18 |
| 10 | 0.39 | 2.46 | 3.21 | 1.38 |
| 20 | 0.39–0.52 | 2.03 | 3.47 | 1.38 |
| 40 | **0.79** | 3.05 | 3.08 | 1.08 |

- **Draw cost tracks screen coverage, not NPC count.** At 0 NPCs the strands still dominate, because the hero's hair is in view. The variation between 10, 20 and 40 is mostly noise.
- **Sim cost steps up with the number of sheets** at 40 NPCs: 51 workgroups is more than this GPU runs in one wave.

**Solver settings (sim only, ms per 60 Hz frame):**

| Substeps × iterations | 8×1 (default) | 4×5 | 4×3 | 4×1 | 2×3 |
|---|---|---|---|---|---|
| Cloth | 0.33 | 0.72 | 0.46 | 0.20 | 0.20 |
| Hair | 0.07 | 0.13 | 0.07 | 0.07 | 0.07 |

- **Cost tracks the number of barrier-separated steps:** about 25–30 µs each, whatever the work, because the solver is latency-bound, not particle-bound.
- **8×1 looked the same as 4×5** in stills (cape billow, robe drape) at about half the cost. 4×1 hung visibly differently. Motion wasn't compared.
- **Keeping a sheet in workgroup memory was slower** (0.8 vs 0.6 ms at 4×5), presumably because 32 KB per workgroup leaves one resident per core.

### Correctness (1080p, final run)

Each technique against its own brute-force reference. "Changed pixels" is the error over only the pixels cloth or hair changes.

| View | Technique | Mean /255 | > 8/255 | > 8/255 of changed pixels |
|---|---|---|---|---|
| courtyard | floor (base) | 0.29 | 0.43% | — |
| courtyard | field + volume | 0.25 | **0.38%** | 1.3% |
| courtyard | field + strands | 0.27 | 0.51% | 2.7% |
| courtyard | triangles + strands | 0.22 | 0.43% | 2.3% |
| courtyard | raster | 0.41 | 1.05% | 8.2% |
| courtyard | cloth field / triangles / raster only | 0.25 / 0.20 / 0.41 | 0.38% / 0.31% / 0.99% | 1.3% / 0.9% / 7.7% |
| courtyard | hair volume / strands / raster only | 0.29 / 0.31 / 0.40 | 0.42% / 0.56% / 0.75% | 0.2% / 21.6% / 52.8% |
| hero | field + volume | 0.39 | 0.67% | 1.9% |
| hero | field + strands | 0.34 | 0.50% | 1.3% |
| hero | triangles + strands | 0.25 | **0.42%** | 1.1% |
| hero | raster | 0.92 | 2.87% | 10.8% |
| banners | all four | 0.21–0.23 | 0.33–0.40% | 3.9–5.1% |
| courtyard 960×540 | field / triangles / raster (floor 0.74%) | 0.40 / 0.37 / 0.53 | 0.66% / 0.74% / 1.22% | 2.5% / 4.2% / 8.6% |

- **Where field cloth differs:** at silhouettes, from the hit tolerance on a 4 mm sheet, and along its shadows.
- **Where strands differ:** where more than 4 strand layers overlap. That's distant hair in the courtyard (21.6% of hair pixels); the tail's colour is approximated there.
- **Where raster differs:** the hair ribbons alias, with no anti-aliasing (52.8% of hair pixels), and the shadow map's self-shadowing doesn't match traced shadows in robe folds.
- **The hair light grid's own error,** which no technique escapes: against exact per-sample light marching, 2.4% (strands) and 2.7% (volume) of the hero frame is off by more than 8/255, which is 54–58% of hair pixels. Before the sun-ward bias that removes banding it was 1.3–1.6%. The bias trades accuracy for the absence of banding.

### Looks, honestly

Screenshots are in `results/`. The views are `courtyard-*`, `hero-*` and `banners-*`, plus hair crops (`*-hair-crop.jpg`), heat maps and diff images. The JPEGs were made from the final run's PNGs with `sips`; a new run writes new PNGs (git-ignored) and leaves them alone.
- **The courtyard reads as a pleasant, sunlit scene,** but it's clearly a prototype: mannequin bodies with egg heads, flat ambient light, no anti-aliasing, a plain ground.
  - **The cloth is the most convincing part.** The hero's cape billows toward the camera with a gold trim and an ochre lining. The flags fly, and the heraldic banners hang on the tower and flutter at the edges.
  - **The robes read as robes** with hem bands, but they hang too smooth and bell-like. The fold seeding produces only gentle folds.
  - **The cloth looks the same** in all three techniques: the simulation, not the drawing, decides the shape.
- **Hair is where the techniques differ:**
  - **Traced strands look best,** the only result here that looks like hair rather than a hair-coloured shape: fine anti-aliased wisps, highlights and a believable silhouette (`hero-tri_strands-hair-crop.jpg`).
  - **Volume hair looks like a soft wig up close** (`hero-field-hair-crop.jpg`): blurry, with faint horizontal banding on the crown from 8 mm voxels. It's fine at the courtyard's distance.
  - **Rasterized ribbons are stringy and aliased** without TAA (`hero-raster-hair-crop.jpg`), but would be the cheap production choice with TAA (a hypothesis).
- **8,192 strands** look finer than 2,048, and 512 look like thick noodles.

### What surprised me

1. **Render passes overlap compute in GPU timestamps.** A raster pass's begin timestamp was taken while the trace was still running, so raw pass durations (5.5 ms) said the raster shading was 4–5× more expensive than it is (1.2 ms). I'd spent effort optimizing a cost that wasn't there before the frame span exposed it. Every spike that mixes raster and compute should judge on frame spans.
2. **Cloth as a field is about bounds, not steps.** The safe steps get small (0.37 mm), but a tight per-quad bound makes each march 1–2 steps, and 2×2 patches cost 3–5× more than single quads. Even so, field cloth costs 2–3× its own triangles intersected directly: the "field" of a simulated sheet is a slower way to intersect a mesh.
3. **Shadows are most of the cost of thin traced things.** Cloth without its shadows: 0.56 ms (field) and 0.20 ms (triangles); with them, 2.2 and 0.9 ms. The low sun puts many patches in each light-grid cell.
4. **The simulation is latency-bound:** about 25 µs per solver step whatever the work. So "small steps" (8×1) fit the budget, more particles per sheet are nearly free until a sheet outgrows a workgroup, and workgroup memory made it slower.
5. **Strands get more expensive per pixel with distance.** Hair at 5 m costs more per pixel than hair filling the screen.
6. **The hybrid's raster-first depth bound** makes the fallback nearly free in close-ups. The meshes hide the world behind them, so the trace does less (0.3–1.7 ms for cloth and hair in the hero view).
7. **Hair self-shadowing from a light grid bands**, the classic deep-shadow-map artifact. A sun-ward bias fixes the look and costs accuracy against exact light.

### What this means for the design

Nothing here is recorded in `decisions.md`; that's the owner's call.

- **Pure fields can draw free-moving cloth and hair,** correctly and at distance, but not within budget at native 1080p in the judged scene (about 1.4×: inconclusive), and not in close-ups (2.5–4× over: fail). The named fallback, simulated meshes rasterized and composited by depth, passes at both distances.
- **A simulated sheet's "field" isn't authored content.** It's the distance to the triangles the simulation produced, rebuilt every frame. It gains nothing from being a field that intersecting the same triangles doesn't give, except a natural soft-shadow estimate. The case for marching it is uniformity with the rest of the world, not cost.
- **Hair wants different representations at different distances,** all fed by the same guides:
  - strands up close: best looking, cost ∝ strands per pixel
  - a density field at distance and for shadows: cheap, soft
- **The language side (D-050):** nothing here is engine-specific.
  - "Distance to a set of triangles from a buffer, with per-patch bounds" is a general stdlib operation.
  - So are "splat curves into a density grid" and "march a density with a transmittance grid."
  - The finding for D-086 holds and extends: freely deforming things can't be warped rest fields. They're data (meshes, curves) that a field program reads.

### Caveats

- **Device and browser:** this M4 in Chrome 154 only. Timings are contaminated (above).
- **Not implemented:** anti-aliasing, TAA and upscaling; cloth self-collision; hair–cloth contact beyond an inflated body; the world's sky occlusion and bounce light (spike 05's question); strand level of detail.
- **Timestamps are quantized to about 65.5 µs,** so passes under about 0.2 ms are imprecise, and medians of 0 mean "below the quantum".
- **"What the compiler would emit" is an assumption.** The person writing these shaders also designed the method, and a real compiler or engine could do better or worse.

### Changes made while building, after exploratory measurements

The criteria didn't change.
1. **Patch size 2×2 → 1×1** quads for the traced cloth: 3–5× cheaper field (above).
2. **Solver 4×5 → 8×1** substeps × iterations: half the cost, the same look.
3. **Collisions once per substep** instead of every iteration (standard PBD).
4. **Hard cloth shadows by default** for the field, matching the triangles. Soft is a measured variant.
5. **The raster fallback became raster first**, with the trace bounded by mesh depth, and shading with an equal depth test and no discard. The first version shaded every overdrawn fragment and shaded the world under the cloth.
6. **The hair light grid:** 32×48×32 for every technique, biased sun-ward to remove banding. A coarser shadow-only density grid for strand and raster hair (it was 0.55 ms; now 0.26–0.33 ms).
7. **References use the same hair light grid** as the fast paths, so they measure each visibility technique. The light grid's own error is reported separately.
8. **Strand layers beyond 4** keep their exact transmittance. Without that, distant hair went see-through.
9. **Binning:** items covering many cells are binned by a workgroup. Boxes are clipped at the near plane: an NPC walking through the `banners` camera had flooded the lists, and one frame took seconds.
10. **Timing:** serialized pass shares and frame spans (render passes overlap), and base bracketing (clock drift).

## Quiet-machine rerun (2026-10-02)

Run file: `results/run-2026-10-02T03-56-53-649Z.json` (serial, after a cool-down, Chrome 154 headless; 99 s). The courtyard base drifted 6% within its set (3.54–3.74 ms), against 43% in the final run above. Draw is marginal on frame spans, as in Timing.

| Metric (1080p) | README value | Quiet value |
|---|---|---|
| Courtyard draw: field + volume | 2.03 ms | 2.10 ms |
| Courtyard draw: field + strands | 2.85 ms | 2.79 ms |
| Courtyard draw: triangles + strands | 3.47 ms | 2.36 ms |
| Courtyard draw: raster | 1.38 ms | 1.05 ms |
| Hero draw: field + volume | 6.06 ms | 3.80 ms |
| Hero draw: field + strands | 6.49 ms | 4.95 ms |
| Hero draw: triangles + strands | 4.65 ms | 3.80 ms |
| Hero draw: raster | 0.33 ms | 0.39 ms |
| Sim, cloth + hair (every technique) | 0.39 ms (0.33 + 0.07) | 0.39 ms (0.33 + 0.07) |
| Courtyard correctness, mean /255 and > 8/255: field + volume; field + strands; triangles + strands; raster | 0.25, 0.38%; 0.27, 0.51%; 0.22, 0.43%; 0.41, 1.05% | Identical, as is every other view's (to the README's precision) |
| Cloth step-cap hits, every view | 0 | 0 |

**Verdicts:** none change. Pure fields stay inconclusive in the courtyard (1.4×) and fail in the close-up (3.8–4.9 ms, 2.5–3.3×); the fallback passes both. Triangles + strands is inconclusive (2.36 ms), not the final run's fail; at 960×540 pure fields now pass outright (1.15 and 1.25 ms).
