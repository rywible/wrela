use super::command_handlers::{
    self, BudgetPolicyV1, CertPerfTimings, DifferentialPipeline, HttpCassetteMode, KpiThresholds,
    PerfGateConfig, TestExecution, TestSelection, TestTarget, budget_jobs_timeout,
    resolve_budget_policy_v1, resolve_test_target,
};
use super::contracts::{EXIT_CODEGEN, EXIT_OK, EXIT_USAGE, OutputFormat};
use super::diag_emit;
use super::replay_trace;
use super::repro_bridge::{ReproCommandInput, run_repro_command};
use miette::SourceSpan;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use wrela::diag::{DiagRecord, DiagSeverity, DiagStage};

pub(super) struct TestCommandInput {
    pub(super) trace: bool,
    pub(super) program_args: Vec<String>,
    pub(super) out_path: Option<String>,
    pub(super) emit_obj: Option<String>,
    pub(super) emit_bin: Option<String>,
    pub(super) path_arg: Option<String>,
    pub(super) test_jobs: Option<usize>,
    pub(super) test_timeout_ms: Option<u64>,
    pub(super) test_record: bool,
    pub(super) test_update_public_surface: bool,
    pub(super) test_selection: TestSelection,
    pub(super) repro_artifact_path: Option<String>,
    pub(super) replay_trace_path: Option<String>,
    pub(super) output_format: OutputFormat,
    pub(super) perf_debug: bool,
    pub(super) perf_gate_path: Option<String>,
    pub(super) perf_max_regression_pct: Option<f64>,
    pub(super) kpi_thresholds: KpiThresholds,
    pub(super) test_seed: Option<u64>,
    pub(super) query_backend: wrela::query_plan::DispatchBackend,
}

pub(super) fn execute_test_command(input: TestCommandInput) -> i32 {
    if input.trace {
        eprintln!("build: command test");
    }
    if !input.program_args.is_empty() {
        eprintln!("error: unexpected extra arguments");
        return EXIT_USAGE;
    }
    if input.out_path.is_some() || input.emit_obj.is_some() || input.emit_bin.is_some() {
        eprintln!("error: -o/--out, --emit-obj, and --emit-bin are not valid with `wrela test`");
        return EXIT_USAGE;
    }
    if input.replay_trace_path.is_some()
        && (input.test_record
            || input.test_update_public_surface
            || command_handlers::test_selection_has_filters(&input.test_selection)
            || input.repro_artifact_path.is_some()
            || input.test_seed.is_some())
    {
        eprintln!(
            "error: --replay-trace cannot be combined with --record, --update-public-surface, --list, --id, --filter, --repro, or --seed"
        );
        return EXIT_USAGE;
    }

    let effective_test_jobs = if input.test_jobs.is_some() {
        input.test_jobs
    } else if std::env::var("WRELA_BUDGET_TEST_JOBS").is_ok() {
        None
    } else {
        default_test_jobs_for_dev_loop()
    };
    let budget_policy = resolve_budget_policy_v1(effective_test_jobs, input.test_timeout_ms);
    let (jobs, timeout) = budget_jobs_timeout(&budget_policy);
    let target = match resolve_test_target(input.path_arg.as_deref()) {
        Ok(target) => target,
        Err(err) => {
            eprintln!("error: {err}");
            return EXIT_USAGE;
        }
    };
    if let TestTarget::SingleFile(path) = &target {
        eprintln!(
            "error: `wrela test` requires a project-root directory; single-file .wr targets are not supported: {}",
            path.display()
        );
        eprintln!(
            "help: run `wrela test <project-root>` using project layout (`src/**`, `tests/**`)"
        );
        return EXIT_USAGE;
    }

    if input.repro_artifact_path.is_some()
        && (input.test_record
            || input.test_update_public_surface
            || command_handlers::test_selection_has_filters(&input.test_selection))
    {
        eprintln!(
            "error: --repro cannot be combined with --record, --update-public-surface, --list, --id, or --filter"
        );
        return EXIT_USAGE;
    }

    if let Some(repro_path) = input.repro_artifact_path {
        return run_repro_command(ReproCommandInput {
            path_arg: input.path_arg,
            repro_artifact_path: repro_path,
            test_record: input.test_record,
            test_jobs: input.test_jobs,
            test_timeout_ms: input.test_timeout_ms,
            output_format: input.output_format,
        });
    }

    if let Some(trace_path) = input.replay_trace_path {
        let path = PathBuf::from(trace_path);
        match replay_trace::replay_signature_from_artifact_typed(&path) {
            Ok(signature) => {
                emit_replay_trace_success(input.output_format, &path, &signature);
                return EXIT_OK;
            }
            Err(err) => {
                emit_replay_trace_error(input.output_format, &path, err);
                return EXIT_CODEGEN;
            }
        }
    }

    if input.test_record {
        eprintln!(
            "maintenance mode: --record updates integration cassettes; no build artifact is emitted"
        );
    }
    if input.test_update_public_surface {
        eprintln!(
            "maintenance mode: --update-public-surface updates snapshot baselines; no build artifact is emitted"
        );
    }

    let gate_cfg = input.perf_gate_path.as_ref().map(|path| PerfGateConfig {
        baseline_path: PathBuf::from(path),
        max_regression_pct: input.perf_max_regression_pct.unwrap_or(5.0),
        kpi_thresholds: input.kpi_thresholds,
    });
    let result = run_tests(
        &target,
        &budget_policy,
        jobs,
        timeout,
        input.output_format,
        input.perf_debug,
        gate_cfg.as_ref(),
        &input.test_selection,
        false,
        if input.test_record {
            HttpCassetteMode::Record
        } else {
            HttpCassetteMode::Replay
        },
        input.test_seed,
        input.query_backend,
    );
    let exit = result.exit;
    if input.test_record || input.test_update_public_surface {
        let workspace_root = match &target {
            TestTarget::ProjectRoot(root) => root.clone(),
            TestTarget::SingleFile(path) => path
                .parent()
                .unwrap_or_else(|| std::path::Path::new("."))
                .to_path_buf(),
        };
        if input.test_update_public_surface
            && exit == EXIT_OK
            && let Err(err) = command_handlers::update_public_surface_baseline(&workspace_root)
        {
            eprintln!("public surface update error: {err}");
            return EXIT_CODEGEN;
        }
        if let Err(err) = command_handlers::write_test_maintenance_summary(
            &workspace_root,
            input.test_record,
            input.test_update_public_surface,
            exit,
        ) {
            eprintln!("maintenance summary error: {err}");
            return EXIT_CODEGEN;
        }
    }
    exit
}

