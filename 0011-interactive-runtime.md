---
name: RFC 0011 Interactive Runtime
overview: Turn the existing EngineFrameScheduler from a benchmark-pulled report producer into the spine of a live, host-driven, user-interactive runtime built around the lowest achievable input-to-photon latency, with 120 fps as a stretch goal that yields to latency when they conflict. Attach generic engine subsystems (surface, input, systems, streaming residency, physics/moves, audio, save/load, tools) in strict dependency order. Staircase stays out of scope; a tiny generic reference world is the forcing function.
todos:
  - id: phase-62-9
    content: "Phase 62.9: Substrate tightening — EngineFrameRuntimePolicy::live(), closure-verdict generalization over EngineSubsystemReport, swapchain-as-Presentation-attachment, residency layering (compiler-side), one-adapter-per-kind invariant"
    status: completed
  - id: phase-62-95
    content: "Phase 62.95: Input-to-photon latency contract — timestamped TickInputEvent, LateInputSampler, motion-to-photon budget+telemetry, present-mode policy, max_frames_in_flight=1 in live(), MotionToPhotonContract"
    status: completed
  - id: phase-63
    content: "Phase 63: LiveEngineHost with continuous PlatformInputPump (ring-buffered, timestamped); late-sampling TickInputSource::Late; wrela live CLI; benchmark Eager path retained"
    status: completed
  - id: phase-64
    content: "Phase 64: wgpu::Surface + winit via runtime/src/platform; Mailbox-default + VRR-aware present policy; max_frames_in_flight=1 in live(); swapchain folded into Presentation framegraph; EngineSubsystemKind::Input semantic translator; authored input_map; apps/reference_host; wrela perf-latency lane"
    status: completed
  - id: phase-65
    content: "Phase 65: system_exec + system_plan + system_contract; SystemPhase ordering; read/write sets via @mut annotations (MIR refinement deferred); EngineSubsystemKind::System wired after StateAdvance"
    status: completed
  - id: phase-66
    content: "Phase 66: RegionResidencyService in compiler/residency; follow-target policy; admits/evicts keyed by ArtifactReuseKey; drives GpuResidentSceneCache and FrameUploadArena"
    status: completed
  - id: phase-67
    content: "Phase 67: physics_exec with XPBD substeps as EngineSubsystemKind::Physics; body/move/moveset parser + HIR + MIR; consumer of CollisionWorkloadBatch (SphereOverlap + SphereSweep/TOI); GPU-resident body state"
    status: completed
  - id: phase-68
    content: "Phase 68: audio_exec with cpal device, triple-buffered voice ledger, @audio_rt kernel subset, audio field / voice decl, binaural + media-driven spatialization, underruns as closure findings"
    status: completed
  - id: phase-69
    content: "Phase 69: persistence runtime reusing ArtifactReuseKey.compatibility_hash; persistent handle = stable_semantic_id; SnapshotSaveRecord with CBOR; SaveIncompatibility diagnostic on generator change"
    status: completed
  - id: phase-70
    content: "Phase 70: apps/reference_host replaces frame-live viewer with full interactive host + editor-scale inspector; cross-references every EngineFrameReport row to live subsystem state"
    status: completed
isProject: false
---

# RFC 0011: Interactive Runtime Foundation

## Thesis

Wrela is an extraordinary headless compiler + renderer + collision engine with a field-native language. It is not yet a game engine for one structural reason:

**every frame is pulled by a benchmark or an inspector; no frame is ever pushed by a live host responding to a user.**

The scheduler at [compiler/engine_frame/scheduler.rs](compiler/engine_frame/scheduler.rs) already knows how to plan, budget, schedule, and report an engine frame with presentation, collision, state-advance, query, and a future-subsystem reserve — EngineSubsystemKind at compiler/engine_frame/mod.rs:95-104 enumerates StateAdvance, Presentation, Collision, Query, GpuRuntime, FutureReserve(String). The trait EngineSubsystemAdapter at compiler/engine_frame/scheduler.rs:93-98 has one method:
rust
fn build(&mut self, builder: &mut EngineGraphBuilder) -> Result<EngineSubsystemPlan, EngineFrameError>;

That is the exact extension point every new subsystem in this RFC plugs into. Adapters are appended after the built-in StateAdvanceRuntimeAdapter in run_frame_with_subsystems at compiler/engine_frame/runtime.rs:388-496, and runs_after in EngineSubsystemDescriptor (compiler/engine_frame/scheduler.rs:16-23) sequences subsystems via the topological wiring at compiler/engine_frame/scheduler.rs:446-473.

But there is no wgpu::Surface, no winit, no input, no audio, no streaming residency service, no runtime system executor, and no save/load anywhere in the tree. Parser-side, RFC 0001 declarations body, generator, archetype, move, moveset, transition, space, audio field, voice, media (as declaration) are **spec-only — none of them are in the parser** (compiler/parser/grammar/mod.rs:102-151 exhausts root dispatch; none match those keywords). system / resource / event are parsed and lowered (parser entries compiler/parser/grammar/mod.rs:38-44,66-68, HIR roles at compiler/hir/def.rs:73-87), but there is no runtime that actually pumps systems per tick.

RFC 0011 turns the engine-frame scheduler into the spine of a live runtime and grows generic subsystems off it. The forcing function is an ordinary reference host (apps/reference_host/, sibling of [apps/frame_live_app](apps/frame_live_app)) that presents a render to a real window at as low a motion-to-photon latency as the hardware supports — targeting 120 Hz when achievable, accepting 60 Hz when latency-vs-framerate conflict — ingests input, pumps state_advance and system, maintains residency, runs authored physics and moves, plays field-driven audio, saves/loads, and emits the same EngineFrameReport shape that [compiler/perf_target](compiler/perf_target) already understands. The reference host is generic; no Staircase content.

## Latency-first stance

Frame rate is a smoothness lever; **input-to-photon latency is the feel lever**. A 60 fps game with 20 ms motion-to-photon outclasses a 120 fps game with 50 ms motion-to-photon for any input-driven gameplay (action, fighting, FPS, soulslike). RFC 0011 picks latency as the primary objective:

1. **Input-to-photon target**: 16 ms p99 desktop, 12 ms competitive (stretch). Measured wall-clock from event arrival at the platform pump to swapchain present completion. Lower than typical AAA (40–80 ms) by design.
2. **Frame rate is the secondary objective**: 120 fps when achievable without violating the latency target; 60 fps when the latency target requires it. Closure rule violation order: latency findings outrank framerate findings.
3. **Every architectural choice prefers latency over throughput**: continuous input pumping with late sampling, single GPU frame in flight, Mailbox/VRR present (never plain FIFO), zero-copy present, no double-buffered state for "safety."
4. **Latency is measured and reported**: every frame's EngineFrameReport carries a latency block (input-event-stamp → present-callback) and the closure verdict has dedicated rules for latency overshoot.

This bias has costs. Single-frame-in-flight reduces GPU/CPU overlap and therefore peak throughput. Mailbox present can show tearing without VRR. Late-sampled input requires careful state-advance design. The plan accepts these costs.

## Design invariants

1. **Latency is the primary objective.** Every subsystem decision must explicitly state its latency contribution and how it is bounded. A change that improves throughput at the expense of measured motion-to-photon latency is rejected absent explicit author opt-in (wrela.toml [presentation] mode = "throughput").
2. CPU remains the semantic oracle. Every subsystem has a CPU oracle before its GPU/native path is accepted — same discipline as [compiler/collision_exec](compiler/collision_exec).
3. The engine-frame report is the single source of truth. Live runtime telemetry is the same EngineFrameReport (compiler/engine_frame/mod.rs:189-231) the benchmark lane produces; closure gates (build_closure_verdict) keep working unchanged. Phase 62.95 extends the report with a latency block; existing benchmark scenarios populate it with synthetic stamps.
4. Fixed-step simulation, variable-step presentation. SimulationTick (compiler/state_advance/mod.rs:14-50, via TickInputBatch) is authoritative for gameplay; presentation interpolates. **Default simulation_hz == present_hz** (running sim at higher rate adds smoothness without reducing latency; running at lower rate adds latency from sample → tick).
5. **Input is sampled as late as possible.** Platform events are pumped continuously into a timestamped ring buffer; StateAdvanceRuntimeAdapter drains the ring buffer at the start of its job, not before the frame call. This is the largest single latency lever after present mode.
6. **Single GPU frame in flight in live() policy.** Default max_frames_in_flight: 1. Trading peak throughput for ~8 ms of latency reduction at 120 Hz. Tools and benchmark policies can override.
7. Generic first, game-specific never. No Staircase-shaped constructs leak into the runtime crate.
8. No regression of the existing 1080p120 closure lane — just perf-engine-closure must still pass at every phase boundary. The benchmark policy uses EngineFrameRuntimePolicy::closure() (unchanged), not live(), so the latency-first defaults do not affect closure throughput numbers.
9. Subsystem additions are EngineSubsystemAdapter implementations, not bespoke call sites. New variants of EngineSubsystemKind are added by extending the enum in compiler/engine_frame/mod.rs:95-104 and corresponding arms in runtime.rs ledger-building (compiler/engine_frame/runtime.rs:807-835).

## Comparison with prior agent proposals

Two prior proposals reviewed:

**"Wrela Real Engine Roadmap"** — adopts its Phase 2 author-productization push (getting-started, examples, project manifest, documented dev loop). Woven in as explicit acceptance criteria on every phase from 64 onward. Rejects its Phase 3 "first content pipeline = textures/materials from disk" because [language/spec/rfcs/0001-field-game-language.md](language/spec/rfcs/0001-field-game-language.md) lists imported meshes, textures, prebaked SDFs, and prebaked animations as explicit non-goals — Wrela's identity is field-native; the content pipeline is field-authored _projects_, not imported assets. Rejects its Phase 4 deferral of input/audio — direction chosen for this RFC is generic runtime subsystems, not finishing closure before opening the runtime front.

**"Input and Game Shell Roadmap"** — tactically right on the surface/input slice; moves adopted verbatim into Phase 64 (zero-copy wgpu::SurfaceTexture, new EngineSubsystemKind::Input, InputSubsystemAdapter scheduled before StateAdvance, semantic-action mapping). Strategically too narrow — stops at input and never reaches systems/residency/physics/audio/save.

## What we do instead of an asset pipeline

RFC 0001 rejects imported assets. The product equivalent is **field-native project packaging**, delivered across phases:

- wrela init **already exists** at compiler/bin/wrela/commands/command_dispatch.rs:124-132 (dispatches to init_project). Phase 64 extends it with templates (--template=hello_window, etc.) and emits a wrela.toml.
- wrela.toml schema declares entrypoint, default view, stdlib version, target backends. No asset roots.
- examples/ ships one working generic demo per phase's new subsystem.
- Generators/archetypes/bodies/regions are versioned via the existing ArtifactReuseKey::compatibility_hash (compiler/artifact_key/mod.rs:12-21) — Phase 69 extends this into the persistence domain.
- wrela dev **already exists** at compiler/bin/wrela/commands/command_dispatch.rs:753-775 (watch loop). Phase 64 extends it to drive the reference host with hot-reload via FrameLiveSession::reload_if_sources_changed (compiler/frame_live.rs:363-368) generalized to systems and input.

## Prerequisites

- RFC 0010 phases 57–61 (collision batching, EngineFrameScheduler, engine-frame budget governor in compiler/engine_frame/scheduler.rs:252-276) need to be in place. RFC 0010 phase 62 (closure gate cleanup) can overlap with Phase 62.9 of this RFC.

## Layering doctrine

Wrela's runtime/ crate currently does _not_ depend on compiler/; compiler/ depends on runtime/. RFC 0011 preserves that one-way flow:

- Anything that mentions a compiler-side type (EngineFrameRuntime, WorldSnapshotHandle, ArtifactReuseKey, KernelModule, CollisionWorkloadBatch, ResidencyPlan, SystemProgram, PhysicsSolver, AudioRtKernel, SnapshotSaveRecord) lives in compiler/. Includes compiler/residency/, compiler/system_exec/, compiler/physics_exec/, compiler/audio_exec/, compiler/persistence/.
- runtime/ only carries platform-glue substrates that have no compiler types in their public API. Examples: runtime/src/platform/window.rs (raw winit + wgpu::Surface), runtime/src/platform/input.rs (raw event types), runtime/src/platform/audio.rs (raw cpal device wrapper), runtime/src/host/clock.rs. The reference host (apps/reference_host) wires them together by depending on both crates.
- Test: any pub item in runtime/ whose signature references a compiler/ type is a layering violation. Phase 62.9 adds a CI lint for this.

## Dependency shape

mermaid
flowchart TD
scheduler["EngineFrameScheduler + run_frame_with_subsystems"]
substrate["Phase 62.9: Substrate tightening (live policy, generic closure verdict, swapchain attachment, layering)"]
latency["Phase 62.95: Input-to-photon latency contract (timestamped events, late sampling, present-mode policy, motion-to-photon telemetry)"]
liveHost["Phase 63: LiveEngineHost + continuous pump + late sampling + wrela live CLI"]
surface["Phase 64: wgpu::Surface + Mailbox/VRR present + frames_in_flight=1 + Input subsystem + reference_host + wrela perf-latency"]
systems["Phase 65: system_exec (SystemPhase ordering + annotated read-write sets)"]
residency["Phase 66: RegionResidencyService (ArtifactReuseKey admits-evicts)"]
physics["Phase 67: physics_exec XPBD + body/move/moveset (consumer of CollisionWorkloadBatch)"]
audio["Phase 68: audio_exec + cpal + audio_rt kernel subset + voice ledger"]
saveload["Phase 69: persistence reusing compatibility_hash + stable_semantic_id"]
tools["Phase 70: reference_host inspector cross-references EngineFrameReport (incl. latency block)"]

    scheduler --> substrate
    substrate --> latency
    latency --> liveHost
    liveHost --> surface
    liveHost --> systems
    surface --> tools
    systems --> residency
    systems --> physics
    residency --> tools
    physics --> tools
    systems --> audio
    audio --> tools
    systems --> saveload
    residency --> saveload
    saveload --> tools

Phases 66, 67, 68 are independent of each other once Phase 65 lands. Phases 62.9 and 62.95 are the load-bearing preludes; nothing else can start until they're done. Phase 62.95 has a hard dependency on 62.9 (it extends EngineFrameRuntimePolicy::live() and EngineFrameReport).

---

## Phase 62.9 — Scheduler and runtime substrate tightening

### Problem

The plan that follows assumes four things about the engine-frame substrate that are not currently true:

1. There is a runtime policy distinct from EngineFrameRuntimePolicy::closure() that allows live, gameplay-class frame execution. There isn't — closure() (compiler/engine_frame/runtime.rs:201-220) sets max_change_class: ChangeClass::Identity, which forbids any state change a real game frame must emit.
2. build_closure_verdict (compiler/engine_frame/closure.rs) treats every EngineSubsystemReport symmetrically. It does not — the verdict builder is hand-coded against the existing kinds, so a Physics or Audio over-budget event would not become a closure finding without explicit gating work.
3. Surface acquire/present is a primitive operation in the framegraph. It isn't — PresentationFramegraph (compiler/presentation_exec/framegraph.rs) has no wgpu::Surface integration; today it always allocates owned color targets.
4. There is exactly one adapter per EngineSubsystemKind. There is no enforcement — add_subsystem (compiler/engine_frame/runtime.rs:355-369) appends adapters to a Vec, and LedgerEntries (compiler/engine_frame/runtime.rs:807-835) keys on kind without dedup.

Building Phases 63–70 on top of those false assumptions would scatter ad-hoc workarounds across the rest of the roadmap. Phase 62.9 fixes the substrate up front so the remaining phases stay surgical.

### Architectural decisions

