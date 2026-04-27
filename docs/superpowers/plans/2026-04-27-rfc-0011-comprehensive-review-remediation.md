# RFC 0011 Comprehensive Review Remediation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` for parallel execution or `superpowers:executing-plans` for serial execution. Track progress with the checkbox steps below. This plan supersedes `2026-04-27-rfc-0011-seven-finding-remediation.md`.

**Goal:** Resolve the initial seven review findings plus the expanded RFC 0011 audit. The current tree has useful substrate pieces, but several subsystems are wired as stubs or report free-form violations that do not become structured closure findings.

**Strategy:** Fix observability and latency primitives first, because they make every later subsystem honest. Then replace project/runtime stubs with explicit production boundaries. Finally complete the heavy subsystem bodies: systems, residency, physics, audio, and persistence.

**Important audit correction:** `justfile` already contains `lint-layering`, `perf-latency`, `dev-smoke`, and `ship-interactive`. The remediation is to strengthen what those lanes exercise, not to add missing lane names.

---

## Finding Map

- Findings 1, 6, 12, 13, 29, 31: covered by Task 1, structured closure findings and presentation/input reporting.
- Findings 2, 10, 11, 37: covered by Task 2, latency-critical platform primitives.
- Findings 3, 8, 9, 14, 15, 38: covered by Task 3, live CLI/reference host project integration and real timing.
- Findings 4 plus expanded ledger inconsistencies: covered by Task 1.
- Findings 5, 16, 17, 18, 19: covered by Task 4, system runtime and scheduler semantics.
- Findings 20, 21, 22: covered by Task 5, residency policy correctness.
- Findings 6, 23, 24, 25: covered by Task 6, physics backend completion.
- Findings 26, 27, 28, 29, 30, 31, 32: covered by Task 7, audio runtime completion.
- Findings 33, 34, 35, 36: covered by Task 8, persistence contract alignment.
- Test/justfile concerns: covered by Task 9. Lanes exist, but `perf-latency` is currently too synthetic.

---

## Task 1: Make Closure Findings and Resource Ledgers Canonical

**Addresses:** Initial finding 4; expanded findings 1, 2, 3, 4, 6, 7, 12, 13, 29, 31.

**Files:**
- `compiler/engine_frame/closure_rules.rs`
- `compiler/engine_frame/runtime.rs`
- `compiler/engine_frame/scheduler.rs`
- `compiler/engine_frame/latency.rs`
- `compiler/engine_frame/input_adapter.rs`
- `compiler/engine_frame/residency_adapter.rs`
- `compiler/engine_frame/physics_adapter.rs`
- `compiler/engine_frame/audio_adapter.rs`
- `compiler/engine_frame/save_adapter.rs`
- `compiler/bin/wrela/commands/live_command.rs`
- `compiler/tests/closure_rules_engine_frame.rs`
- `apps/reference_host/tests/smoke.rs`

- [ ] Add failing tests that prove subsystem violations become `report.closure_findings`, not only `report.violations`.
  - Include `physics.contact_readback_over_budget`.
  - Include `audio.underrun`.
  - Include `presentation.input_ring_overflow`.
  - Include `presentation.fallback_to_vsync_fifo`.
  - Include a generic `save.*` or `system.*` violation.

- [ ] Replace the one-off `apply_latency_budget_to_report` filtering with a generic rule-table pass.
  - It must not filter on `focus == "motion_to_photon_budget"`.
  - It should run once after all subsystem reports, resource ledger entries, and `report.violations` are populated.
  - It should append all findings emitted by canonical rules.

- [ ] Cache the canonical closure rule table.
  - Use `OnceLock<ClosureRuleTable>` if the table is immutable and `Sync`.
  - If trait objects make that awkward, store it in `EngineFrameRuntime` during construction.
  - Do not allocate `Vec<Box<dyn ...>>` in the frame hot path.

