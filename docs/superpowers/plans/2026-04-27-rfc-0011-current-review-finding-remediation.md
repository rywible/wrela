# RFC 0011 Current Review Finding Remediation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Resolve the eight current RFC 0011 audit findings so systems, physics, audio, persistence, and late input match the interactive-runtime acceptance criteria.

**Architecture:** Fix behavior at the ownership boundary for each subsystem: systems record actual runtime emissions, frame dt comes from `EngineFrameInput` clocks, physics reports CPU solver work honestly while preserving GPU-readback policy, collision-backed physics combines collision-exec world contacts with CPU body-body broadphase, audio mixer and media queries enforce runtime contracts, persistence preserves snapshot identity, and the late-input bridge removes blocking locks from the hot path. Each fix starts with a failing regression test and lands with focused verification before the next subsystem.

**Tech Stack:** Rust, Cargo integration tests, Wrela compiler/runtime crates, engine-frame scheduler/adapters, `rtrb` SPSC ring, `wgpu`-style report telemetry.

---

## Scope Check

These findings span five independent subsystems. Keep each task independently shippable and commit after each green task. Do not refactor unrelated RFC 0011 scaffolding while executing this plan.

## File Structure

- Modify `compiler/system_exec/mod.rs`: route actual event emissions, add explicit dt entrypoints, remove declared-emitter-as-emitted behavior.
- Modify `compiler/engine_frame/system_adapter.rs`: pass frame-derived dt into system invocations and record actual per-system emissions without duplicating.
- Modify `compiler/engine_frame/runtime.rs`: add an `EngineFrameInput::frame_dt_seconds()` helper derived from `previous_clock` and `current_clock`.
- Modify `compiler/engine_frame/physics_adapter.rs`: update dt during `prepare_frame`, classify XPBD solver span as CPU, and report GPU timing only when actual GPU timing exists.
- Modify `compiler/physics_exec/mod.rs`: split world contacts from body-body contacts and include body-body contacts in collision-backed physics.
- Modify `runtime/src/audio/voice.rs`: enforce `gate` at the mixer boundary.
- Modify `compiler/audio_exec/mod.rs`: query top-priority voices every frame.
- Modify `compiler/persistence/load_plan.rs`: load from saved `snapshot_epoch`, never from `sim_tick`.
- Modify `compiler/engine_frame/input_ring_bridge.rs`: replace `Mutex` hot-path storage with single-consumer lock-free interior mutability and an atomic reentry guard.
- Modify tests:
  - `compiler/tests/system_determinism.rs`
  - `compiler/tests/system_adapter.rs`
  - `compiler/tests/physics_adapter.rs`
  - `compiler/tests/physics_xpbd_determinism.rs`
  - `compiler/tests/audio_voice_ledger.rs`
  - `runtime/tests/audio_headless.rs`
  - `compiler/tests/persistence_round_trip.rs`
  - `compiler/tests/input_subsystem.rs`

## Task 1: Systems Publish Only Actual Runtime Events

**Files:**
- Modify: `compiler/system_exec/mod.rs`
- Modify: `compiler/engine_frame/system_adapter.rs`
- Test: `compiler/tests/system_determinism.rs`
- Test: `compiler/tests/system_adapter.rs`

- [ ] **Step 1: Write failing executor tests for actual emissions**

Add this invoker and two tests to `compiler/tests/system_determinism.rs`:

```rust
struct ConditionalEmitterInvoker {
    emit: bool,
}

impl SystemMirInvoker for ConditionalEmitterInvoker {
    fn invoke(
        &self,
        mir_function_id: u32,
        ctx: &mut SystemInvocationContext<'_>,
    ) -> Result<(), String> {
        if self.emit && mir_function_id == 2 {
            ctx.emitted_events.push(EventTypeId::new("FrameSummary"));
        }
        Ok(())
    }
}

#[test]
fn declared_event_emitter_does_not_publish_without_send() {
    let program = sample_program();
    let input = sample_input();
    let mut executor = SystemExecutor::new(Arc::new(ConditionalEmitterInvoker { emit: false }));

    let first = executor
        .run_program_with_dt(&program, &input, 1.0 / 120.0)
        .expect("first tick");
    assert!(first.records.iter().all(|record| record.emitted_events.is_empty()));

    let second = executor
        .run_program_with_dt(&program, &input, 1.0 / 120.0)
        .expect("second tick");
    assert!(second.records.iter().all(|record| record.visible_events.is_empty()));
}

#[test]
fn actual_event_emission_is_visible_once_next_tick() {
    let program = sample_program();
    let input = sample_input();
    let mut executor = SystemExecutor::new(Arc::new(ConditionalEmitterInvoker { emit: true }));

    let first = executor
        .run_program_with_dt(&program, &input, 1.0 / 120.0)
        .expect("first tick");
    let emitted_count = first
        .records
        .iter()
        .flat_map(|record| record.emitted_events.iter())
        .filter(|event| event.0 == "FrameSummary")
        .count();
    assert_eq!(emitted_count, 1);

    let second = executor
        .run_program_with_dt(&program, &input, 1.0 / 120.0)
        .expect("second tick");
    let visible_count = second
        .records
        .iter()
        .flat_map(|record| record.visible_events.iter())
        .filter(|event| event.0 == "FrameSummary")
        .count();
    assert_eq!(visible_count, 1);
}
```

- [ ] **Step 2: Run the executor tests and verify they fail**

Run:

```bash
cargo test -p wrela --test system_determinism -- declared_event_emitter_does_not_publish_without_send actual_event_emission_is_visible_once_next_tick --nocapture
```

Expected: compile failure because `run_program_with_dt` does not exist, or assertion failure because declared emitters are committed as actual events.

- [ ] **Step 3: Add actual-emission storage to `SystemExecutor`**

In `compiler/system_exec/mod.rs`, add a pending emission map:

```rust
pub struct SystemExecutor {
    report: SystemExecutionReport,
    visible_events: Vec<EventTypeId>,
    next_tick_events: Vec<EventTypeId>,
    pending_emitted_events: BTreeMap<SystemId, Vec<EventTypeId>>,
    resources: Arc<Mutex<SystemResourceStore>>,
    invoker: Arc<dyn SystemMirInvoker>,
}
```

Update `SystemExecutor::new` and `begin_tick`:

```rust
pub fn new(invoker: Arc<dyn SystemMirInvoker>) -> Self {
    Self {
        report: SystemExecutionReport::default(),
        visible_events: Vec::new(),
        next_tick_events: Vec::new(),
        pending_emitted_events: BTreeMap::new(),
        resources: Arc::new(Mutex::new(SystemResourceStore::default())),
        invoker,
    }
}

pub fn begin_tick(&mut self) {
    self.visible_events = std::mem::take(&mut self.next_tick_events);
    self.pending_emitted_events.clear();
    self.report.records.clear();
}
```

- [ ] **Step 4: Replace declared-emission recording with actual-emission recording**

Replace `invoke_system_body`, `record_system_execution`, `enqueue_emitted_events`, `run_system`, `commit_program_execution_records`, and `run_program` in `compiler/system_exec/mod.rs` with explicit dt variants:

```rust
pub fn invoke_system_body_with_dt(
    &mut self,
    plan: &SystemPlan,
    input: &InputFrame,
    dt_seconds: f64,
) -> Result<Vec<EventTypeId>, SystemExecError> {
    let mut emitted_events = Vec::new();
    let mut ctx = SystemInvocationContext {
        input,
        resources: Arc::clone(&self.resources),
        emitted_events: &mut emitted_events,
        dt_seconds,
        snapshot_epoch: input.epoch,
        snapshot: None,
    };
    self.invoker
        .invoke(plan.mir_function_id, &mut ctx)
        .map_err(SystemExecError::Invoke)?;
    Ok(emitted_events)
}

pub fn record_system_execution(
    &mut self,
    plan: &SystemPlan,
    input: &InputFrame,
    emitted_events: Vec<EventTypeId>,
) -> SystemExecutionRecord {
    self.next_tick_events.extend(emitted_events.iter().cloned());
    let record = SystemExecutionRecord {
        system: plan.id.clone(),
        observed_input_actions: input.actions.len(),
        visible_events: self.visible_events.clone(),
        emitted_events,
    };
    self.report.records.push(record.clone());
    record
}

pub fn enqueue_system_emitted_events(
    &mut self,
    system: SystemId,
    emitted_events: Vec<EventTypeId>,
) {
    self.next_tick_events.extend(emitted_events.iter().cloned());
    self.pending_emitted_events
        .entry(system)
        .or_default()
        .extend(emitted_events);
}

pub fn run_system_with_dt(
    &mut self,
    plan: &SystemPlan,
    input: &InputFrame,
    dt_seconds: f64,
) -> Result<SystemExecutionRecord, SystemExecError> {
    let emitted_events = self.invoke_system_body_with_dt(plan, input, dt_seconds)?;
    Ok(self.record_system_execution(plan, input, emitted_events))
}

pub fn commit_program_execution_records(
    &mut self,
    program: &SystemProgram,
    input: &InputFrame,
) -> SystemExecutionReport {
    for phase in &program.phases {
        for plan in phase {
            let emitted_events = self
                .pending_emitted_events
                .remove(&plan.id)
                .unwrap_or_default();
            let record = SystemExecutionRecord {
                system: plan.id.clone(),
                observed_input_actions: input.actions.len(),
                visible_events: self.visible_events.clone(),
                emitted_events,
            };
            self.report.records.push(record);
        }
    }
    self.report.clone()
}

pub fn run_program_with_dt(
    &mut self,
    program: &SystemProgram,
    input: &InputFrame,
    dt_seconds: f64,
) -> Result<SystemExecutionReport, SystemExecError> {
    self.begin_tick();
    for phase in &program.phases {
        for plan in phase {
            self.run_system_with_dt(plan, input, dt_seconds)?;
        }
    }
    Ok(self.report.clone())
}
```

- [ ] **Step 5: Update old test call sites to pass dt explicitly**

In `compiler/tests/system_determinism.rs`, replace existing `.run_program(&program, &input)` calls with:

```rust
.run_program_with_dt(&program, &input, 1.0 / 120.0)
```

Use `1.0 / 120.0` so the tests do not encode the old 60 Hz fallback.

- [ ] **Step 6: Update the engine-frame system adapter emission path**

In `compiler/engine_frame/system_adapter.rs`, replace the `enqueue_emitted_events(emitted_events)` call with:

```rust
.enqueue_system_emitted_events(plan.id.clone(), emitted_events);
```

Keep the join job calling `commit_program_execution_records`; after Step 4 it records actual emissions in deterministic program order without re-enqueueing them.

- [ ] **Step 7: Run system tests**

Run:

```bash
cargo test -p wrela --test system_determinism -- --nocapture
cargo test -p wrela --test system_adapter -- --nocapture
```

Expected: all tests pass, including the two new emission tests.

- [ ] **Step 8: Commit**

```bash
git add compiler/system_exec/mod.rs compiler/engine_frame/system_adapter.rs compiler/tests/system_determinism.rs compiler/tests/system_adapter.rs
git commit -m "fix: route actual system event emissions -Codex Automated"
```

## Task 2: Derive System and Physics dt From Frame Clocks

**Files:**
- Modify: `compiler/engine_frame/runtime.rs`
- Modify: `compiler/engine_frame/system_adapter.rs`
- Modify: `compiler/engine_frame/physics_adapter.rs`
- Test: `compiler/tests/system_adapter.rs`
- Test: `compiler/tests/physics_adapter.rs`

- [ ] **Step 1: Add failing adapter test for frame-derived system dt**

Add this invoker to `compiler/tests/system_adapter.rs`:

```rust
struct DtRecordingMirInvoker {
    values: Arc<Mutex<Vec<f64>>>,
}

impl SystemMirInvoker for DtRecordingMirInvoker {
    fn invoke(
        &self,
        _mir_function_id: u32,
        ctx: &mut SystemInvocationContext<'_>,
    ) -> Result<(), String> {
        self.values
            .lock()
            .map_err(|_| "dt recording lock poisoned".to_string())?
            .push(ctx.dt_seconds);
        Ok(())
    }
}
```

Add this test:

```rust
#[test]
fn system_adapter_passes_dt_from_engine_frame_clocks() {
    let mut runtime = EngineFrameRuntime::new(Box::new(NoopStateAdvanceExecutor));
    let input_adapter = InputSubsystemAdapter::new(
        InputMapPlan::empty("empty"),
        runtime.materialized_tick_input_slot(),
    );
    let program = SystemProgram::new(vec![SystemPlan::new(
        SystemId::new("RecordDt"),
        SystemContractId::new("record_dt"),
        SystemPhase::Sim,
        SystemAccessSummary::default().reads(SystemResourceId::InputFrame),
        1,
    )])
    .expect("program");
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let system_adapter = SystemSubsystemAdapter::with_invoker(
        program,
        input_adapter.shared_frame(),
        Arc::new(DtRecordingMirInvoker {
            values: Arc::clone(&recorded),
        }),
    );
    let previous_snapshot = stable_region_snapshot_handle(&SmolStr::new("system_adapter_dt"));
    let mut input = frame_input(
        "system_adapter_dt",
        previous_snapshot,
        SimulationTick::new(1),
        Vec::new(),
    );
    input.previous_clock.wall_clock = WallClockStamp::new(1_000_000_000);
    input.current_clock.wall_clock = WallClockStamp::new(1_008_333_333);

    runtime
        .run_frame_with_subsystems(input, vec![Box::new(input_adapter), Box::new(system_adapter)])
        .expect("frame");

    let values = recorded.lock().expect("recorded dt");
    assert_eq!(values.len(), 1);
    assert!((values[0] - (1.0 / 120.0)).abs() < 0.000_001);
}
```

- [ ] **Step 2: Run the dt test and verify it fails**

Run:

```bash
cargo test -p wrela --test system_adapter -- system_adapter_passes_dt_from_engine_frame_clocks --nocapture
```

Expected: failure because `ctx.dt_seconds` is still `1.0 / 60.0`.

- [ ] **Step 3: Add `EngineFrameInput::frame_dt_seconds`**

In `compiler/engine_frame/runtime.rs`, extend the `impl EngineFrameInput` block:

```rust
pub fn frame_dt_seconds(&self) -> f64 {
    let nanos = self
        .current_clock
        .wall_clock
        .get()
        .saturating_sub(self.previous_clock.wall_clock.get());
    if nanos == 0 {
        return 0.0;
    }
    nanos as f64 / 1_000_000_000.0
}
```

- [ ] **Step 4: Thread dt through `SystemSubsystemAdapter`**

In `compiler/engine_frame/system_adapter.rs`, add a field:

```rust
dt_seconds: Arc<Mutex<f64>>,
```

Initialize it in both constructors:

```rust
dt_seconds: Arc::new(Mutex::new(0.0)),
```

Add `prepare_frame` before `build`:

```rust
fn prepare_frame(&mut self, input: &super::EngineFrameInput) {
    if let Ok(mut dt) = self.dt_seconds.lock() {
        *dt = input.frame_dt_seconds();
    }
}
```

In each system job closure, clone `dt_seconds` and set the invocation context from the slot:

```rust
let dt_seconds = Arc::clone(&self.dt_seconds);
```

Inside the closure before creating `SystemInvocationContext`:

```rust
let frame_dt_seconds = *dt_seconds
    .lock()
    .map_err(|_| EngineFrameError::Message("system dt lock poisoned".into()))?;
```

Then replace:

```rust
dt_seconds: 1.0 / 60.0,
```

with:

```rust
dt_seconds: frame_dt_seconds,
```

- [ ] **Step 5: Thread dt through `PhysicsSubsystemAdapter`**

In `compiler/engine_frame/physics_adapter.rs`, update `prepare_frame`:

```rust
fn prepare_frame(&mut self, input: &super::EngineFrameInput) {
    if let Ok(mut report) = self.report.lock() {
        *report = None;
    }
    if let Ok(mut dt) = self.dt.lock() {
        *dt = input.frame_dt_seconds() as f32;
    }
}
```

Keep `PhysicsSubsystemAdapter::new(solver, dt)` as the initial value for tests that build a plan without preparing a frame.

- [ ] **Step 6: Add physics adapter dt regression**

In `compiler/tests/physics_adapter.rs`, add a test using a high initial body and two different frame clock deltas. Run one frame with `1.0 / 120.0`, then one with `1.0 / 30.0`, and assert the second frame moves farther downward than the first. Use `physics.solver()` before boxing to inspect the shared solver after each frame:

```rust
#[test]
fn physics_adapter_uses_frame_clock_dt_each_frame() {
    let mut runtime = EngineFrameRuntime::new(Box::new(NoopStateAdvanceExecutor));
    let input_adapter = InputSubsystemAdapter::new(
        InputMapPlan::empty("empty"),
        runtime.materialized_tick_input_slot(),
    );
    let empty_systems = SystemSubsystemAdapter::new(
        SystemProgram::new(Vec::new()).expect("empty systems"),
        input_adapter.shared_frame(),
    );
    let body = PhysicsBodyDescriptor::dynamic_sphere(PhysicsBodyId(1), 1.0, 0.5);
    let solver = PhysicsSolver::new(
        PhysicsPlan::cpu(vec![body]),
        vec![PhysicsBodyState::new(PhysicsBodyId(1), [0.0, 5.0, 0.0])],
    );
    let physics = PhysicsSubsystemAdapter::new(solver, 1.0 / 60.0);
    let solver_handle = physics.solver();
    let previous_snapshot = stable_region_snapshot_handle(&SmolStr::new("physics_adapter_dt"));
    let tick = SimulationTick::new(1);
    let mut input = wrela::engine_frame::EngineFrameInput {
        scenario_id: "physics_adapter_dt".into(),
        frame_index: 0,
        previous_snapshot: previous_snapshot.clone(),
        previous_clock: TemporalClock::new(
            wrela::time_semantics::SnapshotEpoch::new(previous_snapshot.epoch().0),
            SimulationTick::new(0),
            PresentationFrame::new(0),
            WallClockStamp::new(1_000_000_000),
        ),
        current_clock: TemporalClock::new(
            wrela::time_semantics::SnapshotEpoch::new(previous_snapshot.epoch().0 + 1),
            tick,
            PresentationFrame::new(1),
            WallClockStamp::new(1_008_333_333),
        ),
        tick_inputs: wrela::engine_frame::TickInputSource::eager(TickInputBatch::new(tick, Vec::new())),
        policy: EngineFrameRuntimePolicy::live(),
        query_requests: Vec::new(),
        readback_requests: Vec::new(),
    };
    runtime
        .run_frame_with_subsystems(
            input.clone(),
            vec![Box::new(input_adapter), Box::new(empty_systems), Box::new(physics)],
        )
        .expect("120hz frame");
    let y_after_120 = solver_handle.lock().expect("solver").bodies()[0].position[1];

    input.frame_index = 1;
    input.previous_clock = input.current_clock;
    input.current_clock.wall_clock = WallClockStamp::new(1_041_666_666);
    let input_adapter = InputSubsystemAdapter::new(
        InputMapPlan::empty("empty"),
        runtime.materialized_tick_input_slot(),
    );
    let empty_systems = SystemSubsystemAdapter::new(
        SystemProgram::new(Vec::new()).expect("empty systems"),
        input_adapter.shared_frame(),
    );
    let physics = PhysicsSubsystemAdapter::new(
        PhysicsSolver::new(
            PhysicsPlan::cpu(vec![PhysicsBodyDescriptor::dynamic_sphere(PhysicsBodyId(1), 1.0, 0.5)]),
            vec![PhysicsBodyState::new(PhysicsBodyId(1), [0.0, 5.0, 0.0])],
        ),
        1.0 / 60.0,
    );
    let second_solver = physics.solver();
    runtime
        .run_frame_with_subsystems(input, vec![Box::new(input_adapter), Box::new(empty_systems), Box::new(physics)])
        .expect("30hz frame");
    let y_after_30 = second_solver.lock().expect("solver").bodies()[0].position[1];
    assert!(y_after_30 < y_after_120);
}
```

