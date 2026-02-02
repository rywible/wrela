use crate::hir::{BinaryOp, Literal, UnaryOp};
use crate::mir::ir::{
    CallKind, CallTarget, MirFunction, MirType, Place, Rvalue, Stmt, Terminator, Value,
};
use rowan::TextRange;
use std::collections::HashSet;

pub fn run_function_passes(func: &mut MirFunction) {
    constant_fold(func);
    simplify_branches(func);
    dead_code_elim(func);
    insert_rc(func);
}

pub fn constant_fold(func: &mut MirFunction) {
    for block in &mut func.blocks {
        for stmt in &mut block.stmts {
            if let Stmt::Assign { value, .. } = stmt {
                if let Some(lit) = fold_rvalue(value) {
                    *value = Rvalue::Use(Value::Const(lit));
                }
            }
        }
        if let Terminator::Branch {
            cond,
            then_target,
            else_target,
            span,
        } = &block.terminator
        {
            if let Value::Const(Literal::Bool(flag)) = cond {
                block.terminator = if *flag {
                    Terminator::Jump {
                        target: *then_target,
                        span: *span,
                    }
                } else {
                    Terminator::Jump {
                        target: *else_target,
                        span: *span,
                    }
                };
            }
        }
    }
}

pub fn simplify_branches(func: &mut MirFunction) {
    for block in &mut func.blocks {
        if let Terminator::Branch {
            cond,
            then_target,
            else_target,
            span,
        } = &block.terminator
        {
            if let Value::Const(Literal::Bool(flag)) = cond {
                block.terminator = if *flag {
                    Terminator::Jump {
                        target: *then_target,
                        span: *span,
                    }
                } else {
                    Terminator::Jump {
                        target: *else_target,
                        span: *span,
                    }
                };
            }
        }
    }
}

pub fn dead_code_elim(func: &mut MirFunction) {
    let used = collect_used_values(func);
    for block in &mut func.blocks {
        let mut new_stmts = Vec::with_capacity(block.stmts.len());
        for stmt in block.stmts.drain(..) {
            match &stmt {
                Stmt::Assign { place, value, .. } => {
                    if is_pure_rvalue(value) {
                        if !place_is_used(place, &used) {
                            continue;
                        }
                    }
                }
                Stmt::RcInc { .. } | Stmt::RcDec { .. } => {}
                _ => {}
            }
            new_stmts.push(stmt);
        }
        block.stmts = new_stmts;
    }
}

fn fold_rvalue(value: &Rvalue) -> Option<Literal> {
    match value {
        Rvalue::Unary { op, operand } => match (op, operand) {
            (UnaryOp::Neg, Value::Const(Literal::Int(v))) => Some(Literal::Int(-v)),
            (UnaryOp::Neg, Value::Const(Literal::Float(v))) => Some(Literal::Float(-v)),
            (UnaryOp::Not, Value::Const(Literal::Bool(v))) => Some(Literal::Bool(!v)),
            _ => None,
        },
        Rvalue::Binary { op, lhs, rhs } => fold_binary(*op, lhs, rhs),
        _ => None,
    }
}

