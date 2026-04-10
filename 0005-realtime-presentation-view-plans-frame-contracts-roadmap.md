# RFC 0005: Canonical Presentation Plans, View/Frame Contracts, And Real-Time Roadmap

Status: Proposed. Phase 17 is the committed next implementation milestone. Phases 18-23 are directional and should be revalidated after Phase 17, then again after Phase 20.

Author: GPT-5.4 Pro

Created: 2026-04-10

Target: post-Phase-16 `wrela` language, compiler, query-family runtime, CPU/vGPU/WGSL execution, and real-time presentation architecture

## Summary

Phase 16 gave Wrela the right query-language shape:

- the world is already authored as a semantic substrate
- question families and query contracts are now canonical
- CPU, virtual GPU, and WGSL already share a contract-oriented execution story
- `SceneDomain` is internally split into `spatial`, `surface`, and `participants`
- `support.summary` already proves that not every question needs to look like today’s trace/surface helpers

The next architectural move is not “add a faster preview helper.”

It is to turn presentation into a **compiler-owned observer loop**.

That means three things:

1. presentation must become a first-class compiled plan, not a hardcoded `render -> String -> PPM` helper
2. the screen must become a coherent batch/query surface over the same semantic world
3. frame production must carry explicit contracts for outputs, history, quality, and time budget

This roadmap calls that architecture:

- **view contracts** for the observer
- **frame contracts** for the outputs and guarantees
- **presentation plans** for the pass graph that consumes query families and kernels

The central idea is simple:

**real-time rendering is another way of interrogating the world, but unlike scalar queries it is screen-space, temporal, and budgeted.**

That means the next project is not a special renderer that bypasses the query architecture.
It is a presentation system that **compiles down to**:

- screen-lattice generation
- world batch queries
- materialized semantic attachments
- shading/composite passes
- temporal/history passes
- explicit quality ladders and adaptive control

The destination is not merely a prettier preview.
The destination is a field-native presentation pipeline that can realistically grow toward **60 FPS real-time views** while preserving the semantic substrate that the rest of the engine will depend on.

This roadmap should be executed in two horizons:

1. **Committed next milestone: Phase 17.**
   Build the internal presentation contract/plan bridge and keep authored `render` plus preview output stable.

2. **Directional roadmap: Phases 18-23.**
   Use these phases as the intended architecture, but revisit scope after the Phase 17 bridge exists and again after the first explicit color path exists.

The immediate win is not "real-time" yet.
It is making presentation a canonical compiler-owned plan without disrupting the current preview surface.

## Relationship To Earlier RFCs And Repo Vision

This roadmap builds directly on:

- `language/spec/rfcs/0001-field-game-language.md`
- `language/spec/rfcs/0002-field-engine-implementation-roadmap.md`
- `language/spec/rfcs/0003-phase-9-5-semantic-convergence-plan.md`
- the shipped Phase 10 WGSL work
- the shipped Phase 11–16 query-family and query-contract work
- `AGENTS.md`

`AGENTS.md` gives the right long-term north star:

- Wrela is becoming a field-native game engine
- the world is authored once and interrogated many ways
- rendering is one question among many
- CPU remains a trusted oracle
- GPU backends are execution targets, not authored truth
- preserving the semantic substrate matters more than local shortcuts

This roadmap applies that vision to presentation.

The important rule from the existing repo still holds:

**presentation remains a consumer of query families and portable kernels.**

It does not become a competing execution model.

That point is the difference between “a semantic engine with a renderer” and “a renderer that happens to call some field helpers.”

## Current Repo Read

The current repo state is strong enough that real-time presentation now makes sense, but it also shows exactly what must change.

### What is already strong

1. `compiler/query_contract/mod.rs` is now the canonical source of query identity, family membership, result schemas, backend support, and execution bindings.
2. `compiler/query_plan/mod.rs` and `compiler/kernel/*` already treat questions as portable, contract-bearing execution plans.
3. `compiler/query_exec/wgsl/codegen.rs` already knows how to execute batch work through storage-buffer-style WGSL kernels.
4. `compiler/portable.rs` and `compiler/portable/abi.rs` already own stable ABI layouts for the world/query data that presentation will need to reuse.
5. `Camera`, `Light`, `RayQuery`, `PointQuery`, `PointDirectionQuery`, `Hit3`, `Surface`, `Medium`, and the family domain contracts already exist as portable records.
6. The CPU path already serves as a practical oracle for per-query parity, preview sampling, and plan diagnostics.

### What is currently holding presentation back

1. **`render` is still a compiler-owned PPM helper path, not a view/frame plan.**
   `compiler/mir/lower.rs` still lowers render declarations into `__wr_render_capture_to_ppm` and `__wr_render_scene_color_capture`.

2. **`RenderMetadata` is still a bag of mixed concerns.**
   It currently mixes:
   - domain selection
   - lighting
   - viewport sizing
   - projection compatibility fields
   - preview-only composition hints

3. **The canonical camera projection is not actually canonical yet.**
   `Camera.vertical_fov_degrees` exists in the portable type system, but the current preview helper computes rays from `view_scale` instead of treating FOV as authoritative.

4. **Render budgets are still coupled to legacy authored-domain metadata.**
   `lower_render_trace_budget_values` still scrapes max distance / min step / epsilon / max steps out of domain call bodies. That is practical as compatibility glue, but it is the wrong long-term boundary.

5. **There is no world-batch surface in the query registry yet.**
   Current batch support is capture-oriented. Real-time presentation needs world-batch execution over the screen lattice.

6. **There is no semantic attachment model.**
   The engine has query result records, but not typed frame attachments, lifetime rules, or history slots.

7. **There is no temporal contract yet.**
   No canonical view state, no previous-view data, no motion contract, no reprojection contract, and no history compatibility model.

8. **There is no quality contract that owns the 60 FPS problem.**
   Right now the repo has query-level observability and benchmark scaffolding, but not a frame-level latency/quality control loop.

### What the code says about the next move

The repo is already telling us the right answer.

The existing query-family work made the semantic substrate strong.
The existing WGSL batch machinery made storage-buffer execution real.
The remaining weakness is that presentation still lives in a **legacy helper island**.

So the next project is not “make render declarations more powerful.”

It is:

- canonicalize view/frame semantics
- extend query surfaces to world-batch screen work
- materialize semantic attachments
- lower presentation to an explicit pass graph
- add time/history/quality as first-class contracts
- then make the authored surface reflect that model once it is real

## Why This Comes Before Broader Gameplay Families

The query-family project made the engine capable of answering disciplined world questions.
The next thing that will make the project feel undeniably real is not yet another abstract family.
It is a camera moving through the world at interactive rates.

That matters for three reasons.

First, a real-time view exercises almost every important promise in the engine at once:

- batched world queries
- CPU/GPU parity
- semantic planning
- support/culling reuse
- portable ABI discipline
- observability
- determinism under motion

Second, presentation is where the repo’s “queryable substrate” vision becomes intuitive.
When one authored world can feed scalar probes, screen-space visibility, material resolution, media sampling, and temporal reuse through one planner, the architecture stops being theoretical.

Third, getting to 60 FPS requires the right abstractions early.
If the repo keeps extending the PPM helper path, it will accumulate exactly the kind of non-canonical execution story that `AGENTS.md` warns against.

## Performance Thesis: Why Wrela Can Be Fast

Wrela's performance advantage should come from the compiler understanding authored world meaning.

The goal is not to take a naive field renderer and run it through a cleaner pass graph.
That would still mean:

- every pixel marches independently
- every step considers too much world structure
- every backend rediscovers the same facts at runtime
- the renderer treats fields as opaque functions instead of semantic objects

The compiler-owned presentation pipeline should instead ask:

**For this view, this domain, this query family, this quality contract, and this screen region, what is the cheapest semantically valid way to answer the required world questions?**

That is the engine's unique advantage.
The query-family substrate gives the compiler durable information that a generic renderer does not have:

- world identity and provenance
- spatial, surface, participant, and support-family contracts
- domain detail and feature policy
- support summaries and future conservative bounds
- repetition and authored structure
- stable CPU oracle behavior
- backend support and cost profiles

Real-time viability depends on using that information to **avoid work**, not merely to schedule work.
`WorldBatch` is only the substrate.
Compiler-derived semantic acceleration is what should make the substrate fast.

At 1080p, a full-resolution one-sample frame contains about 2.07 million primary samples.
That is roughly:

- 124 million primary samples per second at 60 FPS
- 249 million primary samples per second at 120 FPS

Those targets are plausible only if the compiler aggressively reduces candidate work, specializes kernels, reuses temporal history, and permits legal quality trade-offs.
This roadmap should therefore treat native 1080p60 and 1080p120 as benchmark targets for representative scenes, not as blanket guarantees for arbitrary authored worlds.

## Query Engine Scope: From Primitives To Programs

Presentation should not be treated as a separate renderer that merely calls the query engine.
It should be treated as the first large **query program** built on top of the current query substrate.

The query engine should be understood as the compiler/runtime layer that serves disciplined, contract-bearing questions about the world.
Today its vocabulary is mostly low-level because that is where the engine had to start:

- point, ray, and hit-shaped items
- distance, normal, nearest, occlusion, surface, radiance, medium, and support-summary questions
- scalar, batch, capture, and world surfaces

That current vocabulary should not define the ceiling of the query engine.
Over time, the query engine should grow from low-level query primitives into a substrate that can also support larger domain-scale query programs.

For this roadmap, keep `PresentationPlan` in the presentation layer.
View contracts, frame contracts, screen lattices, color attachments, history, quality policy, and compatibility export behavior are presentation-specific enough to stay concrete.
Do not invent a generic `QueryProgram` IR before a second serious observer proves the overlap.

The intended evolution is:

1. Build presentation as a concrete domain observer and query program.
2. Later build a narrow collision/traversal observer as another concrete query program.
3. Compare the real overlap: pass graphs, query primitive invocations, materialized intermediates, acceleration artifacts, identity/provenance, cost reports, backend dispatch, and CPU oracle checks.
4. Promote only the repeated machinery into a shared query-program layer inside the query engine.

This keeps the architecture ambitious without prematurely generalizing from one domain.

