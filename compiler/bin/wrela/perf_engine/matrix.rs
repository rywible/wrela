//! Owns matrix/dev-loop evidence aggregation across perf, test, and check lanes.
//! Does not own raw benchmark collection or closure verdict policy.
//!
//! Key invariants:
//! - matrix evidence is derived from already-measured summaries rather than
//!   inventing new timing truth.
//! - lane KPI math here must preserve the typed lane/profile intent chosen by
//!   the caller.
//!
//! Primary entrypoints:
//! - `execute_matrix_command`
//! - matrix evidence helpers in this module
//!
//! Failure modes / common pitfalls:
//! - mixing incomparable lane summaries here makes throughput/readability
//!   evidence look stronger than the underlying runs support.

use super::*;

pub(crate) struct MatrixCommandInput {
    pub(crate) trace: bool,
    pub(crate) program_args: Vec<String>,
    pub(crate) path_arg: Option<String>,
    pub(crate) perf_runs: Option<usize>,
    pub(crate) perf_gate_path: Option<String>,
    pub(crate) perf_max_regression_pct: Option<f64>,
    pub(crate) kpi_thresholds: KpiThresholds,
}

pub(crate) fn execute_matrix_command(input: MatrixCommandInput) -> i32 {
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
    perf_summary: Option<test_eval_perf::PerfSummary>,
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

pub(super) fn run_matrix(
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
        "--lane=fast".to_string(),
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
            name: "fast-tests",
            program: &self_bin,
            args: vec![
                "test".to_string(),
                "language/spec".to_string(),
                "--lane=fast".to_string(),
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
    evidence.perf_summary = test_eval_perf::load_perf_baseline_summary(&perf_baseline_path).ok();
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

fn check_lane_kpis_from_summary(summary: &test_eval_perf::PerfSummary) -> CheckLaneKpis {
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
