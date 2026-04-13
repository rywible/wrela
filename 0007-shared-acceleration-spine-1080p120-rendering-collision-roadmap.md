
# RFC 0007: Shared Acceleration Spine For 1080p120 Rendering And Collision Performance

Status: Proposed post-Phase-34 performance-closure roadmap after repo read, benchmark read, and supplemental rendering/WGSL research

Author: GPT-5.2 Pro

Created: 2026-04-13

Target: post-Phase-34 `wrela` language, compiler, query-family runtime, presentation pipeline, collision pipeline, CPU oracle, vGPU/WGSL execution, and artifact runtime

## Summary

Wrela is now at the point where the architecture is good enough that performance problems are no longer “just optimize the loop” problems.

The repo already has the hard prerequisites that most field engines never build:

- Scene IR carries explicit field, shape, and support structure.
- semantic evidence already distinguishes exact, conservative, and opaque behavior.
- `RaySolverPlan` already exists as a compiler-owned planning boundary.
- presentation already has typed quality contracts, adaptive control, tile culling, hit compaction, and `realtime_120` as a named quality tier.
- collision already has explicit plans, broadphase artifacts, witness caches, and transition-aware execution.
- artifacts already have compatibility and validity contracts instead of ad hoc cache keys.

That means the path to 1080p at 120 FPS is not “add one clever stepping trick.”

It is to build a **shared acceleration spine** that presentation and collision both consume, then progressively replace dense per-ray/per-step work with:

1. **hierarchical conservative candidate traversal**
2. **ray-entry / ray-interval support solving**
3. **analytic and certified hybrid solvers**
4. **derived spatial caches for far-field and repeated work**
5. **presentation scheduling that reduces divergence without hiding semantic changes**
6. **collision reuse of the same artifacts and witnesses instead of a second performance stack**

This RFC proposes exactly that.

The design stance is intentionally conservative:

- production deterministic acceleration comes first
- CPU dense behavior remains the oracle
- every fast path states its guarantee class explicitly
- derived artifacts are compiler-owned and validity-checked
- collision is accelerated by the same substrate, not as an afterthought
- learned methods are allowed only as an **optional late phase** behind verifier-and-fallback rules

The core thesis is simple:

**1080p120 is only realistic if the engine stops paying O(N) world/union traversal and raw dense marching costs in the hot path.**

That means the first-class work items are:

- replace linear world/union scans with a support-aligned acceleration forest
- start rays at the first plausible contact interval instead of at `t = 0`
- let solver plans choose analytic, relaxed, Lipschitz, repeat-aware, interval, and refinement methods per candidate class
- move far-field work onto explicit derived artifacts
- keep presentation and collision on the same acceleration story

## Relationship To Earlier RFCs And Repo Vision

This roadmap builds directly on:

- `language/spec/rfcs/0001-field-game-language.md`
- `language/spec/rfcs/0002-field-engine-implementation-roadmap.md`
- `language/spec/rfcs/0003-phase-9-5-semantic-convergence-plan.md`
- `0004-question-families-query-contracts-roadmap.md`
- `0005-realtime-presentation-view-plans-frame-contracts-roadmap.md`
- `0006-certified-world-snapshots-temporal-semantics-artifact-runtime-query-program-spine-roadmap.md`

RFC 0005 established that presentation is a compiler-owned observer loop with explicit quality contracts, pass graphs, and query-family execution.
RFC 0006 established explicit world snapshots, artifact validity rules, collision plans, a shared query-program spine, and the rule that mixed solver planning must stay evidence-driven.

This RFC is not a replacement for either document.

It is the concrete performance-closure plan that turns those abstractions into the runtime shape needed for 1080p120 and collision-grade throughput.

In other words:

- RFC 0005 made presentation structurally correct.
- RFC 0006 made artifact reuse and cross-observer execution structurally correct.
- RFC 0007 makes the hot path structurally fast.

## Research Grounding

Two attached research notes strongly shaped this plan.

The first note, **AI-Accelerated Ray Marching for Wrela Fields**, makes the right architectural point: Wrela is already a solver platform, so ML should only ever show up as one more solver method with explicit guarantee classes, artifact caching, and dense fallback.
It also correctly argues that larger ray steps are only shippable when they are either provably conservative or wrapped in proposal → verifier → fallback behavior.

The second note, **Field-Based Ray Marching at 1080p 120 FPS**, makes the right production point: the near-term gains come first from support-ray entry intervals, union/subtree BVHs, analytic hits under transforms, safe over-relaxation, interval methods for pathological rays, derived spatial caches, and divergence-aware scheduling.

This RFC follows both conclusions:

- deterministic structural acceleration is the critical path
- optional learned artifacts come only after the deterministic stack is proven

## Current Repo Read

The current repo is close enough to the right architecture that the next work should be a focused refactor, not a rewrite.

### What is already strong

1. `compiler/scene_ir/mod.rs` already gives us field, shape, and support graphs with explicit `DistanceSemantics`, `SceneTraceSafety`, `SupportClass`, transform kinds, and repeat kinds.
2. `compiler/semantic_evidence/mod.rs` already tracks primitive facts, transform facts, repetition facts, analytic-intersection availability, Lipschitz status, and evidence scope/origin.
3. `compiler/query_solver/mod.rs` already has a real portfolio boundary with methods such as:
   - `DenseSphereTracing`
   - `SupportBoundCandidateRejection`
   - `AnalyticPrimitiveIntersection`
   - `LipschitzSafeStepping`
   - `IntervalNewtonIsolation`
   - `SafeguardedNewtonRefinement`
   - `AffineArithmeticBounds`
   - `RepeatAwareTraversal`
   - `TilePacketSolving`
   - `NeighborFrameContinuation`
4. `compiler/presentation_contract/mod.rs` and `compiler/presentation_plan/mod.rs` already expose `RealtimeQualityContract`, named quality tiers including `Realtime120`, degradation order, typed frame attachments, temporal history, and view/frame contracts.
5. `compiler/presentation_exec/*` already has:
   - tile culling
   - hit compaction
   - temporal reuse
   - frame cost reports
   - WGSL presentation execution
6. `compiler/collision_plan/mod.rs` and `compiler/collision_exec/cpu.rs` already have explicit collision plans, support-summary artifacts, broadphase candidates, witness caches, continuation seeds, and transition-aware validity rules.
7. `compiler/artifact_contract/mod.rs` and `compiler/artifact_store/mod.rs` already give the right place to hold derived acceleration artifacts with versioning and compatibility logic.
8. `compiler/query_exec/mod.rs` already exposes strong observability fields for candidate counts, support pruning, trace steps, analytic hits, interval skips, packet/tile rejections, Newton refinements, continuation, and fallback reasons.

### Where the hot path is still too expensive

The current execution still pays dense and linear costs where 120 FPS will not tolerate them.

1. **Dense step loop is still the center of the ray path.**

In `compiler/query_exec/cpu.rs`, the core trace loop is still:

```rust
travel += distance.max(min_step);
```

In `compiler/query_exec/wgsl/codegen.rs`, the generated WGSL path still emits:

```wgsl
travel = travel + max(distance, min_step);
```

That is the canonical fallback, but it is still too close to being the main path.

2. **World traversal is still largely linear.**

The generated WGSL world distance path still loops over every shape:

```wgsl
for (var index: u32 = 0u; index < dispatch_config.shape_count; index = index + 1u) {
    current = min(current, shape_distance_dispatch(world_shapes.values[index], point));
}
```

The same pattern appears in world normals, radiance, and media accumulation.
That means acceleration is not yet structurally owning world traversal.

3. **Union evaluation is still effectively O(N) per distance evaluation.**

`eval_shape_node` still evaluates unions and other shape operators by recursively walking the whole subtree.
For large constructive scenes, this multiplies directly by step count.

4. **Support pruning is currently point-based and early, not ray-interval-based and hierarchical.**

The CPU world-trace path already does something useful: it uses `eval_shape_support_lower_bound(shape, origin)` to reject shapes before tracing.
That is a good start, but it still means:
- lower bounds are queried at the ray origin, not over the current ray interval
- rejection is at the top shape level, not deep inside world/union subtrees
- repeated structures and transformed supports are not yet traversed as first-class ray intervals

5. **Analytic solving exists only for a narrow special case.**

`try_analytic_sphere_hit` is real and useful, but it currently only covers a leaf sphere case with residual verification.
That is proof the architecture works, not yet the performance story.

6. **Presentation already has tile culling and hit compaction, but the candidate story is still coarse.**

Current tile culling is screen-projected region bounds.
That is valuable, but it does not yet hand the primary visibility pass a strong per-tile candidate set derived from the same support hierarchy the query engine understands.

7. **Collision broadphase exists, but it does not yet share a richer acceleration forest with rendering.**

Collision already has `BuildBroadphaseCandidates`, witness caches, and continuation artifacts.
The next win is to make those consume the same spatial acceleration substrate as presentation, not a parallel lighter-weight one.

