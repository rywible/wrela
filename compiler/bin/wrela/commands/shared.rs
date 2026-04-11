use super::cli_args::{CommandSpec, ParsedCommandSpec};
use super::contracts::{
    EXIT_CODEGEN, EXIT_OK, EXIT_PARSE, EXIT_RUNTIME_SIGNAL, EXIT_TYPE, EXIT_USAGE, OutputFormat,
};
use super::{cert_engine, diag_emit, perf_engine, replay_trace};
use miette::SourceSpan;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::env;
use std::fs;
use std::io::{self};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use wrela::diag::catalog::{mir_descriptor, project_descriptor};
use wrela::diag::suppress::suppress_cascades;
use wrela::diag::{DiagFix, DiagRecord, DiagSeverity, DiagSpan, DiagStage, dedupe_records};
use wrela::hir;
use wrela::mir;
use wrela::parser;
#[path = "../repro.rs"]
mod repro;

pub(super) fn run_repro_artifact(
    workspace_root: &Path,
    repro_artifact_path: &Path,
    timeout: Duration,
    output_format: OutputFormat,
    http_mode: HttpCassetteMode,
    budget_policy: &BudgetPolicyV1,
) -> i32 {
    repro::run_repro_artifact(
        workspace_root,
        repro_artifact_path,
        timeout,
        output_format,
        http_mode,
        budget_policy,
    )
}

fn naming_policy_tier(error: &hir::naming::NamingError) -> &'static str {
    match error {
        hir::naming::NamingError::ResultPrefixRequired { .. }
        | hir::naming::NamingError::FactoryPrefixRequired { .. }
        | hir::naming::NamingError::ResultErrorTypeShape { .. }
        | hir::naming::NamingError::TopLevelCheckName { .. }
        | hir::naming::NamingError::MemberCheckPrefix { .. } => "strong",
        hir::naming::NamingError::SnakeCaseRequired { .. }
        | hir::naming::NamingError::PascalCaseRequired { .. }
        | hir::naming::NamingError::VerbLedRequired { .. }
        | hir::naming::NamingError::NounOnlyRequired { .. }
        | hir::naming::NamingError::BooleanPrefixRequired { .. }
        | hir::naming::NamingError::InlineCheckCondition { .. }
        | hir::naming::NamingError::ModuleSemanticRequired { .. }
        | hir::naming::NamingError::CollectionPluralityRequired { .. } => "style",
    }
}

fn naming_policy_severity(error: &hir::naming::NamingError, strict_naming: bool) -> DiagSeverity {
    let tier = naming_policy_tier(error);
    if strict_naming && (tier == "strong" || tier == "style") {
        DiagSeverity::Error
    } else {
        DiagSeverity::Warning
    }
}

fn project_naming_diagnostics(
    project: &hir::project::LoadedProject,
) -> Vec<(PathBuf, String, hir::naming::NamingError)> {
    let mut diagnostics = Vec::new();
    for source_module in &project.source_modules {
        let (_type_errors, type_info) = hir::typeck::check_module_with_info(&source_module.module);
        for err in hir::naming::check_module(&source_module.module, &type_info) {
            diagnostics.push((
                source_module.path.clone(),
                source_module.source.clone(),
                err,
            ));
        }
    }
    diagnostics
}

fn execute_query_contracts_command(
    output_format: OutputFormat,
    path_arg: Option<String>,
    program_args: Vec<String>,
) {
    if path_arg.is_some() {
        eprintln!("error: query-contracts does not take a path");
        std::process::exit(EXIT_USAGE);
    }
    if !program_args.is_empty() {
        eprintln!("error: unexpected extra arguments");
        std::process::exit(EXIT_USAGE);
    }
    let catalog = query_contract_catalog_snapshot();
    if matches!(output_format, OutputFormat::Json) {
        println!(
            "{}",
            serde_json::to_string_pretty(&catalog).unwrap_or_else(|_| "{}".to_string())
        );
        return;
    }
    println!("query contract catalog schema v{}", catalog.schema_version);
    for contract in &catalog.contracts {
        let backends = if contract.backends.is_empty() {
            "none".to_string()
        } else {
            contract.backends.join(",")
        };
        println!(
            "{} v{}  call={}  target={}  cardinality={}  surface={}  capture={}  item={}  result={}  backends={}  legacy={}",
            contract.contract_id,
            contract.contract_version,
            contract.call,
            contract.target,
            contract.cardinality,
            contract.surface,
            contract.capture_kind,
            contract.item_kind,
            contract.result_kind,
            backends,
            contract.legacy_builtin,
        );
    }
    if !catalog.aliases.is_empty() {
        println!("aliases:");
        for alias in &catalog.aliases {
            println!(
                "{} -> {}  ({})",
                alias.alias_id, alias.canonical_id, alias.reason
            );
        }
    }
}

#[derive(Serialize)]
struct PresentationPlanDump {
    schema_version: u32,
    entry_path: String,
    plans: Vec<PresentationPlanDumpItem>,
}

#[derive(Serialize)]
struct PresentationDebugDump {
    view: String,
    region: String,
    domain: String,
    backend: String,
    frames_executed: u32,
    color_ppm: String,
    depth_ppm: String,
    world_normal_ppm: String,
    stats_path: String,
    stats: String,
    frame_cost: wrela::presentation_exec::PresentationFrameCostReport,
    frame_cost_history: Vec<wrela::presentation_exec::PresentationFrameCostReport>,
}

#[derive(Serialize)]
struct FrameContractsDump {
    schema_version: u32,
    entry_path: String,
    views: Vec<FrameContractsDumpItem>,
}

#[derive(Serialize)]
struct FrameContractsDumpItem {
    name: String,
    frame: PresentationFrameDump,
    frame_artifacts: Vec<PresentationFrameArtifactDump>,
    bindings: Vec<PresentationBindingDump>,
}

#[derive(Serialize)]
struct PreviewReportDump {
    schema_version: u32,
    view: String,
    region: String,
    domain: String,
    attachment: String,
    backend: String,
    width: u32,
    height: u32,
    stats: String,
    frame_cost: wrela::presentation_exec::PresentationFrameCostReport,
}

#[derive(Serialize)]
struct FrameBundleDump {
    schema_version: u32,
    view: String,
    region: String,
    domain: String,
    backend: String,
    width: u32,
    height: u32,
    frame_index: u32,
    attachments: Vec<serde_json::Value>,
    frame_cost: wrela::presentation_exec::PresentationFrameCostReport,
}

#[derive(Serialize)]
struct PresentationPlanDumpItem {
    name: String,
    view: PresentationViewDump,
    frame: PresentationFrameDump,
    passes: Vec<PresentationPassDump>,
    frame_artifacts: Vec<PresentationFrameArtifactDump>,
    bindings: Vec<PresentationBindingDump>,
    candidate_query_program_concepts: Vec<String>,
    validation_errors: Vec<String>,
}

#[derive(Serialize)]
struct PresentationViewDump {
    canonical_projection: bool,
    canonical_projection_input: String,
    screen_lattice: PresentationScreenLatticeDump,
    canonical_view_ray: PresentationViewRayDump,
    allows_legacy_projection_override: bool,
    compatibility_projection: PresentationCompatibilityProjectionDump,
}

#[derive(Serialize)]
struct PresentationScreenLatticeDump {
    sample_position: String,
    origin: String,
    width_source: String,
    height_source: String,
}

#[derive(Serialize)]
struct PresentationViewRayDump {
    space: String,
    normalized_direction: bool,
    projection_input: String,
}

#[derive(Serialize)]
struct PresentationCompatibilityProjectionDump {
    legacy_path_active: bool,
    authored_world_up_override: bool,
    authored_view_scale_override: bool,
}

#[derive(Serialize)]
struct PresentationFrameDump {
    outputs: Vec<PresentationAttachmentDump>,
    primary_hit: Option<PresentationPrimaryHitDump>,
    temporal_reuse: Option<String>,
    quality: PresentationQualityDump,
    lighting: PresentationLightingDump,
    observability: Vec<String>,
}

#[derive(Serialize)]
struct PresentationQualityDump {
    tier: String,
    target_fps: u32,
    internal_resolution_scale: f32,
    allow_dynamic_resolution: bool,
    primary_max_steps: i32,
    allow_radiance: bool,
    allow_media: bool,
    temporal_mode: String,
    allow_half_res_participants: bool,
    allow_hit_compaction: bool,
    degradation_order: Vec<String>,
}

#[derive(Serialize)]
struct PresentationPrimaryHitDump {
    attachment: String,
    record: String,
    fields: Vec<String>,
    depth_semantics: String,
    sample_identity: String,
}

#[derive(Serialize)]
struct PresentationAttachmentDump {
    name: String,
    kind: String,
    element_schema: String,
    lifetime: String,
    resolution: String,
    scale: String,
    clear_policy: String,
}

#[derive(Serialize)]
struct PresentationLightingDump {
    key_light: PresentationLightingInputDump,
    fill_direction: PresentationLightingInputDump,
    fill_strength: PresentationLightingInputDump,
    ambient_color: PresentationLightingInputDump,
    allows_legacy_plural_lights_metadata: bool,
}

#[derive(Serialize)]
struct PresentationLightingInputDump {
    binding: String,
    element_schema: String,
    source: String,
    temporary_compatibility_alias: bool,
}

#[derive(Serialize)]
struct PresentationPassDump {
    id: String,
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    screen_samples: Option<PresentationScreenSamplePassDump>,
    consumes: Vec<String>,
    materializes: Vec<String>,
    binding: Option<String>,
    query_dependencies: Vec<PresentationQueryDependencyDump>,
    future_acceleration_hooks: Vec<String>,
    observability: Vec<String>,
}

#[derive(Serialize)]
struct PresentationScreenSamplePassDump {
    viewport_width_source: String,
    viewport_height_source: String,
    samples_per_pixel: u32,
    jitter_source: String,
    item_count_expression: String,
    output_item_record: String,
}

#[derive(Serialize)]
struct PresentationQueryDependencyDump {
    contract_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    question: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    surface: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cardinality: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    call: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    solver_diagnostics: Option<PresentationRaySolverDump>,
}

#[derive(Serialize)]
struct PresentationRaySolverDump {
    plan_id: String,
    methods: Vec<String>,
    fallback: String,
    unavailable_facts: Vec<String>,
}

#[derive(Serialize)]
struct PresentationFrameArtifactDump {
    id: String,
    attachment: String,
    producer_pass: String,
    materialized: bool,
}

#[derive(Serialize)]
struct PresentationBindingDump {
    id: String,
    pass_kind: String,
    recipe: String,
    default_backend: String,
    execution: String,
}

fn execute_presentation_plan_command(
    output_format: OutputFormat,
    path_arg: Option<String>,
    program_args: Vec<String>,
    query_backend: wrela::query_plan::DispatchBackend,
) {
    if !program_args.is_empty() {
        eprintln!("error: unexpected extra arguments");
        std::process::exit(EXIT_USAGE);
    }
    let entry_path = match resolve_entry_path(path_arg.as_deref()) {
        Ok(path) => path,
        Err(err) => {
            eprintln!("error: {err}");
            std::process::exit(EXIT_USAGE);
        }
    };
    let plans = match compile_presentation_plans(&entry_path, output_format, query_backend) {
        Ok(plans) => plans,
        Err(code) => std::process::exit(code),
    };
    let dump = presentation_plan_dump(&entry_path, &plans);
    if matches!(output_format, OutputFormat::Json) {
        println!(
            "{}",
            serde_json::to_string_pretty(&dump).unwrap_or_else(|_| "{}".to_string())
        );
    } else {
        print_presentation_plan_human(&dump);
    }
}

