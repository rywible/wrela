use crate::input::InputButtons;
use crate::key_input_wiring::handle_key_input_state;
use crate::manifest_validation::{
    AnimationContractLoadSummary, AssetFactoryManifestLoadSummary, AssetManifestLoadSummary,
    SceneLayoutManifest, collect_unique_scene_assets,
    parse_and_validate_animation_manifests_from_json,
    parse_and_validate_asset_factory_manifests_from_json,
    parse_and_validate_asset_pack_manifests_from_json,
    parse_and_validate_scene_layout_manifest, validate_glb_asset,
};
use crate::protocol::{Envelope, MessageTypeV5, PROTOCOL_V5, PROTOCOL_V5_SUB_VERSION};
use crate::protocol_metadata::{ProtocolContract, parse_protocol_contract};
use crate::render_manifest::{
    GpuSceneBoundsContract, GpuSceneDrawRecordContract, GpuSceneMaterialRefContract,
    GpuSceneTransformContract, RenderCullMode, RenderManifestDocument, RenderPrimitiveTopology,
    RuntimeDefaultProfileContracts, RuntimeFrameGraphPassSelection, RuntimeGpuSceneBufferContracts,
    RuntimePipelineShaderSelection, RuntimePrewarmGroupSelection, SceneVisibilityCandidate,
    ShaderBundleDocument, VisibilityStageTelemetry, resolve_runtime_shader_selection,
    simulate_visibility_stage_telemetry,
};
use crate::restart_latch::decide_restart_signal;
use js_sys::{Array, Object, Reflect, Uint8Array};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    BinaryType, Event, HtmlCanvasElement, KeyboardEvent, MessageEvent, Response, WebSocket,
};

const DEFAULT_WORLD_WIDTH: f32 = 800.0;
const DEFAULT_WORLD_HEIGHT: f32 = 600.0;
const TICK_DT_MS: u32 = 16;
const PLAYER_HALF_SIZE: f32 = 18.0;
const PICKUP_RADIUS: f32 = 28.0;
const READY_BOOT_STATUS: &str = "Boot complete. wasm client runtime frame loop is live.";
const GPU_BOOT_STATUS: &str = "Booting WebGPU renderer and frame loop.";
const PROTOCOL_BOOT_STATUS: &str = "Loading protocol contract.";
const MANIFEST_BOOT_STATUS: &str = "Loading render/shader artifacts.";
const PROTOCOL_METADATA_FILE: &str = "protocol-v5.json";
const RENDER_MANIFEST_FILE: &str = "render-manifest.json";
const SHADER_BUNDLE_FILE: &str = "shader-bundle.json";
const ASSET_PACK_MANIFEST_FILE: &str = "assets-manifest.json";
const WORLD_CHUNK_MANIFEST_FILE: &str = "world-chunks.json";
const ASSET_FACTORY_MANIFEST_FILE: &str = "asset-factory-manifest-v2.json";
const ASSET_PROVENANCE_LEDGER_FILE: &str = "asset-provenance-ledger-v1.json";
const ASSET_QUALITY_REPORT_FILE: &str = "asset-quality-report-v2.json";
const UI_ATLAS_MANIFEST_FILE: &str = "ui-atlas-manifest-v1.json";
const CHARACTER_BUNDLE_MANIFEST_FILE: &str = "character-bundle-manifest-v3.json";
const ANIMATION_RIG_CATALOG_FILE: &str = "animation-rig-catalog-v1.json";
const ANIMATION_CLIP_BUNDLE_FILE: &str = "animation-clip-bundle-v2.json";
const ANIMATION_GRAPH_CONTRACT_FILE: &str = "animation-graph-contract-v2.json";
const FLORA_SIM_CONTRACT_FILE: &str = "flora-sim-contract-v1.json";
const ANIMATION_QUALITY_REPORT_FILE: &str = "animation-quality-report-v2.json";
const PLAYER_HERO_GLB_FILE: &str = "assets/generated/characters/player_character.glb";
const SCENE_LAYOUT_MANIFEST_FILE: &str = "assets/generated/environment/forest-scene-layout-v1.json";
const ENVIRONMENT_ASSET_DIR: &str = "assets/generated/environment";
const MAX_COLLECTIBLES: usize = u32::BITS as usize;
const MAX_3D_INSTANCES: usize = 1024;
/// Size of one ModelEntry in the storage buffer: two mat4x4<f32> = 128 bytes.
const MODEL_ENTRY_SIZE: u64 = 128;
/// Size of the joint palette storage buffer: MAX_SKINNING_JOINTS * 64 bytes (one mat4x4<f32> per joint).
const JOINT_PALETTE_BUFFER_SIZE: u64 = crate::mesh::MAX_SKINNING_JOINTS as u64 * 64;
const DEFAULT_MMO_ROLE: &str = "world";
const RUNTIME_METRICS_SCHEMA_VERSION: u32 = 2;
const GOVERNOR_ACTION_TRACE_LIMIT: usize = 128;
const RUNTIME_BUDGET_HISTORY_LIMIT: usize = 120;
const PASS_TIMING_SAMPLE_LIMIT: usize = 32;
const VOLUMETRIC_STEPS_DEFAULT: u32 = 64;
const VOLUMETRIC_STEPS_MIN: u32 = 24;
const VOLUMETRIC_STEPS_MAX: u32 = 96;
const ANIMATION_PHASE_TICK_Q16: i32 = 4096;
const DEFAULT_FOREST_ENEMY_INSTANCE_COUNT: usize = 2;
const PLAYER_STATE_DODGE: i32 = 3;
const PLAYER_STATE_ATTACK: i32 = 4;
const PLAYER_STATE_PARRY_ACTIVE: i32 = 6;

thread_local! {
    static APP_RUNTIME: RefCell<Option<Rc<RefCell<Runtime>>>> = const { RefCell::new(None) };
}

fn normalize_mmo_role(raw: &str) -> &'static str {
    match raw.trim().to_ascii_lowercase().as_str() {
        "gateway" => "gateway",
        "zone" => "zone",
        "world" => "world",
        _ => DEFAULT_MMO_ROLE,
    }
}

fn default_mmo_role_string() -> String {
    DEFAULT_MMO_ROLE.to_string()
}

#[derive(Debug, Clone)]
struct PendingInput {
    seq: u64,
    tick: u64,
    axis_x: f32,
    axis_y: f32,
    dt_ms: u32,
    collect_pressed: bool,
}

#[derive(Debug, Clone)]
struct PredictedState {
    tick: u64,
    player_x: f32,
    player_y: f32,
    score: u32,
    collected_mask: u32,
}

impl Default for PredictedState {
    fn default() -> Self {
        Self {
            tick: 0,
            player_x: DEFAULT_WORLD_WIDTH * 0.5,
            player_y: DEFAULT_WORLD_HEIGHT * 0.5,
            score: 0,
            collected_mask: 0,
        }
    }
}

#[derive(Debug, Default, Clone)]
struct RuntimeTelemetry {
    draw_calls: u64,
    compute_passes: u64,
    gpu_upload_bytes: u64,
    compile_stall_count: u64,
    prewarm_blocked_frames: u64,
    visibility_candidate_draws: u64,
    visibility_visible_draws: u64,
    visibility_culled_ratio: f64,
    visibility_indirect_draw_count: u64,
    visibility_cpu_fallback_frames: u64,
    frame_times_ms: VecDeque<f64>,
    last_frame_at: f64,
    frame_index: u64,
    pass_timing_supported: bool,
    pass_timing_fallback_used: bool,
    pass_timings: Vec<RuntimePassTimingSample>,
    budget_outcomes: Vec<RuntimeFrameBudgetOutcome>,
    budget_counters: RuntimeFrameBudgetCounters,
    last_budget_outcome: Option<RuntimeFrameBudgetOutcome>,
}

#[derive(Debug, Clone, Default)]
struct RuntimeCombatEventTelemetry {
    lock_toggle_count: u64,
    target_cycle_count: u64,
    attack_light_count: u64,
    attack_heavy_count: u64,
    parry_count: u64,
    dodge_count: u64,
    death_count: u64,
    restart_count: u64,
}

#[derive(Debug, Clone, Default)]
struct RuntimeAnimationTelemetry {
    contract_loaded: bool,
    graph_ref: String,
    clip_replay_hash: String,
    contract_state_count: u32,
    contract_transition_count: u32,
    quality_event_window_alignment: f64,
    recognized_event_markers: u64,
    unknown_event_markers: u64,
    reconcile_rejections: u64,
    last_authority_state: String,
    last_reconciled_state: String,
}

#[derive(Debug, Clone, Default)]
struct RuntimeFrameBudgetCounters {
    within_budget_frames: u64,
    over_budget_frames: u64,
    long_frame_count: u64,
    hitch_count: u64,
    max_consecutive_long_frames: u64,
    max_consecutive_hitches: u64,
    current_long_frame_streak: u64,
    current_hitch_streak: u64,
}

#[derive(Debug, Clone)]
struct RuntimeFrameBudgetOutcome {
    frame_index: u64,
    frame_time_ms: f64,
    target_frame_time_ms: f64,
    budget_delta_ms: f64,
    within_budget: bool,
    long_frame: bool,
    hitch: bool,
}

#[derive(Debug, Clone)]
struct RuntimePassTimingSample {
    pass_name: String,
    pass_kind: String,
    duration_ms: f64,
    fallback_estimate: bool,
}

#[derive(Debug, Clone, Copy)]
struct RuntimeGovernorBudgets {
    dynamic_resolution_scale: f64,
    shadow_quality_tier: u8,
    ssr_quality_tier: u8,
    probe_update_rate: f64,
    volumetric_steps: u32,
}

