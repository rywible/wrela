# RFC 0006: Certified World Snapshots, Temporal Semantics, Artifact Runtime, And Query-Program Spine Roadmap

Status: Proposed

Author: Codex

Created: 2026-04-12

Target: post-Phase-24 `wrela` architecture, compiler, runtime, observer plans, CPU oracle, virtual GPU, WGSL, and future gameplay-oriented world execution

## Summary

This document defines the architectural roadmap for taking Wrela from its current strong query-family and presentation foundations to the intended long-term end state:

- a certified world-query compiler
- immutable world snapshots with real epoch identity
- explicit temporal and transition semantics
- reusable semantic artifacts with principled validity
- a narrow shared query-program spine extracted only after multiple concrete observers prove the overlap
- CPU as the semantic oracle
- GPU, WGSL, and future backends as execution targets for compiler-owned meaning

This RFC is intentionally ambitious.

It is also intentionally sequenced.

The goal is not to jump straight to a giant universal execution abstraction.
The goal is to land the smallest architectural cuts that preserve meaning, unlock performance, and keep the codebase converging toward one coherent world model rather than a growing pile of special-case pipelines.

The central implementation rule of this RFC is:

- semantic world truth must be snapshot-addressable
- domains and policies must be distinct
- evidence must be first-class and compositional
- change over time must be modeled explicitly
- artifacts must be reusable only through declared validity rules
- shared query-program machinery should be promoted only after at least two concrete observers expose real overlap

## Relationship To Earlier RFCs And Repo Vision

This roadmap builds directly on:

- [RFC 0001](/Users/ryanwible/projects/wrela/language/spec/rfcs/0001-field-game-language.md)
- [RFC 0002](/Users/ryanwible/projects/wrela/language/spec/rfcs/0002-field-engine-implementation-roadmap.md)
- [RFC 0003](/Users/ryanwible/projects/wrela/language/spec/rfcs/0003-phase-9-5-semantic-convergence-plan.md)
- [RFC 0004](/Users/ryanwible/projects/wrela/0004-question-families-query-contracts-roadmap.md)
- [RFC 0005](/Users/ryanwible/projects/wrela/0005-realtime-presentation-view-plans-frame-contracts-roadmap.md)
- [AGENTS.md](/Users/ryanwible/projects/wrela/AGENTS.md)

`AGENTS.md` provides the right long-term north star:

- Wrela is becoming a field-native game engine
- the world is authored once and interrogated many ways
- rendering is one question among many
- CPU remains the semantic oracle
- semantic meaning should survive lowering
- local shortcuts that damage the substrate are not acceptable

RFC 0004 gave Wrela a canonical question-family model.
RFC 0005 made presentation a concrete observer over that substrate.

This RFC is the next architectural jump.

It answers the questions that are now blocking the long-term end state:

- what exactly is being queried: a live world or an immutable snapshot?
- what makes two frames or captures compatible for reuse?
- what is semantic policy versus execution policy?
- what evidence do planners and solvers actually reason from?
- how is change over time represented?
- when should shared query-program machinery become real?

This RFC does not replace RFC 0004 or RFC 0005.
It extends them toward the fully fledged engine architecture they imply.

## Current Repo Read

The repo is already strong enough that this roadmap is practical rather than speculative.

### What is already strong

1. `compiler/query_contract/mod.rs` already provides a real contract-oriented question substrate.
2. `compiler/query_plan/mod.rs` and `compiler/kernel/*` already carry explicit plan records, stages, artifacts, and observability.
3. `compiler/query_solver/mod.rs` already has the beginnings of solver facts, portfolios, and certificates.
4. `compiler/presentation_plan/mod.rs` and `compiler/presentation_contract/mod.rs` already treat presentation as an explicit observer with contracts for view, frame, attachments, temporal reuse, and quality.
5. `compiler/presentation_exec/resources.rs` already treats presentation outputs as typed resources rather than raw PPM-only buffers.
6. CPU, virtual GPU, and WGSL already share enough execution shape that semantic convergence is realistic.

### What is currently holding the architecture back

1. **Snapshot identity is not real yet.**
   Capture epochs still lower to `0` in several places, so one of the strongest architectural ideas in the system is still mostly a placeholder.

2. **Semantic domain and execution policy are still entangled.**
   `SceneDomain` is family-shaped now, but authored domain metadata still carries march-budget knobs that the execution path peels back out later.

3. **Evidence is fragmented.**
   `FieldFacts`, support facts, solver certificates, exactness classes, and provenance constraints exist, but not as one unified monotone evidence object.

4. **Temporal semantics are presentation-shaped rather than engine-shaped.**
   The repo has frame history, motion, and temporal reuse rules, but not yet a general model of transitions, change classes, or temporal identity.

5. **Artifacts are still split across categories.**
   Derived query artifacts, frame artifacts, and history slots are conceptually related but not yet one architectural system.

6. **Logical semantic products are still too tightly coupled to physical storage.**
   Attachment schemas are typed, but layout planning is still mostly bound to immediate byte-buffer allocation decisions.

7. **The same semantic distinctions still get restated in multiple lower layers.**
   Contracts are clean, but plans, kernel plans, world execution, WGSL codegen, and presentation still each carry their own compatibility-era shape.

8. **Differential semantics are mostly implicit and numerical.**
   Normals still rely heavily on finite differences, which is both a semantic ambiguity and a performance tax.

### What the code says about the next move

The codebase is asking for four things:

1. real snapshot and epoch semantics
2. hard separation of semantic policy from execution policy
3. unified evidence and validity
4. artifact and temporal systems that can answer reuse questions explicitly

The codebase is not yet asking for:

1. a giant fully generic universal execution IR
2. public user-authored arbitrary query programs
3. a four-dimensional authored field language as the immediate next cut

That distinction matters.

The right north star is broad.
The right implementation path is staged.

## Why This Comes Before Open-Ended Performance Closure

Wrela should absolutely push hard on real-time rendering performance.

But some architectural work must land before deep performance closure, or the optimization work will harden the wrong boundaries.

Without the cuts in this RFC, performance work is likely to ossify:

- budget knobs hidden inside semantic domain
- ad hoc history invalidation
- artifact reuse by convention rather than declared compatibility
- planner decisions made from scattered booleans instead of evidence
- backend-specific behavior reconstruction rather than normalized intent

This roadmap therefore aims to land the minimum architecture required for truth-preserving performance work, then to make that architecture the thing the performance work scales through.

## Architectural Thesis

The long-term shape of Wrela should be:

- world snapshots as immutable semantic truth
- transitions as first-class descriptions of change
- typed questions over snapshots and transitions
- evidence-aware planning
- reusable artifacts and witnesses with explicit validity
- concrete observer plans such as presentation, collision, tooling, and future gameplay systems
- a shared query-program spine promoted from concrete observers only after the overlap is proven

The engine is not best understood as:

- renderer + collision system + AI system + editor system

It is better understood as:

- a certified world compiler
- that answers typed questions about snapshots and transitions
- through concrete observers
- using evidence, policies, and reusable artifacts

## Goals

This roadmap has nine goals.

1. **Make world snapshots first-class and immutable.**
   Queries, artifacts, and history must target explicit snapshot identity.

2. **Separate semantic domain from execution policy.**
   Semantic enablement, detail bands, and legality constraints must not be conflated with ray budgets or backend preferences.

3. **Unify evidence into one compositional system.**
   Support, exactness, derivatives, provenance, temporal guarantees, and solver legality should live in one monotone evidence model.

4. **Make change over time and authoritative state advance first-class engine concepts.**
   Temporal identity, transition classification, state-advance semantics, and reuse validity must not live only inside the presentation loop.

5. **Unify artifacts as materialized semantic views.**
   Support summaries, culling tables, history buffers, witness buffers, and future acceleration products should share one logical architecture.

6. **Separate logical semantic products from physical storage layout.**
   Logical artifacts should describe meaning and compatibility; layout plans should describe bytes, textures, packing, and residency.

7. **Promote a shared query-program spine only after concrete overlap exists.**
   Presentation should remain concrete now; the shared layer should arrive after a second concrete observer proves the overlap.

8. **Lift differential semantics into the architecture.**
   Gradient-like queries and solver refinement should be evidence-driven rather than mostly finite-difference-driven.

9. **Keep CPU as the semantic oracle throughout.**
   Every performance path, backend path, and artifact reuse path must remain checkable against CPU meaning.

## Explicit Non-Goals

This roadmap does not do the following:

- user-authored arbitrary query-program kernels as a public surface
- plugin-loaded query-program families
- a fully generic executable universal plan that replaces concrete observers immediately
- public user-authored four-dimensional spacetime field syntax in the first phase
- coupling simulation time to presentation cadence as one hard architectural identity
- turning the collision observer into a full rigid-body physics, constraint, stacking, friction, or response system
- backend-specific semantics that bypass CPU oracle behavior

These may be revisited later, but they are not the right first cuts for the north-star architecture.

## Design Rules

Every phase in this RFC must follow these rules.

1. Snapshot identity must be explicit and typed.
2. Epoch is version identity, not a substitute for time.
3. Persistent lineage identity, snapshot-local identity, and authored/content identity must not collapse into one id type.
4. Domains carry semantic policy; execution policy carries cost/guarantee/backend choices.
5. Evidence must be monotone under refinement.
6. Evidence provenance must be explicit when refinement source affects planner legality.
7. Any approximation must be described by a typed legality class, not by a vague numeric flag.
8. Artifact keys are indexing aids; artifact validity predicates are the semantic truth.
9. Artifacts may be reused only through declared compatibility and validity checks.
10. Concrete observers remain concrete until shared overlap is proven by at least two serious observer plans.
11. Debug-only normalized projections are allowed early, but they must remain non-authoritative and non-executing.
12. CPU meaning lands before GPU specialization for every new architectural layer.
13. Tests must cover behavior, not just shapes.
14. Every phase must be parallelizable by workstream with disjoint ownership whenever feasible.

## Decisions Locked By This RFC

The following architectural questions are answered by this RFC now rather than deferred.

### Identity Is Layered

Wrela will use a layered identity model.

- `AuthoredContentId` identifies stable authored content lineage where such a concept exists.
- `EntityLineageId` identifies the persistent semantic thing across snapshots.
- `SnapshotEntityId` identifies the specific entity/version inside one snapshot.

This means:

- one authored thing may correspond to many runtime lineages through instancing or spawning
- one lineage may appear in many snapshots
- one snapshot entity id belongs to exactly one snapshot

No single id type may try to stand in for all three layers.

### Required Guarantee And Selected Method Class Are Separate

Wrela will separate:

- the guarantee class required by the contract or execution policy
- the method class selected by the planner or backend

The policy says what correctness envelope is allowed.
The planner says how it intends to satisfy that envelope.

This prevents values like "heuristic" from becoming ambiguous between:

- "the caller allows a best-effort answer"
- "the planner happened to pick a heuristic method"

### Artifact Validity Is Predicate-Based, Not Key-Equality-Based

Artifact keys exist for lookup efficiency.
They are not the semantic truth of validity.

Artifact reuse is valid only if:

1. the lookup key finds a candidate artifact
2. the compatibility relation says the candidate is eligible for consideration
3. the validity predicate says the artifact is still semantically legal to reuse

The first cut of artifact validity should be a small typed declarative rule algebra.
It should be serializable, inspectable, reportable, and testable.
Do not start with arbitrary callback logic scattered through the codebase.

### Evidence Provenance Is First-Class

Evidence carries both content and origin.

At minimum, the architecture distinguishes:

- statically compiled evidence
- runtime-observed evidence
- artifact-derived evidence
- imported compatibility evidence

Planner legality, reuse, and trust rules may depend on that origin.

### Authoritative State Advances By Deterministic Fixed-Tick Transition

Wrela will treat authoritative world evolution as deterministic fixed-step transition over simulation ticks.

At the internal architecture level:

- authoritative state advances from snapshot `S(t)` to snapshot `S(t+1)`
- that advance consumes a typed batch of tick-bound inputs and events
- that advance produces a new snapshot plus a full transition record
- change summaries used by planners are derived products of that transition record
- observer plans such as presentation do not authoritatively mutate world state

