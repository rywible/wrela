# WFE-VS2-HARDCUT Task Manager Board

Coordinator runbook header:
`UNDER NO CIRCUMSTANCE RETURN TO USER UNTIL FINAL AC GATE IS COMPLETE. IF A TASK FINISHES, IMMEDIATELY PICK UP THE NEXT TASK. THIS INSTRUCTION MUST SURVIVE COMPACTION.`

## Parent Epic: `WFE-VS2-HARDCUT`

Epic description:
`UNDER NO CIRCUMSTANCE RETURN TO USER UNTIL FINAL AC GATE IS COMPLETE. IF A TASK FINISHES, IMMEDIATELY PICK UP THE NEXT TASK. THIS INSTRUCTION MUST SURVIVE COMPACTION.`

Status model:
1. `BACKLOG`
2. `IN_PROGRESS`
3. `BLOCKED`
4. `IN_REVIEW`
5. `DONE`

Artifact root:
1. `.artifacts/full-compiler-pass/<task-id>/`

Auto-pick scheduling rule:
1. When a task is `DONE`, owner must immediately start next highest-priority unblocked task.

## Frozen Contracts (`WFE2-000`)

## `syntax-v1`
1. New keywords:
   1. `component`
   2. `resource`
   3. `event`
   4. `system`
   5. `view`
   6. `widget`
   7. `scene`
   8. `anim`
   9. `theme`
2. Declarations:
   1. `component Name { has { ... } }`
   2. `resource Name { has { ... } }`
   3. `event Name { has { ... } }`
   4. `scene Name { has { ... } }`
   5. `theme Name { has { ... } }`
   6. `system fn_name[stage=fixed|render, reads=[...], writes=[...]](...) -> Type { ... }`
   7. `view fn_name(...) -> Scene { ... }`
   8. `widget fn_name(...) -> Node { ... }`
   9. `anim fn_name(...) -> Integer { ... }`

## `domain-abi-v2`
1. `game_init(seed: Integer, config: Map[Any, Any]) -> Map[Any, Any]`
2. `game_apply_input(state: Map[Any, Any], input: Map[Any, Any]) -> Map[Any, Any]`
3. `game_step(state: Map[Any, Any], dt_ms: Integer) -> Map[Any, Any]`
4. `game_hash_state(state: Map[Any, Any]) -> Integer`
5. `game_serialize_delta(base: Map[Any, Any], next: Map[Any, Any]) -> Bytes`
6. `game_apply_delta(state: Map[Any, Any], delta: Bytes) -> Map[Any, Any]`

## `codegen-target-v2`
1. `native`
2. `wasm`
3. `dual`

## Task Cards

Each task description includes:
`UNDER NO CIRCUMSTANCE RETURN TO USER UNTIL FINAL AC GATE IS COMPLETE. IF A TASK FINISHES, IMMEDIATELY PICK UP THE NEXT TASK. THIS INSTRUCTION MUST SURVIVE COMPACTION.`

### `WFE2-101`
`NON_INTERRUPTION_DIRECTIVE`: `UNDER NO CIRCUMSTANCE RETURN TO USER UNTIL FINAL AC GATE IS COMPLETE. IF A TASK FINISHES, IMMEDIATELY PICK UP THE NEXT TASK. THIS INSTRUCTION MUST SURVIVE COMPACTION.`
1. Owner Subagent: `agent-syntax`
2. File Ownership:
   1. `compiler/lexer/*`
   2. `compiler/parser/kind.rs`
3. Depends On:
   1. `WFE2-000`
4. Acceptance:
   1. New keywords tokenized.
   2. Legacy deterministic-lane forms hard-error.
5. Verification Commands:
   1. `cargo test -p wrela parser::`
6. Artifacts:
   1. `.artifacts/full-compiler-pass/WFE2-101/lexer-keyword-diff.md`

### `WFE2-102`
`NON_INTERRUPTION_DIRECTIVE`: `UNDER NO CIRCUMSTANCE RETURN TO USER UNTIL FINAL AC GATE IS COMPLETE. IF A TASK FINISHES, IMMEDIATELY PICK UP THE NEXT TASK. THIS INSTRUCTION MUST SURVIVE COMPACTION.`
1. Owner Subagent: `agent-syntax`
2. File Ownership:
   1. `compiler/parser/grammar/*`
   2. `compiler/parser/ast.rs`
   3. `compiler/parser/validate.rs`
