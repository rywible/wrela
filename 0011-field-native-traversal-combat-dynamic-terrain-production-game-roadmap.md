# RFC 0011: Field-Native Traversal Combat, Dynamic Terrain, And Production Game Closure Roadmap

Status: Proposed post-RFC-0010 gameplay, runtime, audio, and representative-game roadmap after full repo read and subagent-backed architecture audit

Author: GPT-5.4 Pro

Created: 2026-04-21

Last materially revised: 2026-04-23

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
The earlier language RFCs still define the desired source semantics, but this document owns the implementation closure path from the current checkout to a shippable traversal-combat game family.

When this RFC repeats or narrows language from `language/spec/rfcs/0001-field-game-language.md`, treat RFC 0001 as the semantic source and this RFC as the live-repo execution plan.
If an implementation task discovers a conflict between the two, it must resolve the semantic conflict explicitly in the RFC or an addendum before landing code.

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

## Implementation Ownership Map

This RFC intentionally adds a lot of surface area.
To keep that surface from turning into architecture drift, every implementation must respect this ownership map.

### Authored language ownership

- `compiler/lexer`, `compiler/parser`, `compiler/hir`, and `compiler/hir/typeck` own authored syntax, AST/HIR representation, semantic validation, and diagnostic wording.
- `generator`, `archetype`, `body`, `move`, `moveset`, `space`, `terrain`, `deform`, and region composition are language semantics, not runtime-only conveniences.
- New declaration families may lower into dedicated HIR arenas or typed declaration records, but they must not be represented as generic host `FunctionRole::Function` plus string tags.
- The term `body` in the source language must not be confused with `hir::Body`, which is the compiler's expression/statement body container. Implementation names should use explicit Rust nouns such as `GameplayBodyDecl`, `BodyBundle`, or `DynamicBodyDescriptor` when ambiguity would hurt readability.

### Contract and plan ownership

- `compiler/query_contract` and `compiler/query_plan` own semantic query families and planning recipes.
- `compiler/collision_contract`, `compiler/collision_plan`, and `compiler/collision_exec` own physical/contact/query execution for collision and traversal workloads.
- `compiler/presentation_contract`, `compiler/presentation_plan`, and `compiler/presentation_exec` own authored view metadata, frame/presentation contracts, pass planning, and rendering execution.
- New gameplay, audio, and product-runtime surfaces should follow this pattern with narrow modules such as `compiler/gameplay_contract`, `compiler/gameplay_plan`, `compiler/audio_contract`, `compiler/audio_plan`, and `compiler/audio_exec` when the concepts are not naturally owned by existing query/collision/presentation modules.
- Do not add broad `engine`, `desktop`, or `game` catch-all modules to hide unresolved ownership. New crates or apps are allowed only when this RFC names their boundary and proof lane.

### Runtime ownership

- `runtime/src/state_advance.rs` owns deterministic state transition records and sealed-snapshot handoff vocabulary.
- `runtime/src/domain_abi.rs` owns the host ABI boundary. The current narrow tick ABI must be widened or versioned before it can carry production gameplay input.
- `runtime/src/engine_executor.rs` owns bounded task scheduling and worker execution; long-lived device callbacks such as audio output do not become normal engine-frame jobs.
- Gameplay replay/save/checkpoint support must live in a dedicated runtime module or clearly named state-advance submodule. Certification `replay_trace` tooling is not a gameplay replay substrate.

### Identity and artifact ownership

- `compiler/world_identity` owns stable world, bundle, instance, and snapshot identity vocabulary.
- `compiler/artifact_key`, `compiler/artifact_contract`, and `compiler/artifact_store` own derived artifact keys, validity, compatibility, and storage.
- No gameplay-critical identity may depend on transient insertion order, runtime vector index, pointer address, or a 32-bit feature id unless the RFC explicitly proves the collision and compatibility story.
- Portable projections may remain compact, but the authoritative gameplay/save/replay identity must have a wider stable hash or descriptor chain when 32-bit projections are insufficient.

### Workflow ownership

- `just` remains the canonical repo front door.
- `wrela` remains the authored-world and product-facing surface.
- New proof lanes must state what they prove: compile, semantic language behavior, runtime determinism, replay/save, presentation quality, audio, performance, or ship readiness.
- `just ship` may stay smaller than the most expensive representative closure lane only if the docs explicitly name the omitted lane and the machine/time reason.

## Required Contract Artifacts

Before a phase can implement execution-heavy behavior, it must land the relevant contract artifact first.
This RFC prefers over-specification at contract boundaries because underspecified seams are where hacks otherwise enter.

The required artifacts are:

1. **Gameplay declaration ownership artifact.** Names all new declaration records, HIR/module storage, import/private-block behavior, visibility rules, diagnostics, and lowering entry points.
2. **Identity artifact.** Defines compiled bundle identity, instantiation identity, stable runtime handles, source-version hashing, snapshot lineage, region instance keys, and artifact-key participation.
3. **World composition artifact.** Defines `RegionPlanItem`, `ScatterDescriptor`, conditional branch descriptors, topology containers, residency plans, composition tiers, conflict resolution, and digest output.
4. **Gameplay frame artifact.** Defines typed inputs, fixed-step stage order, sealed snapshot format, replay frame record, checkpoint record, save compatibility record, and failure/rejection reasons.
5. **Traversal contract artifact.** Defines ride-surface, landing, airtime, attack-window, and recovery-clearance input/output records and states whether each record is owned by query, collision, gameplay, or a bridge module.
6. **Mutation artifact.** Defines terrain mutation descriptors, support-bounded invalidation, snapshot-before/after lineage, artifact invalidation tickets, resident-scene update policy, and deterministic replay ordering.
7. **Audio artifact.** Defines audio observer contracts, control-frame schema, mix/voice limits, device callback policy, audio subsystem report, perf budget fields, offline render output, and determinism evidence.
8. **Visual contract artifact.** Defines PBR/material quality metadata, directional shadow contract, sky/atmosphere metadata, traversal camera/readability metadata, hero FX taxonomy, cost labels, and lookdev scenario identity.
9. **Product host artifact.** Defines the playable macOS host boundary, workspace crate/app layout, authored project layout, product commands, bundle command, and handoff checklist.

Each artifact can be a Rust module, RFC addendum, checked-in design note, or strongly typed test fixture.
It is not complete if the implementation depends on an undocumented convention known only to the implementer.

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
   They should be treated as absent until parser, AST/HIR, typecheck, lowering, public-surface tests, and identity tests prove otherwise.
   A declaration name appearing in RFC 0001 is not implementation evidence.

2. **Region composition exists, but executable streaming composition is not finished.**
   The live repo still rejects parameterized regions, does not yet make `space` a live authored topology surface, and `compiler/query_exec/region.rs` still returns explicit runtime errors for `scatter` and conditional region execution.
   The current `scatter` syntax is descriptor-poor; it needs seed, density/mask/slot, ordering, and identity semantics before it can support save-stable generation.

3. **The runtime has fixed-step state machinery, but not yet a production gameplay frame contract.**
   There is a strong `state_advance` spine, but no complete story yet for player input, replay, checkpointing, deterministic game saves, or representative encounter playback.
   The current domain ABI is too narrow for production gameplay input and must be widened or versioned deliberately.

4. **Presentation and collision are strong, but not yet game-family aware.**
   The repo can render and query rich field worlds, but traversal contacts, airtime attack windows, ride-surface queries, and boss deformation are not yet first-class engine contracts.
   Existing generic ray, surface, sweep, and time-of-impact contracts are foundations, not substitutes for typed traversal/encounter records.

5. **The engine frame can measure subsystems, but not all necessary subsystems exist yet.**
   Presentation and collision have meaningful reports. Gameplay and audio do not.

6. **Visual presentation has real foundations, but not production traversal visual contracts.**
   Current lighting, radiance, media, temporal history, and pass planning are useful, but soft shadows, sky/atmosphere, traversal camera metadata, hero FX, and their cost labels are not first-class contracts yet.

7. **The product host surface is not a playable traversal game shell yet.**
   The repo has a useful frame-live inspection app, but no permanent `games/traversal_boss` project, no playable macOS game host boundary, and no representative game proof lanes.

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

11. **Contracts land before execution-heavy shortcuts.**
    If a phase introduces a new domain noun such as ride surface, audio observer, deformation ticket, directional shadow, traversal camera, or game save, the typed contract must land before backend execution grows around it.

12. **Representative content must arrive early enough to constrain architecture.**
    The permanent traversal-boss project does not need to be visually complete early, but its project id, directory, entry module, scenario manifest, and fixture placeholders must land in Phase 63B2.
    Phases 64 through 71 should grow that same project instead of proving themselves only against synthetic or disposable fixtures.
    Late-only representative content is a risk because earlier phases may optimize for benches that do not constrain the product shape.

