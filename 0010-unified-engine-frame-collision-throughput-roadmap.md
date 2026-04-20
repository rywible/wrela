# RFC 0010: Unified Engine Frame, Collision Throughput, And Subsystem Performance Closure Roadmap

Status: Proposed post-Phase-56 engine-frame and collision-throughput roadmap after repo read, benchmark read, and supplemental WebGPU/WGSL research

Author: GPT-5.4 Pro

Created: 2026-04-19

Target: post-RFC-0009 `wrela` compiler, runtime, GPU runtime, presentation execution, collision execution, perf closure tooling, benchmark manifests, and future subsystem scheduling surface

## Summary

Wrela is close enough on rendering performance that the next problem is no longer just “make the renderer faster.”

The next problem is engine shape.

Right now, presentation has moved toward a real GPU-resident framegraph. Collision has some serious ingredients too: typed plans, broadphase artifacts, witness reuse, WGSL batch helper calls, resident-scene reuse hooks, and collision-specific observability. But collision is still not being exercised like an engine subsystem. The benchmark path still drives many workloads as repeated per-query `plan.execute(...)` calls, and the collision WGSL path still bounces through CPU-authoritative orchestration, immediate result readback, and per-query or per-sample dispatch boundaries.

That is why collision can lag behind rendering even though both now touch the same GPU runtime primitives.

Rendering got a framegraph.

Collision got helper dispatches.

Those are not the same thing.

The thesis of this RFC is:

**Wrela should move from presentation-led performance closure to engine-frame performance closure, where rendering, collision, state advance, query evaluation, and future subsystems are planned, timed, budgeted, and reported as one stable frame. Collision must become a batched resident subsystem inside that frame, not a CPU loop that occasionally calls WGSL.**

This RFC adds the missing layer:

1. a unified engine-frame report that joins presentation, collision, GPU runtime, readbacks, queue submissions, CPU certification, and future subsystem budgets
2. a realistic collision benchmark path that submits workload batches instead of repeated single-query plans
3. a collision batch API that groups queries by plan, scene snapshot, candidate set, and result shape
4. GPU-resident collision tickets that can be encoded into an engine frame without immediate hot-path readback
5. GPU-side candidate compaction and narrow-phase evaluation patterns for the collision path
6. a first `EngineFrameScheduler` that coordinates subsystem work and makes room for future subsystems without turning every subsystem into a bespoke benchmark lane
7. closure gates that fail when the engine is only fast because subsystems were measured separately

The design stance remains conservative:

- CPU remains the semantic oracle.
- Collision certification remains explicit.
- No subgroup-dependent correctness path.
- No hidden readback in the hot path.
- No claiming engine closure from independent renderer and collision timings that were not collected under the same frame protocol.
- No large rewrite of presentation or collision before the reporting layer can prove what is actually happening.

## Relationship To Earlier RFCs And Repo Vision

This roadmap builds directly on:

- `language/spec/rfcs/0001-field-game-language.md`
- `language/spec/rfcs/0002-field-engine-implementation-roadmap.md`
- `language/spec/rfcs/0003-phase-9-5-semantic-convergence-plan.md`
- `0004-question-families-query-contracts-roadmap.md`
- `0005-realtime-presentation-view-plans-frame-contracts-roadmap.md`
- `0006-certified-world-snapshots-temporal-semantics-artifact-runtime-query-program-spine-roadmap.md`
- `0007-shared-acceleration-spine-1080p120-rendering-collision-roadmap.md`
- `0008-gpu-resident-framegraph-1080p120-rendering-collision-roadmap.md`
- `0009-solo-developer-throughput-maintenance-closure-roadmap.md`

RFC 0005 made presentation plans explicit.
RFC 0006 made snapshots, artifacts, and query families explicit.
RFC 0007 made shared acceleration and collision-aware artifacts explicit.
RFC 0008 pushed the GPU-resident framegraph and collision throughput closure direction.
RFC 0009 made local shipping and maintenance surfaces more disciplined.

This RFC is the next layer of runtime truth.

It says: if Wrela is becoming a real engine, the performance closure story cannot stay renderer-first. A real engine has to schedule multiple subsystems, protect global frame budgets, and report the truth when one subsystem wins by starving another.

In other words:

- RFC 0008 said the frame must become GPU-authoritative.
- RFC 0010 says the engine frame must become subsystem-authoritative.

## Research Grounding

This plan is shaped by official WebGPU / `wgpu` constraints and common GPU data-parallel patterns.

The important grounding points are:

1. **Immediate mapped-buffer readback remains hostile to a hot path.**
   `wgpu::Buffer` documentation states that while a buffer is mapped, GPU and CPU access are exclusive. That means `map_async` plus device polling is a synchronization boundary, not an innocent result-copy helper.
   Reference: https://docs.rs/wgpu/latest/wgpu/struct.Buffer.html

2. **Many small uploads should reuse staging memory.**
   `wgpu::util::StagingBelt` exists to efficiently perform many buffer writes by sharing and reusing temporary staging buffers. Wrela already has `FrameUploadArena`; the next step is making collision and engine-frame submission actually use that pattern at the subsystem boundary.
   Reference: https://docs.rs/wgpu/latest/wgpu/util/struct.StagingBelt.html

3. **GPU timing should be pass-scoped and optional-feature aware.**
   `wgpu::ComputePassTimestampWrites` records timestamps at the beginning and/or end of a compute pass. `TIMESTAMP_QUERY` is an optional feature, so closure reports must record whether it was supported instead of assuming it always exists.
   References: https://docs.rs/wgpu/latest/wgpu/struct.ComputePassTimestampWrites.html and https://docs.rs/wgpu/latest/wgpu/struct.Features.html

4. **Device limits should be requested deliberately.**
   `wgpu::Limits` recommends starting with restrictive limits and manually increasing only what is needed. Engine-frame work should not request maximal limits just because one collision path wants a larger buffer.
   Reference: https://docs.rs/wgpu/latest/wgpu/struct.Limits.html

5. **Indirect dispatch is a useful future optimization, not the correctness path.**
   WebGPU exposes `dispatchWorkgroupsIndirect`, where the dispatch dimensions come from a GPU buffer. That is a good fit after queue compaction exists, but the first closure path should work with direct dispatch and explicit counts.
   Reference: https://developer.mozilla.org/en-US/docs/Web/API/GPUComputePassEncoder/dispatchWorkgroupsIndirect

6. **Broad-phase collision maps well to GPU spatial subdivision and flat candidate lists.**
   GPU Gems 3 presents a CUDA broad-phase collision approach based on spatial subdivision and reports order-of-magnitude speedup against a CPU implementation. Wrela should not copy CUDA details blindly, but the core pattern matters: generate candidate pairs in parallel, compact them, then run narrow phase over the compacted stream.
   Reference: https://developer.nvidia.com/gpugems/gpugems3/part-v-physics-simulation/chapter-32-broad-phase-collision-detection-cuda

7. **Prefix sum / scan is the standard primitive behind stream compaction.**
   GPU Gems 3 Chapter 39 describes parallel prefix sum as a core GPU primitive and explicitly connects scan to stream compaction. Collision candidate compaction should be designed around count, prefix, scatter, evaluate phases.
   Reference: https://developer.nvidia.com/gpugems/gpugems3/part-vi-gpu-computing/chapter-39-parallel-prefix-sum-scan-cuda

8. **`f16` and subgroup paths must remain optional.**
   `SHADER_F16` can reduce bandwidth for selected storage records, and subgroup operations can improve reductions/compaction on some native backends, but both must remain feature-gated and parity-tested. The correctness path must work without them.
   References: https://docs.rs/wgpu/latest/wgpu/struct.Features.html and https://docs.rs/wgpu/latest/wgpu/struct.FeaturesWGPU.html

## Current Repo Read

The repo already has most of the raw material needed for this roadmap.

### What is already strong

1. **The GPU runtime now has real reusable primitives.**
   `compiler/gpu_runtime` contains `GpuRuntimeMetrics`, `GpuPassProfiler`, `GpuResidentSceneCache`, `GpuResourceCache`, `FrameUploadArena`, a buffer pool, explicit bind-group roles, readback tickets, and layout identity helpers. That is the correct substrate for engine-frame scheduling.

2. **Presentation has a real framegraph surface.**
   `compiler/presentation_exec/framegraph.rs` owns a `PresentationFramegraph` with a GPU context, command encoder, timestamp profiler, attachment arena, readback tickets, and queue submission count. This is the closest thing in the repo to a real frame-level execution model.

3. **Presentation reports already understand GPU-runtime pressure.**
   `PresentationFrameCostReport` includes `GpuRuntimeMetrics`, attachment bytes, pass costs, execution bound classification, quality state, active acceleration artifacts, and bottleneck pass naming. This is a good reporting model to generalize.

4. **Collision has typed contracts and execution traces.**
   `compiler/collision_contract`, `compiler/collision_plan`, and `compiler/collision_exec` already separate public result semantics, plan shape, artifact reuse, broadphase candidates, witness reuse, fallback, and WGSL metrics.

5. **Collision already has WGSL batch helper functions.**
   `compiler/collision_exec/gpu.rs` exposes batched point distance, point normal, and ray trace dispatch helpers. Those helpers already use `GpuQueryDispatcher`, shared snapshots, candidate spans, and `QueryExecutionObservability`.

6. **The perf engine already joins presentation and collision evidence by scenario identity.**
   `build_whole_frame_benchmark_reports(...)` joins presentation and collision reports using scenario ids instead of positional ordering. That is the right instinct.

7. **The benchmark manifests already define whole-frame scenarios with both presentation and collision sections.**
   `benchmarks/whole_frame/1080p120_closure.toml` includes scenarios that pair presentation views with collision workloads. This is the correct place to evolve from joined reports into engine-frame reports.

### Where the repo is still structurally misleading

1. **Collision WGSL execution still resolves through CPU-authoritative orchestration.**
   `compiler/collision_exec/gpu.rs::execute(...)` delegates directly to `collision_exec::cpu::execute(...)`. Inside the CPU path, WGSL is used as helper calls for selected query shapes, but the execution model remains a CPU pass loop.

2. **The benchmark path exercises collision like a command loop, not like a subsystem.**
   In `compiler/bin/wrela/perf_engine/collection.rs`, collision workloads such as point occupancy, dense ray casts, overlaps, repeated sweeps, and TOI reuse loop over `scenario.ops` and call `plan.execute(...)` or `execute_with_store(...)` per query. Under WGSL, that can mean repeated dispatch/upload/readback/certification boundaries. That is not representative of how an engine should schedule thousands of collision checks in a frame.

