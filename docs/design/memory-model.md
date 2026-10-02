# wrela: memory model

*Status: accepted direction, 2026-10-01. This is the one place the memory rules live; other docs link here. Decisions: D-014, D-058, D-059, D-064, D-065, D-066, amended after the [2026-10-01 audit](reviews/2026-10-01-audit.md) by D-083 and D-084, in [decisions.md](decisions.md). The syntax is still imagined.*

## Goals

- **Memory-safe without a garbage collector** (D-014). Frame times stay predictable.
- **Every transfer is visible.** Nothing is copied, moved or mutated without it showing at the point where it happens. The one exception: calling a `mut self` method doesn't repeat `mut`, because the method's name already says what it does (§2).
- **No lifetime annotations, ever.** Agents and humans should never have to reason about lifetime parameters.
- **Friendly to determinism.** Behavior never depends on memory addresses, and snapshots are cheap (D-015).
- **Maps directly onto the targets:** WASM linear memory, WebGPU buffer bindings, and the audio worklet.

## The rules at a glance

1. **Everything is a value.** `&T` isn't a type. The projection types `borrow T` and `mut T` exist only as non-escaping types (§6).
2. **Parameters have modes:** `borrow` (the default), `mut` and `take`. The caller writes `mut` and `take` at the call site.
3. **Moving out of a named place is written `take`.** Copying is written `.clone()`, except for small `Copy` types.
4. **A function may return a *projection*** (`-> borrow T`, `-> mut T`) of one of its parameters. Projections never outlive the caller's scope.
5. **Exclusivity:** while a `mut` access is live, nothing else may touch an overlapping place. Checked statically, within each function.
6. **A type that contains a view is non-escaping.** It follows the same rules as a projection.
7. **Long-lived relationships use handles into arenas.** Never anything pointer-like.
8. **Data that must be copied as raw bytes is `Plain` or `Relocatable`.** That covers sim state, GPU buffers, network messages and keyframes.
9. **Region-bound values stay in their region** (§10).

---

## 1. Values, copies and moves

```wrela
let a = vec3(1m, 2m, 3m)
let b = a                      // vec3 is Copy: an implicit copy

var log = EditLog::new()
var backup = log.clone()       // a deep copy is always explicit
archive(take log)              // moving out of a named place is written `take`
log.push(edit)                 // error: `log` was moved into `archive` on line 6
```

- **`Copy` types copy implicitly.** These are small plain types, like numbers, vectors, `Transform` and `Handle<T>`, that declare `Copy`. Their implementation is structural (D-060).
- **Everything else is copied only with `.clone()`** (the `Clone` trait), **or moved with `take`.** The two words mean what they mean in Rust (D-083).
- **No marker is needed for:** temporaries, as in `archive(EditLog::new())`, or returning a local variable.
- **Destruction is deterministic:** at the end of scope, in reverse declaration order. A value that was moved out isn't destroyed again.
- **There is no null.** Use `Option<T>`.

## 2. Parameter modes

| Mode | Signature | The callee may | The caller writes |
|---|---|---|---|
| **borrow** (default) | `x: T` | read | `f(x)` |
| **mut** | `x: mut T` | read and write, but not keep | `f(mut x)` |
| **take** | `x: take T` | own it: keep it, move it, destroy it | `f(take x)` from a named place; `f(T::new())` for a temporary |

**Method receivers** are written `self`, `mut self` and `take self`.
- **A `mut self` receiver isn't marked at the call site:** `list.push(x)`, not `mut list.push(x)`. This is the one stated exception to "every transfer is visible."
- **A consuming (`take self`) method on a named place is marked:** `take builder.finish()`. On a temporary, no marker is needed: `Builder::new().finish()` (D-084).

`[T]` is a contiguous run of `T`. As a parameter, it's borrowed like anything else. `mut [T]` is a mutable run.

## 3. Local bindings

| Binding | Meaning |
|---|---|
| `let x = place` | A read-only projection of an existing place. Nothing is copied. |
| `let x = temporary`, or `let x = take place` | Owned and immutable. |
| `var x = temporary`, `var x = take place`, or `var x = place.clone()` | Owned and mutable. |
| `mut x = place` | A mutable projection of an existing place. |

```wrela
let g = world.grazers[h]       // read-only projection: no copy
mut t = world.terrain          // mutable projection
t.edits.push(Edit::Dig { at, radius: 1m })
var spare = world.grazers[h].clone()   // an owned copy, mutable (fine: GrazerSim is `Plain`, so not region-bound)
```

## 4. Projections

