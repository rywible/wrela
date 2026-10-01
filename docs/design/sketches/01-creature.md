# Sketch 01: a creature as a field

*Status: draft, 2026-10-01. Questions Q1–Q12 were resolved the same day as D-025–D-036 in [../decisions.md](../decisions.md). Revised for D-050–D-053: the compiler knows nothing about the engine. The syntax is still imagined. Nothing here is decided unless it cites a decision.*

This is the code we'd want to write for the creature milestone.

**The creature:** a *grazer*, a large quadruped herbivore.
- Four legs on uneven ground: the milestone itself.
- A long neck and a tail: bone chains and secondary motion.
- Herds: variation from a seed.

**Conventions:**
- Rust-family syntax. Newlines end statements (D-038).
- Named arguments are optional; positional ones come first (D-039).
- Files use the `.wrela` extension (D-040).
- Units are library constants (D-025):
  - `15cm` is shorthand for `15 * cm`
  - compound units are arithmetic: `1050 * kg/m^3`
  - types are written `f32<kg/m^3>`
- Function properties are attributes: `@comptime`, `@specialize`, `@deterministic`, `@audio` (D-035, D-051, D-052).
- **Layers** (D-050):
  - `Field`, `Sdf`, units and `Rng` are stdlib.
  - `Creature`, `Skeleton`, `Pose`, `GaitNet`, `CreatureLook` and `SimState` are **engine** types, built from the language and stdlib with no compiler support.
  - The grazer itself is **game** code.
- `vec3(y: 1.3m)`: unnamed components default to zero.

---

## 1. Channels: what a point knows about itself

```wrela
use wrela::field::{Field, Sdf, SdfBound, Blend, Cat, ellipsoid, round_cone, half_space, fbm}
use wrela::units::{m, cm, mm, kg, s}
use wrela::acoustics::Absorption

/// What a point on or inside the grazer knows, besides its distance to the surface.
struct Tissue: Blend {
    albedo:     Color,           // blends in linear space
    roughness:  f32,             // blends linearly
    density:    f32<kg/m^3>,     // read by physics
    absorption: Absorption,      // read by acoustics
    material:   Cat<Material>,   // categorical: taken from the winning surface, never averaged
}

const HIDE = Tissue {
    albedo:     rgb(0.42, 0.33, 0.24),
    roughness:  0.7,
    density:    1050 * kg/m^3,
    absorption: Absorption::SOFT_TISSUE,
    material:   Cat(Material::Hide),
}

const HOOF = Tissue {
    albedo:     rgb(0.12, 0.10, 0.09),
    roughness:  0.35,
    density:    1300 * kg/m^3,
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
@lipschitz(max: 1.5)        // optional assertion; the bound itself is compiler-derived (D-027)
fn torso(bulk: f32, seed: Seed) -> Field<SdfBound, Tissue> {
    ellipsoid(radii: vec3(0.45m, 0.50m, 0.90m) * bulk)          // no exact SDF exists for ellipsoids: a bound
        .smooth_union(
            ellipsoid(radii: vec3(0.38m, 0.42m, 0.50m) * bulk).translate(z: -0.7m),   // haunch
            k: 15cm)
        .displace(fbm(freq: 25 / m, octaves: 4), amp: 3mm)      // wrinkles; the Lipschitz bound grows
        .with(|p| Tissue { albedo: dapple(p, seed), ..HIDE })   // channels can vary over space
}

/// One leg segment hanging down its bone's -y axis. A round cone is an exact SDF.
fn leg_segment(len: f32<m>, r_top: f32<m>, r_bottom: f32<m>) -> Field<Sdf, Tissue> {
    round_cone(vec3(), vec3(y: -len), r_top, r_bottom).with(HIDE)
}

/// Intersection only yields a bound, so declaring this `Sdf` would be a compile error.
fn hoof() -> Field<SdfBound, Tissue> {
    round_cone(vec3(), vec3(y: -8cm), r_top: 6cm, r_bottom: 7cm)
        .intersect(half_space(normal: vec3(y: 1), offset: -7cm))   // flat sole
        .with(HOOF)
}
```

**Notes:**
- **Combinators are methods** (D-028). Smoothing parameters and kind changes stay visible.
- **Field kinds are ordinary stdlib types** (D-056). Every op states in its return type what it preserves:
  - `smooth_union`, `intersect` and `displace` turn an `Sdf` into an `SdfBound`.
  - `.with(...)` only touches channels, so the kind is unchanged.
  - An `Sdf` converts to an `SdfBound` implicitly, never the other way.
- **Closures are allowed** (`|p| ...`) even though fields end up on the GPU. A field is specialized before WGSL is generated, either ahead of time or by the client-side specializer, so no closure is left in the shader (D-051).
  - If a field's structure can't be specialized, the entry point errors with a binding-time trace.
  - Fields that genuinely change at runtime use the stdlib's interpreted evaluator, `stage::interpret` (D-029, D-053).

---

## 3. Skeleton and assembly

```wrela
/// Built at compile time, so bone names are checked.
@comptime
fn grazer_skeleton() -> Skeleton {
    let mut s = Skeleton::new()
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
    let mut rng = Rng::new(seed)
    let bulk = rng.range(0.9, 1.15)
    let s = grazer_skeleton().scaled(rng.range(0.92, 1.08))   // structure known at compile time, scale per individual

    Creature::new(s)
        .part(s["chest"], torso(bulk, seed))
        .part(s["neck"],  neck_tube(r_base: 22cm * bulk, r_tip: 12cm))   // a part can span a whole chain
        .part(s["head"],  head(seed))
        .part(s["tail"],  tail())
        .each_leg(|leg| leg.fill(|seg| leg_segment(seg.len, 9cm * bulk, 6cm)).foot(hoof()))
        .blend(k: 6cm)       // smooth-union across joints: organic joints for free
}
```

