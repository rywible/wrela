use super::command_handlers::{
    self, BenchmarkManifest, CollisionBenchmarkReport, DifferentialPipeline, KpiThresholds,
    PerfCmpConfig, PerfGateConfig, PerfProfile, PerfReport, PresentationBenchmarkComparison,
    PresentationBenchmarkReport, PresentationWgslWorkgroupComparison, TestSelection, TestTarget,
    budget_jobs_timeout, build_benchmark_selection, load_benchmark_manifest,
    resolve_budget_policy_v1, resolve_test_target,
};
use super::contracts::{EXIT_CODEGEN, EXIT_OK, EXIT_USAGE, OutputFormat};
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use std::collections::{BTreeMap, HashMap};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use wrela::acceleration::report::explain_why_not_120_findings as explain_acceleration_why_not_120_findings;
use wrela::gpu_runtime::{GpuLimitRequest, shared_wgpu_context};
use wrela::hir;
use wrela::hir::lower as hir_lower;
use wrela::kernel::{KernelStructValue, KernelValue};
use wrela::parser;
use wrela::parser::ast;
use wrela::parser::ast::AstNode;
use wrela::perf_target::{
    PerfClosureExecutionStory, PerfClosureFinding, PerfClosureLaneStatus,
    PerfClosureLaneStatusReport, PerfClosureProfile, PerfClosureReport, PerfClosureVerdict,
    PerfClosureVerdictStatus, quality_degradation_step_name,
};
use wrela::presentation_exec::cost::explain_why_not_120_findings as explain_frame_why_not_120_findings;
use wrela::query_exec::{
    QueryExecContext, QueryTraceSolverMode, WGSL_WORKGROUP_SIZE_OVERRIDE_ENV,
    stable_region_scene_capture_id,
};

pub(super) struct PerfCommandInput {
    pub(super) trace: bool,
    pub(super) program_args: Vec<String>,
    pub(super) path_arg: Option<String>,
    pub(super) perf_runs: Option<usize>,
    pub(super) test_jobs: Option<usize>,
    pub(super) test_timeout_ms: Option<u64>,
    pub(super) benchmark_manifest_path: Option<String>,
    pub(super) perf_profile: PerfProfile,
    pub(super) perf_baseline_out: Option<String>,
    pub(super) perf_gate_path: Option<String>,
    pub(super) perf_max_regression_pct: Option<f64>,
    pub(super) perf_cv_max_pct: Option<f64>,
    pub(super) perf_why_not_120: bool,
    pub(super) kpi_thresholds: KpiThresholds,
    pub(super) output_format: OutputFormat,
    pub(super) perf_debug: bool,
    pub(super) test_selection: TestSelection,
    pub(super) query_backend: wrela::query_plan::DispatchBackend,
}

pub(super) fn execute_perf_command(mut input: PerfCommandInput) -> i32 {
    if input.trace {
        eprintln!("build: command perf");
    }
    if !input.program_args.is_empty() {
        eprintln!("error: unexpected extra arguments");
        return EXIT_USAGE;
    }
    let closure_protocol = matches!(input.perf_profile, PerfProfile::Closure1080p120)
        .then(PerfClosureProfile::canonical_1080p120);
    let use_canonical_counts =
        closure_protocol.is_some() && input.perf_runs.is_none() && input.perf_gate_path.is_none();
    let warmup_runs = closure_protocol
        .as_ref()
        .filter(|_| use_canonical_counts)
        .map(|profile| profile.warmup_runs as usize)
        .unwrap_or(0);
    let runs = closure_protocol
        .as_ref()
        .filter(|_| use_canonical_counts)
        .map(|profile| profile.measured_runs as usize)
        .unwrap_or_else(|| input.perf_runs.unwrap_or(5).max(1));
    let budget_policy = resolve_budget_policy_v1(input.test_jobs, input.test_timeout_ms);
    let (jobs, mut timeout) = budget_jobs_timeout(&budget_policy);
    let target = match resolve_test_target(input.path_arg.as_deref()) {
        Ok(target) => target,
        Err(err) => {
            eprintln!("error: {err}");
            return EXIT_USAGE;
        }
    };
    if let TestTarget::SingleFile(path) = &target {
        eprintln!(
            "error: `wrela perf` requires a project-root directory; single-file .wr targets are not supported: {}",
            path.display()
        );
        eprintln!(
            "help: run `wrela perf <project-root>` using project layout (`src/**`, `tests/**`)"
        );
        return EXIT_USAGE;
    }

    let manifest_path = resolve_perf_benchmark_manifest_path(
        &target,
        input.benchmark_manifest_path,
        input.perf_profile,
    );
    let mut benchmark_manifest = None;
    let mut runtime_only_cv_gate = false;
    if let Some(path) = manifest_path.as_ref() {
        let manifest = match load_benchmark_manifest(path) {
            Ok(manifest) => manifest,
            Err(err) => {
                eprintln!("benchmark manifest error: {err}");
                return EXIT_USAGE;
            }
        };
        let max_timeout_ms = manifest
            .scenarios_for_profile(input.perf_profile)
            .iter()
            .filter_map(|scenario| scenario.timeout_ms)
            .max();
        if let Some(max_timeout_ms) = max_timeout_ms {
            timeout = timeout.max(std::time::Duration::from_millis(max_timeout_ms));
        }
        benchmark_manifest = Some(manifest.clone());
        runtime_only_cv_gate = true;
        match build_benchmark_selection(&target, path, input.perf_profile) {
            Ok(selection_ids) => {
                command_handlers::set_test_selection_include_ids(
                    &mut input.test_selection,
                    selection_ids,
                );
            }
            Err(err) => {
                eprintln!("benchmark manifest error: {err}");
                return EXIT_USAGE;
            }
        }
    }

    let baseline_out = input
        .perf_baseline_out
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".artifacts/perf/baseline.json"));
    let gate_cfg = input.perf_gate_path.as_ref().map(|path| PerfGateConfig {
        baseline_path: PathBuf::from(path),
        max_regression_pct: input.perf_max_regression_pct.unwrap_or(5.0),
        kpi_thresholds: input.kpi_thresholds,
    });
    let cv_max_pct = input.perf_cv_max_pct.unwrap_or(5.0);
    run_perf_harness(
        &target,
        &budget_policy,
        jobs,
        timeout,
        input.output_format,
        input.perf_debug,
        input.perf_why_not_120,
        warmup_runs,
        runs,
        cv_max_pct,
        &baseline_out,
        gate_cfg.as_ref(),
        &input.test_selection,
        runtime_only_cv_gate,
        input.query_backend,
        benchmark_manifest.as_ref(),
        input.perf_profile,
    )
}

pub(super) fn run_perf_harness(
    target: &TestTarget,
    budget_policy: &command_handlers::BudgetPolicyV1,
    jobs: usize,
    timeout: Duration,
    output_format: OutputFormat,
    perf_debug: bool,
    perf_why_not_120: bool,
    warmup_runs: usize,
    runs: usize,
    cv_max_pct: f64,
    baseline_out: &std::path::Path,
    perf_gate: Option<&PerfGateConfig>,
    selection: &TestSelection,
    runtime_only_cv_gate: bool,
    query_backend: wrela::query_plan::DispatchBackend,
    benchmark_manifest: Option<&BenchmarkManifest>,
    perf_profile: PerfProfile,
) -> i32 {
    let presentation_benchmarks_active = benchmark_manifest.is_some_and(|manifest| {
        manifest
            .scenarios_for_profile(perf_profile)
            .iter()
            .any(|scenario| scenario.presentation.is_some())
    });
    let collision_benchmarks_active = benchmark_manifest
        .is_some_and(|manifest| manifest.suite.eq_ignore_ascii_case("collision_perf"));
    let mut samples = Vec::new();
    let mut latest_presentation_reports = None;
    let mut presentation_report_samples = Vec::new();
    let mut presentation_report_errors = Vec::new();
    let mut latest_collision_reports = None;
    let mut collision_report_samples = Vec::new();
    let mut collision_report_errors = Vec::new();
    for idx in 0..warmup_runs {
        println!("perf-warmup {}/{}", idx + 1, warmup_runs);
        let (exit, _, _) = command_handlers::run_tests_once(
            target,
            budget_policy,
            jobs,
            timeout,
            output_format,
            perf_debug,
            true,
            selection,
            false,
            !presentation_benchmarks_active,
            command_handlers::HttpCassetteMode::Replay,
            None,
            query_backend,
            false,
            DifferentialPipeline::Baseline,
            None,
            None,
        );
        if exit != EXIT_OK {
            return exit;
        }
    }
    for idx in 0..runs {
        println!("perf-run {}/{}", idx + 1, runs);
        let (exit, summary, _) = command_handlers::run_tests_once(
            target,
            budget_policy,
            jobs,
            timeout,
            output_format,
            perf_debug,
            true,
            selection,
            false,
            !presentation_benchmarks_active,
            command_handlers::HttpCassetteMode::Replay,
            None,
            query_backend,
            false,
            DifferentialPipeline::Baseline,
            None,
            None,
        );
        if exit != EXIT_OK {
            return exit;
        }
        if let Some(summary) = summary {
            if presentation_benchmarks_active {
                let TestTarget::ProjectRoot(benchmark_root) = target else {
                    eprintln!("perf harness error: presentation benchmarks require a project root");
                    return EXIT_CODEGEN;
                };
                let Some(manifest) = benchmark_manifest else {
                    eprintln!(
                        "perf harness error: presentation benchmarks require a benchmark manifest"
                    );
                    return EXIT_CODEGEN;
                };
                let report_collection = match collect_presentation_benchmark_reports(
                    benchmark_root,
                    manifest,
                    perf_profile,
                    query_backend,
                ) {
                    Ok(collection) => collection,
                    Err(err) => {
                        eprintln!(
                            "perf harness error: failed to collect presentation reports: {err}"
                        );
                        return EXIT_CODEGEN;
                    }
                };
                let runtime_cases = report_collection
                    .reports
                    .iter()
                    .map(|report| {
                        (
                            report.scenario_id.clone(),
                            report.test_name.clone(),
                            report.frame_time_ns,
                        )
                    })
                    .collect::<Vec<_>>();
                let summary = if runtime_cases.is_empty() {
                    summary
                } else {
                    command_handlers::overlay_perf_summary_runtime_cases(&summary, &runtime_cases)
                };
                if matches!(output_format, OutputFormat::Pretty) {
                    command_handlers::emit_perf_summary(&summary, perf_debug);
                    print_presentation_benchmark_reports(&report_collection.reports);
                    for error in &report_collection.errors {
                        eprintln!("presentation-benchmark-error: {error}");
                    }
                }
                presentation_report_samples.extend(report_collection.reports.iter().cloned());
                presentation_report_errors.extend(report_collection.errors.into_iter());
                latest_presentation_reports = Some(report_collection.reports);
                samples.push(summary);
            } else if collision_benchmarks_active {
                let TestTarget::ProjectRoot(benchmark_root) = target else {
                    eprintln!("perf harness error: collision benchmarks require a project root");
                    return EXIT_CODEGEN;
                };
                let Some(manifest) = benchmark_manifest else {
                    eprintln!(
                        "perf harness error: collision benchmarks require a benchmark manifest"
                    );
                    return EXIT_CODEGEN;
                };
                let report_collection = match collect_collision_benchmark_reports(
                    benchmark_root,
                    manifest,
                    perf_profile,
                    query_backend,
                ) {
                    Ok(collection) => collection,
                    Err(err) => {
                        eprintln!("perf harness error: failed to collect collision reports: {err}");
                        return EXIT_CODEGEN;
                    }
                };
                let runtime_cases = report_collection
                    .reports
                    .iter()
                    .flat_map(collision_runtime_cases_from_report)
                    .collect::<Vec<_>>();
                let summary = if runtime_cases.is_empty() {
                    summary
                } else {
                    command_handlers::overlay_perf_summary_runtime_cases(&summary, &runtime_cases)
                };
                collision_report_samples.extend(report_collection.reports.iter().cloned());
                collision_report_errors.extend(report_collection.errors.into_iter());
                latest_collision_reports = Some(report_collection.reports);
                samples.push(summary);
            } else {
                samples.push(summary);
            }
        }
    }
    if samples.is_empty() {
        eprintln!("perf harness error: no samples produced");
        return EXIT_CODEGEN;
    }
    let summary = command_handlers::aggregate_perf_samples(&samples);
    let cv = command_handlers::compute_cv(&samples);
    if collision_benchmarks_active
        && latest_collision_reports.is_none()
        && !collision_report_samples.is_empty()
    {
        latest_collision_reports = Some(collision_report_samples.clone());
    }
    if matches!(output_format, OutputFormat::Pretty) {
        if let Some(reports) = latest_collision_reports.as_ref() {
            print_collision_benchmark_reports(reports);
            for error in &collision_report_errors {
                eprintln!("collision-benchmark-error: {error}");
            }
        }
    }
    let cv_exceeded = if runtime_only_cv_gate {
        cv.runtime_p50_pct > cv_max_pct
            || cv.runtime_p95_pct > cv_max_pct
            || cv.runtime_p99_pct > cv_max_pct
    } else {
        cv.compile_throughput_pct > cv_max_pct
            || cv.runtime_p50_pct > cv_max_pct
            || cv.runtime_p95_pct > cv_max_pct
            || cv.runtime_p99_pct > cv_max_pct
    };
    if cv_exceeded {
        if runtime_only_cv_gate {
            eprintln!(
                "perf harness failed: runtime coefficient of variation exceeded {:.2}%",
                cv_max_pct
            );
            eprintln!(
                "cv: runtime_p50={:.2}% runtime_p95={:.2}% runtime_p99={:.2}% (compile={:.2}% informational)",
                cv.runtime_p50_pct,
                cv.runtime_p95_pct,
                cv.runtime_p99_pct,
                cv.compile_throughput_pct
            );
        } else {
            eprintln!(
                "perf harness failed: coefficient of variation exceeded {:.2}%",
                cv_max_pct
            );
            eprintln!(
                "cv: compile={:.2}% runtime_p50={:.2}% runtime_p95={:.2}% runtime_p99={:.2}%",
                cv.compile_throughput_pct,
                cv.runtime_p50_pct,
                cv.runtime_p95_pct,
                cv.runtime_p99_pct
            );
        }
        return EXIT_CODEGEN;
    }
    if let Some(gate) = perf_gate {
        let baseline = match command_handlers::load_perf_baseline_summary(&gate.baseline_path) {
            Ok(baseline) => baseline,
            Err(err) => {
                eprintln!(
                    "perf gate error: failed to load baseline {}: {}",
                    gate.baseline_path.display(),
                    err
                );
                return EXIT_CODEGEN;
            }
        };
        let failures = command_handlers::evaluate_perf_gate(
            &summary,
            &baseline,
            gate.max_regression_pct,
            &gate.kpi_thresholds,
        );
        if !failures.is_empty() {
            eprintln!(
                "perf gate failed against {} (max regression {:.2}%):",
                gate.baseline_path.display(),
                gate.max_regression_pct
            );
            for failure in failures {
                eprintln!("  - {failure}");
            }
            return EXIT_CODEGEN;
        }
    }

    let closure_profile = PerfClosureProfile::canonical_1080p120();
    let closure_profile_errors = closure_profile.validate();
    if !closure_profile_errors.is_empty() {
        eprintln!("perf harness error: invalid canonical closure profile");
        for error in closure_profile_errors {
            eprintln!("  - {error}");
        }
        return EXIT_CODEGEN;
    }
    let closure_report = Some(build_closure_report(
        &closure_profile,
        benchmark_manifest,
        &samples,
        &presentation_report_samples,
        &presentation_report_errors,
        &collision_report_samples,
        perf_profile,
        warmup_runs,
        samples.len(),
    ));
    let report = PerfReport {
        version: 4,
        generated_at_unix_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::from_secs(0))
            .as_millis(),
        runs,
        cv,
        summary,
        samples,
        closure: closure_report,
        presentation_reports: latest_presentation_reports,
        collision_reports: latest_collision_reports,
    };
    if let Some(parent) = baseline_out.parent()
        && let Err(err) = fs::create_dir_all(parent)
    {
        eprintln!(
            "perf harness error: failed to create {}: {}",
            parent.display(),
            err
        );
        return EXIT_CODEGEN;
    }
    let json = match serde_json::to_vec_pretty(&report) {
        Ok(json) => json,
        Err(err) => {
            eprintln!("perf harness error: failed to serialize report: {err}");
            return EXIT_CODEGEN;
        }
    };
    if let Err(err) = fs::write(baseline_out, json) {
        eprintln!(
            "perf harness error: failed to write {}: {}",
            baseline_out.display(),
            err
        );
        return EXIT_CODEGEN;
    }
    if matches!(output_format, OutputFormat::Pretty)
        && let Some(closure) = report.closure.as_ref()
    {
        print_closure_verdict_report(closure, perf_why_not_120);
    }
    println!("perf baseline written: {}", baseline_out.display());
    EXIT_OK
}

#[derive(Debug, Clone, Deserialize)]
struct PresentationDebugCommandOutput {
    view: String,
    region: String,
    domain: String,
    backend: String,
    query_trace_solver_mode: String,
    frames_executed: u32,
    frame_cost: wrela::presentation_exec::PresentationFrameCostReport,
    #[serde(default)]
    frame_cost_history: Vec<wrela::presentation_exec::PresentationFrameCostReport>,
}

struct PresentationBenchmarkReportCollection {
    reports: Vec<PresentationBenchmarkReport>,
    errors: Vec<String>,
}

struct CollisionBenchmarkReportCollection {
    reports: Vec<command_handlers::CollisionBenchmarkReport>,
    errors: Vec<String>,
}

#[derive(Debug, Clone, Default)]
struct CollisionBenchmarkScenarioMetrics {
    query_count: u64,
    total_runtime_ns: u128,
    total_candidate_count: u64,
    total_rejected_candidate_count: u64,
    total_pruned_node_count: u64,
    total_interval_subdivisions: u64,
    total_interval_refinements: u64,
    total_certificate_successes: u64,
    total_fallback_count: u64,
    available_count_total: u64,
    consumed_count_total: u64,
    rejected_count_total: u64,
    unavailable_count_total: u64,
    last_interval_bracket: Option<[f32; 2]>,
    contact_normal_provenance: Option<String>,
}

#[derive(Debug, Clone, Copy, Default)]
struct PresentationAggregatedMetrics {
    frame_time_ns: u128,
    field_samples: u32,
    average_trace_steps: f32,
    candidate_count_before_pruning: u32,
    candidate_count_after_pruning: u32,
}

#[derive(Debug, Clone, Copy, Default)]
struct PresentationAggregatedSolverCounters {
    solver_relaxed_attempts: u64,
    solver_relaxed_no_root_advances: u64,
    solver_relaxed_brackets: u64,
    solver_relaxed_unresolved: u64,
    solver_interval_attempts: u64,
    solver_interval_no_root_advances: u64,
    solver_interval_brackets: u64,
    solver_interval_unresolved: u64,
    solver_refinement_attempts: u64,
    solver_refinement_failures: u64,
    solver_repeat_attempts: u64,
    solver_repeat_supported: u64,
    solver_repeat_inapplicable: u64,
    solver_repeat_unsupported: u64,
    solver_repeat_unsupported_form: u64,
    solver_repeat_unsupported_bounds: u64,
    solver_repeat_cells_enumerated: u64,
}

fn collect_presentation_benchmark_reports(
    benchmark_root: &Path,
    manifest: &BenchmarkManifest,
    profile: PerfProfile,
    query_backend: wrela::query_plan::DispatchBackend,
) -> Result<PresentationBenchmarkReportCollection, String> {
    let current_exe =
        env::current_exe().map_err(|err| format!("failed to resolve current executable: {err}"))?;
    let mut collection = PresentationBenchmarkReportCollection {
        reports: Vec::new(),
        errors: Vec::new(),
    };
    for scenario in manifest.scenarios_for_profile(profile) {
        let Some(spec) = scenario.presentation.as_ref() else {
            continue;
        };
        match run_presentation_benchmark_report(
            &current_exe,
            benchmark_root,
            scenario,
            spec,
            query_backend,
        ) {
            Ok(report) => collection.reports.push(report),
            Err(err) => collection.errors.push(err),
        }
    }
    Ok(collection)
}

fn collect_collision_benchmark_reports(
    benchmark_root: &Path,
    manifest: &BenchmarkManifest,
    profile: PerfProfile,
    query_backend: wrela::query_plan::DispatchBackend,
) -> Result<CollisionBenchmarkReportCollection, String> {
    let backend = collision_benchmark_backend(query_backend)?;
    let mut collection = CollisionBenchmarkReportCollection {
        reports: Vec::new(),
        errors: Vec::new(),
    };
    let mut contexts = HashMap::<PathBuf, CollisionBenchmarkContext>::new();
    let mut scenario_results = Vec::new();
    for scenario in manifest.scenarios_for_profile(profile) {
        let Some(spec) = scenario.collision.as_ref() else {
            continue;
        };
        let entry_path = collision_benchmark_entry_path(benchmark_root, spec);
        if !contexts.contains_key(&entry_path) {
            contexts.insert(
                entry_path.clone(),
                compile_collision_benchmark_context(&entry_path)?,
            );
        }
        let ctx = contexts
            .get(&entry_path)
            .expect("collision benchmark context inserted");
        match run_collision_benchmark_scenario(ctx, scenario, spec, backend) {
            Ok(result) => scenario_results.push(result),
            Err(err) => collection.errors.push(err),
        }
    }
    if !scenario_results.is_empty() {
        collection
            .reports
            .push(collision_benchmark_report_from_scenarios(
                manifest,
                backend,
                &scenario_results,
            ));
    }
    Ok(collection)
}

struct CollisionBenchmarkScenarioResult {
    execution: command_handlers::CollisionBenchmarkExecutionReport,
    metrics: CollisionBenchmarkScenarioMetrics,
}

struct CollisionBenchmarkContext {
    ctx: QueryExecContext,
    module: hir::Module,
}

fn collision_benchmark_backend(
    query_backend: wrela::query_plan::DispatchBackend,
) -> Result<wrela::query_plan::DispatchBackend, String> {
    match query_backend {
        wrela::query_plan::DispatchBackend::Cpu | wrela::query_plan::DispatchBackend::Auto => {
            Ok(wrela::query_plan::DispatchBackend::Cpu)
        }
        other => Err(format!(
            "collision benchmarks only support cpu or auto query backends, not {other:?}"
        )),
    }
}

fn perf_dispatch_backend_name(backend: wrela::query_plan::DispatchBackend) -> &'static str {
    match backend {
        wrela::query_plan::DispatchBackend::Cpu => "cpu",
        wrela::query_plan::DispatchBackend::VirtualGpu => "virtual_gpu",
        wrela::query_plan::DispatchBackend::Wgsl => "wgsl",
        wrela::query_plan::DispatchBackend::Auto => "auto",
    }
}

fn collision_benchmark_entry_path(
    benchmark_root: &Path,
    spec: &command_handlers::BenchmarkCollisionSpec,
) -> PathBuf {
    spec.entry
        .as_ref()
        .map(|entry| benchmark_root.join(entry))
        .unwrap_or_else(|| benchmark_root.join("tests").join("collision_perf_test.wr"))
}

fn compile_collision_benchmark_context(
    entry_path: &Path,
) -> Result<CollisionBenchmarkContext, String> {
    let source = fs::read_to_string(entry_path).map_err(|err| {
        format!(
            "failed to read collision benchmark source {}: {err}",
            entry_path.display()
        )
    })?;
    let node = parser::parse(&source);
    let root = ast::Root::cast(node).ok_or_else(|| {
        format!(
            "collision benchmark source {} did not parse",
            entry_path.display()
        )
    })?;
    let module = hir_lower::lower(root);
    let semantic = hir::semantic::check_module(&module);
    if !semantic.errors.is_empty() {
        return Err(format!(
            "collision benchmark semantic errors in {}: {:?}",
            entry_path.display(),
            semantic.errors
        ));
    }
    let (type_errors, type_info) = hir::typeck::check_module_with_info(&module);
    if !type_errors.is_empty() {
        return Err(format!(
            "collision benchmark type errors in {}: {type_errors:?}",
            entry_path.display()
        ));
    }
    Ok(CollisionBenchmarkContext {
        ctx: QueryExecContext::compile(&module, &type_info),
        module,
    })
}

fn run_collision_benchmark_scenario(
    context: &CollisionBenchmarkContext,
    scenario: &command_handlers::BenchmarkScenario,
    spec: &command_handlers::BenchmarkCollisionSpec,
    backend: wrela::query_plan::DispatchBackend,
) -> Result<CollisionBenchmarkScenarioResult, String> {
    let scene_id = stable_region_scene_capture_id(&SmolStr::new(spec.region.as_str()));
    let domain = collision_benchmark_domain(&context.module, &spec.domain, &spec.region)?;
    match spec.workload.as_str() {
        "point_occupancy_burst" => {
            run_collision_point_occupancy_burst(&context.ctx, scenario, scene_id, domain, backend)
        }
        "dense_ray_casts" => {
            run_collision_dense_ray_casts(&context.ctx, scenario, scene_id, domain, backend)
        }
        "overlap_burst" => {
            run_collision_overlap_burst(&context.ctx, scenario, scene_id, domain, backend)
        }
        "repeated_sweeps" => {
            run_collision_repeated_sweeps(&context.ctx, scenario, scene_id, domain, backend)
        }
        "toi_transition_reuse" => {
            run_collision_toi_transition_reuse(&context.ctx, scenario, scene_id, domain, backend)
        }
        other => Err(format!(
            "collision benchmark scenario `{}` declares unsupported workload `{other}`",
            scenario.id
        )),
    }
}