#[derive(Debug)]
struct PresentationDebugOptions {
    view: Option<String>,
    region: Option<String>,
    domain: Option<String>,
    out_dir: Option<PathBuf>,
    width: Option<u32>,
    height: Option<u32>,
    camera_position: [f32; 3],
    camera_forward: [f32; 3],
    camera_up: [f32; 3],
    vertical_fov_degrees: f32,
    frame_index: u32,
    delta_seconds: f32,
    frames: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FrameAttachmentFormat {
    Json,
    Ppm,
}

#[derive(Debug)]
struct PreviewCommandOptions {
    view: Option<String>,
    region: Option<String>,
    domain: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    camera_position: [f32; 3],
    camera_forward: [f32; 3],
    camera_up: [f32; 3],
    vertical_fov_degrees: f32,
    frame_index: u32,
    delta_seconds: f32,
    attachment: String,
    json_report: bool,
}

#[derive(Debug)]
struct FrameCommandOptions {
    view: Option<String>,
    region: Option<String>,
    domain: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    camera_position: [f32; 3],
    camera_forward: [f32; 3],
    camera_up: [f32; 3],
    vertical_fov_degrees: f32,
    frame_index: u32,
    delta_seconds: f32,
    attachments: Vec<String>,
    attachment_format: FrameAttachmentFormat,
}

struct CompiledPresentationBundle {
    module: hir::Module,
    query_ctx: wrela::query_exec::QueryExecContext,
    plans: Vec<wrela::presentation_plan::PresentationPlan>,
}

#[derive(Debug, Clone, PartialEq)]
struct PreparedPresentationExecution {
    plan: wrela::presentation_plan::PresentationPlan,
    input: wrela::presentation_exec::PresentationExecutionInput,
    camera: wrela::presentation_contract::CanonicalCameraInput,
    viewport: wrela::presentation_contract::CanonicalViewportInput,
}

struct ReadyPresentationExecution {
    bundle: CompiledPresentationBundle,
    prepared: PreparedPresentationExecution,
    region_name: SmolStr,
    domain_name: SmolStr,
}

fn execute_presentation_debug_command(
    output_format: OutputFormat,
    path_arg: Option<String>,
    program_args: Vec<String>,
    query_backend: wrela::query_plan::DispatchBackend,
) {
    let entry_path = match resolve_entry_path(path_arg.as_deref()) {
        Ok(path) => path,
        Err(err) => {
            eprintln!("error: {err}");
            std::process::exit(EXIT_USAGE);
        }
    };
    let options = match parse_presentation_debug_options(&program_args) {
        Ok(options) => options,
        Err(err) => {
            eprintln!("error: {err}");
            std::process::exit(EXIT_USAGE);
        }
    };
    let bundle = match compile_presentation_bundle(&entry_path, output_format, query_backend) {
        Ok(bundle) => bundle,
        Err(code) => std::process::exit(code),
    };
    let plan = match select_view_plan(&bundle, options.view.as_deref()) {
        Ok(plan) => plan,
        Err(err) => {
            eprintln!("error: {err}");
            std::process::exit(EXIT_USAGE);
        }
    };
    let view_func = bundle
        .module
        .functions
        .iter()
        .find(|(_, func)| func.name == plan.name)
        .map(|(_, func)| func)
        .expect("selected presentation plan should map back to a function");
    let region_name = match select_region_name(&bundle.module, options.region.as_deref()) {
        Ok(name) => name,
        Err(err) => {
            eprintln!("error: {err}");
            std::process::exit(EXIT_USAGE);
        }
    };
    let domain_name = match select_domain_name(&bundle.module, view_func, options.domain.as_deref())
    {
        Ok(name) => name,
        Err(err) => {
            eprintln!("error: {err}");
            std::process::exit(EXIT_USAGE);
        }
    };
    let camera = wrela::presentation_contract::CanonicalCameraInput {
        position: options.camera_position,
        forward: options.camera_forward,
        up: options.camera_up,
        vertical_fov_degrees: options.vertical_fov_degrees,
    };
    let prepared = match prepare_presentation_execution(
        &bundle.module,
        plan,
        view_func,
        region_name.clone(),
        domain_name.clone(),
        camera,
        options.width,
        options.height,
        options.frame_index,
        options.delta_seconds,
        query_backend,
    ) {
        Ok(prepared) => prepared,
        Err(err) => {
            eprintln!("error: {err}");
            std::process::exit(EXIT_USAGE);
        }
    };
    let mut session =
        wrela::presentation_exec::AdaptivePresentationSession::new(prepared.plan.frame.quality.clone());
    let mut frame_cost_history = Vec::new();
    let mut result = None;
    for frame_offset in 0..options.frames.max(1) {
        let mut frame_input = prepared.input.clone();
        frame_input.frame_state = wrela::presentation_exec::frame_state_value(
            prepared.camera,
            prepared.camera,
            prepared.viewport,
            [0.0, 0.0],
            options.frame_index.saturating_add(frame_offset),
            options.delta_seconds,
        );
        let frame_result =
            match session.execute_frame(&bundle.query_ctx, &prepared.plan, &frame_input) {
                Ok(result) => result,
                Err(err) => {
                    eprintln!("presentation execution error: {err}");
                    std::process::exit(EXIT_CODEGEN);
                }
            };
        frame_cost_history.push(frame_result.frame_cost.clone());
        result = Some(frame_result);
    }
    let result = result.expect("presentation debug should execute at least one frame");
    let out_dir = options.out_dir.unwrap_or_else(|| {
        entry_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("presentation_debug")
            .join(plan.name.as_str())
    });
    let artifacts = match wrela::presentation_exec::debug::export_frame_debug(&result, &out_dir) {
        Ok(artifacts) => artifacts,
        Err(err) => {
            eprintln!("presentation debug export error: {err}");
            std::process::exit(EXIT_CODEGEN);
        }
    };
    let stats = wrela::presentation_exec::debug::render_primary_visibility_stats(&result);
    let dump = PresentationDebugDump {
        view: plan.name.to_string(),
        region: region_name.to_string(),
        domain: domain_name.to_string(),
        backend: dispatch_backend_name(result.backend).to_string(),
        frames_executed: frame_cost_history.len() as u32,
        color_ppm: artifacts.color_ppm.display().to_string(),
        depth_ppm: artifacts.depth_ppm.display().to_string(),
        world_normal_ppm: artifacts.world_normal_ppm.display().to_string(),
        stats_path: artifacts.stats_path.display().to_string(),
        stats,
        frame_cost: result.frame_cost.clone(),
        frame_cost_history,
    };
    if matches!(output_format, OutputFormat::Json) {
        println!(
            "{}",
            serde_json::to_string_pretty(&dump).unwrap_or_else(|_| "{}".to_string())
        );
    } else {
        println!(
            "presentation debug view={} backend={}",
            dump.view, dump.backend
        );
        println!("  region: {}", dump.region);
        println!("  domain: {}", dump.domain);
        println!("  frames: {}", dump.frames_executed);
        println!("  color ppm: {}", dump.color_ppm);
        println!("  depth ppm: {}", dump.depth_ppm);
        println!("  world normal ppm: {}", dump.world_normal_ppm);
        println!("  stats: {}", dump.stats_path);
        println!("{}", dump.stats.trim_end());
    }
}

fn execute_preview_command(
    output_format: OutputFormat,
    path_arg: Option<String>,
    program_args: Vec<String>,
    query_backend: wrela::query_plan::DispatchBackend,
) {
    let entry_path = match resolve_entry_path(path_arg.as_deref()) {
        Ok(path) => path,
        Err(err) => {
            eprintln!("error: {err}");
            std::process::exit(EXIT_USAGE);
        }
    };
    let options = match parse_preview_command_options(&program_args) {
        Ok(options) => options,
        Err(err) => {
            eprintln!("error: {err}");
            std::process::exit(EXIT_USAGE);
        }
    };
    if matches!(output_format, OutputFormat::Json) && !options.json_report {
        eprintln!("error: `preview --json` requires `--json-report`; use `frame --json` for typed attachment bundles");
        std::process::exit(EXIT_USAGE);
    }
    let ready = match load_prepared_presentation_execution(
        &entry_path,
        output_format,
        query_backend,
        options.view.as_deref(),
        options.region.as_deref(),
        options.domain.as_deref(),
        wrela::presentation_contract::CanonicalCameraInput {
            position: options.camera_position,
            forward: options.camera_forward,
            up: options.camera_up,
            vertical_fov_degrees: options.vertical_fov_degrees,
        },
        options.width,
        options.height,
        options.frame_index,
        options.delta_seconds,
    ) {
        Ok(ready) => ready,
        Err(code) => std::process::exit(code),
    };
    let result = match wrela::presentation_exec::execute_plan(
        &ready.bundle.query_ctx,
        &ready.prepared.plan,
        &ready.prepared.input,
    ) {
        Ok(result) => result,
        Err(err) => {
            eprintln!("presentation execution error: {err}");
            std::process::exit(EXIT_CODEGEN);
        }
    };
    let attachment_name = match wrela::presentation_exec::debug::attachment_name_for_selector(
        &result,
        &options.attachment,
    ) {
        Ok(name) => name.to_string(),
        Err(err) => {
            eprintln!("preview export error: {err}");
            std::process::exit(EXIT_USAGE);
        }
    };
    if options.json_report {
        let dump = PreviewReportDump {
            schema_version: 1,
            view: ready.prepared.plan.name.to_string(),
            region: ready.region_name.to_string(),
            domain: ready.domain_name.to_string(),
            attachment: attachment_name.clone(),
            backend: dispatch_backend_name(result.backend).to_string(),
            width: result.width,
            height: result.height,
            stats: wrela::presentation_exec::debug::render_primary_visibility_stats(&result),
            frame_cost: result.frame_cost.clone(),
        };
        if matches!(output_format, OutputFormat::Json) {
            println!(
                "{}",
                serde_json::to_string_pretty(&dump).unwrap_or_else(|_| "{}".to_string())
            );
        } else {
            println!("preview report view={} backend={}", dump.view, dump.backend);
            println!("  region: {}", dump.region);
            println!("  domain: {}", dump.domain);
            println!("  attachment: {}", dump.attachment);
            println!("  resolution: {}x{}", dump.width, dump.height);
            println!("{}", dump.stats.trim_end());
        }
        return;
    }
    let ppm = match wrela::presentation_exec::debug::render_attachment_ppm_string(
        &result,
        attachment_name.as_str(),
    ) {
        Ok(ppm) => ppm,
        Err(err) => {
            eprintln!("preview export error: {err}");
            std::process::exit(EXIT_CODEGEN);
        }
    };
    print!("{ppm}");
}

fn execute_frame_command(
    output_format: OutputFormat,
    path_arg: Option<String>,
    program_args: Vec<String>,
    query_backend: wrela::query_plan::DispatchBackend,
) {
    let entry_path = match resolve_entry_path(path_arg.as_deref()) {
        Ok(path) => path,
        Err(err) => {
            eprintln!("error: {err}");
            std::process::exit(EXIT_USAGE);
        }
    };
    let options = match parse_frame_command_options(&program_args) {
        Ok(options) => options,
        Err(err) => {
            eprintln!("error: {err}");
            std::process::exit(EXIT_USAGE);
        }
    };
    let ready = match load_prepared_presentation_execution(
        &entry_path,
        output_format,
        query_backend,
        options.view.as_deref(),
        options.region.as_deref(),
        options.domain.as_deref(),
        wrela::presentation_contract::CanonicalCameraInput {
            position: options.camera_position,
            forward: options.camera_forward,
            up: options.camera_up,
            vertical_fov_degrees: options.vertical_fov_degrees,
        },
        options.width,
        options.height,
        options.frame_index,
        options.delta_seconds,
    ) {
        Ok(ready) => ready,
        Err(code) => std::process::exit(code),
    };
    let result = match wrela::presentation_exec::execute_plan(
        &ready.bundle.query_ctx,
        &ready.prepared.plan,
        &ready.prepared.input,
    ) {
        Ok(result) => result,
        Err(err) => {
            eprintln!("presentation execution error: {err}");
            std::process::exit(EXIT_CODEGEN);
        }
    };
    let attachment_names = match selected_frame_attachment_names(&result, &options.attachments) {
        Ok(names) => names,
        Err(err) => {
            eprintln!("frame export error: {err}");
            std::process::exit(EXIT_USAGE);
        }
    };
    if options.attachment_format == FrameAttachmentFormat::Ppm {
        if matches!(output_format, OutputFormat::Json) {
            eprintln!("error: `frame --json` cannot be combined with `--attachment-format=ppm`");
            std::process::exit(EXIT_USAGE);
        }
        if attachment_names.len() != 1 {
            eprintln!(
                "error: `frame --attachment-format=ppm` requires exactly one selected attachment"
            );
            std::process::exit(EXIT_USAGE);
        }
        let ppm = match wrela::presentation_exec::debug::render_attachment_ppm_string(
            &result,
            attachment_names[0].as_str(),
        ) {
            Ok(ppm) => ppm,
            Err(err) => {
                eprintln!("frame export error: {err}");
                std::process::exit(EXIT_CODEGEN);
            }
        };
        print!("{ppm}");
        return;
    }

    let attachments = attachment_names
        .iter()
        .map(|name| wrela::presentation_exec::debug::attachment_json(&result, name.as_str()))
        .collect::<Result<Vec<_>, _>>();
    let attachments = match attachments {
        Ok(attachments) => attachments,
        Err(err) => {
            eprintln!("frame export error: {err}");
            std::process::exit(EXIT_CODEGEN);
        }
    };
    let dump = FrameBundleDump {
        schema_version: 1,
        view: ready.prepared.plan.name.to_string(),
        region: ready.region_name.to_string(),
        domain: ready.domain_name.to_string(),
        backend: dispatch_backend_name(result.backend).to_string(),
        width: result.width,
        height: result.height,
        frame_index: options.frame_index,
        attachments,
        frame_cost: result.frame_cost.clone(),
    };
    if matches!(output_format, OutputFormat::Json) {
        println!(
            "{}",
            serde_json::to_string_pretty(&dump).unwrap_or_else(|_| "{}".to_string())
        );
    } else {
        println!("frame bundle view={} backend={}", dump.view, dump.backend);
        println!("  region: {}", dump.region);
        println!("  domain: {}", dump.domain);
        println!("  resolution: {}x{}", dump.width, dump.height);
        println!("  attachments:");
        for attachment in &dump.attachments {
            let name = attachment
                .get("name")
                .and_then(|value| value.as_str())
                .unwrap_or("<unknown>");
            let kind = attachment
                .get("kind")
                .and_then(|value| value.as_str())
                .unwrap_or("<unknown>");
            let width = attachment
                .get("width")
                .and_then(|value| value.as_u64())
                .unwrap_or(0);
            let height = attachment
                .get("height")
                .and_then(|value| value.as_u64())
                .unwrap_or(0);
            println!("    {} kind={} {}x{}", name, kind, width, height);
        }
        println!(
            "{}",
            wrela::presentation_exec::render_frame_cost_report(&dump.frame_cost).trim_end()
        );
    }
}

fn execute_frame_contracts_command(
    output_format: OutputFormat,
    path_arg: Option<String>,
    program_args: Vec<String>,
    query_backend: wrela::query_plan::DispatchBackend,
) {
    let entry_path = match resolve_entry_path(path_arg.as_deref()) {
        Ok(path) => path,
        Err(err) => {
            eprintln!("error: {err}");
            std::process::exit(EXIT_USAGE);
        }
    };
    let requested_view = match parse_frame_contracts_view(&program_args) {
        Ok(view) => view,
        Err(err) => {
            eprintln!("error: {err}");
            std::process::exit(EXIT_USAGE);
        }
    };
    let plans = match compile_presentation_plans(&entry_path, output_format, query_backend) {
        Ok(plans) => plans,
        Err(code) => std::process::exit(code),
    };
    let mut views = plans
        .iter()
        .map(presentation_plan_dump_item)
        .filter(|item| {
            requested_view
                .as_deref()
                .map_or(true, |requested| item.name == requested)
        })
        .map(|item| FrameContractsDumpItem {
            name: item.name,
            frame: item.frame,
            frame_artifacts: item.frame_artifacts,
            bindings: item.bindings,
        })
        .collect::<Vec<_>>();
    if let Some(requested_view) = requested_view.as_deref()
        && views.is_empty()
    {
        eprintln!("error: missing view `{requested_view}`");
        std::process::exit(EXIT_USAGE);
    }
    views.sort_by(|lhs, rhs| lhs.name.cmp(&rhs.name));
    let dump = FrameContractsDump {
        schema_version: 1,
        entry_path: entry_path.display().to_string(),
        views,
    };
    if matches!(output_format, OutputFormat::Json) {
        println!(
            "{}",
            serde_json::to_string_pretty(&dump).unwrap_or_else(|_| "{}".to_string())
        );
    } else {
        println!("frame contracts schema v{}", dump.schema_version);
        println!("entry: {}", dump.entry_path);
        for view in &dump.views {
            println!("view {}", view.name);
            let outputs = view
                .frame
                .outputs
                .iter()
                .map(|output| {
                    format!(
                        "{}({},{},{},{},{},{})",
                        output.name,
                        output.kind,
                        output.element_schema,
                        output.lifetime,
                        output.resolution,
                        output.scale,
                        output.clear_policy
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            println!("  frame outputs: {}", outputs);
            println!(
                "  temporal reuse: {}",
                view.frame
                    .temporal_reuse
                    .clone()
                    .unwrap_or_else(|| "Disabled".to_string())
            );
            println!(
                "  quality: tier={} target_fps={}",
                view.frame.quality.tier, view.frame.quality.target_fps
            );
            println!("  bindings:");
            for binding in &view.bindings {
                println!(
                    "    {} recipe={} backend={} execution={}",
                    binding.id, binding.recipe, binding.default_backend, binding.execution
                );
            }
        }
    }
}

fn load_prepared_presentation_execution(
    entry_path: &Path,
    output_format: OutputFormat,
    query_backend: wrela::query_plan::DispatchBackend,
    requested_view: Option<&str>,
    requested_region: Option<&str>,
    requested_domain: Option<&str>,
    camera: wrela::presentation_contract::CanonicalCameraInput,
    width: Option<u32>,
    height: Option<u32>,
    frame_index: u32,
    delta_seconds: f32,
) -> Result<ReadyPresentationExecution, i32> {
    let bundle = compile_presentation_bundle(entry_path, output_format, query_backend)?;
    let plan = match select_view_plan(&bundle, requested_view) {
        Ok(plan) => plan,
        Err(err) => {
            eprintln!("error: {err}");
            return Err(EXIT_USAGE);
        }
    };
    let view_func = bundle
        .module
        .functions
        .iter()
        .find(|(_, func)| func.name == plan.name)
        .map(|(_, func)| func)
        .expect("selected presentation plan should map back to a function");
    let region_name = match select_region_name(&bundle.module, requested_region) {
        Ok(name) => name,
        Err(err) => {
            eprintln!("error: {err}");
            return Err(EXIT_USAGE);
        }
    };
    let domain_name = match select_domain_name(&bundle.module, view_func, requested_domain) {
        Ok(name) => name,
        Err(err) => {
            eprintln!("error: {err}");
            return Err(EXIT_USAGE);
        }
    };
    let prepared = match prepare_presentation_execution(
        &bundle.module,
        plan,
        view_func,
        region_name.clone(),
        domain_name.clone(),
        camera,
        width,
        height,
        frame_index,
        delta_seconds,
        query_backend,
    ) {
        Ok(prepared) => prepared,
        Err(err) => {
            eprintln!("error: {err}");
            return Err(EXIT_USAGE);
        }
    };
    Ok(ReadyPresentationExecution {
        bundle,
        prepared,
        region_name,
        domain_name,
    })
}

fn selected_frame_attachment_names(
    result: &wrela::presentation_exec::PresentationExecutionResult,
    requested: &[String],
) -> Result<Vec<String>, wrela::presentation_exec::PresentationExecError> {
    if requested.is_empty() {
        return Ok(result.attachments.attachments.keys().map(ToString::to_string).collect());
    }
    let mut seen = HashSet::new();
    let mut resolved = Vec::new();
    for selector in requested {
        let name = wrela::presentation_exec::debug::attachment_name_for_selector(result, selector)?;
        if seen.insert(name.to_string()) {
            resolved.push(name.to_string());
        }
    }
    Ok(resolved)
}

fn compile_presentation_plans(
    entry_path: &Path,
    output_format: OutputFormat,
    query_backend: wrela::query_plan::DispatchBackend,
) -> Result<Vec<wrela::presentation_plan::PresentationPlan>, i32> {
    compile_presentation_bundle(entry_path, output_format, query_backend).map(|bundle| bundle.plans)
}

fn compile_presentation_bundle(
    entry_path: &Path,
    output_format: OutputFormat,
    query_backend: wrela::query_plan::DispatchBackend,
) -> Result<CompiledPresentationBundle, i32> {
    let project = match hir::project::load_project_with_entrypoint(entry_path, false) {
        Ok(project) => project,
        Err(errors) => {
            let mut records = Vec::new();
            for err in errors {
                let record = project_record(
                    err.kind,
                    DiagSeverity::Error,
                    err.message,
                    err.path.display().to_string(),
                    err.span,
                );
                records.push((record, err.source));
            }
            diag_emit::emit_deduped_records_with_sources(output_format, records);
            return Err(EXIT_PARSE);
        }
    };

    let module = project.module.clone();
    let source = project.entry_source.clone();
    let source_name = entry_path.display().to_string();
    let mut source_by_path = project.module_sources.clone();
    let provenance = project.provenance.clone();
    source_by_path
        .entry(entry_path.to_path_buf())
        .or_insert_with(|| source.clone());

    let semantic = hir::semantic::check_module(&module);
    let (type_errors, type_info) = hir::typeck::check_module_with_info(&module);
    let mut records = Vec::new();
    for err in semantic.errors {
        let path = resolve_path_from_owner_spans(err.primary_span(), &provenance, &source_name);
        records.push(DiagRecord::from_diagnostic(
            DiagStage::Semantic,
            DiagSeverity::Error,
            &err,
            path,
            err.primary_span(),
        ));
    }
    for err in type_errors {
        let path = resolve_path_from_owner_spans(err.primary_span(), &provenance, &source_name);
        records.push(DiagRecord::from_diagnostic(
            DiagStage::Type,
            DiagSeverity::Error,
            &err,
            path,
            err.primary_span(),
        ));
    }
    if !records.is_empty() {
        for record in suppress_cascades(dedupe_records(records)) {
            let source_for_record = source_by_path
                .get(std::path::Path::new(
                    &record
                        .labels
                        .first()
                        .map(|label| label.span.path.clone())
                        .unwrap_or_else(|| source_name.clone()),
                ))
                .cloned()
                .unwrap_or_else(|| source.clone());
            diag_emit::emit_diag_record(output_format, &record, &source_for_record);
        }
        return Err(EXIT_TYPE);
    }

    let mir_module =
        mir::lower::lower_module_with_types_and_backend(&module, &type_info, query_backend);
    let mut mir_errors = Vec::new();
    for err in mir::validate::validate_module(&mir_module) {
        mir_errors.push(DiagRecord::new(
            DiagStage::Mir,
            DiagSeverity::Error,
            err.message,
            source_name.clone(),
            SourceSpan::from((0usize, 0usize)),
        ));
    }
    if !mir_errors.is_empty() {
        for record in mir_errors {
            diag_emit::emit_diag_record(output_format, &record, &source);
        }
        return Err(EXIT_CODEGEN);
    }

    let query_ctx = wrela::query_exec::QueryExecContext::compile(&module, &type_info);
    let plans = wrela::presentation_plan::plans_for_module(&module, query_backend);
    for plan in &plans {
        let validation_errors = plan.validate();
        if !validation_errors.is_empty() {
            for err in validation_errors {
                eprintln!("presentation plan validation error: {}", err.message);
            }
            return Err(EXIT_CODEGEN);
        }
    }
    Ok(CompiledPresentationBundle {
        module,
        query_ctx,
        plans,
    })
}

type PreviewEvalBindings = HashMap<SmolStr, wrela::kernel::KernelValue>;

fn prepare_presentation_execution(
    module: &hir::Module,
    base_plan: &wrela::presentation_plan::PresentationPlan,
    view_func: &hir::Function,
    region_name: SmolStr,
    domain_name: SmolStr,
    camera: wrela::presentation_contract::CanonicalCameraInput,
    width_override: Option<u32>,
    height_override: Option<u32>,
    frame_index: u32,
    delta_seconds: f32,
    query_backend: wrela::query_plan::DispatchBackend,
) -> Result<PreparedPresentationExecution, String> {
    let domain_func = module
        .functions
        .iter()
        .find(|(_, func)| func.name == domain_name && func.role == hir::FunctionRole::Domain)
        .map(|(_, func)| func)
        .ok_or_else(|| format!("missing domain `{domain_name}`"))?;
    let width = resolve_view_dimension(view_func, width_override, true)?;
    let height = resolve_view_dimension(view_func, height_override, false)?;
    let (frame_domain, ray_budget) = domain_execution_inputs(domain_func, &region_name)?;
    let mut plan = base_plan.clone();
    let domain_metadata = domain_func
        .domain
        .as_ref()
        .ok_or_else(|| format!("selected domain `{domain_name}` is missing domain metadata"))?;
    plan.apply_participant_policy(domain_metadata.radiance, domain_metadata.media);
    let validation_errors = plan.validate();
    if !validation_errors.is_empty() {
        return Err(format!(
            "presentation execution plan `{}` failed validation after participant policy: {}",
            plan.name,
            validation_errors
                .into_iter()
                .map(|err| err.message.to_string())
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }
    let bindings = bind_presentation_function_params(view_func, &region_name, camera);
    let lighting = authored_presentation_lighting_inputs(view_func, &bindings)?;
    let compatibility_projection =
        authored_compatibility_projection_input(&plan, view_func, &bindings, camera)?;
    let frame_state = wrela::presentation_exec::frame_state_value(
        camera,
        camera,
        wrela::presentation_contract::CanonicalViewportInput { width, height },
        [0.0, 0.0],
        frame_index,
        delta_seconds,
    );
    Ok(PreparedPresentationExecution {
        plan,
        input: wrela::presentation_exec::PresentationExecutionInput {
            region_capture: region_name,
            frame_domain,
            frame_state,
            history: None,
            lighting,
            compatibility_projection,
            ray_budget,
            quality_override: None,
            backend: query_backend,
        },
        camera,
        viewport: wrela::presentation_contract::CanonicalViewportInput { width, height },
    })
}

fn bind_presentation_function_params(
    function: &hir::Function,
    region_name: &SmolStr,
    camera: wrela::presentation_contract::CanonicalCameraInput,
) -> PreviewEvalBindings {
    let mut bindings = PreviewEvalBindings::new();
    for param in &function.params {
        match param.ty.as_ref().map(|ty| ty.name.as_str()) {
            Some("RegionCapture") => {
                bindings.insert(
                    param.name.clone(),
                    wrela::kernel::KernelValue::Capture(region_name.clone()),
                );
            }
            Some("Camera") => {
                bindings.insert(param.name.clone(), preview_camera_value(camera));
            }
            _ => {}
        }
    }
    bindings
}

fn authored_presentation_lighting_inputs(
    view_func: &hir::Function,
    bindings: &PreviewEvalBindings,
) -> Result<wrela::presentation_contract::PresentationLightingInputs, String> {
    let metadata = view_func.render.as_ref().ok_or_else(|| {
        format!(
            "selected view `{}` is missing render metadata",
            view_func.name
        )
    })?;
    if metadata.lighting.lights.is_some() {
        return Err(format!(
            "presentation execution does not yet support plural `lights` metadata on `{}`; author `key_light` instead",
            view_func.name
        ));
    }
    let grouped = metadata.lighting.grouped.as_ref();
    let key_light = match grouped
        .and_then(|body| {
            helper_call_named_expr_id(body, "key_light", "light").map(|expr_id| (body, expr_id))
        })
        .or_else(|| {
            metadata
                .lighting
                .light
                .as_ref()
                .and_then(|body| body_terminal_expr_id(body).map(|expr_id| (body, expr_id)))
        }) {
        Some((body, expr_id)) => preview_expect_light(
            &preview_eval_expr(body, expr_id, bindings, "presentation lighting key_light")?,
            "presentation lighting key_light",
        )?,
        None => default_preview_key_light(),
    };
    let fill_direction = match grouped
        .and_then(|body| {
            helper_call_named_expr_id(body, "key_light", "fill_direction")
                .map(|expr_id| (body, expr_id))
        })
        .or_else(|| {
            metadata
                .lighting
                .fill_dir
                .as_ref()
                .and_then(|body| body_terminal_expr_id(body).map(|expr_id| (body, expr_id)))
        }) {
        Some((body, expr_id)) => preview_expect_vec3(
            &preview_eval_expr(body, expr_id, bindings, "presentation lighting fill_direction")?,
            "presentation lighting fill_direction",
        )?,
        None => normalize_preview_vec3([-0.9, 0.45, 0.2]),
    };
    let fill_strength = match grouped
        .and_then(|body| {
            helper_call_named_expr_id(body, "key_light", "fill_strength")
                .map(|expr_id| (body, expr_id))
        })
        .or_else(|| {
            metadata.lighting.fill_strength.as_ref().and_then(|body| {
                body_terminal_expr_id(body).map(|expr_id| (body, expr_id))
            })
        }) {
        Some((body, expr_id)) => preview_expect_f32(
            &preview_eval_expr(body, expr_id, bindings, "presentation lighting fill_strength")?,
            "presentation lighting fill_strength",
        )?,
        None => 0.22,
    };
    let ambient_color = match grouped
        .and_then(|body| {
            helper_call_named_expr_id(body, "key_light", "ambient_color")
                .map(|expr_id| (body, expr_id))
        })
        .or_else(|| {
            metadata.lighting.ambient_color.as_ref().and_then(|body| {
                body_terminal_expr_id(body).map(|expr_id| (body, expr_id))
            })
        }) {
        Some((body, expr_id)) => preview_expect_vec3(
            &preview_eval_expr(body, expr_id, bindings, "presentation lighting ambient_color")?,
            "presentation lighting ambient_color",
        )?,
        None => [0.12, 0.12, 0.12],
    };
    Ok(wrela::presentation_contract::PresentationLightingInputs {
        key_light,
        fill_direction,
        fill_strength,
        ambient_color,
    })
}

fn authored_compatibility_projection_input(
    plan: &wrela::presentation_plan::PresentationPlan,
    view_func: &hir::Function,
    bindings: &PreviewEvalBindings,
    camera: wrela::presentation_contract::CanonicalCameraInput,
) -> Result<Option<wrela::presentation_contract::LegacyCompatibilityProjectionInput>, String> {
    if !plan.view.compatibility_projection.legacy_path_active {
        return Ok(None);
    }
    let metadata = view_func.render.as_ref().ok_or_else(|| {
        format!(
            "selected view `{}` is missing render metadata",
            view_func.name
        )
    })?;
    let world_up = match metadata.compatibility.world_up.as_ref() {
        Some(body) => preview_expect_vec3(
            &preview_eval_body(body, bindings, "presentation compatibility world_up")?,
            "presentation compatibility world_up",
        )?,
        None => camera.up,
    };
    let view_scale = match metadata.compatibility.view_scale.as_ref() {
        Some(body) => preview_expect_f32(
            &preview_eval_body(body, bindings, "presentation compatibility view_scale")?,
            "presentation compatibility view_scale",
        )?,
        None => 0.72,
    };
    Ok(Some(
        wrela::presentation_contract::LegacyCompatibilityProjectionInput {
            world_up,
            view_scale,
        },
    ))
}

fn preview_eval_body(
    body: &hir::Body,
    base_bindings: &PreviewEvalBindings,
    context: &str,
) -> Result<wrela::kernel::KernelValue, String> {
    let mut bindings = base_bindings.clone();
    let mut last_value = None;
    for stmt in &body.root_stmts {
        match &body.stmts[*stmt] {
            hir::Stmt::Expr(expr) => {
                last_value = Some(preview_eval_expr(body, *expr, &bindings, context)?);
            }
            hir::Stmt::Return(Some(expr)) => {
                return preview_eval_expr(body, *expr, &bindings, context);
            }
            hir::Stmt::Let { name, value, .. }
            | hir::Stmt::Assign {
                name,
                op: hir::AssignOp::Assign,
                value,
                ..
            } => {
                let value = preview_eval_expr(body, *value, &bindings, context)?;
                bindings.insert(name.clone(), value);
            }
            hir::Stmt::IgnoreResult { expr } => {
                preview_eval_expr(body, *expr, &bindings, context)?;
            }
            _ => {
                return Err(format!(
                    "{context} only supports literal, arithmetic, constructor, and member-expression bodies"
                ));
            }
        }
    }
    last_value.ok_or_else(|| format!("{context} requires a terminal expression"))
}

fn preview_eval_expr(
    body: &hir::Body,
    expr_id: hir::Idx<hir::Expr>,
    bindings: &PreviewEvalBindings,
    context: &str,
) -> Result<wrela::kernel::KernelValue, String> {
    match &body.exprs[expr_id] {
        hir::Expr::Literal(literal) => preview_literal_value(literal, context),
        hir::Expr::Variable(name) => bindings
            .get(name)
            .cloned()
            .ok_or_else(|| format!("{context} cannot resolve `{name}`")),
        hir::Expr::Unary { op, expr, .. } => {
            let value = preview_eval_expr(body, *expr, bindings, context)?;
            preview_apply_unary(*op, value, context)
        }
        hir::Expr::Binary { lhs, op, rhs, .. } => {
            let lhs = preview_eval_expr(body, *lhs, bindings, context)?;
            let rhs = preview_eval_expr(body, *rhs, bindings, context)?;
            preview_apply_binary(lhs, *op, rhs, context)
        }
        hir::Expr::Call { callee, args, .. } => {
            let hir::Expr::Variable(name) = &body.exprs[*callee] else {
                return Err(format!(
                    "{context} does not support indirect preview-evaluation calls"
                ));
            };
            if name == "capture" {
                let Some(target_expr) = preview_named_or_pos_expr(args, "scene", 0) else {
                    return Err(format!("{context} is missing `scene` for capture"));
                };
                let Some(region_name) = preview_capture_region_name(body, target_expr) else {
                    return Err(format!(
                        "{context} could not resolve the capture scene target"
                    ));
                };
                return Ok(wrela::kernel::KernelValue::Capture(region_name));
            }
            preview_eval_call(name, body, args, bindings, context)
        }
        hir::Expr::Member { object, member, .. } => {
            let object = preview_eval_expr(body, *object, bindings, context)?;
            preview_struct_field(&object, member, context)
        }
        _ => Err(format!(
            "{context} only supports literal, arithmetic, constructor, and member expressions"
        )),
    }
}

fn preview_eval_call(
    callee: &SmolStr,
    body: &hir::Body,
    args: &[hir::Arg],
    bindings: &PreviewEvalBindings,
    context: &str,
) -> Result<wrela::kernel::KernelValue, String> {
    let (positional, mut named) = preview_eval_call_arguments(body, args, bindings, context)?;
    match callee.as_str() {
        "vec3" => Ok(wrela::kernel::KernelValue::Vec3([
            preview_expect_f32(
                &preview_named_or_pos_value(&mut named, &positional, "x", 0, context)?,
                context,
            )?,
            preview_expect_f32(
                &preview_named_or_pos_value(&mut named, &positional, "y", 1, context)?,
                context,
            )?,
            preview_expect_f32(
                &preview_named_or_pos_value(&mut named, &positional, "z", 2, context)?,
                context,
            )?,
        ])),
        "normalize" => {
            let value = preview_named_or_pos_value(&mut named, &positional, "value", 0, context)?;
            Ok(wrela::kernel::KernelValue::Vec3(normalize_preview_vec3(
                preview_expect_vec3(&value, context)?,
            )))
        }
        "Light" => {
            let position = preview_expect_vec3(
                &preview_named_or_pos_value(&mut named, &positional, "position", 0, context)?,
                context,
            )?;
            let direction = preview_expect_vec3(
                &preview_named_or_pos_value(&mut named, &positional, "direction", 1, context)?,
                context,
            )?;
            let intensity = preview_expect_vec3(
                &preview_named_or_pos_value(&mut named, &positional, "intensity", 2, context)?,
                context,
            )?;
            let range = preview_expect_f32(
                &preview_named_or_pos_value(&mut named, &positional, "range", 3, context)?,
                context,
            )?;
            Ok(wrela::presentation_exec::light_value(
                wrela::presentation_contract::CanonicalLightInput {
                    position,
                    direction,
                    intensity,
                    range,
                },
            ))
        }
        "Camera" => {
            let position = preview_expect_vec3(
                &preview_named_or_pos_value(&mut named, &positional, "position", 0, context)?,
                context,
            )?;
            let forward = preview_expect_vec3(
                &preview_named_or_pos_value(&mut named, &positional, "forward", 1, context)?,
                context,
            )?;
            let up = preview_expect_vec3(
                &preview_named_or_pos_value(&mut named, &positional, "up", 2, context)?,
                context,
            )?;
            let vertical_fov_degrees = preview_expect_f32(
                &preview_named_or_pos_value(
                    &mut named,
                    &positional,
                    "vertical_fov_degrees",
                    3,
                    context,
                )?,
                context,
            )?;
            Ok(preview_camera_value(
                wrela::presentation_contract::CanonicalCameraInput {
                    position,
                    forward,
                    up,
                    vertical_fov_degrees,
                },
            ))
        }
        "f32" => Ok(wrela::kernel::KernelValue::F32(preview_expect_f32(
            &preview_named_or_pos_value(&mut named, &positional, "value", 0, context)?,
            context,
        )?)),
        "i32" => Ok(wrela::kernel::KernelValue::I32(preview_expect_i32(
            &preview_named_or_pos_value(&mut named, &positional, "value", 0, context)?,
            context,
        )?)),
        "u32" => Ok(wrela::kernel::KernelValue::U32(preview_expect_u32(
            &preview_named_or_pos_value(&mut named, &positional, "value", 0, context)?,
            context,
        )?)),
        _ => Err(format!(
            "{context} does not support preview evaluation for call `{callee}`"
        )),
    }
}

fn preview_eval_call_arguments(
    body: &hir::Body,
    args: &[hir::Arg],
    bindings: &PreviewEvalBindings,
    context: &str,
) -> Result<(Vec<wrela::kernel::KernelValue>, PreviewEvalBindings), String> {
    let mut positional = Vec::new();
    let mut named = PreviewEvalBindings::new();
    for arg in args {
        match arg {
            hir::Arg::Positional { value, .. } => {
                positional.push(preview_eval_expr(body, *value, bindings, context)?);
            }
            hir::Arg::Named { name, value, .. } => {
                named.insert(
                    name.clone(),
                    preview_eval_expr(body, *value, bindings, context)?,
                );
            }
        }
    }
    Ok((positional, named))
}

fn preview_named_or_pos_expr(
    args: &[hir::Arg],
    name: &str,
    index: usize,
) -> Option<hir::Idx<hir::Expr>> {
    args.iter()
        .find_map(|arg| match arg {
            hir::Arg::Named {
                name: arg_name,
                value,
                ..
            } if arg_name == name => Some(*value),
            _ => None,
        })
        .or_else(|| {
            args.iter()
                .filter_map(|arg| match arg {
                    hir::Arg::Positional { value, .. } => Some(*value),
                    _ => None,
                })
                .nth(index)
        })
}

fn preview_named_or_pos_value(
    named: &mut PreviewEvalBindings,
    positional: &[wrela::kernel::KernelValue],
    name: &str,
    index: usize,
    context: &str,
) -> Result<wrela::kernel::KernelValue, String> {
    named
        .remove(name)
        .or_else(|| positional.get(index).cloned())
        .ok_or_else(|| format!("{context} is missing `{name}`"))
}

fn preview_capture_region_name(body: &hir::Body, expr_id: hir::Idx<hir::Expr>) -> Option<SmolStr> {
    match &body.exprs[expr_id] {
        hir::Expr::Variable(name) => Some(name.clone()),
        hir::Expr::Call { callee, .. } => match &body.exprs[*callee] {
            hir::Expr::Variable(name) => Some(name.clone()),
            _ => None,
        },
        _ => None,
    }
}

fn preview_literal_value(
    literal: &hir::Literal,
    context: &str,
) -> Result<wrela::kernel::KernelValue, String> {
    match literal {
        hir::Literal::Integer(value) => Ok(wrela::kernel::KernelValue::I32(*value as i32)),
        hir::Literal::Float(value) => Ok(wrela::kernel::KernelValue::F32(*value as f32)),
        hir::Literal::Boolean(value) => Ok(wrela::kernel::KernelValue::Bool(*value)),
        _ => Err(format!("{context} does not support that literal kind")),
    }
}

fn preview_apply_unary(
    op: hir::UnaryOp,
    value: wrela::kernel::KernelValue,
    context: &str,
) -> Result<wrela::kernel::KernelValue, String> {
    match (op, value) {
        (hir::UnaryOp::Neg, wrela::kernel::KernelValue::I32(value)) => {
            Ok(wrela::kernel::KernelValue::I32(-value))
        }
        (hir::UnaryOp::Neg, wrela::kernel::KernelValue::F32(value)) => {
            Ok(wrela::kernel::KernelValue::F32(-value))
        }
        (hir::UnaryOp::Neg, wrela::kernel::KernelValue::Vec3(value)) => {
            Ok(wrela::kernel::KernelValue::Vec3([
                -value[0], -value[1], -value[2],
            ]))
        }
        _ => Err(format!("{context} does not support that unary operation")),
    }
}

fn preview_apply_binary(
    lhs: wrela::kernel::KernelValue,
    op: hir::BinaryOp,
    rhs: wrela::kernel::KernelValue,
    context: &str,
) -> Result<wrela::kernel::KernelValue, String> {
    match op {
        hir::BinaryOp::Add => match (&lhs, &rhs) {
            (wrela::kernel::KernelValue::Vec3(lhs), wrela::kernel::KernelValue::Vec3(rhs)) => {
                Ok(wrela::kernel::KernelValue::Vec3([
                    lhs[0] + rhs[0],
                    lhs[1] + rhs[1],
                    lhs[2] + rhs[2],
                ]))
            }
            _ => preview_numeric_binary(lhs, rhs, |lhs, rhs| lhs + rhs, |lhs, rhs| lhs + rhs),
        },
        hir::BinaryOp::Sub => match (&lhs, &rhs) {
            (wrela::kernel::KernelValue::Vec3(lhs), wrela::kernel::KernelValue::Vec3(rhs)) => {
                Ok(wrela::kernel::KernelValue::Vec3([
                    lhs[0] - rhs[0],
                    lhs[1] - rhs[1],
                    lhs[2] - rhs[2],
                ]))
            }
            _ => preview_numeric_binary(lhs, rhs, |lhs, rhs| lhs - rhs, |lhs, rhs| lhs - rhs),
        },
        hir::BinaryOp::Mul => match (&lhs, &rhs) {
            (wrela::kernel::KernelValue::Vec3(value), scalar) => {
                let scalar = preview_expect_f32(scalar, context)?;
                Ok(wrela::kernel::KernelValue::Vec3([
                    value[0] * scalar,
                    value[1] * scalar,
                    value[2] * scalar,
                ]))
            }
            (scalar, wrela::kernel::KernelValue::Vec3(value)) => {
                let scalar = preview_expect_f32(scalar, context)?;
                Ok(wrela::kernel::KernelValue::Vec3([
                    value[0] * scalar,
                    value[1] * scalar,
                    value[2] * scalar,
                ]))
            }
            _ => preview_numeric_binary(lhs, rhs, |lhs, rhs| lhs * rhs, |lhs, rhs| lhs * rhs),
        },
        hir::BinaryOp::Div => match (&lhs, &rhs) {
            (wrela::kernel::KernelValue::Vec3(value), scalar) => {
                let scalar = preview_expect_f32(scalar, context)?;
                Ok(wrela::kernel::KernelValue::Vec3([
                    value[0] / scalar,
                    value[1] / scalar,
                    value[2] / scalar,
                ]))
            }
            _ => preview_numeric_binary(lhs, rhs, |lhs, rhs| lhs / rhs, |lhs, rhs| lhs / rhs),
        },
        _ => Err(format!("{context} does not support that binary operation")),
    }
}

fn preview_numeric_binary(
    lhs: wrela::kernel::KernelValue,
    rhs: wrela::kernel::KernelValue,
    integer_op: impl FnOnce(i32, i32) -> i32,
    float_op: impl FnOnce(f32, f32) -> f32,
) -> Result<wrela::kernel::KernelValue, String> {
    match (&lhs, &rhs) {
        (wrela::kernel::KernelValue::I32(lhs), wrela::kernel::KernelValue::I32(rhs)) => {
            Ok(wrela::kernel::KernelValue::I32(integer_op(*lhs, *rhs)))
        }
        _ => Ok(wrela::kernel::KernelValue::F32(float_op(
            preview_scalar_f32(&lhs)?,
            preview_scalar_f32(&rhs)?,
        ))),
    }
}

fn preview_scalar_f32(value: &wrela::kernel::KernelValue) -> Result<f32, String> {
    match value {
        wrela::kernel::KernelValue::I32(value) => Ok(*value as f32),
        wrela::kernel::KernelValue::U32(value) => Ok(*value as f32),
        wrela::kernel::KernelValue::F32(value) => Ok(*value),
        _ => Err("expected a scalar numeric value".to_string()),
    }
}

fn preview_struct_field(
    value: &wrela::kernel::KernelValue,
    field_name: &str,
    context: &str,
) -> Result<wrela::kernel::KernelValue, String> {
    let wrela::kernel::KernelValue::Struct(record) = value else {
        return Err(format!(
            "{context} expected a struct value for .{field_name}"
        ));
    };
    record
        .fields
        .iter()
        .find(|(name, _)| name == field_name)
        .map(|(_, value)| value.clone())
        .ok_or_else(|| format!("{context} could not find field `{field_name}`"))
}

fn preview_expect_f32(value: &wrela::kernel::KernelValue, context: &str) -> Result<f32, String> {
    preview_scalar_f32(value).map_err(|_| format!("{context} expected an f32-compatible value"))
}

fn preview_expect_i32(value: &wrela::kernel::KernelValue, context: &str) -> Result<i32, String> {
    match value {
        wrela::kernel::KernelValue::I32(value) => Ok(*value),
        wrela::kernel::KernelValue::U32(value) => Ok(*value as i32),
        wrela::kernel::KernelValue::F32(value) => Ok(*value as i32),
        _ => Err(format!("{context} expected an i32-compatible value")),
    }
}

fn preview_expect_u32(value: &wrela::kernel::KernelValue, context: &str) -> Result<u32, String> {
    match value {
        wrela::kernel::KernelValue::I32(value) => Ok((*value).max(0) as u32),
        wrela::kernel::KernelValue::U32(value) => Ok(*value),
        wrela::kernel::KernelValue::F32(value) => Ok(value.max(0.0) as u32),
        _ => Err(format!("{context} expected a u32-compatible value")),
    }
}

fn preview_expect_vec3(
    value: &wrela::kernel::KernelValue,
    context: &str,
) -> Result<[f32; 3], String> {
    match value {
        wrela::kernel::KernelValue::Vec3(value) => Ok(*value),
        _ => Err(format!("{context} expected a vec3 value")),
    }
}

fn preview_expect_light(
    value: &wrela::kernel::KernelValue,
    context: &str,
) -> Result<wrela::presentation_contract::CanonicalLightInput, String> {
    let position =
        preview_expect_vec3(&preview_struct_field(value, "position", context)?, context)?;
    let direction =
        preview_expect_vec3(&preview_struct_field(value, "direction", context)?, context)?;
    let intensity =
        preview_expect_vec3(&preview_struct_field(value, "intensity", context)?, context)?;
    let range = preview_expect_f32(&preview_struct_field(value, "range", context)?, context)?;
    Ok(wrela::presentation_contract::CanonicalLightInput {
        position,
        direction,
        intensity,
        range,
    })
}

fn preview_camera_value(
    camera: wrela::presentation_contract::CanonicalCameraInput,
) -> wrela::kernel::KernelValue {
    wrela::kernel::KernelValue::Struct(wrela::kernel::KernelStructValue {
        name: SmolStr::new("Camera"),
        fields: vec![
            (
                SmolStr::new("position"),
                wrela::kernel::KernelValue::Vec3(camera.position),
            ),
            (
                SmolStr::new("forward"),
                wrela::kernel::KernelValue::Vec3(camera.forward),
            ),
            (
                SmolStr::new("up"),
                wrela::kernel::KernelValue::Vec3(camera.up),
            ),
            (
                SmolStr::new("vertical_fov_degrees"),
                wrela::kernel::KernelValue::F32(camera.vertical_fov_degrees),
            ),
        ],
    })
}

fn default_preview_key_light() -> wrela::presentation_contract::CanonicalLightInput {
    wrela::presentation_contract::CanonicalLightInput {
        position: [2.4, 2.8, 2.4],
        direction: normalize_preview_vec3([-0.8, -0.9, -0.9]),
        intensity: [1.0, 0.98, 0.95],
        range: 12.0,
    }
}

fn normalize_preview_vec3(value: [f32; 3]) -> [f32; 3] {
    let len_sq = value[0] * value[0] + value[1] * value[1] + value[2] * value[2];
    if len_sq <= f32::EPSILON {
        return value;
    }
    let inv_len = len_sq.sqrt().recip();
    [value[0] * inv_len, value[1] * inv_len, value[2] * inv_len]
}

fn parse_presentation_debug_options(args: &[String]) -> Result<PresentationDebugOptions, String> {
    let mut options = PresentationDebugOptions {
        view: None,
        region: None,
        domain: None,
        out_dir: None,
        width: None,
        height: None,
        camera_position: [0.0, 0.0, 2.5],
        camera_forward: [0.0, 0.0, -1.0],
        camera_up: [0.0, 1.0, 0.0],
        vertical_fov_degrees: 60.0,
        frame_index: 0,
        delta_seconds: 1.0 / 60.0,
        frames: 1,
    };
    let mut index = 0usize;
    while index < args.len() {
        let arg = &args[index];
        let (flag, inline_value) = match arg.split_once('=') {
            Some((flag, value)) if flag.starts_with("--") => (flag, Some(value.to_string())),
            _ => (arg.as_str(), None),
        };
        let take_value = |inline_value: &Option<String>,
                          args: &[String],
                          index: &mut usize|
         -> Result<String, String> {
            if let Some(value) = inline_value {
                return Ok(value.clone());
            }
            *index += 1;
            args.get(*index)
                .cloned()
                .ok_or_else(|| format!("missing value for {flag}"))
        };
        match flag {
            "--view" => options.view = Some(take_value(&inline_value, args, &mut index)?),
            "--region" => options.region = Some(take_value(&inline_value, args, &mut index)?),
            "--domain" => options.domain = Some(take_value(&inline_value, args, &mut index)?),
            "--out-dir" => {
                options.out_dir = Some(PathBuf::from(take_value(&inline_value, args, &mut index)?))
            }
            "--width" => {
                options.width = Some(
                    take_value(&inline_value, args, &mut index)?
                        .parse()
                        .map_err(|_| "invalid --width value".to_string())?,
                )
            }
            "--height" => {
                options.height = Some(
                    take_value(&inline_value, args, &mut index)?
                        .parse()
                        .map_err(|_| "invalid --height value".to_string())?,
                )
            }
            "--camera-position" => {
                options.camera_position =
                    parse_vec3_flag(&take_value(&inline_value, args, &mut index)?, flag)?
            }
            "--camera-forward" => {
                options.camera_forward =
                    parse_vec3_flag(&take_value(&inline_value, args, &mut index)?, flag)?
            }
            "--camera-up" => {
                options.camera_up =
                    parse_vec3_flag(&take_value(&inline_value, args, &mut index)?, flag)?
            }
            "--fov" => {
                options.vertical_fov_degrees = take_value(&inline_value, args, &mut index)?
                    .parse()
                    .map_err(|_| "invalid --fov value".to_string())?
            }
            "--frame-index" => {
                options.frame_index = take_value(&inline_value, args, &mut index)?
                    .parse()
                    .map_err(|_| "invalid --frame-index value".to_string())?
            }
            "--delta-seconds" => {
                options.delta_seconds = take_value(&inline_value, args, &mut index)?
                    .parse()
                    .map_err(|_| "invalid --delta-seconds value".to_string())?
            }
            "--frames" => {
                options.frames = take_value(&inline_value, args, &mut index)?
                    .parse()
                    .map_err(|_| "invalid --frames value".to_string())?
            }
            _ => return Err(format!("unexpected argument `{arg}`")),
        }
        index += 1;
    }
    Ok(options)
}

fn parse_preview_command_options(args: &[String]) -> Result<PreviewCommandOptions, String> {
    let mut options = PreviewCommandOptions {
        view: None,
        region: None,
        domain: None,
        width: None,
        height: None,
        camera_position: [0.0, 0.0, 2.5],
        camera_forward: [0.0, 0.0, -1.0],
        camera_up: [0.0, 1.0, 0.0],
        vertical_fov_degrees: 60.0,
        frame_index: 0,
        delta_seconds: 1.0 / 60.0,
        attachment: "color".to_string(),
        json_report: false,
    };
    let mut index = 0usize;
    while index < args.len() {
        let arg = &args[index];
        let (flag, inline_value) = match arg.split_once('=') {
            Some((flag, value)) if flag.starts_with("--") => (flag, Some(value.to_string())),
            _ => (arg.as_str(), None),
        };
        let take_value = |inline_value: &Option<String>,
                          args: &[String],
                          index: &mut usize|
         -> Result<String, String> {
            if let Some(value) = inline_value {
                return Ok(value.clone());
            }
            *index += 1;
            args.get(*index)
                .cloned()
                .ok_or_else(|| format!("missing value for {flag}"))
        };
        match flag {
            "--view" => options.view = Some(take_value(&inline_value, args, &mut index)?),
            "--region" => options.region = Some(take_value(&inline_value, args, &mut index)?),
            "--domain" => options.domain = Some(take_value(&inline_value, args, &mut index)?),
            "--width" => {
                options.width = Some(
                    take_value(&inline_value, args, &mut index)?
                        .parse()
                        .map_err(|_| "invalid --width value".to_string())?,
                )
            }
            "--height" => {
                options.height = Some(
                    take_value(&inline_value, args, &mut index)?
                        .parse()
                        .map_err(|_| "invalid --height value".to_string())?,
                )
            }
            "--camera-position" => {
                options.camera_position =
                    parse_vec3_flag(&take_value(&inline_value, args, &mut index)?, flag)?
            }
            "--camera-forward" => {
                options.camera_forward =
                    parse_vec3_flag(&take_value(&inline_value, args, &mut index)?, flag)?
            }
            "--camera-up" => {
                options.camera_up =
                    parse_vec3_flag(&take_value(&inline_value, args, &mut index)?, flag)?
            }
            "--fov" => {
                options.vertical_fov_degrees = take_value(&inline_value, args, &mut index)?
                    .parse()
                    .map_err(|_| "invalid --fov value".to_string())?
            }
            "--frame-index" => {
                options.frame_index = take_value(&inline_value, args, &mut index)?
                    .parse()
                    .map_err(|_| "invalid --frame-index value".to_string())?
            }
            "--delta-seconds" => {
                options.delta_seconds = take_value(&inline_value, args, &mut index)?
                    .parse()
                    .map_err(|_| "invalid --delta-seconds value".to_string())?
            }
            "--attachment" => {
                options.attachment = take_value(&inline_value, args, &mut index)?;
            }
            "--json-report" => options.json_report = true,
            _ => return Err(format!("unexpected argument `{arg}`")),
        }
        index += 1;
    }
    Ok(options)
}

fn parse_frame_command_options(args: &[String]) -> Result<FrameCommandOptions, String> {
    let mut options = FrameCommandOptions {
        view: None,
        region: None,
        domain: None,
        width: None,
        height: None,
        camera_position: [0.0, 0.0, 2.5],
        camera_forward: [0.0, 0.0, -1.0],
        camera_up: [0.0, 1.0, 0.0],
        vertical_fov_degrees: 60.0,
        frame_index: 0,
        delta_seconds: 1.0 / 60.0,
        attachments: Vec::new(),
        attachment_format: FrameAttachmentFormat::Json,
    };
    let mut index = 0usize;
    while index < args.len() {
        let arg = &args[index];
        let (flag, inline_value) = match arg.split_once('=') {
            Some((flag, value)) if flag.starts_with("--") => (flag, Some(value.to_string())),
            _ => (arg.as_str(), None),
        };
        let take_value = |inline_value: &Option<String>,
                          args: &[String],
                          index: &mut usize|
         -> Result<String, String> {
            if let Some(value) = inline_value {
                return Ok(value.clone());
            }
            *index += 1;
            args.get(*index)
                .cloned()
                .ok_or_else(|| format!("missing value for {flag}"))
        };
        match flag {
            "--view" => options.view = Some(take_value(&inline_value, args, &mut index)?),
            "--region" => options.region = Some(take_value(&inline_value, args, &mut index)?),
            "--domain" => options.domain = Some(take_value(&inline_value, args, &mut index)?),
            "--width" => {
                options.width = Some(
                    take_value(&inline_value, args, &mut index)?
                        .parse()
                        .map_err(|_| "invalid --width value".to_string())?,
                )
            }
            "--height" => {
                options.height = Some(
                    take_value(&inline_value, args, &mut index)?
                        .parse()
                        .map_err(|_| "invalid --height value".to_string())?,
                )
            }
            "--camera-position" => {
                options.camera_position =
                    parse_vec3_flag(&take_value(&inline_value, args, &mut index)?, flag)?
            }
            "--camera-forward" => {
                options.camera_forward =
                    parse_vec3_flag(&take_value(&inline_value, args, &mut index)?, flag)?
            }
            "--camera-up" => {
                options.camera_up =
                    parse_vec3_flag(&take_value(&inline_value, args, &mut index)?, flag)?
            }
            "--fov" => {
                options.vertical_fov_degrees = take_value(&inline_value, args, &mut index)?
                    .parse()
                    .map_err(|_| "invalid --fov value".to_string())?
            }
            "--frame-index" => {
                options.frame_index = take_value(&inline_value, args, &mut index)?
                    .parse()
                    .map_err(|_| "invalid --frame-index value".to_string())?
            }
            "--delta-seconds" => {
                options.delta_seconds = take_value(&inline_value, args, &mut index)?
                    .parse()
                    .map_err(|_| "invalid --delta-seconds value".to_string())?
            }
            "--attachment" => options
                .attachments
                .push(take_value(&inline_value, args, &mut index)?),
            "--attachment-format" => {
                options.attachment_format = match take_value(&inline_value, args, &mut index)?
                    .as_str()
                {
                    "json" => FrameAttachmentFormat::Json,
                    "ppm" => FrameAttachmentFormat::Ppm,
                    other => {
                        return Err(format!(
                            "invalid --attachment-format value `{other}` (expected json or ppm)"
                        ));
                    }
                }
            }
            _ => return Err(format!("unexpected argument `{arg}`")),
        }
        index += 1;
    }
    Ok(options)
}

fn parse_frame_contracts_view(args: &[String]) -> Result<Option<String>, String> {
    let mut view = None;
    let mut index = 0usize;
    while index < args.len() {
        let arg = &args[index];
        let (flag, inline_value) = match arg.split_once('=') {
            Some((flag, value)) if flag.starts_with("--") => (flag, Some(value.to_string())),
            _ => (arg.as_str(), None),
        };
        match flag {
            "--view" => {
                if let Some(value) = inline_value {
                    view = Some(value);
                } else {
                    index += 1;
                    view = Some(
                        args.get(index)
                            .cloned()
                            .ok_or_else(|| "missing value for --view".to_string())?,
                    );
                }
            }
            _ => return Err(format!("unexpected argument `{arg}`")),
        }
        index += 1;
    }
    Ok(view)
}

fn parse_vec3_flag(value: &str, flag: &str) -> Result<[f32; 3], String> {
    let lanes = value
        .split(',')
        .map(|lane| lane.trim().parse::<f32>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| format!("invalid {flag} value `{value}`"))?;
    if lanes.len() != 3 {
        return Err(format!("invalid {flag} value `{value}` (expected x,y,z)"));
    }
    Ok([lanes[0], lanes[1], lanes[2]])
}

fn select_view_plan<'a>(
    bundle: &'a CompiledPresentationBundle,
    requested: Option<&str>,
) -> Result<&'a wrela::presentation_plan::PresentationPlan, String> {
    let mut candidates = bundle
        .plans
        .iter()
        .filter(|plan| {
            bundle
                .module
                .functions
                .iter()
                .any(|(_, func)| func.name == plan.name && func.role == hir::FunctionRole::View)
        })
        .collect::<Vec<_>>();
    if let Some(requested) = requested {
        return candidates
            .into_iter()
            .find(|plan| plan.name == requested)
            .ok_or_else(|| format!("missing view `{requested}`"));
    }
    if candidates.len() == 1 {
        Ok(candidates.remove(0))
    } else {
        Err("presentation execution requires --view when multiple view plans exist".to_string())
    }
}

fn select_region_name(module: &hir::Module, requested: Option<&str>) -> Result<SmolStr, String> {
    let mut candidates = module
        .functions
        .iter()
        .filter(|(_, func)| func.role == hir::FunctionRole::Region)
        .map(|(_, func)| func.name.clone())
        .collect::<Vec<_>>();
    if let Some(requested) = requested {
        return candidates
            .into_iter()
            .find(|name| name == requested)
            .ok_or_else(|| format!("missing region `{requested}`"));
    }
    if candidates.len() == 1 {
        Ok(candidates.remove(0))
    } else {
        Err("presentation execution requires --region when multiple regions exist".to_string())
    }
}

fn select_domain_name(
    module: &hir::Module,
    view: &hir::Function,
    requested: Option<&str>,
) -> Result<SmolStr, String> {
    if let Some(requested) = requested {
        return module
            .functions
            .iter()
            .find(|(_, func)| func.name == requested && func.role == hir::FunctionRole::Domain)
            .map(|(_, func)| func.name.clone())
            .ok_or_else(|| format!("missing domain `{requested}`"));
    }
    if let Some(domain_body) = view
        .render
        .as_ref()
        .and_then(|metadata| metadata.frame.domain.as_ref())
        && let Some(name) = body_called_function_name(domain_body)
    {
        return Ok(name);
    }
    let mut candidates = module
        .functions
        .iter()
        .filter(|(_, func)| func.role == hir::FunctionRole::Domain)
        .map(|(_, func)| func.name.clone())
        .collect::<Vec<_>>();
    if candidates.len() == 1 {
        Ok(candidates.remove(0))
    } else {
        Err("presentation execution requires --domain when the view does not name a single domain".to_string())
    }
}

fn body_called_function_name(body: &hir::Body) -> Option<SmolStr> {
    let expr_id = body_terminal_expr_id(body)?;
    let hir::Expr::Call { callee, .. } = &body.exprs[expr_id] else {
        return None;
    };
    let hir::Expr::Variable(name) = &body.exprs[*callee] else {
        return None;
    };
    Some(name.clone())
}

fn body_terminal_expr_id(body: &hir::Body) -> Option<hir::Idx<hir::Expr>> {
    let stmt = body.root_stmts.last()?;
    match body.stmts[*stmt] {
        hir::Stmt::Expr(expr) => Some(expr),
        hir::Stmt::Return(Some(expr)) => Some(expr),
        _ => None,
    }
}

fn body_terminal_call_args<'a>(body: &'a hir::Body) -> Option<(&'a SmolStr, &'a [hir::Arg])> {
    let expr_id = body_terminal_expr_id(body)?;
    let hir::Expr::Call { callee, args, .. } = &body.exprs[expr_id] else {
        return None;
    };
    let hir::Expr::Variable(name) = &body.exprs[*callee] else {
        return None;
    };
    Some((name, args.as_slice()))
}

fn helper_call_named_expr_id(
    body: &hir::Body,
    helper_name: &str,
    arg_name: &str,
) -> Option<hir::Idx<hir::Expr>> {
    let (callee, args) = body_terminal_call_args(body)?;
    if callee != helper_name {
        return None;
    }
    args.iter().find_map(|arg| match arg {
        hir::Arg::Named { name, value, .. } if name == arg_name => Some(*value),
        _ => None,
    })
}

fn resolve_view_dimension(
    view: &hir::Function,
    override_value: Option<u32>,
    width: bool,
) -> Result<u32, String> {
    if let Some(value) = override_value {
        return Ok(value);
    }
    let metadata = view
        .render
        .as_ref()
        .ok_or_else(|| "selected view is missing render metadata".to_string())?;
    let label = if width { "width" } else { "height" };
    if let Some(viewport_body) = metadata.view.viewport.as_ref()
        && let Some(value) = helper_call_named_expr_id(viewport_body, "viewport", label)
    {
        return eval_expr_u32(viewport_body, value).ok_or_else(|| {
            format!(
                "presentation execution cannot evaluate non-literal view {label}; pass --{label} explicitly"
            )
        });
    }
    let body = if width {
        metadata.view.width.as_ref()
    } else {
        metadata.view.height.as_ref()
    }
    .ok_or_else(|| format!("presentation execution requires --{label} when the view omits {label}"))?;
    eval_body_u32(body).ok_or_else(|| {
        format!(
            "presentation execution cannot evaluate non-literal view {label}; pass --{label} explicitly"
        )
    })
}

fn eval_body_u32(body: &hir::Body) -> Option<u32> {
    let expr_id = body_terminal_expr_id(body)?;
    eval_expr_u32(body, expr_id)
}

fn eval_body_i32(body: &hir::Body) -> Option<i32> {
    let expr_id = body_terminal_expr_id(body)?;
    eval_expr_i32(body, expr_id)
}

fn eval_expr_i32(body: &hir::Body, expr_id: hir::Idx<hir::Expr>) -> Option<i32> {
    match &body.exprs[expr_id] {
        hir::Expr::Literal(hir::Literal::Integer(value)) => Some(*value as i32),
        hir::Expr::Literal(hir::Literal::Float(value)) => Some(*value as i32),
        _ => None,
    }
}

fn eval_body_f32(body: &hir::Body) -> Option<f32> {
    let expr_id = body_terminal_expr_id(body)?;
    eval_expr_f32(body, expr_id)
}

fn eval_expr_u32(body: &hir::Body, expr_id: hir::Idx<hir::Expr>) -> Option<u32> {
    eval_expr_f32(body, expr_id).map(|value| value.max(0.0) as u32)
}

fn eval_expr_f32(body: &hir::Body, expr_id: hir::Idx<hir::Expr>) -> Option<f32> {
    match &body.exprs[expr_id] {
        hir::Expr::Literal(hir::Literal::Integer(value)) => Some(*value as f32),
        hir::Expr::Literal(hir::Literal::Float(value)) => Some(*value as f32),
        _ => None,
    }
}

fn domain_execution_inputs(
    domain: &hir::Function,
    region_name: &SmolStr,
) -> Result<
    (
        wrela::kernel::KernelValue,
        wrela::presentation_contract::CanonicalRayBudget,
    ),
    String,
> {
    let metadata = domain.domain.as_ref().expect("domain metadata");
    let geometry_detail = match metadata.geometry_detail {
        hir::DomainGeometryDetail::Coarse => 0,
        hir::DomainGeometryDetail::Fine => 1,
    };
    Ok((
        wrela::presentation_exec::scene_domain_value(
            wrela::query_exec::stable_region_scene_capture_id(region_name),
            geometry_detail,
            metadata.material,
            metadata.radiance,
            metadata.media,
        ),
        wrela::presentation_contract::CanonicalRayBudget {
            max_distance: authored_domain_f32(
                metadata.max_distance.as_ref(),
                "max_distance",
                16.0,
            )?,
            min_step: authored_domain_f32(metadata.min_step.as_ref(), "min_step", 0.01)?,
            hit_epsilon: authored_domain_f32(metadata.hit_epsilon.as_ref(), "hit_epsilon", 0.001)?,
            max_steps: authored_domain_i32(metadata.max_steps.as_ref(), "max_steps", 128)?,
        },
    ))
}

fn authored_domain_f32(body: Option<&hir::Body>, field: &str, default: f32) -> Result<f32, String> {
    match body {
        Some(body) => eval_body_f32(body).ok_or_else(|| {
            format!(
                "presentation execution cannot evaluate non-literal domain {field}; author a terminal numeric literal for now"
            )
        }),
        None => Ok(default),
    }
}

fn authored_domain_i32(body: Option<&hir::Body>, field: &str, default: i32) -> Result<i32, String> {
    match body {
        Some(body) => eval_body_i32(body).ok_or_else(|| {
            format!(
                "presentation execution cannot evaluate non-literal domain {field}; author a terminal numeric literal for now"
            )
        }),
        None => Ok(default),
    }
}

fn presentation_plan_dump(
    entry_path: &Path,
    plans: &[wrela::presentation_plan::PresentationPlan],
) -> PresentationPlanDump {
    PresentationPlanDump {
        schema_version: 1,
        entry_path: entry_path.display().to_string(),
        plans: plans.iter().map(presentation_plan_dump_item).collect(),
    }
}

fn presentation_plan_dump_item(
    plan: &wrela::presentation_plan::PresentationPlan,
) -> PresentationPlanDumpItem {
    PresentationPlanDumpItem {
        name: plan.name.to_string(),
        view: PresentationViewDump {
            canonical_projection: plan.view.canonical_projection,
            canonical_projection_input: canonical_projection_input_name(
                plan.view.canonical_projection_input,
            ),
            screen_lattice: PresentationScreenLatticeDump {
                sample_position: screen_sample_position_name(
                    plan.view.screen_lattice.sample_position,
                )
                .to_string(),
                origin: screen_lattice_origin_name(plan.view.screen_lattice.origin).to_string(),
                width_source: plan.view.screen_lattice.width_source.to_string(),
                height_source: plan.view.screen_lattice.height_source.to_string(),
            },
            canonical_view_ray: PresentationViewRayDump {
                space: view_ray_space_name(plan.view.canonical_view_ray.space).to_string(),
                normalized_direction: plan.view.canonical_view_ray.normalized_direction,
                projection_input: canonical_projection_input_name(
                    plan.view.canonical_view_ray.projection_input,
                ),
            },
            allows_legacy_projection_override: plan.view.allows_legacy_projection_override,
            compatibility_projection: PresentationCompatibilityProjectionDump {
                legacy_path_active: plan.view.compatibility_projection.legacy_path_active,
                authored_world_up_override: plan
                    .view
                    .compatibility_projection
                    .authored_world_up_override,
                authored_view_scale_override: plan
                    .view
                    .compatibility_projection
                    .authored_view_scale_override,
            },
        },
        frame: PresentationFrameDump {
            outputs: plan
                .frame
                .outputs
                .iter()
                .map(|output| PresentationAttachmentDump {
                    name: output.name.to_string(),
                    kind: frame_attachment_kind_name(output.kind).to_string(),
                    element_schema: attachment_element_schema_name(&output.element_schema),
                    lifetime: attachment_lifetime_name(output.lifetime),
                    resolution: attachment_resolution_name(output.resolution).to_string(),
                    scale: attachment_resolution_scale_name(output.scale),
                    clear_policy: attachment_clear_policy_name(output.clear_policy).to_string(),
                })
                .collect(),
            primary_hit: plan.frame.primary_hit.as_ref().map(|primary_hit| {
                PresentationPrimaryHitDump {
                    attachment: primary_hit.attachment.to_string(),
                    record: primary_hit.record.to_string(),
                    fields: primary_hit.fields.iter().map(ToString::to_string).collect(),
                    depth_semantics: depth_semantics_name(primary_hit.depth_semantics).to_string(),
                    sample_identity: primary_hit.sample_identity.to_string(),
                }
            }),
            temporal_reuse: plan
                .frame
                .temporal
                .as_ref()
                .map(|temporal| temporal_reuse_name(temporal.reuse).to_string()),
            quality: PresentationQualityDump {
                tier: wrela::presentation_plan::quality_tier_name(plan.frame.quality.tier)
                    .to_string(),
                target_fps: plan.frame.quality.target_fps,
                internal_resolution_scale: plan.frame.quality.internal_resolution_scale,
                allow_dynamic_resolution: plan.frame.quality.allow_dynamic_resolution,
                primary_max_steps: plan.frame.quality.primary_max_steps,
                allow_radiance: plan.frame.quality.allow_radiance,
                allow_media: plan.frame.quality.allow_media,
                temporal_mode: temporal_reuse_name(plan.frame.quality.temporal_mode).to_string(),
                allow_half_res_participants: plan.frame.quality.allow_half_res_participants,
                allow_hit_compaction: plan.frame.quality.allow_hit_compaction,
                degradation_order: plan
                    .frame
                    .quality
                    .degradation_order
                    .iter()
                    .map(|step| {
                        wrela::presentation_plan::quality_degradation_step_name(*step).to_string()
                    })
                    .collect(),
            },
            lighting: PresentationLightingDump {
                key_light: presentation_lighting_input_dump(&plan.frame.lighting.key_light),
                fill_direction: presentation_lighting_input_dump(
                    &plan.frame.lighting.fill_direction,
                ),
                fill_strength: presentation_lighting_input_dump(&plan.frame.lighting.fill_strength),
                ambient_color: presentation_lighting_input_dump(&plan.frame.lighting.ambient_color),
                allows_legacy_plural_lights_metadata: plan
                    .frame
                    .lighting
                    .allows_legacy_plural_lights_metadata,
            },
            observability: contract_observability_names(&plan.frame.observability),
        },
        passes: plan
            .passes
            .iter()
            .map(|pass| PresentationPassDump {
                id: pass.id.to_string(),
                kind: presentation_pass_kind_name(&pass.kind),
                screen_samples: presentation_screen_sample_pass_dump(&pass.kind),
                consumes: pass.consumes.iter().map(ToString::to_string).collect(),
                materializes: pass.materializes.iter().map(ToString::to_string).collect(),
                binding: pass
                    .binding
                    .as_ref()
                    .map(|binding| binding.as_str().to_string()),
                query_dependencies: pass
                    .query_dependencies
                    .iter()
                    .map(|contract_id| presentation_query_dependency_dump(*contract_id))
                    .collect(),
                future_acceleration_hooks: pass
                    .future_acceleration_hooks
                    .iter()
                    .map(|hook| acceleration_hook_name(*hook).to_string())
                    .collect(),
                observability: pass_observability_names(&pass.observability),
            })
            .collect(),
        frame_artifacts: plan
            .frame_artifacts
            .iter()
            .map(|artifact| PresentationFrameArtifactDump {
                id: artifact.id.to_string(),
                attachment: artifact.attachment.to_string(),
                producer_pass: artifact.producer_pass.to_string(),
                materialized: artifact.materialized,
            })
            .collect(),
        bindings: plan
            .bindings
            .iter()
            .map(|binding| PresentationBindingDump {
                id: binding.id.as_str().to_string(),
                pass_kind: presentation_pass_kind_name(&binding.pass_kind),
                recipe: presentation_recipe_name(binding.recipe).to_string(),
                default_backend: dispatch_backend_name(binding.default_backend).to_string(),
                execution: presentation_binding_execution_name(binding).to_string(),
            })
            .collect(),
        candidate_query_program_concepts: pass_observability_names(&plan.observability),
        validation_errors: plan
            .validate()
            .into_iter()
            .map(|err| err.message.to_string())
            .collect(),
    }
}

fn print_presentation_plan_human(dump: &PresentationPlanDump) {
    println!("presentation plan schema v{}", dump.schema_version);
    println!("entry: {}", dump.entry_path);
    if dump.plans.is_empty() {
        println!("plans: none");
        return;
    }
    for plan in &dump.plans {
        println!("plan {}", plan.name);
        println!(
            "  view: canonical_projection={} input={} compatibility_legacy_path={} authored_world_up_override={} authored_view_scale_override={}",
            plan.view.canonical_projection,
            plan.view.canonical_projection_input,
            plan.view.compatibility_projection.legacy_path_active,
            plan.view
                .compatibility_projection
                .authored_world_up_override,
            plan.view
                .compatibility_projection
                .authored_view_scale_override
        );
        println!(
            "  screen lattice: sample_position={} origin={} width={} height={}",
            plan.view.screen_lattice.sample_position,
            plan.view.screen_lattice.origin,
            plan.view.screen_lattice.width_source,
            plan.view.screen_lattice.height_source
        );
        println!(
            "  canonical view rays: space={} normalized_direction={} projection_input={}",
            plan.view.canonical_view_ray.space,
            plan.view.canonical_view_ray.normalized_direction,
            plan.view.canonical_view_ray.projection_input
        );
        let outputs = plan
            .frame
            .outputs
            .iter()
            .map(|output| {
                format!(
                    "{}({},{},{},{},{},{})",
                    output.name,
                    output.kind,
                    output.element_schema,
                    output.lifetime,
                    output.resolution,
                    output.scale,
                    output.clear_policy
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        println!(
            "  frame outputs: {}",
            if outputs.is_empty() { "none" } else { &outputs }
        );
        if let Some(primary_hit) = &plan.frame.primary_hit {
            println!(
                "  primary hit attachment: {} record={} depth={} sample_identity={} fields={}",
                primary_hit.attachment,
                primary_hit.record,
                primary_hit.depth_semantics,
                primary_hit.sample_identity,
                primary_hit.fields.join(",")
            );
        }
        println!(
            "  quality: tier={} target_fps={} internal_scale={:.2} dynamic_resolution={} primary_max_steps={} radiance={} media={} temporal_mode={} half_res_participants={} hit_compaction={}",
            plan.frame.quality.tier,
            plan.frame.quality.target_fps,
            plan.frame.quality.internal_resolution_scale,
            plan.frame.quality.allow_dynamic_resolution,
            plan.frame.quality.primary_max_steps,
            plan.frame.quality.allow_radiance,
            plan.frame.quality.allow_media,
            plan.frame.quality.temporal_mode,
            plan.frame.quality.allow_half_res_participants,
            plan.frame.quality.allow_hit_compaction
        );
        println!(
            "  quality degradation order: {}",
            if plan.frame.quality.degradation_order.is_empty() {
                "none".to_string()
            } else {
                plan.frame.quality.degradation_order.join(", ")
            }
        );
        println!(
            "  lighting: key_light={} fill_direction={} fill_strength={} ambient_color={} legacy_plural_lights={}",
            format_lighting_input_dump(&plan.frame.lighting.key_light),
            format_lighting_input_dump(&plan.frame.lighting.fill_direction),
            format_lighting_input_dump(&plan.frame.lighting.fill_strength),
            format_lighting_input_dump(&plan.frame.lighting.ambient_color),
            plan.frame.lighting.allows_legacy_plural_lights_metadata
        );
        println!("  passes:");
        for pass in &plan.passes {
            println!(
                "    {} kind={} binding={}",
                pass.id,
                pass.kind,
                pass.binding.as_deref().unwrap_or("none")
            );
            let queries = pass
                .query_dependencies
                .iter()
                .map(|query| {
                    let solver = query
                        .solver_diagnostics
                        .as_ref()
                        .map(|solver| {
                            format!(" [solver={} fallback={}]", solver.plan_id, solver.fallback)
                        })
                        .unwrap_or_default();
                    format!("{}{}", query.contract_id, solver)
                })
                .collect::<Vec<_>>()
                .join(", ");
            println!(
                "      query dependencies: {}",
                if queries.is_empty() { "none" } else { &queries }
            );
            println!(
                "      materializes: {}",
                if pass.materializes.is_empty() {
                    "none".to_string()
                } else {
                    pass.materializes.join(", ")
                }
            );
            if let Some(screen_samples) = &pass.screen_samples {
                println!(
                    "      screen samples: viewport={}x{} samples_per_pixel={} jitter={} item_count={} record={}",
                    screen_samples.viewport_width_source,
                    screen_samples.viewport_height_source,
                    screen_samples.samples_per_pixel,
                    screen_samples.jitter_source,
                    screen_samples.item_count_expression,
                    screen_samples.output_item_record
                );
            }
            println!(
                "      future acceleration hooks: {}",
                if pass.future_acceleration_hooks.is_empty() {
                    "none".to_string()
                } else {
                    pass.future_acceleration_hooks.join(", ")
                }
            );
        }
        println!("  bindings:");
        for binding in &plan.bindings {
            println!(
                "    {} recipe={} backend={} execution={}",
                binding.id, binding.recipe, binding.default_backend, binding.execution
            );
        }
        println!(
            "  candidate query-program concepts: {}",
            plan.candidate_query_program_concepts.join(", ")
        );
    }
}

fn canonical_projection_input_name(
    input: wrela::presentation_contract::CanonicalProjectionInput,
) -> String {
    match input {
        wrela::presentation_contract::CanonicalProjectionInput::CameraVerticalFovDegrees => {
            "Camera.vertical_fov_degrees".to_string()
        }
    }
}

fn screen_sample_position_name(
    position: wrela::presentation_contract::ScreenLatticeSamplePosition,
) -> &'static str {
    match position {
        wrela::presentation_contract::ScreenLatticeSamplePosition::PixelCenter => "PixelCenter",
    }
}

fn screen_lattice_origin_name(
    origin: wrela::presentation_contract::ScreenLatticeOrigin,
) -> &'static str {
    match origin {
        wrela::presentation_contract::ScreenLatticeOrigin::TopLeft => "TopLeft",
    }
}

fn view_ray_space_name(space: wrela::presentation_contract::CanonicalViewRaySpace) -> &'static str {
    match space {
        wrela::presentation_contract::CanonicalViewRaySpace::World => "World",
    }
}

fn depth_semantics_name(
    semantics: wrela::presentation_contract::DepthAttachmentSemantics,
) -> &'static str {
    match semantics {
        wrela::presentation_contract::DepthAttachmentSemantics::RayParameterDistance => {
            "RayParameterDistance"
        }
    }
}

fn frame_attachment_kind_name(
    kind: wrela::presentation_contract::FrameAttachmentKind,
) -> &'static str {
    match kind {
        wrela::presentation_contract::FrameAttachmentKind::PrimaryHit => "PrimaryHit",
        wrela::presentation_contract::FrameAttachmentKind::Depth => "Depth",
        wrela::presentation_contract::FrameAttachmentKind::WorldNormal => "WorldNormal",
        wrela::presentation_contract::FrameAttachmentKind::Surface => "Surface",
        wrela::presentation_contract::FrameAttachmentKind::Radiance => "Radiance",
        wrela::presentation_contract::FrameAttachmentKind::Medium => "Medium",
        wrela::presentation_contract::FrameAttachmentKind::Motion => "Motion",
        wrela::presentation_contract::FrameAttachmentKind::Color => "Color",
    }
}

fn attachment_lifetime_name(lifetime: wrela::presentation_contract::AttachmentLifetime) -> String {
    match lifetime {
        wrela::presentation_contract::AttachmentLifetime::Transient => "Transient".to_string(),
        wrela::presentation_contract::AttachmentLifetime::Exported => "Exported".to_string(),
        wrela::presentation_contract::AttachmentLifetime::HistorySlot(slot) => {
            format!("HistorySlot({slot})")
        }
    }
}

fn attachment_element_schema_name(
    schema: &wrela::presentation_contract::AttachmentElementSchema,
) -> String {
    match schema {
        wrela::presentation_contract::AttachmentElementSchema::NamedRecord(name) => {
            name.to_string()
        }
        wrela::presentation_contract::AttachmentElementSchema::ScalarF32 => "f32".to_string(),
        wrela::presentation_contract::AttachmentElementSchema::Vec2F32 => "vec2<f32>".to_string(),
        wrela::presentation_contract::AttachmentElementSchema::Vec3F32 => "vec3<f32>".to_string(),
        wrela::presentation_contract::AttachmentElementSchema::Vec4F32 => "vec4<f32>".to_string(),
    }
}

fn attachment_resolution_name(
    resolution: wrela::presentation_contract::AttachmentResolutionClass,
) -> &'static str {
    match resolution {
        wrela::presentation_contract::AttachmentResolutionClass::Viewport => "Viewport",
        wrela::presentation_contract::AttachmentResolutionClass::HalfViewport => "HalfViewport",
        wrela::presentation_contract::AttachmentResolutionClass::QuarterViewport => {
            "QuarterViewport"
        }
    }
}

