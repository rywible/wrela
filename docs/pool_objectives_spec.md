# Pool Objectives and Detach Pools (Spec)

Date: 2026-01-27
Status: Draft

## Summary

This document specifies Wrela's detached execution pools, objective-based scheduling, and the surface syntax that drives pool creation. The design goal is to make concurrency intent explicit while keeping the common case terse and readable.

Key points:
- Detached instances are always treated as pools (a pool of 1 is still a pool).
- Pool size is either a fixed literal or an automatic upper bound.
- Detached pools require objectives only when `await` exists in their transitive call graph.
- Objectives are preferences within fairness bounds, not promises.
- Power users can opt into a low-level pool object with explicit knobs.

## Goals

- Make performance intent explicit at the concurrency boundary (detach).
- Keep the common case short and readable.
- Provide clear, predictable defaults for routing, batching, and backpressure.
- Guarantee fairness: objectives can never starve the system.
- Provide an escape hatch for advanced pool configuration.

## Non-goals

- Expose all routing/batching/backpressure knobs in the surface syntax.
- Allow runtime expressions or variables for pool size.
- Provide a general-purpose configuration DSL in the main syntax.

## Terminology

- Pool: A group of detached instances that share routing, scheduling, and fairness.
- Objective: The scheduling preference for a pool.
- Fairness bounds: Global and per-pool caps that prevent starvation and monopolization.
- Surface syntax: The primary language syntax used by most users.
- Low-level pool object: An advanced escape hatch with explicit knobs.

## Surface Syntax

### Detach

Detached execution is expressed with the `detach` keyword.

Examples:

```
detach Whale() * 1
```

Detached instances are always pools. A single detached instance is a pool of size 1.

### Pool Size

Pool size is specified with `*` after the detached construction.

```
detach Whale() * 6
```

Pool size forms:
- `* <integer>`: fixed size (literal only).
- `* n`: automatic sizing with an upper bound chosen by the runtime.

`n` is a reserved keyword only in the `* n` position.

An explicit `*` size clause is required for all detached pools, including pools of size 1.
Use `* 1` to make a single-instance pool explicit.

Examples:

```
detach Whale() * 6
```

```
detach Whale() * n
```

Restrictions:
- Only integer literals are allowed for fixed sizes.
- No variables, expressions, or runtime values are allowed.
- Pool sizes greater than 1 require a class constructor target (class name or class call).

### Objective

Objective is expressed with the `optimize` keyword.

Inline form (per-detach):

```
detach Whale() * 1 optimize latency
```

Block form (lexical scope):

```
optimize latency:
    worker = detach Worker() * 1
    cache = detach Cache() * n
```

Objectives are a small, fixed set:
- `latency`
- `throughput`
- `conservation`
- `balance`

The objective is required for any `detach` that appears in a function whose transitive call graph contains `await`, unless an objective is already in scope.

### Combined Forms

```
detach Whale() * 6 optimize latency
```

```
detach Whale() * n optimize balance
```

### Ordering

The recommended order is:

```
detach <Type>(...) * <size> optimize <objective>
```

the canonical order above is required.

## Low-level Pool Object

Advanced users can construct a pool object explicitly:

```
pool = Pool.of(Whale, size=6, objective=latency)
worker = detach pool * 1
```

Notes:
- `Pool.of` is intended as a low-level escape hatch for advanced knobs.
- `size` accepts an integer literal or `n`.
- `objective` accepts the same objective identifiers as `optimize`.
- When `Pool.of` is used as the detach target, any `size` or `objective` arguments override the detach tail if both are present.
- The `*` clause is still required for explicitness, even when `Pool.of` is used.

## Objective Scoping

### Rule

Every `detach` must have a resolved objective. If no objective is specified inline, it is inherited from the nearest enclosing objective scope.

### Scopes

Objectives are allowed in the following scopes:
- Block scope via the `optimize <objective>:` form (lexical).
- The `run()` entrypoint function (via block form).

### Requirement

An objective is required only in scopes whose transitive call graph includes `await`.
If no `await` is reachable from a scope, `detach` does not require an objective in that scope.

### Single Objective per Scope

Only one `optimize` declaration is allowed per scope. Nested scopes may override the objective, but a given scope may not define multiple objectives.

### Examples

Objective at function scope (block form):

```
to handle(req):
    optimize balance:
        worker = detach Worker() * 1

Override in a nested scope:

```
to handle(req):
    optimize balance:
        worker = detach Worker() * 1

    optimize latency:
        fast = detach FastWorker() * n