3. **The current collision batch helpers are too low-level for the benchmark/reporting layer.**
   `execute_batched_point_distance_queries_with_candidates(...)` and friends are useful, but they are not yet wrapped in a `CollisionWorkloadBatch` API that knows about plan identity, workload identity, snapshot identity, query count, chunking, candidate grouping, certification, and report assembly.

4. **Collision reporting still gates mostly on non-regression against an old CPU oracle baseline.**
   `build_collision_closure_status(...)` validates sampled runs against `collision_perf.phase40_cpu_oracle` and backend consistency. It does not yet fail on too many WGSL dispatches, too much readback, too much CPU certification, too little batching, or too much queue submission pressure.

5. **Whole-frame reports are additive, not scheduled.**
   `WholeFrameBenchmarkReport` currently stores presentation runtime, collision runtime, total runtime, FPS, bottleneck pass, collision fallback rate, and witness reuse rate. That is useful, but it does not tell whether presentation and collision were encoded into the same GPU frame, whether the scheduler serialized avoidable work, whether collision readback stalled presentation, or whether a future subsystem could fit.

6. **Presentation is the only subsystem with a framegraph.**
   The repo has `PresentationFramegraph`, not `EngineFramegraph`. The name matters because it reflects current ownership: rendering owns the frame. Collision and future subsystems need a shared frame contract.

7. **Engine-level resource contention is not visible.**
   The reporting layer can say rendering is fast and collision is slow, but it cannot yet say: “rendering consumed one queue submit, collision consumed N submits, readbacks forced M device polls, CPU certification took X ms, and the frame has no budget left for state/AI/audio.”

## Why This Comes Before Adding More Subsystems

Adding more subsystems before this RFC would make the repo harder to reason about.

The engine would accumulate separate command paths, separate perf lanes, separate reporting vocabulary, and separate emergency optimizations. That is how an engine becomes fast in demos and unstable in real workloads.

This RFC creates the common frame substrate first:

- one frame identity
- one snapshot identity
- one subsystem budget model
- one GPU runtime metrics envelope
- one readback policy
- one queue submission budget
- one closure report
- one place where future subsystems attach

Then collision throughput work lands into that substrate instead of becoming another special path.

## Goals

1. Make performance reporting engine-frame truthful, not just renderer truthful.
2. Make collision benchmarks exercise batched subsystem workloads instead of repeated single-query command calls.
3. Reduce collision WGSL dispatch/readback pressure from O(query count) toward O(batch groups).
4. Keep CPU oracle and CPU certification visible, budgeted, and explicitly charged.
5. Add a reusable engine-frame scheduler surface that can host future subsystems.
6. Make queue submits, GPU readbacks, scene uploads, CPU certification queries, and per-subsystem timings first-class closure gates.
7. Preserve current semantics while changing the execution shape.
8. Keep the work executable in phases by a junior engineer or agent contributor.

## Non-Goals

1. This RFC does not implement rigid-body physics, constraint solving, audio, AI, networking, or gameplay ECS.
2. This RFC does not remove CPU collision execution.
3. This RFC does not require subgroup operations, indirect dispatch, or `f16` for correctness.
4. This RFC does not promise all collision answers become GPU-final. Exact/certified results still flow through the collision contract.
5. This RFC does not replace presentation’s existing framegraph in the first phase. It wraps and generalizes it.
6. This RFC does not require native-specific GPU APIs outside the current `wgpu`/WGSL portability stance.
7. This RFC does not treat benchmark wins as valid unless closure reporting proves reduced dispatch/readback/subsystem pressure.

## Design Rules

1. **The engine frame owns scheduling.**
   Presentation, collision, and future subsystems contribute work to an engine frame. They do not each invent their own global timing story.

2. **CPU oracle remains semantic authority.**
   GPU collision can filter, batch, estimate, and propose. Exact guarantees remain certified according to the collision contract.

3. **Hot-path readback is opt-in and named.**
   Any readback during closure mode must have a `ReadbackReason`, subsystem owner, byte count, and closure-policy explanation. Silent readback is a bug.

4. **Batches are semantic, not just arrays.**
   A collision batch carries plan id, contract id, snapshot, capture, domain, workload, candidate grouping, and certification policy. A `Vec<Point>` is not enough.

5. **GPU compaction must have a portable fallback.**
   The first compaction path must work without subgroups and without indirect dispatch. Optional fast paths can be layered later.

6. **Reporting must fail closed.**
   Missing subsystem evidence, unknown runtime ownership, or mismatched backend reports must become violations in closure mode.

7. **Future subsystem slots are real but minimal.**
   Add enough engine-frame structure to host state advance, collision, presentation, and later subsystems. Do not design a massive plugin system yet.

8. **Every phase leaves the repo shippable.**
   Each phase adds tests, keeps old command paths working, and documents whether a path is legacy, debug, oracle, or closure.

## Key Architectural Definitions

### Engine Frame

One scheduled unit of engine work for a world snapshot and presentation frame/tick.

It may contain presentation passes, collision batches, state-advance work, query programs, and future subsystem work.

### Subsystem

A bounded runtime participant inside an engine frame.

Initial subsystem kinds:

- `state_advance`
- `presentation`
- `collision`
- `query`
- `gpu_runtime`

Future subsystem kinds can include `ai`, `audio`, `particles`, `network`, and `tools`, but those should not be implemented by this RFC.

### Engine Frame Plan

A typed plan describing subsystem work, dependencies, budgets, readback policy, and GPU submission strategy for one engine frame.

### Engine Frame Report

The canonical per-frame execution evidence object.

It records authoritative frame-throughput timing, CPU and GPU critical-path timing, wait/sync buckets, subsystem timings, queue submits, readbacks, scene uploads, CPU certification, GPU timestamps, active degradations, reserve accounting, and violations.

### Engine Frame Benchmark Report

The aggregated benchmark-facing report built from one or more engine-frame execution records.

It is the object that closure mode consumes to compute median/p95 frame throughput, subsystem distributions, queue-submit/readback pressure, and remaining reserve budget.

### Collision Workload Batch

A semantically typed batch of collision queries for one plan/workload/snapshot/domain grouping.

It is not allowed to hide per-query fallback or certification. It should make batching visible and measurable.

### Collision Candidate Group

A group of collision items that share the same candidate set or candidate-generation strategy.

Grouping by candidate set allows the GPU path to avoid duplicating candidate spans and lets reports show whether broadphase is helping.

### GPU Query Ticket

A handle returned after GPU work has been encoded.

It refers to GPU-resident result buffers, metrics buffers, and decode metadata. It does not imply immediate CPU readback.

### Readback Policy

A frame-level rule for which results can return to CPU during a closure run.

Initial policy values:

- `None`
- `MetricsOnly`
- `DebugExport`
- `CpuOracleParity`
- `LegacyImmediate`

Only `MetricsOnly` should be allowed in the main closure hot path once the engine frame path is active.

## Measurement Model

This RFC adopts the production-engine convention for what "frame time" means.

The authoritative performance number for closure and FPS is **engine-frame wall-clock throughput time**, not `cpu_time + gpu_time`.

That means:

- CPU and GPU work may overlap.
- worker-thread CPU work may overlap.
- subsystem timings are explanatory slices, not additive proof of delivered framerate.
- displayed FPS is derived from delivered frame throughput, not from summed subsystem costs.

The engine-frame measurement vocabulary is therefore:

1. **Frame wall time** — the authoritative throughput number for closure and FPS. This is the time between completed engine frames under the benchmark protocol.
2. **CPU critical-path time** — the CPU-side path that actually gates the frame. This is not "sum of all CPU thread work."
3. **GPU critical-path time** — the GPU-side path that actually gates the frame when timestamp support exists.
4. **Wait/stall time** — explicit time lost to present wait, queue back-pressure, readback wait, device polling, fence wait, or similar synchronization.
5. **Subsystem work time** — presentation, collision, state advance, and future subsystem slices used to explain the bottleneck, not to define the delivered FPS on their own.
6. **Frame latency** — important, but distinct from throughput. This RFC tracks throughput closure first.

In practical terms:

- Closure budgets in this RFC apply first to frame wall time.
- CPU and GPU critical-path numbers are required observability because they explain why frame wall time missed.
- Summed subsystem CPU time and summed subsystem GPU time must never be treated as the authoritative frame-time gate once subsystems share a real frame.
- If the engine reports `FPS`, it should derive that value from frame wall time.

This is the main correction to the earlier additive whole-frame shape in the repo. Rendering time plus collision time is useful as a compatibility bridge, but it is not the end-state truth model.

## End State Of This Roadmap

At the end of this RFC, a canonical 1080p120 run should produce an engine-frame report that can say, plainly:

- steady-state frame wall-time median/p95
- steady-state FPS derived from frame wall time
- CPU critical-path median/p95
- GPU critical-path median/p95 when timestamp support exists
- explicit present/readback/fence/queue-wait buckets when they are observed
- presentation median/p95 and bottleneck pass
- collision median/p95 and worst workload
- queue submit count per frame
- hot-path readback bytes per frame
- scene reupload bytes per frame
- collision dispatch count per frame
- collision items per dispatch
- collision batch utilization
- collision CPU certification query count
- fallback and witness reuse rates
- active acceleration/residency artifacts
- quality degradations, if any
- whether any future-subsystem reserve budget remains

The report should be honest when collision is the bottleneck.
It should also be honest when rendering is fast only because collision was measured outside the frame.

## Project-Level Acceptance Criteria

This RFC is complete when all of the following are true:

1. A new engine-frame report type exists and is emitted by the 1080p120 closure path.
2. The whole-frame closure path can run presentation and collision through one engine-frame protocol.
3. The RFC has one explicit authoritative frame-time definition: frame wall time / throughput time, with CPU and GPU critical-path timings reported separately as explanatory evidence.
4. Collision benchmarks no longer represent WGSL closure by calling `plan.execute(...)` once per query for high-volume workloads.
5. Collision report data includes dispatch count, dispatch items, batch utilization, hot-path readback bytes, scene reupload bytes, CPU certification count, and queue submit count.
6. Closure fails if collision has O(query count) dispatches in the WGSL resident profile.
7. Closure fails if hot-path collision readback is present outside named metrics/debug/oracle policy.
8. GPU candidate compaction exists for at least one collision workload class, with CPU parity tests.
9. The scheduler has reserved budget accounting for at least one future subsystem without representing that reserve as fake executed work.
10. Old CPU oracle paths still pass.
11. The RFC explicitly updates the repo workflow surface if it adds `just test-engine-frame`, `just perf-engine-closure`, or changes the meaning of `just perf-closure` / `just ship`.
12. `just test`, `just test-engine-frame` if added, `just perf-smoke`, and the relevant 1080p120 perf lane run successfully or have a documented machine limitation.

