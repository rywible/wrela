# RFC 0003: Phase 9.5 Semantic Convergence Plan

Status: Proposed

Author: Codex

Created: 2026-04-08

Target: `wrela` compiler semantic convergence before Phase 10 WGSL backend work

## Summary

This document defines a short but deep convergence phase between Phase 9 and Phase 10 of the field-engine roadmap.

Its purpose is not to add new authored language surface area. Its purpose is to remove the remaining architectural split between:

- authored semantic intent
- semantic middle-end representation
- CPU truth behavior
- virtual GPU behavior
- future WGSL backend contracts

The central conclusion of this plan is:

- the language direction is already strong enough
- the ABI groundwork is already strong enough
- the execution architecture is not yet converged enough for broad Phase 10 work

Phase 9.5 is therefore a hardening and convergence phase. It makes Scene IR authoritative, closes the direct-runtime truth gaps, turns query-plan and kernel artifacts into real data contracts, keeps render and preview on those same contracts, and makes virtual GPU a real differential lane instead of a thin wrapper over the direct CPU evaluator.

This phase exists to ensure that WGSL lands as a backend for one semantic engine rather than as the point where several partially overlapping engines are forced to reconcile under pressure.

## Relationship To RFC 0002

This RFC is a refinement of [RFC 0002](/Users/ryanwible/projects/wrela/language/spec/rfcs/0002-field-engine-implementation-roadmap.md), not a replacement.

RFC 0002 already states that:

- Scene IR and query plans should become the semantic source for Phase 9+
- generated kernels should preserve truthful local provenance and identity
- CPU and virtual GPU should share one kernel contract
- WGSL should start only after that contract is solid

Phase 9.5 exists because the current implementation has landed the direction of Phase 9, but not all of its architectural closure.

The practical reading is:

1. Phase 9 introduced the right abstractions.
2. Those abstractions are not yet authoritative enough to safely carry full Phase 10 scope.
3. Phase 9.5 finishes that convergence before broad WGSL bring-up.

## Motivation

The current implementation has several clear strengths:

- HIR semantics are coherent and strongly typed.
- Exactness discipline is real rather than aspirational.
- Opaque leaves are explicitly quarantined.
- Portable ABI layout work is already aligned with WGSL expectations.
- The project already protects semantic and product-level behavior with meaningful tests.

The main remaining risk is not missing concepts. It is semantic duplication.

Today, meaning still leaks across multiple partially overlapping layers:

- HIR carries semantics that later layers do not fully preserve.
- Scene IR captures the right shape of the middle-end, but not the full semantic payload.
- Query plans encode strategy intent, but not yet enough concrete executable contract.
- Kernel lowering mostly transports plan metadata.
- The direct CPU path still performs important semantic choices itself.
- Virtual GPU still mostly proves parity against shared direct behavior rather than against a genuinely separate backend implementation of the same contract.
- Preview and render are not yet explicitly locked to the same semantic execution boundary that future backend work depends on.

That split is survivable while CPU remains the only real truth lane. It becomes much more expensive once WGSL is introduced, because every missing semantic contract turns into one of:

- backend drift
- backend-specific ad hoc reconstruction
- widened parity tolerances that hide real bugs
- slower or less trustworthy bring-up

Phase 9.5 is therefore an explicit investment in backend correctness velocity.

## Problem Statement

The project is currently in the following state:

### What Is Already Good

- The authored language hierarchy is right:
  - `field` for geometry semantics
  - `shape` for material, payload, radiance, and volume binding
  - `region`, `domain`, and `render` for composition and evaluation policy
- Exactness rules are enforced in type checking rather than left informal.
- Opaque custom math is recognized as an optimization boundary instead of being treated as normal scene algebra.
- Portable ABI alignment and layout rules are explicit and tested.
- Query-plan determinism, capture/query execution, and preview/spec integrity already have real regression coverage.

### What Is Not Yet Converged

