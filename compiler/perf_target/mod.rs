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
pub struct PerfClosureProfile {
    pub version: u32,
    pub name: String,
    pub machine_class: String,
    pub adapter_name_pattern: String,
    pub backend: PerfClosureBackend,
    pub backend_contract: String,
    pub requested_limits_profile: String,
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
    pub primary_visibility_median_ms: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_visibility_p95_ms: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_frame_median_ms: Option<f32>,
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
pub struct PerfClosureReport {
    pub profile: PerfClosureProfile,
    pub frame: PerfClosureLaneStatusReport,
    pub collision: PerfClosureLaneStatusReport,
}

impl PerfClosureProfile {
    pub fn canonical_1080p120() -> Self {
        Self {
            version: PERF_CLOSURE_CONTRACT_VERSION,
            name: "canonical_1080p120".to_string(),
            machine_class: "desktop_class_cpu_oracle".to_string(),
            adapter_name_pattern: "cpu_oracle".to_string(),
            backend: PerfClosureBackend::Cpu,
            backend_contract: "cpu_oracle_with_wgsl_parity_reference".to_string(),
            requested_limits_profile: "cpu_oracle_reference".to_string(),
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
                protocol_id: "realtime_presentation.1080p120".to_string(),
                suite: "realtime_presentation".to_string(),
                scene_set_id: "closure_1080p120_frame".to_string(),
                view_set_id: "realtime_120_closure_views".to_string(),
                camera_path_id: "closure_camera_path_fixed".to_string(),
                motion_fixture_id: Some("closure_camera_motion_fixture".to_string()),
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
                protocol_id: "collision_perf.1080p120".to_string(),
                suite: "collision_perf".to_string(),
                scene_set_id: "closure_1080p120_collision".to_string(),
                view_set_id: "collision_closure_cases".to_string(),
                camera_path_id: "closure_collision_probe_path".to_string(),
                motion_fixture_id: Some("closure_collision_motion_fixture".to_string()),
                fixed_seed: 0x1080_0121,
            },
            collision_baseline: PerfClosureCollisionBaseline {
                baseline_id: "collision_perf.phase40_cpu_oracle".to_string(),
                max_runtime_regression_pct: 0.0,
            },
        }
    }

    pub fn named(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "canonical_1080p120" | "closure" | "1080p120" | "realtime_120" => {
                Some(Self::canonical_1080p120())
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
            PerfClosureLaneKind::Frame if lane.suite != "realtime_presentation" => errors.push(
                "frame closure lane suite must be realtime_presentation for the canonical profile"
                    .to_string(),
            ),
            PerfClosureLaneKind::Collision if lane.suite != "collision_perf" => errors.push(
                "collision closure lane suite must be collision_perf for the canonical profile"
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
            frame: PerfClosureLaneStatusReport::unsampled(&profile.frame),
            collision: PerfClosureLaneStatusReport::unsampled(&profile.collision),
            profile,
        }
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
            total_frame_p95_ms: None,
            collision_runtime_median_ms: None,
            collision_runtime_p95_ms: None,
            collision_baseline_id: None,
            collision_runtime_regression_pct: None,
            dominant_bottleneck_pass: None,
            notes: vec![format!(
                "{} lane not sampled for this suite",
                protocol.lane.as_str()
            )],
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

#[cfg(test)]
mod tests {
    use super::{
        PerfClosureDegradationStep, PerfClosureLaneKind, PerfClosureLaneStatus, PerfClosureProfile,
        quality_degradation_step_name,
    };

    #[test]
    fn canonical_profile_validates_and_is_named() {
        let profile = PerfClosureProfile::canonical_1080p120();
        assert!(profile.validate().is_empty(), "{:?}", profile.validate());
        assert_eq!(PerfClosureProfile::named("1080p120"), Some(profile.clone()));
        assert_eq!(profile.frame.lane, PerfClosureLaneKind::Frame);
        assert_eq!(profile.collision.lane, PerfClosureLaneKind::Collision);
        assert_eq!(profile.collision.suite, "collision_perf");
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
    }
}
