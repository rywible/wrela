# Sketch 03: the simulation

*Status: 2026-10-01. Q20–Q24 and E1 are resolved (D-058–D-063). Rewritten for the memory model ([../memory-model.md](../memory-model.md), D-064–D-066), then revised for the [2026-10-01 audit](../reviews/2026-10-01-audit.md) (D-078, D-084, D-085). The syntax is imagined.*

Sketches 01 and 02 were mostly pure math. This one is the code that **mutates**: the world, one gameplay system, the fixed tick, snapshots and rollback. It's the sketch that forced the memory model. The rules live in [memory-model.md](../memory-model.md); this sketch shows them under load.

Each piece of code is labeled with its layer (D-050): **game**, **engine** or **stdlib**.

**Memory rules used here, in brief:**
- Parameters are borrowed by default. `mut` marks exclusive mutable access, and is repeated at the call site.
- `take` moves, and `.clone()` copies.
- Functions may return projections (`-> mut T`) of their parameters.
- Exclusivity is checked within each function.
- Long-lived links are handles.
- Sim state lives in a region, as relocatable data.

---

## 1. The world

```wrela
// game/world.wrela  (game)

/// Everything the simulation knows. It lives in a `Region<World>` (memory model §9),
/// so its bytes are the whole world: snapshots, saves and checksums all work on them.
pub struct World: SimState + StateHash + Serialize {
    tick:    Tick,
    seed:    Seed,
    terrain: Terrain,
    grazers: Arena<GrazerSim>,
    players: Arena<PlayerSim>,
}

/// Terrain is sim state: a base field plus an edit log (D-015).
pub struct Terrain: SimState + StateHash + Serialize {
    base:  TerrainField,       // authored field from the game's IR
    edits: List<Edit>,         // a region list: offsets, not pointers
}

pub enum Edit: SimState + StateHash + Serialize + Copy {
    Dig  { at: vec3<m>, radius: f32<m> },
    Fill { at: vec3<m>, radius: f32<m> },
}
```

**Notes:**
- **`Arena<T>`, `List<T>` and `Handle<T>` are stdlib region containers.** They store offsets relative to the region, never absolute pointers (D-065).
- **`SimState` is a declared engine trait with a structural check** (D-078). It requires `Relocatable`, so putting a `Vec` (which owns heap memory) in the world is an error that explains why.
- **`StateHash` and `Serialize` come from the traits' structural defaults** (§5, D-060). Nobody writes them by hand. `Serialize` writes save files with stable field identifiers (§6).

---

## 2. Modes and projections in practice

```wrela
/// Borrowed by default:
fn nearest(players: Arena<PlayerSim>, at: vec3<m>) -> Option<Handle<PlayerSim>> { ... }

/// A projection of a `mut` parameter:
fn grazer(world: mut World, h: Handle<GrazerSim>) -> mut GrazerSim {
    mut world.grazers[h]
}

mut g = grazer(mut world, h)      // `world` stays mutably borrowed while `g` is live
g.gait.mode = Mode::Flee
```

What gets rejected:

```wrela
struct Herd {
    leader: &GrazerSim            // error: `&` isn't a type in wrela; store `Handle<GrazerSim>`
}

fn dangling() -> borrow Terrain {
    let w = World::new()
    w.terrain                     // error: a projection must come from a parameter; `w` is local
}

@deterministic
fn tangled(world: mut World) {
    for mut g in world.grazers {
        step(mut g, world, intent)
        // error: `world` overlaps `world.grazers`, which `g` is mutably borrowing
        //   help: pass only what `step` reads: `world.terrain`
    }
}
```

That last error is why sketch 01's `step` takes `terrain: Terrain` rather than the whole world. Rust game developers will recognize the fight. Two things keep it rare here:
- **Signatures that take the parts they need.**
- **Tick-start copies of whatever crosses entities** (D-085). Systems read a compact copy, such as a grid of positions, instead of the entities being updated (§3).

---

## 3. One tick

```wrela
// game/tick.wrela  (game)

/// Grazers drift with the herd and flee nearby players. Players can dig.
@deterministic
pub fn tick(world: mut World, input: TickInput) {
    apply_input(mut world.players, mut world.terrain, input)    // disjoint fields: fine

    let near = SpatialGrid::build(world.players.positions(), cell: 10m)   // copies, built in handle order:
    let herd = SpatialGrid::build(world.grazers.positions(), cell: 10m)   //   deterministic (D-085)
    world.grazers.par_each_mut(|g| {                            // `g`: a mutable projection of one grazer
        let threat = near.closest(g.root.pos, within: 25m)
        let mates  = herd.within(g.root.pos, radius: 15m)      // other grazers, as of the start of the tick
        let intent = herd_intent(mut g, threat, mates)          // uses g.rng: a per-grazer stream
        step(mut g, world.terrain, intent)                      // sketch 01's step
    })
    world.tick += 1
}
```

