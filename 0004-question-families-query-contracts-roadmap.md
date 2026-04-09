# RFC 0004: Question Families and Query Contracts Roadmap

Status: Proposed

Author: Codex

Created: 2026-04-09

Target: post-Phase-10 `wrela` language, compiler, query planning, CPU/vGPU/WGSL execution, and render/query authoring surface

## Summary

Phase 10 landed the right backend direction:

- Scene IR is real and semantically rich.
- Query plans already carry explicit candidate/result/hit/artifact contracts.
- Kernel plans and portable ABI layouts are testable.
- CPU, virtual GPU, and WGSL can already execute the current query set with parity tests.

The next architectural step is not “add more one-off query builtins.”

It is to turn the current query stack into a **canonical question-family system**.

The core idea of this RFC is:

- the world is authored once as a semantic substrate
- the compiler owns a fixed registry of disciplined question families
- each family exposes one or more questions
- every question is carried by an explicit query contract
- capture/world/batch calls are just different surfaces over the same family/question pair
- domains carry policy for families, not an ad hoc bag of query-specific knobs
- CPU, virtual GPU, and WGSL consume the same question contracts

This RFC is intentionally practical.

It does **not** attempt to solve the entire “universal query engine” in one jump. It takes the current shipped Phase 10 code and turns it into a stable platform that can grow into that idea without turning into a tangle of hardcoded enums and backend-specific branches.

## Relationship To Earlier RFCs

This RFC builds directly on:

- `0001-field-game-language.md`
- `0002-field-engine-implementation-roadmap.md`
- `0003-phase-9-5-semantic-convergence-plan.md`
- shipped Phase 10 WGSL work

RFC 0001 already says the deep thing:

- the world is queried by different systems with different correctness/cost needs
- domains must be first-class
- gameplay and presentation should not be separate copies of reality

RFC 0002 and RFC 0003 already established the execution rule:

- semantic authoring first
- compiler-owned query plans and contracts
- CPU oracle first
- GPU as backend, not as authored truth

This RFC is the next cut after Phase 10. It takes the current hardcoded query surface and refactors it into a durable family/contract model.

## Current Repo Read

The repo is already much closer to this than it looks from the language surface.

### What is already strong

1. `compiler/scene_ir/mod.rs` already carries semantic scene structure, support structure, provenance structure, identity sources, and analysis summaries.
2. `compiler/query_plan/mod.rs` already has explicit record contracts for dispatch, candidates, results, hit context, participants, and artifacts.
3. `compiler/kernel/ir.rs` and `compiler/kernel/validate.rs` already treat query plans as portable executable contracts.
4. `compiler/portable/abi.rs` already owns host/WGSL layout for contract-bearing records.
5. `compiler/query_exec/{cpu,vgpu,wgsl}.rs` already execute the current query set through the same broad contract architecture.
6. `language/spec/tests/spec/language_spec_test.wr` already has semantic `region`, `domain`, and `render` coverage.

### What is currently holding the design back

The current query surface is still encoded in too many places as hardcoded question enums and builtin function names.

That duplication currently lives in at least these places:

- builtin signatures in `compiler/hir/typeck/context.rs`
- parse/lower tables in `compiler/kernel/lower.rs`
- split spec structs in `compiler/query_exec/spec.rs`
- constructor logic in `compiler/query_plan/mod.rs`
- world query semantics table in `compiler/query_exec/world.rs`
- WGSL flavor tables in `compiler/query_exec/wgsl/codegen.rs`
- bridge export tables in `compiler/query_exec/native_bridge.rs`

This means every new query currently wants a cross-cutting edit through multiple match tables.

That is acceptable for Phase 8–10. It is the wrong shape for Phase 11+.

### Specific design smells to fix before adding lots of new questions

1. **The language surface is hardcoded by builtin name rather than by family/question identity.**
2. **`SceneDomain` is flat and query-shaped, not family-shaped.** It currently mixes detail policy with per-trace budget fields.
3. **Scalar and batch surfaces are not normalized enough.** Batch uses item records (`PointQuery`, `RayQuery`), while scalar trace/radiance world/capture calls still split their inputs across many loose parameters.
4. **WGSL still has per-question codegen flavors and bridge kinds.** This is workable now, but it will not scale once new families land.
5. **There is no canonical registry of supported questions.** The query system is compiler-owned in spirit, but not yet in one authoritative table.

## Why This Comes Before More Features

The temptation after Phase 10 is to keep adding more question builtins:

- `nearest`
- `occluded`
- `support`
- `overlap`
- `walkable`
- etc.

That would be the wrong order.

Without a canonical family/contract layer, each new question would add more branching to the exact code that just became converged in Phase 9.5 and Phase 10.

So the first move must be architectural:

- one registry
- one contract model
- one domain-contract model
- one family-oriented authoring direction

Only after that should the engine grow its query vocabulary.

## Goals

This roadmap has six goals.

1. **One source of truth for questions.**
   Every supported question must exist in one compiler-owned registry.

2. **Families, not unrelated builtins.**
   The engine should reason in terms of families such as `spatial`, `surface`, `participants`, and later `support`.

3. **Domains carry policy; items carry per-call data.**
   Domain contracts should say things like detail tier or feature enablement. Item records should carry per-query data like rays or point-direction samples.

4. **Capture/world/batch are surfaces over the same family/question pair.**
   `spatial.distance` on a shape capture and `spatial.distance` on a world capture are related questions, not different feature islands.

5. **Backends must consume contracts, not hardcoded names.**
   CPU, vGPU, and WGSL should dispatch off descriptor tables and record schemas, not per-question flavor enums.

6. **The language surface should eventually reflect the family model directly.**
   The final user-facing API should look like disciplined family calls, not a bag of historical builtin names.

## Explicit Non-Goals

This roadmap does **not** do the following.

