//! Owns baseline-vs-candidate perf comparison runs, evidence normalization, and
//! rendered comparison reports.
//! Does not own baseline collection itself, closure verdict policy, or CLI
//! parsing.
//!
//! Key invariants:
//! - baseline and candidate samples are compared by typed scenario identity, not
//!   by display labels.
//! - warmup/measurement pairing must stay symmetric so effect-size and
//!   confidence calculations describe comparable work.
//! - rendered deltas must preserve the measured scenario ordering users use to
//!   audit regressions.
//!
//! Primary entrypoints:
//! - `execute_perfcmp_command`
//! - `run_perfcmp`
//! - `classify_perfcmp_verdict`
//!
//! Failure modes / common pitfalls:
//! - collapsing identity into strings too early makes duplicate scenario labels
//!   indistinguishable.
//! - comparing mismatched warmup/measurement sets produces precise-looking but
//!   meaningless effect sizes.

use super::*;

pub(crate) struct PerfcmpCommandInput {
    pub(crate) trace: bool,
    pub(crate) program_args: Vec<String>,
    pub(crate) path_arg: Option<String>,
    pub(crate) benchmark_manifest_path: Option<String>,
    pub(crate) perfcmp_baseline_ref: Option<String>,
    pub(crate) perfcmp_candidate_ref: Option<String>,
    pub(crate) out_path: Option<String>,
    pub(crate) output_format: OutputFormat,
    pub(crate) perf_profile: PerfProfile,
    pub(crate) perfcmp_warmup_pairs: Option<usize>,
    pub(crate) perfcmp_measure_pairs: Option<usize>,
    pub(crate) perfcmp_min_effect_pct: Option<f64>,
    pub(crate) perfcmp_confidence_pct: Option<f64>,
    pub(crate) test_timeout_ms: Option<u64>,
    pub(crate) perf_debug: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum PerfCmpVerdict {
    Win,
    Regression,
    NoSignal,
}

impl std::fmt::Display for PerfCmpVerdict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            Self::Win => "win",
            Self::Regression => "regression",
            Self::NoSignal => "no_signal",
        };
        f.write_str(value)
    }
}

