use super::{
    EngineFrameError, EngineFrameReport, EngineFrameScheduler, EngineFrameTimeline,
    EngineGpuTimingPolicy, EngineGraphBuilder, EngineJobAffinity, EngineMeasurementPolicy,
    EngineRuntimeSource, EngineSpanDomain, EngineSubsystemAdapter, EngineSubsystemDescriptor,
    EngineSubsystemKind, EngineSubsystemPlan, EngineSubsystemReport,
};
use crate::perf_target::PerfClosureEngineFrameBudget;
use crate::state_advance::{ChangeClass, StateAdvanceResult, TickInputBatch};
use crate::time_semantics::TemporalClock;
use crate::world_identity::WorldSnapshotHandle;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct EngineFrameInput {
    pub scenario_id: String,
    pub frame_index: u32,
    pub previous_snapshot: WorldSnapshotHandle,
    pub previous_clock: TemporalClock,
    pub current_clock: TemporalClock,
    pub tick_inputs: TickInputBatch,
    pub policy: EngineFrameRuntimePolicy,
    pub query_requests: Vec<EngineQueryRequest>,
    pub readback_requests: Vec<EngineReadbackRequest>,
}

#[derive(Debug, Clone)]
pub struct EngineFrameOutput {
    pub snapshot: WorldSnapshotHandle,
    pub query_results: EngineQueryResults,
    pub report: EngineFrameReport,
}

#[derive(Debug, Clone)]
pub struct EngineStateAdvanceInput {
    pub previous_snapshot: WorldSnapshotHandle,
    pub previous_clock: TemporalClock,
    pub current_clock: TemporalClock,
    pub inputs: TickInputBatch,
}

