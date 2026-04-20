//! Owns perf-run collection and assembly: manifest selection, benchmark
//! execution, gate inputs, and report construction for the `perf` command.
//! Does not own CLI parsing, closure verdict policy, or perfcmp worktree logic.
//!
//! Key invariants:
//! - typed scenario identity stays intact while runs are collected and only
//!   renders to strings at report boundaries.
//! - baseline overlays may annotate measured runs, but they must not replace the
//!   observed runtime cases that closure logic depends on.
//! - whole-frame joins pair presentation and collision evidence by the same
//!   scenario identity, not by positional ordering.
//!
//! Primary entrypoints:
//! - `execute_perf_command`
//! - `build_whole_frame_benchmark_reports`
//! - `collision_runtime_cases_by_scenario_id`
//!
//! Failure modes / common pitfalls:
//! - losing scenario identity in intermediate tuples makes later overlays and
//!   joins silently misattribute runtime evidence.
//! - splitting this lane too early would scatter one still-evolving ownership
//!   boundary across multiple mutable adapters.
//!
//! Phase 53 explicit size-cap exception: this module stays slightly above the
//! 2,500-line target because the `perf` command still owns one coherent
//! collection/orchestration lane spanning manifest selection, benchmark runs,
//! gate inputs, and report assembly.
use super::*;
use std::ffi::OsString;

pub(crate) struct PerfCommandInput {
    pub(crate) trace: bool,
    pub(crate) program_args: Vec<String>,
    pub(crate) path_arg: Option<String>,
    pub(crate) perf_runs: Option<usize>,
    pub(crate) test_jobs: Option<usize>,
    pub(crate) test_timeout_ms: Option<u64>,
    pub(crate) benchmark_manifest_path: Option<String>,
    pub(crate) perf_profile: PerfProfile,
    pub(crate) perf_baseline_out: Option<String>,
    pub(crate) perf_gate_path: Option<String>,
    pub(crate) perf_max_regression_pct: Option<f64>,
    pub(crate) perf_cv_max_pct: Option<f64>,
    pub(crate) perf_why_not_120: bool,
    pub(crate) kpi_thresholds: KpiThresholds,
    pub(crate) output_format: OutputFormat,
    pub(crate) perf_debug: bool,
    pub(crate) test_selection: TestSelection,
    pub(crate) query_backend: wrela::query_plan::DispatchBackend,
}

struct ScopedEnvVar {
    key: &'static str,
    previous: Option<OsString>,
}

impl ScopedEnvVar {
    fn set(key: &'static str, value: Option<&str>) -> Self {
        let previous = env::var_os(key);
        match value {
            Some(value) => unsafe { env::set_var(key, value) },
            None => unsafe { env::remove_var(key) },
        }
        Self { key, previous }
    }
}

impl Drop for ScopedEnvVar {
    fn drop(&mut self) {
        if let Some(previous) = self.previous.as_ref() {
            unsafe { env::set_var(self.key, previous) };
        } else {
            unsafe { env::remove_var(self.key) };
        }
    }
}

