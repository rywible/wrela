# RFC 0011 Seven-Finding Remediation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Resolve the seven review findings against the in-progress RFC 0011 interactive runtime changes.

**Architecture:** Fix the live substrate first: one monotonic input clock, true GPU-frame pacing, and an interactive `wrela live` path that exercises the reference host. Then make reporting honest by adding closure rules for every new subsystem. Finally replace the largest semantic shims with real contracts: authored system execution, physics through the collision/GPU contract, and a lock-free producer/consumer input ring.

**Tech Stack:** Rust 2024 workspace, `wgpu`, `winit`, `rtrb`, existing `EngineFrameScheduler`, `EngineFrameReport`, `LiveEngineHost`, `apps/reference_host`, `just`, and `cargo test`.

---

## File Map

- Modify `compiler/engine_frame/latency.rs`: add a sampler-side monotonic deadline hook so late sampling can use the same clock as raw input events.
- Modify `compiler/engine_frame/runtime.rs`: drain late input with the sampler-provided deadline and store that exact sample timestamp in `EngineFrameContext`.
- Modify `compiler/engine_frame/scheduler.rs`: derive motion-to-photon stages from the actual late-sample timestamp, not synthetic fixed-step wall time.
- Modify `compiler/engine_frame/input_ring_bridge.rs`: wire the reference-host input sampler to a real monotonic clock source.
- Modify `apps/reference_host/src/lib.rs`: pass the same `started_at`-relative clock to raw event creation and late sampling; release frame pacing from GPU completion.
- Modify `runtime/src/platform/frame_pacing.rs`: add a nonblocking queue-completion release path.
- Modify `compiler/bin/wrela/cli_args.rs` and `compiler/bin/wrela/commands/live_command.rs`: allow interactive `wrela live` and launch the reference host path instead of rejecting it.
- Modify `justfile`: make `perf-latency` exercise the interactive/reference-host latency lane, not only synthetic headless `wrela live`.
- Modify `compiler/engine_frame/closure_rules.rs`: register rules for Input, System, Residency, Physics, Audio, Save, and missing Presentation latency/input findings.
- Modify `compiler/bin/wrela/perf_engine/closure.rs`: aggregate frame/report violations into the closure status so the rule table can convert them to findings.
- Modify `compiler/system_exec/mod.rs`, `compiler/system_plan/mod.rs`, `compiler/engine_frame/system_adapter.rs`: replace mock-only system execution with project-derived program execution and a real compiled-system invoker boundary.
- Modify `compiler/physics_exec/mod.rs`, `compiler/engine_frame/physics_adapter.rs`: add a collision-backed physics backend and GPU-resident/reporting hooks.
- Replace `runtime/src/platform/input_pump.rs` API: split lock-free producer and consumer handles instead of wrapping both `rtrb` halves in `Mutex`.
- Update tests in `compiler/tests/live_host.rs`, `runtime/tests/frame_in_flight_semaphore.rs`, `compiler/tests/closure_rules_engine_frame.rs`, `compiler/tests/system_adapter.rs`, `compiler/tests/physics_adapter.rs`, `runtime/tests/winit_input_pump_ring.rs`, and `apps/reference_host/tests/smoke.rs`.

---

## Task 1: Fix Late-Input Clock Domains

**Finding:** 1

**Files:**
- Modify: `compiler/engine_frame/latency.rs`
- Modify: `compiler/engine_frame/runtime.rs`
- Modify: `compiler/engine_frame/scheduler.rs`
- Modify: `compiler/engine_frame/input_ring_bridge.rs`
- Modify: `apps/reference_host/src/lib.rs`
- Test: `compiler/tests/live_host.rs`

- [ ] **Step 1: Add the failing regression test**

Add a test that creates a late sampler whose event timestamp and sample deadline share the same absolute origin, then asserts stage 1 uses that actual age:

```rust
#[test]
fn late_input_uses_sampler_deadline_not_synthetic_tick_wall_time() {
    let sampler = Arc::new(ClockedScriptedLateSampler::new(
        WallClockStamp::new(10_000_000),
        vec![TickInputEvent::with_timestamps(
            SimulationTick::new(1),
            TickInputKind::Event,
            "keyboard",
            "key.w.down",
            WallClockStamp::new(4_000_000),
            4_000_000,
        )],
    ));
    let mut host = live_host_with_sampler(sampler);
    let output = host.advance(1.0 / 60.0).expect("frame").outputs.remove(0);
    assert_eq!(
        output.report.latency.event_arrival_to_state_advance_nanos,
        6_000_000
    );
}
```

Run:

```bash
cargo test -p wrela --test live_host late_input_uses_sampler_deadline_not_synthetic_tick_wall_time -- --nocapture
```

Expected before implementation: the test fails because `StateAdvanceRuntimeAdapter` uses `current_clock.wall_clock` as the deadline.

- [ ] **Step 2: Extend `LateInputSampler` with a deadline hook**

In `compiler/engine_frame/latency.rs`, add:

```rust
pub trait LateInputSampler: Send + Sync {
    fn drain_up_to(&self, deadline: WallClockStamp) -> TickInputBatch;

    fn sample_deadline(&self, fallback: WallClockStamp) -> WallClockStamp {
        fallback
    }

    fn ring_state(&self) -> InputRingState {
        InputRingState::default()
    }
}
```

- [ ] **Step 3: Use the sampler deadline at the point of materialization**

In `compiler/engine_frame/runtime.rs`, replace the captured `wall_deadline` use inside `StateAdvanceRuntimeAdapter::build` with:

```rust
let fallback_deadline = current_clock.wall_clock;
let inputs = match &tick_source {
    TickInputSource::Eager(_) => {
        tick_source.materialize_for_simulation_tick(tick_for_batch, fallback_deadline)
    }
    TickInputSource::Late(sampler) => {
        let deadline = sampler.sample_deadline(fallback_deadline);
        let mut batch = sampler.drain_up_to(deadline);
        batch.tick = tick_for_batch;
        for event in &mut batch.inputs {
            event.tick = tick_for_batch;
        }
        state_advance_input_sample_nanos_slot.store(deadline.get(), Ordering::Release);
        batch
    }
};
```

Use an `Arc<AtomicU64>` or `Arc<Mutex<Option<u64>>>` captured by the report builder so `ctx.state_advance_input_sample_nanos` receives the exact deadline used by the job.

- [ ] **Step 4: Wire the reference-host clock into the sampler**

In `compiler/engine_frame/input_ring_bridge.rs`, change `RawInputRingLateSampler` to accept a clock callback:

```rust
pub struct RawInputRingLateSampler {
    ring: Arc<RawInputRing>,
    scratch: Mutex<Vec<TimestampedRawEvent>>,
    now_nanos: Arc<dyn Fn() -> u64 + Send + Sync>,
}

impl LateInputSampler for RawInputRingLateSampler {
    fn sample_deadline(&self, _fallback: WallClockStamp) -> WallClockStamp {
        WallClockStamp::new((self.now_nanos)())
    }
}
```

In `apps/reference_host/src/lib.rs`, pass the same `started_at.elapsed().as_nanos()` origin used by `push_raw_event`.

- [ ] **Step 5: Verify**

Run:

```bash
cargo test -p wrela --test live_host late_input_uses_sampler_deadline_not_synthetic_tick_wall_time
cargo test -p wrela --test live_host live_engine_host_late_sampler_materializes_timestamped_events
```

Expected: both pass, and no `latency.input_timestamp_after_sample` appears for normal reference-host input.

---

## Task 2: Release Frame Pacing From GPU Completion

**Finding:** 2

**Files:**
- Modify: `runtime/src/platform/frame_pacing.rs`
- Modify: `apps/reference_host/src/lib.rs`
- Test: `runtime/tests/frame_in_flight_semaphore.rs`
- Test: `apps/reference_host/tests/smoke.rs`

- [ ] **Step 1: Add a semaphore test that proves release can be deferred**