## Why 1080p120 Requires Structural Change

1920 × 1080 is 2,073,600 pixels.
At 120 FPS, that is 248,832,000 primary samples per second before shadows, media, or secondary work.

Even an average of 16 primary steps per ray means billions of field evaluations per second once world traversal cost is included.

So the performance target cannot be met by “make each dense step a bit cheaper.”
It requires all of the following:

- fewer candidate shapes per ray
- fewer subtree evaluations per candidate
- fewer steps to get near the hit
- cheaper late-stage refinement
- less divergence in presentation scheduling
- derived caches that move far-field work out of the procedural inner loop
- collision reusing the same artifacts so perf work is not duplicated

## Goals

1. Make `realtime_120` a real performance target for representative scenes at 1920 × 1080 with explicit reporting of native/internal/reconstructed resolution.
2. Remove linear world traversal and linear large-union traversal from the primary hot path when conservative acceleration data exists.
3. Keep collision performance on the same roadmap by reusing shared acceleration artifacts for point, ray, overlap, sweep, and time-of-impact queries.
4. Keep CPU dense behavior as the correctness oracle.
5. Keep required guarantee and selected method class separate in all solver and artifact decisions.
6. Make every fast path observable enough that a perf report can explain why it won or lost.
7. Make every task junior-executable: clear ownership, concrete files, concrete tests, concrete acceptance criteria.
8. Allow learned artifacts only as optional internal solver methods after deterministic methods have shipped.

## Non-Goals

1. This RFC does not propose handwritten presentation shaders outside the compiler-owned WGSL generation path.
2. This RFC does not make approximate or probabilistic methods part of collision correctness.
3. This RFC does not require hardware ray tracing support.
4. This RFC does not redefine authored world semantics around baked distance fields.
5. This RFC does not treat “120 FPS” as permission to silently lower semantic correctness or hide quality changes.
6. This RFC does not make ML a required dependency for the mainline engine.

## Design Rules

1. **Semantic authority remains public and explicit.** Scene IR, query-family contracts, and semantic evidence remain the source of truth. The acceleration spine is a derived execution substrate, not a replacement semantic model.
2. **One acceleration spine, many observers.** Presentation and collision should consume the same structural acceleration artifacts wherever possible.
3. **Keep four layers distinct.** The RFC must keep the logical acceleration model, artifact taxonomy/validity rules, runtime ABI/layout, and observer-local scheduling/caching policy separate.
4. **Dense CPU semantics remain the oracle.** Every fast path must have a clear fallback.
5. **Required guarantee and selected method class remain separate.** Never blur policy with implementation.
6. **Logical contracts come before packing.** Define what a node, certificate, or artifact means before defining how CPU or WGSL stores it.
7. **Derived artifacts are first-class semantic artifacts.** They must be versioned, validated, and invalidated through the artifact runtime.
8. **Optimize by removing work before approximating work.** Candidate reduction and structural pruning come before fancy stepping.
9. **WGSL remains generated and explicit.** No backend-only handwritten semantics.
10. **No hidden quality changes.** Quality-ladder changes must remain explicit in contracts and reports.
11. **Collision is not a second-class consumer.** If an optimization creates a rendering-only artifact, the RFC must justify why collision cannot use it.
12. **Subgroup-dependent behavior must remain optional and portable.** No correctness path may depend on undocumented subgroup mapping or divergence behavior.
13. **Learned methods must be verifier-backed or conservative-by-construction.** Best-effort ML may exist only behind explicit experimental policy.
14. **Identity and provenance survive acceleration.** `shape_id`, `repeat_id`, `instance_id`, and hit payload rules must remain intact.
15. **Reports must say where the time went.** A speedup without attribution is not “done.”
16. **Closure must be falsifiable.** The engine should not claim `realtime_120` closure without a named canonical closure profile and fixed run protocol.

## Architecture Overview

The proposed runtime shape is intentionally layered so that “one acceleration spine” does not erase the observer boundary.

1. **Semantic authority**
   - Scene IR, query contracts, presentation contracts, collision contracts, and semantic evidence remain the public and compiler-owned meaning of the world
   - no acceleration artifact may redefine authored semantics or weaken a public guarantee silently

2. **Shared derived acceleration artifacts**
   - snapshot-scoped, compiler-owned artifacts derived from Scene IR support/shape structure
   - includes the shared acceleration forest, union-subtree acceleration, and snapshot-scoped brick/support caches
   - stores conservative bounds, identity lineage, candidate class, transform/repeat semantics, and provenance handles
   - reusable across query, presentation, and collision when their contracts allow it

3. **Observer-local artifacts**
   - view-local or transition-local artifacts such as tile candidate tables, clipmaps, temporal continuation seeds, packet queues, and camera-path caches
   - may consume the shared spine, but may not become world truth
   - must declare explicit validity, budget, and fallback behavior

4. **Runtime ABI and layout lowerings**
   - CPU-friendly and WGSL-friendly packed layouts generated from the logical artifact contracts
   - explicit layout signatures, size accounting, and compatibility validation
   - these lowerings are not themselves the semantic contract

5. **Hybrid deterministic solver ladder**
   - support interval entry/exit
   - analytic primitive solving under safe transforms
   - conservative relaxed stepping
   - Lipschitz / derivative-guided refinement
   - interval / affine fallback for pathological cases
   - dense fallback when evidence or verification is insufficient

6. **Presentation scheduling**
   - tile/cluster candidate tables derived from the same forest
   - compacted work queues
   - adapter-aware workgroup sizing and buffer layout
   - explicit `realtime_120` quality decisions

7. **Collision parity**
   - broadphase, witness reuse, continuation, and TOI solving over the same acceleration spine
   - separate contact-normal roles preserved from shading-normal roles

The shared acceleration spine lives in layer 2.
Layer 3 may specialize it for presentation or temporal reuse, but may not redefine it.
Layer 4 lowers layers 2 and 3 into executable layouts and should remain downstream of their contracts.

### New shared types

The core new internal module is a shared acceleration module, not a new authored surface.
It should define logical contracts first, and backend packing later.

The first phase should introduce logical records that answer:

- what kind of support/bound a node represents
- what conservative semantics that bound carries
- which subject/subtree the node belongs to
- which transform/repeat semantics are still active
- which candidate class the node exposes to solvers
- which proof/certificate provenance attaches to the node
- what fallback is required when semantics weaken

Only after those logical contracts exist should the repo define flattened CPU or WGSL layouts.

**Illustrative logical sketch**

```rust
pub const ACCELERATION_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AccelerationSubjectKind {
    World,
    Shape,
    FieldSubtree,
    ShapeSubtree,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AccelerationNodeKind {
    WorldRoot,
    SupportBound,
    ShapeLeaf,
    UnionCluster,
    RepeatCellRange,
    CacheBrickSet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CandidateClass {
    ExactPrimitive,
    ConservativeField,
    OpaqueField,
    RepeatedStructure,
    CachedDistanceRegion,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SupportBoundDescriptor {
    pub support_class: SupportClass,
    pub distance_semantics: DistanceSemantics,
    pub transform_kinds: Vec<TransformKind>,
    pub repeat_kinds: Vec<RepeatKind>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccelerationLineage {
    pub subject: SmolStr,
    pub shape_id: Option<SmolStr>,
    pub subtree_root: Option<SmolStr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertificateProvenanceHandle {
    pub proof_family: SmolStr,
    pub report_key: SmolStr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccelerationFallbackExpectation {
    DenseOracleRequired,
    ConservativeTraversalOnly,
    RenderingOnlyReuse,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AccelerationNodeContract {
    pub id: u32,
    pub kind: AccelerationNodeKind,
    pub candidate_class: CandidateClass,
    pub support: SupportBoundDescriptor,
    pub lineage: AccelerationLineage,
    pub child_start: u32,
    pub child_count: u32,
    pub leaf_index: Option<u32>,
    pub certificate: Option<CertificateProvenanceHandle>,
    pub fallback: Option<AccelerationFallbackExpectation>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AccelerationForestContract {
    pub schema_version: u32,
    pub subject: SmolStr,
    pub subject_kind: AccelerationSubjectKind,
    pub root: u32,
    pub nodes: Vec<AccelerationNodeContract>,
    pub children: Vec<u32>,
    pub leaf_shapes: Vec<SmolStr>,
}
```

This sketch is illustrative, not normative.
The normative requirement is the logical split:

- `compiler/acceleration/mod.rs` defines the logical contracts
- backend-specific packing lives later in ABI/layout code
- CPU and WGSL structs are lowerings of the logical contracts, not the other way around

This module should be shared by query, presentation, and collision planning.
It is not a backend helper.
It is the logical hot-path representation that later executable layouts must preserve.

## Performance Closure Contract

This RFC adopts the following explicit closure target.

Before any phase may claim 1080p120 closure, the repo must check in at least one named canonical closure profile.