13. **Existing RFC 0001 language semantics are the default.**
    This RFC should not silently redefine `generator`, `archetype`, `body`, `move`, `moveset`, `region`, `scatter`, `transition`, or `space`. Any divergence from RFC 0001 must be called out as a deliberate semantic change with migration notes.

14. **Do not confuse debug replay with gameplay replay.**
    Certification `replay_trace` artifacts can remain useful for debugging, but gameplay replay/save/checkpoint records must be first-class runtime data tied to `SimulationTick`, `TemporalClock`, sealed snapshots, and stable world identity.

15. **No hidden 32-bit gameplay identity ceiling.**
    Compact IDs may remain in portable kernels, but save/replay/runtime authority must use stable descriptors or wide hashes when collisions would change gameplay, artifact invalidation, or replay behavior.

16. **Visual quality must be contract-addressable.**
    PBR, shadows, sky, atmosphere, camera readability, and FX cannot enter only as WGSL constants or runtime knobs. Their authored intent, quality choices, and cost labels must survive through HIR, presentation contracts, planning, execution, reports, and representative lookdev scenarios.

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
12. The repo contains explicit contract artifacts for declaration ownership, identity, world composition, gameplay frame/replay/save, traversal records, mutation invalidation, audio, visuals, and product hosting.
13. No new gameplay, audio, deformation, visual, or product-host surface depends on undocumented string labels, transient indexes, hidden global state, or runtime-only sidecars.
14. The representative project appears in smoke, semantic, runtime, replay/save, lookdev, and performance evidence at the smallest truthful scope for each lane.
15. Any omitted or machine-limited closure is recorded with a concrete command, date, machine limitation, and the narrowest missing evidence, instead of being implied as complete.

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

#### Task 63A0 — Land the gameplay declaration ownership artifact

**Description**

Before adding parser support, write down the exact compiler ownership model for the new declaration families.
This prevents `generator`, `archetype`, `body`, `move`, `moveset`, and `space` from becoming loosely tagged host functions that later phases cannot reason about.

**Files**

- new `compiler/gameplay_contract/mod.rs` or checked-in design note if the first landing is doc-only
- `compiler/hir/def.rs`
- `compiler/hir/project.rs`
- `compiler/tests/spec_project_integrity.rs`
- this RFC if the final ownership map changes

**Implementation notes**

The artifact must answer:

- whether each declaration is stored in a dedicated HIR arena, a typed declaration table, or a carefully named function-lane variant
- how `private` blocks, imports, duplicate names, public-surface extraction, and diagnostics work
- how source-language `body` is named in Rust code without colliding conceptually with `hir::Body`
- which declarations may contain nested detail fields, material/radiance/volume exports, `terrain`, `collision`, `payload`, `deform`, phases, predicates, or residency entries
- which declarations are portable, host-owned, or mixed
- which lowering pass first turns a declaration into a compiled bundle identity

Code sketch:

```rust
pub struct GameplayDeclarationOwnership {
    pub declaration: SmolStr,
    pub kind: GameplayDeclarationKind,
    pub visibility: hir::Visibility,
    pub source_origin: hir::SourceOrigin,
    pub portable_boundary: GameplayPortableBoundary,
}

pub enum GameplayDeclarationKind {
    Generator,
    Archetype,
    DynamicBody,
    MoveProgram,
    Moveset,
    Space,
}
```

**Acceptance criteria**

- The ownership artifact is checked in before nontrivial parser/lowering work depends on it.
- New declaration names, module ownership, and public Rust types are explicit.
- The artifact states which RFC 0001 semantics are implemented now, deferred, or deliberately changed.
- A reviewer can determine where each declaration kind should be parsed, typechecked, lowered, tested, and documented.

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
pub enum GameplayDeclKind {
    Generator,
    Archetype,
    DynamicBody,
    MoveProgram,
    Moveset,
    Space,
}

pub struct GameplayDecl {
    pub name: SmolStr,
    pub kind: GameplayDeclKind,
    pub params: Vec<hir::Arg>,
    pub items: Vec<GameplayDeclItem>,
    pub span: TextRange,
}

pub enum GameplayDeclItem {
    DetailField(GameplayDetailField),
    TerrainField(GameplayDetailField),
    CollisionField(GameplayDetailField),
    MaterialExport(SmolStr),
    RadianceExport(SmolStr),
    VolumeExport(SmolStr),
    Payload(hir::Body),
    Transform(hir::Body),
    Deform(hir::Body),
    MoveDuration(hir::Body),
    MovePhase(GameplayMovePhase),
    MovesetTransition(GameplayMovesetTransition),
    SpaceResidency(GameplaySpaceResidency),
}
```

**Acceptance criteria**

- Positive parser tests exist for each new declaration kind.
- Negative parser tests exist for malformed declarations and misplaced clauses.
- AST and HIR can represent these declarations without stringly typed escape hatches.
- Parser tests prove `body` and compiler-internal `hir::Body` naming remain unambiguous in Rust APIs.
- Tests cover declarations inside and outside `private` blocks, duplicate-name diagnostics, and invalid nested items.
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
    CompilerVisibleSupport,
    NoHostCallbackInPortableDeclaration,
    StableSourceOrderTieBreak,
}
```

**Acceptance criteria**

- The compiler rejects non-portable generator parameters and opaque host bindings.
- `archetype`, `body`, `terrain`, `collision`, `payload`, and `deform` items reject host references, raw pointers, imported runtime handles, and ambient mutable state.
- `move` declarations require explicit deterministic phase timing.
- `move` phase intervals are non-overlapping, finite, ordered, and exactly define how boundary-crossing ticks are evaluated or rejected.
- `moveset` declarations fail if referenced moves are missing or incompatible.
- `moveset` transition predicates are side-effect free and cannot allocate, block, perform host I/O, or depend on wall-clock time.
- `space` declarations reject opaque runtime topology callbacks.
- Diagnostics suggest the correct escape hatch: ordinary host `system` code for orchestration, portable helpers for analyzable math, or a future RFC if the semantic model is missing.
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
- runtime handle identity for dynamic instances whose state changes without changing compiled declaration identity
- portable projection identity for GPU/query kernels that need compact references but are not authoritative for save/replay

The runtime should be able to say “this is generator bundle X” separately from “this region/object instance was created from X with parameter hash Y”.
That distinction matters because generators still have no standalone runtime identity of their own and should only appear in replay/save surfaces through instantiated region/object descriptors.

Hash inputs must be named.
At minimum, compiled identity includes declaration kind, canonical module path, declaration name, checked source body, relevant public helper dependencies, semantic version, and compatibility salt.
Instantiation identity includes compiled identity, canonical encoded parameters, explicit seed, source-version hash, topology container identity when applicable, and stable authored placement path.

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
    pub seed_hash: u128,
    pub topology_path_hash: Option<u128>,
    pub runtime_instance_hash: Option<u128>,
}

