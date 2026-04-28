use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use smol_str::SmolStr;
use wrela::engine_frame::{
    EngineFrameRuntime, EngineFrameRuntimePolicy, EngineStateAdvanceExecutor,
    InputSubsystemAdapter, SystemSubsystemAdapter,
};
use wrela::input_contract::InputMapBinding;
use wrela::input_map_plan::InputMapPlan;
use wrela::query_exec::stable_region_snapshot_handle;
use wrela::state_advance::{
    ChangeClass, ChangeSummary, StateAdvanceResult, TickInputBatch, TickInputEvent, TickInputKind,
    WorldTransitionRecord,
};
use wrela::system_contract::{
    EventTypeId, SystemAccessSummary, SystemContractId, SystemId, SystemPhase, SystemResourceId,
};
use wrela::system_exec::{SystemInvocationContext, SystemMirInvoker};
use wrela::system_plan::{SystemPlan, SystemProgram};
use wrela::time_semantics::{PresentationFrame, SimulationTick, TemporalClock, WallClockStamp};
use wrela_runtime::engine_executor::EngineExecutorConfig;

#[derive(Default)]
struct NoopStateAdvanceExecutor;

impl EngineStateAdvanceExecutor for NoopStateAdvanceExecutor {
    fn advance(
        &mut self,
        input: wrela::engine_frame::EngineStateAdvanceInput,
    ) -> Result<StateAdvanceResult, wrela::engine_frame::EngineFrameError> {
        let previous = input.previous_snapshot.clone();
        let next =
            previous.with_epoch(wrela::world_identity::SnapshotEpoch(previous.epoch().0 + 1));
        Ok(StateAdvanceResult::new(
            WorldTransitionRecord::new(
                Some(previous),
                next,
                Some(input.previous_clock),
                input.current_clock,
                input.inputs,
                Vec::new(),
            ),
            ChangeSummary::new(ChangeClass::None, "system adapter test"),
        ))
    }
}

struct NoopMirInvoker;

impl SystemMirInvoker for NoopMirInvoker {
    fn invoke(
        &self,
        _mir_function_id: u32,
        _ctx: &mut SystemInvocationContext<'_>,
    ) -> Result<(), String> {
        Ok(())
    }
}

struct RecordingMirInvoker {
    calls: Arc<Mutex<Vec<u32>>>,
}

impl SystemMirInvoker for RecordingMirInvoker {
    fn invoke(
        &self,
        mir_function_id: u32,
        _ctx: &mut SystemInvocationContext<'_>,
    ) -> Result<(), String> {
        self.calls
            .lock()
            .map_err(|_| "recording MIR invoker lock poisoned".to_string())?
            .push(mir_function_id);
        Ok(())
    }
}

struct DtRecordingMirInvoker {
    recorded: Arc<Mutex<Vec<f64>>>,
}

impl SystemMirInvoker for DtRecordingMirInvoker {
    fn invoke(
        &self,
        _mir_function_id: u32,
        ctx: &mut SystemInvocationContext<'_>,
    ) -> Result<(), String> {
        self.recorded
            .lock()
            .map_err(|_| "dt recording MIR invoker lock poisoned".to_string())?
            .push(ctx.dt_seconds);
        Ok(())
    }
}

struct DelayedFirstMirInvoker {
    second_invoked: Arc<AtomicBool>,
}

impl SystemMirInvoker for DelayedFirstMirInvoker {
    fn invoke(
        &self,
        mir_function_id: u32,
        _ctx: &mut SystemInvocationContext<'_>,
    ) -> Result<(), String> {
        match mir_function_id {
            1 => {
                let deadline = Instant::now() + Duration::from_millis(250);
                while !self.second_invoked.load(Ordering::Acquire) {
                    if Instant::now() >= deadline {
                        return Err(
                            "second system did not run while first invocation was pending".into(),
                        );
                    }
                    thread::sleep(Duration::from_millis(1));
                }
            }
            2 => {
                self.second_invoked.store(true, Ordering::Release);
            }
            _ => {}
        }
        Ok(())
    }
}

struct ReversedCompletionEmitterInvoker {
    second_invoked: Arc<AtomicBool>,
}

impl SystemMirInvoker for ReversedCompletionEmitterInvoker {
    fn invoke(
        &self,
        mir_function_id: u32,
        ctx: &mut SystemInvocationContext<'_>,
    ) -> Result<(), String> {
        match mir_function_id {
            1 => {
                let deadline = Instant::now() + Duration::from_millis(250);
                while !self.second_invoked.load(Ordering::Acquire) {
                    if Instant::now() >= deadline {
                        return Err(
                            "second system did not emit while first invocation was pending".into(),
                        );
                    }
                    thread::sleep(Duration::from_millis(1));
                }
                ctx.emitted_events.push(EventTypeId::new("FirstEvent"));
            }
            2 => {
                ctx.emitted_events.push(EventTypeId::new("SecondEvent"));
                self.second_invoked.store(true, Ordering::Release);
            }
            _ => {}
        }
        Ok(())
    }
}

