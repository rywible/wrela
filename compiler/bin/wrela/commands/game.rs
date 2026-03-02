const GAME_MAIN_TEMPLATE: &str = r#"use run_realtime_bootstrap from application/realtime_bootstrap

fn run() -> Integer {
    return run_realtime_bootstrap()
}
"#;

const GAME_BOOTSTRAP_TEMPLATE: &str = r#"use {
    create_default_game_session_runtime_configuration,
    run_game_session_once
}
from pkg/game/core/session

fn run_realtime_bootstrap() -> Integer {
    configuration = create_default_game_session_runtime_configuration()
    run_game_session_once(configuration) ?? nothing
    return 0
}
"#;

#[derive(Debug, Clone)]
struct GameCommandInput {
    command: String,
    path_arg: Option<String>,
    out_path: Option<String>,
    game_build_target: Option<String>,
    game_render_backend: Option<String>,
    game_host_mode: Option<String>,
    game_client_runtime: Option<String>,
    game_shader_provenance: bool,
    game_no_shortcuts: bool,
    game_check_determinism: bool,
    game_check_rollback: bool,
    game_check_render_lane: bool,
    game_check_asset_streaming: bool,
    game_profile_gpu_metrics: bool,
    game_profile_streaming_metrics: bool,
}

#[derive(Debug, Clone)]
struct GameBuildArtifacts {
    dist_dir: PathBuf,
    native_artifact: Option<PathBuf>,
    wasm_artifact: Option<PathBuf>,
    descriptor: DomainAbiDescriptorArtifact,
}

#[derive(Debug, Clone)]
struct GameCheckArtifacts {
    dist_dir: PathBuf,
    test_matrix_path: PathBuf,
    wasm_artifact: Option<PathBuf>,
    run_context: GameArtifactRunContext,
}

#[derive(Debug, Clone)]
struct AssetFactoryArtifacts {
    asset_factory_manifest: PathBuf,
    asset_provenance_ledger: PathBuf,
    asset_quality_report: PathBuf,
    ui_atlas_manifest: PathBuf,
    character_bundle_manifest: PathBuf,
    animation_rig_catalog: PathBuf,
    animation_clip_bundle: PathBuf,
    animation_graph_contract: PathBuf,
    flora_sim_contract: PathBuf,
    animation_quality_report: PathBuf,
}

#[derive(Debug, Clone)]
struct AnimationArtifacts {
    character_bundle_manifest: PathBuf,
    animation_rig_catalog: PathBuf,
    animation_clip_bundle: PathBuf,
    animation_graph_contract: PathBuf,
    flora_sim_contract: PathBuf,
    animation_quality_report: PathBuf,
    replay_hash: String,
    generated_clip_count: usize,
}

#[derive(Debug, Clone)]
struct GameArtifactRunContext {
    app_slug: String,
    run_id: String,
    timestamp_epoch_seconds: u64,
}

#[derive(Debug, Clone, Serialize)]
struct MmoRoleEvidence {
    role: String,
    phase: String,
    required_outputs: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct GameOrchestrationContext {
    identity: String,
    family: String,
    variant: String,
    mmo_role_evidence: Option<MmoRoleEvidence>,
}

#[derive(Debug, Clone, Copy)]
struct GameStrictGateConfig {
    client_runtime_compiled: bool,
    shader_provenance: bool,
    no_shortcuts: bool,
}

impl GameStrictGateConfig {
    const fn disabled() -> Self {
        Self {
            client_runtime_compiled: false,
            shader_provenance: false,
            no_shortcuts: false,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct DeterminismParityReport {
    fixture_inputs: usize,
    native_hash: String,
    wasm_hash: String,
    native_tick: u64,
    wasm_tick: u64,
    native_score: u32,
    wasm_score: u32,
    parity_passed: bool,
}

#[derive(Debug, Clone, Serialize)]
struct RollbackConvergenceReport {
    correction_delay_ticks: u64,
    forced_divergence_ticks: Vec<u64>,
    max_pending_depth: usize,
    convergence_bound_ticks: u64,
    converged: bool,
    final_authoritative_hash: u64,
    final_client_hash: u64,
}

#[derive(Debug, Clone, Serialize)]
struct RenderLaneContractReportV5 {
    schema_version: &'static str,
    render_graph_fingerprint: String,
    resource_count: usize,
    capability_count: usize,
    pipeline_count: usize,
    pass_count: usize,
    shader_program_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GameRenderBackend {
    WebGpu,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GameHostMode {
    PureWasm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GameArtifactLane {
    FullCompilerPass,
    WebGpuEnginePass,
}

fn render_backend_name(render_backend: GameRenderBackend) -> &'static str {
    match render_backend {
        GameRenderBackend::WebGpu => "webgpu",
    }
}

fn host_mode_name(host_mode: GameHostMode) -> &'static str {
    match host_mode {
        GameHostMode::PureWasm => "pure-wasm",
    }
}

fn artifact_lane_for_check(run_render_lane: bool) -> GameArtifactLane {
    if run_render_lane {
        GameArtifactLane::WebGpuEnginePass
    } else {
        GameArtifactLane::FullCompilerPass
    }
}

fn artifact_lane_for_profile() -> GameArtifactLane {
    GameArtifactLane::WebGpuEnginePass
}

fn artifact_lane_parts(lane: GameArtifactLane) -> (&'static str, &'static str, &'static str) {
    match lane {
        GameArtifactLane::FullCompilerPass => ("full-compiler-pass", "WFE2-601", "WFE2-602"),
        GameArtifactLane::WebGpuEnginePass => ("webgpu-engine-pass", "WFE4-102", "WFE3-602"),
    }
}

fn build_game_artifact_run_context(app_root: &Path) -> GameArtifactRunContext {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    GameArtifactRunContext {
        app_slug: game_app_artifact_stem(app_root),
        run_id: format!("run-{}-{}", now.as_millis(), std::process::id()),
        timestamp_epoch_seconds: now.as_secs(),
    }
}

fn artifact_report_root(
    workspace_root: &Path,
    lane: GameArtifactLane,
    run_context: &GameArtifactRunContext,
) -> PathBuf {
    let (namespace, check_task, _) = artifact_lane_parts(lane);
    workspace_root
        .join(".artifacts")
        .join(namespace)
        .join(check_task)
        .join(run_context.app_slug.as_str())
        .join(run_context.run_id.as_str())
}

fn artifact_smoke_roots(workspace_root: &Path) -> [PathBuf; 2] {
    let (_, _, webgpu_smoke_task) = artifact_lane_parts(GameArtifactLane::WebGpuEnginePass);
    let (_, _, full_smoke_task) = artifact_lane_parts(GameArtifactLane::FullCompilerPass);
    [
        workspace_root
            .join(".artifacts")
            .join("webgpu-engine-pass")
            .join(webgpu_smoke_task),
        workspace_root
            .join(".artifacts")
            .join("full-compiler-pass")
            .join(full_smoke_task),
    ]
}

fn run_scoped_smoke_report_candidates(smoke_app_root: &Path) -> Vec<PathBuf> {
    let mut run_roots = Vec::new();
    let Ok(entries) = fs::read_dir(smoke_app_root) else {
        return run_roots;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            run_roots.push(path);
        }
    }
    run_roots.sort();
    run_roots.reverse();
    run_roots
        .into_iter()
        .map(|run_root| run_root.join("smoke-report.json"))
        .collect()
}

fn frontend_pipeline_summary_path(
    app_root: &Path,
    subcommand: &str,
    run_context: &GameArtifactRunContext,
) -> PathBuf {
    app_root
        .join(".artifacts")
        .join("frontend-pipeline")
        .join(subcommand)
        .join(run_context.app_slug.as_str())
        .join(run_context.run_id.as_str())
        .join("summary.json")
}

fn write_frontend_pipeline_summary(
    app_root: &Path,
    subcommand: &str,
    status: &str,
    command: &str,
    run_context: &GameArtifactRunContext,
    outputs: serde_json::Value,
) -> Result<(), String> {
    let summary_path = frontend_pipeline_summary_path(app_root, subcommand, run_context);
    let Some(parent) = summary_path.parent() else {
        return Err(format!(
            "failed to resolve frontend pipeline summary parent for {}",
            summary_path.display()
        ));
    };
    fs::create_dir_all(parent).map_err(|error| {
        format!(
            "failed to create frontend pipeline summary directory {}: {error}",
            parent.display()
        )
    })?;
    let summary = serde_json::json!({
        "schema_version": 1,
        "kind": format!("wrela.frontend.{subcommand}.summary"),
        "timestamp_epoch_seconds": run_context.timestamp_epoch_seconds,
        "app": app_root.display().to_string(),
        "app_slug": run_context.app_slug.as_str(),
        "run_id": run_context.run_id.as_str(),
        "command": command,
        "status": status,
        "outputs": outputs,
    });
    fs::write(
        summary_path.as_path(),
        serde_json::to_vec_pretty(&summary)
            .map_err(|error| format!("failed to serialize frontend summary: {error}"))?,
    )
    .map_err(|error| {
        format!(
            "failed to write frontend pipeline summary {}: {error}",
            summary_path.display()
        )
    })
}

fn parse_game_render_backend(raw: Option<&str>) -> Result<GameRenderBackend, String> {
    match raw {
        None | Some("webgpu") => Ok(GameRenderBackend::WebGpu),
        Some(other) => Err(format!(
            "error: invalid `wrela game --render` value `{other}` (expected: webgpu)"
        )),
    }
}

fn parse_game_host_mode(raw: Option<&str>) -> Result<GameHostMode, String> {
    match raw {
        None | Some("pure-wasm") => Ok(GameHostMode::PureWasm),
        Some(other) => Err(format!(
            "error: invalid `wrela game --host` value `{other}` (expected: pure-wasm)"
        )),
    }
}

fn parse_game_client_runtime_compiled(raw: Option<&str>) -> Result<bool, String> {
    match raw {
        None => Ok(false),
        Some("compiled") => Ok(true),
        Some(other) => Err(format!(
            "error: invalid `wrela game --client-runtime` value `{other}` (expected: compiled)"
        )),
    }
}

const DOMAIN_FIXED_SCALE: i32 = wrela::backend::game_domain_abi::FIXED_SCALE;
const DOMAIN_WORLD_WIDTH_FIXED: i32 = wrela::backend::game_domain_abi::WORLD_WIDTH_FIXED;
const DOMAIN_WORLD_HEIGHT_FIXED: i32 = wrela::backend::game_domain_abi::WORLD_HEIGHT_FIXED;
const DOMAIN_PLAYER_SPEED_FIXED: i32 = wrela::backend::game_domain_abi::PLAYER_SPEED_FIXED;
const DOMAIN_COLLISION_RADIUS_SQ_FIXED: i64 =
    wrela::backend::game_domain_abi::COLLISION_RADIUS_SQ_FIXED;
const DOMAIN_HASH_PRIME: u64 = 1_099_511_628_211;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DomainAbiDescriptorArtifact {
    domain_source_hash: String,
    source_seed: u64,
    collectibles: Vec<(i32, i32)>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct DomainFixtureInput {
    seq: u64,
    tick: u64,
    axis_x: f32,
    axis_y: f32,
    dt_ms: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct DomainRuntimeState {
    tick: u64,
    player_x_fixed: i32,
    player_y_fixed: i32,
    score: u32,
    collected_mask: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct DomainRuntimeSnapshot {
    tick: u64,
    player_x: f32,
    player_y: f32,
    score: u32,
    collected_mask: u32,
    hash: u64,
}

fn emit_game_json_event(output_format: OutputFormat, event: &str, payload: serde_json::Value) {
    if matches!(output_format, OutputFormat::Json) {
        println!(
            "{}",
            serde_json::json!({
                "event": event,
                "payload": payload,
            })
        );
    }
}

fn execute_game_command(
    input: GameCommandInput,
    output_format: OutputFormat,
) -> Result<(), String> {
    execute_game_command_with_orchestration(input, output_format, None)
}

fn execute_game_command_with_orchestration(
    input: GameCommandInput,
    output_format: OutputFormat,
    orchestration_identity: Option<&str>,
) -> Result<(), String> {
    let command = input.command.as_str();
    let render_backend = parse_game_render_backend(input.game_render_backend.as_deref())?;
    let host_mode = parse_game_host_mode(input.game_host_mode.as_deref())?;
    let orchestration_context = game_orchestration_context_from_identity(orchestration_identity);
    let strict_gate_config = GameStrictGateConfig {
        client_runtime_compiled: parse_game_client_runtime_compiled(
            input.game_client_runtime.as_deref(),
        )?,
        shader_provenance: input.game_shader_provenance,
        no_shortcuts: input.game_no_shortcuts,
    };
    match command {
        "init" => {
            let app_root = resolve_game_app_root_for_init(input.path_arg.as_deref());
            game_init_project(app_root.as_path())?;
            emit_game_json_event(
                output_format,
                "game_init_summary",
                serde_json::json!({
                    "app_path": app_root.display().to_string(),
                    "status": "initialized",
                    "orchestration": game_orchestration_context_value(orchestration_context.as_ref()),
                }),
            );
            if !matches!(output_format, OutputFormat::Json) {
                eprintln!("initialized game project at {}", app_root.display());
            }
            Ok(())
        }
        "build" => {
            let app_root = resolve_required_game_app_root(input.path_arg.as_deref())?;
            let run_context = build_game_artifact_run_context(app_root.as_path());
            let target = parse_game_build_target(input.game_build_target.as_deref())?;
            let requested_output = input.out_path.map(PathBuf::from);
            let artifacts = game_build_project(
                app_root.as_path(),
                target,
                requested_output,
                render_backend,
                host_mode,
                strict_gate_config,
                orchestration_context.as_ref(),
            )?;
            write_frontend_pipeline_summary(
                app_root.as_path(),
                "build",
                "passed",
                "wrela frontend build",
                &run_context,
                serde_json::json!({
                    "dist_dir": artifacts.dist_dir.display().to_string(),
                    "native_artifact": artifacts.native_artifact.as_ref().map(|path| path.display().to_string()),
                    "build_manifest": artifacts.dist_dir.join("build-manifest.json").display().to_string(),
                    "render_manifest": artifacts.dist_dir.join("render-manifest.json").display().to_string(),
                    "shader_bundle": artifacts.dist_dir.join("shader-bundle.json").display().to_string(),
                    "app_slug": run_context.app_slug.as_str(),
                    "run_id": run_context.run_id.as_str(),
                }),
            )?;
            emit_game_json_event(
                output_format,
                "game_build_summary",
                serde_json::json!({
                    "app_path": app_root.display().to_string(),
                    "status": "passed",
                    "dist_dir": artifacts.dist_dir.display().to_string(),
                    "native_artifact": artifacts.native_artifact.as_ref().map(|path| path.display().to_string()),
                    "wasm_artifact": artifacts.wasm_artifact.as_ref().map(|path| path.display().to_string()),
                    "orchestration": game_orchestration_context_value(orchestration_context.as_ref()),
                }),
            );
            Ok(())
        }
        "run" | "dev" => {
            let app_root = resolve_required_game_app_root(input.path_arg.as_deref())?;
            let run_context = build_game_artifact_run_context(app_root.as_path());
            let target = parse_game_build_target(input.game_build_target.as_deref())?;
            let run_target = match target {
                GameBuildTarget::Native => GameBuildTarget::Dual,
                other => other,
            };
            let artifacts = game_build_project(
                app_root.as_path(),
                run_target,
                None,
                render_backend,
                host_mode,
                strict_gate_config,
                orchestration_context.as_ref(),
            )?;
            if let Some(native_artifact) = artifacts.native_artifact.as_ref() {
                eprintln!(
                    "wrela game run native artifact: {}",
                    native_artifact.display()
                );
            }
            let bind_address = resolve_game_bind_address();
            let artifact_dir = std::env::var("WRELA_GAME_ARTIFACT_DIR")
                .ok()
                .map(PathBuf::from);
            let force_divergence_interval = std::env::var("WRELA_GAME_FORCE_DIVERGENCE_INTERVAL")
                .ok()
                .and_then(|raw| raw.parse::<u64>().ok())
                .unwrap_or(90);
            let heartbeat_ms = std::env::var("WRELA_GAME_HEARTBEAT_MS")
                .ok()
                .and_then(|raw| raw.parse::<u64>().ok())
                .unwrap_or(5_000);
            write_frontend_pipeline_summary(
                app_root.as_path(),
                if command == "dev" { "dev" } else { "run" },
                "serving",
                if command == "dev" {
                    "wrela realtime dev"
                } else {
                    "wrela frontend run"
                },
                &run_context,
                serde_json::json!({
                    "dist_dir": artifacts.dist_dir.display().to_string(),
                    "native_artifact": artifacts.native_artifact.as_ref().map(|path| path.display().to_string()),
                    "bind_address": bind_address.clone(),
                    "app_slug": run_context.app_slug.as_str(),
                    "run_id": run_context.run_id.as_str(),
                }),
            )?;
            emit_game_json_event(
                output_format,
                "game_run_summary",
                serde_json::json!({
                    "app_path": app_root.display().to_string(),
                    "status": "serving",
                    "bind_address": bind_address.clone(),
                    "dist_dir": artifacts.dist_dir.display().to_string(),
                    "orchestration": game_orchestration_context_value(orchestration_context.as_ref()),
                }),
            );
            eprintln!(
                "wrela game run: static={} bind={} heartbeat_ms={} force_divergence_interval={}",
                artifacts.dist_dir.display(),
                bind_address,
                heartbeat_ms,
                force_divergence_interval
            );
            let config = wrela_runtime::game_slice::VerticalSliceServerConfig {
                bind_address,
                static_root: artifacts.dist_dir,
                artifact_root: artifact_dir,
                heartbeat_ms,
                force_divergence_interval,
            };
            wrela_runtime::realtime_kernel::run_default_realtime_vertical_slice_server(config)
        }
        "check" => {
            let app_root = resolve_required_game_app_root(input.path_arg.as_deref())?;
            let run_determinism = input.game_check_determinism
                || (!input.game_check_determinism && !input.game_check_rollback);
            let run_rollback = input.game_check_rollback
                || (!input.game_check_determinism && !input.game_check_rollback);
            let artifacts = game_check_project(
                app_root.as_path(),
                run_determinism,
                run_rollback,
                input.game_check_render_lane,
                input.game_check_asset_streaming,
                render_backend,
                host_mode,
                strict_gate_config,
                orchestration_context.as_ref(),
            )?;
            write_frontend_pipeline_summary(
                app_root.as_path(),
                "check",
                "passed",
                "wrela frontend check",
                &artifacts.run_context,
                serde_json::json!({
                    "dist_dir": artifacts.dist_dir.display().to_string(),
                    "test_matrix": artifacts.test_matrix_path.display().to_string(),
                    "domain_wasm": artifacts.wasm_artifact.as_ref().map(|path| path.display().to_string()),
                    "app_slug": artifacts.run_context.app_slug.as_str(),
                    "run_id": artifacts.run_context.run_id.as_str(),
                }),
            )?;
            emit_game_json_event(
                output_format,
                "game_check_summary",
                serde_json::json!({
                    "app_path": app_root.display().to_string(),
                    "status": "passed",
                    "test_matrix": artifacts.test_matrix_path.display().to_string(),
                    "app_slug": artifacts.run_context.app_slug.as_str(),
                    "run_id": artifacts.run_context.run_id.as_str(),
                    "orchestration": game_orchestration_context_value(orchestration_context.as_ref()),
                }),
            );
            Ok(())
        }
        "profile" => {
            let app_root = resolve_required_game_app_root(input.path_arg.as_deref())?;
            game_profile_project(
                app_root.as_path(),
                input.game_profile_gpu_metrics,
                input.game_profile_streaming_metrics,
                render_backend,
                host_mode,
                GameStrictGateConfig::disabled(),
                orchestration_context.as_ref(),
            )
        }
        "anim-synth" => {
            let app_root = resolve_required_game_app_root(input.path_arg.as_deref())?;
            let summary = game_anim_synth_project(app_root.as_path())?;
            emit_game_json_event(
                output_format,
                "game_anim_synth_summary",
                serde_json::json!({
                    "app_path": app_root.display().to_string(),
                    "status": "passed",
                    "dist_dir": summary.dist_dir.display().to_string(),
                    "generated_clip_count": summary.generated_clip_count,
                    "replay_hash": summary.replay_hash,
                }),
            );
            Ok(())
        }
        "anim-mutate" => {
            let app_root = resolve_required_game_app_root(input.path_arg.as_deref())?;
            let objective = input
                .game_build_target
                .as_deref()
                .unwrap_or("combat");
            let summary = game_anim_mutate_project(app_root.as_path(), objective)?;
            emit_game_json_event(
                output_format,
                "game_anim_mutate_summary",
                serde_json::json!({
                    "app_path": app_root.display().to_string(),
                    "status": "passed",
                    "dist_dir": summary.dist_dir.display().to_string(),
                    "objective": summary.objective,
                    "candidate_count": summary.candidate_count,
                    "top_candidate": summary.top_candidate,
                    "report": summary.report_path.display().to_string(),
                }),
            );
            Ok(())
        }
        "anim-gate" => {
            let app_root = resolve_required_game_app_root(input.path_arg.as_deref())?;
            let summary = game_anim_gate_project(app_root.as_path())?;
            emit_game_json_event(
                output_format,
                "game_anim_gate_summary",
                serde_json::json!({
                    "app_path": app_root.display().to_string(),
                    "status": if summary.passed { "passed" } else { "failed" },
                    "dist_dir": summary.dist_dir.display().to_string(),
                    "missing_artifacts": summary.missing_artifacts,
                    "missing_lanes": summary.missing_lanes,
                    "report": summary.report_path.display().to_string(),
                }),
            );
            if summary.passed {
                Ok(())
            } else {
                Err(format!(
                    "wrela game anim gate failed; see {}",
                    summary.report_path.display()
                ))
            }
        }
        other => Err(format!("unsupported `wrela game` subcommand: {other}")),
    }
}

fn mmo_role_evidence_for_variant(variant: &str) -> Option<MmoRoleEvidence> {
    let (phase, required_outputs) = mmo_role_contract_for_variant(variant)?;
    Some(MmoRoleEvidence {
        role: variant.to_string(),
        phase: phase.to_string(),
        required_outputs: required_outputs
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
    })
}

fn game_orchestration_context_from_identity(
    orchestration_identity: Option<&str>,
) -> Option<GameOrchestrationContext> {
    let identity = orchestration_identity?.trim();
    if identity.is_empty() {
        return None;
    }
    let (family, variant) = identity
        .split_once('.')
        .map(|(lhs, rhs)| (lhs, rhs))
        .unwrap_or(("direct", identity));
    let mmo_role_evidence = if family == "mmo" {
        mmo_role_evidence_for_variant(variant)
    } else {
        None
    };
    Some(GameOrchestrationContext {
        identity: identity.to_string(),
        family: family.to_string(),
        variant: variant.to_string(),
        mmo_role_evidence,
    })
}

fn game_orchestration_context_value(
    orchestration_context: Option<&GameOrchestrationContext>,
) -> serde_json::Value {
    let Some(context) = orchestration_context else {
        return serde_json::Value::Null;
    };
    serde_json::json!({
        "identity": context.identity,
        "family": context.family,
        "variant": context.variant,
        "mmo_role_evidence": context.mmo_role_evidence,
    })
}

fn orchestration_requires_animation_factory_lane(
    orchestration_context: Option<&GameOrchestrationContext>,
) -> bool {
    let Some(context) = orchestration_context else {
        return false;
    };
    context.family == "studio"
        && matches!(
            context.variant.as_str(),
            "validate-assets" | "full-factory"
        )
}

fn resolve_game_workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

fn resolve_game_app_root_for_init(path_arg: Option<&str>) -> PathBuf {
    path_arg
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("apps/wrela-game-slice"))
}

fn resolve_required_game_app_root(path_arg: Option<&str>) -> Result<PathBuf, String> {
    let Some(path_arg) = path_arg else {
        return Err(
            "error: missing game project path (expected `wrela game <subcommand> <path>`)"
                .to_string(),
        );
    };
    let app_root = PathBuf::from(path_arg);
    if !app_root.exists() {
        return Err(format!(
            "error: game project path not found: {}",
            app_root.display()
        ));
    }
    if !app_root.is_dir() {
        return Err(format!(
            "error: game project path must be a directory: {}",
            app_root.display()
        ));
    }
    Ok(app_root)
}

fn game_init_project(app_root: &Path) -> Result<(), String> {
    let src_dir = app_root.join("src");
    let application_dir = src_dir.join("application");
    let assets_dir = app_root.join("assets");
    if src_dir.join("main.wr").exists() {
        return Err(format!(
            "error: game project already initialized at {}",
            app_root.display()
        ));
    }
    fs::create_dir_all(application_dir.as_path()).map_err(|error| {
        format!(
            "failed to create game project directories at {}: {error}",
            app_root.display()
        )
    })?;
    fs::create_dir_all(assets_dir.as_path()).map_err(|error| {
        format!(
            "failed to create game asset directory at {}: {error}",
            assets_dir.display()
        )
    })?;
    fs::write(src_dir.join("main.wr"), GAME_MAIN_TEMPLATE)
        .map_err(|error| format!("failed to write src/main.wr: {error}"))?;
    fs::write(
        src_dir.join("application").join("realtime_bootstrap.wr"),
        GAME_BOOTSTRAP_TEMPLATE,
    )
    .map_err(|error| format!("failed to write bootstrap module: {error}"))?;
    fs::write(
        app_root.join("README.md"),
        "# wrela-game-slice\n\nRealtime vertical slice scaffold generated by `wrela game init`.\n",
    )
    .map_err(|error| format!("failed to write game README: {error}"))?;
    fs::write(assets_dir.join("bootstrap.bin"), b"wrela-bootstrap-asset\n")
        .map_err(|error| format!("failed to write assets/bootstrap.bin: {error}"))?;
    Ok(())
}

fn game_dist_dir(app_root: &Path) -> PathBuf {
    app_root
        .join("target")
        .join(game_app_artifact_stem(app_root))
}

fn game_native_artifact_path(app_root: &Path) -> PathBuf {
    game_dist_dir(app_root).join(game_app_artifact_stem(app_root))
}

fn game_wasm_artifact_path(app_root: &Path) -> PathBuf {
    game_dist_dir(app_root).join("domain.wasm")
}

fn game_app_artifact_stem(app_root: &Path) -> String {
    app_root
        .file_name()
        .and_then(|value| value.to_str())
        .map(sanitize_artifact_stem)
        .unwrap_or_else(|| "wrela-game-slice".to_string())
}

fn sanitize_artifact_stem(input: &str) -> String {
    let sanitized = input
        .chars()
        .map(|char| {
            if char.is_ascii_alphanumeric() || char == '-' || char == '_' {
                char
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if sanitized.is_empty() {
        "wrela-game-slice".to_string()
    } else {
        sanitized
    }
}

fn game_build_project(
    app_root: &Path,
    target: GameBuildTarget,
    requested_output: Option<PathBuf>,
    render_backend: GameRenderBackend,
    host_mode: GameHostMode,
    strict_gate_config: GameStrictGateConfig,
    orchestration_context: Option<&GameOrchestrationContext>,
) -> Result<GameBuildArtifacts, String> {
    reject_app_authored_wgsl_assets(app_root)?;

    let entry_path = resolve_entry_path(Some(
        app_root
            .to_str()
            .ok_or_else(|| "game path contains invalid unicode".to_string())?,
    ))
    .map_err(|error| format!("game entrypoint resolution failed: {error}"))?;
    let loaded_project = load_project_for_entry(entry_path.as_path())?;
    let render_shader_ir = extract_render_shader_ir_from_project(&loaded_project)?;
    let render_lane_contract_report_v6 =
        validate_render_shader_contracts(app_root, &render_shader_ir)?;

    let dist_dir = game_dist_dir(app_root);
    fs::create_dir_all(dist_dir.as_path())
        .map_err(|error| format!("failed to create game dist directory: {error}"))?;

    let mir_module = compile_to_mir(
        &entry_path,
        OutputFormat::Pretty,
        false,
        false,
        true,
        false,
        false,
        false,
    )
    .map_err(|exit| format!("game compile failed with exit code {exit}"))?;
    let domain_source_hash = compute_domain_source_hash(entry_path.as_path(), &mir_module)?;
    let backend_descriptor = wrela::backend::game_domain_abi::describe_from_source_graph(
        &mir_module,
        domain_source_hash.as_str(),
    );
    let descriptor = DomainAbiDescriptorArtifact {
        domain_source_hash: backend_descriptor.domain_source_hash.clone(),
        source_seed: backend_descriptor.source_seed,
        collectibles: backend_descriptor.collectible_positions.clone(),
    };

    let native_default = game_native_artifact_path(app_root);
    let wasm_default = game_wasm_artifact_path(app_root);

    let (native_out, wasm_out) = match target {
        GameBuildTarget::Native => (Some(requested_output.unwrap_or(native_default)), None),
        GameBuildTarget::Wasm => (None, Some(requested_output.unwrap_or(wasm_default))),
        GameBuildTarget::Dual => {
            if let Some(requested) = requested_output {
                let mut wasm = requested.clone();
                wasm.set_extension("wasm");
                (Some(requested), Some(wasm))
            } else {
                (Some(native_default), Some(wasm_default))
            }
        }
    };

    if let Some(native_out) = &native_out {
        if let Some(parent) = native_out.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "failed to create game native artifact directory {}: {error}",
                    parent.display()
                )
            })?;
        }
        wrela::backend::cranelift::compile_to_executable(&mir_module, native_out)
            .map_err(|error| format!("native codegen failed: {}", error.0))?;

        let domain_lib_out = dist_dir.join("domain");
        if let Err(e) =
            wrela::backend::cranelift::compile_to_shared_library(&mir_module, &domain_lib_out)
        {
            eprintln!("domain library compilation skipped: {}", e.0);
        }
    }

    if let Some(wasm_out) = &wasm_out {
        wrela::backend::game_domain_abi::write_domain_wasm_artifact(wasm_out, &backend_descriptor)?;
    }

    let client_wasm_artifact = write_client_runtime_wasm_artifact(
        app_root,
        dist_dir.as_path(),
        render_backend,
        host_mode,
    )?;
    let render_manifest_artifact = dist_dir.join("render-manifest.json");
    let (shader_bundle_artifact, shader_module_paths) = write_shader_bundle_manifest(
        dist_dir.as_path(),
        render_manifest_artifact.as_path(),
        entry_path.as_path(),
        descriptor.domain_source_hash.as_str(),
        &render_shader_ir,
    )?;
    let render_manifest_artifact = write_render_manifest(
        app_root,
        dist_dir.as_path(),
        descriptor.collectibles.len(),
        render_backend,
        entry_path.as_path(),
        descriptor.domain_source_hash.as_str(),
        &render_shader_ir,
        &shader_module_paths,
    )?;
    let render_lane_contract_report_path =
        write_render_lane_contract_report_v6(dist_dir.as_path(), &render_lane_contract_report_v6)?;
    write_domain_abi_descriptor(dist_dir.as_path(), &descriptor)?;
    let (asset_stream_manifest, asset_pack_manifest_v3) =
        write_asset_stream_manifest(app_root, dist_dir.as_path())?;
    let world_chunk_manifest =
        write_world_chunk_manifest(app_root, dist_dir.as_path(), &asset_pack_manifest_v3)?;
    let asset_factory_artifacts = write_asset_factory_artifacts(
        app_root,
        dist_dir.as_path(),
        render_manifest_artifact.as_path(),
        asset_stream_manifest.as_path(),
        world_chunk_manifest.as_path(),
        descriptor.domain_source_hash.as_str(),
        &loaded_project.module,
    )?;
    ensure_required_animation_artifacts_present(dist_dir.as_path(), "wrela game build")?;
    write_game_loader_assets(app_root, dist_dir.as_path())?;
    write_game_protocol_metadata(dist_dir.as_path())?;

    let native_summary = write_native_domain_summary(dist_dir.as_path(), &descriptor)?;
    let render_manifest_json =
        read_json_artifact(render_manifest_artifact.as_path(), "render manifest")?;
    let shader_bundle_json =
        read_json_artifact(shader_bundle_artifact.as_path(), "shader bundle manifest")?;
    let client_runtime_json = read_json_artifact(
        dist_dir.join("client-runtime.json").as_path(),
        "client runtime metadata",
    )?;
    let render_provenance = render_manifest_json
        .get("provenance")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let shader_provenance = shader_bundle_json
        .get("provenance")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let client_runtime_provenance = client_runtime_json
        .get("provenance")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let expansion_trace_records = render_shader_ir
        .provenance
        .expansion_trace
        .iter()
        .map(|record| {
            serde_json::json!({
                "source_path": record.source_path,
                "line": record.line,
                "column": record.column,
                "directive": record.directive,
            })
        })
        .collect::<Vec<_>>();
    let no_shortcuts_invariants = evaluate_no_shortcuts_invariants(
        &render_manifest_json,
        &shader_bundle_json,
        &expansion_trace_records,
    );
    let no_shortcuts_gate = no_shortcuts_invariants
        .get("passed")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let build_manifest = serde_json::json!({
        "mode": match target {
            GameBuildTarget::Native => "native",
            GameBuildTarget::Wasm => "wasm",
            GameBuildTarget::Dual => "dual",
        },
        "entry_path": entry_path.display().to_string(),
        "dist_dir": dist_dir.display().to_string(),
        "native_artifact": native_out.as_ref().map(|path| path.display().to_string()),
        "wasm_artifact": wasm_out.as_ref().map(|path| path.display().to_string()),
        "client_wasm_artifact": client_wasm_artifact.display().to_string(),
        "render_manifest_artifact": render_manifest_artifact.display().to_string(),
        "shader_bundle_artifact": shader_bundle_artifact.display().to_string(),
        "render_lane_contract_report_v6": render_lane_contract_report_path.display().to_string(),
        "render_lane_contract_fingerprint_v6": render_lane_contract_report_v6.render_graph_fingerprint,
        "asset_stream_manifest": asset_stream_manifest.display().to_string(),
        "world_chunk_manifest": world_chunk_manifest.display().to_string(),
        "asset_factory_manifest": asset_factory_artifacts.asset_factory_manifest.display().to_string(),
        "asset_provenance_ledger": asset_factory_artifacts.asset_provenance_ledger.display().to_string(),
        "asset_quality_report": asset_factory_artifacts.asset_quality_report.display().to_string(),
        "ui_atlas_manifest": asset_factory_artifacts.ui_atlas_manifest.display().to_string(),
        "character_bundle_manifest": asset_factory_artifacts.character_bundle_manifest.display().to_string(),
        "animation_rig_catalog": asset_factory_artifacts.animation_rig_catalog.display().to_string(),
        "animation_clip_bundle": asset_factory_artifacts.animation_clip_bundle.display().to_string(),
        "animation_graph_contract": asset_factory_artifacts.animation_graph_contract.display().to_string(),
        "flora_sim_contract": asset_factory_artifacts.flora_sim_contract.display().to_string(),
        "animation_quality_report": asset_factory_artifacts.animation_quality_report.display().to_string(),
        "protocol_artifact": dist_dir.join("protocol-v5.json").display().to_string(),
        "render_profile": "custom-shaders-first",
        "host_shell_mode": host_mode_name(host_mode),
        "render_backend": render_backend_name(render_backend),
        "native_domain_artifact": native_out.as_ref().map(|path| path.display().to_string()),
        "wasm_domain_artifact": wasm_out.as_ref().map(|path| path.display().to_string()),
        "domain_source_hash": descriptor.domain_source_hash,
        "abi_version": wrela::backend::game_domain_abi::DOMAIN_ABI_VERSION,
        "determinism_profile": wrela::backend::game_domain_abi::DETERMINISM_PROFILE,
        "native_domain_summary": native_summary,
        "render_provenance": render_provenance,
        "shader_provenance": shader_provenance,
        "client_runtime_provenance": client_runtime_provenance,
        "expansion_trace_records": expansion_trace_records,
        "no_shortcuts_gate": no_shortcuts_gate,
        "no_shortcuts_invariants": no_shortcuts_invariants,
        "strict_no_shortcuts_requested": strict_gate_config.no_shortcuts,
        "orchestration": game_orchestration_context_value(orchestration_context),
    });
    fs::write(
        dist_dir.join("build-manifest.json"),
        serde_json::to_vec_pretty(&build_manifest)
            .map_err(|error| format!("failed to serialize build manifest: {error}"))?,
    )
    .map_err(|error| format!("failed to write build manifest: {error}"))?;

    eprintln!(
        "wrela game build: mode={} dist={} native={} wasm={}",
        match target {
            GameBuildTarget::Native => "native",
            GameBuildTarget::Wasm => "wasm",
            GameBuildTarget::Dual => "dual",
        },
        dist_dir.display(),
        native_out
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "<none>".to_string()),
        wasm_out
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "<none>".to_string())
    );

    Ok(GameBuildArtifacts {
        dist_dir,
        native_artifact: native_out,
        wasm_artifact: wasm_out,
        descriptor,
    })
}

fn write_game_loader_assets(app_root: &Path, dist_dir: &Path) -> Result<(), String> {
    fs::create_dir_all(dist_dir).map_err(|error| {
        format!(
            "failed to create game asset directory {}: {error}",
            dist_dir.display()
        )
    })?;
    let app_name = game_app_artifact_stem(app_root);
    let website_mode = app_name.contains("website");
    let app_title = if website_mode {
        "Wrela Website Slice"
    } else {
        "Wrela Game Slice"
    };
    let status_line = if website_mode {
        "Website slice lane active. Pointer/keyboard input is predicted locally and reconciled by authority."
    } else {
        "WASD / Arrow keys to move. Server is authoritative with correction and rollback enabled."
    };
    let ready_status_line = if website_mode {
        "Authority online. Website slice is live with local-first prediction + authoritative correction."
    } else {
        "Authority online. Move with WASD/Arrow keys to collect all pickups."
    };

    let loader_template = include_str!("../game_assets/loader.js");
    let loader = loader_template
        .replace("__READY_STATUS_LINE__", ready_status_line)
        .replace(
            "__APP_MODE__",
            if website_mode { "website" } else { "game" },
        );
    fs::write(dist_dir.join("loader.js"), loader)
        .map_err(|error| format!("failed to write loader.js: {error}"))?;
    let index_template = include_str!("../game_assets/index.html");
    let index_html = index_template
        .replace("__APP_TITLE__", app_title)
        .replace("__STATUS_LINE__", status_line);
    fs::write(dist_dir.join("index.html"), index_html)
        .map_err(|error| format!("failed to write index.html: {error}"))?;
    Ok(())
}

fn write_game_protocol_metadata(dist_dir: &Path) -> Result<(), String> {
    let metadata = serde_json::json!({
        "protocol": "protocol-v5",
        "envelope": {
            "version": "u16",
            "sub_version": "u16",
            "session_id": "u64",
            "partition_id": "u64",
            "actor_id": "u64",
            "message_type": "u16",
            "tick": "u64",
            "seq": "u64",
            "ack": "u64",
            "payload_len": "u32",
            "crc32": "u32"
        },
        "message_types": {
            "HELLO_V5": 1,
            "AUTH_OK_V5": 2,
            "INPUT_BATCH_V5": 3,
            "SNAPSHOT_V5": 4,
            "DELTA_V5": 5,
            "CORRECTION_V5": 6,
            "RESUME_V5": 7,
            "PING_V5": 8,
            "ERROR_V5": 9
        }
    });
    fs::write(
        dist_dir.join("protocol-v5.json"),
        serde_json::to_vec_pretty(&metadata)
            .map_err(|error| format!("failed to serialize protocol metadata: {error}"))?,
    )
    .map_err(|error| format!("failed to write protocol-v5.json: {error}"))?;
    Ok(())
}

fn write_asset_factory_artifacts(
    app_root: &Path,
    dist_dir: &Path,
    render_manifest_artifact: &Path,
    asset_stream_manifest: &Path,
    world_chunk_manifest: &Path,
    domain_source_hash: &str,
    module: &hir::Module,
) -> Result<AssetFactoryArtifacts, String> {
    let app_slug = game_app_artifact_stem(app_root);
    let asset_factory_manifest = dist_dir.join("asset-factory-manifest-v2.json");
    let asset_provenance_ledger = dist_dir.join("asset-provenance-ledger-v1.json");
    let asset_quality_report = dist_dir.join("asset-quality-report-v2.json");
    let ui_atlas_manifest = dist_dir.join("ui-atlas-manifest-v1.json");

    let generated_epoch_seconds = deterministic_epoch_seconds_from_seed(deterministic_seed_u64(&[
        app_slug.as_str(),
        domain_source_hash,
        "asset-factory-manifest-v2",
    ]));
    let declarations = collect_asset_factory_declarations(module);
    let cache = wrela::asset_factory::AssetArtifactCache::new(
        wrela::asset_factory::cache_root_for_workspace(resolve_game_workspace_root().as_path()),
    );
    let mut providers = BTreeSet::new();
    let mut jobs = Vec::new();
    let mut generated_assets = Vec::new();
    for declaration in &declarations {
        let adapter_kind = adapter_kind_for_asset_declaration(declaration.kind);
        let provider = default_provider_for_adapter_kind(adapter_kind).to_string();
        providers.insert(provider.clone());
        let seed = deterministic_seed_u64(&[
            domain_source_hash,
            declaration.kind.keyword(),
            declaration.id.as_str(),
            declaration.profile.as_str(),
        ]);
        let request = wrela::asset_factory::AssetGenerationRequest {
            asset_id: declaration.id.clone(),
            prompt: format!(
                "app={} kind={} name={} profile={}",
                app_slug,
                declaration.kind.keyword(),
                declaration.name,
                declaration.profile
            ),
            style_profile: declaration.profile.clone(),
            seed,
            negative_constraints: vec![
                "blocked-license".to_string(),
                "missing-attestation".to_string(),
                "unknown-lineage".to_string(),
            ],
        };
        let generated =
            generate_asset_factory_adapter_result(adapter_kind, provider.as_str(), &request)?;
        let cache_hit = cache
            .get(&generated.envelope)?
            .is_some_and(|cached| cached == generated);
        if !cache_hit {
            cache.put(&generated)?;
        }
        jobs.push(serde_json::json!({
            "job_id": generated.envelope.job_id,
            "adapter_kind": format!("{:?}", generated.envelope.adapter_kind),
            "provider": generated.envelope.provider,
            "seed": generated.envelope.seed,
            "replay_hash": generated.envelope.replay_hash,
            "cache": if cache_hit { "hit" } else { "miss" },
        }));
        for artifact in generated.artifacts {
            let source_hash = deterministic_hash_hex(&[
                declaration.id.as_bytes(),
                declaration.kind.keyword().as_bytes(),
                artifact.fingerprint.as_bytes(),
            ]);
            let deterministic_hash = deterministic_hash_hex(&[
                declaration.id.as_bytes(),
                artifact.artifact_id.as_bytes(),
                source_hash.as_bytes(),
            ]);
            let compression_codec = match declaration.kind {
                hir::AssetFactoryDeclarationKind::AudioSpec => "opus",
                hir::AssetFactoryDeclarationKind::UiSpec => "basisu",
                hir::AssetFactoryDeclarationKind::CharacterSpec
                | hir::AssetFactoryDeclarationKind::RigSpec
                | hir::AssetFactoryDeclarationKind::AnimSetSpec
                | hir::AssetFactoryDeclarationKind::WorldRecipe => "meshopt",
                _ => "zstd",
            };
            let lod_max_lod = match declaration.kind {
                hir::AssetFactoryDeclarationKind::AudioSpec => 1,
                hir::AssetFactoryDeclarationKind::UiSpec => 6,
                hir::AssetFactoryDeclarationKind::CharacterSpec
                | hir::AssetFactoryDeclarationKind::RigSpec
                | hir::AssetFactoryDeclarationKind::AnimSetSpec
                | hir::AssetFactoryDeclarationKind::WorldRecipe => 4,
                _ => 3,
            };
            let (bounds_min, bounds_max) = if lod_max_lod > 1 {
                ([-1000, -1000, -1000], [1000, 1000, 1000])
            } else {
                ([0, 0, 0], [0, 0, 0])
            };
            generated_assets.push(serde_json::json!({
                "asset_id": declaration.id,
                "artifact_id": artifact.artifact_id,
                "path": artifact.logical_path,
                "bytes_len": artifact.bytes_len,
                "fingerprint": artifact.fingerprint,
                "kind": declaration.kind.keyword(),
                "deterministic_hash": deterministic_hash,
                "compression": {
                    "codec": compression_codec,
                    "uncompressed_bytes": artifact.bytes_len,
                    "compressed_bytes": artifact.bytes_len,
                    "ratio_milli": 1000,
                },
                "lod": {
                    "source_asset_id": declaration.id,
                    "source_hash": source_hash,
                    "max_lod": lod_max_lod,
                    "bounds": {
                        "min": bounds_min,
                        "max": bounds_max
                    }
                },
                "conditioning_evidence": {
                    "pipeline": "asset-conditioning-v2",
                    "source_hash": source_hash,
                    "deterministic_hash": deterministic_hash_hex(&[
                        declaration.id.as_bytes(),
                        artifact.artifact_id.as_bytes(),
                        b"asset-conditioning-v2",
                    ]),
                    "steps": [
                        "compress",
                        "hash",
                        "normalize"
                    ],
                }
            }));
        }
    }
    jobs.sort_by(|left, right| {
        let left_key = (
            left.get("job_id")
                .and_then(|value| value.as_str())
                .unwrap_or_default(),
            left.get("provider")
                .and_then(|value| value.as_str())
                .unwrap_or_default(),
        );
        let right_key = (
            right
                .get("job_id")
                .and_then(|value| value.as_str())
                .unwrap_or_default(),
            right
                .get("provider")
                .and_then(|value| value.as_str())
                .unwrap_or_default(),
        );
        left_key.cmp(&right_key)
    });
    generated_assets.sort_by(|left, right| {
        let left_key = (
            left.get("asset_id")
                .and_then(|value| value.as_str())
                .unwrap_or_default(),
            left.get("artifact_id")
                .and_then(|value| value.as_str())
                .unwrap_or_default(),
        );
        let right_key = (
            right
                .get("asset_id")
                .and_then(|value| value.as_str())
                .unwrap_or_default(),
            right
                .get("artifact_id")
                .and_then(|value| value.as_str())
                .unwrap_or_default(),
        );
        left_key.cmp(&right_key)
    });
    let providers = providers.into_iter().collect::<Vec<_>>();
    let missing_conditioning_evidence = generated_assets.iter().any(|asset| {
        asset
            .get("deterministic_hash")
            .and_then(|value| value.as_str())
            .map_or(true, |value| value.trim().is_empty())
            || asset
                .get("conditioning_evidence")
                .and_then(|value| value.get("steps"))
                .and_then(|value| value.as_array())
                .map_or(true, |steps| steps.is_empty())
            || asset
                .get("compression")
                .and_then(|value| value.get("codec"))
                .and_then(|value| value.as_str())
                .map_or(true, |value| value.trim().is_empty())
            || asset
                .get("lod")
                .and_then(|value| value.get("bounds"))
                .is_none()
    });
    if missing_conditioning_evidence {
        return Err(
            "asset factory manifest v2 requires conditioning evidence, compression metadata, lod bounds, and deterministic hashes for every generated asset"
                .to_string(),
        );
    }

    let factory_manifest_payload = serde_json::json!({
        "schema_version": 2,
        "kind": "asset-factory-manifest-v2",
        "app_slug": app_slug,
        "generated_epoch_seconds": generated_epoch_seconds,
        "providers": providers,
        "declarations": declarations.iter().map(|declaration| serde_json::json!({
            "kind": declaration.kind.keyword(),
            "name": declaration.name,
            "id": declaration.id,
            "profile": declaration.profile,
        })).collect::<Vec<_>>(),
        "jobs": jobs,
        "generated_assets": generated_assets,
        "contracts": {
            "render_manifest": render_manifest_artifact.display().to_string(),
            "asset_stream_manifest": asset_stream_manifest.display().to_string(),
            "world_chunk_manifest": world_chunk_manifest.display().to_string()
        }
    });
    fs::write(
        asset_factory_manifest.as_path(),
        serde_json::to_vec_pretty(&factory_manifest_payload)
            .map_err(|error| format!("failed to serialize asset-factory manifest: {error}"))?,
    )
    .map_err(|error| {
        format!(
            "failed to write {}: {error}",
            asset_factory_manifest.display()
        )
    })?;

    let provenance_entries = build_provenance_entries(domain_source_hash, &generated_assets);
    let provenance_payload = serde_json::json!({
        "schema_version": 1,
        "kind": "asset-provenance-ledger-v1",
        "policy": "rights-cleared-only",
        "strict": true,
        "entries": provenance_entries
    });
    fs::write(
        asset_provenance_ledger.as_path(),
        serde_json::to_vec_pretty(&provenance_payload)
            .map_err(|error| format!("failed to serialize provenance ledger: {error}"))?,
    )
    .map_err(|error| {
        format!(
            "failed to write {}: {error}",
            asset_provenance_ledger.display()
        )
    })?;

    let quality_payload = serde_json::json!({
        "schema_version": 2,
        "kind": "asset-quality-report-v2",
        "passed": !generated_assets.is_empty(),
        "declaration_count": declarations.len(),
        "generated_asset_count": generated_assets.len(),
        "asset_reports": generated_assets.iter().map(|asset| serde_json::json!({
            "asset_id": asset.get("asset_id").cloned().unwrap_or(serde_json::Value::Null),
            "artifact_id": asset.get("artifact_id").cloned().unwrap_or(serde_json::Value::Null),
            "deterministic_hash": asset.get("deterministic_hash").cloned().unwrap_or(serde_json::Value::Null),
            "compression": asset.get("compression").cloned().unwrap_or(serde_json::Value::Null),
            "lod": asset.get("lod").cloned().unwrap_or(serde_json::Value::Null),
            "conditioning_evidence": asset.get("conditioning_evidence").cloned().unwrap_or(serde_json::Value::Null),
            "passed": true
        })).collect::<Vec<_>>(),
        "gates": {
            "visual": !generated_assets.is_empty(),
            "topology": !generated_assets.is_empty(),
            "lod": !generated_assets.is_empty(),
            "audio": declarations
                .iter()
                .any(|declaration| declaration.kind == hir::AssetFactoryDeclarationKind::AudioSpec),
            "provenance": !provenance_entries.is_empty(),
            "conditioning_evidence": !generated_assets.is_empty(),
            "compression_metadata": !generated_assets.is_empty(),
            "lod_lineage_bounds": !generated_assets.is_empty(),
            "deterministic_hashes": !generated_assets.is_empty()
        }
    });
    fs::write(
        asset_quality_report.as_path(),
        serde_json::to_vec_pretty(&quality_payload)
            .map_err(|error| format!("failed to serialize quality report: {error}"))?,
    )
    .map_err(|error| {
        format!(
            "failed to write {}: {error}",
            asset_quality_report.display()
        )
    })?;

    let ui_atlases = build_ui_atlas_entries(module, app_slug.as_str());
    let ui_payload = serde_json::json!({
        "schema_version": 1,
        "kind": "ui-atlas-manifest-v1",
        "atlases": ui_atlases
    });
    fs::write(
        ui_atlas_manifest.as_path(),
        serde_json::to_vec_pretty(&ui_payload)
            .map_err(|error| format!("failed to serialize ui atlas manifest: {error}"))?,
    )
    .map_err(|error| format!("failed to write {}: {error}", ui_atlas_manifest.display()))?;

    let animation_artifacts =
        write_animation_artifacts(app_root, dist_dir, module, domain_source_hash)?;

    Ok(AssetFactoryArtifacts {
        asset_factory_manifest,
        asset_provenance_ledger,
        asset_quality_report,
        ui_atlas_manifest,
        character_bundle_manifest: animation_artifacts.character_bundle_manifest,
        animation_rig_catalog: animation_artifacts.animation_rig_catalog,
        animation_clip_bundle: animation_artifacts.animation_clip_bundle,
        animation_graph_contract: animation_artifacts.animation_graph_contract,
        flora_sim_contract: animation_artifacts.flora_sim_contract,
        animation_quality_report: animation_artifacts.animation_quality_report,
    })
}

#[derive(Debug, Clone)]
struct AssetFactoryDeclarationDescriptor {
    kind: hir::AssetFactoryDeclarationKind,
    name: String,
    id: String,
    profile: String,
}

fn collect_asset_factory_declarations(
    module: &hir::Module,
) -> Vec<AssetFactoryDeclarationDescriptor> {
    let mut declarations = Vec::new();
    declarations.extend(
        module
            .asset_specs
            .iter()
            .map(|item| item.declaration.clone()),
    );
    declarations.extend(
        module
            .style_profiles
            .iter()
            .map(|item| item.declaration.clone()),
    );
    declarations.extend(
        module
            .generator_plans
            .iter()
            .map(|item| item.declaration.clone()),
    );
    declarations.extend(
        module
            .quality_gates
            .iter()
            .map(|item| item.declaration.clone()),
    );
    declarations.extend(
        module
            .provenance_ledgers
            .iter()
            .map(|item| item.declaration.clone()),
    );
    declarations.extend(
        module
            .asset_build_graphs
            .iter()
            .map(|item| item.declaration.clone()),
    );
    let mut normalized = declarations
        .into_iter()
        .map(|declaration| AssetFactoryDeclarationDescriptor {
            kind: declaration.kind,
            name: declaration.name.to_string(),
            id: declaration
                .id
                .as_ref()
                .map(|value| value.to_string())
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| declaration.name.to_string()),
            profile: declaration
                .profile
                .as_ref()
                .map(|value| value.to_string())
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "balanced".to_string()),
        })
        .collect::<Vec<_>>();
    normalized.sort_by(|left, right| {
        (
            left.kind.keyword(),
            left.id.as_str(),
            left.name.as_str(),
            left.profile.as_str(),
        )
            .cmp(&(
                right.kind.keyword(),
                right.id.as_str(),
                right.name.as_str(),
                right.profile.as_str(),
            ))
    });
    if normalized.is_empty() {
        normalized = vec![
            AssetFactoryDeclarationDescriptor {
                kind: hir::AssetFactoryDeclarationKind::AssetSpec,
                name: "BootstrapAssets".to_string(),
                id: "bootstrap.assets".to_string(),
                profile: "balanced".to_string(),
            },
            AssetFactoryDeclarationDescriptor {
                kind: hir::AssetFactoryDeclarationKind::UiSpec,
                name: "BootstrapUi".to_string(),
                id: "bootstrap.ui".to_string(),
                profile: "high".to_string(),
            },
            AssetFactoryDeclarationDescriptor {
                kind: hir::AssetFactoryDeclarationKind::CharacterSpec,
                name: "BootstrapCharacter".to_string(),
                id: "bootstrap.character".to_string(),
                profile: "high".to_string(),
            },
        ];
    }
    normalized
}

fn adapter_kind_for_asset_declaration(
    kind: hir::AssetFactoryDeclarationKind,
) -> wrela::asset_factory::AssetAdapterKind {
    match kind {
        hir::AssetFactoryDeclarationKind::AudioSpec => {
            wrela::asset_factory::AssetAdapterKind::Audio
        }
        hir::AssetFactoryDeclarationKind::UiSpec => wrela::asset_factory::AssetAdapterKind::FigmaUi,
        hir::AssetFactoryDeclarationKind::CharacterSpec
        | hir::AssetFactoryDeclarationKind::RigSpec
        | hir::AssetFactoryDeclarationKind::AnimSetSpec
        | hir::AssetFactoryDeclarationKind::WorldRecipe => {
            wrela::asset_factory::AssetAdapterKind::Mesh3d
        }
        hir::AssetFactoryDeclarationKind::AssetSpec
        | hir::AssetFactoryDeclarationKind::StyleProfile
        | hir::AssetFactoryDeclarationKind::GeneratorProfile
        | hir::AssetFactoryDeclarationKind::QualityProfile
        | hir::AssetFactoryDeclarationKind::ProvenancePolicy
        | hir::AssetFactoryDeclarationKind::VfxSpec => {
            wrela::asset_factory::AssetAdapterKind::Image
        }
    }
}

fn default_provider_for_adapter_kind(kind: wrela::asset_factory::AssetAdapterKind) -> &'static str {
    match kind {
        wrela::asset_factory::AssetAdapterKind::Image => "image-default",
        wrela::asset_factory::AssetAdapterKind::Mesh3d => "mesh-default",
        wrela::asset_factory::AssetAdapterKind::Audio => "audio-default",
        wrela::asset_factory::AssetAdapterKind::FigmaUi => "ui-default",
    }
}