fn collision_benchmark_report_from_scenarios(
    manifest: &BenchmarkManifest,
    backend: wrela::query_plan::DispatchBackend,
    scenarios: &[CollisionBenchmarkScenarioResult],
) -> command_handlers::CollisionBenchmarkReport {
    let query_count_total = scenarios
        .iter()
        .map(|scenario| scenario.metrics.query_count)
        .sum::<u64>();
    let total_runtime_ns = scenarios
        .iter()
        .map(|scenario| scenario.metrics.total_runtime_ns)
        .sum::<u128>();
    let total_candidate_count = scenarios
        .iter()
        .map(|scenario| scenario.metrics.total_candidate_count)
        .sum::<u64>();
    let total_rejected_candidate_count = scenarios
        .iter()
        .map(|scenario| scenario.metrics.total_rejected_candidate_count)
        .sum::<u64>();
    let total_pruned_node_count = scenarios
        .iter()
        .map(|scenario| scenario.metrics.total_pruned_node_count)
        .sum::<u64>();
    let total_interval_subdivisions = scenarios
        .iter()
        .map(|scenario| scenario.metrics.total_interval_subdivisions)
        .sum::<u64>();
    let total_interval_refinements = scenarios
        .iter()
        .map(|scenario| scenario.metrics.total_interval_refinements)
        .sum::<u64>();
    let total_certificate_successes = scenarios
        .iter()
        .map(|scenario| scenario.metrics.total_certificate_successes)
        .sum::<u64>();
    let total_fallback_count = scenarios
        .iter()
        .map(|scenario| scenario.metrics.total_fallback_count)
        .sum::<u64>();
    let available_count_total = scenarios
        .iter()
        .map(|scenario| scenario.metrics.available_count_total)
        .sum::<u64>();
    let consumed_count_total = scenarios
        .iter()
        .map(|scenario| scenario.metrics.consumed_count_total)
        .sum::<u64>();
    let rejected_count_total = scenarios
        .iter()
        .map(|scenario| scenario.metrics.rejected_count_total)
        .sum::<u64>();
    let unavailable_count_total = scenarios
        .iter()
        .map(|scenario| scenario.metrics.unavailable_count_total)
        .sum::<u64>();
    let queries_per_sec = if total_runtime_ns == 0 {
        0.0
    } else {
        query_count_total as f64 / (total_runtime_ns as f64 / 1_000_000_000.0)
    };
    let reuse_total = consumed_count_total + rejected_count_total + unavailable_count_total;
    command_handlers::CollisionBenchmarkReport {
        suite: manifest.suite.clone(),
        backend: perf_dispatch_backend_name(backend).to_string(),
        command: "collision-suite".to_string(),
        query_count_total,
        total_runtime_ns,
        queries_per_sec,
        average_candidate_count: collision_average(total_candidate_count, query_count_total),
        average_rejected_candidate_count: collision_average(
            total_rejected_candidate_count,
            query_count_total,
        ),
        average_pruned_node_count: collision_average(total_pruned_node_count, query_count_total),
        average_interval_subdivisions: collision_average(
            total_interval_subdivisions,
            query_count_total,
        ),
        average_interval_refinements: collision_average(
            total_interval_refinements,
            query_count_total,
        ),
        average_certificate_successes: collision_average(
            total_certificate_successes,
            query_count_total,
        ),
        witness_reuse_rate: if reuse_total == 0 {
            0.0
        } else {
            consumed_count_total as f64 / reuse_total as f64
        },
        fallback_rate: if query_count_total == 0 {
            0.0
        } else {
            total_fallback_count as f64 / query_count_total as f64
        },
        available_count_total,
        consumed_count_total,
        rejected_count_total,
        unavailable_count_total,
        executions: scenarios
            .iter()
            .map(|scenario| scenario.execution.clone())
            .collect(),
    }
}

fn build_collision_benchmark_execution(
    scenario_id: &str,
    plan: &wrela::collision_plan::CollisionPlan,
    metrics: CollisionBenchmarkScenarioMetrics,
) -> CollisionBenchmarkScenarioResult {
    let reuse_total = metrics.consumed_count_total
        + metrics.rejected_count_total
        + metrics.unavailable_count_total;
    let queries_per_sec = if metrics.total_runtime_ns == 0 {
        0.0
    } else {
        metrics.query_count as f64 / (metrics.total_runtime_ns as f64 / 1_000_000_000.0)
    };
    CollisionBenchmarkScenarioResult {
        execution: command_handlers::CollisionBenchmarkExecutionReport {
            name: scenario_id.to_string(),
            plan_name: plan.name.to_string(),
            contract_id: plan.contract_id.as_str().to_string(),
            query_count: metrics.query_count,
            runtime_ns: metrics.total_runtime_ns,
            queries_per_sec,
            broadphase_candidate_count: collision_average_u32(
                metrics.total_candidate_count,
                metrics.query_count,
            ),
            broadphase_rejected_candidate_count: collision_average_u32(
                metrics.total_rejected_candidate_count,
                metrics.query_count,
            ),
            broadphase_pruned_node_count: collision_average_u32(
                metrics.total_pruned_node_count,
                metrics.query_count,
            ),
            interval_subdivisions: collision_average_u32(
                metrics.total_interval_subdivisions,
                metrics.query_count,
            ),
            interval_refinements: collision_average_u32(
                metrics.total_interval_refinements,
                metrics.query_count,
            ),
            certificate_successes: collision_average_u32(
                metrics.total_certificate_successes,
                metrics.query_count,
            ),
            interval_bracket: metrics.last_interval_bracket,
            fallback_count: metrics.total_fallback_count.min(u64::from(u32::MAX)) as u32,
            contact_normal_provenance: metrics.contact_normal_provenance.clone(),
            available_count: metrics.available_count_total.min(u64::from(u32::MAX)) as u32,
            consumed_count: metrics.consumed_count_total.min(u64::from(u32::MAX)) as u32,
            rejected_count: metrics.rejected_count_total.min(u64::from(u32::MAX)) as u32,
            unavailable_count: metrics.unavailable_count_total.min(u64::from(u32::MAX)) as u32,
            witness_reuse_rate: if reuse_total == 0 {
                0.0
            } else {
                metrics.consumed_count_total as f64 / reuse_total as f64
            },
            fallback_rate: if metrics.query_count == 0 {
                0.0
            } else {
                metrics.total_fallback_count as f64 / metrics.query_count as f64
            },
        },
        metrics,
    }
}

fn collision_average(total: u64, count: u64) -> f64 {
    if count == 0 {
        0.0
    } else {
        total as f64 / count as f64
    }
}

fn collision_average_u32(total: u64, count: u64) -> u32 {
    if count == 0 {
        0
    } else {
        collision_average(total, count)
            .round()
            .clamp(0.0, f64::from(u32::MAX)) as u32
    }
}

fn record_collision_trace(
    metrics: &mut CollisionBenchmarkScenarioMetrics,
    runtime_ns: u128,
    trace: &wrela::collision_plan::CollisionExecutionTrace,
) {
    metrics.query_count = metrics.query_count.saturating_add(1);
    metrics.total_runtime_ns = metrics.total_runtime_ns.saturating_add(runtime_ns);
    metrics.total_candidate_count = metrics
        .total_candidate_count
        .saturating_add(u64::from(trace.broadphase_candidate_count));
    metrics.total_rejected_candidate_count = metrics
        .total_rejected_candidate_count
        .saturating_add(u64::from(trace.broadphase_rejected_candidate_count));
    metrics.total_pruned_node_count = metrics
        .total_pruned_node_count
        .saturating_add(u64::from(trace.broadphase_pruned_node_count));
    metrics.total_interval_subdivisions = metrics
        .total_interval_subdivisions
        .saturating_add(u64::from(trace.interval_subdivisions));
    metrics.total_interval_refinements = metrics
        .total_interval_refinements
        .saturating_add(u64::from(trace.interval_refinements));
    metrics.total_certificate_successes = metrics
        .total_certificate_successes
        .saturating_add(u64::from(trace.certificate_successes));
    metrics.total_fallback_count = metrics
        .total_fallback_count
        .saturating_add(u64::from(trace.fallback_count));
    metrics.available_count_total = metrics
        .available_count_total
        .saturating_add(u64::from(trace.reuse_metrics.available_count));
    metrics.consumed_count_total = metrics
        .consumed_count_total
        .saturating_add(u64::from(trace.reuse_metrics.consumed_count));
    metrics.rejected_count_total = metrics
        .rejected_count_total
        .saturating_add(u64::from(trace.reuse_metrics.rejected_count));
    metrics.unavailable_count_total = metrics
        .unavailable_count_total
        .saturating_add(u64::from(trace.reuse_metrics.unavailable_count));
    if let Some(bracket) = trace.interval_bracket {
        metrics.last_interval_bracket = Some(match metrics.last_interval_bracket {
            Some(current) => [current[0].min(bracket[0]), current[1].max(bracket[1])],
            None => bracket,
        });
    }
    let provenance = trace
        .contact_normal_provenance
        .map(wrela::collision_contract::collision_contact_normal_provenance_name)
        .map(str::to_string)
        .unwrap_or_else(|| "none".to_string());
    match metrics.contact_normal_provenance.as_deref() {
        None => metrics.contact_normal_provenance = Some(provenance),
        Some(existing) if existing == provenance => {}
        Some("mixed") => {}
        Some(_) => metrics.contact_normal_provenance = Some("mixed".to_string()),
    }
}

fn collision_benchmark_domain(
    module: &hir::Module,
    domain_name: &str,
    region_name: &str,
) -> Result<KernelValue, String> {
    let domain = module
        .functions
        .iter()
        .find(|(_, func)| func.name == domain_name && func.role == hir::FunctionRole::Domain)
        .map(|(_, func)| func)
        .ok_or_else(|| format!("missing collision benchmark domain `{domain_name}`"))?;
    let metadata = domain
        .domain
        .as_ref()
        .ok_or_else(|| format!("collision benchmark domain `{domain_name}` is missing metadata"))?;
    let _execution_policy = domain.domain_execution_policy.as_ref().ok_or_else(|| {
        format!(
            "collision benchmark domain `{domain_name}` is missing lowered execution policy metadata"
        )
    })?;
    let geometry_detail = match metadata.geometry_detail {
        hir::DomainGeometryDetail::Coarse => 0,
        hir::DomainGeometryDetail::Fine => 1,
    };
    Ok(wrela::presentation_exec::scene_domain_value(
        stable_region_scene_capture_id(&SmolStr::new(region_name)),
        geometry_detail,
        metadata.material,
        metadata.radiance,
        metadata.media,
    ))
}

fn collision_benchmark_capture(scene_id: u32, epoch: u32) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("RegionCapture"),
        fields: vec![
            (SmolStr::new("scene_id"), KernelValue::U32(scene_id)),
            (SmolStr::new("epoch"), KernelValue::U32(epoch)),
        ],
    })
}

fn collision_benchmark_transition(current_epoch: u32, previous_epoch: u32) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("CollisionSnapshotTransitionInput"),
        fields: vec![
            (
                SmolStr::new("current_snapshot_epoch"),
                KernelValue::U32(current_epoch),
            ),
            (
                SmolStr::new("previous_snapshot_epoch"),
                KernelValue::U32(previous_epoch),
            ),
            (
                SmolStr::new("change_class"),
                KernelValue::U32(wrela::state_advance::ChangeClass::Presentation as u32),
            ),
        ],
    })
}

fn collision_benchmark_point(point: [f32; 3]) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("CollisionPointInput"),
        fields: vec![(SmolStr::new("point"), KernelValue::Vec3(point))],
    })
}

fn collision_benchmark_ray(origin: [f32; 3], direction: [f32; 3]) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("CollisionRayInput"),
        fields: vec![
            (SmolStr::new("origin"), KernelValue::Vec3(origin)),
            (SmolStr::new("direction"), KernelValue::Vec3(direction)),
            (SmolStr::new("max_distance"), KernelValue::F32(12.0)),
            (SmolStr::new("min_step"), KernelValue::F32(0.05)),
            (SmolStr::new("hit_epsilon"), KernelValue::F32(0.001)),
            (SmolStr::new("max_steps"), KernelValue::I32(96)),
        ],
    })
}

fn collision_benchmark_probe(center: [f32; 3], radius: f32) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("CollisionSphereProbe"),
        fields: vec![
            (SmolStr::new("center"), KernelValue::Vec3(center)),
            (SmolStr::new("radius"), KernelValue::F32(radius)),
        ],
    })
}

fn collision_benchmark_sweep(
    start_center: [f32; 3],
    end_center: [f32; 3],
    radius: f32,
) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("CollisionSphereSweepInput"),
        fields: vec![
            (
                SmolStr::new("start_center"),
                KernelValue::Vec3(start_center),
            ),
            (SmolStr::new("end_center"), KernelValue::Vec3(end_center)),
            (SmolStr::new("radius"), KernelValue::F32(radius)),
            (SmolStr::new("contact_tolerance"), KernelValue::F32(0.001)),
            (SmolStr::new("max_iterations"), KernelValue::I32(64)),
        ],
    })
}

fn collision_transition_probe_offset(step: u64) -> [f32; 3] {
    [
        (step % 9) as f32 * 0.04 - 0.16,
        ((step / 9) % 5) as f32 * 0.03 - 0.06,
        (step % 4) as f32 * -0.02,
    ]
}

fn run_collision_point_occupancy_burst(
    ctx: &QueryExecContext,
    scenario: &command_handlers::BenchmarkScenario,
    scene_id: u32,
    domain: KernelValue,
    backend: wrela::query_plan::DispatchBackend,
) -> Result<CollisionBenchmarkScenarioResult, String> {
    let plan = wrela::collision_plan::CollisionPlan::for_query_with_backend(
        wrela::collision_plan::CollisionQueryKind::PointOccupancyWorld,
        backend,
    );
    let capture = collision_benchmark_capture(scene_id, 2);
    let mut metrics = CollisionBenchmarkScenarioMetrics::default();
    for i in 1..=scenario.ops {
        let point = [
            (i % 16) as f32 * 0.08 - 0.60,
            ((i / 16) % 10) as f32 * 0.06 - 0.24,
            (i % 5) as f32 * 0.04 - 0.08,
        ];
        let started = Instant::now();
        let (_, trace) = plan
            .execute(
                ctx,
                &[
                    capture.clone(),
                    domain.clone(),
                    collision_benchmark_point(point),
                ],
            )
            .map_err(|err| format!("collision benchmark `{}` failed: {err}", scenario.id))?;
        record_collision_trace(&mut metrics, started.elapsed().as_nanos(), &trace);
    }
    Ok(build_collision_benchmark_execution(
        &scenario.id,
        &plan,
        metrics,
    ))
}

fn run_collision_dense_ray_casts(
    ctx: &QueryExecContext,
    scenario: &command_handlers::BenchmarkScenario,
    scene_id: u32,
    domain: KernelValue,
    backend: wrela::query_plan::DispatchBackend,
) -> Result<CollisionBenchmarkScenarioResult, String> {
    let plan = wrela::collision_plan::CollisionPlan::for_query_with_backend(
        wrela::collision_plan::CollisionQueryKind::RayCastWorld,
        backend,
    );
    let capture = collision_benchmark_capture(scene_id, 2);
    let mut metrics = CollisionBenchmarkScenarioMetrics::default();
    for i in 1..=scenario.ops {
        let origin = [
            (i % 12) as f32 * 0.12 - 0.66,
            ((i / 12) % 8) as f32 * 0.08 - 0.28,
            3.2 + (i % 3) as f32 * 0.02,
        ];
        let direction = [
            ((i % 5) as i32 - 2) as f32 * 0.04,
            ((i % 7) as i32 - 3) as f32 * -0.03,
            -1.0,
        ];
        let started = Instant::now();
        let (_, trace) = plan
            .execute(
                ctx,
                &[
                    capture.clone(),
                    domain.clone(),
                    collision_benchmark_ray(origin, direction),
                ],
            )
            .map_err(|err| format!("collision benchmark `{}` failed: {err}", scenario.id))?;
        record_collision_trace(&mut metrics, started.elapsed().as_nanos(), &trace);
    }
    Ok(build_collision_benchmark_execution(
        &scenario.id,
        &plan,
        metrics,
    ))
}

fn run_collision_overlap_burst(
    ctx: &QueryExecContext,
    scenario: &command_handlers::BenchmarkScenario,
    scene_id: u32,
    domain: KernelValue,
    backend: wrela::query_plan::DispatchBackend,
) -> Result<CollisionBenchmarkScenarioResult, String> {
    let plan = wrela::collision_plan::CollisionPlan::for_query_with_backend(
        wrela::collision_plan::CollisionQueryKind::SphereOverlapWorld,
        backend,
    );
    let capture = collision_benchmark_capture(scene_id, 2);
    let mut metrics = CollisionBenchmarkScenarioMetrics::default();
    for i in 1..=scenario.ops {
        let (anchor_x, anchor_y) = match i % 3 {
            0 => (0.0, 0.0),
            1 => (1.08, 0.02),
            _ => (-1.38, -0.06),
        };
        let center = [
            anchor_x + (i % 6) as f32 * 0.04 - 0.10,
            anchor_y + ((i / 6) % 5) as f32 * 0.03 - 0.06,
            (i % 4) as f32 * 0.03 - 0.05,
        ];
        let radius = 0.16 + (i % 4) as f32 * 0.02;
        let started = Instant::now();
        let (_, trace) = plan
            .execute(
                ctx,
                &[
                    capture.clone(),
                    domain.clone(),
                    collision_benchmark_probe(center, radius),
                ],
            )
            .map_err(|err| format!("collision benchmark `{}` failed: {err}", scenario.id))?;
        record_collision_trace(&mut metrics, started.elapsed().as_nanos(), &trace);
    }
    Ok(build_collision_benchmark_execution(
        &scenario.id,
        &plan,
        metrics,
    ))
}

fn run_collision_repeated_sweeps(
    ctx: &QueryExecContext,
    scenario: &command_handlers::BenchmarkScenario,
    scene_id: u32,
    domain: KernelValue,
    backend: wrela::query_plan::DispatchBackend,
) -> Result<CollisionBenchmarkScenarioResult, String> {
    let plan = wrela::collision_plan::CollisionPlan::for_query_with_backend(
        wrela::collision_plan::CollisionQueryKind::SphereSweepTransition,
        backend,
    );
    let mut metrics = CollisionBenchmarkScenarioMetrics::default();
    let mut store = wrela::collision_exec::cpu::CollisionArtifactStore::default();
    for i in 1..=scenario.ops {
        let cycle_epoch = ((i - 1) % 255 + 1) as u32;
        if cycle_epoch == 1 {
            store = wrela::collision_exec::cpu::CollisionArtifactStore::default();
        }
        let previous_epoch = if cycle_epoch == 1 { 0 } else { cycle_epoch - 1 };
        let offset = collision_transition_probe_offset(i);
        let start_center = [offset[0], offset[1], 2.9 + offset[2]];
        let end_center = [offset[0] + 0.05, offset[1] - 0.03, -1.1 + offset[2]];
        let started = Instant::now();
        let (_, trace) = wrela::collision_exec::cpu::execute_with_store(
            &plan,
            ctx,
            &[
                collision_benchmark_capture(scene_id, cycle_epoch),
                domain.clone(),
                collision_benchmark_transition(cycle_epoch, previous_epoch),
                collision_benchmark_sweep(start_center, end_center, 0.25),
            ],
            &mut store,
        )
        .map_err(|err| format!("collision benchmark `{}` failed: {err}", scenario.id))?;
        record_collision_trace(&mut metrics, started.elapsed().as_nanos(), &trace);
    }
    Ok(build_collision_benchmark_execution(
        &scenario.id,
        &plan,
        metrics,
    ))
}

fn run_collision_toi_transition_reuse(
    ctx: &QueryExecContext,
    scenario: &command_handlers::BenchmarkScenario,
    scene_id: u32,
    domain: KernelValue,
    backend: wrela::query_plan::DispatchBackend,
) -> Result<CollisionBenchmarkScenarioResult, String> {
    let plan = wrela::collision_plan::CollisionPlan::for_query_with_backend(
        wrela::collision_plan::CollisionQueryKind::SphereTimeOfImpactTransition,
        backend,
    );
    let mut metrics = CollisionBenchmarkScenarioMetrics::default();
    let mut store = wrela::collision_exec::cpu::CollisionArtifactStore::default();
    for i in 1..=scenario.ops {
        let cycle_epoch = ((i - 1) % 255 + 1) as u32;
        if cycle_epoch == 1 {
            store = wrela::collision_exec::cpu::CollisionArtifactStore::default();
        }
        let previous_epoch = if cycle_epoch == 1 { 0 } else { cycle_epoch - 1 };
        let offset = collision_transition_probe_offset(i);
        let start_center = [offset[0], offset[1], 2.4 + offset[2]];
        let end_center = [offset[0] + 0.04, offset[1] - 0.02, -0.9 + offset[2]];
        let started = Instant::now();
        let (_, trace) = wrela::collision_exec::cpu::execute_with_store(
            &plan,
            ctx,
            &[
                collision_benchmark_capture(scene_id, cycle_epoch),
                domain.clone(),
                collision_benchmark_transition(cycle_epoch, previous_epoch),
                collision_benchmark_sweep(start_center, end_center, 0.20),
            ],
            &mut store,
        )
        .map_err(|err| format!("collision benchmark `{}` failed: {err}", scenario.id))?;
        record_collision_trace(&mut metrics, started.elapsed().as_nanos(), &trace);
    }
    Ok(build_collision_benchmark_execution(
        &scenario.id,
        &plan,
        metrics,
    ))
}

fn collision_runtime_cases_from_report(
    report: &command_handlers::CollisionBenchmarkReport,
) -> Vec<(String, String, u128)> {
    report
        .executions
        .iter()
        .map(|execution| {
            (
                format!("{}::{}", execution.contract_id, execution.name),
                execution.name.clone(),
                execution.runtime_ns,
            )
        })
        .collect()
}

fn run_presentation_benchmark_report(
    current_exe: &Path,
    benchmark_root: &Path,
    scenario: &command_handlers::BenchmarkScenario,
    spec: &command_handlers::BenchmarkPresentationSpec,
    query_backend: wrela::query_plan::DispatchBackend,
) -> Result<PresentationBenchmarkReport, String> {
    if !matches!(query_backend, wrela::query_plan::DispatchBackend::Wgsl) {
        let hybrid = run_presentation_benchmark_report_for_mode(
            current_exe,
            benchmark_root,
            scenario,
            spec,
            QueryTraceSolverMode::Hybrid,
            None,
            query_backend,
        )?;
        let dense_only = run_presentation_benchmark_report_for_mode(
            current_exe,
            benchmark_root,
            scenario,
            spec,
            QueryTraceSolverMode::DenseOnly,
            None,
            query_backend,
        )?;
        let mut report = presentation_report_from_debug_output(scenario, hybrid);
        report.ab_comparison = Some(presentation_comparison_from_debug_reports(
            &report,
            &dense_only,
        ));
        return Ok(report);
    }
    let hybrid_candidates = run_presentation_benchmark_reports_for_workgroup_sizes(
        current_exe,
        benchmark_root,
        scenario,
        spec,
        QueryTraceSolverMode::Hybrid,
        query_backend,
    )?;
    let Some(best_hybrid) = hybrid_candidates
        .iter()
        .min_by_key(|report| report.frame_time_ns)
        .cloned()
    else {
        return Err(format!(
            "presentation-debug produced no hybrid workgroup candidates for scenario `{}`",
            scenario.id
        ));
    };
    let workgroup_comparison =
        presentation_workgroup_comparison_from_reports(&hybrid_candidates, &best_hybrid);
    let dense_only = run_presentation_benchmark_report_for_mode(
        current_exe,
        benchmark_root,
        scenario,
        spec,
        QueryTraceSolverMode::DenseOnly,
        Some(best_hybrid.selected_workgroup_size),
        query_backend,
    )?;
    let mut report = best_hybrid;
    report.wgsl_workgroup_comparison = Some(workgroup_comparison);
    report.ab_comparison = Some(presentation_comparison_from_debug_reports(
        &report,
        &dense_only,
    ));
    Ok(report)
}

fn run_presentation_benchmark_reports_for_workgroup_sizes(
    current_exe: &Path,
    benchmark_root: &Path,
    scenario: &command_handlers::BenchmarkScenario,
    spec: &command_handlers::BenchmarkPresentationSpec,
    query_trace_solver_mode: QueryTraceSolverMode,
    query_backend: wrela::query_plan::DispatchBackend,
) -> Result<Vec<PresentationBenchmarkReport>, String> {
    let supported_workgroup_sizes = wrela::query_exec::supported_wgsl_workgroup_sizes()
        .map_err(|err| format!("failed to enumerate supported WGSL workgroup sizes: {err}"))?;
    if supported_workgroup_sizes.is_empty() {
        return Err(format!(
            "no supported legal WGSL workgroup sizes were available for scenario `{}`",
            scenario.id
        ));
    }
    let mut reports = Vec::new();
    for workgroup_size in supported_workgroup_sizes {
        let dump = run_presentation_benchmark_report_for_mode(
            current_exe,
            benchmark_root,
            scenario,
            spec,
            query_trace_solver_mode,
            Some(workgroup_size),
            query_backend,
        )?;
        let report = presentation_report_from_debug_output(scenario, dump);
        if report.selected_workgroup_size != workgroup_size {
            return Err(format!(
                "presentation-debug reported workgroup size {} for scenario `{}` while benchmarking override {}",
                report.selected_workgroup_size, scenario.id, workgroup_size
            ));
        }
        reports.push(report);
    }
    Ok(reports)
}

