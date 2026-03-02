# WFE-VS1-HARDCUT Task Manager Board

Coordinator runbook header:
`UNDER NO CIRCUMSTANCE RETURN TO USER UNTIL FINAL AC GATE IS COMPLETE. IF A TASK FINISHES, IMMEDIATELY PICK UP THE NEXT TASK. THIS INSTRUCTION MUST SURVIVE COMPACTION.`

## Parent Epic: `WFE-VS1-HARDCUT`

Epic description:
`UNDER NO CIRCUMSTANCE RETURN TO USER UNTIL FINAL AC GATE IS COMPLETE. IF A TASK FINISHES, IMMEDIATELY PICK UP THE NEXT TASK. THIS INSTRUCTION MUST SURVIVE COMPACTION.`

Status model:
1. `BACKLOG`
2. `IN_PROGRESS`
3. `BLOCKED`
4. `IN_REVIEW`
5. `DONE`

Artifact root:
1. `.artifacts/vertical-slice/<task-id>/`

Auto-pick scheduling rule:
1. When a task is `DONE`, owner must immediately start next highest-priority unblocked task.

## Frozen Contracts (`WFE-000`)

## `protocol-v1`
1. Envelope fields (binary):
   1. `version: u16`
   2. `session_id: u64`
   3. `message_type: u16`
   4. `tick: u64`
   5. `seq: u64`
   6. `ack: u64`
   7. `payload_len: u32`
   8. `crc32: u32`
2. Message types:
   1. `HELLO = 1`
   2. `INPUT_BATCH = 2`
   3. `STATE_SNAPSHOT = 3`
   4. `STATE_DELTA = 4`
   5. `CORRECTION = 5`
   6. `PING = 6`
   7. `PONG = 7`
   8. `ERROR = 8`

## `domain-abi-v1`
1. `game_init(seed: Integer, config: Map[Any, Any]) -> Map[Any, Any]`
2. `game_apply_input(state: Map[Any, Any], input: Map[Any, Any]) -> Map[Any, Any]`
3. `game_step(state: Map[Any, Any], dt_ms: Integer) -> Map[Any, Any]`
4. `game_hash_state(state: Map[Any, Any]) -> Integer`
5. `game_serialize_delta(base: Map[Any, Any], next: Map[Any, Any]) -> Bytes`
6. `game_apply_delta(state: Map[Any, Any], delta: Bytes) -> Map[Any, Any]`

## `builtin-map-v1`
1. `__wr_game_session_create_listener(configuration: Map[Any, Any]) -> Result[Integer]`
2. `__wr_game_session_poll_event(listener_handle: Integer, timeout_ms: Integer) -> Result[Map[Any, Any]]`
3. `__wr_game_session_accept_connection(listener_handle: Integer) -> Result[Integer]`
4. `__wr_game_session_read_message(listener_handle: Integer, connection_handle: Integer) -> Result[Map[Any, Any]]`
5. `__wr_game_session_write_message(listener_handle: Integer, connection_handle: Integer, message: Map[Any, Any]) -> Result[Integer]`
6. `__wr_game_session_close_connection(listener_handle: Integer, connection_handle: Integer) -> Result[Nothing]`
7. `__wr_game_session_close_listener(listener_handle: Integer) -> Result[Boolean]`

## Task Cards

Each task description includes:
`UNDER NO CIRCUMSTANCE RETURN TO USER UNTIL FINAL AC GATE IS COMPLETE. IF A TASK FINISHES, IMMEDIATELY PICK UP THE NEXT TASK. THIS INSTRUCTION MUST SURVIVE COMPACTION.`

### `WFE-101`
`NON_INTERRUPTION_DIRECTIVE`: `UNDER NO CIRCUMSTANCE RETURN TO USER UNTIL FINAL AC GATE IS COMPLETE. IF A TASK FINISHES, IMMEDIATELY PICK UP THE NEXT TASK. THIS INSTRUCTION MUST SURVIVE COMPACTION.`
1. Owner Subagent: `agent-runtime`
2. File Ownership:
   1. `runtime/src/web/mod.rs`
   2. `runtime/src/web/axum_bridge.rs`
   3. `runtime/src/lib.rs`