This RFC does not define the final public authored gameplay-state DSL.
It does define the internal architectural shape that later DSLs must target.

Ownership split:

- the **runtime** owns authoritative advancement, scheduling, and execution of the state-advance contract
- the **compiler** owns the types, legality model, lowering, and any authored-to-runtime compilation needed to target that contract

The compiler must not quietly become the owner of operational simulation machinery.
The runtime must not quietly invent semantic transition meaning outside the compiler-owned model.

## Key Architectural Definitions

### Snapshot

A **snapshot** is an immutable semantic world value that can be queried repeatedly.

It is not:

- a mutable scene graph
- a render frame
- an incidental host cache entry

It is the semantic truth for a point in authoritative world evolution.

**Code sketch**

```rust
pub struct WorldSnapshotHandle {
    pub snapshot_id: WorldSnapshotId,
    pub epoch: SnapshotEpoch,
    pub root_region: SnapshotEntityId,
}
```

### Epoch

An **epoch** is a version identifier for a snapshot lineage.

It is not:

- wall-clock time
- simulation tick by itself
- presentation frame number

It answers the question:

- "is this the same semantic world state as before?"

### Transition

A **transition** is a compiler-visible description of change between two snapshots.

Examples:

- no change
- rigid transform only
- lighting-only change
- material-only change
- topology-changing geometry change
- identity-breaking replacement

**Code sketch**

```rust
pub struct WorldTransition {
    pub from: WorldSnapshotHandle,
    pub to: WorldSnapshotHandle,
    pub change: ChangeSummary,
}
```

`WorldTransition` is the planner-facing summary form.
It is not necessarily the full authoritative transition record.

### Change Compatibility Lattice

Change classes should not be treated as only a flat taxonomy.

They should support an ordering that answers questions like:

- is change class `A` no worse than change class `B` for this artifact?
- does this planner or artifact accept any change up to some compatibility threshold?

That means long-term change compatibility should behave like a partial order or lattice, not only like scattered enum matches.

This lattice should have one obvious typed home.
Presentation, artifacts, collision, and solver code should import it rather than quietly re-expressing their own severity ladders.

### Clock Family

Wrela needs several typed clock-like concepts:

- `SimulationTick`
- `PresentationFrame`
- `WallClockStamp`
- `SnapshotEpoch`

They often move together.
They must not be modeled as the same thing.

### Identity Layers

Wrela needs at least two identity layers and likely three:

- **Authored/content identity**
  The stable authored thing or declaration lineage that exists across compiles and snapshots.

- **Persistent lineage identity**
  The runtime or semantic lineage that answers "is this still the same thing through time?"

- **Snapshot-local entity identity**
  The concrete entity/version inside one snapshot.

These are related.
They are not interchangeable.

If one id type tries to answer all three questions, temporal reasoning, artifact reuse, and provenance guarantees will become muddy.

### Authoritative State Advance

**Authoritative state advance** is the deterministic transition function that produces the next snapshot from the previous one.

The internal architecture should model:

- previous snapshot
- simulation tick
- typed tick-bound inputs and events
- next snapshot
- full transition record
- derived change summary for planners and artifacts

**Code sketch**

```rust
pub struct TickInputBatch {
    pub tick: SimulationTick,
    pub inputs: Vec<TickInputEvent>,
}

pub struct StateAdvanceResult {
    pub next_snapshot: WorldSnapshotHandle,
    pub transition_record: WorldTransitionRecord,
    pub change_summary: ChangeSummary,
}
```

Presentation, tooling, and query observers may consume snapshots and transitions.
They do not define authoritative world evolution.

### Semantic Domain

A **semantic domain** answers:

- what region/world is legal to query
- what detail bands exist
- what semantic families are enabled
- what correctness obligations or guarantees are required

It does not answer:

- how many steps to march
- which backend to prefer
- whether dynamic resolution is enabled

### Execution Policy

An **execution policy** answers:

- what cost budget is allowed
- what guarantee class is required
- what method classes are permitted
- which backend is preferred
- what quality state is active
- what artifact reuse aggressiveness is acceptable

**Code sketch**

```rust
pub enum RequiredGuaranteeClass {
    Exact,
    ConservativeNoFalseMiss,
    IntervalBounded,
    BestEffort,
}

pub enum SelectedMethodClass {
    ExactOracle,
    ConservativeSolver,
    IntervalSolver,
    HeuristicSolver,
}

pub struct QueryExecutionPolicy {
    pub backend_preference: DispatchBackend,
    pub required_guarantee: RequiredGuaranteeClass,
    pub ray_budget: Option<RayBudgetPolicy>,
}
```

`SelectedMethodClass` belongs on planner or solver outputs, not on the input policy surface.

### Evidence

**Evidence** is the compositional certificate object that planners and solvers reason from.

It is not:

- semantic policy
- backend preference
- runtime scheduling state
- execution counters or telemetry

It should subsume:

- support facts
- distance semantics
- derivative availability
- Lipschitz knowledge
- interval availability
- analytic intersection legality
- identity/provenance guarantees
- temporal stability and change class

**Code sketch**

```rust
pub struct SemanticEvidence {
    pub distance: DistanceEvidence,
    pub support: SupportEvidence,
    pub differential: DifferentialEvidence,
    pub identity: IdentityEvidence,
    pub temporal: TemporalEvidence,
    pub origin: EvidenceOrigin,
    pub scope: EvidenceScope,
}
```

Evidence origin matters.

Static compile-time evidence, runtime-refined evidence, and artifact-derived evidence are not identical in planner trust or reuse semantics.
`SemanticEvidence` should stay a certificate object, not a god-struct for every planner concern in the engine.

### Evidence Origin

Evidence should carry where it came from.

Recommended initial sources:

- `StaticCompiled`
- `RuntimeObserved`
- `ArtifactDerived`
- `ImportedCompatibility`

Recommended initial trust rules:

- `StaticCompiled` evidence may be treated as invariant until authored content or lineage identity says otherwise.
- `RuntimeObserved` evidence is valid only for the originating snapshot unless an explicit transition-validity rule extends it.
- `ArtifactDerived` evidence is valid only while the source artifact remains valid.
- `ImportedCompatibility` evidence must never strengthen correctness claims beyond the declared imported contract.

### Evidence Scope

Evidence also needs an explicit validity horizon or scope.

Recommended initial scopes:

- `CompileInvariant`
- `SnapshotLocal`
- `TransitionCompatible`
- `ArtifactBound`

Origin answers where the evidence came from.
Scope answers how long and under what conditions it may still be treated as valid.

### Artifact

An **artifact** is a materialized semantic view over a snapshot or transition.

Examples:

- support summary
- culling table
- primary-hit witness buffer
- color history
- continuation map
- future navigation broadphase

Artifacts are not "temporary implementation clutter."
They are first-class reusable products with compatibility and validity rules.

Artifact keys should exist.
They should not be mistaken for the full semantic truth of validity.

The artifact system should be designed around:

- fast lookup keys
- explicit compatibility relations
- explicit validity predicates
- invalidation rules over snapshots, transitions, evidence, and policy

### Artifact Compatibility Relation

An **artifact compatibility relation** answers whether a candidate artifact is eligible for consideration under a new query or observer context.

An **artifact validity rule** answers whether reuse is actually legal.

These are separate on purpose.
In the early architecture they should both remain typed declarative data, not opaque code.

### Concrete Observer Plan

A **concrete observer plan** is a domain-specific compiled plan that consumes query primitives, policies, evidence, and artifacts to answer a structured family of questions.

Examples:

- `PresentationPlan`
- future `CollisionPlan`
- future `EditorInspectionPlan`

### Query-Program Spine

A **query-program spine** is the narrow shared, non-executing normalized layer extracted from multiple concrete observer plans.

It should carry only the machinery that is truly shared:

- input bindings
- query primitive invocations
- artifact loads and stores
- dependency edges
- output bindings
- policy requirements
- observability summaries

It should not immediately absorb:

- presentation-specific shading rules
- collision-specific contact and sweep semantics
- backend-specific execution kernels

This shared layer arrives late in the roadmap on purpose.

## End State Of This Roadmap

The end state of this roadmap is:

1. snapshots and transitions are real typed architectural objects
2. domains and execution policies are distinct everywhere
3. evidence is unified, compositional, and monotone
4. temporal reuse and invalidation are engine-level semantics, not only presentation hacks
5. artifacts are one runtime architecture with explicit logical and physical layers
6. presentation and collision exist as real concrete observers and lower into a shared non-executing spine
7. solver planning uses evidence per subtree, artifact, and policy
8. differential semantics are explicit enough to replace major finite-difference hot paths with certified alternatives where legal

## Phase Overview

This RFC defines ten phases after the current directional Phase 24 horizon from RFC 0005.

- **Phase 25:** Snapshot identity, epochs, and stable semantic IDs
- **Phase 26:** Semantic domain, execution policy, and legal approximation
- **Phase 27:** Unified semantic evidence and certificates
- **Phase 28:** Temporal semantics, transition model, and multi-clock discipline
- **Phase 29:** Artifact runtime, materialized semantic views, and physical layout planning
- **Phase 30:** Collision observer foundation and static query families
- **Phase 31:** Transition-aware collision observer, witnesses, and runtime integration
- **Phase 32:** Shared observer vocabulary and query-program spine
- **Phase 33:** Shared spine analyses, diagnostics, and cross-observer validation
- **Phase 34:** Differential semantics, mixed solver planning, and backend convergence

## Phase 25: Snapshot Identity, Epochs, And Stable Semantic IDs

### Goal

Make snapshot identity real and thread it through captures, queries, artifacts, and observer execution.

### Why this is first

Nothing else in this RFC is trustworthy until "what world state are we talking about?" has a real answer.

### Design Rule For This Phase

Queries target immutable snapshot handles.

### Decision Hooks For This Phase

- `Decisions Locked By This RFC -> Identity Is Layered`
- `Decisions Locked By This RFC -> Artifact Validity Is Predicate-Based, Not Key-Equality-Based`
- `Design Rules 1-3, 8-9`

### Parallelization Notes

- Workstream A owns identity types and ABI plumbing.
- Workstream B owns runtime snapshot validity and artifact/history keying.
- Workstream C owns tests and CLI/report coverage.
- Task 25A1 should land first.
- Once 25A1 lands, 25A2 and 25B1 can proceed in parallel.

### Workstream A: Identity Types And ABI

#### Task 25A1 — Add explicit snapshot and semantic-identity types

**Description**

Introduce explicit internal types for snapshot identity, semantic entity identity, and epoch/version identity.

**Files**

- `compiler/query_exec/ids.rs`
- `compiler/portable.rs`
- `compiler/lib.rs`
- optionally new `compiler/world_identity/mod.rs`

**Implementation notes**

Keep compatibility with current portable ABI where necessary, but stop letting raw `u32` values stand in for all identity concepts internally.

Recommended internal types:

- `AuthoredContentId`
- `EntityLineageId`
- `SnapshotEntityId`
- `WorldSnapshotId`
- `SnapshotEpoch`
- `ArtifactKeySeed`

The first landing can use `u64` internally even if some compatibility edges still project to `u32`.
However, merely widening the current hash width is not sufficient by itself.
The important cut is identity layering, not just integer size.

**Decision hooks**

- `Decision: Identity Is Layered` applies directly here. Do not let `AuthoredContentId`, `EntityLineageId`, `SnapshotEntityId`, `WorldSnapshotId`, or `SnapshotEpoch` collapse back into one "generic id" in helper code.
- `Design Rules 1-3` apply directly here. Typed identity is the deliverable, not merely larger integers.