pub struct StableGameplayHandle {
    pub bundle: BundleInstantiationIdentity,
    pub lineage_hash: u128,
    pub snapshot_epoch: SnapshotEpoch,
    pub portable_projection: Option<u32>,
}
```

**Acceptance criteria**

- New authored declarations produce stable compiled identities for artifact keys and separate instantiated identities for replay/save references.
- Generators never become free-standing runtime identities; only their instantiated uses are replay/save-addressable.
- Scene/world lowering can carry these declarations forward without lossy string labels.
- Runtime instance handles remain stable under replay and do not depend on insertion order.
- Any compact portable projection has a documented authoritative wide identity behind it.
- Identity tests cover source-order changes, helper-body changes, parameter changes, seed changes, and unchanged clean builds.
- Tests prove identity stability under repeated clean builds with unchanged source.

#### Task 63B2 — Add the permanent representative project scaffold and canonical authored sample

**Description**

Create the permanent `games/traversal_boss` scaffold as soon as the new gameplay declarations can compile.
This task is intentionally early: later world composition, traversal, audio, visual, and perf work must grow the same project instead of proving themselves against throwaway samples.

**Files**

- new `games/traversal_boss/`
- new `games/traversal_boss/README.md`
- new `games/traversal_boss/project.toml` or equivalent project manifest
- new `games/traversal_boss/src/main.wr`
- new `games/traversal_boss/src/world.wr`
- new `games/traversal_boss/src/rider.wr`
- new `games/traversal_boss/src/boss.wr`
- new `games/traversal_boss/src/view.wr`
- new `games/traversal_boss/scenarios/traversal_boss.toml`
- new `games/traversal_boss/fixtures/replay/.gitkeep`
- new `games/traversal_boss/fixtures/save/.gitkeep`
- `compiler/tests/preview_project.rs`
- `compiler/tests/repo_smoke.rs` or equivalent layout-presence test

**Implementation notes**

This scaffold is compile-and-analyze proof first, not full runtime proof yet.
Keep it small.
Its job is to prevent the new language surface from fragmenting before runtime work begins and to give Phases 64 through 71 a permanent product-shaped target.
Audio is intentionally absent; Phase 68 closes audio on the observer/runtime side instead of by parser syntax.

The initial scaffold must include stable identifiers even if many fixtures are placeholders:

- project id
- source bundle id/hash policy
- authored entry module
- first scenario id
- target view id
- seed/parameter bundle
- replay fixture placeholder path
- save fixture placeholder path
- known incomplete proof lanes

The scaffold may also expose a tiny preview-compatible entry so existing preview tests can exercise it before the product host exists.

**Acceptance criteria**

- The permanent `games/traversal_boss` directory exists before Phase 64 begins.
- The project has a stable project id and scenario id checked into a manifest, not inferred from directory order.
- The authored sample compiles through parse, HIR, typecheck, and lowering.
- The authored sample includes at least one `generator`, `body`, `move`, `moveset`, `archetype`, and `space`.
- Placeholder replay/save/scenario files exist with explicit "not yet executable" metadata rather than missing paths.
- CI-friendly tests confirm the project scaffold, entry module, manifest, and sample stay buildable as later phases land.
- Later phase tasks that need representative content must reference this project unless they explicitly justify a synthetic fixture.

#### Task 63B3 — Add public-surface and diagnostic stability tests for gameplay declarations

**Description**

Make the new language surface durable by testing it like a public feature, not a temporary parser experiment.

**Files**

- `compiler/tests/spec_project_integrity.rs`
- `compiler/tests/preview_project.rs`
- `compiler/tests/fixtures/help_text_snapshot.txt` if command help changes
- `games/traversal_boss/tests/.artifacts/public_surface/current.json`

**Implementation notes**

The first gameplay declaration landing should add a public-surface snapshot for the representative traversal-boss project scaffold.
Do not wait until the project is visually playable.
The snapshot must include declaration names, kinds, exports, and identity summary fields that future phases are expected to preserve or intentionally migrate.

**Acceptance criteria**

- Public-surface snapshots include gameplay declaration kinds and stable identity summaries.
- Diagnostic snapshots cover malformed gameplay declarations, invalid portable boundaries, missing moves, invalid move phases, and invalid `space` residency.
- Public-surface and diagnostic tests fail when a new declaration silently lowers as an ordinary host function.

## Phase 63 exit criteria

- The missing gameplay-native declarations are live compiler concepts, not RFC-only words.
- Stable identities exist for those declarations.
- The permanent representative project scaffold exists and its small authored sample compiles cleanly.
- Public-surface, identity, and diagnostic evidence make the language surface durable for later runtime phases.

---

# Phase 64: Executable World Composition, `space` Topology, And Deterministic Generation

## Goal

Finish the executable streamed-world model needed for giant traversal landscapes and deterministic encounter spaces.

## Why this phase exists

This game family lives or dies on world shape.

If parameterized regions, seeded scatter, conditional encounter layouts, and residency topology remain half-real, every later gameplay and visual system will be forced to fake a smaller world than the design actually wants.

### Workstream A: Executable region closure

#### Task 64A0 — Define the executable world-composition descriptor schema

**Description**

Before removing stubs, define the typed descriptor schema that region execution, world planning, save/replay, artifact invalidation, and perf reporting will all consume.

**Files**

- new `compiler/world_plan/mod.rs`
- `compiler/query_exec/region.rs`
- `compiler/query_exec/world.rs`
- `compiler/world_identity/mod.rs`
- `compiler/tests/region_exec.rs`

**Implementation notes**

The schema must be able to represent existing `place`, `overlay`, and `replace` items, but it must also make `scatter` and structural `if` executable without hiding important identity decisions in runtime code.

Code sketch:

```rust
pub struct RegionCompositionPlan {
    pub region: RegionInstanceKey,
    pub inputs: RegionExecutionInputs,
    pub items: Vec<RegionPlanItem>,
    pub conflict_policy: RegionConflictPolicy,
}

pub struct RegionExecutionInputs {
    pub parameters_hash: u128,
    pub seed: u64,
    pub phase_hash: Option<u128>,
    pub source_version_hash: u128,
}

pub enum RegionPlanItem {
    Place(PlacedContribution),
    Overlay(PlacedContribution),
    Replace(ReplacementContribution),
    Scatter(ScatterDescriptor),
    Conditional(ConditionalRegionPlan),
}

pub struct ScatterDescriptor {
    pub slot_name: SmolStr,
    pub seed_hash: u128,
    pub support_mask: Support3,
    pub density_or_count: ScatterDensity,
    pub ordering: ScatterOrdering,
    pub placement_identity_rule: ScatterIdentityRule,
    pub items: Vec<RegionPlanItem>,
}
```

**Acceptance criteria**

- Region execution has one typed descriptor path for `place`, `overlay`, `replace`, `scatter`, and structural conditionals.
- The schema names seed, mask/support, density/count, ordering, and identity rules for scatter.
- Conflict resolution references RFC 0001 semantics: distance, composition tier, declaration priority if present, source order, then stable declaration identity.
- Tests can inspect the descriptor without running presentation or collision.

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
    pub source_version_hash: u128,
    pub topology_path_hash: Option<u128>,
    pub compatibility_version: u32,
}
```

**Acceptance criteria**

- Parameterized regions no longer fail semantic lowering or execution solely because they have parameters.
- Region parameters are checked for portable, deterministic, canonical encoding before they participate in identity.
- Region instances can be captured and referenced by stable keys.
- Region instance keys do not depend on residency order or runtime insertion order.
- Region instance keys record enough source/version information to support save compatibility diagnostics.
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
`if` predicates in region composition must evaluate from explicit region inputs, phase state, or deterministic portable values.
They must not depend on camera position, frame index, wall time, or per-sample ray state.

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
- Equal-distance, overlapping, or replacement conflicts are resolved through the typed conflict policy rather than `HashMap` iteration order.
- Scatter execution proves that one region can reproduce its placements without requiring the whole world to be resident.
- Diagnostics reject unsupported scatter descriptors before execution when seed/mask/order cannot be made deterministic.
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
- `runtime/src/engine_executor.rs`
- `compiler/artifact_store/mod.rs`
- `compiler/gpu_runtime/resident_scene.rs`
- `compiler/engine_frame/mod.rs`

**Implementation notes**

This task is the lowering and runtime closure for live authored `space` topology.
`world_plan` is allowed only as the lowered form of a live authored `space`, not as a runtime-only sidecar that postpones authored topology semantics.
The first implementation should support only the topology containers needed by the representative traversal project, but it must reject unsupported containers explicitly rather than accepting them as inert labels.
Because this is runtime closure, not just lowering, it must also define deterministic residency transitions, prefetch/evict order, artifact-store participation, resident-scene updates, scheduling ownership, memory/upload budgets, and report fields.

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
    pub entry_name: SmolStr,
    pub member_kind: WorldTopologyKind,
    pub source_region_or_bundle: SmolStr,
    pub follow_handle: Option<StableHandle>,
    pub resident_radius: u32,
    pub preload_radius: u32,
    pub eviction_policy: ResidencyEvictionPolicy,
    pub digest_rule: ResidencyDigestRule,
}

pub struct SpaceLoweringPlan {
    pub source_space: SmolStr,
    pub member_kinds: Vec<WorldTopologyKind>,
    pub composition_order: Vec<SpaceCompositionTier>,
    pub residency: Vec<WorldResidencyPlan>,
    pub dynamic_entries: Vec<DynamicSpaceEntry>,
    pub unsupported_entries: Vec<UnsupportedSpaceEntry>,
}

pub struct WorldResidencyTransition {
    pub clock: TemporalClock,
    pub before_digest: WorldResidencyDigest,
    pub after_digest: WorldResidencyDigest,
    pub prefetch: Vec<RegionInstanceKey>,
    pub activate: Vec<RegionInstanceKey>,
    pub evict: Vec<RegionInstanceKey>,
    pub artifact_keys: Vec<ArtifactKey>,
    pub ordering_hash: u128,
}

pub struct WorldStreamingBudget {
    pub max_resident_regions: u32,
    pub max_prefetch_regions_per_frame: u32,
    pub max_evictions_per_frame: u32,
    pub max_upload_bytes_per_frame: u64,
    pub max_artifact_builds_per_frame: u32,
}

