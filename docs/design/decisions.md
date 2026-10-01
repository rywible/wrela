# wrela: decisions log

Each entry has a stable ID so other docs can cite it. Statuses:
- **Accepted:** agreed.
- **Proposed:** recommended, but not yet explicitly agreed.
- **Open:** undecided.

To change a decision, add a new entry that supersedes it. Don't rewrite history.

---

## Content and scope

### D-001 · Fields are the source of truth; how they're realized is a choice
**Status:** Accepted (2026-10-01)

Content is authored as fields. Realization is chosen per object, per level of detail and per use. The options include triangles extracted on the client, SDF bricks, distance fields, points and impostors.
- **Close-up visibility:** rasterized triangles.
- **Distant detail:** bricks or impostors.
- **Shadows, AO, GI, collision:** distance fields.

**Why:** rasterization is what GPUs and WebGPU do best. Committing to one representation for rendering would bet everything on sphere-tracing inside a constrained browser.

**Known problems to solve:**
- sharp features (dual contouring and its variants)
- chunk seams between levels of detail (Transvoxel-style)
- adaptive extraction density
- materials without UVs (materials are fields too)
- re-extracting locally after edits

### D-002 · Fields are multi-channel
**Status:** Accepted (2026-10-01)

A field returns distance plus channels: albedo, roughness, mass density, acoustic absorption, fracture strength, and so on. Composition is defined over all channels:
- **Union:** takes the channels of the winning surface.
- **Smooth union:** blends channels with the same weight it uses for distance.

Channels are either **continuous**, which can be blended, or **categorical**, like material IDs, which are picked from the winner and never averaged.

**Why:** one representation drives rendering, physics (mass, center of mass and inertia are integrals of density), acoustics and procedural sound.

**Consequence:** the compiler slices fields per consumer and drops the channels that consumer doesn't use. Opaque assets can't do this.

### D-003 · First game: creatures and animation required, human faces out
**Status:** Accepted (2026-10-01)

High-quality creatures and animation are required. Realistic human faces are out of scope for the first game.

### D-004 · Audio is mostly procedural; music is prebaked
**Status:** Accepted (2026-10-01)

| Sound | Approach |
|---|---|
| Impacts and contacts | Modal synthesis from the field's shape and material |
| Wind, fire, water, machines, footsteps | Procedural |
| Creature vocalizations | The hard case: a tiny neural synth (DDSP-style) or prebaked |
| Music and selected effects | Prebaked (Opus), streamed after the first frame |

---

## Language and compiler

### D-005 · A new language, not an embedded DSL
**Status:** Accepted (2026-10-01)

### D-006 · Architecture layers
**Status:** Accepted (2026-10-01)

1. Compiler + stdlib
2. Engine, written in wrela
3. Studio, for agent-native authoring
4. Games

Platform hosts (D-016) sit below layer 1.

### D-007 · The compiler is written in Rust
**Status:** Accepted (2026-10-01)

**Why:**
- naga validates WGSL, wasm-encoder emits WASM, and wgpu plus wasmtime give native headless runs.
- Rust compiles to WASM, so the specializing back end can ship in the browser.

### D-008 · The compiler has two tiers; the IR is the distribution format
**Status:** Accepted (2026-10-01)

- **Front end (ahead of time):** parses, typechecks and optimizes, then emits a compact, versioned IR.
- **Back end (in the browser):** specializes the IR against actual game state and emits WASM and WGSL. It's written in Rust and compiled to WASM.

**Why:** thesis 4 says the compiler uses game semantics *at runtime*. Shipping IR is "ship the recipe, cook on device."

**Consequence:** the back end has to be small and fast. IR compatibility is a release-engineering concern from the first release (D-019).

### D-009 · CPU and GPU code share one source
**Status:** Accepted (2026-10-01)

One language compiles to WASM for the CPU and WGSL for the GPU. The compiler lays out data that crosses between them, so nobody pads structs by hand.

### D-010 · Effect system: explicit entry points, inferred safety
**Status:** Accepted in direction (2026-10-01). Syntax settled by D-035; public-boundary rule by D-030. Sim/present contexts replaced by `@deterministic` (D-052).

- **Entry points are explicit:** `@compute`, `@vertex`, `@fragment`, `@audio`, plus the `sim` and `present` contexts (D-015).
- **Ordinary functions don't need annotations.** The compiler checks them transitively from each entry point. Errors show the call chain, for example: `extract → foo → bar allocates at line 42`.
- **An optional assertion** (for example `@gpu`) lets library authors promise a property, so a violation errors where the function is defined.

**Effects tracked:** allocation, IO, unbounded recursion, dynamic dispatch, nondeterminism.

