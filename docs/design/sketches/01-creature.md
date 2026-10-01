# Sketch 01: a creature as a field

*Status: draft, 2026-10-01. Questions Q1–Q12 were resolved the same day as D-025–D-036 in [../decisions.md](../decisions.md). Revised for D-050–D-053 (the compiler knows nothing about the engine), for the [2026-10-01 audit](../reviews/2026-10-01-audit.md) (D-067–D-088), and for scoped facts (D-092). The syntax is still imagined. Nothing here is decided unless it cites a decision.*

This is the code we'd want to write for the creature milestone.

**The creature:** a *grazer*, a large quadruped herbivore.
- Four legs on uneven ground: the milestone itself.
- A long neck and a tail: bone chains and secondary motion.
- Herds: variation from a seed.

**Conventions:**
- Rust-family syntax. Newlines end statements; only a leading `.` continues a line (D-038, D-079).
- Memory ([memory-model.md](../memory-model.md)):
  - parameters are borrowed by default
  - `mut` marks exclusive mutable access, at the call site too
  - `take` moves
  - `var` declares an owned mutable local
- Named arguments are optional; positional ones come first (D-039).
- Files use the `.wrela` extension (D-040).
- Units (D-025, D-076):
  - `15cm` is shorthand for `15 * cm`; suffixes live in their own namespace, so a local named `m` or `s` can't shadow them
  - compound units are arithmetic: `1050 * kg/m**3`
  - types are written `f32<kg/m**3>`
- Function properties are attributes: `@comptime`, `@deterministic`, `@audio`, `@assert(...)`, `@assume(...)` (D-035, D-052, D-077).
- **Layers** (D-050):
  - `Field`, the kinds (`Exact`, `Bound`, `Lipschitz`), units and `Rng` are stdlib (`std::`).
  - `Creature`, `Skeleton`, `Pose`, `GaitNet`, `CreatureLook`, `SimState` and acoustics are **engine** types, built from the language and stdlib with no compiler support.
  - The grazer itself is **game** code.
- `vec3(y: 1.3m)`: unnamed components default to zero.

---

## 1. Channels: what a point knows about itself

```wrela
use std::field::{Field, Exact, Bound, Lipschitz, Blend, Cat, ellipsoid, round_cone, half_space, fbm}
use std::units::{m, cm, mm, kg, s}
use engine::acoustics::Absorption              // acoustics is engine, not stdlib (D-081)

/// What a point on or inside the grazer knows, besides its distance to the surface.
struct Tissue: Blend {
    albedo:     Color,           // blends in linear space
    roughness:  f32,             // blends linearly
    density:    f32<kg/m**3>,    // read by physics
    absorption: Absorption,      // read by acoustics
    material:   Cat<Material>,   // categorical: taken from the winning surface, never averaged
}

const HIDE = Tissue {
    albedo:     rgb(0.42, 0.33, 0.24),
    roughness:  0.7,
    density:    1050 * kg/m**3,
    absorption: Absorption::SOFT_TISSUE,
    material:   Cat(Material::Hide),
}

const HOOF = Tissue {
    albedo:     rgb(0.12, 0.10, 0.09),
    roughness:  0.35,
    density:    1300 * kg/m**3,
    absorption: Absorption::KERATIN,
    material:   Cat(Material::Keratin),
}
```

**Notes:**
- `Tissue` is a plain struct that opts in to `Blend` (D-026). Each member's type decides how it blends, and the struct's blending is the composition of its members'.
- Each consumer reads only its own channels:
  - the renderer reads `albedo`, `roughness` and `material`
  - physics reads `density`
  - audio reads `absorption`

  The compiler slices out the rest (D-002).

---

## 2. Parts: small fields in bone space