This is distinct from the existing perf run profiles such as `smoke`, `standard`, and `deep`.
Those choose run intensity.
The closure profile chooses the machine, backend, limits, scene set, and pass/fail protocol.

For each canonical closure profile, the contract must state:

- profile name
- exact machine or machine-class identifier
- adapter name pattern or adapter identity
- backend/API (`wgsl`/`wgpu`, CPU oracle companion path, optional vGPU reference)
- required limits/features profile
- output width / height
- target FPS
- allowed internal-resolution floor for `realtime_120`
- exact legal degradation set
- fixed scene-set id
- fixed view/camera-path ids
- fixed motion-path fixtures for cache-sensitive phases
- fixed random seed
- warmup run count
- measured run count
- pass/fail statistics (`median`, `p95`)
- primary visibility budget target
- total frame budget target
- collision baseline id and allowed regression threshold

For any run against a closure profile, reports must distinguish:

- output resolution
- internal resolution scale
- whether reconstruction/expansion was used
- active acceleration artifacts
- active degradations
- primary visibility median / p95
- total frame median / p95
- collision throughput delta vs baseline
- remaining dominant bottleneck pass

The engine should consider the target reached only when both are true:

1. representative `realtime_120` views meet the frame-time contract on the named closure profile and fixed benchmark protocol
2. collision benchmark throughput is at least neutral and preferably materially improved versus the pre-RFC baseline

This means “insane rendering perf” is not counted as success if collision regresses badly.

## Phase Overview

- **Phase 35** — performance contracts, baseline closure, and shared acceleration scaffolding
- **Phase 36** — support-aligned acceleration forest and hierarchical candidate traversal
- **Phase 37** — deterministic hybrid solver closure
- **Phase 38** — WGSL and presentation scheduling closure for `realtime_120`
- **Phase 39** — derived spatial caches and far-field acceleration
- **Phase 40** — collision acceleration parity and witness reuse
- **Phase 41** — benchmark gates, shipping closure, and optional learned artifacts

---

# Phase 35: Performance Contracts, Baselines, And Shared Scaffolding

## Goal

Make the target measurable, extend observability to the real bottlenecks, and introduce a shared acceleration module before deeper execution work begins.

## Why this phase exists

The repo already has good observability, but not enough to tell whether a 120 FPS improvement came from:

- fewer candidates
- shallower traversal
- larger safe steps
- better scheduling
- derived caches
- or a hidden quality drop

That gap needs to be closed first.

### Workstream A: Performance Contract And Baseline

#### Task 35A1 — Add an explicit target-hardware and performance-closure contract

**Description**

Create a small typed contract that captures the canonical closure profile for 1080p120 presentation plus collision non-regression.
This should be distinct from the repo’s existing perf run profiles.

**Files**

- new `compiler/perf_target/mod.rs`
- `compiler/presentation_contract/mod.rs`
- `compiler/bin/wrela/perf_engine.rs`
- `compiler/bin/wrela/commands/test_eval_perf.rs`
- `benchmarks/README.md`

**Implementation notes**

Recommended fields:

- profile name
- machine id or machine-class id
- adapter name pattern
- backend (`wgpu`, native WGSL path, optional virtual GPU reference)
- requested limits profile
- output width / height
- target FPS
- allowed internal-resolution floor for `realtime_120`
- exact legal degradation set
- fixed scene-set id
- fixed camera path or named view set
- fixed motion-path fixtures for cache-sensitive phases
- fixed random seed
- warmup run count
- measured run count
- pass/fail metrics (`median`, `p95`)
- primary visibility budget target
- total frame budget target
- collision throughput baseline id

Do not bury this in ad hoc benchmark JSON.
Make it a typed compiler/runtime concept.
The first implementation should check in at least one named canonical closure profile before the project claims closure.

**Code sketch**

```rust
pub struct PerformanceClosureProfile {
    pub name: SmolStr,
    pub machine_class: SmolStr,
    pub adapter_profile: SmolStr,
    pub backend: DispatchBackend,
    pub output_width: u32,
    pub output_height: u32,
    pub target_fps: u32,
    pub min_internal_scale: f32,
    pub legal_degradations: Vec<QualityDegradationStep>,
    pub scene_set_id: SmolStr,
    pub camera_path_id: SmolStr,
    pub motion_fixture_id: Option<SmolStr>,
    pub fixed_seed: u64,
    pub warmup_runs: u32,
    pub measured_runs: u32,
    pub frame_budget_ms: f32,
    pub frame_p95_budget_ms: f32,
    pub primary_visibility_budget_ms: f32,
    pub primary_visibility_p95_budget_ms: f32,
    pub collision_baseline: SmolStr,
}
```

**Acceptance criteria**

- A typed closure contract exists.
- At least one named canonical closure profile exists in-tree.
- CLI perf commands can load and report against it.
- Tests reject impossible or contradictory contracts.
- Benchmark reports print both frame and collision closure status against the fixed run protocol.

#### Task 35A2 — Extend observability with traversal and certificate counters

**Description**

Add the counters needed to explain the new acceleration stack.

**Files**

- `compiler/query_exec/mod.rs`
- `compiler/query_exec/cost.rs`
- `compiler/presentation_exec/cost.rs`
- `compiler/collision_exec/mod.rs`
- `compiler/tests/query_exec.rs`
- `compiler/tests/presentation_exec.rs`
- `compiler/tests/collision_exec.rs`

**Implementation notes**

Add counters for at least:

- acceleration node visits
- union-cluster visits
- ray-support interval rejections
- ray-support entry jumps
- repeat cell skips
- cache-brick visits
- cache hits / misses
- accepted relaxed steps
- rejected relaxed steps
- analytic transformed hits
- interval subdivisions
- interval proof successes
- continuation seed hits by observer

These should extend existing `QueryExecutionObservability`, not create a second reporting channel.

**Acceptance criteria**

- New counters are recorded in CPU traces.
- Cost reports can attribute wins to traversal, solver, cache, or scheduling causes.
- Presentation frame reports expose the new metrics.
- Collision reports expose at least broadphase, witness reuse, and interval/refinement metrics.

### Workstream B: Shared Acceleration Types

#### Task 35B1 — Add `compiler/acceleration/mod.rs` with shared non-executing types

**Description**

Introduce a shared module for logical acceleration contracts and reports.

**Files**

- new `compiler/acceleration/mod.rs`
- new `compiler/acceleration/report.rs`
- new `compiler/tests/acceleration.rs`

**Implementation notes**

This phase is type-only and report-only.
It should not change execution yet.
It should define the logical model, not CPU or WGSL packing.

Add types for:

- acceleration forest contracts
- node kinds and candidate classes
- support/bound descriptors
- lineage records
- ray intervals and ray-entry results
- cache descriptors and artifact scope
- certificate provenance handles
- fallback expectations when semantics weaken
- per-observer usage summaries

Keep the API dumb and explicit.
Junior engineers should be able to read the structs and understand what runtime data will exist.
Do not define WGSL-friendly packed structs in this phase.

**Acceptance criteria**

- A shared acceleration module exists.
- It compiles with no backend/execution dependency.
- The logical contracts do not depend on WGSL or CPU packing details.
- Unit tests cover basic construction and report formatting.
- A debug dump can print a forest in stable deterministic order.

#### Task 35B2 — Add planner-level artifact contracts for shared acceleration data

**Description**

Teach query, presentation, and collision plans how to declare acceleration artifacts before those artifacts are executable.

**Files**

- `compiler/query_plan/mod.rs`
- `compiler/presentation_plan/mod.rs`
- `compiler/collision_plan/mod.rs`
- `compiler/artifact_contract/mod.rs`
- `compiler/tests/phase9_query_plan.rs`
- `compiler/tests/presentation_plan.rs`
- `compiler/tests/collision_plan.rs`

**Implementation notes**

Recommended scope split:

- **shared snapshot-scoped artifacts**
  - `shared_acceleration_forest`
  - `shared_union_subtree_forest`
  - `distance_brick_cache`
  - `support_brick_cache`
- **query- or observer-local artifacts**
  - `ray_candidate_table`
  - `tile_candidate_table`
  - `view_distance_clipmap`
  - `continuation_seed_table`

Do not create observer-private names for artifacts that are logically shared.
Also do not mislabel observer-local artifacts as shared just because they consume the shared spine.

**Acceptance criteria**

- Plans can declare shared acceleration artifacts.
- Plans can distinguish shared snapshot-scoped artifacts from observer-local artifacts.
- Artifact contracts carry version, compatibility, and validity rules.
- Plan dumps show the artifacts even before runtime execution exists.
- Validation rejects incompatible artifact/observer pairings and invalid scope reuse.

### Workstream C: Baseline Scenes And Closure Harness

#### Task 35C1 — Add benchmark scenes and views that are representative of the real bottlenecks

**Description**

Extend the current representative benchmark suites into explicit closure scenarios and fixed closure protocols.

**Files**