impl Default for RuntimeGovernorBudgets {
    fn default() -> Self {
        Self {
            dynamic_resolution_scale: 1.0,
            shadow_quality_tier: 2,
            ssr_quality_tier: 2,
            probe_update_rate: 0.25,
            volumetric_steps: VOLUMETRIC_STEPS_DEFAULT,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct RuntimeGovernorBounds {
    target_frame_time_ms: f64,
    dynamic_resolution_min: f64,
    dynamic_resolution_max: f64,
    dynamic_resolution_step: f64,
    shadow_tier_min: u8,
    shadow_tier_max: u8,
    ssr_tier_min: u8,
    ssr_tier_max: u8,
    probe_rate_min: f64,
    probe_rate_max: f64,
    volumetric_steps_min: u32,
    volumetric_steps_max: u32,
}

impl Default for RuntimeGovernorBounds {
    fn default() -> Self {
        Self {
            target_frame_time_ms: 16.667,
            dynamic_resolution_min: 0.7,
            dynamic_resolution_max: 1.0,
            dynamic_resolution_step: 0.05,
            shadow_tier_min: 0,
            shadow_tier_max: 2,
            ssr_tier_min: 0,
            ssr_tier_max: 2,
            probe_rate_min: 0.05,
            probe_rate_max: 0.25,
            volumetric_steps_min: VOLUMETRIC_STEPS_MIN,
            volumetric_steps_max: VOLUMETRIC_STEPS_MAX,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct RuntimeGovernorCalibration {
    active: bool,
    complete: bool,
    sampled_frames: u32,
    sample_target_frames: u32,
    startup_average_frame_ms: f64,
    startup_p95_frame_ms: f64,
}

impl Default for RuntimeGovernorCalibration {
    fn default() -> Self {
        Self {
            active: true,
            complete: false,
            sampled_frames: 0,
            sample_target_frames: 45,
            startup_average_frame_ms: 0.0,
            startup_p95_frame_ms: 0.0,
        }
    }
}

#[derive(Debug, Clone)]
struct RuntimeGovernorActionEvent {
    tick: u64,
    frame_index: u64,
    now_ms: f64,
    action: String,
    reason: String,
    budget_delta_ms: f64,
    blocked_by_guardrail: bool,
    before: RuntimeGovernorBudgets,
    after: RuntimeGovernorBudgets,
}

#[derive(Debug, Clone)]
struct RuntimeQualityGovernorState {
    initialized_from_contracts: bool,
    bounds: RuntimeGovernorBounds,
    budgets: RuntimeGovernorBudgets,
    calibration: RuntimeGovernorCalibration,
    actions: Vec<RuntimeGovernorActionEvent>,
    adaptation_cooldown_frames: u32,
    within_budget_streak: u32,
    over_budget_streak: u32,
    adaptation_count: u64,
    blocked_stability_disable_attempts: u64,
    critical_stability_passes: Vec<String>,
}

impl Default for RuntimeQualityGovernorState {
    fn default() -> Self {
        Self {
            initialized_from_contracts: false,
            bounds: RuntimeGovernorBounds::default(),
            budgets: RuntimeGovernorBudgets::default(),
            calibration: RuntimeGovernorCalibration::default(),
            actions: Vec::new(),
            adaptation_cooldown_frames: 0,
            within_budget_streak: 0,
            over_budget_streak: 0,
            adaptation_count: 0,
            blocked_stability_disable_attempts: 0,
            critical_stability_passes: vec![
                "temporal-motion-vectors".to_string(),
                "temporal-taa".to_string(),
                "temporal-reactive-mask".to_string(),
                "temporal-disocclusion-mask".to_string(),
                "prewarm-required".to_string(),
            ],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuntimeConvergenceStage {
    Bootstrap,
    Stream,
    Refine,
    Converged,
}

impl RuntimeConvergenceStage {
    fn rank(self) -> u8 {
        match self {
            RuntimeConvergenceStage::Bootstrap => 0,
            RuntimeConvergenceStage::Stream => 1,
            RuntimeConvergenceStage::Refine => 2,
            RuntimeConvergenceStage::Converged => 3,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            RuntimeConvergenceStage::Bootstrap => "bootstrap",
            RuntimeConvergenceStage::Stream => "stream",
            RuntimeConvergenceStage::Refine => "refine",
            RuntimeConvergenceStage::Converged => "converged",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuntimeResidencyClass {
    Core,
    Hot,
    Warm,
    Cold,
}

impl RuntimeResidencyClass {
    fn as_str(self) -> &'static str {
        match self {
            RuntimeResidencyClass::Core => "core",
            RuntimeResidencyClass::Hot => "hot",
            RuntimeResidencyClass::Warm => "warm",
            RuntimeResidencyClass::Cold => "cold",
        }
    }
}

#[derive(Debug, Clone)]
struct ResidencyAdaptationEvent {
    tick: u64,
    now_ms: f64,
    reason: String,
    from_stage: RuntimeConvergenceStage,
    to_stage: RuntimeConvergenceStage,
    from_residency_class: RuntimeResidencyClass,
    to_residency_class: RuntimeResidencyClass,
    residency_pressure: f64,
}

#[derive(Debug, Clone)]
struct StreamingTelemetry {
    chunk_hit: u64,
    chunk_miss: u64,
    loaded_chunk_count: u64,
    loaded_bytes: u64,
    residency_pressure: f64,
    convergence_stage: RuntimeConvergenceStage,
    residency_class: RuntimeResidencyClass,
    adaptation_events: Vec<ResidencyAdaptationEvent>,
}

impl Default for StreamingTelemetry {
    fn default() -> Self {
        Self {
            chunk_hit: 0,
            chunk_miss: 0,
            loaded_chunk_count: 0,
            loaded_bytes: 0,
            residency_pressure: 0.0,
            convergence_stage: RuntimeConvergenceStage::Bootstrap,
            residency_class: RuntimeResidencyClass::Cold,
            adaptation_events: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct SnapshotPayload {
    tick: u64,
    player_x: f32,
    player_y: f32,
    score: u32,
    collected_mask: u32,
    #[serde(default)]
    hash: Option<u64>,
    #[serde(default)]
    anim_state_id: Option<String>,
    #[serde(default)]
    anim_phase_q16: Option<i32>,
    #[serde(default)]
    anim_event_markers: Vec<String>,
    #[serde(default)]
    anim_root_motion_q16: Option<i32>,
    #[serde(default)]
    anim_reconcile_seq: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct StateDeltaPayload {
    #[serde(default, alias = "t")]
    tick: Option<u64>,
    #[serde(default, alias = "x")]
    player_x: Option<f32>,
    #[serde(default, alias = "y")]
    player_y: Option<f32>,
    #[serde(default, alias = "s")]
    score: Option<u32>,
    #[serde(default, alias = "m")]
    collected_mask: Option<u32>,
    #[serde(default)]
    hash: Option<u64>,
    #[serde(default)]
    anim_state_id: Option<String>,
    #[serde(default)]
    anim_phase_q16: Option<i32>,
    #[serde(default)]
    anim_event_markers: Option<Vec<String>>,
    #[serde(default)]
    anim_root_motion_q16: Option<i32>,
    #[serde(default)]
    anim_reconcile_seq: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
struct HelloPayload {
    #[serde(default = "default_mmo_role_string")]
    role: String,
    #[serde(default)]
    world_width: f32,
    #[serde(default)]
    world_height: f32,
    #[serde(default)]
    collectibles: Vec<(f32, f32)>,
    #[serde(default)]
    snapshot: Option<SnapshotPayload>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct ServerStatePayload {
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    ack: Option<u64>,
    #[serde(default)]
    forced_divergence: bool,
    #[serde(default)]
    rollback_ring_len: Option<u32>,
    #[serde(default)]
    delta_kind: Option<String>,
    #[serde(default)]
    snapshot: Option<SnapshotPayload>,
    #[serde(default)]
    delta: Option<StateDeltaPayload>,
}

#[derive(Debug, Clone, Default)]
struct AnimationAuthorityState {
    anim_state_id: Option<String>,
    anim_phase_q16: i32,
    anim_event_markers: Vec<String>,
    anim_root_motion_q16: i32,
    anim_reconcile_seq: u64,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct ErrorPayload {
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct OutboundInput {
    seq: u64,
    tick: u64,
    axis_x: f32,
    axis_y: f32,
    dt_ms: u32,
    collect_pressed: bool,
}

#[derive(Debug, Clone, Serialize)]
struct OutboundInputBatch {
    inputs: Vec<OutboundInput>,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct FrameUniform {
    canvas_world: [f32; 4],
    player: [f32; 4],
    ui: [u32; 4],
    collectibles: [[f32; 4]; MAX_COLLECTIBLES],
}

#[derive(Debug, Clone)]
struct RenderSceneSnapshot {
    world_width: f32,
    world_height: f32,
    player_x: f32,
    player_y: f32,
    collected_mask: u32,
    app_mode_is_website: bool,
    collectible_positions: Vec<(f32, f32)>,
}

// ── 3D scene types ──────────────────────────────────────────────────────

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct FrameUniform3D {
    view_proj: [[f32; 4]; 4],
    camera_pos: [f32; 4],
    light_dir: [f32; 4],
    light_color: [f32; 4],
    ambient: [f32; 4],
    time: [f32; 4],
    fog_color_and_start: [f32; 4], // xyz = fog color (linear), w = fog start distance
    fog_params: [f32; 4], // x = fog end, y = fog density, z = fog height falloff, w = unused
    // Wind parameters: x = time, y = strength, z = turbulence, w = unused
    wind_params: [f32; 4],
    // Wind direction: xyz = normalized direction, w = unused
    wind_dir: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ModelUniform {
    model: [[f32; 4]; 4],
    normal_model: [[f32; 4]; 4],
}

struct MeshInstance {
    mesh_index: usize,
    model_matrix: [[f32; 4]; 4],
}

struct RenderSceneSnapshot3D<'a> {
    camera_view: [[f32; 4]; 4],
    camera_proj: [[f32; 4]; 4],
    camera_position: [f32; 3],
    mesh_instances: &'a [MeshInstance],
    light_direction: [f32; 3],
    light_color: [f32; 3],
    ambient_color: [f32; 3],
    // Combat visual effects
    hit_stop_active: bool,
    hit_stop_intensity: f32,
    camera_shake: f32,
    parry_flash_alpha: f32,
    chromatic_aberration: f32,
    delta_time_secs: f32,
    /// Current player game state integer (0=idle, 1=walk, ...) used to drive
    /// animation state machine clip selection.
    player_state: i32,
}

// ---------------------------------------------------------------------------
// Combat effects fullscreen pass
// ---------------------------------------------------------------------------

const COMBAT_FX_SHADER: &str = r#"
struct CombatFxParams {
    vignette_intensity: f32,
    chromatic_aberration: f32,
    flash_alpha: f32,
    pad0: f32,
};

@group(0) @binding(0) var scene_texture: texture_2d<f32>;
@group(0) @binding(1) var scene_sampler: sampler;
@group(0) @binding(2) var<uniform> params: CombatFxParams;

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex fn vs(@builtin(vertex_index) vid: u32) -> VsOut {
    var pos = array<vec2<f32>, 3>(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    var uv = array<vec2<f32>, 3>(vec2(0.0, 1.0), vec2(2.0, 1.0), vec2(0.0, -1.0));
    var out: VsOut;
    out.position = vec4(pos[vid], 0.0, 1.0);
    out.uv = uv[vid];
    return out;
}

@fragment fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let centre = vec2(0.5, 0.5);
    let offset_dir = (uv - centre) * params.chromatic_aberration * 0.012;
    let r = textureSample(scene_texture, scene_sampler, uv + offset_dir).r;
    let g = textureSample(scene_texture, scene_sampler, uv).g;
    let b = textureSample(scene_texture, scene_sampler, uv - offset_dir).b;
    var color = vec3(r, g, b);

    let dist = length(uv - centre) * 1.414;
    let vignette = 1.0 - smoothstep(0.3, 1.0, dist) * params.vignette_intensity * 0.6;
    color = color * vignette;

    color = mix(color, vec3(1.0, 1.0, 1.0), params.flash_alpha * 0.35);

    return vec4(color, 1.0);
}
"#;

#[repr(C)]
#[derive(Clone, Copy)]
struct CombatFxUniforms {
    vignette_intensity: f32,
    chromatic_aberration: f32,
    flash_alpha: f32,
    _pad: f32,
}

unsafe impl bytemuck::Pod for CombatFxUniforms {}
unsafe impl bytemuck::Zeroable for CombatFxUniforms {}

const PBR_SHADER_3D: &str = r#"
const PI: f32 = 3.14159265359;

struct CameraUniform {
    view_proj: mat4x4<f32>,
    camera_pos: vec4<f32>,
    light_dir: vec4<f32>,
    light_color: vec4<f32>,
    ambient: vec4<f32>,
    time: vec4<f32>,
    fog_color_and_start: vec4<f32>,  // xyz = fog color (linear), w = fog start distance
    fog_params: vec4<f32>,           // x = fog end, y = fog density, z = fog height falloff
    // Wind parameters: x = time, y = strength, z = turbulence, w = unused
    wind_params: vec4<f32>,
    // Wind direction: xyz = direction, w = unused
    wind_dir: vec4<f32>,
};

struct ModelEntry {
    model: mat4x4<f32>,
    normal_model: mat4x4<f32>,
};

struct MaterialUniforms {
    base_color_factor: vec4<f32>,
    metallic_factor: f32,
    roughness_factor: f32,
    _padding0: f32,
    _padding1: f32,
};

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(1) @binding(0) var<storage, read> model_matrices: array<ModelEntry>;
@group(1) @binding(1) var<storage, read> joint_palette: array<mat4x4<f32>>;

@group(2) @binding(0) var albedo_texture: texture_2d<f32>;
@group(2) @binding(1) var normal_map: texture_2d<f32>;
@group(2) @binding(2) var orm_texture: texture_2d<f32>;
@group(2) @binding(3) var material_sampler: sampler;
@group(2) @binding(4) var<uniform> material: MaterialUniforms;

const SHADOW_CASCADE_COUNT: u32 = 3u;
const SHADOW_CASCADE_RESOLUTION: f32 = 2048.0;
const SHADOW_ATLAS_WIDTH: f32 = 6144.0;

struct ShadowData {
    light_view_proj_0: mat4x4<f32>,
    light_view_proj_1: mat4x4<f32>,
    light_view_proj_2: mat4x4<f32>,
    cascade_splits: vec4<f32>,
    bias: vec4<f32>,
};

@group(3) @binding(0) var shadow_atlas: texture_depth_2d;
@group(3) @binding(1) var shadow_sampler: sampler_comparison;
@group(3) @binding(2) var<uniform> shadow_data: ShadowData;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) joint_indices: vec4<u32>,
    @location(4) joint_weights: vec4<f32>,
    @location(5) vertex_color: vec4<f32>,
    @location(6) tangent: vec4<f32>,
    @builtin(instance_index) instance_id: u32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) world_tangent: vec3<f32>,
    @location(4) tangent_handedness: f32,
};

// Wind displacement function — shared with shadow shader for consistency
fn compute_wind_displacement(pos: vec3<f32>, wind_color: vec4<f32>, time: f32, wind_direction: vec3<f32>, strength: f32, turbulence: f32) -> vec3<f32> {
    let trunk_weight = wind_color.r;
    let branch_weight = wind_color.g;
    let leaf_weight = wind_color.b;
    let phase_offset = wind_color.a;

    // If no wind weights, skip entirely
    let total_weight = trunk_weight + branch_weight + leaf_weight;
    if (total_weight < 0.001) {
        return vec3<f32>(0.0);
    }

    // Gust cycle: slow sine modulation (period ~8 seconds)
    let gust = sin(time * 0.7854) * 0.5 + 0.5; // 2*PI/8 = 0.7854
    let effective_strength = strength * (0.6 + 0.4 * gust);

    // Trunk sway: slow, large displacement along wind direction
    let trunk_phase = time * 1.2 + phase_offset * 6.283;
    let trunk_sway = wind_direction * sin(trunk_phase) * trunk_weight * effective_strength;

    // Branch oscillation: medium frequency, perpendicular-ish
    let branch_phase = time * 3.5 + phase_offset * 12.566;
    let branch_perp = normalize(vec3<f32>(-wind_direction.z, 0.0, wind_direction.x));
    let branch_osc = (wind_direction * sin(branch_phase) * 0.5 + branch_perp * cos(branch_phase * 1.3) * 0.3) * branch_weight * effective_strength * 0.5;

    // Leaf flutter: high frequency, small chaotic displacement
    let leaf_phase = time * 8.0 + phase_offset * 25.13;
    let leaf_disp = vec3<f32>(
        sin(leaf_phase * 1.1 + pos.x * 2.0) * 0.3,
        sin(leaf_phase * 1.7 + pos.y * 3.0) * 0.15,
        cos(leaf_phase * 0.9 + pos.z * 2.5) * 0.3
    ) * leaf_weight * effective_strength * turbulence * 0.3;

    return trunk_sway + branch_osc + leaf_disp;
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let entry = model_matrices[in.instance_id];

    // Apply skeletal skinning when joint weights are non-zero
    let weight_sum = in.joint_weights[0] + in.joint_weights[1]
                   + in.joint_weights[2] + in.joint_weights[3];
    var skinned_pos: vec4<f32>;
    var skinned_normal: vec3<f32>;
    var skinned_tangent: vec3<f32>;
    if (weight_sum > 0.0) {
        let skin_matrix = joint_palette[in.joint_indices[0]] * in.joint_weights[0]
                        + joint_palette[in.joint_indices[1]] * in.joint_weights[1]
                        + joint_palette[in.joint_indices[2]] * in.joint_weights[2]
                        + joint_palette[in.joint_indices[3]] * in.joint_weights[3];
        skinned_pos = skin_matrix * vec4<f32>(in.position, 1.0);
        skinned_normal = (skin_matrix * vec4<f32>(in.normal, 0.0)).xyz;
        skinned_tangent = (skin_matrix * vec4<f32>(in.tangent.xyz, 0.0)).xyz;
    } else {
        skinned_pos = vec4<f32>(in.position, 1.0);
        skinned_normal = in.normal;
        skinned_tangent = in.tangent.xyz;
    }

    // Apply wind displacement after skinning but before model transform
    let wind_offset = compute_wind_displacement(
        skinned_pos.xyz,
        in.vertex_color,
        camera.wind_params.x,   // time
        camera.wind_dir.xyz,    // direction
        camera.wind_params.y,   // strength
        camera.wind_params.z    // turbulence
    );
    skinned_pos = vec4<f32>(skinned_pos.xyz + wind_offset, skinned_pos.w);

    let world_pos = entry.model * skinned_pos;
    out.world_position = world_pos.xyz;
    out.clip_position = camera.view_proj * world_pos;
    out.world_normal = normalize((entry.normal_model * vec4<f32>(skinned_normal, 0.0)).xyz);
    out.world_tangent = normalize((entry.model * vec4<f32>(skinned_tangent, 0.0)).xyz);
    out.tangent_handedness = in.tangent.w;
    out.uv = in.uv;
    return out;
}

fn fresnel_schlick(cos_theta: f32, f0: vec3<f32>) -> vec3<f32> {
    let one_minus = clamp(1.0 - cos_theta, 0.0, 1.0);
    return f0 + (vec3<f32>(1.0, 1.0, 1.0) - f0) * pow(one_minus, 5.0);
}

fn distribution_ggx(normal: vec3<f32>, half_vector: vec3<f32>, roughness: f32) -> f32 {
    let alpha = roughness * roughness;
    let alpha2 = alpha * alpha;
    let n_dot_h = max(dot(normal, half_vector), 0.0);
    let n_dot_h2 = n_dot_h * n_dot_h;
    let denom = (n_dot_h2 * (alpha2 - 1.0) + 1.0);
    return alpha2 / max(PI * denom * denom, 0.0001);
}

fn geometry_schlick_ggx(n_dot_v: f32, roughness: f32) -> f32 {
    let r = roughness + 1.0;
    let k = (r * r) / 8.0;
    return n_dot_v / (n_dot_v * (1.0 - k) + k);
}

fn geometry_smith(normal: vec3<f32>, view: vec3<f32>, light: vec3<f32>, roughness: f32) -> f32 {
    let n_dot_v = max(dot(normal, view), 0.0);
    let n_dot_l = max(dot(normal, light), 0.0);
    return geometry_schlick_ggx(n_dot_v, roughness) * geometry_schlick_ggx(n_dot_l, roughness);
}

// Compute a TBN matrix from vertex tangent attributes for accurate normal mapping.
// Falls back to screen-space derivatives when the tangent vector is degenerate.
fn compute_tbn(world_tangent: vec3<f32>, world_normal: vec3<f32>, handedness: f32, world_pos: vec3<f32>, uv: vec2<f32>) -> mat3x3<f32> {
    let n = normalize(world_normal);
    let tangent_len = length(world_tangent);

    // dpdx/dpdy must be called outside of non-uniform branches (WGSL rule).
    let dp1 = dpdx(world_pos);
    let dp2 = dpdy(world_pos);
    let duv1 = dpdx(uv);
    let duv2 = dpdy(uv);

    var t: vec3<f32>;
    var b: vec3<f32>;
    if (tangent_len > 0.001) {
        // Gram-Schmidt re-orthogonalize tangent against normal
        let raw_t = normalize(world_tangent);
        t = normalize(raw_t - n * dot(n, raw_t));
        b = cross(n, t) * handedness;
    } else {
        // Fallback: screen-space derivative method
        let det = duv1.x * duv2.y - duv1.y * duv2.x;
        let inv_det = select(1.0 / det, 0.0, abs(det) < 0.0001);
        t = normalize((dp1 * duv2.y - dp2 * duv1.y) * inv_det);
        b = normalize((dp2 * duv1.x - dp1 * duv2.x) * inv_det);
    }

    return mat3x3<f32>(t, b, n);
}

fn get_light_view_proj(cascade_index: u32) -> mat4x4<f32> {
    if cascade_index == 0u {
        return shadow_data.light_view_proj_0;
    }
    if cascade_index == 1u {
        return shadow_data.light_view_proj_1;
    }
    return shadow_data.light_view_proj_2;
}

fn sample_shadow_pcf(world_pos: vec3<f32>, normal: vec3<f32>, cascade_index: u32) -> f32 {
    let light_view_proj = get_light_view_proj(cascade_index);

    // Apply normal bias: push the position along the surface normal
    let normal_bias = shadow_data.bias.x;
    let biased_pos = world_pos + normal * normal_bias;

    let light_clip = light_view_proj * vec4<f32>(biased_pos, 1.0);
    let light_ndc = light_clip.xyz / light_clip.w;

    // Convert from NDC [-1,1] to UV [0,1] (x,y) and keep z for depth compare
    let shadow_uv = vec2<f32>(light_ndc.x * 0.5 + 0.5, 1.0 - (light_ndc.y * 0.5 + 0.5));
    let shadow_depth = light_ndc.z;

    // Check if sample is outside shadow map — resolved via mix() to avoid non-uniform control flow
    let out_of_bounds = f32(
        shadow_uv.x < 0.0 || shadow_uv.x > 1.0 ||
        shadow_uv.y < 0.0 || shadow_uv.y > 1.0 ||
        shadow_depth < 0.0 || shadow_depth > 1.0
    );
    let clamped_uv = clamp(shadow_uv, vec2(0.001), vec2(0.999));
    let clamped_depth = clamp(shadow_depth, 0.0, 1.0);

    // Offset UV into the correct cascade region of the atlas
    let cascade_offset_x = f32(cascade_index) * SHADOW_CASCADE_RESOLUTION / SHADOW_ATLAS_WIDTH;
    let cascade_scale_x = SHADOW_CASCADE_RESOLUTION / SHADOW_ATLAS_WIDTH;
    let atlas_uv = vec2<f32>(
        clamped_uv.x * cascade_scale_x + cascade_offset_x,
        clamped_uv.y
    );

    // 5x5 Poisson disc PCF for softer shadow edges (25 samples)
    let texel_size = vec2<f32>(1.0 / SHADOW_ATLAS_WIDTH, 1.0 / SHADOW_CASCADE_RESOLUTION);
    var shadow_sum: f32 = 0.0;
    shadow_sum += textureSampleCompare(shadow_atlas, shadow_sampler, atlas_uv + vec2<f32>(-2.17, -1.35) * texel_size, clamped_depth);
    shadow_sum += textureSampleCompare(shadow_atlas, shadow_sampler, atlas_uv + vec2<f32>(-0.63, -2.38) * texel_size, clamped_depth);
    shadow_sum += textureSampleCompare(shadow_atlas, shadow_sampler, atlas_uv + vec2<f32>(1.12, -1.87) * texel_size, clamped_depth);
    shadow_sum += textureSampleCompare(shadow_atlas, shadow_sampler, atlas_uv + vec2<f32>(2.34, -0.52) * texel_size, clamped_depth);
    shadow_sum += textureSampleCompare(shadow_atlas, shadow_sampler, atlas_uv + vec2<f32>(0.54, -0.37) * texel_size, clamped_depth);
    shadow_sum += textureSampleCompare(shadow_atlas, shadow_sampler, atlas_uv + vec2<f32>(-1.41, 0.25) * texel_size, clamped_depth);
    shadow_sum += textureSampleCompare(shadow_atlas, shadow_sampler, atlas_uv + vec2<f32>(-0.08, 0.72) * texel_size, clamped_depth);
    shadow_sum += textureSampleCompare(shadow_atlas, shadow_sampler, atlas_uv + vec2<f32>(1.67, 0.83) * texel_size, clamped_depth);
    shadow_sum += textureSampleCompare(shadow_atlas, shadow_sampler, atlas_uv + vec2<f32>(-2.41, 0.93) * texel_size, clamped_depth);
    shadow_sum += textureSampleCompare(shadow_atlas, shadow_sampler, atlas_uv + vec2<f32>(-0.92, -1.18) * texel_size, clamped_depth);
    shadow_sum += textureSampleCompare(shadow_atlas, shadow_sampler, atlas_uv + vec2<f32>(0.21, 1.95) * texel_size, clamped_depth);
    shadow_sum += textureSampleCompare(shadow_atlas, shadow_sampler, atlas_uv + vec2<f32>(-1.63, 1.87) * texel_size, clamped_depth);
    shadow_sum += textureSampleCompare(shadow_atlas, shadow_sampler, atlas_uv + vec2<f32>(2.15, 1.78) * texel_size, clamped_depth);
    shadow_sum += textureSampleCompare(shadow_atlas, shadow_sampler, atlas_uv + vec2<f32>(0.87, 2.41) * texel_size, clamped_depth);
    shadow_sum += textureSampleCompare(shadow_atlas, shadow_sampler, atlas_uv + vec2<f32>(-0.38, -0.84) * texel_size, clamped_depth);
    shadow_sum += textureSampleCompare(shadow_atlas, shadow_sampler, atlas_uv + vec2<f32>(1.53, -1.13) * texel_size, clamped_depth);
    shadow_sum += textureSampleCompare(shadow_atlas, shadow_sampler, atlas_uv + vec2<f32>(-1.95, -0.63) * texel_size, clamped_depth);
    shadow_sum += textureSampleCompare(shadow_atlas, shadow_sampler, atlas_uv + vec2<f32>(0.02, -1.52) * texel_size, clamped_depth);
    shadow_sum += textureSampleCompare(shadow_atlas, shadow_sampler, atlas_uv + vec2<f32>(-0.71, 1.37) * texel_size, clamped_depth);
    shadow_sum += textureSampleCompare(shadow_atlas, shadow_sampler, atlas_uv + vec2<f32>(1.28, 0.12) * texel_size, clamped_depth);
    shadow_sum += textureSampleCompare(shadow_atlas, shadow_sampler, atlas_uv + vec2<f32>(-2.08, -1.92) * texel_size, clamped_depth);
    shadow_sum += textureSampleCompare(shadow_atlas, shadow_sampler, atlas_uv + vec2<f32>(0.73, -2.31) * texel_size, clamped_depth);
    shadow_sum += textureSampleCompare(shadow_atlas, shadow_sampler, atlas_uv + vec2<f32>(2.47, 0.41) * texel_size, clamped_depth);
    shadow_sum += textureSampleCompare(shadow_atlas, shadow_sampler, atlas_uv + vec2<f32>(-1.17, 2.28) * texel_size, clamped_depth);
    shadow_sum += textureSampleCompare(shadow_atlas, shadow_sampler, atlas_uv + vec2<f32>(0.0, 0.0) * texel_size, clamped_depth);

    return mix(shadow_sum / 25.0, 1.0, out_of_bounds);
}

fn compute_shadow(world_pos: vec3<f32>, normal: vec3<f32>) -> f32 {
    // Compute clip-space depth for cascade selection (use w component = view-space depth)
    let clip_pos = camera.view_proj * vec4<f32>(world_pos, 1.0);
    let view_depth = clip_pos.w;

    // Select cascade based on view-space depth vs cascade splits
    var cascade_index: u32 = 0u;
    if view_depth > shadow_data.cascade_splits.y {
        cascade_index = 2u;
    } else if view_depth > shadow_data.cascade_splits.x {
        cascade_index = 1u;
    }

    return sample_shadow_pcf(world_pos, normal, cascade_index);
}

fn apply_fog(color: vec3<f32>, world_pos: vec3<f32>, cam_pos: vec3<f32>) -> vec3<f32> {
    let fog_color = camera.fog_color_and_start.xyz;
    let fog_density = camera.fog_params.y;
    let fog_height_falloff = camera.fog_params.z;

    let dist = distance(world_pos, cam_pos);
    let view_dir = normalize(world_pos - cam_pos);

    // Exponential height fog: thicker near ground, thins with altitude
    // Based on: integral of density * exp(-height_falloff * y) along view ray
    let height_factor = exp(-fog_height_falloff * max(world_pos.y, 0.0));
    let fog_amount = clamp(1.0 - exp(-fog_density * dist * height_factor), 0.0, 1.0);

    // In-scattering: forward-scatter glow when looking toward the sun
    let sun_dir = normalize(-camera.light_dir.xyz);
    let scatter_dot = max(dot(view_dir, sun_dir), 0.0);
    let in_scatter = pow(scatter_dot, 8.0) * camera.light_color.xyz * 0.15;

    return mix(color, fog_color + in_scatter, fog_amount);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Sample albedo texture and multiply by material base color factor
    let albedo_sample = textureSample(albedo_texture, material_sampler, in.uv);
    let base_color = albedo_sample.rgb * material.base_color_factor.rgb;
    let alpha = albedo_sample.a * material.base_color_factor.a;

    // Sample ORM texture: R=AO, G=roughness, B=metallic
    let orm_sample = textureSample(orm_texture, material_sampler, in.uv);
    let ao = orm_sample.r;
    let roughness = orm_sample.g * material.roughness_factor;
    let metallic = orm_sample.b * material.metallic_factor;

    // Sample normal map and compute perturbed normal via TBN
    let normal_sample = textureSample(normal_map, material_sampler, in.uv).rgb;
    let tangent_normal = normalize(normal_sample * 2.0 - vec3<f32>(1.0, 1.0, 1.0));
    let tbn = compute_tbn(in.world_tangent, in.world_normal, in.tangent_handedness, in.world_position, in.uv);
    let normal = normalize(tbn * tangent_normal);

    let view = normalize(camera.camera_pos.xyz - in.world_position);
    let light = normalize(-camera.light_dir.xyz);

    let n_dot_l = max(dot(normal, light), 0.0);
    let n_dot_v = max(dot(normal, view), 0.0);
    let half_vector = normalize(view + light);

    let f0 = mix(vec3<f32>(0.04, 0.04, 0.04), base_color, metallic);
    let fresnel = fresnel_schlick(max(dot(half_vector, view), 0.0), f0);
    let distribution = distribution_ggx(normal, half_vector, roughness);
    let geometry = geometry_smith(normal, view, light, roughness);

    let specular = (distribution * geometry * fresnel) / max(4.0 * n_dot_l * n_dot_v, 0.0001);
    let kd = (vec3<f32>(1.0, 1.0, 1.0) - fresnel) * (1.0 - metallic);
    let diffuse = kd * base_color / PI;

    // Shadow attenuation via cascaded shadow mapping with PCF
    let shadow = compute_shadow(in.world_position, normal);

    let ambient_contribution = camera.ambient.xyz * base_color * ao;
    var final_color = (diffuse + specular) * camera.light_color.xyz * n_dot_l * shadow + ambient_contribution;

    // Distance-based fog with height falloff (applied in linear space before tonemapping)
    final_color = apply_fog(final_color, in.world_position, camera.camera_pos.xyz);

    // Output LINEAR HDR color. Tonemapping and gamma correction happen in postprocess.
    return vec4<f32>(final_color, alpha);
}
"#;

fn mat4_inverse_transpose(m: [[f32; 4]; 4]) -> [[f32; 4]; 4] {
    crate::camera_math::mat4_transpose(crate::camera_math::mat4_inverse(m))
}

fn generate_procedural_cube() -> crate::mesh::MeshData {
    use crate::mesh::{MeshData, Vertex3D};

    let faces: [([f32; 3], [[f32; 3]; 4], [[f32; 2]; 4]); 6] = [
        (
            [0.0, 0.0, 1.0],
            [
                [-1.0, -1.0, 1.0],
                [1.0, -1.0, 1.0],
                [1.0, 1.0, 1.0],
                [-1.0, 1.0, 1.0],
            ],
            [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        ),
        (
            [0.0, 0.0, -1.0],
            [
                [1.0, -1.0, -1.0],
                [-1.0, -1.0, -1.0],
                [-1.0, 1.0, -1.0],
                [1.0, 1.0, -1.0],
            ],
            [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        ),
        (
            [0.0, 1.0, 0.0],
            [
                [-1.0, 1.0, 1.0],
                [1.0, 1.0, 1.0],
                [1.0, 1.0, -1.0],
                [-1.0, 1.0, -1.0],
            ],
            [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        ),
        (
            [0.0, -1.0, 0.0],
            [
                [-1.0, -1.0, -1.0],
                [1.0, -1.0, -1.0],
                [1.0, -1.0, 1.0],
                [-1.0, -1.0, 1.0],
            ],
            [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        ),
        (
            [1.0, 0.0, 0.0],
            [
                [1.0, -1.0, 1.0],
                [1.0, -1.0, -1.0],
                [1.0, 1.0, -1.0],
                [1.0, 1.0, 1.0],
            ],
            [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        ),
        (
            [-1.0, 0.0, 0.0],
            [
                [-1.0, -1.0, -1.0],
                [-1.0, -1.0, 1.0],
                [-1.0, 1.0, 1.0],
                [-1.0, 1.0, -1.0],
            ],
            [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        ),
    ];

    let mut vertices = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);

    for (normal, positions, uvs) in &faces {
        let base = vertices.len() as u32;
        for i in 0..4 {
            vertices.push(Vertex3D {
                position: positions[i],
                normal: *normal,
                uv: uvs[i],
                joint_indices: [0, 0, 0, 0],
                joint_weights: [0.0, 0.0, 0.0, 0.0],
                vertex_color: [0.0, 0.0, 0.0, 0.0],
                tangent: [0.0, 0.0, 0.0, 1.0],
            });
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    crate::mesh::compute_tangents(&mut vertices, &indices);

    MeshData {
        vertices,
        indices,
        material: crate::material::MaterialData::default(),
    }
}

fn generate_ground_plane() -> crate::mesh::MeshData {
    use crate::mesh::{MeshData, Vertex3D};

    let half = 20.0f32;
    let mut vertices = vec![
        Vertex3D {
            position: [-half, 0.0, -half],
            normal: [0.0, 1.0, 0.0],
            uv: [0.0, 0.0],
            joint_indices: [0, 0, 0, 0],
            joint_weights: [0.0, 0.0, 0.0, 0.0],
            vertex_color: [0.0, 0.0, 0.0, 0.0],
            tangent: [0.0, 0.0, 0.0, 1.0],
        },
        Vertex3D {
            position: [half, 0.0, -half],
            normal: [0.0, 1.0, 0.0],
            uv: [1.0, 0.0],
            joint_indices: [0, 0, 0, 0],
            joint_weights: [0.0, 0.0, 0.0, 0.0],
            vertex_color: [0.0, 0.0, 0.0, 0.0],
            tangent: [0.0, 0.0, 0.0, 1.0],
        },
        Vertex3D {
            position: [half, 0.0, half],
            normal: [0.0, 1.0, 0.0],
            uv: [1.0, 1.0],
            joint_indices: [0, 0, 0, 0],
            joint_weights: [0.0, 0.0, 0.0, 0.0],
            vertex_color: [0.0, 0.0, 0.0, 0.0],
            tangent: [0.0, 0.0, 0.0, 1.0],
        },
        Vertex3D {
            position: [-half, 0.0, half],
            normal: [0.0, 1.0, 0.0],
            uv: [0.0, 1.0],
            joint_indices: [0, 0, 0, 0],
            joint_weights: [0.0, 0.0, 0.0, 0.0],
            vertex_color: [0.0, 0.0, 0.0, 0.0],
            tangent: [0.0, 0.0, 0.0, 1.0],
        },
    ];
    let indices = vec![0, 1, 2, 0, 2, 3];
    crate::mesh::compute_tangents(&mut vertices, &indices);
    MeshData {
        vertices,
        indices,
        material: crate::material::MaterialData::default(),
    }
}

/// Generate a subdivided grid-floor quad for the preview mode.
/// Spans from (-10, 0, -10) to (10, 0, 10) with a 20x20 grid subdivision.
/// Uses a dark gray albedo with checkerboard pattern baked into the material.
fn generate_preview_grid_plane() -> crate::mesh::MeshData {
    use crate::mesh::Vertex3D;

    let grid_cells = 20u32;
    let half_extent = 10.0f32;
    let cell_size = (2.0 * half_extent) / grid_cells as f32;
    let vert_per_side = grid_cells + 1;

    let mut vertices = Vec::with_capacity((vert_per_side * vert_per_side) as usize);
    let mut indices = Vec::with_capacity((grid_cells * grid_cells * 6) as usize);

    for iz in 0..vert_per_side {
        for ix in 0..vert_per_side {
            let x = -half_extent + ix as f32 * cell_size;
            let z = -half_extent + iz as f32 * cell_size;
            let u = ix as f32 / grid_cells as f32;
            let v = iz as f32 / grid_cells as f32;
            vertices.push(Vertex3D {
                position: [x, 0.0, z],
                normal: [0.0, 1.0, 0.0],
                uv: [u, v],
                joint_indices: [0, 0, 0, 0],
                joint_weights: [0.0, 0.0, 0.0, 0.0],
                vertex_color: [0.0, 0.0, 0.0, 0.0],
                tangent: [0.0, 0.0, 0.0, 1.0],
            });
        }
    }

    for iz in 0..grid_cells {
        for ix in 0..grid_cells {
            let bl = iz * vert_per_side + ix;
            let br = bl + 1;
            let tl = bl + vert_per_side;
            let tr = tl + 1;
            indices.push(bl);
            indices.push(br);
            indices.push(tr);
            indices.push(bl);
            indices.push(tr);
            indices.push(tl);
        }
    }

    // Dark gray checkerboard albedo texture (8x8 pixels)
    let tex_size = 8u32;
    let mut albedo_data = Vec::with_capacity((tex_size * tex_size * 4) as usize);
    for y in 0..tex_size {
        for x in 0..tex_size {
            let dark = (x + y) % 2 == 0;
            let val = if dark { 60u8 } else { 80u8 };
            albedo_data.extend_from_slice(&[val, val, val, 255]);
        }
    }

    let albedo_image = crate::material::TextureImage {
        width: tex_size,
        height: tex_size,
        rgba_data: albedo_data,
    };

    crate::mesh::compute_tangents(&mut vertices, &indices);

    crate::mesh::MeshData {
        vertices,
        indices,
        material: crate::material::MaterialData {
            albedo_image: Some(albedo_image),
            normal_image: None,
            orm_image: None,
            uniforms: crate::material::MaterialUniforms {
                base_color_factor: [1.0, 1.0, 1.0, 1.0],
                metallic_factor: 0.0,
                roughness_factor: 0.95,
                _padding: [0.0; 2],
            },
        },
    }
}

/// Dynamic mesh base indices for each forest asset. Each asset may produce
/// multiple GPU meshes (one per primitive), so these are set at load time by
/// `load_forest_procedural_assets` rather than hardcoded.
struct ForestMeshBases {
    ground: usize,
    tree_trunk: usize,
    tree_foliage: usize,
    rock: usize,
    player: usize,
    enemy: usize,
}

struct ForestSceneBuildResult {
    instances: Vec<MeshInstance>,
    player_instance_index: usize,
    enemy_instance_indices: Vec<usize>,
    camera_anchor_count: usize,
    default_camera_anchor: RuntimeSceneCameraAnchor,
    combat_arena_extents: RuntimeCombatArenaExtents,
    fog_volume_count: usize,
    lut_profile_id: String,
}

#[derive(Clone, Debug)]
struct RuntimeSceneCameraAnchor {
    id: String,
    position: [f32; 3],
    target: [f32; 3],
    fov_y_radians: f32,
}

#[derive(Clone, Copy, Debug)]
struct RuntimeCombatArenaExtents {
    min: [f32; 3],
    max: [f32; 3],
}

impl RuntimeCombatArenaExtents {
    fn center(self) -> [f32; 3] {
        [
            (self.min[0] + self.max[0]) * 0.5,
            (self.min[1] + self.max[1]) * 0.5,
            (self.min[2] + self.max[2]) * 0.5,
        ]
    }
}

/// Deterministic pseudo-random float in [0, 1) seeded from an integer index.
/// Uses a simple hash to produce visual variety without runtime randomness.
fn deterministic_hash_f32(seed: u32) -> f32 {
    // Bit mixing from splitmix32
    let mut x = seed.wrapping_mul(0x9E3779B9);
    x ^= x >> 16;
    x = x.wrapping_mul(0x45D9F3B);
    x ^= x >> 16;
    (x & 0x00FF_FFFF) as f32 / 16_777_216.0
}

/// Build the forest clearing scene: ground, ring of trees, scattered rocks,
/// player character, and rot stalker enemy.
fn build_forest_scene(
    bases: &ForestMeshBases,
    enemy_instance_count: usize,
) -> ForestSceneBuildResult {
    use crate::camera_math::{
        mat4_identity, mat4_mul, mat4_rotation_y, mat4_scale, mat4_translation,
    };

    let mut instances = Vec::with_capacity(128);

    // ── Ground plane at origin ────────────────────────────────────────
    instances.push(MeshInstance {
        mesh_index: bases.ground,
        model_matrix: mat4_identity(),
    });

    // ── Primary tree ring around the clearing ─────────────────────────
    // 10 trees at radius ~9.0 with angular jitter and deterministic rotation
    const TREE_COUNT: usize = 10;
    const TREE_RING_RADIUS: f32 = 9.0;
    for i in 0..TREE_COUNT {
        let base_angle = (i as f32) * (2.0 * std::f32::consts::PI / TREE_COUNT as f32);
        let jitter = (deterministic_hash_f32(i as u32 * 7 + 100) - 0.5) * 0.3;
        let angle = base_angle + jitter;

        let x = TREE_RING_RADIUS * angle.cos();
        let z = TREE_RING_RADIUS * angle.sin();

        let rot_angle = deterministic_hash_f32(i as u32 * 13 + 200) * std::f32::consts::TAU;
        let model = mat4_mul(mat4_translation(x, 0.0, z), mat4_rotation_y(rot_angle));

        instances.push(MeshInstance {
            mesh_index: bases.tree_trunk,
            model_matrix: model,
        });
        instances.push(MeshInstance {
            mesh_index: bases.tree_foliage,
            model_matrix: model,
        });
    }

    // ── Smaller tree clusters filling the mid-ground ──────────────────
    // 25 scaled-down trees at radius 4-7 between the main ring and the center
    const SMALL_TREE_COUNT: usize = 25;
    for i in 0..SMALL_TREE_COUNT {
        let angle = deterministic_hash_f32(i as u32 * 11 + 500) * std::f32::consts::TAU;
        let radius = 4.0 + deterministic_hash_f32(i as u32 * 19 + 510) * 3.0;
        let x = radius * angle.cos();
        let z = radius * angle.sin();

        let rot_angle = deterministic_hash_f32(i as u32 * 29 + 520) * std::f32::consts::TAU;
        let s = 0.5 + deterministic_hash_f32(i as u32 * 37 + 530) * 0.2; // scale 0.5-0.7
        let model = mat4_mul(
            mat4_mul(mat4_translation(x, 0.0, z), mat4_rotation_y(rot_angle)),
            mat4_scale(s, s, s),
        );

        instances.push(MeshInstance {
            mesh_index: bases.tree_trunk,
            model_matrix: model,
        });
        instances.push(MeshInstance {
            mesh_index: bases.tree_foliage,
            model_matrix: model,
        });
    }

    // ── Second (outer) tree ring for depth/backdrop ───────────────────
    // 15 trees at radius 12-15 forming a dense backdrop
    const OUTER_TREE_COUNT: usize = 15;
    for i in 0..OUTER_TREE_COUNT {
        let base_angle = (i as f32) * (2.0 * std::f32::consts::PI / OUTER_TREE_COUNT as f32);
        let jitter = (deterministic_hash_f32(i as u32 * 7 + 600) - 0.5) * 0.35;
        let angle = base_angle + jitter;
        let radius = 12.0 + deterministic_hash_f32(i as u32 * 11 + 610) * 3.0;

        let x = radius * angle.cos();
        let z = radius * angle.sin();

        let rot_angle = deterministic_hash_f32(i as u32 * 13 + 620) * std::f32::consts::TAU;
        // Slightly varied scale for the backdrop trees (0.9-1.1)
        let s = 0.9 + deterministic_hash_f32(i as u32 * 17 + 630) * 0.2;
        let model = mat4_mul(
            mat4_mul(mat4_translation(x, 0.0, z), mat4_rotation_y(rot_angle)),
            mat4_scale(s, s, s),
        );

        instances.push(MeshInstance {
            mesh_index: bases.tree_trunk,
            model_matrix: model,
        });
        instances.push(MeshInstance {
            mesh_index: bases.tree_foliage,
            model_matrix: model,
        });
    }

    // ── Rocks scattered around the clearing edge (original) ───────────
    // 5 rocks at varying distances between 5.0 and 8.0
    const ROCK_COUNT: usize = 5;
    for i in 0..ROCK_COUNT {
        let base_angle = (i as f32) * (2.0 * std::f32::consts::PI / ROCK_COUNT as f32) + 0.4; // offset so rocks don't overlap tree positions
        let radius = 5.0 + deterministic_hash_f32(i as u32 * 17 + 300) * 3.0;
        let x = radius * base_angle.cos();
        let z = radius * base_angle.sin();

        let rot_angle = deterministic_hash_f32(i as u32 * 23 + 400) * std::f32::consts::TAU;
        let model = mat4_mul(mat4_translation(x, 0.0, z), mat4_rotation_y(rot_angle));

        instances.push(MeshInstance {
            mesh_index: bases.rock,
            model_matrix: model,
        });
    }

    // ── Small rocks scattered across the clearing floor ───────────────
    // 12 tiny rocks within radius 1-6 for ground detail
    const SMALL_ROCK_COUNT: usize = 12;
    for i in 0..SMALL_ROCK_COUNT {
        let angle = deterministic_hash_f32(i as u32 * 31 + 700) * std::f32::consts::TAU;
        let radius = 1.0 + deterministic_hash_f32(i as u32 * 41 + 710) * 5.0;
        let x = radius * angle.cos();
        let z = radius * angle.sin();

        let rot_angle = deterministic_hash_f32(i as u32 * 43 + 720) * std::f32::consts::TAU;
        let s = 0.3 + deterministic_hash_f32(i as u32 * 47 + 730) * 0.2; // scale 0.3-0.5
        let model = mat4_mul(
            mat4_mul(mat4_translation(x, 0.0, z), mat4_rotation_y(rot_angle)),
            mat4_scale(s, s, s),
        );

        instances.push(MeshInstance {
            mesh_index: bases.rock,
            model_matrix: model,
        });
    }

    // ── Player character at origin ────────────────────────────────────
    let player_instance_index = instances.len();
    instances.push(MeshInstance {
        mesh_index: bases.player,
        model_matrix: mat4_identity(),
    });

    // ── Enemy lane (multi-target hard cut) ─────────────────────────────
    let spawn_count = enemy_instance_count.max(1);
    let mut enemy_instance_indices = Vec::with_capacity(spawn_count);
    let spawn_offsets = [
        (3.0_f32, 3.0_f32),
        (-2.8_f32, 2.6_f32),
        (3.4_f32, -2.6_f32),
        (-3.4_f32, -2.8_f32),
    ];
    for i in 0..spawn_count {
        let (x, z) = spawn_offsets
            .get(i)
            .copied()
            .unwrap_or_else(|| {
                let angle = (i as f32) * std::f32::consts::TAU / spawn_count as f32;
                (3.0 * angle.cos(), 3.0 * angle.sin())
            });
        let facing = x.atan2(z) + std::f32::consts::PI;
        let model = mat4_mul(mat4_translation(x, 0.0, z), mat4_rotation_y(facing));
        enemy_instance_indices.push(instances.len());
        instances.push(MeshInstance {
            mesh_index: bases.enemy,
            model_matrix: model,
        });
    }

    ForestSceneBuildResult {
        instances,
        player_instance_index,
        enemy_instance_indices,
        camera_anchor_count: 1,
        default_camera_anchor: RuntimeSceneCameraAnchor {
            id: "legacy_default".to_string(),
            position: [4.0, 3.0, 7.0],
            target: [0.0, 1.0, 0.0],
            fov_y_radians: std::f32::consts::FRAC_PI_4,
        },
        combat_arena_extents: RuntimeCombatArenaExtents {
            min: [-10.0, -0.5, -10.0],
            max: [10.0, 6.0, 10.0],
        },
        fog_volume_count: 0,
        lut_profile_id: "legacy_default".to_string(),
    }
}

fn configure_player_blend_state_mappings(
    blender: &mut crate::animation_graph::AnimationBlender,
    mapping: &crate::hero_clip_mapping::HeroRuntimeClipMapping,
) {
    let idle = mapping.idle_clip_index;
    let walk = mapping.walk_clip_index;
    let run = mapping.run_clip_index_or_walk();

    blender.set_state_mapping(0, idle);
    blender.set_state_mapping(1, walk);
    blender.set_state_mapping(2, run);
    blender.set_state_mapping(3, idle);
    blender.set_state_mapping(4, idle);
    blender.set_state_mapping(5, idle);
    blender.set_state_mapping(6, idle);
    blender.set_state_mapping(7, idle);
}

/// Upload a slice of procedural `MeshData` into the renderer, returning the
/// base index of the first mesh added. Each mesh also gets a corresponding
/// material uploaded.
fn upload_procedural_meshes(
    renderer: &mut WebGpuRenderer,
    meshes: &[crate::mesh::MeshData],
) -> usize {
    let base = renderer.meshes.len();
    for mesh_data in meshes {
        let gpu_mesh = crate::mesh::upload_mesh(&renderer.device, mesh_data);
        renderer.meshes.push(gpu_mesh);
        let gpu_mat = crate::material::upload_material(
            &renderer.device,
            &renderer.queue,
            &renderer.bind_group_layout_3d_material,
            &renderer.material_sampler,
            &mesh_data.material,
        );
        renderer.materials.push(gpu_mat);
    }
    base
}

/// Load all forest assets via procedural mesh generation and return the base
/// mesh indices for each asset type. Clears any existing meshes so that
/// indices start from 0.
async fn load_forest_procedural_assets(
    renderer: &mut WebGpuRenderer,
    dist_root: &str,
) -> Result<ForestMeshBases, String> {
    renderer.meshes.clear();
    renderer.materials.clear();
    renderer.skeletons.clear();
    renderer.animation_clips.clear();

    // Helper: apply a ProceduralTexture to a MaterialData, setting the texture
    // images and resetting uniforms so the ORM map drives metallic/roughness.
    fn apply_texture(
        mat: &mut crate::material::MaterialData,
        tex: crate::procedural_textures::ProceduralTexture,
    ) {
        mat.albedo_image = Some(tex.albedo);
        mat.normal_image = Some(tex.normal);
        mat.orm_image = Some(tex.orm);
        mat.uniforms.base_color_factor = [1.0, 1.0, 1.0, 1.0];
        mat.uniforms.metallic_factor = 1.0;
        mat.uniforms.roughness_factor = 1.0;
    }

    // --- Ground: grass textures ------------------------------------------
    let grass_tex = crate::procedural_textures::generate_grass_textures();
    let mut ground_meshes = crate::procedural_meshes::generate_ground_plane();
    for mesh in ground_meshes.iter_mut() {
        apply_texture(&mut mesh.material, grass_tex.clone());
    }
    let ground_base = upload_procedural_meshes(renderer, &ground_meshes);

    // --- Tree: trunk = bark, foliage = leaf ------------------------------
    let mut tree_meshes = crate::procedural_meshes::generate_tree();
    if tree_meshes.len() >= 2 {
        apply_texture(
            &mut tree_meshes[0].material,
            crate::procedural_textures::generate_bark_textures(),
        );
        apply_texture(
            &mut tree_meshes[1].material,
            crate::procedural_textures::generate_leaf_textures(),
        );
    }
    let tree_base = upload_procedural_meshes(renderer, &tree_meshes);

    // --- Rock: rock textures ---------------------------------------------
    let rock_tex = crate::procedural_textures::generate_rock_textures();
    let mut rock_meshes = crate::procedural_meshes::generate_rock();
    for mesh in rock_meshes.iter_mut() {
        apply_texture(&mut mesh.material, rock_tex.clone());
    }
    let rock_base = upload_procedural_meshes(renderer, &rock_meshes);

    // --- Player: required hero GLB (hard cutover) ------------------------
    let player_glb_url = resolve_dist_asset_url(dist_root, PLAYER_HERO_GLB_FILE);
    let player_glb_bytes = crate::mesh::fetch_glb_bytes(player_glb_url.as_str())
        .await
        .map_err(|error| {
            format!(
                "hero GLB boot failure: required player asset was not fetchable at '{}': {}",
                player_glb_url, error
            )
        })?;
    let crate::mesh::GlbData {
        meshes: player_meshes,
        skeleton: player_skeleton,
        animation_clips: player_clips,
    } = crate::mesh::load_glb_with_animations(&player_glb_bytes).map_err(|error| {
        format!(
            "hero GLB boot failure: could not parse '{}' with mesh+skeleton+animation data: {}",
            player_glb_url, error
        )
    })?;
    if player_meshes.is_empty() {
        return Err(format!(
            "hero GLB boot failure: '{}' contains no meshes for player rendering",
            player_glb_url
        ));
    }
    if player_clips.is_empty() {
        return Err(format!(
            "hero GLB boot failure: '{}' contains no animation clips; walking pipeline requires animated hero clips",
            player_glb_url
        ));
    }
    let player_skeleton = player_skeleton.ok_or_else(|| {
        format!(
            "hero GLB boot failure: '{}' contains no skeleton; walking pipeline requires a rigged hero skeleton",
            player_glb_url
        )
    })?;
    if player_skeleton.joints.is_empty() {
        return Err(format!(
            "hero GLB boot failure: '{}' contains an empty skeleton; walking pipeline requires at least one joint",
            player_glb_url
        ));
    }
    let player_clip_names = player_clips
        .iter()
        .map(|clip| clip.name.as_str())
        .collect::<Vec<_>>();
    let clip_bindings =
        crate::hero_clip_mapping::resolve_hero_runtime_clip_mapping(&player_clip_names)
            .map_err(|error| format!("hero GLB boot failure at '{}': {}", player_glb_url, error))?;
    let player_base = upload_procedural_meshes(renderer, &player_meshes);
    configure_player_blend_state_mappings(&mut renderer.animation_blender, &clip_bindings);
    renderer.skeletons.push((player_base, player_skeleton));
    renderer.animation_clips.push((player_base, player_clips));

    // --- Enemy: enemy skin textures (generated once, shared across parts)
    let enemy_tex = crate::procedural_textures::generate_enemy_skin_textures();
    let mut enemy_meshes = crate::procedural_meshes::generate_enemy_character();
    for mesh in enemy_meshes.iter_mut() {
        apply_texture(&mut mesh.material, enemy_tex.clone());
    }
    let enemy_base = upload_procedural_meshes(renderer, &enemy_meshes);

    Ok(ForestMeshBases {
        ground: ground_base,
        tree_trunk: tree_base,
        tree_foliage: tree_base + 1,
        rock: rock_base,
        player: player_base,
        enemy: enemy_base,
    })
}

/// Mapping from asset filename to the base mesh index and number of primitives
/// loaded from the corresponding GLB file.
struct SceneAssetMeshEntry {
    /// First mesh index in the renderer's mesh array for this asset.
    base_index: usize,
    /// Number of GPU mesh primitives produced by the GLB.
    primitive_count: usize,
}

/// Load the scene layout manifest and all referenced environment GLBs, then
/// build the scene instances. Also loads the player character and enemy meshes
/// (unchanged from before). Returns the same `ForestSceneBuildResult` consumed
/// by the rest of the init path.
async fn load_scene_from_manifest(
    renderer: &mut WebGpuRenderer,
    dist_root: &str,
    enemy_instance_count: usize,
) -> Result<ForestSceneBuildResult, String> {
    renderer.meshes.clear();
    renderer.materials.clear();
    renderer.skeletons.clear();
    renderer.animation_clips.clear();

    // ── 1. Fetch and parse the manifest ────────────────────────────────
    let manifest_url =
        resolve_dist_asset_url(dist_root, SCENE_LAYOUT_MANIFEST_FILE);
    let manifest_text = fetch_text_asset(&manifest_url, "scene layout manifest").await.map_err(|error| {
        format!(
            "Forest scene boot failed: could not fetch scene layout manifest at '{}': {}",
            manifest_url, error
        )
    })?;
    let manifest = parse_and_validate_scene_layout_manifest(&manifest_text)?;

    // ── 2. Collect unique assets and load each GLB once ────────────────
    let unique_assets = collect_unique_scene_assets(&manifest);
    let mut asset_map: HashMap<String, SceneAssetMeshEntry> = HashMap::new();

    for asset_name in &unique_assets {
        let asset_url = resolve_dist_asset_url(
            dist_root,
            &format!("{}/{}", ENVIRONMENT_ASSET_DIR, asset_name),
        );
        let glb_bytes = crate::mesh::fetch_glb_bytes(&asset_url)
            .await
            .map_err(|error| {
                format!(
                    "Forest scene boot failed: could not fetch asset '{}' at '{}': {}",
                    asset_name, asset_url, error
                )
            })?;
        // Validate GLB structure before uploading
        validate_glb_asset(asset_name, &glb_bytes)?;
        let meshes = crate::mesh::load_glb(&glb_bytes).map_err(|error| {
            format!(
                "Forest scene boot failed: asset '{}' GLB parse error: {}",
                asset_name, error
            )
        })?;
        let base = upload_procedural_meshes(renderer, &meshes);
        asset_map.insert(
            asset_name.clone(),
            SceneAssetMeshEntry {
                base_index: base,
                primitive_count: meshes.len(),
            },
        );
    }
    if !asset_map.contains_key(&manifest.ground.asset) {
        return Err(format!(
            "Forest scene boot failed: manifest ground asset '{}' was not loaded from '{}'",
            manifest.ground.asset, ENVIRONMENT_ASSET_DIR
        ));
    }

    // ── 3. Load the player hero GLB (unchanged from procedural path) ───
    let player_glb_url = resolve_dist_asset_url(dist_root, PLAYER_HERO_GLB_FILE);
    let player_glb_bytes = crate::mesh::fetch_glb_bytes(player_glb_url.as_str())
        .await
        .map_err(|error| {
            format!(
                "hero GLB boot failure: required player asset was not fetchable at '{}': {}",
                player_glb_url, error
            )
        })?;
    let crate::mesh::GlbData {
        meshes: player_meshes,
        skeleton: player_skeleton,
        animation_clips: player_clips,
    } = crate::mesh::load_glb_with_animations(&player_glb_bytes).map_err(|error| {
        format!(
            "hero GLB boot failure: could not parse '{}' with mesh+skeleton+animation data: {}",
            player_glb_url, error
        )
    })?;
    if player_meshes.is_empty() {
        return Err(format!(
            "hero GLB boot failure: '{}' contains no meshes for player rendering",
            player_glb_url
        ));
    }
    if player_clips.is_empty() {
        return Err(format!(
            "hero GLB boot failure: '{}' contains no animation clips; walking pipeline requires animated hero clips",
            player_glb_url
        ));
    }
    let player_skeleton = player_skeleton.ok_or_else(|| {
        format!(
            "hero GLB boot failure: '{}' contains no skeleton; walking pipeline requires a rigged hero skeleton",
            player_glb_url
        )
    })?;
    if player_skeleton.joints.is_empty() {
        return Err(format!(
            "hero GLB boot failure: '{}' contains an empty skeleton; walking pipeline requires at least one joint",
            player_glb_url
        ));
    }
    let player_clip_names = player_clips
        .iter()
        .map(|clip| clip.name.as_str())
        .collect::<Vec<_>>();
    let clip_bindings =
        crate::hero_clip_mapping::resolve_hero_runtime_clip_mapping(&player_clip_names)
            .map_err(|error| format!("hero GLB boot failure at '{}': {}", player_glb_url, error))?;
    let player_base = upload_procedural_meshes(renderer, &player_meshes);
    configure_player_blend_state_mappings(&mut renderer.animation_blender, &clip_bindings);
    renderer.skeletons.push((player_base, player_skeleton));
    renderer
        .animation_clips
        .push((player_base, player_clips));

    // ── 4. Load the enemy (procedural, same as before) ─────────────────
    let enemy_tex = crate::procedural_textures::generate_enemy_skin_textures();
    let mut enemy_meshes = crate::procedural_meshes::generate_enemy_character();
    for mesh in enemy_meshes.iter_mut() {
        fn apply_texture(
            mat: &mut crate::material::MaterialData,
            tex: crate::procedural_textures::ProceduralTexture,
        ) {
            mat.albedo_image = Some(tex.albedo);
            mat.normal_image = Some(tex.normal);
            mat.orm_image = Some(tex.orm);
            mat.uniforms.base_color_factor = [1.0, 1.0, 1.0, 1.0];
            mat.uniforms.metallic_factor = 1.0;
            mat.uniforms.roughness_factor = 1.0;
        }
        apply_texture(&mut mesh.material, enemy_tex.clone());
    }
    let enemy_base = upload_procedural_meshes(renderer, &enemy_meshes);

    // ── 5. Build scene instances from the manifest ─────────────────────
    let scene = build_forest_scene_from_manifest(
        &manifest,
        &asset_map,
        player_base,
        enemy_base,
        enemy_instance_count,
    );
    Ok(scene)
}

/// Build the scene instances from the parsed manifest and loaded asset map.
/// Includes all environment instances (ground + placed objects) plus the
/// player character and enemy character.
fn build_forest_scene_from_manifest(
    manifest: &SceneLayoutManifest,
    asset_map: &HashMap<String, SceneAssetMeshEntry>,
    player_base: usize,
    enemy_base: usize,
    enemy_instance_count: usize,
) -> ForestSceneBuildResult {
    use crate::camera_math::{
        mat4_identity, mat4_mul, mat4_rotation_y, mat4_scale, mat4_translation,
    };

    let selected_anchor = manifest
        .camera_anchors
        .iter()
        .find(|anchor| anchor.id == "combat_default")
        .unwrap_or(&manifest.camera_anchors[0]);
    let default_camera_anchor = RuntimeSceneCameraAnchor {
        id: selected_anchor.id.clone(),
        position: selected_anchor.position,
        target: selected_anchor.target,
        fov_y_radians: selected_anchor.fov_y_degrees.to_radians(),
    };
    let combat_arena_extents = RuntimeCombatArenaExtents {
        min: manifest.combat_arena_extents.min,
        max: manifest.combat_arena_extents.max,
    };

    let mut instances = Vec::with_capacity(manifest.instances.len() + 16);

    // ── Ground plane ───────────────────────────────────────────────────
    if let Some(ground_entry) = asset_map.get(&manifest.ground.asset) {
        let ground_model = mat4_scale(
            manifest.ground.scale[0],
            manifest.ground.scale[1],
            manifest.ground.scale[2],
        );
        for prim in 0..ground_entry.primitive_count {
            instances.push(MeshInstance {
                mesh_index: ground_entry.base_index + prim,
                model_matrix: ground_model,
            });
        }
    }

    // ── Environment instances from manifest ────────────────────────────
    for inst in &manifest.instances {
        if let Some(entry) = asset_map.get(&inst.asset) {
            let rotation_rad = inst.rotation_y * std::f32::consts::PI / 180.0;
            let model = mat4_mul(
                mat4_mul(
                    mat4_translation(inst.position[0], inst.position[1], inst.position[2]),
                    mat4_rotation_y(rotation_rad),
                ),
                mat4_scale(inst.scale[0], inst.scale[1], inst.scale[2]),
            );
            for prim in 0..entry.primitive_count {
                instances.push(MeshInstance {
                    mesh_index: entry.base_index + prim,
                    model_matrix: model,
                });
            }
        }
    }

    // ── Player character at origin ────────────────────────────────────
    let player_instance_index = instances.len();
    instances.push(MeshInstance {
        mesh_index: player_base,
        model_matrix: mat4_identity(),
    });

    // ── Enemy lane (multi-target hard cut) ─────────────────────────────
    let spawn_count = enemy_instance_count.max(1);
    let mut enemy_instance_indices = Vec::with_capacity(spawn_count);
    let spawn_offsets = [
        (3.0_f32, 3.0_f32),
        (-2.8_f32, 2.6_f32),
        (3.4_f32, -2.6_f32),
        (-3.4_f32, -2.8_f32),
    ];
    for i in 0..spawn_count {
        let (x, z) = spawn_offsets
            .get(i)
            .copied()
            .unwrap_or_else(|| {
                let angle = (i as f32) * std::f32::consts::TAU / spawn_count as f32;
                (3.0 * angle.cos(), 3.0 * angle.sin())
            });
        let facing = x.atan2(z) + std::f32::consts::PI;
        let model = mat4_mul(mat4_translation(x, 0.0, z), mat4_rotation_y(facing));
        enemy_instance_indices.push(instances.len());
        instances.push(MeshInstance {
            mesh_index: enemy_base,
            model_matrix: model,
        });
    }

    ForestSceneBuildResult {
        instances,
        player_instance_index,
        enemy_instance_indices,
        camera_anchor_count: manifest.camera_anchors.len(),
        default_camera_anchor,
        combat_arena_extents,
        fog_volume_count: manifest.fog_volumes.len(),
        lut_profile_id: manifest.lut_profile_id.clone(),
    }
}

// fetch_text_asset definition lives below (2-param version with label for diagnostics)

struct RuntimeShaderAssets {
    render_schema_version: String,
    shader_bundle_schema_version: String,
    pipelines: Vec<RuntimePipelineShaderAssets>,
    frame_graph: Vec<RuntimeFrameGraphPassAssets>,
    prewarm_groups: Vec<RuntimePrewarmGroupAssets>,
    gpu_scene_buffers: RuntimeGpuSceneBufferContracts,
    default_profile_contracts: RuntimeDefaultProfileContracts,
    compute_pass_manifest_ready: bool,
}

struct RuntimePipelineShaderAssets {
    pipeline_id: String,
    shader_module_id: String,
    shader_path: String,
    node_target: Option<String>,
    shader_mode: Option<String>,
    shader_source: String,
    vertex_entry: String,
    fragment_entry: String,
    primitive_topology: RenderPrimitiveTopology,
    primitive_cull_mode: RenderCullMode,
}

struct RuntimeFrameGraphPassAssets {
    name: String,
    pipeline_id: String,
    draw_phase: String,
    depends_on: Vec<String>,
    pass_contract_id: String,
    reads: Vec<String>,
    writes: Vec<String>,
    is_compute_pass: bool,
}

struct RuntimePrewarmGroupAssets {
    id: String,
    required: bool,
    shader_modules: Vec<String>,
}

#[derive(Debug, Clone, Default)]
struct FrameGraphRuntimeEvidence {
    render_manifest_schema: String,
    shader_bundle_schema: String,
    frame_graph_declared_passes: u32,
    frame_graph_frames_executed: u64,
    frame_graph_last_render_passes: u32,
    frame_graph_last_compute_passes: u32,
    compute_pass_manifest_ready: bool,
    prewarm_required_groups: u32,
    prewarm_completed_required_groups: u32,
    prewarm_required_complete: bool,
    prewarm_blocked_frames: u64,
    frame_graph_path_used: bool,
    visibility_candidate_draws: u32,
    visibility_visible_draws: u32,
    visibility_culled_ratio: f64,
    visibility_indirect_draw_count: u32,
    visibility_cpu_fallback_used: bool,
    visibility_indirect_path_default: bool,
    hiz_occlusion_tier_enabled: bool,
    lighting_contract_ready: bool,
    shadow_cascade_count: u32,
    shadow_atlas_resolution: u32,
    reflection_fallback_contract_ready: bool,
    ssr_max_steps: u32,
    ssr_max_rays_per_pixel: u32,
    probe_max_active_probes: u32,
    probe_update_ratio: f64,
    temporal_contract_ready: bool,
    dynamic_resolution_policy_enabled: bool,
    dynamic_resolution_min_scale: f64,
    dynamic_resolution_max_scale: f64,
    dynamic_resolution_target_frame_time_ms: f64,
    temporal_metrics_window_frames: u32,
    temporal_metrics_report_interval_ms: u32,
}

fn infer_target_convergence_stage(
    current: RuntimeConvergenceStage,
    evidence: &FrameGraphRuntimeEvidence,
    loaded_chunk_count: u64,
) -> RuntimeConvergenceStage {
    let target = if loaded_chunk_count == 0 {
        RuntimeConvergenceStage::Bootstrap
    } else if evidence.prewarm_required_complete && evidence.frame_graph_frames_executed >= 96 {
        RuntimeConvergenceStage::Converged
    } else if evidence.prewarm_required_complete {
        RuntimeConvergenceStage::Refine
    } else if evidence.frame_graph_frames_executed > 0 {
        RuntimeConvergenceStage::Stream
    } else {
        RuntimeConvergenceStage::Bootstrap
    };
    if target.rank() > current.rank() {
        target
    } else {
        current
    }
}

fn infer_target_residency_class(
    stage: RuntimeConvergenceStage,
    residency_pressure: f64,
) -> RuntimeResidencyClass {
    if residency_pressure >= 0.86 {
        RuntimeResidencyClass::Core
    } else if residency_pressure >= 0.68 {
        RuntimeResidencyClass::Hot
    } else if stage.rank() >= RuntimeConvergenceStage::Refine.rank() {
        RuntimeResidencyClass::Warm
    } else {
        RuntimeResidencyClass::Cold
    }
}

fn build_animation_graph_runtime(
    summary: &AnimationContractLoadSummary,
) -> Result<crate::animation_graph::AnimationGraphState, String> {
    if summary.states.is_empty() {
        return Err("animation contracts resolved zero states".to_string());
    }

    let states = summary
        .states
        .iter()
        .map(|state| crate::animation_graph::AnimationStateDefinition {
            name: state.id.clone(),
            markers: state
                .markers
                .iter()
                .map(|event| crate::animation_graph::AnimationMarker {
                    event: event.clone(),
                    tick: 1,
                })
                .collect(),
            windows: state
                .windows
                .iter()
                .map(|window| crate::animation_graph::AnimationEventWindow {
                    event: window.id.clone(),
                    start_tick: window.start_frame,
                    end_tick: window.end_frame,
                })
                .collect(),
        })
        .collect::<Vec<_>>();
    let transitions = summary
        .transitions
        .iter()
        .map(|transition| crate::animation_graph::AnimationTransition {
            from: transition.from.clone(),
            to: transition.to.clone(),
            after_ticks: transition.after_ticks.max(1),
        })
        .collect::<Vec<_>>();
    let graph = crate::animation_graph::AnimationGraphDefinition::new(states, transitions)?;
    crate::animation_graph::AnimationGraphState::new(graph, summary.states[0].id.as_str())
}

fn build_animation_event_lookup(
    summary: &AnimationContractLoadSummary,
) -> HashMap<String, HashSet<String>> {
    let mut lookup = HashMap::<String, HashSet<String>>::new();
    for state in &summary.states {
        let mut labels = HashSet::<String>::new();
        labels.extend(state.markers.iter().cloned());
        labels.extend(state.windows.iter().map(|window| window.id.clone()));
        lookup.insert(state.id.clone(), labels);
    }
    lookup
}

fn animation_phase_q16_to_state_tick(phase_q16: i32) -> u32 {
    let normalized = phase_q16.max(0) as u32;
    (normalized / ANIMATION_PHASE_TICK_Q16 as u32).max(1)
}

struct RuntimeRenderPipeline {
    id: String,
    pipeline: wgpu::RenderPipeline,
}

#[derive(Clone, Copy)]
struct WebGpuPrimitiveState {
    topology: wgpu::PrimitiveTopology,
    cull_mode: Option<wgpu::Face>,
}

struct FrameGraphRuntimePass {
    name: String,
    draw_phase: String,
    pipeline_index: Option<usize>,
    pass_contract_id: String,
    is_compute_pass: bool,
}

struct RenderExecutionStats {
    upload_bytes: u64,
    render_passes: u32,
    compute_passes: u32,
    visibility: VisibilityStageTelemetry,
    indirect_draw_count: u32,
    frame_cpu_ms: f64,
    pass_timing_supported: bool,
    pass_timing_fallback_used: bool,
    pass_timings: Vec<RuntimePassTimingSample>,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct DrawIndirectRecord {
    vertex_count: u32,
    instance_count: u32,
    first_vertex: u32,
    first_instance: u32,
}

enum RenderFrameResult {
    Rendered(RenderExecutionStats),
    SurfaceTimeout,
    BlockedOnPrewarm,
}

#[derive(Default, Clone)]
struct RuntimePrewarmTracker {
    required_groups: u32,
    completed_required_groups: u32,
    blocked_frames: u64,
    first_frame_gate_released: bool,
}

impl RuntimePrewarmTracker {
    fn from_assets(
        prewarm_groups: &[RuntimePrewarmGroupAssets],
        compiled_shader_modules: &HashSet<String>,
    ) -> Result<Self, String> {
        let mut required_groups = 0u32;
        let mut completed_required_groups = 0u32;

        for group in prewarm_groups {
            if group.required {
                required_groups = required_groups.saturating_add(1);
            }
            let completed = group
                .shader_modules
                .iter()
                .all(|module| compiled_shader_modules.contains(module.as_str()));
            if group.required && completed {
                completed_required_groups = completed_required_groups.saturating_add(1);
            }
            if group.required && !completed {
                return Err(format!(
                    "required prewarm group '{}' did not compile all shader modules",
                    group.id
                ));
            }
        }

        Ok(Self {
            required_groups,
            completed_required_groups,
            blocked_frames: 0,
            first_frame_gate_released: false,
        })
    }

    fn should_block_frame(&mut self) -> bool {
        if self.required_groups == 0 {
            return false;
        }
        if self.completed_required_groups < self.required_groups {
            self.blocked_frames = self.blocked_frames.saturating_add(1);
            return true;
        }
        if !self.first_frame_gate_released {
            self.first_frame_gate_released = true;
            self.blocked_frames = self.blocked_frames.saturating_add(1);
            return true;
        }
        false
    }

    fn required_complete(&self) -> bool {
        self.completed_required_groups >= self.required_groups
    }
}

struct WebGpuRenderer {
    _instance: wgpu::Instance,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipelines: Vec<RuntimeRenderPipeline>,
    frame_graph: Vec<FrameGraphRuntimePass>,
    render_manifest_schema: String,
    shader_bundle_schema: String,
    compute_pass_manifest_ready: bool,
    frame_graph_frames_executed: u64,
    frame_graph_last_render_passes: u32,
    frame_graph_last_compute_passes: u32,
    prewarm: RuntimePrewarmTracker,
    scene_buffers: RuntimeGpuSceneBufferContracts,
    default_profile_contracts: RuntimeDefaultProfileContracts,
    hiz_occlusion_tier_enabled: bool,
    indirect_submission_path_default: bool,
    bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
    indirect_draw_buffer: wgpu::Buffer,
    // 3D rendering resources
    pipeline_3d: Option<wgpu::RenderPipeline>,
    uniform_buffer_3d: wgpu::Buffer,
    model_uniform_buffer: wgpu::Buffer,
    joint_palette_buffer: wgpu::Buffer,
    bind_group_layout_3d_camera: wgpu::BindGroupLayout,
    bind_group_layout_3d_model: wgpu::BindGroupLayout,
    bind_group_layout_3d_material: wgpu::BindGroupLayout,
    bind_group_3d: wgpu::BindGroup,
    model_bind_group: wgpu::BindGroup,
    depth_texture: wgpu::Texture,
    depth_view: wgpu::TextureView,
    meshes: Vec<crate::mesh::GpuMesh>,
    /// Skeletons loaded from GLB files, indexed by mesh base index (the first
    /// mesh index returned by load_mesh_from_glb).
    skeletons: Vec<(usize, crate::skeletal_animation::Skeleton)>,
    /// Animation clips loaded from GLB files, indexed by mesh base index.
    animation_clips: Vec<(usize, Vec<crate::skeletal_animation::AnimationClip>)>,
    /// Elapsed animation time (seconds), advanced each frame.
    anim_elapsed_secs: f32,
    /// State-machine-driven animation blender for crossfading between clips
    /// based on game state changes.
    animation_blender: crate::animation_graph::AnimationBlender,
    materials: Vec<crate::material::GpuMaterial>,
    default_material_index: usize,
    material_sampler: wgpu::Sampler,
    default_albedo_texture: wgpu::Texture,
    default_normal_texture: wgpu::Texture,
    default_orm_texture: wgpu::Texture,
    surface_format: wgpu::TextureFormat,
    ssao_system: crate::ssao::SsaoSystem,
    shadow_system: crate::shadows::ShadowSystem,
    post_process: crate::postprocess::PostProcessStack,
    sky_pass: crate::sky::SkyPass,
    // Combat effects pass resources
    combat_fx_pipeline: wgpu::RenderPipeline,
    combat_fx_uniform_buffer: wgpu::Buffer,
    combat_fx_sampler: wgpu::Sampler,
    combat_fx_bgl: wgpu::BindGroupLayout,
    combat_fx_copy_texture: wgpu::Texture,
    combat_fx_copy_view: wgpu::TextureView,
    combat_fx_bind_group: wgpu::BindGroup,
}

impl WebGpuRenderer {
    async fn new(canvas: HtmlCanvasElement, shader: RuntimeShaderAssets) -> Result<Self, String> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::BROWSER_WEBGPU,
            ..Default::default()
        });

        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone()))
            .map_err(|error| format!("failed to create WebGPU surface: {error}"))?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .ok_or_else(|| "failed to acquire a WebGPU adapter".to_string())?;

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("wrela-client-device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: wgpu::MemoryHints::Performance,
                },
                None,
            )
            .await
            .map_err(|error| format!("failed to request WebGPU device: {error}"))?;

        let capabilities = surface.get_capabilities(&adapter);
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .or_else(|| capabilities.formats.first().copied())
            .ok_or_else(|| "surface exposes no texture formats".to_string())?;

        let present_mode = capabilities
            .present_modes
            .iter()
            .copied()
            .find(|mode| *mode == wgpu::PresentMode::Fifo)
            .unwrap_or(wgpu::PresentMode::AutoVsync);

        let alpha_mode = capabilities
            .alpha_modes
            .first()
            .copied()
            .unwrap_or(wgpu::CompositeAlphaMode::Opaque);

        let width = canvas.width().max(1);
        let height = canvas.height().max(1);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            format,
            width,
            height,
            present_mode,
            desired_maximum_frame_latency: 2,
            alpha_mode,
            view_formats: vec![],
        };
        surface.configure(&device, &config);

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("wrela-client-renderer-bind-group-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: Some(
                        std::num::NonZeroU64::new(std::mem::size_of::<FrameUniform>() as u64)
                            .expect("FrameUniform has non-zero size"),
                    ),
                },
                count: None,
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("wrela-client-renderer-pipeline-layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("wrela-client-renderer-frame-uniform"),
            size: std::mem::size_of::<FrameUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let indirect_draw_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("wrela-client-renderer-indirect-draw-buffer"),
            size: std::mem::size_of::<DrawIndirectRecord>() as u64,
            usage: wgpu::BufferUsages::INDIRECT | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("wrela-client-renderer-bind-group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let mut pipelines = Vec::with_capacity(shader.pipelines.len());
        let mut compiled_shader_modules = HashSet::<String>::new();
        for pipeline_asset in &shader.pipelines {
            let primitive_state = map_primitive_state_for_webgpu(
                pipeline_asset.primitive_topology,
                pipeline_asset.primitive_cull_mode,
            );
            device.push_error_scope(wgpu::ErrorFilter::Validation);
            let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("wrela-client-renderer-shader-module"),
                source: wgpu::ShaderSource::Wgsl(Cow::Owned(pipeline_asset.shader_source.clone())),
            });
            let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("wrela-client-renderer-pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader_module,
                    entry_point: Some(pipeline_asset.vertex_entry.as_str()),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState {
                    topology: primitive_state.topology,
                    cull_mode: primitive_state.cull_mode,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader_module,
                    entry_point: Some(pipeline_asset.fragment_entry.as_str()),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(wgpu::BlendState::REPLACE),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview: None,
                cache: None,
            });
            if let Some(error) = device.pop_error_scope().await {
                return Err(format!(
                    "failed to build WebGPU pipeline '{}' using module '{}' from '{}' (node_target='{}', shader_mode='{}'): {error}",
                    pipeline_asset.pipeline_id,
                    pipeline_asset.shader_module_id,
                    pipeline_asset.shader_path,
                    pipeline_asset.node_target.as_deref().unwrap_or("<none>"),
                    pipeline_asset.shader_mode.as_deref().unwrap_or("<none>")
                ));
            }
            pipelines.push(RuntimeRenderPipeline {
                id: pipeline_asset.pipeline_id.clone(),
                pipeline: render_pipeline,
            });
            compiled_shader_modules.insert(pipeline_asset.shader_module_id.clone());
        }
        if pipelines.is_empty() {
            return Err("render manifest resolved no pipelines for frame graph".to_string());
        }
        let prewarm = RuntimePrewarmTracker::from_assets(
            shader.prewarm_groups.as_slice(),
            &compiled_shader_modules,
        )?;

        let mut frame_graph = Vec::with_capacity(shader.frame_graph.len());
        let mut seen_passes = HashSet::<String>::new();
        for pass in &shader.frame_graph {
            if pass.reads.is_empty() && pass.writes.is_empty() {
                return Err(format!(
                    "frame graph pass '{}' has an empty pass contract '{}'",
                    pass.name, pass.pass_contract_id
                ));
            }
            let pipeline_index = pipelines
                .iter()
                .position(|pipeline| pipeline.id == pass.pipeline_id);
            if !pass.is_compute_pass && pipeline_index.is_none() {
                return Err(format!(
                    "frame graph pass '{}' references unknown pipeline '{}'",
                    pass.name, pass.pipeline_id
                ));
            }
            for dependency in &pass.depends_on {
                if !seen_passes.contains(dependency) {
                    return Err(format!(
                        "frame graph execution order violation for pass '{}': dependency '{}' has not executed",
                        pass.name, dependency
                    ));
                }
            }
            seen_passes.insert(pass.name.clone());
            frame_graph.push(FrameGraphRuntimePass {
                name: pass.name.clone(),
                draw_phase: pass.draw_phase.clone(),
                pipeline_index,
                pass_contract_id: pass.pass_contract_id.clone(),
                is_compute_pass: pass.is_compute_pass,
            });
        }
        if frame_graph.is_empty() {
            return Err("frame graph declared zero passes".to_string());
        }

        // ── 3D renderer resources ───────────────────────────────────────────
        let bind_group_layout_3d_camera =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("wrela-3d-camera-bind-group-layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: Some(
                            std::num::NonZeroU64::new(std::mem::size_of::<FrameUniform3D>() as u64)
                                .expect("FrameUniform3D has non-zero size"),
                        ),
                    },
                    count: None,
                }],
            });

        let bind_group_layout_3d_model =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("wrela-3d-model-bind-group-layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: Some(
                                std::num::NonZeroU64::new(MODEL_ENTRY_SIZE)
                                    .expect("MODEL_ENTRY_SIZE is non-zero"),
                            ),
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: Some(
                                std::num::NonZeroU64::new(64).expect("mat4x4 is 64 bytes"),
                            ),
                        },
                        count: None,
                    },
                ],
            });

        let uniform_buffer_3d = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("wrela-3d-camera-uniform"),
            size: std::mem::size_of::<FrameUniform3D>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let model_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("wrela-3d-model-storage"),
            size: MODEL_ENTRY_SIZE * MAX_3D_INSTANCES as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let joint_palette_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("wrela-3d-joint-palette"),
            size: JOINT_PALETTE_BUFFER_SIZE,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Initialize joint palette with identity matrices so non-skinned meshes
        // render correctly from the very first frame.
        {
            let identity_palette = crate::mesh::flatten_skinning_palette_for_upload(
                &[],
                crate::mesh::MAX_SKINNING_JOINTS,
            )
            .expect("identity palette creation should not fail");
            queue.write_buffer(
                &joint_palette_buffer,
                0,
                bytemuck::cast_slice(&identity_palette),
            );
        }

        let bind_group_3d = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("wrela-3d-camera-bind-group"),
            layout: &bind_group_layout_3d_camera,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer_3d.as_entire_binding(),
            }],
        });

        let model_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("wrela-3d-model-bind-group"),
            layout: &bind_group_layout_3d_model,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: model_uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: joint_palette_buffer.as_entire_binding(),
                },
            ],
        });

