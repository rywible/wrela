# wrela: the language, as it stands

*Status: current-state description, 2026-10-01 (D-088). This is the one document that describes the whole language. [decisions.md](decisions.md) records how each rule came to be, and [memory-model.md](memory-model.md) holds the full memory rules, which are only summarized here. **The syntax is still imagined:** there's no parser yet, and nothing here has been checked by an implementation. This document is the seed of D-021's small spec.*

## How to read this

- **Every rule cites its decisions** (D-NNN). If this document and the log disagree, the log wins, and this document has a bug.
- **Every feature has a tier** (D-088):
  - **T0, hello field:** needed for the first program that draws a field.
  - **T1, creature milestone:** needed for sketches 01–03.
  - **T2, later.**
- **"Open"** marks something undecided. **"Placeholder"** marks syntax used in the sketches that no decision has settled.
- **The compiler knows nothing about the engine** (D-050). Nothing in this document mentions creatures, meshes, frames or sim state, except in examples. A feature that only makes sense in a game belongs in the engine.

---

## 1. What wrela is

- **One language for CPU and GPU** (D-005, D-009). The same source compiles to WASM for the CPU and to WGSL for the GPU. The compiler lays out data that crosses between them.
- **Compiled ahead of time** by a Rust program on the developer's machine (D-007, D-069). A game ships WASM, WGSL and data. There's no compiler in the browser.
- **Value semantics, no garbage collector** (D-014). Memory is values, arenas, regions and handles. Every copy, move and mutation is visible where it happens (memory-model.md).
- **Generic code is monomorphized** (D-070, D-071). A value's structure is its type, so specialization is ordinary monomorphization and runtime numbers travel as data.
- **The compiler derives interpretations of pure numeric code:** gradients, intervals, Lipschitz bounds and pruning masks (D-012, D-080). Fields are the motivating use, but nothing about it is field-specific.
- **Determinism is a checked property** (`@deterministic`, D-052), with strict floats on the CPU (D-074).
- **Built for agents** (D-021): a familiar Rust/TS/Swift-family syntax, a small spec, machine-readable diagnostics, semantic queries.

### Targets (D-050)

The compiler knows about exactly these execution targets:

| Target | Emitted as | Notes |
|---|---|---|
| CPU, main thread or worker | WASM | Strict floats (D-074) |
| Audio worklet | WASM | `@audio` context (D-072) |
| GPU | WGSL | Compute, vertex and fragment entry points |

---

## 2. Lexical structure and statements

| Rule | Tier | Decisions |
|---|---|---|
| A newline ends a statement, unless it's inside open brackets or the next line starts with `.` | T0 | D-038, D-079 |
| A binary operator that continues a line must *trail* the line; a leading `-` or `\|` starts a new expression | T0 | D-079 |
| `;` may separate statements on one line; the formatter normalizes | T0 | D-038 |
| `//` comments, `///` doc comments | T0 | Placeholder (sketches) |
| Files use the `.wrela` extension | T0 | D-040 |
| Number literals have no type suffixes (no `1.0f32`) | T0 | D-025 |
| A unit can follow a number as a suffix: `15cm` means `15 * cm` | T1 | D-025, D-076 |
| `**` is exponentiation; `^` is XOR | T0 | D-076 |

```wrela
let r = ellipsoid(radii: vec3(0.45m, 0.50m, 0.90m))
    .smooth_union(haunch, k: 15cm)      // a leading `.` continues the expression
    .displace(fbm(freq: 25 / m, octaves: 4), amp: 3mm)

let total = base +                      // a continued line ends with the operator
    extra
```

**Keywords:** the sketches use `fn let var mut take borrow struct enum trait impl for in if else match return pub use const self Self true false unsafe`, plus the usual `while loop break continue`. **Open:** this is an inventory, not a decision.

---

## 3. Items

### Functions (T0)