- `benchmarks/realtime_presentation/tests/realtime_presentation_test.wr`
- new `benchmarks/field_engine/1080p120_*`
- `benchmarks/README.md`

**Implementation notes**

Keep the existing microbench scenes and the current representative real-time fixtures.
Add explicit closure scenarios and fixed benchmark protocols for:

- dense constructive geometry
- repetition-heavy scene
- thin-stack / grazing geometry
- transformed primitive gallery
- mixed opaque/conservative scene
- collision-heavy transition scene
- cache-stress motion path
- camera-motion path with temporal reuse and clipmap churn

Each should have at least one `realtime_quality(target_fps = 120)` view plus a named fixed camera path or motion fixture where applicable.

**Acceptance criteria**

- Closure benchmark scenes exist.
- Benchmark names clearly state whether they are microbench or closure scenes.
- `realtime_120` views are present for representative scenarios.
- Benchmark docs explain what each scene is intended to stress and which fixed protocol they belong to.

### Phase 35 Exit Criteria

- There is an explicit 1080p120 closure profile and benchmark protocol.
- New observability can attribute traversal, solver, cache, and scheduling wins.
- Shared acceleration types exist.
- Planner-level artifact declarations exist for the shared acceleration story with explicit scope boundaries.
- Benchmarks include representative `realtime_120` views and collision-heavy cases.

---

# Phase 36: Support-Aligned Acceleration Forest And Hierarchical Traversal

## Goal

Turn support summaries from “something we can query” into “the structure that owns candidate traversal.”

## Why this phase exists

Right now the repo has the semantic information required to prune work, but not yet the runtime shape to exploit it at scale.
That is why the hot path still looks linear in too many places.

### Workstream A: Forest Construction

#### Task 36A1 — Build a deterministic acceleration forest from world shapes and large union subtrees

**Description**

Lower region/world shape sets and large shape unions into a flattened acceleration forest.

**Files**

- `compiler/scene_ir/mod.rs`
- new `compiler/acceleration/build.rs`
- `compiler/query_plan/mod.rs`
- `compiler/tests/acceleration.rs`
- new `compiler/tests/acceleration_forest.rs`

**Implementation notes**

Start by building the logical forest contract.
Backend-specific flattening for CPU or WGSL should remain a later lowering step.

The forest should include:

- a world root over all candidate shapes
- optional per-shape subtree roots for large unions
- conservative support bounds for each node
- child spans stored in flat arrays
- leaf payloads that point back to `SmolStr` shape ids and semantic roots

Deterministic ordering matters.
Do not use hash-map iteration order for child layout.

**Acceptance criteria**

- The compiler can derive an acceleration forest from representative scenes.
- Forest dumps are deterministic across runs.
- Every leaf in the forest maps back to a valid shape/subtree owner.
- Large unions can be represented without expanding execution-time recursion.

#### Task 36A2 — Add conservative ray/support interval solving

**Description**

Implement explicit ray-entry and ray-exit interval tests for support nodes.

**Files**

- `compiler/acceleration/mod.rs`
- `compiler/acceleration/build.rs`
- `compiler/query_exec/cpu.rs`
- `compiler/tests/acceleration_forest.rs`

**Implementation notes**

Start with:

- AABB support
- sphere support

Then wrap them through:

- rigid transforms
- uniform scale
- repeat cell transforms where the cell range is known

Recommended record:

```rust
pub struct RaySupportInterval {
    pub hit: bool,
    pub t_enter: f32,
    pub t_exit: f32,
    pub starts_inside: bool,
}
```

When a ray misses the support, the whole node is rejected.
When a ray hits the support, traversal should start at `max(current_t, t_enter)` instead of always at `0`.

**Acceptance criteria**

- CPU tests cover miss, tangent, inside, transformed, and repeated-cell cases.
- World traversal can start at support entry rather than blindly marching from the origin.
- Correctness is conservative: false positives are allowed, false negatives are not.
- Reports expose entry-jump counts and support-interval rejections.

#### Task 36A3 — Replace linear world candidate traversal with hierarchical traversal on CPU

**Description**

Change the world-trace path so it walks the acceleration forest instead of linearly visiting every shape.

**Files**

- `compiler/query_exec/cpu.rs`
- `compiler/query_exec/world.rs`
- `compiler/query_solver/mod.rs`
- `compiler/tests/query_exec.rs`

**Implementation notes**

Today `consider_world_trace_shape` is called from a linear shape enumeration.
Refactor world traversal so that:

1. the forest is traversed first
2. only leaf candidates that survive interval/bound tests call `solve_shape_ray`
3. traversal prunes any node whose conservative lower bound exceeds the current best hit

This should also support nearest-hit style world queries, not only presentation rays.

**Acceptance criteria**

- CPU world queries use hierarchical traversal when a forest is available.
- Candidate counts measurably drop on dense constructive and repetition-heavy benchmarks.
- Dense oracle parity is maintained.
- Reports show node visits, leaf visits, and pruned nodes.

### Workstream B: Large-Union Closure

#### Task 36B1 — Replace linear union distance evaluation with subtree traversal

**Description**

Stop treating large unions as “evaluate every child every time.”

**Files**

- `compiler/query_exec/cpu.rs`
- `compiler/scene_ir/mod.rs`
- `compiler/semantic_evidence/mod.rs`
- `compiler/tests/query_exec.rs`

**Implementation notes**

This is the single most important structural refactor in the shape evaluator.

For union-like nodes:

- build a local acceleration subtree
- use conservative support lower bounds to prune children
- short-circuit on current best distance where legal
- preserve winner identity and provenance

Do not weaken identity semantics.
If a union acceleration path cannot preserve the current winner contract, it must fall back.

**Acceptance criteria**

- Large unions no longer require O(N) child evaluation per distance sample when acceleration data exists.
- Winner identity is preserved.
- Dense oracle parity tests pass.
- Benchmarks show measurable branch-visit and field-sample reduction.

#### Task 36B2 — Add planner diagnostics for when acceleration is unavailable or rejected

**Description**

Make it obvious why a scene or subtree did not use acceleration.

**Files**

- `compiler/acceleration/report.rs`
- `compiler/query_solver/mod.rs`
- `compiler/tests/mixed_solver.rs`

**Implementation notes**

Examples of rejection reasons:

- unbounded support
- opaque boundary with insufficient conservative bounds
- identity-preservation risk
- unsupported transform
- unsupported repeat form
- artifact unavailable
- artifact invalid

**Acceptance criteria**

- Planner/solver reports list acceleration rejection reasons.
- Tests cover at least the major rejection classes.
- Diagnostics make it possible for a junior engineer to know what to fix next.

### Phase 36 Exit Criteria

- World traversal can use a hierarchical acceleration forest.
- Large unions can use subtree traversal instead of always evaluating all children.
- Rays can jump to support-entry intervals conservatively.
- Diagnostics explain why acceleration was or was not used.

---

# Phase 37: Deterministic Hybrid Solver Closure

## Goal

Make dense sphere tracing the fallback, not the default, by teaching the solver stack to use analytic, relaxed, derivative-guided, interval, and repeat-aware methods.

## Why this phase exists

A fast traversal layer is not enough.
The solver still needs to take fewer expensive steps once a candidate is selected.

### Workstream A: Step Certificates And Analytic Methods

#### Task 37A1 — Refactor `trace_shape` into a step-certificate loop

**Description**

Turn the current dense loop into a solver-driven loop that advances only through explicit step certificates.

**Files**

- `compiler/query_exec/cpu.rs`
- `compiler/query_solver/mod.rs`
- `compiler/tests/query_exec.rs`
- new `compiler/tests/step_certificates.rs`

**Implementation notes**

Add records such as:

```rust
pub enum StepCertificateKind {
    DenseDistanceBound,
    SupportEntryJump,
    AnalyticHit,
    RelaxedConservativeJump,
    LipschitzBoundedJump,
    IntervalNoRootProof,
    RefinementBracket,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CertificateReuseClass {
    RenderingOnly,
    RenderingAndCollision,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RayStepCertificateMetadata {
    pub guarantee: RequiredGuaranteeClass,
    pub proof_family: SmolStr,
    pub subject: SmolStr,
    pub subject_kind: AccelerationSubjectKind,
    pub tolerance_context: SmolStr,
    pub reusable_by: CertificateReuseClass,
    pub invalidation_reasons: Vec<SmolStr>,
}

pub struct RayStepCertificate {
    pub kind: StepCertificateKind,
    pub metadata: RayStepCertificateMetadata,
    pub t_start: f32,
    pub t_end: f32,
    pub no_hit_before_t_end: bool,
    pub bracket: Option<[f32; 2]>,
    pub provenance: Option<CertificateProvenanceHandle>,
}
```

The loop should become:

1. ask the selected method for a proposal
2. verify / encode the certificate
3. advance or refine
4. fall back explicitly when verification is absent or rejected

Dense stepping is just one certificate kind.
The certificate metadata is what later allows collision to reuse only the certificates that actually justify collision-safe reuse.