- [ ] Register RFC 0011 rules in `ClosureRuleTable::with_canonical_engine_frame_rules`.
  - `presentation.fallback_to_vsync_fifo`
  - `presentation.input_ring_overflow`
  - `presentation.motion_to_photon_over_budget`
  - `presentation.motion_to_photon_perf_lane_over_budget`
  - `presentation.framerate_below_target`
  - `system.*`
  - `residency.*`
  - `physics.substep_over_budget`
  - `physics.contact_readback_over_budget`
  - `physics.substep_clamped`
  - `physics.body_admission_full`
  - `physics.cpu_oracle_divergence`
  - `audio.underrun`
  - `audio.media_queries_over_budget`
  - `audio.voice_count_over_cap`
  - `audio.publish_latency`
  - `save.*`

- [ ] Add a short-term bridge rule that converts known `report.violations` prefixes into `PerfClosureFinding`.
  - Keep this as an adapter while subsystem reports are made typed.
  - Do not parse arbitrary strings for numeric payloads unless the typed report lacks the field.

- [ ] Convert subsystem adapters away from pushing only raw strings.
  - Audio: expose underrun count, stolen voices, media query budget, and publish latency in `AudioFrameReport`.
  - Physics: expose substeps, clamping, contact readback bytes/time, admission failures, and oracle divergence in `PhysicsFrameReport`.
  - Residency: expose stale evictions, deferred admits, budget exhaustion, GPU cache failures.
  - Presentation/Input: expose FIFO fallback and input ring overflow in the presentation/input report surface.

- [ ] Fix `MeasurementQuality::default`.
  - Change default from `Synthetic` to `EstimatedFromCpuClock`.
  - Set `Synthetic` explicitly in benchmark/replay constructors.

- [ ] Remove duplicate `validate_state_advance_change_class`.
  - Keep the validation in one place.
  - Prefer the job-local validation if it is the source of the adapter result.
  - Add a regression test where a disallowed change class fails exactly once and produces one diagnostic.

- [ ] Fix resource ledger entries for new subsystems.
  - Residency must record actual admitted/evicted region IDs, not `region_id: "*"`.
  - Physics `PhysicsContactLedger { tick }` must receive simulation tick, not snapshot epoch.
  - Audio `AudioVoiceLedger { epoch }` must include an `EngineResourceAccess::Write` row.
  - Save/load must record symmetrical save and load access rows.
  - System snapshot reads must reference the input/previous snapshot epoch if systems run before output commit.

- [ ] Update `wrela live --enforce-latency-budget`.
  - The flag name can remain for compatibility, but enforcement should fail on any error-severity `closure_findings`, not only latency findings.

**Verification:**

```bash
cargo test -p wrela --test closure_rules_engine_frame
cargo test -p wrela --test engine_frame_resource_ledger
cargo test -p wrela --test live_host
just perf-engine-closure
```

---

## Task 2: Repair Latency-Critical Platform Primitives

**Addresses:** Initial findings 1, 2, 7; expanded findings 10, 11, 12, 13, 37.

**Files:**
- `compiler/engine_frame/latency.rs`
- `compiler/engine_frame/runtime.rs`
- `compiler/engine_frame/input_ring_bridge.rs`
- `runtime/src/platform/input_pump.rs`
- `runtime/src/platform/frame_pacing.rs`
- `runtime/src/platform/surface.rs`
- `apps/reference_host/src/lib.rs`
- `runtime/tests/winit_input_pump_ring.rs`
- `runtime/tests/frame_in_flight_semaphore.rs`
- `compiler/tests/live_host.rs`

- [ ] Add a sampler-deadline hook to `LateInputSampler`.
  - The live sampler should use the same monotonic origin as raw input events.
  - `StateAdvanceRuntimeAdapter` should drain `TickInputSource::Late` with sampler-provided `now()`, not `current_clock.wall_clock`.
  - Store the exact sample deadline used by the state-advance job in the report context.

- [ ] Add a regression test for clock-domain drift.
  - Create an event timestamped from a host clock.
  - Advance `LiveEngineHost` using a deliberately different synthetic fixed-step wall clock.
  - Assert `event_arrival_to_state_advance_nanos` uses host sample time.

- [ ] Replace lock-backed raw input ring with split SPSC handles.
  - Public runtime API should expose `RawInputProducer` and `RawInputConsumer`.
  - `RawInputProducer` owns `rtrb::Producer`.
  - `RawInputConsumer` owns `rtrb::Consumer`.
  - Shared telemetry should be atomics only.
  - Remove `Mutex<Producer<...>>` and `Mutex<Consumer<...>>` from `runtime/src/platform/input_pump.rs`.