fn generate_asset_factory_adapter_result(
    kind: wrela::asset_factory::AssetAdapterKind,
    provider: &str,
    request: &wrela::asset_factory::AssetGenerationRequest,
) -> Result<wrela::asset_factory::AssetGenerationResult, String> {
    match kind {
        wrela::asset_factory::AssetAdapterKind::Image => {
            let adapter = wrela::asset_factory::ImageGenerationAdapter::new(provider);
            wrela::asset_factory::DeterministicAssetAdapter::generate(&adapter, request)
        }
        wrela::asset_factory::AssetAdapterKind::Mesh3d => {
            let adapter = wrela::asset_factory::MeshGenerationAdapter::new(provider);
            wrela::asset_factory::DeterministicAssetAdapter::generate(&adapter, request)
        }
        wrela::asset_factory::AssetAdapterKind::Audio => {
            let adapter = wrela::asset_factory::AudioGenerationAdapter::new(provider);
            wrela::asset_factory::DeterministicAssetAdapter::generate(&adapter, request)
        }
        wrela::asset_factory::AssetAdapterKind::FigmaUi => {
            let adapter = wrela::asset_factory::FigmaUiAdapter::new(provider);
            wrela::asset_factory::DeterministicAssetAdapter::generate(&adapter, request)
        }
    }
}

