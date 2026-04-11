# RFC 0005: Canonical Presentation Plans, View/Frame Contracts, And Real-Time Roadmap

Status: Revised after Phase 17 and the ray-solver strategy review, then revalidated again after the Phase 21 first explicit color path landed. Phase 18 remains the committed closing milestone. Phases 22-24 remain directional, but the post-Phase-21 color path now makes the remaining splits more concrete.

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

This roadmap should now be executed in two horizons:

1. **Committed next milestone: Phase 18.**
   Replace compatibility-shaped presentation assumptions with clean query axes, a canonical view surface skeleton, and a screen-lattice substrate.

2. **Directional roadmap: Phases 19-24.**
   Use these phases as the intended architecture, but revisit scope again after the first explicit color path exists.

Phase 17 proved the bridge.
The immediate next win is to stop treating backwards compatibility as a design constraint.
Existing `render`/PPM behavior may remain as temporary scaffolding while it is useful for tests, but it is no longer a public promise the roadmap must preserve.

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
7. Phase 17 added `presentation_contract`, `presentation_binding`, and `presentation_plan` as real compiler modules.
8. Phase 17 split render metadata into view, frame, lighting, and compatibility projection buckets.
9. Phase 17 added portable `Viewport`, `ViewState`, and `FrameState` records.
10. The CLI can now inspect compiled presentation plans without exposing raw legacy helper names.

### What is currently holding presentation back

1. **`render` is now routed through a presentation plan, but it is still temporary scaffolding.**
   Phase 17 made `__wr_render_capture_to_ppm` an execution binding detail, but the only executable presentation path is still the legacy PPM export.
   With no backwards-compatibility requirement, this should be retired instead of preserved as a long-term authored surface.

2. **The canonical camera projection is represented but not executable yet.**
   `Camera.vertical_fov_degrees` now has a canonical home in the view contract and render metadata, but the current preview helper still computes rays through compatibility projection values.

3. **Render budgets are still coupled to legacy authored-domain metadata.**
   `lower_render_trace_budget_values` still scrapes max distance / min step / epsilon / max steps out of domain call bodies. That is practical as compatibility glue, but it is the wrong long-term boundary.

4. **The query surface model still combines concepts that Phase 18 should split.**
   Presentation wants query dependencies by family/contract plus independent target and cardinality.
   Adding more combined enum cases like `WorldBatch` will harden a compatibility shape the roadmap can avoid.

5. **There is no world-batch surface in the query registry yet.**
   Current batch support is capture-oriented. Real-time presentation needs world-batch execution over the screen lattice.

6. **There is no executable semantic attachment model.**
   Phase 17 created shallow attachment contracts, but the engine still lacks typed attachment schemas, clear policies, allocation, lifetime validation, and history-slot execution.

7. **Temporal and quality contracts are only shallow placeholders.**
   Phase 17 created the right contract slots, but not real history compatibility, motion semantics, quality ladders, or frame-budget policy.

8. **There is no frame-level execution or cost model yet.**
   Right now the repo has query-level observability and benchmark scaffolding, but not a frame-level latency/quality control loop.

9. **Ray-shaped queries do not yet have a dedicated solver-planning boundary.**
   CPU execution already has pieces of support-based pruning, and WGSL generation can batch ray work, but dense marching is still too implicit.
   Before primary visibility becomes the real presentation hot path, ray-shaped contracts need an internal `RaySolverPlan` layer with dense fallback, solver facts, and solver observability.

### What the code says about the next move

The repo is already telling us the right answer.

The existing query-family work made the semantic substrate strong.
The existing WGSL batch machinery made storage-buffer execution real.
Phase 17 made presentation a compiler-owned plan, but only as a bridge over the old helper path.

So the next project is not “make render declarations more powerful.”
It is also not “preserve render compatibility while adding another path.”

It is:

- split query target/cardinality before world-batch surfaces multiply
- introduce the canonical authored `view` surface skeleton early
- add a query-engine ray-solver boundary after Phase 18, before primary visibility execution hardens
- canonicalize view/frame semantics
- extend query surfaces to world-batch screen work
- materialize semantic attachments
- lower presentation to an explicit pass graph
- add time/history/quality as first-class contracts
- make PPM a debug/export target over attachments, not the conceptual renderer

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
World-target batch execution is only the substrate.
Compiler-derived semantic acceleration is what should make the substrate fast.

At 1080p, a full-resolution one-sample frame contains about 2.07 million primary samples.
That is roughly:

- 124 million primary samples per second at 60 FPS
- 249 million primary samples per second at 120 FPS

Those targets are plausible only if the compiler aggressively reduces candidate work, specializes kernels, reuses temporal history, and permits legal quality trade-offs.
This roadmap should therefore treat native 1080p60 and 1080p120 as benchmark targets for representative scenes, not as blanket guarantees for arbitrary authored worlds.

## Ray Solver Thesis: Why Narrow Can Still Be Fast

The first real-time path should push ray marching and analytic solving as far as possible before broadening into unrelated presentation representations.

The engine should not treat authored fields as opaque distance functions that are sampled until something happens.
It should compile authored fields into **contract-specific ray solvers**.

For ray-shaped query contracts, the compiler should ask:

**For this field tree, ray family, support structure, transform stack, repetition pattern, derivative availability, quality contract, and provenance requirement, what is the cheapest valid solver for this query?**

This keeps the scope narrow without making it shallow.
The initial real-time architecture should stay field-native and query-driven:

- `spatial.nearest` remains the semantic question for primary visibility
- `spatial.occluded` remains the semantic question for visibility/shadow tests
- `spatial.distance` and `spatial.normal` remain query-family contracts
- presentation consumes those contracts through frame passes
- the query engine owns the solver choices that make those contracts fast

That means a future fast path should be able to combine, per query and per field subtree:

- conservative support and hierarchy rejection
- analytic primitive or subtree intersections
- Lipschitz-safe sphere tracing
- interval/root isolation
- safeguarded Newton refinement
- derivative- and curvature-aware steps
- repeat-aware ray traversal
- tile/packet ray solving
- neighbor/frame continuation
- dense marching fallback

This is still not a handwritten shader architecture.
WGSL remains generated backend output.
A small handwritten WGSL prelude may exist as backend runtime support, but presentation passes, query kernels, field logic, and solver choices should come from compiler-owned contracts, plans, facts, and code generation.

The strategic performance bet is:

**Wrela should compile fields to solvers, not merely to distance functions.**

## Query Engine Scope: From Primitives To Programs

Presentation should not be treated as a separate renderer that merely calls the query engine.
It should be treated as the first large **query program** built on top of the current query substrate.

The query engine should be understood as the compiler/runtime layer that serves disciplined, contract-bearing questions about the world.
Today its vocabulary is mostly low-level because that is where the engine had to start:

- point, ray, and hit-shaped items
- distance, normal, nearest, occlusion, surface, radiance, medium, and support-summary questions
- scalar/batch cardinalities and capture/world targets

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

This roadmap has ten goals.

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

6. **Make ray-solver acceleration query-engine-owned.**
   Ray marching, analytic solving, interval reasoning, derivative use, and dense fallback are execution strategies for query contracts, not presentation-specific renderer semantics.

7. **Introduce semantic frame attachments and history contracts.**
   The engine needs typed outputs, lifetimes, and temporal reuse semantics.

8. **Make CPU the oracle for frame semantics too.**
   Every presentation pass must have a CPU truth path before backend-specific tuning becomes authoritative.

9. **Make 60 FPS an explicit contract problem.**
   Quality, degradation, and adaptation should be owned by typed frame contracts and metrics, not by scattered helper constants.

10. **Make the canonical authored surface early and honest.**
   Because this repo does not need backwards compatibility for current `render` syntax, the new `view`/frame surface should arrive as soon as the compiler can type-check and inspect it meaningfully.

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
- widening into proxy/raster/multi-representation rendering before the query-owned ray-solver path has been pushed hard
- guaranteeing native 1080p60 or 1080p120 for every arbitrary authored world in this roadmap

Those may come later. This roadmap exists to build the semantic real-time presentation core.

## Design Rules

Every phase in this RFC must follow these rules.

1. **Presentation remains a consumer of query families and portable kernels.**
   Do not build a second renderer architecture that bypasses the existing semantic substrate.

2. **Keep semantic contracts distinct from execution bindings and physical packing.**
   Logical attachment meaning and view/frame guarantees must not be conflated with helper names, kernel symbols, texture/storage choices, or compact packing schemes.