- Shape provenance exists in HIR but is not preserved in Scene IR.
- Scene IR still carries duplicated HIR trace metadata instead of owning its own complete semantic analysis product.
- Scene value operands are still expression trees rather than typed operator payloads or typed parameter handles.
- Support algebra remains mostly classificatory and symbolic rather than concrete enough to power real artifact generation.
- Query plans describe strategies and stages, but derived artifacts are still mostly declarative placeholders.
- Kernel lowering is mostly structural plan transport rather than a decisive executable abstraction boundary.
- The direct CPU path still contains winner selection, hit assembly, and participant selection behavior that does not yet match the semantic ambition of the rest of the pipeline.
- The virtual GPU path still delegates heavily to the direct CPU query ops and does not yet act as an independently valuable differential backend.
- The current plan does not yet make render and preview explicitly subordinate to the same world/query-plan/kernel contracts as other scene queries.

## Explicit Goals

Phase 9.5 has six goals.

### 1. Make Scene IR The Canonical Semantic Middle-End

Scene IR must become the single compiler-owned source of semantic scene meaning after HIR lowering.

This means Scene IR must own, directly or through tightly attached analysis tables:

- distance semantics
- support semantics
- provenance / winner policy
- identity-bearing repeat and instance data
- detail participation and domain eligibility where applicable
- the data needed to reconstruct truthful local hit context

### 2. Remove Semantic Drift Between CPU Truth And Later Backend Contracts

The CPU truth path must not retain hidden semantics that bypass Scene IR, query plans, or generated kernel contracts.

Where the current direct runtime performs semantic decisions itself, those decisions must either:

- move into the canonical semantic middle-end and its derived contracts, or
- be rewritten as a faithful execution of those contracts

### 3. Turn Query Plans And Derived Artifacts Into Real Contracts

Query plans must evolve from high-quality metadata into executable contracts with explicit data shapes.

Derived artifacts must stop meaning "something of this kind should exist later" and start meaning "this data exists with this schema, this ownership, and this backend-stable interpretation."

### 4. Make Virtual GPU A Real Differential Lane

Virtual GPU must stop being primarily a wrapper over direct CPU evaluation and become a fast backend that executes the same generated contracts that WGSL will eventually consume.

Its value is not visual plausibility. Its value is contract validation.

### 5. Freeze Backend-Neutral Hit, Dispatch, And Artifact Semantics Before WGSL

Before meaningful WGSL work begins, the project must already know:

- what a trace dispatch record is
- what a result record is
- what a culling or accelerator table contains
- how local hit context is assembled
- how provenance, participant selection, and identity flow through all of those records

### 6. Bound The First Phase 10 Scope

Phase 9.5 must leave the project in a state where a narrow initial WGSL slice can ship safely.

The first WGSL slice should be distance and typed batch query work over already-frozen contracts, not the full trace/surface/radiance/media stack all at once.

## Cross-Cutting Rules

The following rules apply to every Phase 9.5 workstream.

### Render And Preview Are Contract Consumers

Render and preview must remain consumers of the same compiler-owned world/query-plan/kernel contracts as other execution paths.

Phase 9.5 must not create or preserve:

- a separate render-only hit assembly path
- a preview-only participant-selection path
- a render-specific provenance model
- a presentation-only execution model that bypasses Scene IR or query-plan contracts

If render or preview need special metadata, detail policy, or presentation-specific derived artifacts, those must layer on top of the same semantic execution boundary rather than creating a second engine.

### Convergence Must Preserve Or Improve Measurable Efficiency

Phase 9.5 is not complete if it only improves semantic cleanliness while materially regressing the performance architecture that RFC 0002 already requires.

Every workstream that adds contract or artifact machinery must also maintain or improve observability for:

- candidate counts
- branch visits
- support-pruning effectiveness
- culling hit rates
- artifact sizes
- dispatch overhead
- query result throughput for representative CPU and virtual GPU fixtures

Phase 9.5 does not require final tuned performance. It does require proof that the converged architecture still supports meaningful pruning and dispatch wins rather than turning every query into a more expensive but cleaner abstraction stack.

## Non-Goals

Phase 9.5 does not aim to:

- redesign the authored language
- add broad new user-facing syntax
- make every field operator exact
- solve every future acceleration strategy
- add a final production-quality WGSL backend
- add mesh or asset escape hatches
- preserve backward compatibility for any temporary internal execution path

## Core Decision

The key decision of this RFC is:

The semantic field/render pipeline shall converge to:

1. authored declarations
2. typed HIR
3. canonical Scene IR plus attached semantic analyses
4. explicit query plans
5. portable kernel and artifact contracts
6. CPU oracle, virtual GPU, and WGSL backend execution over those same contracts

