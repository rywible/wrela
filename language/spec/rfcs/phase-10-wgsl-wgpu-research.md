# Phase 10 WGSL / wgpu Research Notes

Status: Working notes

Author: Codex

Created: 2026-04-08

Scope: research pack for `/Users/ryanwible/projects/wrela/language/spec/rfcs/0002-field-engine-implementation-roadmap.md` phase 10 before implementation starts

## Why This Exists

Phase 10 is not "add some shaders". It is:

- lowering the existing query-plan/kernel contract to WGSL
- keeping CPU, virtual GPU, and real WGSL on the same typed dispatch/result ABI
- making layout, binding, and parity rules explicit enough to snapshot-test

That means the useful research is the subset of WGSL and `wgpu` guidance that helps us preserve the architecture already present in:

- `/Users/ryanwible/projects/wrela/compiler/query_plan/mod.rs`
- `/Users/ryanwible/projects/wrela/compiler/query_exec/vgpu.rs`
- `/Users/ryanwible/projects/wrela/compiler/portable/abi.rs`
- `/Users/ryanwible/projects/wrela/compiler/tests/portable_abi.rs`

## Current Repo Read

The repo is already pointed in the right direction for phase 10:

- `portable_abi_layout` already matches WGSL-style alignment rules for `vec3`, `mat3`, arrays, and record padding.
- dispatch/result contracts already exist as compiler-owned types instead of ad hoc GPU structs.
- the virtual GPU lane already exercises the same query-plan stages we will need to preserve in WGSL.
- the roadmap explicitly wants WGSL to be a backend for phase 9's portable kernel subset, not a second authored scene path.

The best phase-10 patterns are therefore the ones that strengthen that existing architecture, not the ones that bypass it.

## Sourced Findings

### 1. Storage-buffer-first ABI is the safest default for phase 10

WGSL's `storage` layout is the baseline layout model. `uniform` is stricter, and `wgpu`'s downlevel defaults keep uniform bindings much smaller than storage bindings. For query records, hit records, artifact tables, and bulk results, this strongly favors storage buffers as the primary ABI surface.

Implication for Wrela:

- use storage buffers for dispatch records, result buffers, culling tables, support summaries, and accelerator data
- reserve uniforms for very small immutable control blocks only if they materially simplify a kernel
- do not design the core ABI around uniform buffers

Why this matches the repo:

- the current portable ABI layer already thinks in explicit record layouts, which maps naturally to storage buffers
- phase 10 wants batch/query kernels first, and those are buffer-heavy rather than uniform-heavy

### 2. Host-shareable layout rules must stay compiler-owned and testable

WGSL layout rules are exact enough that we should keep the compiler as the single source of truth for:

- field offsets
- array stride
- struct size
- required alignment
- record versioning

Important consequences:

- `vec3` is size 12 but alignment 16
- `mat3` is size 48 with 16-byte column spacing
- arrays use stride, not raw element size
- storage and uniform layout constraints differ

Implication for Wrela:

- keep `compiler/portable/abi.rs` as the canonical ABI oracle
- generate WGSL structs and host encoders from that oracle instead of hand-maintained mirrors
- snapshot-test WGSL layout text and Rust-side layout metadata from the same source

### 3. Rust host mirrors should avoid raw `bool` and casual `vec3`

WGSL allows `bool` in host-shareable layouts, but Rust `bool` is not `bytemuck::Pod`, and `bytemuck::Pod` also rejects any type with padding bytes. That makes "just mirror the WGSL struct with a Rust struct and cast it" surprisingly fragile.

Practical implication:

- ABI-facing booleans should be encoded as `u32` on the host side, or packed/unpacked manually
- ABI-facing `vec3` values should usually be represented as padded 16-byte lanes, not naked three-float Rust fields
- avoid relying on `#[repr(C)]` alone to make GPU structs safe

For this repo specifically:

- the current portable ABI code already models WGSL layout explicitly, so we should prefer generated pack/unpack helpers over hand-written `Pod` mirrors for complex records like `Hit3`
- `bytemuck` is still useful for tightly controlled plain records, but it should sit behind the portable ABI contract rather than replace it

### 4. Use explicit pipeline layouts, not inferred default layouts

`wgpu` supports default pipeline layouts inferred from shader modules, but its own docs recommend explicit layouts in most cases. Explicit pipeline layouts also validate that the layout matches shader expectations and let multiple pipelines share the same resource binding scheme without rebinding churn.

Implication for Wrela:

- define one explicit bind-group layout family for phase-10 compute kernels
- keep binding slots stable across CPU/vGPU/WGSL contract versions
- set `min_binding_size` for buffer bindings wherever the contract size is known
- avoid per-kernel drift in bind slots

Recommended direction:

- one stable compute bind group for "control + artifacts + input + output + debug/counters"
- per-kernel variation should happen in entry points, record contents, and pipeline-overridable constants, not in random binding reshuffles

### 5. Workgroup size should be overridable, adapter-bounded, and 1D-first

WGSL allows `@workgroup_size(...)` parameters to be override expressions, and `wgpu` exposes pipeline overridable constants through `PipelineCompilationOptions::constants`. This is the clean way to tune kernels without forking shader source.

Implication for Wrela:

- use override constants for workgroup size
- start with 1D workgroups for batch query lanes
- clamp chosen sizes to adapter/device limits
- keep the query-plan ABI independent of workgroup size so tuning does not affect correctness

Recommended starting point:

- begin with `@workgroup_size(WG_X)` and a single linear dispatch mapping one invocation to one query item
- start around 64 or 128 logical lanes, then tune after parity is stable
- only move to 2D/3D workgroups when image or tiled workloads clearly need them

### 6. Upload and readback should stay explicit

`wgpu` is very clear about the upload and readback model:

- `Queue::write_buffer()` is convenient and starts GPU work on the next `submit`
- `Queue::write_buffer_with()` can skip one extra copy in some cases
- `StagingBelt` is best when writing many small pieces of data
- mapped readback requires explicit mapping and polling

Implication for Wrela:

- for small per-dispatch control data, use `Queue::write_buffer()`
- if phase 10 starts doing many small table uploads, move that path to `StagingBelt`
- for differentials, write results into `STORAGE | COPY_SRC` buffers, copy into a `MAP_READ` staging buffer, then `map_async` and poll
- keep readback as part of the backend test harness, not mixed into shader logic

### 7. Limits should start conservative and only grow when phase 10 proves a need

`wgpu` recommends starting with restrictive limits and only raising what is required. It also exposes helpers for reusing adapter alignment limits.

Implication for Wrela:

- request the minimum practical compute-capable limit set
- explicitly incorporate adapter alignment values into the requested limits
- do not request oversized limits "just in case"

Recommended policy:

- start from `Limits::downlevel_defaults()` for native bring-up unless phase 10 immediately needs something higher
- merge in adapter alignment via `using_alignment(adapter.limits())`
- raise specific limits only when a concrete query/result/artifact size requires it

### 8. Generated WGSL should be validated twice

There are two useful validation moments:

1. compiler-side validation of generated WGSL / IR before runtime bring-up
2. adapter/runtime validation when creating shader modules and pipelines

`naga::valid::Validator` is the right compiler-side guardrail if we keep codegen close to the `wgpu` stack.

Implication for Wrela:

- validate generated WGSL as part of codegen tests before touching a GPU
- still create shader modules/pipelines in tests to catch backend validation mismatches
- snapshot invalid cases too, especially layout or binding mismatches

## Recommended Patterns For Wrela Phase 10

### A. Keep one logical contract, with backend-specific transport only

The query-plan contract should stay authoritative. WGSL should consume the same logical records that CPU and virtual GPU already understand:

- dispatch contract header
- query-item records
- result-record header
- typed result payloads
- derived artifact descriptors

Recommended rule:

- every ABI-bearing record gets one compiler-owned schema
- every schema gets:
  - portable layout metadata
  - host pack/unpack helpers
  - WGSL struct emission
  - parity tests against CPU/vGPU

This avoids the common trap where shader structs become the "real" ABI and host code starts chasing them manually.

### B. Freeze the first compute bind group layout early

A good first phase-10 bind layout is something like:

- binding 0: dispatch/control header buffer, read-only storage
- binding 1: query-item input buffer, read-only storage
- binding 2: result output buffer, read-write storage
- binding 3: artifact table / culling / support data, read-only storage
- binding 4: counters / debug / observability buffer, read-write storage

This is only a pattern, not a mandated final layout, but the important part is:

- fixed slots
- explicit sizes
- explicit read-only vs read-write intent
- stable usage across kernels

### C. Generate WGSL from the portable ABI, not from ad hoc string templates

The repo already has the beginning of the right abstraction. Phase 10 should extend that into:

- WGSL type emission for portable builtins and contract records
- WGSL buffer declaration emission from the contract/binding schema
- kernel lowering that targets a small, stable WGSL subset