A function can hand back access to part of one of its parameters, instead of a value:

```wrela
fn grazer(world: mut World, h: Handle<GrazerSim>) -> mut GrazerSim {
    mut world.grazers[h]
}

mut g = grazer(mut world, h)   // `world` stays mutably borrowed while `g` is live
g.root.pos.y += 1m
```

- **Returns are written `-> borrow T` or `-> mut T`.** A plain `-> T` returns an owned value.
- **A projection must come from a `borrow` or `mut` parameter.** You can't project from a local or a temporary.
- **The caller treats the result as borrowing every `borrow` and `mut` argument.** This is conservative. An optional `from param` annotation to narrow it can come later, if real code needs it.
- **Projections can't be stored** in structs or collections, or captured by escaping closures (§7). They never outlive the scope that received them, so no lifetimes are needed.
- **Projection types can be type arguments.** `borrow T` and `mut T` are non-escaping types (§6), so `arena.get(h)` returns `Option<borrow T>`, and an iterator can yield `mut T`. Generic code that accepts them declares its type parameter as possibly non-escaping.

## 5. Exclusivity

**Places** are locals, their fields, and elements of containers. Two places **overlap** when one is a prefix of the other. For example, `world` overlaps `world.terrain`, but `world.terrain` and `world.grazers` don't.

**Every element of a container overlaps every other element of that container.** The checker doesn't compare indices.

**The rule:** while a `mut` access is live, from where it's created to its last use, no other access may touch an overlapping place. In a single call, arguments may not overlap if any of them is `mut`.

```wrela
for mut g in world.grazers {
    step(mut g, world, intent)
    // error: `world` overlaps `world.grazers`, which `g` is mutably borrowing
    //   help: pass only what `step` reads, e.g. `world.terrain`
}

let (a, b) = world.grazers.pair_mut(h1, h2)   // two elements at once: checked at runtime,
                                              // panics if h1 == h2
```

- **The check is static and stays within one function.** It never needs to look inside another function, because signatures say everything.
- **The only runtime checks are explicit library calls** like `pair_mut` and `split_at_mut`.
- **There are no mutable globals.** State is passed in explicitly. Constants are fine.
- **There's no interior mutability** like Rust's `Cell` or `RefCell`. Shared mutable state lives in an arena and is reached through `mut` access to that arena.

## 6. Non-escaping types and views

Some values need to *hold* access to other data: views, and iterators that walk a container. In wrela these are **non-escaping types**, after Swift's `~Escapable`.

- **The stdlib provides the roots:**
  - `Span<T>` and `SpanMut<T>`, which are views over contiguous data
  - iterators that borrow a container
  - closures that capture projections
- **Any type with a non-escaping part is non-escaping too.** This is structural and automatic.
- **Non-escaping values follow the projection rules.** They can be passed down and returned as projections. They can never be stored in an escapable type, put in a collection, or captured by an escaping closure.

```wrela
for e in cell.edges().filter(|e| crosses(e)) { ... }   // a borrowing iterator chain: fine

struct Window { rows: Span<f32>, width: u32 }          // non-escaping, because Span is
fn window(img: Image, r: Rect) -> Window { ... }       // a projection of `img`
struct Cache { last: Window }                          // error: `Window` is non-escaping; store the
                                                       //   Rect and project again when you need it
```

**What a non-escaping value borrows can grow** (D-084). It starts as the sources it was projected from. These extend it:
- assigning into one of its fields
- passing it as `mut` alongside other borrowed arguments

In both cases its borrow set gains those sources. The check still stays inside one function. Without this rule, a view could outlive what it points at.

This is what removed most of the cost of second-class references (D-058). Views and iterator chains read just as they do in Rust, without lifetimes.

## 7. Closures

- **By default, a closure parameter is non-escaping.** The callee must finish with it before it returns. These closures can capture projections, both `borrow` and `mut`, and that access counts as live for the duration of the call.
- **An escaping closure is marked `@escaping`.** It can be stored or spawned, and it captures only owned values and handles. Moving a named place into one is written `take`.

```wrela
world.grazers.par_each_mut(|g| step(mut g, world.terrain, intent))   // non-escaping: may capture projections
timers.after(2s, @escaping |w| w.spawn(take herd))                   // escaping: owns what it captures
```

## 8. Handles and arenas

