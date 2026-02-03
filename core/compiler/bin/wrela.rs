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
            let result =
                compile_to_mir(&entry_path, output_format, emit_mir, emit_mir_opt, false);
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
            let root = match resolve_test_root(path_arg.as_deref()) {
                Ok(path) => path,
                Err(err) => {
                    eprintln!("error: {err}");
                    std::process::exit(EXIT_USAGE);
                }
            };
            let exit = run_tests(&root, jobs, timeout, output_format, perf_debug);
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
  test [path]           discover and run tests\n\
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
    let span = JsonSpan {
        offset: span.offset(),
        len: span.len(),
    };
    let json = JsonDiag {
        kind: kind.to_string(),
        message,
        path,
        span,
    };
    println!(
        "{}",
        serde_json::to_string(&json).unwrap_or_else(|_| "{}".to_string())
    );
}

fn is_command(arg: &str) -> bool {
    matches!(
        arg,
        "init" | "update" | "check" | "build" | "compile" | "run" | "dev" | "test"
    )
}

fn resolve_test_root(path_arg: Option<&str>) -> Result<PathBuf, String> {
    let path = PathBuf::from(path_arg.unwrap_or("."));
    let candidate = if path.is_file() {
        path.parent().map(|p| p.to_path_buf()).unwrap_or(path)
    } else {
        path
    };
    if candidate.is_dir() {
        return Ok(candidate);
    }
    Err("test root must be a directory".to_string())
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
}

#[derive(Default, Debug, Clone)]
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
}