fn frame_input(
    scenario_id: &'static str,
    previous_snapshot: wrela::world_identity::WorldSnapshotHandle,
    tick: SimulationTick,
    events: Vec<TickInputEvent>,
) -> wrela::engine_frame::EngineFrameInput {
    wrela::engine_frame::EngineFrameInput {
        scenario_id: scenario_id.into(),
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
            WallClockStamp::new(16_666),
        ),
        tick_inputs: wrela::engine_frame::TickInputSource::eager(TickInputBatch::new(tick, events)),
        policy: EngineFrameRuntimePolicy::live(),
        query_requests: Vec::new(),
        readback_requests: Vec::new(),
    }
}

fn independent_emitter_program() -> SystemProgram {
    let first = SystemPlan::new(
        SystemId::new("First"),
        SystemContractId::new("first"),
        SystemPhase::Sim,
        SystemAccessSummary::default()
            .reads(SystemResourceId::Resource("first".into()))
            .emits_event(EventTypeId::new("FirstEvent")),
        1,
    );
    let second = SystemPlan::new(
        SystemId::new("Second"),
        SystemContractId::new("second"),
        SystemPhase::Sim,
        SystemAccessSummary::default()
            .reads(SystemResourceId::Resource("second".into()))
            .emits_event(EventTypeId::new("SecondEvent")),
        2,
    );
    SystemProgram::new([first, second]).expect("program")
}

#[test]
fn system_adapter_new_runs_empty_program_through_idle_path() {
    let mut runtime = EngineFrameRuntime::new(Box::new(NoopStateAdvanceExecutor));
    let input_adapter = InputSubsystemAdapter::new(
        InputMapPlan::empty("empty"),
        runtime.materialized_tick_input_slot(),
    );
    let system_program = SystemProgram::new(Vec::new()).expect("empty systems");
    let system_adapter = SystemSubsystemAdapter::new(system_program, input_adapter.shared_frame());
    let executor = system_adapter.executor();
    let tick = SimulationTick::new(1);
    let previous_snapshot = stable_region_snapshot_handle(&SmolStr::new("system_adapter_empty"));
    let output = runtime
        .run_frame_with_subsystems(
            frame_input("system_adapter_empty", previous_snapshot, tick, Vec::new()),
            vec![Box::new(input_adapter), Box::new(system_adapter)],
        )
        .expect("empty system frame");

    let system_report = output
        .report
        .subsystem(wrela::engine_frame::EngineSubsystemKind::System)
        .expect("system report");
    assert_eq!(system_report.work_items, 0);
    assert_eq!(executor.lock().expect("executor").report().records.len(), 0);
}

#[test]
fn system_subsystem_runs_after_input_and_reports_work() {
    let mut runtime = EngineFrameRuntime::new(Box::new(NoopStateAdvanceExecutor));
    let input_adapter = InputSubsystemAdapter::new(
        InputMapPlan::new(
            "player",
            vec![InputMapBinding::new(
                "MoveForward",
                "keyboard",
                "key.w.down",
            )],
        )
        .expect("input map"),
        runtime.materialized_tick_input_slot(),
    );
    let system_program = SystemProgram::new([SystemPlan::new(
        SystemId::new("DrainInput"),
        SystemContractId::new("drain"),
        SystemPhase::PreSim,
        SystemAccessSummary::default()
            .reads(SystemResourceId::InputFrame)
            .writes(SystemResourceId::Resource("player".into())),
        1,
    )])
    .expect("program");
    let system_adapter = SystemSubsystemAdapter::with_invoker(
        system_program,
        input_adapter.shared_frame(),
        Arc::new(NoopMirInvoker),
    );
    let executor = system_adapter.executor();
    let tick = SimulationTick::new(1);
    let previous_snapshot = stable_region_snapshot_handle(&SmolStr::new("system_adapter"));
    let output = runtime
        .run_frame_with_subsystems(
            frame_input(
                "system_adapter",
                previous_snapshot,
                tick,
                vec![TickInputEvent::with_timestamps(
                    tick,
                    TickInputKind::Event,
                    "keyboard",
                    "key.w.down",
                    WallClockStamp::new(10),
                    10,
                )],
            ),
            vec![Box::new(input_adapter), Box::new(system_adapter)],
        )
        .expect("frame");
    let system_report = output
        .report
        .subsystem(wrela::engine_frame::EngineSubsystemKind::System)
        .expect("system report");
    assert_eq!(system_report.work_items, 1);
    assert_eq!(executor.lock().expect("executor").report().records.len(), 1);
}

