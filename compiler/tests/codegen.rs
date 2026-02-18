use std::fs;
use std::os::unix::process::ExitStatusExt;
use std::process::Command;
use wrela::hir;
use wrela::hir::project::load_project;
use wrela::mir;
use wrela::mir::analysis;
use wrela::mir::ir::{
    AllocKind, BasicBlock, BlockId, CallKind, CallTarget, Local, MirFunction, MirType, Place,
    Rvalue, Stmt, Temp, TempId, Terminator, Value,
};

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

fn optimize_mir_module(
    mir_module: &mut mir::ir::MirModule,
    check_ir: Option<&hir::checkir::CheckIrModule>,
) {
    // Keep codegen tests aligned with the real CLI pipeline, otherwise we end up "testing" a
    // non-production compiler configuration (and getting misleading failures).
    let analysis = mir::analysis::analyze_module(mir_module);
    for func in &mut mir_module.functions {
        let types = analysis.type_map.function(&func.name);
        mir::opt::run_function_passes_with_types(func, types);
    }
    let _ = mir::opt::run_module_passes_with_rulepack(mir_module, check_ir);
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
    optimize_mir_module(&mut mir_module, Some(&check_ir));
    let mir_errors = mir::validate::validate_module(&mir_module);
    assert!(mir_errors.is_empty(), "mir errors: {mir_errors:?}");

    let keep_native = std::env::var("WR_KEEP_NATIVE_BIN").is_ok();
    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join(executable_name);
    wrela::backend::cranelift::compile_to_executable(&mir_module, &out).expect("codegen failed");
    if keep_native {
        eprintln!("WR_KEEP_NATIVE_BIN={}", out.display());
        std::mem::forget(dir);
    }

    Command::new(&out).output().expect("run failed")
}

#[test]
fn native_actor_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
A Counter:
    can add(x: Integer) -> Integer:
        return x + 1

to run() -> Integer:
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

    let check_ir = hir::checkir::extract_module(&module);
    let mut mir_module = mir::lower::lower_module_with_types(&module, &type_info);
    optimize_mir_module(&mut mir_module, Some(&check_ir));
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
to run() -> Integer:
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

    let check_ir = hir::checkir::extract_module(&module);
    let mut mir_module = mir::lower::lower_module_with_types(&module, &type_info);
    optimize_mir_module(&mut mir_module, Some(&check_ir));
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
fn typed_integer_range_for_uses_mir_fast_path() {
    let source = r#"
to run() -> Integer:
    start = 1
    stop = 4
    mutable total = 0
    for i in start...stop:
        total += i
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

    let mut mir_module = mir::lower::lower_module_with_types(&module, &type_info);
    for func in &mut mir_module.functions {
        mir::opt::run_function_passes(func);
    }
    let run = mir_module
        .functions
        .iter()
        .find(|func| func.name.as_str() == "run")
        .expect("missing run");

    assert!(
        run.locals
            .iter()
            .any(|local| local.name.as_str() == "i" && local.ty == MirType::Integer),
        "expected integer-typed loop variable"
    );
    assert!(
        run.locals
            .iter()
            .any(|local| local.name.starts_with("$range_idx") && local.ty == MirType::Integer),
        "expected integer-typed induction variable"
    );

    for block in &run.blocks {
        for stmt in &block.stmts {
            assert!(
                !matches!(stmt, Stmt::IterInit { .. } | Stmt::IterNext { .. }),
                "typed range fast path should not lower through iterator protocol"
            );
            if let Stmt::Assign { value, .. } = stmt {
                assert!(
                    !matches!(
                        value,
                        Rvalue::Binary {
                            op: hir::BinaryOp::Range,
                            ..
                        }
                    ),
                    "typed range fast path should not materialize range values"
                );
            }
        }
    }
}

