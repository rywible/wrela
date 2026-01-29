#![allow(unused_assignments)]

use miette::{Diagnostic, NamedSource, Report, SourceSpan};
use serde::Serialize;
use std::env;
use std::fs;
use std::io;
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
            let result = compile_to_mir(
                &entry_path,
                output_format,
                emit_mir,
                emit_mir_opt,
            );
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
            let mir_module = match compile_to_mir(
                &entry_path,
                output_format,
                emit_mir,
                emit_mir_opt,
            ) {
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
            if let Err(err) = wrela::backend::cranelift::compile_to_executable(&mir_module, output.as_ref()) {
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
            let mir_module = match compile_to_mir(
                &entry_path,
                output_format,
                emit_mir,
                emit_mir_opt,
            ) {
                Ok(mir) => mir,
                Err(code) => std::process::exit(code),
            };
            let output = out_path.unwrap_or_else(temp_exe_path);
            if let Err(err) = wrela::backend::cranelift::compile_to_executable(&mir_module, output.as_ref()) {
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
            run_dev_loop(&entry_path, poll, output_format, emit_mir, emit_mir_opt, &program_args);
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
\n\
options:\n\
  --prefix PATH         install/update prefix (default: $PREFIX or ~/.local/wrela)\n\
  -o, --out PATH        output path for build/run\n\
  --emit-mir            emit MIR before optimization\n\
  --emit-mir-opt        emit MIR after optimization\n\
  --emit-obj=PATH       emit object file\n\
  --emit-bin=PATH       emit executable\n\
  --poll-ms=N           poll interval for dev (default: 500)\n\
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
    fs::write(main_path, "to run() -> Int:\n    return 0\n")?;
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
    matches!(arg, "init" | "update" | "check" | "build" | "compile" | "run" | "dev")
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
        return Err(format!(
            "no entry file found at {}",
            entry.display()
        ));
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
    let (module, source, source_name) = match hir::project::load_project(entry_path) {
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
            if missing_run && matches!(output_format, OutputFormat::Pretty) {
                eprintln!("note: add `to run()` in your entry file to define the program entrypoint");
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
    for func in &mut mir_module.functions {
        mir::opt::run_function_passes(func);
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
    let src_root = find_src_root(entry_path).unwrap_or_else(|| {
        entry_path.parent().unwrap_or(entry_path).to_path_buf()
    });
    eprintln!(
        "dev: watching {} (poll {}ms)",
        src_root.display(),
        poll_ms
    );
    let mut last = snapshot_sources(&src_root);
    let mut child: Option<std::process::Child> = None;
    loop {
        if sources_changed(&src_root, &mut last) {
            if let Some(mut running) = child.take() {
                let _ = running.kill();
                let _ = running.wait();
            }
            let mir_module = match compile_to_mir(entry_path, output_format, emit_mir, emit_mir_opt) {
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
            if let Err(err) = wrela::backend::cranelift::compile_to_executable(&mir_module, output.as_ref()) {
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
    matches!(path.extension().and_then(|s| s.to_str()), Some("wr") | Some("sp"))
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
    let url = format!(
        "https://github.com/rywible/wrela/releases/download/{tag}/wrela-{target}.tar.gz"
    );

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
    curl.args(["-fsSL", "-o"])
        .arg(&tmp_path)
        .arg(&url);
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
    tar.args(["-xzf"])
        .arg(&tmp_path)
        .arg("-C")
        .arg(&prefix);
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
    curl.args(["-fsSL", "https://api.github.com/repos/rywible/wrela/releases"]);
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
const EXIT_CODEGEN: i32 = 4;