fn run_presentation_benchmark_report_for_mode(
    current_exe: &Path,
    benchmark_root: &Path,
    scenario: &command_handlers::BenchmarkScenario,
    spec: &command_handlers::BenchmarkPresentationSpec,
    query_trace_solver_mode: QueryTraceSolverMode,
    workgroup_size_override: Option<u32>,
    query_backend: wrela::query_plan::DispatchBackend,
) -> Result<PresentationDebugCommandOutput, String> {
    let presentation_target = spec
        .entry
        .as_ref()
        .map(|entry| benchmark_root.join(entry))
        .unwrap_or_else(|| benchmark_root.to_path_buf());
    let mut command = build_presentation_debug_command(
        current_exe,
        &presentation_target,
        spec,
        query_trace_solver_mode,
        workgroup_size_override,
        query_backend,
    );
    let timeout = Duration::from_millis(scenario.timeout_ms.unwrap_or(60_000));
    let output = run_command_with_timeout(&mut command, timeout).map_err(|err| {
        format!(
            "failed to launch presentation-debug for scenario `{}` in mode `{}`: {err}",
            scenario.id,
            query_trace_solver_mode.as_str()
        )
    })?;
    if !output.status.success() {
        return Err(format!(
            "presentation-debug failed for scenario `{}` in mode `{}`: stdout={} stderr={}",
            scenario.id,
            query_trace_solver_mode.as_str(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let dump: PresentationDebugCommandOutput =
        serde_json::from_slice(&output.stdout).map_err(|err| {
            format!(
                "failed to parse presentation-debug JSON for scenario `{}` in mode `{}`: {err}",
                scenario.id,
                query_trace_solver_mode.as_str()
            )
        })?;
    Ok(dump)
}

fn build_presentation_debug_command(
    current_exe: &Path,
    presentation_target: &Path,
    spec: &command_handlers::BenchmarkPresentationSpec,
    query_trace_solver_mode: QueryTraceSolverMode,
    workgroup_size_override: Option<u32>,
    query_backend: wrela::query_plan::DispatchBackend,
) -> Command {
    let mut command = Command::new(current_exe);
    command
        .arg("--json")
        .arg(format!(
            "--query-backend={}",
            perf_dispatch_backend_name(query_backend)
        ))
        .arg("presentation-debug")
        .arg(presentation_target)
        .args(presentation_debug_args(spec, query_trace_solver_mode));
    if let Some(workgroup_size) = workgroup_size_override {
        command.env(WGSL_WORKGROUP_SIZE_OVERRIDE_ENV, workgroup_size.to_string());
    } else {
        command.env_remove(WGSL_WORKGROUP_SIZE_OVERRIDE_ENV);
    }
    command
}

fn run_command_with_timeout(
    command: &mut Command,
    timeout: Duration,
) -> Result<std::process::Output, String> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("failed to spawn command: {err}"))?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                return child
                    .wait_with_output()
                    .map_err(|err| format!("failed to collect command output: {err}"));
            }
            Ok(None) => {}
            Err(err) => return Err(format!("failed to poll command status: {err}")),
        }
        if started.elapsed() >= timeout {
            #[cfg(unix)]
            {
                let process_group = child.id() as i32;
                unsafe {
                    libc::killpg(process_group, libc::SIGTERM);
                }
                thread::sleep(Duration::from_millis(100));
                unsafe {
                    libc::killpg(process_group, libc::SIGKILL);
                }
            }
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("command timed out after {:?}", timeout));
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn presentation_report_from_debug_output(
    scenario: &command_handlers::BenchmarkScenario,
    dump: PresentationDebugCommandOutput,
) -> PresentationBenchmarkReport {
    let measured_history = if scenario.class.eq_ignore_ascii_case("closure") {
        closure_measured_presentation_frame_history(&dump.frame_cost, &dump.frame_cost_history)
    } else {
        presentation_frame_history(&dump.frame_cost, &dump.frame_cost_history)
    };
    let measured_frames_executed = measured_history
        .len()
        .min(dump.frames_executed.max(1) as usize);
    let aggregate = aggregate_presentation_frame_metrics(&measured_history);
    let quality_history = measured_history
        .iter()
        .map(|frame| frame.quality.tier.clone())
        .collect();
    let internal_resolution_history = measured_history
        .iter()
        .map(|frame| frame.quality.internal_resolution_scale)
        .collect();
    let reconstructed_output = measured_history
        .iter()
        .any(|frame| frame.quality.reconstructed_output);
    let active_acceleration_artifacts = measured_history
        .iter()
        .flat_map(|frame| frame.active_acceleration_artifacts.iter().cloned())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let performance_gain_sources = measured_history
        .iter()
        .flat_map(|frame| frame.performance_gain_sources.iter().cloned())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    PresentationBenchmarkReport {
        scenario_id: scenario.id.clone(),
        test_name: scenario.test_name.clone(),
        view: dump.view,
        region: dump.region,
        domain: dump.domain,
        observed_adapter_name: dump
            .backend
            .eq_ignore_ascii_case("wgsl")
            .then(observed_wgsl_adapter_name)
            .flatten(),
        backend: dump.backend,
        query_trace_solver_mode: dump.query_trace_solver_mode,
        selected_workgroup_size: dump.frame_cost.selected_workgroup_size,
        frames_executed: measured_frames_executed as u32,
        frame_time_ns: aggregate.frame_time_ns,
        field_samples: aggregate.field_samples,
        quality_tier: dump.frame_cost.quality.tier.clone(),
        target_fps: dump.frame_cost.quality.target_fps,
        internal_resolution_scale: dump.frame_cost.quality.internal_resolution_scale,
        reconstructed_output,
        quality_history,
        internal_resolution_history,
        bottleneck_pass: dump.frame_cost.bottleneck_pass.clone(),
        active_acceleration_artifacts,
        performance_gain_sources,
        frame_cost: dump.frame_cost,
        frame_cost_history: measured_history,
        wgsl_workgroup_comparison: None,
        ab_comparison: None,
    }
}

fn observed_wgsl_adapter_name() -> Option<String> {
    shared_wgpu_context(GpuLimitRequest::default())
        .ok()
        .map(|context| context.adapter_info.name.trim().to_string())
        .filter(|name| !name.is_empty())
}

fn presentation_comparison_from_debug_reports(
    hybrid: &PresentationBenchmarkReport,
    dense_only: &PresentationDebugCommandOutput,
) -> PresentationBenchmarkComparison {
    let hybrid_history = presentation_frame_history(&hybrid.frame_cost, &hybrid.frame_cost_history);
    let hybrid_metrics = aggregate_presentation_frame_metrics(&hybrid_history);
    let dense_only_metrics =
        aggregate_presentation_frame_metrics(&aligned_presentation_frame_history(
            &dense_only.frame_cost,
            &dense_only.frame_cost_history,
            hybrid_history.len(),
        ));
    let frame_time_ns_delta_vs_dense_only =
        hybrid_metrics.frame_time_ns as i128 - dense_only_metrics.frame_time_ns as i128;
    let frame_time_ns_delta_vs_dense_only_pct = if dense_only_metrics.frame_time_ns == 0 {
        0.0
    } else {
        (frame_time_ns_delta_vs_dense_only as f64 / dense_only_metrics.frame_time_ns as f64) * 100.0
    };
    PresentationBenchmarkComparison {
        dense_only_query_trace_solver_mode: dense_only.query_trace_solver_mode.clone(),
        dense_only_frame_time_ns: dense_only_metrics.frame_time_ns,
        frame_time_ns_delta_vs_dense_only,
        frame_time_ns_delta_vs_dense_only_pct,
        dense_only_average_trace_steps: dense_only_metrics.average_trace_steps,
        average_trace_steps_delta_vs_dense_only: hybrid_metrics.average_trace_steps
            - dense_only_metrics.average_trace_steps,
        dense_only_field_samples: dense_only_metrics.field_samples,
        field_samples_delta_vs_dense_only: hybrid_metrics.field_samples as i64
            - dense_only_metrics.field_samples as i64,
        dense_only_candidate_count_before_pruning: dense_only_metrics
            .candidate_count_before_pruning,
        candidate_count_before_pruning_delta_vs_dense_only: hybrid_metrics
            .candidate_count_before_pruning
            as i64
            - dense_only_metrics.candidate_count_before_pruning as i64,
        dense_only_candidate_count_after_pruning: dense_only_metrics.candidate_count_after_pruning,
        candidate_count_after_pruning_delta_vs_dense_only: hybrid_metrics
            .candidate_count_after_pruning
            as i64
            - dense_only_metrics.candidate_count_after_pruning as i64,
    }
}

fn presentation_workgroup_comparison_from_reports(
    candidate_reports: &[PresentationBenchmarkReport],
    selected: &PresentationBenchmarkReport,
) -> PresentationWgslWorkgroupComparison {
    let selected_frame_time_ns = selected.frame_time_ns;
    let candidate_workgroup_sizes = candidate_reports
        .iter()
        .map(|report| report.selected_workgroup_size)
        .collect::<Vec<_>>();
    let candidate_frame_time_ns = candidate_reports
        .iter()
        .map(|report| report.frame_time_ns)
        .collect::<Vec<_>>();
    let frame_time_ns_delta_vs_selected = candidate_frame_time_ns
        .iter()
        .map(|candidate_frame_time_ns| {
            *candidate_frame_time_ns as i128 - selected_frame_time_ns as i128
        })
        .collect::<Vec<_>>();
    let frame_time_ns_delta_vs_selected_pct = candidate_frame_time_ns
        .iter()
        .map(|candidate_frame_time_ns| {
            if selected_frame_time_ns == 0 {
                0.0
            } else {
                ((*candidate_frame_time_ns as f64 - selected_frame_time_ns as f64)
                    / selected_frame_time_ns as f64)
                    * 100.0
            }
        })
        .collect::<Vec<_>>();
    PresentationWgslWorkgroupComparison {
        selected_workgroup_size: selected.selected_workgroup_size,
        candidate_workgroup_sizes,
        candidate_frame_time_ns,
        frame_time_ns_delta_vs_selected,
        frame_time_ns_delta_vs_selected_pct,
    }
}

fn presentation_frame_history(
    frame_cost: &wrela::presentation_exec::PresentationFrameCostReport,
    frame_cost_history: &[wrela::presentation_exec::PresentationFrameCostReport],
) -> Vec<wrela::presentation_exec::PresentationFrameCostReport> {
    if frame_cost_history.is_empty() {
        vec![frame_cost.clone()]
    } else {
        frame_cost_history.to_vec()
    }
}

fn closure_measured_presentation_frame_history(
    frame_cost: &wrela::presentation_exec::PresentationFrameCostReport,
    frame_cost_history: &[wrela::presentation_exec::PresentationFrameCostReport],
) -> Vec<wrela::presentation_exec::PresentationFrameCostReport> {
    let frame_history = presentation_frame_history(frame_cost, frame_cost_history);
    if frame_history.len() > 1 {
        // Treat the first frame in a multi-frame subprocess sample as an
        // in-process warmup so steady-state closure metrics exclude first-use
        // pipeline compilation and resident-scene upload cost.
        frame_history.into_iter().skip(1).collect()
    } else {
        frame_history
    }
}

fn aligned_presentation_frame_history(
    frame_cost: &wrela::presentation_exec::PresentationFrameCostReport,
    frame_cost_history: &[wrela::presentation_exec::PresentationFrameCostReport],
    measured_frame_count: usize,
) -> Vec<wrela::presentation_exec::PresentationFrameCostReport> {
    let frame_history = presentation_frame_history(frame_cost, frame_cost_history);
    if measured_frame_count == 0 || measured_frame_count >= frame_history.len() {
        frame_history
    } else {
        frame_history[frame_history.len() - measured_frame_count..].to_vec()
    }
}

fn aggregate_presentation_frame_metrics(
    frames: &[wrela::presentation_exec::PresentationFrameCostReport],
) -> PresentationAggregatedMetrics {
    let mut frame_time_ns = 0u128;
    let mut field_samples = 0u64;
    let mut candidate_count_before_pruning = 0u64;
    let mut candidate_count_after_pruning = 0u64;
    let mut weighted_trace_steps = 0.0f64;
    let mut sample_weight = 0u64;
    for frame in frames {
        frame_time_ns = frame_time_ns.saturating_add(frame_cost_total_ns(frame));
        field_samples = field_samples.saturating_add(u64::from(frame.field_samples));
        candidate_count_before_pruning = candidate_count_before_pruning
            .saturating_add(u64::from(frame.candidate_count_before_pruning));
        candidate_count_after_pruning = candidate_count_after_pruning
            .saturating_add(u64::from(frame.candidate_count_after_pruning));
        let frame_samples =
            u64::from(frame.output_width.max(1)) * u64::from(frame.output_height.max(1));
        sample_weight = sample_weight.saturating_add(frame_samples);
        weighted_trace_steps += frame.average_trace_steps as f64 * frame_samples as f64;
    }
    PresentationAggregatedMetrics {
        frame_time_ns,
        field_samples: field_samples.min(u64::from(u32::MAX)) as u32,
        average_trace_steps: if sample_weight == 0 {
            0.0
        } else {
            (weighted_trace_steps / sample_weight as f64) as f32
        },
        candidate_count_before_pruning: candidate_count_before_pruning.min(u64::from(u32::MAX))
            as u32,
        candidate_count_after_pruning: candidate_count_after_pruning.min(u64::from(u32::MAX))
            as u32,
    }
}

fn aggregate_presentation_solver_counters(
    frames: &[wrela::presentation_exec::PresentationFrameCostReport],
) -> PresentationAggregatedSolverCounters {
    let mut counters = PresentationAggregatedSolverCounters::default();
    for frame in frames {
        counters.solver_relaxed_attempts = counters
            .solver_relaxed_attempts
            .saturating_add(u64::from(frame.solver_relaxed_attempts));
        counters.solver_relaxed_no_root_advances = counters
            .solver_relaxed_no_root_advances
            .saturating_add(u64::from(frame.solver_relaxed_no_root_advances));
        counters.solver_relaxed_brackets = counters
            .solver_relaxed_brackets
            .saturating_add(u64::from(frame.solver_relaxed_brackets));
        counters.solver_relaxed_unresolved = counters
            .solver_relaxed_unresolved
            .saturating_add(u64::from(frame.solver_relaxed_unresolved));
        counters.solver_interval_attempts = counters
            .solver_interval_attempts
            .saturating_add(u64::from(frame.solver_interval_attempts));
        counters.solver_interval_no_root_advances = counters
            .solver_interval_no_root_advances
            .saturating_add(u64::from(frame.solver_interval_no_root_advances));
        counters.solver_interval_brackets = counters
            .solver_interval_brackets
            .saturating_add(u64::from(frame.solver_interval_brackets));
        counters.solver_interval_unresolved = counters
            .solver_interval_unresolved
            .saturating_add(u64::from(frame.solver_interval_unresolved));
        counters.solver_refinement_attempts = counters
            .solver_refinement_attempts
            .saturating_add(u64::from(frame.solver_refinement_attempts));
        counters.solver_refinement_failures = counters
            .solver_refinement_failures
            .saturating_add(u64::from(frame.solver_refinement_failures));
        counters.solver_repeat_attempts = counters
            .solver_repeat_attempts
            .saturating_add(u64::from(frame.solver_repeat_attempts));
        counters.solver_repeat_supported = counters
            .solver_repeat_supported
            .saturating_add(u64::from(frame.solver_repeat_supported));
        counters.solver_repeat_inapplicable = counters
            .solver_repeat_inapplicable
            .saturating_add(u64::from(frame.solver_repeat_inapplicable));
        counters.solver_repeat_unsupported = counters
            .solver_repeat_unsupported
            .saturating_add(u64::from(frame.solver_repeat_unsupported));
        counters.solver_repeat_unsupported_form = counters
            .solver_repeat_unsupported_form
            .saturating_add(u64::from(frame.solver_repeat_unsupported_form));
        counters.solver_repeat_unsupported_bounds = counters
            .solver_repeat_unsupported_bounds
            .saturating_add(u64::from(frame.solver_repeat_unsupported_bounds));
        counters.solver_repeat_cells_enumerated = counters
            .solver_repeat_cells_enumerated
            .saturating_add(u64::from(frame.solver_repeat_cells_enumerated));
    }
    counters
}

fn presentation_debug_args(
    spec: &command_handlers::BenchmarkPresentationSpec,
    query_trace_solver_mode: QueryTraceSolverMode,
) -> Vec<String> {
    let mut args = vec![
        "--no-export".to_string(),
        "--solver-mode".to_string(),
        query_trace_solver_mode.as_str().to_string(),
        "--view".to_string(),
        spec.view.clone(),
        "--region".to_string(),
        spec.region.clone(),
        "--width".to_string(),
        spec.width.unwrap_or(64).to_string(),
        "--height".to_string(),
        spec.height.unwrap_or(64).to_string(),
        "--frames".to_string(),
        spec.frames.unwrap_or(1).max(1).to_string(),
        "--camera-position".to_string(),
        format_vec3(spec.camera_position),
        "--camera-forward".to_string(),
        format_vec3(spec.camera_forward),
        "--camera-up".to_string(),
        format_vec3(spec.camera_up),
        "--fov".to_string(),
        spec.vertical_fov_degrees.to_string(),
    ];
    if let Some(domain) = spec.domain.as_ref() {
        args.push("--domain".to_string());
        args.push(domain.clone());
    }
    args
}

fn format_vec3(value: [f32; 3]) -> String {
    format!("{},{},{}", value[0], value[1], value[2])
}

fn resolve_perf_benchmark_manifest_path(
    target: &TestTarget,
    override_path: Option<String>,
    profile: PerfProfile,
) -> Option<PathBuf> {
    if let Some(path) = override_path {
        return Some(PathBuf::from(path));
    }
    let TestTarget::ProjectRoot(root) = target else {
        return None;
    };
    let closure_candidate = root.join("1080p120_closure.toml");
    if matches!(profile, PerfProfile::Closure1080p120) && closure_candidate.is_file() {
        return Some(closure_candidate);
    }
    let candidate = root.join("bench.toml");
    candidate.is_file().then_some(candidate)
}

fn frame_cost_total_ns(report: &wrela::presentation_exec::PresentationFrameCostReport) -> u128 {
    report
        .passes
        .iter()
        .map(|pass| pass.elapsed_micros)
        .sum::<u128>()
        * 1_000
}

fn build_closure_report(
    profile: &PerfClosureProfile,
    manifest: Option<&BenchmarkManifest>,
    samples: &[command_handlers::PerfSummary],
    presentation_reports: &[PresentationBenchmarkReport],
    presentation_report_errors: &[String],
    collision_reports: &[CollisionBenchmarkReport],
    perf_profile: PerfProfile,
    observed_warmup_runs: usize,
    observed_measured_runs: usize,
) -> PerfClosureReport {
    let sampled_suite = if matches!(perf_profile, PerfProfile::Closure1080p120) {
        manifest.map(|manifest| manifest.suite.as_str())
    } else {
        None
    };
    let mut report = PerfClosureReport::unsampled(profile.clone());
    apply_observed_wgsl_runtime_metadata(&mut report.profile, presentation_reports);
    report.cpu_oracle_profile = Some(match profile.execution_story {
        PerfClosureExecutionStory::CpuOracle => {
            PerfClosureProfile::canonical_1080p120_wgsl_resident()
        }
        PerfClosureExecutionStory::WgslResident => {
            PerfClosureProfile::canonical_1080p120_cpu_oracle()
        }
    });
    if sampled_suite.is_some_and(|suite| suite.eq_ignore_ascii_case(profile.frame.suite.as_str())) {
        report.frame = build_frame_closure_status(
            profile,
            presentation_reports,
            presentation_report_errors,
            observed_warmup_runs,
            observed_measured_runs,
        );
    }
    if sampled_suite
        .is_some_and(|suite| suite.eq_ignore_ascii_case(profile.collision.suite.as_str()))
    {
        report.collision = build_collision_closure_status(
            profile,
            samples,
            observed_warmup_runs,
            observed_measured_runs,
        );
    }
    report.verdict = build_closure_verdict(
        profile,
        &report.frame,
        &report.collision,
        presentation_reports,
        collision_reports,
    );
    report
}

fn apply_observed_wgsl_runtime_metadata(
    profile: &mut PerfClosureProfile,
    presentation_reports: &[PresentationBenchmarkReport],
) {
    if !matches!(
        profile.execution_story,
        PerfClosureExecutionStory::WgslResident
    ) {
        return;
    }
    let wgsl_reports = presentation_reports
        .iter()
        .filter(|report| report.backend.eq_ignore_ascii_case("wgsl"))
        .collect::<Vec<_>>();
    if wgsl_reports.is_empty() {
        return;
    }
    if let Some(adapter_name) = wgsl_reports
        .iter()
        .filter_map(|report| report.observed_adapter_name.as_ref())
        .map(|name| name.trim())
        .find(|name| !name.is_empty())
    {
        profile.adapter_name = adapter_name.to_string();
    }
    if let Some(requested_limits_profile) = wgsl_reports
        .iter()
        .map(|report| {
            report
                .frame_cost
                .gpu_runtime
                .requested_limits_profile
                .trim()
        })
        .find(|profile_name| !profile_name.is_empty())
    {
        profile.requested_limits_profile = requested_limits_profile.to_string();
    }
    profile.enabled_optional_features = wgsl_reports
        .iter()
        .flat_map(|report| {
            report
                .frame_cost
                .gpu_runtime
                .enabled_optional_features
                .iter()
                .cloned()
        })
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    profile.timestamps_enabled = wgsl_reports.iter().any(|report| {
        presentation_frame_history(&report.frame_cost, &report.frame_cost_history)
            .into_iter()
            .any(|frame| frame.gpu_runtime.timestamps_supported)
    });
    profile.f16_enabled = profile
        .enabled_optional_features
        .iter()
        .any(|feature| feature == "shader_f16");
    profile.indirect_dispatch_enabled = profile
        .enabled_optional_features
        .iter()
        .any(|feature| feature == "indirect_dispatch");
}

fn build_frame_closure_status(
    profile: &PerfClosureProfile,
    presentation_reports: &[PresentationBenchmarkReport],
    presentation_report_errors: &[String],
    observed_warmup_runs: usize,
    observed_measured_runs: usize,
) -> PerfClosureLaneStatusReport {
    let mut report = PerfClosureLaneStatusReport::unsampled(&profile.frame);
    report.status = PerfClosureLaneStatus::Sampled;
    report.notes.clear();
    let mut violations = presentation_report_errors
        .iter()
        .map(|error| format!("presentation report collection failed: {error}"))
        .collect::<Vec<_>>();
    if presentation_reports.is_empty() {
        if violations.is_empty() {
            report.notes.push(
                "frame closure suite ran without presentation frame-cost reports".to_string(),
            );
        } else {
            report.status = PerfClosureLaneStatus::Violated;
            report.notes.extend(violations);
        }
        return report;
    }

    let legal_degradations = profile
        .legal_degradations
        .iter()
        .map(|step| quality_degradation_step_name(*step))
        .collect::<std::collections::BTreeSet<_>>();
    let mut total_ms = Vec::new();
    let mut primary_ms = Vec::new();
    let mut scales = Vec::new();
    let mut reconstructed_output_detected = false;
    let mut output_width = None;
    let mut output_height = None;
    let mut observed_backends = std::collections::BTreeSet::new();
    let mut active_acceleration_artifacts = std::collections::BTreeSet::new();
    let mut active_degradations = std::collections::BTreeSet::new();
    let mut bottleneck_counts: HashMap<String, usize> = HashMap::new();
    let expected_backend = profile.backend.as_str();

    if observed_warmup_runs != profile.warmup_runs as usize
        || observed_measured_runs != profile.measured_runs as usize
    {
        violations.push(format!(
            "observed run protocol warmup={} measured={} does not match canonical warmup={} measured={}",
            observed_warmup_runs,
            observed_measured_runs,
            profile.warmup_runs,
            profile.measured_runs
        ));
    }

    for sample in presentation_reports {
        observed_backends.insert(sample.backend.clone());
        if !sample.backend.eq_ignore_ascii_case(expected_backend) {
            violations.push(format!(
                "scenario '{}' reported backend '{}' instead of closure backend '{}'",
                sample.scenario_id, sample.backend, expected_backend
            ));
        }
        let measured_frame_costs =
            presentation_frame_history(&sample.frame_cost, &sample.frame_cost_history);
        let frame_costs = measured_frame_costs.iter().collect::<Vec<_>>();
        for frame_cost in frame_costs {
            total_ms.push(ns_to_ms(frame_cost_total_ns(frame_cost)));
            if let Some(primary_pass_ms) = primary_visibility_pass_ms(frame_cost) {
                primary_ms.push(primary_pass_ms);
            }
            scales.push(frame_cost.quality.internal_resolution_scale);
            reconstructed_output_detected |= frame_cost.quality.reconstructed_output;
            output_width = Some(frame_cost.output_width);
            output_height = Some(frame_cost.output_height);

            if frame_cost.output_width != profile.output_width
                || frame_cost.output_height != profile.output_height
            {
                violations.push(format!(
                    "scenario '{}' observed output {}x{} does not match closure target {}x{}",
                    sample.scenario_id,
                    frame_cost.output_width,
                    frame_cost.output_height,
                    profile.output_width,
                    profile.output_height
                ));
            }
            if frame_cost.quality.tier != "realtime_120" {
                violations.push(format!(
                    "scenario '{}' reported quality tier '{}' instead of realtime_120",
                    sample.scenario_id, frame_cost.quality.tier
                ));
            }
            if frame_cost.quality.internal_resolution_scale < profile.min_internal_resolution_scale
            {
                violations.push(format!(
                    "scenario '{}' observed internal scale {:.2} below floor {:.2}",
                    sample.scenario_id,
                    frame_cost.quality.internal_resolution_scale,
                    profile.min_internal_resolution_scale
                ));
            }
            for degradation in &frame_cost.quality.active_degradations {
                active_degradations.insert(degradation.clone());
                if !legal_degradations.contains(degradation.as_str()) {
                    violations.push(format!(
                        "scenario '{}' used undeclared degradation '{}'",
                        sample.scenario_id, degradation
                    ));
                }
            }
            for artifact in &frame_cost.active_acceleration_artifacts {
                active_acceleration_artifacts.insert(artifact.clone());
            }
            if let Some(bottleneck) = &frame_cost.bottleneck_pass {
                *bottleneck_counts.entry(bottleneck.clone()).or_insert(0) += 1;
            }
        }
    }

    report.measured_output_width = output_width;
    report.measured_output_height = output_height;
    report.min_internal_resolution_scale_observed = scales.iter().copied().reduce(f32::min);
    report.max_internal_resolution_scale_observed = scales.iter().copied().reduce(f32::max);
    report.reconstructed_output_detected = Some(reconstructed_output_detected);
    report.active_acceleration_artifacts = active_acceleration_artifacts.into_iter().collect();
    report.active_degradations = active_degradations.into_iter().collect();
    report.total_frame_median_ms = percentile_f32(&total_ms, 0.50);
    report.total_frame_p95_ms = percentile_f32(&total_ms, 0.95);
    report.primary_visibility_median_ms = percentile_f32(&primary_ms, 0.50);
    report.primary_visibility_p95_ms = percentile_f32(&primary_ms, 0.95);
    report.dominant_bottleneck_pass = most_common_key(&bottleneck_counts);
    report.notes.push(format!(
        "presentation reports collected for {} scenario(s) spanning {} closure frame sample(s)",
        presentation_reports.len(),
        total_ms.len()
    ));
    if !observed_backends.is_empty() {
        report.notes.push(format!(
            "presentation backends observed: {}",
            observed_backends.into_iter().collect::<Vec<_>>().join(",")
        ));
    }
    let selected_workgroup_sizes = presentation_reports
        .iter()
        .map(|report| report.selected_workgroup_size)
        .collect::<std::collections::BTreeSet<_>>();
    if !selected_workgroup_sizes.is_empty() {
        report.notes.push(format!(
            "wgsl workgroup size selection observed: {}",
            selected_workgroup_sizes
                .iter()
                .map(|size| size.to_string())
                .collect::<Vec<_>>()
                .join(",")
        ));
    }
    if let Some(comparison) = presentation_reports
        .iter()
        .find_map(|report| report.wgsl_workgroup_comparison.as_ref())
    {
        report.notes.push(format!(
            "wgsl workgroup comparison selected={} candidates={}",
            comparison.selected_workgroup_size,
            format_workgroup_comparison(comparison)
        ));
    }

    if report.primary_visibility_median_ms.is_none() || report.primary_visibility_p95_ms.is_none() {
        violations.push(
            "primary_visibility pass timings were not present in the sampled reports".to_string(),
        );
    }
    if let Some(total_median_ms) = report.total_frame_median_ms
        && total_median_ms > profile.frame_budget.median_ms
    {
        violations.push(format!(
            "frame median {:.2} ms exceeds budget {:.2} ms",
            total_median_ms, profile.frame_budget.median_ms
        ));
    }
    if let Some(total_p95_ms) = report.total_frame_p95_ms
        && total_p95_ms > profile.frame_budget.p95_ms
    {
        violations.push(format!(
            "frame p95 {:.2} ms exceeds budget {:.2} ms",
            total_p95_ms, profile.frame_budget.p95_ms
        ));
    }
    if let Some(primary_median_ms) = report.primary_visibility_median_ms
        && primary_median_ms > profile.primary_visibility_budget.median_ms
    {
        violations.push(format!(
            "primary visibility median {:.2} ms exceeds budget {:.2} ms",
            primary_median_ms, profile.primary_visibility_budget.median_ms
        ));
    }
    if let Some(primary_p95_ms) = report.primary_visibility_p95_ms
        && primary_p95_ms > profile.primary_visibility_budget.p95_ms
    {
        violations.push(format!(
            "primary visibility p95 {:.2} ms exceeds budget {:.2} ms",
            primary_p95_ms, profile.primary_visibility_budget.p95_ms
        ));
    }

    if violations.is_empty() {
        report.status = PerfClosureLaneStatus::Validated;
        report
            .notes
            .push("frame closure met the canonical 1080p120 contract".to_string());
    } else {
        report.status = PerfClosureLaneStatus::Violated;
        report.notes.extend(violations);
    }
    report
}

fn build_collision_closure_status(
    profile: &PerfClosureProfile,
    samples: &[command_handlers::PerfSummary],
    observed_warmup_runs: usize,
    observed_measured_runs: usize,
) -> PerfClosureLaneStatusReport {
    let mut report = PerfClosureLaneStatusReport::unsampled(&profile.collision);
    let summary = command_handlers::aggregate_perf_samples(samples);
    let mut violations = Vec::new();
    report.status = PerfClosureLaneStatus::Sampled;
    report.notes.clear();
    report.collision_baseline_id = Some(profile.collision_baseline.baseline_id.clone());
    report.collision_runtime_median_ms = Some(ns_to_ms(summary.runtime_p50_ns));
    report.collision_runtime_p95_ms = Some(ns_to_ms(summary.runtime_p95_ns));
    report.notes.push(format!(
        "collision closure sampled {} measured perf run(s) under protocol '{}'",
        samples.len(),
        profile.collision.protocol_id
    ));
    if observed_warmup_runs != profile.warmup_runs as usize
        || observed_measured_runs != profile.measured_runs as usize
    {
        violations.push(format!(
            "observed run protocol warmup={} measured={} does not match canonical warmup={} measured={}",
            observed_warmup_runs,
            observed_measured_runs,
            profile.warmup_runs,
            profile.measured_runs
        ));
    }
    match load_collision_baseline_summary(&profile.collision_baseline.baseline_id) {
        Ok(baseline) => {
            let failures = command_handlers::evaluate_perf_gate(
                &summary,
                &baseline,
                profile.collision_baseline.max_runtime_regression_pct as f64,
                &command_handlers::KpiThresholds {
                    check_fallback_max: None,
                    check_batch_min: None,
                    scheduler_p99_improve_min_pct: None,
                    rewrite_overhead_max_pct: None,
                    actor_throughput_improve_min_pct: None,
                    queue_age_p99_max_regress_pct: None,
                    starvation_violations_max: None,
                    scheduler_throughput_improve_min_pct: None,
                    scheduler_loop_p99_max_regress_pct: None,
                    scheduler_local_hit_min: None,
                },
            );
            let regression_pct = if baseline.runtime_p50_ns == 0 {
                0.0
            } else {
                ((summary.runtime_p50_ns as f64 - baseline.runtime_p50_ns as f64)
                    / baseline.runtime_p50_ns as f64
                    * 100.0) as f32
            };
            report.collision_runtime_regression_pct = Some(regression_pct);
            report.notes.push(format!(
                "collision non-regression compared against baseline '{}' from {}",
                profile.collision_baseline.baseline_id,
                collision_baseline_fixture_path(&profile.collision_baseline.baseline_id).display()
            ));
            violations.extend(failures);
        }
        Err(err) => violations.push(format!(
            "collision baseline '{}' unavailable: {}",
            profile.collision_baseline.baseline_id, err
        )),
    }
    if violations.is_empty() {
        report.status = PerfClosureLaneStatus::Validated;
        report.notes.push(format!(
            "collision closure met the canonical non-regression budget ({:.2}% max runtime regression)",
            profile.collision_baseline.max_runtime_regression_pct
        ));
    } else {
        report.status = PerfClosureLaneStatus::Violated;
        report.notes.extend(violations);
    }
    report
}

fn build_closure_verdict(
    profile: &PerfClosureProfile,
    frame: &PerfClosureLaneStatusReport,
    collision: &PerfClosureLaneStatusReport,
    presentation_reports: &[PresentationBenchmarkReport],
    collision_reports: &[CollisionBenchmarkReport],
) -> PerfClosureVerdict {
    let mut findings = BTreeMap::<(String, String), PerfClosureFinding>::new();
    let frame_sampled = !matches!(frame.status, PerfClosureLaneStatus::NotSampled)
        && !presentation_reports.is_empty();
    let collision_sampled = !matches!(collision.status, PerfClosureLaneStatus::NotSampled)
        && !collision_reports.is_empty();
    let sampled = frame_sampled || collision_sampled;

    if frame_sampled {
        for report in presentation_reports {
            for finding in explain_frame_why_not_120_findings(
                &report.frame_cost,
                frame.total_frame_median_ms,
                frame.primary_visibility_median_ms,
                profile.frame_budget.median_ms,
                profile.primary_visibility_budget.median_ms,
            ) {
                merge_closure_finding(&mut findings, finding);
            }
            for finding in explain_acceleration_why_not_120_findings(&report.frame_cost) {
                merge_closure_finding(&mut findings, finding);
            }
        }
    }
    if collision_sampled {
        for report in collision_reports {
            for finding in explain_collision_why_not_120_findings(report) {
                merge_closure_finding(&mut findings, finding);
            }
        }
    }

    let status = if !sampled {
        PerfClosureVerdictStatus::NotApplicable
    } else if matches!(frame.status, PerfClosureLaneStatus::Violated)
        || matches!(collision.status, PerfClosureLaneStatus::Violated)
    {
        PerfClosureVerdictStatus::Failed
    } else {
        PerfClosureVerdictStatus::Met
    };
    let top_remaining_bottleneck = if matches!(status, PerfClosureVerdictStatus::Failed) {
        choose_top_remaining_bottleneck(profile, frame, collision, collision_reports)
    } else {
        None
    };
    let summary = match status {
        PerfClosureVerdictStatus::NotApplicable => {
            format!(
                "{} closure target was not exercised in this run",
                profile.execution_story.as_str()
            )
        }
        PerfClosureVerdictStatus::Met => format!(
            "{} closure target met for sampled lanes across {} presentation report(s) and {} collision report(s)",
            profile.execution_story.as_str(),
            presentation_reports.len(),
            collision_reports.len()
        ),
        PerfClosureVerdictStatus::Failed => format!(
            "{} closure target failed; top remaining bottleneck: {}",
            profile.execution_story.as_str(),
            top_remaining_bottleneck.as_deref().unwrap_or("unknown")
        ),
    };

    PerfClosureVerdict {
        status,
        summary,
        top_remaining_bottleneck,
        findings: findings.into_values().collect(),
    }
}

fn choose_top_remaining_bottleneck(
    profile: &PerfClosureProfile,
    frame: &PerfClosureLaneStatusReport,
    collision: &PerfClosureLaneStatusReport,
    collision_reports: &[CollisionBenchmarkReport],
) -> Option<String> {
    if matches!(frame.status, PerfClosureLaneStatus::Violated) {
        if let Some(bottleneck) = frame.dominant_bottleneck_pass.clone() {
            return Some(bottleneck);
        }
        if let Some(primary_visibility_median_ms) = frame.primary_visibility_median_ms
            && primary_visibility_median_ms > profile.primary_visibility_budget.median_ms
        {
            return Some("primary_visibility".to_string());
        }
        if let Some(frame_median_ms) = frame.total_frame_median_ms
            && frame_median_ms > profile.frame_budget.median_ms
        {
            return Some("surface_or_shading".to_string());
        }
        return Some("presentation".to_string());
    }

    if matches!(collision.status, PerfClosureLaneStatus::Violated) {
        if collision_reports.iter().any(|report| {
            report.witness_reuse_rate < 0.5
                && (report.unavailable_count_total > 0 || report.rejected_count_total > 0)
        }) {
            return Some("collision_witness_reuse".to_string());
        }
        if collision_reports
            .iter()
            .any(|report| report.fallback_rate > 0.0)
        {
            return Some("collision_fallback".to_string());
        }
        if collision.collision_runtime_regression_pct.is_some() {
            return Some("collision_runtime_regression".to_string());
        }
    }

    None
}

fn explain_collision_why_not_120_findings(
    report: &CollisionBenchmarkReport,
) -> Vec<PerfClosureFinding> {
    let mut findings = Vec::new();

    if report.witness_reuse_rate < 0.50
        || report.unavailable_count_total > 0
        || report.rejected_count_total > 0
    {
        findings.push(PerfClosureFinding {
            subsystem: "collision".to_string(),
            focus: "witness_reuse_invalid_or_unsupported".to_string(),
            summary: "collision witness reuse is still being rejected or treated as unavailable, so the lane is not living on the fast path yet".to_string(),
            evidence: vec![
                format!("witness_reuse_rate={:.2}", report.witness_reuse_rate),
                format!("available_count_total={}", report.available_count_total),
                format!("consumed_count_total={}", report.consumed_count_total),
                format!("rejected_count_total={}", report.rejected_count_total),
                format!("unavailable_count_total={}", report.unavailable_count_total),
            ],
            next_step:
                "revisit the witness reuse contract, then make the conservative fallback path explicit when reuse is not valid".to_string(),
        });
    }

    if report.fallback_rate > 0.0 {
        findings.push(PerfClosureFinding {
            subsystem: "collision".to_string(),
            focus: "fallback_rate".to_string(),
            summary: "the collision lane is still falling back on a noticeable fraction of queries".to_string(),
            evidence: vec![
                format!("fallback_rate={:.2}", report.fallback_rate),
                format!("average_candidate_count={:.2}", report.average_candidate_count),
                format!(
                    "average_rejected_candidate_count={:.2}",
                    report.average_rejected_candidate_count
                ),
                format!(
                    "average_pruned_node_count={:.2}",
                    report.average_pruned_node_count
                ),
            ],
            next_step:
                "reduce the reasons the collision plan is falling back, then remeasure against the canonical baseline".to_string(),
        });
    }

    findings
}

fn merge_closure_finding(
    findings: &mut BTreeMap<(String, String), PerfClosureFinding>,
    finding: PerfClosureFinding,
) {
    let key = (finding.subsystem.clone(), finding.focus.clone());
    if let Some(existing) = findings.get_mut(&key) {
        for evidence in finding.evidence {
            if !existing.evidence.contains(&evidence) {
                existing.evidence.push(evidence);
            }
        }
        if existing.summary.is_empty() {
            existing.summary = finding.summary;
        }
        if existing.next_step.is_empty() {
            existing.next_step = finding.next_step;
        }
    } else {
        findings.insert(key, finding);
    }
}

fn collision_baseline_fixture_path(baseline_id: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .join("benchmarks")
        .join("collision_perf")
        .join("baselines")
        .join(format!("{baseline_id}.json"))
}

fn load_collision_baseline_summary(
    baseline_id: &str,
) -> Result<command_handlers::PerfSummary, String> {
    let path = collision_baseline_fixture_path(baseline_id);
    command_handlers::load_perf_baseline_summary(&path)
        .map_err(|err| format!("{}: {}", path.display(), err))
}

fn primary_visibility_pass_ms(
    report: &wrela::presentation_exec::PresentationFrameCostReport,
) -> Option<f32> {
    report
        .passes
        .iter()
        .find(|pass| pass.pass_kind == "primary_visibility")
        .map(|pass| pass.elapsed_micros as f32 / 1_000.0)
}

fn most_common_key(counts: &HashMap<String, usize>) -> Option<String> {
    counts
        .iter()
        .max_by(|left, right| left.1.cmp(right.1).then_with(|| right.0.cmp(left.0)))
        .map(|(key, _)| key.clone())
}

fn ns_to_ms(value: u128) -> f32 {
    value as f32 / 1_000_000.0
}

fn percentile_f32(values: &[f32], quantile: f32) -> Option<f32> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|left, right| left.total_cmp(right));
    let index =
        ((sorted.len().saturating_sub(1)) as f32 * quantile.clamp(0.0, 1.0)).round() as usize;
    sorted.get(index).copied()
}

