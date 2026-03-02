use super::{
    DiagnosticScope, EXIT_CODEGEN, EXIT_OK, EXIT_TYPE, EXIT_USAGE, OutputFormat,
    apply_source_fixes, collect_safe_fixes, resolve_entry_path,
};
use serde::Serialize;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const FRONTEND_PIPELINE_SCHEMA_VERSION: u32 = 1;
const FRONTEND_MAX_FIX_PASSES: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FrontendPipelineSubcommand {
    Preview,
    Explain,
    Fix,
}

impl FrontendPipelineSubcommand {
    fn from_command(command: &str) -> Option<Self> {
        match command {
            "frontend-preview" | "preview" => Some(Self::Preview),
            "frontend-explain" | "explain" => Some(Self::Explain),
            "frontend-fix" | "fix" => Some(Self::Fix),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Preview => "preview",
            Self::Explain => "explain",
            Self::Fix => "fix",
        }
    }

    fn summary_kind(self) -> &'static str {
        match self {
            Self::Preview => "wrela.frontend.preview.summary",
            Self::Explain => "wrela.frontend.explain.summary",
            Self::Fix => "wrela.frontend.fix.summary",
        }
    }

    fn summary_event(self) -> &'static str {
        match self {
            Self::Preview => "frontend_preview_summary",
            Self::Explain => "frontend_explain_summary",
            Self::Fix => "frontend_fix_summary",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
struct FrontendFixSummary {
    attempted: usize,
    applied: usize,
    skipped: usize,
    errors: usize,
    touched_files: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct FrontendPipelineSummary {
    schema_version: u32,
    kind: String,
    subcommand: String,
    app_slug: String,
    run_id: String,
    timestamp_epoch_seconds: u64,
    frontend_path: String,
    entry_path: String,
    status: String,
    fix: FrontendFixSummary,
    #[serde(skip_serializing_if = "Option::is_none")]
    preview_artifacts: Option<FrontendPreviewArtifacts>,
    #[serde(skip_serializing_if = "Option::is_none")]
    explain_artifacts: Option<FrontendExplainArtifacts>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct FrontendPreviewArtifacts {
    artifact_root: String,
    preview_report_path: String,
    interaction_log_path: String,
    assertion_summary_path: String,
    screenshots_path: String,
    console_error_summary_path: String,
    browser_log_path: String,
    server_log_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct FrontendExplainArtifacts {
    artifact_root: String,
    build_manifest_path: String,
    render_manifest_path: String,
    shader_bundle_path: String,
    expansion_trace_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FrontendPreviewArtifactPaths {
    artifact_root: PathBuf,
    preview_report_path: PathBuf,
    interaction_log_path: PathBuf,
    assertion_summary_path: PathBuf,
    screenshots_path: PathBuf,
    console_error_summary_path: PathBuf,
    browser_log_path: PathBuf,
    server_log_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FrontendExplainArtifactPaths {
    artifact_root: PathBuf,
    build_manifest_path: PathBuf,
    render_manifest_path: PathBuf,
    shader_bundle_path: PathBuf,
    expansion_trace_path: PathBuf,
}

impl FrontendPreviewArtifactPaths {
    fn as_summary_artifacts(&self) -> FrontendPreviewArtifacts {
        FrontendPreviewArtifacts {
            artifact_root: self.artifact_root.display().to_string(),
            preview_report_path: self.preview_report_path.display().to_string(),
            interaction_log_path: self.interaction_log_path.display().to_string(),
            assertion_summary_path: self.assertion_summary_path.display().to_string(),
            screenshots_path: self.screenshots_path.display().to_string(),
            console_error_summary_path: self.console_error_summary_path.display().to_string(),
            browser_log_path: self.browser_log_path.display().to_string(),
            server_log_path: self.server_log_path.display().to_string(),
        }
    }
}

impl FrontendExplainArtifactPaths {
    fn as_summary_artifacts(&self) -> FrontendExplainArtifacts {
        FrontendExplainArtifacts {
            artifact_root: self.artifact_root.display().to_string(),
            build_manifest_path: self.build_manifest_path.display().to_string(),
            render_manifest_path: self.render_manifest_path.display().to_string(),
            shader_bundle_path: self.shader_bundle_path.display().to_string(),
            expansion_trace_path: self.expansion_trace_path.display().to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct FrontendPipelineSummaryEvent {
    event: &'static str,
    artifact_path: String,
    summary: FrontendPipelineSummary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FrontendArtifactRunContext {
    app_slug: String,
    run_id: String,
    timestamp_epoch_seconds: u64,
}

fn frontend_app_slug(frontend_root: &Path) -> String {
    frontend_root
        .file_name()
        .and_then(|value| value.to_str())
        .map(|value| {
            let slug = value
                .chars()
                .map(|ch| {
                    if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                        ch
                    } else {
                        '-'
                    }
                })
                .collect::<String>()
                .trim_matches('-')
                .to_string();
            if slug.is_empty() {
                "frontend-app".to_string()
            } else {
                slug
            }
        })
        .unwrap_or_else(|| "frontend-app".to_string())
}

fn build_frontend_artifact_run_context(frontend_root: &Path) -> FrontendArtifactRunContext {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    FrontendArtifactRunContext {
        app_slug: frontend_app_slug(frontend_root),
        run_id: format!("run-{}-{}", now.as_millis(), std::process::id()),
        timestamp_epoch_seconds: now.as_secs(),
    }
}

#[derive(Debug, Serialize)]
struct FrontendPipelineError {
    schema_version: u32,
    kind: String,
    subcommand: String,
    message: String,
}

fn frontend_summary_artifact_path(
    frontend_root: &Path,
    subcommand: FrontendPipelineSubcommand,
    run_context: &FrontendArtifactRunContext,
) -> PathBuf {
    frontend_pipeline_artifact_dir(frontend_root, subcommand, run_context).join("summary.json")
}

fn frontend_pipeline_artifact_dir(
    frontend_root: &Path,
    subcommand: FrontendPipelineSubcommand,
    run_context: &FrontendArtifactRunContext,
) -> PathBuf {
    frontend_root
        .join(".artifacts")
        .join("frontend-pipeline")
        .join(subcommand.as_str())
        .join(run_context.app_slug.as_str())
        .join(run_context.run_id.as_str())
}

fn frontend_preview_artifact_paths(
    frontend_root: &Path,
    run_context: &FrontendArtifactRunContext,
) -> FrontendPreviewArtifactPaths {
    let artifact_root = frontend_pipeline_artifact_dir(
        frontend_root,
        FrontendPipelineSubcommand::Preview,
        run_context,
    );
    FrontendPreviewArtifactPaths {
        preview_report_path: artifact_root.join("preview-report.json"),
        interaction_log_path: artifact_root.join("interaction-log.json"),
        assertion_summary_path: artifact_root.join("assertion-summary.json"),
        screenshots_path: artifact_root.join("screenshots.json"),
        console_error_summary_path: artifact_root.join("console-error-summary.json"),
        browser_log_path: artifact_root.join("browser.log"),
        server_log_path: artifact_root.join("server.log"),
        artifact_root,
    }
}

fn frontend_explain_artifact_paths(
    frontend_root: &Path,
    run_context: &FrontendArtifactRunContext,
) -> FrontendExplainArtifactPaths {
    let artifact_root = frontend_pipeline_artifact_dir(
        frontend_root,
        FrontendPipelineSubcommand::Explain,
        run_context,
    );
    FrontendExplainArtifactPaths {
        build_manifest_path: artifact_root.join("build-manifest.json"),
        render_manifest_path: artifact_root.join("render-manifest.json"),
        shader_bundle_path: artifact_root.join("shader-bundle.json"),
        expansion_trace_path: artifact_root.join("expansion-trace.json"),
        artifact_root,
    }
}

fn build_frontend_summary(
    subcommand: FrontendPipelineSubcommand,
    run_context: &FrontendArtifactRunContext,
    frontend_path: String,
    entry_path: String,
    status: String,
    fix: FrontendFixSummary,
    preview_artifacts: Option<FrontendPreviewArtifacts>,
    explain_artifacts: Option<FrontendExplainArtifacts>,
) -> FrontendPipelineSummary {
    FrontendPipelineSummary {
        schema_version: FRONTEND_PIPELINE_SCHEMA_VERSION,
        kind: subcommand.summary_kind().to_string(),
        subcommand: subcommand.as_str().to_string(),
        app_slug: run_context.app_slug.clone(),
        run_id: run_context.run_id.clone(),
        timestamp_epoch_seconds: run_context.timestamp_epoch_seconds,
        frontend_path,
        entry_path,
        status,
        fix,
        preview_artifacts,
        explain_artifacts,
    }
}

fn write_frontend_summary_artifact(
    artifact_path: &Path,
    summary: &FrontendPipelineSummary,
) -> Result<(), String> {
    let Some(parent) = artifact_path.parent() else {
        return Err(format!(
            "failed to resolve parent directory for {}",
            artifact_path.display()
        ));
    };
    fs::create_dir_all(parent).map_err(|error| {
        format!(
            "failed to create frontend pipeline artifact directory {}: {error}",
            parent.display()
        )
    })?;
    let body = serde_json::to_string_pretty(summary)
        .map(|json| format!("{json}\n"))
        .map_err(|error| format!("failed to encode frontend pipeline summary: {error}"))?;
    fs::write(artifact_path, body).map_err(|error| {
        format!(
            "failed to write frontend pipeline summary {}: {error}",
            artifact_path.display()
        )
    })
}

fn emit_frontend_error(
    output_format: OutputFormat,
    subcommand: FrontendPipelineSubcommand,
    message: &str,
) {
    if matches!(output_format, OutputFormat::Json) {
        let json = FrontendPipelineError {
            schema_version: FRONTEND_PIPELINE_SCHEMA_VERSION,
            kind: "wrela.frontend.error".to_string(),
            subcommand: subcommand.as_str().to_string(),
            message: message.to_string(),
        };
        eprintln!(
            "{}",
            serde_json::to_string(&json).unwrap_or_else(|_| "{}".to_string())
        );
    } else {
        eprintln!("error: {message}");
    }
}

fn emit_frontend_summary_event(
    output_format: OutputFormat,
    subcommand: FrontendPipelineSubcommand,
    artifact_path: &Path,
    summary: &FrontendPipelineSummary,
) {
    if !matches!(output_format, OutputFormat::Json) {
        return;
    }
    let event = FrontendPipelineSummaryEvent {
        event: subcommand.summary_event(),
        artifact_path: artifact_path.display().to_string(),
        summary: summary.clone(),
    };
    println!(
        "{}",
        serde_json::to_string(&event).unwrap_or_else(|_| "{}".to_string())
    );
}

fn run_preview_or_explain(
    entry_path: &Path,
    output_format: OutputFormat,
) -> (FrontendFixSummary, String, i32) {
    let diagnostic_scope = DiagnosticScope::from_entrypoint(entry_path, true);
    match collect_safe_fixes(entry_path, output_format, false, &diagnostic_scope) {
        Ok(fixes) => {
            let attempted = fixes.len();
            (
                FrontendFixSummary {
                    attempted,
                    applied: 0,
                    skipped: attempted,
                    errors: 0,
                    touched_files: 0,
                },
                if attempted == 0 {
                    "clean".to_string()
                } else {
                    "diagnostics".to_string()
                },
                EXIT_OK,
            )
        }
        Err(code) => (
            FrontendFixSummary {
                errors: 1,
                ..Default::default()
            },
            "error".to_string(),
            code,
        ),
    }
}

fn resolve_frontend_preview_script_path(frontend_root: &Path) -> Result<PathBuf, String> {
    for ancestor in frontend_root.ancestors() {
        let script_path = ancestor
            .join("scripts")
            .join("vertical_slice")
            .join("browser_smoke.sh");
        if script_path.is_file() {
            return Ok(script_path);
        }
    }
    let cwd = std::env::current_dir().map_err(|error| {
        format!("failed to resolve current directory for frontend preview: {error}")
    })?;
    let fallback = cwd
        .join("scripts")
        .join("vertical_slice")
        .join("browser_smoke.sh");
    if fallback.is_file() {
        return Ok(fallback);
    }
    Err(format!(
        "frontend preview script not found; expected scripts/vertical_slice/browser_smoke.sh near {}",
        frontend_root.display()
    ))
}

fn run_frontend_preview_harness(
    frontend_root: &Path,
    artifact_paths: &FrontendPreviewArtifactPaths,
) -> Result<(), String> {
    let script_path = resolve_frontend_preview_script_path(frontend_root)?;
    let output = Command::new("bash")
        .arg(script_path.as_path())
        .arg(frontend_root.as_os_str())
        .arg(artifact_paths.artifact_root.as_os_str())
        .output()
        .map_err(|error| {
            format!(
                "failed to run frontend preview harness {}: {error}",
                script_path.display()
            )
        })?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(output.stderr.as_slice())
        .trim()
        .to_string();
    let stdout = String::from_utf8_lossy(output.stdout.as_slice())
        .trim()
        .to_string();
    let detail = if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        format!("process exited with status {}", output.status)
    };
    Err(format!(
        "frontend preview failed: {detail}. Inspect {} and {}",
        artifact_paths.assertion_summary_path.display(),
        artifact_paths.browser_log_path.display()
    ))
}

fn run_frontend_preview(
    frontend_root: &Path,
    run_context: &FrontendArtifactRunContext,
    entry_path: &Path,
    output_format: OutputFormat,
) -> (
    FrontendFixSummary,
    String,
    i32,
    Option<FrontendPreviewArtifacts>,
    Option<String>,
) {
    let (fix, base_status, diagnostics_exit_code) =
        run_preview_or_explain(entry_path, output_format);
    let artifact_paths = frontend_preview_artifact_paths(frontend_root, run_context);
    let preview_artifacts = Some(artifact_paths.as_summary_artifacts());
    if diagnostics_exit_code != EXIT_OK {
        return (
            fix,
            "preview_diagnostics_error".to_string(),
            diagnostics_exit_code,
            preview_artifacts,
            Some(
                "frontend preview diagnostics failed; resolve compiler diagnostics and retry"
                    .to_string(),
            ),
        );
    }

    match run_frontend_preview_harness(frontend_root, &artifact_paths) {
        Ok(()) => (
            fix,
            if base_status == "clean" {
                "preview_passed".to_string()
            } else {
                "preview_passed_with_diagnostics".to_string()
            },
            EXIT_OK,
            preview_artifacts,
            None,
        ),
        Err(message) => (
            fix,
            if base_status == "clean" {
                "preview_assertion_failed".to_string()
            } else {
                "preview_assertion_failed_with_diagnostics".to_string()
            },
            EXIT_TYPE,
            preview_artifacts,
            Some(message),
        ),
    }
}

fn copy_frontend_artifact(source: &Path, destination: &Path, label: &str) -> Result<(), String> {
    let bytes = fs::read(source)
        .map_err(|error| format!("failed to read {label} {}: {error}", source.display()))?;
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create frontend artifact directory {}: {error}",
                parent.display()
            )
        })?;
    }
    fs::write(destination, bytes)
        .map_err(|error| format!("failed to write {label} {}: {error}", destination.display()))
}

fn run_frontend_explain(
    frontend_root: &Path,
    run_context: &FrontendArtifactRunContext,
    frontend_path: &str,
    entry_path: &Path,
    output_format: OutputFormat,
) -> (
    FrontendFixSummary,
    String,
    i32,
    Option<FrontendExplainArtifacts>,
    Option<String>,
) {
    let (fix, base_status, diagnostics_exit_code) =
        run_preview_or_explain(entry_path, output_format);
    let explain_paths = frontend_explain_artifact_paths(frontend_root, run_context);
    let explain_artifacts = Some(explain_paths.as_summary_artifacts());
    if diagnostics_exit_code != EXIT_OK {
        return (
            fix,
            "explain_diagnostics_error".to_string(),
            diagnostics_exit_code,
            explain_artifacts,
            Some(
                "frontend explain diagnostics failed; resolve compiler diagnostics and retry"
                    .to_string(),
            ),
        );
    }

    let build_result = super::execute_game_command(
        super::GameCommandInput {
            command: "build".to_string(),
            path_arg: Some(frontend_path.to_string()),
            out_path: None,
            game_build_target: Some("dual".to_string()),
            game_render_backend: Some("webgpu".to_string()),
            game_host_mode: Some("pure-wasm".to_string()),
            game_client_runtime: Some("compiled".to_string()),
            game_shader_provenance: true,
            game_no_shortcuts: true,
            game_check_determinism: false,
            game_check_rollback: false,
            game_check_render_lane: false,
            game_check_asset_streaming: false,
            game_profile_gpu_metrics: false,
            game_profile_streaming_metrics: false,
        },
        output_format,
    );
    if let Err(error) = build_result {
        return (
            fix,
            "explain_build_error".to_string(),
            EXIT_CODEGEN,
            explain_artifacts,
            Some(format!(
                "frontend explain failed to compile frontend build artifacts: {error}"
            )),
        );
    }

    let dist_dir = super::game_dist_dir(frontend_root);
    let build_manifest_src = dist_dir.join("build-manifest.json");
    let render_manifest_src = dist_dir.join("render-manifest.json");
    let shader_bundle_src = dist_dir.join("shader-bundle.json");

    let copy_result = (|| -> Result<(), String> {
        copy_frontend_artifact(
            build_manifest_src.as_path(),
            explain_paths.build_manifest_path.as_path(),
            "build manifest",
        )?;
        copy_frontend_artifact(
            render_manifest_src.as_path(),
            explain_paths.render_manifest_path.as_path(),
            "render manifest",
        )?;
        copy_frontend_artifact(
            shader_bundle_src.as_path(),
            explain_paths.shader_bundle_path.as_path(),
            "shader bundle",
        )?;

        let render_manifest_text =
            fs::read_to_string(render_manifest_src.as_path()).map_err(|error| {
                format!(
                    "failed to read render manifest for explain trace {}: {error}",
                    render_manifest_src.display()
                )
            })?;
        let render_manifest_json: serde_json::Value =
            serde_json::from_str(render_manifest_text.as_str()).map_err(|error| {
                format!(
                    "failed to parse render manifest for explain trace {}: {error}",
                    render_manifest_src.display()
                )
            })?;
        let expansion_entries = render_manifest_json
            .get("provenance")
            .and_then(|value| value.get("expansion_trace"))
            .cloned()
            .unwrap_or_else(|| serde_json::json!([]));
        let expansion_trace = serde_json::json!({
            "schema_version": "render-expansion-trace-v1",
            "entries": expansion_entries,
        });
        fs::create_dir_all(explain_paths.artifact_root.as_path()).map_err(|error| {
            format!(
                "failed to create explain artifact directory {}: {error}",
                explain_paths.artifact_root.display()
            )
        })?;
        fs::write(
            explain_paths.expansion_trace_path.as_path(),
            serde_json::to_string_pretty(&expansion_trace)
                .map(|json| format!("{json}\n"))
                .map_err(|error| format!("failed to encode expansion trace: {error}"))?,
        )
        .map_err(|error| {
            format!(
                "failed to write expansion trace {}: {error}",
                explain_paths.expansion_trace_path.display()
            )
        })?;
        Ok(())
    })();

    if let Err(error) = copy_result {
        return (
            fix,
            "explain_artifact_error".to_string(),
            EXIT_CODEGEN,
            explain_artifacts,
            Some(error),
        );
    }

    (
        fix,
        if base_status == "clean" {
            "explain_trace_ready".to_string()
        } else {
            "explain_trace_ready_with_diagnostics".to_string()
        },
        EXIT_OK,
        explain_artifacts,
        None,
    )
}

fn run_frontend_fix(
    entry_path: &Path,
    output_format: OutputFormat,
) -> (FrontendFixSummary, String, i32) {
    let diagnostic_scope = DiagnosticScope::from_entrypoint(entry_path, true);
    let mut attempted = 0usize;
    let mut applied = 0usize;
    let mut touched_paths: BTreeSet<String> = BTreeSet::new();
    let mut any_fix_candidates = false;
    let mut had_apply_error = false;

    for _ in 0..FRONTEND_MAX_FIX_PASSES {
        let fixes = match collect_safe_fixes(entry_path, output_format, false, &diagnostic_scope) {
            Ok(fixes) => fixes,
            Err(code) => {
                if applied > 0 {
                    break;
                }
                let summary = FrontendFixSummary {
                    attempted,
                    applied,
                    skipped: attempted.saturating_sub(applied),
                    errors: 1,
                    touched_files: touched_paths.len(),
                };
                return (summary, "error".to_string(), code);
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
                eprintln!("frontend fix apply error: {}", err.message);
                break;
            }
        }
    }

    let summary = FrontendFixSummary {
        attempted,
        applied,
        skipped: attempted.saturating_sub(applied),
        errors: if had_apply_error { 1 } else { 0 },
        touched_files: touched_paths.len(),
    };

    if had_apply_error {
        return (summary, "apply_error".to_string(), EXIT_CODEGEN);
    }
    if !any_fix_candidates || applied == 0 {
        return (summary, "no_safe_fixes".to_string(), EXIT_TYPE);
    }
    (summary, "applied".to_string(), EXIT_OK)
}

pub(super) fn execute_frontend_pipeline_command(
    command: &str,
    path_arg: Option<String>,
    program_args: Vec<String>,
    output_format: OutputFormat,
) -> i32 {
    let Some(subcommand) = FrontendPipelineSubcommand::from_command(command) else {
        if matches!(output_format, OutputFormat::Json) {
            eprintln!(
                "{}",
                serde_json::json!({
                    "schema_version": FRONTEND_PIPELINE_SCHEMA_VERSION,
                    "kind": "wrela.frontend.error",
                    "message": format!("unsupported frontend subcommand: {command}"),
                })
            );
        } else {
            eprintln!("error: unsupported frontend subcommand: {command}");
        }
        return EXIT_USAGE;
    };

    if !program_args.is_empty() {
        emit_frontend_error(output_format, subcommand, "unexpected extra arguments");
        return EXIT_USAGE;
    }

    let frontend_path = match path_arg {
        Some(path) => path,
        None => {
            emit_frontend_error(
                output_format,
                subcommand,
                "missing frontend path (expected `wrela frontend <subcommand> <path>`)",
            );
            return EXIT_USAGE;
        }
    };

    let frontend_root_input = PathBuf::from(frontend_path.as_str());
    if !frontend_root_input.exists() {
        emit_frontend_error(
            output_format,
            subcommand,
            &format!("frontend path not found: {}", frontend_root_input.display()),
        );
        return EXIT_USAGE;
    }
    if !frontend_root_input.is_dir() {
        emit_frontend_error(
            output_format,
            subcommand,
            &format!(
                "frontend path must be a directory: {}",
                frontend_root_input.display()
            ),
        );
        return EXIT_USAGE;
    }

    let frontend_root = match fs::canonicalize(frontend_root_input.as_path()) {
        Ok(path) => path,
        Err(error) => {
            emit_frontend_error(
                output_format,
                subcommand,
                &format!(
                    "failed to canonicalize frontend path {}: {error}",
                    frontend_root_input.display()
                ),
            );
            return EXIT_USAGE;
        }
    };
    let frontend_path = frontend_root.display().to_string();

    let entry_path = match resolve_entry_path(Some(frontend_path.as_str())) {
        Ok(path) => path,
        Err(err) => {
            emit_frontend_error(output_format, subcommand, &err);
            return EXIT_USAGE;
        }
    };
    let entry_path = fs::canonicalize(entry_path.as_path()).unwrap_or(entry_path);

    let artifact_run_context = build_frontend_artifact_run_context(frontend_root.as_path());
    let artifact_path =
        frontend_summary_artifact_path(&frontend_root, subcommand, &artifact_run_context);
    let (fix, status, exit_code, preview_artifacts, explain_artifacts, error_message) =
        match subcommand {
            FrontendPipelineSubcommand::Preview => {
                let (fix, status, exit_code, preview_artifacts, error_message) =
                    run_frontend_preview(
                        frontend_root.as_path(),
                        &artifact_run_context,
                        entry_path.as_path(),
                        output_format,
                    );
                (
                    fix,
                    status,
                    exit_code,
                    preview_artifacts,
                    None,
                    error_message,
                )
            }
            FrontendPipelineSubcommand::Explain => {
                let (fix, status, exit_code, explain_artifacts, error_message) =
                    run_frontend_explain(
                        frontend_root.as_path(),
                        &artifact_run_context,
                        frontend_path.as_str(),
                        entry_path.as_path(),
                        output_format,
                    );
                (
                    fix,
                    status,
                    exit_code,
                    None,
                    explain_artifacts,
                    error_message,
                )
            }
            FrontendPipelineSubcommand::Fix => {
                let (fix, status, exit_code) =
                    run_frontend_fix(entry_path.as_path(), output_format);
                (fix, status, exit_code, None, None, None)
            }
        };

    let summary = build_frontend_summary(
        subcommand,
        &artifact_run_context,
        frontend_path,
        entry_path.display().to_string(),
        status,
        fix,
        preview_artifacts,
        explain_artifacts,
    );
    if let Err(message) = write_frontend_summary_artifact(artifact_path.as_path(), &summary) {
        emit_frontend_error(output_format, subcommand, message.as_str());
        return EXIT_CODEGEN;
    }

    emit_frontend_summary_event(output_format, subcommand, artifact_path.as_path(), &summary);
    if !matches!(output_format, OutputFormat::Json) {
        println!(
            "frontend {}: wrote summary artifact {}",
            subcommand.as_str(),
            artifact_path.display()
        );
    }

    if let Some(message) = error_message {
        emit_frontend_error(output_format, subcommand, message.as_str());
    }

    exit_code
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_run_context() -> FrontendArtifactRunContext {
        FrontendArtifactRunContext {
            app_slug: "app".to_string(),
            run_id: "run-1700000000000-1".to_string(),
            timestamp_epoch_seconds: 1_700_000_000,
        }
    }

    #[test]
    fn frontend_summary_artifact_path_is_run_scoped() {
        let root = PathBuf::from("/tmp/app");
        let run_context = test_run_context();
        assert_eq!(
            frontend_summary_artifact_path(
                &root,
                FrontendPipelineSubcommand::Preview,
                &run_context
            ),
            PathBuf::from(
                "/tmp/app/.artifacts/frontend-pipeline/preview/app/run-1700000000000-1/summary.json"
            )
        );
        assert_eq!(
            frontend_summary_artifact_path(
                &root,
                FrontendPipelineSubcommand::Explain,
                &run_context
            ),
            PathBuf::from(
                "/tmp/app/.artifacts/frontend-pipeline/explain/app/run-1700000000000-1/summary.json"
            )
        );
        assert_eq!(
            frontend_summary_artifact_path(&root, FrontendPipelineSubcommand::Fix, &run_context),
            PathBuf::from(
                "/tmp/app/.artifacts/frontend-pipeline/fix/app/run-1700000000000-1/summary.json"
            )
        );
    }

    #[test]
    fn frontend_preview_artifact_paths_are_run_scoped() {
        let root = PathBuf::from("/tmp/app");
        let run_context = test_run_context();
        let paths = frontend_preview_artifact_paths(&root, &run_context);
        assert_eq!(
            paths.artifact_root,
            PathBuf::from("/tmp/app/.artifacts/frontend-pipeline/preview/app/run-1700000000000-1")
        );
        assert_eq!(
            paths.preview_report_path,
            PathBuf::from(
                "/tmp/app/.artifacts/frontend-pipeline/preview/app/run-1700000000000-1/preview-report.json"
            )
        );
        assert_eq!(
            paths.interaction_log_path,
            PathBuf::from(
                "/tmp/app/.artifacts/frontend-pipeline/preview/app/run-1700000000000-1/interaction-log.json"
            )
        );
        assert_eq!(
            paths.assertion_summary_path,
            PathBuf::from(
                "/tmp/app/.artifacts/frontend-pipeline/preview/app/run-1700000000000-1/assertion-summary.json"
            )
        );
        assert_eq!(
            paths.screenshots_path,
            PathBuf::from(
                "/tmp/app/.artifacts/frontend-pipeline/preview/app/run-1700000000000-1/screenshots.json"
            )
        );
        assert_eq!(
            paths.console_error_summary_path,
            PathBuf::from(
                "/tmp/app/.artifacts/frontend-pipeline/preview/app/run-1700000000000-1/console-error-summary.json"
            )
        );
    }

    #[test]
    fn frontend_fix_summary_event_json_contract() {
        let summary = build_frontend_summary(
            FrontendPipelineSubcommand::Fix,
            &test_run_context(),
            "/tmp/app".to_string(),
            "/tmp/app/src/main.wr".to_string(),
            "applied".to_string(),
            FrontendFixSummary {
                attempted: 4,
                applied: 3,
                skipped: 1,
                errors: 0,
                touched_files: 2,
            },
            None,
            None,
        );
        let event = FrontendPipelineSummaryEvent {
            event: FrontendPipelineSubcommand::Fix.summary_event(),
            artifact_path:
                "/tmp/app/.artifacts/frontend-pipeline/fix/app/run-1700000000000-1/summary.json"
                    .to_string(),
            summary,
        };
        let value = serde_json::to_value(&event).expect("serialize event");
        assert_eq!(
            value.get("event").and_then(|v| v.as_str()),
            Some("frontend_fix_summary")
        );
        assert_eq!(
            value.get("artifact_path").and_then(|v| v.as_str()),
            Some("/tmp/app/.artifacts/frontend-pipeline/fix/app/run-1700000000000-1/summary.json")
        );
        assert_eq!(
            value
                .get("summary")
                .and_then(|v| v.get("kind"))
                .and_then(|v| v.as_str()),
            Some("wrela.frontend.fix.summary")
        );
        assert_eq!(
            value
                .get("summary")
                .and_then(|v| v.get("app_slug"))
                .and_then(|v| v.as_str()),
            Some("app")
        );
        assert_eq!(
            value
                .get("summary")
                .and_then(|v| v.get("run_id"))
                .and_then(|v| v.as_str()),
            Some("run-1700000000000-1")
        );
        assert_eq!(
            value
                .get("summary")
                .and_then(|v| v.get("fix"))
                .and_then(|v| v.get("applied"))
                .and_then(|v| v.as_u64()),
            Some(3)
        );
        assert!(
            value
                .get("summary")
                .and_then(|v| v.get("preview_artifacts"))
                .is_none()
        );
    }

    #[test]
    fn frontend_preview_summary_event_includes_preview_artifacts() {
        let run_context = test_run_context();
        let preview_paths = frontend_preview_artifact_paths(Path::new("/tmp/app"), &run_context);
        let summary = build_frontend_summary(
            FrontendPipelineSubcommand::Preview,
            &run_context,
            "/tmp/app".to_string(),
            "/tmp/app/src/main.wr".to_string(),
            "preview_passed".to_string(),
            FrontendFixSummary::default(),
            Some(preview_paths.as_summary_artifacts()),
            None,
        );
        let event = FrontendPipelineSummaryEvent {
            event: FrontendPipelineSubcommand::Preview.summary_event(),
            artifact_path:
                "/tmp/app/.artifacts/frontend-pipeline/preview/app/run-1700000000000-1/summary.json"
                    .to_string(),
            summary,
        };
        let value = serde_json::to_value(&event).expect("serialize event");
        assert_eq!(
            value
                .get("summary")
                .and_then(|v| v.get("preview_artifacts"))
                .and_then(|v| v.get("preview_report_path"))
                .and_then(|v| v.as_str()),
            Some(
                "/tmp/app/.artifacts/frontend-pipeline/preview/app/run-1700000000000-1/preview-report.json"
            )
        );
        assert_eq!(
            value
                .get("summary")
                .and_then(|v| v.get("preview_artifacts"))
                .and_then(|v| v.get("assertion_summary_path"))
                .and_then(|v| v.as_str()),
            Some(
                "/tmp/app/.artifacts/frontend-pipeline/preview/app/run-1700000000000-1/assertion-summary.json"
            )
        );
    }
}