3. **Do not preserve legacy authored syntax for its own sake.**
   `render` and PPM helpers may remain as temporary scaffolding while they help prove parity, but the roadmap should remove or quarantine them whenever they obscure the canonical `view`/frame model.

4. **Treat `Camera.vertical_fov_degrees` as the canonical projection input going forward.**
   `view_scale` and related fields are short-lived compatibility quarantine only. New `view` code must not depend on them.

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

17. **Keep helper and kernel symbols behind execution binding resolution.**
    Presentation plans may expose binding ids and binding summaries for reporting, but raw helper names, kernel symbols, bridge exports, texture formats, and packing choices belong in execution binding or physical layout adapters.

18. **Keep ray-solver choices under query execution.**
    Presentation plans may report solver summaries, but they must continue to depend on query contracts such as `spatial.nearest.batch.world` rather than direct solver names.

19. **Keep planning ownership crisp.**
    Query plans own contract-level execution shape, ray solver plans own ray-shaped method selection, and presentation plans own frame/view orchestration.
    If a field or report starts crossing those boundaries, move it to the owning layer and expose only a summary upward.

20. **Keep WGSL generated.**
    Handwritten WGSL is limited to small prelude/runtime support code with CPU parity coverage. Query kernels, presentation passes, shading recipes, scene-specific logic, and solver choices should be generated from contracts, facts, IR, and execution bindings.

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

### Field Facts

**Field facts** are compiler-derived facts about field and shape nodes that can make query execution smarter without changing authored meaning.

Examples:

- support bounds and support class
- conservative Lipschitz bounds
- derivative or automatic-differentiation availability
- analytic primitive or subtree intersection availability
- transform and repetition behavior
- monotonicity or critical-point hints where available
- payload/provenance requirements for hit identity

Field facts belong to the query/compiler layer.
Presentation may benefit from them through query execution reports, but it must not own them.

### Ray Solver Plan

A **ray solver plan** is an internal query-engine execution strategy for ray-shaped contracts.

Examples:

- `spatial.nearest.world`
- `spatial.nearest.batch.world`
- `spatial.occluded.world`
- `spatial.occluded.batch.world`

The query contract states what the answer means.
The ray solver plan states how the engine will try to answer it.

The first solver plan may choose dense sphere tracing as a named fallback, but dense marching must not become presentation semantics.
As the compiler gains field facts, the same query contract can lower through better solver plans without changing the authored world or public contract.

### Solver Portfolio

A **solver portfolio** is the ordered set of legal solving methods available to a ray solver plan.

Potential methods include:

- analytic primitive or subtree intersection
- conservative hierarchy/support rejection
- Lipschitz-safe sphere tracing
- interval Newton or Krawczyk root isolation
- bisection or regula falsi fallback
- safeguarded Newton refinement
- affine-arithmetic or Taylor-model interval bounds
- repeat-aware ray traversal
- tile/packet ray solving
- neighbor/frame continuation
- dense marching fallback

Portfolio choice is an execution decision.
It must preserve the query contract's result schema, identity guarantees, conservatism/approximation policy, and backend support.

### Solver Observability

**Solver observability** reports whether the compiler actually reduced ray work.

Initial counters should include:

- analytic hits
- support or hierarchy rejections
- interval skips
- packet/tile rejections
- Newton refinements
- dense fallbacks
- average and maximum ray steps
- hit/miss counts
- certificate failures or fallback reasons when debug certificates are enabled

These counters should aggregate into query and frame reports so performance wins can be attributed to fewer semantic questions, better solver math, backend speed, or quality degradation.

### Solver Certificate

A **solver certificate** is optional debug/oracle metadata that explains why a ray result is valid or why a fallback was used.

Examples:

- a hit bracket
- a no-closer-root interval
- a conservative support rejection
- a Newton convergence record
- a dense fallback reason

Certificates are not required for every release-mode backend path, but the CPU oracle should be able to validate solver behavior and expose certificates in debug/testing modes when aggressive solvers are introduced.

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
- world-target batch queries
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

A **compatibility projection** is any legacy projection behavior quarantined while the new view model replaces the old preview path.

For this roadmap:

- `Camera.vertical_fov_degrees` becomes canonical
- `view_scale` and legacy `world_up` overrides do not belong to new authored `view` code
- reports should distinguish "legacy projection path active" from "authored compatibility override was present"
- compatibility projection should disappear from normal presentation execution once canonical view rays are executable

## Target First Real-Time Slice

To keep the roadmap honest, the first shippable real-time slice should be deliberately narrow.

Phase 17 has landed the internal bridge.
The real-time slice below is now the target, and Phase 18 should begin replacing compatibility scaffolding with canonical view and screen-lattice concepts.

The intended first slice is:

- one world capture
- one camera/view
- one key light plus fill/ambient compatibility
- primary visibility through `spatial.nearest.batch.world` and a query-owned `RaySolverPlan`
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

This roadmap has eight phases.

1. **Phase 17 — Canonical View/Frame Contracts And Internal Presentation Plans**
2. **Phase 18 — Clean Query Axes, Canonical View Surface, And Screen-Lattice Substrate**
3. **Phase 19 — Query-Owned Ray Solver Groundwork**
4. **Phase 20 — Primary Visibility And Semantic Frame Attachments**
5. **Phase 21 — Lighting, Surface/Participant Resolve, And First Real-Time Color Path**
6. **Phase 22 — Temporal Contracts, Motion, And History-Aware Resolve**
7. **Phase 23 — 60 FPS Quality Ladders, Adaptive Control, Ray Solver Acceleration, And Presentation Scheduling**
8. **Phase 24 — Presentation Tooling, Runtime Entry Points, Docs, And Legacy Removal**

The dependency structure is important:

- Phase 17 made presentation internally canonical while preserving old preview behavior as a bridge.
- Phase 18 removes the compatibility-shaped query/view assumptions and introduces the canonical `view` surface skeleton.
- Phase 19 introduces the query-engine ray-solver boundary so primary visibility does not bake dense marching into presentation semantics.
- Phase 20 produces the first real semantic frame attachments through the current ray solver plan.
- Phase 21 turns those attachments into a real color path.
- Phase 22 adds temporal continuity.
- Phase 23 makes the frame budget explicit and pursues 60 FPS systematically, with query-owned ray solver acceleration separated from presentation scheduling optimizations.
- Phase 24 finishes tooling, host entry points, documentation, and deletion of legacy presentation scaffolding after the architecture is real.

Execution horizon:

- Treat **Phase 18** as the next actionable project.
- Treat **Phases 19-24** as the intended direction, not an irreversible commitment.
- Phase 17 has been revalidated; its findings are recorded below.
- Re-open it again after Phase 21 to decide how much temporal, quality-control, ray-solver acceleration, and presentation scheduling work should land in this RFC versus a follow-up RFC.

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

Phase 17 revalidation asked these questions from the implemented plan model and CLI dump:

1. Does `PresentationPlan` naturally want `WorldBatch`, or should the query contract model split target and cardinality before more surfaces are added?
   Make this decision before any non-spatial world-batch family lands.
   If `WorldBatch` creates repeated special cases in plan construction, kernel validation, ABI/reporting, or execution dispatch, split target/cardinality first instead of letting the compatibility shape harden.
2. Which view/projection fields are canonical, and which are compatibility-only?
3. Which query families and contracts does the current preview path actually depend on?
4. Where should semantic acceleration artifacts attach: view contract, frame contract, presentation plan, query plan, or execution binding?
5. What exact `PrimaryHitAttachmentContract` schema must downstream resolve consume?
6. Which cost metrics can be collected immediately in Phase 18 without designing the full Phase 23 controller?
7. Which legacy PPM helper behavior is merely export plumbing, and which behavior still encodes semantic rendering choices?
8. Which pieces of `PresentationPlan` look presentation-specific, and which look like candidate generic query-program machinery to compare against a future collision/traversal observer?

The point of this revisit was to design Phase 18 around the first real compiler-owned presentation object instead of around a guessed shape.

### Phase 17 Revalidation Results

Phase 17 answered enough of these questions to change the rest of the roadmap.

1. **Split target and cardinality now.**
   `PresentationPlan` naturally wants query dependencies by canonical contract id.
   It does not want a hard-coded `WorldBatch` concept in the presentation layer.
   Before Phase 18 adds more surfaces, the query model should split:
   - target: `Capture` or `World`
   - cardinality: `Scalar` or `Batch`

   A combined `WorldBatch` enum member should be treated only as a rejected fallback unless the split proves too invasive.

