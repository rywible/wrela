use crate::presentation_contract::{
    QualityDegradationStep, RealtimeQualityContract, RealtimeQualityTier, TemporalReuseMode,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const PERF_CLOSURE_CONTRACT_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PerfClosureLaneKind {
    Frame,
    Collision,
}

impl PerfClosureLaneKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Frame => "frame",
            Self::Collision => "collision",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PerfClosureLaneStatus {
    Validated,
    Violated,
    Sampled,
    NotSampled,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PerfClosureBackend {
    Cpu,
    Vgpu,
    Wgsl,
    Auto,
}

impl PerfClosureBackend {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Vgpu => "vgpu",
            Self::Wgsl => "wgsl",
            Self::Auto => "auto",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PerfClosureExecutionStory {
    #[default]
    WgslResident,
    CpuOracle,
}

impl PerfClosureExecutionStory {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CpuOracle => "cpu_oracle",
            Self::WgslResident => "wgsl_resident",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PerfClosureDegradationStep {
    ReduceInternalResolution,
    EnableHitCompaction,
    LowerPrimarySteps,
    DisableMedia,
    LowerRadianceQuality,
    DisableRadiance,
    HalfResolutionParticipants,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerfClosureLaneProtocol {
    pub lane: PerfClosureLaneKind,
    pub protocol_id: String,
    pub suite: String,
    pub scene_set_id: String,
    pub view_set_id: String,
    pub camera_path_id: String,
    pub motion_fixture_id: Option<String>,
    pub fixed_seed: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PerfClosureMetricBudget {
    pub median_ms: f32,
    pub p95_ms: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PerfClosureCollisionBaseline {
    pub baseline_id: String,
    pub max_runtime_regression_pct: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PerfClosureEngineFrameBudget {
    pub frame_wall_time_median_ms: f32,
    pub frame_wall_time_p95_ms: f32,
    pub presentation_median_ms: f32,
    pub collision_median_ms: f32,
    pub state_advance_median_ms: f32,
    pub future_subsystem_reserve_ms: f32,
    pub max_queue_submit_count_per_frame: u32,
    pub max_hot_path_readback_bytes_per_frame: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PerfClosureProfile {
    pub version: u32,
    pub name: String,
    #[serde(default)]
    pub execution_story: PerfClosureExecutionStory,
    pub machine_class: String,
    pub adapter_name_pattern: String,
    #[serde(default)]
    pub adapter_name: String,
    pub backend: PerfClosureBackend,
    pub backend_contract: String,
    pub requested_limits_profile: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub enabled_optional_features: Vec<String>,
    #[serde(default)]
    pub timestamps_enabled: bool,
    #[serde(default)]
    pub gpu_timestamps_required_if_supported: bool,
    #[serde(default)]
    pub max_hot_path_readback_bytes_per_frame: u64,
    #[serde(default)]
    pub max_scene_reupload_bytes_per_frame: u64,
    #[serde(default)]
    pub max_cpu_screen_sample_allocations_per_frame: u32,
    #[serde(default)]
    pub max_attachment_cpu_bounce_count: u32,
    #[serde(default)]
    pub max_queue_submit_count_per_frame: u32,
    #[serde(default)]
    pub max_dispatch_count_primary_visibility: u32,
    #[serde(default)]
    pub f16_enabled: bool,
    #[serde(default)]
    pub indirect_dispatch_enabled: bool,
    #[serde(default)]
    pub warmup_protocol: String,
    pub output_width: u32,
    pub output_height: u32,
    pub target_fps: u32,
    pub min_internal_resolution_scale: f32,
    pub legal_degradations: Vec<PerfClosureDegradationStep>,
    pub warmup_runs: u32,
    pub measured_runs: u32,
    pub frame: PerfClosureLaneProtocol,
    pub frame_budget: PerfClosureMetricBudget,
    pub primary_visibility_budget: PerfClosureMetricBudget,
    pub collision: PerfClosureLaneProtocol,
    pub collision_baseline: PerfClosureCollisionBaseline,
    pub engine_frame_budget: PerfClosureEngineFrameBudget,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PerfClosureLaneStatusReport {
    pub lane: PerfClosureLaneKind,
    pub protocol_id: String,
    pub suite: String,
    pub status: PerfClosureLaneStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub measured_output_width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub measured_output_height: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_internal_resolution_scale_observed: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_internal_resolution_scale_observed: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reconstructed_output_detected: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub active_acceleration_artifacts: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub active_degradations: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hot_path_readback_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene_reupload_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_screen_sample_allocations: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attachment_cpu_bounce_count: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queue_submit_count: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_visibility_dispatch_count: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamps_supported: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamped_pass_count: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_visibility_median_ms: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_visibility_p95_ms: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_frame_median_ms: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_frame_median_fps: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_frame_p95_ms: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collision_runtime_median_ms: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collision_runtime_p95_ms: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collision_baseline_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collision_runtime_regression_pct: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dominant_bottleneck_pass: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PerfClosureEngineFrameStatusReport {
    pub status: PerfClosureLaneStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame_wall_time_median_ms: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame_wall_time_p95_ms: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_critical_path_median_ms: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gpu_critical_path_median_ms: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presentation_median_ms: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collision_median_ms: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_advance_median_ms: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub future_subsystem_reserve_ms: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queue_submit_count: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hot_path_readback_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene_reupload_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub active_degradations: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub violations: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
    /// Sampled motion-to-photon median (ms) when the benchmark lane records latency.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub motion_to_photon_median_ms: Option<f32>,
    /// Budget used with `motion_to_photon_median_ms` for closure findings (ms).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub motion_to_photon_budget_ms: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PerfClosureVerdictStatus {
    #[default]
    NotApplicable,
    Met,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PerfClosureFinding {
    pub subsystem: String,
    pub focus: String,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<String>,
    pub next_step: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PerfClosureVerdict {
    pub status: PerfClosureVerdictStatus,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_remaining_bottleneck: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub findings: Vec<PerfClosureFinding>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PerfClosureReport {
    pub profile: PerfClosureProfile,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_oracle_profile: Option<PerfClosureProfile>,
    pub frame: PerfClosureLaneStatusReport,
    pub collision: PerfClosureLaneStatusReport,
    pub engine_frame: PerfClosureEngineFrameStatusReport,
    #[serde(default)]
    pub verdict: PerfClosureVerdict,
}

impl PerfClosureProfile {
    pub fn canonical_1080p120() -> Self {
        Self::canonical_1080p120_wgsl_resident()
    }

    pub fn canonical_1080p120_cpu_oracle() -> Self {
        Self {
            version: PERF_CLOSURE_CONTRACT_VERSION,
            name: "canonical_1080p120_cpu_oracle".to_string(),
            execution_story: PerfClosureExecutionStory::CpuOracle,
            machine_class: "desktop_class_cpu_oracle".to_string(),
            adapter_name_pattern: "cpu_oracle".to_string(),
            adapter_name: "cpu_oracle".to_string(),
            backend: PerfClosureBackend::Cpu,
            backend_contract: "cpu_oracle_with_wgsl_parity_reference".to_string(),
            requested_limits_profile: "cpu_oracle_reference".to_string(),
            enabled_optional_features: Vec::new(),
            timestamps_enabled: false,
            gpu_timestamps_required_if_supported: false,
            max_hot_path_readback_bytes_per_frame: 0,
            max_scene_reupload_bytes_per_frame: 0,
            max_cpu_screen_sample_allocations_per_frame: 0,
            max_attachment_cpu_bounce_count: 0,
            max_queue_submit_count_per_frame: 64,
            max_dispatch_count_primary_visibility: 4096,
            f16_enabled: false,
            indirect_dispatch_enabled: false,
            warmup_protocol: "cpu_oracle_baseline_warmup".to_string(),
            output_width: 1920,
            output_height: 1080,
            target_fps: 120,
            min_internal_resolution_scale: 1.0,
            legal_degradations: vec![
                PerfClosureDegradationStep::EnableHitCompaction,
                PerfClosureDegradationStep::LowerPrimarySteps,
                PerfClosureDegradationStep::DisableMedia,
                PerfClosureDegradationStep::LowerRadianceQuality,
                PerfClosureDegradationStep::DisableRadiance,
                PerfClosureDegradationStep::HalfResolutionParticipants,
            ],
            warmup_runs: 4,
            measured_runs: 12,
            frame: PerfClosureLaneProtocol {
                lane: PerfClosureLaneKind::Frame,
                protocol_id: "engine_frame.1080p120.frame".to_string(),
                suite: "engine_frame".to_string(),
                scene_set_id: "engine_frame_1080p120_frame".to_string(),
                view_set_id: "engine_frame_1080p120_views".to_string(),
                camera_path_id: "engine_frame_camera_path_fixed".to_string(),
                motion_fixture_id: Some("engine_frame_camera_motion_fixture".to_string()),
                fixed_seed: 0x1080_0120,
            },
            frame_budget: PerfClosureMetricBudget {
                median_ms: 8.33,
                p95_ms: 8.33,
            },
            primary_visibility_budget: PerfClosureMetricBudget {
                median_ms: 4.50,
                p95_ms: 5.25,
            },
            collision: PerfClosureLaneProtocol {
                lane: PerfClosureLaneKind::Collision,
                protocol_id: "engine_frame.1080p120.collision".to_string(),
                suite: "engine_frame".to_string(),
                scene_set_id: "engine_frame_1080p120_collision".to_string(),
                view_set_id: "engine_frame_1080p120_collision_cases".to_string(),
                camera_path_id: "engine_frame_collision_probe_path".to_string(),
                motion_fixture_id: Some("engine_frame_collision_motion_fixture".to_string()),
                fixed_seed: 0x1080_0121,
            },
            collision_baseline: PerfClosureCollisionBaseline {
                baseline_id: "collision_perf.phase40_cpu_oracle".to_string(),
                max_runtime_regression_pct: 0.0,
            },
            engine_frame_budget: PerfClosureEngineFrameBudget {
                frame_wall_time_median_ms: 8.33,
                frame_wall_time_p95_ms: 8.33,
                presentation_median_ms: 4.50,
                collision_median_ms: 2.50,
                state_advance_median_ms: 0.25,
                future_subsystem_reserve_ms: 1.00,
                max_queue_submit_count_per_frame: 2,
                max_hot_path_readback_bytes_per_frame: 0,
            },
        }
    }

    pub fn canonical_1080p120_wgsl_resident() -> Self {
        Self {
            version: PERF_CLOSURE_CONTRACT_VERSION,
            name: "canonical_1080p120_wgsl_resident".to_string(),
            execution_story: PerfClosureExecutionStory::WgslResident,
            machine_class: "desktop_class_wgsl_resident".to_string(),
            adapter_name_pattern: "wgsl_resident".to_string(),
            adapter_name: "wgsl_resident".to_string(),
            backend: PerfClosureBackend::Wgsl,
            backend_contract: "wgsl_resident_with_cpu_oracle_reference".to_string(),
            requested_limits_profile: "wgsl_resident_reference".to_string(),
            enabled_optional_features: vec![],
            timestamps_enabled: false,
            gpu_timestamps_required_if_supported: false,
            max_hot_path_readback_bytes_per_frame: 0,
            max_scene_reupload_bytes_per_frame: 0,
            max_cpu_screen_sample_allocations_per_frame: 0,
            max_attachment_cpu_bounce_count: 0,
            max_queue_submit_count_per_frame: 1,
            max_dispatch_count_primary_visibility: 0,
            f16_enabled: false,
            indirect_dispatch_enabled: false,
            warmup_protocol: "pipeline_and_resident_scene_upload".to_string(),
            output_width: 1920,
            output_height: 1080,
            target_fps: 120,
            min_internal_resolution_scale: 1.0,
            legal_degradations: vec![
                PerfClosureDegradationStep::EnableHitCompaction,
                PerfClosureDegradationStep::LowerPrimarySteps,
                PerfClosureDegradationStep::DisableMedia,
                PerfClosureDegradationStep::LowerRadianceQuality,
                PerfClosureDegradationStep::DisableRadiance,
                PerfClosureDegradationStep::HalfResolutionParticipants,
            ],
            warmup_runs: 4,
            measured_runs: 12,
            frame: PerfClosureLaneProtocol {
                lane: PerfClosureLaneKind::Frame,
                protocol_id: "engine_frame.1080p120.frame".to_string(),
                suite: "engine_frame".to_string(),
                scene_set_id: "engine_frame_1080p120_frame".to_string(),
                view_set_id: "engine_frame_1080p120_views".to_string(),
                camera_path_id: "engine_frame_camera_path_fixed".to_string(),
                motion_fixture_id: Some("engine_frame_camera_motion_fixture".to_string()),
                fixed_seed: 0x1080_0120,
            },
            frame_budget: PerfClosureMetricBudget {
                median_ms: 8.33,
                p95_ms: 8.33,
            },
            primary_visibility_budget: PerfClosureMetricBudget {
                median_ms: 4.50,
                p95_ms: 5.25,
            },
            collision: PerfClosureLaneProtocol {
                lane: PerfClosureLaneKind::Collision,
                protocol_id: "engine_frame.1080p120.collision".to_string(),
                suite: "engine_frame".to_string(),
                scene_set_id: "engine_frame_1080p120_collision".to_string(),
                view_set_id: "engine_frame_1080p120_collision_cases".to_string(),
                camera_path_id: "engine_frame_collision_probe_path".to_string(),
                motion_fixture_id: Some("engine_frame_collision_motion_fixture".to_string()),
                fixed_seed: 0x1080_0121,
            },
            collision_baseline: PerfClosureCollisionBaseline {
                baseline_id: "collision_perf.phase40_cpu_oracle".to_string(),
                max_runtime_regression_pct: 0.0,
            },
            engine_frame_budget: PerfClosureEngineFrameBudget {
                frame_wall_time_median_ms: 8.33,
                frame_wall_time_p95_ms: 8.33,
                presentation_median_ms: 4.50,
                collision_median_ms: 2.50,
                state_advance_median_ms: 0.25,
                future_subsystem_reserve_ms: 1.00,
                max_queue_submit_count_per_frame: 2,
                max_hot_path_readback_bytes_per_frame: 0,
            },
        }
    }

    pub fn shader_f16_gate_enabled(&self) -> bool {
        self.f16_enabled
    }

    pub fn shader_f16_gate_state(&self) -> &'static str {
        if self.shader_f16_gate_enabled() {
            "enabled"
        } else {
            "disabled"
        }
    }

    pub fn named(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "canonical_1080p120"
            | "closure"
            | "1080p120"
            | "realtime_120"
            | "wgsl_resident"
            | "canonical_1080p120_wgsl_resident" => Some(Self::canonical_1080p120_wgsl_resident()),
            "canonical_1080p120_cpu_oracle" | "cpu_oracle" | "cpu_oracle_1080p120" => {
                Some(Self::canonical_1080p120_cpu_oracle())
            }
            _ => None,
        }
    }

    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        if self.version != PERF_CLOSURE_CONTRACT_VERSION {
            errors.push(format!(
                "performance closure contract version {} does not match {}",
                self.version, PERF_CLOSURE_CONTRACT_VERSION
            ));
        }
        if self.name.trim().is_empty() {
            errors.push("performance closure profile name must not be empty".to_string());
        }
        if self.adapter_name.trim().is_empty() {
            errors.push("performance closure adapter_name must not be empty".to_string());
        }
        if self.machine_class.trim().is_empty() {
            errors.push("performance closure machine_class must not be empty".to_string());
        }
        if self.adapter_name_pattern.trim().is_empty() {
            errors.push("performance closure adapter_name_pattern must not be empty".to_string());
        }
        if self.backend_contract.trim().is_empty() {
            errors.push("performance closure backend_contract must not be empty".to_string());
        }
        if self.requested_limits_profile.trim().is_empty() {
            errors
                .push("performance closure requested_limits_profile must not be empty".to_string());
        }
        if self.warmup_protocol.trim().is_empty() {
            errors.push("performance closure warmup_protocol must not be empty".to_string());
        }
        match self.execution_story {
            PerfClosureExecutionStory::CpuOracle
                if !matches!(self.backend, PerfClosureBackend::Cpu) =>
            {
                errors.push("cpu_oracle closure profiles must use the cpu backend".to_string());
            }
            PerfClosureExecutionStory::WgslResident
                if !matches!(self.backend, PerfClosureBackend::Wgsl) =>
            {
                errors.push("wgsl_resident closure profiles must use the wgsl backend".to_string());
            }
            _ => {}
        }
        if self.output_width != 1920 || self.output_height != 1080 {
            errors.push("canonical performance closure must target 1920x1080 output".to_string());
        }
        if self.target_fps != 120 {
            errors.push("canonical performance closure must target 120 FPS".to_string());
        }
        if !(0.0 < self.min_internal_resolution_scale && self.min_internal_resolution_scale <= 1.0)
        {
            errors.push(
                "performance closure min_internal_resolution_scale must be in the range (0, 1]"
                    .to_string(),
            );
        }
        if self.warmup_runs == 0 {
            errors.push("performance closure warmup_runs must be greater than zero".to_string());
        }
        if self.measured_runs == 0 {
            errors.push("performance closure measured_runs must be greater than zero".to_string());
        }
        if self.legal_degradations.is_empty() {
            errors.push("performance closure must define a legal degradation set".to_string());
        }
        let mut seen_degradations = BTreeSet::new();
        for step in &self.legal_degradations {
            if !seen_degradations.insert(quality_degradation_step_name(*step)) {
                errors.push(format!(
                    "performance closure legal degradation '{}' appears more than once",
                    quality_degradation_step_name(*step)
                ));
            }
        }
        let mut seen_optional_features = BTreeSet::new();
        for feature in &self.enabled_optional_features {
            if !seen_optional_features.insert(feature) {
                errors.push(format!(
                    "performance closure enabled_optional_features '{}' appears more than once",
                    feature
                ));
            }
        }
        let allows_internal_resolution_drop = self
            .legal_degradations
            .contains(&PerfClosureDegradationStep::ReduceInternalResolution);
        if allows_internal_resolution_drop && self.min_internal_resolution_scale >= 1.0 {
            errors.push(
                "performance closure cannot allow reduce_internal_resolution when the floor is 1.0"
                    .to_string(),
            );
        }
        if !allows_internal_resolution_drop && self.min_internal_resolution_scale < 1.0 {
            errors.push(
                "performance closure with an internal-resolution floor below 1.0 must allow reduce_internal_resolution"
                    .to_string(),
            );
        }
        errors.extend(validate_metric_budget("frame_budget", &self.frame_budget));
        errors.extend(validate_metric_budget(
            "primary_visibility_budget",
            &self.primary_visibility_budget,
        ));
        errors.extend(self.validate_lane_protocol(&self.frame, PerfClosureLaneKind::Frame));
        errors.extend(self.validate_lane_protocol(&self.collision, PerfClosureLaneKind::Collision));
        if self.collision_baseline.baseline_id.trim().is_empty() {
            errors.push(
                "performance closure collision_baseline.baseline_id must not be empty".to_string(),
            );
        }
        if self.collision_baseline.max_runtime_regression_pct < 0.0 {
            errors.push(
                "performance closure collision_baseline.max_runtime_regression_pct must be non-negative"
                    .to_string(),
            );
        }
        errors.extend(validate_engine_frame_budget(&self.engine_frame_budget));
        errors
    }

    pub fn frame_quality_contract(&self) -> RealtimeQualityContract {
        let mut contract = RealtimeQualityContract::named(RealtimeQualityTier::Realtime120)
            .with_temporal_mode(TemporalReuseMode::ReprojectColorAndMotion);
        contract.allow_dynamic_resolution = self
            .legal_degradations
            .contains(&PerfClosureDegradationStep::ReduceInternalResolution);
        contract.internal_resolution_scale = self.min_internal_resolution_scale;
        contract.primary_max_steps = 96;
        contract.allow_media = true;
        contract.allow_half_res_participants = self
            .legal_degradations
            .contains(&PerfClosureDegradationStep::HalfResolutionParticipants);
        contract.allow_hit_compaction = self
            .legal_degradations
            .contains(&PerfClosureDegradationStep::EnableHitCompaction);
        contract.degradation_order = self
            .legal_degradations
            .iter()
            .copied()
            .map(presentation_degradation_step)
            .collect();
        contract
    }

    fn validate_lane_protocol(
        &self,
        lane: &PerfClosureLaneProtocol,
        expected_kind: PerfClosureLaneKind,
    ) -> Vec<String> {
        let mut errors = Vec::new();
        if lane.lane != expected_kind {
            errors.push(format!(
                "performance closure lane {:?} does not match expected {:?}",
                lane.lane, expected_kind
            ));
        }
        if lane.protocol_id.trim().is_empty() {
            errors.push("performance closure protocol_id must not be empty".to_string());
        }
        if lane.suite.trim().is_empty() {
            errors.push("performance closure suite must not be empty".to_string());
        }
        if lane.scene_set_id.trim().is_empty() {
            errors.push("performance closure scene_set_id must not be empty".to_string());
        }
        if lane.view_set_id.trim().is_empty() {
            errors.push("performance closure view_set_id must not be empty".to_string());
        }
        if lane.camera_path_id.trim().is_empty() {
            errors.push("performance closure camera_path_id must not be empty".to_string());
        }
        match lane.lane {
            PerfClosureLaneKind::Frame if lane.suite != "engine_frame" => errors.push(
                "frame closure lane suite must be engine_frame for the canonical profile"
                    .to_string(),
            ),
            PerfClosureLaneKind::Collision if lane.suite != "engine_frame" => errors.push(
                "collision closure lane suite must be engine_frame for the canonical profile"
                    .to_string(),
            ),
            _ => {}
        }
        errors
    }
}

impl PerfClosureReport {
    pub fn unsampled(profile: PerfClosureProfile) -> Self {
        Self {
            cpu_oracle_profile: None,
            frame: PerfClosureLaneStatusReport::unsampled(&profile.frame),
            collision: PerfClosureLaneStatusReport::unsampled(&profile.collision),
            engine_frame: PerfClosureEngineFrameStatusReport::unsampled(),
            profile,
            verdict: PerfClosureVerdict::not_applicable(),
        }
    }
}

impl PerfClosureVerdict {
    pub fn not_applicable() -> Self {
        Self {
            status: PerfClosureVerdictStatus::NotApplicable,
            summary: "closure target was not exercised in this run".to_string(),
            top_remaining_bottleneck: None,
            findings: Vec::new(),
        }
    }

    pub fn met(summary: impl Into<String>) -> Self {
        Self {
            status: PerfClosureVerdictStatus::Met,
            summary: summary.into(),
            top_remaining_bottleneck: None,
            findings: Vec::new(),
        }
    }

    pub fn failed(
        summary: impl Into<String>,
        top_remaining_bottleneck: Option<String>,
        findings: Vec<PerfClosureFinding>,
    ) -> Self {
        Self {
            status: PerfClosureVerdictStatus::Failed,
            summary: summary.into(),
            top_remaining_bottleneck,
            findings,
        }
    }
}

impl Default for PerfClosureVerdict {
    fn default() -> Self {
        Self::not_applicable()
    }
}

impl PerfClosureLaneStatusReport {
    pub fn unsampled(protocol: &PerfClosureLaneProtocol) -> Self {
        Self {
            lane: protocol.lane,
            protocol_id: protocol.protocol_id.clone(),
            suite: protocol.suite.clone(),
            status: PerfClosureLaneStatus::NotSampled,
            measured_output_width: None,
            measured_output_height: None,
            min_internal_resolution_scale_observed: None,
            max_internal_resolution_scale_observed: None,
            reconstructed_output_detected: None,
            active_acceleration_artifacts: Vec::new(),
            active_degradations: Vec::new(),
            primary_visibility_median_ms: None,
            primary_visibility_p95_ms: None,
            total_frame_median_ms: None,
            total_frame_median_fps: None,
            total_frame_p95_ms: None,
            collision_runtime_median_ms: None,
            collision_runtime_p95_ms: None,
            collision_baseline_id: None,
            collision_runtime_regression_pct: None,
            dominant_bottleneck_pass: None,
            hot_path_readback_bytes: None,
            scene_reupload_bytes: None,
            cpu_screen_sample_allocations: None,
            attachment_cpu_bounce_count: None,
            queue_submit_count: None,
            primary_visibility_dispatch_count: None,
            timestamps_supported: None,
            timestamped_pass_count: None,
            notes: vec![format!(
                "{} lane not sampled for this suite",
                protocol.lane.as_str()
            )],
        }
    }
}

impl PerfClosureEngineFrameStatusReport {
    pub fn unsampled() -> Self {
        Self {
            status: PerfClosureLaneStatus::NotSampled,
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
            active_degradations: Vec::new(),
            violations: Vec::new(),
            notes: vec!["engine_frame lane not sampled for this suite".to_string()],
            motion_to_photon_median_ms: None,
            motion_to_photon_budget_ms: None,
        }
    }
}

pub fn quality_degradation_step_name(step: PerfClosureDegradationStep) -> &'static str {
    match step {
        PerfClosureDegradationStep::ReduceInternalResolution => "reduce_internal_resolution",
        PerfClosureDegradationStep::EnableHitCompaction => "enable_hit_compaction",
        PerfClosureDegradationStep::LowerPrimarySteps => "lower_primary_steps",
        PerfClosureDegradationStep::DisableMedia => "disable_media",
        PerfClosureDegradationStep::LowerRadianceQuality => "lower_radiance_quality",
        PerfClosureDegradationStep::DisableRadiance => "disable_radiance",
        PerfClosureDegradationStep::HalfResolutionParticipants => "half_res_participants",
    }
}

fn presentation_degradation_step(step: PerfClosureDegradationStep) -> QualityDegradationStep {
    match step {
        PerfClosureDegradationStep::ReduceInternalResolution => {
            QualityDegradationStep::ReduceInternalResolution
        }
        PerfClosureDegradationStep::EnableHitCompaction => {
            QualityDegradationStep::EnableHitCompaction
        }
        PerfClosureDegradationStep::LowerPrimarySteps => QualityDegradationStep::LowerPrimarySteps,
        PerfClosureDegradationStep::DisableMedia => QualityDegradationStep::DisableMedia,
        PerfClosureDegradationStep::LowerRadianceQuality => {
            QualityDegradationStep::LowerRadianceQuality
        }
        PerfClosureDegradationStep::DisableRadiance => QualityDegradationStep::DisableRadiance,
        PerfClosureDegradationStep::HalfResolutionParticipants => {
            QualityDegradationStep::HalfResolutionParticipants
        }
    }
}

fn validate_metric_budget(name: &str, budget: &PerfClosureMetricBudget) -> Vec<String> {
    let mut errors = Vec::new();
    if !(budget.median_ms.is_finite() && budget.median_ms > 0.0) {
        errors.push(format!(
            "performance closure {name}.median_ms must be finite and positive"
        ));
    }
    if !(budget.p95_ms.is_finite() && budget.p95_ms > 0.0) {
        errors.push(format!(
            "performance closure {name}.p95_ms must be finite and positive"
        ));
    }
    if budget.p95_ms < budget.median_ms {
        errors.push(format!(
            "performance closure {name}.p95_ms must be greater than or equal to median_ms"
        ));
    }
    errors
}

fn validate_engine_frame_budget(budget: &PerfClosureEngineFrameBudget) -> Vec<String> {
    let mut errors = Vec::new();
    for (name, value) in [
        (
            "frame_wall_time_median_ms",
            budget.frame_wall_time_median_ms,
        ),
        ("frame_wall_time_p95_ms", budget.frame_wall_time_p95_ms),
        ("presentation_median_ms", budget.presentation_median_ms),
        ("collision_median_ms", budget.collision_median_ms),
        ("state_advance_median_ms", budget.state_advance_median_ms),
        (
            "future_subsystem_reserve_ms",
            budget.future_subsystem_reserve_ms,
        ),
    ] {
        if !(value.is_finite() && value > 0.0) {
            errors.push(format!(
                "performance closure engine_frame_budget.{name} must be finite and positive"
            ));
        }
    }
    if budget.frame_wall_time_p95_ms < budget.frame_wall_time_median_ms {
        errors.push(
            "performance closure engine_frame_budget.frame_wall_time_p95_ms must be greater than or equal to frame_wall_time_median_ms"
                .to_string(),
        );
    }
    let subsystem_total = budget.presentation_median_ms
        + budget.collision_median_ms
        + budget.state_advance_median_ms
        + budget.future_subsystem_reserve_ms;
    if subsystem_total > budget.frame_wall_time_median_ms + f32::EPSILON {
        errors.push(format!(
            "performance closure engine_frame_budget subsystem budgets ({subsystem_total:.2} ms) exceed frame_wall_time_median_ms ({:.2} ms)",
            budget.frame_wall_time_median_ms
        ));
    }
    errors
}

#[cfg(test)]
mod tests {
    use super::{
        PerfClosureDegradationStep, PerfClosureExecutionStory, PerfClosureLaneKind,
        PerfClosureLaneStatus, PerfClosureProfile, quality_degradation_step_name,
    };

    #[test]
    fn canonical_profile_validates_and_is_named() {
        let profile = PerfClosureProfile::canonical_1080p120();
        assert!(profile.validate().is_empty(), "{:?}", profile.validate());
        assert_eq!(PerfClosureProfile::named("1080p120"), Some(profile.clone()));
        assert_eq!(
            profile.execution_story,
            PerfClosureExecutionStory::WgslResident
        );
        assert_eq!(profile.backend.as_str(), "wgsl");
        assert_eq!(profile.adapter_name, "wgsl_resident");
        assert!(profile.enabled_optional_features.is_empty());
        assert_eq!(profile.timestamps_enabled, false);
        assert_eq!(profile.gpu_timestamps_required_if_supported, false);
        assert_eq!(profile.max_hot_path_readback_bytes_per_frame, 0);
        assert_eq!(profile.max_scene_reupload_bytes_per_frame, 0);
        assert_eq!(profile.max_cpu_screen_sample_allocations_per_frame, 0);
        assert_eq!(profile.max_attachment_cpu_bounce_count, 0);
        assert_eq!(profile.max_queue_submit_count_per_frame, 1);
        assert_eq!(profile.max_dispatch_count_primary_visibility, 0);
        assert_eq!(profile.f16_enabled, false);
        assert_eq!(profile.shader_f16_gate_enabled(), false);
        assert_eq!(profile.shader_f16_gate_state(), "disabled");
        assert_eq!(profile.indirect_dispatch_enabled, false);
        assert_eq!(profile.engine_frame_budget.frame_wall_time_median_ms, 8.33);
        assert_eq!(profile.engine_frame_budget.frame_wall_time_p95_ms, 8.33);
        assert_eq!(profile.engine_frame_budget.presentation_median_ms, 4.50);
        assert_eq!(profile.engine_frame_budget.collision_median_ms, 2.50);
        assert_eq!(profile.engine_frame_budget.state_advance_median_ms, 0.25);
        assert_eq!(
            profile.engine_frame_budget.future_subsystem_reserve_ms,
            1.00
        );
        assert_eq!(
            profile.engine_frame_budget.max_queue_submit_count_per_frame,
            2
        );
        assert_eq!(
            profile
                .engine_frame_budget
                .max_hot_path_readback_bytes_per_frame,
            0
        );
        assert_eq!(
            profile.warmup_protocol,
            "pipeline_and_resident_scene_upload"
        );
        let cpu_oracle = PerfClosureProfile::canonical_1080p120_cpu_oracle();
        assert_eq!(
            PerfClosureProfile::named("cpu_oracle"),
            Some(cpu_oracle.clone())
        );
        assert_eq!(
            cpu_oracle.execution_story,
            PerfClosureExecutionStory::CpuOracle
        );
        assert_eq!(cpu_oracle.backend.as_str(), "cpu");
        assert_eq!(profile.frame.lane, PerfClosureLaneKind::Frame);
        assert_eq!(profile.collision.lane, PerfClosureLaneKind::Collision);
        assert_eq!(profile.frame.suite, "engine_frame");
        assert_eq!(profile.collision.suite, "engine_frame");
        assert_eq!(
            profile
                .legal_degradations
                .iter()
                .map(|step| quality_degradation_step_name(*step))
                .collect::<Vec<_>>(),
            vec![
                "enable_hit_compaction",
                "lower_primary_steps",
                "disable_media",
                "lower_radiance_quality",
                "disable_radiance",
                "half_res_participants",
            ]
        );
    }

    #[test]
    fn shader_f16_gate_helper_reflects_disabled_and_enabled_profiles() {
        let mut profile = PerfClosureProfile::canonical_1080p120();
        assert_eq!(profile.shader_f16_gate_state(), "disabled");
        profile.f16_enabled = true;
        assert_eq!(profile.shader_f16_gate_enabled(), true);
        assert_eq!(profile.shader_f16_gate_state(), "enabled");
    }

    #[test]
    fn contradictory_internal_resolution_contract_is_rejected() {
        let mut profile = PerfClosureProfile::canonical_1080p120();
        profile.min_internal_resolution_scale = 1.0;
        profile
            .legal_degradations
            .insert(0, PerfClosureDegradationStep::ReduceInternalResolution);
        let errors = profile.validate();
        assert!(
            errors
                .iter()
                .any(|error| error.contains("reduce_internal_resolution")),
            "{errors:?}"
        );
    }

    #[test]
    fn closure_report_defaults_to_unsampled_lanes() {
        let profile = PerfClosureProfile::canonical_1080p120();
        let report = super::PerfClosureReport::unsampled(profile);
        assert_eq!(report.frame.status, PerfClosureLaneStatus::NotSampled);
        assert_eq!(report.collision.status, PerfClosureLaneStatus::NotSampled);
        assert_eq!(
            report.engine_frame.status,
            PerfClosureLaneStatus::NotSampled
        );
        assert!(report.cpu_oracle_profile.is_none());
    }

    #[test]
    fn invalid_engine_frame_budget_is_rejected() {
        let mut profile = PerfClosureProfile::canonical_1080p120();
        profile.engine_frame_budget.frame_wall_time_median_ms = 1.0;
        profile.engine_frame_budget.frame_wall_time_p95_ms = 1.0;
        let errors = profile.validate();
        assert!(
            errors
                .iter()
                .any(|error| error.contains("engine_frame_budget subsystem budgets")),
            "{errors:?}"
        );
    }
}
