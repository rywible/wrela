# RFC 0011: Field-Native Traversal Combat, Dynamic Terrain, And Production Game Closure Roadmap

Status: Proposed post-RFC-0010 gameplay, runtime, audio, and representative-game roadmap after full repo read and subagent-backed architecture audit

Author: GPT-5.4 Pro

Created: 2026-04-21

Target: post-RFC-0010 `wrela` language, compiler, runtime, engine-frame scheduling, gameplay loop, procedural audio, representative game project, and macOS desktop ship surface

## Summary

Wrela now has real engine bones.

The repo already contains a serious field language, typed query contracts, a snapshot and artifact spine, a GPU-resident presentation path, collision execution structure, and an engine-frame performance model. That is enough to stop asking, “can this repo render or query interesting field worlds?” The next question is harder and much more important:

**Can Wrela ship a production-grade field-native game whose core loop is high-speed traversal across dramatic hand-authored landscapes, where movement itself is the combat language, bosses deform the terrain under the player, the visuals are startlingly good, and the whole frame still closes at 120 FPS on the target machine?**

Today, the truthful answer is: not yet.

The missing work is not a single “gameplay system.”
It is a missing layer of engine closure:

1. the authored gameplay concepts the repo clearly wants to support such as `generator`, `archetype`, `body`, `move`, `moveset`, and `space` are still more architectural intent than live compiler/runtime surface
2. streamed world composition is structurally present, but executable parameterized regions, live `space` topology, `scatter`, and conditional region execution are not yet closed in the live repo
3. the runtime has snapshots and state-advance contracts, but not yet a production gameplay-frame contract for input, replay, checkpoints, and stable save/load
4. traversal-specific collision, contact, airtime, and attack-through-movement contracts are not yet first-class query families
5. terrain deformation is conceptually aligned with field-based authoring, but it is not yet a first-class mutable-world authority path across gameplay, collision, presentation, and artifact invalidation
6. there is no field-derived audio subsystem yet; the repo already has the right substrate for one, but not yet an audio observer, planner, executor, or real-time runtime
7. there is no permanent representative game project proving that these systems compose honestly under one engine-frame budget

The thesis of this RFC is:

**Wrela should now close the gap from “strong field engine substrate” to “production traversal-combat game engine for a specific reusable game family,” by following the load-bearing seams in the repo today and the first-principles needs of the target game.**

One part of that work is language closure.
Another part is refusing to invent a second world substrate where the repo already has the beginnings of the right one.

For audio in particular, the repo already exposes the important raw ingredients:

- captured world state
- typed query contracts and planners
- `Surface`, `Medium`, and payload-rich world observations
- explicit clocks, ticks, snapshots, and artifact identity
- engine-frame reporting

This roadmap therefore treats audio as a dedicated observer and execution lane over the existing field substrate, not as a new top-level authored world declaration.

That means:

- finish the missing language semantics instead of inventing a parallel gameplay sidecar
- make world generation and streaming executable, deterministic, and save-stable
- add a fixed-step gameplay frame that the engine can replay and measure
- add traversal-combat and boss-deformation contracts on top of the existing query/collision architecture
- add a bounded field-derived audio observer and execution model
- make a representative traversal-boss project a permanent proof surface
- keep performance closure engine-frame truthful, not subsystem-marketing truthful

This RFC is intentionally not a generic “make Wrela support every game.”

It defines the production-grade engine work needed to build one reusable family of games:

- single-player desktop traversal-action games
- field-authored large landscapes with huge silhouettes
- motion-driven combat and airtime attacks
- boss encounters that can reshape the ride surface
- procedural materials, atmosphere, and audio
- 120 FPS engine-frame closure as a non-negotiable target

## Repo Grounding And First-Principles Design Stance

This RFC is reasoned from the shape of the repo today, not from older RFC text as a binding authority.

The live codebase already has clear load-bearing seams:

- `compiler/query_contract`, `compiler/query_plan`, and `compiler/query_exec`
- `compiler/presentation_plan` and `compiler/presentation_exec`
- `compiler/collision_plan` and `compiler/collision_exec`
- `compiler/time_semantics`, `compiler/world_identity`, `compiler/artifact_key`, and `compiler/artifact_store`
- `runtime/src/state_advance.rs`, `runtime/src/domain_abi.rs`, and `runtime/src/engine_executor.rs`
- `compiler/engine_frame`

Those seams already say a lot about what Wrela is becoming:

- world truth is captured, versioned, and replay-relevant
- subsystems are expected to close through contract -> plan -> exec surfaces
- time, identity, and invalidation are explicit engine concepts
- performance is measured at engine-frame wall time, not by subsystem self-report

Earlier RFCs remain useful background and style precedent, but when the repo or the product problem has taught us something better, this roadmap follows repo truth first.

## Current Repo Read

The repo is much stronger than a greenfield engine.

### What is already strong

1. **The authored field language is real.**
   `field`, `shape`, `region`, `domain`, `view`, `material`, `radiance field`, `volume field`, and `system` all exist in the parser, HIR, typecheck, lowering, and current examples.

2. **The query stack already has the right architecture.**
   `compiler/query_contract`, `compiler/query_plan`, and `compiler/query_exec` already separate semantic contracts, executable plans, backend selection, and observability.

3. **Presentation is no longer toy-grade.**
   `compiler/presentation_plan` and `compiler/presentation_exec` already encode frame contracts, outputs, temporal history, authored lighting inputs, material/radiance/medium resolution, GPU primary/gpu post paths, framegraph execution, cost reporting, and quality control.

4. **Snapshots, time, and artifact identity already exist.**
   `compiler/time_semantics`, `compiler/world_identity`, `compiler/artifact_key`, `compiler/artifact_store`, and `runtime/src/state_advance.rs` already provide the right vocabulary for deterministic runtime state and derived execution artifacts.

5. **Collision already has typed plans and throughput infrastructure.**
   `compiler/collision_plan`, `compiler/collision_exec`, candidate tables, batch support, and WGSL helper execution already exist and are no longer just conceptual placeholders.

6. **The engine frame is now a first-class performance surface.**
   `compiler/engine_frame` and the closure tooling added in RFC 0010 mean new subsystems can be measured as part of one frame instead of as isolated demos.

7. **The runtime already has a host boundary.**
   `runtime/src/domain_abi.rs`, `runtime/src/engine_executor.rs`, and the virtual GPU/runtime executor layers mean there is already a place for a real game loop to live.

### What is only partially closed

1. **The language still stops short of several gameplay-native declarations this engine now clearly needs.**
   `generator`, `archetype`, `body`, `move`, `moveset`, and `space` are not yet all live parser-to-runtime declarations in the current repo.

2. **Region composition exists, but executable streaming composition is not finished.**
   The live repo still rejects parameterized regions, does not yet make `space` a live authored topology surface, and `compiler/query_exec/region.rs` still returns explicit runtime errors for `scatter` and conditional region execution.

3. **The runtime has fixed-step state machinery, but not yet a production gameplay frame contract.**
   There is a strong `state_advance` spine, but no complete story yet for player input, replay, checkpointing, deterministic game saves, or representative encounter playback.

4. **Presentation and collision are strong, but not yet game-family aware.**
   The repo can render and query rich field worlds, but traversal contacts, airtime attack windows, ride-surface queries, and boss deformation are not yet first-class engine contracts.

5. **The engine frame can measure subsystems, but not all necessary subsystems exist yet.**
   Presentation and collision have meaningful reports. Gameplay and audio do not.

### What is missing for this game family

1. **Executable authored world generation, `space` topology, and streaming for giant traversal spaces**
2. **A deterministic movement-combat solver built around `body`, `move`, and `moveset`**
3. **A world-mutation path for boss-authored terrain deformation through live `terrain` and `deform` semantics**
4. **Production PBR material, lighting, and soft-shadow closure**
5. **Beautiful sky, atmosphere, and scenic-depth closure for giant traversal vistas**
6. **Traversal-readable hero FX, lookdev governance, and visual perf closure**
7. **A field-derived procedural audio subsystem**
8. **A representative game project that proves the engine shape under real content**
9. **A shipping loop for macOS desktop that exercises this game honestly end to end**

## Supported Game Family

This RFC targets a specific family of games.

The family is:

- single-player desktop traversal-action games
- movement-led combat where jumps, carving, airtime, and surface reading matter
- large field-authored worlds with dramatic silhouettes and sparse but striking encounter structures
- bosses or encounter actors that can deform or replace parts of the traversal surface
- strong atmosphere, lighting, and procedural audio as part of readability and spectacle
- deterministic fixed-step gameplay with representative performance closure at 120 FPS

This RFC deliberately does not optimize for:

- network replication
- open-ended NPC crowds
- dialogue or quest systems
- inventory-heavy RPG structures
- firearm ballistics as the main combat language
- general-purpose asset import pipelines for meshes, rigs, or animation clips
- genre-general engine abstractions that this game family does not need

The guiding product principle is:

**Make Wrela excellent for field-native traversal-combat games, and reusable across adjacent games in that family, instead of prematurely generalizing toward every possible game.**

## Why This Comes Before Building The First Game

If the first representative traversal game is built before this closure work lands, the repo will accumulate exactly the wrong kind of “progress”:

- bespoke runtime state bridges instead of real authored language declarations
- encounter-specific terrain hacks instead of a world-mutation contract
- ad hoc player probes instead of traversal query families
- disconnected audio experiments instead of a first-class observer/planner/runtime lane
- a spectacular demo that does not become a reusable engine surface

