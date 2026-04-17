# Collision GPU Batch Playbook

This playbook covers the GPU-batched collision path that shares resident scene data with the WGSL query runtime.

From the repo root, use `just perf-smoke` or `just perf-closure` for named repo workflows.
The raw `cargo run -p wrela -- ...` commands documented elsewhere remain the underlying
`wrela` invocations when you need collision-specific debugging.

## What The Production Path Looks Like

The collision production story is:

- broadphase narrows candidate sets first
- point/ray/overlap style collision questions batch work into WGSL dispatches
- resident scene payloads are reused instead of rebuilt per query
- CPU remains the certification/oracle path for semantics that are not fully proven on the GPU yet

Transition-style sweep and TOI work can still hybridize:

- GPU brackets and prunes the search space
- CPU performs certification where required by the contract

That is a feature, not a bug, as long as the hybrid behavior is explicit and tested.

## What To Watch In Reports

The important collision WGSL signals are:

- `wgsl_dispatch_count`
- `wgsl_dispatch_items`
- `wgsl_selected_workgroup_size`
- `wgsl_resident_shared_snapshot_artifacts`
- `cpu_certification_query_count`
- `candidate_reduction_effectiveness`

Healthy batching means dispatch counts stay low relative to work items and candidate reduction effectiveness stays materially above zero.

## Resident Data Rules

Collision should reuse the same resident-scene story as rendering:

- snapshot changes may upload
- steady-state repeated queries should not
- selection-sensitive payloads must not alias each other in the resident cache

If a collision perf run reports scene reupload in the hot path without a snapshot change, treat it as a regression in residency rather than “normal overhead.”

## CPU Fallback And Certification

Use CPU fallback deliberately:

- certification for sweep/TOI proofs
- unsupported GPU feature/configuration lanes
- oracle comparison in parity tests

Do not silently bounce static point/ray/overlap workloads back to CPU in the representative GPU batch lane.

## Debug Workflow

When debugging collision batching:

1. inspect the collision plan and broadphase behavior first
2. confirm the backend is really `wgsl`
3. look at candidate reduction and dispatch counts
4. check whether certification count is expected for the query family
5. compare against CPU oracle tests before changing thresholds or contracts