- user-authored arbitrary query kernels
- runtime-loaded plugin query families
- stringly-typed dynamic query dispatch
- topology-aware navigation or impossible-geometry traversal semantics
- body/move/collision-domain overlap against future body DSLs
- affordance, salience, stealth, or other higher-level semantic families
- replacing semantic authoring with GPU-first execution code

Those can come later. This plan exists to make that future tractable.

## Design Rules

Every phase in this RFC must follow these rules.

1. No new question may be added by scattering new match branches across the compiler.
2. The query registry must describe **static** question semantics only. Scene-derived strategy stays in query planning.
3. Domain contracts must express family policy only. Per-call data must live in item records.
4. Family contracts must be typed values, not untyped metadata maps.
5. Render remains a consumer of query families. It does not become a competing execution model.
6. CPU oracle lands first for every new question family capability.
7. If a question is not supported on a backend, the descriptor must say so explicitly.
8. Every phase must ship with parser/HIR tests, planning/kernel tests, portable ABI tests, query execution parity tests where applicable, and spec or preview coverage where applicable.

## Key Architectural Definitions

### Family

A **family** is a compiler-owned namespace of related questions.

Examples for this roadmap:

- `spatial`
- `surface`
- `participants`
- `support`

### Question

A **question** is one operation inside a family.

Examples:

- `distance`
- `normal`
- `nearest`
- `occluded`
- `sample`
- `radiance`
- `medium`
- `summary`

### Surface

A **surface** is how a question is invoked.

For this roadmap the important surfaces are:

- scalar capture
- scalar world
- batch capture
- later batch world if needed

### Query Contract

A **query contract** is the compiler-owned descriptor for one `family.question.surface` variant.

It must say at minimum:

- stable id and version
- family id
- question id
- surface kind
- capture kind
- item schema
- result schema
- whether a domain contract is required
- whether hit context is preserved
- whether participant selection is required
- supported backends
- observability profile

It must **not** hardcode scene-derived pruning choice or artifact contents. Those belong to plan lowering.

It should also avoid carrying lowering-only details like helper symbol names or the current executor wiring. Those belong to a separate execution-binding layer.

### Execution Binding

An **execution binding** maps a semantic query contract onto the current lowering and runtime implementation.

It may include things like:

- planner recipe kind
- default executor
- optional internal kernel kind
- optional helper/export symbol

Execution bindings are compiler-internal adapters.

They are not the semantic source of truth and should not force contract version changes when the implementation strategy is reorganized.

### Domain Contract

A **domain contract** is the typed family policy carried by `SceneDomain` for world queries.

For this roadmap the important rule is:

- detail / capability / correctness policy belongs here
- ray marching numbers and point-direction samples do **not**

### Item Record

An **item record** is the per-call input carried into a question.

Examples:

- `PointQuery`
- `RayQuery`
- `PointDirectionQuery`
- `Hit3`
- later `UnitQuery` for item-less questions

## End State Of This Roadmap

At the end of this roadmap, the user-facing mental model should look like this.

```wr
world = capture world_region
presentation = Presentation(world = world)
collision = Collision(world = world)

hit = spatial.nearest(
    capture = world,
    domain = presentation,
    ray = ray_query(
        origin = vec3(0.0, 0.0, 3.0),
        direction = vec3(0.0, 0.0, -1.0),
        max_distance = 6.0,
        min_step = 0.05,
        hit_epsilon = 0.001,
        max_steps = 96,
    ),
    backend = dispatch_backend_wgsl(),
)

surface_value = surface.sample(
    capture = world,
    domain = presentation,
    hit = hit,
)

lighting = participants.radiance(
    capture = world,
    domain = presentation,
    sample = point_direction_query(
        point = hit.position,
        direction = normalize(vec3(0.0, 1.0, 1.0)),
    ),
)

summary = support.summary(
    capture = world,
    domain = collision,
)
```

The compiler-facing mental model should look like this.

```rust
pub struct QueryContractDescriptor {
    pub id: QueryContractId,
    pub version: u32,
    pub family: QueryFamilyId,
    pub question: QueryQuestionId,
    pub surface: QuerySurfaceKind,
    pub capture_kind: CaptureKind,
    pub item_kind: QueryItemKind,
    pub result_kind: QueryResultKind,
    pub domain_contract: Option<DomainContractKind>,
    pub preserves_local_hit_context: bool,
    pub participant_kind: Option<ParticipantContractKind>,
    pub supported_backends: BackendSupport,
    pub observability: QueryObservabilityProfile,
}

pub struct QueryExecutionBinding {
    pub contract_id: QueryContractId,
    pub planner_recipe: QueryPlannerRecipeKind,
    pub default_executor: PlanExecutor,
    pub default_kernel: Option<InternalKernelKind>,
    pub helper_name: Option<&'static str>,
}
```

The descriptor is the static source of truth for semantic identity and data requirements.

Execution wiring lives in a separate binding layer.

That binding layer is allowed to change when lowering/runtime internals improve. The semantic contract id and version should change only when the user-visible or backend-stable contract changes.

Query planning still derives scene-sensitive strategy from:

- Scene IR
- support summaries
- provenance
- capture kind
- domain contract values

## Phase Overview

This RFC defines six phases after shipped Phase 10.

- **Phase 11:** Canonical query contract registry
- **Phase 12:** Typed family contracts and query-item normalization
- **Phase 13:** Registry-driven lowering, planning, and backend dispatch
- **Phase 14:** Spatial family completion and canonical naming
- **Phase 15:** Support family
- **Phase 16:** Family-oriented language surface and tooling

## Phase 11: Canonical Query Contract Registry

### Goal

Create one authoritative compiler-owned registry for every currently shipped question without changing runtime behavior yet.

### Why this is first

This is the highest-leverage cut in the whole roadmap. Until this lands, every new question multiplies hardcoded tables across the compiler and backends.

### Workstream A: Contract Model

#### Task 11A1 — Add `compiler/query_contract/mod.rs`

**Description**