2. **Canonical projection is FOV / camera basis / viewport / sample offset.**
   `Camera.vertical_fov_degrees` is the canonical projection input.
   `view_scale` and legacy authored `world_up` overrides are compatibility quarantine only.
   Reports should distinguish:
   - legacy compatibility path active
   - authored compatibility override present

3. **The current preview path depends on five query contracts.**
   The Phase 17 plan dump made the dependency set explicit:
   - `spatial.nearest.world`
   - `spatial.occluded.world`
   - `surface.sample.world`
   - `participants.radiance.world`
   - `participants.medium.world`

   Phase 18 should prioritize batch forms for this exact presentation question set.

4. **Semantic acceleration artifacts attach to the presentation plan, but derive from query/world contracts.**
   The presentation plan should own view/frame artifacts such as screen lattices, primary-hit attachments, and view-culling tables.
   Query plans should own primitive query execution contracts.
   Execution bindings should own backend/helper/kernel resolution.

5. **The primary-hit schema must be frozen before execution.**
   Phase 20 must not begin `PrimaryVisibilityPass` against an informal depth/normal idea.
   Phase 18 should name the primary-hit attachment schema and tie it to `Hit3` identity/provenance.

6. **Cost-shape metrics should start in Phase 18.**
   Do not wait for Phase 23.
   As soon as screen lattice and world-batch work exist, report sample count, world-batch item count, query contract ids, backend, dispatch dimensions, hit/miss counts, step counts, domain feature flags, and candidate/pruning counts when available.

7. **Legacy PPM is export plumbing plus compatibility shading, not the semantic frame model.**
   The PPM string export remains useful for debugging, but it should become a consumer of a color attachment.
   Current preview shading math may be ported as a compatibility recipe in Phase 21, not treated as the authoritative long-term lighting model.

8. **Candidate generic query-program machinery is visible but not ready to extract.**
   Pass graph, artifacts, validation, observability, binding summaries, query dependencies, and acceleration hooks may later become shared query-program machinery.
   Keep them presentation-owned until a second observer proves the overlap.

## Phase 18: Clean Query Axes, Canonical View Surface, And Screen-Lattice Substrate

### Goal

Give presentation a clean query-axis model, an early canonical `view` surface, and a real screen-space batch substrate.

### Why this is next

A real-time renderer cannot be built out of scalar world queries wrapped in nested loops forever.
The planner needs a world-batch surface and a screen lattice it can reason about.

Phase 17 also showed that `WorldBatch` should not become a permanent combined enum if the compiler can instead model query target and cardinality as separate axes.

Because backwards compatibility is not required, Phase 18 should introduce the canonical authored `view` skeleton now.
The first `view` surface does not need the full temporal/quality/color stack, but it should become the shape that future work targets.

World-batch alone is not the performance win.
The first implementation may include a straightforward dense baseline for parity, but the contract and observability work must leave room for semantic acceleration:

- candidate counts per sample/tile
- support-pruning decisions
- ray-step distributions
- domain-feature enablement
- backend dispatch sizes
- hit/miss density

If Phase 18 only produces a larger batch API with no way to see or reduce work, it has not set up the 60 FPS path.

### Workstream A: Query-Contract Surface Expansion

#### Task 18A0 — Split query surface into target and cardinality axes

**Description**

Replace the combined surface model with orthogonal query target and query cardinality concepts before world-batch surfaces multiply.

**Files**

- `compiler/query_contract/mod.rs`
- `compiler/query_plan/mod.rs`
- `compiler/kernel/ir.rs`
- `compiler/kernel/lower.rs`
- `compiler/kernel/validate.rs`
- `compiler/query_exec/spec.rs`
- tests in `compiler/tests/query_contract_registry.rs`
- tests in `compiler/tests/kernel.rs`

**Implementation notes**

Add explicit concepts equivalent to:

- query target: `Capture` or `World`
- query cardinality: `Scalar` or `Batch`

`QuerySurfaceKind` may remain as a compatibility adapter internally during the refactor, but public descriptors, reports, validation, and new contract ids should be able to express target/cardinality independently.

Make this split before `surface.sample.batch.world`, `participants.radiance.batch.world`, or `participants.medium.batch.world` land.
This avoids forcing presentation, collision, traversal, audio, and future observers through a combined enum that only happened to fit the first query surfaces.

Do **not** create a totally separate `WorldBatchQueryPlan` if `BatchQueryPlan` can be extended cleanly.

`BatchQueryPlan` already carries:

- `surface`
- `capture_kind`
- `item_kind`
- `result_kind`

Use that.
Avoid unnecessary type explosion.

**Acceptance criteria**

- Query target and cardinality are explicit in the contract model, reporting, and validation.
- Existing scalar/capture query descriptors preserve their meaning through the new axes.
- The existing plan/kernel structures can represent batch work over world targets.
- CLI/query-contract reporting shows target and cardinality cleanly.
- If any combined `WorldBatch` adapter remains, it is documented as internal compatibility glue and not the public shape for new descriptors.

#### Task 18A1 — Add world-batch query surfaces to the plan/kernel layers

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

Use the target/cardinality split from 18A0.
The resulting descriptors may still have ids like `spatial.nearest.batch.world`, but those ids should be generated from clean axes rather than hard-coded surface special cases.

Do **not** create a totally separate `WorldBatchQueryPlan` if `BatchQueryPlan` can be extended cleanly.

**Acceptance criteria**

- World-target batch queries are representable by the existing plan/kernel structures.
- Query descriptors expose target/cardinality explicitly.
- No new presentation-only query-plan type is introduced.
- CLI/query-contract reporting shows the new surfaces cleanly.

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

### Workstream B: Canonical View Surface, Screen-Lattice Records, And Projection Helpers

#### Task 18B0 — Add the canonical authored `view` declaration skeleton

**Description**

Introduce the canonical user-facing presentation declaration early, now that backwards compatibility is not required.

**Files**

- parser/HIR/typeck/lowering layers
- `compiler/presentation_plan/mod.rs`
- `compiler/bin/wrela/commands/shared.rs`
- tests in `compiler/tests/cli.rs`
- tests in new or existing presentation-plan modules

**Implementation notes**

The first `view` surface should be narrow and honest.
It only needs to express enough to type-check, build a presentation plan, and show the shape in `presentation-plan`.

Recommended initial shape:

```wr
view MainView(world: RegionCapture, camera: Camera) {
    viewport = viewport(width = 1280, height = 720)
    domain = scene_domain(world = world)
    outputs = frame_outputs(color = true, depth = true, normal = true)
    lighting = key_light(...)
}
```

Prefer typed helpers over bespoke grammar wherever practical:

- `viewport(...)`
- `frame_outputs(...)`
- `key_light(...)`
- later `realtime_quality(...)`
- later `temporal_history(...)`

This task does not need to execute a full color frame.
It does need to make `view` the authored target for new presentation work.

`render` may remain temporarily for tests, but new presentation features should not be added only to `render`.

**Acceptance criteria**

- A canonical `view` declaration parses, lowers, and type-checks.
- `PresentationPlan` can be built from the first `view` declaration shape.
- `presentation-plan` can dump plans for `view` declarations.
- New `view` projection uses canonical camera FOV and viewport state, not `view_scale`.
- `render` is documented as temporary scaffolding or rejected for new presentation features.

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
Do not let Phase 20 implement `PrimaryVisibilityPass` against an informal "enough identity" concept.

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

This is intentionally earlier than the Phase 23 adaptive controller.
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

- The query-family system expresses target and cardinality as clean axes.
- The query-family system can express world-target batch work.
- The first canonical authored `view` declaration exists and can produce a presentation plan dump.
- Presentation plans can represent a screen lattice explicitly.
- The engine can generate rays from canonical view state.
- CPU/WGSL world-batch parity exists for the required presentation question set.
- Screen-work cost-shape reports can distinguish dense execution from semantically pruned execution.
- The first primary-hit attachment schema is named before Phase 20 execution work begins.

## Phase 19: Query-Owned Ray Solver Groundwork

### Goal

Introduce the query-engine-owned ray solver layer before primary visibility becomes executable.

This phase exists because Phase 18 is already the clean query/view/screen-lattice milestone.
Do not rewrite Phase 18 to include deep solver work.
Instead, use Phase 19 to make sure Phase 20 primary visibility consumes `spatial.nearest.batch.world` through a query-owned solver plan, with dense marching as a named fallback rather than a presentation assumption.

