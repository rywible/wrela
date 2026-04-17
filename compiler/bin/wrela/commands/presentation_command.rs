//! Owns CLI-facing presentation subcommand orchestration and bridges typed
//! presentation command arguments into compilation/execution helpers.
//! Does not own CLI token parsing or the presentation runtime itself.
//!
//! Key invariants:
//! - command-local legality checks that remain here operate on typed parse-time
//!   data, not raw argv bags.
//! - preview/frame/debug/reporting surfaces must describe the backend and
//!   attachments that actually executed.
//!
//! Primary entrypoints:
//! - `execute_preview_command`
//! - `execute_frame_command`
//! - `execute_presentation_debug_command`
//!
//! Failure modes / common pitfalls:
//! - reparsing raw presentation flags after dispatch would reintroduce the
//!   stringly command-bag problem Phase 54 is closing.

use super::observer_projection::*;
use super::presentation_reports::*;
use super::preview_eval::*;
use super::*;

pub(crate) fn execute_presentation_plan_command(args: PresentationPlanCommandArgs) {
    let entry_path = match resolve_entry_path(args.path_arg.as_deref()) {
        Ok(path) => path,
        Err(err) => {
            eprintln!("error: {err}");
            std::process::exit(EXIT_USAGE);
        }
    };
    let plans =
        match compile_presentation_plans(&entry_path, args.output_format, args.query_backend) {
            Ok(plans) => plans,
            Err(code) => std::process::exit(code),
        };
    let dump = presentation_plan_dump(&entry_path, &plans);
    if matches!(args.output_format, OutputFormat::Json) {
        println!(
            "{}",
            serde_json::to_string_pretty(&dump).unwrap_or_else(|_| "{}".to_string())
        );
    } else {
        print_presentation_plan_human(&dump);
    }
}

pub(crate) const WRELA_PRESENTATION_DEBUG_WARM_QUALITY_PIPELINES_ENV: &str =
    "WRELA_PRESENTATION_DEBUG_WARM_QUALITY_PIPELINES";
pub(crate) const WRELA_PRESENTATION_DEBUG_ADAPTIVE_WINDOW_ENV: &str =
    "WRELA_PRESENTATION_DEBUG_ADAPTIVE_WINDOW";