## Goals

This roadmap has nine goals.

1. **Turn presentation into a canonical compiler-owned plan.**
   A frame must lower into an explicit pass graph with typed contracts.

2. **Keep presentation downstream of query families.**
   Presentation reuses `spatial`, `surface`, `participants`, and `support`; it does not replace them.

3. **Add a real screen-space substrate.**
   The screen must be represented as a batchable lattice of samples/views rather than ad hoc nested loops in MIR helpers.

4. **Treat presentation as the first concrete query program.**
   `PresentationPlan` should live in the presentation layer now, while exposing which pass-graph, artifact, observability, and dispatch patterns might later become shared query-program machinery.

5. **Make compiler-derived semantic acceleration central.**
   Presentation must use authored structure, support summaries, conservative bounds, domain policy, and query contracts to reduce work before backend-specific tuning.

6. **Introduce semantic frame attachments and history contracts.**
   The engine needs typed outputs, lifetimes, and temporal reuse semantics.

7. **Make CPU the oracle for frame semantics too.**
   Every presentation pass must have a CPU truth path before backend-specific tuning becomes authoritative.

8. **Make 60 FPS an explicit contract problem.**
   Quality, degradation, and adaptation should be owned by typed frame contracts and metrics, not by scattered helper constants.

9. **Land the author-facing cut once.**
   The internal architecture should converge before users are asked to migrate to new `view`/frame syntax.

## Explicit Non-Goals

This roadmap does **not** attempt to do the following.

- full GI or path tracing
- reflections/refractions as a first milestone
- a hybrid raster + field renderer
- clustered many-light infrastructure
- material graph redesign beyond what current `Surface` semantics need
- swapchain/window/input/platform integration beyond thin host boundaries
- editor viewport UX
- VR / stereo / foveated rendering
- post-stack breadth (bloom, tone mapping suites, color grading, etc.) beyond the minimum needed to validate the architecture
- replacing the world/query model with a presentation-specific DSL
- guaranteeing native 1080p60 or 1080p120 for every arbitrary authored world in this roadmap

Those may come later. This roadmap exists to build the semantic real-time presentation core.

## Design Rules

Every phase in this RFC must follow these rules.

1. **Presentation remains a consumer of query families and portable kernels.**
   Do not build a second renderer architecture that bypasses the existing semantic substrate.

2. **Keep semantic contracts distinct from execution bindings and physical packing.**
   Logical attachment meaning and view/frame guarantees must not be conflated with helper names, kernel symbols, texture/storage choices, or compact packing schemes.

3. **Preserve the current authored surface until the internal model is real.**
   `render` stays stable until the final view/frame surface cut.

4. **Treat `Camera.vertical_fov_degrees` as the canonical projection input going forward.**
   `view_scale` and related fields become legacy compatibility only.

5. **Keep domain policy separate from frame quality.**
   `SceneDomain` answers “what world detail/features are allowed.”
   Frame quality contracts answer “what can the observer trade to hit a budget.”

6. **Use buffers before textures.**
   The first semantic attachment implementation should lower to deterministic linear/storage-buffer layouts that the CPU oracle and WGSL both understand. Texture-specialized lowering can be an optimization layer later.

7. **CPU oracle first.**
   Every new presentation pass lands on CPU before WGSL becomes authoritative.

8. **Measure the frame, not just the query.**
   Add frame-level observability, pass-level cost accounting, and compatibility reports.

9. **Land the first real-time slice narrowly.**
   Primary visibility, one-key-light shading, temporal reuse, and explicit quality ladders are enough. Do not widen into full engine rendering features before the core is solid.

10. **Keep Phase 17 deliberately thin.**
    Phase 17 should establish contracts, plan structure, compatibility lowering, tooling, and golden-preview stability. It should not deeply implement temporal policy, adaptive quality, physical packing, or real-time optimization before real passes exercise those concepts.

11. **Specify screen/projection conventions before execution depends on them.**
    Pixel-center convention, normalized-device-coordinate mapping, y-axis direction, jitter units, aspect handling, FOV behavior, and depth semantics are part of the semantic contract, not backend folklore.

12. **Primary frame attachments must preserve stable semantic identity.**
    A primary-hit attachment must carry enough world identity for later surface, participant, tooling, and history queries to resolve meaning without reinterpreting pixels as anonymous depth samples.

13. **Every phase inherits the `AGENTS.md` completion gate.**
    A phase is not complete until its acceptance criteria are met, appropriate end-to-end gates pass, and an independent review has been performed.

14. **Do less work before doing work faster.**
    A world-batch implementation that simply evaluates every relevant field/shape for every step of every pixel is only a compatibility baseline. The real presentation path must derive conservative acceleration artifacts from authored semantics, support summaries, bounds, repetition structure, domain policy, and query contracts.

15. **Measure cost shape as soon as screen work exists.**
    Do not wait for the final quality controller to observe performance. Beginning in Phase 18, collect candidate counts, ray-step counts, pruning rates, hit/miss rates, dispatch sizes, and backend timing where available.

16. **Start concrete, then promote shared query-program machinery.**
    Keep `PresentationPlan` presentation-owned in this roadmap. If later collision, traversal, audio, AI, or tooling observers duplicate the same pass graph, artifact, materialization, observability, or backend-dispatch structure, promote that repeated machinery into a shared query-program layer in the query engine.

## Key Architectural Definitions

### Query Primitive

A **query primitive** is a low-level, contract-bearing world question.

Examples:

- `spatial.distance`
- `spatial.nearest`
- `spatial.occluded`
- `surface.sample`
- `participants.radiance`
- `participants.medium`
- `support.summary`

Query primitives are the vocabulary the current query engine already knows how to contract, plan, execute, test, and report.
They remain the bedrock underneath presentation and future domains.

### Query Program

A **query program** is a compiler-owned orchestration of query primitives, intermediate results, derived artifacts, backend dispatch choices, and observability that answers a larger structured world question.

Examples:

- produce a frame for a view
- solve a collision/traversal step
- evaluate grounding or clearance
- update an audibility or AI-perception observer

In this roadmap, `PresentationPlan` is the first concrete query program.
It stays in the presentation layer because frame/view semantics are domain-specific.
Only machinery that repeats across multiple real observers should be promoted into a generic query-program layer.

### View Contract

A **view contract** describes the observer.

It answers questions like:

- what camera and projection are active?
- what viewport or internal resolution is used?
- what temporal state exists?
- what compatibility projection rules are still in play?

A view contract is about the **observer** and the **screen-space coordinate system**, not about what attachments are produced.

### Frame Contract

A **frame contract** describes what a frame must produce.

It answers questions like:

- what attachments exist?
- which ones are exported?
- which ones are history-backed?
- what quality guarantees or fallbacks are allowed?
- what temporal reuse policy is valid?

### Semantic Attachment

A **semantic attachment** is a materialized per-sample view of some semantic result.

Examples:

- primary hit
- depth
- world normal
- surface sample
- radiance sample
- medium sample
- motion vector
- final color

This is deliberately like a **materialized view** in database systems.

The important distinction is:

- the **semantic attachment contract** says what the attachment means
- the **physical attachment layout** says how it is packed or stored on a backend

Those must stay separate.

For primary visibility, the semantic attachment must also preserve stable world identity.
At minimum it must carry the hit/miss state plus the identity/provenance needed by the current query contracts to resolve surface, participants, debug inspection, and history compatibility later.
Depth or normal alone is not sufficient.

### Primary Hit Attachment Contract

The **primary hit attachment contract** is the authoritative schema for visibility results produced from a screen lattice.

It should be named before `PrimaryVisibilityPass` implementation begins.
The first version should either wrap `Hit3` directly or preserve equivalent fields:

- hit/miss state
- distance and world position
- world normal
- local position and local normal
- shading frame if required by surface/participant resolve
- step count for observability
- feature, instance, repeat, and root-shape identity
- payload/provenance needed by downstream query contracts

The attachment may rely on the screen-lattice index for pixel/sample identity, but that indexing rule must be explicit.
Depth, normal, and color attachments are derived views over this semantic hit data, not substitutes for it.

### Screen Lattice

The **screen lattice** is the batchable set of pixel/sample coordinates for a view.

It is the presentation equivalent of a query item array.
It exists so that the planner can reason about coherent screen work instead of emitting one-off scalar trace calls.

The lattice contract must define:

- pixel-center convention
- sample offset and jitter units
- normalized coordinate mapping
- y-axis direction
- aspect-ratio handling
- depth/ray parameter semantics

These conventions must be shared by CPU, virtual GPU, WGSL, and compatibility projection paths.

### Presentation Plan

A **presentation plan** is the compiler-owned pass graph for producing a frame.

It is the first concrete query program in this roadmap.
It is presentation-owned because view/frame semantics are domain-specific, but it should make generic query-program-shaped pieces visible for later comparison with collision, traversal, audio, AI, and tooling observers.

It is the frame-oriented equivalent of the query plan:

- semantic attachments
- pass ordering
- world batch queries
- derived frame artifacts
- temporal/history dependencies
- export surfaces
- observability

### Semantic Acceleration Artifact

A **semantic acceleration artifact** is a compiler-derived structure that reduces presentation work while preserving the meaning of the query contract.

Examples include:

- support-derived tile candidate tables
- conservative per-region bounds
- repetition-aware candidate summaries
- hit work lists
- compacted resolve queues
- domain-specific detail filters

The important rule is that these artifacts are derived from the semantic world model and have explicit correctness guarantees.
They are not anonymous backend heuristics.
If an artifact is conservative, approximate, lossy, or backend-specific, that policy must be named and tested.

### Temporal Contract

A **temporal contract** states what history can be reused and under what conditions.

It includes things like:

- required previous view state
- history slot compatibility
- reprojection expectations
- disocclusion handling
- invalidation rules

### Quality Contract

A **quality contract** states the allowable trade-offs for a view/frame.

It should own things like:

- target FPS / frame budget
- internal resolution scaling
- step-count ceilings
- history allowed or not
- which expensive passes can degrade or disable
- whether dynamic resolution is permitted

### Lighting Contract

A **lighting contract** describes the presentation-time lighting inputs used by the current frame pipeline.