fn fold_binary(op: BinaryOp, lhs: &Value, rhs: &Value) -> Option<Literal> {
    match (lhs, rhs) {
        (Value::Const(Literal::Int(a)), Value::Const(Literal::Int(b))) => match op {
            BinaryOp::Add => Some(Literal::Int(a + b)),
            BinaryOp::Sub => Some(Literal::Int(a - b)),
            BinaryOp::Mul => Some(Literal::Int(a * b)),
            BinaryOp::Div => Some(Literal::Int(a / b)),
            BinaryOp::Mod => Some(Literal::Int(a % b)),
            BinaryOp::Eq => Some(Literal::Bool(a == b)),
            BinaryOp::Ne => Some(Literal::Bool(a != b)),
            BinaryOp::Lt => Some(Literal::Bool(a < b)),
            BinaryOp::Gt => Some(Literal::Bool(a > b)),
            BinaryOp::Le => Some(Literal::Bool(a <= b)),
            BinaryOp::Ge => Some(Literal::Bool(a >= b)),
            _ => None,
        },
        (Value::Const(Literal::Float(a)), Value::Const(Literal::Float(b))) => match op {
            BinaryOp::Add => Some(Literal::Float(a + b)),
            BinaryOp::Sub => Some(Literal::Float(a - b)),
            BinaryOp::Mul => Some(Literal::Float(a * b)),
            BinaryOp::Div => Some(Literal::Float(a / b)),
            BinaryOp::Eq => Some(Literal::Bool(a == b)),
            BinaryOp::Ne => Some(Literal::Bool(a != b)),
            BinaryOp::Lt => Some(Literal::Bool(a < b)),
            BinaryOp::Gt => Some(Literal::Bool(a > b)),
            BinaryOp::Le => Some(Literal::Bool(a <= b)),
            BinaryOp::Ge => Some(Literal::Bool(a >= b)),
            _ => None,
        },
        (Value::Const(Literal::Bool(a)), Value::Const(Literal::Bool(b))) => match op {
            BinaryOp::And => Some(Literal::Bool(*a && *b)),
            BinaryOp::Or => Some(Literal::Bool(*a || *b)),
            BinaryOp::Eq => Some(Literal::Bool(a == b)),
            BinaryOp::Ne => Some(Literal::Bool(a != b)),
            _ => None,
        },
        (Value::Const(Literal::String(a)), Value::Const(Literal::String(b))) => match op {
            BinaryOp::Eq => Some(Literal::Bool(a == b)),
            BinaryOp::Ne => Some(Literal::Bool(a != b)),
            _ => None,
        },
        _ => None,
    }
}

fn is_pure_rvalue(value: &Rvalue) -> bool {
    matches!(
        value,
        Rvalue::Use(_)
            | Rvalue::Unary { .. }
            | Rvalue::Binary { .. }
            | Rvalue::ResultOk { .. }
            | Rvalue::ResultErr { .. }
            | Rvalue::ResultIsOk { .. }
            | Rvalue::ResultUnwrap { .. }
            | Rvalue::ResultErrUnwrap { .. }
            | Rvalue::GetField { .. }
            | Rvalue::ClassInit { .. }
            | Rvalue::BuildList { .. }
            | Rvalue::BuildMap { .. }
            | Rvalue::StringInterp { .. }
    )
}

fn collect_used_values(func: &MirFunction) -> HashSet<usize> {
    let mut used = HashSet::new();
    for block in &func.blocks {
        for stmt in &block.stmts {
            match stmt {
                Stmt::Assign { value, .. } => collect_rvalue(value, &mut used),
                Stmt::SetField { base, value, .. } => {
                    collect_value(base, &mut used);
                    collect_value(value, &mut used);
                }
                Stmt::RcInc { value, .. } | Stmt::RcDec { value, .. } => {
                    collect_value(value, &mut used)
                }
                Stmt::Await { pending, .. } => collect_value(pending, &mut used),
                Stmt::Fire { pending, .. } => collect_value(pending, &mut used),
                Stmt::IterInit { iterable, .. } => collect_value(iterable, &mut used),
                Stmt::IterNext { iter, .. } => collect_value(iter, &mut used),
            }
        }
        match &block.terminator {
            Terminator::Return { value, .. } => {
                if let Some(value) = value {
                    collect_value(value, &mut used);
                }
            }
            Terminator::Jump { .. } => {}
            Terminator::Branch { cond, .. } => collect_value(cond, &mut used),
            Terminator::Switch { scrutinee, .. } => collect_value(scrutinee, &mut used),
            Terminator::Unreachable { .. } => {}
        }
    }
    used
}

