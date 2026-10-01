# Spike 01: the grazer, hand-written

*D-067's measurement spike. It's throwaway code, not the start of the engine.*

## What this is

This is the WGSL and WASM the wrela compiler *would* emit for the grazer in sketches 01 and 02, written by hand:

- **The field:** one monomorphized function for `GrazerField`, plus the derived interpretations the compiler would generate. Those are the primal (distance), the forward-mode derivative (gradient), and per-part Lipschitz intervals for culling. Every seed-dependent number comes from a uniform (D-070).
- **Extraction (sketch 02 §§3–6):**
  - `cull_blocks`: per-part intervals give a `PartMask`, and the live-block counter doubles as the indirect-dispatch arguments.
  - `place_vertices`: dual contouring with a QEF, plus skin weights from part distances.
  - `emit_quads`: the index counter doubles as the indirect-draw arguments.
  - Nothing is read back.
- **Drawing (sketch 02 §7):**
  - linear-blend skinning
  - three fragment shaders: per-pixel field evaluation (the thesis), a texture-lookup cost proxy (the baseline that matters, T1), and per-vertex normals (the floor)
  - a shadow map for creatures (D-086)
- **CPU (D-074):** the grazer field and a terrain field in Rust, compiled to `wasm32-unknown-unknown` with strict IEEE f32. It covers evaluations per millisecond, adaptive mass integration (T3), terrain raycasts, and a bit-for-bit comparison against a native build.

## Kill criteria (D-067), written before measuring

1. **Herd scene:** 40 grazers on terrain at 1920×1080 hold 60 fps (16.7 ms per frame) on the primary reference device, with creatures taking **≤ 8 ms of GPU time**. Creature time = shadow pass + depth prepass + shading pass.
2. **If the hand-written version can't meet that,** creatures fall back to cooking on device: baked meshes and textures, rendered conventionally.
3. **The comparison that matters** is per-pixel field shading against the texture-lookup baseline, not against a naive grid.

### This machine is not the reference device