For this roadmap the first slice is intentionally narrow:

- one key light
- one fill contribution
- simple ambient/environment behavior

The point is to move lighting metadata out of ad hoc render fields and into a typed plan input.

### Compatibility Projection

A **compatibility projection** is any legacy projection behavior preserved so existing authored previews continue to work while the new view model lands.

For this roadmap:

- `Camera.vertical_fov_degrees` becomes canonical
- `view_scale` and `world_up` remain compatibility lowering details until the authored surface cut

## Target First Real-Time Slice

To keep the roadmap honest, the first shippable real-time slice should be deliberately narrow.

This is not the first committed implementation milestone.
The first committed milestone is Phase 17: canonical internal plans over the current preview behavior.
The real-time slice below becomes the target once Phase 17 has landed and been reviewed.

The intended first slice is:

- one world capture
- one camera/view
- one key light plus fill/ambient compatibility
- primary visibility
- compiler-derived candidate pruning for at least one representative scene class
- optional surface/radiance/media resolve depending on quality
- typed color/depth/normal/motion outputs
- temporal reuse for color
- CPU oracle and WGSL parity on reduced-size frames
- quality tiers that can reasonably chase a 60 FPS target on representative scenes
- benchmark ladders that report 540p/720p cost shape and 1080p60/1080p120 target-scene attempts

It is **not**:

- GI
- reflections
- post-stack sprawl
- many-light systems
- platform-specific rendering integrations
- a blanket native-1080p performance guarantee for arbitrary worlds

That narrow slice is enough to prove the architecture.

The first performance target should be phrased as:

**make 60 FPS measurable, explainable, and controllable on representative scenes.**

The stretch target should be:

**identify which scene classes and quality modes can plausibly reach 1080p60 or 1080p120, and which compiler-derived artifacts are needed for the rest.**

## Phase Overview

This roadmap has seven phases.

1. **Phase 17 — Canonical View/Frame Contracts And Internal Presentation Plans**
2. **Phase 18 — World-Batch Query Surfaces And Screen-Lattice Substrate**
3. **Phase 19 — Primary Visibility And Semantic Frame Attachments**
4. **Phase 20 — Lighting, Surface/Participant Resolve, And First Real-Time Color Path**
5. **Phase 21 — Temporal Contracts, Motion, And History-Aware Resolve**
6. **Phase 22 — 60 FPS Quality Ladders, Adaptive Control, And Presentation Acceleration**
7. **Phase 23 — Authoritative View/Frame Surface And Tooling**

The dependency structure is important:

- Phase 17 makes presentation internally canonical without changing behavior.
- Phase 18 gives the planner the world-batch and screen-space substrate it needs.
- Phase 19 produces the first real semantic frame attachments.
- Phase 20 turns those attachments into a real color path.
- Phase 21 adds temporal continuity.
- Phase 22 makes the frame budget explicit and pursues 60 FPS systematically, but may split into quality/control and acceleration projects after Phase 20.
- Phase 23 is the user-facing cut after the architecture is real.

Execution horizon:

- Treat **Phase 17** as the next actionable project.
- Treat **Phases 18-23** as the intended direction, not an irreversible commitment.
- Re-open this roadmap after Phase 17 to adjust world-batch and screen-lattice details against the actual presentation-plan bridge.
- Re-open it again after Phase 20 to decide how much temporal, quality-control, and acceleration work should land in this RFC versus a follow-up RFC.

## Phase 17: Canonical View/Frame Contracts And Internal Presentation Plans

### Goal

Turn presentation into an explicit internal contract/plan system while keeping the current authored `render` surface and preview output stable.

This is the concrete next step to take.
Its job is to turn the existing helper-centered path into a plan-centered path without promising real-time execution yet.

### Why this is first

The repo currently has all the ingredients for real-time presentation except one: presentation still lives in a legacy helper lane.

This phase fixes that without asking users to rewrite anything yet.

Phase 17 should be small enough to review as a low-regret architectural bridge:

- contracts and pass-graph shape exist
- current preview behavior is preserved
- compatibility projection is explicit
- tooling can show the resulting plan
- no detailed temporal, quality, packing, or optimization policy is invented ahead of use

Treat the resulting `PresentationPlan` as the first domain-specific query program.
Do not extract a generic query-program layer during Phase 17.
Instead, make candidate shared pieces visible in naming, reports, and code boundaries so a later collision/traversal observer can test whether the abstraction is real.

### Workstream A: Semantic Contract Model

#### Task 17A1 — Add semantic presentation contracts and separate execution bindings

**Description**

Create a new module that owns the semantic contracts for view/frame presentation, plus a separate module for execution bindings that accompany them.

**Files**

- new `compiler/presentation_contract/mod.rs`
- new `compiler/presentation_binding/mod.rs`
- `compiler/lib.rs`

**Implementation notes**

Define the semantic model up front.

Recommended starting semantic types:

- `FrameAttachmentKind`
- `AttachmentLifetime`
- `AttachmentResolutionClass`
- `TemporalReuseMode`
- `LightingContract`
- `ViewContract`
- `FrameContract`
- `PresentationObservabilityProfile`

Recommended accompanying binding type in `compiler/presentation_binding/mod.rs`:

- `PresentationExecutionBinding`

Keep semantic contracts separate from execution bindings.
Do this as a module boundary on day one, not just as a comment convention.
`presentation_contract` should describe what a view/frame/attachment means.
`presentation_binding` should describe which pass recipe, helper, kernel, bridge, or backend binding can execute that meaning.

For Phase 17, keep the model shallow.
`TemporalReuseMode`, attachment lifetimes, resolution classes, and observability profiles may exist as typed placeholders or minimal enums, but detailed temporal policy, quality ladders, history compatibility keys, and physical packing belong to later phases after real passes need them.

Do **not** let the contract types carry:

- WGSL helper names
- bridge export names
- physical texture/storage format choices
- current compact packing choices

Those belong in `presentation_binding` or later physical layout adapters.

**Code sketch**

```rust
pub enum FrameAttachmentKind {
    PrimaryHit,
    Depth,
    WorldNormal,
    Surface,
    Radiance,
    Medium,
    Motion,
    Color,
}

pub enum AttachmentLifetime {
    Transient,
    Exported,
    HistorySlot(u8),
}

pub struct ViewContract {
    pub canonical_projection: bool,
    pub allows_legacy_projection_override: bool,
}

pub struct FrameContract {
    pub outputs: Vec<FrameAttachmentContract>,
    pub temporal: Option<TemporalContract>,
    pub lighting: LightingContract,
}
```

Binding module sketch:

```rust
pub struct PresentationExecutionBinding {
    pub pass_kind: PresentationPassKind,
    pub recipe: PresentationPassRecipeKind,
    pub default_backend: DispatchBackend,
}
```

**Acceptance criteria**

- The semantic contract module exists.
- The execution binding module exists separately from semantic contract types.
- Logical attachment meaning is separate from execution binding.
- The model can describe exported attachments, transient attachments, and history-backed attachments.
- No helper names or backend-specific packing details exist on the semantic contract structs.
- Phase 17 contracts are intentionally minimal and do not pre-implement detailed temporal, quality, or physical-packing policy.

#### Task 17A2 — Add portable view/frame records

**Description**

Add the portable records that later passes and helpers will need.

**Files**

- `compiler/portable.rs`
- `compiler/portable/abi.rs`
- `compiler/tests/portable_abi.rs`
- `compiler/tests/pir.rs`

**Implementation notes**

Add these records first:

- `Viewport`
- `ViewState`
- `FrameState`

Recommended field split:

`Viewport`
- `width: U32`
- `height: U32`

`ViewState`
- `camera: Camera`
- `previous_camera: Camera`
- `viewport: Viewport`
- `jitter: Vec2`

`FrameState`
- `view: ViewState`
- `frame_index: U32`
- `delta_seconds: F32`

Keep them constructible and portable.

Do **not** add textures or swapchain objects here.
The first implementation should remain CPU/WGSL-shared through stable record ABIs.

**Code sketch**

```rust
const VIEWPORT_FIELDS: &[PortableBuiltinField] = &[
    PortableBuiltinField { name: "width", ty: TyAtom(Atom::U32) },
    PortableBuiltinField { name: "height", ty: TyAtom(Atom::U32) },
];

const VIEW_STATE_FIELDS: &[PortableBuiltinField] = &[
    PortableBuiltinField { name: "camera", ty: TyNamed("Camera") },
    PortableBuiltinField { name: "previous_camera", ty: TyNamed("Camera") },
    PortableBuiltinField { name: "viewport", ty: TyNamed("Viewport") },
    PortableBuiltinField { name: "jitter", ty: TyAtom(Atom::Vec2) },
];
```

**Acceptance criteria**

- `Viewport`, `ViewState`, and `FrameState` exist as portable records.
- ABI snapshots cover them.
- The records are usable from MIR/PIR and future kernel lowering.
- No backend-specific rendering handles leak into the portable type system.

#### Task 17A3 — Canonicalize current render metadata internally

**Description**

Reshape the internal render metadata so view, frame, lighting, and compatibility projection are distinct concepts.

**Files**

- `compiler/hir/def.rs`
- `compiler/hir/lower.rs`
- `compiler/hir/semantic.rs`
- `compiler/hir/typeck/types.rs`
- `compiler/parser/validate.rs`

**Implementation notes**

Today `RenderMetadata` mixes concerns.
Split it internally into at least these conceptual buckets:

- view/projection metadata
- frame/export metadata
- lighting metadata
- compatibility projection metadata

Keep the authored `render` body syntax stable for now.

Important rules:

- `Camera.vertical_fov_degrees` becomes the canonical projection input.
- `view_scale` becomes compatibility-only.
- `world_up` becomes compatibility-only.
- `light` and `fill_dir` should clearly belong to a lighting contract.

Do not remove existing authored fields in this phase.
The repo should still parse and run the current preview projects unchanged.

**Acceptance criteria**

- Internal metadata distinguishes view/frame/lighting concerns.
- Current preview projects continue to type-check and lower.
- Canonical projection now has a place to live that does not depend on `view_scale`.
- Compatibility projection fields are explicitly marked as such in code comments and diagnostics.

### Workstream B: Internal Presentation Plan

