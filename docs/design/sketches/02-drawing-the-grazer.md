# Sketch 02: drawing the grazer

*Status: draft for discussion, 2026-10-01. Written in the memory model's parameter modes ([memory-model.md](../memory-model.md)). Revised for D-050–D-053 (the compiler knows nothing about the engine) and for the [2026-10-01 audit](../reviews/2026-10-01-audit.md): structure is types (D-070), parts are an engine concept (D-080), and kernel outputs are invocation-safe (D-084). The syntax is imagined. Decisions are cited as D-NNN from [../decisions.md](../decisions.md). Questions continue from sketch 01, at Q13. Q14–Q19 were accepted as recommended (D-044–D-049).*

Sketch 01 was game content. This sketch is **engine code** (layer 2): what turns the grazer into pixels. It tests two things:
- Is wrela pleasant for writing the engine itself?
- Can the engine get everything it needs from the language and stdlib alone (D-050)?

It also covers most of the "hello field" milestone.

The pipeline:

| When | Step |
|---|---|
| **When the engine realizes an individual** (for example at spawn), then cached | 1. Cull blocks with interval arithmetic and pruning (`@compute`) |
| | 2. Place one vertex per crossed cell, using dual contouring (`@compute`) |
| | 3. Emit quads into an index buffer, with counts staying on the GPU (`@compute`) |
| **Every frame** | 4. Upload the bone palette for the current pose |
| | 5. Skin the mesh (`@vertex`) |
| | 6. Shade by evaluating the field's channels per pixel in rest space (`@fragment`) |

---

## 1. Look settings are ordinary engine data

```wrela
// engine/creature/look.wrela

/// How a creature is drawn. Game code overrides fields (sketch 01 §5), and the engine's
/// quality profiles override them per device. Nothing here is known to the compiler.
pub struct CreatureLook {
    pub surface: Mesh           = Mesh { tolerance: 2mm },
    pub deform:  Skin           = Skin { weights: ByPart { falloff: 5cm }, max_bones: 4 },
    pub shading: Shading        = Shading::PerPixel,
    pub far:     Option<Bricks> = None,
    pub shadow:  Shadow         = Shadow::Map { resolution: 1024 },   // creatures deform: shadow maps (D-086)
    pub eval:    Eval           = Eval::Compiled,     // or Interpreted, for fields built at runtime (D-029)
}
```

**Notes:**
- **The engine's guarantees come from general features:**
  - **Drawing can't affect gameplay.** Realization borrows the creature (the default mode), so it can't write to it. Its outputs live on the GPU, and reading them back is nondeterministic, which `@deterministic` code forbids (D-052).
  - **Error bounds** (`tolerance`) are an engine contract, checked by engine debug code that samples the field against the mesh.
- **Struct field defaults** (D-048) let game code mention only what it changes. That's the general sugar that replaced the old `schedule` block (D-053).

---

## 2. GPU data

```wrela
/// One vertex of an extracted, skinnable mesh. `GpuData` fixes its layout to WGSL's rules
/// (D-084), and the compiler does the padding (D-009). Layout is always lossless; lossy
/// encodings are explicit types (Q19).
pub struct SkinVertex: GpuData {
    rest_pos: vec3<m>,          // a 3-vector of f32<m>; units erase to f32 in the layout
    normal:   UnitVec3,
    bones:    [u8; 4],
    weights:  [Unorm8; 4],      // explicit 8-bit quantization
    parts:    PartMask,         // which parts matter near this vertex (§3): engine data (D-080)
}

/// Per-frame data for one creature: bone matrices for its current pose.
pub struct Palette: GpuData {
    bones: [Mat4; MAX_BONES],
}
```

Nobody pads anything by hand. The same struct is used from the CPU, when allocating and debugging, and from the GPU.

---

## 3. Extraction, pass 1: which blocks matter?

```wrela
/// Which blocks does the surface pass through, and which parts matter in each?
@compute(64)
fn cull_blocks<F: Surface + Parts>(
    field: F,
    grid: Grid,
    live: mut Append<LiveBlock>,
    id:   GlobalId,
) {
    let block = grid.block(id.x)
    let d = field.interval(block.bounds)        // conservative range of distances in this block
    if !d.contains(0m) { return }               // no surface here; never evaluated again
    live.push(LiveBlock {
        block,
        parts: field.parts_near(block.bounds.grow(grid.cell_size)),   // engine: parts that can matter here, plus a margin
    })
}
```