**Acceptance criteria**

- The CPU shape trace loop is driven by explicit certificates.
- Dense behavior is preserved when only dense certificates are enabled.
- New observability can attribute which certificate kinds were used.
- Certificates record enough metadata to explain reuse, invalidation, and guarantee class.
- Tests cover mixed-method traces and fallback behavior.

#### Task 37A2 — Expand analytic primitive intersection under safe transforms

**Description**

Grow the current sphere-only analytic path into a real production primitive set.

**Files**

- `compiler/query_exec/cpu.rs`
- `compiler/semantic_evidence/mod.rs`
- `compiler/query_solver/mod.rs`
- `compiler/tests/query_exec.rs`

**Implementation notes**

Start with primitives already in `FieldPrimitive` where correctness is straightforward:

- sphere
- plane
- slab / box
- capsule
- cylinder
- cone / capped cone only after dedicated review
- torus / ellipsoid only after explicit correctness sign-off

Support these through:

- translate
- rotate
- uniform scale

Implementation pattern:

1. hoist the transform onto the ray
2. solve in local space
3. convert hit back to world-space result
4. verify residual and identity

**Code sketch**

```rust
fn solve_transformed_box(
    ray: RayQuery,
    transform: Affine3,
    half_extents: [f32; 3],
) -> Option<Hit3> {
    let local_ray = ray.transform_by(transform.inverse());
    let t = intersect_aabb(local_ray, [-half_extents[0], -half_extents[1], -half_extents[2]],
                                    [ half_extents[0],  half_extents[1],  half_extents[2]])?;
    Some(reconstruct_hit_in_world(ray, local_ray, t, transform))
}
```

**Acceptance criteria**

- The representative primitive set has analytic solver support.
- Transformed analytic hits preserve hit identity and payload rules.
- CPU oracle comparisons pass against dense behavior.
- Reports distinguish direct analytic hits from analytic-hit verification fallbacks.

### Workstream B: Fewer Steps Near The Surface

#### Task 37B1 — Add conservative relaxed stepping

**Description**

Implement a safe relaxed-stepping method that can advance farther than the raw distance bound when the certificate conditions hold.

**Files**

- `compiler/query_exec/cpu.rs`
- `compiler/query_solver/mod.rs`
- `compiler/tests/step_certificates.rs`
- benchmark coverage in `benchmarks/realtime_presentation/`

**Implementation notes**

This is not “guess a bigger step.”
It is “compute a larger conservative jump under explicit conditions.”

Start with a CPU implementation that uses:

- previous step history
- current distance
- optional local bound data
- immediate fallback to dense stepping when the certificate fails

This is the right place to encode over-relaxation and later automatic step-size tuning.

**Acceptance criteria**

- Relaxed stepping exists behind an explicit solver method.
- If the certificate cannot be justified, the loop falls back immediately.
- Dense oracle parity is preserved on thin-stack and grazing-ray fixtures.
- Benchmarks show a reduction in average step count on supported scenes.

#### Task 37B2 — Add derivative-guided refinement and adaptive epsilon

**Description**

Use certified gradients when available to reduce late-stage step waste and stabilize normals/hits.

**Files**

- `compiler/semantic_evidence/mod.rs`
- `compiler/query_exec/cpu.rs`
- `compiler/query_exec/world.rs`
- `compiler/tests/query_exec.rs`

**Implementation notes**

Build directly on the differential evidence work from RFC 0006.

Add:

- certified gradient access in the solver path
- adaptive epsilon from ray distance, local scale, and gradient magnitude
- safeguarded Newton or bisection/Newton hybrid refinement once a hit bracket exists

If gradient facts are unavailable or weak, stay conservative and fall back.

**Acceptance criteria**

- The solver uses certified gradients when evidence says it may.
- Refinement is bracketed and safeguarded.
- Adaptive epsilon usage is observable.
- Normal/hit parity stays within declared tolerances.

### Workstream C: Pathological Rays And Repeat-Aware Traversal

#### Task 37C1 — Add interval or affine no-root proofs for hard rays

**Description**

Implement a CPU-first rigorous fallback for rays that stall, graze, or enter semantically uncertain regions.

**Files**

- `compiler/query_solver/mod.rs`
- `compiler/query_exec/cpu.rs`
- new `compiler/tests/interval_solver.rs`

**Implementation notes**

Good first implementations:

- interval Newton contraction over ray intervals
- affine bounds over `f(ray(t))`
- no-root proof over `[t0, t1]` for conservative skipping
- bracket isolation for subsequent Newton refinement

Keep this CPU-first until the behavior is convincing.
It is acceptable for WGSL to keep dense fallback longer.

**Acceptance criteria**

- At least one rigorous interval-style fallback exists.
- Pathological fixtures pass against dense oracle behavior.
- Reports expose interval proof successes, subdivisions, and fallbacks.
- The solver can use interval methods only for the rays that need them.

#### Task 37C2 — Add repeat-aware ray-space traversal

**Description**

Traverse repeated structure in ray space instead of evaluating modulo-heavy distance code every step.

**Files**

- `compiler/scene_ir/mod.rs`
- `compiler/query_solver/mod.rs`
- `compiler/query_exec/cpu.rs`
- `compiler/tests/query_exec.rs`

**Implementation notes**

Start with the repeat forms already modeled in Scene IR.

Capabilities:

- transform the ray into repeat-local space
- enumerate plausible cells along a ray interval
- skip empty bounded cells
- preserve `repeat_id` and `instance_id`
- fall back when identity or support is unclear

**Acceptance criteria**

- Repeat-aware traversal exists for the first supported repeat subset.
- Repetition-heavy benchmarks show reduced field samples.
- Winner identity remains correct.
- Unsupported repeat forms fall back explicitly with diagnostics.

### Phase 37 Exit Criteria

- The CPU solver loop is certificate-driven.
- Analytic transformed primitives cover the first meaningful primitive subset.
- Conservative relaxed stepping exists.
- Derivative-guided refinement exists where evidence supports it.
- Interval fallback exists for hard rays.
- Repeat-aware traversal exists for a useful supported subset.

---

# Phase 38: WGSL And Presentation Scheduling Closure For `realtime_120`

## Goal

Move the new traversal and solver structure into generated WGSL, then wire presentation scheduling around it without relying on non-portable subgroup assumptions.

## Why this phase exists

CPU-first is the right way to prove semantics, but 120 FPS lives or dies in the GPU path.

### Workstream A: WGSL Data Layout And Pipeline Stability

#### Task 38A1 — Split WGSL bindings by update frequency and freeze explicit pipeline layouts

**Description**

Refactor the current WGSL resource layout so large static acceleration data is not mixed with per-dispatch and per-pass state.

**Files**

- `compiler/query_exec/wgsl.rs`
- `compiler/query_exec/wgsl/codegen.rs`
- `compiler/presentation_exec/wgsl.rs`
- `compiler/portable/abi.rs`
- `compiler/tests/presentation_exec.rs`

**Implementation notes**

Recommended bind-group split:

- **group 0**: frame / dispatch / small immutable scalars
- **group 1**: static scene + acceleration forest + shape metadata
- **group 2**: pass-local inputs/outputs and work queues
- **group 3**: temporal continuation seeds and derived caches

Keep layouts explicit and shared.
These layouts should be explicit lowerings of the logical acceleration and artifact contracts introduced earlier, not an alternate source of truth.
Do not use pipeline `layout: 'auto'` for common compute paths because explicit layouts and bind-group reuse are the right long-term pattern when multiple pipelines share the same data shape.

**Code sketch**

```wgsl
@group(0) @binding(0) var<storage, read> dispatch_config: DispatchConfig;
@group(1) @binding(0) var<storage, read> accel_nodes: array<AccelNode>;
@group(1) @binding(1) var<storage, read> accel_children: array<u32>;
@group(1) @binding(2) var<storage, read> shape_meta: array<ShapeMeta>;
@group(2) @binding(0) var<storage, read> rays: array<RayQuery>;
@group(2) @binding(1) var<storage, read_write> hits: array<Hit3>;
@group(3) @binding(0) var<storage, read> continuation: array<ContinuationSeed>;
```

**Acceptance criteria**

- Generated WGSL uses explicit reusable layouts for the shared compute paths.
- ABI/layout records are traceable back to the logical artifact contracts.
- Large acceleration data is not packed into tiny per-dispatch structs.
- Pipeline reuse is preserved across compatible compute passes.
- Existing WGSL tests continue to pass.

#### Task 38A2 — Keep acceleration data in storage buffers and size limits explicit

**Description**

Use storage buffers for the large acceleration arrays and request only the limits the engine actually needs.

**Files**

- `compiler/query_exec/wgsl.rs`
- `compiler/presentation_exec/wgsl.rs`
- `compiler/bin/wrela/perf_engine.rs`

**Implementation notes**

Acceleration forests, child spans, candidate lists, work queues, and brick tables belong in storage buffers.

