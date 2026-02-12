#![allow(unused_assignments)]

use miette::{Diagnostic, NamedSource, Report, SourceSpan};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::env;
use std::fs;
use std::io::{self};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use thiserror::Error;
use wrela::hir;
use wrela::mir;
use wrela::parser;

#[path = "wrela/repro.rs"]
mod repro;

#[derive(Debug, Error, Diagnostic)]
#[error("{message}")]
struct ProjectDiag {
    message: String,
    #[label("here")]
    span: SourceSpan,
}

fn main() {
    let trace = std::env::var("WRELA_BUILD_TRACE").is_ok();
    if trace {
        eprintln!("build: cli start");
    }
    let args: Vec<String> = env::args().skip(1).collect();
    let mut output_format = OutputFormat::Pretty;
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_help();
        return;
    }
    if args.iter().any(|arg| arg == "--version" || arg == "-V") {
        println!("wrela {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    let mut emit_mir = false;
    let mut emit_mir_opt = false;
    let mut emit_obj: Option<String> = None;
    let mut emit_bin: Option<String> = None;
    let mut out_path: Option<String> = None;
    let mut prefix_path: Option<String> = None;
    let mut command: Option<String> = None;
    let mut path_arg: Option<String> = None;
    let mut program_args: Vec<String> = Vec::new();
    let mut poll_ms: Option<u64> = None;
    let mut test_jobs: Option<usize> = None;
    let mut test_timeout_ms: Option<u64> = None;
    let mut test_record = false;
    let mut test_update_public_surface = false;
    let mut test_list = false;
    let mut test_id: Option<String> = None;
    let mut test_filter: Option<String> = None;
    let mut test_lane: Option<String> = None;
    let mut test_seed: Option<u64> = None;
    let mut repro_artifact_path: Option<String> = None;
    let mut perf_debug = false;
    let mut perf_runs: Option<usize> = None;
    let mut perf_baseline_out: Option<String> = None;
    let mut perf_gate_path: Option<String> = None;
    let mut perf_max_regression_pct: Option<f64> = None;
    let mut perf_cv_max_pct: Option<f64> = None;
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
    let mut seen_double_dash = false;

    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        if seen_double_dash {
            program_args.push(arg);
            continue;
        }
        if arg == "--" {
            seen_double_dash = true;
            continue;
        }
        if let Some(fmt) = arg.strip_prefix("--format=") {
            output_format = match fmt {
                "json" => OutputFormat::Json,
                _ => OutputFormat::Pretty,
            };
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
            poll_ms = ms.parse::<u64>().ok();
            continue;
        }
        if let Some(jobs) = arg.strip_prefix("--jobs=") {
            test_jobs = jobs.parse::<usize>().ok();
            continue;
        }
        if let Some(ms) = arg.strip_prefix("--test-timeout-ms=") {
            test_timeout_ms = ms.parse::<u64>().ok();
            continue;
        }
        if arg == "--record" {
            test_record = true;
            continue;
        }
        if arg == "--update-public-surface" {
            test_update_public_surface = true;
            continue;
        }
        if arg == "--list" {
            test_list = true;
            continue;
        }
        if let Some(value) = arg.strip_prefix("--id=") {
            test_id = Some(value.to_string());
            continue;
        }
        if let Some(value) = arg.strip_prefix("--filter=") {
            test_filter = Some(value.to_string());
            continue;
        }
        if let Some(value) = arg.strip_prefix("--lane=") {
            test_lane = Some(value.to_string());
            continue;
        }
        if let Some(value) = arg.strip_prefix("--seed=") {
            match value.parse::<u64>() {
                Ok(seed) => test_seed = Some(seed),
                Err(_) => {
                    eprintln!("error: invalid --seed value `{value}`");
                    std::process::exit(EXIT_USAGE);
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
            eprintln!("error: missing path for --repro");
            std::process::exit(EXIT_USAGE);
        }
        if arg == "--perf-debug" {
            perf_debug = true;
            continue;
        }
        if let Some(runs) = arg.strip_prefix("--runs=") {
            perf_runs = runs.parse::<usize>().ok();
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
            perf_max_regression_pct = value.parse::<f64>().ok();
            continue;
        }
        if let Some(value) = arg.strip_prefix("--perf-cv-max-pct=") {
            perf_cv_max_pct = value.parse::<f64>().ok();
            continue;
        }
        if let Some(value) = arg.strip_prefix("--kpi-check-fallback-max=") {
            kpi_check_fallback_max = value.parse::<f64>().ok();
            continue;
        }
        if let Some(value) = arg.strip_prefix("--kpi-check-batch-min=") {
            kpi_check_batch_min = value.parse::<f64>().ok();
            continue;
        }
        if let Some(value) = arg.strip_prefix("--kpi-scheduler-p99-improve-min-pct=") {
            kpi_scheduler_p99_improve_min_pct = value.parse::<f64>().ok();
            continue;
        }
        if let Some(value) = arg.strip_prefix("--kpi-rewrite-overhead-max-pct=") {
            kpi_rewrite_overhead_max_pct = value.parse::<f64>().ok();
            continue;
        }
        if let Some(value) = arg.strip_prefix("--kpi-actor-throughput-improve-min-pct=") {
            kpi_actor_throughput_improve_min_pct = value.parse::<f64>().ok();
            continue;
        }
        if let Some(value) = arg.strip_prefix("--kpi-queue-age-p99-max-regress-pct=") {
            kpi_queue_age_p99_max_regress_pct = value.parse::<f64>().ok();
            continue;
        }
        if let Some(value) = arg.strip_prefix("--kpi-starvation-violations-max=") {
            kpi_starvation_violations_max = value.parse::<f64>().ok();
            continue;
        }
        if let Some(value) = arg.strip_prefix("--kpi-scheduler-throughput-improve-min-pct=") {
            kpi_scheduler_throughput_improve_min_pct = value.parse::<f64>().ok();
            continue;
        }
        if let Some(value) = arg.strip_prefix("--kpi-scheduler-loop-p99-max-regress-pct=") {
            kpi_scheduler_loop_p99_max_regress_pct = value.parse::<f64>().ok();
            continue;
        }
        if let Some(value) = arg.strip_prefix("--kpi-scheduler-local-hit-min=") {
            kpi_scheduler_local_hit_min = value.parse::<f64>().ok();
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
                    eprintln!("error: invalid --warmup-pairs value `{value}`");
                    std::process::exit(EXIT_USAGE);
                }
            }
            continue;
        }
        if let Some(value) = arg.strip_prefix("--measure-pairs=") {
            match value.parse::<usize>() {
                Ok(parsed) => perfcmp_measure_pairs = Some(parsed),
                Err(_) => {
                    eprintln!("error: invalid --measure-pairs value `{value}`");
                    std::process::exit(EXIT_USAGE);
                }
            }
            continue;
        }
        if let Some(value) = arg.strip_prefix("--min-effect-pct=") {
            match value.parse::<f64>() {
                Ok(parsed) => perfcmp_min_effect_pct = Some(parsed),
                Err(_) => {
                    eprintln!("error: invalid --min-effect-pct value `{value}`");
                    std::process::exit(EXIT_USAGE);
                }
            }
            continue;
        }
        if let Some(value) = arg.strip_prefix("--confidence=") {
            match value.parse::<f64>() {
                Ok(parsed) => perfcmp_confidence_pct = Some(parsed),
                Err(_) => {
                    eprintln!("error: invalid --confidence value `{value}`");
                    std::process::exit(EXIT_USAGE);
                }
            }
            continue;
        }
        if arg == "--prefix" {
            if let Some(path) = iter.next() {
                prefix_path = Some(path);
            } else {
                eprintln!("error: missing path for --prefix");
                std::process::exit(EXIT_USAGE);
            }
            continue;
        }
        if arg == "-o" || arg == "--out" {
            if let Some(path) = iter.next() {
                out_path = Some(path);
            } else {
                eprintln!("error: missing output path for {arg}");
                std::process::exit(EXIT_USAGE);
            }
            continue;
        }
        if command.is_none() && is_command(&arg) {
            command = Some(arg);
            continue;
        }
        if path_arg.is_none() {
            path_arg = Some(arg);
        } else {
            program_args.push(arg);
        }
    }

    if command.is_none() && path_arg.is_some() {
        command = Some("run".to_string());
    }

    let command = match command.as_deref() {
        Some(cmd) => cmd,
        None => {
            print_help();
            std::process::exit(EXIT_USAGE);
        }
    };
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
        "check" => {
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
            let result = compile_to_mir(&entry_path, output_format, emit_mir, emit_mir_opt, false);
            if let Err(code) = result {
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
            if find_src_root(&entry_path).is_none() {
                eprintln!(
                    "error: `wrela build` requires project layout (`src/**`) because single-file mode bypasses architecture checks"
                );
                eprintln!(
                    "help: move entrypoint to `src/main.wr` and run `wrela build <project-or-src/main.wr>`"
                );
                std::process::exit(EXIT_USAGE);
            }
            let workspace_root = project_root_for_entry(&entry_path);
            let budget_policy = resolve_budget_policy_v1(test_jobs, test_timeout_ms);
            let jobs = budget_policy.test_jobs.value as usize;
            let timeout = Duration::from_millis(budget_policy.test_timeout_ms.value);
            let legacy_to_qualified_ids = match collect_source_function_id_aliases(&workspace_root)
            {
                Ok(map) => map,
                Err(err) => {
                    eprintln!("certification coverage id map error: {err}");
                    std::process::exit(EXIT_CODEGEN);
                }
            };
            let toolchain_version = resolve_toolchain_version();
            let source_hash = match hash_source_fingerprint(&workspace_root) {
                Ok(hash) => hash,
                Err(err) => {
                    eprintln!("certification cache error: {err}");
                    std::process::exit(EXIT_CODEGEN);
                }
            };
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
                let cert_result = run_tests(
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
                let snapshot =
                    canonicalize_function_coverage(&raw_snapshot, &legacy_to_qualified_ids);
                if let Err(err) =
                    write_function_coverage_snapshot(&function_coverage_path, &snapshot)
                {
                    eprintln!("certification cache error: {err}");
                    std::process::exit(EXIT_CODEGEN);
                }
                let coverage_index_path =
                    certification_coverage_index_path(&workspace_root, &cert_cache_hash);
                let coverage_index = build_function_test_coverage_index(
                    cert_result.summary.as_ref(),
                    &legacy_to_qualified_ids,
                );
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
            let mir_compile_start = Instant::now();
            let mir_module =
                match compile_to_mir(&entry_path, output_format, emit_mir, emit_mir_opt, true) {
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
            let output = out_path
                .or(emit_bin)
                .unwrap_or_else(|| "wrela.out".to_string());
            let codegen_start = Instant::now();
            if let Err(err) =
                wrela::backend::cranelift::compile_to_executable(&mir_module, output.as_ref())
            {
                eprintln!("codegen error: {}", err.0);
                std::process::exit(EXIT_CODEGEN);
            }
            let codegen_ms = codegen_start.elapsed().as_millis();
            let artifact_path = PathBuf::from(&output);
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
            let mir_module =
                match compile_to_mir(&entry_path, output_format, emit_mir, emit_mir_opt, true) {
                    Ok(mir) => mir,
                    Err(code) => std::process::exit(code),
                };
            let output = out_path.unwrap_or_else(temp_exe_path);
            if let Err(err) =
                wrela::backend::cranelift::compile_to_executable(&mir_module, output.as_ref())
            {
                eprintln!("codegen error: {}", err.0);
                std::process::exit(EXIT_CODEGEN);
            }
            let status = Command::new(&output)
                .args(&program_args)
                .status()
                .expect("run failed");
            std::process::exit(status.code().unwrap_or(EXIT_CODEGEN));
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
                &program_args,
            );
        }
        "test" => {
            if trace {
                eprintln!("build: command test");
            }
            if !program_args.is_empty() {
                eprintln!("error: unexpected extra arguments");
                std::process::exit(EXIT_USAGE);
            }
            if out_path.is_some() || emit_obj.is_some() || emit_bin.is_some() {
                eprintln!(
                    "error: -o/--out, --emit-obj, and --emit-bin are not valid with `wrela test`"
                );
                std::process::exit(EXIT_USAGE);
            }
            let budget_policy = resolve_budget_policy_v1(test_jobs, test_timeout_ms);
            let jobs = budget_policy.test_jobs.value as usize;
            let timeout = Duration::from_millis(budget_policy.test_timeout_ms.value);
            let target = match resolve_test_target(path_arg.as_deref()) {
                Ok(target) => target,
                Err(err) => {
                    eprintln!("error: {err}");
                    std::process::exit(EXIT_USAGE);
                }
            };
            if let TestTarget::SingleFile(path) = &target {
                eprintln!(
                    "error: `wrela test` no longer supports single-file targets because they bypass architecture checks: {}",
                    path.display()
                );
                eprintln!(
                    "help: use project layout (`src/**`, `tests/**`) and run `wrela test <project-root>`"
                );
                std::process::exit(EXIT_USAGE);
            }
            if repro_artifact_path.is_some()
                && (test_record
                    || test_update_public_surface
                    || test_selection.list
                    || test_selection.id.is_some()
                    || test_selection.filter.is_some())
            {
                eprintln!(
                    "error: --repro cannot be combined with --record, --update-public-surface, --list, --id, or --filter"
                );
                std::process::exit(EXIT_USAGE);
            }
            if let Some(repro_path) = repro_artifact_path.as_deref() {
                let workspace_root = match &target {
                    TestTarget::ProjectRoot(root) => root.clone(),
                    TestTarget::SingleFile(path) => path
                        .parent()
                        .unwrap_or_else(|| Path::new("."))
                        .to_path_buf(),
                };
                let exit = repro::run_repro_artifact(
                    &workspace_root,
                    Path::new(repro_path),
                    timeout,
                    output_format,
                    if test_record {
                        HttpCassetteMode::Record
                    } else {
                        HttpCassetteMode::Replay
                    },
                    &budget_policy,
                );
                std::process::exit(exit);
            }
            if test_record {
                eprintln!(
                    "maintenance mode: --record updates integration cassettes; no build artifact is emitted"
                );
            }
            if test_update_public_surface {
                eprintln!(
                    "maintenance mode: --update-public-surface updates snapshot baselines; no build artifact is emitted"
                );
            }
            let gate_cfg = perf_gate_path.as_ref().map(|path| PerfGateConfig {
                baseline_path: PathBuf::from(path),
                max_regression_pct: perf_max_regression_pct.unwrap_or(5.0),
                kpi_thresholds,
            });
            let result = run_tests(
                &target,
                &budget_policy,
                jobs,
                timeout,
                output_format,
                perf_debug,
                gate_cfg.as_ref(),
                &test_selection,
                false,
                if test_record {
                    HttpCassetteMode::Record
                } else {
                    HttpCassetteMode::Replay
                },
                test_seed,
            );
            let exit = result.exit;
            if test_record || test_update_public_surface {
                let workspace_root = match &target {
                    TestTarget::ProjectRoot(root) => root.clone(),
                    TestTarget::SingleFile(path) => path
                        .parent()
                        .unwrap_or_else(|| Path::new("."))
                        .to_path_buf(),
                };
                if test_update_public_surface && exit == EXIT_OK {
                    if let Err(err) = update_public_surface_baseline(&workspace_root) {
                        eprintln!("public surface update error: {err}");
                        std::process::exit(EXIT_CODEGEN);
                    }
                }
                if let Err(err) = write_test_maintenance_summary(
                    &workspace_root,
                    test_record,
                    test_update_public_surface,
                    exit,
                ) {
                    eprintln!("maintenance summary error: {err}");
                    std::process::exit(EXIT_CODEGEN);
                }
            }
            std::process::exit(exit);
        }
        "perf" => {
            if trace {
                eprintln!("build: command perf");
            }
            if !program_args.is_empty() {
                eprintln!("error: unexpected extra arguments");
                std::process::exit(EXIT_USAGE);
            }
            let runs = perf_runs.unwrap_or(5).max(1);
            let budget_policy = resolve_budget_policy_v1(test_jobs, test_timeout_ms);
            let jobs = budget_policy.test_jobs.value as usize;
            let mut timeout = Duration::from_millis(budget_policy.test_timeout_ms.value);
            let target = match resolve_test_target(path_arg.as_deref()) {
                Ok(target) => target,
                Err(err) => {
                    eprintln!("error: {err}");
                    std::process::exit(EXIT_USAGE);
                }
            };
            let manifest_path = resolve_benchmark_manifest_path(&target, benchmark_manifest_path);
            let mut runtime_only_cv_gate = false;
            let mut perf_selection = test_selection.clone();
            if let Some(path) = manifest_path.as_ref() {
                let manifest = match load_benchmark_manifest(path) {
                    Ok(manifest) => manifest,
                    Err(err) => {
                        eprintln!("benchmark manifest error: {err}");
                        std::process::exit(EXIT_USAGE);
                    }
                };
                let max_timeout_ms = manifest
                    .scenarios_for_profile(perf_profile)
                    .iter()
                    .filter_map(|scenario| scenario.timeout_ms)
                    .max();
                if let Some(max_timeout_ms) = max_timeout_ms {
                    timeout = timeout.max(Duration::from_millis(max_timeout_ms));
                }
                runtime_only_cv_gate = true;
                match build_benchmark_selection(&target, path, perf_profile) {
                    Ok(selection_ids) => {
                        perf_selection.include_ids = Some(selection_ids);
                    }
                    Err(err) => {
                        eprintln!("benchmark manifest error: {err}");
                        std::process::exit(EXIT_USAGE);
                    }
                }
            }
            let baseline_out = perf_baseline_out
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(".artifacts/perf/baseline.json"));
            let gate_cfg = perf_gate_path.as_ref().map(|path| PerfGateConfig {
                baseline_path: PathBuf::from(path),
                max_regression_pct: perf_max_regression_pct.unwrap_or(5.0),
                kpi_thresholds,
            });
            let cv_max_pct = perf_cv_max_pct.unwrap_or(5.0);
            let exit = run_perf_harness(
                &target,
                &budget_policy,
                jobs,
                timeout,
                output_format,
                perf_debug,
                runs,
                cv_max_pct,
                &baseline_out,
                gate_cfg.as_ref(),
                &perf_selection,
                runtime_only_cv_gate,
            );
            std::process::exit(exit);
        }
        "perfcmp" => {
            if trace {
                eprintln!("build: command perfcmp");
            }
            if !program_args.is_empty() {
                eprintln!("error: unexpected extra arguments");
                std::process::exit(EXIT_USAGE);
            }
            let target = match resolve_test_target(path_arg.as_deref()) {
                Ok(target) => target,
                Err(err) => {
                    eprintln!("error: {err}");
                    std::process::exit(EXIT_USAGE);
                }
            };
            let TestTarget::ProjectRoot(target_root) = target else {
                eprintln!("error: perfcmp target must be a benchmark project directory");
                std::process::exit(EXIT_USAGE);
            };
            let manifest_path = match resolve_benchmark_manifest_path(
                &TestTarget::ProjectRoot(target_root.clone()),
                benchmark_manifest_path,
            ) {
                Some(path) => path,
                None => {
                    eprintln!(
                        "error: benchmark manifest required; pass --benchmark-manifest or place bench.toml under target root"
                    );
                    std::process::exit(EXIT_USAGE);
                }
            };
            let baseline_ref = perfcmp_baseline_ref.unwrap_or_else(|| "origin/main".to_string());
            let candidate_ref = perfcmp_candidate_ref.unwrap_or_else(|| "HEAD".to_string());
            let report_out = out_path
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(".artifacts/perf/perfcmp-report.json"));
            let perfcmp_cfg = PerfCmpConfig {
                baseline_ref,
                candidate_ref,
                manifest_path,
                benchmark_root: target_root,
                profile: perf_profile,
                warmup_pairs_override: perfcmp_warmup_pairs,
                measure_pairs_override: perfcmp_measure_pairs,
                min_effect_pct: perfcmp_min_effect_pct.unwrap_or(2.0),
                confidence_pct: perfcmp_confidence_pct.unwrap_or(95.0),
                output_json: report_out,
                output_format,
                test_timeout_ms,
                perf_debug,
            };
            let exit = run_perfcmp(&perfcmp_cfg);
            std::process::exit(exit);
        }
        "matrix" => {
            if trace {
                eprintln!("build: command matrix");
            }
            if !program_args.is_empty() {
                eprintln!("error: unexpected extra arguments");
                std::process::exit(EXIT_USAGE);
            }
            let workspace_root = match path_arg {
                Some(path) => PathBuf::from(path),
                None => match env::current_dir() {
                    Ok(path) => path,
                    Err(err) => {
                        eprintln!("error: failed to resolve current directory: {err}");
                        std::process::exit(EXIT_USAGE);
                    }
                },
            };
            if !workspace_root.is_dir() {
                eprintln!(
                    "error: matrix target must be an existing directory: {}",
                    workspace_root.display()
                );
                std::process::exit(EXIT_USAGE);
            }
            let runs = perf_runs.unwrap_or(1).max(1);
            let exit = run_matrix(
                &workspace_root,
                runs,
                perf_gate_path.as_deref(),
                perf_max_regression_pct.unwrap_or(5.0),
                &kpi_thresholds,
            );
            std::process::exit(exit);
        }
        _ => {
            print_help();
            std::process::exit(EXIT_USAGE);
        }
    }
}

fn print_help() {
    println!(
        "usage: wrela <command> [options] <path> [-- args]\n\
\n\
commands:\n\
  init [path]           initialize a new project\n\
  update                update the installed toolchain\n\
  check <path>          parse and typecheck (no codegen)\n\
  build <path>          run certification, then compile executable on success only\n\
  compile <path>        alias for build (also certification-gated)\n\
  verify-cert <path>    verify an emitted cert.json report and hashes\n\
  run <path>            compile and run\n\
  dev <path>            watch and rebuild (polling)\n\
  test [path]           run tests from project root or a single .wr file\n\
  perf [path]           run perf harness and write baseline JSON\n\
  perfcmp [path]        run paired baseline/candidate perf comparison\n\
  matrix [path]         run workspace test/spec/perf matrix and write evidence bundle\n\
\n\
options:\n\
  --prefix PATH         install/update prefix (default: $PREFIX or ~/.local/wrela)\n\
  -o, --out PATH        output path for build/run\n\
  --emit-mir            emit MIR before optimization\n\
  --emit-mir-opt        emit MIR after optimization\n\
  --emit-obj=PATH       emit object file\n\
  --emit-bin=PATH       emit executable\n\
  --poll-ms=N           poll interval for dev (default: 500)\n\
  --jobs=N              test runner parallelism (default: 1)\n\
  --test-timeout-ms=N   per-test timeout in milliseconds (default: 5000)\n\
  env: WRELA_BUDGET_*   Budget Policy v1 overrides (autogen/sim/fuzz/mutation + time caps)\n\
  --record              test maintenance mode; updates integration cassettes\n\
  --update-public-surface  test maintenance mode; updates API snapshot baselines\n\
  --list                list discovered tests with stable id/lane metadata\n\
  --id=ID               run/list a single test by stable id\n\
  --filter=PATTERN      run/list tests matching pattern\n\
  --lane=NAME           run/list tests for lane (spec|integration|sim|model|default); valid for test/perf\n\
  --seed=N              schedule seed for sim tests; valid for test/perf\n\
  --benchmark-manifest=PATH  benchmark manifest path (bench.toml)\n\
  --profile=NAME        benchmark profile (smoke|standard|deep)\n\
  --repro PATH          replay a single typed repro artifact (autogen|fuzz)\n\
  --perf-debug          dump perf counters after tests\n\
  --runs=N              perf harness run count (default: 5)\n\
  --baseline-out=PATH   perf baseline JSON output path\n\
  --perf-gate=PATH      compare perf summary against baseline JSON\n\
  --perf-max-regression-pct=N  allowed regression percentage (default: 5)\n\
  --perf-cv-max-pct=N   max coefficient of variation percentage (default: 5)\n\
  --kpi-check-fallback-max=N  max allowed check fallback rate\n\
  --kpi-check-batch-min=N  minimum required average check batch size\n\
  --kpi-scheduler-p99-improve-min-pct=N  min scheduler p99 improvement vs baseline\n\
  --kpi-rewrite-overhead-max-pct=N  max rewrite compile overhead percentage\n\
  --kpi-actor-throughput-improve-min-pct=N  min actor throughput improvement vs baseline\n\
  --kpi-queue-age-p99-max-regress-pct=N  max queue age p99 regression percentage\n\
  --kpi-starvation-violations-max=N  max scheduler starvation violations\n\
  --kpi-scheduler-throughput-improve-min-pct=N  min scheduler throughput improvement vs baseline\n\
  --kpi-scheduler-loop-p99-max-regress-pct=N  max scheduler loop p99 regression percentage\n\
  --kpi-scheduler-local-hit-min=N  minimum local dispatch hit ratio\n\
  --baseline-ref=REF    perfcmp baseline git ref (default: origin/main)\n\
  --candidate-ref=REF   perfcmp candidate git ref (default: HEAD)\n\
  --warmup-pairs=N      perfcmp warmup pair count override\n\
  --measure-pairs=N     perfcmp measured pair count override\n\
  --min-effect-pct=N    perfcmp practical effect threshold (default: 2.0)\n\
  --confidence=N        perfcmp bootstrap CI confidence percent (default: 95)\n\
  --format=json         emit diagnostics as JSON\n\
  -h, --help            show this help\n\
  -V, --version         show version\n"
    );
}

fn init_project(path: &str) -> io::Result<()> {
    let root = Path::new(path);
    let src_dir = root.join("src");
    fs::create_dir_all(&src_dir)?;
    let main_path = src_dir.join("main.wr");
    if main_path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "src/main.wr already exists",
        ));
    }
    fs::write(main_path, "to run() -> Integer:\n    return 0\n")?;
    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum OutputFormat {
    Pretty,
    Json,
}

const CERT_SCHEMA_VERSION: u32 = 3;
const CERT_GATE_VERSIONS_MARKER: &str = "wrela-cert-gates-v1";
const COVERAGE_SNAPSHOT_SCHEMA_VERSION: u32 = 2;
const COVERAGE_INDEX_SCHEMA_VERSION: u32 = 2;
const MUTATION_CACHE_SCHEMA_VERSION: u32 = 1;
const MUTATION_KILL_HISTORY_SCHEMA_VERSION: u32 = 1;
const MUTATION_CACHE_ENGINE_TAG: &str = "wrela-mutation-cache-v1";
const RUNTIME_CARGO_TOML: &str = include_str!("../../runtime/Cargo.toml");
const BUDGET_POLICY_VERSION: u32 = 1;
const DEFAULT_TEST_JOBS: u64 = 1;
const DEFAULT_TEST_TIMEOUT_MS: u64 = 5_000;
const DEFAULT_AUTOGEN_MAX_CASES: u64 = 64;
const DEFAULT_SIM_MAX_CASES: u64 = 256;
const DEFAULT_FUZZ_MAX_CASES: u64 = 128;
const DEFAULT_MUTATION_MAX_CASES: u64 = 32;
const DEFAULT_AUTOGEN_TIME_CAP_MS: u64 = 5_000;
const DEFAULT_SIM_TIME_CAP_MS: u64 = 10_000;
const DEFAULT_FUZZ_TIME_CAP_MS: u64 = 15_000;
const DEFAULT_MUTATION_TIME_CAP_MS: u64 = 20_000;
const CEILING_TEST_JOBS: u64 = 64;
const CEILING_TEST_TIMEOUT_MS: u64 = 120_000;
const CEILING_AUTOGEN_MAX_CASES: u64 = 1_024;
const CEILING_SIM_MAX_CASES: u64 = 4_096;
const CEILING_FUZZ_MAX_CASES: u64 = 4_096;
const CEILING_MUTATION_MAX_CASES: u64 = 512;
const CEILING_AUTOGEN_TIME_CAP_MS: u64 = 60_000;
const CEILING_SIM_TIME_CAP_MS: u64 = 120_000;
const CEILING_FUZZ_TIME_CAP_MS: u64 = 120_000;
const CEILING_MUTATION_TIME_CAP_MS: u64 = 180_000;
const PUBLIC_SURFACE_CURRENT_REL_PATH: &str = "tests/.artifacts/public_surface/current.json";
const PUBLIC_SURFACE_BASELINE_REL_PATH: &str = "tests/public_surface.baseline.json";