- **Named arguments are optional, Kotlin-style** (D-039). Positional arguments come first, and once an argument is named, the rest must be named too. Parameters may have defaults. A lint suggests names for bare literals such as `6cm` or `true`.
- **Parameters have modes** (§6): `x: T` (borrow), `x: mut T`, `x: take T`.
- **Properties are attributes** (§9): `@deterministic fn step(...)`.
- **Return-position `Field<K, C>`** (or any trait-shaped type) names one concrete, inferred type, like Rust's `impl Trait` (D-070).

```wrela
fn leg_segment(len: f32<m>, r_top: f32<m>, r_bottom: f32<m> = 6cm) -> Field<Exact, Tissue> {
    round_cone(vec3(), vec3(y: -len), r_top, r_bottom).with(HIDE)
}

leg_segment(45cm, r_top: 9cm)          // positional first, then named
```

### Structs (T0)

- **Fields may have defaults**, which must be evaluable at compile time (D-048). A struct literal may omit defaulted fields.
- **A struct opts in to traits in its declaration:** `struct Tissue: Blend { ... }` (D-026, D-078).
- **`..base`** fills the remaining fields from another value. Placeholder: sketch 01 uses it; no decision covers it.

```wrela
pub struct CreatureLook {
    pub surface: Mesh   = Mesh { tolerance: 2mm },
    pub shadow:  Shadow = Shadow::Map { resolution: 1024 },
}

const GRAZER_LOOK = CreatureLook { surface: Mesh { tolerance: 1mm } }   // shadow keeps its default
```

### Enums (T0)

Enums are sum types with payloads, matched with `match`. They're how structure is chosen at runtime from a known, finite set: every case is compiled, with a uniform branch (D-070).

```wrela
pub enum Edit: SimState + StateHash + Serialize + Copy {
    Dig  { at: vec3<m>, radius: f32<m> },
    Fill { at: vec3<m>, radius: f32<m> },
}
```

### Traits and impls

- **Traits have associated types and default methods** (D-071). T0.
- **Coherence follows Rust's orphan rule:** an `impl` lives in the crate of the trait or of the type (D-071). T0.
- **Structural defaults:** a trait can provide an implementation written in ordinary wrela that runs at compile time over a type's fields (D-060). That's how `Copy`, `Clone`, `StateHash`, `Serialize` and engine traits like `SimState` get implemented without anyone writing them by hand. T1. **Placeholder syntax:** `@comptime default for<T: struct> { ... }` (sketch 03 §5).

### Constants (T0 for literal values; T1 when the initializer must be evaluated)

`const` items are evaluated at compile time (D-073). A `const` whose initializer calls functions needs the compile-time interpreter, which is tier 1 (D-088). Staging work earlier is done by writing a `const`, never by relying on the optimizer (D-072).

```wrela
const HOOF_MODES = modal_modes(hoof())     // an eigenvalue solve, run by the compiler
```

### Modules and packages

`use std::field::{Field, Exact}` imports names, and the stdlib's root is `std::` (D-081). **Open:** modules and packages are deferred to a sketch (D-087). D-030's public-boundary rule depends on them.

---

## 4. Types