fn collect_rvalue(value: &Rvalue, used: &mut HashSet<usize>) {
    match value {
        Rvalue::Use(value) => collect_value(value, used),
        Rvalue::Unary { operand, .. } => collect_value(operand, used),
        Rvalue::Binary { lhs, rhs, .. } => {
            collect_value(lhs, used);
            collect_value(rhs, used);
        }
        Rvalue::ResultOk { value }
        | Rvalue::ResultErr { value }
        | Rvalue::ResultIsOk { value }
        | Rvalue::ResultUnwrap { value }
        | Rvalue::ResultErrUnwrap { value }
        | Rvalue::Crash { value } => {
            collect_value(value, used);
        }
        Rvalue::GetField { base, .. } => collect_value(base, used),
        Rvalue::ClassInit { .. } => {}
        Rvalue::Call { target, args, .. } => {
            match target {
                CallTarget::Function(_) => {}
                CallTarget::Method { receiver, .. } => collect_value(receiver, used),
                CallTarget::Indirect(value) => collect_value(value, used),
            }
            for arg in args {
                collect_value(arg, used);
            }
        }
        Rvalue::Spawn {
            target, instance, ..
        } => {
            collect_value(target, used);
            collect_value(instance, used);
        }
        Rvalue::PoolNew { handles, .. } => {
            collect_value(handles, used);
        }
        Rvalue::BuildList { items } => {
            for item in items {
                collect_value(item, used);
            }
        }
        Rvalue::BuildMap { items } => {
            for (key, value) in items {
                collect_value(key, used);
                collect_value(value, used);
            }
        }
        Rvalue::StringInterp { parts } => {
            for part in parts {
                if let crate::mir::ir::StringPartValue::Value(value) = part {
                    collect_value(value, used);
                }
            }
        }
    }
}

fn collect_value(value: &Value, used: &mut HashSet<usize>) {
    match value {
        Value::Temp(temp) => {
            used.insert(temp.0);
        }
        Value::Local(_) | Value::Const(_) => {}
    }
}

fn place_is_used(place: &crate::mir::ir::Place, used: &HashSet<usize>) -> bool {
    match place {
        crate::mir::ir::Place::Temp(temp) => used.contains(&temp.0),
        crate::mir::ir::Place::Local(_) => true,
    }
}