fn default_test_jobs_for_dev_loop() -> Option<usize> {
    std::thread::available_parallelism()
        .ok()
        .map(|parallelism| parallelism.get().min(8))
}

pub(super) fn run_tests(
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
    query_backend: wrela::query_plan::DispatchBackend,
) -> TestExecution {
    let mut cert_timings = CertPerfTimings::default();
    let mut harness_cache = std::collections::HashMap::new();
    let mut first_run_timings = command_handlers::RunOnceTimings::default();
    let (exit, summary, signature) = command_handlers::run_tests_once(
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
        query_backend,
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
        let mut alt_timings = command_handlers::RunOnceTimings::default();
        let diff_start = Instant::now();
        let (alt_exit, _, alt_signature) = command_handlers::run_tests_once(
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
            query_backend,
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
        differential_results_hash = Some(command_handlers::fnv1a64_hex(
            format!("{}:{}", first_signature.hash, alt_signature.hash).as_bytes(),
        ));
        if alt_exit != exit || first_signature.hash != alt_signature.hash {
            eprintln!("differential gate failed: baseline and alt pipelines diverged");
            eprintln!("  baseline exit: {exit}");
            eprintln!("  alt exit: {alt_exit}");
            eprintln!("  baseline signature: {}", first_signature.hash);
            eprintln!("  alt signature: {}", alt_signature.hash);
            if let Some(detail) = command_handlers::first_signature_mismatch_detail(
                &first_signature.outcomes,
                &alt_signature.outcomes,
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
        let mut replay_timings = command_handlers::RunOnceTimings::default();
        let (repeat_exit, _, repeat_signature) = command_handlers::run_tests_once(
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
            query_backend,
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
                command_handlers::TEST_JSON_SUMMARY_SEED
            );
            eprintln!("  first run exit: {exit}");
            eprintln!("  replay exit: {repeat_exit}");
            eprintln!("  first signature: {}", first_signature.hash);
            eprintln!("  replay signature: {}", second_signature.hash);
            if let Some(detail) = command_handlers::first_signature_mismatch_detail(
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
        let baseline = match command_handlers::load_perf_baseline_summary(&gate.baseline_path) {
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
        let failures = command_handlers::evaluate_perf_gate(
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
        && let Err(err) = command_handlers::evaluate_connector_contract_gate(root)
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
        match command_handlers::run_mutation_gate(
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

fn emit_replay_trace_success(format: OutputFormat, path: &std::path::Path, signature: &str) {
    match format {
        OutputFormat::Pretty => {
            println!("replay trace verified");
            println!("signature: {signature}");
        }
        OutputFormat::Json => {
            let payload = serde_json::json!({
                "kind": "info",
                "code": "lang::runtime::replay_verified",
                "message": "replay trace verified",
                "path": path.display().to_string(),
                "signature": signature,
            });
            println!(
                "{}",
                serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string())
            );
        }
        OutputFormat::Sarif => {}
    }
}

fn emit_replay_trace_error(
    format: OutputFormat,
    path: &std::path::Path,
    err: replay_trace::ReplayTraceValidationError,
) {
    let message = format!("replay trace error: {}", err.message);
    let mismatch_data = err.data();
    let record = DiagRecord::new(
        DiagStage::Project,
        DiagSeverity::Error,
        message.clone(),
        path.display().to_string(),
        SourceSpan::from((0, 0usize)),
    )
    .with_code(Some(err.code.clone()))
    .with_help(Some(
        "replay trace must exactly match deterministic event ordering, route, seed, and outcome contracts"
            .to_string(),
    ))
    .with_data(Some(serde_json::json!({
        "kind": "replay_trace_validation",
        "mismatch_kind": err.kind,
        "mismatch_code": err.code,
        "mismatch_message": err.message,
        "mismatch": mismatch_data,
        "path": path.display().to_string(),
    })));
    if matches!(format, OutputFormat::Pretty) {
        eprintln!("{message}");
    }
    diag_emit::emit_diag_record(format, &record, "");
    if matches!(format, OutputFormat::Pretty) {
        eprintln!("replay trace mismatch category: {:?}", err.kind);
    }
}
