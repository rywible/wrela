# Runtime Kernel-Waist Contract (Rust vs Wrela Policy)

Date: 2026-02-08
Scope: runtime/compiler boundary for rewrite work.

## Rust Runtime Is Allowed To Do

- RC/value/object memory mechanics (`wr_rc_inc`, `wr_rc_dec`, alloc/free, object layout).
- ABI marshaling and boundary glue (`wr_*` exports, `__wr_*` intrinsic endpoints).
- Syscall/event primitives only (clock, env, fs byte IO, scheduler wakeup primitives).
- Deterministic transport/execution primitives (mailbox queue operations, ready-queue movement).
- Metrics collection counters without changing policy outcomes.

## Rust Runtime Is Forbidden To Do

- Default policy selection that changes behavior semantics.
- Fairness rules that decide user-visible execution priority semantics.
- Backpressure semantics beyond primitive queue/drop mechanics requested by policy.
- Domain behavior choices (retry logic, failure policy, workflow policy, business defaults).
- Hidden policy coupling in helper names or symbol surfaces (`*_policy_*` classes are forbidden).

## Allowed vs Forbidden Examples

1. Allowed: `wr_pool_queue_len` reports primitive queue depth.
   Forbidden: Rust deciding queue budget per actor based on semantic objective defaults.
2. Allowed: `wr_actor_pause` toggles pause state and wakeup notifications.
   Forbidden: Rust deciding when an actor should pause based on workflow-level policy.
3. Allowed: `wr_runtime_cpu_count` exposes host primitive.
   Forbidden: Rust selecting product-level concurrency policy from that value.
4. Allowed: symbol export `wr_metrics_get` to read counters.
   Forbidden: symbols like `wr_policy_scheduler_tick` that encode policy behavior.

## Runtime PR Review Checklist (Behavior-Ban Checklist)

Use this checklist in every runtime/compiler PR touching the kernel waist.

- [ ] Does any new/changed function compute a policy decision (yes/no)? If yes, move it to Wrela stdlib.
- [ ] Are new symbols confined to primitive ABI/mechanics (`wr_*`, `__wr_*`) with no policy prefixes?
- [ ] Does the change only expose primitives, not choose defaults/semantics for users?
- [ ] If queue/backpressure logic changed, is it strictly mechanical and driven by explicit caller policy inputs?
- [ ] If fairness/scheduling changed, is this a primitive transport fix (not behavioral policy)?
- [ ] Did you update `core/spec/thin_core_snapshot.txt` intentionally if symbol surface changed?
- [ ] Did you run parity tests proving pause/resume, mailbox, backpressure, and pool-size behavior did not drift?

## Follow-On Usage Rule

All Phase 1+ runtime rewrite issues must link this contract and include the checklist in their implementation notes.