fn insert_rc(func: &mut MirFunction) {
    let locals_len = func.locals.len();
    let temps_len = func.temps.len();
    let total = locals_len + temps_len;
    if total == 0 {
        return;
    }
    let types: Vec<MirType> = func
        .locals
        .iter()
        .map(|local| local.ty.clone())
        .chain(func.temps.iter().map(|temp| temp.ty.clone()))
        .collect();

    let succs = block_successors(func);
    let preds = block_predecessors(func, &succs);
    let (block_uses, block_defs) = collect_block_uses_defs(func, locals_len);
    let (live_in, live_out) = liveness(func.blocks.len(), &succs, &block_uses, &block_defs, total);
    let (init_in, _init_out) = definite_init(func, &preds, &block_defs, total);

    let mut next_temp_id = func.temps.len();
    for (block_idx, block) in func.blocks.iter_mut().enumerate() {
        let live_out_block = &live_out[block_idx];
        let mut live_after = Vec::with_capacity(block.stmts.len());
        let mut live = live_out_block.clone();
        for stmt in block.stmts.iter().rev() {
            live_after.push(live.clone());
            for def in stmt_defs(stmt, locals_len) {
                live[def] = false;
            }
            for use_idx in stmt_uses(stmt, locals_len) {
                live[use_idx] = true;
            }
        }
        live_after.reverse();

        let mut init = init_in[block_idx].clone();
        for i in 0..total {
            init[i] = init[i] && live_in[block_idx][i];
        }
        let mut new_stmts = Vec::with_capacity(block.stmts.len() * 2 + 2);
        for (idx, stmt) in block.stmts.iter().enumerate() {
            let span = stmt_span(stmt);
            let defs = stmt_defs(stmt, locals_len);
            let mut rvalue_uses = Vec::new();
            if let Stmt::Assign { value, .. } = stmt {
                collect_rvalue_uses(value, locals_len, &mut rvalue_uses);
            }
            for def in &defs {
                if init[*def] && !rvalue_uses.contains(def) && idx_is_ref(&types, *def) {
                    new_stmts.push(Stmt::RcDec {
                        value: value_from_idx(*def, locals_len),
                        span,
                    });
                }
            }
            if let Stmt::Assign { place, value, .. } = stmt {
                let idx = place_idx_opt(place, locals_len);
                if rvalue_uses.contains(&idx) {
                    let temp_id = crate::mir::ir::TempId(next_temp_id);
                    next_temp_id += 1;
                    func.temps.push(crate::mir::ir::Temp {
                        ty: MirType::Unknown,
                    });
                    new_stmts.push(Stmt::Assign {
                        place: Place::Temp(temp_id),
                        value: value.clone(),
                        span,
                    });
                    if init[idx] && idx_is_ref(&types, idx) {
                        new_stmts.push(Stmt::RcDec {
                            value: value_from_idx(idx, locals_len),
                            span,
                        });
                    }
                    new_stmts.push(Stmt::Assign {
                        place: place.clone(),
                        value: Rvalue::Use(Value::Temp(temp_id)),
                        span,
                    });
                    init[idx] = true;
                    continue;
                }
            }
            if let Stmt::Assign { value, .. } = stmt {
                handle_owning_uses(
                    value,
                    &live_after[idx],
                    &mut init,
                    locals_len,
                    span,
                    &mut new_stmts,
                    &types,
                );
            }
            if let Stmt::SetField { value, .. } = stmt {
                handle_owning_value(
                    value,
                    &live_after[idx],
                    &mut init,
                    locals_len,
                    span,
                    &mut new_stmts,
                    &types,
                );
            }
            new_stmts.push(stmt.clone());
            for def in defs {
                init[def] = true;
            }
        }

        let mut exclude = vec![false; total];
        if let Terminator::Return {
            value: Some(value), ..
        } = &block.terminator
        {
            if let Some(idx) = value_idx(value, locals_len) {
                exclude[idx] = true;
                init[idx] = false;
            }
        }
        let term_span = terminator_span(&block.terminator);
        for idx in 0..total {
            if init[idx] && !live_out_block[idx] && !exclude[idx] && idx_is_ref(&types, idx) {
                new_stmts.push(Stmt::RcDec {
                    value: value_from_idx(idx, locals_len),
                    span: term_span,
                });
            }
        }

        block.stmts = new_stmts;
    }
}

fn handle_owning_uses(
    value: &Rvalue,
    live_after: &Vec<bool>,
    init: &mut Vec<bool>,
    locals_len: usize,
    span: TextRange,
    out: &mut Vec<Stmt>,
    types: &[MirType],
) {
    match value {
        Rvalue::Use(value) => {
            if let Some(idx) = value_idx(value, locals_len) {
                if !live_after[idx] {
                    init[idx] = false;
                } else if idx_is_ref(types, idx) {
                    out.push(Stmt::RcInc {
                        value: value.clone(),
                        span,
                    });
                }
            }
        }
        Rvalue::Call { kind, target, args } => {
            if *kind != CallKind::Sync {
                return;
            }
            let mut moved = std::collections::HashSet::new();
            let mut values: Vec<&Value> = Vec::with_capacity(args.len() + 1);
            match target {
                CallTarget::Method { receiver, .. } => values.push(receiver),
                CallTarget::Indirect(value) => values.push(value),
                CallTarget::Function(_) => {}
            }
            values.extend(args.iter());
            for arg in values {
                if let Some(idx) = value_idx(arg, locals_len) {
                    if !live_after[idx] && !moved.contains(&idx) {
                        moved.insert(idx);
                        init[idx] = false;
                    } else if idx_is_ref(types, idx) {
                        out.push(Stmt::RcInc {
                            value: arg.clone(),
                            span,
                        });
                    }
                }
            }
        }
        _ => {}
    }
}

