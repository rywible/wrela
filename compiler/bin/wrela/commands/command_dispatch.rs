//! Owns typed CLI command dispatch and the last-mile conversion from parse-time
//! command models into command-handler inputs.
//! Does not own token parsing or the domain logic implemented by sibling command
//! modules.
//!
//! Key invariants:
//! - dispatch matches on typed command variants, never free-form command names.
//! - conversions here may normalize CLI-facing enums/newtypes, but they must not
//!   silently broaden command legality after parsing.
//! - handler inputs must preserve the parsed scenario/lane identity needed by
//!   perf and closure reporting.
//!
//! Primary entrypoints:
//! - `execute`
//!
//! Failure modes / common pitfalls:
//! - reintroducing string comparisons here duplicates parser logic and weakens
//!   the typed command model.
//! - dropping typed identity during conversion forces downstream tooling back
//!   onto ad hoc string protocols.

use super::collision_command::*;
use super::contracts_command::*;
use super::live_command::{execute_live_command, execute_perf_latency_command};
use super::presentation_command::*;
use super::preview_eval::{
    authored_presentation_lighting_inputs, bind_presentation_function_params,
    prepare_presentation_execution,
};
use super::*;
use ciborium::value::Value;
use wrela::persistence::{
    PersistenceProject, PersistentHandle, SnapshotLedgerRecord, load_snapshot_record, read_record,
};
use wrela::query_exec::ids::stable_semantic_id;

fn into_test_lane(lane: ParsedTestLane) -> TestLane {
    match lane {
        ParsedTestLane::Spec => TestLane::Spec,
        ParsedTestLane::Integration => TestLane::Integration,
        ParsedTestLane::Sim => TestLane::Sim,
        ParsedTestLane::Model => TestLane::Model,
        ParsedTestLane::Default => TestLane::Default,
    }
}

fn into_test_lane_selection(selection: ParsedTestLaneSelection) -> TestLaneSelection {
    match selection {
        ParsedTestLaneSelection::Single(lane) => TestLaneSelection::Single(into_test_lane(lane)),
        ParsedTestLaneSelection::Preset(ParsedTestLanePreset::Fast) => {
            TestLaneSelection::Preset(TestLanePreset::Fast)
        }
        ParsedTestLaneSelection::Preset(ParsedTestLanePreset::Full) => {
            TestLaneSelection::Preset(TestLanePreset::Full)
        }
    }
}

fn into_test_selection(selection: ParsedTestSelection) -> TestSelection {
    TestSelection {
        list: selection.list,
        id: selection.id,
        filter: selection.filter,
        lane: selection.lane.map(into_test_lane_selection),
        include_ids: None,
        cert_selection_report: None,
    }
}

fn into_perf_profile(profile: ParsedPerfProfile) -> PerfProfile {
    match profile {
        ParsedPerfProfile::Smoke => PerfProfile::Smoke,
        ParsedPerfProfile::Standard => PerfProfile::Standard,
        ParsedPerfProfile::Deep => PerfProfile::Deep,
        ParsedPerfProfile::Closure1080p120 => PerfProfile::Closure1080p120,
    }
}

