# RFC 0002: Field Engine Implementation Roadmap

Status: Active

Author: Codex

Created: 2026-04-03

Target: `wrela` language, compiler, runtime, preview path, and future WGSL backend

## Summary

This document defines the implementation order for turning the current `wrela` field work into a real field-native engine architecture.

It is not a second language-design RFC. The language direction is already set by [RFC 0001](/Users/ryanwible/projects/wrela/language/spec/rfcs/0001-field-game-language.md). This roadmap is the phase-by-phase implementation plan for landing that direction as a hard-cut system.

The central implementation rule is:

- authored scene meaning stays semantic and compiler-visible
- CPU truth lands first
- GPU/WGSL arrives as a backend for compiler-generated execution, not as the primary authored scene model

## Current Baseline

The following foundation is already landed:

- one shared HIR with lane-aware semantics
- `kernel fn` as the first real portable declaration
- portable math/data substrate (`Vec*`, `Mat*`, `Quat`, `Bounds*`, `Ray3`, `Transform3`, `Surface`, `Payload`, `Hit3`, `Light`, `Camera`, `ActorHandle`, and friends)
- CPU-reference portable execution
- `field exact distance` and `field conservative distance`
- `material(hit: Hit3) -> Surface`
- semantic composition core
- primitive catalog
- `shape` layer for `field` + `material` + `payload`
- first-class `repeat`, `instance`, `transform`, and `mirror`
- support/bounds metadata for the current semantic graph
- first hard-cut exactness rules for the current operators
- CPU preview scenes and preview regression tests
- first render-side metrics: scene trace count, field sample count, and per-hit step counts

This roadmap starts from that baseline.

## Sequencing Rules

Every phase in this roadmap MUST follow these rules:

1. No backward compatibility work. Every cut is authoritative.
2. Scene meaning MUST stay semantic through analysis and optimization.
3. `kernel fn` MUST remain a lower execution lane, not the primary authored scene abstraction.
4. User-authored arbitrary scene kernels are out of scope.
5. CPU truth and CPU tests MUST land before WGSL for the same feature.
6. The compiler MUST preserve semantic structure long enough to optimize it.
7. Every phase MUST ship with parser/HIR tests, semantic tests, lowering/runtime tests, and spec/preview coverage where relevant.
8. Every phase is not complete until an independent review pass is done and any findings are fixed.

## Explicit Non-Goals For This Roadmap

The following are intentionally not part of the authored scene model:

- user-authored low-level march loops as scene source of truth
- user-authored buffer-driven scene composition via `kernel fn`
- imported mesh/texture/volume assets as authored world truth
- “GPU-only” authoring concepts that bypass semantic scene analysis

If an escape hatch is needed later, it MUST be an explicit opaque leaf with pessimistic semantics, not an alternate authored engine.

## Desired End State

The end state of this roadmap is:

1. authored scene code is semantic and compiler-visible
2. host code can run scalar field/shape queries directly on CPU
3. host code can bulk-dispatch typed scene queries through one backend-neutral abstraction
4. the compiler derives march/query policy from scene structure and metadata
5. the compiler generates CPU and WGSL execution from the same semantic graph
6. virtual GPU and CPU remain first-class verification lanes for GPU work

## Primitive And Operator Inventory

The roadmap is only useful if it makes the remaining semantic inventory explicit.

The current implementation already ships the following semantic geometry substrate:

- exact 3D primitives: `sphere`, `box`, `capsule`, `cylinder`, `plane`, `torus`
- structural operators: `union`, `intersection`, `subtract`
- structural wrappers: `transform`, `mirror`, `repeat`, `instance`
- binding layer: `shape(field, material, payload)`
- scalar host queries: `distance_at`, `normal_at`, `trace_shape`, `surface_at`

The following semantic building blocks still need to ship if we want authored scenes to stop falling back to opaque custom math.

### 3D Primitive Inventory Still Missing