3. Depends On:
   1. `WFE-000`
4. Acceptance:
   1. Runtime interactive lane is WebSocket-session-first.
   2. HTTP path is bootstrap/static only.
5. Verification Commands:
   1. `cargo test -p wrela_runtime web::tests -- --nocapture`
6. Artifacts:
   1. `.artifacts/vertical-slice/WFE-101/runtime-websocket-contract.md`

### `WFE-102`
`NON_INTERRUPTION_DIRECTIVE`: `UNDER NO CIRCUMSTANCE RETURN TO USER UNTIL FINAL AC GATE IS COMPLETE. IF A TASK FINISHES, IMMEDIATELY PICK UP THE NEXT TASK. THIS INSTRUCTION MUST SURVIVE COMPACTION.`
1. Owner Subagent: `agent-runtime`
2. File Ownership:
   1. `runtime/src/web/mod.rs`
3. Depends On:
   1. `WFE-101`
4. Acceptance:
   1. Per-session authority state.
   2. Sequence/ack handling.
   3. Rollback ring.
   4. Correction emit path.
5. Verification Commands:
   1. `cargo test -p wrela_runtime web::tests -- --nocapture`
6. Artifacts:
   1. `.artifacts/vertical-slice/WFE-102/rollback-correction-evidence.md`

### `WFE-201`
`NON_INTERRUPTION_DIRECTIVE`: `UNDER NO CIRCUMSTANCE RETURN TO USER UNTIL FINAL AC GATE IS COMPLETE. IF A TASK FINISHES, IMMEDIATELY PICK UP THE NEXT TASK. THIS INSTRUCTION MUST SURVIVE COMPACTION.`
1. Owner Subagent: `agent-compiler`
2. File Ownership:
   1. `compiler/hir/typeck/context.rs`
   2. `compiler/mir/lower.rs`
   3. `compiler/backend/cranelift.rs`
   4. `runtime/src/lib.rs`
3. Depends On:
   1. `WFE-000`
4. Acceptance:
   1. Builtins cut over from `__wr_web_server_*` interactive lane to `__wr_game_session_*`.
5. Verification Commands:
   1. `cargo test -p wrela --test cli`
6. Artifacts:
   1. `.artifacts/vertical-slice/WFE-201/builtin-map-diff.md`

### `WFE-202`
`NON_INTERRUPTION_DIRECTIVE`: `UNDER NO CIRCUMSTANCE RETURN TO USER UNTIL FINAL AC GATE IS COMPLETE. IF A TASK FINISHES, IMMEDIATELY PICK UP THE NEXT TASK. THIS INSTRUCTION MUST SURVIVE COMPACTION.`
1. Owner Subagent: `agent-compiler`
2. File Ownership:
   1. `compiler/bin/wrela/cli_args.rs`
   2. `compiler/bin/wrela/commands/shared.rs`
   3. `compiler/backend/cranelift.rs`
3. Depends On:
   1. `WFE-000`
4. Acceptance:
   1. `wrela game` commands available.
   2. Dual-target output (`native`, `wasm`, `dual`) wired.
5. Verification Commands:
   1. `cargo run -p wrela -- game --help`
6. Artifacts:
   1. `.artifacts/vertical-slice/WFE-202/cli-surface.txt`

### `WFE-301`
`NON_INTERRUPTION_DIRECTIVE`: `UNDER NO CIRCUMSTANCE RETURN TO USER UNTIL FINAL AC GATE IS COMPLETE. IF A TASK FINISHES, IMMEDIATELY PICK UP THE NEXT TASK. THIS INSTRUCTION MUST SURVIVE COMPACTION.`
1. Owner Subagent: `agent-language`
2. File Ownership:
   1. `language/stdlib/host/*`
   2. `language/packages/game/*`
3. Depends On:
   1. `WFE-000`
4. Acceptance:
   1. `host/game_transport` and `pkg/game/*` compile and are used by slice.