pub(crate) fn execute_perf_command(mut input: PerfCommandInput) -> i32 {
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
        let scenario_selection = manifest.scenario_selection(input.perf_profile);
        let max_timeout_ms = scenario_selection.max_timeout_ms();
        if let Some(max_timeout_ms) = max_timeout_ms {
            timeout = timeout.max(std::time::Duration::from_millis(max_timeout_ms));
        }
        benchmark_manifest = Some(manifest);
        runtime_only_cv_gate = true;
        match test_eval_perf::build_benchmark_selection_from_manifest(
            &target,
            benchmark_manifest
                .as_ref()
                .expect("benchmark manifest should be loaded"),
            input.perf_profile,
        ) {
            Ok(selection_ids) => {
                test_eval_perf::set_test_selection_include_ids(
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
    budget_policy: &build_compile::BudgetPolicyV1,
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
    let query_backend = effective_perf_query_backend(perf_profile, query_backend);
    let closure_protocol = matches!(perf_profile, PerfProfile::Closure1080p120)
        .then(PerfClosureProfile::canonical_1080p120);
    let closure_timestamps_enabled = closure_protocol
        .as_ref()
        .filter(|_| query_backend == wrela::query_plan::DispatchBackend::Wgsl)
        .is_some_and(|profile| profile.timestamps_enabled);
    let _timestamp_env = ScopedEnvVar::set(
        wrela::query_exec::wgsl::QUERY_GPU_TIMESTAMPS_ENV,
        closure_timestamps_enabled.then_some("1"),
    );
    let _timestamp_override = wrela::query_exec::wgsl::override_gpu_timestamps_for_current_thread(
        closure_protocol
            .as_ref()
            .filter(|_| query_backend == wrela::query_plan::DispatchBackend::Wgsl)
            .map(|profile| profile.timestamps_enabled),
    );
    let benchmark_scenarios =
        benchmark_manifest.map(|manifest| manifest.scenario_selection(perf_profile));
    let presentation_benchmarks_active = benchmark_scenarios
        .as_ref()
        .is_some_and(|selection| selection.includes_presentation());
    let collision_benchmarks_active = benchmark_scenarios
        .as_ref()
        .is_some_and(|selection| selection.includes_collision());
    let whole_frame_benchmarks_active =
        presentation_benchmarks_active && collision_benchmarks_active;
    let presentation_collection_mode = presentation_benchmarks_active
        .then(|| presentation_benchmark_collection_mode(perf_profile, perf_why_not_120));
    let skip_authored_composite_harness =
        should_skip_authored_composite_harness(whole_frame_benchmarks_active, perf_profile);
    let expected_presentation_report_count = benchmark_scenarios
        .as_ref()
        .map(|selection| selection.presentation_count())
        .unwrap_or(0);
    let expected_collision_execution_count = benchmark_scenarios
        .as_ref()
        .map(|selection| selection.collision_count())
        .unwrap_or(0);
    let expected_whole_frame_report_count = benchmark_scenarios
        .as_ref()
        .map(|selection| selection.whole_frame_count())
        .unwrap_or(0);
    let mut samples = Vec::new();
    let mut collision_summary_samples = Vec::new();
    let mut latest_presentation_reports = None;
    let mut presentation_report_samples = Vec::new();
    let mut presentation_report_errors = Vec::new();
    let mut latest_whole_frame_reports = None;
    let mut latest_engine_frame_reports = None;
    let mut whole_frame_report_samples = Vec::new();
    let mut engine_frame_report_samples = Vec::new();
    let mut whole_frame_report_errors = Vec::new();
    let mut engine_frame_report_errors = Vec::new();
    let mut latest_collision_reports = None;
    let mut collision_report_samples = Vec::new();
    let mut collision_report_errors = Vec::new();
    let mut late_failures = Vec::<String>::new();
    if skip_authored_composite_harness && matches!(output_format, OutputFormat::Pretty) {
        println!(
            "perf-note: composite closure skips authored whole-frame harness execution and uses collected presentation/collision reports"
        );
    }
    for idx in 0..warmup_runs {
        println!("perf-warmup {}/{}", idx + 1, warmup_runs);
        if skip_authored_composite_harness {
            continue;
        }
        let (exit, _, _) = test_eval_perf::run_tests_once(
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
            test_eval_perf::HttpCassetteMode::Replay,
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
        let summary = if skip_authored_composite_harness {
            Some(empty_perf_summary())
        } else {
            let (exit, summary, _) = test_eval_perf::run_tests_once(
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
                test_eval_perf::HttpCassetteMode::Replay,
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
            summary
        };
        if let Some(summary) = summary {
            if whole_frame_benchmarks_active {
                let TestTarget::ProjectRoot(benchmark_root) = target else {
                    eprintln!("perf harness error: whole-frame benchmarks require a project root");
                    return EXIT_CODEGEN;
                };
                let Some(manifest) = benchmark_manifest else {
                    eprintln!(
                        "perf harness error: whole-frame benchmarks require a benchmark manifest"
                    );
                    return EXIT_CODEGEN;
                };
                let Some(scenarios) = benchmark_scenarios.as_ref() else {
                    eprintln!(
                        "perf harness error: whole-frame benchmarks require benchmark scenarios"
                    );
                    return EXIT_CODEGEN;
                };
                let collect_once_for_closure = matches!(perf_profile, PerfProfile::Closure1080p120);
                let collect_reports_this_run = !collect_once_for_closure
                    || latest_presentation_reports.is_none()
                    || latest_collision_reports.is_none()
                    || latest_whole_frame_reports.is_none()
                    || latest_engine_frame_reports.is_none();
                let (
                    presentation_reports,
                    presentation_errors,
                    collision_reports,
                    collision_errors,
                    whole_frame_reports,
                    whole_frame_errors,
                    engine_frame_reports,
                    engine_frame_errors,
                    report_collection_errors,
                ) = if collect_reports_this_run {
                    let presentation_collection = match collect_presentation_benchmark_reports(
                        benchmark_root,
                        scenarios.scenarios(),
                        query_backend,
                        presentation_collection_mode
                            .expect("presentation benchmarks should set collection mode"),
                    ) {
                        Ok(collection) => collection,
                        Err(err) => {
                            eprintln!(
                                "perf harness error: failed to collect presentation reports: {err}"
                            );
                            return EXIT_CODEGEN;
                        }
                    };
                    let collision_collection = match collect_collision_benchmark_reports(
                        benchmark_root,
                        manifest,
                        scenarios.scenarios(),
                        query_backend,
                    ) {
                        Ok(collection) => collection,
                        Err(err) => {
                            eprintln!(
                                "perf harness error: failed to collect collision reports: {err}"
                            );
                            return EXIT_CODEGEN;
                        }
                    };
                    let mut presentation_errors = presentation_collection.errors;
                    let mut collision_errors = collision_collection.errors;
                    let mut whole_frame_errors = Vec::new();
                    let mut report_collection_errors = Vec::new();
                    if presentation_collection.reports.len() != expected_presentation_report_count {
                        let error = format!(
                            "presentation benchmark collection returned {} report(s) for {} expected scenario(s) in {} mode",
                            presentation_collection.reports.len(),
                            expected_presentation_report_count,
                            presentation_collection_mode
                                .expect("presentation benchmarks should set collection mode")
                                .as_str()
                        );
                        presentation_errors.push(error.clone());
                        report_collection_errors.push(error);
                    }
                    let collision_execution_count = collision_collection
                        .reports
                        .iter()
                        .map(|report| report.executions.len())
                        .sum::<usize>();
                    if collision_execution_count != expected_collision_execution_count {
                        let error = format!(
                            "collision benchmark collection returned {} execution(s) for {} expected scenario(s)",
                            collision_execution_count, expected_collision_execution_count
                        );
                        collision_errors.push(error.clone());
                        report_collection_errors.push(error);
                    }
                    let whole_frame_reports = match build_whole_frame_benchmark_reports(
                        &presentation_collection.reports,
                        &collision_collection.reports,
                    ) {
                        Ok(reports) => reports,
                        Err(err) => {
                            whole_frame_errors.push(err.clone());
                            report_collection_errors.push(err);
                            Vec::new()
                        }
                    };
                    let mut engine_frame_errors = Vec::new();
                    let engine_frame_reports = match build_engine_frame_benchmark_reports(
                        &presentation_collection.reports,
                        &collision_collection.reports,
                        closure_protocol
                            .as_ref()
                            .map(|profile| &profile.engine_frame_budget),
                    ) {
                        Ok(reports) => reports,
                        Err(err) => {
                            engine_frame_errors.push(err.clone());
                            report_collection_errors.push(err);
                            Vec::new()
                        }
                    };
                    if whole_frame_reports.len() != expected_whole_frame_report_count {
                        let error = format!(
                            "whole-frame report collection returned {} report(s) for {} expected composite scenario(s)",
                            whole_frame_reports.len(),
                            expected_whole_frame_report_count
                        );
                        whole_frame_errors.push(error.clone());
                        report_collection_errors.push(error);
                    }
                    if engine_frame_reports.len() != expected_whole_frame_report_count {
                        let error = format!(
                            "engine-frame report collection returned {} report(s) for {} expected composite scenario(s)",
                            engine_frame_reports.len(),
                            expected_whole_frame_report_count
                        );
                        engine_frame_errors.push(error.clone());
                        report_collection_errors.push(error);
                    }
                    (
                        presentation_collection.reports,
                        presentation_errors,
                        collision_collection.reports,
                        collision_errors,
                        whole_frame_reports,
                        whole_frame_errors,
                        engine_frame_reports,
                        engine_frame_errors,
                        report_collection_errors,
                    )
                } else {
                    (
                            latest_presentation_reports
                                .clone()
                                .expect("closure whole-frame benchmarks should have cached presentation reports"),
                            Vec::new(),
                            latest_collision_reports
                                .clone()
                                .expect("closure whole-frame benchmarks should have cached collision reports"),
                            Vec::new(),
                            latest_whole_frame_reports
                                .clone()
                                .expect("closure whole-frame benchmarks should have cached whole-frame reports"),
                            Vec::new(),
                            latest_engine_frame_reports
                                .clone()
                                .expect("closure whole-frame benchmarks should have cached engine-frame reports"),
                            Vec::new(),
                            Vec::new(),
                        )
                };
                let composite_runtime_cases = if manifest.suite.eq_ignore_ascii_case("engine_frame")
                {
                    engine_frame_runtime_cases_from_reports(&engine_frame_reports)
                } else {
                    whole_frame_runtime_cases_from_reports(&whole_frame_reports)
                };
                let collision_runtime_cases = collision_reports
                    .iter()
                    .flat_map(collision_runtime_cases_by_scenario_id)
                    .collect::<Vec<_>>();
                let summary = if composite_runtime_cases.is_empty() {
                    summary
                } else {
                    test_eval_perf::overlay_perf_summary_runtime_cases(
                        &summary,
                        &composite_runtime_cases,
                    )
                };
                if !collision_runtime_cases.is_empty() {
                    collision_summary_samples.push(
                        test_eval_perf::overlay_perf_summary_runtime_cases(
                            &summary,
                            &collision_runtime_cases,
                        ),
                    );
                }
                if matches!(output_format, OutputFormat::Pretty) {
                    test_eval_perf::emit_perf_summary(&summary, perf_debug);
                    if collect_reports_this_run {
                        print_presentation_benchmark_reports(&presentation_reports);
                        print_collision_benchmark_reports(&collision_reports);
                        print_whole_frame_benchmark_reports(&whole_frame_reports);
                        print_engine_frame_benchmark_reports(&engine_frame_reports);
                    }
                    for error in &report_collection_errors {
                        eprintln!("whole-frame-benchmark-error: {error}");
                    }
                }
                if matches!(perf_profile, PerfProfile::Closure1080p120)
                    && collect_reports_this_run
                    && !report_collection_errors.is_empty()
                {
                    late_failures.extend(report_collection_errors.iter().map(|error| {
                        format!("whole-frame closure report collection unstable: {error}")
                    }));
                }
                if collect_reports_this_run {
                    presentation_report_samples.extend(presentation_reports.iter().cloned());
                    presentation_report_errors.extend(presentation_errors);
                    whole_frame_report_samples.extend(whole_frame_reports.iter().cloned());
                    whole_frame_report_errors.extend(whole_frame_errors);
                    engine_frame_report_samples.extend(engine_frame_reports.iter().cloned());
                    engine_frame_report_errors.extend(engine_frame_errors);
                    collision_report_samples.extend(collision_reports.iter().cloned());
                    collision_report_errors.extend(collision_errors);
                    latest_presentation_reports = Some(presentation_reports.clone());
                    latest_collision_reports = Some(collision_reports.clone());
                    latest_whole_frame_reports = Some(whole_frame_reports.clone());
                    latest_engine_frame_reports = Some(engine_frame_reports.clone());
                }
                samples.push(summary);
            } else if presentation_benchmarks_active {
                let TestTarget::ProjectRoot(benchmark_root) = target else {
                    eprintln!("perf harness error: presentation benchmarks require a project root");
                    return EXIT_CODEGEN;
                };
                if benchmark_manifest.is_none() {
                    eprintln!(
                        "perf harness error: presentation benchmarks require a benchmark manifest"
                    );
                    return EXIT_CODEGEN;
                }
                let Some(scenarios) = benchmark_scenarios.as_ref() else {
                    eprintln!(
                        "perf harness error: presentation benchmarks require benchmark scenarios"
                    );
                    return EXIT_CODEGEN;
                };
                let collect_once_for_closure = matches!(perf_profile, PerfProfile::Closure1080p120);
                let collect_reports_this_run =
                    !collect_once_for_closure || latest_presentation_reports.is_none();
                let mut report_collection_errors = Vec::new();
                let presentation_reports = if collect_reports_this_run {
                    let report_collection = match collect_presentation_benchmark_reports(
                        benchmark_root,
                        scenarios.scenarios(),
                        query_backend,
                        presentation_collection_mode
                            .expect("presentation benchmarks should set collection mode"),
                    ) {
                        Ok(collection) => collection,
                        Err(err) => {
                            eprintln!(
                                "perf harness error: failed to collect presentation reports: {err}"
                            );
                            return EXIT_CODEGEN;
                        }
                    };
                    report_collection_errors = report_collection.errors;
                    if report_collection.reports.len() != expected_presentation_report_count {
                        report_collection_errors.push(format!(
                            "presentation benchmark collection returned {} report(s) for {} expected scenario(s) in {} mode",
                            report_collection.reports.len(),
                            expected_presentation_report_count,
                            presentation_collection_mode
                                .expect("presentation benchmarks should set collection mode")
                                .as_str()
                        ));
                    }
                    report_collection.reports
                } else {
                    latest_presentation_reports
                        .clone()
                        .expect("closure presentation benchmarks should have cached reports")
                };
                let runtime_cases = presentation_reports
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
                    test_eval_perf::overlay_perf_summary_runtime_cases(&summary, &runtime_cases)
                };
                if matches!(output_format, OutputFormat::Pretty) {
                    test_eval_perf::emit_perf_summary(&summary, perf_debug);
                    if collect_reports_this_run {
                        print_presentation_benchmark_reports(&presentation_reports);
                    }
                    for error in &report_collection_errors {
                        eprintln!("presentation-benchmark-error: {error}");
                    }
                }
                if matches!(perf_profile, PerfProfile::Closure1080p120)
                    && collect_reports_this_run
                    && !report_collection_errors.is_empty()
                {
                    late_failures.extend(report_collection_errors.iter().map(|error| {
                        format!("presentation closure report collection unstable: {error}")
                    }));
                }
                if collect_reports_this_run {
                    presentation_report_samples.extend(presentation_reports.iter().cloned());
                    presentation_report_errors.extend(report_collection_errors.into_iter());
                    latest_presentation_reports = Some(presentation_reports.clone());
                }
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
                let Some(scenarios) = benchmark_scenarios.as_ref() else {
                    eprintln!(
                        "perf harness error: collision benchmarks require benchmark scenarios"
                    );
                    return EXIT_CODEGEN;
                };
                let report_collection = match collect_collision_benchmark_reports(
                    benchmark_root,
                    manifest,
                    scenarios.scenarios(),
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
                    test_eval_perf::overlay_perf_summary_runtime_cases(&summary, &runtime_cases)
                };
                if matches!(perf_profile, PerfProfile::Closure1080p120)
                    && (report_collection.reports.is_empty()
                        || !report_collection.errors.is_empty())
                {
                    if report_collection.reports.is_empty() {
                        late_failures.push(
                            "collision closure report collection unstable: no collision benchmark reports were produced"
                                .to_string(),
                        );
                    }
                    late_failures.extend(report_collection.errors.iter().map(|error| {
                        format!("collision closure report collection unstable: {error}")
                    }));
                }
                collision_report_samples.extend(report_collection.reports.iter().cloned());
                collision_report_errors.extend(report_collection.errors.into_iter());
                latest_collision_reports = Some(report_collection.reports);
                collision_summary_samples.push(summary.clone());
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
    let summary = test_eval_perf::aggregate_perf_samples(&samples);
    let cv = test_eval_perf::compute_cv(&samples);
    if collision_benchmarks_active
        && latest_collision_reports.is_none()
        && !collision_report_samples.is_empty()
    {
        latest_collision_reports = Some(collision_report_samples.clone());
    }
    if matches!(output_format, OutputFormat::Pretty) {
        if !whole_frame_benchmarks_active && let Some(reports) = latest_collision_reports.as_ref() {
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
        let baseline = match test_eval_perf::load_perf_baseline_summary(&gate.baseline_path) {
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
        let failures = test_eval_perf::evaluate_perf_gate(
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
        &collision_summary_samples,
        &presentation_report_samples,
        &presentation_report_errors,
        &whole_frame_report_samples,
        &whole_frame_report_errors,
        &engine_frame_report_samples,
        &engine_frame_report_errors,
        &collision_report_samples,
        &collision_report_errors,
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
        whole_frame_reports: latest_whole_frame_reports,
        engine_frame_reports: latest_engine_frame_reports,
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
    if !late_failures.is_empty() {
        eprintln!("perf harness failed: unstable benchmark collection");
        for failure in late_failures {
            eprintln!("  - {failure}");
        }
        return EXIT_CODEGEN;
    }
    EXIT_OK
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct PresentationDebugCommandOutput {
    pub(super) view: String,
    pub(super) region: String,
    pub(super) domain: String,
    pub(super) backend: String,
    pub(super) query_trace_solver_mode: String,
    pub(super) frames_executed: u32,
    pub(super) frame_cost: wrela::presentation_exec::PresentationFrameCostReport,
    #[serde(default)]
    pub(super) frame_cost_history: Vec<wrela::presentation_exec::PresentationFrameCostReport>,
}

struct PresentationBenchmarkReportCollection {
    reports: Vec<PresentationBenchmarkReport>,
    errors: Vec<String>,
}

struct CollisionBenchmarkReportCollection {
    reports: Vec<test_eval_perf::CollisionBenchmarkReport>,
    errors: Vec<String>,
}

#[derive(Debug, Clone, Default)]
struct CollisionBenchmarkScenarioMetrics {
    query_count: u64,
    total_batch_count: u64,
    total_runtime_ns: u128,
    timestamps_supported: bool,
    total_timestamped_pass_count: u64,
    total_gpu_time_total_micros: u128,
    max_gpu_time_micros: u128,
    total_candidate_count: u64,
    total_rejected_candidate_count: u64,
    total_pruned_node_count: u64,
    total_candidate_reduction_effectiveness: f64,
    total_interval_subdivisions: u64,
    total_interval_refinements: u64,
    total_certificate_successes: u64,
    total_fallback_count: u64,
    total_wgsl_dispatch_count: u64,
    total_wgsl_dispatch_items: u64,
    total_wgsl_resident_shared_snapshot_artifacts: u64,
    total_cpu_certification_query_count: u64,
    total_hot_path_readback_bytes: u64,
    total_queue_submit_count: u64,
    total_scene_reupload_bytes: u64,
    total_candidate_table_overflow_fallback_count: u64,
    max_wgsl_selected_workgroup_size: u32,
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
pub(super) struct PresentationAggregatedSolverCounters {
    pub(super) solver_relaxed_attempts: u64,
    pub(super) solver_relaxed_no_root_advances: u64,
    pub(super) solver_relaxed_brackets: u64,
    pub(super) solver_relaxed_unresolved: u64,
    pub(super) solver_interval_attempts: u64,
    pub(super) solver_interval_no_root_advances: u64,
    pub(super) solver_interval_brackets: u64,
    pub(super) solver_interval_unresolved: u64,
    pub(super) solver_refinement_attempts: u64,
    pub(super) solver_refinement_failures: u64,
    pub(super) solver_repeat_attempts: u64,
    pub(super) solver_repeat_supported: u64,
    pub(super) solver_repeat_inapplicable: u64,
    pub(super) solver_repeat_unsupported: u64,
    pub(super) solver_repeat_unsupported_form: u64,
    pub(super) solver_repeat_unsupported_bounds: u64,
    pub(super) solver_repeat_cells_enumerated: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PresentationBenchmarkCollectionMode {
    Measurement,
    Diagnostic,
}

impl PresentationBenchmarkCollectionMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Measurement => "measurement",
            Self::Diagnostic => "diagnostic",
        }
    }
}

pub(super) fn effective_perf_query_backend(
    perf_profile: PerfProfile,
    query_backend: wrela::query_plan::DispatchBackend,
) -> wrela::query_plan::DispatchBackend {
    if matches!(perf_profile, PerfProfile::Closure1080p120)
        && matches!(query_backend, wrela::query_plan::DispatchBackend::Auto)
    {
        wrela::query_plan::DispatchBackend::Wgsl
    } else {
        query_backend
    }
}

pub(super) fn presentation_benchmark_collection_mode(
    perf_profile: PerfProfile,
    perf_why_not_120: bool,
) -> PresentationBenchmarkCollectionMode {
    if matches!(perf_profile, PerfProfile::Closure1080p120) && !perf_why_not_120 {
        PresentationBenchmarkCollectionMode::Measurement
    } else {
        PresentationBenchmarkCollectionMode::Diagnostic
    }
}

pub(super) fn should_warm_closure_quality_pipelines(
    scenario: &test_eval_perf::BenchmarkScenario,
) -> bool {
    scenario.class.is_closure()
}

fn collect_presentation_benchmark_reports(
    benchmark_root: &Path,
    scenarios: &[&test_eval_perf::BenchmarkScenario],
    query_backend: wrela::query_plan::DispatchBackend,
    collection_mode: PresentationBenchmarkCollectionMode,
) -> Result<PresentationBenchmarkReportCollection, String> {
    let current_exe =
        env::current_exe().map_err(|err| format!("failed to resolve current executable: {err}"))?;
    let mut collection = PresentationBenchmarkReportCollection {
        reports: Vec::new(),
        errors: Vec::new(),
    };
    for scenario in scenarios {
        let Some(spec) = scenario.presentation.as_ref() else {
            continue;
        };
        match run_presentation_benchmark_report(
            &current_exe,
            benchmark_root,
            scenario,
            spec,
            query_backend,
            collection_mode,
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
    scenarios: &[&test_eval_perf::BenchmarkScenario],
    query_backend: wrela::query_plan::DispatchBackend,
) -> Result<CollisionBenchmarkReportCollection, String> {
    let backend = collision_benchmark_backend(query_backend)?;
    let mut collection = CollisionBenchmarkReportCollection {
        reports: Vec::new(),
        errors: Vec::new(),
    };
    let mut contexts = HashMap::<PathBuf, CollisionBenchmarkContext>::new();
    let mut scenario_results = Vec::new();
    for scenario in scenarios {
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
        for _ in 0..collision_benchmark_warmup_run_count(backend) {
            if let Err(err) = run_collision_benchmark_scenario(ctx, scenario, spec, backend) {
                collection.errors.push(format!(
                    "collision benchmark warmup `{}` failed: {err}",
                    scenario.id
                ));
                continue;
            }
        }
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
    execution: test_eval_perf::CollisionBenchmarkExecutionReport,
    metrics: CollisionBenchmarkScenarioMetrics,
}

struct CollisionBenchmarkContext {
    ctx: QueryExecContext,
    module: hir::Module,
}

pub(super) fn collision_benchmark_backend(
    query_backend: wrela::query_plan::DispatchBackend,
) -> Result<wrela::query_plan::DispatchBackend, String> {
    match query_backend {
        wrela::query_plan::DispatchBackend::Cpu | wrela::query_plan::DispatchBackend::Auto => {
            Ok(wrela::query_plan::DispatchBackend::Cpu)
        }
        wrela::query_plan::DispatchBackend::Wgsl => Ok(wrela::query_plan::DispatchBackend::Wgsl),
        other => Err(format!(
            "collision benchmarks only support cpu, auto, or wgsl query backends, not {other:?}"
        )),
    }
}

pub(super) fn collision_benchmark_warmup_run_count(
    backend: wrela::query_plan::DispatchBackend,
) -> usize {
    if matches!(backend, wrela::query_plan::DispatchBackend::Wgsl) {
        1
    } else {
        0
    }
}

pub(super) fn perf_dispatch_backend_name(
    backend: wrela::query_plan::DispatchBackend,
) -> &'static str {
    match backend {
        wrela::query_plan::DispatchBackend::Cpu => "cpu",
        wrela::query_plan::DispatchBackend::VirtualGpu => "virtual_gpu",
        wrela::query_plan::DispatchBackend::Wgsl => "wgsl",
        wrela::query_plan::DispatchBackend::Auto => "auto",
    }
}

pub(super) fn collision_benchmark_entry_path(
    benchmark_root: &Path,
    spec: &test_eval_perf::BenchmarkCollisionSpec,
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
    scenario: &test_eval_perf::BenchmarkScenario,
    spec: &test_eval_perf::BenchmarkCollisionSpec,
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
) -> test_eval_perf::CollisionBenchmarkReport {
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
    test_eval_perf::CollisionBenchmarkReport {
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
    scenario_id: &test_eval_perf::PerfScenarioId,
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
        execution: test_eval_perf::CollisionBenchmarkExecutionReport {
            name: scenario_id.clone(),
            plan_name: plan.name.to_string(),
            contract_id: plan.contract_id.as_str().to_string(),
            query_count: metrics.query_count,
            batch_count: metrics.total_batch_count.min(u64::from(u32::MAX)) as u32,
            dispatch_count: metrics.total_wgsl_dispatch_count.min(u64::from(u32::MAX)) as u32,
            dispatch_items: metrics.total_wgsl_dispatch_items.min(u64::from(u32::MAX)) as u32,
            average_items_per_dispatch: if metrics.total_wgsl_dispatch_count == 0 {
                metrics.query_count as f32
            } else {
                metrics.total_wgsl_dispatch_items as f32 / metrics.total_wgsl_dispatch_count as f32
            },
            runtime_ns: metrics.total_runtime_ns,
            timestamps_supported: metrics.timestamps_supported,
            timestamped_pass_count: metrics
                .total_timestamped_pass_count
                .min(u64::from(u32::MAX)) as u32,
            gpu_time_total_ns: metrics.total_gpu_time_total_micros.saturating_mul(1_000),
            gpu_time_max_ns: metrics.max_gpu_time_micros.saturating_mul(1_000),
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
            candidate_reduction_effectiveness: if metrics.query_count == 0 {
                0.0
            } else {
                metrics.total_candidate_reduction_effectiveness / metrics.query_count as f64
            } as f32,
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
            wgsl_dispatch_count: metrics.total_wgsl_dispatch_count.min(u64::from(u32::MAX)) as u32,
            wgsl_dispatch_items: metrics.total_wgsl_dispatch_items.min(u64::from(u32::MAX)) as u32,
            wgsl_selected_workgroup_size: metrics.max_wgsl_selected_workgroup_size,
            wgsl_resident_shared_snapshot_artifacts: metrics
                .total_wgsl_resident_shared_snapshot_artifacts
                .min(u64::from(u32::MAX))
                as u32,
            cpu_certification_query_count: metrics
                .total_cpu_certification_query_count
                .min(u64::from(u32::MAX)) as u32,
            hot_path_readback_bytes: metrics.total_hot_path_readback_bytes,
            queue_submit_count: metrics.total_queue_submit_count.min(u64::from(u32::MAX)) as u32,
            scene_reupload_bytes: metrics.total_scene_reupload_bytes,
            candidate_table_overflow_fallback_count: metrics
                .total_candidate_table_overflow_fallback_count
                .min(u64::from(u32::MAX))
                as u32,
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

pub(super) fn collision_average(total: u64, count: u64) -> f64 {
    if count == 0 {
        0.0
    } else {
        total as f64 / count as f64
    }
}

pub(super) fn collision_average_u32(total: u64, count: u64) -> u32 {
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
    metrics.total_batch_count = metrics.total_batch_count.saturating_add(1);
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
    if let Some(wgsl_metrics) = &trace.wgsl_metrics {
        metrics.total_candidate_reduction_effectiveness +=
            f64::from(wgsl_metrics.candidate_reduction_effectiveness);
        metrics.total_wgsl_dispatch_count = metrics
            .total_wgsl_dispatch_count
            .saturating_add(u64::from(wgsl_metrics.dispatch_count));
        metrics.total_wgsl_dispatch_items = metrics
            .total_wgsl_dispatch_items
            .saturating_add(u64::from(wgsl_metrics.dispatch_items));
        metrics.total_wgsl_resident_shared_snapshot_artifacts = metrics
            .total_wgsl_resident_shared_snapshot_artifacts
            .saturating_add(u64::from(wgsl_metrics.resident_shared_snapshot_artifacts));
        metrics.total_cpu_certification_query_count = metrics
            .total_cpu_certification_query_count
            .saturating_add(u64::from(wgsl_metrics.cpu_certification_query_count));
        metrics.max_wgsl_selected_workgroup_size = metrics
            .max_wgsl_selected_workgroup_size
            .max(wgsl_metrics.selected_workgroup_size);
    }
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

fn collision_metrics_from_batch_report(
    runtime_ns: u128,
    report: &wrela::collision_plan::CollisionBatchExecutionReport,
) -> CollisionBenchmarkScenarioMetrics {
    CollisionBenchmarkScenarioMetrics {
        query_count: report.query_count,
        total_batch_count: u64::from(report.batch_count),
        total_runtime_ns: runtime_ns,
        timestamps_supported: report.timestamps_supported,
        total_timestamped_pass_count: u64::from(report.timestamped_pass_count),
        total_gpu_time_total_micros: report.gpu_time_total_micros,
        max_gpu_time_micros: report.gpu_time_max_micros,
        total_candidate_count: report.total_candidate_count,
        total_rejected_candidate_count: report.total_rejected_candidate_count,
        total_pruned_node_count: report.total_pruned_node_count,
        total_candidate_reduction_effectiveness: report.total_candidate_reduction_effectiveness
            * report.query_count as f64,
        total_interval_subdivisions: report.total_interval_subdivisions,
        total_interval_refinements: report.total_interval_refinements,
        total_certificate_successes: report.total_certificate_successes,
        total_fallback_count: u64::from(report.fallback_count),
        total_wgsl_dispatch_count: u64::from(report.dispatch_count),
        total_wgsl_dispatch_items: u64::from(report.dispatch_items),
        total_wgsl_resident_shared_snapshot_artifacts: u64::from(
            report.wgsl_resident_shared_snapshot_artifacts,
        ),
        total_cpu_certification_query_count: u64::from(report.cpu_certification_query_count),
        total_hot_path_readback_bytes: report.hot_path_readback_bytes,
        total_queue_submit_count: u64::from(report.queue_submit_count),
        total_scene_reupload_bytes: report.scene_reupload_bytes,
        total_candidate_table_overflow_fallback_count: u64::from(
            report.candidate_table_overflow_fallback_count,
        ),
        max_wgsl_selected_workgroup_size: report.wgsl_selected_workgroup_size,
        available_count_total: report.available_count_total,
        consumed_count_total: report.consumed_count_total,
        rejected_count_total: report.rejected_count_total,
        unavailable_count_total: report.unavailable_count_total,
        last_interval_bracket: report.last_interval_bracket,
        contact_normal_provenance: report.contact_normal_provenance.clone(),
    }
}

pub(super) fn collision_benchmark_domain(
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

pub(super) fn collision_benchmark_capture(scene_id: u32, epoch: u32) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("RegionCapture"),
        fields: vec![
            (SmolStr::new("scene_id"), KernelValue::U32(scene_id)),
            (SmolStr::new("epoch"), KernelValue::U32(epoch)),
        ],
    })
}

pub(super) fn collision_benchmark_transition(
    current_epoch: u32,
    previous_epoch: u32,
) -> KernelValue {
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

pub(super) fn collision_benchmark_point(point: [f32; 3]) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("CollisionPointInput"),
        fields: vec![(SmolStr::new("point"), KernelValue::Vec3(point))],
    })
}

pub(super) fn collision_benchmark_ray(origin: [f32; 3], direction: [f32; 3]) -> KernelValue {
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

pub(super) fn collision_benchmark_probe(center: [f32; 3], radius: f32) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("CollisionSphereProbe"),
        fields: vec![
            (SmolStr::new("center"), KernelValue::Vec3(center)),
            (SmolStr::new("radius"), KernelValue::F32(radius)),
        ],
    })
}

pub(super) fn collision_benchmark_sweep(
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

pub(super) fn collision_transition_probe_offset(step: u64) -> [f32; 3] {
    [
        (step % 9) as f32 * 0.04 - 0.16,
        ((step / 9) % 5) as f32 * 0.03 - 0.06,
        (step % 4) as f32 * -0.02,
    ]
}

fn collision_batch_chunk_size(ops: u64, backend: wrela::query_plan::DispatchBackend) -> usize {
    if matches!(backend, wrela::query_plan::DispatchBackend::Wgsl) {
        ops.clamp(1, 8192) as usize
    } else {
        ops.clamp(1, 128) as usize
    }
}

fn collision_transition_batch_chunk_size(
    ops: u64,
    backend: wrela::query_plan::DispatchBackend,
) -> usize {
    if matches!(backend, wrela::query_plan::DispatchBackend::Wgsl) {
        ops.clamp(1, 4096) as usize
    } else {
        ops.clamp(1, 32) as usize
    }
}

fn collision_batch_certification_policy(
    backend: wrela::query_plan::DispatchBackend,
) -> wrela::collision_plan::CollisionCertificationPolicy {
    if matches!(backend, wrela::query_plan::DispatchBackend::Wgsl) {
        wrela::collision_plan::CollisionCertificationPolicy::MetricsOnly
    } else {
        wrela::collision_plan::CollisionCertificationPolicy::CpuOracleParity
    }
}

fn build_collision_point_occupancy_batch(
    scenario: &test_eval_perf::BenchmarkScenario,
    plan: &wrela::collision_plan::CollisionPlan,
    scene_id: u32,
    domain: KernelValue,
) -> wrela::collision_plan::CollisionWorkloadBatch {
    let items = (1..=scenario.ops)
        .map(
            |i| wrela::collision_plan::CollisionBatchItem::PointOccupancy {
                point: [
                    (i % 16) as f32 * 0.08 - 0.60,
                    ((i / 16) % 10) as f32 * 0.06 - 0.24,
                    (i % 5) as f32 * 0.04 - 0.08,
                ],
            },
        )
        .collect::<Vec<_>>();
    wrela::collision_plan::CollisionWorkloadBatch::new(
        scenario.id.as_str(),
        "point_occupancy_burst",
        scenario.id.as_str(),
        plan.clone(),
        plan.contract_id,
        format!("collision:{scene_id}:1"),
        collision_benchmark_capture(scene_id, 1),
        domain,
        wrela::collision_plan::CollisionCandidateGroupingPolicy::SharedCandidateDigest,
        collision_batch_certification_policy(plan.backend),
        items,
        collision_batch_chunk_size(scenario.ops, plan.backend),
    )
}

fn build_collision_dense_ray_cast_batch(
    scenario: &test_eval_perf::BenchmarkScenario,
    plan: &wrela::collision_plan::CollisionPlan,
    scene_id: u32,
    domain: KernelValue,
) -> wrela::collision_plan::CollisionWorkloadBatch {
    let items = (1..=scenario.ops)
        .map(|i| {
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
            wrela::collision_plan::CollisionBatchItem::RayCast {
                ray: wrela::collision_contract::CollisionRayInput {
                    origin,
                    direction,
                    max_distance: 12.0,
                    min_step: 0.05,
                    hit_epsilon: 0.001,
                    max_steps: 96,
                },
            }
        })
        .collect::<Vec<_>>();
    wrela::collision_plan::CollisionWorkloadBatch::new(
        scenario.id.as_str(),
        "dense_ray_casts",
        scenario.id.as_str(),
        plan.clone(),
        plan.contract_id,
        format!("collision:{scene_id}:1"),
        collision_benchmark_capture(scene_id, 1),
        domain,
        wrela::collision_plan::CollisionCandidateGroupingPolicy::SharedCandidateDigest,
        collision_batch_certification_policy(plan.backend),
        items,
        collision_batch_chunk_size(scenario.ops, plan.backend),
    )
}

fn build_collision_overlap_batch(
    scenario: &test_eval_perf::BenchmarkScenario,
    plan: &wrela::collision_plan::CollisionPlan,
    scene_id: u32,
    domain: KernelValue,
) -> wrela::collision_plan::CollisionWorkloadBatch {
    let items = (1..=scenario.ops)
        .map(|i| {
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
            wrela::collision_plan::CollisionBatchItem::SphereOverlap { center, radius }
        })
        .collect::<Vec<_>>();
    wrela::collision_plan::CollisionWorkloadBatch::new(
        scenario.id.as_str(),
        "overlap_burst",
        scenario.id.as_str(),
        plan.clone(),
        plan.contract_id,
        format!("collision:{scene_id}:1"),
        collision_benchmark_capture(scene_id, 1),
        domain,
        wrela::collision_plan::CollisionCandidateGroupingPolicy::SharedCandidateDigest,
        collision_batch_certification_policy(plan.backend),
        items,
        collision_batch_chunk_size(scenario.ops, plan.backend),
    )
}

fn build_collision_repeated_sweeps_batch(
    scenario: &test_eval_perf::BenchmarkScenario,
    plan: &wrela::collision_plan::CollisionPlan,
    scene_id: u32,
    domain: KernelValue,
) -> wrela::collision_plan::CollisionWorkloadBatch {
    let current_epoch = 2;
    let previous_epoch = 1;
    let archetype_count = scenario.ops.clamp(1, 8);
    let items = (1..=scenario.ops)
        .map(|i| {
            let offset = collision_transition_probe_offset(((i - 1) % archetype_count) + 1);
            let start_center = [offset[0], offset[1], 2.9 + offset[2]];
            let end_center = [offset[0] + 0.05, offset[1] - 0.03, -1.1 + offset[2]];
            wrela::collision_plan::CollisionBatchItem::SphereSweep {
                transition: wrela::collision_contract::CollisionSnapshotTransitionInput {
                    current_snapshot_epoch: current_epoch,
                    previous_snapshot_epoch: previous_epoch,
                    change_class: wrela::state_advance::ChangeClass::Presentation,
                },
                sweep: wrela::collision_contract::CollisionSphereSweepInput {
                    start_center,
                    end_center,
                    radius: 0.25,
                    contact_tolerance: 0.001,
                    max_iterations: 64,
                },
            }
        })
        .collect::<Vec<_>>();
    wrela::collision_plan::CollisionWorkloadBatch::new(
        scenario.id.as_str(),
        "repeated_sweeps",
        scenario.id.as_str(),
        plan.clone(),
        plan.contract_id,
        format!("collision:{scene_id}:transition"),
        collision_benchmark_capture(scene_id, current_epoch),
        domain,
        wrela::collision_plan::CollisionCandidateGroupingPolicy::SharedBroadphaseRegion,
        collision_batch_certification_policy(plan.backend),
        items,
        collision_transition_batch_chunk_size(scenario.ops, plan.backend),
    )
}

fn build_collision_toi_batch(
    scenario: &test_eval_perf::BenchmarkScenario,
    plan: &wrela::collision_plan::CollisionPlan,
    scene_id: u32,
    domain: KernelValue,
) -> wrela::collision_plan::CollisionWorkloadBatch {
    let current_epoch = 2;
    let previous_epoch = 1;
    let archetype_count = scenario.ops.clamp(1, 8);
    let items = (1..=scenario.ops)
        .map(|i| {
            let offset = collision_transition_probe_offset(((i - 1) % archetype_count) + 1);
            let start_center = [offset[0], offset[1], 2.4 + offset[2]];
            let end_center = [offset[0] + 0.04, offset[1] - 0.02, -0.9 + offset[2]];
            wrela::collision_plan::CollisionBatchItem::SphereTimeOfImpact {
                transition: wrela::collision_contract::CollisionSnapshotTransitionInput {
                    current_snapshot_epoch: current_epoch,
                    previous_snapshot_epoch: previous_epoch,
                    change_class: wrela::state_advance::ChangeClass::Presentation,
                },
                sweep: wrela::collision_contract::CollisionSphereSweepInput {
                    start_center,
                    end_center,
                    radius: 0.20,
                    contact_tolerance: 0.001,
                    max_iterations: 64,
                },
            }
        })
        .collect::<Vec<_>>();
    wrela::collision_plan::CollisionWorkloadBatch::new(
        scenario.id.as_str(),
        "toi_transition_reuse",
        scenario.id.as_str(),
        plan.clone(),
        plan.contract_id,
        format!("collision:{scene_id}:transition"),
        collision_benchmark_capture(scene_id, current_epoch),
        domain,
        wrela::collision_plan::CollisionCandidateGroupingPolicy::SharedBroadphaseRegion,
        collision_batch_certification_policy(plan.backend),
        items,
        collision_transition_batch_chunk_size(scenario.ops, plan.backend),
    )
}

fn run_collision_point_occupancy_burst(
    ctx: &QueryExecContext,
    scenario: &test_eval_perf::BenchmarkScenario,
    scene_id: u32,
    domain: KernelValue,
    backend: wrela::query_plan::DispatchBackend,
) -> Result<CollisionBenchmarkScenarioResult, String> {
    let plan = wrela::collision_plan::CollisionPlan::for_query_with_backend(
        wrela::collision_plan::CollisionQueryKind::PointOccupancyWorld,
        backend,
    );
    let batch = build_collision_point_occupancy_batch(scenario, &plan, scene_id, domain);
    let started = Instant::now();
    let report = if matches!(backend, wrela::query_plan::DispatchBackend::Wgsl) {
        wrela::collision_exec::execute_batch_metrics_only(&batch, ctx)
    } else {
        wrela::collision_exec::execute_batch(&batch, ctx, None).map(|result| result.report)
    }
    .map_err(|err| format!("collision benchmark `{}` failed: {err}", scenario.id))?;
    let metrics = collision_metrics_from_batch_report(started.elapsed().as_nanos(), &report);
    Ok(build_collision_benchmark_execution(
        &scenario.id,
        &plan,
        metrics,
    ))
}

fn run_collision_dense_ray_casts(
    ctx: &QueryExecContext,
    scenario: &test_eval_perf::BenchmarkScenario,
    scene_id: u32,
    domain: KernelValue,
    backend: wrela::query_plan::DispatchBackend,
) -> Result<CollisionBenchmarkScenarioResult, String> {
    let plan = wrela::collision_plan::CollisionPlan::for_query_with_backend(
        wrela::collision_plan::CollisionQueryKind::RayCastWorld,
        backend,
    );
    let batch = build_collision_dense_ray_cast_batch(scenario, &plan, scene_id, domain);
    let started = Instant::now();
    let report = if matches!(backend, wrela::query_plan::DispatchBackend::Wgsl) {
        wrela::collision_exec::execute_batch_metrics_only(&batch, ctx)
    } else {
        wrela::collision_exec::execute_batch(&batch, ctx, None).map(|result| result.report)
    }
    .map_err(|err| format!("collision benchmark `{}` failed: {err}", scenario.id))?;
    let metrics = collision_metrics_from_batch_report(started.elapsed().as_nanos(), &report);
    Ok(build_collision_benchmark_execution(
        &scenario.id,
        &plan,
        metrics,
    ))
}

fn run_collision_overlap_burst(
    ctx: &QueryExecContext,
    scenario: &test_eval_perf::BenchmarkScenario,
    scene_id: u32,
    domain: KernelValue,
    backend: wrela::query_plan::DispatchBackend,
) -> Result<CollisionBenchmarkScenarioResult, String> {
    let plan = wrela::collision_plan::CollisionPlan::for_query_with_backend(
        wrela::collision_plan::CollisionQueryKind::SphereOverlapWorld,
        backend,
    );
    let batch = build_collision_overlap_batch(scenario, &plan, scene_id, domain);
    let started = Instant::now();
    let report = if matches!(backend, wrela::query_plan::DispatchBackend::Wgsl) {
        wrela::collision_exec::execute_batch_metrics_only(&batch, ctx)
    } else {
        wrela::collision_exec::execute_batch(&batch, ctx, None).map(|result| result.report)
    }
    .map_err(|err| format!("collision benchmark `{}` failed: {err}", scenario.id))?;
    let metrics = collision_metrics_from_batch_report(started.elapsed().as_nanos(), &report);
    Ok(build_collision_benchmark_execution(
        &scenario.id,
        &plan,
        metrics,
    ))
}

fn run_collision_repeated_sweeps(
    ctx: &QueryExecContext,
    scenario: &test_eval_perf::BenchmarkScenario,
    scene_id: u32,
    domain: KernelValue,
    backend: wrela::query_plan::DispatchBackend,
) -> Result<CollisionBenchmarkScenarioResult, String> {
    let plan = wrela::collision_plan::CollisionPlan::for_query_with_backend(
        wrela::collision_plan::CollisionQueryKind::SphereSweepTransition,
        backend,
    );
    let batch = build_collision_repeated_sweeps_batch(scenario, &plan, scene_id, domain);
    let started = Instant::now();
    let report = if matches!(backend, wrela::query_plan::DispatchBackend::Wgsl) {
        wrela::collision_exec::execute_batch_metrics_only(&batch, ctx)
    } else {
        wrela::collision_exec::execute_batch(&batch, ctx, None).map(|result| result.report)
    }
    .map_err(|err| format!("collision benchmark `{}` failed: {err}", scenario.id))?;
    let metrics = collision_metrics_from_batch_report(started.elapsed().as_nanos(), &report);
    Ok(build_collision_benchmark_execution(
        &scenario.id,
        &plan,
        metrics,
    ))
}

fn run_collision_toi_transition_reuse(
    ctx: &QueryExecContext,
    scenario: &test_eval_perf::BenchmarkScenario,
    scene_id: u32,
    domain: KernelValue,
    backend: wrela::query_plan::DispatchBackend,
) -> Result<CollisionBenchmarkScenarioResult, String> {
    let plan = wrela::collision_plan::CollisionPlan::for_query_with_backend(
        wrela::collision_plan::CollisionQueryKind::SphereTimeOfImpactTransition,
        backend,
    );
    let batch = build_collision_toi_batch(scenario, &plan, scene_id, domain);
    let started = Instant::now();
    let report = if matches!(backend, wrela::query_plan::DispatchBackend::Wgsl) {
        wrela::collision_exec::execute_batch_metrics_only(&batch, ctx)
    } else {
        wrela::collision_exec::execute_batch(&batch, ctx, None).map(|result| result.report)
    }
    .map_err(|err| format!("collision benchmark `{}` failed: {err}", scenario.id))?;
    let metrics = collision_metrics_from_batch_report(started.elapsed().as_nanos(), &report);
    Ok(build_collision_benchmark_execution(
        &scenario.id,
        &plan,
        metrics,
    ))
}

pub(super) fn collision_runtime_cases_from_report(
    report: &test_eval_perf::CollisionBenchmarkReport,
) -> Vec<(String, test_eval_perf::PerfScenarioId, u128)> {
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

pub(super) fn collision_runtime_cases_by_scenario_id(
    report: &test_eval_perf::CollisionBenchmarkReport,
) -> Vec<(
    test_eval_perf::PerfScenarioId,
    test_eval_perf::PerfScenarioId,
    u128,
)> {
    report
        .executions
        .iter()
        .map(|execution| {
            (
                execution.name.clone(),
                execution.name.clone(),
                execution.runtime_ns,
            )
        })
        .collect()
}

pub(super) fn build_whole_frame_benchmark_reports(
    presentation_reports: &[PresentationBenchmarkReport],
    collision_reports: &[CollisionBenchmarkReport],
) -> Result<Vec<WholeFrameBenchmarkReport>, String> {
    let mut collision_by_scenario = HashMap::new();
    // Invariant: whole-frame evidence joins by typed scenario identity, not by
    // manifest order. Presentation and collision collection can filter or
    // reorder independently once lane selection and error handling kick in.
    for report in collision_reports {
        for execution in &report.executions {
            if collision_by_scenario
                .insert(execution.name.clone(), execution)
                .is_some()
            {
                return Err(format!(
                    "whole-frame report join saw duplicate collision execution for scenario '{}'",
                    execution.name
                ));
            }
        }
    }

    let mut reports = Vec::with_capacity(presentation_reports.len());
    for presentation in presentation_reports {
        let Some(collision) = collision_by_scenario.remove(&presentation.scenario_id) else {
            return Err(format!(
                "whole-frame report join missing collision execution for scenario '{}'",
                presentation.scenario_id
            ));
        };
        let total_runtime_ns = presentation
            .frame_time_ns
            .saturating_add(collision.runtime_ns);
        reports.push(WholeFrameBenchmarkReport {
            scenario_id: presentation.scenario_id.clone(),
            test_name: presentation.test_name.clone(),
            presentation_frame_time_ns: presentation.frame_time_ns,
            collision_runtime_ns: collision.runtime_ns,
            total_runtime_ns,
            steady_state_fps: fps_from_frame_time_ns(
                total_runtime_ns,
                presentation.frames_executed.max(1) as usize,
            ),
            presentation_bottleneck_pass: presentation.bottleneck_pass.clone(),
            collision_fallback_rate: collision.fallback_rate,
            collision_witness_reuse_rate: collision.witness_reuse_rate,
        });
    }

    if let Some(extra) = collision_by_scenario.keys().next() {
        return Err(format!(
            "whole-frame report join found collision execution '{}' without a matching presentation scenario",
            extra
        ));
    }

    Ok(reports)
}

#[derive(Clone)]
struct PerfEngineFrameSubsystem {
    descriptor: wrela::engine_frame::EngineSubsystemDescriptor,
    report: wrela::engine_frame::EngineSubsystemReport,
    active_degradations: Vec<String>,
    violations: Vec<String>,
}

impl wrela::engine_frame::EngineSubsystemWork for PerfEngineFrameSubsystem {
    fn descriptor(&self) -> wrela::engine_frame::EngineSubsystemDescriptor {
        self.descriptor.clone()
    }

    fn prepare(
        &mut self,
        _ctx: &mut wrela::engine_frame::EngineFrameContext,
    ) -> Result<(), wrela::engine_frame::EngineFrameError> {
        Ok(())
    }

    fn encode(
        &mut self,
        _ctx: &mut wrela::engine_frame::EngineFrameContext,
    ) -> Result<(), wrela::engine_frame::EngineFrameError> {
        Ok(())
    }

    fn finish(
        &mut self,
        ctx: &mut wrela::engine_frame::EngineFrameContext,
    ) -> Result<wrela::engine_frame::EngineSubsystemReport, wrela::engine_frame::EngineFrameError>
    {
        extend_unique_strings(
            &mut ctx.active_degradations,
            self.active_degradations.iter().cloned(),
        );
        extend_unique_strings(&mut ctx.violations, self.violations.iter().cloned());
        Ok(self.report.clone())
    }
}

fn extend_unique_strings(target: &mut Vec<String>, values: impl IntoIterator<Item = String>) {
    for value in values {
        if !target.contains(&value) {
            target.push(value);
        }
    }
}

fn ns_to_micros(value: u128) -> u128 {
    value / 1_000
}

pub(super) fn build_engine_frame_benchmark_reports(
    presentation_reports: &[PresentationBenchmarkReport],
    collision_reports: &[CollisionBenchmarkReport],
    engine_frame_budget: Option<&wrela::perf_target::PerfClosureEngineFrameBudget>,
) -> Result<Vec<EngineFrameBenchmarkReport>, String> {
    let mut collision_by_scenario = HashMap::new();
    for report in collision_reports {
        for execution in &report.executions {
            if collision_by_scenario
                .insert(execution.name.clone(), (report.backend.clone(), execution))
                .is_some()
            {
                return Err(format!(
                    "engine-frame report join saw duplicate collision execution for scenario '{}'",
                    execution.name
                ));
            }
        }
    }

    let mut reports = Vec::with_capacity(presentation_reports.len());
    for presentation in presentation_reports {
        let Some((collision_backend, collision)) =
            collision_by_scenario.remove(&presentation.scenario_id)
        else {
            return Err(format!(
                "engine-frame report join missing collision execution for scenario '{}'",
                presentation.scenario_id
            ));
        };
        let presentation_frame_count = u128::from(presentation.frames_executed.max(1));
        let presentation_runtime_micros = ns_to_micros(
            presentation
                .frame_time_ns
                .checked_div(presentation_frame_count)
                .unwrap_or(presentation.frame_time_ns),
        );
        let presentation_frames =
            presentation_frame_history(&presentation.frame_cost, &presentation.frame_cost_history);
        let presentation_gpu_critical_path_micros = presentation_frames
            .iter()
            .filter_map(|frame| {
                (frame.gpu_runtime.gpu_time_total_micros > 0)
                    .then_some(frame.gpu_runtime.gpu_time_total_micros)
            })
            .max();
        let presentation_queue_submit_count = presentation_frames
            .iter()
            .map(|frame| frame.gpu_runtime.queue_submit_count)
            .max()
            .unwrap_or(0);
        let presentation_hot_path_readback_bytes = presentation_frames
            .iter()
            .map(|frame| {
                hot_path_readback_bytes_without_timestamp_traffic(
                    frame.gpu_runtime.readback_bytes,
                    frame.gpu_runtime.timestamps_supported,
                    frame.gpu_runtime.timestamped_pass_count,
                )
            })
            .max()
            .unwrap_or(0);
        let presentation_scene_reupload_bytes = presentation_frames
            .iter()
            .map(|frame| frame.gpu_runtime.scene_reupload_bytes)
            .max()
            .unwrap_or(0);
        let state_advance_notes = if engine_frame_budget.is_some() {
            vec![
                "reserved-slot-unsampled".to_string(),
                "scheduler-adapter".to_string(),
            ]
        } else {
            vec![
                "compatibility-slot-unsampled".to_string(),
                "scheduler-adapter".to_string(),
            ]
        };
        let collision_uses_gpu = collision_backend.eq_ignore_ascii_case("wgsl")
            || collision.wgsl_dispatch_count > 0
            || collision.queue_submit_count > 0;
        let collision_has_timestamped_gpu_measurement =
            collision.timestamps_supported && collision.timestamped_pass_count > 0;
        let collision_gpu_critical_path_micros = if collision_has_timestamped_gpu_measurement {
            Some(ns_to_micros(collision.gpu_time_total_ns))
        } else {
            collision_uses_gpu.then(|| ns_to_micros(collision.runtime_ns))
        };
        let mut collision_notes = vec![
            "scheduler-adapter".to_string(),
            format!("batch_count={}", collision.batch_count),
            format!("dispatch_count={}", collision.dispatch_count),
            format!(
                "average_items_per_dispatch={:.2}",
                collision.average_items_per_dispatch
            ),
            format!(
                "cpu_certification_query_count={}",
                collision.cpu_certification_query_count
            ),
            format!(
                "candidate_table_overflow_fallback_count={}",
                collision.candidate_table_overflow_fallback_count
            ),
        ];
        if collision_has_timestamped_gpu_measurement {
            collision_notes.push(format!(
                "gpu_timestamped_pass_count={}",
                collision.timestamped_pass_count
            ));
        } else if collision_gpu_critical_path_micros.unwrap_or(0) > 0 {
            collision_notes.push("gpu_critical_path_proxy=runtime_ns".to_string());
        }
        let mut subsystems: Vec<Box<dyn wrela::engine_frame::EngineSubsystemWork>> = vec![
            Box::new(PerfEngineFrameSubsystem {
                descriptor: wrela::engine_frame::EngineSubsystemDescriptor {
                    kind: wrela::engine_frame::EngineSubsystemKind::StateAdvance,
                    label: "state_advance".to_string(),
                    runs_after: vec![],
                    requires_gpu: false,
                    allows_hot_path_readback: false,
                },
                report: wrela::engine_frame::EngineSubsystemReport {
                    kind: wrela::engine_frame::EngineSubsystemKind::StateAdvance,
                    label: "state_advance".into(),
                    work_items: 0,
                    cpu_critical_path_micros: 0,
                    gpu_critical_path_micros: None,
                    queue_submit_count: 0,
                    hot_path_readback_bytes: 0,
                    scene_reupload_bytes: 0,
                    wait_time_micros: 0,
                    notes: state_advance_notes,
                },
                active_degradations: Vec::new(),
                violations: Vec::new(),
            }),
            Box::new(PerfEngineFrameSubsystem {
                descriptor: wrela::engine_frame::EngineSubsystemDescriptor {
                    kind: wrela::engine_frame::EngineSubsystemKind::Presentation,
                    label: "presentation".to_string(),
                    runs_after: vec![wrela::engine_frame::EngineSubsystemKind::StateAdvance],
                    requires_gpu: true,
                    allows_hot_path_readback: false,
                },
                report: wrela::engine_frame::EngineSubsystemReport {
                    kind: wrela::engine_frame::EngineSubsystemKind::Presentation,
                    label: "presentation".into(),
                    work_items: u64::from(presentation.frames_executed.max(1)),
                    cpu_critical_path_micros: presentation_runtime_micros,
                    gpu_critical_path_micros: presentation_gpu_critical_path_micros,
                    queue_submit_count: presentation_queue_submit_count,
                    hot_path_readback_bytes: presentation_hot_path_readback_bytes,
                    scene_reupload_bytes: presentation_scene_reupload_bytes,
                    wait_time_micros: 0,
                    notes: vec!["scheduler-adapter".to_string()],
                },
                active_degradations: presentation.frame_cost.quality.active_degradations.clone(),
                violations: Vec::new(),
            }),
            Box::new(PerfEngineFrameSubsystem {
                descriptor: wrela::engine_frame::EngineSubsystemDescriptor {
                    kind: wrela::engine_frame::EngineSubsystemKind::Collision,
                    label: "collision".to_string(),
                    runs_after: vec![wrela::engine_frame::EngineSubsystemKind::Presentation],
                    requires_gpu: collision_uses_gpu,
                    allows_hot_path_readback: false,
                },
                report: wrela::engine_frame::EngineSubsystemReport {
                    kind: wrela::engine_frame::EngineSubsystemKind::Collision,
                    label: "collision".into(),
                    work_items: collision.query_count,
                    cpu_critical_path_micros: ns_to_micros(collision.runtime_ns),
                    gpu_critical_path_micros: collision_gpu_critical_path_micros,
                    queue_submit_count: collision.queue_submit_count,
                    hot_path_readback_bytes: collision.hot_path_readback_bytes,
                    scene_reupload_bytes: collision.scene_reupload_bytes,
                    wait_time_micros: 0,
                    notes: collision_notes,
                },
                active_degradations: Vec::new(),
                violations: Vec::new(),
            }),
        ];
        let mut scheduler = wrela::engine_frame::EngineFrameScheduler {
            budget: engine_frame_budget.cloned(),
        };
        let scheduler_report = scheduler
            .run_frame(presentation.scenario_id.to_string(), 0, &mut subsystems)
            .map_err(|err| {
                format!(
                    "engine-frame scheduler failed for scenario '{}': {err}",
                    presentation.scenario_id
                )
            })?;
        let state_advance_runtime_ns = scheduler_report
            .subsystem(wrela::engine_frame::EngineSubsystemKind::StateAdvance)
            .map(|report| report.cpu_critical_path_micros.saturating_mul(1_000))
            .unwrap_or(0);
        let presentation_runtime_ns = scheduler_report
            .subsystem(wrela::engine_frame::EngineSubsystemKind::Presentation)
            .map(|report| report.cpu_critical_path_micros.saturating_mul(1_000))
            .unwrap_or(presentation.frame_time_ns);
        let collision_runtime_ns = scheduler_report
            .subsystem(wrela::engine_frame::EngineSubsystemKind::Collision)
            .map(|report| report.cpu_critical_path_micros.saturating_mul(1_000))
            .unwrap_or(collision.runtime_ns);
        reports.push(EngineFrameBenchmarkReport {
            scenario_id: presentation.scenario_id.clone(),
            test_name: presentation.test_name.clone(),
            frame_count: presentation.frames_executed.max(1),
            frame_wall_time_ns: scheduler_report
                .frame_wall_time_micros
                .saturating_mul(1_000),
            cpu_critical_path_ns: scheduler_report
                .cpu_critical_path_micros
                .saturating_mul(1_000),
            gpu_critical_path_ns: scheduler_report
                .gpu_critical_path_micros
                .map(|value| value.saturating_mul(1_000)),
            present_wait_ns: scheduler_report.present_wait_micros.saturating_mul(1_000),
            readback_wait_ns: scheduler_report.readback_wait_micros.saturating_mul(1_000),
            steady_state_fps: fps_from_frame_time_ns(
                scheduler_report
                    .frame_wall_time_micros
                    .saturating_mul(1_000),
                1,
            ),
            presentation_runtime_ns,
            collision_runtime_ns,
            state_advance_runtime_ns,
            future_subsystem_reserve_ns: (scheduler_report.future_subsystem_reserve.reserved_micros
                as i128
                + scheduler_report.future_subsystem_reserve.remaining_micros)
                .max(0) as u128
                * 1_000,
            queue_submit_count: scheduler_report.gpu_runtime.queue_submit_count,
            hot_path_readback_bytes: scheduler_report.gpu_runtime.readback_bytes,
            scene_reupload_bytes: scheduler_report.gpu_runtime.scene_reupload_bytes,
            active_degradations: scheduler_report.active_degradations,
            violations: scheduler_report.violations,
            subsystem_reports: scheduler_report.subsystems,
        });
    }

    if let Some(extra) = collision_by_scenario.keys().next() {
        return Err(format!(
            "engine-frame report join found collision execution '{}' without a matching presentation scenario",
            extra
        ));
    }

    Ok(reports)
}

pub(super) fn whole_frame_runtime_cases_from_reports(
    reports: &[WholeFrameBenchmarkReport],
) -> Vec<(test_eval_perf::PerfScenarioId, String, u128)> {
    reports
        .iter()
        .map(|report| {
            (
                report.scenario_id.clone(),
                report.test_name.clone(),
                report.total_runtime_ns,
            )
        })
        .collect()
}

pub(super) fn engine_frame_runtime_cases_from_reports(
    reports: &[EngineFrameBenchmarkReport],
) -> Vec<(test_eval_perf::PerfScenarioId, String, u128)> {
    reports
        .iter()
        .map(|report| {
            (
                report.scenario_id.clone(),
                report.test_name.clone(),
                report.frame_wall_time_ns,
            )
        })
        .collect()
}

pub(super) fn run_presentation_benchmark_report(
    current_exe: &Path,
    benchmark_root: &Path,
    scenario: &test_eval_perf::BenchmarkScenario,
    spec: &test_eval_perf::BenchmarkPresentationSpec,
    query_backend: wrela::query_plan::DispatchBackend,
    collection_mode: PresentationBenchmarkCollectionMode,
) -> Result<PresentationBenchmarkReport, String> {
    let warm_quality_pipelines = should_warm_closure_quality_pipelines(scenario);
    if matches!(
        collection_mode,
        PresentationBenchmarkCollectionMode::Measurement
    ) {
        return run_presentation_benchmark_measurement_report(
            current_exe,
            benchmark_root,
            scenario,
            spec,
            query_backend,
            warm_quality_pipelines,
        );
    }
    if !matches!(query_backend, wrela::query_plan::DispatchBackend::Wgsl) {
        let hybrid = run_presentation_benchmark_report_for_mode(
            current_exe,
            benchmark_root,
            scenario,
            spec,
            QueryTraceSolverMode::Hybrid,
            None,
            query_backend,
            warm_quality_pipelines,
        )?;
        let dense_only = run_presentation_benchmark_report_for_mode(
            current_exe,
            benchmark_root,
            scenario,
            spec,
            QueryTraceSolverMode::DenseOnly,
            None,
            query_backend,
            warm_quality_pipelines,
        )?;
        let mut report = presentation_report_from_debug_output(scenario, hybrid)?;
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
        warm_quality_pipelines,
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
        warm_quality_pipelines,
    )?;
    let mut report = best_hybrid;
    report.wgsl_workgroup_comparison = Some(workgroup_comparison);
    report.ab_comparison = Some(presentation_comparison_from_debug_reports(
        &report,
        &dense_only,
    ));
    Ok(report)
}

pub(super) fn run_presentation_benchmark_measurement_report(
    current_exe: &Path,
    benchmark_root: &Path,
    scenario: &test_eval_perf::BenchmarkScenario,
    spec: &test_eval_perf::BenchmarkPresentationSpec,
    query_backend: wrela::query_plan::DispatchBackend,
    warm_quality_pipelines: bool,
) -> Result<PresentationBenchmarkReport, String> {
    let dump = run_presentation_benchmark_report_for_mode(
        current_exe,
        benchmark_root,
        scenario,
        spec,
        QueryTraceSolverMode::Hybrid,
        None,
        query_backend,
        warm_quality_pipelines,
    )?;
    presentation_report_from_debug_output(scenario, dump)
}

pub(super) fn run_presentation_benchmark_reports_for_workgroup_sizes(
    current_exe: &Path,
    benchmark_root: &Path,
    scenario: &test_eval_perf::BenchmarkScenario,
    spec: &test_eval_perf::BenchmarkPresentationSpec,
    query_trace_solver_mode: QueryTraceSolverMode,
    query_backend: wrela::query_plan::DispatchBackend,
    warm_quality_pipelines: bool,
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
            warm_quality_pipelines,
        )?;
        let report = presentation_report_from_debug_output(scenario, dump)?;
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

pub(super) fn run_presentation_benchmark_report_for_mode(
    current_exe: &Path,
    benchmark_root: &Path,
    scenario: &test_eval_perf::BenchmarkScenario,
    spec: &test_eval_perf::BenchmarkPresentationSpec,
    query_trace_solver_mode: QueryTraceSolverMode,
    workgroup_size_override: Option<u32>,
    query_backend: wrela::query_plan::DispatchBackend,
    warm_quality_pipelines: bool,
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
        warm_quality_pipelines,
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

pub(super) fn build_presentation_debug_command(
    current_exe: &Path,
    presentation_target: &Path,
    spec: &test_eval_perf::BenchmarkPresentationSpec,
    query_trace_solver_mode: QueryTraceSolverMode,
    workgroup_size_override: Option<u32>,
    query_backend: wrela::query_plan::DispatchBackend,
    warm_quality_pipelines: bool,
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
    if warm_quality_pipelines {
        command.env(WRELA_PRESENTATION_DEBUG_WARM_QUALITY_PIPELINES_ENV, "1");
        command.env(WRELA_PRESENTATION_DEBUG_ADAPTIVE_WINDOW_ENV, "1");
    } else {
        command.env_remove(WRELA_PRESENTATION_DEBUG_WARM_QUALITY_PIPELINES_ENV);
        command.env_remove(WRELA_PRESENTATION_DEBUG_ADAPTIVE_WINDOW_ENV);
    }
    command
}

pub(super) fn run_command_with_timeout(
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
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "failed to capture command stdout".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "failed to capture command stderr".to_string())?;
    let stdout_reader = thread::spawn(move || read_command_stream(stdout));
    let stderr_reader = thread::spawn(move || read_command_stream(stderr));
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let stdout = join_command_stream(stdout_reader, "stdout")?;
                let stderr = join_command_stream(stderr_reader, "stderr")?;
                return Ok(Output {
                    status,
                    stdout,
                    stderr,
                });
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
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            return Err(format!("command timed out after {:?}", timeout));
        }
        thread::sleep(Duration::from_millis(25));
    }
}

pub(super) fn read_command_stream<R: Read>(mut stream: R) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes)?;
    Ok(bytes)
}

pub(super) fn join_command_stream(
    handle: thread::JoinHandle<std::io::Result<Vec<u8>>>,
    stream_name: &str,
) -> Result<Vec<u8>, String> {
    match handle.join() {
        Ok(Ok(bytes)) => Ok(bytes),
        Ok(Err(err)) => Err(format!("failed to read command {stream_name}: {err}")),
        Err(_) => Err(format!("command {stream_name} reader thread panicked")),
    }
}

pub(super) fn presentation_report_from_debug_output(
    scenario: &test_eval_perf::BenchmarkScenario,
    dump: PresentationDebugCommandOutput,
) -> Result<PresentationBenchmarkReport, String> {
    let measured_history = if scenario.class.is_closure() {
        closure_measured_presentation_frame_history(&dump.frame_cost, &dump.frame_cost_history)
    } else {
        Ok(presentation_frame_history(
            &dump.frame_cost,
            &dump.frame_cost_history,
        ))
    };
    let measured_history = measured_history?;
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
    Ok(PresentationBenchmarkReport {
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
        steady_state_fps: fps_from_frame_time_ns(aggregate.frame_time_ns, measured_frames_executed),
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
    })
}

pub(super) fn observed_wgsl_adapter_name() -> Option<String> {
    shared_wgpu_context(GpuLimitRequest::default())
        .ok()
        .map(|context| context.adapter_info.name.trim().to_string())
        .filter(|name| !name.is_empty())
}

pub(super) fn presentation_comparison_from_debug_reports(
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

pub(super) fn presentation_workgroup_comparison_from_reports(
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

pub(super) fn presentation_frame_history(
    frame_cost: &wrela::presentation_exec::PresentationFrameCostReport,
    frame_cost_history: &[wrela::presentation_exec::PresentationFrameCostReport],
) -> Vec<wrela::presentation_exec::PresentationFrameCostReport> {
    if frame_cost_history.is_empty() {
        vec![frame_cost.clone()]
    } else {
        frame_cost_history.to_vec()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PresentationQualitySignature {
    tier: String,
    radiance_mode: String,
    media_enabled: bool,
    half_res_participants: bool,
    hit_compaction_enabled: bool,
    internal_resolution_scale_millis: u32,
    active_degradations: Vec<String>,
}

fn presentation_quality_signature(
    frame_cost: &wrela::presentation_exec::PresentationFrameCostReport,
) -> PresentationQualitySignature {
    PresentationQualitySignature {
        tier: frame_cost.quality.tier.clone(),
        radiance_mode: frame_cost.quality.radiance_mode.clone(),
        media_enabled: frame_cost.quality.media_enabled,
        half_res_participants: frame_cost.quality.half_res_participants,
        hit_compaction_enabled: frame_cost.quality.hit_compaction_enabled,
        internal_resolution_scale_millis: (frame_cost.quality.internal_resolution_scale * 1000.0)
            .round()
            .max(0.0) as u32,
        active_degradations: frame_cost.quality.active_degradations.clone(),
    }
}

pub(super) fn closure_measured_presentation_frame_history(
    frame_cost: &wrela::presentation_exec::PresentationFrameCostReport,
    frame_cost_history: &[wrela::presentation_exec::PresentationFrameCostReport],
) -> Result<Vec<wrela::presentation_exec::PresentationFrameCostReport>, String> {
    let frame_history = presentation_frame_history(frame_cost, frame_cost_history);
    if frame_history.is_empty() {
        return Err("presentation-debug produced no frame history".to_string());
    }
    let last_frame = frame_history
        .last()
        .expect("non-empty frame history should have a last frame");
    if last_frame.gpu_runtime.pipeline_cache_misses > 0 {
        return Err(format!(
            "final closure frame ended with {} pipeline cache miss(es) while quality={} radiance={} half_res_participants={} scale={:.2}; increase scenario frames or warm the required quality pipelines before measurement",
            last_frame.gpu_runtime.pipeline_cache_misses,
            last_frame.quality.active_degradations.join("|"),
            last_frame.quality.radiance_mode,
            last_frame.quality.half_res_participants,
            last_frame.quality.internal_resolution_scale,
        ));
    }
    let trailing_signature = presentation_quality_signature(last_frame);
    let mut start = frame_history.len() - 1;
    while start > 0 {
        let candidate = &frame_history[start - 1];
        if candidate.gpu_runtime.pipeline_cache_misses == 0
            && presentation_quality_signature(candidate) == trailing_signature
        {
            start -= 1;
        } else {
            break;
        }
    }
    Ok(frame_history[start..].to_vec())
}

pub(super) fn aligned_presentation_frame_history(
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

pub(super) fn aggregate_presentation_solver_counters(
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

pub(super) fn presentation_debug_args(
    spec: &test_eval_perf::BenchmarkPresentationSpec,
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

pub(super) fn format_vec3(value: [f32; 3]) -> String {
    format!("{},{},{}", value[0], value[1], value[2])
}

pub(super) fn resolve_perf_benchmark_manifest_path(
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

fn should_skip_authored_composite_harness(
    whole_frame_benchmarks_active: bool,
    perf_profile: PerfProfile,
) -> bool {
    whole_frame_benchmarks_active && matches!(perf_profile, PerfProfile::Closure1080p120)
}

fn empty_perf_summary() -> test_eval_perf::PerfSummary {
    test_eval_perf::PerfSummary {
        sample_count: 0,
        compile_throughput_tests_per_sec: 0.0,
        runtime_p50_ns: 0,
        runtime_p95_ns: 0,
        runtime_p99_ns: 0,
        allocs_per_request: 0.0,
        rc_inc: 0,
        rc_dec: 0,
        rc_ops_total: 0,
        dispatch_hit_ratio: 1.0,
        check_fallback_rate: None,
        avg_check_batch_size: None,
        check_oracle_eval_ns_p50: None,
        check_oracle_eval_ns_p95: None,
        effect_annihilation_rewrite_count: None,
        scheduler_dispatch_p99_ns: None,
        scheduler_starvation_violations: None,
        rewrite_compile_overhead_pct: None,
        rewrite_applied_count: None,
        actor_msgs_per_sec_p50: None,
        actor_msgs_per_sec_p95: None,
        queue_enqueue_p99_ns: None,
        queue_dequeue_p99_ns: None,
        queue_age_p99_ns: None,
        mailbox_wake_coalesced_count: None,
        mailbox_rescue_wake_count: None,
        queue_cas_retry_total: None,
        cases: None,
        metrics: test_eval_perf::MetricsTotals::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wrela::collision_plan::CollisionBatchItem;

    fn sample_benchmark_scenario(id: &str, ops: u64) -> test_eval_perf::BenchmarkScenario {
        test_eval_perf::BenchmarkScenario {
            id: id.into(),
            test_name: format!("tests/whole_frame::{id}"),
            ops,
            class: test_eval_perf::BenchmarkScenarioClass::Closure,
            min_runtime_ms: None,
            timeout_ms: None,
            allow_unstable: false,
            presentation: None,
            collision: None,
        }
    }

    fn capture_epoch(value: &KernelValue) -> u32 {
        let KernelValue::Struct(value) = value else {
            panic!("expected capture struct, found {value:?}");
        };
        value
            .fields
            .iter()
            .find(|(name, _)| name.as_str() == "epoch")
            .and_then(|(_, value)| match value {
                KernelValue::U32(epoch) => Some(*epoch),
                _ => None,
            })
            .expect("capture epoch")
    }

    #[test]
    fn transition_batches_keep_item_epochs_aligned_with_batch_capture() {
        let sweep_plan = wrela::collision_plan::CollisionPlan::for_query_with_backend(
            wrela::collision_plan::CollisionQueryKind::SphereSweepTransition,
            wrela::query_plan::DispatchBackend::Wgsl,
        );
        let toi_plan = wrela::collision_plan::CollisionPlan::for_query_with_backend(
            wrela::collision_plan::CollisionQueryKind::SphereTimeOfImpactTransition,
            wrela::query_plan::DispatchBackend::Wgsl,
        );

        let sweep_batch = build_collision_repeated_sweeps_batch(
            &sample_benchmark_scenario("closure_sweeps", 4),
            &sweep_plan,
            7,
            KernelValue::Nothing,
        );
        let toi_batch = build_collision_toi_batch(
            &sample_benchmark_scenario("closure_toi", 4),
            &toi_plan,
            7,
            KernelValue::Nothing,
        );

        let sweep_capture_epoch = capture_epoch(&sweep_batch.capture);
        for item in &sweep_batch.items {
            let CollisionBatchItem::SphereSweep { transition, .. } = item else {
                panic!("expected sphere sweep item, found {item:?}");
            };
            assert_eq!(transition.current_snapshot_epoch, sweep_capture_epoch);
        }

        let toi_capture_epoch = capture_epoch(&toi_batch.capture);
        for item in &toi_batch.items {
            let CollisionBatchItem::SphereTimeOfImpact { transition, .. } = item else {
                panic!("expected toi item, found {item:?}");
            };
            assert_eq!(transition.current_snapshot_epoch, toi_capture_epoch);
        }
    }

    #[test]
    fn composite_closure_skips_authored_harness_only_for_1080p120() {
        assert!(should_skip_authored_composite_harness(
            true,
            PerfProfile::Closure1080p120
        ));
        assert!(!should_skip_authored_composite_harness(
            true,
            PerfProfile::Standard
        ));
        assert!(!should_skip_authored_composite_harness(
            false,
            PerfProfile::Closure1080p120
        ));
    }

    #[test]
    fn transition_reuse_batches_repeat_a_small_archetype_set() {
        let sweep_plan = wrela::collision_plan::CollisionPlan::for_query_with_backend(
            wrela::collision_plan::CollisionQueryKind::SphereSweepTransition,
            wrela::query_plan::DispatchBackend::Wgsl,
        );
        let toi_plan = wrela::collision_plan::CollisionPlan::for_query_with_backend(
            wrela::collision_plan::CollisionQueryKind::SphereTimeOfImpactTransition,
            wrela::query_plan::DispatchBackend::Wgsl,
        );
        let scenario = sample_benchmark_scenario("closure_transition_reuse", 32);

        let sweep_batch =
            build_collision_repeated_sweeps_batch(&scenario, &sweep_plan, 7, KernelValue::Nothing);
        let toi_batch = build_collision_toi_batch(&scenario, &toi_plan, 7, KernelValue::Nothing);

        let sweep_unique = sweep_batch
            .items
            .iter()
            .map(|item| format!("{item:?}"))
            .collect::<std::collections::BTreeSet<_>>();
        let toi_unique = toi_batch
            .items
            .iter()
            .map(|item| format!("{item:?}"))
            .collect::<std::collections::BTreeSet<_>>();

        assert!(sweep_unique.len() < sweep_batch.items.len());
        assert!(toi_unique.len() < toi_batch.items.len());
        assert_eq!(sweep_unique.len(), 8);
        assert_eq!(toi_unique.len(), 8);
    }
}