**Why:** "pure" answers four questions at once:
- Can it run on the GPU?
- Can it run at compile time?
- Can the compiler derive interval and gradient versions of it?
- Can it be cached?

Function coloring like CUDA's `__host__ __device__` doesn't scale to shared math.

### D-011 · Binding times are first-class
**Status:** Accepted (2026-10-01). Stages revised by D-051.

Values are known at one of four stages: compile time, load time, per frame, or per pixel/sample. Baking is partial evaluation, and edits become exact cache invalidation. Precedents are Terra, Zig `comptime` and MetaOCaml.

### D-012 · Field kinds are lang items
**Status:** Accepted in direction (2026-10-01). Lang-item part superseded by D-056.

**Field kinds** include exact SDF, SDF bound, density and attribute. They're *defined* in the stdlib, so agents can read them and they're cheap to change. The compiler *knows* about them, the way Rust knows about `Copy`, and uses their invariants for realization, LOD, pruning and diagnostics. Example diagnostic: "this op needs an exact SDF, but `smooth_union` produced a bound."

**Derived interpretations** of any pure numeric function are general compiler features, not field-specific:
- an interval version, for pruning
- a gradient, for normals
- a Lipschitz bound, for sphere tracing

Learned controllers, materials and physics get them too.

### D-013 · Algorithm and schedule are separate
**Status:** Accepted in direction (2026-10-01). Superseded by D-053.

The algorithm is *what* to compute. The schedule is *how* it's realized: resolution, representation, caching, when to bake, workgroup sizes. Changing the schedule never changes meaning.
- **Defaults:** the compiler chooses schedules automatically. Explicit schedules are an escape hatch.
- **Render schedules** declare error tolerances, and those tolerances also drive automatic LOD.
- **Sim-facing computation** must be bit-exact (D-015). That means a schedule can only describe presentation. Anything the sim reads belongs to the algorithm (see sketch 01). *Refined by D-033.*

### D-014 · No tracing GC in the core
**Status:** Accepted (2026-10-01). Snapshot mechanism refined by D-065 and D-066.

Memory is value types, arenas, frame allocators and generational handles (indices, not pointers).

**Why:** predictable frames. It also makes sim state relocatable, so a snapshot is a memcpy (D-015).

### D-015 · The language must support multiplayer: determinism
**Status:** Accepted (2026-10-01). Item 1 revised by D-052.

1. **A sim/presentation split, enforced by effects.**
   - `sim` code can't read the wall clock, ambient randomness, GPU results or presentation state.
   - `present` code can read sim state but can't write it.
2. **Strict floats in sim code.**
   - no reassociation and no implicit FMA contraction
   - transcendentals implemented in the stdlib and compiled to WASM, never the host's `Math.*`
   - NaNs canonicalized wherever state is hashed
   - no relaxed SIMD

   Presentation code may use fast floats.
3. **Cheap snapshot and restore** of the sim arena, for rollback.
4. **Serialization, delta compression and per-tick state hashes**, all derived by the compiler.
5. **Everything else deterministic:**
   - fixed tick
   - explicit RNG state
   - ordered iteration, with no address-keyed or randomly seeded hashing
   - a fixed merge order for parallel jobs
   - bounded recursion in sim code (stack limits differ between engines)

**Consequences:**
- Runtime specialization of sim code is limited to bit-exact transformations. Constant folding is fine; reassociation isn't.
- Gameplay physics runs on the CPU. The GPU never feeds the sim.
- Field edits are sim state, stored as an edit log.
- Physics-relevant cooking happens on the CPU, deterministically.
- **Testing:** CI replays the same inputs on Chrome, Firefox, Safari and native, and compares per-tick hashes.

**Bonus:** deterministic replays are an agent-native feature on their own. They give reproducible bug reports, replay-based tests and bisection.

---

## Platform and runtime

### D-016 · Platform hosts: thin, written in existing languages
**Status:** Accepted (2026-10-01)

The platform layer is the only boundary between compiled wrela code and the outside world. There are two hosts:

| Host | Language | Covers |
|---|---|---|
| Browser | TypeScript | WebGPU, input, audio worklet, OPFS, fetch, `WebAssembly.compile`, service worker |
| Native | Rust | wasmtime + wgpu, for headless agent runs, CI and dedicated servers |

**Rules:**
- **Mechanism in the hosts, policy in wrela.** For example, a host exposes "read/write bytes at a path." Save namespacing, versioning and migration are written in wrela.
- **The interface is defined once.** Bindings for both hosts are generated from it, so the hosts can't drift apart.

**Why not wrela:** the browser host must be JS, because Web APIs are only reachable from JS. The native host is what *runs* wrela, so writing it in wrela would be a bootstrapping problem.

**What is written in wrela:** the engine, games, the console shell UI and save-system policy.