That would be faster in the short term and more expensive everywhere afterward.

This RFC instead creates the reusable production engine shape first, then proves it with one representative game project that remains in the repo as a permanent truth surface.

## Goals

1. Make the authored gameplay declarations this engine needs live in the real compiler/runtime pipeline.
2. Close executable world generation and streaming for large traversal worlds without giving up deterministic identity.
3. Add a fixed-step gameplay frame contract that cleanly joins input, state advance, replay, checkpoints, and engine-frame reporting.
4. Add traversal-specific query and collision contracts for carving, landing, airtime, and movement-led attack reach.
5. Add boss-authored terrain deformation as a first-class mutable-world authority path.
6. Close PBR materials, lighting, soft shadows, sky, atmosphere, and traversal-readable hero FX to a production visual standard.
7. Add a bounded field-derived audio subsystem that participates in the engine frame.
8. Preserve the repo’s existing contract -> plan -> exec architecture instead of bypassing it with game-specific shortcuts.
9. Keep the engine reusable for nearby traversal-action games without adding subsystems unrelated to that family.
10. Maintain truthful 120 FPS closure as an engine-frame wall-time target, not a subsystem-only target.
11. Produce a permanent representative game project and ship surface on macOS desktop.

## Non-Goals

1. This RFC does not make Wrela a general-purpose engine for every genre.
2. This RFC does not add imported mesh, skeletal animation, or texture authoring as primary content paths.
3. This RFC does not require multiplayer, networking, or rollback netcode.
4. This RFC does not require console backends or non-`wgpu` native graphics APIs.
5. This RFC does not promise a fully general rigid-body physics sandbox.
6. This RFC does not require full cinematic toolchains, dialogue systems, or open-world systemic AI.
7. This RFC does not allow audio, gameplay, or deformation to bypass the existing snapshot/artifact/perf model.
8. This RFC does not accept a flashy vertical slice as completion evidence.

## Design Rules

1. **Reuse existing engine seams.**
   New gameplay and audio work must fit the repo’s established `contract -> plan -> exec` pattern wherever that pattern is already load-bearing, especially across `query_*`, `presentation_*`, `collision_*`, `time_semantics`, `world_identity`, `artifact_*`, and `engine_frame`.

2. **CPU-authoritative gameplay truth remains explicit.**
   GPU execution may accelerate presentation, traversal queries, and selected audio analysis, but gameplay authority, replay correctness, and save stability must remain auditable on the CPU path.

3. **No hidden asset escape hatch.**
   Do not “solve” traversal characters, bosses, or landscapes by quietly introducing imported mesh, rig, or animation assets as the real source of truth.

4. **World mutation must be versioned and observable.**
   Boss-authored terrain changes must carry stable identity, invalidation scope, and replay/save semantics. Silent mutation is a bug.

5. **Audio is its own subsystem, not its own world model.**
   Audio must have dedicated contracts, plans, execution, runtime, and perf reporting, but it should derive from the same captured world substrate that rendering, collision, and gameplay already use.

6. **Representative game content is permanent proof, not throwaway demo code.**
   The first traversal-boss project should remain in the repo as the canonical end-to-end proof surface.

7. **Game-family reuse beats genre-general abstraction.**
   Add abstractions only when they clearly support multiple traversal-action games in this field-native family.

8. **120 FPS is an engine-frame wall-time promise.**
   Reporting and closure continue to use whole-frame throughput time as the authoritative target.

9. **Optional GPU-specialized paths remain optional.**
   Subgroup kernels, `f16`, and other aggressive WGSL optimizations may matter for closure, but they must stay feature-gated and parity-tested rather than becoming the correctness path.

10. **Mac-first does not mean architecture debt.**
    It is acceptable to target the user’s Mac first, but not acceptable to hard-code Metal-only game semantics into higher layers of the engine.

## Key Architectural Definitions

### Traversal Body

A dynamic body whose primary authored purpose is to read and exploit the ride surface.
Examples: snowboard, board-and-rider composite, sand-skimmer, sword-surf platform.

### Ride Surface

The gameplay-relevant subset of world geometry that contributes to carving, landing, launch, traction, and traversal readability.
It is not identical to “all visible geometry.”

### Air Window

A deterministic interval in which movement state, contact history, and attack reach combine to make an aerial attack legal or illegal.

### Boss Terrain Authority

The explicit subsystem that allows a boss encounter to deform, replace, or overlay traversal-relevant terrain while preserving world identity, query correctness, and artifact invalidation.

### Audio Observer

The control-rate audio evaluation context derived from listener pose, velocity, traversal state, encounter state, world snapshot identity, and bounded world observations such as surface, medium, occlusion, and deformation signals.

### Representative Game Project

A permanent in-repo project that exercises traversal, deformation, presentation, audio, replay, and shipping surfaces together.
It is not a temporary benchmark scene.

## End State Of This Roadmap

At the end of this RFC, Wrela should be able to host a traversal-action game with the following shape:

- giant field-authored dunes, cliffs, or ruins stream in deterministically around the rider
- the rider’s board/body uses first-class traversal contact queries rather than ad hoc probes
- authored moves and movesets govern aerial attacks and recovery windows
- bosses contribute geometry, radiance, media, and terrain deformation through first-class authored declarations
- audio is derived from field-aware observation rather than an unrelated middleware sidecar
- the whole system can be replayed, checkpointed, benchmarked, and shipped on the target machine
- the engine frame can honestly report where the frame budget is going

Illustrative authored end state.
Audio is intentionally not shown below as a new top-level authored declaration.
In this roadmap, Phase 68 derives audio from captured world state and gameplay state instead of introducing a separate authored audio substrate.

```wr
generator DuneRibbon(key: CellKey, seed: U64, encounter: EncounterPhase) {
    support = dune_band_support(key)
    detail coarse field conservative distance(p: Vec3) -> F32 { ... }
    detail fine field conservative distance(p: Vec3) -> F32 { ... }
    material surface(hit: Hit3) -> Surface { ... }
    volume field atmosphere(p: Vec3, surface_distance: F32) -> Medium { ... }
}

body RiderBoard(instance: BoardState) {
    mass = 14.0
    inertia = board_inertia(instance.stance)
    collision detail exact distance(p: Vec3) -> F32 { ... }
    material surface(hit: Hit3) -> Surface { ... }
}

move LaunchStrike(rider: RiderState, board: BoardState, boss: BossState) {
    duration = 0.64

    phase gather[0.00..0.24] { ... }
    phase launch[0.24..0.40] { ... }
    phase strike[0.40..0.52] { ... }
    phase recover[0.52..0.64] { ... }
}

moveset RiderCombat {
    idle = CarveFlow
    on jump_attack when Combat.state.target_locked => LaunchStrike
    on recover when Combat.state.airborne => AirRecover
}

archetype SandLeviathan(instance: BossState) {
    transform = instance.transform
    coarse_support = leviathan_coarse_support(instance)
    tight_support = leviathan_tight_support(instance)

    terrain detail coarse field conservative distance(p: Vec3) -> F32 { ... }
    geometry detail fine field conservative distance(p: Vec3) -> F32 { ... }
    material surface(hit: Hit3) -> Surface { ... }
    radiance field emission(p: Vec3, direction: Vec3, feature_id: U32) -> Vec3 { ... }
    payload = boss_payload(instance)
}

region EncounterBasin(key: CellKey, seed: U64, encounter: EncounterPhase) {
    place terrain = DuneRibbon(key=key, seed=seed, encounter=encounter)
    scatter shards { ... }
    if encounter == boss_awake { ... }
}

space TraversalWorld {
    streamed basins: SparseBands[EncounterBasin] radius 6 follow Rider.state.cell
    dynamic board: Singleton[BoardState] using RiderBoard
    dynamic boss: Singleton[BossState] using SandLeviathan
}

domain traversal_collision(world: RegionCapture) {
    geometry_detail = 1
    material = false
    max_distance = 36.0
    min_step = 0.01
    hit_epsilon = 0.0005
    max_steps = 128
}

view traversal_view(world: RegionCapture, camera: Camera) {
    domain = traversal_presentation(world = world)
    viewport = viewport(width = 1920, height = 1080)
    quality = realtime_quality(target_fps = 120)
    outputs = frame_outputs(color = true, depth = true, normal = true, motion = true)
    history = temporal_history(color = true)
}
```

A Phase-68 audio plan observes this world through existing capture/query surfaces plus traversal and encounter state to drive wind, carve, landing, deformation, and boss-presence layers.

## Project-Level Acceptance Criteria

This RFC is complete when all of the following are true:

1. `generator`, `archetype`, `body`, `move`, `moveset`, and `space` exist as live compiler-supported authored declarations with parser, HIR, typecheck, lowering, and targeted tests.
2. Parameterized regions, `scatter`, and conditional region execution work in the live executable world path instead of being parser-only or error stubs.
3. The runtime has a fixed-step gameplay frame contract with explicit input, replay, checkpoint, and save semantics.
4. Traversal query families exist for ride-surface contact, landing, airtime, and attack-through-movement legality.
5. Boss terrain deformation is represented as a first-class world-mutation path with bounded invalidation and replay stability.
6. A field-derived audio subsystem exists, is measured inside the engine frame, and derives from existing captured world state rather than a second authored world model.
7. Production visual closure exists for PBR surfaces, soft shadows, sky/atmosphere, and traversal-readable hero FX, with canonical lookdev and engine-frame evidence.
8. A permanent representative traversal-boss project exists in the repo and exercises the full stack.
9. The representative project can be previewed, replayed, benchmarked, and shipped through documented repo lanes.
10. `just test`, the relevant focused lanes, the representative perf lane, and `just ship` either pass or record an honest machine limitation.
11. The canonical performance story for this game family is engine-frame wall time at 120 FPS, not isolated subsystem success.