| Type | Meaning | Tier | Decisions |
|---|---|---|---|
| `bool`, `i32`, `u32`, `f32` | Scalars, on CPU and GPU | T0 | D-074 |
| `f64`, `i64`, `u64` | CPU only. GPU code is type-checked against what WGSL has. | T0 | D-074 |
| `u8`, `i8`, `u16`, `i16` | CPU; on the GPU only packed in storage | T0 | D-074 |
| `f16` | Explicit lossy type | T1 | D-049 |
| `vec2<U>`, `vec3<U>`, `vec4<U>`, `mat3`, `mat4`, `Quat` | Vectors are generic over a unit (§5); `vec3` alone is unitless | T0 | D-076 |
| `[T; N]` | Fixed-size array | T0 | |
| `[T]`, `mut [T]` | A contiguous run, borrowed or mutable | T0 | memory-model §2 |
| `(A, B)` | Tuple | T0 | |
| `Option<T>` | An ordinary enum; there's no null | T0 | memory-model §1 |
| `Result<T, E>` and `?` | Recoverable errors | T1 | D-061, D-088 |
| `String`, `Text`, `str` | Heap-owned UTF-8, region-resident text, a borrowed run | T1 | D-087 |
| `borrow T`, `mut T` | Projection types: non-escaping. Returning them is T0; using them as type arguments (`Option<borrow T>`) comes with views and iterators in T1. | T0 / T1 | D-064, D-084, D-088 |
| Closures | Non-escaping by default (T0). An `@escaping` closure captures only owned values and handles (T1). | T0 / T1 | D-064 |
| `Handle<T>`, `Arena<T>`, `List<T>`, `Region<T>` | Stdlib containers (§6) | T1 | D-065 |
| `Unorm8`, `Oct16`, … | Lossy encodings are always explicit types | T1 | D-049 |
| `dyn Trait` | CPU only, never in GPU or `@audio` code | T2 | D-071 |

**`&T` doesn't exist** (D-064). There's no reference type to store, so there are no lifetimes.

---

## 5. Units (T1)

Units are part of types and erase at compile time (D-022, D-025, D-076).

- **A unit is a dimension plus a scale.** `cm` is metres scaled by 0.01. Scales convert at compile time; dimensions are checked.
- **Types are written with angle brackets:** `f32<kg/m**3>`, `vec3<m>`, `Transform<m>`.
- **Values are written with arithmetic:** `1050 * kg/m**3`, or a suffix for a single unit, `15cm`.
- **Unit suffixes resolve in their own namespace,** which locals can't shadow. `2m` means metres even if a local is named `m`.
- **Angle is its own dimension.** `sin` takes `f32<rad>`, so mixing degrees and radians is a type error. Arc length is written `r * θ.ratio()`.
- **Units flow through derived gradients:** d(out)/d(in) has unit out/in.
- **Constraint:** no unit may collide with numeric syntax. There's no unit named `e`.

```wrela
let density: f32<kg/m**3> = 1050 * kg/m**3
let wrong = density + 2m        // error: can't add kg/m³ to m
```

---

## 6. Memory (summary of [memory-model.md](memory-model.md))

| Rule | Tier |
|---|---|
| **Everything is a value.** `&T` isn't a type. | T0 |
| **Parameters have modes:** borrow (default), `mut`, `take`. Callers write `mut x` and `take x`. A `mut self` receiver isn't marked; a consuming `take self` method on a named place is (`take b.finish()`). | T0 |
| **Moving out of a named place is written `take`.** Deep copies are `.clone()`; small `Copy` types copy implicitly. Temporaries and returned locals need no marker. | T0 |
| **Bindings:** `let` projects a place read-only or owns a temporary; `var` owns mutably; `mut` projects a place mutably. | T0 |
| **Projections:** a function may return `-> borrow T` or `-> mut T` of one of its parameters. Projections never outlive the caller's scope. | T0 |
| **Exclusivity:** while a `mut` access is live, nothing may touch an overlapping place. Disjoint fields don't overlap; every element of a container overlaps every other (`pair_mut` and `split_at_mut` check at runtime). Checked within each function. | T0 |
| **No mutable globals, and no interior mutability** like `Cell` or `RefCell`. Shared mutable state lives in an arena. | T0 |
| **A projection must come from a `borrow` or `mut` parameter,** and the caller treats the result as borrowing every such argument. | T0 |
| **Non-escaping types:** any type containing a view (`Span<T>`, borrowing iterators, projections) follows the projection rules. That's how views and iterator chains work without lifetimes. | T1 (views and iterators, D-088) |
| **Closures are non-escaping by default** (T0). An escaping closure is marked `@escaping` and captures only owned values and handles (T1). | T0 / T1 |
| **Long-lived relationships are handles into arenas,** never pointers. | T1 |
| **Regions:** `Region<T>` holds one root value in chunks. Its containers store offsets. Region-bound values (`Relocatable` but not `Plain`) stay in their region. | T1 |
| **GPU layout:** a declared `GpuData` trait fixes a type's layout to WGSL rules everywhere. This is how tier 0's lossless GPU layout is expressed. | T0 |
| **Byte-level data:** the auto traits `Plain` and `Relocatable`. | T1 |
| **Snapshots:** copy-on-first-write chunk checkpoints are stdlib operations on regions, not language features. | T2 |
| **Destructors:** deterministic, at end of scope in reverse order; user `Drop` for types that aren't region-bound (D-087). | T1 |
| **`unsafe`** exists only for the stdlib's core; packages declare whether they use it. | T1 |