pub(crate) struct CompiledPresentationBundle {
    pub(crate) module: hir::Module,
    pub(crate) query_ctx: wrela::query_exec::QueryExecContext,
    pub(crate) plans: Vec<wrela::presentation_plan::PresentationPlan>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PreparedPresentationExecution {
    pub(crate) plan: wrela::presentation_plan::PresentationPlan,
    pub(crate) input: wrela::presentation_exec::PresentationExecutionInput,
    pub(crate) semantic_domain: String,
    pub(crate) execution_policy: wrela::presentation_exec::PresentationExecutionPolicy,
    pub(crate) camera: wrela::presentation_contract::CanonicalCameraInput,
    pub(crate) viewport: wrela::presentation_contract::CanonicalViewportInput,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DomainExecutionInputs {
    pub(crate) frame_domain: wrela::kernel::KernelValue,
    pub(crate) semantic_domain: String,
    pub(crate) execution_policy: wrela::presentation_exec::PresentationExecutionPolicy,
}

pub(crate) struct ReadyPresentationExecution {
    pub(crate) bundle: CompiledPresentationBundle,
    pub(crate) prepared: PreparedPresentationExecution,
    pub(crate) region_name: SmolStr,
    pub(crate) domain_name: SmolStr,
}

pub(crate) fn execute_presentation_debug_command(args: PresentationDebugCommandArgs) {
    let entry_path = match resolve_entry_path(args.path_arg.as_deref()) {
        Ok(path) => path,
        Err(err) => {
            eprintln!("error: {err}");
            std::process::exit(EXIT_USAGE);
        }
    };
    let options = args.options;
    let bundle =
        match compile_presentation_bundle(&entry_path, args.output_format, args.query_backend) {
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
        &bundle.query_ctx,
        plan,
        view_func,
        region_name.clone(),
        domain_name.clone(),
        camera,
        options.width,
        options.height,
        options.frame_index,
        options.delta_seconds,
        args.query_backend,
        options.query_trace_solver_mode,
        options.skip_export,
    ) {
        Ok(prepared) => prepared,
        Err(err) => {
            eprintln!("error: {err}");
            std::process::exit(EXIT_USAGE);
        }
    };
    if args.query_backend == wrela::query_plan::DispatchBackend::Wgsl
        && env_flag_truthy(WRELA_PRESENTATION_DEBUG_WARM_QUALITY_PIPELINES_ENV)
    {
        if let Err(err) = warm_presentation_debug_quality_pipelines(
            &bundle.query_ctx,
            &prepared.plan,
            &prepared.input,
            prepared.camera,
            prepared.viewport,
            options.frame_index,
            options.delta_seconds,
        ) {
            eprintln!("presentation warmup error: {err}");
            std::process::exit(EXIT_CODEGEN);
        }
    }
    let adaptive_window = env_usize_override(WRELA_PRESENTATION_DEBUG_ADAPTIVE_WINDOW_ENV);
    let mut session = wrela::presentation_exec::AdaptivePresentationSession::new(
        prepared.plan.frame.quality.clone(),
    );
    if let Some(window) = adaptive_window {
        session = session.with_window(window);
    }
    let mut frame_cost_history = Vec::new();
    let mut result = None;
    for frame_offset in 0..options.frames.max(1) {
        let mut frame_input = prepared.input.clone();
        frame_input.materialize_cpu_attachments = !options.skip_export;
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
    let artifacts = if options.skip_export {
        wrela::presentation_exec::debug::PresentationDebugArtifacts {
            color_ppm: None,
            depth_ppm: None,
            world_normal_ppm: None,
            stats_path: PathBuf::from("<not exported>"),
        }
    } else {
        let out_dir = options.out_dir.unwrap_or_else(|| {
            entry_path
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join("presentation_debug")
                .join(plan.name.as_str())
        });
        match wrela::presentation_exec::debug::export_frame_debug(&result, &out_dir) {
            Ok(artifacts) => artifacts,
            Err(err) => {
                eprintln!("presentation debug export error: {err}");
                std::process::exit(EXIT_CODEGEN);
            }
        }
    };
    let stats = wrela::presentation_exec::debug::render_primary_visibility_stats(&result);
    let snapshot = bundle
        .query_ctx
        .snapshot_report_for_capture_name(&region_name)
        .expect("presentation debug region snapshot");
    let dump = PresentationDebugDump {
        schema_version: 1,
        view: plan.name.to_string(),
        region: region_name.to_string(),
        domain: domain_name.to_string(),
        query_trace_solver_mode: options.query_trace_solver_mode.as_str().to_string(),
        backend: dispatch_backend_name(result.backend).to_string(),
        semantic_domain: prepared.semantic_domain.clone(),
        execution_policy: result.frame_cost.execution_policy.clone(),
        snapshot,
        frames_executed: frame_cost_history.len() as u32,
        color_ppm: artifacts
            .color_ppm
            .as_ref()
            .map(|path| path.display().to_string()),
        depth_ppm: artifacts
            .depth_ppm
            .as_ref()
            .map(|path| path.display().to_string()),
        world_normal_ppm: artifacts
            .world_normal_ppm
            .as_ref()
            .map(|path| path.display().to_string()),
        stats_path: artifacts.stats_path.display().to_string(),
        stats,
        frame_cost: result.frame_cost.clone(),
        frame_cost_history,
    };
    if matches!(args.output_format, OutputFormat::Json) {
        println!(
            "{}",
            serde_json::to_string_pretty(&dump).unwrap_or_else(|_| "{}".to_string())
        );
    } else {
        println!("presentation debug schema v{}", dump.schema_version);
        println!(
            "presentation debug view={} backend={}",
            dump.view, dump.backend
        );
        println!(
            "  query trace solver mode: {}",
            dump.query_trace_solver_mode
        );
        println!("  region: {}", dump.region);
        println!("  domain: {}", dump.domain);
        println!("  semantic domain: {}", dump.semantic_domain);
        println!("  execution policy: {}", dump.execution_policy);
        println!(
            "  snapshot: {} snapshot_id={} epoch={} portable_scene_id={}",
            dump.snapshot.capture_name,
            dump.snapshot.snapshot_id.0,
            dump.snapshot.epoch.0,
            dump.snapshot.portable_scene_id
        );
        println!("  frames: {}", dump.frames_executed);
        println!("  field samples: {}", dump.frame_cost.field_samples);
        println!(
            "  color ppm: {}",
            dump.color_ppm.as_deref().unwrap_or("not materialized")
        );
        println!(
            "  depth ppm: {}",
            dump.depth_ppm.as_deref().unwrap_or("not materialized")
        );
        println!(
            "  world normal ppm: {}",
            dump.world_normal_ppm
                .as_deref()
                .unwrap_or("not materialized")
        );
        println!("  stats: {}", dump.stats_path);
        println!("{}", dump.stats.trim_end());
    }
}

pub(crate) fn env_flag_truthy(name: &str) -> bool {
    env::var(name)
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

pub(crate) fn env_usize_override(name: &str) -> Option<usize> {
    env::var(name)
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
}

pub(crate) fn warm_presentation_debug_quality_pipelines(
    ctx: &wrela::query_exec::QueryExecContext,
    plan: &wrela::presentation_plan::PresentationPlan,
    input: &wrela::presentation_exec::PresentationExecutionInput,
    camera: wrela::presentation_contract::CanonicalCameraInput,
    viewport: wrela::presentation_contract::CanonicalViewportInput,
    frame_index: u32,
    delta_seconds: f32,
) -> Result<(), wrela::presentation_exec::PresentationExecError> {
    let mut history = None;
    let mut quality = plan.frame.quality.initial_state();
    let mut warm_states = vec![quality.clone()];
    while quality.step_down(&plan.frame.quality) {
        warm_states.push(quality.clone());
    }
    for (offset, quality_override) in warm_states.into_iter().enumerate() {
        let mut frame_input = input.clone();
        frame_input.materialize_cpu_attachments = false;
        frame_input.history = history.clone();
        frame_input.quality_override = Some(quality_override);
        frame_input.frame_state = wrela::presentation_exec::frame_state_value(
            camera,
            camera,
            viewport,
            [0.0, 0.0],
            frame_index.saturating_add(offset as u32),
            delta_seconds,
        );
        let result = wrela::presentation_exec::execute_plan(ctx, plan, &frame_input)?;
        history = result.history;
    }
    Ok(())
}

pub(crate) fn execute_preview_command(args: PreviewCommandArgs) {
    let entry_path = match resolve_entry_path(args.path_arg.as_deref()) {
        Ok(path) => path,
        Err(err) => {
            eprintln!("error: {err}");
            std::process::exit(EXIT_USAGE);
        }
    };
    let options = args.options;
    let ready = match load_prepared_presentation_execution(
        &entry_path,
        args.output_format,
        args.query_backend,
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
        wrela::query_exec::QueryTraceSolverMode::Hybrid,
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
        let snapshot = ready
            .bundle
            .query_ctx
            .snapshot_report_for_capture_name(&ready.region_name)
            .expect("preview region snapshot");
        let dump = PreviewReportDump {
            schema_version: 1,
            view: ready.prepared.plan.name.to_string(),
            region: ready.region_name.to_string(),
            domain: ready.domain_name.to_string(),
            attachment: attachment_name.clone(),
            backend: dispatch_backend_name(result.backend).to_string(),
            semantic_domain: ready.prepared.semantic_domain.clone(),
            execution_policy: result.frame_cost.execution_policy.clone(),
            snapshot,
            width: result.width,
            height: result.height,
            stats: wrela::presentation_exec::debug::render_primary_visibility_stats(&result),
            frame_cost: result.frame_cost.clone(),
        };
        if matches!(args.output_format, OutputFormat::Json) {
            println!(
                "{}",
                serde_json::to_string_pretty(&dump).unwrap_or_else(|_| "{}".to_string())
            );
        } else {
            println!("preview report view={} backend={}", dump.view, dump.backend);
            println!("  region: {}", dump.region);
            println!("  domain: {}", dump.domain);
            println!("  semantic domain: {}", dump.semantic_domain);
            println!("  execution policy: {}", dump.execution_policy);
            println!(
                "  snapshot: {} snapshot_id={} epoch={} portable_scene_id={}",
                dump.snapshot.capture_name,
                dump.snapshot.snapshot_id.0,
                dump.snapshot.epoch.0,
                dump.snapshot.portable_scene_id
            );
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

pub(crate) fn execute_frame_command(args: FrameCommandArgs) {
    let entry_path = match resolve_entry_path(args.path_arg.as_deref()) {
        Ok(path) => path,
        Err(err) => {
            eprintln!("error: {err}");
            std::process::exit(EXIT_USAGE);
        }
    };
    let options = args.options;
    let ready = match load_prepared_presentation_execution(
        &entry_path,
        args.output_format,
        args.query_backend,
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
        wrela::query_exec::QueryTraceSolverMode::Hybrid,
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
        semantic_domain: ready.prepared.semantic_domain.clone(),
        execution_policy: result.frame_cost.execution_policy.clone(),
        snapshot: ready
            .bundle
            .query_ctx
            .snapshot_report_for_capture_name(&ready.region_name)
            .expect("frame region snapshot"),
        width: result.width,
        height: result.height,
        frame_index: options.frame_index,
        attachments,
        frame_cost: result.frame_cost.clone(),
    };
    if matches!(args.output_format, OutputFormat::Json) {
        println!(
            "{}",
            serde_json::to_string_pretty(&dump).unwrap_or_else(|_| "{}".to_string())
        );
    } else {
        println!("frame bundle view={} backend={}", dump.view, dump.backend);
        println!("  region: {}", dump.region);
        println!("  domain: {}", dump.domain);
        println!("  semantic domain: {}", dump.semantic_domain);
        println!("  execution policy: {}", dump.execution_policy);
        println!(
            "  snapshot: {} snapshot_id={} epoch={} portable_scene_id={}",
            dump.snapshot.capture_name,
            dump.snapshot.snapshot_id.0,
            dump.snapshot.epoch.0,
            dump.snapshot.portable_scene_id
        );
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

pub(crate) fn execute_frame_contracts_command(args: FrameContractsCommandArgs) {
    let entry_path = match resolve_entry_path(args.path_arg.as_deref()) {
        Ok(path) => path,
        Err(err) => {
            eprintln!("error: {err}");
            std::process::exit(EXIT_USAGE);
        }
    };
    let requested_view = args.requested_view;
    let plans =
        match compile_presentation_plans(&entry_path, args.output_format, args.query_backend) {
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
    if matches!(args.output_format, OutputFormat::Json) {
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
                "  temporal change class: {}",
                view.frame
                    .temporal_change_class
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string())
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

pub(crate) fn load_prepared_presentation_execution(
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
    query_trace_solver_mode: wrela::query_exec::QueryTraceSolverMode,
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
        &bundle.query_ctx,
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
        query_trace_solver_mode,
        false,
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

pub(crate) fn selected_frame_attachment_names(
    result: &wrela::presentation_exec::PresentationExecutionResult,
    requested: &[String],
) -> Result<Vec<String>, wrela::presentation_exec::PresentationExecError> {
    if requested.is_empty() {
        return Ok(result
            .attachments
            .attachments
            .keys()
            .map(ToString::to_string)
            .collect());
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

pub(crate) fn compile_presentation_plans(
    entry_path: &Path,
    output_format: OutputFormat,
    query_backend: wrela::query_plan::DispatchBackend,
) -> Result<Vec<wrela::presentation_plan::PresentationPlan>, i32> {
    compile_presentation_bundle(entry_path, output_format, query_backend).map(|bundle| bundle.plans)
}

pub(crate) fn compile_presentation_bundle(
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

pub(crate) fn select_view_plan<'a>(
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

pub(crate) fn select_region_name(
    module: &hir::Module,
    requested: Option<&str>,
) -> Result<SmolStr, String> {
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

pub(crate) fn select_domain_name(
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
        .presentation
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
        Err(
            "presentation execution requires --domain when the view does not name a single domain"
                .to_string(),
        )
    }
}

pub(crate) fn body_called_function_name(body: &hir::Body) -> Option<SmolStr> {
    let expr_id = body_terminal_expr_id(body)?;
    let hir::Expr::Call { callee, .. } = &body.exprs[expr_id] else {
        return None;
    };
    let hir::Expr::Variable(name) = &body.exprs[*callee] else {
        return None;
    };
    Some(name.clone())
}

pub(crate) fn body_terminal_expr_id(body: &hir::Body) -> Option<hir::Idx<hir::Expr>> {
    let stmt = body.root_stmts.last()?;
    match body.stmts[*stmt] {
        hir::Stmt::Expr(expr) => Some(expr),
        hir::Stmt::Return(Some(expr)) => Some(expr),
        _ => None,
    }
}

pub(crate) fn body_terminal_call_args<'a>(
    body: &'a hir::Body,
) -> Option<(&'a SmolStr, &'a [hir::Arg])> {
    let expr_id = body_terminal_expr_id(body)?;
    let hir::Expr::Call { callee, args, .. } = &body.exprs[expr_id] else {
        return None;
    };
    let hir::Expr::Variable(name) = &body.exprs[*callee] else {
        return None;
    };
    Some((name, args.as_slice()))
}

pub(crate) fn helper_call_named_expr_id(
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

pub(crate) fn resolve_view_dimension(
    view: &hir::Function,
    override_value: Option<u32>,
    width: bool,
) -> Result<u32, String> {
    if let Some(value) = override_value {
        return Ok(value);
    }
    let metadata = view
        .presentation
        .as_ref()
        .ok_or_else(|| "selected view is missing presentation metadata".to_string())?;
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
    .ok_or_else(|| {
        format!("presentation execution requires --{label} when the view omits {label}")
    })?;
    eval_body_u32(body).ok_or_else(|| {
        format!(
            "presentation execution cannot evaluate non-literal view {label}; pass --{label} explicitly"
        )
    })
}

pub(crate) fn eval_body_u32(body: &hir::Body) -> Option<u32> {
    let expr_id = body_terminal_expr_id(body)?;
    eval_expr_u32(body, expr_id)
}

pub(crate) fn eval_body_i32_in_module(module: &hir::Module, body: &hir::Body) -> Option<i32> {
    let expr_id = body_terminal_expr_id(body)?;
    eval_expr_i32_in_module(module, body, expr_id)
}

pub(crate) fn eval_expr_i32_in_module(
    module: &hir::Module,
    body: &hir::Body,
    expr_id: hir::Idx<hir::Expr>,
) -> Option<i32> {
    eval_expr_f32_in_module(module, body, expr_id).map(|value| value as i32)
}

pub(crate) fn eval_body_f32_in_module(module: &hir::Module, body: &hir::Body) -> Option<f32> {
    let expr_id = body_terminal_expr_id(body)?;
    eval_expr_f32_in_module(module, body, expr_id)
}

pub(crate) fn eval_expr_u32(body: &hir::Body, expr_id: hir::Idx<hir::Expr>) -> Option<u32> {
    eval_expr_f32(body, expr_id).map(|value| value.max(0.0) as u32)
}

pub(crate) fn eval_expr_f32(body: &hir::Body, expr_id: hir::Idx<hir::Expr>) -> Option<f32> {
    match &body.exprs[expr_id] {
        hir::Expr::Literal(hir::Literal::Integer(value)) => Some(*value as f32),
        hir::Expr::Literal(hir::Literal::Float(value)) => Some(*value as f32),
        _ => None,
    }
}

pub(crate) fn eval_expr_f32_in_module(
    module: &hir::Module,
    body: &hir::Body,
    expr_id: hir::Idx<hir::Expr>,
) -> Option<f32> {
    fn eval(
        module: &hir::Module,
        body: &hir::Body,
        expr_id: hir::Idx<hir::Expr>,
        stack: &mut HashSet<SmolStr>,
    ) -> Option<f32> {
        match &body.exprs[expr_id] {
            hir::Expr::Literal(hir::Literal::Integer(value)) => Some(*value as f32),
            hir::Expr::Literal(hir::Literal::Float(value)) => Some(*value as f32),
            hir::Expr::Unary {
                op: hir::body::UnaryOp::Neg,
                expr,
                ..
            } => eval(module, body, *expr, stack).map(|value| -value),
            hir::Expr::Call {
                callee,
                args,
                type_args,
            } if args.is_empty() && type_args.is_empty() => {
                let hir::Expr::Variable(name) = &body.exprs[*callee] else {
                    return None;
                };
                if !stack.insert(name.clone()) {
                    return None;
                }
                let value = module
                    .functions
                    .iter()
                    .find(|(_, func)| func.name == *name && func.params.is_empty())
                    .and_then(|(_, func)| func.body.as_ref())
                    .and_then(|helper_body| {
                        let helper_expr = body_terminal_expr_id(helper_body)?;
                        eval(module, helper_body, helper_expr, stack)
                    });
                stack.remove(name);
                value
            }
            _ => None,
        }
    }

    let mut stack = HashSet::new();
    eval(module, body, expr_id, &mut stack)
}

pub(crate) fn domain_execution_inputs(
    module: &hir::Module,
    domain: &hir::Function,
    region_name: &SmolStr,
    query_backend: wrela::query_plan::DispatchBackend,
) -> Result<DomainExecutionInputs, String> {
    let metadata = domain.domain.as_ref().expect("domain metadata");
    let geometry_detail = match metadata.geometry_detail {
        hir::DomainGeometryDetail::Coarse => 0,
        hir::DomainGeometryDetail::Fine => 1,
    };
    let _ = query_backend;
    let policy_metadata = domain.domain_execution_policy.as_ref().ok_or_else(|| {
        format!(
            "domain `{}` is missing lowered execution policy metadata",
            domain.name
        )
    })?;
    let primary_rays = wrela::presentation_exec::RayBudgetPolicy {
        max_distance: authored_domain_f32(module, policy_metadata.max_distance.as_ref())
            .unwrap_or(16.0),
        min_step: authored_domain_f32(module, policy_metadata.min_step.as_ref()).unwrap_or(0.01),
        hit_epsilon: authored_domain_f32(module, policy_metadata.hit_epsilon.as_ref())
            .unwrap_or(0.001),
        max_steps: authored_domain_i32(module, policy_metadata.max_steps.as_ref()).unwrap_or(128),
    };
    let execution_policy = wrela::presentation_exec::PresentationExecutionPolicy::new(
        policy_metadata.required_guarantee,
        policy_metadata.selected_method,
        primary_rays,
    );
    Ok(DomainExecutionInputs {
        frame_domain: wrela::presentation_exec::scene_domain_value(
            wrela::query_exec::stable_region_scene_capture_id(region_name),
            geometry_detail,
            metadata.material,
            metadata.radiance,
            metadata.media,
        ),
        semantic_domain: wrela::presentation_exec::render_semantic_domain_report(
            wrela::query_exec::stable_region_scene_capture_id(region_name),
            geometry_detail,
            metadata.material,
            metadata.radiance,
            metadata.media,
        ),
        execution_policy,
    })
}

pub(crate) fn authored_domain_f32(module: &hir::Module, body: Option<&hir::Body>) -> Option<f32> {
    body.and_then(|body| eval_body_f32_in_module(module, body))
}

pub(crate) fn authored_domain_i32(module: &hir::Module, body: Option<&hir::Body>) -> Option<i32> {
    body.and_then(|body| eval_body_i32_in_module(module, body))
}