- [ ] **Step 7: Run dt tests**

Run:

```bash
cargo test -p wrela --test system_adapter -- system_adapter_passes_dt_from_engine_frame_clocks --nocapture
cargo test -p wrela --test physics_adapter -- physics_adapter_uses_frame_clock_dt_each_frame --nocapture
```

Expected: both pass.

- [ ] **Step 8: Commit**

```bash
git add compiler/engine_frame/runtime.rs compiler/engine_frame/system_adapter.rs compiler/engine_frame/physics_adapter.rs compiler/tests/system_adapter.rs compiler/tests/physics_adapter.rs
git commit -m "fix: derive live subsystem dt from frame clocks -Codex Automated"
```

## Task 3: Report Physics Solver Work as CPU Work

**Files:**
- Modify: `compiler/engine_frame/physics_adapter.rs`
- Test: `compiler/tests/physics_adapter.rs`

- [ ] **Step 1: Add failing telemetry test**

Add to `compiler/tests/physics_adapter.rs`:

```rust
#[test]
fn collision_backed_physics_reports_cpu_solver_time_not_gpu_proxy_time() {
    let mut runtime = EngineFrameRuntime::new(Box::new(NoopStateAdvanceExecutor));
    let input_adapter = InputSubsystemAdapter::new(
        InputMapPlan::empty("empty"),
        runtime.materialized_tick_input_slot(),
    );
    let empty_systems = SystemSubsystemAdapter::new(
        SystemProgram::new(Vec::new()).expect("empty systems"),
        input_adapter.shared_frame(),
    );
    let body = PhysicsBodyDescriptor::dynamic_sphere(PhysicsBodyId(1), 1.0, 0.5);
    let solver = PhysicsSolver::new(
        PhysicsPlan::collision_backed(vec![body]),
        vec![PhysicsBodyState::new(PhysicsBodyId(1), [0.0, 0.1, 0.0])],
    );
    let physics = PhysicsSubsystemAdapter::new(solver, 1.0 / 60.0);
    let previous_snapshot = stable_region_snapshot_handle(&SmolStr::new("physics_adapter_cpu_report"));
    let tick = SimulationTick::new(1);

    let output = runtime
        .run_frame_with_subsystems(
            wrela::engine_frame::EngineFrameInput {
                scenario_id: "physics_adapter_cpu_report".into(),
                frame_index: 0,
                previous_snapshot: previous_snapshot.clone(),
                previous_clock: TemporalClock::new(
                    wrela::time_semantics::SnapshotEpoch::new(previous_snapshot.epoch().0),
                    SimulationTick::new(0),
                    PresentationFrame::new(0),
                    WallClockStamp::new(0),
                ),
                current_clock: TemporalClock::new(
                    wrela::time_semantics::SnapshotEpoch::new(previous_snapshot.epoch().0 + 1),
                    tick,
                    PresentationFrame::new(1),
                    WallClockStamp::new(16_666_667),
                ),
                tick_inputs: wrela::engine_frame::TickInputSource::eager(TickInputBatch::new(tick, Vec::new())),
                policy: EngineFrameRuntimePolicy::live(),
                query_requests: Vec::new(),
                readback_requests: Vec::new(),
            },
            vec![Box::new(input_adapter), Box::new(empty_systems), Box::new(physics)],
        )
        .expect("frame");

    let report = output
        .report
        .subsystem(EngineSubsystemKind::Physics)
        .expect("physics report");
    assert!(report.cpu_critical_path_micros > 0);
    assert_eq!(report.gpu_critical_path_micros, None);
}
```

- [ ] **Step 2: Run telemetry test and verify it fails**

Run:

```bash
cargo test -p wrela --test physics_adapter -- collision_backed_physics_reports_cpu_solver_time_not_gpu_proxy_time --nocapture
```

Expected: failure because collision-backed physics reports CPU as zero and GPU as `Some(executed)`.

- [ ] **Step 3: Classify the XPBD adapter job as CPU**

In `compiler/engine_frame/physics_adapter.rs`, replace the `job_affinity` and `span_domain` conditional with:

```rust
let job_affinity = EngineJobAffinity::Cpu;
let span_domain = EngineSpanDomain::Cpu;
```

Leave `descriptor.requires_gpu = collision_backed` and `descriptor.allows_hot_path_readback = collision_backed`, because the subsystem still depends on collision GPU capability and readback policy.

- [ ] **Step 4: Report only measured CPU timing**

In the `EngineSubsystemReport` builder, replace:

```rust
cpu_critical_path_micros: if collision_backed { 0 } else { executed },
gpu_critical_path_micros: if collision_backed {
    Some(executed)
} else {
    None
},
```

with:

```rust
cpu_critical_path_micros: executed,
gpu_critical_path_micros: None,
```

Replace the GPU timing policy block with:

```rust
gpu_timing: EngineGpuTimingPolicy::Disabled,
```

Keep `wait_time_micros: report.contact_readback_micros` so contact readback remains visible.