Add a test that acquires the semaphore, starts a second acquire on another thread, verifies it blocks, then releases from a simulated completion callback:

```rust
#[test]
fn frame_pacing_second_frame_waits_for_completion_release() {
    let semaphore = Arc::new(FrameInFlightSemaphore::new(1));
    semaphore.acquire();
    let entered = Arc::new(AtomicBool::new(false));
    let worker = {
        let semaphore = Arc::clone(&semaphore);
        let entered = Arc::clone(&entered);
        std::thread::spawn(move || {
            semaphore.acquire();
            entered.store(true, Ordering::Release);
            semaphore.release();
        })
    };
    std::thread::sleep(Duration::from_millis(20));
    assert!(!entered.load(Ordering::Acquire));
    semaphore.release();
    worker.join().expect("worker");
    assert!(entered.load(Ordering::Acquire));
}
```

- [ ] **Step 2: Add a queue-completion helper**

In `runtime/src/platform/frame_pacing.rs`, add:

```rust
impl FrameInFlightSemaphore {
    pub fn release_after_submitted_work_done(self: &Arc<Self>, queue: &wgpu::Queue) {
        let semaphore = Arc::clone(self);
        queue.on_submitted_work_done(move || {
            semaphore.release();
        });
    }
}
```

- [ ] **Step 3: Move release out of `ReferenceHostApp::tick`**

In `apps/reference_host/src/lib.rs`, remove the unconditional `self.frame_pacing.release()` after `host.advance`. Instead:

- Release immediately only if the frame produces no surface-backed GPU submission.
- For the surface-backed path, call `frame_pacing.release_after_submitted_work_done(&native.queue)` immediately after `PresentationFramegraph::submit_segment(false)` succeeds.
- On any error before scheduling the callback, release immediately before exiting the event loop.

- [ ] **Step 4: Assert the smoke test sees the pacing note**

Add a note to the presentation report after scheduling the queue callback:

```rust
metrics.notes.push("frame_pacing_release=queue_work_done".to_string());
```

Update `apps/reference_host/tests/smoke.rs` to require this note when a surface-backed offscreen smoke path is enabled.

- [ ] **Step 5: Verify**

Run:

```bash
cargo test -p wrela_runtime --test frame_in_flight_semaphore
WRELA_TEST_OFFSCREEN=1 cargo test -p wrela_reference_host --test smoke
```

---

## Task 3: Unblock Interactive `wrela live` And Make `perf-latency` Exercise It

**Finding:** 3

**Files:**
- Modify: `compiler/bin/wrela/cli_args.rs`
- Modify: `compiler/bin/wrela/commands/live_command.rs`
- Modify: `apps/reference_host/src/main.rs`
- Modify: `apps/reference_host/src/lib.rs`
- Modify: `justfile`
- Test: `compiler/tests/cli/diagnostics.rs` or `compiler/bin/wrela/cli_args.rs` tests

- [ ] **Step 1: Add CLI parser coverage**

Add/adjust tests so these parse:

```text
wrela live examples/surface_and_input/src/main.wr --headless --frames 2 --json
wrela live examples/surface_and_input/src/main.wr --frames 2
```

Expected before implementation: the second form errors with `wrela live requires --headless`.

- [ ] **Step 2: Remove the parser rejection**

In `compiler/bin/wrela/cli_args.rs`, delete the non-headless rejection at the `CommandName::Live` arm. Keep `--frames > 0` validation.

- [ ] **Step 3: Route interactive mode to the reference host binary**

Because the `wrela` compiler package cannot depend on `apps/reference_host` without a Cargo cycle, launch the workspace binary from `live_command.rs`:

```rust
if !args.options.headless {
    let mut command = std::process::Command::new("cargo");
    command.args(["run", "-p", "wrela_reference_host", "--"]);
    command.env("WRELA_REFERENCE_HOST_PROJECT", entry_path.as_os_str());
    if args.options.frames > 0 {
        command.env("WRELA_REFERENCE_HOST_FRAMES", args.options.frames.to_string());
    }
    let status = command.status().map_err(|err| format!("launch reference host: {err}"))?;
    std::process::exit(status.code().unwrap_or(EXIT_USAGE));
}
```