pub(crate) fn execute_perfcmp_command(input: PerfcmpCommandInput) -> i32 {
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

    let mut by_scenario: HashMap<test_eval_perf::PerfScenarioId, Vec<(u128, u128)>> =
        HashMap::new();
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
            PerfCmpVerdict::Win => win_count += 1,
            PerfCmpVerdict::Regression => regression_count += 1,
            PerfCmpVerdict::NoSignal => no_signal_count += 1,
        }
        if !is_stable {
            unstable_count += 1;
            if scenario.class.is_critical() && !scenario.allow_unstable {
                unstable_critical_count += 1;
            }
        }
        scenario_results.push(PerfCmpScenarioResult {
            id: scenario.id.clone(),
            test_name: scenario.test_name.clone(),
            class: scenario.class,
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
            verdict,
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
                    if result.class.is_critical() && result.verdict == PerfCmpVerdict::Regression {
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
                    if result.is_stable && result.verdict == PerfCmpVerdict::Regression {
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

pub(super) fn profile_pair_counts(
    manifest: &test_eval_perf::BenchmarkManifest,
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
    summary: test_eval_perf::PerfSummary,
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

pub(super) fn effective_perfcmp_timeout_ms(
    scenarios: &[&test_eval_perf::BenchmarkScenario],
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

pub(super) fn cleanup_perfcmp_worktrees(
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

pub(super) fn command_stdout_trimmed(command: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(command).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if value.is_empty() { None } else { Some(value) }
}

pub(super) fn detect_cpu_model() -> String {
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

pub(super) fn detect_physical_cpu_count() -> Option<usize> {
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

pub(super) fn sanitize_git_ref_for_filename(git_ref: &str) -> String {
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

pub(super) fn pct_delta_higher_is_better(candidate: f64, baseline: f64) -> Option<f64> {
    if baseline == 0.0 {
        return None;
    }
    Some(((candidate - baseline) / baseline) * 100.0)
}

pub(super) fn pct_delta_lower_is_better(candidate: f64, baseline: f64) -> Option<f64> {
    if baseline == 0.0 {
        return None;
    }
    Some(((baseline - candidate) / baseline) * 100.0)
}

pub(super) fn compute_summary_metric_deltas_pct(
    baseline: &test_eval_perf::PerfSummary,
    candidate: &test_eval_perf::PerfSummary,
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

pub(super) fn should_skip_optional_suite(optional: bool, suite: &str, host_os: &str) -> bool {
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
    let summary = test_eval_perf::load_perf_baseline_summary(&report_path)?;
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

pub(super) fn git_worktree_add(
    repo_root: &Path,
    worktree: &Path,
    git_ref: &str,
) -> Result<(), String> {
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

pub(super) fn git_worktree_remove(repo_root: &Path, worktree: &Path) -> Result<(), String> {
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

pub(super) fn fnv1a64(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;
    let mut hash = OFFSET;
    for b in bytes {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

pub(super) fn random_bool(seed: &mut u64) -> bool {
    let mut x = *seed;
    x ^= x << 7;
    x ^= x >> 9;
    x ^= x << 8;
    *seed = x;
    (x & 1) == 0
}

pub(super) fn pct_delta_runtime(candidate: u128, baseline: u128) -> f64 {
    if baseline == 0 {
        return 0.0;
    }
    ((baseline as f64 - candidate as f64) / baseline as f64) * 100.0
}

pub(super) fn classify_perfcmp_verdict(
    ci_low_pct: f64,
    ci_high_pct: f64,
    min_effect_pct: f64,
) -> PerfCmpVerdict {
    if ci_low_pct > min_effect_pct {
        PerfCmpVerdict::Win
    } else if ci_high_pct < -min_effect_pct {
        PerfCmpVerdict::Regression
    } else {
        PerfCmpVerdict::NoSignal
    }
}

pub(super) fn median_u128_sorted(samples: &[u128]) -> Option<u128> {
    if samples.is_empty() {
        None
    } else {
        Some(samples[samples.len() / 2])
    }
}

pub(super) fn median_f64_sorted(samples: &[f64]) -> Option<f64> {
    if samples.is_empty() {
        None
    } else {
        Some(samples[samples.len() / 2])
    }
}

pub(super) fn percentile_u128(samples: &[u128], pct: f64) -> u128 {
    if samples.is_empty() {
        return 0;
    }
    let idx = ((samples.len() as f64 - 1.0) * pct).round() as usize;
    samples[idx.min(samples.len() - 1)]
}

pub(super) fn iqr_over_median(samples: &[u128]) -> f64 {
    if samples.len() < 4 {
        return 0.0;
    }
    let q1 = percentile_u128(samples, 0.25);
    let q3 = percentile_u128(samples, 0.75);
    let median = percentile_u128(samples, 0.5).max(1);
    (q3.saturating_sub(q1)) as f64 / median as f64
}

pub(super) fn bootstrap_ci_percentile(
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

pub(super) fn bootstrap_index(seed: &mut u64, len: usize) -> usize {
    let mut x = *seed;
    x = x.wrapping_mul(6364136223846793005).wrapping_add(1);
    *seed = x;
    (x as usize) % len.max(1)
}

pub(super) fn coefficient_of_variation(values: &[f64]) -> f64 {
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

pub(super) fn now_unix_ms() -> u128 {
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
    id: test_eval_perf::PerfScenarioId,
    test_name: String,
    class: test_eval_perf::BenchmarkScenarioClass,
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
    verdict: PerfCmpVerdict,
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
    id: test_eval_perf::PerfScenarioId,
    test_name: String,
    ops: u64,
    runtime_ns_median: u128,
}
