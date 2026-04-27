use smol_str::SmolStr;
use std::sync::{Arc, Mutex};
use wrela::engine_frame::{
    EngineFrameContext, EngineFrameError, EngineFrameInput, EngineFrameReport, EngineFrameRuntime,
    EngineFrameRuntimePolicy, EngineFrameScheduler, EngineFrameTimeline, EngineGpuTimingPolicy,
    EngineGraphBuilder, EngineJobAffinity, EngineMeasurementPolicy, EngineQueryRequest,
    EngineReadbackCategory, EngineReadbackManager, EngineReadbackReadiness, EngineReadbackRequest,
    EngineResourceAccessMode, EngineResourceEpochState, EngineResourceId, EngineResourceResidency,
    EngineRuntimeSource, EngineSpanDomain, EngineSpanId, EngineStateAdvanceExecutor,
    EngineStateAdvanceInput, EngineSubsystemAdapter, EngineSubsystemDescriptor,
    EngineSubsystemKind, EngineSubsystemPlan, EngineSubsystemReport, EngineSubsystemSpanRange,
    MotionToPhotonContract, TickInputSource,
};
use wrela::gpu_runtime::GpuRuntimeMetrics;
use wrela::query_exec::stable_region_snapshot_handle;
use wrela::state_advance::{
    ChangeClass, ChangeSummary, StateAdvanceResult, TickInputBatch, TickInputEvent, TickInputKind,
    WorldTransitionRecord,
};
use wrela::time_semantics::{PresentationFrame, SimulationTick, TemporalClock, WallClockStamp};
use wrela::world_identity::SnapshotEpoch;
use wrela_runtime::engine_executor::EngineExecutorConfig;

fn timeline_measurement_policy() -> EngineMeasurementPolicy {
    EngineMeasurementPolicy {
        runtime_source: EngineRuntimeSource::TimelineSpans,
        gpu_timing: EngineGpuTimingPolicy::Disabled,
        hot_path_readback_allowed: false,
        export_readback_allowed: false,
    }
}

#[derive(Default)]
struct NoopStateAdvanceExecutor;

impl EngineStateAdvanceExecutor for NoopStateAdvanceExecutor {
    fn advance(
        &mut self,
        input: EngineStateAdvanceInput,
    ) -> Result<StateAdvanceResult, EngineFrameError> {
        let previous = input.previous_snapshot.clone();
        let next = previous.with_epoch(SnapshotEpoch(previous.epoch().0.saturating_add(1)));
        Ok(StateAdvanceResult::new(
            WorldTransitionRecord::new(
                Some(previous),
                next,
                Some(input.previous_clock),
                input.current_clock,
                input.inputs,
                Vec::new(),
            ),
            ChangeSummary::new(ChangeClass::Structural, "noop test advance"),
        ))
    }
}

struct IncompatibleStateAdvanceExecutor;

impl EngineStateAdvanceExecutor for IncompatibleStateAdvanceExecutor {
    fn advance(
        &mut self,
        input: EngineStateAdvanceInput,
    ) -> Result<StateAdvanceResult, EngineFrameError> {
        let previous = input.previous_snapshot.clone();
        let next = previous.with_epoch(SnapshotEpoch(previous.epoch().0.saturating_add(1)));
        Ok(StateAdvanceResult::new(
            WorldTransitionRecord::new(
                Some(previous),
                next,
                Some(input.previous_clock),
                input.current_clock,
                input.inputs,
                Vec::new(),
            ),
            ChangeSummary::new(ChangeClass::Incompatible, "test incompatible advance"),
        ))
    }
}

fn runtime_input() -> EngineFrameInput {
    let tick = SimulationTick::new(41);
    let previous_snapshot =
        stable_region_snapshot_handle(&SmolStr::new("engine_frame_runtime_test"));
    EngineFrameInput {
        scenario_id: "runtime_kernel_fixture".to_string(),
        frame_index: 7,
        previous_snapshot,
        previous_clock: TemporalClock::new(
            wrela::time_semantics::SnapshotEpoch::new(1),
            SimulationTick::new(40),
            PresentationFrame::new(6),
            WallClockStamp::new(1000),
        ),
        current_clock: TemporalClock::new(
            wrela::time_semantics::SnapshotEpoch::new(2),
            tick,
            PresentationFrame::new(7),
            WallClockStamp::new(1016),
        ),
        tick_inputs: TickInputSource::eager(TickInputBatch::new(
            tick,
            vec![TickInputEvent::new(
                tick,
                TickInputKind::Command,
                "player",
                "MoveForward(strength=1.0)",
            )],
        )),
        policy: EngineFrameRuntimePolicy::closure(),
        query_requests: Vec::new(),
        readback_requests: Vec::new(),
    }
}

#[test]
fn engine_frame_runtime_state_advance_publishes_authoritative_snapshot() {
    let mut runtime = EngineFrameRuntime::new(Box::new(NoopStateAdvanceExecutor));
    let output = runtime
        .run_frame(runtime_input())
        .expect("runtime frame should advance");

    assert_eq!(output.snapshot.epoch(), SnapshotEpoch(2));
    assert_eq!(output.report.identity.input_snapshot_epoch, 1);
    assert_eq!(output.report.identity.output_snapshot_epoch, Some(2));
    assert_eq!(output.report.state_advance.as_ref().unwrap().input_count, 1);
    assert!(
        output
            .report
            .resource_ledger
            .accesses
            .iter()
            .any(|access| matches!(
                (&access.resource, access.mode),
                (
                    EngineResourceId::WorldSnapshot { epoch: 2 },
                    EngineResourceAccessMode::Write
                )
            ))
    );
    assert!(
        output
            .report
            .subsystems
            .iter()
            .any(
                |subsystem| subsystem.kind == EngineSubsystemKind::StateAdvance
                    && subsystem.measurement_policy.runtime_source
                        == EngineRuntimeSource::TimelineSpans
            )
    );
}