- [ ] **Step 5: Run physics adapter tests**

Run:

```bash
cargo test -p wrela --test physics_adapter -- --nocapture
```

Expected: all physics adapter tests pass.

- [ ] **Step 6: Commit**

```bash
git add compiler/engine_frame/physics_adapter.rs compiler/tests/physics_adapter.rs
git commit -m "fix: report physics solver CPU time honestly -Codex Automated"
```

## Task 4: Preserve Body-Body Contacts in Collision-Backed Physics

**Files:**
- Modify: `compiler/physics_exec/mod.rs`
- Test: `compiler/tests/physics_xpbd_determinism.rs`

- [ ] **Step 1: Add failing body-body contact test**

Add to `compiler/tests/physics_xpbd_determinism.rs`:

```rust
#[test]
fn collision_backed_solver_resolves_body_body_contacts_without_world_contact() {
    let a = PhysicsBodyDescriptor::dynamic_sphere(PhysicsBodyId(1), 1.0, 0.5);
    let b = PhysicsBodyDescriptor::dynamic_sphere(PhysicsBodyId(2), 1.0, 0.5);
    let plan = PhysicsPlan::collision_backed(vec![a, b]);
    let mut solver = PhysicsSolver::with_collision_executor(
        plan,
        vec![
            PhysicsBodyState::new(PhysicsBodyId(1), [0.0, 3.0, 0.0]),
            PhysicsBodyState::new(PhysicsBodyId(2), [0.75, 3.0, 0.0]),
        ],
        Arc::new(NoopBodyContactExecutor),
    )
    .with_collision_world(test_collision_world());

    let before = {
        let bodies = solver.bodies();
        ((bodies[1].position[0] - bodies[0].position[0]).powi(2)
            + (bodies[1].position[1] - bodies[0].position[1]).powi(2)
            + (bodies[1].position[2] - bodies[0].position[2]).powi(2))
        .sqrt()
    };
    let report = solver.step(1.0 / 60.0).expect("step");
    let after = {
        let bodies = solver.bodies();
        ((bodies[1].position[0] - bodies[0].position[0]).powi(2)
            + (bodies[1].position[1] - bodies[0].position[1]).powi(2)
            + (bodies[1].position[2] - bodies[0].position[2]).powi(2))
        .sqrt()
    };

    assert!(report.contacts_resolved > 0);
    assert!(after > before, "body-body contact should separate overlapping dynamic bodies");
}

#[derive(Debug)]
struct NoopBodyContactExecutor;

impl PhysicsCollisionBatchExecutor for NoopBodyContactExecutor {
    fn submit_collision_batch(
        &self,
        _batch: &CollisionWorkloadBatch,
        _bodies: &[PhysicsBodyState],
        _descriptors: &std::collections::HashMap<PhysicsBodyId, PhysicsBodyDescriptor>,
    ) -> PhysicsCollisionBatchExecution {
        PhysicsCollisionBatchExecution {
            submitted: true,
            executor: SmolStr::new("noop_body_contact_test"),
            used_cpu_oracle_fallback: false,
            error: None,
            contacts: Vec::new(),
        }
    }
}
```

- [ ] **Step 2: Run the body-body contact test and verify it fails**

Run:

```bash
cargo test -p wrela --test physics_xpbd_determinism -- collision_backed_solver_resolves_body_body_contacts_without_world_contact --nocapture
```

Expected: failure because collision-backed contacts contain no body-body contacts.

- [ ] **Step 3: Split CPU oracle contacts by responsibility**

In `compiler/physics_exec/mod.rs`, replace `cpu_oracle_collision_contacts` with these helpers:

```rust
fn cpu_oracle_world_contacts(
    bodies: &[PhysicsBodyState],
    descriptors: &HashMap<PhysicsBodyId, PhysicsBodyDescriptor>,
) -> Vec<PhysicsContact> {
    let mut contacts = Vec::new();
    for body in bodies {
        let Some(descriptor) = descriptors.get(&body.id) else {
            continue;
        };
        let bottom = body.position[1] - descriptor.radius;
        if bottom < 0.0 {
            contacts.push(PhysicsContact {
                body: body.id,
                other: None,
                normal_world: [0.0, 1.0, 0.0],
                penetration: -bottom,
                generated_by_ccd: false,
            });
        }
    }
    contacts
}

fn cpu_oracle_body_body_contacts(
    bodies: &[PhysicsBodyState],
    descriptors: &HashMap<PhysicsBodyId, PhysicsBodyDescriptor>,
) -> Vec<PhysicsContact> {
    let mut contacts = Vec::new();
    for i in 0..bodies.len() {
        for j in (i + 1)..bodies.len() {
            let body_a = &bodies[i];
            let body_b = &bodies[j];
            let (Some(desc_a), Some(desc_b)) =
                (descriptors.get(&body_a.id), descriptors.get(&body_b.id))
            else {
                continue;
            };
            let dx = body_b.position[0] - body_a.position[0];
            let dy = body_b.position[1] - body_a.position[1];
            let dz = body_b.position[2] - body_a.position[2];
            let d2 = dx * dx + dy * dy + dz * dz;
            let sum_r = desc_a.radius + desc_b.radius;
            if d2 >= sum_r * sum_r {
                continue;
            }
            let d = d2.sqrt().max(1e-6);
            contacts.push(PhysicsContact {
                body: body_a.id,
                other: Some(body_b.id),
                normal_world: [dx / d, dy / d, dz / d],
                penetration: sum_r - d,
                generated_by_ccd: false,
            });
        }
    }
    contacts
}

fn cpu_oracle_collision_contacts(
    bodies: &[PhysicsBodyState],
    descriptors: &HashMap<PhysicsBodyId, PhysicsBodyDescriptor>,
) -> Vec<PhysicsContact> {
    let mut contacts = cpu_oracle_world_contacts(bodies, descriptors);
    contacts.extend(cpu_oracle_body_body_contacts(bodies, descriptors));
    contacts
}
```

- [ ] **Step 4: Include body-body contacts in collision-backed solve path**

In `PhysicsSolver::submit_collision_workload_batches`, after the loop over collision batches and before returning:

```rust
contacts.extend(cpu_oracle_body_body_contacts(
    &self.bodies,
    &self.descriptors,
));
```

Do not increment `fallback_count` for these body-body contacts. They are CPU solver broadphase contacts, not a collision-exec fallback.