These are the next semantic leaf primitives to add:

- `rounded_box`
- `ellipsoid`
- `cone`
- `capped_cone`
- `box_frame`
- `slab`
- `triangle_prism`
- `hex_prism`

These are not all equal priority.

The minimum no-custom geometry set is:

- `rounded_box`
- `ellipsoid`
- `cone`
- `capped_cone`

The hard-surface and architectural extension set is:

- `box_frame`
- `slab`
- `triangle_prism`
- `hex_prism`

### 2D Profile Primitive Inventory Still Missing

The shape-construction phases depend on a 2D profile language. These profile primitives should be treated as first-class semantic leaves, not hidden helper math:

- `circle2`
- `rect2`
- `rounded_rect2`
- `capsule2`
- `segment2`
- `polygon2`
- `polyline2`

Without these, `extrude`, `revolve`, `sweep`, and `loft` will drift toward arbitrary math too quickly.

### Structural Operator Inventory Still Missing

The semantic scene algebra still needs these operators:

- boolean ownership variants: `union nearest`, `union ordered`, `union disjoint`, plus explicit subtraction ownership
- smooth booleans: `smooth_union`, `smooth_intersection`, `smooth_subtract`
- transform classes: `translate`, `rotate`, `uniform_scale`, `affine_transform`, `warp`
- repeat classes: `repeat_linear`, `repeat_grid`, `radial_repeat`, `mirror_array`, `instance_array`
- deformation classes: `bend`, `twist`, `taper`, `displace`
- construction operators: `extrude`, `revolve`, `sweep`, `loft`
- detail gates: `coarse`, `fine`, and related scene-detail participation controls

### Query And Dispatch Inventory Still Missing

The host/dispatch surface still needs these semantic query forms:

- scalar queries: `nearest`, `occluded`, `overlap`, `support_at`
- typed batch records: `RayBatch`, `PointBatch`, `HitBatch`, `OcclusionBatch`, `DistanceBatch`
- backend-neutral bulk dispatch over captures

### World-Scale Semantic Inventory Still Missing

The performance architecture from RFC 0001 still requires:

- `region`
- `capture`
- `domain`
- `render`
- support annotations and overrides
- detail-layer semantics
- region-scoped placement/instancing semantics

## Semantic-To-Kernel Lowering Contract

The scene model MUST lower through a compiler-owned execution pipeline instead of asking users to author execution kernels directly.

The authoritative lowering path is:

1. authored semantic declarations
   - `field`
   - `shape`
   - later `region`, `capture`, `domain`, `render`
2. shared typed semantic HIR
3. semantic scene graph / query graph lowering
4. analysis and annotation passes
   - exactness capability
   - support/bounds propagation
   - ownership/provenance
   - detail participation
   - domain eligibility
   - capture specialization
5. query-plan lowering
   - scalar query plan
   - bulk query plan
   - render plan
   - bake/cull/derived-artifact plan
6. compiler-generated portable query kernels
7. backend lowering
   - CPU oracle
   - virtual GPU
   - WGSL

This means:

- authored scene meaning MUST NOT be expressed as user-authored `kernel fn`
- `kernel fn` remains the lower execution lane for compiler-generated work, explicitly low-level user work, and runtime internals
- if opaque custom field leaves survive, they lower as quarantined call sites inside compiler-generated query kernels, not as “write your scene in kernels”

## Phase 1: March Policy And Observability

### Goal

Turn the current semantic graph into an execution-planning input instead of just a prettier authoring layer.

This phase is about deriving cheaper trace behavior from:

- support/bounds metadata
- exact vs conservative classification
- semantic operator classes

This phase also adds the counters needed to prove that the compiler/runtime is actually winning.

### Worklanes

#### Lane A: March Policy Classification

Implement a first march-policy pass over the semantic field/shape graph.

The pass MUST classify nodes and traces into categories such as:

- exact-safe
- conservative-safe
- bounded
- periodic
- transform-preserving
- expensive-refine-near-surface