## Phase Overview

- **Phase 63: Gameplay Language Surface And Semantic Closure** — make the missing gameplay-authored declarations real
- **Phase 64: Executable World Composition, `space` Topology, And Deterministic Generation** — finish the streamed world model needed for giant traversal landscapes
- **Phase 65: Gameplay Frame Input, Replay, Save, And Host Runtime Closure** — add a production gameplay-frame contract
- **Phase 66: Traversal Bodies, Ride Contact, And Movement-Combat Contracts** — make traversal and attack-through-movement first-class engine queries
- **Phase 67: Boss Terrain Authority, Deformation, And Encounter Closure** — add mutable terrain and encounter-specific world authority
- **Phase 68: Field-Derived Procedural Audio** — add the bounded audio subsystem this game family needs
- **Phase 69: PBR Materials, Lighting, And Soft Shadow Closure** — close the physically based lighting foundation
- **Phase 70: Sky, Atmosphere, And Scenic-Scale Depth Closure** — make the world-scale vistas beautiful
- **Phase 71: Traversal Readability, Hero FX, And Visual Governance** — make the spectacle legible, reviewable, and honest
- **Phase 72: Representative Game Project, Ship Surface, And Final Closure** — make the whole thing permanent, testable, and shippable

---

# Phase 63: Gameplay Language Surface And Semantic Closure

## Goal

Make the authored gameplay declarations this engine needs real in the live compiler and identity pipeline.

## Why this phase exists

The repo already has a strong authored language, but the missing gameplay-native declarations are exactly where a rushed game project would start cheating.

Do not build runtime-only shadows of `generator`, `archetype`, `body`, `move`, `moveset`, or `space`.
Make those declarations first-class now.
Audio intentionally does not belong in this parser phase; Phase 68 will derive it from the existing world substrate instead of inventing new top-level syntax.

### Workstream A: Parser, AST, HIR, and type system

#### Task 63A1 — Add live parser/AST/HIR support for `generator`, `archetype`, `body`, `move`, `moveset`, and `space`

**Description**

Extend the authored language so the missing gameplay declarations exist in the same live parser and HIR pipeline as `field`, `shape`, `region`, `domain`, and `view`.

**Files**

- `compiler/lexer/tokens.rs`
- `compiler/parser/grammar/func.rs`
- `compiler/parser/ast.rs`
- `compiler/parser/mod.rs`
- `compiler/hir/def.rs`

**Implementation notes**

Reuse the existing declaration machinery where possible.
Do not create a second “gameplay-only parser” path.
`space` is part of the authored world contract the repo already wants and should become live here rather than being deferred into runtime-only policy.
Audio is intentionally out of scope for this parser task.

Code sketch:

```rust
pub enum SpatialDeclKind {
    Field,
    RadianceField,
    VolumeField,
    Shape,
    Region,
    Domain,
    View,
    Generator,
    Archetype,
    Body,
    Move,
    Moveset,
    Space,
}
```

**Acceptance criteria**

- Positive parser tests exist for each new declaration kind.
- Negative parser tests exist for malformed declarations and misplaced clauses.
- AST and HIR can represent these declarations without stringly typed escape hatches.
- Existing authored examples still parse unchanged.

#### Task 63A2 — Add semantic and typecheck rules for the new declarations

**Description**

Make the new declarations analyzable and reject invalid authored programs early.

**Files**

- `compiler/hir/typeck/types.rs`
- `compiler/hir/typeck/context.rs`
- `compiler/hir/semantic.rs`
- `compiler/hir/typeck/tests.rs`

**Implementation notes**

Lock down the portable/runtime boundary now.
Examples:

- `generator` parameters must remain portable
- `archetype`/`body` instance bindings must use portable values
- `move` must expose deterministic duration and phase structure
- `moveset` must reference existing compatible moves
- `space` must use explicit topology/residency declarations over known region containers

Code sketch:

```rust
pub enum GameplayDeclRule {
    PortableOnlyParameters,
    DeterministicPhaseTimeline,
    StableInstanceBinding,
    SideEffectFreeTransitionPredicate,
    ExplicitSpaceTopology,
}
```

**Acceptance criteria**

- The compiler rejects non-portable generator parameters and opaque host bindings.
- `move` declarations require explicit deterministic phase timing.
- `moveset` declarations fail if referenced moves are missing or incompatible.
- `space` declarations reject opaque runtime topology callbacks.
- Error messages explain how to move invalid logic back into ordinary host `system` code when necessary.

### Workstream B: Lowering and identity closure

#### Task 63B1 — Extend lowering, identity, and artifact-key surfaces for gameplay declarations

**Description**

Once the declarations parse and typecheck, the compiler must give them stable compiled identity, explicit instantiated identity, and a lowering target instead of leaving them as syntax-only artifacts.

**Files**

- `compiler/scene_ir/mod.rs`
- `compiler/world_identity/mod.rs`
- `compiler/artifact_key/mod.rs`
- `compiler/query_exec/spec.rs`
- `compiler/query_contract/mod.rs`

**Implementation notes**

The important deliverable is a clean split:

- compiled declaration identity for artifacts and compiler reuse
- instantiated identity for replay/save references, region/object handles, and parameterized uses

The runtime should be able to say “this is generator bundle X” separately from “this region/object instance was created from X with parameter hash Y”.
That distinction matters because generators still have no standalone runtime identity of their own and should only appear in replay/save surfaces through instantiated region/object descriptors.

Code sketch:

```rust
pub enum CompiledBundleKind {
    Generator,
    Archetype,
    Body,
    Move,
    Moveset,
    Space,
}

pub struct CompiledBundleIdentity {
    pub kind: CompiledBundleKind,
    pub declaration: SmolStr,
    pub semantic_hash: u128,
}

pub struct BundleInstantiationIdentity {
    pub compiled: CompiledBundleIdentity,
    pub parameter_hash: u128,
    pub instance_hash: Option<u128>,
}
```

**Acceptance criteria**

- New authored declarations produce stable compiled identities for artifact keys and separate instantiated identities for replay/save references.
- Generators never become free-standing runtime identities; only their instantiated uses are replay/save-addressable.
- Scene/world lowering can carry these declarations forward without lossy string labels.
- Tests prove identity stability under repeated clean builds with unchanged source.

#### Task 63B2 — Add a canonical authored sample that compiles through the new language surface

**Description**

Create one small but representative authored sample that uses the new declarations together.

**Files**

- new `language/preview_traversal/src/main.wr`
- `compiler/tests/preview_project.rs`
- any small documentation note needed for the sample

**Implementation notes**

This sample is compile-and-analyze proof first, not full runtime proof yet.
Keep it small.
Its job is to prevent the new language surface from fragmenting before runtime work begins.
Audio is intentionally absent; Phase 68 closes audio on the observer/runtime side instead of by parser syntax.

**Acceptance criteria**

- The sample compiles through parse, HIR, typecheck, and lowering.
- The sample includes at least one `generator`, `body`, `move`, `moveset`, `archetype`, and `space`.
- CI-friendly tests confirm the sample stays buildable as later phases land.

## Phase 63 exit criteria

- The missing gameplay-native declarations are live compiler concepts, not RFC-only words.
- Stable identities exist for those declarations.
- A small representative authored sample compiles cleanly.

---

# Phase 64: Executable World Composition, `space` Topology, And Deterministic Generation

## Goal

Finish the executable streamed-world model needed for giant traversal landscapes and deterministic encounter spaces.

## Why this phase exists

This game family lives or dies on world shape.

If parameterized regions, seeded scatter, conditional encounter layouts, and residency topology remain half-real, every later gameplay and visual system will be forced to fake a smaller world than the design actually wants.

### Workstream A: Executable region closure

#### Task 64A1 — Remove the parameterized-region execution gap and add stable region instance identity

**Description**

Lift the current prohibition on parameterized regions and make region instances executable and capturable with stable identity.

**Files**

- `compiler/hir/typeck/types.rs`
- `compiler/mir/lower/function_entry.rs`
- `compiler/query_exec/spec.rs`
- `compiler/query_exec/world.rs`
- `compiler/world_identity/mod.rs`

**Implementation notes**

The important design constraint is stable region instance identity.
Do not turn region parameters into anonymous closures or transient runtime lambdas.

Code sketch:

```rust
pub struct RegionInstanceKey {
    pub declaration: SmolStr,
    pub parameter_hash: u128,
    pub source_version: u64,
}
```

**Acceptance criteria**

- Parameterized regions no longer fail semantic lowering or execution solely because they have parameters.
- Region instances can be captured and referenced by stable keys.
- Tests prove unchanged inputs yield unchanged region keys.

#### Task 64A2 — Implement executable `scatter` and conditional region composition

**Description**

Replace the current explicit runtime “not executable yet” behavior with real execution support for `scatter` and coarse structural branching.

**Files**

- `compiler/query_exec/region.rs`
- `compiler/query_exec/context.rs`
- `compiler/query_exec/world.rs`
- new `compiler/tests/region_exec.rs`

**Implementation notes**

`scatter` should remain seed-driven and compiler-visible.
Conditional region logic must stay coarse and structural, not per-sample dynamic branching.

Code sketch:

```rust
pub enum RegionPlanItem {
    Place(PlacedShape),
    Scatter(ScatterDescriptor),
    Conditional {
        predicate: RegionPredicate,
        then_items: Vec<RegionPlanItem>,
        else_items: Vec<RegionPlanItem>,
    },
}
```

**Acceptance criteria**

- `scatter` regions execute without stub errors.
- Conditional region branches execute deterministically from explicit inputs.
- Generated placement order and identity remain stable under identical seeds and parameters.
- Tests cover clean-build reproducibility.

### Workstream B: Streaming and save-stable world plans

#### Task 64B1 — Add live `space` topology and residency closure for traversal worlds

**Description**

Add the authored `space` and lowered world-plan surfaces that say which region instances should be resident around the rider, around the boss, and around active encounter seams.

**Files**

- new `compiler/world_plan/mod.rs`
- `compiler/world_identity/mod.rs`
- `compiler/query_exec/world.rs`
- `runtime/src/domain_abi.rs`
- `runtime/src/state_advance.rs`

**Implementation notes**

This task is the lowering and runtime closure for live authored `space` topology.
`world_plan` is allowed only as the lowered form of a live authored `space`, not as a runtime-only sidecar that postpones authored topology semantics.

Code sketch:

```rust
pub enum WorldTopologyKind {
    RegionLine,
    RegionGrid,
    RegionGraph,
    DynamicSpatial,
    StaticSpatial,
    Singleton,
    SparseBands,
    BoundarySet,
}

pub enum SpaceCompositionTier {
    BaseStaticRegions,
    TransitionRegions,
    ReplacementRegions,
    DynamicTerrain,
    DynamicGeometry,
    RadianceAndMedia,
}

pub struct WorldResidencyPlan {
    pub member_kind: WorldTopologyKind,
    pub follow_handle: Option<StableHandle>,
    pub resident_radius: u32,
    pub preload_radius: u32,
}

pub struct SpaceLoweringPlan {
    pub source_space: SmolStr,
    pub member_kinds: Vec<WorldTopologyKind>,
    pub composition_order: Vec<SpaceCompositionTier>,
    pub residency: Vec<WorldResidencyPlan>,
}
```

**Acceptance criteria**

- At least one authored `space` lowers into a truthful resident-region plan for traversal gameplay.
- Residency policy is explicit and testable.
- The plan preserves a stable authored composition order across static regions, transitions, replacements, and dynamic contributors.
- The representative `SparseBands` and `Singleton` entries lower without runtime sidecars.
- No runtime-only topology sidecar is required to express the representative game's world layout.

#### Task 64B2 — Add deterministic generation and save-stability evidence

**Description**

Prove that generated region content, scatter placements, and topology-derived residency stay stable under fixed inputs.

**Files**

- `runtime/src/state_advance.rs`
- `runtime/src/domain_abi.rs`
- `compiler/tests/artifact_store.rs`
- `compiler/tests/preview_project.rs`
- `compiler/bin/wrela/repro.rs`

**Implementation notes**

Do not wait for the full game to discover save instability.
Add stable digests now.

Code sketch:

```rust
pub struct GeneratedRegionDigest {
    pub region_key: RegionInstanceKey,
    pub placed_item_count: u32,
    pub support_hash: u128,
    pub payload_hash: u128,
}
```

**Acceptance criteria**

- Unchanged seeds and parameters reproduce identical generated-region digests.
- Replay/save tests prove residency order does not affect generated identity.
- The repo has a targeted tool or test harness for comparing generated world digests.

## Phase 64 exit criteria

- Parameterized regions are executable.
- `scatter` and conditional composition are no longer stubbed.
- Traversal-world `space` topology and residency are explicit and deterministic.
- Generated world identity is stable enough for replay and saves.

---

# Phase 65: Gameplay Frame Input, Replay, Save, And Host Runtime Closure

## Goal

Add the production gameplay-frame contract that turns the existing runtime substrate into a real game loop.

## Why this phase exists

Without a real gameplay frame, later traversal, boss, and audio work will still be debugged through one-off preview commands instead of through the same frame contract the shipping game uses.

### Workstream A: Gameplay frame contract

#### Task 65A1 — Add `GameplayFrameInput` and fixed-step gameplay execution stages

**Description**

Define the authoritative host/runtime contract for one gameplay frame.

**Files**

- `runtime/src/domain_abi.rs`
- `runtime/src/state_advance.rs`
- `compiler/state_advance/mod.rs`
- `runtime/src/engine_executor.rs`

**Implementation notes**

Keep the stage order explicit.
Do not bury input sampling, encounter logic, move solving, and world mutation inside one undifferentiated tick.

Code sketch:

```rust
pub struct GameplayFrameInput {
    pub current_clock: TemporalClock,
    pub previous_clock: Option<TemporalClock>,
    pub inputs: TickInputBatch,
    pub debug_overrides: GameplayDebugOverrides,
}

pub enum GameplayStage {
    Input,
    MovesetSelect,
    MoveSolve,
    WorldMutation,
    SnapshotSeal,
}
```

**Acceptance criteria**

- One gameplay frame has a named, documented, and testable stage order.
- Input is explicit data, not ambient global state.
- Authoritative timing flows from `SimulationTick`, `PresentationFrame`, and `TemporalClock` rather than a parallel floating-point delta clock.
- The runtime can produce a sealed snapshot after each fixed step.
- Engine-frame reports can name gameplay-frame work as its own subsystem slice.

#### Task 65A2 — Add checkpoint, save, and replay contracts

**Description**

Make game progress durable and debuggable.

**Files**

- `runtime/src/domain_abi.rs`
- `compiler/time_semantics/mod.rs`
- `compiler/world_identity/mod.rs`
- `compiler/artifact_key/mod.rs`
- new `runtime/src/replay.rs`

**Implementation notes**

The engine needs both user-facing saves and developer-facing replay.
Do not collapse those into one opaque blob.

Code sketch:

```rust
pub struct ReplayFrameRecord {
    pub tick: SimulationTick,
    pub inputs: TickInputBatch,
    pub snapshot_hash: u128,
    pub active_region_hash: u128,
}

pub struct CheckpointRecord {
    pub schema_version: u32,
    pub current_clock: TemporalClock,
    pub sealed_snapshot: Vec<u8>,
}
```

**Acceptance criteria**

- Replays can drive the gameplay frame deterministically.
- Checkpoint schema versioning is explicit.
- Save/load and replay tests prove stable results for unchanged source and identical inputs.

### Workstream B: Runtime descriptors and tooling

#### Task 65B1 — Add runtime registries for archetypes, bodies, movesets, and stable handles

**Description**

The runtime needs deterministic instance registries for dynamic gameplay objects without reverting to ad hoc object IDs.

**Files**

- `runtime/src/state_advance.rs`
- `compiler/world_identity/mod.rs`
- `compiler/artifact_store/mod.rs`
- `runtime/src/virtual_gpu.rs`

**Implementation notes**

The registry must be descriptor-driven.
Gameplay identity cannot depend on insertion order.

Code sketch:

```rust
pub struct RuntimeInstanceDescriptor {
    pub stable_handle: StableHandle,
    pub bundle: CompiledBundleIdentity,
    pub state_hash: u128,
}
```

**Acceptance criteria**

- Dynamic instances have stable handles.
- Archetype/body/moveset descriptors can be resolved from a sealed snapshot.
- Tests prove deterministic handle allocation under replay.

#### Task 65B2 — Add gameplay preview, replay, and inspection tooling

**Description**

Create focused workflow surfaces for this game family so the representative project can be debugged without bespoke scripts.

**Files**

- `compiler/bin/wrela/commands/command_dispatch.rs`
- new `compiler/bin/wrela/commands/gameplay_preview.rs`
- new `compiler/bin/wrela/commands/gameplay_replay.rs`
- `justfile`
- `benchmarks/README.md`

**Implementation notes**

Keep the workflow honest and repo-native.
Prefer `wrela` product-facing commands and focused `just` lanes over ad hoc shell incantations.

**Acceptance criteria**

- There is a documented preview path for the representative game project.
- There is a documented replay/inspect path for gameplay bugs.
- Any new `just` lanes clearly describe whether they prove compile, semantic, runtime, or perf behavior.

## Phase 65 exit criteria

- A production gameplay frame exists.
- Replay and checkpoint semantics are real.
- Dynamic gameplay instances have stable runtime identity.
- The repo has honest preview and replay surfaces for the representative project.

---

# Phase 66: Traversal Bodies, Ride Contact, And Movement-Combat Contracts

## Goal

Make traversal and movement-led combat first-class engine contracts instead of per-game hacks.

## Why this phase exists

The heart of the target game is not “a character controller.”

It is a traversal body reading and exploiting a field-authored world fast enough that movement itself becomes the combat grammar.
That requires dedicated contracts.

### Workstream A: Traversal query families and move solving

#### Task 66A1 — Add traversal query and collision contract families

**Description**

Introduce the typed queries needed to ask gameplay-relevant traversal questions.

**Files**

- `compiler/query_contract/mod.rs`
- `compiler/query_plan/mod.rs`
- `compiler/collision_plan/mod.rs`
- `compiler/tests/query_contract_registry.rs`
- `compiler/tests/collision_plan.rs`

**Implementation notes**

Do not overload existing generic collision queries with traversal-specific meaning.
Name the queries the game actually needs.

Code sketch:

```rust
pub enum TraversalQueryKind {
    RideSurfaceContact,
    LandingWindow,
    AirtimeEnvelope,
    AttackReachWindow,
    RecoveryClearance,
}
```