**Online services** (accounts, cloud-save storage, signaling, matchmaking) aren't wrela either. They're Rust or managed infrastructure, because they gain nothing from wrela's semantics. Game servers are the exception: the native host runs the same compiled sim WASM.

### D-017 · Browser runtime shape
**Status:** Accepted (2026-10-01)

- **The game runs in a worker** with OffscreenCanvas. The main thread only forwards input.
- **WASM↔JS crossings are batched.** Commands are recorded into WASM memory and the host decodes them in bulk.
- **Cross-origin isolated (COOP/COEP) from day one.** This is required for SharedArrayBuffer, threads and sharing memory with the audio worklet.

### D-018 · The console: games.wrela.dev
**Status:** Accepted (2026-10-01)

- **It's a PWA.** The runtime is cached by a service worker, like firmware. Each game is just its IR.
- **Games never touch OPFS directly.** The console gives each game a namespaced save API.
- **OPFS is best-effort storage,** so call `navigator.storage.persist()`, encourage PWA install, and ship save export/import early.
- **Later:** sign-in, cloud saves and multiplayer, all through the same platform boundary.
- **The dashboard is a wrela program.** That dogfoods UI and field-based text early.

### D-019 · IR is versioned; cooked caches have keys
**Status:** Accepted (2026-10-01)

- **IR:** carries a version number and a compatibility policy from the first release.
- **Cooked caches in OPFS:** keyed by game version, runtime version and GPU adapter.

### D-020 · Two size budgets
**Status:** Accepted (2026-10-01). Numbers set by D-041.

- **Time-to-play:** what must arrive before the first frame (a few MB).
- **Total:** everything else, streamed in behind the player.

Music lives in the total budget.

---

## Authoring and animation

### D-021 · Agent-native starts at layer 1
**Status:** Accepted (2026-10-01)

**Compiler deliverables:**
- machine-readable diagnostics
- semantic queries ("what depends on this?", "what does this cost?")
- fast incremental compiles
- hot reload
- deterministic replays

**Language design for agents:**
- familiar syntax from the Rust/TS/Swift family
- a spec small enough to fit in an agent's context window
- excellent error messages
- lots of examples

**Why:** a new language has no training data, so by default it works against agents.

### D-022 · Units of measure in the type system
**Status:** Accepted (2026-10-01)

For example, kg/m³ vs. kg vs. Hz vs. m. It's cheap to check and catches exactly the mistakes agents make.

### D-023 · Creature deformation and motion
**Status:** Accepted (2026-10-01)

**Deformation:**
- **Default:** skin the extracted rest-pose mesh, with skin weights derived from which bone each field part belongs to.
- **Schedule option:** each bone has its own rigid field, smooth-unioned per frame (the Dreams/Claybook approach).
- **Avoid:** warping space through the inverse of the skeleton. It breaks distance bounds.

**Motion (layered):**
- **Base locomotion:** a small learned controller (MANN, Learned Motion Matching, DeepPhase) or a procedural gait.
- **Procedural layers:** IK foot planting, look-at, springs for secondary motion, ragdoll blending.

**Agent loop:** measurable motion-quality metrics. These include foot skating, ground penetration, jerk, balance and contact consistency. A human judges the "feels alive" part from video.

### D-024 · Design the language from the programs we want to write
**Status:** Accepted (2026-10-01)

Write sketches of real workloads before compiler code, and derive the language from them. Sketches live in [sketches/](sketches/).

---

## Language design from sketch 01

These resolve sketch 01's questions Q1–Q12, in order.

### D-025 · Units are library constants
**Status:** Accepted (2026-10-01)

- **Units are ordinary constants:** `m`, `kg`, `s`, `rad`, `deg` and so on.
- **Compound units are plain arithmetic:** `1050 * kg/m^3`.
- **A single unit can be written as a suffix:** `15cm` is shorthand for `15 * cm`.
- **Angle brackets appear only in types:** `f32<kg/m^3>`.
- **Angles are units,** so mixing up `rad` and `deg` is a type error.

Units erase at compile time, so they cost nothing at runtime or on the GPU.

**Constraint:** unit names must never collide with numeric syntax. No unit may be named `e`, and there are no type suffixes like `1.0f32`.

### D-026 · Channel structs are plain structs that opt in to `Blend`
**Status:** Accepted (2026-10-01)

Example: `struct Tissue: Blend { ... }`.

Blend policy lives in member types, not attributes:

| Member type | How it blends |
|---|---|
| `Color` | in linear space |
| `UnitVec3` | renormalized afterwards |
| `Cat<T>` | takes the winner's value |
| `f32` | linearly |

The struct's blending is the composition of its members'.