**Code sketch**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AuthoredContentId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntityLineageId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SnapshotEntityId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WorldSnapshotId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SnapshotEpoch(pub u64);
```

**Acceptance criteria**

- Internal code no longer treats scene id, root feature id, lineage identity, and epoch as interchangeable raw integers.
- The identity model can distinguish persistent lineage from snapshot-local identity.
- New types have deterministic serialization and ordering.
- Compatibility projection to existing ABI values is explicit and documented.

#### Task 25A2 — Replace epoch placeholders and thread real epoch through capture values

**Description**

Remove current `epoch = 0` placeholder behavior from capture construction and interpreter helpers.

**Files**

- `compiler/query_exec/mir_scene_semantics.rs`
- `compiler/kernel/interp.rs`
- `compiler/query_exec/context.rs`
- `compiler/tests/query_exec.rs`
- `compiler/tests/codegen_v2.rs`

**Implementation notes**

Add snapshot-aware query execution context construction.
Capture values should be built from actual snapshot identity rather than from compile-time constants.

**Decision hooks**

- `Decision: Identity Is Layered` means epoch propagation must travel with a typed snapshot handle rather than being reconstructed from unrelated ids.
- `Design Rule 2` applies here. `SnapshotEpoch` is version identity, not presentation frame or simulation tick.

**Code sketch**

```rust
pub struct QueryExecSnapshotContext {
    pub snapshot: WorldSnapshotHandle,
    pub region_shapes: BTreeMap<SnapshotEntityId, Vec<SmolStr>>,
}
```

**Acceptance criteria**

- No core execution path hardcodes capture epoch to zero.
- Snapshot-aware tests can assert epoch propagation end to end.
- Mismatched epoch reuse can be detected at runtime.

### Workstream B: Snapshot Validity And Reuse Keys

#### Task 25B1 — Add snapshot-aware artifact and history keys

**Description**

Introduce a typed key used to reuse or invalidate artifacts and history based on snapshot compatibility.

**Files**

- `compiler/query_plan/mod.rs`
- `compiler/presentation_contract/mod.rs`
- `compiler/presentation_exec/resources.rs`
- optionally new `compiler/artifact_key/mod.rs`

**Implementation notes**

Do not key reuse only by attachment name or frame count.
Treat `policy_digest` as a coarse lookup filter only, never as the final semantic truth of policy compatibility.

Recommended key dimensions:

- snapshot lineage
- epoch
- contract id
- logical artifact schema
- compatibility/layout signature
- coarse policy compatibility digest where relevant

Do not require exact policy equality unless the artifact semantics actually need it.

**Decision hooks**

- `Decision: Artifact Validity Is Predicate-Based, Not Key-Equality-Based` applies directly here. `ArtifactKey` is a candidate lookup tool, not the final authority for legality.
- `Design Rules 8-9` apply directly here. If this task adds a key dimension, it must also preserve space for compatibility and validity predicates.

**Code sketch**

```rust
pub struct ArtifactKey {
    pub snapshot_id: WorldSnapshotId,
    pub epoch: SnapshotEpoch,
    pub contract_id: Option<QueryContractId>,
    pub compatibility_hash: u64,
    pub policy_digest: u64,
}
```

**Acceptance criteria**

- Artifacts and history slots have one explicit reuse key type.
- Presentation history compatibility is no longer frame-count-only in spirit.
- The key model can be reused later by non-presentation artifacts.
- The design explicitly permits compatible policy ranges rather than only exact policy equality.

#### Task 25B2 — Upgrade stable semantic ids to a long-lived internal scheme

**Description**

Move away from treating 32-bit FNV ids as the final semantic identity model.

**Files**

- `compiler/query_exec/ids.rs`
- `compiler/query_exec/context.rs`
- `compiler/tests/query_exec.rs`

**Implementation notes**

The first cut does not need to change every portable field immediately.
It does need to introduce a long-lived internal identity model so later phases do not keep building on `u32` hash assumptions.

**Decision hooks**

- `Decision: Identity Is Layered` means this task is complete only when implementors can tell which layer of identity each callsite is using.
- Merely widening FNV or swapping one hash type for another is not sufficient unless lineage identity and snapshot-local identity become explicit.

**Acceptance criteria**

- Internal identity generation is no longer limited to 32-bit space.
- Internal identity layering is explicit rather than implied by naming convention.
- Compatibility projection is explicit.
- Tests cover deterministic identity generation.

### Workstream C: Tooling And Tests

#### Task 25C1 — Add snapshot identity reports and validation tests

**Description**

Add reporting and tests that make snapshot identity visible and validate reuse boundaries.

**Files**

- `compiler/bin/wrela/commands/shared.rs`
- new `compiler/tests/snapshot_identity.rs`
- `compiler/tests/presentation_exec.rs`

**Implementation notes**

Expose snapshot ids and epochs in debug reports where they matter:

- presentation debug
- query execution diagnostics
- future artifact store diagnostics

**Decision hooks**

- `Decision: Identity Is Layered` and `Design Rule 2` apply to diagnostics too. Reports should show snapshot id, epoch, and any frame or tick data separately rather than folding them into one label.
- `Decision: Artifact Validity Is Predicate-Based, Not Key-Equality-Based` means tests should prove reuse rejection via declared validity logic, not just missing-key behavior.

**Tests**

- capture values carry real epoch ids
- artifacts keyed to one epoch are rejected or invalidated on mismatch
- snapshot ids are stable and deterministic

### Phase 25 Exit Criteria

- Snapshot identity is real and typed.
- Epoch placeholders are gone from core execution.
- Artifacts and history can key reuse off snapshot identity.
- Internal identity is no longer architecturally capped at `u32` hashes.

## Phase 26: Semantic Domain, Execution Policy, And Legal Approximation

### Goal

Separate semantic domain from execution policy everywhere and replace vague guarantee placeholders with a typed legality model.

### Why this is second

Evidence, artifacts, and performance planning all become cleaner once semantic meaning stops carrying cost knobs around with it.

### Design Rule For This Phase

Domains answer "what is legal to ask about."
Policies answer "how hard may we work, what guarantee is required, and what method classes are permitted."

### Decision Hooks For This Phase

- `Decisions Locked By This RFC -> Required Guarantee And Selected Method Class Are Separate`
- `Design Rules 4, 7, 12`

### Parallelization Notes

- Workstream A owns contract records and portable ABI changes.
- Workstream B owns HIR/MIR lowering and compatibility migration.
- Workstream C owns execution-path integration and diagnostics.
- Task 26A1 should land before 26B1 and 26C1.

### Workstream A: Contract Records

#### Task 26A1 — Add execution-policy records separate from `SceneDomain`

**Description**

Create typed execution-policy records for ray budgets, required guarantee class, backend preference, and quality state.

**Files**

- `compiler/portable.rs`
- `compiler/query_contract/mod.rs`
- `compiler/presentation_contract/mod.rs`
- optionally new `compiler/execution_policy/mod.rs`

**Implementation notes**

Recommended types:

- `RayBudgetPolicy`
- `QueryExecutionPolicy`
- `PresentationExecutionPolicy`
- `RequiredGuaranteeClass`
- `SelectedMethodClass`

**Decision hooks**

- `Design Rule 4` applies directly here. Semantic domain and execution policy must remain separate even if temporary compatibility structs reference both.
- `Decision: Required Guarantee And Selected Method Class Are Separate` means policy records should express what is allowed, not what the planner eventually picked.

**Code sketch**

```rust
pub struct RayBudgetPolicy {
    pub max_distance: f32,
    pub min_step: f32,
    pub hit_epsilon: f32,
    pub max_steps: i32,
}

pub struct QueryExecutionPolicy {
    pub required_guarantee: RequiredGuaranteeClass,
    pub backend_preference: DispatchBackend,
    pub ray_budget: Option<RayBudgetPolicy>,
}
```

**Acceptance criteria**

- A typed execution-policy layer exists.
- `SceneDomain` no longer needs to be the place where budgets live conceptually.
- Policies can be hashed and compared independently of semantic domain identity.

#### Task 26A2 — Replace `guarantee: u32` with typed guarantee and method-class concepts

**Description**

Retire the current vague integer placeholder and replace it with typed guarantee and method-class concepts.

**Files**

- `compiler/portable.rs`
- `compiler/presentation_exec/mod.rs`
- `compiler/query_exec/native_bridge.rs`
- `compiler/tests/portable_abi.rs`

**Implementation notes**

Do not let the new type become a dumping ground for unrelated knobs.
The planner-facing selected method class should be modeled separately from the required guarantee class from the start.

**Decision hooks**

- `Decision: Required Guarantee And Selected Method Class Are Separate` is the primary design constraint for this task. `BestEffort` or heuristic legality belongs on the required-guarantee side only if the caller truly allows it.
- Avoid reintroducing ambiguity by naming or storage. If one enum could be read as both "caller allows this" and "planner picked this," the task is not done.

**Code sketch**

```rust
pub enum RequiredGuaranteeClass {
    Exact,
    ConservativeNoFalseMiss,
    IntervalBounded,
    BestEffort,
}

pub enum SelectedMethodClass {
    ExactOracle,
    ConservativeSolver,
    IntervalSolver,
    HeuristicSolver,
}
```

**Acceptance criteria**

- No new code writes `guarantee = 0` as a semantic placeholder.
- Diagnostics can explain illegal contract/policy pairings in typed terms.
- Required guarantee class and selected method class are not modeled as the same enum.
- ABI snapshots are updated accordingly.

### Workstream B: Lowering And Compatibility

#### Task 26B1 — Preserve current authored `domain` syntax while lowering budgets into policies

**Description**

Keep the current surface stable for now, but lower budget-like authored fields into execution-policy values rather than semantic domain values.

**Files**

- `compiler/hir/def.rs`
- `compiler/hir/lower.rs`
- `compiler/mir/lower.rs`
- `compiler/tests/codegen_v2.rs`

**Implementation notes**

The authored surface may stay compatibility-shaped temporarily.
The internal representation must not.

**Decision hooks**

- `Design Rule 4` applies here. Preserve authored syntax if helpful, but lower into separate semantic-domain and execution-policy records immediately.
- `Decision: Required Guarantee And Selected Method Class Are Separate` means lowering should produce policy inputs, not backend-chosen execution outcomes.

**Authored example**

```wr
domain main_domain {
    geometry_detail = fine
    material = true
    radiance = true
    media = true
    max_steps = 96
}
```

Internal lowering target:

```rust
SemanticDomain { ... }
QueryExecutionPolicy {
    required_guarantee: RequiredGuaranteeClass::ConservativeNoFalseMiss,
    ray_budget: Some(RayBudgetPolicy { max_steps: 96, .. }),
    ..
}
```

**Acceptance criteria**

- Current projects still parse and lower.
- Budget fields are no longer stored in the semantic-domain struct.
- Lowering helpers are shared rather than inlined in multiple places.

#### Task 26B2 — Remove literal-only domain budget scraping from presentation execution

**Description**

Replace the current literal-scraping path with evaluated execution-policy inputs.

**Files**

- `compiler/bin/wrela/commands/shared.rs`
- `compiler/presentation_exec/mod.rs`
- `compiler/tests/presentation_exec.rs`

**Implementation notes**

This task should remove the idea that presentation execution must peel raw numeric march settings out of authored domain metadata.

**Decision hooks**

- `Design Rule 4` means no new code path may infer execution budgets by scraping semantic-domain metadata after lowering.
- If a value still needs to reach presentation execution, it should do so through a typed policy object with explicit legality meaning.

**Acceptance criteria**

- `domain_execution_inputs` no longer extracts literal budgets from semantic domain metadata.
- Presentation can consume typed policy values.
- Non-literal policy construction can be supported cleanly.

### Workstream C: Execution And Diagnostics

#### Task 26C1 — Thread semantic domain and execution policy separately through CPU, vGPU, and WGSL query execution

**Description**

Update query execution interfaces to receive and validate both semantic domain and execution policy.

**Files**

- `compiler/query_exec/cpu.rs`
- `compiler/query_exec/vgpu.rs`
- `compiler/query_exec/wgsl.rs`
- `compiler/query_exec/world.rs`

**Implementation notes**

A backend should be able to reject a policy or required guarantee class explicitly without implying that the semantic question itself is unsupported.

**Decision hooks**

- `Decision: Required Guarantee And Selected Method Class Are Separate` means backend code should validate required guarantees independently from the planner's chosen method class.
- `Design Rule 12` applies here. CPU remains the first legality oracle, and other backends should fail explicitly instead of silently weakening semantics.

**Acceptance criteria**

- Execution signatures distinguish domain from policy.
- Illegal guarantee or policy/backend combinations produce explicit diagnostics.
- CPU remains the legality oracle for the first cut.

#### Task 26C2 — Add observability for legal degradations and active policy state

**Description**

Expose which degradations are active and why they remain legal.

**Decision hooks**

- Diagnostics should name the required guarantee, the selected method class, and the specific legal degradation path separately.
- `Design Rule 7` applies here. Approximation legality must be typed and explainable, not hidden in frame-time heuristics.

**Files**

- `compiler/query_exec/cost.rs`
- `compiler/presentation_exec/cost.rs`
- `compiler/bin/wrela/commands/shared.rs`

**Tests**

- legal degradation steps are reported
- unsupported degradations are rejected
- cost reports can distinguish semantic domain from execution policy

### Phase 26 Exit Criteria

- Semantic domain and execution policy are distinct internal layers.
- The vague guarantee placeholder is gone or fully typed.
- Presentation and query execution no longer scrape budgets out of semantic domain metadata.
- Diagnostics and observability can explain legal guarantee and policy state.

## Phase 27: Unified Semantic Evidence And Certificates

### Goal

Replace scattered fact bags and partial certificates with one compositional evidence model.

### Why this is third

This is where semantic clarity, solver legality, and performance planning meet.

### Design Rule For This Phase

Evidence must refine monotonically.

### Decision Hooks For This Phase

- `Decisions Locked By This RFC -> Evidence Provenance Is First-Class`
- `Key Architectural Definitions -> Evidence Scope`
- `Design Rules 5-6`

### Parallelization Notes

- Workstream A owns evidence data model and merge/refinement APIs.
- Workstream B owns scene-derived evidence propagation and planner adoption.
- Workstream C owns tooling and regression coverage.
- Task 27A1 should land first.

### Workstream A: Evidence Model

#### Task 27A1 — Add `SemanticEvidence` and nested evidence records

**Description**

Create the unified evidence object that subsumes the current fact families.

**Files**

- `compiler/query_solver/mod.rs`
- optionally new `compiler/semantic_evidence/mod.rs`
- `compiler/lib.rs`

**Implementation notes**

The first cut should subsume at least:

- distance semantics
- support facts
- differential availability
- identity/provenance guarantees
- temporal stability

Keep the evidence object narrowly about certificates, trust, and validity horizon.
Do not use it as a catch-all container for policy, backend preferences, runtime counters, or scheduling state.

**Decision hooks**

- `Design Rules 5-6` apply directly here. The first shape of `SemanticEvidence` must leave room for monotone refinement, provenance, and scope instead of forcing those in later as bolt-ons.
- Treat "unknown," "conservative," and "not applicable" as meaningfully different states.
- Even if Task 27A3 lands later in the phase, do not choose a container shape here that makes `origin` and `scope` awkward to add without churn.
- If a concern does not behave like evidence or a certificate, do not put it in `SemanticEvidence` just because planners happen to read it.

**Code sketch**

```rust
pub struct SemanticEvidence {
    pub distance: DistanceEvidence,
    pub support: SupportEvidence,
    pub differential: DifferentialEvidence,
    pub identity: IdentityEvidence,
    pub temporal: TemporalEvidence,
}