        let bind_group_layout_3d_material =
            crate::material::create_material_bind_group_layout(&device);

        let material_sampler = crate::material::create_material_sampler(&device);
        let (default_albedo_texture, default_normal_texture, default_orm_texture) =
            crate::material::create_default_textures(&device, &queue);

        let depth_texture = Self::create_depth_texture_static(&device, width, height);
        let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let shadow_system = crate::shadows::ShadowSystem::new(&device, &bind_group_layout_3d_model);

        let pipeline_layout_3d = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("wrela-3d-pipeline-layout"),
            bind_group_layouts: &[
                &bind_group_layout_3d_camera,
                &bind_group_layout_3d_model,
                &bind_group_layout_3d_material,
                &shadow_system.shadow_bind_group_layout,
            ],
            push_constant_ranges: &[],
        });

        let shader_module_3d = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("wrela-3d-pbr-shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(PBR_SHADER_3D)),
        });

        let vertex_buffer_layout = wgpu::VertexBufferLayout {
            array_stride: 88,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: 12,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: 24,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 32,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Uint16x4,
                },
                wgpu::VertexAttribute {
                    offset: 40,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Float32x4,
                },
                // vertex_color (wind weights)
                wgpu::VertexAttribute {
                    offset: 56,
                    shader_location: 5,
                    format: wgpu::VertexFormat::Float32x4,
                },
                // tangent
                wgpu::VertexAttribute {
                    offset: 72,
                    shader_location: 6,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        };

        let pipeline_3d = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("wrela-3d-pipeline"),
            layout: Some(&pipeline_layout_3d),
            vertex: wgpu::VertexState {
                module: &shader_module_3d,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[vertex_buffer_layout],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Greater,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader_module_3d,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba16Float,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
            cache: None,
        });

        let ground = generate_ground_plane();
        let cube = generate_procedural_cube();
        let gpu_ground = crate::mesh::upload_mesh(&device, &ground);
        let gpu_cube = crate::mesh::upload_mesh(&device, &cube);
        let meshes = vec![gpu_ground, gpu_cube];

        // Create default materials for each procedural mesh so material indices
        // stay in sync with mesh indices (materials[i] corresponds to meshes[i]).
        let mut materials = Vec::with_capacity(meshes.len());
        for _ in 0..meshes.len() {
            materials.push(crate::material::create_default_gpu_material(
                &device,
                &queue,
                &bind_group_layout_3d_material,
                &material_sampler,
            ));
        }
        let default_material_index = 0;

        let mut ssao_system = crate::ssao::SsaoSystem::new(
            &device,
            &queue,
            width,
            height,
            &depth_view,
            wgpu::TextureFormat::Rgba16Float,
        );
        ssao_system.set_radius(1.0);
        ssao_system.set_intensity(0.68);

        let mut post_process =
            crate::postprocess::PostProcessStack::new(&device, width, height, format, &depth_view);
        // Stylized gothic anime calibration: deeper contrast, tighter highlights,
        // and controlled shafts so sky values stay unclipped in combat framing.
        post_process.set_exposure(0.78);
        post_process.set_bloom_intensity(0.18);
        post_process.set_bloom_threshold(1.05);
        post_process.set_god_rays_intensity(0.2);

        let sky_pass = crate::sky::SkyPass::new(&device, wgpu::TextureFormat::Rgba16Float);

        // ── Combat effects pass ──────────────────────────────────────────
        let combat_fx_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("combat_fx_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let combat_fx_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("combat_fx_uniforms"),
            size: std::mem::size_of::<CombatFxUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let combat_fx_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("combat_fx_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let combat_fx_copy_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("combat_fx_scene_copy"),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let combat_fx_copy_view =
            combat_fx_copy_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let combat_fx_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("combat_fx_bind_group"),
            layout: &combat_fx_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&combat_fx_copy_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&combat_fx_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: combat_fx_uniform_buffer.as_entire_binding(),
                },
            ],
        });

        let combat_fx_shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("combat_fx_shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(COMBAT_FX_SHADER)),
        });
        let combat_fx_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("combat_fx_pipeline_layout"),
                bind_group_layouts: &[&combat_fx_bgl],
                push_constant_ranges: &[],
            });
        let combat_fx_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("combat_fx_pipeline"),
            layout: Some(&combat_fx_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &combat_fx_shader_module,
                entry_point: Some("vs"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &combat_fx_shader_module,
                entry_point: Some("fs"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Ok(Self {
            _instance: instance,
            surface,
            device,
            queue,
            config,
            pipelines,
            frame_graph,
            render_manifest_schema: shader.render_schema_version,
            shader_bundle_schema: shader.shader_bundle_schema_version,
            compute_pass_manifest_ready: shader.compute_pass_manifest_ready,
            frame_graph_frames_executed: 0,
            frame_graph_last_render_passes: 0,
            frame_graph_last_compute_passes: 0,
            prewarm,
            scene_buffers: shader.gpu_scene_buffers.clone(),
            default_profile_contracts: shader.default_profile_contracts.clone(),
            hiz_occlusion_tier_enabled: shader.gpu_scene_buffers.hiz_occlusion.enabled,
            indirect_submission_path_default: true,
            bind_group,
            uniform_buffer,
            indirect_draw_buffer,
            pipeline_3d: Some(pipeline_3d),
            uniform_buffer_3d,
            model_uniform_buffer,
            joint_palette_buffer,
            bind_group_layout_3d_camera,
            bind_group_layout_3d_model,
            bind_group_layout_3d_material,
            bind_group_3d,
            model_bind_group,
            depth_texture,
            depth_view,
            meshes,
            skeletons: Vec::new(),
            animation_clips: Vec::new(),
            anim_elapsed_secs: 0.0,
            animation_blender: {
                let mut blender = crate::animation_graph::AnimationBlender::new();
                // Default state mappings. Normal runtime bootstrap rewires these
                // from hero GLB clip names after clips are loaded.
                // 0=idle, 1=walk, 2=run, 3=dodge, 4=attack, 5=stagger, 6=parry, 7=recovery
                // If fewer clips exist, out-of-range targets are ignored by the
                // blender and it continues the current clip.
                blender.set_state_mapping(0, 0); // idle      -> clip 0 (idle)
                blender.set_state_mapping(1, 1); // walk      -> clip 1 (walk)
                blender.set_state_mapping(2, 1); // run       -> clip 1 (walk, reuse)
                blender.set_state_mapping(3, 4); // dodge     -> clip 4 (dodge)
                blender.set_state_mapping(4, 2); // attack    -> clip 2 (attack_light)
                blender.set_state_mapping(5, 6); // stagger   -> clip 6 (hit_stagger)
                blender.set_state_mapping(6, 5); // parry     -> clip 5 (parry)
                blender.set_state_mapping(7, 0); // recovery  -> clip 0 (idle, fallback)
                blender
            },
            materials,
            default_material_index,
            material_sampler,
            default_albedo_texture,
            default_normal_texture,
            default_orm_texture,
            surface_format: format,
            shadow_system,
            ssao_system,
            post_process,
            sky_pass,
            combat_fx_pipeline,
            combat_fx_uniform_buffer,
            combat_fx_sampler,
            combat_fx_bgl,
            combat_fx_copy_texture,
            combat_fx_copy_view,
            combat_fx_bind_group,
        })
    }

    fn create_depth_texture_static(
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("wrela-3d-depth-texture"),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
    }

    fn resize(&mut self, width: u32, height: u32) {
        let width = width.max(1);
        let height = height.max(1);
        if self.config.width == width && self.config.height == height {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        self.depth_texture = Self::create_depth_texture_static(&self.device, width, height);
        self.depth_view = self
            .depth_texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.ssao_system
            .resize(&self.device, width, height, &self.depth_view);
        self.post_process
            .resize(&self.device, width, height, self.surface_format, &self.depth_view);

        // Rebuild combat effects copy texture on resize
        self.combat_fx_copy_texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("combat_fx_scene_copy"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.surface_format,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        self.combat_fx_copy_view = self
            .combat_fx_copy_texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.combat_fx_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("combat_fx_bind_group"),
            layout: &self.combat_fx_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&self.combat_fx_copy_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.combat_fx_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.combat_fx_uniform_buffer.as_entire_binding(),
                },
            ],
        });
    }

    fn render_3d(
        &mut self,
        scene: &RenderSceneSnapshot3D<'_>,
        canvas_width: u32,
        canvas_height: u32,
    ) -> Result<RenderFrameResult, String> {
        use crate::camera_math::mat4_mul;

        let frame_cpu_start_ms = performance_now_ms();
        self.resize(canvas_width, canvas_height);

        if self.pipeline_3d.is_none() {
            return Err("3D pipeline not initialized".to_string());
        }

        // Unjittered view-proj for reprojection (TAA + shadows)
        let view_proj = mat4_mul(scene.camera_proj, scene.camera_view);

        // Apply TAA sub-pixel jitter directly to the existing projection matrix.
        // This preserves the reverse-Z depth convention and avoids reconstructing FOV/aspect.
        let jittered_view_proj = if self.post_process.taa_enabled() {
            let (jx, jy) = crate::camera_math::taa_jitter(self.post_process.taa_frame_index());
            let jitter_x = jx / canvas_width.max(1) as f32;
            let jitter_y = jy / canvas_height.max(1) as f32;
            let mut jittered_proj = scene.camera_proj;
            jittered_proj[2][0] += jitter_x * 2.0;
            jittered_proj[2][1] += jitter_y * 2.0;
            mat4_mul(jittered_proj, scene.camera_view)
        } else {
            view_proj
        };

        let frame_uniform = FrameUniform3D {
            view_proj: jittered_view_proj,
            camera_pos: [
                scene.camera_position[0],
                scene.camera_position[1],
                scene.camera_position[2],
                1.0,
            ],
            light_dir: [
                scene.light_direction[0],
                scene.light_direction[1],
                scene.light_direction[2],
                0.0,
            ],
            light_color: [
                scene.light_color[0],
                scene.light_color[1],
                scene.light_color[2],
                1.0,
            ],
            ambient: [
                scene.ambient_color[0],
                scene.ambient_color[1],
                scene.ambient_color[2],
                1.0,
            ],
            time: [self.anim_elapsed_secs, scene.delta_time_secs, 0.0, 0.0],
            // Gothic dusk fog tuned for depth separation and silhouette readability.
            fog_color_and_start: [0.2, 0.23, 0.26, 8.0], // xyz = cool dusk fog, w = fog start
            fog_params: [48.0, 0.055, 1.35, 0.0], // x = fog end, y = density, z = height falloff
            // Wind system parameters
            wind_params: [self.anim_elapsed_secs, 0.15, 1.0, 0.0], // x=time, y=strength, z=turbulence
            wind_dir: [-0.7071, 0.0, -0.7071, 0.0], // normalized [-0.7, 0, -0.7]
        };
        self.queue.write_buffer(
            &self.uniform_buffer_3d,
            0,
            bytemuck::bytes_of(&frame_uniform),
        );

        // ── Animation playback and joint palette upload ───────────────
        self.anim_elapsed_secs += scene.delta_time_secs;
        {
            // Drive the animation blender with the current player state so it
            // crossfades between clips when the game state changes.
            self.animation_blender
                .transition_to_state(scene.player_state);

            let mut palette_uploaded = false;
            for (skel_base, skel) in &self.skeletons {
                let clips = self
                    .animation_clips
                    .iter()
                    .find(|(base, _)| base == skel_base)
                    .map(|(_, clips)| clips.as_slice());
                if let Some(clips) = clips {
                    if !clips.is_empty() {
                        let joint_count = skel.joints.len();
                        let blended = self.animation_blender.advance(
                            scene.delta_time_secs,
                            clips,
                            joint_count,
                        );
                        let skinning_matrices =
                            crate::skeletal_animation::compute_skinning_matrices(
                                skel,
                                &blended.joint_poses,
                            );
                        if let Ok(flat) = crate::mesh::flatten_skinning_palette_for_upload(
                            &skinning_matrices,
                            crate::mesh::MAX_SKINNING_JOINTS,
                        ) {
                            self.queue.write_buffer(
                                &self.joint_palette_buffer,
                                0,
                                bytemuck::cast_slice(&flat),
                            );
                            palette_uploaded = true;
                        }
                    }
                }
            }
            let _ = palette_uploaded; // suppress unused-variable warning
        }

        // ── Shadow cascade update ──────────────────────────────────────
        self.shadow_system.update_cascades(
            &self.queue,
            scene.camera_view,
            scene.camera_proj,
            scene.light_direction,
            0.1,   // near plane (matches OrbitCamera)
            500.0, // far plane (matches OrbitCamera)
        );

        let Some(frame) = self.acquire_frame_with_retry()? else {
            return Ok(RenderFrameResult::SurfaceTimeout);
        };

        // Ensure depth texture matches the actual surface texture dimensions.
        // We compare directly against the depth texture size (not config) to
        // avoid oscillation when canvas CSS size differs from surface backing.
        let frame_w = frame.texture.width();
        let frame_h = frame.texture.height();
        if self.depth_texture.width() != frame_w || self.depth_texture.height() != frame_h {
            self.depth_texture = Self::create_depth_texture_static(&self.device, frame_w, frame_h);
            self.depth_view = self
                .depth_texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            self.ssao_system
                .resize(&self.device, frame_w, frame_h, &self.depth_view);
            self.post_process
                .resize(&self.device, frame_w, frame_h, self.surface_format, &self.depth_view);
        }

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("wrela-3d-render-encoder"),
            });

        // ── Shadow depth passes ────────────────────────────────────────
        {
            let shadow_instances: Vec<(usize, [[f32; 4]; 4])> = scene
                .mesh_instances
                .iter()
                .map(|inst| (inst.mesh_index, inst.model_matrix))
                .collect();
            self.shadow_system.encode_shadow_passes(
                &mut encoder,
                &self.queue,
                &self.meshes,
                &shadow_instances,
                &self.model_uniform_buffer,
                &self.model_bind_group,
                [self.anim_elapsed_secs, 0.15, 1.0, 0.0], // wind_params: time, strength, turbulence
                [-0.7071, 0.0, -0.7071, 0.0],              // wind_dir: normalized [-0.7, 0, -0.7]
            );
        }

        // ── Sky pass ─────────────────────────────────────────────────
        {
            let inv_vp = crate::camera_math::mat4_inverse(view_proj);
            // Note: sun_direction in sky shader points TOWARD the sun (negate light_direction)
            let sky_uniforms = crate::sky::SkyUniforms {
                inv_view_proj: inv_vp,
                sun_direction: [
                    -scene.light_direction[0],
                    -scene.light_direction[1],
                    -scene.light_direction[2],
                    0.0,
                ],
                // Cooler dusk sun with lower shaft intensity.
                sun_color: [1.8, 1.35, 0.9, 16.0],
                // Desaturated dusk zenith.
                sky_zenith: [0.025, 0.04, 0.075, 1.0],
                // Neutral horizon keeps contrast against trunks and characters.
                sky_horizon: [0.11, 0.12, 0.14, 1.0],
                // Dark forest floor bounce.
                sky_ground: [0.055, 0.05, 0.045, 1.0],
                // Standard Rayleigh scattering coefficients
                rayleigh_coeffs: [5.5e-6, 13.0e-6, 22.4e-6, 6360.0e3],
                // Mie: coefficient, anisotropy, atmosphere radius, sample count
                mie_params: [21.0e-6, 0.758, 6420.0e3, 16.0],
            };
            self.sky_pass.render(
                &mut encoder,
                &self.queue,
                self.post_process.hdr_target_view(),
                &sky_uniforms,
            );
        }

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("wrela-3d-render-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: self.post_process.hdr_target_view(),
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            // ── Build sorted instance list and upload storage buffer ────
            // Group instances by mesh_index so we can issue one instanced
            // draw call per unique mesh.  The storage buffer is written in
            // the order that matches the final instance_index seen by the
            // shader.
            let total_instances = scene.mesh_instances.len().min(MAX_3D_INSTANCES);

            // Collect valid instances, sorted by mesh_index for batching.
            let mut sorted_indices: Vec<usize> = (0..total_instances)
                .filter(|&i| scene.mesh_instances[i].mesh_index < self.meshes.len())
                .collect();
            sorted_indices.sort_by_key(|&i| scene.mesh_instances[i].mesh_index);

            // Upload model matrices into the storage buffer in sorted order.
            for (storage_slot, &orig_idx) in sorted_indices.iter().enumerate() {
                let instance = &scene.mesh_instances[orig_idx];
                let normal_model = mat4_inverse_transpose(instance.model_matrix);
                let model_entry = ModelUniform {
                    model: instance.model_matrix,
                    normal_model,
                };
                let offset = storage_slot as u64 * MODEL_ENTRY_SIZE;
                self.queue.write_buffer(
                    &self.model_uniform_buffer,
                    offset,
                    bytemuck::bytes_of(&model_entry),
                );
            }

            pass.set_pipeline(self.pipeline_3d.as_ref().unwrap());
            pass.set_bind_group(0, &self.bind_group_3d, &[]);
            pass.set_bind_group(1, &self.model_bind_group, &[]);
            pass.set_bind_group(3, &self.shadow_system.shadow_bind_group, &[]);

            // Issue one instanced draw call per unique mesh batch.
            let mut batch_start: usize = 0;
            while batch_start < sorted_indices.len() {
                let mesh_index = scene.mesh_instances[sorted_indices[batch_start]].mesh_index;
                let mut batch_end = batch_start + 1;
                while batch_end < sorted_indices.len()
                    && scene.mesh_instances[sorted_indices[batch_end]].mesh_index == mesh_index
                {
                    batch_end += 1;
                }

                let mesh = &self.meshes[mesh_index];
                let first_instance = batch_start as u32;
                let instance_count = (batch_end - batch_start) as u32;

                // Bind material group 2: use per-mesh material if available,
                // otherwise fall back to default material.
                let mat_index = if mesh_index < self.materials.len() {
                    mesh_index
                } else {
                    self.default_material_index
                };
                pass.set_bind_group(2, &self.materials[mat_index].bind_group, &[]);

                pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                pass.set_index_buffer(mesh.index_buffer.slice(..), mesh.index_format);
                pass.draw_indexed(
                    0..mesh.index_count,
                    0,
                    first_instance..first_instance + instance_count,
                );

                batch_start = batch_end;
            }
        }

        // ── SSAO pass ────────────────────────────────────────────────
        // SSAO composites onto the HDR texture so AO is included before
        // the bloom/tonemap chain runs.
        if self.ssao_system.enabled() {
            let inv_proj = crate::camera_math::mat4_inverse(scene.camera_proj);
            self.ssao_system.render(
                &mut encoder,
                &self.queue,
                self.post_process.hdr_texture(),
                self.post_process.hdr_target_view(),
                scene.camera_proj,
                inv_proj,
                scene.ambient_color,
            );
        }

        // ── Post-process: god rays + bloom + ACES tonemap + FXAA ──────
        // Compute sun screen position for god rays. The sun is at "infinity"
        // in the negative light direction, so we project a distant point.
        let sun_screen_pos = {
            let sun_dir = [
                -scene.light_direction[0],
                -scene.light_direction[1],
                -scene.light_direction[2],
            ];
            // Project a point far along the sun direction
            let sun_world = [
                scene.camera_position[0] + sun_dir[0] * 1000.0,
                scene.camera_position[1] + sun_dir[1] * 1000.0,
                scene.camera_position[2] + sun_dir[2] * 1000.0,
            ];
            let clip = crate::camera_math::mat4_transform_point(view_proj, sun_world);
            // clip.w > 0 means in front of camera
            if clip[3] > 0.0 {
                let ndc_x = clip[0] / clip[3];
                let ndc_y = clip[1] / clip[3];
                // Convert NDC [-1,1] to UV [0,1]
                let uv_x = ndc_x * 0.5 + 0.5;
                let uv_y = 1.0 - (ndc_y * 0.5 + 0.5); // flip Y for UV space
                Some([uv_x, uv_y])
            } else {
                None // sun behind camera
            }
        };
        // Reads from HDR render target, writes final tonemapped + AA'd
        // result to the sRGB surface view.
        self.post_process
            .render(&mut encoder, &self.queue, &view, sun_screen_pos, view_proj);

        // ── Combat effects pass ─────────────────────────────────────────
        let combat_fx_active = scene.hit_stop_active
            || scene.parry_flash_alpha > 0.001
            || scene.chromatic_aberration > 0.001;
        if combat_fx_active {
            let uniforms = CombatFxUniforms {
                vignette_intensity: scene.hit_stop_intensity,
                chromatic_aberration: scene.chromatic_aberration,
                flash_alpha: scene.parry_flash_alpha,
                _pad: 0.0,
            };
            self.queue.write_buffer(
                &self.combat_fx_uniform_buffer,
                0,
                bytemuck::bytes_of(&uniforms),
            );

            // Copy current frame so the shader can read it
            encoder.copy_texture_to_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &frame.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyTextureInfo {
                    texture: &self.combat_fx_copy_texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d {
                    width: self.config.width.max(1),
                    height: self.config.height.max(1),
                    depth_or_array_layers: 1,
                },
            );

            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("combat_fx_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.combat_fx_pipeline);
            pass.set_bind_group(0, &self.combat_fx_bind_group, &[]);
            pass.draw(0..3, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        frame.present();
        let frame_cpu_ms = (performance_now_ms() - frame_cpu_start_ms).max(0.0);

        let combat_fx_passes: u32 = if combat_fx_active { 1 } else { 0 };
        let ssao_passes: u32 = if self.ssao_system.enabled() { 4 } else { 0 };
        let shadow_passes: u32 = crate::shadows::CASCADE_COUNT as u32;
        // Post-process: 2 god rays + 6 bloom downsample + 5 bloom upsample + 1 tonemap + 1 FXAA = 15
        let god_ray_passes: u32 = if sun_screen_pos.is_some() { 2 } else { 0 };
        let taa_passes: u32 = if self.post_process.taa_enabled() { 1 } else { 0 };
        let postprocess_passes: u32 = god_ray_passes
            + taa_passes
            + (crate::postprocess::BLOOM_MIP_COUNT as u32)
            + (crate::postprocess::BLOOM_MIP_COUNT as u32 - 1)
            + 2; // tonemap + fxaa
        Ok(RenderFrameResult::Rendered(RenderExecutionStats {
            upload_bytes: std::mem::size_of::<FrameUniform3D>() as u64,
            render_passes: 1 + ssao_passes + shadow_passes + postprocess_passes + combat_fx_passes,
            compute_passes: 0,
            visibility: VisibilityStageTelemetry {
                candidate_draws: scene.mesh_instances.len() as u32,
                visible_draws: scene.mesh_instances.len() as u32,
                culled_ratio: 0.0,
                indirect_draw_count: 0,
                hiz_occlusion_tier_enabled: false,
                cpu_fallback_used: false,
                indirect_submission_path_default: false,
            },
            indirect_draw_count: 0,
            frame_cpu_ms,
            pass_timing_supported: false,
            pass_timing_fallback_used: true,
            pass_timings: vec![RuntimePassTimingSample {
                pass_name: "3d_pbr".to_string(),
                pass_kind: "render".to_string(),
                duration_ms: frame_cpu_ms,
                fallback_estimate: true,
            }],
        }))
    }

    fn build_uniform(
        &self,
        scene: &RenderSceneSnapshot,
        canvas_width: u32,
        canvas_height: u32,
    ) -> FrameUniform {
        let mut uniform = FrameUniform {
            canvas_world: [
                canvas_width.max(1) as f32,
                canvas_height.max(1) as f32,
                scene.world_width.max(1.0),
                scene.world_height.max(1.0),
            ],
            player: [
                scene.player_x,
                scene.player_y,
                PLAYER_HALF_SIZE,
                if scene.app_mode_is_website { 1.0 } else { 0.0 },
            ],
            ui: [
                scene.collectible_positions.len().min(MAX_COLLECTIBLES) as u32,
                scene.collected_mask,
                0,
                0,
            ],
            collectibles: [[0.0; 4]; MAX_COLLECTIBLES],
        };

        for (idx, (x, y)) in scene
            .collectible_positions
            .iter()
            .take(MAX_COLLECTIBLES)
            .enumerate()
        {
            uniform.collectibles[idx] = [*x, *y, 8.0, 0.0];
        }

        uniform
    }

    fn build_visibility_candidates(
        &self,
        scene: &RenderSceneSnapshot,
    ) -> Vec<SceneVisibilityCandidate> {
        let mut candidates =
            Vec::with_capacity(scene.collectible_positions.len().saturating_add(1));
        let bounds_extent = (self.scene_buffers.bounds.stride_bytes / 4).max(8) as f32;
        let collectible_extent = (bounds_extent * 0.25).max(8.0);
        let material_stride_slot = (self.scene_buffers.material_refs.stride_bytes / 16).max(1);
        candidates.push(SceneVisibilityCandidate {
            transform: GpuSceneTransformContract {
                translation: [scene.player_x, scene.player_y, 0.0],
                scale: [PLAYER_HALF_SIZE, PLAYER_HALF_SIZE, 1.0],
            },
            bounds: GpuSceneBoundsContract {
                center: [scene.player_x, scene.player_y, 0.0],
                extents: [
                    PLAYER_HALF_SIZE.max(bounds_extent),
                    PLAYER_HALF_SIZE.max(bounds_extent),
                    0.0,
                ],
            },
            draw_record: GpuSceneDrawRecordContract {
                transform_index: 0,
                bounds_index: 0,
                material_ref_index: 0,
                instance_count: 1,
            },
            material_ref: GpuSceneMaterialRefContract {
                material_slot: material_stride_slot.saturating_sub(1),
            },
        });

        for (index, (x, y)) in scene.collectible_positions.iter().enumerate() {
            if index < u32::BITS as usize {
                let mask = 1u32 << index;
                if scene.collected_mask & mask != 0 {
                    continue;
                }
            }
            let slot = index as u32 + 1;
            candidates.push(SceneVisibilityCandidate {
                transform: GpuSceneTransformContract {
                    translation: [*x, *y, 0.0],
                    scale: [8.0, 8.0, 1.0],
                },
                bounds: GpuSceneBoundsContract {
                    center: [*x, *y, 0.0],
                    extents: [collectible_extent, collectible_extent, 0.0],
                },
                draw_record: GpuSceneDrawRecordContract {
                    transform_index: slot,
                    bounds_index: slot,
                    material_ref_index: slot,
                    instance_count: 1,
                },
                material_ref: GpuSceneMaterialRefContract {
                    material_slot: slot.saturating_mul(material_stride_slot),
                },
            });
        }

        candidates
    }

    fn evaluate_visibility(&self, scene: &RenderSceneSnapshot) -> VisibilityStageTelemetry {
        let candidates = self.build_visibility_candidates(scene);
        let _stride_hint = self
            .scene_buffers
            .transforms
            .stride_bytes
            .saturating_add(self.scene_buffers.draw_records.stride_bytes);
        let mut telemetry = simulate_visibility_stage_telemetry(
            candidates.as_slice(),
            scene.world_width,
            scene.world_height,
            self.hiz_occlusion_tier_enabled,
        );
        telemetry.indirect_submission_path_default = self.indirect_submission_path_default;
        telemetry
    }

    fn acquire_frame_with_retry(&mut self) -> Result<Option<wgpu::SurfaceTexture>, String> {
        match self.surface.get_current_texture() {
            Ok(frame) => Ok(Some(frame)),
            Err(wgpu::SurfaceError::Timeout) => Ok(None),
            Err(wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost) => {
                self.surface.configure(&self.device, &self.config);
                match self.surface.get_current_texture() {
                    Ok(frame) => Ok(Some(frame)),
                    Err(wgpu::SurfaceError::Timeout) => Ok(None),
                    Err(error) => Err(format!(
                        "failed to acquire surface after reconfigure: {error}"
                    )),
                }
            }
            Err(wgpu::SurfaceError::OutOfMemory) => {
                Err("WebGPU surface is out of memory".to_string())
            }
            Err(wgpu::SurfaceError::Other) => {
                Err("WebGPU surface returned an unspecified error".to_string())
            }
        }
    }

    fn render(
        &mut self,
        scene: &RenderSceneSnapshot,
        canvas_width: u32,
        canvas_height: u32,
    ) -> Result<RenderFrameResult, String> {
        let frame_cpu_start_ms = performance_now_ms();
        self.resize(canvas_width, canvas_height);
        if self.prewarm.should_block_frame() {
            return Ok(RenderFrameResult::BlockedOnPrewarm);
        }

        let uniform = self.build_uniform(scene, canvas_width, canvas_height);
        let uniform_bytes = bytemuck::bytes_of(&uniform);
        self.queue
            .write_buffer(&self.uniform_buffer, 0, uniform_bytes);

        let mut visibility = self.evaluate_visibility(scene);
        let indirect_args = DrawIndirectRecord {
            vertex_count: 3,
            instance_count: visibility.visible_draws,
            first_vertex: 0,
            first_instance: 0,
        };
        self.queue.write_buffer(
            &self.indirect_draw_buffer,
            0,
            bytemuck::bytes_of(&indirect_args),
        );

        let Some(frame) = self.acquire_frame_with_retry()? else {
            return Ok(RenderFrameResult::SurfaceTimeout);
        };

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("wrela-client-render-encoder"),
            });

        let mut render_passes = 0u32;
        let mut compute_passes = 0u32;
        let mut indirect_draw_count = 0u32;
        let mut pass_timings = Vec::<RuntimePassTimingSample>::new();
        for pass_plan in &self.frame_graph {
            if pass_plan.is_compute_pass {
                let label = format!(
                    "wrela-client-compute-pass::{}::{}::{}",
                    pass_plan.name, pass_plan.draw_phase, pass_plan.pass_contract_id
                );
                let _pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some(label.as_str()),
                    timestamp_writes: None,
                });
                compute_passes = compute_passes.saturating_add(1);
                pass_timings.push(RuntimePassTimingSample {
                    pass_name: pass_plan.name.clone(),
                    pass_kind: "compute".to_string(),
                    duration_ms: 0.0,
                    fallback_estimate: true,
                });
                continue;
            }

            let Some(pipeline_index) = pass_plan.pipeline_index else {
                return Err(format!(
                    "frame graph pass '{}' has no render pipeline binding",
                    pass_plan.name
                ));
            };
            let Some(runtime_pipeline) = self.pipelines.get(pipeline_index) else {
                return Err(format!(
                    "frame graph pass '{}' references invalid pipeline index {}",
                    pass_plan.name, pipeline_index
                ));
            };

            let load_op = if render_passes == 0 {
                wgpu::LoadOp::Clear(wgpu::Color::BLACK)
            } else {
                wgpu::LoadOp::Load
            };
            let label = format!(
                "wrela-client-render-pass::{}::{}::{}",
                pass_plan.name, pass_plan.draw_phase, pass_plan.pass_contract_id
            );
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(label.as_str()),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: load_op,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });
            pass.set_pipeline(&runtime_pipeline.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            if self.indirect_submission_path_default {
                pass.draw_indirect(&self.indirect_draw_buffer, 0);
                if visibility.visible_draws > 0 {
                    indirect_draw_count = indirect_draw_count.saturating_add(1);
                }
            } else {
                visibility.cpu_fallback_used = true;
                pass.draw(0..3, 0..visibility.visible_draws.max(1));
            }
            render_passes = render_passes.saturating_add(1);
            pass_timings.push(RuntimePassTimingSample {
                pass_name: pass_plan.name.clone(),
                pass_kind: "render".to_string(),
                duration_ms: 0.0,
                fallback_estimate: true,
            });
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        frame.present();
        let frame_cpu_ms = (performance_now_ms() - frame_cpu_start_ms).max(0.0);
        self.frame_graph_frames_executed = self.frame_graph_frames_executed.saturating_add(1);
        self.frame_graph_last_render_passes = render_passes;
        self.frame_graph_last_compute_passes = compute_passes;
        visibility.indirect_draw_count = indirect_draw_count;
        if !pass_timings.is_empty() {
            let est_per_pass_ms = frame_cpu_ms / pass_timings.len() as f64;
            for sample in &mut pass_timings {
                sample.duration_ms = est_per_pass_ms;
            }
        }
        if pass_timings.len() > PASS_TIMING_SAMPLE_LIMIT {
            pass_timings.truncate(PASS_TIMING_SAMPLE_LIMIT);
        }

        Ok(RenderFrameResult::Rendered(RenderExecutionStats {
            upload_bytes: (uniform_bytes.len() + std::mem::size_of::<DrawIndirectRecord>()) as u64,
            render_passes,
            compute_passes,
            visibility,
            indirect_draw_count,
            frame_cpu_ms,
            pass_timing_supported: false,
            pass_timing_fallback_used: true,
            pass_timings,
        }))
    }

    fn runtime_evidence(&self) -> FrameGraphRuntimeEvidence {
        FrameGraphRuntimeEvidence {
            render_manifest_schema: self.render_manifest_schema.clone(),
            shader_bundle_schema: self.shader_bundle_schema.clone(),
            frame_graph_declared_passes: self.frame_graph.len() as u32,
            frame_graph_frames_executed: self.frame_graph_frames_executed,
            frame_graph_last_render_passes: self.frame_graph_last_render_passes,
            frame_graph_last_compute_passes: self.frame_graph_last_compute_passes,
            compute_pass_manifest_ready: self.compute_pass_manifest_ready,
            prewarm_required_groups: self.prewarm.required_groups,
            prewarm_completed_required_groups: self.prewarm.completed_required_groups,
            prewarm_required_complete: self.prewarm.required_complete(),
            prewarm_blocked_frames: self.prewarm.blocked_frames,
            frame_graph_path_used: true,
            visibility_candidate_draws: 0,
            visibility_visible_draws: 0,
            visibility_culled_ratio: 0.0,
            visibility_indirect_draw_count: 0,
            visibility_cpu_fallback_used: false,
            visibility_indirect_path_default: self.indirect_submission_path_default,
            hiz_occlusion_tier_enabled: self.hiz_occlusion_tier_enabled,
            lighting_contract_ready: self.default_profile_contracts.lighting.pbr_enabled
                && self.default_profile_contracts.lighting.hdr_enabled
                && self
                    .default_profile_contracts
                    .lighting
                    .clustered_lighting_enabled
                && self.default_profile_contracts.lighting.shadows_enabled,
            shadow_cascade_count: self.default_profile_contracts.lighting.shadow_cascade_count,
            shadow_atlas_resolution: self
                .default_profile_contracts
                .lighting
                .shadow_atlas_resolution,
            reflection_fallback_contract_ready: self
                .default_profile_contracts
                .reflections
                .fallback_chain
                == ["planar", "ssr", "probe"],
            ssr_max_steps: self.default_profile_contracts.reflections.ssr_max_steps,
            ssr_max_rays_per_pixel: self
                .default_profile_contracts
                .reflections
                .ssr_max_rays_per_pixel,
            probe_max_active_probes: self
                .default_profile_contracts
                .reflections
                .probe_max_active_probes,
            probe_update_ratio: self
                .default_profile_contracts
                .reflections
                .probe_update_ratio as f64,
            temporal_contract_ready: self
                .default_profile_contracts
                .temporal
                .motion_vectors_enabled
                && self.default_profile_contracts.temporal.taa_enabled
                && self
                    .default_profile_contracts
                    .temporal
                    .temporal_upscaling_enabled
                && self
                    .default_profile_contracts
                    .temporal
                    .reactive_mask_enabled
                && self
                    .default_profile_contracts
                    .temporal
                    .disocclusion_mask_enabled,
            dynamic_resolution_policy_enabled: self
                .default_profile_contracts
                .temporal
                .dynamic_resolution_policy
                .enabled,
            dynamic_resolution_min_scale: self
                .default_profile_contracts
                .temporal
                .dynamic_resolution_policy
                .min_scale as f64,
            dynamic_resolution_max_scale: self
                .default_profile_contracts
                .temporal
                .dynamic_resolution_policy
                .max_scale as f64,
            dynamic_resolution_target_frame_time_ms: self
                .default_profile_contracts
                .temporal
                .dynamic_resolution_policy
                .target_frame_time_ms as f64,
            temporal_metrics_window_frames: self
                .default_profile_contracts
                .temporal
                .metrics
                .window_frames,
            temporal_metrics_report_interval_ms: self
                .default_profile_contracts
                .temporal
                .metrics
                .report_interval_ms,
        }
    }
}

