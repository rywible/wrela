#![allow(unused_assignments)]

use miette::{Diagnostic, NamedSource, Report, SourceSpan};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::io::{self};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use thiserror::Error;
use wrela::hir;
use wrela::mir;

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
    let mut perf_debug = false;
    let mut perf_runs: Option<usize> = None;
    let mut perf_baseline_out: Option<String> = None;
    let mut perf_gate_path: Option<String> = None;
    let mut perf_max_regression_pct: Option<f64> = None;
    let mut perf_cv_max_pct: Option<f64> = None;
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
            let mir_module =
                match compile_to_mir(&entry_path, output_format, emit_mir, emit_mir_opt, true) {
                    Ok(mir) => mir,
                    Err(code) => std::process::exit(code),
                };
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
            if let Err(err) =
                wrela::backend::cranelift::compile_to_executable(&mir_module, output.as_ref())
            {
                eprintln!("codegen error: {}", err.0);
                std::process::exit(EXIT_CODEGEN);
            }
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
            let jobs = test_jobs.unwrap_or(1).max(1);
            let timeout = Duration::from_millis(test_timeout_ms.unwrap_or(5000).max(1));
            let target = match resolve_test_target(path_arg.as_deref()) {
                Ok(target) => target,
                Err(err) => {
                    eprintln!("error: {err}");
                    std::process::exit(EXIT_USAGE);
                }
            };
            let gate_cfg = perf_gate_path.as_ref().map(|path| PerfGateConfig {
                baseline_path: PathBuf::from(path),
                max_regression_pct: perf_max_regression_pct.unwrap_or(5.0),
            });
            let exit = run_tests(
                &target,
                jobs,
                timeout,
                output_format,
                perf_debug,
                gate_cfg.as_ref(),
            );
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
            let jobs = test_jobs.unwrap_or(1).max(1);
            let timeout = Duration::from_millis(test_timeout_ms.unwrap_or(5000).max(1));
            let target = match resolve_test_target(path_arg.as_deref()) {
                Ok(target) => target,
                Err(err) => {
                    eprintln!("error: {err}");
                    std::process::exit(EXIT_USAGE);
                }
            };
            let baseline_out = perf_baseline_out
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(".artifacts/perf/baseline.json"));
            let gate_cfg = perf_gate_path.as_ref().map(|path| PerfGateConfig {
                baseline_path: PathBuf::from(path),
                max_regression_pct: perf_max_regression_pct.unwrap_or(5.0),
            });
            let cv_max_pct = perf_cv_max_pct.unwrap_or(5.0);
            let exit = run_perf_harness(
                &target,
                jobs,
                timeout,
                output_format,
                perf_debug,
                runs,
                cv_max_pct,
                &baseline_out,
                gate_cfg.as_ref(),
            );
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
  build <path>          compile to a native executable\n\
  compile <path>        alias for build\n\
  run <path>            compile and run\n\
  dev <path>            watch and rebuild (polling)\n\
  test [path]           run tests from project root or a single .wr file\n\
  perf [path]           run perf harness and write baseline JSON\n\
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
  --perf-debug          dump perf counters after tests\n\
  --runs=N              perf harness run count (default: 5)\n\
  --baseline-out=PATH   perf baseline JSON output path\n\
  --perf-gate=PATH      compare perf summary against baseline JSON\n\
  --perf-max-regression-pct=N  allowed regression percentage (default: 5)\n\
  --perf-cv-max-pct=N   max coefficient of variation percentage (default: 5)\n\
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