#[test]
fn system_adapter_passes_dt_from_engine_frame_clocks() {
    let mut runtime = EngineFrameRuntime::new(Box::new(NoopStateAdvanceExecutor));
    let input_adapter = InputSubsystemAdapter::new(
        InputMapPlan::empty("empty"),
        runtime.materialized_tick_input_slot(),
    );
    let system_program = SystemProgram::new([SystemPlan::new(
        SystemId::new("RecordDt"),
        SystemContractId::new("record_dt"),
        SystemPhase::Sim,
        SystemAccessSummary::default().reads(SystemResourceId::InputFrame),
        1,
    )])
    .expect("program");
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let system_adapter = SystemSubsystemAdapter::with_invoker(
        system_program,
        input_adapter.shared_frame(),
        Arc::new(DtRecordingMirInvoker {
            recorded: Arc::clone(&recorded),
        }),
    );
    let tick = SimulationTick::new(1);
    let previous_snapshot = stable_region_snapshot_handle(&SmolStr::new("system_adapter_dt"));
    let input = wrela::engine_frame::EngineFrameInput {
        scenario_id: "system_adapter_dt".into(),
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
        tick_inputs: wrela::engine_frame::TickInputSource::eager(TickInputBatch::new(
            tick,
            Vec::new(),
        )),
        policy: EngineFrameRuntimePolicy::live(),
        query_requests: Vec::new(),
        readback_requests: Vec::new(),
    };

    runtime
        .run_frame_with_subsystems(
            input,
            vec![Box::new(input_adapter), Box::new(system_adapter)],
        )
        .expect("frame");

    let recorded = recorded.lock().expect("recorded dt");
    assert_eq!(recorded.len(), 1);
    assert!((recorded[0] - (1.0 / 120.0)).abs() < 0.000001);
}

#[test]
fn system_adapter_publishes_parallel_emissions_in_program_order_next_tick() {
    let mut runtime = EngineFrameRuntime::with_executor_config(
        Box::new(NoopStateAdvanceExecutor),
        EngineExecutorConfig {
            cpu_worker_threads: 2,
            external_worker_threads: 1,
        },
    );
    let input_adapter = InputSubsystemAdapter::new(
        InputMapPlan::empty("empty"),
        runtime.materialized_tick_input_slot(),
    );
    let second_invoked = Arc::new(AtomicBool::new(false));
    let system_adapter = SystemSubsystemAdapter::with_invoker(
        independent_emitter_program(),
        input_adapter.shared_frame(),
        Arc::new(ReversedCompletionEmitterInvoker {
            second_invoked: Arc::clone(&second_invoked),
        }),
    );
    let executor = system_adapter.executor();
    let previous_snapshot =
        stable_region_snapshot_handle(&SmolStr::new("system_adapter_parallel_emit_order"));
    let mut subsystem_adapters: Vec<Box<dyn wrela::engine_frame::EngineSubsystemAdapter>> =
        vec![Box::new(input_adapter), Box::new(system_adapter)];

    let first_output = runtime
        .run_frame_with_persistent_subsystems(
            frame_input(
                "system_adapter_parallel_emit_order",
                previous_snapshot,
                SimulationTick::new(1),
                Vec::new(),
            ),
            &mut subsystem_adapters,
        )
        .expect("first frame");

    let next_snapshot = first_output.snapshot.clone();
    runtime
        .run_frame_with_persistent_subsystems(
            frame_input(
                "system_adapter_parallel_emit_order",
                next_snapshot,
                SimulationTick::new(2),
                Vec::new(),
            ),
            &mut subsystem_adapters,
        )
        .expect("second frame");

    let visible_events = executor
        .lock()
        .expect("executor")
        .report()
        .records
        .first()
        .expect("first system record")
        .visible_events
        .iter()
        .map(|event| event.0.to_string())
        .collect::<Vec<_>>();
    assert_eq!(visible_events, vec!["FirstEvent", "SecondEvent"]);
}