pub struct DistanceEvidence {
    pub semantics: DistanceSemantics,
    pub lipschitz: LipschitzStatus,
    pub interval_bounds: FactAvailability,
    pub analytic_intersection: AnalyticIntersectionStatus,
}
```

**Acceptance criteria**

- A single top-level evidence object exists.
- Existing fact concepts can be represented without loss.
- The new model is explicit about unknown, unavailable, and conservative states.

#### Task 27A2 — Define evidence refinement and weakening rules

**Description**

Make evidence composition and refinement explicit rather than emergent from ad hoc booleans.

**Files**

- `compiler/semantic_evidence/mod.rs` or `compiler/query_solver/mod.rs`
- new `compiler/tests/semantic_evidence.rs`

**Implementation notes**

Every field or shape operator should either:

- preserve evidence
- weaken evidence
- refine evidence
- or explicitly destroy some evidence class

**Decision hooks**

- `Design Rule 5` is the core constraint here. Runtime refinement may add information; weakening may remove strength; neither may silently strengthen unsupported claims.
- If an operator breaks a guarantee, make the weakening rule explicit in one place rather than re-encoding it in each planner.

**Code sketch**

```rust
impl SemanticEvidence {
    pub fn weaken_for_warp(&self) -> Self { ... }
    pub fn refine_with_runtime_bounds(&self, bounds: RuntimeBoundsEvidence) -> Self { ... }
}
```

**Acceptance criteria**

- Evidence refinement rules are centralized.
- Tests cover monotone refinement and expected weakening.
- Operators no longer silently invent stronger guarantees.

#### Task 27A3 — Add explicit evidence provenance and scope tracking

**Description**

Track where evidence came from and how long it remains valid so planners do not accidentally treat runtime observations like compile-time invariants.

**Files**

- `compiler/semantic_evidence/mod.rs` or `compiler/query_solver/mod.rs`
- new `compiler/tests/semantic_evidence.rs`
- `compiler/bin/wrela/commands/shared.rs`

**Implementation notes**

The planner should be able to distinguish:

- statically proven evidence
- runtime-refined evidence
- artifact-derived evidence

This should affect:

- reuse legality
- solver trust
- observability reports
- evidence scope and validity horizon

**Decision hooks**

- `Decision: Evidence Provenance Is First-Class` and `Key Architectural Definitions -> Evidence Scope` both apply directly here.
- Origin answers where the evidence came from. Scope answers how long it can be trusted. Do not collapse those into one enum or one boolean.

**Code sketch**

```rust
pub enum EvidenceOrigin {
    StaticCompiled,
    RuntimeObserved,
    ArtifactDerived,
    ImportedCompatibility,
}
```

```rust
pub enum EvidenceScope {
    CompileInvariant,
    SnapshotLocal,
    TransitionCompatible,
    ArtifactBound,
}
```

**Acceptance criteria**

- Evidence provenance is explicit.
- Reports can show origin and refinement path.
- Planner code can avoid treating weaker-origin evidence as if it were compile-time truth.
- Default trust and validity rules exist for each evidence origin.
- Evidence scope or validity horizon is explicit alongside origin.

### Workstream B: Evidence Propagation

#### Task 27B1 — Derive semantic evidence from field and shape scenes

**Description**

Replace direct planner dependence on `FieldFacts` with derived `SemanticEvidence`.

**Files**

- `compiler/query_solver/mod.rs`
- `compiler/scene_ir/mod.rs`
- `compiler/tests/support_family.rs`

**Implementation notes**

`FieldFacts` can remain temporarily as a compatibility wrapper if that eases migration, but the new evidence type must become the authoritative planning input.

**Decision hooks**

- `Decision: Evidence Provenance Is First-Class` means derived scene evidence must preserve where stronger claims came from rather than flattening them into anonymous booleans.
- Planner code should consume `SemanticEvidence` as the source of truth instead of rebuilding bespoke fact bags from scene nodes.

**Acceptance criteria**

- Scene analysis produces `SemanticEvidence`.
- Planner logic can consume evidence without rebuilding the same facts.
- Support-family behavior remains preserved.

#### Task 27B2 — Thread evidence through query plans, solver plans, and artifact declarations

**Description**

Ensure planners, solver portfolios, and artifacts can all cite the same evidence object or summary.

**Files**

- `compiler/query_plan/mod.rs`
- `compiler/kernel/ir.rs`
- `compiler/presentation_plan/mod.rs`
- `compiler/query_solver/mod.rs`

**Implementation notes**

Do not dump the entire evidence object indiscriminately into every public record.
Use summaries where full detail is unnecessary.

**Decision hooks**

- Evidence summaries must preserve every distinction that affects legality: guarantee-relevant content, origin, and scope.
- If a summary drops something a planner or artifact validity rule needs, keep the full evidence attached or add a richer summary type.

**Acceptance criteria**

- Query plans can reference evidence or evidence summaries.
- Solver plans no longer need bespoke fact bags separate from the main evidence model.
- Artifact declarations can say what evidence they depend on or preserve.

### Workstream C: Tooling, Diagnostics, And Tests

#### Task 27C1 — Add evidence reports and regression tests

**Description**

Make evidence visible to implementors and test it explicitly.

**Decision hooks**

- Reports should expose both evidence content and why it is trusted, including origin and scope where relevant.
- Regression tests should cover weakening, refinement, and "do not over-trust runtime/artifact evidence" behavior, not just pretty-print output.

**Files**

- `compiler/bin/wrela/commands/shared.rs`
- new `compiler/tests/semantic_evidence.rs`
- new `compiler/tests/ray_solver_evidence.rs`

**Tests**

- exact primitives produce stronger evidence than warped or opaque leaves
- support evidence weakens through opaque boundaries
- provenance guarantees match shape composition rules
- evidence reports are deterministic

### Phase 27 Exit Criteria

- One unified evidence model exists.
- Evidence refinement is explicit and monotone.
- Planners, solvers, and artifacts can consume the same evidence structure.
- Regression tests prove preservation and weakening behavior.

### Phase 27 Re-evaluation Before Phase 28

Phase 27 should trigger a deliberate re-read of the architecture before temporal work expands.

The main lessons to preserve are:

- the repo wants one rich internal evidence object plus narrower summary forms at observer, artifact, ABI, and report boundaries
- evidence scope and validity horizon are proving useful and should remain distinct from policy and scheduling concerns
- presentation history and query artifacts are already converging on one compatibility-and-validity architecture, but they are not yet the full artifact runtime
- temporal reuse logic is still presentation-shaped in several places, which is precisely why Phase 28 should land before artifact-runtime generalization

This re-evaluation should not reorder the roadmap to pull artifact runtime or shared spine work forward.
It should tighten the next cut.

Specifically, Phase 28 should treat the following as separate concerns:

- typed clocks and authoritative transition records
- evidence scope and validity horizon
- semantic change classes and transition compatibility

Do not let "temporal evidence" become a grab-bag that conflates all three.
If a fact answers how long a claim can be trusted, it belongs with evidence scope and validity horizon.
If it answers what changed between snapshots and what reuse thresholds are legal, it belongs with transition and change semantics.

Phase 28 should also make explicit that `SemanticEvidenceSummary`-style summary records are the intended wire shape across plans, ABI surfaces, and diagnostics whenever the full internal evidence object is not required.
That boundary is already architecturally useful and should be preserved deliberately rather than treated as temporary convenience.

## Phase 28: Temporal Semantics, Transition Model, And Multi-Clock Discipline

### Goal

Make time, change, and authoritative state advance first-class engine concepts rather than presentation-local conveniences.

### Why this is fourth

Snapshots alone are not enough for a game engine.
The engine must also understand transition, identity persistence, and reuse validity over change.

### Design Rule For This Phase

Epoch is version identity.
Time is typed.
Transition is first-class.

### Decision Hooks For This Phase

- `Decisions Locked By This RFC -> Authoritative State Advances By Deterministic Fixed-Tick Transition`
- `Key Architectural Definitions -> Change Compatibility Lattice`
- `Key Architectural Definitions -> Clock Family`
- `Design Rules 2, 8-9`

### Parallelization Notes

- Workstream A owns clock and transition contracts.
- Workstream B owns temporal evidence and reuse legality.
- Workstream C owns frame-input and observer integration.
- Task 28A1 should land before 28B1 and 28C1.

### Workstream A: Clock And Transition Contracts

#### Task 28A1 — Add typed clock records and transition records

**Description**

Introduce explicit types for simulation tick, presentation frame, wall-clock stamp, and transition summaries.

**Files**

- optionally new `compiler/time_semantics/mod.rs`
- `compiler/presentation_contract/mod.rs`
- `compiler/portable.rs`

**Decision hooks**

- `Key Architectural Definitions -> Clock Family` applies directly here. `SimulationTick`, `PresentationFrame`, `WallClockStamp`, and `SnapshotEpoch` must not alias each other in convenience code.
- `Key Architectural Definitions -> Change Compatibility Lattice` means change classifications should be usable in ordered compatibility checks, not only equality matches.
- Give the change-compatibility lattice a central typed home in this phase so later consumers reuse one model instead of growing local enum families.
- Keep observer-local frame/view state distinct from engine temporal state. If a record is carrying authoritative snapshot-transition context, do not hide it inside view-only naming or frame-only compatibility shims.

**Code sketch**

```rust
pub struct SimulationTick(pub u64);
pub struct PresentationFrame(pub u64);
pub struct WallClockStamp(pub i128);