struct Runtime {
    app_mode: String,
    ready_status_line: String,
    dist_root: String,
    canvas: HtmlCanvasElement,
    renderer: Option<WebGpuRenderer>,
    ws: Option<WebSocket>,
    session_id: u64,
    partition_id: u64,
    actor_id: u64,
    seq: u64,
    ack: u64,
    pending_inputs: Vec<PendingInput>,
    local_tick: u64,
    runtime_tick_epoch_offset: u64,
    runtime_tick_monotonic: u64,
    runtime_tick_last_source: u64,
    correction_count: u64,
    last_forced_drift_tick: Option<u64>,
    last_sent_at_ms: f64,
    mmo_role: String,
    status: String,
    world_width: f32,
    world_height: f32,
    collectible_positions: Vec<(f32, f32)>,
    buttons: InputButtons,
    collect_pressed: bool,
    restart_button_latched: bool,
    game_won: bool,
    state: PredictedState,
    telemetry: RuntimeTelemetry,
    streaming: StreamingTelemetry,
    governor: RuntimeQualityGovernorState,
    asset_factory_generated_asset_count: u64,
    asset_factory_provenance_entry_count: u64,
    ui_atlas_count: u64,
    character_bundle_count: u64,
    asset_factory_contract_valid: bool,
    frame_graph: FrameGraphRuntimeEvidence,
    protocol_contract_valid: bool,
    protocol_message_type_count: u32,
    animation_authority: AnimationAuthorityState,
    animation_telemetry: RuntimeAnimationTelemetry,
    animation_graph_state: Option<crate::animation_graph::AnimationGraphState>,
    animation_event_lookup: HashMap<String, HashSet<String>>,
    hiz_occlusion_tier_override: Option<bool>,
    render_mode_3d: bool,
    orbit_camera: crate::camera_math::OrbitCamera,
    orbit_auto_rotate: bool,
    scene_3d_instances: Vec<MeshInstance>,
    player_instance_index: Option<usize>,
    enemy_instance_indices: Vec<usize>,
    scene_camera_anchor_count: usize,
    scene_default_camera_anchor_id: Option<String>,
    scene_combat_arena_extents: Option<RuntimeCombatArenaExtents>,
    scene_fog_volume_count: usize,
    scene_lut_profile_id: Option<String>,
    game_state: Option<crate::game_logic::GameState>,
    game_input: crate::game_logic::GameInput,
    combat_events: RuntimeCombatEventTelemetry,
    // Combat visual effect state (renderer-side tracking)
    parry_flash_alpha: f32,
    base_fov_y: f32,
    hud: Option<crate::hud::Hud>,
    hud_restart_pressed: bool,
    deterministic_time_driver_enabled: bool,
    deterministic_now_ms: f64,
    on_keydown: Option<Closure<dyn FnMut(KeyboardEvent)>>,
    on_keyup: Option<Closure<dyn FnMut(KeyboardEvent)>>,
    on_pointerdown: Option<Closure<dyn FnMut(Event)>>,
    on_pointerup: Option<Closure<dyn FnMut(Event)>>,
    on_ws_open: Option<Closure<dyn FnMut(Event)>>,
    on_ws_close: Option<Closure<dyn FnMut(Event)>>,
    on_ws_error: Option<Closure<dyn FnMut(Event)>>,
    on_ws_message: Option<Closure<dyn FnMut(MessageEvent)>>,
    on_render_game_to_text: Option<Closure<dyn FnMut() -> JsValue>>,
    on_advance_time: Option<Closure<dyn FnMut(f64)>>,
    raf_loop: Option<Rc<RefCell<Option<Closure<dyn FnMut(f64)>>>>>,
    // Preview mode state
    preview_state: Option<crate::preview_mode::PreviewState>,
    on_preview_mousedown: Option<Closure<dyn FnMut(web_sys::MouseEvent)>>,
    on_preview_mouseup: Option<Closure<dyn FnMut(web_sys::MouseEvent)>>,
    on_preview_mousemove: Option<Closure<dyn FnMut(web_sys::MouseEvent)>>,
    on_preview_wheel: Option<Closure<dyn FnMut(web_sys::WheelEvent)>>,
}

fn apply_predicted_state_delta(state: &mut PredictedState, delta: &StateDeltaPayload) {
    if let Some(tick) = delta.tick {
        state.tick = tick;
    }
    if let Some(player_x) = delta.player_x {
        state.player_x = player_x;
    }
    if let Some(player_y) = delta.player_y {
        state.player_y = player_y;
    }
    if let Some(score) = delta.score {
        state.score = score;
    }
    if let Some(collected_mask) = delta.collected_mask {
        state.collected_mask = collected_mask;
    }
}

impl Runtime {
    fn new(
        app_mode: String,
        ready_status_line: String,
        dist_root: String,
        mmo_role: String,
        hiz_occlusion_tier_override: Option<bool>,
        canvas: HtmlCanvasElement,
    ) -> Self {
        Self {
            app_mode,
            ready_status_line,
            dist_root,
            canvas,
            renderer: None,
            ws: None,
            session_id: 0,
            partition_id: 0,
            actor_id: 0,
            seq: 1,
            ack: 0,
            pending_inputs: Vec::new(),
            local_tick: 0,
            runtime_tick_epoch_offset: 0,
            runtime_tick_monotonic: 0,
            runtime_tick_last_source: 0,
            correction_count: 0,
            last_forced_drift_tick: None,
            last_sent_at_ms: 0.0,
            mmo_role: normalize_mmo_role(mmo_role.as_str()).to_string(),
            status: "booting".to_string(),
            world_width: DEFAULT_WORLD_WIDTH,
            world_height: DEFAULT_WORLD_HEIGHT,
            collectible_positions: Vec::new(),
            buttons: InputButtons::default(),
            collect_pressed: false,
            restart_button_latched: false,
            game_won: false,
            state: PredictedState::default(),
            telemetry: RuntimeTelemetry::default(),
            streaming: StreamingTelemetry::default(),
            governor: RuntimeQualityGovernorState::default(),
            asset_factory_generated_asset_count: 0,
            asset_factory_provenance_entry_count: 0,
            ui_atlas_count: 0,
            character_bundle_count: 0,
            asset_factory_contract_valid: false,
            frame_graph: FrameGraphRuntimeEvidence::default(),
            protocol_contract_valid: false,
            protocol_message_type_count: 0,
            animation_authority: AnimationAuthorityState::default(),
            animation_telemetry: RuntimeAnimationTelemetry::default(),
            animation_graph_state: None,
            animation_event_lookup: HashMap::new(),
            hiz_occlusion_tier_override,
            render_mode_3d: true,
            // Cinematic third-person camera: lower angle, closer follow for
            // cathedral forest feel. Default mode is Exploration (distance=8, elev=0.25).
            // Initial azimuth faces toward the rot stalker at (3, 0, 3).
            orbit_camera: {
                let mut cam = crate::camera_math::OrbitCamera::default();
                cam.azimuth = std::f32::consts::PI + std::f32::consts::FRAC_PI_4;
                cam.target = [0.0, 1.0, 0.0]; // slightly above ground for character center
                cam
            },
            orbit_auto_rotate: false,
            // Start with an empty scene; forest instances are populated in
            // bootstrap after GLB assets are loaded.
            scene_3d_instances: Vec::new(),
            player_instance_index: None,
            enemy_instance_indices: Vec::new(),
            scene_camera_anchor_count: 0,
            scene_default_camera_anchor_id: None,
            scene_combat_arena_extents: None,
            scene_fog_volume_count: 0,
            scene_lut_profile_id: None,
            game_state: Some(crate::game_logic::GameState::new()),
            game_input: crate::game_logic::GameInput::default(),
            combat_events: RuntimeCombatEventTelemetry::default(),
            parry_flash_alpha: 0.0,
            base_fov_y: std::f32::consts::FRAC_PI_4,
            hud: None,
            hud_restart_pressed: false,
            deterministic_time_driver_enabled: false,
            deterministic_now_ms: 0.0,
            on_keydown: None,
            on_keyup: None,
            on_pointerdown: None,
            on_pointerup: None,
            on_ws_open: None,
            on_ws_close: None,
            on_ws_error: None,
            on_ws_message: None,
            on_render_game_to_text: None,
            on_advance_time: None,
            raf_loop: None,
            preview_state: None,
            on_preview_mousedown: None,
            on_preview_mouseup: None,
            on_preview_mousemove: None,
            on_preview_wheel: None,
        }
    }

