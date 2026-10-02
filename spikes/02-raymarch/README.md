# Spike 02: the grazer, ray-marched

*A second measurement spike for D-067's question, by another route. It's throwaway code, not the start of the engine.*

## The question

Spike 01 realizes each grazer as triangles (dual contouring), skins them, and evaluates the field per pixel for shading. **Can the same scene be drawn with no triangles at all,** by sphere-tracing the posed field directly in one hand-written compute kernel? If it can, what does it cost and what does it buy, compared with spike 01 on the same machine?

Same scene as spike 01: the same 40 individuals (`makeGrazer(1..40)`), herd layout, cameras, walk cycle, sun, sky and tonemapping, at 1920×1080. The grazer's numbers come from spike 01's `grazer.js`, imported unchanged.

## Kill criteria, written before measuring

Creature GPU time here is the *marginal* cost of creatures:

> creatures = `bin_screen` + `bin_light` + (`trace` with creatures − `trace` with terrain only)

It covers primary visibility, shading and marched shadows on everything (terrain included). Spike 01's figure covers its shadow map, prepass and shading passes. Neither includes the terrain itself.

1. **Viable at all (D-067's criterion, same margin rule as spike 01).** Herd of 40 at 1080p on this M4:
   - **≤ 4 ms:** provisional pass for the M1's 8 ms budget.
   - **4–8 ms:** inconclusive until measured on an M1.
   - **> 8 ms:** fail.
2. **Competitive with spike 01.** Against spike 01's best recorded numbers on this machine (herd with 3cm LOD: 2.49 ms creatures, 3.67 ms whole frame; close-up: 1.05 ms creatures):
   - **Better:** lower creature time.
   - **Competitive:** within 1.5×.
   - **Worse:** more than 1.5×.
3. **Correct, not just fast.** The fast kernel's image must match a brute-force reference march of the same field (plain sphere tracing, half-size steps, one tenth of the hit tolerance, step caps of thousands): mean difference **≤ 0.5/255**, and **≤ 0.5%** of pixels off by more than 8/255. Rays that end by hitting the step cap must be **≤ 0.1%** of creature pixels. A fast number that fails this doesn't count.

Questions to answer either way, with numbers where possible:
- How cost scales with screen coverage (a close-up that fills the screen) and with instance count (a herd of 160), which spike 01 can't run without new extraction work.
- What ray marching requires of the *content and language*: how the field has to be posed, which declared facts the kernel leans on.

## The approach

**No triangles, no extraction, no meshes.** Each frame:

1. **Pose (CPU, JS).** Spike 01's walk cycle gives the bone palette. Instead of skinning a mesh, the parts are *moved*: each part is rigid in one bone's frame. Round cones (legs, neck, tail, muzzle) get world-space endpoints; ellipsoids (torso, head) get the inverse bone transform, so they and their noise are evaluated in rest space. A rigid transform preserves distance, so the posed field is still a distance bound.
   - Spike 01 bends the neck and tail by skinning one cone across several bones. A field can't be skinned that way, so here they're chains of rigid cones, one per bone: 4 for the neck and 6 for the tail. That's 28 parts instead of 20.
2. **`bin_screen` (compute).** One invocation per 8×8-pixel tile. It culls instances against the tile's frustum, then each surviving instance's parts, and writes the tile a near-to-far list of (instance, `PartMask`).
3. **`bin_light` (compute).** The same over a 128×128 grid in the sun's orthographic view. A shadow ray is parallel to the sun, so it stays in one cell: a shadow ray's candidates are its cell's list.
4. **`trace` (compute).** Per pixel:
   - **Terrain:** an analytic heightfield, traced with a *directional* Lipschitz bound (the step uses how fast the ray can approach the terrain, not the 3D distance), which takes far fewer steps at grazing angles than plain sphere tracing.
   - **Creatures:** for each tile entry, exact ray intervals against every candidate part's bound (capsules for cones, ellipsoids in rest space), inflated so that leaving a part out is *exact* along the ray (see below). That gives a per-ray `PartMask` (usually 1–4 parts) and a start and end distance. The march starts at the first bound and stops at the last bound, the terrain, or a nearer hit.
   - **Displacement goes to the shading normal, as in spike 01.** Spike 01's mesh doesn't carry the torso's 3mm fbm displacement either; only its normals do. So the main configuration marches the undisplaced surface.
   - **The `displaced` variant traces true displaced silhouettes,** in two phases. The displacement is bounded by its amplitude sum, so the march runs on the base field minus that bound, and switches to the full displaced field within a few millimetres of the surface. The footprint filters the octaves (D-077), and the step shrinks by the displacement's estimated Lipschitz constant.
   - **The hit tolerance is a quarter of a pixel's footprint,** so distant creatures converge in fewer steps. After a hit, one more step moves the point onto the surface at no extra cost, and the terrain uses a secant step.
   - **Optional over-relaxation** (Keinert et al. 2014), with fallback when the unbounding spheres don't overlap.
   - **Shading:** the same lighting as spike 01. The normal comes from the forward-mode gradient at the hit; the dapples and hoof channel are the same.
   - **Shadows:** a marched soft shadow ray toward the sun, for terrain and creature pixels that face it, through the light grid. The penumbra uses Quílez's closest-approach estimate.

### Why leaving parts out is exact

The creature is a polynomial smooth union with blend radius `k`. If a part's value `b` is at least `k` at every point a ray visits, then along that ray the union with and without the part has the same zero crossings, and equals the union without it wherever that union is ≤ 0. So a part can be dropped from a ray's mask when the ray doesn't enter the part's bound inflated by `k` (plus the part's own inner blend and displacement margins). The bound inflations:
- **Round cone:** the capsule with radius `max(r1, r2) + k` contains the region where the cone's distance is below `k`.
- **Ellipsoid bound** (the stdlib's `k0(k0−1)/k1`): along any ray from the centre it equals `(s−1)/|x/r²|` at scale `s`, and `|x/r²| ≤ 1/min(r)`, so scaling the radii by `1 + m/min(r)` contains the region where it's below `m`.

## Layout

| File | What |
|---|---|
| `pose.js` | Rigid posing of the parts, the bounds, and the per-instance buffer layout |
| `common.wgsl` | The frame uniform, the instance buffer and its layout |
| `field.wgsl` | The posed field: primal, gradient, part bounds and ray intervals |
| `bin.wgsl` | `bin_screen` and `bin_light` |
| `trace.wgsl` | Terrain, creature march, shadows and shading |
| `main.js` | Harness: pipelines, scenes, timing, the reference comparison |
| `results/` | Raw JSON from each run, and screenshots |

## Running it

```bash
python3 spikes/serve.py 8417
```

Open <http://localhost:8417/02-raymarch/>, then press **Run all**. Results appear on the page and in `window.__results`, and are saved to `results/`. `#quick` draws one frame.

## Results (2026-10-01)

**Setup:**
- The same MacBook Air M4 (8-core GPU, 16 GB) and Chromium 152 (the Claude desktop app's browser pane) as spike 01.
- **Spike 01 was re-run in the same session** (CPU part skipped) and reproduced its recorded numbers exactly: herd with 3cm LOD 2.49 ms creatures and 3.67 ms whole frame, close-up 1.05 ms. That run is `results/spike01-rerun-2026-10-01T23-49-04-650Z.json`. Its own saves were intercepted, so spike 01's directory is unchanged.
- The final run is `results/run-2026-10-01T23-50-06-326Z.json`. Each variant was measured in three interleaved rounds of 90 back-to-back frames (after 30 warm-up frames); the rounds agreed to the timestamp quantum. An earlier full run measured variants one after another (`run-2026-10-01T23-46-22-779Z.json`), and it agrees.

### Verdict

| Kill criterion | Measured here | Verdict |
|---|---|---|
| 1. Herd: creatures ≤ 4 ms on the M4 (≤ 8 ms on the M1) | **2.49 ms** | **Provisional pass** |
| 2. Competitive with spike 01 | Herd **2.49 vs 2.49 ms** (1.00×), whole frame 3.74 vs 3.67 ms. Close-up **2.36 vs 1.05 ms** (2.2×). | Herd: **competitive**, not better. Close-up: **worse**. |
| 3. Matches a brute-force reference | Herd: mean 0.22/255, 0.42% of pixels over 8/255. Close-up: 0.10, 0.13%. Fill: 0.05, 0.06%. No step-cap hits except fill (0.035% of creature pixels). | **Pass** for herd, close-up and fill. **Fail** for the herd of 160 (0.80% over 8/255); it passes at a tenth of a pixel, for +0.59 ms. |
| 60 fps when paced at 60 Hz | Two runs of 150 frames: median 7.9 / 10.4 ms, max 16.9 / 16.5 ms. 1 of 300 frames over 16.7 ms. | Pass, with a caveat (below) |

**So yes, it's possible:** pure ray marching draws spike 01's herd for the same GPU time, with no triangles, no extraction, no mesh LOD and no mesh memory. It loses in close-ups, and its shadows cost more.

### Where creature time goes

GPU ms. "Visibility + shading" is creature time without shadows, and includes binning.

| Scene | Covered pixels | Binning | Visibility + shading | Shadows | **Creatures** | Spike 01, same session |
|---|---|---|---|---|---|---|
| Herd of 40 | 287 K (13.8%) | 0.20 | 1.31 | 1.18 | **2.49** | 2.49 (3cm LOD: shadow 0.79, shade 1.70) |
| Close-up | 456 K (22.0%) | 0.07 | 1.51 | 0.85 | **2.36** | 1.05 (shadow 0.20, shade 0.92) |
| Fill (close-up filling the screen) | 1.31 M (63.3%) | 0.07 | 3.93 | 1.57 | **5.51** | not measured |
| Herd of 160 | 377 K (18.2%) | 0.39 | 1.97 | 1.70 | **3.67** | not measured |

- **The herd ties, with a different split.** Visibility and shading are cheaper than rasterizing and shading the 3cm mesh (1.31 vs 1.70 ms). Marched shadows are dearer than the shadow map (1.18 vs 0.79 ms). The shadows aren't equal in quality: these are soft, per-pixel and include self-shadowing; spike 01's are a hard 2048² map.
- **Close-ups are where it loses: 2.2×.** Ray-marching cost grows with covered pixels × steps per pixel. Spike 01's grows with triangles plus one shading evaluation per pixel, and a close-up has few triangles. Shadows are the bigger part of the gap (0.85 vs 0.20 ms).
- **A creature filling 63% of the screen costs 5.5 ms.** Extrapolating linearly to full coverage gives ~8.7 ms (an estimate). On its own that's over the margin rule's 4 ms, so a hero close-up would be inconclusive or worse on the M1.
- **Instance count barely matters.** Four times the herd costs 1.47× (at 1.3× the coverage). Spike 01 would need 4× the extraction (86 ms of GPU per 40 at 3cm) and 4× the mesh memory (36 MB per 40 at 3cm); neither exists here.

### Why it's fast where it is

| Scene | Steps per creature pixel | Parts per evaluation (of 28) | Marches that miss | Shadow steps per march | Terrain steps per pixel |
|---|---|---|---|---|---|
| Herd | 4.4 | 1.76 | 30% | 5.1 | 6.9 |
| Close-up | 5.7 | 1.72 | 35% | 5.6 | 9.4 |
| Fill | 7.1 | 1.91 | 23% | 7.1 | 10.4 |
| Herd of 160 | 4.5 | 1.70 | 28% | 4.2 | 5.9 |

- **Per-ray part masks do most of the work.** An evaluation touches 1.7–1.9 of the 28 parts, and the march starts at the first part bound, so it takes 4–7 steps. The heat maps (`results/*-heat.jpg`) show cost concentrated on the silhouettes, the classic sphere-tracing worst case.
- **Over-relaxation didn't help** (1.2: same or +0.2 ms; 1.6: +0.07 to +0.4 ms). The marches are already short; the bounds do the long skipping.
- **Binning is cheap:** 0.2 ms for 40, 0.39 ms for 160. Lists never overflowed.

### Variants

Creature GPU ms. Each fast variant is compared with the reference of its own surface.

| Scene | Main (¼ px) | ½ px | ⅒ px | Displaced | Relax 1.2 | Relax 1.6 |
|---|---|---|---|---|---|---|
| Herd | 2.49 | 2.10 | 2.88 | 3.21 | 2.49 | 2.56 |
| Close-up | 2.36 | 2.03 | 2.69 | 5.31 | 2.36 | 2.49 |
| Fill | 5.51 | 5.11 | 5.97 | 17.17 | 5.70 | 5.90 |
| Herd of 160 | 3.67 | 3.21 | 4.26 | 4.39 | 3.67 | 3.87 |

- **The hit tolerance is a quality knob.** Half a pixel saves ~0.4 ms, but a ray that passes within the tolerance counts as a hit, so silhouettes grow by up to half a pixel: the herd fails criterion 3 (1.18% of pixels over 8/255). A tenth of a pixel passes everywhere (herd 0.09%, herd of 160 0.17%).
- **True displaced silhouettes work and are visible up close** (`results/fill-full-crop.jpg` against `fill-displaced-crop.jpg`), at +0.7 ms in the herd, +3.0 ms in the close-up and +11.7 ms in the fill. Spike 01 can't show them without much finer meshes. The displaced fill hits the step cap on 0.19% of creature pixels, which fails criterion 3's cap rule.

### Other costs

- **Pipelines: 4 per scene** (spike 01: 8). Cold: `trace` 211 ms, all at once 207 ms (spike 01: 314 ms). Warm: 1.7 ms.
- **Memory:** tile and light lists 6.1 MB, posed instances 0.05 MB per 40. No meshes (spike 01: 36 MB at 3cm, 143 MB at 1.5cm). The frame is RGBA16F (15.8 MB), with no depth buffer.
- **Realization:** none. Spike 01 spends 2.2 ms (3cm) to 9 ms (1.5cm) of GPU per individual before it can draw it.
- **CPU:** posing the parts in JS takes 0.6 ms per frame for 40 and 1.7 ms for 160 on the main thread. Spike 01 also posed bones on the CPU but didn't time it. A compute pass could do it (untested).
- **Terrain** (not creature time): the marched heightfield costs 1.31 ms in the herd view against spike 01's rasterized 0.52 ms, which is why the whole frame is 3.74 against 3.67 ms despite the tie on creatures.

### What ray marching asks of the content and the language

1. **Parts must be rigid in a bone's frame,** or deformed by warps with a known Lipschitz bound. A traced field can't be skinned. Spike 01 bends the neck and tail by skinning one cone across several bones; here each became a chain of per-bone cones (20 parts → 28). Joints are smooth unions of rigid parts, which looks fine for this creature (`results/closeup-full.jpg`). Muscle bulge or sliding skin would need warps, and every warp's distortion shrinks the safe step. This is the biggest constraint, and it bears on D-086.
2. **Each part needs a bound inflated by margins derived from the combinator tree:** the blend radius, the part's own inner blends, and the displacement's amplitude. These are interval-style facts a compiler can derive (D-080). The ellipsoid needed a bespoke derivation (above).
3. **Displacement needs a Lipschitz fact.** The kernel assumes the fbm's slope is at most 3 per unit of amplitude × frequency (a strict bound is ~6.5). The reference check passed with it, so the hypothesis held for this content. It isn't proven.
4. **D-092's problem doesn't arise.** The ellipsoid bound's gradient reaches 11 deep inside the body, but marching only evaluates outside the surface. That's consistent with D-092's scoped facts.
5. **Nothing here is engine-specific (D-050):** bounds, masks and rigid transforms make sense in any program that traces a field.

### Caveats

- **Device:** an M4, not the M1. The secondary devices, Safari and Firefox weren't tested.
- **Timestamps are quantized to ~65.5 µs.** The herd's 2.49 vs 2.49 ms tie is a tie within that, not a measurement to three digits.
- **Creature time is a difference of two passes** (with creatures, minus terrain only). It charges creatures for the shadow rays cast from terrain pixels, which is fair. Spike 01's figure is the sum of its creature passes.
- **Pacing:** at 60 Hz the OS lowers clocks, and the 3.7 ms frame stretches to a median of 7.9–10.4 ms, with a p95 near 15 ms and one frame at 16.9 ms. Spike 01's LOD herd, paced in the same session, had a higher median but a much tighter spread (11.4 ms, max 11.8 ms). Why the spreads differ (compute against raster work in the clock governor, perhaps) isn't tested. As in spike 01, pacing shows the clocks the OS picks, not the headroom.
- **The images aren't identical to spike 01's:** soft shadows against a hard map, rigid neck and tail chains against skinned bends, an analytic terrain against a 62cm grid. Neither spike anti-aliases.
- **"What the compiler would emit"** covers the field functions, bounds and posing. The binning and tracing are engine code.
- **Changes made after exploratory measurements, before the final run** (the criteria didn't change):
  1. Displacement in the march made the first close-up cost 5.2 ms. It moved to the shading normal, for parity with spike 01, and the displaced march became a variant.
  2. Binning with one 64-wide workgroup per tile cost 0.52 ms. One thread per tile costs 0.13 ms.
  3. A half-pixel hit tolerance failed criterion 3 on the herd, so the main configuration uses a quarter pixel. The soft-shadow estimate and the hit refinement were added at the same point, to cut penumbra and grazing-terrain differences against the reference.

### What this means for the design

Nothing here is recorded in `decisions.md`; that's the owner's call.

- **Ray marching is viable at herd distance and competitive there,** and it removes extraction, mesh LOD and mesh memory, with cost that tracks screen coverage rather than instance count.
- **It's worse for close-ups** (2.2× in the close-up, 5.5 ms at 63% coverage), and marched shadows cost more than a shadow map.
- **That points to choosing by screen size,** which D-001 already allows: trace small and distant creatures, rasterize close ones. That's a hypothesis for a combined spike, not a finding. It would mean authoring creatures as rigid parts (point 1 above) so both paths can draw them.