**Acceptance criteria**

- Traversal query kinds are explicit and discoverable in the query contract registry.
- Plans can represent traversal-specific outputs without stringly typed payload conventions.
- Tests cover at least ride contact, landing legality, and attack reach.

#### Task 66A2 — Add `body`/`move`/`moveset` solver integration for traversal combat

**Description**

Connect authored bodies and moves into the fixed-step runtime.

**Files**

- `compiler/state_advance/mod.rs`
- `runtime/src/state_advance.rs`
- `runtime/src/domain_abi.rs`
- new `runtime/src/move_solver.rs`

**Implementation notes**

The first move solver should prioritize determinism, debuggability, and clear authored semantics over maximal physics generality.
It should still be production-grade for this game family.

Code sketch:

```rust
pub struct MoveStepResult {
    pub next_move: SmolStr,
    pub contacts: Vec<ContactEvent>,
    pub attack_windows: Vec<AttackWindow>,
    pub body_updates: Vec<BodyStateDelta>,
}
```

**Acceptance criteria**

- The runtime can evaluate authored moves and movesets on a fixed timestep.
- Move transitions are deterministic under replay.
- Contact and attack-window outputs are visible to gameplay tests and debug tooling.

### Workstream B: Traversal throughput and representative evidence

#### Task 66B1 — Add batched traversal execution with CPU parity and WGSL acceleration

**Description**

Traversal queries will be hot.
Batch them deliberately and accelerate them through the existing collision/GPU runtime spine.

**Files**

- `compiler/collision_exec/cpu.rs`
- `compiler/collision_exec/gpu.rs`
- `compiler/gpu_runtime/resident_scene.rs`
- `compiler/tests/collision_exec/cpu.rs`
- `compiler/tests/collision_exec/wgsl.rs`

**Implementation notes**

This should build on the batch and resident-scene work that already exists.
Do not invent a second traversal-only GPU runtime.

Code sketch:

```rust
pub struct TraversalBatch {
    pub snapshot: WorldSnapshotHandle,
    pub queries: Vec<TraversalQueryRequest>,
    pub certification_policy: CertificationPolicy,
}
```

**Acceptance criteria**

- Traversal queries can run in batches rather than as serial one-off probes.
- CPU parity tests exist for the accelerated path.
- Closure-mode traversal batches do not require per-query immediate readback.

#### Task 66B2 — Add representative traversal scenarios and perf evidence

**Description**

Add the first honest benchmark and test content for the actual core loop inside the canonical `engine_frame` suite manifests the repo already discovers.

**Files**

- `benchmarks/engine_frame/bench.toml`
- `benchmarks/engine_frame/1080p120_closure.toml`
- `compiler/bin/wrela/perf_engine/collection.rs`
- `compiler/tests/engine_frame.rs`
- `compiler/tests/cli/perf.rs`

**Implementation notes**

Use scenarios that actually matter:

- sustained carving
- chained jumps
- landing recovery
- boss approach window

**Acceptance criteria**

- The repo has a canonical `engine_frame` perf lane for representative traversal workloads.
- Engine-frame output names traversal work explicitly.
- Representative traversal scenarios live in the suite-root `bench.toml` / `1080p120_closure.toml` manifests or the RFC explicitly updates manifest discovery.
- Regression tests fail if traversal workloads silently fall back to obviously unrepresentative single-query execution patterns.

## Phase 66 exit criteria

- Traversal has named query families.
- Authored moves and movesets drive fixed-step movement combat.
- Traversal throughput is benchmarked honestly inside the engine frame.

---

# Phase 67: Boss Terrain Authority, Deformation, And Encounter Closure

## Goal

Make boss-authored terrain mutation a first-class engine concept that remains compatible with replay, performance, and visual closure.

## Why this phase exists

This game family depends on bosses that change the ride.

If boss deformation is implemented as a local effect outside the world/snapshot/artifact model, it will break exactly the systems that matter most: contact, fairness, presentation stability, and replay correctness.

### Workstream A: Mutable world authority

#### Task 67A1 — Make `terrain` contribution and `deform` semantics live for boss terrain authority

**Description**

Extend authored dynamic declarations so traversal-relevant `terrain` contribution and `deform = expr` hooks become live and authoritative for traversal terrain mutation.

**Files**

- `compiler/hir/typeck/types.rs`
- `compiler/scene_ir/mod.rs`
- `compiler/query_contract/mod.rs`
- `compiler/collision_plan/mod.rs`
- `compiler/presentation_plan/mod.rs`

**Implementation notes**

Close the authored deformation path, do not replace it.
Separate terrain contribution from ordinary visible geometry when the semantics differ, but make that distinction flow from the authored `terrain` and `deform` model rather than from a second mutation API.
Traversal fairness depends on this distinction.

Code sketch:

```rust
pub struct TerrainMutationDescriptor {
    pub owner: StableHandle,
    pub deform_program: SmolStr,
    pub support: Support3,
    pub mutation_kind: TerrainMutationKind,
    pub replay_hash: u128,
}
```

**Acceptance criteria**

- Boss-authored terrain contribution is representable through live `terrain` and `deform` semantics rather than abusing generic geometry ownership.
- Terrain mutation carries explicit support and identity.
- Collision and presentation plans can see that a terrain mutation happened.

#### Task 67A2 — Add bounded artifact invalidation and resident-scene update for deformed terrain

**Description**

Local boss deformation derived from authored `deform` and `terrain` changes must not require a whole-world rebuild.

**Files**

- `compiler/artifact_store/mod.rs`
- `compiler/world_identity/mod.rs`
- `compiler/gpu_runtime/resident_scene.rs`
- `runtime/src/virtual_gpu.rs`

**Implementation notes**

Invalidate by explicit support and snapshot lineage.
Full-scene rebuild should remain the fallback, not the steady-state path.
`WorldMutationTicket` is runtime evidence derived from authored deformation, not a second authoring surface.

Code sketch:

```rust
pub struct WorldMutationTicket {
    pub snapshot_before: WorldSnapshotHandle,
    pub snapshot_after: WorldSnapshotHandle,
    pub invalidated_support: Support3,
    pub affected_artifacts: Vec<ArtifactKey>,
}
```

**Acceptance criteria**

- Local deformation can update collision/presentation artifacts without full-scene rebuild in the common case.
- Invalidation scope is visible in reports and tests.
- Replay tests prove mutation ordering remains deterministic.
- No parallel deformation authoring API is required beyond the live `terrain` and `deform` semantics.

### Workstream B: Encounter-specific gameplay closure

#### Task 67B1 — Add encounter contracts for attack-through-movement boss fights

**Description**

Boss fights in this game family are won by movement geometry, not by a separate combat minigame.
The engine needs explicit contracts for that.

**Files**

- `compiler/query_contract/mod.rs`
- `compiler/query_plan/mod.rs`
- `compiler/collision_plan/batch.rs`
- `compiler/tests/collision_plan.rs`

**Implementation notes**

Examples:

- is a weak point reachable from this airtime envelope?
- does the current launch trajectory intersect the valid attack window?
- does boss deformation invalidate the current recovery line?

Code sketch:

```rust
pub struct AttackWindowQuery {
    pub rider_pose: RiderPose,
    pub trajectory: MotionArc,
    pub target_handle: StableHandle,
    pub time_horizon_seconds: f32,
}
```

**Acceptance criteria**

- Encounter-specific movement attack queries are typed and testable.
- Plans can explain why an attack window was valid or invalid.
- Tests cover deformation-driven failure and success cases.

#### Task 67B2 — Add deterministic encounter matrices and replay failure tooling

**Description**

Boss logic must be provable under replay, not only watchable in preview.

**Files**

- `compiler/bin/wrela/repro.rs`
- `runtime/src/state_advance.rs`
- `compiler/tests/engine_frame.rs`
- `compiler/tests/cli/perf.rs`

**Implementation notes**

Create a small but strong encounter matrix:

- boss dormant
- boss wake transition
- active terrain mutation
- reachable attack window
- missed recovery window

**Acceptance criteria**

- The repo has deterministic replay evidence for key encounter states.
- Bug reports can point at a replayable encounter record, not just a screenshot.
- Encounter regressions fail with enough context to localize the broken stage.

## Phase 67 exit criteria

- Boss terrain mutation is a first-class world authority path.
- Deformation updates can propagate incrementally.
- Attack-through-movement boss encounters are testable and replayable.

---

# Phase 68: Field-Derived Procedural Audio

## Goal

Add the bounded, production-grade audio subsystem this game family needs, derived from captured world state and integrated into the engine frame.

## Why this phase exists

The target game should feel stunning, not merely correct.

For this game family, audio is not decorative.
Wind, carve hiss, board chatter, canyon bloom, boss strain, landing impact, and terrain rupture all help sell speed, scale, danger, and readability.

The implementation stance has to stay practical:

- the audio device callback must stay real-time safe and non-blocking
- whole-world field evaluation must not happen at audio sample rate
- audio should observe the same world substrate the rest of the engine already trusts: region/world captures, `Surface`, `Medium`, occlusion, snapshot identity, traversal state, and encounter state

This phase therefore treats audio as its own `contract -> plan -> exec -> runtime` lane, but not as a second authored world model.

### Workstream A: Audio observation, planning, and runtime

#### Task 68A1 — Add audio observation contracts and planning surfaces over existing world capture

**Description**

Create the compiler-side contract and plan model for audio observation over existing region/world captures plus gameplay state.

**Files**

