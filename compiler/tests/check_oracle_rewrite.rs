use rowan::TextRange;
use std::fs;
use wrela::hir;
use wrela::hir::checkir::{CheckBinaryOp, CheckValue, extract_module};
use wrela::mir;
use wrela::mir::ir::{
    BasicBlock, BlockId, Local, LocalId, MirFunction, MirModule, MirType, Place, Rvalue, Stmt,
    Temp, TempId, Terminator, Value,
};
use wrela::mir::rewrite::{RewriteBudget, admit_rulepack, apply_rulepack, mine_candidates};

fn load_module_from_source(source: &str) -> hir::Module {
    let dir = tempfile::tempdir().expect("tempdir");
    let entry_path = dir.path().join("src").join("main.wr");
    fs::create_dir_all(entry_path.parent().expect("src dir")).expect("create src dir");
    fs::write(&entry_path, source).expect("write source");
    let project = hir::project::load_project(&entry_path).expect("load project");
    project.module
}

#[test]
fn checkir_extracts_checks_and_evaluates_scalar_and_batch() {
    let source = r#"
check value_is_positive(v: Integer) -> Boolean:
    return v > 0 and v + 1 > 1

A Probe:
    checks is_ready(v: Integer) -> Boolean:
        return v * 1 > 0

to helper(v: Integer) -> Boolean:
    return v > 0

check skipped_check(v: Integer) -> Boolean:
    return helper(v)

to run() -> Integer:
    return 0
"#;

    let module = load_module_from_source(source);
    let checkir = extract_module(&module);

    let check = checkir
        .checks
        .iter()
        .find(|check| check.name.as_str() == "value_is_positive")
        .expect("missing top-level check");
    assert!(check.ops_used.contains(&CheckBinaryOp::And));
    assert!(check.ops_used.contains(&CheckBinaryOp::Add));

    assert_eq!(
        check.eval_scalar_bool(&[CheckValue::Integer(3)]),
        Some(true)
    );
    assert_eq!(
        check.eval_scalar_bool(&[CheckValue::Integer(0)]),
        Some(false)
    );

    let batch = check.eval_batch_bool(&[
        vec![CheckValue::Integer(-4)],
        vec![CheckValue::Integer(1)],
        vec![CheckValue::Integer(2)],
    ]);
    assert_eq!(batch.lane_width, 8);
    assert_eq!(batch.values, vec![Some(false), Some(true), Some(true)]);

    assert!(
        checkir
            .checks
            .iter()
            .any(|check| check.name.as_str() == "Probe.is_ready")
    );
    assert!(
        checkir
            .skipped
            .iter()
            .any(|skipped| skipped.name.as_str() == "skipped_check")
    );
}

#[test]
fn rewrite_admission_and_application_are_deterministic() {
    let module = sample_rewrite_module();

    let candidates = mine_candidates(&module, None);
    let pack = admit_rulepack(&candidates, 8);

    let mut left = module.clone();
    let mut right = module.clone();

    let left_report = apply_rulepack(&mut left, &pack, RewriteBudget { max_steps: 128 });
    let right_report = apply_rulepack(&mut right, &pack, RewriteBudget { max_steps: 128 });

    assert_eq!(left_report, right_report);
    assert_eq!(left, right);
    assert!(left_report.applied >= 3);
}

#[test]
fn rewrite_budget_guard_exhausts_deterministically() {
    let mut module = sample_rewrite_module();
    let candidates = mine_candidates(&module, None);
    let pack = admit_rulepack(&candidates, 8);

    let report = apply_rulepack(&mut module, &pack, RewriteBudget { max_steps: 1 });
    assert!(report.budget_exhausted);
    assert_eq!(report.steps, 1);
}

fn sample_rewrite_module() -> MirModule {
    let span = TextRange::empty(0.into());
    let func = MirFunction {
        name: "run".into(),
        params: vec![LocalId(0)],
        locals: vec![Local {
            name: "x".into(),
            mutable: false,
            ty: MirType::Integer,
        }],
        temps: vec![
            Temp {
                ty: MirType::Integer,
            },
            Temp {
                ty: MirType::Integer,
            },
        ],
        blocks: vec![
            BasicBlock {
                stmts: vec![
                    Stmt::Assign {
                        place: Place::Temp(TempId(0)),
                        value: Rvalue::Binary {
                            op: hir::BinaryOp::Add,
                            lhs: Value::Local(LocalId(0)),
                            rhs: Value::Const(hir::Literal::Integer(0)),
                        },
                        span,
                    },
                    Stmt::Assign {
                        place: Place::Temp(TempId(1)),
                        value: Rvalue::Binary {
                            op: hir::BinaryOp::Mul,
                            lhs: Value::Temp(TempId(0)),
                            rhs: Value::Const(hir::Literal::Integer(1)),
                        },
                        span,
                    },
                ],
                terminator: Terminator::Branch {
                    cond: Value::Const(hir::Literal::Boolean(true)),
                    then_target: BlockId(1),
                    else_target: BlockId(2),
                    span,
                },
            },
            BasicBlock {
                stmts: vec![],
                terminator: Terminator::Return {
                    value: Some(Value::Temp(TempId(1))),
                    span,
                },
            },
            BasicBlock {
                stmts: vec![],
                terminator: Terminator::Return {
                    value: Some(Value::Local(LocalId(0))),
                    span,
                },
            },
        ],
        entry: BlockId(0),
        suspendable: false,
    };

    mir::ir::MirModule {
        functions: vec![func],
        type_tags: Vec::new(),
        classes: Vec::new(),
    }
}
