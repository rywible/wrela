# RFC 0011 Review Remediation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Resolve all ten audit findings against the RFC 0011 interactive runtime plan, restoring red gates first and then replacing placeholder subsystem wiring with tested, RFC-aligned behavior.

**Architecture:** Fix the build and lint gates before touching subsystem semantics. Then address runtime correctness at the substrate boundaries: absolute latency clocks, deterministic system planning/execution, authored declaration parsing/lowering, real-time audio safety, current-epoch persistence, and reference-host presentation wiring. The broad Phase 70 "full interactive host" gap is split into incremental slices so each lands with a concrete smoke test.

**Tech Stack:** Rust 2024 workspace, `just`, `cargo test`, `wgpu`, `winit`, `cpal`, `rtrb`, `arc-swap`, `ciborium`, `zstd`.

---

## File Map

- Modify `compiler/tests/live_host.rs`: add missing `closure_findings` fields in manual `EngineFrameReport` literals.
- Modify `compiler/tests/engine_frame.rs`: add missing `closure_findings` fields in manual `EngineFrameReport` literals.
- Modify `justfile`: make `lint-layering` ignore comments while still failing on real runtime crate compiler imports.
- Modify `compiler/engine_frame/scheduler.rs`: compute motion-to-photon latency from one absolute monotonic origin instead of mixing frame-relative span times with platform monotonic input times.
- Modify `compiler/engine_frame/runtime.rs`: pass `EngineFrameInput.current_clock.wall_clock` into scheduler latency derivation as the absolute frame deadline/origin used by the late input sampler.
- Modify `compiler/tests/live_host.rs`: add a regression test proving real input arrival before frame start contributes to stage 1 latency.
- Modify `compiler/system_plan/mod.rs`: replace symmetric read/write ordering with stable one-way ordering and keep aliasing writers rejected.
- Modify `compiler/tests/system_plan_validation.rs`: add reader/writer same-resource regression.
- Modify `compiler/system_exec/mod.rs`: make the default system invoker execute registered compiled bodies or fail loudly when no compiled invoker is installed.
- Modify `compiler/engine_frame/system_adapter.rs`: require a real invoker for production construction; keep explicit test constructor for fake invokers.
- Modify `compiler/tests/system_adapter.rs`: assert the production constructor rejects missing invoker or that registered invoker is called.
- Modify `compiler/parser/grammar/mod.rs`, `compiler/parser/kind.rs`, `compiler/parser/mod.rs`: add explicit `body`, `audio field`, `voice`, `media field`, and `input_map action` parser coverage.
- Modify `compiler/hir/def.rs`, `compiler/hir/lower.rs`: lower the new declarations into real role/metadata records rather than generic syntax nodes.
- Add or modify parser/HIR tests under `compiler/tests/` and `compiler/parser/mod.rs`.
- Modify `runtime/src/audio/ring.rs`: remove `Mutex` from audio callback path by exposing direct SPSC halves or a callback-owned consumer.
- Modify `runtime/src/audio/device.rs`: preallocate sample conversion buffers before stream construction; no resize inside callback.
- Modify `runtime/src/audio/worker.rs`: consume from the lock-free callback reader.
- Modify `runtime/tests/audio_headless.rs` or add a focused runtime audio test: assert the callback path does not take the old `SampleRing` mutex path.
- Modify `compiler/engine_frame/save_adapter.rs`: save the state-advance output snapshot/current epoch and schedule after `Presentation`.
- Modify `compiler/tests/save_adapter.rs`: assert saved header epoch equals output snapshot epoch.
- Modify `compiler/presentation_exec/framegraph.rs`: ensure swapchain acquire/present are surfaced as presentation spans.
- Modify `apps/reference_host/src/lib.rs`: stop direct clear-present as the main present path; wire host surface into the presentation framegraph/runtime path.
- Modify `apps/reference_host/tests/smoke.rs`: require presentation swapchain spans and non-noop subsystem state in the inspector smoke.

---

## Task 1: Restore Red Build And Lint Gates

**Findings:** 1, 2

**Files:**
- Modify: `compiler/tests/live_host.rs`
- Modify: `compiler/tests/engine_frame.rs`
- Modify: `justfile`

- [ ] **Step 1: Write/confirm failing gate evidence**

Run:

```bash
just test-engine-frame
just lint-layering
```

Expected before the fix:

```text
error[E0063]: missing field `closure_findings` in initializer of `EngineFrameReport`
lint-layering: forbidden compiler reference in runtime/src
```

- [ ] **Step 2: Update manual report literals**

Add this field to every manual `EngineFrameReport { ... }` literal that does not already include it:

```rust
closure_findings: Vec::new(),
```

Known locations from the audit:

```text
compiler/tests/live_host.rs:130
compiler/tests/engine_frame.rs:493
```

After editing, verify there are no remaining report literals missing the field:

```bash
rg -n "EngineFrameReport \\{" compiler/tests compiler/bin/wrela/perf_engine/tests.rs
```

- [ ] **Step 3: Make `lint-layering` ignore comment-only matches**

In `justfile`, replace each direct `if rg -n "$pat" "$path" --glob '*.rs' 2>/dev/null; then` block with this pattern:

```bash
matches="$(rg -n "$pat" "$path" --glob '*.rs' 2>/dev/null \
  | rg -v '^[^:]+:[0-9]+:[[:space:]]*(//|//!|///|/\*|\*)' || true)"
if [[ -n "$matches" ]]; then
  printf '%s\n' "$matches"
  echo "lint-layering: forbidden compiler reference in $path (pattern: $pat)" >&2
  fail=1
fi
```

This preserves real import/reference detection while ignoring doc comments such as `/// Mirrors wrela::...`.

- [ ] **Step 4: Verify gates pass**

Run:

```bash
just lint-layering
just live-smoke
just test-engine-frame
```

Expected:

```text
lint-layering: ok
test result: ok
```

- [ ] **Step 5: Commit**

```bash
git add compiler/tests/live_host.rs compiler/tests/engine_frame.rs justfile
git commit -m "test: restore RFC 0011 engine-frame gates -Codex Automated"
```

---

## Task 2: Fix Motion-To-Photon Clock Domain Mixing

**Finding:** 5

**Files:**
- Modify: `compiler/engine_frame/scheduler.rs`
- Modify: `compiler/tests/live_host.rs`

- [ ] **Step 1: Add a failing latency regression test**

Add a test in `compiler/tests/live_host.rs` that constructs a late input event with an absolute monotonic timestamp before the scheduler frame starts, runs one frame, and asserts stage 1 includes that pre-frame wait instead of collapsing to zero.

Use this shape:

```rust
#[test]
fn late_input_latency_uses_absolute_monotonic_origin() {
    let sampler = Arc::new(ScriptedLateSampler::new(vec![TickInputEvent::with_timestamps(
        SimulationTick::new(1),
        TickInputKind::Event,
        "keyboard",
        "key.w",
        WallClockStamp::new(1_000_000),
        1_000_000,
    )]));
    let mut host = live_host_with_sampler(sampler);
    host.wall_nanos = 20_000_000;
    let out = host.advance(1.0 / 60.0).expect("frame").outputs.remove(0);
    assert!(
        out.report.latency.event_arrival_to_state_advance_nanos >= 1_000_000,
        "stage1 should include absolute event age, got {:?}",
        out.report.latency
    );
}
```

Adapt helper names to the existing test helpers in `compiler/tests/live_host.rs`.

- [ ] **Step 2: Run the failing test**

Run:

```bash
cargo test -p wrela --test live_host late_input_latency_uses_absolute_monotonic_origin -- --nocapture
```

Expected before the fix:

```text
FAILED
stage1 should include absolute event age
```

- [ ] **Step 3: Use the frame input wall clock as the shared monotonic origin**

The live host already passes `EngineFrameInput.current_clock.wall_clock` as the late-sampler drain deadline. Use that same value as the absolute frame deadline for motion-to-photon math; do not use `Instant::elapsed()` inside the scheduler, because it is frame-relative and unrelated to platform input timestamps.

In `compiler/engine_frame/scheduler.rs`, change the latency builder signature:

```rust
fn motion_to_photon_contract_from_timeline(
    timeline: &EngineFrameTimeline,
    frame_wall_time_micros: u128,
    earliest_input_arrival_nanos: Option<u64>,
    frame_deadline_nanos: u64,
) -> MotionToPhotonContract
```

In the scheduler report assembly, pass `input.current_clock.wall_clock.get()` through from `EngineFrameRuntime::run_frame_with_persistent_subsystems`. If the scheduler entrypoint currently lacks access to `EngineFrameInput`, add a `run_frame_borrowed_with_latency_origin(...)` overload that takes:

```rust
pub struct EngineFrameLatencyOrigin {
    pub frame_deadline_nanos: u64,
}
```

Then call it from runtime with:

```rust
EngineFrameLatencyOrigin {
    frame_deadline_nanos: input.current_clock.wall_clock.get(),
}
```

- [ ] **Step 4: Compute span absolute nanoseconds from that origin**

Derive the frame start from the shared frame deadline minus measured scheduler wall time:

```rust
let frame_duration_nanos = frame_wall_time_micros
    .saturating_mul(1000)
    .min(u64::MAX as u128) as u64;
let frame_start_nanos = frame_deadline_nanos.saturating_sub(frame_duration_nanos);
let abs_ns = |micros: u128| {
    frame_start_nanos.saturating_add(
        micros.saturating_mul(1000).min(u64::MAX as u128) as u64,
    )
};
```