- new `compiler/audio_contract/mod.rs`
- new `compiler/audio_plan/mod.rs`
- new `compiler/audio_exec/mod.rs`
- `compiler/lib.rs`
- `compiler/query_contract/mod.rs`

**Implementation notes**

Do not treat audio as “just another render pass.”
The audio subsystem should consume world identity and observation inputs through its own contract.
Do not add top-level `audio field` syntax here.
The authored world remains the ordinary field substrate.

Audio plans should be able to derive from:

- listener pose and velocity
- traversal state such as speed, turn rate, contact state, airtime state, and attack state
- encounter state such as boss phase, threat, and deformation intensity
- existing world observations such as `spatial.nearest.world`, `surface.sample.world`, `participants.medium.world`, and `spatial.occluded.world`
- bounded probe fans around the listener, board, boss, and impact sites when indirect sound or environment sends are needed

Code sketch:

```rust
pub struct AudioObserver {
    pub world_snapshot: WorldSnapshotHandle,
    pub current_clock: TemporalClock,
    pub listener: ListenerState,
    pub traversal: TraversalAudioState,
    pub encounter: EncounterAudioState,
}

pub struct AudioObservationPlan {
    pub control_rate_hz: u32,
    pub direct_path_queries: Vec<QueryContractId>,
    pub environment_probes: Vec<AudioProbe>,
    pub max_spatial_emitters: u32,
}

pub struct AudioMixContract {
    pub beds: Vec<AudioBedDescriptor>,
    pub events: Vec<AudioEventDescriptor>,
    pub spatialization: SpatializationMode,
}
```

**Acceptance criteria**

- Audio plans derive from existing world capture and gameplay state, with no required new top-level authored declaration.
- Audio contracts can describe listener context, bounded world observations, control rate, and bounded voice/event sets.
- Audio plans lower into explicit audio-exec artifacts instead of dispatching directly from gameplay/runtime code.
- Tests cover contract serialization and plan stability.

#### Task 68A2 — Add a real-time-safe control-rate audio runtime and engine-frame integration

**Description**

Implement the first production audio path for this game family.

**Files**

- new `compiler/audio_exec/mod.rs`
- new `runtime/src/audio_runtime.rs`
- `runtime/src/engine_executor.rs`
- `compiler/engine_frame/mod.rs`
- `compiler/bin/wrela/commands/test_eval_perf.rs`
- `compiler/bin/wrela/perf_engine/closure.rs`
- `compiler/bin/wrela/perf_engine/collection.rs`
- `compiler/perf_target/mod.rs`

**Implementation notes**

Start with a control-rate world-observation model that drives procedural voices, events, and buses.
Do not make per-sample whole-world field tracing the default path.
That would be expensive in the wrong way.

Use a `cpal`-style device backend where a dedicated high-priority callback fills the output buffer.
The callback path should never perform world queries, blocking I/O, shader compilation, or unbounded allocation.
Instead:

- run world observation at control rate on the gameplay/runtime side
- double-buffer or otherwise hand off compact audio control frames to the callback
- let the callback do bounded synthesis, mixing, filtering, and spatialization work only

Only control-rate updates, handoff bookkeeping, and telemetry belong to the engine-frame report.
The long-lived device callback remains a runtime-owned audio I/O concern outside the per-frame scheduler.
Update the canonical closure budget surface at the same time so audio has an explicit budget and why-not-120 verdict path, not just raw subsystem telemetry.

The first production audio stack should be layered:

- continuous beds: wind, speed pressure, board hiss, surface rumble, boss tension
- event voices: landings, scrapes, attack releases, near-misses, terrain fractures
- spatial sources: boss vocalization, deformation ruptures, debris, distant landmarks
- environment sends: occlusion, transmission EQ, reflection/reverb, open-air vs enclosed bloom

Parameter derivation should come from the existing world substrate:

- surface and payload data drive scrape, landing, and carve timbre
- medium data drives air absorption, fogginess, bloom, and atmospheric color
- occlusion queries drive direct-path muffling and line-of-sight loss
- bounded probe fans estimate openness, enclosure, and reflection send

Clock-domain closure is part of the production contract:

- stamp control frames against `TemporalClock` and `WallClockStamp`
- define bounded stale-frame behavior when a control update misses the callback deadline
- handle device-loss, sample-rate changes, and output-format renegotiation without undefined mix state

If higher-end spatial propagation is needed later, keep it behind the audio plan as an optional backend that consumes exported geometry/material proxies.
Do not make third-party propagation middleware the authored source of truth.
If used, it should stay behind the audio plan as an implementation choice for HRTF, direct occlusion/transmission, and reflections/reverb.

Code sketch:

```rust
pub struct AudioSubsystemReport {
    pub control_updates: u32,
    pub active_voices: u32,
    pub callback_fill_p99_micros: u128,
    pub underrun_count: u32,
    pub dropped_events: u32,
}
```

**Acceptance criteria**

- Audio control updates, handoff, and telemetry appear as named work in the canonical engine-frame closure output.
- Audio workload, callback latency, and underrun data are visible in reports.
- The long-lived device callback is not scheduled as an engine-frame task; only control-rate work and reporting are.
- The callback path performs no whole-world query work and no blocking host work.
- Stale control frames, sample-rate changes, and device-loss/renegotiation are handled by explicit runtime policy.
- The correctness path works without requiring GPU audio execution or third-party propagation middleware.

### Workstream B: Tooling, debugging, and bounded closure

#### Task 68B1 — Add audio audition and debugging tools

**Description**

Make field-derived audio debuggable by humans.

**Files**

- `compiler/bin/wrela/commands/command_dispatch.rs`
- new `compiler/bin/wrela/commands/audio_preview.rs`
- `runtime/src/audio_runtime.rs`

**Implementation notes**

At minimum, developers need to inspect:

- active voices
- active buses and sends
- control-rate driver values
- spatial source attribution
- boss vs traversal mix contribution
- surface, medium, and occlusion inputs driving the current mix

Also provide an offline render path for short replay windows so audio bugs can be reproduced and discussed without live play.

**Acceptance criteria**

- The repo can preview, inspect, or offline-render representative audio scenes from replayable encounter windows.
- Debug output can explain which audio layers and world observations are active.
- Audio bugs can be reproduced from replay or offline render without live gameplay.

#### Task 68B2 — Add bounded audio perf and determinism tests

**Description**

Keep audio as one disciplined subsystem, not a sprawling second engine.

**Files**

- new `compiler/tests/audio_exec.rs`
- `compiler/tests/engine_frame.rs`
- `compiler/tests/cli/perf.rs`

**Implementation notes**

The acceptance bar is:

- deterministic control-rate behavior under replay
- bounded runtime cost
- no hidden frame-budget theft

Prefer proving determinism through control-frame logs and bounded offline-render summaries rather than pretending raw output PCM should be the replay artifact of record.

**Acceptance criteria**

- Audio determinism tests exist for fixed seed and fixed replay inputs.
- Perf tests report audio cost inside engine-frame closure.
- Audio stays within an explicit subsystem budget for the representative project.

## Phase 68 exit criteria

- Wrela has a first-class field-derived audio subsystem.
- Audio derives from captured world state and gameplay state rather than a second authored substrate.
- Audio participates in engine-frame reporting.
- Audio remains bounded, testable, and replay-friendly.

---

# Phase 69: PBR Materials, Lighting, And Soft Shadow Closure

## Goal

Close the physically based material, lighting, and shadow foundation needed for stunning traversal worlds.

## Why this phase exists

The repo already has real `Surface` records, authored material functions, radiance/medium inputs, and basic lighting controls.
That is a foundation, not AAA closure.

If terrain, boss skin, ruins, dust-coated stone, wet sand, and metallic accents do not respond beautifully and consistently to light, every later sky or atmosphere improvement will read as fake.

This phase closes the direct-lighting and material-response layer that everything else depends on.

### Workstream A: Material and lighting closure

#### Task 69A1 — Close the production surface/material model for traversal worlds

**Description**

Extend the existing `Surface`/lighting path into a production material model for traversal terrain, boss bodies, ruins, and atmosphere-facing hero assets.

**Files**

- `compiler/presentation_plan/mod.rs`
- `compiler/presentation_exec/mod.rs`
- `compiler/presentation_exec/cpu.rs`
- `compiler/presentation_exec/wgsl/shaders.rs`
- `compiler/tests/presentation_exec/quality.rs`

**Implementation notes**

Build from the `Surface` substrate the repo already has.
Do not replace authored `material` functions with opaque imported material graphs.

Close the production path for:

- consistent roughness/metalness/clearcoat response
- terrain-specific lobe control for sand, rock, crust, wet sheen, and carved tracks
- deformation-aware material transitions
- CPU/WGSL shading parity good enough that lookdev can be trusted on both paths

Code sketch:

```rust
pub struct TraversalSurfaceQuality {
    pub specular_antialiasing: bool,
    pub clearcoat_energy_compensation: bool,
    pub terrain_blend_budget: u8,
    pub authored_material_lod: u8,
}
```

**Acceptance criteria**

- Representative terrain and boss materials show stable response under camera and light changes.
- CPU and WGSL shading paths stay parity-tested for the production surface model.
- Material detail and shading quality choices are explicit in planning and cost reporting.

#### Task 69A2 — Add directional-light and soft-shadow closure for huge traversal spaces

**Description**

Add a production directional-light and soft-shadow path for giant traversal worlds and deformation-heavy boss fights.

**Files**