- [ ] **Step 5: Keep CPU-oracle executor world-only fallback behavior explicit**

In `CpuOraclePhysicsCollisionBatchExecutor::submit_collision_batch`, replace:

```rust
let contacts = if batch.contract_id == COLLISION_SPHERE_OVERLAP_WORLD {
    cpu_oracle_collision_contacts(bodies, descriptors)
} else {
    Vec::new()
};
```

with:

```rust
let contacts = if batch.contract_id == COLLISION_SPHERE_OVERLAP_WORLD {
    cpu_oracle_world_contacts(bodies, descriptors)
} else {
    Vec::new()
};
```

The solver path now appends body-body contacts once, so this prevents duplicate pair contacts when the CPU oracle executor is used as a collision-batch executor.

- [ ] **Step 6: Run physics determinism tests**

Run:

```bash
cargo test -p wrela --test physics_xpbd_determinism -- --nocapture
```

Expected: all tests pass and the new body-body test separates the overlapping pair.

- [ ] **Step 7: Commit**

```bash
git add compiler/physics_exec/mod.rs compiler/tests/physics_xpbd_determinism.rs
git commit -m "fix: keep body-body contacts in collision-backed physics -Codex Automated"
```

## Task 5: Enforce Audio Voice Gate at the Mixer

**Files:**
- Modify: `runtime/src/audio/voice.rs`
- Test: `runtime/tests/audio_headless.rs`

- [ ] **Step 1: Add failing audio gate test**

Add to `runtime/tests/audio_headless.rs`:

```rust
use wrela_runtime::audio::ring::SampleRing;
use wrela_runtime::audio::voice::{DspProgram, VoiceRenderer, VoiceState};

#[test]
fn renderer_mutes_gated_off_voice_even_when_program_ignores_gate() {
    let mut renderer = VoiceRenderer::new(48_000);
    let ring = SampleRing::with_capacity(32);
    let voice = VoiceState {
        id: 1,
        source_signature: 1,
        source_program: DspProgram::sine(),
        source_frequency_hz: 440.0,
        position: [0.0, 0.0, 1.0],
        velocity: [0.0, 0.0, 0.0],
        gain: 1.0,
        priority: 1,
        occlusion_db: 0.0,
        reverb_send: 0.0,
        lowpass_hz: 20_000.0,
        gate: false,
    };

    assert_eq!(renderer.render_to_ring(&[voice], &ring, 16), 16);

    let mut out = [[1.0f32; 2]; 16];
    assert_eq!(ring.pop_stereo_block(&mut out), 16);
    assert!(out.iter().all(|frame| frame[0] == 0.0 && frame[1] == 0.0));
}
```

- [ ] **Step 2: Run the audio gate test and verify it fails**

Run:

```bash
cargo test -p wrela_runtime --test audio_headless -- renderer_mutes_gated_off_voice_even_when_program_ignores_gate --nocapture
```

Expected: failure because the default sine program still renders while `gate=false`.

- [ ] **Step 3: Multiply mixer output by gate**

In `runtime/src/audio/voice.rs`, replace:

```rust
let sample = voice.source_program.evaluate(*phase, freq, voice.gate)
    * voice.gain
    * distance_attenuation(voice.position)
    * media_gain(voice);
```

with:

```rust
let gate_gain = if voice.gate { 1.0 } else { 0.0 };
let sample = voice.source_program.evaluate(*phase, freq, voice.gate)
    * gate_gain
    * voice.gain
    * distance_attenuation(voice.position)
    * media_gain(voice);
```

Keep phase advancement outside the gate so oscillators remain time-continuous across gate changes.

- [ ] **Step 4: Run runtime audio tests**

Run:

```bash
cargo test -p wrela_runtime --test audio_headless -- --nocapture
cargo test -p wrela --test audio_dsp_offline -- --nocapture
```

Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add runtime/src/audio/voice.rs runtime/tests/audio_headless.rs
git commit -m "fix: mute gated audio voices at mixer boundary -Codex Automated"
```

## Task 6: Query High-Priority Audio Media Every Frame

**Files:**
- Modify: `compiler/audio_exec/mod.rs`
- Test: `compiler/tests/audio_voice_ledger.rs`

- [ ] **Step 1: Replace the misleading stagger test**

In `compiler/tests/audio_voice_ledger.rs`, replace `media_queries_stagger_lower_priority_voices_without_spurious_budget_findings` with:

```rust
#[test]
fn media_queries_keep_high_priority_voices_full_rate_without_budget_findings() {
    let ledger = Arc::new(VoiceLedger::new());
    let publisher = AudioSnapshotPublisher::new(
        AudioConfig {
            max_voices: 4,
            max_full_rate_media_queries: 2,
            ..AudioConfig::default()
        },
        Arc::clone(&ledger),
    );
    let plan = AudioDspPlan {
        voices: vec![
            sine_voice(1, 100, 0.5),
            sine_voice(2, 90, 0.5),
            sine_voice(3, 10, 0.5),
            sine_voice(4, 5, 0.5),
        ],
    };

    let first = publisher.publish(1, &plan);
    let second = publisher.publish(2, &plan);
    let third = publisher.publish(3, &plan);

    assert_eq!(first.media_queries, 2);
    assert_eq!(second.media_queries, 2);
    assert_eq!(third.media_queries, 2);
    assert_eq!(first.media_queried_voice_ids, vec![1, 2]);
    assert_eq!(second.media_queried_voice_ids, vec![1, 2]);
    assert_eq!(third.media_queried_voice_ids, vec![1, 2]);
    assert!([first, second, third].iter().all(|report| {
        !report
            .structured_findings
            .contains(&AudioFinding::MediaQueriesOverBudget)
    }));
}
```

- [ ] **Step 2: Run the media-query test and verify it fails**

Run:

```bash
cargo test -p wrela --test audio_voice_ledger -- media_queries_keep_high_priority_voices_full_rate_without_budget_findings --nocapture
```

Expected: failure because the second frame queries `[3, 4]`.

- [ ] **Step 3: Make media queries stable by priority**

In `compiler/audio_exec/mod.rs`, replace `media_queries_for_frame` with:

```rust
fn media_queries_for_frame(&self, voices: &[AudioVoicePlan]) -> Vec<u64> {
    let cap = self.config.max_full_rate_media_queries;
    if cap == 0 || voices.is_empty() {
        return Vec::new();
    }
    voices
        .iter()
        .take(cap.min(voices.len()))
        .map(|voice| voice.id.0)
        .collect()
}
```

Remove the `media_query_cursor` field from `AudioSnapshotPublisher` and remove `media_query_cursor: AtomicU64::new(0),` from both constructors. Remove `AtomicU64` from the imports if `last_seen_underruns` is the only remaining atomic field covered by a narrower import.

- [ ] **Step 4: Run audio execution tests**

Run:

```bash
cargo test -p wrela --test audio_voice_ledger -- --nocapture
cargo test -p wrela --test audio_voice_stealing_determinism -- --nocapture
```

Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add compiler/audio_exec/mod.rs compiler/tests/audio_voice_ledger.rs
git commit -m "fix: keep priority audio media queries full rate -Codex Automated"
```

