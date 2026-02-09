use crate::hir::Literal;
use crate::mir::ir::{MirFunction, Place, Rvalue, Stmt, TempId, Value};
use rowan::TextRange;
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EffectStmtKey {
    pub block: usize,
    pub stmt: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EffectKind {
    ResultOkWrap,
    ResultErrWrap,
    ResultUnwrap,
    ResultErrUnwrap,
    ResultIsOk,
    Await,
    Fire,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EffectNode {
    pub key: EffectStmtKey,
    pub kind: EffectKind,
    pub place: Option<Place>,
    pub input: Option<Value>,
    pub span: TextRange,
}

pub type ReconstructionMetadataMap = BTreeMap<EffectStmtKey, TextRange>;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct EffectIr {
    pub nodes: Vec<EffectNode>,
    pub reconstruction: ReconstructionMetadataMap,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EffectAnnihilationReport {
    pub rewritten_statements: usize,
    pub reconstruction: ReconstructionMetadataMap,
}

pub fn extract_effect_ir(func: &MirFunction) -> EffectIr {
    let mut out = EffectIr::default();

    for (block_idx, block) in func.blocks.iter().enumerate() {
        for (stmt_idx, stmt) in block.stmts.iter().enumerate() {
            let key = EffectStmtKey {
                block: block_idx,
                stmt: stmt_idx,
            };
            let span = stmt_span(stmt);
            out.reconstruction.insert(key, span);

            match stmt {
                Stmt::Assign { place, value, .. } => {
                    if let Some((kind, input)) = effect_from_rvalue(value) {
                        out.nodes.push(EffectNode {
                            key,
                            kind,
                            place: Some(place.clone()),
                            input,
                            span,
                        });
                    }
                }
                Stmt::Await { dst, pending, span } => {
                    out.nodes.push(EffectNode {
                        key,
                        kind: EffectKind::Await,
                        place: Some(dst.clone()),
                        input: Some(pending.clone()),
                        span: *span,
                    });
                }
                Stmt::Fire { pending, span } => {
                    out.nodes.push(EffectNode {
                        key,
                        kind: EffectKind::Fire,
                        place: None,
                        input: Some(pending.clone()),
                        span: *span,
                    });
                }
                _ => {}
            }
        }
    }

    out
}

pub fn annihilate_result_wrappers(func: &mut MirFunction) -> EffectAnnihilationReport {
    let reconstruction = extract_effect_ir(func).reconstruction;
    let mut rewritten_statements = 0;

    for block in &mut func.blocks {
        let mut result_sources: HashMap<usize, (bool, Value)> = HashMap::new();

        for stmt in &block.stmts {
            let Stmt::Assign { place, value, .. } = stmt else {
                continue;
            };
            let Place::Temp(temp) = place else {
                continue;
            };
            match value {
                Rvalue::ResultOk { value } => {
                    result_sources.insert(temp.0, (true, value.clone()));
                }
                Rvalue::ResultErr { value } => {
                    result_sources.insert(temp.0, (false, value.clone()));
                }
                _ => {}
            }
        }

        for stmt in &mut block.stmts {
            let Stmt::Assign { value, .. } = stmt else {
                continue;
            };
            match value {
                Rvalue::ResultUnwrap { value: inner } => {
                    if let Some(src) = annihilation_rewrite(inner, true, &result_sources) {
                        *value = Rvalue::Use(src);
                        rewritten_statements += 1;
                    }
                }
                Rvalue::ResultErrUnwrap { value: inner } => {
                    if let Some(src) = annihilation_rewrite(inner, false, &result_sources) {
                        *value = Rvalue::Use(src);
                        rewritten_statements += 1;
                    }
                }
                Rvalue::ResultIsOk { value: inner } => {
                    let Value::Temp(temp) = inner else {
                        continue;
                    };
                    let Some((is_ok, _)) = result_sources.get(&temp.0) else {
                        continue;
                    };
                    *value = Rvalue::Use(Value::Const(Literal::Boolean(*is_ok)));
                    rewritten_statements += 1;
                }
                _ => {}
            }
        }
    }

    EffectAnnihilationReport {
        rewritten_statements,
        reconstruction,
    }
}

fn annihilation_rewrite(
    inner: &Value,
    expect_ok: bool,
    result_sources: &HashMap<usize, (bool, Value)>,
) -> Option<Value> {
    let Value::Temp(TempId(temp)) = inner else {
        return None;
    };
    let (is_ok, source) = result_sources.get(temp)?;
    if *is_ok == expect_ok {
        Some(source.clone())
    } else {
        None
    }
}

fn effect_from_rvalue(value: &Rvalue) -> Option<(EffectKind, Option<Value>)> {
    match value {
        Rvalue::ResultOk { value } => Some((EffectKind::ResultOkWrap, Some(value.clone()))),
        Rvalue::ResultErr { value } => Some((EffectKind::ResultErrWrap, Some(value.clone()))),
        Rvalue::ResultUnwrap { value } => Some((EffectKind::ResultUnwrap, Some(value.clone()))),
        Rvalue::ResultErrUnwrap { value } => {
            Some((EffectKind::ResultErrUnwrap, Some(value.clone())))
        }
        Rvalue::ResultIsOk { value } => Some((EffectKind::ResultIsOk, Some(value.clone()))),
        _ => None,
    }
}

fn stmt_span(stmt: &Stmt) -> TextRange {
    match stmt {
        Stmt::Phi { span, .. }
        | Stmt::Assign { span, .. }
        | Stmt::SetField { span, .. }
        | Stmt::RcInc { span, .. }
        | Stmt::RcDec { span, .. }
        | Stmt::Await { span, .. }
        | Stmt::Fire { span, .. }
        | Stmt::IterInit { span, .. }
        | Stmt::IterNext { span, .. } => *span,
    }
}