Do not inflate requested limits casually.
Use the closure contract to request only what the selected artifact layouts need.

This task should also add layout-signature reporting so artifact layouts and requested GPU limits stay visible in diagnostics.

**Acceptance criteria**

- Large acceleration structures are backed by storage-buffer layouts.
- Requested GPU limits are derived from actual layout needs.
- Perf/diagnostic output prints requested-vs-used layout sizes.
- Validation rejects artifact layouts that exceed the active hardware profile.

### Workstream B: WGSL Traversal And Solver Parity

#### Task 38B1 — Generate acceleration-forest traversal in WGSL for world and batch ray queries

**Description**

Replace the linear shape loop in the hot WGSL world/batch ray paths with forest traversal when a compatible artifact exists.

**Files**

- `compiler/query_exec/wgsl/codegen.rs`
- `compiler/query_exec/wgsl.rs`
- `compiler/tests/query_exec.rs`
- `compiler/tests/presentation_exec.rs`

**Implementation notes**

The generated shader should:

1. traverse the world root
2. apply support interval rejection and entry jumps
3. visit leaf candidates only as needed
4. preserve current best hit and identity
5. fall back to existing dense behavior when the artifact is unavailable or incompatible

WGSL does not need recursion.
Flattened stackless or bounded-stack traversal is acceptable.

**Acceptance criteria**

- Generated WGSL uses acceleration traversal for supported world/batch ray paths.
- Dense fallback remains available.
- CPU/WGSL parity tests pass on representative closure scenes.
- Reports show WGSL traversal metrics, not only dense-step metrics.

#### Task 38B2 — Generate solver-specific WGSL fast paths only from solver plans

**Description**

Teach WGSL codegen to consume `RaySolverPlan` selections instead of rebuilding ad hoc behavior in the backend.

**Files**

- `compiler/query_exec/wgsl/codegen.rs`
- `compiler/query_exec/wgsl.rs`
- `compiler/query_solver/mod.rs`
- `compiler/tests/mixed_solver.rs`

**Implementation notes**

Representative first WGSL fast paths:

- support-entry jump
- analytic sphere / plane / box
- conservative relaxed step
- gradient-based refinement if a stable lowering exists

Do not sneak in backend-only heuristics that are not reflected in solver plans.

**Acceptance criteria**

- WGSL codegen follows solver-plan selections for at least the first representative method set.
- Dense fallback remains explicit in reports.
- CPU/WGSL parity exists for the supported method subset.
- Backend behavior no longer depends on hidden special cases.

### Workstream C: Presentation Scheduling And Divergence Control

#### Task 38C1 — Add per-tile candidate tables derived from the shared acceleration forest

**Description**

Upgrade presentation from projected-bounds tile culling to observer-local tile candidate tables with conservative candidate lists.

**Files**

- `compiler/presentation_plan/mod.rs`
- `compiler/presentation_exec/mod.rs`
- `compiler/presentation_exec/cpu.rs`
- `compiler/presentation_exec/wgsl.rs`
- `compiler/tests/presentation_exec.rs`

**Implementation notes**

Current tile culling is already useful.
This task should keep it, but strengthen it into an observer-local candidate-table artifact:

- map tile or cluster → candidate-set span
- drive primary visibility from that span
- keep full fallback to global world traversal

This is a scheduling optimization, not a semantic change.

**Acceptance criteria**

- Presentation can consume a tile candidate artifact.
- The tile candidate artifact is clearly marked as observer-local rather than shared snapshot-scoped state.
- Primary visibility work items per tile drop on dense scenes.
- Correctness stays conservative.
- Reports show tile candidate reduction and tile-cull efficiency separately.

#### Task 38C2 — Add packet/work-queue scheduling without subgroup assumptions

**Description**

Batch rays by state and candidate span to reduce divergence as observer-local scheduling policy, but do not make correctness depend on subgroup topology.

**Files**

- `compiler/presentation_exec/wgsl.rs`
- `compiler/query_exec/wgsl/codegen.rs`
- `compiler/presentation_exec/resources.rs`
- `compiler/tests/presentation_exec.rs`

**Implementation notes**

Good first batches:

- rays with no candidates
- rays in analytic-only candidate spans
- rays needing dense/interval fallback
- compacted hit pixels for late passes

Do not assume that subgroup ids map to local invocation indices.
Any subgroup usage must be optional and used only where non-portability is acceptable.

**Acceptance criteria**

- Presentation can execute through work queues or packets on the WGSL path.
- Divergence-sensitive passes show measurable improvement on representative scenes.
- The implementation remains correct without subgroup-specific features.
- Diagnostics make it clear whether packetization was active.

#### Task 38C3 — Add adapter-aware workgroup-size selection

**Description**

Stop treating one workgroup size as globally correct.

**Files**

- `compiler/query_exec/wgsl.rs`
- `compiler/presentation_exec/wgsl.rs`
- `compiler/bin/wrela/perf_engine.rs`
- `compiler/tests/presentation_exec.rs`

**Implementation notes**

Allow a small approved set, for example 32 / 64 / 128, then pick per hardware profile by benchmark.
Store the chosen value in the closure report.

This should remain bounded and explicit.
No “auto-tune forever” runtime behavior.

**Acceptance criteria**

- Workgroup size is selected from an explicit legal set.
- Benchmarks can compare configurations.
- The chosen size is reported in perf diagnostics.
- Validation rejects sizes incompatible with the active limits profile.

#### Task 38C4 — Tighten `realtime_120` quality control around the new acceleration artifacts

**Description**

Make `realtime_120` quality reports explain not only degradations but also which acceleration artifacts were active.

**Files**

- `compiler/presentation_contract/mod.rs`
- `compiler/presentation_exec/controller.rs`
- `compiler/presentation_exec/cost.rs`
- `compiler/tests/presentation_exec.rs`

**Implementation notes**

For `realtime_120`, reports should distinguish:

- native vs reduced internal resolution
- tile candidate table on/off
- packet scheduling on/off
- hit compaction on/off
- derived cache use on/off
- primary solver method mix
- bottleneck pass after all of the above

**Acceptance criteria**

- `realtime_120` reports name active acceleration artifacts explicitly.
- Adaptive control uses only legal degradation steps.
- Acceleration wins are distinguished from quality reductions in reports.
- Existing quality-contract tests continue to pass.

### Phase 38 Exit Criteria

- WGSL world/batch ray paths can use acceleration-forest traversal.
- Presentation can use per-tile candidate tables.
- Work-queue/packet scheduling exists without relying on non-portable subgroup behavior.
- `realtime_120` reports clearly separate acceleration from degradation.

---

# Phase 39: Derived Spatial Caches And Far-Field Acceleration

## Goal

Move far-field and repeated procedural work off the inner loop through explicit derived artifacts while preserving exact or conservative narrow-phase behavior.

## Why this phase exists

Hierarchical traversal and better solver methods get the engine much farther.
For hard scenes and 1080p120 closure, the next jump comes from not reevaluating the full procedural world in the far field.

### Workstream A: Brick And Narrow-Band Cache Artifacts

#### Task 39A1 — Add `DistanceBrickCache` and `SupportBrickCache` artifact schemas

**Description**

Introduce explicit shared artifact schemas for sparse bricks storing conservative support information and optional narrow-band distance samples.

**Files**

- `compiler/query_plan/mod.rs`
- `compiler/artifact_contract/mod.rs`
- `compiler/artifact_store/mod.rs`
- new `compiler/acceleration/cache.rs`
- `compiler/tests/artifact_store.rs`

**Implementation notes**

Recommended schema fields:

- brick dimensions
- voxel size
- narrow-band width
- semantic source root
- conservative / approximate / exact narrow-band flag
- artifact scope (`shared_snapshot_scoped`)
- layout signature
- snapshot compatibility scope
- update granularity / rebuild mode

Do not treat cache blobs as untyped bytes.
Give them named logical fields.

**Code sketch**

```rust
pub struct DistanceBrickCacheSchema {
    pub brick_edge: u32,
    pub voxel_size: f32,
    pub narrow_band_width: f32,
    pub semantic_root: u32,
    pub conservative_empty_space: bool,
    pub exact_narrow_band: bool,
}
```

**Acceptance criteria**

- Brick cache artifact schemas exist and are versioned.
- Artifact validity includes layout-signature and snapshot checks.
- Artifact schemas declare their scope explicitly as shared snapshot-scoped artifacts.
- Tests cover artifact compatibility and invalidation.
- Plan dumps show cache artifact usage clearly.

#### Task 39A2 — Build sparse brick caches from snapshots and supports

**Description**

Implement the builder that materializes cache artifacts from world snapshots.

**Files**

- new `compiler/acceleration/cache.rs`
- `compiler/artifact_store/mod.rs`
- `compiler/tests/acceleration_forest.rs`

**Implementation notes**

Start CPU-first.
Use support bounds to limit brick generation.
Do not sample the entire world blindly.
This phase must also define the runtime budget policy for cache generation and upload.

