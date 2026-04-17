# Context Map

Phase 50 makes the repo's go-forward ownership rules explicit before the larger
module and workflow refactors land.

Use this map operationally:

- Later phases should map touched files to one or more named contexts from this
  document instead of inventing new boundaries on the fly.
- Start inside the named context's entrypoints before you cross seams.
- Prove a change with the context's listed tests, benchmarks, or repo lanes.
- If a change needs a new cross-context import, prefer a small named seam over a
  convenience import.

## Primary Contexts

### Authoring frontend

- Owns source text, lexing, parsing, syntax trees, and author-facing
  diagnostics: `compiler/lexer`, `compiler/parser`, `compiler/diag`, and the
  author-facing project-loading slice of `compiler/hir`.
- Start here from `compiler/parser/mod.rs`, `cargo run -p wrela -- check ...`,
  `cargo run -p wrela -- build ...`, and authored projects under
  `language/spec` and `language/view_basic`.
- Primary public nouns: `SyntaxNode`, `ParseError`, `Module`, `Project`.
- Prove it with `compiler/lexer/tests.rs`,
  `compiler/tests/spec_project_integrity.rs`,
  `compiler/tests/project_e2e.rs`,
  `compiler/tests/thin_core_snapshot.rs`, and
  `cargo run -p wrela -- test language/spec --lane=fast`.

### Semantic compilation pipeline

- Owns semantic normalization and lowering from HIR/PIR/MIR into executable
  contracts and plans: `compiler/hir`, `compiler/pir`, `compiler/mir`,
  `compiler/kernel`, `compiler/scene_ir`, and the shared
  transition/evidence vocabulary in `compiler/state_advance` and
  `compiler/semantic_evidence`.
- Start here from the module roots above, plus compile-facing reports such as
  `cargo run -p wrela -- query-contracts` and
  `cargo run -p wrela -- frame-contracts`.
- Primary public nouns: `Type`, `TypeInfo`, `QueryPlan`, `PresentationPlan`,
  `CollisionPlan`, `SemanticEvidenceSummary`.
- Phase 53 seam roots:
  `compiler/mir/lower/mod.rs` exposes `lower_module`,
  `lower_module_with_types`, and `lower_module_with_types_and_backend`.
  Statement, expression, kernel, and interface lowering stay private to that
  tree.
- Prove it with `compiler/tests/codegen_v2.rs`, `compiler/tests/pir.rs`,
  `compiler/tests/kernel.rs`, `compiler/tests/semantic_evidence.rs`,
  `compiler/tests/presentation_plan.rs`,
  `compiler/tests/collision_plan.rs`, and
  `cargo run -p wrela -- check language/spec`.

### Query execution

- Owns query family contracts, planning, backend selection, CPU/WGSL execution,
  and observability: `compiler/query_contract`, `compiler/query_plan`,
  `compiler/query_program_spine`, `compiler/query_solver`, and
  `compiler/query_exec`.
- Start here from `compiler/query_exec/mod.rs`, `compiler/query_program_spine`,
  and `QueryExecContext`,
  `cargo run -p wrela -- query-contracts`, and the query-backed evaluation used
  by `preview`, `frame`, and `presentation-debug`.
- Primary public nouns: `QueryContractId`, `DispatchBackend`,
  `QueryProgramSpine`, `QueryExecContext`, `BatchQueryExecutionTrace`,
  `SemanticCostReport`.
- Phase 53 seam roots:
  `compiler/query_exec/cpu/mod.rs` owns the CPU entrypoints and
  `QueryExecError`; `compiler/query_exec/wgsl/codegen/mod.rs` owns shader
  generation plus the ABI helpers required by the runtime; and
  `compiler/query_exec/mir/mod.rs` is the explicit bridge back into MIR
  lowering, exporting only named query and capture helper lowerers.
- Prove it with `compiler/tests/query_contract_registry.rs`,
  `compiler/tests/query_program_spine.rs`, `compiler/tests/query_exec.rs`,
  `compiler/tests/ray_solver_plan.rs`,
  `compiler/tests/ray_solver_evidence.rs`, `just test-query`,
  `benchmarks/micro/bench.toml`, and `benchmarks/field_engine/bench.toml`.