#[test]
fn engine_frame_runtime_rejects_incompatible_state_advance() {
    let mut runtime = EngineFrameRuntime::new(Box::new(IncompatibleStateAdvanceExecutor));
    let error = runtime
        .run_frame(runtime_input())
        .expect_err("incompatible transition should fail the frame");
    assert!(format!("{error}").contains("state advance change is incompatible"));
}

#[test]
fn engine_frame_runtime_rejects_hot_path_readbacks_in_closure_policy() {
    let mut input = runtime_input();
    input.readback_requests.push(EngineReadbackRequest {
        owner: EngineSubsystemKind::Query,
        reason: "query-result".to_string(),
        category: EngineReadbackCategory::Gameplay,
        bytes: 64,
        required_for_frame_completion: true,
    });
    let mut runtime = EngineFrameRuntime::new(Box::new(NoopStateAdvanceExecutor));
    let error = runtime
        .run_frame(input)
        .expect_err("closure policy should reject hot-path readback");
    assert!(format!("{error}").contains("hot-path readback rejected"));
}

#[test]
fn engine_frame_runtime_dedupes_queries_on_snapshot_and_contract() {
    let mut input = runtime_input();
    input.query_requests = vec![
        EngineQueryRequest {
            owner: EngineSubsystemKind::Collision,
            contract_id: "nearest-hit".to_string(),
            query_kind: "spatial.nearest".to_string(),
            required_this_tick: true,
        },
        EngineQueryRequest {
            owner: EngineSubsystemKind::Presentation,
            contract_id: "nearest-hit".to_string(),
            query_kind: "spatial.nearest".to_string(),
            required_this_tick: true,
        },
    ];
    let mut runtime = EngineFrameRuntime::new(Box::new(NoopStateAdvanceExecutor));
    let output = runtime
        .run_frame(input)
        .expect("runtime frame should batch queries");

    assert_eq!(output.query_results.batches.len(), 1);
    assert_eq!(output.query_results.batches[0].request_count, 2);
    assert_eq!(output.query_results.batches[0].snapshot_epoch, 2);
    assert_eq!(output.report.query_ledger.batches.len(), 1);
}

#[test]
fn engine_frame_runtime_records_gpu_resident_query_handles_without_value_readback() {
    let mut input = runtime_input();
    input.query_requests = vec![EngineQueryRequest {
        owner: EngineSubsystemKind::Collision,
        contract_id: "nearest-hit".to_string(),
        query_kind: "spatial.nearest".to_string(),
        required_this_tick: true,
    }];
    let mut runtime = EngineFrameRuntime::new(Box::new(NoopStateAdvanceExecutor));
    let output = runtime
        .run_frame(input)
        .expect("runtime frame should publish resident query handles");

    assert_eq!(output.query_results.resident_handles.len(), 1);
    let handle = &output.query_results.resident_handles[0];
    assert_eq!(handle.snapshot_epoch, 2);
    assert_eq!(handle.residency, EngineResourceResidency::GpuResident);
    assert!(!handle.value_readback_scheduled);
    assert_eq!(output.report.gpu_frame_ledger.readback_ticket_count, 0);
    assert!(output.report.resource_ledger.states.iter().any(|state| {
        state.resource == handle.resource
            && state.residency == EngineResourceResidency::GpuResident
            && state.epoch_state == EngineResourceEpochState::Valid { epoch: 2 }
    }));
}

#[test]
fn engine_frame_resource_ledger_rejects_wrong_epoch_consumers() {
    let mut input = runtime_input();
    input.query_requests = vec![EngineQueryRequest {
        owner: EngineSubsystemKind::Presentation,
        contract_id: "visibility".to_string(),
        query_kind: "surface.sample".to_string(),
        required_this_tick: true,
    }];
    let mut runtime = EngineFrameRuntime::new(Box::new(NoopStateAdvanceExecutor));
    let output = runtime
        .run_frame(input)
        .expect("runtime frame should publish resource states");
    let query_buffer = output.query_results.resident_handles[0].resource.clone();

    assert!(
        output
            .report
            .resource_ledger
            .is_valid_for_epoch(&query_buffer, 2)
    );
    assert!(
        !output
            .report
            .resource_ledger
            .is_valid_for_epoch(&query_buffer, 1)
    );
}

#[test]
fn engine_frame_runtime_records_deferred_readback_tickets() {
    let mut input = runtime_input();
    input.readback_requests.push(EngineReadbackRequest {
        owner: EngineSubsystemKind::Query,
        reason: "debug-sampled-query".to_string(),
        category: EngineReadbackCategory::Oracle,
        bytes: 32,
        required_for_frame_completion: false,
    });
    let mut runtime = EngineFrameRuntime::new(Box::new(NoopStateAdvanceExecutor));
    let output = runtime
        .run_frame(input)
        .expect("deferred oracle readback should be ticketed");

    assert_eq!(output.report.readback_ledger.tickets.len(), 1);
    let ticket = &output.report.readback_ledger.tickets[0];
    assert_eq!(ticket.owner, EngineSubsystemKind::Query);
    assert_eq!(ticket.reason, "debug-sampled-query");
    assert_eq!(ticket.bytes, 32);
    assert_eq!(ticket.snapshot_epoch, 2);
    assert_eq!(ticket.readiness, EngineReadbackReadiness::Deferred);
    assert_eq!(output.report.gpu_frame_ledger.readback_ticket_count, 1);
}