Then replace `state_start_nanos`, `state_end_nanos`, `render_submit_nanos`, `gpu_end_nanos`, `present_start_nanos`, and `present_end_nanos` conversions with `abs_ns(...)` rather than frame-relative `mu_to_ns(...)`.

- [ ] **Step 5: Verify latency tests**

Run:

```bash
cargo test -p wrela --test live_host late_input_latency_uses_absolute_monotonic_origin motion_to_photon_over_budget_appends_violation
just live-smoke
```

Expected:

```text
test result: ok
```

- [ ] **Step 6: Commit**

```bash
git add compiler/engine_frame/scheduler.rs compiler/tests/live_host.rs
git commit -m "fix: use one monotonic clock for live latency -Codex Automated"
```

---

## Task 3: Repair System Plan Ordering And Production Execution

**Findings:** 6, 7

**Files:**
- Modify: `compiler/system_plan/mod.rs`
- Modify: `compiler/system_exec/mod.rs`
- Modify: `compiler/engine_frame/system_adapter.rs`
- Modify: `compiler/tests/system_plan_validation.rs`
- Modify: `compiler/tests/system_adapter.rs`

- [ ] **Step 1: Add reader/writer scheduling regression**

In `compiler/tests/system_plan_validation.rs`, add:

```rust
#[test]
fn system_plan_orders_reader_writer_without_cycle() {
    let read_player = SystemAccessSummary::default()
        .reads(SystemResourceId::Resource("player".into()));
    let write_player = SystemAccessSummary::default()
        .writes(SystemResourceId::Resource("player".into()));
    let reader = SystemPlan::new(
        SystemId::new("ReadPlayer"),
        SystemContractId::new("read"),
        SystemPhase::Sim,
        read_player,
        1,
    );
    let writer = SystemPlan::new(
        SystemId::new("WritePlayer"),
        SystemContractId::new("write"),
        SystemPhase::Sim,
        write_player,
        2,
    );

    let program = SystemProgram::new([reader, writer]).expect("reader/writer should schedule");
    let ids = program
        .phase(SystemPhase::Sim)
        .iter()
        .map(|plan| plan.id.0.as_str())
        .collect::<Vec<_>>();
    assert_eq!(ids, vec!["ReadPlayer", "WritePlayer"]);
}
```

- [ ] **Step 2: Run the failing system plan test**

Run:

```bash
cargo test -p wrela --test system_plan_validation system_plan_orders_reader_writer_without_cycle
```

Expected before fix:

```text
FAILED
system schedule cycle
```

- [ ] **Step 3: Make read/write ordering one-way and stable**

Replace `must_run_before` in `compiler/system_plan/mod.rs` with:

```rust
fn must_run_before(left: &SystemPlan, right: &SystemPlan) -> bool {
    let w_l_r_r = !left.access.writes.is_disjoint(&right.access.reads);
    let r_l_w_r = !left.access.reads.is_disjoint(&right.access.writes);
    let w_l_w_r = !left.access.writes.is_disjoint(&right.access.writes);
    if w_l_w_r {
        return left.id.0 < right.id.0;
    }
    if w_l_r_r || r_l_w_r {
        return left.id.0 < right.id.0;
    }
    false
}
```

Make the same ordering change in `compiler/engine_frame/system_adapter.rs::intra_phase_must_run_before`.

- [ ] **Step 4: Add production invoker regression**

In `compiler/tests/system_adapter.rs`, add a test that constructs a `SystemSubsystemAdapter` through the production constructor and asserts compiled-body invocation cannot silently no-op. Choose one of these expected behaviors and encode it:

```rust
#[test]
fn system_adapter_requires_real_invoker_for_production_constructor() {
    let input_frame = Arc::new(Mutex::new(Some(empty_input_frame())));
    let program = one_system_program();
    let err = SystemSubsystemAdapter::new(program, input_frame)
        .try_build_for_production()
        .expect_err("production systems require a real invoker");
    assert!(err.to_string().contains("system MIR invoker is not configured"));
}
```

If the codebase prefers constructor-level enforcement, replace `try_build_for_production()` with a fallible constructor:

```rust
SystemSubsystemAdapter::new(program, input_frame).expect_err(...)
```

- [ ] **Step 5: Replace default no-op invoker with loud failure**

In `compiler/system_exec/mod.rs`, replace `DefaultMirInvoker::invoke` with:

```rust
fn invoke(&self, mir_function_id: u32, _input: &InputFrame) -> Result<(), String> {
    Err(format!(
        "system MIR invoker is not configured for function {mir_function_id}"
    ))
}
```

Keep `SystemSubsystemAdapter::with_invoker(...)` as the test/runtime path for supplied invokers.

- [ ] **Step 6: Verify systems**

Run:

```bash
cargo test -p wrela --test system_plan_validation --test system_adapter --test system_determinism
```

Expected:

```text
test result: ok
```

