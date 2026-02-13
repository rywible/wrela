# Concurrency And Scheduling

This document captures scheduling and concurrency guarantees required for runtime correctness.

## Guarantees

1. Actor mailbox operations are thread-safe under documented runtime synchronization.
2. Atomic operations use explicit memory order semantics.
3. Scheduler fairness/starvation signals are measurable via runtime metrics and KPI gates.

## Enforcing Tests And Gates

- Runtime scheduler and mailbox behavior: `/Users/ryanwible/projects/wrela/runtime/src/kernel.rs`
- Perf KPI gates: `/Users/ryanwible/projects/wrela/compiler/bin/wrela.rs`
- Benchmark suites: `/Users/ryanwible/projects/wrela/benchmarks/`

## ABI Impact

No ABI break is required for these guarantees. Metrics and scheduler behavior remain runtime-internal.