- **Add EngineFrameRuntimePolicy::live()** as a peer of closure() and a debug tools(). live() targets gameplay frames: allow_hot_path_gameplay_readbacks: false, allow_private_gpu_submits: false, max_change_class: ChangeClass::Behavior, enforce_engine_frame_budget: true, engine_frame_budget_ms: Some(8.33) (1080p120). tools() is a permissive debug-only policy used by inspector overlays in Phase 70 and is never the default.
- **Generalize closure verdict construction.** Rewrite build_closure_verdict to iterate over report.subsystems() and apply per-kind rules from a ClosureRuleTable keyed on EngineSubsystemKind. Existing rules for StateAdvance / Presentation / Collision / Query / GpuRuntime are ported verbatim; the table accepts new entries for Physics, Audio, Input, System, Residency, Save. Adding a new subsystem in a later phase is one rule-table entry, not a verdict-builder edit.
- **Treat the swapchain as a Presentation attachment, not a sibling subsystem.** Extend PresentationFramegraph with a new attachment role AttachmentKind::SwapchainColor and a constructor PresentationFramegraph::from_plan_and_gpu_resources_with_swapchain(plan, resources, swapchain_handle). The swapchain handle is an opaque trait object owned by LiveEngineHost; the framegraph calls acquire() at the head of its first pass and present() at the tail of its last pass. There is no EngineSubsystemKind::Surface. This avoids the runs_after: [Presentation] ordering trap (a separate Surface subsystem would race with Presentation on the same render targets).
- **Enforce one-adapter-per-kind.** EngineFrameRuntime::add_subsystem returns Err(EngineFrameError::DuplicateSubsystemKind(kind)) if an adapter for that kind is already registered (built-in StateAdvance counts). Tests in compiler/engine_frame/runtime/tests.rs cover the rejection case and the non-rejection case for FutureReserve(name) (which deliberately allows multiple, keyed on the name).
- **EngineGraphBuilder::add_job stays as-is for now.** The current signature takes a FnOnce() with no per-job context argument; adapters capture Arc<Mutex<\_>> for the few cases that need shared state (Phase 63 example below). An ergonomic add_job_with_context is documented as future work in compiler/engine_frame/scheduler.rs but is not built in 62.9. We revisit only if Phase 65/67/68 ergonomics force it.
- **Lock layering with a CI lint.** Add a just lint-layering lane that greps runtime/src/\*_/_.rs for compiler:: imports outside #[cfg(test)] and fails if any are found. Wire it into just lint. This prevents the residency / physics / audio crates from accidentally drifting into runtime/.
- **Closure verdict telemetry stays one report, one frame.** No per-subsystem closure subreports, no async aggregation. The single EngineFrameReport remains canonical; per-kind rules just generate more granular ClosureFindings under the same envelope.

### Files added

- [compiler/engine_frame/closure_rules.rs](compiler/engine_frame/closure_rules.rs) — ClosureRuleTable, per-kind rule traits, default rules ported from closure.rs.
- compiler/presentation_exec/swapchain.rs — SwapchainHandle trait (acquire, present, current_format, current_extent), PresentationFramegraph::from_plan_and_gpu_resources_with_swapchain constructor.

### Files modified

- [compiler/engine_frame/runtime.rs](compiler/engine_frame/runtime.rs) — add EngineFrameRuntimePolicy::live() and tools(); reject duplicate subsystem kinds in add_subsystem.
- [compiler/engine_frame/closure.rs](compiler/engine_frame/closure.rs) — rewrite build_closure_verdict to consult ClosureRuleTable; existing behavior preserved bit-for-bit on the current kinds.
- [compiler/presentation_exec/framegraph.rs](compiler/presentation_exec/framegraph.rs) — add AttachmentKind::SwapchainColor and the swapchain-aware constructor.
- [justfile](justfile) — add lint-layering lane and include it in lint.

### Acceptance criteria

- just check-clean passes.
- just test passes; new tests cover live() policy, duplicate-kind rejection, and the closure-rule-table replay of the existing closure findings (golden parity).
- just perf-engine-closure passes — closure verdict bit-stable against pre-62.9 baseline (this is the regression gate).
- just lint-layering is a working lane and currently passes (no compiler:: imports in runtime/).
- just ship passes.

### Why this phase exists

Without it, Phase 64 has to invent a fragile EngineSubsystemKind::Surface (with broken runs_after semantics), Phase 67 has to land Physics adapters that produce reports the closure verdict silently ignores, Phase 68 needs an audio-specific verdict path, and Phase 66's residency types end up in runtime/ cycling back into compiler/. Doing those four substrate fixes once, here, removes ~40% of the new-files churn from Phases 64/65/66/67/68.

---

## Phase 62.95 — Input-to-photon latency contract

### Problem

The plan's primary objective is the lowest achievable input-to-photon latency. None of the substrate currently exposes that as a measured, budgeted, or enforced property:

1. TickInputEvent has no wall-clock stamp on the originating platform event. There is no way to compute "event arrival → present" latency end-to-end.
2. EngineFrameInput.tick_inputs: TickInputBatch is a pre-built value at the call site. The platform input pump must therefore be drained _before_ run_frame_with_subsystems is called — every microsecond between drain and StateAdvance execution is added latency. There is no late-sampling primitive.
3. EngineFrameRuntimePolicy::live() (introduced in Phase 62.9) has no notion of present mode, no max_frames_in_flight, and no motion-to-photon target.
4. EngineFrameReport has per-subsystem CPU times but no end-to-end latency stage breakdown. The closure verdict cannot reason about latency.

Phase 62.95 makes input-to-photon a first-class contract: timestamped events, a late-sampling primitive, a present-mode policy, a frames-in-flight cap in live(), a motion-to-photon target with closure rules, and a latency block on every EngineFrameReport.

### Architectural decisions

- **Timestamp every platform event at arrival.** Extend TickInputEvent (compiler/state_advance/mod.rs:14-50 neighborhood) with wall_clock: WallClockStamp and monotonic_nanos: u64. The platform pump (Phase 64) writes both at the moment the OS event is observed. Recorded/headless sources synthesize them deterministically so closure parity holds.
- **TickInputSource becomes an enum with a late-sampling variant.**

rust
pub enum TickInputSource {
Eager(TickInputBatch), // headless, recorded, benchmark
Late(Arc<dyn LateInputSampler + Send + Sync>),
}

pub trait LateInputSampler: Send + Sync {
fn drain_up_to(&self, deadline: WallClockStamp) -> TickInputBatch;
fn ring_state(&self) -> InputRingState; // for telemetry (depth, drops)
}

EngineFrameInput.tick_inputs (compiler/engine_frame/runtime.rs:15-26) changes from TickInputBatch to TickInputSource. StateAdvanceRuntimeAdapter (compiler/engine_frame/runtime.rs:499-594) materializes the batch as the _first_ operation in its job: for Eager, it just unwraps; for Late, it calls drain_up_to(now()) and stamps a latency.input_sample_to_state_advance_start_nanos = 0 measurement. This is the latest possible sampling point that still preserves the existing single-source-of-truth for TickInputBatch.

- **EngineFrameRuntimePolicy::live() extended.** Add three fields, with latency-first defaults:
  - motion_to_photon_target_ms: Option<f64> — default Some(16.0) (60 Hz floor); competitive override is 12.0. Closure rule fires on overshoot.
  - max_frames_in_flight: u32 — default 1. Trades GPU/CPU overlap for ~8 ms of latency reduction at 120 Hz (one fewer pipelined frame).
  - present_mode_policy: PresentModePolicy — default PresentModePolicy::PreferMailboxThenVrrFifoThenFifo. Plain Fifo is allowed only if both Mailbox and VRR-aware FIFO are unavailable on the device, and produces a presentation.fallback_to_vsync_fifo finding (warning, not error).
  - tools() policy keeps the closure-style throughput-first defaults (frames_in_flight=2+, no latency target) for inspector overlays that need GPU/CPU overlap. closure() is unchanged.
- **MotionToPhotonContract defines what we measure.** Add compiler/engine_frame/latency.rs with:

rust
pub struct MotionToPhotonContract {
pub event_arrival_to_state_advance_nanos: u64, // platform pump → first sim job
pub state_advance_to_render_submit_nanos: u64, // sim → queue.submit
pub render_submit_to_gpu_complete_nanos: u64, // GPU work
pub gpu_complete_to_present_callback_nanos: u64, // post-GPU → present()
pub estimated_present_to_photons_nanos: u64, // worst-case display refresh
pub total_estimate_nanos: u64,
pub measurement_quality: MeasurementQuality,
}
pub enum MeasurementQuality { ExactGpuTimestamp, EstimatedFromCpuClock, Synthetic }

Stage-3 (GPU completion) uses wgpu::QuerySet::Timestamp when supported; falls back to EstimatedFromCpuClock. Stage-5 is 1 / refresh_rate_hz worst-case (or 0 with VRR within range). Synthetic stamps populate the contract on benchmark/recorded paths so existing closure scenarios produce a consistent shape.

- **EngineFrameReport.latency: MotionToPhotonContract is mandatory.** Add to the existing report struct (compiler/engine_frame/mod.rs:189-231). Phase 62.9's ClosureRuleTable gets a Presentation rule entry: presentation.motion_to_photon_over_budget fires when latency.total_estimate_nanos > policy.motion_to_photon_target_ms \* 1e6. Severity: error in live(), warning in tools(), suppressed in closure() (closure is throughput-first).
- **Continuous platform pump primitive (compiler-side trait, runtime-side impl).** compiler/engine_frame/latency.rs declares the trait LateInputSampler; Phase 64 provides runtime/src/platform/input_pump.rs::WinitInputPump implementing it via a lock-free SPSC ring buffer (crossbeam-queue::ArrayQueue<TimestampedRawEvent>). The pump runs on the host event loop thread; the sampler is called from the engine-frame thread. **No locks** between pump and sampler — only atomic indices into the ring.
- **No "double buffered for safety" anywhere.** Existing snapshot identity (WorldSnapshotHandle) is already an immutable handle, so there's no ownership reason to triple-buffer state. Audio's triple buffer (Phase 68) is the only exception, and it is a _separate_ thread crossing — not a latency hop.
- **Same simulation-rate-equals-present-rate default.** LiveProjectConfig.simulation_hz defaults to the present rate the swapchain reports (typically display refresh). Authors who need higher sim rate for physics stability override via wrela.toml and accept the latency tradeoff explicitly.
- **Closure parity.** Phase 62.95 must not change closure-lane numbers. The benchmark scenarios use EngineFrameRuntimePolicy::closure() (which keeps max_frames_in_flight: 2, no motion-to-photon target, FIFO present mode allowed). The latency block on the report is populated from synthetic stamps in benchmark mode and is asserted to be present but not budgeted.

### Files added

- compiler/engine_frame/latency.rs — MotionToPhotonContract, MeasurementQuality, LateInputSampler trait, InputRingState, engine_frame_context::current_late_input_sampler().
- compiler/engine_frame/closure_rules/presentation_latency.rs — presentation.motion_to_photon_over_budget rule (also covers presentation.fallback_to_vsync_fifo).

### Files modified

- [compiler/engine_frame/runtime.rs](compiler/engine_frame/runtime.rs) — EngineFrameRuntimePolicy::live() extended with three fields; EngineFrameInput.tick_inputs typed as TickInputSource.
- [compiler/engine_frame/mod.rs](compiler/engine_frame/mod.rs) — EngineFrameReport.latency: MotionToPhotonContract added.
- [compiler/state_advance/mod.rs](compiler/state_advance/mod.rs) — TickInputEvent gains wall_clock and monotonic_nanos. StateAdvanceRuntimeAdapter materializes TickInputSource at job entry and stamps the input-arrival measurement.
- [compiler/bin/wrela/perf_engine/collection.rs](compiler/bin/wrela/perf_engine/collection.rs) — synthesizes timestamps so existing scenarios remain bit-stable.

### Acceptance criteria

- just check-clean passes.
- just test passes; new tests cover:
  - TickInputSource::Late produces a TickInputBatch whose events all have monotonic timestamps within the drain window.
  - MotionToPhotonContract stages sum to within 1 µs of total_estimate_nanos.
  - The closure-rule replay test (Phase 62.9 golden) is bit-stable; latency block populated from synthetic stamps does not change the verdict.
  - live() policy with motion_to_photon_target_ms: 16.0 produces a finding when synthetic latency exceeds the target.
- just perf-engine-closure passes — closure verdict bit-stable against pre-62.95 baseline.
- just lint-layering still passes; LateInputSampler trait is in compiler/, the winit impl is in runtime/.

### Why this phase exists

Latency is the primary product goal. If the plan does not lift it to a first-class substrate concern _before_ Phase 63 wires the host loop and Phase 64 wires the surface, every implementer of those phases will choose the easy throughput-friendly default (drain input pre-frame, Fifo present, default 2+ frames in flight) — and motion-to-photon will land at 40–60 ms instead of 12–20 ms, with no closure findings to drag it back. Doing this once, here, makes "latency-first" a property of the substrate rather than a policy that has to be re-fought in every phase.

---

## Phase 63 — Live frame loop (spine)

### Problem

EngineFrameRuntime::run_frame_with_subsystems (compiler/engine_frame/runtime.rs:388-496) is called exactly twice in the tree today: from the benchmark collection path (compiler/bin/wrela/perf_engine/collection.rs) and from internal tests. It takes an EngineFrameInput (compiler/engine_frame/runtime.rs:15-26) containing scenario_id, frame_index, previous_snapshot, clocks, tick_inputs, policy, query_requests, readback_requests, and returns an EngineFrameOutput { snapshot, query_results, report }. Every frame is pulled by something authoritative that already knows what should happen.

A live host is the inverse: something external pushes wall-clock time at the runtime, and the runtime decides whether it's time to advance a simulation tick, renders a presentation frame, and returns when both are done. Nothing in the tree does this.

### Architectural decisions

- **Fixed-step simulation, present-paced loop.** The host runs one logical frame per present opportunity. simulation_hz defaults to the present rate the swapchain reports (display refresh, typically 60/120/144); accumulator runs fixed-step within that pacing. Higher sim rates are an explicit wrela.toml opt-in with a documented latency cost.
- **One process, three clocks.** SimulationTick (monotonic, fixed dt), PresentationFrame (monotonic, per-rendered-frame), WallClockStamp (platform time). TemporalClock already carries all three.
- **Use EngineFrameRuntimePolicy::live()** as extended in Phases 62.9 and 62.95. Compared to closure(), live() relaxes max_change_class from Identity to Behavior, sets motion_to_photon_target_ms: Some(16.0), max_frames_in_flight: 1, and present_mode_policy: PreferMailboxThenVrrFifoThenFifo. Tools/debug overrides are tools(), never the default.
- **No new subsystems in this phase.** The live host is a wrapper around the existing scheduler. Phase 63 is purely about driving what already exists from a loop.
- **Platform input is pumped _continuously_ into a timestamped ring buffer; sampling happens _late_, inside the frame.** LiveEngineHost owns a PlatformInputPump (Phase 64 contributes the winit/gilrs impl) that runs on the host thread and writes timestamped events to a lock-free SPSC ring. LiveEngineHost::advance does **not** drain the pump before calling run_frame_with_subsystems; instead it constructs a TickInputSource::Late(sampler) (Phase 62.95) and passes it in. StateAdvanceRuntimeAdapter calls sampler.drain_up_to(now()) as the first thing it does in its job, then stamps latency.event_arrival_to_state_advance_nanos for each event. This is the single largest latency lever in the engine after present mode.
- **Headless and recorded paths use the eager source.** Tests, benchmark, and replay use TickInputSource::Eager(TickInputBatch) so timing is deterministic. The same trait surface, two variants — no behavioral fork in StateAdvanceRuntimeAdapter beyond the unwrap-vs-drain dispatch.
- **Share the tick→EngineFrameInput translator with the benchmark path.** Extract EngineFrameInput assembly from perf_engine/collection.rs into compiler/engine_frame/live.rs::build_engine_frame_input(...). The benchmark path uses TickInputSource::Eager; the live path uses TickInputSource::Late. There is exactly one code path that produces EngineFrameInput.
- **Introduce LiveProjectConfig instead of new methods on LoadedProject.** Phase 63 doesn't extend the LoadedProject API at compiler/hir/project.rs. Instead, LiveEngineHost takes a LiveProjectConfig (scenario_id: String, default_query_requests: Vec<QueryRequest>, simulation_hz_override: Option<f64>) constructed once at startup from the project + CLI args. The benchmark path constructs the same struct from its existing scenario manifest.