3. Depends On:
   1. `WFE2-101`
4. Acceptance:
   1. Grammar + AST nodes for all extended declarations parse deterministically.
5. Verification Commands:
   1. `cargo test -p wrela parser::`
6. Artifacts:
   1. `.artifacts/full-compiler-pass/WFE2-102/parser-ast-evidence.md`

### `WFE2-103`
`NON_INTERRUPTION_DIRECTIVE`: `UNDER NO CIRCUMSTANCE RETURN TO USER UNTIL FINAL AC GATE IS COMPLETE. IF A TASK FINISHES, IMMEDIATELY PICK UP THE NEXT TASK. THIS INSTRUCTION MUST SURVIVE COMPACTION.`
1. Owner Subagent: `agent-hir`
2. File Ownership:
   1. `compiler/hir/def.rs`
   2. `compiler/hir/lower.rs`
   3. `compiler/hir/mod.rs`
3. Depends On:
   1. `WFE2-102`
4. Acceptance:
   1. Role-aware lowering metadata exists for declaration kinds.
5. Verification Commands:
   1. `cargo test -p wrela hir::`
6. Artifacts:
   1. `.artifacts/full-compiler-pass/WFE2-103/hir-role-model.md`

### `WFE2-104`
`NON_INTERRUPTION_DIRECTIVE`: `UNDER NO CIRCUMSTANCE RETURN TO USER UNTIL FINAL AC GATE IS COMPLETE. IF A TASK FINISHES, IMMEDIATELY PICK UP THE NEXT TASK. THIS INSTRUCTION MUST SURVIVE COMPACTION.`
1. Owner Subagent: `agent-hir`
2. File Ownership:
   1. `compiler/hir/semantic.rs`
   2. `compiler/hir/typeck/*`
   3. `compiler/hir/project.rs`
3. Depends On:
   1. `WFE2-103`
4. Acceptance:
   1. System metadata checks and lane boundary checks compile.
5. Verification Commands:
   1. `cargo test -p wrela hir::semantic`
6. Artifacts:
   1. `.artifacts/full-compiler-pass/WFE2-104/semantic-contracts.md`

### `WFE2-105`
`NON_INTERRUPTION_DIRECTIVE`: `UNDER NO CIRCUMSTANCE RETURN TO USER UNTIL FINAL AC GATE IS COMPLETE. IF A TASK FINISHES, IMMEDIATELY PICK UP THE NEXT TASK. THIS INSTRUCTION MUST SURVIVE COMPACTION.`
1. Owner Subagent: `agent-hir`
2. File Ownership:
   1. `compiler/hir/typeck/*`
   2. `compiler/diag/*`
   3. `compiler/bin/wrela/diag_emit.rs`
3. Depends On:
   1. `WFE2-103`
4. Acceptance:
   1. Float disallowed in deterministic lane with clear diagnostics.
5. Verification Commands:
   1. `cargo test -p wrela hir::typeck`
6. Artifacts:
   1. `.artifacts/full-compiler-pass/WFE2-105/fixed-lane-gate.md`

### `WFE2-201`
`NON_INTERRUPTION_DIRECTIVE`: `UNDER NO CIRCUMSTANCE RETURN TO USER UNTIL FINAL AC GATE IS COMPLETE. IF A TASK FINISHES, IMMEDIATELY PICK UP THE NEXT TASK. THIS INSTRUCTION MUST SURVIVE COMPACTION.`
1. Owner Subagent: `agent-backend`
2. File Ownership:
   1. `compiler/backend/cranelift.rs`
   2. `compiler/backend/mod.rs`
3. Depends On:
   1. `WFE2-000`
4. Acceptance:
   1. Target abstraction for native/wasm/dual exists.
5. Verification Commands:
   1. `cargo test -p wrela --test codegen`
6. Artifacts:
   1. `.artifacts/full-compiler-pass/WFE2-201/target-abstraction.md`