fn attachment_resolution_scale_name(
    scale: wrela::presentation_contract::AttachmentResolutionScale,
) -> String {
    format!("{}x{}", scale.divisor_x, scale.divisor_y)
}

fn attachment_clear_policy_name(
    clear_policy: wrela::presentation_contract::AttachmentClearPolicy,
) -> &'static str {
    match clear_policy {
        wrela::presentation_contract::AttachmentClearPolicy::Zero => "Zero",
        wrela::presentation_contract::AttachmentClearPolicy::SemanticDefault => "SemanticDefault",
        wrela::presentation_contract::AttachmentClearPolicy::PreservePrevious => "PreservePrevious",
    }
}

fn temporal_reuse_name(reuse: wrela::presentation_contract::TemporalReuseMode) -> &'static str {
    match reuse {
        wrela::presentation_contract::TemporalReuseMode::Disabled => "Disabled",
        wrela::presentation_contract::TemporalReuseMode::ReprojectColor => "ReprojectColor",
        wrela::presentation_contract::TemporalReuseMode::ReprojectColorAndMotion => {
            "ReprojectColorAndMotion"
        }
    }
}

fn contract_observability_names(
    observability: &wrela::presentation_contract::PresentationObservabilityProfile,
) -> Vec<String> {
    let mut names = Vec::new();
    if observability.pass_graph {
        names.push("pass_graph".to_string());
    }
    if observability.materialized_intermediates {
        names.push("materialized_intermediates".to_string());
    }
    if observability.query_dependencies {
        names.push("query_dependencies".to_string());
    }
    if observability.backend_dispatch {
        names.push("backend_dispatch".to_string());
    }
    if observability.future_acceleration_hooks {
        names.push("future_acceleration_hooks".to_string());
    }
    names
}