- [ ] **Step 7: Commit**

```bash
git add compiler/system_plan/mod.rs compiler/system_exec/mod.rs compiler/engine_frame/system_adapter.rs compiler/tests/system_plan_validation.rs compiler/tests/system_adapter.rs
git commit -m "fix: make system planning and execution honest -Codex Automated"
```

---

## Task 4: Replace Generic RFC Declaration Blocks With Real Parser/HIR Surface

**Finding:** 8

**Files:**
- Modify: `compiler/parser/grammar/mod.rs`
- Modify: `compiler/parser/kind.rs`
- Modify: `compiler/parser/mod.rs`
- Modify: `compiler/hir/def.rs`
- Modify: `compiler/hir/lower.rs`
- Test: `compiler/tests/rfc0011_author_surface.rs`

- [ ] **Step 1: Add parser tests for the RFC syntax**

Create `compiler/tests/rfc0011_author_surface.rs` with tests for:

```rust
#[test]
fn parses_input_map_actions() {
    let source = r#"
input_map PlayerInputMap {
    action MoveForward = key.w | gamepad.left_stick_y < -0.2
    action Strike = mouse.left_button | gamepad.right_trigger > 0.5
}
"#;
    let (node, errors) = wrela::parser::parse_with_errors(source);
    assert!(errors.is_empty(), "{errors:?}");
    assert!(node.descendants().any(|n| n.kind() == SyntaxKind::InputMapDef));
    assert!(node.descendants().any(|n| n.kind() == SyntaxKind::InputMapAction));
}

#[test]
fn parses_physics_and_audio_declarations() {
    let source = r#"
body Player {
    class: dynamic
    mass: 70.0
    support: sphere(radius=0.42)
}

audio field Pulse(freq: F32, gate: Bool, t: F32) -> F32 {
    @audio_rt
    return if gate { sin(freq * t) } else { 0.0 }
}

voice AlarmVoice(source: Pulse, position: Vec3, gain: F32) {
    priority: 10
}

media field CaveMedium(path: MediaPath) -> MediaSample {
    return MediaSample(occlusion_db=0.0, reverb_send=0.2, lowpass_hz=18000.0)
}
"#;
    let (_node, errors) = wrela::parser::parse_with_errors(source);
    assert!(errors.is_empty(), "{errors:?}");
}
```

Adapt imports to the existing parser test style.

- [ ] **Step 2: Run failing parser tests**

Run:

```bash
cargo test -p wrela --test rfc0011_author_surface
```

Expected before fix:

```text
FAILED
```

- [ ] **Step 3: Add explicit syntax kinds**

In `compiler/parser/kind.rs`, add:

```rust
BodyDef,
InputMapAction,
AudioFieldDef,
VoiceDef,
MediaFieldDef,
MoveStateDef,
```

Keep existing `InputMapDef`, `MoveDef`, and `MovesetDef`.

- [ ] **Step 4: Replace generic dispatch with explicit grammar**

In `compiler/parser/grammar/mod.rs`:

```rust
if p.at_ident_text("body") {
    runtime_named_block_def(p, "body", SyntaxKind::BodyDef);
    return;
}
if p.at_ident_text("input_map") {
    input_map_def(p);
    return;
}
if p.at_ident_text("audio") {
    audio_field_def(p);
    return;
}
if p.at_ident_text("voice") {
    runtime_named_block_def(p, "voice", SyntaxKind::VoiceDef);
    return;
}
if p.at_ident_text("media") {
    media_field_def(p);
    return;
}
```

Implement `input_map_def`, `audio_field_def`, and `media_field_def` in the same file or a new focused grammar file following local parser style. `input_map_def` must parse `action Name = expr` entries as `SyntaxKind::InputMapAction`.

- [ ] **Step 5: Add HIR roles and metadata**

In `compiler/hir/def.rs`, add function/declaration roles:

```rust
InputMap,
Body,
Move,
Moveset,
AudioField,
Voice,
MediaField,
```

Add metadata structs with the minimal fields needed by existing adapters:

```rust
pub struct InputMapMetadata {
    pub bindings: Vec<crate::input_contract::InputMapBinding>,
}

pub struct PhysicsBodyMetadata {
    pub class_name: SmolStr,
    pub stable_id: u64,
}

pub struct AudioFieldMetadata {
    pub audio_rt: bool,
}
```

- [ ] **Step 6: Lower parsed declarations**

In `compiler/hir/lower.rs`, extend root lowering so:

```rust
SyntaxKind::InputMapDef => self.lower_input_map_decl(node),
SyntaxKind::BodyDef => self.lower_body_decl(node),
SyntaxKind::AudioFieldDef => self.lower_audio_field_decl(node),
SyntaxKind::VoiceDef => self.lower_voice_decl(node),
SyntaxKind::MediaFieldDef => self.lower_media_field_decl(node),
```