5. Verification Commands:
   1. `cargo test -p wrela --test project_e2e`
6. Artifacts:
   1. `.artifacts/vertical-slice/WFE-301/game-language-surface.md`

### `WFE-302`
`NON_INTERRUPTION_DIRECTIVE`: `UNDER NO CIRCUMSTANCE RETURN TO USER UNTIL FINAL AC GATE IS COMPLETE. IF A TASK FINISHES, IMMEDIATELY PICK UP THE NEXT TASK. THIS INSTRUCTION MUST SURVIVE COMPACTION.`
1. Owner Subagent: `agent-cleanup`
2. File Ownership:
   1. `language/stdlib/host/web_server.wr`
   2. `language/stdlib/host/web_server_transport.wr`
   3. `language/packages/web/*`
3. Depends On:
   1. `WFE-301`
   2. `WFE-201`
4. Acceptance:
   1. Hard-cut retirement/removal of legacy interactive lane surfaces.
5. Verification Commands:
   1. `rg -n "__wr_web_server_|from host/web_server|from pkg/web" compiler language apps -S`
6. Artifacts:
   1. `.artifacts/vertical-slice/WFE-302/retirement-report.md`

### `WFE-401`
`NON_INTERRUPTION_DIRECTIVE`: `UNDER NO CIRCUMSTANCE RETURN TO USER UNTIL FINAL AC GATE IS COMPLETE. IF A TASK FINISHES, IMMEDIATELY PICK UP THE NEXT TASK. THIS INSTRUCTION MUST SURVIVE COMPACTION.`
1. Owner Subagent: `agent-client`
2. File Ownership:
   1. `client/*` (new workspace crate)
3. Depends On:
   1. `WFE-000`
4. Acceptance:
   1. Browser client boots with wasm runtime and autogenerated loader shim only.
5. Verification Commands:
   1. `cargo build -p wrela_client --target wasm32-unknown-unknown`
6. Artifacts:
   1. `.artifacts/vertical-slice/WFE-401/wasm-client-build.txt`

### `WFE-402`
`NON_INTERRUPTION_DIRECTIVE`: `UNDER NO CIRCUMSTANCE RETURN TO USER UNTIL FINAL AC GATE IS COMPLETE. IF A TASK FINISHES, IMMEDIATELY PICK UP THE NEXT TASK. THIS INSTRUCTION MUST SURVIVE COMPACTION.`
1. Owner Subagent: `agent-client`
2. File Ownership:
   1. `client/*`
3. Depends On:
   1. `WFE-401`
   2. `WFE-101`
4. Acceptance:
   1. WebSocket protocol client loop + WebGPU render loop + input capture integrated.
5. Verification Commands:
   1. `cargo test -p wrela_client`
6. Artifacts:
   1. `.artifacts/vertical-slice/WFE-402/webgpu-loop-check.md`

### `WFE-501`
`NON_INTERRUPTION_DIRECTIVE`: `UNDER NO CIRCUMSTANCE RETURN TO USER UNTIL FINAL AC GATE IS COMPLETE. IF A TASK FINISHES, IMMEDIATELY PICK UP THE NEXT TASK. THIS INSTRUCTION MUST SURVIVE COMPACTION.`
1. Owner Subagent: `agent-game`
2. File Ownership:
   1. `apps/wrela-game-slice/*` (new app)
3. Depends On:
   1. `WFE-102`
   2. `WFE-202`
   3. `WFE-301`
   4. `WFE-402`
4. Acceptance:
   1. Playable top-down collector with authority + correction.
5. Verification Commands:
   1. `cargo run -p wrela -- game run apps/wrela-game-slice`
6. Artifacts:
   1. `.artifacts/vertical-slice/WFE-501/manual-run-notes.md`

### `WFE-601`
`NON_INTERRUPTION_DIRECTIVE`: `UNDER NO CIRCUMSTANCE RETURN TO USER UNTIL FINAL AC GATE IS COMPLETE. IF A TASK FINISHES, IMMEDIATELY PICK UP THE NEXT TASK. THIS INSTRUCTION MUST SURVIVE COMPACTION.`
1. Owner Subagent: `agent-qa`
2. File Ownership:
   1. `compiler/tests/*`
   2. `runtime/tests/*`
