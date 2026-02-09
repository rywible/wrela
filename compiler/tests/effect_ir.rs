use rowan::TextRange;
use wrela::hir::Literal;
use wrela::mir::effect_ir::{EffectKind, extract_effect_ir};
use wrela::mir::ir::{
    BasicBlock, BlockId, Local, LocalId, MirFunction, MirType, Place, Rvalue, Stmt, Temp, TempId,
    Terminator, Value,
};

fn span(start: u32, end: u32) -> TextRange {
    TextRange::new(start.into(), end.into())
}

fn test_function() -> MirFunction {
    MirFunction {
        name: "effect_test".into(),
        params: vec![],
        locals: vec![
            Local {
                name: "ok_val".into(),
                mutable: false,
                ty: MirType::Integer,
            },
            Local {
                name: "err_val".into(),
                mutable: false,
                ty: MirType::String,
            },
        ],
        temps: vec![
            Temp {
                ty: MirType::Result(Box::new(MirType::Integer), Box::new(MirType::String)),
            },
            Temp {
                ty: MirType::Result(Box::new(MirType::Integer), Box::new(MirType::String)),
            },
            Temp {
                ty: MirType::Integer,
            },
            Temp {
                ty: MirType::String,
            },
            Temp {
                ty: MirType::Boolean,
            },
            Temp {
                ty: MirType::Pending(Box::new(MirType::Integer)),
            },
            Temp {
                ty: MirType::Integer,
            },
        ],
        blocks: vec![BasicBlock {
            stmts: vec![
                Stmt::Assign {
                    place: Place::Temp(TempId(0)),
                    value: Rvalue::ResultOk {
                        value: Value::Local(LocalId(0)),
                    },
                    span: span(0, 1),
                },
                Stmt::Assign {
                    place: Place::Temp(TempId(1)),
                    value: Rvalue::ResultErr {
                        value: Value::Local(LocalId(1)),
                    },
                    span: span(1, 2),
                },
                Stmt::Assign {
                    place: Place::Temp(TempId(2)),
                    value: Rvalue::ResultUnwrap {
                        value: Value::Temp(TempId(0)),
                    },
                    span: span(2, 3),
                },
                Stmt::Assign {
                    place: Place::Temp(TempId(3)),
                    value: Rvalue::ResultErrUnwrap {
                        value: Value::Temp(TempId(1)),
                    },
                    span: span(3, 4),
                },
                Stmt::Assign {
                    place: Place::Temp(TempId(4)),
                    value: Rvalue::ResultIsOk {
                        value: Value::Temp(TempId(0)),
                    },
                    span: span(4, 5),
                },
                Stmt::Await {
                    dst: Place::Temp(TempId(6)),
                    pending: Value::Temp(TempId(5)),
                    span: span(5, 6),
                },
            ],
            terminator: Terminator::Return {
                value: None,
                span: span(6, 7),
            },
        }],
        entry: BlockId(0),
        suspendable: false,
    }
}

#[test]
fn extracts_effect_ir_and_reconstruction_map() {
    let func = test_function();
    let effect_ir = extract_effect_ir(&func);

    assert_eq!(effect_ir.reconstruction.len(), 6);
    assert_eq!(effect_ir.nodes.len(), 6);
    assert!(
        effect_ir
            .nodes
            .iter()
            .any(|node| node.kind == EffectKind::ResultOkWrap)
    );
    assert!(
        effect_ir
            .nodes
            .iter()
            .any(|node| node.kind == EffectKind::ResultErrWrap)
    );
    assert!(
        effect_ir
            .nodes
            .iter()
            .any(|node| node.kind == EffectKind::ResultUnwrap)
    );
    assert!(
        effect_ir
            .nodes
            .iter()
            .any(|node| node.kind == EffectKind::ResultErrUnwrap)
    );
    assert!(
        effect_ir
            .nodes
            .iter()
            .any(|node| node.kind == EffectKind::ResultIsOk)
    );
    assert!(
        effect_ir
            .nodes
            .iter()
            .any(|node| node.kind == EffectKind::Await)
    );
}

#[test]
fn annihilation_rewrites_straightforward_result_wrappers() {
    let mut func = test_function();
    let report = wrela::mir::effect_ir::annihilate_result_wrappers(&mut func);

    assert_eq!(report.rewritten_statements, 3);
    let stmts = &func.blocks[0].stmts;

    assert!(matches!(
        stmts[2],
        Stmt::Assign {
            value: Rvalue::Use(Value::Local(LocalId(0))),
            ..
        }
    ));
    assert!(matches!(
        stmts[3],
        Stmt::Assign {
            value: Rvalue::Use(Value::Local(LocalId(1))),
            ..
        }
    ));
    assert!(matches!(
        stmts[4],
        Stmt::Assign {
            value: Rvalue::Use(Value::Const(Literal::Boolean(true))),
            ..
        }
    ));
}

#[test]
fn reconstruction_metadata_map_stays_stable_after_annihilation() {
    let mut func = test_function();
    let before = extract_effect_ir(&func).reconstruction;

    wrela::mir::effect_ir::annihilate_result_wrappers(&mut func);
    let after = extract_effect_ir(&func).reconstruction;

    assert_eq!(before, after);
}