pub struct ChangeSummary {
    pub geometry: GeometryChangeClass,
    pub materials: MaterialChangeClass,
    pub participants: ParticipantChangeClass,
    pub identity_preserved: bool,
}
```

**Acceptance criteria**

- The codebase no longer treats frame index, epoch, and tick as if they were the same concept.
- Transition summaries have a real typed home.
- Change compatibility is modeled as an ordering relation or lattice, not only as flat enum labels.
- The change-compatibility lattice has one obvious module or type family that later phases can import.
- Portable and presentation contracts can carry typed time/transition state where needed.

#### Task 28A2 — Add internal transition-aware query and artifact terminology

**Description**

Prepare the architecture for queries and artifacts that depend on two snapshots, not one.

**Files**

- `compiler/query_contract/mod.rs`
- `compiler/query_plan/mod.rs`
- `compiler/presentation_contract/mod.rs`

**Implementation notes**

This phase does not need a public transition-query family yet.
It does need the internal architecture to stop assuming that every meaningful question targets exactly one snapshot.

**Decision hooks**

- `Decision: Authoritative State Advances By Deterministic Fixed-Tick Transition` means two-snapshot and transition-aware dependencies are first-class internal concepts, not presentation hacks.
- Artifact and planner logic should consult change compatibility relations rather than inventing one-off "previous frame is good enough" rules.

**Acceptance criteria**

- Internal contracts can express "current snapshot plus previous snapshot" style dependencies.
- Artifact compatibility rules can cite transitions or change summaries.
- No public language surface change is required yet.

#### Task 28A3 — Add an internal authoritative state-advance contract and full transition record

**Description**

Define the internal contract for deterministic fixed-tick state advance and the authoritative transition record it produces.

**Files**

- optionally new `compiler/state_advance/mod.rs`
- optionally new `runtime/src/state_advance.rs`
- `compiler/portable.rs`
- `compiler/tests/temporal_semantics.rs`

**Implementation notes**

This task is internal architecture work.
It does not require the final public authored gameplay DSL in the same phase.

The important distinction is:

- `WorldTransitionRecord` is the authoritative output of state advance
- `ChangeSummary` is the planner-facing derived summary
- the runtime owns execution of authoritative advancement
- the compiler owns the types, legality model, and lowering into that runtime contract

Be explicit in code ownership:

- do not let compiler modules quietly accumulate scheduler or simulation-loop ownership
- do not let runtime modules quietly invent transition semantics outside compiler-owned types and legality rules

**Decision hooks**

- `Decision: Authoritative State Advances By Deterministic Fixed-Tick Transition` is the primary constraint for this task.
- The runtime/compiler ownership split is part of the architecture, not an implementation convenience. If a helper muddies that boundary, refactor it.

**Code sketch**

```rust
pub struct WorldTransitionRecord {
    pub from: WorldSnapshotHandle,
    pub to: WorldSnapshotHandle,
    pub tick: SimulationTick,
    pub inputs: TickInputBatch,
    pub identity_events: Vec<IdentityTransitionEvent>,
}
```

**Acceptance criteria**

- Internal architecture has a real state-advance contract.
- Transition summaries are explicitly derived from a fuller transition record.
- Observer systems remain downstream consumers rather than authoritative state mutators.

### Workstream B: Temporal Evidence And Validity

#### Task 28B1 — Add temporal evidence and change-class reasoning

**Description**

Extend the architecture so it can describe both temporal validity horizon and semantic change, without collapsing them into one notion.

**Files**

- `compiler/semantic_evidence/mod.rs` or `compiler/query_solver/mod.rs`
- `compiler/presentation_contract/mod.rs`
- new `compiler/tests/temporal_semantics.rs`

**Implementation notes**

Recommended first change classes:

- `None`
- `RigidTransformOnly`
- `MaterialOnly`
- `ParticipantOnly`
- `GeometryTopologyChange`
- `IdentityBreakingReplacement`

These classes should support a compatibility ordering so planner and artifact logic can ask whether one class is acceptable wherever another class is allowed.

The important architectural split for this task is:

- evidence scope / validity horizon answers how long a claim may be trusted
- change classes / transition compatibility answer what changed and what reuse rules may legally survive that change

The implementation may keep these concepts near each other.
It must not let one quietly stand in for the other.

**Decision hooks**

- `Key Architectural Definitions -> Change Compatibility Lattice` applies directly here. Change reasoning should centralize the partial order instead of repeating ad hoc severity logic in each consumer.
- Temporal validity-horizon facts should compose with evidence scope: snapshot-local claims must not silently survive across incompatible transitions.
- If current temporal evidence fields are only mirroring evidence scope categories, treat that as scaffolding to refine rather than as the final semantic model for change.
- If a consumer needs a threshold or acceptance rule, it should reference the central change-compatibility types from Task 28A1 rather than inventing a local ordering.

**Code sketch**

```rust
pub struct TemporalEvidence {
    pub stationary: FactAvailability,
    pub rigid_over_interval: FactAvailability,
    pub topology_stable: FactAvailability,
    pub bounded_velocity: FactAvailability,
}
```

**Acceptance criteria**

- The architecture can express temporal validity horizon separately from transition/change compatibility.
- Evidence can express temporal stability.
- Change classes can drive reuse legality.
- Tests validate invalidation on topology-breaking changes.

#### Task 28B2 — Tie temporal history reuse to transition validity rather than only frame continuity

**Description**

Upgrade temporal reuse rules so they depend on snapshot compatibility and change class.

**Files**

- `compiler/presentation_contract/mod.rs`
- `compiler/presentation_exec/resources.rs`
- `compiler/presentation_exec/wgsl.rs`
- `compiler/tests/presentation_plan.rs`

**Implementation notes**

History reuse may still use the current attachment model.
What changes is the reason it is valid.

**Decision hooks**

- `Decision: Artifact Validity Is Predicate-Based, Not Key-Equality-Based` applies directly here. Reuse should be legal because the transition-validity rule says so, not because two frame counters happen to line up.
- Use change compatibility thresholds and snapshot lineage checks explicitly rather than hand-coded "same previous frame" assumptions.
- Existing frame-continuity and camera-cut heuristics may remain as compatibility scaffolding during migration, but the destination of this task is transition-aware legality, not better heuristics with the same hidden semantics.

**Acceptance criteria**

- History validity can reference snapshot and transition compatibility.
- Presentation temporal policies can explicitly require or reject certain change classes.
- Tests cover invalidation on camera cuts and on incompatible world changes.

### Workstream C: Observer Integration

#### Task 28C1 — Extend frame state to carry snapshot and transition context

**Description**

Update frame input/state so observer execution can reason about both current and previous snapshot context.

**Files**

- `compiler/presentation_exec/mod.rs`
- `compiler/portable.rs`
- `compiler/tests/portable_abi.rs`

**Implementation notes**

`FrameState` and any adjacent transition-context record should be able to talk about:

- current snapshot epoch
- previous snapshot epoch
- simulation tick
- presentation frame
- optional change summary

If this phase discovers that observer-facing view/frame data and engine temporal state want different records, split them cleanly rather than forcing one overloaded struct to carry both forever.

**Decision hooks**

- `Key Architectural Definitions -> Clock Family` means `FrameState` should carry typed context rather than overloading one numeric slot with multiple meanings.
- Observer-facing frame state remains downstream context; it must not become an authoritative state-mutation channel.

**Acceptance criteria**

- `FrameState` can carry both observer-time and snapshot-transition context.
- Current preview behavior remains representable.
- ABI tests are updated.

#### Task 28C2 — Add transition-aware motion and continuation diagnostics

**Description**

Make continuation and motion semantics visible in reports so engineers can debug temporal validity rather than guessing.

**Decision hooks**

- Diagnostics should cite the transition/change basis for continuation decisions, not only "history hit" or "history miss."
- If temporal reuse was rejected because of change-lattice incompatibility or evidence-scope expiry, say that directly.

**Files**

- `compiler/presentation_exec/cost.rs`
- `compiler/bin/wrela/commands/shared.rs`

**Tests**

- motion/continuation diagnostics mention transition compatibility
- invalid temporal reuse is visible in debug output

### Phase 28 Exit Criteria

- Typed clock concepts exist.
- Internal authoritative state-advance contract exists.
- Transition and change summaries are real architectural objects.
- Temporal validity horizon and change compatibility are both explicit and distinct.
- Temporal evidence can drive reuse validity.
- Frame and observer state can carry snapshot-transition context.

## Phase 29: Artifact Runtime, Materialized Semantic Views, And Physical Layout Planning

### Goal

Turn artifacts into one first-class runtime architecture and split logical meaning from physical layout.

### Why this is fifth

By this point the engine will know what snapshot it is in, what policy it is using, what evidence it has, and what changed.
That is the minimum context needed for principled artifact reuse.

### Design Rule For This Phase

Logical contracts describe meaning.
Layout plans describe storage.

### Decision Hooks For This Phase

- `Decisions Locked By This RFC -> Artifact Validity Is Predicate-Based, Not Key-Equality-Based`
- `Key Architectural Definitions -> Artifact Compatibility Relation`
- `Key Architectural Definitions -> Evidence Scope`
- `Design Rules 8-9`

### Parallelization Notes

- Workstream A owns the unified logical artifact model.
- Workstream B owns runtime store and reuse/invalidation behavior.
- Workstream C owns physical layout planning and backend adapters.
- Task 29A1 should land before 29B1 and 29C1.

### Workstream A: Unified Logical Artifact Model

#### Task 29A1 — Unify query artifacts, frame artifacts, and history slots under one logical artifact model

**Description**

Create one logical artifact model that can describe both query-derived and observer-derived materializations.

**Files**

- `compiler/query_plan/mod.rs`
- `compiler/presentation_plan/mod.rs`
- `compiler/presentation_contract/mod.rs`
- optionally new `compiler/artifact_contract/mod.rs`

**Implementation notes**

Examples that should map into one system:

- support summary
- culling table
- primary-hit attachment
- color history
- continuation primary-hit history

**Code sketch**

```rust
pub struct SemanticArtifactContract {
    pub id: SmolStr,
    pub kind: SemanticArtifactKind,
    pub logical_schema: ArtifactLogicalSchema,
    pub compatibility: ArtifactCompatibilityRelation,
    pub validity: ArtifactValidityRule,
}
```

Recommended first compatibility dimensions:

- snapshot lineage relation
- transition or change-class relation
- policy compatibility relation
- evidence compatibility relation

**Decision hooks**

- `Decision: Artifact Validity Is Predicate-Based, Not Key-Equality-Based` applies directly here. Compatibility and validity belong in the contract, not only in store lookups.
- Evidence origin and scope may affect artifact eligibility. Do not assume all evidence with the same shape is reusable the same way.
- `ArtifactValidityRule` should start as a small typed declarative algebra that reports what it checked. Do not hide first-cut validity semantics behind arbitrary closures or adapter-specific code.

**Acceptance criteria**

- One logical artifact model can describe both query and presentation materializations.
- Existing plans can be migrated without loss of meaning.
- Validity rules are part of the contract.
- Compatibility relations are explicit rather than implied only by key equality.

#### Task 29A2 — Let plans declare artifact producers, consumers, and validity requirements explicitly

**Description**

Make artifact dependencies explicit in plans rather than implicit in pass wiring or helper behavior.

**Decision hooks**

- Plans should declare artifact producers, consumers, and preserved validity assumptions explicitly so later shared analyses do not have to rediscover them.
- If a plan depends on a compatible policy range or change threshold, encode that dependency instead of relying on comments or pass order.

**Files**

- `compiler/query_plan/mod.rs`
- `compiler/presentation_plan/mod.rs`
- `compiler/kernel/ir.rs`

**Acceptance criteria**

- Plans can say which artifacts they load, produce, and preserve.
- Artifact dependencies are inspectable in reports.
- Validation catches missing artifact producers or illegal reuse assumptions.

### Workstream B: Artifact Store Runtime

#### Task 29B1 — Add a runtime artifact store keyed by snapshot, policy, and compatibility

**Description**

Implement a runtime artifact store that can materialize, reuse, and invalidate artifacts under declared rules.

**Files**

- new `compiler/artifact_store/mod.rs`
- `compiler/presentation_exec/resources.rs`
- `compiler/query_exec/*`

**Decision hooks**

- `Decision: Artifact Validity Is Predicate-Based, Not Key-Equality-Based` means the store must perform lookup, compatibility screening, and validity checking as separate steps.
- `Key Architectural Definitions -> Evidence Scope` means artifact legality may depend on whether supporting evidence is compile-invariant, snapshot-local, transition-compatible, or artifact-bound.

**Code sketch**

```rust
pub struct ArtifactStore {
    pub entries: BTreeMap<ArtifactKey, Vec<StoredArtifact>>,
}
```

**Acceptance criteria**

- A runtime artifact store exists.
- Artifacts can be inserted, looked up, invalidated, and reported.
- The store is not presentation-only in architecture.
- Reuse is not modeled as plain key equality alone.

#### Task 29B2 — Add artifact reuse and invalidation tests across snapshot and transition boundaries

**Description**

Prove that artifacts are reused only when their validity rules say so.

**Decision hooks**

- Tests should cover compatible policy ranges, transition-lattice acceptance, and evidence-scope expiry, not only snapshot-id mismatches.
- A passing test suite should make it hard for implementors to reintroduce raw key-equality semantics under schedule pressure.

**Files**

- new `compiler/tests/artifact_store.rs`
- `compiler/tests/presentation_exec.rs`
- `compiler/tests/query_exec.rs`

**Tests**

- support summaries reused across compatible snapshots
- history invalidated on incompatible transitions
- culling tables rejected when required evidence changed

### Workstream C: Physical Layout Planning

#### Task 29C1 — Split logical attachment and artifact contracts from physical layout plans

**Description**

Introduce a physical layout-planning layer that decides storage representation independently of logical meaning.

**Files**

- `compiler/presentation_exec/resources.rs`
- optionally new `compiler/artifact_layout/mod.rs`
- `compiler/presentation_contract/mod.rs`

**Implementation notes**

The first cut does not need to support every future layout.
It does need the architecture to make space for:

- linear buffers
- storage textures
- AoS vs SoA
- packed vs padded layouts

**Decision hooks**

- `Design Rule For This Phase` is the main guardrail here. Logical artifact contracts must remain about meaning, compatibility, and validity even after physical layout planning exists.
- Do not let physical storage enums leak back into semantic contracts as implicit meaning.

**Code sketch**

```rust
pub enum PhysicalArtifactStorage {
    LinearBuffer,
    StorageTexture2d,
}

pub struct ArtifactLayoutPlan {
    pub storage: PhysicalArtifactStorage,
    pub element_stride: u32,
    pub residency: ArtifactResidencyClass,
}
```

**Acceptance criteria**

- Logical artifact contracts no longer imply one storage representation.
- Resource allocation consumes a layout plan rather than inventing layout ad hoc.
- Existing byte-buffer allocation can remain as one layout strategy.

#### Task 29C2 — Thread layout plans through WGSL and execution adapters

**Description**

Ensure backend adapters consume layout plans rather than reconstructing assumptions about storage from logical contracts alone.

**Decision hooks**

- Backend adapters should treat layout plans as physical instructions, not as permission to reinterpret logical artifact meaning.
- If codegen needs semantic legality information, it should read the logical contract or policy/evidence inputs directly rather than inferring them from storage format.

**Files**

- `compiler/presentation_exec/wgsl.rs`
- `compiler/query_exec/wgsl.rs`
- `compiler/query_exec/wgsl/codegen.rs`
- `compiler/tests/portable_abi.rs`

**Acceptance criteria**

- WGSL emission can reference physical layout plans.
- Layout planning remains separate from logical meaning.
- Tests cover at least one alternate layout path beyond the current default assumptions.

#### Task 29C3 — Add a debug-only normalized projection for convergence pressure reporting

**Description**

Before the shared query-program spine becomes authoritative, add a report-only normalized vocabulary that lets engineers inspect overlap between current plans.

**Files**

- `compiler/bin/wrela/commands/shared.rs`
- `compiler/presentation_plan/mod.rs`
- `compiler/query_plan/mod.rs`
- optionally new `compiler/query_program_debug/mod.rs`

**Implementation notes**

This projection is intentionally:

- non-authoritative
- non-executing
- diagnostic only

Its job is to expose convergence pressure without creating abstraction debt.

This task may begin earlier than Phase 29 as a non-blocking diagnostics track if it helps expose convergence pressure during Phases 25-27.
Its authority level must remain unchanged even if it lands early.

**Decision hooks**

- `Design Rule 11` applies directly here. This projection is for visibility only and must never become an execution dependency by accident.
- If implementors are tempted to reuse it for real planning, that is a signal to wait for Phase 30 instead.

**Acceptance criteria**

- Reports can project current plans into a tiny normalized vocabulary.
- No execution path depends on this projection.
- The projection can be deleted or replaced later without semantic fallout.

### Phase 29 Exit Criteria

- Artifacts are one logical architecture.
- A runtime artifact store exists.
- Reuse and invalidation are tested against explicit validity rules.
- Logical and physical artifact layers are distinct.

### Lessons Carried Forward From Phase 29

- The stable seam is semantic contract plus explicit validity plus physical layout, not helper-shaped caches or byte-oriented shortcuts.
- Later observers should reuse the artifact substrate directly through explicit contracts, artifact uses, and validity rules while preserving concrete observer ownership.
- The debug normalized projection remains diagnostic-only until real overlap between presentation and collision justifies a shared spine.
- CLI and report surfaces may share observer-neutral helpers for semantic artifact, artifact-use, and validation reporting, but those helpers must not become execution ownership or substitute for concrete observer semantics.

## Phase 30: Collision Observer Foundation And Static Query Families

### Goal

Make collision a real second concrete observer with explicit contracts, plans, witnesses, and CPU-oracle-checkable semantics over immutable snapshots.

### Why this is sixth

By this point snapshots, policies, evidence, temporal model, and artifact runtime exist.
That is enough to build collision as a serious subsystem rather than as a bag of helper calls.

### Design Rule For This Phase

Collision questions must be first-class observer contracts, not ad hoc solver entrypoints hidden behind generic execution helpers.
This observer is about contact and overlap semantics, not rigid-body simulation or response.

### Decision Hooks For This Phase

- `Design Rule 10`
- `Key Architectural Definitions -> Concrete Observer Plan`
- `Decisions Locked By This RFC -> Artifact Validity Is Predicate-Based, Not Key-Equality-Based`
- `Design Rule 12`

### Parallelization Notes

- Workstream A owns collision contracts and witness schemas.
- Workstream B owns collision plan construction and observer wiring.
- Workstream C owns validation, CLI surfaces, and CPU-oracle fixtures.
- Task 30A1 should land before 30B1.

### Workstream A: Collision Contracts

#### Task 30A1 — Add explicit `collision_contract` support for static collision questions

**Description**

Create a real collision contract surface instead of hiding collision under presentation-shaped or generic query helper APIs.

Recommended initial scope:

- point containment and occupancy classification
- ray cast first-hit and miss-reason reporting
- overlap or separation witness production for a narrow supported shape family

**Files**

- new `compiler/collision_contract/mod.rs`
- `compiler/lib.rs`
- `compiler/bin/wrela/commands/shared.rs`

**Implementation notes**

Keep the first cut static and snapshot-scoped.
Do not blur transition-aware sweeps into this task; that belongs in Phase 31.
Do not let collision scope quietly expand into constraints, impulses, stacking, friction, or response.
Prefer the Phase 29 artifact vocabulary for witness and cacheable outputs instead of inventing collision-local cache languages.

**Decision hooks**

- `Design Rule 10` applies directly here. The collision observer must be a serious concrete subsystem, not a disguised spine demo.
- Contracts should declare witness schemas, guarantee classes, and required policy explicitly rather than burying them in execution code.

**Acceptance criteria**

- A real `collision_contract` surface exists.
- Collision inputs, outputs, and witness shapes are typed and explicit.
- Collision does not depend on presentation-specific contract records to exist.

### Workstream B: Collision Plan Construction

#### Task 30B1 — Add a concrete `CollisionPlan` and planner wiring

**Description**

Compile collision contracts into a concrete observer plan that can consume query primitives, evidence, and artifacts without losing collision ownership.

**Files**

- new `compiler/collision_plan/mod.rs`
- `compiler/query_plan/mod.rs`
- `compiler/lib.rs`

**Implementation notes**

The plan should own collision-specific passes such as candidate gathering, primitive evaluation, witness resolution, and output materialization.
It should not be a lightly renamed presentation plan.
If collision needs reusable intermediate state, describe it through `SemanticArtifactContract`, `ArtifactUse`, and explicit validity rules rather than hidden executor-local caches.

**Code sketch**

```rust
pub struct CollisionPlan {
    pub name: SmolStr,
    pub inputs: Vec<CollisionInputBinding>,
    pub passes: Vec<CollisionPass>,
    pub artifacts: Vec<SemanticArtifactContract>,
    pub outputs: Vec<CollisionOutputBinding>,
}
```

**Acceptance criteria**

- A second concrete observer exists as `CollisionPlan`.
- It consumes query primitives and artifacts through explicit plan records.
- CPU execution can remain the semantic oracle for static collision plans.

### Workstream C: Validation, Fixtures, And CLI

#### Task 30C1 — Add collision-plan validation, fixtures, and report surfaces

**Description**

Validate the collision observer as a real subsystem rather than as a sketch.
Report surfaces should reuse observer-neutral helpers for semantic artifacts, artifact uses, and validation summaries where helpful, without promoting a shared execution abstraction early.

**Decision hooks**

- Validation should prove concrete ownership, explicit dependencies, witness declarations, and CPU-oracle-checkable semantics.
- Do not accept tests that only show the observer can eventually project into a shared shape; it must stand on its own first.

**Files**

- new `compiler/tests/collision_plan.rs`
- `compiler/bin/wrela/commands/shared.rs`

**Tests**

- collision plans validate their dependencies and witness declarations
- point, ray, and overlap outputs remain CPU-oracle-checkable
- diagnostics can report required guarantee versus selected method class

### Phase 30 Exit Criteria

- A real collision observer exists.
- Static collision questions compile through explicit contracts and plans.
- Collision can be reasoned about independently of any future shared spine.

## Phase 31: Transition-Aware Collision Observer, Witnesses, And Runtime Integration

### Goal

Extend collision into a real gameplay-facing observer over snapshots and transitions, with explicit witness reuse and runtime integration.

### Why this is seventh

Once static collision is concrete, the temporal and artifact architecture can be exercised by a domain that genuinely needs it.

### Design Rule For This Phase

Transition-aware collision must state clearly whether it is answering a snapshot question or a change-over-time question.
It still does not own rigid-body simulation, constraint solving, or gameplay response.

### Decision Hooks For This Phase

- `Design Rules 4, 8-9, 12`
- `Key Architectural Definitions -> Artifact Compatibility Relation`
- `Decisions Locked By This RFC -> Evidence Provenance Is First-Class`

### Parallelization Notes

- Workstream A owns transition-aware collision contracts.
- Workstream B owns collision witness artifacts and reuse semantics.
- Workstream C owns CPU execution, validation, and diagnostics.
- Task 31A1 should land before 31C1.

### Workstream A: Transition-Aware Collision Contracts

#### Task 31A1 — Add sweep, time-of-impact, and transition-scoped collision questions

**Description**

Promote collision from static occupancy tests into explicit transition-aware queries that can answer movement and contact questions.

Recommended scope:

- segment or shape sweep
- earliest legal contact or no-hit certificate
- conservative versus exact collision guarantee classes

**Files**

- `compiler/collision_contract/mod.rs`
- `compiler/collision_plan/mod.rs`

**Implementation notes**

Do not let conservative broadphase rejection masquerade as an exact time-of-impact witness.
Transition-scoped contracts should declare the authority they require from transitions, snapshots, and evidence.

**Acceptance criteria**

- Transition-aware collision contracts exist.
- Snapshot versus transition authority is explicit in the contract surface.
- Witness schemas for contact, time fraction, and contact normal flavor are declared.

### Workstream B: Collision Artifacts And Witness Reuse

#### Task 31B1 — Add collision witness artifacts, continuation seeds, and reuse validity rules

**Description**

Treat collision support summaries, broadphase candidates, witness caches, and continuation seeds as first-class artifacts rather than hidden accelerators.

**Decision hooks**

- Collision reuse must go through the same compatibility and validity rules as the general artifact runtime.
- Witness reuse should be justified by transition class, evidence scope, and declared legality, not by raw cache presence.

**Files**

- `compiler/collision_plan/mod.rs`
- `compiler/artifact_key/mod.rs`
- `compiler/artifact_store/mod.rs`
- new `compiler/tests/collision_artifacts.rs`

**Acceptance criteria**

- Collision artifacts are declared and typed.
- Reuse and invalidation rules are explicit and testable.
- Plans can explain when witness reuse is legal or rejected.

### Workstream C: CPU Oracle Execution And Diagnostics

#### Task 31C1 — Add CPU collision execution, transition-aware validation, and witness diagnostics

**Description**

Make the collision observer executable and inspectable before asking it to justify any shared abstractions.

**Files**

- new `compiler/collision_exec/cpu.rs`
- `compiler/lib.rs`
- new `compiler/tests/collision_exec.rs`
- `compiler/bin/wrela/commands/shared.rs`

**Implementation notes**

CPU must remain the semantic oracle here.
GPU or WGSL specialization can wait until the observer semantics are trustworthy.

**Acceptance criteria**

- Static and transition-aware collision plans can execute on CPU.
- Validation covers witness reuse and invalidation under temporal changes.
- CLI and test fixtures can dump collision plans, witnesses, and reuse decisions.

### Phase 31 Exit Criteria

- Collision is a real observer over snapshots and transitions.
- Collision witness artifacts and reuse semantics are explicit.
- CPU oracle execution and diagnostics make collision independently inspectable.

## Phase 32: Shared Observer Vocabulary And Query-Program Spine

### Goal

Extract an explicit shared observer vocabulary and non-executing query-program spine from presentation and collision without collapsing observer ownership.

### Why this is eighth

This is the earliest point where shared observer abstractions are justified by two serious concrete observers rather than by aspiration.

### Design Rule For This Phase

Unifying abstractions must be real typed outputs with named ownership boundaries, not hand-waved future cleanup.

### Decision Hooks For This Phase

- `Design Rules 10-11`
- `Key Architectural Definitions -> Query-Program Spine`
- `Key Architectural Definitions -> Concrete Observer Plan`

### Architecture Notes Entering This Phase

- The shared seam justified by presentation and collision is a descriptive projection layer, not a shared authored observer-plan type.
- The strongest existing shared substrate is the semantic artifact vocabulary and artifact-store reuse/validity reporting; phase 32 should reuse that real substrate instead of inventing a parallel one.
- Shared node vocabulary should stay broad and graph-oriented. Concrete pass enums, runtime metrics, backend kernels, and observer-specific math remain observer-local.
- Shared reporting is the first concrete consumer. The current presentation-only normalized projection should be treated as a precursor to the real shared spine rather than as a separate long-term surface.
- Runtime observability overlaps only at summary level. Raw execution traces and per-observer runtime metrics must remain owned by their concrete observers.
- Validation helper extraction is allowed only for graph-shape and dependency invariants, not for collision authority/policy semantics or presentation view/frame semantics.

### Parallelization Notes

- Workstream A owns the shared observer vocabulary.
- Workstream B owns projection from concrete observers into the spine.
- Workstream C owns projection reports and regression fixtures.
- Task 32A1 should land before 32B1.

### Workstream A: Shared Observer Vocabulary

#### Task 32A1 — Add a non-executing `query_program_spine` module with explicit shared types

**Description**

Create the real shared layer extracted from presentation and collision.
This phase must produce a concrete vocabulary, not just a promise to generalize later.
This shared layer is a non-executing plan envelope over broad observer graph concepts, not a new common authored plan representation.

**Files**

- new `compiler/query_program_spine/mod.rs`
- `compiler/lib.rs`

**Implementation notes**

Required real outputs in the first cut:

- `QueryProgramSpine`
- `SpineNode`
- `SpineDependencyEdge`
- `ObserverProjection`
- `SpineObservabilitySummary`

Recommended initial node families:

- `InputBinding`
- `PrimitiveInvocation`
- `ArtifactLoad`
- `ArtifactStore`
- `PolicyRequirement`
- `DependencyEdge`
- `OutputBinding`
- `ObservabilitySummary`

These node families should capture broad graph roles rather than mirroring observer-specific pass enums.
This module should reuse the existing semantic artifact vocabulary wherever possible, including semantic artifact contracts, artifact uses, and artifact-store reuse/validity descriptions.
This module should not own backend kernels, observer-specific math, or raw runtime trace payloads.

**Decision hooks**

- The spine owns shared description, not observer semantics or execution ownership.
- The spine is the projection/report surface for concrete observers, not a replacement for `PresentationPlan` or `CollisionPlan`.
- If a node shape is only needed by one observer, keep it out of the spine until real overlap proves otherwise.
- Shared observability should be summarized at the spine boundary; detailed execution traces stay observer-local.

**Code sketch**

```rust
pub struct QueryProgramSpine {
    pub observer_kind: ObserverKind,
    pub inputs: Vec<SpineInputBinding>,
    pub nodes: Vec<SpineNode>,
    pub dependencies: Vec<SpineDependencyEdge>,
    pub outputs: Vec<SpineOutputBinding>,
    pub observability: SpineObservabilitySummary,
}
```

**Acceptance criteria**

- The shared spine exists.
- Its canonical vocabulary is explicit and named in code.
- It can represent both presentation and collision dependencies without absorbing observer-specific semantics.
- It reuses the existing shared artifact substrate instead of defining a second artifact abstraction stack.
- It is clearly non-executing and cannot be mistaken for a shared observer runtime.

### Workstream B: Observer Projection

#### Task 32B1 — Derive deterministic spine projections from presentation and collision plans

**Description**

Add projection from concrete observer plans into the shared spine.
Projection is where the common abstraction pays rent: concrete plans stay semantically rich and execution-owning, while the spine exposes the shared graph and artifact vocabulary.

**Decision hooks**

- Concrete observer plans remain the semantic owners. Projection should be deterministic and lossy only where the shared vocabulary is intentionally narrower.
- If projection pressure suggests the spine needs more nodes, add them only when both observers truly need them.
- Projection should prefer broad node families plus shared artifact/reuse vocabulary over observer-specific enum mirroring.
- Do not force presentation and collision into a shared native plan trait unless the trait only describes the projection boundary.

**Files**

- `compiler/presentation_plan/mod.rs`
- `compiler/collision_plan/mod.rs`
- `compiler/query_program_spine/mod.rs`

**Acceptance criteria**

- Both concrete observers can project into one shared spine representation.
- Projection is deterministic and testable.
- Concrete observer plans remain the execution owners.
- Projection makes shared inputs, dependency edges, query invocations, artifact lifecycle, and output bindings inspectable through one vocabulary.
- Intentional lossy boundaries are explicit where presentation or collision carries observer-local semantics that do not belong in the spine.

### Workstream C: Shared Projection Reports

#### Task 32C1 — Add shared dumps and regression fixtures for observer projections

**Description**

Make the new shared abstraction visible and testable so it proves its value immediately.
The first consumer of the spine should be reporting and fixtures rather than execution.

**Files**

- `compiler/query_program_spine/mod.rs`
- `compiler/bin/wrela/commands/shared.rs`
- `compiler/query_program_debug/mod.rs`
- new `compiler/tests/query_program_spine.rs`

**Acceptance criteria**

- Reports can show presentation and collision plans through one common vocabulary.
- Projection fixtures lock down intentional lossy boundaries.
- The shared vocabulary is inspectable without becoming executable.
- Shared reports cover the common graph/artifact/observability surface without forcing a fake shared runtime-trace schema.
- The old presentation-only normalized projection is either folded into the spine or explicitly reduced to a thin compatibility adapter.

### Phase 32 Exit Criteria

- A real shared observer vocabulary exists as a descriptive, non-executing spine rather than as a shared authored plan.
- Presentation and collision project into one non-executing spine.
- The shared abstraction reuses common artifact/reporting vocabulary instead of duplicating it.
- Concrete execution, authority, policy, temporal, and runtime-trace ownership remain local to their observers.
- The unifying abstractions are explicit code outputs, not roadmap narration.

## Phase 33: Shared Spine Analyses, Diagnostics, And Cross-Observer Validation

### Goal

Move the truly shared analyses and observability onto the spine and prove the new abstraction pays rent without swallowing observer semantics.

### Why this is ninth

Only after the vocabulary and projections are stable does it make sense to move shared reasoning upward.

### Design Rule For This Phase

Shared analyses may move upward only when they preserve observer ownership and reuse the validity semantics already established elsewhere.

### Decision Hooks For This Phase

- `Design Rules 8-13`
- `Key Architectural Definitions -> Query-Program Spine`
- `Decisions Locked By This RFC -> Artifact Validity Is Predicate-Based, Not Key-Equality-Based`

### Parallelization Notes

- Workstream A owns dependency and lifetime analysis.
- Workstream B owns policy, backend, and observability summaries.
- Workstream C owns cross-observer regression coverage and CLI diagnostics.
- Task 33A1 and 33B1 can start in parallel once Phase 32 projections stabilize.

### Workstream A: Dependency And Lifetime Analysis

#### Task 33A1 — Move dependency analysis and artifact lifetime checks onto the spine

**Description**

Promote only the analyses that are genuinely shared.

**Files**

- `compiler/query_program_spine/mod.rs`
- new `compiler/query_program_spine/validate.rs`

**Implementation notes**

The point is not to make the spine execute.
The point is to make shared reasoning live in one place.

**Decision hooks**

- Shared analyses should reuse the same artifact-validity, policy-legality, and dependency semantics already established in earlier phases rather than inventing a parallel ruleset.
- If an analysis still depends on observer-specific behavior, leave it concrete instead of forcing it upward.

**Acceptance criteria**

- Dependency graphs can be analyzed through the spine.
- Artifact lifetime validation can be shared.
- Observer-specific execution semantics remain outside the spine.

### Workstream B: Shared Policy And Backend Summaries

#### Task 33B1 — Add shared policy-legality, backend-summary, and observability analysis

**Description**

Centralize the summaries that both observers need when reporting, validating, or preparing backend work.

**Files**

- `compiler/query_program_spine/validate.rs`
- optionally new `compiler/query_program_spine/report.rs`
- `compiler/bin/wrela/commands/shared.rs`

**Acceptance criteria**

- Policy requirements can be summarized through the spine.
- Backend summaries can be derived without rebuilding observer-specific execution tables.
- Observability reports can use one common vocabulary.

### Workstream C: Cross-Observer Validation

#### Task 33C1 — Add cross-observer regression coverage and CLI diagnostics

**Description**

Prove the shared abstraction helps real observers without turning into a giant executor.

**Files**

- `compiler/tests/query_program_spine.rs`
- `compiler/bin/wrela/commands/shared.rs`

**Tests**

- projection determinism across presentation and collision fixtures
- shared validation for artifact lifetime and policy requirements
- diagnostics that report common structure without erasing observer-specific meaning

### Phase 33 Exit Criteria

- Shared analyses have moved upward onto the spine where they truly belong.
- Diagnostics can show observer plans through a common vocabulary.
- No execution path depends on the spine becoming a generic universal executor.

## Phase 34: Differential Semantics, Mixed Solver Planning, And Backend Convergence

### Goal

Use snapshots, policy, evidence, temporal semantics, artifacts, the collision observer, and the shared spine to unlock the intended solver and backend architecture.

### Why this is last

This is where the earlier phases pay off.

### Design Rule For This Phase

Use the strongest legal method per subtree, policy, artifact, and transition context.

### Decision Hooks For This Phase

- `Decisions Locked By This RFC -> Required Guarantee And Selected Method Class Are Separate`
- `Decisions Locked By This RFC -> Evidence Provenance Is First-Class`
- `Decisions Locked By This RFC -> Artifact Validity Is Predicate-Based, Not Key-Equality-Based`
- `Design Rule 12`

### Parallelization Notes

- Workstream A owns differential semantics and normal-role cleanup.
- Workstream B owns mixed solver planning and evidence-driven method selection.
- Workstream C owns backend convergence and parity.
- Task 34A1 and 34B1 can start in parallel once Phase 27 evidence, Phase 29 artifacts, and Phase 32 spine vocabulary are stable enough.

### Workstream A: Differential Semantics

#### Task 34A1 — Add differential evidence and symbolic derivative propagation for the supported semantic core

**Description**

Lift gradients and derivative availability into explicit compiler semantics for the supported primitive and transform set.

**Files**

- `compiler/scene_ir/mod.rs`
- `compiler/query_solver/mod.rs`
- `compiler/query_exec/cpu.rs`
- `compiler/query_exec/wgsl/codegen.rs`

**Implementation notes**

The first cut should handle:

- exact primitives
- rigid transforms
- uniform scale
- selected smooth operators

Fallback to finite differences only where evidence says certified differential behavior is unavailable.

**Decision hooks**

- Differential propagation is only allowed to strengthen behavior where the evidence model says it is legal. Unknown or heuristic derivative information must remain explicitly weaker.
- Evidence origin and scope still matter here; runtime-observed derivative hints are not compile-invariant truths.

**Acceptance criteria**

- Differential evidence exists and can be propagated.
- At least a useful semantic subset no longer requires central finite differences everywhere.
- CPU oracle tests cover differential correctness.

#### Task 34A2 — Split internal normal roles into certified field gradient, feature normal, and heuristic shading normal

**Description**

Stop treating "normal" as if it meant exactly one thing across shading, contact, and solver refinement.

**Files**

- `compiler/query_contract/mod.rs`
- `compiler/query_exec/world.rs`
- `compiler/query_exec/cpu.rs`
- `compiler/query_exec/wgsl/codegen.rs`

**Implementation notes**

The public surface may remain compatibility-shaped for now.
Internally the engine should know which flavor it is producing.

**Decision hooks**

- Keep certified field gradients, feature normals, and heuristic shading normals distinct in type or tagged representation.
- If an internal path falls back to a heuristic normal, diagnostics should be able to report that without pretending it satisfied a stronger guarantee.

**Acceptance criteria**

- Internal APIs can distinguish normal roles.
- Heuristic normals are no longer confused with certified gradients.
- Diagnostics can explain which normal flavor was used.

### Workstream B: Mixed Solver Planning

#### Task 34B1 — Upgrade `RaySolverPlan` to choose methods per subtree and candidate class

**Description**

Move from coarse whole-query portfolios toward evidence-driven mixed strategies.

**Files**

- `compiler/query_solver/mod.rs`
- `compiler/query_plan/mod.rs`
- `compiler/presentation_plan/mod.rs`
- `compiler/collision_plan/mod.rs`

**Implementation notes**

Candidate methods may include:

- dense sphere tracing
- support-bound rejection
- analytic primitive intersection
- Lipschitz-safe stepping
- interval isolation
- safeguarded Newton refinement
- repeat-aware traversal
- temporal continuation

**Decision hooks**

- `Decision: Required Guarantee And Selected Method Class Are Separate` applies directly here. Solver selection should cite both the required guarantee and the chosen method class.
- Evidence origin, scope, and transition compatibility should participate in selection legality anywhere they can invalidate an otherwise attractive method.

**Code sketch**

```rust
pub struct SolverSelection {
    pub subject: SnapshotEntityId,
    pub method: RaySolverMethod,
    pub method_class: SelectedMethodClass,
    pub evidence_summary: SmolStr,
}
```

**Acceptance criteria**

- Solver plans can express mixed method selection.
- Selection cites evidence and policy, not only contract id.
- CPU oracle fallback remains explicit.

#### Task 34B2 — Integrate artifact reuse and transition-aware continuation into solver plans

**Description**

Let solvers use compatible artifacts and temporal continuation explicitly when legal.

**Decision hooks**

- `Decision: Artifact Validity Is Predicate-Based, Not Key-Equality-Based` applies directly here. Solver reuse must go through the same compatibility and validity rules as the artifact runtime.
- Transition-aware continuation should be justified by change compatibility and evidence scope, not by "previous frame exists" shortcuts.

**Files**

- `compiler/query_solver/mod.rs`
- `compiler/artifact_store/mod.rs`
- `compiler/presentation_exec/wgsl.rs`

**Acceptance criteria**

- Solver plans can request compatible artifacts.
- Transition-aware continuation is explicit rather than ad hoc.
- Diagnostics report why continuation or artifact reuse was used or rejected.

### Workstream C: Backend Convergence And Validation

#### Task 34C1 — Stop rebuilding semantics from per-question matches in backend code where the new spine and solver plans can drive behavior

**Description**

Refactor backend execution to consume richer normalized inputs rather than repeating semantic reconstruction tables.

**Files**

- `compiler/query_exec/wgsl/codegen.rs`
- `compiler/query_exec/vgpu.rs`
- `compiler/query_exec/world.rs`
- `compiler/kernel/ir.rs`

**Implementation notes**

This task should be staged carefully.
Do not break CPU oracle clarity in the name of elegance.

**Decision hooks**

- `Design Rule 12` applies directly here. Normalized backend inputs are only a win if CPU-oracle-checkable semantics stay visible.
- Do not let shared spine or solver normalization erase concrete observer ownership or the required-guarantee versus selected-method distinction.

**Acceptance criteria**

- At least one major backend path consumes solver/evidence/spine-driven normalized behavior instead of rebuilding it from question matches alone.
- Concrete observer semantics remain intact.
- Validation coverage proves parity.

#### Task 34C2 — Add end-to-end parity and performance-closure benchmarks for the new architecture

**Description**

Prove the architecture works semantically and improves the right performance paths.

**Decision hooks**

- End-to-end validation should cover legality and reuse semantics, not only fps numbers: guarantee enforcement, evidence-origin/scope effects, and transition-aware artifact validity all belong in the harness.
- Benchmark reporting should make it clear when speedups come from legal reuse or stronger evidence rather than from silent semantic weakening.

**Files**

- `benchmarks/field_engine/`
- `compiler/tests/query_exec.rs`
- `compiler/tests/presentation_exec.rs`
- `compiler/tests/collision_exec.rs`
- new `compiler/tests/mixed_solver.rs`

**Tests**

- CPU vs WGSL parity on representative observer plans
- artifact reuse validity under temporal changes
- mixed solver correctness vs dense oracle
- performance harnesses for representative scenes and views

### Phase 34 Exit Criteria

- Differential semantics are first-class for a meaningful semantic subset.
- Solver plans can select mixed strategies from evidence.
- Backends begin consuming normalized behavior rather than reconstructing semantics from compatibility-era tables.
- End-to-end parity and benchmark coverage exist for the new architecture.

## Phase Ordering Summary

This roadmap intentionally lands in this order:

1. snapshot identity
2. semantic domain versus execution policy
3. unified evidence
4. temporal semantics and transitions
5. artifact runtime and logical/physical split
6. collision observer foundation and static query families
7. transition-aware collision, witnesses, and runtime integration
8. shared observer vocabulary and query-program spine
9. shared spine analyses and diagnostics
10. differential semantics and mixed solver/backend convergence

## Recommended Active Scheduling Window

Treat this RFC as the north-star roadmap, but only actively schedule **Phases 25-27** at first.

Treat **Phases 28-34** as committed direction rather than committed near-term execution until the earlier phases prove themselves in one real vertical slice.

That posture keeps the roadmap useful without turning it into a rigid religious text.

## Vertical Slice Review Gate

Before treating Phases 28-34 as active execution rather than directional commitment, run one explicit vertical-slice review over a concrete observer path.

The vertical slice should prove at least:

1. real snapshot identity and epochs
2. distinct semantic domain and execution policy
3. unified evidence with explicit origin
4. one artifact reuse path validated by declared compatibility and validity rules
5. one observer path that becomes simpler rather than more abstractly tangled because of the new architecture

## Why This Order

This order preserves the architectural constraints that matter most:

- identity before reuse
- legality before optimization
- evidence before solver sophistication
- transition semantics before temporal reuse becomes foundational
- concrete observers before generic shared layers

Reversing that order would make the system feel more abstract faster, but it would also increase the chance of hardening the wrong abstractions.

## Recommended Team Parallelization Model

For a small team, the cleanest ownership split is:

- **Track A: Contracts, ABI, and semantic model**
  Owns portable records, domain/policy split, time semantics, evidence types.

- **Track B: Runtime and artifacts**
  Owns snapshot context, artifact store, layout planning, reuse/invalidation runtime.

- **Track C: Concrete observers**
  Owns presentation-plan migration, collision observer, and eventual shared observer projection/spine.

- **Track D: Solvers and backends**
  Owns evidence-driven solver planning, CPU oracle parity, vGPU/WGSL convergence.

- **Track E: Tests, diagnostics, and CLI**
  Owns report surfaces, debug output, snapshot/evidence/temporal regression suites, and benchmark harnesses.

Within a phase, each workstream is intended to be claimable by one engineer with clearly bounded write ownership.
When a task needs a shared type to exist first, that dependency is called out explicitly.

## Implementor Notes For Junior Engineers

If you pick up a task from this RFC:

1. Start from the "Files" list.
2. Read the phase-level and task-level `Decision hooks` before editing code.
3. Read the surrounding types before changing anything.
4. Preserve the existing public authored surface unless the task explicitly says to cut it.
5. Add tests at the same time as the implementation.
6. If a task introduces a new typed concept, add a small debug or CLI report path so the concept is visible.
7. When a phase says CPU oracle first, do not start in WGSL.
8. If two interpretations are possible, prefer the one that preserves semantic meaning and makes legality explicit.

## What To Tackle First

Start with **Phase 25, Workstream A**.

That is the foundation that the later phases depend on most heavily:

- typed identity
- real epochs
- snapshot-aware execution context

After that, the best next parallel move is:

- one engineer on Phase 26 Workstream A
- one engineer on Phase 25 Workstream B
- one engineer preparing Phase 27 evidence-model scaffolding

## Recommended Follow-Up RFCs After This Roadmap

Once this roadmap is substantially landed, the next RFCs should likely be:

1. public authored transition and state-advance surface for gameplay logic
2. public transition-query families and authored temporal world surface
3. concrete gameplay observer plans over snapshots and transitions
4. navigation and affordance families built on artifact/runtime validity
5. distributed or networked deterministic world execution if that becomes a real requirement

## Final Recommendation

Use this RFC as the architectural north-star implementation roadmap, not as a demand to build the final abstraction all at once.

The intended development posture is:

- keep the end-state in view
- land the smallest cuts that preserve meaning
- prove overlap before generalizing
- make time, evidence, and artifacts first-class
- then let performance work and future gameplay systems scale through that substrate

That path is the most likely one to produce the engine the repo vision is pointing at:

- semantic
- portable
- testable
- temporally coherent
- artifact-aware
- and powerful enough to support both high-performance presentation and real gameplay over one world model