#[test]
fn integer_tight_loop_has_no_rc_traffic_in_mir() {
    let source = r#"
to run() -> Integer:
    mutable total = 0
    for i in 1...500:
        total = total + i
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

    let mut mir_module = mir::lower::lower_module_with_types(&module, &type_info);
    for func in &mut mir_module.functions {
        mir::opt::run_function_passes(func);
    }
    let run = mir_module
        .functions
        .iter()
        .find(|func| func.name.as_str() == "run")
        .expect("missing run");

    assert!(
        run.blocks
            .iter()
            .flat_map(|block| block.stmts.iter())
            .all(|stmt| !matches!(stmt, Stmt::RcInc { .. } | Stmt::RcDec { .. })),
        "typed integer loop should not emit retain/release traffic"
    );
}

#[test]
fn check_given_boolean_lane_has_no_rc_traffic() {
    let source = r#"
check is_positive(value: Integer) -> Boolean:
    return value > 0

to run() -> Integer:
    a = is_positive given 3
    b = is_positive given -1
    if a:
        if not b:
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

    let mut mir_module = mir::lower::lower_module_with_types(&module, &type_info);
    for func in &mut mir_module.functions {
        mir::opt::run_function_passes(func);
    }
    let run = mir_module
        .functions
        .iter()
        .find(|func| func.name.as_str() == "run")
        .expect("missing run");
    let has_rc = run
        .blocks
        .iter()
        .flat_map(|block| block.stmts.iter())
        .any(|stmt| matches!(stmt, Stmt::RcInc { .. } | Stmt::RcDec { .. }));
    assert!(!has_rc, "check/given boolean lane should remain scalar");
}

#[test]
fn result_otherwise_from_call_keeps_guarded_unwrap() {
    let source = r#"
to try_to_read(flag: Boolean) -> Result[Integer]:
    if flag:
        return 9
    return error "bad"

to run() -> Integer:
    return try_to_read(false) otherwise 5
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

    let mut mir_module = mir::lower::lower_module_with_types(&module, &type_info);
    for func in &mut mir_module.functions {
        mir::opt::run_function_passes(func);
    }
    let run = mir_module
        .functions
        .iter()
        .find(|func| func.name.as_str() == "run")
        .expect("missing run");
    let mut saw_result_is_ok = false;
    let mut saw_result_unwrap = false;
    for stmt in run.blocks.iter().flat_map(|block| block.stmts.iter()) {
        if let Stmt::Assign { value, .. } = stmt {
            match value {
                Rvalue::ResultIsOk { .. } => saw_result_is_ok = true,
                Rvalue::ResultUnwrap { .. } => saw_result_unwrap = true,
                _ => {}
            }
        }
    }
    assert!(saw_result_is_ok, "otherwise should guard with ResultIsOk");
    assert!(
        saw_result_unwrap,
        "unknown call result should keep guarded unwrap"
    );
}

#[test]
fn native_short_circuit_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
to run() -> Integer:
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

    let mut mir_module = mir::lower::lower_module_with_types(&module, &type_info);
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
fn native_enum_zero_arg_variant_value_and_exhaustive_match_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
A Status is either:
    Pending
    Done

to run() -> Integer:
    s = Status.Pending
    match s:
        Status.Pending: return 1
        Status.Done: return 2
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

    let mut mir_module = mir::lower::lower_module_with_types(&module, &type_info);
    for func in &mut mir_module.functions {
        mir::opt::run_function_passes(func);
    }
    let mir_errors = mir::validate::validate_module(&mir_module);
    assert!(mir_errors.is_empty(), "mir errors: {mir_errors:?}");

    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("wr_enum_zero_arg_smoke");
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