Recommended first pass:

- conservative support occupancy per brick
- optional narrow-band distance brick for presentation
- deterministic brick ordering for stable diffs
- explicit build budget class
- explicit upload budget class
- fallback behavior when a build or upload budget is exhausted

**Acceptance criteria**

- Sparse brick caches can be built for representative scenes.
- Brick generation is bounded by support data.
- Artifacts are deterministic for a fixed snapshot.
- Reports show brick counts, memory footprint, build cost, and upload size.

### Workstream B: Hybrid Cache Tracing

#### Task 39B1 — Add a CPU hybrid solver that uses bricks for far-field traversal and exact refinement near hits

**Description**

Use cache traversal to skip empty space, then switch back to exact/analytic methods near the surface.

**Files**

- `compiler/query_exec/cpu.rs`
- `compiler/query_solver/mod.rs`
- `compiler/tests/query_exec.rs`
- `benchmarks/realtime_presentation/`

**Implementation notes**

The rule should be:

- cache may accelerate empty-space traversal
- exact or conservative narrow-phase solver still owns the final hit
- if the cache cannot justify the next move, fall back immediately
- if cache budget pressure disables the path, fallback must be explicit and reported

This keeps the cache as an acceleration artifact, not a semantic authority.

**Acceptance criteria**

- CPU hybrid tracing exists for supported cache artifacts.
- Final hit parity matches dense oracle behavior.
- Benchmarks show reduced far-field field sampling.
- Reports show cache hits, cache misses, fallback reasons, and whether budget pressure disabled the cache path.

#### Task 39B2 — Add WGSL cache traversal for presentation primary visibility

**Description**

Port the hybrid cache path to generated WGSL for view execution.

**Files**

- `compiler/query_exec/wgsl/codegen.rs`
- `compiler/presentation_exec/wgsl.rs`
- `compiler/tests/presentation_exec.rs`

**Implementation notes**

Keep the WGSL path simple first:

- read cache brick metadata from storage buffers
- use conservative empty-space skipping
- switch to the exact/analytic solver for the final narrow phase

Avoid texture-only special casing unless it clearly wins on the target hardware profile.
Keep cache upload and residency accounting visible in the diagnostics so frame pacing problems are attributable.

**Acceptance criteria**

- Presentation WGSL can consume the first brick-cache artifact.
- CPU/WGSL parity passes on supported cache scenes.
- Perf reports show when cache traversal was actually used.
- Perf reports show cache residency/upload usage.
- Dense fallback remains available.

### Workstream C: View-Local And Temporal Cache Reuse

#### Task 39C1 — Add a presentation-only view distance clipmap artifact

**Description**

Introduce a camera-centered distance/support clipmap for far-field presentation acceleration.

**Files**

- `compiler/presentation_plan/mod.rs`
- `compiler/presentation_exec/mod.rs`
- `compiler/presentation_exec/resources.rs`
- new `compiler/acceleration/clipmap.rs`
- `compiler/tests/presentation_exec.rs`

**Implementation notes**

This artifact is presentation-specific because it is view-centered and temporal.
It should still go through normal artifact validity and layout-signature rules.
It must also declare explicit update budgets, residency policy, and fallback behavior under camera motion.

Use it only for:

- far-field primary traversal
- conservative candidate/tile generation
- optional shadow or occlusion acceleration later

Do not make collision depend on a view-centered cache.

**Acceptance criteria**

- A view-local clipmap artifact exists for presentation.
- Its validity and update rules are explicit.
- Reports show clipmap resolution, updates, usage, build/upload cost, and eviction behavior.
- Presentation falls back cleanly when the clipmap is absent or invalid.

#### Task 39C2 — Add explicit cache fallback and stale-cache diagnostics

**Description**

Make cache invalidation and fallback behavior impossible to misunderstand.

**Files**

- `compiler/artifact_store/mod.rs`
- `compiler/acceleration/report.rs`
- `compiler/presentation_exec/cost.rs`
- `compiler/tests/artifact_store.rs`

**Implementation notes**

Examples of rejection reasons:

- snapshot mismatch
- layout mismatch
- unsupported quality tier
- insufficient narrow-band coverage
- artifact version mismatch
- memory budget exceeded
- build budget exhausted
- upload budget exhausted

**Acceptance criteria**

- Cache rejection reasons are explicit in reports.
- Fallback to non-cache execution is correct and observable.
- Tests cover stale and incompatible artifacts.
- Motion-path fixtures cover stale-cache, rebuild-pressure, and budget-pressure behavior.
- No cache path silently changes semantics or frame-pacing policy.

### Phase 39 Exit Criteria

- Sparse brick cache artifacts exist and are versioned.
- CPU and WGSL can use hybrid far-field cache traversal.
- Presentation can use a view-local clipmap artifact.
- Cache invalidation, budget policy, and fallback are explicit and tested.

---

# Phase 40: Collision Acceleration Parity And Witness Reuse

## Goal

Make collision benefit from the same acceleration spine so rendering speedups do not come at collision’s expense.

## Why this phase exists

The repo already has better collision architecture than most renderers.
This phase makes sure it also gets the same performance discipline.

### Workstream A: Shared Broadphase

#### Task 40A1 — Rebuild collision broadphase on top of the shared acceleration forest

**Description**

Replace or augment today’s broadphase candidate gathering with forest traversal and support intervals.

**Files**

- `compiler/collision_exec/cpu.rs`
- `compiler/collision_plan/mod.rs`
- `compiler/acceleration/build.rs`
- `compiler/tests/collision_exec.rs`

**Implementation notes**

This should serve:

- point occupancy
- world ray cast
- sphere overlap
- sphere sweep first contact
- sphere time of impact

Use the same conservative support interval logic as the presentation/query path where it applies.

**Acceptance criteria**

- Collision broadphase can consume the shared acceleration forest.
- Candidate counts drop on representative collision scenes.
- Point/ray/overlap/sweep/TOI fixtures preserve correctness.
- Reports expose broadphase rejection metrics.

#### Task 40A2 — Add sweep and TOI bracket reuse from shared solver certificates

**Description**

Reuse the step/bracket/certificate concepts from the ray solver in sweep and time-of-impact queries.

**Files**

- `compiler/collision_exec/cpu.rs`
- `compiler/query_solver/mod.rs`
- `compiler/tests/collision_exec.rs`

**Implementation notes**

A sweep is not a view ray, but it still benefits from:

- conservative interval rejection
- bracketed contact intervals
- safeguarded refinement
- dense fallback when proof is absent

The code should reuse concepts, not copy-paste new ad hoc ones.
Collision should consume only certificates whose metadata explicitly permits collision-safe reuse.

**Acceptance criteria**

- Sweep and TOI execution can use bracket/certificate concepts from the shared solver model.
- Existing collision correctness is preserved.
- Collision rejects rendering-only certificates explicitly.
- Reports expose interval brackets, refinements, and fallback rates.
- Collision code duplication is reduced.

### Workstream B: Witness And Continuation Reuse

#### Task 40B1 — Expand witness artifacts with conservative separation and bracket metadata

**Description**

Make witness caches materially useful to acceleration rather than just result reuse.

**Files**

- `compiler/collision_plan/mod.rs`
- `compiler/collision_exec/cpu.rs`
- `compiler/artifact_store/mod.rs`
- `compiler/tests/collision_exec.rs`

**Implementation notes**

Recommended witness additions:

- separation upper/lower bounds
- last valid bracket interval
- supporting feature classification
- normal flavor
- transition compatibility summary

This lets later frames or later queries start from a better state.

**Code sketch**

```rust
pub struct CollisionWitnessSeed {
    pub subject: SmolStr,
    pub separation_lower_bound: f32,
    pub separation_upper_bound: f32,
    pub last_contact_fraction: Option<f32>,
    pub bracket: Option<[f32; 2]>,
    pub normal_flavor: CollisionContactNormalFlavor,
}
```

**Acceptance criteria**

- Witness artifacts carry useful acceleration metadata.
- Artifact validity rules cover transition compatibility.
- Sweep/TOI paths can consume the new witness data.
- Reports show reuse rates and rejection reasons.

#### Task 40B2 — Add collision continuation seeding from previous successful witnesses

**Description**

Use compatible earlier witnesses as legal continuation seeds.

**Files**

- `compiler/collision_exec/cpu.rs`
- `compiler/collision_plan/mod.rs`
- `compiler/tests/collision_exec.rs`

**Implementation notes**

This should mirror the already-existing continuation idea in presentation/query execution:

- compatible transition only
- explicit rejection reasons
- no “previous frame exists” shortcuts

**Acceptance criteria**

- Collision continuation seeds are used only when validity rules allow it.
- Diagnostics explain why a seed was consumed or rejected.
- Collision performance improves on transition-heavy scenarios.
- Correctness is preserved under incompatible transitions by falling back.

### Workstream C: Contact Semantics And Benchmarks