### `WFE2-202`
`NON_INTERRUPTION_DIRECTIVE`: `UNDER NO CIRCUMSTANCE RETURN TO USER UNTIL FINAL AC GATE IS COMPLETE. IF A TASK FINISHES, IMMEDIATELY PICK UP THE NEXT TASK. THIS INSTRUCTION MUST SURVIVE COMPACTION.`
1. Owner Subagent: `agent-backend`
2. File Ownership:
   1. `compiler/backend/cranelift.rs`
3. Depends On:
   1. `WFE2-201`
4. Acceptance:
   1. Compiler emits wasm artifact from Wrela source graph path.
5. Verification Commands:
   1. `cargo run -p wrela -- game build apps/wrela-game-slice --target=wasm`
6. Artifacts:
   1. `.artifacts/full-compiler-pass/WFE2-202/wasm-emission.md`

### `WFE2-203`
`NON_INTERRUPTION_DIRECTIVE`: `UNDER NO CIRCUMSTANCE RETURN TO USER UNTIL FINAL AC GATE IS COMPLETE. IF A TASK FINISHES, IMMEDIATELY PICK UP THE NEXT TASK. THIS INSTRUCTION MUST SURVIVE COMPACTION.`
1. Owner Subagent: `agent-backend`
2. File Ownership:
   1. `compiler/mir/lower.rs`
   2. `compiler/backend/cranelift.rs`
   3. `compiler/hir/typeck/context.rs`
3. Depends On:
   1. `WFE2-201`
   2. `WFE2-104`
4. Acceptance:
   1. Domain ABI signatures exported for native+wasm.
5. Verification Commands:
   1. `cargo test -p wrela --test thin_core_snapshot`
6. Artifacts:
   1. `.artifacts/full-compiler-pass/WFE2-203/domain-abi-exports.md`

### `WFE2-204`
`NON_INTERRUPTION_DIRECTIVE`: `UNDER NO CIRCUMSTANCE RETURN TO USER UNTIL FINAL AC GATE IS COMPLETE. IF A TASK FINISHES, IMMEDIATELY PICK UP THE NEXT TASK. THIS INSTRUCTION MUST SURVIVE COMPACTION.`
1. Owner Subagent: `agent-cli`
2. File Ownership:
   1. `compiler/bin/wrela/cli_args.rs`
   2. `compiler/bin/wrela/commands/game.rs`
   3. `compiler/bin/wrela/commands/shared.rs`
   4. `compiler/bin/wrela/diag_emit.rs`
3. Depends On:
   1. `WFE2-202`
   2. `WFE2-203`
4. Acceptance:
   1. Game commands use compiler-owned dual-target artifacts only.
5. Verification Commands:
   1. `cargo run -p wrela -- game --help`
6. Artifacts:
   1. `.artifacts/full-compiler-pass/WFE2-204/cli-contract.md`

### `WFE2-301`
`NON_INTERRUPTION_DIRECTIVE`: `UNDER NO CIRCUMSTANCE RETURN TO USER UNTIL FINAL AC GATE IS COMPLETE. IF A TASK FINISHES, IMMEDIATELY PICK UP THE NEXT TASK. THIS INSTRUCTION MUST SURVIVE COMPACTION.`
1. Owner Subagent: `agent-language`
2. File Ownership:
   1. `language/stdlib/host/game_transport.wr`
   2. `language/packages/game/*`
3. Depends On:
   1. `WFE2-000`
4. Acceptance:
   1. `pkg/game/fixed` and keyword-lowering contracts exist.
5. Verification Commands:
   1. `cargo run -p wrela -- check language/spec`
6. Artifacts:
   1. `.artifacts/full-compiler-pass/WFE2-301/language-surface.md`

### `WFE2-302`
`NON_INTERRUPTION_DIRECTIVE`: `UNDER NO CIRCUMSTANCE RETURN TO USER UNTIL FINAL AC GATE IS COMPLETE. IF A TASK FINISHES, IMMEDIATELY PICK UP THE NEXT TASK. THIS INSTRUCTION MUST SURVIVE COMPACTION.`
1. Owner Subagent: `agent-runtime`
2. File Ownership:
   1. `runtime/src/web/*`
   2. `runtime/src/lib.rs`
3. Depends On:
   1. `WFE2-203`
   2. `WFE2-301`