**Notes:**
- **`par_each_mut` is stdlib, and it's deterministic by construction** (D-062):
  - Each call gets `mut` access to exactly one element, and exclusivity proves no two calls share it.
  - The closure captures `near` and `world.terrain` borrowed. Neither overlaps `world.grazers`.
  - Captured data must be `Shareable`.
  - Reductions combine in a fixed tree order.
- **The herd reads its neighbors from `herd`,** a copy of every grazer's position made at the start of the tick (D-085). It can't read `world.grazers` directly, because that's what `par_each_mut` is mutating. The first draft passed no neighbors at all, so the herd couldn't actually herd (audit S1).
- **Each grazer has its own RNG stream,** seeded from the world seed and its handle. Results don't depend on which thread ran which grazer.
- **Before the parallel phase, `par_each_mut` marks every chunk it will hand out,** in a sequential pre-pass. Two threads never race to write a chunk first (D-084).
- **The compiler doesn't know any of this is a game.** It sees a `@deterministic` function, disjoint places, and a closure that doesn't escape.

```wrela
// game/terrain.wrela  (game)

impl Terrain {
    /// Collision queries in the sim evaluate this on the CPU, with strict floats.
    @deterministic
    pub fn distance(self, p: vec3<m>) -> f32<m> {
        var d = self.base.distance(p)
        for e in self.edits {                              // a runtime-length loop over data
            d = match e {
                Edit::Dig  { at, radius } => max(d, radius - (p - at).length()),
                Edit::Fill { at, radius } => min(d, (p - at).length() - radius),
            }
        }
        d
    }
}
```

**An edit log doesn't need the tape interpreter** (D-029).
- The base field's structure is specialized. The edits are data, consumed by an ordinary loop.
- The renderer re-extracts only the chunks an edit touches. That's engine code.
- With thousands of edits, the engine would index them spatially. That's ordinary data structures, with no compiler involvement.

---

## 4. Snapshots and rollback

```wrela
// engine/sim/timeline.wrela  (engine)

/// The engine's rollback driver, generic over any game's world.
pub struct Timeline<W: SimState> {
    sim:    Region<W>,             // the world lives in its own region
    tick:   Tick,
    inputs: Ring<TickInput>,       // TickInput is a small Copy type
}

impl<W: SimState> Timeline<W> {
    pub fn advance(mut self, input: TickInput, step: @deterministic fn(mut W, TickInput)) {
        self.sim.checkpoint(self.tick)                // new epoch: each chunk is saved on its first write
        self.inputs.push(input)
        self.sim.write(|mut w| step(mut w, input))
        self.tick += 1
    }

    /// A correction arrived for tick `t`, for example from the server (D-042):
    /// rewind, fix the input, and replay to the present.
    pub fn correct(mut self, t: Tick, input: TickInput, step: @deterministic fn(mut W, TickInput)) {
        self.sim.rewind(to: t)                        // restore every chunk written since tick `t`
        self.inputs.replace(t, input)
        for tick in t..self.tick {
            self.sim.checkpoint(tick)                 // replay records fresh epochs
            let i = self.inputs[tick]                 // `inputs` and `sim` are disjoint fields
            self.sim.write(|mut w| step(mut w, i))
        }
    }

    pub fn keyframe(self) -> Bytes { self.sim.keyframe() }     // repro bundles and net sync; same build only
    pub fn checksum(self) -> u64   { self.sim.chunk_hash() }   // incremental: only chunks written this tick are rehashed (D-084)
}
```

**Notes:**
- **`@deterministic fn(mut W, TickInput)` is a function type** (D-037). `Timeline` can only run deterministic steps, and it never learns what a grazer is.
- **No `.clone()` of the world, ever.** `checkpoint` and `rewind` work on region chunks (D-066):
  - Per-tick cost is proportional to the chunks *written*. Every simulated entity is written every tick, so the cost is roughly the size of the active entity data: 10,000 entities at 256 bytes each is about 2.5 MB, roughly 0.3 ms. These are estimates to be measured.
  - Keyframes (full copies) happen every few seconds, not every tick. They're same-build only (D-084).
  - The checksum costs the same as the checkpoint, because only written chunks are rehashed. The first draft hashed the whole world every tick, which would have undone the point of chunking (audit M4).
- **Why it's sound:**
  - `step` only touches the world inside `write(|mut w| ...)`.
  - Projections can't escape that closure.
  - `checkpoint` and `rewind` need `mut self.sim`, so exclusivity proves no write can straddle an epoch.
- **Under the server-authoritative model (D-042):**
  - Clients use `correct` to reconcile their own predicted player.
  - The server never rolls back.
  - The same type serves both.