fn print_presentation_benchmark_reports(reports: &[PresentationBenchmarkReport]) {
    println!("presentation-benchmarks:");
    for report in reports {
        let effective_history =
            presentation_frame_history(&report.frame_cost, &report.frame_cost_history);
        let solver_counters = aggregate_presentation_solver_counters(&effective_history);
        println!(
            "presentation-scenario {} test={} backend={} query_trace_solver_mode={} selected_workgroup_size={} frames={} frame_time_ns={} field_samples={} quality={} target_fps={} scale={:.2} scale_history={} reconstructed_output={} bottleneck_pass={} acceleration={} gain_sources={}",
            report.scenario_id,
            report.test_name,
            report.backend,
            report.query_trace_solver_mode,
            report.selected_workgroup_size,
            report.frames_executed,
            report.frame_time_ns,
            report.field_samples,
            report.quality_tier,
            report.target_fps,
            report.internal_resolution_scale,
            report
                .internal_resolution_history
                .iter()
                .map(|scale| format!("{scale:.2}"))
                .collect::<Vec<_>>()
                .join(","),
            report.reconstructed_output,
            report.bottleneck_pass.as_deref().unwrap_or("none"),
            report.active_acceleration_artifacts.join(","),
            report.performance_gain_sources.join(","),
        );
        if let Some(comparison) = &report.wgsl_workgroup_comparison {
            println!(
                "  wgsl workgroup comparison selected={} candidates={}",
                comparison.selected_workgroup_size,
                format_workgroup_comparison(comparison),
            );
        }
        if let Some(comparison) = &report.ab_comparison {
            println!(
                "  ab hybrid-vs-dense-only frame_time_ns_delta={} ({:.2}%) average_trace_steps_delta={:.3} field_samples_delta={} candidate_count_before_pruning_delta={} candidate_count_after_pruning_delta={} dense_only_frame_time_ns={} dense_only_average_trace_steps={:.3} dense_only_field_samples={} dense_only_candidate_count_before_pruning={} dense_only_candidate_count_after_pruning={}",
                comparison.frame_time_ns_delta_vs_dense_only,
                comparison.frame_time_ns_delta_vs_dense_only_pct,
                comparison.average_trace_steps_delta_vs_dense_only,
                comparison.field_samples_delta_vs_dense_only,
                comparison.candidate_count_before_pruning_delta_vs_dense_only,
                comparison.candidate_count_after_pruning_delta_vs_dense_only,
                comparison.dense_only_frame_time_ns,
                comparison.dense_only_average_trace_steps,
                comparison.dense_only_field_samples,
                comparison.dense_only_candidate_count_before_pruning,
                comparison.dense_only_candidate_count_after_pruning,
            );
        }
        println!(
            "  solver counters relaxed_attempts={} relaxed_no_root_advances={} relaxed_brackets={} relaxed_unresolved={} interval_attempts={} interval_no_root_advances={} interval_brackets={} interval_unresolved={} refinement_attempts={} refinement_failures={} repeat_attempts={} repeat_supported={} repeat_inapplicable={} repeat_unsupported={} repeat_unsupported_form={} repeat_unsupported_bounds={} repeat_cells_enumerated={}",
            solver_counters.solver_relaxed_attempts,
            solver_counters.solver_relaxed_no_root_advances,
            solver_counters.solver_relaxed_brackets,
            solver_counters.solver_relaxed_unresolved,
            solver_counters.solver_interval_attempts,
            solver_counters.solver_interval_no_root_advances,
            solver_counters.solver_interval_brackets,
            solver_counters.solver_interval_unresolved,
            solver_counters.solver_refinement_attempts,
            solver_counters.solver_refinement_failures,
            solver_counters.solver_repeat_attempts,
            solver_counters.solver_repeat_supported,
            solver_counters.solver_repeat_inapplicable,
            solver_counters.solver_repeat_unsupported,
            solver_counters.solver_repeat_unsupported_form,
            solver_counters.solver_repeat_unsupported_bounds,
            solver_counters.solver_repeat_cells_enumerated,
        );
        for pass in &report.frame_cost.passes {
            println!(
                "presentation-pass {} {} kind={} items={} elapsed_us={} dispatches={} bytes_read={} bytes_written={} notes={}",
                report.scenario_id,
                pass.pass_id,
                pass.pass_kind,
                pass.work_items,
                pass.elapsed_micros,
                pass.dispatch_count,
                pass.attachment_bytes_read,
                pass.attachment_bytes_written,
                pass.notes.join("|"),
            );
        }
    }
}

fn print_collision_benchmark_reports(reports: &[command_handlers::CollisionBenchmarkReport]) {
    println!("collision-benchmarks:");
    for report in reports {
        println!(
            "collision-scenario suite={} backend={} command={} total_runtime_ns={} queries_per_sec={:.2} avg_candidates={:.2} avg_rejected_candidates={:.2} avg_pruned_nodes={:.2} avg_interval_subdivisions={:.2} avg_interval_refinements={:.2} avg_certificate_successes={:.2} witness_reuse_rate={:.2} fallback_rate={:.2}",
            report.suite,
            report.backend,
            report.command,
            report.total_runtime_ns,
            report.queries_per_sec,
            report.average_candidate_count,
            report.average_rejected_candidate_count,
            report.average_pruned_node_count,
            report.average_interval_subdivisions,
            report.average_interval_refinements,
            report.average_certificate_successes,
            report.witness_reuse_rate,
            report.fallback_rate,
        );
        for execution in &report.executions {
            println!(
                "  collision-execution {} plan={} contract={} query_count={} runtime_ns={} qps={:.2} candidate_count={} rejected_candidate_count={} pruned_node_count={} interval_subdivisions={} interval_refinements={} certificate_successes={} fallback_count={} interval_bracket={} contact_normal_provenance={} reuse_available={} reuse_consumed={} reuse_rejected={} reuse_unavailable={} witness_reuse_rate={:.2} fallback_rate={:.2}",
                execution.name,
                execution.plan_name,
                execution.contract_id,
                execution.query_count,
                execution.runtime_ns,
                execution.queries_per_sec,
                execution.broadphase_candidate_count,
                execution.broadphase_rejected_candidate_count,
                execution.broadphase_pruned_node_count,
                execution.interval_subdivisions,
                execution.interval_refinements,
                execution.certificate_successes,
                execution.fallback_count,
                execution
                    .interval_bracket
                    .map(|bracket| format!("[{:.6}, {:.6}]", bracket[0], bracket[1]))
                    .unwrap_or_else(|| "none".to_string()),
                execution
                    .contact_normal_provenance
                    .as_deref()
                    .unwrap_or("none"),
                execution.available_count,
                execution.consumed_count,
                execution.rejected_count,
                execution.unavailable_count,
                execution.witness_reuse_rate,
                execution.fallback_rate,
            );
        }
    }
}

fn print_closure_verdict_report(report: &PerfClosureReport, verbose: bool) {
    println!(
        "closure verdict: {}",
        closure_verdict_status_name(report.verdict.status)
    );
    println!(
        "  profile: {} ({}, backend={}, adapter={}, requested_limits={}, timestamps={}, f16={}, indirect_dispatch={}, warmup={})",
        report.profile.name,
        report.profile.execution_story.as_str(),
        report.profile.backend.as_str(),
        report.profile.adapter_name,
        report.profile.requested_limits_profile,
        report.profile.timestamps_enabled,
        report.profile.f16_enabled,
        report.profile.indirect_dispatch_enabled,
        report.profile.warmup_protocol,
    );
    if !report.profile.enabled_optional_features.is_empty() {
        println!(
            "  enabled optional features: {}",
            report.profile.enabled_optional_features.join(", ")
        );
    }
    if let Some(cpu_oracle_profile) = report.cpu_oracle_profile.as_ref() {
        println!(
            "  cpu-oracle companion: {} ({}, backend={}, adapter={})",
            cpu_oracle_profile.name,
            cpu_oracle_profile.execution_story.as_str(),
            cpu_oracle_profile.backend.as_str(),
            cpu_oracle_profile.adapter_name
        );
    }
    println!("  summary: {}", report.verdict.summary);
    if let Some(bottleneck) = report.verdict.top_remaining_bottleneck.as_deref() {
        println!("  top remaining bottleneck: {bottleneck}");
    }
    if verbose || matches!(report.verdict.status, PerfClosureVerdictStatus::Failed) {
        println!("why-not-120:");
        if report.verdict.findings.is_empty() {
            println!("  - no specific subsystem finding was inferred from the sampled reports");
        } else {
            for finding in &report.verdict.findings {
                println!(
                    "  - subsystem={} focus={}",
                    finding.subsystem, finding.focus
                );
                println!("    summary: {}", finding.summary);
                if !finding.evidence.is_empty() {
                    println!("    evidence: {}", finding.evidence.join(" | "));
                }
                println!("    next step: {}", finding.next_step);
            }
        }
    }
}

fn closure_verdict_status_name(status: PerfClosureVerdictStatus) -> &'static str {
    match status {
        PerfClosureVerdictStatus::NotApplicable => "not_applicable",
        PerfClosureVerdictStatus::Met => "met",
        PerfClosureVerdictStatus::Failed => "failed",
    }
}