### Presentation execution

- Owns authored view contracts, presentation plans, attachment binding,
  framegraph execution, and presentation observability:
  `compiler/presentation_binding`, `compiler/presentation_contract`,
  `compiler/presentation_plan`, and `compiler/presentation_exec`.
- Start here from `cargo run -p wrela -- frame-contracts ...`,
  `cargo run -p wrela -- preview ...`,
  `cargo run -p wrela -- frame ...`, and
  `cargo run -p wrela -- presentation-debug ...`.
- Primary public nouns: `FrameContract`, `PresentationPlan`,
  `PresentationExecutionInput`, `PresentationFramegraph`,
  `PresentationQualityReport`.
- Phase 53 seam roots:
  `compiler/presentation_exec/wgsl/mod.rs` is the WGSL backend entrypoint seen
  by the surrounding presentation executor (`execute_plan`). Pass execution,
  pipeline construction, staging uploads, and shader source helpers stay
  private to that backend tree.
- Prove it with `compiler/tests/presentation_plan.rs`,
  `compiler/tests/presentation_exec.rs`,
  `compiler/tests/preview_project.rs`,
  `benchmarks/realtime_presentation/bench.toml`,
  `benchmarks/whole_frame/bench.toml`, and `just perf-closure`.

### Collision execution

- Owns collision family contracts, plans, execution, and collision-facing reuse
  and acceleration behavior: `compiler/collision_contract`,
  `compiler/collision_plan`, `compiler/collision_exec`, and the collision-facing
  slices of `compiler/acceleration` and `compiler/artifact_store`.
- Start here from `cargo run -p wrela -- collision-contracts`,
  `cargo run -p wrela -- collision-plan`,
  `cargo run -p wrela -- collision-run`, and
  `benchmarks/collision_perf/`.
- Primary public nouns: `CollisionContractId`, `CollisionPlan`,
  `CollisionExecutionPolicy`, `CollisionWitnessSchema`.
- Prove it with `compiler/tests/collision_plan.rs`,
  `compiler/tests/collision_exec.rs`,
  `compiler/tests/collision_artifacts.rs`,
  `compiler/tests/acceleration.rs`,
  `compiler/tests/acceleration_forest.rs`,
  `benchmarks/collision_perf/bench.toml`, and `just test-all`.

### Runtime and artifact substrate

- Owns runtime ABI/value machinery plus the shared identity, time, artifact, and
  GPU substrate used by multiple execution contexts: `runtime/src`,
  `compiler/artifact_contract`, `compiler/artifact_key`,
  `compiler/artifact_layout`, `compiler/artifact_store`,
  `compiler/world_identity`, `compiler/time_semantics`,
  `compiler/gpu_runtime`, and the shared acceleration substrate.
- Start here from `runtime/src/lib.rs`, the module roots above, and the reuse or
  report types consumed by query, presentation, and collision execution.
- Primary public nouns: `Value`, `TypeId`, `ArtifactStore`, `ArtifactReuseKey`,
  `WorldSnapshotHandle`, `SnapshotIdentityReport`, `GpuRuntimeMetrics`.
- Prove it with `compiler/tests/artifact_store.rs`,
  `compiler/tests/snapshot_identity.rs`,
  `compiler/tests/temporal_semantics.rs`,
  `compiler/tests/portable_abi.rs`,
  `compiler/tests/thin_core_snapshot.rs`, runtime unit tests, and the perf
  playbooks under `docs/perf/`.

### Tooling and orchestration

- Owns repo workflows, CLI command entrypoints, benchmark orchestration, perf
  harnesses, and contributor-facing guidance: `compiler/bin/wrela/*`,
  `justfile`, `scripts/devloop_measure.py`, `benchmarks/*`, and `docs/dev/*`.
- Start here from `compiler/bin/wrela.rs`, `just --list`,
  `cargo run -p wrela -- --help`, `docs/dev/lanes.md`, and
  `docs/dev/devloop_playbook.md`.