fn handle_owning_value(
    value: &Value,
    live_after: &Vec<bool>,
    init: &mut Vec<bool>,
    locals_len: usize,
    span: TextRange,
    out: &mut Vec<Stmt>,
    types: &[MirType],
) {
    if let Some(idx) = value_idx(value, locals_len) {
        if !live_after[idx] {
            init[idx] = false;
        } else if idx_is_ref(types, idx) {
            out.push(Stmt::RcInc {
                value: value.clone(),
                span,
            });
        }
    }
}

fn block_successors(func: &MirFunction) -> Vec<Vec<usize>> {
    let mut succs = vec![Vec::new(); func.blocks.len()];
    for (idx, block) in func.blocks.iter().enumerate() {
        let list = match &block.terminator {
            Terminator::Jump { target, .. } => vec![target.0],
            Terminator::Branch {
                then_target,
                else_target,
                ..
            } => vec![then_target.0, else_target.0],
            Terminator::Switch { cases, default, .. } => {
                let mut out = Vec::with_capacity(cases.len() + 1);
                for (_, target) in cases {
                    out.push(target.0);
                }
                out.push(default.0);
                out
            }
            Terminator::Return { .. } | Terminator::Unreachable { .. } => Vec::new(),
        };
        succs[idx] = list;
    }
    succs
}

fn block_predecessors(func: &MirFunction, succs: &[Vec<usize>]) -> Vec<Vec<usize>> {
    let mut preds = vec![Vec::new(); func.blocks.len()];
    for (idx, block_succs) in succs.iter().enumerate() {
        for succ in block_succs {
            preds[*succ].push(idx);
        }
    }
    preds
}

fn collect_block_uses_defs(
    func: &MirFunction,
    locals_len: usize,
) -> (Vec<Vec<bool>>, Vec<Vec<bool>>) {
    let total = locals_len + func.temps.len();
    let mut uses = vec![vec![false; total]; func.blocks.len()];
    let mut defs = vec![vec![false; total]; func.blocks.len()];
    for (idx, block) in func.blocks.iter().enumerate() {
        let mut block_uses = vec![false; total];
        let mut block_defs = vec![false; total];
        for stmt in &block.stmts {
            for use_idx in stmt_uses(stmt, locals_len) {
                if !block_defs[use_idx] {
                    block_uses[use_idx] = true;
                }
            }
            for def in stmt_defs(stmt, locals_len) {
                block_defs[def] = true;
            }
        }
        for use_idx in terminator_uses(&block.terminator, locals_len) {
            if !block_defs[use_idx] {
                block_uses[use_idx] = true;
            }
        }
        uses[idx] = block_uses;
        defs[idx] = block_defs;
    }
    (uses, defs)
}