pub struct WorldStreamingReport {
    pub scenario_id: RepresentativeScenarioId,
    pub transition: WorldResidencyTransition,
    pub budget: WorldStreamingBudget,
    pub resident_scene_updates: Vec<ResidentSceneUpdateId>,
    pub budget_misses: Vec<WorldStreamingBudgetMiss>,
}
```

**Acceptance criteria**

- At least one authored `space` lowers into a truthful resident-region plan for traversal gameplay.
- Residency policy is explicit and testable.
- The plan preserves a stable authored composition order across static regions, transitions, replacements, and dynamic contributors.
- The representative `SparseBands` and `Singleton` entries lower without runtime sidecars.
- Unsupported topology containers produce diagnostics or explicit unsupported entries, not silent no-ops.
- `space` lowering records which region instances are resident, preloaded, evicted, and dynamically inserted for a representative rider/boss clock.
- No runtime-only topology sidecar is required to express the representative game's world layout.
- Runtime residency transitions are deterministic records with before/after digests and stable prefetch/activate/evict ordering.
- Streaming uses artifact keys and validity predicates for generated regions instead of rebuilding anonymous resident content.
- Resident-scene updates report which artifacts were reused, rebuilt, uploaded, or evicted.
- Engine-executor ownership is explicit for prefetch/build/upload work; streaming cannot run as an unbounded host callback.
- Engine-frame output includes world streaming report fields and budget misses for representative traversal scenarios.
- Tests cover unchanged replay clocks, crossing a residency boundary, eviction under budget pressure, and replay divergence when the expected residency digest changes.

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
    pub composition_order_hash: u128,
    pub conflict_policy_hash: u128,
}

pub struct WorldResidencyDigest {
    pub source_space: SmolStr,
    pub clock: TemporalClock,
    pub resident_region_hash: u128,
    pub dynamic_entry_hash: u128,
    pub eviction_hash: u128,
}
```

**Acceptance criteria**

- Unchanged seeds and parameters reproduce identical generated-region digests.
- Replay/save tests prove residency order does not affect generated identity.
- Digest tests prove that supported source changes, parameter changes, seed changes, and phase changes produce expected digest differences.
- The repro tooling can print a compact before/after diff of region, scatter, and residency digest fields.
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

#### Task 65A0 — Define the gameplay input, sealed snapshot, replay, and save schema

**Description**

Define the data model before widening the runtime.
The current runtime has useful clock/input/transition vocabulary, but a production traversal game needs typed player input, deterministic frame stages, sealed snapshot identity, replay records, and save/checkpoint compatibility.

**Files**

- `runtime/src/domain_abi.rs`
- `runtime/src/state_advance.rs`
- new `runtime/src/replay.rs`
- `compiler/state_advance/mod.rs`
- `compiler/world_identity/mod.rs`

**Implementation notes**

The schema must distinguish:

- raw device input sampled by the host
- normalized gameplay input consumed by the fixed-step gameplay frame
- authored intent consumed by `moveset`
- stable device ids, stable control ids, and input-map versions
- analog normalization, deadzones, clamping, and digital/analog hysteresis
- fixed-tick resampling, late input, dropped input, pause/resume, and focus-loss policy
- debug overrides that must never appear in normal replay unless explicitly recorded
- sealed snapshots used by replay/save/checkpoint
- compatibility metadata used when source or bundle versions change
- canonical serialization for replay/save payloads, including ordering, endianness, component schema ids, migration behavior, and hash inputs

Code sketch:

```rust
pub struct GameplayInputFrame {
    pub tick: SimulationTick,
    pub sample_window: InputSampleWindow,
    pub input_map_version: u32,
    pub canonicalization_policy: InputCanonicalizationPolicy,
    pub devices: Vec<DeviceInputSample>,
    pub controls: Vec<CanonicalControlSample>,
    pub intents: Vec<GameplayIntent>,
    pub late_or_dropped: Vec<InputSampleRejection>,
    pub debug_override_hash: Option<u128>,
    pub canonical_hash: u128,
}

pub struct DeviceInputSample {
    pub device_id: StableDeviceId,
    pub control_id: StableControlId,
    pub host_timestamp: HostInputTimestamp,
    pub raw_value: RawInputValue,
    pub normalized_value: NormalizedInputValue,
    pub canonical_tick: SimulationTick,
}

pub struct SealedGameplaySnapshot {
    pub snapshot: WorldSnapshotHandle,
    pub clock: TemporalClock,
    pub source_compatibility_hash: u128,
    pub bundle_registry_hash: u128,
    pub state_payload: CanonicalStatePayload,
    pub state_hash: u128,
}

pub struct CanonicalStatePayload {
    pub schema_version: u32,
    pub byte_order: CanonicalByteOrder,
    pub component_schemas: Vec<StateComponentSchema>,
    pub components: Vec<StateComponentPayload>,
    pub ordering_hash: u128,
    pub payload_hash: u128,
}

pub struct GameplaySaveCompatibility {
    pub schema_version: u32,
    pub compiler_semantic_version: u32,
    pub required_bundle_hashes: Vec<u128>,
    pub migration_policy: SaveMigrationPolicy,
}
```

**Acceptance criteria**

- Input, replay, checkpoint, and save records have typed fields rather than generic strings.
- Replay records carry enough data to re-run fixed-step gameplay without live device input.
- Save/checkpoint records carry schema and bundle compatibility metadata.
- Debug overrides are opt-in, recorded, and rejected by default in canonical replay/save tests.
- Input canonicalization records stable device/control ids, input-map version, normalization/deadzone policy, fixed-tick resampling, and late/dropped input decisions.
- Debug override participation in the canonical input hash is explicit, including the default rejection path.
- Replay/save payload serialization is canonical: ordering, endianness, component schema ids, compatibility hashes, and migration policy are all named.
- Tests prove host device ordering, platform-specific timestamps, and equivalent analog samples do not change canonical gameplay input when the canonicalization policy says they are equivalent.
- Tests prove save/replay hash mismatches report the first divergent input, component schema, stage, or snapshot hash.

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
    pub inputs: GameplayInputFrame,
    pub active_space: SpaceRuntimeDescriptor,
    pub bundle_registry: GameplayBundleRegistry,
    pub debug_overrides: GameplayDebugOverrides,
}

pub enum GameplayStage {
    Input,
    SpaceResidencyUpdate,
    MovesetSelect,
    MoveSolve,
    TraversalContactSolve,
    EncounterSolve,
    WorldMutation,
    AudioControlObserve,
    SnapshotSeal,
}

pub struct GameplayFrameOutput {
    pub sealed_snapshot: SealedGameplaySnapshot,
    pub stage_reports: Vec<GameplayStageReport>,
    pub replay_record: ReplayFrameRecord,
    pub engine_frame_subsystem_report: EngineSubsystemReport,
}
```

**Acceptance criteria**

- One gameplay frame has a named, documented, and testable stage order.
- Input is explicit data, not ambient global state.
- Authoritative timing flows from `SimulationTick`, `PresentationFrame`, and `TemporalClock` rather than a parallel floating-point delta clock.
- The runtime can produce a sealed snapshot after each fixed step.
- Stage reports include deterministic hashes for residency update, move solve, contact solve, encounter solve, mutation, and snapshot seal.
- The host ABI is versioned or widened so gameplay frame input is not squeezed through axis-only parameters.
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
    pub clock: TemporalClock,
    pub inputs: GameplayInputFrame,
    pub stage_hashes: Vec<GameplayStageHash>,
    pub snapshot_hash: u128,
    pub active_region_hash: u128,
    pub mutation_hash: u128,
    pub audio_control_hash: Option<u128>,
}

pub struct CheckpointRecord {
    pub schema_version: u32,
    pub current_clock: TemporalClock,
    pub sealed_snapshot: SealedGameplaySnapshot,
    pub compatibility: GameplaySaveCompatibility,
}

pub struct SaveGameRecord {
    pub schema_version: u32,
    pub project_id: SmolStr,
    pub created_with_source_hash: u128,
    pub checkpoint: CheckpointRecord,
    pub user_progress_payload: CanonicalSavePayload,
}

pub struct CanonicalSavePayload {
    pub schema_version: u32,
    pub byte_order: CanonicalByteOrder,
    pub sections: Vec<SavePayloadSection>,
    pub migration_history: Vec<SaveMigrationRecord>,
    pub payload_hash: u128,
}
```

**Acceptance criteria**