Each lowerer must create a HIR entry with the correct role and metadata. Do not execute subsystem planning from HIR yet; this task only makes the authored surface real and queryable.

- [ ] **Step 7: Verify parser/HIR and existing smoke**

Run:

```bash
cargo test -p wrela --test rfc0011_author_surface
cargo test -p wrela parser::runtime_foundation_declarations_parse_as_root_items
just dev-smoke
```

Expected:

```text
test result: ok
```

- [ ] **Step 8: Commit**

```bash
git add compiler/parser/grammar/mod.rs compiler/parser/kind.rs compiler/parser/mod.rs compiler/hir/def.rs compiler/hir/lower.rs compiler/tests/rfc0011_author_surface.rs
git commit -m "feat: parse and lower RFC 0011 author surface -Codex Automated"
```

---

## Task 5: Make Audio Callback Path Real-Time Safe

**Finding:** 9

**Files:**
- Modify: `runtime/src/audio/ring.rs`
- Modify: `runtime/src/audio/device.rs`
- Modify: `runtime/src/audio/worker.rs`
- Test: `runtime/tests/audio_headless.rs`

- [ ] **Step 1: Add a regression test for callback-owned consumer**

In `runtime/tests/audio_headless.rs`, add:

```rust
#[test]
fn audio_callback_uses_dedicated_consumer_without_mutex() {
    let (producer, mut consumer) = wrela_runtime::audio::ring::SampleRing::split(16);
    assert_eq!(producer.push_block(&[0.25, 0.5, 0.75, 1.0]), 4);
    let mut out = [0.0; 4];
    let underruns = std::sync::atomic::AtomicU64::new(0);
    wrela_runtime::audio::worker::fill_output_from_consumer_atomic(
        &mut out,
        &mut consumer,
        &underruns,
    );
    assert_eq!(out, [0.25, 0.5, 0.75, 1.0]);
    assert_eq!(underruns.load(std::sync::atomic::Ordering::Relaxed), 0);
}
```

- [ ] **Step 2: Run failing audio test**

Run:

```bash
cargo test -p wrela_runtime --test audio_headless audio_callback_uses_dedicated_consumer_without_mutex
```

Expected before fix:

```text
FAILED
no function or associated item named `split`
```

- [ ] **Step 3: Split producer and consumer types**

In `runtime/src/audio/ring.rs`, introduce:

```rust
pub struct SampleProducer {
    producer: rtrb::Producer<f32>,
}

pub struct SampleConsumer {
    consumer: rtrb::Consumer<f32>,
}

impl SampleRing {
    pub fn split(capacity: usize) -> (SampleProducer, SampleConsumer) {
        let (producer, consumer) = rtrb::RingBuffer::new(capacity.max(1));
        (SampleProducer { producer }, SampleConsumer { consumer })
    }
}
```

Add lock-free block methods:

```rust
impl SampleProducer {
    pub fn push_block(&mut self, block: &[f32]) -> usize {
        let mut pushed = 0;
        for sample in block {
            if self.producer.push(*sample).is_ok() {
                pushed += 1;
            } else {
                break;
            }
        }
        pushed
    }
}

impl SampleConsumer {
    pub fn pop_block(&mut self, output: &mut [f32]) -> usize {
        let mut popped = 0;
        for slot in output {
            match self.consumer.pop() {
                Ok(sample) => {
                    *slot = sample;
                    popped += 1;
                }
                Err(_) => break,
            }
        }
        popped
    }
}
```

Keep the old `SampleRing` mutex wrapper only for non-real-time tests/helpers, and do not call it from CPAL callbacks.

- [ ] **Step 4: Update worker callback helper**

In `runtime/src/audio/worker.rs`, add:

```rust
pub fn fill_output_from_consumer_atomic(
    output: &mut [f32],
    consumer: &mut SampleConsumer,
    underruns: &AtomicU64,
) {
    let popped = consumer.pop_block(output);
    if popped < output.len() {
        for slot in output.iter_mut().skip(popped) {
            *slot = 0.0;
        }
        underruns.fetch_add(1, Ordering::Relaxed);
    }
}
```

- [ ] **Step 5: Preallocate conversion scratch outside callback**

In `runtime/src/audio/device.rs`, construct `SampleProducer`/`SampleConsumer` before `build_output_stream`. For I16/U16, allocate scratch with exact length before closure creation:

```rust
let scratch_len = (config.block_size as usize).saturating_mul(channels as usize);
let mut scratch = vec![0.0_f32; scratch_len];
```

Inside the callback, never call `resize`; if `data.len() > scratch.len()`, fill silence and increment underrun once:

```rust
if data.len() > scratch.len() {
    data.fill(0);
    underruns.fetch_add(1, Ordering::Relaxed);
    return;
}
```

- [ ] **Step 6: Verify runtime audio**

Run:

```bash
cargo test -p wrela_runtime --test audio_headless
cargo test -p wrela --test audio_adapter --test audio_voice_ledger
```

Expected:

```text
test result: ok
```

- [ ] **Step 7: Commit**

```bash
git add runtime/src/audio/ring.rs runtime/src/audio/worker.rs runtime/src/audio/device.rs runtime/tests/audio_headless.rs
git commit -m "fix: remove locks from audio callback path -Codex Automated"
```

---

## Task 6: Save The Current Epoch And Order Save After Presentation

**Finding:** 10

**Files:**
- Modify: `compiler/engine_frame/save_adapter.rs`
- Modify: `compiler/tests/save_adapter.rs`

- [ ] **Step 1: Add failing current-epoch save test**

In `compiler/tests/save_adapter.rs`, add:

```rust
#[test]
fn save_publisher_saves_state_advance_output_epoch() {
    let (mut runtime, snapshot, project) = save_test_runtime();
    let publisher = SavePublisher::new(true, snapshot.clone(), project, 0, 0, Vec::new());
    let record_slot = publisher.record();
    let output = runtime
        .run_frame_with_subsystems(live_save_input(snapshot), vec![Box::new(publisher)])
        .expect("frame");
    let record = record_slot.lock().expect("record").clone().expect("save record");
    assert_eq!(record.header.snapshot_epoch, output.snapshot.epoch().0);
}
```

Adapt helper names to existing `save_adapter.rs` test helpers.

- [ ] **Step 2: Run failing save test**

Run:

```bash
cargo test -p wrela --test save_adapter save_publisher_saves_state_advance_output_epoch
```

Expected before fix:

```text
FAILED
left: previous epoch
right: output epoch
```

- [ ] **Step 3: Bind save to state-advance output**

Change `SaveAdapterFrameState` to hold an optional state outcome slot or output snapshot. Prefer the existing runtime state outcome mechanism:

```rust
pub enum SaveWorldBinding {
    Fixed(WorldSnapshotHandle),
    StateOutcome(Arc<Mutex<Option<Result<StateAdvanceResult, EngineFrameError>>>>),
}
```

Add a constructor:

```rust
pub fn with_state_outcome(
    request: bool,
    project: PersistenceProject,
    outcome: Arc<Mutex<Option<Result<StateAdvanceResult, EngineFrameError>>>>,
    ledger: Vec<SnapshotLedgerRecord>,
) -> Self
```

In the save job, resolve:

```rust
let snapshot = match &world_binding {
    SaveWorldBinding::Fixed(snapshot) => snapshot.clone(),
    SaveWorldBinding::StateOutcome(slot) => {
        let guard = slot.lock().map_err(|_| EngineFrameError::Message("state outcome lock poisoned".into()))?;
        let Some(Ok(outcome)) = guard.as_ref() else {
            return Err(EngineFrameError::Message("save requested before successful state advance".into()));
        };
        outcome.transition_record.to_snapshot.clone()
    }
};
```

- [ ] **Step 4: Schedule save after presentation when presentation exists**

Change the descriptor:

```rust
runs_after: vec![EngineSubsystemKind::StateAdvance, EngineSubsystemKind::Presentation],
```

If the scheduler currently requires all `runs_after` kinds to exist, implement optional dependency support or split constructors:

```rust
pub fn after_presentation(...) -> Self
pub fn after_state_advance_for_headless_tests(...) -> Self
```

Use `after_presentation` in live/reference-host wiring.

- [ ] **Step 5: Verify save**

Run:

```bash
cargo test -p wrela --test save_adapter --test persistence_round_trip
```

Expected:

```text
test result: ok
```

- [ ] **Step 6: Commit**

```bash
git add compiler/engine_frame/save_adapter.rs compiler/tests/save_adapter.rs
git commit -m "fix: save current engine-frame snapshot -Codex Automated"
```

---

## Task 7: Fold Swapchain Present Into Presentation Reporting

**Finding:** 4

**Files:**
- Modify: `compiler/presentation_exec/framegraph.rs`
- Modify: `compiler/engine_frame/scheduler.rs` if span labels require scheduler support
- Modify: `apps/reference_host/src/lib.rs`
- Modify: `compiler/tests/swapchain_attachment.rs` or create it if absent
- Modify: `apps/reference_host/tests/smoke.rs`

- [ ] **Step 1: Add swapchain span test**

Create or update `compiler/tests/swapchain_attachment.rs`:

```rust
#[test]
fn swapchain_present_path_emits_acquire_and_present_spans() {
    let graph = build_stub_swapchain_framegraph();
    let report = run_graph_through_presentation_adapter(graph).expect("frame");
    let labels = report
        .timeline_spans()
        .iter()
        .map(|span| span.label.as_str())
        .collect::<Vec<_>>();
    assert!(labels.contains(&"presentation.swapchain_acquire"));
    assert!(labels.contains(&"presentation.swapchain_present"));
    assert!(report.readback_ledger.tickets.is_empty());
}
```

