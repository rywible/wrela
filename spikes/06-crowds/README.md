# Spike 06: crowds and live edits

*A measurement spike for the flagship's open world. It's throwaway code, not the start of the engine.*

## The question

In a ray-marched world, **what do hundreds of moving things, and live world edits, cost?**

The scene is a town square at the foot of a stone tower, on terrain: 300 moving characters and creatures (villagers, farm animals, birds overhead), while the static world is edited continuously: craters, a town wall being blasted, a trench being dug.

## Kill criteria, written before measuring

Budgets come from `spikes/README.md` (a hypothesis, not a decision). Everything is measured at native 1920×1080 on the reference M4, from GPU timestamps under sustained load (30 warm-up frames, then the median of 90 back-to-back frames).

1. **Creatures: the creatures-and-moving-things slice, 2.5 ms.** 300 instances in the `square` view, with birds.
   - Creature time is the *marginal* cost of having them, as in spike 02:
     > creatures = `pose` + `cull` + `bin_screen` + `bin_light` + (`trace` with creatures − `trace` without)
   - It covers posing on the GPU, binning, primary visibility, shading, their shadows on the world and on each other, and the world's shadows on them.
   - **Pass** ≤ 2.5 ms. **Inconclusive** ≤ 5.0 ms. **Fail** > 5.0 ms.
2. **Edits: amortized within the world-geometry slice (3.0 ms).** The spike gives edits a sub-slice of **10% of it, 0.3 ms per frame**, at **20 edits per second** (one edit every three frames at 60 Hz).
   - Edit time is re-cooking: `classify` + `cook` passes, averaged over every frame of the measurement (frames without an edit count as zero).
   - A hitch rule as well: the worst single frame's re-cook must be **≤ 1.0 ms**.
   - **Pass** if both hold. **Inconclusive** if each is within 2× (0.6 ms amortized, 2.0 ms worst). **Fail** otherwise.
   - The world march itself (terrain, tower, houses) is spike 04's question. It's reported here for context, with its sum against the 3.0 ms slice, but it isn't judged.
3. **Correct, not just fast** (protocol rule 3). Each fast path is compared with a brute-force reference render of the same content: the analytic world field with every edit evaluated directly (no bricks), every instance with every part (no binning, no masks), half-size steps, a twentieth of the hit tolerance, step caps in the thousands.
   - **Creature path** (fast creatures over the analytic world, against the reference): mean difference **≤ 0.5/255**, **≤ 0.5%** of pixels off by more than 8/255, step-cap hits **≤ 0.1%** of creature pixels. Spike 02's thresholds.
   - **Brick cache** (bricks against the analytic world, everything else equal): mean **≤ 1.0/255** and **≤ 2%** of pixels off by more than 8/255. The cache is a lossy representation (12.5 cm voxels) by design, so the threshold is looser than for an exact method. It's a judgement made now, before measuring.
   - A fast number with a wrong image doesn't count.
   - *Added during development, before the final run:* the moving things' hit tolerance is a quality knob, as in spike 02. The creature-time verdict uses the cheapest measured tolerance (¼ or ⅒ of a pixel) that passes this criterion.
4. **Looks.** Screenshots of every scene are judged by eye, honestly. They don't need to be beautiful for creatures (the brief says so), but the world should be.