MIR remains useful, but it is not the semantic home for scene meaning.

MIR may still be used for:

- general lowering
- helper generation
- portable function lowering
- debug or trace-oriented synthesized code

MIR must not remain the place where critical scene semantics live only implicitly.

Render and preview also remain consumers, not alternate semantic owners. Any render plan or preview plan introduced during this phase must lower from the same canonical Scene IR and shared query-plan/kernel contracts rather than becoming a second execution model.

## Current Gaps To Close

This section names the specific convergence gaps that Phase 9.5 must close.

### Gap A: Scene IR Does Not Yet Preserve Full Shape Semantics

Current issues:

- HIR shape graphs carry provenance policy and provenance trees.
- Scene IR shape scenes do not.
- Scene IR shape and field scenes still carry `hir::GraphTraceMetadata`.
- Scene IR nodes are still primarily tree-by-value with `Use { target }` references.
- operator payloads are still expressed through lightweight expression trees rather than typed payload records.

Why it matters:

- provenance and winner selection are not optional details; they are part of semantic truth
- local hit context cannot be reconstructed faithfully without semantic ownership
- later plans and backends should not need to re-open HIR to recover meaning Scene IR was supposed to own

### Gap B: Support Algebra Is Not Yet Concrete Enough

Current issues:

- support is represented structurally, which is good
- but the payload is still too weak for serious culling and backend-stable artifact generation
- support classes such as bounded/periodic/unbounded are useful summaries, but not sufficient artifact contracts

Why it matters:

- culling tables and future accelerators need concrete support payloads
- backend lowering should consume support geometry, not reconstruct it from heuristics
- support-driven pruning should be explainable in terms of actual support data

### Gap C: Query Plans Are Still More Descriptive Than Contractual

Current issues:

- plans encode helper names, stages, and strategy enums
- derived artifacts are still symbolic
- candidate generation and pruning are not yet represented as concrete dataflow over explicit candidate sets and artifact schemas

Why it matters:

- WGSL needs real layouts and record contracts
- CPU and virtual GPU parity should validate data contracts, not just stage names
- debugging backend drift requires explicit contract surfaces

### Gap D: Kernel Lowering Is Not Yet A Strong Enough Abstraction Boundary

Current issues:

- query-plan lowering into kernel plan structs is mostly a structural copy
- the batch-query interpreter primarily reflects per-item stage traces rather than exercising richer executable semantics

Why it matters:

- a thin transport layer is not yet the shared backend contract RFC 0002 calls for
- backends need a stable, explicit interpretation of plan and artifact data

### Gap E: Direct CPU Execution Still Has Truth Gaps

Current issues that must be treated as architectural bugs, not merely TODOs:

- opaque leaves use a placeholder sentinel distance instead of a principled conservative fallback policy
- some authored field operators still resolve to unsupported behavior in the direct truth path
- trace winner selection still relies on first-leaf fallback in places where provenance should decide ownership
- local hit context remains partially synthetic instead of truthfully assembled
- participant selection for radiance and media is still first-leaf based rather than winner based

Why it matters:

- CPU is the oracle
- if CPU truth is weaker than semantic intent, every later backend comparison becomes less meaningful

### Gap F: Virtual GPU Is Not Yet Independent Enough

Current issues:

- virtual GPU still delegates core operations to direct CPU query ops
- current parity mainly proves shared direct semantics agreement plus dispatch scaffolding

Why it matters:

- the virtual GPU lane should catch contract errors before real WGSL
- if it shares too much implementation with direct CPU semantics, its bug-detection value is limited

## Phase 9.5 Workstreams

Phase 9.5 is organized into seven workstreams. They are ordered. Some may overlap, but their dependency direction is intentional.

## Workstream 1: Canonical Scene IR

### Goal

Promote Scene IR from "promising semantic middle-end" to "authoritative semantic source after HIR."

### Required Changes

1. Introduce stable Scene IR node identities.

Scene IR must support stable references that survive analysis and artifact derivation. A tree-only representation is insufficient for canonical ownership, artifact attachment, and explicit reuse.

Minimum requirements:

- stable field node IDs
- stable shape node IDs
- stable support node IDs or support handles
- stable leaf IDs aligned with feature/provenance tracking

Scene IR IDs must be clearly related to, but not confused with, ABI-visible runtime identity fields such as:

- `feature_id`
- `instance_id`
- `repeat_id`
- `root_shape_id`

The convergence pass must define which IDs are:

- compiler-internal semantic graph identities
- ABI-visible query result identities
- derived mappings between the two

2. Preserve shape provenance in Scene IR.

Scene IR must represent:

- union provenance policy
- intersection provenance policy
- subtraction provenance policy
- nested provenance structure

This may be encoded:

- directly on shape nodes, or
- in a tightly attached analysis structure keyed by Scene IR node IDs

Either approach is acceptable if Scene IR remains the canonical source.

3. Replace duplicated HIR trace metadata with Scene IR-owned analysis products.

`hir::GraphTraceMetadata` may still exist upstream, but Scene IR should no longer carry it as a hidden borrowed reservoir of meaning.

Replace it with Scene IR analysis outputs such as:

- trace safety summary
- opaque boundary summary
- support-pruning eligibility summary
- local-context preservation summary

4. Replace lightweight operator operands with typed semantic payloads.

Examples:

- `Translate { offset: TypedVec3Operand, inner }`
- `UniformScale { scale: TypedScalarOperand, inner }`
- `RepeatGrid { repeat: TypedVec3Operand, inner }`
- `Extrude { height: TypedScalarOperand, profile: ProfileSceneHandle }`

Allowing a general expression DAG for portable parameterization is acceptable, but operators must reference it through typed handles rather than arbitrary ad hoc expression payloads.

5. Separate summaries from truth.

Scene IR should preserve both:

- canonical semantic truth
- derived summary views for planning

Summaries such as support class or opaque-boundary booleans remain useful, but they must be derived from richer canonical representations.

6. Keep render- and preview-facing semantic views derived.

If Scene IR grows render-facing summaries or plan-entry metadata, those must be explicitly derived views over the same canonical semantic nodes rather than a separate render IR that can drift semantically.

### Deliverables

- canonical Scene IR node ID model
- provenance-preserving shape scene representation
- typed operator payload model
- Scene IR semantic analysis tables
- updated lowering/tests proving stable deterministic construction

### Acceptance Criteria

- no downstream phase-critical semantic logic needs to recover provenance from HIR
- Scene IR can answer winner/provenance questions without reopening HIR
- Scene IR can identify leaves, instances, and repeat-origin identity through stable handles
- Scene IR snapshots are deterministic and semantically complete enough for planning
- Scene IR-to-render and Scene IR-to-preview views are derived from the same canonical nodes rather than separate semantic sources

## Workstream 2: Concrete Support Algebra

### Goal

Upgrade support from symbolic structure plus coarse class flags into a backend-stable semantic geometry algebra.

### Required Changes

1. Define a concrete support algebra.

The initial support model must include at least:

- `Unknown`
- `Unbounded`
- `Aabb`
- `Sphere`
- `Union`
- `Intersection`
- `Difference`
- `Transform`
- `Periodic`
- `Repeat`
- `OpaqueBoundary`

This RFC does not require a final perfect support algebra. It requires one concrete enough to back real contracts.

2. Attach typed payloads to support nodes.

Examples:

- `Aabb { min, max }`
- `Sphere { center, radius }`
- `Transform { transform, inner }`
- `Periodic { cell, motif }`
- `Repeat { kind, period, motif }`

3. Define compositional lowering rules.

Each field and shape operator must define:

- how support is derived
- what information is preserved
- when support degrades to unknown
- when opaque boundaries stop specialization

4. Split support truth from support summaries.

The planner still needs fast summary queries such as:

- bounded?
- periodic?
- can coarse support pruning apply?

Those summaries should become cached derivations from concrete support payloads.

5. Preserve support observability.

Support-driven pruning remains a performance architecture feature, not just a semantic bookkeeping feature.

The support system should therefore expose counters or traceable metrics for:

- rejected candidates
- branches skipped due to support
- support-derived culling-table effectiveness
- unknown-support fallback frequency

### Deliverables

- support algebra type definitions
- support propagation table or pass definitions
- support summary derivation pass
- tests for transform/boolean/repeat/construction support composition

### Acceptance Criteria

- culling or pruning artifacts can reference concrete support payloads
- support propagation does not need to infer meaning from unrelated trace metadata
- support summaries remain deterministic and conservative
- support-driven pruning remains measurable through counters or execution traces that can prove it still reduces real work

