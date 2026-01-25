#![allow(unused_assignments)]

use miette::{Diagnostic, NamedSource, Report, SourceSpan};
use serde::Serialize;
use std::env;
use std::fs;
use std::io::{self, Read};
use std::path::Path;
use thiserror::Error;
use wrela::hir;
use wrela::mir;
use wrela::parser;
use wrela::parser::ast::AstNode;

#[derive(Debug, Error, Diagnostic)]
#[error("{message}")]
struct ProjectDiag {
    message: String,
    #[label("here")]
    span: SourceSpan,
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut emit_mir = false;
    let mut emit_mir_opt = false;
    let mut emit_obj: Option<String> = None;
    let mut emit_bin: Option<String> = None;
    let mut path_arg: Option<String> = None;
    let mut output_format = OutputFormat::Pretty;
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_help();
        return;
    }
    if args.iter().any(|arg| arg == "--version" || arg == "-V") {
        println!("wrela {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if let Some(first) = args.first() {
        if first == "init" {
            let target = args.get(1).map(String::as_str).unwrap_or(".");
            if let Err(err) = init_project(target) {
                eprintln!("init error: {err}");
                std::process::exit(EXIT_USAGE);
            }
            return;
        }
    }
    for arg in args {
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
        path_arg = Some(arg);
    }

    let (module, source, source_name) = match path_arg {
        Some(path) => {
            let entry_path = Path::new(&path);
            match hir::project::load_project(entry_path) {
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
                    if missing_run {
                        if matches!(output_format, OutputFormat::Pretty) {
                            eprintln!(
                                "note: add `to run()` in your entry file to define the program entrypoint"
                            );
                        }
                    }
                    std::process::exit(EXIT_PARSE);
                }
            }
        }
        None => {
            let mut input = String::new();
            if io::stdin().read_to_string(&mut input).is_err() || input.trim().is_empty() {
                print_help();
                std::process::exit(EXIT_USAGE);
            }
            let (node, parse_errors) = parser::parse_with_errors(&input);
            if !parse_errors.is_empty() {
                for err in parse_errors {
                    emit_diag(
                        output_format,
                        "error",
                        err.message,
                        err.span,
                        "<stdin>".to_string(),
                        input.clone(),
                    );
                }
                std::process::exit(EXIT_PARSE);
            }

            let validation_errors = parser::validate::validate(&node);
            if !validation_errors.is_empty() {
                for err in validation_errors {
                    emit_diag(
                        output_format,
                        "error",
                        err.message,
                        err.span,
                        "<stdin>".to_string(),
                        input.clone(),
                    );
                }
                std::process::exit(EXIT_PARSE);
            }

            let root = parser::ast::Root::cast(node).expect("expected root node");
            (hir::lower::lower(root), input, "<stdin>".to_string())
        }
    };

    let mut had_errors = false;
    let semantic = hir::semantic::check_module(&module);
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
        std::process::exit(EXIT_TYPE);
    }

    let mut mir_module = mir::lower::lower_module_with_types(&module, Some(&type_info));
    if emit_mir {
        println!("{:#?}", mir_module);
    }
    for func in &mut mir_module.functions {
        mir::opt::run_function_passes(func);
    }
    if emit_mir_opt {
        println!("{:#?}", mir_module);
    }
    for err in mir::validate::validate_module(&mir_module) {
        eprintln!("mir validation error: {}", err.message);
        had_errors = true;
    }

    if had_errors {
        std::process::exit(EXIT_CODEGEN);
    }

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

    if let Some(path) = emit_bin {
        if let Err(err) =
            wrela::backend::cranelift::compile_to_executable(&mir_module, path.as_ref())
        {
            eprintln!("codegen error: {}", err.0);
            std::process::exit(EXIT_CODEGEN);
        }
    }
}

fn print_help() {
    println!(
        "usage: wrela [options] <path>\n\
\n\
commands:\n\
  init [path]           initialize a new project\n\
\n\
options:\n\
  --emit-mir            emit MIR before optimization\n\
  --emit-mir-opt        emit MIR after optimization\n\
  --emit-obj=PATH       emit object file\n\
  --emit-bin=PATH       emit executable\n\
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

const EXIT_USAGE: i32 = 1;
const EXIT_PARSE: i32 = 2;
const EXIT_TYPE: i32 = 3;
const EXIT_CODEGEN: i32 = 4;