```wrela
/// Torso, in the chest bone's space. +z is forward, +y is up.
@assert(lipschitz <= 1.5, near: 10cm)   // checked near the surface; the constant is compiler-derived (D-077, D-092)
fn torso(bulk: f32, seed: Seed) -> Field<Lipschitz, Tissue> {
    ellipsoid(radii: vec3(0.45m, 0.50m, 0.90m) * bulk)          // no exact SDF exists for ellipsoids: a `Bound`
        .smooth_union(
            ellipsoid(radii: vec3(0.38m, 0.42m, 0.50m) * bulk).translate(z: -0.7m),   // haunch
            k: 15cm)
        .displace(fbm(freq: 25 / m, octaves: 4), amp: 3mm)      // wrinkles: now `Lipschitz`, with L > 1
        .with(|p| Tissue { albedo: dapple(p, seed), ..HIDE })   // channels can vary over space
}

/// One leg segment hanging down its bone's -y axis. A round cone is an exact SDF.
fn leg_segment(len: f32<m>, r_top: f32<m>, r_bottom: f32<m>) -> Field<Exact, Tissue> {
    round_cone(vec3(), vec3(y: -len), r_top, r_bottom).with(HIDE)
}

/// Intersection only yields a bound, so declaring this `Exact` would be a compile error.
fn hoof() -> Field<Bound, Tissue> {
    round_cone(vec3(), vec3(y: -8cm), r_top: 6cm, r_bottom: 7cm)
        .intersect(half_space(normal: vec3(y: 1), offset: -7cm))   // flat sole
        .with(HOOF)
}
```

**Notes:**
- **Combinators are methods** (D-028). Smoothing parameters and kind changes stay visible.
- **Field kinds are ordinary stdlib types** (D-056, D-077). Every op states in its return type what it preserves:
  - `smooth_union` and `intersect` turn `Exact` into `Bound`: never overestimates, Lipschitz constant ≤ 1.
  - `displace` turns either into `Lipschitz`: sign-correct, but L may exceed 1, so it can overestimate.
  - `.to_bound()` turns a `Lipschitz` field back into a `Bound` by dividing by its derived L (`facts::lipschitz`).
  - `.with(...)` only touches channels, so the kind is unchanged.
  - `Exact` converts to `Bound`, and `Bound` to `Lipschitz`, implicitly. Never the other way.
- **Filtering:** `fbm` declares `@assume(bandlimit: ...)`, so shading can fade octaves that are finer than a pixel instead of aliasing (D-077).
- **Closures are allowed** (`|p| ...`) even though fields end up on the GPU. A field's structure is its type (D-070), and closures are monomorphized and inlined at build time, so no closure is left in the shader.
  - Fields built at runtime from an unbounded space use the stdlib's interpreted evaluator, `stage::interpret` (D-029).

---

## 3. Skeleton and assembly

```wrela
/// Built at compile time, so bone names are checked.
@comptime
fn grazer_skeleton() -> Skeleton {
    var s = Skeleton::new()
    let pelvis = s.root("pelvis", at: vec3(y: 1.3m))
    let chest  = s.bone("chest", parent: pelvis, at: vec3(z: 1.1m))
    let neck   = s.chain("neck", parent: chest, joints: 4, span: vec3(y: 0.6m, z: 0.5m))
    s.bone("head", parent: neck.last(), at: vec3(z: 0.15m))
    s.chain("tail", parent: pelvis, joints: 6, span: vec3(y: -0.3m, z: -0.9m))
    for side in [Side::Left, Side::Right] {
        s.leg("fore", side, parent: chest,  at: vec3(x: side.sign() * 0.3m, y: -0.2m),
              segments: [45cm, 42cm, 20cm])
        s.leg("hind", side, parent: pelvis, at: vec3(x: side.sign() * 0.3m, y: -0.1m),
              segments: [50cm, 45cm, 22cm])
    }
    s
}

/// One grazer. Each member of a herd uses a different seed, so variation costs zero bytes.
pub fn grazer(seed: Seed) -> Creature<Tissue> {
    var rng = Rng::new(seed)
    let bulk = rng.range(0.9, 1.15)
    let s = grazer_skeleton().scaled(rng.range(0.92, 1.08))   // structure known at compile time, scale per individual

    Creature::new(take s)                // the creature owns its skeleton from here on
        .part("chest", torso(bulk, seed))     // bone names are checked against the skeleton (D-031)
        .part("neck",  neck_tube(r_base: 22cm * bulk, r_tip: 12cm))   // a part can span a whole chain
        .part("head",  head(seed))
        .part("tail",  tail())
        .each_leg(|leg| leg.fill(|seg| leg_segment(seg.len, 9cm * bulk, 6cm)).foot(hoof()))
        .blend(k: 6cm)       // smooth-union across joints: organic joints for free
}
```