#[derive(Serialize, Deserialize)]
struct CertificationReport {
    cert_schema_version: u32,
    generated_at_unix_ms: u128,
    entry_path: String,
    workspace_root: String,
    artifact_path: String,
    tests_passed: bool,
    toolchain_version: String,
    compiler_version: String,
    compiler_git_sha: Option<String>,
    runtime_version: String,
    gate_versions_marker: String,
    source_hash: String,
    seeds_used: CertificationSeedsUsed,
    budgets_used: CertificationBudgetsUsed,
    coverage_summary_hash: Option<String>,
    mutation_summary_hash: Option<String>,
    differential_results_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    impact_manifest: Option<CertifiedImpactManifest>,
    binary_hash: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct CertifiedImpactManifest {
    source_files: Vec<CertifiedSourceFileFingerprint>,
    src_modules: Vec<CertifiedSrcModuleSnapshot>,
}

#[derive(Clone, Serialize, Deserialize)]
struct CertifiedSourceFileFingerprint {
    rel_path: String,
    hash: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct CertifiedSrcModuleSnapshot {
    module_path: String,
    rel_path: String,
    hash: String,
    uses: Vec<String>,
    runtime_sensitive: bool,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
struct PublicSurfaceSnapshot {
    version: u32,
    items: Vec<PublicSurfaceItem>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
struct PublicSurfaceItem {
    qualified_name: String,
    signature: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    connector_literals: Vec<PublicSurfaceConnectorLiteral>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
struct PublicSurfaceConnectorLiteral {
    service: String,
    endpoint: String,
    method: String,
    url: String,
}

#[derive(Serialize, Deserialize)]
struct CertificationSeedsUsed {
    sim: u64,
    autogen: u64,
    fuzz: u64,
}

#[derive(Serialize, Deserialize)]
struct CertificationBudgetsUsed {
    policy_version: u32,
    test_jobs: BudgetValue,
    test_timeout_ms: BudgetValue,
    autogen_max_cases: BudgetValue,
    sim_max_cases: BudgetValue,
    fuzz_max_cases: BudgetValue,
    mutation_max_cases: BudgetValue,
    autogen_time_cap_ms: BudgetValue,
    sim_time_cap_ms: BudgetValue,
    fuzz_time_cap_ms: BudgetValue,
    mutation_time_cap_ms: BudgetValue,
}

#[derive(Debug, Deserialize)]
struct ConnectorCoverageCassette {
    request: ConnectorCoverageRequest,
    response: ConnectorCoverageResponse,
}

#[derive(Debug, Deserialize)]
struct ConnectorCoverageRequest {
    service: String,
    endpoint: String,
}

#[derive(Debug, Deserialize)]
struct ConnectorCoverageResponse {
    status: u16,
}

#[derive(Serialize)]
struct BuildPerfEvent {
    event: &'static str,
    perf: BuildPerfPayload,
}

#[derive(Serialize)]
struct BuildPerfPayload {
    cache: BuildPerfCache,
    timings: BuildPerfTimings,
}

#[derive(Serialize)]
struct BuildPerfCache {
    hit: bool,
    hash: String,
    reason: String,
}

#[derive(Serialize)]
struct BuildPerfTimings {
    certification_ms: u128,
    cert_collect_tests_ms: u128,
    cert_compile_harness_ms: u128,
    cert_determinism_ms: u128,
    cert_mutation_discovery_ms: u128,
    cert_mutation_execution_ms: u128,
    cert_diff_ms: u128,
    mir_compile_ms: u128,
    codegen_ms: u128,
    cert_report_ms: u128,
    total_ms: u128,
}

#[derive(Clone, Serialize, Deserialize)]
struct BudgetValue {
    value: u64,
    default: u64,
    ceiling: u64,
    provenance: BudgetProvenance,
}

#[derive(Clone, Serialize, Deserialize)]
struct BudgetProvenance {
    source: String,
    key: String,
    requested: u64,
    clamped_to_ceiling: bool,
}

#[derive(Clone, Serialize, Deserialize)]
struct BudgetPolicyV1 {
    policy_version: u32,
    test_jobs: BudgetValue,
    test_timeout_ms: BudgetValue,
    autogen_max_cases: BudgetValue,
    sim_max_cases: BudgetValue,
    fuzz_max_cases: BudgetValue,
    mutation_max_cases: BudgetValue,
    autogen_time_cap_ms: BudgetValue,
    sim_time_cap_ms: BudgetValue,
    fuzz_time_cap_ms: BudgetValue,
    mutation_time_cap_ms: BudgetValue,
}

#[derive(Serialize)]
struct TestMaintenanceSummary {
    version: u32,
    generated_at_unix_ms: u128,
    workspace_root: String,
    mode_record: bool,
    mode_update_public_surface: bool,
    exit_code: i32,
    deployable_artifacts_emitted: bool,
}

#[derive(Serialize)]
struct BuildCertCacheJsonEvent {
    event: &'static str,
    cache_hit: bool,
    cache_hash: String,
    cache_dir: String,
}

#[derive(Clone, Serialize)]
struct CertSelectionJsonEvent {
    event: &'static str,
    mode: String,
    changed_files: Vec<String>,
    changed_src_modules: Vec<String>,
    impacted_src_modules: Vec<String>,
    selected_test_count: usize,
    selected_stage_count: usize,
    stages: Vec<CertSelectionStage>,
    reasons: Vec<String>,
}

#[derive(Clone, Serialize)]
struct CertSelectionStage {
    lane: String,
    selected: bool,
    reason: String,
}

#[derive(Clone, Default)]
struct CertSelectionReport {
    mode: String,
    changed_files: Vec<String>,
    changed_src_modules: Vec<String>,
    impacted_src_modules: Vec<String>,
    stages: Vec<CertSelectionStage>,
    reasons: Vec<String>,
}

fn write_certification_report(
    entry_path: &Path,
    workspace_root: &Path,
    artifact_path: &Path,
    budgets_used: &BudgetPolicyV1,
    toolchain_version: &str,
    source_hash: &str,
    cache_hash: &str,
    differential_results_hash: Option<&str>,
    mutation_summary_hash: Option<&str>,
) -> Result<(), String> {
    let generated_at_unix_ms = now_unix_ms();
    let binary_hash = hash_file_fingerprint(artifact_path)?;
    let compiler_version = env!("CARGO_PKG_VERSION").to_string();
    let compiler_git_sha = resolve_compiler_git_sha();
    let runtime_version = resolve_runtime_version();
    let report = CertificationReport {
        cert_schema_version: CERT_SCHEMA_VERSION,
        generated_at_unix_ms,
        entry_path: entry_path.display().to_string(),
        workspace_root: workspace_root.display().to_string(),
        artifact_path: artifact_path.display().to_string(),
        tests_passed: true,
        toolchain_version: toolchain_version.to_string(),
        compiler_version,
        compiler_git_sha,
        runtime_version,
        gate_versions_marker: CERT_GATE_VERSIONS_MARKER.to_string(),
        source_hash: source_hash.to_string(),
        seeds_used: CertificationSeedsUsed {
            sim: 0x5A17,
            autogen: 0xA670,
            fuzz: 0xF022,
        },
        budgets_used: certification_budgets_used(budgets_used),
        coverage_summary_hash: None,
        mutation_summary_hash: mutation_summary_hash.map(str::to_string),
        differential_results_hash: differential_results_hash.map(str::to_string),
        impact_manifest: build_certified_impact_manifest(workspace_root).ok(),
        binary_hash: binary_hash.clone(),
    };
    let payload = serde_json::to_vec_pretty(&report).map_err(|err| err.to_string())?;
    let adjacent_path = artifact_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("cert.json");
    fs::write(&adjacent_path, &payload).map_err(|err| {
        format!(
            "failed to write adjacent cert report {}: {}",
            adjacent_path.display(),
            err
        )
    })?;
    let cache_dir = workspace_root
        .join("target")
        .join("wrela_cert")
        .join(cache_hash);
    fs::create_dir_all(&cache_dir).map_err(|err| {
        format!(
            "failed to create cert cache {}: {}",
            cache_dir.display(),
            err
        )
    })?;
    let cache_path = cache_dir.join("cert.json");
    fs::write(&cache_path, &payload).map_err(|err| {
        format!(
            "failed to write cached cert report {}: {}",
            cache_path.display(),
            err
        )
    })?;
    if binary_hash != cache_hash {
        let compat_dir = workspace_root
            .join("target")
            .join("wrela_cert")
            .join(&binary_hash);
        fs::create_dir_all(&compat_dir).map_err(|err| {
            format!(
                "failed to create compatibility cert cache {}: {}",
                compat_dir.display(),
                err
            )
        })?;
        let compat_path = compat_dir.join("cert.json");
        fs::write(&compat_path, &payload).map_err(|err| {
            format!(
                "failed to write compatibility cached cert report {}: {}",
                compat_path.display(),
                err
            )
        })?;
    }
    let latest_success_path = workspace_root
        .join("target")
        .join("wrela_cert")
        .join("last_success_cert.json");
    fs::write(&latest_success_path, &payload).map_err(|err| {
        format!(
            "failed to write latest successful cert report {}: {}",
            latest_success_path.display(),
            err
        )
    })?;
    Ok(())
}

fn certification_budgets_used(policy: &BudgetPolicyV1) -> CertificationBudgetsUsed {
    CertificationBudgetsUsed {
        policy_version: policy.policy_version,
        test_jobs: policy.test_jobs.clone(),
        test_timeout_ms: policy.test_timeout_ms.clone(),
        autogen_max_cases: policy.autogen_max_cases.clone(),
        sim_max_cases: policy.sim_max_cases.clone(),
        fuzz_max_cases: policy.fuzz_max_cases.clone(),
        mutation_max_cases: policy.mutation_max_cases.clone(),
        autogen_time_cap_ms: policy.autogen_time_cap_ms.clone(),
        sim_time_cap_ms: policy.sim_time_cap_ms.clone(),
        fuzz_time_cap_ms: policy.fuzz_time_cap_ms.clone(),
        mutation_time_cap_ms: policy.mutation_time_cap_ms.clone(),
    }
}

fn resolve_compiler_git_sha() -> Option<String> {
    if let Some(sha) = option_env!("WRELA_GIT_SHA")
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return Some(sha.to_string());
    }
    if let Some(sha) = std::env::var("WRELA_GIT_SHA")
        .ok()
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return Some(sha.to_string());
    }
    if let Some(sha) = std::env::var("GITHUB_SHA")
        .ok()
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return Some(sha.to_string());
    }

    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let output = Command::new("git")
        .arg("-C")
        .arg(manifest_dir)
        .args(["rev-parse", "--verify", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if sha.is_empty() { None } else { Some(sha) }
}

fn resolve_runtime_version() -> String {
    if let Some(version) = parse_cargo_package_version(RUNTIME_CARGO_TOML) {
        return version;
    }
    "unknown".to_string()
}

fn resolve_toolchain_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

fn certification_cache_hash(source_hash: &str, toolchain_version: &str) -> String {
    let mut hasher = Fnv1a64::new();
    hasher.update(b"wrela-cert-cache-v2");
    hasher.update(&[0]);
    hasher.update(b"source_hash:");
    hasher.update(source_hash.as_bytes());
    hasher.update(&[0]);
    hasher.update(b"toolchain_version:");
    hasher.update(toolchain_version.as_bytes());
    hasher.finish_hex()
}

fn emit_certification_cache_hit(output_format: OutputFormat, cache_hash: &str, cache_dir: &Path) {
    match output_format {
        OutputFormat::Pretty => {
            eprintln!("certification cache hit: {}", cache_hash);
        }
        OutputFormat::Json => {
            let event = BuildCertCacheJsonEvent {
                event: "certification_cache",
                cache_hit: true,
                cache_hash: cache_hash.to_string(),
                cache_dir: cache_dir.display().to_string(),
            };
            println!(
                "{}",
                serde_json::to_string(&event).unwrap_or_else(|_| "{}".to_string())
            );
        }
    }
}

fn emit_build_perf_event(
    output_format: OutputFormat,
    cache_hit: bool,
    cache_hash: String,
    cache_reason: String,
    timings: BuildPerfTimings,
) {
    if !matches!(output_format, OutputFormat::Json) {
        return;
    }
    let event = BuildPerfEvent {
        event: "build_perf",
        perf: BuildPerfPayload {
            cache: BuildPerfCache {
                hit: cache_hit,
                hash: cache_hash,
                reason: cache_reason,
            },
            timings,
        },
    };
    println!(
        "{}",
        serde_json::to_string(&event).unwrap_or_else(|_| "{}".to_string())
    );
}

fn emit_cert_selection_report(
    output_format: OutputFormat,
    report: &CertSelectionReport,
    selected_test_count: usize,
) {
    let selected_stage_count = report.stages.iter().filter(|stage| stage.selected).count();
    match output_format {
        OutputFormat::Pretty => {
            eprintln!(
                "certification selection: mode={} selected_tests={} selected_stages={}",
                report.mode, selected_test_count, selected_stage_count
            );
            if !report.changed_files.is_empty() {
                eprintln!("  changed_files: {}", report.changed_files.join(", "));
            }
            if !report.changed_src_modules.is_empty() {
                eprintln!(
                    "  changed_src_modules: {}",
                    report.changed_src_modules.join(", ")
                );
            }
            if !report.impacted_src_modules.is_empty() {
                eprintln!(
                    "  impacted_src_modules: {}",
                    report.impacted_src_modules.join(", ")
                );
            }
            for stage in &report.stages {
                eprintln!(
                    "  stage={} selected={} reason={}",
                    stage.lane, stage.selected, stage.reason
                );
            }
            for reason in &report.reasons {
                eprintln!("  reason: {reason}");
            }
        }
        OutputFormat::Json => {
            let event = CertSelectionJsonEvent {
                event: "certification_selection",
                mode: report.mode.clone(),
                changed_files: report.changed_files.clone(),
                changed_src_modules: report.changed_src_modules.clone(),
                impacted_src_modules: report.impacted_src_modules.clone(),
                selected_test_count,
                selected_stage_count,
                stages: report.stages.clone(),
                reasons: report.reasons.clone(),
            };
            println!(
                "{}",
                serde_json::to_string(&event).unwrap_or_else(|_| "{}".to_string())
            );
        }
    }
}

fn resolve_certification_test_selection(
    workspace_root: &Path,
    output_format: OutputFormat,
) -> TestSelection {
    let mut selection = TestSelection::default();
    let latest_success_path = workspace_root
        .join("target")
        .join("wrela_cert")
        .join("last_success_cert.json");
    if !latest_success_path.is_file() {
        selection.cert_selection_report = Some(CertSelectionReport {
            mode: "full".to_string(),
            stages: vec![
                CertSelectionStage {
                    lane: "spec".to_string(),
                    selected: true,
                    reason: "safe default".to_string(),
                },
                CertSelectionStage {
                    lane: "integration".to_string(),
                    selected: true,
                    reason: "safe default".to_string(),
                },
                CertSelectionStage {
                    lane: "sim".to_string(),
                    selected: true,
                    reason: "safe default".to_string(),
                },
                CertSelectionStage {
                    lane: "model".to_string(),
                    selected: true,
                    reason: "safe default".to_string(),
                },
                CertSelectionStage {
                    lane: "default".to_string(),
                    selected: true,
                    reason: "safe default".to_string(),
                },
            ],
            reasons: vec![
                "no previous successful cert manifest; running full certification suite"
                    .to_string(),
            ],
            ..CertSelectionReport::default()
        });
        return selection;
    }

    let previous_report = match read_certification_report(&latest_success_path) {
        Ok(report) => report,
        Err(err) => {
            selection.cert_selection_report = Some(CertSelectionReport {
                mode: "full".to_string(),
                stages: vec![
                    CertSelectionStage {
                        lane: "spec".to_string(),
                        selected: true,
                        reason: "safe default".to_string(),
                    },
                    CertSelectionStage {
                        lane: "integration".to_string(),
                        selected: true,
                        reason: "safe default".to_string(),
                    },
                    CertSelectionStage {
                        lane: "sim".to_string(),
                        selected: true,
                        reason: "safe default".to_string(),
                    },
                    CertSelectionStage {
                        lane: "model".to_string(),
                        selected: true,
                        reason: "safe default".to_string(),
                    },
                    CertSelectionStage {
                        lane: "default".to_string(),
                        selected: true,
                        reason: "safe default".to_string(),
                    },
                ],
                reasons: vec![format!(
                    "failed to parse previous successful cert manifest ({}): {}",
                    latest_success_path.display(),
                    err
                )],
                ..CertSelectionReport::default()
            });
            return selection;
        }
    };

    let Some(previous_manifest) = previous_report.impact_manifest else {
        selection.cert_selection_report = Some(CertSelectionReport {
            mode: "full".to_string(),
            stages: vec![
                CertSelectionStage {
                    lane: "spec".to_string(),
                    selected: true,
                    reason: "safe default".to_string(),
                },
                CertSelectionStage {
                    lane: "integration".to_string(),
                    selected: true,
                    reason: "safe default".to_string(),
                },
                CertSelectionStage {
                    lane: "sim".to_string(),
                    selected: true,
                    reason: "safe default".to_string(),
                },
                CertSelectionStage {
                    lane: "model".to_string(),
                    selected: true,
                    reason: "safe default".to_string(),
                },
                CertSelectionStage {
                    lane: "default".to_string(),
                    selected: true,
                    reason: "safe default".to_string(),
                },
            ],
            reasons: vec![
                "previous successful cert is missing impact manifest; running full suite"
                    .to_string(),
            ],
            ..CertSelectionReport::default()
        });
        return selection;
    };

    let current_manifest = match build_certified_impact_manifest(workspace_root) {
        Ok(manifest) => manifest,
        Err(err) => {
            selection.cert_selection_report = Some(CertSelectionReport {
                mode: "full".to_string(),
                stages: vec![
                    CertSelectionStage {
                        lane: "spec".to_string(),
                        selected: true,
                        reason: "safe default".to_string(),
                    },
                    CertSelectionStage {
                        lane: "integration".to_string(),
                        selected: true,
                        reason: "safe default".to_string(),
                    },
                    CertSelectionStage {
                        lane: "sim".to_string(),
                        selected: true,
                        reason: "safe default".to_string(),
                    },
                    CertSelectionStage {
                        lane: "model".to_string(),
                        selected: true,
                        reason: "safe default".to_string(),
                    },
                    CertSelectionStage {
                        lane: "default".to_string(),
                        selected: true,
                        reason: "safe default".to_string(),
                    },
                ],
                reasons: vec![format!("failed to build current impact manifest: {err}")],
                ..CertSelectionReport::default()
            });
            return selection;
        }
    };

    let changed_files = diff_changed_files(&previous_manifest, &current_manifest);
    let changed_src_modules = diff_changed_src_modules(&previous_manifest, &current_manifest);
    let impacted_src_modules =
        impacted_src_modules_from_changed(&current_manifest.src_modules, &changed_src_modules);
    let runtime_sensitive_impacted = impacted_src_modules.iter().any(|module_path| {
        current_manifest
            .src_modules
            .iter()
            .find(|module| &module.module_path == module_path)
            .is_some_and(|module| module.runtime_sensitive)
    });

    let mut tests = Vec::new();
    let tests_root = workspace_root.join("tests");
    if !tests_root.is_dir() || collect_tests(&tests_root, &tests_root, &mut tests).is_err() {
        return selection;
    }
    tests.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));

    let integration_reachability = integration_reachability_to_impacted(
        workspace_root,
        &current_manifest,
        &impacted_src_modules,
    );
    let mut selected_ids = HashSet::new();
    for test in &tests {
        match test.lane {
            TestLane::Spec => {
                selected_ids.insert(test.id.clone());
            }
            TestLane::Integration => {
                if integration_reachability
                    .get(&test.module_path)
                    .copied()
                    .unwrap_or(false)
                {
                    selected_ids.insert(test.id.clone());
                }
            }
            TestLane::Sim => {
                if runtime_sensitive_impacted {
                    selected_ids.insert(test.id.clone());
                }
            }
            TestLane::Model | TestLane::Default => {
                selected_ids.insert(test.id.clone());
            }
        }
    }
    let lane_selected_ids = selected_ids.clone();

    let previous_index_hash = certification_cache_hash(
        &previous_report.source_hash,
        &previous_report.toolchain_version,
    );
    match load_function_test_coverage_index(workspace_root, &previous_index_hash) {
        Ok(index) if index.is_empty() => {
            if !changed_src_modules.is_empty() {
                selection_reasons_push(
                    &mut selection,
                    "previous certification coverage index is empty; keeping lane-based selection"
                        .to_string(),
                );
            }
        }
        Ok(index) => {
            let changed_function_ids = changed_function_ids_from_modules(
                workspace_root,
                &current_manifest,
                &changed_src_modules,
            );
            if changed_function_ids.is_empty() {
                selection_reasons_push(
                    &mut selection,
                    "no changed top-level functions/checks extracted from changed src modules; keeping lane-based selection"
                        .to_string(),
                );
            } else {
                let mut mapped_test_ids = BTreeSet::new();
                let mut unmapped_function_count = 0usize;
                for function_id in &changed_function_ids {
                    if let Some(test_ids) = index.get(function_id) {
                        for test_id in test_ids {
                            mapped_test_ids.insert(test_id.clone());
                        }
                    } else {
                        unmapped_function_count += 1;
                    }
                }
                if mapped_test_ids.is_empty() {
                    selection_reasons_push(
                        &mut selection,
                        format!(
                            "coverage index has no mapped tests for {} changed function ids (likely stale); keeping lane-based selection",
                            changed_function_ids.len()
                        ),
                    );
                } else {
                    let mut trimmed_ids = selected_ids
                        .iter()
                        .filter(|id| mapped_test_ids.contains(*id))
                        .cloned()
                        .collect::<HashSet<_>>();
                    if trimmed_ids.is_empty() {
                        selection_reasons_push(
                            &mut selection,
                            format!(
                                "coverage index mapping would prune all selected tests (lane_selected={} mapped={} changed_functions={}); keeping lane-based selection",
                                lane_selected_ids.len(),
                                mapped_test_ids.len(),
                                changed_function_ids.len()
                            ),
                        );
                    } else {
                        selected_ids.clear();
                        selected_ids.extend(trimmed_ids.drain());
                        selection_reasons_push(
                            &mut selection,
                            format!(
                                "coverage index trim applied: lane_selected={} trimmed={} changed_functions={} unmapped_functions={}",
                                lane_selected_ids.len(),
                                selected_ids.len(),
                                changed_function_ids.len(),
                                unmapped_function_count
                            ),
                        );
                    }
                }
            }
        }
        Err(err) => {
            if !changed_src_modules.is_empty() {
                selection_reasons_push(
                    &mut selection,
                    format!(
                        "coverage index unavailable for previous cert hash {}: {}; keeping lane-based selection",
                        previous_index_hash, err
                    ),
                );
            }
        }
    }
    if selected_ids.is_empty() && !lane_selected_ids.is_empty() {
        selected_ids = lane_selected_ids.clone();
        selection_reasons_push(
            &mut selection,
            "selection safety guard restored lane-based selection to avoid empty certification set"
                .to_string(),
        );
    }

    let mut stage_counts: BTreeMap<&str, usize> = BTreeMap::new();
    for test in &tests {
        *stage_counts.entry(test.lane.as_str()).or_insert(0) += 1;
    }
    let mut selected_stage_counts: BTreeMap<&str, usize> = BTreeMap::new();
    for test in &tests {
        if selected_ids.contains(&test.id) {
            *selected_stage_counts.entry(test.lane.as_str()).or_insert(0) += 1;
        }
    }
    let stage_names = ["spec", "integration", "sim", "model", "default"];
    let stages = stage_names
        .iter()
        .map(|lane| {
            let total = stage_counts.get(lane).copied().unwrap_or(0);
            let selected = selected_stage_counts.get(lane).copied().unwrap_or(0);
            let reason = match *lane {
                "spec" => "always selected".to_string(),
                "integration" => format!(
                    "selected modules that transitively import impacted src modules ({selected}/{total})"
                ),
                "sim" => {
                    if runtime_sensitive_impacted {
                        "runtime-sensitive impacted src modules detected".to_string()
                    } else {
                        "no runtime-sensitive impacted src modules detected".to_string()
                    }
                }
                _ => "safe behavior: run all".to_string(),
            };
            CertSelectionStage {
                lane: (*lane).to_string(),
                selected: selected > 0 || total == 0,
                reason,
            }
        })
        .collect::<Vec<_>>();

    let mut reasons = Vec::new();
    if changed_files.is_empty() {
        reasons.push("no source file deltas observed between manifests".to_string());
    }
    reasons.push(format!(
        "changed_files={} changed_src_modules={} impacted_src_modules={}",
        changed_files.len(),
        changed_src_modules.len(),
        impacted_src_modules.len()
    ));
    if matches!(output_format, OutputFormat::Pretty) {
        if changed_src_modules.is_empty() {
            reasons.push(
                "no src module deltas; integration and sim lanes reduced by policy".to_string(),
            );
        }
    }
    if let Some(report) = selection.cert_selection_report.as_ref() {
        reasons.extend(report.reasons.clone());
    }

    selection.include_ids = Some(selected_ids);
    selection.cert_selection_report = Some(CertSelectionReport {
        mode: "incremental".to_string(),
        changed_files,
        changed_src_modules,
        impacted_src_modules,
        stages,
        reasons,
    });
    selection
}

fn selection_reasons_push(selection: &mut TestSelection, reason: String) {
    let report = selection
        .cert_selection_report
        .get_or_insert_with(CertSelectionReport::default);
    report.reasons.push(reason);
}

fn changed_function_ids_from_modules(
    workspace_root: &Path,
    current_manifest: &CertifiedImpactManifest,
    changed_src_modules: &[String],
) -> BTreeSet<String> {
    use wrela::parser::ast::AstNode;

    let module_to_rel_path: BTreeMap<&str, &str> = current_manifest
        .src_modules
        .iter()
        .map(|module| (module.module_path.as_str(), module.rel_path.as_str()))
        .collect();
    let mut function_ids = BTreeSet::new();
    for module_path in changed_src_modules {
        let Some(rel_path) = module_to_rel_path.get(module_path.as_str()) else {
            continue;
        };
        let source_path = workspace_root.join(rel_path);
        let Ok(source) = fs::read_to_string(&source_path) else {
            continue;
        };
        let (syntax, parse_errors) = parser::parse_with_errors(&source);
        if !parse_errors.is_empty() {
            continue;
        }
        let Some(root) = parser::ast::Root::cast(syntax) else {
            continue;
        };
        let lowered = hir::lower::lower(root);
        for (_, function) in lowered.functions.iter() {
            if matches!(
                function.kind,
                hir::FunctionKind::Function | hir::FunctionKind::Check
            ) {
                let qualified_identity =
                    qualified_function_identity(module_path, function.name.as_str());
                function_ids.insert(stable_function_id(&qualified_identity));
            }
        }
    }
    function_ids
}

fn read_certification_report(path: &Path) -> Result<CertificationReport, String> {
    let payload = fs::read_to_string(path)
        .map_err(|err| format!("failed to read {}: {}", path.display(), err))?;
    serde_json::from_str(&payload)
        .map_err(|err| format!("failed to parse {} as cert json: {}", path.display(), err))
}

fn diff_changed_files(
    previous: &CertifiedImpactManifest,
    current: &CertifiedImpactManifest,
) -> Vec<String> {
    let previous_map: BTreeMap<&str, &str> = previous
        .source_files
        .iter()
        .map(|file| (file.rel_path.as_str(), file.hash.as_str()))
        .collect();
    let current_map: BTreeMap<&str, &str> = current
        .source_files
        .iter()
        .map(|file| (file.rel_path.as_str(), file.hash.as_str()))
        .collect();
    let all_paths: BTreeSet<&str> = previous_map
        .keys()
        .copied()
        .chain(current_map.keys().copied())
        .collect();
    all_paths
        .into_iter()
        .filter(|path| previous_map.get(path) != current_map.get(path))
        .map(|path| path.to_string())
        .collect()
}

fn diff_changed_src_modules(
    previous: &CertifiedImpactManifest,
    current: &CertifiedImpactManifest,
) -> Vec<String> {
    let previous_map: BTreeMap<&str, &str> = previous
        .src_modules
        .iter()
        .map(|module| (module.module_path.as_str(), module.hash.as_str()))
        .collect();
    let current_map: BTreeMap<&str, &str> = current
        .src_modules
        .iter()
        .map(|module| (module.module_path.as_str(), module.hash.as_str()))
        .collect();
    let all_modules: BTreeSet<&str> = previous_map
        .keys()
        .copied()
        .chain(current_map.keys().copied())
        .collect();
    all_modules
        .into_iter()
        .filter(|module| previous_map.get(module) != current_map.get(module))
        .map(|module| module.to_string())
        .collect()
}

fn impacted_src_modules_from_changed(
    src_modules: &[CertifiedSrcModuleSnapshot],
    changed_src_modules: &[String],
) -> Vec<String> {
    let module_set: HashSet<&str> = src_modules
        .iter()
        .map(|module| module.module_path.as_str())
        .collect();
    let mut reverse: HashMap<&str, Vec<&str>> = HashMap::new();
    for module in src_modules {
        for dep in &module.uses {
            if module_set.contains(dep.as_str()) {
                reverse
                    .entry(dep.as_str())
                    .or_default()
                    .push(module.module_path.as_str());
            }
        }
    }
    let mut queue = VecDeque::new();
    let mut impacted = BTreeSet::new();
    for module in changed_src_modules {
        if module_set.contains(module.as_str()) {
            impacted.insert(module.clone());
            queue.push_back(module.clone());
        }
    }
    while let Some(module) = queue.pop_front() {
        if let Some(users) = reverse.get(module.as_str()) {
            for user in users {
                if impacted.insert((*user).to_string()) {
                    queue.push_back((*user).to_string());
                }
            }
        }
    }
    impacted.into_iter().collect()
}

fn integration_reachability_to_impacted(
    workspace_root: &Path,
    manifest: &CertifiedImpactManifest,
    impacted_src_modules: &[String],
) -> HashMap<String, bool> {
    let tests_root = workspace_root.join("tests");
    if !tests_root.is_dir() {
        return HashMap::new();
    }
    let mut module_sources = Vec::new();
    if collect_wr_modules(&tests_root, &tests_root, "tests", &mut module_sources).is_err() {
        return HashMap::new();
    }

    let src_module_set: HashSet<&str> = manifest
        .src_modules
        .iter()
        .map(|module| module.module_path.as_str())
        .collect();
    let test_module_set: HashSet<&str> = module_sources
        .iter()
        .map(|module| module.module_path.as_str())
        .collect();
    let known_modules: HashSet<&str> = src_module_set
        .iter()
        .copied()
        .chain(test_module_set.iter().copied())
        .collect();
    let mut adjacency: HashMap<String, Vec<String>> = HashMap::new();
    for module in &manifest.src_modules {
        let deps = module
            .uses
            .iter()
            .filter(|dep| known_modules.contains(dep.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        adjacency.insert(module.module_path.clone(), deps);
    }
    for module in &module_sources {
        let deps = module
            .uses
            .iter()
            .filter(|dep| known_modules.contains(dep.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        adjacency.insert(module.module_path.clone(), deps);
    }

    let impacted: HashSet<&str> = impacted_src_modules
        .iter()
        .map(|module| module.as_str())
        .collect();
    let mut result = HashMap::new();
    for module in module_sources {
        if infer_test_lane(&module.module_path) != TestLane::Integration {
            continue;
        }
        result.insert(
            module.module_path.clone(),
            module_reaches_impacted(&module.module_path, &adjacency, &impacted),
        );
    }
    result
}

fn module_reaches_impacted(
    start: &str,
    adjacency: &HashMap<String, Vec<String>>,
    impacted: &HashSet<&str>,
) -> bool {
    let mut queue = VecDeque::new();
    let mut seen: HashSet<String> = HashSet::new();
    queue.push_back(start.to_string());
    seen.insert(start.to_string());
    while let Some(module) = queue.pop_front() {
        if impacted.contains(module.as_str()) {
            return true;
        }
        if let Some(deps) = adjacency.get(&module) {
            for dep in deps {
                if seen.insert(dep.clone()) {
                    queue.push_back(dep.clone());
                }
            }
        }
    }
    false
}

fn parse_cargo_package_version(cargo_toml: &str) -> Option<String> {
    let mut in_package = false;
    for raw in cargo_toml.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            in_package = line == "[package]";
            continue;
        }
        if !in_package {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != "version" {
            continue;
        }
        let trimmed = value.trim();
        let unquoted = trimmed.strip_prefix('"')?.strip_suffix('"')?;
        return Some(unquoted.to_string());
    }
    None
}

fn resolve_budget_policy_v1(
    test_jobs: Option<usize>,
    test_timeout_ms: Option<u64>,
) -> BudgetPolicyV1 {
    BudgetPolicyV1 {
        policy_version: BUDGET_POLICY_VERSION,
        test_jobs: resolve_budget_value(
            DEFAULT_TEST_JOBS,
            CEILING_TEST_JOBS,
            test_jobs.map(|v| (v as u64, "--jobs")),
            "WRELA_BUDGET_TEST_JOBS",
        ),
        test_timeout_ms: resolve_budget_value(
            DEFAULT_TEST_TIMEOUT_MS,
            CEILING_TEST_TIMEOUT_MS,
            test_timeout_ms.map(|v| (v, "--test-timeout-ms")),
            "WRELA_BUDGET_TEST_TIMEOUT_MS",
        ),
        autogen_max_cases: resolve_budget_value(
            DEFAULT_AUTOGEN_MAX_CASES,
            CEILING_AUTOGEN_MAX_CASES,
            None,
            "WRELA_BUDGET_AUTOGEN_MAX_CASES",
        ),
        sim_max_cases: resolve_budget_value(
            DEFAULT_SIM_MAX_CASES,
            CEILING_SIM_MAX_CASES,
            None,
            "WRELA_BUDGET_SIM_MAX_CASES",
        ),
        fuzz_max_cases: resolve_budget_value(
            DEFAULT_FUZZ_MAX_CASES,
            CEILING_FUZZ_MAX_CASES,
            None,
            "WRELA_BUDGET_FUZZ_MAX_CASES",
        ),
        mutation_max_cases: resolve_budget_value(
            DEFAULT_MUTATION_MAX_CASES,
            CEILING_MUTATION_MAX_CASES,
            None,
            "WRELA_BUDGET_MUTATION_MAX_CASES",
        ),
        autogen_time_cap_ms: resolve_budget_value(
            DEFAULT_AUTOGEN_TIME_CAP_MS,
            CEILING_AUTOGEN_TIME_CAP_MS,
            None,
            "WRELA_BUDGET_AUTOGEN_TIME_CAP_MS",
        ),
        sim_time_cap_ms: resolve_budget_value(
            DEFAULT_SIM_TIME_CAP_MS,
            CEILING_SIM_TIME_CAP_MS,
            None,
            "WRELA_BUDGET_SIM_TIME_CAP_MS",
        ),
        fuzz_time_cap_ms: resolve_budget_value(
            DEFAULT_FUZZ_TIME_CAP_MS,
            CEILING_FUZZ_TIME_CAP_MS,
            None,
            "WRELA_BUDGET_FUZZ_TIME_CAP_MS",
        ),
        mutation_time_cap_ms: resolve_budget_value(
            DEFAULT_MUTATION_TIME_CAP_MS,
            CEILING_MUTATION_TIME_CAP_MS,
            None,
            "WRELA_BUDGET_MUTATION_TIME_CAP_MS",
        ),
    }
}

fn resolve_budget_value(
    default: u64,
    ceiling: u64,
    cli_override: Option<(u64, &str)>,
    env_key: &str,
) -> BudgetValue {
    if let Some((requested, key)) = cli_override {
        return budget_value(default, ceiling, requested, "cli", key);
    }
    if let Some(requested) = parse_budget_env_u64(env_key) {
        return budget_value(default, ceiling, requested, "env", env_key);
    }
    budget_value(default, ceiling, default, "default", "hardcoded")
}

fn parse_budget_env_u64(key: &str) -> Option<u64> {
    std::env::var(key).ok()?.trim().parse::<u64>().ok()
}

fn budget_value(
    default: u64,
    ceiling: u64,
    requested: u64,
    source: &str,
    key: &str,
) -> BudgetValue {
    let requested = requested.max(1);
    BudgetValue {
        value: requested.min(ceiling),
        default,
        ceiling,
        provenance: BudgetProvenance {
            source: source.to_string(),
            key: key.to_string(),
            requested,
            clamped_to_ceiling: requested > ceiling,
        },
    }
}

fn verify_certification_report(cert_path: &Path) -> Result<(), String> {
    if !cert_path.exists() {
        return Err(format!(
            "verify-cert failed:\n  - cert path not found: {}",
            cert_path.display()
        ));
    }

    let payload = fs::read_to_string(cert_path).map_err(|err| {
        format!(
            "verify-cert failed:\n  - failed to read cert {}: {}",
            cert_path.display(),
            err
        )
    })?;
    let cert_json: serde_json::Value = serde_json::from_str(&payload).map_err(|err| {
        format!(
            "verify-cert failed:\n  - invalid cert JSON at {}: {}",
            cert_path.display(),
            err
        )
    })?;
    let cert_schema_version = cert_json
        .get("cert_schema_version")
        .and_then(serde_json::Value::as_u64);
    if cert_schema_version != Some(CERT_SCHEMA_VERSION as u64) {
        let got = cert_schema_version
            .map(|value| value.to_string())
            .unwrap_or_else(|| "missing".to_string());
        return Err(format!(
            "verify-cert failed:\n  - schema mismatch: expected {} but got {}",
            CERT_SCHEMA_VERSION, got
        ));
    }

    let report: CertificationReport = serde_json::from_value(cert_json).map_err(|err| {
        format!(
            "verify-cert failed:\n  - cert schema {} parse error: {}",
            CERT_SCHEMA_VERSION, err
        )
    })?;

    let mut failures: Vec<String> = Vec::new();
    if report.gate_versions_marker != CERT_GATE_VERSIONS_MARKER {
        failures.push(format!(
            "gate versions marker mismatch: expected '{}' but got '{}'",
            CERT_GATE_VERSIONS_MARKER, report.gate_versions_marker
        ));
    }
    if report.compiler_version.trim().is_empty() {
        failures.push("compiler version is empty".to_string());
    }
    if report.runtime_version.trim().is_empty() {
        failures.push("runtime version is empty".to_string());
    }

    let cert_dir = cert_path.parent().unwrap_or_else(|| Path::new("."));
    let artifact_path = resolve_cert_path(&report.artifact_path, cert_dir);
    if !artifact_path.exists() {
        failures.push(format!("binary path missing: {}", artifact_path.display()));
    } else {
        match hash_file_fingerprint(&artifact_path) {
            Ok(actual_binary_hash) => {
                if actual_binary_hash != report.binary_hash {
                    failures.push(format!(
                        "binary hash mismatch: expected {} but got {} ({})",
                        report.binary_hash,
                        actual_binary_hash,
                        artifact_path.display()
                    ));
                }
            }
            Err(err) => failures.push(format!("binary hash failed: {err}")),
        }
    }

    let workspace_root = resolve_cert_path(&report.workspace_root, cert_dir);
    if workspace_root.exists() {
        match hash_source_fingerprint(&workspace_root) {
            Ok(actual_source_hash) => {
                if actual_source_hash != report.source_hash {
                    failures.push(format!(
                        "source hash mismatch: expected {} but got {} ({})",
                        report.source_hash,
                        actual_source_hash,
                        workspace_root.display()
                    ));
                }
            }
            Err(err) => failures.push(format!("source hash failed: {err}")),
        }
    } else if !report.workspace_root.trim().is_empty() {
        failures.push(format!(
            "workspace root missing for source hash verification: {}",
            workspace_root.display()
        ));
    }

    if failures.is_empty() {
        return Ok(());
    }
    let body = failures
        .into_iter()
        .map(|line| format!("  - {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    Err(format!("verify-cert failed:\n{body}"))
}

fn resolve_cert_path(raw: &str, cert_dir: &Path) -> PathBuf {
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        path
    } else {
        cert_dir.join(path)
    }
}

fn write_test_maintenance_summary(
    workspace_root: &Path,
    mode_record: bool,
    mode_update_public_surface: bool,
    exit_code: i32,
) -> Result<(), String> {
    let generated_at_unix_ms = now_unix_ms();
    let summary = TestMaintenanceSummary {
        version: 1,
        generated_at_unix_ms,
        workspace_root: workspace_root.display().to_string(),
        mode_record,
        mode_update_public_surface,
        exit_code,
        deployable_artifacts_emitted: false,
    };
    let payload = serde_json::to_vec_pretty(&summary).map_err(|err| err.to_string())?;
    let artifact_dir = workspace_root
        .join("tests")
        .join(".artifacts")
        .join("maintenance");
    fs::create_dir_all(&artifact_dir).map_err(|err| {
        format!(
            "failed to create maintenance artifact directory {}: {}",
            artifact_dir.display(),
            err
        )
    })?;
    let summary_path = artifact_dir.join(format!("maintenance-{}.json", generated_at_unix_ms));
    let latest_path = artifact_dir.join("maintenance-latest.json");
    fs::write(&summary_path, &payload).map_err(|err| {
        format!(
            "failed to write maintenance summary {}: {}",
            summary_path.display(),
            err
        )
    })?;
    fs::write(&latest_path, payload).map_err(|err| {
        format!(
            "failed to write maintenance latest summary {}: {}",
            latest_path.display(),
            err
        )
    })?;
    Ok(())
}

fn hash_file_fingerprint(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path)
        .map_err(|err| format!("failed to read hash input {}: {}", path.display(), err))?;
    Ok(fnv1a64_hex(&bytes))
}

fn hash_source_fingerprint(workspace_root: &Path) -> Result<String, String> {
    let mut files: Vec<(String, PathBuf)> = Vec::new();
    collect_hash_files(&workspace_root.join("src"), "src", "wr", &mut files)?;
    collect_hash_files(&workspace_root.join("tests"), "tests", "wr", &mut files)?;
    collect_hash_files(
        &workspace_root.join("tests").join("cassettes"),
        "tests/cassettes",
        "json",
        &mut files,
    )?;
    files.sort_by(|a, b| a.0.cmp(&b.0));
    let mut hasher = Fnv1a64::new();
    for (rel, path) in files {
        hasher.update(b"file:");
        hasher.update(rel.as_bytes());
        hasher.update(&[0]);
        let bytes = fs::read(&path).map_err(|err| {
            format!(
                "failed to read source hash input {}: {}",
                path.display(),
                err
            )
        })?;
        hasher.update(&(bytes.len() as u64).to_le_bytes());
        hasher.update(&bytes);
    }
    Ok(hasher.finish_hex())
}

fn collect_hash_files(
    dir: &Path,
    dir_label: &str,
    extension: &str,
    out: &mut Vec<(String, PathBuf)>,
) -> Result<(), String> {
    if !dir.exists() {
        return Ok(());
    }
    let entries = fs::read_dir(dir)
        .map_err(|err| format!("failed to read source directory {}: {}", dir.display(), err))?;
    let mut children: Vec<PathBuf> = entries
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("failed to list source directory {}: {}", dir.display(), err))?
        .into_iter()
        .map(|entry| entry.path())
        .collect();
    children.sort_by(|a, b| path_sort_key(a).cmp(&path_sort_key(b)));
    for child in children {
        if child.is_dir() {
            let child_name = child
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| format!("non-utf8 path in source tree: {}", child.display()))?;
            let next_label = format!("{dir_label}/{child_name}");
            collect_hash_files(&child, &next_label, extension, out)?;
        } else if child.is_file() {
            if child.extension().and_then(|ext| ext.to_str()) != Some(extension) {
                continue;
            }
            let child_name = child
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| format!("non-utf8 path in source tree: {}", child.display()))?;
            out.push((format!("{dir_label}/{child_name}"), child));
        }
    }
    Ok(())
}

#[derive(Clone)]
struct WrModuleSource {
    module_path: String,
    rel_path: String,
    source: String,
    hash: String,
    uses: Vec<String>,
}

fn build_certified_impact_manifest(
    workspace_root: &Path,
) -> Result<CertifiedImpactManifest, String> {
    let mut files: Vec<(String, PathBuf)> = Vec::new();
    collect_hash_files(&workspace_root.join("src"), "src", "wr", &mut files)?;
    collect_hash_files(&workspace_root.join("tests"), "tests", "wr", &mut files)?;
    collect_hash_files(
        &workspace_root.join("tests").join("cassettes"),
        "tests/cassettes",
        "json",
        &mut files,
    )?;
    files.sort_by(|a, b| a.0.cmp(&b.0));
    let source_files = files
        .iter()
        .map(|(rel_path, path)| {
            let hash = hash_file_fingerprint(path)?;
            Ok(CertifiedSourceFileFingerprint {
                rel_path: rel_path.clone(),
                hash,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    let src_root = workspace_root.join("src");
    let mut src_modules = Vec::new();
    collect_wr_modules(&src_root, &src_root, "src", &mut src_modules)?;
    let mut src_snapshots = src_modules
        .into_iter()
        .map(|module| CertifiedSrcModuleSnapshot {
            module_path: module.module_path,
            rel_path: module.rel_path,
            hash: module.hash,
            uses: module.uses,
            runtime_sensitive: source_looks_runtime_sensitive(&module.source),
        })
        .collect::<Vec<_>>();
    src_snapshots.sort_by(|a, b| a.module_path.cmp(&b.module_path));

    Ok(CertifiedImpactManifest {
        source_files,
        src_modules: src_snapshots,
    })
}

fn collect_wr_modules(
    root: &Path,
    strip_root: &Path,
    root_label: &str,
    out: &mut Vec<WrModuleSource>,
) -> Result<(), String> {
    if !root.exists() {
        return Ok(());
    }
    let entries = fs::read_dir(root).map_err(|err| {
        format!(
            "failed to read source directory {}: {}",
            root.display(),
            err
        )
    })?;
    let mut children: Vec<PathBuf> = entries
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| {
            format!(
                "failed to list source directory {}: {}",
                root.display(),
                err
            )
        })?
        .into_iter()
        .map(|entry| entry.path())
        .collect();
    children.sort_by(|a, b| path_sort_key(a).cmp(&path_sort_key(b)));
    for child in children {
        if child.is_dir() {
            collect_wr_modules(&child, strip_root, root_label, out)?;
            continue;
        }
        if child.extension().and_then(|ext| ext.to_str()) != Some("wr") {
            continue;
        }
        let source = fs::read_to_string(&child)
            .map_err(|err| format!("failed to read source file {}: {}", child.display(), err))?;
        let hash = fnv1a64_hex(source.as_bytes());
        let module_path = module_path_for_wr_file(&child, strip_root, root_label)?;
        let rel = child.strip_prefix(strip_root).map_err(|_| {
            format!(
                "file {} must live under {}",
                child.display(),
                strip_root.display()
            )
        })?;
        let rel_path = format!(
            "{}/{}",
            root_label,
            rel.to_string_lossy().replace('\\', "/")
        );
        let uses = parse_wr_use_edges(&source);
        out.push(WrModuleSource {
            module_path,
            rel_path,
            source,
            hash,
            uses,
        });
    }
    Ok(())
}

fn module_path_for_wr_file(path: &Path, root: &Path, root_label: &str) -> Result<String, String> {
    let rel = path
        .strip_prefix(root)
        .map_err(|_| format!("file {} must live under {}", path.display(), root.display()))?;
    let mut rel = rel.to_path_buf();
    rel.set_extension("");
    let parts: Vec<String> = rel
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .filter(|segment| !segment.is_empty())
        .collect();
    if root_label == "tests" {
        Ok(format!("tests/{}", parts.join("/")))
    } else {
        Ok(parts.join("/"))
    }
}

fn parse_wr_use_edges(source: &str) -> Vec<String> {
    use wrela::parser::ast::AstNode;

    let (syntax, parse_errors) = parser::parse_with_errors(source);
    if !parse_errors.is_empty() {
        return Vec::new();
    }
    let Some(root) = parser::ast::Root::cast(syntax) else {
        return Vec::new();
    };
    let module = hir::lower::lower(root);
    let mut uses = module
        .uses
        .iter()
        .map(|use_stmt| use_stmt.module.to_string())
        .filter(|module| !module.trim().is_empty())
        .collect::<Vec<_>>();
    uses.sort();
    uses.dedup();
    uses
}

fn source_looks_runtime_sensitive(source: &str) -> bool {
    let normalized = source.to_ascii_lowercase();
    [
        "actor", "pool", "runtime", "__wr_", "detach", "mailbox", "sched_",
    ]
    .iter()
    .any(|marker| normalized.contains(marker))
}

fn enforce_public_surface_gate(workspace_root: &Path) -> Result<(), String> {
    let snapshot = build_public_surface_snapshot(workspace_root)?;
    let current_path = workspace_root.join(PUBLIC_SURFACE_CURRENT_REL_PATH);
    write_public_surface_snapshot(&current_path, &snapshot)?;
    let baseline_path = workspace_root.join(PUBLIC_SURFACE_BASELINE_REL_PATH);
    if !baseline_path.is_file() {
        return Ok(());
    }
    let baseline = load_public_surface_snapshot(&baseline_path)?;
    if baseline == snapshot {
        return Ok(());
    }
    let summary = summarize_public_surface_diff(&baseline, &snapshot);
    Err(format!(
        "public surface gate failed:\n  baseline: {}\n  current: {}\n{}\nrun `wrela test --update-public-surface` to accept the new public surface",
        baseline_path.display(),
        current_path.display(),
        summary
    ))
}

fn enforce_importable_coverage_gate(
    workspace_root: &Path,
    function_coverage: &BTreeMap<String, u64>,
) -> Result<(), String> {
    let snapshot = build_public_surface_snapshot(workspace_root)?;
    let mut uncovered = snapshot
        .items
        .iter()
        .filter(|item| is_importable_coverage_target(&item.qualified_name))
        .filter_map(|item| {
            let function_id = stable_function_id(&item.qualified_name);
            let hits = function_coverage.get(&function_id).copied().unwrap_or(0);
            (hits == 0).then_some(item.qualified_name.clone())
        })
        .collect::<Vec<_>>();
    uncovered.sort();
    if uncovered.is_empty() {
        return Ok(());
    }
    let details = uncovered
        .iter()
        .map(|name| format!("  - {name}"))
        .collect::<Vec<_>>()
        .join("\n");
    Err(format!(
        "coverage gate failed: expected 100% function coverage for importable items under src/domain/** and src/application/**.\nuncovered importable functions/checks ({}):\n{}\naction: add tests that execute each uncovered item, or mark it private if it is internal-only",
        uncovered.len(),
        details
    ))
}

fn is_importable_coverage_target(qualified_name: &str) -> bool {
    qualified_name.starts_with("domain/") || qualified_name.starts_with("application/")
}

fn update_public_surface_baseline(workspace_root: &Path) -> Result<(), String> {
    let snapshot = build_public_surface_snapshot(workspace_root)?;
    let current_path = workspace_root.join(PUBLIC_SURFACE_CURRENT_REL_PATH);
    write_public_surface_snapshot(&current_path, &snapshot)?;
    let baseline_path = workspace_root.join(PUBLIC_SURFACE_BASELINE_REL_PATH);
    write_public_surface_snapshot(&baseline_path, &snapshot)?;
    println!(
        "public surface baseline updated: {}",
        baseline_path.display()
    );
    Ok(())
}

fn build_public_surface_snapshot(workspace_root: &Path) -> Result<PublicSurfaceSnapshot, String> {
    use wrela::parser::ast::AstNode;

    let src_root = workspace_root.join("src");
    let mut modules = Vec::new();
    collect_wr_modules(&src_root, &src_root, "src", &mut modules)?;
    modules.sort_by(|a, b| a.module_path.cmp(&b.module_path));
    let mut items = Vec::new();
    for module in modules {
        let (syntax, parse_errors) = parser::parse_with_errors(&module.source);
        if !parse_errors.is_empty() {
            continue;
        }
        let Some(root) = parser::ast::Root::cast(syntax) else {
            continue;
        };
        let lowered = hir::lower::lower(root);
        for (_, function) in lowered.functions.iter() {
            if function.visibility != hir::Visibility::Public {
                continue;
            }
            if !matches!(
                function.kind,
                hir::FunctionKind::Function | hir::FunctionKind::Check
            ) {
                continue;
            }
            let qualified_name = format!("{}::{}", module.module_path, function.name);
            let signature = render_public_function_signature(function);
            let connector_literals = function
                .body
                .as_ref()
                .map(collect_public_surface_connector_literals)
                .unwrap_or_default();
            items.push(PublicSurfaceItem {
                qualified_name,
                signature,
                connector_literals,
            });
        }
    }
    items.sort_by(|a, b| a.qualified_name.cmp(&b.qualified_name));
    Ok(PublicSurfaceSnapshot { version: 1, items })
}

fn render_public_function_signature(function: &hir::Function) -> String {
    let params = function
        .params
        .iter()
        .map(|param| {
            let ty = param
                .ty
                .as_ref()
                .map(render_public_surface_type)
                .unwrap_or_else(|| "_".to_string());
            format!("{}: {}", param.name, ty)
        })
        .collect::<Vec<_>>();
    let ret = function
        .ret_type
        .as_ref()
        .map(render_public_surface_type)
        .unwrap_or_else(|| "Nothing".to_string());
    format!("({}) -> {ret}", params.join(", "))
}

fn render_public_surface_type(ty: &hir::TypeRef) -> String {
    if ty.args.is_empty() {
        return ty.name.to_string();
    }
    let args = ty
        .args
        .iter()
        .map(render_public_surface_type)
        .collect::<Vec<_>>()
        .join(", ");
    format!("{}[{args}]", ty.name)
}

fn collect_public_surface_connector_literals(
    body: &hir::Body,
) -> Vec<PublicSurfaceConnectorLiteral> {
    let mut literals = BTreeSet::new();
    collect_public_surface_connector_literals_from_stmts(body, &body.root_stmts, &mut literals);
    literals.into_iter().collect()
}

fn collect_public_surface_connector_literals_from_stmts(
    body: &hir::Body,
    stmts: &[hir::arena::Idx<hir::Stmt>],
    out: &mut BTreeSet<PublicSurfaceConnectorLiteral>,
) {
    for stmt_idx in stmts {
        match &body.stmts[*stmt_idx] {
            hir::Stmt::Expr(expr)
            | hir::Stmt::IgnoreResult { expr }
            | hir::Stmt::Capture { value: expr, .. }
            | hir::Stmt::Require {
                condition: expr, ..
            } => {
                collect_public_surface_connector_literals_from_expr(body, *expr, out);
            }
            hir::Stmt::Assert { expr, .. } => {
                collect_public_surface_connector_literals_from_expr(body, *expr, out);
            }
            hir::Stmt::Let { value, .. } | hir::Stmt::Assign { value, .. } => {
                collect_public_surface_connector_literals_from_expr(body, *value, out);
            }
            hir::Stmt::Optimize { body: block, .. } => {
                collect_public_surface_connector_literals_from_stmts(body, block, out);
            }
            hir::Stmt::If {
                condition,
                then_branch,
                else_branch,
            } => {
                collect_public_surface_connector_literals_from_expr(body, *condition, out);
                collect_public_surface_connector_literals_from_stmts(body, then_branch, out);
                if let Some(else_branch) = else_branch {
                    collect_public_surface_connector_literals_from_stmts(body, else_branch, out);
                }
            }
            hir::Stmt::For {
                iterable,
                body: block,
                ..
            } => {
                collect_public_surface_connector_literals_from_expr(body, *iterable, out);
                collect_public_surface_connector_literals_from_stmts(body, block, out);
            }
            hir::Stmt::Match {
                subject,
                cases,
                otherwise,
            } => {
                collect_public_surface_connector_literals_from_expr(body, *subject, out);
                for case in cases {
                    collect_public_surface_connector_literals_from_stmts(body, &case.body, out);
                }
                if let Some(otherwise) = otherwise {
                    collect_public_surface_connector_literals_from_stmts(body, otherwise, out);
                }
            }
            hir::Stmt::While {
                condition,
                body: block,
            } => {
                collect_public_surface_connector_literals_from_expr(body, *condition, out);
                collect_public_surface_connector_literals_from_stmts(body, block, out);
            }
            hir::Stmt::Return(Some(value)) | hir::Stmt::Defer { expr: value } => {
                collect_public_surface_connector_literals_from_expr(body, *value, out);
            }
            hir::Stmt::Return(None)
            | hir::Stmt::Use { .. }
            | hir::Stmt::Break
            | hir::Stmt::Continue => {}
        }
    }
}

fn collect_public_surface_connector_literals_from_expr(
    body: &hir::Body,
    expr_idx: hir::arena::Idx<hir::Expr>,
    out: &mut BTreeSet<PublicSurfaceConnectorLiteral>,
) {
    match &body.exprs[expr_idx] {
        hir::Expr::Literal(_) | hir::Expr::Variable(_) => {}
        hir::Expr::Detach { target, .. }
        | hir::Expr::Unary { expr: target, .. }
        | hir::Expr::TypeApply { callee: target, .. }
        | hir::Expr::Crash { expr: target } => {
            collect_public_surface_connector_literals_from_expr(body, *target, out);
        }
        hir::Expr::Binary { lhs, rhs, .. } => {
            collect_public_surface_connector_literals_from_expr(body, *lhs, out);
            collect_public_surface_connector_literals_from_expr(body, *rhs, out);
        }
        hir::Expr::Call { callee, args, .. } | hir::Expr::GivenCall { callee, args, .. } => {
            if is_try_to_http_call(body, *callee)
                && let Some(literal) = extract_try_to_http_literal_tuple(body, args)
            {
                out.insert(literal);
            }
            collect_public_surface_connector_literals_from_expr(body, *callee, out);
            for arg in args {
                match arg {
                    hir::Arg::Positional { value, .. } | hir::Arg::Named { value, .. } => {
                        collect_public_surface_connector_literals_from_expr(body, *value, out);
                    }
                }
            }
        }
        hir::Expr::Member { object, .. } => {
            collect_public_surface_connector_literals_from_expr(body, *object, out);
        }
        hir::Expr::List(items) => {
            for item in items {
                collect_public_surface_connector_literals_from_expr(body, *item, out);
            }
        }
        hir::Expr::Map(entries) => {
            for (key, value) in entries {
                collect_public_surface_connector_literals_from_expr(body, *key, out);
                collect_public_surface_connector_literals_from_expr(body, *value, out);
            }
        }
        hir::Expr::StringInterp(parts) => {
            for part in parts {
                if let hir::StringPart::Expr(expr) = part {
                    collect_public_surface_connector_literals_from_expr(body, *expr, out);
                }
            }
        }
    }
}

fn is_try_to_http_call(body: &hir::Body, callee: hir::arena::Idx<hir::Expr>) -> bool {
    match &body.exprs[callee] {
        hir::Expr::Variable(name) => name == "try_to_http_call",
        hir::Expr::TypeApply { callee, .. } => is_try_to_http_call(body, *callee),
        _ => false,
    }
}

fn extract_try_to_http_literal_tuple(
    body: &hir::Body,
    args: &[hir::Arg],
) -> Option<PublicSurfaceConnectorLiteral> {
    let positional = args
        .iter()
        .filter_map(|arg| match arg {
            hir::Arg::Positional { value, .. } => Some(*value),
            hir::Arg::Named { .. } => None,
        })
        .collect::<Vec<_>>();
    if positional.len() < 4 {
        return None;
    }
    let service = extract_literal_string(body, positional[0])?;
    let endpoint = extract_literal_string(body, positional[1])?;
    let method = extract_literal_string(body, positional[2])?;
    let url = extract_literal_string(body, positional[3])?;
    Some(PublicSurfaceConnectorLiteral {
        service,
        endpoint,
        method,
        url,
    })
}

fn extract_literal_string(body: &hir::Body, expr: hir::arena::Idx<hir::Expr>) -> Option<String> {
    match &body.exprs[expr] {
        hir::Expr::Literal(hir::Literal::String(value)) => Some(value.to_string()),
        _ => None,
    }
}

fn write_public_surface_snapshot(
    path: &Path,
    snapshot: &PublicSurfaceSnapshot,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create {}: {}", parent.display(), err))?;
    }
    let bytes = serde_json::to_vec_pretty(snapshot).map_err(|err| err.to_string())?;
    fs::write(path, bytes).map_err(|err| format!("failed to write {}: {}", path.display(), err))
}

fn load_public_surface_snapshot(path: &Path) -> Result<PublicSurfaceSnapshot, String> {
    let bytes =
        fs::read(path).map_err(|err| format!("failed to read {}: {}", path.display(), err))?;
    serde_json::from_slice::<PublicSurfaceSnapshot>(&bytes)
        .map_err(|err| format!("failed to parse {}: {}", path.display(), err))
}

fn summarize_public_surface_diff(
    baseline: &PublicSurfaceSnapshot,
    current: &PublicSurfaceSnapshot,
) -> String {
    let baseline_by_name = baseline
        .items
        .iter()
        .map(|item| (item.qualified_name.as_str(), item))
        .collect::<BTreeMap<_, _>>();
    let current_by_name = current
        .items
        .iter()
        .map(|item| (item.qualified_name.as_str(), item))
        .collect::<BTreeMap<_, _>>();
    let added = current_by_name
        .keys()
        .filter(|name| !baseline_by_name.contains_key(*name))
        .copied()
        .collect::<Vec<_>>();
    let removed = baseline_by_name
        .keys()
        .filter(|name| !current_by_name.contains_key(*name))
        .copied()
        .collect::<Vec<_>>();
    let changed = baseline_by_name
        .iter()
        .filter_map(|(name, baseline_item)| {
            let current_item = current_by_name.get(name)?;
            if *baseline_item == *current_item {
                None
            } else {
                Some((*name, *baseline_item, *current_item))
            }
        })
        .collect::<Vec<_>>();

    let mut lines = Vec::new();
    if !added.is_empty() {
        lines.push(format!("  added importable items ({}):", added.len()));
        lines.extend(added.into_iter().map(|name| format!("    + {name}")));
    }
    if !removed.is_empty() {
        lines.push(format!("  removed importable items ({}):", removed.len()));
        lines.extend(removed.into_iter().map(|name| format!("    - {name}")));
    }
    if !changed.is_empty() {
        lines.push(format!("  changed importable items ({}):", changed.len()));
        for (name, baseline_item, current_item) in changed {
            lines.push(format!("    ~ {name}"));
            if baseline_item.signature != current_item.signature {
                lines.push(format!(
                    "      signature: {} -> {}",
                    baseline_item.signature, current_item.signature
                ));
            }
            if baseline_item.connector_literals != current_item.connector_literals {
                lines.push(format!(
                    "      connector_literals: {} -> {}",
                    baseline_item.connector_literals.len(),
                    current_item.connector_literals.len()
                ));
            }
        }
    }
    if lines.is_empty() {
        lines.push("  public surface changed (unable to summarize details)".to_string());
    }
    lines.join("\n")
}

fn evaluate_connector_contract_gate(workspace_root: &Path) -> Result<(), String> {
    let root = fs::canonicalize(workspace_root).unwrap_or_else(|_| workspace_root.to_path_buf());
    let cassette_root = root.join("tests").join("cassettes");
    if !cassette_root.is_dir() {
        return Ok(());
    }
    let mut cassette_files = Vec::new();
    collect_json_files_recursive(&cassette_root, &mut cassette_files)?;
    if cassette_files.is_empty() {
        return Ok(());
    }
    let mut coverage: std::collections::BTreeMap<(String, String), (bool, bool)> =
        std::collections::BTreeMap::new();
    for file in cassette_files {
        let bytes = fs::read(&file)
            .map_err(|err| format!("failed to read cassette {}: {err}", file.display()))?;
        let cassette: ConnectorCoverageCassette = serde_json::from_slice(&bytes)
            .map_err(|err| format!("invalid cassette schema in {}: {err}", file.display()))?;
        let key = (
            cassette.request.service.clone(),
            cassette.request.endpoint.clone(),
        );
        let entry = coverage.entry(key).or_insert((false, false));
        if cassette.response.status < 400 {
            entry.0 = true;
        } else {
            entry.1 = true;
        }
    }

    let mut missing = Vec::new();
    for ((service, endpoint), (has_success, has_failure)) in coverage {
        if !has_success || !has_failure {
            missing.push(format!(
                "  - {service}/{endpoint}: success_replay={} failure_replay={}",
                has_success, has_failure
            ));
        }
    }
    if missing.is_empty() {
        return Ok(());
    }
    Err(format!(
        "connector contract coverage requires both success and failure replay cassettes per endpoint:\n{}",
        missing.join("\n")
    ))
}

fn collect_json_files_recursive(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries =
        fs::read_dir(dir).map_err(|err| format!("failed to read {}: {err}", dir.display()))?;
    let mut children: Vec<PathBuf> = entries
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("failed to list {}: {err}", dir.display()))?
        .into_iter()
        .map(|entry| entry.path())
        .collect();
    children.sort_by(|a, b| path_sort_key(a).cmp(&path_sort_key(b)));
    for child in children {
        if child.is_dir() {
            collect_json_files_recursive(&child, out)?;
        } else if child.extension().and_then(|ext| ext.to_str()) == Some("json") {
            out.push(child);
        }
    }
    Ok(())
}

fn path_sort_key(path: &Path) -> (usize, String) {
    let rank = match (path.is_file(), path.is_dir()) {
        (true, _) => 0,
        (_, true) => 1,
        _ => 2,
    };
    (rank, path.to_string_lossy().to_string())
}

const FNV1A64_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
const FNV1A64_PRIME: u64 = 0x100000001b3;

struct Fnv1a64 {
    state: u64,
}

impl Fnv1a64 {
    fn new() -> Self {
        Self {
            state: FNV1A64_OFFSET_BASIS,
        }
    }

    fn update(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.state ^= *byte as u64;
            self.state = self.state.wrapping_mul(FNV1A64_PRIME);
        }
    }

    fn finish_hex(&self) -> String {
        format!("{:016x}", self.state)
    }

    fn finish_u64(&self) -> u64 {
        self.state
    }
}

fn fnv1a64_hex(bytes: &[u8]) -> String {
    let mut hasher = Fnv1a64::new();
    hasher.update(bytes);
    hasher.finish_hex()
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hasher = Fnv1a64::new();
    hasher.update(bytes);
    hasher.finish_u64()
}

#[derive(Serialize)]
struct JsonSpan {
    offset: usize,
    len: usize,
}

#[derive(Serialize)]
struct JsonDiag {
    kind: String,
    message: String,
    path: String,
    span: JsonSpan,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rule: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    help: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    suggestions: Option<Vec<JsonSuggestion>>,
}

#[derive(Serialize)]
struct JsonSuggestion {
    replacement: String,
    span: JsonSpan,
    rationale: String,
    confidence: f64,
}

#[derive(Default)]
struct JsonDiagMetadata {
    code: Option<String>,
    rule: Option<String>,
    help: Option<String>,
    suggestions: Option<Vec<JsonSuggestion>>,
}

fn emit_diag(
    format: OutputFormat,
    kind: &str,
    message: String,
    span: SourceSpan,
    path: String,
    source: String,
) {
    match format {
        OutputFormat::Pretty => {
            let report = Report::new(ProjectDiag { message, span })
                .with_source_code(NamedSource::new(path, source));
            if kind == "warning" {
                eprintln!("warning: {report:?}");
            } else {
                eprintln!("{report:?}");
            }
        }
        OutputFormat::Json => {
            emit_json_diag(kind, message, span, path);
        }
    }
}

fn emit_json_diag(kind: &str, message: String, span: SourceSpan, path: String) {
    emit_json_diag_with_metadata(kind, message, span, path, None);
}

fn emit_json_diag_with_metadata(
    kind: &str,
    message: String,
    span: SourceSpan,
    path: String,
    metadata: Option<JsonDiagMetadata>,
) {
    let span = JsonSpan {
        offset: span.offset(),
        len: span.len(),
    };
    let metadata = metadata.unwrap_or_default();
    let json = JsonDiag {
        kind: kind.to_string(),
        message,
        path,
        span,
        code: metadata.code,
        rule: metadata.rule,
        help: metadata.help,
        suggestions: metadata.suggestions,
    };
    println!(
        "{}",
        serde_json::to_string(&json).unwrap_or_else(|_| "{}".to_string())
    );
}

fn naming_rule_from_code(code: &str) -> Option<String> {
    let (_, rule) = code.split_once("lang::naming::")?;
    Some(rule.to_string())
}

fn extract_json_metadata(diag: &dyn Diagnostic) -> Option<JsonDiagMetadata> {
    let code = diag.code().map(|value| value.to_string())?;
    let rule = naming_rule_from_code(&code)?;
    let help = diag.help().map(|value| value.to_string());
    Some(JsonDiagMetadata {
        code: Some(code),
        rule: Some(rule),
        help,
        suggestions: Some(Vec::new()),
    })
}

fn emit_json_diag_for_diagnostic(
    kind: &str,
    diag: &dyn Diagnostic,
    span: SourceSpan,
    path: String,
) {
    let metadata = extract_json_metadata(diag);
    emit_json_diag_with_metadata(kind, diag.to_string(), span, path, metadata);
}

fn is_command(arg: &str) -> bool {
    matches!(
        arg,
        "init"
            | "update"
            | "check"
            | "build"
            | "compile"
            | "verify-cert"
            | "run"
            | "dev"
            | "test"
            | "perf"
            | "perfcmp"
            | "matrix"
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
    perf_summary: Option<PerfSummary>,
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
        "language/spec/spec.wr".to_string(),
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
            args: vec!["test".to_string(), "language/spec/spec.wr".to_string()],
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
    evidence.perf_summary = load_perf_baseline_summary(&perf_baseline_path).ok();
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

fn check_lane_kpis_from_summary(summary: &PerfSummary) -> CheckLaneKpis {
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

fn now_unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_millis()
}

enum TestTarget {
    ProjectRoot(PathBuf),
    SingleFile(PathBuf),
}

fn resolve_test_target(path_arg: Option<&str>) -> Result<TestTarget, String> {
    let path = PathBuf::from(path_arg.unwrap_or("."));
    if path.is_file() {
        if path.extension().and_then(|s| s.to_str()) == Some("wr") {
            return Ok(TestTarget::SingleFile(path));
        }
        return Err(format!(
            "test file must have .wr extension: {}",
            path.display()
        ));
    }
    if path.is_dir() {
        return Ok(TestTarget::ProjectRoot(path));
    }
    Err("test target must be an existing directory or .wr file".to_string())
}

fn resolve_benchmark_manifest_path(
    target: &TestTarget,
    override_path: Option<String>,
) -> Option<PathBuf> {
    if let Some(path) = override_path {
        return Some(PathBuf::from(path));
    }
    let TestTarget::ProjectRoot(root) = target else {
        return None;
    };
    let candidate = root.join("bench.toml");
    candidate.is_file().then_some(candidate)
}

fn load_benchmark_manifest(path: &Path) -> Result<BenchmarkManifest, String> {
    let text = fs::read_to_string(path)
        .map_err(|err| format!("failed to read {}: {}", path.display(), err))?;
    let manifest: BenchmarkManifest =
        toml::from_str(&text).map_err(|err| format!("failed to parse bench.toml: {err}"))?;
    if manifest.version != 1 {
        return Err(format!(
            "unsupported benchmark manifest version {}; expected 1",
            manifest.version
        ));
    }
    if manifest.scenarios.is_empty() {
        return Err("benchmark manifest must define at least one scenario".to_string());
    }
    for scenario in &manifest.scenarios {
        let func_name = scenario
            .test_name
            .rsplit("::")
            .next()
            .unwrap_or(scenario.test_name.as_str());
        let expected_suffix = format!("_ops_{}", scenario.ops);
        if !func_name.ends_with(expected_suffix.as_str()) {
            return Err(format!(
                "scenario `{}` test `{}` must end with `{}`",
                scenario.id, scenario.test_name, expected_suffix
            ));
        }
    }
    Ok(manifest)
}

fn discover_tests_for_target(target: &TestTarget) -> Result<Vec<TestCase>, String> {
    match target {
        TestTarget::ProjectRoot(root) => {
            let tests_root = root.join("tests");
            let mut tests = Vec::new();
            collect_tests(&tests_root, &tests_root, &mut tests).map_err(|err| err.to_string())?;
            tests.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
            Ok(tests)
        }
        TestTarget::SingleFile(_) => {
            Err("benchmark manifests require project-root targets with tests/".to_string())
        }
    }
}

fn build_benchmark_selection(
    target: &TestTarget,
    manifest_path: &Path,
    profile: PerfProfile,
) -> Result<HashSet<String>, String> {
    let manifest = load_benchmark_manifest(manifest_path)?;
    let tests = discover_tests_for_target(target)?;
    let test_by_name: HashMap<&str, &TestCase> = tests
        .iter()
        .map(|test| (test.name.as_str(), test))
        .collect();
    let mut include_ids = HashSet::new();
    for scenario in manifest.scenarios_for_profile(profile) {
        let Some(test) = test_by_name.get(scenario.test_name.as_str()) else {
            return Err(format!(
                "scenario `{}` references unknown test `{}`",
                scenario.id, scenario.test_name
            ));
        };
        include_ids.insert(test.id.clone());
    }
    if include_ids.is_empty() {
        return Err("benchmark profile selected zero scenarios".to_string());
    }
    Ok(include_ids)
}

fn profile_pair_counts(
    manifest: &BenchmarkManifest,
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
    if let Some(override_value) = warmup_override {
        warmup = override_value.max(1);
    }
    if let Some(override_value) = measure_override {
        measure = override_value.max(1);
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
    summary: PerfSummary,
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
    scenarios: &[&BenchmarkScenario],
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
    baseline: &PerfSummary,
    candidate: &PerfSummary,
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

fn run_perfcmp(config: &PerfCmpConfig) -> i32 {
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
    let mut gate_failures = Vec::new();
    let mut unstable_count = 0usize;
    let mut unstable_critical_count = 0usize;
    let mut win_count = 0usize;
    let mut regression_count = 0usize;
    let mut no_signal_count = 0usize;
    let mut bootstrap_seed = seed ^ 0x9e37_79b9_7f4a_7c15;

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
        let result = PerfCmpScenarioResult {
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
        };
        scenario_results.push(result);
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
    command.arg("--format=json");
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
    let summary = load_perf_baseline_summary(&report_path)?;
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

fn iqr_over_median(samples: &[u128]) -> f64 {
    if samples.len() < 4 {
        return 0.0;
    }
    let q1 = percentile(samples, 0.25);
    let q3 = percentile(samples, 0.75);
    let median = percentile(samples, 0.5).max(1);
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

#[derive(Clone)]
struct TestCase {
    id: String,
    lane: TestLane,
    name: String,
    module_path: String,
    func_name: String,
    is_serial: bool,
    allows_env_set: bool,
    allows_fs_escape: bool,
    has_oracle: bool,
    generated_call_body: Option<String>,
    generated_case_kind: Option<GeneratedCaseKind>,
    generated_entry_source: Option<String>,
    autogen_module_source: Option<String>,
    autogen_seed: Option<u64>,
    autogen_span: Option<String>,
    sim_seed: Option<u64>,
    canonical_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TestLane {
    Spec,
    Integration,
    Sim,
    Model,
    Default,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum GeneratedCaseKind {
    Autogen,
    Fuzz,
}

impl TestLane {
    fn as_str(self) -> &'static str {
        match self {
            TestLane::Spec => "spec",
            TestLane::Integration => "integration",
            TestLane::Sim => "sim",
            TestLane::Model => "model",
            TestLane::Default => "default",
        }
    }
}

#[derive(Clone, Copy)]
enum HttpCassetteMode {
    Replay,
    Record,
}

#[derive(Clone, Copy)]
enum DifferentialPipeline {
    Baseline,
    Alt,
}

impl DifferentialPipeline {
    fn as_env_value(self) -> &'static str {
        match self {
            DifferentialPipeline::Baseline => "baseline",
            DifferentialPipeline::Alt => "alt",
        }
    }
}

#[derive(Clone)]
struct AutogenCheckDecl {
    module_path: String,
    func_name: String,
    params: Vec<AutogenCheckParam>,
    module_source: String,
    source_span: Option<String>,
}

const REPRO_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum ReproArtifact {
    Autogen(AutogenReproArtifact),
    Fuzz(FuzzReproArtifact),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AutogenReproArtifact {
    version: u32,
    generated_at_unix_ms: u64,
    workspace_root: String,
    test_id: String,
    module_path: String,
    func_name: String,
    seed: u64,
    span: Option<String>,
    original_call: String,
    shrunk_call: Option<String>,
    replay_call: String,
    failure: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FuzzReproArtifact {
    version: u32,
    generated_at_unix_ms: u64,
    workspace_root: String,
    test_id: String,
    module_path: String,
    func_name: String,
    seed: u64,
    span: Option<String>,
    call: String,
    uses_bytes_helper: bool,
    failure: String,
}

#[derive(Clone)]
struct AutogenCheckParam {
    name: String,
    ty: AutogenScalarType,
}

#[derive(Clone)]
struct FuzzTargetDecl {
    module_path: String,
    func_name: String,
    param_name: String,
    param_ty: FuzzParamType,
    module_source: String,
    source_span: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FuzzParamType {
    String,
    Bytes,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AutogenScalarType {
    Integer,
    Boolean,
    String,
}

#[derive(Debug, Clone, Serialize)]
struct MutationGateReport {
    version: u32,
    generated_at_unix_ms: u128,
    discovery_ms: u128,
    execution_ms: u128,
    compile_total_ms: u128,
    test_run_total_ms: u128,
    parallel_workers: usize,
    cache_hits: usize,
    cache_misses: usize,
    cache_invalidations: usize,
    total_mutants: usize,
    valid_mutants: usize,
    invalid_mutants: usize,
    killed_mutants: usize,
    survived_mutants: usize,
    no_covering_tests_mutants: usize,
    kill_rate_pct: f64,
    domain_application_kill_rate_pct: Option<f64>,
    mutants: Vec<MutationMutantResult>,
}

#[derive(Debug, Clone, Serialize)]
struct MutationMutantResult {
    function: String,
    function_id: String,
    mutation_type: String,
    tests_ran: Vec<String>,
    compile_ms: u128,
    test_run_ms: u128,
    status: String,
    reason: Option<String>,
}

struct MutationGateOutcome {
    summary_hash: Option<String>,
    discovery_ms: u128,
    execution_ms: u128,
}

struct MutationExecutionResult {
    job_index: usize,
    mutant: MutationMutantResult,
    cache_hits: usize,
    cache_misses: usize,
    cache_invalidations: usize,
}

#[derive(Clone)]
struct MutationCandidateJob {
    job_index: usize,
    candidate: MirMutationCandidate,
    tests_to_run: Vec<TestCase>,
}

#[derive(Clone)]
struct MutationExecutionContext {
    workspace_root: PathBuf,
    source_hash: String,
    toolchain_version: String,
    cache_root: PathBuf,
    cache_enabled: bool,
}

#[derive(Serialize, Deserialize)]
struct MutationCacheMetadata {
    schema_version: u32,
    toolchain_version: String,
    source_hash: String,
    candidate_key: String,
    mutant_binary_path: String,
    build_status: String,
    invalid_reason: Option<String>,
    compile_ms: u128,
}

#[derive(Default, Serialize, Deserialize)]
struct MutationKillHistoryArtifact {
    schema_version: u32,
    entries: BTreeMap<String, MutationKillHistoryEntry>,
}

#[derive(Clone, Serialize, Deserialize)]
struct MutationKillHistoryEntry {
    kills: u64,
    attempts: u64,
    last_seen_unix_ms: u128,
}

struct MutantCompileSuccess {
    exe_path: PathBuf,
    compile_ms: u128,
    cache_hits: usize,
    cache_misses: usize,
    cache_invalidations: usize,
}

struct MutantCompileFailure {
    reason: String,
    compile_ms: u128,
    cache_hits: usize,
    cache_misses: usize,
    cache_invalidations: usize,
}

impl HttpCassetteMode {
    fn as_env_value(self) -> &'static str {
        match self {
            HttpCassetteMode::Replay => "replay",
            HttpCassetteMode::Record => "record",
        }
    }
}

#[derive(Clone, Default)]
struct TestSelection {
    list: bool,
    id: Option<String>,
    filter: Option<String>,
    lane: Option<TestLane>,
    include_ids: Option<HashSet<String>>,
    cert_selection_report: Option<CertSelectionReport>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct MetricsDump {
    messages_sent: u64,
    messages_dropped: u64,
    pending_resolved: u64,
    pending_dropped: u64,
    mailbox_high_water: u64,
    rc_inc: u64,
    rc_dec: u64,
    alloc_list: u64,
    alloc_map: u64,
    alloc_string: u64,
    alloc_bytes: u64,
    alloc_result: u64,
    alloc_pending: u64,
    mailbox_enqueue_ok: u64,
    mailbox_enqueue_fail: u64,
    mailbox_dequeue: u64,
    #[serde(default)]
    sched_dispatched: u64,
    #[serde(default)]
    sched_skipped_no_credit: u64,
    #[serde(default)]
    sched_profile_switch: u64,
    #[serde(default)]
    sched_starvation_violation: u64,
    #[serde(default)]
    sched_cross_shard_migration: u64,
    #[serde(default)]
    abi_typed_lane: u64,
    #[serde(default)]
    abi_boxed_lane: u64,
    #[serde(default)]
    queue_cas_retry_total: u64,
    #[serde(default)]
    mailbox_wake_coalesced_count: u64,
    #[serde(default)]
    mailbox_rescue_wake_count: u64,
    #[serde(default)]
    sched_local_dispatch_count: u64,
    #[serde(default)]
    sched_global_dispatch_count: u64,
    #[serde(default)]
    sched_plan_recompute_count: u64,
    #[serde(default)]
    sched_steal_attempts: u64,
    #[serde(default)]
    sched_steal_success: u64,
    #[serde(default)]
    sched_migration_blocked_hysteresis: u64,
    #[serde(default)]
    sched_migration_blocked_cooldown: u64,
    #[serde(default)]
    queue_enqueue_p99_ns: u128,
    #[serde(default)]
    queue_dequeue_p99_ns: u128,
    #[serde(default)]
    queue_age_p99_ns: u128,
    #[serde(default)]
    sched_dispatch_loop_ns_p99: u128,
    #[serde(default)]
    queue_burst_drain_avg: f64,
    #[serde(default)]
    function_coverage: BTreeMap<String, u64>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
struct MetricsTotals {
    messages_sent: u64,
    messages_dropped: u64,
    pending_resolved: u64,
    pending_dropped: u64,
    mailbox_high_water: u64,
    rc_inc: u64,
    rc_dec: u64,
    alloc_list: u64,
    alloc_map: u64,
    alloc_string: u64,
    alloc_bytes: u64,
    alloc_result: u64,
    alloc_pending: u64,
    mailbox_enqueue_ok: u64,
    mailbox_enqueue_fail: u64,
    mailbox_dequeue: u64,
    sched_dispatched: u64,
    sched_skipped_no_credit: u64,
    #[serde(default)]
    sched_profile_switch: u64,
    #[serde(default)]
    sched_starvation_violation: u64,
    #[serde(default)]
    sched_cross_shard_migration: u64,
    #[serde(default)]
    abi_typed_lane: u64,
    #[serde(default)]
    abi_boxed_lane: u64,
    #[serde(default)]
    queue_cas_retry_total: u64,
    #[serde(default)]
    mailbox_wake_coalesced_count: u64,
    #[serde(default)]
    mailbox_rescue_wake_count: u64,
    #[serde(default)]
    sched_local_dispatch_count: u64,
    #[serde(default)]
    sched_global_dispatch_count: u64,
    #[serde(default)]
    sched_plan_recompute_count: u64,
    #[serde(default)]
    sched_steal_attempts: u64,
    #[serde(default)]
    sched_steal_success: u64,
    #[serde(default)]
    sched_migration_blocked_hysteresis: u64,
    #[serde(default)]
    sched_migration_blocked_cooldown: u64,
    #[serde(default)]
    queue_enqueue_p99_ns: u128,
    #[serde(default)]
    queue_dequeue_p99_ns: u128,
    #[serde(default)]
    queue_age_p99_ns: u128,
    #[serde(default)]
    sched_dispatch_loop_ns_p99: u128,
    #[serde(default)]
    queue_burst_drain_avg: f64,
    #[serde(default)]
    function_coverage: BTreeMap<String, u64>,
}

impl MetricsTotals {
    fn add(&mut self, metrics: &MetricsDump) {
        self.messages_sent += metrics.messages_sent;
        self.messages_dropped += metrics.messages_dropped;
        self.pending_resolved += metrics.pending_resolved;
        self.pending_dropped += metrics.pending_dropped;
        self.mailbox_high_water = self.mailbox_high_water.max(metrics.mailbox_high_water);
        self.rc_inc += metrics.rc_inc;
        self.rc_dec += metrics.rc_dec;
        self.alloc_list += metrics.alloc_list;
        self.alloc_map += metrics.alloc_map;
        self.alloc_string += metrics.alloc_string;
        self.alloc_bytes += metrics.alloc_bytes;
        self.alloc_result += metrics.alloc_result;
        self.alloc_pending += metrics.alloc_pending;
        self.mailbox_enqueue_ok += metrics.mailbox_enqueue_ok;
        self.mailbox_enqueue_fail += metrics.mailbox_enqueue_fail;
        self.mailbox_dequeue += metrics.mailbox_dequeue;
        self.sched_dispatched += metrics.sched_dispatched;
        self.sched_skipped_no_credit += metrics.sched_skipped_no_credit;
        self.sched_profile_switch += metrics.sched_profile_switch;
        self.sched_starvation_violation += metrics.sched_starvation_violation;
        self.sched_cross_shard_migration += metrics.sched_cross_shard_migration;
        self.abi_typed_lane += metrics.abi_typed_lane;
        self.abi_boxed_lane += metrics.abi_boxed_lane;
        self.queue_cas_retry_total += metrics.queue_cas_retry_total;
        self.mailbox_wake_coalesced_count += metrics.mailbox_wake_coalesced_count;
        self.mailbox_rescue_wake_count += metrics.mailbox_rescue_wake_count;
        self.sched_local_dispatch_count += metrics.sched_local_dispatch_count;
        self.sched_global_dispatch_count += metrics.sched_global_dispatch_count;
        self.sched_plan_recompute_count += metrics.sched_plan_recompute_count;
        self.sched_steal_attempts += metrics.sched_steal_attempts;
        self.sched_steal_success += metrics.sched_steal_success;
        self.sched_migration_blocked_hysteresis += metrics.sched_migration_blocked_hysteresis;
        self.sched_migration_blocked_cooldown += metrics.sched_migration_blocked_cooldown;
        self.queue_enqueue_p99_ns = self.queue_enqueue_p99_ns.max(metrics.queue_enqueue_p99_ns);
        self.queue_dequeue_p99_ns = self.queue_dequeue_p99_ns.max(metrics.queue_dequeue_p99_ns);
        self.queue_age_p99_ns = self.queue_age_p99_ns.max(metrics.queue_age_p99_ns);
        self.sched_dispatch_loop_ns_p99 = self
            .sched_dispatch_loop_ns_p99
            .max(metrics.sched_dispatch_loop_ns_p99);
        self.queue_burst_drain_avg = self
            .queue_burst_drain_avg
            .max(metrics.queue_burst_drain_avg);
        for (function_id, hits) in &metrics.function_coverage {
            *self
                .function_coverage
                .entry(function_id.clone())
                .or_insert(0) += *hits;
        }
    }

    fn total_allocs(&self) -> u64 {
        self.alloc_list
            + self.alloc_map
            + self.alloc_string
            + self.alloc_bytes
            + self.alloc_result
            + self.alloc_pending
    }
}

struct TestExecution {
    exit: i32,
    summary: Option<PerfSummary>,
    differential_results_hash: Option<String>,
    mutation_summary_hash: Option<String>,
    cert_timings: CertPerfTimings,
}

#[derive(Clone, Copy, Default)]
struct CertPerfTimings {
    collect_tests_ms: u128,
    compile_harness_ms: u128,
    determinism_ms: u128,
    mutation_discovery_ms: u128,
    mutation_execution_ms: u128,
    differential_ms: u128,
}

struct TestRun {
    metrics: Option<MetricsDump>,
    runtime_ns: u128,
}

#[derive(Clone)]
struct TestHarness {
    exe_path: PathBuf,
    compile_ns: u128,
}

#[derive(Default)]
struct RunOnceTimings {
    collect_tests_ms: u128,
    compile_harness_ms: u128,
}

#[derive(Serialize)]
struct TestJsonSummary {
    run: TestJsonRunMetadata,
    tests: Vec<TestJsonCase>,
    timings: TestJsonTimings,
}

#[derive(Serialize)]
struct TestJsonRunMetadata {
    seed: u64,
    lane: String,
    jobs: usize,
    budgets_used: BudgetPolicyV1,
}

#[derive(Serialize)]
struct TestJsonCase {
    id: String,
    name: String,
    lane: String,
    status: String,
    duration_ms: u128,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Serialize)]
struct TestJsonTimings {
    discovery_ms: u128,
    selection_ms: u128,
    execution_ms: u128,
    total_ms: u128,
}

const TEST_JSON_SUMMARY_SEED: u64 = 0x5A17;

#[derive(Clone)]
struct DeterminismSignature {
    hash: String,
    outcomes: Vec<DeterminismOutcome>,
}

#[derive(Clone, Serialize, PartialEq, Eq)]
struct DeterminismOutcome {
    id: String,
    name: String,
    lane: String,
    status: String,
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PerfCaseSample {
    #[serde(default)]
    id: String,
    name: String,
    compile_ns: u128,
    runtime_ns: u128,
    #[serde(skip_serializing_if = "Option::is_none")]
    metrics: Option<MetricsDump>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PerfSummary {
    sample_count: usize,
    compile_throughput_tests_per_sec: f64,
    runtime_p50_ns: u128,
    runtime_p95_ns: u128,
    runtime_p99_ns: u128,
    allocs_per_request: f64,
    rc_inc: u64,
    rc_dec: u64,
    rc_ops_total: u64,
    dispatch_hit_ratio: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    check_fallback_rate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    avg_check_batch_size: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    check_oracle_eval_ns_p50: Option<u128>,
    #[serde(skip_serializing_if = "Option::is_none")]
    check_oracle_eval_ns_p95: Option<u128>,
    #[serde(skip_serializing_if = "Option::is_none")]
    effect_annihilation_rewrite_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scheduler_dispatch_p99_ns: Option<u128>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scheduler_starvation_violations: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rewrite_compile_overhead_pct: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rewrite_applied_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    actor_msgs_per_sec_p50: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    actor_msgs_per_sec_p95: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    queue_enqueue_p99_ns: Option<u128>,
    #[serde(skip_serializing_if = "Option::is_none")]
    queue_dequeue_p99_ns: Option<u128>,
    #[serde(skip_serializing_if = "Option::is_none")]
    queue_age_p99_ns: Option<u128>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mailbox_wake_coalesced_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mailbox_rescue_wake_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    queue_cas_retry_total: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cases: Option<Vec<PerfCaseSample>>,
    metrics: MetricsTotals,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct KpiThresholds {
    #[serde(skip_serializing_if = "Option::is_none")]
    check_fallback_max: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    check_batch_min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scheduler_p99_improve_min_pct: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rewrite_overhead_max_pct: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    actor_throughput_improve_min_pct: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    queue_age_p99_max_regress_pct: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    starvation_violations_max: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scheduler_throughput_improve_min_pct: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scheduler_loop_p99_max_regress_pct: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scheduler_local_hit_min: Option<f64>,
}

impl KpiThresholds {
    fn any_set(&self) -> bool {
        self.check_fallback_max.is_some()
            || self.check_batch_min.is_some()
            || self.scheduler_p99_improve_min_pct.is_some()
            || self.rewrite_overhead_max_pct.is_some()
            || self.actor_throughput_improve_min_pct.is_some()
            || self.queue_age_p99_max_regress_pct.is_some()
            || self.starvation_violations_max.is_some()
            || self.scheduler_throughput_improve_min_pct.is_some()
            || self.scheduler_loop_p99_max_regress_pct.is_some()
            || self.scheduler_local_hit_min.is_some()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PerfCv {
    compile_throughput_pct: f64,
    runtime_p50_pct: f64,
    runtime_p95_pct: f64,
    runtime_p99_pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PerfReport {
    version: u32,
    generated_at_unix_ms: u128,
    runs: usize,
    cv: PerfCv,
    summary: PerfSummary,
    samples: Vec<PerfSummary>,
}

#[derive(Debug, Clone)]
struct PerfGateConfig {
    baseline_path: PathBuf,
    max_regression_pct: f64,
    kpi_thresholds: KpiThresholds,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum PerfProfile {
    Smoke,
    Standard,
    Deep,
}

impl PerfProfile {
    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "smoke" => Some(Self::Smoke),
            "standard" => Some(Self::Standard),
            "deep" => Some(Self::Deep),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Smoke => "smoke",
            Self::Standard => "standard",
            Self::Deep => "deep",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct BenchmarkManifest {
    version: u32,
    suite: String,
    #[serde(default)]
    optional: bool,
    #[serde(default)]
    profiles: BenchmarkProfiles,
    scenarios: Vec<BenchmarkScenario>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct BenchmarkProfiles {
    #[serde(default)]
    smoke: Option<BenchmarkProfileConfig>,
    #[serde(default)]
    standard: Option<BenchmarkProfileConfig>,
    #[serde(default)]
    deep: Option<BenchmarkProfileConfig>,
}

#[derive(Debug, Clone, Deserialize)]
struct BenchmarkProfileConfig {
    warmup_pairs: usize,
    measure_pairs: usize,
    coverage: String,
}

#[derive(Debug, Clone, Deserialize)]
struct BenchmarkScenario {
    id: String,
    test_name: String,
    ops: u64,
    class: String,
    #[serde(default)]
    min_runtime_ms: Option<u64>,
    #[serde(default)]
    timeout_ms: Option<u64>,
    #[serde(default)]
    allow_unstable: bool,
}

impl BenchmarkManifest {
    fn scenarios_for_profile(&self, profile: PerfProfile) -> Vec<&BenchmarkScenario> {
        let coverage = self
            .profiles
            .config_for(profile)
            .map(|cfg| cfg.coverage.to_ascii_lowercase())
            .unwrap_or_else(|| "all".to_string());
        if coverage == "critical" {
            self.scenarios
                .iter()
                .filter(|scenario| scenario.class.eq_ignore_ascii_case("critical"))
                .collect()
        } else {
            self.scenarios.iter().collect()
        }
    }
}

impl BenchmarkProfiles {
    fn config_for(&self, profile: PerfProfile) -> Option<&BenchmarkProfileConfig> {
        match profile {
            PerfProfile::Smoke => self.smoke.as_ref(),
            PerfProfile::Standard => self.standard.as_ref(),
            PerfProfile::Deep => self.deep.as_ref(),
        }
    }
}

#[derive(Debug, Clone)]
struct PerfCmpConfig {
    baseline_ref: String,
    candidate_ref: String,
    manifest_path: PathBuf,
    benchmark_root: PathBuf,
    profile: PerfProfile,
    warmup_pairs_override: Option<usize>,
    measure_pairs_override: Option<usize>,
    min_effect_pct: f64,
    confidence_pct: f64,
    output_json: PathBuf,
    output_format: OutputFormat,
    test_timeout_ms: Option<u64>,
    perf_debug: bool,
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

fn run_tests(
    target: &TestTarget,
    budget_policy: &BudgetPolicyV1,
    jobs: usize,
    timeout: Duration,
    output_format: OutputFormat,
    perf_debug: bool,
    perf_gate: Option<&PerfGateConfig>,
    selection: &TestSelection,
    enforce_determinism_gate: bool,
    http_mode: HttpCassetteMode,
    sim_seed_override: Option<u64>,
) -> TestExecution {
    let mut cert_timings = CertPerfTimings::default();
    let mut harness_cache: HashMap<String, TestHarness> = HashMap::new();
    let mut first_run_timings = RunOnceTimings::default();
    let (exit, summary, signature) = run_tests_once(
        target,
        budget_policy,
        jobs,
        timeout,
        output_format,
        perf_debug,
        perf_gate.is_some(),
        selection,
        true,
        true,
        http_mode,
        sim_seed_override,
        enforce_determinism_gate,
        DifferentialPipeline::Baseline,
        Some(&mut first_run_timings),
        Some(&mut harness_cache),
    );
    cert_timings.collect_tests_ms += first_run_timings.collect_tests_ms;
    cert_timings.compile_harness_ms += first_run_timings.compile_harness_ms;
    let mut differential_results_hash = None;
    let mut mutation_summary_hash = None;
    if enforce_determinism_gate {
        let determinism_start = Instant::now();
        let Some(first_signature) = signature else {
            if exit != EXIT_OK {
                return TestExecution {
                    exit,
                    summary: None,
                    differential_results_hash: None,
                    mutation_summary_hash: None,
                    cert_timings,
                };
            }
            return TestExecution {
                exit: EXIT_OK,
                summary,
                differential_results_hash: None,
                mutation_summary_hash: None,
                cert_timings,
            };
        };
        let mut alt_timings = RunOnceTimings::default();
        let diff_start = Instant::now();
        let (alt_exit, _, alt_signature) = run_tests_once(
            target,
            budget_policy,
            jobs,
            timeout,
            output_format,
            perf_debug,
            perf_gate.is_some(),
            selection,
            false,
            false,
            http_mode,
            sim_seed_override,
            enforce_determinism_gate,
            DifferentialPipeline::Alt,
            Some(&mut alt_timings),
            Some(&mut harness_cache),
        );
        cert_timings.collect_tests_ms += alt_timings.collect_tests_ms;
        cert_timings.compile_harness_ms += alt_timings.compile_harness_ms;
        let Some(alt_signature) = alt_signature else {
            eprintln!("differential gate failed: alt pipeline produced no signature");
            return TestExecution {
                exit: EXIT_CODEGEN,
                summary: None,
                differential_results_hash: None,
                mutation_summary_hash: None,
                cert_timings,
            };
        };
        cert_timings.differential_ms += diff_start.elapsed().as_millis();
        differential_results_hash = Some(fnv1a64_hex(
            format!("{}:{}", first_signature.hash, alt_signature.hash).as_bytes(),
        ));
        if alt_exit != exit || first_signature.hash != alt_signature.hash {
            eprintln!("differential gate failed: baseline and alt pipelines diverged");
            eprintln!("  baseline exit: {exit}");
            eprintln!("  alt exit: {alt_exit}");
            eprintln!("  baseline signature: {}", first_signature.hash);
            eprintln!("  alt signature: {}", alt_signature.hash);
            if let Some(detail) =
                first_signature_mismatch_detail(&first_signature.outcomes, &alt_signature.outcomes)
            {
                eprintln!("  mismatch detail: {detail}");
            }
            return TestExecution {
                exit: EXIT_CODEGEN,
                summary: None,
                differential_results_hash,
                mutation_summary_hash: None,
                cert_timings,
            };
        }
        let mut replay_timings = RunOnceTimings::default();
        let (repeat_exit, _, repeat_signature) = run_tests_once(
            target,
            budget_policy,
            jobs,
            timeout,
            output_format,
            perf_debug,
            perf_gate.is_some(),
            selection,
            false,
            false,
            http_mode,
            sim_seed_override,
            enforce_determinism_gate,
            DifferentialPipeline::Baseline,
            Some(&mut replay_timings),
            Some(&mut harness_cache),
        );
        cert_timings.collect_tests_ms += replay_timings.collect_tests_ms;
        cert_timings.compile_harness_ms += replay_timings.compile_harness_ms;
        let Some(second_signature) = repeat_signature else {
            eprintln!(
                "determinism gate failed: replay did not produce a certification outcome signature"
            );
            return TestExecution {
                exit: EXIT_CODEGEN,
                summary: None,
                differential_results_hash,
                mutation_summary_hash: None,
                cert_timings,
            };
        };
        if repeat_exit != exit || first_signature.hash != second_signature.hash {
            eprintln!(
                "determinism gate failed: certified suite produced inconsistent outcomes with seed {:#x}",
                TEST_JSON_SUMMARY_SEED
            );
            eprintln!("  first run exit: {exit}");
            eprintln!("  replay exit: {repeat_exit}");
            eprintln!("  first signature: {}", first_signature.hash);
            eprintln!("  replay signature: {}", second_signature.hash);
            if let Some(detail) = first_signature_mismatch_detail(
                &first_signature.outcomes,
                &second_signature.outcomes,
            ) {
                eprintln!("  mismatch detail: {detail}");
            }
            return TestExecution {
                exit: EXIT_CODEGEN,
                summary: None,
                differential_results_hash,
                mutation_summary_hash: None,
                cert_timings,
            };
        }
        cert_timings.determinism_ms += determinism_start.elapsed().as_millis();
    }
    if exit != EXIT_OK {
        return TestExecution {
            exit,
            summary: None,
            differential_results_hash,
            mutation_summary_hash: None,
            cert_timings,
        };
    }
    if let (Some(gate), Some(perf_summary)) = (perf_gate, summary.as_ref()) {
        let baseline = match load_perf_baseline_summary(&gate.baseline_path) {
            Ok(baseline) => baseline,
            Err(err) => {
                eprintln!(
                    "perf gate error: failed to load baseline {}: {}",
                    gate.baseline_path.display(),
                    err
                );
                return TestExecution {
                    exit: EXIT_CODEGEN,
                    summary,
                    differential_results_hash,
                    mutation_summary_hash: None,
                    cert_timings,
                };
            }
        };
        let failures = evaluate_perf_gate(
            perf_summary,
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
            return TestExecution {
                exit: EXIT_CODEGEN,
                summary,
                differential_results_hash,
                mutation_summary_hash: None,
                cert_timings,
            };
        }
    }
    if enforce_determinism_gate
        && let TestTarget::ProjectRoot(root) = target
        && let Err(err) = evaluate_connector_contract_gate(root)
    {
        eprintln!("connector contract gate failed:\n{err}");
        return TestExecution {
            exit: EXIT_CODEGEN,
            summary,
            differential_results_hash,
            mutation_summary_hash: None,
            cert_timings,
        };
    }
    if enforce_determinism_gate
        && let TestTarget::ProjectRoot(root) = target
        && let Some(perf_summary) = summary.as_ref()
    {
        match run_mutation_gate(
            root,
            perf_summary,
            budget_policy.mutation_max_cases.value as usize,
            budget_policy.mutation_time_cap_ms.value,
        ) {
            Ok(outcome) => {
                mutation_summary_hash = outcome.summary_hash;
                cert_timings.mutation_discovery_ms += outcome.discovery_ms;
                cert_timings.mutation_execution_ms += outcome.execution_ms;
            }
            Err(err) => {
                eprintln!("mutation gate failed:\n{err}");
                return TestExecution {
                    exit: EXIT_CODEGEN,
                    summary,
                    differential_results_hash,
                    mutation_summary_hash: None,
                    cert_timings,
                };
            }
        }
    }
    TestExecution {
        exit: EXIT_OK,
        summary,
        differential_results_hash,
        mutation_summary_hash,
        cert_timings,
    }
}

fn run_tests_once(
    target: &TestTarget,
    budget_policy: &BudgetPolicyV1,
    jobs: usize,
    timeout: Duration,
    output_format: OutputFormat,
    perf_debug: bool,
    perf_lane: bool,
    selection: &TestSelection,
    emit_json_summary: bool,
    emit_pretty_output: bool,
    http_mode: HttpCassetteMode,
    sim_seed_override: Option<u64>,
    certify_mode: bool,
    pipeline: DifferentialPipeline,
    mut run_timing_out: Option<&mut RunOnceTimings>,
    harness_cache: Option<&mut HashMap<String, TestHarness>>,
) -> (i32, Option<PerfSummary>, Option<DeterminismSignature>) {
    configure_runtime_for_test_lane(perf_lane, perf_debug);
    let total_start = Instant::now();
    let discovery_start = Instant::now();
    let mut tests = Vec::new();
    let (workspace_root, compile_root, tests_root, missing_path_msg) = match target {
        TestTarget::ProjectRoot(root) => {
            let workspace_root = fs::canonicalize(root).unwrap_or_else(|_| root.clone());
            let src_root = workspace_root.join("src");
            let tests_root = workspace_root.join("tests");
            let tests_root_opt = if tests_root.is_dir() {
                if let Err(err) = collect_tests(&tests_root, &tests_root, &mut tests) {
                    eprintln!("test discovery error: {err}");
                    return (EXIT_USAGE, None, None);
                }
                Some(tests_root.clone())
            } else {
                None
            };
            match collect_autogen_spec_tests(
                &workspace_root,
                budget_policy.autogen_max_cases.value,
                budget_policy.autogen_time_cap_ms.value,
            ) {
                Ok(mut generated) => tests.append(&mut generated),
                Err(err) => {
                    eprintln!("test discovery error: {err}");
                    return (EXIT_USAGE, None, None);
                }
            }
            if certify_mode {
                match collect_fuzz_tests(
                    &workspace_root,
                    budget_policy.fuzz_max_cases.value,
                    budget_policy.fuzz_time_cap_ms.value,
                ) {
                    Ok(mut generated) => tests.append(&mut generated),
                    Err(err) => {
                        eprintln!("test discovery error: {err}");
                        return (EXIT_USAGE, None, None);
                    }
                }
            }
            let missing_path_msg = tests_root_opt
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| workspace_root.display().to_string());
            (workspace_root, src_root, tests_root_opt, missing_path_msg)
        }
        TestTarget::SingleFile(path) => {
            let Some(parent) = path.parent() else {
                eprintln!("test discovery error: file has no parent directory");
                return (EXIT_USAGE, None, None);
            };
            let source = match fs::read_to_string(path) {
                Ok(source) => source,
                Err(err) => {
                    eprintln!("test discovery error: {err}");
                    return (EXIT_USAGE, None, None);
                }
            };
            let module_path = match module_path_for_single_file(path) {
                Ok(module_path) => module_path,
                Err(err) => {
                    eprintln!("test discovery error: {err}");
                    return (EXIT_USAGE, None, None);
                }
            };
            if let Err(err) = collect_tests_from_source(&source, &module_path, false, &mut tests) {
                eprintln!("test discovery error: {err}");
                return (EXIT_USAGE, None, None);
            }
            let workspace_root = fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf());
            (
                workspace_root.clone(),
                workspace_root,
                None,
                path.display().to_string(),
            )
        }
    };
    let discovery_ms = discovery_start.elapsed().as_millis();

    let selection_start = Instant::now();
    tests.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    let selected_tests = select_tests(tests, selection);
    let canonical_authored_selected: Vec<TestCase> = selected_tests
        .iter()
        .filter(|test| test.generated_case_kind.is_none() && test.sim_seed.is_none())
        .cloned()
        .collect();
    if certify_mode
        && !selection.list
        && let Err(err) = enforce_serial_test_cap(&canonical_authored_selected)
    {
        eprintln!("serial gate failed: {err}");
        return (EXIT_CODEGEN, None, None);
    }
    let tests = if selection.list {
        selected_tests
    } else {
        expand_sim_seed_cases(selected_tests, sim_seed_override, certify_mode)
    };
    let selection_ms = selection_start.elapsed().as_millis();
    if let Some(timing) = run_timing_out.as_deref_mut() {
        timing.collect_tests_ms = discovery_ms + selection_ms;
    }
    if (emit_pretty_output || emit_json_summary)
        && let Some(report) = selection.cert_selection_report.as_ref()
    {
        emit_cert_selection_report(output_format, report, tests.len());
    }

    if tests.is_empty() {
        if selection.id.is_some() || selection.filter.is_some() {
            eprintln!("no tests matched selection at {}", missing_path_msg);
        } else {
            eprintln!("no tests found at {}", missing_path_msg);
        }
        return (EXIT_OK, None, None);
    }

    if selection.list {
        match output_format {
            OutputFormat::Pretty => list_tests(&tests),
            OutputFormat::Json => {
                let summary = TestJsonSummary {
                    run: TestJsonRunMetadata {
                        seed: TEST_JSON_SUMMARY_SEED,
                        lane: summarize_run_lane(&tests),
                        jobs,
                        budgets_used: budget_policy.clone(),
                    },
                    tests: tests
                        .iter()
                        .map(|test| TestJsonCase {
                            id: test.id.clone(),
                            name: test.name.clone(),
                            lane: test.lane.as_str().to_string(),
                            status: "listed".to_string(),
                            duration_ms: 0,
                            error: None,
                        })
                        .collect(),
                    timings: TestJsonTimings {
                        discovery_ms,
                        selection_ms,
                        execution_ms: 0,
                        total_ms: total_start.elapsed().as_millis(),
                    },
                };
                emit_test_json_summary(&summary);
            }
        }
        return (EXIT_OK, None, None);
    }

    let missing_oracles: Vec<&TestCase> = tests.iter().filter(|test| !test.has_oracle).collect();
    if !missing_oracles.is_empty() {
        eprintln!(
            "oracle gate failed: test functions must contain at least one `assert` or `require`"
        );
        for test in missing_oracles {
            eprintln!("  - {}: no assertion oracle found", test.name);
        }
        return (EXIT_CODEGEN, None, None);
    }

    let harness = match compile_test_harness(
        &workspace_root,
        &compile_root,
        tests_root.as_deref(),
        &tests,
        output_format,
        harness_cache,
    ) {
        Ok(harness) => harness,
        Err(err) => {
            eprintln!("test harness error: {err}");
            return (EXIT_CODEGEN, None, None);
        }
    };
    if let Some(timing) = run_timing_out.as_deref_mut() {
        timing.compile_harness_ms = harness.compile_ns / 1_000_000;
    }

    let total_tests = tests.len();
    let base_compile_ns = harness.compile_ns / total_tests as u128;
    let compile_ns_remainder = harness.compile_ns % total_tests as u128;

    let execution_start = Instant::now();
    let (serial_tests, parallel_tests): (Vec<TestCase>, Vec<TestCase>) =
        tests.into_iter().partition(|test| test.is_serial);
    let queue = std::sync::Arc::new(std::sync::Mutex::new(std::collections::VecDeque::from(
        parallel_tests,
    )));
    let (tx, rx) =
        std::sync::mpsc::channel::<(TestCase, bool, Duration, String, Option<TestRun>)>();
    let mut handles = Vec::new();
    let worker_count = jobs.max(1);
    for _ in 0..worker_count {
        let queue = std::sync::Arc::clone(&queue);
        let tx = tx.clone();
        let harness_exe_path = harness.exe_path.clone();
        let workspace_root = workspace_root.clone();
        handles.push(std::thread::spawn(move || {
            loop {
                let next = {
                    let mut guard = queue.lock().expect("test queue");
                    guard.pop_front()
                };
                let Some(test) = next else { break };
                let start = Instant::now();
                let (ok, err, run) = execute_test_case(
                    &harness_exe_path,
                    &workspace_root,
                    &test,
                    timeout,
                    output_format,
                    http_mode,
                    pipeline,
                    certify_mode,
                );
                let _ = tx.send((test, ok, start.elapsed(), err, run));
            }
        }));
    }
    drop(tx);

    let mut ok_count = 0usize;
    let mut fail_count = 0usize;
    let mut compile_ns: Vec<u128> = Vec::new();
    let mut runtime_ns: Vec<u128> = Vec::new();
    let mut cases: Vec<PerfCaseSample> = Vec::new();
    let mut metrics_totals = MetricsTotals::default();
    let mut metrics_count = 0usize;
    let mut json_cases = Vec::new();
    let mut completed = 0usize;
    for (test, ok, dur, err, run) in rx.iter() {
        let compile_slice_ns = if completed < compile_ns_remainder as usize {
            base_compile_ns + 1
        } else {
            base_compile_ns
        };
        completed += 1;
        compile_ns.push(compile_slice_ns);

        if let Some(run) = run.as_ref() {
            runtime_ns.push(run.runtime_ns);
            if let Some(metrics) = run.metrics.as_ref() {
                metrics_totals.add(metrics);
                metrics_count += 1;
            }
            cases.push(PerfCaseSample {
                id: test.id.clone(),
                name: test.name.clone(),
                compile_ns: compile_slice_ns,
                runtime_ns: run.runtime_ns,
                metrics: run.metrics.clone(),
            });
        }
        json_cases.push(TestJsonCase {
            id: test.id,
            name: test.name.clone(),
            lane: test.lane.as_str().to_string(),
            status: if ok {
                "ok".to_string()
            } else {
                "fail".to_string()
            },
            duration_ms: dur.as_millis(),
            error: if ok { None } else { Some(err.clone()) },
        });
        if ok {
            if emit_pretty_output && matches!(output_format, OutputFormat::Pretty) {
                println!("ok   {:>7?}  {}", dur, test.name);
            }
            ok_count += 1;
        } else {
            if emit_pretty_output && matches!(output_format, OutputFormat::Pretty) {
                println!("fail {:>7?}  {}  {}", dur, test.name, err);
            }
            fail_count += 1;
        }
    }
    for test in serial_tests {
        let start = Instant::now();
        let (ok, err, run) = execute_test_case(
            &harness.exe_path,
            &workspace_root,
            &test,
            timeout,
            output_format,
            http_mode,
            pipeline,
            certify_mode,
        );
        let dur = start.elapsed();
        let compile_slice_ns = if completed < compile_ns_remainder as usize {
            base_compile_ns + 1
        } else {
            base_compile_ns
        };
        completed += 1;
        compile_ns.push(compile_slice_ns);
        if let Some(run) = run.as_ref() {
            runtime_ns.push(run.runtime_ns);
            if let Some(metrics) = run.metrics.as_ref() {
                metrics_totals.add(metrics);
                metrics_count += 1;
            }
            cases.push(PerfCaseSample {
                id: test.id.clone(),
                name: test.name.clone(),
                compile_ns: compile_slice_ns,
                runtime_ns: run.runtime_ns,
                metrics: run.metrics.clone(),
            });
        }
        json_cases.push(TestJsonCase {
            id: test.id,
            name: test.name.clone(),
            lane: test.lane.as_str().to_string(),
            status: if ok {
                "ok".to_string()
            } else {
                "fail".to_string()
            },
            duration_ms: dur.as_millis(),
            error: if ok { None } else { Some(err.clone()) },
        });
        if ok {
            if emit_pretty_output && matches!(output_format, OutputFormat::Pretty) {
                println!("ok   {:>7?}  {}", dur, test.name);
            }
            ok_count += 1;
        } else {
            if emit_pretty_output && matches!(output_format, OutputFormat::Pretty) {
                println!("fail {:>7?}  {}  {}", dur, test.name, err);
            }
            fail_count += 1;
        }
    }
    for handle in handles {
        let _ = handle.join();
    }
    json_cases.sort_by(|a, b| {
        a.id.cmp(&b.id)
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.lane.cmp(&b.lane))
    });
    let execution_ms = execution_start.elapsed().as_millis();
    let total_ms = total_start.elapsed().as_millis();
    let summary_lane = summarize_run_lane_from_json_cases(&json_cases);
    if emit_pretty_output && matches!(output_format, OutputFormat::Pretty) {
        println!("tests: {} passed, {} failed", ok_count, fail_count);
    }
    let signature = build_determinism_signature(&json_cases);
    if matches!(output_format, OutputFormat::Json) && emit_json_summary {
        let summary = TestJsonSummary {
            run: TestJsonRunMetadata {
                seed: TEST_JSON_SUMMARY_SEED,
                lane: summary_lane,
                jobs,
                budgets_used: budget_policy.clone(),
            },
            tests: json_cases,
            timings: TestJsonTimings {
                discovery_ms,
                selection_ms,
                execution_ms,
                total_ms,
            },
        };
        emit_test_json_summary(&summary);
    }
    if fail_count != 0 || runtime_ns.is_empty() {
        return (EXIT_CODEGEN, None, Some(signature));
    }
    let mut summary = build_perf_summary(&compile_ns, &runtime_ns, metrics_count, &metrics_totals);
    // Attach per-test samples so perf consumers (macrobench) can compute per-scenario
    // percentiles without changing the core gate logic.
    summary.cases = Some(cases);
    if emit_pretty_output && matches!(output_format, OutputFormat::Pretty) {
        print_perf_summary(&summary, perf_debug);
    }
    (EXIT_OK, Some(summary), Some(signature))
}

fn run_perf_harness(
    target: &TestTarget,
    budget_policy: &BudgetPolicyV1,
    jobs: usize,
    timeout: Duration,
    output_format: OutputFormat,
    perf_debug: bool,
    runs: usize,
    cv_max_pct: f64,
    baseline_out: &Path,
    perf_gate: Option<&PerfGateConfig>,
    selection: &TestSelection,
    runtime_only_cv_gate: bool,
) -> i32 {
    let mut samples = Vec::new();
    for idx in 0..runs {
        println!("perf-run {}/{}", idx + 1, runs);
        let (exit, summary, _) = run_tests_once(
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
            HttpCassetteMode::Replay,
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
    let summary = aggregate_perf_samples(&samples);
    let cv = compute_cv(&samples);
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
        let baseline = match load_perf_baseline_summary(&gate.baseline_path) {
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
        let failures = evaluate_perf_gate(
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
    if let Some(parent) = baseline_out.parent() {
        if let Err(err) = fs::create_dir_all(parent) {
            eprintln!(
                "perf harness error: failed to create {}: {}",
                parent.display(),
                err
            );
            return EXIT_CODEGEN;
        }
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

fn build_determinism_signature(cases: &[TestJsonCase]) -> DeterminismSignature {
    let outcomes: Vec<DeterminismOutcome> = cases
        .iter()
        .map(|case| DeterminismOutcome {
            id: case.id.clone(),
            name: case.name.clone(),
            lane: case.lane.clone(),
            status: case.status.clone(),
            error: case.error.clone(),
        })
        .collect();
    let payload = serde_json::to_vec(&(TEST_JSON_SUMMARY_SEED, &outcomes))
        .unwrap_or_else(|_| TEST_JSON_SUMMARY_SEED.to_le_bytes().to_vec());
    let hash = fnv1a64_hex(&payload);
    DeterminismSignature { hash, outcomes }
}

fn first_signature_mismatch_detail(
    first: &[DeterminismOutcome],
    second: &[DeterminismOutcome],
) -> Option<String> {
    if first.len() != second.len() {
        return Some(format!(
            "case count differs: first={} replay={}",
            first.len(),
            second.len()
        ));
    }
    for (lhs, rhs) in first.iter().zip(second.iter()) {
        if lhs != rhs {
            return Some(format!(
                "{} => first(status={}, error={:?}) replay(status={}, error={:?})",
                lhs.name, lhs.status, lhs.error, rhs.status, rhs.error
            ));
        }
    }
    None
}

fn configure_runtime_for_test_lane(perf_lane: bool, _perf_debug: bool) {
    if !perf_lane {
        return;
    }
    if env::var_os("WRELA_RUNTIME_PROFILE").is_none() {
        // Perf lanes should exercise release runtime defaults.
        // SAFETY: this happens before test worker threads are spawned.
        unsafe { env::set_var("WRELA_RUNTIME_PROFILE", "release") };
    }
    if env::var_os("WRELA_RUNTIME_METRICS").is_none() {
        // KPI-gated matrix lanes require runtime metrics to be emitted.
        // SAFETY: this happens before test worker threads are spawned.
        unsafe { env::set_var("WRELA_RUNTIME_METRICS", "1") };
    }
}

fn build_perf_summary(
    compile_ns: &[u128],
    runtime_ns: &[u128],
    metrics_count: usize,
    metrics_totals: &MetricsTotals,
) -> PerfSummary {
    let compile_total_ns: u128 = compile_ns.iter().copied().sum();
    let compile_throughput_tests_per_sec = if compile_total_ns == 0 {
        0.0
    } else {
        compile_ns.len() as f64 / (compile_total_ns as f64 / 1_000_000_000.0)
    };

    let mut runtime_sorted = runtime_ns.to_vec();
    runtime_sorted.sort_unstable();
    let runtime_p50_ns = percentile(&runtime_sorted, 0.50);
    let runtime_p95_ns = percentile(&runtime_sorted, 0.95);
    let runtime_p99_ns = percentile(&runtime_sorted, 0.99);

    let allocs_per_request = if metrics_count == 0 {
        0.0
    } else {
        metrics_totals.total_allocs() as f64 / metrics_count as f64
    };
    let dispatch_total = metrics_totals.sched_dispatched + metrics_totals.sched_skipped_no_credit;
    let dispatch_hit_ratio = if dispatch_total == 0 {
        1.0
    } else {
        metrics_totals.sched_dispatched as f64 / dispatch_total as f64
    };
    let rc_ops_total = metrics_totals.rc_inc + metrics_totals.rc_dec;
    let runtime_total_ns: u128 = runtime_ns.iter().copied().sum();
    let actor_msgs_per_sec = if runtime_total_ns == 0 || metrics_totals.mailbox_dequeue == 0 {
        None
    } else {
        Some(
            metrics_totals.mailbox_dequeue as f64
                / (runtime_total_ns as f64 / 1_000_000_000.0).max(f64::EPSILON),
        )
    };
    PerfSummary {
        sample_count: runtime_ns.len(),
        compile_throughput_tests_per_sec,
        runtime_p50_ns,
        runtime_p95_ns,
        runtime_p99_ns,
        allocs_per_request,
        rc_inc: metrics_totals.rc_inc,
        rc_dec: metrics_totals.rc_dec,
        rc_ops_total,
        dispatch_hit_ratio,
        check_fallback_rate: None,
        avg_check_batch_size: None,
        check_oracle_eval_ns_p50: None,
        check_oracle_eval_ns_p95: None,
        effect_annihilation_rewrite_count: None,
        scheduler_dispatch_p99_ns: (metrics_totals.sched_dispatch_loop_ns_p99 > 0)
            .then_some(metrics_totals.sched_dispatch_loop_ns_p99),
        scheduler_starvation_violations: Some(metrics_totals.sched_starvation_violation),
        rewrite_compile_overhead_pct: None,
        rewrite_applied_count: None,
        actor_msgs_per_sec_p50: actor_msgs_per_sec,
        actor_msgs_per_sec_p95: actor_msgs_per_sec,
        queue_enqueue_p99_ns: (metrics_totals.queue_enqueue_p99_ns > 0)
            .then_some(metrics_totals.queue_enqueue_p99_ns),
        queue_dequeue_p99_ns: (metrics_totals.queue_dequeue_p99_ns > 0)
            .then_some(metrics_totals.queue_dequeue_p99_ns),
        queue_age_p99_ns: (metrics_totals.queue_age_p99_ns > 0)
            .then_some(metrics_totals.queue_age_p99_ns),
        mailbox_wake_coalesced_count: Some(metrics_totals.mailbox_wake_coalesced_count),
        mailbox_rescue_wake_count: Some(metrics_totals.mailbox_rescue_wake_count),
        queue_cas_retry_total: Some(metrics_totals.queue_cas_retry_total),
        cases: None,
        metrics: metrics_totals.clone(),
    }
}

fn print_perf_summary(summary: &PerfSummary, perf_debug: bool) {
    println!(
        "perf: compile_tps={:.2} p50_ns={} p95_ns={} p99_ns={} allocs/request={:.2} rc_ops={} dispatch_hit_ratio={:.4}",
        summary.compile_throughput_tests_per_sec,
        summary.runtime_p50_ns,
        summary.runtime_p95_ns,
        summary.runtime_p99_ns,
        summary.allocs_per_request,
        summary.rc_ops_total,
        summary.dispatch_hit_ratio
    );
    let check_lane = check_lane_kpis_from_summary(summary);
    println!(
        "check-lane: typed_total={} boxed_total={} typed_ratio={:.4}",
        check_lane.typed_lane_total, check_lane.boxed_lane_total, check_lane.typed_lane_ratio
    );
    if perf_debug {
        println!(
            "perf-debug: rc_inc={} rc_dec={} mailbox_enqueue_ok={} mailbox_enqueue_fail={} mailbox_dequeue={} mailbox_high_water={} alloc_list={} alloc_map={} alloc_string={} alloc_bytes={} alloc_result={} alloc_pending={} messages_sent={} messages_dropped={} pending_resolved={} pending_dropped={} sched_dispatched={} sched_skipped_no_credit={} sched_profile_switch={} sched_starvation_violation={} sched_cross_shard_migration={} abi_typed_lane={} abi_boxed_lane={}",
            summary.metrics.rc_inc,
            summary.metrics.rc_dec,
            summary.metrics.mailbox_enqueue_ok,
            summary.metrics.mailbox_enqueue_fail,
            summary.metrics.mailbox_dequeue,
            summary.metrics.mailbox_high_water,
            summary.metrics.alloc_list,
            summary.metrics.alloc_map,
            summary.metrics.alloc_string,
            summary.metrics.alloc_bytes,
            summary.metrics.alloc_result,
            summary.metrics.alloc_pending,
            summary.metrics.messages_sent,
            summary.metrics.messages_dropped,
            summary.metrics.pending_resolved,
            summary.metrics.pending_dropped,
            summary.metrics.sched_dispatched,
            summary.metrics.sched_skipped_no_credit,
            summary.metrics.sched_profile_switch,
            summary.metrics.sched_starvation_violation,
            summary.metrics.sched_cross_shard_migration,
            summary.metrics.abi_typed_lane,
            summary.metrics.abi_boxed_lane
        );
    }
}

fn aggregate_perf_samples(samples: &[PerfSummary]) -> PerfSummary {
    if samples.len() == 1 {
        return samples[0].clone();
    }
    let len = samples.len() as f64;
    let mut metrics = MetricsTotals::default();
    for sample in samples {
        metrics.messages_sent += sample.metrics.messages_sent;
        metrics.messages_dropped += sample.metrics.messages_dropped;
        metrics.pending_resolved += sample.metrics.pending_resolved;
        metrics.pending_dropped += sample.metrics.pending_dropped;
        metrics.mailbox_high_water = metrics
            .mailbox_high_water
            .max(sample.metrics.mailbox_high_water);
        metrics.rc_inc += sample.metrics.rc_inc;
        metrics.rc_dec += sample.metrics.rc_dec;
        metrics.alloc_list += sample.metrics.alloc_list;
        metrics.alloc_map += sample.metrics.alloc_map;
        metrics.alloc_string += sample.metrics.alloc_string;
        metrics.alloc_bytes += sample.metrics.alloc_bytes;
        metrics.alloc_result += sample.metrics.alloc_result;
        metrics.alloc_pending += sample.metrics.alloc_pending;
        metrics.mailbox_enqueue_ok += sample.metrics.mailbox_enqueue_ok;
        metrics.mailbox_enqueue_fail += sample.metrics.mailbox_enqueue_fail;
        metrics.mailbox_dequeue += sample.metrics.mailbox_dequeue;
        metrics.sched_dispatched += sample.metrics.sched_dispatched;
        metrics.sched_skipped_no_credit += sample.metrics.sched_skipped_no_credit;
        metrics.sched_profile_switch += sample.metrics.sched_profile_switch;
        metrics.sched_starvation_violation += sample.metrics.sched_starvation_violation;
        metrics.sched_cross_shard_migration += sample.metrics.sched_cross_shard_migration;
        metrics.abi_typed_lane += sample.metrics.abi_typed_lane;
        metrics.abi_boxed_lane += sample.metrics.abi_boxed_lane;
        metrics.queue_cas_retry_total += sample.metrics.queue_cas_retry_total;
        metrics.mailbox_wake_coalesced_count += sample.metrics.mailbox_wake_coalesced_count;
        metrics.mailbox_rescue_wake_count += sample.metrics.mailbox_rescue_wake_count;
        metrics.sched_local_dispatch_count += sample.metrics.sched_local_dispatch_count;
        metrics.sched_global_dispatch_count += sample.metrics.sched_global_dispatch_count;
        metrics.sched_plan_recompute_count += sample.metrics.sched_plan_recompute_count;
        metrics.sched_steal_attempts += sample.metrics.sched_steal_attempts;
        metrics.sched_steal_success += sample.metrics.sched_steal_success;
        metrics.sched_migration_blocked_hysteresis +=
            sample.metrics.sched_migration_blocked_hysteresis;
        metrics.sched_migration_blocked_cooldown += sample.metrics.sched_migration_blocked_cooldown;
        metrics.queue_enqueue_p99_ns = metrics
            .queue_enqueue_p99_ns
            .max(sample.metrics.queue_enqueue_p99_ns);
        metrics.queue_dequeue_p99_ns = metrics
            .queue_dequeue_p99_ns
            .max(sample.metrics.queue_dequeue_p99_ns);
        metrics.queue_age_p99_ns = metrics
            .queue_age_p99_ns
            .max(sample.metrics.queue_age_p99_ns);
        metrics.sched_dispatch_loop_ns_p99 = metrics
            .sched_dispatch_loop_ns_p99
            .max(sample.metrics.sched_dispatch_loop_ns_p99);
        metrics.queue_burst_drain_avg = metrics
            .queue_burst_drain_avg
            .max(sample.metrics.queue_burst_drain_avg);
    }
    let mut runtime_p50: Vec<u128> = samples.iter().map(|s| s.runtime_p50_ns).collect();
    let mut runtime_p95: Vec<u128> = samples.iter().map(|s| s.runtime_p95_ns).collect();
    let mut runtime_p99: Vec<u128> = samples.iter().map(|s| s.runtime_p99_ns).collect();
    runtime_p50.sort_unstable();
    runtime_p95.sort_unstable();
    runtime_p99.sort_unstable();
    PerfSummary {
        sample_count: samples.iter().map(|s| s.sample_count).sum(),
        compile_throughput_tests_per_sec: samples
            .iter()
            .map(|s| s.compile_throughput_tests_per_sec)
            .sum::<f64>()
            / len,
        runtime_p50_ns: runtime_p50[runtime_p50.len() / 2],
        runtime_p95_ns: runtime_p95[runtime_p95.len() / 2],
        runtime_p99_ns: runtime_p99[runtime_p99.len() / 2],
        allocs_per_request: samples.iter().map(|s| s.allocs_per_request).sum::<f64>() / len,
        rc_inc: (samples.iter().map(|s| s.rc_inc as f64).sum::<f64>() / len).round() as u64,
        rc_dec: (samples.iter().map(|s| s.rc_dec as f64).sum::<f64>() / len).round() as u64,
        rc_ops_total: (samples.iter().map(|s| s.rc_ops_total as f64).sum::<f64>() / len).round()
            as u64,
        dispatch_hit_ratio: samples.iter().map(|s| s.dispatch_hit_ratio).sum::<f64>() / len,
        check_fallback_rate: average_optional_f64(samples, |s| s.check_fallback_rate),
        avg_check_batch_size: average_optional_f64(samples, |s| s.avg_check_batch_size),
        check_oracle_eval_ns_p50: median_optional_u128(samples, |s| s.check_oracle_eval_ns_p50),
        check_oracle_eval_ns_p95: median_optional_u128(samples, |s| s.check_oracle_eval_ns_p95),
        effect_annihilation_rewrite_count: average_optional_u64(samples, |s| {
            s.effect_annihilation_rewrite_count
        }),
        scheduler_dispatch_p99_ns: median_optional_u128(samples, |s| s.scheduler_dispatch_p99_ns),
        scheduler_starvation_violations: average_optional_u64(samples, |s| {
            s.scheduler_starvation_violations
        }),
        rewrite_compile_overhead_pct: average_optional_f64(samples, |s| {
            s.rewrite_compile_overhead_pct
        }),
        rewrite_applied_count: average_optional_u64(samples, |s| s.rewrite_applied_count),
        actor_msgs_per_sec_p50: average_optional_f64(samples, |s| s.actor_msgs_per_sec_p50),
        actor_msgs_per_sec_p95: average_optional_f64(samples, |s| s.actor_msgs_per_sec_p95),
        queue_enqueue_p99_ns: median_optional_u128(samples, |s| s.queue_enqueue_p99_ns),
        queue_dequeue_p99_ns: median_optional_u128(samples, |s| s.queue_dequeue_p99_ns),
        queue_age_p99_ns: median_optional_u128(samples, |s| s.queue_age_p99_ns),
        mailbox_wake_coalesced_count: average_optional_u64(samples, |s| {
            s.mailbox_wake_coalesced_count
        }),
        mailbox_rescue_wake_count: average_optional_u64(samples, |s| s.mailbox_rescue_wake_count),
        queue_cas_retry_total: average_optional_u64(samples, |s| s.queue_cas_retry_total),
        cases: None,
        metrics,
    }
}

fn average_optional_f64(
    samples: &[PerfSummary],
    pick: impl Fn(&PerfSummary) -> Option<f64>,
) -> Option<f64> {
    let values: Vec<f64> = samples.iter().filter_map(pick).collect();
    if values.is_empty() {
        None
    } else {
        Some(values.iter().sum::<f64>() / values.len() as f64)
    }
}

fn average_optional_u64(
    samples: &[PerfSummary],
    pick: impl Fn(&PerfSummary) -> Option<u64>,
) -> Option<u64> {
    let values: Vec<u64> = samples.iter().filter_map(pick).collect();
    if values.is_empty() {
        None
    } else {
        Some((values.iter().map(|v| *v as f64).sum::<f64>() / values.len() as f64).round() as u64)
    }
}

fn median_optional_u128(
    samples: &[PerfSummary],
    pick: impl Fn(&PerfSummary) -> Option<u128>,
) -> Option<u128> {
    let mut values: Vec<u128> = samples.iter().filter_map(pick).collect();
    if values.is_empty() {
        None
    } else {
        values.sort_unstable();
        Some(values[values.len() / 2])
    }
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
    (variance.sqrt() / mean) * 100.0
}

fn compute_cv(samples: &[PerfSummary]) -> PerfCv {
    let cv_samples: &[PerfSummary] = if samples.len() > 3 {
        &samples[1..]
    } else {
        samples
    };
    let compile: Vec<f64> = cv_samples
        .iter()
        .map(|sample| sample.compile_throughput_tests_per_sec)
        .collect();
    let p50: Vec<f64> = cv_samples
        .iter()
        .map(|sample| sample.runtime_p50_ns as f64)
        .collect();
    let p95: Vec<f64> = cv_samples
        .iter()
        .map(|sample| sample.runtime_p95_ns as f64)
        .collect();
    let p99: Vec<f64> = cv_samples
        .iter()
        .map(|sample| sample.runtime_p99_ns as f64)
        .collect();
    PerfCv {
        compile_throughput_pct: coefficient_of_variation(&compile),
        runtime_p50_pct: coefficient_of_variation(&p50),
        runtime_p95_pct: coefficient_of_variation(&p95),
        runtime_p99_pct: coefficient_of_variation(&p99),
    }
}

fn load_perf_baseline_summary(path: &Path) -> Result<PerfSummary, String> {
    let bytes = fs::read(path).map_err(|err| err.to_string())?;
    if let Ok(report) = serde_json::from_slice::<PerfReport>(&bytes) {
        return Ok(report.summary);
    }
    serde_json::from_slice::<PerfSummary>(&bytes).map_err(|err| err.to_string())
}

fn evaluate_perf_gate(
    current: &PerfSummary,
    baseline: &PerfSummary,
    max_regression_pct: f64,
    kpi_thresholds: &KpiThresholds,
) -> Vec<String> {
    let mut failures = Vec::new();
    let up = 1.0 + (max_regression_pct / 100.0);
    let down = 1.0 - (max_regression_pct / 100.0);

    let runtime_p50_limit = baseline.runtime_p50_ns as f64 * up;
    if current.runtime_p50_ns as f64 > runtime_p50_limit {
        failures.push(format!(
            "runtime_p50_ns {} > {:.0}",
            current.runtime_p50_ns, runtime_p50_limit
        ));
    }
    let runtime_p95_limit = baseline.runtime_p95_ns as f64 * up;
    if current.runtime_p95_ns as f64 > runtime_p95_limit {
        failures.push(format!(
            "runtime_p95_ns {} > {:.0}",
            current.runtime_p95_ns, runtime_p95_limit
        ));
    }
    let runtime_p99_limit = baseline.runtime_p99_ns as f64 * up;
    if current.runtime_p99_ns as f64 > runtime_p99_limit {
        failures.push(format!(
            "runtime_p99_ns {} > {:.0}",
            current.runtime_p99_ns, runtime_p99_limit
        ));
    }
    let compile_min = baseline.compile_throughput_tests_per_sec * down;
    if current.compile_throughput_tests_per_sec < compile_min {
        failures.push(format!(
            "compile_tps {:.2} < {:.2}",
            current.compile_throughput_tests_per_sec, compile_min
        ));
    }
    let allocs_max = baseline.allocs_per_request * up;
    if current.allocs_per_request > allocs_max {
        failures.push(format!(
            "allocs/request {:.2} > {:.2}",
            current.allocs_per_request, allocs_max
        ));
    }
    let dispatch_min = baseline.dispatch_hit_ratio * down;
    if current.dispatch_hit_ratio < dispatch_min {
        failures.push(format!(
            "dispatch_hit_ratio {:.4} < {:.4}",
            current.dispatch_hit_ratio, dispatch_min
        ));
    }
    if let (Some(current_value), Some(limit)) = (
        current.check_fallback_rate,
        kpi_thresholds.check_fallback_max,
    ) {
        if current_value > limit {
            failures.push(format!(
                "check_fallback_rate {:.4} > {:.4}",
                current_value, limit
            ));
        }
    }
    if let (Some(current_value), Some(min)) =
        (current.avg_check_batch_size, kpi_thresholds.check_batch_min)
    {
        if current_value < min {
            failures.push(format!(
                "avg_check_batch_size {:.2} < {:.2}",
                current_value, min
            ));
        }
    }
    if let (Some(current_value), Some(baseline_value), Some(min_improve_pct)) = (
        current.scheduler_dispatch_p99_ns,
        baseline.scheduler_dispatch_p99_ns,
        kpi_thresholds.scheduler_p99_improve_min_pct,
    ) {
        if baseline_value > 0 {
            let improvement_pct =
                ((baseline_value as f64 - current_value as f64) / baseline_value as f64) * 100.0;
            if improvement_pct < min_improve_pct {
                failures.push(format!(
                    "scheduler_dispatch_p99_ns improvement {:.2}% < {:.2}%",
                    improvement_pct, min_improve_pct
                ));
            }
        }
    }
    if let (Some(current_value), Some(limit)) = (
        current.rewrite_compile_overhead_pct,
        kpi_thresholds.rewrite_overhead_max_pct,
    ) {
        if current_value > limit {
            failures.push(format!(
                "rewrite_compile_overhead_pct {:.2} > {:.2}",
                current_value, limit
            ));
        }
    }
    if let (Some(current_value), Some(baseline_value), Some(min_improve_pct)) = (
        current.actor_msgs_per_sec_p50,
        baseline.actor_msgs_per_sec_p50,
        kpi_thresholds.actor_throughput_improve_min_pct,
    ) {
        if baseline_value > 0.0 {
            let improvement_pct = ((current_value - baseline_value) / baseline_value) * 100.0;
            if improvement_pct < min_improve_pct {
                failures.push(format!(
                    "actor_msgs_per_sec_p50 improvement {:.2}% < {:.2}%",
                    improvement_pct, min_improve_pct
                ));
            }
        }
    }
    if let (Some(current_value), Some(baseline_value), Some(max_regress_pct)) = (
        current.queue_age_p99_ns,
        baseline.queue_age_p99_ns,
        kpi_thresholds.queue_age_p99_max_regress_pct,
    ) {
        if baseline_value > 0 {
            let regress_pct =
                ((current_value as f64 - baseline_value as f64) / baseline_value as f64) * 100.0;
            if regress_pct > max_regress_pct {
                failures.push(format!(
                    "queue_age_p99_ns regression {:.2}% > {:.2}%",
                    regress_pct, max_regress_pct
                ));
            }
        }
    }
    if let Some(max_violations) = kpi_thresholds.starvation_violations_max {
        let current_violations = current
            .scheduler_starvation_violations
            .unwrap_or(current.metrics.sched_starvation_violation);
        if current_violations as f64 > max_violations {
            failures.push(format!(
                "scheduler_starvation_violations {} > {:.0}",
                current_violations, max_violations
            ));
        }
    }
    if let Some(min_improve_pct) = kpi_thresholds.scheduler_throughput_improve_min_pct {
        let baseline_value = baseline.metrics.sched_dispatched as f64;
        let current_value = current.metrics.sched_dispatched as f64;
        if baseline_value > 0.0 {
            let improvement_pct = ((current_value - baseline_value) / baseline_value) * 100.0;
            if improvement_pct < min_improve_pct {
                failures.push(format!(
                    "scheduler_dispatched improvement {:.2}% < {:.2}%",
                    improvement_pct, min_improve_pct
                ));
            }
        }
    }
    if let Some(max_regress_pct) = kpi_thresholds.scheduler_loop_p99_max_regress_pct {
        if let (Some(current_p99), Some(baseline_p99)) = (
            current.scheduler_dispatch_p99_ns,
            baseline.scheduler_dispatch_p99_ns,
        ) {
            if baseline_p99 > 0 {
                let regress_pct =
                    ((current_p99 as f64 - baseline_p99 as f64) / baseline_p99 as f64) * 100.0;
                if regress_pct > max_regress_pct {
                    failures.push(format!(
                        "scheduler_dispatch_p99_ns regression {:.2}% > {:.2}%",
                        regress_pct, max_regress_pct
                    ));
                }
            }
        }
    }
    if let Some(min_ratio) = kpi_thresholds.scheduler_local_hit_min {
        let local = current.metrics.sched_local_dispatch_count as f64;
        let global = current.metrics.sched_global_dispatch_count as f64;
        let total = local + global;
        if total > 0.0 {
            let ratio = local / total;
            if ratio < min_ratio {
                failures.push(format!(
                    "scheduler_local_dispatch_ratio {:.4} < {:.4}",
                    ratio, min_ratio
                ));
            }
        }
    }
    failures
}

fn percentile(samples: &[u128], pct: f64) -> u128 {
    if samples.is_empty() {
        return 0;
    }
    let n = samples.len();
    let rank = (pct * (n as f64 - 1.0)).ceil() as usize;
    samples[rank.min(n - 1)]
}

fn collect_tests(root: &Path, tests_root: &Path, out: &mut Vec<TestCase>) -> io::Result<()> {
    let mut children: Vec<PathBuf> = fs::read_dir(root)?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(|entry| entry.path())
        .collect();
    children.sort_by(|a, b| path_sort_key(a).cmp(&path_sort_key(b)));
    for path in children {
        if path.is_dir() {
            collect_tests(&path, tests_root, out)?;
            continue;
        }
        if path.extension().and_then(|s| s.to_str()) != Some("wr") {
            continue;
        }
        enforce_test_file_suffix(&path)?;
        let source = fs::read_to_string(&path)?;
        let module_path = module_path_for_test_file(&path, tests_root)?;
        collect_tests_from_source(&source, &module_path, true, out)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    }
    Ok(())
}

fn enforce_test_file_suffix(path: &Path) -> io::Result<()> {
    let name = path.file_name().and_then(|s| s.to_str()).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid test file name: {}", path.display()),
        )
    })?;
    if !name.ends_with("_test.wr") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("test file must end with `_test.wr`: {}", path.display()),
        ));
    }
    Ok(())
}

fn module_path_for_test_file(path: &Path, tests_root: &Path) -> io::Result<String> {
    let rel = path.strip_prefix(tests_root).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("test file must live under {}", tests_root.display()),
        )
    })?;
    let mut rel_path = rel.to_path_buf();
    rel_path.set_extension("");
    let mut parts: Vec<String> = rel_path
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if let Some(last) = parts.last_mut() {
        if let Some(stripped) = last.strip_suffix("_test") {
            *last = stripped.to_string();
        }
    }
    Ok(format!("tests/{}", parts.join("/")))
}

fn module_path_for_single_file(path: &Path) -> io::Result<String> {
    let stem = path.file_stem().and_then(|s| s.to_str()).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid test file name: {}", path.display()),
        )
    })?;
    Ok(stem.to_string())
}

fn collect_tests_from_source(
    source: &str,
    module_path: &str,
    enforce_function_name_contract: bool,
    out: &mut Vec<TestCase>,
) -> Result<(), String> {
    use wrela::parser::ast::AstNode;

    let (syntax, parse_errors) = parser::parse_with_errors(source);
    if !parse_errors.is_empty() {
        return Ok(());
    }
    let root = parser::ast::Root::cast(syntax)
        .ok_or_else(|| "internal parser error: expected root syntax node".to_string())?;
    let module = hir::lower::lower(root);
    let lane = infer_test_lane(module_path);
    let mut discovered = Vec::new();
    for (_, func) in module.functions.iter() {
        if func.kind != hir::FunctionKind::Function {
            continue;
        }
        let func_name = func.name.to_string();
        if enforce_function_name_contract
            && func_name.starts_with("test")
            && !is_test_function_name(&func_name)
        {
            return Err(format!(
                "test naming error: {}::{} must start with `test_`",
                module_path, func_name
            ));
        }
        if !is_test_function_name(&func_name) {
            continue;
        }
        let attrs = parse_test_attributes(func);
        if !attrs.unknown.is_empty() {
            return Err(format!(
                "test attribute error: {}::{} uses unsupported attributes [{}]; allowed attributes are @serial, @allows_env_set, @allows_fs_escape",
                module_path,
                func_name,
                attrs.unknown.join(", ")
            ));
        }
        if lane == TestLane::Spec && (attrs.allows_env_set || attrs.allows_fs_escape) {
            return Err(format!(
                "teacher: spec lane forbids capability exceptions; remove @allows_* from {}::{} or move the test under tests/integration/**",
                module_path, func_name
            ));
        }
        if lane != TestLane::Integration && (attrs.allows_env_set || attrs.allows_fs_escape) {
            return Err(format!(
                "test attribute error: capability exceptions are only allowed in integration lane; move {}::{} under tests/integration/**",
                module_path, func_name
            ));
        }
        let stable_id = stable_test_id(module_path, &func_name);
        discovered.push(TestCase {
            id: stable_id.clone(),
            lane,
            name: format!("{module_path}::{func_name}"),
            module_path: module_path.to_string(),
            func_name,
            is_serial: attrs.serial,
            allows_env_set: attrs.allows_env_set,
            allows_fs_escape: attrs.allows_fs_escape,
            has_oracle: function_has_oracle(func),
            generated_call_body: None,
            generated_case_kind: None,
            generated_entry_source: None,
            autogen_module_source: None,
            autogen_seed: None,
            autogen_span: None,
            sim_seed: None,
            canonical_id: stable_id,
        });
    }
    if enforce_function_name_contract && discovered.is_empty() {
        return Err(format!(
            "test discovery error: {} must define at least one `test_` function",
            module_path
        ));
    }
    discovered.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    out.extend(discovered);
    Ok(())
}

#[derive(Default)]
struct ParsedTestAttributes {
    serial: bool,
    allows_env_set: bool,
    allows_fs_escape: bool,
    unknown: Vec<String>,
}

fn parse_test_attributes(func: &hir::Function) -> ParsedTestAttributes {
    let mut parsed = ParsedTestAttributes::default();
    for attr in &func.attributes {
        match attr.as_str() {
            "serial" => parsed.serial = true,
            "allows_env_set" => parsed.allows_env_set = true,
            "allows_fs_escape" => parsed.allows_fs_escape = true,
            other => parsed.unknown.push(format!("@{other}")),
        }
    }
    parsed
}

fn collect_autogen_spec_tests(
    workspace_root: &Path,
    max_cases: u64,
    time_cap_ms: u64,
) -> Result<Vec<TestCase>, String> {
    let max_cases = max_cases as usize;
    if max_cases == 0 {
        return Ok(Vec::new());
    }
    let checks = discover_autogen_checks(workspace_root)?;
    Ok(generate_autogen_spec_tests(&checks, max_cases, time_cap_ms))
}

fn discover_autogen_checks(workspace_root: &Path) -> Result<Vec<AutogenCheckDecl>, String> {
    use wrela::parser::ast::AstNode;

    let mut modules = Vec::new();
    let src_root = workspace_root.join("src");
    collect_wr_modules(&src_root, &src_root, "src", &mut modules)?;
    let tests_root = workspace_root.join("tests");
    let spec_root = tests_root.join("spec");
    collect_wr_modules(&spec_root, &tests_root, "tests", &mut modules)?;

    let mut discovered = Vec::new();
    for module_source in modules {
        let (syntax, parse_errors) = parser::parse_with_errors(&module_source.source);
        if !parse_errors.is_empty() {
            continue;
        }
        let Some(root) = parser::ast::Root::cast(syntax) else {
            continue;
        };
        let module = hir::lower::lower(root);
        for (_, func) in module.functions.iter() {
            if func.kind != hir::FunctionKind::Check {
                continue;
            }
            let Some(check) = autogen_check_decl_from_function(
                &module_source.module_path,
                func.name.as_str(),
                func,
                module_source.source.as_str(),
            ) else {
                continue;
            };
            discovered.push(check);
        }
    }
    discovered.sort_by(|a, b| {
        a.module_path
            .cmp(&b.module_path)
            .then(a.func_name.cmp(&b.func_name))
    });
    Ok(discovered)
}

fn autogen_check_decl_from_function(
    module_path: &str,
    func_name: &str,
    func: &hir::Function,
    module_source: &str,
) -> Option<AutogenCheckDecl> {
    let ret = func.ret_type.as_ref()?;
    if !autogen_type_ref_is_scalar(ret, AutogenScalarType::Boolean) {
        return None;
    }
    let mut params = Vec::with_capacity(func.params.len());
    for param in &func.params {
        let ty = param.ty.as_ref()?;
        let scalar = autogen_scalar_type_from_ref(ty)?;
        params.push(AutogenCheckParam {
            name: param.name.to_string(),
            ty: scalar,
        });
    }
    Some(AutogenCheckDecl {
        module_path: module_path.to_string(),
        func_name: func_name.to_string(),
        params,
        module_source: module_source.to_string(),
        source_span: func
            .name_span
            .map(|span| format!("{}..{}", u32::from(span.start()), u32::from(span.end()))),
    })
}

fn autogen_scalar_type_from_ref(ty: &hir::TypeRef) -> Option<AutogenScalarType> {
    if !ty.args.is_empty() {
        return None;
    }
    match ty.name.as_str() {
        "Integer" => Some(AutogenScalarType::Integer),
        "Boolean" => Some(AutogenScalarType::Boolean),
        "String" => Some(AutogenScalarType::String),
        _ => None,
    }
}

fn autogen_type_ref_is_scalar(ty: &hir::TypeRef, expected: AutogenScalarType) -> bool {
    autogen_scalar_type_from_ref(ty) == Some(expected)
}

fn generate_autogen_spec_tests(
    checks: &[AutogenCheckDecl],
    max_cases: usize,
    time_cap_ms: u64,
) -> Vec<TestCase> {
    let mut generated = Vec::new();
    if checks.is_empty() || max_cases == 0 {
        return generated;
    }
    let started = Instant::now();
    let time_cap = Duration::from_millis(time_cap_ms.max(1));
    let mut case_index = 0usize;
    while generated.len() < max_cases && started.elapsed() < time_cap {
        let before = generated.len();
        for check in checks {
            if generated.len() >= max_cases {
                break;
            }
            if started.elapsed() >= time_cap {
                break;
            }
            let case_seed = fnv1a64(
                format!("{}::{}::{case_index}", check.module_path, check.func_name).as_bytes(),
            );
            let call_body = autogen_given_call(check, case_index);
            generated.push(TestCase {
                id: stable_autogen_test_id(&check.module_path, &check.func_name, case_index),
                lane: TestLane::Spec,
                name: format!(
                    "{}::{}::autogen_case_{:04}",
                    check.module_path, check.func_name, case_index
                ),
                module_path: check.module_path.clone(),
                func_name: check.func_name.clone(),
                is_serial: false,
                allows_env_set: false,
                allows_fs_escape: false,
                has_oracle: true,
                generated_call_body: Some(call_body.clone()),
                generated_case_kind: Some(GeneratedCaseKind::Autogen),
                generated_entry_source: Some(autogen_standalone_entry_source(
                    &check.module_source,
                    &call_body,
                )),
                autogen_module_source: Some(check.module_source.clone()),
                autogen_seed: Some(case_seed),
                autogen_span: check.source_span.clone(),
                sim_seed: None,
                canonical_id: stable_autogen_test_id(
                    &check.module_path,
                    &check.func_name,
                    case_index,
                ),
            });
        }
        if generated.len() == before {
            break;
        }
        case_index = case_index.saturating_add(1);
    }
    generated
}

fn stable_autogen_test_id(module_path: &str, func_name: &str, case_index: usize) -> String {
    format!(
        "autogen:{}",
        fnv1a64_hex(format!("{module_path}::{func_name}::{case_index}").as_bytes())
    )
}

fn autogen_given_call(check: &AutogenCheckDecl, case_index: usize) -> String {
    if check.params.is_empty() {
        return format!("{} given", check.func_name);
    }
    let mut args = Vec::with_capacity(check.params.len());
    for (param_index, param) in check.params.iter().enumerate() {
        let value = autogen_scalar_literal(
            param.ty,
            &check.module_path,
            &check.func_name,
            case_index,
            param_index,
        );
        args.push(format!("{}={value}", param.name));
    }
    format!("{} given {}", check.func_name, args.join(", "))
}

fn autogen_standalone_entry_source(module_source: &str, call_body: &str) -> String {
    let rewritten = module_source.replacen("to run(", "to autogen_hidden_run(", 1);
    format!(
        "{rewritten}\n\nto run() -> Integer:\n    assert value ({call_body}) == true\n    return 0\n"
    )
}

fn autogen_scalar_literal(
    ty: AutogenScalarType,
    module_path: &str,
    func_name: &str,
    case_index: usize,
    param_index: usize,
) -> String {
    let boundary_index = case_index / 2 + param_index;
    if case_index % 2 == 0 {
        return autogen_boundary_literal(ty, boundary_index);
    }
    let seed =
        fnv1a64(format!("{module_path}::{func_name}::{case_index}::{param_index}").as_bytes());
    autogen_random_literal(ty, seed)
}

fn autogen_boundary_literal(ty: AutogenScalarType, boundary_index: usize) -> String {
    match ty {
        AutogenScalarType::Integer => {
            let values = ["0", "1", "-1", "2147483647", "-2147483648"];
            values[boundary_index % values.len()].to_string()
        }
        AutogenScalarType::Boolean => {
            if boundary_index % 2 == 0 {
                "false".to_string()
            } else {
                "true".to_string()
            }
        }
        AutogenScalarType::String => {
            let values = ["\"\"", "\"a\"", "\"edge\"", "\"hello0\"", "\"z9\""];
            values[boundary_index % values.len()].to_string()
        }
    }
}

fn autogen_random_literal(ty: AutogenScalarType, seed: u64) -> String {
    let mut state = autogen_mix64(seed ^ 0xA670);
    match ty {
        AutogenScalarType::Integer => {
            state = autogen_mix64(state);
            let value = (state % 2001) as i64 - 1000;
            value.to_string()
        }
        AutogenScalarType::Boolean => {
            state = autogen_mix64(state);
            if state % 2 == 0 {
                "false".to_string()
            } else {
                "true".to_string()
            }
        }
        AutogenScalarType::String => {
            state = autogen_mix64(state);
            let len = ((state % 8) + 1) as usize;
            let mut out = String::with_capacity(len + 2);
            out.push('"');
            for _ in 0..len {
                state = autogen_mix64(state);
                let ch = match state % 36 {
                    value @ 0..=25 => (b'a' + value as u8) as char,
                    value => (b'0' + (value as u8 - 26)) as char,
                };
                out.push(ch);
            }
            out.push('"');
            out
        }
    }
}

fn autogen_mix64(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn collect_fuzz_tests(
    workspace_root: &Path,
    max_cases: u64,
    time_cap_ms: u64,
) -> Result<Vec<TestCase>, String> {
    let max_cases = max_cases as usize;
    if max_cases == 0 {
        return Ok(Vec::new());
    }
    let targets = discover_fuzz_targets(workspace_root)?;
    Ok(generate_fuzz_tests(&targets, max_cases, time_cap_ms))
}

fn discover_fuzz_targets(workspace_root: &Path) -> Result<Vec<FuzzTargetDecl>, String> {
    use wrela::parser::ast::AstNode;

    let mut modules = Vec::new();
    let src_root = workspace_root.join("src");
    collect_wr_modules(&src_root, &src_root, "src", &mut modules)?;

    let mut discovered = Vec::new();
    for module_source in modules {
        let (syntax, parse_errors) = parser::parse_with_errors(&module_source.source);
        if !parse_errors.is_empty() {
            continue;
        }
        let Some(root) = parser::ast::Root::cast(syntax) else {
            continue;
        };
        let module = hir::lower::lower(root);
        for (_, func) in module.functions.iter() {
            if func.kind != hir::FunctionKind::Function {
                continue;
            }
            let Some(target) = fuzz_target_decl_from_function(
                &module_source.module_path,
                func.name.as_str(),
                func,
                module_source.source.as_str(),
            ) else {
                continue;
            };
            discovered.push(target);
        }
    }
    discovered.sort_by(|a, b| {
        a.module_path
            .cmp(&b.module_path)
            .then(a.func_name.cmp(&b.func_name))
    });
    Ok(discovered)
}

fn fuzz_target_decl_from_function(
    module_path: &str,
    func_name: &str,
    func: &hir::Function,
    module_source: &str,
) -> Option<FuzzTargetDecl> {
    let is_target = func_name.starts_with("try_to_parse_")
        || func_name.starts_with("try_to_decode_")
        || func_name.starts_with("try_to_deserialize_");
    if !is_target {
        return None;
    }
    if func.params.len() != 1 {
        return None;
    }
    let param = &func.params[0];
    let ty = param.ty.as_ref()?;
    let param_ty = fuzz_param_type_from_ref(ty)?;
    Some(FuzzTargetDecl {
        module_path: module_path.to_string(),
        func_name: func_name.to_string(),
        param_name: param.name.to_string(),
        param_ty,
        module_source: module_source.to_string(),
        source_span: func
            .name_span
            .map(|span| format!("{}..{}", u32::from(span.start()), u32::from(span.end()))),
    })
}

fn fuzz_param_type_from_ref(ty: &hir::TypeRef) -> Option<FuzzParamType> {
    if !ty.args.is_empty() {
        return None;
    }
    match ty.name.as_str() {
        "String" => Some(FuzzParamType::String),
        "Bytes" => Some(FuzzParamType::Bytes),
        _ => None,
    }
}

fn generate_fuzz_tests(
    targets: &[FuzzTargetDecl],
    max_cases: usize,
    time_cap_ms: u64,
) -> Vec<TestCase> {
    let mut generated = Vec::new();
    if targets.is_empty() || max_cases == 0 {
        return generated;
    }
    let started = Instant::now();
    let time_cap = Duration::from_millis(time_cap_ms.max(1));
    let mut case_index = 0usize;
    while generated.len() < max_cases && started.elapsed() < time_cap {
        let before = generated.len();
        for target in targets {
            if generated.len() >= max_cases || started.elapsed() >= time_cap {
                break;
            }
            let seed = fnv1a64(
                format!(
                    "fuzz::{}::{}::{case_index}",
                    target.module_path, target.func_name
                )
                .as_bytes(),
            );
            let call_body = fuzz_given_call(target, seed, case_index);
            let case_id = stable_fuzz_test_id(&target.module_path, &target.func_name, case_index);
            generated.push(TestCase {
                id: case_id.clone(),
                lane: TestLane::Integration,
                name: format!(
                    "{}::{}::fuzz_case_{:04}",
                    target.module_path, target.func_name, case_index
                ),
                module_path: target.module_path.clone(),
                func_name: target.func_name.clone(),
                is_serial: false,
                allows_env_set: false,
                allows_fs_escape: false,
                has_oracle: true,
                generated_call_body: Some(call_body.clone()),
                generated_case_kind: Some(GeneratedCaseKind::Fuzz),
                generated_entry_source: Some(fuzz_standalone_entry_source(
                    &target.module_source,
                    &call_body,
                    target.param_ty == FuzzParamType::Bytes,
                )),
                autogen_module_source: Some(target.module_source.clone()),
                autogen_seed: Some(seed),
                autogen_span: target.source_span.clone(),
                sim_seed: None,
                canonical_id: case_id,
            });
        }
        if generated.len() == before {
            break;
        }
        case_index = case_index.saturating_add(1);
    }
    generated
}

fn stable_fuzz_test_id(module_path: &str, func_name: &str, case_index: usize) -> String {
    format!(
        "fuzz:{}",
        fnv1a64_hex(format!("{module_path}::{func_name}::{case_index}").as_bytes())
    )
}

fn fuzz_given_call(target: &FuzzTargetDecl, seed: u64, case_index: usize) -> String {
    let values = fuzz_input_bytes(seed, case_index);
    let arg = match target.param_ty {
        FuzzParamType::String => fuzz_string_literal(&values),
        FuzzParamType::Bytes => format!(
            "get_bytes_from_list(items=[{}])",
            values
                .iter()
                .map(|value| value.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    };
    format!("{} given {}={arg}", target.func_name, target.param_name)
}

fn fuzz_input_bytes(seed: u64, case_index: usize) -> Vec<u8> {
    let mut state = autogen_mix64(seed ^ 0xF022_9E37 ^ case_index as u64);
    let len = ((state % 24) + 1) as usize;
    let mut out = Vec::with_capacity(len);
    for _ in 0..len {
        state = autogen_mix64(state);
        out.push((state % 256) as u8);
    }
    out
}

fn fuzz_string_literal(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() + 2);
    out.push('"');
    for byte in bytes {
        let c = match byte {
            b'"' => "\\\"".to_string(),
            b'\\' => "\\\\".to_string(),
            32..=126 => (*byte as char).to_string(),
            _ => {
                let mapped = b'a' + (byte % 26);
                (mapped as char).to_string()
            }
        };
        out.push_str(&c);
    }
    out.push('"');
    out
}

fn fuzz_standalone_entry_source(
    module_source: &str,
    call_body: &str,
    include_bytes_helper: bool,
) -> String {
    let rewritten = module_source.replacen("to run(", "to fuzz_hidden_run(", 1);
    let bytes_use = if include_bytes_helper {
        "use get_bytes_from_list from bytes\n\n"
    } else {
        ""
    };
    format!(
        "{rewritten}\n\n{bytes_use}to run() -> Integer:\n    ignore result {call_body}\n    return 0\n"
    )
}

fn is_test_function_name(name: &str) -> bool {
    name.starts_with("test_")
}

fn function_has_oracle(func: &hir::Function) -> bool {
    let Some(body) = func.body.as_ref() else {
        return false;
    };
    body_has_oracle(body, &body.root_stmts)
}

fn body_has_oracle(body: &hir::Body, stmts: &[hir::Idx<hir::Stmt>]) -> bool {
    for stmt_id in stmts {
        match &body.stmts[*stmt_id] {
            hir::Stmt::Assert { .. } | hir::Stmt::Require { .. } => return true,
            hir::Stmt::Optimize { body: nested, .. } => {
                if body_has_oracle(body, nested) {
                    return true;
                }
            }
            hir::Stmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                if body_has_oracle(body, then_branch)
                    || else_branch
                        .as_ref()
                        .is_some_and(|branch| body_has_oracle(body, branch))
                {
                    return true;
                }
            }
            hir::Stmt::For {
                body: loop_body, ..
            }
            | hir::Stmt::While {
                body: loop_body, ..
            } => {
                if body_has_oracle(body, loop_body) {
                    return true;
                }
            }
            hir::Stmt::Match {
                cases, otherwise, ..
            } => {
                if cases.iter().any(|case| body_has_oracle(body, &case.body))
                    || otherwise
                        .as_ref()
                        .is_some_and(|branch| body_has_oracle(body, branch))
                {
                    return true;
                }
            }
            hir::Stmt::Expr(_)
            | hir::Stmt::Let { .. }
            | hir::Stmt::Assign { .. }
            | hir::Stmt::IgnoreResult { .. }
            | hir::Stmt::Capture { .. }
            | hir::Stmt::Defer { .. }
            | hir::Stmt::Return(_)
            | hir::Stmt::Use { .. }
            | hir::Stmt::Break
            | hir::Stmt::Continue => {}
        }
    }
    false
}

fn stable_test_id(module_path: &str, func_name: &str) -> String {
    fnv1a64_hex(format!("{module_path}::{func_name}").as_bytes())
}

fn stable_function_id(function_identity: &str) -> String {
    fnv1a64(function_identity.as_bytes()).to_string()
}

fn stable_legacy_function_id(function_name: &str) -> String {
    fnv1a64(function_name.as_bytes()).to_string()
}

fn qualified_function_identity(module_path: &str, function_name: &str) -> String {
    format!("{module_path}::{function_name}")
}

fn infer_test_lane(module_path: &str) -> TestLane {
    let canonical = module_path.replace('\\', "/").to_ascii_lowercase();
    let segments: Vec<&str> = canonical
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    let lane_segment = segments
        .windows(2)
        .find(|window| window[0] == "tests")
        .map(|window| window[1])
        .or_else(|| segments.first().copied())
        .unwrap_or_default();
    match lane_segment {
        "spec" => TestLane::Spec,
        "integration" => TestLane::Integration,
        "sim" => TestLane::Sim,
        "model" => TestLane::Model,
        _ => TestLane::Default,
    }
}

fn parse_test_lane_filter(value: &str) -> Option<TestLane> {
    match value.trim().to_ascii_lowercase().as_str() {
        "spec" => Some(TestLane::Spec),
        "integration" => Some(TestLane::Integration),
        "sim" => Some(TestLane::Sim),
        "model" => Some(TestLane::Model),
        "default" => Some(TestLane::Default),
        _ => None,
    }
}

fn enforce_serial_test_cap(tests: &[TestCase]) -> Result<(), String> {
    let total = tests.len();
    if total == 0 {
        return Ok(());
    }
    let serial_count = tests.iter().filter(|test| test.is_serial).count();
    if serial_count == 0 {
        return Ok(());
    }
    let pct_cap = ((total as f64) * 0.05).ceil() as usize;
    let pct_cap = pct_cap.max(1);
    if serial_count <= pct_cap && serial_count <= 10 {
        return Ok(());
    }
    Err(format!(
        "serial test cap exceeded: {} serial tests out of {} total. policy is <=5% (cap {}) and <=10 absolute. reduce @serial usage or redesign tests to run in parallel",
        serial_count, total, pct_cap
    ))
}

fn select_tests(mut tests: Vec<TestCase>, selection: &TestSelection) -> Vec<TestCase> {
    if let Some(include_ids) = selection.include_ids.as_ref() {
        tests.retain(|test| include_ids.contains(&test.id));
    }
    if let Some(id) = selection.id.as_ref() {
        tests.retain(|test| test.id == *id);
    }
    if let Some(pattern) = selection.filter.as_ref() {
        tests.retain(|test| {
            test.name.contains(pattern)
                || test.id.contains(pattern)
                || test.module_path.contains(pattern)
                || test.lane.as_str().contains(pattern)
        });
    }
    if let Some(lane) = selection.lane {
        tests.retain(|test| test.lane == lane);
    }
    tests
}

fn expand_sim_seed_cases(
    tests: Vec<TestCase>,
    sim_seed_override: Option<u64>,
    certify_mode: bool,
) -> Vec<TestCase> {
    let mut expanded = Vec::new();
    for test in tests {
        if test.lane != TestLane::Sim && test.lane != TestLane::Model {
            expanded.push(test);
            continue;
        }
        if let Some(seed) = sim_seed_override {
            expanded.push(sim_seed_variant(&test, seed));
            continue;
        }
        if certify_mode {
            let max_seed = if test.lane == TestLane::Sim {
                256u64
            } else {
                64u64
            };
            for seed in 0..max_seed {
                expanded.push(sim_seed_variant(&test, seed));
            }
            continue;
        }
        expanded.push(sim_seed_variant(&test, TEST_JSON_SUMMARY_SEED));
    }
    expanded
}

fn sim_seed_variant(test: &TestCase, seed: u64) -> TestCase {
    let mut variant = test.clone();
    variant.sim_seed = Some(seed);
    variant.id = format!("{}::seed:{seed}", test.id);
    variant.name = format!("{} [seed={}]", test.name, seed);
    variant
}

fn list_tests(tests: &[TestCase]) {
    for test in tests {
        let mut attrs = Vec::new();
        if test.is_serial {
            attrs.push("@serial");
        }
        if test.allows_env_set {
            attrs.push("@allows_env_set");
        }
        if test.allows_fs_escape {
            attrs.push("@allows_fs_escape");
        }
        let attrs_suffix = if attrs.is_empty() {
            String::new()
        } else {
            format!(" attrs={}", attrs.join(","))
        };
        println!(
            "id={} lane={} name={}{}",
            test.id,
            test.lane.as_str(),
            test.name,
            attrs_suffix
        );
    }
    println!("tests: {} listed", tests.len());
}

fn summarize_run_lane(tests: &[TestCase]) -> String {
    let Some(first) = tests.first() else {
        return "none".to_string();
    };
    let first_lane = first.lane.as_str();
    if tests.iter().all(|test| test.lane.as_str() == first_lane) {
        first_lane.to_string()
    } else {
        "mixed".to_string()
    }
}

fn summarize_run_lane_from_json_cases(cases: &[TestJsonCase]) -> String {
    let Some(first) = cases.first() else {
        return "none".to_string();
    };
    let first_lane = first.lane.as_str();
    if cases.iter().all(|case| case.lane == first_lane) {
        first_lane.to_string()
    } else {
        "mixed".to_string()
    }
}

fn emit_test_json_summary(summary: &TestJsonSummary) {
    println!(
        "{}",
        serde_json::to_string(summary).unwrap_or_else(|_| "{}".to_string())
    );
}

fn compile_test_harness(
    workspace_root: &Path,
    compile_root: &Path,
    tests_root: Option<&Path>,
    tests: &[TestCase],
    output_format: OutputFormat,
    harness_cache: Option<&mut HashMap<String, TestHarness>>,
) -> Result<TestHarness, String> {
    let temp_dir = workspace_root.join("target").join("wrela_tests");
    fs::create_dir_all(&temp_dir)
        .map_err(|err| format!("failed to create test temp directory: {err}"))?;
    let mut cache_key_hasher = Fnv1a64::new();
    cache_key_hasher.update(compile_root.to_string_lossy().as_bytes());
    cache_key_hasher.update(&[0]);
    if let Some(root) = tests_root {
        cache_key_hasher.update(root.to_string_lossy().as_bytes());
        cache_key_hasher.update(&[0]);
    }
    for test in tests {
        cache_key_hasher.update(test.id.as_bytes());
        cache_key_hasher.update(&[0]);
        cache_key_hasher.update(test.module_path.as_bytes());
        cache_key_hasher.update(&[0]);
        cache_key_hasher.update(test.func_name.as_bytes());
        cache_key_hasher.update(&[0]);
    }
    let harness_key = format!("harness_{}", cache_key_hasher.finish_hex());
    if let Some(cache) = harness_cache.as_ref()
        && let Some(existing) = cache.get(&harness_key)
    {
        return Ok(TestHarness {
            exe_path: existing.exe_path.clone(),
            compile_ns: 0,
        });
    }
    let run_dir = temp_dir.join(&harness_key);
    fs::create_dir_all(&run_dir)
        .map_err(|err| format!("failed to create harness directory: {err}"))?;
    let entry_path = run_dir.join("entry.wr");
    let exe_path = run_dir.join("harness_bin");

    let mut source = String::new();
    let harness_tests: Vec<&TestCase> = tests
        .iter()
        .filter(|test| test.generated_entry_source.is_none())
        .collect();
    let mut dispatch_arms: Vec<(String, String)> = Vec::with_capacity(harness_tests.len());

    let use_wrappers = tests_root.is_some() && has_duplicate_test_function_names(&harness_tests);
    let mut wrappers_root: Option<PathBuf> = None;
    if use_wrappers {
        let tests_root = tests_root.expect("project tests root");
        let wrappers_dir = tests_root
            .join("wrela_harness")
            .join(&harness_key)
            .join("cases");
        fs::create_dir_all(&wrappers_dir).map_err(|err| {
            format!(
                "failed to create harness cases directory {}: {err}",
                wrappers_dir.display()
            )
        })?;
        wrappers_root = Some(tests_root.join("wrela_harness").join(&harness_key));
        for (idx, test) in harness_tests.iter().enumerate() {
            let wrapper_func = format!("run_case_{idx}");
            let wrapper_module = format!("tests/wrela_harness/{harness_key}/cases/case_{idx}");
            let wrapper_source = format!(
                "use {func} from {module}\n\nto {wrapper_func}() -> Nothing:\n    {dispatch}\n",
                func = test.func_name,
                module = test.module_path,
                dispatch = test_case_dispatch_stmt(test)
            );
            let wrapper_path = wrappers_dir.join(format!("case_{idx}.wr"));
            fs::write(&wrapper_path, wrapper_source)
                .map_err(|err| format!("failed to write harness case wrapper: {err}"))?;
            source.push_str(&format!("use {wrapper_func} from {wrapper_module}\n"));
            dispatch_arms.push((test.id.clone(), wrapper_func));
        }
    } else {
        let mut helpers = String::new();
        for (idx, test) in harness_tests.iter().enumerate() {
            let dispatch_func = format!("run_case_{idx}");
            source.push_str(&format!(
                "use {func} from {module}\n",
                func = test.func_name,
                module = test.module_path
            ));
            helpers.push_str(&format!(
                "to {dispatch_func}() -> Nothing:\n    {dispatch}\n",
                dispatch = test_case_dispatch_stmt(test)
            ));
            dispatch_arms.push((test.id.clone(), dispatch_func));
        }
        source.push('\n');
        source.push_str(&helpers);
    }
    source.push('\n');
    source.push_str("to run() -> Integer:\n");
    source.push_str("    selected_value = __wr_env_get(\"WRELA_TEST_ID\")\n");
    source.push_str("    mutable selected = \"\"\n");
    source.push_str("    match selected_value:\n");
    source.push_str("        String:\n");
    source.push_str("            selected = selected_value\n");
    source.push_str("        otherwise:\n");
    source.push_str("            selected = \"\"\n");
    for (id, dispatch_func) in &dispatch_arms {
        source.push_str(&format!("    if selected == \"{id}\":\n"));
        source.push_str(&format!("        {dispatch_func}()\n"));
        source.push_str("        return 0\n");
    }
    source.push_str("    return 4\n");

    fs::write(&entry_path, source).map_err(|err| format!("failed to write test harness: {err}"))?;

    let trace = std::env::var("WRELA_BUILD_TRACE").is_ok();
    if trace {
        eprintln!(
            "build: test harness compile start ({} dispatched tests)",
            harness_tests.len()
        );
    }
    let compile_start = Instant::now();
    let mir_module = compile_to_mir_with_root(&entry_path, compile_root, tests_root, output_format)
        .map_err(|_| "compile failed".to_string())?;
    wrela::backend::cranelift::compile_to_executable(&mir_module, &exe_path)
        .map_err(|err| format!("codegen error: {}", err.0))?;
    let compile_ns = compile_start.elapsed().as_nanos();
    if trace {
        eprintln!(
            "build: test harness compile done ({:.2?})",
            compile_start.elapsed()
        );
    }
    if let Some(path) = wrappers_root {
        let _ = fs::remove_dir_all(path);
    }
    let harness = TestHarness {
        exe_path,
        compile_ns,
    };
    if let Some(cache) = harness_cache {
        cache.insert(harness_key, harness.clone());
    }
    Ok(harness)
}

fn test_case_dispatch_stmt(test: &TestCase) -> String {
    if let Some(call_body) = test.generated_call_body.as_ref() {
        format!("assert value ({call_body}) == true")
    } else {
        format!("{}()", test.func_name)
    }
}

fn has_duplicate_test_function_names(tests: &[&TestCase]) -> bool {
    let mut names = HashSet::new();
    for test in tests {
        if !names.insert(test.func_name.clone()) {
            return true;
        }
    }
    false
}

fn run_single_test(
    harness_exe_path: &Path,
    workspace_root: &Path,
    test: &TestCase,
    timeout: Duration,
    output_format: OutputFormat,
    http_mode: HttpCassetteMode,
    pipeline: DifferentialPipeline,
) -> Result<TestRun, String> {
    let temp_dir = harness_exe_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let _ = fs::create_dir_all(&temp_dir);
    let file_stem = test.id.replace('/', "_").replace(':', "_");
    let metrics_path = temp_dir.join(format!("{}_metrics.json", file_stem));
    let _ = fs::remove_file(&metrics_path);
    let test_temp_dir = temp_dir
        .join("cases")
        .join(sanitize_test_path_component(&test.id));
    fs::create_dir_all(&test_temp_dir)
        .map_err(|err| format!("failed to create per-test temp directory: {err}"))?;
    let runtime_start = Instant::now();
    if let Some(delay_ms) = synthetic_slowdown_ms() {
        std::thread::sleep(Duration::from_millis(delay_ms));
    }
    let mut extra_env_owned: Vec<(String, String)> = vec![
        ("WRELA_TEST_ID".to_string(), test.id.clone()),
        (
            "WRELA_TEST_TEMP".to_string(),
            test_temp_dir.to_string_lossy().to_string(),
        ),
        (
            "WRELA_WORKSPACE_ROOT".to_string(),
            workspace_root.to_string_lossy().to_string(),
        ),
        (
            "WRELA_HTTP_MODE".to_string(),
            http_mode.as_env_value().to_string(),
        ),
        (
            "WRELA_DIFF_PIPELINE".to_string(),
            pipeline.as_env_value().to_string(),
        ),
    ];
    if test.lane == TestLane::Spec || test.lane == TestLane::Sim {
        extra_env_owned.push(("WRELA_TEST_VIRTUAL_TIME".to_string(), "1".to_string()));
        extra_env_owned.push(("WRELA_VIRTUAL_TIME_START_NS".to_string(), "0".to_string()));
    }
    if test.lane == TestLane::Spec {
        extra_env_owned.push((
            "WRELA_SPEC_FS_ROOT".to_string(),
            test_temp_dir.to_string_lossy().to_string(),
        ));
    }
    if let Some(seed) = test.sim_seed {
        let seed_value = seed.to_string();
        if test.lane == TestLane::Sim {
            extra_env_owned.push(("WRELA_SCHED_SEED".to_string(), seed_value.clone()));
        }
        if test.lane == TestLane::Model {
            extra_env_owned.push(("WRELA_MODEL_SEED".to_string(), seed_value));
        }
    }
    let extra_env: Vec<(&str, &str)> = extra_env_owned
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    let local_autogen_exe = if let Some(source) = test.generated_entry_source.as_ref() {
        let entry_path = test_temp_dir.join("autogen_entry.wr");
        fs::write(&entry_path, source)
            .map_err(|err| format!("failed to write autogen test entry: {err}"))?;
        let autogen_exe = test_temp_dir.join("autogen_bin");
        let src_root = workspace_root.join("src");
        let tests_root = workspace_root.join("tests");
        let mir_module = compile_to_mir_with_root(
            &entry_path,
            &src_root,
            tests_root.is_dir().then_some(tests_root.as_path()),
            output_format,
        )
        .map_err(|_| format!("autogen compile failed: {}", test.name))?;
        wrela::backend::cranelift::compile_to_executable(&mir_module, &autogen_exe)
            .map_err(|err| format!("autogen codegen error: {}", err.0))?;
        Some(autogen_exe)
    } else {
        None
    };
    let exec_path = local_autogen_exe.as_deref().unwrap_or(harness_exe_path);

    run_with_timeout(
        exec_path,
        timeout,
        Some(&metrics_path),
        Some(&test_temp_dir),
        &[],
        &extra_env,
    )?;
    let runtime_ns = runtime_start.elapsed().as_nanos();
    let metrics = read_metrics_dump(&metrics_path);
    Ok(TestRun {
        metrics,
        runtime_ns,
    })
}

fn execute_test_case(
    harness_exe_path: &Path,
    workspace_root: &Path,
    test: &TestCase,
    timeout: Duration,
    output_format: OutputFormat,
    http_mode: HttpCassetteMode,
    pipeline: DifferentialPipeline,
    certify_mode: bool,
) -> (bool, String, Option<TestRun>) {
    let result = run_single_test(
        harness_exe_path,
        workspace_root,
        test,
        timeout,
        output_format,
        http_mode,
        pipeline,
    );
    match result {
        Ok(run) => (true, String::new(), Some(run)),
        Err(msg) => {
            let mut detail = String::new();
            let mut failure_msg = msg;
            if test.lane == TestLane::Sim || test.lane == TestLane::Model {
                if let Some(seed) = test.sim_seed {
                    if certify_mode && test.lane == TestLane::Sim {
                        let mut replay_ok = 0usize;
                        for _ in 0..3 {
                            if run_single_test(
                                harness_exe_path,
                                workspace_root,
                                test,
                                timeout,
                                output_format,
                                http_mode,
                                pipeline,
                            )
                            .is_ok()
                            {
                                replay_ok += 1;
                            }
                        }
                        if replay_ok > 0 {
                            failure_msg.push_str(&format!(
                                " | determinism confirmation failed: {replay_ok}/3 reruns passed unexpectedly"
                            ));
                        }
                    }
                    let replay_hint = format!(
                        "wrela test --lane={} --seed={seed} --id={} .",
                        test.lane.as_str(),
                        test.canonical_id
                    );
                    detail.push_str(&format!(" replay=`{replay_hint}`"));
                    let trace_path = if test.lane == TestLane::Sim {
                        write_sim_trace_artifact(workspace_root, test, &failure_msg)
                    } else {
                        write_model_trace_artifact(workspace_root, test, &failure_msg)
                    };
                    if let Ok(path) = trace_path {
                        detail.push_str(&format!(" trace={}", path.display()));
                    }
                }
            }
            if let Some(call) = test.generated_call_body.as_ref() {
                match test.generated_case_kind {
                    Some(GeneratedCaseKind::Autogen) => {
                        detail.push_str(&format!(
                            " | autogen failure: check={}::{} seed={} span={} call=`{}`",
                            test.module_path,
                            test.func_name,
                            test.autogen_seed.unwrap_or(TEST_JSON_SUMMARY_SEED),
                            test.autogen_span.as_deref().unwrap_or("unknown"),
                            call
                        ));
                        match repro::write_autogen_repro_artifact(
                            workspace_root,
                            harness_exe_path,
                            test,
                            timeout,
                            output_format,
                            http_mode,
                            &failure_msg,
                        ) {
                            Ok((path, shrunk_call)) => {
                                if let Some(shrunk) = shrunk_call {
                                    detail.push_str(&format!(" shrunk_call=`{shrunk}`"));
                                }
                                detail.push_str(&format!(" repro={}", path.display()));
                            }
                            Err(err) => {
                                detail
                                    .push_str(&format!(" repro_error={}", err.replace('\n', " ")));
                            }
                        }
                    }
                    Some(GeneratedCaseKind::Fuzz) => {
                        detail.push_str(&format!(
                            " | fuzz failure: target={}::{} seed={} span={} call=`{}`",
                            test.module_path,
                            test.func_name,
                            test.autogen_seed.unwrap_or(TEST_JSON_SUMMARY_SEED),
                            test.autogen_span.as_deref().unwrap_or("unknown"),
                            call
                        ));
                        match repro::write_fuzz_repro_artifact(workspace_root, test, &failure_msg) {
                            Ok(path) => {
                                detail.push_str(&format!(" repro={}", path.display()));
                            }
                            Err(err) => {
                                detail
                                    .push_str(&format!(" repro_error={}", err.replace('\n', " ")));
                            }
                        }
                    }
                    None => {}
                }
            }
            (false, format!("{failure_msg}{detail}"), None)
        }
    }
}

fn write_sim_trace_artifact(
    workspace_root: &Path,
    test: &TestCase,
    failure: &str,
) -> Result<PathBuf, String> {
    #[derive(Serialize)]
    struct SimTraceArtifact {
        version: u32,
        generated_at_unix_ms: u128,
        test_id: String,
        canonical_test_id: String,
        lane: String,
        seed: u64,
        failure: String,
        event_log: Vec<String>,
    }

    let seed = test.sim_seed.unwrap_or(TEST_JSON_SUMMARY_SEED);
    let artifact_dir = workspace_root
        .join("tests")
        .join(".artifacts")
        .join("sim")
        .join(sanitize_test_path_component(&test.canonical_id));
    fs::create_dir_all(&artifact_dir).map_err(|err| {
        format!(
            "failed to create sim artifact directory {}: {}",
            artifact_dir.display(),
            err
        )
    })?;
    let artifact_path = artifact_dir.join(format!("{seed}.json"));
    let payload = serde_json::to_vec_pretty(&SimTraceArtifact {
        version: 1,
        generated_at_unix_ms: now_unix_ms(),
        test_id: test.id.clone(),
        canonical_test_id: test.canonical_id.clone(),
        lane: test.lane.as_str().to_string(),
        seed,
        failure: failure.to_string(),
        event_log: vec![
            format!("dispatch.start seed={seed}"),
            format!("dispatch.fail test={} seed={seed}", test.canonical_id),
        ],
    })
    .map_err(|err| err.to_string())?;
    fs::write(&artifact_path, payload).map_err(|err| {
        format!(
            "failed to write sim trace artifact {}: {}",
            artifact_path.display(),
            err
        )
    })?;
    Ok(artifact_path)
}

fn write_model_trace_artifact(
    workspace_root: &Path,
    test: &TestCase,
    failure: &str,
) -> Result<PathBuf, String> {
    #[derive(Serialize)]
    struct ModelTraceArtifact {
        version: u32,
        generated_at_unix_ms: u128,
        test_id: String,
        canonical_test_id: String,
        lane: String,
        seed: u64,
        failure: String,
        command_trace: Vec<String>,
    }

    let seed = test.sim_seed.unwrap_or(TEST_JSON_SUMMARY_SEED);
    let artifact_dir = workspace_root
        .join("tests")
        .join(".artifacts")
        .join("model")
        .join(sanitize_test_path_component(&test.canonical_id));
    fs::create_dir_all(&artifact_dir).map_err(|err| {
        format!(
            "failed to create model artifact directory {}: {}",
            artifact_dir.display(),
            err
        )
    })?;
    let artifact_path = artifact_dir.join(format!("{seed}.json"));
    let payload = serde_json::to_vec_pretty(&ModelTraceArtifact {
        version: 1,
        generated_at_unix_ms: now_unix_ms(),
        test_id: test.id.clone(),
        canonical_test_id: test.canonical_id.clone(),
        lane: test.lane.as_str().to_string(),
        seed,
        failure: failure.to_string(),
        command_trace: vec![
            format!("model.seed={seed}"),
            format!("model.failure test={} seed={seed}", test.canonical_id),
        ],
    })
    .map_err(|err| err.to_string())?;
    fs::write(&artifact_path, payload).map_err(|err| {
        format!(
            "failed to write model trace artifact {}: {}",
            artifact_path.display(),
            err
        )
    })?;
    Ok(artifact_path)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum AutogenValue {
    Integer(i64),
    Boolean(bool),
    String(String),
    List(Vec<AutogenValue>),
    Raw(String),
}

fn shrink_autogen_call(
    harness_exe_path: &Path,
    workspace_root: &Path,
    test: &TestCase,
    timeout: Duration,
    output_format: OutputFormat,
    http_mode: HttpCassetteMode,
) -> Option<String> {
    let call = test.generated_call_body.as_ref()?;
    let (func_name, mut args) = parse_autogen_call(call)?;
    if args.is_empty() {
        return None;
    }
    let mut changed = false;
    let mut attempts = 0usize;
    for idx in 0..args.len() {
        loop {
            if attempts >= 128 {
                break;
            }
            let candidates = shrink_value_candidates(&args[idx].1);
            let mut improved = false;
            for candidate in candidates {
                if candidate == args[idx].1 {
                    continue;
                }
                let mut trial_args = args.clone();
                trial_args[idx].1 = candidate;
                let trial_call = render_autogen_call(&func_name, &trial_args);
                if autogen_call_still_fails(
                    harness_exe_path,
                    workspace_root,
                    test,
                    timeout,
                    output_format,
                    http_mode,
                    &trial_call,
                ) {
                    args = trial_args;
                    changed = true;
                    improved = true;
                    attempts += 1;
                    break;
                }
                attempts += 1;
                if attempts >= 128 {
                    break;
                }
            }
            if !improved {
                break;
            }
        }
    }
    if changed {
        Some(render_autogen_call(&func_name, &args))
    } else {
        None
    }
}

fn autogen_call_still_fails(
    harness_exe_path: &Path,
    workspace_root: &Path,
    test: &TestCase,
    timeout: Duration,
    output_format: OutputFormat,
    http_mode: HttpCassetteMode,
    candidate_call: &str,
) -> bool {
    let Some(candidate_test) = autogen_test_with_call(test, candidate_call) else {
        return false;
    };
    run_single_test(
        harness_exe_path,
        workspace_root,
        &candidate_test,
        timeout,
        output_format,
        http_mode,
        DifferentialPipeline::Baseline,
    )
    .is_err()
}

fn autogen_test_with_call(test: &TestCase, call_body: &str) -> Option<TestCase> {
    let module_source = test.autogen_module_source.as_ref()?;
    let mut candidate = test.clone();
    candidate.generated_call_body = Some(call_body.to_string());
    candidate.generated_entry_source =
        Some(autogen_standalone_entry_source(module_source, call_body));
    Some(candidate)
}

fn parse_autogen_call(call: &str) -> Option<(String, Vec<(String, AutogenValue)>)> {
    let (func_name, args_raw) = call.split_once(" given ")?;
    let func_name = func_name.trim().to_string();
    if func_name.is_empty() {
        return None;
    }
    if args_raw.trim().is_empty() {
        return Some((func_name, Vec::new()));
    }
    let mut args = Vec::new();
    for chunk in split_top_level(args_raw, ',') {
        let trimmed = chunk.trim();
        let (name, value_raw) = trimmed.split_once('=')?;
        let value_raw = value_raw.trim();
        args.push((name.trim().to_string(), parse_autogen_value(value_raw)));
    }
    Some((func_name, args))
}

fn parse_autogen_value(raw: &str) -> AutogenValue {
    let trimmed = raw.trim();
    if trimmed == "true" {
        return AutogenValue::Boolean(true);
    }
    if trimmed == "false" {
        return AutogenValue::Boolean(false);
    }
    if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2 {
        return AutogenValue::String(trimmed[1..trimmed.len() - 1].to_string());
    }
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        let inner = &trimmed[1..trimmed.len() - 1];
        if inner.trim().is_empty() {
            return AutogenValue::List(Vec::new());
        }
        let elements = split_top_level(inner, ',')
            .into_iter()
            .map(|part| parse_autogen_value(part.trim()))
            .collect();
        return AutogenValue::List(elements);
    }
    if let Ok(value) = trimmed.parse::<i64>() {
        return AutogenValue::Integer(value);
    }
    AutogenValue::Raw(trimmed.to_string())
}

fn render_autogen_call(func_name: &str, args: &[(String, AutogenValue)]) -> String {
    if args.is_empty() {
        return format!("{func_name} given");
    }
    let rendered_args = args
        .iter()
        .map(|(name, value)| format!("{name}={}", render_autogen_value(value)))
        .collect::<Vec<_>>()
        .join(", ");
    format!("{func_name} given {rendered_args}")
}

fn render_autogen_value(value: &AutogenValue) -> String {
    match value {
        AutogenValue::Integer(v) => v.to_string(),
        AutogenValue::Boolean(v) => {
            if *v {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        AutogenValue::String(v) => format!("\"{}\"", v.replace('\"', "\\\"")),
        AutogenValue::List(values) => format!(
            "[{}]",
            values
                .iter()
                .map(render_autogen_value)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        AutogenValue::Raw(v) => v.clone(),
    }
}

fn shrink_value_candidates(value: &AutogenValue) -> Vec<AutogenValue> {
    let mut candidates = Vec::new();
    match value {
        AutogenValue::Integer(v) => {
            if *v != 0 {
                candidates.push(AutogenValue::Integer(0));
                let half = v / 2;
                if half != *v && half != 0 {
                    candidates.push(AutogenValue::Integer(half));
                }
                if *v > 1 {
                    candidates.push(AutogenValue::Integer(1));
                } else if *v < -1 {
                    candidates.push(AutogenValue::Integer(-1));
                }
            }
        }
        AutogenValue::String(v) => {
            if !v.is_empty() {
                candidates.push(AutogenValue::String(String::new()));
                let half_len = v.chars().count() / 2;
                if half_len > 0 {
                    let shorter = v.chars().take(half_len).collect::<String>();
                    if shorter.len() < v.len() {
                        candidates.push(AutogenValue::String(shorter));
                    }
                }
            }
        }
        AutogenValue::List(items) => {
            if !items.is_empty() {
                candidates.push(AutogenValue::List(Vec::new()));
                if items.len() > 1 {
                    candidates.push(AutogenValue::List(items[..items.len() / 2].to_vec()));
                }
                candidates.push(AutogenValue::List(items[..items.len() - 1].to_vec()));
            }
        }
        AutogenValue::Boolean(true) => {
            candidates.push(AutogenValue::Boolean(false));
        }
        AutogenValue::Boolean(false) | AutogenValue::Raw(_) => {}
    }
    dedupe_autogen_values(candidates)
}

fn dedupe_autogen_values(values: Vec<AutogenValue>) -> Vec<AutogenValue> {
    let mut out = Vec::new();
    for value in values {
        if !out.contains(&value) {
            out.push(value);
        }
    }
    out
}

fn split_top_level(input: &str, delimiter: char) -> Vec<String> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut depth = 0usize;
    let mut in_string = false;
    let bytes = input.as_bytes();
    let mut idx = 0usize;
    while idx < bytes.len() {
        let ch = bytes[idx] as char;
        match ch {
            '"' => {
                let escaped = idx > 0 && bytes[idx - 1] == b'\\';
                if !escaped {
                    in_string = !in_string;
                }
            }
            '[' if !in_string => depth = depth.saturating_add(1),
            ']' if !in_string && depth > 0 => depth -= 1,
            _ if ch == delimiter && !in_string && depth == 0 => {
                parts.push(input[start..idx].to_string());
                start = idx + ch.len_utf8();
            }
            _ => {}
        }
        idx += ch.len_utf8();
    }
    parts.push(input[start..].to_string());
    parts
}

fn synthetic_slowdown_ms() -> Option<u64> {
    let raw = env::var("WRELA_TEST_SLOWDOWN_MS").ok()?;
    raw.parse::<u64>().ok()
}

fn sanitize_test_path_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        "test".to_string()
    } else {
        out
    }
}

fn inherited_test_env_keys() -> &'static [&'static str] {
    &[
        "PATH",
        "HOME",
        "USERPROFILE",
        "TMPDIR",
        "TMP",
        "TEMP",
        "SYSTEMROOT",
        "WINDIR",
        "COMSPEC",
        "PATHEXT",
        "LANG",
        "LC_ALL",
        "TZ",
        "SSL_CERT_FILE",
        "SSL_CERT_DIR",
        "LD_LIBRARY_PATH",
        "DYLD_LIBRARY_PATH",
    ]
}

fn read_metrics_dump(path: &Path) -> Option<MetricsDump> {
    let data = fs::read(path).ok()?;
    serde_json::from_slice(&data).ok()
}

fn write_function_coverage_snapshot(
    path: &Path,
    snapshot: &BTreeMap<String, u64>,
) -> Result<(), String> {
    #[derive(Serialize)]
    struct FunctionCoverageSnapshotArtifact<'a> {
        schema_version: u32,
        function_coverage: &'a BTreeMap<String, u64>,
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create {}: {}", parent.display(), err))?;
    }
    let payload = serde_json::to_vec(&FunctionCoverageSnapshotArtifact {
        schema_version: COVERAGE_SNAPSHOT_SCHEMA_VERSION,
        function_coverage: snapshot,
    })
    .map_err(|err| {
        format!(
            "failed to serialize function coverage snapshot {}: {}",
            path.display(),
            err
        )
    })?;
    fs::write(path, payload).map_err(|err| format!("failed to write {}: {}", path.display(), err))
}

fn load_function_coverage_snapshot(path: &Path) -> Result<BTreeMap<String, u64>, String> {
    #[derive(Deserialize)]
    struct FunctionCoverageSnapshotArtifact {
        schema_version: u32,
        function_coverage: BTreeMap<String, u64>,
    }
    let payload =
        fs::read(path).map_err(|err| format!("failed to read {}: {}", path.display(), err))?;
    let artifact: FunctionCoverageSnapshotArtifact = serde_json::from_slice(&payload)
        .map_err(|err| format!("failed to parse {}: {}", path.display(), err))?;
    if artifact.schema_version != COVERAGE_SNAPSHOT_SCHEMA_VERSION {
        return Err(format!(
            "stale function coverage snapshot schema in {}: expected {}, got {}",
            path.display(),
            COVERAGE_SNAPSHOT_SCHEMA_VERSION,
            artifact.schema_version
        ));
    }
    Ok(artifact.function_coverage)
}

fn certification_coverage_index_path(workspace_root: &Path, cert_cache_hash: &str) -> PathBuf {
    workspace_root
        .join("target")
        .join("wrela_cert")
        .join("index")
        .join(format!("{cert_cache_hash}.json"))
}

fn write_function_test_coverage_index(
    path: &Path,
    index: &BTreeMap<String, Vec<String>>,
) -> Result<(), String> {
    #[derive(Serialize)]
    struct FunctionCoverageIndexArtifact<'a> {
        schema_version: u32,
        function_to_tests: &'a BTreeMap<String, Vec<String>>,
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create {}: {}", parent.display(), err))?;
    }
    let payload = serde_json::to_vec(&FunctionCoverageIndexArtifact {
        schema_version: COVERAGE_INDEX_SCHEMA_VERSION,
        function_to_tests: index,
    })
    .map_err(|err| {
        format!(
            "failed to serialize function test coverage index {}: {}",
            path.display(),
            err
        )
    })?;
    fs::write(path, payload).map_err(|err| format!("failed to write {}: {}", path.display(), err))
}

fn load_function_test_coverage_index(
    workspace_root: &Path,
    cert_cache_hash: &str,
) -> Result<BTreeMap<String, Vec<String>>, String> {
    #[derive(Deserialize)]
    struct FunctionCoverageIndexArtifact {
        schema_version: u32,
        function_to_tests: BTreeMap<String, Vec<String>>,
    }
    let path = certification_coverage_index_path(workspace_root, cert_cache_hash);
    let payload =
        fs::read(&path).map_err(|err| format!("failed to read {}: {}", path.display(), err))?;
    let artifact: FunctionCoverageIndexArtifact = serde_json::from_slice(&payload)
        .map_err(|err| format!("failed to parse {}: {}", path.display(), err))?;
    if artifact.schema_version != COVERAGE_INDEX_SCHEMA_VERSION {
        return Err(format!(
            "stale function coverage index schema in {}: expected {}, got {}",
            path.display(),
            COVERAGE_INDEX_SCHEMA_VERSION,
            artifact.schema_version
        ));
    }
    Ok(artifact.function_to_tests)
}

fn build_function_test_coverage_index(
    summary: Option<&PerfSummary>,
    legacy_to_qualified_ids: &BTreeMap<String, String>,
) -> BTreeMap<String, Vec<String>> {
    let mut grouped: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let Some(cases) = summary.and_then(|value| value.cases.as_ref()) else {
        return BTreeMap::new();
    };
    for case in cases {
        let test_id = if case.id.is_empty() {
            match case.name.rsplit_once("::") {
                Some((module_path, func_name)) => stable_test_id(module_path, func_name),
                None => continue,
            }
        } else {
            case.id.clone()
        };
        let Some(metrics) = case.metrics.as_ref() else {
            continue;
        };
        for (function_id, hits) in &metrics.function_coverage {
            if *hits == 0 {
                continue;
            }
            let canonical_function_id = legacy_to_qualified_ids
                .get(function_id)
                .cloned()
                .unwrap_or_else(|| function_id.clone());
            grouped
                .entry(canonical_function_id)
                .or_default()
                .insert(test_id.clone());
        }
    }
    grouped
        .into_iter()
        .map(|(function_id, test_ids)| (function_id, test_ids.into_iter().collect()))
        .collect()
}

fn canonicalize_function_coverage(
    function_coverage: &BTreeMap<String, u64>,
    legacy_to_qualified_ids: &BTreeMap<String, String>,
) -> BTreeMap<String, u64> {
    let mut canonical = BTreeMap::new();
    for (function_id, hits) in function_coverage {
        let canonical_id = legacy_to_qualified_ids
            .get(function_id)
            .cloned()
            .unwrap_or_else(|| function_id.clone());
        *canonical.entry(canonical_id).or_insert(0) += *hits;
    }
    canonical
}

fn collect_source_function_id_aliases(
    workspace_root: &Path,
) -> Result<BTreeMap<String, String>, String> {
    use wrela::parser::ast::AstNode;

    let src_root = workspace_root.join("src");
    let mut modules = Vec::new();
    collect_wr_modules(&src_root, &src_root, "src", &mut modules)?;
    modules.sort_by(|a, b| a.module_path.cmp(&b.module_path));

    let mut aliases = BTreeMap::new();
    for module in modules {
        let (syntax, parse_errors) = parser::parse_with_errors(&module.source);
        if !parse_errors.is_empty() {
            let first = &parse_errors[0];
            return Err(format!(
                "coverage id mapping requires parse-clean src modules: {} ({} parse error(s), first: {})",
                module.rel_path,
                parse_errors.len(),
                first.message
            ));
        }
        let Some(root) = parser::ast::Root::cast(syntax) else {
            return Err(format!(
                "coverage id mapping failed: parser produced no root for {}",
                module.rel_path
            ));
        };
        let lowered = hir::lower::lower(root);
        for (_, function) in lowered.functions.iter() {
            if !matches!(
                function.kind,
                hir::FunctionKind::Function | hir::FunctionKind::Check
            ) {
                continue;
            }
            let qualified_identity =
                qualified_function_identity(&module.module_path, function.name.as_str());
            let legacy_id = stable_legacy_function_id(function.name.as_str());
            let canonical_id = stable_function_id(&qualified_identity);
            if let Some(previous) = aliases.get(&legacy_id)
                && previous != &canonical_id
            {
                return Err(format!(
                    "coverage id collision during hard cutover: legacy id {} maps to multiple functions; this build must be resolved before certification",
                    legacy_id
                ));
            }
            aliases.insert(legacy_id, canonical_id);
        }
    }
    Ok(aliases)
}

fn run_mutation_gate(
    workspace_root: &Path,
    summary: &PerfSummary,
    max_cases: usize,
    time_cap_ms: u64,
) -> Result<MutationGateOutcome, String> {
    if max_cases == 0 {
        return Ok(MutationGateOutcome {
            summary_hash: None,
            discovery_ms: 0,
            execution_ms: 0,
        });
    }
    let started = Instant::now();
    let time_cap = Duration::from_millis(time_cap_ms.max(1));
    let discovery_start = Instant::now();
    let legacy_to_qualified_ids = collect_source_function_id_aliases(workspace_root)?;
    let coverage_index =
        build_function_test_coverage_index(Some(summary), &legacy_to_qualified_ids);
    let snapshot = build_public_surface_snapshot(workspace_root)?;
    let authored_tests = discover_authored_tests_for_mutation(workspace_root)?;
    let mut importable_by_module: BTreeMap<String, BTreeMap<String, ImportableFunctionInfo>> =
        BTreeMap::new();
    for item in snapshot
        .items
        .iter()
        .filter(|item| is_importable_coverage_target(&item.qualified_name))
    {
        let Some((module_path, function_name)) = item.qualified_name.rsplit_once("::") else {
            continue;
        };
        let function_id = stable_function_id(&item.qualified_name);
        importable_by_module
            .entry(module_path.to_string())
            .or_default()
            .insert(
                function_name.to_string(),
                ImportableFunctionInfo {
                    qualified_name: item.qualified_name.clone(),
                    function_id,
                },
            );
    }
    let mir_module = compile_mutation_discovery_module(workspace_root, &importable_by_module)?;
    let mut candidates = Vec::new();
    let mut seen_candidates = BTreeSet::new();
    for functions in importable_by_module.values() {
        for candidate in discover_mir_mutation_candidates(&mir_module, functions) {
            let key = mutation_candidate_key(&candidate);
            if seen_candidates.insert(key) {
                candidates.push(candidate);
            }
        }
    }
    candidates.sort_by(|a, b| {
        a.qualified_name
            .cmp(&b.qualified_name)
            .then(a.function_name.cmp(&b.function_name))
            .then(a.op_index.cmp(&b.op_index))
            .then(a.mutation_type.cmp(b.mutation_type))
    });
    let discovery_ms = discovery_start.elapsed().as_millis();
    let authored_by_id: HashMap<String, TestCase> = authored_tests
        .into_iter()
        .map(|test| (test.id.clone(), test))
        .collect();
    let source_hash = hash_source_fingerprint(workspace_root)
        .map_err(|err| format!("mutation cache source hash error: {err}"))?;
    let toolchain_version = resolve_toolchain_version();
    let cache_enabled = mutation_cache_enabled();
    let cache_root = mutation_cache_root(workspace_root);
    if cache_enabled {
        let _ = fs::create_dir_all(&cache_root);
    }
    let history_path = mutation_kill_history_path(&cache_root);
    let mut history = load_mutation_kill_history(&history_path);

    let execution_start = Instant::now();
    let mut queued_jobs = Vec::new();
    let mut ordered_results = Vec::new();
    let mut total = 0usize;
    let mutation_cap = max_cases.min(candidates.len());
    for (job_index, candidate) in candidates.into_iter().take(mutation_cap).enumerate() {
        if started.elapsed() >= time_cap {
            break;
        }
        total += 1;
        let selected_ids = coverage_index
            .get(&candidate.function_id)
            .cloned()
            .unwrap_or_default();
        let tests_to_run: Vec<TestCase> = selected_ids
            .iter()
            .filter_map(|id| authored_by_id.get(id).cloned())
            .collect();
        if tests_to_run.is_empty() {
            ordered_results.push((
                job_index,
                MutationMutantResult {
                    function: candidate.qualified_name.clone(),
                    function_id: candidate.function_id.clone(),
                    mutation_type: candidate.mutation_type.to_string(),
                    tests_ran: Vec::new(),
                    compile_ms: 0,
                    test_run_ms: 0,
                    status: "survived".to_string(),
                    reason: Some("no-covering-tests".to_string()),
                },
                0usize,
                0usize,
                0usize,
            ));
            continue;
        }
        let ordered_tests = order_tests_for_mutation_candidate(&candidate, tests_to_run, &history);
        queued_jobs.push(MutationCandidateJob {
            job_index,
            candidate,
            tests_to_run: ordered_tests,
        });
    }

    let mutation_workers = resolve_mutation_workers();
    let worker_count = mutation_workers.min(queued_jobs.len().max(1));
    let queue = std::sync::Arc::new(std::sync::Mutex::new(std::collections::VecDeque::from(
        queued_jobs,
    )));
    let (tx, rx) = std::sync::mpsc::channel::<MutationExecutionResult>();
    let context = std::sync::Arc::new(MutationExecutionContext {
        workspace_root: workspace_root.to_path_buf(),
        source_hash,
        toolchain_version,
        cache_root,
        cache_enabled,
    });
    let mut handles = Vec::new();
    for _ in 0..worker_count {
        let queue = std::sync::Arc::clone(&queue);
        let tx = tx.clone();
        let worker_context = std::sync::Arc::clone(&context);
        handles.push(std::thread::spawn(move || {
            loop {
                let next_job = match queue.lock() {
                    Ok(mut guard) => guard.pop_front(),
                    Err(_) => None,
                };
                let Some(job) = next_job else { break };
                let (mutant, cache_hits, cache_misses, cache_invalidations) =
                    run_mutation_job(&worker_context, &job);
                let _ = tx.send(MutationExecutionResult {
                    job_index: job.job_index,
                    mutant,
                    cache_hits,
                    cache_misses,
                    cache_invalidations,
                });
            }
        }));
    }
    drop(tx);
    for result in rx {
        ordered_results.push((
            result.job_index,
            result.mutant,
            result.cache_hits,
            result.cache_misses,
            result.cache_invalidations,
        ));
    }
    for handle in handles {
        if handle.join().is_err() {
            return Err(
                "mutation gate worker panic: mutation execution aborted before report completion"
                    .to_string(),
            );
        }
    }
    ordered_results.sort_by_key(|(job_index, _, _, _, _)| *job_index);
    let cache_hits: usize = ordered_results.iter().map(|(_, _, hits, _, _)| *hits).sum();
    let cache_misses: usize = ordered_results
        .iter()
        .map(|(_, _, _, misses, _)| *misses)
        .sum();
    let cache_invalidations: usize = ordered_results
        .iter()
        .map(|(_, _, _, _, invalidations)| *invalidations)
        .sum();
    let mutants: Vec<MutationMutantResult> = ordered_results
        .into_iter()
        .map(|(_, mutant, _, _, _)| mutant)
        .collect();
    update_mutation_kill_history_from_mutants(&mut history, &mutants);
    let _ = write_mutation_kill_history(&history_path, &history);

    let invalid = mutants
        .iter()
        .filter(|mutant| mutant.status == "invalid-mutant")
        .count();
    let survived = mutants
        .iter()
        .filter(|mutant| mutant.status == "survived")
        .count();
    let killed = mutants
        .iter()
        .filter(|mutant| mutant.status == "killed")
        .count();
    let no_covering = mutants
        .iter()
        .filter(|mutant| mutant.reason.as_deref() == Some("no-covering-tests"))
        .count();
    let valid = killed + survived;
    let kill_rate_pct = if valid == 0 {
        100.0
    } else {
        (killed as f64 / valid as f64) * 100.0
    };
    let domain_kill_rate_pct = if valid == 0 {
        None
    } else {
        Some(kill_rate_pct)
    };

    let report = MutationGateReport {
        version: 4,
        generated_at_unix_ms: now_unix_ms(),
        discovery_ms,
        execution_ms: execution_start.elapsed().as_millis(),
        compile_total_ms: mutants.iter().map(|mutant| mutant.compile_ms).sum(),
        test_run_total_ms: mutants.iter().map(|mutant| mutant.test_run_ms).sum(),
        parallel_workers: worker_count.max(1),
        cache_hits,
        cache_misses,
        cache_invalidations,
        total_mutants: total,
        valid_mutants: valid,
        invalid_mutants: invalid,
        killed_mutants: killed,
        survived_mutants: survived,
        no_covering_tests_mutants: no_covering,
        kill_rate_pct,
        domain_application_kill_rate_pct: domain_kill_rate_pct,
        mutants,
    };
    let report_path = workspace_root
        .join("tests")
        .join(".artifacts")
        .join("mutation")
        .join("report.json");
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create {}: {}", parent.display(), err))?;
    }
    let payload = serde_json::to_vec_pretty(&report)
        .map_err(|err| format!("failed to serialize mutation report: {err}"))?;
    fs::write(&report_path, &payload).map_err(|err| {
        format!(
            "failed to write mutation report {}: {}",
            report_path.display(),
            err
        )
    })?;
    let summary_hash = fnv1a64_hex(&payload);
    let execution_ms = report.execution_ms;

    let mut failures = Vec::new();
    if report.survived_mutants > 0 {
        let survivors = report
            .mutants
            .iter()
            .filter(|mutant| mutant.status == "survived")
            .map(|mutant| {
                let reason = mutant.reason.as_deref().unwrap_or("tests-passed");
                format!("  - {} [{}]", mutant.function, reason)
            })
            .collect::<Vec<_>>()
            .join("\n");
        failures.push(format!(
            "mutation gate failed: {} survived mutants detected under src/domain/** and src/application/**.\nsurvivors:\n{}\naction: add assertions/tests that kill these mutants",
            report.survived_mutants,
            survivors
        ));
    }
    if let Some(rate) = report.domain_application_kill_rate_pct
        && rate < 85.0
    {
        failures.push(format!(
            "domain/application mutation kill rate {:.2}% is below required 85.00%",
            rate
        ));
    }
    if !failures.is_empty() {
        return Err(format!(
            "{}\nmutation report: {}",
            failures.join("\n"),
            report_path.display()
        ));
    }
    Ok(MutationGateOutcome {
        summary_hash: Some(summary_hash),
        discovery_ms,
        execution_ms,
    })
}

fn compile_mutation_discovery_module(
    workspace_root: &Path,
    importable_by_module: &BTreeMap<String, BTreeMap<String, ImportableFunctionInfo>>,
) -> Result<mir::ir::MirModule, String> {
    let mut source = String::new();
    for (module_path, functions) in importable_by_module {
        if functions.is_empty() {
            continue;
        }
        let imports = functions.keys().cloned().collect::<Vec<_>>().join(", ");
        source.push_str(&format!("use {imports} from {module_path}\n"));
    }
    source.push_str("\nto run() -> Integer:\n    return 0\n");

    let discovery_root = workspace_root
        .join("target")
        .join("wrela_mutation")
        .join("discovery");
    fs::create_dir_all(&discovery_root)
        .map_err(|err| format!("failed to create {}: {}", discovery_root.display(), err))?;
    let entry_path = discovery_root.join(format!("project_{}.wr", fnv1a64_hex(source.as_bytes())));
    fs::write(&entry_path, source).map_err(|err| {
        format!(
            "mutation gate failed to write discovery entry {}: {}",
            entry_path.display(),
            err
        )
    })?;

    let src_root = workspace_root.join("src");
    let tests_root = workspace_root.join("tests");
    compile_to_mir_with_root(
        &entry_path,
        &src_root,
        tests_root.is_dir().then_some(tests_root.as_path()),
        OutputFormat::Pretty,
    )
    .map_err(|code| {
        format!(
            "mutation gate failed to compile MIR discovery entry {} (exit code {code})",
            entry_path.display()
        )
    })
}

#[derive(Clone, Copy)]
enum MutationSite {
    Branch { block_idx: usize },
    Comparison { block_idx: usize, stmt_idx: usize },
    IntegerLiteralUse { block_idx: usize, stmt_idx: usize },
    IntegerLiteralBinaryLhs { block_idx: usize, stmt_idx: usize },
    IntegerLiteralBinaryRhs { block_idx: usize, stmt_idx: usize },
    ResultGuard { block_idx: usize, stmt_idx: usize },
}

#[derive(Clone)]
struct MirMutationCandidate {
    qualified_name: String,
    function_name: String,
    function_id: String,
    mutation_type: &'static str,
    op_index: usize,
    site: MutationSite,
}

#[derive(Clone)]
struct ImportableFunctionInfo {
    qualified_name: String,
    function_id: String,
}

fn discover_mir_mutation_candidates(
    module: &mir::ir::MirModule,
    importable_functions: &BTreeMap<String, ImportableFunctionInfo>,
) -> Vec<MirMutationCandidate> {
    let mut candidates = Vec::new();
    for function in &module.functions {
        let function_name = function.name.to_string();
        let Some(importable) = importable_functions.get(&function_name) else {
            continue;
        };
        let mut op_index = 0usize;
        for (block_idx, block) in function.blocks.iter().enumerate() {
            if let mir::ir::Terminator::Branch { .. } = block.terminator {
                candidates.push(MirMutationCandidate {
                    qualified_name: importable.qualified_name.clone(),
                    function_name: function_name.clone(),
                    function_id: importable.function_id.clone(),
                    mutation_type: "conditional_branch_inversion",
                    op_index,
                    site: MutationSite::Branch { block_idx },
                });
                op_index += 1;
            }
            for (stmt_idx, stmt) in block.stmts.iter().enumerate() {
                let mir::ir::Stmt::Assign { value, .. } = stmt else {
                    continue;
                };
                match value {
                    mir::ir::Rvalue::Binary { op, lhs, rhs } => {
                        if invertible_comparison(*op).is_some() {
                            candidates.push(MirMutationCandidate {
                                qualified_name: importable.qualified_name.clone(),
                                function_name: function_name.clone(),
                                function_id: importable.function_id.clone(),
                                mutation_type: "comparison_inversion",
                                op_index,
                                site: MutationSite::Comparison {
                                    block_idx,
                                    stmt_idx,
                                },
                            });
                            op_index += 1;
                        }
                        if matches!(lhs, mir::ir::Value::Const(hir::Literal::Integer(_))) {
                            candidates.push(MirMutationCandidate {
                                qualified_name: importable.qualified_name.clone(),
                                function_name: function_name.clone(),
                                function_id: importable.function_id.clone(),
                                mutation_type: "integer_literal_perturbation",
                                op_index,
                                site: MutationSite::IntegerLiteralBinaryLhs {
                                    block_idx,
                                    stmt_idx,
                                },
                            });
                            op_index += 1;
                        }
                        if matches!(rhs, mir::ir::Value::Const(hir::Literal::Integer(_))) {
                            candidates.push(MirMutationCandidate {
                                qualified_name: importable.qualified_name.clone(),
                                function_name: function_name.clone(),
                                function_id: importable.function_id.clone(),
                                mutation_type: "integer_literal_perturbation",
                                op_index,
                                site: MutationSite::IntegerLiteralBinaryRhs {
                                    block_idx,
                                    stmt_idx,
                                },
                            });
                            op_index += 1;
                        }
                    }
                    mir::ir::Rvalue::Use(mir::ir::Value::Const(hir::Literal::Integer(_))) => {
                        candidates.push(MirMutationCandidate {
                            qualified_name: importable.qualified_name.clone(),
                            function_name: function_name.clone(),
                            function_id: importable.function_id.clone(),
                            mutation_type: "integer_literal_perturbation",
                            op_index,
                            site: MutationSite::IntegerLiteralUse {
                                block_idx,
                                stmt_idx,
                            },
                        });
                        op_index += 1;
                    }
                    mir::ir::Rvalue::ResultIsOk { .. } => {
                        candidates.push(MirMutationCandidate {
                            qualified_name: importable.qualified_name.clone(),
                            function_name: function_name.clone(),
                            function_id: importable.function_id.clone(),
                            mutation_type: "result_guard_perturbation",
                            op_index,
                            site: MutationSite::ResultGuard {
                                block_idx,
                                stmt_idx,
                            },
                        });
                        op_index += 1;
                    }
                    _ => {}
                }
            }
        }
    }
    candidates
}

fn discover_authored_tests_for_mutation(workspace_root: &Path) -> Result<Vec<TestCase>, String> {
    let tests_root = workspace_root.join("tests");
    if !tests_root.is_dir() {
        return Ok(Vec::new());
    }
    let mut discovered = Vec::new();
    collect_tests(&tests_root, &tests_root, &mut discovered).map_err(|err| err.to_string())?;
    discovered.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    Ok(expand_sim_seed_cases(discovered, None, true))
}

fn mutation_candidate_key(candidate: &MirMutationCandidate) -> String {
    let site = match candidate.site {
        MutationSite::Branch { block_idx } => format!("branch:{block_idx}"),
        MutationSite::Comparison {
            block_idx,
            stmt_idx,
        } => {
            format!("comparison:{block_idx}:{stmt_idx}")
        }
        MutationSite::IntegerLiteralUse {
            block_idx,
            stmt_idx,
        } => {
            format!("int_use:{block_idx}:{stmt_idx}")
        }
        MutationSite::IntegerLiteralBinaryLhs {
            block_idx,
            stmt_idx,
        } => {
            format!("int_lhs:{block_idx}:{stmt_idx}")
        }
        MutationSite::IntegerLiteralBinaryRhs {
            block_idx,
            stmt_idx,
        } => {
            format!("int_rhs:{block_idx}:{stmt_idx}")
        }
        MutationSite::ResultGuard {
            block_idx,
            stmt_idx,
        } => {
            format!("result_guard:{block_idx}:{stmt_idx}")
        }
    };
    format!(
        "{}|{}|{}|{}",
        candidate.qualified_name, candidate.function_name, candidate.mutation_type, site
    )
}

fn resolve_mutation_workers() -> usize {
    const DEFAULT_WORKER_CAP: usize = 4;
    const ABSOLUTE_WORKER_CAP: usize = 16;
    let default_workers = std::thread::available_parallelism()
        .map(|value| value.get().min(DEFAULT_WORKER_CAP))
        .unwrap_or(1);
    let requested = std::env::var("WRELA_MUTATION_WORKERS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(default_workers);
    requested.clamp(1, ABSOLUTE_WORKER_CAP)
}

fn run_mutation_job(
    context: &MutationExecutionContext,
    job: &MutationCandidateJob,
) -> (MutationMutantResult, usize, usize, usize) {
    let compile_attempt_start = Instant::now();
    let mut tests_ran = Vec::new();
    let compile_result =
        compile_mutant_binary_for_tests(context, &job.candidate, &job.tests_to_run);
    let compile = match compile_result {
        Ok(outcome) => outcome,
        Err(failure) => {
            if context.cache_enabled {
                let _ = persist_invalid_mutation_cache_entry(
                    context,
                    &job.candidate,
                    &failure.reason,
                    failure.compile_ms,
                );
            }
            return (
                MutationMutantResult {
                    function: job.candidate.qualified_name.clone(),
                    function_id: job.candidate.function_id.clone(),
                    mutation_type: job.candidate.mutation_type.to_string(),
                    tests_ran,
                    compile_ms: failure
                        .compile_ms
                        .max(compile_attempt_start.elapsed().as_millis()),
                    test_run_ms: 0,
                    status: "invalid-mutant".to_string(),
                    reason: Some(failure.reason),
                },
                failure.cache_hits,
                failure.cache_misses,
                failure.cache_invalidations,
            );
        }
    };

    let timeout = Duration::from_millis(DEFAULT_TEST_TIMEOUT_MS);
    let run_start = Instant::now();
    let mut killed = false;
    for test in &job.tests_to_run {
        tests_ran.push(test.id.clone());
        let run = run_single_test(
            &compile.exe_path,
            &context.workspace_root,
            test,
            timeout,
            OutputFormat::Pretty,
            HttpCassetteMode::Replay,
            DifferentialPipeline::Baseline,
        );
        if run.is_err() {
            killed = true;
            break;
        }
    }
    let test_run_ms = run_start.elapsed().as_millis();
    (
        MutationMutantResult {
            function: job.candidate.qualified_name.clone(),
            function_id: job.candidate.function_id.clone(),
            mutation_type: job.candidate.mutation_type.to_string(),
            tests_ran,
            compile_ms: compile.compile_ms,
            test_run_ms,
            status: if killed {
                "killed".to_string()
            } else {
                "survived".to_string()
            },
            reason: None,
        },
        compile.cache_hits,
        compile.cache_misses,
        compile.cache_invalidations,
    )
}

fn compile_mutant_binary_for_tests(
    context: &MutationExecutionContext,
    candidate: &MirMutationCandidate,
    tests: &[TestCase],
) -> Result<MutantCompileSuccess, MutantCompileFailure> {
    let candidate_key = mutation_candidate_key(candidate);
    let cache_key = mutation_cache_key(
        &context.source_hash,
        &context.toolchain_version,
        &candidate_key,
    );
    let cache_entry_dir = context.cache_root.join(&cache_key);
    let cache_metadata_path = cache_entry_dir.join("metadata.json");
    let cache_bin_path = cache_entry_dir.join("mutant_bin");
    let mut cache_invalidations = 0usize;
    if context.cache_enabled
        && let Some(metadata) = load_mutation_cache_metadata(&cache_metadata_path)
    {
        let valid_metadata = metadata.schema_version == MUTATION_CACHE_SCHEMA_VERSION
            && metadata.toolchain_version == context.toolchain_version
            && metadata.source_hash == context.source_hash
            && metadata.candidate_key == candidate_key;
        if valid_metadata && metadata.build_status == "ok" && cache_bin_path.is_file() {
            return Ok(MutantCompileSuccess {
                exe_path: cache_bin_path,
                compile_ms: 0,
                cache_hits: 1,
                cache_misses: 0,
                cache_invalidations: 0,
            });
        }
        if valid_metadata && metadata.build_status == "invalid" {
            return Err(MutantCompileFailure {
                reason: metadata
                    .invalid_reason
                    .unwrap_or_else(|| "cached invalid mutant".to_string()),
                compile_ms: 0,
                cache_hits: 1,
                cache_misses: 0,
                cache_invalidations: 0,
            });
        }
        cache_invalidations += 1;
        let _ = fs::remove_dir_all(&cache_entry_dir);
    }

    let mutation_key = sanitize_test_path_component(&format!(
        "{}__{}__{}",
        candidate.function_name, candidate.mutation_type, candidate.op_index
    ));
    let mutation_root = context
        .workspace_root
        .join("target")
        .join("wrela_mutation")
        .join(&mutation_key);
    fs::create_dir_all(&mutation_root).map_err(|err| MutantCompileFailure {
        reason: format!("failed to create mutation temp directory: {err}"),
        compile_ms: 0,
        cache_hits: 0,
        cache_misses: usize::from(context.cache_enabled),
        cache_invalidations,
    })?;
    let entry_path = mutation_root.join("entry.wr");
    let exe_path = if context.cache_enabled {
        if let Err(err) = fs::create_dir_all(&cache_entry_dir) {
            return Err(MutantCompileFailure {
                reason: format!(
                    "failed to create mutation cache directory {}: {err}",
                    cache_entry_dir.display()
                ),
                compile_ms: 0,
                cache_hits: 0,
                cache_misses: usize::from(context.cache_enabled),
                cache_invalidations,
            });
        }
        cache_bin_path.clone()
    } else {
        mutation_root.join("mutant_bin")
    };

    let (entry_source, wrappers_root) =
        mutation_dispatch_entry_source(&context.workspace_root, &mutation_key, tests).map_err(
            |err| MutantCompileFailure {
                reason: err,
                compile_ms: 0,
                cache_hits: 0,
                cache_misses: usize::from(context.cache_enabled),
                cache_invalidations,
            },
        )?;
    fs::write(&entry_path, entry_source).map_err(|err| MutantCompileFailure {
        reason: format!("failed to write mutation harness entry: {err}"),
        compile_ms: 0,
        cache_hits: 0,
        cache_misses: usize::from(context.cache_enabled),
        cache_invalidations,
    })?;

    let compile_start = Instant::now();
    let src_root = context.workspace_root.join("src");
    let tests_root = context.workspace_root.join("tests");
    let mut module = compile_to_mir_with_root(
        &entry_path,
        &src_root,
        tests_root.is_dir().then_some(tests_root.as_path()),
        OutputFormat::Pretty,
    )
    .map_err(|code| MutantCompileFailure {
        reason: format!("mutant compile failed before mutation (exit code {code})"),
        compile_ms: compile_start.elapsed().as_millis(),
        cache_hits: 0,
        cache_misses: usize::from(context.cache_enabled),
        cache_invalidations,
    })?;
    apply_mir_mutation(&mut module, candidate).map_err(|err| MutantCompileFailure {
        reason: err,
        compile_ms: compile_start.elapsed().as_millis(),
        cache_hits: 0,
        cache_misses: usize::from(context.cache_enabled),
        cache_invalidations,
    })?;
    wrela::backend::cranelift::compile_to_executable(&module, &exe_path).map_err(|err| {
        MutantCompileFailure {
            reason: format!("mutant codegen error: {}", err.0),
            compile_ms: compile_start.elapsed().as_millis(),
            cache_hits: 0,
            cache_misses: usize::from(context.cache_enabled),
            cache_invalidations,
        }
    })?;
    let compile_ms = compile_start.elapsed().as_millis();
    let _ = fs::remove_dir_all(wrappers_root);

    if context.cache_enabled {
        let metadata = MutationCacheMetadata {
            schema_version: MUTATION_CACHE_SCHEMA_VERSION,
            toolchain_version: context.toolchain_version.clone(),
            source_hash: context.source_hash.clone(),
            candidate_key,
            mutant_binary_path: exe_path.display().to_string(),
            build_status: "ok".to_string(),
            invalid_reason: None,
            compile_ms,
        };
        let _ = write_json_atomic(&cache_metadata_path, &metadata);
    }

    Ok(MutantCompileSuccess {
        exe_path,
        compile_ms,
        cache_hits: 0,
        cache_misses: usize::from(context.cache_enabled),
        cache_invalidations,
    })
}

fn mutation_cache_enabled() -> bool {
    match std::env::var("WRELA_MUTATION_CACHE") {
        Ok(value) => !matches!(value.to_ascii_lowercase().as_str(), "off" | "false" | "0"),
        Err(_) => true,
    }
}

fn mutation_cache_root(workspace_root: &Path) -> PathBuf {
    workspace_root.join("target").join("wrela_mutation_cache")
}

fn mutation_kill_history_path(cache_root: &Path) -> PathBuf {
    cache_root.join("kill_history.json")
}

fn mutation_cache_key(source_hash: &str, toolchain_version: &str, candidate_key: &str) -> String {
    let mut hasher = Fnv1a64::new();
    hasher.update(MUTATION_CACHE_ENGINE_TAG.as_bytes());
    hasher.update(&[0]);
    hasher.update(source_hash.as_bytes());
    hasher.update(&[0]);
    hasher.update(toolchain_version.as_bytes());
    hasher.update(&[0]);
    hasher.update(candidate_key.as_bytes());
    hasher.finish_hex()
}

fn persist_invalid_mutation_cache_entry(
    context: &MutationExecutionContext,
    candidate: &MirMutationCandidate,
    reason: &str,
    compile_ms: u128,
) -> Result<(), String> {
    let candidate_key = mutation_candidate_key(candidate);
    let cache_key = mutation_cache_key(
        &context.source_hash,
        &context.toolchain_version,
        &candidate_key,
    );
    let entry_dir = context.cache_root.join(cache_key);
    let metadata_path = entry_dir.join("metadata.json");
    let metadata = MutationCacheMetadata {
        schema_version: MUTATION_CACHE_SCHEMA_VERSION,
        toolchain_version: context.toolchain_version.clone(),
        source_hash: context.source_hash.clone(),
        candidate_key,
        mutant_binary_path: entry_dir.join("mutant_bin").display().to_string(),
        build_status: "invalid".to_string(),
        invalid_reason: Some(reason.to_string()),
        compile_ms,
    };
    write_json_atomic(&metadata_path, &metadata)
}

fn load_mutation_cache_metadata(path: &Path) -> Option<MutationCacheMetadata> {
    let bytes = fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn load_mutation_kill_history(path: &Path) -> MutationKillHistoryArtifact {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => {
            return MutationKillHistoryArtifact {
                schema_version: MUTATION_KILL_HISTORY_SCHEMA_VERSION,
                entries: BTreeMap::new(),
            };
        }
    };
    let artifact: MutationKillHistoryArtifact = match serde_json::from_slice(&bytes) {
        Ok(artifact) => artifact,
        Err(_) => {
            return MutationKillHistoryArtifact {
                schema_version: MUTATION_KILL_HISTORY_SCHEMA_VERSION,
                entries: BTreeMap::new(),
            };
        }
    };
    if artifact.schema_version != MUTATION_KILL_HISTORY_SCHEMA_VERSION {
        return MutationKillHistoryArtifact {
            schema_version: MUTATION_KILL_HISTORY_SCHEMA_VERSION,
            entries: BTreeMap::new(),
        };
    }
    artifact
}

fn write_mutation_kill_history(
    path: &Path,
    history: &MutationKillHistoryArtifact,
) -> Result<(), String> {
    write_json_atomic(path, history)
}

fn mutation_history_key(function_id: &str, mutation_type: &str, test_id: &str) -> String {
    format!("{function_id}|{mutation_type}|{test_id}")
}

fn order_tests_for_mutation_candidate(
    candidate: &MirMutationCandidate,
    mut tests: Vec<TestCase>,
    history: &MutationKillHistoryArtifact,
) -> Vec<TestCase> {
    tests.sort_by(|a, b| {
        let key_a = mutation_history_key(&candidate.function_id, candidate.mutation_type, &a.id);
        let key_b = mutation_history_key(&candidate.function_id, candidate.mutation_type, &b.id);
        let score_a = history.entries.get(&key_a);
        let score_b = history.entries.get(&key_b);
        let kills_a = score_a.map(|entry| entry.kills).unwrap_or(0);
        let attempts_a = score_a.map(|entry| entry.attempts).unwrap_or(0);
        let kills_b = score_b.map(|entry| entry.kills).unwrap_or(0);
        let attempts_b = score_b.map(|entry| entry.attempts).unwrap_or(0);
        let rate_lhs = (kills_a as u128) * (attempts_b.max(1) as u128);
        let rate_rhs = (kills_b as u128) * (attempts_a.max(1) as u128);
        rate_rhs
            .cmp(&rate_lhs)
            .then(attempts_b.cmp(&attempts_a))
            .then(a.id.cmp(&b.id))
    });
    tests
}

fn update_mutation_kill_history_from_mutants(
    history: &mut MutationKillHistoryArtifact,
    mutants: &[MutationMutantResult],
) {
    history.schema_version = MUTATION_KILL_HISTORY_SCHEMA_VERSION;
    let seen_at = now_unix_ms();
    for mutant in mutants {
        if mutant.status != "killed" && mutant.status != "survived" {
            continue;
        }
        let killer = (mutant.status == "killed")
            .then(|| mutant.tests_ran.last().cloned())
            .flatten();
        for test_id in &mutant.tests_ran {
            let key = mutation_history_key(&mutant.function_id, &mutant.mutation_type, test_id);
            let entry = history
                .entries
                .entry(key)
                .or_insert(MutationKillHistoryEntry {
                    kills: 0,
                    attempts: 0,
                    last_seen_unix_ms: seen_at,
                });
            entry.attempts = entry.attempts.saturating_add(1);
            if killer.as_deref() == Some(test_id.as_str()) {
                entry.kills = entry.kills.saturating_add(1);
            }
            entry.last_seen_unix_ms = seen_at;
        }
    }
}

fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|err| format!("failed to serialize {}: {}", path.display(), err))?;
    write_bytes_atomic(path, &bytes)
}

fn write_bytes_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let Some(parent) = path.parent() else {
        return Err(format!(
            "atomic write target has no parent: {}",
            path.display()
        ));
    };
    fs::create_dir_all(parent)
        .map_err(|err| format!("failed to create {}: {}", parent.display(), err))?;
    let tmp_path = parent.join(format!(".tmp-{}-{}", std::process::id(), now_unix_ms()));
    fs::write(&tmp_path, bytes).map_err(|err| {
        format!(
            "failed to write temporary file {}: {}",
            tmp_path.display(),
            err
        )
    })?;
    fs::rename(&tmp_path, path).map_err(|err| {
        format!(
            "failed to atomically rename {} -> {}: {}",
            tmp_path.display(),
            path.display(),
            err
        )
    })?;
    Ok(())
}

fn mutation_dispatch_entry_source(
    workspace_root: &Path,
    mutation_key: &str,
    tests: &[TestCase],
) -> Result<(String, PathBuf), String> {
    let tests_root = workspace_root.join("tests");
    if !tests_root.is_dir() {
        return Err("mutation harness generation requires tests/ directory".to_string());
    }
    let wrappers_root = tests_root.join("wrela_mutation").join(mutation_key);
    let wrappers_dir = wrappers_root.join("cases");
    fs::create_dir_all(&wrappers_dir).map_err(|err| {
        format!(
            "failed to create mutation wrapper directory {}: {err}",
            wrappers_dir.display()
        )
    })?;
    let mut source = String::new();
    let mut dispatch_arms = Vec::with_capacity(tests.len());
    for (idx, test) in tests.iter().enumerate() {
        let wrapper_func = format!("run_case_{idx}");
        let wrapper_module = format!("tests/wrela_mutation/{mutation_key}/cases/case_{idx}");
        let wrapper_source = format!(
            "use {func} from {module}\n\nto {wrapper_func}() -> Nothing:\n    {dispatch}\n",
            func = test.func_name,
            module = test.module_path,
            dispatch = test_case_dispatch_stmt(test)
        );
        let wrapper_path = wrappers_dir.join(format!("case_{idx}.wr"));
        fs::write(&wrapper_path, wrapper_source).map_err(|err| {
            format!(
                "failed to write mutation wrapper {}: {}",
                wrapper_path.display(),
                err
            )
        })?;
        source.push_str(&format!("use {wrapper_func} from {wrapper_module}\n"));
        dispatch_arms.push((test.id.clone(), wrapper_func));
    }
    source.push('\n');
    source.push_str("to run() -> Integer:\n");
    source.push_str("    selected_value = __wr_env_get(\"WRELA_TEST_ID\")\n");
    source.push_str("    mutable selected = \"\"\n");
    source.push_str("    match selected_value:\n");
    source.push_str("        String:\n");
    source.push_str("            selected = selected_value\n");
    source.push_str("        otherwise:\n");
    source.push_str("            selected = \"\"\n");
    for (id, dispatch_func) in &dispatch_arms {
        source.push_str(&format!("    if selected == \"{id}\":\n"));
        source.push_str(&format!("        {dispatch_func}()\n"));
        source.push_str("        return 0\n");
    }
    source.push_str("    return 4\n");
    Ok((source, wrappers_root))
}

fn apply_mir_mutation(
    module: &mut mir::ir::MirModule,
    candidate: &MirMutationCandidate,
) -> Result<(), String> {
    let mut matching_indices = module
        .functions
        .iter()
        .enumerate()
        .filter_map(|(index, func)| {
            (func.name.as_str() == candidate.function_name).then_some(index)
        })
        .collect::<Vec<_>>();
    if matching_indices.is_empty() {
        return Err(format!(
            "function '{}' not found while applying mutant",
            candidate.function_name
        ));
    }
    if matching_indices.len() > 1 {
        return Err(format!(
            "ambiguous mutation target '{}': {} MIR functions match by name",
            candidate.function_name,
            matching_indices.len()
        ));
    }
    let function_index = matching_indices.pop().unwrap_or(0);
    let function = &mut module.functions[function_index];
    match candidate.site {
        MutationSite::Branch { block_idx } => {
            let block = function
                .blocks
                .get_mut(block_idx)
                .ok_or_else(|| format!("invalid branch mutation block index {}", block_idx))?;
            let mir::ir::Terminator::Branch {
                then_target,
                else_target,
                ..
            } = &mut block.terminator
            else {
                return Err("branch mutation site no longer contains a branch".to_string());
            };
            std::mem::swap(then_target, else_target);
        }
        MutationSite::Comparison {
            block_idx,
            stmt_idx,
        } => {
            let stmt = mutation_assign_stmt(function, block_idx, stmt_idx)?;
            let mir::ir::Rvalue::Binary { op, .. } = stmt else {
                return Err("comparison mutation site no longer contains a binary op".to_string());
            };
            *op = invertible_comparison(*op)
                .ok_or_else(|| "comparison mutation site is not invertible".to_string())?;
        }
        MutationSite::IntegerLiteralUse {
            block_idx,
            stmt_idx,
        } => {
            let stmt = mutation_assign_stmt(function, block_idx, stmt_idx)?;
            let mir::ir::Rvalue::Use(mir::ir::Value::Const(hir::Literal::Integer(value))) = stmt
            else {
                return Err(
                    "integer mutation site no longer contains a constant literal".to_string(),
                );
            };
            *value = perturb_integer(*value);
        }
        MutationSite::IntegerLiteralBinaryLhs {
            block_idx,
            stmt_idx,
        } => {
            let stmt = mutation_assign_stmt(function, block_idx, stmt_idx)?;
            let mir::ir::Rvalue::Binary { lhs, .. } = stmt else {
                return Err("integer mutation lhs site no longer contains a binary op".to_string());
            };
            let mir::ir::Value::Const(hir::Literal::Integer(value)) = lhs else {
                return Err(
                    "integer mutation lhs site no longer contains an integer literal".to_string(),
                );
            };
            *value = perturb_integer(*value);
        }
        MutationSite::IntegerLiteralBinaryRhs {
            block_idx,
            stmt_idx,
        } => {
            let stmt = mutation_assign_stmt(function, block_idx, stmt_idx)?;
            let mir::ir::Rvalue::Binary { rhs, .. } = stmt else {
                return Err("integer mutation rhs site no longer contains a binary op".to_string());
            };
            let mir::ir::Value::Const(hir::Literal::Integer(value)) = rhs else {
                return Err(
                    "integer mutation rhs site no longer contains an integer literal".to_string(),
                );
            };
            *value = perturb_integer(*value);
        }
        MutationSite::ResultGuard {
            block_idx,
            stmt_idx,
        } => {
            let stmt = mutation_assign_stmt(function, block_idx, stmt_idx)?;
            if !matches!(stmt, mir::ir::Rvalue::ResultIsOk { .. }) {
                return Err("result-guard mutation site no longer contains ResultIsOk".to_string());
            }
            *stmt = mir::ir::Rvalue::Use(mir::ir::Value::Const(hir::Literal::Boolean(true)));
        }
    }
    Ok(())
}

fn mutation_assign_stmt(
    function: &mut mir::ir::MirFunction,
    block_idx: usize,
    stmt_idx: usize,
) -> Result<&mut mir::ir::Rvalue, String> {
    let block = function
        .blocks
        .get_mut(block_idx)
        .ok_or_else(|| format!("invalid mutation block index {}", block_idx))?;
    let stmt = block
        .stmts
        .get_mut(stmt_idx)
        .ok_or_else(|| format!("invalid mutation stmt index {}", stmt_idx))?;
    let mir::ir::Stmt::Assign { value, .. } = stmt else {
        return Err("mutation site no longer contains an assignment".to_string());
    };
    Ok(value)
}

fn invertible_comparison(op: hir::BinaryOp) -> Option<hir::BinaryOp> {
    match op {
        hir::BinaryOp::Eq => Some(hir::BinaryOp::Ne),
        hir::BinaryOp::Ne => Some(hir::BinaryOp::Eq),
        hir::BinaryOp::Lt => Some(hir::BinaryOp::Ge),
        hir::BinaryOp::Gt => Some(hir::BinaryOp::Le),
        hir::BinaryOp::Le => Some(hir::BinaryOp::Gt),
        hir::BinaryOp::Ge => Some(hir::BinaryOp::Lt),
        _ => None,
    }
}

fn perturb_integer(value: i64) -> i64 {
    if value >= 0 {
        value.saturating_add(1)
    } else {
        value.saturating_sub(1)
    }
}

fn run_with_timeout(
    exe: &Path,
    timeout: Duration,
    metrics_path: Option<&Path>,
    cwd: Option<&Path>,
    args: &[&str],
    extra_env: &[(&str, &str)],
) -> Result<(), String> {
    let exe_path = if exe.is_absolute() {
        exe.to_path_buf()
    } else {
        env::current_dir()
            .map_err(|err| format!("failed to read current directory: {err}"))?
            .join(exe)
    };
    let mut command = Command::new(exe_path);
    command.args(args);
    if let Some(path) = cwd {
        command.current_dir(path);
    }
    command.env_clear();
    for key in inherited_test_env_keys() {
        if let Some(value) = env::var_os(key) {
            command.env(key, value);
        }
    }
    if let Some(path) = metrics_path {
        command.env("WRELA_METRICS_PATH", path);
    }
    for (key, value) in extra_env {
        command.env(key, value);
    }
    let mut child = command.spawn().map_err(|e| format!("failed to run: {e}"))?;
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().map_err(|e| format!("wait failed: {e}"))? {
            if status.success() {
                return Ok(());
            }
            return Err(format!("exit code {}", status.code().unwrap_or(1)));
        }
        if start.elapsed() >= timeout {
            let _ = child.kill();
            return Err("timeout".to_string());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn compile_to_mir_with_root(
    entry_path: &Path,
    root_dir: &Path,
    tests_dir: Option<&Path>,
    output_format: OutputFormat,
) -> Result<mir::ir::MirModule, i32> {
    let (module, source, source_name) = match hir::project::load_project_with_roots(
        entry_path,
        root_dir,
        tests_dir.map(|p| p.to_path_buf()),
        true,
    ) {
        Ok(project) => {
            for warn in project.warnings {
                emit_diag(
                    output_format,
                    "warning",
                    warn.message,
                    warn.span,
                    warn.path.display().to_string(),
                    warn.source,
                );
            }
            (
                project.module,
                project.entry_source,
                entry_path.display().to_string(),
            )
        }
        Err(errors) => {
            for err in errors {
                emit_diag(
                    output_format,
                    "error",
                    err.message,
                    err.span,
                    err.path.display().to_string(),
                    err.source,
                );
            }
            return Err(EXIT_PARSE);
        }
    };
    let (type_errors, type_info) = hir::typeck::check_module_with_info(&module);
    if let Some(err) = type_errors.into_iter().next() {
        emit_diag(
            output_format,
            "error",
            err.to_string(),
            err.primary_span(),
            source_name.clone(),
            source.clone(),
        );
        return Err(EXIT_TYPE);
    }
    let naming_errors = hir::naming::check_module(&module, &type_info);
    if let Some(err) = naming_errors.into_iter().next() {
        emit_diag(
            output_format,
            "error",
            err.to_string(),
            err.primary_span(),
            source_name.clone(),
            source.clone(),
        );
        return Err(EXIT_TYPE);
    }
    let mir_module = mir::lower::lower_module_with_types(&module, Some(&type_info));
    let mut had_errors = false;
    for err in mir::validate::validate_module(&mir_module) {
        emit_diag(
            output_format,
            "error",
            err.message,
            SourceSpan::from((0usize, 0usize)),
            source_name.clone(),
            source.clone(),
        );
        had_errors = true;
    }
    if had_errors {
        Err(EXIT_CODEGEN)
    } else {
        Ok(mir_module)
    }
}

fn resolve_entry_path(path_arg: Option<&str>) -> Result<PathBuf, String> {
    let path = match path_arg {
        Some(path) => PathBuf::from(path),
        None => return Err("missing input path".to_string()),
    };
    if path.is_dir() {
        let entry = path.join("src").join("main.wr");
        if entry.exists() {
            return Ok(entry);
        }
        return Err(format!("no entry file found at {}", entry.display()));
    }
    if !path.exists() {
        return Err(format!("path not found: {}", path.display()));
    }
    Ok(path)
}

fn project_root_for_entry(entry_path: &Path) -> PathBuf {
    for ancestor in entry_path.ancestors() {
        if ancestor.file_name().is_some_and(|name| name == "src") {
            if let Some(parent) = ancestor.parent() {
                return parent.to_path_buf();
            }
        }
    }
    entry_path.parent().unwrap_or(entry_path).to_path_buf()
}

fn compile_to_mir(
    entry_path: &Path,
    output_format: OutputFormat,
    emit_mir: bool,
    emit_mir_opt: bool,
    require_entrypoint: bool,
) -> Result<mir::ir::MirModule, i32> {
    let trace = std::env::var("WRELA_BUILD_TRACE").is_ok();
    let stage = |name: &str, start: &Instant| {
        if trace {
            eprintln!("build: {} ({:.2?})", name, start.elapsed());
        }
    };
    let start = Instant::now();
    if trace {
        eprintln!("build: start {:?}", entry_path);
    }
    let (module, source, source_name) =
        match hir::project::load_project_with_entrypoint(entry_path, require_entrypoint) {
            Ok(project) => {
                for warn in project.warnings {
                    emit_diag(
                        output_format,
                        "warning",
                        warn.message,
                        warn.span,
                        warn.path.display().to_string(),
                        warn.source,
                    );
                }
                (
                    project.module,
                    project.entry_source,
                    entry_path.display().to_string(),
                )
            }
            Err(errors) => {
                let mut missing_run = false;
                for err in errors {
                    if err.message.contains("define 'to run()'") {
                        missing_run = true;
                    }
                    emit_diag(
                        output_format,
                        "error",
                        err.message,
                        err.span,
                        err.path.display().to_string(),
                        err.source,
                    );
                }
                if missing_run
                    && require_entrypoint
                    && matches!(output_format, OutputFormat::Pretty)
                {
                    eprintln!(
                        "note: add `to run()` in your entry file to define the program entrypoint"
                    );
                }
                return Err(EXIT_PARSE);
            }
        };
    stage("load_project", &start);

    let mut had_errors = false;
    let semantic = hir::semantic::check_module(&module);
    stage("semantic", &start);
    for err in semantic.errors {
        match output_format {
            OutputFormat::Pretty => {
                let report = Report::new(err)
                    .with_source_code(NamedSource::new(source_name.clone(), source.clone()));
                eprintln!("{report:?}");
            }
            OutputFormat::Json => {
                emit_json_diag_for_diagnostic(
                    "error",
                    &err,
                    err.primary_span(),
                    source_name.clone(),
                );
            }
        }
        had_errors = true;
    }
    for warn in semantic.warnings {
        match output_format {
            OutputFormat::Pretty => {
                let report = Report::new(warn)
                    .with_source_code(NamedSource::new(source_name.clone(), source.clone()));
                eprintln!("warning: {report:?}");
            }
            OutputFormat::Json => {
                emit_json_diag_for_diagnostic(
                    "warning",
                    &warn,
                    warn.primary_span(),
                    source_name.clone(),
                );
            }
        }
    }

    let (type_errors, type_info) = hir::typeck::check_module_with_info(&module);
    stage("typeck", &start);
    for err in type_errors {
        match output_format {
            OutputFormat::Pretty => {
                let report = Report::new(err)
                    .with_source_code(NamedSource::new(source_name.clone(), source.clone()));
                eprintln!("{report:?}");
            }
            OutputFormat::Json => {
                emit_json_diag_for_diagnostic(
                    "error",
                    &err,
                    err.primary_span(),
                    source_name.clone(),
                );
            }
        }
        had_errors = true;
    }

    let naming_errors = hir::naming::check_module(&module, &type_info);
    stage("naming", &start);
    for err in naming_errors {
        match output_format {
            OutputFormat::Pretty => {
                let report = Report::new(err)
                    .with_source_code(NamedSource::new(source_name.clone(), source.clone()));
                eprintln!("{report:?}");
            }
            OutputFormat::Json => {
                emit_json_diag(
                    "error",
                    err.to_string(),
                    err.primary_span(),
                    source_name.clone(),
                );
            }
        }
        had_errors = true;
    }

    if had_errors {
        return Err(EXIT_TYPE);
    }

    let check_ir = hir::checkir::extract_module(&module);
    if std::env::var("WRELA_CHECK_ORACLE_TRACE").is_ok() {
        let vector_eligible = check_ir
            .checks
            .iter()
            .filter(|check| check.supports_vector_lane)
            .count();
        eprintln!(
            "check-oracle: extracted={} skipped={} vector_eligible={}",
            check_ir.checks.len(),
            check_ir.skipped.len(),
            vector_eligible
        );
        for check in &check_ir.checks {
            eprintln!(
                "check-oracle-shape: name={} shape_id={} vector_lane={}",
                check.name, check.shape_id, check.supports_vector_lane
            );
        }
    }

    let mut mir_module = mir::lower::lower_module_with_types(&module, Some(&type_info));
    stage("mir_lower", &start);
    if emit_mir {
        println!("{:#?}", mir_module);
    }
    let analysis = mir::analysis::analyze_module(&mir_module);
    for func in &mut mir_module.functions {
        let types = analysis.type_map.function(&func.name);
        mir::opt::run_function_passes_with_types(func, types);
    }
    let rewrite_report =
        mir::opt::run_module_passes_with_rulepack(&mut mir_module, Some(&check_ir));
    if std::env::var("WRELA_CHECK_ORACLE_TRACE").is_ok() {
        eprintln!(
            "rewrite: mined={} admitted={} applied={} steps={} exhausted={}",
            rewrite_report.mined,
            rewrite_report.admitted,
            rewrite_report.applied,
            rewrite_report.steps,
            rewrite_report.budget_exhausted
        );
    }
    stage("mir_opt", &start);
    if emit_mir_opt {
        println!("{:#?}", mir_module);
    }
    for err in mir::validate::validate_module(&mir_module) {
        eprintln!("mir validation error: {}", err.message);
        had_errors = true;
    }

    if had_errors {
        return Err(EXIT_CODEGEN);
    }

    Ok(mir_module)
}

fn temp_exe_path() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_nanos();
    let name = format!("wrela_run_{}_{}", std::process::id(), nanos);
    env::temp_dir().join(name).to_string_lossy().to_string()
}

fn run_dev_loop(
    entry_path: &Path,
    poll_ms: u64,
    output_format: OutputFormat,
    emit_mir: bool,
    emit_mir_opt: bool,
    program_args: &[String],
) {
    let src_root = find_src_root(entry_path)
        .unwrap_or_else(|| entry_path.parent().unwrap_or(entry_path).to_path_buf());
    eprintln!("dev: watching {} (poll {}ms)", src_root.display(), poll_ms);
    let mut last = snapshot_sources(&src_root);
    let mut child: Option<std::process::Child> = None;
    loop {
        if sources_changed(&src_root, &mut last) {
            if let Some(mut running) = child.take() {
                let _ = running.kill();
                let _ = running.wait();
            }
            let mir_module =
                match compile_to_mir(entry_path, output_format, emit_mir, emit_mir_opt, true) {
                    Ok(mir) => mir,
                    Err(code) => {
                        if code != EXIT_USAGE {
                            eprintln!("dev: build failed (exit {code})");
                        }
                        sleep_ms(poll_ms);
                        continue;
                    }
                };
            let output = temp_exe_path();
            if let Err(err) =
                wrela::backend::cranelift::compile_to_executable(&mir_module, output.as_ref())
            {
                eprintln!("codegen error: {}", err.0);
                sleep_ms(poll_ms);
                continue;
            }
            match Command::new(&output).args(program_args).spawn() {
                Ok(proc) => {
                    child = Some(proc);
                }
                Err(err) => {
                    eprintln!("dev: run failed: {err}");
                }
            }
        }
        sleep_ms(poll_ms);
    }
}

fn find_src_root(entry_path: &Path) -> Option<PathBuf> {
    for ancestor in entry_path.ancestors() {
        if ancestor.file_name().map(|n| n == "src").unwrap_or(false) {
            return Some(ancestor.to_path_buf());
        }
    }
    None
}

fn snapshot_sources(root: &Path) -> Vec<(PathBuf, SystemTime)> {
    let mut out = Vec::new();
    collect_sources(root, &mut out);
    out
}

fn sources_changed(root: &Path, last: &mut Vec<(PathBuf, SystemTime)>) -> bool {
    let mut current = Vec::new();
    collect_sources(root, &mut current);
    if current.len() != last.len() {
        *last = current;
        return true;
    }
    current.sort_by(|a, b| a.0.cmp(&b.0));
    last.sort_by(|a, b| a.0.cmp(&b.0));
    for (a, b) in current.iter().zip(last.iter()) {
        if a.0 != b.0 || a.1 != b.1 {
            *last = current;
            return true;
        }
    }
    false
}

fn collect_sources(root: &Path, out: &mut Vec<(PathBuf, SystemTime)>) {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_sources(&path, out);
        } else if is_source_file(&path) {
            if let Ok(meta) = entry.metadata() {
                if let Ok(modified) = meta.modified() {
                    out.push((path, modified));
                }
            }
        }
    }
}

fn is_source_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|s| s.to_str()),
        Some("wr") | Some("sp")
    )
}

fn sleep_ms(ms: u64) {
    std::thread::sleep(Duration::from_millis(ms));
}

fn update_toolchain(prefix_override: Option<&str>) -> Result<(), String> {
    let prefix = prefix_override
        .map(PathBuf::from)
        .or_else(|| env::var("PREFIX").ok().map(PathBuf::from))
        .unwrap_or_else(|| {
            env::var("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(".local")
                .join("wrela")
        });

    let target = resolve_target_triple()?;
    let tag = env::var("WRELA_TAG").ok().filter(|s| !s.is_empty());
    let tag = match tag {
        Some(tag) => tag,
        None => fetch_latest_tag()?,
    };
    let url =
        format!("https://github.com/rywible/wrela/releases/download/{tag}/wrela-{target}.tar.gz");

    fs::create_dir_all(&prefix).map_err(|err| format!("create prefix failed: {err}"))?;
    let tmp_path = env::temp_dir().join(format!(
        "wrela_update_{}_{}.tar.gz",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::from_secs(0))
            .as_nanos()
    ));

    let mut curl = Command::new("curl");
    curl.args(["-fsSL", "-o"]).arg(&tmp_path).arg(&url);
    let curl_out = curl.output().map_err(|err| {
        if err.kind() == io::ErrorKind::NotFound {
            "curl not found (install curl to use `wrela update`)".to_string()
        } else {
            format!("failed to run curl: {err}")
        }
    })?;
    if !curl_out.status.success() {
        let stderr = String::from_utf8_lossy(&curl_out.stderr);
        return Err(format!("download failed: {}", stderr.trim()));
    }

    let mut tar = Command::new("tar");
    tar.args(["-xzf"]).arg(&tmp_path).arg("-C").arg(&prefix);
    let tar_out = tar.output().map_err(|err| {
        if err.kind() == io::ErrorKind::NotFound {
            "tar not found (install tar to use `wrela update`)".to_string()
        } else {
            format!("failed to run tar: {err}")
        }
    })?;
    if !tar_out.status.success() {
        let stderr = String::from_utf8_lossy(&tar_out.stderr);
        return Err(format!("extract failed: {}", stderr.trim()));
    }

    let _ = fs::remove_file(&tmp_path);
    println!("Updated Wrela at: {}", prefix.display());
    Ok(())
}

fn resolve_target_triple() -> Result<&'static str, String> {
    let os = env::consts::OS;
    let arch = env::consts::ARCH;
    match (os, arch) {
        ("macos", "aarch64") => Ok("aarch64-apple-darwin"),
        ("macos", "x86_64") => Ok("x86_64-apple-darwin"),
        ("linux", "aarch64") => Ok("aarch64-unknown-linux-gnu"),
        ("linux", "x86_64") => Ok("x86_64-unknown-linux-gnu"),
        _ => Err(format!("unsupported platform: {os}/{arch}")),
    }
}

fn fetch_latest_tag() -> Result<String, String> {
    let mut curl = Command::new("curl");
    curl.args([
        "-fsSL",
        "https://api.github.com/repos/rywible/wrela/releases",
    ]);
    let output = curl.output().map_err(|err| {
        if err.kind() == io::ErrorKind::NotFound {
            "curl not found (install curl to use `wrela update`)".to_string()
        } else {
            format!("failed to run curl: {err}")
        }
    })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("failed to fetch releases: {}", stderr.trim()));
    }
    let body = String::from_utf8_lossy(&output.stdout);
    parse_first_tag(&body).ok_or_else(|| {
        "failed to resolve a release tag (set WRELA_TAG to a specific release)".to_string()
    })
}

fn parse_first_tag(body: &str) -> Option<String> {
    let key = "\"tag_name\"";
    let pos = body.find(key)?;
    let after = &body[pos + key.len()..];
    let quote_start = after.find('"')?;
    let after_quote = &after[quote_start + 1..];
    let quote_end = after_quote.find('"')?;
    let tag = &after_quote[..quote_end];
    if tag.is_empty() || tag == "null" {
        None
    } else {
        Some(tag.to_string())
    }
}

const EXIT_USAGE: i32 = 1;
const EXIT_PARSE: i32 = 2;
const EXIT_TYPE: i32 = 3;
const EXIT_OK: i32 = 0;
const EXIT_CODEGEN: i32 = 4;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn perf_summary_with_kpis() -> PerfSummary {
        PerfSummary {
            sample_count: 1,
            compile_throughput_tests_per_sec: 100.0,
            runtime_p50_ns: 100,
            runtime_p95_ns: 150,
            runtime_p99_ns: 200,
            allocs_per_request: 1.0,
            rc_inc: 0,
            rc_dec: 0,
            rc_ops_total: 0,
            dispatch_hit_ratio: 1.0,
            check_fallback_rate: Some(0.10),
            avg_check_batch_size: Some(8.0),
            check_oracle_eval_ns_p50: Some(50),
            check_oracle_eval_ns_p95: Some(90),
            effect_annihilation_rewrite_count: Some(2),
            scheduler_dispatch_p99_ns: Some(800),
            scheduler_starvation_violations: Some(0),
            rewrite_compile_overhead_pct: Some(4.0),
            rewrite_applied_count: Some(10),
            actor_msgs_per_sec_p50: Some(1000.0),
            actor_msgs_per_sec_p95: Some(900.0),
            queue_enqueue_p99_ns: Some(100),
            queue_dequeue_p99_ns: Some(120),
            queue_age_p99_ns: Some(150),
            mailbox_wake_coalesced_count: Some(2),
            mailbox_rescue_wake_count: Some(0),
            queue_cas_retry_total: Some(1),
            cases: None,
            metrics: MetricsTotals::default(),
        }
    }