- [ ] Make input overflow visible to the frame report.
  - The consumer/sampler should return or expose the overflow latch state.
  - The Input or Presentation adapter should emit `presentation.input_ring_overflow`.
  - Clear the latch only after the frame has recorded it.

- [ ] Make frame pacing GPU-driven.
  - Add `FrameInFlightSemaphore::release_after_submitted_work_done(self: &Arc<Self>, queue: &wgpu::Queue)`.
  - Remove immediate release after `host.advance`.
  - Release immediately only on no-submit paths or errors before the queue callback is scheduled.
  - Add a test where frame N+1 blocks until simulated queue completion releases frame N.

- [ ] Fix present-mode fallback reporting.
  - Make `select_wgpu_present_mode` return a typed selection result with mode, fallback reason, and finding code.
  - Ensure `PreferMailboxThenVrrFifoThenFifo` does not mark FifoRelaxed as a warning fallback.
  - Ensure plain FIFO fallback emits `presentation.fallback_to_vsync_fifo`.

- [ ] Collapse or harden the two reference-host swapchain paths.
  - Prefer a single swapchain handle path if feasible.
  - If pre-acquire remains, tie its slot lifecycle to the GPU frame token so a second frame cannot consume an empty slot.
  - Convert `"missing preacquired texture"` from a reachable race into an impossible state guarded by tests.

**Verification:**

```bash
cargo test -p wrela_runtime --test winit_input_pump_ring
cargo test -p wrela_runtime --test frame_in_flight_semaphore
cargo test -p wrela --test live_host live_engine_host_late_sampler_materializes_timestamped_events
WRELA_TEST_OFFSCREEN=1 cargo test -p wrela_reference_host --test smoke
```

---

## Task 3: Make `wrela live` and Reference Host Exercise Real Project Runtime Paths

**Addresses:** Initial finding 3; expanded findings 8, 9, 14, 15, 38.

**Files:**
- `compiler/bin/wrela/cli_args.rs`
- `compiler/bin/wrela/commands/live_command.rs`
- `compiler/engine_frame/live.rs`
- `apps/reference_host/src/main.rs`
- `apps/reference_host/src/lib.rs`
- `apps/reference_host/tests/smoke.rs`
- `justfile`

- [ ] Remove the interactive `wrela live` parser rejection.
  - Keep validation for invalid frame counts.
  - Add parser tests for headless and interactive forms.

- [ ] Route interactive `wrela live` to the reference host.
  - Avoid a Cargo dependency cycle by launching `wrela_reference_host` as a workspace binary or by factoring a shared host library if the workspace already permits it.
  - Pass project path, frame count, offscreen mode, and inspector/tool flags explicitly.

- [ ] Replace no-op executor defaults in production paths.
  - Headless smoke may keep an explicitly named test executor.
  - Project-driven `wrela live` and reference host runs must construct adapters from `LoadedProject` or fail with a clear “runtime backend not implemented for this project feature” diagnostic.
  - Do not silently discard `_loaded_project`.

- [ ] Plumb simulation rate.
  - Use project config or CLI override.
  - Default live simulation rate should come from present/display refresh where available.
  - `LiveEngineHost::new_headless` should start at honest wall time zero; tests should not rely on seeding wall time to one step.

- [ ] Use real elapsed time in `ReferenceHostApp::tick`.
  - Replace hardcoded `1.0 / 60.0` with elapsed seconds since the previous tick.
  - Clamp extreme hitches with an explicit diagnostic instead of pretending they were 16.67 ms.

- [ ] Make `perf-latency` exercise the interactive/reference-host path.
  - The lane exists; change it from synthetic headless-only to offscreen reference-host execution.
  - Record p50/p95/p99 latency from real host reports.
  - Keep headless `live-smoke` as a cheap substrate test, not the latency gate.

- [ ] Make smoke duration-sensitive.
  - Honor `WRELA_REF_HOST_SMOKE_SECS`.
  - Default to a short local duration, but `just ship-interactive` should run the longer configured duration.