---

## 5. Structural implementations: where `StateHash` comes from

```wrela
// std/hash.wrela  (stdlib)

pub trait StateHash {
    fn state_hash(self, h: mut Hasher)

    /// Structural default, generated at compile time from the type's fields. A type that
    /// declares `StateHash` without writing `state_hash` gets this. (Syntax is a placeholder.)
    @comptime
    default for<T: struct> {
        fn state_hash(self, h: mut Hasher) {
            for field in T.fields() {             // compile-time reflection; unrolled
                self.[field].state_hash(mut h)
            }
        }
    }
}

impl StateHash for f32 {
    fn state_hash(self, h: mut Hasher) {
        h.write_u32(self.canonical_nan().bits())  // NaNs canonicalized (D-015)
    }
}
```

**Notes:**
- **The derivation is ordinary wrela, run at compile time** (D-060). The compiler knows how to reflect over a struct's fields. It doesn't know what hashing *is*.
- **`Copy`, `Serialize`, delta compression and the engine's own traits** can define structural defaults the same way.
- **The per-tick checksum hashes raw chunk bytes instead** (§4). That's safe because the bytes are canonical: zeroed padding and canonical NaNs (D-074). `StateHash` is for *diagnosing* a desync: it can say which field differs.

---

## 6. Errors and panics

```wrela
/// Recoverable failures are values. A save file is structural data with stable field IDs,
/// not a memory image, so it survives layout changes and can be migrated (D-084).
fn load_save(bytes: [u8]) -> Result<Region<World>, SaveError> {
    let header = SaveHeader::parse(bytes)?
    let data = migrate(bytes.after(header), from: header.version)?
    Region::build(|r| World::deserialize_in(r, data))   // fields validated as they're read: saves are untrusted
}

/// Bugs panic.
@deterministic
fn scare(world: mut World, h: Handle<GrazerSim>) {
    world.grazers[h].gait.mode = Mode::Flee      // a stale handle panics: that's a bug, not an error
}
```

**Notes:**
- **A panic in `@deterministic` code is a deterministic trap.** Every client traps on the same tick.
- **The engine turns that into a gift for agents:**
  1. the platform host catches the trap
  2. the engine rewinds to the tick's checkpoint, so the world is consistent again
  3. it writes a **repro bundle**: a keyframe plus the inputs since, tagged with the build (keyframes are same-build only)
  4. replaying the bundle reproduces the bug exactly, every time
- **GPU code can't panic.** For an out-of-bounds index, debug builds set an error flag and release builds clamp (D-074).
- **The first draft made save files keyframes.** A keyframe is a memory image, so every type's layout would have become the save format, and validating an untrusted image is a security surface (audit M8). Keyframes are now for rollback, repro bundles and network sync within one build.

---

## 7. What gets rejected, collected

| Code | Error | Rule |
|---|---|---|
| `struct Herd { leader: &GrazerSim }` | `&` isn't a type; store a handle | D-064 |
| `step(mut g, world, ...)` inside `for mut g in world.grazers` | `world` overlaps a live `mut` borrow | D-058 |
| `fn dangling() -> borrow Terrain` projecting a local | projections must come from parameters | D-058 |
| `struct World: SimState { log: Vec<Edit> }` | `Vec` owns heap memory, so `World` wouldn't be `Relocatable` | D-065 |
| `struct World: SimState { look: GrazerLook }` | engine message: presentation data can't be sim state | D-078, D-055 |
| `var saved = world.terrain.edits.clone()` | `edits` is region-bound and can't be owned outside its region | D-084 |
| `clock::now()` inside `tick` | nondeterministic inside `@deterministic` | D-052 |
| `var g = take world.grazers[h]` | can't move out of a projection; use `.clone()`, or `world.grazers.remove(h)` | D-064 |

---

## 8. Resolved questions

| | Question | Outcome |
|---|---|---|
| **Q20** | Reference model | Second-class references (D-058), amended to parameter modes and non-escaping types (D-064) |
| **Q21** | Copies and moves | `Copy` for small types, `.clone()` otherwise (D-059, D-083). Amended so that moving out of a named place is written `take` (D-064). |
| **Q22** | Structural implementations | Compile-time reflection (D-060) |
| **Q23** | Errors | `Result` with `?`; panics for bugs (D-061) |
| **Q24** | Parallel sim | Data-parallel combinators only (D-062) |
| **E1** | Engine: read last tick's world? | Accepted as D-063, then superseded by D-085. There's no previous world under D-066, so only what crosses entities is double-buffered. |

---

## Not covered yet

- Async platform IO (fetch, OPFS). References across `await` are settled: structured concurrency only (D-087).
- Networking: transport, serialization formats, interest management
- A deterministic rigid-body solver
- Modules and packages
- Strings and text
