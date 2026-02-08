# Parallel Track Implementation Playbooks (A/B/C/D/E)

Audience: junior contributors shipping runtime rewrite work without architectural drift.

## Track A - Runtime Kernel/Engine

- Start here:
  - `core/runtime/src/lib.rs`
  - `core/runtime/src/actor.rs`
  - `core/runtime/src/scheduler.rs`
- Example change:
  - Add a primitive runtime export that reports queue depth without injecting policy decisions.
- Required validation before merge:
  - `cargo test -p wrela_runtime --lib`
  - `cargo test -p wrela --test codegen native_pool_mailbox_len_smoke -- --exact`
- Anti-patterns:
  - Adding default fairness/backpressure policy logic directly in Rust runtime.

## Track B - Compiler ABI/Lowering

- Start here:
  - `core/compiler/hir/semantic.rs`
  - `core/compiler/mir/lower.rs`
  - `core/compiler/tests/thin_core_snapshot.rs`
- Example change:
  - Wire a new intrinsic call through semantic + MIR with explicit snapshot update.
- Required validation before merge:
  - `cargo test -p wrela --test thin_core_snapshot`
  - `cargo test -p wrela --tests`
- Anti-patterns:
  - Adding symbol names that bypass thin-core guardrails or encode policy behavior.

## Track C - Runtime Surface/Stdlib API

- Start here:
  - `core/compiler/stdlib/*.wr`
  - `core/spec/stdlib_surface.wr`
- Example change:
  - Introduce a stdlib helper that composes existing primitives without changing runtime ABI.
- Required validation before merge:
  - `./target/debug/wrela test`
  - `cargo test -p wrela --test codegen`
- Anti-patterns:
  - Sneaking product/domain modules back into thin-core runtime surface.

## Track D - Validation/Perf/Snapshots

- Start here:
  - `core/compiler/tests/codegen.rs`
  - `core/compiler/tests/thin_core_snapshot.rs`
  - `core/spec/runtime_rewrite/perf_harness.sh`
- Example change:
  - Add a policy matrix row for parity tests and rerun perf baseline + regression budget.
- Required validation before merge:
  - `cargo test -p wrela --test codegen native_pool_policy_matrix_smoke -- --exact`
  - `cargo test -p wrela --test thin_core_snapshot`
  - `./core/spec/runtime_rewrite/perf_harness.sh current`
- Anti-patterns:
  - Merging behavior changes without AC evidence and before/after perf numbers.

## Track E - Docs/Cutover/Governance

- Start here:
  - `core/spec/runtime_rewrite/*.md`
  - `README.md`
  - `INSTALL.md`
- Example change:
  - Update migration checklist with binary pass/fail criteria and rollback steps.
- Required validation before merge:
  - Confirm all commands in docs execute successfully once.
  - Confirm links from top-level docs resolve.
- Anti-patterns:
  - Vague “looks good” signoff criteria that cannot be proven from command output.

## 10-Minute Triage Flow For Juniors

1. Read issue AC and map each AC item to one concrete file/test command.
2. Open the track section above and start with the first listed file.
3. Implement the minimal change.
4. Run only the required validations for your track, then expand to full affected suite.
5. Post AC checklist evidence in Linear with command output summaries.