#[test]
fn engine_frame_runtime_rejects_attachment_cpu_bounce_in_closure_policy() {
    let mut input = runtime_input();
    input.readback_requests.push(EngineReadbackRequest {
        owner: EngineSubsystemKind::Presentation,
        reason: "color-attachment-cpu-bounce".to_string(),
        category: EngineReadbackCategory::AttachmentCpuBounce,
        bytes: 256,
        required_for_frame_completion: true,
    });
    let mut runtime = EngineFrameRuntime::new(Box::new(NoopStateAdvanceExecutor));
    let error = runtime
        .run_frame(input)
        .expect_err("closure policy should reject attachment CPU bounce");
    assert!(format!("{error}").contains("attachment CPU bounce rejected"));
}

#[test]
fn engine_readback_manager_is_the_policy_authority_for_deferred_tickets() {
    let manager = EngineReadbackManager::new(EngineFrameRuntimePolicy::closure());
    let ledger = manager
        .register_frame_readbacks(
            9,
            vec![EngineReadbackRequest {
                owner: EngineSubsystemKind::Collision,
                reason: "collision-audit-sample".to_string(),
                category: EngineReadbackCategory::Oracle,
                bytes: 48,
                required_for_frame_completion: false,
            }],
        )
        .expect("oracle readback should be deferred under closure policy");

    assert_eq!(ledger.accepted.len(), 1);
    assert_eq!(ledger.tickets.len(), 1);
    assert_eq!(ledger.tickets[0].snapshot_epoch, 9);
    assert_eq!(
        ledger.tickets[0].readiness,
        EngineReadbackReadiness::Deferred
    );
}

#[test]
fn engine_frame_runtime_rejects_subsystem_reported_hot_path_readbacks() {
    struct HotReadbackAdapter;

    impl EngineSubsystemAdapter for HotReadbackAdapter {
        fn build(
            &mut self,
            builder: &mut EngineGraphBuilder,
        ) -> Result<EngineSubsystemPlan, EngineFrameError> {
            let descriptor = EngineSubsystemDescriptor {
                kind: EngineSubsystemKind::Collision,
                label: "collision".to_string(),
                runs_after: vec![EngineSubsystemKind::StateAdvance],
                requires_gpu: true,
                allows_hot_path_readback: false,
            };
            let job = builder.add_synthetic_job(
                descriptor.kind.clone(),
                "collision.execute".to_string(),
                EngineJobAffinity::Gpu,
                EngineSpanDomain::Gpu,
                Vec::new(),
                true,
                1,
            );
            Ok(EngineSubsystemPlan::new(
                descriptor.clone(),
                vec![job],
                vec![job],
                move |_timeline: &EngineFrameTimeline, _ctx: &mut EngineFrameContext| {
                    Ok(EngineSubsystemReport {
                        kind: descriptor.kind.clone(),
                        label: descriptor.label.clone(),
                        work_items: 1,
                        cpu_critical_path_micros: 1,
                        gpu_critical_path_micros: Some(1),
                        executed_wall_time_micros: 1,
                        self_reported_runtime_micros: Some(1),
                        orchestration_gap_micros: 0,
                        measurement_policy: timeline_measurement_policy(),
                        queue_submit_count: 1,
                        hot_path_readback_bytes: 4,
                        scene_reupload_bytes: 0,
                        timestamped_pass_count: 0,
                        timing_readback_bytes: 0,
                        wait_time_micros: 0,
                        notes: Vec::new(),
                    })
                },
            ))
        }
    }

    let mut runtime = EngineFrameRuntime::new(Box::new(NoopStateAdvanceExecutor));
    let error = runtime
        .run_frame_with_subsystems(runtime_input(), vec![Box::new(HotReadbackAdapter)])
        .expect_err("closure policy should reject subsystem-reported readbacks");

    assert!(format!("{error}").contains("hot-path readback rejected"));
}

#[test]
fn engine_frame_runtime_runs_kernel_subsystems_after_authoritative_state_advance() {
    struct QueryKernelAdapter {
        log: Arc<Mutex<Vec<String>>>,
    }

    impl EngineSubsystemAdapter for QueryKernelAdapter {
        fn build(
            &mut self,
            builder: &mut EngineGraphBuilder,
        ) -> Result<EngineSubsystemPlan, EngineFrameError> {
            let descriptor = EngineSubsystemDescriptor {
                kind: EngineSubsystemKind::Query,
                label: "query".to_string(),
                runs_after: vec![EngineSubsystemKind::StateAdvance],
                requires_gpu: true,
                allows_hot_path_readback: false,
            };
            let log = Arc::clone(&self.log);
            let job = builder.add_job(
                descriptor.kind.clone(),
                "query.execute".to_string(),
                EngineJobAffinity::Gpu,
                EngineSpanDomain::Gpu,
                Vec::new(),
                true,
                move || {
                    log.lock().expect("log").push("query".to_string());
                    Ok(())
                },
            );
            Ok(EngineSubsystemPlan::new(
                descriptor.clone(),
                vec![job],
                vec![job],
                move |_timeline: &EngineFrameTimeline, ctx: &mut EngineFrameContext| {
                    if ctx.published_snapshot_epoch != Some(2) {
                        return Err(EngineFrameError::Message(format!(
                            "query did not receive published snapshot epoch: {:?}",
                            ctx.published_snapshot_epoch
                        )));
                    }
                    Ok(EngineSubsystemReport {
                        kind: descriptor.kind.clone(),
                        label: descriptor.label.clone(),
                        work_items: 1,
                        cpu_critical_path_micros: 0,
                        gpu_critical_path_micros: Some(0),
                        executed_wall_time_micros: 0,
                        self_reported_runtime_micros: Some(0),
                        orchestration_gap_micros: 0,
                        measurement_policy: timeline_measurement_policy(),
                        queue_submit_count: 0,
                        hot_path_readback_bytes: 0,
                        scene_reupload_bytes: 0,
                        timestamped_pass_count: 0,
                        timing_readback_bytes: 0,
                        wait_time_micros: 0,
                        notes: vec![
                            "kernel-owned".to_string(),
                            "consumed_snapshot_epoch=2".to_string(),
                        ],
                    })
                },
            ))
        }
    }

    let log = Arc::new(Mutex::new(Vec::new()));
    let mut runtime = EngineFrameRuntime::new(Box::new(NoopStateAdvanceExecutor));
    let output = runtime
        .run_frame_with_subsystems(
            runtime_input(),
            vec![Box::new(QueryKernelAdapter {
                log: Arc::clone(&log),
            })],
        )
        .expect("runtime should execute state advance plus kernel subsystem");

    assert_eq!(log.lock().expect("log").as_slice(), ["query"]);
    assert!(
        output
            .report
            .subsystem(EngineSubsystemKind::StateAdvance)
            .is_some()
    );
    assert!(
        output
            .report
            .subsystem(EngineSubsystemKind::Query)
            .is_some()
    );
    assert_eq!(output.report.identity.output_snapshot_epoch, Some(2));
    assert!(output.report.resource_ledger.accesses.iter().any(|access| {
        access.subsystem == EngineSubsystemKind::Query
            && access.resource == (EngineResourceId::WorldSnapshot { epoch: 2 })
            && access.mode == EngineResourceAccessMode::Read
    }));
}