### Why this is next

Primary visibility is the first place where performance pressure becomes unavoidable.
If the first primary pass bakes dense sphere tracing directly into presentation execution, the engine will have to untangle that assumption later.

The right boundary is:

`PresentationPlan -> spatial.nearest.batch.world -> RaySolverPlan -> CPU/WGSL generated execution`

Presentation owns the view, frame attachments, pass graph, and exports.
The query engine owns how a ray-shaped query is solved.

This phase should stay narrow.
It is not the full frontier math project.
Its job is to create the internal slots, reports, and first CPU-backed solver behaviors that later phases can optimize aggressively.

No solver method graduates from exploratory status unless it improves primary-visibility cost shape on representative scenes and preserves query-contract semantics under CPU oracle tests.

### Workstream A: Query-Engine Solver Model

#### Task 19A1 — Add `FieldFacts` skeletons for field and shape nodes

**Description**

Add a query/compiler-side fact model that can describe what the compiler knows about authored field and shape nodes.

**Files**

- `compiler/query_plan/mod.rs` or a new query-engine facts module
- `compiler/scene_ir/mod.rs`
- `compiler/kernel/*` if fact ids need to survive lowering
- tests in `compiler/tests/phase9_query_plan.rs`, `compiler/tests/kernel.rs`, or a new solver-plan test file

**Implementation notes**

Start with facts that can be derived from existing scene/support structure without requiring advanced math:

- support class and conservative bounds availability
- whether authored support can be used for conservative pruning
- primitive identity where the compiler already knows it
- transform/repetition summary
- whether local hit identity/provenance must be preserved
- whether derivative, Lipschitz, analytic, interval, or repeat-aware facts are currently unavailable

It is acceptable for most advanced fact slots to start as `Unknown` or `Unavailable`.
The important part is to make absence explicit so reports can distinguish:

- solver could not optimize because facts were unavailable
- solver chose dense fallback despite facts
- solver used facts to skip or refine work

**Acceptance criteria**

- A `FieldFacts` or equivalent model exists in the query/compiler layer.
- The model can represent support, primitive, transform, repetition, derivative, Lipschitz, analytic, interval, and provenance-requirement facts.
- Existing query planning can expose a fact summary without changing public query contracts.
- Reports/tests can show unavailable facts explicitly.

#### Task 19A2 — Add `RaySolverPlan` and `SolverPortfolio` for ray-shaped spatial contracts

**Description**

Add an internal solver-plan layer for ray-shaped query contracts.

**Files**

- `compiler/query_plan/mod.rs` or a new `compiler/query_solver/mod.rs`
- `compiler/kernel/ir.rs`
- `compiler/kernel/lower.rs`
- `compiler/kernel/validate.rs`
- tests in a new `compiler/tests/ray_solver_plan.rs` if useful

**Implementation notes**

The first solver plan should target:

- `spatial.nearest.world`
- `spatial.nearest.batch.world`
- `spatial.occluded.world`
- `spatial.occluded.batch.world`

Recommended first internal types:

- `RaySolverPlan`
- `RaySolverPortfolio`
- `RaySolverMethod`
- `RaySolverFallback`
- `RaySolverCorrectnessPolicy`

Initial methods may include only:

- dense sphere tracing
- support-bound candidate rejection where already available
- exact dense fallback

But the enum/model should leave room for:

- analytic primitive intersections
- hierarchy/support rejection
- Lipschitz-safe stepping
- interval Newton or Krawczyk root isolation
- safeguarded Newton refinement
- affine arithmetic or Taylor-model bounds
- repeat-aware traversal
- tile/packet ray solving
- neighbor/frame continuation

Do not expose these solver names as authored syntax.
Do not make presentation passes depend on concrete solver variants.

**Acceptance criteria**

- Ray-shaped spatial contracts can resolve to a `RaySolverPlan`.
- Dense marching is represented as an explicit fallback, not as implicit presentation behavior.
- The solver plan records which contract semantics it must preserve, including `Hit3` identity/provenance when required.
- Query/presentation reporting can summarize the selected solver without making it a public contract.

#### Task 19A3 — Add solver observability and optional certificate shapes

**Description**

Make solver performance and fallback behavior visible before advanced solvers land.

**Files**

- `compiler/query_exec/mod.rs`
- `compiler/query_exec/cost.rs`
- `compiler/query_exec/*`
- CLI/reporting tests

**Implementation notes**

Add counters or report fields for:

- solver plan id or summary
- analytic hits
- support/hierarchy rejections
- interval skips
- packet/tile rejections
- Newton refinements
- dense fallback count
- fallback reasons
- average and maximum ray steps
- certificate failures when debug certificates are enabled

The first implementation may report zeros for advanced counters.
That is still valuable because it fixes the report surface before optimization work starts.

Add a debug-only or test-only certificate shape if useful.
It may start shallow:

- solver method used
- hit/miss
- hit bracket if known
- no-closer-hit proof unavailable/available
- fallback reason

**Acceptance criteria**

- Query execution reports can distinguish dense fallback from solver-assisted execution.
- Solver counters aggregate into semantic cost reports.
- A debug/test certificate shape exists or the report explicitly reserves the fields needed for one.
- Tests cover report shape for dense fallback and at least one support-pruned ray case.

### Workstream B: First Solver Improvements

#### Task 19B1 — Add analytic primitive hooks for the simplest field shapes

**Description**

Teach the solver model how to represent analytic ray intersections for primitive/subtree cases that the compiler can recognize safely.

**Files**

- `compiler/scene_ir/mod.rs`
- `compiler/query_plan/mod.rs` or solver module
- `compiler/query_exec/cpu.rs`
- tests in ray-solver or query-exec suites

**Implementation notes**

Start deliberately small:

- sphere
- plane
- slab/box where existing primitive semantics make this safe
- transformed primitive with affine transform hoisting when easy

The first CPU implementation may use analytic hits only as a candidate accelerator and then verify/refine against the existing CPU query semantics.
Preserve hit identity, feature id, instance id, repeat id, root shape id, and payload requirements.

Do not attempt to solve all CSG analytically in this task.

**Acceptance criteria**

- The solver plan can record analytic primitive availability.
- CPU execution can use at least one analytic primitive path under a query contract.
- Dense fallback remains available and tested.
- Analytic and dense CPU paths agree within the declared tolerance/identity contract for deterministic fixtures.

#### Task 19B2 — Add Lipschitz-safe stepping and adaptive hit epsilon scaffolding

**Description**

Introduce the first math facts needed to make marching both safer and faster without changing semantics.

**Files**

- query facts/solver modules
- `compiler/query_exec/cpu.rs`
- tests in query-exec or solver suites

**Implementation notes**

Represent conservative Lipschitz status per node:

- exact known
- conservative known
- unknown

Initial propagation can be shallow.
For example:

- rigid transforms preserve the bound
- uniform scale adjusts it
- unions/intersections take a conservative max
- unknown/displacement/opaque nodes force fallback behavior

Add adaptive epsilon scaffolding based on:

- ray distance
- field scale or transform scale when known
- quality/debug mode defaults

The first execution path may only report these values and use them in narrow safe cases.

**Acceptance criteria**

- Solver facts can represent Lipschitz availability and unknowns.
- CPU solver can use a conservative Lipschitz step where safe.
- Unknown facts fall back conservatively.
- Tests cover a safe known case and an unknown/fallback case.

#### Task 19B3 — Add derivative/refinement hooks without making them mandatory

**Description**

Prepare the solver layer for gradient-based normals and safeguarded Newton refinement while keeping dense fallback authoritative.

**Files**

- query facts/solver modules
- PIR/MIR or portable-function lowering if derivatives are represented there
- `compiler/query_exec/cpu.rs`
- tests

**Implementation notes**

This task does not need full automatic differentiation.
It should establish the contract shape for:

- derivative available/unavailable
- gradient source
- refinement method
- fallback reason when Newton/refinement is not legal

If a small derivative-backed primitive path is easy, add it.
Otherwise, keep this as a validated planning/reporting scaffold and let Phase 23 implement deeper derivative solvers.

**Acceptance criteria**

- Solver planning can represent derivative/refinement availability.
- Reports explain when Newton/refinement was unavailable or skipped.
- Dense fallback remains authoritative.
- No presentation pass depends directly on derivative/refinement details.