## Workstream 3: Truthful Provenance And Local Hit Context

### Goal

Make provenance, local frame data, and identity-bearing hit fields truthful compiler-owned semantics rather than best-effort direct-runtime reconstruction.

### Required Changes

1. Define canonical winner selection semantics.

The compiler contract must state how winner selection works for:

- union nearest
- union ordered
- intersection nearest
- intersection ordered
- subtract left
- subtract right
- opaque boundaries

2. Define local context assembly rules.

Every hit-capable path must preserve or compute:

- world position
- world normal
- leaf-local position
- leaf-local normal
- shading frame
- feature ID
- instance ID
- repeat ID
- root shape ID
- payload

3. Make participant selection provenance-aware.

Surface, radiance, and medium selection must be driven by truthful winner semantics, not by first-leaf fallback.

4. Define identity propagation rules for repeat and instance semantics.

The contract must state:

- what repeat identity means
- what instance identity means
- where those values are assembled
- how they survive batch and backend execution

5. Define opaque-leaf behavior in hit assembly.

Opaque leaves may degrade optimization and exactness, but they must not silently invent incorrect winner semantics.

6. Keep render and preview hit semantics aligned.

Any render or preview path that consumes hit records must use the same provenance, local-frame, identity, and participant-selection contract as direct query execution.

### Deliverables

- provenance and winner-selection contract
- local hit context contract
- participant-selection contract
- regression tests for local frame and identity-bearing fields

### Acceptance Criteria

- there is one explicit compiler contract for hit assembly
- trace, surface, radiance, and medium queries all consume the same provenance semantics
- local context is no longer assembled through first-leaf shortcuts
- preview and render hit consumption cannot diverge from the same local-context contract

## Workstream 4: Query Plans As Real Contracts

### Goal

Turn query plans from stage-labeled metadata into backend-neutral executable contracts.

### Required Changes

1. Extend query plans with explicit contract payloads.

Plans must not stop at strategy enums. They should carry or reference:

- candidate record shapes
- result record shapes
- artifact handles
- domain flag requirements
- hit-context requirements
- participant-selection requirements

2. Make candidate generation explicit.

At minimum, plans for trace and occlusion work should explicitly model:

- candidate source
- candidate record identity
- pruning inputs
- winner-selection mode

3. Make pruning explicit.

A pruning stage should be backed by:

- a support lower-bound contract, or
- a culling-table contract, or
- an explicit conservative traversal fallback

4. Add real artifact contracts.

For each derived artifact family, define:

- schema
- producer
- consumer
- determinism guarantees
- backend interpretation

The minimum set is:

- capture cache contract
- support summary contract
- culling table contract
- dispatch record contract
- hit/result buffer contract

Render and preview plans that consume query results must use these same contracts or explicit derived views over them.

5. Separate planning from execution policy defaults.

The planner may still choose strategies automatically, but the chosen contract must be explicit in the resulting plan.

6. Add performance-aware planning validation.

Query-plan evolution in this phase must remain observable with respect to:

- candidate counts before and after pruning
- branch visits
- culling-table hit rates
- artifact memory footprint
- dispatch/setup cost relative to useful work

### Deliverables

- expanded query-plan data model
- artifact schemas
- planning rules for candidate/pruning/winner/participant stages
- tests for contract determinism and contract parity
- planning observability for pruning effectiveness and dispatch/artifact cost

### Acceptance Criteria

- plans can be inspected as executable contracts rather than inferred as policy hints
- culling and hit/result records have stable definitions before WGSL
- plan-to-kernel lowering consumes explicit contracts, not missing implicit knowledge
- render and preview plan consumers do not introduce a parallel execution contract
- planning improvements can be justified with counters rather than only by architectural taste

## Workstream 5: Stronger Kernel And Artifact Boundary

### Goal

Make kernel lowering the stable transport and execution boundary described in RFC 0002.

### Required Changes

1. Define kernel-facing plan and artifact records as first-class transport types.

Kernel-facing structures must be explicit about:

- dispatch records
- input record shapes
- output record shapes
- artifact bindings
- local-context preservation requirements

2. Reduce semantic reconstruction inside kernel execution.

Kernel execution should consume pre-decided semantics:

- candidate rules
- pruning rules
- winner rules
- participant rules

