use std::fs;
use std::process::Command;
use wrela::hir;
use wrela::hir::project::load_project;
use wrela::mir;

fn load_module_from_source(source: &str) -> hir::Module {
    let dir = tempfile::tempdir().expect("tempdir");
    let entry_path = dir.path().join("src").join("main.wr");
    fs::create_dir_all(entry_path.parent().unwrap()).expect("create src dir");
    fs::write(&entry_path, source).expect("write source");
    let project = load_project(&entry_path).expect("load project");
    project.module
}

#[test]
fn native_actor_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
A Counter:
    can add(x: Int) -> Int:
        return x + 1

to run() -> Int:
    c = spawn Counter()
    v = await c.add(41) otherwise 0
    return v
"#;

    let module = load_module_from_source(source);
    let semantic = hir::semantic::check_module(&module);
    assert!(
        semantic.errors.is_empty(),
        "semantic errors: {:?}",
        semantic.errors
    );
    let (type_errors, type_info) = hir::typeck::check_module_with_info(&module);
    assert!(type_errors.is_empty(), "type errors: {type_errors:?}");

    let mut mir_module = mir::lower::lower_module_with_types(&module, Some(&type_info));
    for func in &mut mir_module.functions {
        mir::opt::run_function_passes(func);
    }
    let mir_errors = mir::validate::validate_module(&mir_module);
    assert!(mir_errors.is_empty(), "mir errors: {mir_errors:?}");

    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("wr_smoke");
    wrela::backend::cranelift::compile_to_executable(&mir_module, &out).expect("codegen failed");

    let status = Command::new(&out).status().expect("run failed");
    let expected = ((42 << 3) | 1) & 0xFF;
    assert_eq!(status.code().unwrap_or(-1), expected);
}

#[test]
fn native_numeric_and_range_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
to run() -> Int:
    changing total = 0
    for i in 1...3:
        total += i
    if 1.0 + 2.0 == 3.0:
        total += 10
    if "a" + "b" == "ab":
        total += 100
    return total
"#;

    let module = load_module_from_source(source);
    let semantic = hir::semantic::check_module(&module);
    assert!(
        semantic.errors.is_empty(),
        "semantic errors: {:?}",
        semantic.errors
    );
    let (type_errors, type_info) = hir::typeck::check_module_with_info(&module);
    assert!(type_errors.is_empty(), "type errors: {type_errors:?}");

    let mut mir_module = mir::lower::lower_module_with_types(&module, Some(&type_info));
    for func in &mut mir_module.functions {
        mir::opt::run_function_passes(func);
    }
    let mir_errors = mir::validate::validate_module(&mir_module);
    assert!(mir_errors.is_empty(), "mir errors: {mir_errors:?}");

    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("wr_numeric_range_smoke");
    wrela::backend::cranelift::compile_to_executable(&mir_module, &out).expect("codegen failed");

    let status = Command::new(&out).status().expect("run failed");
    let expected = ((116 << 3) | 1) & 0xFF;
    assert_eq!(status.code().unwrap_or(-1), expected);
}

#[test]
fn native_short_circuit_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
to run() -> Int:
    changing x = 0
    if false and (1 / x == 0):
        return 0
    if true or (1 / x == 0):
        return 1
    return 0
"#;

    let module = load_module_from_source(source);
    let semantic = hir::semantic::check_module(&module);
    assert!(
        semantic.errors.is_empty(),
        "semantic errors: {:?}",
        semantic.errors
    );
    let (type_errors, type_info) = hir::typeck::check_module_with_info(&module);
    assert!(type_errors.is_empty(), "type errors: {type_errors:?}");

    let mut mir_module = mir::lower::lower_module_with_types(&module, Some(&type_info));
    for func in &mut mir_module.functions {
        mir::opt::run_function_passes(func);
    }
    let mir_errors = mir::validate::validate_module(&mir_module);
    assert!(mir_errors.is_empty(), "mir errors: {mir_errors:?}");

    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("wr_short_circuit_smoke");
    wrela::backend::cranelift::compile_to_executable(&mir_module, &out).expect("codegen failed");

    let status = Command::new(&out).status().expect("run failed");
    let expected = ((1 << 3) | 1) & 0xFF;
    assert_eq!(status.code().unwrap_or(-1), expected);
}

#[test]
fn native_actor_control_flow_awaits() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
A Counter:
    can add(x: Int) -> Int:
        return x + 1

to run() -> Int:
    c = spawn Counter()
    changing total = 0
    if true:
        total += await c.add(1) otherwise 0
    else:
        total += await c.add(2) otherwise 0
    for i in 1...2:
        total += await c.add(i) otherwise 0
    return total