Use the local report/span accessors that exist in `EngineFrameReport`.

- [ ] **Step 2: Run failing swapchain test**

Run:

```bash
cargo test -p wrela --test swapchain_attachment
```

Expected before fix:

```text
FAILED
missing presentation.swapchain_acquire
```

- [ ] **Step 3: Represent acquire/present as presentation jobs or spans**

In `compiler/presentation_exec/framegraph.rs`, move acquire/present boundaries into observable presentation work. If framegraph internals cannot emit scheduler spans directly, add notes to the `EngineSubsystemReport` and expose labels through presentation adapter jobs:

```rust
builder.add_job(
    EngineSubsystemKind::Presentation,
    "presentation.swapchain_acquire",
    EngineJobAffinity::Gpu,
    EngineSpanDomain::PresentWait,
    deps,
    false,
    move || { swapchain.acquire().map(|_| ()).map_err(to_engine_error) },
);
```

Then render uses the acquired texture, and terminal job:

```rust
builder.add_job(
    EngineSubsystemKind::Presentation,
    "presentation.swapchain_present",
    EngineJobAffinity::Gpu,
    EngineSpanDomain::PresentWait,
    vec![render_job],
    false,
    move || acquired.present().map_err(to_engine_error),
);
```

- [ ] **Step 4: Remove direct clear-present from reference host hot path**

In `apps/reference_host/src/lib.rs`, replace `surface.present_clear_frame()` in `ReferenceHostApp::tick` with runtime/presentation swapchain wiring. Keep `present_clear_frame` only as an error fallback during startup, not as the normal frame path.

- [ ] **Step 5: Verify swapchain and reference host smoke**

Run:

```bash
cargo test -p wrela --test swapchain_attachment
cargo test -p wrela_reference_host --test smoke
```

Expected:

```text
test result: ok
```

- [ ] **Step 6: Commit**

```bash
git add compiler/presentation_exec/framegraph.rs compiler/tests/swapchain_attachment.rs apps/reference_host/src/lib.rs apps/reference_host/tests/smoke.rs
git commit -m "fix: report swapchain acquire and present in presentation -Codex Automated"
```

---

## Task 8: Replace Reference Host No-Op Spine With Real Runtime Wiring

**Finding:** 3

**Files:**
- Modify: `apps/reference_host/src/lib.rs`
- Modify: `apps/reference_host/src/inspector/*.rs`
- Modify: `apps/reference_host/tests/smoke.rs`
- Modify: `compiler/bin/wrela/commands/live_command.rs`
- Modify: `examples/surface_and_input/src/main.wr` or add `examples/full_stack/`

- [ ] **Step 1: Strengthen smoke test so no-op host fails**

In `apps/reference_host/tests/smoke.rs`, require more than state advance:

```rust
#[test]
fn reference_host_smoke_runs_interactive_subsystems() {
    let reports = wrela_reference_host::run_headless_smoke(8).expect("smoke");
    let last = reports.last().expect("report");
    let kinds = last.subsystems.iter().map(|s| &s.kind).collect::<Vec<_>>();
    assert!(kinds.contains(&&EngineSubsystemKind::Input));
    assert!(kinds.contains(&&EngineSubsystemKind::System));
    assert!(kinds.contains(&&EngineSubsystemKind::Presentation));
    assert!(
        last.subsystems.iter().any(|s| s.label != "state_advance" && s.work_items > 0),
        "reference host must expose live subsystem state, got {:?}",
        last.subsystems
    );
}
```

- [ ] **Step 2: Run failing reference host smoke**

Run:

```bash
cargo test -p wrela_reference_host --test smoke reference_host_smoke_runs_interactive_subsystems
```

Expected before fix:

```text
FAILED
missing Input/System/Presentation
```

- [ ] **Step 3: Build host adapters from project/runtime state**

In `apps/reference_host/src/lib.rs`, replace `new_headless_host()` and `new_input_driven_host()` internals so they register the available adapters:

```rust
let input_adapter = InputSubsystemAdapter::new(input_map_plan, runtime.materialized_tick_input_slot());
let input_frame = input_adapter.shared_frame();
host.add_subsystem(Box::new(input_adapter));
host.add_subsystem(Box::new(SystemSubsystemAdapter::with_invoker(system_program, input_frame, compiled_invoker)));
host.add_subsystem(Box::new(ResidencySubsystemAdapter::with_state_outcome(...)));
host.add_subsystem(Box::new(PhysicsSubsystemAdapter::new(...)));
host.add_subsystem(Box::new(AudioSnapshotPublisher::from_shared_state(...)));
host.add_subsystem(Box::new(SavePublisher::with_state_outcome(...)));
```