- Replays can drive the gameplay frame deterministically.
- Checkpoint schema versioning is explicit.
- User-facing saves and developer replay records are separate types with separate tests.
- Replay mismatch diagnostics name the first divergent stage and the expected/observed hash.
- Save/load tests cover unchanged source, compatible source changes, and explicitly incompatible source changes.
- Save/load and replay tests prove stable results for unchanged source and identical inputs.
- Save payload sections are typed and canonically ordered; opaque byte blobs are allowed only as named, versioned sections with schema ids, ownership, and hash participation.
- Save migration tests prove old compatible payloads either migrate deterministically or fail with a typed compatibility diagnostic.

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
    pub lineage_hash: u128,
    pub descriptor_version: u32,
    pub current_snapshot: WorldSnapshotHandle,
}
```

**Acceptance criteria**

- Dynamic instances have stable handles.
- Archetype/body/moveset descriptors can be resolved from a sealed snapshot.
- Descriptor lookup is independent of insertion order and survives save/load round trips.
- Registry reports can explain missing, incompatible, or migrated bundle descriptors.
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
- `wrela replay` or an equivalently named command consumes gameplay replay records, not certification-only replay traces.
- Tooling can dump stage hashes, active move/moveset state, stable handles, active regions, and mutation tickets for a replay window.
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

#### Task 66A0 — Define traversal contract ownership and typed records

**Description**

Decide which traversal questions are query contracts, collision contracts, gameplay solver records, or bridge records.
Do this before adding execution paths so traversal semantics do not get hidden inside generic collision results.

**Files**

- `compiler/query_contract/mod.rs`
- `compiler/collision_contract/mod.rs`
- `compiler/collision_plan/batch.rs`
- new `compiler/gameplay_contract/mod.rs`
- `compiler/tests/query_contract_registry.rs`
- `compiler/tests/collision_plan.rs`

**Implementation notes**

Ownership rule:

- broad geometric questions such as nearest, trace, occlusion, medium, and surface sampling stay in query contracts
- physical contact, sweep, time-of-impact, and witness data stay in collision contracts
- move legality, phase windows, input intent, and attack decisions stay in gameplay contracts
- traversal bridge records may combine query/collision evidence, but they must name their source contracts and evidence scope

Code sketch:

```rust
pub struct RideSurfaceContactRequest {
    pub body: StableGameplayHandle,
    pub world: WorldSnapshotHandle,
    pub pose: BodyPose,
    pub velocity: Vec3,
    pub probe: RideSurfaceProbe,
    pub domain: TraversalDomainDescriptor,
}

pub struct RideSurfaceContactResult {
    pub contact: Option<ContactWitness>,
    pub surface: Option<Surface>,
    pub slope: f32,
    pub traction: f32,
    pub rideable: bool,
    pub evidence: TraversalEvidenceSummary,
}

pub struct AttackReachWindowRequest {
    pub rider: StableGameplayHandle,
    pub target: StableGameplayHandle,
    pub motion_arc: MotionArc,
    pub phase_window: MovePhaseWindow,
    pub deformation_epoch: SnapshotEpoch,
}
```

**Acceptance criteria**

- Each traversal noun has an owning module and exact request/result record.
- Records name source contracts and evidence scope rather than packing opaque payload strings.
- Tests prove registry discoverability for traversal records and reject accidental use of generic collision batches where traversal records are required.

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

pub enum TraversalEvidenceKind {
    CollisionWitness,
    SurfaceSample,
    MediumSample,
    MovementState,
    DeformationEpoch,
}
```

**Acceptance criteria**

- Traversal query kinds are explicit and discoverable in the query contract registry.
- Plans can represent traversal-specific outputs without stringly typed payload conventions.
- Each traversal plan states whether it uses CPU-only authority, WGSL acceleration with CPU certification, or measured-but-nonauthoritative GPU evidence.
- Ride-surface contacts include support/candidate provenance so bad landing decisions can be debugged.
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
    pub deterministic_hash: u128,
    pub rejected_transitions: Vec<MoveTransitionRejection>,
}
```

**Acceptance criteria**

- The runtime can evaluate authored moves and movesets on a fixed timestep.
- Move transitions are deterministic under replay.
- Solver rules define source-order handling, priority classes, simultaneous transition tie-breaks, contact tolerance, and phase-boundary crossing behavior.
- Solver outputs can be serialized into replay stage hashes.
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
    pub evidence_policy: TraversalEvidencePolicy,
    pub max_hot_path_readback_bytes: u64,
}
```

**Acceptance criteria**

- Traversal queries can run in batches rather than as serial one-off probes.
- CPU parity tests exist for the accelerated path.
- Batched traversal reuses resident scene/candidate artifacts when compatible instead of rebuilding per query.
- Reports expose batch size, candidate reuse, CPU certification count, fallback count, and immediate readback bytes.
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

#### Task 67A0 — Define mutation lifecycle, snapshot lineage, and invalidation semantics

**Description**

Define what a terrain mutation is before implementing boss deformation execution.
The mutation path must connect authored `terrain`/`deform` semantics to snapshot lineage, traversal fairness, collision/presentation invalidation, resident-scene updates, replay, and perf reporting.

**Files**

- `compiler/world_identity/mod.rs`
- `compiler/artifact_contract/mod.rs`
- `compiler/artifact_key/mod.rs`
- `compiler/artifact_store/mod.rs`
- `runtime/src/state_advance.rs`
- this RFC if lifecycle naming changes

**Implementation notes**

Every mutation must have:

- an authored owner and stable runtime handle
- a source bundle and deform/terrain program identity
- before/after snapshot handles
- deterministic ordering relative to gameplay frame stages
- explicit support bounds and affected composition tiers
- a replay hash and rejection reason when the mutation cannot be applied
- affected artifact classes and invalidation scope

Code sketch:

```rust
pub enum TerrainMutationKind {
    Deform,
    Overlay,
    Replace,
    Remove,
}

pub struct TerrainMutationLifecycle {
    pub ticket: WorldMutationTicket,
    pub stage: GameplayStage,
    pub ordering_key: u128,
    pub replay_hash: u128,
    pub affected_tiers: Vec<SpaceCompositionTier>,
    pub invalidation: MutationInvalidationPlan,
}

pub struct MutationInvalidationPlan {
    pub collision_artifacts: Vec<ArtifactKey>,
    pub presentation_artifacts: Vec<ArtifactKey>,
    pub world_plan_artifacts: Vec<ArtifactKey>,
    pub resident_scene_update: ResidentSceneUpdatePolicy,
    pub full_rebuild_required: bool,
}
```

**Acceptance criteria**

- Mutation lifecycle records define before/after snapshot lineage and deterministic ordering.
- Invalidation scope is support-bounded when possible and explicitly falls back to full rebuild when not.
- Replay tests can identify mutation-order divergence independently of rendering/collision output.
- Mutation reports name affected artifact classes and resident-scene update policy.

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
    pub before_snapshot: WorldSnapshotHandle,
    pub expected_after_hash: u128,
}
```

**Acceptance criteria**

- Boss-authored terrain contribution is representable through live `terrain` and `deform` semantics rather than abusing generic geometry ownership.
- Terrain mutation carries explicit support and identity.
- `terrain`, ordinary visual `geometry`, and physical `collision` participation are distinct when the authored declaration makes them distinct.
- Mutations that cannot provide conservative support are rejected or marked as full-rebuild-required before steady-state closure can claim incremental updates.
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
    pub ordering_key: u128,
    pub compatibility: ChangeCompatibility,
}
```

**Acceptance criteria**

- Local deformation can update collision/presentation artifacts without full-scene rebuild in the common case.
- Invalidation scope is visible in reports and tests.
- Replay tests prove mutation ordering remains deterministic.
- Resident-scene updates report upload bytes, rebuilt node counts, stale artifact rejection, and fallback-to-full-rebuild reasons.
- Collision, presentation, and traversal queries observe the same mutation epoch for a gameplay frame.
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

#### Task 68A0 — Define audio subsystem contracts, budgets, and callback boundary

**Description**

Audio is currently greenfield in the live repo.
Before implementation, define the exact compiler/runtime/reporting boundary so the first audio landing is bounded and engine-frame compatible.

**Files**

- new `compiler/audio_contract/mod.rs`
- new `compiler/audio_plan/mod.rs`
- `compiler/engine_frame/mod.rs`
- `compiler/perf_target/mod.rs`
- `runtime/src/domain_abi.rs`
- new `runtime/src/audio_runtime.rs`
- `runtime/Cargo.toml`

**Implementation notes**

The contract artifact must define:

- whether a new dependency such as `cpal` is introduced, and under which target cfgs
- `EngineSubsystemKind::Audio` and/or an equivalent typed reporting surface
- audio control-frame schema
- hard voice/event/probe limits for the first implementation
- callback ownership, stale-frame behavior, underrun accounting, sample-rate renegotiation, and device-loss policy
- canonical audio budget fields in perf closure output
- why raw PCM is not the canonical replay artifact