## Task 7: Preserve Snapshot Epoch on Load

**Files:**
- Modify: `compiler/persistence/load_plan.rs`
- Test: `compiler/tests/persistence_round_trip.rs`

- [ ] **Step 1: Add failing epoch preservation test**

Add to `compiler/tests/persistence_round_trip.rs`:

```rust
#[test]
fn load_preserves_snapshot_epoch_when_sim_tick_is_larger() {
    let snapshot = stable_region_snapshot_handle(&SmolStr::new("save_demo"));
    let record = save_snapshot_record(&snapshot, &project(), 5_000, 7_000, Vec::new()).expect("save");
    let saved_epoch = record.header.snapshot_epoch;
    assert_ne!(saved_epoch, record.header.sim_tick);

    let (snapshot, plan) = load_snapshot_record(record, &project()).expect("load");

    assert_eq!(snapshot.epoch().0, saved_epoch);
    assert_eq!(plan.snapshot_epoch.0, saved_epoch);
    assert_eq!(plan.sim_tick, 5_000);
}
```

- [ ] **Step 2: Run the persistence test and verify it fails**

Run:

```bash
cargo test -p wrela --test persistence_round_trip -- load_preserves_snapshot_epoch_when_sim_tick_is_larger --nocapture
```

Expected: failure because loaded epoch is `5000`.

- [ ] **Step 3: Load from saved snapshot epoch only**

In `compiler/persistence/load_plan.rs`, replace:

```rust
let epoch = SnapshotEpoch(
    record
        .header
        .snapshot_epoch
        .max(record.header.sim_tick)
        .max(1),
);
```

with:

```rust
let epoch = SnapshotEpoch(record.header.snapshot_epoch.max(1));
```

Update the comment above `load_snapshot_record` so it no longer says the loader may fall back to `sim_tick`.

- [ ] **Step 4: Update the existing wrong expectation**

In `compiler/tests/persistence_round_trip.rs`, bind the saved epoch before loading:

```rust
let saved_epoch = record.header.snapshot_epoch.max(1);
let (_loaded_snapshot, plan) = load_snapshot_record(record, &project()).expect("load");
```

Then replace the old assertion:

```rust
assert_eq!(plan.snapshot_epoch.0, 42);
```

with:

```rust
assert_eq!(plan.snapshot_epoch.0, saved_epoch);
```

- [ ] **Step 5: Run persistence tests**

Run:

```bash
cargo test -p wrela --test persistence_round_trip -- --nocapture
```

Expected: all persistence round-trip tests pass.

- [ ] **Step 6: Commit**

```bash
git add compiler/persistence/load_plan.rs compiler/tests/persistence_round_trip.rs
git commit -m "fix: preserve snapshot epoch during load -Codex Automated"
```

## Task 8: Remove Mutexes From the Late Input Hot Path

**Files:**
- Modify: `compiler/engine_frame/input_ring_bridge.rs`
- Test: `compiler/tests/input_subsystem.rs`

- [ ] **Step 1: Add a regression test for non-blocking reentry behavior**

Add to `compiler/tests/input_subsystem.rs`:

```rust
#[test]
fn raw_input_late_sampler_reentrant_drain_returns_empty_instead_of_blocking() {
    let (mut producer, consumer) = wrela_runtime::platform::input_pump::RawInputRing::split_with_capacity(8);
    producer.push_event(wrela_runtime::platform::input::TimestampedRawEvent {
        source: SmolStr::new("keyboard"),
        detail: SmolStr::new("key.w.down"),
        kind: wrela_runtime::platform::input::RawInputKind::Keyboard,
        wall_clock_micros: 1,
        monotonic_nanos: 1_000,
    });
    let sampler = RawInputRingLateSampler::new(consumer);

    let first = sampler.drain_up_to(WallClockStamp::new(1_000));
    let second = sampler.drain_up_to(WallClockStamp::new(1_000));

    assert_eq!(first.inputs.len(), 1);
    assert!(second.inputs.is_empty());
}
```

This test proves the public behavior remains stable. The implementation step below removes the blocking mutexes; the code review gate verifies the hot path no longer contains `Mutex`.

- [ ] **Step 2: Run the input test before implementation**

Run:

```bash
cargo test -p wrela --test input_subsystem -- raw_input_late_sampler_reentrant_drain_returns_empty_instead_of_blocking --nocapture
```

Expected: pass before and after. This is a behavior-preservation test for the lock-free rewrite.

- [ ] **Step 3: Replace Mutex fields with `UnsafeCell` plus an atomic drain guard**

In `compiler/engine_frame/input_ring_bridge.rs`, replace imports:

```rust
use std::sync::{Arc, Mutex};
```

with:

```rust
use std::cell::UnsafeCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
```

Replace the struct with:

```rust
pub struct RawInputRingLateSampler {
    consumer: UnsafeCell<RawInputConsumer>,
    scratch: UnsafeCell<Vec<TimestampedRawEvent>>,
    draining: AtomicBool,
    now_nanos: Arc<dyn Fn() -> u64 + Send + Sync>,
}

unsafe impl Send for RawInputRingLateSampler {}
unsafe impl Sync for RawInputRingLateSampler {}
```

Add the safety comment immediately above the unsafe impls:

```rust
// SAFETY: RawInputRingLateSampler owns the single consumer half of an SPSC ring.
// drain_up_to uses `draining` as a non-blocking single-consumer guard before
// taking mutable access through UnsafeCell. ring_state and clear_overflow only
// read or update atomic telemetry through RawInputConsumer methods.
```

