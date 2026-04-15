
# RFC 0008: GPU-Resident Framegraph And Collision Throughput Closure For 1080p120

Status: Proposed post-RFC-0007 execution-model and hot-path closure roadmap after repo read, benchmark read, and supplemental WebGPU/WGSL research

Author: GPT-5.4 Pro

Created: 2026-04-15

Target: post-RFC-0007 `wrela` language, compiler, shared acceleration runtime, WGSL execution, presentation pipeline, collision pipeline, perf closure tooling, and CPU oracle

## Summary

RFC 0007 was the right move.

It gave the repo a shared acceleration spine, a stronger solver ladder, and the beginning of a real performance-closure story.
That work is necessary, but it is not sufficient for 1080p120.

After reading the current renderer and collision paths, the remaining bottleneck no longer looks like “missing acceleration data.”
It looks like an execution-model problem:

- the WGSL hot path still allocates buffers per dispatch
- the WGSL hot path still reads results back to the CPU immediately
- presentation passes still decode and re-encode attachments on the CPU between GPU kernels
- primary visibility still uses CPU-built `Vec<KernelValue>` screen samples and CPU-side hit expansion/materialization
- packetized visibility still issues too many small dispatches
- several WGSL world helper paths still fall back to linear `shape_count` loops
- collision still resolves to the CPU backend and still performs serial candidate loops in narrow phase
- the canonical 1080p120 closure lane still treats CPU as the representative presentation backend

So the remaining work is not “invent a cleverer ray marcher.”

It is to make the hot path structurally GPU-resident, structurally batched, and structurally measurable.

The thesis of this RFC is:

**Wrela does not reach real 1080p120 until the steady-state frame becomes GPU-authoritative from ray generation through final color/history update, and until collision can consume the same resident scene data through batched GPU execution instead of serial CPU query loops.**

That means the next closure steps are:

1. add truthful GPU timing and residency/churn metrics
2. make snapshot-scoped scene/acceleration/cache data persist on the GPU across frames
3. replace CPU-owned presentation attachments with GPU-resident attachments
4. remove CPU screen-sample generation, CPU hit expansion, CPU attachment materialization, and CPU pass glue from the WGSL lane
5. collapse many tiny query dispatches into a small number of large batched dispatches
6. make collision reuse the same resident scene buffers and batched WGSL execution model
7. turn the canonical 1080p120 closure lane into a true WGSL resident-framegraph closure lane, with CPU kept as oracle and parity reference

The design stance stays conservative:

- CPU remains the semantic oracle
- no flag day rewrite
- no subgroup-dependent correctness path
- no hidden precision loss
- no hot-path readback in closure mode
- no “claiming 120” with a CPU-backed representative lane

## Relationship To Earlier RFCs And Repo Vision

This roadmap builds directly on:

- `language/spec/rfcs/0001-field-game-language.md`
- `language/spec/rfcs/0002-field-engine-implementation-roadmap.md`
- `language/spec/rfcs/0003-phase-9-5-semantic-convergence-plan.md`
- `0004-question-families-query-contracts-roadmap.md`
- `0005-realtime-presentation-view-plans-frame-contracts-roadmap.md`
- `0006-certified-world-snapshots-temporal-semantics-artifact-runtime-query-program-spine-roadmap.md`
- `0007-shared-acceleration-spine-1080p120-rendering-collision-roadmap.md`

RFC 0005 made presentation structurally explicit.
RFC 0006 made snapshots, artifacts, and query families structurally explicit.
RFC 0007 made acceleration, solver choice, and cache artifacts structurally explicit.

This RFC is the next layer down.

It says: now that the engine has the right acceleration structure, stop destroying that advantage with a CPU-authoritative execution path.

In other words:

- RFC 0005 made the frame graph logically correct.
- RFC 0006 made query/artifact identity correct.
- RFC 0007 made the solver and acceleration substrate strong enough.
- RFC 0008 makes the execution model fast enough to let those earlier wins matter.

## Research Grounding

This plan is shaped by official WebGPU / `wgpu` guidance and constraints, not by folklore.

The most important grounding points are:

1. **Mapped/readback buffers are hostile to the hot path.**
   `wgpu` explicitly treats mapped buffers as CPU-owned while mapped; they cannot be used by GPU commands at the same time.
   That makes per-dispatch `map_async` + poll + decode a structural stall source, not a harmless convenience.

2. **Many small writes should use persistent staging/reuse, not fresh tiny buffers.**
   `wgpu::Queue::write_buffer()` is queued and begins on `submit`, and `wgpu::util::StagingBelt` exists specifically to make many small writes cheaper through reusable staging allocations.

3. **Real GPU timing should use timestamp queries, not only CPU wall clock.**
   `TIMESTAMP_QUERY` and pass timestamp writes are the right way to separate CPU submission cost from actual GPU work.

4. **Storage textures and storage buffers make a GPU-resident attachment story possible.**
   WGPU exposes read/write storage textures, but not every attachment should become a texture.
   Structured records such as `Hit3`, `Surface`, and `Medium` are usually better as storage buffers.
   Final color/history or sampled post passes can use textures where that is a better fit.

5. **Limits and optional features should be requested deliberately.**
   `wgpu` recommends requesting only the limits actually needed, because asking for more than necessary can reduce compatibility and performance.

6. **`f16` is optional and subgroup behavior is optional.**
   `SHADER_F16` can reduce bandwidth for selected attachments, but it must stay feature-gated and parity-tested.
   Subgroup features remain optional and must never become the correctness path.

7. **GPU-driven dispatch can be useful later, but should stay optional.**
   `dispatchWorkgroupsIndirect` is a good fit for queue-compacted workloads, but it is not required for the first correctness-preserving resident framegraph closure.

This RFC turns those facts into concrete repo work items.

## Current Repo Read

The repo already has enough structure to avoid a rewrite.

### What is already strong

1. `QueryExecContext` already builds and exposes shared acceleration forests and cache catalogs, so the semantic source of truth for persistent scene/acceleration data already exists.
2. `presentation_contract`, `presentation_plan`, and `presentation_exec` already encode a typed pass graph with explicit attachments, quality tiers, temporal history, tile culling, and pass-level cost reports.
3. `query_exec` already has WGSL generation, stable workgroup-size validation, observability fields, and a real split between CPU and WGSL execution.
4. `collision_plan` and `collision_exec` already have broadphase artifacts, witness reuse structure, continuation seeds, and explicit collision output semantics.
5. `perf_target`, `benchmarks`, and `presentation_exec::cost` already provide a place to define closure profiles and “why not 120?” findings.

That means the missing work is not a missing abstraction layer.
It is changing which side of the CPU/GPU boundary owns the steady-state frame.

### Where the hot path is still pathologically slow

The remaining slow path is visible in concrete code, and it is mostly ordinary compute/runtime overhead.

1. **WGSL query dispatch still does per-dispatch buffer creation, upload, submit, readback, and decode.**

`compiler/query_exec/wgsl.rs` currently:

- encodes dispatch/input/accel/cache/meta buffers every dispatch
- creates fresh storage buffers every dispatch
- submits compute work
- immediately creates a readback buffer
- maps it
- polls the device
- copies bytes into `Vec<u8>`
- decodes back into `Vec<KernelValue>`

That means the supposed “GPU path” is still CPU-synchronized at the end of every dispatch.

2. **Presentation WGSL helper passes still do per-chunk output readback.**

`compiler/presentation_exec/wgsl.rs::dispatch_linear_shader_with_chunk_limit` creates dispatch/input/output buffers per chunk, submits, and immediately reads the chunk result back into CPU bytes before repacking the attachment.

That makes shading/composite/temporal compute passes GPU compute in the narrowest possible sense, but not in the end-to-end runtime sense that matters for 120 FPS.

3. **Presentation attachments are still CPU-authoritative in the WGSL lane.**

The WGSL presentation path still mutates `AttachmentResourceSet` CPU byte arrays between passes.
`shade_primary_wgsl`, `composite_color_wgsl`, and `temporal_resolve_wgsl` decode attachment bytes into CPU values, build CPU-side kernel-value arrays, run a GPU pass, then overwrite CPU byte arrays with the result.