Code sketch:

```rust
pub struct AudioControlFrame {
    pub clock: TemporalClock,
    pub listener: ListenerState,
    pub control_hash: u128,
    pub beds: Vec<AudioBedControl>,
    pub events: Vec<AudioEventControl>,
    pub spatial_emitters: Vec<AudioEmitterControl>,
}

pub struct AudioRuntimeBoundary {
    pub device_backend: AudioDeviceBackend,
    pub callback_realtime_safe: bool,
    pub max_control_frame_age: u32,
    pub max_voices: u32,
    pub max_events_per_frame: u32,
}

pub struct AudioBudgetContract {
    pub control_update_median_ms: f32,
    pub control_update_p95_ms: f32,
    pub callback_fill_p99_micros: u128,
    pub max_underruns_per_minute: u32,
}
```

**Acceptance criteria**

- The audio subsystem has named compiler, runtime, report, and perf-budget boundaries before synthesis execution grows.
- Audio appears in engine-frame/closure reporting as a typed subsystem or explicitly named reserved slot with migration path.
- Callback real-time constraints are testable by code review and targeted runtime tests.
- The first implementation has bounded voice/event/probe limits and rejects unbounded plans.

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
    pub max_probe_queries_per_control_frame: u32,
    pub stale_frame_policy: AudioStaleFramePolicy,
}

pub struct AudioMixContract {
    pub beds: Vec<AudioBedDescriptor>,
    pub events: Vec<AudioEventDescriptor>,
    pub spatialization: SpatializationMode,
    pub max_active_voices: u32,
    pub bus_layout: AudioBusLayout,
}
```

**Acceptance criteria**

- Audio plans derive from existing world capture and gameplay state, with no required new top-level authored declaration.
- Audio contracts can describe listener context, bounded world observations, control rate, and bounded voice/event sets.
- Audio plans lower into explicit audio-exec artifacts instead of dispatching directly from gameplay/runtime code.
- Audio plans name every query contract they may issue and cap query counts per control frame.
- Plans reject unbounded emitter creation, unbounded probe fans, and sample-rate world queries.
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
    pub max_voice_limit: u32,
    pub callback_fill_p99_micros: u128,
    pub underrun_count: u32,
    pub dropped_events: u32,
    pub stale_control_frame_count: u32,
    pub device_restarts: u32,
}
```

**Acceptance criteria**

- Audio control updates, handoff, and telemetry appear as named work in the canonical engine-frame closure output.
- Audio workload, callback latency, and underrun data are visible in reports.
- The long-lived device callback is not scheduled as an engine-frame task; only control-rate work and reporting are.
- The callback path performs no whole-world query work and no blocking host work.
- Stale control frames, sample-rate changes, and device-loss/renegotiation are handled by explicit runtime policy.
- Perf closure includes audio control-update cost, callback health, underruns, dropped events, and stale-frame counts.
- Audio tests can run in deterministic offline mode without requiring a live output device.
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

#### Task 69A0 — Define production material and shadow contract deltas

**Description**

Before adding shader work, define the authored and presentation-contract fields that make production material quality, directional lighting, and shadows visible to HIR, planning, execution, reports, and lookdev.

**Files**

- `compiler/hir/def.rs`
- `compiler/hir/lower.rs`
- `compiler/hir/typeck/types.rs`
- `compiler/presentation_contract/mod.rs`
- `compiler/presentation_plan/mod.rs`
- `compiler/presentation_exec/cost.rs`
- `compiler/tests/presentation_plan.rs`

**Implementation notes**

The contract delta must name:

- material quality knobs that are authored or selected by `view` metadata
- whether clearcoat, specular AA, terrain material LOD, and deformation-aware blending are part of `Surface`, `PresentationMetadata`, or a traversal material quality contract
- directional-light and sun coherence fields
- shadow atlas/cascade/clipmap contract fields
- invalidation inputs for deformed terrain
- presentation and engine-frame cost labels for material and shadow work

Code sketch:

```rust
pub struct TraversalMaterialContract {
    pub surface_model: SurfaceModel,
    pub terrain_lod: u8,
    pub deformation_blend: DeformationMaterialBlend,
    pub specular_antialiasing: bool,
    pub clearcoat_energy_compensation: bool,
}

pub struct DirectionalShadowContract {
    pub source_light: DirectionalLightId,
    pub strategy: DirectionalShadowStrategy,
    pub cascade_count: u32,
    pub clipmap_radius_meters: f32,
    pub filter_radius_pixels: f32,
    pub temporal_stabilization: bool,
    pub invalidation_policy: ShadowInvalidationPolicy,
}
```

**Acceptance criteria**

- Material and shadow controls survive HIR lowering into typed presentation contracts.
- Plans expose material/shadow quality choices without GPU-only hidden flags.
- Cost reports can distinguish base shading, material detail, shadow map/update, shadow sampling, and deformation-driven shadow invalidation.
- Lookdev tests can request a named material/shadow quality target.

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
    pub deformation_blend_budget: u8,
    pub cpu_wgsl_parity_tolerance: f32,
}
```

**Acceptance criteria**

- Representative terrain and boss materials show stable response under camera and light changes.
- CPU and WGSL shading paths stay parity-tested for the production surface model.
- Material contract tests prove authored/view quality metadata survives to `PresentationPlan`.
- Parity tests name tolerances and failure diagnostics for each material lobe.
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
    pub deformation_invalidation: ShadowInvalidationPolicy,
    pub max_shadow_update_bytes_per_frame: u64,
}
```

**Acceptance criteria**

- Representative traversal scenes have stable directional shadows across near and far terrain.
- Boss deformation invalidates only the shadow work it must invalidate.
- Shadow artifacts participate in artifact keys, validity predicates, and resident-scene update reports.
- Shadow scenarios include high-speed camera motion and deformation churn, not only static beauty shots.
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

- Hillaire-style scalable dynamic sky/atmosphere as the default production target unless the decision artifact proves it is unsuitable
- Bruneton-style precomputed atmosphere as a correctness/reference model and validation oracle
- Hošek-Wilkie-style fitted sky as a limited fallback/reference for sky-dome-only needs, not the full traversal atmosphere solution

The required output of this task is a short decision artifact checked into the repo or this RFC addendum that contains:

- the chosen implementation family
- a short tradeoff table comparing the alternatives
- the concrete `HIR` and `presentation_contract` deltas required to author sky/atmosphere in Wrela
- the artifact/LUT plan and invalidation rules
- explicit in-scope vs out-of-scope items for the representative project
- how the selected model maps to `radiance field`, `volume field`, view metadata, LUT artifacts, invalidation, GPU resources, and CPU/reference tests

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
- LUT/artifact generation, cache keys, invalidation, and fallback/reference paths are named before the first shader path lands.
- The decision artifact names a minimal first scenic-depth target and a deferred list so clouds and high-end volumetrics do not silently expand scope.
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
    pub sun: Option<Body>,
    pub implementation_family: SkyAtmosphereFamily,
    pub sun_disk_enabled: bool,
    pub aerial_perspective_enabled: bool,
    pub horizon_falloff: Option<Body>,
    pub multi_scatter_approximation: bool,
    pub lut_policy: SkyAtmosphereLutPolicy,
    pub invalidation_policy: SkyAtmosphereInvalidationPolicy,
}
```

**Acceptance criteria**

- Representative scenes have a production sky path instead of a placeholder gradient-only sky.
- Solar lighting and sky contribution remain visually coherent across the representative scenarios.
- Sky/sun/shadow coherence is contract-tested through the same authored `view` metadata.
- LUT/resource artifacts have stable keys and report rebuild/reuse behavior.
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

The first closure target should be scenic depth for terrain-scale vistas, not weather simulation.
At minimum, it should handle aerial perspective, dust/haze density driven by `volume field` / `Medium`, far-field silhouette stability, and sun/sky color coherence.

**Acceptance criteria**

- The representative traversal view uses the resident presentation path as the truthful hot path for atmosphere and scenic depth.
- Far-field and atmosphere costs are visible in perf reports.
- Scenic-depth reports distinguish sky lookup/LUT cost, aerial-perspective cost, medium sampling/probe cost, and post-composite cost.
- Optional clouds or layered high-altitude detail are either disabled, behind named quality settings, or covered by separate budget evidence.
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

#### Task 71A0 — Define traversal visual governance and scenario identity

**Description**

Before adding more effects, define the visual governance surface that says what counts as traversal readability, what counts as hero FX, how scenarios are named, and how cost labels map back to engine-frame reports.

**Files**

- `compiler/presentation_contract/mod.rs`
- `compiler/presentation_plan/mod.rs`
- `compiler/presentation_exec/cost.rs`
- `compiler/perf_target/mod.rs`
- new `benchmarks/lookdev/traversal_boss/README.md`

**Implementation notes**

This task prevents the visual phases from turning into an unbounded pile of post-processing.
The governance contract must name the exact visual categories that later execution code may report against.

Required categories:

- traversal camera/readability
- motion vectors and temporal stability
- material and terrain detail
- directional shadows
- sky, atmosphere, and scenic depth
- hero traversal FX
- boss deformation and encounter FX
- quality downgrades and fallback policies

Code sketch:

```rust
pub struct TraversalVisualGovernance {
    pub scenario_id: VisualScenarioId,
    pub readability_target: TraversalReadabilityTarget,
    pub fx_budget: HeroFxBudget,
    pub quality_floor: TraversalQualityFloor,
    pub cost_label_policy: TraversalVisualCostLabelPolicy,
}