#### Task 40C1 — Finish the split between certified contact normals and shading normals in collision outputs

**Description**

Carry the internal normal-role split all the way through collision acceleration and output materialization.

**Files**

- `compiler/collision_contract/mod.rs`
- `compiler/collision_exec/cpu.rs`
- `compiler/query_exec/world.rs`
- `compiler/tests/collision_exec.rs`

**Implementation notes**

Collision must know whether it is using:

- certified field gradient
- feature normal
- fallback heuristic

Do not reuse the rendering notion of “normal” casually.

**Acceptance criteria**

- Collision outputs can distinguish contact-normal provenance.
- Acceleration paths do not blur heuristic normals with certified ones.
- Diagnostics can print which normal flavor was used.
- Existing public collision outputs remain compatible where intended.

#### Task 40C2 — Add dedicated collision performance benchmarks

**Description**

Create a benchmark suite that measures collision throughput directly.

**Files**

- new `benchmarks/collision_perf/`
- `benchmarks/README.md`
- `compiler/bin/wrela/perf_engine.rs`

**Implementation notes**

Include scenarios for:

- many point occupancy probes
- dense ray casts
- overlap bursts
- repeated sweeps through static clutter
- TOI under transition reuse

Report:

- queries/sec
- average candidate counts
- average interval proofs/refinements
- witness reuse rate
- fallback rate

**Acceptance criteria**

- A dedicated collision benchmark suite exists.
- Perf reports explain why throughput changed.
- Collision is included in performance-closure reporting.
- Rendering-only wins can no longer hide collision regressions.

### Phase 40 Exit Criteria

- Collision broadphase can consume the shared acceleration forest.
- Sweep and TOI use shared bracket/certificate concepts.
- Witness caches materially improve continuation.
- Dedicated collision throughput benchmarks exist.

---

# Phase 41: Benchmark Gates, Shipping Closure, And Optional Learned Artifacts

## Goal

Turn the performance work into a shippable closure process, then provide a safe sandbox for optional learned methods.

## Why this phase exists

Once the deterministic stack is real, the project needs hard gates, documentation, and a disciplined place for experimental solver work.

### Workstream A: Closure Gates

#### Task 41A1 — Add explicit 1080p120 and collision closure gates to perf tooling

**Description**

Turn the closure contract into a repeatable pass/fail report.

**Files**

- `compiler/bin/wrela/perf_engine.rs`
- `compiler/bin/wrela/commands/test_eval_perf.rs`
- `benchmarks/README.md`
- CI or local automation scripts if present

**Implementation notes**

The report should include:

- frame median / p95
- primary visibility median / p95
- active internal resolution scale
- active degradations
- active acceleration artifacts
- collision throughput delta vs baseline
- unresolved top bottleneck

Do not reduce this to a single number.

**Acceptance criteria**

- Perf tooling can state whether the closure target was met.
- Reports include both frame and collision closure signals.
- The top remaining bottleneck is named when closure fails.
- Developers can compare before/after runs deterministically.

#### Task 41A2 — Add a “why not 120?” report mode

**Description**

Provide a structured diagnostic mode for failure analysis.

**Files**

- `compiler/presentation_exec/cost.rs`
- `compiler/bin/wrela/perf_engine.rs`
- `compiler/acceleration/report.rs`

**Implementation notes**

The mode should answer questions like:

- were too many rays still dense?
- did tile candidate tables fail to prune?
- was WGSL still using linear traversal?
- were caches unavailable or invalid?
- was the frame bound by surface/participants/shading rather than visibility?
- did collision regress because witness reuse was invalid or unsupported?

**Acceptance criteria**

- A structured failure analysis report exists.
- It points to concrete subsystems rather than vague “slow” labels.
- A junior engineer can use the report to choose the next task confidently.

### Workstream B: Optional Learned Artifacts

#### Task 41B1 — Add experimental `LearnedStepProposal` and `ConservativeNeuralBound` method slots

**Description**

Create optional internal solver hooks for learned methods without making them part of the production-critical path.

**Files**

- `compiler/query_solver/mod.rs`
- `compiler/artifact_contract/mod.rs`
- `compiler/artifact_store/mod.rs`
- new `compiler/acceleration/learned.rs`
- `compiler/tests/mixed_solver.rs`

**Implementation notes**

Add methods such as:

- `LearnedStepProposal`
- `ConservativeNeuralBound`

These are **internal experimental methods**.
They must not be enabled in shipping quality tiers by default.

Their only legal modes are:

- proposal → verifier → fallback
- conservative bound with explicit no-false-negative policy

Do not allow best-effort learned methods on collision contracts.

**Acceptance criteria**

- Experimental learned method slots exist behind an internal feature flag.
- They are disabled by default.
- Solver reports show when they were selected, verified, rejected, or bypassed.
- No public contract depends on them.

#### Task 41B2 — Add a CPU-oracle dataset and verifier pipeline for learned experiments

**Description**

Build the minimal artifact pipeline needed for later ML experiments.

**Files**

- new `compiler/acceleration/learned.rs`
- `compiler/query_exec/cpu.rs`
- `compiler/artifact_store/mod.rs`
- new `compiler/tests/learned_solver.rs`

**Implementation notes**

Start with a compile-time/offline dataset builder that can sample:

- point → conservative distance
- point + direction → dense-oracle hit distance
- candidate support interval labels
- acceptance/rejection labels for proposed larger steps

Any learned proposal must be checked by a verifier before it can advance the ray.
If verification fails, fall back immediately.

**Acceptance criteria**

- A dataset/export path exists for internal experiments.
- Learned proposals cannot run without a verifier/fallback path.
- Collision contracts reject non-conservative learned methods.
- Reports expose verifier acceptance rate and fallback rate.

### Workstream C: Documentation And Handoff

#### Task 41C1 — Add engineering playbooks for the acceleration stack

**Description**

Document how a new engineer should debug traversal, solver, cache, and collision issues.

**Files**

- new `docs/perf/acceleration_playbook.md`
- `benchmarks/README.md`
- CLI help/docs if present

**Implementation notes**

Include:

- where to look for forest dumps
- how to read solver-plan reports
- how to read closure reports
- how to reproduce representative benchmark runs
- what “do not change without oracle parity” means in practice

**Acceptance criteria**

- A written playbook exists.
- It covers rendering and collision.
- It references actual report/dump commands.
- A junior engineer can follow it without tribal knowledge.

### Phase 41 Exit Criteria

- The repo has explicit closure gates for 1080p120 and collision throughput.
- Failure reports can explain why closure was not met.
- Experimental learned methods exist only behind verifier-backed internal flags.
- Engineering docs exist for the acceleration stack.

---

# Recommended Implementation Order Inside The Phases

Within the roadmap, the highest-value order is:

1. Phase 35A2 + 35B1 + 35C1
2. Phase 36A1 + 36A2 + 36A3
3. Phase 36B1
4. Phase 37A1 + 37A2
5. Phase 37B1 + 37B2
6. Phase 38A1 + 38B1
7. Phase 38C1 + 38C2
8. Phase 39A1 + 39B1
9. Phase 40A1 + 40B1
10. Phase 39B2 + 39C1
11. Phase 41 closure work
12. Phase 41 learned experiments last

That order gives the fastest route to real performance wins while keeping the architecture clean.

# Practical Repo Refactors This RFC Explicitly Endorses

The following refactors are welcome and likely necessary:

1. Introduce a new `compiler/acceleration/*` module family instead of scattering acceleration structs across query, presentation, and collision modules.
2. Refactor `compiler/query_exec/cpu.rs` so world traversal and shape traversal are no longer tightly coupled to linear iteration helpers.
3. Refactor `compiler/query_exec/wgsl/codegen.rs` so world traversal and solver method selection come from normalized plan inputs rather than repeated hand-built loops.
4. Extend artifact schemas rather than creating anonymous backend-private cache blobs.
5. Move repeated support/ray math helpers into shared modules if they are needed by query, presentation, and collision paths.
6. Add new benchmark directories and test modules freely; the current suite is too small to be the last word on closure.

# Hard Rules For Shipping

The engine should not call this roadmap complete if any of the following remain true:

- `realtime_120` succeeds only by hidden degradations not reported to the user
- collision throughput regresses materially and the perf report cannot explain it
- WGSL primary visibility still uses linear world traversal on supported closure scenes
- large unions still evaluate all children per step in supported accelerated scenes
- derived caches can silently go stale
- learned methods can affect correctness without a verifier/fallback path

# Final Position

The fastest believable path to “insane rendering perf” in this repo is not to gamble on one exotic algorithm.

It is to cash in the architectural advantages the repo already built:

- support-aware Scene IR
- semantic evidence
- solver plans
- presentation plans
- collision plans
- explicit artifacts
- CPU oracle discipline

The shared acceleration spine is the missing bridge between “the repo has the right ideas” and “the repo actually moves 1080p120 views and collision-heavy scenes at production speed.”

That is the bridge this RFC builds.