Create a new module that owns the static description of supported question contracts.

**Files**

- new `compiler/query_contract/mod.rs`
- optionally new `compiler/query_contract/builtins.rs`
- `compiler/lib.rs`

**Implementation notes**

Define the following types up front:

- `QueryContractId`
- `QueryFamilyId`
- `QueryQuestionId`
- `QuerySurfaceKind`
- `DomainContractKind`
- `BackendSupport`
- `QueryObservabilityProfile`
- `QueryContractDescriptor`
- `QueryPlannerRecipeKind`
- `QueryExecutionBinding`

Add `QueryItemKind::Unit` now even if the first users arrive later. This prevents the registry from assuming every question needs a point, ray, or hit.

Do **not** move scene-derived pruning into this module.

Keep `QueryContractDescriptor` semantic.

Do **not** put `helper_name`, `kernel`, or `executor` on the contract descriptor itself.
Those should live on `QueryExecutionBinding` or another lowering adapter owned by the same subsystem.

**Code sketch**

```rust
pub enum QueryFamilyId {
    Spatial,
    Surface,
    Participants,
    Support,
}

pub enum QuerySurfaceKind {
    CaptureScalar,
    WorldScalar,
    CaptureBatch,
}

pub struct QueryContractDescriptor {
    pub id: QueryContractId,
    pub version: u32,
    pub family: QueryFamilyId,
    pub question: QueryQuestionId,
    pub surface: QuerySurfaceKind,
    pub capture_kind: CaptureKind,
    pub item_kind: QueryItemKind,
    pub result_kind: QueryResultKind,
    pub domain_contract: Option<DomainContractKind>,
    pub preserves_local_hit_context: bool,
    pub participant_kind: Option<ParticipantContractKind>,
    pub supported_backends: BackendSupport,
    pub observability: QueryObservabilityProfile,
}

pub struct QueryExecutionBinding {
    pub contract_id: QueryContractId,
    pub planner_recipe: QueryPlannerRecipeKind,
    pub default_executor: PlanExecutor,
    pub default_kernel: Option<InternalKernelKind>,
    pub helper_name: Option<&'static str>,
}
```

**Acceptance criteria**

- New module exists and compiles conceptually.
- The registry supports deterministic iteration order.
- The model can describe item-less questions.
- No scene-derived pruning fields exist on the descriptor.
- Helper names and current executor/kernel routing are not part of semantic contract identity.

#### Task 11A2 — Seed the registry with every currently shipped question

**Description**

Add descriptors for the current query set.

Minimum seed list:

- `spatial.distance.capture.field`
- `spatial.distance.capture.shape`
- `spatial.distance.world`
- `spatial.distance.batch.field`
- `spatial.distance.batch.shape`
- `spatial.normal.capture.field`
- `spatial.normal.capture.shape`
- `spatial.normal.world`
- `spatial.normal.batch.field`
- `spatial.normal.batch.shape`
- `spatial.trace.capture.shape` (to be renamed later)
- `spatial.trace.world` (to be renamed later)
- `spatial.occluded.batch.shape`
- `surface.sample.capture.shape`
- `surface.sample.world`
- `participants.radiance.capture.shape`
- `participants.radiance.world`
- `participants.medium.capture.shape`
- `participants.medium.world`

**Files**

- `compiler/query_contract/mod.rs`
- `compiler/query_plan/mod.rs`

**Implementation notes**

Treat the current questions as the compatibility seed.

Do not try to rename `trace` to `nearest` in this phase. Keep the current semantics stable first. The canonical rename belongs later.

**Acceptance criteria**

- Every currently shipped query plan can be mapped to exactly one descriptor.
- Registry ids are stable strings, not ad hoc generated names.
- Query plan tests can assert descriptor ids.

#### Task 11A3 — Add execution bindings for every shipped descriptor

**Description**

Create a binding table that maps each semantic descriptor onto the current planner, lowering, and runtime implementation.

**Files**

- `compiler/query_contract/mod.rs`
- optionally new `compiler/query_contract/bindings.rs`
- `compiler/query_plan/mod.rs`

**Implementation notes**

This is where current details such as helper names, default executors, and default kernel kinds should live.

The binding layer may still be enum-backed at first, but it must be explicitly downstream of descriptor resolution rather than pretending to be the descriptor itself.

**Acceptance criteria**

- Every currently shipped descriptor has exactly one execution binding.
- Existing helper names are reachable by binding lookup rather than being embedded in semantic contract ids.
- Execution bindings can change without renaming semantic contracts.

### Workstream B: Planning Integration

#### Task 11B1 — Carry contract identity through query plans and kernel plans

**Description**

Add contract identity to `BatchQueryPlan`, `CaptureQueryPlan`, `WorldQueryPlan`, and their lowered kernel equivalents.

**Files**

- `compiler/query_plan/mod.rs`
- `compiler/kernel/ir.rs`
- `compiler/kernel/lower.rs`
- `compiler/kernel/validate.rs`

**Implementation notes**

Add at minimum:

- `contract_id`
- `family`
- `surface`

Keep the current enums for one phase as adapters only. The new fields should be authoritative for new work.

Query plans may continue to carry derived execution data such as executor or helper name for one phase, but those values should come from execution-binding lookup after descriptor resolution rather than from open-coded enum matches.

Validators should confirm that the descriptor’s static item/result kinds match the plan’s item/result contracts.

**Acceptance criteria**

- Every plan carries a stable contract id.
- Kernel validation checks descriptor/contract consistency.
- Existing plan tests continue to pass conceptually with new descriptor assertions.

#### Task 11B2 — Replace manual world semantics tables with descriptor lookups

**Description**

Refactor `compiler/query_exec/world.rs` so world query metadata comes from the new registry instead of a separate manual table.

**Files**

- `compiler/query_exec/world.rs`
- `compiler/query_contract/mod.rs`

**Implementation notes**

The world helper table should become a thin adapter or disappear entirely.