### Workstream C: Integration With Presentation And Backends

#### Task 19C1 — Route ray-shaped world queries through `RaySolverPlan`

**Description**

Ensure primary visibility and occlusion-capable world-batch queries execute through the query-owned solver boundary.

**Files**

- `compiler/query_exec/mod.rs`
- `compiler/query_exec/cpu.rs`
- `compiler/query_exec/vgpu.rs`
- `compiler/query_exec/wgsl.rs`
- `compiler/presentation_plan/mod.rs`
- tests in query-exec and presentation-plan suites

**Implementation notes**

Presentation should still request query contracts:

- `spatial.nearest.batch.world`
- `spatial.occluded.batch.world`

The query engine should decide which `RaySolverPlan` executes them.

For WGSL, it is acceptable for Phase 19 to keep generated dense fallback kernels while exposing the solver summary and fallback counters.
Do not add handwritten presentation WGSL.

**Acceptance criteria**

- `spatial.nearest.world` and `spatial.nearest.batch.world` route through the solver boundary on CPU.
- WGSL reports dense fallback or generated solver fallback explicitly if advanced solver lowering is not ready.
- Presentation plans report solver summaries only as diagnostics, not as pass semantics.
- CPU/WGSL parity remains anchored to the query contract, not to a specific solver method.

#### Task 19C2 — Add solver-specific CPU oracle tests

**Description**

Create tests that compare solver-assisted execution with the trusted CPU dense semantics.

**Files**

- new or existing query-exec/solver tests
- deterministic fixtures

**Implementation notes**

Cover:

- dense fallback
- support-pruned ray
- at least one analytic primitive hook if implemented
- miss behavior
- hit identity/provenance preservation
- occlusion early-exit semantics where available

The goal is not to prove every future solver now.
The goal is to establish the pattern every future solver must follow.

**Acceptance criteria**

- Solver-assisted results are checked against CPU dense/oracle behavior.
- Identity/provenance preservation is tested for hits.
- Misses and fallback reasons are tested.
- Tests fail if presentation bypasses the query solver boundary for primary ray contracts.

#### Task 19C3 — Keep generated WGSL as the backend path

**Description**

Make the WGSL story explicit before solver work begins to lower to GPU.

**Files**

- `compiler/query_exec/wgsl/codegen.rs`
- `compiler/query_exec/wgsl/prelude.wgsl`
- tests in WGSL/query-exec suites

**Implementation notes**

WGSL should remain generated from:

- query contracts
- field facts
- ray solver plans
- portable ABI records
- kernel IR / PIR as appropriate

The handwritten prelude may contain small reusable math/runtime functions.
It must not become the place where presentation-specific solver or shading behavior lives.

**Acceptance criteria**

- Any new WGSL-facing solver behavior is generated or represented as small prelude/runtime support.
- Tests make generated WGSL identify its solver/fallback path in reports.
- No authored or presentation-specific handwritten WGSL shader path is added.

### Phase 19 Exit Criteria

- The query engine has an internal ray-solver boundary for ray-shaped spatial contracts.
- Dense sphere tracing/marching is a named fallback strategy, not a presentation assumption.
- Field facts can describe support, primitive, transform, repetition, Lipschitz, derivative, analytic, interval, and provenance-related availability.
- Solver observability reports dense fallback versus solver-assisted execution.
- CPU oracle tests establish the pattern for validating future solver methods.
- Presentation plans continue to depend on query contracts, not solver method names.
- WGSL remains generated backend output.

## Phase 20: Primary Visibility And Semantic Frame Attachments

### Goal

Produce the first real frame attachments from a view by tracing primary visibility through the new world-batch substrate and the query-engine ray solver boundary.

### Why this is next

The first real-time frame milestone is not final beauty.
It is a **semantic primary pass** that materializes stable per-sample world meaning.

That is the presentation equivalent of a semantic G-buffer.
It is also the first pass that can prove whether the compiler is avoiding work in the view.
Primary visibility metrics should make candidate reduction and ray-step behavior visible from the start.

The primary pass must not own ray marching semantics.
It requests `spatial.nearest.batch.world`.
The query engine selects the current `RaySolverPlan`, which may still choose dense marching as a fallback.

### Workstream A: Attachment Contracts And Resources

#### Task 20A1 — Add semantic attachment contracts to the presentation plan

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

#### Task 20A2 — Add the first physical attachment allocator using linear buffers

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

#### Task 20B1 — Implement `PrimaryVisibilityPass`

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
4. run `spatial.nearest.batch.world` through the current `RaySolverPlan`
5. materialize a primary-hit attachment
6. optionally derive depth/world-normal attachments

Keep this pass thin.
The primary-hit attachment is the semantic source of truth for this pass; depth and world-normal are derived attachments.
The pass should report dense candidate count versus pruned candidate count whenever a semantic acceleration artifact is active.
It should also report the selected solver summary, solver-assisted counters, and dense fallback counts exposed by the query engine.

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
- Primary visibility reports the selected ray solver summary and dense fallback count.
- Presentation execution does not duplicate ray solver semantics outside the query engine.

#### Task 20B2 — Add debug export for primary attachments

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

#### Task 20C1 — Add WGSL execution for `PrimaryVisibilityPass`

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
- execute world nearest batch work through generated query/ray-solver code or an explicitly reported generated dense fallback
- write primary-hit/depth/normal buffers

Parity should be checked on small frame sizes first.

**Acceptance criteria**

- WGSL execution exists for primary visibility.
- CPU/WGSL parity tests exist for primary-hit, depth, and normal attachments.
- Attachment dimensions and miss semantics match.
- Any WGSL ray-solver behavior is generated from query/solver plans or lives in small reusable prelude support, not in handwritten presentation shaders.

### Phase 20 Exit Criteria

- Presentation plans can declare and allocate semantic attachments.
- The engine can produce primary-hit/depth/normal attachments from a view through `spatial.nearest.batch.world` and the current query-owned ray solver plan.
- CPU and WGSL agree on the primary pass for small frames.
- The repo has a practical debugging path for frame attachments.

## Phase 21: Lighting, Surface/Participant Resolve, And First Real-Time Color Path

### Goal

Turn semantic primary visibility into the first full color frame path using explicit resolve and shading passes.

### Why this is next

Primary visibility alone is the architectural spine.
This phase turns it into something visually real while still keeping the pass graph clean and explicit.

### Workstream A: Lighting Contracts

#### Task 21A1 — Extend `LightingContract` into typed presentation lighting inputs

**Description**

Mature the shallow Phase 17 lighting contract into typed presentation-time lighting inputs.

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
Because backwards compatibility is not required, this task should target canonical `view` declarations first and only keep `render` mapping while it remains useful as a temporary test scaffold.

**Acceptance criteria**

- `LightingContract` carries typed lighting inputs for the first color path.
- Canonical `view` declarations lower lighting into the presentation plan.
- Any remaining `render` lighting mapping is explicitly temporary.
- Lighting inputs are now part of the presentation plan, not one-off helper argument plumbing.

### Workstream B: Resolve Passes

#### Task 21B1 — Add `SurfaceResolvePass`

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

#### Task 21B2 — Add `ParticipantsResolvePass`

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

#### Task 21C1 — Add `ShadePrimaryPass` and `CompositeColorPass`

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

#### Task 21C2 — Rebase PPM/debug export on top of the new presentation pipeline and retire legacy render helpers

**Description**

Make PPM/debug export consume explicit color attachments from the presentation pipeline rather than the legacy helper island.

**Files**

- `compiler/mir/lower.rs`
- `compiler/tests/preview_project.rs`
- `compiler/tests/codegen_v2.rs`

**Implementation notes**

The desired shape is:

`view decl -> presentation plan -> color attachment -> PPM/debug export`

At the end of this task:

- `__wr_render_scene_color_capture` should be gone, or
- it should be a test-only thin wrapper over the new pass graph with a removal ticket

Do not preserve old user-facing syntax for its own sake.
If keeping `render` makes tests cheaper during this task, keep it as scaffolding only.

**Acceptance criteria**

- PPM/debug export runs through the presentation plan.
- CPU/WGSL preview tolerance remains acceptable.
- The old render-helper island is no longer the conceptual or executable source of truth for new presentation paths.
- Any remaining legacy helper wrapper is documented as temporary and covered by a removal plan.

### Phase 21 Exit Criteria