**Notes:**
- **Binding times mix within a single value.** The skeleton's *structure* is known at compile time, so `"chest"` is checked against it (D-031). Its *scale* is only known when `grazer` runs.
- **`take s` hands the skeleton to the creature** (D-064). Using `s` afterwards would be an error, which is why bones are named on the builder, not looked up through `s`.
- **Every grazer has the same type** (D-070). A field's structure is its type, so the whole herd shares one set of pipelines, and the seed-derived numbers reach the GPU as data.
  - *When* individuals are realized (level load, spawn) is the engine's decision.
  - `grazer` is exported, so its effects are stated at the boundary (D-030). It has none, so there's nothing to write.
  - `Creature<Tissue>` in return position names one concrete type that the compiler infers (D-070).
- **Private helpers rely on inference** (D-030). The `@assert` on `torso` is optional.
- **`Creature<Tissue>`'s kind is inferred.** `.blend` smooth-unions parts including the torso's `Lipschitz` field, so the creature is `Lipschitz`. Anything that sphere-traces it calls `.to_bound()` first.

---

## 4. Physical properties belong to the definition

```wrela
/// What the simulation sees. These values change gameplay, so they're part of what the
/// grazer *is*, not how it's drawn. They must come out bit-identical on every client (D-015).
@deterministic
pub fn grazer_physique(c: Creature<Tissue>) -> Physique {
    Physique {
        mass:      c.integrate(|t| t.density, per: Bone, finest: 1cm), // adaptive; mass, center of mass, inertia per bone
        collision: c.fit_capsules(per: Bone, tolerance: 1cm),          // cheap shapes to re-simulate on rollback
    }
}
```

**Notes:**
- **Anything the sim reads is computed by `@deterministic` code** (D-052), so it's bit-identical on every client.
  - A 1cm vs. 2cm integration grid gives a different mass. A different mass gives a different simulation, and a different simulation means a desync.
  - So `finest` and `tolerance` are part of the gameplay, and they live here, in the definition.
- **Integration is adaptive** (T3). Intervals classify cells as fully inside, fully outside or straddling, and only straddling cells are refined.
  - That makes the cost proportional to surface area: roughly 100,000 cells for a grazer, instead of the 15 million a uniform 1cm grid would take (an estimate).
  - It runs once per individual, at spawn.
  - The D-067 spike measures it against the sim budget (D-068).
- **Draw settings can't leak in.** `GRAZER_LOOK` (§5) only feeds GPU work. Reading GPU results requires readback, which `@deterministic` forbids.
- **The engine decides when this runs,** for example once at spawn. Nothing about it is a language-level "load".

---

## 5. Look: how the grazer is drawn

```wrela
/// Engine configuration: an ordinary struct value. Unmentioned fields keep the engine's
/// defaults (D-048), and the engine's quality profiles may override any of them per device.
pub const GRAZER_LOOK = CreatureLook {
    surface: Mesh { tolerance: 1mm },                  // up close: rasterized triangles (D-001)
    deform:  Skin { weights: ByPart { falloff: 6cm } }, // D-023 default
    far:     Some(Bricks { voxel: 3cm, beyond: 40m }),
    shadow:  ShadowMap { resolution: 1024 },           // it deforms: shadow maps, not distance fields (D-086)
}
```

**Notes:**
- **This used to be a `schedule` block.** That was engine vocabulary disguised as a language construct (D-050).
  - Struct field defaults, a general feature, make it read just as cleanly.
  - The algorithm/look separation is now an engine pattern (D-053).
- **Per-individual baking needs no syntax.** `grazer(seed)` is pure, and the engine realizes each individual once.

---

## 6. Simulation and presentation