Questions that require a domain flag or participant family should read that from the descriptor.

**Acceptance criteria**

- `world_query_semantics` is removed or reduced to a descriptor wrapper.
- No duplicated world-query flag table remains.

### Workstream C: Test Harness

#### Task 11C1 — Add registry snapshot and exhaustiveness tests

**Description**

Add dedicated tests that lock the registry shape and ensure every legacy query enum maps into the semantic registry and an execution binding.

**Files**

- new `compiler/tests/query_contract_registry.rs`

**Implementation notes**

Test three things:

1. deterministic descriptor order
2. stable ids and versions
3. exhaustive mapping from current enums to descriptor ids
4. exhaustive mapping from descriptor ids to execution bindings

**Acceptance criteria**

- New test file exists.
- The test suite can detect descriptor or binding drift without running the full query stack.

### Phase 11 Exit Criteria

- There is one canonical query registry.
- Every current query plan carries a contract id.
- Every shipped contract resolves through one execution binding.
- No new question can be added without touching the registry first.
- No user-facing surface change yet.

## Phase 12: Typed Family Contracts And Query-Item Normalization

### Goal

Turn family policy into typed domain contracts and remove duplicated per-call query knobs from `SceneDomain`.

### Why this comes second

Right now `SceneDomain` is a flat bag that mixes policy and per-trace call data. That makes the family model muddy and keeps scalar/world/batch surfaces from converging.

### Design Rule For This Phase

**Items carry per-call data. Domains carry policy.**

That means:

- `RayQuery` carries `max_distance`, `min_step`, `hit_epsilon`, and `max_steps`
- `PointDirectionQuery` carries point + direction
- `SceneDomain` carries detail and enablement policy
- `SceneDomain` does **not** carry per-ray march settings anymore

### Workstream A: Contract Records

#### Task 12A1 — Add `PointDirectionQuery` as a first-class portable builtin record

**Description**

Promote the already-internal point-direction sample shape into a real portable builtin record.

**Files**

- `compiler/portable.rs`
- `compiler/portable/abi.rs`
- `compiler/hir/typeck/context.rs`
- `compiler/tests/portable_abi.rs`

**Implementation notes**

Use the same pattern as `PointQuery` and `RayQuery`.

Recommended shape:

```rust
PointDirectionQuery {
    point: Vec3,
    direction: Vec3,
}
```

This record will be used by `participants.radiance` in later phases.

**Acceptance criteria**

- `PointDirectionQuery` is a builtin portable record.
- ABI layout and WGSL emission are covered by tests.
- Typechecker can use it in function signatures.

#### Task 12A2 — Replace flat `SceneDomain` with typed family subcontracts

**Description**

Define new builtin records:

- `SpatialDomainContract`
- `SurfaceDomainContract`
- `ParticipantDomainContract`
- `SceneDomain`

Recommended shapes:

```rust
SpatialDomainContract {
    geometry_detail: I32,
    guarantee: U32, // exact/conservative/approximate, staged in with defaults
}

SurfaceDomainContract {
    material: Bool,
}

ParticipantDomainContract {
    radiance: Bool,
    media: Bool,
}

SceneDomain {
    scene_id: U32,
    spatial: SpatialDomainContract,
    surface: SurfaceDomainContract,
    participants: ParticipantDomainContract,
}
```

**Files**

- `compiler/portable.rs`
- `compiler/portable/abi.rs`
- `compiler/mir/lower.rs`
- `compiler/hir/def.rs`

**Implementation notes**

Remove these fields from `SceneDomain`:

- `max_distance`
- `min_step`
- `hit_epsilon`
- `max_steps`

Those belong in `RayQuery`, not in the domain.

Keep `geometry_detail` under `spatial`.

This is primarily an internal contract cleanup phase.
The internal `SceneDomain` shape should change here even if the user-facing authored syntax remains temporarily stable.

**Acceptance criteria**

- `SceneDomain` uses nested family contracts.
- Flat trace-budget fields no longer exist on `SceneDomain`.
- ABI tests cover nested struct layout.
- WGSL struct emission uses portable ABI generation only.

#### Task 12A3 — Retire public query-wrapper records that mix capture and item

**Description**

`TraceQuery` and `SurfaceQuery` are not the right long-term public shapes because they mix target, item, and policy.

Retire them from the public portable surface.

**Files**

- `compiler/portable.rs`
- any dependent tests such as `compiler/tests/pir.rs`

**Implementation notes**

The public surface should carry:

- `capture` separately
- `domain` separately when needed
- `ray` / `point` / `point_direction` / `hit` as item records

If any internal adapter still needs the old wrapper records for one phase, keep them private to lowering instead of public builtins.

**Acceptance criteria**

- `TraceQuery` and `SurfaceQuery` are no longer part of the public builtin query vocabulary.
- No user-facing signature requires a mixed capture+item wrapper record.

### Workstream B: Domain Lowering And Compatibility

#### Task 12B1 — Keep current `domain` authoring stable while lowering to family-shaped contracts

**Description**

Preserve the current authored `domain` surface for this phase, but lower it into the new nested family-shaped `SceneDomain` representation internally.

**Files**

- `compiler/hir/lower.rs`
- `compiler/hir/def.rs`
- `compiler/hir/typeck/types.rs`
- `compiler/hir/typeck/context.rs`
- `language/spec/tests/spec/language_spec_test.wr`

**Implementation notes**

This phase should avoid forcing a user-facing source migration just to land an internal contract cleanup.

Treat the current authored domain fields as sugar over nested family contracts.

If explicit constructors like `spatial_domain_contract(...)` are useful internally or for tests, they may exist as non-primary surface area during this phase.
The authoritative public syntax cut belongs later, together with the family query namespace cut, so users migrate once instead of twice.

**Acceptance criteria**

- Existing domain declarations still compile from the user’s point of view.
- Lowering and typechecking target the nested family contract shape internally.
- No mandatory authored-domain migration is required in Phase 12.