- The engine can produce a final color frame from explicit presentation passes.
- Preview output is now a consumer of presentation plans.
- Lighting and participant work are explicit and schedulable.
- CPU/WGSL parity exists for the first full color path.

### Phase 21 Revalidation Results

Phase 21 implementation answered the revalidation request from the earlier roadmap revision.

What the code now taught the team:

- The first explicit color path is stable enough that later phases should treat `PresentationPlan` execution, not legacy preview helpers, as the semantic source of truth for frame production.
- Typed lighting inputs, explicit surface/participant resolve, and attachment-backed composite passes are already the right contract boundary; later work should optimize or extend those contracts instead of re-opening their basic shape.
- Preview and debug export now belong on top of the same presentation execution path, so Phase 24 should focus on removing the remaining legacy authored `render` scaffolding and host helper wrappers rather than inventing another preview-specific runtime branch.
- Phase 22 should stay tightly scoped to temporal state, motion, and history-aware resolve. It should not absorb runtime entrypoint cleanup or compatibility-wrapper retirement now that the first color path already proved those are separate concerns.
- Phase 23 remains the right place for broader acceleration, scheduling, and quality-control work because Phase 21 showed the main remaining questions are about cost and observability, not about whether the frame/pass contracts are viable.

## Phase 22: Temporal Contracts, Motion, And History-Aware Resolve

### Goal

Add explicit temporal state and history reuse so presentation becomes stable under motion and gains the first major 60 FPS lever.

This phase is directional until the Phase 21 color path exists.
Do not start it by designing a large temporal framework in the abstract; start it only after primary visibility, resolve, and color attachments can provide real inputs and real failure cases.

### Why this is next

Real-time field rendering without temporal semantics will fight noise, shimmer, and brute-force cost.
This phase turns temporal reuse into a contract instead of an accident.

### Workstream A: Temporal Contracts

#### Task 22A1 — Mature `TemporalContract` and history slot semantics

**Description**

Mature the shallow Phase 17 temporal contract placeholder into explicit history/reuse semantics.

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

- The typed temporal contract has real history/reuse/invalidation semantics.
- Frame contracts can declare history-backed attachments.
- Validation catches impossible history combinations.

#### Task 22A2 — Extend `FrameState` and attachments for motion/history compatibility

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

#### Task 22B1 — Implement `MotionResolvePass`

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

#### Task 22B2 — Implement `TemporalResolvePass`

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

#### Task 22B3 — Add temporal hooks for ray-solver continuation

**Description**

Expose enough temporal hit state for later solver continuation without making continuation part of the color resolve itself.

**Files**

- `compiler/presentation_contract/mod.rs`
- `compiler/presentation_exec/*`
- query solver/reporting modules if the continuation seed is represented there
- tests in `compiler/tests/presentation_exec.rs`

**Implementation notes**

The temporal presentation path should be able to provide the query engine with optional continuation seeds:

- previous hit identity
- previous hit distance or bracket when available
- previous screen/sample coordinate
- motion vector or reprojection mapping
- invalidation/disocclusion status

This is not the full continuation solver.
The query engine will own that in Phase 23.
This task only ensures the frame/history model does not erase the information a solver needs.

**Acceptance criteria**

- Presentation can preserve optional hit-continuation seed data across frames.
- Invalid seeds are marked explicitly.
- Query reports can say whether continuation data was available, consumed, rejected, or unavailable.
- Color temporal resolve remains separate from query-solver continuation.

### Workstream C: Temporal Tooling And Tests

#### Task 22C1 — Add temporal stability tests and sample content

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

### Phase 22 Exit Criteria

- Frame contracts can declare temporal reuse.
- The engine can produce motion vectors and reproject history.
- CPU and WGSL temporal resolve agree within declared tolerances.
- Temporal stability is measurable in tests.

## Phase 23: 60 FPS Quality Ladders, Adaptive Control, Ray Solver Acceleration, And Presentation Scheduling

### Goal

Make “real-time” an explicit contract and mature the structural tools the engine needs to chase 60 FPS without abandoning semantics.

This phase is also a checkpoint phase.
After Phase 21, decide whether quality control and acceleration should remain in this RFC or become a focused follow-up roadmap based on measured presentation cost.
Acceleration begins earlier through Phase 18 cost-shape instrumentation and Phase 19 ray-solver groundwork.
Phase 23 is where those ideas become a coherent quality-control and optimization system.

Expect this phase to split unless the Phase 21 color path shows the bottlenecks are simple.
Likely split points are:

- **quality/control**: contracts, degradation ladders, observability, adaptive controller
- **query-owned ray solver acceleration**: hierarchy rejection, analytic solving, interval/refinement math, packet solving, repeat-aware traversal
- **presentation scheduling**: hit compaction, physical packing, half-resolution execution, attachment bandwidth

The roadmap keeps them together directionally because they affect each other, but implementation should not force one giant phase if measured bottlenecks argue for a focused follow-up RFC.

### Why this is next

At this point the pipeline exists.
Now the repo must stop hoping that faster hardware or nicer math alone will solve frame time.

This phase treats 60 FPS as a **control problem**:

- define allowed trade-offs
- measure actual cost
- adapt legally
- optimize the query solvers and pass graph structurally

### Workstream A: Quality Contracts

#### Task 23A1 — Add `RealtimeQualityContract`

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

#### Task 23A2 — Add named quality ladders and degradation order

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

#### Task 23B1 — Add frame-level observability and cost accounting

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

#### Task 23B2 — Add an adaptive controller for target FPS

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

### Workstream C: Query-Owned Ray Solver Acceleration

#### Task 23C1 — Add hierarchical conservative ray culling

**Description**

Exploit the existing semantic support infrastructure and `FieldFacts` model to skip field/shape subtrees that cannot affect a ray before the current best hit.

**Files**

- query facts/solver modules
- `compiler/query_plan/mod.rs`
- `compiler/scene_ir/mod.rs`
- `compiler/query_exec/*`
- tests/benchmarks

**Implementation notes**

This is where `support.summary` starts paying direct ray-solver dividends.

The solver should be able to ask, for a ray or ray interval:

- can this subtree produce a hit before `current_best_t`?
- can this subtree be skipped conservatively?
- did this skip require exact, conservative, approximate, or fallback policy?

Start with conservative support lower bounds and existing support summaries.
Later implementations may replace linear candidate lists with explicit hierarchical traversal.

The culling artifact should state its correctness policy explicitly:

- conservative if it may include false positives but must not drop valid hits
- approximate if it may trade correctness under a named quality mode
- backend-specific only if its semantic source and fallback are clear

**Acceptance criteria**

- Ray solver planning can derive conservative culling from scene support data.
- The artifact is used to prune ray candidate work before exact field evaluation.
- Benchmarks show measurable candidate reduction.
- Correctness policy is explicit and covered by CPU oracle tests.

#### Task 23C2 — Add analytic primitive/subtree solvers

**Description**

Replace dense marching with analytic solving for primitive or simple subtree cases the compiler can recognize safely.

**Files**

- query facts/solver modules
- `compiler/query_exec/cpu.rs`
- `compiler/query_exec/wgsl/codegen.rs`
- tests/benchmarks

**Implementation notes**

Extend the Phase 19 analytic hooks into real solver methods.

Prioritize:

- sphere
- plane
- slab/box
- capsule/cylinder/cone where current primitive semantics are clear
- transformed primitives with ray-space transform hoisting
- simple extrusions or profiles only when correctness is obvious

Analytic hits must still preserve query identity/provenance contracts.
When analytic solving cannot prove the required result, fall back to dense or interval/refinement methods.

**Acceptance criteria**

- Analytic solver methods exist for at least the first representative primitive set.
- CPU oracle tests compare analytic and dense solver results.
- WGSL execution is generated from solver plans or small prelude support.
- Fallback behavior is explicit in reports.

#### Task 23C3 — Add derivative, Lipschitz, and safeguarded Newton refinement

**Description**

Use field math to reduce late-stage marching work and improve hit/normal stability.

**Files**

- query facts/solver modules
- PIR/MIR lowering if automatic differentiation is introduced there
- `compiler/query_exec/cpu.rs`
- `compiler/query_exec/wgsl/codegen.rs`
- tests/benchmarks

**Implementation notes**

Add or mature:

- conservative Lipschitz propagation
- derivative availability
- generated gradients where supported
- safeguarded Newton or bisection/Newton hybrid refinement
- adaptive epsilon from ray distance, field scale, gradient magnitude, and pixel footprint where available