**Verification:**

```bash
cargo run -p wrela -- live examples/surface_and_input/src/main.wr --headless --frames=2 --json
WRELA_TEST_OFFSCREEN=1 cargo run -p wrela -- live examples/surface_and_input/src/main.wr --frames=2
just perf-latency
just ship-interactive
```

---

## Task 4: Complete System Runtime Semantics

**Addresses:** Initial finding 5; expanded findings 16, 17, 18, 19.

**Files:**
- `compiler/system_exec/mod.rs`
- `compiler/system_plan/mod.rs`
- `compiler/engine_frame/system_adapter.rs`
- `compiler/parser/grammar/func.rs`
- `compiler/parser/grammar/mod.rs`
- `compiler/hir/lower.rs`
- `compiler/mir/passes/system_access.rs`
- `compiler/tests/system_plan_validation.rs`
- `compiler/tests/system_adapter.rs`
- `compiler/tests/system_access_summary.rs`
- `examples/systems_basic/`

- [ ] Make system execution fail loudly when no production invoker exists.
  - `DefaultMirInvoker` should not return `Ok(())` for non-empty programs.
  - Reference host mocks should be named test/reference invokers and used through explicit constructors.

- [ ] Add a project-derived `CompiledSystemRuntime`.
  - Collect `FunctionRole::System`.
  - Build `SystemProgram`.
  - Attach a real MIR invocation context or return a typed unsupported-backend error.

- [ ] Fix read/write conflict ordering.
  - Do not order read/write conflicts by `SystemId`.
  - Same-phase write/write conflicts must fail validation before DAG construction.
  - Read-after-write or write-after-read should require explicit declared ordering until `@runs_before` or a richer dependency model exists.

- [ ] Parse and lower `@phase(pre_sim | sim | post_sim)`.
  - Store phase metadata in HIR.
  - Default only if the RFC allows a default; otherwise missing phase should be a diagnostic.

- [ ] Implement annotation-driven access summaries now.
  - Use `@mut` parameters as writes.
  - Plain resource parameters are reads.
  - `InputFrame` is a read.
  - `EventEmitter[T]` emits `T`.
  - Defer MIR field-level refinement, but provide the `compiler/mir/passes/system_access.rs` placeholder only if it has tests and returns conservative summaries.

- [ ] Remove redundant adapter topological sanity checks or move them into validation.
  - `SystemProgram::new` should define the invariant.
  - Adapter should trust validated `SystemProgram`.

- [ ] Rename `record_program_execution`.
  - Use `commit_program_execution_records` or equivalent.
  - Keep invocation and reporting separate.

- [ ] Add `examples/systems_basic/`.
  - Include DrainInput, IntegrateTransforms, and EmitFrameEvents equivalents.
  - Ensure it is included in smoke/docs if the project supports examples.

**Verification:**

```bash
cargo test -p wrela --test system_plan_validation
cargo test -p wrela --test system_adapter
cargo test -p wrela --test system_determinism
cargo test -p wrela --test system_access_summary
cargo run -p wrela -- check examples/systems_basic/src/main.wr
```

---

## Task 5: Fix Region Residency Policy and Reporting

**Addresses:** Expanded findings 20, 21, 22 and resource-ledger portions of finding 4.

**Files:**
- `compiler/residency/mod.rs`
- `compiler/engine_frame/residency_adapter.rs`
- `compiler/tests/residency_plan_determinism.rs`
- `compiler/tests/residency_budget_enforcement.rs`
- `compiler/tests/residency_staleness_priority.rs`
- `compiler/tests/residency_subsystem_integration.rs`

- [ ] Add a prediction horizon to residency policy.
  - Use `translation + velocity * dt`.
  - The default should be the current frame step or a small explicit prewarm horizon, not one second.
  - Apply the same helper in `RegionLine::candidates_for` and `predicted_position`.

- [ ] Fix stale eviction semantics.
  - Resident-but-incompatible regions are stale.
  - Regions outside the desired set are distance/LRU evictions, not stale due to compatibility.
  - Eviction priority is staleness first, then distance/LRU.