```

### Compile-time Errors

If a `detach` is encountered and no objective can be resolved from enclosing scopes, compilation fails with a diagnostic that:
- explains the missing objective,
- suggests adding `optimize balance` in the nearest scope, and
- points to the relevant documentation.

## Objective Semantics

Objectives are preferences within fairness bounds. They do not guarantee outcomes and do not override system fairness.

### General Rules

- Objectives influence scheduling, batching, and scaling tactics.
- Objectives never override fairness caps.
- Objectives apply equally to pools of size 1 and larger pools.
- If a pool has fixed size, objective affects scheduling/batching but cannot change multiplicity.
- If a pool uses `* n`, objective may influence actual pool size within fairness bounds.

Auto pool sizing uses runtime bounds:
- The runtime selects a size within `[min, max]` fairness caps.
- Defaults are conservative and can be tuned via environment variables.

## Observability Hooks

The runtime exposes lightweight pool/actor observability hooks:
- `pool_size(handle)` returns the pool size (actors return 1).
- `pool_rr(handle)` returns the round-robin counter for a pool.
- `actor_mailbox_len(handle)` returns the current mailbox length (pools return the sum).
- `metrics_get(id)` reads runtime counters (e.g., drop counters).

These are intended for diagnostics and profiling, not control flow.

### Objective Behaviors (Default Intent)

These are default behaviors used to select sane runtime settings. They are not user-facing guarantees.

Latency:
- Favor low queueing delay.
- Scale up quickly within fairness caps.
- Minimize batching.
- Prefer faster wakeups.

Throughput:
- Favor high sustained throughput.
- Enable batching by default.
- Allow deeper queues.
- Scale up more aggressively when the system has headroom.

Conservation:
- Favor lower resource usage.
- Scale conservatively.
- Prefer stable utilization.
- Allow batching if it improves efficiency, but not at the cost of fairness.

Balance:
- Moderate defaults across all tactics.
- Conservative queue sizes.
- Modest batching.
- Stable scaling.

## Fairness Bounds (Non-negotiable)

All optimization occurs within global and per-pool fairness bounds. These bounds ensure the system remains healthy under load.

### Invariants

- No starvation: every runnable pool receives a minimum share of compute.
- No monopolization: no pool exceeds its maximum share.
- Borrowing is allowed but revocable: idle capacity can be used but is reclaimed under contention.
- Objectives only affect behavior within these bounds.

### Fairness Model (High-level)

Let total capacity be `C` (normalized compute).
Each pool `i` has:
- `min_share_i`
- `max_share_i`
- `weight_i`

Scheduling proceeds in phases:
1) Reserve all `min_share_i` for runnable pools.
2) Distribute remaining capacity by weighted fair sharing, clipped by `max_share_i`.
3) Reclaim unused capacity from idle pools and redistribute.

Objective affects how each pool uses its share (queueing, batching, scaling), but does not change fairness invariants.

## Pool Size Semantics

### Fixed Size

```
detach Whale() * 6 optimize latency
```

This creates a pool with exactly 6 instances. The objective affects scheduling and batching, but not pool size.

### Automatic Upper Bound

```
detach Whale() * n optimize latency
```

`* n` indicates that the runtime selects the actual pool size automatically. The objective influences this selection, but it is still bounded by fairness caps.

This is the recommended form when users do not know the right pool size.

## Spawn vs Detach

Wrela uses `detach` as the primary keyword for isolated, concurrent execution. The `spawn` keyword is deprecated and should be removed or treated as an alias if it exists.

Plain construction (non-detached) remains as:

```
Whale()
```

No objective is required for non-detached construction.

## Low-level Pool Object (Power-user Escape Hatch)

Advanced users can build a pool explicitly using a low-level pool object. This is not the default or recommended style.

### Current Shape (Implemented)

```
pool = Pool.of(Whale,
    size = 6,
    objective = latency,
    batch = 32,
    backpressure = queue(64)
)

workers = detach pool * 1
```

Backpressure forms:
- `backpressure = queue(<int>)` sets the mailbox capacity.
- `backpressure = drop` sets a zero enqueue timeout (drops when full).

Notes:
- This form is for advanced tuning and experimentation.
- The surface syntax desugars to this form with objective-driven defaults.

## Defaults and Desugaring

Surface syntax should desugar to low-level pool configuration using objective-based defaults.

Example:

```
detach Whale() * n optimize latency
```

Desugars to a pool with:
- automatic sizing
- latency-biased defaults
- round-robin routing
- bounded queues
- minimal or disabled batching

Exact default values are implementation-defined but must respect fairness bounds.

## Diagnostics

### Missing Objective

If no objective is in scope for a `detach`:
- Compilation fails with a clear error.
- Suggest adding `optimize balance` in the nearest scope.
- Provide a pointer to this spec.

### Invalid Pool Size

- If pool size is not an integer literal, compilation fails.
- If `* n` is used as a variable, compilation fails with a message that `n` is reserved in this position.

## Examples

### Minimal Detached Pool

```
to run():
    optimize balance:
        worker = detach Worker() * 1
```

### Auto-sized Pool

```
to run():
    optimize throughput:
        workers = detach Worker() * n
```

### Fixed-size Pool

```
to run():
    optimize latency:
        workers = detach Worker() * 8
```

### Objective Override in Nested Scope

```
to run():
    optimize balance:
        logger = detach Logger() * 1

    optimize latency:
        fast = detach FastWorker() * n
```

## Open Questions (for later revisions)

- Exact numerical defaults for queue sizes and batch limits.
- Exposure of routing and richer batching/backpressure in surface syntax.
- The compile-time algorithm for checking `await` reachability in the call graph.
- Tooling for showing resolved objectives and fairness bounds.
