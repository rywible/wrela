use crate::gpu_runtime::GpuRuntimeMetrics;
use serde::{Deserialize, Serialize};

mod runtime;
mod scheduler;

pub const ENGINE_FRAME_TIMELINE_VERSION: u32 = 2;

pub use runtime::{
    EngineBudgetDirectives, EngineFrameIdentityReport, EngineFrameInput, EngineFrameOutput,
    EngineFrameRuntime, EngineFrameRuntimePolicy, EngineGpuFrameContext, EngineGpuFrameLedger,
    EngineQueryBatchReport, EngineQueryLedger, EngineQueryRequest, EngineQueryResidentHandle,
    EngineQueryResults, EngineReadbackCategory, EngineReadbackLedger, EngineReadbackManager,
    EngineReadbackReadiness, EngineReadbackRequest, EngineReadbackTicketReport,
    EngineResourceAccess, EngineResourceAccessMode, EngineResourceEpochState, EngineResourceId,
    EngineResourceLedger, EngineResourceResidency, EngineResourceState, EngineStateAdvanceExecutor,
    EngineStateAdvanceInput, EngineStateAdvanceReport,
};
pub use scheduler::{
    EngineBudgetDecision, EngineBudgetGovernor, EngineFrameContext, EngineFrameError,
    EngineFrameGraph, EngineFrameScheduler, EngineGraphBuilder, EngineSubsystemAdapter,
    EngineSubsystemDescriptor, EngineSubsystemPlan,
};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(transparent)]
pub struct EngineJobHandle(pub u32);

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(transparent)]
pub struct EngineFenceId(pub u32);

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(transparent)]
pub struct EngineSpanId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EngineJobAffinity {
    #[default]
    Cpu,
    Gpu,
    External,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EngineSpanDomain {
    #[default]
    Cpu,
    Gpu,
    GpuWait,
    ReadbackWait,
    PresentWait,
    External,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EngineRuntimeSource {
    #[default]
    TimelineSpans,
    SelfReported,
    CompatibilityJoin,
    ReservedSlotUnsampled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EngineGpuTimingPolicy {
    #[default]
    Disabled,
    Timestamped,
    RuntimeProxy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EngineMeasurementPolicy {
    #[serde(default)]
    pub runtime_source: EngineRuntimeSource,
    #[serde(default)]
    pub gpu_timing: EngineGpuTimingPolicy,
    #[serde(default)]
    pub hot_path_readback_allowed: bool,
    #[serde(default)]
    pub export_readback_allowed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineSubsystemKind {
    StateAdvance,
    Presentation,
    Collision,
    Query,
    GpuRuntime,
    FutureReserve(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineSpanRecord {
    pub id: EngineSpanId,
    pub subsystem: EngineSubsystemKind,
    pub label: String,
    pub domain: EngineSpanDomain,
    pub started_micros: u128,
    pub ended_micros: u128,
    pub thread_name: String,
    pub queue_submission: bool,
}

impl EngineSpanRecord {
    pub fn elapsed_micros(&self) -> u128 {
        self.ended_micros.saturating_sub(self.started_micros)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineSubsystemSpanRange {
    pub kind: EngineSubsystemKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_span_id: Option<EngineSpanId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_span_id: Option<EngineSpanId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EngineFrameTimeline {
    pub version: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub critical_path_span_ids: Vec<EngineSpanId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub queue_submission_spans: Vec<EngineSpanId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subsystem_span_ranges: Vec<EngineSubsystemSpanRange>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spans: Vec<EngineSpanRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineSubsystemBudget {
    pub median_ms: f32,
    pub p95_ms: f32,
    pub max_queue_submits: u32,
    pub max_hot_path_readback_bytes: u64,
    pub max_scene_reupload_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineSubsystemReport {
    pub kind: EngineSubsystemKind,
    pub label: String,
    pub work_items: u64,
    pub cpu_critical_path_micros: u128,
    pub gpu_critical_path_micros: Option<u128>,
    #[serde(default, skip_serializing_if = "is_zero_u128")]
    pub executed_wall_time_micros: u128,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub self_reported_runtime_micros: Option<u128>,
    #[serde(default, skip_serializing_if = "is_zero_u128")]
    pub orchestration_gap_micros: u128,
    #[serde(default)]
    pub measurement_policy: EngineMeasurementPolicy,
    pub queue_submit_count: u32,
    pub hot_path_readback_bytes: u64,
    pub scene_reupload_bytes: u64,
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub timestamped_pass_count: u32,
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub timing_readback_bytes: u64,
    pub wait_time_micros: u128,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EngineFutureReserveReport {
    pub reserved_micros: u128,
    pub remaining_micros: i128,
    pub exhausted: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineFrameReport {
    pub scenario_id: String,
    pub frame_index: u32,
    #[serde(default)]
    pub identity: EngineFrameIdentityReport,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_advance: Option<EngineStateAdvanceReport>,
    #[serde(default)]
    pub resource_ledger: EngineResourceLedger,
    #[serde(default)]
    pub readback_ledger: EngineReadbackLedger,
    #[serde(default)]
    pub query_ledger: EngineQueryLedger,
    #[serde(default)]
    pub gpu_frame_ledger: EngineGpuFrameLedger,
    #[serde(default)]
    pub budget_directives: EngineBudgetDirectives,
    pub frame_wall_time_micros: u128,
    pub cpu_critical_path_micros: u128,
    pub gpu_critical_path_micros: Option<u128>,
    pub present_wait_micros: u128,
    pub gpu_wait_micros: u128,
    pub readback_wait_micros: u128,
    pub steady_state_fps: f64,
    pub gpu_runtime: GpuRuntimeMetrics,
    pub timeline_version: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub critical_path_span_ids: Vec<EngineSpanId>,
    pub cpu_busy_micros: u128,
    pub gpu_busy_micros: u128,
    pub overlap_ratio: f32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub queue_submission_spans: Vec<EngineSpanId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subsystem_span_ranges: Vec<EngineSubsystemSpanRange>,
    pub subsystems: Vec<EngineSubsystemReport>,
    pub future_subsystem_reserve: EngineFutureReserveReport,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub active_degradations: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub violations: Vec<String>,
}

impl EngineFrameReport {
    pub fn subsystem(&self, kind: EngineSubsystemKind) -> Option<&EngineSubsystemReport> {
        self.subsystems
            .iter()
            .find(|subsystem| subsystem.kind == kind)
    }
}

fn is_zero_u128(value: &u128) -> bool {
    *value == 0
}

fn is_zero_u64(value: &u64) -> bool {
    *value == 0
}

fn is_zero_u32(value: &u32) -> bool {
    *value == 0
}