#### Task 12B2 — Update MIR lowering to build nested `SceneDomain`

**Description**

Refactor domain lowering and render default-domain construction to assemble nested family contracts.

**Files**

- `compiler/mir/lower.rs`

**Implementation notes**

Update both:

- domain declaration lowering
- any helper that synthesizes a default `SceneDomain` for render or test paths

Use one helper for contract construction instead of inlining `SceneDomain` field writes in multiple places.

**Acceptance criteria**

- There is one shared helper to build default `SceneDomain` values.
- MIR lowering no longer writes flat `SceneDomain` fields.

### Workstream C: Scalar Query Signature Cleanup

#### Task 12C1 — Change scalar trace calls to take `ray: RayQuery`

**Description**

Replace loose scalar trace parameters with a single typed `RayQuery` item.

**Old shape**

```wr
trace_world(
    capture = world,
    domain = collision,
    origin = vec3(...),
    direction = vec3(...),
    max_distance = 6.0,
    min_step = 0.05,
    hit_epsilon = 0.001,
    max_steps = 96,
)
```

**New shape**

```wr
spatial.trace(
    capture = world,
    domain = collision,
    ray = ray_query(
        origin = vec3(...),
        direction = vec3(...),
        max_distance = 6.0,
        min_step = 0.05,
        hit_epsilon = 0.001,
        max_steps = 96,
    ),
)
```

If the family surface has not landed yet, use the same item change on the legacy builtin first.

**Files**

- `compiler/hir/typeck/context.rs`
- `compiler/kernel/lower.rs`
- `compiler/query_exec/spec.rs`
- `compiler/query_exec/{cpu,vgpu,wgsl}.rs`
- `language/spec/tests/spec/language_spec_test.wr`

**Implementation notes**

This is a major simplification.

It unifies scalar trace with batch trace and removes duplicate query-budget fields from domains.

If the family namespace surface has not landed yet, do this first on legacy builtin names and keep the authored migration small.

**Acceptance criteria**

- Scalar trace signatures no longer list loose origin/direction/budget arguments.
- All scalar trace execution paths consume a `RayQuery` item internally.
- Batch and scalar trace share the same item record shape.

#### Task 12C2 — Change radiance calls to take `sample: PointDirectionQuery`

**Description**

Replace loose `(point, direction)` radiance inputs with a typed point-direction sample record.

**Files**

- same family of files as Task 12C1

**Acceptance criteria**

- Radiance signatures no longer duplicate point and direction fields across specialized code paths.
- Participants family can rely on one stable item schema.

### Workstream D: Domain Execution Updates

#### Task 12D1 — Update world-domain validation and flag lookup to nested contracts

**Description**

Refactor domain validation helpers to read:

- `domain.spatial.geometry_detail`
- `domain.surface.material`
- `domain.participants.radiance`
- `domain.participants.media`

**Files**

- `compiler/query_exec/cpu.rs`
- `compiler/query_exec/vgpu.rs`
- `compiler/query_exec/wgsl.rs`

**Acceptance criteria**

- No code path reads old flat domain fields.
- World validation works against nested family contracts.

### Phase 12 Exit Criteria

- `SceneDomain` is family-shaped, not query-shaped.
- Per-ray budgets live in `RayQuery`, not in `SceneDomain`.
- `PointDirectionQuery` is real.
- Scalar trace/radiance signatures are normalized around item records.
- The internal domain contract cleanup is landed without requiring a separate user-facing domain syntax migration yet.

## Phase 13: Registry-Driven Lowering, Planning, And Backend Dispatch

### Goal

Make the query registry truly authoritative by removing per-question hardcoded lowering and execution decisions from the authoritative path.

### Why this is third

After Phase 12, the query inputs and domain contracts are finally shaped correctly. That is the right moment to replace the current hardcoded lowering and backend flavor tables.

This phase should make generic descriptor-driven dispatch real internally.
It does not need to delete every compatibility wrapper at the boundary on day one if that would slow down convergence.

### Workstream A: Unified Invocation Specs

#### Task 13A1 — Replace split spec structs with canonical invocation specs

**Description**

Replace the split structs in `compiler/query_exec/spec.rs` with a smaller set of descriptor-driven invocation specs.

Recommended target:

```rust
pub struct ScalarQueryInvocationSpec {
    pub contract_id: QueryContractId,
    pub capture: hir::Idx<Expr>,
    pub domain: Option<hir::Idx<Expr>>,
    pub item: hir::Idx<Expr>,
    pub backend: Option<hir::Idx<Expr>>,
}

pub struct BatchQueryInvocationSpec {
    pub contract_id: QueryContractId,
    pub capture: hir::Idx<Expr>,
    pub domain: Option<hir::Idx<Expr>>,
    pub items: hir::Idx<Expr>,
    pub backend: hir::Idx<Expr>,
}
```

**Files**

- `compiler/query_exec/spec.rs`
- `compiler/kernel/lower.rs`

**Implementation notes**

Do not keep one spec struct per question family/question shape. That would just recreate the current problem with new names.

**Acceptance criteria**

- `query_exec/spec.rs` no longer has separate structs for field/shape/world point/world shape/batch variants.
- All query invocations carry a contract id.

#### Task 13A2 — Lower query calls by descriptor lookup, not by hardcoded builtins alone

**Description**

Refactor `compiler/kernel/lower.rs` so query lowering is driven by descriptor lookup.

**Files**

- `compiler/kernel/lower.rs`
- `compiler/query_contract/mod.rs`

**Implementation notes**

It is acceptable to keep a short compatibility table from legacy builtin names to descriptor ids during this phase.

Execution-binding lookup should happen immediately after descriptor resolution.

It is **not** acceptable to keep large per-question lowering branches once the descriptor and its binding have been resolved.

**Acceptance criteria**

