use std::fs;
use std::process::Command;
use wrela::hir;
use wrela::hir::project::load_project;
use wrela::mir;

fn load_module_from_source(source: &str) -> hir::Module {
    let dir = tempfile::tempdir().expect("tempdir");
    let entry_path = dir.path().join("src").join("main.wr");
    fs::create_dir_all(entry_path.parent().expect("src parent")).expect("create src dir");
    fs::write(&entry_path, source).expect("write source");
    let project = load_project(&entry_path).expect("load project");
    project.module
}

fn expected_int_exit(value: i64) -> i32 {
    (value as i32) & 0xFF
}

fn compile_and_run_native_source(source: &str, executable_name: &str) -> std::process::Output {
    let module = load_module_from_source(source);
    let semantic = hir::semantic::check_module(&module);
    assert!(
        semantic.errors.is_empty(),
        "semantic errors: {:?}",
        semantic.errors
    );
    let (type_errors, type_info) = hir::typeck::check_module_with_info(&module);
    assert!(type_errors.is_empty(), "type errors: {type_errors:?}");

    let check_ir = hir::checkir::extract_module(&module);
    let mut mir_module = mir::lower::lower_module_with_types(&module, &type_info);
    let analysis = mir::analysis::analyze_module(&mir_module);
    for func in &mut mir_module.functions {
        let types = analysis.type_map.function(&func.name);
        mir::opt::run_function_passes_with_types(func, types);
    }
    let _ = mir::opt::run_module_passes_with_rulepack(&mut mir_module, Some(&check_ir));
    let mir_errors = mir::validate::validate_module(&mir_module);
    assert!(mir_errors.is_empty(), "mir errors: {mir_errors:?}");

    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join(executable_name);
    wrela::backend::cranelift::compile_to_executable(&mir_module, &out).expect("codegen failed");
    Command::new(&out).output().expect("run failed")
}

#[test]
fn native_v2_numeric_range_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
fn run() -> Integer {
    mutable total = 0
    for i in 1...3 {
        total += i
    }
    if "a" + "b" == "ab" {
        total += 100
    }
    return total
}
"#;

    let output = compile_and_run_native_source(source, "wr_v2_numeric_range_smoke");
    let expected = expected_int_exit(106);
    assert_eq!(output.status.code().unwrap_or(-1), expected);
}

#[test]
fn native_v2_result_fallback_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
fn try_read(flag: Boolean) -> Result[Integer] {
    if flag {
        return 9
    }
    return error "bad"
}

fn run() -> Integer {
    return try_read(flag=false) ?? 5
}
"#;

    let output = compile_and_run_native_source(source, "wr_v2_result_fallback_smoke");
    let expected = expected_int_exit(5);
    assert_eq!(output.status.code().unwrap_or(-1), expected);
}