- [ ] Preserve budget accounting.
  - Re-admit should consume one evict slot, one admit slot, and upload bytes.
  - Deferred work must be explicit in report fields.

- [ ] Propagate GPU cache insert errors.
  - Change `apply_with_gpu_cache` to return `Result<ResidencyReport, ResidencyError>`.
  - Replace discarded `let _ = get_or_insert_with(...)` with `?`.

- [ ] Record real region IDs in `EngineResourceLedger`.
  - Add one ledger write/read per admitted or resident region needed by inspector.
  - Avoid wildcard `ResidentRegion { region_id: "*" }`.

**Verification:**

```bash
cargo test -p wrela --test residency_plan_determinism
cargo test -p wrela --test residency_budget_enforcement
cargo test -p wrela --test residency_staleness_priority
cargo test -p wrela --test residency_subsystem_integration
```

---

## Task 6: Replace Stub Physics With the RFC Collision-Backed XPBD Runtime

**Addresses:** Initial finding 6; expanded findings 23, 24, 25.

**Files:**
- `compiler/physics_contract/mod.rs`
- `compiler/physics_plan/mod.rs`
- `compiler/physics_exec/mod.rs`
- `compiler/physics_exec/xpbd.rs`
- `compiler/physics_exec/ccd.rs`
- `compiler/physics_exec/move_fsm.rs`
- `compiler/physics_exec/report.rs`
- `compiler/engine_frame/physics_adapter.rs`
- `compiler/tests/physics_xpbd_determinism.rs`
- `compiler/tests/physics_ccd_equivalence.rs`
- `compiler/tests/physics_move_fsm.rs`
- `compiler/tests/physics_body_id_stability.rs`
- `compiler/tests/physics_integration.rs`

- [ ] Split backends.
  - `PhysicsBackend::CpuOracle` is deterministic reference.
  - `PhysicsBackend::CollisionBacked` builds `CollisionWorkloadBatch`.
  - Do not silently fall back to CPU from live collision-backed mode.

- [ ] Fix adapter contract.
  - Collision-backed physics must set `requires_gpu: true`.
  - It must set `allows_hot_path_readback: true`.
  - Contact readback is budgeted and reported, not hidden.

- [ ] Implement XPBD structure.
  - Integrate velocities.
  - Predict positions.
  - Detect contacts.
  - Warm-start lambdas.
  - Iterate positional constraints.
  - Recompute velocities from position deltas.
  - Iterate velocity/friction/restitution constraints.
  - Store warm-start contact cache.

- [ ] Route contact detection through collision batches.
  - Emit `SphereOverlap`.
  - Emit `SphereSweep` / `SphereTimeOfImpact` for CCD.
  - Record `physics.broadphase`, `physics.detect_contacts`, and `physics.contact_readback`.

- [ ] Mirror body state to GPU for collision-backed mode.
  - Use `FrameUploadArena` or the existing upload abstraction.
  - Record `EngineResourceId::PhysicsBodyState { epoch }` writes.

- [ ] Add move/moveset execution.
  - Parse/lower if not already complete.
  - Validate FSM totality.
  - Advance move timers and contact-triggered transitions in physics.

- [ ] Replace linear descriptor lookup.
  - Maintain `BTreeMap` or `HashMap<PhysicsBodyId, PhysicsBodyDescriptor>`.
  - Avoid per-body `iter().find(...)` in substeps and contact resolution.

- [ ] Add closure findings.
  - `physics.substep_over_budget`
  - `physics.contact_readback_over_budget`
  - `physics.substep_clamped`
  - `physics.body_admission_full`
  - `physics.cpu_oracle_divergence`

**Verification:**

```bash
cargo test -p wrela --test physics_xpbd_determinism
cargo test -p wrela --test physics_ccd_equivalence
cargo test -p wrela --test physics_move_fsm
cargo test -p wrela --test physics_body_id_stability
cargo test -p wrela --test physics_integration
```

---

## Task 7: Complete Audio Runtime Contract

**Addresses:** Expanded findings 26, 27, 28, 29, 30, 31, 32.