- Contract lookup happens before plan construction.
- Binding lookup happens immediately after descriptor resolution.
- The bulk of query lowering is shared after descriptor/binding resolution.

### Workstream B: Registry-Driven Query Plan Builders

#### Task 13B1 — Refactor plan constructors to consume descriptors

**Description**

Replace direct enum-oriented constructors like `for_query(kind, ...)` with descriptor-driven builders.

**Files**

- `compiler/query_plan/mod.rs`

**Implementation notes**

Keep scene-sensitive planning exactly where it belongs:

- candidate strategy
- pruning strategy
- artifact derivation

The descriptor supplies static question semantics. The execution binding supplies planner recipe and default execution wiring. The scene summary supplies strategy.

**Acceptance criteria**

- New plan builders take a `QueryContractDescriptor` plus execution binding, or take `QueryContractId` and resolve both.
- Legacy enum constructors, if still present, become thin adapters only.

#### Task 13B2 — Derive item/result/domain requirements from descriptors everywhere

**Description**

Remove duplicated item/result/domain branching from:

- query plan construction
- kernel validation
- dispatch contract assembly

**Files**

- `compiler/query_plan/mod.rs`
- `compiler/kernel/validate.rs`
- `compiler/portable/abi.rs`

**Acceptance criteria**

- Static item/result expectations come from descriptors.
- Planner recipe and default execution wiring come from execution bindings.
- Validation errors mention contract ids and versions.

### Workstream C: Backend Dispatch And Codegen

#### Task 13C1 — Replace WGSL `QueryFlavor` with descriptor-driven emission

**Description**

Refactor `compiler/query_exec/wgsl/codegen.rs` so shader generation is keyed by query descriptor instead of a manually maintained `QueryFlavor` enum.

**Files**

- `compiler/query_exec/wgsl/codegen.rs`
- `compiler/query_exec/wgsl.rs`

**Implementation notes**

This can still use internal helper categories where useful, but they must be derived from descriptors instead of acting as a separate source of truth.

**Acceptance criteria**

- `QueryFlavor` is deleted or reduced to an internal generated category with no manual public mapping table.
- Shader emission chooses item/result ABI from descriptor data.

#### Task 13C2 — Begin generic contract-driven bridge convergence as a non-blocking follow-on

**Description**

Move `compiler/query_exec/native_bridge.rs` toward a generic contract-driven bridge path once descriptor-driven lowering, planning, and WGSL codegen are already stable.

Preferred target:

- one generic internal scalar bridge
- one generic internal batch bridge
- descriptor ordinal or contract id carried in the dispatch header

**Files**

- `compiler/query_exec/native_bridge.rs`
- any runtime-facing ABI wrappers it touches

**Implementation notes**

This becomes feasible only after item normalization.

This task is explicitly non-blocking for Phase 13 exit.
If it threatens the core descriptor-driven convergence work, defer the full bridge collapse until immediately after Phase 13.

The generic bridge should pack:

- dispatch header
- contract ordinal/version
- item buffer
- optional world-shape index buffer

Do not invent a second ad hoc shader ABI here. Reuse the portable ABI and the descriptor registry.

If existing exported entry points remain temporarily for ABI stability or landing simplicity, they should become thin wrappers over the generic internal path rather than remaining separate implementations.

Deleting every exported wrapper is useful cleanup, but it is not the critical-path proof that the architecture has converged.

**Acceptance criteria**

- There is one descriptor-driven packing model for a future generic scalar and batch bridge path.
- If a generic internal bridge lands in this phase, any remaining per-question exports are thin wrappers over it rather than separate implementations.
- Deferring exported-wrapper collapse does not block Phase 13 completion.

#### Task 13C3 — Dispatch CPU/vGPU/WGSL from the same descriptor path

**Description**

Make backend routing contract-driven for the full current query set.

**Files**

- `compiler/query_exec/mod.rs`
- `compiler/query_exec/{cpu,vgpu,wgsl}.rs`

**Acceptance criteria**

- Direct execution traces report family/question contract ids.
- Unsupported-backend errors are descriptor-driven.

### Workstream D: Migration Tests

#### Task 13D1 — Add descriptor-driven parity tests for the full current query set

**Description**

Add or refactor parity tests so they assert over contract ids rather than only over legacy query enums.

**Files**

- `compiler/tests/query_exec.rs`
- new `compiler/tests/question_family_query_exec.rs` if desired

**Acceptance criteria**

- CPU/vGPU/WGSL parity still covers the full shipped query set after the migration.
- Test traces mention the descriptor id used.

### Phase 13 Exit Criteria

- The registry is truly authoritative.
- Query lowering is descriptor-driven.
- WGSL codegen is descriptor-driven.
- No new question needs a new hardcoded flavor enum or bespoke lowering branch in order to work.
- Native bridge convergence may still be pending as immediate follow-on cleanup and does not block Phase 13 exit.

## Phase 14: Spatial Family Completion And Canonical Naming

### Goal

Turn the current geometry query set into a coherent `spatial` family and finish the missing core spatial question surface.

### Why this comes before new non-render families

`spatial` is the foundation family. It already exists in pieces. We should make it internally coherent before we branch outward.

### Workstream A: Canonical Spatial Questions

#### Task 14A1 — Define canonical spatial question ids

**Description**

Add canonical question ids under the `spatial` family:

- `distance`
- `normal`
- `nearest`
- `occluded`

**Implementation notes**

`nearest` is the better semantic name for what the engine currently calls `trace`.

Internally, migrate descriptors first:

- current `trace` descriptors become compatibility aliases pointing at `nearest`
- current batch occlusion becomes the first `occluded` contract

**Acceptance criteria**

- `nearest` exists as the canonical internal question id.
- `trace` is an alias, not the primary canonical name.

#### Task 14A2 — Add scalar occlusion for shape and world surfaces

**Description**

The engine already has shape-batch occlusion. Add the scalar surfaces:

- shape capture occlusion
- world occlusion

Re-use `RayQuery` and `OcclusionResult`.