Keep the existing headless JSON path unchanged.

- [ ] **Step 4: Add reference-host CLI env parsing**

In `apps/reference_host/src/main.rs`, read:

```rust
let frames = std::env::var("WRELA_REFERENCE_HOST_FRAMES")
    .ok()
    .and_then(|value| value.parse::<u32>().ok());
```

Pass it into `ReferenceHostConfig { frames, ..Default::default() }`.

- [ ] **Step 5: Update `just perf-latency`**

Change the lane from headless-only `wrela live` to:

```make
perf-latency:
    just live-smoke
    WRELA_TEST_OFFSCREEN=1 WRELA_REFERENCE_HOST_FRAMES=120 cargo run -p wrela --release -- live examples/surface_and_input/src/main.wr --frames=120
```

- [ ] **Step 6: Verify**

Run:

```bash
cargo test -p wrela --bin wrela parse_live_headless_is_typed_after_parsing
cargo run -p wrela -- live examples/surface_and_input/src/main.wr --headless --frames=2 --json > /tmp/wrela-live.jsonl
WRELA_TEST_OFFSCREEN=1 cargo run -p wrela -- live examples/surface_and_input/src/main.wr --frames=2
```

Expected: headless emits JSON reports; interactive launches/exits the reference host path.

---

## Task 4: Add Closure Rules For All RFC 0011 Subsystems

**Finding:** 4

**Files:**
- Modify: `compiler/engine_frame/closure_rules.rs`
- Modify: `compiler/bin/wrela/perf_engine/closure.rs`
- Modify: `compiler/perf_target/mod.rs`
- Test: `compiler/tests/closure_rules_engine_frame.rs`

- [ ] **Step 1: Add failing rule-table coverage**

In `compiler/tests/closure_rules_engine_frame.rs`, assert canonical registration covers all new kinds:

```rust
#[test]
fn canonical_rules_cover_rfc0011_subsystems() {
    let table = ClosureRuleTable::with_canonical_engine_frame_rules();
    let covered = table.registered_subsystems();
    for kind in [
        EngineSubsystemKind::Input,
        EngineSubsystemKind::System,
        EngineSubsystemKind::Residency,
        EngineSubsystemKind::Physics,
        EngineSubsystemKind::Audio,
        EngineSubsystemKind::Save,
        EngineSubsystemKind::Presentation,
    ] {
        assert!(covered.contains(&kind), "missing {kind:?}");
    }
}
```

- [ ] **Step 2: Add violation prefix rules**

In `compiler/engine_frame/closure_rules.rs`, add a reusable rule:

```rust
struct ViolationPrefixRule {
    kind: EngineSubsystemKind,
    subsystem: &'static str,
    prefixes: &'static [&'static str],
}
```

Its `collect` scans `report.violations` and emits one `PerfClosureFinding` per matching prefix with:

```rust
focus: matched_prefix.trim_end_matches('.').to_string()
summary: format!("{subsystem} reported an RFC 0011 closure violation")
evidence: vec![format!("violation={violation}")]
next_step: "inspect the subsystem EngineFrameReport row and fix the reported runtime contract breach".to_string()
```

- [ ] **Step 3: Register required prefixes**

Add these registrations:

```rust
table.register(Box::new(ViolationPrefixRule::new(
    EngineSubsystemKind::Presentation,
    "presentation",
    &[
        "presentation.fallback_to_vsync_fifo",
        "presentation.input_ring_overflow",
        "presentation.motion_to_photon_over_budget",
        "presentation.motion_to_photon_perf_lane_over_budget",
        "presentation.framerate_below_target",
    ],
)));
table.register(Box::new(ViolationPrefixRule::new(EngineSubsystemKind::Input, "input", &["input."])));
table.register(Box::new(ViolationPrefixRule::new(EngineSubsystemKind::System, "system", &["system."])));
table.register(Box::new(ViolationPrefixRule::new(EngineSubsystemKind::Residency, "residency", &["residency."])));
table.register(Box::new(ViolationPrefixRule::new(
    EngineSubsystemKind::Physics,
    "physics",
    &[
        "physics.substep_over_budget",
        "physics.contact_readback_over_budget",
        "physics.substep_clamped",
        "physics.body_admission_full",
        "physics.cpu_oracle_divergence",
    ],
)));
table.register(Box::new(ViolationPrefixRule::new(
    EngineSubsystemKind::Audio,
    "audio",
    &[
        "audio.underrun",
        "audio.media_queries_over_budget",
        "audio.voice_count_over_cap",
        "audio.publish_latency",
    ],
)));
table.register(Box::new(ViolationPrefixRule::new(EngineSubsystemKind::Save, "save", &["save."])));
```

- [ ] **Step 4: Aggregate frame violations into closure status**

In `compiler/bin/wrela/perf_engine/closure.rs`, when building `PerfClosureEngineFrameStatusReport`, include every sampled `EngineFrameBenchmarkReport` violation in `report.violations`. If the benchmark report type does not currently carry the raw `EngineFrameReport.violations`, add a `violations: Vec<String>` field in the conversion path.

- [ ] **Step 5: Verify**

Run:

```bash
cargo test -p wrela --test closure_rules_engine_frame
just test-engine-frame
```

---

## Task 5: Wire Systems To Authored Runtime Execution

**Finding:** 5

**Files:**
- Modify: `compiler/system_exec/mod.rs`
- Modify: `compiler/system_plan/mod.rs`
- Modify: `compiler/engine_frame/system_adapter.rs`
- Modify: `compiler/hir/project.rs`
- Test: `compiler/tests/system_adapter.rs`
- Test: `compiler/tests/system_determinism.rs`

- [ ] **Step 1: Add a production-constructor failure test**

Add a test proving `SystemSubsystemAdapter::new` cannot silently create an adapter with an unusable default invoker for a non-empty authored program:

```rust
#[test]
fn production_system_adapter_requires_compiled_invoker() {
    let program = one_system_program();
    let input_frame = Arc::new(Mutex::new(Some(sample_input())));
    let err = SystemSubsystemAdapter::new(program, input_frame).expect_err("must reject");
    assert!(err.to_string().contains("compiled system invoker required"));
}
```

- [ ] **Step 2: Split constructors**

Change `SystemSubsystemAdapter::new` to return `Result<Self, EngineFrameError>` and reject non-empty programs unless supplied a real invoker. Keep tests explicit by renaming the fake path:

```rust
pub fn with_invoker_for_tests(
    program: SystemProgram,
    input_frame: Arc<Mutex<Option<InputFrame>>>,
    invoker: Arc<dyn SystemMirInvoker>,
) -> Self
```

Use `with_invoker` for production and keep the default invoker only for empty programs.

- [ ] **Step 3: Add a project-derived bundle boundary**

In `compiler/system_exec/mod.rs`, add:

```rust
pub struct CompiledSystemRuntime {
    pub program: SystemProgram,
    pub invoker: Arc<dyn SystemMirInvoker>,
}
```

In `compiler/hir/project.rs`, add:

```rust
pub fn compiled_system_runtime(project: &LoadedProject) -> Result<CompiledSystemRuntime, SystemExecError>
```

The first implementation should:

- collect `FunctionRole::System` functions,
- build `SystemPlan` entries using the lowered `@phase` and `@mut` access metadata,
- fail with `SystemExecError::Invoke("compiled MIR system invoker not installed")` if no MIR execution backend is available.

This makes unsupported execution explicit instead of pretending the systems ran.

- [ ] **Step 4: Update reference host**

The reference host may continue using a fake invoker, but it must call `with_invoker_for_tests` or `with_invoker` with an explicitly named `ReferenceSystemInvoker`, so production and smoke shims are visually distinct.

- [ ] **Step 5: Verify**

Run:

```bash
cargo test -p wrela --test system_adapter
cargo test -p wrela --test system_determinism
cargo check --workspace
```