    #[test]
    fn evaluate_perf_gate_applies_kpi_thresholds() {
        let baseline = perf_summary_with_kpis();
        let mut current = perf_summary_with_kpis();
        current.check_fallback_rate = Some(0.25);
        current.avg_check_batch_size = Some(4.0);
        current.scheduler_dispatch_p99_ns = Some(950);
        current.rewrite_compile_overhead_pct = Some(7.5);
        current.actor_msgs_per_sec_p50 = Some(900.0);
        current.queue_age_p99_ns = Some(220);
        current.scheduler_starvation_violations = Some(2);
        let thresholds = KpiThresholds {
            check_fallback_max: Some(0.20),
            check_batch_min: Some(6.0),
            scheduler_p99_improve_min_pct: Some(10.0),
            rewrite_overhead_max_pct: Some(5.0),
            actor_throughput_improve_min_pct: Some(0.0),
            queue_age_p99_max_regress_pct: Some(10.0),
            starvation_violations_max: Some(0.0),
            scheduler_throughput_improve_min_pct: Some(0.0),
            scheduler_loop_p99_max_regress_pct: Some(20.0),
            scheduler_local_hit_min: Some(0.0),
        };

        let failures = evaluate_perf_gate(&current, &baseline, 5.0, &thresholds);

        assert!(
            failures
                .iter()
                .any(|line| line.contains("check_fallback_rate"))
        );
        assert!(
            failures
                .iter()
                .any(|line| line.contains("avg_check_batch_size"))
        );
        assert!(
            failures
                .iter()
                .any(|line| line.contains("scheduler_dispatch_p99_ns improvement"))
        );
        assert!(
            failures
                .iter()
                .any(|line| line.contains("rewrite_compile_overhead_pct"))
        );
        assert!(
            failures
                .iter()
                .any(|line| line.contains("actor_msgs_per_sec_p50 improvement"))
        );
        assert!(
            failures
                .iter()
                .any(|line| line.contains("queue_age_p99_ns regression"))
        );
        assert!(
            failures
                .iter()
                .any(|line| line.contains("scheduler_starvation_violations"))
        );
    }

