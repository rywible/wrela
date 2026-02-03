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

fn expected_int_exit(val: i64) -> i32 {
    (val as i32) & 0xFF
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
    optimize balance:
        c = detach Counter() * 1
        v = await c.add(41) otherwise 0
        return v
"#;

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

    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("wr_smoke");
    wrela::backend::cranelift::compile_to_executable(&mir_module, &out).expect("codegen failed");

    let status = Command::new(&out).status().expect("run failed");
    let expected = expected_int_exit(42);
    assert_eq!(status.code().unwrap_or(-1), expected);
}

#[test]
fn native_numeric_and_range_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
to run() -> Int:
    mutable total = 0
    for i in 1...3:
        total += i
    if 1.0 + 2.0 == 3.0:
        total += 10
    if "a" + "b" == "ab":
        total += 100
    return total
"#;

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

    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("wr_numeric_range_smoke");
    wrela::backend::cranelift::compile_to_executable(&mir_module, &out).expect("codegen failed");

    let status = Command::new(&out).status().expect("run failed");
    let expected = expected_int_exit(116);
    assert_eq!(status.code().unwrap_or(-1), expected);
}

#[test]
fn native_short_circuit_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
to run() -> Int:
    mutable x = 0
    if false and (1 / x == 0):
        return 0
    if true or (1 / x == 0):
        return 1
    return 0
"#;

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

    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("wr_short_circuit_smoke");
    wrela::backend::cranelift::compile_to_executable(&mir_module, &out).expect("codegen failed");

    let status = Command::new(&out).status().expect("run failed");
    let expected = expected_int_exit(1);
    assert_eq!(status.code().unwrap_or(-1), expected);
}

#[test]
fn native_logger_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
use Logger from log

to run() -> Int:
    Logger.log_info("boot")
    Logger.log_warning("warn")
    Logger.log_error_with("err", { "code": 7 })
    return 1
"#;

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

    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("wr_logger_smoke");
    wrela::backend::cranelift::compile_to_executable(&mir_module, &out).expect("codegen failed");

    let status = Command::new(&out).status().expect("run failed");
    let expected = expected_int_exit(1);
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
    optimize balance:
        c = detach Counter() * 1
        mutable total = 0
        if true:
            total += await c.add(1) otherwise 0
        otherwise:
            total += await c.add(2) otherwise 0
        for i in 1...2:
            total += await c.add(i) otherwise 0
        return total
"#;

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

    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("wr_actor_control_flow");
    wrela::backend::cranelift::compile_to_executable(&mir_module, &out).expect("codegen failed");

    let status = Command::new(&out).status().expect("run failed");
    let expected = expected_int_exit(7);
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

    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("wr_class_fields");
    wrela::backend::cranelift::compile_to_executable(&mir_module, &out).expect("codegen failed");

    let status = Command::new(&out).status().expect("run failed");
    let expected = expected_int_exit(5);
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
    __wr_write_file("{path}", "123") otherwise nil
    value = __wr_read_file("{path}") otherwise "0"
    parsed = __wr_parse_int(value) otherwise 0
    parsed_float = __wr_parse_float("2.5") otherwise 0.0
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
    let expected = expected_int_exit(1);
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
    optimize balance:
        c = detach Counter() * 1
        mutable total = 0
        match 2:
            1:
                total += await c.add(1) otherwise 0
            2:
                total += await c.add(2) otherwise 0
            otherwise:
                total += 0
        return total
"#;

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

    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("wr_actor_match_await");
    wrela::backend::cranelift::compile_to_executable(&mir_module, &out).expect("codegen failed");

    let status = Command::new(&out).status().expect("run failed");
    let expected = expected_int_exit(3);
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
    let expected = expected_int_exit(2);
    assert_eq!(status.code().unwrap_or(-1), expected);
}

#[test]
fn native_member_assign_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
A Counter:
    has:
        value: Int

    can add(delta: Int) -> Nothing:
        its.value += delta