- `compiler/presentation_contract/mod.rs`
- `compiler/hir/def.rs`
- `compiler/hir/lower.rs`
- `compiler/hir/typeck/types.rs`
- `compiler/presentation_plan/mod.rs`
- `compiler/presentation_exec/gpu_primary.rs`
- `benchmarks/lookdev/traversal_boss/README.md`
- new `compiler/presentation_exec/shadows.rs`
- `compiler/presentation_exec/wgsl/passes.rs`
- `compiler/presentation_exec/wgsl/shaders.rs`
- `compiler/presentation_exec/cost.rs`
- `compiler/tests/presentation_exec/quality.rs`

**Implementation notes**

The game needs stable sun shadows across huge landscapes.
That almost certainly means a resident directional-shadow solution such as cascades or clipmaps plus filtered penumbrae and deformation-aware invalidation.
These controls must flow through the authored presentation contract and then into planning and execution; do not add GPU-only shadow knobs that bypass the contract layer.

The target is:

- horizon-scale sun direction that matches the authored sky/light model
- stable shadow texel snapping under high-speed camera motion
- soft-filtered penumbrae that do not shimmer at speed
- selective rerender/invalidation when boss deformation changes the terrain

Seed provisional traversal lookdev scenarios here and validate against moving traversal shots from the start; do not validate the shadow foundation against placeholder static beauty frames only.

Code sketch:

```rust
pub struct DirectionalShadowContract {
    pub cascade_count: u32,
    pub clipmap_radius_meters: f32,
    pub filter_radius_pixels: f32,
    pub temporal_stabilization: bool,
}
```

**Acceptance criteria**

- Representative traversal scenes have stable directional shadows across near and far terrain.
- Boss deformation invalidates only the shadow work it must invalidate.
- Shadow cost and quality settings are explicit in perf and quality reports.
- Shadow validation uses provisional traversal scenarios that later become canonical in Phase 71.

## Phase 69 exit criteria

- Production material response is real and parity-tested.
- Soft directional shadows are real, stable, and measured.
- The lighting foundation is good enough for later sky, atmosphere, and FX work to build on honestly.

---

# Phase 70: Sky, Atmosphere, And Scenic-Scale Depth Closure

## Goal

Make the horizon, sky, aerial perspective, and world-scale depth beautiful enough to carry huge traversal vistas and boss reveals.

## Why this phase exists

The repo already has simple sky gradients, radiance fields, and medium fields.
That is a useful starting point, not the final answer.

This game family needs a sky and atmosphere layer that can sell quiet distance shots, harsh sunlight, dust-filled basins, and huge silhouette reveals without collapsing the frame budget.

### Workstream A: Sky and atmosphere models

#### Task 70A0 — Choose the sky/atmosphere implementation family and close the Wrela semantic surface

**Description**

Before building the production sky path, lock down two things:

1. which sky/atmosphere implementation family Wrela is actually targeting
2. how sky, sun, and atmosphere fit semantically into the existing Wrela language and presentation contract

**Files**

- `compiler/hir/def.rs`
- `compiler/hir/lower.rs`
- `compiler/hir/typeck/types.rs`
- `compiler/presentation_contract/mod.rs`
- `compiler/presentation_plan/mod.rs`
- `language/view_basic/src/main.wr`
- this RFC

**Implementation notes**

Do not start Phase 70 by immediately writing shaders.
This is a semantic and architectural closure task first.

The current repo already has the beginnings of the right model:

- sky-like authored radiance already fits naturally as `radiance field`
- atmosphere-like authored media already fits naturally as `volume field` returning `Medium`
- typed `view` metadata already lowers through HIR into `presentation_contract`

That means the default semantic direction should be:

- no new top-level `sky` declaration
- no second rendering-only authoring language
- authored sky radiance stays in `radiance field`
- authored atmosphere stays in `volume field`
- sun, shadows, aerial perspective, and sky/atmosphere selection become typed `view` / `presentation_contract` inputs

This task should explicitly compare the main implementation families and pick one:

- Hillaire-style scalable dynamic sky/atmosphere as the recommended production target
- Bruneton-style precomputed atmosphere as a correctness/reference model and validation oracle
- Hošek-Wilkie-style fitted sky as a limited fallback/reference for sky-dome-only needs, not the full traversal atmosphere solution

The required output of this task is a short decision artifact checked into the repo or this RFC addendum that contains:

- the chosen implementation family
- a short tradeoff table comparing the alternatives
- the concrete `HIR` and `presentation_contract` deltas required to author sky/atmosphere in Wrela
- the artifact/LUT plan and invalidation rules
- explicit in-scope vs out-of-scope items for the representative project

Decision criteria should be explicit:

- visual quality in giant traversal vistas
- compatibility with the existing `radiance field` / `volume field` substrate
- support for dynamic sun direction and aerial perspective
- fit with the 120-FPS engine-frame budget on the target Mac
- implementation complexity appropriate for a solo engine roadmap

Clouds should stay explicitly out of scope unless this task proves they are required for the representative project.

Illustrative semantic target:

```wr
radiance field traversal_sky(p: Vec3, direction: Vec3, feature_id: U32) -> Vec3 { ... }

volume field traversal_atmosphere(p: Vec3, surface_distance: F32) -> Medium { ... }

view traversal_view(world: RegionCapture, camera: Camera) {
    domain = traversal_presentation(world = world)
    lighting = lighting(
        sun = directional_light(...),
        sky = traversal_sky,
        atmosphere = traversal_atmosphere,
        shadows = directional_shadows(...),
        aerial_perspective = true
    )
}
```

**Acceptance criteria**

- The RFC names the chosen sky/atmosphere implementation family and why it fits Wrela’s product needs.
- The semantic decision is explicit: sky and atmosphere extend existing `radiance field`, `volume field`, and `view` metadata rather than introducing a second world-authoring substrate.
- Typed authored/view metadata requirements are identified in `HIR` and `presentation_contract` before shader implementation begins.
- Deferred items such as clouds or multiple-scattering stretch goals are explicitly marked as in-scope or out-of-scope.

#### Task 70A1 — Add a production sky and solar-lighting model

**Description**

Close the production sky model so the world has a hero-worthy sun, horizon, zenith, and sky-light relationship.

**Files**

- `compiler/presentation_contract/mod.rs`
- `compiler/hir/def.rs`
- `compiler/hir/lower.rs`
- `compiler/hir/typeck/types.rs`
- `compiler/presentation_plan/mod.rs`
- `compiler/presentation_exec/gpu_primary.rs`
- `compiler/presentation_exec/gpu_post.rs`
- `benchmarks/lookdev/traversal_boss/README.md`
- `compiler/presentation_exec/wgsl/passes.rs`
- `compiler/presentation_exec/wgsl/shaders.rs`
- `compiler/tests/presentation_exec/quality.rs`

**Implementation notes**

Build from the existing radiance and lighting substrate.
Do not invent a separate sky renderer that ignores authored world semantics.
These sky and solar controls must become typed authored/view contract inputs that survive HIR, contract formation, planning, replay, and execution.

The target is:

- sun-disk and directional-light coherence
- horizon color separation and zenith falloff
- time-of-day or art-direction controls that remain explicit
- sky contribution that remains compatible with authored radiance and view planning

Seed provisional traversal lookdev scenarios here as well so sky validation includes rider-speed camera motion, horizon management, and boss-reveal shots from the beginning.

Code sketch:

```rust
pub struct PresentationSkyAtmosphereMetadata {
    pub sky_radiance: Option<Body>,
    pub atmosphere_medium: Option<Body>,
    pub sun_disk_enabled: bool,
    pub aerial_perspective_enabled: bool,
    pub horizon_falloff: Option<Body>,
    pub multi_scatter_approximation: bool,
}
```

**Acceptance criteria**

- Representative scenes have a production sky path instead of a placeholder gradient-only sky.
- Solar lighting and sky contribution remain visually coherent across the representative scenarios.
- Sky cost and quality are visible in presentation and engine-frame reports.
- Sky validation uses provisional traversal scenarios that later become canonical in Phase 71.
- The implementation extends existing authored/view and presentation-contract surfaces instead of creating a parallel sky-side contract path.

#### Task 70A2 — Close aerial perspective, volumetrics, and scenic depth for traversal vistas

**Description**

Push the resident presentation path toward the scenic-depth requirements of dunes, cliffs, dust basins, foggy voids, and boss-scale reveals.

**Files**

- `compiler/presentation_exec/clipmap.rs`
- `compiler/presentation_exec/gpu_primary.rs`
- `compiler/presentation_exec/gpu_post.rs`
- `compiler/presentation_exec/wgsl/passes.rs`
- `compiler/presentation_exec/cost.rs`
- `compiler/gpu_runtime/resident_scene.rs`

**Implementation notes**

This is where optional subgroup kernels should be considered aggressively for traversal-heavy hot spots, but only behind parity-tested feature gates.

The target is:

- stable far-field silhouettes
- believable aerial perspective over long distances
- dust, haze, and basin atmosphere that deepen scale instead of flattening it
- boss-scale reveal depth that remains stable at speed

Layered cloud or high-altitude sky detail is acceptable if the art direction needs it, but only if it is explicitly budgeted and measured.

**Acceptance criteria**

- The representative traversal view uses the resident presentation path as the truthful hot path for atmosphere and scenic depth.
- Far-field and atmosphere costs are visible in perf reports.
- Optional subgroup/F16 paths remain feature-gated and parity-tested.

## Phase 70 exit criteria

- The repo has a production sky and solar-lighting model.
- Scenic depth and atmosphere are real, measured, and stable enough for traversal play.
- The world-scale vistas no longer depend on placeholder lookdev.