---

## Task 6: Route Physics Through Collision/GPU Contract

**Finding:** 6

**Files:**
- Modify: `compiler/physics_exec/mod.rs`
- Modify: `compiler/physics_plan/mod.rs`
- Modify: `compiler/engine_frame/physics_adapter.rs`
- Test: `compiler/tests/physics_adapter.rs`
- Test: `compiler/tests/physics_xpbd_determinism.rs`

- [ ] **Step 1: Add a failing adapter contract test**

Add a test requiring the physics adapter report to show GPU/collision-contract intent when configured for the live backend:

```rust
#[test]
fn physics_live_backend_requires_gpu_and_reports_contact_readback_budget() {
    let solver = PhysicsSolver::new(PhysicsPlan::collision_backed(vec![body()]), vec![state()]);
    let adapter = PhysicsSubsystemAdapter::new(solver, 1.0 / 60.0);
    let descriptor = adapter.debug_descriptor_for_tests();
    assert!(descriptor.requires_gpu);
    assert!(descriptor.allows_hot_path_readback);
}
```

Expected before implementation: it fails because `requires_gpu` and `allows_hot_path_readback` are false.

- [ ] **Step 2: Add backend selection**

In `compiler/physics_plan/mod.rs`, add:

```rust
pub enum PhysicsBackend {
    CpuOracle,
    CollisionBacked,
}

impl PhysicsPlan {
    pub fn collision_backed(bodies: Vec<PhysicsBodyDescriptor>) -> Self {
        Self { backend: PhysicsBackend::CollisionBacked, bodies, ..Self::cpu(bodies) }
    }
}
```

- [ ] **Step 3: Add collision-backed execution boundary**

In `compiler/physics_exec/mod.rs`, split `step`:

```rust
pub fn step(&mut self, dt: f32) -> Result<PhysicsFrameReport, PhysicsError> {
    match self.plan.backend {
        PhysicsBackend::CpuOracle => self.step_cpu_oracle(dt),
        PhysicsBackend::CollisionBacked => self.step_collision_backed(dt),
    }
}
```

`step_collision_backed` should build a `CollisionWorkloadBatch` with `SphereOverlap` and `SphereSweep` items for the bodies, call the existing collision execution boundary, and record:

```rust
report.readback_bytes = contact_result_bytes;
report.findings.push("physics.contact_readback_over_budget".to_string()) // only when over budget
```

If the collision backend cannot be constructed yet for a fixture, return `PhysicsError::CollisionBackendUnavailable` rather than silently falling back to CPU.

- [ ] **Step 4: Update adapter descriptor**

In `compiler/engine_frame/physics_adapter.rs`, set:

```rust
requires_gpu: solver.backend() == PhysicsBackend::CollisionBacked,
allows_hot_path_readback: solver.backend() == PhysicsBackend::CollisionBacked,
```

Add report notes:

```rust
"backend=collision_backed"
"contact_readback_bytes={}"
```

- [ ] **Step 5: Verify**

Run:

```bash
cargo test -p wrela --test physics_adapter
cargo test -p wrela --test physics_xpbd_determinism
```

---

## Task 7: Replace Lock-Backed Input Ring With Split Lock-Free Handles

**Finding:** 7

**Files:**
- Modify: `runtime/src/platform/input_pump.rs`
- Modify: `compiler/engine_frame/input_ring_bridge.rs`
- Modify: `apps/reference_host/src/lib.rs`
- Test: `runtime/tests/winit_input_pump_ring.rs`
- Test: `compiler/tests/live_host.rs`

- [ ] **Step 1: Add a source-level lock regression test**

Add a runtime test that fails if `RawInputRingProducer` or `RawInputRingConsumer` contains `Mutex` in its type name/debug layout is not possible directly, so test the public API instead: there should be no `RawInputRing::push_event(&self, ...)` shared object API left.

Use compile-time assertions:

```rust
fn assert_send<T: Send>() {}
fn assert_not_sync<T: Send>() {}

#[test]
fn raw_input_ring_uses_split_spsc_handles() {
    assert_send::<RawInputProducer>();
    assert_send::<RawInputConsumer>();
    let (mut producer, mut consumer) = RawInputRing::split(4096);
    producer.push_event(evt("kbd", 1));
    let mut out = Vec::new();
    assert_eq!(consumer.drain_up_to_nanos(1, &mut out), 1);
}
```

- [ ] **Step 2: Replace shared ring with split handles**

In `runtime/src/platform/input_pump.rs`, define:

```rust
pub struct RawInputRing;

pub struct RawInputProducer {
    producer: Producer<TimestampedRawEvent>,
    telemetry: Arc<RawInputRingTelemetry>,
}

pub struct RawInputConsumer {
    consumer: Consumer<TimestampedRawEvent>,
    telemetry: Arc<RawInputRingTelemetry>,
}

pub struct RawInputRingTelemetry {
    dropped_events: AtomicU32,
    out_of_order_events: AtomicU32,
    last_monotonic: AtomicU64,
    overflow_latch: AtomicBool,
    approx_depth: AtomicU32,
}
```

Add:

```rust
impl RawInputRing {
    pub fn split(capacity: usize) -> (RawInputProducer, RawInputConsumer) {
        let (producer, consumer) = RingBuffer::new(capacity.max(1));
        let telemetry = Arc::new(RawInputRingTelemetry::default());
        (
            RawInputProducer { producer, telemetry: Arc::clone(&telemetry) },
            RawInputConsumer { consumer, telemetry },
        )
    }
}
```

Move `push_event` onto `&mut RawInputProducer` and `drain_up_to_nanos` onto `&mut RawInputConsumer`.

- [ ] **Step 3: Update `RawInputRingLateSampler`**

In `compiler/engine_frame/input_ring_bridge.rs`, replace `Arc<RawInputRing>` with:

```rust
consumer: Mutex<RawInputConsumer>
```

The mutex here is only inside the compiler-side sampler wrapper to satisfy `LateInputSampler: Sync`; the runtime/platform ring itself must not expose a lock-backed producer/consumer. The state-advance path should be the only consumer, so this lock is uncontended and outside the runtime public API. If strict no-lock late sampling is required, replace the trait object with a single-thread-owned sampler in a follow-up.

- [ ] **Step 4: Update reference host ownership**

In `apps/reference_host/src/lib.rs`:

```rust
let (input_producer, input_consumer) = RawInputRing::split(4096);
```

Store `input_producer` in `ReferenceHostApp` and pass `input_consumer` into `RawInputRingLateSampler::new`.

Change `push_raw_event` to take `&mut self` and call:

```rust
self.input_producer.push_event(TimestampedRawEvent::new(...));
```

- [ ] **Step 5: Verify**

Run:

```bash
cargo test -p wrela_runtime --test winit_input_pump_ring
cargo test -p wrela --test live_host live_engine_host_late_sampler_materializes_timestamped_events
cargo check --workspace
```

---

## Final Verification

Run the full review remediation gate:

```bash
cargo check --workspace
cargo test -p wrela --test live_host
cargo test -p wrela --test closure_rules_engine_frame
cargo test -p wrela --test system_adapter
cargo test -p wrela --test physics_adapter
cargo test -p wrela_runtime --test frame_in_flight_semaphore
cargo test -p wrela_runtime --test winit_input_pump_ring
WRELA_TEST_OFFSCREEN=1 cargo test -p wrela_reference_host --test smoke
just lint-layering
```

Then run the expensive lanes before declaring the RFC phase complete:

```bash
just perf-latency
just perf-engine-closure
just ship
```

## Recommended PR Split

1. Latency substrate and frame pacing: Tasks 1 and 2.
2. Interactive CLI and latency lane: Task 3.
3. Closure rule coverage: Task 4.
4. Systems execution boundary: Task 5.
5. Physics collision/GPU backend: Task 6.
6. Lock-free input ring: Task 7.

Each PR should include the focused tests from its task plus `cargo check --workspace`.