## Phase Overview

- **Phase 57: Engine-Frame Reporting And Subsystem Budget Model** — Add the unified reporting and budget surface before changing execution behavior.
- **Phase 58: Realistic Collision Workload Batching** — Make collision benchmarks and execution APIs batch real workloads.
- **Phase 59: GPU-Resident Collision Tickets And Readback Discipline** — Split collision GPU encode from immediate result collection.
- **Phase 60: GPU Candidate Compaction And Narrow-Phase Throughput** — Add GPU-side count/prefix/scatter/evaluate patterns for collision.
- **Phase 61: Engine Frame Scheduler And Budget Governor** — Introduce the first scheduler that coordinates presentation, collision, and future subsystem slots.
- **Phase 62: Closure Gates, Regression Evidence, And Legacy Cleanup** — Make the new engine-frame path canonical and fail closed.

---

# Phase 57: Engine-Frame Reporting And Subsystem Budget Model

## Goal

Create one performance-reporting model that can describe a whole engine frame across presentation, collision, GPU runtime, state advance, and future subsystems.

## Why this phase exists

Do not optimize first.

The repo already has enough separate metrics to create false confidence. Before changing collision execution shape, make the reporting layer able to say what changed.

This phase should be mostly additive. It should not rewrite presentation or collision execution.

### Workstream A: Engine-frame model

#### Task 57A1 — Add `compiler/engine_frame/mod.rs`

**Description**

Add a new module for engine-frame plans, subsystem reports, budgets, and closure-facing summaries.

**Files**

- new `compiler/engine_frame/mod.rs`
- `compiler/lib.rs`
- any module docs touched by the new public module

**Implementation notes**

Start with pure data types. Do not add scheduling behavior yet.

Code sketch:

```rust
use crate::gpu_runtime::GpuRuntimeMetrics;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineSubsystemKind {
    StateAdvance,
    Presentation,
    Collision,
    Query,
    GpuRuntime,
    FutureReserve(SmolStr),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineSubsystemBudget {
    pub median_ms: f32,
    pub p95_ms: f32,
    pub max_queue_submits: u32,
    pub max_hot_path_readback_bytes: u64,
    pub max_scene_reupload_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineSubsystemReport {
    pub kind: EngineSubsystemKind,
    pub label: SmolStr,
    pub work_items: u64,
    pub cpu_critical_path_micros: u128,
    pub gpu_critical_path_micros: Option<u128>,
    pub queue_submit_count: u32,
    pub hot_path_readback_bytes: u64,
    pub scene_reupload_bytes: u64,
    pub wait_time_micros: u128,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineFrameReport {
    pub scenario_id: String,
    pub frame_index: u32,
    pub frame_wall_time_micros: u128,
    pub cpu_critical_path_micros: u128,
    pub gpu_critical_path_micros: Option<u128>,
    pub present_wait_micros: u128,
    pub gpu_wait_micros: u128,
    pub readback_wait_micros: u128,
    pub steady_state_fps: f64,
    pub gpu_runtime: GpuRuntimeMetrics,
    pub subsystems: Vec<EngineSubsystemReport>,
    pub future_subsystem_reserve_micros: u128,
    pub future_subsystem_reserve_exhausted: bool,
    pub active_degradations: Vec<String>,
    pub violations: Vec<String>,
}
```

Keep this module boring. The value is the shared vocabulary.

The important constraint is semantic, not cosmetic:

- `frame_wall_time_micros` is the authoritative throughput value.
- CPU/GPU critical-path values explain the miss.
- subsystem slices must not be summed and presented as the delivered frame time.
- future reserve stays a separate accounting field, not a fake observed subsystem sample.

**Acceptance criteria**

- `compiler/engine_frame/mod.rs` exists and is exported from `compiler/lib.rs`.
- The module compiles without creating dependencies from low-level runtime modules back into CLI code.
- Types derive `Serialize`, `Deserialize`, `Debug`, `Clone`, and `PartialEq` where useful for reports/tests.
- Unit tests verify JSON round-trip for `EngineFrameReport`.

#### Task 57A2 — Add engine-frame budgets to `perf_target`

**Description**

Extend `PerfClosureProfile` with an engine-frame budget envelope. The existing presentation and collision budgets remain, but the closure profile also gets a total frame budget and per-subsystem reserve budgets.

**Files**

- `compiler/perf_target/mod.rs`
- `compiler/bin/wrela/perf_engine/tests.rs`

**Implementation notes**

Add a budget map or explicit fields. Prefer explicit fields first to keep the profile legible.

Code sketch:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PerfClosureEngineFrameBudget {
    pub frame_wall_time_median_ms: f32,
    pub frame_wall_time_p95_ms: f32,
    pub presentation_median_ms: f32,
    pub collision_median_ms: f32,
    pub state_advance_median_ms: f32,
    pub future_subsystem_reserve_ms: f32,
    pub max_queue_submit_count_per_frame: u32,
    pub max_hot_path_readback_bytes_per_frame: u64,
}
```

Initial canonical values should be conservative and easy to revisit:

```rust
engine_frame_budget: PerfClosureEngineFrameBudget {
    frame_wall_time_median_ms: 8.33,
    frame_wall_time_p95_ms: 8.33,
    presentation_median_ms: 5.50,
    collision_median_ms: 1.50,
    state_advance_median_ms: 0.25,
    future_subsystem_reserve_ms: 1.00,
    max_queue_submit_count_per_frame: 1,
    max_hot_path_readback_bytes_per_frame: 0,
}
```

The exact numbers can be adjusted, but the key is that collision gets an affirmative budget instead of only a non-regression baseline.

The first engine-frame budget should gate wall time, queue submits, and hot-path readback. CPU/GPU critical-path metrics should be required in the report, but they do not need to become hard closure gates in the very first additive assembly phase.

**Acceptance criteria**

- The canonical 1080p120 profile includes an engine-frame budget.
- Profile validation rejects non-positive total budgets.
- Profile validation rejects subsystem budgets whose sum is obviously larger than the total frame budget unless a documented oversubscription field is explicitly added.
- Tests cover canonical profile defaults and validation errors.

### Workstream B: Report assembly

#### Task 57B1 — Add `EngineFrameBenchmarkReport` to the perf-reporting model

**Description**

Add a user-visible benchmark report that joins presentation and collision evidence as an engine frame, without yet changing how the work is executed.

**Files**

- `compiler/bin/wrela/commands/test_eval_perf.rs`
- `compiler/bin/wrela/perf_engine/collection.rs`
- `compiler/bin/wrela/perf_engine/closure.rs`
- `compiler/bin/wrela/perf_engine/tests.rs`

**Implementation notes**

The first implementation can be an assembly layer over existing reports.

Code sketch:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct EngineFrameBenchmarkReport {
    pub(crate) scenario_id: PerfScenarioId,
    pub(crate) test_name: String,
    pub(crate) frame_count: u32,
    pub(crate) frame_wall_time_ns: u128,
    pub(crate) cpu_critical_path_ns: u128,
    pub(crate) gpu_critical_path_ns: Option<u128>,
    pub(crate) present_wait_ns: u128,
    pub(crate) readback_wait_ns: u128,
    pub(crate) steady_state_fps: f64,
    pub(crate) presentation_runtime_ns: u128,
    pub(crate) collision_runtime_ns: u128,
    pub(crate) state_advance_runtime_ns: u128,
    pub(crate) future_subsystem_reserve_ns: u128,
    pub(crate) queue_submit_count: u32,
    pub(crate) hot_path_readback_bytes: u64,
    pub(crate) scene_reupload_bytes: u64,
    pub(crate) subsystem_reports: Vec<wrela::engine_frame::EngineSubsystemReport>,
}
```

Build it from existing `PresentationBenchmarkReport`, `CollisionBenchmarkReport`, and `WholeFrameBenchmarkReport` records.

In this first assembly phase, some values may still be compatibility approximations. That is acceptable as long as the report names them honestly and treats `frame_wall_time_ns` as the future authoritative field to converge on.

Keep the old `WholeFrameBenchmarkReport` for compatibility in this phase.

**Acceptance criteria**

- Engine-frame reports are emitted for whole-frame scenarios that contain both presentation and collision specs.
- Reports include presentation and collision subsystem entries.
- Reports preserve scenario identity and fail if presentation/collision scenario ids do not match.
- Tests cover report assembly from one presentation report and one collision report.

#### Task 57B2 — Add engine-frame closure status without changing verdict policy yet

**Description**

Add an engine-frame lane status to `PerfClosureReport`, but keep the existing frame/collision verdict rules until Phase 62.

**Files**

- `compiler/perf_target/mod.rs`
- `compiler/bin/wrela/perf_engine/closure.rs`
- `compiler/bin/wrela/perf_engine/tests.rs`

**Implementation notes**

Add:

```rust
pub engine_frame: PerfClosureLaneStatusReport,
```

or add a new dedicated report if `PerfClosureLaneStatusReport` is too presentation/collision-specific.

A dedicated report is cleaner:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PerfClosureEngineFrameStatusReport {
    pub status: PerfClosureLaneStatus,
    pub frame_wall_time_median_ms: Option<f32>,
    pub frame_wall_time_p95_ms: Option<f32>,
    pub cpu_critical_path_median_ms: Option<f32>,
    pub gpu_critical_path_median_ms: Option<f32>,
    pub presentation_median_ms: Option<f32>,
    pub collision_median_ms: Option<f32>,
    pub queue_submit_count: Option<u32>,
    pub hot_path_readback_bytes: Option<u64>,
    pub scene_reupload_bytes: Option<u64>,
    pub notes: Vec<String>,
}
```

In this phase, set status to `Sampled` or `NotSampled`; do not fail new closure gates yet.

The wording matters here: this status report should describe what the engine frame measured, not imply that presentation runtime plus collision runtime is the final frame-time truth forever.

**Acceptance criteria**

- Closure JSON includes an `engine_frame` section.
- Engine-frame status is `Sampled` when whole-frame engine reports exist.
- Existing frame and collision closure tests continue to pass.
- New tests prove missing engine-frame reports do not break non-whole-frame perf runs.

### Workstream C: Developer workflow

#### Task 57C1 — Add a focused engine-frame test lane

**Description**

Add a targeted test lane for the new reporting surface.

**Files**

- `justfile`
- `AGENTS.md`
- optional new `compiler/tests/engine_frame.rs`

**Implementation notes**

Add a narrow lane before adding heavier execution tests.

If this RFC adds a new `just` lane, it must also update the explicit repo workflow language. Do not silently expand the workflow surface in code while leaving `AGENTS.md`, `benchmarks/README.md`, or the canonical lane list behind.

Code sketch:

```just
engine-frame-tests := "cargo test -p wrela --test engine_frame"