**Notes:**
- **`F` is the field's type, which is its structure** (D-070). The kernel is monomorphized for it at build time, and the field's numbers arrive as uniforms (§5).
- **Nobody wrote `interval`.** The compiler derives it from the field's code (D-012). On the GPU it widens outward by each operation's error bound, so it's conservative, never too narrow (D-075).
- **`parts_near` is engine code** (D-080). It tests each part's derived interval and returns a `PartMask`.
  - Inside each part, the compiler's own pruning applies invisibly, through opaque masks that no library reads.
  - The compiler never needs to know what a part, or a smooth union, *is*.
- **Rough estimate, to be measured:**
  - The grazer's bounding box at ~1.5cm cells is about 4.4M cells.
  - Culling keeps the 5–10% near the surface.
  - Pruning cuts each evaluation from about 20 parts to 2–3.
  - Together that's roughly **100× fewer part evaluations** than a naive grid.

  That's a win over a naive grid, not over a competent engine: a hand-written bounding-volume hierarchy over parts would cull just as well. What derivation adds is that nobody has to write it, for any field anyone authors, agents included. Whether field evaluation can compete with baked lookups *at all* is a hypothesis, and the D-067 spike measures it (audit T1).

---

## 4. Extraction, pass 2: placing vertices

```wrela
/// In each live block, give every cell the surface crosses one vertex: the point that
/// best fits the surface crossings on the cell's edges (dual contouring's QEF).
@compute(4, 4, 4)
fn place_vertices<F: Surface + Parts>(
    field: F,
    skin:  Skin,
    blocks:   [LiveBlock],
    verts:    mut Append<SkinVertex>,
    cell_map: mut AtomicMap<u32>,     // invocation-safe: atomic stores (D-084)
    block_id: WorkgroupId,
    cell_id:  LocalId,
) {
    let block = blocks[block_id.x]
    let cell  = block.cell(cell_id)
    let f     = field.with_parts(block.parts)             // evaluate only the parts that matter here

    var qef = Qef::new()
    for edge in cell.edges() {                            // 12 edges, unrolled at compile time
        if f.sign(edge.a) == f.sign(edge.b) { continue }
        let p = edge.root(|x| f.distance(x), iters: 4)    // where the surface crosses this edge
        qef.add(p, f.gradient(p).normalize())             // gradient: compiler-derived (D-012)
    }
    if qef.is_empty() { return }

    let pos = qef.solve(within: cell.bounds)
    let (bones, weights) = skin.weights_at(pos, f.parts())   // each part's own distance → skin weights (D-023)
    cell_map.store(cell, verts.push(SkinVertex {
        rest_pos: pos,
        normal:   f.gradient(pos).normalize(),
        bones,
        weights,
        parts:    block.parts,
    }))
}
```

**Pass 3 (not shown)** walks the crossed edges and emits a quad joining the four neighboring cells' vertices through `cell_map`. It writes the index count straight into an indirect-draw buffer, so the CPU reads nothing back during extraction.

**Notes:**
- **The closure** `|x| f.distance(x)` is fine in GPU code. It's resolved statically and inlined (Q17).
- **Every invocation in the dispatch shares `verts` and `cell_map`.** Exclusivity is per binding, not per invocation. So both are invocation-safe types: an append buffer and atomic stores. A plain `mut [u32]` here would be rejected (D-084).
- **`Parts` and `skin.weights_at`** are engine code. They derive skin weights from each part's own distance field, so nobody paints weights.

---

## 5. Specialization: what makes a new pipeline

What the build generates for a herd of 40 grazers. Every grazer has the same type (D-070):

```
cull_blocks<GrazerField>       1 pipeline     one type for every seed
place_vertices<GrazerField>    1 pipeline
emit_quads                     1 pipeline
skin                           1 pipeline
shade<GrazerField>             1 pipeline
+ 40 small uniform buffers with each grazer's seed-derived values (bulk, scale, dapple seed)
```

