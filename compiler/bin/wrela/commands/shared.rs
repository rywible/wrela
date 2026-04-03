use super::cli_args::{CommandSpec, ParsedCommandSpec};
use super::contracts::{
    EXIT_CODEGEN, EXIT_OK, EXIT_PARSE, EXIT_RUNTIME_SIGNAL, EXIT_TYPE, EXIT_USAGE, OutputFormat,
};
use super::{cert_engine, deploy, diag_emit, perf_engine, replay_trace};
use miette::SourceSpan;
use serde::{Deserialize, Serialize};
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
            diagnostics.push((source_module.path.clone(), source_module.source.clone(), err));
        }
    }
    diagnostics
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
    let deploy_target = parsed.deploy_target;
    let deploy_app = parsed.deploy_app;
    let deploy_region = parsed.deploy_region;
    let deploy_machines = parsed.deploy_machines;
    let deploy_policy = parsed.deploy_policy;
    let deploy_replication_factor = parsed.deploy_replication_factor;
    let deploy_write_quorum = parsed.deploy_write_quorum;
    let deploy_logical_shards = parsed.deploy_logical_shards;
    let deploy_active_groups = parsed.deploy_active_groups;
    let deploy_force = parsed.deploy_force;
    let deploy_generate_only = parsed.deploy_generate_only;
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
    if command != "deploy"
        && (deploy_target.is_some()
            || deploy_app.is_some()
            || deploy_region.is_some()
            || deploy_machines.is_some()
            || deploy_policy.is_some()
            || deploy_replication_factor.is_some()
            || deploy_write_quorum.is_some()
            || deploy_logical_shards.is_some()
            || deploy_active_groups.is_some()
            || deploy_force
            || deploy_generate_only)
    {
        eprintln!(
            "error: --target, --app, --region, --machines, --deploy-policy, --rf, --wq, --logical-shards, --active-groups, --force, and --generate-only are only valid with `wrela deploy`"
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
                ) {
                    Ok(mir) => mir,
                    Err(code) => std::process::exit(code),
                }
            };
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
        "deploy" => {
            let exit = deploy::execute_deploy_command(deploy::DeployCommandInput {
                trace,
                path_arg,
                program_args,
                target: deploy_target,
                app: deploy_app,
                region: deploy_region,
                machines: deploy_machines,
                deploy_policy,
                replication_factor: deploy_replication_factor,
                write_quorum: deploy_write_quorum,
                logical_shards: deploy_logical_shards,
                active_groups: deploy_active_groups,
                force: deploy_force,
                generate_only: deploy_generate_only,
            });
            std::process::exit(exit);
        }
        _ => {
            diag_emit::print_help();
            std::process::exit(EXIT_USAGE);
        }
    }
}