    #[test]
    fn evaluate_perf_gate_ignores_missing_optional_kpis() {
        let mut baseline = perf_summary_with_kpis();
        let mut current = perf_summary_with_kpis();
        baseline.scheduler_dispatch_p99_ns = None;
        current.scheduler_dispatch_p99_ns = None;
        current.check_fallback_rate = None;
        current.avg_check_batch_size = None;
        current.rewrite_compile_overhead_pct = None;
        let thresholds = KpiThresholds {
            check_fallback_max: Some(0.20),
            check_batch_min: Some(6.0),
            scheduler_p99_improve_min_pct: Some(10.0),
            rewrite_overhead_max_pct: Some(5.0),
            actor_throughput_improve_min_pct: None,
            queue_age_p99_max_regress_pct: None,
            starvation_violations_max: None,
            scheduler_throughput_improve_min_pct: None,
            scheduler_loop_p99_max_regress_pct: None,
            scheduler_local_hit_min: None,
        };

        let failures = evaluate_perf_gate(&current, &baseline, 5.0, &thresholds);

        assert!(failures.is_empty());
    }

    #[test]
    fn sim_seed_expansion_uses_256_seeds_in_cert_mode() {
        let base = TestCase {
            id: "sim-id".to_string(),
            lane: TestLane::Sim,
            name: "tests/sim/foo::test_bar".to_string(),
            module_path: "tests/sim/foo".to_string(),
            func_name: "test_bar".to_string(),
            is_serial: false,
            allows_env_set: false,
            allows_fs_escape: false,
            has_oracle: true,
            generated_call_body: None,
            generated_case_kind: None,
            generated_entry_source: None,
            autogen_module_source: None,
            autogen_seed: None,
            autogen_span: None,
            sim_seed: None,
            canonical_id: "sim-id".to_string(),
        };
        let expanded = expand_sim_seed_cases(vec![base], None, true);
        assert_eq!(expanded.len(), 256);
        assert_eq!(expanded.first().and_then(|t| t.sim_seed), Some(0));
        assert_eq!(expanded.last().and_then(|t| t.sim_seed), Some(255));
    }

