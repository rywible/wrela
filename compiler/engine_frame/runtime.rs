use super::latency::{PresentModePolicy, TickInputSource, input_arrival_timestamp_stats};
use super::{
    EngineFrameError, EngineFrameReport, EngineFrameScheduler, EngineFrameTimeline,
    EngineGpuTimingPolicy, EngineGraphBuilder, EngineJobAffinity, EngineMeasurementPolicy,
    EngineRuntimeSource, EngineSpanDomain, EngineSubsystemAdapter, EngineSubsystemDescriptor,
    EngineSubsystemKind, EngineSubsystemPlan, EngineSubsystemReport,
};
use crate::perf_target::{PerfClosureEngineFrameBudget, PerfClosureProfile};
use crate::state_advance::{ChangeClass, StateAdvanceResult, TickInputBatch};
use crate::time_semantics::TemporalClock;
use crate::world_identity::WorldSnapshotHandle;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};

#[derive(Debug, Clone)]
pub struct EngineFrameInput {
    pub scenario_id: String,
    pub frame_index: u32,
    pub previous_snapshot: WorldSnapshotHandle,
    pub previous_clock: TemporalClock,
    pub current_clock: TemporalClock,
    pub tick_inputs: TickInputSource,
    pub policy: EngineFrameRuntimePolicy,
    pub query_requests: Vec<EngineQueryRequest>,
    pub readback_requests: Vec<EngineReadbackRequest>,
}

impl EngineFrameInput {
    pub fn expected_simulation_tick(&self) -> crate::state_advance::SimulationTick {
        self.current_clock.simulation_tick
    }
}

#[derive(Debug, Clone)]
pub struct EngineFrameOutput {
    pub snapshot: WorldSnapshotHandle,
    pub query_results: EngineQueryResults,
    pub report: EngineFrameReport,
}

#[derive(Debug, Clone, Default)]
pub struct MaterializedTickInputSlot {
    inner: Arc<RwLock<Option<TickInputBatch>>>,
}

impl MaterializedTickInputSlot {
    fn publish(&self, batch: TickInputBatch) -> Result<(), EngineFrameError> {
        let mut guard = self
            .inner
            .write()
            .map_err(|_| EngineFrameError::Message("tick input slot lock poisoned".into()))?;
        *guard = Some(batch);
        Ok(())
    }

    pub fn snapshot(&self) -> Result<Option<TickInputBatch>, EngineFrameError> {
        self.inner
            .read()
            .map_err(|_| EngineFrameError::Message("tick input slot lock poisoned".into()))
            .map(|guard| guard.clone())
    }

    fn clear(&self) -> Result<(), EngineFrameError> {
        let mut guard = self
            .inner
            .write()
            .map_err(|_| EngineFrameError::Message("tick input slot lock poisoned".into()))?;
        *guard = None;
        Ok(())
    }
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
    /// When `Some`, `EngineFrameReport.latency.total_estimate_nanos` is compared each frame.
    pub motion_to_photon_target_ms: Option<f64>,
    pub max_frames_in_flight: u32,
    pub present_mode_policy: PresentModePolicy,
}

impl EngineFrameRuntimePolicy {
    pub fn closure() -> Self {
        Self {
            allow_hot_path_gameplay_readbacks: false,
            allow_debug_export_readbacks: false,
            allow_private_gpu_submits: false,
            max_change_class: ChangeClass::Identity,
            budget: None,
            motion_to_photon_target_ms: None,
            max_frames_in_flight: 2,
            present_mode_policy: PresentModePolicy::Fifo,
        }
    }

    /// Interactive gameplay default (RFC 0011): latency-first, canonical engine-frame budget.
    pub fn live() -> Self {
        Self {
            allow_hot_path_gameplay_readbacks: false,
            allow_debug_export_readbacks: false,
            allow_private_gpu_submits: false,
            max_change_class: ChangeClass::Behavior,
            budget: Some(
                PerfClosureProfile::canonical_1080p120_wgsl_resident().engine_frame_budget,
            ),
            motion_to_photon_target_ms: Some(16.0),
            max_frames_in_flight: 1,
            present_mode_policy: PresentModePolicy::PreferMailboxThenVrrFifoThenFifo,
        }
    }