**Files:**
- `compiler/audio_exec/mod.rs`
- `compiler/audio_exec/rt_check.rs`
- `compiler/audio_exec/spatial.rs`
- `compiler/audio_exec/voice_ledger.rs`
- `compiler/engine_frame/audio_adapter.rs`
- `runtime/src/audio/ring.rs`
- `runtime/src/audio/voice.rs`
- `runtime/src/audio/worker.rs`
- `compiler/tests/audio_rt_check.rs`
- `compiler/tests/audio_dsp_offline.rs`
- `compiler/tests/audio_voice_stealing_determinism.rs`
- `compiler/tests/audio_voice_ledger.rs`
- `runtime/tests/audio_worker.rs`

- [ ] Fix audio subsystem ordering.
  - `AudioSnapshotPublisher` should run after `System` and `Physics`.
  - If physics is absent, scheduler should treat the missing optional dependency according to existing dependency rules or the adapter should be configured without physics for audio-only projects.

- [ ] Add phase-continuous rendering.
  - Store per-voice oscillator phase in published/runtime voice state or audio worker state.
  - Advance phase sample-by-sample.
  - Add an offline test that verifies no discontinuity at block boundaries.

- [ ] Convert sample ring and renderer to stereo.
  - Store interleaved stereo samples or a typed stereo frame.
  - Update worker/device tests.
  - Keep mono helpers only as explicit test utilities.

- [ ] Implement Phase 68 spatialization baseline.
  - ILD head-shadow gain.
  - ITD fractional delay or a clear first increment with tests.
  - Media-driven low-pass/reverb send fields carried in `VoiceState`.

- [ ] Implement media query staggering.
  - Full-rate query top `max_full_rate_media_queries` voices.
  - Remaining voices query round-robin over following frames.
  - Do not emit `audio.media_queries_over_budget` just because voices exceed the full-rate cap.
  - Emit it only when the actual number of queries issued exceeds the cap.

- [ ] Make audio findings structured.
  - Replace or supplement `Vec<String>` with typed report fields consumed by closure rules.
  - Preserve the string bridge only temporarily for compatibility.

- [ ] Fix underrun delta accounting.
  - Prefer monotonic total counter plus `last_seen` on engine side.
  - Avoid `swap(0)` semantics that report the first underrun in the following frame.

- [ ] Add `@audio_rt` validation.
  - Create `compiler/audio_exec/rt_check.rs`.
  - Reject allocation.
  - Reject unbounded loops.
  - Reject blocking effects.
  - Reject non-`@audio_rt` callees in the call graph.
  - Reject unbounded `Result` propagation.
  - Add compile-error tests with concrete diagnostics.

**Verification:**

```bash
cargo test -p wrela --test audio_rt_check
cargo test -p wrela --test audio_dsp_offline
cargo test -p wrela --test audio_voice_stealing_determinism
cargo test -p wrela --test audio_voice_ledger
cargo test -p wrela_runtime --test audio_worker
```

---

## Task 8: Align Persistence With Stable IDs and CBOR Schema Evolution

**Addresses:** Expanded findings 33, 34, 35, 36.

**Files:**
- `compiler/persistence/header.rs`
- `compiler/persistence/snapshot.rs`
- `compiler/persistence/load_plan.rs`
- `compiler/engine_frame/save_adapter.rs`
- `compiler/tests/persistence_round_trip.rs`
- `compiler/tests/persistence_version_bump.rs`
- `compiler/tests/persistence_payload_schema.rs`
- `compiler/tests/persistence_handle_stability.rs`
- `compiler/tests/save_adapter.rs`

- [ ] Make `PersistentHandle` a stable semantic ID wrapper.
  - Use the existing `StableSemanticId` type or the canonical type returned by `stable_semantic_id`.
  - Do not hand-roll an unrelated `pub u64`.
  - Add tests across snapshot compatibility-equal transitions.

- [ ] Store ledger payloads as CBOR values.
  - Replace `payload: Vec<u8>` with `ciborium::value::Value` or a project-local CBOR value type.
  - Keep the top-level save body compressed CBOR bytes.
  - Ensure per-record schema migration can inspect named fields.