4. **Primary visibility setup is still CPU-heavy.**

The current WGSL path still:

- calls `generate_screen_samples()` to build one `ScreenSampleQuery` `KernelValue` per pixel
- clones those screen sample vectors when internal and output viewports differ
- extracts rays from those CPU values
- expands internal hits on the CPU with `expand_internal_hits()`
- materializes primary/depth/world-normal attachments on the CPU with `materialize_primary_visibility_attachments()`

At 1920×1080, that is millions of enum-rich Rust values and copies per frame before later passes even begin.

5. **Packetized primary visibility still fans out into too many tiny dispatches.**

`execute_packetized_primary_visibility_query()` still walks packet queues on the CPU and, for candidate-table mode, dispatches per packet and then per candidate shape within the packet.
That destroys dispatch amortization and makes the CPU part of the scheduling loop.

6. **Several accelerated WGSL world helper paths still emit unconditional linear loops over `shape_count`.**

`compiler/query_exec/wgsl/codegen.rs` still emits unconditional world loops for `world_distance_point`, `world_radiance_query`, `world_medium_point`, and the default normal path.
That means some non-primary paths still ignore the acceleration spine when acceleration data exists.

7. **Collision is still effectively CPU-only, and the narrow phase is still serial candidate iteration.**

`compiler/collision_exec/cpu.rs::resolve_backend()` only accepts CPU/Auto→CPU.
`candidate_limited_point_query()` and `candidate_limited_ray_query()` loop candidates one by one and issue per-candidate capture queries.

8. **The canonical closure lane still frames CPU as the representative backend.**

`compiler/perf_target/mod.rs` defines `canonical_1080p120()` as a CPU-oracle profile, and `benchmarks/README.md` still describes the complex representative presentation lane as CPU-backed pending WGSL closure.
That is honest today, but it means the repo still lacks a true in-tree WGSL closure target.

## Why 1080p120 Still Requires An Execution-Model Change

1920 × 1080 is 2,073,600 pixels.
At 120 FPS, that is 248,832,000 primary samples per second.

If the engine accelerates tracing but still:

- allocates a CPU object per pixel
- clones per-pixel sample vectors
- expands internal hits on the CPU
- encodes/decodes typed attachments between passes
- creates fresh buffers per dispatch
- maps a buffer after every pass
- submits once per tiny compute step

then acceleration cannot save the frame.

The failure mode is no longer “too many ray steps.”
It is “too much book-keeping around each ray step and each pass.”

So the remaining target requires a structural change:

**the WGSL lane must stop being CPU-authoritative orchestration with GPU islands, and become a GPU-authoritative framegraph with CPU oracle/debug/export side lanes.**

## Goals

1. Make the representative 1080p120 presentation closure lane a real WGSL resident-framegraph lane, not a CPU stand-in.
2. Keep steady-state scene/acceleration/cache data resident on the GPU across frames.
3. Remove hot-path CPU readback from the WGSL closure lane.
4. Remove CPU generation of per-pixel screen sample/ray structures from the WGSL closure lane.
5. Remove CPU attachment decode/encode glue between WGSL passes.
6. Reduce dispatch count from “many tiny dispatches per packet/pass/chunk” to “a small number of large dispatches per frame.”
7. Reuse the same resident scene data for collision workloads, starting with point/ray/overlap and then hybridizing sweep/TOI.
8. Preserve CPU-oracle correctness, parity tests, and explicit fallback behavior.
9. Make the bottlenecks falsifiable with GPU timestamps, upload/readback counters, and residency metrics.
10. Keep every task junior-executable: clear file targets, clear acceptance criteria, and no hidden cross-phase assumptions.

## Non-Goals

1. This RFC does not replace the shared acceleration spine from RFC 0007.
2. This RFC does not introduce hardware ray tracing as a requirement.
3. This RFC does not make subgroup operations mandatory.
4. This RFC does not make `f16` the default correctness path.
5. This RFC does not remove the CPU backend or CPU oracle tests.
6. This RFC does not require a full custom render-graph framework outside the compiler/runtime.
7. This RFC does not treat debug export and benchmark checksum readback as “bad”; it only forbids those readbacks in the timed steady-state hot path.
8. This RFC does not force sweep/TOI fully onto the GPU before parity and certification are proven.

## Design Rules

1. **CPU remains the oracle.** CPU is the semantic reference path, not the steady-state performance path.
2. **No hot-path readback in closure mode.** Readback is explicit, reason-tagged, and off the timed frame path.
3. **Static scene data is snapshot-scoped.** World shape indices, acceleration nodes, children, cache bricks, and shape metadata should upload on snapshot change, not on every dispatch.
4. **The WGSL frame path is GPU-authoritative.** Attachments live on the GPU first; CPU bytes are derived only for debug/export/tests.
5. **Do not allocate per-pixel `KernelValue` graphs in the WGSL closure lane.** Ray generation must be implicit or GPU-generated.
6. **One frame should look like one graph, not a bag of unrelated submits.** Record the frame through one primary command encoder and one queue submit unless a platform-specific exception is documented.
7. **Large batched dispatches beat many tiny dispatches.** Packet queues, candidate tables, and work compaction should reduce CPU scheduling overhead, not add to it.
8. **Acceleration must survive lowering.** If the planner has an acceleration forest, the generated WGSL world path should not quietly fall back to unconditional linear loops when an accelerated path is possible.
9. **Structured attachments may stay as buffers.** Do not turn everything into textures by reflex.
10. **Texture-backed attachments must justify themselves.** Use textures where sampling, filtering, or presentation makes them the better fit.
11. **Request only the limits and features actually needed.** Features such as timestamps and `f16` are optional and explicitly surfaced in reports.
12. **Subgroups remain optional.** Any subgroup specialization must remain behind feature detection and must never be required for closure.
13. **No flag day migration.** Land the resident path alongside the old path, prove parity, then quarantine/remove the legacy CPU-bounce helpers.
14. **Collision is a first-class consumer.** Shared residency and batched dispatch work must be reusable by collision, not presentation-only.
15. **Reports must explain execution-model failures explicitly.** The “why not 120?” story should call out readback churn, upload churn, CPU primary setup, and dispatch fragmentation.
16. **Closure is measured on GPU time and CPU submission time separately.** One number is not enough.

## Architecture Overview

The runtime shape for the closure path should look like this.

1. **Semantic authority**
   - Scene IR, query contracts, presentation contracts, collision contracts, and semantic evidence remain the public meaning of the world.
   - The GPU-resident path is a lowering and execution strategy, not a new semantic model.

2. **Shared GPU runtime substrate**
   - a single adapter/device/profile owner
   - feature and limits selection
   - upload arena / staging reuse
   - timestamp profiler
   - explicit readback lane
   - stable bind-group and pipeline layout registry

3. **Snapshot-scoped resident scene**
   - GPU mirrors of world shape indices, acceleration nodes, acceleration children, cache bricks, shape metadata, and other snapshot-scoped query inputs
   - keyed by snapshot identity + relevant layout signature + detail level + feature mask
   - reused by presentation and collision

4. **GPU-resident attachment arena**
   - presentation attachments live as storage buffers or textures
   - history is GPU-resident
   - CPU bytes are a derived export form, not the primary storage form for the WGSL lane

5. **GPU query dispatcher**
   - accepts dynamic item buffers and small per-pass constants
   - references persistent scene bind groups
   - returns GPU buffer handles for downstream passes
   - aggregates observability into a compact metrics buffer

6. **Presentation framegraph**
   - ray generation
   - primary visibility
   - hit materialization / depth / normal writeout
   - surface resolve
   - participant resolve
   - shading
   - temporal/history update
   - composite / present
   all recorded as one dependency-aware frame execution

7. **Collision batch engine**
   - same resident scene buffers
   - batch ABI for point/ray/overlap first
   - hybrid GPU candidate filtering + CPU certification for sweep/TOI until stronger proof exists

8. **Debug/export/readback lane**
   - explicit
   - reason-tagged
   - allowed in tests, debug commands, checksums, and offline captures
   - forbidden in steady-state timed closure frames

### New shared types