### Extension points (existing code)

| What                                                 | Where                                                                             |
| ---------------------------------------------------- | --------------------------------------------------------------------------------- |
| EngineFrameRuntime::run_frame_with_subsystems        | compiler/engine_frame/runtime.rs:388-496                                          |
| EngineFrameInput construction                        | currently inline in compiler/bin/wrela/perf_engine/collection.rs; extract         |
| EngineFrameRuntimePolicy::live() (new in Phase 62.9) | compiler/engine_frame/runtime.rs:59-75 (alongside existing closure())             |
| TemporalClock tick/frame/wall-clock fields           | compiler/time_semantics/mod.rs (via compiler/state_advance/mod.rs:1-4 re-exports) |
| Benchmark collection loop                            | compiler/bin/wrela/perf_engine/collection.rs                                      |

### New files

- compiler/engine_frame/live.rs — LiveEngineHost, LiveEngineHostBuilder, LiveEngineTick, build_engine_frame_input.
- compiler/bin/wrela/commands/live.rs — execute_live_command.
- compiler/wrela/cli_args.rs gains a Live { project_path, mode: LiveMode } variant (LiveMode::Headless { frames: u32 }, LiveMode::Interactive — the latter lands in Phase 64).

### Code shape

rust
pub struct LiveProjectConfig {
pub scenario_id: String,
pub default_query_requests: Vec<QueryRequest>,
pub simulation_hz_override: Option<f64>, // None = match present rate
}

pub struct LiveEngineHost {
runtime: EngineFrameRuntime,
policy: EngineFrameRuntimePolicy, // EngineFrameRuntimePolicy::live() unless tools mode
project: LoadedProject,
config: LiveProjectConfig,
simulation_hz: f64,
accumulator_secs: f64,
previous_snapshot: WorldSnapshotHandle,
previous_clock: TemporalClock,
current_clock: TemporalClock,
frame_index: u32,
// Continuous platform pump → ring buffer. Drained by StateAdvance late.
input_pump: Arc<dyn LateInputSampler + Send + Sync>,
// Eager fallback for headless/recorded; mutually exclusive with input_pump.
eager_source: Option<Box<dyn EagerTickInputSource>>,
}

pub trait EagerTickInputSource: Send {
fn drain_for_tick(&mut self, tick: SimulationTick, wall: WallClockStamp) -> TickInputBatch;
}

pub struct HeadlessTickSource; // empty inputs per tick
pub struct RecordedTickSource { records: Vec<TickInputBatch>, cursor: usize }

impl LiveEngineHost {
pub fn advance(&mut self, wall_elapsed_secs: f64) -> Result<LiveEngineTick, EngineFrameError> {
self.accumulator_secs += wall_elapsed_secs;
let step = 1.0 / self.simulation_hz;
let mut outputs = Vec::new();
while self.accumulator_secs >= step {
// Build TickInputSource WITHOUT draining the pump — the drain is
// deferred to StateAdvanceRuntimeAdapter for late sampling.
let tick_source = match &mut self.eager_source {
Some(s) => TickInputSource::Eager(
s.drain_for_tick(
self.previous_clock.tick(),
self.current_clock.wall_clock(),
),
),
None => TickInputSource::Late(Arc::clone(&self.input_pump)),
};
let input = build_engine_frame_input(
&self.config,
self.frame_index,
self.previous_snapshot.clone(),
self.previous_clock.clone(),
self.current_clock.clone(),
tick_source,
self.policy.clone(),
);
let output = self.runtime.run_frame_with_subsystems(input, Vec::new())?;
self.previous_snapshot = output.snapshot.clone();
self.previous_clock = output.report.identity.clock.clone();
self.current_clock = advance_clock(&self.previous_clock, step);
self.frame_index = self.frame_index.saturating_add(1);
self.accumulator_secs -= step;
outputs.push(output);
}
Ok(LiveEngineTick { outputs })
}
}

pub fn build_engine_frame_input(
config: &LiveProjectConfig,
frame_index: u32,
previous_snapshot: WorldSnapshotHandle,
previous_clock: TemporalClock,
current_clock: TemporalClock,
tick_inputs: TickInputSource,
policy: EngineFrameRuntimePolicy,
) -> EngineFrameInput {
EngineFrameInput {
scenario_id: config.scenario_id.clone(),
frame_index,
previous_snapshot,
previous_clock,
current_clock,
tick_inputs,
policy,
query_requests: config.default_query_requests.clone(),
readback_requests: Vec::new(),
}
}

### Author surface

No new language surface in Phase 63. The host is driven by wrela live against any existing project that compiles and declares a view.

### CLI

bash
wrela live <project> --headless --frames 120 # headless, emits JSON EngineFrameReport per frame
wrela live <project> --headless --record-input <path> # records inputs (empty in 63)
wrela live <project> --headless --replay-input <path> # replays recorded inputs (used in 64)

### Tests

- compiler/tests/live_host.rs — drive LiveEngineHost for 120 simulated frames against a trivial view, assert:
  - frame_index monotonic, snapshot.epoch() increases by exactly 1 per tick.
  - Clocks monotonic.
  - EngineFrameReport.budget_directives equals the policy budget on every frame.
  - report.violations is empty.
  - report.latency.event_arrival_to_state_advance_nanos is well-formed (synthetic stamps in headless mode produce monotonic, non-negative values).
- compiler/tests/live_host_late_sampling.rs — synthesize a LateInputSampler impl backed by an in-memory ring; emit timestamped events at known wall-clock offsets; drive LiveEngineHost::advance and assert the events surface in the resulting TickInputBatch materialized by StateAdvance, with latency.event_arrival_to_state_advance_nanos ≤ a tight bound (e.g., 500 µs in single-threaded test mode).
- compiler/bin/wrela/perf_engine/collection.rs refactored to call build_engine_frame_input with TickInputSource::Eager; existing perf tests must pass unchanged (closure verdict bit-stable).

### Acceptance

- wrela live <project> --headless --frames 120 prints 120 EngineFrameReport JSON records identical in shape to just perf-engine-closure's reports, including the new latency block.
- Benchmark lane (just perf-engine-closure, just test-engine-frame) still passes unchanged.
- At least one compiler/tests/live_host.rs test exists and is listed in compiler/tests/repo_smoke.rs.
- AGENTS.md gains a line noting just live-smoke as a cheap headless-runtime sanity lane.
- Late-sampling integration test passes: a synthesized LateInputSampler produces events whose event_arrival_to_state_advance_nanos is ≤ 500 µs.

---

## Phase 64 — Interactive surface, input, project scaffold

### Problem

PresentationFramegraph (compiler/presentation_exec/framegraph.rs:43-55) owns a GpuRuntimeContext (device + queue), CommandEncoder, GpuAttachmentArena, and ReadbackTickets, but **has no wgpu::Surface, no get_current_texture, no present** (verified: zero hits for wgpu::Surface|create_surface|winit|cpal|keyboard|gamepad in the tree). Every rendered frame today terminates in a readback to CPU memory for inspection or benchmark reporting. A live game loop has to do the opposite: skip the readback and hand an acquired swapchain texture to the framegraph as an attachment, then call queue.submit + surface_texture.present().

Separately: there is no input anywhere. TickInputBatch (compiler/state_advance/mod.rs:14-50) carries a Vec<TickInputEvent> of opaque events; the runtime doesn't know what a keyboard or a gamepad is.

### Architectural decisions

- **Zero-copy present, swapchain folded into Presentation, no Surface subsystem.** Phase 62.9 already added AttachmentKind::SwapchainColor and PresentationFramegraph::from_plan_and_gpu_resources_with_swapchain. Phase 64 wires it: LiveEngineHost constructs a SwapchainHandle over the winit window and passes it into the framegraph constructor before the frame. Acquire is the framegraph's first encoded operation; present is the last. The existing readback ticket machinery stays for inspection but is never touched on the present path. No EngineSubsystemKind::Surface.
- **Mailbox-default present mode policy.** PresentModePolicy::PreferMailboxThenVrrFifoThenFifo (Phase 62.95 default for live()):
  1. If the device supports wgpu::PresentMode::Mailbox, use it. Mailbox is "replace last" — present completes immediately if a frame is ready, otherwise the last frame is shown again. Zero queued-present latency. Tearing is possible only when frame rate exceeds refresh rate (which is when authored content is "wasting" frames anyway).
  2. Else if VRR-aware FIFO is available (wgpu::PresentMode::FifoRelaxed), use it. On VRR displays, this presents at the moment of GPU completion within the VRR range — same effective latency as Mailbox, no tearing.
  3. Else fall back to plain Fifo. Emits presentation.fallback_to_vsync_fifo finding (warning). Authors can also explicitly opt in via wrela.toml [presentation] present_mode = "fifo" (no warning then).
- **Single GPU frame in flight.** live() policy from Phase 62.95 sets max_frames_in_flight: 1. The wgpu queue is _not_ allowed to queue a second CPU-recorded command buffer until the first one's GPU work has completed. Implemented via a wgpu::Queue::on_submitted_work_done callback that releases a per-frame semaphore; LiveEngineHost::advance blocks at the _top_ of the loop on the prior frame's semaphore. This trades ~50% peak frame-rate ceiling for ~8 ms of latency at 120 Hz.
- **Continuous platform input pump on the main thread.** WinitInputPump runs on the winit event loop (the only place winit allows event reads); each Event::WindowEvent and gamepad poll is timestamped with Instant::now() and pushed into a lock-free SPSC crossbeam-queue::ArrayQueue<TimestampedRawEvent> (capacity 4096; ring overflow is a presentation.input_ring_overflow finding). LiveEngineHost runs the engine frame on a worker thread; the worker calls LateInputSampler::drain_up_to(now()) (Phase 62.95) inside StateAdvanceRuntimeAdapter to drain timestamped events at the latest possible moment.
- **Input drives gameplay via two distinct surfaces.**
  1. **Raw input is drained _late_ into TickInputBatch.** StateAdvanceRuntimeAdapter calls sampler.drain_up_to(now()) as the first thing it does, materializing the batch from the timestamped ring. Each event keeps its wall_clock and monotonic_nanos.
  2. **Semantic input is published _inside_ the frame as a resource.** Add EngineSubsystemKind::Input. InputSubsystemAdapter has runs_after: vec![EngineSubsystemKind::StateAdvance] and requires_gpu: false. Its job reads TickInputBatch from the materialized frame input, translates it to semantic actions via input_map, and writes an immutable InputFrame into a new EngineResourceId::InputFrame { epoch }. The Phase 65 SystemSubsystemAdapter reads this resource. StateAdvance does _not_ depend on InputFrame; only systems do.
- **Motion-to-photon telemetry wired end-to-end.** Phase 64 populates the four CPU-side stages of MotionToPhotonContract (Phase 62.95):
  - event_arrival_to_state_advance_nanos: median of the per-event (state_advance_start - event.monotonic_nanos) across the drained batch.
  - state_advance_to_render_submit_nanos: end-of-StateAdvance timestamp to queue.submit timestamp.
  - render_submit_to_gpu_complete_nanos: from wgpu::QuerySet::Timestamp if supported, else estimated.
  - gpu_complete_to_present_callback_nanos: from the on_submitted_work_done callback to surface_texture.present() return.
  - Stage 5 (estimated_present_to_photons_nanos) uses display refresh rate from the swapchain caps; 0 within VRR range.
- **wrela perf-latency lane.** New just perf-latency lane plus wrela perf-latency <project> CLI command. Drives a 600-frame interactive run with synthetic-input injection: emits a TickInputEvent with a known wall-clock stamp via a back-channel into the pump, measures motion-to-photon to a fixed pixel change captured via wgpu::SurfaceTexture post-present readback (one-shot, not on the hot path), reports p50/p95/p99 across the run. Closure rule presentation.motion_to_photon_perf_lane_over_budget fires if p99 exceeds motion_to_photon_target_ms. This is the regression gate.
- **Platform layer in the runtime crate; no compiler types in runtime/.** runtime/src/platform/ owns winit, gilrs, the SwapchainHandle impl, and WinitInputPump. Public APIs return raw types only. Phase 62.9's lint-layering lane catches regressions.
- **Raw events translated via authored input_map.** system authors never see OS keycodes.
- **wrela init already exists.** Extend it with --template=<name> and scaffold wrela.toml + examples/. wrela dev already exists (command_dispatch.rs:753-775); its watch loop is extended to drive apps/reference_host with hot-reload via generalized FrameLiveSession::reload_if_sources_changed (compiler/frame_live.rs:363-368).

### Extension points (existing code)

| What                             | Where                                                                                                                   |
| -------------------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| Swapchain attachment integration | compiler/presentation_exec/framegraph.rs:43-55 (constructor added in Phase 62.9; Phase 64 wires its callers)            |
| GpuAttachmentArena               | declared in framegraph.rs; Phase 62.9 extended with swapchain_attachment(...)                                           |
| EngineSubsystemKind              | extend enum at compiler/engine_frame/mod.rs:95-104 with Input only                                                      |
| Resource ledger arms             | extend match in compiler/engine_frame/runtime.rs:807-835 for Input                                                      |
| EngineResourceId                 | extend enum at compiler/engine_frame/runtime.rs:98-108 with InputFrame { epoch }                                        |
| Closure verdict rules for Input  | add ClosureRuleTable entry in compiler/engine_frame/closure_rules.rs (table introduced in Phase 62.9)                   |
| EngineSubsystemAdapter           | compiler/engine_frame/scheduler.rs:93-98                                                                                |
| FunctionRole                     | extend at compiler/hir/def.rs:73-87 with InputMap                                                                       |
| Parser root dispatch             | extend compiler/parser/grammar/mod.rs:102-151 — add input_map keyword arm                                               |
| CLI ParsedCommand                | extend at compiler/bin/wrela/cli_args.rs:388-414 — add Live { project, mode }, extend Init { template: Option<String> } |
| init_project                     | compiler/bin/wrela/commands/command_dispatch.rs:124-132                                                                 |
| dev watch loop                   | compiler/bin/wrela/commands/command_dispatch.rs:753-775                                                                 |
| Frame-live hot reload            | compiler/frame_live.rs:297-390                                                                                          |

### New files

- runtime/src/platform/mod.rs — PlatformEventPump, PlatformBackend trait. Public surface uses raw types only (no compiler imports).
- runtime/src/platform/window.rs — winit event loop integration, WindowHandle, surface creation, swapchain configuration honoring PresentModePolicy.
- runtime/src/platform/input.rs — raw event collection, button-state differ, gamepad via gilrs, Instant::now() timestamping at arrival.
- runtime/src/platform/input_pump.rs — WinitInputPump: lock-free SPSC ring (crossbeam-queue::ArrayQueue<TimestampedRawEvent>, capacity 4096), implements LateInputSampler (Phase 62.95). Overflow detection emits presentation.input_ring_overflow.
- runtime/src/platform/surface.rs — SwapchainHandle implementation: wraps wgpu::Surface, implements the trait introduced in Phase 62.9, plus wgpu::QuerySet::Timestamp integration when supported.
- runtime/src/platform/frame_pacing.rs — FrameInFlightSemaphore: per-frame semaphore released by wgpu::Queue::on_submitted_work_done. LiveEngineHost::advance blocks on it at the top of the loop when max_frames_in_flight: 1.
- compiler/engine_frame/input_adapter.rs — InputSubsystemAdapter. (No surface_adapter.rs — swapchain integration is part of Presentation.)
- compiler/input_contract/mod.rs — InputFrame, SemanticAction, InputMapId, InputMapBinding.
- compiler/input_map_plan/mod.rs — compiled InputMapPlan that InputSubsystemAdapter executes each frame.
- compiler/bin/wrela/perf_latency/mod.rs — wrela perf-latency collection + reporting; synthesizes a back-channel TickInputEvent into the pump and measures motion-to-photon per frame.
- apps/reference_host/Cargo.toml, apps/reference_host/src/main.rs, apps/reference_host/src/lib.rs.
- examples/surface_and_input/ — project with one view + one input_map + one gamepad orbit system.