#[test]
fn engine_frame_report_round_trips_through_json() {
    let report = EngineFrameReport {
        scenario_id: "closure_fixture".to_string(),
        frame_index: 3,
        identity: Default::default(),
        state_advance: None,
        resource_ledger: Default::default(),
        readback_ledger: Default::default(),
        query_ledger: Default::default(),
        gpu_frame_ledger: Default::default(),
        budget_directives: Default::default(),
        frame_wall_time_micros: 8_100,
        cpu_critical_path_micros: 5_400,
        gpu_critical_path_micros: Some(3_200),
        present_wait_micros: 120,
        gpu_wait_micros: 90,
        readback_wait_micros: 0,
        steady_state_fps: 123.4,
        gpu_runtime: GpuRuntimeMetrics {
            queue_submit_count: 1,
            readback_bytes: 0,
            upload_bytes: 128,
            ..GpuRuntimeMetrics::default()
        },
        timeline_version: 2,
        critical_path_span_ids: vec![EngineSpanId(0), EngineSpanId(2)],
        cpu_busy_micros: 5_800,
        gpu_busy_micros: 3_500,
        overlap_ratio: 0.42,
        queue_submission_spans: vec![EngineSpanId(1)],
        subsystem_span_ranges: vec![
            EngineSubsystemSpanRange {
                kind: EngineSubsystemKind::Presentation,
                start_span_id: Some(EngineSpanId(0)),
                end_span_id: Some(EngineSpanId(1)),
            },
            EngineSubsystemSpanRange {
                kind: EngineSubsystemKind::Collision,
                start_span_id: Some(EngineSpanId(2)),
                end_span_id: Some(EngineSpanId(2)),
            },
        ],
        timeline_spans: Vec::new(),
        subsystems: vec![
            EngineSubsystemReport {
                kind: EngineSubsystemKind::Presentation,
                label: "presentation".into(),
                work_items: 7,
                cpu_critical_path_micros: 4_000,
                gpu_critical_path_micros: Some(3_900),
                executed_wall_time_micros: 4_000,
                self_reported_runtime_micros: Some(3_920),
                orchestration_gap_micros: 80,
                measurement_policy: timeline_measurement_policy(),
                queue_submit_count: 1,
                hot_path_readback_bytes: 0,
                scene_reupload_bytes: 0,
                timestamped_pass_count: 0,
                timing_readback_bytes: 0,
                wait_time_micros: 80,
                notes: vec!["framegraph".to_string()],
            },
            EngineSubsystemReport {
                kind: EngineSubsystemKind::Collision,
                label: "collision".into(),
                work_items: 7200,
                cpu_critical_path_micros: 800,
                gpu_critical_path_micros: Some(600),
                executed_wall_time_micros: 800,
                self_reported_runtime_micros: Some(790),
                orchestration_gap_micros: 10,
                measurement_policy: timeline_measurement_policy(),
                queue_submit_count: 0,
                hot_path_readback_bytes: 0,
                scene_reupload_bytes: 0,
                timestamped_pass_count: 0,
                timing_readback_bytes: 0,
                wait_time_micros: 10,
                notes: vec!["timeline-derived".to_string()],
            },
        ],
        future_subsystem_reserve: wrela::engine_frame::EngineFutureReserveReport {
            reserved_micros: 1_000,
            remaining_micros: 230,
            exhausted: false,
        },
        active_degradations: vec!["enable_hit_compaction".to_string()],
        violations: Vec::new(),
        latency: MotionToPhotonContract::default(),
        closure_findings: Vec::new(),
    };

    let json = serde_json::to_string(&report).expect("serialize engine frame report");
    let round_trip: EngineFrameReport =
        serde_json::from_str(&json).expect("deserialize engine frame report");
    assert_eq!(round_trip, report);
}

#[test]
fn engine_frame_scheduler_runs_empty_frame() {
    let mut scheduler = EngineFrameScheduler::default();
    let mut subsystems: Vec<Box<dyn EngineSubsystemAdapter>> = Vec::new();
    let report = scheduler
        .run_frame("empty_fixture", 0, &mut subsystems)
        .expect("empty frame report");

    assert_eq!(report.scenario_id, "empty_fixture");
    assert_eq!(report.frame_index, 0);
    assert!(report.subsystems.is_empty());
    assert_eq!(report.future_subsystem_reserve.reserved_micros, 0);
    assert_eq!(report.timeline_version, 2);
}