fn format_workgroup_comparison(comparison: &PresentationWgslWorkgroupComparison) -> String {
    comparison
        .candidate_workgroup_sizes
        .iter()
        .zip(&comparison.candidate_frame_time_ns)
        .zip(&comparison.frame_time_ns_delta_vs_selected_pct)
        .map(|((workgroup_size, frame_time_ns), delta_pct)| {
            format!("{}:{}ns({:+.2}%)", workgroup_size, frame_time_ns, delta_pct)
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub(super) struct PerfcmpCommandInput {
    pub(super) trace: bool,
    pub(super) program_args: Vec<String>,
    pub(super) path_arg: Option<String>,
    pub(super) benchmark_manifest_path: Option<String>,
    pub(super) perfcmp_baseline_ref: Option<String>,
    pub(super) perfcmp_candidate_ref: Option<String>,
    pub(super) out_path: Option<String>,
    pub(super) output_format: OutputFormat,
    pub(super) perf_profile: PerfProfile,
    pub(super) perfcmp_warmup_pairs: Option<usize>,
    pub(super) perfcmp_measure_pairs: Option<usize>,
    pub(super) perfcmp_min_effect_pct: Option<f64>,
    pub(super) perfcmp_confidence_pct: Option<f64>,
    pub(super) test_timeout_ms: Option<u64>,
    pub(super) perf_debug: bool,
}

pub(super) fn execute_perfcmp_command(input: PerfcmpCommandInput) -> i32 {
    if input.trace {
        eprintln!("build: command perfcmp");
    }
    if !input.program_args.is_empty() {
        eprintln!("error: unexpected extra arguments");
        return EXIT_USAGE;
    }
    let target = match resolve_test_target(input.path_arg.as_deref()) {
        Ok(target) => target,
        Err(err) => {
            eprintln!("error: {err}");
            return EXIT_USAGE;
        }
    };
    let TestTarget::ProjectRoot(target_root) = target else {
        eprintln!("error: perfcmp target must be a benchmark project directory");
        return EXIT_USAGE;
    };

    let manifest_path = match resolve_perf_benchmark_manifest_path(
        &TestTarget::ProjectRoot(target_root.clone()),
        input.benchmark_manifest_path,
        input.perf_profile,
    ) {
        Some(path) => path,
        None => {
            eprintln!(
                "error: benchmark manifest required; pass --benchmark-manifest or place bench.toml under target root"
            );
            return EXIT_USAGE;
        }
    };
    let baseline_ref = input
        .perfcmp_baseline_ref
        .unwrap_or_else(|| "origin/main".to_string());
    let candidate_ref = input
        .perfcmp_candidate_ref
        .unwrap_or_else(|| "HEAD".to_string());
    let report_out = input
        .out_path
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".artifacts/perf/perfcmp-report.json"));
    let perfcmp_cfg = PerfCmpConfig {
        baseline_ref,
        candidate_ref,
        manifest_path,
        benchmark_root: target_root,
        profile: input.perf_profile,
        warmup_pairs_override: input.perfcmp_warmup_pairs,
        measure_pairs_override: input.perfcmp_measure_pairs,
        min_effect_pct: input.perfcmp_min_effect_pct.unwrap_or(2.0),
        confidence_pct: input.perfcmp_confidence_pct.unwrap_or(95.0),
        output_json: report_out,
        output_format: input.output_format,
        test_timeout_ms: input.test_timeout_ms,
        perf_debug: input.perf_debug,
    };
    run_perfcmp(&perfcmp_cfg)
}

pub(super) fn run_perfcmp(config: &PerfCmpConfig) -> i32 {
    let manifest = match load_benchmark_manifest(&config.manifest_path) {
        Ok(manifest) => manifest,
        Err(err) => {
            eprintln!("perfcmp error: {err}");
            return EXIT_USAGE;
        }
    };
    let timing_env_knobs = perfcmp_env_knobs(config.profile, PerfRunMode::Timing);
    if should_skip_optional_suite(manifest.optional, &manifest.suite, env::consts::OS) {
        let report = PerfCmpReport {
            version: 1,
            generated_at_unix_ms: now_unix_ms(),
            suite: manifest.suite.clone(),
            optional_suite: true,
            skipped_reason: Some(format!(
                "optional linux suite skipped on non-linux host ({})",
                env::consts::OS
            )),
            profile: config.profile.as_str().to_string(),
            baseline_ref: config.baseline_ref.clone(),
            candidate_ref: config.candidate_ref.clone(),
            benchmark_root: config.benchmark_root.display().to_string(),
            manifest_path: config.manifest_path.display().to_string(),
            warmup_pairs: 0,
            measured_pairs: 0,
            confidence_pct: config.confidence_pct,
            min_effect_pct: config.min_effect_pct,
            env_knobs: timing_env_knobs,
            diagnostics: None,
            host: collect_perfcmp_host_info(),
            scenarios: Vec::new(),
            summary: PerfCmpSummary {
                scenario_count: 0,
                win_count: 0,
                regression_count: 0,
                no_signal_count: 0,
                unstable_count: 0,
                unstable_critical_count: 0,
                non_blocking_suite: true,
                gate_passed: true,
                gate_failures: Vec::new(),
            },
        };
        match write_perfcmp_report(config, &report) {
            Ok(markdown_path) => {
                if matches!(config.output_format, OutputFormat::Pretty) {
                    println!("perfcmp report: {}", config.output_json.display());
                    println!("perfcmp markdown: {}", markdown_path.display());
                    println!(
                        "perfcmp note: {}",
                        report
                            .skipped_reason
                            .as_deref()
                            .unwrap_or("optional suite skipped")
                    );
                }
            }
            Err(err) => {
                eprintln!("perfcmp error: {err}");
                return EXIT_CODEGEN;
            }
        }
        return EXIT_OK;
    }

    let scenarios = manifest.scenarios_for_profile(config.profile);
    if scenarios.is_empty() {
        eprintln!("perfcmp error: profile selected zero scenarios");
        return EXIT_USAGE;
    }
    let (warmup_pairs, measured_pairs) = profile_pair_counts(
        &manifest,
        config.profile,
        config.warmup_pairs_override,
        config.measure_pairs_override,
    );
    let timeout_ms = effective_perfcmp_timeout_ms(&scenarios, config.test_timeout_ms);
    let jobs = Some(1usize);
    let repo_root = match env::current_dir() {
        Ok(path) => path,
        Err(err) => {
            eprintln!("perfcmp error: failed to resolve current directory: {err}");
            return EXIT_USAGE;
        }
    };
    let repo_root = match fs::canonicalize(repo_root) {
        Ok(path) => path,
        Err(err) => {
            eprintln!("perfcmp error: failed to canonicalize current directory: {err}");
            return EXIT_USAGE;
        }
    };
    let benchmark_root = match fs::canonicalize(&config.benchmark_root) {
        Ok(path) => path,
        Err(err) => {
            eprintln!(
                "perfcmp error: failed to canonicalize benchmark root {}: {err}",
                config.benchmark_root.display()
            );
            return EXIT_USAGE;
        }
    };
    let manifest_path = match fs::canonicalize(&config.manifest_path) {
        Ok(path) => path,
        Err(err) => {
            eprintln!(
                "perfcmp error: failed to canonicalize benchmark manifest {}: {err}",
                config.manifest_path.display()
            );
            return EXIT_USAGE;
        }
    };
    let Some(relative_benchmark_root) = benchmark_root
        .strip_prefix(&repo_root)
        .ok()
        .map(Path::to_path_buf)
    else {
        eprintln!(
            "perfcmp error: benchmark root {} must live under repository root {}",
            benchmark_root.display(),
            repo_root.display()
        );
        return EXIT_USAGE;
    };
    let Some(relative_manifest_path) = manifest_path
        .strip_prefix(&repo_root)
        .ok()
        .map(Path::to_path_buf)
    else {
        eprintln!(
            "perfcmp error: benchmark manifest {} must live under repository root {}",
            manifest_path.display(),
            repo_root.display()
        );
        return EXIT_USAGE;
    };

    let perfcmp_temp = env::temp_dir().join(format!("wrela-perfcmp-{}", now_unix_ms()));
    if let Err(err) = fs::create_dir_all(&perfcmp_temp) {
        eprintln!(
            "perfcmp error: failed to create temp directory {}: {err}",
            perfcmp_temp.display()
        );
        return EXIT_CODEGEN;
    }
    let baseline_worktree = perfcmp_temp.join("baseline");
    let candidate_worktree = perfcmp_temp.join("candidate");
    if let Err(err) = git_worktree_add(&repo_root, &baseline_worktree, &config.baseline_ref) {
        eprintln!("perfcmp error: {err}");
        let _ = fs::remove_dir_all(&perfcmp_temp);
        return EXIT_CODEGEN;
    }
    if let Err(err) = git_worktree_add(&repo_root, &candidate_worktree, &config.candidate_ref) {
        eprintln!("perfcmp error: {err}");
        cleanup_perfcmp_worktrees(
            &repo_root,
            &baseline_worktree,
            &candidate_worktree,
            &perfcmp_temp,
        );
        return EXIT_CODEGEN;
    }

    let pair_total = warmup_pairs + measured_pairs;
    let mut seed = fnv1a64(
        format!(
            "{}:{}:{}:{}",
            config.baseline_ref,
            config.candidate_ref,
            config.profile.as_str(),
            pair_total
        )
        .as_bytes(),
    );

    let mut by_scenario: HashMap<String, Vec<(u128, u128)>> = HashMap::new();
    for scenario in &scenarios {
        by_scenario.insert(scenario.id.clone(), Vec::new());
    }

    for pair_idx in 0..pair_total {
        let baseline_first = random_bool(&mut seed);
        let order = if baseline_first {
            [PerfCmpVariant::Baseline, PerfCmpVariant::Candidate]
        } else {
            [PerfCmpVariant::Candidate, PerfCmpVariant::Baseline]
        };
        let mut baseline_summary: Option<PerfcmpRunSummary> = None;
        let mut candidate_summary: Option<PerfcmpRunSummary> = None;

        for variant in order {
            let run_seed =
                fnv1a64(format!("{seed}:{}:{}:timing", pair_idx, variant.as_str()).as_bytes());
            let summary = match variant {
                PerfCmpVariant::Baseline => run_perf_once_in_worktree(
                    &baseline_worktree,
                    &relative_benchmark_root,
                    &relative_manifest_path,
                    config.profile,
                    jobs,
                    timeout_ms,
                    config.perf_debug,
                    run_seed,
                    &timing_env_knobs,
                    PerfRunMode::Timing,
                    None,
                ),
                PerfCmpVariant::Candidate => run_perf_once_in_worktree(
                    &candidate_worktree,
                    &relative_benchmark_root,
                    &relative_manifest_path,
                    config.profile,
                    jobs,
                    timeout_ms,
                    config.perf_debug,
                    run_seed,
                    &timing_env_knobs,
                    PerfRunMode::Timing,
                    None,
                ),
            };
            let summary = match summary {
                Ok(summary) => summary,
                Err(err) => {
                    eprintln!("perfcmp error: {err}");
                    cleanup_perfcmp_worktrees(
                        &repo_root,
                        &baseline_worktree,
                        &candidate_worktree,
                        &perfcmp_temp,
                    );
                    return EXIT_CODEGEN;
                }
            };
            match variant {
                PerfCmpVariant::Baseline => baseline_summary = Some(summary),
                PerfCmpVariant::Candidate => candidate_summary = Some(summary),
            }
        }
        let Some(baseline_summary) = baseline_summary else {
            eprintln!(
                "perfcmp error: missing baseline summary for pair {}",
                pair_idx + 1
            );
            cleanup_perfcmp_worktrees(
                &repo_root,
                &baseline_worktree,
                &candidate_worktree,
                &perfcmp_temp,
            );
            return EXIT_CODEGEN;
        };
        let Some(candidate_summary) = candidate_summary else {
            eprintln!(
                "perfcmp error: missing candidate summary for pair {}",
                pair_idx + 1
            );
            cleanup_perfcmp_worktrees(
                &repo_root,
                &baseline_worktree,
                &candidate_worktree,
                &perfcmp_temp,
            );
            return EXIT_CODEGEN;
        };
        if pair_idx < warmup_pairs {
            continue;
        }
        for scenario in &scenarios {
            let Some(&baseline_runtime) = baseline_summary.runtime_by_test.get(&scenario.test_name)
            else {
                eprintln!(
                    "perfcmp error: baseline summary missing scenario test `{}`",
                    scenario.test_name
                );
                cleanup_perfcmp_worktrees(
                    &repo_root,
                    &baseline_worktree,
                    &candidate_worktree,
                    &perfcmp_temp,
                );
                return EXIT_CODEGEN;
            };
            let Some(&candidate_runtime) =
                candidate_summary.runtime_by_test.get(&scenario.test_name)
            else {
                eprintln!(
                    "perfcmp error: candidate summary missing scenario test `{}`",
                    scenario.test_name
                );
                cleanup_perfcmp_worktrees(
                    &repo_root,
                    &baseline_worktree,
                    &candidate_worktree,
                    &perfcmp_temp,
                );
                return EXIT_CODEGEN;
            };
            if let Some(samples) = by_scenario.get_mut(&scenario.id) {
                samples.push((baseline_runtime, candidate_runtime));
            }
        }
    }

    let diagnostics_env_knobs = perfcmp_env_knobs(config.profile, PerfRunMode::Diagnostics);
    let metrics_dir = repo_root.join(".artifacts").join("perf").join("metrics");
    if let Err(err) = fs::create_dir_all(&metrics_dir) {
        eprintln!(
            "perfcmp error: failed to create diagnostics metrics dir {}: {}",
            metrics_dir.display(),
            err
        );
        cleanup_perfcmp_worktrees(
            &repo_root,
            &baseline_worktree,
            &candidate_worktree,
            &perfcmp_temp,
        );
        return EXIT_CODEGEN;
    }
    let baseline_metrics_path = metrics_dir.join(format!(
        "perfcmp-{}-{}-baseline.json",
        manifest.suite,
        now_unix_ms()
    ));
    let candidate_metrics_path = metrics_dir.join(format!(
        "perfcmp-{}-{}-candidate.json",
        manifest.suite,
        now_unix_ms()
    ));
    let diagnostics_seed = fnv1a64(format!("{seed}:diagnostics").as_bytes());
    let baseline_diag = match run_perf_once_in_worktree(
        &baseline_worktree,
        &relative_benchmark_root,
        &relative_manifest_path,
        config.profile,
        jobs,
        timeout_ms,
        config.perf_debug,
        diagnostics_seed ^ 0x11,
        &diagnostics_env_knobs,
        PerfRunMode::Diagnostics,
        Some(&baseline_metrics_path),
    ) {
        Ok(run) => run,
        Err(err) => {
            eprintln!("perfcmp error: diagnostics baseline run failed: {err}");
            cleanup_perfcmp_worktrees(
                &repo_root,
                &baseline_worktree,
                &candidate_worktree,
                &perfcmp_temp,
            );
            return EXIT_CODEGEN;
        }
    };
    let candidate_diag = match run_perf_once_in_worktree(
        &candidate_worktree,
        &relative_benchmark_root,
        &relative_manifest_path,
        config.profile,
        jobs,
        timeout_ms,
        config.perf_debug,
        diagnostics_seed ^ 0x22,
        &diagnostics_env_knobs,
        PerfRunMode::Diagnostics,
        Some(&candidate_metrics_path),
    ) {
        Ok(run) => run,
        Err(err) => {
            eprintln!("perfcmp error: diagnostics candidate run failed: {err}");
            cleanup_perfcmp_worktrees(
                &repo_root,
                &baseline_worktree,
                &candidate_worktree,
                &perfcmp_temp,
            );
            return EXIT_CODEGEN;
        }
    };

    cleanup_perfcmp_worktrees(
        &repo_root,
        &baseline_worktree,
        &candidate_worktree,
        &perfcmp_temp,
    );

    let mut scenario_results = Vec::new();
    let mut win_count = 0usize;
    let mut regression_count = 0usize;
    let mut no_signal_count = 0usize;
    let mut unstable_count = 0usize;
    let mut unstable_critical_count = 0usize;
    let mut gate_failures = Vec::new();
    let mut bootstrap_seed = fnv1a64(
        format!(
            "{}:{}:{}:{}:{}",
            config.baseline_ref,
            config.candidate_ref,
            config.profile.as_str(),
            measured_pairs,
            config.min_effect_pct
        )
        .as_bytes(),
    );
    for scenario in scenarios {
        let Some(samples) = by_scenario.get(&scenario.id) else {
            continue;
        };
        if samples.is_empty() {
            continue;
        }
        let mut baseline_times: Vec<u128> = samples.iter().map(|(base, _)| *base).collect();
        let mut candidate_times: Vec<u128> = samples.iter().map(|(_, cand)| *cand).collect();
        baseline_times.sort_unstable();
        candidate_times.sort_unstable();
        let mut deltas_pct: Vec<f64> = samples
            .iter()
            .map(|(base, cand)| pct_delta_runtime(*cand, *base))
            .collect();
        deltas_pct.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let pairs: Vec<PerfCmpPairedSample> = samples
            .iter()
            .map(
                |(baseline_runtime, candidate_runtime)| PerfCmpPairedSample {
                    baseline_runtime_ns: *baseline_runtime,
                    candidate_runtime_ns: *candidate_runtime,
                    delta_pct: pct_delta_runtime(*candidate_runtime, *baseline_runtime),
                },
            )
            .collect();
        let delta_pct_median = median_f64_sorted(&deltas_pct).unwrap_or(0.0);
        let (ci_low_pct, ci_high_pct) = bootstrap_ci_percentile(
            &deltas_pct,
            config.confidence_pct,
            10_000,
            &mut bootstrap_seed,
        );
        let candidate_as_f64: Vec<f64> = candidate_times.iter().map(|v| *v as f64).collect();
        let stability_cv_pct = coefficient_of_variation(&candidate_as_f64);
        let stability_iqr_over_median = iqr_over_median(&candidate_times);
        let cv_limit = if manifest.suite.eq_ignore_ascii_case("micro") {
            2.5
        } else {
            5.0
        };
        let baseline_runtime_ns_median = median_u128_sorted(&baseline_times).unwrap_or(0);
        let candidate_runtime_ns_median = median_u128_sorted(&candidate_times).unwrap_or(0);
        let meets_min_runtime = scenario
            .min_runtime_ms
            .map(|min_ms| {
                let min_ns = (min_ms as u128) * 1_000_000;
                baseline_runtime_ns_median >= min_ns && candidate_runtime_ns_median >= min_ns
            })
            .unwrap_or(true);
        let is_stable =
            meets_min_runtime && stability_cv_pct <= cv_limit && stability_iqr_over_median <= 0.15;
        let verdict = classify_perfcmp_verdict(ci_low_pct, ci_high_pct, config.min_effect_pct);
        match verdict {
            "win" => win_count += 1,
            "regression" => regression_count += 1,
            _ => no_signal_count += 1,
        }
        if !is_stable {
            unstable_count += 1;
            if scenario.class.eq_ignore_ascii_case("critical") && !scenario.allow_unstable {
                unstable_critical_count += 1;
            }
        }
        scenario_results.push(PerfCmpScenarioResult {
            id: scenario.id.clone(),
            test_name: scenario.test_name.clone(),
            class: scenario.class.clone(),
            ops: scenario.ops,
            pair_count: samples.len(),
            baseline_runtime_ns_median,
            candidate_runtime_ns_median,
            min_runtime_ms: scenario.min_runtime_ms,
            meets_min_runtime,
            timeout_ms: scenario.timeout_ms,
            delta_pct_median,
            ci_low_pct,
            ci_high_pct,
            stability_cv_pct,
            stability_iqr_over_median,
            is_stable,
            allow_unstable: scenario.allow_unstable,
            verdict: verdict.to_string(),
            deltas_pct,
            pairs,
        });
    }
    scenario_results.sort_by(|a, b| a.test_name.cmp(&b.test_name));

    if !manifest.optional {
        match config.profile {
            PerfProfile::Smoke => {}
            PerfProfile::Standard => {
                for result in &scenario_results {
                    if result.class.eq_ignore_ascii_case("critical")
                        && result.verdict == "regression"
                    {
                        gate_failures.push(format!(
                            "critical scenario regression: {} ({:.2}%, CI [{:.2}%, {:.2}%])",
                            result.test_name,
                            result.delta_pct_median,
                            result.ci_low_pct,
                            result.ci_high_pct
                        ));
                    }
                }
            }
            PerfProfile::Closure1080p120 => {}
            PerfProfile::Deep => {
                for result in &scenario_results {
                    if result.is_stable && result.verdict == "regression" {
                        gate_failures.push(format!(
                            "stable regression: {} ({:.2}%, CI [{:.2}%, {:.2}%])",
                            result.test_name,
                            result.delta_pct_median,
                            result.ci_low_pct,
                            result.ci_high_pct
                        ));
                    }
                }
                if unstable_critical_count > 0 {
                    gate_failures.push(format!(
                        "unstable critical scenarios: {unstable_critical_count}"
                    ));
                }
                if !scenario_results.is_empty() {
                    let unstable_ratio = unstable_count as f64 / scenario_results.len() as f64;
                    if unstable_ratio > 0.20 {
                        gate_failures.push(format!(
                            "unstable scenario ratio {:.2}% exceeds 20%",
                            unstable_ratio * 100.0
                        ));
                    }
                }
            }
        }
    }

    let diagnostics = PerfCmpDiagnostics {
        baseline_perf_report_path: baseline_diag.report_path.display().to_string(),
        candidate_perf_report_path: candidate_diag.report_path.display().to_string(),
        baseline_metrics_path: baseline_diag
            .metrics_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_default(),
        candidate_metrics_path: candidate_diag
            .metrics_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_default(),
        metric_deltas_pct: compute_summary_metric_deltas_pct(
            &baseline_diag.summary,
            &candidate_diag.summary,
        ),
    };
    let summary = PerfCmpSummary {
        scenario_count: scenario_results.len(),
        win_count,
        regression_count,
        no_signal_count,
        unstable_count,
        unstable_critical_count,
        non_blocking_suite: manifest.optional,
        gate_passed: gate_failures.is_empty(),
        gate_failures,
    };
    let report = PerfCmpReport {
        version: 1,
        generated_at_unix_ms: now_unix_ms(),
        suite: manifest.suite.clone(),
        optional_suite: manifest.optional,
        skipped_reason: None,
        profile: config.profile.as_str().to_string(),
        baseline_ref: config.baseline_ref.clone(),
        candidate_ref: config.candidate_ref.clone(),
        benchmark_root: benchmark_root.display().to_string(),
        manifest_path: manifest_path.display().to_string(),
        warmup_pairs,
        measured_pairs,
        confidence_pct: config.confidence_pct,
        min_effect_pct: config.min_effect_pct,
        env_knobs: timing_env_knobs.clone(),
        diagnostics: Some(diagnostics),
        host: collect_perfcmp_host_info(),
        scenarios: scenario_results,
        summary,
    };
    let markdown_path = match write_perfcmp_report(config, &report) {
        Ok(markdown_path) => markdown_path,
        Err(err) => {
            eprintln!("perfcmp error: {err}");
            return EXIT_CODEGEN;
        }
    };
    let baseline_scenarios: Vec<PerfCmpSuiteBaselineScenario> = report
        .scenarios
        .iter()
        .map(|scenario| PerfCmpSuiteBaselineScenario {
            id: scenario.id.clone(),
            test_name: scenario.test_name.clone(),
            ops: scenario.ops,
            runtime_ns_median: scenario.baseline_runtime_ns_median,
        })
        .collect();
    let candidate_scenarios: Vec<PerfCmpSuiteBaselineScenario> = report
        .scenarios
        .iter()
        .map(|scenario| PerfCmpSuiteBaselineScenario {
            id: scenario.id.clone(),
            test_name: scenario.test_name.clone(),
            ops: scenario.ops,
            runtime_ns_median: scenario.candidate_runtime_ns_median,
        })
        .collect();
    if let Err(err) = write_suite_baseline_artifact(
        &manifest.suite,
        config.profile,
        &config.baseline_ref,
        &config.output_json,
        &baseline_scenarios,
    ) {
        eprintln!("perfcmp warning: {err}");
    }
    if let Err(err) = write_suite_baseline_artifact(
        &manifest.suite,
        config.profile,
        &config.candidate_ref,
        &config.output_json,
        &candidate_scenarios,
    ) {
        eprintln!("perfcmp warning: {err}");
    }
    if matches!(config.output_format, OutputFormat::Pretty) {
        println!("perfcmp report: {}", config.output_json.display());
        println!("perfcmp markdown: {}", markdown_path.display());
        println!(
            "perfcmp summary: wins={} regressions={} no_signal={} unstable={} non_blocking={}",
            report.summary.win_count,
            report.summary.regression_count,
            report.summary.no_signal_count,
            report.summary.unstable_count,
            report.summary.non_blocking_suite
        );
    }
    if report.summary.gate_passed {
        EXIT_OK
    } else {
        for failure in &report.summary.gate_failures {
            eprintln!("perfcmp gate failed: {failure}");
        }
        EXIT_CODEGEN
    }
}

fn profile_pair_counts(
    manifest: &command_handlers::BenchmarkManifest,
    profile: PerfProfile,
    warmup_override: Option<usize>,
    measure_override: Option<usize>,
) -> (usize, usize) {
    let (mut warmup, mut measure) = match profile {
        PerfProfile::Smoke => (2usize, 6usize),
        PerfProfile::Standard => (3usize, 10usize),
        PerfProfile::Deep => (5usize, 18usize),
        PerfProfile::Closure1080p120 => (4usize, 12usize),
    };
    if let Some(config) = manifest.profiles.config_for(profile) {
        warmup = config.warmup_pairs.max(1);
        measure = config.measure_pairs.max(1);
    }
    if let Some(v) = warmup_override {
        warmup = v.max(1);
    }
    if let Some(v) = measure_override {
        measure = v.max(1);
    }
    (warmup, measure)
}

#[derive(Clone, Copy)]
enum PerfCmpVariant {
    Baseline,
    Candidate,
}

impl PerfCmpVariant {
    fn as_str(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::Candidate => "candidate",
        }
    }
}

#[derive(Clone, Copy)]
enum PerfRunMode {
    Timing,
    Diagnostics,
}

#[derive(Debug, Clone)]
struct PerfcmpRunSummary {
    runtime_by_test: HashMap<String, u128>,
    summary: command_handlers::PerfSummary,
    report_path: PathBuf,
    metrics_path: Option<PathBuf>,
}

fn perfcmp_env_knobs(profile: PerfProfile, run_mode: PerfRunMode) -> BTreeMap<String, String> {
    let mut knobs = BTreeMap::new();
    knobs.insert(
        "WRELA_RUNTIME_METRICS".to_string(),
        match run_mode {
            PerfRunMode::Timing => "0".to_string(),
            PerfRunMode::Diagnostics => "1".to_string(),
        },
    );
    knobs.insert("WRELA_RUNTIME_PROFILE".to_string(), "release".to_string());
    knobs.insert(
        "WRELA_RUNTIME_SCHED_PROFILE_AUTO".to_string(),
        "0".to_string(),
    );
    if env::consts::OS == "linux" {
        let value = match profile {
            PerfProfile::Smoke => "1",
            PerfProfile::Standard => "1",
            PerfProfile::Deep => "0",
            PerfProfile::Closure1080p120 => "0",
        };
        knobs.insert("WRELA_DISABLE_IO_URING".to_string(), value.to_string());
    }
    knobs
}

fn effective_perfcmp_timeout_ms(
    scenarios: &[&command_handlers::BenchmarkScenario],
    cli_timeout_ms: Option<u64>,
) -> Option<u64> {
    let scenario_max = scenarios
        .iter()
        .filter_map(|scenario| scenario.timeout_ms)
        .max();
    match (cli_timeout_ms, scenario_max) {
        (Some(cli), Some(from_manifest)) => Some(cli.max(from_manifest)),
        (Some(cli), None) => Some(cli),
        (None, Some(from_manifest)) => Some(from_manifest),
        (None, None) => None,
    }
}

fn cleanup_perfcmp_worktrees(
    repo_root: &Path,
    baseline_worktree: &Path,
    candidate_worktree: &Path,
    temp_root: &Path,
) {
    let _ = git_worktree_remove(repo_root, baseline_worktree);
    let _ = git_worktree_remove(repo_root, candidate_worktree);
    let _ = fs::remove_dir_all(temp_root);
}

fn write_perfcmp_report(config: &PerfCmpConfig, report: &PerfCmpReport) -> Result<PathBuf, String> {
    if let Some(parent) = config.output_json.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "failed to create report directory {}: {}",
                parent.display(),
                err
            )
        })?;
    }
    let json = serde_json::to_vec_pretty(report)
        .map_err(|err| format!("failed to serialize report: {err}"))?;
    fs::write(&config.output_json, json).map_err(|err| {
        format!(
            "failed to write report {}: {}",
            config.output_json.display(),
            err
        )
    })?;
    let markdown_path = config.output_json.with_extension("md");
    fs::write(&markdown_path, render_perfcmp_markdown(report)).map_err(|err| {
        format!(
            "failed to write markdown report {}: {}",
            markdown_path.display(),
            err
        )
    })?;
    Ok(markdown_path)
}

fn command_stdout_trimmed(command: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(command).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if value.is_empty() { None } else { Some(value) }
}

fn detect_cpu_model() -> String {
    match env::consts::OS {
        "linux" => {
            if let Ok(text) = fs::read_to_string("/proc/cpuinfo") {
                for line in text.lines() {
                    if let Some(value) = line.strip_prefix("model name\t: ") {
                        return value.trim().to_string();
                    }
                }
            }
            command_stdout_trimmed("uname", &["-m"]).unwrap_or_else(|| "unknown".to_string())
        }
        "macos" => command_stdout_trimmed("sysctl", &["-n", "machdep.cpu.brand_string"])
            .unwrap_or_else(|| "unknown".to_string()),
        _ => command_stdout_trimmed("uname", &["-m"]).unwrap_or_else(|| "unknown".to_string()),
    }
}

fn detect_physical_cpu_count() -> Option<usize> {
    match env::consts::OS {
        "macos" => command_stdout_trimmed("sysctl", &["-n", "hw.physicalcpu"])
            .and_then(|value| value.parse::<usize>().ok()),
        "linux" => command_stdout_trimmed("nproc", &["--all"])
            .and_then(|value| value.parse::<usize>().ok()),
        _ => None,
    }
}

