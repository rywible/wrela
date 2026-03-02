use rowan::TextRange;
use wrela::hir::Literal;
use wrela::mir::effect_ir::{EffectKind, extract_effect_ir};
use wrela::mir::ir::{
    BasicBlock, BlockId, Local, LocalId, MirFunction, MirType, Place, Rvalue, Stmt, Temp, TempId,
    Terminator, Value,
};
use wrela::mir::opt;

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
    assert_eq!(report.cross_block_rewrites, 0);
    assert!(report.blocked_rewrite_reasons.is_empty());
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

#[test]
fn annihilation_rewrites_cross_block_when_dominance_holds() {
    let span = span(0, 1);
    let mut func = MirFunction {
        name: "cross_block_ok".into(),
        params: vec![],
        locals: vec![Local {
            name: "ok".into(),
            mutable: false,
            ty: MirType::Integer,
        }],
        temps: vec![
            Temp {
                ty: MirType::Result(Box::new(MirType::Integer), Box::new(MirType::String)),
            },
            Temp {
                ty: MirType::Integer,
            },
        ],
        blocks: vec![
            BasicBlock {
                stmts: vec![Stmt::Assign {
                    place: Place::Temp(TempId(0)),
                    value: Rvalue::ResultOk {
                        value: Value::Local(LocalId(0)),
                    },
                    span,
                }],
                terminator: Terminator::Jump {
                    target: BlockId(1),
                    span,
                },
            },
            BasicBlock {
                stmts: vec![Stmt::Assign {
                    place: Place::Temp(TempId(1)),
                    value: Rvalue::ResultUnwrap {
                        value: Value::Temp(TempId(0)),
                    },
                    span,
                }],
                terminator: Terminator::Return {
                    value: Some(Value::Temp(TempId(1))),
                    span,
                },
            },
        ],
        entry: BlockId(0),
        suspendable: false,
    };

    let report = wrela::mir::effect_ir::annihilate_result_wrappers(&mut func);
    assert_eq!(report.rewritten_statements, 1);
    assert_eq!(report.cross_block_rewrites, 1);
    assert!(report.blocked_rewrite_reasons.is_empty());
    assert!(matches!(
        func.blocks[1].stmts[0],
        Stmt::Assign {
            value: Rvalue::Use(Value::Local(LocalId(0))),
            ..
        }
    ));
}

#[test]
fn annihilation_blocks_non_dominating_cross_block_rewrite() {
    let span = span(0, 1);
    let mut func = MirFunction {
        name: "cross_block_blocked".into(),
        params: vec![],
        locals: vec![Local {
            name: "ok".into(),
            mutable: false,
            ty: MirType::Integer,
        }],
        temps: vec![
            Temp {
                ty: MirType::Result(Box::new(MirType::Integer), Box::new(MirType::String)),
            },
            Temp {
                ty: MirType::Integer,
            },
        ],
        blocks: vec![
            BasicBlock {
                stmts: vec![Stmt::Assign {
                    place: Place::Temp(TempId(1)),
                    value: Rvalue::ResultUnwrap {
                        value: Value::Temp(TempId(0)),
                    },
                    span,
                }],
                terminator: Terminator::Jump {
                    target: BlockId(1),
                    span,
                },
            },
            BasicBlock {
                stmts: vec![Stmt::Assign {
                    place: Place::Temp(TempId(0)),
                    value: Rvalue::ResultOk {
                        value: Value::Local(LocalId(0)),
                    },
                    span,
                }],
                terminator: Terminator::Return { value: None, span },
            },
        ],
        entry: BlockId(0),
        suspendable: false,
    };

    let report = wrela::mir::effect_ir::annihilate_result_wrappers(&mut func);
    assert_eq!(report.rewritten_statements, 0);
    assert_eq!(report.cross_block_rewrites, 0);
    assert_eq!(
        report
            .blocked_rewrite_reasons
            .get("blocked_dominance")
            .copied(),
        Some(2)
    );
}

#[test]
fn opt_hoists_loop_invariant_result_is_ok() {
    let span = span(0, 1);
    let mut func = MirFunction {
        name: "loop_hoist".into(),
        params: vec![],
        locals: vec![Local {
            name: "ok".into(),
            mutable: false,
            ty: MirType::Integer,
        }],
        temps: vec![
            Temp {
                ty: MirType::Result(Box::new(MirType::Integer), Box::new(MirType::String)),
            },
            Temp {
                ty: MirType::Boolean,
            },
        ],
        blocks: vec![
            BasicBlock {
                stmts: vec![Stmt::Assign {
                    place: Place::Temp(TempId(0)),
                    value: Rvalue::ResultOk {
                        value: Value::Local(LocalId(0)),
                    },
                    span,
                }],
                terminator: Terminator::Jump {
                    target: BlockId(1),
                    span,
                },
            },
            BasicBlock {
                stmts: vec![Stmt::Assign {
                    place: Place::Temp(TempId(1)),
                    value: Rvalue::ResultIsOk {
                        value: Value::Temp(TempId(0)),
                    },
                    span,
                }],
                terminator: Terminator::Branch {
                    cond: Value::Const(Literal::Boolean(true)),
                    then_target: BlockId(2),
                    else_target: BlockId(3),
                    span,
                },
            },
            BasicBlock {
                stmts: vec![],
                terminator: Terminator::Jump {
                    target: BlockId(1),
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
        ],
        entry: BlockId(0),
        suspendable: false,
    };

    opt::run_function_passes(&mut func);

    let preheader_has_result_is_ok = func.blocks[0].stmts.iter().any(|stmt| {
        matches!(
            stmt,
            Stmt::Assign {
                value: Rvalue::Use(Value::Const(Literal::Boolean(_))),
                ..
            } | Stmt::Assign {
                value: Rvalue::ResultIsOk { .. },
                ..
            }
        )
    });
    let loop_header_has_result_is_ok = func.blocks[1].stmts.iter().any(|stmt| {
        matches!(
            stmt,
            Stmt::Assign {
                value: Rvalue::ResultIsOk { .. },
                ..
            }
        )
    });

    assert!(preheader_has_result_is_ok);
    assert!(!loop_header_has_result_is_ok);
}