    fn set_status(&mut self, message: impl Into<String>) {
        self.status = message.into();
        web_sys::console::log_1(&JsValue::from_str(&format!("[wrela] {}", self.status)));
        self.publish_runtime_state();
    }

    fn emit_residency_adaptation_event(
        &mut self,
        now_ms: f64,
        reason: impl Into<String>,
        from_stage: RuntimeConvergenceStage,
        to_stage: RuntimeConvergenceStage,
        from_residency_class: RuntimeResidencyClass,
        to_residency_class: RuntimeResidencyClass,
    ) {
        let event = ResidencyAdaptationEvent {
            tick: self.state.tick,
            now_ms,
            reason: reason.into(),
            from_stage,
            to_stage,
            from_residency_class,
            to_residency_class,
            residency_pressure: self.streaming.residency_pressure,
        };
        web_sys::console::log_1(&JsValue::from_str(&format!(
            "[wrela] residency adaptation tick={} stage:{}->{} class:{}->{} pressure={:.3} reason={}",
            event.tick,
            event.from_stage.as_str(),
            event.to_stage.as_str(),
            event.from_residency_class.as_str(),
            event.to_residency_class.as_str(),
            event.residency_pressure,
            event.reason
        )));
        self.streaming.adaptation_events.push(event);
        if self.streaming.adaptation_events.len() > 64 {
            self.streaming.adaptation_events.remove(0);
        }
    }

    fn update_streaming_scheduler(&mut self, now_ms: f64) {
        let loaded_pressure = (self.streaming.loaded_chunk_count as f64 / 512.0).min(1.0);
        let frame_count = self.frame_graph.frame_graph_frames_executed.max(1) as f64;
        let prewarm_block_ratio =
            (self.frame_graph.prewarm_blocked_frames as f64 / frame_count).min(1.0);
        let stage_bias = match self.streaming.convergence_stage {
            RuntimeConvergenceStage::Bootstrap => 0.24,
            RuntimeConvergenceStage::Stream => 0.16,
            RuntimeConvergenceStage::Refine => 0.08,
            RuntimeConvergenceStage::Converged => 0.03,
        };
        self.streaming.residency_pressure =
            (loaded_pressure * 0.65 + prewarm_block_ratio * 0.25 + stage_bias).min(1.0);

        let previous_stage = self.streaming.convergence_stage;
        let previous_class = self.streaming.residency_class;
        let next_stage = infer_target_convergence_stage(
            previous_stage,
            &self.frame_graph,
            self.streaming.loaded_chunk_count,
        );
        let next_class =
            infer_target_residency_class(next_stage, self.streaming.residency_pressure);

        if next_stage != previous_stage || next_class != previous_class {
            let reason = if next_stage != previous_stage && next_class != previous_class {
                "stage+residency-transition"
            } else if next_stage != previous_stage {
                "convergence-stage-transition"
            } else {
                "residency-class-transition"
            };
            self.emit_residency_adaptation_event(
                now_ms,
                reason,
                previous_stage,
                next_stage,
                previous_class,
                next_class,
            );
            if next_stage.rank() > previous_stage.rank() {
                self.streaming.chunk_hit = self.streaming.chunk_hit.saturating_add(1);
            }
        }

        self.streaming.convergence_stage = next_stage;
        self.streaming.residency_class = next_class;
    }

    fn refresh_governor_contracts_from_frame_graph(&mut self) {
        let min_scale = self
            .frame_graph
            .dynamic_resolution_min_scale
            .clamp(0.25, 1.0);
        let max_scale = self
            .frame_graph
            .dynamic_resolution_max_scale
            .clamp(min_scale, 1.0);
        let target_frame_time_ms = self
            .frame_graph
            .dynamic_resolution_target_frame_time_ms
            .max(1.0);
        let shadow_max_tier = if self.frame_graph.shadow_atlas_resolution >= 2048
            && self.frame_graph.shadow_cascade_count >= 4
        {
            2
        } else if self.frame_graph.shadow_atlas_resolution >= 1024
            && self.frame_graph.shadow_cascade_count >= 2
        {
            1
        } else {
            0
        };
        let ssr_max_tier = if self.frame_graph.ssr_max_steps >= 48 {
            2
        } else if self.frame_graph.ssr_max_steps >= 24 {
            1
        } else {
            0
        };
        let probe_rate_max = self.frame_graph.probe_update_ratio.clamp(0.05, 1.0);
        let probe_rate_min = (probe_rate_max * 0.25).max(0.05);

        self.governor.bounds = RuntimeGovernorBounds {
            target_frame_time_ms,
            dynamic_resolution_min: min_scale,
            dynamic_resolution_max: max_scale,
            dynamic_resolution_step: 0.05,
            shadow_tier_min: 0,
            shadow_tier_max: shadow_max_tier,
            ssr_tier_min: 0,
            ssr_tier_max: ssr_max_tier,
            probe_rate_min,
            probe_rate_max,
            volumetric_steps_min: VOLUMETRIC_STEPS_MIN,
            volumetric_steps_max: VOLUMETRIC_STEPS_MAX,
        };
        if !self.governor.initialized_from_contracts {
            self.governor.initialized_from_contracts = true;
            self.governor.budgets = RuntimeGovernorBudgets {
                dynamic_resolution_scale: max_scale,
                shadow_quality_tier: shadow_max_tier,
                ssr_quality_tier: ssr_max_tier,
                probe_update_rate: probe_rate_max,
                volumetric_steps: VOLUMETRIC_STEPS_DEFAULT,
            };
            self.governor.calibration = RuntimeGovernorCalibration::default();
            return;
        }

        self.governor.budgets.dynamic_resolution_scale = self
            .governor
            .budgets
            .dynamic_resolution_scale
            .clamp(min_scale, max_scale);
        self.governor.budgets.shadow_quality_tier = self
            .governor
            .budgets
            .shadow_quality_tier
            .clamp(self.governor.bounds.shadow_tier_min, shadow_max_tier);
        self.governor.budgets.ssr_quality_tier = self
            .governor
            .budgets
            .ssr_quality_tier
            .clamp(self.governor.bounds.ssr_tier_min, ssr_max_tier);
        self.governor.budgets.probe_update_rate = self
            .governor
            .budgets
            .probe_update_rate
            .clamp(probe_rate_min, probe_rate_max);
        self.governor.budgets.volumetric_steps = self
            .governor
            .budgets
            .volumetric_steps
            .clamp(VOLUMETRIC_STEPS_MIN, VOLUMETRIC_STEPS_MAX);
    }

    fn push_governor_action_event(
        &mut self,
        now_ms: f64,
        action: impl Into<String>,
        reason: impl Into<String>,
        budget_delta_ms: f64,
        blocked_by_guardrail: bool,
        before: RuntimeGovernorBudgets,
        after: RuntimeGovernorBudgets,
    ) {
        let event = RuntimeGovernorActionEvent {
            tick: self.state.tick,
            frame_index: self.telemetry.frame_index,
            now_ms,
            action: action.into(),
            reason: reason.into(),
            budget_delta_ms,
            blocked_by_guardrail,
            before,
            after,
        };
        self.governor.actions.push(event);
        if self.governor.actions.len() > GOVERNOR_ACTION_TRACE_LIMIT {
            self.governor.actions.remove(0);
        }
    }

    fn observe_frame_budget(&mut self, frame_time_ms: f64) -> RuntimeFrameBudgetOutcome {
        let target_frame_time_ms = self.governor.bounds.target_frame_time_ms.max(1.0);
        let budget_delta_ms = frame_time_ms - target_frame_time_ms;
        let long_frame_threshold_ms = target_frame_time_ms * 1.20;
        let hitch_threshold_ms = target_frame_time_ms * 2.0;
        let within_budget = budget_delta_ms <= 0.0;
        let long_frame = frame_time_ms > long_frame_threshold_ms;
        let hitch = frame_time_ms > hitch_threshold_ms;
        self.telemetry.frame_index = self.telemetry.frame_index.saturating_add(1);

        if within_budget {
            self.telemetry.budget_counters.within_budget_frames = self
                .telemetry
                .budget_counters
                .within_budget_frames
                .saturating_add(1);
        } else {
            self.telemetry.budget_counters.over_budget_frames = self
                .telemetry
                .budget_counters
                .over_budget_frames
                .saturating_add(1);
        }
        if long_frame {
            self.telemetry.budget_counters.long_frame_count = self
                .telemetry
                .budget_counters
                .long_frame_count
                .saturating_add(1);
            self.telemetry.budget_counters.current_long_frame_streak = self
                .telemetry
                .budget_counters
                .current_long_frame_streak
                .saturating_add(1);
        } else {
            self.telemetry.budget_counters.current_long_frame_streak = 0;
        }
        if hitch {
            self.telemetry.budget_counters.hitch_count =
                self.telemetry.budget_counters.hitch_count.saturating_add(1);
            self.telemetry.budget_counters.current_hitch_streak = self
                .telemetry
                .budget_counters
                .current_hitch_streak
                .saturating_add(1);
        } else {
            self.telemetry.budget_counters.current_hitch_streak = 0;
        }
        self.telemetry.budget_counters.max_consecutive_long_frames = self
            .telemetry
            .budget_counters
            .max_consecutive_long_frames
            .max(self.telemetry.budget_counters.current_long_frame_streak);
        self.telemetry.budget_counters.max_consecutive_hitches = self
            .telemetry
            .budget_counters
            .max_consecutive_hitches
            .max(self.telemetry.budget_counters.current_hitch_streak);

        let outcome = RuntimeFrameBudgetOutcome {
            frame_index: self.telemetry.frame_index,
            frame_time_ms,
            target_frame_time_ms,
            budget_delta_ms,
            within_budget,
            long_frame,
            hitch,
        };
        self.telemetry.budget_outcomes.push(outcome.clone());
        if self.telemetry.budget_outcomes.len() > RUNTIME_BUDGET_HISTORY_LIMIT {
            self.telemetry.budget_outcomes.remove(0);
        }
        self.telemetry.last_budget_outcome = Some(outcome.clone());
        outcome
    }

    fn apply_quality_governor(&mut self, now_ms: f64, outcome: &RuntimeFrameBudgetOutcome) {
        self.refresh_governor_contracts_from_frame_graph();
        if self.governor.calibration.active && !self.governor.calibration.complete {
            self.governor.calibration.sampled_frames =
                self.governor.calibration.sampled_frames.saturating_add(1);
            let sampled = self
                .telemetry
                .frame_times_ms
                .iter()
                .rev()
                .take(self.governor.calibration.sample_target_frames as usize)
                .copied()
                .collect::<Vec<_>>();
            if !sampled.is_empty() {
                let total: f64 = sampled.iter().sum();
                self.governor.calibration.startup_average_frame_ms = total / sampled.len() as f64;
                self.governor.calibration.startup_p95_frame_ms = percentile(&sampled, 95.0);
            }
            if self.governor.calibration.sampled_frames
                >= self.governor.calibration.sample_target_frames
            {
                self.governor.calibration.active = false;
                self.governor.calibration.complete = true;
                let bounded_target = self.governor.calibration.startup_average_frame_ms.clamp(
                    self.governor.bounds.target_frame_time_ms * 0.9,
                    self.governor.bounds.target_frame_time_ms * 1.15,
                );
                self.governor.bounds.target_frame_time_ms = bounded_target;
                let baseline = self.governor.budgets;
                self.push_governor_action_event(
                    now_ms,
                    "startup_calibration_complete",
                    "bounded-startup-calibration",
                    outcome.budget_delta_ms,
                    false,
                    baseline,
                    baseline,
                );
            }
            return;
        }

        if self.governor.adaptation_cooldown_frames > 0 {
            self.governor.adaptation_cooldown_frames -= 1;
        }
        if outcome.within_budget {
            self.governor.within_budget_streak =
                self.governor.within_budget_streak.saturating_add(1);
            self.governor.over_budget_streak = 0;
        } else {
            self.governor.over_budget_streak = self.governor.over_budget_streak.saturating_add(1);
            self.governor.within_budget_streak = 0;
        }

        if outcome.hitch && self.governor.over_budget_streak >= 3 {
            let baseline = self.governor.budgets;
            self.governor.blocked_stability_disable_attempts = self
                .governor
                .blocked_stability_disable_attempts
                .saturating_add(1);
            self.push_governor_action_event(
                now_ms,
                "disable_stability_passes",
                "guardrail-blocked-critical-stability-disable",
                outcome.budget_delta_ms,
                true,
                baseline,
                baseline,
            );
        }

        if self.governor.adaptation_cooldown_frames > 0 {
            return;
        }

        let before = self.governor.budgets;
        let mut action = None::<&str>;
        if !outcome.within_budget && self.governor.over_budget_streak >= 2 {
            if self.governor.budgets.dynamic_resolution_scale
                > self.governor.bounds.dynamic_resolution_min + 0.001
            {
                self.governor.budgets.dynamic_resolution_scale =
                    (self.governor.budgets.dynamic_resolution_scale
                        - self.governor.bounds.dynamic_resolution_step)
                        .max(self.governor.bounds.dynamic_resolution_min);
                action = Some("decrease_dynamic_resolution");
            } else if self.governor.budgets.volumetric_steps
                > self.governor.bounds.volumetric_steps_min
            {
                self.governor.budgets.volumetric_steps = self
                    .governor
                    .budgets
                    .volumetric_steps
                    .saturating_sub(8)
                    .max(self.governor.bounds.volumetric_steps_min);
                action = Some("decrease_volumetric_steps");
            } else if self.governor.budgets.ssr_quality_tier > self.governor.bounds.ssr_tier_min {
                self.governor.budgets.ssr_quality_tier -= 1;
                action = Some("decrease_ssr_quality");
            } else if self.governor.budgets.probe_update_rate
                > self.governor.bounds.probe_rate_min + 0.001
            {
                self.governor.budgets.probe_update_rate = (self.governor.budgets.probe_update_rate
                    - 0.05)
                    .max(self.governor.bounds.probe_rate_min);
                action = Some("decrease_probe_rate");
            } else if self.governor.budgets.shadow_quality_tier
                > self.governor.bounds.shadow_tier_min
            {
                self.governor.budgets.shadow_quality_tier -= 1;
                action = Some("decrease_shadow_quality");
            }
        } else if outcome.within_budget && self.governor.within_budget_streak >= 45 {
            if self.governor.budgets.shadow_quality_tier < self.governor.bounds.shadow_tier_max {
                self.governor.budgets.shadow_quality_tier += 1;
                action = Some("increase_shadow_quality");
            } else if self.governor.budgets.probe_update_rate
                < self.governor.bounds.probe_rate_max - 0.001
            {
                self.governor.budgets.probe_update_rate = (self.governor.budgets.probe_update_rate
                    + 0.05)
                    .min(self.governor.bounds.probe_rate_max);
                action = Some("increase_probe_rate");
            } else if self.governor.budgets.ssr_quality_tier < self.governor.bounds.ssr_tier_max {
                self.governor.budgets.ssr_quality_tier += 1;
                action = Some("increase_ssr_quality");
            } else if self.governor.budgets.volumetric_steps
                < self.governor.bounds.volumetric_steps_max
            {
                self.governor.budgets.volumetric_steps = self
                    .governor
                    .budgets
                    .volumetric_steps
                    .saturating_add(8)
                    .min(self.governor.bounds.volumetric_steps_max);
                action = Some("increase_volumetric_steps");
            } else if self.governor.budgets.dynamic_resolution_scale
                < self.governor.bounds.dynamic_resolution_max - 0.001
            {
                self.governor.budgets.dynamic_resolution_scale =
                    (self.governor.budgets.dynamic_resolution_scale
                        + self.governor.bounds.dynamic_resolution_step)
                        .min(self.governor.bounds.dynamic_resolution_max);
                action = Some("increase_dynamic_resolution");
            }
        }

        if let Some(action_name) = action {
            self.governor.adaptation_count = self.governor.adaptation_count.saturating_add(1);
            self.governor.adaptation_cooldown_frames = 12;
            self.push_governor_action_event(
                now_ms,
                action_name,
                if outcome.within_budget {
                    "sustained-under-budget"
                } else {
                    "over-budget-correction"
                },
                outcome.budget_delta_ms,
                false,
                before,
                self.governor.budgets,
            );
        }
    }

    fn sync_canvas_size(&mut self) -> bool {
        let Some(window) = web_sys::window() else {
            return false;
        };
        let Ok(width) = window.inner_width() else {
            return false;
        };
        let Ok(height) = window.inner_height() else {
            return false;
        };
        let width_px = width.as_f64().unwrap_or(800.0).max(1.0) as u32;
        let height_px = height.as_f64().unwrap_or(600.0).max(1.0) as u32;

        let mut resized = false;
        if self.canvas.width() != width_px {
            self.canvas.set_width(width_px);
            resized = true;
        }
        if self.canvas.height() != height_px {
            self.canvas.set_height(height_px);
            resized = true;
        }
        resized
    }

    fn update_runtime_tick_monotonic(&mut self) {
        if self.state.tick < self.runtime_tick_last_source {
            let next_monotonic = self.runtime_tick_monotonic.saturating_add(1);
            self.runtime_tick_epoch_offset = next_monotonic.saturating_sub(self.state.tick);
        }
        self.runtime_tick_last_source = self.state.tick;
        self.runtime_tick_monotonic = self
            .runtime_tick_epoch_offset
            .saturating_add(self.state.tick);
    }

    fn publish_runtime_state(&mut self) {
        self.update_runtime_tick_monotonic();
        let runtime = Object::new();
        object_set(
            &runtime,
            "session",
            JsValue::from_str(&self.session_id.to_string()),
        );
        object_set(
            &runtime,
            "partition",
            JsValue::from_f64(self.partition_id as f64),
        );
        object_set(&runtime, "actor", JsValue::from_f64(self.actor_id as f64));
        object_set(
            &runtime,
            "score",
            JsValue::from_f64(f64::from(self.state.score)),
        );
        object_set(&runtime, "ack", JsValue::from_f64(self.ack as f64));
        object_set(
            &runtime,
            "corrections",
            JsValue::from_f64(self.correction_count as f64),
        );
        object_set(
            &runtime,
            "tick",
            JsValue::from_f64(self.runtime_tick_monotonic as f64),
        );
        object_set(
            &runtime,
            "state_tick",
            JsValue::from_f64(self.state.tick as f64),
        );
        object_set(
            &runtime,
            "pending",
            JsValue::from_f64(self.pending_inputs.len() as f64),
        );
        object_set(
            &runtime,
            "hash",
            JsValue::from_str(&format!("0x{:x}", self.hash_state())),
        );
        let divergence = self
            .last_forced_drift_tick
            .map(|tick| format!("tick {tick}"))
            .unwrap_or_else(|| "none".to_string());
        object_set(&runtime, "divergence", JsValue::from_str(&divergence));
        object_set(&runtime, "role", JsValue::from_str(self.mmo_role.as_str()));
        object_set(&runtime, "status", JsValue::from_str(self.status.as_str()));
        object_set(
            &runtime,
            "app_mode",
            JsValue::from_str(self.app_mode.as_str()),
        );
        object_set(
            &runtime,
            "dist_root",
            JsValue::from_str(self.dist_root.as_str()),
        );
        object_set(&runtime, "won", JsValue::from_bool(self.game_won));
        object_set(
            &runtime,
            "deterministic_time_driver_enabled",
            JsValue::from_bool(self.deterministic_time_driver_enabled),
        );
        object_set(
            &runtime,
            "frame_graph_path_used",
            JsValue::from_bool(self.frame_graph.frame_graph_path_used),
        );
        object_set(
            &runtime,
            "frame_graph_declared_passes",
            JsValue::from_f64(self.frame_graph.frame_graph_declared_passes as f64),
        );
        object_set(
            &runtime,
            "frame_graph_frames_executed",
            JsValue::from_f64(self.frame_graph.frame_graph_frames_executed as f64),
        );
        object_set(
            &runtime,
            "frame_graph_last_render_passes",
            JsValue::from_f64(self.frame_graph.frame_graph_last_render_passes as f64),
        );
        object_set(
            &runtime,
            "frame_graph_last_compute_passes",
            JsValue::from_f64(self.frame_graph.frame_graph_last_compute_passes as f64),
        );
        object_set(
            &runtime,
            "compute_pass_manifest_ready",
            JsValue::from_bool(self.frame_graph.compute_pass_manifest_ready),
        );
        object_set(
            &runtime,
            "prewarm_required_groups",
            JsValue::from_f64(self.frame_graph.prewarm_required_groups as f64),
        );
        object_set(
            &runtime,
            "prewarm_completed_required_groups",
            JsValue::from_f64(self.frame_graph.prewarm_completed_required_groups as f64),
        );
        object_set(
            &runtime,
            "prewarm_required_complete",
            JsValue::from_bool(self.frame_graph.prewarm_required_complete),
        );
        object_set(
            &runtime,
            "prewarm_blocked_frames",
            JsValue::from_f64(self.frame_graph.prewarm_blocked_frames as f64),
        );
        object_set(
            &runtime,
            "visibility_candidate_draws",
            JsValue::from_f64(self.frame_graph.visibility_candidate_draws as f64),
        );
        object_set(
            &runtime,
            "visibility_visible_draws",
            JsValue::from_f64(self.frame_graph.visibility_visible_draws as f64),
        );
        object_set(
            &runtime,
            "visibility_culled_ratio",
            JsValue::from_f64(round3(self.frame_graph.visibility_culled_ratio)),
        );
        object_set(
            &runtime,
            "visibility_indirect_draw_count",
            JsValue::from_f64(self.frame_graph.visibility_indirect_draw_count as f64),
        );
        object_set(
            &runtime,
            "visibility_cpu_fallback_used",
            JsValue::from_bool(self.frame_graph.visibility_cpu_fallback_used),
        );
        object_set(
            &runtime,
            "visibility_indirect_path_default",
            JsValue::from_bool(self.frame_graph.visibility_indirect_path_default),
        );
        object_set(
            &runtime,
            "hiz_occlusion_tier_enabled",
            JsValue::from_bool(self.frame_graph.hiz_occlusion_tier_enabled),
        );
        object_set(
            &runtime,
            "lighting_contract_ready",
            JsValue::from_bool(self.frame_graph.lighting_contract_ready),
        );
        object_set(
            &runtime,
            "shadow_cascade_count",
            JsValue::from_f64(self.frame_graph.shadow_cascade_count as f64),
        );
        object_set(
            &runtime,
            "shadow_atlas_resolution",
            JsValue::from_f64(self.frame_graph.shadow_atlas_resolution as f64),
        );
        object_set(
            &runtime,
            "reflection_fallback_contract_ready",
            JsValue::from_bool(self.frame_graph.reflection_fallback_contract_ready),
        );
        object_set(
            &runtime,
            "ssr_max_steps",
            JsValue::from_f64(self.frame_graph.ssr_max_steps as f64),
        );
        object_set(
            &runtime,
            "ssr_max_rays_per_pixel",
            JsValue::from_f64(self.frame_graph.ssr_max_rays_per_pixel as f64),
        );
        object_set(
            &runtime,
            "probe_max_active_probes",
            JsValue::from_f64(self.frame_graph.probe_max_active_probes as f64),
        );
        object_set(
            &runtime,
            "probe_update_ratio",
            JsValue::from_f64(round3(self.frame_graph.probe_update_ratio)),
        );
        object_set(
            &runtime,
            "temporal_contract_ready",
            JsValue::from_bool(self.frame_graph.temporal_contract_ready),
        );
        object_set(
            &runtime,
            "dynamic_resolution_policy_enabled",
            JsValue::from_bool(self.frame_graph.dynamic_resolution_policy_enabled),
        );
        object_set(
            &runtime,
            "dynamic_resolution_min_scale",
            JsValue::from_f64(round3(self.frame_graph.dynamic_resolution_min_scale)),
        );
        object_set(
            &runtime,
            "dynamic_resolution_max_scale",
            JsValue::from_f64(round3(self.frame_graph.dynamic_resolution_max_scale)),
        );
        object_set(
            &runtime,
            "dynamic_resolution_target_frame_time_ms",
            JsValue::from_f64(round3(
                self.frame_graph.dynamic_resolution_target_frame_time_ms,
            )),
        );
        object_set(
            &runtime,
            "temporal_metrics_window_frames",
            JsValue::from_f64(self.frame_graph.temporal_metrics_window_frames as f64),
        );
        object_set(
            &runtime,
            "temporal_metrics_report_interval_ms",
            JsValue::from_f64(self.frame_graph.temporal_metrics_report_interval_ms as f64),
        );
        object_set(
            &runtime,
            "compile_stall_count",
            JsValue::from_f64(self.telemetry.compile_stall_count as f64),
        );
        object_set(
            &runtime,
            "render_manifest_schema",
            JsValue::from_str(self.frame_graph.render_manifest_schema.as_str()),
        );
        object_set(
            &runtime,
            "shader_bundle_schema",
            JsValue::from_str(self.frame_graph.shader_bundle_schema.as_str()),
        );
        object_set(
            &runtime,
            "protocol_contract_valid",
            JsValue::from_bool(self.protocol_contract_valid),
        );
        object_set(
            &runtime,
            "protocol_message_type_count",
            JsValue::from_f64(self.protocol_message_type_count as f64),
        );
        object_set(
            &runtime,
            "asset_factory_contract_valid",
            JsValue::from_bool(self.asset_factory_contract_valid),
        );
        object_set(
            &runtime,
            "asset_factory_generated_asset_count",
            JsValue::from_f64(self.asset_factory_generated_asset_count as f64),
        );
        object_set(
            &runtime,
            "asset_factory_provenance_entry_count",
            JsValue::from_f64(self.asset_factory_provenance_entry_count as f64),
        );
        object_set(
            &runtime,
            "ui_atlas_count",
            JsValue::from_f64(self.ui_atlas_count as f64),
        );
        object_set(
            &runtime,
            "character_bundle_count",
            JsValue::from_f64(self.character_bundle_count as f64),
        );
        object_set(
            &runtime,
            "streaming_convergence_stage",
            JsValue::from_str(self.streaming.convergence_stage.as_str()),
        );
        object_set(
            &runtime,
            "streaming_residency_class",
            JsValue::from_str(self.streaming.residency_class.as_str()),
        );
        object_set(
            &runtime,
            "streaming_adaptation_event_count",
            JsValue::from_f64(self.streaming.adaptation_events.len() as f64),
        );
        object_set(
            &runtime,
            "governor_adaptation_count",
            JsValue::from_f64(self.governor.adaptation_count as f64),
        );
        object_set(
            &runtime,
            "governor_blocked_stability_disable_attempts",
            JsValue::from_f64(self.governor.blocked_stability_disable_attempts as f64),
        );
        set_global_object("__wrelaRuntime", &runtime);
    }