test-engine-frame:
    {{engine-frame-tests}}
```

If no integration test file is needed yet, make the lane run the relevant perf-engine tests by name.

**Acceptance criteria**

- `just test-engine-frame` exists.
- `AGENTS.md` mentions the lane as the focused lane for engine-frame/reporting changes.
- The lane runs in under the normal focused-test budget on a warm workspace.

## Phase 57 exit criteria

- Engine-frame data types exist.
- Canonical closure profile has engine-frame budgets.
- Whole-frame perf reports can emit engine-frame report records.
- Closure JSON includes engine-frame status.
- No execution behavior has been changed yet.

---

# Phase 58: Realistic Collision Workload Batching

## Goal

Make collision benchmarks and execution APIs represent collision as subsystem batches, not repeated single-query commands.

## Why this phase exists

The current collision WGSL helper functions can batch items, but the higher-level benchmark path still loops over `scenario.ops` and calls a full collision plan per query. That destroys dispatch amortization and makes collision look worse than it should in a real engine.

This phase makes the benchmark path tell the truth.

### Workstream A: Batch contract

#### Task 58A1 — Add `CollisionWorkloadBatch` and typed batch items

**Description**

Define a public collision batch shape that can represent the existing benchmark workloads without erasing the semantic identity of what is being executed.

**Files**

- `compiler/collision_plan/mod.rs` or new `compiler/collision_plan/batch.rs`
- `compiler/collision_exec/mod.rs`
- `compiler/tests/collision_plan.rs`

**Implementation notes**

Keep the first batch type small and explicit.

Code sketch:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollisionCertificationPolicy {
    MetricsOnly,
    CpuOracleParity,
    ExactRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollisionCandidateGroupingPolicy {
    PerItem,
    SharedCandidateDigest,
    SharedBroadphaseRegion,
}

#[derive(Debug, Clone)]
pub enum CollisionBatchItem {
    PointOccupancy { point: [f32; 3] },
    RayCast { ray: crate::collision_contract::CollisionRayInput },
    SphereOverlap { center: [f32; 3], radius: f32 },
    SphereSweep {
        transition: crate::collision_contract::CollisionSnapshotTransitionInput,
        sweep: crate::collision_contract::CollisionSphereSweepInput,
    },
    SphereTimeOfImpact {
        transition: crate::collision_contract::CollisionSnapshotTransitionInput,
        sweep: crate::collision_contract::CollisionSphereSweepInput,
    },
}

#[derive(Debug, Clone)]
pub struct CollisionWorkloadBatch {
    pub name: smol_str::SmolStr,
    pub workload_id: smol_str::SmolStr,
    pub scenario_id: smol_str::SmolStr,
    pub plan: CollisionPlan,
    pub contract_id: smol_str::SmolStr,
    pub snapshot_id: smol_str::SmolStr,
    pub capture: crate::kernel::KernelValue,
    pub domain: crate::kernel::KernelValue,
    pub candidate_grouping: CollisionCandidateGroupingPolicy,
    pub certification_policy: CollisionCertificationPolicy,
    pub items: Vec<CollisionBatchItem>,
    pub chunk_size: usize,
}
```

Add helpers:

```rust
impl CollisionWorkloadBatch {
    pub fn is_empty(&self) -> bool { self.items.is_empty() }
    pub fn query_count(&self) -> usize { self.items.len() }
    pub fn chunks(&self) -> impl Iterator<Item = &[CollisionBatchItem]> { ... }
}
```

Do not make this generic yet. Junior engineers should be able to inspect and debug the batch contents.

The important tightening here is not "more metadata for its own sake." It is making sure the batch object itself says:

- what workload this is
- what plan/contract it belongs to
- which snapshot/capture it targets
- how candidates are grouped
- what certification promise the caller expects

A `Vec<Point>` is not enough for closure-grade reporting.

**Acceptance criteria**

- Existing collision plans can be wrapped in a `CollisionWorkloadBatch`.
- Batch validation rejects mixed item kinds for plans that cannot support them.
- Batch validation rejects missing workload/contract/snapshot identity.
- Unit tests cover point, ray, overlap, sweep, and TOI batch construction.

#### Task 58A2 — Add `CollisionBatchExecutionReport`

**Description**

Create a report object that records batching behavior directly.

**Files**

- `compiler/collision_plan/mod.rs`
- `compiler/bin/wrela/commands/test_eval_perf.rs`
- `compiler/bin/wrela/perf_engine/collection.rs`

**Implementation notes**

Code sketch:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CollisionBatchExecutionReport {
    pub workload: String,
    pub plan_name: String,
    pub contract_id: String,
    pub query_count: u64,
    pub batch_count: u32,
    pub dispatch_count: u32,
    pub dispatch_items: u32,
    pub average_items_per_dispatch: f32,
    pub hot_path_readback_bytes: u64,
    pub queue_submit_count: u32,
    pub cpu_certification_query_count: u32,
    pub fallback_count: u32,
    pub witness_reuse_rate: f64,
}
```

Add it alongside the existing `CollisionBenchmarkExecutionReport`; do not delete the old fields yet.

**Acceptance criteria**

- Collision benchmark JSON includes batch execution metadata.
- Existing report consumers keep working.
- Tests assert average items per dispatch is computed safely when dispatch count is zero.

### Workstream B: Batch execution on CPU and WGSL

#### Task 58B1 — Add `execute_batch_cpu` as the semantic baseline

**Description**

Implement a CPU batch executor that initially loops over items internally but reports batch-level metrics.

**Files**

- `compiler/collision_exec/cpu.rs`
- `compiler/collision_exec/mod.rs`
- `compiler/tests/collision_exec/cpu.rs`

**Implementation notes**

Do not optimize the CPU path first. The point is to establish batch semantics and parity.

Code sketch:

```rust
pub fn execute_batch_cpu(
    batch: &CollisionWorkloadBatch,
    ctx: &QueryExecContext,
    store: Option<&mut CollisionArtifactStore>,
) -> Result<CollisionBatchResult, CollisionExecError> {
    let mut results = Vec::with_capacity(batch.items.len());
    let mut report = CollisionBatchExecutionReport::new(batch);

    for item in &batch.items {
        let args = batch.args_for_item(item)?;
        let started = std::time::Instant::now();
        let (result, trace) = batch.plan.execute(ctx, &args)?;
        report.record_trace(started.elapsed(), &trace);
        results.push(result);
    }

    Ok(CollisionBatchResult { results, report })
}
```

If `execute_with_store` is needed for transition workloads, thread the store through carefully.

**Acceptance criteria**

- CPU batch results match existing per-query execution for all current benchmark workload types.
- Transition workloads preserve artifact-store behavior across items.
- Report totals match the sum of individual traces.

#### Task 58B2 — Add `execute_batch_wgsl` for point occupancy and ray cast first

**Description**

Use existing WGSL batch helper functions to execute high-volume point occupancy and ray cast workloads in chunks.

**Files**

- `compiler/collision_exec/gpu.rs`
- `compiler/collision_exec/mod.rs`
- `compiler/tests/collision_exec/wgsl.rs`

**Implementation notes**

Start with point occupancy and ray cast because existing helpers already cover point distance/normal and ray trace.

For point occupancy:

1. Build all points for a chunk.
2. Build or reuse candidate group(s).
3. Run one batched distance query for all points in the group.
4. Run one batched normal query only for points that need witness normals.
5. Materialize `CollisionOccupancyResult` per item.
6. Record dispatch count and dispatch items.

Code sketch:

```rust
fn execute_point_occupancy_batch_wgsl(
    ctx: &QueryExecContext,
    snapshot: &WorldSnapshotHandle,
    batch: &CollisionWorkloadBatch,
    points: &[[f32; 3]],
    candidates: &[SmolStr],
) -> Result<CollisionBatchResult, CollisionExecError> {
    let (distances, distance_obs) = execute_batched_point_distance_queries_with_candidates(
        ctx,
        Some(snapshot),
        batch.capture.clone(),
        batch.domain.clone(),
        points,
        candidates,
    )?;

    let normal_points = points_for_required_normals(points, &distances)?;
    let (normals, normal_obs) = execute_batched_point_normal_queries_with_candidates(
        ctx,
        Some(snapshot),
        batch.capture.clone(),
        batch.domain.clone(),
        &normal_points,
        candidates,
    )?;

    materialize_point_occupancy_results(points, distances, normals, distance_obs, normal_obs)
}
```

For ray cast:

```rust
let rays = chunk.iter().map(ray_from_item).collect::<Vec<_>>();
let (hits, obs) = execute_batched_ray_trace_queries_with_candidates(...)?;
```

If `execute_batched_ray_trace_queries_with_candidates` does not exist yet, add it as a sibling to the single-ray helper.

**Acceptance criteria**

- WGSL batch execution supports point occupancy and ray cast benchmark workloads.
- For `N` point occupancy items with one candidate group, dispatch count is at most `2 * ceil(N / chunk_size)`.
- For `N` ray cast items with one candidate group, dispatch count is at most `ceil(N / chunk_size)`.
- Results match CPU oracle within existing collision tolerances.
- Tests assert dispatch count is not O(query count) for a batch of at least 256 items.

#### Task 58B3 — Group collision batch items by candidate set

**Description**

Avoid rebuilding identical candidate spans for every item. Group items by candidate set or candidate-generation strategy before dispatch.

**Files**

- `compiler/collision_exec/cpu.rs`
- `compiler/collision_exec/gpu.rs`
- `compiler/collision_plan/mod.rs`

**Implementation notes**

Add a stable grouping key.

Code sketch:

```rust
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct CollisionCandidateGroupKey {
    scene_id: u32,
    candidate_names_digest: u64,
}