- Primary public nouns: repo lanes such as `test`, `test-all`, `perf-smoke`,
  and `ship`, plus `wrela` subcommands and JSON report surfaces.
- Phase 53 seam roots:
  `compiler/bin/wrela/commands/mod.rs` is the CLI dispatch surface, and
  `compiler/bin/wrela/perf_engine/mod.rs` is the perf/perfcmp/matrix surface.
  Callers should go through `execute`, `execute_perf_command`,
  `execute_perfcmp_command`, and `execute_matrix_command` instead of reaching
  into collection, closure, or worktree helpers.
- Prove it with `compiler/tests/cli.rs`,
  `compiler/tests/one_shot_metrics_harness.rs`,
  `just baseline-devloop`, and benchmark manifests under `benchmarks/*`.

## Allowed Dependency Directions

- The authoring frontend may depend on shared compiler vocabulary, but it may
  not depend on query, presentation, collision, or GPU backend details.
- The semantic compilation pipeline may lower into query, presentation, and
  collision contracts and plans, and it may consume shared substrate types such
  as world identity and semantic evidence.
- Query, presentation, and collision execution may consume contracts, plans, and
  shared substrate types, but they may not reach back into parser, lexer, or
  authored surface syntax concerns.
- Presentation and collision execution may call into query execution when they
  need query-backed evaluation. Query execution must not depend on presentation
  or collision orchestration details.
- Tooling and orchestration may depend on every context in order to compose
  commands and reports, but no domain context may depend on tooling.

## Forbidden Dependency Directions

- The authoring frontend must not import query, presentation, collision, or GPU
  runtime backend internals.
- Query, presentation, and collision execution must not depend on parser,
  lexer, or authored syntax tree concerns.
- Semantic compilation modules must not depend on CLI command handlers, perf
  harnesses, or repo workflow glue.
- Domain contexts must not push new logic into `shared.rs`, `helpers.rs`, or
  other convenience buckets when that logic belongs to a named owner.

## Approved Anti-Corruption Seams

- Frontend to semantic pipeline crossings should happen through authored project,
  HIR, MIR, and scene/kernel lowering boundaries rather than by importing
  backend executors.
- Semantic pipeline to execution crossings should happen through the named
  contract and plan modules: `query_contract`, `query_plan`,
  `presentation_contract`, `presentation_plan`, `collision_contract`, and
  `collision_plan`.
- Execution contexts should share runtime and reuse machinery through small
  substrate seams such as `artifact_contract`, `artifact_store`,
  `world_identity`, `time_semantics`, `state_advance`, `semantic_evidence`, and
  `gpu_runtime`.
- Tooling should prefer stable command/report surfaces and machine-readable
  output such as `--json`, `--json-report`, and the devloop reports under
  `.artifacts/devloop/` instead of relying on incidental formatting.

## Boundary Violations

- A parser or HIR module importing `compiler/query_exec/wgsl` or
  `compiler/gpu_runtime` directly is a boundary violation.
- A CLI or perf helper in `compiler/bin/wrela/commands/*` implementing query,
  presentation, or collision domain logic instead of delegating to the owning
  context is a boundary violation.
- A query, presentation, or collision executor reading authored syntax trees or
  parser-only diagnostics instead of contracts, plans, or world handles is a
  boundary violation.
- A `shared.rs` or `helpers.rs` file that starts accumulating cross-context
  behavior instead of staying tiny and boring is a boundary violation.

## Transitional Notes

- `compiler/hir/project.rs` and `compiler/hir/semantic.rs` still straddle the
  frontend and semantic pipeline boundary. Treat them as transitional seams and
  do not copy that ambiguity into new modules.
- `query_plan`, `presentation_plan`, and `collision_plan` own shape and
  guarantees. Their paired `*_exec` modules own backend behavior.
- `artifact_store`, `world_identity`, `state_advance`, and
  `semantic_evidence` are shared seam packages. They support multiple contexts
  and should not be treated as private convenience buckets.