**Files**

- `compiler/query_contract/mod.rs`
- `compiler/query_plan/mod.rs`
- `compiler/query_exec/{cpu,vgpu,wgsl}.rs`
- `compiler/tests/query_exec.rs`

**Acceptance criteria**

- Scalar occlusion exists on capture and world surfaces.
- CPU/vGPU/WGSL parity covers the new scalar occlusion surfaces.

### Workstream B: Planning And Execution Cleanup

#### Task 14B1 — Unify nearest and occlusion on one ray-execution core

**Description**

Use one ray-execution core for both `nearest` and `occluded`.

`nearest` returns `Hit3`.
`occluded` returns `OcclusionResult`.

Planning differences should be descriptor-driven and result-oriented, not separate bespoke march implementations.

**Acceptance criteria**

- The hot execution path is shared.
- Only result assembly and contract details differ.

#### Task 14B2 — Add spatial-family observability conventions

**Description**

Define the standard observability profile for spatial questions:

- candidate count
- branch visits
- support prune effectiveness
- culling hit rate where applicable
- trace steps for ray questions
- field samples where relevant

**Files**

- `compiler/query_contract/mod.rs`
- `compiler/query_exec/mod.rs`

**Acceptance criteria**

- Spatial descriptors carry one observability profile.
- Traces report the same categories consistently across backends.

### Phase 14 Exit Criteria

- `spatial` is a coherent canonical family.
- `nearest` is the canonical internal name.
- Scalar and batch occlusion are both real questions.

## Phase 15: Support Family

### Goal

Expose compiler-visible support information as a first-class non-render question family.

### Why support is the first non-render family

The repo already has the right raw material:

- `SupportExpr`
- support node records
- support class summaries
- support-derived artifact contracts

That makes `support` the safest family for proving the “world as queryable substrate” idea beyond surface shading.

### Family Scope For This RFC

This RFC lands one support question first:

- `support.summary`

That is deliberate. It is better to land one strong support question than three shallow ones.

### Recommended User-Facing Result Shape

```rust
SupportSummaryResult {
    support_class: U32,
    semantics: U32,
    has_bounds: Bool,
    opaque_boundary: Bool,
    can_coarse_support_prune: Bool,
    min: Vec3,
    max: Vec3,
}
```

### Workstream A: Contracts And Result Types

#### Task 15A1 — Add the `support.summary` contract descriptors

**Description**

Add support-family descriptors for:

- capture scalar summary
- world scalar summary

Use `QueryItemKind::Unit` / `UnitQuery` for the no-item case.

**Files**

- `compiler/query_contract/mod.rs`
- `compiler/query_plan/mod.rs`
- `compiler/portable.rs`
- `compiler/portable/abi.rs`

**Implementation notes**

Add a trivial `UnitQuery` portable record if needed for generic dispatch machinery.

Do not special-case “no item” by inventing a separate dispatch architecture.

**Acceptance criteria**

- Support-summary descriptors exist.
- A unit-item path exists in the generic contract machinery.
- `SupportSummaryResult` is a portable ABI-bearing record.

### Workstream B: Planning And Execution

#### Task 15B1 — Answer support summaries from semantic support data, not by marching

**Description**

Implement support-summary execution by reading:

- `SceneSummary`
- support records
- support expressions
- derived artifact contracts

Do not approximate this by live field evaluation.

**Files**

- `compiler/query_plan/mod.rs`
- `compiler/query_exec/{cpu,vgpu}.rs`
- optionally `compiler/query_exec/wgsl.rs` if a host-backed WGSL path is justified

**Implementation notes**

For world support summary, merge region-visible shapes according to the active `spatial.geometry_detail` policy.

For opaque boundaries:

- set `opaque_boundary = true`
- set `has_bounds = false` unless bounds are still provable

**Acceptance criteria**

- Capture support summary is exact with respect to Scene IR support data.
- World support summary respects domain detail tier.
- Opaque support behaves conservatively.

#### Task 15B2 — Declare backend support explicitly in descriptors

**Description**

If WGSL is not worth supporting in the first cut of `support.summary`, say so explicitly in the descriptor.

**Acceptance criteria**

- Unsupported backend requests fail with a contract-driven error.
- No silent backend fallback occurs unless the descriptor explicitly allows it.

### Workstream C: Tests

#### Task 15C1 — Add direct support-family contract and execution tests

**Files**

- new `compiler/tests/support_family.rs`
- `compiler/tests/portable_abi.rs`

**Acceptance criteria**

- Capture support summary tests exist.
- World support summary tests exist.
- ABI tests cover `SupportSummaryResult`.

### Phase 15 Exit Criteria

- `support` exists as a real family.
- The engine can answer at least one non-render, non-hit question directly from semantic support data.

## Phase 16: Family-Oriented Language Surface And Tooling

### Goal

Expose the family model directly in the authored language and make it visible in tooling.

### Why this is last

The family surface should be the last cut, not the first. By the time it lands, the internal registry and backends should already be stable.

This is also the right moment to make any final user-facing domain authoring cleanup.
If users need to migrate examples or mental models, they should do it once here rather than once in Phase 12 and again in Phase 16.

### Workstream A: Family Namespace Surface

#### Task 16A0 — Make the user-facing query and domain surface cut once

**Description**

Bundle the final query-family syntax cut and any authored-domain cleanup into one user-facing migration.

If explicit family-shaped domain authoring is still desirable, land it in the same phase as `spatial.*`, `surface.*`, `participants.*`, and `support.*` calls.

**Implementation notes**

Do not require users to first migrate domain declarations and then later migrate query calls.

Either:

- keep the current domain authoring syntax as enduring sugar over family-shaped contracts, or
- make the explicit family-shaped domain syntax authoritative in the same release where family query namespaces become authoritative

**Acceptance criteria**

- The user-facing migration happens once in this phase.
- Docs, examples, and spec coverage can move together without a second separate domain-only rewrite.