- **`Arena<T>` stores values in generational slots.** `Handle<T>` is an index plus a generation. It's `Copy`, `Plain` (§10), and can be stored anywhere.
- **Access:**
  - `arena[h]` is a projection, either borrow or `mut` depending on context. A stale handle panics.
  - `arena.get(h)` returns an `Option` of a projection.
  - `arena.remove(h)` moves the value out, unless the value is region-bound (§10). Then it's destroyed in place, or moved between containers of the same region.
- **Graphs, parent links and "this entity refers to that one" are all handles.** References never last longer than a call.

## 9. Allocation and regions

- **Allocation is an effect** (D-010). It's forbidden in GPU and audio code.
- **The global heap** backs `Vec`, `String` and `Box`. They own their memory and free it when dropped.
- **Scratch arenas** (per frame, per pass) hand out non-escaping containers:

```wrela
fn build_lists(scratch: mut Scratch, ...) {
    var near = scratch.list::<Handle<GrazerSim>>()   // non-escaping: borrows `scratch`
    ...
}   // `scratch.reset()` needs `mut scratch`, so exclusivity proves no list outlives a reset
```

- **A region, `Region<T>`,** is a self-contained memory area holding one root value of type `T`. It's built from fixed-size chunks.
  - Region containers (`Arena`, `List`, `Text`) allocate inside their region and store **region-relative offsets**, never absolute pointers.
  - You reach the root through `region.read(|w| ...)` or `region.write(|mut w| ...)`.

- **Region containers are created explicitly** from the region: `r.list::<T>()`, `r.arena::<T>()`. There's no ambient "current region" at creation time.
- **Each container records its region's ID.** When it grows, it finds its region's allocator in the thread's table of open regions.
  - Several regions can be open at once.
  - Using a container whose region isn't open is a panic.

This lives in the stdlib's unsafe core (§14). The compiler doesn't know regions exist.

## 10. Plain and relocatable data

Two stdlib auto traits (D-054) describe data that can be handled as raw bytes:

| Trait | Contains | Meaning | Used for |
|---|---|---|---|
| `Plain` | numbers, `Handle<T>`, fixed arrays, `Plain` structs | No pointers and no offsets. Padding is zeroed and NaNs are canonical (D-074), so a value's bytes are a deterministic function of its fields. They aren't equal for every pair of equal values: `-0.0 == 0.0`. Copy the bytes anywhere. | GPU buffers (D-049), network messages, handles |
| `Relocatable` | `Plain` data plus region containers | No absolute pointers. Valid whenever the *whole region* moves together. | Region roots, sim state, keyframes |

- **Heap types (`Vec`, `String`, `Box`) are neither.**
- **The engine's `SimState`** is a declared trait (D-078) that requires `Relocatable`. So a game's world can always be checkpointed and synced as bytes.
- **WASM is little-endian everywhere,** so the bytes are portable between clients.

**Region-bound values (D-084).** A value that's `Relocatable` but not `Plain` holds offsets into its region, so it's *region-bound*:
- It can't be owned outside its region: no `take` or `.clone()` into a local, a capture or another region.
- It's reached by projection only.
- It's cloned only into the same region, through that region's containers.

This is what makes "nothing outside the region points into it" (§11) a rule rather than a hope.

**Pointing outward is forbidden too.** Sim state never holds a handle into an arena outside its region, because a rewind would restore the handle but not the arena. It refers to outside data by a deterministic key instead, such as a definition plus a seed (D-084).

**GPU layout.** A type that crosses to the GPU declares `GpuData`, which fixes its layout to WGSL's rules everywhere it's used. Other `Plain` types use a natural CPU layout. There's one layout per type, never two.

## 11. Snapshots: copy-on-first-write chunks

```wrela
region.checkpoint(epoch)     // start an epoch: each chunk's old bytes are saved the first time it's written
region.rewind(to: epoch)     // restore every chunk saved since then
region.keyframe() -> Bytes   // the whole region, for save files and repro bundles
```

**Why it's sound:**
1. **Every write to region data goes through a stdlib container.** Containers mark a chunk when they hand out `mut` access to it. Inline data is covered by the projection that reaches it.
2. **Projections are second-class.** None can outlive the call that created it. `checkpoint` and `rewind` take `mut` access to the region, so exclusivity proves that no projection is live across an epoch boundary. A write can never land in the wrong epoch.
3. **Nothing outside the region points into it, and nothing inside points out.** Region-bound values can't leave (§10), outside code holds only handles (indices), and sim state refers outward only by deterministic keys.