Acceptance criteria:

- a semantic trace plan is built before CPU execution
- the plan is derived from graph structure, not user-provided raw step budgets
- exact nodes can take more aggressive safe steps than conservative ones

Tests:

- unit tests for policy selection on small graphs
- graph-shape tests for exact vs conservative propagation into trace plans

#### Lane B: Support-Driven Pruning

Use support/bounds metadata to reject irrelevant branches before distance sampling.

Acceptance criteria:

- traces can skip subgraphs whose support cannot affect the ray/sample
- bounded repeated branches do not force whole-scene evaluation
- policy selection is globally safe even when pruning is active

Tests:

- counter-based tests proving fewer branch visits on bounded scenes
- preview scene probes proving correctness matches the unpruned path

#### Lane C: Observability Counters

Expand metrics beyond `scene_trace` and `field_sample`.

Add counters for:

- support-pruned branches
- candidate branches visited
- exact-path traces
- conservative-path traces
- step histogram buckets
- samples per successful hit

Acceptance criteria:

- counters are queryable from host code and tests
- preview/spec tests can assert optimization behavior without snapshotting full images

Tests:

- unit tests for metric ids and increments
- native tests comparing optimized vs unoptimized branch visits

#### Lane D: Preview And Regression Pressure

Cut the repetition and boolean preview scenes over to the new march-policy counters and assertions.

Acceptance criteria:

- the preview suite still renders successfully on CPU
- at least one scene proves semantic structure reduces work compared with a custom-math equivalent

Tests:

- preview regression suite
- codegen test asserting structural repeat is no worse than manual repetition

### Exit Criteria

- we can explain why a trace is cheap or expensive
- we can measure that structural semantics are buying something
- we have enough observability to safely add more expressive operators

## Phase 2: Exactness Capability System And First-Class Support

### Goal

Turn exactness and support into explicit compiler-managed capabilities rather than partial conventions.

The current hard-cut exactness rules are a good start, but we still need a proper capability table and authored support semantics before the scene language grows more expressive.

### Worklanes

#### Lane A: Exactness Capability Table

Define exactness per semantic node kind.

The table MUST classify:

- exact-preserving primitives
- exact-preserving structural operators
- conservative-only operators
- operators that require explicit downgrade
- operators that require extra declared support information

Acceptance criteria:

- exactness is derived from semantic node kinds, not just from call restrictions
- every new primitive/operator added later must register its exactness behavior explicitly

Tests:

- capability-table unit tests
- exactness diagnostic snapshot tests

#### Lane B: Support As Authored Semantics

Add first-class support/bounds clauses where inference is not enough.

Acceptance criteria:

- authors can state finite support/bounds explicitly when required
- inferred and explicit support combine predictably
- support metadata is preserved through lowering

Tests:

- parser/HIR tests for support clauses
- support override and propagation tests

#### Lane C: Exactness And Support Diagnostics

Make the compiler explain why a field is exact, conservative, bounded, or unbounded.

Acceptance criteria:

- diagnostics identify the semantic node that caused degradation or unboundedness
- preview/spec regressions can assert those diagnostics on representative scenes

Tests:

- diagnostic snapshot tests
- native/spec negative tests

### Exit Criteria

- exactness is a real capability system
- support is first-class where inference is insufficient
- future operators can be added without semantic ambiguity

## Phase 3: Boolean Ownership And Provenance

### Goal

Make material/payload ownership through `union`, `intersection`, and `subtract` explicit and deterministic.

The engine MUST not rely on ad hoc resampling to discover “who owns the surface” after a hit.

### Worklanes

#### Lane A: Ownership Semantics

Define and implement ownership rules for:

- `union`
- `intersection`
- `subtract`

Rules MUST cover:

- nearest winner
- ordered/priority winner
- subtract exposed-face owner
- ties and degenerate overlaps

Acceptance criteria:

- ownership policy is explicit in semantics
- subtraction default behavior is no longer an accidental lowering detail

Tests:

- payload/material winner tests on overlapping primitives
- subtract face tests proving expected owner identity

#### Lane B: Feature Identity

Add stable feature/subshape identity for hits.

Acceptance criteria:

- `Hit3` or an adjacent query record can identify which semantic subshape won
- the identity survives transforms/repeats/instances predictably

Tests:

- repeated-instance feature-id stability tests
- boolean composition provenance tests

#### Lane C: Renderer Cutover

Update CPU preview/material resolution to consume direct provenance instead of fallback resampling.

Acceptance criteria:

- preview/render path gets material/payload from trace results directly
- the hit path does not need to manually re-evaluate child fields just to decide ownership

Tests:

- native render tests proving stable surface identity
- preview scene regression suite

### Exit Criteria

- hit ownership is semantic, explicit, and testable
- material/payload provenance no longer depends on accidental evaluation order

## Phase 4: Capture Boundary, Host Queries, And Typed Bulk Dispatch

### Goal

Formalize the CPU/GPU boundary as a typed dispatch system over captured semantic scenes.

This phase is where host Wrela gets a clean abstraction for:

- scalar CPU queries
- bulk CPU dispatch
- virtual GPU dispatch
- future WGSL dispatch

### Worklanes

#### Lane A: Capture As Execution Boundary

Introduce `capture` as the stable executable boundary for scene queries.

Acceptance criteria:

- host code can capture semantic scene state into a queryable object
- captures are immutable snapshots suitable for CPU and GPU backends

Tests:

- capture snapshot correctness tests
- identical-capture deterministic query tests

#### Lane B: Scalar Host Query Surface

Normalize and extend the host query surface for single-point and single-ray work.

Core queries:

- `distance_at`
- `normal_at`
- `trace_shape`
- `surface_at`
- later `nearest`, `occluded`, `overlap`

Acceptance criteria:

- scalar queries run naturally from host code with no buffer ceremony
- gameplay/tools/tests can stay entirely on CPU

Tests:

- host query native tests
- spec tests for the scalar query surface

#### Lane C: Typed Bulk Dispatch

Add a backend-neutral bulk dispatch abstraction for scene queries.

The abstraction MUST compile bindings and runtime setup away behind typed interfaces.

Representative modes:

- `backend = cpu`
- `backend = virtual_gpu`
- `backend = wgsl`
- `backend = auto`

Acceptance criteria:

- one portable query surface can run on CPU or virtual GPU without source changes
- authored code does not manually manage raw bindings for normal bulk scene queries

Tests:

- CPU bulk-dispatch unit tests
- virtual GPU integration tests
- CPU vs virtual GPU differential tests

#### Lane D: Backend-Neutral Query Records

Define typed input/output records for bulk dispatch.

Representative records:

- ray batches
- point batches
- hit batches
- occlusion batches

Acceptance criteria:

- typed dispatch works over portable value records rather than raw scalar tuples
- record ABI is stable enough for CPU, virtual GPU, and later WGSL

Tests:

- ABI/layout tests
- differential tests across backends

### Exit Criteria

- host code can call scene queries naturally on CPU
- host code can bulk-dispatch scene work without raw binding code
- the capture boundary exists as the stable handoff into execution backends

## Phase 5: Primitive Completion, Smooth Blends, And Deformations

### Goal

Complete the remaining semantic geometry leaf set and then add the first expressive “expensive” operators, but only after march-policy instrumentation and exactness/support semantics exist.

### Worklanes

#### Lane A: Remaining 3D Primitive Catalog

Add the remaining exact/conservative primitive leaves:

- `rounded_box`
- `ellipsoid`
- `cone`
- `capped_cone`
- `box_frame`
- `slab`
- `triangle_prism`
- `hex_prism`

Acceptance criteria:

- every primitive is a semantic node with explicit exactness and support behavior
- no primitive is introduced only as a library helper or opaque custom pattern

Tests:

- parser/HIR tests
- point-sampling tests
- support/exactness tests

#### Lane B: Transform And Repeat Families

Split the current wrappers into explicit semantic operator families:

- `translate`
- `rotate`
- `uniform_scale`
- `affine_transform`
- `warp`
- `repeat_linear`
- `repeat_grid`
- `radial_repeat`
- `mirror_array`
- `instance_array`

Acceptance criteria:

- each operator family is a distinct semantic node, not one overloaded wrapper bucket
- each family registers exactness, support, and march-policy behavior explicitly

Tests:

- parser/HIR tests
- support/exactness tests
- trace-plan tests over repeated and transformed scenes

#### Lane C: Smooth Boolean Operators

Add:

- `smooth_union`
- `smooth_intersection`
- `smooth_subtract`

Acceptance criteria:

- operators are semantic nodes, not inferred math
- exactness degradation is explicit and enforced

Tests:

- parser/HIR tests
- exactness diagnostics
- CPU point-sampling tests

#### Lane D: Deformation Families

Add:

- `bend`
- `twist`
- `taper`
- `displace`

Acceptance criteria:

- each deformation has explicit support and exactness behavior
- the compiler can classify them as “needs conservative handling”

Tests:

- legality tests
- support propagation tests
- preview scenes that visibly exercise each deformation

#### Lane E: Cost Attribution

Tie deformation/blend costs into the march-policy counters.

Acceptance criteria:

- the engine can report when a trace became more expensive because of a blend or deformation

Tests:

- counter-based regression tests

### Exit Criteria

- the remaining baseline primitive catalog is available semantically
- expressive field operators exist without collapsing back to opaque custom math
- their performance cost is visible and testable

## Phase 6: Shape Construction Operators And Profile Algebra

### Goal

Support authored silhouettes and more complicated scenes without forcing users into custom field bodies.

### Worklanes

#### Lane A: 2D Profile Primitive Catalog

Add semantic 2D profile primitives:

- `circle2`
- `rect2`
- `rounded_rect2`
- `capsule2`
- `segment2`
- `polygon2`
- `polyline2`

Acceptance criteria:

- profile primitives are semantic leaves with explicit support/layout rules
- construction operators can consume them without dropping to arbitrary math

Tests:

- parser/HIR tests
- profile point-sampling tests

#### Lane B: Construction Operators

Add:

- `extrude`
- `revolve`
- `sweep`
- `loft`

Acceptance criteria:

- operators are semantic field nodes with typed parameters
- local-space behavior is explicit

Tests:

- parser/HIR tests
- CPU point-sampling tests

#### Lane C: Bounds And Support Rules

Define support/bounds propagation for each construction operator.

Acceptance criteria:

- the compiler can still prune/cull constructed shapes

Tests:

- metadata propagation tests
- trace-plan tests over constructed geometry

#### Lane D: Scene Stress Suite

Add hard scenes that need construction operators:

- architectural openings
- swept supports
- character-like blockout silhouettes
- nested profile/revolve scenes

Acceptance criteria:

- these scenes render on the CPU oracle without dropping to opaque custom math

Tests:

- preview regressions
- point-probe tests

### Exit Criteria

- the semantic field language is expressive enough for real silhouettes and scene blockouts

## Phase 7: Richer Shading Context, Radiance, And Volume

### Goal

Evolve `material` and the shading model beyond the minimal `Hit3 -> Surface` MVP, while keeping it portable and testable.

### Worklanes

#### Lane A: Material Context Enrichment

Add stable shading inputs such as:

- feature id
- local coordinates
- optional curvature-ish data
- later ray footprint or cone info

Acceptance criteria:

- procedural materials become stable under repetition and camera motion

Tests:

- material-context native tests
- preview regressions for repeated scenes

#### Lane B: Radiance And Volume Fields

Add:

- `radiance field`
- `volume field`

Acceptance criteria:

- authored emissive and media semantics are real scene constructs, not renderer-only hacks

Tests:

- CPU sampling tests
- preview scenes with emissive and haze/fog

#### Lane C: CPU Truth Renderer Upgrade

Upgrade the CPU truth renderer with:

- hard shadows
- ambient occlusion
- PBR surface usage
- emissive/radiance participation

Acceptance criteria:

- the CPU oracle is visually meaningful enough to be a GPU-differential truth path

Tests:

- preview regressions
- numeric shading sanity tests

### Exit Criteria

- portable authored shading is rich enough to support believable scenes
- CPU rendering remains the truth path for later GPU work

## Phase 8: Region, Domain, Render, And World-Scale Composition

### Goal

Move from local semantic scene graphs to the real world/runtime architecture from RFC 0001.

### Worklanes

#### Lane A: Region Composition

Add:

- `region`
- local region identity
- seeded/world-parameterized composition

Acceptance criteria:

- worlds are composed from finite, streamable semantic regions

Tests:

- region composition tests
- deterministic seed/placement tests

#### Lane B: Domain-Specialized Query Semantics

Add:

- `domain`
- `coarse` / `fine` detail participation
- domain-specific evaluator selection

Acceptance criteria:

- rendering, collision, navigation, and other domains can choose different query strategies from one authored world
- expensive detail can be activated only in the domains and tiers that need it

Tests:

- domain-specific CPU query tests
- policy-selection tests
- detail-tier participation tests

#### Lane C: Render Declarations

Add `render` as the authored presentation contract over captured semantic worlds.

Acceptance criteria:

- presentation becomes semantic and compiler-owned rather than handwritten preview code

Tests:

- render-surface tests
- preview cutover tests

### Exit Criteria

- world composition and query specialization are first-class semantic systems
- the performance architecture is no longer just local scene semantics

## Phase 9: Derived Artifacts, Culling, Baking, Internal Kernels, And Opaque Leaf Quarantine

### Goal

Keep authored scenes semantic, but let the compiler/runtime generate lower-lane work from them.

This is where `kernel fn` becomes useful as a derived execution substrate rather than a scene-authoring escape hatch.

### Worklanes

#### Lane A: Derived Artifact Pipeline

Generate artifacts such as:

- culling tables
- capture caches
- occupancy/visibility data
- later probe/light data

Acceptance criteria:

- artifacts are derived from semantic scenes and captures, never source of truth

Tests:

- derivation determinism tests
- artifact-vs-direct-query parity tests

#### Lane B: Compiler-Generated Internal Kernels

Lower selected derived jobs into portable/internal kernel execution.

The first internal kernel families should be:

- capture/update kernels
- batch distance/occlusion/trace kernels
- culling kernels
- bake kernels for derived artifacts

Lowering contract:

- semantic scene graph/query graph lowers to a specialized query plan
- the query plan lowers to compiler-generated portable kernel bodies
- those generated kernels lower to CPU, virtual GPU, and later WGSL
- user-authored scene code never bypasses the semantic graph and drops straight into kernel execution

Acceptance criteria:

- authored scenes are not expressed as manual kernels
- compiler-generated kernels can run on CPU first and later WGSL

Tests:

- CPU internal-kernel tests
- virtual GPU integration tests

#### Lane C: Opaque Leaf Policy

If an escape hatch is still needed, add it only here and only as an explicit opaque leaf.

Rules:

- no user-authored arbitrary scene kernel path
- opaque fields MUST declare conservative status
- opaque fields MUST provide or accept explicit support/bounds constraints
- opaque fields MUST be a pessimization boundary

Acceptance criteria:

- the escape hatch cannot masquerade as a normal optimizable node

Tests:

- legality tests
- pessimization-boundary tests

### Exit Criteria

- lower-lane execution exists without sacrificing authored scene semantics
- any remaining escape hatch is explicit and quarantined