**How it decides (D-070):**
- **Structure is the type:** which combinators, in what shape. Generic kernels are monomorphized per field type at build time.
- **Everything else is data, delivered as uniforms.** That includes radii, `bulk`, seeds, and control-flow values: `fbm(octaves: 4)` loops to a uniform bound, and unrolling it is the optimizer's choice.
- **Values known at compile time** are constant-folded, as in any compiler.

**Why it matters:** baking every number into code would mean 40 pipelines. Pipeline creation is one of the slowest operations in a browser, so you'd get 40 hitches.

**A seed can't change a type, so it can't change structure.**
- Optional horns would be an `Option<Horns>` field: one pipeline, with a uniform branch.
- Making horns a *type-level* choice instead gives two instantiations. The pipeline query then reports "grazer: 2 instantiations → 2 pipelines per kernel." That's the kind of answer an agent needs before it ships a herd.

None of this is engine-specific. It's ordinary monomorphization, and any generic wrela code gets the same behavior.

---

## 6. Realization, end to end

```wrela
/// Turn one creature into a skinned mesh on the GPU. Ordinary engine code: the engine
/// decides when to call it (at spawn) and caches the result.
pub fn realize_mesh<C: Blend>(
    creature: Creature<C>,           // parameter position: generic over the concrete creature type (D-070)
    look: CreatureLook,
    gpu:  mut Gpu,
) -> SkinnedMesh {
    let field = creature.field()
    let grid  = Grid::covering(creature.bounds(), cell: look.surface.cell_size(field))   // tolerance + curvature bound
    var live  = gpu.append::<LiveBlock>(capacity: grid.block_count())     // `var`: passed as `mut` below
    var verts = gpu.append::<SkinVertex>(capacity: grid.cell_count() / 8)
    var cells = gpu.cell_map(grid)
    var quads = gpu.indices(capacity: verts.capacity() * 6)

    gpu.dispatch(cull_blocks, (field, grid, mut live), count: grid.block_count())
    gpu.dispatch_indirect(place_vertices,
        (field, look.deform, live, mut verts, mut cells), groups: live.count())
    gpu.dispatch_indirect(emit_quads, (live, cells, mut quads), groups: live.count())

    let draw = quads.indirect_draw()                              // counts stay on the GPU
    SkinnedMesh { verts: take verts, quads: take quads, draw }    // moves are written `take` (D-064)
}
```

**Notes:**
- **Passing `field` to a generic kernel is all it takes.** The kernel is monomorphized for the field's type at build time (D-070). `gpu.dispatch` is stdlib. Nothing like `compile_pipeline(...)` appears in engine code, and the compiler doesn't know this is an engine.
- **The first draft built `SkinnedMesh { verts, quads, draw: quads.indirect_draw() }`.** That moved two named places without `take`, and then read `quads` after moving it (audit S2). The memory model's rules caught it.
- **`look.eval == Eval::Interpreted`** would make the engine pass `stage::interpret(field)` instead of `field`. That's the stdlib's tape evaluator for fields built at runtime: the same kernels, instantiated once for the tape type (D-029).
- **Caching is engine code.** After a mesh is in use, the engine reads it back once, off the critical path, and stores it in OPFS keyed by build and seed. An extracted mesh is valid on any GPU adapter, so the adapter isn't part of the key (D-083).
- **Detail finer than the mesh comes from shading.** The grazer's 3mm wrinkles are smaller than the cells, so per-pixel gradients carry them (§7). `tolerance` then governs *silhouette* error. Blocks whose interval shows unresolved detail get refined (not shown).

---

## 7. Drawing: skin and shade