The first implementation step should introduce shared internal types that make residency and GPU-owned results explicit.

**Illustrative logical sketch**

```rust
pub const GPU_RUNTIME_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GpuResidentSceneKey {
    pub snapshot: crate::world_identity::SnapshotIdentityReport,
    pub detail: i32,
    pub layout_signature: u64,
    pub feature_mask: u64,
}

#[derive(Debug)]
pub struct GpuResidentScene {
    pub key: GpuResidentSceneKey,
    pub world_shapes: wgpu::Buffer,
    pub accel_nodes: wgpu::Buffer,
    pub accel_children: wgpu::Buffer,
    pub shape_meta: wgpu::Buffer,
    pub cache_bricks: wgpu::Buffer,
    pub bind_group_scene: wgpu::BindGroup,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuAttachmentStorageKind {
    StorageBuffer,
    StorageTexture,
    SampledTexture,
    RenderTargetTexture,
}

#[derive(Debug)]
pub struct GpuAttachmentHandle {
    pub name: SmolStr,
    pub kind: GpuAttachmentStorageKind,
    pub width: u32,
    pub height: u32,
    pub format_tag: SmolStr,
}

#[derive(Debug)]
pub struct GpuAttachmentArena {
    pub width: u32,
    pub height: u32,
    pub attachments: BTreeMap<SmolStr, GpuAttachmentHandle>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuQueryBufferHandle {
    pub abi_signature: u64,
    pub item_count: u32,
    pub buffer_id: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadbackReason {
    DebugExport,
    BenchmarkChecksum,
    ValidationProbe,
    TimestampResolve,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct GpuExecutionChurnCounters {
    pub upload_bytes: u64,
    pub readback_bytes: u64,
    pub scene_reupload_bytes: u64,
    pub transient_buffer_creations: u32,
    pub transient_bind_group_creations: u32,
    pub queue_submit_count: u32,
    pub cpu_screen_sample_allocations: u32,
    pub attachment_decode_count: u32,
    pub attachment_encode_count: u32,
}
```

This sketch is illustrative, not normative.
The normative split is:

- shared GPU runtime types live in a shared internal module
- presentation and collision consume those types
- the old CPU-owned `AttachmentResourceSet` remains valid for CPU execution and debug materialization
- WGSL resident execution gets explicit GPU handles instead of implicit `Vec<KernelValue>` round-trips

## Performance Closure Contract

RFC 0007 already introduced the idea of a typed closure profile.
RFC 0008 extends it with execution-model invariants.

The representative frame closure contract should now distinguish:

- CPU wall-clock frame time
- GPU frame time
- CPU submit/setup time
- hot-path readback bytes
- steady-state scene upload bytes
- transient buffer creation count
- queue submit count
- attachment decode/encode counts
- CPU screen-sample allocation count
- dispatch count
- dispatch workgroups
- enabled optional features (`timestamps`, `f16`, `subgroup`, indirect dispatch)

For the WGSL resident closure lane, the contract should add these invariants:

- **steady-state hot-path readback bytes per frame = 0**
- **steady-state scene upload bytes per frame = 0**
- **CPU screen-sample allocations per frame = 0**
- **attachment decode/encode count in the timed WGSL lane = 0**
- **steady-state queue submit count per frame <= 1** unless a documented exception is active
- **GPU timestamps are present** when the adapter supports them
- **CPU oracle companion lane remains green** for parity and regression checks

The engine should not claim “WGSL 1080p120 closure” until the representative frame lane meets both sets of conditions:

1. timing/budget closure against the named profile
2. execution-model closure against the no-readback/no-reupload/no-CPU-glue invariants above

Collision closure remains coupled to the same named profile family.
A frame lane win does not count as success if the collision lane regresses badly.

## Phase Overview

- **Phase 42** — truthful WGSL timing, churn metrics, and real closure lanes
- **Phase 43** — persistent scene residency, upload discipline, and stable layouts
- **Phase 44** — GPU frame inputs, primary visibility output residency, and framegraph scaffolding
- **Phase 45** — dispatch amortization and WGSL world-path closure
- **Phase 46** — post-visibility GPU pass closure and bandwidth tuning
- **Phase 47** — collision throughput closure on shared GPU data
- **Phase 48** — closure gates, parity, documentation, and legacy-path cleanup

---

# Phase 42: Truthful WGSL Timing, Churn Metrics, And Real Closure Lanes

## Goal

Stop measuring the wrong thing and stop hiding execution-model problems behind CPU-oracle closure.

## Why this phase exists

The repo cannot intelligently optimize the resident GPU path until it can answer three questions with evidence:

1. how much time the GPU actually spends per pass
2. how much CPU work is spent feeding or stalling the GPU
3. whether the canonical 1080p120 closure lane is testing the path we actually intend to ship

### Workstream A: GPU timing and churn observability

#### Task 42A1 — Add pass-scope GPU timestamp profiling

**Description**

Add an adapter-aware GPU profiler that records pass-scope timestamps for the WGSL frame lane and the WGSL query lane.
CPU wall-clock timings should remain, but they must stop pretending to be the whole story.

**Files**

- new `compiler/gpu_runtime/mod.rs`
- new `compiler/gpu_runtime/device.rs`
- new `compiler/gpu_runtime/profiler.rs`
- `compiler/query_exec/wgsl.rs`
- `compiler/presentation_exec/wgsl.rs`
- `compiler/presentation_exec/cost.rs`
- `compiler/bin/wrela/perf_engine.rs`

**Implementation notes**

Recommended steps:

1. centralize adapter/device creation for the WGSL runtime
2. request `TIMESTAMP_QUERY` only when supported
3. allocate one query set per frame-in-flight or one reusable ring of query sets
4. record begin/end timestamps around every presentation pass and around every top-level query dispatch batch
5. resolve timestamps at the end of the frame into a dedicated query-resolve buffer
6. read those timestamp results back through the explicit readback lane, not ad hoc pass code
7. surface both CPU and GPU timings in pass and frame reports

Do not fail the backend on adapters without timestamp support.
Instead, expose `gpu_timestamps_available=false` and keep CPU timing as fallback.

**Code sketch**

```rust
pub struct GpuPassProfiler {
    pub timestamps_supported: bool,
    pub query_set: Option<wgpu::QuerySet>,
    pub resolve_buffer: Option<wgpu::Buffer>,
    pub next_query: u32,
}

impl GpuPassProfiler {
    pub fn begin_compute_pass<'a>(
        &'a mut self,
        encoder: &'a mut wgpu::CommandEncoder,
        label: &'a str,
    ) -> (wgpu::ComputePass<'a>, Option<(u32, u32)>) {
        let timestamp_writes = self.allocate_pair().map(|(start, end)| {
            wgpu::ComputePassTimestampWrites {
                query_set: self.query_set.as_ref().unwrap(),
                beginning_of_pass_write_index: Some(start),
                end_of_pass_write_index: Some(end),
            }
        });
        let pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some(label),
            timestamp_writes,
        });
        (pass, self.last_allocated_pair())
    }
}
```

**Acceptance criteria**

- GPU pass timing is reported separately from CPU wall-clock timing when the adapter supports timestamps.
- The report clearly states whether timestamps were available.
- The profiler is shared across query and presentation WGSL execution.
- No pass performs ad hoc timestamp management on its own.
- Perf output can show “GPU-bound” versus “CPU submission/readback bound.”

#### Task 42A2 — Add upload/readback/churn counters and new “why not 120?” findings

**Description**

Extend observability so the report can say “this frame is slow because it still read back 18 MB, re-uploaded static scene buffers, built 2 million CPU screen samples, and issued 96 tiny dispatches.”

**Files**

- new `compiler/gpu_runtime/metrics.rs`
- `compiler/query_exec/mod.rs`
- `compiler/query_exec/wgsl.rs`
- `compiler/presentation_exec/cost.rs`
- `compiler/presentation_exec/mod.rs`
- `compiler/presentation_exec/wgsl.rs`
- `compiler/acceleration/report.rs`
- `compiler/perf_target/mod.rs`

**Implementation notes**

Add counters for at least:

- upload bytes
- readback bytes
- scene re-upload bytes
- transient buffer creations
- transient bind group creations
- queue submit count
- dispatch count
- CPU screen-sample allocations
- attachment decode count
- attachment encode count
- primary-visibility packet fanout count
- per-frame pipeline cache misses/warmup hits if available

Then add new closure findings such as:

- `cpu_gpu_churn`
- `steady_state_scene_reupload`
- `cpu_primary_setup`
- `attachment_cpu_bounce`
- `dispatch_fragmentation`

Treat `generate_screen_samples()`, `expand_internal_hits()`, `materialize_primary_visibility_attachments()`, `decode_attachment()`, and hot-path `readback_storage_buffer()` as first-class observability sources.

**Acceptance criteria**

- `PresentationFrameCostReport` and related perf output include the new counters.
- `--why-not-120` can name CPU↔GPU churn and CPU primary setup as first-class failure modes.
- Reports identify whether a frame was blocked by GPU work, CPU setup, readback stalls, or dispatch fragmentation.
- A junior engineer can point to one report and know where to start.

### Workstream B: Real closure lanes

#### Task 42B1 — Add a true WGSL resident-framegraph closure profile and keep CPU as companion oracle

**Description**

Add a canonical closure profile that is explicitly for the resident WGSL frame path.
The CPU-oracle profile remains in-tree, but it is no longer the representative “we hit 1080p120” story for presentation.

**Files**

- `compiler/perf_target/mod.rs`
- `compiler/bin/wrela/perf_engine.rs`
- `benchmarks/README.md`
- `benchmarks/realtime_presentation/1080p120_closure.toml`
- `benchmarks/collision_perf/1080p120_closure.toml`

**Implementation notes**

Recommended profile split:

- `canonical_1080p120_cpu_oracle`
- `canonical_1080p120_wgsl_resident`

The WGSL profile should record:

- adapter name
- enabled optional features
- requested limits profile
- whether timestamps are enabled
- whether `f16` is enabled
- whether indirect dispatch is enabled
- warmup protocol for pipelines and resident scene upload

Do not time first-use shader compilation or first-use scene upload in the steady-state measured lane.
Warm those outside the measured window.

**Acceptance criteria**

- The repo has an in-tree named WGSL resident closure profile.
- Perf commands can run the representative presentation lane against that profile.
- Reports clearly distinguish CPU-oracle closure from WGSL resident closure.
- Benchmark docs no longer imply that CPU is the representative final backend for the complex closure lane.

## Phase 42 exit criteria

- GPU timestamps are available in reports when the adapter supports them.
- New churn counters exist and are visible in frame reports.
- `--why-not-120` can diagnose CPU↔GPU churn and CPU primary setup.
- At least one named WGSL resident closure profile exists in-tree.

---

# Phase 43: Persistent Scene Residency, Upload Discipline, And Stable Layouts

## Goal

Stop re-uploading static scene/acceleration data and stop recreating GPU objects that should persist.

## Why this phase exists

RFC 0007 already built the shared acceleration forest.
The current WGSL path still rebuilds and re-uploads derived scene data every dispatch, which throws away most of the benefit.

### Workstream A: Resident scene data

#### Task 43A1 — Introduce `GpuResidentScene` keyed by snapshot identity and layout signature

**Description**

Build a persistent GPU mirror for snapshot-scoped scene data and make both presentation and collision query execution consume it.

**Files**

- new `compiler/gpu_runtime/resident_scene.rs`
- new `compiler/gpu_runtime/layout.rs`
- `compiler/query_exec/context.rs`
- `compiler/query_exec/wgsl.rs`
- `compiler/acceleration/mod.rs`
- `compiler/acceleration/build.rs`

**Implementation notes**

The resident scene should at minimum mirror:

- world shape indices
- acceleration nodes
- acceleration children
- shape metadata
- cache bricks
- any other static query-lowering buffers used across many dispatches

Key the cache by:

- snapshot identity report
- detail level
- relevant layout signature
- optional feature mask

Only re-upload when that key changes.
Do not key it by “this dispatch happened.”

Provide explicit accessors like:

- `resident_world_scene(capture, detail)`
- `resident_shape_scene(shape)`
- `resident_region_scene(region, detail)`

**Code sketch**

```rust
pub struct GpuResidentSceneCache {
    entries: HashMap<GpuResidentSceneKey, Arc<GpuResidentScene>>,
}

impl GpuResidentSceneCache {
    pub fn get_or_create_world(
        &mut self,
        gpu: &GpuRuntime,
        ctx: &QueryExecContext,
        capture: &SmolStr,
        detail: i32,
        layout_signature: u64,
    ) -> Result<Arc<GpuResidentScene>, QueryExecError> {
        let key = GpuResidentSceneKey {
            snapshot: ctx
                .snapshot_report_for_capture_name(capture)
                .expect("snapshot report"),
            detail,
            layout_signature,
            feature_mask: gpu.feature_mask(),
        };
        if let Some(entry) = self.entries.get(&key) {
            return Ok(entry.clone());
        }
        let built = Arc::new(build_gpu_resident_scene(gpu, ctx, capture, detail, &key)?);
        self.entries.insert(key, built.clone());
        Ok(built)
    }
}
```

**Acceptance criteria**

- Static scene/acceleration/cache buffers are uploaded once per snapshot/layout change, not once per dispatch.
- WGSL query execution can reference a persistent scene bind group.
- Frame reports can distinguish first-use upload from steady-state reuse.
- In steady-state representative frames, `scene_reupload_bytes == 0`.

#### Task 43A2 — Add a reusable upload arena for dynamic per-frame and per-dispatch data

**Description**

Replace repeated `create_buffer_init()` calls for tiny dynamic payloads with a reusable upload strategy.

**Files**

- new `compiler/gpu_runtime/upload.rs`
- `compiler/query_exec/wgsl.rs`
- `compiler/presentation_exec/wgsl.rs`

**Implementation notes**

Dynamic data still exists:

- dispatch config structs
- ray/query item arrays
- frame constants
- per-pass compacted index buffers
- indirect-dispatch argument buffers if used later

Use a reusable upload arena.
The implementation may use a `StagingBelt`, a ring of upload buffers, or a similar sub-allocation strategy.

Rules:

- do not use long-lived mapped buffers as the hot-path scene buffers
- do not create one fresh GPU buffer per tiny write
- expose alignment helpers so junior engineers do not hand-roll offsets
- allow explicit lifetime boundaries per frame

**Code sketch**

```rust
pub struct FrameUploadArena {
    pub staging_belt: wgpu::util::StagingBelt,
    pub scratch_encoder: Option<wgpu::CommandEncoder>,
}

impl FrameUploadArena {
    pub fn write_storage_bytes(
        &mut self,
        device: &wgpu::Device,
        target: &wgpu::Buffer,
        offset: u64,
        bytes: &[u8],
    ) {
        let encoder = self.scratch_encoder.as_mut().expect("frame encoder");
        let mut view = self.staging_belt.write_buffer(
            encoder,
            target,
            offset,
            wgpu::BufferSize::new(bytes.len() as u64).unwrap(),
            device,
        );
        view.copy_from_slice(bytes);
    }
}
```

**Acceptance criteria**

- Hot-path dynamic uploads no longer create a fresh target buffer per small write.
- Upload bytes are still counted, but transient buffer creation count falls sharply.
- The upload arena is shared by query and presentation runtime code.
- No junior engineer needs to hand-manage staging-buffer lifetime in pass code.

### Workstream B: Stable layouts and pipeline reuse

#### Task 43B1 — Freeze bind-group layouts by update frequency and warm pipelines before timed runs

**Description**

Define stable layout groups so the runtime reuses bind groups and pipelines instead of rebuilding them in the inner loop.

**Files**

- new `compiler/gpu_runtime/pipeline_cache.rs`
- new `compiler/gpu_runtime/bindings.rs`
- `compiler/query_exec/wgsl.rs`
- `compiler/presentation_exec/wgsl.rs`
- `compiler/presentation_exec/mod.rs`
- `compiler/bin/wrela/perf_engine.rs`

**Implementation notes**

Use a consistent split such as:

- **Group 0** — scene-static resident buffers
- **Group 1** — frame constants / camera / temporal constants
- **Group 2** — pass-local IO attachments
- **Group 3** — optional scratch / continuation / indirect args

Benefits:

- scene bind groups can persist across many frames
- frame bind groups change once per frame
- pass-local bind groups change only when attachments or compacted work buffers change

Also centralize optional feature and limits selection here.
Request only what is needed.
For example:

- timestamps only if supported and enabled
- `f16` only in explicit experimental/perf lanes
- subgroup features never required for closure

Warm all pipelines and bind-group layouts outside the measured closure interval.

**Acceptance criteria**

- Query and presentation WGSL code share stable bind-group layout conventions.
- Bind-group creation is no longer on the inner dispatch loop for steady-state frames.
- Timed closure runs do not include first-use pipeline compilation.
- Reports surface which optional features and limits profile were actually enabled.

## Phase 43 exit criteria

- Static scene data is resident across frames.
- Dynamic uploads use a reusable arena instead of fresh per-dispatch tiny buffers.
- Stable bind-group layout conventions exist and are shared.
- Steady-state representative frames show zero static scene re-uploads.

---

# Phase 44: GPU Frame Inputs, Primary Visibility Output Residency, And Framegraph Scaffolding

## Goal

Remove CPU-owned primary-view setup and primary-output glue from the WGSL lane.

## Why this phase exists

The current WGSL path still spends a large amount of CPU time building screen sample values, cloning them, expanding hits, and materializing attachments.
That overhead is large enough to erase acceleration wins even before later shading passes run.

### Workstream A: Ray generation and primary-output writeout

#### Task 44A1 — Replace CPU `generate_screen_samples()` with implicit or GPU-generated ray setup

**Description**

Stop building one `ScreenSampleQuery` `KernelValue` per pixel in the WGSL closure lane.
Generate rays implicitly from pixel index and camera uniforms, or run a dedicated GPU raygen pass when later passes truly need reusable per-sample records.

**Files**

- `compiler/presentation_exec/mod.rs`
- `compiler/presentation_exec/wgsl.rs`
- new `compiler/presentation_exec/gpu_primary.rs`
- `compiler/presentation_plan/mod.rs`

**Implementation notes**

Preferred order:

1. make primary visibility compute ray direction directly from invocation id + camera constants
2. only materialize a ray buffer when a later pass genuinely needs it
3. keep CPU `generate_screen_samples()` for the CPU backend and for tests that intentionally inspect CPU sample values

Do not encode `ScreenSampleQuery` as a giant CPU vector on the WGSL closure path.

If internal resolution differs from output resolution, the primary kernel should use internal-resolution coordinates directly.
Do not clone or rescale CPU sample arrays for that case.

**Code sketch**

```wgsl
struct CameraUniforms {
  position: vec3<f32>,
  _pad0: f32,
  forward: vec3<f32>,
  _pad1: f32,
  right: vec3<f32>,
  _pad2: f32,
  up: vec3<f32>,
  vertical_fov_degrees: f32,
  viewport_width: u32,
  viewport_height: u32,
  frame_index: u32,
  jitter_x: f32,
  jitter_y: f32,
};

@group(1) @binding(0)
var<uniform> camera: CameraUniforms;

fn ray_for_pixel(pixel: vec2<u32>) -> RayQuery {
  let uv = (vec2<f32>(pixel) + vec2<f32>(0.5 + camera.jitter_x, 0.5 + camera.jitter_y))
      / vec2<f32>(f32(camera.viewport_width), f32(camera.viewport_height));
  // project uv -> view ray using camera.forward/right/up and fov
  // return RayQuery without any CPU-built KernelValue scaffolding
}
```

**Acceptance criteria**

- WGSL closure mode does not call `generate_screen_samples()` for steady-state presentation frames.
- CPU screen-sample allocation count is zero in the timed WGSL lane.
- Primary visibility can execute from camera/frame uniforms plus invocation id.
- CPU sample generation remains available only for CPU backend and test/debug code.

#### Task 44A2 — Replace CPU `expand_internal_hits()` and `materialize_primary_visibility_attachments()` with GPU passes

**Description**

Move primary hit expansion and primary attachment materialization onto the GPU.

**Files**

- `compiler/presentation_exec/mod.rs`
- `compiler/presentation_exec/wgsl.rs`
- new `compiler/presentation_exec/gpu_primary.rs`
- `compiler/presentation_exec/resources.rs`

**Implementation notes**

There are two distinct jobs here:

1. **internal-resolution expansion**
   - when primary visibility runs at reduced internal resolution, upsample or scatter into output-space attachments on the GPU
   - this may be a dedicated compute pass or folded into later shading/composite depending on the chosen contract

2. **primary attachment materialization**
   - write primary hit, depth, and world-normal attachments directly from GPU result buffers
   - misses should be handled in shader logic, not CPU loops over attachment bytes

Keep the CPU helpers for the CPU backend and for oracle tests.
The WGSL closure lane should not call them.

**Acceptance criteria**

- WGSL closure mode does not call `expand_internal_hits()` in timed frames.
- WGSL closure mode does not call `materialize_primary_visibility_attachments()` in timed frames.
- Primary hit, depth, and world-normal attachments are written on the GPU.
- Reports can attribute the GPU cost of primary writeout as its own pass.

### Workstream B: Attachment residency and framegraph scaffolding

#### Task 44B1 — Add `GpuAttachmentArena` and make WGSL attachments GPU-resident by default

**Description**

Introduce a GPU-owned attachment arena for the WGSL closure lane.
The current `AttachmentResourceSet` CPU bytes remain useful for CPU execution and debug materialization, but they should not remain the primary storage for the WGSL lane.

**Files**

- new `compiler/presentation_exec/gpu_resources.rs`
- `compiler/presentation_exec/resources.rs`
- `compiler/presentation_exec/wgsl.rs`
- `compiler/presentation_exec/debug.rs`

**Implementation notes**

Not every attachment should be stored the same way.

Recommended policy:

- storage buffers for structured records such as `Hit3`, `Surface`, `Medium`
- textures for final color, sampled history, or places where texture sampling is useful
- explicit format policy per attachment kind

Add a storage abstraction such as:

- `CpuDenseBytes`
- `GpuBuffer`
- `GpuTexture`
- `Mirrored` for debug/testing when both are needed intentionally

Do not let pass code silently materialize CPU bytes just because that is convenient.

**Code sketch**

```rust
pub enum AttachmentBacking {
    CpuDenseBytes(Vec<u8>),
    GpuBuffer {
        buffer: wgpu::Buffer,
        element_stride: u32,
    },
    GpuTexture {
        texture: wgpu::Texture,
        view: wgpu::TextureView,
        format: wgpu::TextureFormat,
    },
    Mirrored {
        cpu: Vec<u8>,
        gpu: wgpu::Buffer,
        element_stride: u32,
    },
}

pub struct GpuAttachmentArena {
    pub width: u32,
    pub height: u32,
    pub attachments: BTreeMap<SmolStr, AttachmentBacking>,
}
```

**Acceptance criteria**

- WGSL presentation code can read/write attachments without decoding CPU byte arrays.
- `decode_attachment()` is no longer on the timed WGSL steady-state path.
- Debug/export code can still materialize CPU bytes explicitly when requested.
- Attachment storage choice is explicit and inspectable in reports.

#### Task 44B2 — Add `PresentationFrameGraph` with one primary command encoder and explicit readback lane

**Description**

Make the frame execution explicit as a GPU-owned graph instead of a sequence of unrelated helper calls that each submit/read back independently.

**Files**

- new `compiler/presentation_exec/framegraph.rs`
- `compiler/presentation_exec/wgsl.rs`
- `compiler/presentation_exec/mod.rs`
- `compiler/gpu_runtime/readback.rs`

**Implementation notes**

The framegraph does not need to be a giant general-purpose engine.
It needs to do four practical things:

1. own pass ordering and resource dependencies in one place
2. record passes into one primary command encoder
3. route timestamp writes and pass metrics centrally
4. make readback explicit and reason-tagged

A simple staged API is enough.

**Code sketch**