"#;

    let module = load_module_from_source(source);
    let semantic = hir::semantic::check_module(&module);
    assert!(
        semantic.errors.is_empty(),
        "semantic errors: {:?}",
        semantic.errors
    );
    let (type_errors, type_info) = hir::typeck::check_module_with_info(&module);
    assert!(type_errors.is_empty(), "type errors: {type_errors:?}");

    let mut mir_module = mir::lower::lower_module_with_types(&module, Some(&type_info));
    for func in &mut mir_module.functions {
        mir::opt::run_function_passes(func);
    }
    let mir_errors = mir::validate::validate_module(&mir_module);
    assert!(mir_errors.is_empty(), "mir errors: {mir_errors:?}");

    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("wr_actor_control_flow");
    wrela::backend::cranelift::compile_to_executable(&mir_module, &out).expect("codegen failed");

    let status = Command::new(&out).status().expect("run failed");
    let expected = ((7 << 3) | 1) & 0xFF;
    assert_eq!(status.code().unwrap_or(-1), expected);
}

#[test]
fn native_class_fields_and_collections() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
A Box:
    has:
        value: Int

to run() -> Int:
    b = Box(value=5)
    l = [1, 2, 3]
    m = {"a": 1, "b": 2}
    if true:
        return b.value
    return 0
"#;

    let module = load_module_from_source(source);
    let semantic = hir::semantic::check_module(&module);
    assert!(
        semantic.errors.is_empty(),
        "semantic errors: {:?}",
        semantic.errors
    );
    let (type_errors, type_info) = hir::typeck::check_module_with_info(&module);
    assert!(type_errors.is_empty(), "type errors: {type_errors:?}");

    let mut mir_module = mir::lower::lower_module_with_types(&module, Some(&type_info));
    for func in &mut mir_module.functions {
        mir::opt::run_function_passes(func);
    }
    let mir_errors = mir::validate::validate_module(&mir_module);
    assert!(mir_errors.is_empty(), "mir errors: {mir_errors:?}");

    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("wr_class_fields");
    wrela::backend::cranelift::compile_to_executable(&mir_module, &out).expect("codegen failed");

    let status = Command::new(&out).status().expect("run failed");
    let expected = ((5 << 3) | 1) & 0xFF;
    assert_eq!(status.code().unwrap_or(-1), expected);
}

#[test]
fn native_builtins_parse_and_io() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("builtins.txt");
    let path_str = path.to_string_lossy().replace('\\', "\\\\");
    let source = format!(
        r#"
to run() -> Int:
    write_file("{path}", "123") otherwise nil
    value = read_file("{path}") otherwise "0"
    parsed = parse_int(value) otherwise 0
    parsed_float = parse_float("2.5") otherwise 0.0
    if parsed == 123 and parsed_float == 2.5:
        return 1
    return 0
"#,
        path = path_str
    );

    let module = load_module_from_source(&source);
    let semantic = hir::semantic::check_module(&module);
    assert!(
        semantic.errors.is_empty(),
        "semantic errors: {:?}",
        semantic.errors
    );
    let (type_errors, type_info) = hir::typeck::check_module_with_info(&module);
    assert!(type_errors.is_empty(), "type errors: {type_errors:?}");

    let mut mir_module = mir::lower::lower_module_with_types(&module, Some(&type_info));
    for func in &mut mir_module.functions {
        mir::opt::run_function_passes(func);
    }
    let mir_errors = mir::validate::validate_module(&mir_module);
    assert!(mir_errors.is_empty(), "mir errors: {mir_errors:?}");

    let out = dir.path().join("wr_builtins");
    wrela::backend::cranelift::compile_to_executable(&mir_module, &out).expect("codegen failed");

    let status = Command::new(&out).status().expect("run failed");
    let expected = ((1 << 3) | 1) & 0xFF;
    assert_eq!(status.code().unwrap_or(-1), expected);
}

#[test]
fn native_actor_match_await() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
A Counter:
    can add(x: Int) -> Int:
        return x + 1

to run() -> Int:
    c = spawn Counter()
    changing total = 0
    match 2:
        1:
            total += await c.add(1) otherwise 0
        2:
            total += await c.add(2) otherwise 0
        otherwise:
            total += 0
    return total
"#;

    let module = load_module_from_source(source);
    let semantic = hir::semantic::check_module(&module);
    assert!(
        semantic.errors.is_empty(),
        "semantic errors: {:?}",
        semantic.errors
    );
    let (type_errors, type_info) = hir::typeck::check_module_with_info(&module);
    assert!(type_errors.is_empty(), "type errors: {type_errors:?}");

    let mut mir_module = mir::lower::lower_module_with_types(&module, Some(&type_info));
    for func in &mut mir_module.functions {
        mir::opt::run_function_passes(func);
    }
    let mir_errors = mir::validate::validate_module(&mir_module);
    assert!(mir_errors.is_empty(), "mir errors: {mir_errors:?}");

    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("wr_actor_match_await");
    wrela::backend::cranelift::compile_to_executable(&mir_module, &out).expect("codegen failed");

    let status = Command::new(&out).status().expect("run failed");
    let expected = ((3 << 3) | 1) & 0xFF;
    assert_eq!(status.code().unwrap_or(-1), expected);
}