It should not rediscover them from unrelated higher-level structures.

3. Upgrade plan interpretation and tracing.

The current tracing view is useful for observability, but Phase 9.5 should add richer execution-aware validation for:

- artifact usage
- candidate flow
- winner selection
- dispatch/result record conformance

4. Freeze the first backend-stable contract slice.

The first frozen slice should cover:

- field distance batch
- shape distance batch
- shape trace batch
- occlusion batch
- result record layouts
- artifact loading conventions

Contract families introduced in this phase must also declare:

- an owning module
- a contract version location
- the rule for bumping that version when layouts or semantics change

### Deliverables

- strengthened kernel plan/record types
- artifact binding contract
- execution-aware kernel trace validation
- parity tests between query plans and kernel contracts
- contract version ownership and bump rules

### Acceptance Criteria

- kernel lowering is more than a structural copy of plan metadata
- CPU and virtual GPU execute clearly defined contracts at this boundary
- WGSL can target this boundary without inventing additional semantics
- contract versions are explicit enough for virtual GPU and future WGSL validation to detect mismatch cleanly

## Workstream 6: CPU Oracle Closure

### Goal

Bring the CPU truth path fully in line with the semantic contracts frozen by Workstreams 1 through 5.

### Required Changes

1. Replace placeholder opaque fallback with principled conservative policy.

Opaque leaves must use an explicit conservative fallback lane.

Acceptable interim forms include:

- interval-style conservative evaluation
- explicit support-constrained pessimization
- contract-defined fallback query path

Unacceptable forms include:

- arbitrary sentinel distances with no stated contract

2. Close direct runtime operator gaps.

For field operators exposed by the authored language and expected in Phase 9+ truth paths, the CPU oracle must either:

- implement them truthfully, or
- route them through the generated canonical execution path

This especially applies to construction/profile operators and any operator already admitted into the semantic authored surface.

3. Remove first-leaf selection shortcuts from semantic queries.

The CPU oracle must no longer use first-leaf fallbacks for:

- trace winner selection
- radiance participant selection
- medium participant selection

4. Replace synthetic hit assembly with truthful hit assembly.

Identity, local coordinates, local normals, and shading frame data must come from the canonical contract.

5. Route world queries through the same semantic contract family.

World queries must remain policy-aware through domains, but their underlying shape/hit behavior should still be the same truthful contract as direct shape work.

6. Preserve performance observability while closing truth gaps.

Truth fixes must not blind the compiler/runtime to whether support pruning, culling, and dispatch selection still reduce work. CPU oracle closure should therefore retain or expand metrics for:

- branch visits
- field sample counts
- trace step counts
- support-pruned candidate rejection
- opaque fallback frequency

### Deliverables

- closed CPU fallback policy for opaque leaves
- direct runtime coverage or rerouting for currently exposed operators
- truthful hit assembly implementation
- winner-aware surface/radiance/medium selection
- regression tests for all of the above

### Acceptance Criteria

- CPU can serve as a trustworthy oracle for backend parity
- authored surface availability and CPU truth behavior no longer materially disagree
- hit/local-frame regression tests pass through the canonical path
- CPU truth closure still leaves measurable evidence that planning and pruning help rather than disappear into a black-box evaluator

## Workstream 7: Honest Virtual GPU

### Goal

Turn virtual GPU into the fast contract-validation lane between CPU truth and real WGSL.

### Required Changes

1. Make virtual GPU consume kernel and artifact contracts directly.

Virtual GPU must execute:

- dispatch records
- artifact bindings
- result record layouts
- candidate/pruning/winner semantics

through its own backend adapter rather than direct CPU shortcuts wherever feasible.

2. Keep CPU as oracle but reduce shared implementation at the semantic decision points.

Sharing low-level arithmetic helpers is acceptable.

Sharing high-level semantic decisions such as winner selection, participant choice, or opaque fallback policy defeats the purpose of the lane.

3. Add backend-differential tests at the contract boundary.

Minimum differentials:

- CPU vs virtual GPU distance parity
- CPU vs virtual GPU batch parity
- CPU vs virtual GPU trace/hit parity
- CPU vs virtual GPU artifact contract parity
- CPU vs virtual GPU local-context parity

4. Add failure-mode tests.

Virtual GPU should be able to detect:

- artifact schema mismatches
- missing required plan stages
- inconsistent dispatch/result records
- incompatible contract versioning

5. Add lightweight backend-cost observability.

Virtual GPU should expose enough execution counters to compare:

- dispatch/setup work
- candidate counts
- pruned candidates
- contract-validation failures
- artifact loads

### Deliverables

- virtual GPU backend adapter over shared contracts
- contract-differential tests
- backend validation errors for contract mismatch cases
- virtual GPU execution counters for backend-cost comparison

### Acceptance Criteria

- virtual GPU is no longer primarily a wrapper over direct query ops
- parity demonstrates agreement across a real backend boundary
- the lane is useful for WGSL bring-up rather than merely decorative
- virtual GPU comparisons can surface contract-correct but cost-regressive changes

## Execution Order

Phase 9.5 should land in the following order.

1. Canonical Scene IR
2. Concrete support algebra
3. Truthful provenance and local hit context
4. Query plans as real contracts
5. Stronger kernel and artifact boundary
6. CPU oracle closure
7. Honest virtual GPU
8. Narrow Phase 10 distance/batch WGSL spike

The most important sequencing rule is:

Do not begin broad trace/surface/radiance/media WGSL bring-up until Workstreams 1 through 7 have reached exit criteria.

## Deliverable Breakdown By Area

This section translates the workstreams into likely code ownership areas.

### HIR

HIR work in Phase 9.5 should stay limited and intentional.

Expected responsibilities:

- continue to define authored semantic structure
- remain the source of initial provenance and operator meaning
- stop serving as the hidden reservoir for downstream execution truth after Scene IR lowering

### Scene IR

Expected responsibilities:

- canonical semantic node graph
- typed operator payloads
- support algebra
- provenance and identity-bearing semantic attachments
- semantic summaries for planning

### Query Plan

Expected responsibilities:

- explicit capture/query/world plan contracts
- render-plan and preview-plan consumption contracts that stay subordinate to the same semantic engine
- candidate generation and pruning contract selection
- artifact schema references
- local-context and participant-selection requirements

### Kernel

Expected responsibilities:

- stable transport of plans and artifacts into executable lower-lane records
- backend-neutral dispatch and result record contracts
- explicit contract version ownership
- execution-aware validation and tracing

### Query Execution

Expected responsibilities:

- execute canonical contracts
- stop owning hidden semantic truth
- preserve CPU oracle quality
- support real virtual GPU differential work
- preserve or improve observability of pruning, dispatch, and artifact cost

### Portable ABI

Expected responsibilities:

- continue to define and validate record layout determinism
- expand as needed to cover newly frozen artifact or dispatch/result contracts
- remain explicit and testable before WGSL emission
- remain aligned for preview/render consumers of those same records

## Testing Plan

Phase 9.5 is not complete without a broader and more contract-focused test matrix.

### Scene IR Tests

- deterministic lowering snapshot tests
- provenance-preservation tests
- typed payload lowering tests
- support algebra composition tests
- opaque-boundary propagation tests

### Query Plan Tests

- determinism tests
- artifact schema tests
- candidate/pruning selection tests
- winner/participant stage coverage tests
- invalid-contract validation tests
- render/preview contract-consumption parity tests
- planning effectiveness counters for representative scenes

### Kernel Tests

- plan-to-kernel contract parity tests
- dispatch/result record validation tests
- artifact binding tests
- kernel trace conformance tests
- contract version mismatch tests

### CPU Oracle Tests

- operator closure tests for currently exposed field operations
- opaque conservative fallback tests
- truthful hit-context regression tests
- provenance-aware trace/surface/radiance/media tests
- repeat and instance identity tests
- metrics regression tests for pruning and branch-visit behavior

### Virtual GPU Tests

- CPU vs virtual GPU parity over the same contract
- artifact misuse failure tests
- local-context differential tests
- plan-stage validation tests
- backend-cost comparison counters for representative dispatches

### ABI Tests

- dispatch record layout tests
- hit/result record layout tests
- culling or accelerator table layout tests
- alignment/padding snapshot tests

### Product-Level Tests

- preview scene parity tests where applicable
- spec project integrity updates when surfaced semantics change
- determinism tests for batch dispatch and capture behavior
- preview/render contract parity tests proving presentation stays downstream of the same semantic engine

### Performance Validation