#### Task 17B1 — Add `compiler/presentation_plan/mod.rs`

**Description**

Create a canonical internal pass graph for presentation.

**Files**

- new `compiler/presentation_plan/mod.rs`
- optionally new `compiler/presentation_plan/validate.rs`
- `compiler/presentation_binding/mod.rs`
- `compiler/lib.rs`

**Implementation notes**

The plan should be close in spirit to `query_plan`, but not identical.
It needs to represent passes, attachments, and exports.

Recommended initial types:

- `PresentationPlan`
- `PresentationPass`
- `PresentationPassKind`
- `FrameAttachmentContract`
- `FrameArtifactContract`
- `PresentationObservability`

Recommended initial pass kinds:

- `LegacyPpmExport`
- `GenerateScreenSamples`
- `WorldBatchQuery`
- `KernelDispatch`
- `ExportAttachment`

The key design point:

- the presentation plan owns logical pass structure
- execution binding owns helper/kernel/backend details
- `presentation_plan` may reference binding ids or binding summaries, but it must not become the owner of backend helper names or physical packing choices
- pass graph, materialization, artifact, observability, and dispatch concepts should be clear enough to compare later against collision/traversal observer plans, but they should not be generalized prematurely

**Code sketch**

```rust
pub struct PresentationPlan {
    pub name: SmolStr,
    pub view: ViewContract,
    pub frame: FrameContract,
    pub passes: Vec<PresentationPass>,
}

pub enum PresentationPassKind {
    LegacyPpmExport,
    GenerateScreenSamples,
    WorldBatchQuery { contract_id: QueryContractId },
    KernelDispatch,
    ExportAttachment { attachment: SmolStr },
}
```

**Acceptance criteria**

- A canonical `PresentationPlan` type exists.
- The plan can represent at least the current preview as a degenerate one-view one-export pipeline.
- Semantic pass structure is separate from execution binding.
- Validation hooks exist or have a clear placeholder.

#### Task 17B2 — Lower current `render` declarations into `PresentationPlan`

**Description**

Route existing render lowering through a presentation plan, but preserve current behavior.

**Files**

- `compiler/mir/lower.rs`
- `compiler/hir/lower.rs`
- new or existing presentation-plan lowering helpers
- `compiler/tests/preview_project.rs`
- `compiler/tests/codegen_v2.rs`

**Implementation notes**

The current `render` path should become:

`render decl -> canonical presentation plan -> legacy PPM binding`

That means `__wr_render_capture_to_ppm` is still allowed to exist, but it should become an execution binding detail rather than the conceptual center of presentation.

Keep the current output stable.
Prefer bit-for-bit stability for existing preview snapshots where possible; where floating-point or backend tolerance already applies, preserve the existing tolerance envelope and document it in the test.

Do not widen the authored surface in this phase.

**Acceptance criteria**

- Existing preview projects still run unchanged.
- Lowering goes through `PresentationPlan` internally.
- `__wr_render_capture_to_ppm` is now a compatibility implementation detail.
- No user-facing syntax changes are required.
- Preview golden output is unchanged or remains within the existing documented CPU/WGSL tolerance.

### Workstream C: Tooling And Introspection

#### Task 17C1 — Add a CLI presentation-plan dump

**Description**

Add a command that prints or emits JSON for the compiled presentation plan.

**Files**

- `compiler/bin/wrela/cli_args.rs`
- `compiler/bin/wrela/commands/shared.rs`
- `compiler/bin/wrela/diag_emit.rs`
- tests in `compiler/tests/cli.rs`

**Implementation notes**

Recommended command shapes:

- `cargo run -p wrela -- presentation-plan path/to/project`
- `cargo run -p wrela -- presentation-plan path/to/project --json`

Show at least:

- view contract summary
- frame outputs
- compatibility projection use
- pass list
- execution backends / bindings
- pass-level query-family dependencies
- obvious future acceleration hook points, even if no acceleration artifact exists yet
- candidate query-program-shaped concepts such as pass graph, materialized intermediates, derived artifacts, backend dispatch, and observability

This will make the rest of the roadmap much easier to debug.

**Acceptance criteria**

- The new CLI command exists.
- Human-readable and JSON forms work.
- The dump clearly shows whether a render is still using compatibility projection fields.
- The dump makes it clear which query families, contracts, and future acceleration hook points a presentation plan depends on.
- The dump makes presentation-specific concepts distinguishable from candidate generic query-program machinery.

### Phase 17 Exit Criteria

- Presentation has a canonical internal contract/plan model.
- Existing authored `render` programs still compile and run.
- Preview output remains stable.
- The repo can inspect compiled presentation plans from the CLI.
- This phase has been reviewed before Phase 18 scope is treated as final.

### Phase 17 Revalidation Questions

Before starting Phase 18, answer these questions from the implemented plan model and CLI dump:

1. Does `PresentationPlan` naturally want `WorldBatch`, or should the query contract model split target and cardinality before more surfaces are added?
   Make this decision before any non-spatial world-batch family lands.
   If `WorldBatch` creates repeated special cases in plan construction, kernel validation, ABI/reporting, or execution dispatch, split target/cardinality first instead of letting the compatibility shape harden.
2. Which view/projection fields are canonical, and which are compatibility-only?
3. Which query families and contracts does the current preview path actually depend on?
4. Where should semantic acceleration artifacts attach: view contract, frame contract, presentation plan, query plan, or execution binding?
5. What exact `PrimaryHitAttachmentContract` schema must downstream resolve consume?
6. Which cost metrics can be collected immediately in Phase 18 without designing the full Phase 22 controller?
7. Which legacy PPM helper behavior is merely export plumbing, and which behavior still encodes semantic rendering choices?
8. Which pieces of `PresentationPlan` look presentation-specific, and which look like candidate generic query-program machinery to compare against a future collision/traversal observer?

The point of this revisit is to design Phase 18 around the first real compiler-owned presentation object instead of around a guessed shape.

## Phase 18: World-Batch Query Surfaces And Screen-Lattice Substrate

### Goal

Give presentation a real screen-space batch substrate by adding world-batch query surfaces and canonical screen sample generation.

### Why this is next

A real-time renderer cannot be built out of scalar world queries wrapped in nested loops forever.
The planner needs a world-batch surface and a screen lattice it can reason about.

However, world-batch alone is not the performance win.
The first implementation may include a straightforward dense baseline for parity, but the contract and observability work must leave room for semantic acceleration:

- candidate counts per sample/tile
- support-pruning decisions
- ray-step distributions
- domain-feature enablement
- backend dispatch sizes
- hit/miss density

If Phase 18 only produces a larger batch API with no way to see or reduce work, it has not set up the 60 FPS path.

### Workstream A: Query-Contract Surface Expansion

#### Task 18A1 — Add `WorldBatch` to the query contract model and plan/kernel layers

**Description**

Extend the query-family system so batch work can target world captures, not just field/shape captures.

**Files**

- `compiler/query_contract/mod.rs`
- `compiler/query_plan/mod.rs`
- `compiler/kernel/ir.rs`
- `compiler/kernel/lower.rs`
- `compiler/kernel/validate.rs`
- `compiler/query_exec/spec.rs`

**Implementation notes**

Add a new `QuerySurfaceKind` member:

- `WorldBatch`

If the edit stays small, consider splitting the underlying model into separate axes:

- query target: `Capture` or `World`
- query cardinality: `Scalar` or `Batch`

That split is cleaner than growing a combined enum forever.
However, do not force a large cross-cutting refactor just to land the presentation substrate.
If `WorldBatch` is added as the pragmatic step, document that `QuerySurfaceKind` is a compatibility shape combining target and cardinality.
This decision must be made before `surface.sample.batch.world`, `participants.radiance.batch.world`, or `participants.medium.batch.world` land.

Do **not** create a totally separate `WorldBatchQueryPlan` if `BatchQueryPlan` can be extended cleanly.

`BatchQueryPlan` already carries:

- `surface`
- `capture_kind`
- `item_kind`
- `result_kind`

Use that.
Avoid unnecessary type explosion.

**Acceptance criteria**

- `WorldBatch` exists in the contract model.
- The existing plan/kernel structures can represent world-batch work.
- CLI/query-contract reporting shows the new surface cleanly.
- The implementation either splits target/cardinality cleanly or documents why `WorldBatch` remains the conservative compatibility-shaped extension.
- No non-spatial world-batch family lands until the combined-enum versus orthogonal-axis decision is recorded.

#### Task 18A2 — Seed world-batch contracts for the spatial family

**Description**

Add the first world-batch descriptors and bindings for the spatial family.

**Files**

- `compiler/query_contract/mod.rs`
- tests in `compiler/tests/query_contract_registry.rs`
- tests in `compiler/tests/cli.rs`

**Implementation notes**

Add canonical descriptors/bindings for:

- `spatial.distance.batch.world`
- `spatial.normal.batch.world`
- `spatial.nearest.batch.world`
- `spatial.occluded.batch.world`

Keep `nearest`/`trace` compatibility behavior consistent with the current scalar world contracts.

**Acceptance criteria**

- The spatial family has canonical world-batch descriptors.
- Registry dumps and compatibility aliases are correct.
- Backend support is explicit per descriptor.

#### Task 18A3 — Seed world-batch contracts for `surface` and `participants`

**Description**

Add the world-batch descriptors and bindings needed for real frame shading.

**Files**

- `compiler/query_contract/mod.rs`
- tests in `compiler/tests/query_contract_registry.rs`
- tests in `compiler/tests/cli.rs`

**Implementation notes**

Add canonical descriptors/bindings for:

- `surface.sample.batch.world`
- `participants.radiance.batch.world`
- `participants.medium.batch.world`

Carry domain requirements and backend support explicitly.

These are essential because presentation should resolve surface/radiance/media in coherent passes, not through per-pixel scalar helpers.

**Acceptance criteria**

- Surface and participants families have world-batch descriptors.
- Domain requirements and backend support are explicit.
- The registry becomes sufficient for a full primary-hit -> resolve -> shade presentation pipeline.

### Workstream B: Screen-Lattice Records And Projection Helpers

#### Task 18B1 — Add `ScreenSampleQuery` and canonical view-to-ray helpers