#[test]
fn engine_frame_scheduler_reports_panicking_subsystem_tasks_as_errors() {
    struct PanicAdapter;

    impl EngineSubsystemAdapter for PanicAdapter {
        fn build(
            &mut self,
            builder: &mut EngineGraphBuilder,
        ) -> Result<EngineSubsystemPlan, EngineFrameError> {
            let descriptor = EngineSubsystemDescriptor {
                kind: EngineSubsystemKind::Presentation,
                label: "presentation".to_string(),
                runs_after: vec![],
                requires_gpu: false,
                allows_hot_path_readback: false,
            };
            let job = builder.add_job(
                descriptor.kind.clone(),
                "presentation.panic".to_string(),
                EngineJobAffinity::Cpu,
                EngineSpanDomain::Cpu,
                Vec::new(),
                false,
                move || panic!("synthetic subsystem panic"),
            );
            Ok(EngineSubsystemPlan::new(
                descriptor.clone(),
                vec![job],
                vec![job],
                move |_timeline: &EngineFrameTimeline, _ctx: &mut EngineFrameContext| {
                    Ok(EngineSubsystemReport {
                        kind: descriptor.kind.clone(),
                        label: descriptor.label.clone(),
                        work_items: 1,
                        cpu_critical_path_micros: 0,
                        gpu_critical_path_micros: None,
                        executed_wall_time_micros: 0,
                        self_reported_runtime_micros: Some(0),
                        orchestration_gap_micros: 0,
                        measurement_policy: timeline_measurement_policy(),
                        queue_submit_count: 0,
                        hot_path_readback_bytes: 0,
                        scene_reupload_bytes: 0,
                        timestamped_pass_count: 0,
                        timing_readback_bytes: 0,
                        wait_time_micros: 0,
                        notes: Vec::new(),
                    })
                },
            ))
        }
    }

    let mut scheduler = EngineFrameScheduler::with_executor_config(EngineExecutorConfig {
        cpu_worker_threads: 1,
        external_worker_threads: 1,
    });
    let mut subsystems: Vec<Box<dyn EngineSubsystemAdapter>> = vec![Box::new(PanicAdapter)];

    let error = scheduler
        .run_frame("panic_fixture", 0, &mut subsystems)
        .expect_err("panicking subsystem should fail the frame deterministically");
    let message = match error {
        EngineFrameError::Message(message) => message,
        other => panic!("expected EngineFrameError::Message, got {other:?}"),
    };
    assert!(message.contains("presentation.panic"));
    assert!(message.contains("synthetic subsystem panic"));
}

#[test]
fn engine_frame_scheduler_respects_subsystem_dependencies() {
    struct LoggingAdapter {
        descriptor: EngineSubsystemDescriptor,
        log: Arc<Mutex<Vec<String>>>,
    }

    impl EngineSubsystemAdapter for LoggingAdapter {
        fn build(
            &mut self,
            builder: &mut EngineGraphBuilder,
        ) -> Result<EngineSubsystemPlan, EngineFrameError> {
            let descriptor = self.descriptor.clone();
            let label = descriptor.label.clone();
            let log = Arc::clone(&self.log);
            let job = builder.add_job(
                descriptor.kind.clone(),
                format!("{label}.job"),
                EngineJobAffinity::Cpu,
                EngineSpanDomain::Cpu,
                Vec::new(),
                false,
                move || {
                    log.lock().expect("log").push(label.clone());
                    Ok(())
                },
            );
            Ok(EngineSubsystemPlan::new(
                descriptor.clone(),
                vec![job],
                vec![job],
                move |_timeline: &EngineFrameTimeline, _ctx: &mut EngineFrameContext| {
                    Ok(EngineSubsystemReport {
                        kind: descriptor.kind.clone(),
                        label: descriptor.label.clone(),
                        work_items: 1,
                        cpu_critical_path_micros: 0,
                        gpu_critical_path_micros: None,
                        executed_wall_time_micros: 0,
                        self_reported_runtime_micros: Some(0),
                        orchestration_gap_micros: 0,
                        measurement_policy: timeline_measurement_policy(),
                        queue_submit_count: 0,
                        hot_path_readback_bytes: 0,
                        scene_reupload_bytes: 0,
                        timestamped_pass_count: 0,
                        timing_readback_bytes: 0,
                        wait_time_micros: 0,
                        notes: Vec::new(),
                    })
                },
            ))
        }
    }

    let log = Arc::new(Mutex::new(Vec::new()));
    let mut scheduler = EngineFrameScheduler::with_executor_config(EngineExecutorConfig {
        cpu_worker_threads: 1,
        external_worker_threads: 1,
    });
    let mut subsystems: Vec<Box<dyn EngineSubsystemAdapter>> = vec![
        Box::new(LoggingAdapter {
            descriptor: EngineSubsystemDescriptor {
                kind: EngineSubsystemKind::Collision,
                label: "collision".to_string(),
                runs_after: vec![EngineSubsystemKind::Presentation],
                requires_gpu: false,
                allows_hot_path_readback: false,
            },
            log: Arc::clone(&log),
        }),
        Box::new(LoggingAdapter {
            descriptor: EngineSubsystemDescriptor {
                kind: EngineSubsystemKind::Presentation,
                label: "presentation".to_string(),
                runs_after: vec![EngineSubsystemKind::StateAdvance],
                requires_gpu: false,
                allows_hot_path_readback: false,
            },
            log: Arc::clone(&log),
        }),
        Box::new(LoggingAdapter {
            descriptor: EngineSubsystemDescriptor {
                kind: EngineSubsystemKind::StateAdvance,
                label: "state_advance".to_string(),
                runs_after: vec![],
                requires_gpu: false,
                allows_hot_path_readback: false,
            },
            log: Arc::clone(&log),
        }),
    ];

    let report = scheduler
        .run_frame("dependency_fixture", 1, &mut subsystems)
        .expect("dependency-ordered frame");

    assert_eq!(
        report
            .subsystems
            .iter()
            .map(|subsystem| subsystem.label.as_str())
            .collect::<Vec<_>>(),
        vec!["state_advance", "presentation", "collision"]
    );
    assert_eq!(
        log.lock().expect("log").as_slice(),
        ["state_advance", "presentation", "collision"]
    );
}