fn liveness(
    blocks_len: usize,
    succs: &[Vec<usize>],
    uses: &[Vec<bool>],
    defs: &[Vec<bool>],
    total: usize,
) -> (Vec<Vec<bool>>, Vec<Vec<bool>>) {
    let mut live_in = vec![vec![false; total]; blocks_len];
    let mut live_out = vec![vec![false; total]; blocks_len];
    loop {
        let mut changed = false;
        for b in 0..blocks_len {
            let mut out = vec![false; total];
            for succ in &succs[b] {
                for i in 0..total {
                    out[i] |= live_in[*succ][i];
                }
            }
            let mut in_set = vec![false; total];
            for i in 0..total {
                in_set[i] = uses[b][i] || (out[i] && !defs[b][i]);
            }
            if in_set != live_in[b] || out != live_out[b] {
                live_in[b] = in_set;
                live_out[b] = out;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    (live_in, live_out)
}

fn definite_init(
    func: &MirFunction,
    preds: &[Vec<usize>],
    defs: &[Vec<bool>],
    total: usize,
) -> (Vec<Vec<bool>>, Vec<Vec<bool>>) {
    let mut init_in = vec![vec![false; total]; func.blocks.len()];
    let mut init_out = vec![vec![false; total]; func.blocks.len()];
    let mut entry_init = vec![false; total];
    for param in &func.params {
        entry_init[param.0] = true;
    }
    init_in[func.entry.0] = entry_init.clone();
    init_out[func.entry.0] = entry_init.clone();

    loop {
        let mut changed = false;
        for b in 0..func.blocks.len() {
            let in_set = if preds[b].is_empty() {
                if b == func.entry.0 {
                    entry_init.clone()
                } else {
                    vec![false; total]
                }
            } else {
                let mut acc = init_out[preds[b][0]].clone();
                for pred in preds[b].iter().skip(1) {
                    for i in 0..total {
                        acc[i] &= init_out[*pred][i];
                    }
                }
                acc
            };
            let mut out_set = in_set.clone();
            for i in 0..total {
                out_set[i] = out_set[i] || defs[b][i];
            }
            if in_set != init_in[b] || out_set != init_out[b] {
                init_in[b] = in_set;
                init_out[b] = out_set;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    (init_in, init_out)
}

fn stmt_defs(stmt: &Stmt, locals_len: usize) -> Vec<usize> {
    match stmt {
        Stmt::Assign { place, .. } => vec![place_idx(place, locals_len)],
        Stmt::Await { dst, .. } => vec![place_idx(dst, locals_len)],
        Stmt::IterInit { dst, .. } => vec![place_idx(dst, locals_len)],
        Stmt::IterNext {
            dst_value,
            dst_done,
            ..
        } => {
            vec![
                place_idx(dst_value, locals_len),
                place_idx(dst_done, locals_len),
            ]
        }
        Stmt::SetField { .. } | Stmt::RcInc { .. } | Stmt::RcDec { .. } | Stmt::Fire { .. } => {
            Vec::new()
        }
    }
}

fn stmt_uses(stmt: &Stmt, locals_len: usize) -> Vec<usize> {
    let mut out = Vec::new();
    match stmt {
        Stmt::Assign { value, .. } => collect_rvalue_uses(value, locals_len, &mut out),
        Stmt::SetField { base, value, .. } => {
            if let Some(idx) = value_idx(base, locals_len) {
                out.push(idx);
            }
            if let Some(idx) = value_idx(value, locals_len) {
                out.push(idx);
            }
        }
        Stmt::RcInc { value, .. } | Stmt::RcDec { value, .. } => {
            if let Some(idx) = value_idx(value, locals_len) {
                out.push(idx);
            }
        }
        Stmt::Await { pending, .. } => {
            if let Some(idx) = value_idx(pending, locals_len) {
                out.push(idx);
            }
        }
        Stmt::Fire { pending, .. } => {
            if let Some(idx) = value_idx(pending, locals_len) {
                out.push(idx);
            }
        }
        Stmt::IterInit { iterable, .. } => {
            if let Some(idx) = value_idx(iterable, locals_len) {
                out.push(idx);
            }
        }
        Stmt::IterNext { iter, .. } => {
            if let Some(idx) = value_idx(iter, locals_len) {
                out.push(idx);
            }
        }
    }
    out
}

fn collect_rvalue_uses(value: &Rvalue, locals_len: usize, out: &mut Vec<usize>) {
    match value {
        Rvalue::Use(value) => {
            if let Some(idx) = value_idx(value, locals_len) {
                out.push(idx);
            }
        }
        Rvalue::Unary { operand, .. } => {
            if let Some(idx) = value_idx(operand, locals_len) {
                out.push(idx);
            }
        }
        Rvalue::Binary { lhs, rhs, .. } => {
            if let Some(idx) = value_idx(lhs, locals_len) {
                out.push(idx);
            }
            if let Some(idx) = value_idx(rhs, locals_len) {
                out.push(idx);
            }
        }
        Rvalue::ResultOk { value }
        | Rvalue::ResultErr { value }
        | Rvalue::ResultIsOk { value }
        | Rvalue::ResultUnwrap { value }
        | Rvalue::ResultErrUnwrap { value }
        | Rvalue::Crash { value } => {
            if let Some(idx) = value_idx(value, locals_len) {
                out.push(idx);
            }
        }
        Rvalue::GetField { base, .. } => {
            if let Some(idx) = value_idx(base, locals_len) {
                out.push(idx);
            }
        }
        Rvalue::Call { target, args, .. } => {
            match target {
                CallTarget::Function(_) => {}
                CallTarget::Method { receiver, .. } => {
                    if let Some(idx) = value_idx(receiver, locals_len) {
                        out.push(idx);
                    }
                }
                CallTarget::Indirect(value) => {
                    if let Some(idx) = value_idx(value, locals_len) {
                        out.push(idx);
                    }
                }
            }
            for arg in args {
                if let Some(idx) = value_idx(arg, locals_len) {
                    out.push(idx);
                }
            }
        }
        Rvalue::Spawn {
            target, instance, ..
        } => {
            if let Some(idx) = value_idx(target, locals_len) {
                out.push(idx);
            }
            if let Some(idx) = value_idx(instance, locals_len) {
                out.push(idx);
            }
        }
        Rvalue::PoolNew { handles, .. } => {
            if let Some(idx) = value_idx(handles, locals_len) {
                out.push(idx);
            }
        }
        Rvalue::BuildList { items } => {
            for item in items {
                if let Some(idx) = value_idx(item, locals_len) {
                    out.push(idx);
                }
            }
        }
        Rvalue::BuildMap { items } => {
            for (key, value) in items {
                if let Some(idx) = value_idx(key, locals_len) {
                    out.push(idx);
                }
                if let Some(idx) = value_idx(value, locals_len) {
                    out.push(idx);
                }
            }
        }
        Rvalue::StringInterp { parts } => {
            for part in parts {
                if let crate::mir::ir::StringPartValue::Value(value) = part {
                    if let Some(idx) = value_idx(value, locals_len) {
                        out.push(idx);
                    }
                }
            }
        }
        Rvalue::ClassInit { .. } => {}
    }
}

fn terminator_uses(term: &Terminator, locals_len: usize) -> Vec<usize> {
    let mut out = Vec::new();
    match term {
        Terminator::Return { value, .. } => {
            if let Some(value) = value {
                if let Some(idx) = value_idx(value, locals_len) {
                    out.push(idx);
                }
            }
        }
        Terminator::Branch { cond, .. } => {
            if let Some(idx) = value_idx(cond, locals_len) {
                out.push(idx);
            }
        }
        Terminator::Switch { scrutinee, .. } => {
            if let Some(idx) = value_idx(scrutinee, locals_len) {
                out.push(idx);
            }
        }
        Terminator::Jump { .. } | Terminator::Unreachable { .. } => {}
    }
    out
}

fn value_idx(value: &Value, locals_len: usize) -> Option<usize> {
    match value {
        Value::Local(local) => Some(local.0),
        Value::Temp(temp) => Some(locals_len + temp.0),
        Value::Const(_) => None,
    }
}

fn place_idx(place: &Place, locals_len: usize) -> usize {
    match place {
        Place::Local(local) => local.0,
        Place::Temp(temp) => locals_len + temp.0,
    }
}

fn place_idx_opt(place: &Place, locals_len: usize) -> usize {
    place_idx(place, locals_len)
}

fn value_from_idx(idx: usize, locals_len: usize) -> Value {
    if idx < locals_len {
        Value::Local(crate::mir::ir::LocalId(idx))
    } else {
        Value::Temp(crate::mir::ir::TempId(idx - locals_len))
    }
}

fn idx_is_ref(types: &[MirType], idx: usize) -> bool {
    types.get(idx).map(|ty| ty.is_ref()).unwrap_or(true)
}

fn stmt_span(stmt: &Stmt) -> TextRange {
    match stmt {
        Stmt::Assign { span, .. }
        | Stmt::SetField { span, .. }
        | Stmt::RcInc { span, .. }
        | Stmt::RcDec { span, .. }
        | Stmt::Await { span, .. }
        | Stmt::Fire { span, .. }
        | Stmt::IterInit { span, .. }
        | Stmt::IterNext { span, .. } => *span,
    }
}

fn terminator_span(term: &Terminator) -> TextRange {
    match term {
        Terminator::Return { span, .. }
        | Terminator::Jump { span, .. }
        | Terminator::Branch { span, .. }
        | Terminator::Switch { span, .. }
        | Terminator::Unreachable { span, .. } => *span,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hir::lower as hir_lower;
    use crate::mir::ir::BlockId;
    use crate::mir::ir::{BasicBlock, Local, LocalId};
    use crate::mir::lower::lower_module;
    use crate::parser::ast;
    use crate::parser::ast::AstNode;
    use crate::parser::parse;

    #[test]
    fn test_constant_folding_binary() {
        let input = "to f() -> Nothing:\n    x = 1 + 2\n";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = hir_lower::lower(root);
        let mut mir = lower_module(&module);
        let func = mir.functions.iter_mut().find(|f| f.name == "f").unwrap();
        constant_fold(func);
        let assigns: Vec<_> = func.blocks[func.entry.0]
            .stmts
            .iter()
            .filter_map(|stmt| match stmt {
                Stmt::Assign { value, .. } => Some(value),
                _ => None,
            })
            .collect();
        assert!(
            assigns
                .iter()
                .any(|value| matches!(value, Rvalue::Use(Value::Const(Literal::Int(3)))))
        );
    }

    #[test]
    fn test_dead_code_elim_unused_temp() {
        let input = "to f() -> Nothing:\n    1 + 2\n";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = hir_lower::lower(root);
        let mut mir = lower_module(&module);
        let func = mir.functions.iter_mut().find(|f| f.name == "f").unwrap();
        dead_code_elim(func);
        let mut has_binary = false;
        for block in &func.blocks {
            for stmt in &block.stmts {
                if let Stmt::Assign { value, .. } = stmt {
                    if matches!(value, Rvalue::Binary { .. }) {
                        has_binary = true;
                    }
                }
            }
        }
        assert!(!has_binary);
    }

    #[test]
    fn test_rc_inserts_self_assign_temp() {
        let span = TextRange::new(0.into(), 0.into());
        let mut func = MirFunction {
            name: "test".into(),
            params: vec![],
            locals: vec![Local {
                name: "x".into(),
                mutable: true,
                ty: MirType::Unknown,
            }],
            temps: vec![],
            blocks: vec![BasicBlock {
                stmts: vec![Stmt::Assign {
                    place: Place::Local(LocalId(0)),
                    value: Rvalue::Binary {
                        op: BinaryOp::Add,
                        lhs: Value::Local(LocalId(0)),
                        rhs: Value::Const(Literal::Int(1)),
                    },
                    span,
                }],
                terminator: Terminator::Return { value: None, span },
            }],
            entry: BlockId(0),
            suspendable: false,
        };
        insert_rc(&mut func);
        let mut saw_temp_assign = false;
        let mut saw_dec_old = false;
        for stmt in &func.blocks[0].stmts {
            match stmt {
                Stmt::Assign {
                    place: Place::Temp(_),
                    ..
                } => saw_temp_assign = true,
                Stmt::RcDec {
                    value: Value::Local(LocalId(0)),
                    ..
                } => saw_dec_old = true,
                _ => {}
            }
        }
        assert!(saw_temp_assign, "expected temp assignment for self-assign");
        assert!(saw_dec_old, "expected dec of old value after temp");
    }
}
