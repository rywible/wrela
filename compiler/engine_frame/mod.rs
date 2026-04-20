use crate::gpu_runtime::GpuRuntimeMetrics;
use serde::{Deserialize, Serialize};

mod scheduler;

pub use scheduler::{
    EngineBudgetDecision, EngineBudgetGovernor, EngineFrameContext, EngineFrameError,
    EngineFrameScheduler, EngineSubsystemDescriptor, EngineSubsystemWork,
};

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
    pub queue_submit_count: u32,
    pub hot_path_readback_bytes: u64,
    pub scene_reupload_bytes: u64,
    pub wait_time_micros: u128,
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
    pub frame_wall_time_micros: u128,
    pub cpu_critical_path_micros: u128,
    pub gpu_critical_path_micros: Option<u128>,
    pub present_wait_micros: u128,
    pub gpu_wait_micros: u128,
    pub readback_wait_micros: u128,
    pub steady_state_fps: f64,
    pub gpu_runtime: GpuRuntimeMetrics,
    pub subsystems: Vec<EngineSubsystemReport>,
    pub future_subsystem_reserve: EngineFutureReserveReport,
    pub active_degradations: Vec<String>,
    pub violations: Vec<String>,
}

impl EngineFrameReport {
    pub fn subsystem(&self, kind: EngineSubsystemKind) -> Option<&EngineSubsystemReport> {
        self.subsystems
            .iter()
            .find(|subsystem| subsystem.kind == kind)
    }
}
