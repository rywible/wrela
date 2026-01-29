# Performance Plan (v0.x)

Goals:
- Reduce await boundary overhead (enqueue + pending + scheduling).
- Cut refcount churn and short-lived allocations.
- Improve tail latency under load with minimal engineering effort.

Scope:
- Items 1-6 only (no kernel-bypass or custom NIC runtime).
- Favor safe, incremental changes with clear fallback.

Guiding Principles:
- Prefer changes that eliminate whole categories of overhead.
- Keep language semantics stable; start with compiler/runtime optimizations.
- Make performance knobs configurable and testable.

## 1) Ownership Transfer Across Actor Boundaries

Objective:
- Remove most `wr_rc_inc/dec` traffic when sending values to actors.

Approach:
- Introduce move semantics for temporaries and locals proven dead after send.
- In MIR, mark arguments as `Move` when safe; lower to runtime send without inc/dec.
- For values that escape or are reused, keep current RC behavior.

Key Changes:
- HIR/MIR analysis pass to detect last-use on actor call args.
- Backend: new runtime entry points or flags to indicate "owned" args.
- Runtime: avoid refcount ops for moved values.

Risks:
- Incorrect liveness can cause use-after-free or leaks.
- Needs strong test coverage around aliasing and control flow.

Success Metrics:
- Significant drop in `METRIC_RC_INC/DEC`.
- Lower CPU time in actor-heavy benchmarks.

## 2) Single-Producer/Consumer Await Elision

Objective:
- Bypass mailbox and pending where interleavings are provably irrelevant.

Approach:
- Build a call graph of actor interactions.
- Detect edges where the receiver has exactly one sender.
- Rewrite `await send` into direct call when safe.

Key Changes:
- Compiler analysis pass (project-wide).
- Lowering path for "direct actor call" (no mailbox).
- Fallback to normal send when uncertainty exists.

Risks:
- Incorrect analysis breaks ordering semantics.
- Needs conservative rules; start with simple cases.

Success Metrics:
- Reduced enqueue/await overhead on hot paths.
- Lower tail latency in microbenchmarks with chain actors.

## 3) Arena Allocation + Escape Analysis

Objective:
- Reduce heap churn for short-lived objects created during actor method execution.

Approach:
- Add an arena allocator per actor tick or per message batch.
- Compiler detects values that do not escape the call.
- Allocate those values in arena and free en masse after processing.

Key Changes:
- Runtime arena allocator (simple bump allocator).
- Compiler escape analysis pass for objects created in methods.

Risks:
- Escape analysis mistakes cause use-after-free.
- Arena limits need to be bounded.

Success Metrics:
- Fewer allocations in allocator profiling.
- Lower GC/RC pressure in heavy message workloads.

## 4) Message/Args Hot Path Optimizations

Objective:
- Remove small but frequent overheads in actor send + pending.

Approach:
- Use small inline storage for args (SmallVec-like).
- Pool pending state objects.
- Avoid locking around sender where possible.

Key Changes:
- Replace `Vec<Value>` with small-buffered storage for common arg counts.
- Add a pending pool for reuse.
- Make sender clone-free and lock-free on fast path.

Risks:
- Complexity creep; keep changes narrow and measurable.

Success Metrics:
- Lower alloc counts in send/await.
- Reduced latency per message in microbenchmarks.

## 5) Batch Intrinsics to Reduce FFI Crossings

Objective:
- Lower runtime boundary crossings for common operations.

Approach:
- Identify hot paths in stdlib (list/map/string/numeric loops).
- Lower to runtime intrinsics that operate on slices/buffers.

Key Changes:
- Add a small set of runtime intrinsics (e.g., list map/filter, string concat).
- Compiler lowering rules for those operations.

Risks:
- Expands ABI surface area; keep intrinsics minimal.

Success Metrics:
- Fewer runtime calls per high-level operation.
- Throughput gains in data-heavy workloads.

## 6) Predictive Autoscaling (Opt-In)

Objective:
- Reduce p99 under load by adjusting pool sizes based on runtime signals.

Approach:
- Collect queue depth, enqueue latency, and CPU usage per pool.
- Implement a simple controller with min/max bounds.
- Default off; expose config to enable.

Key Changes:
- Metrics instrumentation for pools.
- Runtime control loop (low frequency, e.g., every 500ms).
- Config flags for min/max and target latency.

Risks:
- Overreaction or oscillation; keep conservative defaults.
- Additional complexity in runtime scheduling.

Success Metrics:
- Lower p99 latency in bursty traffic tests.
- Stable CPU usage without manual tuning.

## Suggested Order

1. Ownership transfer (move semantics)  
2. Single-producer await elision  
3. Arena allocation + escape analysis  
4. Message/args hot path optimizations  
5. Batch intrinsics  
6. Predictive autoscaling (opt-in)

## Validation Plan

- Add microbenchmarks for actor send/await loops.
- Track `METRIC_RC_INC/DEC`, enqueue latency, and mailbox high-water.
- Run stress tests with fixed and bursty loads.
 - New: `scripts/bench_pool.sh` runs `bench/pool_bench.wr` for baseline throughput.