#[test]
fn system_adapter_invokes_without_executor_lock_and_commits_records_in_program_order() {
    let mut runtime = EngineFrameRuntime::with_executor_config(
        Box::new(NoopStateAdvanceExecutor),
        EngineExecutorConfig {
            cpu_worker_threads: 2,
            external_worker_threads: 1,
        },
    );
    let input_adapter = InputSubsystemAdapter::new(
        InputMapPlan::empty("empty"),
        runtime.materialized_tick_input_slot(),
    );
    let first = SystemPlan::new(
        SystemId::new("First"),
        SystemContractId::new("first"),
        SystemPhase::Sim,
        SystemAccessSummary::default().reads(SystemResourceId::Resource("first".into())),
        1,
    );
    let second = SystemPlan::new(
        SystemId::new("Second"),
        SystemContractId::new("second"),
        SystemPhase::Sim,
        SystemAccessSummary::default().reads(SystemResourceId::Resource("second".into())),
        2,
    );
    let system_program = SystemProgram::new([first, second]).expect("program");
    let second_invoked = Arc::new(AtomicBool::new(false));
    let system_adapter = SystemSubsystemAdapter::with_invoker(
        system_program,
        input_adapter.shared_frame(),
        Arc::new(DelayedFirstMirInvoker {
            second_invoked: Arc::clone(&second_invoked),
        }),
    );
    let executor = system_adapter.executor();
    let tick = SimulationTick::new(1);
    let previous_snapshot =
        stable_region_snapshot_handle(&SmolStr::new("system_adapter_commit_order"));

    runtime
        .run_frame_with_subsystems(
            frame_input(
                "system_adapter_commit_order",
                previous_snapshot,
                tick,
                Vec::new(),
            ),
            vec![Box::new(input_adapter), Box::new(system_adapter)],
        )
        .expect("frame");

    let records = executor
        .lock()
        .expect("executor")
        .report()
        .records
        .iter()
        .map(|record| record.system.0.to_string())
        .collect::<Vec<_>>();
    assert_eq!(records, vec!["First", "Second"]);
}

#[test]
fn system_adapter_orders_reader_writer_dependencies_like_system_program() {
    let mut runtime = EngineFrameRuntime::new(Box::new(NoopStateAdvanceExecutor));
    let input_adapter = InputSubsystemAdapter::new(
        InputMapPlan::empty("empty"),
        runtime.materialized_tick_input_slot(),
    );
    let reader = SystemPlan::new(
        SystemId::new("ReadPlayer"),
        SystemContractId::new("read"),
        SystemPhase::Sim,
        SystemAccessSummary::default().reads(SystemResourceId::Resource("player".into())),
        1,
    )
    .runs_before(SystemId::new("WritePlayer"));
    let writer = SystemPlan::new(
        SystemId::new("WritePlayer"),
        SystemContractId::new("write"),
        SystemPhase::Sim,
        SystemAccessSummary::default().writes(SystemResourceId::Resource("player".into())),
        2,
    );
    let system_program = SystemProgram::new([writer, reader]).expect("program");
    let calls = Arc::new(Mutex::new(Vec::new()));
    let system_adapter = SystemSubsystemAdapter::with_invoker(
        system_program,
        input_adapter.shared_frame(),
        Arc::new(RecordingMirInvoker {
            calls: Arc::clone(&calls),
        }),
    );
    let tick = SimulationTick::new(1);
    let previous_snapshot = stable_region_snapshot_handle(&SmolStr::new("system_adapter_order"));
    let output = runtime
        .run_frame_with_subsystems(
            frame_input("system_adapter_order", previous_snapshot, tick, Vec::new()),
            vec![Box::new(input_adapter), Box::new(system_adapter)],
        )
        .expect("frame");

    let system_report = output
        .report
        .subsystem(wrela::engine_frame::EngineSubsystemKind::System)
        .expect("system report");
    assert_eq!(system_report.work_items, 2);
    assert_eq!(&*calls.lock().expect("calls"), &[1, 2]);
}

#[test]
fn system_adapter_new_fails_loudly_for_authored_program_without_invoker() {
    let mut runtime = EngineFrameRuntime::new(Box::new(NoopStateAdvanceExecutor));
    let input_adapter = InputSubsystemAdapter::new(
        InputMapPlan::empty("empty"),
        runtime.materialized_tick_input_slot(),
    );
    let system_program = SystemProgram::new([SystemPlan::new(
        SystemId::new("DrainInput"),
        SystemContractId::new("drain"),
        SystemPhase::PreSim,
        SystemAccessSummary::default()
            .reads(SystemResourceId::InputFrame)
            .writes(SystemResourceId::Resource("player".into())),
        1,
    )])
    .expect("program");
    let system_adapter = SystemSubsystemAdapter::new(system_program, input_adapter.shared_frame());
    let tick = SimulationTick::new(1);
    let previous_snapshot = stable_region_snapshot_handle(&SmolStr::new("system_adapter_default"));

    let result = runtime.run_frame_with_subsystems(
        frame_input(
            "system_adapter_default",
            previous_snapshot,
            tick,
            Vec::new(),
        ),
        vec![Box::new(input_adapter), Box::new(system_adapter)],
    );

    assert!(
        result.is_err(),
        "default system adapter must not silently run authored systems"
    );
    let err = result.err().expect("error");
    assert!(
        err.to_string()
            .contains("system MIR invoker is not configured for function 1"),
        "unexpected error: {err}"
    );
}