to run() -> Integer:
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

    let mut mir_module = mir::lower::lower_module_with_types(&module, &type_info);
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
fn mir_alloc_annotations_preserved() {
    let temp_escape = TempId(0);
    let temp_local = TempId(1);
    let block = BasicBlock {
        stmts: vec![
            Stmt::Assign {
                place: Place::Temp(temp_escape),
                value: Rvalue::BuildList {
                    items: Vec::new(),
                    alloc: AllocKind::LocalTemp,
                },
                span: rowan::TextRange::new(0.into(), 0.into()),
            },
            Stmt::Assign {
                place: Place::Temp(temp_local),
                value: Rvalue::BuildList {
                    items: Vec::new(),
                    alloc: AllocKind::LocalTemp,
                },
                span: rowan::TextRange::new(0.into(), 0.into()),
            },
            Stmt::RcInc {
                value: Value::Temp(temp_local),
                span: rowan::TextRange::new(0.into(), 0.into()),
            },
        ],
        terminator: Terminator::Return {
            value: Some(Value::Temp(temp_escape)),
            span: rowan::TextRange::new(0.into(), 0.into()),
        },
    };
    let mut func = MirFunction {
        name: "allocs".into(),
        params: Vec::new(),
        locals: vec![Local {
            name: "x".into(),
            mutable: false,
            ty: MirType::Unknown,
        }],
        temps: vec![
            Temp {
                ty: MirType::Unknown,
            },
            Temp {
                ty: MirType::Unknown,
            },
        ],
        blocks: vec![block],
        entry: BlockId(0),
        suspendable: false,
    };

    mir::opt::run_function_passes(&mut func);

    let mut allocs = Vec::new();
    for stmt in &func.blocks[0].stmts {
        if let Stmt::Assign { value, .. } = stmt {
            if let Rvalue::BuildList { alloc, .. } = value {
                allocs.push(*alloc);
            }
        }
    }
    assert!(allocs.contains(&AllocKind::Escaping));
    assert!(allocs.contains(&AllocKind::LocalTemp));
}

#[test]
fn non_escaping_local_temp_avoids_rc_traffic() {
    let span = rowan::TextRange::new(0.into(), 0.into());
    let temp_local = TempId(0);
    let mut func = MirFunction {
        name: "alloc_local_temp".into(),
        params: Vec::new(),
        locals: Vec::new(),
        temps: vec![Temp {
            ty: MirType::Unknown,
        }],
        blocks: vec![BasicBlock {
            stmts: vec![Stmt::Assign {
                place: Place::Temp(temp_local),
                value: Rvalue::BuildList {
                    items: Vec::new(),
                    alloc: AllocKind::LocalTemp,
                },
                span,
            }],
            terminator: Terminator::Return { value: None, span },
        }],
        entry: BlockId(0),
        suspendable: false,
    };

    mir::opt::run_function_passes(&mut func);

    let has_rc = func
        .blocks
        .iter()
        .flat_map(|b| &b.stmts)
        .any(|stmt| matches!(stmt, Stmt::RcInc { .. } | Stmt::RcDec { .. }));
    assert!(
        !has_rc,
        "local temp allocation should not gain RC bookkeeping"
    );
}

#[test]
fn call_graph_and_type_map_smoke() {
    let source = r#"
to g(x: Integer) -> Integer:
    return x + 1

to f() -> Integer:
    return g(1)

to run() -> Integer:
    return f()
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

    let mir_module = mir::lower::lower_module_with_types(&module, &type_info);
    let analysis = analysis::analyze_module(&mir_module);

    let g_name = "g".into();
    let f_name = "f".into();
    let f_edges = analysis.call_graph.edges(&f_name);
    assert!(f_edges.iter().any(|name| name.as_str() == "g"));

    let g_func = mir_module
        .functions
        .iter()
        .find(|f| f.name.as_str() == "g")
        .expect("missing g");
    let g_types = analysis
        .type_map
        .function(&g_name)
        .expect("missing g types");
    let param = g_func.params.first().expect("missing param");
    let param_ty = g_types
        .locals
        .get(param.0)
        .cloned()
        .unwrap_or(MirType::Unknown);
    assert_eq!(param_ty, MirType::Integer);
}

