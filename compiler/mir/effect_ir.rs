use crate::hir::Literal;
use crate::mir::ir::{MirFunction, Place, Rvalue, Stmt, Value};
use rowan::TextRange;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

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
    pub cross_block_rewrites: usize,
    pub blocked_rewrite_reasons: BTreeMap<String, usize>,
    pub reconstruction: ReconstructionMetadataMap,
}

#[derive(Debug, Clone)]
struct ResultSource {
    is_ok: bool,
    value: Value,
    def_key: EffectStmtKey,
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
                Stmt::ActorFire { span, .. } => {
                    out.nodes.push(EffectNode {
                        key,
                        kind: EffectKind::Fire,
                        place: None,
                        input: None,
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
    let preds = block_predecessors(func);
    let doms = compute_dominators(func, &preds);

    let (result_sources, aliases) = collect_result_sources(func);
    let mut report = EffectAnnihilationReport {
        reconstruction,
        ..EffectAnnihilationReport::default()
    };

    apply_annihilation_stage(func, &result_sources, &aliases, &doms, &mut report);

    // Second stage catches wrappers forwarded through temp aliases after first-stage rewrites.
    let (result_sources, aliases) = collect_result_sources(func);
    apply_annihilation_stage(func, &result_sources, &aliases, &doms, &mut report);

    report
}

fn apply_annihilation_stage(
    func: &mut MirFunction,
    result_sources: &HashMap<usize, ResultSource>,
    aliases: &HashMap<usize, usize>,
    doms: &[Vec<bool>],
    report: &mut EffectAnnihilationReport,
) {
    for (block_idx, block) in func.blocks.iter_mut().enumerate() {
        for (stmt_idx, stmt) in block.stmts.iter_mut().enumerate() {
            let key = EffectStmtKey {
                block: block_idx,
                stmt: stmt_idx,
            };
            let Stmt::Assign { value, .. } = stmt else {
                continue;
            };

            match value {
                Rvalue::ResultUnwrap { value: inner } => {
                    if let Some(source) = resolve_result_source(inner, result_sources, aliases) {
                        rewrite_from_source(value, true, key, source, doms, report);
                    } else {
                        bump_reason(report, "missing_result_source");
                    }
                }
                Rvalue::ResultErrUnwrap { value: inner } => {
                    if let Some(source) = resolve_result_source(inner, result_sources, aliases) {
                        rewrite_from_source(value, false, key, source, doms, report);
                    } else {
                        bump_reason(report, "missing_result_source");
                    }
                }
                Rvalue::ResultIsOk { value: inner } => {
                    if let Some(source) = resolve_result_source(inner, result_sources, aliases) {
                        if !dominates_stmt(source.def_key, key, doms) {
                            bump_reason(report, "blocked_dominance");
                            continue;
                        }
                        *value = Rvalue::Use(Value::Const(Literal::Boolean(source.is_ok)));
                        report.rewritten_statements += 1;
                        if source.def_key.block != key.block {
                            report.cross_block_rewrites += 1;
                        }
                    } else {
                        bump_reason(report, "missing_result_source");
                    }
                }
                _ => {}
            }
        }
    }
}

fn rewrite_from_source(
    rvalue: &mut Rvalue,
    expect_ok: bool,
    use_key: EffectStmtKey,
    source: &ResultSource,
    doms: &[Vec<bool>],
    report: &mut EffectAnnihilationReport,
) {
    if source.is_ok != expect_ok {
        bump_reason(report, "blocked_variant_mismatch");
        return;
    }
    if !dominates_stmt(source.def_key, use_key, doms) {
        bump_reason(report, "blocked_dominance");
        return;
    }
    *rvalue = Rvalue::Use(source.value.clone());
    report.rewritten_statements += 1;
    if source.def_key.block != use_key.block {
        report.cross_block_rewrites += 1;
    }
}

fn resolve_result_source<'a>(
    value: &Value,
    result_sources: &'a HashMap<usize, ResultSource>,
    aliases: &HashMap<usize, usize>,
) -> Option<&'a ResultSource> {
    let Value::Temp(temp) = value else {
        return None;
    };

    let mut current = temp.0;
    let mut seen = HashSet::new();
    loop {
        if !seen.insert(current) {
            return None;
        }
        if let Some(source) = result_sources.get(&current) {
            return Some(source);
        }
        let Some(next) = aliases.get(&current) else {
            return None;
        };
        current = *next;
    }
}

fn collect_result_sources(
    func: &MirFunction,
) -> (HashMap<usize, ResultSource>, HashMap<usize, usize>) {
    let mut result_sources: HashMap<usize, ResultSource> = HashMap::new();
    let mut aliases: HashMap<usize, usize> = HashMap::new();

    for (block_idx, block) in func.blocks.iter().enumerate() {
        for (stmt_idx, stmt) in block.stmts.iter().enumerate() {
            let Stmt::Assign { place, value, .. } = stmt else {
                continue;
            };
            let Place::Temp(temp) = place else {
                continue;
            };
            let key = EffectStmtKey {
                block: block_idx,
                stmt: stmt_idx,
            };

            match value {
                Rvalue::ResultOk { value } => {
                    result_sources.insert(
                        temp.0,
                        ResultSource {
                            is_ok: true,
                            value: value.clone(),
                            def_key: key,
                        },
                    );
                }
                Rvalue::ResultErr { value } => {
                    result_sources.insert(
                        temp.0,
                        ResultSource {
                            is_ok: false,
                            value: value.clone(),
                            def_key: key,
                        },
                    );
                }
                Rvalue::Use(Value::Temp(alias)) => {
                    aliases.insert(temp.0, alias.0);
                }
                _ => {}
            }
        }
    }

    (result_sources, aliases)
}

fn dominates_stmt(def: EffectStmtKey, use_key: EffectStmtKey, doms: &[Vec<bool>]) -> bool {
    if def.block == use_key.block {
        return def.stmt < use_key.stmt;
    }
    doms.get(use_key.block)
        .and_then(|row| row.get(def.block))
        .copied()
        .unwrap_or(false)
}

fn bump_reason(report: &mut EffectAnnihilationReport, reason: &str) {
    let entry = report
        .blocked_rewrite_reasons
        .entry(reason.to_string())
        .or_insert(0);
    *entry += 1;
}

fn block_predecessors(func: &MirFunction) -> Vec<Vec<usize>> {
    let mut preds = vec![Vec::new(); func.blocks.len()];
    for (idx, block) in func.blocks.iter().enumerate() {
        for succ in block_successors(block) {
            if let Some(slot) = preds.get_mut(succ) {
                slot.push(idx);
            }
        }
    }
    preds
}

fn compute_dominators(func: &MirFunction, preds: &[Vec<usize>]) -> Vec<Vec<bool>> {
    let blocks_len = func.blocks.len();
    let entry = func.entry.0;
    let mut reachable = vec![false; blocks_len];
    let mut queue = VecDeque::new();
    queue.push_back(entry);
    while let Some(block) = queue.pop_front() {
        if !reachable[block] {
            reachable[block] = true;
            for succ in block_successors(&func.blocks[block]) {
                queue.push_back(succ);
            }
        }
    }

    let mut doms = vec![vec![true; blocks_len]; blocks_len];
    for b in 0..blocks_len {
        if b == entry {
            let mut row = vec![false; blocks_len];
            row[entry] = true;
            doms[b] = row;
        } else if !reachable[b] {
            doms[b] = vec![false; blocks_len];
        }
    }

    let mut changed = true;
    while changed {
        changed = false;
        for b in 0..blocks_len {
            if b == entry || !reachable[b] || preds[b].is_empty() {
                continue;
            }
            let mut new_dom = vec![true; blocks_len];
            for pred in &preds[b] {
                for (i, bit) in new_dom.iter_mut().enumerate() {
                    *bit &= doms[*pred][i];
                }
            }
            new_dom[b] = true;
            if new_dom != doms[b] {
                doms[b] = new_dom;
                changed = true;
            }
        }
    }
    doms
}

fn block_successors(block: &crate::mir::ir::BasicBlock) -> Vec<usize> {
    match &block.terminator {
        crate::mir::ir::Terminator::Return { .. }
        | crate::mir::ir::Terminator::Unreachable { .. } => Vec::new(),
        crate::mir::ir::Terminator::Jump { target, .. } => vec![target.0],
        crate::mir::ir::Terminator::Branch {
            then_target,
            else_target,
            ..
        } => vec![then_target.0, else_target.0],
        crate::mir::ir::Terminator::Switch { cases, default, .. } => {
            let mut out = Vec::with_capacity(cases.len() + 1);
            for (_, block) in cases {
                out.push(block.0);
            }
            out.push(default.0);
            out.sort_unstable();
            out.dedup();
            out
        }
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
        | Stmt::ActorFire { span, .. }
        | Stmt::IterInit { span, .. }
        | Stmt::IterNext { span, .. } => *span,
    }
}