Phase 9.5 should introduce or preserve enough counters to answer the following questions on representative fixtures:

- Are support-driven candidate rejections increasing, decreasing, or regressing to zero?
- Are branch visits decreasing when bounded support and culling are available?
- How large are culling and dispatch artifacts for representative scenes?
- Is dispatch/setup work growing faster than useful query work?
- Do the new contracts preserve or improve throughput for the initial distance and batch slices?

These do not need final shipping thresholds in this RFC, but they must be tracked and reviewed before calling the phase complete.

## Exit Criteria

Phase 9.5 is complete only when all of the following are true.

1. Scene IR is the canonical semantic source after HIR lowering.
2. Shape provenance is preserved in the canonical middle-end.
3. Support algebra is concrete enough to back real derived-artifact contracts.
4. Query plans express explicit backend-neutral contracts rather than strategy hints alone.
5. Kernel lowering transports those contracts without semantic reconstruction leaks.
6. CPU truth preserves winner selection, local hit context, and participant selection faithfully.
7. Opaque leaves use a principled conservative fallback contract.
8. Virtual GPU executes the same contracts through a meaningfully independent backend path.
9. Dispatch, hit, result, and artifact schemas are explicit and ABI-tested before WGSL.
10. A narrow initial WGSL slice can target already-frozen contracts without inventing new semantics.
11. Render and preview remain consumers of the same semantic execution boundary rather than separate execution models.
12. Planning, pruning, and artifact machinery remain observable enough to prove the converged architecture is not merely cleaner, but still operationally effective.

## Phase 10 Gate

Broad Phase 10 work is blocked until Phase 9.5 exit criteria are met.

Only one limited exception is allowed:

A narrow experimental WGSL spike may proceed earlier if it is explicitly scoped to:

- distance queries
- typed batch records
- already-frozen dispatch/result layouts
- no new provenance or local-hit-context semantics

That spike must not be treated as proof that the whole backend is ready.

## Recommended First WGSL Slice After Phase 9.5

Once this RFC is complete, the recommended initial WGSL slice is:

1. field and shape distance batch dispatch
2. explicit dispatch/result record validation
3. CPU vs virtual GPU vs WGSL parity over those records
4. only then shape trace and hit assembly
5. only then surface
6. only then radiance and medium

This sequencing keeps the riskiest semantic payloads until after the shared contract boundary has already been proven on simpler work.

## Risks And Failure Modes

### Risk: Scene IR Becomes Too Abstract To Be Useful

Mitigation:

- keep operator payloads typed and explicit
- keep summaries cached
- favor simple, inspectable contracts over over-general frameworks

### Risk: The Team Tries To Patch CPU Truth Without Contract Convergence

Mitigation:

- require each CPU fix to tie back to a Scene IR, plan, or kernel contract
- avoid "just fix the direct runtime" patches that deepen architectural duplication

### Risk: Virtual GPU Refactor Slows Momentum

Mitigation:

- stage the work around one frozen contract slice first
- prefer proving distance and batch flows before trying to make every query family independent at once

### Risk: WGSL Pressure Pulls Work Forward Prematurely

Mitigation:

- enforce the Phase 10 gate in planning
- treat missing contract definitions as blockers, not as implementation TODOs to solve inside the WGSL backend

## Migration Rules

During Phase 9.5, internal compatibility is not a goal.

The following migration rules apply:

1. If a semantic decision currently lives in two places, choose one canonical owner and delete the duplicate path.
2. If a downstream layer must reconstruct meaning from upstream ad hoc state, move that meaning into the canonical contract instead.
3. If an artifact exists only as an enum or label, define its schema before letting WGSL depend on it.
4. If CPU and virtual GPU share high-level semantic decision logic, plan to separate them at the contract boundary.
5. If an exposed authored operator is not truthfully executable through the canonical path, either close the gap or explicitly narrow the supported scope before Phase 10.

## Practical Definition Of Success

At the end of Phase 9.5, the project should be in this state:

- HIR describes authored intent well.
- Scene IR owns the scene's semantic truth.
- query plans make execution intent explicit.
- kernel and artifact contracts are stable and testable.
- CPU is a trustworthy oracle over those contracts.
- virtual GPU is a trustworthy differential lane over those contracts.
- WGSL can begin as a backend implementation task rather than an architecture-discovery task.

That is the real goal of this convergence pass.