```wrela
fn grazer(world: mut World, h: Handle<GrazerSim>) -> mut GrazerSim {
    mut world.grazers[h]                 // a projection of the `mut` parameter
}

mut g = grazer(mut world, h)             // `world` stays mutably borrowed while `g` is live
g.gait.mode = Mode::Flee

var backup = log.clone()                 // explicit deep copy
archive(take log)                        // explicit move
```

---

## 7. Generics and traits

| Rule | Tier | Decisions |
|---|---|---|
| **Generics are always monomorphized.** | T0 | D-071 |
| **Structure is types.** Combinators are generic types, so building a field builds a type, and the field's numbers are plain data that reach the GPU as uniforms. | T0 | D-070 |
| **`impl Trait` convention:** a trait-shaped type in return position is one inferred concrete type; in parameter position it makes the function implicitly generic. | T0 | D-070 |
| **Choosing structure at runtime:** an enum for a finite set (all cases compiled, uniform branch); `std::stage::interpret` for unbounded structure built at runtime. | T0 / T2 | D-070, D-053 |
| **Control-flow values are data:** loop bounds and comparisons become uniforms; unrolling is the optimizer's choice. | T0 | D-070 |
| **The pipeline-count query** reports how many instantiations each GPU entry point has, and why. | T1 | D-044, D-070 |
| **Auto traits:** `Plain`, `Relocatable`, `Sendable`, `Shareable`. A type gets one when all its fields have it, and can opt out. | T1 | D-054, D-078 |
| **Declared traits with a structural check:** a library trait can require that every field also implements it (`SimState` is the engine's example). | T1 | D-078, D-060 |
| **Library-authored diagnostics:** `@diagnostic(...)` attaches a message to a trait or type, for when a bound isn't met. | T1 | D-055 |
| **Effects are checked per instantiation.** | T0 | D-071 |

```wrela
/// Generic over any field with this shape; monomorphized per concrete field type.
@compute(64)
fn cull_blocks<F: Surface>(field: F, grid: Grid, live: mut Append<LiveBlock>, id: GlobalId) {
    let block = grid.block(id.x)
    if !field.interval(block.bounds).contains(0m) { return }   // `interval` is derived (§13)
    live.push(LiveBlock { block })
}
```

---

## 8. Effects and contexts

**Effects** (D-072): `alloc`, `io`, `nondet`, `recursion` (unbounded), `dyn`, `host`, `panic`.

**Each context forbids a subset:**

| Context | Forbidden | Tier |
|---|---|---|
| GPU entry points (`@compute`, `@vertex`, `@fragment`) | `alloc`, `io`, `nondet`, `recursion`, `dyn`, `host`, `panic` | T0 |
| `@audio` | `alloc`, `io`, `recursion`, `dyn`, `host` | T2 |
| `@deterministic` | `nondet`, `host` (except declared deterministic host calls), `recursion` (D-094) | T1 |
| Compile-time evaluation | `io` (except declared embeds), `host`, `nondet` | T1 |
| Derived interpretations (gradient, interval) | `alloc`, `io`, `nondet`, `host` | T0 |

- **Inside a package, effects are inferred** (D-010, D-030). Annotations only assert. Errors show the call chain: `extract → foo → bar allocates at line 42`.
- **Exported functions state their effects** (D-030). `wrela fix` writes the annotations, and queries show what was inferred.
- **Public higher-order functions inherit effects from their closure arguments** by default (D-030). `map` is GPU-safe whenever its closure is.
- **Staging is guaranteed or rejected, never best-effort** (D-072). The optimizer may hoist work, but code must not rely on it. Work that must happen earlier is written earlier, as a `const` or a parameter.

---

## 9. Attributes

`@` attributes are a **closed set defined by the language** (D-037, D-081). All of them except `@diagnostic` are part of a type or contract: they change what a function *is*.

| Attribute | On | Meaning | Tier |
|---|---|---|---|
| `@compute(x, y, z)` | functions | GPU compute entry point, with workgroup size | T0 |
| `@vertex`, `@fragment` | functions | GPU render entry points | T0 |
| `@gpu` | functions | Asserts the function is GPU-safe, so a violation errors at its definition (D-010) | T0 |
| `@comptime` | functions, parameters | Runs, or is known, when the compiler runs (D-051, D-073) | T1 |
| `@deterministic` | functions, function types | The determinism constraint (§14) | T1 |
| `@assert(fact)` | functions | A checked fact, e.g. `@assert(lipschitz <= 1.5)` (D-077) | T1 |
| `@assume(fact)` | functions | A trusted fact, e.g. `@assume(lipschitz: 1)`, `@assume(bandlimit: ...)`. Debug builds spot-check by sampling; every assumption is greppable. (D-077) | T1 |
| `@escaping` | closures | The closure may outlive the call. memory-model §7 writes it on the closure expression: `@escaping \|w\| ...`. (D-064) | T1 |
| `@diagnostic(...)` | traits, types | A library-authored error message; not part of the type (D-055, D-081) | T1 |
| `@audio` | functions | Audio-worklet entry point (D-072) | T2 |

User-defined metadata, if it's ever needed, gets a different syntax, so `@` always means semantics (D-037).

---

## 10. Compile-time evaluation (T1)

- **An interpreter inside the compiler** runs `@comptime` functions and `const` initializers (D-073).
- **Types are compile-time values,** so reflection can enumerate a struct's fields and an enum's variants. That's what structural defaults (§3) are written with (D-060).
- **Compile-time heap values may be embedded in the output only if they're `Plain`** (D-073).
- **File reads at compile time** go through declared `embed` paths inside the package, hashed so builds are reproducible (D-073).
- **No procedural macros** (D-060).

---

## 11. Numerics

**CPU code always uses strict IEEE floats** (D-074). There's no fast-math mode, no reassociation, no implicit FMA contraction and no relaxed SIMD. Transcendentals come from the stdlib, compiled to WASM, never from the host (D-015).

**Tiers:** tier 0 emits WASM, whose float arithmetic is already IEEE-strict apart from NaN bits. The numeric rules (the table below, NaN canonicalization, stdlib transcendentals) are tier 1 with `@deterministic` (D-088).

**GPU code follows WGSL semantics,** and its results are presentation-only: GPU results can't reach `@deterministic` code (§14).

**Numeric semantics** (D-074):

| Case | CPU | GPU |
|---|---|---|
| Integer overflow | Traps in every build | Wraps |
| Integer divide by zero, out-of-range float → int | Traps | WGSL-defined values |
| Out-of-bounds index | Traps | Debug builds set an error flag; release builds clamp |
| NaN | Canonicalized wherever observable in `@deterministic` code: bit casts, sign tests, stores into `Plain` or `Relocatable` memory, hashing. Debug builds trap on NaN creation. | WGSL |

**A value's bytes are a deterministic function of its fields** (zeroed padding, canonical NaNs). Equal values don't always have equal bytes: `-0.0 == 0.0` (D-074).

**Evidence:** spike 01 hashed 1M evaluations of the grazer field, a mass integration and 10K raycasts, compiled from Rust with these rules. WASM in Chromium and native aarch64 gave identical bits. That's one platform pair, and Rust rather than wrela (spikes/01-grazer).

---

## 12. GPU code

| Rule | Tier | Decisions |
|---|---|---|
| **Entry points:** `@compute(...)`, `@vertex`, `@fragment` | T0 | D-010, D-035 |
| **Builtins are typed:** `GlobalId`, `WorkgroupId`, `LocalId`, `ClipPosition`, and `Flat<T>` for values that aren't interpolated. The docs map them to WGSL's `@builtin(...)`. | T0 | D-046 |
| **Closures and iterators are allowed when statically resolved:** monomorphized and inlined, fixed-size iterators unrolled. Diagnostics flag unrolling or inlining blowups. | T0 (closures) / T1 (iterators) | D-047, D-088 |
| **Layout is automatic but lossless.** `GpuData` fixes a type's layout to WGSL rules everywhere (T0); lossy encodings are explicit types (T1). Nobody pads by hand. | T0 / T1 | D-049, D-084 |
| **A kernel's `mut` parameters must be safe to share across invocations:** atomics, `Append<T>`, `Slots<T>` (each invocation writes only its own slot), `AtomicMap`. A plain `mut [u32]` is rejected. | T0 | D-084 |
| **Uniform vs varying** is the target's own distinction, which WGSL already analyzes. | T0 | D-051 |
| **Workgroup-shared memory and barriers.** Spike 01's `place_vertices` needed them. **Open:** the design. | T0 | D-093 |
| **GPU interval arithmetic widens each result outward** by its operation's WGSL error bound, so it stays conservative. | T0 | D-075 |

```wrela
@fragment
fn shade<F: Surface + Channels<C>, C: Blend>(field: F, s: Skinned, lights: Lights) -> Color {
    let fp = fwidth(s.rest_pos)                         // this pixel's footprint
    let t  = field.channels(s.rest_pos, footprint: fp)  // only the channels used here are computed (D-002)
    let n  = s.rot * field.gradient(s.rest_pos, footprint: fp).normalize()   // derived gradient
    lights.shade(t.albedo, t.roughness, n)
}
```

---

## 13. Derived interpretations

The compiler derives these from any function that qualifies under the effect table (§8). It doesn't know what a field is (D-056).

| Interpretation | What it gives | Tier | Decisions |
|---|---|---|---|
| `gradient` | Forward-mode derivative | T0 | D-012 |
| `interval` | A conservative range over a box | T0 | D-012, D-075 |
| Lipschitz bound | Composed from declared facts on primitives; read through `facts::lipschitz(f)` at compile time | T1 | D-057, D-077 |
| Pruning | `f.prune(bounds) -> LiveMask<F>`, `f.with_live(mask)` | T1 | D-045, D-080 |

- **A choice point** is a `min`, `max`, `select`, `if` or `match` arm whose outcome an interval can decide (D-080).
- **`LiveMask<F>` is opaque and typed by the function it prunes.** Library code can store and pass it, but can't read its bits (D-080).
- **Declared facts** supply what derivation can't: `@assume(lipschitz: 1)` on `length`, and a range and `@assume(bandlimit: ...)` on noise (D-057, D-077). The compiler composes them through callers. Placeholder: D-077 spells only `lipschitz` and `bandlimit`, so the spelling of a range fact is open.
- **Bandlimits** let noise fade octaves finer than a footprint instead of aliasing, on the GPU and anywhere else a footprint is known (D-077).
- **Open (from spike 01):** a fact may hold only near where it's used. The ellipsoid bound's gradient is unbounded at its centre, so a global Lipschitz constant fails even though the constant holds within reach of the surface. See D-092.

---

## 14. Determinism (T1)

`@deterministic` is an effect constraint on functions and function types (D-052).

- **Inside it:** strict floats (always true on the CPU), stdlib transcendentals, canonical NaNs at observation points.
- **Forbidden:** the clock, ambient randomness, GPU readback, unordered iteration, relaxed SIMD, anything that depends on memory addresses (D-015, D-052), and unbounded recursion, because stack limits differ between engines (D-015, D-094).
- **A panic is a deterministic trap:** every client traps on the same tick (D-061).
- **Parallelism is data-parallel only** (D-062), through stdlib combinators (`par_each_mut`, `par_map_reduce`). Exclusivity proves disjointness; `Shareable` covers captured data; reductions combine in a fixed tree order; per-entity RNG streams keep results independent of scheduling. T2.

The sim/presentation split is an engine pattern built on this, not a language feature (D-052).

---

## 15. Errors (T1)

- **Recoverable failures are `Result<T, E>`, propagated with `?`** (D-061).
- **Bugs panic.** On the CPU, a panic is a trap. GPU code can't panic; out-of-bounds access sets a debug flag or clamps (D-074).
- **What happens after a trap** is the program's policy. The engine rewinds to the tick's checkpoint and writes a repro bundle (D-087).

---

## 16. Concurrency (T2)

- **Threads:** values cross threads only if `Sendable`; data shared between threads must be `Shareable` (memory-model §12).
- **Async is structured concurrency only.** Projections and non-escaping values can't cross an `await` in a task that outlives its caller (D-087).

---

## 17. The stdlib boundary

- **The compiler knows three things** (D-050): the language, a closed list of stdlib items, and the execution targets.
- **The closed list is published in the spec** (D-081). Candidates: `Copy`, `Clone`, `Option`, the iteration protocol, the operator traits, GPU entry-point lowering, `Plain` layout. **Open:** the final list.
- **Stdlib modules pass an admission test** (D-081): would this make sense in a program that isn't a game? Acoustics, for example, is engine code.
- **The stdlib is written in wrela** with a small unsafe core (D-081).

**Indicative stdlib map** (placeholder; module boundaries aren't decided):

| Module | Contents |
|---|---|
| `std::field` | `Field<K, C>`, the kinds `Exact` / `Bound` / `Lipschitz`, `Surface`, `Channels`, `Blend`, `Cat<T>`, primitives, combinators, noise |
| `std::units` | `m`, `cm`, `mm`, `kg`, `s`, `rad`, `deg`, … |
| `std::gpu` | `dispatch`, `Append<T>`, `Slots<T>`, `AtomicMap`, typed builtins |
| `std::region` | `Region<T>`, `Arena<T>`, `List<T>`, `Handle<T>`, checkpoints |
| `std::stage` | `interpret`: a tape evaluator for fields built at runtime |
| `std::hash`, `std::serialize` | `StateHash`, `Serialize`, with structural defaults |

### Fields are stdlib code

- **A field returns a distance plus channels** (D-002). Channel structs opt in to `Blend`, and each member's type decides how it blends: `Color` in linear space, `f32` linearly, `UnitVec3` renormalized, `Cat<T>` from the winner (D-026).
- **Kinds are ordinary types** (D-056, D-077):

  | Kind | Meaning |
  |---|---|
  | `Exact` | The true distance |
  | `Bound` | Never overestimates; Lipschitz ≤ 1 |
  | `Lipschitz` | Sign-correct; derived L may exceed 1 |

  Each op states in its return type what it preserves. `.to_bound()` divides by the derived L (D-077). Placeholder: sketch 01 has `Exact` convert to `Bound`, and `Bound` to `Lipschitz`, implicitly. No decision covers implicit conversions, and the language has no general mechanism for them yet.
- **Combinators are methods** (D-028): `a.smooth_union(b, k: 15cm)`, and n-ary on collections. There's no operator overloading on fields; vectors and units do get operators.
- **Fields built at runtime** from an unbounded space use `stage::interpret` (D-029, D-053). T2.

---

## 18. What wrela leaves out, compared with Rust

- **References as types,** and with them lifetime parameters, variance, `'static` and higher-ranked bounds (D-058, D-064)
- **`Pin`,** because references can't live across suspension points (memory-model §15)
- **`Cell` and `RefCell`** (interior mutability), and **`Rc` and `Arc`** in ordinary code (memory-model §16)
- **Mutable globals** (memory-model §5)
- **Implicit moves out of named places** (D-064)
- **A garbage collector** (D-014)
- **Procedural macros:** compile-time reflection replaces them (D-060)
- **Fast-math on the CPU** (D-074)
- **User-defined attributes** (D-037)
- **Null** (memory-model §1)

---

## 19. A tier-0 program

This is the subset needed for "hello field" (D-088 tier 0): a field, a derived gradient, one compute kernel and one fragment shader. No units, regions or determinism.

```wrela
use std::field::{Field, Bound, Surface, sphere, round_cone}
use std::gpu::{GlobalId, Slots}

/// A field. Its structure (a smooth union of two primitives) is its type;
/// the radii are data, so every blob shares one pipeline.
pub fn blob(r: f32) -> Field<Bound, ()> {
    sphere(radius: r)
        .smooth_union(round_cone(vec3(), vec3(y: 1.0), 0.3, 0.1), k: 0.1)
}

pub struct Grid: GpuData {
    origin: vec3,             // `vec3` without a unit is unitless
    cell:   f32,
    n:      u32,
}

/// One sample per invocation. `Slots` lets each invocation write only its own slot.
@compute(64)
fn sample<F: Surface>(field: F, grid: Grid, out: mut Slots<f32>, id: GlobalId) {
    let i = id.x
    let p = grid.origin + vec3(f32(i % grid.n), f32((i / grid.n) % grid.n), f32(i / (grid.n * grid.n))) * grid.cell
    out[id] = field.distance(p)
}

/// Normals come from the derived gradient. Nobody wrote it.
@fragment
fn normals<F: Surface>(field: F, pos: vec3) -> Color {
    let n = field.gradient(pos).normalize()
    Color(n * 0.5 + 0.5)
}
```

---

## 20. Tiers at a glance

| Tier | Features |
|---|---|
| **T0** | Statements and literals; functions with modes and named arguments; structs with defaults; enums (including `Option`); `const` with literal values; traits with associated types and default methods; monomorphized generics and `impl Trait`; projections and exclusivity; non-escaping closures; scalar, vector and matrix types; `@compute`/`@vertex`/`@fragment`/`@gpu`; typed builtins; invocation-safe kernel outputs; workgroup-shared memory (D-093); lossless GPU layout through `GpuData`; derived `gradient` and `interval`; WGSL and WASM emission. |
| **T1** | Units; `@comptime`, evaluated `const` initializers and reflection; structural defaults; auto and declared traits; `@diagnostic`; `@deterministic` and the numeric rules; `@assert`/`@assume` and bandlimits; Lipschitz facts and pruning; handles, arenas and regions; `Plain`/`Relocatable`; views, iterators and other non-escaping types; lossy GPU encodings; strings; destructors; `Result`, `?` and panics; `@escaping`; the pipeline-count query. |
| **T2** | Threads and parallel combinators; async; `@audio`; checkpoints and keyframes; `dyn Trait`; `stage::interpret`; any compiler tier in the browser. |

---

## 21. Open

- **Modules and packages** (D-087). D-030's public-boundary rule depends on them.
- **The keyword list** (§2).
- **The closed list of stdlib items the compiler knows** (D-081).
- **Syntax for structural defaults:** `@comptime default for<T: struct>` is a placeholder (§3).
- **How a generic parameter declares that it accepts non-escaping types** (memory-model §4).
- **Scope for declared facts** (§13, D-092).
- **Workgroup-shared memory and barriers in kernels** (§12, D-093).
- **`from param` annotations** (decisions.md, Open).
- **Region chunk sizes and undo-ring sizes** (memory-model.md, Open).
- **A general `schedule` construct** for any function's evaluation stays possible as future sugar (D-053). Add it only when kernels need it.