```wrela
pub struct Skinned {
    clip:     ClipPosition,       // typed builtin: the vertex output position (Q16)
    rest_pos: vec3<m>,            // interpolated; shading happens in rest space
    rot:      Quat,               // interpolated skinning rotation, for per-pixel normals
    normal:   UnitVec3,
    parts:    Flat<PartMask>,     // not interpolated
}

@vertex
fn skin(v: SkinVertex, palette: Palette, view: View) -> Skinned {
    let m = v.bones.zip(v.weights).sum(|(b, w)| palette.bones[b] * w.to_f32())
    Skinned {
        clip:     view.project(m * v.rest_pos),
        rest_pos: v.rest_pos,
        rot:      m.rotation(),
        normal:   (m * v.normal).normalize(),
        parts:    Flat(v.parts),
    }
}

@fragment
fn shade<F: Surface + Channels<C>, C: Blend>(
    field:   F,
    shading: Shading,
    s:      Skinned,
    lights: Lights,
) -> Color {
    let f  = field.with_parts(s.parts.0)              // 2–3 parts per pixel, not 20
    let fp = fwidth(s.rest_pos)                       // this pixel's footprint in rest space
    let t  = f.channels(s.rest_pos, footprint: fp)    // only albedo, roughness, material (D-002); detail finer
                                                      // than a pixel fades instead of aliasing (D-077)
    let n = match shading {                           // a uniform branch, folded away when the look is a constant
        Shading::PerPixel  => s.rot * f.gradient(s.rest_pos, footprint: fp).normalize(),   // wrinkles finer than the mesh
        Shading::PerVertex => s.normal,
    }
    lights.shade(t.albedo, t.roughness, t.material, n)
}
```

**Notes:**
- **Materials at pixel resolution, with no textures and no UVs.** The fragment shader evaluates the sliced, pruned field in rest space. The dapple pattern from sketch 01 is computed, not sampled.
- **`Shading::PerVertex`** is the cheap setting for low-end profiles. Baking channels into a brick texture would be a third option (not shown).
- **Part culling crosses stages.** The part mask computed in pass 1, at realization time, is still saving work per pixel at 60fps. Pass 1 builds it with a one-cell margin, so a triangle that straddles two blocks is still covered.
- **Filtering** (audit T5). Textures get mipmapping for free; procedural detail doesn't. Noise declares its bandlimit (`@assume(bandlimit: ...)`), and the footprint lets it fade octaves finer than a pixel. Gradients get the same treatment, so highlights don't sparkle.

---

## 8. What the compiler accepts and rejects

**Allowed:** a per-frame value that only feeds arithmetic.

```wrela
fn draw_wobbly(g: GrazerSim, frame: FrameInfo, gpu: mut Gpu) {
    let f = grazer_field(g).displace(fbm(freq: 10 / m), amp: sin(frame.time) * 2cm)
    gpu.dispatch(cull_blocks, (f, ...))
    // ok: `amp` only feeds arithmetic, so it's data (§5): a uniform updated each frame
}
```

**Values are data, wherever they come from** (D-070).

**Rejected:** choosing between two *types* at runtime.

```wrela
fn draw_shedding(g: GrazerSim, frame: FrameInfo, gpu: mut Gpu) {
    let horns = if frame.time > 10s { horns_long() } else { horns_short() }
    // error: `if` and `else` have different types
    //   note: a field's structure is its type (D-070): these are two different shapes
    //   help: to choose at runtime, use an enum: `Either::Left(horns_long())` /
    //         `Either::Right(horns_short())`. Both shapes compile into one pipeline with a
    //         uniform branch. For shapes built at runtime from an open-ended space, use
    //         `stage::interpret` (D-029)
}
```

This is an ordinary type error. The compiler doesn't know what a frame is; `frame.time` is just a runtime value.

The others are ordinary errors:

```wrela
@compute(64)
fn bad_cull<F: Surface>(field: F, names: Vec<String>, id: GlobalId) { ... }
// error: `Vec<String>` allocates; GPU entry points can't take it (D-072)

@compute(64)
fn bad_scatter(out: mut [u32], id: GlobalId) { out[id.x] = 1 }
// error: a kernel's `mut` parameter is shared by every invocation; use `Slots<u32>`,
//        `Append<T>` or atomics (D-084)

pub fn cheating_realize<C: Blend>(creature: Creature<C>, gpu: mut Gpu) -> SkinnedMesh {
    creature.physique.mass *= 2      // error: `creature` is borrowed, not `mut` (ordinary mutability)
    ...
}
```

---

## 9. Layer audit

Every construct in this sketch, and who owns it (D-050):