#[test]
fn native_class_method_call_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
A Counter:
    has:
        value: Int

    can inc() -> Int:
        return its.value + 1

to run() -> Int:
    c = Counter(value=1)
    return c.inc()
"#;

    let module = load_module_from_source(source);
    let semantic = hir::semantic::check_module(&module);
    assert!(
        semantic.errors.is_empty(),
        "semantic errors: {:?}",
        semantic.errors
    );
    let (type_errors, type_info) = hir::typeck::check_module_with_info(&module);
    assert!(type_errors.is_empty(), "type errors: {type_errors:?}");

    let mut mir_module = mir::lower::lower_module_with_types(&module, Some(&type_info));
    for func in &mut mir_module.functions {
        mir::opt::run_function_passes(func);
    }
    let mir_errors = mir::validate::validate_module(&mir_module);
    assert!(mir_errors.is_empty(), "mir errors: {mir_errors:?}");

    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("wr_class_method_smoke");
    wrela::backend::cranelift::compile_to_executable(&mir_module, &out).expect("codegen failed");

    let status = Command::new(&out).status().expect("run failed");
    let expected = ((2 << 3) | 1) & 0xFF;
    assert_eq!(status.code().unwrap_or(-1), expected);
}

#[test]
fn native_actor_fire_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
A Counter:
    can add(x: Int) -> Int:
        return x + 1

to run() -> Int:
    c = spawn Counter()
    fire c.add(1)
    v = await c.add(41) otherwise 0
    return v
"#;

    let module = load_module_from_source(source);
    let semantic = hir::semantic::check_module(&module);
    assert!(
        semantic.errors.is_empty(),
        "semantic errors: {:?}",
        semantic.errors
    );
    let (type_errors, type_info) = hir::typeck::check_module_with_info(&module);
    assert!(type_errors.is_empty(), "type errors: {type_errors:?}");

    let mut mir_module = mir::lower::lower_module_with_types(&module, Some(&type_info));
    for func in &mut mir_module.functions {
        mir::opt::run_function_passes(func);
    }
    let mir_errors = mir::validate::validate_module(&mir_module);
    assert!(mir_errors.is_empty(), "mir errors: {mir_errors:?}");

    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("wr_actor_fire_smoke");
    wrela::backend::cranelift::compile_to_executable(&mir_module, &out).expect("codegen failed");

    let status = Command::new(&out).status().expect("run failed");
    let expected = ((42 << 3) | 1) & 0xFF;
    assert_eq!(status.code().unwrap_or(-1), expected);
}

#[test]
fn native_result_otherwise_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
to fail(flag: Bool) -> Result:
    if flag:
        return err "nope"
    return 7

to run() -> Int:
    v = fail(true) otherwise 5
    return v
"#;

    let module = load_module_from_source(source);
    let semantic = hir::semantic::check_module(&module);
    assert!(
        semantic.errors.is_empty(),
        "semantic errors: {:?}",
        semantic.errors
    );
    let (type_errors, type_info) = hir::typeck::check_module_with_info(&module);
    assert!(type_errors.is_empty(), "type errors: {type_errors:?}");

    let mut mir_module = mir::lower::lower_module_with_types(&module, Some(&type_info));
    for func in &mut mir_module.functions {
        mir::opt::run_function_passes(func);
    }
    let mir_errors = mir::validate::validate_module(&mir_module);
    assert!(mir_errors.is_empty(), "mir errors: {mir_errors:?}");

    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("wr_result_otherwise_smoke");
    wrela::backend::cranelift::compile_to_executable(&mir_module, &out).expect("codegen failed");

    let status = Command::new(&out).status().expect("run failed");
    let expected = ((5 << 3) | 1) & 0xFF;
    assert_eq!(status.code().unwrap_or(-1), expected);
}

#[test]
fn native_crash_exits() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
to run() -> Int:
    crash("boom")
    return 0
"#;

    let module = load_module_from_source(source);
    let semantic = hir::semantic::check_module(&module);
    assert!(
        semantic.errors.is_empty(),
        "semantic errors: {:?}",
        semantic.errors
    );
    let (type_errors, type_info) = hir::typeck::check_module_with_info(&module);
    assert!(type_errors.is_empty(), "type errors: {type_errors:?}");

    let mut mir_module = mir::lower::lower_module_with_types(&module, Some(&type_info));
    for func in &mut mir_module.functions {
        mir::opt::run_function_passes(func);
    }
    let mir_errors = mir::validate::validate_module(&mir_module);
    assert!(mir_errors.is_empty(), "mir errors: {mir_errors:?}");

    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("wr_crash");
    wrela::backend::cranelift::compile_to_executable(&mir_module, &out).expect("codegen failed");

    let status = Command::new(&out).status().expect("run failed");
    assert!(!status.success(), "expected crash to exit non-zero");
}