**Cost, as estimates to be measured:**
- Per-tick cost is proportional to the chunks *written*, not the world's size. Every simulated entity is written every tick, so the cost is roughly the size of the active entity data. Static data, such as the terrain's base field and inventories, is free. For example, 10,000 active entities at 256 bytes each come to about 2.5 MB per tick, roughly 0.3 ms.
- Keyframes cost a full copy, so they're taken every few seconds, not every tick.
- Under D-042, clients only roll back their own predicted entities, which are small.

**Details (D-084):**
- **Parallel combinators** mark every chunk they'll hand out in a sequential pre-pass, so two threads never race to be first to write a chunk.
- **Checksums are incremental.** Each chunk keeps a hash. Only chunks written this tick are rehashed, and the per-chunk hashes are combined. Raw-byte hashing is safe because of canonical bytes (§10).
- **Keyframes are same-build only.** They're for rollback, repro bundles and network sync within one version, because a keyframe is a memory image: every layout is baked in.
- **Save files use structural `Serialize`** (D-060), with stable field identifiers, so saves survive layout changes and can be migrated.

Bulk data belongs in containers, which mark one element's chunk at a time. A huge inline array in the root would be marked all at once.

## 12. Threads

- **Platform:** web workers plus SharedArrayBuffer (D-017). Every worker shares one WASM memory, whose maximum is reserved at startup. [platform.md](platform.md) §3 has the thread layout (D-098).
- **Two stdlib auto traits:** `Sendable` (may move to another thread) and `Shareable` (may be borrowed from several threads at once).
- **Parallelism goes through data-parallel combinators** (D-062). Exclusivity proves disjointness, so there are no locks in game or engine code.
- **Atomics and queues** live in the stdlib's unsafe core.

## 13. GPU and audio

- **Modes map onto WebGPU bindings:** `borrow` becomes a read-only storage or uniform binding, and `mut` becomes a `read_write` storage binding.
- **WebGPU forbids aliasing writable bindings** within a dispatch, which matches exclusivity *per binding*.
- **That doesn't cover a single dispatch.** Every invocation holds the same `mut` binding at once, so exclusivity says nothing about how invocations share it (D-084). A kernel's `mut` parameters therefore accept only invocation-safe types:
  - atomics
  - `Append<T>`
  - `Slots<T>`, where each invocation writes only the slot keyed by its own ID

  A plain `mut` array parameter is rejected.
- **Data that crosses to the GPU must be `Plain` and `GpuData`.**
- **GPU-resident data is reached through handles** (D-102). `GpuBuffer<T>` is `Plain` and `Copy`, like `Handle<T>`, and can't be read through on the CPU. Uploads are explicit copies; readback is asynchronous and `nondet`. There's no zero-copy path between WASM memory and the GPU ([platform.md](platform.md) §4).
- **`@audio` code** borrows preallocated `Plain` buffers and never allocates.

## 14. The unsafe core

`unsafe` exists only so the stdlib can implement things the checker can't verify:
- `Vec`, `Arena`, `Span`, `Region`
- the open-region table
- atomics and queues
- host bindings

Each package declares whether it uses `unsafe`. Game and engine code shouldn't need to, and the package policy flags it if it does.

## 15. Async

Async uses structured concurrency only (D-087). Projections and non-escaping values can't cross an `await` in a task that outlives its caller. That's how wrela avoids ever needing `Pin`.

## 16. What wrela leaves out, compared with Rust

- Lifetime parameters, variance, `'static` and higher-ranked bounds
- `Pin`, because references can't live across suspension points (§15)
- `Cell` and `RefCell` (interior mutability)
- `Rc` and `Arc` in ordinary code
- Implicit moves out of named places
- Reference *types*: `&T` isn't a type at all

## 17. Diagnostics

The errors agents will hit most, and what they say:

```
error: `&` isn't a type in wrela
  --> leader: &GrazerSim
  help: to refer to another grazer, store `Handle<GrazerSim>`

error: `world` is already mutably borrowed
  --> step(mut g, world, intent)
  note: `g` borrows `world.grazers` mutably until line 14
  help: pass only what `step` reads: `world.terrain`

error: can't move out of `world.grazers[h]`: it's a projection
  help: use `world.grazers[h].clone()`, or `world.grazers.remove(h)` to take it out of the arena

error: `log` was moved into `archive` on line 5
  help: write `archive(log.clone())` there if you still need `log`

error: `edits` is region-bound and can't be owned outside its region
  --> var saved = world.terrain.edits.clone()
  help: read it through a projection, or clone it into a container of the same region
```

---

## Open

- **`from param` annotations** to narrow which argument a projection borrows. Add them only if conservative borrowing hurts in real code.
- **Chunk size and undo-ring sizing.** To be measured.
