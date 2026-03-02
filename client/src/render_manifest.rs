#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use std::collections::{BTreeSet, BinaryHeap, HashMap, HashSet};
use wrela_material_graph::{DefaultProfileContractsV1, validate_default_profile_contracts};

const RENDER_MANIFEST_SCHEMA: &str = "render-schema-v6";
const SHADER_BUNDLE_SCHEMA: &str = "shader-bundle-v6";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RenderPrimitiveTopology {
    Triangles,
    TriangleStrip,
    Lines,
    LineStrip,
    Points,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RenderCullMode {
    None,
    BackFace,
    FrontFace,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RenderManifestDocument {
    pub schema_version: String,
    pub pipelines: Vec<RenderPipelineManifestEntry>,
    #[serde(default)]
    pub frame_graph: Vec<RenderPassManifestEntry>,
    #[serde(default)]
    pub contracts: Option<RenderContractsManifestEntry>,
    #[serde(default)]
    pub resource_contracts: Vec<RenderResourceContractManifestEntry>,
    #[serde(default)]
    pub pass_contracts: Vec<RenderPassContractManifestEntry>,
    pub shader_modules: Vec<RenderShaderModuleManifestEntry>,
    #[serde(default)]
    pub gpu_scene_buffers: Option<RenderGpuSceneBuffersManifestEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RenderPipelineManifestEntry {
    pub id: String,
    pub shader_module: String,
    pub vertex_entry: String,
    pub fragment_entry: String,
    pub primitive: RenderPrimitiveManifestEntry,
    #[serde(default, alias = "node-target")]
    pub node_target: Option<String>,
    #[serde(default, alias = "shader-mode")]
    pub shader_mode: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RenderPrimitiveManifestEntry {
    pub topology: RenderPrimitiveTopology,
    pub cull_mode: RenderCullMode,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RenderPassManifestEntry {
    pub name: String,
    pub pipeline: String,
    #[serde(default)]
    pub draw_phase: String,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub pass_type: Option<String>,
    #[serde(default, alias = "node-target")]
    pub node_target: Option<String>,
    #[serde(default, alias = "shader-mode")]
    pub shader_mode: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RenderResourceContractManifestEntry {
    pub id: String,
    pub kind: String,
    #[serde(default)]
    pub external: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RenderPassContractManifestEntry {
    pub id: String,
    pub pass: String,
    #[serde(default)]
    pub reads: Vec<String>,
    #[serde(default)]
    pub writes: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct RenderContractsManifestEntry {
    #[serde(default)]
    pub resources: Vec<RenderContractsResourceEntry>,
    #[serde(default)]
    pub passes: Vec<RenderContractsPassEntry>,
    #[serde(
        default,
        alias = "default-profile",
        alias = "default_profile_contracts"
    )]
    pub default_profile: Option<RenderDefaultProfileContractsManifestEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RenderContractsResourceEntry {
    pub id: String,
    pub kind: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RenderContractsPassEntry {
    pub name: String,
    #[serde(default)]
    pub reads: Vec<String>,
    #[serde(default)]
    pub writes: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct RenderDefaultProfileContractsManifestEntry {
    #[serde(default)]
    pub lighting: Option<RenderLightingContractManifestEntry>,
    #[serde(default)]
    pub reflections: Option<RenderReflectionFallbackContractManifestEntry>,
    #[serde(default)]
    pub temporal: Option<RenderTemporalStackContractManifestEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct RenderLightingContractManifestEntry {
    #[serde(default)]
    pub pbr: Option<RenderToggleContractManifestEntry>,
    #[serde(default)]
    pub hdr: Option<RenderToggleContractManifestEntry>,
    #[serde(default)]
    pub tonemap: Option<RenderTonemapContractManifestEntry>,
    #[serde(default)]
    pub clustered_lighting: Option<RenderClusteredLightingContractManifestEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct RenderToggleContractManifestEntry {
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct RenderTonemapContractManifestEntry {
    #[serde(default)]
    pub operator: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct RenderClusteredLightingContractManifestEntry {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub max_lights_per_cluster: u32,
    #[serde(default)]
    pub shadow: Option<RenderShadowContractManifestEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct RenderShadowContractManifestEntry {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub cascade_count: u32,
    #[serde(default)]
    pub atlas_resolution: u32,
    #[serde(default)]
    pub quality_tier: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct RenderReflectionFallbackContractManifestEntry {
    #[serde(default)]
    pub fallback_chain: Vec<String>,
    #[serde(default)]
    pub planar_budget: Option<RenderReflectionPlanarBudgetManifestEntry>,
    #[serde(default)]
    pub ssr_budget: Option<RenderReflectionSsrBudgetManifestEntry>,
    #[serde(default)]
    pub probe_budget: Option<RenderReflectionProbeBudgetManifestEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct RenderReflectionPlanarBudgetManifestEntry {
    #[serde(default)]
    pub max_planes: u32,
    #[serde(default)]
    pub resolution_scale: f32,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct RenderReflectionSsrBudgetManifestEntry {
    #[serde(default)]
    pub max_steps: u32,
    #[serde(default)]
    pub max_rays_per_pixel: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct RenderReflectionProbeBudgetManifestEntry {
    #[serde(default)]
    pub max_active_probes: u32,
    #[serde(default)]
    pub update_ratio: f32,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct RenderTemporalStackContractManifestEntry {
    #[serde(default)]
    pub motion_vectors: Option<RenderToggleContractManifestEntry>,
    #[serde(default)]
    pub taa: Option<RenderTaaContractManifestEntry>,
    #[serde(default)]
    pub upscaling: Option<RenderUpscalingContractManifestEntry>,
    #[serde(default)]
    pub reactive_mask: Option<RenderToggleContractManifestEntry>,
    #[serde(default)]
    pub disocclusion_mask: Option<RenderToggleContractManifestEntry>,
    #[serde(default)]
    pub dynamic_resolution_policy: Option<RenderDynamicResolutionPolicyManifestEntry>,
    #[serde(default)]
    pub metrics: Option<RenderTemporalMetricsManifestEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct RenderTaaContractManifestEntry {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub history_frames: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct RenderUpscalingContractManifestEntry {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub mode: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct RenderDynamicResolutionPolicyManifestEntry {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub min_scale: f32,
    #[serde(default)]
    pub max_scale: f32,
    #[serde(default)]
    pub target_frame_time_ms: f32,
    #[serde(default)]
    pub scale_step: f32,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct RenderTemporalMetricsManifestEntry {
    #[serde(default)]
    pub window_frames: u32,
    #[serde(default)]
    pub report_interval_ms: u32,
    #[serde(default)]
    pub max_jitter_ms: f32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RenderShaderModuleManifestEntry {
    pub id: String,
    #[serde(default)]
    pub path: String,
    #[serde(default, alias = "generated-path")]
    pub generated_path: Option<String>,
    #[serde(default, alias = "gpu-path")]
    pub gpu_path: Option<String>,
    #[serde(default, alias = "node-target")]
    pub node_target: Option<String>,
    #[serde(default, alias = "shader-mode")]
    pub shader_mode: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct RenderGpuSceneBufferContractManifestEntry {
    pub resource_id: String,
    #[serde(default)]
    pub kind: String,
    pub stride_bytes: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct RenderHiZOcclusionTierManifestEntry {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub tier: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct RenderGpuSceneBuffersManifestEntry {
    #[serde(default)]
    pub transforms: Option<RenderGpuSceneBufferContractManifestEntry>,
    #[serde(default)]
    pub bounds: Option<RenderGpuSceneBufferContractManifestEntry>,
    #[serde(default)]
    pub draw_records: Option<RenderGpuSceneBufferContractManifestEntry>,
    #[serde(default)]
    pub material_refs: Option<RenderGpuSceneBufferContractManifestEntry>,
    #[serde(default)]
    pub hiz_occlusion: Option<RenderHiZOcclusionTierManifestEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ShaderBundleDocument {
    pub schema_version: String,
    pub shader_modules: Vec<ShaderBundleModuleEntry>,
    pub prewarm_groups: Vec<ShaderBundlePrewarmGroupEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ShaderBundleModuleEntry {
    pub id: String,
    #[serde(default)]
    pub path: String,
    #[serde(default, alias = "generated-path")]
    pub generated_path: Option<String>,
    #[serde(default, alias = "gpu-path")]
    pub gpu_path: Option<String>,
    #[serde(default, alias = "node-target")]
    pub node_target: Option<String>,
    #[serde(default, alias = "shader-mode")]
    pub shader_mode: Option<String>,
    #[serde(default)]
    pub entrypoints: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ShaderBundlePrewarmGroupEntry {
    pub id: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default, alias = "modules")]
    pub shader_modules: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeShaderSelection {
    pub render_schema_version: String,
    pub shader_bundle_schema_version: String,
    pub pipelines: Vec<RuntimePipelineShaderSelection>,
    pub frame_graph: Vec<RuntimeFrameGraphPassSelection>,
    pub resource_contracts: Vec<RuntimeResourceContractSelection>,
    pub prewarm_groups: Vec<RuntimePrewarmGroupSelection>,
    pub gpu_scene_buffers: RuntimeGpuSceneBufferContracts,
    pub default_profile_contracts: RuntimeDefaultProfileContracts,
    pub compute_pass_manifest_ready: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeDefaultProfileContracts {
    pub lighting: RuntimeLightingContract,
    pub reflections: RuntimeReflectionFallbackContract,
    pub temporal: RuntimeTemporalStackContract,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeLightingContract {
    pub pbr_enabled: bool,
    pub hdr_enabled: bool,
    pub tonemap_operator: String,
    pub clustered_lighting_enabled: bool,
    pub max_lights_per_cluster: u32,
    pub shadows_enabled: bool,
    pub shadow_cascade_count: u32,
    pub shadow_atlas_resolution: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeReflectionFallbackContract {
    pub fallback_chain: Vec<String>,
    pub planar_max_planes: u32,
    pub planar_resolution_scale: f32,
    pub ssr_max_steps: u32,
    pub ssr_max_rays_per_pixel: u32,
    pub probe_max_active_probes: u32,
    pub probe_update_ratio: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeTemporalStackContract {
    pub motion_vectors_enabled: bool,
    pub taa_enabled: bool,
    pub temporal_upscaling_enabled: bool,
    pub reactive_mask_enabled: bool,
    pub disocclusion_mask_enabled: bool,
    pub dynamic_resolution_policy: RuntimeDynamicResolutionPolicyContract,
    pub metrics: RuntimeTemporalMetricsContract,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeDynamicResolutionPolicyContract {
    pub enabled: bool,
    pub min_scale: f32,
    pub max_scale: f32,
    pub target_frame_time_ms: f32,
    pub scale_step: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeTemporalMetricsContract {
    pub window_frames: u32,
    pub report_interval_ms: u32,
    pub max_jitter_ms: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimePipelineShaderSelection {
    pub pipeline_id: String,
    pub shader_module_id: String,
    pub shader_path: String,
    pub vertex_entry: String,
    pub fragment_entry: String,
    pub topology: RenderPrimitiveTopology,
    pub cull_mode: RenderCullMode,
    pub node_target: Option<String>,
    pub shader_mode: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeFrameGraphPassSelection {
    pub name: String,
    pub pipeline_id: String,
    pub draw_phase: String,
    pub depends_on: Vec<String>,
    pub pass_contract_id: String,
    pub reads: Vec<String>,
    pub writes: Vec<String>,
    pub is_compute_pass: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeResourceContractSelection {
    pub id: String,
    pub kind: String,
    pub external: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimePrewarmGroupSelection {
    pub id: String,
    pub required: bool,
    pub shader_modules: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeGpuSceneBufferContract {
    pub resource_id: String,
    pub kind: String,
    pub stride_bytes: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeHiZOcclusionTierSelection {
    pub enabled: bool,
    pub tier: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeGpuSceneBufferContracts {
    pub transforms: RuntimeGpuSceneBufferContract,
    pub bounds: RuntimeGpuSceneBufferContract,
    pub draw_records: RuntimeGpuSceneBufferContract,
    pub material_refs: RuntimeGpuSceneBufferContract,
    pub hiz_occlusion: RuntimeHiZOcclusionTierSelection,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GpuSceneTransformContract {
    pub translation: [f32; 3],
    pub scale: [f32; 3],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GpuSceneBoundsContract {
    pub center: [f32; 3],
    pub extents: [f32; 3],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GpuSceneDrawRecordContract {
    pub transform_index: u32,
    pub bounds_index: u32,
    pub material_ref_index: u32,
    pub instance_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GpuSceneMaterialRefContract {
    pub material_slot: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneVisibilityCandidate {
    pub transform: GpuSceneTransformContract,
    pub bounds: GpuSceneBoundsContract,
    pub draw_record: GpuSceneDrawRecordContract,
    pub material_ref: GpuSceneMaterialRefContract,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VisibilityStageTelemetry {
    pub candidate_draws: u32,
    pub visible_draws: u32,
    pub culled_ratio: f32,
    pub indirect_draw_count: u32,
    pub hiz_occlusion_tier_enabled: bool,
    pub cpu_fallback_used: bool,
    pub indirect_submission_path_default: bool,
}

pub fn simulate_visibility_stage_telemetry(
    candidates: &[SceneVisibilityCandidate],
    view_width: f32,
    view_height: f32,
    hiz_occlusion_tier_enabled: bool,
) -> VisibilityStageTelemetry {
    let mut candidate_draws = 0u32;
    let mut visible_draws = 0u32;

    let view_width = view_width.max(1.0);
    let view_height = view_height.max(1.0);
    for (index, candidate) in candidates.iter().enumerate() {
        if candidate.draw_record.instance_count == 0 {
            continue;
        }
        candidate_draws = candidate_draws.saturating_add(1);

        let center_x = candidate.bounds.center[0];
        let center_y = candidate.bounds.center[1];
        let extent_x = candidate.bounds.extents[0].abs().max(0.01);
        let extent_y = candidate.bounds.extents[1].abs().max(0.01);
        let min_x = center_x - extent_x;
        let max_x = center_x + extent_x;
        let min_y = center_y - extent_y;
        let max_y = center_y + extent_y;
        let in_frustum =
            max_x >= 0.0 && max_y >= 0.0 && min_x <= view_width && min_y <= view_height;
        if !in_frustum {
            continue;
        }

        // Hi-Z tier can optionally cull additional instances before submission.
        if hiz_occlusion_tier_enabled && index % 4 == 3 {
            continue;
        }
        visible_draws = visible_draws.saturating_add(candidate.draw_record.instance_count);
    }

    let culled_ratio = if candidate_draws == 0 {
        0.0
    } else {
        (candidate_draws.saturating_sub(visible_draws)) as f32 / candidate_draws as f32
    };

    VisibilityStageTelemetry {
        candidate_draws,
        visible_draws,
        culled_ratio,
        indirect_draw_count: visible_draws,
        hiz_occlusion_tier_enabled,
        cpu_fallback_used: false,
        indirect_submission_path_default: true,
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ShaderVariantSelector {
    node_target: Option<String>,
    shader_mode: Option<String>,
}

impl ShaderVariantSelector {
    fn from_fields(node_target: Option<&str>, shader_mode: Option<&str>) -> Self {
        Self {
            node_target: normalize_optional(node_target),
            shader_mode: normalize_shader_mode(shader_mode),
        }
    }

    fn with_fallback(&self, node_target: Option<&str>, shader_mode: Option<&str>) -> Self {
        Self {
            node_target: self
                .node_target
                .clone()
                .or_else(|| normalize_optional(node_target)),
            shader_mode: self
                .shader_mode
                .clone()
                .or_else(|| normalize_shader_mode(shader_mode)),
        }
    }

    fn describe(&self) -> String {
        let node_target = self
            .node_target
            .as_ref()
            .map_or_else(|| "<none>".to_string(), |value| value.clone());
        let shader_mode = self
            .shader_mode
            .as_ref()
            .map_or_else(|| "<none>".to_string(), |value| value.clone());
        format!("node_target='{node_target}', shader_mode='{shader_mode}'")
    }
}

pub fn resolve_runtime_shader_selection(
    render: &RenderManifestDocument,
    bundle: &ShaderBundleDocument,
) -> Result<RuntimeShaderSelection, String> {
    validate_schema(
        "render manifest",
        render.schema_version.as_str(),
        RENDER_MANIFEST_SCHEMA,
    )?;
    validate_schema(
        "shader bundle",
        bundle.schema_version.as_str(),
        SHADER_BUNDLE_SCHEMA,
    )?;
    if render.frame_graph.is_empty() {
        return Err("render manifest frame_graph is empty".to_string());
    }
    let resolved_order = validate_and_resolve_frame_graph_order(render.frame_graph.as_slice())?;
    let resolved_resource_contracts = resolve_resource_contract_entries(render);
    let resolved_pass_contracts = resolve_pass_contract_entries(render);
    let resource_contracts = validate_resource_contracts(resolved_resource_contracts.as_slice())?;
    let pass_contracts_by_pass = validate_pass_contracts(
        render.frame_graph.as_slice(),
        resolved_pass_contracts.as_slice(),
        resource_contracts.as_slice(),
    )?;
    let gpu_scene_buffers = resolve_gpu_scene_buffer_contracts(render.gpu_scene_buffers.as_ref())?;
    let default_profile_contracts = resolve_default_profile_contracts(render)?;
    let prewarm_groups = validate_prewarm_groups(
        bundle.prewarm_groups.as_slice(),
        bundle.shader_modules.as_slice(),
    )?;

    let mut pipelines = Vec::<RuntimePipelineShaderSelection>::new();
    let mut frame_graph =
        Vec::<RuntimeFrameGraphPassSelection>::with_capacity(render.frame_graph.len());
    let mut compute_pass_manifest_ready = false;
    let mut selected_shader_modules = BTreeSet::<String>::new();

    for pass_index in resolved_order {
        let pass = &render.frame_graph[pass_index];
        let pass_selector = ShaderVariantSelector::from_fields(
            pass.node_target.as_deref(),
            pass.shader_mode.as_deref(),
        );
        let pipeline = select_variant(
            &render.pipelines,
            pass.pipeline.as_str(),
            &pass_selector,
            |entry| entry.id.as_str(),
            |entry| entry.node_target.as_deref(),
            |entry| entry.shader_mode.as_deref(),
            "render manifest pipeline",
        )?;
        if pipeline.vertex_entry.trim().is_empty() {
            return Err(format!(
                "render pipeline '{}' has empty vertex_entry",
                pipeline.id
            ));
        }
        if pipeline.fragment_entry.trim().is_empty() {
            return Err(format!(
                "render pipeline '{}' has empty fragment_entry",
                pipeline.id
            ));
        }

        let pipeline_selector = pass_selector.with_fallback(
            pipeline.node_target.as_deref(),
            pipeline.shader_mode.as_deref(),
        );
        let render_shader = select_variant(
            &render.shader_modules,
            pipeline.shader_module.as_str(),
            &pipeline_selector,
            |entry| entry.id.as_str(),
            |entry| entry.node_target.as_deref(),
            |entry| entry.shader_mode.as_deref(),
            "render manifest shader module",
        )?;
        let shader_selector = pipeline_selector.with_fallback(
            render_shader.node_target.as_deref(),
            render_shader.shader_mode.as_deref(),
        );
        let bundle_shader = select_variant(
            &bundle.shader_modules,
            pipeline.shader_module.as_str(),
            &shader_selector,
            |entry| entry.id.as_str(),
            |entry| entry.node_target.as_deref(),
            |entry| entry.shader_mode.as_deref(),
            "shader bundle module",
        )?;
        let final_selector = shader_selector.with_fallback(
            bundle_shader.node_target.as_deref(),
            bundle_shader.shader_mode.as_deref(),
        );
        let render_shader_path = resolve_shader_module_path(
            "render manifest shader module",
            render_shader.id.as_str(),
            render_shader.path.as_str(),
            render_shader.generated_path.as_deref(),
            render_shader.gpu_path.as_deref(),
            &final_selector,
        )?;
        let bundle_shader_path = resolve_shader_module_path(
            "shader bundle module",
            bundle_shader.id.as_str(),
            bundle_shader.path.as_str(),
            bundle_shader.generated_path.as_deref(),
            bundle_shader.gpu_path.as_deref(),
            &final_selector,
        )?;
        if render_shader_path != bundle_shader_path {
            return Err(format!(
                "shader module '{}' path mismatch between manifests for {}: render='{}', bundle='{}'",
                pipeline.shader_module,
                final_selector.describe(),
                render_shader_path,
                bundle_shader_path
            ));
        }
        if !bundle_shader
            .entrypoints
            .iter()
            .any(|entry| entry == pipeline.vertex_entry.as_str())
        {
            return Err(format!(
                "shader bundle module '{}' is missing vertex entrypoint '{}'",
                bundle_shader.id, pipeline.vertex_entry
            ));
        }
        if !bundle_shader
            .entrypoints
            .iter()
            .any(|entry| entry == pipeline.fragment_entry.as_str())
        {
            return Err(format!(
                "shader bundle module '{}' is missing fragment entrypoint '{}'",
                bundle_shader.id, pipeline.fragment_entry
            ));
        }

        let runtime_pipeline_id = runtime_pipeline_id(pipeline.id.as_str(), &final_selector);
        if !pipelines
            .iter()
            .any(|candidate| candidate.pipeline_id == runtime_pipeline_id)
        {
            selected_shader_modules.insert(pipeline.shader_module.clone());
            pipelines.push(RuntimePipelineShaderSelection {
                pipeline_id: runtime_pipeline_id.clone(),
                shader_module_id: pipeline.shader_module.clone(),
                shader_path: bundle_shader_path,
                vertex_entry: pipeline.vertex_entry.clone(),
                fragment_entry: pipeline.fragment_entry.clone(),
                topology: pipeline.primitive.topology.clone(),
                cull_mode: pipeline.primitive.cull_mode.clone(),
                node_target: final_selector.node_target.clone(),
                shader_mode: final_selector.shader_mode.clone(),
            });
        }

        let is_compute_pass = pass
            .pass_type
            .as_ref()
            .is_some_and(|kind| kind.eq_ignore_ascii_case("compute"))
            || pass.draw_phase.eq_ignore_ascii_case("compute")
            || pass.name.to_ascii_lowercase().contains("compute");
        let pass_contract = pass_contracts_by_pass
            .get(pass.name.as_str())
            .ok_or_else(|| {
                format!(
                    "render manifest pass '{}' is missing a pass contract",
                    pass.name
                )
            })?;
        compute_pass_manifest_ready |= is_compute_pass;
        frame_graph.push(RuntimeFrameGraphPassSelection {
            name: pass.name.clone(),
            pipeline_id: runtime_pipeline_id,
            draw_phase: pass.draw_phase.clone(),
            depends_on: pass.depends_on.clone(),
            pass_contract_id: pass_contract.id.clone(),
            reads: pass_contract.reads.clone(),
            writes: pass_contract.writes.clone(),
            is_compute_pass,
        });
    }

    let runtime_prewarm_groups =
        resolve_runtime_prewarm_groups(prewarm_groups.as_slice(), &selected_shader_modules)?;

    Ok(RuntimeShaderSelection {
        render_schema_version: render.schema_version.clone(),
        shader_bundle_schema_version: bundle.schema_version.clone(),
        pipelines,
        frame_graph,
        resource_contracts,
        prewarm_groups: runtime_prewarm_groups,
        gpu_scene_buffers,
        default_profile_contracts,
        compute_pass_manifest_ready,
    })
}

#[derive(Debug, Clone)]
struct ValidatedPassContract {
    id: String,
    reads: Vec<String>,
    writes: Vec<String>,
}

#[derive(Debug, Clone)]
struct ValidatedPrewarmGroup {
    id: String,
    required: bool,
    shader_modules: Vec<String>,
}

fn resolve_resource_contract_entries(
    render: &RenderManifestDocument,
) -> Vec<RenderResourceContractManifestEntry> {
    if !render.resource_contracts.is_empty() {
        return render.resource_contracts.clone();
    }
    render
        .contracts
        .as_ref()
        .map(|contracts| {
            contracts
                .resources
                .iter()
                .map(|resource| RenderResourceContractManifestEntry {
                    // Contract resources that are not render targets are treated as externally
                    // provided inputs to the frame graph (scene buffers, uniforms, samplers).
                    id: resource.id.clone(),
                    kind: resource.kind.clone(),
                    external: !resource.kind.to_ascii_lowercase().contains("target"),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn resolve_pass_contract_entries(
    render: &RenderManifestDocument,
) -> Vec<RenderPassContractManifestEntry> {
    if !render.pass_contracts.is_empty() {
        return render.pass_contracts.clone();
    }
    render
        .contracts
        .as_ref()
        .map(|contracts| {
            contracts
                .passes
                .iter()
                .map(|pass| RenderPassContractManifestEntry {
                    id: format!(
                        "{}_contract",
                        normalize_contract_id_fragment(pass.name.as_str())
                    ),
                    pass: pass.name.clone(),
                    reads: pass.reads.clone(),
                    writes: pass.writes.clone(),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn resolve_default_profile_contracts(
    render: &RenderManifestDocument,
) -> Result<RuntimeDefaultProfileContracts, String> {
    let default_profile = render
        .contracts
        .as_ref()
        .and_then(|contracts| contracts.default_profile.as_ref())
        .ok_or_else(|| {
            "render manifest contracts.default_profile is required for WL05/WL06/WL07 validation"
                .to_string()
        })?;
    let contracts = material_graph_default_profile_contracts_from_manifest(default_profile)?;
    validate_default_profile_contracts(&contracts)
        .map_err(|err| format!("render manifest default profile validation failed: {err}"))?;

    Ok(RuntimeDefaultProfileContracts {
        lighting: RuntimeLightingContract {
            pbr_enabled: contracts.lighting.pbr_enabled,
            hdr_enabled: contracts.lighting.hdr_enabled,
            tonemap_operator: contracts.lighting.tonemap_operator.to_string(),
            clustered_lighting_enabled: contracts.lighting.clustered_lighting.enabled,
            max_lights_per_cluster: contracts.lighting.clustered_lighting.max_lights_per_cluster,
            shadows_enabled: contracts.lighting.clustered_lighting.shadow.enabled,
            shadow_cascade_count: contracts.lighting.clustered_lighting.shadow.cascade_count,
            shadow_atlas_resolution: contracts
                .lighting
                .clustered_lighting
                .shadow
                .atlas_resolution,
        },
        reflections: RuntimeReflectionFallbackContract {
            fallback_chain: contracts
                .reflections
                .fallback_chain
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            planar_max_planes: contracts.reflections.planar_budget.max_planes,
            planar_resolution_scale: contracts.reflections.planar_budget.resolution_scale,
            ssr_max_steps: contracts.reflections.ssr_budget.max_steps,
            ssr_max_rays_per_pixel: contracts.reflections.ssr_budget.max_rays_per_pixel,
            probe_max_active_probes: contracts.reflections.probe_budget.max_active_probes,
            probe_update_ratio: contracts.reflections.probe_budget.update_ratio,
        },
        temporal: RuntimeTemporalStackContract {
            motion_vectors_enabled: contracts.temporal.motion_vectors_enabled,
            taa_enabled: contracts.temporal.taa_enabled,
            temporal_upscaling_enabled: contracts.temporal.temporal_upscaling_enabled,
            reactive_mask_enabled: contracts.temporal.reactive_mask_enabled,
            disocclusion_mask_enabled: contracts.temporal.disocclusion_mask_enabled,
            dynamic_resolution_policy: RuntimeDynamicResolutionPolicyContract {
                enabled: contracts.temporal.dynamic_resolution_policy.enabled,
                min_scale: contracts.temporal.dynamic_resolution_policy.min_scale,
                max_scale: contracts.temporal.dynamic_resolution_policy.max_scale,
                target_frame_time_ms: contracts
                    .temporal
                    .dynamic_resolution_policy
                    .target_frame_time_ms,
                scale_step: contracts.temporal.dynamic_resolution_policy.scale_step,
            },
            metrics: RuntimeTemporalMetricsContract {
                window_frames: contracts.temporal.metrics.window_frames,
                report_interval_ms: contracts.temporal.metrics.report_interval_ms,
                max_jitter_ms: contracts.temporal.metrics.max_jitter_ms,
            },
        },
    })
}

fn material_graph_default_profile_contracts_from_manifest(
    default_profile: &RenderDefaultProfileContractsManifestEntry,
) -> Result<DefaultProfileContractsV1, String> {
    let lighting = default_profile.lighting.as_ref().ok_or_else(|| {
        "render manifest contracts.default_profile.lighting is required".to_string()
    })?;
    let clustered = lighting.clustered_lighting.as_ref().ok_or_else(|| {
        "WL05 validation failed: contracts.default_profile.lighting.clustered_lighting is required"
            .to_string()
    })?;
    let shadow = clustered.shadow.as_ref().ok_or_else(|| {
        "WL05 validation failed: contracts.default_profile.lighting.clustered_lighting.shadow is required"
            .to_string()
    })?;
    let tonemap_operator = lighting
        .tonemap
        .as_ref()
        .and_then(|entry| normalize_optional(entry.operator.as_deref()))
        .ok_or_else(|| {
            "WL05 validation failed: contracts.default_profile.lighting.tonemap.operator is required"
                .to_string()
        })?;

    let reflections = default_profile.reflections.as_ref().ok_or_else(|| {
        "render manifest contracts.default_profile.reflections is required".to_string()
    })?;
    let planar_budget = reflections.planar_budget.as_ref().ok_or_else(|| {
        "WL06 validation failed: reflections.planar_budget is required".to_string()
    })?;
    let ssr_budget = reflections
        .ssr_budget
        .as_ref()
        .ok_or_else(|| "WL06 validation failed: reflections.ssr_budget is required".to_string())?;
    let probe_budget = reflections.probe_budget.as_ref().ok_or_else(|| {
        "WL06 validation failed: reflections.probe_budget is required".to_string()
    })?;

    let temporal = default_profile.temporal.as_ref().ok_or_else(|| {
        "render manifest contracts.default_profile.temporal is required".to_string()
    })?;
    let taa = temporal
        .taa
        .as_ref()
        .ok_or_else(|| "WL07 validation failed: temporal.taa is required".to_string())?;
    let upscaling = temporal
        .upscaling
        .as_ref()
        .ok_or_else(|| "WL07 validation failed: temporal.upscaling is required".to_string())?;
    let upscaling_mode = normalize_optional(upscaling.mode.as_deref())
        .ok_or_else(|| "WL07 validation failed: temporal.upscaling.mode is required".to_string())?;
    let dynamic_policy = temporal.dynamic_resolution_policy.as_ref().ok_or_else(|| {
        "WL07 validation failed: temporal.dynamic_resolution_policy is required".to_string()
    })?;
    let metrics = temporal
        .metrics
        .as_ref()
        .ok_or_else(|| "WL07 validation failed: temporal.metrics is required".to_string())?;

    let raw_contracts = serde_json::json!({
        "schema_version": 1,
        "profile": "default",
        "lighting": {
            "pbr_enabled": lighting.pbr.as_ref().is_some_and(|entry| entry.enabled),
            "hdr_enabled": lighting.hdr.as_ref().is_some_and(|entry| entry.enabled),
            "tonemap_operator": tonemap_operator,
            "clustered_lighting": {
                "enabled": clustered.enabled,
                "max_lights_per_cluster": clustered.max_lights_per_cluster,
                "shadow": {
                    "enabled": shadow.enabled,
                    "cascade_count": shadow.cascade_count,
                    "atlas_resolution": shadow.atlas_resolution,
                    "quality_tier": normalize_optional(shadow.quality_tier.as_deref()).unwrap_or_else(|| "high".to_string()),
                }
            }
        },
        "reflections": {
            "fallback_chain": reflections.fallback_chain,
            "planar_budget": {
                "max_planes": planar_budget.max_planes,
                "resolution_scale": planar_budget.resolution_scale
            },
            "ssr_budget": {
                "max_steps": ssr_budget.max_steps,
                "max_rays_per_pixel": ssr_budget.max_rays_per_pixel
            },
            "probe_budget": {
                "max_active_probes": probe_budget.max_active_probes,
                "update_ratio": probe_budget.update_ratio
            }
        },
        "temporal": {
            "motion_vectors_enabled": temporal.motion_vectors.as_ref().is_some_and(|entry| entry.enabled),
            "taa_enabled": taa.enabled,
            "temporal_upscaling_enabled": upscaling.enabled,
            "temporal_upscaler_mode": upscaling_mode,
            "reactive_mask_enabled": temporal.reactive_mask.as_ref().is_some_and(|entry| entry.enabled),
            "disocclusion_mask_enabled": temporal.disocclusion_mask.as_ref().is_some_and(|entry| entry.enabled),
            "dynamic_resolution_policy": {
                "enabled": dynamic_policy.enabled,
                "min_scale": dynamic_policy.min_scale,
                "max_scale": dynamic_policy.max_scale,
                "target_frame_time_ms": dynamic_policy.target_frame_time_ms,
                "scale_step": dynamic_policy.scale_step
            },
            "metrics": {
                "window_frames": metrics.window_frames,
                "report_interval_ms": metrics.report_interval_ms,
                "max_jitter_ms": metrics.max_jitter_ms
            }
        }
    });

    serde_json::from_value::<DefaultProfileContractsV1>(raw_contracts).map_err(|err| {
        format!("render manifest contracts.default_profile type validation failed: {err}")
    })
}

fn normalize_contract_id_fragment(raw: &str) -> String {
    let mut normalized = String::new();
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            normalized.push(ch.to_ascii_lowercase());
        } else {
            normalized.push('_');
        }
    }
    while normalized.contains("__") {
        normalized = normalized.replace("__", "_");
    }
    let trimmed = normalized.trim_matches('_');
    if trimmed.is_empty() {
        "unnamed".to_string()
    } else {
        trimmed.to_string()
    }
}

fn resolve_gpu_scene_buffer_contracts(
    manifest: Option<&RenderGpuSceneBuffersManifestEntry>,
) -> Result<RuntimeGpuSceneBufferContracts, String> {
    let transforms = resolve_gpu_scene_buffer_contract(
        manifest.and_then(|entry| entry.transforms.as_ref()),
        "scene_transforms",
        "storage-buffer",
        48,
        "transforms",
    )?;
    let bounds = resolve_gpu_scene_buffer_contract(
        manifest.and_then(|entry| entry.bounds.as_ref()),
        "scene_bounds",
        "storage-buffer",
        32,
        "bounds",
    )?;
    let draw_records = resolve_gpu_scene_buffer_contract(
        manifest.and_then(|entry| entry.draw_records.as_ref()),
        "scene_draw_records",
        "storage-buffer",
        32,
        "draw_records",
    )?;
    let material_refs = resolve_gpu_scene_buffer_contract(
        manifest.and_then(|entry| entry.material_refs.as_ref()),
        "scene_material_refs",
        "storage-buffer",
        16,
        "material_refs",
    )?;
    let hiz_occlusion = manifest
        .and_then(|entry| entry.hiz_occlusion.as_ref())
        .map(|entry| RuntimeHiZOcclusionTierSelection {
            enabled: entry.enabled,
            tier: normalize_optional(entry.tier.as_deref()),
        })
        .unwrap_or(RuntimeHiZOcclusionTierSelection {
            enabled: false,
            tier: None,
        });

    Ok(RuntimeGpuSceneBufferContracts {
        transforms,
        bounds,
        draw_records,
        material_refs,
        hiz_occlusion,
    })
}

fn resolve_gpu_scene_buffer_contract(
    manifest: Option<&RenderGpuSceneBufferContractManifestEntry>,
    default_resource_id: &str,
    default_kind: &str,
    default_stride_bytes: u32,
    label: &str,
) -> Result<RuntimeGpuSceneBufferContract, String> {
    let Some(entry) = manifest else {
        return Ok(RuntimeGpuSceneBufferContract {
            resource_id: default_resource_id.to_string(),
            kind: default_kind.to_string(),
            stride_bytes: default_stride_bytes,
        });
    };

    let resource_id = normalize_optional(Some(entry.resource_id.as_str())).ok_or_else(|| {
        format!("render manifest gpu_scene_buffers.{label} has empty resource_id")
    })?;
    let kind =
        normalize_optional(Some(entry.kind.as_str())).unwrap_or_else(|| default_kind.to_string());
    if entry.stride_bytes == 0 {
        return Err(format!(
            "render manifest gpu_scene_buffers.{label} has invalid stride_bytes=0"
        ));
    }

    Ok(RuntimeGpuSceneBufferContract {
        resource_id,
        kind,
        stride_bytes: entry.stride_bytes,
    })
}

fn validate_resource_contracts(
    resources: &[RenderResourceContractManifestEntry],
) -> Result<Vec<RuntimeResourceContractSelection>, String> {
    if resources.is_empty() {
        return Err("render manifest resource_contracts is empty".to_string());
    }

    let mut seen_ids = HashSet::<&str>::new();
    let mut runtime_resources = Vec::with_capacity(resources.len());
    for resource in resources {
        if resource.id.trim().is_empty() {
            return Err("render manifest resource_contracts contains an empty id".to_string());
        }
        if resource.kind.trim().is_empty() {
            return Err(format!(
                "render manifest resource_contract '{}' has empty kind",
                resource.id
            ));
        }
        if !seen_ids.insert(resource.id.as_str()) {
            return Err(format!(
                "render manifest resource_contracts contains duplicate id '{}'",
                resource.id
            ));
        }
        runtime_resources.push(RuntimeResourceContractSelection {
            id: resource.id.clone(),
            kind: resource.kind.clone(),
            external: resource.external,
        });
    }

    Ok(runtime_resources)
}

fn validate_pass_contracts(
    frame_graph: &[RenderPassManifestEntry],
    pass_contracts: &[RenderPassContractManifestEntry],
    resources: &[RuntimeResourceContractSelection],
) -> Result<HashMap<String, ValidatedPassContract>, String> {
    if pass_contracts.is_empty() {
        return Err("render manifest pass_contracts is empty".to_string());
    }

    let pass_names = frame_graph
        .iter()
        .map(|pass| pass.name.as_str())
        .collect::<HashSet<_>>();
    let resource_by_id = resources
        .iter()
        .map(|resource| (resource.id.as_str(), resource))
        .collect::<HashMap<_, _>>();
    let mut seen_contract_ids = HashSet::<&str>::new();
    let mut contracts_by_pass = HashMap::<String, ValidatedPassContract>::new();

    for contract in pass_contracts {
        if contract.id.trim().is_empty() {
            return Err("render manifest pass_contracts contains an empty id".to_string());
        }
        if contract.pass.trim().is_empty() {
            return Err(format!(
                "render manifest pass_contract '{}' has empty pass reference",
                contract.id
            ));
        }
        if !seen_contract_ids.insert(contract.id.as_str()) {
            return Err(format!(
                "render manifest pass_contracts contains duplicate id '{}'",
                contract.id
            ));
        }
        if !pass_names.contains(contract.pass.as_str()) {
            return Err(format!(
                "render manifest pass_contract '{}' references unknown frame_graph pass '{}'",
                contract.id, contract.pass
            ));
        }
        if contracts_by_pass.contains_key(contract.pass.as_str()) {
            return Err(format!(
                "render manifest pass '{}' has multiple pass contracts",
                contract.pass
            ));
        }
        validate_resource_references(
            contract.id.as_str(),
            "reads",
            contract.reads.as_slice(),
            &resource_by_id,
        )?;
        validate_resource_references(
            contract.id.as_str(),
            "writes",
            contract.writes.as_slice(),
            &resource_by_id,
        )?;
        contracts_by_pass.insert(
            contract.pass.clone(),
            ValidatedPassContract {
                id: contract.id.clone(),
                reads: contract.reads.clone(),
                writes: contract.writes.clone(),
            },
        );
    }

    for pass in frame_graph {
        if !contracts_by_pass.contains_key(pass.name.as_str()) {
            return Err(format!(
                "render manifest pass '{}' has no pass_contract",
                pass.name
            ));
        }
    }

    validate_resource_dependencies(frame_graph, &contracts_by_pass, &resource_by_id)?;
    Ok(contracts_by_pass)
}

fn validate_resource_references(
    pass_contract_id: &str,
    label: &str,
    resource_ids: &[String],
    resources_by_id: &HashMap<&str, &RuntimeResourceContractSelection>,
) -> Result<(), String> {
    let mut seen = HashSet::<&str>::new();
    for resource_id in resource_ids {
        if resource_id.trim().is_empty() {
            return Err(format!(
                "render manifest pass_contract '{}' has empty resource id in {}",
                pass_contract_id, label
            ));
        }
        if !seen.insert(resource_id.as_str()) {
            return Err(format!(
                "render manifest pass_contract '{}' has duplicate resource '{}' in {}",
                pass_contract_id, resource_id, label
            ));
        }
        if !resources_by_id.contains_key(resource_id.as_str()) {
            return Err(format!(
                "render manifest pass_contract '{}' references unresolved resource '{}' in {}",
                pass_contract_id, resource_id, label
            ));
        }
    }
    Ok(())
}

fn validate_resource_dependencies(
    frame_graph: &[RenderPassManifestEntry],
    pass_contracts_by_pass: &HashMap<String, ValidatedPassContract>,
    resources_by_id: &HashMap<&str, &RuntimeResourceContractSelection>,
) -> Result<(), String> {
    let pass_by_name = frame_graph
        .iter()
        .map(|pass| (pass.name.as_str(), pass))
        .collect::<HashMap<_, _>>();
    let mut writers_by_resource = HashMap::<&str, Vec<&str>>::new();

    for pass in frame_graph {
        let Some(contract) = pass_contracts_by_pass.get(pass.name.as_str()) else {
            continue;
        };
        for resource_id in &contract.writes {
            writers_by_resource
                .entry(resource_id.as_str())
                .or_default()
                .push(pass.name.as_str());
        }
    }

    for pass in frame_graph {
        let Some(contract) = pass_contracts_by_pass.get(pass.name.as_str()) else {
            continue;
        };

        let dependency_closure = collect_dependency_closure(pass.name.as_str(), &pass_by_name)?;
        for resource_id in &contract.reads {
            let Some(resource) = resources_by_id.get(resource_id.as_str()) else {
                continue;
            };
            if resource.external {
                continue;
            }

            let is_written_by_self = contract.writes.iter().any(|written| written == resource_id);
            if is_written_by_self {
                continue;
            }
            let Some(writers) = writers_by_resource.get(resource_id.as_str()) else {
                return Err(format!(
                    "render manifest pass '{}' reads unresolved resource '{}' with no producing pass",
                    pass.name, resource_id
                ));
            };

            let has_dependency_writer = writers
                .iter()
                .any(|writer| dependency_closure.contains(*writer));
            if !has_dependency_writer {
                return Err(format!(
                    "render manifest pass '{}' reads resource '{}' without depending on a producing pass",
                    pass.name, resource_id
                ));
            }
        }
    }

    Ok(())
}

fn collect_dependency_closure<'a>(
    pass_name: &'a str,
    pass_by_name: &HashMap<&'a str, &'a RenderPassManifestEntry>,
) -> Result<HashSet<&'a str>, String> {
    let mut stack = vec![pass_name];
    let mut visited = HashSet::<&str>::new();
    while let Some(name) = stack.pop() {
        let Some(pass) = pass_by_name.get(name) else {
            return Err(format!(
                "render manifest dependency traversal failed for missing pass '{}'",
                name
            ));
        };
        for dependency in &pass.depends_on {
            let dep_name = dependency.as_str();
            if visited.insert(dep_name) {
                stack.push(dep_name);
            }
        }
    }
    Ok(visited)
}

fn validate_prewarm_groups(
    prewarm_groups: &[ShaderBundlePrewarmGroupEntry],
    shader_modules: &[ShaderBundleModuleEntry],
) -> Result<Vec<ValidatedPrewarmGroup>, String> {
    let module_ids = shader_modules
        .iter()
        .map(|module| module.id.as_str())
        .collect::<HashSet<_>>();
    let mut seen_group_ids = HashSet::<&str>::new();
    let mut groups = Vec::with_capacity(prewarm_groups.len());

    for group in prewarm_groups {
        if group.id.trim().is_empty() {
            return Err("shader bundle prewarm_groups contains an empty id".to_string());
        }
        if !seen_group_ids.insert(group.id.as_str()) {
            return Err(format!(
                "shader bundle prewarm_groups contains duplicate id '{}'",
                group.id
            ));
        }
        if group.shader_modules.is_empty() {
            return Err(format!(
                "shader bundle prewarm_group '{}' must reference at least one shader module",
                group.id
            ));
        }

        let mut seen_modules = HashSet::<&str>::new();
        for module_id in &group.shader_modules {
            if module_id.trim().is_empty() {
                return Err(format!(
                    "shader bundle prewarm_group '{}' contains an empty shader module id",
                    group.id
                ));
            }
            if !seen_modules.insert(module_id.as_str()) {
                return Err(format!(
                    "shader bundle prewarm_group '{}' contains duplicate shader module '{}'",
                    group.id, module_id
                ));
            }
            if !module_ids.contains(module_id.as_str()) {
                return Err(format!(
                    "shader bundle prewarm_group '{}' references unknown shader module '{}'",
                    group.id, module_id
                ));
            }
        }

        groups.push(ValidatedPrewarmGroup {
            id: group.id.clone(),
            required: group.required,
            shader_modules: group.shader_modules.clone(),
        });
    }

    Ok(groups)
}

fn resolve_runtime_prewarm_groups(
    prewarm_groups: &[ValidatedPrewarmGroup],
    selected_shader_modules: &BTreeSet<String>,
) -> Result<Vec<RuntimePrewarmGroupSelection>, String> {
    let mut resolved = Vec::with_capacity(prewarm_groups.len());
    for group in prewarm_groups {
        let selected_modules = group
            .shader_modules
            .iter()
            .filter(|module_id| selected_shader_modules.contains(module_id.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        if group.required && selected_modules.is_empty() {
            return Err(format!(
                "shader bundle prewarm_group '{}' is required but references no shader modules used by the frame graph",
                group.id
            ));
        }
        resolved.push(RuntimePrewarmGroupSelection {
            id: group.id.clone(),
            required: group.required,
            shader_modules: selected_modules,
        });
    }
    Ok(resolved)
}

fn validate_and_resolve_frame_graph_order(
    frame_graph: &[RenderPassManifestEntry],
) -> Result<Vec<usize>, String> {
    if frame_graph.is_empty() {
        return Ok(Vec::new());
    }

    let mut index_by_name = HashMap::<String, usize>::new();
    for (index, pass) in frame_graph.iter().enumerate() {
        if pass.name.trim().is_empty() {
            return Err("render manifest frame_graph contains a pass with empty name".to_string());
        }
        if index_by_name.insert(pass.name.clone(), index).is_some() {
            return Err(format!(
                "render manifest frame_graph contains duplicate pass name '{}'",
                pass.name
            ));
        }
    }

    let mut indegree = vec![0usize; frame_graph.len()];
    let mut edges = vec![Vec::<usize>::new(); frame_graph.len()];
    for (pass_index, pass) in frame_graph.iter().enumerate() {
        let mut seen_dependencies = HashSet::<&str>::new();
        for dependency in &pass.depends_on {
            let dependency_name = dependency.as_str();
            if !seen_dependencies.insert(dependency_name) {
                continue;
            }
            let Some(&dependency_index) = index_by_name.get(dependency_name) else {
                return Err(format!(
                    "frame_graph pass '{}' depends on missing pass '{}'",
                    pass.name, dependency
                ));
            };
            indegree[pass_index] = indegree[pass_index].saturating_add(1);
            edges[dependency_index].push(pass_index);
        }
    }

    let mut ready = indegree
        .iter()
        .enumerate()
        .filter_map(|(index, degree)| (*degree == 0).then_some(Reverse(index)))
        .collect::<BinaryHeap<_>>();

    let mut resolved_order = Vec::with_capacity(frame_graph.len());
    while let Some(Reverse(index)) = ready.pop() {
        resolved_order.push(index);

        for &dependent in &edges[index] {
            indegree[dependent] = indegree[dependent].saturating_sub(1);
            if indegree[dependent] == 0 {
                ready.push(Reverse(dependent));
            }
        }
    }

    if resolved_order.len() != frame_graph.len() {
        let mut blocked = indegree
            .iter()
            .enumerate()
            .filter_map(|(index, degree)| (*degree > 0).then_some(frame_graph[index].name.as_str()))
            .collect::<Vec<_>>();
        blocked.sort_unstable();
        return Err(format!(
            "render manifest frame_graph contains a dependency cycle (blocked passes: {})",
            blocked.join(", ")
        ));
    }

    let declared_order = (0..frame_graph.len()).collect::<Vec<_>>();
    if resolved_order != declared_order {
        let declared = frame_graph
            .iter()
            .map(|pass| pass.name.as_str())
            .collect::<Vec<_>>()
            .join(" -> ");
        let resolved = resolved_order
            .iter()
            .map(|index| frame_graph[*index].name.as_str())
            .collect::<Vec<_>>()
            .join(" -> ");
        return Err(format!(
            "render manifest frame_graph order mismatch: declared order '{declared}' is not dependency-safe; expected '{resolved}'"
        ));
    }

    Ok(resolved_order)
}

fn validate_schema(label: &str, actual: &str, expected: &str) -> Result<(), String> {
    if actual == expected {
        return Ok(());
    }
    Err(format!(
        "{label} schema mismatch: expected '{expected}', got '{actual}'"
    ))
}

fn normalize_optional(value: Option<&str>) -> Option<String> {
    value.and_then(|entry| {
        let trimmed = entry.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn value_matches_selector(entry: &str, selected: &str) -> bool {
    entry.eq_ignore_ascii_case(selected)
}

fn format_variant_descriptor(node_target: Option<&str>, shader_mode: Option<&str>) -> String {
    let node_target = normalize_optional(node_target).unwrap_or_else(|| "<none>".to_string());
    let shader_mode = normalize_optional(shader_mode).unwrap_or_else(|| "<none>".to_string());
    format!("node_target='{node_target}', shader_mode='{shader_mode}'")
}

fn select_variant<'a, T>(
    entries: &'a [T],
    id: &str,
    selector: &ShaderVariantSelector,
    id_of: impl Fn(&'a T) -> &'a str,
    node_target_of: impl Fn(&'a T) -> Option<&'a str>,
    shader_mode_of: impl Fn(&'a T) -> Option<&'a str>,
    label: &str,
) -> Result<&'a T, String> {
    let mut candidates = Vec::<(&T, usize, String)>::new();
    let mut matched_id_count = 0usize;

    for entry in entries {
        if id_of(entry) != id {
            continue;
        }

        matched_id_count = matched_id_count.saturating_add(1);
        let node_target = normalize_optional(node_target_of(entry));
        let shader_mode = normalize_optional(shader_mode_of(entry));
        if selector.node_target.as_deref().is_some_and(|selected| {
            node_target
                .as_deref()
                .is_some_and(|actual| !value_matches_selector(actual, selected))
        }) {
            continue;
        }
        if selector.shader_mode.as_deref().is_some_and(|selected| {
            shader_mode
                .as_deref()
                .is_some_and(|actual| !value_matches_selector(actual, selected))
        }) {
            continue;
        }

        let mut specificity = 0usize;
        if selector.node_target.as_deref().is_some_and(|selected| {
            node_target
                .as_deref()
                .is_some_and(|actual| value_matches_selector(actual, selected))
        }) {
            specificity = specificity.saturating_add(1);
        }
        if selector.shader_mode.as_deref().is_some_and(|selected| {
            shader_mode
                .as_deref()
                .is_some_and(|actual| value_matches_selector(actual, selected))
        }) {
            specificity = specificity.saturating_add(1);
        }

        candidates.push((
            entry,
            specificity,
            format_variant_descriptor(node_target.as_deref(), shader_mode.as_deref()),
        ));
    }

    if candidates.is_empty() {
        if matched_id_count == 0 {
            return Err(format!("{label} '{id}' was not found"));
        }
        return Err(format!(
            "{label} '{id}' has no variant matching {}",
            selector.describe()
        ));
    }
    if candidates.len() == 1 {
        return Ok(candidates[0].0);
    }

    let max_specificity = candidates
        .iter()
        .map(|(_, specificity, _)| *specificity)
        .max()
        .unwrap_or(0);
    let mut best = candidates
        .iter()
        .filter(|(_, specificity, _)| *specificity == max_specificity)
        .collect::<Vec<_>>();
    if best.len() == 1 {
        return Ok(best.remove(0).0);
    }

    let variants = candidates
        .iter()
        .map(|(_, _, descriptor)| descriptor.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    Err(format!(
        "{label} '{id}' is ambiguous for {} (variants: {variants})",
        selector.describe()
    ))
}

fn normalize_shader_mode(raw: Option<&str>) -> Option<String> {
    normalize_optional(raw).map(|mode| mode.to_ascii_lowercase())
}

fn resolve_shader_module_path(
    label: &str,
    module_id: &str,
    default_path: &str,
    generated_path: Option<&str>,
    gpu_path: Option<&str>,
    selector: &ShaderVariantSelector,
) -> Result<String, String> {
    let default_path = normalize_optional(Some(default_path));
    let generated_path = normalize_optional(generated_path);
    let gpu_path = normalize_optional(gpu_path);

    let selected_mode = normalize_shader_mode(selector.shader_mode.as_deref());
    let resolved = match selected_mode.as_deref() {
        Some(mode) if mode.contains("generated") => generated_path,
        Some(mode) if mode.contains("gpu") => gpu_path,
        Some(_) | None => default_path,
    };

    resolved.ok_or_else(|| {
        format!(
            "{label} '{module_id}' has no shader path for {}",
            selector.describe()
        )
    })
}

fn runtime_pipeline_id(base_id: &str, selector: &ShaderVariantSelector) -> String {
    if selector.node_target.is_none() && selector.shader_mode.is_none() {
        return base_id.to_string();
    }
    let mut key = base_id.to_string();
    if let Some(node_target) = selector.node_target.as_deref() {
        key.push_str("::node_target=");
        key.push_str(node_target);
    }
    if let Some(shader_mode) = selector.shader_mode.as_deref() {
        key.push_str("::shader_mode=");
        key.push_str(shader_mode);
    }
    key
}

#[cfg(test)]
mod tests {
    use super::{
        GpuSceneBoundsContract, GpuSceneDrawRecordContract, GpuSceneMaterialRefContract,
        GpuSceneTransformContract, RenderCullMode, RenderManifestDocument, RenderPrimitiveTopology,
        RuntimeShaderSelection, SceneVisibilityCandidate, ShaderBundleDocument,
        material_graph_default_profile_contracts_from_manifest, resolve_runtime_shader_selection,
        simulate_visibility_stage_telemetry,
    };
    use serde_json::{Value, json};
    use std::collections::{HashMap, HashSet};

    #[derive(Debug, Default)]
    struct MockBackendExecution {
        executed_passes: Vec<String>,
        produced_resources: HashSet<String>,
        read_count: usize,
        write_count: usize,
    }

    fn run_mock_backend(
        selection: &RuntimeShaderSelection,
    ) -> Result<MockBackendExecution, String> {
        let mut execution = MockBackendExecution::default();
        let mut executed_passes = HashSet::<String>::new();
        let resource_externals = selection
            .resource_contracts
            .iter()
            .map(|resource| (resource.id.as_str(), resource.external))
            .collect::<HashMap<_, _>>();

        for pass in &selection.frame_graph {
            for dependency in &pass.depends_on {
                if !executed_passes.contains(dependency.as_str()) {
                    return Err(format!(
                        "mock backend scheduled pass '{}' before dependency '{}'",
                        pass.name, dependency
                    ));
                }
            }
            for resource_id in &pass.reads {
                let external = *resource_externals
                    .get(resource_id.as_str())
                    .ok_or_else(|| {
                        format!("mock backend missing resource contract for '{resource_id}'")
                    })?;
                let written_by_pass = pass.writes.iter().any(|write| write == resource_id);
                if !external
                    && !written_by_pass
                    && !execution.produced_resources.contains(resource_id)
                {
                    return Err(format!(
                        "mock backend pass '{}' read resource '{}' before production",
                        pass.name, resource_id
                    ));
                }
                execution.read_count = execution.read_count.saturating_add(1);
            }
            for resource_id in &pass.writes {
                execution.produced_resources.insert(resource_id.clone());
                execution.write_count = execution.write_count.saturating_add(1);
            }
            executed_passes.insert(pass.name.clone());
            execution.executed_passes.push(pass.name.clone());
        }

        Ok(execution)
    }

    fn make_render_manifest(manifest_schema: &str) -> RenderManifestDocument {
        serde_json::from_value(json!({
            "schema_version": manifest_schema,
            "pipelines": [
                {
                    "id": "depth_pipeline",
                    "shader_module": "shader_depth",
                    "vertex_entry": "vs_depth",
                    "fragment_entry": "fs_depth",
                    "primitive": {"topology": "triangles", "cull_mode": "none"}
                },
                {
                    "id": "ui_pipeline",
                    "shader_module": "shader_ui",
                    "vertex_entry": "vs_ui",
                    "fragment_entry": "fs_ui",
                    "primitive": {"topology": "triangles", "cull_mode": "none"}
                }
            ],
            "frame_graph": [
                {"name": "depth", "pipeline": "depth_pipeline", "draw_phase": "depth"},
                {"name": "ui", "pipeline": "ui_pipeline", "draw_phase": "ui", "depends_on": ["depth"]}
            ],
            "resource_contracts": [
                {"id": "camera", "kind": "uniform", "external": true},
                {"id": "depth_color", "kind": "texture"},
                {"id": "ui_color", "kind": "texture"}
            ],
            "pass_contracts": [
                {
                    "id": "pass_depth",
                    "pass": "depth",
                    "reads": ["camera"],
                    "writes": ["depth_color"]
                },
                {
                    "id": "pass_ui",
                    "pass": "ui",
                    "reads": ["camera", "depth_color"],
                    "writes": ["ui_color"]
                }
            ],
            "contracts": {
                "resources": [
                    {"id": "camera", "kind": "uniform"},
                    {"id": "depth_color", "kind": "texture"},
                    {"id": "ui_color", "kind": "texture"}
                ],
                "passes": [
                    {"name": "depth", "reads": ["camera"], "writes": ["depth_color"]},
                    {"name": "ui", "reads": ["camera", "depth_color"], "writes": ["ui_color"]}
                ],
                "default_profile": default_profile_contract_json()
            },
            "shader_modules": [
                {"id": "shader_depth", "path": "shader_depth.wgsl"},
                {"id": "shader_ui", "path": "shader_ui.wgsl"}
            ]
        }))
        .expect("render manifest should parse")
    }

    fn default_profile_contract_json() -> Value {
        json!({
            "lighting": {
                "pbr": {"enabled": true},
                "hdr": {"enabled": true},
                "tonemap": {"operator": "aces"},
                "clustered_lighting": {
                    "enabled": true,
                    "max_lights_per_cluster": 64,
                    "shadow": {
                        "enabled": true,
                        "cascade_count": 4,
                        "atlas_resolution": 2048
                    }
                }
            },
            "reflections": {
                "fallback_chain": ["planar", "ssr", "probe"],
                "planar_budget": {"max_planes": 2, "resolution_scale": 1.0},
                "ssr_budget": {"max_steps": 32, "max_rays_per_pixel": 1},
                "probe_budget": {"max_active_probes": 32, "update_ratio": 0.25}
            },
            "temporal": {
                "motion_vectors": {"enabled": true},
                "taa": {"enabled": true, "history_frames": 12},
                "upscaling": {"enabled": true, "mode": "temporal"},
                "reactive_mask": {"enabled": true},
                "disocclusion_mask": {"enabled": true},
                "dynamic_resolution_policy": {
                    "enabled": true,
                    "min_scale": 0.6,
                    "max_scale": 1.0,
                    "target_frame_time_ms": 16.7,
                    "scale_step": 0.05
                },
                "metrics": {
                    "window_frames": 120,
                    "report_interval_ms": 1000,
                    "max_jitter_ms": 0.75
                }
            }
        })
    }

    fn make_shader_bundle(bundle_schema: &str) -> ShaderBundleDocument {
        serde_json::from_value(json!({
            "schema_version": bundle_schema,
            "shader_modules": [
                {
                    "id": "shader_depth",
                    "path": "shader_depth.wgsl",
                    "entrypoints": ["vs_depth", "fs_depth"]
                },
                {
                    "id": "shader_ui",
                    "path": "shader_ui.wgsl",
                    "entrypoints": ["vs_ui", "fs_ui"]
                }
            ],
            "prewarm_groups": [
                {"id": "required_boot", "required": true, "shader_modules": ["shader_depth"]},
                {"id": "optional_ui", "required": false, "shader_modules": ["shader_ui"]}
            ]
        }))
        .expect("shader bundle should parse")
    }

    fn render_with_overrides(
        mut render: RenderManifestDocument,
        field: &str,
        value: Value,
    ) -> RenderManifestDocument {
        let mut encoded = serde_json::to_value(render).expect("encode render");
        encoded[field] = value;
        render = serde_json::from_value(encoded).expect("decode render with overrides");
        render
    }

    fn bundle_with_overrides(
        mut bundle: ShaderBundleDocument,
        field: &str,
        value: Value,
    ) -> ShaderBundleDocument {
        let mut encoded = serde_json::to_value(bundle).expect("encode bundle");
        encoded[field] = value;
        bundle = serde_json::from_value(encoded).expect("decode bundle with overrides");
        bundle
    }

    #[test]
    fn resolve_shader_selection_accepts_v5_manifests_with_contracts() {
        let render = make_render_manifest("render-schema-v6");
        let bundle = make_shader_bundle("shader-bundle-v6");

        let resolved = resolve_runtime_shader_selection(&render, &bundle).expect("resolve");
        assert_eq!(resolved.pipelines.len(), 2);
        assert_eq!(resolved.frame_graph.len(), 2);
        assert_eq!(resolved.resource_contracts.len(), 3);
        assert_eq!(resolved.prewarm_groups.len(), 2);
        assert_eq!(resolved.frame_graph[1].reads, vec!["camera", "depth_color"]);
        assert_eq!(resolved.frame_graph[1].writes, vec!["ui_color"]);
        assert_eq!(resolved.frame_graph[1].pass_contract_id, "pass_ui");
        assert_eq!(
            resolved.pipelines[0].topology,
            RenderPrimitiveTopology::Triangles
        );
        assert_eq!(resolved.pipelines[0].cull_mode, RenderCullMode::None);
        assert_eq!(
            resolved.gpu_scene_buffers.transforms.resource_id,
            "scene_transforms"
        );
        assert_eq!(resolved.gpu_scene_buffers.draw_records.stride_bytes, 32);
        assert!(resolved.default_profile_contracts.lighting.pbr_enabled);
        assert_eq!(
            resolved
                .default_profile_contracts
                .reflections
                .fallback_chain
                .as_slice(),
            &["planar", "ssr", "probe"]
        );
        assert!(resolved.default_profile_contracts.temporal.taa_enabled);
    }

    #[test]
    fn resolve_shader_selection_accepts_explicit_gpu_scene_buffer_contracts() {
        let render = render_with_overrides(
            make_render_manifest("render-schema-v6"),
            "gpu_scene_buffers",
            json!({
                "transforms": {
                    "resource_id": "xforms",
                    "kind": "storage-buffer",
                    "stride_bytes": 64
                },
                "bounds": {
                    "resource_id": "bounds",
                    "kind": "storage-buffer",
                    "stride_bytes": 48
                },
                "draw_records": {
                    "resource_id": "draws",
                    "kind": "storage-buffer",
                    "stride_bytes": 32
                },
                "material_refs": {
                    "resource_id": "materials",
                    "kind": "storage-buffer",
                    "stride_bytes": 16
                },
                "hiz_occlusion": {
                    "enabled": true,
                    "tier": "tier-1"
                }
            }),
        );
        let bundle = make_shader_bundle("shader-bundle-v6");

        let resolved = resolve_runtime_shader_selection(&render, &bundle).expect("resolve");
        assert_eq!(resolved.gpu_scene_buffers.transforms.resource_id, "xforms");
        assert_eq!(resolved.gpu_scene_buffers.bounds.stride_bytes, 48);
        assert_eq!(
            resolved.gpu_scene_buffers.material_refs.resource_id,
            "materials"
        );
        assert!(resolved.gpu_scene_buffers.hiz_occlusion.enabled);
        assert_eq!(
            resolved.gpu_scene_buffers.hiz_occlusion.tier.as_deref(),
            Some("tier-1")
        );
    }

    #[test]
    fn resolve_shader_selection_accepts_contracts_path_for_resources_and_passes() {
        let render = render_with_overrides(
            render_with_overrides(
                render_with_overrides(
                    make_render_manifest("render-schema-v6"),
                    "resource_contracts",
                    json!([]),
                ),
                "pass_contracts",
                json!([]),
            ),
            "contracts",
            json!({
                "resources": [
                    {"id": "camera", "kind": "uniform"},
                    {"id": "depth_color", "kind": "texture"},
                    {"id": "ui_color", "kind": "texture"}
                ],
                "passes": [
                    {"name": "depth", "reads": ["camera"], "writes": ["depth_color"]},
                    {"name": "ui", "reads": ["camera", "depth_color"], "writes": ["ui_color"]}
                ],
                "default_profile": default_profile_contract_json()
            }),
        );
        let bundle = make_shader_bundle("shader-bundle-v6");

        let resolved = resolve_runtime_shader_selection(&render, &bundle).expect("resolve");
        assert_eq!(resolved.resource_contracts.len(), 3);
        assert_eq!(resolved.frame_graph[0].pass_contract_id, "depth_contract");
        assert_eq!(resolved.frame_graph[1].pass_contract_id, "ui_contract");
    }

    #[test]
    fn resolve_shader_selection_rejects_missing_wl05_wl06_wl07_default_profile_contracts() {
        let render = render_with_overrides(
            make_render_manifest("render-schema-v6"),
            "contracts",
            json!({
                "resources": [
                    {"id": "camera", "kind": "uniform"},
                    {"id": "depth_color", "kind": "texture"},
                    {"id": "ui_color", "kind": "texture"}
                ],
                "passes": [
                    {"name": "depth", "reads": ["camera"], "writes": ["depth_color"]},
                    {"name": "ui", "reads": ["camera", "depth_color"], "writes": ["ui_color"]}
                ]
            }),
        );
        let bundle = make_shader_bundle("shader-bundle-v6");

        let err = resolve_runtime_shader_selection(&render, &bundle).expect_err("must fail");
        assert!(
            err.contains("contracts.default_profile"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn resolve_shader_selection_rejects_incomplete_reflection_fallback_chain() {
        let render = render_with_overrides(
            make_render_manifest("render-schema-v6"),
            "contracts",
            json!({
                "resources": [
                    {"id": "camera", "kind": "uniform"},
                    {"id": "depth_color", "kind": "texture"},
                    {"id": "ui_color", "kind": "texture"}
                ],
                "passes": [
                    {"name": "depth", "reads": ["camera"], "writes": ["depth_color"]},
                    {"name": "ui", "reads": ["camera", "depth_color"], "writes": ["ui_color"]}
                ],
                "default_profile": {
                    "lighting": default_profile_contract_json()["lighting"].clone(),
                    "reflections": {
                        "fallback_chain": ["planar", "probe"],
                        "planar_budget": {"max_planes": 2, "resolution_scale": 1.0},
                        "ssr_budget": {"max_steps": 32, "max_rays_per_pixel": 1},
                        "probe_budget": {"max_active_probes": 32, "update_ratio": 0.25}
                    },
                    "temporal": default_profile_contract_json()["temporal"].clone()
                }
            }),
        );
        let bundle = make_shader_bundle("shader-bundle-v6");

        let err = resolve_runtime_shader_selection(&render, &bundle).expect_err("must fail");
        assert!(err.contains("WL06"), "unexpected error: {err}");
    }

    #[test]
    fn default_profile_validation_parity_for_reflection_fallback_chain() {
        let bad_profile = json!({
            "lighting": default_profile_contract_json()["lighting"].clone(),
            "reflections": {
                "fallback_chain": ["planar", "probe"],
                "planar_budget": {"max_planes": 2, "resolution_scale": 1.0},
                "ssr_budget": {"max_steps": 32, "max_rays_per_pixel": 1},
                "probe_budget": {"max_active_probes": 32, "update_ratio": 0.25}
            },
            "temporal": default_profile_contract_json()["temporal"].clone()
        });
        let profile_entry = serde_json::from_value(bad_profile).expect("profile entry parse");
        let typed_contracts =
            material_graph_default_profile_contracts_from_manifest(&profile_entry)
                .expect("type conversion must succeed");
        let shared_error = super::validate_default_profile_contracts(&typed_contracts)
            .expect_err("shared validator must reject broken fallback chain");

        let render = render_with_overrides(
            make_render_manifest("render-schema-v6"),
            "contracts",
            json!({
                "resources": [
                    {"id": "camera", "kind": "uniform"},
                    {"id": "depth_color", "kind": "texture"},
                    {"id": "ui_color", "kind": "texture"}
                ],
                "passes": [
                    {"name": "depth", "reads": ["camera"], "writes": ["depth_color"]},
                    {"name": "ui", "reads": ["camera", "depth_color"], "writes": ["ui_color"]}
                ],
                "default_profile": profile_entry
            }),
        );
        let bundle = make_shader_bundle("shader-bundle-v6");
        let runtime_error =
            resolve_runtime_shader_selection(&render, &bundle).expect_err("must fail");
        assert!(
            runtime_error.contains(shared_error.as_str()),
            "runtime error should include shared validator output: runtime='{runtime_error}' shared='{shared_error}'"
        );
    }

    #[test]
    fn resolve_shader_selection_rejects_unknown_tonemap_operator_via_shared_typed_contracts() {
        let render = render_with_overrides(
            make_render_manifest("render-schema-v6"),
            "contracts",
            json!({
                "resources": [
                    {"id": "camera", "kind": "uniform"},
                    {"id": "depth_color", "kind": "texture"},
                    {"id": "ui_color", "kind": "texture"}
                ],
                "passes": [
                    {"name": "depth", "reads": ["camera"], "writes": ["depth_color"]},
                    {"name": "ui", "reads": ["camera", "depth_color"], "writes": ["ui_color"]}
                ],
                "default_profile": {
                    "lighting": {
                        "pbr": {"enabled": true},
                        "hdr": {"enabled": true},
                        "tonemap": {"operator": "ACES-unknown"},
                        "clustered_lighting": {
                            "enabled": true,
                            "max_lights_per_cluster": 64,
                            "shadow": {"enabled": true, "cascade_count": 4, "atlas_resolution": 2048}
                        }
                    },
                    "reflections": default_profile_contract_json()["reflections"].clone(),
                    "temporal": default_profile_contract_json()["temporal"].clone()
                }
            }),
        );
        let bundle = make_shader_bundle("shader-bundle-v6");
        let err = resolve_runtime_shader_selection(&render, &bundle).expect_err("must fail");
        assert!(
            err.contains("type validation failed"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn resolve_shader_selection_rejects_invalid_temporal_policy() {
        let render = render_with_overrides(
            make_render_manifest("render-schema-v6"),
            "contracts",
            json!({
                "resources": [
                    {"id": "camera", "kind": "uniform"},
                    {"id": "depth_color", "kind": "texture"},
                    {"id": "ui_color", "kind": "texture"}
                ],
                "passes": [
                    {"name": "depth", "reads": ["camera"], "writes": ["depth_color"]},
                    {"name": "ui", "reads": ["camera", "depth_color"], "writes": ["ui_color"]}
                ],
                "default_profile": {
                    "lighting": default_profile_contract_json()["lighting"].clone(),
                    "reflections": default_profile_contract_json()["reflections"].clone(),
                    "temporal": {
                        "motion_vectors": {"enabled": true},
                        "taa": {"enabled": true, "history_frames": 12},
                        "upscaling": {"enabled": true, "mode": "temporal"},
                        "reactive_mask": {"enabled": true},
                        "disocclusion_mask": {"enabled": true},
                        "dynamic_resolution_policy": {
                            "enabled": true,
                            "min_scale": 1.1,
                            "max_scale": 1.0,
                            "target_frame_time_ms": 16.7,
                            "scale_step": 0.05
                        },
                        "metrics": {
                            "window_frames": 120,
                            "report_interval_ms": 1000,
                            "max_jitter_ms": 0.75
                        }
                    }
                }
            }),
        );
        let bundle = make_shader_bundle("shader-bundle-v6");

        let err = resolve_runtime_shader_selection(&render, &bundle).expect_err("must fail");
        assert!(err.contains("WL07"), "unexpected error: {err}");
    }

    #[test]
    fn visibility_stage_telemetry_reports_non_zero_metrics_for_scene_content() {
        let candidates = vec![
            SceneVisibilityCandidate {
                transform: GpuSceneTransformContract {
                    translation: [100.0, 120.0, 0.0],
                    scale: [1.0, 1.0, 1.0],
                },
                bounds: GpuSceneBoundsContract {
                    center: [100.0, 120.0, 0.0],
                    extents: [10.0, 10.0, 0.0],
                },
                draw_record: GpuSceneDrawRecordContract {
                    transform_index: 0,
                    bounds_index: 0,
                    material_ref_index: 0,
                    instance_count: 1,
                },
                material_ref: GpuSceneMaterialRefContract { material_slot: 1 },
            },
            SceneVisibilityCandidate {
                transform: GpuSceneTransformContract {
                    translation: [2000.0, 2000.0, 0.0],
                    scale: [1.0, 1.0, 1.0],
                },
                bounds: GpuSceneBoundsContract {
                    center: [2000.0, 2000.0, 0.0],
                    extents: [12.0, 12.0, 0.0],
                },
                draw_record: GpuSceneDrawRecordContract {
                    transform_index: 1,
                    bounds_index: 1,
                    material_ref_index: 1,
                    instance_count: 1,
                },
                material_ref: GpuSceneMaterialRefContract { material_slot: 2 },
            },
            SceneVisibilityCandidate {
                transform: GpuSceneTransformContract {
                    translation: [320.0, 260.0, 0.0],
                    scale: [1.0, 1.0, 1.0],
                },
                bounds: GpuSceneBoundsContract {
                    center: [320.0, 260.0, 0.0],
                    extents: [16.0, 16.0, 0.0],
                },
                draw_record: GpuSceneDrawRecordContract {
                    transform_index: 2,
                    bounds_index: 2,
                    material_ref_index: 2,
                    instance_count: 1,
                },
                material_ref: GpuSceneMaterialRefContract { material_slot: 3 },
            },
        ];

        let telemetry = simulate_visibility_stage_telemetry(&candidates, 800.0, 600.0, false);
        assert_eq!(telemetry.candidate_draws, 3);
        assert_eq!(telemetry.visible_draws, 2);
        assert!(telemetry.culled_ratio > 0.0);
        assert_eq!(telemetry.indirect_draw_count, telemetry.visible_draws);
        assert!(telemetry.indirect_submission_path_default);
        assert!(!telemetry.cpu_fallback_used);
    }

    #[test]
    fn visibility_stage_hiz_tier_toggle_culls_more_draws() {
        let candidates = (0..8)
            .map(|index| SceneVisibilityCandidate {
                transform: GpuSceneTransformContract {
                    translation: [50.0 + index as f32, 60.0 + index as f32, 0.0],
                    scale: [1.0, 1.0, 1.0],
                },
                bounds: GpuSceneBoundsContract {
                    center: [50.0 + index as f32, 60.0 + index as f32, 0.0],
                    extents: [8.0, 8.0, 0.0],
                },
                draw_record: GpuSceneDrawRecordContract {
                    transform_index: index,
                    bounds_index: index,
                    material_ref_index: index,
                    instance_count: 1,
                },
                material_ref: GpuSceneMaterialRefContract {
                    material_slot: index,
                },
            })
            .collect::<Vec<_>>();

        let without_hiz = simulate_visibility_stage_telemetry(&candidates, 1024.0, 768.0, false);
        let with_hiz = simulate_visibility_stage_telemetry(&candidates, 1024.0, 768.0, true);
        assert!(with_hiz.hiz_occlusion_tier_enabled);
        assert!(without_hiz.visible_draws > with_hiz.visible_draws);
        assert!(with_hiz.culled_ratio > without_hiz.culled_ratio);
    }

    #[test]
    fn schema_rejects_legacy_adapter_primitive_tokens() {
        let err = serde_json::from_value::<RenderManifestDocument>(json!({
            "schema_version": "render-schema-v6",
            "pipelines": [{
                "id": "depth_pipeline",
                "shader_module": "shader_depth",
                "vertex_entry": "vs_depth",
                "fragment_entry": "fs_depth",
                "primitive": {"topology": "triangle-list", "cull_mode": "none"}
            }],
            "frame_graph": [{"name": "depth", "pipeline": "depth_pipeline", "draw_phase": "depth"}],
            "resource_contracts": [{"id": "camera", "kind": "uniform", "external": true}],
            "pass_contracts": [{"id": "pass_depth", "pass": "depth", "reads": ["camera"], "writes": []}],
            "shader_modules": [{"id": "shader_depth", "path": "shader_depth.wgsl"}]
        }))
        .expect_err("legacy adapter tokens should fail to parse");
        let message = err.to_string();
        assert!(
            message.contains("unknown variant"),
            "unexpected parse error: {message}"
        );
    }

    #[test]
    fn schema_serialization_uses_backend_neutral_tokens() {
        let render = make_render_manifest("render-schema-v6");
        let payload = serde_json::to_string(&render).expect("serialize render manifest");
        assert!(payload.contains("\"topology\":\"triangles\""));
        for leaked_token in ["triangle-list", "bgra8unorm", "wgpu::"] {
            assert!(
                !payload.contains(leaked_token),
                "schema leaked adapter token '{leaked_token}'"
            );
        }
    }

    #[test]
    fn resolve_shader_selection_rejects_v5_render_manifest_schema() {
        let render = make_render_manifest("render-schema-v5");
        let bundle = make_shader_bundle("shader-bundle-v6");

        let err = resolve_runtime_shader_selection(&render, &bundle)
            .expect_err("must reject v5 render manifest");
        assert!(
            err.contains("render manifest schema mismatch"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn resolve_shader_selection_rejects_v5_shader_bundle_schema() {
        let render = make_render_manifest("render-schema-v6");
        let bundle = make_shader_bundle("shader-bundle-v5");

        let err = resolve_runtime_shader_selection(&render, &bundle)
            .expect_err("must reject v5 shader bundle");
        assert!(
            err.contains("shader bundle schema mismatch"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn rejects_v5_schema() {
        let render_v5 = make_render_manifest("render-schema-v5");
        let bundle_v6 = make_shader_bundle("shader-bundle-v6");
        let render_err = resolve_runtime_shader_selection(&render_v5, &bundle_v6)
            .expect_err("v5 render schema must be rejected");
        assert!(render_err.contains("render manifest schema mismatch"));

        let render_v6 = make_render_manifest("render-schema-v6");
        let bundle_v5 = make_shader_bundle("shader-bundle-v5");
        let bundle_err = resolve_runtime_shader_selection(&render_v6, &bundle_v5)
            .expect_err("v5 bundle schema must be rejected");
        assert!(bundle_err.contains("shader bundle schema mismatch"));
    }

    #[test]
    fn resolve_shader_selection_rejects_unresolved_resource_reference() {
        let render = render_with_overrides(
            make_render_manifest("render-schema-v6"),
            "pass_contracts",
            json!([
                {
                    "id": "pass_depth",
                    "pass": "depth",
                    "reads": ["camera"],
                    "writes": ["depth_color"]
                },
                {
                    "id": "pass_ui",
                    "pass": "ui",
                    "reads": ["camera", "missing_resource"],
                    "writes": ["ui_color"]
                }
            ]),
        );
        let bundle = make_shader_bundle("shader-bundle-v6");

        let err = resolve_runtime_shader_selection(&render, &bundle)
            .expect_err("must reject unresolved resource");
        assert!(
            err.contains("unresolved resource"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn resolve_shader_selection_rejects_invalid_resource_dependency_chain() {
        let render = render_with_overrides(
            make_render_manifest("render-schema-v6"),
            "frame_graph",
            json!([
                {"name": "depth", "pipeline": "depth_pipeline", "draw_phase": "depth"},
                {"name": "ui", "pipeline": "ui_pipeline", "draw_phase": "ui"}
            ]),
        );
        let bundle = make_shader_bundle("shader-bundle-v6");

        let err = resolve_runtime_shader_selection(&render, &bundle)
            .expect_err("must reject missing dependency");
        assert!(
            err.contains("without depending on a producing pass"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn resolve_shader_selection_rejects_missing_frame_graph_dependency() {
        let render = render_with_overrides(
            make_render_manifest("render-schema-v6"),
            "frame_graph",
            json!([
                {"name": "depth", "pipeline": "depth_pipeline", "draw_phase": "depth", "depends_on": ["ghost"]},
                {"name": "ui", "pipeline": "ui_pipeline", "draw_phase": "ui", "depends_on": ["depth"]}
            ]),
        );
        let bundle = make_shader_bundle("shader-bundle-v6");

        let err = resolve_runtime_shader_selection(&render, &bundle).expect_err("must fail");
        assert!(
            err.contains("depends on missing pass"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn resolve_shader_selection_requires_explicit_gpu_path_without_fallback() {
        let render = render_with_overrides(
            make_render_manifest("render-schema-v6"),
            "pipelines",
            json!([
                {
                    "id": "depth_pipeline",
                    "shader_module": "shader_depth",
                    "vertex_entry": "vs_depth",
                    "fragment_entry": "fs_depth",
                    "shader_mode": "gpu",
                    "primitive": {"topology": "triangles", "cull_mode": "none"}
                },
                {
                    "id": "ui_pipeline",
                    "shader_module": "shader_ui",
                    "vertex_entry": "vs_ui",
                    "fragment_entry": "fs_ui",
                    "primitive": {"topology": "triangles", "cull_mode": "none"}
                }
            ]),
        );
        let bundle = bundle_with_overrides(
            make_shader_bundle("shader-bundle-v6"),
            "shader_modules",
            json!([
                {
                    "id": "shader_depth",
                    "path": "shader_depth.wgsl",
                    "entrypoints": ["vs_depth", "fs_depth"]
                },
                {
                    "id": "shader_ui",
                    "path": "shader_ui.wgsl",
                    "entrypoints": ["vs_ui", "fs_ui"]
                }
            ]),
        );

        let err = resolve_runtime_shader_selection(&render, &bundle).expect_err("must fail");
        assert!(
            err.contains("has no shader path"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn resolve_shader_selection_rejects_required_prewarm_group_without_selected_modules() {
        let render = make_render_manifest("render-schema-v6");
        let bundle = bundle_with_overrides(
            make_shader_bundle("shader-bundle-v6"),
            "prewarm_groups",
            json!([
                {
                    "id": "required_boot",
                    "required": true,
                    "shader_modules": ["unused_shader"]
                }
            ]),
        );

        let err = resolve_runtime_shader_selection(&render, &bundle).expect_err("must fail");
        assert!(
            err.contains("unknown shader module"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn mock_backend_conformance_executes_passes_with_valid_resource_semantics() {
        let render = make_render_manifest("render-schema-v6");
        let bundle = make_shader_bundle("shader-bundle-v6");
        let resolved = resolve_runtime_shader_selection(&render, &bundle).expect("resolve");

        let execution = run_mock_backend(&resolved).expect("mock backend must execute");
        assert_eq!(execution.executed_passes, vec!["depth", "ui"]);
        assert!(execution.produced_resources.contains("depth_color"));
        assert!(execution.produced_resources.contains("ui_color"));
        assert_eq!(execution.read_count, 3);
        assert_eq!(execution.write_count, 2);
    }

    #[test]
    fn mock_backend_conformance_handles_compute_pass_dependencies() {
        let render = render_with_overrides(
            render_with_overrides(
                make_render_manifest("render-schema-v6"),
                "frame_graph",
                json!([
                    {"name": "depth", "pipeline": "depth_pipeline", "draw_phase": "depth"},
                    {
                        "name": "compute_lighting",
                        "pipeline": "depth_pipeline",
                        "draw_phase": "compute",
                        "pass_type": "compute",
                        "depends_on": ["depth"]
                    },
                    {"name": "ui", "pipeline": "ui_pipeline", "draw_phase": "ui", "depends_on": ["compute_lighting"]}
                ]),
            ),
            "pass_contracts",
            json!([
                {
                    "id": "pass_depth",
                    "pass": "depth",
                    "reads": ["camera"],
                    "writes": ["depth_color"]
                },
                {
                    "id": "pass_compute_lighting",
                    "pass": "compute_lighting",
                    "reads": ["depth_color"],
                    "writes": ["depth_color"]
                },
                {
                    "id": "pass_ui",
                    "pass": "ui",
                    "reads": ["camera", "depth_color"],
                    "writes": ["ui_color"]
                }
            ]),
        );
        let bundle = make_shader_bundle("shader-bundle-v6");

        let resolved = resolve_runtime_shader_selection(&render, &bundle).expect("resolve");
        assert!(resolved.compute_pass_manifest_ready);
        let execution = run_mock_backend(&resolved).expect("mock backend must execute");
        assert_eq!(
            execution.executed_passes,
            vec!["depth", "compute_lighting", "ui"]
        );
    }
}
