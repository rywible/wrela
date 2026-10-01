# Sketch 04: what ships

*Status: draft, 2026-10-01. Follows D-067 and D-069: the compiler runs ahead of time, and games ship WASM, WGSL and data. The target code is real: [spike 01](../../../spikes/01-grazer/)'s hand-written WGSL and Rust/WASM, which were measured. What's imagined is the compiler that would produce them. Decisions are cited as D-NNN from [../decisions.md](../decisions.md).*

Sketches 01 and 02 showed the source. This sketch shows the build's output for the same grazer:
- **the type** that `grazer(seed)` returns
- **the lowering** of that type to WGSL
- **the derived interpretations** in the output
- **the pipelines** and the layout contract between WASM and WGSL
- **the CPU side**
- **the sizes**

It ends with what the spike says the compiler has to do well.

---

## 1. The build

```
grazer.wrela, engine/**, std/**   ──►   wrela build (Rust, on the developer's machine)
                                           │
                    ┌──────────────────────┼───────────────────────┐
                    ▼                      ▼                       ▼
              game.wasm              *.wgsl, one per          data/ (consts, weights)
        (engine + game, CPU)      pipeline family (GPU)       + manifest (layouts)
```

- **One whole-program compile.** The engine is compiled into the game, because monomorphization needs to see every instantiation (D-069).
- **`game.wasm`** holds every CPU function the game reaches: the engine's tick, `grazer(seed)`, `grazer_physique`, `step`, the stdlib's region code.
- **WGSL modules** hold every GPU entry point, instantiated per concrete type (§5).
- **`data/`** holds what `const` items evaluated to at compile time: `HOOF_MODES`, the gait network's weights (D-036), anything `embed`ded (D-073).
- **The manifest** lists pipelines, bind group layouts and every `GpuData` layout, so the WASM side and the WGSL side agree byte for byte (D-084).

---

## 2. The type of a grazer

`grazer(seed)` returns `Creature<Tissue>` (sketch 01 §3). In return position that names one concrete type, which the compiler infers (D-070). Written out, it's roughly:

```
Creature<Tissue,
  Blend<k,                                                  // .blend(k: 6cm)
    Part<"chest", With<Displace<SmoothUnion<Ellipsoid, Translate<Ellipsoid>>, Fbm>, Closure#dapple>>,
    Part<"neck",  With<RoundCone, Const<Tissue>>>,
    Part<"head",  With<SmoothUnion<Ellipsoid, RoundCone>, Const<Tissue>>>,
    Part<"tail",  With<RoundCone, Const<Tissue>>>,
    EachLeg<4,
      Fill<3, With<RoundCone, Const<Tissue>>>,              // three segments per leg
      Foot<With<Intersect<RoundCone, HalfSpace>, Const<Tissue>>>>>>
```

- **That's the structure:** which combinators, in what shape. Every grazer in the herd has this type, so the herd shares one set of pipelines.
- **The value of that type is plain data:** radii, centres, `k`, the noise offset, octave counts.
  - In the spike it's the `Grazer` uniform: 8 header floats plus 16 per part, 1,312 bytes in all.
  - `grazer.js` computes it from the seed. In wrela, `grazer(seed)` would compute it in `game.wasm`, and the engine would upload it.
- **`EachLeg<4, Fill<3, ...>>`** is a homogeneous collection: twelve values of one type. The lowering keeps it a collection (§3), not twelve copies.

**Open:** how part names and bone bindings appear in the type. Bone names are checked at compile time (D-031), so they're in the type somehow, but neither the type nor the uniform needs them at runtime.

---

## 3. Lowering the type to WGSL

Each node of the type becomes a function, monomorphized for its children. These are excerpts from the spike's `field.wgsl`, which is what the lowering should produce.

**A combinator node becomes a function of its children:**

```wgsl
fn torso_d(p: vec3f, fp: f32) -> f32 {
  return torso_base_d(p) + G.misc.y * fbm_d(p + G.seed.xyz, fp);   // Displace<SmoothUnion<...>, Fbm>
}
```

**A homogeneous collection becomes a loop over uniform data.** Twelve leg segments share one type, so they're one loop body, not twelve inlined copies:

```wgsl
for (var i = 4u; i < 16u; i++) {                       // EachLeg × Fill: same type, a loop
  if (((mask >> i) & 1u) != 0u) { d = smin_d(d, cone_d(p, i), k); }
}
```

**A control-flow value is data** (D-070). `fbm(octaves: 4)` loops to a uniform bound, and a footprint can end it early:

```wgsl
let octaves = u32(G.misc.w);                          // a uniform loop bound
for (var o = 0u; o < octaves; o++) {
  let w = a * octave_weight(fp, fr);                  // bandlimit: fade octaves finer than the footprint (D-077)
  if (w <= 0.0) { break; }
  ...
}
```

**Closures are gone.** `.with(|p| Tissue { albedo: dapple(p, seed), ..HIDE })` inlines into the shading function as straight-line code (D-047).

**The engine's `PartMask`** (D-080) shows up as the bit tests above. It's ordinary data, a `u32` the engine computed, and the compiler doesn't know what it means. The compiler's own `LiveMask` pruning would apply *inside* each part's expression and isn't shown.

---

## 4. Derived interpretations in the output

For each pure function an entry point reaches, the compiler emits only the interpretations that are actually used (D-012):