4. Acceptance:
   1. Runtime executes compiler-owned domain ABI lane.
5. Verification Commands:
   1. `cargo test -p wrela_runtime`
6. Artifacts:
   1. `.artifacts/full-compiler-pass/WFE2-302/runtime-domain-adapter.md`

### `WFE2-303`
`NON_INTERRUPTION_DIRECTIVE`: `UNDER NO CIRCUMSTANCE RETURN TO USER UNTIL FINAL AC GATE IS COMPLETE. IF A TASK FINISHES, IMMEDIATELY PICK UP THE NEXT TASK. THIS INSTRUCTION MUST SURVIVE COMPACTION.`
1. Owner Subagent: `agent-cli`
2. File Ownership:
   1. `compiler/bin/wrela/game_assets/*`
3. Depends On:
   1. `WFE2-204`
   2. `WFE2-302`
4. Acceptance:
   1. Loader consumes compiler ABI exports and render loop converges.
5. Verification Commands:
   1. `cargo run -p wrela -- game build apps/wrela-game-slice --target=dual`
6. Artifacts:
   1. `.artifacts/full-compiler-pass/WFE2-303/loader-abi-proof.md`

### `WFE2-401`
`NON_INTERRUPTION_DIRECTIVE`: `UNDER NO CIRCUMSTANCE RETURN TO USER UNTIL FINAL AC GATE IS COMPLETE. IF A TASK FINISHES, IMMEDIATELY PICK UP THE NEXT TASK. THIS INSTRUCTION MUST SURVIVE COMPACTION.`
1. Owner Subagent: `agent-app-game`
2. File Ownership:
   1. `apps/wrela-game-slice/**`
3. Depends On:
   1. `WFE2-104`
   2. `WFE2-204`
   3. `WFE2-301`
   4. `WFE2-303`
4. Acceptance:
   1. Collector app authored in new Wrela keywords.
5. Verification Commands:
   1. `cargo run -p wrela -- game run apps/wrela-game-slice`
6. Artifacts:
   1. `.artifacts/full-compiler-pass/WFE2-401/game-slice-migration.md`

### `WFE2-402`
`NON_INTERRUPTION_DIRECTIVE`: `UNDER NO CIRCUMSTANCE RETURN TO USER UNTIL FINAL AC GATE IS COMPLETE. IF A TASK FINISHES, IMMEDIATELY PICK UP THE NEXT TASK. THIS INSTRUCTION MUST SURVIVE COMPACTION.`
1. Owner Subagent: `agent-app-web`
2. File Ownership:
   1. `apps/wrela-website-slice/**`
3. Depends On:
   1. `WFE2-104`
   2. `WFE2-204`
   3. `WFE2-301`
   4. `WFE2-303`
4. Acceptance:
   1. Website demo authored in `view/widget/theme/anim` syntax on same lane.
5. Verification Commands:
   1. `cargo run -p wrela -- game run apps/wrela-website-slice`
6. Artifacts:
   1. `.artifacts/full-compiler-pass/WFE2-402/website-slice.md`

### `WFE2-601`
`NON_INTERRUPTION_DIRECTIVE`: `UNDER NO CIRCUMSTANCE RETURN TO USER UNTIL FINAL AC GATE IS COMPLETE. IF A TASK FINISHES, IMMEDIATELY PICK UP THE NEXT TASK. THIS INSTRUCTION MUST SURVIVE COMPACTION.`
1. Owner Subagent: `agent-qa`
2. File Ownership:
   1. `compiler/tests/*`
   2. `runtime/tests/*`
   3. `scripts/*`
3. Depends On:
   1. `WFE2-401`
   2. `WFE2-402`
   3. `WFE2-302`
4. Acceptance:
   1. Parser/typeck/codegen/parity/rollback/e2e suites pass.
5. Verification Commands:
   1. `cargo test --workspace`
6. Artifacts:
   1. `.artifacts/full-compiler-pass/WFE2-601/test-matrix.md`