Update constructor fields:

```rust
consumer: UnsafeCell::new(consumer),
scratch: UnsafeCell::new(Vec::with_capacity(64)),
draining: AtomicBool::new(false),
```

- [ ] **Step 4: Add a non-blocking drain guard**

Add this helper near the struct:

```rust
struct DrainGuard<'a>(&'a AtomicBool);

impl<'a> Drop for DrainGuard<'a> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

impl RawInputRingLateSampler {
    fn try_enter_drain(&self) -> Option<DrainGuard<'_>> {
        self.draining
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .ok()
            .map(|_| DrainGuard(&self.draining))
    }
}
```

- [ ] **Step 5: Rewrite `drain_up_to`, `ring_state`, and `clear_overflow` without locks**

In the `LateInputSampler` impl, replace the current locked body with:

```rust
fn drain_up_to(&self, deadline: WallClockStamp) -> TickInputBatch {
    let Some(_guard) = self.try_enter_drain() else {
        return TickInputBatch::new(SimulationTick::new(0), Vec::new());
    };

    // SAFETY: `_guard` proves this is the only active drain. The SPSC consumer
    // half has exactly one logical consumer, owned by this sampler.
    let consumer = unsafe { &mut *self.consumer.get() };
    // SAFETY: scratch is only accessed while `_guard` is held.
    let buffer = unsafe { &mut *self.scratch.get() };
    buffer.clear();
    consumer.drain_up_to_nanos(deadline.get(), buffer);

    let mut inputs = Vec::with_capacity(buffer.len());
    for event in buffer.drain(..) {
        inputs.push(TickInputEvent::with_timestamps(
            SimulationTick::new(0),
            TickInputKind::Event,
            event.source,
            event.detail,
            WallClockStamp::new(event.wall_clock_micros.saturating_mul(1000)),
            event.monotonic_nanos,
        ));
    }
    TickInputBatch::new(SimulationTick::new(0), inputs)
}

fn ring_state(&self) -> InputRingState {
    // SAFETY: RawInputConsumer::ring_state reads atomic telemetry only.
    let s = unsafe { &*self.consumer.get() }.ring_state();
    InputRingState {
        depth: s.depth,
        dropped_events: s.dropped_events,
        overflow: s.overflow,
    }
}

fn clear_overflow(&self) {
    // SAFETY: RawInputConsumer::clear_overflow writes the atomic overflow latch only.
    unsafe { &*self.consumer.get() }.clear_overflow();
}
```

- [ ] **Step 6: Run an explicit no-Mutex scan**

Run:

```bash
! rg "Mutex<RawInputConsumer>|scratch: Mutex|\\.lock\\(\\)" compiler/engine_frame/input_ring_bridge.rs
```

Expected: no matches and exit code 0 from the leading `!` command.

- [ ] **Step 7: Run input tests and layering lint**

Run:

```bash
cargo test -p wrela --test input_subsystem -- --nocapture
cargo test -p wrela_runtime --test winit_input_pump_ring -- --nocapture
just lint-layering
```

Expected: all pass.

- [ ] **Step 8: Commit**

```bash
git add compiler/engine_frame/input_ring_bridge.rs compiler/tests/input_subsystem.rs
git commit -m "fix: remove locks from late input sampler hot path -Codex Automated"
```

## Task 9: Focused Regression Sweep

**Files:**
- No code changes unless a focused regression fails.

- [ ] **Step 1: Run all touched focused tests**

Run:

```bash
cargo test -p wrela --test system_determinism -- --nocapture
cargo test -p wrela --test system_adapter -- --nocapture
cargo test -p wrela --test physics_adapter -- --nocapture
cargo test -p wrela --test physics_xpbd_determinism -- --nocapture
cargo test -p wrela --test audio_voice_ledger -- --nocapture
cargo test -p wrela --test audio_voice_stealing_determinism -- --nocapture
cargo test -p wrela --test audio_dsp_offline -- --nocapture
cargo test -p wrela --test persistence_round_trip -- --nocapture
cargo test -p wrela --test input_subsystem -- --nocapture
cargo test -p wrela_runtime --test audio_headless -- --nocapture
cargo test -p wrela_runtime --test winit_input_pump_ring -- --nocapture
just lint-layering
```

Expected: all pass.

- [ ] **Step 2: Run current higher-level smoke lanes**

Run:

```bash
cargo test -p wrela_reference_host --test smoke -- --nocapture
just perf-engine-closure
```

Expected: both pass. Treat `just perf-engine-closure` as a required completion gate for this remediation.

- [ ] **Step 3: Audit the original findings against the final diff**

Check each condition manually:

```bash
rg -n "plan\\.access\\.emits_events.*collect|dt_seconds: 1\\.0 / 60\\.0|cpu_critical_path_micros: if collision_backed|gpu_critical_path_micros: if collision_backed|snapshot_epoch.*max\\(record.header.sim_tick\\)|Mutex<RawInputConsumer>|scratch: Mutex" compiler runtime
```

Expected: no matches for the old buggy patterns.

- [ ] **Step 4: Final commit if any sweep-only fixes were needed**

```bash
git add compiler runtime apps Cargo.lock justfile
git commit -m "test: verify RFC 0011 review remediation -Codex Automated"
```

Skip this commit if Task 9 made no file changes.

## Self-Review

- Finding 1 is covered by Task 1: declared event emitters no longer enqueue events, and actual emissions are recorded once.
- Finding 2 is covered by Task 2: systems and physics get dt from `EngineFrameInput` clocks.
- Finding 3 is covered by Task 3: XPBD solver work is reported on CPU, with no fake GPU critical path.
- Finding 4 is covered by Task 4: collision-backed physics keeps body-body contacts while still submitting world collision batches.
- Finding 5 is covered by Task 5: gated voices are muted at the mixer boundary.
- Finding 6 is covered by Task 6: top-priority audio media queries are full-rate.
- Finding 7 is covered by Task 7: load preserves saved snapshot epoch independent of sim tick.
- Finding 8 is covered by Task 8: `RawInputRingLateSampler` removes `Mutex` from the drain path.
- Placeholder scan: no placeholder tokens or vague follow-up markers, and every task has exact files, commands, and expected results.
- Type consistency: new `run_program_with_dt`, `invoke_system_body_with_dt`, and `enqueue_system_emitted_events` are introduced before later tasks depend on them.