fn run_tests(
    root: &Path,
    jobs: usize,
    timeout: Duration,
    output_format: OutputFormat,
    perf_debug: bool,
) -> i32 {
    let src_root = root.join("src");
    let tests_root = root.join("tests");
    if !tests_root.is_dir() {
        eprintln!("no tests found at {}", tests_root.display());
        return EXIT_OK;
    }

    let mut tests = Vec::new();
    if let Err(err) = collect_tests(&tests_root, &src_root, &tests_root, &mut tests) {
        eprintln!("test discovery error: {err}");
        return EXIT_USAGE;
    }
    if tests.is_empty() {
        eprintln!("no tests found at {}", tests_root.display());
        return EXIT_OK;
    }

    let queue = std::sync::Arc::new(std::sync::Mutex::new(std::collections::VecDeque::from(tests)));
    let (tx, rx) = std::sync::mpsc::channel::<(String, bool, Duration, String, Option<MetricsDump>)>();
    let mut handles = Vec::new();

    for _ in 0..jobs {
        let queue = std::sync::Arc::clone(&queue);
        let tx = tx.clone();
        let src_root = src_root.clone();
        let root = root.to_path_buf();
        handles.push(std::thread::spawn(move || {
            loop {
                let next = {
                    let mut guard = queue.lock().expect("test queue");
                    guard.pop_front()
                };
                let Some(test) = next else { break };
                let start = Instant::now();
                let result = run_single_test(&root, &src_root, &test, timeout, output_format);
                let dur = start.elapsed();
                let (ok, err, metrics) = match result {
                    Ok(run) => (true, String::new(), run.metrics),
                    Err(msg) => (false, msg, None),
                };
                let _ = tx.send((test.name.clone(), ok, dur, err, metrics));
            }
        }));
    }
    drop(tx);

    let mut ok_count = 0usize;
    let mut fail_count = 0usize;
    let mut durations_ns: Vec<u128> = Vec::new();
    let mut metrics_totals = MetricsTotals::default();
    let mut metrics_count = 0usize;
    for (name, ok, dur, err, metrics) in rx.iter() {
        durations_ns.push(dur.as_nanos());
        if let Some(metrics) = metrics.as_ref() {
            metrics_totals.add(metrics);
            metrics_count += 1;
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
    if !durations_ns.is_empty() {
        durations_ns.sort_unstable();
        let p50 = percentile(&durations_ns, 0.50);
        let p99 = percentile(&durations_ns, 0.99);
        let allocs_per_request = if metrics_count == 0 {
            0.0
        } else {
            metrics_totals.total_allocs() as f64 / metrics_count as f64
        };
        println!(
            "perf: p50_ns={} p99_ns={} allocs/request={:.2}",
            p50, p99, allocs_per_request
        );
        if perf_debug {
            println!(
                "perf-debug: rc_inc={} rc_dec={} mailbox_enqueue_ok={} mailbox_enqueue_fail={} mailbox_dequeue={} mailbox_high_water={} alloc_list={} alloc_map={} alloc_string={} alloc_bytes={} alloc_result={} alloc_pending={} messages_sent={} messages_dropped={} pending_resolved={} pending_dropped={}",
                metrics_totals.rc_inc,
                metrics_totals.rc_dec,
                metrics_totals.mailbox_enqueue_ok,
                metrics_totals.mailbox_enqueue_fail,
                metrics_totals.mailbox_dequeue,
                metrics_totals.mailbox_high_water,
                metrics_totals.alloc_list,
                metrics_totals.alloc_map,
                metrics_totals.alloc_string,
                metrics_totals.alloc_bytes,
                metrics_totals.alloc_result,
                metrics_totals.alloc_pending,
                metrics_totals.messages_sent,
                metrics_totals.messages_dropped,
                metrics_totals.pending_resolved,
                metrics_totals.pending_dropped
            );
        }
    }
    if fail_count == 0 { EXIT_OK } else { EXIT_CODEGEN }
}

fn percentile(samples: &[u128], pct: f64) -> u128 {
    if samples.is_empty() {
        return 0;
    }
    let n = samples.len();
    let rank = (pct * (n as f64 - 1.0)).ceil() as usize;
    samples[rank.min(n - 1)]
}

fn collect_tests(
    root: &Path,
    src_root: &Path,
    tests_root: &Path,
    out: &mut Vec<TestCase>,
) -> io::Result<()> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_tests(&path, src_root, tests_root, out)?;
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
    root: &Path,
    src_root: &Path,
    test: &TestCase,
    timeout: Duration,
    output_format: OutputFormat,
) -> Result<TestRun, String> {
    let temp_dir = root.join("target").join("wrela_tests");
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
    let tests_root = root.join("tests");
    let mir_module = match compile_to_mir_with_root(&entry_path, src_root, Some(&tests_root), output_format) {
        Ok(mir) => mir,
        Err(_) => return Err("compile failed".to_string()),
    };
    if let Err(err) = wrela::backend::cranelift::compile_to_executable(&mir_module, &exe_path) {
        return Err(format!("codegen error: {}", err.0));
    }
    let metrics_path = temp_dir.join(format!("{}_metrics.json", file_stem));
    let _ = fs::remove_file(&metrics_path);
    run_with_timeout(&exe_path, timeout, Some(&metrics_path)).map_err(|e| e)?;
    let metrics = read_metrics_dump(&metrics_path);
    Ok(TestRun { metrics })
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
            (project.module, project.entry_source, entry_path.display().to_string())
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
    let (module, source, source_name) = match hir::project::load_project_with_entrypoint(
        entry_path,
        require_entrypoint,
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
            if missing_run && require_entrypoint && matches!(output_format, OutputFormat::Pretty) {
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
    for warn in semantic.warnings {
        match output_format {
            OutputFormat::Pretty => {
                let report = Report::new(warn)
                    .with_source_code(NamedSource::new(source_name.clone(), source.clone()));
                eprintln!("warning: {report:?}");
            }
            OutputFormat::Json => {
                emit_json_diag(
                    "warning",
                    warn.to_string(),
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
    mir::opt::run_module_passes(&mut mir_module);
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
            let mir_module = match compile_to_mir(
                entry_path,
                output_format,
                emit_mir,
                emit_mir_opt,
                true,
            )
            {
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