fn execute_save_command(args: SaveCommandArgs) -> Result<(), String> {
    let project_path = args
        .project_path
        .as_deref()
        .ok_or_else(|| "missing project path for `wrela save`".to_string())?;
    let out_path = args.out_path.clone().unwrap_or_else(|| {
        let mut path = Path::new(project_path).to_path_buf();
        path.set_extension("wrela-save");
        path.display().to_string()
    });
    let mut command = std::process::Command::new("cargo");
    command
        .arg("run")
        .arg("-p")
        .arg("wrela_reference_host")
        .arg("--quiet")
        .env("WRELA_REFERENCE_HOST_HEADLESS", "1")
        .env("WRELA_REFERENCE_HOST_PROJECT", project_path)
        .env("WRELA_REFERENCE_HOST_SAVE_PATH", &out_path)
        .env("WRELA_REFERENCE_HOST_FRAMES", "8");
    let output = command
        .output()
        .map_err(|err| format!("launch reference-host save runtime: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "reference-host save runtime failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let summary = stdout.trim();
    if !Path::new(&out_path).exists() {
        return Err(format!(
            "reference-host save runtime did not write `{out_path}`"
        ));
    }
    if matches!(args.output_format, OutputFormat::Json) {
        println!("{summary}");
    } else {
        println!("saved live runtime snapshot via reference host: {summary}");
    }
    Ok(())
}

fn execute_load_command(args: LoadCommandArgs) -> Result<(), String> {
    let save_path = args
        .save_path
        .as_deref()
        .ok_or_else(|| "missing save path for `wrela load`".to_string())?;
    let record = read_record(Path::new(save_path)).map_err(|err| err.to_string())?;
    let project = match args.project_path.as_deref() {
        Some(project_path) => persistence_project_for_path(project_path)?.0,
        None => PersistenceProject {
            project_id: record.header.project_id.clone(),
            wrela_version: record.header.wrela_version.clone(),
            engine_compatibility_hash: record.header.engine_compatibility_hash,
            generator_compatibility_hashes: record.header.generator_compatibility_hashes.clone(),
            archetype_schema_hashes: record.header.archetype_schema_hashes.clone(),
        },
    };
    let (_snapshot, plan) =
        load_snapshot_record(record, &project).map_err(|err| err.to_string())?;
    match args.output_format {
        OutputFormat::Json => println!(
            "{{\"command\":\"load\",\"path\":\"{}\",\"project_id\":\"{}\",\"snapshot_epoch\":{},\"ledger_records\":{}}}",
            json_escape(save_path),
            json_escape(&project.project_id),
            plan.snapshot_epoch.0,
            plan.ledger.len()
        ),
        _ => println!(
            "loaded `{}` for project `{}` at epoch {} ({} ledger records)",
            save_path,
            project.project_id,
            plan.snapshot_epoch.0,
            plan.ledger.len()
        ),
    }
    Ok(())
}

fn persistence_project_for_path(
    path: &str,
) -> Result<(PersistenceProject, Vec<SnapshotLedgerRecord>), String> {
    let loaded = wrela::hir::project::load_project_with_entrypoint(Path::new(path), false)
        .map_err(|errors| format!("load project `{path}`: {errors:?}"))?;
    let project_id = Path::new(path)
        .file_stem()
        .and_then(|os| os.to_str())
        .unwrap_or("wrela_project")
        .to_string();
    let mut generator_compatibility_hashes = BTreeMap::new();
    let mut archetype_schema_hashes = BTreeMap::new();
    for (_, function) in loaded.module.functions.iter() {
        let role = format!("{:?}", function.role);
        let hash = stable_semantic_id(&[
            project_id.as_bytes(),
            b"persistence",
            function.name.as_str().as_bytes(),
            role.as_bytes(),
        ]);
        match function.role {
            wrela::hir::FunctionRole::Region
            | wrela::hir::FunctionRole::Field
            | wrela::hir::FunctionRole::Body => {
                generator_compatibility_hashes.insert(function.name.to_string(), hash);
            }
            wrela::hir::FunctionRole::System
            | wrela::hir::FunctionRole::Voice
            | wrela::hir::FunctionRole::InputMap => {
                archetype_schema_hashes.insert(function.name.to_string(), hash);
            }
            _ => {}
        }
    }
    let ledger = loaded
        .module
        .functions
        .iter()
        .filter(|(_, function)| {
            matches!(
                function.role,
                wrela::hir::FunctionRole::System
                    | wrela::hir::FunctionRole::InputMap
                    | wrela::hir::FunctionRole::Body
                    | wrela::hir::FunctionRole::Voice
                    | wrela::hir::FunctionRole::Region
            )
        })
        .map(|(_, function)| SnapshotLedgerRecord {
            handle: PersistentHandle::from_stable_semantic_parts(&[
                project_id.as_bytes(),
                function.name.as_str().as_bytes(),
            ]),
            type_id: format!("{:?}", function.role),
            payload: Value::Text(function.name.to_string()),
        })
        .collect();
    Ok((
        PersistenceProject {
            engine_compatibility_hash: stable_semantic_id(&[project_id.as_bytes(), b"engine"]),
            project_id,
            wrela_version: env!("CARGO_PKG_VERSION").to_string(),
            generator_compatibility_hashes,
            archetype_schema_hashes,
        },
        ledger,
    ))
}

fn json_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn build_kpi_thresholds(
    check_fallback_max: Option<f64>,
    check_batch_min: Option<f64>,
    scheduler_p99_improve_min_pct: Option<f64>,
    rewrite_overhead_max_pct: Option<f64>,
    actor_throughput_improve_min_pct: Option<f64>,
    queue_age_p99_max_regress_pct: Option<f64>,
    starvation_violations_max: Option<f64>,
    scheduler_throughput_improve_min_pct: Option<f64>,
    scheduler_loop_p99_max_regress_pct: Option<f64>,
    scheduler_local_hit_min: Option<f64>,
) -> KpiThresholds {
    KpiThresholds {
        check_fallback_max,
        check_batch_min,
        scheduler_p99_improve_min_pct,
        rewrite_overhead_max_pct,
        actor_throughput_improve_min_pct,
        queue_age_p99_max_regress_pct,
        starvation_violations_max,
        scheduler_throughput_improve_min_pct,
        scheduler_loop_p99_max_regress_pct,
        scheduler_local_hit_min,
    }
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
    match parsed {
        ParsedCommand::Init(args) => {
            if trace {
                eprintln!("build: command init");
            }
            let target = args.target.as_deref().unwrap_or(".");
            if let Err(err) = init_project_with_template(target, args.template.as_deref()) {
                eprintln!("init error: {err}");
                std::process::exit(EXIT_USAGE);
            }
        }
        ParsedCommand::Update(args) => {
            if trace {
                eprintln!("build: command update");
            }
            if let Err(err) = update_toolchain(args.prefix_path.as_deref()) {
                eprintln!("update error: {err}");
                std::process::exit(EXIT_CODEGEN);
            }
        }
        ParsedCommand::QueryContracts(args) => {
            if trace {
                eprintln!("build: command query-contracts");
            }
            execute_query_contracts_command(args);
        }
        ParsedCommand::CollisionContracts(args) => {
            if trace {
                eprintln!("build: command collision-contracts");
            }
            execute_collision_contracts_command(args);
        }
        ParsedCommand::CollisionPlan(args) => {
            if trace {
                eprintln!("build: command collision-plan");
            }
            execute_collision_plan_command(args);
        }
        ParsedCommand::CollisionRun(args) => {
            if trace {
                eprintln!("build: command collision-run");
            }
            execute_collision_run_command(args);
        }
        ParsedCommand::Preview(args) => {
            if trace {
                eprintln!("build: command preview");
            }
            execute_preview_command(args);
        }
        ParsedCommand::Frame(args) => {
            if trace {
                eprintln!("build: command frame");
            }
            execute_frame_command(args);
        }
        ParsedCommand::FrameLive(args) => {
            if trace {
                eprintln!("build: command frame-live");
            }
            execute_frame_live_command(args);
        }
        ParsedCommand::Live(args) => {
            if trace {
                eprintln!("build: command live");
            }
            execute_live_command(args);
        }
        ParsedCommand::PerfLatency(args) => {
            if trace {
                eprintln!("build: command perf-latency");
            }
            execute_perf_latency_command(args);
        }
        ParsedCommand::Save(args) => {
            if trace {
                eprintln!("build: command save");
            }
            if let Err(err) = execute_save_command(args) {
                eprintln!("save error: {err}");
                std::process::exit(EXIT_RUNTIME_SIGNAL);
            }
        }
        ParsedCommand::Load(args) => {
            if trace {
                eprintln!("build: command load");
            }
            if let Err(err) = execute_load_command(args) {
                eprintln!("load error: {err}");
                std::process::exit(EXIT_RUNTIME_SIGNAL);
            }
        }
        ParsedCommand::FrameContracts(args) => {
            if trace {
                eprintln!("build: command frame-contracts");
            }
            execute_frame_contracts_command(args);
        }
        ParsedCommand::PresentationPlan(args) => {
            if trace {
                eprintln!("build: command presentation-plan");
            }
            execute_presentation_plan_command(args);
        }
        ParsedCommand::PresentationDebug(args) => {
            if trace {
                eprintln!("build: command presentation-debug");
            }
            execute_presentation_debug_command(args);
        }
        ParsedCommand::Check(args) | ParsedCommand::Analyze(args) => {
            if trace {
                eprintln!("build: command check");
            }
            let entry_path = resolve_entry_path(args.path_arg.as_deref());
            let entry_path = match entry_path {
                Ok(path) => path,
                Err(err) => {
                    eprintln!("error: {err}");
                    std::process::exit(EXIT_USAGE);
                }
            };
            let result = compile_to_mir(
                &entry_path,
                args.output_format,
                args.emit_mir,
                args.emit_mir_opt,
                false,
                true,
                args.strict_naming,
                args.analysis_holes_only,
                args.query_backend,
            );
            if let Err(code) = result {
                std::process::exit(code);
            }
        }
        ParsedCommand::Fix(args) => {
            if trace {
                eprintln!("build: command fix");
            }
            let entry_path = resolve_entry_path(args.path_arg.as_deref());
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
                DiagnosticScope::from_entrypoint(&entry_path, args.workspace_diagnostics);

            for _ in 0..MAX_PASSES {
                let fixes = match collect_safe_fixes(
                    &entry_path,
                    args.output_format,
                    args.fix_allow_review_fixes,
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
            emit_fix_summary(args.output_format, summary);

            if had_apply_error {
                std::process::exit(EXIT_CODEGEN);
            }
            if !any_fix_candidates || applied == 0 {
                eprintln!("fix: no safe non-overlapping fixes found");
                std::process::exit(EXIT_TYPE);
            }
            eprintln!("fix: applied {} safe fix(es)", applied);
        }
        ParsedCommand::Fmt(args) => {
            if trace {
                eprintln!("build: command fmt");
            }
            let format_targets = match resolve_format_targets(args.path_arg.as_deref()) {
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
                    args.output_format,
                    args.fix_allow_review_fixes,
                    args.workspace_diagnostics,
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
            emit_fmt_summary(args.output_format, summary);
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
        ParsedCommand::Build(args) | ParsedCommand::Compile(args) => {
            if trace {
                eprintln!("build: command build");
            }
            let build_start = Instant::now();
            let entry_path = resolve_entry_path(args.path_arg.as_deref());
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
            let budget_policy = resolve_budget_policy_v1(args.test_jobs, args.test_timeout_ms);
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
            if args.integration_mode {
                eprintln!(
                    "warning: --integration-mode on build bypasses strict naming checks and certification gates for integration-facing executables"
                );
                let mir_compile_start = Instant::now();
                let mir_module = match compile_to_mir(
                    &entry_path,
                    args.output_format,
                    args.emit_mir,
                    args.emit_mir_opt,
                    true,
                    false,
                    false,
                    false,
                    args.query_backend,
                ) {
                    Ok(mir) => mir,
                    Err(code) => std::process::exit(code),
                };
                let mir_compile_ms = mir_compile_start.elapsed().as_millis();
                if let Some(path) = args.emit_obj {
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
                let output_path = args
                    .out_path
                    .or(args.emit_bin)
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
                    args.output_format,
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
                emit_certification_cache_hit(args.output_format, &cert_cache_hash, &cert_cache_dir);
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
                    resolve_certification_test_selection(&workspace_root, args.output_format);
                let cert_result = cert_engine::run_tests(
                    &TestTarget::ProjectRoot(workspace_root.clone()),
                    &budget_policy,
                    jobs,
                    timeout,
                    args.output_format,
                    args.perf_debug,
                    None,
                    &cert_selection,
                    true,
                    HttpCassetteMode::Replay,
                    None,
                    args.query_backend,
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
            if args.integration_mode {
                eprintln!(
                    "warning: --integration-mode on build bypasses strict naming checks for integration-facing executables"
                );
            }
            let mir_compile_start = Instant::now();
            let mir_module = match compile_to_mir(
                &entry_path,
                args.output_format,
                args.emit_mir,
                args.emit_mir_opt,
                true,
                !args.integration_mode,
                args.strict_naming,
                false,
                args.query_backend,
            ) {
                Ok(mir) => mir,
                Err(code) => std::process::exit(code),
            };
            let mir_compile_ms = mir_compile_start.elapsed().as_millis();
            if let Some(path) = args.emit_obj {
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
            let output_path = args
                .out_path
                .or(args.emit_bin)
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
                args.output_format,
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
        ParsedCommand::VerifyCert(args) => {
            let cert_path = PathBuf::from(args.cert_path);
            if let Err(err) = verify_certification_report(&cert_path) {
                eprintln!("{err}");
                std::process::exit(EXIT_CODEGEN);
            }
            println!("cert verified: {}", cert_path.display());
        }
        ParsedCommand::Run(args) => {
            if trace {
                eprintln!("build: command run");
            }
            let entry_path = resolve_entry_path(args.path_arg.as_deref());
            let entry_path = match entry_path {
                Ok(path) => path,
                Err(err) => {
                    eprintln!("error: {err}");
                    std::process::exit(EXIT_USAGE);
                }
            };
            let mir_module = if args.integration_mode {
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
                    args.output_format,
                    args.emit_mir,
                    args.emit_mir_opt,
                    true,
                    false,
                    false,
                    false,
                    args.query_backend,
                ) {
                    Ok(mir) => mir,
                    Err(code) => std::process::exit(code),
                }
            } else {
                match compile_to_mir(
                    &entry_path,
                    args.output_format,
                    args.emit_mir,
                    args.emit_mir_opt,
                    true,
                    true,
                    args.strict_naming,
                    false,
                    args.query_backend,
                ) {
                    Ok(mir) => mir,
                    Err(code) => std::process::exit(code),
                }
            };
            let generated_temp_output = args.out_path.is_none();
            let output = args.out_path.unwrap_or_else(temp_exe_path);
            if let Err(err) =
                wrela::backend::cranelift::compile_to_executable(&mir_module, output.as_ref())
            {
                eprintln!("codegen error: {}", err.0);
                std::process::exit(EXIT_CODEGEN);
            }
            let status = match Command::new(&output).args(&args.program_args).status() {
                Ok(status) => status,
                Err(err) => {
                    eprintln!("error: failed to run compiled binary {output}: {err}");
                    std::process::exit(EXIT_CODEGEN);
                }
            };
            if generated_temp_output {
                let _ = fs::remove_file(&output);
            }
            std::process::exit(status.code().unwrap_or(EXIT_RUNTIME_SIGNAL));
        }
        ParsedCommand::Dev(args) => {
            if trace {
                eprintln!("build: command dev");
            }
            let entry_path = resolve_entry_path(args.path_arg.as_deref());
            let entry_path = match entry_path {
                Ok(path) => path,
                Err(err) => {
                    eprintln!("error: {err}");
                    std::process::exit(EXIT_USAGE);
                }
            };
            let poll = args.poll_ms.unwrap_or(500);
            run_dev_loop(
                &entry_path,
                poll,
                args.output_format,
                args.emit_mir,
                args.emit_mir_opt,
                args.strict_naming,
                args.query_backend,
                &args.program_args,
            );
        }
        ParsedCommand::Test(args) => {
            let kpi_thresholds = build_kpi_thresholds(
                args.kpi_check_fallback_max,
                args.kpi_check_batch_min,
                args.kpi_scheduler_p99_improve_min_pct,
                args.kpi_rewrite_overhead_max_pct,
                args.kpi_actor_throughput_improve_min_pct,
                args.kpi_queue_age_p99_max_regress_pct,
                args.kpi_starvation_violations_max,
                args.kpi_scheduler_throughput_improve_min_pct,
                args.kpi_scheduler_loop_p99_max_regress_pct,
                args.kpi_scheduler_local_hit_min,
            );
            let exit = cert_engine::execute_test_command(cert_engine::TestCommandInput {
                trace,
                program_args: Vec::new(),
                out_path: args.out_path,
                emit_obj: args.emit_obj,
                emit_bin: args.emit_bin,
                path_arg: args.path_arg,
                test_jobs: args.test_jobs,
                test_timeout_ms: args.test_timeout_ms,
                test_record: args.test_record,
                test_update_public_surface: args.test_update_public_surface,
                test_selection: into_test_selection(args.test_selection),
                repro_artifact_path: args.repro_artifact_path,
                replay_trace_path: args.replay_trace_path,
                output_format: args.output_format,
                perf_debug: args.perf_debug,
                perf_gate_path: args.perf_gate_path,
                perf_max_regression_pct: args.perf_max_regression_pct,
                kpi_thresholds,
                test_seed: args.test_seed,
                query_backend: args.query_backend,
            });
            std::process::exit(exit);
        }
        ParsedCommand::Eval(args) => {
            let exit = execute_eval_command(EvalCommandInput {
                trace,
                path_arg: args.path_arg,
                program_args: args.program_args,
                runs: args.runs,
                output_format: args.output_format,
            });
            std::process::exit(exit);
        }
        ParsedCommand::Perf(args) => {
            let kpi_thresholds = build_kpi_thresholds(
                args.kpi_check_fallback_max,
                args.kpi_check_batch_min,
                args.kpi_scheduler_p99_improve_min_pct,
                args.kpi_rewrite_overhead_max_pct,
                args.kpi_actor_throughput_improve_min_pct,
                args.kpi_queue_age_p99_max_regress_pct,
                args.kpi_starvation_violations_max,
                args.kpi_scheduler_throughput_improve_min_pct,
                args.kpi_scheduler_loop_p99_max_regress_pct,
                args.kpi_scheduler_local_hit_min,
            );
            let exit = perf_engine::execute_perf_command(perf_engine::PerfCommandInput {
                trace,
                program_args: Vec::new(),
                path_arg: args.path_arg,
                perf_runs: args.perf_runs,
                test_jobs: args.test_jobs,
                test_timeout_ms: args.test_timeout_ms,
                benchmark_manifest_path: args.benchmark_manifest_path,
                perf_profile: into_perf_profile(args.perf_profile),
                perf_baseline_out: args.perf_baseline_out,
                perf_gate_path: args.perf_gate_path,
                perf_max_regression_pct: args.perf_max_regression_pct,
                perf_cv_max_pct: args.perf_cv_max_pct,
                perf_why_not_120: args.perf_why_not_120,
                kpi_thresholds,
                output_format: args.output_format,
                perf_debug: args.perf_debug,
                test_selection: into_test_selection(args.test_selection),
                query_backend: args.query_backend,
            });
            std::process::exit(exit);
        }
        ParsedCommand::Perfcmp(args) => {
            let exit = perf_engine::execute_perfcmp_command(perf_engine::PerfcmpCommandInput {
                trace,
                program_args: Vec::new(),
                path_arg: args.path_arg,
                benchmark_manifest_path: args.benchmark_manifest_path,
                perfcmp_baseline_ref: args.perfcmp_baseline_ref,
                perfcmp_candidate_ref: args.perfcmp_candidate_ref,
                out_path: args.out_path,
                output_format: args.output_format,
                perf_profile: into_perf_profile(args.perf_profile),
                perfcmp_warmup_pairs: args.perfcmp_warmup_pairs,
                perfcmp_measure_pairs: args.perfcmp_measure_pairs,
                perfcmp_min_effect_pct: args.perfcmp_min_effect_pct,
                perfcmp_confidence_pct: args.perfcmp_confidence_pct,
                test_timeout_ms: args.test_timeout_ms,
                perf_debug: args.perf_debug,
            });
            std::process::exit(exit);
        }
        ParsedCommand::Matrix(args) => {
            let kpi_thresholds = build_kpi_thresholds(
                args.kpi_check_fallback_max,
                args.kpi_check_batch_min,
                args.kpi_scheduler_p99_improve_min_pct,
                args.kpi_rewrite_overhead_max_pct,
                args.kpi_actor_throughput_improve_min_pct,
                args.kpi_queue_age_p99_max_regress_pct,
                args.kpi_starvation_violations_max,
                args.kpi_scheduler_throughput_improve_min_pct,
                args.kpi_scheduler_loop_p99_max_regress_pct,
                args.kpi_scheduler_local_hit_min,
            );
            let exit = perf_engine::execute_matrix_command(perf_engine::MatrixCommandInput {
                trace,
                program_args: Vec::new(),
                path_arg: args.path_arg,
                perf_runs: args.perf_runs,
                perf_gate_path: args.perf_gate_path,
                perf_max_regression_pct: args.perf_max_regression_pct,
                kpi_thresholds,
            });
            std::process::exit(exit);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wrela::hir::lower as hir_lower;
    use wrela::parser::ast;
    use wrela::parser::ast::AstNode;
    use wrela::parser::parse;

    fn lower_inline_module(source: &str) -> hir::Module {
        let node = parse(source);
        let root = ast::Root::cast(node).expect("root");
        hir_lower::lower(root)
    }

    fn function<'a>(module: &'a hir::Module, name: &str) -> &'a hir::Function {
        module
            .functions
            .iter()
            .find(|(_, func)| func.name == name)
            .map(|(_, func)| func)
            .unwrap_or_else(|| panic!("missing function `{name}`"))
    }

    #[test]
    fn authored_lighting_follows_grouped_view_helpers() {
        let module = lower_inline_module(
            r#"
view sample_view(world: RegionCapture, camera: Camera) {
    viewport = viewport(width = 2, height = 2)
    lighting = key_light(
        light = Light(
            position = camera.position + vec3(0.5, 1.0, 0.5),
            direction = normalize(vec3(-0.4, -0.7, -0.2)),
            intensity = vec3(1.0, 1.0, 1.0),
            range = 8.0
        ),
        fill_direction = normalize(vec3(-0.2, 0.8, 0.4)),
        fill_strength = 0.33,
        ambient_color = vec3(0.08, 0.11, 0.14)
    )
}
"#,
        );
        let view = function(&module, "sample_view");
        let camera = wrela::presentation_contract::CanonicalCameraInput {
            position: [1.0, 2.0, 3.0],
            forward: [0.0, 0.0, -1.0],
            up: [0.0, 1.0, 0.0],
            vertical_fov_degrees: 46.0,
        };
        let bindings = bind_presentation_function_params(
            view,
            &wrela::query_exec::stable_region_snapshot_handle(&SmolStr::new("scene_region")),
            camera,
        );

        let lighting =
            authored_presentation_lighting_inputs(view, &bindings).expect("authored lighting");
        assert_eq!(lighting.key_light.position, [1.5, 3.0, 3.5]);
        assert_eq!(lighting.key_light.range, 8.0);
        assert!((lighting.fill_strength - 0.33).abs() <= 1e-6);
        assert_eq!(lighting.ambient_color, [0.08, 0.11, 0.14]);
    }

    #[test]
    fn prepared_execution_applies_domain_participant_policy() {
        let module = lower_inline_module(
            r#"
field exact distance scene_field(p: Vec3) -> F32 {
    sphere(radius = 1.0)
}

material scene_material(hit: Hit3) -> Surface {
    return diffuse(color = vec3(0.7, 0.7, 0.7))
}

shape scene_shape {
    field = scene_field
    material = scene_material
}

region scene_region() {
    place scene = scene_shape
}

domain sample_domain(world: RegionCapture) {
    geometry_detail = 1
    material = true
    radiance = false
    media = false
    max_distance = 6.0
    min_step = 0.05
    hit_epsilon = 0.001
    max_steps = 64
}

view sample_view(world: RegionCapture, camera: Camera) {
    domain = sample_domain(world = world)
    viewport = viewport(width = 2, height = 2)
}
"#,
        );
        let (_type_errors, type_info) = hir::typeck::check_module_with_info(&module);
        let query_ctx = wrela::query_exec::QueryExecContext::compile(&module, &type_info);
        let view = function(&module, "sample_view");
        let plan = wrela::presentation_plan::PresentationPlan::from_view_function(
            view,
            wrela::query_plan::DispatchBackend::Auto,
        )
        .expect("plan");
        let prepared = prepare_presentation_execution(
            &module,
            &query_ctx,
            &plan,
            view,
            SmolStr::new("scene_region"),
            SmolStr::new("sample_domain"),
            wrela::presentation_contract::CanonicalCameraInput {
                position: [0.0, 0.0, 3.0],
                forward: [0.0, 0.0, -1.0],
                up: [0.0, 1.0, 0.0],
                vertical_fov_degrees: 46.0,
            },
            None,
            None,
            0,
            1.0 / 60.0,
            wrela::query_plan::DispatchBackend::Auto,
            wrela::query_exec::QueryTraceSolverMode::Hybrid,
            false,
        )
        .expect("prepared execution");

        assert!(prepared.plan.validate().is_empty());
        assert!(!prepared.plan.passes.iter().any(|pass| {
            matches!(
                pass.kind,
                wrela::presentation_plan::PresentationPassKind::ParticipantsResolve { .. }
            )
        }));
        assert!(
            prepared
                .plan
                .frame
                .outputs
                .iter()
                .all(|attachment| attachment.name != "radiance" && attachment.name != "medium")
        );
    }

    #[test]
    fn prepared_execution_strips_export_pass_when_disabled() {
        let module = lower_inline_module(
            r#"
field exact distance scene_field(p: Vec3) -> F32 {
    sphere(radius = 1.0)
}

material scene_material(hit: Hit3) -> Surface {
    return diffuse(color = vec3(0.7, 0.7, 0.7))
}

shape scene_shape {
    field = scene_field
    material = scene_material
}

region scene_region() {
    place scene = scene_shape
}

domain sample_domain(world: RegionCapture) {
    geometry_detail = 1
    material = true
    radiance = false
    media = false
    max_distance = 6.0
    min_step = 0.05
    hit_epsilon = 0.001
    max_steps = 64
}

view sample_view(world: RegionCapture, camera: Camera) {
    domain = sample_domain(world = world)
    viewport = viewport(width = 2, height = 2)
}
"#,
        );
        let (_type_errors, type_info) = hir::typeck::check_module_with_info(&module);
        let query_ctx = wrela::query_exec::QueryExecContext::compile(&module, &type_info);
        let view = function(&module, "sample_view");
        let plan = wrela::presentation_plan::PresentationPlan::from_view_function(
            view,
            wrela::query_plan::DispatchBackend::Auto,
        )
        .expect("plan");

        assert!(plan.export_binding().is_some());
        assert!(plan.passes.iter().any(|pass| {
            matches!(
                pass.kind,
                wrela::presentation_plan::PresentationPassKind::ExportAttachment { .. }
            )
        }));

        let prepared = prepare_presentation_execution(
            &module,
            &query_ctx,
            &plan,
            view,
            SmolStr::new("scene_region"),
            SmolStr::new("sample_domain"),
            wrela::presentation_contract::CanonicalCameraInput {
                position: [0.0, 0.0, 3.0],
                forward: [0.0, 0.0, -1.0],
                up: [0.0, 1.0, 0.0],
                vertical_fov_degrees: 46.0,
            },
            None,
            None,
            0,
            1.0 / 60.0,
            wrela::query_plan::DispatchBackend::Auto,
            wrela::query_exec::QueryTraceSolverMode::Hybrid,
            true,
        )
        .expect("prepared execution");

        assert!(prepared.plan.validate().is_empty());
        assert!(prepared.plan.export_binding().is_none());
        assert!(!prepared.plan.passes.iter().any(|pass| {
            matches!(
                pass.kind,
                wrela::presentation_plan::PresentationPassKind::ExportAttachment { .. }
            )
        }));
    }
}