This should replace finite-difference-style work where the contract allows it and keep dense fallback for unknown or unstable fields.

**Acceptance criteria**

- Solver methods can use derivatives and Lipschitz facts where available.
- Unknown or unstable derivative facts fall back conservatively.
- Normals/refinement agree with CPU oracle within declared tolerances.
- Reports show refinement counts, failures, and fallback reasons.

#### Task 23C4 — Add interval, affine, or Taylor-bound root isolation prototypes

**Description**

Prototype rigorous interval-style ray solving for expensive or uncertain field regions.

**Files**

- query solver modules
- `compiler/query_exec/cpu.rs`
- optional generated WGSL only after CPU behavior is proven
- tests/benchmarks

**Implementation notes**

Start CPU-first.
Good first targets:

- interval Newton or Krawczyk contraction over ray intervals
- affine arithmetic bounds for correlated expressions
- Taylor-model bounds for smooth subtrees
- Bernstein-style polynomial bounds only if the expression lowering makes it practical

This task is allowed to remain experimental inside the query engine, but it must not change public query semantics.

**Acceptance criteria**

- At least one interval/root-isolation prototype exists behind an internal solver method.
- CPU tests show correct hit/miss behavior against dense/oracle cases.
- Reports expose interval skips, uncertain subdivisions, and fallback reasons.
- WGSL lowering is optional until CPU behavior is convincing.

#### Task 23C5 — Add repeat-aware and ray-space traversal

**Description**

Use authored transform and repetition semantics to avoid evaluating repeated structures as opaque modulo-heavy distance code.

**Files**

- `compiler/scene_ir/mod.rs`
- query facts/solver modules
- `compiler/query_exec/*`
- tests/benchmarks

**Implementation notes**

For repeat and transform nodes, the solver should be able to:

- hoist static transforms onto the ray
- traverse plausible repeated cells along the ray
- skip empty or bounded cells
- preserve `repeat_id`, `instance_id`, and provenance

Start with the repeat forms already represented in SceneIR and only add traversal where identity preservation is clear.

**Acceptance criteria**

- Solver planning can identify repeat-aware opportunities.
- At least one repeat-heavy fixture shows reduced candidate/step work.
- Repeat identity/provenance remains stable.
- Fallback to dense repeat evaluation remains available.

#### Task 23C6 — Add tile/packet ray solving and continuation hooks

**Description**

Exploit coherence across neighboring rays and frames without moving solver semantics into presentation.

**Files**

- query solver modules
- `compiler/query_exec/*`
- `compiler/presentation_exec/*` only for passing screen-tile/frame-history context into query execution
- tests/benchmarks

**Implementation notes**

The solver may receive query context that describes:

- screen tile/ray packet membership
- shared ray cone or interval bounds
- previous-frame hit seed when temporal contracts allow it
- continuation validity policy

The query engine still owns the solver method.
Presentation supplies view/frame context and consumes the resulting attachments.

**Acceptance criteria**

- Solver plans can represent packet/tile and continuation methods.
- CPU oracle tests cover fallback when packet/continuation assumptions fail.
- Benchmarks show whether packet/continuation work reduces per-ray candidate or step counts.
- Reports distinguish packet rejection, continuation success, continuation correction, and fallback.

#### Task 23C7 — Add ray-footprint, monotonicity, and profile-guided solver specialization

**Description**

Use deeper compiler analysis and measured hot paths to specialize generated ray solvers without widening beyond the ray/query backend.

**Files**

- query facts/solver modules
- `compiler/query_exec/cost.rs`
- `compiler/query_exec/*`
- `compiler/query_exec/wgsl/codegen.rs`
- benchmarks/tests

**Implementation notes**

This task groups the frontier optimizations that should be explored after the core solver methods are real:

- pixel-footprint or cone/beam-aware solving
- monotonicity and critical-point interval splitting for `f(ray(t))`
- profile-guided solver selection and branch ordering
- common-subexpression elimination across field, gradient, support, and transform evaluation
- contract-legal mixed precision for coarse bounds, far-field checks, or approximate ranking
- generated kernel layout tuning for register pressure, inlining, workgroup sizing, and divergence

Every optimization must report whether it changed semantic work, numerical precision, backend scheduling, or quality policy.

**Acceptance criteria**

- Solver reports can attribute gains to footprint/cone, monotonicity, profile-guided layout, CSE, mixed precision, or backend kernel tuning.
- CPU oracle tests cover any optimization that can affect hit/miss or identity.
- Mixed precision is only used under an explicit correctness or quality policy.
- Generated WGSL remains the backend path; tuning does not introduce handwritten presentation shaders.

### Workstream D: Presentation Scheduling And Physical Layout

#### Task 23D1 — Add tile/cluster culling artifacts derived from semantic support data

**Description**

Map coarse screen regions to plausible shape candidate sets so presentation can schedule less screen work before invoking exact ray solvers.

**Files**

- `compiler/presentation_plan/mod.rs`
- `compiler/query_plan/mod.rs` if new artifact contracts are needed
- `compiler/scene_ir/mod.rs`
- `compiler/presentation_exec/*`
- tests/benchmarks

**Implementation notes**

This is the presentation-side scheduling counterpart to query-owned ray culling.

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

#### Task 23D2 — Add hit compaction for expensive passes

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

This is one of the highest-value structural optimizations in the presentation scheduler.

**Acceptance criteria**

- Hit compaction exists as an explicit presentation optimization.
- Later passes can consume compacted work lists.
- Frame outputs remain correct after remapping to full-frame coordinates.

#### Task 23D3 — Separate semantic attachments from physical packing and half-resolution execution

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

### Workstream E: Benchmarks And Gates

#### Task 23E1 — Add a real-time presentation benchmark suite

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

### Phase 23 Exit Criteria

- Presentation has typed quality contracts and degradation ladders.
- The engine can adapt quality legally against a frame target.
- Query-owned ray solver acceleration exists beyond dense fallback for at least representative scenes.
- Support-driven ray and tile/cluster pruning exists.
- Hit compaction exists for expensive passes.
- The repo has a meaningful real-time presentation benchmark suite.
- Reports identify whether 60/120 FPS attempts succeeded natively, through reconstruction, through quality degradation, or not yet.
- If quality/control and acceleration split into separate projects, each split has its own acceptance criteria and independent review gate.

## Phase 24: Presentation Tooling, Runtime Entry Points, Docs, And Legacy Removal

### Goal

Finish the view/frame system as an engine surface: host-facing entry points, reporting, documentation, executable samples, and removal of legacy presentation scaffolding.

### Why this is last

The authoritative `view` surface now lands early in Phase 18.
This phase is no longer the first authored cut.
Instead, it is the cleanup and productization phase after:

- the internal contracts exist
- the pass graph is real
- primary visibility and color attachments work
- temporal and quality contracts have real semantics
- tooling can explain the frame

Because the repo does not need backwards compatibility, Phase 24 should delete old presentation paths rather than preserve them.

### Workstream A: Canonical Surface Completion

#### Task 24A1 — Complete the authored `view` surface around the mature contracts

**Description**

Bring the Phase 18 `view` skeleton up to the full real-time slice now that color, temporal, and quality execution exist.

**Files**

- parser/HIR/typeck/lowering layers
- spec/docs/examples
- tests in spec, CLI, and presentation suites

**Implementation notes**

The mature surface should remain regular and typed:

```wr
view MainView(world: RegionCapture, camera: Camera) {
    domain = scene_domain(world = world)
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

Do not reintroduce broad bespoke syntax if typed helpers can express the same thing.

**Acceptance criteria**

- The authored `view` surface covers the first real-time slice.
- The surface matches the internal contract model.
- Typed helpers cover viewport, quality, outputs, history, and lighting.
- The spec suite covers the authored surface sufficiently.

#### Task 24A2 — Remove or hard-error legacy `render` presentation syntax

**Description**

Delete the old `render` presentation surface, or turn it into a clear hard error with a migration diagnostic if deletion is too disruptive for parser recovery.

**Files**

- parser/HIR/typeck/lowering layers
- `compiler/tests/preview_project.rs`
- CLI/project e2e tests
- spec/docs

**Implementation notes**

Do not maintain compatibility sugar.
The final engine should have one authoritative presentation model.

PPM can remain as an export/debug format, but not as a reason to preserve `render` as an authored presentation concept.

**Acceptance criteria**

- New tests and samples use `view`, not `render`.
- Legacy `render` no longer lowers to presentation execution.
- If `render` is still recognized syntactically, it produces an explicit diagnostic that points to `view`.
- No final roadmap acceptance criterion depends on preserving old preview content.

### Workstream B: Runtime Entry Points And Attachment Export

#### Task 24B1 — Add host/runtime entry points for named views

**Description**

Expose compiled views as first-class host-facing entry points so the engine runtime can evaluate them without going through preview-only paths.

**Files**

- CLI/runtime bridging code
- `compiler/bin/wrela/*`
- potential runtime adapter code
- tests in CLI/project e2e suites

**Implementation notes**

Examples:

- `wrela preview path/to/project --view MainView`
- `wrela frame path/to/project --view MainView --attachment color`
- runtime APIs that request a named view/frame output bundle

Keep this as a thin host boundary.
Do not build a windowing/swapchain subsystem into the compiler.

**Acceptance criteria**

- Named views have clear host/runtime entry points.
- CLI and test harnesses can evaluate specific views and exported attachments.
- Entry points return typed frame-output bundles rather than preview-only strings.

#### Task 24B2 — Complete attachment/export/debug tooling

**Description**

Make frame outputs inspectable without coupling the system to PPM.

**Files**

- CLI/reporting code
- presentation execution debug/export modules
- docs/tests

**Implementation notes**

Recommended commands/report modes:

- `presentation-plan`
- `frame-contracts`
- `preview --view ... --attachment depth`
- `preview --view ... --json-report`
- `frame --view ... --attachment color --format ppm`
- `frame --view ... --attachment normal --format ppm`

The important thing is that engineers can see:

- what the view requested
- what plan was built
- what quality tier executed
- what attachments/history were used
- what backend/binding executed each pass
- which semantic acceleration artifacts were active

**Acceptance criteria**

- Tooling can report presentation plans and frame contracts.
- Attachment/export/debug flows are documented.
- PPM is one export format over color/depth/normal attachments, not the presentation model.

### Workstream C: Docs, Spec, Samples, And Final Revalidation

#### Task 24C1 — Update the spec, examples, and sample projects

**Description**

Make the new presentation architecture concrete in docs and executable content.

**Files**

- `language/spec/README.md`
- `language/spec/tests/spec/language_spec_test.wr`
- `language/view_*` sample projects
- `README.md`

**Implementation notes**

Add at least one sample that shows:

- a `view` declaration
- typed outputs
- quality/history settings
- CPU and WGSL execution
- attachment export and reporting

Keep the first sample narrow and comprehensible.

**Acceptance criteria**

- Docs explain the new view/frame model.
- Executable samples exist.
- The spec suite covers the authored surface sufficiently.
- Old preview examples have been replaced or explicitly marked as removed.

#### Task 24C2 — Revalidate generic query-program machinery

**Description**

Record what presentation taught the compiler about possible shared query-program machinery before this roadmap closes.

**Files**

- roadmap/docs
- optional architecture notes

**Implementation notes**

Do not promote generic machinery just because presentation used it.
Compare the implemented presentation pieces against at least one planned non-presentation observer shape.

Candidate pieces to record:

- pass graph validation
- materialized artifact contracts
- query dependency reporting
- observability aggregation
- backend dispatch summaries
- semantic acceleration artifacts
- query-owned ray solver plans
- field facts and solver observability
- CPU oracle checks

**Acceptance criteria**

- The roadmap records which `PresentationPlan` pieces remain presentation-specific.
- The roadmap records which pieces are candidates for future query-program extraction.
- No generic query-program layer is created without a second concrete observer.

**Phase 24C2 revalidation record**

The implemented `PresentationPlan` proved useful as the first concrete query program, but several parts remain presentation-specific and should stay that way until another observer exists:

- screen-lattice generation and canonical camera-to-pixel ray construction
- frame attachment semantics for color, depth, world normal, motion, and history slots
- presentation lighting/composite policy such as key-light defaults and shaded-color composition
- temporal reuse policy tied to frame-to-frame continuity, motion vectors, and history compatibility
- export-oriented attachment selection such as PPM over color/depth/normal attachments

The work also exposed pieces that are plausible future query-program extraction candidates once collision/traversal or another observer needs the same machinery:

- pass graph validation and dependency ordering
- materialized artifact contracts and lifetime validation
- query dependency reporting and backend dispatch summaries
- semantic-acceleration artifact reporting and solver observability aggregation
- CPU oracle comparison hooks, plan diagnostics, and cost-report aggregation

No generic query-program layer should be extracted yet. Presentation is still the only concrete observer using this full stack, so the compiler should keep these pieces recorded rather than generalized until a second observer demonstrates the same boundary.

### Phase 24 Exit Criteria

- The canonical authored `view` surface is complete for the first real-time slice.
- Legacy `render` presentation scaffolding has been removed or hard-errored.
- Host/runtime entry points for views exist.
- Attachment/export/debug tooling is documented and tested.
- Docs/spec/examples/tooling all reflect the new architecture.
- Candidate shared query-program machinery has been recorded but not prematurely extracted.

## Final Exit Criteria For This Roadmap

This roadmap is complete when all of the following are true.

1. Presentation is no longer centered on `__wr_render_capture_to_ppm` or `__wr_render_scene_color_capture`.
2. The compiler has a canonical `PresentationPlan` and typed view/frame contracts.
3. Query target/cardinality axes exist, and world-target batch query surfaces exist for the question set presentation needs.
4. Semantic attachments, history contracts, and quality contracts exist.
5. CPU oracle and WGSL parity exist for the primary presentation passes.
6. The engine can produce a temporally stable real-time color path through explicit passes.
7. Ray-shaped spatial contracts execute through query-owned ray solver plans, with dense marching represented as a fallback rather than presentation semantics.
8. Compiler-derived semantic acceleration artifacts and solver methods can reduce presentation work while preserving named query/frame contracts.
9. Solver observability can explain whether wins came from analytic solving, hierarchy/support rejection, interval/refinement math, packet/continuation reuse, ray-footprint reasoning, profile-guided specialization, dense fallback avoidance, backend speed, or quality degradation.
10. The repo has benchmark and observability support that make 60 FPS a measurable, controllable target rather than a wish.
11. Native and reconstructed 1080p60/1080p120 target-scene attempts are reported clearly, without implying blanket performance guarantees.
12. The authored `view` surface matches the internal architecture, and legacy `render` presentation scaffolding has been retired rather than preserved for compatibility.
13. Each phase has passed its acceptance criteria and the `AGENTS.md` independent-review completion gate.
14. The roadmap was explicitly revalidated after Phase 17, and is revalidated again after Phase 21, with later scope adjusted to what the implemented plan/color path actually taught the team.
15. `PresentationPlan` is documented as the first concrete query program, and any candidate generic query-program machinery has been recorded for comparison against future collision/traversal work rather than prematurely extracted.

## Suggested Execution Order Inside The Team

Phase 17 is complete and its review feedback has been handled.
Start Phase 18 from the revalidated shape above.

A practical execution order is:

- land **18A0** first so target/cardinality axes are clean before new batch descriptors multiply
- land **18B0** early so new presentation work targets canonical `view`, not legacy `render`
- land **18B1** and **18B3** before primary execution work so projection and primary-hit identity are not guessed later
- land **18A1–18A3** before deep presentation execution work
- move **18C0** as early as possible once screen samples and batch items exist
- then let one engineer own **18B2** while another lands **18C1**
- after Phase 18, land **Phase 19** before primary visibility execution:
  - field facts skeletons
  - ray solver plan boundary
  - dense fallback reporting
  - solver observability/certificate shape
  - first CPU-backed analytic/Lipschitz/refinement hooks where feasible
- after that split Phase 20 into:
  - attachment contracts/resources
  - CPU primary pass
  - WGSL primary pass
- Phase 21 can parallelize surface/participants resolve against shading/composite work once the attachment schemas are frozen
- Phase 22 should stay fairly tight because temporal compatibility is easy to muddy if too many people move it at once; preserve continuation seed data for later query-solver work
- Phase 23 is the place for broader performance parallelization once the contracts are stable; expect quality/control, query-owned ray solver acceleration, and presentation scheduling to split if Phase 21 measurements justify it
- Phase 24 should complete tooling/docs/runtime entry points and delete legacy presentation scaffolding; it is no longer the first authored surface branch

That order keeps the semantic substrate clean and lets junior engineers take self-contained tasks without stepping on unstable boundaries.