    #[test]
    fn model_seed_expansion_uses_multiple_seeds_in_cert_mode() {
        let base = TestCase {
            id: "model-id".to_string(),
            lane: TestLane::Model,
            name: "tests/model/foo::test_bar".to_string(),
            module_path: "tests/model/foo".to_string(),
            func_name: "test_bar".to_string(),
            is_serial: false,
            allows_env_set: false,
            allows_fs_escape: false,
            has_oracle: true,
            generated_call_body: None,
            generated_case_kind: None,
            generated_entry_source: None,
            autogen_module_source: None,
            autogen_seed: None,
            autogen_span: None,
            sim_seed: None,
            canonical_id: "model-id".to_string(),
        };
        let expanded = expand_sim_seed_cases(vec![base], None, true);
        assert_eq!(expanded.len(), 64);
        assert_eq!(expanded.first().and_then(|t| t.sim_seed), Some(0));
        assert_eq!(expanded.last().and_then(|t| t.sim_seed), Some(63));
    }

    #[test]
    fn perfcmp_profile_parse_accepts_known_values() {
        assert_eq!(PerfProfile::parse("smoke"), Some(PerfProfile::Smoke));
        assert_eq!(PerfProfile::parse("standard"), Some(PerfProfile::Standard));
        assert_eq!(PerfProfile::parse("deep"), Some(PerfProfile::Deep));
        assert_eq!(PerfProfile::parse("invalid"), None);
    }