fn pass_observability_names(
    observability: &wrela::presentation_plan::PresentationObservability,
) -> Vec<String> {
    let mut names = Vec::new();
    if observability.pass_graph {
        names.push("pass_graph".to_string());
    }
    if observability.materialized_intermediates {
        names.push("materialized_intermediates".to_string());
    }
    if observability.query_dependencies {
        names.push("query_dependencies".to_string());
    }
    if observability.backend_dispatch {
        names.push("backend_dispatch".to_string());
    }
    if observability.future_acceleration_hooks {
        names.push("future_acceleration_hooks".to_string());
    }
    names
}

fn presentation_lighting_input_dump(
    contract: &wrela::presentation_contract::LightingInputContract,
) -> PresentationLightingInputDump {
    PresentationLightingInputDump {
        binding: contract.binding.to_string(),
        element_schema: attachment_element_schema_name(&contract.element_schema),
        source: lighting_input_source_name(contract.source).to_string(),
        temporary_compatibility_alias: contract.temporary_compatibility_alias,
    }
}

fn format_lighting_input_dump(contract: &PresentationLightingInputDump) -> String {
    format!(
        "{}:{}:{}:compat_alias={}",
        contract.binding,
        contract.element_schema,
        contract.source,
        contract.temporary_compatibility_alias
    )
}

