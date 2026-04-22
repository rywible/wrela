//! Owns CLI token parsing and parse-time legality checks for the `wrela`
//! command surface.
//! Does not own command execution, domain orchestration, or report rendering.
//!
//! Key invariants:
//! - free-form strings are accepted only at the CLI boundary, then converted to
//!   typed command, lane, and profile models.
//! - invalid flag/command combinations fail during parsing, before dispatch.
//! - parser output must preserve enough structure for dispatch to stop matching
//!   on command-name strings.
//!
//! Primary entrypoints:
//! - `parse`
//!
//! Failure modes / common pitfalls:
//! - adding a new flag without constraining its legal commands reintroduces the
//!   monolithic "bag of options" hazard.
//! - mixing rendering defaults into parsing makes it harder to keep machine and
//!   human output policies separate.

use super::contracts::OutputFormat;
use std::path::PathBuf;
use wrela::query_plan::DispatchBackend;

#[derive(Debug, Clone, PartialEq)]
pub enum ParsedCommandSpec {
    Help,
    Version,
    Ready(ParsedCommand),
    Error(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct CommandSpec {
    pub trace_enabled: bool,
    pub parsed: ParsedCommandSpec,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParsedTestLane {
    Spec,
    Integration,
    Sim,
    Model,
    Default,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParsedTestLanePreset {
    Fast,
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParsedTestLaneSelection {
    Single(ParsedTestLane),
    Preset(ParsedTestLanePreset),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ParsedTestSelection {
    pub list: bool,
    pub id: Option<String>,
    pub filter: Option<String>,
    pub lane: Option<ParsedTestLaneSelection>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParsedPerfProfile {
    Smoke,
    Standard,
    Deep,
    Closure1080p120,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InitCommandArgs {
    pub target: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UpdateCommandArgs {
    pub prefix_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CatalogCommandArgs {
    pub output_format: OutputFormat,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CollisionCommandArgs {
    pub output_format: OutputFormat,
    pub query_backend: DispatchBackend,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PresentationPlanCommandArgs {
    pub output_format: OutputFormat,
    pub path_arg: Option<String>,
    pub query_backend: DispatchBackend,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PresentationDebugOptions {
    pub view: Option<String>,
    pub region: Option<String>,
    pub domain: Option<String>,
    pub query_trace_solver_mode: wrela::query_exec::QueryTraceSolverMode,
    pub out_dir: Option<PathBuf>,
    pub skip_export: bool,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub camera_position: [f32; 3],
    pub camera_forward: [f32; 3],
    pub camera_up: [f32; 3],
    pub vertical_fov_degrees: f32,
    pub frame_index: u32,
    pub delta_seconds: f32,
    pub frames: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameAttachmentFormat {
    Json,
    Ppm,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PreviewCommandOptions {
    pub view: Option<String>,
    pub region: Option<String>,
    pub domain: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub camera_position: [f32; 3],
    pub camera_forward: [f32; 3],
    pub camera_up: [f32; 3],
    pub vertical_fov_degrees: f32,
    pub frame_index: u32,
    pub delta_seconds: f32,
    pub attachment: String,
    pub json_report: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PreviewCommandArgs {
    pub output_format: OutputFormat,
    pub path_arg: Option<String>,
    pub query_backend: DispatchBackend,
    pub options: PreviewCommandOptions,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FrameCommandOptions {
    pub view: Option<String>,
    pub region: Option<String>,
    pub domain: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub camera_position: [f32; 3],
    pub camera_forward: [f32; 3],
    pub camera_up: [f32; 3],
    pub vertical_fov_degrees: f32,
    pub frame_index: u32,
    pub delta_seconds: f32,
    pub attachments: Vec<String>,
    pub attachment_format: FrameAttachmentFormat,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FrameCommandArgs {
    pub output_format: OutputFormat,
    pub path_arg: Option<String>,
    pub query_backend: DispatchBackend,
    pub options: FrameCommandOptions,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FrameLiveCommandOptions {
    pub view: Option<String>,
    pub region: Option<String>,
    pub domain: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub camera_position: [f32; 3],
    pub camera_forward: [f32; 3],
    pub camera_up: [f32; 3],
    pub vertical_fov_degrees: f32,
    pub frame_index: u32,
    pub delta_seconds: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FrameLiveCommandArgs {
    pub output_format: OutputFormat,
    pub path_arg: Option<String>,
    pub query_backend: DispatchBackend,
    pub options: FrameLiveCommandOptions,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FrameContractsCommandArgs {
    pub output_format: OutputFormat,
    pub path_arg: Option<String>,
    pub query_backend: DispatchBackend,
    pub requested_view: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PresentationDebugCommandArgs {
    pub output_format: OutputFormat,
    pub path_arg: Option<String>,
    pub query_backend: DispatchBackend,
    pub options: PresentationDebugOptions,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AnalyzeCommandArgs {
    pub output_format: OutputFormat,
    pub emit_mir: bool,
    pub emit_mir_opt: bool,
    pub path_arg: Option<String>,
    pub strict_naming: bool,
    pub analysis_holes_only: bool,
    pub query_backend: DispatchBackend,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RewriteCommandArgs {
    pub output_format: OutputFormat,
    pub path_arg: Option<String>,
    pub fix_allow_review_fixes: bool,
    pub workspace_diagnostics: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BuildCommandArgs {
    pub output_format: OutputFormat,
    pub emit_mir: bool,
    pub emit_mir_opt: bool,
    pub emit_obj: Option<String>,
    pub emit_bin: Option<String>,
    pub out_path: Option<String>,
    pub path_arg: Option<String>,
    pub integration_mode: bool,
    pub test_jobs: Option<usize>,
    pub test_timeout_ms: Option<u64>,
    pub perf_debug: bool,
    pub strict_naming: bool,
    pub query_backend: DispatchBackend,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VerifyCertCommandArgs {
    pub cert_path: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RunCommandArgs {
    pub output_format: OutputFormat,
    pub emit_mir: bool,
    pub emit_mir_opt: bool,
    pub out_path: Option<String>,
    pub path_arg: Option<String>,
    pub program_args: Vec<String>,
    pub integration_mode: bool,
    pub strict_naming: bool,
    pub query_backend: DispatchBackend,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DevCommandArgs {
    pub output_format: OutputFormat,
    pub emit_mir: bool,
    pub emit_mir_opt: bool,
    pub path_arg: Option<String>,
    pub program_args: Vec<String>,
    pub poll_ms: Option<u64>,
    pub strict_naming: bool,
    pub query_backend: DispatchBackend,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TestCommandArgs {
    pub output_format: OutputFormat,
    pub out_path: Option<String>,
    pub emit_obj: Option<String>,
    pub emit_bin: Option<String>,
    pub path_arg: Option<String>,
    pub test_jobs: Option<usize>,
    pub test_timeout_ms: Option<u64>,
    pub test_record: bool,
    pub test_update_public_surface: bool,
    pub test_selection: ParsedTestSelection,
    pub repro_artifact_path: Option<String>,
    pub replay_trace_path: Option<String>,
    pub perf_debug: bool,
    pub perf_gate_path: Option<String>,
    pub perf_max_regression_pct: Option<f64>,
    pub kpi_check_fallback_max: Option<f64>,
    pub kpi_check_batch_min: Option<f64>,
    pub kpi_scheduler_p99_improve_min_pct: Option<f64>,
    pub kpi_rewrite_overhead_max_pct: Option<f64>,
    pub kpi_actor_throughput_improve_min_pct: Option<f64>,
    pub kpi_queue_age_p99_max_regress_pct: Option<f64>,
    pub kpi_starvation_violations_max: Option<f64>,
    pub kpi_scheduler_throughput_improve_min_pct: Option<f64>,
    pub kpi_scheduler_loop_p99_max_regress_pct: Option<f64>,
    pub kpi_scheduler_local_hit_min: Option<f64>,
    pub test_seed: Option<u64>,
    pub query_backend: DispatchBackend,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EvalCommandArgs {
    pub output_format: OutputFormat,
    pub path_arg: Option<String>,
    pub program_args: Vec<String>,
    pub runs: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PerfCommandArgs {
    pub output_format: OutputFormat,
    pub path_arg: Option<String>,
    pub perf_runs: Option<usize>,
    pub test_jobs: Option<usize>,
    pub test_timeout_ms: Option<u64>,
    pub benchmark_manifest_path: Option<String>,
    pub perf_profile: ParsedPerfProfile,
    pub perf_baseline_out: Option<String>,
    pub perf_gate_path: Option<String>,
    pub perf_max_regression_pct: Option<f64>,
    pub perf_cv_max_pct: Option<f64>,
    pub perf_why_not_120: bool,
    pub kpi_check_fallback_max: Option<f64>,
    pub kpi_check_batch_min: Option<f64>,
    pub kpi_scheduler_p99_improve_min_pct: Option<f64>,
    pub kpi_rewrite_overhead_max_pct: Option<f64>,
    pub kpi_actor_throughput_improve_min_pct: Option<f64>,
    pub kpi_queue_age_p99_max_regress_pct: Option<f64>,
    pub kpi_starvation_violations_max: Option<f64>,
    pub kpi_scheduler_throughput_improve_min_pct: Option<f64>,
    pub kpi_scheduler_loop_p99_max_regress_pct: Option<f64>,
    pub kpi_scheduler_local_hit_min: Option<f64>,
    pub perf_debug: bool,
    pub test_selection: ParsedTestSelection,
    pub query_backend: DispatchBackend,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PerfcmpCommandArgs {
    pub output_format: OutputFormat,
    pub path_arg: Option<String>,
    pub benchmark_manifest_path: Option<String>,
    pub perfcmp_baseline_ref: Option<String>,
    pub perfcmp_candidate_ref: Option<String>,
    pub out_path: Option<String>,
    pub perf_profile: ParsedPerfProfile,
    pub perfcmp_warmup_pairs: Option<usize>,
    pub perfcmp_measure_pairs: Option<usize>,
    pub perfcmp_min_effect_pct: Option<f64>,
    pub perfcmp_confidence_pct: Option<f64>,
    pub test_timeout_ms: Option<u64>,
    pub perf_debug: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatrixCommandArgs {
    pub path_arg: Option<String>,
    pub perf_runs: Option<usize>,
    pub perf_gate_path: Option<String>,
    pub perf_max_regression_pct: Option<f64>,
    pub kpi_check_fallback_max: Option<f64>,
    pub kpi_check_batch_min: Option<f64>,
    pub kpi_scheduler_p99_improve_min_pct: Option<f64>,
    pub kpi_rewrite_overhead_max_pct: Option<f64>,
    pub kpi_actor_throughput_improve_min_pct: Option<f64>,
    pub kpi_queue_age_p99_max_regress_pct: Option<f64>,
    pub kpi_starvation_violations_max: Option<f64>,
    pub kpi_scheduler_throughput_improve_min_pct: Option<f64>,
    pub kpi_scheduler_loop_p99_max_regress_pct: Option<f64>,
    pub kpi_scheduler_local_hit_min: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParsedCommand {
    Init(InitCommandArgs),
    Update(UpdateCommandArgs),
    QueryContracts(CatalogCommandArgs),
    CollisionContracts(CatalogCommandArgs),
    CollisionPlan(CollisionCommandArgs),
    CollisionRun(CollisionCommandArgs),
    Preview(PreviewCommandArgs),
    Frame(FrameCommandArgs),
    FrameLive(FrameLiveCommandArgs),
    FrameContracts(FrameContractsCommandArgs),
    PresentationPlan(PresentationPlanCommandArgs),
    PresentationDebug(PresentationDebugCommandArgs),
    Check(AnalyzeCommandArgs),
    Analyze(AnalyzeCommandArgs),
    Fix(RewriteCommandArgs),
    Fmt(RewriteCommandArgs),
    Build(BuildCommandArgs),
    Compile(BuildCommandArgs),
    VerifyCert(VerifyCertCommandArgs),
    Run(RunCommandArgs),
    Dev(DevCommandArgs),
    Test(TestCommandArgs),
    Eval(EvalCommandArgs),
    Perf(PerfCommandArgs),
    Perfcmp(PerfcmpCommandArgs),
    Matrix(MatrixCommandArgs),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommandName {
    Init,
    Update,
    QueryContracts,
    CollisionContracts,
    CollisionPlan,
    CollisionRun,
    Preview,
    Frame,
    FrameLive,
    FrameContracts,
    PresentationPlan,
    PresentationDebug,
    Check,
    Analyze,
    Fix,
    Fmt,
    Build,
    Compile,
    VerifyCert,
    Run,
    Dev,
    Test,
    Eval,
    Perf,
    Perfcmp,
    Matrix,
}

#[derive(Debug, Clone)]
struct ParseState {
    emit_mir: bool,
    emit_mir_opt: bool,
    emit_obj: Option<String>,
    emit_bin: Option<String>,
    out_path: Option<String>,
    prefix_path: Option<String>,
    query_backend: Option<DispatchBackend>,
    integration_mode: bool,
    path_arg: Option<String>,
    program_args: Vec<String>,
    poll_ms: Option<u64>,
    test_jobs: Option<usize>,
    test_timeout_ms: Option<u64>,
    test_record: bool,
    test_update_public_surface: bool,
    test_selection: ParsedTestSelection,
    test_seed: Option<u64>,
    repro_artifact_path: Option<String>,
    replay_trace_path: Option<String>,
    perf_debug: bool,
    perf_runs: Option<usize>,
    perf_baseline_out: Option<String>,
    perf_gate_path: Option<String>,
    perf_max_regression_pct: Option<f64>,
    perf_cv_max_pct: Option<f64>,
    perf_why_not_120: bool,
    kpi_check_fallback_max: Option<f64>,
    kpi_check_batch_min: Option<f64>,
    kpi_scheduler_p99_improve_min_pct: Option<f64>,
    kpi_rewrite_overhead_max_pct: Option<f64>,
    kpi_actor_throughput_improve_min_pct: Option<f64>,
    kpi_queue_age_p99_max_regress_pct: Option<f64>,
    kpi_starvation_violations_max: Option<f64>,
    kpi_scheduler_throughput_improve_min_pct: Option<f64>,
    kpi_scheduler_loop_p99_max_regress_pct: Option<f64>,
    kpi_scheduler_local_hit_min: Option<f64>,
    benchmark_manifest_path: Option<String>,
    perf_profile_name: Option<String>,
    perfcmp_baseline_ref: Option<String>,
    perfcmp_candidate_ref: Option<String>,
    perfcmp_warmup_pairs: Option<usize>,
    perfcmp_measure_pairs: Option<usize>,
    perfcmp_min_effect_pct: Option<f64>,
    perfcmp_confidence_pct: Option<f64>,
    analysis_holes_only: bool,
    strict_naming: bool,
    fix_allow_review_fixes: bool,
    workspace_diagnostics: bool,
    output_format: OutputFormat,
}

impl ParsedCommand {
    #[cfg(test)]
    pub fn command_name(&self) -> &'static str {
        match self {
            ParsedCommand::Init(_) => "init",
            ParsedCommand::Update(_) => "update",
            ParsedCommand::QueryContracts(_) => "query-contracts",
            ParsedCommand::CollisionContracts(_) => "collision-contracts",
            ParsedCommand::CollisionPlan(_) => "collision-plan",
            ParsedCommand::CollisionRun(_) => "collision-run",
            ParsedCommand::Preview(_) => "preview",
            ParsedCommand::Frame(_) => "frame",
            ParsedCommand::FrameLive(_) => "frame-live",
            ParsedCommand::FrameContracts(_) => "frame-contracts",
            ParsedCommand::PresentationPlan(_) => "presentation-plan",
            ParsedCommand::PresentationDebug(_) => "presentation-debug",
            ParsedCommand::Check(_) => "check",
            ParsedCommand::Analyze(_) => "analyze",
            ParsedCommand::Fix(_) => "fix",
            ParsedCommand::Fmt(_) => "fmt",
            ParsedCommand::Build(_) => "build",
            ParsedCommand::Compile(_) => "compile",
            ParsedCommand::VerifyCert(_) => "verify-cert",
            ParsedCommand::Run(_) => "run",
            ParsedCommand::Dev(_) => "dev",
            ParsedCommand::Test(_) => "test",
            ParsedCommand::Eval(_) => "eval",
            ParsedCommand::Perf(_) => "perf",
            ParsedCommand::Perfcmp(_) => "perfcmp",
            ParsedCommand::Matrix(_) => "matrix",
        }
    }
}

impl ParsedPerfProfile {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "smoke" => Some(Self::Smoke),
            "standard" => Some(Self::Standard),
            "deep" => Some(Self::Deep),
            "1080p120" => Some(Self::Closure1080p120),
            _ => None,
        }
    }
}

impl CommandName {
    fn as_str(self) -> &'static str {
        match self {
            Self::Init => "init",
            Self::Update => "update",
            Self::QueryContracts => "query-contracts",
            Self::CollisionContracts => "collision-contracts",
            Self::CollisionPlan => "collision-plan",
            Self::CollisionRun => "collision-run",
            Self::Preview => "preview",
            Self::Frame => "frame",
            Self::FrameLive => "frame-live",
            Self::FrameContracts => "frame-contracts",
            Self::PresentationPlan => "presentation-plan",
            Self::PresentationDebug => "presentation-debug",
            Self::Check => "check",
            Self::Analyze => "analyze",
            Self::Fix => "fix",
            Self::Fmt => "fmt",
            Self::Build => "build",
            Self::Compile => "compile",
            Self::VerifyCert => "verify-cert",
            Self::Run => "run",
            Self::Dev => "dev",
            Self::Test => "test",
            Self::Eval => "eval",
            Self::Perf => "perf",
            Self::Perfcmp => "perfcmp",
            Self::Matrix => "matrix",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "init" => Some(Self::Init),
            "update" => Some(Self::Update),
            "check" => Some(Self::Check),
            "analyze" => Some(Self::Analyze),
            "fix" => Some(Self::Fix),
            "fmt" => Some(Self::Fmt),
            "build" => Some(Self::Build),
            "compile" => Some(Self::Compile),
            "query-contracts" => Some(Self::QueryContracts),
            "collision-contracts" => Some(Self::CollisionContracts),
            "collision-plan" => Some(Self::CollisionPlan),
            "collision-run" => Some(Self::CollisionRun),
            "preview" => Some(Self::Preview),
            "frame" => Some(Self::Frame),
            "frame-live" => Some(Self::FrameLive),
            "frame-contracts" => Some(Self::FrameContracts),
            "presentation-plan" => Some(Self::PresentationPlan),
            "presentation-debug" => Some(Self::PresentationDebug),
            "verify-cert" => Some(Self::VerifyCert),
            "run" => Some(Self::Run),
            "dev" => Some(Self::Dev),
            "test" => Some(Self::Test),
            "eval" => Some(Self::Eval),
            "perf" => Some(Self::Perf),
            "perfcmp" => Some(Self::Perfcmp),
            "matrix" => Some(Self::Matrix),
            _ => None,
        }
    }
}

impl ParseState {
    fn query_backend_or_auto(&self) -> DispatchBackend {
        self.query_backend.unwrap_or(DispatchBackend::Auto)
    }
}

fn build_parsed_command(command: CommandName, state: ParseState) -> Result<ParsedCommand, String> {
    let test_filters_requested = state.test_selection.list
        || state.test_selection.id.is_some()
        || state.test_selection.filter.is_some()
        || state.test_selection.lane.is_some();
    let kpi_thresholds_requested = [
        state.kpi_check_fallback_max,
        state.kpi_check_batch_min,
        state.kpi_scheduler_p99_improve_min_pct,
        state.kpi_rewrite_overhead_max_pct,
        state.kpi_actor_throughput_improve_min_pct,
        state.kpi_queue_age_p99_max_regress_pct,
        state.kpi_starvation_violations_max,
        state.kpi_scheduler_throughput_improve_min_pct,
        state.kpi_scheduler_loop_p99_max_regress_pct,
        state.kpi_scheduler_local_hit_min,
    ]
    .into_iter()
    .any(|value| value.is_some());

    if !matches!(
        command,
        CommandName::Preview
            | CommandName::Frame
            | CommandName::FrameLive
            | CommandName::FrameContracts
            | CommandName::PresentationPlan
            | CommandName::PresentationDebug
            | CommandName::Run
            | CommandName::Dev
            | CommandName::Eval
    ) && !state.program_args.is_empty()
    {
        return Err("error: unexpected extra arguments".to_string());
    }

    if matches!(
        command,
        CommandName::Update
            | CommandName::QueryContracts
            | CommandName::CollisionContracts
            | CommandName::CollisionPlan
            | CommandName::CollisionRun
    ) && state.path_arg.is_some()
    {
        return Err(format!("error: {} does not take a path", command.as_str()));
    }

    if !matches!(
        command,
        CommandName::Check
            | CommandName::Analyze
            | CommandName::Build
            | CommandName::Compile
            | CommandName::Run
            | CommandName::Dev
    ) && (state.emit_mir || state.emit_mir_opt)
    {
        return Err(format!(
            "error: --emit-mir and --emit-mir-opt are only valid with `wrela {}`, `wrela {}`, `wrela {}`, `wrela {}`, `wrela {}`, or `wrela {}`",
            CommandName::Check.as_str(),
            CommandName::Analyze.as_str(),
            CommandName::Build.as_str(),
            CommandName::Compile.as_str(),
            CommandName::Run.as_str(),
            CommandName::Dev.as_str(),
        ));
    }

    if !matches!(
        command,
        CommandName::Build | CommandName::Compile | CommandName::Test
    ) && (state.emit_obj.is_some() || state.emit_bin.is_some())
    {
        return Err(format!(
            "error: --emit-obj and --emit-bin are only valid with `wrela {}`, `wrela {}`, or `wrela {}`",
            CommandName::Build.as_str(),
            CommandName::Compile.as_str(),
            CommandName::Test.as_str(),
        ));
    }

    if !matches!(
        command,
        CommandName::Build
            | CommandName::Compile
            | CommandName::Run
            | CommandName::Test
            | CommandName::Perfcmp
    ) && state.out_path.is_some()
    {
        return Err(format!(
            "error: -o/--out is only valid with `wrela {}`, `wrela {}`, `wrela {}`, `wrela {}`, or `wrela {}`",
            CommandName::Build.as_str(),
            CommandName::Compile.as_str(),
            CommandName::Run.as_str(),
            CommandName::Test.as_str(),
            CommandName::Perfcmp.as_str(),
        ));
    }

    if command != CommandName::Update && state.prefix_path.is_some() {
        return Err("error: --prefix is only valid with `wrela update`".to_string());
    }

    if !matches!(
        command,
        CommandName::CollisionPlan
            | CommandName::CollisionRun
            | CommandName::Preview
            | CommandName::Frame
            | CommandName::FrameContracts
            | CommandName::PresentationPlan
            | CommandName::PresentationDebug
            | CommandName::Check
            | CommandName::Analyze
            | CommandName::Build
            | CommandName::Compile
            | CommandName::Run
            | CommandName::Dev
            | CommandName::Test
            | CommandName::Perf
    ) && state.query_backend.is_some()
    {
        return Err(format!(
            "error: --query-backend is not valid with `wrela {}`",
            command.as_str()
        ));
    }

    if command != CommandName::Dev && state.poll_ms.is_some() {
        return Err("error: --poll-ms is only valid with `wrela dev`".to_string());
    }

    if !matches!(
        command,
        CommandName::Build | CommandName::Compile | CommandName::Test | CommandName::Perf
    ) && state.test_jobs.is_some()
    {
        return Err("error: --jobs is only valid with `wrela build`, `wrela compile`, `wrela test`, or `wrela perf`".to_string());
    }

    if !matches!(
        command,
        CommandName::Build
            | CommandName::Compile
            | CommandName::Test
            | CommandName::Perf
            | CommandName::Perfcmp
    ) && state.test_timeout_ms.is_some()
    {
        return Err("error: --test-timeout-ms is only valid with `wrela build`, `wrela compile`, `wrela test`, `wrela perf`, or `wrela perfcmp`".to_string());
    }

    if command != CommandName::Test && (state.test_record || state.test_update_public_surface) {
        return Err(
            "error: --record and --update-public-surface are only valid with `wrela test`"
                .to_string(),
        );
    }

    if !matches!(
        command,
        CommandName::Build | CommandName::Compile | CommandName::Run
    ) && state.integration_mode
    {
        return Err(
            "error: --integration-mode is only valid with `wrela run`, `wrela build`, or `wrela compile`"
                .to_string(),
        );
    }

    if !matches!(command, CommandName::Test | CommandName::Perf) && test_filters_requested {
        return Err("error: --list, --id, --filter, and --lane are only valid with `wrela test` or `wrela perf`".to_string());
    }

    if !matches!(command, CommandName::Test | CommandName::Perf) && state.test_seed.is_some() {
        return Err("error: --seed is only valid with `wrela test` or `wrela perf`".to_string());
    }

    if command != CommandName::Test && state.repro_artifact_path.is_some() {
        return Err("error: --repro is only valid with `wrela test`".to_string());
    }

    if command != CommandName::Test && state.replay_trace_path.is_some() {
        return Err("error: --replay-trace is only valid with `wrela test`".to_string());
    }

    if !matches!(
        command,
        CommandName::Build
            | CommandName::Compile
            | CommandName::Test
            | CommandName::Perf
            | CommandName::Perfcmp
    ) && state.perf_debug
    {
        return Err("error: --perf-debug is only valid with `wrela build`, `wrela compile`, `wrela test`, `wrela perf`, or `wrela perfcmp`".to_string());
    }

    if !matches!(
        command,
        CommandName::Eval | CommandName::Perf | CommandName::Matrix
    ) && state.perf_runs.is_some()
    {
        return Err(
            "error: --runs is only valid with `wrela eval`, `wrela perf`, or `wrela matrix`"
                .to_string(),
        );
    }

    if command != CommandName::Perf && state.perf_baseline_out.is_some() {
        return Err("error: --baseline-out is only valid with `wrela perf`".to_string());
    }

    if !matches!(
        command,
        CommandName::Test | CommandName::Perf | CommandName::Matrix
    ) && (state.perf_gate_path.is_some()
        || state.perf_max_regression_pct.is_some()
        || kpi_thresholds_requested)
    {
        return Err("error: perf gates and KPI thresholds are only valid with `wrela test`, `wrela perf`, or `wrela matrix`".to_string());
    }

    if command != CommandName::Perf && state.perf_cv_max_pct.is_some() {
        return Err("error: --perf-cv-max-pct is only valid with `wrela perf`".to_string());
    }

    if command != CommandName::Perf && state.perf_why_not_120 {
        return Err("error: --why-not-120 is only valid with `wrela perf`".to_string());
    }

    if !matches!(command, CommandName::Perf | CommandName::Perfcmp)
        && state.benchmark_manifest_path.is_some()
    {
        return Err(
            "error: --benchmark-manifest is only valid with `wrela perf` or `wrela perfcmp`"
                .to_string(),
        );
    }

    if !matches!(command, CommandName::Perf | CommandName::Perfcmp)
        && state.perf_profile_name.is_some()
    {
        return Err(
            "error: --profile is only valid with `wrela perf` or `wrela perfcmp`".to_string(),
        );
    }

    if command != CommandName::Perfcmp
        && (state.perfcmp_baseline_ref.is_some()
            || state.perfcmp_candidate_ref.is_some()
            || state.perfcmp_warmup_pairs.is_some()
            || state.perfcmp_measure_pairs.is_some()
            || state.perfcmp_min_effect_pct.is_some()
            || state.perfcmp_confidence_pct.is_some())
    {
        return Err("error: --baseline-ref, --candidate-ref, --warmup-pairs, --measure-pairs, --min-effect-pct, and --confidence are only valid with `wrela perfcmp`".to_string());
    }

    if !matches!(command, CommandName::Check | CommandName::Analyze) && state.analysis_holes_only {
        return Err(
            "error: --holes-only is only valid with `wrela check` or `wrela analyze`".to_string(),
        );
    }

    if !matches!(
        command,
        CommandName::Check
            | CommandName::Analyze
            | CommandName::Build
            | CommandName::Compile
            | CommandName::Run
            | CommandName::Dev
    ) && state.strict_naming
    {
        return Err("error: --strict-naming is only valid with `wrela check`, `wrela analyze`, `wrela build`, `wrela compile`, `wrela run`, or `wrela dev`".to_string());
    }

    if !matches!(command, CommandName::Fix | CommandName::Fmt) && state.fix_allow_review_fixes {
        return Err(
            "error: --allow-review-fixes is only valid with `wrela fix` or `wrela fmt`".to_string(),
        );
    }

    if !matches!(command, CommandName::Fix | CommandName::Fmt) && state.workspace_diagnostics {
        return Err(
            "error: --workspace-diagnostics is only valid with `wrela fix` or `wrela fmt`"
                .to_string(),
        );
    }

    let perf_profile = if matches!(command, CommandName::Perf | CommandName::Perfcmp) {
        ParsedPerfProfile::parse(state.perf_profile_name.as_deref().unwrap_or("standard"))
            .ok_or_else(|| {
                "error: invalid --profile value (expected smoke|standard|deep|1080p120)".to_string()
            })?
    } else {
        ParsedPerfProfile::Standard
    };

    let query_backend = state.query_backend_or_auto();

    Ok(match command {
        CommandName::Init => ParsedCommand::Init(InitCommandArgs {
            target: state.path_arg,
        }),
        CommandName::Update => ParsedCommand::Update(UpdateCommandArgs {
            prefix_path: state.prefix_path,
        }),
        CommandName::QueryContracts => ParsedCommand::QueryContracts(CatalogCommandArgs {
            output_format: state.output_format,
        }),
        CommandName::CollisionContracts => ParsedCommand::CollisionContracts(CatalogCommandArgs {
            output_format: state.output_format,
        }),
        CommandName::CollisionPlan => ParsedCommand::CollisionPlan(CollisionCommandArgs {
            output_format: state.output_format,
            query_backend,
        }),
        CommandName::CollisionRun => ParsedCommand::CollisionRun(CollisionCommandArgs {
            output_format: state.output_format,
            query_backend: normalize_collision_run_backend(query_backend)?,
        }),
        CommandName::Preview => {
            let options = parse_preview_command_options(&state.program_args)?;
            validate_preview_command_args(state.output_format, &options)?;
            ParsedCommand::Preview(PreviewCommandArgs {
                output_format: state.output_format,
                path_arg: state.path_arg,
                query_backend,
                options,
            })
        }
        CommandName::Frame => {
            let options = parse_frame_command_options(&state.program_args)?;
            validate_frame_command_args(state.output_format, &options)?;
            ParsedCommand::Frame(FrameCommandArgs {
                output_format: state.output_format,
                path_arg: state.path_arg,
                query_backend,
                options,
            })
        }
        CommandName::FrameLive => ParsedCommand::FrameLive(FrameLiveCommandArgs {
            output_format: state.output_format,
            path_arg: state.path_arg,
            query_backend,
            options: parse_frame_live_command_options(&state.program_args)?,
        }),
        CommandName::FrameContracts => ParsedCommand::FrameContracts(FrameContractsCommandArgs {
            output_format: state.output_format,
            path_arg: state.path_arg,
            query_backend,
            requested_view: parse_frame_contracts_view(&state.program_args)?,
        }),
        CommandName::PresentationPlan => {
            if !state.program_args.is_empty() {
                return Err("error: unexpected extra arguments".to_string());
            }
            ParsedCommand::PresentationPlan(PresentationPlanCommandArgs {
                output_format: state.output_format,
                path_arg: state.path_arg,
                query_backend,
            })
        }
        CommandName::PresentationDebug => {
            ParsedCommand::PresentationDebug(PresentationDebugCommandArgs {
                output_format: state.output_format,
                path_arg: state.path_arg,
                query_backend,
                options: parse_presentation_debug_options(&state.program_args)?,
            })
        }
        CommandName::Check => ParsedCommand::Check(AnalyzeCommandArgs {
            output_format: state.output_format,
            emit_mir: state.emit_mir,
            emit_mir_opt: state.emit_mir_opt,
            path_arg: state.path_arg,
            strict_naming: state.strict_naming,
            analysis_holes_only: state.analysis_holes_only,
            query_backend,
        }),
        CommandName::Analyze => ParsedCommand::Analyze(AnalyzeCommandArgs {
            output_format: state.output_format,
            emit_mir: state.emit_mir,
            emit_mir_opt: state.emit_mir_opt,
            path_arg: state.path_arg,
            strict_naming: state.strict_naming,
            analysis_holes_only: state.analysis_holes_only,
            query_backend,
        }),
        CommandName::Fix => ParsedCommand::Fix(RewriteCommandArgs {
            output_format: state.output_format,
            path_arg: state.path_arg,
            fix_allow_review_fixes: state.fix_allow_review_fixes,
            workspace_diagnostics: state.workspace_diagnostics,
        }),
        CommandName::Fmt => ParsedCommand::Fmt(RewriteCommandArgs {
            output_format: state.output_format,
            path_arg: state.path_arg,
            fix_allow_review_fixes: state.fix_allow_review_fixes,
            workspace_diagnostics: state.workspace_diagnostics,
        }),
        CommandName::Build => ParsedCommand::Build(BuildCommandArgs {
            output_format: state.output_format,
            emit_mir: state.emit_mir,
            emit_mir_opt: state.emit_mir_opt,
            emit_obj: state.emit_obj,
            emit_bin: state.emit_bin,
            out_path: state.out_path,
            path_arg: state.path_arg,
            integration_mode: state.integration_mode,
            test_jobs: state.test_jobs,
            test_timeout_ms: state.test_timeout_ms,
            perf_debug: state.perf_debug,
            strict_naming: state.strict_naming,
            query_backend,
        }),
        CommandName::Compile => ParsedCommand::Compile(BuildCommandArgs {
            output_format: state.output_format,
            emit_mir: state.emit_mir,
            emit_mir_opt: state.emit_mir_opt,
            emit_obj: state.emit_obj,
            emit_bin: state.emit_bin,
            out_path: state.out_path,
            path_arg: state.path_arg,
            integration_mode: state.integration_mode,
            test_jobs: state.test_jobs,
            test_timeout_ms: state.test_timeout_ms,
            perf_debug: state.perf_debug,
            strict_naming: state.strict_naming,
            query_backend,
        }),
        CommandName::VerifyCert => ParsedCommand::VerifyCert(VerifyCertCommandArgs {
            cert_path: state
                .path_arg
                .ok_or_else(|| "error: missing cert path".to_string())?,
        }),
        CommandName::Run => ParsedCommand::Run(RunCommandArgs {
            output_format: state.output_format,
            emit_mir: state.emit_mir,
            emit_mir_opt: state.emit_mir_opt,
            out_path: state.out_path,
            path_arg: state.path_arg,
            program_args: state.program_args,
            integration_mode: state.integration_mode,
            strict_naming: state.strict_naming,
            query_backend,
        }),
        CommandName::Dev => ParsedCommand::Dev(DevCommandArgs {
            output_format: state.output_format,
            emit_mir: state.emit_mir,
            emit_mir_opt: state.emit_mir_opt,
            path_arg: state.path_arg,
            program_args: state.program_args,
            poll_ms: state.poll_ms,
            strict_naming: state.strict_naming,
            query_backend,
        }),
        CommandName::Test => ParsedCommand::Test(TestCommandArgs {
            output_format: state.output_format,
            out_path: state.out_path,
            emit_obj: state.emit_obj,
            emit_bin: state.emit_bin,
            path_arg: state.path_arg,
            test_jobs: state.test_jobs,
            test_timeout_ms: state.test_timeout_ms,
            test_record: state.test_record,
            test_update_public_surface: state.test_update_public_surface,
            test_selection: state.test_selection,
            repro_artifact_path: state.repro_artifact_path,
            replay_trace_path: state.replay_trace_path,
            perf_debug: state.perf_debug,
            perf_gate_path: state.perf_gate_path,
            perf_max_regression_pct: state.perf_max_regression_pct,
            kpi_check_fallback_max: state.kpi_check_fallback_max,
            kpi_check_batch_min: state.kpi_check_batch_min,
            kpi_scheduler_p99_improve_min_pct: state.kpi_scheduler_p99_improve_min_pct,
            kpi_rewrite_overhead_max_pct: state.kpi_rewrite_overhead_max_pct,
            kpi_actor_throughput_improve_min_pct: state.kpi_actor_throughput_improve_min_pct,
            kpi_queue_age_p99_max_regress_pct: state.kpi_queue_age_p99_max_regress_pct,
            kpi_starvation_violations_max: state.kpi_starvation_violations_max,
            kpi_scheduler_throughput_improve_min_pct: state
                .kpi_scheduler_throughput_improve_min_pct,
            kpi_scheduler_loop_p99_max_regress_pct: state.kpi_scheduler_loop_p99_max_regress_pct,
            kpi_scheduler_local_hit_min: state.kpi_scheduler_local_hit_min,
            test_seed: state.test_seed,
            query_backend,
        }),
        CommandName::Eval => ParsedCommand::Eval(EvalCommandArgs {
            output_format: state.output_format,
            path_arg: state.path_arg,
            program_args: state.program_args,
            runs: state.perf_runs,
        }),
        CommandName::Perf => ParsedCommand::Perf(PerfCommandArgs {
            output_format: state.output_format,
            path_arg: state.path_arg,
            perf_runs: state.perf_runs,
            test_jobs: state.test_jobs,
            test_timeout_ms: state.test_timeout_ms,
            benchmark_manifest_path: state.benchmark_manifest_path,
            perf_profile,
            perf_baseline_out: state.perf_baseline_out,
            perf_gate_path: state.perf_gate_path,
            perf_max_regression_pct: state.perf_max_regression_pct,
            perf_cv_max_pct: state.perf_cv_max_pct,
            perf_why_not_120: state.perf_why_not_120,
            kpi_check_fallback_max: state.kpi_check_fallback_max,
            kpi_check_batch_min: state.kpi_check_batch_min,
            kpi_scheduler_p99_improve_min_pct: state.kpi_scheduler_p99_improve_min_pct,
            kpi_rewrite_overhead_max_pct: state.kpi_rewrite_overhead_max_pct,
            kpi_actor_throughput_improve_min_pct: state.kpi_actor_throughput_improve_min_pct,
            kpi_queue_age_p99_max_regress_pct: state.kpi_queue_age_p99_max_regress_pct,
            kpi_starvation_violations_max: state.kpi_starvation_violations_max,
            kpi_scheduler_throughput_improve_min_pct: state
                .kpi_scheduler_throughput_improve_min_pct,
            kpi_scheduler_loop_p99_max_regress_pct: state.kpi_scheduler_loop_p99_max_regress_pct,
            kpi_scheduler_local_hit_min: state.kpi_scheduler_local_hit_min,
            perf_debug: state.perf_debug,
            test_selection: state.test_selection,
            query_backend,
        }),
        CommandName::Perfcmp => ParsedCommand::Perfcmp(PerfcmpCommandArgs {
            output_format: state.output_format,
            path_arg: state.path_arg,
            benchmark_manifest_path: state.benchmark_manifest_path,
            perfcmp_baseline_ref: state.perfcmp_baseline_ref,
            perfcmp_candidate_ref: state.perfcmp_candidate_ref,
            out_path: state.out_path,
            perf_profile,
            perfcmp_warmup_pairs: state.perfcmp_warmup_pairs,
            perfcmp_measure_pairs: state.perfcmp_measure_pairs,
            perfcmp_min_effect_pct: state.perfcmp_min_effect_pct,
            perfcmp_confidence_pct: state.perfcmp_confidence_pct,
            test_timeout_ms: state.test_timeout_ms,
            perf_debug: state.perf_debug,
        }),
        CommandName::Matrix => ParsedCommand::Matrix(MatrixCommandArgs {
            path_arg: state.path_arg,
            perf_runs: state.perf_runs,
            perf_gate_path: state.perf_gate_path,
            perf_max_regression_pct: state.perf_max_regression_pct,
            kpi_check_fallback_max: state.kpi_check_fallback_max,
            kpi_check_batch_min: state.kpi_check_batch_min,
            kpi_scheduler_p99_improve_min_pct: state.kpi_scheduler_p99_improve_min_pct,
            kpi_rewrite_overhead_max_pct: state.kpi_rewrite_overhead_max_pct,
            kpi_actor_throughput_improve_min_pct: state.kpi_actor_throughput_improve_min_pct,
            kpi_queue_age_p99_max_regress_pct: state.kpi_queue_age_p99_max_regress_pct,
            kpi_starvation_violations_max: state.kpi_starvation_violations_max,
            kpi_scheduler_throughput_improve_min_pct: state
                .kpi_scheduler_throughput_improve_min_pct,
            kpi_scheduler_loop_p99_max_regress_pct: state.kpi_scheduler_loop_p99_max_regress_pct,
            kpi_scheduler_local_hit_min: state.kpi_scheduler_local_hit_min,
        }),
    })
}

fn apply_output_format_flag(
    flag: &str,
    fmt: &str,
    output_format: &mut OutputFormat,
) -> Result<(), String> {
    *output_format = match fmt {
        "human" => OutputFormat::Pretty,
        "json" => OutputFormat::Json,
        "sarif" => OutputFormat::Sarif,
        _ => {
            return Err(format!(
                "error: invalid {flag} value `{fmt}` (expected one of: human, json, sarif)"
            ));
        }
    };
    Ok(())
}

fn parse_test_lane_flag(value: &str) -> Option<ParsedTestLaneSelection> {
    match value {
        "fast" => Some(ParsedTestLaneSelection::Preset(ParsedTestLanePreset::Fast)),
        "full" => Some(ParsedTestLaneSelection::Preset(ParsedTestLanePreset::Full)),
        "spec" => Some(ParsedTestLaneSelection::Single(ParsedTestLane::Spec)),
        "integration" => Some(ParsedTestLaneSelection::Single(ParsedTestLane::Integration)),
        "sim" => Some(ParsedTestLaneSelection::Single(ParsedTestLane::Sim)),
        "model" => Some(ParsedTestLaneSelection::Single(ParsedTestLane::Model)),
        "default" => Some(ParsedTestLaneSelection::Single(ParsedTestLane::Default)),
        _ => None,
    }
}

fn parse_query_backend_flag(value: &str) -> Result<DispatchBackend, String> {
    match value {
        "cpu" => Ok(DispatchBackend::Cpu),
        "virtual_gpu" => Ok(DispatchBackend::VirtualGpu),
        "wgsl" => Ok(DispatchBackend::Wgsl),
        "auto" => Ok(DispatchBackend::Auto),
        _ => Err(format!(
            "error: invalid --query-backend value `{value}` (expected one of: cpu, virtual_gpu, wgsl, auto)"
        )),
    }
}

fn parse_query_trace_solver_mode(
    value: &str,
) -> Result<wrela::query_exec::QueryTraceSolverMode, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "hybrid" => Ok(wrela::query_exec::QueryTraceSolverMode::Hybrid),
        "dense-only" | "dense_only" => Ok(wrela::query_exec::QueryTraceSolverMode::DenseOnly),
        _ => Err(format!(
            "invalid --solver-mode value `{value}`; expected `hybrid` or `dense-only`"
        )),
    }
}

fn parse_presentation_debug_options(args: &[String]) -> Result<PresentationDebugOptions, String> {
    let mut options = PresentationDebugOptions {
        view: None,
        region: None,
        domain: None,
        query_trace_solver_mode: wrela::query_exec::QueryTraceSolverMode::Hybrid,
        out_dir: None,
        skip_export: false,
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
            "--solver-mode" => {
                let mode = take_value(&inline_value, args, &mut index)?;
                options.query_trace_solver_mode = parse_query_trace_solver_mode(&mode)?;
            }
            "--out-dir" => {
                options.out_dir = Some(PathBuf::from(take_value(&inline_value, args, &mut index)?))
            }
            "--no-export" => options.skip_export = true,
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
            "--attachment" => options.attachment = take_value(&inline_value, args, &mut index)?,
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
            "--attachment" => {
                options
                    .attachments
                    .push(take_value(&inline_value, args, &mut index)?)
            }
            "--attachment-format" => {
                options.attachment_format =
                    match take_value(&inline_value, args, &mut index)?.as_str() {
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

fn parse_frame_live_command_options(args: &[String]) -> Result<FrameLiveCommandOptions, String> {
    let mut options = FrameLiveCommandOptions {
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
            _ => return Err(format!("unexpected argument `{arg}`")),
        }
        index += 1;
    }
    Ok(options)
}

fn validate_preview_command_args(
    output_format: OutputFormat,
    options: &PreviewCommandOptions,
) -> Result<(), String> {
    if matches!(output_format, OutputFormat::Json) && !options.json_report {
        return Err(
            "error: `preview --json` requires `--json-report`; use `frame --json` for typed attachment bundles"
                .to_string(),
        );
    }
    Ok(())
}

fn validate_frame_command_args(
    output_format: OutputFormat,
    options: &FrameCommandOptions,
) -> Result<(), String> {
    if options.attachment_format != FrameAttachmentFormat::Ppm {
        return Ok(());
    }
    if matches!(output_format, OutputFormat::Json) {
        return Err(
            "error: `frame --json` cannot be combined with `--attachment-format=ppm`".to_string(),
        );
    }
    if options.attachments.len() != 1 {
        return Err(
            "error: `frame --attachment-format=ppm` requires exactly one selected attachment"
                .to_string(),
        );
    }
    Ok(())
}

fn normalize_collision_run_backend(
    query_backend: DispatchBackend,
) -> Result<DispatchBackend, String> {
    match query_backend {
        DispatchBackend::Cpu | DispatchBackend::Auto => Ok(DispatchBackend::Cpu),
        other => Err(format!(
            "error: collision-run only supports cpu or auto query backends, not {:?}",
            other
        )),
    }
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

pub fn parse(raw_args: Vec<String>) -> CommandSpec {
    let trace_enabled = std::env::var("WRELA_BUILD_TRACE").is_ok();
    if raw_args.first().is_some_and(|arg| arg == "help") {
        return CommandSpec {
            trace_enabled,
            parsed: ParsedCommandSpec::Help,
        };
    }
    if raw_args.iter().any(|arg| arg == "--help" || arg == "-h") {
        return CommandSpec {
            trace_enabled,
            parsed: ParsedCommandSpec::Help,
        };
    }
    if raw_args.iter().any(|arg| arg == "--version" || arg == "-V") {
        return CommandSpec {
            trace_enabled,
            parsed: ParsedCommandSpec::Version,
        };
    }

    let mut emit_mir = false;
    let mut emit_mir_opt = false;
    let mut emit_obj: Option<String> = None;
    let mut emit_bin: Option<String> = None;
    let mut out_path: Option<String> = None;
    let mut prefix_path: Option<String> = None;
    let mut query_backend: Option<DispatchBackend> = None;
    let mut command: Option<CommandName> = None;
    let mut integration_mode = false;
    let mut path_arg: Option<String> = None;
    let mut program_args: Vec<String> = Vec::new();
    let mut poll_ms: Option<u64> = None;
    let mut test_jobs: Option<usize> = None;
    let mut test_timeout_ms: Option<u64> = None;
    let mut test_record = false;
    let mut test_update_public_surface = false;
    let mut test_selection = ParsedTestSelection::default();
    let mut test_seed: Option<u64> = None;
    let mut repro_artifact_path: Option<String> = None;
    let mut replay_trace_path: Option<String> = None;
    let mut perf_debug = false;
    let mut perf_runs: Option<usize> = None;
    let mut perf_baseline_out: Option<String> = None;
    let mut perf_gate_path: Option<String> = None;
    let mut perf_max_regression_pct: Option<f64> = None;
    let mut perf_cv_max_pct: Option<f64> = None;
    let mut perf_why_not_120 = false;
    let mut kpi_check_fallback_max: Option<f64> = None;
    let mut kpi_check_batch_min: Option<f64> = None;
    let mut kpi_scheduler_p99_improve_min_pct: Option<f64> = None;
    let mut kpi_rewrite_overhead_max_pct: Option<f64> = None;
    let mut kpi_actor_throughput_improve_min_pct: Option<f64> = None;
    let mut kpi_queue_age_p99_max_regress_pct: Option<f64> = None;
    let mut kpi_starvation_violations_max: Option<f64> = None;
    let mut kpi_scheduler_throughput_improve_min_pct: Option<f64> = None;
    let mut kpi_scheduler_loop_p99_max_regress_pct: Option<f64> = None;
    let mut kpi_scheduler_local_hit_min: Option<f64> = None;
    let mut benchmark_manifest_path: Option<String> = None;
    let mut perf_profile_name: Option<String> = None;
    let mut perfcmp_baseline_ref: Option<String> = None;
    let mut perfcmp_candidate_ref: Option<String> = None;
    let mut perfcmp_warmup_pairs: Option<usize> = None;
    let mut perfcmp_measure_pairs: Option<usize> = None;
    let mut perfcmp_min_effect_pct: Option<f64> = None;
    let mut perfcmp_confidence_pct: Option<f64> = None;
    let mut analysis_holes_only = false;
    let mut strict_naming = false;
    let mut fix_allow_review_fixes = false;
    let mut workspace_diagnostics = false;
    let mut output_format = OutputFormat::Pretty;
    let mut seen_double_dash = false;

    let mut iter = raw_args.into_iter();
    while let Some(arg) = iter.next() {
        if seen_double_dash {
            program_args.push(arg);
            continue;
        }
        if arg == "--" {
            seen_double_dash = true;
            continue;
        }
        if arg == "--json" {
            output_format = OutputFormat::Json;
            continue;
        }
        if arg == "--holes-only" {
            analysis_holes_only = true;
            continue;
        }
        if arg == "--strict-naming" {
            strict_naming = true;
            continue;
        }
        if arg == "--allow-review-fixes" {
            fix_allow_review_fixes = true;
            continue;
        }
        if arg == "--format" || arg.starts_with("--format=") {
            return CommandSpec {
                trace_enabled,
                parsed: ParsedCommandSpec::Error(
                    "error: `--format` was removed; use `--error-format`".to_string(),
                ),
            };
        }
        if arg == "--workspace-diagnostics" {
            workspace_diagnostics = true;
            continue;
        }
        if let Some(value) = arg.strip_prefix("--query-backend=") {
            match parse_query_backend_flag(value) {
                Ok(parsed) => query_backend = Some(parsed),
                Err(err) => {
                    return CommandSpec {
                        trace_enabled,
                        parsed: ParsedCommandSpec::Error(err),
                    };
                }
            }
            continue;
        }
        if arg == "--error-format" {
            if let Some(fmt) = iter.next() {
                if let Err(err) = apply_output_format_flag(&arg, &fmt, &mut output_format) {
                    return CommandSpec {
                        trace_enabled,
                        parsed: ParsedCommandSpec::Error(err),
                    };
                }
                continue;
            }
            return CommandSpec {
                trace_enabled,
                parsed: ParsedCommandSpec::Error(format!("error: missing value for {arg}")),
            };
        }
        if let Some(fmt) = arg.strip_prefix("--error-format=") {
            if let Err(err) = apply_output_format_flag("--error-format", fmt, &mut output_format) {
                return CommandSpec {
                    trace_enabled,
                    parsed: ParsedCommandSpec::Error(err),
                };
            }
            continue;
        }
        if arg == "--emit-mir" {
            emit_mir = true;
            continue;
        }
        if arg == "--emit-mir-opt" {
            emit_mir_opt = true;
            continue;
        }
        if let Some(path) = arg.strip_prefix("--emit-obj=") {
            emit_obj = Some(path.to_string());
            continue;
        }
        if let Some(path) = arg.strip_prefix("--emit-bin=") {
            emit_bin = Some(path.to_string());
            continue;
        }
        if let Some(ms) = arg.strip_prefix("--poll-ms=") {
            match ms.parse::<u64>() {
                Ok(parsed) => poll_ms = Some(parsed),
                Err(_) => {
                    return CommandSpec {
                        trace_enabled,
                        parsed: ParsedCommandSpec::Error(format!(
                            "error: invalid --poll-ms value `{ms}`"
                        )),
                    };
                }
            }
            continue;
        }
        if let Some(jobs) = arg.strip_prefix("--jobs=") {
            match jobs.parse::<usize>() {
                Ok(parsed) => test_jobs = Some(parsed),
                Err(_) => {
                    return CommandSpec {
                        trace_enabled,
                        parsed: ParsedCommandSpec::Error(format!(
                            "error: invalid --jobs value `{jobs}`"
                        )),
                    };
                }
            }
            continue;
        }
        if let Some(ms) = arg.strip_prefix("--test-timeout-ms=") {
            match ms.parse::<u64>() {
                Ok(parsed) => test_timeout_ms = Some(parsed),
                Err(_) => {
                    return CommandSpec {
                        trace_enabled,
                        parsed: ParsedCommandSpec::Error(format!(
                            "error: invalid --test-timeout-ms value `{ms}`"
                        )),
                    };
                }
            }
            continue;
        }
        if arg == "--record" {
            test_record = true;
            continue;
        }
        if arg == "--integration-mode" {
            integration_mode = true;
            continue;
        }
        if arg == "--update-public-surface" {
            test_update_public_surface = true;
            continue;
        }
        if arg == "--list" {
            test_selection.list = true;
            continue;
        }
        if let Some(value) = arg.strip_prefix("--id=") {
            test_selection.id = Some(value.to_string());
            continue;
        }
        if let Some(value) = arg.strip_prefix("--filter=") {
            test_selection.filter = Some(value.to_string());
            continue;
        }
        if let Some(value) = arg.strip_prefix("--lane=") {
            match parse_test_lane_flag(value) {
                Some(parsed_lane) => test_selection.lane = Some(parsed_lane),
                None => {
                    return CommandSpec {
                        trace_enabled,
                        parsed: ParsedCommandSpec::Error(format!(
                            "error: invalid --lane value `{value}` (expected one of fast|full|spec|integration|sim|model|default)"
                        )),
                    };
                }
            }
            continue;
        }
        if let Some(value) = arg.strip_prefix("--seed=") {
            match value.parse::<u64>() {
                Ok(seed) => test_seed = Some(seed),
                Err(_) => {
                    return CommandSpec {
                        trace_enabled,
                        parsed: ParsedCommandSpec::Error(format!(
                            "error: invalid --seed value `{value}`"
                        )),
                    };
                }
            }
            continue;
        }
        if let Some(path) = arg.strip_prefix("--repro=") {
            repro_artifact_path = Some(path.to_string());
            continue;
        }
        if arg == "--repro" {
            if let Some(path) = iter.next() {
                repro_artifact_path = Some(path);
                continue;
            }
            return CommandSpec {
                trace_enabled,
                parsed: ParsedCommandSpec::Error("error: missing path for --repro".to_string()),
            };
        }
        if let Some(path) = arg.strip_prefix("--replay-trace=") {
            replay_trace_path = Some(path.to_string());
            continue;
        }
        if arg == "--replay-trace" {
            if let Some(path) = iter.next() {
                replay_trace_path = Some(path);
                continue;
            }
            return CommandSpec {
                trace_enabled,
                parsed: ParsedCommandSpec::Error(
                    "error: missing path for --replay-trace".to_string(),
                ),
            };
        }
        if arg == "--perf-debug" {
            perf_debug = true;
            continue;
        }
        if arg == "--why-not-120" {
            perf_why_not_120 = true;
            continue;
        }
        if let Some(runs) = arg.strip_prefix("--runs=") {
            match runs.parse::<usize>() {
                Ok(parsed) => perf_runs = Some(parsed),
                Err(_) => {
                    return CommandSpec {
                        trace_enabled,
                        parsed: ParsedCommandSpec::Error(format!(
                            "error: invalid --runs value `{runs}`"
                        )),
                    };
                }
            }
            continue;
        }
        if let Some(path) = arg.strip_prefix("--baseline-out=") {
            perf_baseline_out = Some(path.to_string());
            continue;
        }
        if let Some(path) = arg.strip_prefix("--perf-gate=") {
            perf_gate_path = Some(path.to_string());
            continue;
        }
        if let Some(value) = arg.strip_prefix("--perf-max-regression-pct=") {
            match value.parse::<f64>() {
                Ok(parsed) => perf_max_regression_pct = Some(parsed),
                Err(_) => {
                    return CommandSpec {
                        trace_enabled,
                        parsed: ParsedCommandSpec::Error(format!(
                            "error: invalid --perf-max-regression-pct value `{value}`"
                        )),
                    };
                }
            }
            continue;
        }
        if let Some(value) = arg.strip_prefix("--perf-cv-max-pct=") {
            match value.parse::<f64>() {
                Ok(parsed) => perf_cv_max_pct = Some(parsed),
                Err(_) => {
                    return CommandSpec {
                        trace_enabled,
                        parsed: ParsedCommandSpec::Error(format!(
                            "error: invalid --perf-cv-max-pct value `{value}`"
                        )),
                    };
                }
            }
            continue;
        }
        if let Some(value) = arg.strip_prefix("--kpi-check-fallback-max=") {
            match value.parse::<f64>() {
                Ok(parsed) => kpi_check_fallback_max = Some(parsed),
                Err(_) => {
                    return CommandSpec {
                        trace_enabled,
                        parsed: ParsedCommandSpec::Error(format!(
                            "error: invalid --kpi-check-fallback-max value `{value}`"
                        )),
                    };
                }
            }
            continue;
        }
        if let Some(value) = arg.strip_prefix("--kpi-check-batch-min=") {
            match value.parse::<f64>() {
                Ok(parsed) => kpi_check_batch_min = Some(parsed),
                Err(_) => {
                    return CommandSpec {
                        trace_enabled,
                        parsed: ParsedCommandSpec::Error(format!(
                            "error: invalid --kpi-check-batch-min value `{value}`"
                        )),
                    };
                }
            }
            continue;
        }
        if let Some(value) = arg.strip_prefix("--kpi-scheduler-p99-improve-min-pct=") {
            match value.parse::<f64>() {
                Ok(parsed) => kpi_scheduler_p99_improve_min_pct = Some(parsed),
                Err(_) => {
                    return CommandSpec {
                        trace_enabled,
                        parsed: ParsedCommandSpec::Error(format!(
                            "error: invalid --kpi-scheduler-p99-improve-min-pct value `{value}`"
                        )),
                    };
                }
            }
            continue;
        }
        if let Some(value) = arg.strip_prefix("--kpi-rewrite-overhead-max-pct=") {
            match value.parse::<f64>() {
                Ok(parsed) => kpi_rewrite_overhead_max_pct = Some(parsed),
                Err(_) => {
                    return CommandSpec {
                        trace_enabled,
                        parsed: ParsedCommandSpec::Error(format!(
                            "error: invalid --kpi-rewrite-overhead-max-pct value `{value}`"
                        )),
                    };
                }
            }
            continue;
        }
        if let Some(value) = arg.strip_prefix("--kpi-actor-throughput-improve-min-pct=") {
            match value.parse::<f64>() {
                Ok(parsed) => kpi_actor_throughput_improve_min_pct = Some(parsed),
                Err(_) => {
                    return CommandSpec {
                        trace_enabled,
                        parsed: ParsedCommandSpec::Error(format!(
                            "error: invalid --kpi-actor-throughput-improve-min-pct value `{value}`"
                        )),
                    };
                }
            }
            continue;
        }
        if let Some(value) = arg.strip_prefix("--kpi-queue-age-p99-max-regress-pct=") {
            match value.parse::<f64>() {
                Ok(parsed) => kpi_queue_age_p99_max_regress_pct = Some(parsed),
                Err(_) => {
                    return CommandSpec {
                        trace_enabled,
                        parsed: ParsedCommandSpec::Error(format!(
                            "error: invalid --kpi-queue-age-p99-max-regress-pct value `{value}`"
                        )),
                    };
                }
            }
            continue;
        }
        if let Some(value) = arg.strip_prefix("--kpi-starvation-violations-max=") {
            match value.parse::<f64>() {
                Ok(parsed) => kpi_starvation_violations_max = Some(parsed),
                Err(_) => {
                    return CommandSpec {
                        trace_enabled,
                        parsed: ParsedCommandSpec::Error(format!(
                            "error: invalid --kpi-starvation-violations-max value `{value}`"
                        )),
                    };
                }
            }
            continue;
        }
        if let Some(value) = arg.strip_prefix("--kpi-scheduler-throughput-improve-min-pct=") {
            match value.parse::<f64>() {
                Ok(parsed) => kpi_scheduler_throughput_improve_min_pct = Some(parsed),
                Err(_) => {
                    return CommandSpec {
                        trace_enabled,
                        parsed: ParsedCommandSpec::Error(format!(
                            "error: invalid --kpi-scheduler-throughput-improve-min-pct value `{value}`"
                        )),
                    };
                }
            }
            continue;
        }
        if let Some(value) = arg.strip_prefix("--kpi-scheduler-loop-p99-max-regress-pct=") {
            match value.parse::<f64>() {
                Ok(parsed) => kpi_scheduler_loop_p99_max_regress_pct = Some(parsed),
                Err(_) => {
                    return CommandSpec {
                        trace_enabled,
                        parsed: ParsedCommandSpec::Error(format!(
                            "error: invalid --kpi-scheduler-loop-p99-max-regress-pct value `{value}`"
                        )),
                    };
                }
            }
            continue;
        }
        if let Some(value) = arg.strip_prefix("--kpi-scheduler-local-hit-min=") {
            match value.parse::<f64>() {
                Ok(parsed) => kpi_scheduler_local_hit_min = Some(parsed),
                Err(_) => {
                    return CommandSpec {
                        trace_enabled,
                        parsed: ParsedCommandSpec::Error(format!(
                            "error: invalid --kpi-scheduler-local-hit-min value `{value}`"
                        )),
                    };
                }
            }
            continue;
        }
        if let Some(path) = arg.strip_prefix("--benchmark-manifest=") {
            benchmark_manifest_path = Some(path.to_string());
            continue;
        }
        if let Some(value) = arg.strip_prefix("--profile=") {
            perf_profile_name = Some(value.to_string());
            continue;
        }
        if let Some(value) = arg.strip_prefix("--baseline-ref=") {
            perfcmp_baseline_ref = Some(value.to_string());
            continue;
        }
        if let Some(value) = arg.strip_prefix("--candidate-ref=") {
            perfcmp_candidate_ref = Some(value.to_string());
            continue;
        }
        if let Some(value) = arg.strip_prefix("--warmup-pairs=") {
            match value.parse::<usize>() {
                Ok(parsed) => perfcmp_warmup_pairs = Some(parsed),
                Err(_) => {
                    return CommandSpec {
                        trace_enabled,
                        parsed: ParsedCommandSpec::Error(format!(
                            "error: invalid --warmup-pairs value `{value}`"
                        )),
                    };
                }
            }
            continue;
        }
        if let Some(value) = arg.strip_prefix("--measure-pairs=") {
            match value.parse::<usize>() {
                Ok(parsed) => perfcmp_measure_pairs = Some(parsed),
                Err(_) => {
                    return CommandSpec {
                        trace_enabled,
                        parsed: ParsedCommandSpec::Error(format!(
                            "error: invalid --measure-pairs value `{value}`"
                        )),
                    };
                }
            }
            continue;
        }
        if let Some(value) = arg.strip_prefix("--min-effect-pct=") {
            match value.parse::<f64>() {
                Ok(parsed) => perfcmp_min_effect_pct = Some(parsed),
                Err(_) => {
                    return CommandSpec {
                        trace_enabled,
                        parsed: ParsedCommandSpec::Error(format!(
                            "error: invalid --min-effect-pct value `{value}`"
                        )),
                    };
                }
            }
            continue;
        }
        if let Some(value) = arg.strip_prefix("--confidence=") {
            match value.parse::<f64>() {
                Ok(parsed) => perfcmp_confidence_pct = Some(parsed),
                Err(_) => {
                    return CommandSpec {
                        trace_enabled,
                        parsed: ParsedCommandSpec::Error(format!(
                            "error: invalid --confidence value `{value}`"
                        )),
                    };
                }
            }
            continue;
        }
        if arg == "--prefix" {
            if let Some(path) = iter.next() {
                prefix_path = Some(path);
            } else {
                return CommandSpec {
                    trace_enabled,
                    parsed: ParsedCommandSpec::Error(
                        "error: missing path for --prefix".to_string(),
                    ),
                };
            }
            continue;
        }
        if arg == "-o" || arg == "--out" {
            if let Some(path) = iter.next() {
                out_path = Some(path);
            } else {
                return CommandSpec {
                    trace_enabled,
                    parsed: ParsedCommandSpec::Error(format!(
                        "error: missing output path for {arg}"
                    )),
                };
            }
            continue;
        }
        if command.is_none() {
            if let Some(parsed_command) = CommandName::parse(&arg) {
                command = Some(parsed_command);
                continue;
            }
        }
        if path_arg.is_none() {
            path_arg = Some(arg);
        } else {
            program_args.push(arg);
        }
    }

    if command.is_none() && path_arg.is_some() {
        command = Some(CommandName::Run);
    }

    let command = match command {
        Some(command) => command,
        None => {
            return CommandSpec {
                trace_enabled,
                parsed: ParsedCommandSpec::Error("__print_help__".to_string()),
            };
        }
    };
    let state = ParseState {
        emit_mir,
        emit_mir_opt,
        emit_obj,
        emit_bin,
        out_path,
        prefix_path,
        query_backend,
        integration_mode,
        path_arg,
        program_args,
        poll_ms,
        test_jobs,
        test_timeout_ms,
        test_record,
        test_update_public_surface,
        test_selection,
        test_seed,
        repro_artifact_path,
        replay_trace_path,
        perf_debug,
        perf_runs,
        perf_baseline_out,
        perf_gate_path,
        perf_max_regression_pct,
        perf_cv_max_pct,
        perf_why_not_120,
        kpi_check_fallback_max,
        kpi_check_batch_min,
        kpi_scheduler_p99_improve_min_pct,
        kpi_rewrite_overhead_max_pct,
        kpi_actor_throughput_improve_min_pct,
        kpi_queue_age_p99_max_regress_pct,
        kpi_starvation_violations_max,
        kpi_scheduler_throughput_improve_min_pct,
        kpi_scheduler_loop_p99_max_regress_pct,
        kpi_scheduler_local_hit_min,
        benchmark_manifest_path,
        perf_profile_name,
        perfcmp_baseline_ref,
        perfcmp_candidate_ref,
        perfcmp_warmup_pairs,
        perfcmp_measure_pairs,
        perfcmp_min_effect_pct,
        perfcmp_confidence_pct,
        analysis_holes_only,
        strict_naming,
        fix_allow_review_fixes,
        workspace_diagnostics,
        output_format,
    };

    match build_parsed_command(command, state) {
        Ok(parsed) => CommandSpec {
            trace_enabled,
            parsed: ParsedCommandSpec::Ready(parsed),
        },
        Err(err) => CommandSpec {
            trace_enabled,
            parsed: ParsedCommandSpec::Error(err),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn to_args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    fn parse_ready(values: &[&str]) -> ParsedCommand {
        match parse(to_args(values)).parsed {
            ParsedCommandSpec::Ready(parsed) => parsed,
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    fn parse_error(values: &[&str]) -> String {
        match parse(to_args(values)).parsed {
            ParsedCommandSpec::Error(err) => err,
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_preserves_argument_order() {
        match parse_ready(&["test", "apps/ledger-lite", "--list"]) {
            ParsedCommand::Test(args) => {
                assert_eq!(args.path_arg.as_deref(), Some("apps/ledger-lite"));
                assert!(args.test_selection.list);
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_detects_invalid_seed() {
        assert!(parse_error(&["test", "--seed=abc"]).contains("invalid --seed value"));
    }

    #[test]
    fn parse_help_control() {
        let spec = parse(vec!["--help".to_string()]);
        assert!(matches!(spec.parsed, ParsedCommandSpec::Help));
    }

    #[test]
    fn parse_help_command_alias() {
        let spec = parse(vec!["help".to_string()]);
        assert!(matches!(spec.parsed, ParsedCommandSpec::Help));
    }

    #[test]
    fn parse_version_control() {
        let spec = parse(vec!["--version".to_string()]);
        assert!(matches!(spec.parsed, ParsedCommandSpec::Version));
    }

    #[test]
    fn parse_repro_requires_value() {
        assert!(parse_error(&["test", "--repro"]).contains("missing path for --repro"));
    }

    #[test]
    fn parse_malformed_numeric_values() {
        assert!(parse_error(&["perfcmp", "--confidence=x"]).contains("invalid --confidence value"));
        assert!(parse_error(&["test", "--jobs=abc"]).contains("invalid --jobs value"));
        assert!(parse_error(&["dev", "--poll-ms=fast"]).contains("invalid --poll-ms value"));
        assert!(
            parse_error(&["perf", "--kpi-check-batch-min=nope"])
                .contains("invalid --kpi-check-batch-min value")
        );
    }

    #[test]
    fn parse_json_format_and_program_args() {
        match parse_ready(&[
            "--error-format=json",
            "run",
            "apps/ledger-lite",
            "--",
            "--dry-run",
            "value",
        ]) {
            ParsedCommand::Run(args) => {
                assert_eq!(args.output_format, OutputFormat::Json);
                assert_eq!(args.path_arg.as_deref(), Some("apps/ledger-lite"));
                assert!(!args.integration_mode);
                assert_eq!(
                    args.program_args,
                    vec!["--dry-run".to_string(), "value".to_string()]
                );
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_sarif_format_and_program_args() {
        match parse_ready(&[
            "--error-format=sarif",
            "check",
            "apps/ledger-lite/src/main.wr",
        ]) {
            ParsedCommand::Check(args) => {
                assert_eq!(args.output_format, OutputFormat::Sarif);
                assert_eq!(
                    args.path_arg.as_deref(),
                    Some("apps/ledger-lite/src/main.wr")
                );
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_json_shorthand_sets_json_output_format() {
        match parse_ready(&["--json", "check", "."]) {
            ParsedCommand::Check(args) => assert_eq!(args.output_format, OutputFormat::Json),
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_holes_only_flag() {
        match parse_ready(&["--holes-only", "analyze", "."]) {
            ParsedCommand::Analyze(args) => assert!(args.analysis_holes_only),
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_allow_review_fixes_flag() {
        match parse_ready(&["--allow-review-fixes", "fix", "."]) {
            ParsedCommand::Fix(args) => assert!(args.fix_allow_review_fixes),
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_error_format_human_sets_pretty_output() {
        match parse_ready(&["--error-format=human", "check", "."]) {
            ParsedCommand::Check(args) => assert_eq!(args.output_format, OutputFormat::Pretty),
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_output_format_last_flag_wins() {
        match parse_ready(&[
            "--json",
            "--error-format=sarif",
            "--error-format=json",
            "check",
            ".",
        ]) {
            ParsedCommand::Check(args) => assert_eq!(args.output_format, OutputFormat::Json),
            other => panic!("unexpected parse result: {other:?}"),
        }

        match parse_ready(&[
            "--error-format=json",
            "--json",
            "--error-format=sarif",
            "check",
            ".",
        ]) {
            ParsedCommand::Check(args) => assert_eq!(args.output_format, OutputFormat::Sarif),
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_invalid_error_format_value() {
        assert!(
            parse_error(&["--error-format=wat", "check", "."])
                .contains("invalid --error-format value")
        );
    }

    #[test]
    fn parse_workspace_diagnostics_flag() {
        match parse_ready(&["--workspace-diagnostics", "fmt", "."]) {
            ParsedCommand::Fmt(args) => assert!(args.workspace_diagnostics),
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_perf_why_not_120_flag() {
        match parse_ready(&["perf", "--why-not-120", "."]) {
            ParsedCommand::Perf(args) => assert!(args.perf_why_not_120),
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_rejects_removed_format_alias() {
        assert!(parse_error(&["--format=json", "check", "."]).contains("`--format` was removed"));
    }

    #[test]
    fn parse_requires_command() {
        let spec = parse(Vec::new());
        match spec.parsed {
            ParsedCommandSpec::Error(err) => assert_eq!(err, "__print_help__"),
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_accepts_known_commands() {
        for argv in [
            &["init"][..],
            &["update"],
            &["check"],
            &["analyze"],
            &["fix"],
            &["fmt"],
            &["build"],
            &["compile"],
            &["query-contracts"],
            &["collision-contracts"],
            &["collision-plan"],
            &["collision-run"],
            &["preview"],
            &["frame"],
            &["frame-live"],
            &["frame-contracts"],
            &["presentation-plan"],
            &["presentation-debug"],
            &["verify-cert", "cert.json"],
            &["run"],
            &["dev"],
            &["test"],
            &["eval"],
            &["perf"],
            &["perfcmp"],
            &["matrix"],
        ] {
            let parsed = parse_ready(argv);
            assert_eq!(parsed.command_name(), argv[0]);
        }
    }

    #[test]
    fn parse_run_integration_mode_flag() {
        match parse_ready(&[
            "run",
            "--integration-mode",
            "src/application/composition/main.wr",
        ]) {
            ParsedCommand::Run(args) => {
                assert!(args.integration_mode);
                assert_eq!(
                    args.path_arg.as_deref(),
                    Some("src/application/composition/main.wr")
                );
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_run_query_backend_flag() {
        match parse_ready(&["run", "--query-backend=wgsl", "language/preview"]) {
            ParsedCommand::Run(args) => {
                assert_eq!(args.query_backend, DispatchBackend::Wgsl);
                assert_eq!(args.path_arg.as_deref(), Some("language/preview"));
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_rejects_invalid_query_backend_flag() {
        assert!(
            parse_error(&["run", "--query-backend=metal", "language/preview"])
                .contains("invalid --query-backend value")
        );
    }

    #[test]
    fn parse_preview_is_typed_after_parsing() {
        match parse_ready(&[
            "--json",
            "preview",
            "language/preview",
            "--json-report",
            "--attachment=depth",
            "--width=640",
        ]) {
            ParsedCommand::Preview(args) => {
                assert_eq!(args.output_format, OutputFormat::Json);
                assert_eq!(args.path_arg.as_deref(), Some("language/preview"));
                assert!(args.options.json_report);
                assert_eq!(args.options.attachment, "depth");
                assert_eq!(args.options.width, Some(640));
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_preview_rejects_json_without_json_report() {
        assert!(
            parse_error(&["--json", "preview", "language/preview"])
                .contains("`preview --json` requires `--json-report`")
        );
    }

    #[test]
    fn parse_frame_is_typed_after_parsing() {
        match parse_ready(&[
            "frame",
            "language/preview",
            "--attachment=color",
            "--attachment-format=ppm",
        ]) {
            ParsedCommand::Frame(args) => {
                assert_eq!(args.path_arg.as_deref(), Some("language/preview"));
                assert_eq!(args.options.attachments, vec!["color".to_string()]);
                assert_eq!(args.options.attachment_format, FrameAttachmentFormat::Ppm);
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_frame_rejects_json_ppm_combo() {
        assert!(
            parse_error(&[
                "--json",
                "frame",
                "language/preview",
                "--attachment-format=ppm",
            ])
            .contains("`frame --json` cannot be combined with `--attachment-format=ppm`")
        );
    }

    #[test]
    fn parse_frame_rejects_ppm_without_single_attachment() {
        assert!(
            parse_error(&["frame", "language/preview", "--attachment-format=ppm"]).contains(
                "`frame --attachment-format=ppm` requires exactly one selected attachment"
            )
        );
        assert!(
            parse_error(&[
                "frame",
                "language/preview",
                "--attachment=color",
                "--attachment=depth",
                "--attachment-format=ppm",
            ])
            .contains("`frame --attachment-format=ppm` requires exactly one selected attachment")
        );
    }

    #[test]
    fn parse_frame_live_is_typed_after_parsing() {
        match parse_ready(&[
            "frame-live",
            "language/preview",
            "--view=main",
            "--width=640",
            "--height=360",
            "--json",
        ]) {
            ParsedCommand::FrameLive(args) => {
                assert_eq!(args.output_format, OutputFormat::Json);
                assert_eq!(args.path_arg.as_deref(), Some("language/preview"));
                assert_eq!(args.options.view.as_deref(), Some("main"));
                assert_eq!(args.options.width, Some(640));
                assert_eq!(args.options.height, Some(360));
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_frame_contracts_requested_view_is_typed() {
        match parse_ready(&["frame-contracts", "language/preview", "--view=main"]) {
            ParsedCommand::FrameContracts(args) => {
                assert_eq!(args.path_arg.as_deref(), Some("language/preview"));
                assert_eq!(args.requested_view.as_deref(), Some("main"));
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_presentation_debug_is_typed_after_parsing() {
        match parse_ready(&[
            "presentation-debug",
            "language/preview",
            "--view=main",
            "--solver-mode=dense-only",
            "--frames=2",
            "--out-dir=tmp/debug",
        ]) {
            ParsedCommand::PresentationDebug(args) => {
                assert_eq!(args.path_arg.as_deref(), Some("language/preview"));
                assert_eq!(args.options.view.as_deref(), Some("main"));
                assert_eq!(
                    args.options.query_trace_solver_mode,
                    wrela::query_exec::QueryTraceSolverMode::DenseOnly
                );
                assert_eq!(args.options.frames, 2);
                assert_eq!(args.options.out_dir, Some(PathBuf::from("tmp/debug")));
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_collision_run_rejects_non_cpu_backend_at_parse_time() {
        assert!(
            parse_error(&["collision-run", "--query-backend=wgsl"])
                .contains("collision-run only supports cpu or auto query backends")
        );
    }

    #[test]
    fn parse_test_lane_is_typed_after_parsing() {
        match parse_ready(&["perf", "--lane=integration", "."]) {
            ParsedCommand::Perf(args) => {
                assert_eq!(
                    args.test_selection.lane,
                    Some(ParsedTestLaneSelection::Single(ParsedTestLane::Integration,))
                );
            }
            other => panic!("unexpected parse result: {other:?}"),
        }
    }

    #[test]
    fn parse_rejects_command_local_flags_during_parsing() {
        assert!(
            parse_error(&["check", "--list", "."])
                .contains("only valid with `wrela test` or `wrela perf`")
        );
        assert!(
            parse_error(&["run", "--emit-obj=out.o", "."])
                .contains("only valid with `wrela build`, `wrela compile`, or `wrela test`")
        );
    }
}