| Construct | Owner |
|---|---|
| `@compute`, `@vertex`, `@fragment`, `GlobalId`, `ClipPosition`, `Flat<T>` | Language: GPU execution target |
| Generics and monomorphization (structure is types), values as uniforms | Language (D-070) |
| `interval`, `gradient`, `fwidth`, opaque `LiveMask` pruning | Language: derived interpretations and GPU target |
| Units, struct layout, struct field defaults, closures on the GPU | Language |
| `Gpu`, `dispatch`, `Append<T>`, `AtomicMap<T>`, `Slots<T>`, `GpuData`, `stage::interpret`, storage | Stdlib |
| `Field`, `Surface`, `Channels`, `Blend` | Stdlib: plain library code (D-056) |
| `Creature`, `CreatureLook`, `Mesh`, `Skin`, `Parts`, `PartMask`, `parts_near`, `with_parts`, `Qef`, `realize_mesh`, `Palette`, `Lights` | Engine |

The compiler appears in the first four rows only.

---

## 10. Questions

> **Q13. Where does the schedule vocabulary live?**
> *Resolved by D-050 and D-053.* Neither option survived. Look settings are plain engine data. A general Halide-style `schedule` construct for any function remains possible as future sugar.

> **Q14. What triggers a new pipeline?**
> *Later redrawn by D-070: structure is types, and `@specialize` was removed. Kept for history.*
> - **A. Specialize on everything known:** fastest per evaluation, but one pipeline per individual means compile hitches.
> - **B. Specialize on structure only:** shape, types, and control-flow values. Everything else becomes uniforms, including values that change every frame (§8).
> - **C. B plus `@specialize(values)`** for hot cases with few variants. A runtime pipeline budget falls back to B when it's exceeded.
>
> **Recommend C, with B as the default.**
> **Cost:** less constant folding on data values, which is a small loss. It also needs a "how many pipelines, and why" query.

> **Q15. Pruning: derived by the compiler or provided by the library?**
> *Refined by D-080: compiler masks are opaque, and creature parts are an engine concept. Kept for history.*
> - **A. Derived** from interval analysis of any pure function: `prune(bounds) -> LiveMask` and `with_live(mask)`.
> - **B. Library-only,** for known combinators like `union` and `smooth_union`.
>
> **Recommend A.** It's consistent with D-050 and D-056: the compiler knows math, not fields. Fields people write themselves, and learned functions, benefit too.
> **Cost:** compiler complexity. Masks are also bounded: WGSL has no u64, so masks are arrays of u32, and deep trees need hierarchical masks.

> **Q16. How are GPU builtins written?**
> - **A. WGSL-style attributes:** `@builtin(global_invocation_id) id: vec3<u32>`.
> - **B. Typed builtins:** `id: GlobalId`, `clip: ClipPosition`, and `Flat<T>` for values that aren't interpolated.
>
> **Recommend B.** Types carry the meaning, there's less attribute noise, and errors read better.
> **Cost:** it diverges from WGSL, which agents already know. The docs need a mapping table.

> **Q17. Closures and iterators in GPU code?**
> - **A. Forbid them;** loops only.
> - **B. Allow them when statically resolved:** monomorphized, inlined, and fixed-size iterators unrolled. Dynamic dispatch stays the forbidden effect (D-010).
>
> **Recommend B.**
> **Cost:** inlining increases code size and register pressure. Diagnostics need to flag blowups.

> **Q18. Default values on struct fields?**
> - **A. Yes:** `pub eval: Eval = Eval::Specialized`, with defaults evaluable at compile time.
> - **B. No:** use constructors or builders.
>
> **Recommend A.** Engine configuration relies on it, and Swift and Kotlin have it.
> **Cost:** one more feature.

> **Q19. Can the compiler choose lossy GPU layouts?**
> - **A. Yes:** the compiler compresses automatically, for example octahedral normals or f16.
> - **B. Layout is automatic but lossless.** Lossy encodings (`Unorm8`, `Oct16`, `f16`) are explicit types.
>
> **Recommend B.** Agents can't accidentally lose precision.
> **Cost:** compression is opt-in by hand. A lint can suggest it.

---

## Not covered yet

- `emit_quads` itself
- Adaptive refinement for detail finer than the cells
- Far-field bricks
- Distance-volume shadows
- LOD transitions
- The frame graph and pass ordering
- The platform layer's command encoding (D-017)
- The OPFS cache format