```rust
let mut fg = PresentationFrameGraph::new(&gpu, &plan, &frame_inputs, &attachments);

let primary = fg.primary_visibility(&resident_scene, &camera_uniforms)?;
let primary_outputs = fg.materialize_primary_outputs(primary)?;
let surfaces = fg.surface_resolve(&resident_scene, primary_outputs.hit)?;
let participants = fg.participant_resolve(&resident_scene, primary_outputs.hit)?;
let shaded = fg.shade(surfaces, participants)?;
let history = fg.temporal_resolve(shaded)?;
let color = fg.composite(history)?;

let frame_result = fg.finish_and_submit()?;
```

The rule is simple:
normal closure execution should not call a raw readback helper from inside a pass implementation.

**Acceptance criteria**

- The WGSL presentation path records through a central framegraph owner.
- Steady-state representative frames use one primary queue submit unless a documented exception is active.
- All readbacks in WGSL presentation go through an explicit readback-lane API with a `ReadbackReason`.
- Timestamp profiling and pass accounting are attached to framegraph passes, not scattered helper code.

## Phase 44 exit criteria

- CPU-built per-pixel screen sample structures are gone from the WGSL closure path.
- Primary hit expansion and attachment materialization are GPU-owned in the WGSL closure path.
- WGSL attachments are GPU-resident by default.
- The frame is recorded through a central framegraph and explicit readback lane.

---

# Phase 45: Dispatch Amortization And WGSL World-Path Closure

## Goal

Turn many tiny dispatches into a small number of large dispatches, and stop generating obviously linear world helper paths when acceleration data exists.

## Why this phase exists

Even after scene residency and attachment residency land, the current primary visibility path still leaves too much performance on the floor through CPU-side packet scheduling and repeated small dispatches.

### Workstream A: Query dispatch amortization

#### Task 45A1 — Introduce a persistent `GpuQueryDispatcher` that returns GPU handles instead of immediate CPU values

**Description**

Add a new internal query-execution API for resident GPU consumers.
This API should take resident scene references plus dynamic item buffers and return GPU output handles that downstream passes can consume directly.

**Files**

- new `compiler/query_exec/gpu_dispatch.rs`
- `compiler/query_exec/wgsl.rs`
- `compiler/presentation_exec/wgsl.rs`
- `compiler/collision_exec/gpu.rs`
- `compiler/query_exec/mod.rs`

**Implementation notes**

The existing public/direct query helpers that return `Vec<KernelValue>` can stay for CPU/debug/test code.
The new internal path should return something like:

- output buffer handle
- ABI signature
- item count
- optional compacted metrics buffer handle

This is the API bridge that makes “keep rendering fully on the GPU” real.

Also shrink observability readback.
Do not read back a giant per-item structure when a compact metrics buffer or per-frame summary is enough.

**Code sketch**

```rust
pub struct GpuDispatchResult {
    pub values: GpuQueryBufferHandle,
    pub metrics: Option<GpuQueryBufferHandle>,
    pub item_count: u32,
}

pub trait ResidentGpuQueryExecutor {
    fn execute_world_batch_gpu(
        &mut self,
        resident_scene: &GpuResidentScene,
        contract_id: QueryContractId,
        item_buffer: &wgpu::Buffer,
        item_count: u32,
    ) -> Result<GpuDispatchResult, QueryExecError>;
}
```

**Acceptance criteria**

- Presentation code can execute batch queries and keep the result on the GPU.
- Collision code can use the same API for batched workloads.
- Hot-path query execution no longer requires decoding into `Vec<KernelValue>` between passes.
- Observability readback is compact and explicit.

#### Task 45A2 — Collapse packet/candidate primary visibility into batched candidate-table kernels

**Description**

Replace the current nested CPU loops over packets and candidate shapes with a batched GPU path that evaluates candidate tables and reduces best hits on the GPU.

**Files**

- `compiler/presentation_exec/wgsl.rs`
- `compiler/presentation_exec/mod.rs`
- new `compiler/presentation_exec/gpu_primary.rs`
- `compiler/query_exec/gpu_dispatch.rs`

**Implementation notes**

Today’s packetized candidate path still does too much of this shape:

- build packet queue on CPU
- for each packet
  - build packet ray list
  - for each candidate shape
    - dispatch a batch query
    - merge best hit on CPU
- dispatch a fallback batch for uncovered samples

The replacement path should instead flatten candidate data into GPU-friendly arrays such as:

- `packet_offsets`
- `packet_lengths`
- `packet_candidate_shapes`
- `sample_to_packet`
- optional `active_sample_indices`

Then run one or a very small number of kernels that:

1. load the candidate range for a packet/sample
2. evaluate candidates
3. reduce nearest hit
4. write the best hit directly to the GPU hit attachment/buffer

Optional later improvement:
if supported and worthwhile, allow an earlier compaction pass to write an indirect-dispatch buffer for later candidate kernels.

**Illustrative WGSL sketch**

```wgsl
@group(2) @binding(0) var<storage, read> packet_offsets: array<u32>;
@group(2) @binding(1) var<storage, read> packet_lengths: array<u32>;
@group(2) @binding(2) var<storage, read> packet_candidates: array<u32>;
@group(2) @binding(3) var<storage, read> active_samples: array<u32>;
@group(2) @binding(4) var<storage, read_write> primary_hits: array<Hit3>;

@compute @workgroup_size(64)
fn primary_visibility_candidates(@builtin(global_invocation_id) gid: vec3<u32>) {
  let work_index = gid.x;
  if (work_index >= active_sample_count) { return; }

  let sample_index = active_samples[work_index];
  let packet_id = sample_to_packet(sample_index);
  let start = packet_offsets[packet_id];
  let len = packet_lengths[packet_id];

  let ray = ray_for_sample(sample_index);
  var best = default_hit(ray.origin);

  for (var i: u32 = 0u; i < len; i = i + 1u) {
    let shape_index = packet_candidates[start + i];
    let hit = trace_world_shape_candidate(shape_index, ray, 0.0);
    if (hit.hit && (!best.hit || hit.distance < best.distance)) {
      best = hit;
    }
  }

  primary_hits[sample_index] = best;
}
```

**Acceptance criteria**

- The WGSL primary visibility path no longer performs per-packet, per-shape query dispatch loops on the CPU.
- Dispatch count for primary visibility becomes O(pass stages), not O(packets × candidates).
- Fallback work is still correct, but is no longer the default scheduling structure.
- Reports surface packet compaction ratio and candidate-table effectiveness.

### Workstream B: WGSL world-path closure

#### Task 45B1 — Stop generating unconditional `shape_count` loops for accelerated world distance/normal/radiance/media paths

**Description**

Finish the WGSL acceleration story by ensuring the generated world helper paths do not quietly become linear scans when acceleration data is available.

**Files**

- `compiler/query_exec/wgsl/codegen.rs`
- `compiler/query_exec/wgsl.rs`
- `compiler/tests/query_exec_wgsl.rs`
- `compiler/tests/presentation_exec.rs`

**Implementation notes**

There are different cases here:

- **distance / trace**
  - should use the acceleration forest or candidate list when present

- **normal at a confirmed hit**
  - should prefer the hit root shape or a small local candidate set instead of unconditional world scan

- **radiance / media**
  - should accumulate over a filtered candidate set derived from the acceleration structure or hit-local context, not unconditional `0..shape_count`

Always keep dense fallback available when semantics or planner evidence require it.
The bug to fix is not “linear loops are always illegal.”
It is “linear loops are silently used even when the engine already has better structural data.”

**Acceptance criteria**

- Generated WGSL for accelerated representative scenes no longer emits unconditional `for index in 0..shape_count` helpers for world distance, radiance, or medium queries when an accelerated path is available.
- CPU/WGSL parity tests still pass.
- Reports can say whether an accelerated world helper path or dense fallback path was selected.

## Phase 45 exit criteria

- Presentation and collision can consume GPU query outputs directly.
- Primary visibility no longer fans out into CPU-scheduled packet/candidate micro-dispatches.
- Accelerated WGSL world helper paths stop silently falling back to unconditional world scans.
- Dispatch fragmentation becomes visible and materially lower in representative traces.

---