- [ ] Rename engine compatibility diagnostic.
  - `EngineVersionMismatch` carries hashes today.
  - Rename to `EngineCompatibilityHashMismatch` or include actual saved/running version fields.

- [ ] Add save/load integration tests.
  - Round trip through `SavePublisher`.
  - Version bump emits exact changed generator.
  - Archetype schema change emits `ArchetypeSchemaChanged`.
  - Handle stability test uses the real stable semantic ID path.

- [ ] Record load events in the engine resource ledger.
  - Save and load should both be visible to the Phase 70 inspector.

**Verification:**

```bash
cargo test -p wrela --test persistence_round_trip
cargo test -p wrela --test persistence_version_bump
cargo test -p wrela --test persistence_payload_schema
cargo test -p wrela --test persistence_handle_stability
cargo test -p wrela --test save_adapter
```

---

## Task 9: Strengthen Tests, Lanes, and Documentation Gates

**Addresses:** expanded test/justfile findings and the “AC complete but stubbed” pattern.

**Files:**
- `justfile`
- `AGENTS.md`
- Getting-started docs
- `examples/surface_and_input/`
- `examples/systems_basic/`
- `examples/streaming_corridor/`
- `examples/physics_playground/`
- `examples/audio_field/`
- `examples/save_and_load/`

- [ ] Keep existing just lane names, but strengthen their bodies.
  - `lint-layering`: keep current lane and ensure it is included in `just lint`.
  - `perf-latency`: run offscreen interactive reference host, not only headless synthetic reports.
  - `dev-smoke`: build/check the phase examples that should ship.
  - `ship-interactive`: honor duration env and require all subsystem spans.

- [ ] Add missing tests named by RFC 0011.
  - `physics_ccd_equivalence`
  - `physics_move_fsm`
  - `physics_body_id_stability`
  - `audio_rt_check`
  - `audio_dsp_offline`
  - `audio_voice_stealing_determinism`
  - `persistence_version_bump`
  - `persistence_payload_schema`
  - `persistence_handle_stability`
  - `system_access_summary`

- [ ] Update docs after implementation.
  - Systems authoring.
  - Streaming/residency.
  - Body/move/moveset.
  - Audio authoring.
  - Persistence.
  - End-to-end `wrela init --template=full_stack` to `wrela dev`.

- [ ] Add a completion audit test for `apps/reference_host`.
  - Require Input, System, Residency, Physics, Audio, Save, Presentation spans.
  - Require structured closure findings vector is present and empty for the happy path.
  - Require resource ledger entries for input frame, system snapshot access, resident regions, physics body/contact state, audio voice ledger, and save record.

**Verification:**

```bash
just lint-layering
just perf-latency
just dev-smoke
just ship-interactive
just ship
```

---

## Recommended PR Split

1. **Observability and closure rules:** Task 1. This should land first because it makes every later subsystem failure visible.
2. **Latency primitives:** Task 2. This restores the core RFC input-to-photon contract.
3. **Live CLI/reference host:** Task 3. This makes the lanes exercise the real host.
4. **Systems:** Task 4.
5. **Residency:** Task 5.
6. **Physics:** Task 6. This is the heaviest PR; keep it focused.
7. **Audio:** Task 7.
8. **Persistence:** Task 8.
9. **Examples/docs/final gates:** Task 9.

Every PR should run:

```bash
cargo check --workspace
just lint-layering
```

Subsystem PRs should also run their focused tests and any touched just lane.

---

## Final RFC 0011 Completion Gate

Run this before marking the RFC phases complete:

```bash
cargo check --workspace
cargo test --workspace
just lint-layering
just perf-engine-closure
just perf-latency
just dev-smoke
just ship-interactive
just ship
```

Expected final state:

- `wrela live <project>` works in headless and interactive modes.
- Interactive mode uses real elapsed host timing, real input timestamps, and GPU-completion frame pacing.
- `EngineFrameReport.closure_findings` contains all RFC 0011 rule outputs.
- New subsystem reports are typed enough that the inspector and closure gates do not parse free-form strings.
- Reference host spans and resource ledger entries cross-reference every live subsystem.
- Physics, audio, persistence, residency, and systems are no longer advertised as complete while running stub paths.