pub trait EngineStateAdvanceExecutor: Send {
    fn advance(
        &mut self,
        input: EngineStateAdvanceInput,
    ) -> Result<StateAdvanceResult, EngineFrameError>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct EngineFrameRuntimePolicy {
    pub allow_hot_path_gameplay_readbacks: bool,
    pub allow_debug_export_readbacks: bool,
    pub allow_private_gpu_submits: bool,
    pub max_change_class: ChangeClass,
    pub budget: Option<PerfClosureEngineFrameBudget>,
}

impl EngineFrameRuntimePolicy {
    pub fn closure() -> Self {
        Self {
            allow_hot_path_gameplay_readbacks: false,
            allow_debug_export_readbacks: false,
            allow_private_gpu_submits: false,
            max_change_class: ChangeClass::Identity,
            budget: None,
        }
    }
}

impl Default for EngineFrameRuntimePolicy {
    fn default() -> Self {
        Self::closure()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EngineFrameIdentityReport {
    pub frame_index: u32,
    pub simulation_tick: u64,
    pub presentation_frame: u64,
    pub input_snapshot_epoch: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_snapshot_epoch: Option<u64>,
    pub wall_clock: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineStateAdvanceReport {
    pub input_count: usize,
    pub identity_event_count: usize,
    pub from_snapshot_epoch: u64,
    pub to_snapshot_epoch: u64,
    pub change_class: String,
    pub accepted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineResourceId {
    WorldSnapshot { epoch: u64 },
    ResidentScene { epoch: u64 },
    DynamicStateBuffer { epoch: u64 },
    Transient(String),
    PresentationAttachment(String),
    CollisionOutput(String),
    QueryBuffer(String),
    ReadbackBuffer(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineResourceAccessMode {
    Read,
    Write,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineResourceAccess {
    pub subsystem: EngineSubsystemKind,
    pub resource: EngineResourceId,
    pub mode: EngineResourceAccessMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineResourceResidency {
    CpuAuthoritative,
    GpuResident,
    Shared,
    DeferredReadback,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineResourceEpochState {
    Uninitialized,
    Valid { epoch: u64 },
    Updating { from_epoch: u64, to_epoch: u64 },
    Invalidated { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineResourceState {
    pub resource: EngineResourceId,
    pub residency: EngineResourceResidency,
    pub epoch_state: EngineResourceEpochState,
    pub producer: EngineSubsystemKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EngineResourceLedger {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub accesses: Vec<EngineResourceAccess>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub states: Vec<EngineResourceState>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub violations: Vec<String>,
}

impl EngineResourceLedger {
    pub fn is_valid_for_epoch(&self, resource: &EngineResourceId, epoch: u64) -> bool {
        self.states.iter().any(|state| {
            &state.resource == resource
                && matches!(
                    state.epoch_state,
                    EngineResourceEpochState::Valid { epoch: valid_epoch }
                        if valid_epoch == epoch
                )
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineReadbackCategory {
    Gameplay,
    Timing,
    DebugExport,
    AttachmentCpuBounce,
    Oracle,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineReadbackRequest {
    pub owner: EngineSubsystemKind,
    pub reason: String,
    pub category: EngineReadbackCategory,
    pub bytes: u64,
    pub required_for_frame_completion: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineReadbackReadiness {
    Deferred,
    TimingOnly,
    Ready,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineReadbackTicketReport {
    pub owner: EngineSubsystemKind,
    pub reason: String,
    pub category: EngineReadbackCategory,
    pub bytes: u64,
    pub snapshot_epoch: u64,
    pub readiness: EngineReadbackReadiness,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EngineReadbackLedger {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub accepted: Vec<EngineReadbackRequest>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rejected: Vec<EngineReadbackRequest>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tickets: Vec<EngineReadbackTicketReport>,
}

#[derive(Debug, Clone)]
pub struct EngineReadbackManager {
    policy: EngineFrameRuntimePolicy,
}

impl EngineReadbackManager {
    pub fn new(policy: EngineFrameRuntimePolicy) -> Self {
        Self { policy }
    }

    pub fn register_frame_readbacks(
        &self,
        output_epoch: u64,
        requests: Vec<EngineReadbackRequest>,
    ) -> Result<EngineReadbackLedger, EngineFrameError> {
        self.validate_requests(&requests)?;
        Ok(build_readback_ledger(output_epoch, requests))
    }

    fn validate_requests(
        &self,
        requests: &[EngineReadbackRequest],
    ) -> Result<(), EngineFrameError> {
        validate_readback_policy(&self.policy, requests)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineQueryRequest {
    pub owner: EngineSubsystemKind,
    pub contract_id: String,
    pub query_kind: String,
    pub required_this_tick: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineQueryBatchReport {
    pub snapshot_epoch: u64,
    pub contract_id: String,
    pub query_kind: String,
    pub required_this_tick: bool,
    pub request_count: usize,
    pub owners: Vec<EngineSubsystemKind>,
    pub resident: bool,
    pub value_readback_scheduled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineQueryResidentHandle {
    pub resource: EngineResourceId,
    pub snapshot_epoch: u64,
    pub residency: EngineResourceResidency,
    pub value_readback_scheduled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EngineQueryLedger {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub batches: Vec<EngineQueryBatchReport>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EngineQueryResults {
    pub batches: Vec<EngineQueryBatchReport>,
    pub resident_handles: Vec<EngineQueryResidentHandle>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EngineGpuFrameLedger {
    pub scheduler_owned_queue_submits: u32,
    pub private_queue_submits: u32,
    pub resident_cache_hits: u32,
    pub resident_cache_misses: u32,
    pub upload_bytes: u64,
    pub readback_ticket_count: u32,
    pub attachment_cpu_bounce_count: u32,
    pub cpu_screen_sample_allocations: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub violations: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct EngineGpuFrameContext {
    ledger: EngineGpuFrameLedger,
}

impl EngineGpuFrameContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn note_scheduler_owned_submit(&mut self) {
        self.ledger.scheduler_owned_queue_submits =
            self.ledger.scheduler_owned_queue_submits.saturating_add(1);
    }

    pub fn note_private_submit(&mut self) {
        self.ledger.private_queue_submits = self.ledger.private_queue_submits.saturating_add(1);
        self.ledger
            .violations
            .push("engine_frame_private_gpu_submit_detected".to_string());
    }

    pub fn note_resident_cache_hit(&mut self) {
        self.ledger.resident_cache_hits = self.ledger.resident_cache_hits.saturating_add(1);
    }

    pub fn note_resident_cache_miss(&mut self) {
        self.ledger.resident_cache_misses = self.ledger.resident_cache_misses.saturating_add(1);
    }

    pub fn note_upload_bytes(&mut self, bytes: u64) {
        self.ledger.upload_bytes = self.ledger.upload_bytes.saturating_add(bytes);
    }

    pub fn note_readback_ticket(&mut self) {
        self.ledger.readback_ticket_count = self.ledger.readback_ticket_count.saturating_add(1);
    }

    pub fn note_attachment_cpu_bounce(&mut self) {
        self.ledger.attachment_cpu_bounce_count =
            self.ledger.attachment_cpu_bounce_count.saturating_add(1);
    }

    pub fn note_cpu_screen_sample_allocation(&mut self) {
        self.ledger.cpu_screen_sample_allocations =
            self.ledger.cpu_screen_sample_allocations.saturating_add(1);
    }

    pub fn into_ledger(self) -> EngineGpuFrameLedger {
        self.ledger
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct EngineBudgetDirectives {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame_wall_time_budget_ms: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub future_reserve_ms: Option<f32>,
    pub hot_path_readbacks_allowed: bool,
    pub private_gpu_submits_allowed: bool,
}

pub struct EngineFrameRuntime {
    state_advance: Arc<Mutex<Box<dyn EngineStateAdvanceExecutor>>>,
    scheduler: EngineFrameScheduler,
}

impl EngineFrameRuntime {
    pub fn new(state_advance: Box<dyn EngineStateAdvanceExecutor>) -> Self {
        Self {
            state_advance: Arc::new(Mutex::new(state_advance)),
            scheduler: EngineFrameScheduler::default(),
        }
    }

    pub fn with_executor_config(
        state_advance: Box<dyn EngineStateAdvanceExecutor>,
        executor_config: wrela_runtime::engine_executor::EngineExecutorConfig,
    ) -> Self {
        Self {
            state_advance: Arc::new(Mutex::new(state_advance)),
            scheduler: EngineFrameScheduler::with_executor_config(executor_config),
        }
    }

    pub fn run_frame(
        &mut self,
        input: EngineFrameInput,
    ) -> Result<EngineFrameOutput, EngineFrameError> {
        self.run_frame_with_subsystems(input, Vec::new())
    }

    pub fn run_frame_with_subsystems(
        &mut self,
        input: EngineFrameInput,
        mut subsystem_adapters: Vec<Box<dyn EngineSubsystemAdapter>>,
    ) -> Result<EngineFrameOutput, EngineFrameError> {
        let readback_manager = EngineReadbackManager::new(input.policy.clone());
        readback_manager.validate_requests(&input.readback_requests)?;
        self.scheduler.budget = input.policy.budget.clone();

        let state_input = EngineStateAdvanceInput {
            previous_snapshot: input.previous_snapshot.clone(),
            previous_clock: input.previous_clock,
            current_clock: input.current_clock,
            inputs: input.tick_inputs.clone(),
        };
        let state_result = Arc::new(Mutex::new(None));
        let mut adapters: Vec<Box<dyn EngineSubsystemAdapter>> =
            vec![Box::new(StateAdvanceRuntimeAdapter {
                executor: Arc::clone(&self.state_advance),
                input: state_input,
                result: Arc::clone(&state_result),
            })];
        adapters.append(&mut subsystem_adapters);
        let mut report = self.scheduler.run_frame(
            input.scenario_id.clone(),
            input.frame_index,
            &mut adapters,
        )?;
        let state_advance = state_result
            .lock()
            .map_err(|_| EngineFrameError::Message("state advance result lock poisoned".into()))?
            .clone()
            .ok_or_else(|| {
                EngineFrameError::Message("state advance did not publish a result".into())
            })??;

        validate_state_advance(&input, &state_advance)?;
        if !input
            .policy
            .max_change_class
            .allows(state_advance.change_summary.class)
        {
            return Err(EngineFrameError::Message(format!(
                "state advance change is incompatible with frame policy: {:?}",
                state_advance.change_summary.class
            )));
        }

        let snapshot = state_advance.transition_record.to_snapshot.clone();
        let output_epoch = snapshot.epoch().0;
        let identity = EngineFrameIdentityReport {
            frame_index: input.frame_index,
            simulation_tick: input.current_clock.simulation_tick.get(),
            presentation_frame: input.current_clock.presentation_frame.get(),
            input_snapshot_epoch: input.previous_snapshot.epoch().0,
            output_snapshot_epoch: Some(output_epoch),
            wall_clock: input.current_clock.wall_clock.get(),
        };
        let state_report = EngineStateAdvanceReport {
            input_count: state_advance.transition_record.inputs.inputs.len(),
            identity_event_count: state_advance.transition_record.identity_events.len(),
            from_snapshot_epoch: input.previous_snapshot.epoch().0,
            to_snapshot_epoch: output_epoch,
            change_class: change_class_label(state_advance.change_summary.class).to_string(),
            accepted: true,
        };
        let query_results =
            EngineQueryService::default().execute(output_epoch, input.query_requests);
        let resource_ledger = build_resource_ledger(
            &input.previous_snapshot,
            output_epoch,
            &query_results,
            &report,
        );
        let mut readback_requests = input.readback_requests;
        readback_requests.extend(subsystem_reported_readback_requests(&report));
        let readback_ledger =
            readback_manager.register_frame_readbacks(output_epoch, readback_requests)?;
        let mut gpu_frame_ledger = report.gpu_frame_ledger.clone();
        gpu_frame_ledger.resident_cache_misses = gpu_frame_ledger
            .resident_cache_misses
            .saturating_add(query_results.resident_handles.len() as u32);
        gpu_frame_ledger.readback_ticket_count = gpu_frame_ledger
            .readback_ticket_count
            .saturating_add(readback_ledger.tickets.len() as u32);
        validate_gpu_frame_policy(&input.policy, gpu_frame_ledger.private_queue_submits)?;
        report.identity = identity;
        report.state_advance = Some(state_report);
        report.resource_ledger = resource_ledger;
        report.readback_ledger = readback_ledger;
        report.query_ledger = EngineQueryLedger {
            batches: query_results.batches.clone(),
        };
        report.gpu_frame_ledger = gpu_frame_ledger;
        report.budget_directives = budget_directives(&input.policy);

        Ok(EngineFrameOutput {
            snapshot,
            query_results,
            report,
        })
    }
}

struct StateAdvanceRuntimeAdapter {
    executor: Arc<Mutex<Box<dyn EngineStateAdvanceExecutor>>>,
    input: EngineStateAdvanceInput,
    result: Arc<Mutex<Option<Result<StateAdvanceResult, EngineFrameError>>>>,
}

impl EngineSubsystemAdapter for StateAdvanceRuntimeAdapter {
    fn build(
        &mut self,
        builder: &mut EngineGraphBuilder,
    ) -> Result<EngineSubsystemPlan, EngineFrameError> {
        let descriptor = EngineSubsystemDescriptor {
            kind: EngineSubsystemKind::StateAdvance,
            label: "state_advance".to_string(),
            runs_after: Vec::new(),
            requires_gpu: false,
            allows_hot_path_readback: false,
        };
        let executor = Arc::clone(&self.executor);
        let input = self.input.clone();
        let result_for_job = Arc::clone(&self.result);
        let result_for_report = Arc::clone(&self.result);
        let input_count = input.inputs.inputs.len() as u64;
        let job = builder.add_job(
            descriptor.kind.clone(),
            "state_advance.advance".to_string(),
            EngineJobAffinity::Cpu,
            EngineSpanDomain::Cpu,
            Vec::new(),
            false,
            move || {
                let mut executor = executor.lock().map_err(|_| {
                    EngineFrameError::Message("state advance executor lock poisoned".into())
                })?;
                let advanced = executor.advance(input);
                *result_for_job.lock().map_err(|_| {
                    EngineFrameError::Message("state advance result lock poisoned".into())
                })? = Some(advanced);
                Ok(())
            },
        );
        Ok(EngineSubsystemPlan::new(
            descriptor.clone(),
            vec![job],
            vec![job],
            move |timeline: &EngineFrameTimeline, ctx| {
                let executed =
                    subsystem_elapsed_micros(timeline, EngineSubsystemKind::StateAdvance);
                if let Some(result) = result_for_report
                    .lock()
                    .map_err(|_| {
                        EngineFrameError::Message("state advance result lock poisoned".into())
                    })?
                    .as_ref()
                {
                    match result {
                        Ok(result) => {
                            ctx.input_snapshot_epoch = result
                                .transition_record
                                .from_snapshot
                                .as_ref()
                                .map(|snapshot| snapshot.epoch().0);
                            ctx.published_snapshot_epoch =
                                Some(result.transition_record.to_snapshot.epoch().0);
                        }
                        Err(err) => ctx
                            .violations
                            .push(format!("state_advance_error_before_report={err}")),
                    }
                }
                Ok(EngineSubsystemReport {
                    kind: descriptor.kind.clone(),
                    label: descriptor.label.clone(),
                    work_items: input_count,
                    cpu_critical_path_micros: executed,
                    gpu_critical_path_micros: None,
                    executed_wall_time_micros: executed,
                    self_reported_runtime_micros: Some(executed),
                    orchestration_gap_micros: 0,
                    measurement_policy: EngineMeasurementPolicy {
                        runtime_source: EngineRuntimeSource::TimelineSpans,
                        gpu_timing: EngineGpuTimingPolicy::Disabled,
                        hot_path_readback_allowed: false,
                        export_readback_allowed: false,
                    },
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

fn validate_readback_policy(
    policy: &EngineFrameRuntimePolicy,
    requests: &[EngineReadbackRequest],
) -> Result<(), EngineFrameError> {
    for request in requests {
        let rejected = match request.category {
            EngineReadbackCategory::Gameplay => {
                request.required_for_frame_completion
                    && request.bytes > 0
                    && !policy.allow_hot_path_gameplay_readbacks
            }
            EngineReadbackCategory::DebugExport => !policy.allow_debug_export_readbacks,
            EngineReadbackCategory::AttachmentCpuBounce => request.bytes > 0,
            EngineReadbackCategory::Timing | EngineReadbackCategory::Oracle => false,
        };
        if rejected {
            let message = if request.category == EngineReadbackCategory::AttachmentCpuBounce {
                format!(
                    "attachment CPU bounce rejected by engine frame policy: owner={:?} reason={} bytes={}",
                    request.owner, request.reason, request.bytes
                )
            } else {
                format!(
                    "hot-path readback rejected by engine frame policy: owner={:?} reason={} bytes={}",
                    request.owner, request.reason, request.bytes
                )
            };
            return Err(EngineFrameError::Message(format!("{message}")));
        }
    }
    Ok(())
}

fn build_readback_ledger(
    output_epoch: u64,
    requests: Vec<EngineReadbackRequest>,
) -> EngineReadbackLedger {
    let mut ledger = EngineReadbackLedger::default();
    for request in requests {
        if request.bytes > 0 {
            ledger.tickets.push(EngineReadbackTicketReport {
                owner: request.owner.clone(),
                reason: request.reason.clone(),
                category: request.category,
                bytes: request.bytes,
                snapshot_epoch: output_epoch,
                readiness: match request.category {
                    EngineReadbackCategory::Timing => EngineReadbackReadiness::TimingOnly,
                    _ => EngineReadbackReadiness::Deferred,
                },
            });
        }
        ledger.accepted.push(request);
    }
    ledger
}

fn validate_gpu_frame_policy(
    policy: &EngineFrameRuntimePolicy,
    private_gpu_submits: u32,
) -> Result<(), EngineFrameError> {
    if private_gpu_submits > 0 && !policy.allow_private_gpu_submits {
        return Err(EngineFrameError::Message(format!(
            "private GPU submits rejected by engine frame policy: count={private_gpu_submits}"
        )));
    }
    Ok(())
}

fn subsystem_reported_readback_requests(report: &EngineFrameReport) -> Vec<EngineReadbackRequest> {
    let mut requests = Vec::new();
    for subsystem in &report.subsystems {
        if subsystem.hot_path_readback_bytes > 0 {
            requests.push(EngineReadbackRequest {
                owner: subsystem.kind.clone(),
                reason: format!("subsystem_reported_hot_path:{}", subsystem.label),
                category: EngineReadbackCategory::Gameplay,
                bytes: subsystem.hot_path_readback_bytes,
                required_for_frame_completion: true,
            });
        }
        if subsystem.timing_readback_bytes > 0 {
            requests.push(EngineReadbackRequest {
                owner: subsystem.kind.clone(),
                reason: format!("subsystem_reported_timing:{}", subsystem.label),
                category: EngineReadbackCategory::Timing,
                bytes: subsystem.timing_readback_bytes,
                required_for_frame_completion: false,
            });
        }
    }
    if report.gpu_frame_ledger.attachment_cpu_bounce_count > 0 {
        requests.push(EngineReadbackRequest {
            owner: EngineSubsystemKind::Presentation,
            reason: "gpu_frame_ledger_attachment_cpu_bounce".to_string(),
            category: EngineReadbackCategory::AttachmentCpuBounce,
            bytes: u64::from(report.gpu_frame_ledger.attachment_cpu_bounce_count),
            required_for_frame_completion: true,
        });
    }
    requests
}

fn validate_state_advance(
    input: &EngineFrameInput,
    result: &StateAdvanceResult,
) -> Result<(), EngineFrameError> {
    if result.transition_record.inputs.tick != input.tick_inputs.tick {
        return Err(EngineFrameError::Message(
            "state advance returned a transition for the wrong simulation tick".into(),
        ));
    }
    if result.transition_record.from_snapshot.as_ref() != Some(&input.previous_snapshot) {
        return Err(EngineFrameError::Message(
            "state advance did not consume the frame input snapshot".into(),
        ));
    }
    let expected_epoch = input.previous_snapshot.epoch().0.saturating_add(1);
    if result.transition_record.to_snapshot.epoch().0 != expected_epoch {
        return Err(EngineFrameError::Message(format!(
            "state advance must publish exactly one next snapshot epoch: expected {expected_epoch}, got {}",
            result.transition_record.to_snapshot.epoch().0
        )));
    }
    if result.transition_record.current_clock.snapshot_epoch.get()
        != result.transition_record.to_snapshot.epoch().0
    {
        return Err(EngineFrameError::Message(
            "state advance clock epoch does not match published snapshot".into(),
        ));
    }
    Ok(())
}

fn build_resource_ledger(
    previous_snapshot: &WorldSnapshotHandle,
    output_epoch: u64,
    query_results: &EngineQueryResults,
    report: &EngineFrameReport,
) -> EngineResourceLedger {
    let mut accesses = vec![
        EngineResourceAccess {
            subsystem: EngineSubsystemKind::StateAdvance,
            resource: EngineResourceId::WorldSnapshot {
                epoch: previous_snapshot.epoch().0,
            },
            mode: EngineResourceAccessMode::Read,
        },
        EngineResourceAccess {
            subsystem: EngineSubsystemKind::StateAdvance,
            resource: EngineResourceId::WorldSnapshot {
                epoch: output_epoch,
            },
            mode: EngineResourceAccessMode::Write,
        },
    ];
    let mut states = vec![
        EngineResourceState {
            resource: EngineResourceId::WorldSnapshot {
                epoch: previous_snapshot.epoch().0,
            },
            residency: EngineResourceResidency::CpuAuthoritative,
            epoch_state: EngineResourceEpochState::Valid {
                epoch: previous_snapshot.epoch().0,
            },
            producer: EngineSubsystemKind::StateAdvance,
        },
        EngineResourceState {
            resource: EngineResourceId::WorldSnapshot {
                epoch: output_epoch,
            },
            residency: EngineResourceResidency::CpuAuthoritative,
            epoch_state: EngineResourceEpochState::Valid {
                epoch: output_epoch,
            },
            producer: EngineSubsystemKind::StateAdvance,
        },
        EngineResourceState {
            resource: EngineResourceId::ResidentScene {
                epoch: output_epoch,
            },
            residency: EngineResourceResidency::GpuResident,
            epoch_state: EngineResourceEpochState::Valid {
                epoch: output_epoch,
            },
            producer: EngineSubsystemKind::GpuRuntime,
        },
        EngineResourceState {
            resource: EngineResourceId::DynamicStateBuffer {
                epoch: output_epoch,
            },
            residency: EngineResourceResidency::GpuResident,
            epoch_state: EngineResourceEpochState::Valid {
                epoch: output_epoch,
            },
            producer: EngineSubsystemKind::GpuRuntime,
        },
    ];
    for batch in &query_results.batches {
        accesses.push(EngineResourceAccess {
            subsystem: EngineSubsystemKind::Query,
            resource: EngineResourceId::WorldSnapshot {
                epoch: batch.snapshot_epoch,
            },
            mode: EngineResourceAccessMode::Read,
        });
        accesses.push(EngineResourceAccess {
            subsystem: EngineSubsystemKind::Query,
            resource: EngineResourceId::QueryBuffer(format!(
                "{}:{}",
                batch.contract_id, batch.query_kind
            )),
            mode: EngineResourceAccessMode::Write,
        });
    }
    for subsystem in &report.subsystems {
        match subsystem.kind {
            EngineSubsystemKind::Presentation
            | EngineSubsystemKind::Collision
            | EngineSubsystemKind::Query => accesses.push(EngineResourceAccess {
                subsystem: subsystem.kind.clone(),
                resource: EngineResourceId::WorldSnapshot {
                    epoch: output_epoch,
                },
                mode: EngineResourceAccessMode::Read,
            }),
            _ => {}
        }
    }
    for handle in &query_results.resident_handles {
        states.push(EngineResourceState {
            resource: handle.resource.clone(),
            residency: handle.residency,
            epoch_state: EngineResourceEpochState::Valid {
                epoch: handle.snapshot_epoch,
            },
            producer: EngineSubsystemKind::Query,
        });
    }
    EngineResourceLedger {
        accesses,
        states,
        violations: Vec::new(),
    }
}

#[derive(Default)]
struct EngineQueryService;

impl EngineQueryService {
    fn execute(&self, output_epoch: u64, requests: Vec<EngineQueryRequest>) -> EngineQueryResults {
        let mut batches = BTreeMap::<(String, String, bool), EngineQueryBatchReport>::new();
        for request in requests {
            let key = (
                request.contract_id.clone(),
                request.query_kind.clone(),
                request.required_this_tick,
            );
            let batch = batches
                .entry(key)
                .or_insert_with(|| EngineQueryBatchReport {
                    snapshot_epoch: output_epoch,
                    contract_id: request.contract_id.clone(),
                    query_kind: request.query_kind.clone(),
                    required_this_tick: request.required_this_tick,
                    request_count: 0,
                    owners: Vec::new(),
                    resident: true,
                    value_readback_scheduled: false,
                });
            batch.request_count = batch.request_count.saturating_add(1);
            if !batch.owners.contains(&request.owner) {
                batch.owners.push(request.owner);
            }
        }
        let batches = batches.into_values().collect::<Vec<_>>();
        let resident_handles = batches
            .iter()
            .map(|batch| EngineQueryResidentHandle {
                resource: EngineResourceId::QueryBuffer(format!(
                    "epoch{}:{}:{}",
                    batch.snapshot_epoch, batch.contract_id, batch.query_kind
                )),
                snapshot_epoch: batch.snapshot_epoch,
                residency: EngineResourceResidency::GpuResident,
                value_readback_scheduled: batch.value_readback_scheduled,
            })
            .collect();
        EngineQueryResults {
            batches,
            resident_handles,
        }
    }
}

fn budget_directives(policy: &EngineFrameRuntimePolicy) -> EngineBudgetDirectives {
    EngineBudgetDirectives {
        frame_wall_time_budget_ms: policy
            .budget
            .as_ref()
            .map(|budget| budget.frame_wall_time_median_ms),
        future_reserve_ms: policy
            .budget
            .as_ref()
            .map(|budget| budget.future_subsystem_reserve_ms),
        hot_path_readbacks_allowed: policy.allow_hot_path_gameplay_readbacks,
        private_gpu_submits_allowed: policy.allow_private_gpu_submits,
    }
}

fn subsystem_elapsed_micros(timeline: &EngineFrameTimeline, kind: EngineSubsystemKind) -> u128 {
    timeline
        .spans
        .iter()
        .filter(|span| span.subsystem == kind)
        .map(|span| span.elapsed_micros())
        .sum()
}

fn change_class_label(class: ChangeClass) -> &'static str {
    match class {
        ChangeClass::None => "none",
        ChangeClass::Presentation => "presentation",
        ChangeClass::Structural => "structural",
        ChangeClass::Topology => "topology",
        ChangeClass::Identity => "identity",
        ChangeClass::Incompatible => "incompatible",
    }
}