    /// Inspector / tools overlay: permissive readbacks, no motion-to-photon gate.
    pub fn tools() -> Self {
        Self {
            allow_hot_path_gameplay_readbacks: true,
            allow_debug_export_readbacks: true,
            allow_private_gpu_submits: true,
            max_change_class: ChangeClass::Identity,
            budget: None,
            motion_to_photon_target_ms: None,
            max_frames_in_flight: 2,
            present_mode_policy: PresentModePolicy::Fifo,
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
    InputFrame { epoch: u64 },
    ResidentRegion { region_id: String, epoch: u64 },
    PhysicsBodyState { epoch: u64 },
    PhysicsContactLedger { tick: u64 },
    PhysicsMoveState { body_id: u64, tick: u64 },
    AudioVoiceLedger { epoch: u64 },
    SaveRecord { epoch: u64 },
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
    materialized_tick_inputs: MaterializedTickInputSlot,
    /// Shared with [`ResidencySubsystemAdapter::with_state_outcome`] and similar adapters
    /// so post-state-advance subsystems observe the same `WorldSnapshotHandle` as state advance.
    state_advance_outcome: Arc<Mutex<Option<Result<StateAdvanceResult, EngineFrameError>>>>,
}

impl EngineFrameRuntime {
    pub fn new(state_advance: Box<dyn EngineStateAdvanceExecutor>) -> Self {
        Self {
            state_advance: Arc::new(Mutex::new(state_advance)),
            scheduler: EngineFrameScheduler::default(),
            materialized_tick_inputs: MaterializedTickInputSlot::default(),
            state_advance_outcome: Arc::new(Mutex::new(None)),
        }
    }

    pub fn with_executor_config(
        state_advance: Box<dyn EngineStateAdvanceExecutor>,
        executor_config: wrela_runtime::engine_executor::EngineExecutorConfig,
    ) -> Self {
        Self {
            state_advance: Arc::new(Mutex::new(state_advance)),
            scheduler: EngineFrameScheduler::with_executor_config(executor_config),
            materialized_tick_inputs: MaterializedTickInputSlot::default(),
            state_advance_outcome: Arc::new(Mutex::new(None)),
        }
    }

    /// Clone this handle into [`crate::engine_frame::ResidencySubsystemAdapter::with_state_outcome`].
    pub fn state_advance_outcome_slot(
        &self,
    ) -> Arc<Mutex<Option<Result<StateAdvanceResult, EngineFrameError>>>> {
        Arc::clone(&self.state_advance_outcome)
    }

    pub fn materialized_tick_input_slot(&self) -> MaterializedTickInputSlot {
        self.materialized_tick_inputs.clone()
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
        self.run_frame_with_persistent_subsystems(input, &mut subsystem_adapters)
    }

    pub fn run_frame_with_persistent_subsystems(
        &mut self,
        input: EngineFrameInput,
        subsystem_adapters: &mut [Box<dyn EngineSubsystemAdapter>],
    ) -> Result<EngineFrameOutput, EngineFrameError> {
        let readback_manager = EngineReadbackManager::new(input.policy.clone());
        self.scheduler.budget = input.policy.budget.clone();
        self.materialized_tick_inputs.clear()?;
        {
            let mut slot = self.state_advance_outcome.lock().map_err(|_| {
                EngineFrameError::Message("state advance outcome slot lock poisoned".into())
            })?;
            *slot = None;
        }

        for adapter in subsystem_adapters.iter_mut() {
            adapter.prepare_frame(&input);
        }

        let state_result = Arc::clone(&self.state_advance_outcome);
        let mut state_adapter = StateAdvanceRuntimeAdapter {
            executor: Arc::clone(&self.state_advance),
            previous_snapshot: input.previous_snapshot.clone(),
            previous_clock: input.previous_clock,
            current_clock: input.current_clock,
            tick_source: input.tick_inputs.clone(),
            max_change_class: input.policy.max_change_class,
            materialized_tick_inputs: self.materialized_tick_inputs.clone(),
            result: Arc::clone(&state_result),
        };
        let mut adapters: Vec<&mut dyn EngineSubsystemAdapter> = vec![&mut state_adapter];
        adapters.extend(
            subsystem_adapters
                .iter_mut()
                .map(|adapter| adapter.as_mut() as &mut dyn EngineSubsystemAdapter),
        );
        let mut report = self.scheduler.run_frame_borrowed(
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
            input.current_clock.simulation_tick.get(),
            &query_results,
            &report,
        );
        let mut readback_requests = input.readback_requests.clone();
        readback_requests.extend(subsystem_reported_readback_requests(&report));
        readback_manager.validate_requests(&readback_requests)?;
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
        apply_latency_budget_to_report(&input.policy, &mut report);

        Ok(EngineFrameOutput {
            snapshot,
            query_results,
            report,
        })
    }
}

struct StateAdvanceRuntimeAdapter {
    executor: Arc<Mutex<Box<dyn EngineStateAdvanceExecutor>>>,
    previous_snapshot: WorldSnapshotHandle,
    previous_clock: TemporalClock,
    current_clock: TemporalClock,
    tick_source: TickInputSource,
    max_change_class: ChangeClass,
    materialized_tick_inputs: MaterializedTickInputSlot,
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
        let previous_snapshot = self.previous_snapshot.clone();
        let previous_clock = self.previous_clock;
        let current_clock = self.current_clock;
        let tick_source = self.tick_source.clone();
        let max_change_class = self.max_change_class;
        let result_for_job = Arc::clone(&self.result);
        let result_for_report = Arc::clone(&self.result);
        let materialized_tick_inputs = self.materialized_tick_inputs.clone();
        let materialized_tick_inputs_for_report = self.materialized_tick_inputs.clone();
        let tick_for_batch = current_clock.simulation_tick;
        let wall_deadline = current_clock.wall_clock;
        // The late-sampling deadline is the StateAdvance input sample time;
        // it shares the `TickInputEvent::monotonic_nanos` origin by
        // `LateInputSampler` contract and is not a scheduler span origin.
        let state_advance_input_sample_nanos = Arc::new(AtomicU64::new(wall_deadline.get()));
        let state_advance_input_sample_nanos_for_job =
            Arc::clone(&state_advance_input_sample_nanos);
        let state_advance_input_sample_nanos_for_report =
            Arc::clone(&state_advance_input_sample_nanos);
        let tick_source_for_report = tick_source.clone();
        let job = builder.add_job(
            descriptor.kind.clone(),
            "state_advance.advance".to_string(),
            EngineJobAffinity::Cpu,
            EngineSpanDomain::Cpu,
            Vec::new(),
            false,
            move || {
                let sample_deadline = tick_source.sample_deadline(wall_deadline);
                state_advance_input_sample_nanos_for_job
                    .store(sample_deadline.get(), Ordering::Release);
                let inputs =
                    tick_source.materialize_for_simulation_tick(tick_for_batch, sample_deadline);
                let materialized_inputs = inputs.clone();
                let input = EngineStateAdvanceInput {
                    previous_snapshot: previous_snapshot.clone(),
                    previous_clock,
                    current_clock,
                    inputs,
                };
                let mut executor = executor.lock().map_err(|_| {
                    EngineFrameError::Message("state advance executor lock poisoned".into())
                })?;
                let advanced = executor.advance(input);
                let mut guard = result_for_job.lock().map_err(|_| {
                    EngineFrameError::Message("state advance result lock poisoned".into())
                })?;
                match advanced {
                    Ok(advanced) => {
                        if let Err(err) = validate_state_advance_result(
                            &previous_snapshot,
                            tick_for_batch,
                            &advanced,
                        )
                        .and_then(|_| {
                            validate_state_advance_change_class(max_change_class, &advanced)
                        }) {
                            *guard = Some(Err(err.clone()));
                            return Err(err);
                        }
                        materialized_tick_inputs.publish(materialized_inputs)?;
                        *guard = Some(Ok(advanced));
                        Ok(())
                    }
                    Err(err) => {
                        *guard = Some(Err(err.clone()));
                        Err(err)
                    }
                }
            },
        );
        Ok(EngineSubsystemPlan::new(
            descriptor.clone(),
            vec![job],
            vec![job],
            move |timeline: &EngineFrameTimeline, ctx| {
                let state_advance_input_sample_nanos =
                    state_advance_input_sample_nanos_for_report.load(Ordering::Acquire);
                ctx.state_advance_input_sample_nanos = Some(state_advance_input_sample_nanos);
                if let TickInputSource::Late(sampler) = &tick_source_for_report {
                    let ring_state = sampler.ring_state();
                    if ring_state.overflow {
                        if !ctx
                            .violations
                            .iter()
                            .any(|v| v == "presentation.input_ring_overflow")
                        {
                            ctx.violations
                                .push("presentation.input_ring_overflow".to_string());
                        }
                        sampler.clear_overflow();
                    }
                }
                if let Some(batch) = materialized_tick_inputs_for_report.snapshot()? {
                    let stats =
                        input_arrival_timestamp_stats(&batch, state_advance_input_sample_nanos);
                    if let Some(earliest) = stats.earliest_valid_arrival_nanos {
                        ctx.earliest_input_arrival_nanos = Some(
                            ctx.earliest_input_arrival_nanos
                                .map(|prev| prev.min(earliest))
                                .unwrap_or(earliest),
                        );
                    }
                    if stats.has_future_timestamp() {
                        ctx.future_input_timestamp_count = ctx
                            .future_input_timestamp_count
                            .saturating_add(stats.future_timestamp_count);
                    }
                }
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
                let work_items = result_for_report
                    .lock()
                    .map_err(|_| {
                        EngineFrameError::Message("state advance result lock poisoned".into())
                    })?
                    .as_ref()
                    .and_then(|slot| slot.as_ref().ok())
                    .map(|result| result.transition_record.inputs.inputs.len() as u64)
                    .unwrap_or(0);
                Ok(EngineSubsystemReport {
                    kind: descriptor.kind.clone(),
                    label: descriptor.label.clone(),
                    work_items,
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
    validate_state_advance_result(
        &input.previous_snapshot,
        input.expected_simulation_tick(),
        result,
    )
}

fn validate_state_advance_result(
    previous_snapshot: &WorldSnapshotHandle,
    expected_simulation_tick: crate::state_advance::SimulationTick,
    result: &StateAdvanceResult,
) -> Result<(), EngineFrameError> {
    if previous_snapshot.epoch().0 == u64::MAX {
        return Err(EngineFrameError::Message(
            "snapshot epoch reached u64::MAX; advance would wrap".into(),
        ));
    }
    if result.transition_record.inputs.tick != expected_simulation_tick {
        return Err(EngineFrameError::Message(
            "state advance returned a transition for the wrong simulation tick".into(),
        ));
    }
    if result.transition_record.from_snapshot.as_ref() != Some(previous_snapshot) {
        return Err(EngineFrameError::Message(
            "state advance did not consume the frame input snapshot".into(),
        ));
    }
    let expected_epoch = previous_snapshot.epoch().0.saturating_add(1);
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

fn validate_state_advance_change_class(
    max_change_class: ChangeClass,
    result: &StateAdvanceResult,
) -> Result<(), EngineFrameError> {
    if !max_change_class.allows(result.change_summary.class) {
        return Err(EngineFrameError::Message(format!(
            "state advance change is incompatible with frame policy: {:?}",
            result.change_summary.class
        )));
    }
    Ok(())
}

fn build_resource_ledger(
    previous_snapshot: &WorldSnapshotHandle,
    output_epoch: u64,
    simulation_tick: u64,
    query_results: &EngineQueryResults,
    report: &EngineFrameReport,
) -> EngineResourceLedger {
    let mut accesses = report.resource_ledger.accesses.clone();
    accesses.extend([
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
    ]);
    let mut states = report.resource_ledger.states.clone();
    states.extend([
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
    ]);
    let gpu_scene_touched = report.gpu_frame_ledger.upload_bytes > 0
        || report.subsystems.iter().any(|subsystem| {
            subsystem.scene_reupload_bytes > 0 || subsystem.queue_submit_count > 0
        });
    if gpu_scene_touched {
        states.push(EngineResourceState {
            resource: EngineResourceId::ResidentScene {
                epoch: output_epoch,
            },
            residency: EngineResourceResidency::GpuResident,
            epoch_state: EngineResourceEpochState::Valid {
                epoch: output_epoch,
            },
            producer: EngineSubsystemKind::GpuRuntime,
        });
        states.push(EngineResourceState {
            resource: EngineResourceId::DynamicStateBuffer {
                epoch: output_epoch,
            },
            residency: EngineResourceResidency::GpuResident,
            epoch_state: EngineResourceEpochState::Valid {
                epoch: output_epoch,
            },
            producer: EngineSubsystemKind::GpuRuntime,
        });
    }
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
            EngineSubsystemKind::Input => {
                accesses.push(EngineResourceAccess {
                    subsystem: subsystem.kind.clone(),
                    resource: EngineResourceId::InputFrame {
                        epoch: output_epoch,
                    },
                    mode: EngineResourceAccessMode::Write,
                });
                states.push(EngineResourceState {
                    resource: EngineResourceId::InputFrame {
                        epoch: output_epoch,
                    },
                    residency: EngineResourceResidency::CpuAuthoritative,
                    epoch_state: EngineResourceEpochState::Valid {
                        epoch: output_epoch,
                    },
                    producer: subsystem.kind.clone(),
                });
            }
            EngineSubsystemKind::System => {
                accesses.push(EngineResourceAccess {
                    subsystem: subsystem.kind.clone(),
                    resource: EngineResourceId::InputFrame {
                        epoch: output_epoch,
                    },
                    mode: EngineResourceAccessMode::Read,
                });
                accesses.push(EngineResourceAccess {
                    subsystem: subsystem.kind.clone(),
                    resource: EngineResourceId::WorldSnapshot {
                        epoch: previous_snapshot.epoch().0,
                    },
                    mode: EngineResourceAccessMode::Read,
                });
            }
            EngineSubsystemKind::Residency => {
                accesses.push(EngineResourceAccess {
                    subsystem: subsystem.kind.clone(),
                    resource: EngineResourceId::WorldSnapshot {
                        epoch: output_epoch,
                    },
                    mode: EngineResourceAccessMode::Read,
                });
            }
            EngineSubsystemKind::Physics => {
                accesses.push(EngineResourceAccess {
                    subsystem: subsystem.kind.clone(),
                    resource: EngineResourceId::WorldSnapshot {
                        epoch: output_epoch,
                    },
                    mode: EngineResourceAccessMode::Read,
                });
                states.push(EngineResourceState {
                    resource: EngineResourceId::PhysicsBodyState {
                        epoch: output_epoch,
                    },
                    residency: EngineResourceResidency::Shared,
                    epoch_state: EngineResourceEpochState::Valid {
                        epoch: output_epoch,
                    },
                    producer: subsystem.kind.clone(),
                });
                states.push(EngineResourceState {
                    resource: EngineResourceId::PhysicsContactLedger {
                        tick: simulation_tick,
                    },
                    residency: EngineResourceResidency::CpuAuthoritative,
                    epoch_state: EngineResourceEpochState::Valid {
                        epoch: output_epoch,
                    },
                    producer: subsystem.kind.clone(),
                });
            }
            EngineSubsystemKind::Audio => {
                accesses.push(EngineResourceAccess {
                    subsystem: subsystem.kind.clone(),
                    resource: EngineResourceId::AudioVoiceLedger {
                        epoch: output_epoch,
                    },
                    mode: EngineResourceAccessMode::Write,
                });
                states.push(EngineResourceState {
                    resource: EngineResourceId::AudioVoiceLedger {
                        epoch: output_epoch,
                    },
                    residency: EngineResourceResidency::CpuAuthoritative,
                    epoch_state: EngineResourceEpochState::Valid {
                        epoch: output_epoch,
                    },
                    producer: subsystem.kind.clone(),
                });
            }
            EngineSubsystemKind::Save => {
                if subsystem.work_items > 0 {
                    accesses.push(EngineResourceAccess {
                        subsystem: subsystem.kind.clone(),
                        resource: EngineResourceId::WorldSnapshot {
                            epoch: output_epoch,
                        },
                        mode: EngineResourceAccessMode::Read,
                    });
                    accesses.push(EngineResourceAccess {
                        subsystem: subsystem.kind.clone(),
                        resource: EngineResourceId::SaveRecord {
                            epoch: output_epoch,
                        },
                        mode: EngineResourceAccessMode::Write,
                    });
                    states.push(EngineResourceState {
                        resource: EngineResourceId::SaveRecord {
                            epoch: output_epoch,
                        },
                        residency: EngineResourceResidency::CpuAuthoritative,
                        epoch_state: EngineResourceEpochState::Valid {
                            epoch: output_epoch,
                        },
                        producer: subsystem.kind.clone(),
                    });
                }
            }
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
        violations: report.resource_ledger.violations.clone(),
    }
}

#[derive(Default)]
struct EngineQueryService;

impl EngineQueryService {
    fn execute(&self, output_epoch: u64, requests: Vec<EngineQueryRequest>) -> EngineQueryResults {
        let mut batches = BTreeMap::<(String, String), EngineQueryBatchReport>::new();
        for request in requests {
            let key = (request.contract_id.clone(), request.query_kind.clone());
            let batch = batches
                .entry(key)
                .or_insert_with(|| EngineQueryBatchReport {
                    snapshot_epoch: output_epoch,
                    contract_id: request.contract_id.clone(),
                    query_kind: request.query_kind.clone(),
                    required_this_tick: false,
                    request_count: 0,
                    owners: Vec::new(),
                    resident: true,
                    value_readback_scheduled: false,
                });
            batch.required_this_tick |= request.required_this_tick;
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

/// When `policy.motion_to_photon_target_ms` is set, append a violation if the report exceeds it.
///
/// RFC 0011 M4: also drives the canonical `MotionToPhotonBudgetRule` closure
/// finding so the live policy budget produces the same actionable evidence the
/// perf-closure machinery emits in benchmark runs.
pub fn apply_latency_budget_to_report(
    policy: &EngineFrameRuntimePolicy,
    report: &mut EngineFrameReport,
) {
    if let Some(ms) = policy.motion_to_photon_target_ms {
        let limit_ns = (ms * 1_000_000.0).max(0.0) as u64;
        if report.latency.total_estimate_nanos > limit_ns {
            report
                .violations
                .push("presentation.motion_to_photon_over_budget".into());
            report
                .active_degradations
                .push("latency.tighten_present_mode_and_rebudget_subsystems".into());
        }
    }

    let observed_ms = (report.latency.total_estimate_nanos as f64 / 1_000_000.0) as f32;
    let status_report = crate::perf_target::PerfClosureEngineFrameStatusReport {
        status: crate::perf_target::PerfClosureLaneStatus::Sampled,
        frame_wall_time_median_ms: None,
        frame_wall_time_p95_ms: None,
        cpu_critical_path_median_ms: None,
        gpu_critical_path_median_ms: None,
        presentation_median_ms: None,
        collision_median_ms: None,
        state_advance_median_ms: None,
        future_subsystem_reserve_ms: None,
        queue_submit_count: None,
        hot_path_readback_bytes: None,
        scene_reupload_bytes: None,
        active_degradations: report.active_degradations.clone(),
        violations: report.violations.clone(),
        notes: report
            .subsystems
            .iter()
            .flat_map(|subsystem| subsystem.notes.clone())
            .collect(),
        motion_to_photon_median_ms: Some(observed_ms),
        motion_to_photon_budget_ms: policy.motion_to_photon_target_ms.map(|ms| ms as f32),
    };
    let budget = policy.budget.clone().unwrap_or_else(|| {
        crate::perf_target::PerfClosureProfile::canonical_1080p120_wgsl_resident()
            .engine_frame_budget
    });
    let findings =
        crate::engine_frame::collect_engine_frame_budget_findings(&budget, &status_report);
    for finding in findings {
        if !report.closure_findings.iter().any(|existing| {
            existing.subsystem == finding.subsystem
                && existing.focus == finding.focus
                && existing.evidence == finding.evidence
        }) {
            report.closure_findings.push(finding);
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
        ChangeClass::Behavior => "behavior",
        ChangeClass::Incompatible => "incompatible",
    }
}