### Author surface (parser + HIR additions)

New top-level declaration input_map:
wr
input_map PlayerInputMap {
action MoveForward = key.w | gamepad.left_stick_y < -0.2
action MoveBack = key.s | gamepad.left_stick_y > 0.2
action Strike = mouse.left_button | gamepad.right_trigger > 0.5
action Look = axis2(mouse.delta_x, mouse.delta_y) | axis2(gamepad.right_stick_x, gamepad.right_stick_y)
}

Grammar production added to compiler/parser/grammar/mod.rs:102-151:
rust
"input_map" => Some(parse_input_map_decl(parser)),

HIR lowering adds FunctionRole::InputMap at compiler/hir/def.rs:73-87 and a new InputMapMetadata { bindings: Vec<InputMapBinding> } alongside the existing PresentationMetadata / DomainMetadata. Typeck validates that every action has a unique semantic id and every physical source resolves to a known PlatformInput constant.

Systems consume InputFrame by declaring an input: InputFrame parameter — already parses as system accepts named params.

### Code shape

rust
pub struct InputFrame {
pub epoch: SnapshotEpoch,
pub tick: SimulationTick,
pub actions: ImmutableMap<SemanticActionId, SemanticActionState>,
}

pub enum SemanticActionState {
Button { pressed: bool, just_pressed: bool, just_released: bool },
Axis1 { value: f32 },
Axis2 { x: f32, y: f32 },
}

pub struct InputSubsystemAdapter {
map: InputMapPlan,
// Tick input batch is materialized from TickInputSource by StateAdvance.
// Late-sampled in live mode (Phase 62.95), eager in headless/recorded.
shared_frame: Arc<Mutex<Option<InputFrame>>>,
}

impl EngineSubsystemAdapter for InputSubsystemAdapter {
fn build(&mut self, builder: &mut EngineGraphBuilder) -> Result<EngineSubsystemPlan, EngineFrameError> {
let descriptor = EngineSubsystemDescriptor {
kind: EngineSubsystemKind::Input,
label: "input".into(),
// TickInputBatch is materialized by StateAdvance from TickInputSource
// (eager unwrap or late drain). Input subsystem reads it via
// EngineFrameContext::tick_inputs() and produces the semantic InputFrame.
runs_after: vec![EngineSubsystemKind::StateAdvance],
requires_gpu: false,
allows_hot_path_readback: false,
};
let map = self.map.clone();
let frame_slot = Arc::clone(&self.shared_frame);

        // EngineGraphBuilder::add_job takes a plain FnOnce() -> Result<(), _>
        // (compiler/engine_frame/scheduler.rs:93-150). Adapters that need shared
        // state capture Arc<Mutex<_>> like below. Phase 62.9 documents this
        // pattern; an ergonomic add_job_with_context is future work.
        let job = builder.add_job(
            EngineSubsystemKind::Input,
            "input.translate",
            EngineJobAffinity::Cpu,
            EngineSpanDomain::Cpu,
            Vec::new(), // intra-subsystem deps; cross-subsystem ordering is from runs_after
            false,
            Box::new(move || {
                // The map closure pulls TickInputBatch out of the active frame
                // input via a thread-local set by EngineFrameRuntime::run_frame.
                // Phase 64 introduces engine_frame_context::current_tick_inputs()
                // for adapter-side reads; same mechanism the StateAdvance adapter
                // uses today.
                let raw = engine_frame_context::current_tick_inputs();
                let translated = map.translate(raw);
                *frame_slot.lock().unwrap() = Some(translated);
                Ok(())
            }),
        )?;

        Ok(EngineSubsystemPlan::new(
            descriptor,
            vec![job],
            // report_builder closure runs after job completion to assemble the
            // per-subsystem EngineSubsystemReport entry. Closure-rule table
            // (Phase 62.9) keys on EngineSubsystemKind::Input for findings.
            Box::new(move |_collected| EngineSubsystemReport::new(EngineSubsystemKind::Input)),
        ))
    }

}

Presentation framegraph swapchain integration is fully in Phase 62.9. Phase 64 only wires the call site:
rust
// In LiveEngineHost::advance, before run_frame_with_subsystems:
let swapchain = self.window.swapchain_handle(); // Arc<dyn SwapchainHandle>
self.runtime.set_presentation_swapchain(Some(swapchain));
// run_frame_with_subsystems is unchanged; PresentationFramegraph
// internally calls swapchain.acquire() at the head of pass encoding
// and swapchain.present() in the post-submit terminal phase.

### wrela.toml

toml
[package]
name = "hello_window"
version = "0.1.0"

[engine]
stdlib = "0.1"
default_view = "main_view"

[backends]
query = "wgsl"
collision = "wgsl"

### Tests

- runtime/tests/platform_input_headless.rs — build a TickInputBatch from a recorded event log, assert semantic translation through InputMapPlan is correct, and assert every TickInputEvent carries non-zero wall_clock and monotonic_nanos.
- runtime/tests/winit_input_pump_ring.rs — burst 8192 timestamped events into the SPSC ring; assert no losses up to ring capacity, then assert overflow detection kicks in and emits presentation.input_ring_overflow.
- runtime/tests/frame_in_flight_semaphore.rs — assert LiveEngineHost::advance blocks at the top of frame N+1 until on_submitted_work_done for frame N has fired (under wgpu::Backend::Noop).
- compiler/tests/input_subsystem.rs — drive InputSubsystemAdapter through EngineFrameScheduler with a constructed late-sampled batch; assert InputFrame appears in EngineResourceLedger after StateAdvance and before any system resource read.
- compiler/tests/swapchain_attachment.rs — use the wgpu::Backend::Noop virtual device wired through a stub SwapchainHandle; assert the framegraph emits an AttachmentKind::SwapchainColor color attachment, that acquire/present are called exactly once, and that no EngineReadbackLedger entries are recorded on the present path.
- compiler/tests/present_mode_policy.rs — assert PresentModePolicy::PreferMailboxThenVrrFifoThenFifo selects Mailbox when both are advertised, FifoRelaxed when only it is, plain Fifo only when neither, and that the Fifo fallback emits presentation.fallback_to_vsync_fifo.
- apps/reference_host/tests/smoke.rs — spawns an offscreen window (when WRELA_TEST_OFFSCREEN=1), runs LiveEngineHost for 60 frames, asserts no violations, asserts report.latency.total_estimate_nanos ≤ 25 ms p99 on the reference machine.
- compiler/bin/wrela/perf_latency/tests/synthetic_loop.rs — drives wrela perf-latency against a stub project where the swapchain present completion is synthesized; asserts the p50/p95/p99 motion-to-photon numbers are well-formed and that exceeding motion_to_photon_target_ms emits the over-budget finding.

### Acceptance

- apps/reference_host opens an interactive window at the display's native refresh, renders zero-copy to the swapchain, accepts keyboard/mouse/gamepad, drives a generic orbit camera. No readback on the present path.
- EngineFrameReport.latency block populated end-to-end: all five stages have non-zero values, measurement_quality is ExactGpuTimestamp on hardware that supports it.
- **Latency target met on reference machine**: wrela perf-latency examples/surface_and_input produces p99 motion-to-photon ≤ 16 ms on a 120 Hz Mailbox-capable display, ≤ 22 ms on a 60 Hz FIFO-only display. These numbers become CI gates for just ship.
- **Framerate target attempted but secondary**: 1080p120 sustained on the reference machine is the target; if not achievable simultaneously with the latency target on a given hardware class, the latency target wins and a presentation.framerate_below_target finding is emitted (warning, not error).
- presentation.fallback_to_vsync_fifo, presentation.input_ring_overflow, presentation.motion_to_photon_over_budget, presentation.motion_to_photon_perf_lane_over_budget, and presentation.framerate_below_target rules are landed in ClosureRuleTable.
- Engine-frame report contains input.translate, presentation.swapchain_acquire, presentation.swapchain_present spans with honest attribution.
- wrela init hello_window --template=hello_window scaffolds a project that builds and runs through wrela dev.
- examples/surface_and_input/ exists and is smoke-tested by a new just dev-smoke lane.
- just perf-latency is a new just lane; just ship runs it.
- just lint-layering still passes — runtime/src/platform/ exposes only raw types in its public surface.

---

## Phase 65 — system runtime

### Problem

The parser accepts system Name(...) { ... } (compiler/parser/grammar/mod.rs:66-68), HIR lowers it to FunctionRole::System (compiler/hir/def.rs:73-87), and typeck validates it under the "deterministic game policy" (compiler/hir/typeck/types.rs:926-948). But: **systems are never executed as part of the frame** — they compile to ordinary MIR functions (compiler/mir/lower/function_entry.rs:153-238, no special path for FunctionRole::System besides the deterministic policy flag). They're only callable by other functions; there is no scheduler pumping them per tick.

Meanwhile StateAdvanceRuntimeAdapter (compiler/engine_frame/runtime.rs:499-594) runs a single generic state-advance step that is internally-decided, not authored.

### Architectural decisions

- **Systems run between Input and Presentation, after StateAdvance.** That gives: Input → StateAdvance → System (multi-phase) → Residency → Collision/Physics/Query → Presentation → Surface.present.
- **Explicit SystemPhase ordering.** Systems declare phase via attribute @phase(pre_sim | sim | post_sim). Within a phase, ordering is by the read/write-set DAG. This is the same pattern the scheduler already uses for inter-subsystem ordering via runs_after, just at intra-subsystem granularity.
- **Read/write sets inferred from MIR.** No annotations. A MIR pass analyzes each FunctionRole::System function, walks its call graph + field accesses, and produces a SystemAccessSummary { reads: Vec<ResourceId>, writes: Vec<ResourceId> }. The summary is keyed by the set of capture arguments and resource + event types the system takes. This is conservative — over-approximation is OK.
- **Aliasing disallowed at plan time.** If two systems in the same phase both write the same ResourceId, plan validation fails with a specific diagnostic pointing to both declarations. This makes system ordering non-surprising.
- **Events are the one-way communication primitive.** event MyEvent { ... } already parses and lowers. Phase 65 wires events through the engine frame: an event emitted in tick N is observable by systems with a declared EventReader<MyEvent> parameter in tick N+1 (one-tick deferred, deterministic).
- **system_exec mirrors collision_exec and presentation_exec.** Typed plan, executor, observability, CPU oracle. There is no GPU system backend in Phase 65 — systems are pure CPU. GPU compute for systems is a future optimization routed through the existing kernel fn GPU lowering, not a Phase 65 concern.

### Extension points (existing code)

| What                          | Where                                                                      |
| ----------------------------- | -------------------------------------------------------------------------- |
| FunctionRole::System          | compiler/hir/def.rs:73-87                                                  |
| @phase(...) attribute         | extend attribute parsing at compiler/parser/grammar/func.rs:94-123         |
| MIR-level access analysis     | new pass in compiler/mir/opt.rs peers                                      |
| StateAdvanceRuntimeAdapter    | compiler/engine_frame/runtime.rs:499-594 (unchanged; systems run after it) |
| Deterministic game policy     | compiler/hir/typeck/types.rs:926-948                                       |
| EngineSubsystemKind           | extend at compiler/engine_frame/mod.rs:95-104 with System                  |
| Resource ledger               | extend at compiler/engine_frame/runtime.rs:807-835 for System              |
| resource / event HIR lowering | compiler/hir/lower.rs:89-277 — already handle these                        |

### New files

- compiler/system_contract/mod.rs — SystemContractId, SystemFamilyId { Sim, Presentation, Debug }, SystemPhase, SystemAccessSummary.
- compiler/system_plan/{mod.rs, plan.rs, validation.rs} — SystemPlan { id, contract, phase, reads, writes, mir: MirFunctionId }, SystemProgram { phases: Vec<Vec<SystemPlan>> }, validate_system_program.
- compiler/system_exec/{mod.rs, cpu.rs, observability.rs} — CPU oracle + executor.
- compiler/engine_frame/system_adapter.rs — SystemSubsystemAdapter.
- compiler/mir/passes/system_access.rs — new access-summary MIR pass.

### Code shape

rust #[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SystemPhase {
PreSim,
Sim,
PostSim,
}

pub struct SystemAccessSummary {
pub reads: Vec<SystemResourceId>,
pub writes: Vec<SystemResourceId>,
pub reads_events: Vec<EventTypeId>,
pub emits_events: Vec<EventTypeId>,
}

pub enum SystemResourceId {
Resource(ResourceTypeId),
WorldCapture(RegionCaptureId),
InputFrame,
Snapshot,
SnapshotMut,
}

pub struct SystemPlan {
pub id: SystemId,
pub contract: SystemContractId,
pub phase: SystemPhase,
pub access: SystemAccessSummary,
pub mir: MirFunctionId,
}

pub struct SystemProgram {
pub phases: [Vec<SystemPlan>; 3],
pub event_table: EventRoutingTable,
}

pub struct SystemSubsystemAdapter {
program: SystemProgram,
executor: Arc<Mutex<SystemExecutor>>,
input_slot: Arc<Mutex<Option<InputFrame>>>,
}

impl EngineSubsystemAdapter for SystemSubsystemAdapter {
fn build(&mut self, builder: &mut EngineGraphBuilder) -> Result<EngineSubsystemPlan, EngineFrameError> {
let descriptor = EngineSubsystemDescriptor {
kind: EngineSubsystemKind::System,
label: "system".into(),
// StateAdvance produces the new snapshot; Input produces InputFrame.
// Both must precede systems.
runs*after: vec![EngineSubsystemKind::StateAdvance, EngineSubsystemKind::Input],
requires_gpu: false,
allows_hot_path_readback: false,
};
let mut root_jobs = Vec::new();
let mut terminal_jobs = Vec::new();
let mut previous_phase_terminals: Vec<EngineJobHandle> = Vec::new();
for (phase_idx, phase) in self.program.phases.iter().enumerate() {
let phase_label = match phase_idx { 0 => "pre_sim", 1 => "sim", * => "post*sim" };
// plan_intra_phase_dag uses SystemAccessSummary read/write sets to
// build a DAG inside the phase: any system whose write-set
// intersects another's read- or write-set must be ordered after it.
// Aliasing writers (two systems writing the same resource with no
// declared order) are a SystemPlanError rejected at build time.
let dag = plan_intra_phase_dag(phase)?;
let mut phase_jobs = Vec::new();
for node in dag.topological() {
let deps = if node.predecessors.is_empty() {
previous_phase_terminals.clone()
} else {
node.predecessors.clone()
};
let plan = node.plan.clone();
let exec = Arc::clone(&self.executor);
let input = Arc::clone(&self.input_slot);
let job = builder.add_job(
EngineSubsystemKind::System,
format!("system.{}.{}", phase_label, plan.id),
EngineJobAffinity::Cpu,
EngineSpanDomain::Cpu,
deps,
false,
// FnOnce, no ctx argument — same Arc<Mutex<*>> pattern as
// InputSubsystemAdapter. EngineFrameContext for snapshot
// access is reached via engine*frame_context::current*\*
// accessors set up inside run_frame_with_subsystems.
Box::new(move || {
let input = input.lock().unwrap().clone().expect("input frame");
let ctx = engine_frame_context::current_frame();
exec.lock().unwrap().run_system(&plan, &ctx, &input)
}),
)?;
if node.predecessors.is_empty() { root_jobs.push(job); }
phase_jobs.push(job);
}
previous_phase_terminals = phase_jobs.clone();
if phase_idx == 2 { terminal_jobs = phase_jobs; }
}
let report_kind = EngineSubsystemKind::System;
Ok(EngineSubsystemPlan::new(
descriptor,
root_jobs,
Box::new(move |\_collected| EngineSubsystemReport::new(report_kind)),
))
}
}