pub enum TraversalVisualCostLabel {
    CameraAndMotion,
    BaseMaterial,
    MaterialDetail,
    DirectionalShadowUpdate,
    DirectionalShadowSample,
    SkyAtmosphere,
    ScenicDepth,
    HeroFx,
    DeformationVisualUpdate,
    TemporalResolve,
}
```

**Acceptance criteria**

- Traversal visual categories are defined in typed contracts before new backend-only FX code grows around them.
- Each category maps to stable cost labels used by presentation and engine-frame reports.
- Lookdev scenario identity is a stable typed value, not a free-form string in scattered benchmark files.
- Quality downgrades have named policies and report entries.
- The RFC's material, shadow, sky, atmosphere, readability, and FX phases use the same scenario and cost-label vocabulary.

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
- strike-window visibility
- landing-target visibility
- boss-scale reveal framing
- temporal history reset/invalidation behavior under abrupt traversal changes

These controls must be authorable and survive through HIR and the presentation contract layer before they reach controller/runtime policy.
The runtime controller may smooth and clamp camera behavior, but the authored contract must still own the intent and bounds.

Code sketch:

```rust
pub struct TraversalViewMetadata {
    pub camera_mode: TraversalCameraMode,
    pub follow_target: TraversalFollowTarget,
    pub look_ahead_seconds: f32,
    pub landing_focus_bias: f32,
    pub strike_window_visibility_bias: f32,
    pub boss_reveal_bias: f32,
    pub max_camera_accel: f32,
    pub fov_policy: TraversalFovPolicy,
    pub motion_vector_quality: MotionVectorQuality,
    pub horizon_stability_weight: f32,
    pub temporal_history_policy: TraversalTemporalHistoryPolicy,
}
```

**Acceptance criteria**

- Traversal readability metadata is authorable and survives HIR, typed presentation contracts, planning, and execution setup.
- Presentation plans can carry traversal-specific view metadata.
- Metadata has validation bounds and diagnostics for impossible or contradictory camera settings.
- The execution path cannot satisfy this task by storing host-only camera state outside the presentation contract.
- Tests prove traversal view metadata survives planning and execution setup.
- Motion history and camera setup remain deterministic under replay.
- Replay tests prove the same sealed gameplay snapshot and traversal view metadata produce the same camera intent and temporal-history decisions.

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
- damage/readiness telegraph reinforcement that does not replace gameplay records
- terrain-recovery highlights after deformation

Every effect must preserve silhouette readability, temporal stability, and explicit cost reporting.
FX should be driven by typed gameplay/traversal/deformation records, not by scraping presentation pixels or reading hidden GPU side channels.

Code sketch:

```rust
pub struct HeroFxEmitterContract {
    pub source: HeroFxSource,
    pub scenario_category: HeroFxScenarioCategory,
    pub visibility_policy: HeroFxVisibilityPolicy,
    pub max_particles_or_splats: u32,
    pub max_gpu_bytes_per_frame: u64,
    pub temporal_stability: HeroFxTemporalPolicy,
    pub cost_label: TraversalVisualCostLabel,
}

pub enum HeroFxSource {
    CarveContact(RideSurfaceContactResultId),
    Landing(MoveStepResultId),
    BossWake(WorldMutationTicket),
    StrikeWindow(AttackReachWindowResultId),
    EncounterTelegraph(EncounterSignalId),
}
```

**Acceptance criteria**

- Representative traversal and boss scenes have hero FX that improve spectacle without hiding gameplay state.
- Each hero FX emitter names its authoritative gameplay/traversal/deformation source record.
- FX execution is bounded by typed per-frame count and memory budgets.
- FX costs are visible in perf reports instead of disappearing into generic post-processing time.
- Temporal instability and readability regressions fail named quality scenarios.
- Tests prove disabling hero FX does not change authoritative gameplay, replay, collision, mutation, or audio-control results.

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

Each scenario must record:

- stable scenario id
- authored entry module and view
- seed and parameter bundle
- gameplay/replay snippet used to drive motion
- expected active regions and mutation state
- enabled quality profile
- expected report categories
- screenshot or image-stat thresholds, when visual image checks are practical

Code sketch:

```rust
pub struct TraversalLookdevScenario {
    pub id: VisualScenarioId,
    pub entry: AuthoredEntryPoint,
    pub view: ViewName,
    pub replay: Option<GameplayReplayFixture>,
    pub seed: u128,
    pub quality_profile: TraversalQualityProfile,
    pub expected_regions: Vec<RegionInstanceKey>,
    pub expected_cost_labels: Vec<TraversalVisualCostLabel>,
    pub image_thresholds: Vec<LookdevImageThreshold>,
}
```

**Acceptance criteria**

- The repo has named visual scenarios tied to the representative game family.
- Benchmarks and lookdev evidence use the same scenario identities, with those scenarios added to the canonical `engine_frame` manifests or accompanied by an explicit manifest-discovery change.
- Regressions can point to a concrete visual scenario instead of a generic scene hash.
- Scenario fixtures are deterministic across clean builds.
- At least one scenario drives camera motion from a gameplay replay fixture rather than a static preview camera.
- Documentation states which scenarios are required for normal `just ship`, which are required for `perf-game-closure`, and which are optional lookdev-only evidence.

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
- quality downgrades applied to recover frame time
- resident-scene artifact churn that prevents reuse

Code sketch:

```rust
pub struct TraversalVisualClosureFinding {
    pub scenario_id: VisualScenarioId,
    pub missed_budget: Option<FrameBudgetMiss>,
    pub dominant_cost_labels: Vec<TraversalVisualCostLabel>,
    pub quality_downgrades: Vec<QualityDowngradeRecord>,
    pub artifact_churn: Vec<ArtifactChurnRecord>,
    pub recommendation: TraversalClosureRecommendation,
}
```

**Acceptance criteria**

- Traversal-specific why-not-120 findings exist in perf closure output.
- Quality downgrades, if used, are explicit and reported.
- Findings use the same cost labels defined by the visual governance task.
- Reports can distinguish "visually closed but too slow", "fast enough but below quality floor", and "not representative because scenario fixture is incomplete".
- A scenario cannot be marked visually closed when required image/temporal/readability checks are absent.
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

#### Task 72A0 — Define the product host boundary and project layout

**Description**

Define the playable desktop product boundary before creating the representative game.
The existing preview and frame-live surfaces are valuable inspection tools, but they are not a substitute for a game host with input, fixed-step gameplay, audio, swapchain presentation, replay, save/load, and app packaging.

**Files**

- `Cargo.toml`
- `justfile`
- new `games/traversal_boss/`
- new `apps/traversal_boss_app/`
- `apps/frame_live_app/README.md` or equivalent docs if needed to distinguish app roles
- `compiler/bin/wrela/cli_args.rs`
- `compiler/bin/wrela/commands/command_dispatch.rs`

**Implementation notes**

The representative project should have two related but distinct surfaces:

- `games/traversal_boss/` owns authored world, gameplay declarations, representative fixtures, scenario manifests, docs, replay/save fixtures, and project-specific config.
- `apps/traversal_boss_app/` owns the macOS desktop shell: windowing, device input sampling, audio device setup, swapchain presentation, app lifecycle, packaging, and host-to-runtime wiring.

The desktop shell should be WGPU/WGSL-first for the current Mac target.
Do not route the shippable product through `preview` screenshots or the frame-live debug app.
The frame-live app can remain a debugging/inspection surface for prepared presentation bundles.

Code sketch:

```rust
pub struct TraversalBossHostBoundary {
    pub project_root: ProjectRoot,
    pub renderer_backend: RendererBackend,
    pub input_backend: HostInputBackend,
    pub audio_backend: HostAudioBackend,
    pub gameplay_runtime: GameplayRuntimeConfig,
    pub replay_policy: ProductReplayPolicy,
    pub save_policy: ProductSavePolicy,
    pub proof_surface: ProductProofSurface,
}