If a subsystem cannot be built from the example yet, use a minimal real plan with non-zero work and a comment naming the remaining RFC gap. Do not use `ReferenceNoopExecutor` as the only behavior.

- [ ] **Step 4: Make CLI `wrela live` share the same host builder**

In `compiler/bin/wrela/commands/live_command.rs`, replace local `CliNoopStateAdvanceExecutor` host setup with a shared builder function from the reference host or compiler live module. The CLI should load the project and wire the same input/system/presentation adapter set as headless smoke.

- [ ] **Step 5: Verify host**

Run:

```bash
cargo test -p wrela_reference_host --test smoke
cargo run -p wrela -- live examples/surface_and_input/src/main.wr --headless --frames=8 --json > /tmp/wrela-live.jsonl
```

Expected:

```text
test result: ok
```

And:

```bash
rg '"kind":"input"|"kind":"system"|"kind":"presentation"' /tmp/wrela-live.jsonl
```

Expected: at least one match for each required subsystem.

- [ ] **Step 6: Commit**

```bash
git add apps/reference_host/src/lib.rs apps/reference_host/src/inspector apps/reference_host/tests/smoke.rs compiler/bin/wrela/commands/live_command.rs examples
git commit -m "feat: wire reference host to live subsystems -Codex Automated"
```

---

## Task 9: Reconcile Perf-Latency Lane With Real Latency Measurement

**Related Findings:** 4, 5

**Files:**
- Modify: `justfile`
- Modify: `compiler/bin/wrela/commands/live_command.rs`
- Add or modify: `compiler/bin/wrela/perf_latency/mod.rs` if choosing the RFC's dedicated command

- [ ] **Step 1: Decide lane shape**

Use one of these two paths:

1. RFC-exact: add `wrela perf-latency <project>` and have `just perf-latency` call it.
2. Minimal incremental: keep `wrela live --enforce-latency-budget`, but require real swapchain/presentation spans and real late input injection.

For RFC fidelity, choose path 1.

- [ ] **Step 2: Add a failing CLI test**

Add a CLI test asserting:

```bash
wrela perf-latency examples/surface_and_input
```

prints p50/p95/p99 and exits non-zero when synthetic p99 exceeds the configured target.

- [ ] **Step 3: Implement or route the command**

If adding the command, wire:

```rust
ParsedCommand::PerfLatency(PerfLatencyCommandArgs { path_arg, frames, output_format })
```

Then implement collection by injecting timestamped input into the same ring used by `LiveEngineHost`.

- [ ] **Step 4: Verify perf-latency**

Run:

```bash
just perf-latency
```

Expected:

```text
0 latency findings
```

- [ ] **Step 5: Commit**

```bash
git add justfile compiler/bin/wrela/cli_args.rs compiler/bin/wrela/commands compiler/bin/wrela/perf_latency
git commit -m "feat: add RFC 0011 perf latency lane -Codex Automated"
```

---

## Task 10: Full Gate Sweep

**Findings:** all

**Files:**
- No planned source edits unless a gate exposes a regression.

- [ ] **Step 1: Format**

Run:

```bash
just fmt
just fmt-check
```

Expected:

```text
cargo fmt --all --check exits 0
```

- [ ] **Step 2: Fast compile/test lanes**

Run:

```bash
cargo check --workspace
just lint-layering
just live-smoke
just test-engine-frame
```

Expected:

```text
Finished
lint-layering: ok
test result: ok
```

- [ ] **Step 3: Focused subsystem lanes**

Run:

```bash
cargo test -p wrela --test input_subsystem --test system_adapter --test system_determinism --test system_plan_validation
cargo test -p wrela --test residency_service --test residency_subsystem_integration
cargo test -p wrela --test physics_adapter --test physics_xpbd_determinism
cargo test -p wrela --test audio_adapter --test audio_voice_ledger
cargo test -p wrela --test persistence_round_trip --test save_adapter
cargo test -p wrela_reference_host --test smoke
```

Expected:

```text
test result: ok
```

- [ ] **Step 4: RFC lanes**

Run:

```bash
just dev-smoke
just perf-latency
just ship-interactive
```

Expected:

```text
all commands exit 0
```

- [ ] **Step 5: Commit final verification notes**

If verification required source fixes, commit them:

```bash
git add .
git commit -m "chore: verify RFC 0011 remediation gates -Codex Automated"
```

---

## Self-Review

- Findings 1 and 2 are covered by Task 1.
- Finding 5 is covered by Task 2.
- Findings 6 and 7 are covered by Task 3.
- Finding 8 is covered by Task 4.
- Finding 9 is covered by Task 5.
- Finding 10 is covered by Task 6.
- Finding 4 is covered by Task 7 and validated again in Task 9.
- Finding 3 is covered by Task 8.
- Final end-to-end verification is covered by Task 10.
- No task relies on knowingly leaving a no-op path in place as accepted runtime behavior.