    #[test]
    fn profile_pair_counts_obey_overrides() {
        let manifest = BenchmarkManifest {
            version: 1,
            suite: "micro".to_string(),
            optional: false,
            profiles: BenchmarkProfiles {
                smoke: Some(BenchmarkProfileConfig {
                    warmup_pairs: 4,
                    measure_pairs: 9,
                    coverage: "all".to_string(),
                }),
                standard: None,
                deep: None,
            },
            scenarios: vec![BenchmarkScenario {
                id: "s".to_string(),
                test_name: "tests/default/micro::test_x_ops_1".to_string(),
                ops: 1,
                class: "critical".to_string(),
                min_runtime_ms: None,
                timeout_ms: None,
                allow_unstable: false,
            }],
        };

        assert_eq!(
            profile_pair_counts(&manifest, PerfProfile::Smoke, None, None),
            (4, 9)
        );
        assert_eq!(
            profile_pair_counts(&manifest, PerfProfile::Smoke, Some(2), Some(3)),
            (2, 3)
        );
    }

    #[test]
    fn classify_perfcmp_verdict_respects_effect_threshold() {
        assert_eq!(classify_perfcmp_verdict(2.1, 3.0, 2.0), "win");
        assert_eq!(classify_perfcmp_verdict(-4.0, -2.1, 2.0), "regression");
        assert_eq!(classify_perfcmp_verdict(-1.0, 1.0, 2.0), "no_signal");
    }

