use super::command_handlers::{
    self, DifferentialPipeline, KpiThresholds, PerfCmpConfig, PerfGateConfig, PerfProfile,
    PerfReport, TestSelection, TestTarget, budget_jobs_timeout, build_benchmark_selection,
    load_benchmark_manifest, resolve_benchmark_manifest_path, resolve_budget_policy_v1,
    resolve_test_target,
};
use super::contracts::{EXIT_CODEGEN, EXIT_OK, EXIT_USAGE, OutputFormat};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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
    pub(super) kpi_thresholds: KpiThresholds,
    pub(super) output_format: OutputFormat,
    pub(super) perf_debug: bool,
    pub(super) test_selection: TestSelection,
}

pub(super) fn execute_perf_command(mut input: PerfCommandInput) -> i32 {
    if input.trace {
        eprintln!("build: command perf");
    }
    if !input.program_args.is_empty() {
        eprintln!("error: unexpected extra arguments");
        return EXIT_USAGE;
    }
    let runs = input.perf_runs.unwrap_or(5).max(1);
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

    let manifest_path = resolve_benchmark_manifest_path(&target, input.benchmark_manifest_path);
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
        runs,
        cv_max_pct,
        &baseline_out,
        gate_cfg.as_ref(),
        &input.test_selection,
        runtime_only_cv_gate,
    )
}

pub(super) fn run_perf_harness(
    target: &TestTarget,
    budget_policy: &command_handlers::BudgetPolicyV1,
    jobs: usize,
    timeout: Duration,
    output_format: OutputFormat,
    perf_debug: bool,
    runs: usize,
    cv_max_pct: f64,
    baseline_out: &std::path::Path,
    perf_gate: Option<&PerfGateConfig>,
    selection: &TestSelection,
    runtime_only_cv_gate: bool,
) -> i32 {
    let mut samples = Vec::new();
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
            true,
            command_handlers::HttpCassetteMode::Replay,
            None,
            false,
            DifferentialPipeline::Baseline,
            None,
            None,
        );
        if exit != EXIT_OK {
            return exit;
        }
        if let Some(summary) = summary {
            samples.push(summary);
        }
    }
    if samples.is_empty() {
        eprintln!("perf harness error: no samples produced");
        return EXIT_CODEGEN;
    }
    let summary = command_handlers::aggregate_perf_samples(&samples);
    let cv = command_handlers::compute_cv(&samples);
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

    let report = PerfReport {
        version: 1,
        generated_at_unix_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::from_secs(0))
            .as_millis(),
        runs,
        cv,
        summary,
        samples,
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
    println!("perf baseline written: {}", baseline_out.display());
    EXIT_OK
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

    let manifest_path = match resolve_benchmark_manifest_path(
        &TestTarget::ProjectRoot(target_root.clone()),
        input.benchmark_manifest_path,
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
}