    fn publish_metrics(&self) {
        let frame_times: Vec<f64> = self.telemetry.frame_times_ms.iter().copied().collect();
        let p50 = percentile(&frame_times, 50.0);
        let p95 = percentile(&frame_times, 95.0);

        let gpu = Object::new();
        object_set(&gpu, "frame_time_p50_ms", JsValue::from_f64(round3(p50)));
        object_set(&gpu, "frame_time_p95_ms", JsValue::from_f64(round3(p95)));
        object_set(
            &gpu,
            "draw_calls",
            JsValue::from_f64(self.telemetry.draw_calls as f64),
        );
        object_set(
            &gpu,
            "compute_passes",
            JsValue::from_f64(self.telemetry.compute_passes as f64),
        );
        object_set(
            &gpu,
            "gpu_upload_bytes",
            JsValue::from_f64(self.telemetry.gpu_upload_bytes as f64),
        );
        object_set(
            &gpu,
            "compile_stall_count",
            JsValue::from_f64(self.telemetry.compile_stall_count as f64),
        );
        object_set(
            &gpu,
            "prewarm_blocked_frames",
            JsValue::from_f64(self.telemetry.prewarm_blocked_frames as f64),
        );
        object_set(
            &gpu,
            "prewarm_required_groups",
            JsValue::from_f64(self.frame_graph.prewarm_required_groups as f64),
        );
        object_set(
            &gpu,
            "prewarm_completed_required_groups",
            JsValue::from_f64(self.frame_graph.prewarm_completed_required_groups as f64),
        );
        object_set(
            &gpu,
            "prewarm_required_complete",
            JsValue::from_bool(self.frame_graph.prewarm_required_complete),
        );
        object_set(
            &gpu,
            "visibility_candidate_draws",
            JsValue::from_f64(self.frame_graph.visibility_candidate_draws as f64),
        );
        object_set(
            &gpu,
            "visibility_visible_draws",
            JsValue::from_f64(self.frame_graph.visibility_visible_draws as f64),
        );
        object_set(
            &gpu,
            "visibility_culled_ratio",
            JsValue::from_f64(round3(self.frame_graph.visibility_culled_ratio)),
        );
        object_set(
            &gpu,
            "visibility_indirect_draw_count",
            JsValue::from_f64(self.frame_graph.visibility_indirect_draw_count as f64),
        );
        object_set(
            &gpu,
            "visibility_candidate_draws_total",
            JsValue::from_f64(self.telemetry.visibility_candidate_draws as f64),
        );
        object_set(
            &gpu,
            "visibility_visible_draws_total",
            JsValue::from_f64(self.telemetry.visibility_visible_draws as f64),
        );
        object_set(
            &gpu,
            "visibility_indirect_draw_count_total",
            JsValue::from_f64(self.telemetry.visibility_indirect_draw_count as f64),
        );
        object_set(
            &gpu,
            "visibility_cpu_fallback_used",
            JsValue::from_bool(self.frame_graph.visibility_cpu_fallback_used),
        );
        object_set(
            &gpu,
            "visibility_indirect_path_default",
            JsValue::from_bool(self.frame_graph.visibility_indirect_path_default),
        );
        object_set(
            &gpu,
            "hiz_occlusion_tier_enabled",
            JsValue::from_bool(self.frame_graph.hiz_occlusion_tier_enabled),
        );
        object_set(
            &gpu,
            "lighting_contract_ready",
            JsValue::from_bool(self.frame_graph.lighting_contract_ready),
        );
        object_set(
            &gpu,
            "reflection_fallback_contract_ready",
            JsValue::from_bool(self.frame_graph.reflection_fallback_contract_ready),
        );
        object_set(
            &gpu,
            "temporal_contract_ready",
            JsValue::from_bool(self.frame_graph.temporal_contract_ready),
        );
        object_set(
            &gpu,
            "dynamic_resolution_policy_enabled",
            JsValue::from_bool(self.frame_graph.dynamic_resolution_policy_enabled),
        );
        object_set(
            &gpu,
            "dynamic_resolution_min_scale",
            JsValue::from_f64(round3(self.frame_graph.dynamic_resolution_min_scale)),
        );
        object_set(
            &gpu,
            "dynamic_resolution_max_scale",
            JsValue::from_f64(round3(self.frame_graph.dynamic_resolution_max_scale)),
        );
        object_set(
            &gpu,
            "dynamic_resolution_target_frame_time_ms",
            JsValue::from_f64(round3(
                self.frame_graph.dynamic_resolution_target_frame_time_ms,
            )),
        );
        object_set(
            &gpu,
            "temporal_metrics_window_frames",
            JsValue::from_f64(self.frame_graph.temporal_metrics_window_frames as f64),
        );
        object_set(
            &gpu,
            "temporal_metrics_report_interval_ms",
            JsValue::from_f64(self.frame_graph.temporal_metrics_report_interval_ms as f64),
        );
        object_set(
            &gpu,
            "visibility_culled_ratio_last",
            JsValue::from_f64(round3(self.telemetry.visibility_culled_ratio)),
        );
        object_set(
            &gpu,
            "visibility_cpu_fallback_frames",
            JsValue::from_f64(self.telemetry.visibility_cpu_fallback_frames as f64),
        );

        let streaming = Object::new();
        object_set(
            &streaming,
            "chunk_hit",
            JsValue::from_f64(self.streaming.chunk_hit as f64),
        );
        object_set(
            &streaming,
            "chunk_miss",
            JsValue::from_f64(self.streaming.chunk_miss as f64),
        );
        object_set(
            &streaming,
            "loaded_chunk_count",
            JsValue::from_f64(self.streaming.loaded_chunk_count as f64),
        );
        object_set(
            &streaming,
            "loaded_bytes",
            JsValue::from_f64(self.streaming.loaded_bytes as f64),
        );
        object_set(
            &streaming,
            "residency_pressure",
            JsValue::from_f64(round3(self.streaming.residency_pressure)),
        );
        object_set(
            &streaming,
            "convergence_stage",
            JsValue::from_str(self.streaming.convergence_stage.as_str()),
        );
        object_set(
            &streaming,
            "residency_class",
            JsValue::from_str(self.streaming.residency_class.as_str()),
        );
        object_set(
            &streaming,
            "adaptation_event_count",
            JsValue::from_f64(self.streaming.adaptation_events.len() as f64),
        );
        if let Some(last_event) = self.streaming.adaptation_events.last() {
            let event = Object::new();
            object_set(&event, "tick", JsValue::from_f64(last_event.tick as f64));
            object_set(
                &event,
                "now_ms",
                JsValue::from_f64(round3(last_event.now_ms)),
            );
            object_set(
                &event,
                "reason",
                JsValue::from_str(last_event.reason.as_str()),
            );
            object_set(
                &event,
                "from_stage",
                JsValue::from_str(last_event.from_stage.as_str()),
            );
            object_set(
                &event,
                "to_stage",
                JsValue::from_str(last_event.to_stage.as_str()),
            );
            object_set(
                &event,
                "from_residency_class",
                JsValue::from_str(last_event.from_residency_class.as_str()),
            );
            object_set(
                &event,
                "to_residency_class",
                JsValue::from_str(last_event.to_residency_class.as_str()),
            );
            object_set(
                &event,
                "residency_pressure",
                JsValue::from_f64(round3(last_event.residency_pressure)),
            );
            object_set(&streaming, "last_adaptation_event", JsValue::from(event));
        }

        let pass_timing_samples = Array::new();
        for sample in &self.telemetry.pass_timings {
            let entry = Object::new();
            object_set(
                &entry,
                "pass_name",
                JsValue::from_str(sample.pass_name.as_str()),
            );
            object_set(
                &entry,
                "pass_kind",
                JsValue::from_str(sample.pass_kind.as_str()),
            );
            object_set(
                &entry,
                "duration_ms",
                JsValue::from_f64(round3(sample.duration_ms)),
            );
            object_set(
                &entry,
                "fallback_estimate",
                JsValue::from_bool(sample.fallback_estimate),
            );
            let _ = pass_timing_samples.push(&JsValue::from(entry));
        }

        let budget_outcomes = Array::new();
        for outcome in &self.telemetry.budget_outcomes {
            let entry = Object::new();
            object_set(
                &entry,
                "frame_index",
                JsValue::from_f64(outcome.frame_index as f64),
            );
            object_set(
                &entry,
                "frame_time_ms",
                JsValue::from_f64(round3(outcome.frame_time_ms)),
            );
            object_set(
                &entry,
                "target_frame_time_ms",
                JsValue::from_f64(round3(outcome.target_frame_time_ms)),
            );
            object_set(
                &entry,
                "budget_delta_ms",
                JsValue::from_f64(round3(outcome.budget_delta_ms)),
            );
            object_set(
                &entry,
                "within_budget",
                JsValue::from_bool(outcome.within_budget),
            );
            object_set(&entry, "long_frame", JsValue::from_bool(outcome.long_frame));
            object_set(&entry, "hitch", JsValue::from_bool(outcome.hitch));
            let _ = budget_outcomes.push(&JsValue::from(entry));
        }

        let frame_budget = Object::new();
        object_set(
            &frame_budget,
            "within_budget_frames",
            JsValue::from_f64(self.telemetry.budget_counters.within_budget_frames as f64),
        );
        object_set(
            &frame_budget,
            "over_budget_frames",
            JsValue::from_f64(self.telemetry.budget_counters.over_budget_frames as f64),
        );
        object_set(
            &frame_budget,
            "long_frame_count",
            JsValue::from_f64(self.telemetry.budget_counters.long_frame_count as f64),
        );
        object_set(
            &frame_budget,
            "hitch_count",
            JsValue::from_f64(self.telemetry.budget_counters.hitch_count as f64),
        );
        object_set(
            &frame_budget,
            "max_consecutive_long_frames",
            JsValue::from_f64(self.telemetry.budget_counters.max_consecutive_long_frames as f64),
        );
        object_set(
            &frame_budget,
            "max_consecutive_hitches",
            JsValue::from_f64(self.telemetry.budget_counters.max_consecutive_hitches as f64),
        );
        if let Some(last_budget_outcome) = self.telemetry.last_budget_outcome.as_ref() {
            let last = Object::new();
            object_set(
                &last,
                "frame_index",
                JsValue::from_f64(last_budget_outcome.frame_index as f64),
            );
            object_set(
                &last,
                "frame_time_ms",
                JsValue::from_f64(round3(last_budget_outcome.frame_time_ms)),
            );
            object_set(
                &last,
                "target_frame_time_ms",
                JsValue::from_f64(round3(last_budget_outcome.target_frame_time_ms)),
            );
            object_set(
                &last,
                "budget_delta_ms",
                JsValue::from_f64(round3(last_budget_outcome.budget_delta_ms)),
            );
            object_set(
                &last,
                "within_budget",
                JsValue::from_bool(last_budget_outcome.within_budget),
            );
            object_set(
                &last,
                "long_frame",
                JsValue::from_bool(last_budget_outcome.long_frame),
            );
            object_set(
                &last,
                "hitch",
                JsValue::from_bool(last_budget_outcome.hitch),
            );
            object_set(&frame_budget, "last_outcome", JsValue::from(last));
        }
        object_set(
            &frame_budget,
            "recent_outcomes",
            JsValue::from(budget_outcomes),
        );

        let governor_actions = Array::new();
        for event in &self.governor.actions {
            let action = Object::new();
            object_set(&action, "tick", JsValue::from_f64(event.tick as f64));
            object_set(
                &action,
                "frame_index",
                JsValue::from_f64(event.frame_index as f64),
            );
            object_set(&action, "now_ms", JsValue::from_f64(round3(event.now_ms)));
            object_set(&action, "action", JsValue::from_str(event.action.as_str()));
            object_set(&action, "reason", JsValue::from_str(event.reason.as_str()));
            object_set(
                &action,
                "budget_delta_ms",
                JsValue::from_f64(round3(event.budget_delta_ms)),
            );
            object_set(
                &action,
                "blocked_by_guardrail",
                JsValue::from_bool(event.blocked_by_guardrail),
            );

            let before = Object::new();
            object_set(
                &before,
                "dynamic_resolution_scale",
                JsValue::from_f64(round3(event.before.dynamic_resolution_scale)),
            );
            object_set(
                &before,
                "shadow_quality_tier",
                JsValue::from_f64(event.before.shadow_quality_tier as f64),
            );
            object_set(
                &before,
                "ssr_quality_tier",
                JsValue::from_f64(event.before.ssr_quality_tier as f64),
            );
            object_set(
                &before,
                "probe_update_rate",
                JsValue::from_f64(round3(event.before.probe_update_rate)),
            );
            object_set(
                &before,
                "volumetric_steps",
                JsValue::from_f64(event.before.volumetric_steps as f64),
            );

            let after = Object::new();
            object_set(
                &after,
                "dynamic_resolution_scale",
                JsValue::from_f64(round3(event.after.dynamic_resolution_scale)),
            );
            object_set(
                &after,
                "shadow_quality_tier",
                JsValue::from_f64(event.after.shadow_quality_tier as f64),
            );
            object_set(
                &after,
                "ssr_quality_tier",
                JsValue::from_f64(event.after.ssr_quality_tier as f64),
            );
            object_set(
                &after,
                "probe_update_rate",
                JsValue::from_f64(round3(event.after.probe_update_rate)),
            );
            object_set(
                &after,
                "volumetric_steps",
                JsValue::from_f64(event.after.volumetric_steps as f64),
            );
            object_set(&action, "before", JsValue::from(before));
            object_set(&action, "after", JsValue::from(after));
            let _ = governor_actions.push(&JsValue::from(action));
        }

        let critical_stability_passes = Array::new();
        for pass in &self.governor.critical_stability_passes {
            let _ = critical_stability_passes.push(&JsValue::from_str(pass.as_str()));
        }

        let governor_bounds = Object::new();
        object_set(
            &governor_bounds,
            "target_frame_time_ms",
            JsValue::from_f64(round3(self.governor.bounds.target_frame_time_ms)),
        );
        object_set(
            &governor_bounds,
            "dynamic_resolution_min",
            JsValue::from_f64(round3(self.governor.bounds.dynamic_resolution_min)),
        );
        object_set(
            &governor_bounds,
            "dynamic_resolution_max",
            JsValue::from_f64(round3(self.governor.bounds.dynamic_resolution_max)),
        );
        object_set(
            &governor_bounds,
            "shadow_tier_min",
            JsValue::from_f64(self.governor.bounds.shadow_tier_min as f64),
        );
        object_set(
            &governor_bounds,
            "shadow_tier_max",
            JsValue::from_f64(self.governor.bounds.shadow_tier_max as f64),
        );
        object_set(
            &governor_bounds,
            "ssr_tier_min",
            JsValue::from_f64(self.governor.bounds.ssr_tier_min as f64),
        );
        object_set(
            &governor_bounds,
            "ssr_tier_max",
            JsValue::from_f64(self.governor.bounds.ssr_tier_max as f64),
        );
        object_set(
            &governor_bounds,
            "probe_rate_min",
            JsValue::from_f64(round3(self.governor.bounds.probe_rate_min)),
        );
        object_set(
            &governor_bounds,
            "probe_rate_max",
            JsValue::from_f64(round3(self.governor.bounds.probe_rate_max)),
        );
        object_set(
            &governor_bounds,
            "volumetric_steps_min",
            JsValue::from_f64(self.governor.bounds.volumetric_steps_min as f64),
        );
        object_set(
            &governor_bounds,
            "volumetric_steps_max",
            JsValue::from_f64(self.governor.bounds.volumetric_steps_max as f64),
        );

        let governor_state = Object::new();
        object_set(
            &governor_state,
            "initialized_from_contracts",
            JsValue::from_bool(self.governor.initialized_from_contracts),
        );
        object_set(
            &governor_state,
            "adaptation_count",
            JsValue::from_f64(self.governor.adaptation_count as f64),
        );
        object_set(
            &governor_state,
            "blocked_stability_disable_attempts",
            JsValue::from_f64(self.governor.blocked_stability_disable_attempts as f64),
        );
        object_set(
            &governor_state,
            "adaptation_cooldown_frames",
            JsValue::from_f64(self.governor.adaptation_cooldown_frames as f64),
        );
        object_set(
            &governor_state,
            "within_budget_streak",
            JsValue::from_f64(self.governor.within_budget_streak as f64),
        );
        object_set(
            &governor_state,
            "over_budget_streak",
            JsValue::from_f64(self.governor.over_budget_streak as f64),
        );
        object_set(
            &governor_state,
            "critical_stability_passes",
            JsValue::from(critical_stability_passes),
        );
        object_set(&governor_state, "bounds", JsValue::from(governor_bounds));
        let governor_budgets = Object::new();
        object_set(
            &governor_budgets,
            "dynamic_resolution_scale",
            JsValue::from_f64(round3(self.governor.budgets.dynamic_resolution_scale)),
        );
        object_set(
            &governor_budgets,
            "shadow_quality_tier",
            JsValue::from_f64(self.governor.budgets.shadow_quality_tier as f64),
        );
        object_set(
            &governor_budgets,
            "ssr_quality_tier",
            JsValue::from_f64(self.governor.budgets.ssr_quality_tier as f64),
        );
        object_set(
            &governor_budgets,
            "probe_update_rate",
            JsValue::from_f64(round3(self.governor.budgets.probe_update_rate)),
        );
        object_set(
            &governor_budgets,
            "volumetric_steps",
            JsValue::from_f64(self.governor.budgets.volumetric_steps as f64),
        );
        object_set(&governor_state, "budgets", JsValue::from(governor_budgets));
        let governor_calibration = Object::new();
        object_set(
            &governor_calibration,
            "active",
            JsValue::from_bool(self.governor.calibration.active),
        );
        object_set(
            &governor_calibration,
            "complete",
            JsValue::from_bool(self.governor.calibration.complete),
        );
        object_set(
            &governor_calibration,
            "sampled_frames",
            JsValue::from_f64(self.governor.calibration.sampled_frames as f64),
        );
        object_set(
            &governor_calibration,
            "sample_target_frames",
            JsValue::from_f64(self.governor.calibration.sample_target_frames as f64),
        );
        object_set(
            &governor_calibration,
            "startup_average_frame_ms",
            JsValue::from_f64(round3(self.governor.calibration.startup_average_frame_ms)),
        );
        object_set(
            &governor_calibration,
            "startup_p95_frame_ms",
            JsValue::from_f64(round3(self.governor.calibration.startup_p95_frame_ms)),
        );
        object_set(
            &governor_state,
            "calibration",
            JsValue::from(governor_calibration),
        );
        object_set(
            &governor_state,
            "actions",
            JsValue::from(governor_actions.clone()),
        );

        let runtime_metrics_v2 = Object::new();
        object_set(
            &runtime_metrics_v2,
            "schema_version",
            JsValue::from_f64(RUNTIME_METRICS_SCHEMA_VERSION as f64),
        );
        object_set(
            &runtime_metrics_v2,
            "kind",
            JsValue::from_str("runtime-metrics-v2"),
        );
        object_set(
            &runtime_metrics_v2,
            "pass_timings_supported",
            JsValue::from_bool(self.telemetry.pass_timing_supported),
        );
        object_set(
            &runtime_metrics_v2,
            "pass_timing_fallback_used",
            JsValue::from_bool(self.telemetry.pass_timing_fallback_used),
        );
        object_set(
            &runtime_metrics_v2,
            "pass_timings",
            JsValue::from(pass_timing_samples),
        );
        object_set(
            &runtime_metrics_v2,
            "frame_budget",
            JsValue::from(frame_budget),
        );
        object_set(
            &runtime_metrics_v2,
            "governor",
            JsValue::from(governor_state),
        );

        let metrics = Object::new();
        object_set(
            &metrics,
            "schema_version",
            JsValue::from_f64(RUNTIME_METRICS_SCHEMA_VERSION as f64),
        );
        object_set(&metrics, "kind", JsValue::from_str("runtime-metrics-v2"));
        object_set(&metrics, "role", JsValue::from_str(self.mmo_role.as_str()));
        object_set(&metrics, "gpu", JsValue::from(gpu));
        object_set(&metrics, "streaming", JsValue::from(streaming));
        object_set(
            &metrics,
            "runtime_metrics_v2",
            JsValue::from(runtime_metrics_v2),
        );
        object_set(
            &metrics,
            "governor_action_trace",
            JsValue::from(governor_actions),
        );
        object_set(
            &metrics,
            "asset_factory_generated_asset_count",
            JsValue::from_f64(self.asset_factory_generated_asset_count as f64),
        );
        object_set(
            &metrics,
            "asset_factory_provenance_entry_count",
            JsValue::from_f64(self.asset_factory_provenance_entry_count as f64),
        );
        object_set(
            &metrics,
            "ui_atlas_count",
            JsValue::from_f64(self.ui_atlas_count as f64),
        );
        object_set(
            &metrics,
            "character_bundle_count",
            JsValue::from_f64(self.character_bundle_count as f64),
        );
        set_global_object("__wrelaMetrics", &metrics);
    }

    fn hash_state(&self) -> u64 {
        const HASH_PRIME: u64 = 1_099_511_628_211;
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        hash ^= self.state.tick;
        hash = hash.wrapping_mul(HASH_PRIME);
        hash ^= self.state.player_x.to_bits() as u64;
        hash = hash.wrapping_mul(HASH_PRIME);
        hash ^= self.state.player_y.to_bits() as u64;
        hash = hash.wrapping_mul(HASH_PRIME);
        hash ^= u64::from(self.state.score);
        hash = hash.wrapping_mul(HASH_PRIME);
        hash ^= u64::from(self.state.collected_mask);
        hash = hash.wrapping_mul(HASH_PRIME);
        hash
    }

    fn apply_local_input(&mut self, axis_x: f32, axis_y: f32, dt_ms: u32) {
        let dt_seconds = (dt_ms.max(1) as f32) / 1000.0;
        let speed_px_per_second = 240.0;
        self.state.player_x += axis_x * speed_px_per_second * dt_seconds;
        self.state.player_y += axis_y * speed_px_per_second * dt_seconds;

        let min_x = PLAYER_HALF_SIZE;
        let min_y = PLAYER_HALF_SIZE;
        let max_x = (self.world_width - PLAYER_HALF_SIZE).max(PLAYER_HALF_SIZE);
        let max_y = (self.world_height - PLAYER_HALF_SIZE).max(PLAYER_HALF_SIZE);
        self.state.player_x = self.state.player_x.clamp(min_x, max_x);
        self.state.player_y = self.state.player_y.clamp(min_y, max_y);
        self.state.tick = self.state.tick.saturating_add(1);

        let min_distance_sq = PICKUP_RADIUS * PICKUP_RADIUS;
        for (idx, (x, y)) in self.collectible_positions.iter().enumerate() {
            if idx >= u32::BITS as usize {
                break;
            }
            let mask = 1u32 << idx;
            if self.state.collected_mask & mask != 0 {
                continue;
            }
            let dx = *x - self.state.player_x;
            let dy = *y - self.state.player_y;
            if dx * dx + dy * dy <= min_distance_sq {
                self.state.collected_mask |= mask;
                self.state.score = self.state.score.saturating_add(1);
            }
        }
        self.refresh_win_state();
    }

    fn overwrite_with_authoritative_snapshot(&mut self, snapshot: &SnapshotPayload) {
        self.state.tick = snapshot.tick;
        self.state.player_x = snapshot.player_x;
        self.state.player_y = snapshot.player_y;
        self.state.score = snapshot.score;
        self.state.collected_mask = snapshot.collected_mask;
        self.local_tick = self.local_tick.max(snapshot.tick);
        self.refresh_win_state();
    }

    fn apply_authoritative_delta(&mut self, delta: &StateDeltaPayload) {
        apply_predicted_state_delta(&mut self.state, delta);
        self.local_tick = self.local_tick.max(self.state.tick);
        self.refresh_win_state();
    }

    fn total_collectibles(&self) -> u32 {
        self.collectible_positions.len().min(MAX_COLLECTIBLES) as u32
    }

    fn win_target_score(&self) -> u32 {
        let total = self.total_collectibles();
        if total == 0 { 0 } else { 1 }
    }

    fn refresh_win_state(&mut self) {
        let target = self.win_target_score();
        self.game_won = target > 0 && self.state.score >= target;
    }

    fn next_restart_signal(&mut self) -> bool {
        let decision = decide_restart_signal(
            self.collect_pressed,
            self.game_won,
            self.restart_button_latched,
        );
        self.restart_button_latched = decision.restart_button_latched;
        decision.restart_requested
    }

    fn apply_local_restart(&mut self) {
        self.state = PredictedState::default();
        self.pending_inputs.clear();
        self.ack = 0;
        self.local_tick = 0;
        self.refresh_win_state();
    }

    fn prune_pending_inputs(&mut self) {
        let ack = self.ack;
        self.pending_inputs.retain(|input| input.seq > ack);
    }

    fn replay_pending_inputs(&mut self) {
        let pending = self.pending_inputs.clone();
        for input in pending {
            self.apply_local_input(input.axis_x, input.axis_y, input.dt_ms);
        }
    }

    fn apply_server_state_payload(&mut self, payload: ServerStatePayload, count_correction: bool) {
        if let Some(role) = payload.role.as_deref() {
            self.mmo_role = normalize_mmo_role(role).to_string();
        }
        if let Some(ack) = payload.ack {
            self.ack = self.ack.max(ack);
            self.prune_pending_inputs();
        }
        if let Some(snapshot) = payload.snapshot.as_ref() {
            self.overwrite_with_authoritative_snapshot(snapshot);
            self.replay_pending_inputs();
            if payload.forced_divergence {
                self.last_forced_drift_tick = Some(snapshot.tick);
            }
        } else if let Some(delta) = payload.delta.as_ref() {
            self.apply_authoritative_delta(delta);
            if payload.forced_divergence {
                self.last_forced_drift_tick = Some(self.state.tick);
            }
        }
        if count_correction {
            self.correction_count = self.correction_count.saturating_add(1);
        }
    }

    fn on_hello(&mut self, payload: HelloPayload) {
        self.mmo_role = normalize_mmo_role(payload.role.as_str()).to_string();
        if payload.world_width > 1.0 {
            self.world_width = payload.world_width;
        } else {
            self.world_width = DEFAULT_WORLD_WIDTH;
        }
        if payload.world_height > 1.0 {
            self.world_height = payload.world_height;
        } else {
            self.world_height = DEFAULT_WORLD_HEIGHT;
        }

        self.collectible_positions = payload.collectibles;
        self.ack = 0;
        self.pending_inputs.clear();
        if let Some(snapshot) = payload.snapshot.as_ref() {
            self.overwrite_with_authoritative_snapshot(snapshot);
        } else {
            self.refresh_win_state();
        }
        self.set_status(self.ready_status_line.clone());
    }

    fn send_input_if_ready(&mut self, now_ms: f64) {
        if now_ms - self.last_sent_at_ms < f64::from(TICK_DT_MS.saturating_sub(1)) {
            return;
        }
        let ws = self.ws.as_ref().cloned();
        let transport_ready = ws
            .as_ref()
            .is_some_and(|socket| socket.ready_state() == WebSocket::OPEN)
            && self.session_id > 0;

        let (axis_x, axis_y) = self.buttons.axes();
        self.local_tick = self.local_tick.saturating_add(1);

        let restart_requested = self.next_restart_signal();
        let input = PendingInput {
            seq: self.seq,
            tick: self.local_tick,
            axis_x,
            axis_y,
            dt_ms: TICK_DT_MS,
            collect_pressed: restart_requested,
        };
        self.seq = self.seq.saturating_add(1);

        if restart_requested && !transport_ready {
            self.apply_local_restart();
            self.last_sent_at_ms = now_ms;
            return;
        }

        self.apply_local_input(input.axis_x, input.axis_y, input.dt_ms);
        if !transport_ready {
            self.last_sent_at_ms = now_ms;
            return;
        }
        self.pending_inputs.push(input.clone());

        let payload = OutboundInputBatch {
            inputs: vec![OutboundInput {
                seq: input.seq,
                tick: input.tick,
                axis_x: input.axis_x,
                axis_y: input.axis_y,
                dt_ms: input.dt_ms,
                collect_pressed: input.collect_pressed,
            }],
        };

        let encoded_payload = match serde_json::to_vec(&payload) {
            Ok(payload) => payload,
            Err(error) => {
                self.set_status(format!("failed to encode input payload: {error}"));
                return;
            }
        };

        let frame = Envelope {
            version: PROTOCOL_V5,
            sub_version: PROTOCOL_V5_SUB_VERSION,
            partition_id: self.partition_id,
            session_id: self.session_id,
            actor_id: self.actor_id,
            message_type: MessageTypeV5::InputBatchV5,
            tick: input.tick,
            seq: input.seq,
            ack: self.ack,
            payload: encoded_payload,
        };

        let encoded_frame = match frame.encode() {
            Ok(bytes) => bytes,
            Err(error) => {
                self.set_status(format!("failed to encode protocol envelope: {error}"));
                return;
            }
        };

        let Some(socket) = ws.as_ref() else {
            self.set_status("failed sending input frame: transport unavailable".to_string());
            return;
        };
        if let Err(error) = socket.send_with_u8_array(encoded_frame.as_slice()) {
            self.set_status(format!("failed sending input frame: {error:?}"));
            return;
        }

        self.last_sent_at_ms = now_ms;
    }

    fn render_frame(&mut self, now_ms: f64) {
        self.sync_canvas_size();

        let mut observed_frame_ms = None::<f64>;
        if self.telemetry.last_frame_at > 0.0 {
            let frame_ms = now_ms - self.telemetry.last_frame_at;
            self.telemetry.frame_times_ms.push_back(frame_ms);
            if self.telemetry.frame_times_ms.len() > 240 {
                self.telemetry.frame_times_ms.pop_front();
            }
            observed_frame_ms = Some(frame_ms);
        }
        self.telemetry.last_frame_at = now_ms;

        let scene = RenderSceneSnapshot {
            world_width: self.world_width,
            world_height: self.world_height,
            player_x: self.state.player_x,
            player_y: self.state.player_y,
            collected_mask: self.state.collected_mask,
            app_mode_is_website: self.app_mode == "website",
            collectible_positions: self.collectible_positions.clone(),
        };

        let canvas_width = self.canvas.width();
        let canvas_height = self.canvas.height();
        let dynamic_scale = self.governor.budgets.dynamic_resolution_scale.clamp(
            self.governor.bounds.dynamic_resolution_min,
            self.governor.bounds.dynamic_resolution_max,
        );
        let render_width = ((canvas_width as f64) * dynamic_scale).round().max(1.0) as u32;
        let render_height = ((canvas_height as f64) * dynamic_scale).round().max(1.0) as u32;

        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };

        // ── Preview mode rendering ──────────────────────────────────
        if self.preview_state.is_some() {
            // Process any pending mesh load requests
            let requests: Vec<crate::preview_mode::MeshLoadRequest> =
                if let Some(ref mut ps) = self.preview_state {
                    ps.needs_mesh_load.drain(..).collect()
                } else {
                    Vec::new()
                };
            for req in requests {
                // Spawn async fetch for each GLB mesh
                APP_RUNTIME.with(|state| {
                    if let Some(runtime_rc) = state.borrow().as_ref() {
                        let runtime_for_load = runtime_rc.clone();
                        let entity_name = req.entity_name.clone();
                        let url = req.url.clone();
                        wasm_bindgen_futures::spawn_local(async move {
                            match crate::mesh::fetch_glb_bytes(&url).await {
                                Ok(bytes) => {
                                    match crate::mesh::load_glb(&bytes) {
                                        Ok(mesh_data_vec) => {
                                            if let Ok(mut rt) = runtime_for_load.try_borrow_mut() {
                                                if let Some(ref mut renderer) = rt.renderer {
                                                    let base_idx = upload_procedural_meshes(
                                                        renderer,
                                                        &mesh_data_vec,
                                                    );
                                                    if let Some(ref mut ps) = rt.preview_state {
                                                        ps.set_entity_mesh(&entity_name, base_idx);
                                                    }
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            web_sys::console::warn_1(
                                                &format!("Preview: GLB parse error for {entity_name}: {e}").into(),
                                            );
                                        }
                                    }
                                }
                                Err(e) => {
                                    web_sys::console::warn_1(
                                        &format!("Preview: fetch error for {entity_name}: {e}").into(),
                                    );
                                }
                            }
                        });
                    }
                });
            }

            // Rebuild scene instances from preview entities + grid floor
            let mut instances = Vec::new();

            // Grid floor
            if let Some(ref ps) = self.preview_state {
                if let Some(grid_idx) = ps.grid_mesh_index {
                    instances.push(MeshInstance {
                        mesh_index: grid_idx,
                        model_matrix: crate::camera_math::mat4_identity(),
                    });
                }
            }

            // Scene entities with loaded meshes
            if let Some(ref ps) = self.preview_state {
                for entity in &ps.entities {
                    if let Some(mesh_idx) = entity.mesh_index {
                        instances.push(MeshInstance {
                            mesh_index: mesh_idx,
                            model_matrix: entity.model_matrix,
                        });
                    }
                }
            }

            self.scene_3d_instances = instances;

            // Use preview camera
            let (cam_view, cam_proj, cam_pos, light_dir, light_col, ambient) =
                if let Some(ref ps) = self.preview_state {
                    let aspect = canvas_width.max(1) as f32 / canvas_height.max(1) as f32;
                    (
                        ps.camera.view_matrix(),
                        ps.camera.projection_matrix(aspect),
                        ps.camera.eye_position(),
                        ps.sun_direction(),
                        ps.sun_color(),
                        ps.ambient_color(),
                    )
                } else {
                    // fallback (should never reach here)
                    let aspect = canvas_width.max(1) as f32 / canvas_height.max(1) as f32;
                    let cam = crate::camera_math::OrbitCamera::default();
                    (
                        cam.view_matrix(),
                        cam.projection_matrix(aspect),
                        cam.eye_position(),
                        [-0.5, -0.5, -0.3],
                        [2.5, 2.0, 1.2],
                        [0.2, 0.25, 0.35],
                    )
                };

            let scene_3d = RenderSceneSnapshot3D {
                camera_view: cam_view,
                camera_proj: cam_proj,
                camera_position: cam_pos,
                mesh_instances: &self.scene_3d_instances,
                light_direction: light_dir,
                light_color: light_col,
                ambient_color: ambient,
                hit_stop_active: false,
                hit_stop_intensity: 0.0,
                camera_shake: 0.0,
                parry_flash_alpha: 0.0,
                chromatic_aberration: 0.0,
                delta_time_secs: observed_frame_ms.unwrap_or(16.0) as f32 / 1000.0,
                player_state: 0,
            };

            match renderer.render_3d(&scene_3d, canvas_width, canvas_height) {
                Ok(RenderFrameResult::Rendered(stats)) => {
                    self.telemetry.draw_calls = self
                        .telemetry
                        .draw_calls
                        .saturating_add(stats.render_passes as u64);
                    self.telemetry.gpu_upload_bytes = self
                        .telemetry
                        .gpu_upload_bytes
                        .saturating_add(stats.upload_bytes);
                    self.frame_graph = renderer.runtime_evidence();
                }
                Ok(RenderFrameResult::SurfaceTimeout) => {}
                Ok(RenderFrameResult::BlockedOnPrewarm) => {}
                Err(error) => self.set_status(format!("Preview render error: {error}")),
            }
            return;
        }

        if self.render_mode_3d {
            // ── Game tick ──────────────────────────────────────────────────
            // Convert WASD movement buttons into milli-scaled move vector
            let move_x = (self.buttons.bitmask() & 0b1000 != 0) as i32 * 1000 // right
                       - (self.buttons.bitmask() & 0b0100 != 0) as i32 * 1000; // left
            let move_z = (self.buttons.bitmask() & 0b0001 != 0) as i32 * 1000  // up = +z
                       - (self.buttons.bitmask() & 0b0010 != 0) as i32 * 1000; // down = -z
            self.game_input.move_x = move_x;
            self.game_input.move_z = move_z;

            // Deterministic test-only kill chord (A+B) for restart-loop validation.
            // This is gated behind the deterministic time driver so normal gameplay
            // controls are unaffected.
            if self.deterministic_time_driver_enabled
                && self.game_input.attack_light
                && self.game_input.parry
            {
                if let Some(ref mut game_state) = self.game_state {
                    if game_state.player_health > 0 {
                        game_state.player_health = 0;
                        self.combat_events.death_count =
                            self.combat_events.death_count.saturating_add(1);
                    }
                }
            }

            // ── Restart on death ──────────────────────────────────────
            if self.hud_restart_pressed {
                self.hud_restart_pressed = false;
                if let Some(ref gs) = self.game_state {
                    if gs.player_health <= 0 {
                        self.game_state = Some(crate::game_logic::GameState::new());
                        self.combat_events.restart_count =
                            self.combat_events.restart_count.saturating_add(1);
                    }
                }
            }

            if let Some(ref game_state) = self.game_state {
                let previous_state = game_state.clone();
                let new_state = crate::game_logic::tick_game(game_state, &self.game_input);
                let rd = new_state.render_data();

                if previous_state.lock_on_target != new_state.lock_on_target {
                    if previous_state.lock_on_target < 0 || new_state.lock_on_target < 0 {
                        self.combat_events.lock_toggle_count =
                            self.combat_events.lock_toggle_count.saturating_add(1);
                    } else {
                        self.combat_events.target_cycle_count =
                            self.combat_events.target_cycle_count.saturating_add(1);
                    }
                }
                if previous_state.player_state != new_state.player_state {
                    match new_state.player_state {
                        PLAYER_STATE_ATTACK => {
                            if new_state.player_attack_heavy {
                                self.combat_events.attack_heavy_count =
                                    self.combat_events.attack_heavy_count.saturating_add(1);
                            } else {
                                self.combat_events.attack_light_count =
                                    self.combat_events.attack_light_count.saturating_add(1);
                            }
                        }
                        PLAYER_STATE_PARRY_ACTIVE => {
                            self.combat_events.parry_count =
                                self.combat_events.parry_count.saturating_add(1);
                        }
                        PLAYER_STATE_DODGE => {
                            self.combat_events.dodge_count =
                                self.combat_events.dodge_count.saturating_add(1);
                        }
                        _ => {}
                    }
                }
                if previous_state.player_health > 0 && new_state.player_health <= 0 {
                    self.combat_events.death_count =
                        self.combat_events.death_count.saturating_add(1);
                }

                // Update dynamic mesh instance transforms using tracked scene indices.
                if let Some(player_idx) = self
                    .player_instance_index
                    .filter(|&idx| idx < self.scene_3d_instances.len())
                {
                    self.scene_3d_instances[player_idx].model_matrix =
                        crate::camera_math::mat4_translation(
                            rd.player_pos[0],
                            rd.player_pos[1],
                            rd.player_pos[2],
                        );
                    // Rotate player to face direction
                    if rd.player_facing[0].abs() > 0.01 || rd.player_facing[1].abs() > 0.01 {
                        let angle = rd.player_facing[0].atan2(rd.player_facing[1]);
                        let rot = crate::camera_math::mat4_rotation_y(angle);
                        let trans = self.scene_3d_instances[player_idx].model_matrix;
                        self.scene_3d_instances[player_idx].model_matrix =
                            crate::camera_math::mat4_mul(trans, rot);
                    }
                }

                for (enemy_lane_idx, instance_idx) in self.enemy_instance_indices.iter().enumerate() {
                    if *instance_idx >= self.scene_3d_instances.len() {
                        continue;
                    }
                    let maybe_enemy = if enemy_lane_idx < new_state.enemy_count {
                        Some(new_state.enemies[enemy_lane_idx])
                    } else {
                        None
                    };
                    if let Some(enemy) = maybe_enemy.filter(|enemy| enemy.alive) {
                        let enemy_pos = [
                            enemy.x as f32 / 1000.0,
                            enemy.y as f32 / 1000.0,
                            enemy.z as f32 / 1000.0,
                        ];
                        let enemy_facing = [
                            enemy.facing_x as f32 / 1000.0,
                            enemy.facing_z as f32 / 1000.0,
                        ];
                        self.scene_3d_instances[*instance_idx].model_matrix =
                            crate::camera_math::mat4_translation(
                                enemy_pos[0],
                                enemy_pos[1],
                                enemy_pos[2],
                            );
                        if enemy_facing[0].abs() > 0.01 || enemy_facing[1].abs() > 0.01 {
                            let angle = enemy_facing[0].atan2(enemy_facing[1]);
                            let rot = crate::camera_math::mat4_rotation_y(angle);
                            let trans = self.scene_3d_instances[*instance_idx].model_matrix;
                            self.scene_3d_instances[*instance_idx].model_matrix =
                                crate::camera_math::mat4_mul(trans, rot);
                        }
                    } else {
                        // Move dead/unused enemy lanes off screen.
                        self.scene_3d_instances[*instance_idx].model_matrix =
                            crate::camera_math::mat4_translation(0.0, -100.0, 0.0);
                    }
                }

                // Update camera to follow lock-on focus or player with smooth damping
                let frame_dt = observed_frame_ms.unwrap_or(16.0) as f32 / 1000.0;
                let desired_target = if rd.lock_on_active && rd.lock_on_target_index >= 0 {
                    let enemy_y = rd.lock_on_target_pos[1] + 1.2;
                    [
                        (rd.player_pos[0] + rd.lock_on_target_pos[0]) * 0.5,
                        ((rd.player_pos[1] + 1.0) + enemy_y) * 0.5,
                        (rd.player_pos[2] + rd.lock_on_target_pos[2]) * 0.5,
                    ]
                } else {
                    [rd.player_pos[0], rd.player_pos[1] + 1.0, rd.player_pos[2]]
                };
                self.orbit_camera.smooth_follow(desired_target, frame_dt, 5.0);

                // Camera shake offset (applied after smooth follow)
                let shake = rd.camera_shake;
                if shake > 0.001 {
                    let phase = (new_state.tick_count as f32) * 17.3;
                    self.orbit_camera.target[0] += phase.sin() * shake * 0.3;
                    self.orbit_camera.target[1] += phase.cos() * shake * 0.2;
                }

                // Camera mode is lock-on first to avoid distance-heuristic pops.
                if rd.lock_on_active {
                    self.orbit_camera
                        .set_mode(crate::camera_math::CameraMode::Combat);
                } else {
                    self.orbit_camera
                        .set_mode(crate::camera_math::CameraMode::Exploration);
                }
                self.orbit_camera.update_mode_blend(frame_dt, 3.0);

                // Parry flash: trigger on parry, decay over ~3 frames
                if rd.parry_flash {
                    self.parry_flash_alpha = 1.0;
                } else {
                    self.parry_flash_alpha *= 0.55; // ~3 frame fade
                    if self.parry_flash_alpha < 0.01 {
                        self.parry_flash_alpha = 0.0;
                    }
                }

                // FOV pulse: narrow during hit stop, snap back smoothly after
                if rd.hit_stop_active {
                    // Reduce FOV by ~3 degrees (0.052 rad) during hit stop
                    self.orbit_camera.fov_y = self.base_fov_y - 0.052;
                } else {
                    // Smoothly return to base FOV
                    self.orbit_camera.fov_y += (self.base_fov_y - self.orbit_camera.fov_y) * 0.15;
                }

                self.game_state = Some(new_state);
            }

            // ── HUD update ──────────────────────────────────────────────
            if let (Some(hud), Some(gs)) = (&mut self.hud, &self.game_state) {
                hud.update(
                    &crate::hud::HudState {
                        player_health_ratio: gs.player_health_ratio(),
                        player_stamina_ratio: gs.player_stamina_ratio(),
                        enemy_health_ratio: gs.enemy_health_ratio(),
                        enemy_alive: gs.enemy_health > 0,
                        resonance_tier: gs.resonance_tier(),
                        resonance: gs.resonance,
                        kills: gs.kills,
                        player_dead: gs.player_health <= 0,
                    },
                    now_ms,
                );
            }

            let aspect = canvas_width.max(1) as f32 / canvas_height.max(1) as f32;
            // Derive combat visual effect parameters from game state
            let (hit_stop_active, hit_stop_intensity, cam_shake, chromatic_ab) =
                if let Some(ref gs) = self.game_state {
                    let rd = gs.render_data();
                    let intensity = if rd.hit_stop_active {
                        (gs.hit_stop_remaining as f32 / 8.0).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    // Chromatic aberration: proportional to camera shake
                    let ca = rd.camera_shake.min(1.0);
                    (rd.hit_stop_active, intensity, rd.camera_shake, ca)
                } else {
                    (false, 0.0, 0.0, 0.0)
                };

            let scene_3d = RenderSceneSnapshot3D {
                camera_view: self.orbit_camera.view_matrix(),
                camera_proj: self.orbit_camera.projection_matrix(aspect),
                camera_position: self.orbit_camera.eye_position(),
                mesh_instances: &self.scene_3d_instances,
                // Gothic forest key light: cool directional key with stronger downward bias.
                light_direction: [-0.35, -0.72, -0.25],
                // Slightly cool key preserves stylized highlights without white clipping.
                light_color: [1.45, 1.4, 1.95],
                // Dark ambient floor keeps silhouettes readable in combat framing.
                ambient_color: [0.07, 0.1, 0.16],
                hit_stop_active,
                hit_stop_intensity,
                camera_shake: cam_shake,
                parry_flash_alpha: self.parry_flash_alpha,
                chromatic_aberration: chromatic_ab,
                delta_time_secs: observed_frame_ms.unwrap_or(16.0) as f32 / 1000.0,
                player_state: self.game_state.as_ref().map_or(0, |gs| gs.player_state),
            };

            // Pass full canvas dimensions (not dynamic-scaled) because in WebGPU
            // on the web, the surface texture is always sized to the canvas backing
            // buffer, ignoring the configured width/height. Using scaled dimensions
            // would cause a depth texture vs surface texture size mismatch.
            match renderer.render_3d(&scene_3d, canvas_width, canvas_height) {
                Ok(RenderFrameResult::Rendered(stats)) => {
                    self.telemetry.draw_calls = self
                        .telemetry
                        .draw_calls
                        .saturating_add(stats.render_passes as u64);
                    self.telemetry.gpu_upload_bytes = self
                        .telemetry
                        .gpu_upload_bytes
                        .saturating_add(stats.upload_bytes);
                    self.telemetry.pass_timing_supported = stats.pass_timing_supported;
                    self.telemetry.pass_timing_fallback_used = stats.pass_timing_fallback_used;
                    self.telemetry.pass_timings = stats.pass_timings;
                    self.frame_graph = renderer.runtime_evidence();
                    if observed_frame_ms.is_none() && stats.frame_cpu_ms > 0.0 {
                        observed_frame_ms = Some(stats.frame_cpu_ms);
                    }
                }
                Ok(RenderFrameResult::SurfaceTimeout) => {}
                Ok(RenderFrameResult::BlockedOnPrewarm) => {}
                Err(error) => self.set_status(format!("WebGPU 3D render error: {error}")),
            }

            self.refresh_governor_contracts_from_frame_graph();
            if let Some(frame_ms) =
                observed_frame_ms.filter(|value| value.is_finite() && *value > 0.0)
            {
                let budget_outcome = self.observe_frame_budget(frame_ms);
                self.apply_quality_governor(now_ms, &budget_outcome);
            }
            return;
        }

        match renderer.render(&scene, render_width, render_height) {
            Ok(RenderFrameResult::Rendered(stats)) => {
                self.telemetry.draw_calls = self
                    .telemetry
                    .draw_calls
                    .saturating_add(stats.render_passes as u64);
                self.telemetry.compute_passes = self
                    .telemetry
                    .compute_passes
                    .saturating_add(stats.compute_passes as u64);
                self.telemetry.gpu_upload_bytes = self
                    .telemetry
                    .gpu_upload_bytes
                    .saturating_add(stats.upload_bytes);
                self.telemetry.visibility_candidate_draws = self
                    .telemetry
                    .visibility_candidate_draws
                    .saturating_add(stats.visibility.candidate_draws as u64);
                self.telemetry.visibility_visible_draws = self
                    .telemetry
                    .visibility_visible_draws
                    .saturating_add(stats.visibility.visible_draws as u64);
                self.telemetry.visibility_culled_ratio = stats.visibility.culled_ratio as f64;
                self.telemetry.visibility_indirect_draw_count = self
                    .telemetry
                    .visibility_indirect_draw_count
                    .saturating_add(stats.indirect_draw_count as u64);
                self.telemetry.pass_timing_supported = stats.pass_timing_supported;
                self.telemetry.pass_timing_fallback_used = stats.pass_timing_fallback_used;
                self.telemetry.pass_timings = stats.pass_timings;
                if stats.visibility.cpu_fallback_used {
                    self.telemetry.visibility_cpu_fallback_frames = self
                        .telemetry
                        .visibility_cpu_fallback_frames
                        .saturating_add(1);
                }
                self.frame_graph = renderer.runtime_evidence();
                self.frame_graph.frame_graph_last_compute_passes = stats.compute_passes;
                self.frame_graph.visibility_candidate_draws = stats.visibility.candidate_draws;
                self.frame_graph.visibility_visible_draws = stats.visibility.visible_draws;
                self.frame_graph.visibility_culled_ratio = stats.visibility.culled_ratio as f64;
                self.frame_graph.visibility_indirect_draw_count =
                    stats.visibility.indirect_draw_count;
                self.frame_graph.visibility_cpu_fallback_used = stats.visibility.cpu_fallback_used;
                self.frame_graph.visibility_indirect_path_default =
                    stats.visibility.indirect_submission_path_default;
                self.frame_graph.hiz_occlusion_tier_enabled =
                    stats.visibility.hiz_occlusion_tier_enabled;
                if observed_frame_ms.is_none() && stats.frame_cpu_ms > 0.0 {
                    observed_frame_ms = Some(stats.frame_cpu_ms);
                }
            }
            Ok(RenderFrameResult::SurfaceTimeout) => {}
            Ok(RenderFrameResult::BlockedOnPrewarm) => {
                self.telemetry.compile_stall_count =
                    self.telemetry.compile_stall_count.saturating_add(1);
                self.telemetry.prewarm_blocked_frames =
                    self.telemetry.prewarm_blocked_frames.saturating_add(1);
                self.frame_graph = renderer.runtime_evidence();
                self.telemetry.pass_timing_supported = false;
                self.telemetry.pass_timing_fallback_used = true;
                self.telemetry.pass_timings.clear();
            }
            Err(error) => self.set_status(format!("WebGPU render error: {error}")),
        }
        self.refresh_governor_contracts_from_frame_graph();
        if let Some(frame_ms) = observed_frame_ms.filter(|value| value.is_finite() && *value > 0.0)
        {
            let budget_outcome = self.observe_frame_budget(frame_ms);
            self.apply_quality_governor(now_ms, &budget_outcome);
        }
    }

    fn run_runtime_step(&mut self, now_ms: f64) {
        self.send_input_if_ready(now_ms);
        self.render_frame(now_ms);
        self.update_streaming_scheduler(now_ms);
        self.publish_runtime_state();
        self.publish_metrics();
    }

    fn advance_time(&mut self, ms: f64) {
        if !ms.is_finite() || ms <= 0.0 {
            return;
        }
        if !self.deterministic_time_driver_enabled {
            self.deterministic_now_ms = self.telemetry.last_frame_at.max(self.last_sent_at_ms);
        }
        self.deterministic_time_driver_enabled = true;
        let target = self.deterministic_now_ms + ms;
        let step = f64::from(TICK_DT_MS.max(1));

        while self.deterministic_now_ms + step <= target {
            self.deterministic_now_ms += step;
            self.run_runtime_step(self.deterministic_now_ms);
        }

        if self.deterministic_now_ms < target {
            self.deterministic_now_ms = target;
            self.run_runtime_step(self.deterministic_now_ms);
        }
    }

    fn render_game_to_text(&self) -> String {
        let collectibles = self
            .collectible_positions
            .iter()
            .enumerate()
            .map(|(idx, (x, y))| {
                let collected = if idx >= u32::BITS as usize {
                    true
                } else {
                    (self.state.collected_mask & (1u32 << idx)) != 0
                };
                serde_json::json!({
                    "index": idx,
                    "x": round3(*x as f64),
                    "y": round3(*y as f64),
                    "collected": collected,
                })
            })
            .collect::<Vec<_>>();
        let win_target_score = self.win_target_score();
        let player_state = self
            .game_state
            .as_ref()
            .map_or(0, |state| state.player_state);
        let combat_camera = self
            .game_state
            .as_ref()
            .map(|state| {
                let rd = state.render_data();
                let dx = rd.player_pos[0] - rd.lock_on_target_pos[0];
                let dz = rd.player_pos[2] - rd.lock_on_target_pos[2];
                let lock_on_target_distance = (dx * dx + dz * dz).sqrt();
                serde_json::json!({
                    "enemy_count": rd.enemy_count,
                    "rendered_enemy_instance_count": self.enemy_instance_indices.len(),
                    "lock_on_active": rd.lock_on_active,
                    "lock_on_target_index": rd.lock_on_target_index,
                    "lock_on_target_distance": round3(lock_on_target_distance as f64),
                    "boss_phase": rd.boss_phase,
                    "readability_state": rd.readability_state,
                    "camera_eye": {
                        "x": round3(rd.camera_eye[0] as f64),
                        "y": round3(rd.camera_eye[1] as f64),
                        "z": round3(rd.camera_eye[2] as f64),
                    },
                    "camera_target": {
                        "x": round3(rd.camera_target[0] as f64),
                        "y": round3(rd.camera_target[1] as f64),
                        "z": round3(rd.camera_target[2] as f64),
                    },
                    "lock_on_target_pos": {
                        "x": round3(rd.lock_on_target_pos[0] as f64),
                        "y": round3(rd.lock_on_target_pos[1] as f64),
                        "z": round3(rd.lock_on_target_pos[2] as f64),
                    }
                })
            })
            .unwrap_or_else(|| {
                serde_json::json!({
                    "enemy_count": 0,
                    "rendered_enemy_instance_count": self.enemy_instance_indices.len(),
                    "lock_on_active": false,
                    "lock_on_target_index": -1,
                })
            });
        let scene_combat_arena = self.scene_combat_arena_extents.map(|extents| {
            let center = extents.center();
            serde_json::json!({
                "min": extents.min,
                "max": extents.max,
                "center": center,
            })
        });
        let payload = serde_json::json!({
            "coordinate_system": "origin=(0,0) top-left; +x right; +y down",
            "role": self.mmo_role.as_str(),
            "status": self.status,
            "tick": self.state.tick,
            "player_state": player_state,
            "player": {
                "x": round3(self.state.player_x as f64),
                "y": round3(self.state.player_y as f64),
                "half_size": PLAYER_HALF_SIZE,
            },
            "world": {
                "width": self.world_width,
                "height": self.world_height,
            },
            "score": self.state.score,
            "collectibles_total": self.total_collectibles(),
            "collectibles_remaining": self.total_collectibles().saturating_sub(self.state.score),
            "win_target_score": win_target_score,
            "collected_mask": format!("0x{:08x}", self.state.collected_mask),
            "collectibles": collectibles,
            "won": self.game_won,
            "combat_camera": combat_camera,
            "combat_events": {
                "lock_toggles": self.combat_events.lock_toggle_count,
                "target_cycles": self.combat_events.target_cycle_count,
                "attack_light": self.combat_events.attack_light_count,
                "attack_heavy": self.combat_events.attack_heavy_count,
                "parry": self.combat_events.parry_count,
                "dodge": self.combat_events.dodge_count,
                "deaths": self.combat_events.death_count,
                "restarts": self.combat_events.restart_count,
            },
            "scene_layout": {
                "camera_anchor_count": self.scene_camera_anchor_count,
                "default_camera_anchor_id": self.scene_default_camera_anchor_id.clone(),
                "combat_arena_extents": scene_combat_arena,
                "fog_volume_count": self.scene_fog_volume_count,
                "lut_profile_id": self.scene_lut_profile_id.clone(),
            },
            "frame_graph": {
                "path_used": self.frame_graph.frame_graph_path_used,
                "declared_passes": self.frame_graph.frame_graph_declared_passes,
                "frames_executed": self.frame_graph.frame_graph_frames_executed,
                "last_render_passes": self.frame_graph.frame_graph_last_render_passes,
                "last_compute_passes": self.frame_graph.frame_graph_last_compute_passes,
                "compute_pass_manifest_ready": self.frame_graph.compute_pass_manifest_ready,
                "prewarm_required_groups": self.frame_graph.prewarm_required_groups,
                "prewarm_completed_required_groups": self.frame_graph.prewarm_completed_required_groups,
                "prewarm_required_complete": self.frame_graph.prewarm_required_complete,
                "prewarm_blocked_frames": self.frame_graph.prewarm_blocked_frames,
                "compile_stall_count": self.telemetry.compile_stall_count,
                "visibility_candidate_draws": self.frame_graph.visibility_candidate_draws,
                "visibility_visible_draws": self.frame_graph.visibility_visible_draws,
                "visibility_culled_ratio": round3(self.frame_graph.visibility_culled_ratio),
                "visibility_indirect_draw_count": self.frame_graph.visibility_indirect_draw_count,
                "visibility_cpu_fallback_used": self.frame_graph.visibility_cpu_fallback_used,
                "visibility_indirect_path_default": self.frame_graph.visibility_indirect_path_default,
                "hiz_occlusion_tier_enabled": self.frame_graph.hiz_occlusion_tier_enabled,
            },
            "asset_factory": {
                "contract_valid": self.asset_factory_contract_valid,
                "generated_asset_count": self.asset_factory_generated_asset_count,
                "provenance_entry_count": self.asset_factory_provenance_entry_count,
                "ui_atlas_count": self.ui_atlas_count,
                "character_bundle_count": self.character_bundle_count,
            }
        });
        serde_json::to_string(&payload).unwrap_or_else(|_| "{\"error\":\"encode\"}".to_string())
    }
}

fn round3(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

fn performance_now_ms() -> f64 {
    js_sys::Date::now()
}

fn percentile(values: &[f64], pct: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let index = ((pct / 100.0) * (sorted.len() as f64)).floor() as usize;
    sorted[index.min(sorted.len().saturating_sub(1))]
}

fn object_set(object: &Object, key: &str, value: JsValue) {
    let _ = Reflect::set(object.as_ref(), &JsValue::from_str(key), &value);
}

fn set_global_object(name: &str, object: &Object) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let _ = Reflect::set(window.as_ref(), &JsValue::from_str(name), object.as_ref());
}

fn config_string(config: &JsValue, key: &str, fallback: &str) -> String {
    Reflect::get(config, &JsValue::from_str(key))
        .ok()
        .and_then(|value| value.as_string())
        .unwrap_or_else(|| fallback.to_string())
}

fn config_bool(config: &JsValue, key: &str) -> Option<bool> {
    Reflect::get(config, &JsValue::from_str(key))
        .ok()
        .and_then(|value| value.as_bool())
}

fn window_required() -> Result<web_sys::Window, JsValue> {
    web_sys::window().ok_or_else(|| JsValue::from_str("window is unavailable"))
}

fn js_error_to_string(error: JsValue) -> String {
    error
        .as_string()
        .unwrap_or_else(|| "operation failed with non-string js error".to_string())
}

fn resolve_dist_asset_url(dist_root: &str, file_name: &str) -> String {
    let trimmed = dist_root.trim();
    if trimmed.is_empty() || trimmed == "." {
        return file_name.to_string();
    }
    if trimmed.ends_with('/') {
        return format!("{trimmed}{file_name}");
    }
    format!("{trimmed}/{file_name}")
}

async fn fetch_text_asset(url: &str, label: &str) -> Result<String, String> {
    let window = web_sys::window().ok_or_else(|| "window is unavailable".to_string())?;
    let response_js = JsFuture::from(window.fetch_with_str(url))
        .await
        .map_err(|error| {
            format!(
                "failed to fetch {label} at '{url}': {}",
                js_error_to_string(error)
            )
        })?;
    let response: Response = response_js
        .dyn_into::<Response>()
        .map_err(|_| format!("fetch for {label} at '{url}' returned a non-Response value"))?;
    if !response.ok() {
        return Err(format!(
            "failed to fetch {label} at '{url}': HTTP {} {}",
            response.status(),
            response.status_text()
        ));
    }

    let text_js = JsFuture::from(response.text().map_err(|error| {
        format!(
            "failed to read {label} text at '{url}': {}",
            js_error_to_string(error)
        )
    })?)
    .await
    .map_err(|error| {
        format!(
            "failed to resolve text body for {label} at '{url}': {}",
            js_error_to_string(error)
        )
    })?;

    text_js
        .as_string()
        .ok_or_else(|| format!("{label} at '{url}' did not decode to a UTF-8 string"))
}

async fn load_and_validate_asset_pack_manifests(
    dist_root: &str,
) -> Result<AssetManifestLoadSummary, String> {
    let asset_pack_manifest_url = resolve_dist_asset_url(dist_root, ASSET_PACK_MANIFEST_FILE);
    let world_chunk_manifest_url = resolve_dist_asset_url(dist_root, WORLD_CHUNK_MANIFEST_FILE);

    let (asset_pack_manifest_text, world_chunk_manifest_text) = futures::future::try_join(
        fetch_text_asset(asset_pack_manifest_url.as_str(), "asset pack manifest"),
        fetch_text_asset(world_chunk_manifest_url.as_str(), "world chunk manifest"),
    )
    .await?;

    parse_and_validate_asset_pack_manifests_from_json(
        asset_pack_manifest_text.as_str(),
        world_chunk_manifest_text.as_str(),
        asset_pack_manifest_url.as_str(),
        world_chunk_manifest_url.as_str(),
    )
}

async fn load_and_validate_asset_factory_manifests(
    dist_root: &str,
) -> Result<AssetFactoryManifestLoadSummary, String> {
    let factory_manifest_url = resolve_dist_asset_url(dist_root, ASSET_FACTORY_MANIFEST_FILE);
    let provenance_manifest_url = resolve_dist_asset_url(dist_root, ASSET_PROVENANCE_LEDGER_FILE);
    let quality_manifest_url = resolve_dist_asset_url(dist_root, ASSET_QUALITY_REPORT_FILE);
    let ui_manifest_url = resolve_dist_asset_url(dist_root, UI_ATLAS_MANIFEST_FILE);
    let character_manifest_url = resolve_dist_asset_url(dist_root, CHARACTER_BUNDLE_MANIFEST_FILE);
    let animation_rig_catalog_url = resolve_dist_asset_url(dist_root, ANIMATION_RIG_CATALOG_FILE);
    let animation_clip_bundle_url = resolve_dist_asset_url(dist_root, ANIMATION_CLIP_BUNDLE_FILE);
    let animation_graph_contract_url =
        resolve_dist_asset_url(dist_root, ANIMATION_GRAPH_CONTRACT_FILE);
    let flora_sim_contract_url = resolve_dist_asset_url(dist_root, FLORA_SIM_CONTRACT_FILE);
    let animation_quality_report_url =
        resolve_dist_asset_url(dist_root, ANIMATION_QUALITY_REPORT_FILE);

    let (
        factory_manifest_text,
        provenance_manifest_text,
        quality_manifest_text,
        ui_manifest_text,
        character_manifest_text,
    ) = futures::future::try_join5(
        fetch_text_asset(factory_manifest_url.as_str(), "asset factory manifest"),
        fetch_text_asset(provenance_manifest_url.as_str(), "asset provenance ledger"),
        fetch_text_asset(quality_manifest_url.as_str(), "asset quality report"),
        fetch_text_asset(ui_manifest_url.as_str(), "ui atlas manifest"),
        fetch_text_asset(character_manifest_url.as_str(), "character bundle manifest"),
    )
    .await?;
    let (
        animation_rig_catalog_text,
        animation_clip_bundle_text,
        animation_graph_contract_text,
        flora_sim_contract_text,
        animation_quality_report_text,
    ) = futures::future::try_join5(
        fetch_text_asset(animation_rig_catalog_url.as_str(), "animation rig catalog"),
        fetch_text_asset(animation_clip_bundle_url.as_str(), "animation clip bundle"),
        fetch_text_asset(
            animation_graph_contract_url.as_str(),
            "animation graph contract",
        ),
        fetch_text_asset(flora_sim_contract_url.as_str(), "flora sim contract"),
        fetch_text_asset(
            animation_quality_report_url.as_str(),
            "animation quality report",
        ),
    )
    .await?;

    let summary = parse_and_validate_asset_factory_manifests_from_json(
        factory_manifest_text.as_str(),
        provenance_manifest_text.as_str(),
        quality_manifest_text.as_str(),
        ui_manifest_text.as_str(),
        character_manifest_text.as_str(),
        factory_manifest_url.as_str(),
        provenance_manifest_url.as_str(),
        quality_manifest_url.as_str(),
        ui_manifest_url.as_str(),
        character_manifest_url.as_str(),
    )?;
    parse_and_validate_animation_manifests_from_json(
        animation_rig_catalog_text.as_str(),
        animation_clip_bundle_text.as_str(),
        animation_graph_contract_text.as_str(),
        flora_sim_contract_text.as_str(),
        animation_quality_report_text.as_str(),
        animation_rig_catalog_url.as_str(),
        animation_clip_bundle_url.as_str(),
        animation_graph_contract_url.as_str(),
        flora_sim_contract_url.as_str(),
        animation_quality_report_url.as_str(),
    )?;
    Ok(summary)
}

fn map_primitive_state_for_webgpu(
    topology: RenderPrimitiveTopology,
    cull_mode: RenderCullMode,
) -> WebGpuPrimitiveState {
    let topology = match topology {
        RenderPrimitiveTopology::Triangles => wgpu::PrimitiveTopology::TriangleList,
        RenderPrimitiveTopology::TriangleStrip => wgpu::PrimitiveTopology::TriangleStrip,
        RenderPrimitiveTopology::Lines => wgpu::PrimitiveTopology::LineList,
        RenderPrimitiveTopology::LineStrip => wgpu::PrimitiveTopology::LineStrip,
        RenderPrimitiveTopology::Points => wgpu::PrimitiveTopology::PointList,
    };
    let cull_mode = match cull_mode {
        RenderCullMode::None => None,
        RenderCullMode::BackFace => Some(wgpu::Face::Back),
        RenderCullMode::FrontFace => Some(wgpu::Face::Front),
    };
    WebGpuPrimitiveState {
        topology,
        cull_mode,
    }
}

fn ensure_shader_contains_entrypoint(
    shader_source: &str,
    entry: &str,
    stage: &str,
) -> Result<(), String> {
    let needle = format!("fn {entry}");
    if shader_source.contains(needle.as_str()) {
        return Ok(());
    }
    Err(format!(
        "shader source is missing {stage} entrypoint '{entry}'"
    ))
}

async fn load_runtime_shader_assets(dist_root: &str) -> Result<RuntimeShaderAssets, String> {
    let render_manifest_url = resolve_dist_asset_url(dist_root, RENDER_MANIFEST_FILE);
    let shader_bundle_url = resolve_dist_asset_url(dist_root, SHADER_BUNDLE_FILE);

    let render_manifest_text =
        fetch_text_asset(render_manifest_url.as_str(), "render manifest").await?;
    let shader_bundle_text = fetch_text_asset(shader_bundle_url.as_str(), "shader bundle").await?;

    let render_manifest: RenderManifestDocument =
        serde_json::from_str(render_manifest_text.as_str()).map_err(|error| {
            format!(
                "invalid render manifest JSON at '{}': {error}",
                render_manifest_url
            )
        })?;
    let shader_bundle: ShaderBundleDocument = serde_json::from_str(shader_bundle_text.as_str())
        .map_err(|error| {
            format!(
                "invalid shader bundle JSON at '{}': {error}",
                shader_bundle_url
            )
        })?;

    let selection = resolve_runtime_shader_selection(&render_manifest, &shader_bundle)
        .map_err(|error| format!("render/shader manifest resolution failed: {error}"))?;
    let mut shader_source_cache = Vec::<(String, String)>::new();
    let mut pipelines = Vec::with_capacity(selection.pipelines.len());
    for pipeline in &selection.pipelines {
        let shader_source = match shader_source_cache
            .iter()
            .find(|(path, _)| path == &pipeline.shader_path)
        {
            Some((_, source)) => source.clone(),
            None => {
                let shader_url = resolve_dist_asset_url(dist_root, pipeline.shader_path.as_str());
                let source = fetch_text_asset(shader_url.as_str(), "shader module").await?;
                if source.trim().is_empty() {
                    return Err(format!(
                        "shader module '{}' loaded from '{}' was empty",
                        pipeline.shader_module_id, shader_url
                    ));
                }
                shader_source_cache.push((pipeline.shader_path.clone(), source.clone()));
                source
            }
        };

        pipelines.push(map_pipeline_shader_asset(pipeline, shader_source)?);
    }

    let frame_graph = selection
        .frame_graph
        .iter()
        .map(map_frame_graph_pass_asset)
        .collect::<Vec<_>>();
    let prewarm_groups = selection
        .prewarm_groups
        .iter()
        .map(map_prewarm_group_asset)
        .collect::<Vec<_>>();

    Ok(RuntimeShaderAssets {
        render_schema_version: selection.render_schema_version,
        shader_bundle_schema_version: selection.shader_bundle_schema_version,
        pipelines,
        frame_graph,
        prewarm_groups,
        gpu_scene_buffers: selection.gpu_scene_buffers,
        default_profile_contracts: selection.default_profile_contracts,
        compute_pass_manifest_ready: selection.compute_pass_manifest_ready,
    })
}

async fn load_protocol_contract(dist_root: &str) -> Result<ProtocolContract, String> {
    let protocol_url = resolve_dist_asset_url(dist_root, PROTOCOL_METADATA_FILE);
    let payload_text = fetch_text_asset(protocol_url.as_str(), "protocol metadata").await?;
    parse_protocol_contract(payload_text.as_str())
        .map_err(|error| format!("invalid protocol metadata at '{}': {error}", protocol_url))
}

fn map_pipeline_shader_asset(
    pipeline: &RuntimePipelineShaderSelection,
    shader_source: String,
) -> Result<RuntimePipelineShaderAssets, String> {
    ensure_shader_contains_entrypoint(
        shader_source.as_str(),
        pipeline.vertex_entry.as_str(),
        "vertex",
    )?;
    ensure_shader_contains_entrypoint(
        shader_source.as_str(),
        pipeline.fragment_entry.as_str(),
        "fragment",
    )?;
    Ok(RuntimePipelineShaderAssets {
        pipeline_id: pipeline.pipeline_id.clone(),
        shader_module_id: pipeline.shader_module_id.clone(),
        shader_path: pipeline.shader_path.clone(),
        node_target: pipeline.node_target.clone(),
        shader_mode: pipeline.shader_mode.clone(),
        shader_source,
        vertex_entry: pipeline.vertex_entry.clone(),
        fragment_entry: pipeline.fragment_entry.clone(),
        primitive_topology: pipeline.topology,
        primitive_cull_mode: pipeline.cull_mode,
    })
}

fn map_frame_graph_pass_asset(
    pass: &RuntimeFrameGraphPassSelection,
) -> RuntimeFrameGraphPassAssets {
    RuntimeFrameGraphPassAssets {
        name: pass.name.clone(),
        pipeline_id: pass.pipeline_id.clone(),
        draw_phase: pass.draw_phase.clone(),
        depends_on: pass.depends_on.clone(),
        pass_contract_id: pass.pass_contract_id.clone(),
        reads: pass.reads.clone(),
        writes: pass.writes.clone(),
        is_compute_pass: pass.is_compute_pass,
    }
}

fn map_prewarm_group_asset(group: &RuntimePrewarmGroupSelection) -> RuntimePrewarmGroupAssets {
    RuntimePrewarmGroupAssets {
        id: group.id.clone(),
        required: group.required,
        shader_modules: group.shader_modules.clone(),
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct CombatKeyMapping {
    attack_light: bool,
    attack_heavy: bool,
    dodge: bool,
    parry: bool,
    lock_on_toggle: bool,
    target_cycle_left: bool,
    target_cycle_right: bool,
    restart: bool,
}

fn map_combat_key_mapping(key: &str, lock_on_active: bool) -> Option<CombatKeyMapping> {
    let mapping = match key {
        "j" | "J" => CombatKeyMapping {
            attack_light: true,
            ..CombatKeyMapping::default()
        },
        "k" | "K" => CombatKeyMapping {
            attack_heavy: true,
            ..CombatKeyMapping::default()
        },
        " " => CombatKeyMapping {
            dodge: true,
            ..CombatKeyMapping::default()
        },
        "l" | "L" | "Shift" => CombatKeyMapping {
            parry: true,
            ..CombatKeyMapping::default()
        },
        "Tab" => CombatKeyMapping {
            lock_on_toggle: true,
            ..CombatKeyMapping::default()
        },
        "q" | "Q" => CombatKeyMapping {
            target_cycle_left: true,
            ..CombatKeyMapping::default()
        },
        "e" | "E" => CombatKeyMapping {
            target_cycle_right: true,
            ..CombatKeyMapping::default()
        },
        "r" | "R" => CombatKeyMapping {
            restart: true,
            ..CombatKeyMapping::default()
        },
        // Playwright skill aliases
        "Enter" => CombatKeyMapping {
            lock_on_toggle: true,
            attack_heavy: true,
            restart: true,
            ..CombatKeyMapping::default()
        },
        "a" | "A" => CombatKeyMapping {
            attack_light: true,
            target_cycle_left: lock_on_active,
            ..CombatKeyMapping::default()
        },
        "b" | "B" => CombatKeyMapping {
            parry: true,
            target_cycle_right: lock_on_active,
            ..CombatKeyMapping::default()
        },
        _ => return None,
    };
    Some(mapping)
}

fn install_input_handlers(runtime: Rc<RefCell<Runtime>>) -> Result<(), JsValue> {
    let window = window_required()?;
    let canvas = runtime.borrow().canvas.clone();

    let keydown_runtime = runtime.clone();
    let on_keydown = Closure::wrap(Box::new(move |event: KeyboardEvent| {
        let handled = {
            let key = event.key();
            if let Ok(mut runtime) = keydown_runtime.try_borrow_mut() {
                let mut collect_pressed = runtime.collect_pressed;
                let handled = handle_key_input_state(
                    &mut runtime.buttons,
                    &mut collect_pressed,
                    key.as_str(),
                    true,
                );
                runtime.collect_pressed = collect_pressed;
                let lock_on_active = runtime
                    .game_state
                    .as_ref()
                    .map(|state| state.lock_on_target >= 0)
                    .unwrap_or(false);
                let combat_mapping = map_combat_key_mapping(key.as_str(), lock_on_active);
                if let Some(mapping) = combat_mapping {
                    if mapping.attack_light {
                        runtime.game_input.attack_light = true;
                    }
                    if mapping.attack_heavy {
                        runtime.game_input.attack_heavy = true;
                    }
                    if mapping.dodge {
                        runtime.game_input.dodge = true;
                    }
                    if mapping.parry {
                        runtime.game_input.parry = true;
                    }
                    if mapping.lock_on_toggle {
                        runtime.game_input.lock_on_toggle = true;
                    }
                    if mapping.target_cycle_left {
                        runtime.game_input.target_cycle_left = true;
                    }
                    if mapping.target_cycle_right {
                        runtime.game_input.target_cycle_right = true;
                    }
                    if mapping.restart {
                        runtime.hud_restart_pressed = true;
                    }
                }
                let debug_toggle = if key.as_str() == "F1" {
                    if let Some(ref mut renderer) = runtime.renderer {
                        let new_state = !renderer.ssao_system.enabled();
                        renderer.ssao_system.set_enabled(new_state);
                    }
                    true
                } else {
                    false
                };
                handled || combat_mapping.is_some() || debug_toggle
            } else {
                false
            }
        };
        if handled {
            event.prevent_default();
        }
    }) as Box<dyn FnMut(KeyboardEvent)>);
    window.add_event_listener_with_callback("keydown", on_keydown.as_ref().unchecked_ref())?;

    let keyup_runtime = runtime.clone();
    let on_keyup = Closure::wrap(Box::new(move |event: KeyboardEvent| {
        let handled = {
            let key = event.key();
            if let Ok(mut runtime) = keyup_runtime.try_borrow_mut() {
                let mut collect_pressed = runtime.collect_pressed;
                let handled = handle_key_input_state(
                    &mut runtime.buttons,
                    &mut collect_pressed,
                    key.as_str(),
                    false,
                );
                runtime.collect_pressed = collect_pressed;
                // Combat input release for 3D game mode
                let combat_handled = match key.as_str() {
                    "j" | "J" => {
                        runtime.game_input.attack_light = false;
                        true
                    }
                    "k" | "K" => {
                        runtime.game_input.attack_heavy = false;
                        true
                    }
                    "a" | "A" => {
                        // Release both contextual aliases so lock-state changes mid-press
                        // cannot leave either action stuck.
                        runtime.game_input.attack_light = false;
                        runtime.game_input.target_cycle_left = false;
                        true
                    }
                    "b" | "B" => {
                        runtime.game_input.parry = false;
                        runtime.game_input.target_cycle_right = false;
                        true
                    }
                    " " => {
                        runtime.game_input.dodge = false;
                        true
                    }
                    "l" | "L" | "Shift" => {
                        runtime.game_input.parry = false;
                        true
                    }
                    "Tab" => {
                        runtime.game_input.lock_on_toggle = false;
                        true
                    }
                    "Enter" => {
                        runtime.game_input.lock_on_toggle = false;
                        runtime.game_input.attack_heavy = false;
                        true
                    }
                    "q" | "Q" => {
                        runtime.game_input.target_cycle_left = false;
                        true
                    }
                    "e" | "E" => {
                        runtime.game_input.target_cycle_right = false;
                        true
                    }
                    _ => false,
                };
                handled || combat_handled
            } else {
                false
            }
        };
        if handled {
            event.prevent_default();
        }
    }) as Box<dyn FnMut(KeyboardEvent)>);
    window.add_event_listener_with_callback("keyup", on_keyup.as_ref().unchecked_ref())?;

    let pointerdown_runtime = runtime.clone();
    let on_pointerdown = Closure::wrap(Box::new(move |_event: Event| {
        if let Ok(mut runtime) = pointerdown_runtime.try_borrow_mut() {
            runtime.collect_pressed = true;
        }
    }) as Box<dyn FnMut(Event)>);
    canvas
        .add_event_listener_with_callback("pointerdown", on_pointerdown.as_ref().unchecked_ref())?;

    let pointerup_runtime = runtime.clone();
    let on_pointerup = Closure::wrap(Box::new(move |_event: Event| {
        if let Ok(mut runtime) = pointerup_runtime.try_borrow_mut() {
            runtime.collect_pressed = false;
        }
    }) as Box<dyn FnMut(Event)>);
    canvas.add_event_listener_with_callback("pointerup", on_pointerup.as_ref().unchecked_ref())?;

    let mut runtime_mut = runtime.borrow_mut();
    runtime_mut.on_keydown = Some(on_keydown);
    runtime_mut.on_keyup = Some(on_keyup);
    runtime_mut.on_pointerdown = Some(on_pointerdown);
    runtime_mut.on_pointerup = Some(on_pointerup);
    Ok(())
}

// ---------------------------------------------------------------------------
// Preview mode: mouse input and WebSocket
// ---------------------------------------------------------------------------

fn install_preview_input_handlers(runtime: Rc<RefCell<Runtime>>) -> Result<(), JsValue> {
    let canvas = runtime.borrow().canvas.clone();

    let md_runtime = runtime.clone();
    let on_mousedown = Closure::wrap(Box::new(move |event: web_sys::MouseEvent| {
        if let Ok(mut rt) = md_runtime.try_borrow_mut() {
            if let Some(ref mut ps) = rt.preview_state {
                ps.on_mouse_down(event.client_x() as f32, event.client_y() as f32);
            }
        }
    }) as Box<dyn FnMut(web_sys::MouseEvent)>);
    canvas.add_event_listener_with_callback("mousedown", on_mousedown.as_ref().unchecked_ref())?;

    let mu_runtime = runtime.clone();
    let on_mouseup = Closure::wrap(Box::new(move |_event: web_sys::MouseEvent| {
        if let Ok(mut rt) = mu_runtime.try_borrow_mut() {
            if let Some(ref mut ps) = rt.preview_state {
                ps.on_mouse_up();
            }
        }
    }) as Box<dyn FnMut(web_sys::MouseEvent)>);
    canvas.add_event_listener_with_callback("mouseup", on_mouseup.as_ref().unchecked_ref())?;

    let mm_runtime = runtime.clone();
    let on_mousemove = Closure::wrap(Box::new(move |event: web_sys::MouseEvent| {
        if let Ok(mut rt) = mm_runtime.try_borrow_mut() {
            if let Some(ref mut ps) = rt.preview_state {
                ps.on_mouse_move(event.client_x() as f32, event.client_y() as f32);
            }
        }
    }) as Box<dyn FnMut(web_sys::MouseEvent)>);
    canvas.add_event_listener_with_callback("mousemove", on_mousemove.as_ref().unchecked_ref())?;

    let wh_runtime = runtime.clone();
    let on_wheel = Closure::wrap(Box::new(move |event: web_sys::WheelEvent| {
        event.prevent_default();
        if let Ok(mut rt) = wh_runtime.try_borrow_mut() {
            if let Some(ref mut ps) = rt.preview_state {
                ps.on_wheel(event.delta_y() as f32);
            }
        }
    }) as Box<dyn FnMut(web_sys::WheelEvent)>);
    {
        let wheel_opts = web_sys::AddEventListenerOptions::new();
        wheel_opts.set_passive(false);
        canvas.add_event_listener_with_callback_and_add_event_listener_options(
            "wheel",
            on_wheel.as_ref().unchecked_ref(),
            &wheel_opts,
        )?;
    }

    let mut runtime_mut = runtime.borrow_mut();
    runtime_mut.on_preview_mousedown = Some(on_mousedown);
    runtime_mut.on_preview_mouseup = Some(on_mouseup);
    runtime_mut.on_preview_mousemove = Some(on_mousemove);
    runtime_mut.on_preview_wheel = Some(on_wheel);
    Ok(())
}

fn connect_preview_socket(runtime: Rc<RefCell<Runtime>>) -> Result<(), JsValue> {
    let window = window_required()?;
    let location = window.location();
    let scheme = if location.protocol()?.eq_ignore_ascii_case("https:") {
        "wss"
    } else {
        "ws"
    };
    let ws_url = format!("{scheme}://{}/preview", location.host()?);
    let ws = WebSocket::new(ws_url.as_str())?;
    // Preview uses TEXT messages (JSON), not binary
    ws.set_binary_type(BinaryType::Arraybuffer);

    let open_runtime = runtime.clone();
    let on_ws_open = Closure::wrap(Box::new(move |_event: Event| {
        if let Ok(mut rt) = open_runtime.try_borrow_mut() {
            rt.set_status("Preview WebSocket connected.");
        }
    }) as Box<dyn FnMut(Event)>);
    ws.set_onopen(Some(on_ws_open.as_ref().unchecked_ref()));

    let close_runtime = runtime.clone();
    let on_ws_close = Closure::wrap(Box::new(move |_event: Event| {
        if let Ok(mut rt) = close_runtime.try_borrow_mut() {
            rt.set_status("Preview WebSocket disconnected.");
        }
    }) as Box<dyn FnMut(Event)>);
    ws.set_onclose(Some(on_ws_close.as_ref().unchecked_ref()));

    let error_runtime = runtime.clone();
    let on_ws_error = Closure::wrap(Box::new(move |_event: Event| {
        if let Ok(mut rt) = error_runtime.try_borrow_mut() {
            rt.set_status("Preview WebSocket error.");
        }
    }) as Box<dyn FnMut(Event)>);
    ws.set_onerror(Some(on_ws_error.as_ref().unchecked_ref()));

    let message_runtime = runtime.clone();
    let on_ws_message = Closure::wrap(Box::new(move |event: MessageEvent| {
        // Preview uses text messages (JSON)
        let text = match event.data().as_string() {
            Some(s) => s,
            None => return,
        };
        if let Ok(mut rt) = message_runtime.try_borrow_mut() {
            if let Some(ref mut ps) = rt.preview_state {
                ps.handle_message(&text);
            }
        }
    }) as Box<dyn FnMut(MessageEvent)>);
    ws.set_onmessage(Some(on_ws_message.as_ref().unchecked_ref()));

    let mut runtime_mut = runtime.borrow_mut();
    runtime_mut.ws = Some(ws);
    runtime_mut.on_ws_open = Some(on_ws_open);
    runtime_mut.on_ws_close = Some(on_ws_close);
    runtime_mut.on_ws_error = Some(on_ws_error);
    runtime_mut.on_ws_message = Some(on_ws_message);
    Ok(())
}

fn connect_socket(runtime: Rc<RefCell<Runtime>>) -> Result<(), JsValue> {
    let window = window_required()?;
    let location = window.location();
    let scheme = if location.protocol()?.eq_ignore_ascii_case("https:") {
        "wss"
    } else {
        "ws"
    };
    let ws_url = format!("{scheme}://{}/ws", location.host()?);
    let ws = WebSocket::new(ws_url.as_str())?;
    ws.set_binary_type(BinaryType::Arraybuffer);

    let open_runtime = runtime.clone();
    let on_ws_open = Closure::wrap(Box::new(move |_event: Event| {
        if let Ok(mut runtime) = open_runtime.try_borrow_mut() {
            runtime.set_status(
                "Connected. wasm runtime handles protocol/input/prediction/render loop.",
            );
        }
    }) as Box<dyn FnMut(Event)>);
    ws.set_onopen(Some(on_ws_open.as_ref().unchecked_ref()));

    let close_runtime = runtime.clone();
    let on_ws_close = Closure::wrap(Box::new(move |_event: Event| {
        if let Ok(mut runtime) = close_runtime.try_borrow_mut() {
            runtime.set_status("Disconnected from authority server.");
        }
    }) as Box<dyn FnMut(Event)>);
    ws.set_onclose(Some(on_ws_close.as_ref().unchecked_ref()));

    let error_runtime = runtime.clone();
    let on_ws_error = Closure::wrap(Box::new(move |_event: Event| {
        if let Ok(mut runtime) = error_runtime.try_borrow_mut() {
            runtime.set_status("WebSocket transport error.");
        }
    }) as Box<dyn FnMut(Event)>);
    ws.set_onerror(Some(on_ws_error.as_ref().unchecked_ref()));

    let message_runtime = runtime.clone();
    let on_ws_message = Closure::wrap(Box::new(move |event: MessageEvent| {
        let bytes = match event.data().dyn_into::<js_sys::ArrayBuffer>() {
            Ok(buffer) => Uint8Array::new(&buffer).to_vec(),
            Err(_) => return,
        };

        let envelope = match Envelope::decode(bytes.as_slice()) {
            Ok(envelope) => envelope,
            Err(error) => {
                if let Ok(mut runtime) = message_runtime.try_borrow_mut() {
                    runtime.set_status(format!("Envelope decode failed: {error}"));
                }
                return;
            }
        };

        if envelope.version != PROTOCOL_V5 || envelope.sub_version != PROTOCOL_V5_SUB_VERSION {
            if let Ok(mut runtime) = message_runtime.try_borrow_mut() {
                runtime.set_status(format!(
                    "Protocol v5 mismatch: got version={} sub_version={}, expected version={} sub_version={} (protocol-v5.json)",
                    envelope.version,
                    envelope.sub_version,
                    PROTOCOL_V5,
                    PROTOCOL_V5_SUB_VERSION
                ));
            }
            return;
        }

        let payload_text = String::from_utf8_lossy(envelope.payload.as_slice()).to_string();

        if let Ok(mut runtime) = message_runtime.try_borrow_mut() {
            runtime.partition_id = envelope.partition_id;
            runtime.session_id = envelope.session_id;
            runtime.actor_id = envelope.actor_id;
            runtime.publish_runtime_state();

            match envelope.message_type {
                MessageTypeV5::HelloV5 => {
                    match serde_json::from_slice::<HelloPayload>(&envelope.payload) {
                        Ok(payload) => runtime.on_hello(payload),
                        Err(error) => {
                            runtime.set_status(format!("Invalid hello payload: {error}"));
                        }
                    }
                }
                MessageTypeV5::SnapshotV5 => {
                    let payload = serde_json::from_slice::<ServerStatePayload>(&envelope.payload)
                        .unwrap_or_default();
                    runtime.apply_server_state_payload(payload, false);
                }
                MessageTypeV5::DeltaV5 => {
                    let payload = serde_json::from_slice::<ServerStatePayload>(&envelope.payload)
                        .unwrap_or_default();
                    runtime.apply_server_state_payload(payload, false);
                }
                MessageTypeV5::CorrectionV5 => {
                    let payload = serde_json::from_slice::<ServerStatePayload>(&envelope.payload)
                        .unwrap_or_default();
                    runtime.apply_server_state_payload(payload, true);
                }
                MessageTypeV5::ErrorV5 => {
                    let parsed = serde_json::from_slice::<ErrorPayload>(&envelope.payload)
                        .unwrap_or_else(|_| ErrorPayload {
                            code: None,
                            message: None,
                        });
                    let detail = parsed
                        .message
                        .or(parsed.code)
                        .unwrap_or_else(|| payload_text.clone());
                    runtime.set_status(format!("Authority error: {detail}"));
                }
                _ => {}
            }
        }
    }) as Box<dyn FnMut(MessageEvent)>);
    ws.set_onmessage(Some(on_ws_message.as_ref().unchecked_ref()));

    let mut runtime_mut = runtime.borrow_mut();
    runtime_mut.ws = Some(ws);
    runtime_mut.on_ws_open = Some(on_ws_open);
    runtime_mut.on_ws_close = Some(on_ws_close);
    runtime_mut.on_ws_error = Some(on_ws_error);
    runtime_mut.on_ws_message = Some(on_ws_message);
    Ok(())
}

fn start_frame_loop(runtime: Rc<RefCell<Runtime>>) -> Result<(), JsValue> {
    let window = window_required()?;
    let loop_cell: Rc<RefCell<Option<Closure<dyn FnMut(f64)>>>> = Rc::new(RefCell::new(None));
    let loop_cell_clone = loop_cell.clone();
    let loop_runtime = runtime.clone();

    *loop_cell.borrow_mut() = Some(Closure::wrap(Box::new(move |now_ms: f64| {
        if let Ok(mut runtime) = loop_runtime.try_borrow_mut() {
            if !runtime.deterministic_time_driver_enabled {
                runtime.run_runtime_step(now_ms);
            } else {
                runtime.publish_runtime_state();
                runtime.publish_metrics();
            }
        }

        if let Some(window) = web_sys::window() {
            if let Some(callback) = loop_cell_clone.borrow().as_ref() {
                let _ = window.request_animation_frame(callback.as_ref().unchecked_ref());
            }
        }
    }) as Box<dyn FnMut(f64)>));

    if let Some(callback) = loop_cell.borrow().as_ref() {
        window.request_animation_frame(callback.as_ref().unchecked_ref())?;
    }

    runtime.borrow_mut().raf_loop = Some(loop_cell);
    Ok(())
}

fn install_deterministic_hooks(runtime: Rc<RefCell<Runtime>>) -> Result<(), JsValue> {
    let window = window_required()?;

    let render_to_text_runtime = runtime.clone();
    let render_to_text = Closure::wrap(Box::new(move || -> JsValue {
        if let Ok(runtime) = render_to_text_runtime.try_borrow() {
            JsValue::from_str(runtime.render_game_to_text().as_str())
        } else {
            JsValue::from_str("{\"error\":\"runtime_busy\"}")
        }
    }) as Box<dyn FnMut() -> JsValue>);
    Reflect::set(
        window.as_ref(),
        &JsValue::from_str("render_game_to_text"),
        render_to_text.as_ref(),
    )?;

    let advance_runtime = runtime.clone();
    let advance_time = Closure::wrap(Box::new(move |ms: f64| {
        if let Ok(mut runtime) = advance_runtime.try_borrow_mut() {
            runtime.advance_time(ms);
        }
    }) as Box<dyn FnMut(f64)>);
    Reflect::set(
        window.as_ref(),
        &JsValue::from_str("advanceTime"),
        advance_time.as_ref(),
    )?;

    let mut runtime_mut = runtime.borrow_mut();
    runtime_mut.on_render_game_to_text = Some(render_to_text);
    runtime_mut.on_advance_time = Some(advance_time);
    Ok(())
}

async fn bootstrap_webgpu_runtime(runtime: Rc<RefCell<Runtime>>) -> Result<(), String> {
    let (canvas, dist_root, hiz_occlusion_tier_override, is_preview) = {
        let runtime_ref = runtime.borrow();
        (
            runtime_ref.canvas.clone(),
            runtime_ref.dist_root.clone(),
            runtime_ref.hiz_occlusion_tier_override,
            runtime_ref.app_mode == "preview",
        )
    };

    // ── Preview mode: lightweight bootstrap ──────────────────────────
    if is_preview {
        if let Ok(mut runtime_mut) = runtime.try_borrow_mut() {
            runtime_mut.set_status("Booting preview mode WebGPU renderer.");
        }

        let shader_assets = load_runtime_shader_assets(dist_root.as_str()).await?;
        let mut renderer = WebGpuRenderer::new(canvas, shader_assets).await?;
        if let Some(enabled) = hiz_occlusion_tier_override {
            renderer.hiz_occlusion_tier_enabled = enabled;
        }

        // Upload the grid floor mesh
        let grid_mesh_data = generate_preview_grid_plane();
        let grid_base = upload_procedural_meshes(&mut renderer, &[grid_mesh_data]);

        {
            let mut runtime_mut = runtime.borrow_mut();
            let mut preview_state = crate::preview_mode::PreviewState::new();
            preview_state.grid_mesh_index = Some(grid_base);
            runtime_mut.preview_state = Some(preview_state);
            runtime_mut.render_mode_3d = true;
            // Place grid floor as the only initial scene instance
            runtime_mut.scene_3d_instances = vec![MeshInstance {
                mesh_index: grid_base,
                model_matrix: crate::camera_math::mat4_identity(),
            }];
            runtime_mut.player_instance_index = None;
            runtime_mut.enemy_instance_indices.clear();
            runtime_mut.scene_camera_anchor_count = 0;
            runtime_mut.scene_default_camera_anchor_id = None;
            runtime_mut.scene_combat_arena_extents = None;
            runtime_mut.scene_fog_volume_count = 0;
            runtime_mut.scene_lut_profile_id = None;
            runtime_mut.frame_graph = renderer.runtime_evidence();
            runtime_mut.renderer = Some(renderer);
            runtime_mut.sync_canvas_size();
        }

        install_preview_input_handlers(runtime.clone()).map_err(js_error_to_string)?;
        connect_preview_socket(runtime.clone()).map_err(js_error_to_string)?;
        start_frame_loop(runtime.clone()).map_err(js_error_to_string)?;

        if let Ok(mut runtime_mut) = runtime.try_borrow_mut() {
            runtime_mut.set_status("Preview mode ready. Waiting for scene updates.");
        }
        return Ok(());
    }

    // ── Normal mode bootstrap ────────────────────────────────────────
    if let Ok(mut runtime_mut) = runtime.try_borrow_mut() {
        runtime_mut.set_status(format!("{PROTOCOL_BOOT_STATUS} root='{dist_root}'"));
    }

    let protocol_contract = load_protocol_contract(dist_root.as_str()).await?;
    if let Ok(mut runtime_mut) = runtime.try_borrow_mut() {
        runtime_mut.protocol_contract_valid = true;
        runtime_mut.protocol_message_type_count = protocol_contract.message_types.len() as u32;
    }

    if let Ok(mut runtime_mut) = runtime.try_borrow_mut() {
        runtime_mut.set_status(format!("{MANIFEST_BOOT_STATUS} root='{dist_root}'"));
    }

    let asset_manifest_summary = load_and_validate_asset_pack_manifests(dist_root.as_str())
        .await
        .map_err(|error| format!("asset manifest boot failure: {error}"))?;
    let asset_factory_summary = load_and_validate_asset_factory_manifests(dist_root.as_str())
        .await
        .map_err(|error| format!("asset factory manifest boot failure: {error}"))?;
    let shader_assets = load_runtime_shader_assets(dist_root.as_str()).await?;
    let mut renderer = WebGpuRenderer::new(canvas, shader_assets).await?;
    if let Some(enabled) = hiz_occlusion_tier_override {
        renderer.hiz_occlusion_tier_enabled = enabled;
    }

    let enemy_instance_count = runtime
        .borrow()
        .game_state
        .as_ref()
        .map(|state| state.enemy_count.max(1))
        .unwrap_or(DEFAULT_FOREST_ENEMY_INSTANCE_COUNT);

    // Load scene from manifest (environment GLBs + hero GLB + enemy) and build scene instances.
    let scene = load_scene_from_manifest(&mut renderer, dist_root.as_str(), enemy_instance_count)
        .await
        .map_err(|error| format!("Forest scene boot failed: {error}"))?;
    let ForestSceneBuildResult {
        instances,
        player_instance_index,
        enemy_instance_indices,
        camera_anchor_count,
        default_camera_anchor,
        combat_arena_extents,
        fog_volume_count,
        lut_profile_id,
    } = scene;

    {
        let mut runtime_mut = runtime.borrow_mut();
        let cam_offset = [
            default_camera_anchor.position[0] - default_camera_anchor.target[0],
            default_camera_anchor.position[1] - default_camera_anchor.target[1],
            default_camera_anchor.position[2] - default_camera_anchor.target[2],
        ];
        let cam_distance =
            (cam_offset[0] * cam_offset[0] + cam_offset[1] * cam_offset[1] + cam_offset[2] * cam_offset[2])
                .sqrt()
                .clamp(2.5, 40.0);
        let cam_azimuth = cam_offset[0].atan2(cam_offset[2]);
        let cam_elevation = (cam_offset[1] / cam_distance)
            .asin()
            .clamp(
                crate::camera_math::MIN_ELEVATION,
                crate::camera_math::MAX_ELEVATION,
            );

        // Populate the scene with positioned instances now that meshes are loaded
        runtime_mut.scene_3d_instances = instances;
        runtime_mut.player_instance_index = Some(player_instance_index);
        runtime_mut.enemy_instance_indices = enemy_instance_indices;
        runtime_mut.scene_camera_anchor_count = camera_anchor_count;
        runtime_mut.scene_default_camera_anchor_id = Some(default_camera_anchor.id);
        runtime_mut.scene_combat_arena_extents = Some(combat_arena_extents);
        runtime_mut.scene_fog_volume_count = fog_volume_count;
        runtime_mut.scene_lut_profile_id = Some(lut_profile_id);

        runtime_mut.orbit_camera.target = default_camera_anchor.target;
        runtime_mut.orbit_camera.azimuth = cam_azimuth;
        runtime_mut.orbit_camera.elevation = cam_elevation;
        runtime_mut.orbit_camera.distance = cam_distance;
        runtime_mut.orbit_camera.fov_y = default_camera_anchor.fov_y_radians;
        runtime_mut.base_fov_y = default_camera_anchor.fov_y_radians;

        runtime_mut.streaming.loaded_chunk_count = asset_manifest_summary.loaded_chunk_count;
        runtime_mut.streaming.loaded_bytes = asset_manifest_summary.loaded_bytes;
        runtime_mut.streaming.residency_pressure =
            (asset_manifest_summary.loaded_chunk_count as f64 / 512.0).min(1.0);
        runtime_mut.streaming.chunk_hit = asset_manifest_summary.loaded_chunk_count;
        runtime_mut.streaming.chunk_miss = 0;
        runtime_mut.streaming.convergence_stage = RuntimeConvergenceStage::Bootstrap;
        runtime_mut.streaming.residency_class = infer_target_residency_class(
            runtime_mut.streaming.convergence_stage,
            runtime_mut.streaming.residency_pressure,
        );
        let initial_residency_class = runtime_mut.streaming.residency_class;
        runtime_mut.emit_residency_adaptation_event(
            0.0,
            "bootstrap-manifest-load",
            RuntimeConvergenceStage::Bootstrap,
            RuntimeConvergenceStage::Bootstrap,
            RuntimeResidencyClass::Cold,
            initial_residency_class,
        );
        runtime_mut.asset_factory_generated_asset_count =
            asset_factory_summary.generated_asset_count;
        runtime_mut.asset_factory_provenance_entry_count =
            asset_factory_summary.provenance_entry_count;
        runtime_mut.ui_atlas_count = asset_factory_summary.ui_atlas_count;
        runtime_mut.character_bundle_count = asset_factory_summary.character_bundle_count;
        runtime_mut.asset_factory_contract_valid = true;
        runtime_mut.frame_graph = renderer.runtime_evidence();
        runtime_mut.refresh_governor_contracts_from_frame_graph();
        runtime_mut.renderer = Some(renderer);
        runtime_mut.set_status(format!(
            "Asset/world/factory manifests validated: pack_chunks={} world_chunks={} bytes={} generated_assets={} ui_atlases={} character_bundles={}",
            asset_manifest_summary.loaded_chunk_count,
            asset_manifest_summary.world_chunk_count,
            asset_manifest_summary.loaded_bytes,
            asset_factory_summary.generated_asset_count,
            asset_factory_summary.ui_atlas_count,
            asset_factory_summary.character_bundle_count
        ));
        runtime_mut.sync_canvas_size();
    }

    connect_socket(runtime.clone()).map_err(js_error_to_string)?;
    start_frame_loop(runtime.clone()).map_err(js_error_to_string)?;

    if let Ok(mut runtime_mut) = runtime.try_borrow_mut() {
        runtime_mut.set_status(READY_BOOT_STATUS);
    }
    Ok(())
}

#[wasm_bindgen]
pub fn start_client(config: JsValue) -> Result<(), JsValue> {
    console_error_panic_hook::set_once();

    APP_RUNTIME.with(|state| {
        if state.borrow().is_some() {
            return Ok(());
        }

        let window = window_required()?;
        let document = window
            .document()
            .ok_or_else(|| JsValue::from_str("document is unavailable"))?;
        let canvas = document
            .get_element_by_id("game-canvas")
            .ok_or_else(|| JsValue::from_str("missing #game-canvas"))?
            .dyn_into::<HtmlCanvasElement>()
            .map_err(|_| JsValue::from_str("#game-canvas is not an HtmlCanvasElement"))?;

        let app_mode = config_string(&config, "appMode", "game");
        let dist_root = config_string(&config, "distRoot", ".");
        let mmo_role = config_string(&config, "mmoRole", DEFAULT_MMO_ROLE);
        let hiz_occlusion_tier_override = config_bool(&config, "hizOcclusionTierEnabled");
        let ready_status_line = config_string(
            &config,
            "readyStatusLine",
            "Authority online. Runtime session is live.",
        );

        let runtime = Rc::new(RefCell::new(Runtime::new(
            app_mode,
            ready_status_line,
            dist_root,
            mmo_role,
            hiz_occlusion_tier_override,
            canvas,
        )));

        {
            let mut runtime_mut = runtime.borrow_mut();
            runtime_mut.sync_canvas_size();
            // Create HUD overlay
            match crate::hud::Hud::create(&document) {
                Ok(hud) => runtime_mut.hud = Some(hud),
                Err(err) => {
                    web_sys::console::warn_1(&JsValue::from_str(&format!(
                        "[wrela] HUD creation failed: {err}"
                    )));
                }
            }
            runtime_mut.publish_runtime_state();
            runtime_mut.publish_metrics();
            runtime_mut.set_status(GPU_BOOT_STATUS);
        }

        install_input_handlers(runtime.clone())?;
        install_deterministic_hooks(runtime.clone())?;
        state.replace(Some(runtime.clone()));

        wasm_bindgen_futures::spawn_local(async move {
            if let Err(error) = bootstrap_webgpu_runtime(runtime.clone()).await {
                if let Ok(mut runtime_mut) = runtime.try_borrow_mut() {
                    runtime_mut.set_status(format!("WebGPU bootstrap failed: {error}"));
                }
            }
        });

        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::{
        FrameGraphRuntimeEvidence, HelloPayload, PredictedState, RuntimeConvergenceStage,
        RuntimeResidencyClass, ServerStatePayload, StateDeltaPayload, apply_predicted_state_delta,
        build_forest_scene, infer_target_convergence_stage, infer_target_residency_class,
        map_combat_key_mapping, normalize_mmo_role, ForestMeshBases,
    };

    #[test]
    fn parse_server_state_payload_accepts_compact_delta_aliases() {
        let payload = r#"{
            "ack": 42,
            "forced_divergence": true,
            "delta": {
                "t": 120,
                "x": 245.5,
                "y": 188.25,
                "s": 3,
                "m": 5
            }
        }"#;

        let parsed: ServerStatePayload =
            serde_json::from_str(payload).expect("compact delta payload should parse");
        assert_eq!(parsed.ack, Some(42));
        assert!(parsed.forced_divergence);

        let delta = parsed.delta.expect("delta should be present");
        assert_eq!(delta.tick, Some(120));
        assert_eq!(delta.player_x, Some(245.5));
        assert_eq!(delta.player_y, Some(188.25));
        assert_eq!(delta.score, Some(3));
        assert_eq!(delta.collected_mask, Some(5));
    }

    #[test]
    fn parse_hello_payload_defaults_role_to_world() {
        let payload = r#"{
            "world_width": 800.0,
            "world_height": 600.0
        }"#;
        let parsed: HelloPayload =
            serde_json::from_str(payload).expect("hello payload without role should parse");
        assert_eq!(normalize_mmo_role(parsed.role.as_str()), "world");
    }

    #[test]
    fn parse_server_state_payload_normalizes_role_labels() {
        let payload = r#"{
            "role": "ZONE",
            "ack": 7
        }"#;
        let parsed: ServerStatePayload =
            serde_json::from_str(payload).expect("server state payload should parse");
        assert_eq!(parsed.ack, Some(7));
        assert_eq!(parsed.role.as_deref().map(normalize_mmo_role), Some("zone"));
    }

    #[test]
    fn apply_predicted_state_delta_updates_only_present_fields() {
        let mut state = PredictedState {
            tick: 10,
            player_x: 400.0,
            player_y: 300.0,
            score: 2,
            collected_mask: 0b0011,
        };
        let delta = StateDeltaPayload {
            tick: Some(11),
            player_x: Some(420.0),
            player_y: None,
            score: Some(4),
            collected_mask: None,
            ..Default::default()
        };

        apply_predicted_state_delta(&mut state, &delta);

        assert_eq!(state.tick, 11);
        assert_eq!(state.player_x, 420.0);
        assert_eq!(state.player_y, 300.0);
        assert_eq!(state.score, 4);
        assert_eq!(state.collected_mask, 0b0011);
    }

    #[test]
    fn convergence_stage_progression_is_monotonic() {
        let mut evidence = FrameGraphRuntimeEvidence::default();
        let mut stage = RuntimeConvergenceStage::Bootstrap;

        stage = infer_target_convergence_stage(stage, &evidence, 2);
        assert_eq!(stage, RuntimeConvergenceStage::Bootstrap);

        evidence.frame_graph_frames_executed = 1;
        stage = infer_target_convergence_stage(stage, &evidence, 2);
        assert_eq!(stage, RuntimeConvergenceStage::Stream);

        evidence.prewarm_required_complete = true;
        evidence.frame_graph_frames_executed = 24;
        stage = infer_target_convergence_stage(stage, &evidence, 2);
        assert_eq!(stage, RuntimeConvergenceStage::Refine);

        evidence.frame_graph_frames_executed = 128;
        stage = infer_target_convergence_stage(stage, &evidence, 2);
        assert_eq!(stage, RuntimeConvergenceStage::Converged);

        evidence.prewarm_required_complete = false;
        evidence.frame_graph_frames_executed = 0;
        stage = infer_target_convergence_stage(stage, &evidence, 2);
        assert_eq!(stage, RuntimeConvergenceStage::Converged);
    }

    #[test]
    fn residency_class_adapts_to_pressure_thresholds() {
        assert_eq!(
            infer_target_residency_class(RuntimeConvergenceStage::Bootstrap, 0.9),
            RuntimeResidencyClass::Core
        );
        assert_eq!(
            infer_target_residency_class(RuntimeConvergenceStage::Stream, 0.7),
            RuntimeResidencyClass::Hot
        );
        assert_eq!(
            infer_target_residency_class(RuntimeConvergenceStage::Refine, 0.2),
            RuntimeResidencyClass::Warm
        );
        assert_eq!(
            infer_target_residency_class(RuntimeConvergenceStage::Bootstrap, 0.2),
            RuntimeResidencyClass::Cold
        );
    }

    #[test]
    fn combat_key_mapping_supports_playwright_aliases() {
        let enter_mapping =
            map_combat_key_mapping("Enter", false).expect("enter alias should map");
        assert!(enter_mapping.lock_on_toggle);
        assert!(enter_mapping.attack_heavy);
        assert!(enter_mapping.restart);

        let a_explore = map_combat_key_mapping("a", false).expect("a alias should map");
        assert!(a_explore.attack_light);
        assert!(!a_explore.target_cycle_left);

        let a_locked = map_combat_key_mapping("a", true).expect("a alias should map");
        assert!(a_locked.attack_light);
        assert!(a_locked.target_cycle_left);

        let b_explore = map_combat_key_mapping("b", false).expect("b alias should map");
        assert!(b_explore.parry);
        assert!(!b_explore.target_cycle_right);

        let b_locked = map_combat_key_mapping("b", true).expect("b alias should map");
        assert!(b_locked.parry);
        assert!(b_locked.target_cycle_right);
    }

    #[test]
    fn forest_scene_build_spawns_requested_enemy_lane_count() {
        let bases = ForestMeshBases {
            ground: 0,
            tree_trunk: 1,
            tree_foliage: 2,
            rock: 3,
            player: 4,
            enemy: 5,
        };
        let scene = build_forest_scene(&bases, 3);
        assert_eq!(scene.enemy_instance_indices.len(), 3);
        assert!(scene.enemy_instance_indices.iter().all(|idx| *idx > scene.player_instance_index));
    }
}