**Requires:** traits/interfaces early in the language.

### D-027 · Lipschitz bounds are compiler-derived
**Status:** Accepted (2026-10-01). Generalized by D-057.

- **Derived:** the compiler computes the bound (D-012) and exposes it through a query.
- **Optional assertion:** `@lipschitz(max: 1.5)` errors if the bound is exceeded.
- **Escape hatch:** `.assume_lipschitz(1.2)` for when the derived bound is too loose. Debug builds validate assumed bounds by sampling gradients.

### D-028 · Field combinators are methods
**Status:** Accepted (2026-10-01)

- **Binary and unary ops:** `a.smooth_union(b, k: 15cm)`.
- **n-ary ops:** collection methods, as in `[a, b, c].smooth_union(k: 6cm)`.
- **No operator overloading on fields.** Vectors and units still get operators.

**Why:** one obvious way to write things, with smoothing parameters and kind changes always visible.

### D-029 · Specialized vs. interpreted evaluation is a schedule choice
**Status:** Accepted (2026-10-01). Mechanism revised by D-053.

- **The default is specialized.**
- **`eval: interpreted` opts in** to GPU tape evaluation with interval pruning (Keeter's MPR approach). This is for fields that change at runtime: destruction, sculpting and edit logs.
- **When specialization is impossible,** the error appears at the entry point with a trace showing which value isn't known early enough. It suggests either fixing binding times or switching to interpreted.

**Cost:** two evaluation back ends that must agree.

### D-030 · Infer inside a package; explicit at public boundaries
**Status:** Accepted (2026-10-01)

- **Inside a package,** effects and binding times are inferred. Annotations only assert.
- **Exported functions** state their effects and binding times. `wrela fix` writes these annotations, and queries show what was inferred.
- **Public higher-order functions** default to inheriting effects from their closure arguments. For example, `map` is GPU-safe whenever its closure is.

**Why:** light code for agents inside a package, and contracts that can't silently change across packages, which avoids semver hazards.

### D-031 · Bone access: checked strings plus typed handles
**Status:** Accepted (2026-10-01)

Bones are looked up as `s["chest"]`, checked at compile time when the skeleton is known then. The lookup returns a typed `Bone` handle to pass around afterwards. Generated members (`s.chest`) wait until compile-time type generation exists for other reasons.

### D-032 · Schedules are separate constructs
**Status:** Accepted (2026-10-01). Superseded by D-053.

A schedule lives in the same file as its definition by convention. Quality and device profiles can override it from elsewhere, with explicit precedence. A query reports the *effective* schedule for a definition on a given device.

### D-033 · Rules for sim-facing work and schedules
**Status:** Accepted (2026-10-01). Refines D-013. Revised by D-052 and D-053.

- **Sim-facing derivations are never schedulable.** Mass integration grids, collision-shape fitting and similar are approximations, and the approximation *is* the gameplay. They belong to the definition.
- **Sim execution may be scheduled,** but only with transformations the compiler can prove bit-exact: data layout, parallelism with a fixed reduction order, and acceleration structures for exact queries.
- **Sim schedules are global to a game version,** never per-device.

### D-034 · The sim/presentation line for animation, with sim tiers
**Status:** Accepted (2026-10-01)

- **In sim by default:** root motion, gait phase, foot contacts and the hitbox pose.
- **In presentation:** secondary motion and cosmetic layers.
- **Sim tiers:** creatures that matter get the full sim pose. Ambient creatures keep only root and phase in the sim, and compute their pose in presentation. The tier is itself sim state, so it stays deterministic.
- **Revisit if the first multiplayer game is co-op PvE.** Server-authoritative creatures with client interpolation would remove the rollback multiplier entirely.

### D-035 · Function properties are attributes
**Status:** Accepted (2026-10-01). Attribute set revised by D-051 and D-052.

Binding times, contexts and entry points all use one form: `@comptime`, `@load`, `@sim`, `@present`, `@compute(64)`, `@vertex`, `@fragment`, `@audio`, `@gpu`, `@lipschitz(...)`.

**Why:**
- one mechanism, and parameters come naturally
- new targets don't need grammar changes
- it matches WGSL, which agents already know

**Note:** these attributes change a function's type; they aren't metadata. Docs and diagnostics must say so.

### D-036 · Learned weights ship as embedded data
**Status:** Accepted (2026-10-01)

- **The network architecture is wrela code,** so the compiler can fuse, quantize and specialize it.
- **Weights are versioned build artifacts** with provenance: a hash of the training script and data.
- **They're quantized** (int8 or fp16) and count against the size budgets (D-020).
- **Reference motion never ships.** Only the trained weights do.
- **Training lives outside the language for now** (likely PyTorch). Training in wrela, using D-012's derived gradients, is a long-term option.

---

## Decided on delegation

The user delegated these open items to Claude on 2026-10-01. Push back on any of them freely.

### D-037 · `@` attributes are a closed set that's part of types
**Status:** Accepted (delegated, 2026-10-01). Refines D-035. Attribute set revised by D-051 and D-052.

- **The language defines every `@` attribute.** Each one is part of the type of the function or parameter it decorates. If user-defined metadata is ever needed, it gets a different syntax, so `@` always means "semantic."
- **Binding-time attributes also apply to parameters,** as in `@load field: &F`. D-030 needs this for public boundaries, and kernels need it to specialize on their arguments (sketch 02).
- **Attributes may appear in function types,** as in `@sim fn(&mut World)`. Swift's `@MainActor` and `@Sendable` are the precedent.

**Why attributes rather than keywords:** about ten names would otherwise be reserved, and `load` is far too common an identifier to steal. `@` is already a familiar *shape* to agents from Python, Swift, TS and WGSL, even when the names are new.

### D-038 · Newlines end statements
**Status:** Accepted (delegated, 2026-10-01)

A newline ends a statement unless one of these holds:
- the next line starts with `.` or a binary operator, which keeps leading-dot combinator chains working (D-028)
- the newline is inside open brackets

A line that starts with `(` or `[` always begins a new statement. This avoids JavaScript's semicolon-insertion trap.

`;` can separate statements on one line, and the formatter normalizes everything.

### D-039 · Named arguments are optional, Kotlin-style
**Status:** Accepted (delegated, 2026-10-01)

- **Any argument may be passed by name.**
- **Positional arguments come first.** Once an argument is named, the rest must be named too.
- **Parameters may have defaults.**
- **A lint suggests names for bare literal arguments,** like `6cm` or `true`.

**Why:** call sites read clearly without Swift's mandatory labels, and agents get one consistent rule.

### D-040 · The file extension is `.wrela`
**Status:** Accepted (delegated, 2026-10-01)

It's unambiguous, easy to grep and easy for GitHub to detect. There's precedent in `.zig`, `.odin` and `.swift`. The short alternative `.wrl` is already taken by VRML.

### D-041 · Size and time budgets
**Status:** Accepted (delegated, 2026-10-01). Sets the numbers for D-020. These are targets to revisit once we have measurements.

| Budget | Target (compressed) |
|---|---|
| Runtime: host + specializing back end + engine, cached once as "firmware" | ≤ 4 MB |
| A game's time-to-play payload: the IR and content needed for the first playable frame | ≤ 4 MB |
| Cold start | ≤ 8 MB, playable within **5 s** on a mid-range laptop over 50 Mbps. That includes download, WASM compile, specialization and cooking. |
| Warm start: runtime and cooked caches present | playable within **2 s** |
| Total per game, including streamed music | ≤ 32 MB |

CI measures these and fails the build when a budget is exceeded.

### D-042 · First multiplayer target: server-authoritative
**Status:** Accepted (delegated, 2026-10-01). Assumes the first game is creature-heavy PvE (D-003); revisit if it turns out to be PvP.

- **Dedicated servers** run the sim WASM in the native host (D-016).
- **Locally controlled entities** use client-side prediction with rollback.
- **Creatures** are simulated on the server and interpolated on clients. This removes the rollback multiplier from D-034.
- **Full determinism (D-015) is still required.** It's needed for prediction and reconciliation, for replays and agent testing, and to keep rollback-based games possible.

---

## From sketch 02

### D-043 · The engine defines the schedule vocabulary
**Status:** Proposed. **Withdrawn:** superseded by D-053.

- **A schedule is a struct with defaults,** attached to a type through the `Schedulable` trait. A `schedule x { ... }` block is sugar for that struct with some fields overridden, built at compile time.
- **Realizers implement the `Realizer` trait.**
- **The compiler enforces three things:**
  - schedule values are known at compile or load time
  - realizers are `@present`
  - realizers declare error bounds
- **Revises D-013:** defaults come from the engine type, not the compiler. Per-device tuning is a runtime policy.

### D-044 · Specialize on structure; values become uniforms
**Status:** Accepted (2026-10-01). Override syntax revised by D-053.

- **Structure is what gets compiled into code:** expression shape, types, and any value that steers control flow. Binding-time analysis tells them apart.
- **Values that only feed arithmetic become uniforms.**
- **`@load` on a parameter constrains structure, not data.** A per-frame value that only feeds arithmetic is allowed, and it becomes a uniform updated each frame. A per-frame value that changes structure is an error, unless it's hoisted into a choice between known variants or uses `eval: Interpreted` (D-029).
- **`Specialize::Values` is a schedule override.** A runtime pipeline budget falls back to structure-only specialization when it's exceeded.
- **A query reports** how many pipelines a definition needs, and why.

### D-045 · Pruning is a derived interpretation
**Status:** Accepted (2026-10-01). Extends D-012.

- `f.prune(bounds) -> LiveMask` and `f.with_live(mask)` are derived from interval analysis of ordinary code, for any pure function. The compiler doesn't need to know what a smooth union is.
- WGSL has no u64, so masks are arrays of u32. Deep trees need hierarchical masks.

### D-046 · GPU builtins are typed
**Status:** Accepted (2026-10-01)

Builtins are types: `GlobalId`, `WorkgroupId`, `LocalId`, `ClipPosition`, and `Flat<T>` for values that aren't interpolated. They replace WGSL's `@builtin(...)` attributes, and the docs include a mapping table to WGSL.

### D-047 · Closures and iterators are allowed in GPU code when statically resolved
**Status:** Accepted (2026-10-01)

They're monomorphized and inlined, and fixed-size iterators are unrolled. Dynamic dispatch stays the forbidden effect (D-010). Diagnostics flag unrolling or inlining blowups.

### D-048 · Struct fields can have defaults
**Status:** Accepted (2026-10-01)

For example, `pub eval: Eval = Eval::Specialized`. Defaults must be evaluable at compile time.

### D-049 · GPU struct layout is automatic but lossless
**Status:** Accepted (2026-10-01)

The compiler chooses padding and field order. Any lossy encoding (`Unorm8`, `Oct16` normals, `f16`) must be an explicit type. A lint can suggest compression.

---

## The compiler knows nothing about the engine

### D-050 · Principle: the engine gets no special treatment
**Status:** Accepted (user, 2026-10-01)

The engine is built only on the language and its stdlib. The compiler never knows about the engine:
- no keywords, attributes or lang items for engine concepts
- no compiler rules phrased in engine terms

The compiler may know about exactly three things:
1. **The language itself.**
2. **Stdlib lang items** (D-012).
3. **Execution targets:** WASM on the main thread, workers, the audio worklet, and WGSL on the GPU.

**Sugar is fine** as long as it's general: it must mean the same thing for any program, game or not. For example, struct field defaults (D-048) make engine configuration read cleanly, and a future Halide-style `schedule` that describes how *any* function is evaluated would be legitimate. A construct whose vocabulary is meshes, skins or LOD isn't.

**Test for any proposal:** would this feature make sense in a wrela program that isn't a game? If not, it belongs in the engine.

D-051 to D-055 apply this principle to earlier decisions.

### D-051 · Binding times are `@comptime` and `@specialize`
**Status:** Accepted (follows from D-050; specifics chosen by Claude, so push back freely). Revises D-011.

D-011's four stages were compile time, load, per frame and per pixel. "Load" and "frame" are engine concepts, so the language's stages become:

| Stage | Meaning |
|---|---|
| `@comptime` | Known when the ahead-of-time compiler runs. |
| `@specialize` (parameters) | The client-side specializer compiles this argument's *structure* into the code it generates (D-044). Data that only feeds arithmetic still becomes uniforms. `@specialize(values)` bakes data in too. |
| Runtime | Ordinary values. On the GPU, the target's own *uniform vs. varying* distinction applies, which WGSL already analyzes. |

The *engine* decides when specialization happens: at level load, when a creature spawns, and so on. The language only defines what it means.

**The function-level `@load` assertion goes away.** A pure function's results can be passed to `@specialize` parameters automatically. Public functions state their effects (D-030), so purity is visible at the boundary.

### D-052 · `@deterministic` replaces `@sim`; `@present` is removed
**Status:** Accepted (follows from D-050; specifics chosen by Claude, so push back freely). Revises D-015 item 1 and D-033.

**What the language provides: the `@deterministic` effect constraint.** Inside it:
- floats are strict
- transcendentals come from the stdlib
- these are all forbidden: the clock, ambient randomness, GPU readback, unordered iteration, relaxed SIMD, and anything that depends on memory addresses

**The engine builds the sim/presentation split as a pattern on top of general features:**
- **Sim step functions are `@deterministic`** and are the only ones that receive `&mut` sim state.
- **Presentation code is ordinary code.** It receives sim state through `&`, so writing to it is an ordinary mutability error.
- **The engine defines a `SimState` marker trait** as an auto trait (D-054). Presentation types opt out, so they can't be stored in sim state. The engine writes the error message itself (D-055).
- **GPU results can't reach the sim.** Reading them requires GPU readback, which is nondeterministic, so it's forbidden inside `@deterministic` code.

### D-053 · No `schedule` construct for now; engine realization settings are plain data
**Status:** Accepted (follows from D-050; specifics chosen by Claude, so push back freely). Supersedes D-013 and D-032, revises D-029 and D-044, and withdraws D-043.

- **How a creature is drawn is engine configuration:** an ordinary struct with defaults, such as `CreatureLook { surface: Mesh { tolerance: 1mm }, ... }`.
  - Per-device quality profiles are engine code.
  - Error bounds are an engine contract, checked by engine debug code.
- **Algorithm/schedule separation survives as an engine design pattern,** not a language construct.
- **Compiler-level evaluation choices use general mechanisms:**
  - structure vs. values: `@specialize` / `@specialize(values)`
  - specialized vs. interpreted: the stdlib's `std::stage` API, for example `stage::interpret(f)`, which produces a tape evaluator with interval pruning (D-029)
- **A general Halide-style `schedule` construct** for *any* function's evaluation (tiling, workgroup size, unrolling, where it runs) stays possible as future sugar under D-050. Add it only if kernels need it.

### D-054 · Auto traits
**Status:** Accepted (2026-10-01)

A marker trait can be declared *auto*. Every type whose fields all implement it gets it automatically, and individual types can opt out. Rust's `Send` and `Sync` are the precedent.

**Why:** this lets libraries enforce structural rules with no compiler knowledge of what the rules are about. The engine's `SimState` (D-052) is the first user.

### D-055 · Library-authored diagnostics
**Status:** Accepted (2026-10-01)

Libraries can attach messages to traits and types, for when a bound isn't satisfied. Rust's `#[diagnostic::on_unimplemented]` is the precedent. The syntax would be `@diagnostic(...)`, a language-defined attribute that any library may use.

**Why:** engine-specific errors with zero engine knowledge in the compiler. For example: "`GrazerLook` is presentation data and can't be stored in sim state; keep it in the creature's look and read sim state from there."

### D-056 · Fields are plain stdlib code
**Status:** Accepted (user, 2026-10-01). Supersedes the lang-item part of D-012.

- **Field kinds** (`Sdf`, `SdfBound`, `Density`, ...) are ordinary stdlib types.
- **Kind rules are ordinary type checking.** Their messages come from library diagnostics (D-055).
- **The compiler knows math, not fields.** Derived interpretations (gradient, interval, pruning, Lipschitz) work on any pure function.

What math alone can't give is supplied two ways:
- **Types** carry what can't be derived at all: whether a field is an exact distance or a bound, and what it represents.
- **Declared facts** (D-057) supply what's derived poorly: Lipschitz constants through singular primitives, and noise ranges.

### D-057 · Declared facts on functions
**Status:** Accepted (2026-10-01). Generalizes D-027.

A pure function may declare properties the compiler can't prove, such as `@lipschitz(1)` or `@range(-1, 1)`.
- **The compiler trusts them** and composes them through callers: the Lipschitz constant of `f∘g` is at most L(f)·L(g), of `min` at most the max of its inputs', and so on.
- **Debug builds spot-check them** by sampling.
- **The stdlib declares facts on its primitives** (`length`, `smooth_min`, noise functions). That's what keeps derived bounds tight.

**Why:** naive composition fails on the most basic SDF. `length(p)` has Lipschitz constant 1, but composing `sqrt` with squares gives an unbounded bound near the origin.

**Possible later:** `@bandlimit` on noise functions, for LOD and antialiasing.

---

## From sketch 03

### D-058 · References are second-class
**Status:** Accepted (2026-10-01). Amended by D-064.

1. **Storage:** references can be passed into functions, but never stored in structs or collections, or captured by escaping closures.
2. **Projections:** a function may return a reference only as a projection of one of its reference parameters. The caller treats it as borrowing that argument. With several reference parameters, it borrows all of them, conservatively.
3. **Exclusivity:** checked locally, within a function. A live `&mut` excludes all other access to the same place, and disjoint fields don't conflict.
4. **Long-lived relationships** use handles into arenas.
5. **No lifetime annotations exist.**

**Why:** it fits value types, arenas and handles (D-014), and keeps snapshots trivial (D-015). It's also the most agent-friendly safe model: Hylo's approach, or Swift's `inout`.

**Cost:** views and borrowing iterators have to be expressed as projections, closures, or indices and handles.

### D-059 · Moves by default; `Copy` for small plain types; explicit `.copy()` otherwise
**Status:** Accepted (2026-10-01). Amended by D-064.

This is Rust's model. Big copies, like snapshots, are visible in the code, and move errors suggest `.copy()`.

### D-060 · Structural implementations through compile-time reflection
**Status:** Accepted (2026-10-01)

A trait can provide a structural default, written in ordinary wrela, that runs at compile time over a type's fields. That's how `StateHash`, `Serialize`, `Copy` and engine traits get implemented without anyone writing them by hand.

- **No procedural macros.**
- **No compiler knowledge of specific traits.** This realizes D-015 item 4 under D-050.
- **Diagnostics point at the user's type,** not the generated code.

### D-061 · Errors are `Result` with `?`; bugs panic
**Status:** Accepted (2026-10-01)

- **In `@deterministic` code, a panic is a deterministic trap.** The engine can turn it into a repro bundle: the last snapshot plus the inputs since.
- **Panics in GPU code are still open.** WGSL clamps out-of-bounds access instead of trapping, so single-source code can behave differently on CPU and GPU.

### D-062 · Deterministic parallelism is data-parallel only
**Status:** Accepted (2026-10-01)

- **Stdlib combinators:** `par_each_mut`, `par_map_reduce`, and so on.
- **Safety comes from general rules:** exclusivity proves disjointness, and a stdlib auto trait, `Shareable`, covers captured data (D-054).
- **Reductions combine in a fixed tree order.**
- **Per-entity RNG streams** keep results independent of thread scheduling.

### D-063 · Engine: systems may read last tick's world
**Status:** Accepted (2026-10-01) (engine design, not language)

`Timeline` already keeps the previous tick's world, so the engine can offer `tick(prev: &W, now: &mut W)`:
- systems read `prev` and write `now`
- most borrow puzzles disappear
- results don't depend on the order systems run in

The cost is one tick of latency for interactions within a tick. The engine offers both styles, and each system picks.

---

## Memory model

The full rules are in [memory-model.md](memory-model.md).

### D-064 · Parameter modes, explicit transfers, non-escaping types
**Status:** Accepted (user, 2026-10-01). Amends D-058 and D-059. The binding forms and the call-site `take` were chosen by Claude while writing memory-model.md; push back freely.

- **`&T` is not a type.** Parameters have modes: `borrow` (the default), `mut` and `take`.
  - The caller writes `mut x` and `take x` at the call site. Method receivers aren't marked.
  - Projections are returned as `-> borrow T` or `-> mut T`.
- **Every transfer is visible.**
  - Moving out of a named place is written `take`. This amends D-059's implicit moves.
  - Deep copies are `.copy()`, and small `Copy` types copy implicitly.
  - Temporaries, and returning a local, need no marker.
- **Local bindings:**
  - `let` projects a place read-only, or owns a temporary.
  - `var` owns a value mutably.
  - `mut` projects a place mutably.
- **Non-escaping types,** as in Swift's `~Escapable`:
  - The stdlib provides the roots: `Span<T>`, `SpanMut<T>`, borrowing iterators, and closures that capture projections.
  - Any type with a non-escaping part is non-escaping, and follows the projection rules.
  - This is what lets views and iterator chains work without lifetimes.
- **Closures are non-escaping by default.** `@escaping` closures capture only owned values and handles.

**Why:** `&T` is a false friend. Agents trained on Rust would expect to store it and annotate lifetimes. With modes there's no reference type to misuse, and all mutation, moves and copies show up where they happen.

### D-065 · Regions and relocatable data
**Status:** Accepted (user, 2026-10-01)

- **`Region<T>`** is a chunked memory area holding one root value. Its containers (`Arena`, `List`, `Text`) store region-relative offsets instead of pointers.
- **Two stdlib auto traits (D-054) describe byte-level data:**
  - `Plain`: no pointers and no offsets, with zeroed padding. Used for GPU buffers, network messages and handles.
  - `Relocatable`: `Plain` data plus region containers. Used for region roots, sim state and saves.
- **The engine's `SimState` requires `Relocatable`.**
- **Scratch arenas hand out non-escaping containers,** so exclusivity proves nothing outlives a reset.
- The region-context mechanism lives in the stdlib's unsafe core. The compiler doesn't know regions exist (D-050).

### D-066 · Snapshots use copy-on-first-write chunks
**Status:** Accepted (user, 2026-10-01)

- **`region.checkpoint`, `rewind` and `keyframe`** are stdlib operations.
- **A chunk's old bytes are saved the first time it's written in an epoch,** so per-tick cost is proportional to the chunks written, not the world's size.
- **Keyframes** (full copies) are taken every few seconds, for repro bundles and saves.

**Soundness rests on D-058 and D-064:** projections can't outlive their call, and checkpoints take `mut` access to the region. Together they guarantee no write lands in the wrong epoch.

---

## Open

- **The first game's concept.** This belongs to the creative director, so it wasn't delegated.
- **Panics and out-of-bounds behavior in GPU code** (D-061).
- **Async and references.** The leaning: projections and non-escaping values can't cross an `await` in a task that outlives its caller, so only structured concurrency is allowed. Decide this with platform IO.