Recommended anti-pattern to avoid:

- hand-writing separate WGSL structs for `Hit3`, `Surface`, dispatch headers, or artifact tables while Rust keeps a different definition elsewhere

### D. Prefer per-kernel entry points over one giant polymorphic shader

For the first real backend bring-up, the clearest design is:

- shared module fragments for record types, helpers, and artifact access
- one entry point per internal kernel kind or narrow kernel family

That fits the roadmap better than one mega-shader with a large runtime switch, because:

- validation is easier
- parity failures are easier to localize
- bind layout can still stay shared
- it maps cleanly to the current `InternalKernelKind` model

### E. Treat floating-point tolerance as part of the contract

GPU parity is not "bit identical or fail" for every floating path. Phase 10 should define tolerances per result family:

- distance
- normal
- hit position / hit distance
- local frame and provenance-bearing fields
- radiance/media outputs

Recommended rule:

- encode tolerances in tests next to the record contracts
- keep provenance/identity checks stricter than floating shading checks
- make tolerance widening an explicit reviewed choice, not a hidden test hack

### F. Use observability buffers from the start

The roadmap already values counters. WGSL bring-up gets much easier if kernels can optionally report:

- dispatched item count
- candidate count
- branch/prune counts where meaningful
- hit count / miss count
- overflow / truncation flags

That gives phase 10 a direct bridge back to the current CPU/vGPU observability model.

## Concrete Advice For The First Landing

If we want the cleanest path through phase 10, the order should be:

1. Freeze the portable dispatch/result layouts that phase 10 v1 will support.
2. Add WGSL struct and buffer-declaration emission from the portable ABI layer.
3. Add compiler-side WGSL validation and text snapshots.
4. Add a native `wgpu` backend for distance batch queries only.
5. Add CPU/vGPU/WGSL parity tests over raw result records.
6. Add trace/hit assembly after distance parity is boring.
7. Add observability buffers and counter parity.
8. Add rendered-image parity only after query parity is stable.

That order matches both the roadmap and the technical constraints in the docs.

## Patterns To Avoid

- making handwritten WGSL the source of truth for scene/query behavior
- using inferred/default pipeline layouts as the long-term backend contract
- mirroring complex WGSL records with naive Rust `#[repr(C)]` structs and assuming they are safe to cast
- baking workgroup size into source with no override/tuning path
- using uniforms as the default carrier for large query/result contracts
- comparing only final images first and skipping raw query/result parity
- requesting more `wgpu` limits than phase 10 actually needs

## Proposed Phase-10 Design Direction

Based on the current repo and the docs, the most maintainable phase-10 shape looks like:

- compiler-owned ABI and binding schema remains central
- WGSL emission is generated from that schema
- `wgpu` backend is a transport/execution layer over the existing contract
- virtual GPU remains the fast differential lane for query-plan and kernel evolution
- CPU remains the oracle for semantic correctness

Said differently: phase 10 should feel like "add a real GPU transport for the phase-9 kernel contract", not "start a new renderer".

## Sources

Primary sources consulted on 2026-04-08:

- WGSL Editor's Draft, 2026-03-24:
  - https://gpuweb.github.io/gpuweb/wgsl/
  - relevant sections: host-shareable types, address space layout constraints, `workgroup_size`
- `wgpu` 29.0.1 docs:
  - https://docs.rs/wgpu/latest/wgpu/struct.Limits.html
  - https://docs.rs/wgpu/latest/wgpu/struct.ComputePipelineDescriptor.html
  - https://docs.rs/wgpu/latest/wgpu/enum.BindingType.html
  - https://docs.rs/wgpu/latest/wgpu/enum.BufferBindingType.html
  - https://docs.rs/wgpu/latest/wgpu/struct.BufferBinding.html
  - https://docs.rs/wgpu/latest/wgpu/struct.Buffer.html
  - https://docs.rs/wgpu/latest/wgpu/struct.Queue.html
  - https://docs.rs/wgpu/latest/wgpu/util/trait.DeviceExt.html
  - https://docs.rs/wgpu/latest/wgpu/util/struct.StagingBelt.html
  - https://docs.rs/wgpu/latest/wgpu/struct.PipelineCompilationOptions.html
- `naga` 29 docs:
  - https://wgpu.rs/doc/naga/valid/struct.Validator.html
- `bytemuck` 1.25 docs:
  - https://docs.rs/bytemuck/latest/bytemuck/trait.Pod.html
  - https://docs.rs/bytemuck/latest/bytemuck/trait.Zeroable.html