---

# Phase 71: Traversal Readability, Hero FX, And Visual Governance

## Goal

Make the spectacle legible at speed, add the hero effects that sell impact, and keep the visual ambition honest with named scenarios and closure output.

## Why this phase exists

AAA visuals are not just materials and sky.
The player needs to be able to read jumps, landings, carve lines, deformation waves, and strike windows at 120 FPS.

This phase is where “beautiful” and “playable” stop fighting each other.

### Workstream A: Traversal view, motion, and hero FX

#### Task 71A1 — Add traversal view and camera contracts for speed readability

**Description**

Extend authored view metadata and presentation planning so traversal-heavy content can express camera and readability needs explicitly.

**Files**

- `compiler/hir/def.rs`
- `compiler/hir/lower.rs`
- `compiler/hir/typeck/types.rs`
- `compiler/presentation_contract/mod.rs`
- `compiler/presentation_plan/mod.rs`
- `compiler/presentation_exec/controller.rs`
- `compiler/presentation_exec/temporal.rs`
- `compiler/tests/presentation_plan.rs`

**Implementation notes**

Traversal readability is not just FOV.
The engine needs explicit room for:

- high-speed camera follow behavior
- look-ahead framing
- jump readability
- motion-vector quality
- horizon stability

These controls must be authorable and survive through HIR and the presentation contract layer before they reach controller/runtime policy.

Code sketch:

```rust
pub struct TraversalViewMetadata {
    pub look_ahead_seconds: f32,
    pub landing_focus_bias: f32,
    pub motion_vector_quality: MotionVectorQuality,
    pub horizon_stability_weight: f32,
}
```

**Acceptance criteria**

- Traversal readability metadata is authorable and survives HIR, typed presentation contracts, planning, and execution setup.
- Presentation plans can carry traversal-specific view metadata.
- Tests prove traversal view metadata survives planning and execution setup.
- Motion history and camera setup remain deterministic under replay.

#### Task 71A2 — Add hero traversal and boss FX closure without losing readability

**Description**

Add the high-value presentation effects that sell speed, impact, and deformation while keeping the scene readable at 120 FPS.

**Files**

- `compiler/presentation_exec/gpu_post.rs`
- `compiler/presentation_exec/wgsl/passes.rs`
- `compiler/presentation_exec/temporal.rs`
- `compiler/presentation_exec/cost.rs`
- `compiler/tests/presentation_exec/quality.rs`

**Implementation notes**

The goal is not “more particles everywhere.”
It is a specific, bounded traversal/boss FX set:

- carve spray and dust ribbons
- landing bursts
- near-surface speed accents
- boss wake and deformation shock fronts
- strike and miss feedback that stays readable in motion

Every effect must preserve silhouette readability, temporal stability, and explicit cost reporting.

**Acceptance criteria**

- Representative traversal and boss scenes have hero FX that improve spectacle without hiding gameplay state.
- FX costs are visible in perf reports instead of disappearing into generic post-processing time.
- Temporal instability and readability regressions fail named quality scenarios.

### Workstream B: Lookdev scenarios and performance honesty

#### Task 71B1 — Add hero lookdev scenarios for traversal, jump, and boss reveal content

**Description**

Promote the provisional traversal scenarios introduced in Phases 69 and 70 into the canonical hero lookdev scenario set for this game family.

**Files**

- new `benchmarks/lookdev/traversal_boss/README.md`
- `benchmarks/engine_frame/bench.toml`
- `benchmarks/engine_frame/1080p120_closure.toml`
- `benchmarks/README.md`
- `compiler/tests/cli/perf.rs`

**Implementation notes**

The scenes should not be random.
They should be named scenarios such as:

- dawn dune carve
- cliffline jump chain
- storm basin boss wake
- deformation impact recovery line

**Acceptance criteria**

- The repo has named visual scenarios tied to the representative game family.
- Benchmarks and lookdev evidence use the same scenario identities, with those scenarios added to the canonical `engine_frame` manifests or accompanied by an explicit manifest-discovery change.
- Regressions can point to a concrete visual scenario instead of a generic scene hash.

#### Task 71B2 — Add traversal-specific “why not 120?” findings and quality governance

**Description**

Use the engine-frame closure model to keep the visual ambition honest.

**Files**

- `compiler/presentation_exec/cost.rs`
- `compiler/bin/wrela/perf_engine/closure.rs`
- `compiler/perf_target/mod.rs`

**Implementation notes**

The closure layer should be able to say whether traversal misses 120 because of:

- material and shading cost
- directional-shadow cost
- sky or atmosphere cost
- hero FX and motion-readability cost
- boss deformation update churn
- audio or gameplay stealing reserve

**Acceptance criteria**

- Traversal-specific why-not-120 findings exist in perf closure output.
- Quality downgrades, if used, are explicit and reported.
- The repo cannot claim traversal visual closure without canonical `engine_frame` evidence.

## Phase 71 exit criteria

- Traversal presentation has explicit readability contracts.
- The repo has named hero lookdev scenarios and truthful 120-FPS findings for them.
- The visual ambition is no longer under-specified or unmeasured.

---

# Phase 72: Representative Game Project, Ship Surface, And Final Closure

## Goal

Make the whole engine shape permanent, executable, and shippable through one representative traversal-boss project.

## Why this phase exists

Without a permanent product proof surface, the earlier phases can still regress into “engine capability fragments.”

This phase makes the repo answer the only question that matters:

Can a solo developer build this class of game on top of Wrela, in this repo, without hidden one-off infrastructure?

### Workstream A: Representative project and workflow

#### Task 72A1 — Add the permanent representative traversal-boss project

**Description**

Create the in-repo project that proves the engine stack.

**Files**

- new `games/traversal_boss/`
- new `games/traversal_boss/README.md`
- representative authored `.wr` modules and minimal runtime glue inside that project

**Implementation notes**

This project should include:

- traversal world content
- one rider/board body setup
- at least one boss encounter with terrain mutation
- representative audio hooks
- representative view and perf scenarios

It should be small enough to maintain and rich enough to matter.

**Acceptance criteria**

- The project boots through repo-native commands.
- The project is not a dead sample; it is the canonical representative product proof for this game family.
- The project exercises the language/runtime/audio/deformation/visual stack together.

#### Task 72A2 — Add canonical repo lanes and product-facing commands for the representative project

**Description**

Make the developer workflow honest and ergonomic.

**Files**

- `justfile`
- `AGENTS.md`
- `benchmarks/README.md`
- `compiler/bin/wrela/commands/command_dispatch.rs`

**Implementation notes**

Likely additions include focused lanes for gameplay tests and representative perf closure, but every new lane must say what truth it proves.

Examples:

- `just test-gameplay`
- `just perf-game-closure`
- `wrela preview games/traversal_boss`
- `wrela replay games/traversal_boss`

**Acceptance criteria**

- The representative project has clear build, preview, replay, and perf lanes.
- Workflow docs distinguish compile proof, semantic proof, runtime proof, and performance proof.
- `just ship` includes the representative project in its truth surface or explicitly explains why not.

### Workstream B: Final closure and ship evidence

#### Task 72B1 — Add canonical end-to-end closure scenarios for the representative project

**Description**

Define the end-state benchmark and regression scenarios that future work must preserve.

**Files**

- `benchmarks/engine_frame/bench.toml`
- `benchmarks/engine_frame/1080p120_closure.toml`
- `compiler/tests/engine_frame.rs`
- `compiler/tests/cli/perf.rs`

**Implementation notes**

The minimum canonical scenario set should include:

- free traversal across stable terrain
- chained airtime attack approach
- boss wake and deformation
- successful strike window
- recovery after missed line

**Acceptance criteria**

- End-to-end scenarios exist and are named in repo artifacts.
- Engine-frame reports can attribute frame cost across gameplay, collision, presentation, and audio for those scenarios.
- The canonical `just perf-engine-closure` lane can discover the representative scenarios without custom one-off manifest filenames, or the RFC explicitly updates lane wiring.
- Regressions fail against the representative project, not only against synthetic benches.

#### Task 72B2 — Add final ship checklist and maintenance closure

**Description**

Close the loop so the representative project can actually be handed off and maintained.

**Files**

- `games/traversal_boss/README.md`
- `benchmarks/README.md`
- `AGENTS.md`
- `compiler/tests/repo_smoke.rs`

**Implementation notes**

The ship checklist should cover:

- preview path
- replay path
- perf closure path
- save/load validation
- representative audio validation
- representative deformation validation
- machine-limitation disclosure when the target hardware misses closure

**Acceptance criteria**

- The representative project has a maintained operator-facing checklist.
- Repo smoke or documentation checks ensure the project entry points stay present.
- No phase is considered complete unless the representative project still builds and its evidence surface remains truthful.

## Phase 72 exit criteria

- A permanent representative traversal-boss project exists.
- Repo-native lanes cover preview, replay, perf, and ship.
- End-to-end closure is measured against real representative content.

## Final Statement

This RFC is intentionally the point where Wrela stops being only “a very promising field engine” and becomes a production-grade engine for a specific, ambitious game family.

If these phases land honestly, the repo should be able to support a traversal game with:

- breathtaking field-authored landscapes
- beautiful sky, soft shadows, and convincing PBR materials
- speed-led combat
- bosses that reshape the ride
- procedural audio that belongs to the world instead of sitting beside it
- whole-frame performance closure that still tells the truth

That is the right next frontier for this codebase.