fn lighting_input_source_name(
    source: wrela::presentation_contract::LightingInputBindingSource,
) -> &'static str {
    match source {
        wrela::presentation_contract::LightingInputBindingSource::AuthoredMetadata => {
            "AuthoredMetadata"
        }
        wrela::presentation_contract::LightingInputBindingSource::DefaultCompatibilityRecipe => {
            "DefaultCompatibilityRecipe"
        }
    }
}

fn presentation_pass_kind_name(kind: &wrela::presentation_plan::PresentationPassKind) -> String {
    match kind {
        wrela::presentation_plan::PresentationPassKind::LegacyPpmExport => {
            "LegacyPpmExport".to_string()
        }
        wrela::presentation_plan::PresentationPassKind::GenerateScreenSamples { .. } => {
            "GenerateScreenSamples".to_string()
        }
        wrela::presentation_plan::PresentationPassKind::PrimaryVisibility { contract } => {
            format!("PrimaryVisibility({})", contract.query_contract.as_str())
        }
        wrela::presentation_plan::PresentationPassKind::SurfaceResolve { contract } => {
            format!("SurfaceResolve({})", contract.query_contract.as_str())
        }
        wrela::presentation_plan::PresentationPassKind::ParticipantsResolve { contract } => {
            format!(
                "ParticipantsResolve(radiance={},medium={})",
                contract
                    .radiance_query_contract
                    .map(|contract| contract.as_str().to_string())
                    .unwrap_or_else(|| "disabled".to_string()),
                contract
                    .medium_query_contract
                    .map(|contract| contract.as_str().to_string())
                    .unwrap_or_else(|| "disabled".to_string())
            )
        }
        wrela::presentation_plan::PresentationPassKind::ShadePrimary { contract } => {
            format!("ShadePrimary({})", contract.output_attachment)
        }
        wrela::presentation_plan::PresentationPassKind::CompositeColor { contract } => {
            format!(
                "CompositeColor({}->{})",
                contract.input_attachment, contract.output_attachment
            )
        }
        wrela::presentation_plan::PresentationPassKind::MotionResolve { contract } => {
            format!(
                "MotionResolve({}->{})",
                contract.primary_hit_attachment, contract.output_attachment
            )
        }
        wrela::presentation_plan::PresentationPassKind::TemporalResolve { contract } => {
            format!(
                "TemporalResolve({}->{})",
                contract.input_attachment, contract.output_attachment
            )
        }
        wrela::presentation_plan::PresentationPassKind::WorldBatchQuery { contract_id } => {
            format!("WorldBatchQuery({})", contract_id.as_str())
        }
        wrela::presentation_plan::PresentationPassKind::KernelDispatch => {
            "KernelDispatch".to_string()
        }
        wrela::presentation_plan::PresentationPassKind::ExportAttachment { attachment } => {
            format!("ExportAttachment({attachment})")
        }
    }
}