#### Task 16A1 — Add intrinsic family namespaces

**Description**

Teach the language to treat bare identifiers like these as intrinsic query-family namespaces:

- `spatial`
- `surface`
- `participants`
- `support`

Then resolve member calls such as:

- `spatial.distance(...)`
- `spatial.nearest(...)`
- `surface.sample(...)`
- `participants.radiance(...)`
- `support.summary(...)`

**Files**

- `compiler/hir/typeck/context.rs`
- `compiler/hir/typeck/expr.rs`
- `compiler/kernel/lower.rs`
- possibly small parser changes if needed, though member-call syntax already exists

**Implementation notes**

Prefer intrinsic namespace values plus member-call resolution over inventing new grammar.

That keeps the syntax small and lets the existing member-expression pipeline do most of the work.

**Acceptance criteria**

- Family member calls resolve against the contract registry.
- Overload resolution handles capture type and scalar vs batch surface.

#### Task 16A2 — Add batch family calls under the same namespace

**Description**

Expose batch questions as family members too.

Recommended names:

- `spatial.distance_batch(...)`
- `spatial.normal_batch(...)`
- `spatial.nearest_batch(...)`
- `spatial.occluded_batch(...)`

**Acceptance criteria**

- Batch family calls compile and lower through the same descriptor registry.

### Workstream B: Authoritative Surface Cut

#### Task 16B1 — Remove direct user-facing builtin query names from the primary docs and spec surface

**Description**

Once family namespaces work, move the spec and examples to the family-oriented surface.

Legacy names such as `distance_world`, `trace_world`, `surface_world`, and friends should be treated as compatibility aliases only if they are still temporarily retained.

If explicit family-shaped domain authoring also lands, update domain examples in the same pass.

**Files**

- `language/spec/tests/spec/language_spec_test.wr`
- preview project sources
- any user-facing docs that mention the old surface

**Acceptance criteria**

- New authored examples use family calls.
- Domain examples, if changed, are updated in the same pass.
- Direct builtin names are removed from the primary examples.

#### Task 16B2 — Collapse builtin signature tables around the family registry

**Description**

Refactor builtin signature registration so family namespaces and descriptors are authoritative, not a flat list of unrelated query function signatures.

**Files**

- `compiler/hir/typeck/context.rs`

**Acceptance criteria**

- Flat user-facing query builtin registration is gone or reduced to explicit compatibility aliases.
- The family registry drives user-visible query signatures.

### Workstream C: Tooling And Reports

#### Task 16C1 — Add CLI/report support for family/question contracts

**Description**

Extend tooling so it can print the supported families, questions, surfaces, and contract versions.

This should also appear in any contract-report or cert-report surfaces that already exist.

**Files**

- CLI/reporting paths under `compiler/bin` and related test surfaces

**Acceptance criteria**

- Tooling can list query contracts.
- Trace or cert output includes contract id/version where relevant.

#### Task 16C2 — Add a “new question checklist” to the repo docs

**Description**

Document the required steps for adding a new family question.

The checklist should include:

1. add descriptor
2. add execution binding
3. add item/result record shapes
4. add domain contract fields if needed
5. add plan builder path
6. add CPU oracle
7. add backend support or mark unsupported
8. add observability
9. add tests

**Acceptance criteria**

- A junior engineer can follow the checklist without reverse-engineering the architecture.

### Phase 16 Exit Criteria

- The language surface exposes family namespaces.
- The old pile of builtin query names is no longer the primary user model.
- Tooling shows family/question contract identity.

## What To Tackle First

Start with **Phase 11, Workstream A**.

More concretely, the first three tasks to execute should be:

1. **11A1 — Add the registry module.**
2. **11A2 — Seed it with the current shipped query set.**
3. **11B1 — Carry contract ids into query plans and kernel plans.**

Do **not** start by adding new user-facing queries.

Do **not** start by adding new parser syntax.

Do **not** start by adding a navigation family.

The engine needs one canonical question registry before it needs more questions.

## Per-Question Implementation Checklist

From Phase 11 onward, every new question should follow this checklist.

### Static Contract Work

- add family/question/surface descriptor
- assign stable id and version
- assign item kind and result kind
- assign domain contract dependency
- assign backend support policy
- assign observability profile
- add execution binding entry

### Schema Work

- add or confirm item record ABI
- add or confirm result record ABI
- add or confirm artifact contract ABI if needed
- add WGSL struct emission snapshots

### Planning Work

- add descriptor-driven plan builder path
- define candidate/winner/participant semantics
- define artifact expectations
- define unsupported-backend behavior

### Execution Work

- add CPU oracle
- add vGPU support or explicitly reject it
- add WGSL support or explicitly reject it
- add parity tolerances if floating-point-sensitive

### Test Work

- typechecker coverage
- kernel validation coverage
- portable ABI coverage
- direct query execution coverage
- batch coverage if applicable
- language spec coverage if user-facing

No question is complete until that checklist is complete.

## Recommended Follow-Up RFCs After This Roadmap

Once this roadmap is done, the next clean follow-up RFCs are:

1. **Collision/body family RFC**
   - overlap against future `body`/collision exports
   - swept tests
   - contact generation

2. **Navigation family RFC**
   - walkability
   - clearance
   - local-up conventions
   - adjacency/topology semantics

3. **Higher semantic families RFC**
   - affordance
   - audibility
   - salience
   - danger

Those should build on the family/contract architecture from this RFC rather than bypass it.

## Final Recommendation

The correct next move after Phase 10 is **not** “add more world-query builtins.”

It is:

- make questions canonical
- make domains family-shaped
- normalize items and results
- let the registry drive planning and backend execution
- then grow the question vocabulary from that foundation

If we do that, `wrela` stops looking like “a renderer with extra queries” and starts becoming what it is aiming to be:

a semantic world substrate that different systems can interrogate through disciplined compiler-owned question families.
