use crate::diag::catalog::MirDiagKind;
use crate::mir::ir::{
    CallKind, CallTarget, MirFunction, MirModule, MirType, Place, Rvalue, Stmt, Terminator, Value,
};
use miette::SourceSpan;
use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub struct MirValidationError {
    pub kind: MirDiagKind,
    pub message: String,
    pub span: Option<SourceSpan>,
}

pub fn validate_module(module: &MirModule) -> Vec<MirValidationError> {
    let mut errors = Vec::new();
    for func in &module.functions {
        for mut err in validate_function(func) {
            err.message = format!("function '{}': {}", func.name, err.message);
            errors.push(err);
        }
    }
    errors
}

fn validate_function(func: &MirFunction) -> Vec<MirValidationError> {
    let mut errors = Vec::new();
    let mut has_await = false;
    for (idx, block) in func.blocks.iter().enumerate() {
        if matches!(block.terminator, Terminator::Unreachable { .. }) {
            errors.push(MirValidationError {
                kind: MirDiagKind::MissingTerminator,
                message: format!("block {idx} has no terminator"),
                span: None,
            });
        }
        if block
            .stmts
            .iter()
            .any(|stmt| matches!(stmt, Stmt::Await { .. }))
        {
            has_await = true;
        }
    }
    if has_await && !func.suspendable {
        errors.push(MirValidationError {
            kind: MirDiagKind::AwaitSuspendableMismatch,
            message: "await present but function is not marked suspendable".to_string(),
            span: None,
        });
    }

    let reachable = compute_reachable(func);
    let preds = compute_predecessors(func, &reachable);
    let (in_states, out_states) = compute_definite_defs(func, &reachable, &preds);
    for (idx, block) in func.blocks.iter().enumerate() {
        if !reachable[idx] {
            continue;
        }
        let mut seen_non_phi = false;
        for stmt in &block.stmts {
            match stmt {
                Stmt::Phi { .. } => {
                    if seen_non_phi {
                        errors.push(MirValidationError {
                            kind: MirDiagKind::PhiOrder,
                            message: format!("phi after non-phi in block {idx}"),
                            span: None,
                        });
                    }
                }
                _ => seen_non_phi = true,
            }
        }
        let mut defined = in_states[idx].clone();
        for stmt in &block.stmts {
            if let Stmt::Phi { sources, .. } = stmt {
                validate_phi_sources(func, idx, &preds[idx], sources, &out_states, &mut errors);
                apply_stmt_defs(stmt, &mut defined);
                continue;
            }
            check_stmt_uses(func, idx, stmt, &defined, &mut errors);
            apply_stmt_defs(stmt, &mut defined);
        }
        check_terminator_uses(func, idx, &block.terminator, &defined, &mut errors);
    }
    errors
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DefState {
    locals: Vec<bool>,
    temps: Vec<bool>,
}

impl DefState {
    fn new(locals: usize, temps: usize, value: bool) -> Self {
        Self {
            locals: vec![value; locals],
            temps: vec![value; temps],
        }
    }

    fn set_place(&mut self, place: &Place) {
        match place {
            Place::Local(id) => {
                if let Some(slot) = self.locals.get_mut(id.0) {
                    *slot = true;
                }
            }
            Place::Temp(id) => {
                if let Some(slot) = self.temps.get_mut(id.0) {
                    *slot = true;
                }
            }
        }
    }

    fn is_defined(&self, value: &Value) -> bool {
        match value {
            Value::Const(_) => true,
            Value::Local(id) => self.locals.get(id.0).copied().unwrap_or(false),
            Value::Temp(id) => self.temps.get(id.0).copied().unwrap_or(false),
        }
    }

    fn intersect_from(&mut self, other: &DefState) {
        for (slot, other_slot) in self.locals.iter_mut().zip(&other.locals) {
            *slot &= *other_slot;
        }
        for (slot, other_slot) in self.temps.iter_mut().zip(&other.temps) {
            *slot &= *other_slot;
        }
    }
}

fn compute_reachable(func: &MirFunction) -> Vec<bool> {
    let mut reachable = vec![false; func.blocks.len()];
    let mut queue = VecDeque::new();
    queue.push_back(func.entry.0);
    while let Some(idx) = queue.pop_front() {
        if reachable[idx] {
            continue;
        }
        reachable[idx] = true;
        for succ in terminator_successors(&func.blocks[idx].terminator) {
            if succ.0 < func.blocks.len() {
                queue.push_back(succ.0);
            }
        }
    }
    reachable
}

fn compute_predecessors(func: &MirFunction, reachable: &[bool]) -> Vec<Vec<usize>> {
    let mut preds = vec![Vec::new(); func.blocks.len()];
    for (idx, block) in func.blocks.iter().enumerate() {
        if !reachable[idx] {
            continue;
        }
        for succ in terminator_successors(&block.terminator) {
            if succ.0 < func.blocks.len() {
                preds[succ.0].push(idx);
            }
        }
    }
    preds
}

fn compute_definite_defs(
    func: &MirFunction,
    reachable: &[bool],
    preds: &[Vec<usize>],
) -> (Vec<DefState>, Vec<DefState>) {
    let locals_len = func.locals.len();
    let temps_len = func.temps.len();
    let mut in_states = vec![DefState::new(locals_len, temps_len, true); func.blocks.len()];
    let mut out_states = vec![DefState::new(locals_len, temps_len, true); func.blocks.len()];
    let mut entry_state = DefState::new(locals_len, temps_len, false);
    for param in &func.params {
        entry_state.set_place(&Place::Local(*param));
    }
    if func.entry.0 < in_states.len() {
        in_states[func.entry.0] = entry_state.clone();
        out_states[func.entry.0] = entry_state.clone();
    }

    let mut changed = true;
    while changed {
        changed = false;
        for (idx, block) in func.blocks.iter().enumerate() {
            if !reachable[idx] {
                continue;
            }
            let mut in_state = if idx == func.entry.0 {
                entry_state.clone()
            } else if preds[idx].is_empty() {
                DefState::new(locals_len, temps_len, false)
            } else {
                let mut state = out_states[preds[idx][0]].clone();
                for pred in &preds[idx][1..] {
                    state.intersect_from(&out_states[*pred]);
                }
                state
            };
            if in_state != in_states[idx] {
                in_states[idx] = in_state.clone();
                changed = true;
            }
            apply_block_defs(block, &mut in_state);
            if in_state != out_states[idx] {
                out_states[idx] = in_state;
                changed = true;
            }
        }
    }
    (in_states, out_states)
}

fn apply_block_defs(block: &crate::mir::ir::BasicBlock, state: &mut DefState) {
    for stmt in &block.stmts {
        apply_stmt_defs(stmt, state);
    }
}

fn apply_stmt_defs(stmt: &Stmt, state: &mut DefState) {
    match stmt {
        Stmt::Phi { place, .. } => state.set_place(place),
        Stmt::Assign { place, .. } => state.set_place(place),
        Stmt::Await { dst, .. } => state.set_place(dst),
        Stmt::IterInit { dst, .. } => state.set_place(dst),
        Stmt::IterNext {
            dst_value,
            dst_done,
            ..
        } => {
            state.set_place(dst_value);
            state.set_place(dst_done);
        }
        Stmt::SetField { .. }
        | Stmt::RcInc { .. }
        | Stmt::RcDec { .. }
        | Stmt::Fire { .. }
        | Stmt::ActorFire { .. } => {}
    }
}

fn check_stmt_uses(
    func: &MirFunction,
    block_idx: usize,
    stmt: &Stmt,
    defined: &DefState,
    errors: &mut Vec<MirValidationError>,
) {
    match stmt {
        Stmt::Phi { .. } => {}
        Stmt::Assign { place, value, .. } => {
            if let Rvalue::BuildList { alloc, .. }
            | Rvalue::BuildMap { alloc, .. }
            | Rvalue::StringInterp { alloc, .. }
            | Rvalue::StrConcat { alloc, .. } = value
                && matches!(alloc, crate::mir::ir::AllocKind::LocalTemp)
                && !matches!(place, Place::Temp(_))
            {
                errors.push(MirValidationError {
                    kind: MirDiagKind::Internal,
                    message: format!(
                        "local-temp alloc assigned to non-temp place in block {block_idx}"
                    ),
                    span: None,
                });
            }
            check_rvalue_uses(func, block_idx, value, defined, errors);
        }
        Stmt::SetField { base, value, .. } => {
            check_value_use(func, block_idx, base, defined, errors);
            check_value_use(func, block_idx, value, defined, errors);
        }
        Stmt::RcInc { value, .. } | Stmt::RcDec { value, .. } => {
            check_value_use(func, block_idx, value, defined, errors);
            if let Some(kind) = value_type(func, value) {
                if !kind.is_ref() {
                    errors.push(MirValidationError {
                        kind: MirDiagKind::Internal,
                        span: None,
                        message: format!("rc op on non-ref value in block {block_idx}"),
                    });
                }
            } else if matches!(value, Value::Const(_))
                && (matches!(value, Value::Const(crate::hir::Literal::Boolean(_)))
                    || matches!(value, Value::Const(crate::hir::Literal::Nil))
                    || matches!(value, Value::Const(crate::hir::Literal::Float(_))))
            {
                errors.push(MirValidationError {
                    kind: MirDiagKind::Internal,
                    span: None,
                    message: format!("rc op on non-ref literal in block {block_idx}"),
                });
            }
        }
        Stmt::Await { pending, .. } | Stmt::Fire { pending, .. } => {
            check_value_use(func, block_idx, pending, defined, errors);
        }
        Stmt::ActorFire { target, args, .. } => {
            if let CallTarget::Method { receiver, .. } = target {
                check_value_use(func, block_idx, receiver, defined, errors);
            }
            for arg in args {
                check_value_use(func, block_idx, arg, defined, errors);
            }
        }
        Stmt::IterInit { iterable, .. } => {
            check_value_use(func, block_idx, iterable, defined, errors);
        }
        Stmt::IterNext { iter, .. } => {
            check_value_use(func, block_idx, iter, defined, errors);
        }
    }
}

fn validate_phi_sources(
    _func: &MirFunction,
    block_idx: usize,
    preds: &[usize],
    sources: &[(crate::mir::ir::BlockId, Value)],
    out_states: &[DefState],
    errors: &mut Vec<MirValidationError>,
) {
    if preds.is_empty() {
        if !sources.is_empty() {
            errors.push(MirValidationError {
                kind: MirDiagKind::Internal,
                span: None,
                message: format!("phi in entry block {block_idx} has sources"),
            });
        }
        return;
    }
    let mut seen = std::collections::HashSet::new();
    for (pred, value) in sources {
        if pred.0 >= out_states.len() {
            errors.push(MirValidationError {
                kind: MirDiagKind::Internal,
                span: None,
                message: format!(
                    "phi source references invalid block {} in {block_idx}",
                    pred.0
                ),
            });
            continue;
        }
        if !preds.contains(&pred.0) {
            errors.push(MirValidationError {
                kind: MirDiagKind::Internal,
                span: None,
                message: format!(
                    "phi source from non-predecessor block {} in {block_idx}",
                    pred.0
                ),
            });
        }
        if !seen.insert(pred.0) {
            errors.push(MirValidationError {
                kind: MirDiagKind::Internal,
                span: None,
                message: format!(
                    "phi has duplicate source from block {} in {block_idx}",
                    pred.0
                ),
            });
        }
        let _ = value;
    }
    for pred in preds {
        if !seen.contains(pred) {
            errors.push(MirValidationError {
                kind: MirDiagKind::Internal,
                span: None,
                message: format!("phi missing source from block {} in {block_idx}", pred),
            });
        }
    }
}

fn check_rvalue_uses(
    func: &MirFunction,
    block_idx: usize,
    value: &Rvalue,
    defined: &DefState,
    errors: &mut Vec<MirValidationError>,
) {
    match value {
        Rvalue::Use(value)
        | Rvalue::ResultOk { value }
        | Rvalue::ResultErr { value }
        | Rvalue::ResultIsOk { value }
        | Rvalue::ResultUnwrap { value }
        | Rvalue::ResultErrUnwrap { value }
        | Rvalue::Crash { value } => {
            check_value_use(func, block_idx, value, defined, errors);
        }
        Rvalue::Unary { operand, .. } => {
            check_value_use(func, block_idx, operand, defined, errors);
        }
        Rvalue::Binary { lhs, rhs, .. } => {
            check_value_use(func, block_idx, lhs, defined, errors);
            check_value_use(func, block_idx, rhs, defined, errors);
        }
        Rvalue::GetField { base, .. } => {
            check_value_use(func, block_idx, base, defined, errors);
        }
        Rvalue::Call { kind, target, args } => {
            match target {
                CallTarget::Function(_) => {}
                CallTarget::Method {
                    receiver,
                    method_id,
                    ..
                } => {
                    check_value_use(func, block_idx, receiver, defined, errors);
                    if matches!(kind, CallKind::Actor) {
                        if method_id.is_none() {
                            errors.push(MirValidationError {
                                kind: MirDiagKind::Internal,
                                span: None,
                                message: format!(
                                    "actor call missing method id in block {block_idx}"
                                ),
                            });
                        }
                        if let Some(receiver_ty) = value_type(func, receiver)
                            && !matches!(receiver_ty, MirType::Actor(_))
                        {
                            errors.push(MirValidationError {
                                kind: MirDiagKind::Internal,
                                span: None,
                                message: format!(
                                    "actor call on non-actor value in block {block_idx}"
                                ),
                            });
                        }
                    }
                }
                CallTarget::GuardedInterface { fast_paths, .. } => {
                    if !matches!(kind, CallKind::Sync) {
                        errors.push(MirValidationError {
                            kind: MirDiagKind::Internal,
                            span: None,
                            message: format!(
                                "guarded interface call must be sync in block {block_idx}"
                            ),
                        });
                    }
                    if args.is_empty() {
                        errors.push(MirValidationError {
                            kind: MirDiagKind::Internal,
                            span: None,
                            message: format!(
                                "guarded interface call missing receiver arg in block {block_idx}"
                            ),
                        });
                    } else {
                        check_value_use(func, block_idx, &args[0], defined, errors);
                    }
                    if fast_paths.is_empty() {
                        errors.push(MirValidationError {
                            kind: MirDiagKind::Internal,
                            span: None,
                            message: format!(
                                "guarded interface call missing fast paths in block {block_idx}"
                            ),
                        });
                    }
                }
                CallTarget::Indirect(value) => {
                    check_value_use(func, block_idx, value, defined, errors);
                    if matches!(kind, CallKind::Actor) {
                        errors.push(MirValidationError {
                            kind: MirDiagKind::Internal,
                            span: None,
                            message: format!("actor call on indirect target in block {block_idx}"),
                        });
                    }
                }
            }
            for arg in args {
                check_value_use(func, block_idx, arg, defined, errors);
            }
        }
        Rvalue::Spawn {
            target, instance, ..
        } => {
            check_value_use(func, block_idx, target, defined, errors);
            check_value_use(func, block_idx, instance, defined, errors);
        }
        Rvalue::PoolNew { handles, .. } => {
            check_value_use(func, block_idx, handles, defined, errors);
        }
        Rvalue::BuildList { items, .. } => {
            for item in items {
                check_value_use(func, block_idx, item, defined, errors);
            }
        }
        Rvalue::BuildMap { items, .. } => {
            for (key, value) in items {
                check_value_use(func, block_idx, key, defined, errors);
                check_value_use(func, block_idx, value, defined, errors);
            }
        }
        Rvalue::StrConcat { parts, .. } => {
            for value in parts {
                check_value_use(func, block_idx, value, defined, errors);
            }
        }
        Rvalue::StringInterp { parts, .. } => {
            for part in parts {
                if let crate::mir::ir::StringPartValue::Value(value) = part {
                    check_value_use(func, block_idx, value, defined, errors);
                }
            }
        }
        Rvalue::ClassInit { .. } => {}
    }
}

fn check_terminator_uses(
    func: &MirFunction,
    block_idx: usize,
    term: &Terminator,
    defined: &DefState,
    errors: &mut Vec<MirValidationError>,
) {
    match term {
        Terminator::Return {
            value: Some(value), ..
        } => {
            check_value_use(func, block_idx, value, defined, errors);
        }
        Terminator::Branch { cond, .. } => {
            check_value_use(func, block_idx, cond, defined, errors);
        }
        Terminator::Switch { scrutinee, .. } => {
            check_value_use(func, block_idx, scrutinee, defined, errors);
        }
        Terminator::Return { value: None, .. }
        | Terminator::Jump { .. }
        | Terminator::Unreachable { .. } => {}
    }
}

fn check_value_use(
    func: &MirFunction,
    block_idx: usize,
    value: &Value,
    defined: &DefState,
    errors: &mut Vec<MirValidationError>,
) {
    if !defined.is_defined(value) {
        errors.push(MirValidationError {
            kind: MirDiagKind::Internal,
            span: None,
            message: format!(
                "use-before-def in block {block_idx}: {}",
                value_label(func, value)
            ),
        });
    }
}

fn value_label(func: &MirFunction, value: &Value) -> String {
    match value {
        Value::Const(_) => "const".to_string(),
        Value::Local(id) => func
            .locals
            .get(id.0)
            .map(|local| format!("local '{}'", local.name))
            .unwrap_or_else(|| format!("local {}", id.0)),
        Value::Temp(id) => format!("temp {}", id.0),
    }
}

fn value_type(func: &MirFunction, value: &Value) -> Option<MirType> {
    match value {
        Value::Const(lit) => Some(match lit {
            crate::hir::Literal::Integer(_) => MirType::Integer,
            crate::hir::Literal::Float(_) => MirType::Float,
            crate::hir::Literal::Boolean(_) => MirType::Boolean,
            crate::hir::Literal::String(_) => MirType::String,
            crate::hir::Literal::Nil => MirType::Nil,
        }),
        Value::Local(id) => func.locals.get(id.0).map(|local| local.ty.clone()),
        Value::Temp(id) => func.temps.get(id.0).map(|temp| temp.ty.clone()),
    }
    .and_then(|ty| {
        if matches!(ty, MirType::Unknown) {
            None
        } else {
            Some(ty)
        }
    })
}

fn terminator_successors(term: &Terminator) -> Vec<crate::mir::ir::BlockId> {
    match term {
        Terminator::Return { .. } | Terminator::Unreachable { .. } => Vec::new(),
        Terminator::Jump { target, .. } => vec![*target],
        Terminator::Branch {
            then_target,
            else_target,
            ..
        } => vec![*then_target, *else_target],
        Terminator::Switch { cases, default, .. } => {
            let mut targets = Vec::with_capacity(cases.len() + 1);
            for (_, target) in cases {
                targets.push(*target);
            }
            targets.push(*default);
            targets
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mir::ir::{
        AllocKind, BasicBlock, BlockId, Local, LocalId, MirFunction, MirType, Place,
        PortableAbiType, Rvalue, Stmt, Temp, Terminator, Value,
    };
    use rowan::TextRange;
    use smol_str::SmolStr;

    #[test]
    fn local_temp_alloc_must_target_temp() {
        let stmt = Stmt::Assign {
            place: Place::Local(LocalId(0)),
            value: Rvalue::BuildList {
                items: Vec::new(),
                alloc: AllocKind::LocalTemp,
            },
            span: TextRange::new(0.into(), 0.into()),
        };
        let func = MirFunction {
            name: SmolStr::new("f"),
            params: Vec::new(),
            abi_params: Vec::new(),
            abi_return: PortableAbiType::Value,
            locals: vec![Local {
                name: SmolStr::new("x"),
                mutable: true,
                ty: MirType::Unknown,
            }],
            temps: vec![Temp {
                ty: MirType::Unknown,
            }],
            blocks: vec![BasicBlock {
                stmts: vec![stmt],
                terminator: Terminator::Return {
                    value: Some(Value::Const(crate::hir::Literal::Nil)),
                    span: TextRange::new(0.into(), 0.into()),
                },
            }],
            entry: BlockId(0),
            suspendable: false,
        };
        let errors = validate_function(&func);
        assert!(
            errors.iter().any(|err| err
                .message
                .contains("local-temp alloc assigned to non-temp")),
            "expected local-temp alloc validation error"
        );
    }

    #[test]
    fn phi_requires_all_sources() {
        let entry = BasicBlock {
            stmts: vec![],
            terminator: Terminator::Jump {
                target: BlockId(1),
                span: TextRange::new(0.into(), 0.into()),
            },
        };
        let join = BasicBlock {
            stmts: vec![Stmt::Phi {
                place: Place::Local(LocalId(0)),
                sources: Vec::new(),
                original: LocalId(0),
                span: TextRange::new(0.into(), 0.into()),
            }],
            terminator: Terminator::Return {
                value: Some(Value::Local(LocalId(0))),
                span: TextRange::new(0.into(), 0.into()),
            },
        };
        let func = MirFunction {
            name: SmolStr::new("f"),
            params: Vec::new(),
            abi_params: Vec::new(),
            abi_return: PortableAbiType::Value,
            locals: vec![Local {
                name: SmolStr::new("x"),
                mutable: false,
                ty: MirType::Unknown,
            }],
            temps: vec![],
            blocks: vec![entry, join],
            entry: BlockId(0),
            suspendable: false,
        };
        let errors = validate_function(&func);
        assert!(
            errors
                .iter()
                .any(|err| err.message.contains("phi missing source")),
            "expected phi source validation error"
        );
    }
}