fn presentation_screen_sample_pass_dump(
    kind: &wrela::presentation_plan::PresentationPassKind,
) -> Option<PresentationScreenSamplePassDump> {
    match kind {
        wrela::presentation_plan::PresentationPassKind::GenerateScreenSamples { contract } => {
            Some(PresentationScreenSamplePassDump {
                viewport_width_source: contract.viewport_width_source.to_string(),
                viewport_height_source: contract.viewport_height_source.to_string(),
                samples_per_pixel: contract.samples_per_pixel,
                jitter_source: contract.jitter_source.to_string(),
                item_count_expression: contract.item_count_expression.to_string(),
                output_item_record: contract.output_item_record.to_string(),
            })
        }
        _ => None,
    }
}

fn presentation_recipe_name(
    recipe: wrela::presentation_binding::PresentationPassRecipeKind,
) -> &'static str {
    match recipe {
        wrela::presentation_binding::PresentationPassRecipeKind::LegacyPpmExport => {
            "LegacyPpmExport"
        }
        wrela::presentation_binding::PresentationPassRecipeKind::GenerateScreenSamples => {
            "GenerateScreenSamples"
        }
        wrela::presentation_binding::PresentationPassRecipeKind::PrimaryVisibility => {
            "PrimaryVisibility"
        }
        wrela::presentation_binding::PresentationPassRecipeKind::SurfaceResolve => "SurfaceResolve",
        wrela::presentation_binding::PresentationPassRecipeKind::ParticipantsResolve => {
            "ParticipantsResolve"
        }
        wrela::presentation_binding::PresentationPassRecipeKind::ShadePrimary => "ShadePrimary",
        wrela::presentation_binding::PresentationPassRecipeKind::CompositeColor => "CompositeColor",
        wrela::presentation_binding::PresentationPassRecipeKind::MotionResolve => "MotionResolve",
        wrela::presentation_binding::PresentationPassRecipeKind::TemporalResolve => {
            "TemporalResolve"
        }
        wrela::presentation_binding::PresentationPassRecipeKind::WorldBatchQuery => {
            "WorldBatchQuery"
        }
        wrela::presentation_binding::PresentationPassRecipeKind::KernelDispatch => "KernelDispatch",
        wrela::presentation_binding::PresentationPassRecipeKind::ExportAttachment => {
            "ExportAttachment"
        }
    }
}

fn presentation_binding_execution_name(
    binding: &wrela::presentation_binding::PresentationBindingSummary,
) -> &'static str {
    match binding.recipe {
        wrela::presentation_binding::PresentationPassRecipeKind::LegacyPpmExport => {
            "legacy_ppm_export"
        }
        wrela::presentation_binding::PresentationPassRecipeKind::GenerateScreenSamples => {
            "screen_sample_generation"
        }
        wrela::presentation_binding::PresentationPassRecipeKind::PrimaryVisibility => {
            "primary_visibility"
        }
        wrela::presentation_binding::PresentationPassRecipeKind::SurfaceResolve => {
            "surface_resolve"
        }
        wrela::presentation_binding::PresentationPassRecipeKind::ParticipantsResolve => {
            "participants_resolve"
        }
        wrela::presentation_binding::PresentationPassRecipeKind::ShadePrimary => "shade_primary",
        wrela::presentation_binding::PresentationPassRecipeKind::CompositeColor => {
            "composite_color"
        }
        wrela::presentation_binding::PresentationPassRecipeKind::MotionResolve => {
            "motion_resolve"
        }
        wrela::presentation_binding::PresentationPassRecipeKind::TemporalResolve => {
            "temporal_resolve"
        }
        wrela::presentation_binding::PresentationPassRecipeKind::WorldBatchQuery => {
            "world_batch_query"
        }
        wrela::presentation_binding::PresentationPassRecipeKind::KernelDispatch => {
            "kernel_dispatch"
        }
        wrela::presentation_binding::PresentationPassRecipeKind::ExportAttachment => {
            "attachment_export"
        }
    }
}

fn dispatch_backend_name(backend: wrela::query_plan::DispatchBackend) -> &'static str {
    match backend {
        wrela::query_plan::DispatchBackend::Cpu => "cpu",
        wrela::query_plan::DispatchBackend::VirtualGpu => "virtual_gpu",
        wrela::query_plan::DispatchBackend::Wgsl => "wgsl",
        wrela::query_plan::DispatchBackend::Auto => "auto",
    }
}

fn acceleration_hook_name(
    hook: wrela::presentation_plan::PresentationAccelerationHook,
) -> &'static str {
    match hook {
        wrela::presentation_plan::PresentationAccelerationHook::ScreenLattice => "ScreenLattice",
        wrela::presentation_plan::PresentationAccelerationHook::WorldBatch => "WorldBatch",
        wrela::presentation_plan::PresentationAccelerationHook::SemanticSupport => {
            "SemanticSupport"
        }
        wrela::presentation_plan::PresentationAccelerationHook::TemporalHistory => {
            "TemporalHistory"
        }
    }
}

fn presentation_query_dependency_dump(
    contract_id: wrela::query_plan::QueryContractId,
) -> PresentationQueryDependencyDump {
    let descriptor = wrela::query_contract::query_contract(contract_id);
    let solver_diagnostics = wrela::query_solver::RaySolverPlan::for_contract(contract_id, None)
        .map(|solver| {
            let summary = solver.diagnostic_summary();
            PresentationRaySolverDump {
                plan_id: summary.plan_id.to_string(),
                methods: summary
                    .methods
                    .iter()
                    .map(|method| wrela::query_solver::ray_solver_method_name(*method).to_string())
                    .collect(),
                fallback: wrela::query_solver::ray_solver_fallback_name(summary.fallback)
                    .to_string(),
                unavailable_facts: summary
                    .unavailable_facts
                    .iter()
                    .map(|fact| fact.to_string())
                    .collect(),
            }
        });
    PresentationQueryDependencyDump {
        contract_id: contract_id.as_str().to_string(),
        family: descriptor.map(|descriptor| {
            wrela::query_contract::query_family_name(descriptor.family).to_string()
        }),
        question: descriptor.map(|descriptor| {
            wrela::query_contract::query_question_name(descriptor.question).to_string()
        }),
        surface: descriptor.map(|descriptor| {
            wrela::query_contract::query_surface_name(descriptor.surface).to_string()
        }),
        target: descriptor.map(|descriptor| {
            wrela::query_contract::query_target_name(descriptor.target).to_string()
        }),
        cardinality: descriptor.map(|descriptor| {
            wrela::query_contract::query_cardinality_name(descriptor.cardinality).to_string()
        }),
        call: descriptor.map(|descriptor| {
            format!(
                "{}.{}",
                wrela::query_contract::query_family_name(descriptor.family),
                wrela::query_contract::query_family_member_name(descriptor)
            )
        }),
        solver_diagnostics,
    }
}