fn collect_perfcmp_host_info() -> PerfCmpHostInfo {
    PerfCmpHostInfo {
        os: env::consts::OS.to_string(),
        kernel: command_stdout_trimmed("uname", &["-r"]).unwrap_or_else(|| "unknown".to_string()),
        arch: env::consts::ARCH.to_string(),
        cpu_model: detect_cpu_model(),
        logical_cpu_count: std::thread::available_parallelism()
            .map(|count| count.get())
            .unwrap_or(1),
        physical_cpu_count: detect_physical_cpu_count(),
    }
}

fn sanitize_git_ref_for_filename(git_ref: &str) -> String {
    let mut value = String::with_capacity(git_ref.len());
    for ch in git_ref.chars() {
        if ch.is_ascii_alphanumeric() || ch == '.' || ch == '_' || ch == '-' {
            value.push(ch);
        } else {
            value.push('_');
        }
    }
    if value.is_empty() {
        "unknown".to_string()
    } else {
        value
    }
}

fn write_suite_baseline_artifact(
    suite: &str,
    profile: PerfProfile,
    git_ref: &str,
    source_report: &Path,
    scenarios: &[PerfCmpSuiteBaselineScenario],
) -> Result<PathBuf, String> {
    let filename = format!(
        "{}-{}-{}.json",
        suite,
        profile.as_str(),
        sanitize_git_ref_for_filename(git_ref)
    );
    let path = PathBuf::from(".artifacts")
        .join("perf")
        .join("baselines")
        .join(filename);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "failed to create suite baseline dir {}: {}",
                parent.display(),
                err
            )
        })?;
    }
    let artifact = PerfCmpSuiteBaseline {
        version: 1,
        generated_at_unix_ms: now_unix_ms(),
        suite: suite.to_string(),
        profile: profile.as_str().to_string(),
        git_ref: git_ref.to_string(),
        source_report: source_report.display().to_string(),
        scenarios: scenarios.to_vec(),
    };
    let json = serde_json::to_vec_pretty(&artifact)
        .map_err(|err| format!("failed to serialize suite baseline: {err}"))?;
    fs::write(&path, json)
        .map_err(|err| format!("failed to write suite baseline {}: {}", path.display(), err))?;
    Ok(path)
}

fn pct_delta_higher_is_better(candidate: f64, baseline: f64) -> Option<f64> {
    if baseline == 0.0 {
        return None;
    }
    Some(((candidate - baseline) / baseline) * 100.0)
}

fn pct_delta_lower_is_better(candidate: f64, baseline: f64) -> Option<f64> {
    if baseline == 0.0 {
        return None;
    }
    Some(((baseline - candidate) / baseline) * 100.0)
}

fn compute_summary_metric_deltas_pct(
    baseline: &command_handlers::PerfSummary,
    candidate: &command_handlers::PerfSummary,
) -> BTreeMap<String, f64> {
    let mut deltas = BTreeMap::new();
    deltas.insert(
        "runtime_p50_ns".to_string(),
        pct_delta_runtime(candidate.runtime_p50_ns, baseline.runtime_p50_ns),
    );
    deltas.insert(
        "runtime_p95_ns".to_string(),
        pct_delta_runtime(candidate.runtime_p95_ns, baseline.runtime_p95_ns),
    );
    deltas.insert(
        "runtime_p99_ns".to_string(),
        pct_delta_runtime(candidate.runtime_p99_ns, baseline.runtime_p99_ns),
    );
    deltas.insert(
        "allocs_per_request".to_string(),
        pct_delta_lower_is_better(candidate.allocs_per_request, baseline.allocs_per_request)
            .unwrap_or(0.0),
    );
    if let Some(delta) = pct_delta_higher_is_better(
        candidate.compile_throughput_tests_per_sec,
        baseline.compile_throughput_tests_per_sec,
    ) {
        deltas.insert("compile_throughput_tests_per_sec".to_string(), delta);
    }
    if let Some(delta) =
        pct_delta_higher_is_better(candidate.dispatch_hit_ratio, baseline.dispatch_hit_ratio)
    {
        deltas.insert("dispatch_hit_ratio".to_string(), delta);
    }
    if let (Some(base), Some(cand)) = (baseline.queue_age_p99_ns, candidate.queue_age_p99_ns) {
        deltas.insert(
            "queue_age_p99_ns".to_string(),
            pct_delta_runtime(cand, base),
        );
    }
    if let (Some(base), Some(cand)) = (
        baseline.scheduler_dispatch_p99_ns,
        candidate.scheduler_dispatch_p99_ns,
    ) {
        deltas.insert(
            "scheduler_dispatch_p99_ns".to_string(),
            pct_delta_runtime(cand, base),
        );
    }
    deltas
}

fn should_skip_optional_suite(optional: bool, suite: &str, host_os: &str) -> bool {
    optional && suite.eq_ignore_ascii_case("linux") && host_os != "linux"
}

fn run_perf_once_in_worktree(
    worktree_root: &Path,
    benchmark_rel_path: &Path,
    manifest_rel_path: &Path,
    profile: PerfProfile,
    jobs: Option<usize>,
    timeout_ms: Option<u64>,
    perf_debug: bool,
    seed: u64,
    env_knobs: &BTreeMap<String, String>,
    run_mode: PerfRunMode,
    metrics_path: Option<&Path>,
) -> Result<PerfcmpRunSummary, String> {
    let run_label = match run_mode {
        PerfRunMode::Timing => "timing",
        PerfRunMode::Diagnostics => "diagnostics",
    };
    let report_path = worktree_root.join(".artifacts").join("perf").join(format!(
        "perfcmp-{}-{}-{}.json",
        profile.as_str(),
        run_label,
        now_unix_ms()
    ));
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "failed to create worktree perf artifact dir {}: {}",
                parent.display(),
                err
            )
        })?;
    }
    let mut command = Command::new("cargo");
    command.current_dir(worktree_root);
    command.args(["run", "-q", "-p", "wrela", "--", "perf"]);
    command.arg(benchmark_rel_path);
    command.arg("--runs=1");
    command.arg(format!("--baseline-out={}", report_path.display()));
    command.arg(format!(
        "--benchmark-manifest={}",
        manifest_rel_path.display()
    ));
    command.arg(format!("--profile={}", profile.as_str()));
    command.arg(format!("--seed={seed}"));
    command.arg("--error-format=json");
    if let Some(jobs) = jobs {
        command.arg(format!("--jobs={jobs}"));
    }
    if let Some(timeout_ms) = timeout_ms {
        command.arg(format!("--test-timeout-ms={timeout_ms}"));
    }
    if perf_debug {
        command.arg("--perf-debug");
    }
    for (key, value) in env_knobs {
        command.env(key, value);
    }
    let mut metrics_path_buf = None;
    if let Some(metrics_path) = metrics_path {
        if let Some(parent) = metrics_path.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                format!("failed to create metrics dir {}: {}", parent.display(), err)
            })?;
        }
        command.env("WRELA_METRICS_PATH", metrics_path);
        metrics_path_buf = Some(metrics_path.to_path_buf());
    }
    let output = command
        .output()
        .map_err(|err| format!("failed to execute perf command in worktree: {err}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "perf command failed in {} with exit {:?}: {}",
            worktree_root.display(),
            output.status.code(),
            stderr.trim()
        ));
    }
    let summary = command_handlers::load_perf_baseline_summary(&report_path)?;
    let Some(cases) = summary.cases.clone() else {
        return Err(format!(
            "perf summary {} did not include per-case samples",
            report_path.display()
        ));
    };
    let mut runtime_by_test = HashMap::new();
    for case in cases {
        runtime_by_test.insert(case.name, case.runtime_ns);
    }
    Ok(PerfcmpRunSummary {
        runtime_by_test,
        summary,
        report_path,
        metrics_path: metrics_path_buf,
    })
}

fn git_worktree_add(repo_root: &Path, worktree: &Path, git_ref: &str) -> Result<(), String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["worktree", "add", "--detach"])
        .arg(worktree)
        .arg(git_ref)
        .output()
        .map_err(|err| format!("failed to invoke git worktree add: {err}"))?;
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "git worktree add failed for ref `{}`: {}",
        git_ref,
        String::from_utf8_lossy(&output.stderr).trim()
    ))
}

fn git_worktree_remove(repo_root: &Path, worktree: &Path) -> Result<(), String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["worktree", "remove", "--force"])
        .arg(worktree)
        .output()
        .map_err(|err| format!("failed to invoke git worktree remove: {err}"))?;
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "git worktree remove failed for {}: {}",
        worktree.display(),
        String::from_utf8_lossy(&output.stderr).trim()
    ))
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;
    let mut hash = OFFSET;
    for b in bytes {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

fn random_bool(seed: &mut u64) -> bool {
    let mut x = *seed;
    x ^= x << 7;
    x ^= x >> 9;
    x ^= x << 8;
    *seed = x;
    (x & 1) == 0
}

fn pct_delta_runtime(candidate: u128, baseline: u128) -> f64 {
    if baseline == 0 {
        return 0.0;
    }
    ((baseline as f64 - candidate as f64) / baseline as f64) * 100.0
}

fn classify_perfcmp_verdict(
    ci_low_pct: f64,
    ci_high_pct: f64,
    min_effect_pct: f64,
) -> &'static str {
    if ci_low_pct > min_effect_pct {
        "win"
    } else if ci_high_pct < -min_effect_pct {
        "regression"
    } else {
        "no_signal"
    }
}

fn median_u128_sorted(samples: &[u128]) -> Option<u128> {
    if samples.is_empty() {
        None
    } else {
        Some(samples[samples.len() / 2])
    }
}

fn median_f64_sorted(samples: &[f64]) -> Option<f64> {
    if samples.is_empty() {
        None
    } else {
        Some(samples[samples.len() / 2])
    }
}

fn percentile_u128(samples: &[u128], pct: f64) -> u128 {
    if samples.is_empty() {
        return 0;
    }
    let idx = ((samples.len() as f64 - 1.0) * pct).round() as usize;
    samples[idx.min(samples.len() - 1)]
}

fn iqr_over_median(samples: &[u128]) -> f64 {
    if samples.len() < 4 {
        return 0.0;
    }
    let q1 = percentile_u128(samples, 0.25);
    let q3 = percentile_u128(samples, 0.75);
    let median = percentile_u128(samples, 0.5).max(1);
    (q3.saturating_sub(q1)) as f64 / median as f64
}

fn bootstrap_ci_percentile(
    values: &[f64],
    confidence_pct: f64,
    resamples: usize,
    seed: &mut u64,
) -> (f64, f64) {
    if values.is_empty() {
        return (0.0, 0.0);
    }
    let confidence = (confidence_pct / 100.0).clamp(0.5, 0.999);
    let alpha = (1.0 - confidence) / 2.0;
    let mut dist = Vec::with_capacity(resamples.max(1));
    for _ in 0..resamples.max(1) {
        let mut sample = Vec::with_capacity(values.len());
        for _ in 0..values.len() {
            let index = bootstrap_index(seed, values.len());
            sample.push(values[index]);
        }
        sample.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        dist.push(median_f64_sorted(&sample).unwrap_or(0.0));
    }
    dist.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let low_idx = ((alpha * dist.len() as f64).floor() as usize).min(dist.len() - 1);
    let high_idx = (((1.0 - alpha) * dist.len() as f64).ceil() as usize)
        .saturating_sub(1)
        .min(dist.len() - 1);
    (dist[low_idx], dist[high_idx])
}

fn bootstrap_index(seed: &mut u64, len: usize) -> usize {
    let mut x = *seed;
    x = x.wrapping_mul(6364136223846793005).wrapping_add(1);
    *seed = x;
    (x as usize) % len.max(1)
}

fn coefficient_of_variation(values: &[f64]) -> f64 {
    if values.len() <= 1 {
        return 0.0;
    }
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    if mean.abs() <= f64::EPSILON {
        return 0.0;
    }
    let variance = values
        .iter()
        .map(|value| {
            let d = *value - mean;
            d * d
        })
        .sum::<f64>()
        / values.len() as f64;
    (variance.sqrt() / mean.abs()) * 100.0
}

fn now_unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_millis()
}