**Fallback if creatures fail:** rasterize moving things (spike 01's path: extracted meshes, skinned) and composite them with the marched world by depth.
**Fallback if edits fail:** budget the re-cook (at most *B* bricks per frame, finishing an edit over several frames), or evaluate edits analytically from a spatial list instead of baking them.

## Sweeps

- **Instance count:** 50, 150, 300, 600, 1,000 in the `square` view.
- **Edits per second:** 0, 5, 10, 20 and 40 (to find where it breaks), 300 instances.
- **Screen coverage:** three cameras over the same crowd: `aerial` (far), `square` (the main view), `street` (eye height, in the crowd).
- **Resolution:** every count and coverage point at 1920×1080 and at a 960×540 internal resolution. No upscaler is implemented, so no upscaled quality is claimed.

## Method

### The static world: a sparse brick cache

**The world is marched from a cache of distance-field bricks.**
- **The reason given before measuring was the edits:** the base world (terrain, tower, wall, houses) was assumed cheap enough to evaluate analytically at this scale. But every edit adds a term, and the edit log grows without bound (D-015: field edits are sim state, stored as an edit log; D-029 names destruction and edit logs as the case for interpreted evaluation).
- A cache pays for an edit once, when it's cooked. After that the march costs the same whatever the log's length.
- The spike also times the analytic march (`world_analytic`) on the same content, to test that reasoning. **The results find a different reason** (see "The static world" below).

**Layout:**
- **Region:** 96 × 40 × 96 m around the square, in 1 m cells.
- **Brick map:** one `u32` per cell. A cell near a surface (|distance at its centre| < 1.0 m, which covers the cell's diagonal) holds a brick slot. Any other cell holds its centre distance as a half float, clamped to 4 m.
- **Bricks:** 8³ voxels of 12.5 cm, stored as 9³ corner samples. Shared borders are duplicated, so trilinear filtering never reads a neighbour. They live in one `rg16float` 3D atlas (288³, 32,768 slots): distance, and a "disturbed" channel that edits write (earth or broken stone) for shading.
- **Beyond the region:** the terrain continues as an analytic heightfield, traced with spike 02's directional Lipschitz bound.

**The march:**
- **Through an empty cell,** the step is the larger of the distance to the cell's exit and the cell's centre distance minus the distance to the centre. Both are safe.
- **In a brick cell,** it's sphere tracing on the hardware-filtered trilinear field, at 0.9 of the sampled distance. That the trilinear field's slope stays below 1/0.9 is a hypothesis; the reference comparison checks it.
- **A converged hit is placed with an exact f32 trilinear** (eight `textureLoad`s and a secant). Hardware filtering quantizes its weights to about 1/256 of a voxel, and at grazing angles that moved hits by millimetres along the plaza. It nearly doubled the cache's error in the street view (below).
- **Normals and AO:**
  - Shading normals come from the bricks: four taps 3 cm apart.
  - Short-range AO takes three taps along the normal.
  - Surface detail (stone courses, cobbles, tiles, timber, windows) is procedural in shading and filtered by the pixel footprint, as spikes 01 and 02 did with displacement.
- **World shadows:** a soft sun shadow ray through the cache (Quílez's closest-approach estimate in brick cells, the conservative centre bound in empty cells).

### Edits: re-cook only the affected bricks

- **An edit** is a CSG primitive appended to the log: a smooth subtraction of a sphere (crater, blast, dig) or a smooth union of a squashed one (rubble).
- **One edit event** is one or more primitives:
  - a crater: one sphere
  - a blast: one hole in the wall, plus three rubble lumps
  - a dig: one sphere on a trench line
- **CPU per event:**
  - Append the primitives.
  - Work out the box of cells they can reach.
  - Pick out the log entries that overlap that box. They're uploaded in log order, since smooth CSG doesn't commute.
- **GPU per event, in that frame,** in two timed passes, with nothing read back:
  1. **`classify`:** one thread per cell of the box. It first checks what the cell can be affected by:
     - **Cells an edit can reach the surface of:** evaluates base + overlapping entries at the centre, then allocates a brick from a free stack, keeps it, or releases it, and appends brick cells to a cook list.
     - **Cells only within an addition's 4 m empty-distance reach:** only updates an empty cell's distance, and leaves a brick cell alone (its samples are out of reach).
  2. **`cook`:**
     - an indirect dispatch, one 9×9×3 workgroup per listed brick
     - each workgroup first filters the overlapping entries down to those that can reach its brick (in workgroup memory, sorted back into log order), then writes 729 samples
     - released slots go back on the stack afterwards, so none is reused within a batch
- **The source of truth is always base + log,** so the cache can't drift. The run checks this: after the sweep, the incrementally maintained cache is compared with one cooked from scratch from the same log.
- **The full cook** of the region is the same two passes over every cell, in 144 chunks (one submit each).

### Moving things: posed on the GPU, binned per tile, composited

- **Content:**
  - **Villagers:** bipeds, 12–14 parts (robes and hats by seed).
  - **Farm animals:** sheep, pigs, cows, horses and dogs, 12 parts.
  - **Birds:** circling and banking overhead, flapping or gliding, 8 parts.
  - All are varied by seed: 57% villagers, 33% animals, 10% birds.
  - Every part is a round cone, rigid in world space after posing, smooth-unioned per instance (spike 02's rule: rigid parts keep the distance bound).
- **CPU per frame:**
  - each instance's root state: position, heading, gait phase, and bank and glide for birds
  - 32 bytes per instance
  - closed-form circles here; in a game this is the sim's output
- **`pose` (compute):**
  - one thread per instance
  - builds the skeleton from the gait phase and writes the posed parts and a bound sphere
  - The same posing is written in JS (`poseJS`), to time it on the CPU and to check the GPU's output against.
- **`cull` (compute):**
  - one thread per instance
  - frustum-culls the bound sphere and projects its view-space box to a conservative screen rectangle
  - appends the instance to every overlapped coarse tile (64×64 px), and to the coarse cells of the sun's light grid
- **`bin_screen` and `bin_light` (compute):**
  - spike 02's binning, made hierarchical so it doesn't cost tiles × instances
  - one workgroup per coarse tile loads that tile's list into shared memory
  - each thread then bins one 8×8 tile: sphere test, per-part capsule test, near-to-far insertion keeping the nearest 24
  - The light grid (128², ~0.94 m cells, 16² coarse, 16 entries per cell) works the same way in the sun's orthographic view.
- **`trace` (compute), per pixel:**
  1. the static world (bricks)
  2. the tile's instances: bound-sphere test, exact per-part ray intervals giving a per-ray part mask, then a march of the masked field, bounded by the nearest hit so far (spike 02)
  3. shading of whichever is nearer
  4. one sun shadow ray (a single call site, so it's inlined once), through the brick cache and through the light-grid cell's instances

  Moving things cast shadows on the world and on each other, and the world shadows them. A creature shadow's penumbra counts only within 10 cm of the occluder's surface. That keeps binning (capsules inflated by 10 cm) and the reference (bound spheres inflated by 10 cm) seeing the same shadow, and it only limits casters more than ~3 m from what they shade (birds).

### Correctness

- **The reference `ref`:**
  - every instance with every part (no binning, no masks)
  - the analytic world with the whole log (no bricks)
  - half-size steps, a twentieth of the hit tolerance, step caps of 1,500–3,000
- **Its world goes through a spatial edit list:**
  - Each 2 m cell lists the log entries that can change the field anywhere it's evaluated in that cell: outside a surface, or up to 0.5 m inside it for normals and AO.
  - The field is clamped to 2 m, so a distant addition that's skipped can't make a step unsafe. At 2 m the clamp can't change a penumbra either: shadow rays leave the 32 m-high region before `SOFT · 2 / t` falls below 1.
  - This keeps the reference exact while bounding each pixel's cost. The first version looped over every edit, and single 8×240-pixel tiles took 240 ms with ~600 edits in the log.
- **Pairs reported (1080p, t = 4 s):**
  - `crowd_check` vs `ref`: **the creature path**. `crowd_check` is fast creatures (binning, masks, tolerance) over the reference's own world, so only the creature path differs. The ⅒-pixel tolerance is `crowd_check_eps10`.
  - `full` vs `full_analytic`: **the brick cache**, at equal tolerances.
  - `full` vs `ref`: everything.
- **Metrics:** mean |difference| out of 255, the share of pixels off by more than 8/255, and step-cap hits for every march type.
- **Also checked:**
  - **Posing:** the GPU's posed parts are read back and compared with `poseJS` for all 1,000 instances.
  - **Incremental re-cooking:** checked against a from-scratch cook of the same log.

### GPU safety

No single submit runs longer than ~100 ms (`spikes/README.md`):
- **Heavy variants** (the analytic world, the reference, their stats) trace in tiles, one submit each, awaited. A tile shrinks to 8×120 px or grows, so each takes about 25 ms. The longest submit in the final run was **46 ms**.
- **Full cooks** run in 144 chunks; the longest took 14 ms.
- **Pipelines** are created one at a time, and trace variants on first use.

## Layout

| File | What |
|---|---|
| `crowd.js` | The crowd: per-instance parameters by seed, the root-state "sim", and `poseJS`, the JS copy of `pose` |
| `edits.js` | The deterministic edit schedule (craters, blasts, digs), each event's box and overlap list, the analytic world's edit lists |
| `common.wgsl` | Frame uniform, constants, layouts, terrain height, noise |
| `world.wgsl` | The analytic world: terrain, tower, wall, houses; edits; materials |
| `cache.wgsl` | Brick map and atlas access, the brick march, exact hit placement, world shadows |
| `creature.wgsl` | Round cones, the masked field, bounds and ray intervals |
| `pose.wgsl` | `pose` |
| `bin.wgsl` | `cull`, `bin_screen`, `bin_light` |
| `cook.wgsl` | `begin_batch`, `classify`, `cook`, `release` |
| `trace.wgsl` | The frame: world, crowd, shadows, shading, sky; the analytic world with its edit lists |
| `main.js` | Harness: pipelines, scenes, sweeps, timing, the reference comparison, the summary |
| `results/` | `run-*.json` (with a computed `summary` of the criteria), `console.log`, JPEG copies of key screenshots (PNGs are ignored) |

## Running it

```bash
python3 spikes/serve.py 8417
```

```bash
spikes/headless.sh 06-crowds '#run' 900
```

- **`#run`** measures everything in about 75 s of page time (it waits for the GPU lock first). It writes `results/run-<timestamp>.json`, whose `summary` block evaluates the kill criteria, plus PNG screenshots, then `results/DONE`.
- **`#quick`** renders each scene once and saves screenshots: a few seconds.
- **Options for development:**
  - `#quick-quality` adds the reference comparison
  - `#quick-time` adds a few timings
  - `#quick-stress` grows the log to ~560 entries and re-checks

## Results (2026-10-01, indicative)

**Setup:**
- MacBook Air M4 (8-core GPU), headless Chrome 154 via `spikes/headless.sh`, `#run`.
- The final run is `results/run-2026-10-02T01-39-12-299Z.json`.
- **These timings are indicative, not final.** The run held the GPU lock, so no other spike's page ran during it. But other spikes were being built on the same machine, and the run wasn't on a quiet machine. Earlier runs, taken while other spikes' pages shared the GPU, were up to 5× slower, and erratically so; none of those numbers are used here.
- The owner's final measurement on a quiet machine supersedes every timing below. Correctness results don't depend on load.
- The main decomposition (`square`, 300, 1080p) is three interleaved rounds of 90 frames; the rounds agree to the 65.5 µs timestamp quantum.

### Verdict (indicative)

| Kill criterion | Measured | Verdict |
|---|---|---|
| 1. Creatures ≤ 2.5 ms (300, `square`, 1080p) | **1.84 ms** at the ⅒-px tolerance that passes criterion 3 (1.71 ms at ¼ px, which narrowly fails it) | **Pass** |
| 2. Edits at 20/s: ≤ 0.3 ms amortized, ≤ 1.0 ms worst frame | **0.12 ms** amortized, **0.72 ms** worst (a blast) | **Pass** |
| 3a. Creature path ≤ 0.5/255 mean, ≤ 0.5% over 8/255, ≤ 0.1% caps | ⅒ px: mean ≤ 0.12, **0.03–0.19%** over 8 in all four views, caps ≤ 0.035%. ¼ px: 0.55% in `square` | **Pass** at ⅒ px; ¼ px fails narrowly |
| 3b. Brick cache ≤ 1.0/255 mean, ≤ 2% over 8/255 | `aerial` 0.41%, `square` 1.34%, `street` 1.67%; **`wall` close-up 2.58%** (mean 0.997), 2.80% after the sweep | **Pass** at distance; **fail** in the close-up of blasted masonry |
| 4. Looks | See "How it looks" | The world is pleasant; the creatures are mannequins |

**Context, not judged (spike 04's and 05's questions):**
- **The world isn't close to its 3.0 ms slice in this view:** 7.41 ms for primary visibility and shading, plus 1.64 ms for its shadows. Re-cooking adds 0.12 ms amortized.
- **The whole frame:** 10.8 ms back to back.
- **Paced at 60 Hz** (busy-wait), 0 of 150 frames went over 16.7 ms, with and without 20 edits/s. As in spikes 01 and 02, the OS lowered clocks: the median paced frame was 14.8 ms against 10.8 back to back.

### Where creature time goes

`square` view, 1080p, ¼-px tolerance, GPU ms. Pass times marked * come from 20 back-to-back dispatches, divided; in-frame, each is under one timestamp quantum.

| Instances | Coverage | pose* | cull* | bin_screen* | bin_light* | Visibility | Shadows | **Creatures** | Frame | Creatures at 540p |
|---|---|---|---|---|---|---|---|---|---|---|
| 50 | 1.7% | 0.007 | 0.029 | 0.026 | 0.020 | 0.13 | 0.33 | **0.46** | 9.63 | 0.13 |
| 150 | 4.3% | 0.003 | 0.029 | 0.049 | 0.043 | 0.20 | 0.72 | **1.05** | 10.09 | 0.39 |
| 300 | 7.0% | 0.003 | 0.033 | 0.072 | 0.066 | 0.33 | 1.18 | **1.64** | 10.81 | 0.66 |
| 600 | 11.2% | 0.007 | 0.029 | 0.138 | 0.128 | 0.52 | 2.03 | **2.82** | 11.99 | 1.18 |
| 1,000 | 15.1% | 0.007 | 0.029 | 0.203 | 0.223 | 0.72 | 2.88 | **4.06** | 13.17 | 1.83 |

- **"Visibility"** is trace without creature shadows minus trace without creatures. **"Shadows"** is full trace minus trace without creature shadows. "Creatures" uses the in-frame pass medians, per the criterion. The 300 row here (1.64 ms) and the three-round main measurement (1.71 ms) differ by one quantum.
- **Shadows are about 70% of creature time at every count.** That repeats spike 02's finding that marched shadows cost more than visibility. Their arithmetic is small (1.2M shadow steps at 300, about 5.5 per march), so the cost is probably divergence and latency inside one large kernel. That's a hypothesis; a separate, compacted shadow pass is the obvious next experiment (untested).
- **Posing and culling cost almost nothing on the GPU:** 3–7 µs and ~30 µs at every count. Binning grows linearly but stays small: 0.43 ms for both grids at 1,000.
- **The limits:**
  - **At 1080p,** creatures cross 2.5 ms between 300 and 600 instances (≈520 by linear interpolation, an estimate) and reach 4.06 ms at 1,000. That's inconclusive, not failed. 5.0 ms would be at ≈1,300 instances, extrapolating 3.1 µs per instance (an estimate).
  - **At 540p,** 1,000 instances cost 1.83 ms.
- **Lists:** one light-cell entry overflowed at 1,000 (a 16-entry cell); no other list overflowed. Steps per creature pixel rose from 3.9 to 5.5 with the count, at 1.8–1.9 parts per evaluation.

### Coverage

300 instances, GPU ms:

| View | Coverage | Creatures, 1080p, ¼ px | ⅒ px | of which shadows | Creatures, 540p |
|---|---|---|---|---|---|
| `aerial` | 1.2% | 0.72 | — | 0.39 | 0.33 |
| `square` | 7.0% | 1.71 | 1.84 | 1.25 | 0.66 |
| `street` | 24.7% | 2.03 | 2.23 | 1.70 | 0.79 |

Cost grows with coverage, sublinearly here: 20× the coverage costs 2.8×. In the street view, a quarter of the screen is creatures for about 2 ms. These creatures are far simpler than spike 02's grazer, whose 63% fill cost 5.5 ms, so the two aren't comparable.

### Edits

`square`, 300 instances, 1080p, 120 frames per rate, GPU ms:

| Edits/s | Events | Re-cook median | Worst frame | **Amortized** | Frame median / p95 |
|---|---|---|---|---|---|
| 0 | 0 | — | — | 0 | 10.81 / 10.88 |
| 5 | 10 | 0.26 | 0.52 | **0.02** | 10.81 / 11.01 |
| 10 | 20 | 0.26 | 0.52 | **0.04** | 10.81 / 11.21 |
| 20 | 40 | 0.39 | 0.72 | **0.12** | 10.88 / 11.40 |
| 40 | 80 | 0.39 | 1.11 | **0.32** | 11.14 / 11.86 |

- **By kind, at 20/s** (median / worst): blast 0.52 / 0.72 ms, crater 0.26 / 0.46, dig 0.13 / 0.20.
- **At 40/s it's inconclusive:** 0.32 ms amortized against the 0.3 ms sub-slice, and the worst frame is 1.11 ms.
- **Per event, from the 60 events applied before measuring:**

  | Kind | Primitives | Cells classified | Bricks cooked | Log entries overlapping |
  |---|---|---|---|---|
  | crater | 1 | 950 | 251 | 10 |
  | dig | 1 | 307 | 113 | 10 |
  | blast | 4 | 964 | 290 | 50 |

  The CPU's share per event is under 0.1 ms. `performance.now()` is coarsened to 0.1 ms here, so it can't resolve less.
- **`cook` is nearly all of it;** `classify` is under one quantum. A first version cost 3.6 ms per blast, because it re-cooked every brick within a rubble lump's 4 m empty-distance reach and evaluated every overlapping entry at every sample. Splitting those two cases in `classify`, and filtering entries per brick in `cook`, cut it roughly 6–8× (both figures were measured with other spikes on the GPU, so the ratio is indicative).
- **The cache stays exact:** after the sweep (598 log entries), the incrementally maintained cache and one cooked from scratch give **identical images** (0 pixels differ, in `wall` and `square`).
- **Allocation:** bricks went from 26,320 (empty log) to 26,677 (598 entries) of 32,768 slots, with no allocation failures.
- **A full cook** of the region takes 1.6 ms of classify plus 19 ms of cook, in 144 chunks; it's the same with 598 entries in the log.

### The static world: bricks against analytic

| `square`, no creatures or shadows | 1080p | 540p | Steps per pixel |
|---|---|---|---|
| Bricks: march + shading | **7.41** | 1.97 | 24.4 in the region (7.4 in bricks, 17.0 crossing empty cells) + 5.5 of far heightfield |
| Bricks: march and normals only (no materials, AO or detail) | 5.18 | 1.38 | |
| Analytic, with per-cell edit lists, 126 entries | 54.6 | 13.8 | 33.4 + 5.5 |
| Analytic, same, no edits | 53.5 | 13.6 | |

- **The cache is needed at this scale, but not mainly for the reason stated before measuring.** With a spatial edit list, the analytic world barely notices the edits: 53.5 → 54.6 ms from 0 to 126 entries, and 57.9 ms with 563 in a development run. **It's 7× the cache even with no edits.** Each analytic step evaluates the terrain (twelve trig calls, normalized by its gradient) and every nearby structure; each cached step is a map read and one texture sample. That's the cost the cache removes.
- The analytic tracer here is naive: plain sphere tracing, no heightfield bound inside the region, coarse structure bounds. A better one would narrow the gap. That it would close it is untested, and seems unlikely at this ratio (a hypothesis).
- **Shading is 2.2 ms** (materials, AO, procedural detail). The world's shadows are 1.64 ms.
- **The march's cost is crossing empty cells** (17 steps per pixel, cell by cell within 4 m of the ground) and the horizon band (`results/square-heat_world.jpg`). A coarser skip level, or a larger empty-distance clamp, would cut it. Either is a trade against add-edits' update boxes; untested.
- **Memory:** the atlas is 91 MB (288³ × `rg16float`), the brick map 1.4 MB. An 8-bit distance, normalized to the band, would quarter the atlas; untested here. Posed instances take 0.55 MB per 1,000, the lists 10.6 MB. No meshes.

### Posing: CPU against GPU

| Instances | Root state (CPU) | Upload | Posing in JS (CPU) | `pose` on the GPU |
|---|---|---|---|---|
| 50 | 0.002 ms | 1.6 KB | 0.028 ms | 0.007 ms |
| 300 | 0.012 ms | 9.6 KB | 0.164 ms | 0.003 ms |
| 1,000 | 0.038 ms | 32 KB | 0.562 ms | 0.007 ms |

- **The GPU poses 1,000 instances in about 7 µs.** What stays on the CPU (the root state) is 38 µs. The CPU figures are batches of 50 frames, divided.
- **The GPU's output matches `poseJS`** to 3.8 µm for all 1,000 instances, with identical headers.
- **Posing in JS is cheaper here than spike 02 suggested** (0.6 ms for 40 there; 0.56 ms for 1,000 here). These creatures are simpler: 8–14 cones with closed-form limbs, against spike 02's 28 parts posed through 29 bone matrices. So the GPU's advantage here is real but small in absolute terms. For spike 02's grazer it would be larger, but that's an estimate, not a measurement.

### Correctness

1080p, t = 4 s, 126 log entries (598 after the sweep). Mean |difference| / share of pixels over 8/255:

| View | Creature path, ¼ px | Creature path, ⅒ px | Brick cache | Everything (`full` vs `ref`) |
|---|---|---|---|---|
| `square` | 0.26 / **0.55%** | 0.10 / 0.19% | 0.44 / 1.34% | 0.69 / 1.87% |
| `street` | 0.30 / 0.50% | 0.12 / 0.17% | 0.60 / 1.67% | 0.88 / 2.14% |
| `aerial` | 0.09 / 0.27% | 0.04 / 0.11% | 0.14 / 0.41% | 0.22 / 0.67% |
| `wall` | 0.05 / 0.07% | 0.02 / 0.03% | 1.00 / **2.58%** | 0.99 / 2.64% |
| `wall`, after the sweep | | | 1.05 / **2.80%** | 1.02 / 2.84% |

- **Step caps, fast path:**
  - **Creature primary marches:** 0–0.003% of creature pixels at ¼ px, ≤ 0.035% at ⅒ px.
  - **World primary:** 0–0.12% of pixels. The `street` view's distant plaza is grazing; a capped ray is shaded where it stopped.
  - **World shadow rays:** ≤ 0.09% of shadow rays.
  - **Creature shadow marches:** 0.15–0.21% of creature shadow marches. A shadow ray skimming along a body takes 2 mm steps; these aren't judged by the criterion.
- **Step caps, reference:** at most 10 pixels per view (≤ 0.0005%).
- **The creature path's errors** are thin silhouette rims, where a ¼-px tolerance grows them; the soft self-shadow terminator; and penumbra edges (`results/street-diff-crowd.jpg`). At ⅒ px they pass everywhere, for +0.13 ms.
- **The brick cache's errors are all at creases and thin features,** which 12.5 cm voxels round off (`results/wall-diff-cache.jpg`, `results/street-diff-cache.jpg`):
  - blast-hole rims and crater lips
  - merlon edges and the wall's foot
  - the tower's corbel ring and slit windows
  - eave soffits
  - The shadows those features cast move with them.
  
  At mid distance this stays under 2%. In the close-up of blasted masonry it doesn't, and more blasting adds more creases. Making the normal taps 3 cm instead of 6 cm and placing hits exactly took the `street` view from 2.96% to 1.67%, and the `wall` from 2.89% to 2.58%. The rest is resolution.

### How it looks

- **The world is pleasant** (`results/square.jpg`, `street.jpg`, `aerial.jpg`, `wall-full.jpg`):
  - **What works:** warm sun, soft shadows, a cloudy sky with distance haze, half-timbered houses with tiled roofs and windows, coursed stone on the tower and wall, and cobbles.
  - **Blasting reads well:** the wall breaks open with ragged rims and rubble at its foot (`results/wall-after-sweep-full.jpg`), and crater fields read as churned earth.
- **Its flaws:**
  - The rolling meadow is plain: green with noise, and no vegetation (spike 03's question).
  - At distance the plaza is a flat grey-brown, because the cobbles fade out with the footprint.
  - Rubble is smooth pale lumps.
  - Blasting leaves wall fragments floating in the air: CSG with no structural collapse.
- **The creatures are mannequins,** as the brief allowed:
  - smooth capsule people in flat colours, pigs as pink lozenges, birds as stick crosses
  - Their motion is legible, and their shadows on the cobbles and on each other read well.
  - Nothing is anti-aliased.
- **At 540p** the procedural detail correctly fades with the larger footprint (`results/square-540p.jpg`), so houses lose their timbering. No upscaler is implemented.

### Surprises

1. **Shadows, not visibility, are the creature cost:** about 70% at every count. Visibility for 300 instances is 0.33 ms.
2. **Posing doesn't matter at this complexity,** on either processor: microseconds on the GPU, about half a millisecond for 1,000 in JS.
3. **The cache's case is per-step cost, not edit count.** With spatial edit lists, analytic evaluation is flat in edits but 7× the bricks.
4. **Hardware trilinear filtering's weight precision was visible.** At grazing angles the ~1/256-voxel weight quantization moved hits by millimetres and shifted cobble edges, nearly doubling the cache's measured error in the street view until hits were placed with an exact f32 trilinear.
5. **Edits are cheap once the cook is selective,** and an incremental cache maintained over 598 entries matched a from-scratch cook bit for bit. The naive version was several times dearer (3.6 ms per blast) and would have hitched.
6. **The reference was the GPU hazard,** not the fast path. Per-pixel cost (thousands of half-steps × hundreds of edits), not pixel count, made 8×240-pixel tiles take 240 ms, until the reference got spatial edit lists too.

### Caveats

- **Timings are indicative** (see Setup). Timestamps are quantized to 65.5 µs, so in-frame passes under ~0.2 ms are imprecise; the repeated micro-benchmark covers `pose`, `cull` and the bins.
- **Creature time is a difference of two trace passes,** as in spike 02, and charges creatures for shadow tests from world pixels near them.
- **The analytic-world timings are tiled** (each tile awaited), not back to back, so clocks may sit lower. That would overstate them a little; the 7× ratio is an estimate of that order.
- **The crowd isn't simulated:** closed-form circles, no avoidance, so people pass through each other at high counts. Walkers stand on the base terrain, not on edited ground; they're kept away from the craters and trench.
- **Not included:** anti-aliasing, temporal reuse, LOD for distant creatures, any skinning or deformation (rigid parts only, as spike 02 found ray marching requires), and secondary devices or other browsers.
- **Changes made during development, before the final run** (the criteria didn't change except the noted tolerance rule):
  1. **Single call sites.** Creature shadows were inlined at two call sites; one call site lowered creature shadow cost (≈1.9 → 1.6 ms, measured with other spikes on the GPU, so indicative).
  2. **Tighter penumbra.** The creature penumbra limit went from 20 to 10 cm, after the heat map showed shadow cost concentrated in penumbra halos.
  3. **Selective re-cooking.** Re-cooking was made selective (above), after the first blasts cost 3.6 ms.
  4. **Cache precision.** World hits got secant refinement, then exact trilinear placement, and brick normals went to 3 cm taps. These cut the cache's measured error, as recorded above.
  5. **The roof's fascia.** House roofs got a 25 cm fascia instead of a knife edge at the eaves, because a knife edge is thinner than a voxel. It's a content change, made for the cache; real roofs have one.
  6. **Isolating the comparisons.** World and creature tolerances were split into separate knobs, and `crowd_check` was added, so that "creature path" measures only the creature path. The first comparison mixed in the analytic world's own tolerance.
  7. **GPU safety (after the 2026-10-01 display reset).**
     - Heavy renders and full cooks became tiled.
     - Pipelines are created one at a time.
     - The reference's world got per-cell edit lists and a 2 m clamp, and its step caps were halved (3,000 world, 2,000 creature).
     - The reference's images didn't change: the cache comparison was 2.797% before and 2.799% after, in a 563-entry stress run.

### What this means for the design

Nothing here is recorded in `decisions.md`; that's the owner's call.
- **Hundreds of moving things fit in a ray-marched world**, if the creatures are rigid-part fields as spike 02 found they must be.
  - 300 cost about 1.8 ms at 1080p and 0.7 ms at 540p (indicative).
  - The budget breaks between 300 and 600 at native 1080p.
  - Rasterizing them (the fallback) isn't triggered.
- **Their shadows are where to spend optimization effort,** not posing or binning. Untested options:
  - a compacted shadow pass
  - a creature shadow map (spike 01's 0.79 ms for 40 grazers)
  - shadow LOD with distance
- **Posing belongs on the GPU,** or anywhere: it's microseconds. The CPU's share is the sim's root state, 32 bytes per instance.
- **Live edits fit comfortably** with a GPU-cooked brick cache: 0.12 ms amortized at 20 events per second, under 1 ms worst. The cook must be selective.
- **The cache costs fidelity at creases,** at 12.5 cm voxels. It's fine at mid distance, but over budget for a close-up of blasted masonry. Finer bricks near the camera would be the fix, with a memory cost (4× per halving of voxel size; estimate). So would refining the final hit against the analytic field with its local edit list. Both are untested.
- **Edits as a log of CSG primitives re-cooked from base + log** (D-015) worked as specified here, with exact results. It does assume the log stays spatially indexed: per-box overlap lists on the CPU, and per-cell lists for analytic evaluation.
- **Nothing here is engine-specific in the language sense (D-050).** Rigid parts, bounds, brick caches, CSG logs and spatial lists make sense in any program that traces fields.

## Quiet-machine rerun (2026-10-02)

Run file: `results/run-2026-10-02T03-54-01-646Z.json` (serial, after a cool-down, headless Chrome 154, same M4). GPU ms at 1080p, `square`, 300 instances unless noted; quiet values are the JSON's, rounded.

| Metric | README value | Quiet value |
|---|---|---|
| 1. Creatures, ⅒ px (the judged tolerance) | 1.84 | 1.84 (1.836) |
| 1. Creatures, ¼ px / of which shadows | 1.71 / 1.25 | 1.64 / 1.18 |
| 2. Edits at 20/s: amortized / worst frame | 0.12 / 0.72 | 0.12 / 0.72 (0.118 / 0.721) |
| Static world, bricks: march + shading / its shadows | 7.41 / 1.64 | 7.41 / 1.64 |
| Static world, analytic: 126 entries / no edits (tiled) | 54.6 / 53.5 | 54.9 / 53.8 |
| Whole frame: back to back / paced median; paced frames over 16.7 ms, with and without 20 edits/s | 10.8 / 14.8; 0 of 150 each | 10.75 (10.81 in the count sweep) / 14.81; 0 of 150 each |
| 3a. Creature path, ⅒ px: worst mean / over 8/255 in `square`, `street`, `aerial`, `wall` | 0.12 / 0.19, 0.17, 0.11, 0.03% | 0.12 / 0.19, 0.17, 0.11, 0.03% |
| 3a. Creature path, ¼ px, `square`, over 8/255 / worst ⅒-px step caps | 0.55% / ≤ 0.035% | 0.55% / 0.035% |
| 3b. Brick cache, over 8/255: `aerial`, `square`, `street` | 0.41, 1.34, 1.67% | 0.41, 1.34, 1.67% |
| 3b. Brick cache, `wall`: over 8/255 (mean) / after the sweep | 2.58% (0.997) / 2.80% | 2.58% (0.997) / 2.80% |

**No verdict changes** under the README's own criteria: 1 and 2 pass; 3a passes at ⅒ px and ¼ px still fails narrowly (`square`, 0.55%); 3b passes at distance and still fails in the `wall` close-up. Criterion 4 (looks) is judged by eye and isn't in the JSON.