**Description**

Introduce a first-class screen-sample record and pure helpers that turn view state into rays.

**Files**

- `compiler/portable.rs`
- `compiler/portable/abi.rs`
- `compiler/mir/lower.rs`
- `compiler/tests/portable_abi.rs`
- `compiler/tests/query_exec.rs`

**Implementation notes**

Add a portable record such as:

```rust
ScreenSampleQuery {
    pixel_x: U32,
    pixel_y: U32,
    sample_offset: Vec2,
}
```

Then add a canonical helper conceptually like:

```wr
ray = view_ray(
    view = frame.view,
    sample = screen_sample,
    max_distance = quality.primary.max_distance,
    min_step = quality.primary.min_step,
    hit_epsilon = quality.primary.hit_epsilon,
    max_steps = quality.primary.max_steps,
)
```

Important rules:

- use `Camera.vertical_fov_degrees` as the canonical projection input
- derive aspect from `Viewport`
- keep `view_scale` in a compatibility helper only
- define pixel center as `(pixel_x + 0.5, pixel_y + 0.5)` before jitter unless a supersampling policy explicitly says otherwise
- define jitter/sample offsets in pixel units, then convert through the same normalized-coordinate path on every backend
- define the normalized coordinate mapping and y-axis direction once and test it from CPU and WGSL
- define whether depth attachments store ray parameter, camera-space z, linearized distance, or another named quantity

**Acceptance criteria**

- `ScreenSampleQuery` exists as a portable record.
- CPU tests prove center/corner/FOV projection behavior.
- Canonical projection uses camera FOV, not `view_scale`.
- Compatibility projection remains isolated.
- Projection convention tests cover pixel center, corners, aspect ratio, y-axis direction, jitter units, and depth semantics.

#### Task 18B2 — Add a `GenerateScreenSamples` presentation pass

**Description**

Teach the presentation plan to represent screen-lattice generation explicitly.

**Files**

- `compiler/presentation_plan/mod.rs`
- new or existing presentation lowering/validation helpers
- tests in new `compiler/tests/presentation_plan.rs`

**Implementation notes**

The plan should be able to say:

- viewport size
- sample count per pixel
- jitter state
- item count for downstream batch work

The first implementation may materialize the lattice as a transient array/buffer.
Later optimizations can generate it implicitly on-device.

**Acceptance criteria**

- Presentation plans can explicitly represent screen sample generation.
- The pass produces a typed item set suitable for world-batch queries.
- Validation catches mismatched viewport/item counts.

#### Task 18B3 — Freeze the first `PrimaryHitAttachmentContract`

**Description**

Name the authoritative primary-hit attachment schema before primary visibility execution begins.

**Files**

- `compiler/presentation_contract/mod.rs`
- `compiler/presentation_plan/mod.rs`
- `compiler/portable/abi.rs` if a portable presentation record is introduced
- tests in `compiler/tests/presentation_plan.rs` or `compiler/tests/portable_abi.rs`

**Implementation notes**

The contract should either wrap `Hit3` directly or preserve equivalent semantic identity:

- hit/miss state
- distance and world position
- world normal
- local position and local normal
- shading frame if required by downstream resolve
- step count
- feature, instance, repeat, and root-shape ids
- payload/provenance required by surface and participant contracts

If the screen-lattice index is the source of pixel/sample identity, document that indexing rule in the contract.
Do not let Phase 19 implement `PrimaryVisibilityPass` against an informal "enough identity" concept.

**Acceptance criteria**

- A named `PrimaryHitAttachmentContract` or equivalent schema exists before `PrimaryVisibilityPass` implementation.
- The schema is tied to existing `Hit3`/query-contract identity guarantees or explicitly explains any divergence.
- Downstream resolve passes have a stable contract to consume.

### Workstream C: Execution And Benchmarks

#### Task 18C0 — Add early screen-work cost-shape instrumentation

**Description**

Expose the performance shape of screen-lattice and world-batch work before the full presentation renderer exists.

**Files**

- `compiler/query_exec/cost.rs`
- `compiler/query_exec/*`
- `compiler/presentation_plan/mod.rs` if pass-level counters are already available
- tests in `compiler/tests/query_exec.rs`
- CLI/reporting tests where appropriate

**Implementation notes**

Track at least:

- screen sample count
- world-batch item count
- candidate count before and after pruning
- average and maximum ray steps
- hit/miss count
- domain flags used by the query
- backend selected and dispatch dimensions where available

This is intentionally earlier than the Phase 22 adaptive controller.
Phase 18 should tell the team whether the compiler is reducing work or merely batching expensive work.

**Acceptance criteria**

- Screen/world-batch reports expose candidate counts, step counts, hit/miss rates, and backend dispatch size where available.
- Reports distinguish dense compatibility execution from semantically pruned execution.
- Tests cover the metric shape for at least one tiny deterministic screen batch.

#### Task 18C1 — Implement world-batch execution on CPU, vGPU, and WGSL

**Description**

Make the new world-batch surfaces executable across the existing backends.

**Files**

- `compiler/query_exec/mod.rs`
- `compiler/query_exec/cpu.rs`
- `compiler/query_exec/vgpu.rs`
- `compiler/query_exec/wgsl.rs`
- `compiler/query_exec/native_bridge.rs`
- `compiler/query_exec/world.rs`
- tests in `compiler/tests/query_exec.rs`

**Implementation notes**

Prefer reusing the existing generic batch path rather than inventing a special presentation-only bridge.

The main new behavior is:

- region/world capture + domain + items array
- batch execution returning typed result arrays

Keep parity testing tight.
It is acceptable for the first CPU/WGSL implementation to include a dense baseline, but do not make that baseline the only contract shape.
The plan/result/observability model must preserve enough information for support-derived pruning and future view-culling artifacts.

**Acceptance criteria**

- World-batch queries run on CPU.
- World-batch queries run on WGSL where supported.
- CPU/WGSL parity tests exist for the new surfaces.
- Native bridge support exists without bespoke one-off exports per question.
- Dense baseline execution is clearly identified as a baseline, not the intended fast path.
- The implementation preserves metrics and hooks needed for semantic acceleration.

#### Task 18C2 — Add screen-batch microbenchmarks

**Description**

Create the first benchmark scenarios for world-batch screen work.

**Files**

- new benchmark scenarios under `benchmarks/field_engine/` or new `benchmarks/realtime_presentation/`
- benchmark tests

**Implementation notes**

At minimum add scenarios for:

- primary nearest world-batch rays over a small viewport
- depth/normal world-batch sampling over a small viewport
- support-pruned versus dense candidate evaluation on a representative small scene

These do not need to hit full game-scale resolutions yet.
They exist so the team can observe cost shape as the presentation pipeline grows.

**Acceptance criteria**

- Screen-space batch benchmarks exist.
- Perf reports can compare CPU and WGSL world-batch behavior.
- Perf reports include candidate count, ray-step count, hit/miss density, and pruning-rate information where available.

### Phase 18 Exit Criteria

- The query-family system can express world-batch work.
- Presentation plans can represent a screen lattice explicitly.
- The engine can generate rays from canonical view state.
- CPU/WGSL world-batch parity exists for the required presentation question set.
- Screen-work cost-shape reports can distinguish dense execution from semantically pruned execution.
- The first primary-hit attachment schema is named before Phase 19 execution work begins.

## Phase 19: Primary Visibility And Semantic Frame Attachments

### Goal

Produce the first real frame attachments from a view by tracing primary visibility through the new world-batch substrate.

### Why this is next

The first real-time frame milestone is not final beauty.
It is a **semantic primary pass** that materializes stable per-sample world meaning.

That is the presentation equivalent of a semantic G-buffer.
It is also the first pass that can prove whether the compiler is avoiding work in the view.
Primary visibility metrics should make candidate reduction and ray-step behavior visible from the start.

### Workstream A: Attachment Contracts And Resources

#### Task 19A1 — Add semantic attachment contracts to the presentation plan

**Description**

Teach presentation plans to describe typed frame attachments and their lifetimes.

**Files**

- `compiler/presentation_contract/mod.rs`
- `compiler/presentation_plan/mod.rs`
- new or existing validation code
- tests in `compiler/tests/presentation_plan.rs`

**Implementation notes**

Add a `FrameAttachmentContract` that carries at least:

- id / name
- semantic kind
- element schema
- resolution class / scale
- lifetime
- clear policy

Important principle:

- attachment **meaning** is semantic
- attachment **packing** is a later physical concern

Recommended first semantic attachment kinds:

- `PrimaryHit`
- `Depth`
- `WorldNormal`
- `Color`
- `Motion`

`Surface`, `Radiance`, and `Medium` can arrive in the next phase’s shading pipeline.

For `PrimaryHit`, define the element schema around semantic identity, not only display-derived values.
It should preserve enough of the existing hit/provenance contract to support later surface resolve, participant resolve, debug inspection, and history compatibility.
At minimum, miss state must be explicit and distinguishable from a valid hit with default-looking numeric fields.
This should reuse or refine the `PrimaryHitAttachmentContract` frozen in Phase 18 rather than inventing a new shape during execution work.

**Acceptance criteria**

- Presentation plans can declare typed attachments.
- Attachments distinguish semantic kind, lifetime, and resolution class.
- Validation exists for duplicate names and impossible lifetime combinations.
- `PrimaryHit` schema preserves stable world identity/provenance needed by downstream semantic queries.

#### Task 19A2 — Add the first physical attachment allocator using linear buffers

**Description**

Implement a deterministic resource model for frame attachments using CPU arrays and WGSL storage buffers first.

**Files**

- new `compiler/presentation_exec/resources.rs`
- `compiler/query_exec/wgsl/codegen.rs` or new presentation WGSL codegen module
- `compiler/portable/abi.rs`
- tests in `compiler/tests/portable_abi.rs`

**Implementation notes**

Do **not** start with textures.

Start with:

- row-major CPU buffers
- row-major WGSL storage buffers
- deterministic element layout from portable ABI rules where practical

This keeps the CPU oracle and WGSL aligned.

Backend texture lowering can come later as an optimization path that preserves the same semantic contract.

**Acceptance criteria**