Note EngineSubsystemPlan::new takes a report_builder closure, not a terminal_jobs vector — terminal ordering is derived from the job DAG inside the builder. Closure-rule entries for System go into ClosureRuleTable (Phase 62.9): per-system over-budget findings, aliasing-writer findings.

### Author surface

wr
@phase(pre_sim)
system DrainInput(input: InputFrame, @mut player: PlayerState) {
if input.actions.MoveForward.pressed {
player.velocity.z -= 5.0 \* dt()
}
}

@phase(sim)
system IntegrateTransforms(@mut world: PlayerWorld, dt: F32) {
for entity in world.moving {
entity.transform.translate(entity.velocity \* dt)
}
}

@phase(post_sim)
system EmitFrameEvents(world: PlayerWorld, emit: EventEmitter[FrameSummary]) {
emit.send(FrameSummary(player_band=world.player.band))
}

Access sets are derived from parameter annotations (@mut = write, plain reference = read, EventEmitter[T] = emits T, InputFrame = read of the InputFrame resource). Phase 65 lands the annotation-driven path; **MIR-based refinement of access sets is explicitly deferred** as a Phase 65.5 follow-up, because the existing MIR access analyses (compiler/mir/effect_ir.rs:18-92) track effect kinds, not field-level reads/writes — building that takes weeks and is not on the critical path for shipping a working system runtime. Authors who need finer-grained access can split a system into smaller systems or add explicit runs_before ordering hints (a separate @runs_before(OtherSystem) attribute that bypasses the access-set DAG).

### Tests

- compiler/tests/system_access_summary.rs — assert MIR access pass produces expected reads/writes for hand-written systems.
- compiler/tests/system_plan_validation.rs — assert that two systems in the same phase writing the same resource produce SystemPlanError::AliasingWriters.
- compiler/tests/system_determinism.rs — drive SystemSubsystemAdapter for 10 000 ticks with a fixed input trace on two threads; assert bit-identical final world state.
- compiler/tests/system_events_one_tick_deferred.rs — assert an event emitted in tick N is observable only in tick N+1.

### Acceptance

- EngineSubsystemKind::System added; EngineSubsystemReport appears for systems with correct cpu_critical_path_micros.
- examples/systems_basic/ ships a 3-system demo (DrainInput, IntegrateTransforms, EmitFrameEvents); wrela init --template=systems scaffolds it.
- Determinism test passes.
- Getting-started doc gains a system authoring section with phase and access-set explanation.

---

## Phase 66 — RegionResidencyService

### Problem

GpuResidentSceneCache (compiler/gpu_runtime/resident_scene.rs:11-64) is keyed by (SnapshotIdentityReport + detail + GpuLayoutIdentity + selection_signature) and caches resident scene buffers. But the cache is passive — whatever executes a presentation or collision plan populates the cache. There is no live service that, given a follow-target (camera), says "these 16 regions should be resident at 1 Hz upload rate, these 4 at 30 Hz, evict these 8 since they're >200m from the target." The benchmark path fabricates the residency set via manifest. A live host needs streaming.

### Architectural decisions

- **The service lives in the compiler crate** (compiler/residency/), not runtime/. Although it's stateful and driven by runtime events, every type in its public API is a compiler type: WorldSnapshotHandle (compiler/world_identity/mod.rs:182-206), ArtifactReuseKey (compiler/artifact_key/mod.rs:12-82), RegionMetadata (compiler/hir/def.rs:128-132), GpuResidentSceneCache (compiler/gpu_runtime/resident_scene.rs:11-64), FrameUploadArena (compiler/gpu_runtime/upload.rs:131-199). Putting the service in runtime/ would force runtime/ to import compiler types, breaking the layering doctrine and tripping the lint-layering lane added in Phase 62.9. The follow-target geometric input is a small plain-data struct (Transform3 + Velocity3) that lives in compiler/residency/follow.rs.
- **Follow-target is a Transform3 plus optional velocity.** Predictive admission uses velocity × dt to pre-warm regions the target is moving toward.
- **Topology drives candidate set.** Authored regions expose a topology (implicitly RegionLine/RegionGrid/RegionGraph from RFC 0001 — none of these are currently parsed; Phase 66 introduces region topology ... as a new optional clause on region declarations). Without authored topology, every region is a candidate (brute force) — acceptable for small worlds, explicit slow path with a diagnostic.
- **Admit/evict policy is staleness-first, then distance-LRU.** A region with a stale ArtifactReuseKey.compatibility_hash is evicted before a merely-far one. Within equal staleness, LRU + distance.
- **Budget per frame.** ResidencyPolicy { max_upload_bytes_per_frame: u64, max_admits_per_frame: u32, max_evicts_per_frame: u32 }. Residency never exceeds budget in a single frame; overflow is deferred to the next frame.
- **Uploads use FrameUploadArena.** Existing StagingBelt infrastructure (compiler/gpu_runtime/upload.rs:131-199) is the concrete upload path.
- **Reports count against the engine frame.** EngineSubsystemKind::Residency with its own span for plan + admit + evict + upload.
- **Residency is a consumer of WorldSnapshotHandle.** Given a fresh snapshot each tick, the service diffs against the previously-satisfied residency plan and computes the smallest set of admit/evict operations.

### Extension points (existing code)

| What                                    | Where                                                                           |
| --------------------------------------- | ------------------------------------------------------------------------------- |
| GpuResidentSceneCache                   | compiler/gpu_runtime/resident_scene.rs:11-64                                    |
| GpuResidentSceneKey                     | same file                                                                       |
| FrameUploadArena + staging belt         | compiler/gpu_runtime/upload.rs:131-199                                          |
| ArtifactReuseKey + compatibility checks | compiler/artifact_key/mod.rs:12-82                                              |
| WorldSnapshotHandle::with_epoch         | compiler/world_identity/mod.rs:182-206                                          |
| RegionMetadata HIR                      | compiler/hir/def.rs:128-132                                                     |
| Region parser                           | compiler/parser/grammar/mod.rs:106-108, compiler/parser/grammar/func.rs:369-405 |
| EngineSubsystemKind                     | extend enum for Residency                                                       |
| EngineResourceId                        | extend with ResidentRegion { region_id, epoch }                                 |

### New files

- compiler/residency/mod.rs — RegionResidencyService, ResidencyPolicy, ResidencyPlan.
- compiler/residency/topology.rs — RegionLine, RegionGrid, RegionGraph, ResidencyTopology trait.
- compiler/residency/candidate.rs — candidate set computation from topology + follow target.
- compiler/residency/decision.rs — staleness+distance-LRU policy, budget enforcement.
- compiler/residency/follow.rs — FollowTarget { transform: Transform3, velocity: Option<Velocity3> }. Plain-data input to the service; LiveEngineHost constructs it per frame from the camera state in the post-StateAdvance snapshot.
- compiler/engine_frame/residency_adapter.rs — ResidencySubsystemAdapter.
- compiler/region_topology/mod.rs — compiler-side topology descriptors (parser/HIR coverage) and region topology grammar extension.
- examples/streaming_corridor/ — 64-region line topology demo.

### Code shape

rust
pub struct RegionResidencyService {
policy: ResidencyPolicy,
topology: Box<dyn ResidencyTopology>,
resident: BTreeMap<RegionId, ResidentRegionState>,
cache: Arc<GpuResidentSceneCache>,
upload_arena: Arc<Mutex<FrameUploadArena>>,
follow_target: FollowTargetSnapshot,
previous_snapshot: WorldSnapshotHandle,
}

pub struct ResidentRegionState {
region_id: RegionId,
reuse_key: ArtifactReuseKey,
resident_since: SimulationTick,
last_touched: SimulationTick,
bytes: u64,
}

pub struct ResidencyPlan {
pub admits: Vec<ResidencyAdmit>,
pub evicts: Vec<ResidencyEvict>,
pub unchanged: Vec<RegionId>,
pub deferred: Vec<RegionId>,
pub bytes_planned: u64,
}

impl RegionResidencyService {
pub fn plan(&mut self, target: FollowTargetSnapshot, snapshot: &WorldSnapshotHandle, tick: SimulationTick) -> Result<ResidencyPlan, ResidencyError> {
let candidates = self.topology.candidates*for(&target, &self.policy.candidate_window);
let desired = self.score_and_select(candidates, &target, snapshot);
let mut plan = ResidencyPlan::default();
for region in &desired {
match self.resident.get(&region.region_id) {
Some(state) if state.reuse_key.compatible_with(&region.reuse_key) => plan.unchanged.push(region.region_id),
* if plan.bytes*planned + region.bytes <= self.policy.max_upload_bytes_per_frame
&& plan.admits.len() < self.policy.max_admits_per_frame as usize => {
plan.admits.push(ResidencyAdmit::from(region));
plan.bytes_planned += region.bytes;
}
* => plan.deferred.push(region.region*id),
}
}
let desired_set: BTreeSet<*> = desired.iter().map(|r| r.region_id).collect();
for (id, state) in &self.resident {
if !desired_set.contains(id) && plan.evicts.len() < self.policy.max_evicts_per_frame as usize {
plan.evicts.push(ResidencyEvict::lru(state, tick));
}
}
Ok(plan)
}

    pub fn apply(&mut self, plan: &ResidencyPlan, ctx: &mut EngineFrameContext) -> Result<ResidencyReport, ResidencyError> { ... }

}

pub struct ResidencySubsystemAdapter { ... }

impl EngineSubsystemAdapter for ResidencySubsystemAdapter {
fn build(&mut self, builder: &mut EngineGraphBuilder) -> Result<EngineSubsystemPlan, EngineFrameError> {
let descriptor = EngineSubsystemDescriptor {
kind: EngineSubsystemKind::Residency,
label: "residency".into(),
runs_after: vec![EngineSubsystemKind::StateAdvance, EngineSubsystemKind::System],
requires_gpu: true,
allows_hot_path_readback: false,
};
// plan job (CPU) → admits job(s) (GPU, possibly parallel) → evicts job (CPU) → fence
...
}
}

### Author surface

wr
region corridor_band(seed: U64, band: I32) -> RegionCapture {
topology line(axis=WorldAxis.Y, spacing=8.0)
support sphere(radius=6.0)
distance = corridor_field(band=band, seed=seed)
}

view main(world: PlayerWorld) -> FrameState {
follow_target: world.player.transform
residency {
candidate_window: 120.0
max_upload_bytes_per_frame: 2_000_000
max_admits_per_frame: 4
max_evicts_per_frame: 4
}
...
}

### Tests

- runtime/tests/residency_plan_determinism.rs — scripted follow path over 1000 ticks, assert plan sequence bit-identical across runs.
- runtime/tests/residency_budget_enforcement.rs — with a tiny budget, assert admits are deferred, not over-budget.
- runtime/tests/residency_staleness_priority.rs — when a region's generator changes (compatibility_hash mismatch), assert it is evicted before a merely-far region.
- compiler/tests/residency_subsystem_integration.rs — drive through EngineFrameScheduler, assert residency spans present and non-regressing 1080p120 closure.

### Acceptance

- 64-region corridor demo in examples/streaming_corridor/ streams at 1080p120 with zero closure violations.
- EngineSubsystemKind::Residency present in reports; residency_plan, residency_admit, residency_evict spans are emitted.
- Generator version-bump integration test: modifying the region generator causes downstream regions to be evicted and re-admitted with the new key.
- Getting-started doc gains a streaming section.

---

## Phase 67 — Fixed-step physics and move / moveset runtime

### Problem

Physics in a field-native engine is structurally different from traditional rigid-body physics:

- **No meshes.** Geometry is fields; surfaces are zero-level sets. You cannot run GJK/EPA because there is no convex hull to sample. But the collision contract (compiler/collision_contract/mod.rs:7-110) already provides signed distance, normals, and swept intersections as first-class queries through CollisionBatchItem::{PointOccupancy, RayCast, SphereOverlap, SphereSweep, SphereTimeOfImpact} (compiler/collision_plan/batch.rs:24-44). **Penetration depth is field distance with sign flipped; contact normal is field gradient.** No GJK needed.
- **Simulation bodies don't exist yet.** The parser has no body, move, or moveset declarations (confirmed spec-only per exploration in 0001-field-game-language.md:439-454).
- **RFC 0001 mandates physics-authored moves, not prebaked animation clips.**

How do you do physics in this setting, and where does it live in the scheduler?

### Architectural decisions (with rationale)

1. **Physics is a _consumer_ of the collision contract, not an extension of it.** collision_exec remains the pure spatial query service. physics_exec is a time-stepped subsystem that builds CollisionWorkloadBatch values at the start of each physics substep (broadphase), submits them through the existing batch executor, and uses the returned overlap/sweep/TOI witnesses to compute contacts. This keeps the repo's existing layering clean and means collision WGSL/CPU work is already batched correctly.
2. **CPU solver, GPU-resident body state, GPU-batched contact detection.**

- **Solver (integrator + constraint iterations)** is CPU because constraint solvers have serial sub-step dependencies (body A's position correction affects body B's next correction in the same iteration). GPU round-trip for each iteration would be catastrophic.
- **Body state lives in both CPU and GPU** each tick. Authoritative copy on CPU; GPU mirror in a PhysicsBodyBuffer lives in EngineResourceId::PhysicsBodyState { epoch }. The mirror is written once per tick by a single CPU→GPU upload via FrameUploadArena::write_storage_bytes, reused by all collision batches.
- **Contact detection runs through CollisionWorkloadBatch** — the broadphase candidate grouping, WGSL dispatch, and GPU compaction are already correct. Physics is just another workload submitter.
- **Resulting contact evidence is read back to CPU** once per substep (one batched readback). This is unavoidable for a CPU solver; it is budgeted explicitly as physics.contact_readback in the report. Alternative (fully-GPU solver) is a future optimization path after 67 stabilizes.

3. **XPBD (Extended Position-Based Dynamics), not PGS or Featherstone.** Rationale:

- XPBD operates on positions directly, perfect fit for fixed-substep + positional-iteration shape already established by the collision batch model.
- Compliance parameters (alpha = compliance / dt²) map cleanly to authored stiffness on body declarations.
- Mass-ratio stability is acceptable for the target workload (humanoids + deformable terrain regions, not e.g. a ship-with-bolts).
- Deterministic at fixed substep count; no line-search or CG iterations.
- Alternative PGS (Projected Gauss-Seidel) considered and rejected: better for long chains but requires a warm-started velocity solve; XPBD's position-first model is a better fit to field-based penetration evidence.
- Alternative Featherstone: overkill for the target game class (no articulated multi-body chains with many DOFs initially).

4. **CCD via existing SphereSweep + SphereTimeOfImpact (CollisionBatchItem).** For each dynamic body moving faster than body.ccd_threshold_per_substep, emit a SphereSweep item against the world in the same batch as the broadphase overlaps. First-impact results override the discrete-overlap substep position integration.
5. **body declaration compiles to a PhysicsBodyDescriptor.** Support bounds are **derived** from the existing field support metadata (FieldMetadata at compiler/hir/def.rs via lower_field_decl in compiler/hir/lower.rs:1213-1292) — no manual bounding volume authoring. This leverages the compiler's existing knowledge that the compiler already has to produce support metadata for the field; physics just consumes it.
6. **move / moveset compile to a deterministic MoveFsm.** Phase entry/exit, transition conditions, parry windows, and effector assignments are all compile-time known. Runtime only advances timers and evaluates predicates; no dynamic dispatch. The compiler validates the FSM is total (every state has transitions to/from), which is the RFC 0001 requirement for save stability.
7. **Body IDs are artifact-key-stable.** PhysicsBodyId is derived from stable_semantic_id (same function used by ArtifactReuseKey — compiler/artifact_key/mod.rs:80-82) so bodies survive save/load in Phase 69.
8. **Kinematic bodies exist.** body @kinematic is scripted motion (e.g. an elevator); it's not solved, but still generates contacts for other bodies. This is necessary for authoring level geometry that moves.
9. **Hard caps and a CPU-only oracle path are mandatory.** Per-tick caps, default values:

- max_substeps_per_tick: 4 (default 2). Above the cap, the solver clamps and emits a physics.substep_clamped finding via ClosureRuleTable (Phase 62.9).
- max_dynamic_bodies: 128 (default). Above the cap, admission is rejected with a PhysicsError::BodyAdmissionFull.
- physics.contact_readback_ms budget: 1.0 ms per tick. Over-budget emits physics.contact_readback_over_budget finding.
- **CPU oracle path**: a PhysicsBackend::Cpu mode runs the full broadphase + contact detection + solver entirely on CPU, bypassing CollisionWorkloadBatch GPU dispatch. This is the determinism oracle for physics_xpbd_determinism.rs and is the fallback when GPU is unavailable. The GPU-batched path must produce bit-identical results within authored tolerance vs. the CPU oracle. Same discipline as [compiler/collision_exec](compiler/collision_exec) already follows.

### Extension points (existing code)

| What                                                                 | Where                                                                                                     |
| -------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------- |
| CollisionWorkloadBatch                                               | compiler/collision_plan/batch.rs:51-64                                                                    |
| CollisionBatchItem::{SphereOverlap, SphereSweep, SphereTimeOfImpact} | same file lines 24-44                                                                                     |
| execute_batch                                                        | compiler/collision_exec/mod.rs:32-44                                                                      |
| CollisionBatchExecutionReport                                        | compiler/collision_plan/batch.rs:66-102                                                                   |
| FrameUploadArena                                                     | compiler/gpu_runtime/upload.rs:131-199                                                                    |
| GpuBufferPool                                                        | compiler/gpu_runtime/upload.rs:32-94                                                                      |
| FieldMetadata support                                                | compiler/hir/lower.rs:1213-1292                                                                           |
| stable_semantic_id                                                   | query_exec::ids::stable_semantic_id                                                                       |
| FunctionRole                                                         | extend at compiler/hir/def.rs:73-87 with Body, Move, Moveset                                              |
| Parser root dispatch                                                 | extend compiler/parser/grammar/mod.rs:102-151 with body, move, moveset                                    |
| EngineSubsystemKind                                                  | extend with Physics                                                                                       |
| EngineResourceId                                                     | extend with PhysicsBodyState { epoch }, PhysicsContactLedger { tick }, PhysicsMoveState { body_id, tick } |
| TickInputBatch and StateAdvanceRuntimeAdapter                        | compiler/engine_frame/runtime.rs:499-594 (physics runs after state advance, before presentation)          |

### New files

- compiler/physics_contract/mod.rs — PhysicsContractId, PhysicsBodyClass { Dynamic, Kinematic, Static }, PhysicsContactShape { PointSphere, SphereSphere }, PhysicsWitnessKind.
- compiler/physics_plan/mod.rs — PhysicsPlan, PhysicsSubstepPolicy, PhysicsIntegrator { Xpbd }, PhysicsCcdPolicy.
- compiler/physics_exec/mod.rs — dispatcher (CPU / GPU-mirrored).
- compiler/physics_exec/xpbd.rs — XPBD substep: substep(&mut self) { integrate(); broadphase_batch(); detect_contacts(); warm_start_contacts(); solve_positions(iterations); solve_velocities(iterations); finalize(); }.
- compiler/physics_exec/ccd.rs — sweep-based CCD.
- compiler/physics_exec/move_fsm.rs — MoveFsm, MoveInstance, MoveTransitionRequest.
- compiler/physics_exec/report.rs — PhysicsFrameReport { substeps, integrations, contacts_detected, contacts_resolved, ccd_swept_bodies, fallback_count, readback_bytes, ... }.
- compiler/engine_frame/physics_adapter.rs — PhysicsSubsystemAdapter.
- compiler/parser/grammar/physics.rs — parse_body_decl, parse_move_decl, parse_moveset_decl.
- compiler/hir/lower/physics.rs — lower_body_decl, lower_move_decl, lower_moveset_decl.
- examples/physics_playground/ — generic character + 3 authored moves demo.

### Code shape

rust #[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct PhysicsBodyId(pub u64);

pub struct PhysicsBodyDescriptor {
pub id: PhysicsBodyId,
pub class: PhysicsBodyClass,
pub mass_kg: f32,
pub inverse_mass: f32,
pub inverse_inertia_local: Mat3,
pub support: FieldSupport, // derived from authored field support
pub ccd_threshold_per_substep: f32,
pub friction_static: f32,
pub friction_dynamic: f32,
pub restitution: f32,
}

pub struct PhysicsBodyState {
pub id: PhysicsBodyId,
pub transform: Transform3,
pub linear_velocity: Vec3,
pub angular_velocity: Vec3,
pub pending_force: Vec3,
pub pending_torque: Vec3,
pub previous_transform: Transform3, // XPBD position-delta reference
}

pub struct PhysicsContact {
pub body_a: PhysicsBodyId,
pub body_b: Option<PhysicsBodyId>, // None = body vs world field
pub point_world: Vec3,
pub normal_world: Vec3,
pub penetration: f32,
pub lambda_n: f32, // warm-started normal impulse
pub lambda_t: Vec2, // warm-started tangent impulses
pub friction: f32,
pub restitution: f32,
pub generated_by_ccd: bool,
}

pub struct PhysicsSolver {
dt: f32,
substeps_per_tick: u32,
positional_iterations: u32,
velocity_iterations: u32,
bodies: BTreeMap<PhysicsBodyId, PhysicsBodyState>,
descriptors: BTreeMap<PhysicsBodyId, PhysicsBodyDescriptor>,
contact_cache: ContactWarmStartCache,
move_instances: Vec<MoveInstance>,
collision_plan: CollisionPlanHandle,
capture: KernelValue,
domain: KernelValue,
}

impl PhysicsSolver {
fn substep(&mut self, ctx: &mut EngineFrameContext) -> Result<PhysicsSubstepReport, PhysicsError> {
let dt = self.dt / self.substeps*per_tick as f32;
self.integrate_velocities(dt);
self.predict_positions(dt);
let (overlaps, sweeps) = self.build_batch_items(dt);
let batch = CollisionWorkloadBatch {
name: "physics_substep".into(),
workload_id: "physics.substep".into(),
scenario_id: "physics".into(),
plan: self.collision_plan.clone(),
contract_id: "collision.overlap+sweep".into(),
snapshot_id: format!("physics:epoch:{}", ctx.previous_snapshot_epoch).into(),
capture: self.capture.clone(),
domain: self.domain.clone(),
candidate_grouping: CollisionCandidateGroupingPolicy::SharedCandidateDigest,
certification_policy: CollisionCertificationPolicy::MetricsOnly,
items: overlaps.into_iter().chain(sweeps).collect(),
chunk_size: 512,
};
// execute_batch signature: (batch, ctx, store) — see compiler/collision_exec/mod.rs:32-44
let result = collision_exec::execute_batch(&batch, ctx.query_exec_ctx(), None)?;
let contacts = self.contacts_from_collision_results(&result)?;
self.contact_cache.warm_start(&mut contacts);
for * in 0..self.positional*iterations { self.solve_positions_xpbd(&mut contacts, dt); }
self.update_velocities_from_positions(dt);
for * in 0..self.velocity_iterations { self.solve_velocities_xpbd(&mut contacts, dt); }
self.contact_cache.store(&contacts);
self.advance_move_instances(dt, &contacts)?;
Ok(PhysicsSubstepReport::from(&result, &contacts))
}
}

impl EngineSubsystemAdapter for PhysicsSolver {
fn build(&mut self, builder: &mut EngineGraphBuilder) -> Result<EngineSubsystemPlan, EngineFrameError> {
let descriptor = EngineSubsystemDescriptor {
kind: EngineSubsystemKind::Physics,
label: "physics".into(),
runs_after: vec![EngineSubsystemKind::System],
requires_gpu: true,
allows_hot_path_readback: true, // contact readback is budgeted, not forbidden
};
let upload_job = builder.add_job(
EngineSubsystemKind::Physics, "physics.body_upload",
EngineJobAffinity::Cpu, EngineSpanDomain::Cpu,
Vec::new(), false, /_ task _/ ...)?;
let mut previous = vec![upload_job];
let mut all_substep_terminals = Vec::new();
for i in 0..self.substeps_per_tick {
let substep_job = builder.add_job(
EngineSubsystemKind::Physics, format!("physics.substep.{i}"),
EngineJobAffinity::Cpu, EngineSpanDomain::Cpu,
previous.clone(), false, /_ substep task _/ ...)?;
previous = vec![substep_job];
all_substep_terminals.push(substep_job);
}
let move_job = builder.add_job(
EngineSubsystemKind::Physics, "physics.move_fsm",
EngineJobAffinity::Cpu, EngineSpanDomain::Cpu,
previous, false, /_ move fsm task _/ ...)?;
Ok(EngineSubsystemPlan::new(descriptor, vec![upload_job], vec![move_job]))
}
}

### Author surface

wr
body Player {
class: dynamic
mass: 70.0
support: sphere(radius=0.42)
ccd_threshold: 2.0
friction: (static=0.8, dynamic=0.5)
restitution: 0.1
transform: initial_player_transform()
collision_domain: player_combat_domain
}

body Blade @kinematic {
support: capsule(axis=Vec3(0, 0.9, 0), radius=0.03)
transform: initial_blade_transform()
collision_domain: player_combat_domain
}

moveset PlayerMoveset {
initial: idle

    state idle {
        on input.strike => draw_sever
        on input.orbit  => orbit_guard
    }
    state orbit_guard {
        duration: 0.8
        on input.strike => draw_sever
        on complete     => idle
    }
    state draw_sever {
        duration: 0.45
        parry_window: 0.08..0.22
        on contact(Blade, _) during 0.1..0.3 => stagger
        on complete => recovery
    }
    state stagger {
        duration: 0.6
        on complete => idle
    }
    state recovery {
        duration: 0.35
        on complete => idle
    }

}

move draw_sever {
effector: Blade
phases {
windup { 0.0..0.1; blade_pose: shoulder_cocked }
active { 0.1..0.3; blade_pose: arc_swing; contact_allowed: true }
recover { 0.3..0.45; blade_pose: returning }
}
}

Parser additions to compiler/parser/grammar/mod.rs:102-151:
rust
"body" => Some(physics::parse_body_decl(parser)),
"move" => Some(physics::parse_move_decl(parser)),
"moveset" => Some(physics::parse_moveset_decl(parser)),

HIR: FunctionRole::Body, FunctionRole::Move, FunctionRole::Moveset with metadata structs mirroring PresentationMetadata.

### Tests

- compiler/tests/physics_xpbd_determinism.rs — drive solver for 5000 substeps with fixed inputs against both CPU and CPU+GPU-contact backends; assert bit-identical final state.
- compiler/tests/physics_ccd_equivalence.rs — at low velocity, swept and discrete produce the same contact within a tight tolerance; at high velocity, discrete tunnels but swept doesn't.
- compiler/tests/physics_move_fsm.rs — exhaustively walk every authored transition, assert no unreachable states and no deadlocks.
- compiler/tests/physics_body_id_stability.rs — bodies survive compatibility-equal snapshot transitions under the same stable_semantic_id.
- benchmarks/engine_frame/physics_closure.toml — new closure scenario with 64 dynamic bodies + 1 moveset.
- compiler/tests/physics_integration.rs — drive through LiveEngineHost, assert physics spans + physics.contact_readback counted.

### Acceptance

- EngineSubsystemKind::Physics in EngineSubsystemKind; physics.integrate, physics.broadphase, physics.detect_contacts, physics.solve_positions, physics.solve_velocities, physics.move_fsm spans present.
- examples/physics_playground/ ships a generic character (3 bodys + 1 moveset + 3 moves) that runs interactively in apps/reference_host with gamepad input.
- Closure scenario engine_1080p120_physics_closure passes without regressing presentation/collision closure.
- Move FSM totality check is a compile-time error when violated.
- CCD equivalence test passes.
- ClosureRuleTable entries for Physics are landed in compiler/engine_frame/closure_rules.rs (extending the table introduced in Phase 62.9): physics.substep_over_budget, physics.contact_readback_over_budget, physics.substep_clamped, physics.body_admission_full, physics.cpu_oracle_divergence.
- CPU oracle backend (PhysicsBackend::Cpu) is wired and physics_xpbd_determinism.rs exercises it as the reference; the CPU↔GPU divergence test is a standing closure gate.
- Getting-started doc gains a body / move / moveset section.

---

## Phase 68 — Audio subsystem

### Problem

Audio is hard, and the RFC 0001 constraints make it harder:

1. **No imported waveform assets.** Every sound must be procedural — an authored audio field F(x, t) -> F32.
2. **Hard real-time.** Audio callbacks run every ~5ms (48kHz × 256 samples) on a dedicated thread. Missing one deadline produces an audible click.
3. **No allocator on audio thread.** Garbage collection, heap allocation, blocking I/O, panics are all audible failures.
4. **GPU is not a real option.** Round-trip latency from GPU compute shader → mapped buffer → audio device is on the order of 20ms+ in the best case. We cannot use GPU for DSP.
5. **Spatialization needs scene awareness.** Listener-relative spatialization needs occlusion/reverb that depends on the field-authored world — but the field queries are CPU-expensive, so we have to decide what to amortize at the engine-frame boundary vs what to compute per-sample.

### Architectural decisions (with rationale)

1. **CPU-only audio.** No GPU. DSP runs on a dedicated cpal callback thread. This is non-negotiable per the latency analysis above.
2. **@audio_rt as a kernel-fn subset.** Existing kernel fn is already a portable subset with a typed ABI (compiler/kernel/lower.rs:36-99). Phase 68 introduces a stricter subset via attribute @audio_rt. A kernel marked @audio_rt must pass a new validation pass:

- **No heap allocation.** No String, no Vec constructors, no List.append. Only fixed-size stack values.
- **Bounded loops.** Every for must have a compile-time-known upper bound ≤ block_size × max_iterations_per_sample.
- **No blocking effects.** No await, no actor-send, no IO intrinsics. The existing effect tracking in compiler/mir/effect_ir.rs:13-21 already separates Await, Fire, ActorFire from pure code; audio_rt extends this with a new pass that forbids all three.
- **Bounded results.** No Result types propagating; error-by-panic paths replaced with "default safe value" fallbacks.
- **Only math + constant intrinsics.** Table: sin, cos, exp, log, tanh, simple filter state updates, clamp, lerp, wrap. No sqrt in the inner loop (or: allow it, it's 2-3ns on modern cores — measured and enforced with budget).
- This is enforced **at compile time** via a new pass compiler/audio_exec/rt_check.rs that is run on every function in the call graph of any @audio_rt kernel.

3. **Audio field / voice / media as first-class declarations.** Three new top-level decls mirroring the radiance/volume surface (compiler/parser/grammar/mod.rs:122-124):

- audio field F(...) -> F32 — pure audio DSP kernel, must be @audio_rt.
- voice V(source: AudioField, position: Vec3, gain: F32, ...) { envelope: ..., priority: ... } — binds a field to an emitter with runtime state.
- media M(...) — already parsed as a domain flag; Phase 68 adds media field as a new decl for spatial audio attenuation/reverb.

4. **Triple-buffered voice ledger.** The engine-frame tick writes voice state; the audio thread reads the most recent completed buffer. No locks on the audio thread — only an atomic pointer swap.

- Engine thread: writes to ledger slot (n mod 3), then atomically publishes.
- Audio thread: atomically loads the published pointer, reads for the duration of its callback.
- Interpolation: voice state between two published snapshots is linearly interpolated per sample over the block.

5. **48kHz / 256 samples default.** 5.33ms latency, one block per ~0.64 simulation ticks at 120Hz. Override via wrela.toml ([audio] sample_rate = 96000, block_size = 128). These constants are compile-time in the generated DSP graph for maximum specialization.
6. **Binaural panning (ITD + ILD), not HRTF.** HRTF requires per-subject impulse response tables and is a database problem; it's deferred. Phase 68 ships:

- **ITD** (interaural time delay) via one-sample-fraction fractional delay line (Thiran allpass).
- **ILD** (interaural level difference) via head-shadow attenuation model.
- This is good enough for positional audio with head tracking deferred to Phase 70.

7. **Media-field occlusion/reverb driven by per-tick queries, per-sample application, with hard caps.** Occlusion and reverb are expensive. Decision:

- Once per engine frame, **for at most max_full_rate_media_queries voices** (default 16, configurable in [audio]), issue a participants.medium(path=listener↔emitter) query. This gives a typed MediaSample { occlusion_db, reverb_send, lowpass_hz }.
- Voices beyond the full-rate cap are queried at 1/N the rate, round-robin per frame. With 64 voices and 16 full-rate slots, 48 voices are queried in three staggered waves over the next three frames, then refreshed.
- Voice-to-priority sort (decision 8) determines which voices get full-rate queries.
- The result is written into the voice's publish record.
- The audio thread applies the (engine-frame-latency-bounded) filter coefficients per sample with linear interpolation to the next tick's value.
- This means occlusion/reverb lag by one engine frame max (~~8ms) for high-priority voices and up to four frames (~~32ms) for staggered ones — both below perceptual threshold for typical game audio.
- audio.media_queries_over_budget finding fires if any frame exceeds max_full_rate_media_queries. audio.voice_count_over_cap finding fires if voice count exceeds max_voices.

8. **Voice stealing is deterministic.** A global cap max_voices (default 64); when the authored voice count exceeds cap, the priority: I32 field on voices determines which are audible. Tie-break by stable insertion order (via stable_semantic_id).
9. **Audio thread is NOT an EngineSubsystemAdapter.** It runs continuously. What IS an adapter is AudioSnapshotPublisher: a small CPU job each engine frame that takes the current voice state, runs media-field queries, and publishes to the triple buffer. Under-run counters are read back from the audio thread at tick time and emitted into EngineFrameReport.
10. **Media query is an extension of participants.medium, not a new family.** The existing QueryFamilyId::Participants already covers it, and participants.medium already exists as a contract (compiler/query_contract/mod.rs:429-449). Phase 68 specifies that participants.medium accepts a typed result record MediaSample (audio-flavored fields: occlusion_db, reverb_send, lowpass_hz). No participants.medium.audio sub-id is added — the sample type itself carries audio semantics, and the contract wiring is a single new typed return record alongside the existing media records.

### Extension points (existing code)

| What                                           | Where                                                                                                                                                                                                       |
| ---------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Kernel lowering + errors                       | compiler/kernel/lower.rs:36-99                                                                                                                                                                              |
| MIR effect IR (extend for audio_rt block-list) | compiler/mir/effect_ir.rs:13-21, 55-112                                                                                                                                                                     |
| Parser root dispatch                           | compiler/parser/grammar/mod.rs:102-151 add audio, voice, media as prefix keywords                                                                                                                           |
| radiance field parser (template)               | compiler/parser/grammar/func.rs:257-278                                                                                                                                                                     |
| FunctionRole                                   | extend at compiler/hir/def.rs:73-87 with AudioField, Voice, MediaField                                                                                                                                      |
| participants.medium contract                   | existing in compiler/query_contract/mod.rs:429-449; extend the typed result enum with MediaSample for audio                                                                                                 |
| EngineSubsystemKind                            | extend with Audio                                                                                                                                                                                           |
| EngineResourceId                               | extend with AudioVoiceLedger { epoch }                                                                                                                                                                      |
| wrela.toml parse                               | extend project config (compiler/hir/project.rs for LoadedProject; the toml schema extension lives wherever Phase 64 puts the schema struct — add [audio] table there)                                       |
| Closure rule entries for Audio                 | add ClosureRuleTable entries in compiler/engine_frame/closure_rules.rs (table introduced in Phase 62.9): audio.underrun, audio.media_queries_over_budget, audio.voice_count_over_cap, audio.publish_latency |

### New files

- compiler/audio_contract/mod.rs — AudioContractId, AudioFamilyId { Voice, MediaSample }, MediaSample (serializable record with occlusion_db, reverb_send, lowpass_hz).
- compiler/audio_plan/mod.rs — AudioDspPlan, AudioVoicePlan, AudioSpatialPlan.
- compiler/audio_exec/mod.rs — dispatcher.
- compiler/audio_exec/dsp.rs — compiled DSP graph types: AudioDspGraph trait, CompiledDspGraph impl.
- compiler/audio_exec/rt_check.rs — the @audio_rt validation pass.
- compiler/audio_exec/voice_ledger.rs — triple-buffered publish/subscribe.
- compiler/audio_exec/spatial.rs — ITD + ILD + fractional delay + head-shadow filter.
- compiler/audio_exec/snapshot_publisher.rs — AudioSnapshotPublisher: EngineSubsystemAdapter.
- runtime/src/audio/mod.rs — entry point.
- runtime/src/audio/device.rs — cpal device acquisition + stream.
- runtime/src/audio/worker.rs — audio thread callback; no allocator, no lock.
- runtime/src/audio/ring.rs — lock-free sample ring (SPSC).
- runtime/src/audio/underrun_counter.rs — atomic counter published back to engine thread.
- compiler/parser/grammar/audio.rs — parse_audio_field_decl, parse_voice_decl, parse_media_field_decl.
- compiler/hir/lower/audio.rs — HIR lowering.
- examples/audio_field/ — procedural ambience + ping demo.

### Code shape

Kernel-level @audio_rt validation:
rust #[derive(Debug, thiserror::Error)]
pub enum AudioRtError { #[error("audio_rt kernel may not allocate: found {0} at {1}")]
AllocationForbidden(String, SourceSpan), #[error("audio_rt kernel loop has no compile-time bound at {0}")]
UnboundedLoop(SourceSpan), #[error("audio_rt kernel uses blocking effect {0:?} at {1}")]
BlockingEffect(EffectKind, SourceSpan), #[error("audio_rt kernel returns unbounded Result at {0}")]
UnboundedResult(SourceSpan), #[error("audio_rt kernel calls non-audio_rt function {0}")]
NonAudioRtCall(FunctionId),
}

pub fn validate_audio_rt_function(module: &MirModule, function: MirFunctionId) -> Result<(), Vec<AudioRtError>> {
let fn_body = module.function(function);
let mut errors = Vec::new();
forbid_allocations(fn_body, &mut errors);
require_bounded_loops(fn_body, &mut errors);
forbid_blocking_effects(fn_body, &mut errors);
require_bounded_results(fn_body, &mut errors);
require_audio_rt_call_graph(module, function, &mut errors);
if errors.is_empty() { Ok(()) } else { Err(errors) }
}

Runtime audio subsystem:
rust
pub struct AudioSubsystem {
device: cpal::Device,
\_stream: cpal::Stream,
sample_rate: u32,
block_size: u32,
voice_ledger: Arc<TripleBufferedVoiceLedger>,
underrun_counter: Arc<AtomicU64>,
report_tx: crossbeam_channel::Sender<AudioSubsystemFrameReport>,
}

pub struct TripleBufferedVoiceLedger {
slots: [UnsafeCell<VoiceLedgerSlot>; 3],
published: AtomicUsize,
writing: AtomicUsize,
}

pub struct VoiceLedgerSlot {
tick: SimulationTick,
voices: ArrayVec<PublishedVoice, 64>,
listener: ListenerState,
}

pub struct PublishedVoice {
id: VoiceId,
dsp: DspGraphHandle,
position: Vec3,
velocity: Vec3,
gain: f32,
envelope_state: EnvelopeState,
media: MediaSample,
priority: i32,
gate: bool,
}

impl AudioSubsystem {
fn audio_callback(
output: &mut [f32],
ledger: &TripleBufferedVoiceLedger,
sample_rate: u32,
underrun: &AtomicU64,
previous_sample_time: &mut u64,
) {
let deadline = cpal_stream_deadline();
let slot_ptr = ledger.published_ptr();
let slot = unsafe { &*slot_ptr };
let block_samples = output.len() / 2; // stereo
for sample_idx in 0..block_samples {
let t = sample_to_time(*previous_sample_time + sample_idx as u64, sample_rate);
let mut left = 0.0_f32;
let mut right = 0.0_f32;
for voice in &slot.voices {
let (l, r) = voice.dsp.sample_stereo_at(t, voice, &slot.listener);
left += l; right += r;
}
output[2 * sample_idx ] = left;
output[2 * sample_idx + 1] = right;
}
\*previous_sample_time += block_samples as u64;
if Instant::now() > deadline { underrun.fetch_add(1, Ordering::Relaxed); }
}
}

pub struct AudioSnapshotPublisher {
voice_ledger: Arc<TripleBufferedVoiceLedger>,
voices: Vec<VoiceHandle>,
media_query_plan: QueryPlanHandle,
underrun_counter: Arc<AtomicU64>,
last_underrun: AtomicU64,
report_tx: crossbeam_channel::Sender<AudioSubsystemFrameReport>,
}

impl EngineSubsystemAdapter for AudioSnapshotPublisher {
fn build(&mut self, builder: &mut EngineGraphBuilder) -> Result<EngineSubsystemPlan, EngineFrameError> {
let descriptor = EngineSubsystemDescriptor {
kind: EngineSubsystemKind::Audio,
label: "audio".into(),
runs*after: vec![EngineSubsystemKind::System, EngineSubsystemKind::Physics],
requires_gpu: false,
allows_hot_path_readback: false,
};
let publish_job = builder.add_job(
EngineSubsystemKind::Audio, "audio.publish",
EngineJobAffinity::Cpu, EngineSpanDomain::Cpu,
Vec::new(), false,
Box::new({
let ledger = Arc::clone(&self.voice_ledger);
let media_plan = self.media_query_plan.clone();
let voices = self.voices.clone();
let underrun = Arc::clone(&self.underrun_counter);
let last_underrun = self.last_underrun.load(Ordering::Relaxed);
let report_tx = self.report_tx.clone();
move |ctx| {
let media_samples = run_media_queries(&voices, &media_plan, ctx)?;
let active = prioritize_and_steal_voices(&voices, 64);
ledger.write_published(|slot| populate_slot(slot, &active, &media_samples))?;
let current = underrun.load(Ordering::Relaxed);
let delta = current.saturating_sub(last_underrun);
let * = report_tx.send(AudioSubsystemFrameReport {
voices_active: active.len() as u32,
voices_stolen: (voices.len() - active.len()) as u32,
underrun_delta: delta,
media_queries_issued: media_samples.len() as u32,
});
Ok(())
}
}),
)?;
Ok(EngineSubsystemPlan::from_single(descriptor, publish_job))
}
}

### Author surface

wr
audio field Pulse(freq: F32, gate: Bool, t: F32) -> F32 {
@audio_rt
return if gate { sin(two_pi _ freq _ t) } else { 0.0 }
}

audio field FilteredNoise(cutoff: F32, rng: RngState, t: F32) -> F32 {
@audio_rt
noise = white_noise(rng, t)
return lowpass(noise, cutoff)
}

voice AlarmVoice(source: Pulse, position: Vec3, gain: F32) {
envelope: adsr(a=0.01, d=0.1, s=0.7, r=0.3)
priority: 10
media: participants.medium.audio
}

voice AmbienceBed(source: FilteredNoise, position: Vec3, gain: F32) {
envelope: none
priority: 1
media: participants.medium.audio
}

wrela.toml:
toml
[audio]
sample_rate = 48000
block_size = 256
max_voices = 64

### Tests

- compiler/tests/audio_rt_check.rs — validator rejects allocation, unbounded loops, blocking effects, non-audio_rt callees; accepts valid kernels.
- compiler/tests/audio_dsp_offline.rs — compile a simple sine kernel, render 1 second at 48kHz into a buffer, assert sample-exact against a reference.
- compiler/tests/audio_ledger_consistency.rs — engine thread writes N ticks; audio-thread reader never observes a torn slot.
- compiler/tests/audio_voice_stealing_determinism.rs — with >max_voices voices, assert stable deterministic stealing order.
- runtime/tests/audio_headless.rs — CI: no device required, use cpal::default_host().default_output_device() fallback to null device. Offline render path validates DSP graph without a real stream.
- compiler/tests/audio_integration.rs — LiveEngineHost + AudioSnapshotPublisher, assert audio span in EngineFrameReport.

### Acceptance

- EngineSubsystemKind::Audio added; audio.publish span present.
- examples/audio_field/ ships a procedural 3-voice demo (pulse alarm + filtered ambience + modulated ping); wrela init --template=audio scaffolds it.
- Offline sample-exact DSP test passes.
- Underruns over a 60-second wrela live run are 0 on a reference machine.
- ClosureRuleTable entries for Audio are landed in compiler/engine_frame/closure_rules.rs (extending the table introduced in Phase 62.9): audio.underrun, audio.media_queries_over_budget, audio.voice_count_over_cap, audio.publish_latency. Any underrun_delta > 0 in a frame becomes a finding.
- @audio_rt validation is a compile error (not a warning) when violated; error messages are concrete.
- Media-query staggering verified: an audio_64_voice_closure.toml scenario with 64 voices stays under the 16/frame full-rate cap and produces no audio.media_queries_over_budget findings.
- Getting-started doc gains an audio authoring section.

---

## Phase 69 — Save and load runtime

### Problem

RFC 0001 states the save-stability contract (regions, scatter, persistent handles stable under unchanged generators) but there is no save/load runtime. The compiler already has most of the infrastructure for stability:

- ArtifactReuseKey.compatibility_hash: u64 (compiler/artifact_key/mod.rs:12-21) already discriminates compatible-vs-not content hashes.
- stable_semantic_id (defined in compiler/query_exec/ids.rs, used at compiler/world_identity/mod.rs:183-188) already produces artifact-key-stable IDs for semantic objects.
- WorldSnapshotHandle::with_epoch (compiler/world_identity/mod.rs:182-206) already re-derives artifact-key seed and snapshot entity id across epoch transitions.

What's missing is: serialize the _current epoch's_ world state to disk in a format that survives process restarts and generator version changes, and deserialize it back into a valid WorldSnapshotHandle with identical residency behavior.

### Architectural decisions

- **Reuse compatibility_hash as the save-compatibility key.** Save header stores the generator set's compatibility_hashs per region/archetype. On load, comparing-equal hashes means "load directly"; mismatches produce a SaveIncompatibility diagnostic naming the changed generator. No silent migration.
- **CBOR, not JSON, not custom binary.** CBOR (ciborium crate) is:
  - Compact enough for frame-sized state (≪ JSON).
  - Schema-evolvable via named fields.
  - Deterministic encoding order via serde's struct field order.
  - **ciborium is a new direct dependency added by this phase** (it is not currently in the tree). The crate is small (one transitive serde dep already in tree), well-maintained, and the leading no-std-friendly CBOR option for Rust.
- **Persistent handles are stable semantic IDs.** A PersistentHandle is just a StableSemanticId. Loading reconstitutes bodies, voices, entities with these IDs — physics body state survives save/load without drift.
- **Snapshot save format = header + ledger + body store.**
  - Header: engine version, compatibility_hash, sim tick, presentation frame, seed state.
  - Ledger: list of { PersistentHandle, TypeId, payload: CborValue } records.
  - Body store: compressed payload, CBOR.
- **Load is a LoadPlan that StateAdvanceRuntimeAdapter consumes in a special "first tick after load" mode.** The load plan drives a synthetic TickInputBatch that reconstructs state by invoking the authored system with @phase(pre_sim) load event. Authors opt into load-time behavior explicitly via @on_load systems.
- **Save is a one-shot engine-frame subsystem job.** SavePublisher: EngineSubsystemAdapter runs after Presentation, dumps state via reflection from MIR-emitted persistence metadata. It only runs on ticks where save_request_flag is set; otherwise it's a zero-work no-op.

### Extension points (existing code)

| What                            | Where                                                              |
| ------------------------------- | ------------------------------------------------------------------ |
| ArtifactReuseKey + policy modes | compiler/artifact_key/mod.rs:12-82                                 |
| stable_semantic_id              | compiler/query_exec/ids.rs                                         |
| WorldSnapshotHandle             | compiler/world_identity/mod.rs:42-206                              |
| SnapshotEpoch::INITIAL          | compiler/world_identity/mod.rs:66-71                               |
| StateAdvanceRuntimeAdapter      | compiler/engine_frame/runtime.rs:499-594 — add "load mode" variant |
| resource HIR lowering           | compiler/hir/lower.rs:89-277 (extend with @persistent)             |
| CLI                             | add wrela save/load commands at compiler/bin/wrela/commands/       |

### New files

- compiler/persistence/mod.rs — SnapshotSaveRecord, PersistentHandle, PersistenceError. (Compiler-side because WorldSnapshotHandle, LoadedProject, StableSemanticId are compiler types — same layering rationale as Phase 66 residency.)
- compiler/persistence/header.rs — SnapshotSaveHeader, HeaderCompatibility, SaveIncompatibility.
- compiler/persistence/snapshot.rs — serialize/deserialize logic.
- compiler/persistence/load_plan.rs — LoadPlan, load_into_runtime.
- compiler/persistence_contract/mod.rs — declared attributes, per-type schemas.
- compiler/engine_frame/save_adapter.rs — SavePublisher: EngineSubsystemAdapter.
- compiler/bin/wrela/commands/save.rs, .../load.rs.
- examples/save_and_load/ — save slots + version-bump recovery demo.

### Code shape

rust
pub struct SnapshotSaveRecord {
pub header: SnapshotSaveHeader,
pub body: Vec<u8>, // zstd-compressed CBOR
}

pub struct SnapshotSaveHeader {
pub wrela_version: String,
pub project_id: String,
pub engine_compatibility_hash: u64,
pub generator_compatibility_hashes: BTreeMap<String, u64>,
pub archetype_schema_hashes: BTreeMap<String, u64>,
pub sim_tick: u64,
pub presentation_frame: u64,
pub saved_at_unix_nanos: u64,
pub cbor_schema_version: u32,
}

pub enum HeaderCompatibility {
Exact,
CompatibleMigrateUp { warnings: Vec<String> },
Incompatible { reason: SaveIncompatibility },
}

pub enum SaveIncompatibility {
EngineVersionMismatch { saved: String, running: String },
GeneratorDiverged { name: String, saved_hash: u64, running_hash: u64 },
ArchetypeSchemaChanged { name: String, saved_hash: u64, running_hash: u64 },
ProjectIdMismatch { saved: String, running: String },
}

pub struct PersistentHandle(pub StableSemanticId);

pub fn save_snapshot(
runtime: &EngineFrameRuntime,
snapshot: &WorldSnapshotHandle,
project: &LoadedProject,
path: &Path,
) -> Result<SnapshotSaveRecord, PersistenceError> { ... }

pub fn load_snapshot(
record: SnapshotSaveRecord,
project: &LoadedProject,
) -> Result<(WorldSnapshotHandle, LoadPlan), PersistenceError> { ... }

### Author surface

wr
@persistent
resource PlayerProgress {
band: I32
total_distance: F32
bosses_defeated: Set[BossId]
}

@on_load
system RestorePlayerProgress(load: LoadEvent, @mut progress: PlayerProgress) {
progress = load.read[PlayerProgress]()
}

### Tests

- compiler/tests/persistence_round_trip.rs — start runtime, drive 5000 ticks, save, restart process, load, drive 5000 more ticks, assert bit-identical trajectory.
- compiler/tests/persistence_version_bump.rs — modify a generator function, assert load returns HeaderCompatibility::Incompatible { reason: GeneratorDiverged { .. } }.
- compiler/tests/persistence_payload_schema.rs — unchanged generator but changed archetype schema produces ArchetypeSchemaChanged.
- compiler/tests/persistence_handle_stability.rs — PersistentHandles are identical across sessions for the same logical entity.

### Acceptance

- Save/load round trip for a non-trivial scene produces bit-identical trajectories.
- wrela save and wrela load CLI commands work end-to-end.
- SaveIncompatibility diagnostic points at the exact changed generator by name.
- examples/save_and_load/ ships a save-slots + version-bump recovery demo; wrela init --template=persistent scaffolds it.
- Getting-started doc gains a persistence section.

---

## Phase 70 — Reference host and editor-scale inspector

### Problem

apps/frame_live_app ([apps/frame_live_app/src/lib.rs](apps/frame_live_app/src/lib.rs), 519 lines) is a perf-flavored inspector: a worker thread runs FrameLiveSession headless-style, and the UI displays the latest FrameLiveFrame as an egui::ColorImage. It is not interactive (no input → state → frame loop) and its inspection surface is limited to the presentation output.

We need a reference host that (a) is a real interactive target for authors, and (b) is an editor-scale inspection surface that cross-references every EngineFrameReport row back to the live state that produced it.

### Architectural decisions

- **Sibling, not replacement.** apps/frame_live_app stays as the perf inspector. apps/reference_host is the runtime inspector.
- **Single codebase with two modes.** apps/reference_host compiled with --features=inspector shows inspector panels alongside the 3D surface; without the feature, it's just the game window. Inspector state is never on the hot path.
- **Click-through from report to live state.** Each EngineFrameReport subsystem row has a click handler that reveals the live subsystem state. E.g., clicking the physics row opens a panel with all active PhysicsBodyStates and MoveInstances; clicking audio opens a voice list with current envelope states; clicking residency opens a map of currently-resident regions with upload budgets.
- **Hot reload via generalized FrameLiveSession::reload_if_sources_changed.** Extended in Phase 65 to also reload systems, in Phase 67 to reload movesets, in Phase 68 to reload audio DSP graphs.
- **Just one new just lane.** just ship-interactive runs the reference-host smoke test as part of pre-handoff gates.

### Extension points (existing code)

| What                                                   | Where                                                                    |
| ------------------------------------------------------ | ------------------------------------------------------------------------ |
| apps/frame_live_app/src/lib.rs worker-pattern template | [apps/frame_live_app/src/lib.rs](apps/frame_live_app/src/lib.rs):147-247 |
| FrameLiveSession hot-reload                            | compiler/frame_live.rs:297-390                                           |
| EngineFrameReport span IDs                             | compiler/engine_frame/mod.rs:189-231                                     |
| EngineResourceLedger                                   | compiler/engine_frame/runtime.rs:150-162                                 |
| wrela dev                                              | compiler/bin/wrela/commands/command_dispatch.rs:753-775                  |

### New files

- apps/reference_host/Cargo.toml, apps/reference_host/src/{main.rs, lib.rs}.
- apps/reference_host/src/inspector/{mod.rs, physics.rs, audio.rs, residency.rs, systems.rs, persistence.rs, timeline.rs}.
- apps/reference_host/tests/smoke.rs.
- justfile gains a ship-interactive lane.

### Author surface

No new language surface in Phase 70. This phase is pure wiring.

### Tests

- apps/reference_host/tests/smoke.rs — with WRELA_TEST_OFFSCREEN=1, run the host for 15 seconds against examples/physics_playground/ (configurable via WRELA_REF_HOST_SMOKE_SECS, default 15, just ship-interactive overrides to 60). Assert no closure violations, no audio underruns, all subsystem spans present.

### Acceptance

- A fresh contributor can wrela init demo --template=full_stack then wrela dev demo and get an interactive 1080p120 window with input, systems, residency, physics, audio, and save/load working against a generic demo project.
- Inspector mode reveals every EngineFrameReport row as clickable, with a live panel per subsystem.
- just ship-interactive runs the reference-host smoke test as part of pre-handoff gates.
- Getting-started doc ends with an end-to-end "from wrela init to playable vertical slice" walkthrough.

---

## Explicit non-goals

- No Staircase-specific content anywhere.
- No editor UI beyond inspection (authoring surfaces stay text-based; a real editor is post-RFC-0011).
- No networking or multiplayer.
- No replacing benchmark lanes; the perf-closure path keeps its own dedicated surface.
- No imported assets — fields remain the only content surface, per RFC 0001.
- No HRTF in Phase 68 (binaural panning only).
- No fully-GPU physics solver in Phase 67 (CPU solver + GPU-batched contact detection).
- No multi-body articulated dynamics in Phase 67 (single-body XPBD + kinematic bodies; articulated chains are a future extension).

## Risks and mitigations

- **Risk**: Lowest-possible-latency stance causes peak frame rate to drop below 120 fps on common hardware. **Mitigation**: this is an _accepted_ tradeoff per the latency-first thesis. presentation.framerate_below_target fires as a warning, not an error. Authors who prefer throughput can opt into wrela.toml [presentation] mode = "throughput" which sets max_frames_in_flight: 2 and present_mode = "fifo".
- **Risk**: Mailbox present mode tears on hardware below the target frame rate. **Mitigation**: VRR-aware FIFO fallback path; explicit author opt-in to plain FIFO via wrela.toml; documented tradeoff in the getting-started doc. Closure rule emits a warning when fallback occurs so the author knows their hardware fell back.
- **Risk**: Late-sampled input drains an empty ring on first frame and produces a spurious 0-event tick. **Mitigation**: ring is pre-warmed with a synthetic "frame_start" event timestamped at host startup; first frame sees at least one event with a non-zero stamp.
- **Risk**: Input ring overflow on bursty input (e.g., gamepad sample storm). **Mitigation**: 4096-event capacity covers 100 ms of input at typical 1 kHz polling. Overflow emits presentation.input_ring_overflow finding; ring auto-recovers (oldest-dropped policy).
- **Risk**: Motion-to-photon stage 5 (estimated_present_to_photons_nanos) is a worst-case display refresh estimate, which may overstate latency on VRR displays within range. **Mitigation**: when VRR is detected, stage 5 is reported as 0; measurement_quality: EstimatedFromCpuClock flags inferred values.
- **Risk**: max_frames_in_flight: 1 causes GPU bubble between frames on slow GPUs. **Mitigation**: closure rule presentation.gpu_idle_excessive (delta between gpu_complete and next submit) catches this; recommend authors override to 2 only when a finding fires.
- **Risk**: wrela perf-latency is fragile on machines with VRR + variable refresh rate. **Mitigation**: lane records refresh-rate samples per frame; results are normalized against the median observed refresh rate.
- **Risk**: wgpu swapchain integration breaks the framegraph's attachment identity assumptions. **Mitigation**: Phase 62.9 adds the AttachmentKind::SwapchainColor role and from_plan_and_gpu_resources_with_swapchain constructor _before_ anything depends on it; the benchmark framegraph path uses the old constructor unchanged.
- **Risk**: Substrate work in Phase 62.9/62.95 is invisible to authors and creates the temptation to skip it. **Mitigation**: Phases 64/67/68 explicitly list 62.9 and 62.95 acceptance criteria as prerequisites; the closure-rule table replay test enforces parity against pre-62.9 baseline so the substrate change cannot land without proving non-regression.
- **Risk**: Layering decay — a future contributor adds a runtime/src/foo.rs that imports compiler::Bar, breaking the one-way invariant. **Mitigation**: Phase 62.9 ships just lint-layering as a CI lane that fails on any compiler import in runtime/ outside #[cfg(test)]. Wired into just lint.
- **Risk**: One-adapter-per-kind invariant catches a legitimate use case in the wild. **Mitigation**: EngineSubsystemKind::FutureReserve(name) deliberately allows multiple instances keyed on the name; new subsystems with multi-adapter needs use FutureReserve until they can be lifted into a dedicated kind in a successor RFC.
- **Risk**: Phase 65 system scheduler depends on MIR-inferred read/write sets that the existing MIR doesn't produce. **Mitigation**: Phase 65 lands annotation-driven access sets only (@mut and the EventEmitter[T] parameter type carry full information). MIR refinement is explicitly a Phase 65.5 follow-up that does not block Phase 66/67/68 from starting.
- **Risk**: EngineFrameRuntimePolicy::live() is too permissive and lets gameplay code start doing readbacks in the hot path. **Mitigation**: live() keeps allow_hot_path_gameplay_readbacks: false and allow_private_gpu_submits: false; only max_change_class is relaxed from Identity to Behavior. Tools-only operations (overlay capture, pixel-pick) use EngineFrameRuntimePolicy::tools(), which is gated behind --inspector and never the default.
- **Risk**: Fixed-step simulation plus variable-step presentation produces interpolation drift. **Mitigation**: Phase 63 defines the interpolation contract; Phase 67 provides a CPU-deterministic reference simulation for diffing.
- **Risk**: XPBD stiffness tuning is per-project. **Mitigation**: body.compliance defaults produce stable contact for typical humanoid masses; authored override via declaration; closure gates include physics-stability findings.
- **Risk**: Physics caps (max_substeps_per_tick, max_dynamic_bodies, physics.contact_readback_ms) become silent gameplay limits. **Mitigation**: every cap has a corresponding ClosureRuleTable finding (Phase 62.9), so hitting a cap is loud, not silent. CPU oracle backend gives authors a no-cap reference run for diagnosis.
- **Risk**: Audio @audio_rt validation is too restrictive and authors route around it with non-audio-rt kernels. **Mitigation**: error messages name the specific offending intrinsic/pattern with a one-line suggested alternative; a catalog of portable @audio_rt-safe helpers ships in the stdlib (language/stdlib/audio/).
- **Risk**: Audio media-query budget (16/frame full-rate) silently degrades occlusion fidelity for far voices. **Mitigation**: closure rule audio.media_queries_over_budget catches over-budget frames; the round-robin staggering preserves correctness within ~32ms for low-priority voices, well below perceptual threshold.
- **Risk**: Contact readback per substep is the dominant frame cost in Phase 67. **Mitigation**: measured and budgeted as physics.contact_readback_ms (1ms cap) on the report; if it exceeds the cap, a closure finding is emitted with the suggested GPU-resident solver as a future path.
- **Risk**: Scope creep into Staircase. **Mitigation**: reference host ships with a deliberately generic demo; any test naming a Staircase construct is rejected in review.
- **Risk**: Pressure to add a classic asset import path as the repo becomes more visible. **Mitigation**: field-native packaging (wrela init, wrela.toml, examples/, generator versioning) is explicitly called the content-pipeline equivalent; any decision to relax RFC 0001 non-goals requires a dedicated successor RFC.

## What this buys us

After Phase 70 the repo is, for the first time, a **game engine** in the ordinary sense — and one tuned for _feel_. A developer can wrela init demo --template=full_stack, then wrela dev demo, open a real window targeting 16 ms input-to-photon latency (12 ms in competitive mode), move a character with a gamepad driven by authored physics and moves, hear field-authored procedural audio, see regions stream around them, save, quit, come back, load, and keep playing — and every subsystem is CPU-testable, budget-accounted, closure-gated, latency-measured, and integrated into the same EngineFrameReport pipeline the perf closure lane already uses.

The latency contract sets it apart: most engines treat motion-to-photon as a tunable; this one treats it as the primary product property, with framerate as the secondary objective and explicit warnings when they conflict. A 60 fps Wrela game on commodity hardware lands at roughly the same motion-to-photon latency as a 120 fps AAA engine — and the 120 fps Wrela path on capable hardware lands well under it.

Staircase becomes a downstream game project built on top, not the engine itself.