fn render_perfcmp_markdown(report: &PerfCmpReport) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "# perfcmp report ({}/{})\n\n",
        report.suite, report.profile
    ));
    out.push_str(&format!(
        "- optional suite: `{}`\n- baseline: `{}`\n- candidate: `{}`\n- warmup/measured pairs: `{}/{}`\n- confidence: `{:.1}%`\n- min effect: `{:.2}%`\n\n",
        report.optional_suite,
        report.baseline_ref,
        report.candidate_ref,
        report.warmup_pairs,
        report.measured_pairs,
        report.confidence_pct,
        report.min_effect_pct
    ));
    if let Some(reason) = report.skipped_reason.as_deref() {
        out.push_str(&format!("## Skipped\n- {reason}\n\n"));
        return out;
    }
    out.push_str("| scenario | class | baseline med (ns) | candidate med (ns) | delta med (%) | CI low (%) | CI high (%) | stable | verdict |\n");
    out.push_str("| --- | --- | ---: | ---: | ---: | ---: | ---: | --- | --- |\n");
    for scenario in &report.scenarios {
        out.push_str(&format!(
            "| `{}` | `{}` | {} | {} | {:.2} | {:.2} | {:.2} | `{}` | `{}` |\n",
            scenario.test_name,
            scenario.class,
            scenario.baseline_runtime_ns_median,
            scenario.candidate_runtime_ns_median,
            scenario.delta_pct_median,
            scenario.ci_low_pct,
            scenario.ci_high_pct,
            scenario.is_stable,
            scenario.verdict
        ));
    }
    if let Some(diagnostics) = report.diagnostics.as_ref() {
        out.push_str("\n## Diagnostics\n");
        out.push_str(&format!(
            "- baseline perf report: `{}`\n- candidate perf report: `{}`\n- baseline metrics path: `{}`\n- candidate metrics path: `{}`\n",
            diagnostics.baseline_perf_report_path,
            diagnostics.candidate_perf_report_path,
            diagnostics.baseline_metrics_path,
            diagnostics.candidate_metrics_path
        ));
        if !diagnostics.metric_deltas_pct.is_empty() {
            out.push_str("\n| metric | delta (%) |\n| --- | ---: |\n");
            for (metric, delta) in &diagnostics.metric_deltas_pct {
                out.push_str(&format!("| `{metric}` | {:.2} |\n", delta));
            }
        }
    }
    if !report.summary.gate_failures.is_empty() {
        out.push_str("\n## Gate Failures\n");
        for failure in &report.summary.gate_failures {
            out.push_str(&format!("- {failure}\n"));
        }
    }
    out
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PerfCmpReport {
    version: u32,
    generated_at_unix_ms: u128,
    suite: String,
    optional_suite: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    skipped_reason: Option<String>,
    profile: String,
    baseline_ref: String,
    candidate_ref: String,
    benchmark_root: String,
    manifest_path: String,
    warmup_pairs: usize,
    measured_pairs: usize,
    confidence_pct: f64,
    min_effect_pct: f64,
    env_knobs: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    diagnostics: Option<PerfCmpDiagnostics>,
    host: PerfCmpHostInfo,
    scenarios: Vec<PerfCmpScenarioResult>,
    summary: PerfCmpSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PerfCmpHostInfo {
    os: String,
    kernel: String,
    arch: String,
    cpu_model: String,
    logical_cpu_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    physical_cpu_count: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PerfCmpScenarioResult {
    id: String,
    test_name: String,
    class: String,
    ops: u64,
    pair_count: usize,
    baseline_runtime_ns_median: u128,
    candidate_runtime_ns_median: u128,
    #[serde(skip_serializing_if = "Option::is_none")]
    min_runtime_ms: Option<u64>,
    meets_min_runtime: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    timeout_ms: Option<u64>,
    delta_pct_median: f64,
    ci_low_pct: f64,
    ci_high_pct: f64,
    stability_cv_pct: f64,
    stability_iqr_over_median: f64,
    is_stable: bool,
    allow_unstable: bool,
    verdict: String,
    deltas_pct: Vec<f64>,
    pairs: Vec<PerfCmpPairedSample>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PerfCmpPairedSample {
    baseline_runtime_ns: u128,
    candidate_runtime_ns: u128,
    delta_pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PerfCmpDiagnostics {
    baseline_perf_report_path: String,
    candidate_perf_report_path: String,
    baseline_metrics_path: String,
    candidate_metrics_path: String,
    metric_deltas_pct: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PerfCmpSummary {
    scenario_count: usize,
    win_count: usize,
    regression_count: usize,
    no_signal_count: usize,
    unstable_count: usize,
    unstable_critical_count: usize,
    non_blocking_suite: bool,
    gate_passed: bool,
    gate_failures: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PerfCmpSuiteBaseline {
    version: u32,
    generated_at_unix_ms: u128,
    suite: String,
    profile: String,
    git_ref: String,
    source_report: String,
    scenarios: Vec<PerfCmpSuiteBaselineScenario>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PerfCmpSuiteBaselineScenario {
    id: String,
    test_name: String,
    ops: u64,
    runtime_ns_median: u128,
}

pub(super) struct MatrixCommandInput {
    pub(super) trace: bool,
    pub(super) program_args: Vec<String>,
    pub(super) path_arg: Option<String>,
    pub(super) perf_runs: Option<usize>,
    pub(super) perf_gate_path: Option<String>,
    pub(super) perf_max_regression_pct: Option<f64>,
    pub(super) kpi_thresholds: KpiThresholds,
}

pub(super) fn execute_matrix_command(input: MatrixCommandInput) -> i32 {
    if input.trace {
        eprintln!("build: command matrix");
    }
    if !input.program_args.is_empty() {
        eprintln!("error: unexpected extra arguments");
        return EXIT_USAGE;
    }
    let workspace_root = match input.path_arg {
        Some(path) => PathBuf::from(path),
        None => match std::env::current_dir() {
            Ok(path) => path,
            Err(err) => {
                eprintln!("error: failed to resolve current directory: {err}");
                return EXIT_USAGE;
            }
        },
    };
    if !workspace_root.is_dir() {
        eprintln!(
            "error: matrix target must be an existing directory: {}",
            workspace_root.display()
        );
        return EXIT_USAGE;
    }
    let runs = input.perf_runs.unwrap_or(1).max(1);
    run_matrix(
        &workspace_root,
        runs,
        input.perf_gate_path.as_deref(),
        input.perf_max_regression_pct.unwrap_or(5.0),
        &input.kpi_thresholds,
    )
}

#[derive(Debug, Serialize)]
struct MatrixEvidenceBundle {
    version: u32,
    generated_at_unix_ms: u128,
    workspace_root: String,
    success: bool,
    exit_code: i32,
    perf_runs: usize,
    perf_baseline_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    perf_gate_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    kpi_thresholds: Option<KpiThresholds>,
    #[serde(skip_serializing_if = "Option::is_none")]
    perf_summary: Option<command_handlers::PerfSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    check_lane_kpis: Option<CheckLaneKpis>,
    steps: Vec<MatrixStepEvidence>,
}

#[derive(Debug, Clone, Serialize)]
struct CheckLaneKpis {
    typed_lane_total: u64,
    boxed_lane_total: u64,
    typed_lane_ratio: f64,
}

#[derive(Debug, Serialize)]
struct MatrixStepEvidence {
    name: String,
    command: Vec<String>,
    cwd: String,
    started_at_unix_ms: u128,
    duration_ms: u128,
    exit_code: i32,
    success: bool,
    stdout_log: String,
    stderr_log: String,
}

struct MatrixStepSpec<'a> {
    name: &'a str,
    program: &'a Path,
    args: Vec<String>,
}

fn run_matrix(
    workspace_root: &Path,
    perf_runs: usize,
    perf_gate_path: Option<&str>,
    perf_max_regression_pct: f64,
    kpi_thresholds: &KpiThresholds,
) -> i32 {
    let artifact_dir = workspace_root.join(".artifacts").join("matrix");
    if let Err(err) = fs::create_dir_all(&artifact_dir) {
        eprintln!(
            "matrix error: failed to create {}: {}",
            artifact_dir.display(),
            err
        );
        return EXIT_CODEGEN;
    }

    let generated_at_unix_ms = now_unix_ms();
    let bundle_path = artifact_dir.join(format!("matrix-{}.json", generated_at_unix_ms));
    let latest_path = artifact_dir.join("matrix-latest.json");
    let perf_baseline_path = artifact_dir.join("perf-baseline.json");

    let cargo_bin = env::var("WRELA_MATRIX_CARGO_BIN").unwrap_or_else(|_| "cargo".to_string());
    let self_bin = env::var("WRELA_MATRIX_SELF_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|_| env::current_exe().unwrap_or_else(|_| PathBuf::from("wrela")));

    let mut perf_args = vec![
        "perf".to_string(),
        format!("--runs={perf_runs}"),
        format!("--baseline-out={}", perf_baseline_path.display()),
        "--lane=spec".to_string(),
        "language/spec".to_string(),
    ];
    if let Some(path) = perf_gate_path {
        perf_args.push(format!("--perf-gate={path}"));
        perf_args.push(format!(
            "--perf-max-regression-pct={}",
            perf_max_regression_pct
        ));
        if let Some(value) = kpi_thresholds.check_fallback_max {
            perf_args.push(format!("--kpi-check-fallback-max={value}"));
        }
        if let Some(value) = kpi_thresholds.check_batch_min {
            perf_args.push(format!("--kpi-check-batch-min={value}"));
        }
        if let Some(value) = kpi_thresholds.scheduler_p99_improve_min_pct {
            perf_args.push(format!("--kpi-scheduler-p99-improve-min-pct={value}"));
        }
        if let Some(value) = kpi_thresholds.rewrite_overhead_max_pct {
            perf_args.push(format!("--kpi-rewrite-overhead-max-pct={value}"));
        }
        if let Some(value) = kpi_thresholds.actor_throughput_improve_min_pct {
            perf_args.push(format!("--kpi-actor-throughput-improve-min-pct={value}"));
        }
        if let Some(value) = kpi_thresholds.queue_age_p99_max_regress_pct {
            perf_args.push(format!("--kpi-queue-age-p99-max-regress-pct={value}"));
        }
        if let Some(value) = kpi_thresholds.starvation_violations_max {
            perf_args.push(format!("--kpi-starvation-violations-max={value}"));
        }
        if let Some(value) = kpi_thresholds.scheduler_throughput_improve_min_pct {
            perf_args.push(format!(
                "--kpi-scheduler-throughput-improve-min-pct={value}"
            ));
        }
        if let Some(value) = kpi_thresholds.scheduler_loop_p99_max_regress_pct {
            perf_args.push(format!("--kpi-scheduler-loop-p99-max-regress-pct={value}"));
        }
        if let Some(value) = kpi_thresholds.scheduler_local_hit_min {
            perf_args.push(format!("--kpi-scheduler-local-hit-min={value}"));
        }
    }

    let steps = vec![
        MatrixStepSpec {
            name: "cargo-test-workspace",
            program: Path::new(&cargo_bin),
            args: vec!["test".to_string(), "--workspace".to_string()],
        },
        MatrixStepSpec {
            name: "spec-tests",
            program: &self_bin,
            args: vec![
                "test".to_string(),
                "language/spec".to_string(),
                "--lane=spec".to_string(),
            ],
        },
        MatrixStepSpec {
            name: "perf-harness",
            program: &self_bin,
            args: perf_args,
        },
    ];

    let mut evidence = MatrixEvidenceBundle {
        version: 2,
        generated_at_unix_ms,
        workspace_root: workspace_root.display().to_string(),
        success: false,
        exit_code: EXIT_CODEGEN,
        perf_runs,
        perf_baseline_path: perf_baseline_path.display().to_string(),
        perf_gate_path: perf_gate_path.map(|s| s.to_string()),
        kpi_thresholds: kpi_thresholds.any_set().then_some(*kpi_thresholds),
        perf_summary: None,
        check_lane_kpis: None,
        steps: Vec::new(),
    };

    let mut final_exit = EXIT_OK;
    for (index, step) in steps.into_iter().enumerate() {
        let result = run_matrix_step(index + 1, workspace_root, &artifact_dir, step);
        let exit_code = result.exit_code;
        let success = result.success;
        evidence.steps.push(result);
        if !success {
            final_exit = if exit_code == EXIT_OK {
                EXIT_CODEGEN
            } else {
                exit_code
            };
            break;
        }
    }

    evidence.success = final_exit == EXIT_OK;
    evidence.exit_code = final_exit;
    evidence.perf_summary = command_handlers::load_perf_baseline_summary(&perf_baseline_path).ok();
    evidence.check_lane_kpis = evidence
        .perf_summary
        .as_ref()
        .map(check_lane_kpis_from_summary);
    if let Err(err) = write_matrix_bundle(&bundle_path, &latest_path, &evidence) {
        eprintln!("matrix error: failed to write evidence bundle: {err}");
        return EXIT_CODEGEN;
    }
    println!(
        "matrix evidence: {}",
        latest_path.canonicalize().unwrap_or(latest_path).display()
    );

    final_exit
}

fn run_matrix_step(
    index: usize,
    workspace_root: &Path,
    artifact_dir: &Path,
    step: MatrixStepSpec<'_>,
) -> MatrixStepEvidence {
    println!("matrix: {}", step.name);
    let started_at_unix_ms = now_unix_ms();
    let started = Instant::now();
    let mut command = Command::new(step.program);
    command.current_dir(workspace_root).args(&step.args);
    let output = command.output();
    let duration_ms = started.elapsed().as_millis();
    let stdout_log = artifact_dir.join(format!("{index:02}-{}.stdout.log", step.name));
    let stderr_log = artifact_dir.join(format!("{index:02}-{}.stderr.log", step.name));
    let mut exit_code = EXIT_CODEGEN;
    let mut success = false;

    match output {
        Ok(output) => {
            let _ = fs::write(&stdout_log, &output.stdout);
            let _ = fs::write(&stderr_log, &output.stderr);
            exit_code = output.status.code().unwrap_or(EXIT_CODEGEN);
            success = output.status.success();
        }
        Err(err) => {
            let msg = format!("failed to execute {}: {err}\n", step.program.display());
            let _ = fs::write(&stderr_log, msg);
            let _ = fs::write(&stdout_log, []);
        }
    }

    MatrixStepEvidence {
        name: step.name.to_string(),
        command: std::iter::once(step.program.display().to_string())
            .chain(step.args)
            .collect(),
        cwd: workspace_root.display().to_string(),
        started_at_unix_ms,
        duration_ms,
        exit_code,
        success,
        stdout_log: stdout_log.display().to_string(),
        stderr_log: stderr_log.display().to_string(),
    }
}

fn write_matrix_bundle(
    bundle_path: &Path,
    latest_path: &Path,
    evidence: &MatrixEvidenceBundle,
) -> Result<(), String> {
    let payload = serde_json::to_vec_pretty(evidence).map_err(|err| err.to_string())?;
    fs::write(bundle_path, &payload).map_err(|err| err.to_string())?;
    fs::write(latest_path, payload).map_err(|err| err.to_string())?;
    Ok(())
}

fn check_lane_kpis_from_summary(summary: &command_handlers::PerfSummary) -> CheckLaneKpis {
    let typed = summary.metrics.abi_typed_lane;
    let boxed = summary.metrics.abi_boxed_lane;
    let total = typed + boxed;
    let typed_lane_ratio = if total == 0 {
        1.0
    } else {
        typed as f64 / total as f64
    };
    CheckLaneKpis {
        typed_lane_total: typed,
        boxed_lane_total: boxed,
        typed_lane_ratio,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("workspace root")
            .to_path_buf()
    }

    #[test]
    fn sanitize_git_ref_for_filename_replaces_unsafe_chars() {
        assert_eq!(
            sanitize_git_ref_for_filename("feature/perf+gate@2026"),
            "feature_perf_gate_2026".to_string()
        );
        assert_eq!(sanitize_git_ref_for_filename(""), "unknown".to_string());
    }

    #[test]
    fn classify_perfcmp_verdict_respects_effect_threshold() {
        assert_eq!(classify_perfcmp_verdict(3.5, 8.0, 2.0), "win");
        assert_eq!(classify_perfcmp_verdict(-8.0, -3.1, 2.0), "regression");
        assert_eq!(classify_perfcmp_verdict(-1.0, 1.2, 2.0), "no_signal");
    }

    #[test]
    fn fnv1a64_is_deterministic() {
        let first = fnv1a64(b"wrela-perfcmp");
        let second = fnv1a64(b"wrela-perfcmp");
        let different = fnv1a64(b"wrela-perfcmp-2");
        assert_eq!(first, second);
        assert_ne!(first, different);
    }

    #[test]
    fn coefficient_of_variation_handles_small_and_stable_sets() {
        assert_eq!(coefficient_of_variation(&[]), 0.0);
        assert_eq!(coefficient_of_variation(&[42.0]), 0.0);
        let cv = coefficient_of_variation(&[100.0, 100.0, 100.0, 100.0]);
        assert!(cv <= f64::EPSILON, "expected near-zero cv, got {cv}");
    }

    #[test]
    fn bootstrap_ci_percentile_is_seed_deterministic() {
        let values = [1.0, 2.0, 3.0, 4.0, 5.0];
        let mut seed_a = 7u64;
        let mut seed_b = 7u64;
        let ci_a = bootstrap_ci_percentile(&values, 95.0, 128, &mut seed_a);
        let ci_b = bootstrap_ci_percentile(&values, 95.0, 128, &mut seed_b);
        assert_eq!(ci_a, ci_b);
    }

    #[cfg(unix)]
    #[test]
    fn run_command_with_timeout_aborts_long_running_process() {
        let mut command = Command::new("sh");
        command.arg("-c").arg("sleep 5 & wait");
        let started = Instant::now();
        let err = run_command_with_timeout(&mut command, Duration::from_millis(100))
            .expect_err("long-running command should time out");
        assert!(err.contains("timed out"), "{err}");
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "timeout returned too late: {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn real_realtime_presentation_closure_manifest_matches_expected_protocol() {
        let bench_root = workspace_root()
            .join("benchmarks")
            .join("realtime_presentation");
        let manifest_path = bench_root.join("1080p120_closure.toml");
        let raw_manifest = fs::read_to_string(&manifest_path).expect("read closure manifest");
        let manifest_toml: toml::Value = toml::from_str(&raw_manifest).expect("parse closure toml");
        let manifest = load_benchmark_manifest(&manifest_path).expect("load closure manifest");
        assert_eq!(manifest.suite, "realtime_presentation");
        assert_eq!(
            manifest_toml
                .get("profiles")
                .and_then(|value| value.get("closure_1080p120"))
                .and_then(|value| value.get("warmup_pairs"))
                .and_then(|value| value.as_integer()),
            Some(4)
        );
        assert_eq!(
            manifest_toml
                .get("profiles")
                .and_then(|value| value.get("closure_1080p120"))
                .and_then(|value| value.get("measure_pairs"))
                .and_then(|value| value.as_integer()),
            Some(12)
        );
        assert_eq!(
            manifest_toml
                .get("profiles")
                .and_then(|value| value.get("closure_1080p120"))
                .and_then(|value| value.get("coverage"))
                .and_then(|value| value.as_str()),
            Some("all")
        );
        assert_eq!(
            manifest_toml
                .get("profiles")
                .and_then(|value| value.get("closure_1080p120"))
                .and_then(|value| value.get("execution_story"))
                .and_then(|value| value.as_str()),
            Some("wgsl_resident")
        );
        assert_eq!(
            manifest_toml
                .get("profiles")
                .and_then(|value| value.get("closure_1080p120"))
                .and_then(|value| value.get("adapter_name"))
                .and_then(|value| value.as_str()),
            Some("wgsl_resident")
        );
        assert_eq!(
            manifest_toml
                .get("profiles")
                .and_then(|value| value.get("closure_1080p120"))
                .and_then(|value| value.get("warmup_protocol"))
                .and_then(|value| value.as_str()),
            Some("pipeline_and_resident_scene_upload")
        );
        assert_eq!(
            manifest_toml
                .get("profiles")
                .and_then(|value| value.get("closure_1080p120"))
                .and_then(|value| value.get("companion_profile"))
                .and_then(|value| value.as_str()),
            Some("canonical_1080p120_cpu_oracle")
        );

        let scenarios = manifest.scenarios_for_profile(PerfProfile::Closure1080p120);
        let scenario_ids = scenarios
            .iter()
            .map(|scenario| scenario.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            scenario_ids,
            vec![
                "closure_1080p120_dense_constructive",
                "closure_1080p120_repetition_heavy",
                "closure_1080p120_thin_stack_grazing",
                "closure_1080p120_media_radiance",
                "closure_1080p120_transformed_primitive_gallery",
                "closure_1080p120_mixed_opaque_conservative",
                "closure_1080p120_cache_stress_motion_path",
                "closure_1080p120_camera_motion_temporal_reuse_clipmap_churn",
            ]
        );
        assert!(scenarios.iter().all(|scenario| scenario.class == "closure"));
        assert!(
            scenarios
                .iter()
                .all(|scenario| scenario.presentation.is_some())
        );
        assert!(scenarios.iter().all(|scenario| {
            let presentation = scenario.presentation.as_ref().expect("presentation spec");
            presentation.width == Some(1920)
                && presentation.height == Some(1080)
                && presentation.frames == Some(2)
        }));

        let selection = build_benchmark_selection(
            &TestTarget::ProjectRoot(bench_root),
            &manifest_path,
            PerfProfile::Closure1080p120,
        )
        .expect("build closure benchmark selection");
        assert_eq!(selection.len(), scenario_ids.len());
    }

    #[test]
    fn real_field_engine_closure_manifest_matches_expected_protocol() {
        let bench_root = workspace_root().join("benchmarks").join("field_engine");
        let manifest_path = bench_root.join("1080p120_closure.toml");
        let raw_manifest = fs::read_to_string(&manifest_path).expect("read closure manifest");
        let manifest_toml: toml::Value = toml::from_str(&raw_manifest).expect("parse closure toml");
        let manifest = load_benchmark_manifest(&manifest_path).expect("load closure manifest");
        assert_eq!(manifest.suite, "field_engine");
        assert_eq!(
            manifest_toml
                .get("profiles")
                .and_then(|value| value.get("closure_1080p120"))
                .and_then(|value| value.get("warmup_pairs"))
                .and_then(|value| value.as_integer()),
            Some(4)
        );
        assert_eq!(
            manifest_toml
                .get("profiles")
                .and_then(|value| value.get("closure_1080p120"))
                .and_then(|value| value.get("measure_pairs"))
                .and_then(|value| value.as_integer()),
            Some(12)
        );
        assert_eq!(
            manifest_toml
                .get("profiles")
                .and_then(|value| value.get("closure_1080p120"))
                .and_then(|value| value.get("coverage"))
                .and_then(|value| value.as_str()),
            Some("all")
        );
        assert_eq!(
            manifest_toml
                .get("profiles")
                .and_then(|value| value.get("closure_1080p120"))
                .and_then(|value| value.get("execution_story"))
                .and_then(|value| value.as_str()),
            Some("cpu_oracle")
        );
        assert_eq!(
            manifest_toml
                .get("profiles")
                .and_then(|value| value.get("closure_1080p120"))
                .and_then(|value| value.get("adapter_name"))
                .and_then(|value| value.as_str()),
            Some("cpu_oracle")
        );
        assert_eq!(
            manifest_toml
                .get("profiles")
                .and_then(|value| value.get("closure_1080p120"))
                .and_then(|value| value.get("warmup_protocol"))
                .and_then(|value| value.as_str()),
            Some("cpu_oracle_baseline_warmup")
        );
        assert_eq!(
            manifest_toml
                .get("profiles")
                .and_then(|value| value.get("closure_1080p120"))
                .and_then(|value| value.get("companion_profile"))
                .and_then(|value| value.as_str()),
            Some("canonical_1080p120_wgsl_resident")
        );

        let scenarios = manifest.scenarios_for_profile(PerfProfile::Closure1080p120);
        let scenario_ids = scenarios
            .iter()
            .map(|scenario| scenario.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            scenario_ids,
            vec![
                "closure_1080p120_repetition_identity_stability",
                "closure_1080p120_mixed_solver_dense_oracle",
                "closure_1080p120_collision_heavy_transition",
                "closure_1080p120_transformed_primitive_gallery",
                "closure_1080p120_mixed_opaque_conservative",
            ]
        );
        assert!(scenarios.iter().all(|scenario| scenario.class == "closure"));
        assert!(
            scenarios
                .iter()
                .all(|scenario| scenario.presentation.is_none())
        );

        let selection = build_benchmark_selection(
            &TestTarget::ProjectRoot(bench_root),
            &manifest_path,
            PerfProfile::Closure1080p120,
        )
        .expect("build closure benchmark selection");
        assert_eq!(selection.len(), scenario_ids.len());
    }

    #[test]
    fn real_collision_perf_manifest_matches_expected_protocol() {
        let bench_root = workspace_root().join("benchmarks").join("collision_perf");
        let manifest_path = bench_root.join("bench.toml");
        let raw_manifest = fs::read_to_string(&manifest_path).expect("read collision manifest");
        let manifest_toml: toml::Value =
            toml::from_str(&raw_manifest).expect("parse collision toml");
        let manifest = load_benchmark_manifest(&manifest_path).expect("load collision manifest");
        assert_eq!(manifest.suite, "collision_perf");
        assert_eq!(
            manifest_toml
                .get("profiles")
                .and_then(|value| value.get("standard"))
                .and_then(|value| value.get("warmup_pairs"))
                .and_then(|value| value.as_integer()),
            Some(3)
        );
        assert_eq!(
            manifest_toml
                .get("profiles")
                .and_then(|value| value.get("standard"))
                .and_then(|value| value.get("measure_pairs"))
                .and_then(|value| value.as_integer()),
            Some(10)
        );
        let scenarios = manifest.scenarios_for_profile(PerfProfile::Standard);
        let scenario_ids = scenarios
            .iter()
            .map(|scenario| scenario.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            scenario_ids,
            vec![
                "collision_perf_point_occupancy_burst",
                "collision_perf_dense_ray_casts",
                "collision_perf_overlap_burst",
                "collision_perf_repeated_sweeps",
                "collision_perf_toi_transition_reuse",
            ]
        );
        assert!(
            scenarios
                .iter()
                .all(|scenario| scenario.class == "critical")
        );
        let selection = build_benchmark_selection(
            &TestTarget::ProjectRoot(bench_root),
            &manifest_path,
            PerfProfile::Standard,
        )
        .expect("build collision benchmark selection");
        assert_eq!(selection.len(), scenario_ids.len());
    }

    #[test]
    fn real_collision_perf_closure_manifest_matches_expected_protocol() {
        let bench_root = workspace_root().join("benchmarks").join("collision_perf");
        let manifest_path = bench_root.join("1080p120_closure.toml");
        let raw_manifest = fs::read_to_string(&manifest_path).expect("read closure manifest");
        let manifest_toml: toml::Value = toml::from_str(&raw_manifest).expect("parse closure toml");
        let manifest = load_benchmark_manifest(&manifest_path).expect("load closure manifest");
        assert_eq!(manifest.suite, "collision_perf");
        assert_eq!(
            manifest_toml
                .get("profiles")
                .and_then(|value| value.get("closure_1080p120"))
                .and_then(|value| value.get("warmup_pairs"))
                .and_then(|value| value.as_integer()),
            Some(4)
        );
        assert_eq!(
            manifest_toml
                .get("profiles")
                .and_then(|value| value.get("closure_1080p120"))
                .and_then(|value| value.get("measure_pairs"))
                .and_then(|value| value.as_integer()),
            Some(12)
        );
        let scenarios = manifest.scenarios_for_profile(PerfProfile::Closure1080p120);
        let scenario_ids = scenarios
            .iter()
            .map(|scenario| scenario.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            scenario_ids,
            vec![
                "closure_1080p120_point_occupancy_burst",
                "closure_1080p120_dense_ray_casts",
                "closure_1080p120_overlap_burst",
                "closure_1080p120_repeated_sweeps",
                "closure_1080p120_toi_transition_reuse",
            ]
        );
        assert!(scenarios.iter().all(|scenario| scenario.class == "closure"));
        let selection = build_benchmark_selection(
            &TestTarget::ProjectRoot(bench_root),
            &manifest_path,
            PerfProfile::Closure1080p120,
        )
        .expect("build closure benchmark selection");
        assert_eq!(selection.len(), scenario_ids.len());
    }

    #[test]
    fn frame_closure_status_records_report_collection_failures_as_violations() {
        let profile = PerfClosureProfile::canonical_1080p120();
        let report = build_frame_closure_status(
            &profile,
            &[],
            &["scenario `dense` timed out".to_string()],
            0,
            1,
        );
        assert_eq!(report.status, PerfClosureLaneStatus::Violated);
        assert!(
            report
                .notes
                .iter()
                .any(|note| note.contains("presentation report collection failed"))
        );
    }

    #[test]
    fn frame_closure_status_rejects_backend_mismatch_for_wgsl_resident_profile() {
        let mut profile = PerfClosureProfile::canonical_1080p120();
        profile.output_width = 64;
        profile.output_height = 64;
        profile.warmup_runs = 0;
        profile.measured_runs = 1;
        profile.frame_budget.median_ms = 100.0;
        profile.frame_budget.p95_ms = 100.0;
        profile.primary_visibility_budget.median_ms = 100.0;
        profile.primary_visibility_budget.p95_ms = 100.0;

        let report = build_frame_closure_status(
            &profile,
            &[PresentationBenchmarkReport {
                scenario_id: "closure_fixture".to_string(),
                test_name: "tests/fixture".to_string(),
                view: "view".to_string(),
                region: "region".to_string(),
                domain: "domain".to_string(),
                backend: "cpu".to_string(),
                observed_adapter_name: None,
                query_trace_solver_mode: "hybrid".to_string(),
                selected_workgroup_size: 0,
                frames_executed: 1,
                frame_time_ns: 1_000_000,
                field_samples: 128,
                quality_tier: "realtime_120".to_string(),
                target_fps: 120,
                internal_resolution_scale: 1.0,
                reconstructed_output: false,
                quality_history: vec!["realtime_120".to_string()],
                internal_resolution_history: vec![1.0],
                bottleneck_pass: Some("primary_visibility".to_string()),
                active_acceleration_artifacts: vec![],
                performance_gain_sources: vec!["backend_speed".to_string()],
                frame_cost: {
                    let mut frame_cost =
                        sample_presentation_frame_cost(64, 64, 1.0, 128, 2.0, 16, 8, 1_000);
                    frame_cost.quality.tier = "realtime_120".to_string();
                    frame_cost.quality.target_fps = 120;
                    frame_cost
                },
                frame_cost_history: vec![],
                wgsl_workgroup_comparison: None,
                ab_comparison: None,
            }],
            &[],
            0,
            1,
        );

        assert_eq!(report.status, PerfClosureLaneStatus::Violated);
        assert!(
            report
                .notes
                .iter()
                .any(|note| note.contains("presentation backends observed: cpu"))
        );
        assert!(report.notes.iter().any(|note| {
            note.contains("reported backend 'cpu'") && note.contains("closure backend 'wgsl'")
        }));
    }

    #[test]
    fn closure_profile_prefers_observed_wgsl_runtime_metadata() {
        let mut profile = PerfClosureProfile::canonical_1080p120();
        let mut frame_cost = sample_presentation_frame_cost(64, 64, 1.0, 128, 2.0, 16, 8, 1_000);
        frame_cost.gpu_runtime.timestamps_supported = true;
        frame_cost.gpu_runtime.requested_limits_profile =
            "storage_buffers_per_stage=10 storage_binding_bytes=268435456 bind_groups=4 workgroup_x=128"
                .to_string();
        frame_cost.gpu_runtime.enabled_optional_features =
            vec!["timestamp_query".to_string(), "shader_f16".to_string()];

        apply_observed_wgsl_runtime_metadata(
            &mut profile,
            &[PresentationBenchmarkReport {
                scenario_id: "closure_fixture".to_string(),
                test_name: "tests/fixture".to_string(),
                view: "view".to_string(),
                region: "region".to_string(),
                domain: "domain".to_string(),
                backend: "wgsl".to_string(),
                observed_adapter_name: Some("Test Adapter".to_string()),
                query_trace_solver_mode: "hybrid".to_string(),
                selected_workgroup_size: 64,
                frames_executed: 1,
                frame_time_ns: 1_000_000,
                field_samples: 128,
                quality_tier: "realtime_120".to_string(),
                target_fps: 120,
                internal_resolution_scale: 1.0,
                reconstructed_output: false,
                quality_history: vec!["realtime_120".to_string()],
                internal_resolution_history: vec![1.0],
                bottleneck_pass: Some("primary_visibility".to_string()),
                active_acceleration_artifacts: vec![],
                performance_gain_sources: vec![],
                frame_cost,
                frame_cost_history: vec![],
                wgsl_workgroup_comparison: None,
                ab_comparison: None,
            }],
        );

        assert_eq!(profile.adapter_name, "Test Adapter");
        assert_eq!(
            profile.requested_limits_profile,
            "storage_buffers_per_stage=10 storage_binding_bytes=268435456 bind_groups=4 workgroup_x=128"
        );
        assert_eq!(
            profile.enabled_optional_features,
            vec!["shader_f16".to_string(), "timestamp_query".to_string()]
        );
        assert!(profile.timestamps_enabled);
        assert!(profile.f16_enabled);
        assert!(!profile.indirect_dispatch_enabled);
    }

    #[test]
    fn closure_verdict_surface_names_the_top_bottleneck_and_subsystem_findings() {
        let profile = PerfClosureProfile::canonical_1080p120();
        let mut frame_status = PerfClosureLaneStatusReport::unsampled(&profile.frame);
        frame_status.status = PerfClosureLaneStatus::Violated;
        frame_status.total_frame_median_ms = Some(11.0);
        frame_status.primary_visibility_median_ms = Some(4.0);
        frame_status.dominant_bottleneck_pass = Some("postprocess_shading".to_string());

        let mut collision_status = PerfClosureLaneStatusReport::unsampled(&profile.collision);
        collision_status.status = PerfClosureLaneStatus::Violated;
        collision_status.collision_runtime_regression_pct = Some(6.5);

        let mut frame_cost =
            sample_presentation_frame_cost(64, 64, 1.0, 100, 16.0, 100, 98, 11_000);
        frame_cost.execution_policy =
            "required=best-effort selected=heuristic-solver backend=wgsl".to_string();
        frame_cost.primary_hit_rate = 0.62;
        frame_cost.average_trace_steps = 16.0;
        frame_cost.support_prune_effectiveness = 0.02;
        frame_cost.candidate_count_before_pruning = 100;
        frame_cost.candidate_count_after_pruning = 98;
        frame_cost.active_acceleration_artifacts = vec![];
        frame_cost.performance_gain_sources = vec![];
        frame_cost.cache_brick_visits = 0;
        frame_cost.cache_brick_hits = 0;
        frame_cost.cache_brick_misses = 0;
        frame_cost.cache_interval_advances = 0;
        frame_cost.bottleneck_pass = Some("postprocess_shading".to_string());
        frame_cost.passes = vec![
            wrela::presentation_exec::PresentationPassCost {
                pass_id: "primary.visibility".to_string(),
                pass_kind: "primary_visibility".to_string(),
                work_items: 1024,
                elapsed_micros: 4_000,
                gpu_elapsed_micros: None,
                dispatch_count: 1,
                attachment_bytes_read: 0,
                attachment_bytes_written: 4_096,
                notes: vec![],
            },
            wrela::presentation_exec::PresentationPassCost {
                pass_id: "postprocess.shading".to_string(),
                pass_kind: "postprocess".to_string(),
                work_items: 1024,
                elapsed_micros: 7_000,
                gpu_elapsed_micros: None,
                dispatch_count: 1,
                attachment_bytes_read: 4_096,
                attachment_bytes_written: 4_096,
                notes: vec![],
            },
        ];

        let presentation_report = PresentationBenchmarkReport {
            scenario_id: "closure_fixture".to_string(),
            test_name: "tests/closure_fixture::test_ops_1".to_string(),
            view: "view".to_string(),
            region: "region".to_string(),
            domain: "domain".to_string(),
            backend: "wgsl".to_string(),
            observed_adapter_name: None,
            query_trace_solver_mode: "hybrid".to_string(),
            selected_workgroup_size: 64,
            frames_executed: 1,
            frame_time_ns: 11_000_000,
            field_samples: 100,
            quality_tier: "realtime_120".to_string(),
            target_fps: 120,
            internal_resolution_scale: 1.0,
            reconstructed_output: false,
            quality_history: vec!["realtime_120".to_string()],
            internal_resolution_history: vec![1.0],
            bottleneck_pass: Some("postprocess_shading".to_string()),
            active_acceleration_artifacts: vec![],
            performance_gain_sources: vec![],
            frame_cost,
            frame_cost_history: vec![],
            wgsl_workgroup_comparison: None,
            ab_comparison: None,
        };

        let collision_report = CollisionBenchmarkReport {
            suite: "collision_perf".to_string(),
            backend: "cpu".to_string(),
            command: "collision-suite".to_string(),
            query_count_total: 4,
            total_runtime_ns: 10_000,
            queries_per_sec: 400.0,
            average_candidate_count: 32.0,
            average_rejected_candidate_count: 28.0,
            average_pruned_node_count: 4.0,
            average_interval_subdivisions: 2.0,
            average_interval_refinements: 1.0,
            average_certificate_successes: 0.0,
            witness_reuse_rate: 0.20,
            fallback_rate: 0.50,
            available_count_total: 2,
            consumed_count_total: 0,
            rejected_count_total: 1,
            unavailable_count_total: 1,
            executions: vec![],
        };

        let verdict = build_closure_verdict(
            &profile,
            &frame_status,
            &collision_status,
            &[presentation_report],
            &[collision_report],
        );
        assert_eq!(verdict.status, PerfClosureVerdictStatus::Failed);
        assert_eq!(
            verdict.top_remaining_bottleneck.as_deref(),
            Some("postprocess_shading")
        );
        assert!(verdict
            .findings
            .iter()
            .any(|finding| finding.subsystem == "presentation" && finding.focus == "dense_rays"));
        assert!(verdict.findings.iter().any(
            |finding| finding.subsystem == "presentation" && finding.focus == "pruning_failure"
        ));
        assert!(
            verdict
                .findings
                .iter()
                .any(|finding| finding.subsystem == "acceleration"
                    && finding.focus == "caches_unavailable_or_invalid")
        );
        assert!(
            verdict
                .findings
                .iter()
                .any(|finding| finding.subsystem == "acceleration"
                    && finding.focus == "wgsl_linear_traversal")
        );
        assert!(
            verdict
                .findings
                .iter()
                .any(|finding| finding.subsystem == "presentation"
                    && finding.focus == "visibility_vs_shading_bound")
        );
        assert!(
            verdict
                .findings
                .iter()
                .any(|finding| finding.subsystem == "collision"
                    && finding.focus == "witness_reuse_invalid_or_unsupported")
        );
    }

    #[test]
    fn checked_in_collision_baseline_fixture_loads() {
        let summary = load_collision_baseline_summary("collision_perf.phase40_cpu_oracle")
            .expect("load checked-in collision baseline");
        assert!(summary.runtime_p50_ns > 0);
        assert!(summary.runtime_p95_ns >= summary.runtime_p50_ns);
        assert!(summary.runtime_p99_ns >= summary.runtime_p95_ns);
    }

    #[test]
    fn presentation_debug_args_default_dimensions_to_64() {
        let spec = command_handlers::BenchmarkPresentationSpec {
            view: "bench_view".to_string(),
            region: "bench_region".to_string(),
            entry: Some("tests/bench_fixture.wr".to_string()),
            domain: Some("bench_domain".to_string()),
            width: None,
            height: None,
            frames: Some(4),
            camera_position: [0.0, 1.0, 2.0],
            camera_forward: [0.0, 0.0, -1.0],
            camera_up: [0.0, 1.0, 0.0],
            vertical_fov_degrees: 48.0,
        };
        let args = presentation_debug_args(&spec, QueryTraceSolverMode::Hybrid);
        assert!(
            args.windows(2)
                .any(|pair| pair[0] == "--width" && pair[1] == "64")
        );
        assert!(
            args.windows(2)
                .any(|pair| pair[0] == "--height" && pair[1] == "64")
        );
        assert!(
            args.windows(2)
                .any(|pair| pair[0] == "--frames" && pair[1] == "4")
        );
        assert!(
            args.windows(2)
                .any(|pair| pair[0] == "--view" && pair[1] == "bench_view")
        );
        assert!(
            args.windows(2)
                .any(|pair| pair[0] == "--region" && pair[1] == "bench_region")
        );
        assert!(
            args.windows(2)
                .any(|pair| pair[0] == "--domain" && pair[1] == "bench_domain")
        );
        assert!(
            args.windows(2)
                .any(|pair| pair[0] == "--solver-mode" && pair[1] == "hybrid")
        );
    }

    #[test]
    fn presentation_debug_command_uses_wgsl_backend_and_shared_workgroup_override() {
        let spec = command_handlers::BenchmarkPresentationSpec {
            view: "bench_view".to_string(),
            region: "bench_region".to_string(),
            entry: Some("tests/bench_fixture.wr".to_string()),
            domain: Some("bench_domain".to_string()),
            width: Some(96),
            height: Some(54),
            frames: Some(2),
            camera_position: [0.0, 1.0, 2.0],
            camera_forward: [0.0, 0.0, -1.0],
            camera_up: [0.0, 1.0, 0.0],
            vertical_fov_degrees: 48.0,
        };
        let command = build_presentation_debug_command(
            Path::new("/tmp/wrela"),
            Path::new("/tmp/bench_fixture.wr"),
            &spec,
            QueryTraceSolverMode::Hybrid,
            Some(64),
            wrela::query_plan::DispatchBackend::Cpu,
        );
        let args = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(args.iter().any(|arg| arg == "--query-backend=cpu"));
        assert!(args.iter().any(|arg| arg == "presentation-debug"));
        let envs = command
            .get_envs()
            .filter_map(|(key, value)| {
                value.map(|value| {
                    (
                        key.to_string_lossy().into_owned(),
                        value.to_string_lossy().into_owned(),
                    )
                })
            })
            .collect::<HashMap<_, _>>();
        assert_eq!(
            envs.get(WGSL_WORKGROUP_SIZE_OVERRIDE_ENV)
                .map(String::as_str),
            Some("64")
        );
    }

    fn sample_presentation_frame_cost(
        internal_width: u32,
        internal_height: u32,
        internal_resolution_scale: f32,
        field_samples: u32,
        average_trace_steps: f32,
        candidate_count_before_pruning: u32,
        candidate_count_after_pruning: u32,
        elapsed_micros: u128,
    ) -> wrela::presentation_exec::PresentationFrameCostReport {
        wrela::presentation_exec::PresentationFrameCostReport {
            semantic_domain: "bench_domain".to_string(),
            execution_policy: "required=best-effort selected=heuristic-solver backend=cpu"
                .to_string(),
            legal_degradations: vec![],
            output_width: 64,
            output_height: 64,
            internal_width,
            internal_height,
            quality: wrela::presentation_exec::PresentationQualityReport {
                tier: "realtime_60".to_string(),
                target_fps: 60,
                output_width: 64,
                output_height: 64,
                internal_width,
                internal_height,
                internal_resolution_scale,
                achieved_native_output: internal_width == 64 && internal_height == 64,
                reconstructed_output: internal_width != 64 || internal_height != 64,
                temporal_mode: "TemporalAA".to_string(),
                radiance_mode: "full".to_string(),
                media_enabled: true,
                half_res_participants: false,
                hit_compaction_enabled: internal_resolution_scale < 1.0,
                active_degradations: Vec::new(),
            },
            primary_hit_rate: 0.75,
            average_trace_steps,
            max_trace_steps: 24,
            candidate_count_before_pruning,
            candidate_count_after_pruning,
            support_prune_effectiveness: 0.4,
            tile_cull_total_tiles: 16,
            tile_cull_active_tiles: 9,
            tile_cull_efficiency: 0.4375,
            tile_candidate_total_samples: 256,
            tile_candidate_active_samples: 128,
            tile_candidate_reduction: 128,
            packet_scheduling_active: true,
            selected_workgroup_size: 64,
            surface_resolve_count: 256,
            participant_resolve_count: 128,
            history_reuse_rate: 0.25,
            continuation_diagnostics: vec![],
            acceleration_node_visits: 0,
            union_cluster_visits: 0,
            ray_support_interval_rejections: 0,
            ray_support_entry_jumps: 0,
            repeat_cell_skips: 0,
            cache_brick_visits: 0,
            cache_brick_hits: 0,
            cache_brick_misses: 0,
            cache_interval_advances: 0,
            accepted_relaxed_steps: 0,
            rejected_relaxed_steps: 0,
            analytic_transformed_hits: 0,
            interval_subdivisions: 0,
            interval_proof_successes: 0,
            observer_continuation_seed_hits: 0,
            solver_relaxed_attempts: 0,
            solver_relaxed_no_root_advances: 0,
            solver_relaxed_brackets: 0,
            solver_relaxed_unresolved: 0,
            solver_interval_attempts: 0,
            solver_interval_no_root_advances: 0,
            solver_interval_brackets: 0,
            solver_interval_unresolved: 0,
            solver_refinement_attempts: 0,
            solver_refinement_failures: 0,
            solver_repeat_attempts: 0,
            solver_repeat_supported: 0,
            solver_repeat_inapplicable: 0,
            solver_repeat_unsupported: 0,
            solver_repeat_unsupported_form: 0,
            solver_repeat_unsupported_bounds: 0,
            solver_repeat_cells_enumerated: 0,
            field_samples,
            cpu_time_total_micros: 0,
            execution_bound: "cpu_wall_clock_only".to_string(),
            gpu_runtime: Default::default(),
            attachment_bytes: vec![],
            passes: vec![wrela::presentation_exec::PresentationPassCost {
                pass_id: "primary.visibility".to_string(),
                pass_kind: "primary_visibility".to_string(),
                work_items: 1024,
                elapsed_micros,
                gpu_elapsed_micros: None,
                dispatch_count: 1,
                attachment_bytes_read: 0,
                attachment_bytes_written: 8192,
                notes: vec![],
            }],
            active_acceleration_artifacts: vec![],
            bottleneck_pass: Some("primary_visibility".to_string()),
            performance_gain_sources: vec!["backend_speed".to_string()],
        }
    }

    #[test]
    fn closure_measured_presentation_frame_history_drops_cold_start_frame() {
        let cold = sample_presentation_frame_cost(64, 64, 1.0, 10, 2.0, 6, 4, 1_000);
        let warm = sample_presentation_frame_cost(32, 32, 0.5, 30, 4.0, 10, 7, 2_000);

        let effective = closure_measured_presentation_frame_history(&warm, &[cold, warm.clone()]);

        assert_eq!(effective, vec![warm]);
    }

    #[test]
    fn presentation_report_from_debug_output_carries_quality_and_pass_data() {
        let scenario = command_handlers::BenchmarkScenario {
            id: "presentation_fixture".to_string(),
            test_name: "tests/fixture::test_ops_64".to_string(),
            ops: 64,
            class: "critical".to_string(),
            min_runtime_ms: None,
            timeout_ms: None,
            allow_unstable: false,
            presentation: None,
            collision: None,
        };
        let dump = PresentationDebugCommandOutput {
            view: "bench_view".to_string(),
            region: "bench_region".to_string(),
            domain: "bench_domain".to_string(),
            backend: "cpu".to_string(),
            query_trace_solver_mode: "hybrid".to_string(),
            frames_executed: 2,
            frame_cost: wrela::presentation_exec::PresentationFrameCostReport {
                semantic_domain: "bench_domain".to_string(),
                execution_policy: "required=best-effort selected=heuristic-solver backend=cpu"
                    .to_string(),
                legal_degradations: vec!["reduce_internal_resolution".to_string()],
                output_width: 64,
                output_height: 64,
                internal_width: 32,
                internal_height: 32,
                field_samples: 512,
                cpu_time_total_micros: 0,
                execution_bound: "cpu_wall_clock_only".to_string(),
                gpu_runtime: Default::default(),
                quality: wrela::presentation_exec::PresentationQualityReport {
                    tier: "realtime_60".to_string(),
                    target_fps: 60,
                    output_width: 64,
                    output_height: 64,
                    internal_width: 32,
                    internal_height: 32,
                    internal_resolution_scale: 0.5,
                    achieved_native_output: false,
                    reconstructed_output: true,
                    temporal_mode: "TemporalAA".to_string(),
                    radiance_mode: "full".to_string(),
                    media_enabled: true,
                    half_res_participants: false,
                    hit_compaction_enabled: true,
                    active_degradations: vec!["reduce_internal_resolution".to_string()],
                },
                primary_hit_rate: 0.75,
                average_trace_steps: 12.0,
                max_trace_steps: 24,
                candidate_count_before_pruning: 100,
                candidate_count_after_pruning: 60,
                support_prune_effectiveness: 0.4,
                tile_cull_total_tiles: 16,
                tile_cull_active_tiles: 9,
                tile_cull_efficiency: 0.4375,
                tile_candidate_total_samples: 256,
                tile_candidate_active_samples: 128,
                tile_candidate_reduction: 128,
                packet_scheduling_active: true,
                selected_workgroup_size: 64,
                surface_resolve_count: 256,
                participant_resolve_count: 128,
                history_reuse_rate: 0.5,
                continuation_diagnostics: vec![
                    "continuation verdict=available reason=none change_class=stable accepted_change_class=camera-motion"
                        .to_string()
                ],
                acceleration_node_visits: 0,
                union_cluster_visits: 0,
                ray_support_interval_rejections: 0,
                ray_support_entry_jumps: 0,
                repeat_cell_skips: 0,
                cache_brick_visits: 0,
                cache_brick_hits: 0,
                cache_brick_misses: 0,
                cache_interval_advances: 0,
                accepted_relaxed_steps: 0,
                rejected_relaxed_steps: 0,
                analytic_transformed_hits: 0,
                interval_subdivisions: 0,
                interval_proof_successes: 0,
                observer_continuation_seed_hits: 0,
                solver_relaxed_attempts: 0,
                solver_relaxed_no_root_advances: 0,
                solver_relaxed_brackets: 0,
                solver_relaxed_unresolved: 0,
                solver_interval_attempts: 0,
                solver_interval_no_root_advances: 0,
                solver_interval_brackets: 0,
                solver_interval_unresolved: 0,
                solver_refinement_attempts: 0,
                solver_refinement_failures: 0,
                solver_repeat_attempts: 0,
                solver_repeat_supported: 0,
                solver_repeat_inapplicable: 0,
                solver_repeat_unsupported: 0,
                solver_repeat_unsupported_form: 0,
                solver_repeat_unsupported_bounds: 0,
                solver_repeat_cells_enumerated: 0,
                attachment_bytes: vec![wrela::presentation_exec::PresentationAttachmentBytes {
                    attachment: "color".to_string(),
                    width: 64,
                    height: 64,
                    total_size_bytes: 16384,
                }],
                passes: vec![wrela::presentation_exec::PresentationPassCost {
                    pass_id: "primary.visibility".to_string(),
                    pass_kind: "primary_visibility".to_string(),
                    work_items: 1024,
                    elapsed_micros: 3300,
                    gpu_elapsed_micros: None,
                    dispatch_count: 1,
                    attachment_bytes_read: 0,
                    attachment_bytes_written: 8192,
                    notes: vec!["dynamic_resolution".to_string()],
                }],
                active_acceleration_artifacts: vec![
                    "tile_candidate_table".to_string(),
                    "packet_scheduling".to_string(),
                ],
                bottleneck_pass: Some("primary_visibility".to_string()),
                performance_gain_sources: vec![
                    "support_pruning".to_string(),
                    "tile_culling".to_string(),
                    "tile_candidate_table".to_string(),
                    "packet_scheduling".to_string(),
                    "quality_degradation_active".to_string(),
                ],
            },
            frame_cost_history: vec![
                wrela::presentation_exec::PresentationFrameCostReport {
                    semantic_domain: "bench_domain".to_string(),
                    execution_policy: "required=best-effort selected=heuristic-solver backend=cpu"
                        .to_string(),
                    legal_degradations: vec![],
                    output_width: 64,
                    output_height: 64,
                    internal_width: 64,
                    internal_height: 64,
                    field_samples: 512,
                    cpu_time_total_micros: 0,
                    execution_bound: "cpu_wall_clock_only".to_string(),
                    gpu_runtime: Default::default(),
                    quality: wrela::presentation_exec::PresentationQualityReport {
                        tier: "realtime_60".to_string(),
                        target_fps: 60,
                        output_width: 64,
                        output_height: 64,
                        internal_width: 64,
                        internal_height: 64,
                        internal_resolution_scale: 1.0,
                        achieved_native_output: true,
                        reconstructed_output: false,
                        temporal_mode: "TemporalAA".to_string(),
                        radiance_mode: "full".to_string(),
                        media_enabled: true,
                        half_res_participants: false,
                        hit_compaction_enabled: false,
                        active_degradations: vec![],
                    },
                    primary_hit_rate: 0.8,
                    average_trace_steps: 14.0,
                    max_trace_steps: 24,
                    candidate_count_before_pruning: 100,
                    candidate_count_after_pruning: 60,
                    support_prune_effectiveness: 0.4,
                    tile_cull_total_tiles: 16,
                    tile_cull_active_tiles: 9,
                    tile_cull_efficiency: 0.4375,
                    tile_candidate_total_samples: 256,
                    tile_candidate_active_samples: 256,
                    tile_candidate_reduction: 0,
                    packet_scheduling_active: false,
                    selected_workgroup_size: 0,
                    surface_resolve_count: 256,
                    participant_resolve_count: 128,
                    history_reuse_rate: 0.0,
                    continuation_diagnostics: vec![],
                    acceleration_node_visits: 0,
                    union_cluster_visits: 0,
                    ray_support_interval_rejections: 0,
                    ray_support_entry_jumps: 0,
                    repeat_cell_skips: 0,
                    cache_brick_visits: 0,
                    cache_brick_hits: 0,
                    cache_brick_misses: 0,
                    cache_interval_advances: 0,
                    accepted_relaxed_steps: 0,
                    rejected_relaxed_steps: 0,
                    analytic_transformed_hits: 0,
                    interval_subdivisions: 0,
                    interval_proof_successes: 0,
                    observer_continuation_seed_hits: 0,
                    solver_relaxed_attempts: 0,
                    solver_relaxed_no_root_advances: 0,
                    solver_relaxed_brackets: 0,
                    solver_relaxed_unresolved: 0,
                    solver_interval_attempts: 0,
                    solver_interval_no_root_advances: 0,
                    solver_interval_brackets: 0,
                    solver_interval_unresolved: 0,
                    solver_refinement_attempts: 0,
                    solver_refinement_failures: 0,
                    solver_repeat_attempts: 0,
                    solver_repeat_supported: 0,
                    solver_repeat_inapplicable: 0,
                    solver_repeat_unsupported: 0,
                    solver_repeat_unsupported_form: 0,
                    solver_repeat_unsupported_bounds: 0,
                    solver_repeat_cells_enumerated: 0,
                    attachment_bytes: vec![],
                    passes: vec![wrela::presentation_exec::PresentationPassCost {
                        pass_id: "primary.visibility".to_string(),
                        pass_kind: "primary_visibility".to_string(),
                        work_items: 1024,
                        elapsed_micros: 1200,
                        gpu_elapsed_micros: None,
                        dispatch_count: 1,
                        attachment_bytes_read: 0,
                        attachment_bytes_written: 4096,
                        notes: vec![],
                    }],
                    active_acceleration_artifacts: vec![],
                    bottleneck_pass: Some("primary_visibility".to_string()),
                    performance_gain_sources: vec!["backend_speed".to_string()],
                },
                wrela::presentation_exec::PresentationFrameCostReport {
                    semantic_domain: "bench_domain".to_string(),
                    execution_policy: "required=best-effort selected=heuristic-solver backend=cpu"
                        .to_string(),
                    legal_degradations: vec!["reduce_internal_resolution".to_string()],
                    output_width: 64,
                    output_height: 64,
                    internal_width: 32,
                    internal_height: 32,
                    field_samples: 512,
                    cpu_time_total_micros: 0,
                    execution_bound: "cpu_wall_clock_only".to_string(),
                    gpu_runtime: Default::default(),
                    quality: wrela::presentation_exec::PresentationQualityReport {
                        tier: "realtime_60".to_string(),
                        target_fps: 60,
                        output_width: 64,
                        output_height: 64,
                        internal_width: 32,
                        internal_height: 32,
                        internal_resolution_scale: 0.5,
                        achieved_native_output: false,
                        reconstructed_output: true,
                        temporal_mode: "TemporalAA".to_string(),
                        radiance_mode: "full".to_string(),
                        media_enabled: true,
                        half_res_participants: false,
                        hit_compaction_enabled: true,
                        active_degradations: vec!["reduce_internal_resolution".to_string()],
                    },
                    primary_hit_rate: 0.75,
                    average_trace_steps: 12.0,
                    max_trace_steps: 24,
                    candidate_count_before_pruning: 100,
                    candidate_count_after_pruning: 60,
                    support_prune_effectiveness: 0.4,
                    tile_cull_total_tiles: 16,
                    tile_cull_active_tiles: 9,
                    tile_cull_efficiency: 0.4375,
                    tile_candidate_total_samples: 256,
                    tile_candidate_active_samples: 256,
                    tile_candidate_reduction: 0,
                    packet_scheduling_active: false,
                    selected_workgroup_size: 0,
                    surface_resolve_count: 256,
                    participant_resolve_count: 128,
                    history_reuse_rate: 0.25,
                    continuation_diagnostics: vec![],
                    acceleration_node_visits: 0,
                    union_cluster_visits: 0,
                    ray_support_interval_rejections: 0,
                    ray_support_entry_jumps: 0,
                    repeat_cell_skips: 0,
                    cache_brick_visits: 0,
                    cache_brick_hits: 0,
                    cache_brick_misses: 0,
                    cache_interval_advances: 0,
                    accepted_relaxed_steps: 0,
                    rejected_relaxed_steps: 0,
                    analytic_transformed_hits: 0,
                    interval_subdivisions: 0,
                    interval_proof_successes: 0,
                    observer_continuation_seed_hits: 0,
                    solver_relaxed_attempts: 0,
                    solver_relaxed_no_root_advances: 0,
                    solver_relaxed_brackets: 0,
                    solver_relaxed_unresolved: 0,
                    solver_interval_attempts: 0,
                    solver_interval_no_root_advances: 0,
                    solver_interval_brackets: 0,
                    solver_interval_unresolved: 0,
                    solver_refinement_attempts: 0,
                    solver_refinement_failures: 0,
                    solver_repeat_attempts: 0,
                    solver_repeat_supported: 0,
                    solver_repeat_inapplicable: 0,
                    solver_repeat_unsupported: 0,
                    solver_repeat_unsupported_form: 0,
                    solver_repeat_unsupported_bounds: 0,
                    solver_repeat_cells_enumerated: 0,
                    attachment_bytes: vec![],
                    passes: vec![wrela::presentation_exec::PresentationPassCost {
                        pass_id: "primary.visibility".to_string(),
                        pass_kind: "primary_visibility".to_string(),
                        work_items: 1024,
                        elapsed_micros: 2100,
                        gpu_elapsed_micros: None,
                        dispatch_count: 1,
                        attachment_bytes_read: 0,
                        attachment_bytes_written: 8192,
                        notes: vec!["dynamic_resolution".to_string()],
                    }],
                    active_acceleration_artifacts: vec![
                        "tile_candidate_table".to_string(),
                        "packet_scheduling".to_string(),
                    ],
                    bottleneck_pass: Some("primary_visibility".to_string()),
                    performance_gain_sources: vec![
                        "support_pruning".to_string(),
                        "tile_culling".to_string(),
                        "tile_candidate_table".to_string(),
                        "packet_scheduling".to_string(),
                        "quality_degradation_active".to_string(),
                    ],
                },
            ],
        };

        let report = presentation_report_from_debug_output(&scenario, dump);
        assert_eq!(report.scenario_id, "presentation_fixture");
        assert_eq!(report.frames_executed, 2);
        assert_eq!(report.frame_time_ns, 3_300_000);
        assert_eq!(report.field_samples, 1024);
        assert_eq!(report.query_trace_solver_mode, "hybrid");
        assert_eq!(report.selected_workgroup_size, 64);
        assert_eq!(report.quality_tier, "realtime_60");
        assert_eq!(report.internal_resolution_scale, 0.5);
        assert!(report.reconstructed_output);
        assert_eq!(report.internal_resolution_history, vec![1.0, 0.5]);
        assert_eq!(
            report.bottleneck_pass.as_deref(),
            Some("primary_visibility")
        );
        assert!(report.wgsl_workgroup_comparison.is_none());
        assert_eq!(report.frame_cost.passes.len(), 1);
        assert_eq!(report.frame_cost.passes[0].pass_kind, "primary_visibility");
    }

    #[test]
    fn presentation_report_from_debug_output_drops_cold_start_frame_for_closure_scenarios() {
        let scenario = command_handlers::BenchmarkScenario {
            id: "closure_fixture".to_string(),
            test_name: "tests/fixture::closure_ops_64".to_string(),
            ops: 64,
            class: "closure".to_string(),
            min_runtime_ms: None,
            timeout_ms: None,
            allow_unstable: false,
            presentation: None,
            collision: None,
        };
        let warm = sample_presentation_frame_cost(32, 32, 0.5, 30, 4.0, 10, 7, 2_000);
        let dump = PresentationDebugCommandOutput {
            view: "bench_view".to_string(),
            region: "bench_region".to_string(),
            domain: "bench_domain".to_string(),
            backend: "cpu".to_string(),
            query_trace_solver_mode: "hybrid".to_string(),
            frames_executed: 2,
            frame_cost: warm.clone(),
            frame_cost_history: vec![
                sample_presentation_frame_cost(64, 64, 1.0, 10, 2.0, 6, 4, 1_000),
                warm.clone(),
            ],
        };

        let report = presentation_report_from_debug_output(&scenario, dump);

        assert_eq!(report.frames_executed, 1);
        assert_eq!(report.frame_time_ns, 2_000_000);
        assert_eq!(report.field_samples, 30);
        assert_eq!(report.internal_resolution_history, vec![0.5]);
        assert_eq!(report.frame_cost_history, vec![warm]);
    }

    #[test]
    fn presentation_comparison_aggregates_multi_frame_solver_metrics() {
        let scenario = command_handlers::BenchmarkScenario {
            id: "presentation_fixture".to_string(),
            test_name: "tests/fixture::test_ops_64".to_string(),
            ops: 64,
            class: "critical".to_string(),
            min_runtime_ms: None,
            timeout_ms: None,
            allow_unstable: false,
            presentation: None,
            collision: None,
        };
        let hybrid_dump = PresentationDebugCommandOutput {
            view: "bench_view".to_string(),
            region: "bench_region".to_string(),
            domain: "bench_domain".to_string(),
            backend: "cpu".to_string(),
            query_trace_solver_mode: "hybrid".to_string(),
            frames_executed: 2,
            frame_cost: sample_presentation_frame_cost(32, 32, 0.5, 30, 4.0, 10, 7, 2_000),
            frame_cost_history: vec![
                sample_presentation_frame_cost(64, 64, 1.0, 10, 2.0, 6, 4, 1_000),
                sample_presentation_frame_cost(32, 32, 0.5, 30, 4.0, 10, 7, 2_000),
            ],
        };
        let dense_only_dump = PresentationDebugCommandOutput {
            query_trace_solver_mode: "dense-only".to_string(),
            frame_cost: sample_presentation_frame_cost(32, 32, 0.5, 40, 5.0, 12, 9, 2_500),
            frame_cost_history: vec![
                sample_presentation_frame_cost(64, 64, 1.0, 20, 3.0, 8, 6, 1_500),
                sample_presentation_frame_cost(32, 32, 0.5, 40, 5.0, 12, 9, 2_500),
            ],
            ..hybrid_dump.clone()
        };

        let hybrid_report = presentation_report_from_debug_output(&scenario, hybrid_dump);
        let comparison =
            presentation_comparison_from_debug_reports(&hybrid_report, &dense_only_dump);

        assert_eq!(hybrid_report.field_samples, 40);
        assert_eq!(comparison.dense_only_field_samples, 60);
        assert_eq!(comparison.field_samples_delta_vs_dense_only, -20);
        assert_eq!(comparison.dense_only_candidate_count_before_pruning, 20);
        assert_eq!(
            comparison.candidate_count_before_pruning_delta_vs_dense_only,
            -4
        );
        assert_eq!(comparison.dense_only_candidate_count_after_pruning, 15);
        assert_eq!(
            comparison.candidate_count_after_pruning_delta_vs_dense_only,
            -4
        );
        assert!((comparison.dense_only_average_trace_steps - 4.0).abs() < f32::EPSILON);
        assert!((comparison.average_trace_steps_delta_vs_dense_only + 1.0).abs() < f32::EPSILON);
        assert_eq!(comparison.dense_only_frame_time_ns, 4_000_000);
        assert_eq!(comparison.frame_time_ns_delta_vs_dense_only, -1_000_000);
    }

    #[test]
    fn presentation_workgroup_comparison_tracks_candidate_deltas() {
        let make_report = |workgroup_size: u32, frame_time_ns: u128| PresentationBenchmarkReport {
            scenario_id: "scenario".to_string(),
            test_name: "tests/fixture".to_string(),
            view: "bench_view".to_string(),
            region: "bench_region".to_string(),
            domain: "bench_domain".to_string(),
            backend: "wgsl".to_string(),
            observed_adapter_name: None,
            query_trace_solver_mode: "hybrid".to_string(),
            selected_workgroup_size: workgroup_size,
            frames_executed: 1,
            frame_time_ns,
            field_samples: 512,
            quality_tier: "realtime_120".to_string(),
            target_fps: 120,
            internal_resolution_scale: 1.0,
            reconstructed_output: false,
            quality_history: vec!["realtime_120".to_string()],
            internal_resolution_history: vec![1.0],
            bottleneck_pass: Some("primary_visibility".to_string()),
            active_acceleration_artifacts: vec!["packet_scheduling".to_string()],
            performance_gain_sources: vec!["packet_scheduling".to_string()],
            frame_cost: sample_presentation_frame_cost(64, 64, 1.0, 512, 4.0, 10, 8, 1_000),
            frame_cost_history: vec![],
            wgsl_workgroup_comparison: None,
            ab_comparison: None,
        };
        let reports = vec![
            make_report(32, 7_500_000),
            make_report(64, 6_000_000),
            make_report(128, 6_500_000),
        ];
        let comparison = presentation_workgroup_comparison_from_reports(&reports, &reports[1]);
        assert_eq!(comparison.selected_workgroup_size, 64);
        assert_eq!(comparison.candidate_workgroup_sizes, vec![32, 64, 128]);
        assert_eq!(
            comparison.frame_time_ns_delta_vs_selected,
            vec![1_500_000, 0, 500_000]
        );
        assert_eq!(
            format_workgroup_comparison(&comparison),
            "32:7500000ns(+25.00%) 64:6000000ns(+0.00%) 128:6500000ns(+8.33%)"
        );
    }
}