| Interpretation | Used by | Spike's version | What the measurement says |
|---|---|---|---|
| Primal, `*_d` | culling, root-finding | `grazer_d` | |
| Forward derivative, `*_g` | normals in extraction and shading | `grazer_g` | Per-pixel field shading costs 1.6–2.8× a texture-lookup proxy (spike results) |
| Interval | `cull_blocks` | Each part's distance at the block centre ± L × radius | Keeps 14–35% of blocks, against sketch 02's estimate of 5–10%. **Tightness is a compiler quality metric:** looser intervals cost extraction time directly. |
| Lipschitz facts | interval, `.to_bound()` | Assumed per part (`L_ELLIPSOID = 1.25`) | The ellipsoid bound's gradient reaches 11 near its centre. A global constant would fail sketch 01's `@assert`, so facts are now scoped to near the surface (D-092). |
| Channel slices | `shade`, mass integration | Separate functions per consumer | Extraction computes no channels; mass computes density only (D-002). |

The spike had to write the channel slices by hand, and it was easy to compute a channel nobody reads. Slicing per consumer is worth having in the compiler.

---

## 5. Pipelines and the layout contract

What the build generates for the herd scene (D-070's pipeline-count query would print this):

```
$ wrela query pipelines --scene herd
cull_blocks<GrazerField>       compute   1 instantiation
place_vertices<GrazerField>    compute   1 instantiation
emit_quads                     compute   1
skin_shadow                    render    1
skin (prepass)                 render    1   (unused: the herd is faster without it)
shade<GrazerField>             render    1
terrain                        render    1
blit                           render    1   (engine)
total: 8 of a 64-pipeline budget (D-068)
```

**Measured creation time on the M4** (spike results):

| Case | Time |
|---|---|
| Cold, unique source, one at a time | `place_vertices` 312 ms, `shade_field` 137 ms, everything else ≤ 6.4 ms |
| Cold, all at once | 314 ms |
| Warm, same session | 3.4 ms total |

That fits behind a loading screen. It also argues for keeping instantiation counts low: two grazer types would double the two expensive pipelines.

**The layout contract.** Every type that crosses to the GPU is `GpuData` (D-084), so one layout serves:
- the WASM code that fills it (`grazer(seed)` writing the `Grazer` uniform)
- the WGSL that reads it
- the vertex-buffer attributes. `SkinVertex` is 48 bytes, written by `place_vertices` as storage and read by `skin` as a vertex buffer.

The manifest records each layout, and the build checks that both sides agree. In the spike, `grazer.js` and `field.wgsl` agree by convention only. That's the bug class this removes.

---

## 6. The CPU side

| Work | When | Measured (WASM, M4) |
|---|---|---|
| `grazer(seed)`: compute the 1,312-byte value | At spawn | Not measured: the spike computes it in JS. It's a few hundred float operations. |
| `grazer_physique`: adaptive mass integration | At spawn | 39 ms at a 2cm finest cell; 151 ms at 1cm. The two agree within 0.02%, and both are within 0.03% of the uniform-grid ground truth. |
| `step`: one raycast per foot | Every tick | 0.52 ms for 40 grazers (of the 4 ms sim budget) |
| Field evaluation, whole grazer | | 2,284 per ms (4,837 with pruning) |

**Bit-identical across one pair of targets** (D-074): the CPU code in the spike gave identical hashes compiled to wasm32 and to native aarch64. x86 isn't tested. Strict IEEE f32, no fused multiply-add, only correctly rounded operations.

**One consequence for the source.** `finest: 1cm` in `grazer_physique` (sketch 01 §4) costs four times `finest: 2cm` and changes the mass by 0.02%. Since the value is gameplay (D-033), a game author should see that cost: the cost query (D-021) is how they'd see it.

---

## 7. Sizes

Measured from the spike's hand-written output; the compiler's output will differ:

| Artifact | Bytes | gzip |
|---|---|---|
| Field + extraction WGSL | 22,971 | 7,462 |
| Field + drawing WGSL | 17,751 | 5,748 |
| CPU field, mass and raycasts (Rust → WASM) | 38,242 | 14,212 |
| The grazer's content: the `Grazer` value | 1,312 per individual, computed from a seed | — |

**An estimate, not a measurement:** the grazer's whole contribution to the download is under 30 KB compressed, mostly code. That's against a time-to-play budget of 6 MB including the engine (D-069). The budget will be spent on the engine, terrain, other creatures and the first music cue, not on this creature.

---

## 8. What the compiler has to do well

The spike surfaced these. Each is a requirement on the compiler or the language, not a new feature request for the engine:

1. **Keep homogeneous collections as loops over uniform data,** not unrolled copies (§3). It's the difference between one pipeline whose size doesn't depend on leg count and twelve inlined copies.
2. **Emit only the interpretations and channels that are used** (§4).
3. **Derive tight intervals.** Interval tightness sets extraction cost; it's a measurable quality metric for the compiler.
4. **Make pruned evaluation agree at shared samples.** Two blocks with different masks evaluated a shared corner and occasionally disagreed on its sign, which left a hole: 2 holes in ~2M quads. Either pruned evaluation is bit-exact with unpruned evaluation, or the engine evaluates shared samples under one mask (D-091).
5. **Support workgroup-shared memory and barriers in kernels.** `place_vertices` evaluates each block's 125 corners once into workgroup memory, then shares them across 64 invocations. Sketch 02 didn't show this, the language has no design for it yet, and the spike needed it (D-093).
6. **Track the scope of declared facts** through composition, and check it against what each consumer needs (D-092).
7. **Check the layout contract** between WASM and WGSL at build time (§5).

---

## Not covered yet

- How the manifest is versioned within a build (D-069: the IR stays internal)
- Hot reload: which artifacts change when one constant changes
- `@deterministic` code's build-time checks: the effect table, NaN canonicalization
- Music and other streamed data
- The console's view of a game (D-082)