#[derive(Clone, Copy)]
enum OutputFormat {
    Pretty,
    Json,
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
            | "run"
            | "dev"
            | "test"
            | "perf"
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
    steps: Vec<MatrixStepEvidence>,
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
        version: 1,
        generated_at_unix_ms,
        workspace_root: workspace_root.display().to_string(),
        success: false,
        exit_code: EXIT_CODEGEN,
        perf_runs,
        perf_baseline_path: perf_baseline_path.display().to_string(),
        perf_gate_path: perf_gate_path.map(|s| s.to_string()),
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

#[derive(Clone)]
struct TestCase {
    name: String,
    module_path: String,
    func_name: String,
}

#[derive(Debug, Deserialize, Clone)]
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
    abi_typed_lane: u64,
    #[serde(default)]
    abi_boxed_lane: u64,
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
    abi_typed_lane: u64,
    abi_boxed_lane: u64,
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
        self.abi_typed_lane += metrics.abi_typed_lane;
        self.abi_boxed_lane += metrics.abi_boxed_lane;
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

struct TestRun {
    metrics: Option<MetricsDump>,
    compile_ns: u128,
    runtime_ns: u128,
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
    metrics: MetricsTotals,
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
}

fn run_tests(
    target: &TestTarget,
    jobs: usize,
    timeout: Duration,
    output_format: OutputFormat,
    perf_debug: bool,
    perf_gate: Option<&PerfGateConfig>,
) -> i32 {
    let (exit, summary) = run_tests_once(
        target,
        jobs,
        timeout,
        output_format,
        perf_debug,
        perf_gate.is_some(),
    );
    if exit != EXIT_OK {
        return exit;
    }
    if let (Some(gate), Some(summary)) = (perf_gate, summary.as_ref()) {
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
        let failures = evaluate_perf_gate(summary, &baseline, gate.max_regression_pct);
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
    EXIT_OK
}

fn run_tests_once(
    target: &TestTarget,
    jobs: usize,
    timeout: Duration,
    output_format: OutputFormat,
    perf_debug: bool,
    perf_lane: bool,
) -> (i32, Option<PerfSummary>) {
    configure_runtime_for_test_lane(perf_lane, perf_debug);
    let mut tests = Vec::new();
    let (workspace_root, compile_root, tests_root, missing_path_msg) = match target {
        TestTarget::ProjectRoot(root) => {
            let src_root = root.join("src");
            let tests_root = root.join("tests");
            if !tests_root.is_dir() {
                eprintln!("no tests found at {}", tests_root.display());
                return (EXIT_OK, None);
            }
            if let Err(err) = collect_tests(&tests_root, &tests_root, &mut tests) {
                eprintln!("test discovery error: {err}");
                return (EXIT_USAGE, None);
            }
            (
                root.clone(),
                src_root,
                Some(tests_root.clone()),
                tests_root.display().to_string(),
            )
        }
        TestTarget::SingleFile(path) => {
            let Some(parent) = path.parent() else {
                eprintln!("test discovery error: file has no parent directory");
                return (EXIT_USAGE, None);
            };
            let source = match fs::read_to_string(path) {
                Ok(source) => source,
                Err(err) => {
                    eprintln!("test discovery error: {err}");
                    return (EXIT_USAGE, None);
                }
            };
            let module_path = match module_path_for_single_file(path) {
                Ok(module_path) => module_path,
                Err(err) => {
                    eprintln!("test discovery error: {err}");
                    return (EXIT_USAGE, None);
                }
            };
            for func in extract_test_functions(&source) {
                tests.push(TestCase {
                    name: format!("{module_path}::{func}"),
                    module_path: module_path.clone(),
                    func_name: func,
                });
            }
            (
                parent.to_path_buf(),
                parent.to_path_buf(),
                None,
                path.display().to_string(),
            )
        }
    };

    if tests.is_empty() {
        eprintln!("no tests found at {}", missing_path_msg);
        return (EXIT_OK, None);
    }

    let queue = std::sync::Arc::new(std::sync::Mutex::new(std::collections::VecDeque::from(
        tests,
    )));
    let (tx, rx) = std::sync::mpsc::channel::<(String, bool, Duration, String, Option<TestRun>)>();
    let mut handles = Vec::new();

    for _ in 0..jobs {
        let queue = std::sync::Arc::clone(&queue);
        let tx = tx.clone();
        let compile_root = compile_root.clone();
        let workspace_root = workspace_root.clone();
        let tests_root = tests_root.clone();
        handles.push(std::thread::spawn(move || {
            loop {
                let next = {
                    let mut guard = queue.lock().expect("test queue");
                    guard.pop_front()
                };
                let Some(test) = next else { break };
                let start = Instant::now();
                let result = run_single_test(
                    &workspace_root,
                    &compile_root,
                    tests_root.as_deref(),
                    &test,
                    timeout,
                    output_format,
                );
                let dur = start.elapsed();
                let (ok, err, run) = match result {
                    Ok(run) => (true, String::new(), Some(run)),
                    Err(msg) => (false, msg, None),
                };
                let _ = tx.send((test.name.clone(), ok, dur, err, run));
            }
        }));
    }
    drop(tx);

    let mut ok_count = 0usize;
    let mut fail_count = 0usize;
    let mut compile_ns: Vec<u128> = Vec::new();
    let mut runtime_ns: Vec<u128> = Vec::new();
    let mut metrics_totals = MetricsTotals::default();
    let mut metrics_count = 0usize;
    for (name, ok, dur, err, run) in rx.iter() {
        if let Some(run) = run.as_ref() {
            compile_ns.push(run.compile_ns);
            runtime_ns.push(run.runtime_ns);
            if let Some(metrics) = run.metrics.as_ref() {
                metrics_totals.add(metrics);
                metrics_count += 1;
            }
        }
        if ok {
            println!("ok   {:>7?}  {}", dur, name);
            ok_count += 1;
        } else {
            println!("fail {:>7?}  {}  {}", dur, name, err);
            fail_count += 1;
        }
    }
    for handle in handles {
        let _ = handle.join();
    }
    println!("tests: {} passed, {} failed", ok_count, fail_count);
    if fail_count != 0 || runtime_ns.is_empty() {
        return (EXIT_CODEGEN, None);
    }
    let summary = build_perf_summary(&compile_ns, &runtime_ns, metrics_count, &metrics_totals);
    print_perf_summary(&summary, perf_debug);
    (EXIT_OK, Some(summary))
}

fn run_perf_harness(
    target: &TestTarget,
    jobs: usize,
    timeout: Duration,
    output_format: OutputFormat,
    perf_debug: bool,
    runs: usize,
    cv_max_pct: f64,
    baseline_out: &Path,
    perf_gate: Option<&PerfGateConfig>,
) -> i32 {
    let mut samples = Vec::new();
    for idx in 0..runs {
        println!("perf-run {}/{}", idx + 1, runs);
        let (exit, summary) =
            run_tests_once(target, jobs, timeout, output_format, perf_debug, true);
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
    if cv.compile_throughput_pct > cv_max_pct
        || cv.runtime_p50_pct > cv_max_pct
        || cv.runtime_p95_pct > cv_max_pct
        || cv.runtime_p99_pct > cv_max_pct
    {
        eprintln!(
            "perf harness failed: coefficient of variation exceeded {:.2}%",
            cv_max_pct
        );
        eprintln!(
            "cv: compile={:.2}% runtime_p50={:.2}% runtime_p95={:.2}% runtime_p99={:.2}%",
            cv.compile_throughput_pct, cv.runtime_p50_pct, cv.runtime_p95_pct, cv.runtime_p99_pct
        );
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
        let failures = evaluate_perf_gate(&summary, &baseline, gate.max_regression_pct);
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

fn configure_runtime_for_test_lane(perf_lane: bool, perf_debug: bool) {
    if !perf_lane {
        return;
    }
    if env::var_os("WRELA_RUNTIME_PROFILE").is_none() {
        // Perf lanes should exercise release runtime defaults.
        // SAFETY: this happens before test worker threads are spawned.
        unsafe { env::set_var("WRELA_RUNTIME_PROFILE", "release") };
    }
    if env::var_os("WRELA_RUNTIME_METRICS").is_none() {
        // Keep metrics out of hot loops unless profiling mode is explicitly enabled.
        // SAFETY: this happens before test worker threads are spawned.
        unsafe { env::set_var("WRELA_RUNTIME_METRICS", if perf_debug { "1" } else { "0" }) };
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
    if perf_debug {
        println!(
            "perf-debug: rc_inc={} rc_dec={} mailbox_enqueue_ok={} mailbox_enqueue_fail={} mailbox_dequeue={} mailbox_high_water={} alloc_list={} alloc_map={} alloc_string={} alloc_bytes={} alloc_result={} alloc_pending={} messages_sent={} messages_dropped={} pending_resolved={} pending_dropped={} sched_dispatched={} sched_skipped_no_credit={} abi_typed_lane={} abi_boxed_lane={}",
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
        metrics.abi_typed_lane += sample.metrics.abi_typed_lane;
        metrics.abi_boxed_lane += sample.metrics.abi_boxed_lane;
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
        metrics,
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
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_tests(&path, tests_root, out)?;
            continue;
        }
        if path.extension().and_then(|s| s.to_str()) != Some("wr") {
            continue;
        }
        let source = fs::read_to_string(&path)?;
        let module_path = module_path_for_test_file(&path, tests_root)?;
        let names = extract_test_functions(&source);
        for func in names {
            let name = format!("{module_path}::{func}");
            out.push(TestCase {
                name,
                module_path: module_path.clone(),
                func_name: func,
            });
        }
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
    let mut rel = rel.to_path_buf();
    rel.set_extension("");
    let parts: Vec<String> = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .filter(|s| !s.is_empty())
        .collect();
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

fn extract_test_functions(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with("to test_") {
            continue;
        }
        let rest = trimmed.trim_start_matches("to ").trim();
        let name_end = rest.find('(').unwrap_or(rest.len());
        let name = rest[..name_end].trim();
        if name.starts_with("test_") && !name.is_empty() {
            out.push(name.to_string());
        }
    }
    out
}

fn run_single_test(
    workspace_root: &Path,
    compile_root: &Path,
    tests_root: Option<&Path>,
    test: &TestCase,
    timeout: Duration,
    output_format: OutputFormat,
) -> Result<TestRun, String> {
    let temp_dir = workspace_root.join("target").join("wrela_tests");
    let _ = fs::create_dir_all(&temp_dir);
    let file_stem = test
        .name
        .replace('/', "_")
        .replace(':', "_")
        .replace("::", "_");
    let entry_path = temp_dir.join(format!("{}_entry.wr", file_stem));
    let exe_path = temp_dir.join(format!("{}_bin", file_stem));
    let entry = format!(
        "use {func} from {module}\n\nto run() -> Integer:\n    {func}()\n    return 0\n",
        func = test.func_name,
        module = test.module_path
    );
    if let Err(err) = fs::write(&entry_path, entry) {
        return Err(format!("failed to write test entry: {err}"));
    }
    let compile_start = Instant::now();
    let mir_module =
        match compile_to_mir_with_root(&entry_path, compile_root, tests_root, output_format) {
            Ok(mir) => mir,
            Err(_) => return Err("compile failed".to_string()),
        };
    if let Err(err) = wrela::backend::cranelift::compile_to_executable(&mir_module, &exe_path) {
        return Err(format!("codegen error: {}", err.0));
    }
    let compile_ns = compile_start.elapsed().as_nanos();
    let metrics_path = temp_dir.join(format!("{}_metrics.json", file_stem));
    let _ = fs::remove_file(&metrics_path);
    let runtime_start = Instant::now();
    if let Some(delay_ms) = synthetic_slowdown_ms() {
        std::thread::sleep(Duration::from_millis(delay_ms));
    }
    run_with_timeout(&exe_path, timeout, Some(&metrics_path)).map_err(|e| e)?;
    let runtime_ns = runtime_start.elapsed().as_nanos();
    let metrics = read_metrics_dump(&metrics_path);
    Ok(TestRun {
        metrics,
        compile_ns,
        runtime_ns,
    })
}

fn synthetic_slowdown_ms() -> Option<u64> {
    let raw = env::var("WRELA_TEST_SLOWDOWN_MS").ok()?;
    raw.parse::<u64>().ok()
}

fn read_metrics_dump(path: &Path) -> Option<MetricsDump> {
    let data = fs::read(path).ok()?;
    serde_json::from_slice(&data).ok()
}

fn run_with_timeout(
    exe: &Path,
    timeout: Duration,
    metrics_path: Option<&Path>,
) -> Result<(), String> {
    let mut command = Command::new(exe);
    if let Some(path) = metrics_path {
        command.env("WRELA_METRICS_PATH", path);
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
        eprintln!(
            "check-oracle: extracted={} skipped={}",
            check_ir.checks.len(),
            check_ir.skipped.len()
        );
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