#[test]
fn engine_frame_scheduler_uses_overlap_aware_wall_time_for_parallel_subsystems() {
    #[derive(Clone)]
    struct SyntheticAdapter {
        descriptor: EngineSubsystemDescriptor,
        cpu_micros: u128,
    }

    impl EngineSubsystemAdapter for SyntheticAdapter {
        fn build(
            &mut self,
            builder: &mut EngineGraphBuilder,
        ) -> Result<EngineSubsystemPlan, EngineFrameError> {
            let descriptor = self.descriptor.clone();
            let cpu_micros = self.cpu_micros;
            let job = builder.add_synthetic_job(
                descriptor.kind.clone(),
                format!("{}.cpu", descriptor.label),
                EngineJobAffinity::Cpu,
                EngineSpanDomain::Cpu,
                Vec::new(),
                false,
                cpu_micros,
            );
            Ok(EngineSubsystemPlan::new(
                descriptor.clone(),
                vec![job],
                vec![job],
                move |_timeline: &EngineFrameTimeline, _ctx: &mut EngineFrameContext| {
                    Ok(EngineSubsystemReport {
                        kind: descriptor.kind.clone(),
                        label: descriptor.label.clone(),
                        work_items: 1,
                        cpu_critical_path_micros: cpu_micros,
                        gpu_critical_path_micros: None,
                        executed_wall_time_micros: cpu_micros,
                        self_reported_runtime_micros: Some(cpu_micros),
                        orchestration_gap_micros: 0,
                        measurement_policy: timeline_measurement_policy(),
                        queue_submit_count: 0,
                        hot_path_readback_bytes: 0,
                        scene_reupload_bytes: 0,
                        timestamped_pass_count: 0,
                        timing_readback_bytes: 0,
                        wait_time_micros: 0,
                        notes: vec!["synthetic".to_string()],
                    })
                },
            ))
        }
    }

    let mut scheduler = EngineFrameScheduler::with_executor_config(EngineExecutorConfig {
        cpu_worker_threads: 2,
        external_worker_threads: 1,
    });
    let mut subsystems: Vec<Box<dyn EngineSubsystemAdapter>> = vec![
        Box::new(SyntheticAdapter {
            descriptor: EngineSubsystemDescriptor {
                kind: EngineSubsystemKind::StateAdvance,
                label: "state_advance".to_string(),
                runs_after: vec![],
                requires_gpu: false,
                allows_hot_path_readback: false,
            },
            cpu_micros: 1_000,
        }),
        Box::new(SyntheticAdapter {
            descriptor: EngineSubsystemDescriptor {
                kind: EngineSubsystemKind::Presentation,
                label: "presentation".to_string(),
                runs_after: vec![EngineSubsystemKind::StateAdvance],
                requires_gpu: false,
                allows_hot_path_readback: false,
            },
            cpu_micros: 4_000,
        }),
        Box::new(SyntheticAdapter {
            descriptor: EngineSubsystemDescriptor {
                kind: EngineSubsystemKind::Collision,
                label: "collision".to_string(),
                runs_after: vec![EngineSubsystemKind::StateAdvance],
                requires_gpu: false,
                allows_hot_path_readback: false,
            },
            cpu_micros: 2_000,
        }),
    ];

    let report = scheduler
        .run_frame("overlap_fixture", 0, &mut subsystems)
        .expect("overlap-aware frame");

    assert!(report.frame_wall_time_micros >= 5_000);
    assert!(report.frame_wall_time_micros < 7_000);
    assert_eq!(report.cpu_critical_path_micros, 5_000);
    assert_eq!(report.cpu_busy_micros, 5_000);
    assert_eq!(report.overlap_ratio, 0.0);
}