- Attachment buffers can be allocated deterministically on CPU.
- Equivalent storage-buffer layout exists for WGSL.
- The semantic contract does not leak backend texture details.

### Workstream B: Primary Visibility

#### Task 19B1 — Implement `PrimaryVisibilityPass`

**Description**

Add the first real presentation pass that traces primary rays against the world and writes a primary-hit attachment.

**Files**

- `compiler/presentation_plan/mod.rs`
- new `compiler/presentation_exec/mod.rs`
- new `compiler/presentation_exec/cpu.rs`
- tests in new `compiler/tests/presentation_exec.rs`

**Implementation notes**

The pass should do this, conceptually:

1. consume `FrameState`
2. consume `ScreenSampleQuery[]`
3. generate `RayQuery[]`
4. run `spatial.nearest.batch.world`
5. materialize a primary-hit attachment
6. optionally derive depth/world-normal attachments

Keep this pass thin.
The primary-hit attachment is the semantic source of truth for this pass; depth and world-normal are derived attachments.
The pass should report dense candidate count versus pruned candidate count whenever a semantic acceleration artifact is active.

Do **not** resolve:

- surface
- radiance
- medium
- final lighting

The primary pass should preserve stable world meaning and keep the hot path minimal.

**Acceptance criteria**

- A `PrimaryVisibilityPass` exists in the plan/execution model.
- CPU execution materializes a primary-hit attachment.
- Optional depth/world-normal derivation works.
- Miss semantics are explicit and tested.
- Downstream resolve tests can use `PrimaryHit` identity without re-tracing or treating depth as the authoritative hit.
- Primary visibility reports candidate reduction, ray-step distribution, hit/miss rate, and backend dispatch shape where available.

#### Task 19B2 — Add debug export for primary attachments

**Description**

Add a simple debug/export path so engineers can inspect primary-hit, depth, and normal attachments.

**Files**

- new or existing `compiler/presentation_exec/debug.rs`
- CLI support
- tests in `compiler/tests/cli.rs`

**Implementation notes**

Support simple outputs such as:

- PPM depth visualization
- PPM normal visualization
- textual stats for hit rate / miss rate / attachment dimensions

This is essential for junior debugging.

**Acceptance criteria**

- Depth and normal attachments can be inspected from the CLI.
- Debug export works from CPU execution.

### Workstream C: WGSL Execution And Parity

#### Task 19C1 — Add WGSL execution for `PrimaryVisibilityPass`

**Description**

Run the primary visibility pass through WGSL using the world-batch substrate and attachment buffers.

**Files**

- `compiler/query_exec/wgsl/codegen.rs` and/or new presentation WGSL modules
- `compiler/presentation_exec/wgsl.rs`
- tests in `compiler/tests/presentation_exec.rs`

**Implementation notes**

The WGSL path should:

- read screen samples
- generate rays canonically
- execute world nearest batch work
- write primary-hit/depth/normal buffers

Parity should be checked on small frame sizes first.

**Acceptance criteria**

- WGSL execution exists for primary visibility.
- CPU/WGSL parity tests exist for primary-hit, depth, and normal attachments.
- Attachment dimensions and miss semantics match.

### Phase 19 Exit Criteria

- Presentation plans can declare and allocate semantic attachments.
- The engine can produce primary-hit/depth/normal attachments from a view.
- CPU and WGSL agree on the primary pass for small frames.
- The repo has a practical debugging path for frame attachments.

## Phase 20: Lighting, Surface/Participant Resolve, And First Real-Time Color Path

### Goal

Turn semantic primary visibility into the first full color frame path using explicit resolve and shading passes.

### Why this is next

Primary visibility alone is the architectural spine.
This phase turns it into something visually real while still keeping the pass graph clean and explicit.

### Workstream A: Lighting Contracts

#### Task 20A1 — Add `LightingContract` and move current render lighting metadata into it

**Description**

Create a typed lighting contract for presentation and lower current render `light` / `fill_dir` metadata into it.

**Files**

- `compiler/presentation_contract/mod.rs`
- `compiler/hir/def.rs`
- `compiler/hir/lower.rs`
- `compiler/mir/lower.rs`
- tests in `compiler/tests/preview_project.rs`

**Implementation notes**

Start narrow.

Recommended first fields:

- `key_light: Light`
- `fill_direction: Vec3`
- `fill_strength: F32`
- `ambient_color: Vec3`

Do not widen into many-light infrastructure here.

The immediate purpose is to make lighting a typed frame input rather than ad hoc helper parameters.

**Acceptance criteria**

- `LightingContract` exists.
- Current render metadata lowers into it without authored churn.
- Lighting inputs are now part of the presentation plan, not one-off helper argument plumbing.

### Workstream B: Resolve Passes

#### Task 20B1 — Add `SurfaceResolvePass`

**Description**

Materialize surface data from the primary-hit attachment through a world-batch family pass.

**Files**

- `compiler/presentation_plan/mod.rs`
- `compiler/presentation_exec/cpu.rs`
- `compiler/query_contract/mod.rs` / `compiler/query_plan/mod.rs` if any final world-batch plumbing is still needed
- tests in `compiler/tests/presentation_exec.rs`

**Implementation notes**

The pass should conceptually do:

- read `PrimaryHit`
- create hit batch items or reuse hit attachment directly
- run `surface.sample.batch.world`
- write `Surface` attachment

For this first implementation, it is okay to process all pixels and write default values for misses.
Hit compaction comes later.

**Acceptance criteria**

- `SurfaceResolvePass` exists.
- CPU path materializes a surface attachment from primary hits.
- Miss/default behavior is explicit and tested.

#### Task 20B2 — Add `ParticipantsResolvePass`

**Description**

Materialize radiance and medium data through explicit participant-family passes.

**Files**

- `compiler/presentation_plan/mod.rs`
- `compiler/presentation_exec/cpu.rs`
- tests in `compiler/tests/presentation_exec.rs`

**Implementation notes**

The pass should be conditional on the frame contract and quality contract.

It should be able to skip:

- radiance resolve
- medium resolve

when they are not requested.

This keeps participant work explicit and schedulable.

**Acceptance criteria**

- Radiance and medium resolve are explicit passes.
- The plan can disable them when the frame contract or quality contract does not need them.
- CPU path materializes radiance/medium attachments when enabled.

### Workstream C: Shading And Compatibility Rebase

#### Task 20C1 — Add `ShadePrimaryPass` and `CompositeColorPass`

**Description**

Turn the resolved semantic attachments into the first real color output.

**Files**

- new or existing `compiler/presentation_exec/shade.rs`
- `compiler/presentation_exec/cpu.rs`
- WGSL presentation shading codegen
- tests in `compiler/tests/presentation_exec.rs`

**Implementation notes**

Port the current preview look intentionally as a **compatibility shading recipe**.
The goal is not a new artistic model yet.
The goal is to express the current shading/composite behavior as explicit passes.
Do not let the current preview look become the permanent semantic law of presentation.
Long-term material, lighting, and artistic models remain future contracts layered on top of this compatibility recipe.

A good first split is:

- `ShadePrimaryPass` -> direct lighting + fill + emissive/radiance/medium blend
- `CompositeColorPass` -> final exported color attachment

Keep the math close to `__wr_render_scene_color_capture` initially.
That makes compatibility testing much easier.

**Acceptance criteria**

- The current preview shading logic has a pass-oriented equivalent.
- That equivalent is marked as a compatibility recipe, not the authoritative long-term presentation model.
- Color is produced from explicit attachments and contracts.
- CPU and WGSL parity tests exist for final color on small frames.

#### Task 20C2 — Rebase preview PPM on top of the new presentation pipeline

**Description**

Make existing preview projects execute through the presentation plan rather than through the legacy helper island.

**Files**

- `compiler/mir/lower.rs`
- `compiler/tests/preview_project.rs`
- `compiler/tests/codegen_v2.rs`

**Implementation notes**

The desired shape is:

`render decl -> presentation plan -> color attachment -> PPM export`

At the end of this task:

- `__wr_render_scene_color_capture` should be gone, or
- it should be a thin compatibility wrapper over the new pass graph

Do not change user-facing syntax here.

**Acceptance criteria**

- Preview projects run through the presentation plan.
- CPU/WGSL preview tolerance remains acceptable.
- The old render-helper island is no longer the conceptual source of truth.

### Phase 20 Exit Criteria

- The engine can produce a final color frame from explicit presentation passes.
- Preview output is now a consumer of presentation plans.
- Lighting and participant work are explicit and schedulable.
- CPU/WGSL parity exists for the first full color path.

## Phase 21: Temporal Contracts, Motion, And History-Aware Resolve

### Goal

Add explicit temporal state and history reuse so presentation becomes stable under motion and gains the first major 60 FPS lever.

This phase is directional until the Phase 20 color path exists.
Do not start it by designing a large temporal framework in the abstract; start it only after primary visibility, resolve, and color attachments can provide real inputs and real failure cases.

### Why this is next

Real-time field rendering without temporal semantics will fight noise, shimmer, and brute-force cost.
This phase turns temporal reuse into a contract instead of an accident.

### Workstream A: Temporal Contracts

#### Task 21A1 — Add `TemporalContract` and history slot semantics

**Description**

Teach frame contracts to declare temporal reuse explicitly.

**Files**

- `compiler/presentation_contract/mod.rs`
- `compiler/presentation_plan/mod.rs`
- validation/tests in new or existing presentation-plan test modules

**Implementation notes**

Add at least:

- history slot ids
- reuse mode
- invalidation policy
- maximum age / strictness if needed

Recommended starting modes:

- `Disabled`
- `ReprojectColor`
- `ReprojectColorAndMotion`

Keep this typed.
Do not use ad hoc string settings.

**Acceptance criteria**

- A typed temporal contract exists.
- Frame contracts can declare history-backed attachments.
- Validation catches impossible history combinations.

#### Task 21A2 — Extend `FrameState` and attachments for motion/history compatibility

**Description**

Carry the previous-view data and motion semantics needed for reprojection.

**Files**

- `compiler/portable.rs`
- `compiler/portable/abi.rs`
- `compiler/presentation_contract/mod.rs`
- tests in `compiler/tests/portable_abi.rs`