pub enum ProductProofSurface {
    CompileOnly,
    Semantic,
    RuntimeDeterminism,
    DesktopSmoke,
    PerfClosure,
    AppBundle,
}
```

**Acceptance criteria**

- The RFC and repo docs distinguish the representative authored project from the desktop host app.
- The workspace has a planned crate/app boundary for `apps/traversal_boss_app` instead of reusing `apps/frame_live_app` as the shipping shell.
- Host responsibilities are explicit: input sampling, fixed-step tick drive, audio device lifecycle, swapchain presentation, pause/resume, replay capture/playback, save/load, and graceful device failure.
- The authored project remains the source of truth for world/gameplay content; the host app is not allowed to inject untracked terrain, encounter, material, or audio behavior.
- The project layout has smoke tests or docs checks that prevent the directories and entry files from disappearing.

#### Task 72A1 — Complete the permanent representative traversal-boss project

**Description**

Complete the in-repo project scaffold from Phase 63B2 so it proves the full engine stack.

**Files**

- new `games/traversal_boss/`
- new `games/traversal_boss/README.md`
- new `games/traversal_boss/src/main.wr`
- new `games/traversal_boss/src/world.wr`
- new `games/traversal_boss/src/rider.wr`
- new `games/traversal_boss/src/boss.wr`
- new `games/traversal_boss/src/audio.wr`
- new `games/traversal_boss/src/view.wr`
- new `games/traversal_boss/fixtures/replay/`
- new `games/traversal_boss/fixtures/save/`
- new `games/traversal_boss/scenarios/`
- representative runtime/project config inside the project

**Implementation notes**

This project should include:

- traversal world content
- one rider/board body setup
- at least one boss encounter with terrain mutation
- representative audio hooks
- representative view and perf scenarios

It should be small enough to maintain and rich enough to matter.
The first version may be mechanically simple, but it must exercise the real authored semantics from the previous phases.
It should not be a pile of Rust-side mocks with `.wr` files nearby for decoration.

Minimum content:

- one streamable traversal space with at least two static terrain regions and one transition/replacement region
- one rider/body/moveset path using first-class traversal contact records
- one boss or encounter actor that emits a typed deformation/mutation ticket
- one successful strike-through-movement window and one missed-line recovery fixture
- one audio-control path for wind/carve/landing/boss presence
- one traversal view using the material, shadow, sky, atmosphere, readability, and hero-FX contract vocabulary
- one deterministic replay fixture and one save/checkpoint fixture

**Acceptance criteria**

- The project boots through repo-native commands.
- The project is not a dead sample; it is the canonical representative product proof for this game family.
- The project exercises the language/runtime/audio/deformation/visual stack together.
- Project fixtures prove preview, gameplay replay, save/load, audio-control, deformation, traversal contact, and engine-frame paths against the same authored content.
- Representative project code does not require hidden host callbacks or runtime-only sidecars to express world layout, encounter behavior, material decisions, or audio observations.
- The project README names the supported Mac target, known machine limitations, required proof lanes, and what visual/perf closure means for this representative project.

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
- `just test-traversal`
- `just test-audio`
- `just test-traversal-boss`
- `just perf-game-closure`
- `just bundle-traversal-boss-app`
- `wrela preview games/traversal_boss`
- `wrela replay games/traversal_boss`
- `wrela save-check games/traversal_boss`
- `wrela game-smoke games/traversal_boss`

The exact CLI spelling may change during implementation, but the command surface must remain product-facing and repo-native.
Raw `cargo run --bin ...` commands are acceptable as debugging escape hatches, not as the documented product workflow.

**Acceptance criteria**

- The representative project has clear build, preview, replay, and perf lanes.
- Workflow docs distinguish compile proof, semantic proof, runtime proof, and performance proof.
- `just test` or another documented fast lane includes the representative project at a cheap semantic/runtime smoke level.
- `just ship` includes representative compile, semantic, runtime, replay/save, and desktop smoke evidence, or explicitly documents any omitted expensive lane and the machine/time reason.
- `just perf-game-closure` and `just perf-engine-closure` have a documented relationship: either one delegates to the other for representative scenarios, or the docs explain the separate truth surfaces.
- Product-facing `wrela` commands emit typed reports that include project id, scenario id, source/bundle hashes, replay/save compatibility, backend, and proof surface.
- New lanes are reflected in `AGENTS.md` so future agents do not bypass the representative product proof.

### Workstream B: Final closure and ship evidence

#### Task 72B0 — Define the representative closure manifest and evidence schema

**Description**

Define the final evidence artifact before adding end-to-end scenarios, so completion cannot be claimed from scattered logs.

**Files**

- `benchmarks/README.md`
- `benchmarks/engine_frame/bench.toml`
- `benchmarks/engine_frame/1080p120_closure.toml`
- `compiler/perf_target/mod.rs`
- `compiler/engine_frame/mod.rs`
- new `games/traversal_boss/closure_manifest.toml`

**Implementation notes**

The closure manifest should connect authored project content, gameplay replay fixtures, visual scenarios, audio fixtures, save/checkpoint fixtures, and engine-frame performance targets.

Code sketch:

```rust
pub struct RepresentativeClosureManifest {
    pub project_id: ProjectId,
    pub source_bundle_hash: u128,
    pub scenarios: Vec<RepresentativeScenarioId>,
    pub required_lanes: Vec<ProofLaneId>,
    pub target_hardware: TargetMachineProfile,
    pub frame_budget: FrameBudget,
    pub allowed_waivers: Vec<ClosureWaiver>,
}

pub struct RepresentativeClosureEvidence {
    pub manifest_hash: u128,
    pub lane_results: Vec<ProofLaneResult>,
    pub perf_reports: Vec<EngineFrameReportId>,
    pub replay_reports: Vec<GameplayReplayReportId>,
    pub save_reports: Vec<SaveCompatibilityReportId>,
    pub visual_reports: Vec<LookdevReportId>,
    pub audio_reports: Vec<AudioSubsystemReportId>,
}
```

**Acceptance criteria**

- Final closure evidence is described by a typed manifest/report schema, not by loose markdown claims.
- The manifest records target hardware, frame budget, scenario ids, required lanes, and permitted waivers.
- Evidence can say which scenarios passed, failed, were waived, or were not run because of machine limitations.
- A future maintainer can reproduce the representative closure run from the manifest and repo-native commands.

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
- audio-heavy carving through occluding terrain
- scenic boss reveal with sky/atmosphere/shadow/FX active

Each scenario should name its:

- authored entry point
- gameplay replay fixture
- save/checkpoint fixture when applicable
- active view/quality profile
- expected traversal, collision, presentation, audio, and mutation report labels
- 120-FPS target or documented lower target if the local Mac cannot honestly close the frame

**Acceptance criteria**

- End-to-end scenarios exist and are named in repo artifacts.
- Engine-frame reports can attribute frame cost across gameplay, collision, presentation, and audio for those scenarios.
- The canonical `just perf-engine-closure` lane can discover the representative scenarios without custom one-off manifest filenames, or the RFC explicitly updates lane wiring.
- Regressions fail against the representative project, not only against synthetic benches.
- Scenario reports include replay determinism, save compatibility, audio-control budget, visual quality, mutation invalidation, and engine-frame budget status.
- Perf closure can separate "engine regression" from "scenario content changed" using source/bundle/manifest hashes.

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
- desktop app smoke path
- app bundle/package path
- input device fallback path
- WGPU/device failure diagnostics
- known target-machine limitations and waivers
- machine-limitation disclosure when the target hardware misses closure

**Acceptance criteria**

- The representative project has a maintained operator-facing checklist.
- Repo smoke or documentation checks ensure the project entry points stay present.
- No phase is considered complete unless the representative project still builds and its evidence surface remains truthful.
- The final checklist names the exact `just` and `wrela` commands used for handoff.
- The desktop app can fail gracefully with actionable diagnostics when WGPU, audio device setup, or input device discovery is unavailable.
- The checklist distinguishes "shippable on the target Mac", "engine-correct but visually/perf incomplete", and "not run on this machine".
- Maintenance docs state how future RFCs must update the representative closure manifest when they change gameplay, traversal, audio, visual, or performance contracts.

## Phase 72 exit criteria

- A permanent representative traversal-boss project exists.
- A desktop host app exists for the representative project, distinct from the frame-live inspection app.
- Repo-native lanes cover preview, replay, perf, and ship.
- End-to-end closure is measured against real representative content.
- Final evidence is reproducible from a checked-in closure manifest and typed reports.

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