#[test]
fn engine_frame_scheduler_uses_overlap_aware_wall_time_for_parallel_external_subsystems() {
    #[derive(Clone)]
    struct SyntheticAdapter {
        descriptor: EngineSubsystemDescriptor,
        elapsed_micros: u128,
    }

    impl EngineSubsystemAdapter for SyntheticAdapter {
        fn build(
            &mut self,
            builder: &mut EngineGraphBuilder,
        ) -> Result<EngineSubsystemPlan, EngineFrameError> {
            let descriptor = self.descriptor.clone();
            let elapsed_micros = self.elapsed_micros;
            let job = builder.add_synthetic_job(
                descriptor.kind.clone(),
                format!("{}.external", descriptor.label),
                EngineJobAffinity::External,
                EngineSpanDomain::External,
                Vec::new(),
                false,
                elapsed_micros,
            );
            Ok(EngineSubsystemPlan::new(
                descriptor.clone(),
                vec![job],
                vec![job],
                move |_timeline: &EngineFrameTimeline, _ctx: &mut EngineFrameContext| {
                    Ok(EngineSubsystemReport {
                        kind: descriptor.kind.clone(),
                        label: descriptor.label.clone(),
                        work_items: 1,
                        cpu_critical_path_micros: elapsed_micros,
                        gpu_critical_path_micros: None,
                        executed_wall_time_micros: elapsed_micros,
                        self_reported_runtime_micros: Some(elapsed_micros),
                        orchestration_gap_micros: 0,
                        measurement_policy: timeline_measurement_policy(),
                        queue_submit_count: 0,
                        hot_path_readback_bytes: 0,
                        scene_reupload_bytes: 0,
                        timestamped_pass_count: 0,
                        timing_readback_bytes: 0,
                        wait_time_micros: 0,
                        notes: vec!["synthetic-external".to_string()],
                    })
                },
            ))
        }
    }

    let mut scheduler = EngineFrameScheduler::with_executor_config(EngineExecutorConfig {
        cpu_worker_threads: 1,
        external_worker_threads: 2,
    });
    let mut subsystems: Vec<Box<dyn EngineSubsystemAdapter>> = vec![
        Box::new(SyntheticAdapter {
            descriptor: EngineSubsystemDescriptor {
                kind: EngineSubsystemKind::StateAdvance,
                label: "state_advance".to_string(),
                runs_after: vec![],
                requires_gpu: false,
                allows_hot_path_readback: false,
            },
            elapsed_micros: 1_000,
        }),
        Box::new(SyntheticAdapter {
            descriptor: EngineSubsystemDescriptor {
                kind: EngineSubsystemKind::Presentation,
                label: "presentation".to_string(),
                runs_after: vec![EngineSubsystemKind::StateAdvance],
                requires_gpu: true,
                allows_hot_path_readback: false,
            },
            elapsed_micros: 4_000,
        }),
        Box::new(SyntheticAdapter {
            descriptor: EngineSubsystemDescriptor {
                kind: EngineSubsystemKind::Collision,
                label: "collision".to_string(),
                runs_after: vec![EngineSubsystemKind::StateAdvance],
                requires_gpu: true,
                allows_hot_path_readback: false,
            },
            elapsed_micros: 2_000,
        }),
    ];

    let report = scheduler
        .run_frame("external_overlap_fixture", 0, &mut subsystems)
        .expect("overlap-aware external frame");

    assert_eq!(report.frame_wall_time_micros, 5_000);
    assert_eq!(report.cpu_critical_path_micros, 5_000);
    assert_eq!(report.cpu_busy_micros, 5_000);
}

#[test]
fn engine_frame_scheduler_records_parallel_cpu_gpu_busy_windows() {
    struct ParallelSubsystemAdapter;

    impl EngineSubsystemAdapter for ParallelSubsystemAdapter {
        fn build(
            &mut self,
            builder: &mut EngineGraphBuilder,
        ) -> Result<EngineSubsystemPlan, EngineFrameError> {
            let cpu = builder.add_synthetic_job(
                EngineSubsystemKind::Presentation,
                "presentation.cpu",
                EngineJobAffinity::Cpu,
                EngineSpanDomain::Cpu,
                Vec::new(),
                false,
                4_000,
            );
            let gpu = builder.add_synthetic_job(
                EngineSubsystemKind::Presentation,
                "presentation.gpu",
                EngineJobAffinity::Gpu,
                EngineSpanDomain::Gpu,
                Vec::new(),
                true,
                3_000,
            );
            Ok(EngineSubsystemPlan::new(
                EngineSubsystemDescriptor {
                    kind: EngineSubsystemKind::Presentation,
                    label: "presentation".to_string(),
                    runs_after: vec![],
                    requires_gpu: true,
                    allows_hot_path_readback: false,
                },
                vec![cpu, gpu],
                vec![cpu, gpu],
                |_timeline: &EngineFrameTimeline, _ctx: &mut EngineFrameContext| {
                    Ok(EngineSubsystemReport {
                        kind: EngineSubsystemKind::Presentation,
                        label: "presentation".to_string(),
                        work_items: 1,
                        cpu_critical_path_micros: 4_000,
                        gpu_critical_path_micros: Some(3_000),
                        executed_wall_time_micros: 4_000,
                        self_reported_runtime_micros: Some(4_000),
                        orchestration_gap_micros: 0,
                        measurement_policy: EngineMeasurementPolicy {
                            gpu_timing: EngineGpuTimingPolicy::Timestamped,
                            ..timeline_measurement_policy()
                        },
                        queue_submit_count: 1,
                        hot_path_readback_bytes: 0,
                        scene_reupload_bytes: 0,
                        timestamped_pass_count: 1,
                        timing_readback_bytes: 16,
                        wait_time_micros: 0,
                        notes: vec!["parallel-cpu-gpu".to_string()],
                    })
                },
            ))
        }
    }

    let mut scheduler = EngineFrameScheduler::with_executor_config(EngineExecutorConfig {
        cpu_worker_threads: 2,
        external_worker_threads: 1,
    });
    let mut subsystems: Vec<Box<dyn EngineSubsystemAdapter>> =
        vec![Box::new(ParallelSubsystemAdapter)];
    let report = scheduler
        .run_frame("cpu_gpu_fixture", 0, &mut subsystems)
        .expect("parallel cpu/gpu frame");

    assert!(report.frame_wall_time_micros >= 4_000);
    assert!(report.frame_wall_time_micros < 6_000);
    assert!(report.gpu_busy_micros >= 3_000);
    assert!(report.overlap_ratio > 0.5);
    assert_eq!(report.queue_submission_spans.len(), 1);
}