**Implementation notes**

The frame/view state should now be sufficient to compute motion vectors from:

- current view
- previous view
- hit position or depth

Add a semantic `Motion` attachment kind if it does not already exist.

Also add a history compatibility key concept derived from:

- attachment schema
- view contract
- relevant quality contract fields

**Acceptance criteria**

- `FrameState` carries enough information for reprojection.
- Motion attachment semantics are explicit.
- History compatibility/invalidation has a canonical representation.

### Workstream B: Temporal Execution

#### Task 21B1 — Implement `MotionResolvePass`

**Description**

Compute per-pixel motion vectors from view state and semantic hit data.

**Files**

- `compiler/presentation_exec/cpu.rs`
- `compiler/presentation_exec/shade.rs` or new motion module
- tests in `compiler/tests/presentation_exec.rs`

**Implementation notes**

The pass should:

- read primary-hit/depth
- compare current and previous view transforms
- write motion vectors or invalid markers for misses/disocclusions

Keep semantics explicit for misses and newly visible pixels.

**Acceptance criteria**

- CPU path materializes motion vectors.
- Miss/disocclusion behavior is explicit and tested.

#### Task 21B2 — Implement `TemporalResolvePass`

**Description**

Add a first history-aware resolve for color.

**Files**

- `compiler/presentation_exec/cpu.rs`
- WGSL presentation execution
- tests in `compiler/tests/presentation_exec.rs`

**Implementation notes**

Start with a simple, defensible algorithm:

- reproject previous color by motion
- validate history compatibility
- neighborhood clamp or conservative blend
- fall back to current color on invalid history

Do **not** add a large denoiser or heavyweight post stack here.
The goal is clear semantics and measurable stability.

**Acceptance criteria**

- CPU temporal resolve exists.
- WGSL temporal resolve exists.
- History invalidation and fallback are explicit.
- Color stability improves under camera motion in tests.

### Workstream C: Temporal Tooling And Tests

#### Task 21C1 — Add temporal stability tests and sample content

**Description**

Create tests and sample projects that exercise history, motion, and invalidation.

**Files**

- new sample under `language/` if helpful
- `compiler/tests/presentation_exec.rs`
- `compiler/tests/preview_project.rs` or a new frame-specific test file

**Implementation notes**

Add cases for:

- static camera repeated frames
- slow camera motion
- abrupt camera cut / history invalidation
- thin/alias-prone geometry

Use small frames for determinism.

**Acceptance criteria**

- Temporal stability tests exist.
- History invalidation is covered.
- Static repeated frames remain deterministic.

### Phase 21 Exit Criteria

- Frame contracts can declare temporal reuse.
- The engine can produce motion vectors and reproject history.
- CPU and WGSL temporal resolve agree within declared tolerances.
- Temporal stability is measurable in tests.

## Phase 22: 60 FPS Quality Ladders, Adaptive Control, And Presentation Acceleration

### Goal

Make “real-time” an explicit contract and mature the structural tools the engine needs to chase 60 FPS without abandoning semantics.

This phase is also a checkpoint phase.
After Phase 20, decide whether quality control and acceleration should remain in this RFC or become a focused follow-up roadmap based on measured presentation cost.
Acceleration begins earlier through Phase 18/19 instrumentation and support-pruning hooks.
Phase 22 is where those ideas become a coherent quality-control and optimization system.

Expect this phase to split unless the Phase 20 color path shows the bottlenecks are simple.
Likely split points are:

- **quality/control**: contracts, degradation ladders, observability, adaptive controller
- **acceleration**: support-driven culling, hit compaction, physical packing, half-resolution execution

The roadmap keeps them together directionally because they affect each other, but implementation should not force one giant phase if measured bottlenecks argue for a focused follow-up RFC.

### Why this is next

At this point the pipeline exists.
Now the repo must stop hoping that faster hardware or nicer math alone will solve frame time.

This phase treats 60 FPS as a **control problem**:

- define allowed trade-offs
- measure actual cost
- adapt legally
- optimize the pass graph structurally

### Workstream A: Quality Contracts

#### Task 22A1 — Add `RealtimeQualityContract`

**Description**

Create a typed contract for frame-time trade-offs.

**Files**

- `compiler/presentation_contract/mod.rs`
- `compiler/portable.rs` if any portable record is needed
- validation/tests in presentation-plan modules

**Implementation notes**

Recommended fields:

- `target_fps`
- `allow_dynamic_resolution`
- `internal_resolution_scale`
- `primary_max_steps`
- `allow_radiance`
- `allow_media`
- `temporal_mode`
- `allow_half_res_participants`
- `allow_hit_compaction`

The quality contract should belong to presentation/frame planning.
It should **not** be smuggled back into `SceneDomain`.

**Code sketch**

```rust
pub struct RealtimeQualityContract {
    pub target_fps: u32,
    pub internal_resolution_scale: f32,
    pub allow_dynamic_resolution: bool,
    pub primary_max_steps: i32,
    pub allow_radiance: bool,
    pub allow_media: bool,
    pub temporal_mode: TemporalReuseMode,
}
```

**Acceptance criteria**

- A typed quality contract exists.
- The contract is distinct from `SceneDomain`.
- Validation exists for invalid scale/step combinations.

#### Task 22A2 — Add named quality ladders and degradation order

**Description**

Define canonical quality tiers and the order in which presentation may legally degrade.

**Files**

- `compiler/presentation_contract/mod.rs`
- `compiler/presentation_plan/mod.rs`
- CLI/reporting/tests

**Implementation notes**

Recommended named tiers:

- `realtime_60`
- `realtime_120`
- `high`
- `ultra`
- `debug`

Define an explicit degradation order, for example:

1. reduce internal resolution
2. lower primary steps
3. disable media
4. lower radiance quality or disable radiance
5. switch participants to half resolution

For `realtime_60` and `realtime_120`, reports should distinguish:

- achieved native output resolution
- achieved reconstructed output resolution
- internal resolution scale
- active temporal mode
- active semantic acceleration artifacts
- remaining bottleneck pass

Make this explicit in code and diagnostics.
Do not bury it in backend heuristics.

**Acceptance criteria**

- Quality ladders are typed and named.
- Degradation order is explicit.
- Reports can explain which tier/fallbacks were active.
- Reports clearly distinguish native 1080p60/120 attempts from reconstructed or reduced-internal-resolution modes.

### Workstream B: Observability And Adaptive Control

#### Task 22B1 — Add frame-level observability and cost accounting

**Description**

Build a frame-specific observability layer on top of the existing query metrics.

**Files**

- new `compiler/presentation_exec/cost.rs`
- `compiler/presentation_exec/*`
- CLI/reporting/tests

**Implementation notes**

Track at least:

- frame dimensions / internal resolution scale
- primary hit rate
- average and max trace steps
- candidate count before and after semantic pruning
- support-pruning and tile/cluster culling effectiveness
- surface resolve count
- participant resolve count
- history reuse rate
- attachment byte counts
- per-pass dispatch time
- tile cull efficiency once available

This should aggregate query-family observability where appropriate.

**Acceptance criteria**

- Frame-level metrics exist.
- Reports can explain where time and bandwidth go.
- Metrics are available in CLI and/or JSON form.
- Metrics make it clear whether performance gains came from doing less semantic work, lowering quality, or backend speed.

#### Task 22B2 — Add an adaptive controller for target FPS

**Description**

Add a closed-loop controller that adjusts legal quality knobs based on measured frame cost.

**Files**

- `compiler/presentation_exec/mod.rs`
- new `compiler/presentation_exec/controller.rs`
- tests in new or existing presentation-exec test modules

**Implementation notes**

Start simple.
A moving-average controller is enough for the first cut.

The controller should only change fields allowed by the active quality contract.

It should be deterministic in tests by using mocked frame timing samples.

**Acceptance criteria**

- A deterministic adaptive controller exists.
- The controller only uses legal degradations from the quality contract.
- Tests cover step-down and step-up behavior around the FPS target.

### Workstream C: Presentation Acceleration

#### Task 22C1 — Add tile/cluster culling artifacts derived from semantic support data

**Description**

Exploit the existing semantic support infrastructure to prune screen-space work by tile or cluster.

**Files**

- `compiler/presentation_plan/mod.rs`
- `compiler/query_plan/mod.rs` if new artifact contracts are needed
- `compiler/scene_ir/mod.rs`
- `compiler/presentation_exec/*`
- tests/benchmarks

**Implementation notes**

This is where `support.summary` starts paying presentation dividends.

Add a derived artifact such as `ViewCullingTable` or equivalent that maps coarse screen regions to plausible shape candidate sets.

This should be driven from semantic support data, not from ad hoc runtime-only heuristics.
The artifact should state its correctness policy explicitly:

- conservative if it may include false positives but must not drop valid hits
- approximate if it may trade correctness under a named quality mode
- backend-specific only if its semantic source and fallback are clear

**Acceptance criteria**

- Presentation planning can derive a culling artifact from scene support data.
- The artifact is used to prune tile/cluster candidate work.
- Benchmarks show measurable candidate reduction.
- Correctness policy is explicit and covered by CPU oracle tests.

#### Task 22C2 — Add hit compaction for expensive passes

**Description**

Avoid running surface/participant/shading work over guaranteed misses when the contract allows compaction.

**Files**

- `compiler/presentation_plan/mod.rs`
- `compiler/presentation_exec/*`
- WGSL execution
- tests/benchmarks

**Implementation notes**

After primary visibility, compact hit pixels into a work list for:

- surface resolve
- radiance resolve
- medium resolve
- expensive shading work

Keep a mapping back to full-frame coordinates for final composite.

This is one of the highest-value structural optimizations in the whole roadmap.

**Acceptance criteria**

- Hit compaction exists as an explicit presentation optimization.
- Later passes can consume compacted work lists.
- Frame outputs remain correct after remapping to full-frame coordinates.

#### Task 22C3 — Separate semantic attachments from physical packing and half-resolution execution

**Description**

Introduce a physical layout layer that can pack and scale attachments without changing their semantic contracts.

**Files**

- `compiler/presentation_exec/resources.rs`
- WGSL presentation execution
- tests/benchmarks

**Implementation notes**