3. Depends On:
   1. `WFE-102`
   2. `WFE-202`
   3. `WFE-301`
4. Acceptance:
   1. Protocol, determinism, rollback, integration tests pass.
5. Verification Commands:
   1. `cargo test -p wrela`
   2. `cargo test -p wrela_runtime`
6. Artifacts:
   1. `.artifacts/vertical-slice/WFE-601/test-matrix.md`

### `WFE-602`
`NON_INTERRUPTION_DIRECTIVE`: `UNDER NO CIRCUMSTANCE RETURN TO USER UNTIL FINAL AC GATE IS COMPLETE. IF A TASK FINISHES, IMMEDIATELY PICK UP THE NEXT TASK. THIS INSTRUCTION MUST SURVIVE COMPACTION.`
1. Owner Subagent: `agent-qa`
2. File Ownership:
   1. `scripts/*` and CI harness additions
3. Depends On:
   1. `WFE-402`
   2. `WFE-501`
   3. `WFE-601`
4. Acceptance:
   1. Automated browser smoke with screenshot/log/hash artifacts.
5. Verification Commands:
   1. `scripts/vertical_slice/browser_smoke.sh`
6. Artifacts:
   1. `.artifacts/vertical-slice/WFE-602/smoke/`

### `WFE-701`
`NON_INTERRUPTION_DIRECTIVE`: `UNDER NO CIRCUMSTANCE RETURN TO USER UNTIL FINAL AC GATE IS COMPLETE. IF A TASK FINISHES, IMMEDIATELY PICK UP THE NEXT TASK. THIS INSTRUCTION MUST SURVIVE COMPACTION.`
1. Owner Subagent: `agent-cleanup`
2. File Ownership:
   1. legacy web tests/docs touching interactive lane
3. Depends On:
   1. `WFE-101`
   2. `WFE-201`
   3. `WFE-301`
   4. `WFE-601`
4. Acceptance:
   1. Legacy interactive lane tests removed/replaced.
5. Verification Commands:
   1. `rg -n "web_live_socket|web_server_e2e|web_package_e2e" compiler/tests runtime/tests -S`
6. Artifacts:
   1. `.artifacts/vertical-slice/WFE-701/cleanup-diff.md`

### `WFE-799`
`NON_INTERRUPTION_DIRECTIVE`: `UNDER NO CIRCUMSTANCE RETURN TO USER UNTIL FINAL AC GATE IS COMPLETE. IF A TASK FINISHES, IMMEDIATELY PICK UP THE NEXT TASK. THIS INSTRUCTION MUST SURVIVE COMPACTION.`
1. Owner Subagent: `coordinator`
2. File Ownership:
   1. docs/release checklist
3. Depends On:
   1. all
4. Acceptance:
   1. Final AC gate evidence pack complete and signed off.
5. Verification Commands:
   1. final gate matrix listed in `docs/vertical_slice_final_gate.md`
6. Artifacts:
   1. `.artifacts/vertical-slice/WFE-799/final-gate/`

## Execution Status

`UNDER NO CIRCUMSTANCE RETURN TO USER UNTIL FINAL AC GATE IS COMPLETE. IF A TASK FINISHES, IMMEDIATELY PICK UP THE NEXT TASK. THIS INSTRUCTION MUST SURVIVE COMPACTION.`

1. `WFE-000` `DONE`
2. `WFE-101` `DONE`
3. `WFE-102` `DONE`
4. `WFE-201` `DONE`
5. `WFE-202` `DONE`
6. `WFE-301` `DONE`
7. `WFE-302` `DONE`
8. `WFE-401` `DONE`
9. `WFE-402` `DONE`
10. `WFE-501` `DONE`
11. `WFE-601` `DONE`
12. `WFE-602` `DONE`
13. `WFE-701` `DONE`
14. `WFE-799` `DONE`