#[test]
fn engine_frame_scheduler_records_budget_violations_and_reserve_accounting() {
    #[derive(Clone)]
    struct BudgetAdapter {
        descriptor: EngineSubsystemDescriptor,
        cpu_micros: u128,
        gpu_micros: Option<u128>,
        queue_submit_count: u32,
        hot_path_readback_bytes: u64,
        degradations: Vec<String>,
    }

    impl EngineSubsystemAdapter for BudgetAdapter {
        fn build(
            &mut self,
            builder: &mut EngineGraphBuilder,
        ) -> Result<EngineSubsystemPlan, EngineFrameError> {
            let descriptor = self.descriptor.clone();
            let cpu_job = builder.add_synthetic_job(
                descriptor.kind.clone(),
                format!("{}.cpu", descriptor.label),
                EngineJobAffinity::Cpu,
                EngineSpanDomain::Cpu,
                Vec::new(),
                false,
                self.cpu_micros,
            );
            let gpu_job = self.gpu_micros.map(|gpu_micros| {
                builder.add_synthetic_job(
                    descriptor.kind.clone(),
                    format!("{}.gpu", descriptor.label),
                    EngineJobAffinity::Gpu,
                    EngineSpanDomain::Gpu,
                    Vec::new(),
                    self.queue_submit_count > 0,
                    gpu_micros,
                )
            });
            let root_jobs = gpu_job
                .iter()
                .copied()
                .chain(std::iter::once(cpu_job))
                .collect::<Vec<_>>();
            let terminal_jobs = root_jobs.clone();
            let degradations = self.degradations.clone();
            let queue_submit_count = self.queue_submit_count;
            let hot_path_readback_bytes = self.hot_path_readback_bytes;
            let cpu_micros = self.cpu_micros;
            let gpu_micros = self.gpu_micros;
            Ok(EngineSubsystemPlan::new(
                descriptor.clone(),
                root_jobs,
                terminal_jobs,
                move |_timeline: &EngineFrameTimeline, ctx: &mut EngineFrameContext| {
                    for degradation in &degradations {
                        if !ctx.active_degradations.contains(degradation) {
                            ctx.active_degradations.push(degradation.clone());
                        }
                    }
                    Ok(EngineSubsystemReport {
                        kind: descriptor.kind.clone(),
                        label: descriptor.label.clone(),
                        work_items: 1,
                        cpu_critical_path_micros: cpu_micros,
                        gpu_critical_path_micros: gpu_micros,
                        executed_wall_time_micros: cpu_micros,
                        self_reported_runtime_micros: Some(cpu_micros),
                        orchestration_gap_micros: 0,
                        measurement_policy: timeline_measurement_policy(),
                        queue_submit_count,
                        hot_path_readback_bytes,
                        scene_reupload_bytes: 0,
                        timestamped_pass_count: 0,
                        timing_readback_bytes: 0,
                        wait_time_micros: 0,
                        notes: Vec::new(),
                    })
                },
            ))
        }
    }

    let mut scheduler = EngineFrameScheduler::with_executor_config(EngineExecutorConfig {
        cpu_worker_threads: 2,
        external_worker_threads: 1,
    });
    scheduler.budget =
        Some(wrela::perf_target::PerfClosureProfile::canonical_1080p120().engine_frame_budget);
    let mut subsystems: Vec<Box<dyn EngineSubsystemAdapter>> = vec![
        Box::new(BudgetAdapter {
            descriptor: EngineSubsystemDescriptor {
                kind: EngineSubsystemKind::StateAdvance,
                label: "state_advance".to_string(),
                runs_after: vec![],
                requires_gpu: false,
                allows_hot_path_readback: false,
            },
            cpu_micros: 500,
            gpu_micros: None,
            queue_submit_count: 0,
            hot_path_readback_bytes: 0,
            degradations: vec![],
        }),
        Box::new(BudgetAdapter {
            descriptor: EngineSubsystemDescriptor {
                kind: EngineSubsystemKind::Presentation,
                label: "presentation".to_string(),
                runs_after: vec![EngineSubsystemKind::StateAdvance],
                requires_gpu: true,
                allows_hot_path_readback: false,
            },
            cpu_micros: 7_000,
            gpu_micros: Some(6_000),
            queue_submit_count: 1,
            hot_path_readback_bytes: 0,
            degradations: vec!["enable_hit_compaction".to_string()],
        }),
        Box::new(BudgetAdapter {
            descriptor: EngineSubsystemDescriptor {
                kind: EngineSubsystemKind::Collision,
                label: "collision".to_string(),
                runs_after: vec![EngineSubsystemKind::Presentation],
                requires_gpu: true,
                allows_hot_path_readback: false,
            },
            cpu_micros: 1_500,
            gpu_micros: Some(1_000),
            queue_submit_count: 2,
            hot_path_readback_bytes: 16,
            degradations: vec![],
        }),
    ];

    let report = scheduler
        .run_frame("budget_fixture", 0, &mut subsystems)
        .expect("budgeted frame report");

    assert_eq!(report.future_subsystem_reserve.reserved_micros, 1_000);
    assert!(report.future_subsystem_reserve.exhausted);
    assert!(report.future_subsystem_reserve.remaining_micros < 0);
    assert!(
        report
            .active_degradations
            .contains(&"enable_hit_compaction".to_string())
    );
    assert!(
        report
            .violations
            .contains(&"engine_frame_hot_path_readback_budget_exceeded".to_string())
    );
    assert!(
        report
            .violations
            .contains(&"engine_frame_queue_submit_budget_exceeded".to_string())
    );
    assert!(
        report
            .violations
            .contains(&"engine_frame_future_reserve_exhausted".to_string())
    );
    assert_eq!(report.gpu_frame_ledger.scheduler_owned_queue_submits, 2);
    assert_eq!(report.gpu_frame_ledger.private_queue_submits, 1);
    assert!(
        report
            .gpu_frame_ledger
            .violations
            .contains(&"engine_frame_private_gpu_submit_detected".to_string())
    );
}