#[test]
fn mir_ssa_phi_inserted() {
    let source = r#"
to run() -> Integer:
    mutable x = 1
    mutable y = 0
    if x > 0:
        y += 1
    otherwise:
        y += 2
    return y
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

    let mut mir_module = mir::lower::lower_module_with_types(&module, &type_info);
    for func in &mut mir_module.functions {
        mir::opt::run_function_passes(func);
    }

    let func = mir_module
        .functions
        .iter()
        .find(|func| func.name == "run")
        .expect("missing run");
    let has_phi = func.blocks.iter().any(|block| {
        block
            .stmts
            .iter()
            .any(|stmt| matches!(stmt, Stmt::Phi { .. }))
    });
    assert!(has_phi, "expected SSA phi insertion");
}

#[test]
fn native_actor_control_flow_awaits() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
A Counter:
    can add(x: Integer) -> Integer:
        return x + 1

to run() -> Integer:
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

    let mut mir_module = mir::lower::lower_module_with_types(&module, &type_info);
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
        value: Integer

to run() -> Integer:
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

    let mut mir_module = mir::lower::lower_module_with_types(&module, &type_info);
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
fn native_class_slot_layout_correctness() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
A Counter:
    has:
        mutable value: Integer
        mutable step: Integer

    can bump() -> Nothing:
        its.value += its.step

to run() -> Integer:
    c = Counter(value=5, step=2)
    c.bump()
    c.step = 4
    c.bump()
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

    let mut mir_module = mir::lower::lower_module_with_types(&module, &type_info);
    for func in &mut mir_module.functions {
        mir::opt::run_function_passes(func);
    }
    let mir_errors = mir::validate::validate_module(&mir_module);
    assert!(mir_errors.is_empty(), "mir errors: {mir_errors:?}");

    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("wr_class_slot_layout_correctness");
    wrela::backend::cranelift::compile_to_executable(&mir_module, &out).expect("codegen failed");

    let status = Command::new(&out).status().expect("run failed");
    let expected = expected_int_exit(11);
    assert_eq!(status.code().unwrap_or(-1), expected);
}