## Phase 10: WGSL Backend And Differential Validation

### Goal

Lower the semantic query/dispatch system to WGSL compute after CPU truth is solid.

### Worklanes

#### Lane A: WGSL Lowering

Lower:

- portable query plans
- generated internal kernels
- typed dispatch records

Acceptance criteria:

- WGSL is emitted from semantic/portable lowering, not handwritten as the primary scene language

Tests:

- WGSL codegen tests
- layout/binding tests

#### Lane B: Virtual GPU Differential Lane

Use the virtual GPU as an integration/testing lane between CPU truth and real WGSL.

Acceptance criteria:

- CPU, virtual GPU, and WGSL can run the same query/dispatch surface

Tests:

- CPU vs virtual GPU parity tests
- virtual GPU vs WGSL parity tests

#### Lane C: Image And Query Differentials

Run differential checks for:

- scalar queries
- bulk query outputs
- rendered images

Acceptance criteria:

- GPU correctness is defined as parity with CPU truth within declared tolerances

Tests:

- per-query differentials
- image/regression tests

### Exit Criteria

- WGSL is a real backend for the same authored semantic engine
- CPU remains the oracle

## Phase 11: Performance Closure

### Goal

Turn the semantic engine into a measurably fast engine without changing authored semantics.

### Worklanes

#### Lane A: Specialization

Add specialization based on:

- supports
- domains
- captures
- phase/world variants
- repeated-instance structure

Acceptance criteria:

- specialization cuts work without changing authored behavior

Tests:

- counter-based perf regressions
- parity tests against the unspecialized path

#### Lane B: Render/Query Cost Reports

Build developer-facing cost explanations.

Acceptance criteria:

- the compiler/runtime can explain why a scene or query is expensive

Tests:

- diagnostic snapshot tests

#### Lane C: Hard Scene Benchmark Suite

Create a permanent hard-scene suite covering:

- large repetition
- thin features
- nested transforms
- smooth blends
- deformations
- character-like silhouettes
- region/domain composition

Acceptance criteria:

- the suite exercises the real engine architecture rather than just toy examples

Tests:

- perf counters
- CPU/WGSL image differentials

### Exit Criteria

- performance work is driven by semantic structure and measurements
- authored code does not need to change to get faster

## Phase Ordering Summary

Implement in this order:

1. march policy and observability
2. exactness capability system and first-class support
3. boolean ownership and provenance
4. capture boundary, host queries, and typed bulk dispatch
5. primitive completion, smooth blends, and deformations
6. shape construction operators and profile algebra
7. richer shading context, radiance, and volume
8. region, domain, render, and world-scale composition
9. derived artifacts, culling, baking, internal kernels, and opaque leaf quarantine
10. WGSL backend and differential validation
11. performance closure

## Why This Order

This order is deliberate:

- March policy comes first because the current graph is finally rich enough to optimize and measure.
- Exactness and support come next because every later primitive and operator needs an explicit capability story.
- Ownership comes after that because payload/material provenance must be semantic before dispatch and rendering scale up.
- Capture and dispatch come before WGSL because the CPU/GPU boundary must be typed and testable before backend work.
- Primitive completion, smooth operators, and construction ops wait until cost attribution and capability rules exist, so we can add expressiveness without losing control.
- Region/domain/render wait until local scene semantics and dispatch boundaries are real, so they can become the performance architecture instead of speculative syntax.
- WGSL comes after CPU truth, virtual GPU validation, and typed dispatch, so the backend is an implementation target instead of a second engine.

## Final Implementation Rule

The compiler MUST own the lowering from semantic scene graph to execution.

Authors describe:

- fields
- shapes
- regions
- captures
- domains
- renders

The compiler derives:

- trace/query policy
- support-based pruning
- specialization
- derived artifacts
- CPU execution
- virtual GPU execution
- WGSL execution

That is the architecture this roadmap is intended to ship.