    #[test]
    fn bootstrap_ci_is_seeded_reproducible() {
        let values = vec![-1.0, 0.0, 1.0, 2.0, 3.0];
        let mut seed_a = 12345u64;
        let mut seed_b = 12345u64;
        let a = bootstrap_ci_percentile(&values, 95.0, 1000, &mut seed_a);
        let b = bootstrap_ci_percentile(&values, 95.0, 1000, &mut seed_b);
        assert_eq!(a, b);
    }

    #[test]
    fn optional_linux_suite_skip_rule_only_skips_non_linux() {
        assert!(should_skip_optional_suite(true, "linux", "macos"));
        assert!(!should_skip_optional_suite(true, "linux", "linux"));
        assert!(!should_skip_optional_suite(true, "micro", "macos"));
        assert!(!should_skip_optional_suite(false, "linux", "macos"));
    }

    #[test]
    fn manifest_rejects_mismatched_ops_suffix() {
        let path = env::temp_dir().join(format!("wrela-bench-{}.toml", now_unix_ms()));
        let mut file = fs::File::create(&path).expect("create temp manifest");
        writeln!(
            file,
            "version = 1\nsuite = \"micro\"\n\n[[scenarios]]\nid = \"a\"\ntest_name = \"tests/default/micro::test_demo_ops_10\"\nops = 20\nclass = \"critical\"\nallow_unstable = false\n"
        )
        .expect("write manifest");
        let err = load_benchmark_manifest(&path).expect_err("expected ops suffix mismatch");
        assert!(err.contains("must end with"));
        let _ = fs::remove_file(path);
    }
}