pub fn execute(spec: CommandSpec) {
    let trace = spec.trace_enabled;
    if trace {
        eprintln!("build: cli start");
    }
    let parsed = match spec.parsed {
        ParsedCommandSpec::Help => {
            diag_emit::print_help();
            return;
        }
        ParsedCommandSpec::Version => {
            println!("wrela {}", env!("CARGO_PKG_VERSION"));
            return;
        }
        ParsedCommandSpec::Error(err) => {
            if err == "__print_help__" {
                diag_emit::print_help();
            } else {
                eprintln!("{err}");
            }
            std::process::exit(EXIT_USAGE);
        }
        ParsedCommandSpec::Ready(parsed) => parsed,
    };
    let output_format = if parsed.output_format_sarif {
        OutputFormat::Sarif
    } else if parsed.output_format_json {
        OutputFormat::Json
    } else if parsed.output_format_human {
        OutputFormat::Pretty
    } else {
        OutputFormat::Pretty
    };
    let emit_mir = parsed.emit_mir;
    let emit_mir_opt = parsed.emit_mir_opt;
    let emit_obj = parsed.emit_obj;
    let emit_bin = parsed.emit_bin;
    let out_path = parsed.out_path;
    let prefix_path = parsed.prefix_path;
    let query_backend = parsed
        .query_backend
        .unwrap_or(wrela::query_plan::DispatchBackend::Auto);
    let command = parsed.command;
    let integration_mode = parsed.integration_mode;
    let path_arg = parsed.path_arg;
    let program_args = parsed.program_args;
    let poll_ms = parsed.poll_ms;
    let test_jobs = parsed.test_jobs;
    let test_timeout_ms = parsed.test_timeout_ms;
    let test_record = parsed.test_record;
    let test_update_public_surface = parsed.test_update_public_surface;
    let test_list = parsed.test_list;
    let test_id = parsed.test_id;
    let test_filter = parsed.test_filter;
    let test_lane = parsed.test_lane;
    let test_seed = parsed.test_seed;
    let repro_artifact_path = parsed.repro_artifact_path;
    let replay_trace_path = parsed.replay_trace_path;
    let perf_debug = parsed.perf_debug;
    let perf_runs = parsed.perf_runs;
    let perf_baseline_out = parsed.perf_baseline_out;
    let perf_gate_path = parsed.perf_gate_path;
    let perf_max_regression_pct = parsed.perf_max_regression_pct;
    let perf_cv_max_pct = parsed.perf_cv_max_pct;
    let kpi_check_fallback_max = parsed.kpi_check_fallback_max;
    let kpi_check_batch_min = parsed.kpi_check_batch_min;
    let kpi_scheduler_p99_improve_min_pct = parsed.kpi_scheduler_p99_improve_min_pct;
    let kpi_rewrite_overhead_max_pct = parsed.kpi_rewrite_overhead_max_pct;
    let kpi_actor_throughput_improve_min_pct = parsed.kpi_actor_throughput_improve_min_pct;
    let kpi_queue_age_p99_max_regress_pct = parsed.kpi_queue_age_p99_max_regress_pct;
    let kpi_starvation_violations_max = parsed.kpi_starvation_violations_max;
    let kpi_scheduler_throughput_improve_min_pct = parsed.kpi_scheduler_throughput_improve_min_pct;
    let kpi_scheduler_loop_p99_max_regress_pct = parsed.kpi_scheduler_loop_p99_max_regress_pct;
    let kpi_scheduler_local_hit_min = parsed.kpi_scheduler_local_hit_min;
    let benchmark_manifest_path = parsed.benchmark_manifest_path;
    let perf_profile_name = parsed.perf_profile_name;
    let perfcmp_baseline_ref = parsed.perfcmp_baseline_ref;
    let perfcmp_candidate_ref = parsed.perfcmp_candidate_ref;
    let perfcmp_warmup_pairs = parsed.perfcmp_warmup_pairs;
    let perfcmp_measure_pairs = parsed.perfcmp_measure_pairs;
    let perfcmp_min_effect_pct = parsed.perfcmp_min_effect_pct;
    let perfcmp_confidence_pct = parsed.perfcmp_confidence_pct;
    let analysis_holes_only = parsed.analysis_holes_only;
    let strict_naming = parsed.strict_naming;
    let fix_allow_review_fixes = parsed.fix_allow_review_fixes;
    let workspace_diagnostics = parsed.workspace_diagnostics;
    let _orchestration_identity = parsed.orchestration_identity;

    let command = command.as_str();
    let kpi_thresholds = KpiThresholds {
        check_fallback_max: kpi_check_fallback_max,
        check_batch_min: kpi_check_batch_min,
        scheduler_p99_improve_min_pct: kpi_scheduler_p99_improve_min_pct,
        rewrite_overhead_max_pct: kpi_rewrite_overhead_max_pct,
        actor_throughput_improve_min_pct: kpi_actor_throughput_improve_min_pct,
        queue_age_p99_max_regress_pct: kpi_queue_age_p99_max_regress_pct,
        starvation_violations_max: kpi_starvation_violations_max,
        scheduler_throughput_improve_min_pct: kpi_scheduler_throughput_improve_min_pct,
        scheduler_loop_p99_max_regress_pct: kpi_scheduler_loop_p99_max_regress_pct,
        scheduler_local_hit_min: kpi_scheduler_local_hit_min,
    };
    if command != "test" && (test_record || test_update_public_surface) {
        eprintln!("error: --record and --update-public-surface are only valid with `wrela test`");
        std::process::exit(EXIT_USAGE);
    }
    if command != "run" && command != "build" && command != "compile" && integration_mode {
        eprintln!("error: --integration-mode is only valid with `wrela run` or `wrela build`");
        std::process::exit(EXIT_USAGE);
    }
    if command != "test" && test_list {
        eprintln!("error: --list is only valid with `wrela test`");
        std::process::exit(EXIT_USAGE);
    }
    if command != "test" && command != "perf" && (test_id.is_some() || test_filter.is_some()) {
        eprintln!("error: --id and --filter are only valid with `wrela test` or `wrela perf`");
        std::process::exit(EXIT_USAGE);
    }
    if command != "test" && command != "perf" && test_lane.is_some() {
        eprintln!("error: --lane is only valid with `wrela test` or `wrela perf`");
        std::process::exit(EXIT_USAGE);
    }
    if command != "test" && command != "perf" && test_seed.is_some() {
        eprintln!("error: --seed is only valid with `wrela test` or `wrela perf`");
        std::process::exit(EXIT_USAGE);
    }
    if command != "test" && repro_artifact_path.is_some() {
        eprintln!("error: --repro is only valid with `wrela test`");
        std::process::exit(EXIT_USAGE);
    }
    if command != "test" && replay_trace_path.is_some() {
        eprintln!("error: --replay-trace is only valid with `wrela test`");
        std::process::exit(EXIT_USAGE);
    }
    if command != "perf" && command != "perfcmp" && benchmark_manifest_path.is_some() {
        eprintln!("error: --benchmark-manifest is only valid with `wrela perf` or `wrela perfcmp`");
        std::process::exit(EXIT_USAGE);
    }
    if command != "perf" && command != "perfcmp" && perf_profile_name.is_some() {
        eprintln!("error: --profile is only valid with `wrela perf` or `wrela perfcmp`");
        std::process::exit(EXIT_USAGE);
    }
    if command != "perfcmp"
        && (perfcmp_baseline_ref.is_some()
            || perfcmp_candidate_ref.is_some()
            || perfcmp_warmup_pairs.is_some()
            || perfcmp_measure_pairs.is_some()
            || perfcmp_min_effect_pct.is_some()
            || perfcmp_confidence_pct.is_some())
    {
        eprintln!(
            "error: --baseline-ref, --candidate-ref, --warmup-pairs, --measure-pairs, --min-effect-pct, and --confidence are only valid with `wrela perfcmp`"
        );
        std::process::exit(EXIT_USAGE);
    }
    if command != "check" && command != "analyze" && analysis_holes_only {
        eprintln!("error: --holes-only is only valid with `wrela check` or `wrela analyze`");
        std::process::exit(EXIT_USAGE);
    }
    if strict_naming
        && command != "check"
        && command != "analyze"
        && command != "build"
        && command != "compile"
        && command != "run"
        && command != "dev"
    {
        eprintln!(
            "error: --strict-naming is only valid with `wrela check`, `wrela analyze`, `wrela build`, `wrela compile`, `wrela run`, or `wrela dev`"
        );
        std::process::exit(EXIT_USAGE);
    }
    if command != "fix" && command != "fmt" && fix_allow_review_fixes {
        eprintln!("error: --allow-review-fixes is only valid with `wrela fix` or `wrela fmt`");
        std::process::exit(EXIT_USAGE);
    }
    if command != "fix" && command != "fmt" && workspace_diagnostics {
        eprintln!("error: --workspace-diagnostics is only valid with `wrela fix` or `wrela fmt`");
        std::process::exit(EXIT_USAGE);
    }
    let parsed_test_lane = if let Some(raw_lane) = test_lane.as_deref() {
        match parse_test_lane_filter(raw_lane) {
            Some(lane) => Some(lane),
            None => {
                eprintln!(
                    "error: invalid --lane value `{raw_lane}` (expected one of spec|integration|sim|model|default)"
                );
                std::process::exit(EXIT_USAGE);
            }
        }
    } else {
        None
    };
    let test_selection = TestSelection {
        list: test_list,
        id: test_id,
        filter: test_filter,
        lane: parsed_test_lane,
        include_ids: None,
        cert_selection_report: None,
    };
    let perf_profile = match PerfProfile::parse(perf_profile_name.as_deref().unwrap_or("standard"))
    {
        Some(profile) => profile,
        None => {
            eprintln!("error: invalid --profile value (expected smoke|standard|deep)");
            std::process::exit(EXIT_USAGE);
        }
    };

    match command {
        "init" => {
            if trace {
                eprintln!("build: command init");
            }
            let target = path_arg.as_deref().unwrap_or(".");
            if let Err(err) = init_project(target) {
                eprintln!("init error: {err}");
                std::process::exit(EXIT_USAGE);
            }
        }
        "update" => {
            if trace {
                eprintln!("build: command update");
            }
            if path_arg.is_some() {
                eprintln!("error: update does not take a path");
                std::process::exit(EXIT_USAGE);
            }
            if !program_args.is_empty() {
                eprintln!("error: unexpected extra arguments");
                std::process::exit(EXIT_USAGE);
            }
            if let Err(err) = update_toolchain(prefix_path.as_deref()) {
                eprintln!("update error: {err}");
                std::process::exit(EXIT_CODEGEN);
            }
        }
        "query-contracts" => {
            if trace {
                eprintln!("build: command query-contracts");
            }
            execute_query_contracts_command(output_format, path_arg, program_args);
        }
        "preview" => {
            if trace {
                eprintln!("build: command preview");
            }
            execute_preview_command(output_format, path_arg, program_args, query_backend);
        }
        "frame" => {
            if trace {
                eprintln!("build: command frame");
            }
            execute_frame_command(output_format, path_arg, program_args, query_backend);
        }
        "frame-contracts" => {
            if trace {
                eprintln!("build: command frame-contracts");
            }
            execute_frame_contracts_command(output_format, path_arg, program_args, query_backend);
        }
        "presentation-plan" => {
            if trace {
                eprintln!("build: command presentation-plan");
            }
            execute_presentation_plan_command(output_format, path_arg, program_args, query_backend);
        }
        "presentation-debug" => {
            if trace {
                eprintln!("build: command presentation-debug");
            }
            execute_presentation_debug_command(
                output_format,
                path_arg,
                program_args,
                query_backend,
            );
        }
        "check" | "analyze" => {
            if trace {
                eprintln!("build: command check");
            }
            if !program_args.is_empty() {
                eprintln!("error: unexpected extra arguments");
                std::process::exit(EXIT_USAGE);
            }
            let entry_path = resolve_entry_path(path_arg.as_deref());
            let entry_path = match entry_path {
                Ok(path) => path,
                Err(err) => {
                    eprintln!("error: {err}");
                    std::process::exit(EXIT_USAGE);
                }
            };
            let result = compile_to_mir(
                &entry_path,
                output_format,
                emit_mir,
                emit_mir_opt,
                false,
                true,
                strict_naming,
                analysis_holes_only,
                query_backend,
            );
            if let Err(code) = result {
                std::process::exit(code);
            }
        }
        "fix" => {
            if trace {
                eprintln!("build: command fix");
            }
            if !program_args.is_empty() {
                eprintln!("error: unexpected extra arguments");
                std::process::exit(EXIT_USAGE);
            }
            let entry_path = resolve_entry_path(path_arg.as_deref());
            let entry_path = match entry_path {
                Ok(path) => path,
                Err(err) => {
                    eprintln!("error: {err}");
                    std::process::exit(EXIT_USAGE);
                }
            };
            const MAX_PASSES: usize = 12;
            let mut attempted = 0usize;
            let mut applied = 0usize;
            let mut touched_paths: BTreeSet<String> = BTreeSet::new();
            let mut any_fix_candidates = false;
            let mut had_apply_error = false;
            let diagnostic_scope =
                DiagnosticScope::from_entrypoint(&entry_path, workspace_diagnostics);

            for _ in 0..MAX_PASSES {
                let fixes = match collect_safe_fixes(
                    &entry_path,
                    output_format,
                    fix_allow_review_fixes,
                    &diagnostic_scope,
                ) {
                    Ok(fixes) => fixes,
                    Err(code) => {
                        if applied > 0 {
                            break;
                        }
                        std::process::exit(code);
                    }
                };
                if fixes.is_empty() {
                    break;
                }
                any_fix_candidates = true;
                attempted = attempted.saturating_add(fixes.len());
                for fix in &fixes {
                    touched_paths.insert(fix.span.path.clone());
                }
                match apply_source_fixes(&fixes) {
                    Ok(report) => {
                        applied = applied.saturating_add(report.applied);
                        if report.applied == 0 {
                            break;
                        }
                    }
                    Err(err) => {
                        applied = applied.saturating_add(err.applied);
                        had_apply_error = true;
                        eprintln!("fix apply error: {}", err.message);
                        break;
                    }
                }
            }

            let summary = FixSummary {
                attempted,
                applied,
                skipped: attempted.saturating_sub(applied),
                errors: if had_apply_error { 1 } else { 0 },
                touched_files: touched_paths.len(),
            };
            emit_fix_summary(output_format, summary);

            if had_apply_error {
                std::process::exit(EXIT_CODEGEN);
            }
            if !any_fix_candidates || applied == 0 {
                eprintln!("fix: no safe non-overlapping fixes found");
                std::process::exit(EXIT_TYPE);
            }
            eprintln!("fix: applied {} safe fix(es)", applied);
        }
        "fmt" => {
            if trace {
                eprintln!("build: command fmt");
            }
            if !program_args.is_empty() {
                eprintln!("error: unexpected extra arguments");
                std::process::exit(EXIT_USAGE);
            }
            let format_targets = match resolve_format_targets(path_arg.as_deref()) {
                Ok(targets) => targets,
                Err(err) => {
                    eprintln!("error: {err}");
                    std::process::exit(EXIT_USAGE);
                }
            };
            let mut summary = FmtSummary::default();
            summary.targets_scanned = format_targets.len();
            let mut fmt_exit_code: Option<i32> = None;
            for target in &format_targets {
                match run_format_loop(
                    target,
                    output_format,
                    fix_allow_review_fixes,
                    workspace_diagnostics,
                ) {
                    Ok(target_summary) => {
                        summary.iterations =
                            summary.iterations.saturating_add(target_summary.iterations);
                        summary.attempted =
                            summary.attempted.saturating_add(target_summary.attempted);
                        summary.applied = summary.applied.saturating_add(target_summary.applied);
                        summary.touched_files = summary
                            .touched_files
                            .saturating_add(target_summary.touched_files);
                    }
                    Err(code) => {
                        summary.failed_targets = summary.failed_targets.saturating_add(1);
                        if fmt_exit_code.is_none() {
                            fmt_exit_code = Some(code);
                        }
                    }
                }
            }
            emit_fmt_summary(output_format, summary);
            if summary.failed_targets > 0 {
                eprintln!(
                    "fmt: {} target(s) failed during sweep",
                    summary.failed_targets
                );
            } else if summary.applied == 0 {
                eprintln!("fmt: already canonical");
            } else {
                eprintln!(
                    "fmt: applied {} rewrite(s) across {} file(s) in {} pass(es)",
                    summary.applied, summary.touched_files, summary.iterations
                );
            }
            if let Some(code) = fmt_exit_code {
                std::process::exit(code);
            }
        }
        "build" | "compile" => {
            if trace {
                eprintln!("build: command build");
            }
            let build_start = Instant::now();
            if !program_args.is_empty() {
                eprintln!("error: unexpected extra arguments");
                std::process::exit(EXIT_USAGE);
            }
            let entry_path = resolve_entry_path(path_arg.as_deref());
            let entry_path = match entry_path {
                Ok(path) => path,
                Err(err) => {
                    eprintln!("error: {err}");
                    std::process::exit(EXIT_USAGE);
                }
            };
            if trace {
                eprintln!("build: resolved entry {}", entry_path.display());
            }
            if find_src_root(&entry_path).is_none() {
                eprintln!(
                    "error: `wrela build` requires project layout (`src/**`) because single-file mode bypasses architecture checks"
                );
                eprintln!(
                    "help: move entrypoint to `src/main.wr` and run `wrela build <project-or-src/main.wr>`"
                );
                std::process::exit(EXIT_USAGE);
            }
            if trace {
                eprintln!("build: source root verified");
            }
            let workspace_root = project_root_for_entry(&entry_path);
            if trace {
                eprintln!("build: workspace root {}", workspace_root.display());
            }
            let budget_policy = resolve_budget_policy_v1(test_jobs, test_timeout_ms);
            let jobs = budget_policy.test_jobs.value as usize;
            let timeout = Duration::from_millis(budget_policy.test_timeout_ms.value);
            if trace {
                eprintln!(
                    "build: budget resolved jobs={} timeout_ms={}",
                    jobs,
                    timeout.as_millis()
                );
                eprintln!("build: collecting coverage id aliases");
            }
            if integration_mode {
                eprintln!(
                    "warning: --integration-mode on build bypasses strict naming checks and certification gates for integration-facing executables"
                );
                let mir_compile_start = Instant::now();
                let mir_module = match compile_to_mir(
                    &entry_path,
                    output_format,
                    emit_mir,
                    emit_mir_opt,
                    true,
                    false,
                    false,
                    false,
                    query_backend,
                ) {
                    Ok(mir) => mir,
                    Err(code) => std::process::exit(code),
                };
                let mir_compile_ms = mir_compile_start.elapsed().as_millis();
                if let Some(path) = emit_obj {
                    match wrela::backend::cranelift::compile_to_object(&mir_module) {
                        Ok(obj) => {
                            if let Err(err) = fs::write(&path, obj) {
                                eprintln!("failed to write object: {err}");
                                std::process::exit(EXIT_CODEGEN);
                            }
                        }
                        Err(err) => {
                            eprintln!("codegen error: {}", err.0);
                            std::process::exit(EXIT_CODEGEN);
                        }
                    }
                }
                let output_path = out_path
                    .or(emit_bin)
                    .map(PathBuf::from)
                    .unwrap_or_else(|| workspace_root.join("wrela.out"));
                let output = output_path.to_string_lossy().to_string();
                let codegen_start = Instant::now();
                if let Err(err) =
                    wrela::backend::cranelift::compile_to_executable(&mir_module, output.as_ref())
                {
                    eprintln!("codegen error: {}", err.0);
                    std::process::exit(EXIT_CODEGEN);
                }
                let codegen_ms = codegen_start.elapsed().as_millis();
                emit_build_perf_event(
                    output_format,
                    true,
                    "integration-mode-skip-cert".to_string(),
                    "integration-mode-skip-cert".to_string(),
                    BuildPerfTimings {
                        certification_ms: 0,
                        cert_collect_tests_ms: 0,
                        cert_compile_harness_ms: 0,
                        cert_determinism_ms: 0,
                        cert_mutation_discovery_ms: 0,
                        cert_mutation_execution_ms: 0,
                        cert_diff_ms: 0,
                        mir_compile_ms,
                        codegen_ms,
                        cert_report_ms: 0,
                        total_ms: build_start.elapsed().as_millis(),
                    },
                );
                return;
            }
            let toolchain_version = resolve_toolchain_version();
            if trace {
                eprintln!("build: toolchain version {}", toolchain_version);
                eprintln!("build: hashing source fingerprint");
            }
            let source_hash = match hash_source_fingerprint(&workspace_root) {
                Ok(hash) => hash,
                Err(err) => {
                    eprintln!("certification cache error: {err}");
                    std::process::exit(EXIT_CODEGEN);
                }
            };
            if trace {
                eprintln!("build: source fingerprint hash={source_hash}");
            }
            let cert_cache_hash = certification_cache_hash(&source_hash, &toolchain_version);
            let cert_cache_dir = workspace_root
                .join("target")
                .join("wrela_cert")
                .join(&cert_cache_hash);
            let cert_report_path = cert_cache_dir.join("cert.json");
            let function_coverage_path = cert_cache_dir.join("function_coverage.json");
            let mut cert_cache_hit = cert_report_path.is_file() && function_coverage_path.is_file();
            let mut cert_cache_reason = if cert_cache_hit {
                "unchanged-certified-inputs".to_string()
            } else {
                "cache-miss-or-first-run".to_string()
            };
            let certification_start = Instant::now();
            let mut differential_results_hash: Option<String> = None;
            let mut mutation_summary_hash: Option<String> = None;
            let mut cert_timings = CertPerfTimings::default();
            let mut cached_coverage_snapshot = None;
            if cert_cache_hit {
                emit_certification_cache_hit(output_format, &cert_cache_hash, &cert_cache_dir);
                match load_function_coverage_snapshot(&function_coverage_path) {
                    Ok(snapshot) => cached_coverage_snapshot = Some(snapshot),
                    Err(err) => {
                        cert_cache_hit = false;
                        cert_cache_reason = "cache-schema-stale-recomputed".to_string();
                        eprintln!(
                            "certification cache stale; recomputing certification artifacts: {err}"
                        );
                    }
                }
            }
            let function_coverage = if let Some(snapshot) = cached_coverage_snapshot {
                snapshot
            } else {
                let cert_selection =
                    resolve_certification_test_selection(&workspace_root, output_format);
                let cert_result = cert_engine::run_tests(
                    &TestTarget::ProjectRoot(workspace_root.clone()),
                    &budget_policy,
                    jobs,
                    timeout,
                    output_format,
                    perf_debug,
                    None,
                    &cert_selection,
                    true,
                    HttpCassetteMode::Replay,
                    None,
                    query_backend,
                );
                if cert_result.exit != EXIT_OK {
                    eprintln!("build blocked: certification failed; no artifact emitted");
                    std::process::exit(cert_result.exit);
                }
                differential_results_hash = cert_result.differential_results_hash.clone();
                mutation_summary_hash = cert_result.mutation_summary_hash.clone();
                cert_timings = cert_result.cert_timings;
                let raw_snapshot = cert_result
                    .summary
                    .as_ref()
                    .map(|summary| summary.metrics.function_coverage.clone())
                    .unwrap_or_default();
                let snapshot = canonicalize_function_coverage(&raw_snapshot);
                if let Err(err) =
                    write_function_coverage_snapshot(&function_coverage_path, &snapshot)
                {
                    eprintln!("certification cache error: {err}");
                    std::process::exit(EXIT_CODEGEN);
                }
                let coverage_index_path =
                    certification_coverage_index_path(&workspace_root, &cert_cache_hash);
                let coverage_index =
                    build_function_test_coverage_index(cert_result.summary.as_ref());
                if let Err(err) =
                    write_function_test_coverage_index(&coverage_index_path, &coverage_index)
                {
                    eprintln!("certification cache error: {err}");
                    std::process::exit(EXIT_CODEGEN);
                }
                snapshot
            };
            let certification_ms = certification_start.elapsed().as_millis();
            if let Err(err) = enforce_importable_coverage_gate(&workspace_root, &function_coverage)
            {
                eprintln!("{err}");
                std::process::exit(EXIT_CODEGEN);
            }
            if let Err(err) = enforce_public_surface_gate(&workspace_root) {
                eprintln!("{err}");
                std::process::exit(EXIT_CODEGEN);
            }
            if integration_mode {
                eprintln!(
                    "warning: --integration-mode on build bypasses strict naming checks for integration-facing executables"
                );
            }
            let mir_compile_start = Instant::now();
            let mir_module = match compile_to_mir(
                &entry_path,
                output_format,
                emit_mir,
                emit_mir_opt,
                true,
                !integration_mode,
                strict_naming,
                false,
                query_backend,
            ) {
                Ok(mir) => mir,
                Err(code) => std::process::exit(code),
            };
            let mir_compile_ms = mir_compile_start.elapsed().as_millis();
            if let Some(path) = emit_obj {
                match wrela::backend::cranelift::compile_to_object(&mir_module) {
                    Ok(obj) => {
                        if let Err(err) = fs::write(&path, obj) {
                            eprintln!("failed to write object: {err}");
                            std::process::exit(EXIT_CODEGEN);
                        }
                    }
                    Err(err) => {
                        eprintln!("codegen error: {}", err.0);
                        std::process::exit(EXIT_CODEGEN);
                    }
                }
            }
            let output_path = out_path
                .or(emit_bin)
                .map(PathBuf::from)
                .unwrap_or_else(|| workspace_root.join("wrela.out"));
            let output = output_path.to_string_lossy().to_string();
            let codegen_start = Instant::now();
            if let Err(err) =
                wrela::backend::cranelift::compile_to_executable(&mir_module, output.as_ref())
            {
                eprintln!("codegen error: {}", err.0);
                std::process::exit(EXIT_CODEGEN);
            }
            let codegen_ms = codegen_start.elapsed().as_millis();
            let artifact_path = output_path;
            let cert_report_start = Instant::now();
            if let Err(err) = write_certification_report(
                &entry_path,
                &workspace_root,
                &artifact_path,
                &budget_policy,
                &toolchain_version,
                &source_hash,
                &cert_cache_hash,
                differential_results_hash.as_deref(),
                mutation_summary_hash.as_deref(),
            ) {
                eprintln!("certification report error: {err}");
                std::process::exit(EXIT_CODEGEN);
            }
            let cert_report_ms = cert_report_start.elapsed().as_millis();
            let total_ms = build_start.elapsed().as_millis();
            emit_build_perf_event(
                output_format,
                cert_cache_hit,
                cert_cache_hash,
                cert_cache_reason,
                BuildPerfTimings {
                    certification_ms,
                    cert_collect_tests_ms: cert_timings.collect_tests_ms,
                    cert_compile_harness_ms: cert_timings.compile_harness_ms,
                    cert_determinism_ms: cert_timings.determinism_ms,
                    cert_mutation_discovery_ms: cert_timings.mutation_discovery_ms,
                    cert_mutation_execution_ms: cert_timings.mutation_execution_ms,
                    cert_diff_ms: cert_timings.differential_ms,
                    mir_compile_ms,
                    codegen_ms,
                    cert_report_ms,
                    total_ms,
                },
            );
        }
        "verify-cert" => {
            if !program_args.is_empty() {
                eprintln!("error: unexpected extra arguments");
                std::process::exit(EXIT_USAGE);
            }
            let cert_path = match path_arg {
                Some(path) => PathBuf::from(path),
                None => {
                    eprintln!("error: missing cert path");
                    std::process::exit(EXIT_USAGE);
                }
            };
            if let Err(err) = verify_certification_report(&cert_path) {
                eprintln!("{err}");
                std::process::exit(EXIT_CODEGEN);
            }
            println!("cert verified: {}", cert_path.display());
        }
        "run" => {
            if trace {
                eprintln!("build: command run");
            }
            let entry_path = resolve_entry_path(path_arg.as_deref());
            let entry_path = match entry_path {
                Ok(path) => path,
                Err(err) => {
                    eprintln!("error: {err}");
                    std::process::exit(EXIT_USAGE);
                }
            };
            let mir_module = if integration_mode {
                if !integration_mode_entry_path_is_allowed(&entry_path) {
                    eprintln!(
                        "error: --integration-mode requires entrypoint under src/application/composition/** or src/infrastructure/integrations/**"
                    );
                    eprintln!(
                        "help: move entrypoint to src/application/composition/main.wr or src/infrastructure/integrations/<name>.wr"
                    );
                    std::process::exit(EXIT_USAGE);
                }
                eprintln!(
                    "warning: --integration-mode is fixture-scoped; use only for integration executables under approved paths"
                );
                match compile_to_mir(
                    &entry_path,
                    output_format,
                    emit_mir,
                    emit_mir_opt,
                    true,
                    false,
                    false,
                    false,
                    query_backend,
                ) {
                    Ok(mir) => mir,
                    Err(code) => std::process::exit(code),
                }
            } else {
                match compile_to_mir(
                    &entry_path,
                    output_format,
                    emit_mir,
                    emit_mir_opt,
                    true,
                    true,
                    strict_naming,
                    false,
                    query_backend,
                ) {
                    Ok(mir) => mir,
                    Err(code) => std::process::exit(code),
                }
            };
            let generated_temp_output = out_path.is_none();
            let output = out_path.unwrap_or_else(temp_exe_path);
            if let Err(err) =
                wrela::backend::cranelift::compile_to_executable(&mir_module, output.as_ref())
            {
                eprintln!("codegen error: {}", err.0);
                std::process::exit(EXIT_CODEGEN);
            }
            let status = match Command::new(&output).args(&program_args).status() {
                Ok(status) => status,
                Err(err) => {
                    eprintln!("error: failed to run compiled binary {output}: {err}");
                    std::process::exit(EXIT_CODEGEN);
                }
            };
            if generated_temp_output {
                let _ = fs::remove_file(&output);
            }
            std::process::exit(status.code().unwrap_or(EXIT_RUNTIME_SIGNAL));
        }
        "dev" => {
            if trace {
                eprintln!("build: command dev");
            }
            let entry_path = resolve_entry_path(path_arg.as_deref());
            let entry_path = match entry_path {
                Ok(path) => path,
                Err(err) => {
                    eprintln!("error: {err}");
                    std::process::exit(EXIT_USAGE);
                }
            };
            let poll = poll_ms.unwrap_or(500);
            run_dev_loop(
                &entry_path,
                poll,
                output_format,
                emit_mir,
                emit_mir_opt,
                strict_naming,
                query_backend,
                &program_args,
            );
        }
        "test" => {
            let exit = cert_engine::execute_test_command(cert_engine::TestCommandInput {
                trace,
                program_args,
                out_path,
                emit_obj,
                emit_bin,
                path_arg,
                test_jobs,
                test_timeout_ms,
                test_record,
                test_update_public_surface,
                test_selection,
                repro_artifact_path,
                replay_trace_path,
                output_format,
                perf_debug,
                perf_gate_path,
                perf_max_regression_pct,
                kpi_thresholds,
                test_seed,
                query_backend,
            });
            std::process::exit(exit);
        }
        "eval" => {
            let exit = execute_eval_command(EvalCommandInput {
                trace,
                path_arg,
                program_args,
                runs: perf_runs,
                output_format,
            });
            std::process::exit(exit);
        }
        "perf" => {
            let exit = perf_engine::execute_perf_command(perf_engine::PerfCommandInput {
                trace,
                program_args,
                path_arg,
                perf_runs,
                test_jobs,
                test_timeout_ms,
                benchmark_manifest_path,
                perf_profile,
                perf_baseline_out,
                perf_gate_path,
                perf_max_regression_pct,
                perf_cv_max_pct,
                kpi_thresholds,
                output_format,
                perf_debug,
                test_selection,
                query_backend,
            });
            std::process::exit(exit);
        }
        "perfcmp" => {
            let exit = perf_engine::execute_perfcmp_command(perf_engine::PerfcmpCommandInput {
                trace,
                program_args,
                path_arg,
                benchmark_manifest_path,
                perfcmp_baseline_ref,
                perfcmp_candidate_ref,
                out_path,
                output_format,
                perf_profile,
                perfcmp_warmup_pairs,
                perfcmp_measure_pairs,
                perfcmp_min_effect_pct,
                perfcmp_confidence_pct,
                test_timeout_ms,
                perf_debug,
            });
            std::process::exit(exit);
        }
        "matrix" => {
            let exit = perf_engine::execute_matrix_command(perf_engine::MatrixCommandInput {
                trace,
                program_args,
                path_arg,
                perf_runs,
                perf_gate_path,
                perf_max_regression_pct,
                kpi_thresholds,
            });
            std::process::exit(exit);
        }
        _ => {
            diag_emit::print_help();
            std::process::exit(EXIT_USAGE);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wrela::hir::lower as hir_lower;
    use wrela::parser::ast;
    use wrela::parser::ast::AstNode;
    use wrela::parser::parse;

    fn lower_inline_module(source: &str) -> hir::Module {
        let node = parse(source);
        let root = ast::Root::cast(node).expect("root");
        hir_lower::lower(root)
    }

    fn function<'a>(module: &'a hir::Module, name: &str) -> &'a hir::Function {
        module
            .functions
            .iter()
            .find(|(_, func)| func.name == name)
            .map(|(_, func)| func)
            .unwrap_or_else(|| panic!("missing function `{name}`"))
    }

    #[test]
    fn authored_lighting_follows_grouped_view_helpers() {
        let module = lower_inline_module(
            r#"
view sample_view(world: RegionCapture, camera: Camera) {
    viewport = viewport(width = 2, height = 2)
    lighting = key_light(
        light = Light(
            position = camera.position + vec3(0.5, 1.0, 0.5),
            direction = normalize(vec3(-0.4, -0.7, -0.2)),
            intensity = vec3(1.0, 1.0, 1.0),
            range = 8.0
        ),
        fill_direction = normalize(vec3(-0.2, 0.8, 0.4)),
        fill_strength = 0.33,
        ambient_color = vec3(0.08, 0.11, 0.14)
    )
}
"#,
        );
        let view = function(&module, "sample_view");
        let camera = wrela::presentation_contract::CanonicalCameraInput {
            position: [1.0, 2.0, 3.0],
            forward: [0.0, 0.0, -1.0],
            up: [0.0, 1.0, 0.0],
            vertical_fov_degrees: 46.0,
        };
        let bindings = bind_presentation_function_params(view, &SmolStr::new("scene_region"), camera);

        let lighting =
            authored_presentation_lighting_inputs(view, &bindings).expect("authored lighting");
        assert_eq!(lighting.key_light.position, [1.5, 3.0, 3.5]);
        assert_eq!(lighting.key_light.range, 8.0);
        assert!((lighting.fill_strength - 0.33).abs() <= 1e-6);
        assert_eq!(lighting.ambient_color, [0.08, 0.11, 0.14]);
    }

    #[test]
    fn prepared_execution_applies_domain_participant_policy() {
        let module = lower_inline_module(
            r#"
domain sample_domain(world: RegionCapture) {
    geometry_detail = 1
    material = true
    radiance = false
    media = false
    max_distance = 6.0
    min_step = 0.05
    hit_epsilon = 0.001
    max_steps = 64
}

view sample_view(world: RegionCapture, camera: Camera) {
    domain = sample_domain(world = world)
    viewport = viewport(width = 2, height = 2)
}
"#,
        );
        let view = function(&module, "sample_view");
        let plan = wrela::presentation_plan::PresentationPlan::from_render_function(
            view,
            wrela::query_plan::DispatchBackend::Auto,
        )
        .expect("plan");
        let prepared = prepare_presentation_execution(
            &module,
            &plan,
            view,
            SmolStr::new("scene_region"),
            SmolStr::new("sample_domain"),
            wrela::presentation_contract::CanonicalCameraInput {
                position: [0.0, 0.0, 3.0],
                forward: [0.0, 0.0, -1.0],
                up: [0.0, 1.0, 0.0],
                vertical_fov_degrees: 46.0,
            },
            None,
            None,
            0,
            1.0 / 60.0,
            wrela::query_plan::DispatchBackend::Auto,
        )
        .expect("prepared execution");

        assert!(prepared.plan.validate().is_empty());
        assert!(!prepared.plan.passes.iter().any(|pass| {
            matches!(
                pass.kind,
                wrela::presentation_plan::PresentationPassKind::ParticipantsResolve { .. }
            )
        }));
        assert!(
            prepared
                .plan
                .frame
                .outputs
                .iter()
                .all(|attachment| attachment.name != "radiance" && attachment.name != "medium")
        );
    }
}