This is where physical layout can start diverging from semantic schema.

Examples:

- packed depth/normal buffers
- half-resolution radiance or medium attachments
- packed motion vectors

The semantic contract remains stable.
Only the execution binding and physical layout change.

**Acceptance criteria**

- Semantic attachment meaning remains stable.
- Physical packing/resolution can vary by backend or quality tier.
- Reports explain the chosen packing/scaling strategy.

### Workstream D: Benchmarks And Gates

#### Task 22D1 — Add a real-time presentation benchmark suite

**Description**

Create benchmark scenarios that reflect presentation pass costs rather than only isolated query costs.

**Files**

- new `benchmarks/realtime_presentation/`
- benchmark tests/harness integration
- docs in `benchmarks/README.md`

**Implementation notes**

Add representative scenes for:

- dense constructive geometry
- repetition-heavy scenes
- thin-stack / alias-prone geometry
- media/radiance-enabled scenes

Track at least:

- ms/frame or normalized cost/frame
- pass breakdown
- quality tier chosen
- internal resolution scale

Do not hardcode machine-specific pass/fail numbers in a way that makes CI meaningless.
Prefer structural regression thresholds and informative reports.

**Acceptance criteria**

- A dedicated presentation benchmark suite exists.
- Perf reports explain pass breakdown and quality decisions.
- The suite is useful for chasing a 60 FPS target on real developer hardware.

### Phase 22 Exit Criteria

- Presentation has typed quality contracts and degradation ladders.
- The engine can adapt quality legally against a frame target.
- Support-driven tile/cluster pruning exists.
- Hit compaction exists for expensive passes.
- The repo has a meaningful real-time presentation benchmark suite.
- Reports identify whether 60/120 FPS attempts succeeded natively, through reconstruction, through quality degradation, or not yet.
- If quality/control and acceleration split into separate projects, each split has its own acceptance criteria and independent review gate.

## Phase 23: Authoritative View/Frame Surface And Tooling

### Goal

Land the user-facing view/frame surface after the internal architecture is real, and do the migration once.

### Why this is last

The repo should not ask authors to rewrite presentation syntax until:

- the internal contracts exist
- the pass graph is real
- the first real-time slice works
- quality/history/tooling are in place

This phase makes the new model visible.

### Workstream A: Authored Surface Cut

#### Task 23A0 — Make the author-facing cut once

**Description**

Bundle the final authored presentation syntax cut into one deliberate migration.

**Implementation notes**

Do not ask users to migrate:

- once for view/frame internals
- then again for quality/history
- then again for output declarations

Land the authoritative authored surface only after the architecture behind it exists.

**Acceptance criteria**

- There is one coherent authored migration.
- Docs, examples, and compatibility tests move together.

#### Task 23A1 — Add an authoritative `view` declaration surface (or equivalent) for real-time presentation

**Description**

Introduce the user-facing declaration that reflects the new architecture cleanly.

**Files**

- parser/HIR/typeck/lowering layers
- spec/docs/examples
- tests in spec and preview suites

**Implementation notes**

Recommended direction:

- add a `view` declaration for real-time presentation
- keep `render` as compatibility/offline sugar where it remains useful

Conceptual authored example:

```wr
view MainView(world: RegionCapture, camera: Camera) {
    domain = Presentation(world = world)
    viewport = viewport(width = 1280, height = 720)
    quality = realtime_quality(target_fps = 60)
    lighting = key_light(
        light = Light(...),
        fill_direction = normalize(vec3(-0.4, 0.5, 0.2))
    )
    outputs = frame_outputs(color = true, depth = true, normal = true, motion = true)
    history = temporal_history(color = true)
}
```

This keeps the architecture legible:

- observer/view
- world domain
- quality
- lighting
- outputs
- history

**Acceptance criteria**

- The authoritative authored presentation surface exists.
- The new surface matches the internal contract model.
- The surface is expressive enough for the first real-time slice.

#### Task 23A2 — Add typed authored helpers for viewport, quality, outputs, and history

**Description**

Provide typed helpers/constructors that make authored view declarations readable without introducing a giant bespoke grammar.

**Files**

- stdlib or builtin helper definitions as appropriate
- parser/HIR/typeck where needed
- spec/docs/tests

**Implementation notes**

Prefer typed helpers over a large amount of custom syntax.
This keeps the language surface regular and leverages the existing expression/type system.

Recommended helpers:

- `viewport(...)`
- `realtime_quality(...)`
- `frame_outputs(...)`
- `temporal_history(...)`
- `key_light(...)`

**Acceptance criteria**

- Typed helpers exist for the core view/frame concepts.
- The authored surface stays compact and regular.

### Workstream B: Compatibility And Runtime Entry Points

#### Task 23B1 — Keep `render` compatibility explicit and tested

**Description**

Preserve a compatibility path for the current preview-oriented `render` surface while the new `view` model becomes authoritative.

**Files**

- parser/HIR/typeck/lowering layers
- tests in `compiler/tests/preview_project.rs`
- spec/docs compatibility coverage

**Implementation notes**

`render` can remain as:

- compatibility sugar over `view + export`, or
- an explicitly legacy/offline presentation surface

Either choice is acceptable.
The key requirement is that the compatibility story is explicit and tested.

**Acceptance criteria**

- Existing preview content still has a supported path.
- Compatibility behavior is documented and covered by tests.

#### Task 23B2 — Add host/runtime entry points for named views

**Description**

Expose compiled views as first-class host-facing entry points so the engine runtime can evaluate them without going through ad hoc preview-only paths.

**Files**

- CLI/runtime bridging code
- `compiler/bin/wrela/*`
- potential runtime adapter code
- tests in CLI/project e2e suites

**Implementation notes**

Examples:

- `wrela preview path/to/project --view MainView`
- runtime APIs that request a named view/frame output bundle

Keep this as a thin host boundary.
Do not build a windowing/swapchain subsystem into the compiler.

**Acceptance criteria**

- Named views have clear host/runtime entry points.
- CLI and test harnesses can evaluate specific views and exported attachments.

### Workstream C: Docs, Spec, And Samples

#### Task 23C1 — Update the spec, examples, and sample projects

**Description**

Make the new presentation architecture concrete in docs and executable content.

**Files**

- `language/spec/README.md`
- `language/spec/tests/spec/language_spec_test.wr`
- `language/preview*` or new `language/view_*` projects
- `README.md`

**Implementation notes**

Add at least one sample that shows:

- a view declaration
- typed outputs
- quality/history settings
- CPU and WGSL execution

Keep the first sample narrow and comprehensible.

**Acceptance criteria**

- Docs explain the new view/frame model.
- Executable samples exist.
- The spec suite covers the authored surface sufficiently.

#### Task 23C2 — Add presentation-plan and frame-contract reporting to CLI/docs

**Description**

Make the new system easy to inspect and debug from tooling.

**Files**

- CLI/reporting code
- docs/tests

**Implementation notes**

Recommended commands/report modes:

- `presentation-plan`
- `frame-contracts`
- `preview --view ... --attachment depth`
- `preview --view ... --json-report`

The important thing is that engineers can see:

- what the view requested
- what plan was built
- what quality tier executed
- what attachments/history were used

**Acceptance criteria**

- Tooling can report presentation plans and frame contracts.
- Attachment/export/debug flows are documented.

### Phase 23 Exit Criteria

- The authoritative authored presentation surface exists.
- Compatibility with current preview/render content is explicit and tested.
- Host/runtime entry points for views exist.
- Docs/spec/examples/tooling all reflect the new architecture.

## Final Exit Criteria For This Roadmap

This roadmap is complete when all of the following are true.

1. Presentation is no longer centered on `__wr_render_capture_to_ppm` or `__wr_render_scene_color_capture`.
2. The compiler has a canonical `PresentationPlan` and typed view/frame contracts.
3. World-batch query surfaces exist for the question set presentation needs.
4. Semantic attachments, history contracts, and quality contracts exist.
5. CPU oracle and WGSL parity exist for the primary presentation passes.
6. The engine can produce a temporally stable real-time color path through explicit passes.
7. Compiler-derived semantic acceleration artifacts can reduce presentation work while preserving named query/frame contracts.
8. The repo has benchmark and observability support that make 60 FPS a measurable, controllable target rather than a wish.
9. Native and reconstructed 1080p60/1080p120 target-scene attempts are reported clearly, without implying blanket performance guarantees.
10. The authored presentation surface matches the internal architecture and has a clear compatibility story.
11. Each phase has passed its acceptance criteria and the `AGENTS.md` independent-review completion gate.
12. The roadmap was explicitly revalidated after Phase 17 and after Phase 20, with later scope adjusted to what the implemented plan/color path actually taught the team.
13. `PresentationPlan` is documented as the first concrete query program, and any candidate generic query-program machinery has been recorded for comparison against future collision/traversal work rather than prematurely extracted.

## Suggested Execution Order Inside The Team

Start with Phase 17 only.
Do not begin Phase 18 implementation until Phase 17 exit criteria pass and the independent review feedback has been handled.

A practical execution order is:

- start with **17A1, 17A2, 17B1** together
- then do **17A3** and **17B2**
- add **17C1** as soon as a degenerate presentation plan can be built, because the plan dump will make review and follow-on design easier
- run the preview golden/stability suite before calling Phase 17 done
- re-open this roadmap and adjust Phase 18 details before implementing world-batch surfaces, including what should remain presentation-owned and what should be watched as candidate query-program machinery
- then land **18A1–18A3** before deep execution work
- then let one engineer own **18B1/18B2** while another lands **18C1**
- after that split Phase 19 into:
  - attachment contracts/resources
  - CPU primary pass
  - WGSL primary pass
- Phase 20 can parallelize surface/participants resolve against shading/composite work once the attachment schemas are frozen
- Phase 21 should stay fairly tight because temporal compatibility is easy to muddy if too many people move it at once
- Phase 22 is the place for broader performance parallelization once the contracts are stable; expect a quality/control versus acceleration split if Phase 20 measurements justify it
- Phase 23 should be the final authored/tooling/documentation cut, not an early branch

That order keeps the semantic substrate clean and lets junior engineers take self-contained tasks without stepping on unstable boundaries.