struct CollisionCandidateGroup {
    key: CollisionCandidateGroupKey,
    item_indices: Vec<usize>,
    candidate_shape_names: Vec<SmolStr>,
}
```

For the first pass, it is acceptable to compute candidates on the CPU using existing broadphase logic, then group by the resulting candidate list digest. Phase 60 moves more of this to the GPU.

**Acceptance criteria**

- Repeated candidate sets share one group.
- Candidate group count and average items per group are reported.
- Tests include two points with the same candidates and one point with different candidates.
- WGSL candidate spans are built once per group/chunk instead of once per item.

### Workstream C: Benchmark path conversion

#### Task 58C1 — Refactor collision benchmark workloads to build batches

**Description**

Change the collision benchmark workload functions in `perf_engine/collection.rs` so they construct `CollisionWorkloadBatch` values and call batch execution.

**Files**

- `compiler/bin/wrela/perf_engine/collection.rs`
- `compiler/bin/wrela/perf_engine/tests.rs`
- `benchmarks/collision_perf/1080p120_closure.toml` if chunk sizes need manifest configuration

**Implementation notes**

Before:

```rust
for i in 1..=scenario.ops {
    let point = ...;
    let started = Instant::now();
    let (_, trace) = plan.execute(ctx, &[capture.clone(), domain.clone(), point_value])?;
    record_collision_trace(&mut metrics, started.elapsed().as_nanos(), &trace);
}
```

After:

```rust
let items = (1..=scenario.ops)
    .map(|i| CollisionBatchItem::PointOccupancy { point: point_for_i(i) })
    .collect::<Vec<_>>();
let batch = CollisionWorkloadBatch {
    name: scenario.id.as_str().into(),
    workload_id: scenario.id.as_str().into(),
    scenario_id: scenario.id.as_str().into(),
    plan,
    contract_id: plan.contract_id().as_str().into(),
    snapshot_id: format!("scene:{}:epoch:1", scene_id).into(),
    capture,
    domain,
    candidate_grouping: CollisionCandidateGroupingPolicy::SharedCandidateDigest,
    certification_policy: CollisionCertificationPolicy::MetricsOnly,
    items,
    chunk_size: collision_batch_chunk_size(scenario),
};
let started = Instant::now();
let result = collision_exec::execute_batch(ctx, &batch, backend)?;
record_collision_batch_report(&mut metrics, started.elapsed(), result.report);
```

Use a conservative default chunk size such as 1024 items. Make it easy to tune later.

**Acceptance criteria**

- Point occupancy and ray-cast benchmark workloads use batch execution.
- CPU backend still produces the same aggregate metrics as the old per-query loop within expected timing noise.
- WGSL backend shows dispatch count based on chunks/groups, not query count.
- Tests fail if a high-volume WGSL benchmark path calls `plan.execute(...)` inside the per-query loop.

#### Task 58C2 — Add closure reporting gates for collision batching

**Description**

Teach closure status to fail when collision WGSL resident closure does not batch.

**Files**

- `compiler/perf_target/mod.rs`
- `compiler/bin/wrela/perf_engine/closure.rs`
- `compiler/bin/wrela/perf_engine/tests.rs`

**Implementation notes**

Add profile fields:

```rust
pub max_collision_dispatches_per_1000_queries: f32,
pub min_collision_average_items_per_dispatch: f32,
pub max_collision_hot_path_readback_bytes_per_frame: u64,
pub max_collision_cpu_certification_queries_per_1000_queries: f32,
```

Initial gates can be lenient:

```rust
max_collision_dispatches_per_1000_queries: 8.0,
min_collision_average_items_per_dispatch: 128.0,
max_collision_hot_path_readback_bytes_per_frame: 0,
max_collision_cpu_certification_queries_per_1000_queries: 64.0,
```

Do not bikeshed exact numbers. The first gate should catch O(N) dispatches.

**Acceptance criteria**

- Closure fails when WGSL collision dispatch count is proportional to query count.
- Closure notes include `query_count`, `dispatch_count`, and `average_items_per_dispatch`.
- CPU oracle profile does not apply WGSL batching gates.
- Tests cover pass and fail cases.

## Phase 58 exit criteria

- Collision has a typed batch API.
- CPU batch execution establishes semantic parity.
- WGSL batch execution supports point occupancy and ray cast workloads.
- Collision benchmarks use batches for high-volume workloads.
- Closure can detect fake WGSL throughput caused by per-query dispatches.

---

# Phase 59: GPU-Resident Collision Tickets And Readback Discipline

## Goal

Separate GPU collision work submission from immediate CPU result collection, so collision can become part of an engine frame without forcing readback after every dispatch.

## Why this phase exists

A batch API reduces dispatch count, but the hot path is still not truly resident if every batch ends with immediate result readback and CPU decode.

The engine needs a way to encode collision work, keep results GPU-resident, and read back only when the frame policy allows it.

### Workstream A: Ticket API

#### Task 59A1 — Add `GpuQueryTicket` and readback policy

**Description**

Introduce a ticket returned by encoded GPU query work. The ticket holds result buffers and decode metadata but does not collect bytes by default.

**Files**

- `compiler/query_exec/gpu_dispatch.rs`
- `compiler/query_exec/wgsl.rs`
- `compiler/gpu_runtime/readback.rs`
- `compiler/tests/query_exec.rs` or focused WGSL tests

**Implementation notes**

Code sketch:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuReadbackPolicy {
    None,
    MetricsOnly,
    DebugExport,
    CpuOracleParity,
    LegacyImmediate,
}

#[derive(Debug, Clone)]
pub(crate) struct GpuQueryDecodeMetadata {
    pub result_abi: PortableAbiType,
    pub contract_id: SmolStr,
    pub selected_workgroup_size: u32,
}

#[derive(Clone)]
pub(crate) struct GpuQueryTicket {
    pub values: GpuQueryBufferHandle,
    pub metrics: Option<GpuQueryBufferHandle>,
    pub item_count: u32,
    pub readback_policy: GpuReadbackPolicy,
    pub decode: GpuQueryDecodeMetadata,
}
```

Add methods to `GpuQueryDispatcher`:

```rust
pub(crate) fn encode_to_ticket(
    &self,
    encoder: &mut wgpu::CommandEncoder,
    profiler: &mut GpuPassProfiler,
    policy: GpuReadbackPolicy,
) -> Result<GpuQueryTicket, QueryExecError> {
    self.initialize_dispatch_state()?;
    self.encode_compute_pass(encoder, profiler);
    Ok(GpuQueryTicket {
        values: self.dispatch_result().values,
        metrics: self.dispatch_result().metrics,
        item_count: self.item_count(),
        readback_policy: policy,
        decode: self.decode_metadata(),
    })
}
```

Keep existing immediate methods as wrappers around `LegacyImmediate`.

The ticket must be self-sufficient enough that a collector can decode result and observability buffers without keeping the original dispatcher alive as hidden state. That matches the current repo seam more honestly and prevents the "ticket" from being only a half-separation.

**Acceptance criteria**

- Existing immediate query tests still pass.
- New tests can encode a query and inspect a ticket without reading back values.
- `GpuReadbackPolicy::None` schedules no readback tickets.
- Tickets can be collected and decoded without requiring the original dispatcher object to survive as hidden decode state.
- `LegacyImmediate` remains clearly named as legacy/debug/oracle behavior.

#### Task 59A2 — Add collision GPU tickets

**Description**

Expose collision-specific ticketed execution for batch workloads.

**Files**

- `compiler/collision_exec/gpu.rs`
- `compiler/collision_exec/mod.rs`
- `compiler/tests/collision_exec/wgsl.rs`

**Implementation notes**

Add a collision wrapper around query tickets.

Code sketch:

```rust
pub(crate) struct CollisionGpuBatchTicket {
    pub workload: SmolStr,
    pub query_kind: CollisionQueryKind,
    pub item_count: u32,
    pub distance_ticket: Option<GpuQueryTicket>,
    pub normal_ticket: Option<GpuQueryTicket>,
    pub ray_ticket: Option<GpuQueryTicket>,
    pub report_seed: CollisionBatchExecutionReport,
}
```

For point occupancy, the collision ticket may contain both distance and normal tickets. For ray cast, it contains a ray ticket.

**Acceptance criteria**

- Point occupancy and ray cast can encode collision GPU work into tickets.
- Tickets record item count and expected dispatch count.
- Immediate CPU materialization remains available through a clearly named helper like `collect_collision_gpu_batch_for_oracle(...)`.

### Workstream B: Framegraph integration

#### Task 59B1 — Make presentation framegraph readback policy reusable

**Description**

The current `PresentationFramegraph` has readback scheduling. Move the generic pieces to `gpu_runtime` or `engine_frame` so collision can use the same policy.

**Files**

- `compiler/presentation_exec/framegraph.rs`
- `compiler/gpu_runtime/readback.rs`
- `compiler/engine_frame/mod.rs`

**Implementation notes**

Do not move presentation-specific attachment logic. Move only policy/summary pieces.

