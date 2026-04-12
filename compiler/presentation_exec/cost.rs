use crate::presentation_contract::{
    QualityDegradationStep, RealtimeQualityState, RealtimeRadianceMode,
};
use crate::execution_policy::{
    PresentationExecutionPolicy, RayBudgetPolicy, RequiredGuaranteeClass, SelectedMethodClass,
};
use crate::presentation_plan::quality_tier_name;
use crate::query_plan::DispatchBackend;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PresentationPassCost {
    pub pass_id: String,
    pub pass_kind: String,
    pub work_items: u32,
    pub elapsed_micros: u128,
    pub dispatch_count: u32,
    pub attachment_bytes_read: u64,
    pub attachment_bytes_written: u64,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PresentationAttachmentBytes {
    pub attachment: String,
    pub width: u32,
    pub height: u32,
    pub total_size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PresentationQualityReport {
    pub tier: String,
    pub target_fps: u32,
    pub output_width: u32,
    pub output_height: u32,
    pub internal_width: u32,
    pub internal_height: u32,
    pub internal_resolution_scale: f32,
    pub achieved_native_output: bool,
    pub reconstructed_output: bool,
    pub temporal_mode: String,
    pub radiance_mode: String,
    pub media_enabled: bool,
    pub half_res_participants: bool,
    pub hit_compaction_enabled: bool,
    pub active_degradations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PresentationFrameCostReport {
    pub semantic_domain: String,
    pub execution_policy: String,
    pub legal_degradations: Vec<String>,
    pub output_width: u32,
    pub output_height: u32,
    pub internal_width: u32,
    pub internal_height: u32,
    pub quality: PresentationQualityReport,
    pub primary_hit_rate: f32,
    pub average_trace_steps: f32,
    pub max_trace_steps: u32,
    pub candidate_count_before_pruning: u32,
    pub candidate_count_after_pruning: u32,
    pub support_prune_effectiveness: f32,
    pub tile_cull_total_tiles: u32,
    pub tile_cull_active_tiles: u32,
    pub tile_cull_efficiency: f32,
    pub surface_resolve_count: u32,
    pub participant_resolve_count: u32,
    pub history_reuse_rate: f32,
    pub attachment_bytes: Vec<PresentationAttachmentBytes>,
    pub passes: Vec<PresentationPassCost>,
    pub active_acceleration_artifacts: Vec<String>,
    pub bottleneck_pass: Option<String>,
    pub performance_gain_sources: Vec<String>,
}

pub fn quality_report(
    quality: &RealtimeQualityState,
    output_width: u32,
    output_height: u32,
) -> PresentationQualityReport {
    let divisor =
        crate::presentation_exec::internal_resolution_divisor(quality.internal_resolution_scale)
            .max(1);
    let internal_width = output_width.max(1).div_ceil(divisor);
    let internal_height = output_height.max(1).div_ceil(divisor);
    PresentationQualityReport {
        tier: quality_tier_name(quality.tier).to_string(),
        target_fps: quality.target_fps,
        output_width,
        output_height,
        internal_width,
        internal_height,
        internal_resolution_scale: quality.internal_resolution_scale,
        achieved_native_output: internal_width == output_width && internal_height == output_height,
        reconstructed_output: internal_width != output_width || internal_height != output_height,
        temporal_mode: format!("{:?}", quality.temporal_mode),
        radiance_mode: radiance_mode_name(quality.radiance_mode).to_string(),
        media_enabled: quality.media_enabled,
        half_res_participants: quality.half_res_participants,
        hit_compaction_enabled: quality.hit_compaction_enabled,
        active_degradations: quality
            .active_degradations
            .iter()
            .map(|step| quality_degradation_name(*step).to_string())
            .collect(),
    }
}

pub fn quality_degradation_name(step: QualityDegradationStep) -> &'static str {
    match step {
        QualityDegradationStep::ReduceInternalResolution => "reduce_internal_resolution",
        QualityDegradationStep::EnableHitCompaction => "enable_hit_compaction",
        QualityDegradationStep::LowerPrimarySteps => "lower_primary_steps",
        QualityDegradationStep::DisableMedia => "disable_media",
        QualityDegradationStep::LowerRadianceQuality => "lower_radiance_quality",
        QualityDegradationStep::DisableRadiance => "disable_radiance",
        QualityDegradationStep::HalfResolutionParticipants => "half_res_participants",
    }
}

pub fn radiance_mode_name(mode: RealtimeRadianceMode) -> &'static str {
    match mode {
        RealtimeRadianceMode::Full => "full",
        RealtimeRadianceMode::Reduced => "reduced",
        RealtimeRadianceMode::Disabled => "disabled",
    }
}

pub fn required_guarantee_class_name(class: RequiredGuaranteeClass) -> &'static str {
    match class {
        RequiredGuaranteeClass::Exact => "exact",
        RequiredGuaranteeClass::ConservativeNoFalseMiss => "conservative_no_false_miss",
        RequiredGuaranteeClass::IntervalBounded => "interval_bounded",
        RequiredGuaranteeClass::BestEffort => "best_effort",
    }
}