# Phase 46: Post-Visibility GPU Pass Closure And Bandwidth Tuning

## Goal

Finish the resident framegraph after primary visibility: no CPU attachment glue, no CPU participant work-item construction, and a deliberate attachment bandwidth policy.

## Why this phase exists

The current WGSL lane still pays CPU cost after visibility:

- surface resolve writes CPU attachment bytes
- participant resolve builds CPU work-item vectors
- shading decodes CPU attachments and writes them back
- temporal resolve mutates history via CPU bytes

That must stop for the frame to stay GPU-resident end to end.

### Workstream A: Post-visibility pass conversion

#### Task 46A1 — Move surface resolve and participant resolve fully onto GPU attachments

**Description**

Rewrite surface resolve and participant resolve so they read GPU hit buffers/attachments and write GPU surface/radiance/medium attachments directly.

**Files**

- `compiler/presentation_exec/wgsl.rs`
- new `compiler/presentation_exec/gpu_post.rs`
- `compiler/presentation_exec/mod.rs`
- `compiler/presentation_exec/resources.rs`

**Implementation notes**

Surface resolve should no longer:

- read hits into CPU values
- call `encode_values_at_indices()` on CPU attachments
- scatter default surfaces through CPU loops

Participant resolve should no longer:

- call `participant_query_work_items()` in the WGSL closure lane
- build CPU `PointQuery`/`PointDirectionQuery` arrays for later GPU work

Instead:

- compact work on the GPU
- derive query points and directions on the GPU from hit buffers, miss policy, and camera constants
- write results directly to GPU attachments

Keep the CPU helper implementations for CPU backend/oracle usage.

**Acceptance criteria**

- WGSL closure mode does not use `encode_values_at_indices()` for surface/radiance/medium writes in timed frames.
- WGSL closure mode does not use `participant_query_work_items()` in timed frames.
- Surface/radiance/medium attachments are written and consumed on the GPU.
- Pass reports show separate GPU costs for surface resolve and participant resolve.

#### Task 46A2 — Move shading, composite, temporal resolve, and history update fully onto the GPU

**Description**

Rewrite the remaining post-visibility passes so they consume resident attachments and produce resident outputs without CPU decode/repack.

**Files**

- `compiler/presentation_exec/wgsl.rs`
- new `compiler/presentation_exec/gpu_post.rs`
- `compiler/presentation_exec/temporal.rs`
- `compiler/presentation_exec/gpu_resources.rs`

**Implementation notes**

Specific cleanup targets:

- `shade_primary_wgsl`
- `composite_color_wgsl`
- `temporal_resolve_wgsl`

Those functions should become pass recorders, not CPU data mungers.

History handling rules:

- history stays GPU-resident
- ping-pong or copy-based update policy is explicit
- debug/export readback of history is explicit and out-of-band

Final output rules:

- final color remains on the GPU until present/export
- if an output image or checksum is needed, that is a separate explicit export/readback pass

**Acceptance criteria**

- WGSL closure mode no longer calls `decode_attachment()` in shading/composite/temporal pass code.
- History update does not mutate CPU attachment bytes in timed frames.
- Final color remains GPU-resident until an explicit export/present step.
- The framegraph owns shading/composite/temporal pass ordering and metrics.

### Workstream B: Bandwidth, format, and occupancy policy

#### Task 46B1 — Add an explicit attachment format policy, optional `f16` lane, and post-residency workgroup retuning

**Description**

Once attachments and passes are GPU-resident, define which attachments need full `f32`, which can optionally use `f16`, and which should be buffer-backed versus texture-backed.
Then retune workgroup sizes and transient-store policy under the new resident path.

**Files**

- `compiler/presentation_contract/mod.rs`
- `compiler/presentation_exec/resources.rs`
- `compiler/presentation_exec/mod.rs`
- `compiler/presentation_exec/gpu_resources.rs`
- `compiler/presentation_exec/gpu_post.rs`
- `compiler/tests/presentation_exec.rs`

**Implementation notes**

Recommended default policy:

- **keep `f32`**
  - distances
  - hit data
  - collision-critical numeric values
  - anything where parity is sensitive

- **candidate optional `f16` lane**
  - color/history/radiance accumulators
  - maybe selected normal/aux attachments if tests prove tolerances acceptable

- **buffer-backed**
  - structured records like `Hit3`, `Surface`, `Medium`

- **texture-backed**
  - final color
  - sampled history
  - any attachment that later passes sample spatially

Also revisit workgroup sizes only after the resident framegraph is in place.
Do not tune workgroup sizes against the old CPU-bounce path and assume the result transfers.

If a transient render attachment is not read after a pass, explicitly allow discard-style store behavior where the backend supports it.

**Acceptance criteria**

- Attachment storage kind and precision are explicit policy, not hidden implementation accidents.
- The repo can run the WGSL resident lane with `f16` disabled and enabled behind a feature gate.
- Parity tests define acceptable tolerances for any reduced-precision lane.
- Workgroup-size tuning is rerun against the resident framegraph path, not copied from pre-residency numbers.

## Phase 46 exit criteria

- Surface, participant, shading, temporal, and composite passes stay GPU-resident in the timed WGSL lane.
- CPU attachment decode/encode glue is gone from the timed WGSL lane.
- Final color stays on the GPU until explicit present/export.
- Attachment precision/storage policy is explicit and test-covered.

---

# Phase 47: Collision Throughput Closure On Shared GPU Data

## Goal

Use the same resident scene data and batched WGSL execution model for collision workloads, starting with the easiest high-volume cases and keeping hybrid certification where needed.

## Why this phase exists

The renderer and collision pipeline should not each invent their own performance substrate.
The current collision path still falls back to CPU-only execution and serial candidate loops even though the repo now has the ingredients for batched GPU execution.

### Workstream A: Collision backend and batched narrow phase

#### Task 47A1 — Add a WGSL collision backend for batched point, ray, and overlap workloads

**Description**

Introduce `collision_exec/gpu.rs` and support `DispatchBackend::Wgsl` for the first wave of collision workloads: point occupancy, ray casts, and overlap-heavy bursts.

**Files**

- new `compiler/collision_exec/gpu.rs`
- `compiler/collision_exec/cpu.rs`
- `compiler/collision_plan/mod.rs`
- `compiler/query_exec/gpu_dispatch.rs`
- `compiler/tests/collision_exec.rs`

**Implementation notes**

Do not start with every collision mode at once.
Start where the batch shape is obvious and the benchmarks already exist:

- point occupancy bursts
- dense ray casts
- overlap bursts

The GPU collision backend should reuse:

- `GpuResidentScene`
- `GpuQueryDispatcher`
- shared workgroup selection rules
- shared observability and churn counters

Keep CPU as oracle and fallback.

**Code sketch**

```rust
pub struct CollisionBatchInput {
    pub query_kind: CollisionQueryKind,
    pub query_buffer: wgpu::Buffer,
    pub query_count: u32,
    pub candidate_offsets: wgpu::Buffer,
    pub candidate_lengths: wgpu::Buffer,
    pub candidate_shapes: wgpu::Buffer,
}

pub struct CollisionBatchResult {
    pub result_buffer: GpuQueryBufferHandle,
    pub metrics_buffer: Option<GpuQueryBufferHandle>,
}
```

**Acceptance criteria**

- `DispatchBackend::Wgsl` is accepted for at least point, ray, and overlap collision plans.
- The collision backend reuses resident scene data instead of rebuilding scene buffers ad hoc.
- CPU oracle and fallback behavior remain available.
- Collision perf output can show CPU versus WGSL lane comparisons.

#### Task 47A2 — Replace serial candidate loops with flattened batch evaluation and reduction

**Description**

Remove the “for candidate in candidates { execute query }” structure from collision hot paths where batching is available.

**Files**

- `compiler/collision_exec/cpu.rs`
- new `compiler/collision_exec/gpu.rs`
- `compiler/tests/collision_exec.rs`

**Implementation notes**

Flatten work into arrays such as:

- query-to-candidate offset/length
- candidate shape ids
- query payload array

Then batch-evaluate and reduce.

Examples:

- point query → minimum distance / best normal candidate
- ray query → nearest hit
- overlap query → any-hit or best-penetration rule, depending on contract