#[test]
fn native_builtins_bytes_and_io() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("builtins.txt");
    let path_str = path.to_string_lossy().replace('\\', "\\\\");
    let source = format!(
        r#"
to run() -> Integer:
    payload = __wr_bytes_from_string("123")
    __wr_fs_write_bytes("{path}", payload) otherwise nothing

    read_payload = __wr_fs_read_bytes("{path}") otherwise __wr_bytes_from_string("0")
    text = __wr_bytes_to_string(read_payload)
    numbers = __wr_bytes_to_list(read_payload)
    round_trip = __wr_bytes_from_list(numbers)

    if text == "123" and __wr_bytes_len(round_trip) == 3:
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

    let mut mir_module = mir::lower::lower_module_with_types(&module, &type_info);
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
fn native_builtin_external_call_stub() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
use try_to_call_external from host/external

to run() -> Integer:
    headers = __wr_map_new()
    response = try_to_call_external(
        "billing",
        "charge",
        "POST",
        "https://api.example.test/charges",
        headers,
        "amount=100",
        2500
    ) otherwise "bad"

    if response == "bad":
        return 0
    return 1
"#;
    let dir = tempfile::tempdir().expect("tempdir");
    let entry_path = dir
        .path()
        .join("src")
        .join("infrastructure")
        .join("integrations")
        .join("main.wr");
    fs::create_dir_all(entry_path.parent().unwrap()).expect("create src dir");
    fs::write(&entry_path, source).expect("write source");

    let project = load_project(&entry_path).expect("load project");
    let module = project.module;
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
    optimize_mir_module(&mut mir_module, Some(&check_ir));
    let mir_errors = mir::validate::validate_module(&mir_module);
    assert!(mir_errors.is_empty(), "mir errors: {mir_errors:?}");

    let out = dir.path().join("wr_external_call_stub");
    wrela::backend::cranelift::compile_to_executable(&mir_module, &out).expect("codegen failed");
    let output = Command::new(&out).output().expect("run failed");
    let expected = expected_int_exit(1);
    match output.status.code() {
        Some(code) => assert_eq!(code, expected),
        None => {
            panic!(
                "process terminated by signal {:?}: {}",
                output.status.signal(),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

#[test]
fn native_actor_match_await() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
A Counter:
    can add(x: Integer) -> Integer:
        return x + 1

to run() -> Integer:
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

    let mut mir_module = mir::lower::lower_module_with_types(&module, &type_info);
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
        value: Integer

    can inc() -> Integer:
        return its.value + 1

to run() -> Integer:
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

    let mut mir_module = mir::lower::lower_module_with_types(&module, &type_info);
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
        mutable value: Integer

    can add(delta: Integer) -> Nothing:
        its.value += delta

to run() -> Integer:
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

    let mut mir_module = mir::lower::lower_module_with_types(&module, &type_info);
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
    can ping(x: Integer) -> Integer:
        return x

to run() -> Integer:
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

    let mut mir_module = mir::lower::lower_module_with_types(&module, &type_info);
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
    can ping(x: Integer) -> Integer:
        return x

to run() -> Integer:
    optimize balance:
        c = detach Counter() * n
        if size(c) > 0:
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

    let mut mir_module = mir::lower::lower_module_with_types(&module, &type_info);
    for func in &mut mir_module.functions {
        mir::opt::run_function_passes(func);
    }
    let mir_errors = mir::validate::validate_module(&mir_module);
    assert!(mir_errors.is_empty(), "mir errors: {mir_errors:?}");

    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("wr_pool_auto_smoke");
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
fn native_pool_mailbox_len_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
use queue_len from pool
use:
    get_mailbox_length,
    pause,
    pause_and_wait,
    resume
from actor

A Counter:
    can ping(x: Integer) -> Integer:
        return x

to run() -> Integer:
    optimize balance:
        c = detach Counter() * 2
        pause(c)
        pause_and_wait(c)
        fire c.ping(1)
        len = queue_len(c) + get_mailbox_length(c)
        resume(c)
        observed = await c.ping(2) otherwise 0
        if len >= 1:
            if observed == 2:
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

    let mut mir_module = mir::lower::lower_module_with_types(&module, &type_info);
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
    pause_and_wait,
    resume
from actor
use Runtime from runtime
use:
    get,
    get_dropped_paused_id
from metrics

A Counter:
    can ping(x: Integer) -> Integer:
        return x

to run() -> Integer:
    Runtime(paused_queue_cap=1).__configure__()
    optimize balance:
        c = detach Counter() * 2
        pause(c)
        pause_and_wait(c)
        for i in 1...4:
            fire c.ping(i)
        dropped = get(get_dropped_paused_id())
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

    let mut mir_module = mir::lower::lower_module_with_types(&module, &type_info);
    for func in &mut mir_module.functions {
        mir::opt::run_function_passes(func);
    }
    let mir_errors = mir::validate::validate_module(&mir_module);
    assert!(mir_errors.is_empty(), "mir errors: {mir_errors:?}");

    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("wr_pool_pause_drop_smoke");
    wrela::backend::cranelift::compile_to_executable(&mir_module, &out).expect("codegen failed");
    if std::env::var("WR_KEEP_NATIVE_BIN").is_ok() {
        eprintln!("WR_KEEP_NATIVE_BIN={}", out.display());
        std::mem::forget(dir);
    }

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
fn native_pool_backpressure_config_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = format!(
        r#"
use size from pool

A Counter:
    can ping(x: Integer) -> Integer:
        return x

to run() -> Integer:
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

    let mut mir_module = mir::lower::lower_module_with_types(&module, &type_info);
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
fn native_pool_policy_matrix_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }

    struct PolicyMatrixRow {
        objective: &'static str,
        min_size: i64,
        max_size: i64,
        weight: i64,
        queue_cap: i64,
        expected_size: i64,
    }

    let rows = [
        PolicyMatrixRow {
            objective: "balance",
            min_size: 1,
            max_size: 4,
            weight: 3,
            queue_cap: 1,
            expected_size: 4,
        },
        PolicyMatrixRow {
            objective: "latency",
            min_size: 1,
            max_size: 2,
            weight: 1,
            queue_cap: 1,
            expected_size: 2,
        },
        PolicyMatrixRow {
            objective: "throughput",
            min_size: 2,
            max_size: 4,
            weight: 1,
            queue_cap: 1,
            expected_size: 4,
        },
    ];

    for row in rows {
        let source = format!(
            r#"
A Counter:
    can ping(x: Integer) -> Integer:
        return x

to run() -> Integer:
    optimize {objective}:
        c = detach Pool.of(
            Counter,
            size=n,
            min={min_size},
            max={max_size},
            weight={weight},
            backpressure=queue({queue_cap})
        ) * 1
        __wr_actor_pause(c)
        __wr_actor_pause_wait(c)
        for i in 1...4:
            fire c.ping(i)
        observed = __wr_pool_queue_len(c) + __wr_actor_mailbox_len(c)
        pool_size = __wr_pool_size(c)
        __wr_actor_resume(c)
        if pool_size == {expected_size} and observed >= 1:
            return 1
        return 0
"#,
            objective = row.objective,
            min_size = row.min_size,
            max_size = row.max_size,
            weight = row.weight,
            queue_cap = row.queue_cap,
            expected_size = row.expected_size
        );

        let output = compile_and_run_native_source(
            &source,
            &format!("wr_pool_policy_matrix_{}", row.objective),
        );
        let expected = expected_int_exit(1);
        match output.status.code() {
            Some(code) => assert_eq!(
                code, expected,
                "policy matrix row failed for objective={}",
                row.objective
            ),
            None => {
                use std::os::unix::process::ExitStatusExt;
                panic!(
                    "row objective={} terminated by signal {:?}: {}",
                    row.objective,
                    output.status.signal(),
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
    }
}

#[test]
fn native_scheduler_policy_helpers_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }

    let output = compile_and_run_native_source(
        r#"
use:
    choose_next_shard,
    compute_dispatch_budget,
    scheduler_should_steal_work,
    refill_credits
from scheduler

to run() -> Integer:
    shard_a = choose_next_shard(11, 4, false)
    shard_b = choose_next_shard(11, 4, true)
    budget = compute_dispatch_budget(12, 4, 9)
    steal_a = scheduler_should_steal_work given 0, 3, false
    steal_b = scheduler_should_steal_work given 0, 3, true
    credits = refill_credits(1, 5, 2, 2)

    if shard_a == 3 and shard_b == 0 and budget == 8 and steal_a and not steal_b and credits == 5:
        return 1
    return 0
"#,
        "wr_scheduler_policy_helpers",
    );

    let expected = expected_int_exit(1);
    match output.status.code() {
        Some(code) => assert_eq!(code, expected),
        None => {
            use std::os::unix::process::ExitStatusExt;
            panic!(
                "terminated by signal {:?}: {}",
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
    can ping(x: Integer) -> Integer:
        return x

to run() -> Integer:
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

    let mut mir_module = mir::lower::lower_module_with_types(&module, &type_info);
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
    can ping(x: Integer) -> Integer:
        return x

to run() -> Integer:
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

    let mut mir_module = mir::lower::lower_module_with_types(&module, &type_info);
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
    can ping(x: Integer) -> Integer:
        return x

to run() -> Integer:
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

    let mut mir_module = mir::lower::lower_module_with_types(&module, &type_info);
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
    can add(x: Integer) -> Integer:
        return x + 1

to run() -> Integer:
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

    let mut mir_module = mir::lower::lower_module_with_types(&module, &type_info);
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
fn native_actor_fire_burst_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
A Counter:
    has:
        mutable total: Integer = 0
    can add(x: Integer) -> Integer:
        its.total += x
        return its.total

to run() -> Integer:
    optimize balance:
        c = detach Counter() * 1
        __wr_actor_fire_burst_begin(c)
        fire c.add(2)
        fire c.add(3)
        __wr_actor_fire_burst_end(c)
        d = 0
        v = await c.add(4) otherwise d
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

    let mut mir_module = mir::lower::lower_module_with_types(&module, &type_info);
    for func in &mut mir_module.functions {
        mir::opt::run_function_passes(func);
    }
    let mir_errors = mir::validate::validate_module(&mir_module);
    assert!(mir_errors.is_empty(), "mir errors: {mir_errors:?}");

    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("wr_actor_fire_burst_smoke");
    wrela::backend::cranelift::compile_to_executable(&mir_module, &out).expect("codegen failed");

    let keep_native = std::env::var("WR_KEEP_NATIVE_BIN").is_ok();
    if keep_native {
        eprintln!("WR_KEEP_NATIVE_BIN={}", out.display());
        std::mem::forget(dir);
    }

    let output = Command::new(&out).output().expect("run failed");
    let expected = expected_int_exit(9);
    let code = output.status.code().unwrap_or(-1);
    assert_eq!(
        code,
        expected,
        "native exited code={code} signal={:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.signal(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn native_pool_fire_burst_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
A Counter:
    has:
        mutable total: Integer = 0
    can add(x: Integer) -> Integer:
        its.total += x
        return its.total

to run() -> Integer:
    optimize balance:
        p = detach Pool.of(Counter, size=1) * 1
        __wr_actor_fire_burst_begin(p)
        fire p.add(1)
        fire p.add(2)
        __wr_actor_fire_burst_end(p)
        d = 0
        v = await p.add(3) otherwise d
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

    let mut mir_module = mir::lower::lower_module_with_types(&module, &type_info);
    for func in &mut mir_module.functions {
        mir::opt::run_function_passes(func);
    }
    let mir_errors = mir::validate::validate_module(&mir_module);
    assert!(mir_errors.is_empty(), "mir errors: {mir_errors:?}");

    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("wr_pool_fire_burst_smoke");
    wrela::backend::cranelift::compile_to_executable(&mir_module, &out).expect("codegen failed");

    let keep_native = std::env::var("WR_KEEP_NATIVE_BIN").is_ok();
    if keep_native {
        eprintln!("WR_KEEP_NATIVE_BIN={}", out.display());
        std::mem::forget(dir);
    }

    let output = Command::new(&out).output().expect("run failed");
    let expected = expected_int_exit(6);
    let code = output.status.code().unwrap_or(-1);
    assert_eq!(
        code,
        expected,
        "native exited code={code} signal={:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.signal(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn native_result_otherwise_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
to fail(flag: Boolean) -> Result:
    if flag:
        return error "nope"
    return 7

to run() -> Integer:
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

    let mut mir_module = mir::lower::lower_module_with_types(&module, &type_info);
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
    Processing(worker_id: Integer)

to run() -> Integer:
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

    let mut mir_module = mir::lower::lower_module_with_types(&module, &type_info);
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
to maybe(ok: Boolean) -> Result[Integer, Integer]:
    if ok:
        return 1
    return error 9

to run() -> Integer:
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

    let mut mir_module = mir::lower::lower_module_with_types(&module, &type_info);
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

to run() -> Integer:
    b = Box[Integer](value=3)
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

    let mut mir_module = mir::lower::lower_module_with_types(&module, &type_info);
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

to show(p: Printable) -> Integer:
    if p.show() == "foo":
        return 1
    return 2

to run() -> Integer:
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

    let mut mir_module = mir::lower::lower_module_with_types(&module, &type_info);
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
fn interface_devirt_monomorphic_rewrites_callsite_to_direct() {
    let source = r#"
A Printable:
    must score() -> Integer

A Foo:
    is a Printable
    can score() -> Integer:
        return 7

to call(p: Printable) -> Integer:
    return p.score()

to run() -> Integer:
    return call(p=Foo())
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

    let check_ir = hir::checkir::extract_module(&module);
    let mut mir_module = mir::lower::lower_module_with_types(&module, &type_info);
    optimize_mir_module(&mut mir_module, Some(&check_ir));

    let call_fn = mir_module
        .functions
        .iter()
        .find(|func| func.name.as_str() == "call")
        .expect("missing call function");

    let mut saw_direct = false;
    for block in &call_fn.blocks {
        for stmt in &block.stmts {
            let Stmt::Assign { value, .. } = stmt else {
                continue;
            };
            let Rvalue::Call {
                kind: CallKind::Sync,
                target: CallTarget::Function(name),
                ..
            } = value
            else {
                continue;
            };
            if name.as_str().starts_with("Foo.score") {
                saw_direct = true;
            }
            assert_ne!(
                name.as_str(),
                "Printable.score",
                "monomorphic interface call should not keep dispatch target"
            );
            assert!(
                !name.as_str().starts_with("__wr_iface_guard."),
                "monomorphic interface call should not use guard helper"
            );
        }
    }
    assert!(
        saw_direct,
        "expected monomorphic interface call to rewrite to direct Foo.score target"
    );
}

#[test]
fn interface_devirt_polymorphic_generates_guard_with_fallback() {
    let source = r#"
A Printable:
    must score() -> Integer

A Foo:
    is a Printable
    can score() -> Integer:
        return 1

A Bar:
    is a Printable
    can score() -> Integer:
        return 2

A Baz:
    is a Printable
    can score() -> Integer:
        return 3

to call(p: Printable) -> Integer:
    return p.score()

to run() -> Integer:
    return call(p=Baz())
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

    let check_ir = hir::checkir::extract_module(&module);
    let mut mir_module = mir::lower::lower_module_with_types(&module, &type_info);
    optimize_mir_module(&mut mir_module, Some(&check_ir));

    let call_fn = mir_module
        .functions
        .iter()
        .find(|func| func.name.as_str() == "call")
        .expect("missing call function");

    let guarded = call_fn
        .blocks
        .iter()
        .flat_map(|block| block.stmts.iter())
        .find_map(|stmt| {
            let Stmt::Assign { value, .. } = stmt else {
                return None;
            };
            let Rvalue::Call {
                kind: CallKind::Sync,
                target,
                ..
            } = value
            else {
                return None;
            };
            let CallTarget::GuardedInterface {
                fast_paths,
                fallback,
            } = target
            else {
                return None;
            };
            Some((fast_paths.clone(), fallback.clone()))
        })
        .expect("expected callsite to use guarded interface target");
    let (fast_paths, fallback) = guarded;
    assert!(
        fast_paths.len() >= 2,
        "expected guarded target to include multiple fast paths"
    );
    assert!(
        fallback.as_str() == "Printable.score",
        "expected guarded target to keep interface dispatch fallback"
    );

    if std::env::var("WR_SKIP_NATIVE").is_err() {
        let dir = tempfile::tempdir().expect("tempdir");
        let out = dir.path().join("wr_interface_guard_fallback");
        wrela::backend::cranelift::compile_to_executable(&mir_module, &out)
            .expect("codegen failed");
        let status = Command::new(&out).status().expect("run failed");
        let expected = expected_int_exit(3);
        assert_eq!(status.code().unwrap_or(-1), expected);
    }
}

#[test]
fn native_defer_smoke() {
    if std::env::var("WR_SKIP_NATIVE").is_ok() {
        return;
    }
    let source = r#"
A Counter:
    has:
        mutable value: Integer
    can add(delta: Integer) -> Nothing:
        its.value += delta

to bump(counter: Counter) -> Nothing:
    defer counter.add(1)
    defer counter.add(2)
    return

to run() -> Integer:
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

    let mut mir_module = mir::lower::lower_module_with_types(&module, &type_info);
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
        mutable value: Integer
    can add(delta: Integer) -> Nothing:
        its.value += delta

to bump(counter: Counter, flip: Boolean) -> Nothing:
    defer counter.add(1)
    if flip:
        defer counter.add(2)
        return
    defer counter.add(4)
    return

to run() -> Integer:
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

    let mut mir_module = mir::lower::lower_module_with_types(&module, &type_info);
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
to run() -> Integer:
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

    let mut mir_module = mir::lower::lower_module_with_types(&module, &type_info);
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