pub fn selected_method_class_name(class: SelectedMethodClass) -> &'static str {
    match class {
        SelectedMethodClass::ExactOracle => "exact_oracle",
        SelectedMethodClass::ConservativeSolver => "conservative_solver",
        SelectedMethodClass::IntervalSolver => "interval_solver",
        SelectedMethodClass::HeuristicSolver => "heuristic_solver",
    }
}

fn format_ray_budget(budget: RayBudgetPolicy) -> String {
    format!(
        "max_distance={} min_step={} hit_epsilon={} max_steps={}",
        budget.max_distance, budget.min_step, budget.hit_epsilon, budget.max_steps
    )
}

pub fn render_semantic_domain_report(
    scene_id: u32,
    geometry_detail: i32,
    material: bool,
    radiance: bool,
    media: bool,
) -> String {
    format!(
        "scene_id={} geometry_detail={} material={} radiance={} media={}",
        scene_id, geometry_detail, material, radiance, media
    )
}

pub fn render_execution_policy_report(
    policy: &PresentationExecutionPolicy,
    backend: DispatchBackend,
    legal_degradations: &[QualityDegradationStep],
) -> String {
    format!(
        "backend={} required_guarantee={} selected_method={} primary_rays={} legal_degradations={}",
        match backend {
            DispatchBackend::Cpu => "cpu",
            DispatchBackend::VirtualGpu => "virtual_gpu",
            DispatchBackend::Wgsl => "wgsl",
            DispatchBackend::Auto => "auto",
        },
        required_guarantee_class_name(policy.required_guarantee),
        selected_method_class_name(policy.selected_method),
        format_ray_budget(policy.primary_rays),
        legal_degradations
            .iter()
            .map(|step| quality_degradation_name(*step))
            .collect::<Vec<_>>()
            .join("|")
    )
}

pub fn render_frame_cost_report(report: &PresentationFrameCostReport) -> String {
    let mut out = String::new();
    out.push_str(&format!("semantic_domain={}\n", report.semantic_domain));
    out.push_str(&format!("execution_policy={}\n", report.execution_policy));
    if !report.legal_degradations.is_empty() {
        out.push_str(&format!(
            "legal_degradations={}\n",
            report.legal_degradations.join(",")
        ));
    }
    out.push_str(&format!(
        "quality tier={} target_fps={} output={}x{} internal={}x{} scale={:.2} native_output={} reconstructed_output={} radiance={} media={} half_res_participants={} hit_compaction={}\n",
        report.quality.tier,
        report.quality.target_fps,
        report.output_width,
        report.output_height,
        report.internal_width,
        report.internal_height,
        report.quality.internal_resolution_scale,
        report.quality.achieved_native_output,
        report.quality.reconstructed_output,
        report.quality.radiance_mode,
        report.quality.media_enabled,
        report.quality.half_res_participants,
        report.quality.hit_compaction_enabled,
    ));
    if !report.quality.active_degradations.is_empty() {
        out.push_str(&format!(
            "active_degradations={}\n",
            report.quality.active_degradations.join(",")
        ));
    }
    out.push_str(&format!(
        "primary_hit_rate={:.3} average_trace_steps={:.3} max_trace_steps={} candidates_before={} candidates_after={} support_prune_effectiveness={:.3}\n",
        report.primary_hit_rate,
        report.average_trace_steps,
        report.max_trace_steps,
        report.candidate_count_before_pruning,
        report.candidate_count_after_pruning,
        report.support_prune_effectiveness,
    ));
    out.push_str(&format!(
        "tile_cull_total_tiles={} tile_cull_active_tiles={} tile_cull_efficiency={:.3} surface_resolve_count={} participant_resolve_count={} history_reuse_rate={:.3}\n",
        report.tile_cull_total_tiles,
        report.tile_cull_active_tiles,
        report.tile_cull_efficiency,
        report.surface_resolve_count,
        report.participant_resolve_count,
        report.history_reuse_rate,
    ));
    if !report.active_acceleration_artifacts.is_empty() {
        out.push_str(&format!(
            "active_acceleration_artifacts={}\n",
            report.active_acceleration_artifacts.join(",")
        ));
    }
    if let Some(bottleneck) = &report.bottleneck_pass {
        out.push_str(&format!("bottleneck_pass={bottleneck}\n"));
    }
    if !report.performance_gain_sources.is_empty() {
        out.push_str(&format!(
            "performance_gain_sources={}\n",
            report.performance_gain_sources.join(",")
        ));
    }
    out.push_str("attachment_bytes:\n");
    for attachment in &report.attachment_bytes {
        out.push_str(&format!(
            "- {} {}x{} {} bytes\n",
            attachment.attachment, attachment.width, attachment.height, attachment.total_size_bytes
        ));
    }
    out.push_str("passes:\n");
    for pass in &report.passes {
        out.push_str(&format!(
            "- {} kind={} items={} elapsed_us={} dispatches={} bytes_read={} bytes_written={}",
            pass.pass_id,
            pass.pass_kind,
            pass.work_items,
            pass.elapsed_micros,
            pass.dispatch_count,
            pass.attachment_bytes_read,
            pass.attachment_bytes_written,
        ));
        if !pass.notes.is_empty() {
            out.push_str(&format!(" notes={}", pass.notes.join("|")));
        }
        out.push('\n');
    }
    out
}