Code sketch:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GpuReadbackSummary {
    pub reason: ReadbackReason,
    pub subsystem: EngineSubsystemKind,
    pub label: SmolStr,
    pub size_bytes: u64,
    pub hot_path: bool,
}
```

**Acceptance criteria**

- Presentation attachment readback still works.
- Collision can schedule metrics readback using the same summary model.
- Closure reports can aggregate readback bytes by subsystem.

#### Task 59B2 — Stop collecting collision result buffers in closure mode

**Description**

For the WGSL resident closure path, collision should not read back every result unless the CPU oracle/parity policy explicitly asks for it.

**Files**

- `compiler/collision_exec/gpu.rs`
- `compiler/bin/wrela/perf_engine/collection.rs`
- `compiler/bin/wrela/perf_engine/closure.rs`

**Implementation notes**

In closure mode, collect only metrics needed to report dispatches, item counts, and violations. For final semantic parity tests, use CPU oracle lanes or explicit parity modes.

Add a mode parameter:

```rust
pub enum CollisionExecutionCollectionMode {
    ClosureMetricsOnly,
    CpuOracleParity,
    DebugFullResults,
}
```

**Acceptance criteria**

- WGSL resident closure mode records zero collision result readback bytes.
- Debug/parity mode can still read full results and compare to CPU.
- Closure fails if result readback happens under `ClosureMetricsOnly`.
- Tests cover both closure and parity modes.

### Workstream C: Upload and resource reuse

#### Task 59C1 — Route collision batch uploads through `FrameUploadArena`

**Description**

Ensure collision batch input uploads use the shared upload arena/staging-belt pattern instead of many independent small writes.

**Files**

- `compiler/query_exec/wgsl.rs`
- `compiler/query_exec/gpu_dispatch.rs`
- `compiler/collision_exec/gpu.rs`
- `compiler/gpu_runtime/upload.rs`

**Implementation notes**

The repo already has `FrameUploadArena`. The task is to ensure collision batches use it when encoded into an engine frame.

Code sketch:

```rust
let mut arena = lock_shared_upload_arena(
    native.limit_request,
    &native.device,
    collision_upload_chunk_size(input_bytes.len()),
);
arena.set_scratch_encoder(existing_frame_encoder);
arena.write_storage_bytes(&self.input_buffer, 0, input_bytes)?;
```

Be careful with encoder ownership. If this is hard, add a tiny `EngineFrameUploadContext` wrapper instead of fighting borrow checker complexity in a giant function.

**Acceptance criteria**

- Collision batch upload byte count is reported.
- Collision batch input upload does not allocate a fresh staging path per item.
- Tests or metrics prove transient buffer creations do not scale with item count for batched workloads.

## Phase 59 exit criteria

- GPU query tickets exist.
- Collision can encode GPU batch work without immediate result readback.
- Readback policy is explicit and closure-aware.
- Collision closure mode can report metrics without full result readback.
- Uploads are routed through reusable staging where possible.

---

# Phase 60: GPU Candidate Compaction And Narrow-Phase Throughput

## Goal

Move collision from “batched query helper calls over CPU-built candidate groups” toward GPU-side candidate counting, compaction, and narrow-phase evaluation.

## Why this phase exists

Batching existing helpers is necessary, but it still leaves too much collision work on the CPU:

- broadphase candidate enumeration
- candidate grouping
- candidate span construction
- sweep sample scheduling
- per-query CPU materialization

GPU collision throughput needs the standard data-parallel shape:

1. count candidate work
2. prefix sum counts
3. scatter compact candidates
4. evaluate compact candidate stream
5. reduce per item
6. certify exact answers on CPU only where required

### Workstream A: Candidate table v1

#### Task 60A1 — Add `CollisionCandidateTable` layout

**Description**

Define a portable flat candidate-table layout that can live on CPU or GPU.

**Files**

- `compiler/collision_exec/gpu.rs` or new `compiler/collision_exec/candidate_table.rs`
- `compiler/portable/*` if ABI helpers are needed
- `compiler/tests/collision_exec/wgsl.rs`

**Implementation notes**

Code sketch:

```rust
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct CollisionCandidateSpanRecord {
    pub item_index: u32,
    pub start: u32,
    pub count: u32,
    pub flags: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct CollisionCandidateRecord {
    pub item_index: u32,
    pub shape_index: u32,
    pub candidate_flags: u32,
    pub _pad: u32,
}

pub struct CollisionCandidateTable {
    pub spans: Vec<CollisionCandidateSpanRecord>,
    pub candidates: Vec<CollisionCandidateRecord>,
}
```

Keep it simple and explicit. It should be easy to dump in tests.

**Acceptance criteria**

- Candidate table can represent existing CPU broadphase output.
- Candidate table has ABI encoding/decoding tests.
- Candidate table reports candidate count, rejected count, and max candidates per item.

#### Task 60A2 — Add CPU-built table path before GPU-built table path

**Description**

Replace ad hoc candidate span vectors with `CollisionCandidateTable` while candidates are still built on CPU.

**Files**

- `compiler/collision_exec/cpu.rs`
- `compiler/collision_exec/gpu.rs`
- `compiler/tests/collision_exec/cpu.rs`
- `compiler/tests/collision_exec/wgsl.rs`

**Implementation notes**

This is a refactor phase. Do not move candidate generation to GPU yet.

**Acceptance criteria**

- Existing WGSL candidate-spans tests pass through the new table type.
- Reports include candidate table density and max span length.
- No behavior change in CPU oracle path.

### Workstream B: GPU count/scatter path

#### Task 60B1 — Add GPU candidate count pass for point/ray workloads

**Description**

Add a WGSL pass that counts candidate shapes per item using resident acceleration/shape metadata.

**Files**

- `compiler/collision_exec/gpu.rs`
- `compiler/query_exec/wgsl/codegen/*` if shared traversal helpers are needed
- `compiler/collision_exec/wgsl/*.wgsl` if this RFC introduces separate collision shaders
- `compiler/tests/collision_exec/wgsl.rs`

**Implementation notes**

Prefer a separate collision shader file if codegen changes get too tangled.

WGSL sketch:

```wgsl
struct CandidateCount {
    count: atomic<u32>,
};

@group(0) @binding(0) var<storage, read> accel_nodes: array<AccelNode>;
@group(1) @binding(0) var<storage, read> items: array<CollisionItem>;
@group(2) @binding(0) var<storage, read_write> counts: array<u32>;

@compute @workgroup_size(128)
fn count_candidates(@builtin(global_invocation_id) gid: vec3<u32>) {
    let item_index = gid.x;
    if (item_index >= item_count) { return; }

    var count: u32 = 0u;
    // Conservative first pass: traverse resident accel nodes and count leaves whose bounds overlap.
    // Later phases can use packet/tile-specific tables.
    for (var node_index: u32 = 0u; node_index < accel_node_count; node_index = node_index + 1u) {
        if (item_overlaps_node(items[item_index], accel_nodes[node_index])) {
            count = count + 1u;
        }
    }
    counts[item_index] = count;
}
```

This first pass may be conservative. Correctness matters first.

**Acceptance criteria**

- GPU count pass matches CPU broadphase candidate counts on small fixtures.
- Count pass is timestamped when timestamp support exists.
- Count pass does not read back counts in closure mode.

#### Task 60B2 — Implement prefix-sum path, with fixed-cap fallback first

**Description**

Add candidate compaction. Start with a fixed-cap table if necessary, then add prefix sum.

**Files**

- `compiler/collision_exec/gpu.rs`
- new `compiler/gpu_runtime/scan.rs` if shared prefix-sum code is introduced
- `compiler/tests/collision_exec/wgsl.rs`

**Implementation notes**

The safe incremental path is:

1. Fixed-cap candidate table per item, such as 32 candidates per item.
2. Report overflow count and fall back to CPU when overflow occurs.
3. Add a workgroup-local prefix sum for small batches.
4. Add multi-pass prefix sum for large batches.

Code sketch for fixed-cap intermediate:

```rust
pub struct CollisionCompactionPolicy {
    pub max_candidates_per_item: u32,
    pub overflow_fallback: bool,
}
```

WGSL sketch:

```wgsl
let slot = item_index * max_candidates_per_item + local_candidate_index;
if (local_candidate_index < max_candidates_per_item) {
    flat_candidates[slot] = CollisionCandidate(item_index, shape_index, flags, 0u);
} else {
    atomicAdd(&overflow_count, 1u);
}
```

Then add scan-based compaction:

```text
count_candidates -> exclusive_scan(counts) -> scatter_candidates -> evaluate_candidates -> reduce_results
```

**Acceptance criteria**

- Fixed-cap path works and reports overflow.
- Overflow triggers CPU fallback or larger chunk policy; it must not silently drop candidates.
- Prefix-sum path is added behind a feature/policy flag.
- Tests cover zero candidates, one candidate, many candidates, and overflow.

#### Task 60B3 — Add flat narrow-phase evaluation and per-item reduction

**Description**

Evaluate compacted candidates in parallel and reduce to one result per collision item.

**Files**

- `compiler/collision_exec/gpu.rs`
- collision WGSL shader/codegen files
- `compiler/tests/collision_exec/wgsl.rs`

**Implementation notes**

For point occupancy, reduce minimum signed distance and associated normal candidate.
For ray cast, reduce nearest hit.
For overlap, reduce any hit plus best witness candidate.

WGSL sketch:

```wgsl
struct CandidateEvalResult {
    item_index: u32,
    hit: u32,
    distance: f32,
    shape_index: u32,
};

@compute @workgroup_size(128)
fn evaluate_candidates(@builtin(global_invocation_id) gid: vec3<u32>) {
    let candidate_index = gid.x;
    if (candidate_index >= candidate_count) { return; }
    let c = candidates[candidate_index];
    eval_results[candidate_index] = evaluate_item_shape(items[c.item_index], c.shape_index);
}
```

Reduction can initially be a simple per-item loop in a follow-up pass. Optimize later.

**Acceptance criteria**

- Point occupancy uses flat candidate evaluation in WGSL.
- Ray cast uses flat candidate evaluation in WGSL or is explicitly left for the next task with a failing TODO test ignored by default.
- CPU parity tests compare materialized results.
- Reports include candidate compaction ratio and reduction pass count.

### Workstream C: Sweeps and TOI

#### Task 60C1 — Batch sweep sample generation on GPU

**Description**

Move the sample-point generation for sweep and TOI workloads into a GPU-friendly batch representation.

**Files**

- `compiler/collision_exec/cpu.rs`
- `compiler/collision_exec/gpu.rs`
- `compiler/tests/collision_exec/wgsl.rs`

**Implementation notes**

The existing `gpu_assisted_sweep_outcome(...)` samples 4–16 points per sweep and calls batched distance queries. The next step is batching across many sweeps.

Code sketch:

```rust
pub struct CollisionSweepSampleBatch {
    pub sweep_indices: Vec<u32>,
    pub fractions: Vec<f32>,
    pub centers: Vec<[f32; 3]>,
}
```

For `M` sweeps and `S` samples each, encode `M*S` points as one batch.

**Acceptance criteria**

- Repeated sweeps generate sample points as a batch.
- WGSL distance evaluation runs across all samples in chunked batches.
- CPU certification is only run for candidate hit brackets or contract-required final answers.
- Reports include samples evaluated, brackets found, and CPU certification count.

#### Task 60C2 — Keep CPU certification explicit and budgeted

**Description**

Do not hide exactness costs. Add explicit certification accounting to every GPU-assisted collision path.

**Files**

- `compiler/collision_exec/cpu.rs`
- `compiler/collision_exec/gpu.rs`
- `compiler/bin/wrela/perf_engine/collection.rs`
- `compiler/bin/wrela/perf_engine/closure.rs`

**Implementation notes**

Use existing `cpu_certification_query_count`, but make it batch-aware and closure-gated.

**Acceptance criteria**

- Every GPU-assisted collision report includes CPU certification query count.
- Closure can fail on certification explosion.
- Findings distinguish “GPU narrow phase slow” from “CPU certification too frequent.”

## Phase 60 exit criteria

- Collision has a flat candidate-table layout.
- Candidate tables can be CPU-built and GPU-consumed.
- GPU candidate count/scatter exists for at least point occupancy.
- GPU narrow-phase evaluation and reduction exist for at least point occupancy.
- Sweeps batch their sample evaluations across many items.
- CPU certification is explicit and budgeted.

---

# Phase 61: Engine Frame Scheduler And Budget Governor

## Goal

Introduce the first real engine-frame scheduler that coordinates presentation, collision, state-advance, GPU runtime, and future subsystem reserve budgets.

## Why this phase exists

Once collision can batch and stay resident, Wrela needs a place to schedule it with rendering instead of measuring it beside rendering.

This phase should be modest. It is not a general-purpose job system. It is a typed frame orchestrator for the current engine.

### Workstream A: Scheduler skeleton

#### Task 61A1 — Add `EngineFrameScheduler`

**Description**

Add an engine-frame scheduler that accepts subsystem work packets and produces an `EngineFrameReport`.

**Files**

- new `compiler/engine_frame/scheduler.rs` or extend `compiler/engine_frame/mod.rs`
- `compiler/lib.rs`
- `compiler/tests/engine_frame.rs`

**Implementation notes**

Keep the trait narrow.

Code sketch:

```rust
pub trait EngineSubsystemWork {
    fn descriptor(&self) -> EngineSubsystemDescriptor;
    fn prepare(&mut self, ctx: &mut EngineFrameContext) -> Result<(), EngineFrameError>;
    fn encode(&mut self, ctx: &mut EngineFrameContext) -> Result<(), EngineFrameError>;
    fn finish(&mut self, ctx: &mut EngineFrameContext) -> Result<EngineSubsystemReport, EngineFrameError>;
}

pub struct EngineSubsystemDescriptor {
    pub kind: EngineSubsystemKind,
    pub label: SmolStr,
    pub runs_after: Vec<EngineSubsystemKind>,
    pub requires_gpu: bool,
    pub allows_hot_path_readback: bool,
}

pub struct EngineFrameScheduler {
    pub budget: PerfClosureEngineFrameBudget,
    pub readback_policy: GpuReadbackPolicy,
}
```

Add a concrete `EngineFrameContext` with:

- snapshot handle
- optional GPU context
- command encoder
- profiler
- upload arena handle or helper
- readback tickets
- accumulated metrics

The scheduler contract needs to be explicit about three things:

1. ordering: subsystem work is scheduled in deterministic topological order from declared dependencies
2. ownership: the scheduler owns the encoder/profiler/readback-policy story, and subsystems work through `EngineFrameContext`
3. truth model: subsystem reports explain the frame; they do not each invent their own global timing definition

This does **not** need to become a general job system. It does need to be more than a sequential bag of callbacks.

**Acceptance criteria**

- Scheduler can run with no subsystems and produce an empty report.
- Scheduler can run a fake subsystem in tests and record timing/report data.
- Scheduler owns readback policy and budget checks.

#### Task 61A2 — Wrap presentation as an engine subsystem

**Description**

Adapt existing presentation execution to run under `EngineFrameScheduler` without deleting the standalone presentation path.

**Files**

- `compiler/presentation_exec/framegraph.rs`
- `compiler/presentation_exec/mod.rs`
- `compiler/engine_frame/mod.rs`
- `compiler/tests/presentation_exec/wgsl.rs`

**Implementation notes**

Create an adapter, not a rewrite.

Code sketch:

```rust
pub struct PresentationSubsystemWork {
    pub plan: PresentationPlan,
    pub view_args: PresentationViewArgs,
    pub report: Option<PresentationFrameCostReport>,
}

impl EngineSubsystemWork for PresentationSubsystemWork {
    fn descriptor(&self) -> EngineSubsystemDescriptor {
        EngineSubsystemDescriptor {
            kind: EngineSubsystemKind::Presentation,
            label: "presentation".into(),
            runs_after: vec![],
            requires_gpu: true,
            allows_hot_path_readback: false,
        }
    }
    fn prepare(&mut self, ctx: &mut EngineFrameContext) -> Result<(), EngineFrameError> { ... }
    fn encode(&mut self, ctx: &mut EngineFrameContext) -> Result<(), EngineFrameError> { ... }
    fn finish(&mut self, ctx: &mut EngineFrameContext) -> Result<EngineSubsystemReport, EngineFrameError> { ... }
}
```

The adapter can initially call the existing presentation path and translate the report. Later it can share the command encoder directly.

**Acceptance criteria**

- Presentation can contribute an `EngineSubsystemReport`.
- Existing presentation benchmark reports continue to work.
- Engine-frame report includes presentation GPU runtime metrics.

#### Task 61A3 — Wrap collision batch execution as an engine subsystem

**Description**

Adapt `CollisionWorkloadBatch` execution into scheduler work.

**Files**

- `compiler/collision_exec/mod.rs`
- `compiler/collision_exec/gpu.rs`
- `compiler/engine_frame/mod.rs`
- `compiler/tests/collision_exec/wgsl.rs`

**Implementation notes**

Code sketch:

```rust
pub struct CollisionSubsystemWork {
    pub batches: Vec<CollisionWorkloadBatch>,
    pub mode: CollisionExecutionCollectionMode,
    pub report: Option<CollisionBatchExecutionReport>,
}

impl EngineSubsystemWork for CollisionSubsystemWork {
    fn descriptor(&self) -> EngineSubsystemDescriptor {
        EngineSubsystemDescriptor {
            kind: EngineSubsystemKind::Collision,
            label: "collision".into(),
            runs_after: vec![EngineSubsystemKind::StateAdvance],
            requires_gpu: true,
            allows_hot_path_readback: matches!(self.mode, CollisionExecutionCollectionMode::DebugFullResults | CollisionExecutionCollectionMode::CpuOracleParity),
        }
    }

    fn encode(&mut self, ctx: &mut EngineFrameContext) -> Result<(), EngineFrameError> {
        for batch in &self.batches {
            collision_exec::encode_batch_into_engine_frame(ctx, batch, self.mode)?;
        }
        Ok(())
    }
}
```

**Acceptance criteria**

- Collision can run inside the scheduler in metrics-only mode.
- Collision subsystem report includes batch count, query count, dispatch count, CPU certification count, readback bytes, and fallback rate.
- Engine-frame tests include one presentation subsystem and one collision subsystem in the same frame report.

### Workstream B: Budget governor

#### Task 61B1 — Add budget evaluation to `EngineFrameReport`

**Description**

Evaluate total and per-subsystem budgets after each engine frame.

**Files**

- `compiler/engine_frame/mod.rs`
- `compiler/perf_target/mod.rs`
- `compiler/bin/wrela/perf_engine/closure.rs`

**Implementation notes**

Code sketch:

```rust
pub fn evaluate_engine_frame_budget(
    report: &mut EngineFrameReport,
    budget: &PerfClosureEngineFrameBudget,
) {
    if micros_to_ms(report.frame_wall_time_micros) > budget.frame_wall_time_p95_ms {
        report.violations.push("engine_frame_wall_time_p95_budget_exceeded".to_string());
    }
    if report.gpu_runtime.readback_bytes > budget.max_hot_path_readback_bytes_per_frame {
        report.violations.push("hot_path_readback_budget_exceeded".to_string());
    }
}
```

Keep this simple and deterministic.

The main rule is: budget evaluation gates on frame wall time first. CPU/GPU critical-path timings are explanatory evidence and may become additional gates later, but the first closure contract should not regress to additive CPU+GPU accounting.

**Acceptance criteria**

- Engine-frame reports contain violations when budgets are exceeded.
- Closure status consumes those violations.
- Tests cover total-frame, collision, queue-submit, and readback violations.

#### Task 61B2 — Add quality/degradation decisions at engine-frame scope

**Description**

Move toward frame-level quality decisions so presentation quality does not ignore collision pressure.

**Files**

- `compiler/presentation_exec/controller.rs`
- `compiler/engine_frame/mod.rs`
- `compiler/perf_target/mod.rs`

**Implementation notes**

Do not replace `AdaptivePresentationController` yet. Add a small wrapper that can consider collision pressure.

Code sketch:

```rust
pub struct EngineBudgetGovernor {
    pub presentation: AdaptivePresentationController,
    pub collision_pressure_window: Vec<f32>,
}

impl EngineBudgetGovernor {
    pub fn observe_engine_frame(&mut self, report: &EngineFrameReport) -> EngineBudgetDecision {
        // If collision exceeds budget, prefer collision batching/quality actions before lowering rendering quality.
        // If rendering exceeds budget, use existing presentation degradation rules.
        // If both exceed, report both; do not hide one behind the other.
    }
}
```

**Acceptance criteria**

- Engine budget governor can observe collision over-budget status.
- Existing presentation quality controller tests still pass.
- Reports distinguish presentation degradation from collision pressure.

### Workstream C: Future subsystem reserve

#### Task 61C1 — Add future-subsystem reserve accounting

**Description**

Reserve time for future systems before they exist, so 1080p120 closure does not spend the entire frame on rendering and collision.

**Files**

- `compiler/perf_target/mod.rs`
- `compiler/engine_frame/mod.rs`
- `compiler/bin/wrela/perf_engine/closure.rs`

**Implementation notes**

Do not add a fake executed subsystem sample just to represent reserve accounting.

Add an explicit reserve accounting object instead:

```rust
pub struct EngineFutureReserveReport {
    pub reserved_micros: u128,
    pub remaining_micros: i128,
    pub exhausted: bool,
}
```

This is not fake performance. It is budget accounting, and it should stay visibly separate from observed subsystem execution.

**Acceptance criteria**

- Engine-frame closure report includes future-subsystem reserve accounting.
- Total frame budget evaluation includes the reserve.
- Closure cannot claim the full 8.33 ms if rendering+collision leave no reserve.
- Reserve is not represented as fabricated observed CPU/GPU work.

## Phase 61 exit criteria

- `EngineFrameScheduler` exists.
- Presentation and collision can be adapted as subsystems.
- Engine-frame budget violations are computed.
- Future-subsystem reserve budget is included in closure accounting.
- The scheduler is still small enough for a junior engineer to understand.

---

# Phase 62: Closure Gates, Regression Evidence, And Legacy Cleanup

## Goal

Make engine-frame closure canonical, fail closed on fake wins, and clean up misleading legacy reporting paths.

## Why this phase exists

New architecture is only useful if the repo’s default proof path uses it.

This phase turns the previous work into durable repo behavior.

### Workstream A: Canonical benchmark lane

#### Task 62A1 — Add `benchmarks/engine_frame/1080p120_closure.toml`

**Description**

Create a benchmark suite where engine-frame work is first-class, not assembled after the fact from separate reports.

**Files**

- new `benchmarks/engine_frame/1080p120_closure.toml`
- new `benchmarks/engine_frame/bench.toml`
- new `benchmarks/engine_frame/tests/engine_frame_test.wr` if needed
- `benchmarks/README.md`

**Implementation notes**

Start by mirroring whole-frame scenarios, then point them at the engine-frame execution path.

The suite should preserve the current compatibility bridge explicitly:

- `whole_frame` remains the additive compatibility lane until engine-frame closure is stable
- `engine_frame` becomes the canonical throughput lane once the new scheduler/report path is proven
- docs must say which lane is canonical at each step so the repo does not end up with two competing "main" stories

Code sketch:

```toml
version = 1
suite = "engine_frame"

[profiles.closure_1080p120]
warmup_pairs = 4
measure_pairs = 12
coverage = "all"
execution_story = "wgsl_resident_engine_frame"
adapter_name = "wgsl_resident"
warmup_protocol = "pipeline_resident_scene_and_engine_frame_upload"

[[scenarios]]
id = "engine_1080p120_dense_constructive_collision"
class = "closure"
ops = 1
presentation = { entry = "tests/engine_frame_test.wr", view = "show_dense_constructive_1080p120_closure_view", width = 1920, height = 1080, frames = 7 }
collision = { entry = "tests/engine_frame_test.wr", region = "collision_perf_region", domain = "collision_perf_domain", workload = "dense_ray_casts", ops = 72000, chunk_size = 1024 }
state_advance = { ticks = 7, fixed_seed = 10800120 }
future_reserve_ms = 1.0
```

**Acceptance criteria**

- The engine-frame benchmark suite loads through the benchmark manifest loader.
- It runs at least one scenario through the engine-frame scheduler.
- Existing whole-frame suite still exists but is marked compatibility/legacy once engine-frame closure is stable.

#### Task 62A2 — Add `just perf-engine-closure`

**Description**

Expose the canonical engine-frame closure lane.

**Files**

- `justfile`
- `AGENTS.md`
- `benchmarks/README.md`

**Implementation notes**

Code sketch:

```just
perf-engine-closure-cmd := "cargo run -p wrela -- perf benchmarks/engine_frame --profile=1080p120 --query-backend=wgsl --why-not-120"

perf-engine-closure:
    {{perf-engine-closure-cmd}}
```

Keep `perf-closure` for compatibility until cleanup.

This RFC must also say what happens to `just ship`.

The current repo truth is that `just ship` stops at `just perf-smoke`, not the representative closure lane. If this RFC wants to change that, it should do so explicitly in the workflow docs and acceptance criteria instead of implying it through the verification matrix.

**Acceptance criteria**

- `just perf-engine-closure` exists.
- Docs identify it as the canonical engine-frame perf lane.
- `just perf-closure` either delegates to it or is explicitly labeled as the old whole-frame lane.
- `AGENTS.md`, `benchmarks/README.md`, and the `justfile` agree about whether `just ship` remains smoke-only or expands to include engine-frame closure.

### Workstream B: Hard closure gates

#### Task 62B1 — Make engine-frame status part of the verdict

**Description**

Update `build_closure_verdict(...)` so the engine-frame lane can fail the closure verdict.

**Files**

- `compiler/bin/wrela/perf_engine/closure.rs`
- `compiler/bin/wrela/perf_engine/tests.rs`
- `compiler/perf_target/mod.rs`

**Implementation notes**

Current verdict logic fails on frame or collision violations. Add engine-frame violations.

Code sketch:

```rust
let engine_frame_sampled = !matches!(engine_frame.status, PerfClosureLaneStatus::NotSampled);
let status = if !frame_sampled && !collision_sampled && !engine_frame_sampled {
    PerfClosureVerdictStatus::NotApplicable
} else if matches!(engine_frame.status, PerfClosureLaneStatus::Violated)
    || matches!(frame.status, PerfClosureLaneStatus::Violated)
    || matches!(collision.status, PerfClosureLaneStatus::Violated)
{
    PerfClosureVerdictStatus::Failed
} else {
    PerfClosureVerdictStatus::Met
};
```

**Acceptance criteria**

- Closure verdict fails when engine-frame budget fails.
- Closure verdict fails when engine-frame evidence is expected but missing.
- Top remaining bottleneck can be `engine_frame_collision`, `engine_frame_presentation`, `engine_frame_readback`, `engine_frame_queue_submit`, or `future_reserve_exhausted`.

#### Task 62B2 — Add collision-specific closure findings

**Description**

Upgrade `explain_collision_why_not_120_findings(...)` so it explains batching, readback, dispatch, and certification issues.

**Files**

- `compiler/bin/wrela/perf_engine/closure.rs`
- `compiler/bin/wrela/perf_engine/tests.rs`

**Implementation notes**

Add findings for:

- O(query count) dispatches
- low average items per dispatch
- hot-path readback bytes
- CPU certification explosion
- candidate compaction overflow
- GPU count/scatter unsupported fallback

Code sketch:

```rust
if report.average_items_per_dispatch < profile.min_collision_average_items_per_dispatch {
    findings.push(PerfClosureFinding {
        subsystem: "collision".to_string(),
        focus: "batch_utilization".to_string(),
        summary: "collision WGSL work is not amortizing dispatch cost across enough items".to_string(),
        evidence: vec![
            format!("average_items_per_dispatch={:.2}", report.average_items_per_dispatch),
            format!("dispatch_count={}", report.dispatch_count),
            format!("query_count={}", report.query_count_total),
        ],
        next_step: "group collision items by candidate set and route high-volume workloads through CollisionWorkloadBatch".to_string(),
    });
}
```

**Acceptance criteria**

- Why-not-120 output names collision batch utilization when it is the bottleneck.
- Why-not-120 output distinguishes collision readback from collision compute.
- Tests cover each new finding.

### Workstream C: Legacy cleanup

#### Task 62C1 — Mark per-query WGSL collision benchmark paths as legacy-only

**Description**

Prevent future regressions where a WGSL closure path accidentally returns to per-query `plan.execute(...)` loops.

**Files**

- `compiler/bin/wrela/perf_engine/collection.rs`
- `compiler/collision_exec/gpu.rs`
- `compiler/collision_exec/cpu.rs`
- `compiler/tests/*`

**Implementation notes**

Add comments and tests that make the intended path unmistakable.

Example guard in benchmark collection:

```rust
if matches!(backend, DispatchBackend::Wgsl) && scenario.ops > 128 && !uses_batch_path {
    return Err(format!(
        "WGSL collision closure workload '{}' must use CollisionWorkloadBatch, not per-query plan execution",
        scenario.id
    ));
}
```

**Acceptance criteria**

- High-volume WGSL collision benchmark workloads cannot run through per-query loops.
- CPU oracle paths can still use per-query execution where appropriate.
- Legacy helpers are named with `legacy`, `debug`, or `oracle` when they force immediate readback.

#### Task 62C2 — Publish final engine-frame perf scorecard

**Description**

Record the before/after state and remaining bottlenecks.

**Files**

- new `.artifacts/engine_frame/phase62-scorecard.json`
- new `docs/perf/engine_frame_closure.md`
- `benchmarks/README.md`

**Implementation notes**

The scorecard should include:

- old whole-frame presentation median/p95
- old collision median/p95
- new engine-frame median/p95
- collision dispatch count before/after
- average items per dispatch before/after
- readback bytes before/after
- queue submits before/after
- CPU certification count before/after
- fallback/witness reuse before/after
- remaining bottleneck and next concrete task

**Acceptance criteria**

- Machine-readable and human-readable scorecards exist.
- Missed budgets are called out honestly.
- The scorecard names the next concrete bottleneck rather than saying “optimize collision” generically.

## Phase 62 exit criteria

- Engine-frame closure lane exists and is canonical.
- Engine-frame status participates in closure verdicts.
- Collision batching/readback/certification gates are enforced.
- Legacy per-query WGSL benchmark paths cannot masquerade as closure paths.
- Final scorecard records before/after evidence and next bottlenecks.

---

## Suggested Implementation Order For A Junior Engineer

Do the work in this order:

1. Add pure report types first. No execution changes.
2. Add tests for report JSON round-trip and budget validation.
3. Add engine-frame report assembly from existing presentation/collision reports.
4. Add collision batch types and CPU batch execution.
5. Convert one benchmark workload, point occupancy, to CPU batch execution.
6. Convert point occupancy to WGSL batch execution.
7. Add dispatch-count and average-items-per-dispatch reporting.
8. Add closure gates for batching.
9. Add ticketed GPU execution.
10. Add readback policy.
11. Add candidate table layout.
12. Add GPU candidate count/scatter for point occupancy.
13. Wrap presentation and collision as scheduler subsystems.
14. Add engine-frame benchmark suite.
15. Make engine-frame closure fail closed.

Do not start with the scheduler. That is tempting, but it will create abstractions before the reports know what must be proven.

## Verification Matrix

Use this matrix while implementing:

| Change area | Focused proof | Broader proof |
|---|---|---|
| Engine-frame report types | `just test-engine-frame` | `just test` |
| Perf profile/budget changes | targeted `perf_engine` tests | `just test-cli` |
| Collision batch CPU | `cargo test -p wrela --test collision_exec cpu` or focused test name | `just test-compiler` |
| Collision batch WGSL | `cargo test -p wrela --test collision_exec wgsl` or focused test name | `just test-compiler` |
| Benchmark collection | targeted `perf_engine` tests | `just perf-smoke` |
| Engine-frame closure | `just perf-engine-closure` | `just ship` only if this RFC explicitly updates `ship` to include that lane |

If a machine lacks a usable WGSL adapter, record that as an environment limitation and still run CPU/reporting tests.

## Final Note

The current architecture is close enough that the next step is not another isolated speed trick.

The next step is making Wrela behave like an engine.

That means collision cannot stay a CPU loop with GPU callouts. It has to become a batch-producing, GPU-resident, budgeted subsystem with explicit certification costs. Rendering cannot be the only owner of frame truth. The perf engine cannot call something “whole-frame” unless it can explain how the subsystems interacted inside the frame.

This RFC gives the repo that missing shape.