```wrela
/// Sim state. Lives in the world's region and is checkpointed every tick (D-066).
/// `SimState` is a declared engine trait with a structural check (D-078): every field must be
/// sim state too.
struct GrazerSim: SimState {
    kind:       CreatureKey,    // definition + seed: a deterministic key, never a handle outside the region (D-084)
    tier:       SimTier,        // Full: pose lives in sim. Ambient: root + phase only (D-034)
    rng:        Rng,            // per-grazer stream, seeded from (world seed, handle): parallel-safe
    root:       Transform,
    gait:       GaitState,      // phase, speed, mode
    pose:       Pose,           // gameplay pose: hitboxes, foot contacts (Full tier only)
}

/// Presentation state. Never rolled back; may differ between clients.
struct GrazerLook {
    pose: Pose,                 // Ambient tier: the full pose is computed here instead
    tail: SpringChain,
    ears: [Spring; 2],
}

/// Small learned controller, trained offline. Quantized weights with recorded provenance (D-036).
const GAIT = GaitNet::embed("grazer_gait.wnn")

/// Takes the terrain, not the whole world: the grazer lives inside the world, so mutating it
/// while borrowing the world would overlap (sketch 03 §2).
@deterministic
fn step(g: mut GrazerSim, terrain: Terrain, intent: Intent) {
    g.gait = GAIT.eval(g.gait, intent, slope: terrain.slope_under(g.root))
    g.root = g.root.advance(g.gait.root_motion())
    if g.tier == SimTier::Full {
        g.pose = g.gait.pose()
        for mut leg in g.pose.legs() {                                  // each leg: a mutable projection of its own bones
            let ground = terrain.raycast(from: leg.foot_pos(), dir: DOWN)  // field query on the CPU, strict floats
            leg.plant(on: ground)                                       // two-bone IK
        }
    }
}

fn animate(g: GrazerSim, look: mut GrazerLook, world: World, dt: f32<s>) {
    look.pose = match g.tier {
        SimTier::Full    => g.pose.clone(),                           // explicit: `g` is borrowed (D-064)
        SimTier::Ambient => g.gait.pose().planted_on(world.terrain),   // same work, but never rolled back
    }
    look.tail.follow(look.pose["tail"], dt)      // secondary motion is presentation-only
}

fn draw(g: GrazerSim, look: GrazerLook, frame: mut Frame) {
    frame.draw(g.kind, pose: look.pose.layer(look.tail).layer(look.ears))   // the engine finds the realized mesh by key
}
```

**Notes:**
- **The sim/presentation split is an engine pattern** (D-052).
  - The engine calls `step` with `mut` access to sim state.
  - It calls `animate` and `draw` with sim state borrowed, which is the default mode and read-only.
  - Presentation functions need no attribute. They're just ordinary code.

**Two bugs the memory model caught here** ([memory-model.md](../memory-model.md)):
- **The first draft** looped `for foot in g.pose.feet()` while calling `g.pose.plant(...)` in the body. That mutates the pose while iterating over it, and exclusivity rejects it. Iterating `legs()` as disjoint mutable projections is the fix.
- **`animate` wrote `look.pose = g.pose`.** That moves out of borrowed sim state, so it now says `.clone()`, and the cost of the copy is visible.

What gets rejected, and by whom:

```wrela
@deterministic
fn bad_step(g: mut GrazerSim) {
    let t = clock::now()            // compiler: reading the clock is nondeterministic (D-052)
}

fn bad_draw(g: GrazerSim, look: GrazerLook, frame: mut Frame) {
    g.root.y += look.tail.sway      // compiler: `g` is borrowed, not `mut` (ordinary mutability)
}

struct CheatingSim: SimState {
    look: GrazerLook                // engine: "`GrazerLook` is presentation data and can't be stored
}                                   //          in sim state" (structural check D-078 + message D-055)
```

The compiler catches the first two with general rules. It catches the third only because the engine defined `SimState`, `GrazerLook` never declared it, and the engine wrote the message. The compiler doesn't know what sim state *is*.

---

## 7. Sound

```wrela
/// The hoof's vibration modes, from its field (shape + density). This is an eigenvalue solve,
/// so it's written as a `const` and runs at compile time. Staging is never left to the
/// optimizer (D-072).
const HOOF_MODES = modal_modes(hoof())

/// A hoof hits the ground.
@audio
fn hoof_strike(hit: Contact) -> Voice {
    modal(HOOF_MODES, against: hit.surface.absorption, impulse: hit.impulse)
}
```

**Notes:**
- The same field that renders the hoof decides how heavy it is and how it sounds.
- `hit.surface.absorption` is the terrain field's acoustic channel at the contact point.
- **The first draft relied on the compiler hoisting** `modal(shape: hoof(), ...)` out of the audio callback. If that hoisting ever failed silently, an eigenvalue solve would run on the audio thread. Now the staging is written explicitly (F9).

---

## Not covered yet

- Shading and the material model
- LOD transitions and seams
- Herds and instancing
- The terrain definition
- The gait controller's internals
- Hot reload
- What the agent-facing queries look like