### `WFE2-701`
`NON_INTERRUPTION_DIRECTIVE`: `UNDER NO CIRCUMSTANCE RETURN TO USER UNTIL FINAL AC GATE IS COMPLETE. IF A TASK FINISHES, IMMEDIATELY PICK UP THE NEXT TASK. THIS INSTRUCTION MUST SURVIVE COMPACTION.`
1. Owner Subagent: `agent-cleanup`
2. File Ownership:
   1. `legacy domain crate files`
   2. manifests/docs/tests
3. Depends On:
   1. `WFE2-401`
   2. `WFE2-601`
4. Acceptance:
   1. Rust domain crate removed.
   2. no fallback references remain.
5. Verification Commands:
   1. `rg -n "legacy domain fallback references" -S docs`
6. Artifacts:
   1. `.artifacts/full-compiler-pass/WFE2-701/hard-cut-report.md`

### `WFE2-799`
`NON_INTERRUPTION_DIRECTIVE`: `UNDER NO CIRCUMSTANCE RETURN TO USER UNTIL FINAL AC GATE IS COMPLETE. IF A TASK FINISHES, IMMEDIATELY PICK UP THE NEXT TASK. THIS INSTRUCTION MUST SURVIVE COMPACTION.`
1. Owner Subagent: `coordinator`
2. File Ownership:
   1. `docs/*`
3. Depends On:
   1. `WFE2-601`
   2. `WFE2-701`
4. Acceptance:
   1. Final AC evidence pack complete.
5. Verification Commands:
   1. `scripts/full_compiler_pass/final_gate.sh`
6. Artifacts:
   1. `.artifacts/full-compiler-pass/WFE2-799/final-gate/`

### `WFE2-990`
`NON_INTERRUPTION_DIRECTIVE`: `UNDER NO CIRCUMSTANCE RETURN TO USER UNTIL FINAL AC GATE IS COMPLETE. IF A TASK FINISHES, IMMEDIATELY PICK UP THE NEXT TASK. THIS INSTRUCTION MUST SURVIVE COMPACTION.`
1. Owner Subagent: `agent-review`
2. File Ownership:
   1. read-only repo audit
3. Depends On:
   1. `WFE2-799`
4. Acceptance:
   1. Independent architecture/correctness review complete.
   2. All P0-P2 findings remediated and re-reviewed.
5. Verification Commands:
   1. `cargo test --workspace && scripts/full_compiler_pass/review_gate.sh`
6. Artifacts:
   1. `.artifacts/full-compiler-pass/WFE2-990/review-report.md`

## Parallel Waves
1. Wave A: `WFE2-000`, `WFE2-101`, `WFE2-201`, `WFE2-301`.
2. Wave B: `WFE2-102`, `WFE2-103`, `WFE2-202`, `WFE2-302`.
3. Wave C: `WFE2-104`, `WFE2-105`, `WFE2-203`, `WFE2-204`, `WFE2-303`.
4. Wave D: `WFE2-401`, `WFE2-402`, `WFE2-601`.
5. Wave E: `WFE2-701`, `WFE2-799`.
6. Wave F: `WFE2-990`.

## Coordinator Autonomous Scheduling Rules
1. Keep every subagent assigned to exactly one unblocked task.
2. On task completion, immediately assign next unblocked task.
3. Never idle while unblocked work exists.
4. Never send interim status to user.
5. If review finds P0-P2 issues, reopen impacted tasks and rerun gates.

## Execution Status
1. `WFE2-000` `IN_PROGRESS`
2. `WFE2-101` `IN_PROGRESS`
3. `WFE2-102` `BACKLOG`
4. `WFE2-103` `BACKLOG`
5. `WFE2-104` `BACKLOG`
6. `WFE2-105` `BACKLOG`
7. `WFE2-201` `IN_PROGRESS`
8. `WFE2-202` `BACKLOG`
9. `WFE2-203` `BACKLOG`
10. `WFE2-204` `BACKLOG`
11. `WFE2-301` `IN_PROGRESS`
12. `WFE2-302` `BACKLOG`
13. `WFE2-303` `BACKLOG`
14. `WFE2-401` `BACKLOG`
15. `WFE2-402` `BACKLOG`
16. `WFE2-601` `BACKLOG`
17. `WFE2-701` `BACKLOG`
18. `WFE2-799` `BACKLOG`
19. `WFE2-990` `BACKLOG`
