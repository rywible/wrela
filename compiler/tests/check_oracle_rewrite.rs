use rowan::TextRange;
use std::fs;
use wrela::hir;
use wrela::hir::checkir::{CheckBinaryOp, CheckValue, extract_module};
use wrela::mir;
use wrela::mir::ir::{
    BasicBlock, BlockId, Local, LocalId, MirFunction, MirModule, MirType, Place,
    PortableAbiType, Rvalue, Stmt, Temp, TempId, Terminator, Value,
};
use wrela::mir::rewrite::{
    RewriteBudget, admit_rulepack, admit_rulepack_scored, apply_rulepack, mine_admit_and_apply,
    mine_candidates,
};

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
class Checks {
    fn value_is_positive(v: Integer) -> Boolean {
        return v > 0 and v + 1 > 1
    }

    fn helper(v: Integer) -> Boolean {
        return v > 0
    }

    fn skipped_check(v: Integer) -> Boolean {
        return helper(v)
    }
}

class Probe {
    fn is_ready(v: Integer) -> Boolean {
        return v * 1 > 0
    }
}

fn run() -> Integer {
    return 0
}
"#;

    let module = load_module_from_source(source);
    let checkir = extract_module(&module);

    let check = checkir
        .checks
        .iter()
        .find(|check| check.name.as_str() == "Checks.value_is_positive")
        .expect("missing top-level check");
    assert!(check.ops_used.contains(&CheckBinaryOp::And));
    assert!(check.ops_used.contains(&CheckBinaryOp::Add));
    assert_ne!(check.shape_id, 0);
    assert!(check.supports_vector_lane);

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
    assert!(batch.fallback_reason_counts.is_empty());

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
            .any(|skipped| skipped.name.as_str() == "Checks.skipped_check")
    );
}

#[test]
fn checkir_batch_plan_fallback_accounting_is_deterministic() {
    let source = r#"
class Risk {
    fn risky(v: Integer) -> Boolean {
        return (10 / v) > 1
    }
}

fn run() -> Integer {
    return 0
}
"#;
    let module = load_module_from_source(source);
    let checkir = extract_module(&module);
    let check = checkir
        .checks
        .iter()
        .find(|check| check.name.as_str() == "Risk.risky")
        .expect("missing check");
    assert!(!check.supports_vector_lane);

    let rows = vec![
        vec![CheckValue::Integer(2)],
        vec![CheckValue::Integer(0)],
        vec![CheckValue::Integer(5)],
    ];

    let mut left_plan = check.build_batch_plan(4);
    let left = check.eval_batch_with_plan(&rows, &mut left_plan);
    let mut right_plan = check.build_batch_plan(4);
    let right = check.eval_batch_with_plan(&rows, &mut right_plan);

    assert_eq!(left.values, vec![Some(true), None, Some(true)]);
    assert_eq!(left.values, right.values);
    assert_eq!(left.fallback_reason_counts, right.fallback_reason_counts);
    assert_eq!(
        left_plan.fallback_reason_counts,
        right_plan.fallback_reason_counts
    );
    assert_eq!(
        left.fallback_reason_counts
            .get("vector_lane_unsupported")
            .copied(),
        Some(3)
    );
    assert_eq!(
        left.fallback_reason_counts.get("scalar_eval_none").copied(),
        Some(1)
    );
}

#[test]
fn rewrite_admission_and_application_are_deterministic() {
    let module = sample_rewrite_module();

    let candidates = mine_candidates(&module, None);
    let admission = admit_rulepack_scored(&candidates, &module, None, RewriteBudget::default(), 8);
    let pack = admission.pack;

    let mut left = module.clone();
    let mut right = module.clone();

    let left_report = apply_rulepack(
        &mut left,
        &pack,
        RewriteBudget {
            max_steps: 128,
            ..RewriteBudget::default()
        },
    );
    let right_report = apply_rulepack(
        &mut right,
        &pack,
        RewriteBudget {
            max_steps: 128,
            ..RewriteBudget::default()
        },
    );

    assert_eq!(left_report, right_report);
    assert_eq!(left, right);
    assert!(left_report.applied >= 3);
}

#[test]
fn checkir_shape_hash_is_canonical_for_commutative_forms() {
    let source = r#"
class Pair {
    fn left(a: Integer, b: Integer) -> Boolean {
        return (a + b) > 0
    }

    fn right(a: Integer, b: Integer) -> Boolean {
        return (b + a) > 0
    }
}

fn run() -> Integer {
    return 0
}
"#;
    let module = load_module_from_source(source);
    let checkir = extract_module(&module);
    let left = checkir
        .checks
        .iter()
        .find(|check| check.name.as_str() == "Pair.left")
        .expect("left");
    let right = checkir
        .checks
        .iter()
        .find(|check| check.name.as_str() == "Pair.right")
        .expect("right");
    assert_eq!(left.shape_id, right.shape_id);
}

#[test]
fn rewrite_budget_guard_exhausts_deterministically() {
    let mut module = sample_rewrite_module();
    let candidates = mine_candidates(&module, None);
    let pack = admit_rulepack(&candidates, 8);

    let report = apply_rulepack(
        &mut module,
        &pack,
        RewriteBudget {
            max_steps: 1,
            ..RewriteBudget::default()
        },
    );
    assert!(report.budget_exhausted);
    assert_eq!(report.steps, 1);
}

#[test]
fn rewrite_admission_respects_budget_and_deterministic_order() {
    let module = sample_rewrite_module();
    let candidates = mine_candidates(&module, None);
    let admission_a = admit_rulepack_scored(
        &candidates,
        &module,
        None,
        RewriteBudget {
            max_compile_cost: 2,
            max_rule_risk: 10,
            ..RewriteBudget::default()
        },
        8,
    );
    let admission_b = admit_rulepack_scored(
        &candidates,
        &module,
        None,
        RewriteBudget {
            max_compile_cost: 2,
            max_rule_risk: 10,
            ..RewriteBudget::default()
        },
        8,
    );

    assert_eq!(admission_a, admission_b);
    assert!(!admission_a.pack.rules.is_empty());
    assert!(admission_a.ignored_by_budget >= 1);
    assert!(
        admission_a
            .admission_reason
            .iter()
            .any(|note| note.reason.as_str() == "ignored_by_budget")
    );
}

#[test]
fn rewrite_oscillation_guard_blocks_repeated_hash_states() {
    let mut module = sample_rewrite_module();
    let report = mine_admit_and_apply(
        &mut module,
        None,
        RewriteBudget {
            per_function_rewrite_cap: 1,
            ..RewriteBudget::default()
        },
        8,
    );
    assert!(report.applied <= 1);
    assert!(report.oscillation_block_count <= report.steps);
}

#[test]
fn rewrite_mine_admit_apply_is_deterministic_with_scores() {
    let mut left = sample_rewrite_module();
    let mut right = sample_rewrite_module();
    let budget = RewriteBudget {
        max_compile_cost: 6,
        max_rule_risk: 10,
        per_function_rewrite_cap: 16,
        ..RewriteBudget::default()
    };

    let left_report = mine_admit_and_apply(&mut left, None, budget, 8);
    let right_report = mine_admit_and_apply(&mut right, None, budget, 8);
    assert_eq!(left_report, right_report);
    assert_eq!(left, right);
}

fn sample_rewrite_module() -> MirModule {
    let span = TextRange::empty(0.into());
    let func = MirFunction {
        name: "run".into(),
        params: vec![LocalId(0)],
        abi_params: vec![PortableAbiType::Value],
        abi_return: PortableAbiType::Value,
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