Preserve provenance:

- root feature / shape id
- witness lineage where applicable
- certified versus best-effort status

The reduction can happen on GPU or as one compact post-step, but it should not return to one CPU query per candidate.

**Acceptance criteria**

- Representative point/ray/overlap collision hot paths no longer do one CPU dispatch per candidate in WGSL mode.
- Collision observability can show candidate batch size and reduction effectiveness.
- Result provenance remains intact.

### Workstream B: Hybrid sweep/TOI and witness reuse

#### Task 47B1 — Keep sweep/TOI hybrid: GPU candidate filtering and bracket sampling, CPU certification for final answers

**Description**

Do not force the hardest collision queries fully onto the GPU too early.
Use the GPU where it is clearly helpful, but keep final certification/refinement on the CPU until parity and evidence are strong enough.

**Files**

- `compiler/collision_exec/cpu.rs`
- new `compiler/collision_exec/gpu.rs`
- `compiler/collision_plan/mod.rs`
- `benchmarks/collision_perf/1080p120_closure.toml`

**Implementation notes**

Recommended split:

- GPU:
  - broad candidate filtering
  - batch distance sampling
  - candidate interval/bracket estimation
  - coarse witness proposal

- CPU:
  - final TOI certification
  - final witness validation
  - any exact refinement path that still depends on stronger CPU tooling

Also expand witness reuse:

- previous successful candidate set
- previous successful bracket
- previous best root shape / feature
- explicit rejection reasons when reuse is invalid

This is exactly where a conservative hybrid path is better than overpromising a full GPU solution too early.

**Acceptance criteria**

- Sweep/TOI workloads can use GPU-assisted candidate filtering or bracket generation.
- Final certified results remain correct and explicit.
- Collision reports surface witness reuse hits, rejections, and CPU-certification cost.
- Collision perf benchmarks show reduced CPU work for representative sweep/TOI lanes.

## Phase 47 exit criteria

- Collision has a real WGSL backend for the highest-volume batch-friendly workloads.
- Serial candidate loops are removed from those WGSL-enabled collision workloads.
- Sweep/TOI gains a useful hybrid GPU-assisted path without weakening certification.
- Collision and presentation visibly share the same resident-scene substrate.

---

# Phase 48: Closure Gates, Parity, Documentation, And Legacy-Path Cleanup

## Goal

Make the resident framegraph and collision batch path shippable, testable, and hard to regress.

## Why this phase exists

The last 10% of a performance roadmap often dies in ambiguity:
two paths exist forever, closure is argued by anecdotes, and junior engineers cannot tell which helpers are still legal.
This phase prevents that.

### Workstream A: Hard closure gates and parity

#### Task 48A1 — Add hard closure assertions for readback, reupload, dispatch fragmentation, and timing

**Description**

Turn the new execution-model counters into actual pass/fail closure gates.

**Files**

- `compiler/perf_target/mod.rs`
- `compiler/bin/wrela/perf_engine.rs`
- `compiler/presentation_exec/cost.rs`
- `compiler/query_exec/mod.rs`
- `benchmarks/README.md`

**Implementation notes**

Recommended new closure fields or notes:

- `max_hot_path_readback_bytes_per_frame`
- `max_scene_reupload_bytes_per_frame`
- `max_cpu_screen_sample_allocations_per_frame`
- `max_attachment_cpu_bounce_count`
- `max_queue_submit_count_per_frame`
- `max_dispatch_count_primary_visibility`
- `gpu_timestamps_required_if_supported`

The exact numbers can be profile-specific.
The important part is that they become explicit and enforced.

**Acceptance criteria**

- Closure reports can fail because the timed WGSL lane still read back data, rebuilt scene buffers, or bounced attachments through CPU memory.
- Perf tooling prints those reasons directly.
- The closure story now checks both timing and execution-model health.

#### Task 48A2 — Expand CPU↔WGSL parity suites for resident attachments, optional `f16`, and collision batching

**Description**

Grow the parity harness so every resident-path optimization has a corresponding correctness check.

**Files**

- `compiler/tests/presentation_exec.rs`
- `compiler/tests/query_exec_wgsl.rs`
- `compiler/tests/collision_exec.rs`
- `compiler/tests/perf_target.rs`

**Implementation notes**

Needed parity coverage:

- primary hit/depth/world-normal attachment parity
- surface/radiance/media parity under resident attachments
- temporal/history parity
- color output parity
- collision point/ray/overlap parity
- sweep/TOI hybrid certification behavior
- `f16` tolerance lanes where enabled
- feature matrix fallbacks:
  - timestamps unavailable
  - `f16` unavailable
  - subgroup unavailable
  - indirect dispatch disabled

Do not hide precision differences.
Make them explicit and test-scoped.

**Acceptance criteria**

- Resident WGSL path has meaningful parity coverage against CPU oracle.
- Optional feature lanes have explicit fallback tests.
- Collision batching has correctness tests, not only perf tests.
- Junior engineers can run one test suite and know whether a resident-path refactor stayed safe.

### Workstream B: Documentation and cleanup

#### Task 48B1 — Add junior-facing playbooks and quarantine/remove the legacy CPU-bounce WGSL helpers

**Description**

Document the new runtime shape and make it hard for future work to drift back into CPU-owned pass glue.

**Files**

- new `docs/perf/gpu_resident_framegraph_playbook.md`
- new `docs/perf/collision_gpu_batch_playbook.md`
- `docs/perf/acceleration_playbook.md`
- `compiler/presentation_exec/wgsl.rs`
- `compiler/query_exec/wgsl.rs`

**Implementation notes**

The docs should explain:

- how the resident scene cache works
- how attachment storage choice works
- which helpers are legal in timed WGSL frames
- how to interpret timestamp/churn reports
- how to debug/export a frame without corrupting the closure lane
- how collision batching is structured
- how and when to use CPU fallback

Once the resident path is stable:

- move old immediate-readback helpers behind explicit legacy/test-only gates, or
- delete them if they are no longer needed

Examples of helpers that should not remain silently legal on the production path:

- `dispatch_linear_shader_with_chunk_limit()` as a readback-oriented convenience
- raw hot-path `readback_storage_buffer()` calls from pass code
- CPU attachment decode/encode glue in WGSL steady-state paths

**Acceptance criteria**

- The repo has a playbook for the resident framegraph path and for collision GPU batching.
- Legacy CPU-bounce WGSL helpers are either removed or clearly labeled as legacy/test-only.
- New engineers can tell which path is the production closure path and which path is oracle/debug scaffolding.

## Phase 48 exit criteria

- Closure tooling enforces both timing and execution-model invariants.
- CPU↔WGSL parity is strong enough to support resident-path refactors confidently.
- Docs explain the resident path well enough for junior engineers to work safely.
- Legacy CPU-bounce helpers are no longer the path of least resistance.

---

## Suggested implementation order inside each phase

This RFC is intentionally staged so the riskiest semantic work lands last.

A good within-phase order is:

1. instrument and make the problem undeniable
2. make static data resident
3. remove CPU primary-view setup
4. remove CPU attachment ownership
5. reduce dispatch fragmentation
6. finish post-visibility passes
7. bring collision onto the same substrate
8. harden, test, and delete the old shortcuts

That sequencing makes rollback and debugging materially easier.

## Final closure checklist

The project should only say “we got rendering and collision the rest of the way to 1080p120” once all of the following are true for a named representative WGSL closure profile:

- total frame timing meets budget
- primary visibility timing meets budget
- hot-path readback bytes are zero in steady state
- static scene re-upload bytes are zero in steady state
- CPU screen-sample allocation count is zero
- attachment decode/encode count is zero in the timed WGSL lane
- primary visibility is not driven by CPU packet/candidate micro-dispatch loops
- accelerated WGSL world helper paths are actually accelerated
- post-visibility passes stay GPU-resident
- collision point/ray/overlap use batched WGSL execution on the shared resident scene
- sweep/TOI show useful hybrid gains without weakening certification
- CPU oracle parity remains green
- docs make the new path the obvious path

That is the closure story this repo needs now.

Not a cleverer ray marcher.
A better machine.