**Notes:**
- **Binding times mix within a single value.** The skeleton's *structure* is known at compile time, so `s["chest"]` is checked and returns a typed `Bone` handle (D-031). Its *scale* is only known when `grazer` runs.
- **`grazer` is pure,** so its result can be passed to `@specialize` parameters (sketch 02).
  - *When* that happens (level load, spawn) is the engine's decision. The language only defines what it means (D-051).
  - `grazer` is exported, so its effects are stated at the boundary (D-030). It has none, so there's nothing to write.
- **Private helpers rely on inference** (D-030). The `@lipschitz` on `torso` is just an optional assertion.
- **`Creature<Tissue>`'s kind is inferred.** `.blend` smooth-unions the parts, so it's an `SdfBound`.

---

## 4. Physical properties belong to the definition

```wrela
/// What the simulation sees. These values change gameplay, so they're part of what the
/// grazer *is*, not how it's drawn. They must come out bit-identical on every client (D-015).
@deterministic
pub fn grazer_physique(c: &Creature<Tissue>) -> Physique {
    Physique {
        mass:      c.integrate(|t| t.density, per: Bone, cell: 1cm),   // mass, center of mass, inertia per bone
        collision: c.fit_capsules(per: Bone, tolerance: 1cm),          // cheap shapes to re-simulate on rollback
    }
}
```

**Notes:**
- **Anything the sim reads is computed by `@deterministic` code** (D-052), so it's bit-identical on every client.
  - A 1cm vs. 2cm integration grid gives a different mass. A different mass gives a different simulation, and a different simulation means a desync.
  - So `cell` and `tolerance` are part of the gameplay, and they live here, in the definition.
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
    shadow:  DistanceVolume { voxel: 6cm },
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
/// Sim state. Lives in the sim arena and is snapshotted every tick for rollback.
/// `SimState` is an engine-defined auto trait (D-054): every field must be sim state too.
struct GrazerSim: SimState {
    individual: Handle<Creature<Tissue>>,
    tier:       SimTier,        // Full: pose lives in sim. Ambient: root + phase only (D-034)
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

@deterministic
fn step(g: &mut GrazerSim, world: &World, intent: Intent) {
    g.gait = GAIT.eval(g.gait, intent, slope: world.terrain.slope_under(g.root))
    g.root = g.root.advance(g.gait.root_motion())
    if g.tier == SimTier::Full {
        g.pose = g.gait.pose()
        for foot in g.pose.feet() {
            let ground = world.terrain.raycast(from: foot.pos, dir: DOWN)   // field query on the CPU, strict floats
            g.pose.plant(foot, on: ground)                                  // two-bone IK
        }
    }
}

fn animate(g: &GrazerSim, look: &mut GrazerLook, world: &World, dt: f32<s>) {
    look.pose = match g.tier {
        SimTier::Full    => g.pose,
        SimTier::Ambient => g.gait.pose().planted_on(world.terrain),   // same work, but never rolled back
    }
    look.tail.follow(look.pose["tail"], dt)      // secondary motion is presentation-only
}

fn draw(g: &GrazerSim, look: &GrazerLook, frame: &mut Frame) {
    frame.draw(g.individual, pose: look.pose.layer(look.tail).layer(look.ears))
}
```

**Notes:**
- **The sim/presentation split is an engine pattern** (D-052).
  - The engine calls `step` with `&mut` sim state.
  - It calls `animate` and `draw` with sim state behind `&`.
  - Presentation functions need no attribute. They're just ordinary code.

What gets rejected, and by whom:

```wrela
@deterministic
fn bad_step(g: &mut GrazerSim) {
    let t = clock::now()            // compiler: reading the clock is nondeterministic (D-052)
}

fn bad_draw(g: &GrazerSim, look: &GrazerLook, frame: &mut Frame) {
    g.root.y += look.tail.sway      // compiler: can't assign through `&` (ordinary mutability)
}

struct CheatingSim: SimState {
    look: GrazerLook                // engine: "`GrazerLook` is presentation data and can't be stored
}                                   //          in sim state" (auto trait D-054 + engine message D-055)
```

The compiler catches the first two with general rules. It catches the third only because the engine defined `SimState` and wrote the message. The compiler doesn't know what sim state *is*.

---

## 7. Sound

```wrela
/// A hoof hits the ground. The hoof's vibration modes come from its field (shape + density).
/// They don't depend on the hit, so the compiler hoists that analysis out of the audio
/// callback. `hoof()` is constant, so it can even run ahead of time.
@audio
fn hoof_strike(hit: Contact) -> Voice {
    modal(shape: hoof(), against: hit.surface.absorption, impulse: hit.impulse)
}
```

**Notes:**
- The same field that renders the hoof decides how heavy it is and how it sounds.
- `hit.surface.absorption` is the terrain field's acoustic channel at the contact point.

---

## Not covered yet

- Shading and the material model
- LOD transitions and seams
- Herds and instancing
- The terrain definition
- The gait controller's internals
- Hot reload
- What the agent-facing queries look like