The spike ran on a **MacBook Air M4 (8-core GPU, 16 GB)** in Chromium 152 (the Claude desktop app's browser pane). The primary reference device is a MacBook Air M1, 8 GB, on Chrome stable (D-068).

- **An unverified estimate:** public GPU benchmarks put the M4 at about 1.5–2× the M1.
- **Rule, set before measuring:** a pass here counts as a *provisional* pass for the M1 only if creatures take **≤ 4 ms** of GPU time, a 2× margin.
- Between 4 and 8 ms, the result is **inconclusive until measured on an M1**.
- Above 8 ms, the result is a **fail**, since the M1 won't be faster.

The secondary devices (Iris Xe, RTX 3060) and Safari/Firefox weren't tested.

## Layout

| File | What |
|---|---|
| `grazer.js` | What `grazer(seed)` computes: skeleton, part parameters and the uniform layout. Also the walk-cycle pose (presentation code). |
| `field.wgsl` | The grazer field and its derived interpretations |
| `extract.wgsl` | `cull_blocks`, `place_vertices`, `emit_quads` |
| `draw.wgsl` | Skinning, the three shading modes, shadows, terrain, blit |
| `main.js` | Harness: pipelines, extraction, the herd scene, timing |
| `cpu/` | Rust: the CPU field, mass integration, raycasts. `build.sh` builds `cpu.wasm`. |
| `results/` | Raw JSON from each run, and screenshots |
| `../serve.py` | Static server that also accepts PUTs into `results/`, so a run saves itself |

## Running it

```bash
spikes/01-grazer/cpu/build.sh
```

```bash
python3 spikes/serve.py 8417
```

Open <http://localhost:8417/01-grazer/>, then press **Run all**. Results appear on the page and in `window.__results`, and are saved to `results/`. `#quick` extracts the herd and draws one frame without measuring.

The native comparison:

```bash
cd spikes/01-grazer/cpu && node params.mjs > params-seed1.f32 && cargo run --release --bin native params-seed1.f32
```

## Results (2026-10-01)

**Setup:**
- MacBook Air M4 (8-core GPU, 16 GB), in Chromium 152 inside the Claude desktop app.
- Two full runs; the final one is `results/run-2026-10-01T23-07-40-460Z.json`. They agree to within about 2%.
- GPU times come from timestamp queries under sustained load (back-to-back frames), as the median of 90 frames.

### Verdict

| Kill criterion | Measured here | Verdict under the margin rule |
|---|---|---|
| Herd: creatures ≤ 8 ms GPU on the M1 | **2.49 ms** with a 3cm mesh at herd distance; **6.49 ms** with a 1.5cm mesh on everyone | **Provisional pass** with LOD; **inconclusive** without |
| Herd: 60 fps | Whole frame 3.67 ms (LOD) / 7.21 ms (no LOD). Paced at 60 Hz, 0 of 150 frames over 16.7 ms in either. | Pass here |

- **Per-pixel field shading survives on this device.** The cooking fallback isn't triggered.
- **The margin depends on two things not yet measured:** an engine with mesh LOD, and the M1 itself.

### Where creature time goes

GPU ms per pass, best variant for each (all herd variants are faster without the depth prepass):

| Scene | Triangles | Covered pixels | Shadow | Shade: field | Shade: lookup | Shade: vertex | Creatures (field) |
|---|---|---|---|---|---|---|---|
| Herd, 1.5cm mesh | 4.17 M | 279 K (13.4%) | 2.49 | 4.06 | 3.08 | 2.82 | 6.49 |
| Herd, 3cm mesh | 1.04 M | 279 K (13.5%) | 0.79 | 1.70 | 1.05 | 0.92 | 2.49 |
| Close-up, one grazer | 108 K | 449 K (21.7%) | 0.20 | 0.92 | 0.33 | 0.33 | 1.05 |

- **Triangles, not shading, dominate at 1.5cm.** The shadow and prepass passes each take ~2.4 ms for 4.2M triangles, and most of those triangles are smaller than a pixel at herd distance.
- **The comparison that matters (T1):** per-pixel field evaluation costs **1.6×** the texture-lookup proxy in the LOD herd (1.70 vs 1.05 ms) and **2.8×** in the close-up (2.0 vs 0.73 ns per covered pixel). In absolute terms, that's +0.65 ms for the herd.
  - The proxy reads one shared 1 MB texture, so its cache behaviour is better than real per-individual bakes would be. The measured gap probably *overstates* the field's relative cost.
  - **Extrapolation (an estimate):** a 1080p screen fully covered by grazer at the close-up rate is ~4.2 ms of field shading.
- **LOD costs almost nothing visually.** The 3cm and 1.5cm herd images differ by a mean of 0.07/255; 0.1% of pixels differ by more than 8/255, all on silhouettes. Per-pixel shading carries the detail, so mesh resolution can drop quickly with distance.

### Extraction: realizing one individual

| Cell | GPU ms | Triangles | Live blocks | Holes (all 40) |
|---|---|---|---|---|
| 3cm | 2.16 | 25.7 K | 35% | 0 |
| 2cm | 5.05 | 57.9 K | 25% | 0 |
| 1.5cm | 9.04 | 103 K | 19.5% | 2 |
| 1cm | 21.0 | 233 K | 13.8% | 10 |

- **`place_vertices` is 98% of it.** Culling and quad emission are each ≤ 0.3 ms.
- **At 1.5cm, one individual is half a frame of GPU time.** The engine has to realize coarse first and refine later, or spread the work across frames. The whole herd at 1.5cm takes 361 ms; at 3cm, 86 ms.
- **Culling keeps 14–35% of blocks,** against sketch 02's estimate of 5–10%. The intervals come from each part's distance at the block centre plus L × radius, which is loose. Tighter intervals are the obvious next optimization (unmeasured).
- **Holes:** a block's `PartMask` drops parts from the smooth-union fold, which can change the fold's rounding at a corner two blocks share. Rarely, that flips the corner's sign and a quad goes missing: 2 out of ~2M quads at 1.5cm. An order-independent mask would fix it.
- **Memory:** the herd's meshes take 143 MB at 1.5cm and 36 MB at 3cm.

### Pipelines

- **8 per scene** (budget: 64).
- **Cold**, with unique source so nothing is cached: `place_vertices` 312 ms, `shade_field` 137 ms, everything else ≤ 6 ms. All of them at once take 314 ms.
- **Warm**, in the same session: 3.4 ms in total.
- The cross-session disk cache wasn't measured.

### CPU (WASM, strict f32)

- **Evaluations of the whole grazer:**
  - WASM: 2,284 per ms; 4,837 with sphere-bound pruning
  - native: 6,928 per ms; 6,634 pruned
- **Determinism:** one hash over 1M evaluations, an adaptive mass integration and 10K raycasts. WASM in Chromium and native aarch64 both give **`fe85e8e2fa409633`**. That's one platform pair; x86 isn't tested.
- **Raycasts:** 3.2 µs per ray against a terrain field with 32 edits, about 11 evaluations each. One per foot per tick for 40 grazers is 0.52 ms of the 4 ms sim budget.
- **Mass integration (T3), per individual:**

  | Finest cell | WASM ms | Mass | Evaluations |
  |---|---|---|---|
  | 2cm | 39 | 1118.94 kg | 243 K |
  | 1cm | 151 | 1119.11 kg | 962 K |
  | 5mm | 593 | 1119.11 kg | 3.85 M |

  The uniform-grid ground truth is 1119.26 kg at 1cm (native, 25M evaluations). So 2cm is enough: it's within 0.03%.

  The sketch estimated ~100K cells at 1cm; it's 962K. Forty grazers at spawn take 1.5 s of one core even at 2cm, so physique needs workers or caching.
- **Lipschitz probe (200K points):** mean |∇| 0.98, max 11.0. 0.14% of points exceed 1.5, all deep inside the body (d < −0.45 m) near ellipsoid centres.
  - The ellipsoid bound formula's gradient is unbounded at its centre.
  - So a compiler deriving one global constant would *fail* sketch 01's `@assert(lipschitz <= 1.5)`, even though the constant holds everywhere culling and root-finding look.

### Caveats

- **Device:** an M4, not the M1. The secondary devices, Safari and Firefox weren't tested.
- **Clock scaling:** the first run timed frames one at a time, and idle gaps let the GPU clock down: the same terrain pass measured 0.4–1.6 ms. The final numbers are sustained back-to-back frames instead. When frames were paced at 60 Hz, the OS lowered clocks and the same 1.5cm herd frame took 13.6 ms instead of 7.2. That's fine for holding 60 fps, but it means 60 Hz pacing hides how much headroom is left.
- **The page was hidden** in the app's browser pane during the runs. Timestamps are unaffected; the 60 Hz pacing used a busy-wait because requestAnimationFrame and timers are throttled.
- **Timestamps are quantized to ~65.5 µs** (2^16 ns observed). Passes under ~0.2 ms are imprecise.
- **"What the compiler would emit" is an assumption.** The person writing these shaders also designed the language, and a real compiler could do better or worse.
- **Not included:** LOD transitions, far bricks, MSAA/TAA, more than one light. The terrain is a displaced grid, not a field. The creature's proportions are the spike's own.

### What this means for the design

These are recorded as D-089–D-093 in `docs/design/decisions.md`.

1. **Keep per-pixel field shading as the default.** The fallback isn't triggered here, but it's provisional until the M1 is measured.
2. **Creature cost is triangle-bound, so screen-size mesh LOD is required** (engine). It needs to drop resolution much nearer than sketch 01's 40 m bricks.
3. **Realization must be amortized** (engine): coarse first, then refine.
4. **CPU field queries fit the sim budget.** Spawn-time physique should use a 2cm finest cell and run off the main thread.
5. **Lipschitz facts need a scope** (stdlib and language): "within this distance of the surface", or the stdlib's ellipsoid needs a formula with a bounded gradient.
6. **The strict-float determinism hypothesis held** for wasm32 against native aarch64.