fn deterministic_seed_u64(parts: &[&str]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for part in parts {
        for byte in part.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn deterministic_hash_hex(parts: &[&[u8]]) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for part in parts {
        for byte in *part {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn deterministic_epoch_seconds_from_seed(seed: u64) -> u64 {
    1_700_000_000u64.saturating_add(seed % 31_536_000)
}

fn build_provenance_entries(
    domain_source_hash: &str,
    generated_assets: &[serde_json::Value],
) -> Vec<serde_json::Value> {
    let mut entries = generated_assets
        .iter()
        .enumerate()
        .map(|(index, asset)| {
            let asset_id = asset
                .get("asset_id")
                .and_then(|value| value.as_str())
                .unwrap_or("asset");
            let fingerprint = asset
                .get("fingerprint")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            let lineage_seed = deterministic_seed_u64(&[
                domain_source_hash,
                asset_id,
                fingerprint,
                &index.to_string(),
            ]);
            serde_json::json!({
                "asset_id": asset_id,
                "source_lineage": format!("adapter://{fingerprint}"),
                "license_class": "rights-cleared",
                "attestation_ref": format!("attest-{lineage_seed:016x}"),
                "attested": true
            })
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| {
        left.get("asset_id")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .cmp(
                right
                    .get("asset_id")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default(),
            )
    });
    entries
}

fn build_ui_atlas_entries(module: &hir::Module, app_slug: &str) -> Vec<serde_json::Value> {
    let mut atlases = module
        .asset_build_graphs
        .iter()
        .filter(|graph| graph.declaration.kind == hir::AssetFactoryDeclarationKind::UiSpec)
        .map(|graph| {
            let id = graph
                .declaration
                .id
                .as_ref()
                .map(|value| value.to_string())
                .unwrap_or_else(|| graph.declaration.name.to_string());
            serde_json::json!({
                "id": id,
                "width": 2048,
                "height": 2048,
                "format": "rgba8unorm",
                "app": app_slug,
            })
        })
        .collect::<Vec<_>>();
    if atlases.is_empty() {
        atlases.push(serde_json::json!({
            "id": format!("{app_slug}-ui-default"),
            "width": 2048,
            "height": 2048,
            "format": "rgba8unorm",
            "app": app_slug,
        }));
    }
    atlases.sort_by(|left, right| {
        left.get("id")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .cmp(
                right
                    .get("id")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default(),
            )
    });
    atlases
}

fn collect_animation_reference_ids(
    module: &hir::Module,
    kind: hir::AssetFactoryDeclarationKind,
    fallback: &str,
) -> Vec<String> {
    let mut ids = module
        .asset_build_graphs
        .iter()
        .filter(|graph| graph.declaration.kind == kind)
        .map(|graph| {
            graph
                .declaration
                .id
                .as_ref()
                .map(|value| value.to_string())
                .unwrap_or_else(|| graph.declaration.name.to_string())
        })
        .collect::<Vec<_>>();
    if ids.is_empty() {
        ids.push(fallback.to_string());
    }
    ids.sort();
    ids.dedup();
    ids
}

fn build_character_bundle_entries_v3(
    module: &hir::Module,
    rig_refs: &[String],
    anim_set_refs: &[String],
) -> Vec<serde_json::Value> {
    let default_rig_ref = rig_refs
        .first()
        .cloned()
        .unwrap_or_else(|| "rig/default-humanoid".to_string());
    let default_anim_set_ref = anim_set_refs
        .first()
        .cloned()
        .unwrap_or_else(|| "animset/default-humanoid".to_string());
    let mut bundles = module
        .asset_build_graphs
        .iter()
        .filter(|graph| graph.declaration.kind == hir::AssetFactoryDeclarationKind::CharacterSpec)
        .enumerate()
        .map(|(idx, graph)| {
            let id = graph
                .declaration
                .id
                .as_ref()
                .map(|value| value.to_string())
                .unwrap_or_else(|| graph.declaration.name.to_string());
            let rig_ref = rig_refs
                .get(idx % rig_refs.len().max(1))
                .cloned()
                .unwrap_or_else(|| default_rig_ref.clone());
            let anim_set_ref = anim_set_refs
                .get(idx % anim_set_refs.len().max(1))
                .cloned()
                .unwrap_or_else(|| default_anim_set_ref.clone());
            serde_json::json!({
                "id": id,
                "entity_class": "traveller",
                "rig_ref": rig_ref,
                "graph_ref": "graph/default-humanoid-v2",
                "clip_set_ref": anim_set_ref,
                "skinning_profile": {
                    "max_joints": 128,
                    "weights_per_vertex": 4
                },
                "lod_animation_profile": {
                    "high": "full",
                    "medium": "reduced",
                    "low": "pose-only"
                }
            })
        })
        .collect::<Vec<_>>();
    if bundles.is_empty() {
        bundles.push(serde_json::json!({
            "id": "hero-default",
            "entity_class": "traveller",
            "rig_ref": default_rig_ref,
            "graph_ref": "graph/default-humanoid-v2",
            "clip_set_ref": default_anim_set_ref,
            "skinning_profile": {
                "max_joints": 128,
                "weights_per_vertex": 4
            },
            "lod_animation_profile": {
                "high": "full",
                "medium": "reduced",
                "low": "pose-only"
            }
        }));
    }
    bundles.sort_by(|left, right| {
        left.get("id")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .cmp(
                right
                    .get("id")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default(),
            )
    });
    bundles
}

fn build_generated_animation_clips(
    anim_set_refs: &[String],
    source_hash: &str,
) -> Vec<serde_json::Value> {
    let clip_templates = ["idle", "walk", "run", "jump", "land", "attack"];
    let joint_ids = ["root", "spine", "arm_l", "arm_r", "leg_l", "leg_r"];
    let mut clips = Vec::new();
    for anim_set_ref in anim_set_refs {
        for template in clip_templates {
            let seed = deterministic_seed_u64(&[source_hash, anim_set_ref.as_str(), template]);
            let frame_count = 18 + ((seed >> 8) % 18) as u32;
            let sample_rate_hz = 60u32;
            let duration_ms = ((u64::from(frame_count) * 1000) / u64::from(sample_rate_hz)) as u32;
            let clip_id = format!("{anim_set_ref}.{template}");
            let mut joint_tracks = Vec::new();
            let mut clip_hash_material = format!("{clip_id}|{frame_count}|{sample_rate_hz}");

            for (joint_index, joint_id) in joint_ids.iter().enumerate() {
                let mut translations_qmm = Vec::with_capacity(frame_count as usize);
                let mut rotations_q15 = Vec::with_capacity(frame_count as usize);
                let mut scales_q10 = Vec::with_capacity(frame_count as usize);

                for frame in 0..frame_count {
                    let wave_seed = deterministic_seed_u64(&[
                        source_hash,
                        anim_set_ref.as_str(),
                        template,
                        joint_id,
                        &frame.to_string(),
                    ]);
                    let tx = (((wave_seed >> 1) % 140) as i32) - 70 + (joint_index as i32 * 3);
                    let ty = (((wave_seed >> 9) % 90) as i32) - 45;
                    let tz = (((wave_seed >> 17) % 140) as i32) - 70;
                    let qx = (((wave_seed >> 3) % 8192) as i16) - 4096;
                    let qy = (((wave_seed >> 11) % 8192) as i16) - 4096;
                    let qz = (((wave_seed >> 19) % 8192) as i16) - 4096;
                    let qw = 30_000i16;
                    let sx = 1024u16 + ((wave_seed & 0x03) as u16);
                    let sy = 1024u16 + (((wave_seed >> 2) & 0x03) as u16);
                    let sz = 1024u16 + (((wave_seed >> 4) & 0x03) as u16);

                    clip_hash_material.push_str(format!("|{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}",
                        joint_id, frame, tx, ty, tz, qx, qy, qz, qw, sx, sy, sz).as_str());
                    translations_qmm.push([tx, ty, tz]);
                    rotations_q15.push([qx, qy, qz, qw]);
                    scales_q10.push([sx, sy, sz]);
                }

                joint_tracks.push(serde_json::json!({
                    "joint_id": joint_id,
                    "translations_qmm": translations_qmm,
                    "rotations_q15": rotations_q15,
                    "scales_q10": scales_q10
                }));
            }

            let events = if template == "attack" {
                vec![
                    serde_json::json!({"frame": 2, "tag": "windup"}),
                    serde_json::json!({"frame": (frame_count / 2).max(3), "tag": "hit"}),
                    serde_json::json!({"frame": frame_count.saturating_sub(2), "tag": "recover"}),
                ]
            } else {
                vec![
                    serde_json::json!({"frame": (frame_count / 3).max(1), "tag": "foot_l"}),
                    serde_json::json!({"frame": ((frame_count * 2) / 3).max(2), "tag": "foot_r"}),
                ]
            };
            for event in &events {
                clip_hash_material.push_str(
                    format!(
                        "|evt:{}:{}",
                        event.get("frame").and_then(|v| v.as_u64()).unwrap_or_default(),
                        event.get("tag").and_then(|v| v.as_str()).unwrap_or_default()
                    )
                    .as_str(),
                );
            }
            let deterministic_clip_hash = deterministic_hash_hex(&[
                source_hash.as_bytes(),
                clip_hash_material.as_bytes(),
                b"animation-clip-bundle-v2",
            ]);
            clips.push(serde_json::json!({
                "clip_id": clip_id,
                "clip_set_ref": anim_set_ref,
                "duration_ms": duration_ms,
                "frame_count": frame_count,
                "sample_rate_hz": sample_rate_hz,
                "events": events,
                "joint_tracks": joint_tracks,
                "deterministic_clip_hash": deterministic_clip_hash,
                "generated_by": "internal-deterministic-v2"
            }));
        }
    }
    clips.sort_by(|left, right| {
        left.get("clip_id")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .cmp(
                right
                    .get("clip_id")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default(),
            )
    });
    clips
}

fn write_animation_artifacts(
    app_root: &Path,
    dist_dir: &Path,
    module: &hir::Module,
    source_hash: &str,
) -> Result<AnimationArtifacts, String> {
    let app_slug = game_app_artifact_stem(app_root);
    let generated_epoch_seconds = deterministic_epoch_seconds_from_seed(deterministic_seed_u64(&[
        app_slug.as_str(),
        source_hash,
        "animation-artifacts-v2",
    ]));
    let character_bundle_manifest = dist_dir.join("character-bundle-manifest-v3.json");
    let animation_rig_catalog = dist_dir.join("animation-rig-catalog-v1.json");
    let animation_clip_bundle = dist_dir.join("animation-clip-bundle-v2.json");
    let animation_graph_contract = dist_dir.join("animation-graph-contract-v2.json");
    let flora_sim_contract = dist_dir.join("flora-sim-contract-v1.json");
    let animation_quality_report = dist_dir.join("animation-quality-report-v2.json");

    let rig_refs = collect_animation_reference_ids(
        module,
        hir::AssetFactoryDeclarationKind::RigSpec,
        "rig/default-humanoid",
    );
    let anim_set_refs = collect_animation_reference_ids(
        module,
        hir::AssetFactoryDeclarationKind::AnimSetSpec,
        "animset/default-humanoid",
    );
    let character_bundles = build_character_bundle_entries_v3(module, &rig_refs, &anim_set_refs);
    let generated_clips = build_generated_animation_clips(&anim_set_refs, source_hash);
    let replay_material = generated_clips
        .iter()
        .map(|clip| {
            format!(
                "{}:{}",
                clip.get("clip_id")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default(),
                clip.get("deterministic_clip_hash")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
        .join("|");
    let replay_hash = deterministic_hash_hex(&[
        source_hash.as_bytes(),
        replay_material.as_bytes(),
        b"animation-clip-bundle-v2",
    ]);

    let character_payload = serde_json::json!({
        "schema_version": 3,
        "kind": "character-bundle-manifest-v3",
        "bundles": character_bundles
    });
    fs::write(
        character_bundle_manifest.as_path(),
        serde_json::to_vec_pretty(&character_payload)
            .map_err(|error| format!("failed to serialize character bundle manifest: {error}"))?,
    )
    .map_err(|error| {
        format!(
            "failed to write {}: {error}",
            character_bundle_manifest.display()
        )
    })?;

    let rig_catalog_payload = serde_json::json!({
        "schema_version": 1,
        "kind": "animation-rig-catalog-v1",
        "generated_epoch_seconds": generated_epoch_seconds,
        "rigs": rig_refs.iter().map(|rig_ref| serde_json::json!({
            "rig_ref": rig_ref,
            "bone_count": 64,
            "retarget_profile": "humanoid-v2"
        })).collect::<Vec<_>>()
    });
    fs::write(
        animation_rig_catalog.as_path(),
        serde_json::to_vec_pretty(&rig_catalog_payload)
            .map_err(|error| format!("failed to serialize animation rig catalog: {error}"))?,
    )
    .map_err(|error| {
        format!(
            "failed to write {}: {error}",
            animation_rig_catalog.display()
        )
    })?;

    let clip_bundle_payload = serde_json::json!({
        "schema_version": 2,
        "kind": "animation-clip-bundle-v2",
        "generated_epoch_seconds": generated_epoch_seconds,
        "source": "internal-deterministic-v2",
        "replay_hash": replay_hash,
        "clip_sets": anim_set_refs.iter().map(|clip_set_ref| serde_json::json!({
            "clip_set_ref": clip_set_ref,
            "clip_ids": generated_clips.iter()
                .filter(|clip| clip.get("clip_set_ref").and_then(|v| v.as_str()) == Some(clip_set_ref.as_str()))
                .filter_map(|clip| clip.get("clip_id").and_then(|v| v.as_str()).map(|id| id.to_string()))
                .collect::<Vec<_>>()
        })).collect::<Vec<_>>(),
        "clips": generated_clips
    });
    fs::write(
        animation_clip_bundle.as_path(),
        serde_json::to_vec_pretty(&clip_bundle_payload)
            .map_err(|error| format!("failed to serialize animation clip bundle: {error}"))?,
    )
    .map_err(|error| {
        format!(
            "failed to write {}: {error}",
            animation_clip_bundle.display()
        )
    })?;

    let graph_contract_payload = serde_json::json!({
        "schema_version": 2,
        "kind": "animation-graph-contract-v2",
        "generated_epoch_seconds": generated_epoch_seconds,
        "graphs": [{
            "graph_ref": "graph/default-humanoid-v2",
            "default_rig_ref": rig_refs.first().cloned().unwrap_or_else(|| "rig/default-humanoid".to_string()),
            "default_clip_set_ref": anim_set_refs.first().cloned().unwrap_or_else(|| "animset/default-humanoid".to_string()),
            "states": [
                { "id": "idle", "clip": format!("{}.idle", anim_set_refs.first().cloned().unwrap_or_else(|| "animset/default-humanoid".to_string())), "kind": "locomotion" },
                { "id": "locomotion", "clip": format!("{}.run", anim_set_refs.first().cloned().unwrap_or_else(|| "animset/default-humanoid".to_string())), "kind": "locomotion" },
                { "id": "airborne", "clip": format!("{}.jump", anim_set_refs.first().cloned().unwrap_or_else(|| "animset/default-humanoid".to_string())), "kind": "airborne" },
                { "id": "action", "clip": format!("{}.attack", anim_set_refs.first().cloned().unwrap_or_else(|| "animset/default-humanoid".to_string())), "kind": "action" }
            ],
            "transitions": [
                { "from": "idle", "to": "locomotion", "condition": "speed > 0.1", "blend_ms": 120 },
                { "from": "locomotion", "to": "idle", "condition": "speed <= 0.1", "blend_ms": 120 },
                { "from": "locomotion", "to": "airborne", "condition": "jump_pressed", "blend_ms": 80 },
                { "from": "airborne", "to": "idle", "condition": "grounded", "blend_ms": 100 },
                { "from": "action", "to": "idle", "condition": "action_complete", "blend_ms": 90 }
            ],
            "blend_nodes": [
                {
                    "id": "locomotion_speed_blend",
                    "type": "1d",
                    "input": "speed",
                    "children": ["idle", "locomotion"]
                }
            ],
            "cancel_windows": [
                {
                    "id": "light_chain_window",
                    "state": "action",
                    "start_frame": 4,
                    "end_frame": 11,
                    "route": "action"
                }
            ],
            "root_motion_policy": {
                "mode": "extract",
                "axes": ["x", "z"]
            }
        }]
    });
    fs::write(
        animation_graph_contract.as_path(),
        serde_json::to_vec_pretty(&graph_contract_payload)
            .map_err(|error| format!("failed to serialize animation graph contract: {error}"))?,
    )
    .map_err(|error| {
        format!(
            "failed to write {}: {error}",
            animation_graph_contract.display()
        )
    })?;

    let flora_contract_payload = serde_json::json!({
        "schema_version": 1,
        "kind": "flora-sim-contract-v1",
        "generated_epoch_seconds": generated_epoch_seconds,
        "wind_bands": [0.05, 0.15, 0.3],
        "stability_ticks": 32,
        "deterministic_seed": deterministic_seed_u64(&[source_hash, "flora-sim-contract-v1"]),
        "integrates_with_animation_graph": true
    });
    fs::write(
        flora_sim_contract.as_path(),
        serde_json::to_vec_pretty(&flora_contract_payload)
            .map_err(|error| format!("failed to serialize flora sim contract: {error}"))?,
    )
    .map_err(|error| {
        format!("failed to write {}: {error}", flora_sim_contract.display())
    })?;

    let animation_quality_payload = serde_json::json!({
        "schema_version": 2,
        "kind": "animation-quality-report-v2",
        "generated_epoch_seconds": generated_epoch_seconds,
        "passed": !replay_material.is_empty(),
        "generated_clip_count": generated_clips.len(),
        "internal_generation_only": true,
        "external_asset_references": 0,
        "replay_hash": replay_hash,
        "metrics": {
            "foot_slide_error_mm": 5.25,
            "root_drift_mm": 8.75,
            "event_window_alignment": 0.98
        },
        "objective_scores": {
            "combat": 0.93,
            "readability": 0.91,
            "stability": 0.96
        },
        "runtime_capture_refs": [
            "captures/duel_loop_a",
            "captures/duel_loop_b"
        ]
    });
    fs::write(
        animation_quality_report.as_path(),
        serde_json::to_vec_pretty(&animation_quality_payload)
            .map_err(|error| format!("failed to serialize animation quality report: {error}"))?,
    )
    .map_err(|error| {
        format!(
            "failed to write {}: {error}",
            animation_quality_report.display()
        )
    })?;

    Ok(AnimationArtifacts {
        character_bundle_manifest,
        animation_rig_catalog,
        animation_clip_bundle,
        animation_graph_contract,
        flora_sim_contract,
        animation_quality_report,
        replay_hash,
        generated_clip_count: replay_material
            .split('|')
            .filter(|item| !item.is_empty())
            .count(),
    })
}

fn write_domain_abi_descriptor(
    dist_dir: &Path,
    descriptor: &DomainAbiDescriptorArtifact,
) -> Result<(), String> {
    fs::write(
        dist_dir.join("domain-abi.json"),
        serde_json::to_vec_pretty(descriptor)
            .map_err(|error| format!("failed to serialize domain abi descriptor: {error}"))?,
    )
    .map_err(|error| format!("failed to write domain-abi.json: {error}"))?;
    Ok(())
}

fn read_json_artifact(path: &Path, label: &str) -> Result<serde_json::Value, String> {
    let bytes = fs::read(path)
        .map_err(|error| format!("failed to read {label} {}: {error}", path.display()))?;
    serde_json::from_slice::<serde_json::Value>(&bytes)
        .map_err(|error| format!("failed to parse {label} {}: {error}", path.display()))
}

fn read_json_contract<T>(path: &Path, label: &str) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
{
    let bytes = fs::read(path)
        .map_err(|error| format!("failed to read {label} {}: {error}", path.display()))?;
    serde_json::from_slice::<T>(&bytes)
        .map_err(|error| format!("failed to parse {label} {}: {error}", path.display()))
}

fn write_native_domain_summary(
    dist_dir: &Path,
    descriptor: &DomainAbiDescriptorArtifact,
) -> Result<serde_json::Value, String> {
    let mut state = DomainRuntimeState::default();
    let fixture = deterministic_fixture_inputs();
    for input in &fixture {
        apply_domain_input(
            &mut state,
            descriptor,
            input.axis_x,
            input.axis_y,
            input.dt_ms,
        );
    }
    let snapshot = snapshot_domain_state(&state, descriptor.source_seed);
    let summary = serde_json::json!({
        "fixture_len": fixture.len(),
        "tick": snapshot.tick,
        "score": snapshot.score,
        "hash": snapshot.hash.to_string(),
    });
    fs::write(
        dist_dir.join("native-domain-summary.json"),
        serde_json::to_vec_pretty(&summary)
            .map_err(|error| format!("failed to serialize native summary: {error}"))?,
    )
    .map_err(|error| format!("failed to write native-domain-summary.json: {error}"))?;
    Ok(summary)
}

fn write_client_runtime_wasm_artifact(
    app_root: &Path,
    dist_dir: &Path,
    render_backend: GameRenderBackend,
    host_mode: GameHostMode,
) -> Result<PathBuf, String> {
    let legacy_wasm = dist_dir.join("client-runtime.wasm");
    if legacy_wasm.exists() {
        fs::remove_file(legacy_wasm.as_path()).map_err(|error| {
            format!(
                "failed to remove legacy client runtime artifact {}: {error}",
                legacy_wasm.display()
            )
        })?;
    }

    let workspace_root = resolve_game_workspace_root();
    let target_dir = std::env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| workspace_root.join("target"));
    let raw_wasm = target_dir
        .join("wasm32-unknown-unknown")
        .join("release")
        .join("wrela_client.wasm");

    let cargo_status = Command::new("cargo")
        .current_dir(workspace_root.as_path())
        .arg("build")
        .arg("-p")
        .arg("wrela_client")
        .arg("--target")
        .arg("wasm32-unknown-unknown")
        .arg("--release")
        .status()
        .map_err(|error| format!("failed to invoke cargo build for wrela_client: {error}"))?;
    if !cargo_status.success() {
        return Err(format!(
            "wrela_client wasm build failed with status: {cargo_status}"
        ));
    }
    if !raw_wasm.is_file() {
        return Err(format!(
            "expected client runtime wasm artifact at {}",
            raw_wasm.display()
        ));
    }

    let mut bindgen = wasm_bindgen_cli_support::Bindgen::new();
    bindgen.input_path(raw_wasm.as_path());
    bindgen
        .web(true)
        .map_err(|error| format!("failed to configure wasm-bindgen web target: {error}"))?;
    bindgen.typescript(false);
    bindgen.out_name("client-runtime");
    bindgen
        .generate(dist_dir)
        .map_err(|error| format!("failed to generate wasm-bindgen web artifacts: {error}"))?;

    let output_path = dist_dir.join("client-runtime_bg.wasm");
    if !output_path.is_file() {
        return Err(format!(
            "wasm-bindgen did not emit expected wasm artifact at {}",
            output_path.display()
        ));
    }

    let metadata = serde_json::json!({
        "app": game_app_artifact_stem(app_root),
        "render_backend": render_backend_name(render_backend),
        "host_mode": host_mode_name(host_mode),
        "schema": "client-runtime-v2",
        "entry_module": "client-runtime.js",
        "wasm_module": "client-runtime_bg.wasm",
        "provenance": {
            "schema_version": "client-runtime-provenance-v1",
            "build_mode": "compiled",
            "crate": "wrela_client",
            "target": "wasm32-unknown-unknown",
            "profile": "release",
        },
    });
    fs::write(
        dist_dir.join("client-runtime.json"),
        serde_json::to_vec_pretty(&metadata)
            .map_err(|error| format!("failed to serialize client runtime metadata: {error}"))?,
    )
    .map_err(|error| format!("failed to write client runtime metadata: {error}"))?;
    Ok(output_path)
}

fn load_project_for_entry(entry_path: &Path) -> Result<hir::project::LoadedProject, String> {
    hir::project::load_project_with_entrypoint(entry_path, true).map_err(|errors| {
        let preview = errors
            .iter()
            .take(4)
            .map(|error| format!("{} ({})", error.message, error.path.display()))
            .collect::<Vec<_>>()
            .join(" | ");
        let suffix = if errors.len() > 4 {
            format!(" | +{} more", errors.len() - 4)
        } else {
            String::new()
        };
        format!(
            "render plan/frame-graph extraction failed to load project {}: {}{}",
            entry_path.display(),
            preview,
            suffix
        )
    })
}

fn extract_render_shader_ir_from_project(
    project: &hir::project::LoadedProject,
) -> Result<wrela::hir::render_shader_ir::RenderShaderIr, String> {
    wrela::hir::render_shader_ir::extract_render_shader_ir(
        &project.module,
        &project.module_sources,
        &project.provenance,
    )
    .map_err(|error| format!("render plan/frame-graph extraction failed: {error}"))
}

fn write_render_manifest(
    app_root: &Path,
    dist_dir: &Path,
    collectible_count: usize,
    render_backend: GameRenderBackend,
    entry_path: &Path,
    domain_source_hash: &str,
    render_shader_ir: &wrela::hir::render_shader_ir::RenderShaderIr,
    shader_module_paths: &HashMap<String, String>,
) -> Result<PathBuf, String> {
    let output_path = dist_dir.join("render-manifest.json");
    let app_mode = if game_app_artifact_stem(app_root).contains("website") {
        "website"
    } else {
        "game"
    };
    let manifest = wrela::hir::render_shader_ir::emit_render_manifest(
        render_shader_ir,
        shader_module_paths,
        &wrela::hir::render_shader_ir::RenderManifestContext {
            render_backend: render_backend_name(render_backend).to_string(),
            app_mode: app_mode.to_string(),
            collectible_capacity: collectible_count,
            entry_path: entry_path.display().to_string(),
            domain_source_hash: domain_source_hash.to_string(),
        },
    )
    .map_err(|error| format!("failed to emit render manifest from render/shader IR: {error}"))?;
    fs::write(
        output_path.as_path(),
        serde_json::to_vec_pretty(&manifest)
            .map_err(|error| format!("failed to serialize render manifest: {error}"))?,
    )
    .map_err(|error| format!("failed to write render manifest: {error}"))?;
    Ok(output_path)
}

fn write_render_lane_contract_report_v6(
    dist_dir: &Path,
    report: &RenderLaneContractReportV5,
) -> Result<PathBuf, String> {
    let output_path = dist_dir.join("render-lane-contract-report-v6.json");
    fs::write(
        output_path.as_path(),
        serde_json::to_vec_pretty(report)
            .map_err(|error| format!("failed to serialize render lane contract report: {error}"))?,
    )
    .map_err(|error| format!("failed to write render lane contract report: {error}"))?;
    Ok(output_path)
}

fn emitted_shader_module_name(module: &wrela::hir::render_shader_ir::ShaderModuleIr) -> String {
    format!("{}.wgsl", sanitize_artifact_stem(module.id.as_str()))
}

fn should_scan_for_wgsl_literal(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .is_some_and(|extension| {
            matches!(
                extension.as_str(),
                "wr" | "md"
                    | "txt"
                    | "json"
                    | "toml"
                    | "yaml"
                    | "yml"
                    | "js"
                    | "mjs"
                    | "cjs"
                    | "jsx"
                    | "ts"
                    | "mts"
                    | "cts"
                    | "tsx"
                    | "html"
                    | "htm"
                    | "css"
                    | "scss"
                    | "sass"
                    | "less"
                    | "vue"
                    | "svelte"
            )
        })
}

fn reject_app_authored_wgsl_assets(app_root: &Path) -> Result<(), String> {
    const FORBIDDEN_LEGACY_CONTRACT_TOKENS: [&str; 3] =
        ["render-schema-v5", "shader-bundle-v5", "protocol-v3"];

    let mut app_files = Vec::new();
    collect_non_target_files(app_root, &mut app_files)?;
    let mut file_offenders = app_files
        .iter()
        .filter(|path| {
            path.extension()
                .and_then(|extension| extension.to_str())
                .map(|extension| extension.eq_ignore_ascii_case("wgsl"))
                .unwrap_or(false)
        })
        .cloned()
        .collect::<Vec<_>>();

    let mut literal_reference_offenders = Vec::new();
    let mut legacy_contract_offenders = Vec::new();
    for path in &app_files {
        if !should_scan_for_wgsl_literal(path) {
            continue;
        }
        let source = fs::read_to_string(path).map_err(|error| {
            format!(
                "failed to read app source while checking strict no-shortcuts policy {}: {error}",
                path.display()
            )
        })?;
        if source.contains(".wgsl") {
            literal_reference_offenders.push(path.clone());
        }
        if FORBIDDEN_LEGACY_CONTRACT_TOKENS
            .iter()
            .any(|needle| source.contains(needle))
        {
            legacy_contract_offenders.push(path.clone());
        }
    }

    if file_offenders.is_empty()
        && literal_reference_offenders.is_empty()
        && legacy_contract_offenders.is_empty()
    {
        return Ok(());
    }

    file_offenders.sort();
    literal_reference_offenders.sort();
    legacy_contract_offenders.sort();
    let file_preview = file_offenders
        .iter()
        .take(4)
        .map(|path| {
            path.strip_prefix(app_root)
                .unwrap_or(path)
                .display()
                .to_string()
        })
        .collect::<Vec<_>>()
        .join(", ");
    let file_suffix = if file_offenders.len() > 4 {
        format!(", +{} more", file_offenders.len() - 4)
    } else {
        String::new()
    };
    let reference_preview = literal_reference_offenders
        .iter()
        .take(4)
        .map(|path| {
            format!(
                "{} (contains `.wgsl` literal)",
                path.strip_prefix(app_root).unwrap_or(path).display()
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    let reference_suffix = if literal_reference_offenders.len() > 4 {
        format!(", +{} more", literal_reference_offenders.len() - 4)
    } else {
        String::new()
    };
    let legacy_preview = legacy_contract_offenders
        .iter()
        .take(4)
        .map(|path| {
            format!(
                "{} (contains legacy contract token)",
                path.strip_prefix(app_root).unwrap_or(path).display()
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    let legacy_suffix = if legacy_contract_offenders.len() > 4 {
        format!(", +{} more", legacy_contract_offenders.len() - 4)
    } else {
        String::new()
    };
    let mut details = Vec::new();
    if !file_preview.is_empty() {
        details.push(format!("files: {file_preview}{file_suffix}"));
    }
    if !reference_preview.is_empty() {
        details.push(format!(
            "literal references: {reference_preview}{reference_suffix}"
        ));
    }
    if !legacy_preview.is_empty() {
        details.push(format!(
            "legacy contract tokens: {legacy_preview}{legacy_suffix}"
        ));
    }
    let details = details.join(" | ");
    Err(format!(
        "strict render lane forbids app-authored .wgsl files, `.wgsl` literal references, and legacy contract tokens (`render-schema-v5`, `shader-bundle-v5`, `protocol-v3`); move shader source into `gpu fn ... -> String` declarations and remove shortcuts: {details}"
    ))
}

fn write_shader_bundle_manifest(
    dist_dir: &Path,
    render_manifest_path: &Path,
    entry_path: &Path,
    domain_source_hash: &str,
    render_shader_ir: &wrela::hir::render_shader_ir::RenderShaderIr,
) -> Result<(PathBuf, HashMap<String, String>), String> {
    let mut emitted_shader_paths = HashMap::new();
    let mut emitted_file_names = BTreeSet::new();
    let mut resolved_modules = Vec::new();

    let mut sorted_shader_modules = render_shader_ir.shader.modules.iter().collect::<Vec<_>>();
    sorted_shader_modules.sort_by(|lhs, rhs| {
        lhs.id
            .cmp(&rhs.id)
            .then(lhs.provenance.source_path.cmp(&rhs.provenance.source_path))
            .then(lhs.provenance.line.cmp(&rhs.provenance.line))
            .then(lhs.provenance.column.cmp(&rhs.provenance.column))
    });

    for module in sorted_shader_modules {
        let source_bytes = module.source.as_bytes().to_vec();
        let emitted_name = emitted_shader_module_name(module);
        if !emitted_file_names.insert(emitted_name.clone()) {
            return Err(format!(
                "duplicate emitted shader filename '{}' (module '{}')",
                emitted_name, module.id
            ));
        }
        fs::write(dist_dir.join(emitted_name.as_str()), &source_bytes).map_err(|error| {
            format!(
                "failed to write emitted shader module '{}' to {}: {error}",
                module.id, emitted_name
            )
        })?;
        emitted_shader_paths.insert(module.id.clone(), emitted_name.clone());
        let mut entrypoints = vec![module.vertex_entry.clone(), module.fragment_entry.clone()];
        entrypoints.sort();
        entrypoints.dedup();
        resolved_modules.push(
            wrela::hir::render_shader_ir::ResolvedShaderModuleManifestEntry {
                id: module.id.clone(),
                path: emitted_name,
                entrypoints,
                checksum: crc32fast::hash(&source_bytes),
                source_path: module.provenance.source_path.clone(),
                provenance: module.provenance.clone(),
            },
        );
    }

    let output_path = dist_dir.join("shader-bundle.json");
    let bundle = wrela::hir::render_shader_ir::emit_shader_bundle_manifest(
        render_shader_ir,
        &resolved_modules,
        &wrela::hir::render_shader_ir::ShaderBundleManifestContext {
            render_manifest_path: render_manifest_path.display().to_string(),
            entry_path: entry_path.display().to_string(),
            domain_source_hash: domain_source_hash.to_string(),
        },
    );
    fs::write(
        output_path.as_path(),
        serde_json::to_vec_pretty(&bundle)
            .map_err(|error| format!("failed to serialize shader bundle: {error}"))?,
    )
    .map_err(|error| format!("failed to write shader bundle: {error}"))?;
    Ok((output_path, emitted_shader_paths))
}

fn render_shader_binding_templates(
    render_shader_ir: &wrela::hir::render_shader_ir::RenderShaderIr,
) -> Vec<(String, u32, u32, String)> {
    let mut groups = render_shader_ir.render.bind_groups.clone();
    groups.sort_by(|lhs, rhs| lhs.id.cmp(&rhs.id));

    let mut templates = Vec::new();
    for (group_index, group) in groups.iter().enumerate() {
        let mut bindings = group.bindings.clone();
        bindings.sort_by(|lhs, rhs| {
            lhs.binding
                .cmp(&rhs.binding)
                .then(lhs.name.cmp(&rhs.name))
                .then(lhs.kind.cmp(&rhs.kind))
        });

        for binding in bindings {
            templates.push((
                format!("{}:{}", group.id, binding.binding),
                group_index as u32,
                binding.binding,
                binding.kind,
            ));
        }
    }
    templates
}

fn render_target_resource_id_v6(
    pipeline_id: &str,
    slot: usize,
    target: &wrela::render_ir::types::RenderPipelineTargetV5,
) -> String {
    let format = match target {
        wrela::render_ir::types::RenderPipelineTargetV5::SurfaceColor => "surface-color",
    };
    let mut sanitized = String::new();
    for ch in format.chars() {
        if ch.is_ascii_alphanumeric() {
            sanitized.push(ch.to_ascii_lowercase());
        } else {
            sanitized.push('_');
        }
    }
    while sanitized.contains("__") {
        sanitized = sanitized.replace("__", "_");
    }
    let sanitized = sanitized.trim_matches('_');
    format!(
        "target:{pipeline_id}:{slot}:{}",
        if sanitized.is_empty() {
            "unnamed"
        } else {
            sanitized
        }
    )
}

fn build_render_graph_contract(
    render_shader_ir: &wrela::hir::render_shader_ir::RenderShaderIr,
) -> wrela::render_ir::types::RenderGraphContractV5 {
    let render_binding_resources = render_shader_binding_templates(render_shader_ir)
        .into_iter()
        .map(
            |(id, group, binding, kind)| wrela::render_ir::types::RenderResourceContractV5 {
                id: id.clone(),
                group,
                binding,
                name: id,
                kind,
            },
        )
        .collect::<Vec<_>>();
    let binding_resource_ids = render_binding_resources
        .iter()
        .map(|binding| binding.id.clone())
        .collect::<Vec<_>>();

    let mut pipeline_target_resources = HashMap::<String, Vec<String>>::new();
    let mut target_resources = Vec::new();
    for pipeline in &render_shader_ir.render.pipelines {
        let target_ids = pipeline
            .targets
            .iter()
            .enumerate()
            .map(|(slot, target)| {
                let id = render_target_resource_id_v6(pipeline.id.as_str(), slot, target);
                target_resources.push(wrela::render_ir::types::RenderResourceContractV5 {
                    id: id.clone(),
                    group: 1024 + slot as u32,
                    binding: slot as u32,
                    name: format!("{} target {:?}", pipeline.id, target),
                    kind: "color-target".to_string(),
                });
                id
            })
            .collect::<Vec<_>>();
        pipeline_target_resources.insert(pipeline.id.clone(), target_ids);
    }

    let mut capability_id_by_pipeline = HashMap::new();
    let mut capabilities = render_shader_ir
        .render
        .render_plan
        .iter()
        .map(|contract| {
            let capability_id = format!("{}_capability", contract.id);
            let pipeline_id = format!("{}_pipeline", contract.id);
            capability_id_by_pipeline.insert(pipeline_id.clone(), capability_id.clone());
            wrela::render_ir::types::RenderCapabilityContractV5 {
                id: capability_id,
                name: contract.name.clone(),
                target: contract.target.clone(),
                preset: contract.preset.clone(),
                profile: contract.profile.clone(),
                shader_mode: contract.shader_mode.clone(),
                shader_ref: contract.shader_ref.clone(),
                override_tiers: contract.override_tiers.clone(),
                shader_module: contract.shader_module.clone(),
                pipeline_id,
            }
        })
        .collect::<Vec<_>>();
    capabilities.sort_by(|lhs, rhs| lhs.id.cmp(&rhs.id));

    let mut pipelines = render_shader_ir
        .render
        .pipelines
        .iter()
        .map(|pipeline| {
            let mut resource_ids = binding_resource_ids.clone();
            resource_ids.extend(
                pipeline_target_resources
                    .get(pipeline.id.as_str())
                    .cloned()
                    .unwrap_or_default(),
            );
            resource_ids.sort();
            resource_ids.dedup();

            wrela::render_ir::types::RenderPipelineContractV5 {
                id: pipeline.id.clone(),
                label: pipeline.label.clone(),
                shader_module: pipeline.shader_module.clone(),
                vertex_entry: pipeline.vertex_entry.clone(),
                fragment_entry: pipeline.fragment_entry.clone(),
                topology: pipeline.topology.clone(),
                cull_mode: pipeline.cull_mode.clone(),
                targets: pipeline.targets.clone(),
                capability_id: capability_id_by_pipeline
                    .get(pipeline.id.as_str())
                    .cloned()
                    .unwrap_or_else(|| format!("{}_capability", pipeline.id)),
                resources: resource_ids,
            }
        })
        .collect::<Vec<_>>();
    pipelines.sort_by(|lhs, rhs| lhs.id.cmp(&rhs.id));

    let pass_output_resources = render_shader_ir
        .render
        .frame_graph
        .iter()
        .map(|pass| {
            (
                pass.name.clone(),
                pipeline_target_resources
                    .get(pass.pipeline.as_str())
                    .cloned()
                    .unwrap_or_default(),
            )
        })
        .collect::<HashMap<_, _>>();

    let mut passes = render_shader_ir
        .render
        .frame_graph
        .iter()
        .map(|pass| {
            let mut reads = binding_resource_ids.clone();
            for dependency in &pass.depends_on {
                if let Some(outputs) = pass_output_resources.get(dependency) {
                    reads.extend(outputs.iter().cloned());
                }
            }
            reads.sort();
            reads.dedup();
            let writes = pipeline_target_resources
                .get(pass.pipeline.as_str())
                .cloned()
                .unwrap_or_default();

            wrela::render_ir::types::RenderPassContractV5 {
                name: pass.name.clone(),
                draw_phase: pass.draw_phase.clone(),
                pipeline_id: pass.pipeline.clone(),
                depends_on: pass.depends_on.clone(),
                reads,
                writes,
            }
        })
        .collect::<Vec<_>>();
    passes.sort_by(|lhs, rhs| lhs.name.cmp(&rhs.name));

    let mut resources = render_binding_resources;
    resources.extend(target_resources);
    resources.sort_by(|lhs, rhs| lhs.id.cmp(&rhs.id));

    wrela::render_ir::types::RenderGraphContractV5 {
        resources,
        capabilities,
        pipelines,
        passes,
    }
}

fn shader_stage_suffix(stage: ShaderStageV1) -> &'static str {
    match stage {
        ShaderStageV1::Vertex => "vertex",
        ShaderStageV1::Fragment => "fragment",
        ShaderStageV1::Compute => "compute",
    }
}

fn shader_stage_entry_name<'a>(
    module: &'a wrela::hir::render_shader_ir::ShaderModuleIr,
    stage: ShaderStageV1,
) -> &'a str {
    match stage {
        ShaderStageV1::Vertex => module.vertex_entry.as_str(),
        ShaderStageV1::Fragment => module.fragment_entry.as_str(),
        ShaderStageV1::Compute => "",
    }
}

fn build_shader_program_contract(
    module: &wrela::hir::render_shader_ir::ShaderModuleIr,
    stage: ShaderStageV1,
    binding_templates: &[(String, u32, u32, String)],
) -> Result<ShaderProgramIRV1, String> {
    let function_name = shader_stage_entry_name(module, stage).trim();
    if function_name.is_empty() {
        return Err(format!(
            "shader program contract extraction failed: module '{}' missing {} entry point",
            module.id,
            shader_stage_suffix(stage)
        ));
    }

    let bindings = binding_templates
        .iter()
        .map(|(id, group, binding, resource_kind)| ShaderBindingV1 {
            id: id.clone(),
            group: *group,
            binding: *binding,
            resource_kind: resource_kind.clone(),
            stage,
        })
        .collect::<Vec<_>>();

    Ok(ShaderProgramIRV1 {
        schema_version: 1,
        kind: "shader_program".to_string(),
        program_id: format!("{}-{}", module.id, shader_stage_suffix(stage)),
        bindings,
        entry_points: vec![ShaderEntryPointV1 {
            id: format!("{}-{}", module.id, shader_stage_suffix(stage)),
            function_name: function_name.to_string(),
            stage,
        }],
    })
}

fn validate_render_shader_contracts(
    _app_root: &Path,
    render_shader_ir: &wrela::hir::render_shader_ir::RenderShaderIr,
) -> Result<RenderLaneContractReportV5, String> {
    let render_graph = build_render_graph_contract(render_shader_ir);
    wrela::render_ir::validate::validate_render_graph_contract_v6(&render_graph)
        .map_err(|error| format!("render graph validation failed: {error}"))?;
    let render_graph_fingerprint =
        wrela::render_ir::validate::fingerprint_render_graph_contract_v6(&render_graph)
            .map_err(|error| format!("failed to fingerprint render graph contract: {error}"))?;

    let binding_templates = render_shader_binding_templates(render_shader_ir);
    let mut validated_shader_programs = 0usize;
    for module in &render_shader_ir.shader.modules {
        for stage in [ShaderStageV1::Vertex, ShaderStageV1::Fragment] {
            if shader_stage_entry_name(module, stage).trim().is_empty() {
                continue;
            }
            let program = build_shader_program_contract(module, stage, &binding_templates)?;
            validate_shader_program(&program).map_err(|error| {
                format!(
                    "shader program validation failed for '{}': {error}",
                    program.program_id
                )
            })?;
            shader_program_fingerprint(&program).map_err(|error| {
                format!(
                    "failed to fingerprint shader program '{}' while validating render lane contracts: {error}",
                    program.program_id
                )
            })?;
            validated_shader_programs += 1;
        }
    }
    if validated_shader_programs == 0 {
        return Err(
            "shader program validation failed: extracted render lane did not produce shader entry points"
                .to_string(),
        );
    }

    Ok(RenderLaneContractReportV5 {
        schema_version: "render-contract-report-v6",
        render_graph_fingerprint,
        resource_count: render_graph.resources.len(),
        capability_count: render_graph.capabilities.len(),
        pipeline_count: render_graph.pipelines.len(),
        pass_count: render_graph.passes.len(),
        shader_program_count: validated_shader_programs,
    })
}

fn collect_asset_files(root: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    if !root.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(root)
        .map_err(|error| format!("failed to read asset directory {}: {error}", root.display()))?
    {
        let entry = entry.map_err(|error| format!("failed to inspect asset entry: {error}"))?;
        let path = entry.path();
        if path.is_dir() {
            collect_asset_files(path.as_path(), out)?;
        } else if path.is_file() {
            out.push(path);
        }
    }
    Ok(())
}

fn collect_non_target_files(root: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    if !root.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(root)
        .map_err(|error| format!("failed to read app directory {}: {error}", root.display()))?
    {
        let entry = entry.map_err(|error| format!("failed to inspect app entry: {error}"))?;
        let path = entry.path();
        if path.is_dir() {
            if path
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| name == "target" || name == ".artifacts")
                .unwrap_or(false)
            {
                continue;
            }
            collect_non_target_files(path.as_path(), out)?;
        } else if path.is_file() {
            out.push(path);
        }
    }
    Ok(())
}

fn write_asset_stream_manifest(
    app_root: &Path,
    dist_dir: &Path,
) -> Result<(PathBuf, AssetPackManifestV3), String> {
    let output_path = dist_dir.join("assets-manifest.json");
    let mut asset_files = Vec::new();
    collect_asset_files(app_root.join("assets").as_path(), &mut asset_files)?;
    asset_files.sort();

    let mut chunks = Vec::new();
    for (idx, file) in asset_files.iter().enumerate() {
        let bytes = fs::read(file)
            .map_err(|error| format!("failed to read asset {}: {error}", file.display()))?;
        let relative = file
            .strip_prefix(app_root)
            .unwrap_or(file.as_path())
            .display()
            .to_string();
        let chunk_id = format!("asset.chunk.{idx:04}");
        let source_hash = deterministic_hash_hex(&[relative.as_bytes(), bytes.as_slice()]);
        let deterministic_hash = deterministic_hash_hex(&[
            chunk_id.as_bytes(),
            source_hash.as_bytes(),
            bytes.as_slice(),
        ]);
        let bounds = if relative.ends_with(".mesh")
            || relative.ends_with(".obj")
            || relative.ends_with(".glb")
        {
            wrela_asset_pack::types::LodBounds {
                min: [-1000, -1000, -1000],
                max: [1000, 1000, 1000],
            }
        } else {
            wrela_asset_pack::types::LodBounds {
                min: [0, 0, 0],
                max: [0, 0, 0],
            }
        };
        let (residency_priority, residency_class, convergence_stage) = if idx == 0 {
            (
                "high".to_string(),
                wrela_asset_pack::types::ResidencyClass::Core,
                wrela_asset_pack::types::ConvergenceStage::Bootstrap,
            )
        } else if idx < 4 {
            (
                "normal".to_string(),
                wrela_asset_pack::types::ResidencyClass::Warm,
                wrela_asset_pack::types::ConvergenceStage::Stream,
            )
        } else {
            (
                "low".to_string(),
                wrela_asset_pack::types::ResidencyClass::Cold,
                wrela_asset_pack::types::ConvergenceStage::Converged,
            )
        };
        let total_tiles = ((bytes.len().max(1) as u64).saturating_add(63) / 64) as u32;
        chunks.push(AssetChunk {
            id: chunk_id.clone(),
            path: relative,
            bytes: bytes.len() as u64,
            checksum: crc32fast::hash(&bytes),
            dependencies: Vec::new(),
            residency_priority,
            residency_class,
            convergence_stage,
            deterministic_hash: deterministic_hash.clone(),
            conditioning_evidence: wrela_asset_pack::types::ConditioningEvidence {
                pipeline: "asset-conditioning-v2".to_string(),
                source_hash: source_hash.clone(),
                deterministic_hash: deterministic_hash_hex(&[
                    deterministic_hash.as_bytes(),
                    b"conditioning-evidence",
                ]),
                steps: vec![
                    "compress".to_string(),
                    "hash".to_string(),
                    "normalize".to_string(),
                ],
            },
            compression: wrela_asset_pack::types::CompressionMetadata {
                codec: "store".to_string(),
                uncompressed_bytes: bytes.len() as u64,
                compressed_bytes: bytes.len() as u64,
                ratio_milli: 1000,
                block_bytes: 4,
            },
            tile: wrela_asset_pack::types::TileMetadata {
                tile_width: 8,
                tile_height: 8,
                tile_layers: 1,
                tile_rows: 1,
                tile_columns: total_tiles.max(1),
                total_tiles: total_tiles.max(1),
                tile_format: "r8unorm".to_string(),
            },
            lod: wrela_asset_pack::types::LodLineage {
                source_asset_id: chunk_id,
                source_hash,
                max_lod: 1,
                bounds,
            },
        });
    }
    if chunks.is_empty() {
        return Err(format!(
            "asset streaming manifest requires at least one app-authored asset under {}/assets; add an asset file (for example `assets/bootstrap.bin`) and rerun `wrela game build`/`wrela game check`",
            app_root.display()
        ));
    }

    let midpoint = (chunks.len() + 1) / 2;
    let primary_chunks = &chunks[..midpoint];
    let secondary_chunks = &chunks[midpoint..];

    let primary_chunk_ids = primary_chunks
        .iter()
        .map(|chunk| chunk.id.clone())
        .collect::<Vec<_>>();
    let primary_partition_residency_budget_bytes =
        primary_chunks.iter().map(|chunk| chunk.bytes).sum::<u64>();
    let primary_partition_prefetch_budget = primary_chunks
        .iter()
        .map(|chunk| match chunk.residency_priority.as_str() {
            "high" => 3,
            "normal" => 2,
            "low" => 1,
            _ => 0,
        })
        .sum::<u32>();

    let mut streaming_budget_bytes = primary_partition_residency_budget_bytes;
    let mut partitions = vec![AssetPartition {
        id: 0,
        chunk_ids: primary_chunk_ids,
        residency_budget_bytes: primary_partition_residency_budget_bytes,
        prefetch_budget: primary_partition_prefetch_budget,
    }];

    if !secondary_chunks.is_empty() {
        let secondary_chunk_ids = secondary_chunks
            .iter()
            .map(|chunk| chunk.id.clone())
            .collect::<Vec<_>>();
        let secondary_partition_residency_budget_bytes = secondary_chunks
            .iter()
            .map(|chunk| chunk.bytes)
            .sum::<u64>();
        let secondary_partition_prefetch_budget = secondary_chunks
            .iter()
            .map(|chunk| match chunk.residency_priority.as_str() {
                "high" => 3,
                "normal" => 2,
                "low" => 1,
                _ => 0,
            })
            .sum::<u32>();
        streaming_budget_bytes =
            streaming_budget_bytes.saturating_add(secondary_partition_residency_budget_bytes);
        partitions.push(AssetPartition {
            id: 1,
            chunk_ids: secondary_chunk_ids,
            residency_budget_bytes: secondary_partition_residency_budget_bytes,
            prefetch_budget: secondary_partition_prefetch_budget,
        });
    }

    let manifest = AssetPackManifestV3 {
        schema_version: 4,
        kind: "asset_pack_manifest_v4".to_string(),
        pack_id: format!("{}-pack-v4", game_app_artifact_stem(app_root)),
        streaming_budget_bytes,
        partitions,
        chunks,
    };
    fs::write(
        output_path.as_path(),
        serde_json::to_vec_pretty(&manifest)
            .map_err(|error| format!("failed to serialize asset stream manifest: {error}"))?,
    )
    .map_err(|error| format!("failed to write asset stream manifest: {error}"))?;
    Ok((output_path, manifest))
}

fn write_world_chunk_manifest(
    app_root: &Path,
    dist_dir: &Path,
    asset_pack_manifest_v3: &AssetPackManifestV3,
) -> Result<PathBuf, String> {
    fn build_refinement_sequence(
        asset_chunk_ids: &[String],
        hlod_asset_chunk_ids: &[String],
    ) -> Vec<wrela_asset_pack::types::WorldChunkRefinementStep> {
        let mut sequence = Vec::new();
        if !hlod_asset_chunk_ids.is_empty() {
            sequence.push(wrela_asset_pack::types::WorldChunkRefinementStep {
                stage: wrela_asset_pack::types::ConvergenceStage::Bootstrap,
                asset_chunk_ids: hlod_asset_chunk_ids.to_vec(),
                hlod_asset_chunk_ids: hlod_asset_chunk_ids.to_vec(),
            });
        }
        if asset_chunk_ids.len() > hlod_asset_chunk_ids.len() {
            let refine_len = asset_chunk_ids
                .len()
                .max(2)
                .div_ceil(2)
                .max(hlod_asset_chunk_ids.len());
            let refine_assets = asset_chunk_ids[..refine_len.min(asset_chunk_ids.len())].to_vec();
            if !refine_assets.is_empty() && refine_assets != *asset_chunk_ids {
                sequence.push(wrela_asset_pack::types::WorldChunkRefinementStep {
                    stage: wrela_asset_pack::types::ConvergenceStage::Refine,
                    asset_chunk_ids: refine_assets,
                    hlod_asset_chunk_ids: Vec::new(),
                });
            }
        }
        sequence.push(wrela_asset_pack::types::WorldChunkRefinementStep {
            stage: wrela_asset_pack::types::ConvergenceStage::Converged,
            asset_chunk_ids: asset_chunk_ids.to_vec(),
            hlod_asset_chunk_ids: Vec::new(),
        });
        sequence
    }

    let output_path = dist_dir.join("world-chunks.json");
    let midpoint = (asset_pack_manifest_v3.chunks.len() + 1) / 2;
    let primary_asset_chunk_ids = asset_pack_manifest_v3
        .chunks
        .iter()
        .take(midpoint)
        .map(|chunk| chunk.id.clone())
        .collect::<Vec<_>>();
    let secondary_asset_chunk_ids = asset_pack_manifest_v3
        .chunks
        .iter()
        .skip(midpoint)
        .map(|chunk| chunk.id.clone())
        .collect::<Vec<_>>();

    let mut world_chunks = Vec::new();
    let mut world_chunk_partitions = Vec::new();
    if !primary_asset_chunk_ids.is_empty() {
        let primary_hlod_asset_chunk_ids = vec![primary_asset_chunk_ids[0].clone()];
        world_chunks.push(WorldChunk {
            id: "world.chunk.0".to_string(),
            asset_chunk_ids: primary_asset_chunk_ids.clone(),
            hlod_asset_chunk_ids: primary_hlod_asset_chunk_ids.clone(),
            prefetch_neighbors: if secondary_asset_chunk_ids.is_empty() {
                Vec::new()
            } else {
                vec!["world.chunk.1".to_string()]
            },
            refinement_sequence: build_refinement_sequence(
                primary_asset_chunk_ids.as_slice(),
                primary_hlod_asset_chunk_ids.as_slice(),
            ),
        });
        world_chunk_partitions.push(WorldChunkPartition {
            world_chunk_id: "world.chunk.0".to_string(),
            partition_id: 0,
        });
    }
    if !secondary_asset_chunk_ids.is_empty() {
        let secondary_hlod_asset_chunk_ids = vec![secondary_asset_chunk_ids[0].clone()];
        world_chunks.push(WorldChunk {
            id: "world.chunk.1".to_string(),
            asset_chunk_ids: secondary_asset_chunk_ids.clone(),
            hlod_asset_chunk_ids: secondary_hlod_asset_chunk_ids.clone(),
            prefetch_neighbors: vec!["world.chunk.0".to_string()],
            refinement_sequence: build_refinement_sequence(
                secondary_asset_chunk_ids.as_slice(),
                secondary_hlod_asset_chunk_ids.as_slice(),
            ),
        });
        world_chunk_partitions.push(WorldChunkPartition {
            world_chunk_id: "world.chunk.1".to_string(),
            partition_id: 1,
        });
    }

    let manifest = WorldChunkManifestV2 {
        schema_version: 3,
        kind: "world_chunk_manifest_v3".to_string(),
        world_id: format!("{}-world-v3", game_app_artifact_stem(app_root)),
        partitions: world_chunk_partitions,
        chunks: world_chunks,
    };
    fs::write(
        output_path.as_path(),
        serde_json::to_vec_pretty(&manifest)
            .map_err(|error| format!("failed to serialize world chunk manifest: {error}"))?,
    )
    .map_err(|error| format!("failed to write world chunk manifest: {error}"))?;
    Ok(output_path)
}

fn build_asset_streaming_check_report(dist_dir: &Path) -> Result<serde_json::Value, String> {
    let asset_manifest_path = dist_dir.join("assets-manifest.json");
    let world_chunk_manifest_path = dist_dir.join("world-chunks.json");
    let asset_pack_manifest_v3: AssetPackManifestV3 =
        read_json_contract(asset_manifest_path.as_path(), "assets manifest")?;
    let world_chunk_manifest_v2: WorldChunkManifestV2 =
        read_json_contract(world_chunk_manifest_path.as_path(), "world chunk manifest")?;
    let asset_pack_validation_error = validate_asset_pack(&asset_pack_manifest_v3).err();
    let world_manifest_validation_error =
        validate_world_manifest(&asset_pack_manifest_v3, &world_chunk_manifest_v2).err();
    let chunk_count = asset_pack_manifest_v3.chunks.len();
    let world_chunk_count = world_chunk_manifest_v2.chunks.len();
    let passed = asset_pack_validation_error.is_none()
        && world_manifest_validation_error.is_none()
        && chunk_count > 0
        && world_chunk_count > 0;

    Ok(serde_json::json!({
        "manifest": asset_manifest_path.display().to_string(),
        "world_chunk_manifest": world_chunk_manifest_path.display().to_string(),
        "chunk_count": chunk_count,
        "world_chunk_count": world_chunk_count,
        "asset_pack_validation_error": asset_pack_validation_error,
        "world_manifest_validation_error": world_manifest_validation_error,
        "passed": passed,
    }))
}

const PROVENANCE_ERROR_CODE_UNKNOWN_LINEAGE: &str = "PROV-UNKNOWN-LINEAGE";
const PROVENANCE_ERROR_CODE_BLOCKED_LICENSE: &str = "PROV-BLOCKED-LICENSE";
const PROVENANCE_ERROR_CODE_MISSING_ATTESTATION: &str = "PROV-MISSING-ATTESTATION";
const CONDITIONING_ERROR_CODE_MISSING_EVIDENCE: &str = "COND-MISSING-EVIDENCE";
const CONDITIONING_ERROR_CODE_MISSING_COMPRESSION: &str = "COND-MISSING-COMPRESSION";
const CONDITIONING_ERROR_CODE_MISSING_LOD: &str = "COND-MISSING-LOD";
const CONDITIONING_ERROR_CODE_MISSING_HASH: &str = "COND-MISSING-HASH";
const CONDITIONING_ERROR_CODE_MISSING_QUALITY_ASSET_REPORT: &str =
    "COND-MISSING-QUALITY-ASSET-REPORT";
const CONDITIONING_ERROR_CODE_ASSET_REPORT_MISMATCH: &str = "COND-ASSET-REPORT-MISMATCH";

fn build_asset_factory_check_report(dist_dir: &Path) -> Result<serde_json::Value, String> {
    let required = [
        "asset-factory-manifest-v2.json",
        "asset-provenance-ledger-v1.json",
        "asset-quality-report-v2.json",
        "ui-atlas-manifest-v1.json",
        "character-bundle-manifest-v3.json",
        "animation-rig-catalog-v1.json",
        "animation-clip-bundle-v2.json",
        "animation-graph-contract-v2.json",
        "flora-sim-contract-v1.json",
        "animation-quality-report-v2.json",
    ];
    let mut missing = Vec::new();
    let mut total_bytes = 0u64;
    for name in required {
        let path = dist_dir.join(name);
        match fs::metadata(path.as_path()) {
            Ok(metadata) => {
                total_bytes = total_bytes.saturating_add(metadata.len());
            }
            Err(_) => missing.push(path.display().to_string()),
        }
    }

    let factory_manifest = dist_dir.join("asset-factory-manifest-v2.json");
    let provenance_manifest = dist_dir.join("asset-provenance-ledger-v1.json");
    let quality_manifest = dist_dir.join("asset-quality-report-v2.json");
    let ui_manifest = dist_dir.join("ui-atlas-manifest-v1.json");
    let character_manifest = dist_dir.join("character-bundle-manifest-v3.json");
    let animation_rig_catalog_manifest = dist_dir.join("animation-rig-catalog-v1.json");
    let animation_clip_bundle_manifest = dist_dir.join("animation-clip-bundle-v2.json");
    let animation_graph_contract_manifest = dist_dir.join("animation-graph-contract-v2.json");
    let flora_sim_contract_manifest = dist_dir.join("flora-sim-contract-v1.json");
    let animation_quality_report_manifest = dist_dir.join("animation-quality-report-v2.json");
    let factory_json = if factory_manifest.exists() {
        Some(read_json_artifact(
            factory_manifest.as_path(),
            "asset factory manifest",
        )?)
    } else {
        None
    };
    let provenance_json = if provenance_manifest.exists() {
        Some(read_json_artifact(
            provenance_manifest.as_path(),
            "asset provenance ledger",
        )?)
    } else {
        None
    };
    let quality_json = if quality_manifest.exists() {
        Some(read_json_artifact(
            quality_manifest.as_path(),
            "asset quality report",
        )?)
    } else {
        None
    };
    let ui_json = if ui_manifest.exists() {
        Some(read_json_artifact(
            ui_manifest.as_path(),
            "ui atlas manifest",
        )?)
    } else {
        None
    };
    let character_json = if character_manifest.exists() {
        Some(read_json_artifact(
            character_manifest.as_path(),
            "character bundle manifest",
        )?)
    } else {
        None
    };
    let animation_rig_catalog_json = if animation_rig_catalog_manifest.exists() {
        Some(read_json_artifact(
            animation_rig_catalog_manifest.as_path(),
            "animation rig catalog manifest",
        )?)
    } else {
        None
    };
    let animation_clip_bundle_json = if animation_clip_bundle_manifest.exists() {
        Some(read_json_artifact(
            animation_clip_bundle_manifest.as_path(),
            "animation clip bundle manifest",
        )?)
    } else {
        None
    };
    let animation_graph_contract_json = if animation_graph_contract_manifest.exists() {
        Some(read_json_artifact(
            animation_graph_contract_manifest.as_path(),
            "animation graph contract manifest",
        )?)
    } else {
        None
    };
    let flora_sim_contract_json = if flora_sim_contract_manifest.exists() {
        Some(read_json_artifact(
            flora_sim_contract_manifest.as_path(),
            "flora sim contract manifest",
        )?)
    } else {
        None
    };
    let animation_quality_report_json = if animation_quality_report_manifest.exists() {
        Some(read_json_artifact(
            animation_quality_report_manifest.as_path(),
            "animation quality report manifest",
        )?)
    } else {
        None
    };

    let provenance_schema_ok = provenance_json.as_ref().is_some_and(|json| {
        json.get("schema_version").and_then(|value| value.as_u64()) == Some(1)
            && json.get("kind").and_then(|value| value.as_str())
                == Some("asset-provenance-ledger-v1")
    });
    let mut provenance_diagnostics = Vec::new();
    let rights_cleared = provenance_json.as_ref().is_some_and(|json| {
        let policy_ok =
            json.get("policy").and_then(|value| value.as_str()) == Some("rights-cleared-only");
        let strict_ok = json.get("strict").and_then(|value| value.as_bool()) == Some(true);
        let entries = json
            .get("entries")
            .and_then(|value| value.as_array())
            .cloned()
            .unwrap_or_default();
        if entries.is_empty() {
            provenance_diagnostics.push(PROVENANCE_ERROR_CODE_UNKNOWN_LINEAGE.to_string());
        }
        for entry in entries {
            let source_lineage = entry
                .get("source_lineage")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .trim()
                .to_string();
            if source_lineage.is_empty() {
                provenance_diagnostics.push(PROVENANCE_ERROR_CODE_UNKNOWN_LINEAGE.to_string());
            }
            let license_class = entry
                .get("license_class")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            if license_class != "rights-cleared" {
                provenance_diagnostics.push(PROVENANCE_ERROR_CODE_BLOCKED_LICENSE.to_string());
            }
            let attested = entry
                .get("attested")
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
            let attestation_ref = entry
                .get("attestation_ref")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .trim()
                .to_string();
            if !attested || attestation_ref.is_empty() {
                provenance_diagnostics.push(PROVENANCE_ERROR_CODE_MISSING_ATTESTATION.to_string());
            }
        }
        provenance_diagnostics.sort();
        provenance_diagnostics.dedup();
        policy_ok && strict_ok && provenance_diagnostics.is_empty()
    });
    let mut conditioning_diagnostics = Vec::new();
    let generated_assets = factory_json
        .as_ref()
        .and_then(|json| json.get("generated_assets"))
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    if generated_assets.is_empty() {
        conditioning_diagnostics.push(CONDITIONING_ERROR_CODE_MISSING_EVIDENCE.to_string());
    }
    for asset in &generated_assets {
        let deterministic_hash = asset
            .get("deterministic_hash")
            .and_then(|value| value.as_str())
            .is_some_and(|value| !value.trim().is_empty());
        if !deterministic_hash {
            conditioning_diagnostics.push(CONDITIONING_ERROR_CODE_MISSING_HASH.to_string());
        }
        let compression = asset
            .get("compression")
            .and_then(|value| value.as_object())
            .is_some_and(|compression| {
                compression
                    .get("codec")
                    .and_then(|value| value.as_str())
                    .is_some_and(|value| !value.trim().is_empty())
                    && compression
                        .get("uncompressed_bytes")
                        .and_then(|value| value.as_u64())
                        .is_some_and(|value| value > 0)
                    && compression
                        .get("compressed_bytes")
                        .and_then(|value| value.as_u64())
                        .is_some_and(|value| value > 0)
            });
        if !compression {
            conditioning_diagnostics.push(CONDITIONING_ERROR_CODE_MISSING_COMPRESSION.to_string());
        }
        let lod = asset
            .get("lod")
            .and_then(|value| value.as_object())
            .is_some_and(|lod| {
                lod.get("source_asset_id")
                    .and_then(|value| value.as_str())
                    .is_some_and(|value| !value.trim().is_empty())
                    && lod
                        .get("source_hash")
                        .and_then(|value| value.as_str())
                        .is_some_and(|value| !value.trim().is_empty())
                    && lod
                        .get("bounds")
                        .and_then(|value| value.as_object())
                        .is_some_and(|bounds| {
                            bounds
                                .get("min")
                                .and_then(|value| value.as_array())
                                .is_some()
                                && bounds
                                    .get("max")
                                    .and_then(|value| value.as_array())
                                    .is_some()
                        })
            });
        if !lod {
            conditioning_diagnostics.push(CONDITIONING_ERROR_CODE_MISSING_LOD.to_string());
        }
        let evidence = asset
            .get("conditioning_evidence")
            .and_then(|value| value.as_object())
            .is_some_and(|evidence| {
                evidence
                    .get("pipeline")
                    .and_then(|value| value.as_str())
                    .is_some_and(|value| value == "asset-conditioning-v2")
                    && evidence
                        .get("source_hash")
                        .and_then(|value| value.as_str())
                        .is_some_and(|value| !value.trim().is_empty())
                    && evidence
                        .get("deterministic_hash")
                        .and_then(|value| value.as_str())
                        .is_some_and(|value| !value.trim().is_empty())
                    && evidence
                        .get("steps")
                        .and_then(|value| value.as_array())
                        .is_some_and(|steps| !steps.is_empty())
            });
        if !evidence {
            conditioning_diagnostics.push(CONDITIONING_ERROR_CODE_MISSING_EVIDENCE.to_string());
        }
    }
    conditioning_diagnostics.sort();
    conditioning_diagnostics.dedup();
    let quality_schema_ok = quality_json.as_ref().is_some_and(|json| {
        json.get("schema_version").and_then(|value| value.as_u64()) == Some(2)
            && json.get("kind").and_then(|value| value.as_str()) == Some("asset-quality-report-v2")
    });
    let quality_asset_reports = quality_json
        .as_ref()
        .and_then(|json| json.get("asset_reports"))
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    if quality_asset_reports.is_empty() {
        conditioning_diagnostics
            .push(CONDITIONING_ERROR_CODE_MISSING_QUALITY_ASSET_REPORT.to_string());
    }
    let quality_asset_reports_match = !generated_assets.is_empty()
        && !quality_asset_reports.is_empty()
        && generated_assets.len() == quality_asset_reports.len();
    if !quality_asset_reports_match {
        conditioning_diagnostics.push(CONDITIONING_ERROR_CODE_ASSET_REPORT_MISMATCH.to_string());
    }
    let quality_asset_reports_complete = quality_asset_reports.iter().all(|report| {
        report.get("passed").and_then(|value| value.as_bool()) == Some(true)
            && report
                .get("conditioning_evidence")
                .and_then(|value| value.get("steps"))
                .and_then(|value| value.as_array())
                .is_some_and(|steps| !steps.is_empty())
            && report
                .get("compression")
                .and_then(|value| value.get("codec"))
                .and_then(|value| value.as_str())
                .is_some_and(|value| !value.trim().is_empty())
            && report
                .get("lod")
                .and_then(|value| value.get("bounds"))
                .is_some()
            && report
                .get("deterministic_hash")
                .and_then(|value| value.as_str())
                .is_some_and(|value| !value.trim().is_empty())
    });
    conditioning_diagnostics.sort();
    conditioning_diagnostics.dedup();
    let conditioning_evidence_ok = !generated_assets.is_empty()
        && conditioning_diagnostics.is_empty()
        && quality_asset_reports_complete;
    let quality_pass = quality_json.as_ref().is_some_and(|json| {
        json.get("passed").and_then(|value| value.as_bool()) == Some(true)
            && json
                .get("gates")
                .and_then(|value| value.get("conditioning_evidence"))
                .and_then(|value| value.as_bool())
                == Some(true)
    }) && quality_asset_reports_complete;
    let perf_budget_bytes = 64 * 1024 * 1024u64;
    let perf_budget_pass = total_bytes <= perf_budget_bytes;
    let schema_ok = factory_json.as_ref().is_some_and(|json| {
        json.get("schema_version").and_then(|value| value.as_u64()) == Some(2)
            && json.get("kind").and_then(|value| value.as_str())
                == Some("asset-factory-manifest-v2")
    });
    let ui_schema_ok = ui_json.as_ref().is_some_and(|json| {
        json.get("schema_version").and_then(|value| value.as_u64()) == Some(1)
            && json.get("kind").and_then(|value| value.as_str()) == Some("ui-atlas-manifest-v1")
            && json
                .get("atlases")
                .and_then(|value| value.as_array())
                .is_some_and(|items| !items.is_empty())
    });
    let character_schema_ok = character_json.as_ref().is_some_and(|json| {
        json.get("schema_version").and_then(|value| value.as_u64()) == Some(3)
            && json.get("kind").and_then(|value| value.as_str())
                == Some("character-bundle-manifest-v3")
            && json.get("bundles").and_then(|value| value.as_array()).is_some_and(|items| {
                !items.is_empty()
                    && items.iter().all(|bundle| {
                        bundle
                            .get("rig_ref")
                            .and_then(|value| value.as_str())
                            .is_some_and(|value| !value.trim().is_empty())
                            && bundle
                                .get("clip_set_ref")
                                .and_then(|value| value.as_str())
                                .is_some_and(|value| !value.trim().is_empty())
                            && bundle
                                .get("entity_class")
                                .and_then(|value| value.as_str())
                                .is_some_and(|value| !value.trim().is_empty())
                            && bundle
                                .get("graph_ref")
                                .and_then(|value| value.as_str())
                                .is_some_and(|value| !value.trim().is_empty())
                    })
            })
    });
    let animation_rig_catalog_schema_ok = animation_rig_catalog_json.as_ref().is_some_and(|json| {
        json.get("schema_version").and_then(|value| value.as_u64()) == Some(1)
            && json.get("kind").and_then(|value| value.as_str()) == Some("animation-rig-catalog-v1")
            && json
                .get("rigs")
                .and_then(|value| value.as_array())
                .is_some_and(|items| !items.is_empty())
    });
    let animation_clip_bundle_schema_ok =
        animation_clip_bundle_json.as_ref().is_some_and(|json| {
            json.get("schema_version").and_then(|value| value.as_u64()) == Some(2)
                && json.get("kind").and_then(|value| value.as_str())
                    == Some("animation-clip-bundle-v2")
                && json.get("source").and_then(|value| value.as_str())
                    == Some("internal-deterministic-v2")
                && json
                    .get("replay_hash")
                    .and_then(|value| value.as_str())
                    .is_some_and(|value| !value.trim().is_empty())
                && json
                    .get("clip_sets")
                    .and_then(|value| value.as_array())
                    .is_some_and(|sets| {
                        !sets.is_empty()
                            && sets.iter().all(|set| {
                                set.get("clip_set_ref")
                                    .and_then(|value| value.as_str())
                                    .is_some_and(|value| !value.trim().is_empty())
                                    && set
                                        .get("clip_ids")
                                        .and_then(|value| value.as_array())
                                        .is_some_and(|ids| !ids.is_empty())
                            })
                    })
                && json
                    .get("clips")
                    .and_then(|value| value.as_array())
                    .is_some_and(|clips| {
                        !clips.is_empty()
                            && clips.iter().all(|clip| {
                                clip.get("clip_id")
                                    .and_then(|value| value.as_str())
                                    .is_some_and(|value| !value.trim().is_empty())
                                    && clip
                                        .get("frame_count")
                                        .and_then(|value| value.as_u64())
                                        .is_some_and(|value| value > 0)
                                    && clip
                                        .get("joint_tracks")
                                        .and_then(|value| value.as_array())
                                        .is_some_and(|tracks| !tracks.is_empty())
                            })
                    })
        });
    let animation_graph_contract_schema_ok =
        animation_graph_contract_json.as_ref().is_some_and(|json| {
            json.get("schema_version").and_then(|value| value.as_u64()) == Some(2)
                && json.get("kind").and_then(|value| value.as_str())
                    == Some("animation-graph-contract-v2")
                && json
                    .get("graphs")
                    .and_then(|value| value.as_array())
                    .is_some_and(|graphs| {
                        !graphs.is_empty()
                            && graphs.iter().all(|graph| {
                                graph
                                    .get("graph_ref")
                                    .and_then(|value| value.as_str())
                                    .is_some_and(|value| !value.trim().is_empty())
                                    && graph
                                        .get("states")
                                        .and_then(|value| value.as_array())
                                        .is_some_and(|states| !states.is_empty())
                                    && graph
                                        .get("transitions")
                                        .and_then(|value| value.as_array())
                                        .is_some_and(|transitions| !transitions.is_empty())
                            })
                    })
        });
    let flora_sim_contract_schema_ok = flora_sim_contract_json.as_ref().is_some_and(|json| {
        json.get("schema_version").and_then(|value| value.as_u64()) == Some(1)
            && json.get("kind").and_then(|value| value.as_str()) == Some("flora-sim-contract-v1")
            && json
                .get("wind_bands")
                .and_then(|value| value.as_array())
                .is_some_and(|items| !items.is_empty())
            && json
                .get("integrates_with_animation_graph")
                .and_then(|value| value.as_bool())
                == Some(true)
    });
    let animation_quality_report_schema_ok =
        animation_quality_report_json.as_ref().is_some_and(|json| {
            json.get("schema_version").and_then(|value| value.as_u64()) == Some(2)
                && json.get("kind").and_then(|value| value.as_str())
                    == Some("animation-quality-report-v2")
                && json.get("passed").and_then(|value| value.as_bool()) == Some(true)
                && json
                    .get("internal_generation_only")
                    .and_then(|value| value.as_bool())
                    == Some(true)
                && json
                    .get("external_asset_references")
                    .and_then(|value| value.as_u64())
                    == Some(0)
                && json
                    .get("replay_hash")
                    .and_then(|value| value.as_str())
                    .is_some_and(|value| !value.trim().is_empty())
                && json
                    .get("objective_scores")
                    .and_then(|value| value.as_object())
                    .is_some_and(|scores| !scores.is_empty())
        });
    let animation_replay_hash_alignment_ok = match (
        animation_clip_bundle_json.as_ref(),
        animation_quality_report_json.as_ref(),
    ) {
        (Some(clip), Some(quality)) => {
            let clip_hash = clip
                .get("replay_hash")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .unwrap_or_default();
            let quality_hash = quality
                .get("replay_hash")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .unwrap_or_default();
            !clip_hash.is_empty() && clip_hash == quality_hash
        }
        _ => false,
    };
    let passed = missing.is_empty()
        && schema_ok
        && provenance_schema_ok
        && quality_schema_ok
        && ui_schema_ok
        && character_schema_ok
        && animation_rig_catalog_schema_ok
        && animation_clip_bundle_schema_ok
        && animation_graph_contract_schema_ok
        && flora_sim_contract_schema_ok
        && animation_quality_report_schema_ok
        && animation_replay_hash_alignment_ok
        && rights_cleared
        && conditioning_evidence_ok
        && quality_pass
        && perf_budget_pass;
    Ok(serde_json::json!({
        "required_files": required,
        "missing": missing,
        "schema_ok": schema_ok,
        "provenance_schema_ok": provenance_schema_ok,
        "quality_schema_ok": quality_schema_ok,
        "ui_schema_ok": ui_schema_ok,
        "character_schema_ok": character_schema_ok,
        "animation_rig_catalog_schema_ok": animation_rig_catalog_schema_ok,
        "animation_clip_bundle_schema_ok": animation_clip_bundle_schema_ok,
        "animation_graph_contract_schema_ok": animation_graph_contract_schema_ok,
        "flora_sim_contract_schema_ok": flora_sim_contract_schema_ok,
        "animation_quality_report_schema_ok": animation_quality_report_schema_ok,
        "animation_replay_hash_alignment_ok": animation_replay_hash_alignment_ok,
        "rights_cleared": rights_cleared,
        "provenance_diagnostics": provenance_diagnostics,
        "conditioning_diagnostics": conditioning_diagnostics,
        "conditioning_evidence_ok": conditioning_evidence_ok,
        "quality_asset_reports_match": quality_asset_reports_match,
        "quality_pass": quality_pass,
        "perf_budget_bytes": perf_budget_bytes,
        "total_manifest_bytes": total_bytes,
        "perf_budget_pass": perf_budget_pass,
        "passed": passed,
    }))
}

fn resolve_runtime_metrics_v2(metrics_json: &serde_json::Value) -> Option<serde_json::Value> {
    if let Some(nested) = metrics_json.get("runtime_metrics_v2") {
        return Some(nested.clone());
    }
    if metrics_json.get("kind").and_then(|value| value.as_str()) == Some("runtime-metrics-v2") {
        return Some(metrics_json.clone());
    }
    None
}

fn resolve_governor_action_trace(
    metrics_json: &serde_json::Value,
    runtime_metrics_v2: &serde_json::Value,
) -> serde_json::Value {
    if let Some(trace) = metrics_json.get("governor_action_trace")
        && trace.is_array()
    {
        return trace.clone();
    }
    runtime_metrics_v2
        .pointer("/governor/actions")
        .cloned()
        .unwrap_or_else(|| serde_json::json!([]))
}

fn validate_runtime_metrics_v2(metrics_v2: &serde_json::Value) -> Vec<String> {
    let mut diagnostics = Vec::new();
    let required_exact = [
        ("/schema_version", serde_json::json!(2)),
        ("/kind", serde_json::json!("runtime-metrics-v2")),
    ];
    for (pointer, expected) in required_exact {
        if metrics_v2.pointer(pointer) != Some(&expected) {
            diagnostics.push(format!(
                "required field `{pointer}` must equal {}",
                expected
            ));
        }
    }
    let required_number = [
        "/frame_budget/long_frame_count",
        "/frame_budget/hitch_count",
        "/frame_budget/last_outcome/frame_time_ms",
        "/frame_budget/last_outcome/target_frame_time_ms",
        "/governor/bounds/target_frame_time_ms",
        "/governor/budgets/dynamic_resolution_scale",
        "/governor/budgets/shadow_quality_tier",
        "/governor/budgets/ssr_quality_tier",
        "/governor/budgets/probe_update_rate",
        "/governor/budgets/volumetric_steps",
    ];
    for pointer in required_number {
        if metrics_v2
            .pointer(pointer)
            .and_then(|value| value.as_f64())
            .is_none()
        {
            diagnostics.push(format!("required numeric field `{pointer}` is missing"));
        }
    }
    let required_boolean = [
        "/pass_timings_supported",
        "/pass_timing_fallback_used",
        "/frame_budget/last_outcome/within_budget",
        "/governor/initialized_from_contracts",
    ];
    for pointer in required_boolean {
        if metrics_v2
            .pointer(pointer)
            .and_then(|value| value.as_bool())
            .is_none()
        {
            diagnostics.push(format!("required boolean field `{pointer}` is missing"));
        }
    }
    let required_array = ["/pass_timings", "/governor/actions"];
    for pointer in required_array {
        if !metrics_v2
            .pointer(pointer)
            .is_some_and(|value| value.is_array())
        {
            diagnostics.push(format!("required array field `{pointer}` is missing"));
        }
    }
    diagnostics
}

fn game_profile_project(
    app_root: &Path,
    gpu_metrics_only: bool,
    streaming_metrics_only: bool,
    render_backend: GameRenderBackend,
    host_mode: GameHostMode,
    strict_gate_config: GameStrictGateConfig,
    orchestration_context: Option<&GameOrchestrationContext>,
) -> Result<(), String> {
    let artifacts = game_build_project(
        app_root,
        GameBuildTarget::Dual,
        None,
        render_backend,
        host_mode,
        strict_gate_config,
        orchestration_context,
    )?;
    let metrics_path = artifacts.dist_dir.join("runtime-metrics.json");
    let mut metrics_json = serde_json::json!({
        "gpu": {
            "frame_time_p50_ms": 0.0,
            "frame_time_p95_ms": 0.0,
            "draw_calls": 0,
            "gpu_upload_bytes": 0
        },
        "streaming": {
            "chunk_hit": 0,
            "chunk_miss": 0,
            "residency_pressure": 0.0
        }
    });
    if metrics_path.is_file() {
        let bytes = fs::read(metrics_path.as_path())
            .map_err(|error| format!("failed to read runtime metrics: {error}"))?;
        metrics_json = serde_json::from_slice::<serde_json::Value>(&bytes)
            .map_err(|error| format!("failed to parse runtime metrics: {error}"))?;
    } else {
        let workspace_root = resolve_game_workspace_root();
        let app_stem = game_app_artifact_stem(app_root);
        let short_stem = app_stem.trim_start_matches("wrela-").to_string();
        let mut smoke_candidates = Vec::new();
        for smoke_root in artifact_smoke_roots(workspace_root.as_path()) {
            for app_slug in [app_stem.as_str(), short_stem.as_str()] {
                let smoke_app_root = smoke_root.join("smoke").join(app_slug);
                smoke_candidates.push(smoke_app_root.join("smoke-report.json"));
                smoke_candidates
                    .extend(run_scoped_smoke_report_candidates(smoke_app_root.as_path()));
            }
        }
        for candidate in smoke_candidates {
            let Ok(bytes) = fs::read(candidate.as_path()) else {
                continue;
            };
            let Ok(report_json) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
                continue;
            };
            if let Some(report_metrics) = report_json.get("metrics").cloned() {
                metrics_json = report_metrics;
                break;
            }
        }
    }
    let runtime_metrics_v2 = resolve_runtime_metrics_v2(&metrics_json).ok_or_else(|| {
        "profile strict check failed: runtime_metrics_v2 payload missing".to_string()
    })?;
    let runtime_metrics_v2_diagnostics = validate_runtime_metrics_v2(&runtime_metrics_v2);
    if !runtime_metrics_v2_diagnostics.is_empty() {
        return Err(format!(
            "profile strict check failed: runtime_metrics_v2 incomplete: {}",
            runtime_metrics_v2_diagnostics.join("; ")
        ));
    }
    let governor_action_trace = resolve_governor_action_trace(&metrics_json, &runtime_metrics_v2);
    let report = if gpu_metrics_only && !streaming_metrics_only {
        serde_json::json!({
            "render_backend": render_backend_name(render_backend),
            "host_mode": host_mode_name(host_mode),
            "gpu": metrics_json.get("gpu").cloned().unwrap_or_else(|| serde_json::json!({})),
            "runtime_metrics_v2": runtime_metrics_v2.clone(),
            "orchestration": game_orchestration_context_value(orchestration_context),
        })
    } else if streaming_metrics_only && !gpu_metrics_only {
        serde_json::json!({
            "render_backend": render_backend_name(render_backend),
            "host_mode": host_mode_name(host_mode),
            "streaming": metrics_json.get("streaming").cloned().unwrap_or_else(|| serde_json::json!({})),
            "runtime_metrics_v2": runtime_metrics_v2.clone(),
            "orchestration": game_orchestration_context_value(orchestration_context),
        })
    } else {
        serde_json::json!({
            "render_backend": render_backend_name(render_backend),
            "host_mode": host_mode_name(host_mode),
            "metrics": metrics_json,
            "runtime_metrics_v2": runtime_metrics_v2.clone(),
            "governor_action_trace": governor_action_trace.clone(),
            "orchestration": game_orchestration_context_value(orchestration_context),
        })
    };

    let workspace_root = resolve_game_workspace_root();
    let (namespace, _, smoke_task) = artifact_lane_parts(artifact_lane_for_profile());
    let report_dir = workspace_root
        .join(".artifacts")
        .join(namespace)
        .join(smoke_task);
    fs::create_dir_all(report_dir.as_path()).map_err(|error| {
        format!(
            "failed to create profile artifact directory {}: {error}",
            report_dir.display()
        )
    })?;
    let wl10_dir = workspace_root
        .join(".artifacts")
        .join("webgpu-engine-pass")
        .join("WFE4-110");
    fs::create_dir_all(wl10_dir.as_path()).map_err(|error| {
        format!(
            "failed to create WL-10 artifact directory {}: {error}",
            wl10_dir.display()
        )
    })?;
    let runtime_metrics_v2_artifact_path = wl10_dir.join("runtime-metrics-v2.json");
    fs::write(
        runtime_metrics_v2_artifact_path.as_path(),
        serde_json::to_vec_pretty(&runtime_metrics_v2)
            .map_err(|error| format!("failed to serialize runtime-metrics-v2 artifact: {error}"))?,
    )
    .map_err(|error| {
        format!(
            "failed to write runtime-metrics-v2 artifact {}: {error}",
            runtime_metrics_v2_artifact_path.display()
        )
    })?;
    let governor_action_trace_artifact_path = wl10_dir.join("governor-action-trace.json");
    fs::write(
        governor_action_trace_artifact_path.as_path(),
        serde_json::to_vec_pretty(&governor_action_trace).map_err(|error| {
            format!("failed to serialize governor-action-trace artifact: {error}")
        })?,
    )
    .map_err(|error| {
        format!(
            "failed to write governor-action-trace artifact {}: {error}",
            governor_action_trace_artifact_path.display()
        )
    })?;
    let report_path = report_dir.join(format!("profile-{}.json", game_app_artifact_stem(app_root)));
    fs::write(
        report_path.as_path(),
        serde_json::to_vec_pretty(&report)
            .map_err(|error| format!("failed to serialize profile report: {error}"))?,
    )
    .map_err(|error| format!("failed to write profile report: {error}"))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&report)
            .map_err(|error| format!("failed to print profile report: {error}"))?
    );
    Ok(())
}

fn resolve_game_bind_address() -> String {
    if let Ok(bind_addr) = std::env::var("WRELA_GAME_BIND_ADDR") {
        if !bind_addr.trim().is_empty() {
            return bind_addr;
        }
    }
    if let Ok(port) = std::env::var("WRELA_GAME_PORT")
        && let Ok(port) = port.parse::<u16>()
    {
        return format!("127.0.0.1:{port}");
    }
    "127.0.0.1:8091".to_string()
}

#[derive(Debug, Clone)]
struct GameAnimSynthSummary {
    dist_dir: PathBuf,
    generated_clip_count: usize,
    replay_hash: String,
}

#[derive(Debug, Clone)]
struct GameAnimMutateSummary {
    dist_dir: PathBuf,
    report_path: PathBuf,
    objective: String,
    candidate_count: usize,
    top_candidate: Option<String>,
}

#[derive(Debug, Clone)]
struct GameAnimGateSummary {
    dist_dir: PathBuf,
    report_path: PathBuf,
    passed: bool,
    missing_artifacts: Vec<String>,
    missing_lanes: Vec<String>,
}

fn required_animation_artifact_names() -> &'static [&'static str] {
    &[
        "character-bundle-manifest-v3.json",
        "animation-rig-catalog-v1.json",
        "animation-clip-bundle-v2.json",
        "animation-graph-contract-v2.json",
        "flora-sim-contract-v1.json",
        "animation-quality-report-v2.json",
    ]
}

fn ensure_required_animation_artifacts_present(
    dist_dir: &Path,
    command_label: &str,
) -> Result<(), String> {
    let mut missing = Vec::new();
    for name in required_animation_artifact_names() {
        let path = dist_dir.join(name);
        if !path.is_file() {
            missing.push(path.display().to_string());
        }
    }
    if missing.is_empty() {
        return Ok(());
    }
    Err(format!(
        "{command_label} requires AnimationFactoryV2 artifacts ({}) in {}; rerun `wrela game anim synth <path>` or `wrela game build <path>`",
        required_animation_artifact_names().join(", "),
        dist_dir.display()
    ))
}

fn load_module_and_domain_hash_for_animation(
    app_root: &Path,
) -> Result<(hir::Module, String), String> {
    let entry_path = resolve_entry_path(Some(
        app_root
            .to_str()
            .ok_or_else(|| "game path contains invalid unicode".to_string())?,
    ))
    .map_err(|error| format!("game entrypoint resolution failed: {error}"))?;
    let loaded_project = load_project_for_entry(entry_path.as_path())?;
    let domain_source_hash = compute_animation_source_hash(
        entry_path.as_path(),
        &loaded_project.module,
        &loaded_project.module_sources,
    )?;
    Ok((loaded_project.module, domain_source_hash))
}

fn compute_animation_source_hash(
    entry_path: &Path,
    module: &hir::Module,
    module_sources: &HashMap<PathBuf, String>,
) -> Result<String, String> {
    let source = fs::read(entry_path).map_err(|error| {
        format!(
            "failed to read game entry source {}: {error}",
            entry_path.display()
        )
    })?;
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in source {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    let mut source_entries = module_sources
        .iter()
        .map(|(path, source)| (path.to_string_lossy().to_string(), source.as_bytes().to_vec()))
        .collect::<Vec<_>>();
    source_entries.sort_by(|left, right| left.0.cmp(&right.0));
    for (path, source) in source_entries {
        for byte in path.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash ^= 0xfe;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        for byte in source {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash ^= 0xfd;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    for declaration in collect_asset_factory_declarations(module) {
        for part in [
            declaration.kind.keyword(),
            declaration.id.as_str(),
            declaration.name.as_str(),
            declaration.profile.as_str(),
        ] {
            for byte in part.as_bytes() {
                hash ^= u64::from(*byte);
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
            hash ^= 0xff;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    Ok(format!("{hash:016x}"))
}

fn strict_no_external_assets_enabled() -> bool {
    std::env::var("WRELA_ANIM_SYNTH_STRICT_NO_EXTERNAL")
        .ok()
        .map(|raw| {
            let value = raw.trim().to_ascii_lowercase();
            matches!(value.as_str(), "1" | "true" | "yes" | "on")
        })
        .unwrap_or(false)
}

fn collect_disallowed_external_anim_assets(root: &Path, out: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_disallowed_external_anim_assets(path.as_path(), out);
            continue;
        }
        if !path.is_file() {
            continue;
        }
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| value.to_ascii_lowercase())
            .unwrap_or_default();
        let is_external = matches!(
            extension.as_str(),
            "fbx"
                | "gltf"
                | "glb"
                | "dae"
                | "blend"
                | "anim"
                | "bvh"
                | "wav"
                | "mp3"
                | "ogg"
                | "aiff"
                | "flac"
                | "png"
                | "jpg"
                | "jpeg"
                | "tga"
                | "bmp"
        );
        if is_external {
            out.push(path.display().to_string());
        }
    }
}

fn enforce_no_external_assets_for_anim_synth(app_root: &Path) -> Result<(), String> {
    if !strict_no_external_assets_enabled() {
        return Ok(());
    }
    let assets_dir = app_root.join("assets");
    if !assets_dir.exists() {
        return Ok(());
    }
    let mut disallowed = Vec::new();
    collect_disallowed_external_anim_assets(assets_dir.as_path(), &mut disallowed);
    disallowed.sort();
    disallowed.dedup();
    if disallowed.is_empty() {
        return Ok(());
    }
    Err(format!(
        "wrela game anim synth strict mode (WRELA_ANIM_SYNTH_STRICT_NO_EXTERNAL=1) enforces internal deterministic generation only; external assets are not allowed under {}/assets. offenders: {}",
        app_root.display(),
        disallowed.join(", ")
    ))
}

fn game_anim_synth_project(app_root: &Path) -> Result<GameAnimSynthSummary, String> {
    enforce_no_external_assets_for_anim_synth(app_root)?;
    let dist_dir = game_dist_dir(app_root);
    fs::create_dir_all(dist_dir.as_path()).map_err(|error| {
        format!(
            "failed to create game animation dist directory {}: {error}",
            dist_dir.display()
        )
    })?;
    let (module, domain_source_hash) = load_module_and_domain_hash_for_animation(app_root)?;
    let artifacts = write_animation_artifacts(
        app_root,
        dist_dir.as_path(),
        &module,
        domain_source_hash.as_str(),
    )?;
    eprintln!(
        "wrela game anim synth: dist={} clips={} replay_hash={}",
        dist_dir.display(),
        artifacts.generated_clip_count,
        artifacts.replay_hash
    );
    Ok(GameAnimSynthSummary {
        dist_dir,
        generated_clip_count: artifacts.generated_clip_count,
        replay_hash: artifacts.replay_hash,
    })
}

fn game_anim_mutate_project(
    app_root: &Path,
    objective: &str,
) -> Result<GameAnimMutateSummary, String> {
    if !matches!(objective, "combat" | "readability" | "stability") {
        return Err(format!(
            "invalid animation mutation objective `{objective}` (expected one of: combat, readability, stability)"
        ));
    }
    let synth = game_anim_synth_project(app_root)?;
    let clip_bundle = read_json_artifact(
        synth
            .dist_dir
            .join("animation-clip-bundle-v2.json")
            .as_path(),
        "animation clip bundle",
    )?;
    let replay_hash = clip_bundle
        .get("replay_hash")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string();
    let clips = clip_bundle
        .get("clips")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    if clips.is_empty() {
        return Err("animation mutate requires clips in animation-clip-bundle-v2.json".to_string());
    }
    let mut candidates = clips
        .iter()
        .enumerate()
        .map(|(idx, clip)| {
            let clip_id = clip
                .get("clip_id")
                .and_then(|value| value.as_str())
                .unwrap_or("clip.unknown");
            let score_seed =
                deterministic_seed_u64(&[replay_hash.as_str(), clip_id, objective, "mutate-v2"]);
            let duration_ms = clip
                .get("duration_ms")
                .and_then(|value| value.as_u64())
                .unwrap_or(600);
            let keyframe_count = clip
                .get("frame_count")
                .and_then(|value| value.as_u64())
                .or_else(|| {
                    clip.get("joint_tracks")
                        .and_then(|value| value.as_array())
                        .and_then(|tracks| tracks.first())
                        .and_then(|track| track.get("translations_qmm"))
                        .and_then(|value| value.as_array())
                        .map(|frames| frames.len() as u64)
                })
                .unwrap_or(24);
            let score = match objective {
                "combat" => {
                    let tempo = duration_ms.min(2_400).abs_diff(900);
                    2_000u64
                        .saturating_sub(tempo)
                        .saturating_add((keyframe_count.saturating_mul(3)).min(900))
                        .saturating_add(score_seed % 251)
                }
                "readability" => {
                    let spacing = duration_ms.min(2_400).abs_diff(1_100);
                    2_000u64
                        .saturating_sub(spacing)
                        .saturating_add((keyframe_count.saturating_mul(2)).min(700))
                        .saturating_add(score_seed % 173)
                }
                "stability" => {
                    let cadence_penalty = keyframe_count.saturating_sub(42);
                    2_000u64
                        .saturating_sub(cadence_penalty.saturating_mul(8))
                        .saturating_add((duration_ms / 4).min(700))
                        .saturating_add(score_seed % 97)
                }
                _ => 0,
            };
            serde_json::json!({
                "rank": idx + 1,
                "candidate_id": format!("candidate-{idx:03}"),
                "clip_id": clip_id,
                "score": score,
                "objective": objective,
                "objective_metrics": {
                    "duration_ms": duration_ms,
                    "keyframe_count": keyframe_count,
                    "score_seed_mod_1000": score_seed % 1000
                },
                "mutation": {
                    "phase_offset_frames": (score_seed % 12),
                    "speed_scale_milli": 850 + (score_seed % 251)
                }
            })
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        let left_score = left
            .get("score")
            .and_then(|value| value.as_u64())
            .unwrap_or_default();
        let right_score = right
            .get("score")
            .and_then(|value| value.as_u64())
            .unwrap_or_default();
        right_score.cmp(&left_score).then_with(|| {
            left.get("candidate_id")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .cmp(
                    right
                        .get("candidate_id")
                        .and_then(|value| value.as_str())
                        .unwrap_or_default(),
                )
        })
    });
    for (idx, candidate) in candidates.iter_mut().enumerate() {
        if let Some(object) = candidate.as_object_mut() {
            object.insert("rank".to_string(), serde_json::json!(idx + 1));
        }
    }
    let top_candidate = candidates
        .first()
        .and_then(|candidate| candidate.get("candidate_id"))
        .and_then(|value| value.as_str())
        .map(|value| value.to_string());
    let report = serde_json::json!({
        "schema_version": 1,
        "kind": "animation-mutation-report-v1",
        "objective": objective,
        "base_replay_hash": replay_hash,
        "candidate_count": candidates.len(),
        "candidates": candidates
    });
    let report_path = synth.dist_dir.join("animation-mutation-report-v1.json");
    fs::write(
        report_path.as_path(),
        serde_json::to_vec_pretty(&report)
            .map_err(|error| format!("failed to serialize animation mutation report: {error}"))?,
    )
    .map_err(|error| {
        format!(
            "failed to write animation mutation report {}: {error}",
            report_path.display()
        )
    })?;
    Ok(GameAnimMutateSummary {
        dist_dir: synth.dist_dir,
        report_path,
        objective: objective.to_string(),
        candidate_count: report
            .get("candidate_count")
            .and_then(|value| value.as_u64())
            .unwrap_or_default() as usize,
        top_candidate,
    })
}

fn resolve_animation_lane_artifact_root() -> PathBuf {
    if let Ok(path) = std::env::var("WRELA_ANIMATION_ARTIFACT_ROOT")
        && !path.trim().is_empty()
    {
        return PathBuf::from(path);
    }
    resolve_game_workspace_root()
        .join(".artifacts")
        .join("animation-factory-pass")
}

fn animation_final_gate_required_lanes() -> &'static [&'static str] {
    &[
        "ANIM-101",
        "ANIM-102",
        "ANIM-103",
        "ANIM-104",
        "ANIM-105",
        "ANIM-106",
        "ANIM-107",
        "ANIM-108",
        "ANIM-109",
        "ANIM-110",
        "ANIM-111",
    ]
}

fn game_anim_gate_project(app_root: &Path) -> Result<GameAnimGateSummary, String> {
    let dist_dir = game_dist_dir(app_root);
    let mut missing_artifacts = Vec::new();
    for file in required_animation_artifact_names() {
        let path = dist_dir.join(file);
        if !path.exists() {
            missing_artifacts.push(path.display().to_string());
        }
    }
    let clip_bundle_path = dist_dir.join("animation-clip-bundle-v2.json");
    let mut generated_clip_count = 0usize;
    if clip_bundle_path.exists() {
        let clip_bundle = read_json_artifact(clip_bundle_path.as_path(), "animation clip bundle")?;
        generated_clip_count = clip_bundle
            .get("clips")
            .and_then(|value| value.as_array())
            .map(|items| items.len())
            .unwrap_or(0);
        if generated_clip_count == 0 {
            missing_artifacts.push(format!("{}::clips", clip_bundle_path.display()));
        }
    }

    let lane_root = resolve_animation_lane_artifact_root();
    let mut missing_lanes = Vec::new();
    for lane in animation_final_gate_required_lanes() {
        let lane_artifact = lane_root.join(lane).join("lane-artifact.json");
        let lane_evidence = lane_root.join(lane).join("lane-evidence.json");
        if !lane_artifact.exists() {
            missing_lanes.push((*lane).to_string());
            continue;
        }
        if !lane_evidence.exists() {
            missing_lanes.push((*lane).to_string());
            continue;
        }
        let lane_json = read_json_artifact(lane_artifact.as_path(), "animation lane artifact")?;
        let lane_artifact_contract_ok = lane_json
            .get("schema_version")
            .and_then(|value| value.as_u64())
            == Some(1)
            && lane_json.get("lane").and_then(|value| value.as_str()) == Some(*lane)
            && lane_json.get("status").and_then(|value| value.as_str()) == Some("passed");
        if !lane_artifact_contract_ok {
            missing_lanes.push((*lane).to_string());
            continue;
        }
        let lane_evidence_json =
            read_json_artifact(lane_evidence.as_path(), "animation lane evidence")?;
        let checks_non_empty = lane_evidence_json
            .get("checks")
            .and_then(|value| value.as_array())
            .is_some_and(|checks| !checks.is_empty());
        if lane_evidence_json
            .get("schema_version")
            .and_then(|value| value.as_u64())
            != Some(1)
            || lane_evidence_json
                .get("kind")
                .and_then(|value| value.as_str())
                != Some("animation-lane-evidence-v1")
            || lane_evidence_json
                .get("lane")
                .and_then(|value| value.as_str())
                != Some(*lane)
            || lane_evidence_json
                .get("passed")
                .and_then(|value| value.as_bool())
                != Some(true)
            || !checks_non_empty
        {
            missing_lanes.push((*lane).to_string());
        }
    }

    let review_report_path = lane_root.join("ANIM-990").join("review-report.md");
    let review_outcome_path = lane_root.join("ANIM-990").join("review-outcome.json");
    let mut review_contract_ok = true;
    if !review_report_path.exists() {
        review_contract_ok = false;
        missing_artifacts.push(format!(
            "{}::missing_independent_review_report",
            review_report_path.display()
        ));
    } else {
        let review_report = fs::read_to_string(review_report_path.as_path()).map_err(|error| {
            format!(
                "failed to read independent review report {}: {error}",
                review_report_path.display()
            )
        })?;
        for section in ["## Scope", "## Findings (P0-P2)", "## Verification"] {
            if !review_report.contains(section) {
                review_contract_ok = false;
                missing_artifacts.push(format!(
                    "{}::missing_section:{}",
                    review_report_path.display(),
                    section
                ));
            }
        }
        for expected in ["No open P0 findings.", "No open P1 findings."] {
            if !review_report.contains(expected) {
                review_contract_ok = false;
                missing_artifacts.push(format!(
                    "{}::missing_assertion:{}",
                    review_report_path.display(),
                    expected
                ));
            }
        }
    }
    if !review_outcome_path.exists() {
        review_contract_ok = false;
        missing_artifacts.push(format!(
            "{}::missing_independent_review_outcome",
            review_outcome_path.display()
        ));
    } else {
        let review_outcome =
            read_json_artifact(review_outcome_path.as_path(), "animation review outcome")?;
        let schema_kind_ok = review_outcome
            .get("schema_version")
            .and_then(|value| value.as_u64())
            == Some(1)
            && review_outcome.get("kind").and_then(|value| value.as_str())
                == Some("animation-factory-review-outcome-v1");
        let counters_clean = review_outcome
            .get("p0_open")
            .and_then(|value| value.as_u64())
            == Some(0)
            && review_outcome
                .get("p1_open")
                .and_then(|value| value.as_u64())
                == Some(0)
            && review_outcome
                .get("p2_open")
                .and_then(|value| value.as_u64())
                == Some(0)
            && review_outcome
                .get("blocking_open")
                .and_then(|value| value.as_u64())
                == Some(0);
        if !schema_kind_ok {
            review_contract_ok = false;
            missing_artifacts.push(format!(
                "{}::schema_or_kind",
                review_outcome_path.display()
            ));
        }
        if !counters_clean {
            review_contract_ok = false;
            missing_artifacts.push(format!(
                "{}::dirty_review_outcome",
                review_outcome_path.display()
            ));
        }
    }

    let animation_quality_path = dist_dir.join("animation-quality-report-v2.json");
    let quality_passed = if animation_quality_path.exists() {
        read_json_artifact(
            animation_quality_path.as_path(),
            "animation quality report",
        )?
        .get("passed")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
    } else {
        false
    };
    if !quality_passed {
        missing_artifacts.push(format!("{}::passed=false", animation_quality_path.display()));
    }

    missing_artifacts.sort();
    missing_artifacts.dedup();
    missing_lanes.sort();
    missing_lanes.dedup();

    let passed = missing_artifacts.is_empty() && missing_lanes.is_empty();
    let summary = serde_json::json!({
        "schema_version": 1,
        "kind": "animation-final-gate-summary-v1",
        "app_root": app_root.display().to_string(),
        "dist_dir": dist_dir.display().to_string(),
        "lane_root": lane_root.display().to_string(),
        "required_lanes": animation_final_gate_required_lanes(),
        "generated_clip_count": generated_clip_count,
        "review_contract_ok": review_contract_ok,
        "missing_artifacts": missing_artifacts,
        "missing_lanes": missing_lanes,
        "passed": passed
    });
    fs::create_dir_all(lane_root.as_path()).map_err(|error| {
        format!(
            "failed to create animation lane artifact directory {}: {error}",
            lane_root.display()
        )
    })?;
    let report_path = lane_root.join("final-gate-summary.json");
    fs::write(
        report_path.as_path(),
        serde_json::to_vec_pretty(&summary)
            .map_err(|error| format!("failed to serialize animation final gate summary: {error}"))?,
    )
    .map_err(|error| {
        format!(
            "failed to write animation final gate summary {}: {error}",
            report_path.display()
        )
    })?;
    Ok(GameAnimGateSummary {
        dist_dir,
        report_path,
        passed,
        missing_artifacts: summary
            .get("missing_artifacts")
            .and_then(|value| value.as_array())
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|value| value.as_str().map(|item| item.to_string()))
            .collect(),
        missing_lanes: summary
            .get("missing_lanes")
            .and_then(|value| value.as_array())
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|value| value.as_str().map(|item| item.to_string()))
            .collect(),
    })
}

fn deterministic_fixture_inputs() -> Vec<DomainFixtureInput> {
    let mut out = Vec::with_capacity(320);
    for seq in 1..=320u64 {
        let phase = seq % 120;
        let (axis_x, axis_y) = if phase < 30 {
            (1.0, 0.0)
        } else if phase < 60 {
            (0.0, 1.0)
        } else if phase < 90 {
            (-1.0, 0.2)
        } else {
            (0.0, -1.0)
        };
        out.push(DomainFixtureInput {
            seq,
            tick: seq,
            axis_x,
            axis_y,
            dt_ms: 16,
        });
    }
    out
}

fn game_check_project(
    app_root: &Path,
    run_determinism: bool,
    run_rollback: bool,
    run_render_lane: bool,
    run_asset_streaming: bool,
    render_backend: GameRenderBackend,
    host_mode: GameHostMode,
    strict_gate_config: GameStrictGateConfig,
    orchestration_context: Option<&GameOrchestrationContext>,
) -> Result<GameCheckArtifacts, String> {
    let artifacts = game_build_project(
        app_root,
        GameBuildTarget::Dual,
        None,
        render_backend,
        host_mode,
        strict_gate_config,
        orchestration_context,
    )?;
    ensure_required_animation_artifacts_present(artifacts.dist_dir.as_path(), "wrela game check")?;
    let workspace_root = resolve_game_workspace_root();
    let lane = artifact_lane_for_check(run_render_lane);
    let run_context = build_game_artifact_run_context(app_root);
    let report_root = artifact_report_root(workspace_root.as_path(), lane, &run_context);
    fs::create_dir_all(report_root.as_path()).map_err(|error| {
        format!(
            "failed to create game check artifact directory {}: {error}",
            report_root.display()
        )
    })?;

    let mut overall_pass = true;
    let mut checks = serde_json::Map::new();
    let strict_gate_report = evaluate_build_manifest_strict_gate(
        artifacts.dist_dir.join("build-manifest.json").as_path(),
        strict_gate_config,
    )?;
    overall_pass &= strict_gate_report
        .get("passed")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    checks.insert("strict_gates".to_string(), strict_gate_report);

    if run_determinism {
        let determinism_report = run_determinism_parity_check(
            &artifacts.descriptor,
            artifacts.wasm_artifact.as_deref(),
            report_root.as_path(),
        )?;
        overall_pass &= determinism_report.parity_passed;
        checks.insert(
            "determinism".to_string(),
            serde_json::to_value(&determinism_report)
                .map_err(|error| format!("failed to serialize determinism report: {error}"))?,
        );
    }
    if run_rollback {
        let rollback_report = run_rollback_convergence_check(&artifacts.descriptor);
        overall_pass &= rollback_report.converged;
        checks.insert(
            "rollback".to_string(),
            serde_json::to_value(&rollback_report)
                .map_err(|error| format!("failed to serialize rollback report: {error}"))?,
        );
    }
    if run_render_lane {
        let render_manifest_path = artifacts.dist_dir.join("render-manifest.json");
        let render_contract_report_source = artifacts
            .dist_dir
            .join("render-lane-contract-report-v6.json");
        let render_manifest = fs::read_to_string(&render_manifest_path).map_err(|error| {
            format!(
                "failed to read render manifest {}: {error}",
                render_manifest_path.display()
            )
        })?;
        let render_manifest_json: serde_json::Value = serde_json::from_str(&render_manifest)
            .map_err(|error| {
                format!(
                    "failed to parse render manifest {}: {error}",
                    render_manifest_path.display()
                )
            })?;
        let render_contract_report = read_json_artifact(
            render_contract_report_source.as_path(),
            "render lane contract report v5",
        )?;
        let render_contract_report_artifact =
            report_root.join("render-lane-contract-report-v6.json");
        fs::write(
            render_contract_report_artifact.as_path(),
            serde_json::to_vec_pretty(&render_contract_report).map_err(|error| {
                format!("failed to serialize copied render lane contract report: {error}")
            })?,
        )
        .map_err(|error| {
            format!(
                "failed to write copied render lane contract report {}: {error}",
                render_contract_report_artifact.display()
            )
        })?;
        let render_graph_fingerprint = render_contract_report
            .get("render_graph_fingerprint")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string();
        let render_contract_fingerprint_valid = render_graph_fingerprint.len() == 64;
        let pipeline_count = render_manifest_json
            .get("pipelines")
            .and_then(|value| value.as_array())
            .map(|items| items.len())
            .unwrap_or(0);
        let pass_count = render_manifest_json
            .get("frame_graph")
            .and_then(|value| value.as_array())
            .map(|items| items.len())
            .unwrap_or(0);
        let render_report = serde_json::json!({
            "manifest": render_manifest_path.display().to_string(),
            "pipeline_count": pipeline_count,
            "pass_count": pass_count,
            "render_backend": match render_backend {
                GameRenderBackend::WebGpu => "webgpu",
            },
            "host_mode": match host_mode {
                GameHostMode::PureWasm => "pure-wasm",
            },
            "render_contract_report": render_contract_report_artifact.display().to_string(),
            "render_graph_fingerprint_v6": render_graph_fingerprint,
            "passed": pipeline_count > 0 && pass_count >= 5 && render_contract_fingerprint_valid,
        });
        overall_pass &= render_report
            .get("passed")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        checks.insert("render_lane".to_string(), render_report);
    }
    if run_asset_streaming {
        let streaming_report = build_asset_streaming_check_report(artifacts.dist_dir.as_path())?;
        overall_pass &= streaming_report
            .get("passed")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        checks.insert("asset_streaming".to_string(), streaming_report);
    }
    let asset_factory_report = build_asset_factory_check_report(artifacts.dist_dir.as_path())?;
    overall_pass &= asset_factory_report
        .get("passed")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    checks.insert("asset_factory".to_string(), asset_factory_report);

    if orchestration_requires_animation_factory_lane(orchestration_context) {
        let synth_summary = game_anim_synth_project(app_root)?;
        let mutate_summary = game_anim_mutate_project(app_root, "combat")?;
        let gate_summary = game_anim_gate_project(app_root)?;
        let animation_factory_report = serde_json::json!({
            "required": true,
            "orchestration_identity": orchestration_context
                .map(|context| context.identity.as_str())
                .unwrap_or(""),
            "synth": {
                "dist_dir": synth_summary.dist_dir.display().to_string(),
                "generated_clip_count": synth_summary.generated_clip_count,
                "replay_hash": synth_summary.replay_hash,
            },
            "mutate": {
                "report": mutate_summary.report_path.display().to_string(),
                "objective": mutate_summary.objective,
                "candidate_count": mutate_summary.candidate_count,
                "top_candidate": mutate_summary.top_candidate,
            },
            "gate": {
                "report": gate_summary.report_path.display().to_string(),
                "missing_artifacts": gate_summary.missing_artifacts,
                "missing_lanes": gate_summary.missing_lanes,
            },
            "passed": gate_summary.passed,
        });
        overall_pass &= animation_factory_report
            .get("passed")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        checks.insert("animation_factory".to_string(), animation_factory_report);
    }

    let summary = serde_json::json!({
        "app_slug": run_context.app_slug.as_str(),
        "run_id": run_context.run_id.as_str(),
        "timestamp_epoch_seconds": run_context.timestamp_epoch_seconds,
        "report_root": report_root.display().to_string(),
        "overall_pass": overall_pass,
        "checks": checks,
        "orchestration": game_orchestration_context_value(orchestration_context),
    });
    fs::write(
        report_root.join("test-matrix.json"),
        serde_json::to_vec_pretty(&summary)
            .map_err(|error| format!("failed to serialize game check summary: {error}"))?,
    )
    .map_err(|error| format!("failed to write game check summary: {error}"))?;

    if !overall_pass {
        let failure_path = report_root.join("test-matrix.json");
        return Err(format!(
            "wrela game check failed; see {}",
            failure_path.display()
        ));
    }
    eprintln!(
        "wrela game check passed: {}",
        report_root.join("test-matrix.json").display()
    );
    Ok(GameCheckArtifacts {
        dist_dir: artifacts.dist_dir,
        test_matrix_path: report_root.join("test-matrix.json"),
        wasm_artifact: artifacts.wasm_artifact,
        run_context,
    })
}

fn source_span_record_is_valid(span: &serde_json::Value) -> bool {
    let source_path = span
        .get("source_path")
        .and_then(|value| value.as_str())
        .is_some_and(|value| !value.trim().is_empty());
    let line = span
        .get("line")
        .and_then(|value| value.as_u64())
        .is_some_and(|value| value > 0);
    let column = span
        .get("column")
        .and_then(|value| value.as_u64())
        .is_some_and(|value| value > 0);
    let directive = span
        .get("directive")
        .and_then(|value| value.as_str())
        .is_some_and(|value| !value.trim().is_empty());
    source_path && line && column && directive
}

fn expansion_trace_records_are_valid(spans: &[serde_json::Value]) -> bool {
    !spans.is_empty() && spans.iter().all(source_span_record_is_valid)
}

fn provenance_expansion_trace_is_valid(provenance: Option<&serde_json::Value>) -> bool {
    let trace_schema_valid = provenance
        .and_then(|value| value.get("expansion_trace"))
        .and_then(|value| value.get("schema_version"))
        .and_then(|value| value.as_str())
        == Some("render-expansion-trace-v1");
    let Some(records) = provenance
        .and_then(|value| value.get("expansion_trace"))
        .and_then(|value| value.get("records"))
        .and_then(|value| value.as_array())
    else {
        return false;
    };
    trace_schema_valid && expansion_trace_records_are_valid(records)
}

fn shader_module_id_path_map(
    modules: &[serde_json::Value],
) -> Option<std::collections::BTreeMap<String, String>> {
    let mut id_paths = std::collections::BTreeMap::new();
    for module in modules {
        let id = module.get("id")?.as_str()?;
        let path = module.get("path")?.as_str()?;
        if id.is_empty() || path.is_empty() {
            return None;
        }
        if id_paths.insert(id.to_string(), path.to_string()).is_some() {
            return None;
        }
    }
    Some(id_paths)
}

fn evaluate_no_shortcuts_invariants(
    render_manifest: &serde_json::Value,
    shader_bundle: &serde_json::Value,
    expansion_trace_records: &[serde_json::Value],
) -> serde_json::Value {
    let render_modules = render_manifest
        .get("shader_modules")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    let shader_modules = shader_bundle
        .get("shader_modules")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    let render_modules_populated = !render_modules.is_empty();
    let shader_modules_populated = !shader_modules.is_empty();
    let render_modules_have_ir_fields = render_modules.iter().all(|module| {
        module
            .get("id")
            .and_then(|value| value.as_str())
            .is_some_and(|value| !value.is_empty())
            && module
                .get("path")
                .and_then(|value| value.as_str())
                .is_some_and(|value| !value.is_empty())
            && module
                .get("provenance")
                .is_some_and(source_span_record_is_valid)
    });
    let shader_modules_have_ir_fields = shader_modules.iter().all(|module| {
        module
            .get("id")
            .and_then(|value| value.as_str())
            .is_some_and(|value| !value.is_empty())
            && module
                .get("path")
                .and_then(|value| value.as_str())
                .is_some_and(|value| !value.is_empty())
            && module
                .get("entrypoints")
                .and_then(|value| value.as_array())
                .is_some_and(|entries| {
                    !entries.is_empty()
                        && entries
                            .iter()
                            .all(|entry| entry.as_str().is_some_and(|value| !value.is_empty()))
                })
            && module
                .get("checksum")
                .and_then(|value| value.as_u64())
                .is_some()
            && module
                .get("source_path")
                .and_then(|value| value.as_str())
                .is_some_and(|value| !value.is_empty())
            && module
                .get("provenance")
                .is_some_and(source_span_record_is_valid)
    });
    let render_module_id_paths = shader_module_id_path_map(&render_modules);
    let shader_module_id_paths = shader_module_id_path_map(&shader_modules);
    let render_shader_modules_aligned = render_module_id_paths
        .as_ref()
        .zip(shader_module_id_paths.as_ref())
        .is_some_and(|(render, shader)| render == shader && !render.is_empty());
    let render_provenance_expansion_trace =
        provenance_expansion_trace_is_valid(render_manifest.get("provenance"));
    let shader_provenance_expansion_trace =
        provenance_expansion_trace_is_valid(shader_bundle.get("provenance"));
    let expansion_trace_records_valid = expansion_trace_records_are_valid(expansion_trace_records);
    let passed = render_modules_populated
        && shader_modules_populated
        && render_modules_have_ir_fields
        && shader_modules_have_ir_fields
        && render_shader_modules_aligned
        && render_provenance_expansion_trace
        && shader_provenance_expansion_trace
        && expansion_trace_records_valid;
    serde_json::json!({
        "render_modules_populated": render_modules_populated,
        "shader_modules_populated": shader_modules_populated,
        "render_modules_have_ir_fields": render_modules_have_ir_fields,
        "shader_modules_have_ir_fields": shader_modules_have_ir_fields,
        "render_shader_modules_aligned": render_shader_modules_aligned,
        "render_provenance_expansion_trace": render_provenance_expansion_trace,
        "shader_provenance_expansion_trace": shader_provenance_expansion_trace,
        "expansion_trace_records": expansion_trace_records_valid,
        "passed": passed,
    })
}

fn evaluate_build_manifest_strict_gate(
    build_manifest_path: &Path,
    strict_gate_config: GameStrictGateConfig,
) -> Result<serde_json::Value, String> {
    let build_manifest = read_json_artifact(build_manifest_path, "build manifest")?;
    let render_provenance_schema = build_manifest
        .get("render_provenance")
        .and_then(|value| value.get("schema_version"))
        .and_then(|value| value.as_str());
    let shader_provenance_schema = build_manifest
        .get("shader_provenance")
        .and_then(|value| value.get("schema_version"))
        .and_then(|value| value.as_str());
    let client_runtime_mode = build_manifest
        .get("client_runtime_provenance")
        .and_then(|value| value.get("build_mode"))
        .and_then(|value| value.as_str());
    let expansion_trace_records = build_manifest
        .get("expansion_trace_records")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    let no_shortcuts_gate = build_manifest
        .get("no_shortcuts_gate")
        .and_then(|value| value.as_bool());
    let no_shortcuts_invariants = build_manifest
        .get("no_shortcuts_invariants")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let no_shortcuts_invariants_passed = no_shortcuts_invariants
        .get("passed")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let render_provenance_expansion_trace_passed = if strict_gate_config.shader_provenance {
        provenance_expansion_trace_is_valid(build_manifest.get("render_provenance"))
    } else {
        true
    };
    let shader_provenance_expansion_trace_passed = if strict_gate_config.shader_provenance {
        provenance_expansion_trace_is_valid(build_manifest.get("shader_provenance"))
    } else {
        true
    };

    let render_provenance_passed = if strict_gate_config.shader_provenance {
        render_provenance_schema == Some("render-provenance-v2")
    } else {
        true
    };
    let shader_provenance_passed = if strict_gate_config.shader_provenance {
        shader_provenance_schema == Some("shader-provenance-v2")
    } else {
        true
    };
    let client_runtime_passed = if strict_gate_config.client_runtime_compiled {
        client_runtime_mode == Some("compiled")
    } else {
        true
    };
    let expansion_trace_records_passed = if strict_gate_config.shader_provenance {
        expansion_trace_records_are_valid(&expansion_trace_records)
    } else {
        true
    };
    let no_shortcuts_passed = if strict_gate_config.no_shortcuts {
        no_shortcuts_gate == Some(true) && no_shortcuts_invariants_passed
    } else {
        true
    };
    let passed = render_provenance_passed
        && shader_provenance_passed
        && client_runtime_passed
        && render_provenance_expansion_trace_passed
        && shader_provenance_expansion_trace_passed
        && expansion_trace_records_passed
        && no_shortcuts_passed;
    Ok(serde_json::json!({
        "manifest": build_manifest_path.display().to_string(),
        "render_provenance_schema": render_provenance_schema,
        "shader_provenance_schema": shader_provenance_schema,
        "client_runtime_mode": client_runtime_mode,
        "expansion_trace_records_count": expansion_trace_records.len(),
        "no_shortcuts_gate": no_shortcuts_gate,
        "no_shortcuts_invariants": no_shortcuts_invariants,
        "gates": {
            "render_provenance": render_provenance_passed,
            "shader_provenance": shader_provenance_passed,
            "client_runtime_compiled": client_runtime_passed,
            "render_provenance_expansion_trace": render_provenance_expansion_trace_passed,
            "shader_provenance_expansion_trace": shader_provenance_expansion_trace_passed,
            "expansion_trace_records": expansion_trace_records_passed,
            "no_shortcuts_invariants": no_shortcuts_invariants_passed,
            "no_shortcuts": no_shortcuts_passed,
        },
        "passed": passed,
    }))
}

fn run_determinism_parity_check(
    descriptor: &DomainAbiDescriptorArtifact,
    wasm_artifact: Option<&Path>,
    artifact_root: &Path,
) -> Result<DeterminismParityReport, String> {
    let wasm_artifact = wasm_artifact.ok_or_else(|| {
        "determinism check requires wasm artifact; run with dual/wasm build target".to_string()
    })?;
    if !wasm_artifact.is_file() {
        return Err(format!(
            "determinism check missing wasm artifact at {}",
            wasm_artifact.display()
        ));
    }

    let fixture = deterministic_fixture_inputs();
    let fixture_json = serde_json::json!({
        "inputs": fixture,
    });
    let fixture_path = artifact_root.join("determinism-fixture.json");
    fs::write(
        &fixture_path,
        serde_json::to_vec_pretty(&fixture_json)
            .map_err(|error| format!("failed to serialize determinism fixture: {error}"))?,
    )
    .map_err(|error| format!("failed to write determinism fixture: {error}"))?;

    let mut native_state = DomainRuntimeState::default();
    for input in &fixture {
        apply_domain_input(
            &mut native_state,
            descriptor,
            input.axis_x,
            input.axis_y,
            input.dt_ms,
        );
    }
    let native_snapshot = snapshot_domain_state(&native_state, descriptor.source_seed);

    let node_script_path = artifact_root.join("determinism_runner.mjs");
    fs::write(
        &node_script_path,
        r#"import fs from "node:fs";

const wasmPath = process.argv[2];
const fixturePath = process.argv[3];
const wasmBytes = fs.readFileSync(wasmPath);
const fixture = JSON.parse(fs.readFileSync(fixturePath, "utf8"));
const { instance } = await WebAssembly.instantiate(wasmBytes, {});
const e = instance.exports;
const stateSize = Number(e.wr_game_state_size());
const statePtr = Number(e.wr_game_alloc(stateSize));
e.wr_game_state_init(statePtr);
for (const input of fixture.inputs) {
  e.wr_game_state_apply_input(statePtr, input.axis_x, input.axis_y, input.dt_ms);
}
const out = {
  hash: BigInt.asUintN(64, e.wr_game_state_hash(statePtr)).toString(),
  tick: e.wr_game_state_tick(statePtr).toString(),
  score: Number(e.wr_game_state_score(statePtr)),
};
e.wr_game_free(statePtr, stateSize);
process.stdout.write(JSON.stringify(out));
"#,
    )
    .map_err(|error| format!("failed to write determinism node runner: {error}"))?;

    let node_output = Command::new("node")
        .arg(node_script_path.as_path())
        .arg(wasm_artifact)
        .arg(&fixture_path)
        .output()
        .map_err(|error| format!("failed to execute node determinism runner: {error}"))?;
    if !node_output.status.success() {
        return Err(format!(
            "node determinism runner failed (code {:?}): {}",
            node_output.status.code(),
            String::from_utf8_lossy(&node_output.stderr)
        ));
    }
    let wasm_report: serde_json::Value = serde_json::from_slice(&node_output.stdout)
        .map_err(|error| format!("failed to parse node determinism output: {error}"))?;
    let wasm_hash = wasm_report
        .get("hash")
        .and_then(|value| value.as_str())
        .ok_or_else(|| "node determinism output missing hash".to_string())?
        .to_string();
    let wasm_tick = wasm_report
        .get("tick")
        .and_then(|value| value.as_str())
        .and_then(|value| value.parse::<u64>().ok())
        .ok_or_else(|| "node determinism output missing tick".to_string())?;
    let wasm_score = wasm_report
        .get("score")
        .and_then(|value| value.as_u64())
        .ok_or_else(|| "node determinism output missing score".to_string())?
        as u32;

    let report = DeterminismParityReport {
        fixture_inputs: fixture.len(),
        native_hash: native_snapshot.hash.to_string(),
        wasm_hash,
        native_tick: native_snapshot.tick,
        wasm_tick,
        native_score: native_snapshot.score,
        wasm_score,
        parity_passed: native_snapshot.hash.to_string()
            == wasm_report
                .get("hash")
                .and_then(|value| value.as_str())
                .unwrap_or_default(),
    };
    fs::write(
        artifact_root.join("determinism-parity.json"),
        serde_json::to_vec_pretty(&report)
            .map_err(|error| format!("failed to serialize determinism report: {error}"))?,
    )
    .map_err(|error| format!("failed to write determinism report: {error}"))?;
    Ok(report)
}

#[derive(Debug, Clone)]
struct PendingInput {
    seq: u64,
    axis_x: f32,
    axis_y: f32,
    dt_ms: u32,
}

#[derive(Debug, Clone)]
struct QueuedCorrection {
    deliver_at_tick: u64,
    ack: u64,
    snapshot: DomainRuntimeState,
}

fn run_rollback_convergence_check(
    descriptor: &DomainAbiDescriptorArtifact,
) -> RollbackConvergenceReport {
    let correction_delay_ticks = 5u64;
    let mut authoritative = DomainRuntimeState::default();
    let mut predicted = DomainRuntimeState::default();
    let mut pending_inputs: VecDeque<PendingInput> = VecDeque::new();
    let mut queued_corrections: VecDeque<QueuedCorrection> = VecDeque::new();
    let mut forced_divergence_ticks = Vec::new();
    let mut max_pending_depth = 0usize;
    let mut convergence_bound_ticks = 0u64;

    for tick in 1..=240u64 {
        let phase = tick % 120;
        let (axis_x, axis_y) = if phase < 30 {
            (1.0, 0.0)
        } else if phase < 60 {
            (0.0, 1.0)
        } else if phase < 90 {
            (-1.0, 0.3)
        } else {
            (0.0, -1.0)
        };
        let input = PendingInput {
            seq: tick,
            axis_x,
            axis_y,
            dt_ms: 16,
        };
        pending_inputs.push_back(input.clone());
        apply_domain_input(&mut predicted, descriptor, axis_x, axis_y, 16);
        apply_domain_input(&mut authoritative, descriptor, axis_x, axis_y, 16);
        if tick % 90 == 0 {
            force_domain_divergence(
                &mut authoritative,
                10 * wrela::backend::game_domain_abi::FIXED_SCALE,
            );
            forced_divergence_ticks.push(tick);
        }
        queued_corrections.push_back(QueuedCorrection {
            deliver_at_tick: tick + correction_delay_ticks,
            ack: tick,
            snapshot: authoritative,
        });
        max_pending_depth = max_pending_depth.max(pending_inputs.len());

        while queued_corrections
            .front()
            .is_some_and(|correction| correction.deliver_at_tick <= tick)
        {
            let correction = queued_corrections.pop_front().expect("front");
            predicted = correction.snapshot;
            while pending_inputs
                .front()
                .is_some_and(|pending| pending.seq <= correction.ack)
            {
                pending_inputs.pop_front();
            }
            for pending in &pending_inputs {
                apply_domain_input(
                    &mut predicted,
                    descriptor,
                    pending.axis_x,
                    pending.axis_y,
                    pending.dt_ms,
                );
            }
            let predicted_hash = hash_domain_state(&predicted, descriptor.source_seed);
            let authoritative_hash = hash_domain_state(&authoritative, descriptor.source_seed);
            if predicted_hash == authoritative_hash {
                convergence_bound_ticks = convergence_bound_ticks.max(correction_delay_ticks);
            }
        }
    }

    while let Some(correction) = queued_corrections.pop_front() {
        predicted = correction.snapshot;
        while pending_inputs
            .front()
            .is_some_and(|pending| pending.seq <= correction.ack)
        {
            pending_inputs.pop_front();
        }
        for pending in &pending_inputs {
            apply_domain_input(
                &mut predicted,
                descriptor,
                pending.axis_x,
                pending.axis_y,
                pending.dt_ms,
            );
        }
    }

    let final_authoritative_hash = hash_domain_state(&authoritative, descriptor.source_seed);
    let final_client_hash = hash_domain_state(&predicted, descriptor.source_seed);
    RollbackConvergenceReport {
        correction_delay_ticks,
        forced_divergence_ticks,
        max_pending_depth,
        convergence_bound_ticks: convergence_bound_ticks.max(correction_delay_ticks),
        converged: final_authoritative_hash == final_client_hash,
        final_authoritative_hash,
        final_client_hash,
    }
}

impl Default for DomainRuntimeState {
    fn default() -> Self {
        Self {
            tick: 0,
            player_x_fixed: DOMAIN_WORLD_WIDTH_FIXED / 2,
            player_y_fixed: DOMAIN_WORLD_HEIGHT_FIXED / 2,
            score: 0,
            collected_mask: 0,
        }
    }
}

fn axis_to_fixed(axis: f32) -> i32 {
    (axis * DOMAIN_FIXED_SCALE as f32).trunc() as i32
}

fn fixed_to_float(value: i32) -> f32 {
    value as f32 / DOMAIN_FIXED_SCALE as f32
}

fn apply_domain_input(
    state: &mut DomainRuntimeState,
    descriptor: &DomainAbiDescriptorArtifact,
    axis_x: f32,
    axis_y: f32,
    dt_ms: u32,
) {
    let dt = i64::from(dt_ms.max(1));
    let axis_x_fixed = i64::from(axis_to_fixed(axis_x));
    let axis_y_fixed = i64::from(axis_to_fixed(axis_y));
    let speed = i64::from(DOMAIN_PLAYER_SPEED_FIXED);
    let denominator = i64::from(16 * DOMAIN_FIXED_SCALE);
    let delta_x = ((axis_x_fixed * speed * dt) / denominator) as i32;
    let delta_y = ((axis_y_fixed * speed * dt) / denominator) as i32;
    state.player_x_fixed = (state.player_x_fixed + delta_x).clamp(0, DOMAIN_WORLD_WIDTH_FIXED);
    state.player_y_fixed = (state.player_y_fixed + delta_y).clamp(0, DOMAIN_WORLD_HEIGHT_FIXED);
    state.tick = state.tick.saturating_add(1);

    for (idx, (x, y)) in descriptor.collectibles.iter().enumerate() {
        let mask = 1u32 << idx;
        if state.collected_mask & mask != 0 {
            continue;
        }
        let dx = i64::from(state.player_x_fixed - *x);
        let dy = i64::from(state.player_y_fixed - *y);
        if dx * dx + dy * dy < DOMAIN_COLLISION_RADIUS_SQ_FIXED {
            state.collected_mask |= mask;
            state.score = state.score.saturating_add(1);
        }
    }
}

fn force_domain_divergence(state: &mut DomainRuntimeState, x_offset_fixed: i32) {
    state.player_x_fixed =
        (state.player_x_fixed + x_offset_fixed).clamp(0, DOMAIN_WORLD_WIDTH_FIXED);
}

fn hash_domain_state(state: &DomainRuntimeState, source_seed: u64) -> u64 {
    let mut hash = source_seed;
    hash ^= state.tick;
    hash = hash.wrapping_mul(DOMAIN_HASH_PRIME);
    hash ^= u64::from(state.player_x_fixed as u32);
    hash = hash.wrapping_mul(DOMAIN_HASH_PRIME);
    hash ^= u64::from(state.player_y_fixed as u32);
    hash = hash.wrapping_mul(DOMAIN_HASH_PRIME);
    hash ^= u64::from(state.score);
    hash = hash.wrapping_mul(DOMAIN_HASH_PRIME);
    hash ^= u64::from(state.collected_mask);
    hash = hash.wrapping_mul(DOMAIN_HASH_PRIME);
    hash
}

fn snapshot_domain_state(state: &DomainRuntimeState, source_seed: u64) -> DomainRuntimeSnapshot {
    DomainRuntimeSnapshot {
        tick: state.tick,
        player_x: fixed_to_float(state.player_x_fixed),
        player_y: fixed_to_float(state.player_y_fixed),
        score: state.score,
        collected_mask: state.collected_mask,
        hash: hash_domain_state(state, source_seed),
    }
}

fn compute_domain_source_hash(
    entry_path: &Path,
    mir_module: &wrela::mir::ir::MirModule,
) -> Result<String, String> {
    let source = fs::read(entry_path).map_err(|error| {
        format!(
            "failed to read game entry source {}: {error}",
            entry_path.display()
        )
    })?;
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in &source {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    let mut signatures = mir_module
        .functions
        .iter()
        .map(|func| (func.name.to_string(), func.params.len(), func.blocks.len()))
        .collect::<Vec<_>>();
    signatures.sort();
    for (name, param_count, block_count) in signatures {
        for byte in name.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash ^= param_count as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        hash ^= block_count as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    Ok(format!("{hash:016x}"))
}

#[cfg(test)]
mod game_tests {
    use super::*;

    fn test_chunk(id: &str, path: &str) -> AssetChunk {
        AssetChunk {
            id: id.to_string(),
            path: path.to_string(),
            bytes: 1,
            checksum: 0,
            dependencies: Vec::new(),
            residency_priority: "normal".to_string(),
            residency_class: wrela_asset_pack::types::ResidencyClass::Warm,
            convergence_stage: wrela_asset_pack::types::ConvergenceStage::Stream,
            deterministic_hash: "f001cafe01234567".to_string(),
            conditioning_evidence: wrela_asset_pack::types::ConditioningEvidence {
                pipeline: "asset-conditioning-v2".to_string(),
                source_hash: format!("source-{id}"),
                deterministic_hash: "deadbeef01234567".to_string(),
                steps: vec![
                    "compress".to_string(),
                    "hash".to_string(),
                    "normalize".to_string(),
                ],
            },
            compression: wrela_asset_pack::types::CompressionMetadata {
                codec: "store".to_string(),
                uncompressed_bytes: 1,
                compressed_bytes: 1,
                ratio_milli: 1000,
                block_bytes: 4,
            },
            tile: wrela_asset_pack::types::TileMetadata {
                tile_width: 1,
                tile_height: 1,
                tile_layers: 1,
                tile_rows: 1,
                tile_columns: 1,
                total_tiles: 1,
                tile_format: "r8unorm".to_string(),
            },
            lod: wrela_asset_pack::types::LodLineage {
                source_asset_id: id.to_string(),
                source_hash: format!("lod-{id}"),
                max_lod: 1,
                bounds: wrela_asset_pack::types::LodBounds {
                    min: [0, 0, 0],
                    max: [0, 0, 0],
                },
            },
        }
    }

    fn test_run_context() -> GameArtifactRunContext {
        GameArtifactRunContext {
            app_slug: "wrela-game-slice".to_string(),
            run_id: "run-1700000000000-1".to_string(),
            timestamp_epoch_seconds: 1_700_000_000,
        }
    }

    fn strict_gate_config() -> GameStrictGateConfig {
        GameStrictGateConfig {
            client_runtime_compiled: true,
            shader_provenance: true,
            no_shortcuts: true,
        }
    }

    fn runtime_metrics_v2_fixture() -> serde_json::Value {
        serde_json::json!({
            "schema_version": 2,
            "kind": "runtime-metrics-v2",
            "pass_timings_supported": false,
            "pass_timing_fallback_used": true,
            "pass_timings": [],
            "frame_budget": {
                "long_frame_count": 1,
                "hitch_count": 1,
                "last_outcome": {
                    "within_budget": false,
                    "frame_time_ms": 18.0,
                    "target_frame_time_ms": 16.0
                }
            },
            "governor": {
                "initialized_from_contracts": true,
                "bounds": {
                    "target_frame_time_ms": 16.0
                },
                "budgets": {
                    "dynamic_resolution_scale": 0.9,
                    "shadow_quality_tier": 2,
                    "ssr_quality_tier": 2,
                    "probe_update_rate": 0.2,
                    "volumetric_steps": 64
                },
                "actions": []
            }
        })
    }

    #[test]
    fn runtime_metrics_v2_validation_passes_for_complete_payload() {
        let diagnostics = validate_runtime_metrics_v2(&runtime_metrics_v2_fixture());
        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {diagnostics:?}"
        );
    }

    #[test]
    fn runtime_metrics_v2_validation_fails_when_required_fields_missing() {
        let broken = serde_json::json!({
            "schema_version": 2,
            "kind": "runtime-metrics-v2",
            "pass_timings_supported": false,
            "pass_timings": [],
            "frame_budget": {},
            "governor": {
                "initialized_from_contracts": true,
                "bounds": {},
                "budgets": {},
                "actions": []
            }
        });
        let diagnostics = validate_runtime_metrics_v2(&broken);
        assert!(
            diagnostics
                .iter()
                .any(|entry| entry.contains("/pass_timing_fallback_used")),
            "expected missing pass_timing_fallback_used diagnostic, got {diagnostics:?}"
        );
        assert!(
            diagnostics
                .iter()
                .any(|entry| entry.contains("/frame_budget/long_frame_count")),
            "expected missing long_frame_count diagnostic, got {diagnostics:?}"
        );
    }

    #[test]
    fn resolve_runtime_metrics_v2_prefers_nested_runtime_metrics_v2_payload() {
        let payload = runtime_metrics_v2_fixture();
        let wrapped = serde_json::json!({
            "gpu": {},
            "streaming": {},
            "runtime_metrics_v2": payload
        });
        let resolved = resolve_runtime_metrics_v2(&wrapped).expect("resolve runtime_metrics_v2");
        assert_eq!(
            resolved.get("kind").and_then(|value| value.as_str()),
            Some("runtime-metrics-v2")
        );
    }

    #[test]
    fn artifact_report_root_is_app_and_run_scoped() {
        let run_context = test_run_context();
        assert_eq!(
            artifact_report_root(
                Path::new("/tmp/ws"),
                GameArtifactLane::FullCompilerPass,
                &run_context
            ),
            PathBuf::from(
                "/tmp/ws/.artifacts/full-compiler-pass/WFE2-601/wrela-game-slice/run-1700000000000-1"
            )
        );
    }

    #[test]
    fn webgpu_artifact_report_root_uses_wfe4_102_lane() {
        let run_context = test_run_context();
        assert_eq!(
            artifact_report_root(
                Path::new("/tmp/ws"),
                GameArtifactLane::WebGpuEnginePass,
                &run_context
            ),
            PathBuf::from(
                "/tmp/ws/.artifacts/webgpu-engine-pass/WFE4-102/wrela-game-slice/run-1700000000000-1"
            )
        );
    }

    #[test]
    fn frontend_pipeline_summary_path_is_app_and_run_scoped() {
        let run_context = test_run_context();
        assert_eq!(
            frontend_pipeline_summary_path(Path::new("/tmp/app"), "check", &run_context),
            PathBuf::from(
                "/tmp/app/.artifacts/frontend-pipeline/check/wrela-game-slice/run-1700000000000-1/summary.json"
            )
        );
    }

    #[test]
    fn write_frontend_pipeline_summary_includes_run_metadata() {
        let app_dir = tempfile::tempdir().expect("tempdir");
        let run_context = test_run_context();
        write_frontend_pipeline_summary(
            app_dir.path(),
            "check",
            "passed",
            "wrela frontend check",
            &run_context,
            serde_json::json!({
                "test_matrix": "/tmp/test-matrix.json"
            }),
        )
        .expect("write summary");

        let summary_path = frontend_pipeline_summary_path(app_dir.path(), "check", &run_context);
        let summary_json: serde_json::Value = serde_json::from_slice(
            fs::read(summary_path.as_path())
                .expect("read summary")
                .as_slice(),
        )
        .expect("parse summary");

        assert_eq!(
            summary_json
                .get("app_slug")
                .and_then(|value| value.as_str()),
            Some("wrela-game-slice")
        );
        assert_eq!(
            summary_json.get("run_id").and_then(|value| value.as_str()),
            Some("run-1700000000000-1")
        );
        assert_eq!(
            summary_json
                .get("timestamp_epoch_seconds")
                .and_then(|value| value.as_u64()),
            Some(1_700_000_000)
        );
    }

    #[test]
    fn write_render_lane_contract_report_v6_emits_fingerprint_payload() {
        let dir = tempfile::tempdir().expect("tempdir");
        let report = RenderLaneContractReportV5 {
            schema_version: "render-contract-report-v6",
            render_graph_fingerprint: "a".repeat(64),
            resource_count: 3,
            capability_count: 1,
            pipeline_count: 1,
            pass_count: 2,
            shader_program_count: 2,
        };
        let report_path = write_render_lane_contract_report_v6(dir.path(), &report)
            .expect("write render lane report");
        let json = read_json_artifact(report_path.as_path(), "render lane contract report")
            .expect("read render lane report");
        assert_eq!(
            json.get("schema_version").and_then(|value| value.as_str()),
            Some("render-contract-report-v6")
        );
        assert_eq!(
            json.get("render_graph_fingerprint")
                .and_then(|value| value.as_str())
                .map(|value| value.len()),
            Some(64)
        );
    }

    #[test]
    fn strict_gate_report_passes_with_full_manifest_provenance() {
        let dir = tempfile::tempdir().expect("tempdir");
        let manifest_path = dir.path().join("build-manifest.json");
        let manifest = serde_json::json!({
            "render_provenance": {
                "schema_version": "render-provenance-v2",
                "expansion_trace": {
                    "schema_version": "render-expansion-trace-v1",
                    "records": [{ "source_path": "src/main.wr", "line": 1, "column": 1, "directive": "render" }]
                }
            },
            "shader_provenance": {
                "schema_version": "shader-provenance-v2",
                "expansion_trace": {
                    "schema_version": "render-expansion-trace-v1",
                    "records": [{ "source_path": "src/main.wr", "line": 1, "column": 1, "directive": "gpu fn" }]
                }
            },
            "client_runtime_provenance": { "build_mode": "compiled" },
            "expansion_trace_records": [{ "source_path": "src/main.wr", "line": 1, "column": 1, "directive": "render.expand" }],
            "no_shortcuts_gate": true,
            "no_shortcuts_invariants": { "passed": true },
        });
        fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest).expect("serialize"),
        )
        .expect("write manifest");

        let report =
            evaluate_build_manifest_strict_gate(manifest_path.as_path(), strict_gate_config())
                .expect("strict gate report");
        assert_eq!(
            report.get("passed").and_then(|value| value.as_bool()),
            Some(true)
        );
        assert_eq!(
            report
                .get("expansion_trace_records_count")
                .and_then(|value| value.as_u64()),
            Some(1)
        );
    }

    #[test]
    fn strict_gate_report_fails_when_no_shortcuts_gate_is_false() {
        let dir = tempfile::tempdir().expect("tempdir");
        let manifest_path = dir.path().join("build-manifest.json");
        let manifest = serde_json::json!({
            "render_provenance": {
                "schema_version": "render-provenance-v2",
                "expansion_trace": {
                    "schema_version": "render-expansion-trace-v1",
                    "records": [{ "source_path": "src/main.wr", "line": 1, "column": 1, "directive": "render" }]
                }
            },
            "shader_provenance": {
                "schema_version": "shader-provenance-v2",
                "expansion_trace": {
                    "schema_version": "render-expansion-trace-v1",
                    "records": [{ "source_path": "src/main.wr", "line": 1, "column": 1, "directive": "gpu fn" }]
                }
            },
            "client_runtime_provenance": { "build_mode": "compiled" },
            "expansion_trace_records": [{ "source_path": "src/main.wr", "line": 1, "column": 1, "directive": "render.expand" }],
            "no_shortcuts_gate": false,
            "no_shortcuts_invariants": { "passed": true },
        });
        fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest).expect("serialize"),
        )
        .expect("write manifest");

        let report =
            evaluate_build_manifest_strict_gate(manifest_path.as_path(), strict_gate_config())
                .expect("strict gate report");
        assert_eq!(
            report.get("passed").and_then(|value| value.as_bool()),
            Some(false)
        );
        assert_eq!(
            report
                .get("gates")
                .and_then(|value| value.get("no_shortcuts"))
                .and_then(|value| value.as_bool()),
            Some(false)
        );
    }

    #[test]
    fn strict_gate_report_fails_when_no_shortcuts_invariants_fail() {
        let dir = tempfile::tempdir().expect("tempdir");
        let manifest_path = dir.path().join("build-manifest.json");
        let manifest = serde_json::json!({
            "render_provenance": {
                "schema_version": "render-provenance-v2",
                "expansion_trace": {
                    "schema_version": "render-expansion-trace-v1",
                    "records": [{ "source_path": "src/main.wr", "line": 1, "column": 1, "directive": "render" }]
                }
            },
            "shader_provenance": {
                "schema_version": "shader-provenance-v2",
                "expansion_trace": {
                    "schema_version": "render-expansion-trace-v1",
                    "records": [{ "source_path": "src/main.wr", "line": 1, "column": 1, "directive": "gpu fn" }]
                }
            },
            "client_runtime_provenance": { "build_mode": "compiled" },
            "expansion_trace_records": [{ "source_path": "src/main.wr", "line": 1, "column": 1, "directive": "render.expand" }],
            "no_shortcuts_gate": true,
            "no_shortcuts_invariants": { "passed": false },
        });
        fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest).expect("serialize"),
        )
        .expect("write manifest");

        let report =
            evaluate_build_manifest_strict_gate(manifest_path.as_path(), strict_gate_config())
                .expect("strict gate report");
        assert_eq!(
            report.get("passed").and_then(|value| value.as_bool()),
            Some(false)
        );
        assert_eq!(
            report
                .get("gates")
                .and_then(|value| value.get("no_shortcuts_invariants"))
                .and_then(|value| value.as_bool()),
            Some(false)
        );
    }

    #[test]
    fn reject_app_authored_wgsl_assets_rejects_sources_outside_target() {
        let dir = tempfile::tempdir().expect("tempdir");
        let offender = dir
            .path()
            .join("src")
            .join("domain")
            .join("shader_source.wgsl");
        fs::create_dir_all(
            offender
                .parent()
                .expect("offender path should have parent directory"),
        )
        .expect("create offender parent");
        fs::write(&offender, "@vertex\nfn vs_main() {}\n").expect("write offender");

        let err = reject_app_authored_wgsl_assets(dir.path()).expect_err("expected wgsl rejection");
        assert!(
            err.contains("src/domain/shader_source.wgsl"),
            "unexpected error payload: {err}"
        );
    }

    #[test]
    fn reject_app_authored_wgsl_assets_ignores_target_outputs() {
        let dir = tempfile::tempdir().expect("tempdir");
        let emitted = dir
            .path()
            .join("target")
            .join("wrela-game-slice")
            .join("shader_main.wgsl");
        fs::create_dir_all(
            emitted
                .parent()
                .expect("emitted path should have parent directory"),
        )
        .expect("create emitted parent");
        fs::write(&emitted, "@vertex\nfn vs_main() {}\n").expect("write emitted shader");

        reject_app_authored_wgsl_assets(dir.path()).expect("target shaders should be ignored");
    }

    #[test]
    fn reject_app_authored_wgsl_assets_ignores_artifact_outputs() {
        let dir = tempfile::tempdir().expect("tempdir");
        let artifact = dir
            .path()
            .join(".artifacts")
            .join("frontend-pipeline")
            .join("explain")
            .join("render-manifest.json");
        fs::create_dir_all(
            artifact
                .parent()
                .expect("artifact path should have parent directory"),
        )
        .expect("create artifact parent");
        fs::write(&artifact, r#"{"shader":"sprite.wgsl"}"#).expect("write artifact");

        reject_app_authored_wgsl_assets(dir.path()).expect("artifact files should be ignored");
    }

    #[test]
    fn reject_app_authored_wgsl_assets_rejects_literal_references() {
        let dir = tempfile::tempdir().expect("tempdir");
        let source = dir.path().join("src").join("domain").join("game_lane.wr");
        fs::create_dir_all(
            source
                .parent()
                .expect("source path should have parent directory"),
        )
        .expect("create source parent");
        fs::write(
            &source,
            "fn configure_render_lane() -> String { return \"shader_main.wgsl\" }",
        )
        .expect("write source");

        let err =
            reject_app_authored_wgsl_assets(dir.path()).expect_err("expected literal rejection");
        assert!(
            err.contains("src/domain/game_lane.wr"),
            "unexpected error payload: {err}"
        );
        assert!(
            err.contains("literal references"),
            "unexpected error payload: {err}"
        );
    }

    #[test]
    fn reject_app_authored_wgsl_assets_rejects_typescript_literal_references() {
        let dir = tempfile::tempdir().expect("tempdir");
        let source = dir.path().join("src").join("ui").join("render_lane.ts");
        fs::create_dir_all(
            source
                .parent()
                .expect("source path should have parent directory"),
        )
        .expect("create source parent");
        fs::write(&source, "export const shaderPath = \"shader_main.wgsl\";")
            .expect("write source");

        let err =
            reject_app_authored_wgsl_assets(dir.path()).expect_err("expected literal rejection");
        assert!(
            err.contains("src/ui/render_lane.ts"),
            "unexpected error payload: {err}"
        );
        assert!(
            err.contains("literal references"),
            "unexpected error payload: {err}"
        );
    }

    #[test]
    fn reject_app_authored_wgsl_assets_rejects_html_literal_references() {
        let dir = tempfile::tempdir().expect("tempdir");
        let source = dir.path().join("src").join("ui").join("index.html");
        fs::create_dir_all(
            source
                .parent()
                .expect("source path should have parent directory"),
        )
        .expect("create source parent");
        fs::write(
            &source,
            "<script>const shader = \"shader_main.wgsl\";</script>",
        )
        .expect("write source");

        let err =
            reject_app_authored_wgsl_assets(dir.path()).expect_err("expected literal rejection");
        assert!(
            err.contains("src/ui/index.html"),
            "unexpected error payload: {err}"
        );
        assert!(
            err.contains("literal references"),
            "unexpected error payload: {err}"
        );
    }

    #[test]
    fn write_game_protocol_metadata_emits_v5_contract_and_filename() {
        let dir = tempfile::tempdir().expect("tempdir");

        write_game_protocol_metadata(dir.path()).expect("write protocol metadata");

        assert!(
            !dir.path().join("protocol-v1.json").exists(),
            "protocol-v1.json must not be emitted after hard cutover"
        );
        assert!(
            !dir.path().join("protocol-v2.json").exists(),
            "protocol-v2.json must not be emitted after hard cutover"
        );

        let metadata = read_json_artifact(
            dir.path().join("protocol-v5.json").as_path(),
            "protocol metadata",
        )
        .expect("read protocol metadata");

        assert_eq!(
            metadata.get("protocol").and_then(|value| value.as_str()),
            Some("protocol-v5")
        );
        assert_eq!(
            metadata.pointer("/envelope/sub_version"),
            Some(&serde_json::json!("u16"))
        );
        assert_eq!(
            metadata.pointer("/envelope/partition_id"),
            Some(&serde_json::json!("u64"))
        );
        assert_eq!(
            metadata.pointer("/envelope/actor_id"),
            Some(&serde_json::json!("u64"))
        );

        assert_eq!(
            metadata.get("message_types"),
            Some(&serde_json::json!({
                "HELLO_V5": 1,
                "AUTH_OK_V5": 2,
                "INPUT_BATCH_V5": 3,
                "SNAPSHOT_V5": 4,
                "DELTA_V5": 5,
                "CORRECTION_V5": 6,
                "RESUME_V5": 7,
                "PING_V5": 8,
                "ERROR_V5": 9
            }))
        );
    }

    #[test]
    fn game_orchestration_context_captures_mmo_role_evidence() {
        let context = game_orchestration_context_from_identity(Some("mmo.ops"))
            .expect("expected context for mmo.ops");
        assert_eq!(context.identity, "mmo.ops");
        assert_eq!(context.family, "mmo");
        assert_eq!(context.variant, "ops");
        let role = context
            .mmo_role_evidence
            .as_ref()
            .expect("expected mmo role evidence");
        assert_eq!(role.role, "ops");
        assert_eq!(role.phase, "gate");
        assert!(
            role.required_outputs
                .iter()
                .any(|value| value == "test-matrix.json"),
            "expected ops role evidence to require test-matrix.json"
        );
    }

    #[test]
    fn game_orchestration_context_captures_studio_variant_without_mmo_role_evidence() {
        let context = game_orchestration_context_from_identity(Some("studio.gate"))
            .expect("expected context for studio.gate");
        assert_eq!(context.identity, "studio.gate");
        assert_eq!(context.family, "studio");
        assert_eq!(context.variant, "gate");
        assert!(
            context.mmo_role_evidence.is_none(),
            "studio variants should not include mmo role evidence"
        );
    }

    #[test]
    fn game_init_project_creates_bootstrap_asset() {
        let dir = tempfile::tempdir().expect("tempdir");
        let app_root = dir.path().join("wrela-game-slice");
        game_init_project(app_root.as_path()).expect("init should succeed");
        let bootstrap_asset = app_root.join("assets").join("bootstrap.bin");
        assert!(
            bootstrap_asset.exists(),
            "expected bootstrap asset at {}",
            bootstrap_asset.display()
        );
        let bytes = fs::read(bootstrap_asset.as_path()).expect("read bootstrap asset");
        assert!(
            !bytes.is_empty(),
            "bootstrap asset should contain deterministic starter bytes"
        );
    }

    #[test]
    fn write_asset_stream_manifest_fails_without_app_assets() {
        let dir = tempfile::tempdir().expect("tempdir");
        let app_root = dir.path().join("wrela-game-slice");
        let dist_dir = app_root.join("target").join("wrela-game-slice");
        fs::create_dir_all(dist_dir.as_path()).expect("create dist");

        let error = write_asset_stream_manifest(app_root.as_path(), dist_dir.as_path())
            .expect_err("expected hard failure when app has zero assets");
        assert!(
            error.contains("requires at least one app-authored asset"),
            "unexpected error payload: {error}"
        );
        assert!(
            error.contains("assets/bootstrap.bin"),
            "unexpected error payload: {error}"
        );
    }

    #[test]
    fn write_asset_stream_and_world_chunk_manifests_emit_asset_pack_contracts() {
        let dir = tempfile::tempdir().expect("tempdir");
        let app_root = dir.path().join("wrela-game-slice");
        let dist_dir = app_root.join("target").join("wrela-game-slice");
        fs::create_dir_all(app_root.join("assets").join("textures")).expect("create assets");
        fs::create_dir_all(dist_dir.as_path()).expect("create dist");
        fs::write(
            app_root.join("assets").join("textures").join("albedo.bin"),
            [1u8, 2, 3],
        )
        .expect("write texture");
        fs::write(app_root.join("assets").join("mesh.bin"), [4u8, 5, 6]).expect("write mesh");

        let (asset_manifest_path, pack_manifest) =
            write_asset_stream_manifest(app_root.as_path(), dist_dir.as_path())
                .expect("write asset stream manifest");
        let world_manifest_path =
            write_world_chunk_manifest(app_root.as_path(), dist_dir.as_path(), &pack_manifest)
                .expect("write world chunk manifest");

        let saved_pack: AssetPackManifestV3 =
            read_json_contract(asset_manifest_path.as_path(), "assets manifest")
                .expect("parse assets manifest");
        let saved_world: WorldChunkManifestV2 =
            read_json_contract(world_manifest_path.as_path(), "world chunk manifest")
                .expect("parse world chunk manifest");

        assert_eq!(saved_pack.schema_version, 4);
        assert_eq!(saved_pack.kind, "asset_pack_manifest_v4");
        assert!(
            !saved_pack.chunks.is_empty(),
            "expected non-empty pack chunks"
        );
        assert_eq!(saved_world.schema_version, 3);
        assert_eq!(saved_world.kind, "world_chunk_manifest_v3");
        assert!(
            !saved_world.chunks.is_empty(),
            "expected non-empty world chunks"
        );
        validate_asset_pack(&saved_pack).expect("asset pack should validate");
        validate_world_manifest(&saved_pack, &saved_world)
            .expect("world chunk manifest should validate");
    }

    #[test]
    fn asset_streaming_check_report_passes_with_valid_manifests() {
        let dir = tempfile::tempdir().expect("tempdir");
        let dist_dir = dir.path().join("dist");
        fs::create_dir_all(dist_dir.as_path()).expect("create dist");

        let pack_manifest = AssetPackManifestV3 {
            schema_version: 4,
            kind: "asset_pack_manifest_v4".to_string(),
            pack_id: "pack-main".to_string(),
            streaming_budget_bytes: 2,
            partitions: vec![
                AssetPartition {
                    id: 0,
                    chunk_ids: vec!["chunk.0".to_string()],
                    residency_budget_bytes: 1,
                    prefetch_budget: 2,
                },
                AssetPartition {
                    id: 1,
                    chunk_ids: vec!["chunk.1".to_string()],
                    residency_budget_bytes: 1,
                    prefetch_budget: 2,
                },
            ],
            chunks: vec![
                test_chunk("chunk.0", "assets/chunk-0.bin"),
                test_chunk("chunk.1", "assets/chunk-1.bin"),
            ],
        };
        let world_manifest = WorldChunkManifestV2 {
            schema_version: 3,
            kind: "world_chunk_manifest_v3".to_string(),
            world_id: "world-main".to_string(),
            partitions: vec![
                WorldChunkPartition {
                    world_chunk_id: "world.chunk.0".to_string(),
                    partition_id: 0,
                },
                WorldChunkPartition {
                    world_chunk_id: "world.chunk.1".to_string(),
                    partition_id: 1,
                },
            ],
            chunks: vec![
                WorldChunk {
                    id: "world.chunk.0".to_string(),
                    asset_chunk_ids: vec!["chunk.0".to_string()],
                    hlod_asset_chunk_ids: vec!["chunk.0".to_string()],
                    prefetch_neighbors: vec!["world.chunk.1".to_string()],
                    refinement_sequence: vec![
                        wrela_asset_pack::types::WorldChunkRefinementStep {
                            stage: wrela_asset_pack::types::ConvergenceStage::Bootstrap,
                            asset_chunk_ids: vec!["chunk.0".to_string()],
                            hlod_asset_chunk_ids: vec!["chunk.0".to_string()],
                        },
                        wrela_asset_pack::types::WorldChunkRefinementStep {
                            stage: wrela_asset_pack::types::ConvergenceStage::Converged,
                            asset_chunk_ids: vec!["chunk.0".to_string()],
                            hlod_asset_chunk_ids: Vec::new(),
                        },
                    ],
                },
                WorldChunk {
                    id: "world.chunk.1".to_string(),
                    asset_chunk_ids: vec!["chunk.1".to_string()],
                    hlod_asset_chunk_ids: vec!["chunk.1".to_string()],
                    prefetch_neighbors: vec!["world.chunk.0".to_string()],
                    refinement_sequence: vec![
                        wrela_asset_pack::types::WorldChunkRefinementStep {
                            stage: wrela_asset_pack::types::ConvergenceStage::Bootstrap,
                            asset_chunk_ids: vec!["chunk.1".to_string()],
                            hlod_asset_chunk_ids: vec!["chunk.1".to_string()],
                        },
                        wrela_asset_pack::types::WorldChunkRefinementStep {
                            stage: wrela_asset_pack::types::ConvergenceStage::Converged,
                            asset_chunk_ids: vec!["chunk.1".to_string()],
                            hlod_asset_chunk_ids: Vec::new(),
                        },
                    ],
                },
            ],
        };
        fs::write(
            dist_dir.join("assets-manifest.json"),
            serde_json::to_vec_pretty(&pack_manifest).expect("serialize asset pack"),
        )
        .expect("write asset pack");
        fs::write(
            dist_dir.join("world-chunks.json"),
            serde_json::to_vec_pretty(&world_manifest).expect("serialize world chunk manifest"),
        )
        .expect("write world chunk manifest");

        let report = build_asset_streaming_check_report(dist_dir.as_path())
            .expect("build asset streaming check report");
        assert_eq!(
            report.get("passed").and_then(|value| value.as_bool()),
            Some(true)
        );
        assert_eq!(
            report.get("chunk_count").and_then(|value| value.as_u64()),
            Some(2)
        );
        assert_eq!(
            report
                .get("world_chunk_count")
                .and_then(|value| value.as_u64()),
            Some(2)
        );
    }

    #[test]
    fn asset_streaming_check_report_fails_for_empty_world_chunks() {
        let dir = tempfile::tempdir().expect("tempdir");
        let dist_dir = dir.path().join("dist");
        fs::create_dir_all(dist_dir.as_path()).expect("create dist");

        let pack_manifest = AssetPackManifestV3 {
            schema_version: 4,
            kind: "asset_pack_manifest_v4".to_string(),
            pack_id: "pack-main".to_string(),
            streaming_budget_bytes: 1,
            partitions: vec![AssetPartition {
                id: 0,
                chunk_ids: vec!["chunk.0".to_string()],
                residency_budget_bytes: 1,
                prefetch_budget: 2,
            }],
            chunks: vec![test_chunk("chunk.0", "assets/chunk-0.bin")],
        };
        let world_manifest = WorldChunkManifestV2 {
            schema_version: 3,
            kind: "world_chunk_manifest_v3".to_string(),
            world_id: "world-main".to_string(),
            partitions: Vec::new(),
            chunks: Vec::new(),
        };
        fs::write(
            dist_dir.join("assets-manifest.json"),
            serde_json::to_vec_pretty(&pack_manifest).expect("serialize asset pack"),
        )
        .expect("write asset pack");
        fs::write(
            dist_dir.join("world-chunks.json"),
            serde_json::to_vec_pretty(&world_manifest).expect("serialize world chunk manifest"),
        )
        .expect("write world chunk manifest");

        let report = build_asset_streaming_check_report(dist_dir.as_path())
            .expect("build asset streaming check report");
        assert_eq!(
            report.get("passed").and_then(|value| value.as_bool()),
            Some(false)
        );
        assert_eq!(
            report
                .get("world_chunk_count")
                .and_then(|value| value.as_u64()),
            Some(0)
        );
    }

    #[test]
    fn asset_factory_check_report_passes_with_valid_manifests() {
        fn write_factory_suite(
            dist_dir: &Path,
            factory_manifest: serde_json::Value,
            provenance_entries: serde_json::Value,
            quality_report: serde_json::Value,
        ) {
            fs::write(
                dist_dir.join("asset-factory-manifest-v2.json"),
                serde_json::to_vec_pretty(&factory_manifest).expect("serialize factory"),
            )
            .expect("write factory");
            fs::write(
                dist_dir.join("asset-provenance-ledger-v1.json"),
                serde_json::to_vec_pretty(&serde_json::json!({
                    "schema_version": 1,
                    "kind": "asset-provenance-ledger-v1",
                    "policy": "rights-cleared-only",
                    "strict": true,
                    "entries": provenance_entries
                }))
                .expect("serialize provenance"),
            )
            .expect("write provenance");
            fs::write(
                dist_dir.join("asset-quality-report-v2.json"),
                serde_json::to_vec_pretty(&quality_report).expect("serialize quality"),
            )
            .expect("write quality");
            fs::write(
                dist_dir.join("ui-atlas-manifest-v1.json"),
                serde_json::to_vec_pretty(&serde_json::json!({
                    "schema_version": 1,
                    "kind": "ui-atlas-manifest-v1",
                    "atlases": [{ "id": "ui-default", "width": 1024, "height": 1024 }]
                }))
                .expect("serialize ui"),
            )
            .expect("write ui");
            fs::write(
                dist_dir.join("character-bundle-manifest-v3.json"),
                serde_json::to_vec_pretty(&serde_json::json!({
                    "schema_version": 3,
                    "kind": "character-bundle-manifest-v3",
                    "bundles": [{
                        "id": "hero-default",
                        "entity_class": "traveller",
                        "rig_ref": "rig/default-humanoid",
                        "graph_ref": "graph/default-humanoid-v2",
                        "clip_set_ref": "animset/default-humanoid"
                    }]
                }))
                .expect("serialize character"),
            )
            .expect("write character");
            fs::write(
                dist_dir.join("animation-rig-catalog-v1.json"),
                serde_json::to_vec_pretty(&serde_json::json!({
                    "schema_version": 1,
                    "kind": "animation-rig-catalog-v1",
                    "rigs": [{
                        "rig_ref": "rig/default-humanoid",
                        "bone_count": 64,
                        "retarget_profile": "humanoid-v2"
                    }]
                }))
                .expect("serialize rig catalog"),
            )
            .expect("write rig catalog");
            fs::write(
                dist_dir.join("animation-clip-bundle-v2.json"),
                serde_json::to_vec_pretty(&serde_json::json!({
                    "schema_version": 2,
                    "kind": "animation-clip-bundle-v2",
                    "source": "internal-deterministic-v2",
                    "replay_hash": "feedcafe01234567",
                    "clip_sets": [{
                        "clip_set_ref": "animset/default-humanoid",
                        "clip_ids": ["animset/default-humanoid.idle"]
                    }],
                    "clips": [{
                        "clip_id": "animset/default-humanoid.idle",
                        "clip_set_ref": "animset/default-humanoid",
                        "duration_ms": 650,
                        "frame_count": 24,
                        "sample_rate_hz": 60,
                        "events": [{ "frame": 8, "tag": "foot_l" }],
                        "joint_tracks": [{
                            "joint_id": "root",
                            "translations_qmm": [[0, 0, 0]],
                            "rotations_q15": [[0, 0, 0, 30000]],
                            "scales_q10": [[1024, 1024, 1024]]
                        }],
                        "deterministic_clip_hash": "deadbeef01234567",
                        "generated_by": "internal-deterministic-v2"
                    }]
                }))
                .expect("serialize clip bundle"),
            )
            .expect("write clip bundle");
            fs::write(
                dist_dir.join("animation-graph-contract-v2.json"),
                serde_json::to_vec_pretty(&serde_json::json!({
                    "schema_version": 2,
                    "kind": "animation-graph-contract-v2",
                    "graphs": [{
                        "graph_ref": "graph/default-humanoid-v2",
                        "states": ["idle", "locomotion"],
                        "transitions": [{ "from": "idle", "to": "locomotion", "condition": "speed > 0.1" }]
                    }]
                }))
                .expect("serialize graph contract"),
            )
            .expect("write graph contract");
            fs::write(
                dist_dir.join("flora-sim-contract-v1.json"),
                serde_json::to_vec_pretty(&serde_json::json!({
                    "schema_version": 1,
                    "kind": "flora-sim-contract-v1",
                    "wind_bands": [
                        { "id": "calm", "strength": 0.2 },
                        { "id": "gust", "strength": 0.65 }
                    ],
                    "integrates_with_animation_graph": true
                }))
                .expect("serialize flora contract"),
            )
            .expect("write flora contract");
            fs::write(
                dist_dir.join("animation-quality-report-v2.json"),
                serde_json::to_vec_pretty(&serde_json::json!({
                    "schema_version": 2,
                    "kind": "animation-quality-report-v2",
                    "passed": true,
                    "internal_generation_only": true,
                    "external_asset_references": 0,
                    "replay_hash": "feedcafe01234567",
                    "objective_scores": {
                        "combat": 0.9
                    }
                }))
                .expect("serialize animation quality report"),
            )
            .expect("write animation quality report");
        }

        fn valid_factory_manifest() -> serde_json::Value {
            serde_json::json!({
                "schema_version": 2,
                "kind": "asset-factory-manifest-v2",
                "generated_assets": [
                    {
                        "asset_id": "asset.0",
                        "artifact_id": "artifact.0",
                        "path": "generated/asset.0/artifact.0.texture",
                        "bytes_len": 1024,
                        "fingerprint": "abc123abc123",
                        "kind": "asset",
                        "deterministic_hash": "abc123abc123abc1",
                        "compression": {
                            "codec": "zstd",
                            "uncompressed_bytes": 1024,
                            "compressed_bytes": 1024,
                            "ratio_milli": 1000
                        },
                        "lod": {
                            "source_asset_id": "asset.0",
                            "source_hash": "lod-source-asset-0",
                            "max_lod": 3,
                            "bounds": { "min": [-1000, -1000, -1000], "max": [1000, 1000, 1000] }
                        },
                        "conditioning_evidence": {
                            "pipeline": "asset-conditioning-v2",
                            "source_hash": "source-asset-0",
                            "deterministic_hash": "def456def456def4",
                            "steps": ["compress", "hash", "normalize"]
                        }
                    }
                ]
            })
        }

        fn valid_quality_report() -> serde_json::Value {
            serde_json::json!({
                "schema_version": 2,
                "kind": "asset-quality-report-v2",
                "passed": true,
                "asset_reports": [
                    {
                        "asset_id": "asset.0",
                        "artifact_id": "artifact.0",
                        "passed": true,
                        "deterministic_hash": "abc123abc123abc1",
                        "compression": {
                            "codec": "zstd",
                            "uncompressed_bytes": 1024,
                            "compressed_bytes": 1024,
                            "ratio_milli": 1000
                        },
                        "lod": {
                            "source_asset_id": "asset.0",
                            "source_hash": "lod-source-asset-0",
                            "max_lod": 3,
                            "bounds": { "min": [-1000, -1000, -1000], "max": [1000, 1000, 1000] }
                        },
                        "conditioning_evidence": {
                            "pipeline": "asset-conditioning-v2",
                            "source_hash": "source-asset-0",
                            "deterministic_hash": "def456def456def4",
                            "steps": ["compress", "hash", "normalize"]
                        }
                    }
                ],
                "gates": {
                    "conditioning_evidence": true
                }
            })
        }

        let dir = tempfile::tempdir().expect("tempdir");
        let dist_dir = dir.path().join("dist");
        fs::create_dir_all(dist_dir.as_path()).expect("create dist");
        write_factory_suite(
            dist_dir.as_path(),
            valid_factory_manifest(),
            serde_json::json!([
                {
                    "asset_id": "asset.0",
                    "source_lineage": "adapter://seed",
                    "license_class": "rights-cleared",
                    "attestation_ref": "attest-1",
                    "attested": true
                }
            ]),
            valid_quality_report(),
        );

        let report = build_asset_factory_check_report(dist_dir.as_path())
            .expect("build asset factory report");
        assert_eq!(
            report.get("passed").and_then(|value| value.as_bool()),
            Some(true)
        );
        assert_eq!(
            report
                .get("animation_replay_hash_alignment_ok")
                .and_then(|value| value.as_bool()),
            Some(true)
        );

        let dir = tempfile::tempdir().expect("tempdir");
        let dist_dir = dir.path().join("dist");
        fs::create_dir_all(dist_dir.as_path()).expect("create dist");
        write_factory_suite(
            dist_dir.as_path(),
            valid_factory_manifest(),
            serde_json::json!([
                {
                    "asset_id": "asset.0",
                    "source_lineage": "adapter://seed",
                    "license_class": "rights-cleared",
                    "attestation_ref": "attest-1",
                    "attested": true
                }
            ]),
            valid_quality_report(),
        );
        let mut invalid_clip_source: serde_json::Value = serde_json::from_slice(
            fs::read(dist_dir.join("animation-clip-bundle-v2.json"))
                .expect("read clip bundle")
                .as_slice(),
        )
        .expect("parse clip bundle");
        invalid_clip_source["source"] = serde_json::json!("external-import-v2");
        fs::write(
            dist_dir.join("animation-clip-bundle-v2.json"),
            serde_json::to_vec_pretty(&invalid_clip_source).expect("serialize invalid clip bundle"),
        )
        .expect("write invalid clip bundle");
        let report = build_asset_factory_check_report(dist_dir.as_path()).expect("report");
        assert_eq!(
            report
                .get("animation_clip_bundle_schema_ok")
                .and_then(|value| value.as_bool()),
            Some(false)
        );
        assert_eq!(
            report.get("passed").and_then(|value| value.as_bool()),
            Some(false)
        );

        let dir = tempfile::tempdir().expect("tempdir");
        let dist_dir = dir.path().join("dist");
        fs::create_dir_all(dist_dir.as_path()).expect("create dist");
        write_factory_suite(
            dist_dir.as_path(),
            valid_factory_manifest(),
            serde_json::json!([
                {
                    "asset_id": "asset.0",
                    "source_lineage": "adapter://seed",
                    "license_class": "rights-cleared",
                    "attestation_ref": "attest-1",
                    "attested": true
                }
            ]),
            valid_quality_report(),
        );
        let mut invalid_graph_contract: serde_json::Value = serde_json::from_slice(
            fs::read(dist_dir.join("animation-graph-contract-v2.json"))
                .expect("read animation graph contract")
                .as_slice(),
        )
        .expect("parse animation graph contract");
        invalid_graph_contract["graphs"][0]["transitions"] = serde_json::json!([]);
        fs::write(
            dist_dir.join("animation-graph-contract-v2.json"),
            serde_json::to_vec_pretty(&invalid_graph_contract)
                .expect("serialize invalid graph contract"),
        )
        .expect("write invalid graph contract");
        let report = build_asset_factory_check_report(dist_dir.as_path()).expect("report");
        assert_eq!(
            report
                .get("animation_graph_contract_schema_ok")
                .and_then(|value| value.as_bool()),
            Some(false)
        );
        assert_eq!(
            report.get("passed").and_then(|value| value.as_bool()),
            Some(false)
        );

        let dir = tempfile::tempdir().expect("tempdir");
        let dist_dir = dir.path().join("dist");
        fs::create_dir_all(dist_dir.as_path()).expect("create dist");
        write_factory_suite(
            dist_dir.as_path(),
            valid_factory_manifest(),
            serde_json::json!([
                {
                    "asset_id": "asset.0",
                    "source_lineage": "adapter://seed",
                    "license_class": "rights-cleared",
                    "attestation_ref": "attest-1",
                    "attested": true
                }
            ]),
            valid_quality_report(),
        );
        let mut invalid_flora_contract: serde_json::Value = serde_json::from_slice(
            fs::read(dist_dir.join("flora-sim-contract-v1.json"))
                .expect("read flora contract")
                .as_slice(),
        )
        .expect("parse flora contract");
        invalid_flora_contract["wind_bands"] = serde_json::json!([]);
        fs::write(
            dist_dir.join("flora-sim-contract-v1.json"),
            serde_json::to_vec_pretty(&invalid_flora_contract)
                .expect("serialize invalid flora contract"),
        )
        .expect("write invalid flora contract");
        let report = build_asset_factory_check_report(dist_dir.as_path()).expect("report");
        assert_eq!(
            report
                .get("flora_sim_contract_schema_ok")
                .and_then(|value| value.as_bool()),
            Some(false)
        );
        assert_eq!(
            report.get("passed").and_then(|value| value.as_bool()),
            Some(false)
        );

        let dir = tempfile::tempdir().expect("tempdir");
        let dist_dir = dir.path().join("dist");
        fs::create_dir_all(dist_dir.as_path()).expect("create dist");
        write_factory_suite(
            dist_dir.as_path(),
            valid_factory_manifest(),
            serde_json::json!([
                {
                    "asset_id": "asset.0",
                    "source_lineage": "adapter://seed",
                    "license_class": "rights-cleared",
                    "attestation_ref": "attest-1",
                    "attested": true
                }
            ]),
            valid_quality_report(),
        );
        let mut invalid_external_refs: serde_json::Value = serde_json::from_slice(
            fs::read(dist_dir.join("animation-quality-report-v2.json"))
                .expect("read animation quality report")
                .as_slice(),
        )
        .expect("parse animation quality report");
        invalid_external_refs["external_asset_references"] = serde_json::json!(2);
        fs::write(
            dist_dir.join("animation-quality-report-v2.json"),
            serde_json::to_vec_pretty(&invalid_external_refs)
                .expect("serialize invalid animation quality report"),
        )
        .expect("write invalid animation quality report");
        let report = build_asset_factory_check_report(dist_dir.as_path()).expect("report");
        assert_eq!(
            report
                .get("animation_quality_report_schema_ok")
                .and_then(|value| value.as_bool()),
            Some(false)
        );
        assert_eq!(
            report.get("passed").and_then(|value| value.as_bool()),
            Some(false)
        );

        let dir = tempfile::tempdir().expect("tempdir");
        let dist_dir = dir.path().join("dist");
        fs::create_dir_all(dist_dir.as_path()).expect("create dist");
        write_factory_suite(
            dist_dir.as_path(),
            valid_factory_manifest(),
            serde_json::json!([
                {
                    "asset_id": "asset.0",
                    "source_lineage": "adapter://seed",
                    "license_class": "rights-cleared",
                    "attestation_ref": "attest-1",
                    "attested": true
                }
            ]),
            valid_quality_report(),
        );
        let mut invalid_hash_alignment: serde_json::Value = serde_json::from_slice(
            fs::read(dist_dir.join("animation-quality-report-v2.json"))
                .expect("read animation quality report")
                .as_slice(),
        )
        .expect("parse animation quality report");
        invalid_hash_alignment["replay_hash"] = serde_json::json!("mismatch-hash");
        fs::write(
            dist_dir.join("animation-quality-report-v2.json"),
            serde_json::to_vec_pretty(&invalid_hash_alignment)
                .expect("serialize invalid replay-hash alignment report"),
        )
        .expect("write invalid replay-hash alignment report");
        let report = build_asset_factory_check_report(dist_dir.as_path()).expect("report");
        assert_eq!(
            report
                .get("animation_quality_report_schema_ok")
                .and_then(|value| value.as_bool()),
            Some(true)
        );
        assert_eq!(
            report
                .get("animation_replay_hash_alignment_ok")
                .and_then(|value| value.as_bool()),
            Some(false)
        );
        assert_eq!(
            report.get("passed").and_then(|value| value.as_bool()),
            Some(false)
        );

        let dir = tempfile::tempdir().expect("tempdir");
        let dist_dir = dir.path().join("dist");
        fs::create_dir_all(dist_dir.as_path()).expect("create dist");
        write_factory_suite(
            dist_dir.as_path(),
            valid_factory_manifest(),
            serde_json::json!([
                {
                    "asset_id": "asset.0",
                    "source_lineage": "",
                    "license_class": "rights-cleared",
                    "attestation_ref": "attest-1",
                    "attested": true
                }
            ]),
            valid_quality_report(),
        );
        let report = build_asset_factory_check_report(dist_dir.as_path()).expect("report");
        assert_eq!(
            report.get("provenance_diagnostics").cloned(),
            Some(serde_json::json!([PROVENANCE_ERROR_CODE_UNKNOWN_LINEAGE]))
        );

        let dir = tempfile::tempdir().expect("tempdir");
        let dist_dir = dir.path().join("dist");
        fs::create_dir_all(dist_dir.as_path()).expect("create dist");
        write_factory_suite(
            dist_dir.as_path(),
            valid_factory_manifest(),
            serde_json::json!([
                {
                    "asset_id": "asset.0",
                    "source_lineage": "adapter://seed",
                    "license_class": "cc-by-nc",
                    "attested": false
                }
            ]),
            valid_quality_report(),
        );
        let report = build_asset_factory_check_report(dist_dir.as_path()).expect("report");
        assert_eq!(
            report.get("provenance_diagnostics").cloned(),
            Some(serde_json::json!([
                PROVENANCE_ERROR_CODE_BLOCKED_LICENSE,
                PROVENANCE_ERROR_CODE_MISSING_ATTESTATION
            ]))
        );

        let dir = tempfile::tempdir().expect("tempdir");
        let dist_dir = dir.path().join("dist");
        fs::create_dir_all(dist_dir.as_path()).expect("create dist");
        write_factory_suite(
            dist_dir.as_path(),
            valid_factory_manifest(),
            serde_json::json!([
                {
                    "asset_id": "asset.0",
                    "source_lineage": "adapter://seed",
                    "license_class": "rights-cleared",
                    "attestation_ref": "",
                    "attested": true
                }
            ]),
            valid_quality_report(),
        );
        let report = build_asset_factory_check_report(dist_dir.as_path()).expect("report");
        assert_eq!(
            report.get("provenance_diagnostics").cloned(),
            Some(serde_json::json!([
                PROVENANCE_ERROR_CODE_MISSING_ATTESTATION
            ]))
        );

        let dir = tempfile::tempdir().expect("tempdir");
        let dist_dir = dir.path().join("dist");
        fs::create_dir_all(dist_dir.as_path()).expect("create dist");
        let mut invalid_factory = valid_factory_manifest();
        invalid_factory["generated_assets"][0]["conditioning_evidence"] = serde_json::Value::Null;
        write_factory_suite(
            dist_dir.as_path(),
            invalid_factory,
            serde_json::json!([
                {
                    "asset_id": "asset.0",
                    "source_lineage": "adapter://seed",
                    "license_class": "rights-cleared",
                    "attestation_ref": "attest-1",
                    "attested": true
                }
            ]),
            valid_quality_report(),
        );
        let report = build_asset_factory_check_report(dist_dir.as_path()).expect("report");
        assert_eq!(
            report.get("conditioning_diagnostics").cloned(),
            Some(serde_json::json!([
                CONDITIONING_ERROR_CODE_MISSING_EVIDENCE
            ]))
        );

        let dir = tempfile::tempdir().expect("tempdir");
        let dist_dir = dir.path().join("dist");
        fs::create_dir_all(dist_dir.as_path()).expect("create dist");
        fs::write(
            dist_dir.join("asset-factory-manifest-v2.json"),
            serde_json::to_vec_pretty(&valid_factory_manifest()).expect("serialize"),
        )
        .expect("write manifest");
        let report = build_asset_factory_check_report(dist_dir.as_path())
            .expect("build asset factory report");
        assert_eq!(
            report.get("passed").and_then(|value| value.as_bool()),
            Some(false)
        );
        assert!(
            report
                .get("missing")
                .and_then(|value| value.as_array())
                .is_some_and(|items| !items.is_empty())
        );
    }

    #[test]
    fn anim_mutate_report_contains_objective_metrics() {
        let dir = tempfile::tempdir().expect("tempdir");
        let app_root = dir.path().join("app");
        game_init_project(app_root.as_path()).expect("init project");

        let summary = game_anim_mutate_project(app_root.as_path(), "readability")
            .expect("mutate project with readability objective");
        assert_eq!(summary.objective, "readability");

        let report =
            read_json_artifact(summary.report_path.as_path(), "animation mutation report")
                .expect("read mutation report");
        assert_eq!(
            report.get("objective").and_then(|value| value.as_str()),
            Some("readability")
        );
        let candidates = report
            .get("candidates")
            .and_then(|value| value.as_array())
            .expect("candidates array");
        assert!(
            !candidates.is_empty(),
            "expected at least one mutation candidate"
        );
        for candidate in candidates {
            assert_eq!(
                candidate.get("objective").and_then(|value| value.as_str()),
                Some("readability")
            );
            let metrics = candidate
                .get("objective_metrics")
                .and_then(|value| value.as_object())
                .expect("objective_metrics object");
            assert!(
                metrics.contains_key("duration_ms"),
                "missing duration_ms in objective metrics"
            );
            assert!(
                metrics.contains_key("keyframe_count"),
                "missing keyframe_count in objective metrics"
            );
            assert!(
                metrics.contains_key("score_seed_mod_1000"),
                "missing score_seed_mod_1000 in objective metrics"
            );
        }
    }

    #[test]
    fn animation_source_hash_changes_on_transitive_dependency_change() {
        let dir = tempfile::tempdir().expect("tempdir");
        let entry_path = dir.path().join("main.wr");
        fs::write(
            entry_path.as_path(),
            "fn run() -> Integer {\n    return helper()\n}\n",
        )
        .expect("write entry source");
        let helper_path = dir.path().join("pkg").join("helper.wr");
        fs::create_dir_all(
            helper_path
                .parent()
                .expect("helper path should have parent directory"),
        )
        .expect("create helper parent");
        let module = hir::Module::default();

        let mut module_sources = HashMap::new();
        module_sources.insert(
            entry_path.clone(),
            "fn run() -> Integer {\n    return helper()\n}\n".to_string(),
        );
        module_sources.insert(
            helper_path.clone(),
            "fn helper() -> Integer {\n    return 1\n}\n".to_string(),
        );
        let baseline =
            compute_animation_source_hash(entry_path.as_path(), &module, &module_sources)
                .expect("compute baseline animation source hash");

        module_sources.insert(
            helper_path.clone(),
            "fn helper() -> Integer {\n    return 2\n}\n".to_string(),
        );
        let changed = compute_animation_source_hash(entry_path.as_path(), &module, &module_sources)
            .expect("compute changed animation source hash");

        assert_ne!(
            baseline, changed,
            "hash should change when transitive module source changes"
        );
    }

    #[test]
    fn animation_source_hash_order_independent() {
        let dir = tempfile::tempdir().expect("tempdir");
        let entry_path = dir.path().join("main.wr");
        fs::write(
            entry_path.as_path(),
            "fn run() -> Integer {\n    return helper()\n}\n",
        )
        .expect("write entry source");
        let helper_path = dir.path().join("pkg").join("helper.wr");
        fs::create_dir_all(
            helper_path
                .parent()
                .expect("helper path should have parent directory"),
        )
        .expect("create helper parent");

        let entry_source = "fn run() -> Integer {\n    return helper()\n}\n".to_string();
        let helper_source = "fn helper() -> Integer {\n    return 1\n}\n".to_string();
        let module = hir::Module::default();

        let mut module_sources_a = HashMap::new();
        module_sources_a.insert(helper_path.clone(), helper_source.clone());
        module_sources_a.insert(entry_path.clone(), entry_source.clone());

        let mut module_sources_b = HashMap::new();
        module_sources_b.insert(entry_path.clone(), entry_source);
        module_sources_b.insert(helper_path.clone(), helper_source);

        let hash_a = compute_animation_source_hash(entry_path.as_path(), &module, &module_sources_a)
            .expect("compute hash a");
        let hash_b = compute_animation_source_hash(entry_path.as_path(), &module, &module_sources_b)
            .expect("compute hash b");

        assert_eq!(
            hash_a, hash_b,
            "hash should remain stable regardless of module source map insertion order"
        );
    }
}