to run() -> Int:
    c = Counter(value=1)
    c.add(2)
    return c.value
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
    let out = dir.path().join("wr_member_assign_smoke");
    wrela::backend::cranelift::compile_to_executable(&mir_module, &out).expect("codegen failed");

    let status = Command::new(&out).status().expect("run failed");
    let expected = expected_int_exit(3);
    assert_eq!(status.code().unwrap_or(-1), expected);
}

#[test]
fn native_pool_round_robin_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
use:
    size,
    round_robin
from pool

A Counter:
    can ping(x: Int) -> Int:
        return x

to run() -> Int:
    optimize balance:
        c = detach Counter() * 2
        v1 = await c.ping(1) otherwise 0
        v2 = await c.ping(1) otherwise 0
        v3 = await c.ping(1) otherwise 0
        v4 = await c.ping(1) otherwise 0
        pool_count = size(c)
        rr = round_robin(c)
        return pool_count * 10 + rr
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
    let out = dir.path().join("wr_pool_rr_smoke");
    wrela::backend::cranelift::compile_to_executable(&mir_module, &out).expect("codegen failed");

    let output = Command::new(&out).output().expect("run failed");
    let expected = expected_int_exit(24);
    match output.status.code() {
        Some(code) => assert_eq!(code, expected),
        None => {
            use std::os::unix::process::ExitStatusExt;
            panic!(
                "process terminated by signal {:?}: {}",
                output.status.signal(),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

#[test]
fn native_pool_auto_size_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
use size from pool

A Counter:
    can ping(x: Int) -> Int:
        return x

to run() -> Int:
    optimize balance:
        c = detach Counter() * n
        return size(c)
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
    let out = dir.path().join("wr_pool_auto_smoke");
    wrela::backend::cranelift::compile_to_executable(&mir_module, &out).expect("codegen failed");

    let output = Command::new(&out)
        .env("WRELA_POOL_AUTO_MIN", "3")
        .env("WRELA_POOL_AUTO_MAX", "3")
        .output()
        .expect("run failed");
    let expected = expected_int_exit(3);
    match output.status.code() {
        Some(code) => assert_eq!(code, expected),
        None => {
            use std::os::unix::process::ExitStatusExt;
            panic!(
                "process terminated by signal {:?}: {}",
                output.status.signal(),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

#[test]
fn native_pool_mailbox_len_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
use queue_len from pool
use:
    mailbox_len,
    pause,
    pause_wait,
    resume
from actor

A Counter:
    can ping(x: Int) -> Int:
        return x

to run() -> Int:
    optimize balance:
        c = detach Counter() * 2
        pause(c)
        pause_wait(c)
        fire c.ping(1)
        len = queue_len(c) + mailbox_len(c)
        resume(c)
        if len >= 1:
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
    let out = dir.path().join("wr_pool_mailbox_len_smoke");
    wrela::backend::cranelift::compile_to_executable(&mir_module, &out).expect("codegen failed");

    let output = Command::new(&out).output().expect("run failed");
    let expected = expected_int_exit(1);
    match output.status.code() {
        Some(code) => assert_eq!(code, expected),
        None => {
            use std::os::unix::process::ExitStatusExt;
            panic!(
                "process terminated by signal {:?}: {}",
                output.status.signal(),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

#[test]
fn native_pool_pause_drop_metrics_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
use:
    pause,
    pause_wait,
    resume
from actor
use:
    get,
    dropped_paused_id
from metrics

A Counter:
    can ping(x: Int) -> Int:
        return x

to run() -> Int:
    optimize balance:
        c = detach Counter() * 2
        pause(c)
        pause_wait(c)
        for i in 1...4:
            fire c.ping(i)
        dropped = get(dropped_paused_id())
        resume(c)
        if dropped > 0:
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
    let out = dir.path().join("wr_pool_pause_drop_smoke");
    wrela::backend::cranelift::compile_to_executable(&mir_module, &out).expect("codegen failed");

    let output = Command::new(&out)
        .env("WRELA_PAUSE_QUEUE_CAP", "1")
        .output()
        .expect("run failed");
    let expected = expected_int_exit(1);
    match output.status.code() {
        Some(code) => assert_eq!(code, expected),
        None => {
            use std::os::unix::process::ExitStatusExt;
            panic!(
                "process terminated by signal {:?}: {}",
                output.status.signal(),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

#[test]
fn native_pool_backpressure_config_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = format!(
        r#"
use size from pool

A Counter:
    can ping(x: Int) -> Int:
        return x

to run() -> Int:
    optimize balance:
        c = detach Pool.of(Counter, size=1, backpressure=queue(1)) * 1
        return size(c)
"#
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

    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("wr_pool_backpressure_drop_smoke");
    wrela::backend::cranelift::compile_to_executable(&mir_module, &out).expect("codegen failed");

    let output = Command::new(&out).output().expect("run failed");
    let expected = expected_int_exit(1);
    match output.status.code() {
        Some(code) => assert_eq!(code, expected),
        None => {
            use std::os::unix::process::ExitStatusExt;
            panic!(
                "process terminated by signal {:?}: {}",
                output.status.signal(),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

#[test]
fn native_pool_backpressure_stress() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    if std::env::var("WR_STRESS_POOL_BP").is_err() {
        return;
    }
    let source = r#"
use:
    pause,
    pause_wait,
    resume
from actor
use:
    get,
    messages_dropped_id
from metrics

A Counter:
    can ping(x: Int) -> Int:
        return x

to run() -> Int:
    optimize balance:
        c = detach Pool.of(Counter, size=1, backpressure=queue(1)) * 1
        pause(c)
        pause_wait(c)
        for i in 1...400:
            fire c.ping(i)
        dropped = get(messages_dropped_id())
        resume(c)
        if dropped > 0:
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
    let out = dir.path().join("wr_pool_backpressure_stress");
    wrela::backend::cranelift::compile_to_executable(&mir_module, &out).expect("codegen failed");

    let output = Command::new(&out).output().expect("run failed");
    let expected = expected_int_exit(1);
    match output.status.code() {
        Some(code) => assert_eq!(code, expected),
        None => {
            use std::os::unix::process::ExitStatusExt;
            panic!(
                "process terminated by signal {:?}: {}",
                output.status.signal(),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

#[test]
fn native_pool_of_overrides_tail_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
use size from pool

A Counter:
    can ping(x: Int) -> Int:
        return x

to run() -> Int:
    optimize balance:
        c = detach Pool.of(Counter, size=3) * 1
        return size(c)
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
    let out = dir.path().join("wr_pool_of_override_smoke");
    wrela::backend::cranelift::compile_to_executable(&mir_module, &out).expect("codegen failed");

    let output = Command::new(&out).output().expect("run failed");
    let expected = expected_int_exit(3);
    match output.status.code() {
        Some(code) => assert_eq!(code, expected),
        None => {
            use std::os::unix::process::ExitStatusExt;
            panic!(
                "process terminated by signal {:?}: {}",
                output.status.signal(),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

#[test]
fn native_pool_auto_bounds_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
use size from pool

A Counter:
    can ping(x: Int) -> Int:
        return x

to run() -> Int:
    optimize balance:
        c = detach Pool.of(Counter, size=n, min=2, max=2) * 1
        return size(c)
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
    let out = dir.path().join("wr_pool_auto_bounds_smoke");
    wrela::backend::cranelift::compile_to_executable(&mir_module, &out).expect("codegen failed");

    let output = Command::new(&out).output().expect("run failed");
    let expected = expected_int_exit(2);
    match output.status.code() {
        Some(code) => assert_eq!(code, expected),
        None => {
            use std::os::unix::process::ExitStatusExt;
            panic!(
                "process terminated by signal {:?}: {}",
                output.status.signal(),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
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
    optimize balance:
        c = detach Counter() * 1
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
    let expected = expected_int_exit(42);
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
    let expected = expected_int_exit(5);
    assert_eq!(status.code().unwrap_or(-1), expected);
}

#[test]
fn native_enum_match_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
A Status is either:
    Pending
    Processing(worker_id: Int)

to run() -> Int:
    status = Status.Processing(worker_id=7)
    match status:
        Status.Processing(id): return id
        Status.Pending: return 0
        otherwise: return 1
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
    let out = dir.path().join("wr_enum_match_smoke");
    wrela::backend::cranelift::compile_to_executable(&mir_module, &out).expect("codegen failed");

    let status = Command::new(&out).status().expect("run failed");
    let expected = expected_int_exit(7);
    assert_eq!(status.code().unwrap_or(-1), expected);
}

#[test]
fn native_result_match_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
to maybe(ok: Bool) -> Result[Int, Int]:
    if ok:
        return 1
    return err 9

to run() -> Int:
    match maybe(ok=false):
        Ok(v): return v
        Err(e): return e
        otherwise: return 0
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
    let out = dir.path().join("wr_result_match_smoke");
    wrela::backend::cranelift::compile_to_executable(&mir_module, &out).expect("codegen failed");

    let status = Command::new(&out).status().expect("run failed");
    let expected = expected_int_exit(9);
    assert_eq!(status.code().unwrap_or(-1), expected);
}

#[test]
fn native_generic_class_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
A Box[T]:
    has:
        value: T

to run() -> Int:
    b = Box[Int](value=3)
    return b.value
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
    let out = dir.path().join("wr_generic_class_smoke");
    wrela::backend::cranelift::compile_to_executable(&mir_module, &out).expect("codegen failed");

    let status = Command::new(&out).status().expect("run failed");
    let expected = expected_int_exit(3);
    assert_eq!(status.code().unwrap_or(-1), expected);
}

#[test]
fn native_interface_dispatch_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
A Printable:
    must show() -> String

A Foo:
    is a Printable
    can show() -> String:
        return "foo"

A Bar:
    is a Printable
    can show() -> String:
        return "bar"

to show(p: Printable) -> Int:
    if p.show() == "foo":
        return 1
    return 2

to run() -> Int:
    return show(p=Bar())
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
    let out = dir.path().join("wr_interface_dispatch_smoke");
    wrela::backend::cranelift::compile_to_executable(&mir_module, &out).expect("codegen failed");

    let status = Command::new(&out).status().expect("run failed");
    let expected = expected_int_exit(2);
    assert_eq!(status.code().unwrap_or(-1), expected);
}

#[test]
fn native_defer_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
A Counter:
    has:
        value: Int
    can add(delta: Int) -> Nothing:
        its.value += delta

to bump(counter: Counter) -> Nothing:
    defer counter.add(1)
    defer counter.add(2)
    return

to run() -> Int:
    c = Counter(value=0)
    bump(counter=c)
    return c.value
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
    let out = dir.path().join("wr_defer_smoke");
    wrela::backend::cranelift::compile_to_executable(&mir_module, &out).expect("codegen failed");

    let status = Command::new(&out).status().expect("run failed");
    let expected = expected_int_exit(3);
    assert_eq!(status.code().unwrap_or(-1), expected);
}

#[test]
fn native_defer_nested_and_early_return() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
A Counter:
    has:
        value: Int
    can add(delta: Int) -> Nothing:
        its.value += delta

to bump(counter: Counter, flip: Bool) -> Nothing:
    defer counter.add(1)
    if flip:
        defer counter.add(2)
        return
    defer counter.add(4)
    return

to run() -> Int:
    c = Counter(value=0)
    bump(counter=c, flip=true)
    bump(counter=c, flip=false)
    return c.value
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
    let out = dir.path().join("wr_defer_nested");
    wrela::backend::cranelift::compile_to_executable(&mir_module, &out).expect("codegen failed");

    let status = Command::new(&out).status().expect("run failed");
    let expected = expected_int_exit(10);
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
