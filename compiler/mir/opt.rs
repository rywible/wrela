use crate::hir::checkir::CheckIrModule;
use crate::hir::{BinaryOp, Literal, UnaryOp};
use crate::mir::analysis::{CallGraph, FunctionTypes, analyze_module};
use crate::mir::effect_ir;
use crate::mir::ir::{
    AllocKind, BasicBlock, CallKind, CallTarget, Local, LocalId, MirFunction, MirModule, MirType,
    Place, Rvalue, Stmt, SwitchCase, Temp, TempId, Terminator, TypeTagId, Value,
};
use crate::mir::rewrite::{RewriteBudget, RewriteReport, mine_admit_and_apply};
use rowan::TextRange;
use smol_str::SmolStr;
use std::collections::{HashMap, HashSet, VecDeque};

pub fn run_function_passes(func: &mut MirFunction) {
    run_function_passes_with(func, None);
}

pub fn run_function_passes_with(func: &mut MirFunction, types: Option<&FunctionTypes>) {
    devirtualize_calls(func, types);
    specialize_container_ops(func, types);
    flatten_string_concat_chains(func);
    if std::env::var("WRELA_DISABLE_ESCAPE_ANALYSIS").is_err() {
        annotate_allocs(func);
    }
    constant_fold(func);
    simplify_branches(func);
    dead_code_elim(func);
    convert_to_ssa(func);
    if std::env::var("WRELA_DISABLE_RESULT_ANNIHILATION").is_err()
        && std::env::var("WRELA_DISABLE_RESULT_PEEPHOLE").is_err()
    {
        hoist_loop_invariant_result_is_ok(func);
        result_peephole(func);
    }
    scalar_replace_literals(func);
    strength_reduce_mods(func);
    insert_rc(func);
    // RC insertion can introduce new dead stores (e.g., removed by earlier peepholes but kept
    // alive via pre-RC liveness assumptions). Clean it up so we don't keep doing useless work.
    dead_code_elim(func);
}

fn flatten_string_concat_chains(func: &mut MirFunction) {
    let temps = func.temps.iter().map(|t| t.ty.clone()).collect::<Vec<_>>();
    let locals = func.locals.iter().map(|l| l.ty.clone()).collect::<Vec<_>>();
    if temps.is_empty() {
        return;
    }

    let mut use_counts = vec![0u32; temps.len()];
    let mut defs: Vec<Option<Rvalue>> = vec![None; temps.len()];
    let mut local_use_counts = vec![0u32; locals.len()];

    for block in &func.blocks {
        for stmt in &block.stmts {
            match stmt {
                Stmt::Assign { place, value, .. } => {
                    if let Place::Temp(t) = place
                        && t.0 < defs.len()
                    {
                        defs[t.0] = Some(value.clone());
                    }
                    bump_uses_rvalue(value, &mut use_counts);
                }
                Stmt::SetField { base, value, .. } => {
                    bump_use(base, &mut use_counts);
                    bump_use(value, &mut use_counts);
                }
                Stmt::RcInc { value, .. } | Stmt::RcDec { value, .. } => {
                    bump_use(value, &mut use_counts);
                }
                Stmt::Await { pending, .. } => bump_use(pending, &mut use_counts),
                Stmt::Fire { pending, .. } => bump_use(pending, &mut use_counts),
                Stmt::IterInit { iterable, .. } => bump_use(iterable, &mut use_counts),
                Stmt::IterNext { iter, .. } => bump_use(iter, &mut use_counts),
                Stmt::Phi { .. } => {}
            }
        }
        bump_uses_terminator(&block.terminator, &mut use_counts);
    }

    // Also count reads of locals so we can safely inline "local = temp" aliases into concats.
    for block in &func.blocks {
        for stmt in &block.stmts {
            bump_local_reads_in_stmt(stmt, &mut local_use_counts);
        }
        bump_local_reads_in_terminator(&block.terminator, &mut local_use_counts);
    }

    for block in &mut func.blocks {
        let mut local_temp_alias: HashMap<LocalId, TempId> = HashMap::new();
        for stmt in &mut block.stmts {
            let Stmt::Assign { place, value, .. } = stmt else {
                continue;
            };
            // Track `local = temp` aliases within the block so we can flatten
            // `s0 = "...{i}"; s1 = s0 + "..."; s2 = s1 + "..."` into a single concat.
            if let Place::Local(local_id) = &*place {
                if let Rvalue::Use(Value::Temp(temp_id)) = &*value {
                    local_temp_alias.insert(*local_id, *temp_id);
                } else {
                    local_temp_alias.remove(local_id);
                }
            }

            let Place::Temp(dst) = place.clone() else {
                continue;
            };
            if temps.get(dst.0) != Some(&MirType::String) {
                continue;
            }
            let Rvalue::Binary {
                op: BinaryOp::Add,
                lhs,
                rhs,
            } = &*value
            else {
                continue;
            };
            if value_ty_with_slices(lhs, &locals, &temps) != MirType::String
                || value_ty_with_slices(rhs, &locals, &temps) != MirType::String
            {
                continue;
            }
            let mut parts = Vec::new();
            collect_string_concat_parts(lhs, &locals, &temps, &defs, &use_counts, &mut parts);
            parts.push(rhs.clone());
            if parts.len() < 3 {
                continue;
            }
            *value = Rvalue::StrConcat {
                parts,
                alloc: AllocKind::Escaping,
            };
        }
    }

    // Second pass: flatten nested StrConcat/StringInterp temps (and local aliases to temps)
    // into a single StrConcat when it creates a meaningful arity win.
    for block in &mut func.blocks {
        let mut local_temp_alias: HashMap<LocalId, TempId> = HashMap::new();
        for stmt in &mut block.stmts {
            let Stmt::Assign { place, value, .. } = stmt else {
                continue;
            };
            if let Place::Local(local_id) = &*place {
                if let Rvalue::Use(Value::Temp(temp_id)) = &*value {
                    local_temp_alias.insert(*local_id, *temp_id);
                } else {
                    local_temp_alias.remove(local_id);
                }
                continue;
            }

            let Place::Temp(dst) = place.clone() else {
                continue;
            };
            if temps.get(dst.0) != Some(&MirType::String) {
                continue;
            }

            let (orig_parts, alloc_kind) = match value {
                Rvalue::StrConcat { parts, alloc } => (parts.clone(), *alloc),
                Rvalue::StringInterp { parts, alloc } => {
                    let mut out = Vec::new();
                    for part in parts {
                        match part {
                            crate::mir::ir::StringPartValue::Literal(s) => {
                                if !s.is_empty() {
                                    out.push(Value::Const(Literal::String(s.clone())));
                                }
                            }
                            crate::mir::ir::StringPartValue::Value(v) => out.push(v.clone()),
                        }
                    }
                    (out, *alloc)
                }
                _ => continue,
            };

            let mut flattened: Vec<Value> = Vec::new();
            for part in &orig_parts {
                collect_stringish_concat_parts(
                    part,
                    &locals,
                    &temps,
                    &defs,
                    &use_counts,
                    &local_use_counts,
                    &local_temp_alias,
                    &mut flattened,
                );
            }
            // Drop empty string literals; they are common from interpolation and create noise.
            flattened.retain(|v| !is_empty_string_const(v));
            if flattened.len() < 3 {
                continue;
            }
            *value = Rvalue::StrConcat {
                parts: flattened,
                alloc: alloc_kind,
            };
        }
    }
}

fn collect_string_concat_parts(
    value: &Value,
    locals: &[MirType],
    temps: &[MirType],
    defs: &[Option<Rvalue>],
    use_counts: &[u32],
    out: &mut Vec<Value>,
) {
    if let Value::Temp(t) = value
        && use_counts.get(t.0).copied().unwrap_or(0) == 1
        && let Some(Rvalue::Binary {
            op: BinaryOp::Add,
            lhs,
            rhs,
        }) = defs.get(t.0).and_then(|v| v.clone())
        && value_ty_with_slices(&lhs, locals, temps) == MirType::String
        && value_ty_with_slices(&rhs, locals, temps) == MirType::String
    {
        collect_string_concat_parts(&lhs, locals, temps, defs, use_counts, out);
        out.push(rhs);
        return;
    }
    out.push(value.clone());
}

fn collect_stringish_concat_parts(
    value: &Value,
    locals: &[MirType],
    temps: &[MirType],
    defs: &[Option<Rvalue>],
    use_counts: &[u32],
    local_use_counts: &[u32],
    local_temp_alias: &HashMap<LocalId, TempId>,
    out: &mut Vec<Value>,
) {
    match value {
        Value::Local(local_id) => {
            if let Some(temp_id) = local_temp_alias.get(local_id) {
                // Only inline through the alias if the temp is a pure concat building block and
                // the local is only read once in the IR (so we don't duplicate work).
                if local_use_counts.get(local_id.0).copied().unwrap_or(0) <= 1 {
                    collect_stringish_concat_parts(
                        &Value::Temp(*temp_id),
                        locals,
                        temps,
                        defs,
                        use_counts,
                        local_use_counts,
                        local_temp_alias,
                        out,
                    );
                    return;
                }
            }
        }
        Value::Temp(temp_id) => {
            if use_counts.get(temp_id.0).copied().unwrap_or(0) == 1
                && let Some(def) = defs.get(temp_id.0).and_then(|v| v.clone())
            {
                match def {
                    Rvalue::Binary {
                        op: BinaryOp::Add,
                        lhs,
                        rhs,
                    } => {
                        if value_ty_with_slices(&lhs, locals, temps) == MirType::String
                            && value_ty_with_slices(&rhs, locals, temps) == MirType::String
                        {
                            collect_stringish_concat_parts(
                                &lhs,
                                locals,
                                temps,
                                defs,
                                use_counts,
                                local_use_counts,
                                local_temp_alias,
                                out,
                            );
                            collect_stringish_concat_parts(
                                &rhs,
                                locals,
                                temps,
                                defs,
                                use_counts,
                                local_use_counts,
                                local_temp_alias,
                                out,
                            );
                            return;
                        }
                    }
                    Rvalue::StrConcat { parts, .. } => {
                        for part in &parts {
                            collect_stringish_concat_parts(
                                part,
                                locals,
                                temps,
                                defs,
                                use_counts,
                                local_use_counts,
                                local_temp_alias,
                                out,
                            );
                        }
                        return;
                    }
                    Rvalue::StringInterp { parts, .. } => {
                        for part in &parts {
                            match part {
                                crate::mir::ir::StringPartValue::Literal(s) => {
                                    if !s.is_empty() {
                                        out.push(Value::Const(Literal::String(s.clone())));
                                    }
                                }
                                crate::mir::ir::StringPartValue::Value(v) => {
                                    collect_stringish_concat_parts(
                                        v,
                                        locals,
                                        temps,
                                        defs,
                                        use_counts,
                                        local_use_counts,
                                        local_temp_alias,
                                        out,
                                    );
                                }
                            }
                        }
                        return;
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }
    out.push(value.clone());
}

fn is_empty_string_const(value: &Value) -> bool {
    matches!(value, Value::Const(Literal::String(s)) if s.is_empty())
}

fn bump_local_reads_in_value(value: &Value, local_use_counts: &mut [u32]) {
    if let Value::Local(local_id) = value
        && let Some(slot) = local_use_counts.get_mut(local_id.0)
    {
        *slot = slot.saturating_add(1);
    }
}

fn bump_local_reads_in_rvalue(value: &Rvalue, local_use_counts: &mut [u32]) {
    match value {
        Rvalue::Use(v) => bump_local_reads_in_value(v, local_use_counts),
        Rvalue::Unary { operand, .. } => bump_local_reads_in_value(operand, local_use_counts),
        Rvalue::Binary { lhs, rhs, .. } => {
            bump_local_reads_in_value(lhs, local_use_counts);
            bump_local_reads_in_value(rhs, local_use_counts);
        }
        Rvalue::StrConcat { parts, .. } => {
            for p in parts {
                bump_local_reads_in_value(p, local_use_counts);
            }
        }
        Rvalue::StringInterp { parts, .. } => {
            for p in parts {
                if let crate::mir::ir::StringPartValue::Value(v) = p {
                    bump_local_reads_in_value(v, local_use_counts);
                }
            }
        }
        Rvalue::ResultOk { value }
        | Rvalue::ResultErr { value }
        | Rvalue::ResultIsOk { value }
        | Rvalue::ResultUnwrap { value }
        | Rvalue::ResultErrUnwrap { value }
        | Rvalue::Crash { value } => bump_local_reads_in_value(value, local_use_counts),
        Rvalue::GetField { base, .. } => bump_local_reads_in_value(base, local_use_counts),
        Rvalue::Call { args, target, .. } => {
            match target {
                CallTarget::Method { receiver, .. } => {
                    bump_local_reads_in_value(receiver, local_use_counts)
                }
                CallTarget::Indirect(v) => bump_local_reads_in_value(v, local_use_counts),
                _ => {}
            }
            for a in args {
                bump_local_reads_in_value(a, local_use_counts);
            }
        }
        Rvalue::ClassInit { .. } => {}
        Rvalue::Spawn {
            target, instance, ..
        } => {
            bump_local_reads_in_value(target, local_use_counts);
            bump_local_reads_in_value(instance, local_use_counts);
        }
        Rvalue::PoolNew { handles, .. } => {
            bump_local_reads_in_value(handles, local_use_counts);
        }
        Rvalue::BuildList { items, .. } => {
            for it in items {
                bump_local_reads_in_value(it, local_use_counts);
            }
        }
        Rvalue::BuildMap { items, .. } => {
            for (k, v) in items {
                bump_local_reads_in_value(k, local_use_counts);
                bump_local_reads_in_value(v, local_use_counts);
            }
        }
    }
}

fn bump_local_reads_in_stmt(stmt: &Stmt, local_use_counts: &mut [u32]) {
    match stmt {
        Stmt::Assign { value, .. } => bump_local_reads_in_rvalue(value, local_use_counts),
        Stmt::SetField { base, value, .. } => {
            bump_local_reads_in_value(base, local_use_counts);
            bump_local_reads_in_value(value, local_use_counts);
        }
        Stmt::RcInc { value, .. } | Stmt::RcDec { value, .. } => {
            bump_local_reads_in_value(value, local_use_counts);
        }
        Stmt::Await { pending, .. } => bump_local_reads_in_value(pending, local_use_counts),
        Stmt::Fire { pending, .. } => bump_local_reads_in_value(pending, local_use_counts),
        Stmt::IterInit { iterable, .. } => bump_local_reads_in_value(iterable, local_use_counts),
        Stmt::IterNext { iter, .. } => bump_local_reads_in_value(iter, local_use_counts),
        Stmt::Phi { sources, .. } => {
            for (_bid, v) in sources {
                bump_local_reads_in_value(v, local_use_counts);
            }
        }
    }
}

fn bump_local_reads_in_terminator(term: &Terminator, local_use_counts: &mut [u32]) {
    match term {
        Terminator::Return { value: Some(v), .. } => bump_local_reads_in_value(v, local_use_counts),
        Terminator::Return { value: None, .. } => {}
        Terminator::Jump { .. } => {}
        Terminator::Branch { cond, .. } => bump_local_reads_in_value(cond, local_use_counts),
        Terminator::Switch { scrutinee, .. } => {
            bump_local_reads_in_value(scrutinee, local_use_counts)
        }
        Terminator::Unreachable { .. } => {}
    }
}

fn bump_use(value: &Value, use_counts: &mut [u32]) {
    if let Value::Temp(t) = value
        && let Some(slot) = use_counts.get_mut(t.0)
    {
        *slot = slot.saturating_add(1);
    }
}

fn bump_uses_terminator(term: &Terminator, use_counts: &mut [u32]) {
    match term {
        Terminator::Return { value, .. } => {
            if let Some(v) = value {
                bump_use(v, use_counts);
            }
        }
        Terminator::Jump { .. } => {}
        Terminator::Branch { cond, .. } => bump_use(cond, use_counts),
        Terminator::Switch { scrutinee, .. } => bump_use(scrutinee, use_counts),
        Terminator::Unreachable { .. } => {}
    }
}

fn bump_uses_rvalue(value: &Rvalue, use_counts: &mut [u32]) {
    match value {
        Rvalue::Use(v) => bump_use(v, use_counts),
        Rvalue::Unary { operand, .. } => bump_use(operand, use_counts),
        Rvalue::Binary { lhs, rhs, .. } => {
            bump_use(lhs, use_counts);
            bump_use(rhs, use_counts);
        }
        Rvalue::StrConcat { parts, .. } => {
            for v in parts {
                bump_use(v, use_counts);
            }
        }
        Rvalue::ResultOk { value }
        | Rvalue::ResultErr { value }
        | Rvalue::ResultIsOk { value }
        | Rvalue::ResultUnwrap { value }
        | Rvalue::ResultErrUnwrap { value }
        | Rvalue::Crash { value } => bump_use(value, use_counts),
        Rvalue::GetField { base, .. } => bump_use(base, use_counts),
        Rvalue::Call { target, args, .. } => {
            match target {
                CallTarget::Method { receiver, .. } => bump_use(receiver, use_counts),
                CallTarget::Indirect(v) => bump_use(v, use_counts),
                _ => {}
            }
            for arg in args {
                bump_use(arg, use_counts);
            }
        }
        Rvalue::ClassInit { .. } => {}
        Rvalue::Spawn {
            target, instance, ..
        } => {
            bump_use(target, use_counts);
            bump_use(instance, use_counts);
        }
        Rvalue::PoolNew { handles, .. } => bump_use(handles, use_counts),
        Rvalue::BuildList { items, .. } => {
            for item in items {
                bump_use(item, use_counts);
            }
        }
        Rvalue::BuildMap { items, .. } => {
            for (k, v) in items {
                bump_use(k, use_counts);
                bump_use(v, use_counts);
            }
        }
        Rvalue::StringInterp { parts, .. } => {
            for part in parts {
                if let crate::mir::ir::StringPartValue::Value(v) = part {
                    bump_use(v, use_counts);
                }
            }
        }
    }
}

pub fn run_function_passes_with_types(func: &mut MirFunction, types: Option<&FunctionTypes>) {
    run_function_passes_with(func, types);
}

pub fn inline_small_pure_functions(module: &mut MirModule, graph: &CallGraph) -> usize {
    const INLINE_STMT_LIMIT: usize = 8;
    const INLINE_PER_FUNCTION_LIMIT: usize = 32;

    #[derive(Clone)]
    struct InlineCandidate {
        func: MirFunction,
    }

    fn candidate_from(func: &MirFunction) -> Option<InlineCandidate> {
        if func.suspendable || func.name == "main" {
            return None;
        }
        if func.blocks.len() != 1 {
            return None;
        }
        let block = &func.blocks[func.entry.0];
        if !matches!(block.terminator, Terminator::Return { .. }) {
            return None;
        }
        let mut stmt_count = 0usize;
        for stmt in &block.stmts {
            let Stmt::Assign { value, .. } = stmt else {
                return None;
            };
            if !is_pure_rvalue(value) {
                return None;
            }
            stmt_count += 1;
        }
        if stmt_count > INLINE_STMT_LIMIT {
            return None;
        }
        Some(InlineCandidate { func: func.clone() })
    }

    fn remap_value(
        value: &Value,
        local_map: &HashMap<LocalId, LocalId>,
        temp_map: &HashMap<TempId, TempId>,
    ) -> Value {
        match value {
            Value::Const(lit) => Value::Const(lit.clone()),
            Value::Local(id) => Value::Local(*local_map.get(id).expect("missing local map")),
            Value::Temp(id) => Value::Temp(*temp_map.get(id).expect("missing temp map")),
        }
    }

    fn remap_place(
        place: &Place,
        local_map: &HashMap<LocalId, LocalId>,
        temp_map: &HashMap<TempId, TempId>,
    ) -> Place {
        match place {
            Place::Local(id) => Place::Local(*local_map.get(id).expect("missing local map")),
            Place::Temp(id) => Place::Temp(*temp_map.get(id).expect("missing temp map")),
        }
    }

    fn remap_call_target(
        target: &CallTarget,
        local_map: &HashMap<LocalId, LocalId>,
        temp_map: &HashMap<TempId, TempId>,
    ) -> CallTarget {
        match target {
            CallTarget::Function(name) => CallTarget::Function(name.clone()),
            CallTarget::Method {
                receiver,
                method,
                method_id,
            } => CallTarget::Method {
                receiver: remap_value(receiver, local_map, temp_map),
                method: method.clone(),
                method_id: *method_id,
            },
            CallTarget::GuardedInterface {
                fast_paths,
                fallback,
            } => CallTarget::GuardedInterface {
                fast_paths: fast_paths.clone(),
                fallback: fallback.clone(),
            },
            CallTarget::Indirect(value) => {
                CallTarget::Indirect(remap_value(value, local_map, temp_map))
            }
        }
    }

    fn remap_rvalue(
        value: &Rvalue,
        local_map: &HashMap<LocalId, LocalId>,
        temp_map: &HashMap<TempId, TempId>,
    ) -> Rvalue {
        match value {
            Rvalue::Use(value) => Rvalue::Use(remap_value(value, local_map, temp_map)),
            Rvalue::Unary { op, operand } => Rvalue::Unary {
                op: *op,
                operand: remap_value(operand, local_map, temp_map),
            },
            Rvalue::Binary { op, lhs, rhs } => Rvalue::Binary {
                op: *op,
                lhs: remap_value(lhs, local_map, temp_map),
                rhs: remap_value(rhs, local_map, temp_map),
            },
            Rvalue::StrConcat { parts, alloc } => Rvalue::StrConcat {
                parts: parts
                    .iter()
                    .map(|part| remap_value(part, local_map, temp_map))
                    .collect(),
                alloc: *alloc,
            },
            Rvalue::ResultOk { value } => Rvalue::ResultOk {
                value: remap_value(value, local_map, temp_map),
            },
            Rvalue::ResultErr { value } => Rvalue::ResultErr {
                value: remap_value(value, local_map, temp_map),
            },
            Rvalue::ResultIsOk { value } => Rvalue::ResultIsOk {
                value: remap_value(value, local_map, temp_map),
            },
            Rvalue::ResultUnwrap { value } => Rvalue::ResultUnwrap {
                value: remap_value(value, local_map, temp_map),
            },
            Rvalue::ResultErrUnwrap { value } => Rvalue::ResultErrUnwrap {
                value: remap_value(value, local_map, temp_map),
            },
            Rvalue::Crash { value } => Rvalue::Crash {
                value: remap_value(value, local_map, temp_map),
            },
            Rvalue::GetField { base, field, slot } => Rvalue::GetField {
                base: remap_value(base, local_map, temp_map),
                field: field.clone(),
                slot: *slot,
            },
            Rvalue::Call { kind, target, args } => Rvalue::Call {
                kind: *kind,
                target: remap_call_target(target, local_map, temp_map),
                args: args
                    .iter()
                    .map(|arg| remap_value(arg, local_map, temp_map))
                    .collect(),
            },
            Rvalue::ClassInit { class_id, fields } => Rvalue::ClassInit {
                class_id: *class_id,
                fields: fields.clone(),
            },
            Rvalue::Spawn {
                target,
                instance,
                size,
                objective,
                config,
            } => Rvalue::Spawn {
                target: remap_value(target, local_map, temp_map),
                instance: remap_value(instance, local_map, temp_map),
                size: *size,
                objective: *objective,
                config: *config,
            },
            Rvalue::PoolNew {
                handles,
                objective,
                min_size,
                max_size,
                weight,
                queue_cap,
            } => Rvalue::PoolNew {
                handles: remap_value(handles, local_map, temp_map),
                objective: *objective,
                min_size: *min_size,
                max_size: *max_size,
                weight: *weight,
                queue_cap: *queue_cap,
            },
            Rvalue::BuildList { items, alloc } => Rvalue::BuildList {
                items: items
                    .iter()
                    .map(|item| remap_value(item, local_map, temp_map))
                    .collect(),
                alloc: *alloc,
            },
            Rvalue::BuildMap { items, alloc } => Rvalue::BuildMap {
                items: items
                    .iter()
                    .map(|(k, v)| {
                        (
                            remap_value(k, local_map, temp_map),
                            remap_value(v, local_map, temp_map),
                        )
                    })
                    .collect(),
                alloc: *alloc,
            },
            Rvalue::StringInterp { parts, alloc } => Rvalue::StringInterp {
                parts: parts
                    .iter()
                    .map(|part| match part {
                        crate::mir::ir::StringPartValue::Literal(lit) => {
                            crate::mir::ir::StringPartValue::Literal(lit.clone())
                        }
                        crate::mir::ir::StringPartValue::Value(value) => {
                            crate::mir::ir::StringPartValue::Value(remap_value(
                                value, local_map, temp_map,
                            ))
                        }
                    })
                    .collect(),
                alloc: *alloc,
            },
        }
    }

    let mut candidates: HashMap<SmolStr, InlineCandidate> = HashMap::new();
    for func in &module.functions {
        if let Some(candidate) = candidate_from(func) {
            candidates.insert(func.name.clone(), candidate);
        }
    }

    let mut total_inlined = 0usize;
    let mut inline_counter = 0usize;
    for func in &mut module.functions {
        let mut inline_budget = INLINE_PER_FUNCTION_LIMIT;
        for block in &mut func.blocks {
            if inline_budget == 0 {
                continue;
            }
            let old_stmts = std::mem::take(&mut block.stmts);
            let mut new_stmts = Vec::with_capacity(old_stmts.len());
            for stmt in old_stmts {
                let Stmt::Assign { place, value, span } = &stmt else {
                    new_stmts.push(stmt);
                    continue;
                };
                let Rvalue::Call { kind, target, args } = value else {
                    new_stmts.push(stmt);
                    continue;
                };
                let CallTarget::Function(name) = target else {
                    new_stmts.push(stmt);
                    continue;
                };
                if *kind != CallKind::Sync {
                    new_stmts.push(stmt);
                    continue;
                }
                if graph.call_count(name) == 0 {
                    new_stmts.push(stmt);
                    continue;
                }
                let Some(candidate) = candidates.get(name) else {
                    new_stmts.push(stmt);
                    continue;
                };
                if inline_budget == 0 {
                    new_stmts.push(stmt);
                    continue;
                }
                if func.name == candidate.func.name {
                    new_stmts.push(stmt);
                    continue;
                }
                if candidate.func.params.len() != args.len() {
                    new_stmts.push(stmt);
                    continue;
                }

                inline_counter += 1;
                inline_budget = inline_budget.saturating_sub(1);
                total_inlined += 1;

                let mut local_map = HashMap::new();
                for (idx, local) in candidate.func.locals.iter().enumerate() {
                    let name = SmolStr::new(format!(
                        "{}__inl{}_{}",
                        candidate.func.name, inline_counter, local.name
                    ));
                    let new_local = Local {
                        name,
                        mutable: local.mutable,
                        ty: local.ty.clone(),
                    };
                    let new_id = LocalId(func.locals.len());
                    func.locals.push(new_local);
                    local_map.insert(LocalId(idx), new_id);
                }
                let mut temp_map = HashMap::new();
                for (idx, temp) in candidate.func.temps.iter().enumerate() {
                    let new_id = TempId(func.temps.len());
                    func.temps.push(Temp {
                        ty: temp.ty.clone(),
                    });
                    temp_map.insert(TempId(idx), new_id);
                }

                for (param, arg) in candidate.func.params.iter().zip(args.iter()) {
                    let mapped_param = *local_map.get(param).expect("missing param local mapping");
                    new_stmts.push(Stmt::Assign {
                        place: Place::Local(mapped_param),
                        value: Rvalue::Use(arg.clone()),
                        span: *span,
                    });
                }

                let callee_block = &candidate.func.blocks[candidate.func.entry.0];
                for callee_stmt in &callee_block.stmts {
                    let Stmt::Assign { place, value, span } = callee_stmt else {
                        continue;
                    };
                    new_stmts.push(Stmt::Assign {
                        place: remap_place(place, &local_map, &temp_map),
                        value: remap_rvalue(value, &local_map, &temp_map),
                        span: *span,
                    });
                }

                let ret_value = match &callee_block.terminator {
                    Terminator::Return { value, .. } => value
                        .as_ref()
                        .map(|value| remap_value(value, &local_map, &temp_map))
                        .unwrap_or(Value::Const(Literal::Nil)),
                    _ => Value::Const(Literal::Nil),
                };
                new_stmts.push(Stmt::Assign {
                    place: place.clone(),
                    value: Rvalue::Use(ret_value),
                    span: *span,
                });
            }
            block.stmts = new_stmts;
        }
    }

    total_inlined
}

pub fn run_module_passes(module: &mut MirModule) {
    let _ = run_module_passes_with_rulepack(module, None);
}

pub fn run_module_passes_with_rulepack(
    module: &mut MirModule,
    checkir: Option<&CheckIrModule>,
) -> RewriteReport {
    if std::env::var("WRELA_DISABLE_INTERFACE_DEVIRTUALIZE").is_err() {
        devirtualize_interface_dispatch_calls(module);
    }
    let analysis = analyze_module(module);
    clone_small_hot_functions(module, &analysis.call_graph);
    let analysis = analyze_module(module);
    tree_shake_unused_functions(module, &analysis.call_graph);
    let batch_rewrite = rewrite_check_callsite_clusters(module, checkir);
    if std::env::var("WRELA_CHECK_ORACLE_TRACE").is_ok() {
        eprintln!(
            "check-oracle-batch: clusters={} rewritten={} scalar_fallback_clusters={}",
            batch_rewrite.clusters_seen,
            batch_rewrite.clusters_rewritten,
            batch_rewrite.scalar_fallback_clusters
        );
    }

    let max_rules = std::env::var("WRELA_REWRITE_MAX_RULES")
        .ok()
        .and_then(|raw| raw.parse::<usize>().ok())
        .unwrap_or(8);
    let budget = RewriteBudget {
        max_steps: std::env::var("WRELA_REWRITE_BUDGET")
            .ok()
            .and_then(|raw| raw.parse::<usize>().ok())
            .unwrap_or(50_000),
        max_compile_cost: std::env::var("WRELA_REWRITE_ADMISSION_BUDGET")
            .ok()
            .and_then(|raw| raw.parse::<u32>().ok())
            .unwrap_or(16),
        max_rule_risk: std::env::var("WRELA_REWRITE_MAX_RISK")
            .ok()
            .and_then(|raw| raw.parse::<u32>().ok())
            .unwrap_or(8),
        per_function_rewrite_cap: std::env::var("WRELA_REWRITE_PER_FUNCTION_CAP")
            .ok()
            .and_then(|raw| raw.parse::<usize>().ok())
            .unwrap_or(128),
    };
    mine_admit_and_apply(module, checkir, budget, max_rules)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct BatchCallsiteRewriteReport {
    clusters_seen: usize,
    clusters_rewritten: usize,
    scalar_fallback_clusters: usize,
}

fn rewrite_check_callsite_clusters(
    module: &mut MirModule,
    checkir: Option<&CheckIrModule>,
) -> BatchCallsiteRewriteReport {
    let Some(checkir) = checkir else {
        return BatchCallsiteRewriteReport::default();
    };
    let check_map: HashMap<SmolStr, bool> = checkir
        .checks
        .iter()
        .map(|check| (check.name.clone(), check.supports_vector_lane))
        .collect();
    if check_map.is_empty() {
        return BatchCallsiteRewriteReport::default();
    }

    let mut report = BatchCallsiteRewriteReport::default();
    let mut available = HashSet::new();
    for func in &module.functions {
        available.insert(func.name.clone());
    }

    for func in &mut module.functions {
        for block in &mut func.blocks {
            let mut idx = 0usize;
            while idx < block.stmts.len() {
                let Some((target, end)) = detect_sync_call_cluster(&block.stmts, idx) else {
                    idx += 1;
                    continue;
                };
                let cluster_len = end - idx;
                if cluster_len < 2 {
                    idx += 1;
                    continue;
                }
                report.clusters_seen += 1;

                let Some(vector_compatible) = check_map.get(&target).copied() else {
                    report.scalar_fallback_clusters += 1;
                    idx = end;
                    continue;
                };
                if !vector_compatible {
                    report.scalar_fallback_clusters += 1;
                    idx = end;
                    continue;
                }

                let batch_name = SmolStr::new(format!("{target}__batch"));
                if !available.contains(&batch_name) {
                    report.scalar_fallback_clusters += 1;
                    idx = end;
                    continue;
                }

                for stmt in &mut block.stmts[idx..end] {
                    let Stmt::Assign { value, .. } = stmt else {
                        continue;
                    };
                    let Rvalue::Call { target, .. } = value else {
                        continue;
                    };
                    *target = CallTarget::Function(batch_name.clone());
                }
                report.clusters_rewritten += 1;
                idx = end;
            }
        }
    }

    report
}

fn detect_sync_call_cluster(stmts: &[Stmt], start: usize) -> Option<(SmolStr, usize)> {
    let Stmt::Assign { value, .. } = stmts.get(start)? else {
        return None;
    };
    let Rvalue::Call {
        kind: CallKind::Sync,
        target: CallTarget::Function(name),
        ..
    } = value
    else {
        return None;
    };

    let mut end = start + 1;
    while let Some(Stmt::Assign { value, .. }) = stmts.get(end) {
        let Rvalue::Call {
            kind: CallKind::Sync,
            target: CallTarget::Function(next),
            ..
        } = value
        else {
            break;
        };
        if next != name {
            break;
        }
        end += 1;
    }
    Some((name.clone(), end))
}

#[derive(Debug, Clone)]
struct InterfaceDispatchCase {
    tag: TypeTagId,
    target: SmolStr,
}

#[derive(Debug, Clone)]
struct InterfaceDispatchInfo {
    cases: Vec<InterfaceDispatchCase>,
}

fn devirtualize_interface_dispatch_calls(module: &mut MirModule) {
    const MAX_GUARDED_INTERFACE_FAST_PATHS: usize = 3;
    let dispatch = collect_interface_dispatch_functions(module);
    if dispatch.is_empty() {
        return;
    }

    for func in &mut module.functions {
        if dispatch.contains_key(&func.name) {
            continue;
        }
        for block in &mut func.blocks {
            for stmt in &mut block.stmts {
                let Stmt::Assign { value, .. } = stmt else {
                    continue;
                };
                let Rvalue::Call { kind, target, .. } = value else {
                    continue;
                };
                if *kind != CallKind::Sync {
                    continue;
                }
                let CallTarget::Function(name) = target else {
                    continue;
                };
                let Some(info) = dispatch.get(name) else {
                    continue;
                };
                if info.cases.len() == 1 {
                    *target = CallTarget::Function(info.cases[0].target.clone());
                    continue;
                }
                if info.cases.len() > MAX_GUARDED_INTERFACE_FAST_PATHS {
                    continue;
                }
                let fast_paths = info
                    .cases
                    .iter()
                    .take(MAX_GUARDED_INTERFACE_FAST_PATHS)
                    .map(|case| (case.tag, case.target.clone()))
                    .collect();
                *target = CallTarget::GuardedInterface {
                    fast_paths,
                    fallback: name.clone(),
                };
            }
        }
    }
}

fn collect_interface_dispatch_functions(
    module: &MirModule,
) -> HashMap<SmolStr, InterfaceDispatchInfo> {
    let mut out = HashMap::new();
    for func in &module.functions {
        if let Some(info) = parse_interface_dispatch_function(func) {
            out.insert(func.name.clone(), info);
        }
    }
    out
}

fn parse_interface_dispatch_function(func: &MirFunction) -> Option<InterfaceDispatchInfo> {
    if func.params.is_empty() {
        return None;
    }
    let entry = func.blocks.get(func.entry.0)?;
    let Terminator::Switch {
        scrutinee, cases, ..
    } = &entry.terminator
    else {
        return None;
    };
    if !matches!(scrutinee, Value::Local(local) if *local == func.params[0]) {
        return None;
    }
    if cases.is_empty() {
        return None;
    }

    let mut parsed_cases = Vec::with_capacity(cases.len());
    for (case, block_id) in cases {
        let SwitchCase::Type(tag) = case else {
            return None;
        };
        let block = func.blocks.get(block_id.0)?;
        let target = find_direct_dispatch_call(block, &func.params)?;
        parsed_cases.push(InterfaceDispatchCase { tag: *tag, target });
    }
    Some(InterfaceDispatchInfo {
        cases: parsed_cases,
    })
}

fn find_direct_dispatch_call(block: &BasicBlock, params: &[LocalId]) -> Option<SmolStr> {
    for stmt in &block.stmts {
        let Stmt::Assign { value, .. } = stmt else {
            continue;
        };
        let Rvalue::Call { kind, target, args } = value else {
            continue;
        };
        if *kind != CallKind::Sync || !args_match_params(args, params) {
            continue;
        }
        if let CallTarget::Function(name) = target {
            return Some(name.clone());
        }
    }
    None
}

fn args_match_params(args: &[Value], params: &[LocalId]) -> bool {
    if args.len() != params.len() {
        return false;
    }
    for (arg, param) in args.iter().zip(params) {
        if !matches!(arg, Value::Local(local) if local == param) {
            return false;
        }
    }
    true
}

fn devirtualize_calls(func: &mut MirFunction, types: Option<&FunctionTypes>) {
    let locals = types
        .map(|t| t.locals.clone())
        .unwrap_or_else(|| func.locals.iter().map(|l| l.ty.clone()).collect());
    let temps = types
        .map(|t| t.temps.clone())
        .unwrap_or_else(|| func.temps.iter().map(|t| t.ty.clone()).collect());
    for block in &mut func.blocks {
        for stmt in &mut block.stmts {
            let Stmt::Assign { value, .. } = stmt else {
                continue;
            };
            let Rvalue::Call { kind, target, args } = value else {
                continue;
            };
            if *kind != CallKind::Sync {
                continue;
            }
            let CallTarget::Method {
                receiver,
                method,
                method_id,
            } = target
            else {
                continue;
            };
            if method_id.is_none() {
                continue;
            }
            let recv_ty = value_ty_with_slices(receiver, &locals, &temps);
            let MirType::Named(class_name) = recv_ty else {
                continue;
            };
            let qualified = qualify_method_name(method, &class_name);
            let recv = receiver.clone();
            let mut new_args = Vec::with_capacity(args.len() + 1);
            new_args.push(recv);
            new_args.extend(args.iter().cloned());
            *args = new_args;
            *target = CallTarget::Function(qualified);
        }
    }
}

fn specialize_container_ops(func: &mut MirFunction, types: Option<&FunctionTypes>) {
    let locals = types
        .map(|t| t.locals.clone())
        .unwrap_or_else(|| func.locals.iter().map(|l| l.ty.clone()).collect());
    let temps = types
        .map(|t| t.temps.clone())
        .unwrap_or_else(|| func.temps.iter().map(|t| t.ty.clone()).collect());
    for block in &mut func.blocks {
        for stmt in &mut block.stmts {
            let Stmt::Assign { value, .. } = stmt else {
                continue;
            };
            let Rvalue::Call { kind, target, args } = value else {
                continue;
            };
            if *kind != CallKind::Sync {
                continue;
            }
            let CallTarget::Method {
                receiver, method, ..
            } = target
            else {
                continue;
            };
            let recv_ty = value_ty_with_slices(receiver, &locals, &temps);
            let MirType::Named(class_name) = recv_ty else {
                continue;
            };
            let op = unqual_method_name(method);
            let builtin = match (class_name.as_str(), op.as_str()) {
                ("Map", "get") => "__wr_map_get",
                ("Map", "set") => "__wr_map_set",
                ("List", "push") => "__wr_list_push",
                _ => continue,
            };
            let recv = receiver.clone();
            let mut new_args = Vec::with_capacity(args.len() + 1);
            new_args.push(recv);
            new_args.extend(args.iter().cloned());
            *args = new_args;
            *target = CallTarget::Function(SmolStr::new(builtin));
        }
    }
}

fn qualify_method_name(method: &SmolStr, class_name: &SmolStr) -> SmolStr {
    if method.as_str().contains('.') {
        return method.clone();
    }
    SmolStr::new(format!("{}.{}", class_name, method))
}

fn unqual_method_name(method: &SmolStr) -> SmolStr {
    method
        .as_str()
        .rsplit('.')
        .next()
        .map(SmolStr::new)
        .unwrap_or_else(|| method.clone())
}

fn value_ty_with_slices(value: &Value, locals: &[MirType], temps: &[MirType]) -> MirType {
    match value {
        Value::Const(lit) => match lit {
            Literal::Integer(_) => MirType::Integer,
            Literal::Float(_) => MirType::Float,
            Literal::Boolean(_) => MirType::Boolean,
            Literal::String(_) => MirType::String,
            Literal::Nil => MirType::Nil,
        },
        Value::Local(local) => locals.get(local.0).cloned().unwrap_or(MirType::Unknown),
        Value::Temp(temp) => temps.get(temp.0).cloned().unwrap_or(MirType::Unknown),
    }
}

fn clone_small_hot_functions(module: &mut MirModule, graph: &CallGraph) {
    let mut func_map: HashMap<SmolStr, MirFunction> = HashMap::new();
    for func in &module.functions {
        func_map.insert(func.name.clone(), func.clone());
    }
    let mut cloned = Vec::new();
    let mut clone_names: HashMap<(SmolStr, SmolStr), SmolStr> = HashMap::new();
    let mut counter = 0usize;
    for func in &mut module.functions {
        for block in &mut func.blocks {
            for stmt in &mut block.stmts {
                let Stmt::Assign { value, .. } = stmt else {
                    continue;
                };
                let Rvalue::Call { target, .. } = value else {
                    continue;
                };
                let CallTarget::Function(name) = target else {
                    continue;
                };
                if graph.call_count(name) < 2 {
                    continue;
                }
                let Some(callee) = func_map.get(name) else {
                    continue;
                };
                if !is_small_function(callee) || callee.suspendable || callee.name == "main" {
                    continue;
                }
                let key = (func.name.clone(), name.clone());
                let clone_name = clone_names.entry(key).or_insert_with(|| {
                    counter += 1;
                    SmolStr::new(format!("{}__clone{}", name, counter))
                });
                if !func_map.contains_key(clone_name) {
                    let mut clone = callee.clone();
                    clone.name = clone_name.clone();
                    cloned.push(clone);
                }
                *target = CallTarget::Function(clone_name.clone());
            }
        }
    }
    module.functions.extend(cloned);
}

fn is_small_function(func: &MirFunction) -> bool {
    if func.blocks.len() > 1 {
        return false;
    }
    let mut stmts = 0usize;
    for block in &func.blocks {
        stmts += block.stmts.len();
    }
    stmts <= 6
}

fn tree_shake_unused_functions(module: &mut MirModule, graph: &CallGraph) {
    let mut roots = HashSet::new();
    roots.insert(SmolStr::new("main"));
    for class in &module.classes {
        for method in &class.methods {
            roots.insert(method.func.clone());
        }
    }

    let mut reachable = HashSet::new();
    let mut queue = VecDeque::new();
    for root in roots {
        queue.push_back(root);
    }
    while let Some(name) = queue.pop_front() {
        if !reachable.insert(name.clone()) {
            continue;
        }
        for callee in graph.edges(&name) {
            queue.push_back(callee.clone());
        }
    }

    module
        .functions
        .retain(|func| reachable.contains(&func.name));
}

fn annotate_allocs(func: &mut MirFunction) {
    let mut deps: Vec<Vec<usize>> = vec![Vec::new(); func.temps.len()];
    let mut escapes = vec![false; func.temps.len()];

    for block in &func.blocks {
        for stmt in &block.stmts {
            if let Stmt::Assign { place, value, .. } = stmt
                && let Place::Temp(dst) = place
            {
                let mut used = Vec::new();
                collect_temp_ids_rvalue(value, &mut used);
                deps[dst.0].extend(used);
            }
            if let Stmt::SetField { value, .. } = stmt {
                collect_temp_ids_value(value, &mut escapes);
            }
        }
        if let Terminator::Return {
            value: Some(value), ..
        } = &block.terminator
        {
            collect_temp_ids_value(value, &mut escapes);
        }
    }

    for block in &func.blocks {
        for stmt in &block.stmts {
            if let Stmt::Assign { value, .. } = stmt {
                match value {
                    Rvalue::Call { kind, target, args } => {
                        if matches!(kind, CallKind::Actor) {
                            collect_temp_ids_call_target(target, &mut escapes);
                            for arg in args {
                                collect_temp_ids_value(arg, &mut escapes);
                            }
                        }
                    }
                    Rvalue::Spawn {
                        target, instance, ..
                    } => {
                        collect_temp_ids_value(target, &mut escapes);
                        collect_temp_ids_value(instance, &mut escapes);
                    }
                    Rvalue::PoolNew { handles, .. } => {
                        collect_temp_ids_value(handles, &mut escapes);
                    }
                    _ => {}
                }
            }
        }
    }

    let mut stack: Vec<usize> = escapes
        .iter()
        .enumerate()
        .filter_map(|(idx, flag)| if *flag { Some(idx) } else { None })
        .collect();
    while let Some(temp) = stack.pop() {
        let deps_list = deps[temp].clone();
        for dep in deps_list {
            if !escapes[dep] {
                escapes[dep] = true;
                stack.push(dep);
            }
        }
    }

    for block in &mut func.blocks {
        for stmt in &mut block.stmts {
            if let Stmt::Assign { place, value, .. } = stmt
                && let Place::Temp(dst) = place
            {
                let alloc = if escapes[dst.0] {
                    AllocKind::Escaping
                } else {
                    AllocKind::LocalTemp
                };
                match value {
                    Rvalue::BuildList { alloc: slot, .. }
                    | Rvalue::BuildMap { alloc: slot, .. }
                    | Rvalue::StringInterp { alloc: slot, .. }
                    | Rvalue::StrConcat { alloc: slot, .. } => {
                        *slot = alloc;
                    }
                    _ => {}
                }
            }
        }
    }
}

fn convert_to_ssa(func: &mut MirFunction) {
    if func.suspendable {
        return;
    }
    let locals_len = func.locals.len();
    if locals_len == 0 || func.blocks.is_empty() {
        return;
    }

    let succs = block_successors(func);
    let preds = block_predecessors(func, &succs);
    let reachable = compute_reachable(func.entry.0, &succs);
    let doms = compute_dominators(func.blocks.len(), func.entry.0, &preds);
    let idom = compute_idom(&doms, func.entry.0, &reachable);
    let dom_tree = compute_dom_tree(&idom);
    let dom_frontiers = compute_dom_frontiers(func.entry.0, &succs, &idom, &dom_tree);

    let mut def_blocks: Vec<Vec<usize>> = vec![Vec::new(); locals_len];
    for (block_idx, block) in func.blocks.iter().enumerate() {
        if !reachable[block_idx] {
            continue;
        }
        for stmt in &block.stmts {
            for local in stmt_local_defs(stmt) {
                if !def_blocks[local.0].contains(&block_idx) {
                    def_blocks[local.0].push(block_idx);
                }
            }
        }
    }

    let mut phi_locals_per_block: Vec<Vec<LocalId>> = vec![Vec::new(); func.blocks.len()];
    for (local_idx, defs) in def_blocks.iter().enumerate() {
        if defs.len() < 2 {
            continue;
        }
        let mut work: VecDeque<usize> = defs.iter().copied().collect();
        let mut has_phi = vec![false; func.blocks.len()];
        while let Some(block) = work.pop_front() {
            for frontier in &dom_frontiers[block] {
                if !reachable[*frontier] {
                    continue;
                }
                if *frontier == func.entry.0 {
                    continue;
                }
                if !has_phi[*frontier] {
                    has_phi[*frontier] = true;
                    let local = LocalId(local_idx);
                    phi_locals_per_block[*frontier].push(local);
                    if !defs.contains(frontier) {
                        work.push_back(*frontier);
                    }
                }
            }
        }
    }

    for (block_idx, block) in func.blocks.iter_mut().enumerate() {
        if phi_locals_per_block[block_idx].is_empty() {
            continue;
        }
        let mut new_stmts =
            Vec::with_capacity(block.stmts.len() + phi_locals_per_block[block_idx].len());
        for local in &phi_locals_per_block[block_idx] {
            new_stmts.push(Stmt::Phi {
                place: Place::Local(*local),
                sources: Vec::new(),
                original: *local,
                span: TextRange::empty(0.into()),
            });
        }
        new_stmts.append(&mut block.stmts);
        block.stmts = new_stmts;
    }

    let mut stacks: Vec<Vec<LocalId>> = Vec::with_capacity(locals_len);
    for idx in 0..locals_len {
        stacks.push(vec![LocalId(idx)]);
    }
    let mut version_counts = vec![0usize; locals_len];

    rename_block(
        &mut func.blocks,
        &mut func.locals,
        func.entry.0,
        &succs,
        &dom_tree,
        &reachable,
        &mut stacks,
        &mut version_counts,
    );
}

fn compute_reachable(entry: usize, succs: &[Vec<usize>]) -> Vec<bool> {
    let mut reachable = vec![false; succs.len()];
    let mut queue = VecDeque::new();
    queue.push_back(entry);
    while let Some(block) = queue.pop_front() {
        if reachable[block] {
            continue;
        }
        reachable[block] = true;
        for succ in &succs[block] {
            if *succ < succs.len() {
                queue.push_back(*succ);
            }
        }
    }
    reachable
}

fn compute_dominators(blocks_len: usize, entry: usize, preds: &[Vec<usize>]) -> Vec<Vec<bool>> {
    let mut doms = vec![vec![true; blocks_len]; blocks_len];
    for b in 0..blocks_len {
        if b == entry {
            let mut entry_dom = vec![false; blocks_len];
            entry_dom[entry] = true;
            doms[b] = entry_dom;
        }
    }

    let mut changed = true;
    while changed {
        changed = false;
        for b in 0..blocks_len {
            if b == entry {
                continue;
            }
            if preds[b].is_empty() {
                continue;
            }
            let mut new_dom = vec![true; blocks_len];
            for p in &preds[b] {
                for i in 0..blocks_len {
                    new_dom[i] &= doms[*p][i];
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

fn compute_idom(doms: &[Vec<bool>], entry: usize, reachable: &[bool]) -> Vec<Option<usize>> {
    let blocks_len = doms.len();
    let mut idom = vec![None; blocks_len];
    idom[entry] = Some(entry);
    for b in 0..blocks_len {
        if b == entry {
            continue;
        }
        if !reachable[b] {
            idom[b] = None;
            continue;
        }
        let mut candidates: Vec<usize> = (0..blocks_len).filter(|d| doms[b][*d]).collect();
        candidates.retain(|d| *d != b);
        let mut best = None;
        for d in candidates.iter().copied() {
            let mut is_idom = true;
            for other in candidates.iter().copied() {
                if other != d && doms[other][d] {
                    is_idom = false;
                    break;
                }
            }
            if is_idom {
                best = Some(d);
                break;
            }
        }
        idom[b] = best;
    }
    idom
}

fn compute_dom_tree(idom: &[Option<usize>]) -> Vec<Vec<usize>> {
    let mut tree = vec![Vec::new(); idom.len()];
    for (block, parent) in idom.iter().enumerate() {
        if let Some(p) = parent
            && *p != block
        {
            tree[*p].push(block);
        }
    }
    tree
}

fn compute_dom_frontiers(
    entry: usize,
    succs: &[Vec<usize>],
    idom: &[Option<usize>],
    dom_tree: &[Vec<usize>],
) -> Vec<Vec<usize>> {
    let mut df = vec![Vec::new(); succs.len()];
    for b in 0..succs.len() {
        for s in &succs[b] {
            if idom[*s].unwrap_or(*s) != b {
                df[b].push(*s);
            }
        }
    }
    let mut stack = vec![entry];
    while let Some(b) = stack.pop() {
        for child in &dom_tree[b] {
            stack.push(*child);
            for w in df[*child].clone() {
                if idom[w].unwrap_or(w) != b && !df[b].contains(&w) {
                    df[b].push(w);
                }
            }
        }
    }
    for b in 0..df.len() {
        df[b].sort_unstable();
        df[b].dedup();
    }
    df
}

fn stmt_local_defs(stmt: &Stmt) -> Vec<LocalId> {
    match stmt {
        Stmt::Phi { place, .. } | Stmt::Assign { place, .. } => match place {
            Place::Local(local) => vec![*local],
            Place::Temp(_) => Vec::new(),
        },
        Stmt::Await { dst, .. } | Stmt::IterInit { dst, .. } => match dst {
            Place::Local(local) => vec![*local],
            Place::Temp(_) => Vec::new(),
        },
        Stmt::IterNext {
            dst_value,
            dst_done,
            ..
        } => {
            let mut out = Vec::new();
            if let Place::Local(local) = dst_value {
                out.push(*local);
            }
            if let Place::Local(local) = dst_done {
                out.push(*local);
            }
            out
        }
        Stmt::SetField { .. } | Stmt::RcInc { .. } | Stmt::RcDec { .. } | Stmt::Fire { .. } => {
            Vec::new()
        }
    }
}

fn rename_block(
    blocks: &mut Vec<BasicBlock>,
    locals: &mut Vec<Local>,
    block_idx: usize,
    succs: &[Vec<usize>],
    dom_tree: &[Vec<usize>],
    reachable: &[bool],
    stacks: &mut Vec<Vec<LocalId>>,
    version_counts: &mut Vec<usize>,
) {
    if !reachable[block_idx] {
        return;
    }
    let mut pushed: Vec<LocalId> = Vec::new();
    {
        let block = &mut blocks[block_idx];
        for stmt in block.stmts.iter_mut() {
            if let Stmt::Phi {
                place, original, ..
            } = stmt
            {
                let new_local = new_local_version(locals, *original, version_counts);
                *place = Place::Local(new_local);
                stacks[original.0].push(new_local);
                pushed.push(*original);
            } else {
                break;
            }
        }

        for stmt in block.stmts.iter_mut() {
            match stmt {
                Stmt::Phi { .. } => {}
                Stmt::Assign { place, value, .. } => {
                    rename_rvalue(value, stacks);
                    if let Place::Local(local_id) = *place {
                        let new_local = new_local_version(locals, local_id, version_counts);
                        *place = Place::Local(new_local);
                        stacks[local_id.0].push(new_local);
                        pushed.push(local_id);
                    }
                }
                Stmt::SetField { base, value, .. } => {
                    rename_value(base, stacks);
                    rename_value(value, stacks);
                }
                Stmt::RcInc { value, .. } | Stmt::RcDec { value, .. } => {
                    rename_value(value, stacks);
                }
                Stmt::Await { dst, pending, .. } => {
                    rename_value(pending, stacks);
                    if let Place::Local(local_id) = *dst {
                        let new_local = new_local_version(locals, local_id, version_counts);
                        *dst = Place::Local(new_local);
                        stacks[local_id.0].push(new_local);
                        pushed.push(local_id);
                    }
                }
                Stmt::Fire { pending, .. } => rename_value(pending, stacks),
                Stmt::IterInit { dst, iterable, .. } => {
                    rename_value(iterable, stacks);
                    if let Place::Local(local_id) = *dst {
                        let new_local = new_local_version(locals, local_id, version_counts);
                        *dst = Place::Local(new_local);
                        stacks[local_id.0].push(new_local);
                        pushed.push(local_id);
                    }
                }
                Stmt::IterNext {
                    iter,
                    dst_value,
                    dst_done,
                    ..
                } => {
                    rename_value(iter, stacks);
                    if let Place::Local(local_id) = *dst_value {
                        let new_local = new_local_version(locals, local_id, version_counts);
                        *dst_value = Place::Local(new_local);
                        stacks[local_id.0].push(new_local);
                        pushed.push(local_id);
                    }
                    if let Place::Local(local_id) = *dst_done {
                        let new_local = new_local_version(locals, local_id, version_counts);
                        *dst_done = Place::Local(new_local);
                        stacks[local_id.0].push(new_local);
                        pushed.push(local_id);
                    }
                }
            }
        }

        rename_terminator(&mut block.terminator, stacks);
    }

    for succ in &succs[block_idx] {
        if !reachable[*succ] {
            continue;
        }
        let block = &mut blocks[*succ];
        for stmt in block.stmts.iter_mut() {
            if let Stmt::Phi {
                sources, original, ..
            } = stmt
            {
                if let Some(current) = stacks[original.0].last().copied() {
                    sources.push((crate::mir::ir::BlockId(block_idx), Value::Local(current)));
                }
            } else {
                break;
            }
        }
    }

    for child in &dom_tree[block_idx] {
        rename_block(
            blocks,
            locals,
            *child,
            succs,
            dom_tree,
            reachable,
            stacks,
            version_counts,
        );
    }

    for original in pushed.into_iter().rev() {
        stacks[original.0].pop();
    }
}

fn new_local_version(
    locals: &mut Vec<Local>,
    original: LocalId,
    version_counts: &mut Vec<usize>,
) -> LocalId {
    let orig = locals.get(original.0).cloned().unwrap_or(Local {
        name: SmolStr::new("tmp"),
        mutable: false,
        ty: MirType::Unknown,
    });
    let version = version_counts[original.0];
    version_counts[original.0] += 1;
    let name = SmolStr::new(format!("{}#ssa{}", orig.name, version));
    let id = LocalId(locals.len());
    locals.push(Local {
        name,
        mutable: false,
        ty: orig.ty,
    });
    id
}

fn rename_value(value: &mut Value, stacks: &[Vec<LocalId>]) {
    if let Value::Local(local) = value
        && let Some(current) = stacks.get(local.0).and_then(|stack| stack.last())
    {
        *value = Value::Local(*current);
    }
}

fn rename_rvalue(value: &mut Rvalue, stacks: &[Vec<LocalId>]) {
    match value {
        Rvalue::Use(value)
        | Rvalue::ResultOk { value }
        | Rvalue::ResultErr { value }
        | Rvalue::ResultIsOk { value }
        | Rvalue::ResultUnwrap { value }
        | Rvalue::ResultErrUnwrap { value }
        | Rvalue::Crash { value } => rename_value(value, stacks),
        Rvalue::Unary { operand, .. } => rename_value(operand, stacks),
        Rvalue::Binary { lhs, rhs, .. } => {
            rename_value(lhs, stacks);
            rename_value(rhs, stacks);
        }
        Rvalue::GetField { base, .. } => rename_value(base, stacks),
        Rvalue::Call { target, args, .. } => {
            rename_call_target(target, stacks);
            for arg in args {
                rename_value(arg, stacks);
            }
        }
        Rvalue::ClassInit { .. } => {}
        Rvalue::Spawn {
            target, instance, ..
        } => {
            rename_value(target, stacks);
            rename_value(instance, stacks);
        }
        Rvalue::PoolNew { handles, .. } => rename_value(handles, stacks),
        Rvalue::BuildList { items, .. } => {
            for item in items {
                rename_value(item, stacks);
            }
        }
        Rvalue::BuildMap { items, .. } => {
            for (key, value) in items {
                rename_value(key, stacks);
                rename_value(value, stacks);
            }
        }
        Rvalue::StringInterp { parts, .. } => {
            for part in parts {
                if let crate::mir::ir::StringPartValue::Value(value) = part {
                    rename_value(value, stacks);
                }
            }
        }
        Rvalue::StrConcat { parts, .. } => {
            for part in parts {
                rename_value(part, stacks);
            }
        }
    }
}

fn rename_call_target(target: &mut CallTarget, stacks: &[Vec<LocalId>]) {
    match target {
        CallTarget::Function(_) => {}
        CallTarget::Method { receiver, .. } => rename_value(receiver, stacks),
        CallTarget::GuardedInterface { .. } => {}
        CallTarget::Indirect(value) => rename_value(value, stacks),
    }
}

fn rename_terminator(term: &mut Terminator, stacks: &[Vec<LocalId>]) {
    match term {
        Terminator::Return { value, .. } => {
            if let Some(value) = value {
                rename_value(value, stacks);
            }
        }
        Terminator::Jump { .. } => {}
        Terminator::Branch { cond, .. } => rename_value(cond, stacks),
        Terminator::Switch { scrutinee, .. } => rename_value(scrutinee, stacks),
        Terminator::Unreachable { .. } => {}
    }
}

fn result_peephole(func: &mut MirFunction) {
    let report = effect_ir::annihilate_result_wrappers(func);
    if std::env::var("WRELA_CHECK_ORACLE_TRACE").is_ok() {
        eprintln!(
            "effect-annihilation: rewritten={} cross_block={} blocked={}",
            report.rewritten_statements,
            report.cross_block_rewrites,
            report
                .blocked_rewrite_reasons
                .values()
                .copied()
                .sum::<usize>()
        );
    }
}

fn hoist_loop_invariant_result_is_ok(func: &mut MirFunction) {
    if func.blocks.len() < 2 {
        return;
    }
    let succs = block_successors(func);
    let preds = block_predecessors(func, &succs);
    let reachable = compute_reachable(func.entry.0, &succs);
    let doms = compute_dominators(func.blocks.len(), func.entry.0, &preds);
    let def_block_by_temp = collect_temp_def_blocks(func);

    let mut planned: Vec<(usize, usize, usize)> = Vec::new();
    for pred in 0..succs.len() {
        if !reachable[pred] {
            continue;
        }
        for header in &succs[pred] {
            if *header >= doms.len() || !doms[pred][*header] {
                continue;
            }
            let Some(preheader) = unique_loop_preheader(*header, pred, &preds, &doms) else {
                continue;
            };
            for (stmt_idx, stmt) in func.blocks[*header].stmts.iter().enumerate() {
                let Stmt::Assign { value, .. } = stmt else {
                    continue;
                };
                let Rvalue::ResultIsOk {
                    value: Value::Temp(temp),
                } = value
                else {
                    continue;
                };
                let Some(def_block) = def_block_by_temp.get(temp.0).copied().flatten() else {
                    continue;
                };
                if in_backedge_loop(def_block, *header, pred, &preds) {
                    continue;
                }
                planned.push((*header, stmt_idx, preheader));
            }
        }
    }
    if planned.is_empty() {
        return;
    }

    planned.sort_unstable();
    planned.dedup();

    let mut removals: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut inserts: HashMap<usize, Vec<Stmt>> = HashMap::new();
    for (block_idx, stmt_idx, preheader) in planned {
        let stmt = func.blocks[block_idx].stmts[stmt_idx].clone();
        removals.entry(block_idx).or_default().push(stmt_idx);
        inserts.entry(preheader).or_default().push(stmt);
    }

    for (preheader, mut stmts) in inserts {
        if stmts.is_empty() {
            continue;
        }
        func.blocks[preheader].stmts.append(&mut stmts);
    }
    for (block_idx, mut to_remove) in removals {
        to_remove.sort_unstable_by(|lhs, rhs| rhs.cmp(lhs));
        for stmt_idx in to_remove {
            if stmt_idx < func.blocks[block_idx].stmts.len() {
                func.blocks[block_idx].stmts.remove(stmt_idx);
            }
        }
    }
}

fn collect_temp_def_blocks(func: &MirFunction) -> Vec<Option<usize>> {
    let mut defs = vec![None; func.temps.len()];
    for (block_idx, block) in func.blocks.iter().enumerate() {
        for stmt in &block.stmts {
            let Stmt::Assign { place, .. } = stmt else {
                continue;
            };
            let Place::Temp(temp) = place else {
                continue;
            };
            defs[temp.0] = Some(block_idx);
        }
    }
    defs
}

fn unique_loop_preheader(
    header: usize,
    loop_pred: usize,
    preds: &[Vec<usize>],
    doms: &[Vec<bool>],
) -> Option<usize> {
    let mut outside = Vec::new();
    for pred in &preds[header] {
        if *pred == loop_pred {
            continue;
        }
        if doms[*pred][header] {
            continue;
        }
        outside.push(*pred);
    }
    if outside.len() == 1 {
        outside.into_iter().next()
    } else {
        None
    }
}

fn in_backedge_loop(
    block: usize,
    header: usize,
    backedge_pred: usize,
    preds: &[Vec<usize>],
) -> bool {
    if block == header || block == backedge_pred {
        return true;
    }
    let mut seen = HashSet::new();
    let mut stack = vec![backedge_pred];
    while let Some(node) = stack.pop() {
        if !seen.insert(node) {
            continue;
        }
        if node == block {
            return true;
        }
        if node == header {
            continue;
        }
        for pred in &preds[node] {
            stack.push(*pred);
        }
    }
    false
}

fn scalar_replace_literals(func: &mut MirFunction) {
    let mut list_literals: HashMap<usize, Vec<Value>> = HashMap::new();
    let mut map_literals: HashMap<usize, Vec<(Literal, Value)>> = HashMap::new();

    for block in &func.blocks {
        for stmt in &block.stmts {
            if let Stmt::Assign { place, value, .. } = stmt
                && let Place::Temp(temp) = place
            {
                match value {
                    Rvalue::BuildList { items, .. } => {
                        list_literals.insert(temp.0, items.clone());
                    }
                    Rvalue::BuildMap { items, .. } => {
                        let mut literal_items = Vec::new();
                        let mut ok = true;
                        for (key, value) in items {
                            if let Value::Const(lit) = key {
                                literal_items.push((lit.clone(), value.clone()));
                            } else {
                                ok = false;
                                break;
                            }
                        }
                        if ok {
                            map_literals.insert(temp.0, literal_items);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    let mut replaceable: HashSet<usize> = list_literals
        .keys()
        .chain(map_literals.keys())
        .copied()
        .collect();
    if replaceable.is_empty() {
        return;
    }

    let mut used_in_get: HashSet<usize> = HashSet::new();
    for block in &func.blocks {
        for stmt in &block.stmts {
            match stmt {
                Stmt::Assign { value, .. } => match value {
                    Rvalue::Call { target, args, .. } => {
                        let temp_arg = args.first();
                        let key_arg = args.get(1);
                        match (target, temp_arg, key_arg) {
                            (
                                CallTarget::Function(name),
                                Some(Value::Temp(temp)),
                                Some(Value::Const(Literal::Integer(_))),
                            ) if name.as_str() == "__wr_list_get" => {
                                used_in_get.insert(temp.0);
                            }
                            (
                                CallTarget::Function(name),
                                Some(Value::Temp(temp)),
                                Some(Value::Const(key_lit)),
                            ) if name.as_str() == "__wr_map_get" => {
                                if !map_literals.contains_key(&temp.0)
                                    || !matches!(
                                        key_lit,
                                        Literal::Integer(_)
                                            | Literal::Boolean(_)
                                            | Literal::String(_)
                                            | Literal::Nil
                                    )
                                {
                                    replaceable.remove(&temp.0);
                                } else {
                                    used_in_get.insert(temp.0);
                                }
                            }
                            (CallTarget::Function(name), Some(Value::Temp(temp)), _)
                                if name.as_str() == "__wr_list_get" =>
                            {
                                if !list_literals.contains_key(&temp.0) {
                                    replaceable.remove(&temp.0);
                                }
                            }
                            (CallTarget::Function(name), Some(Value::Temp(temp)), _)
                                if name.as_str() == "__wr_map_get" =>
                            {
                                if !map_literals.contains_key(&temp.0) {
                                    replaceable.remove(&temp.0);
                                }
                            }
                            (_, Some(Value::Temp(temp)), _) => {
                                replaceable.remove(&temp.0);
                            }
                            _ => {}
                        }
                    }
                    _ => {
                        collect_disallowed_temp_uses(value, &mut replaceable);
                    }
                },
                Stmt::SetField { base, value, .. } => {
                    collect_disallowed_value_use(base, &mut replaceable);
                    collect_disallowed_value_use(value, &mut replaceable);
                }
                Stmt::RcInc { value, .. } | Stmt::RcDec { value, .. } => {
                    if let Value::Temp(temp) = value
                        && !list_literals.contains_key(&temp.0)
                        && !map_literals.contains_key(&temp.0)
                    {
                        replaceable.remove(&temp.0);
                    }
                }
                Stmt::Await { pending, .. }
                | Stmt::Fire { pending, .. }
                | Stmt::IterInit {
                    iterable: pending, ..
                } => {
                    collect_disallowed_value_use(pending, &mut replaceable);
                }
                Stmt::IterNext { iter, .. } => {
                    collect_disallowed_value_use(iter, &mut replaceable);
                }
                Stmt::Phi { sources, .. } => {
                    for (_, value) in sources {
                        collect_disallowed_value_use(value, &mut replaceable);
                    }
                }
            }
        }
        match &block.terminator {
            Terminator::Return { value, .. } => {
                if let Some(value) = value {
                    collect_disallowed_value_use(value, &mut replaceable);
                }
            }
            Terminator::Branch { cond, .. } => {
                collect_disallowed_value_use(cond, &mut replaceable);
            }
            Terminator::Switch { scrutinee, .. } => {
                collect_disallowed_value_use(scrutinee, &mut replaceable);
            }
            Terminator::Jump { .. } | Terminator::Unreachable { .. } => {}
        }
    }

    replaceable.retain(|temp| used_in_get.contains(temp));

    if replaceable.is_empty() {
        return;
    }

    for block in &mut func.blocks {
        let mut new_stmts = Vec::with_capacity(block.stmts.len());
        for mut stmt in block.stmts.drain(..) {
            let mut skip = false;
            match &mut stmt {
                Stmt::Assign { place, value, .. } => {
                    if let Place::Temp(temp) = place
                        && replaceable.contains(&temp.0)
                        && matches!(value, Rvalue::BuildList { .. } | Rvalue::BuildMap { .. })
                    {
                        skip = true;
                    }
                    if let Rvalue::Call { target, args, .. } = value
                        && let CallTarget::Function(name) = target
                    {
                        let arg0 = args.first().cloned();
                        let arg1 = args.get(1).cloned();
                        let mut replacement = None;
                        if name.as_str() == "__wr_list_get" {
                            if let (
                                Some(Value::Temp(temp)),
                                Some(Value::Const(Literal::Integer(idx))),
                            ) = (arg0, arg1)
                                && replaceable.contains(&temp.0)
                                && let Some(items) = list_literals.get(&temp.0)
                            {
                                let idx = idx as isize;
                                let value = if idx >= 0 && (idx as usize) < items.len() {
                                    items[idx as usize].clone()
                                } else {
                                    Value::Const(Literal::Nil)
                                };
                                replacement = Some(value);
                            }
                        } else if name.as_str() == "__wr_map_get"
                            && let (Some(Value::Temp(temp)), Some(Value::Const(key_lit))) =
                                (arg0, arg1)
                            && replaceable.contains(&temp.0)
                            && let Some(items) = map_literals.get(&temp.0)
                        {
                            let mut found = None;
                            for (key, val) in items {
                                if key == &key_lit {
                                    found = Some(val.clone());
                                    break;
                                }
                            }
                            replacement = Some(found.unwrap_or(Value::Const(Literal::Nil)));
                        }
                        if let Some(replacement_value) = replacement {
                            *value = Rvalue::Use(replacement_value);
                        }
                    }
                }
                Stmt::RcInc { value, .. } | Stmt::RcDec { value, .. } => {
                    if let Value::Temp(temp) = value
                        && replaceable.contains(&temp.0)
                    {
                        skip = true;
                    }
                }
                _ => {}
            }
            if !skip {
                new_stmts.push(stmt);
            }
        }
        block.stmts = new_stmts;
    }
}

fn collect_disallowed_temp_uses(value: &Rvalue, replaceable: &mut HashSet<usize>) {
    let mut temps = Vec::new();
    collect_temp_ids_rvalue(value, &mut temps);
    for temp in temps {
        replaceable.remove(&temp);
    }
}

fn collect_disallowed_value_use(value: &Value, replaceable: &mut HashSet<usize>) {
    if let Value::Temp(temp) = value {
        replaceable.remove(&temp.0);
    }
}

fn strength_reduce_mods(func: &mut MirFunction) {
    for block in &mut func.blocks {
        let mut seen: Vec<(Value, Value, Value)> = Vec::new();
        for stmt in &mut block.stmts {
            if let Stmt::Assign { place, value, .. } = stmt
                && let Rvalue::Binary {
                    op: BinaryOp::Mod,
                    lhs,
                    rhs,
                } = value
            {
                if let Some((_, _, prev)) = seen.iter().find(|(l, r, _)| l == lhs && r == rhs) {
                    *value = Rvalue::Use(prev.clone());
                    continue;
                }
                if let Place::Temp(temp) = place {
                    seen.push((lhs.clone(), rhs.clone(), Value::Temp(*temp)));
                }
            }
        }
    }
}

fn collect_temp_ids_value(value: &Value, escapes: &mut Vec<bool>) {
    if let Value::Temp(id) = value
        && let Some(slot) = escapes.get_mut(id.0)
    {
        *slot = true;
    }
}

fn collect_temp_ids_call_target(target: &CallTarget, escapes: &mut Vec<bool>) {
    match target {
        CallTarget::Function(_) => {}
        CallTarget::Method { receiver, .. } => collect_temp_ids_value(receiver, escapes),
        CallTarget::GuardedInterface { .. } => {}
        CallTarget::Indirect(value) => collect_temp_ids_value(value, escapes),
    }
}

fn collect_temp_ids_rvalue(value: &Rvalue, out: &mut Vec<usize>) {
    match value {
        Rvalue::Use(value)
        | Rvalue::ResultOk { value }
        | Rvalue::ResultErr { value }
        | Rvalue::ResultIsOk { value }
        | Rvalue::ResultUnwrap { value }
        | Rvalue::ResultErrUnwrap { value }
        | Rvalue::Crash { value } => collect_temp_ids_in_value(value, out),
        Rvalue::Unary { operand, .. } => collect_temp_ids_in_value(operand, out),
        Rvalue::Binary { lhs, rhs, .. } => {
            collect_temp_ids_in_value(lhs, out);
            collect_temp_ids_in_value(rhs, out);
        }
        Rvalue::GetField { base, .. } => collect_temp_ids_in_value(base, out),
        Rvalue::Call { target, args, .. } => {
            collect_temp_ids_in_call_target(target, out);
            for arg in args {
                collect_temp_ids_in_value(arg, out);
            }
        }
        Rvalue::ClassInit { .. } => {}
        Rvalue::Spawn {
            target, instance, ..
        } => {
            collect_temp_ids_in_value(target, out);
            collect_temp_ids_in_value(instance, out);
        }
        Rvalue::PoolNew { handles, .. } => collect_temp_ids_in_value(handles, out),
        Rvalue::BuildList { items, .. } => {
            for item in items {
                collect_temp_ids_in_value(item, out);
            }
        }
        Rvalue::BuildMap { items, .. } => {
            for (key, value) in items {
                collect_temp_ids_in_value(key, out);
                collect_temp_ids_in_value(value, out);
            }
        }
        Rvalue::StringInterp { parts, .. } => {
            for part in parts {
                if let crate::mir::ir::StringPartValue::Value(value) = part {
                    collect_temp_ids_in_value(value, out);
                }
            }
        }
        Rvalue::StrConcat { parts, .. } => {
            for part in parts {
                collect_temp_ids_in_value(part, out);
            }
        }
    }
}

fn collect_temp_ids_in_value(value: &Value, out: &mut Vec<usize>) {
    if let Value::Temp(id) = value {
        out.push(id.0);
    }
}

fn collect_temp_ids_in_call_target(target: &CallTarget, out: &mut Vec<usize>) {
    match target {
        CallTarget::Function(_) => {}
        CallTarget::Method { receiver, .. } => collect_temp_ids_in_value(receiver, out),
        CallTarget::GuardedInterface { .. } => {}
        CallTarget::Indirect(value) => collect_temp_ids_in_value(value, out),
    }
}

pub fn constant_fold(func: &mut MirFunction) {
    for block in &mut func.blocks {
        for stmt in &mut block.stmts {
            if let Stmt::Assign { value, .. } = stmt
                && let Some(lit) = fold_rvalue(value)
            {
                *value = Rvalue::Use(Value::Const(lit));
            }
        }
        if let Terminator::Branch {
            cond,
            then_target,
            else_target,
            span,
        } = &block.terminator
            && let Value::Const(Literal::Boolean(flag)) = cond
        {
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

pub fn simplify_branches(func: &mut MirFunction) {
    for block in &mut func.blocks {
        if let Terminator::Branch {
            cond,
            then_target,
            else_target,
            span,
        } = &block.terminator
            && let Value::Const(Literal::Boolean(flag)) = cond
        {
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

pub fn dead_code_elim(func: &mut MirFunction) {
    let used = collect_used_values(func);
    for block in &mut func.blocks {
        let mut new_stmts = Vec::with_capacity(block.stmts.len());
        for stmt in block.stmts.drain(..) {
            match &stmt {
                Stmt::Phi { .. } => {}
                Stmt::Assign { place, value, .. } => {
                    if is_pure_rvalue(value) && !place_is_used(place, &used) {
                        continue;
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
            (UnaryOp::Neg, Value::Const(Literal::Integer(v))) => Some(Literal::Integer(-v)),
            (UnaryOp::Neg, Value::Const(Literal::Float(v))) => Some(Literal::Float(-v)),
            (UnaryOp::Not, Value::Const(Literal::Boolean(v))) => Some(Literal::Boolean(!v)),
            _ => None,
        },
        Rvalue::Binary { op, lhs, rhs } => fold_binary(*op, lhs, rhs),
        _ => None,
    }
}

fn fold_binary(op: BinaryOp, lhs: &Value, rhs: &Value) -> Option<Literal> {
    match (lhs, rhs) {
        (Value::Const(Literal::Integer(a)), Value::Const(Literal::Integer(b))) => match op {
            BinaryOp::Add => Some(Literal::Integer(a + b)),
            BinaryOp::Sub => Some(Literal::Integer(a - b)),
            BinaryOp::Mul => Some(Literal::Integer(a * b)),
            BinaryOp::Div => Some(Literal::Integer(a / b)),
            BinaryOp::Mod => Some(Literal::Integer(a % b)),
            BinaryOp::Eq => Some(Literal::Boolean(a == b)),
            BinaryOp::Ne => Some(Literal::Boolean(a != b)),
            BinaryOp::Lt => Some(Literal::Boolean(a < b)),
            BinaryOp::Gt => Some(Literal::Boolean(a > b)),
            BinaryOp::Le => Some(Literal::Boolean(a <= b)),
            BinaryOp::Ge => Some(Literal::Boolean(a >= b)),
            _ => None,
        },
        (Value::Const(Literal::Float(a)), Value::Const(Literal::Float(b))) => match op {
            BinaryOp::Add => Some(Literal::Float(a + b)),
            BinaryOp::Sub => Some(Literal::Float(a - b)),
            BinaryOp::Mul => Some(Literal::Float(a * b)),
            BinaryOp::Div => Some(Literal::Float(a / b)),
            BinaryOp::Eq => Some(Literal::Boolean(a == b)),
            BinaryOp::Ne => Some(Literal::Boolean(a != b)),
            BinaryOp::Lt => Some(Literal::Boolean(a < b)),
            BinaryOp::Gt => Some(Literal::Boolean(a > b)),
            BinaryOp::Le => Some(Literal::Boolean(a <= b)),
            BinaryOp::Ge => Some(Literal::Boolean(a >= b)),
            _ => None,
        },
        (Value::Const(Literal::Boolean(a)), Value::Const(Literal::Boolean(b))) => match op {
            BinaryOp::And => Some(Literal::Boolean(*a && *b)),
            BinaryOp::Or => Some(Literal::Boolean(*a || *b)),
            BinaryOp::Eq => Some(Literal::Boolean(a == b)),
            BinaryOp::Ne => Some(Literal::Boolean(a != b)),
            _ => None,
        },
        (Value::Const(Literal::String(a)), Value::Const(Literal::String(b))) => match op {
            BinaryOp::Eq => Some(Literal::Boolean(a == b)),
            BinaryOp::Ne => Some(Literal::Boolean(a != b)),
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
            | Rvalue::StrConcat { .. }
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
                Stmt::Phi { sources, .. } => {
                    for (_, value) in sources {
                        collect_value(value, &mut used);
                    }
                }
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
                CallTarget::GuardedInterface { .. } => {}
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
        Rvalue::BuildList { items, .. } => {
            for item in items {
                collect_value(item, used);
            }
        }
        Rvalue::BuildMap { items, .. } => {
            for (key, value) in items {
                collect_value(key, used);
                collect_value(value, used);
            }
        }
        Rvalue::StringInterp { parts, .. } => {
            for part in parts {
                if let crate::mir::ir::StringPartValue::Value(value) = part {
                    collect_value(value, used);
                }
            }
        }
        Rvalue::StrConcat { parts, .. } => {
            for part in parts {
                collect_value(part, used);
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
    let terminator_uses_list: Vec<Vec<usize>> = func
        .blocks
        .iter()
        .map(|block| terminator_uses(&block.terminator, locals_len))
        .collect();
    let mut edge_rcdec_prepend: Vec<Vec<Stmt>> = vec![Vec::new(); func.blocks.len()];
    for (block_idx, term_uses) in terminator_uses_list.iter().enumerate() {
        if term_uses.is_empty() {
            continue;
        }
        let term_span = terminator_span(&func.blocks[block_idx].terminator);
        for succ in &succs[block_idx] {
            if preds[*succ].len() != 1 {
                continue;
            }
            for idx in term_uses {
                if *idx >= locals_len {
                    continue;
                }
                if !idx_is_ref(&types, *idx) {
                    continue;
                }
                if live_in[*succ][*idx] {
                    continue;
                }
                edge_rcdec_prepend[*succ].push(Stmt::RcDec {
                    value: value_from_idx(*idx, locals_len),
                    span: term_span,
                });
            }
        }
    }

    let mut next_temp_id = func.temps.len();
    for (block_idx, block) in func.blocks.iter_mut().enumerate() {
        let live_out_block = &live_out[block_idx];
        let term_uses = &terminator_uses_list[block_idx];
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
            && let Some(idx) = value_idx(value, locals_len)
        {
            exclude[idx] = true;
            init[idx] = false;
        }
        let term_span = terminator_span(&block.terminator);
        for idx in 0..total {
            if term_uses.contains(&idx) {
                continue;
            }
            if init[idx] && !live_out_block[idx] && !exclude[idx] && idx_is_ref(&types, idx) {
                new_stmts.push(Stmt::RcDec {
                    value: value_from_idx(idx, locals_len),
                    span: term_span,
                });
            }
        }

        if !edge_rcdec_prepend[block_idx].is_empty() {
            let mut combined =
                Vec::with_capacity(edge_rcdec_prepend[block_idx].len() + new_stmts.len());
            combined.append(&mut edge_rcdec_prepend[block_idx]);
            combined.extend(new_stmts);
            block.stmts = combined;
        } else {
            block.stmts = new_stmts;
        }
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
                CallTarget::GuardedInterface { .. } => {}
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
            if let Stmt::Phi { sources, place, .. } = stmt {
                for (_, value) in sources {
                    if let Some(idx) = value_idx(value, locals_len)
                        && !block_defs[idx]
                    {
                        block_uses[idx] = true;
                    }
                }
                block_defs[place_idx(place, locals_len)] = true;
                continue;
            }
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
        Stmt::Phi { place, .. } => vec![place_idx(place, locals_len)],
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
        Stmt::Phi { sources, .. } => {
            for (_, value) in sources {
                if let Some(idx) = value_idx(value, locals_len) {
                    out.push(idx);
                }
            }
        }
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
                CallTarget::GuardedInterface { .. } => {}
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
        Rvalue::BuildList { items, .. } => {
            for item in items {
                if let Some(idx) = value_idx(item, locals_len) {
                    out.push(idx);
                }
            }
        }
        Rvalue::BuildMap { items, .. } => {
            for (key, value) in items {
                if let Some(idx) = value_idx(key, locals_len) {
                    out.push(idx);
                }
                if let Some(idx) = value_idx(value, locals_len) {
                    out.push(idx);
                }
            }
        }
        Rvalue::StringInterp { parts, .. } => {
            for part in parts {
                if let crate::mir::ir::StringPartValue::Value(value) = part
                    && let Some(idx) = value_idx(value, locals_len)
                {
                    out.push(idx);
                }
            }
        }
        Rvalue::StrConcat { parts, .. } => {
            for part in parts {
                if let Some(idx) = value_idx(part, locals_len) {
                    out.push(idx);
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
            if let Some(value) = value
                && let Some(idx) = value_idx(value, locals_len)
            {
                out.push(idx);
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
    use crate::mir::ir::{BasicBlock, Local, LocalId, Temp, TempId};
    use crate::mir::lower::lower_module;
    use crate::parser::ast;
    use crate::parser::ast::AstNode;
    use crate::parser::parse;

    #[test]
    fn test_constant_folding_binary() {
        let input = "to f() -> Nothing:
    x = 1 + 2
";
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
                .any(|value| matches!(value, Rvalue::Use(Value::Const(Literal::Integer(3)))))
        );
    }

    #[test]
    fn test_dead_code_elim_unused_temp() {
        let input = "to f() -> Nothing:
    1 + 2
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = hir_lower::lower(root);
        let mut mir = lower_module(&module);
        let func = mir.functions.iter_mut().find(|f| f.name == "f").unwrap();
        dead_code_elim(func);
        let mut has_binary = false;
        for block in &func.blocks {
            for stmt in &block.stmts {
                if let Stmt::Assign { value, .. } = stmt
                    && matches!(value, Rvalue::Binary { .. })
                {
                    has_binary = true;
                }
            }
        }
        assert!(!has_binary, "expected dead code elim to remove binary");
    }

    #[test]
    fn test_inline_small_pure_function() {
        let input = "to add(a: Integer, b: Integer) -> Integer:
    return a + b

to run() -> Integer:
    return add(1, 2)
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = hir_lower::lower(root);
        let mut mir = lower_module(&module);
        let analysis = analyze_module(&mir);
        let inlined = inline_small_pure_functions(&mut mir, &analysis.call_graph);
        assert!(inlined > 0, "expected inliner to run at least once");

        let run = mir.functions.iter().find(|f| f.name == "run").unwrap();
        let mut has_call = false;
        for stmt in &run.blocks[run.entry.0].stmts {
            if let Stmt::Assign { value, .. } = stmt
                && matches!(value, Rvalue::Call { .. })
            {
                has_call = true;
            }
        }
        assert!(!has_call, "expected run() to inline add() call");
    }

    #[test]
    fn test_scalar_replace_map_literal_get() {
        let block = BasicBlock {
            stmts: vec![
                Stmt::Assign {
                    place: Place::Temp(TempId(0)),
                    value: Rvalue::BuildMap {
                        items: vec![
                            (
                                Value::Const(Literal::String("a".into())),
                                Value::Const(Literal::Integer(1)),
                            ),
                            (
                                Value::Const(Literal::String("b".into())),
                                Value::Const(Literal::Integer(2)),
                            ),
                        ],
                        alloc: AllocKind::LocalTemp,
                    },
                    span: TextRange::new(0.into(), 0.into()),
                },
                Stmt::Assign {
                    place: Place::Temp(TempId(1)),
                    value: Rvalue::Call {
                        kind: CallKind::Sync,
                        target: CallTarget::Function("__wr_map_get".into()),
                        args: vec![
                            Value::Temp(TempId(0)),
                            Value::Const(Literal::String("b".into())),
                        ],
                    },
                    span: TextRange::new(0.into(), 0.into()),
                },
            ],
            terminator: Terminator::Return {
                value: Some(Value::Temp(TempId(1))),
                span: TextRange::new(0.into(), 0.into()),
            },
        };
        let mut func = MirFunction {
            name: "map_get".into(),
            params: Vec::new(),
            locals: Vec::new(),
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

        run_function_passes(&mut func);

        let has_build_map = func.blocks[0].stmts.iter().any(|stmt| match stmt {
            Stmt::Assign { value, .. } => matches!(value, Rvalue::BuildMap { .. }),
            _ => false,
        });
        assert!(!has_build_map, "expected map literal to be replaced");
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
                        rhs: Value::Const(Literal::Integer(1)),
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

    #[test]
    fn test_rc_skips_integer_self_assign() {
        let span = TextRange::new(0.into(), 0.into());
        let mut func = MirFunction {
            name: "test_int".into(),
            params: vec![],
            locals: vec![Local {
                name: "x".into(),
                mutable: true,
                ty: MirType::Integer,
            }],
            temps: vec![],
            blocks: vec![BasicBlock {
                stmts: vec![Stmt::Assign {
                    place: Place::Local(LocalId(0)),
                    value: Rvalue::Binary {
                        op: BinaryOp::Add,
                        lhs: Value::Local(LocalId(0)),
                        rhs: Value::Const(Literal::Integer(1)),
                    },
                    span,
                }],
                terminator: Terminator::Return { value: None, span },
            }],
            entry: BlockId(0),
            suspendable: false,
        };
        insert_rc(&mut func);
        assert!(
            !func.blocks[0]
                .stmts
                .iter()
                .any(|stmt| matches!(stmt, Stmt::RcInc { .. } | Stmt::RcDec { .. })),
            "typed integer lane should not emit RC traffic"
        );
    }

    #[test]
    fn result_peephole_elides_proven_ok_wrapper_ops() {
        let span = TextRange::new(0.into(), 0.into());
        let mut func = MirFunction {
            name: "result_ok_fastpath".into(),
            params: vec![],
            locals: vec![Local {
                name: "v".into(),
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
                Temp {
                    ty: MirType::Boolean,
                },
            ],
            blocks: vec![BasicBlock {
                stmts: vec![
                    Stmt::Assign {
                        place: Place::Temp(TempId(0)),
                        value: Rvalue::ResultOk {
                            value: Value::Local(LocalId(0)),
                        },
                        span,
                    },
                    Stmt::Assign {
                        place: Place::Temp(TempId(1)),
                        value: Rvalue::ResultUnwrap {
                            value: Value::Temp(TempId(0)),
                        },
                        span,
                    },
                    Stmt::Assign {
                        place: Place::Temp(TempId(2)),
                        value: Rvalue::ResultIsOk {
                            value: Value::Temp(TempId(0)),
                        },
                        span,
                    },
                ],
                terminator: Terminator::Return { value: None, span },
            }],
            entry: BlockId(0),
            suspendable: false,
        };

        result_peephole(&mut func);

        let stmts = &func.blocks[0].stmts;
        assert!(matches!(
            stmts[1],
            Stmt::Assign {
                value: Rvalue::Use(Value::Local(LocalId(0))),
                ..
            }
        ));
        assert!(matches!(
            stmts[2],
            Stmt::Assign {
                value: Rvalue::Use(Value::Const(Literal::Boolean(true))),
                ..
            }
        ));
    }

    #[test]
    fn result_peephole_preserves_error_propagation_shape() {
        let span = TextRange::new(0.into(), 0.into());
        let mut func = MirFunction {
            name: "result_err_fastpath".into(),
            params: vec![],
            locals: vec![],
            temps: vec![
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
            ],
            blocks: vec![BasicBlock {
                stmts: vec![
                    Stmt::Assign {
                        place: Place::Temp(TempId(0)),
                        value: Rvalue::ResultErr {
                            value: Value::Const(Literal::String("boom".into())),
                        },
                        span,
                    },
                    Stmt::Assign {
                        place: Place::Temp(TempId(1)),
                        value: Rvalue::ResultUnwrap {
                            value: Value::Temp(TempId(0)),
                        },
                        span,
                    },
                    Stmt::Assign {
                        place: Place::Temp(TempId(2)),
                        value: Rvalue::ResultErrUnwrap {
                            value: Value::Temp(TempId(0)),
                        },
                        span,
                    },
                    Stmt::Assign {
                        place: Place::Temp(TempId(3)),
                        value: Rvalue::ResultIsOk {
                            value: Value::Temp(TempId(0)),
                        },
                        span,
                    },
                ],
                terminator: Terminator::Return { value: None, span },
            }],
            entry: BlockId(0),
            suspendable: false,
        };

        result_peephole(&mut func);

        let stmts = &func.blocks[0].stmts;
        assert!(matches!(
            stmts[1],
            Stmt::Assign {
                value: Rvalue::ResultUnwrap {
                    value: Value::Temp(TempId(0))
                },
                ..
            }
        ));
        assert!(matches!(
            stmts[2],
            Stmt::Assign {
                value: Rvalue::Use(Value::Const(Literal::String(_))),
                ..
            }
        ));
        assert!(matches!(
            stmts[3],
            Stmt::Assign {
                value: Rvalue::Use(Value::Const(Literal::Boolean(false))),
                ..
            }
        ));
    }

    #[test]
    fn devirtualize_monomorphic_method_call() {
        let span = TextRange::new(0.into(), 0.into());
        let mut func = MirFunction {
            name: "caller".into(),
            params: vec![],
            locals: vec![Local {
                name: "recv".into(),
                mutable: false,
                ty: MirType::Named("Foo".into()),
            }],
            temps: vec![Temp {
                ty: MirType::Unknown,
            }],
            blocks: vec![BasicBlock {
                stmts: vec![Stmt::Assign {
                    place: Place::Temp(TempId(0)),
                    value: Rvalue::Call {
                        kind: CallKind::Sync,
                        target: CallTarget::Method {
                            receiver: Value::Local(LocalId(0)),
                            method: SmolStr::new("Foo.bar"),
                            method_id: Some(1),
                        },
                        args: vec![Value::Const(Literal::Integer(1))],
                    },
                    span,
                }],
                terminator: Terminator::Return {
                    value: Some(Value::Temp(TempId(0))),
                    span,
                },
            }],
            entry: BlockId(0),
            suspendable: false,
        };

        let types = FunctionTypes {
            locals: vec![MirType::Named("Foo".into())],
            temps: vec![MirType::Unknown],
        };
        devirtualize_calls(&mut func, Some(&types));

        let call = match &func.blocks[0].stmts[0] {
            Stmt::Assign { value, .. } => value,
            _ => panic!("expected call"),
        };
        let Rvalue::Call { target, args, .. } = call else {
            panic!("expected call");
        };
        assert!(matches!(target, CallTarget::Function(name) if name.as_str() == "Foo.bar"));
        assert_eq!(args.len(), 2);
    }

    #[test]
    fn devirtualize_skips_unknown_receiver() {
        let span = TextRange::new(0.into(), 0.into());
        let mut func = MirFunction {
            name: "caller".into(),
            params: vec![],
            locals: vec![Local {
                name: "recv".into(),
                mutable: false,
                ty: MirType::Unknown,
            }],
            temps: vec![Temp {
                ty: MirType::Unknown,
            }],
            blocks: vec![BasicBlock {
                stmts: vec![Stmt::Assign {
                    place: Place::Temp(TempId(0)),
                    value: Rvalue::Call {
                        kind: CallKind::Sync,
                        target: CallTarget::Method {
                            receiver: Value::Local(LocalId(0)),
                            method: SmolStr::new("bar"),
                            method_id: Some(1),
                        },
                        args: vec![Value::Const(Literal::Integer(1))],
                    },
                    span,
                }],
                terminator: Terminator::Return {
                    value: Some(Value::Temp(TempId(0))),
                    span,
                },
            }],
            entry: BlockId(0),
            suspendable: false,
        };

        let types = FunctionTypes {
            locals: vec![MirType::Unknown],
            temps: vec![MirType::Unknown],
        };
        devirtualize_calls(&mut func, Some(&types));

        let call = match &func.blocks[0].stmts[0] {
            Stmt::Assign { value, .. } => value,
            _ => panic!("expected call"),
        };
        let Rvalue::Call { target, .. } = call else {
            panic!("expected call");
        };
        assert!(matches!(target, CallTarget::Method { .. }));
    }

    #[test]
    fn tree_shake_removes_unreachable() {
        let span = TextRange::new(0.into(), 0.into());
        let mut module = MirModule {
            functions: vec![
                MirFunction {
                    name: "main".into(),
                    params: vec![],
                    locals: vec![],
                    temps: vec![Temp {
                        ty: MirType::Unknown,
                    }],
                    blocks: vec![BasicBlock {
                        stmts: vec![Stmt::Assign {
                            place: Place::Temp(TempId(0)),
                            value: Rvalue::Call {
                                kind: CallKind::Sync,
                                target: CallTarget::Function("alive".into()),
                                args: vec![],
                            },
                            span,
                        }],
                        terminator: Terminator::Return {
                            value: Some(Value::Temp(TempId(0))),
                            span,
                        },
                    }],
                    entry: BlockId(0),
                    suspendable: false,
                },
                MirFunction {
                    name: "alive".into(),
                    params: vec![],
                    locals: vec![],
                    temps: vec![],
                    blocks: vec![BasicBlock {
                        stmts: vec![],
                        terminator: Terminator::Return { value: None, span },
                    }],
                    entry: BlockId(0),
                    suspendable: false,
                },
                MirFunction {
                    name: "dead".into(),
                    params: vec![],
                    locals: vec![],
                    temps: vec![],
                    blocks: vec![BasicBlock {
                        stmts: vec![],
                        terminator: Terminator::Return { value: None, span },
                    }],
                    entry: BlockId(0),
                    suspendable: false,
                },
            ],
            type_tags: vec![],
            classes: vec![],
        };

        run_module_passes(&mut module);
        assert!(module.functions.iter().any(|f| f.name.as_str() == "main"));
        assert!(module.functions.iter().any(|f| f.name.as_str() == "alive"));
        assert!(!module.functions.iter().any(|f| f.name.as_str() == "dead"));
    }

    #[test]
    fn clone_small_hot_function_into_callers() {
        let span = TextRange::new(0.into(), 0.into());
        let small = MirFunction {
            name: "small".into(),
            params: vec![],
            locals: vec![],
            temps: vec![],
            blocks: vec![BasicBlock {
                stmts: vec![],
                terminator: Terminator::Return { value: None, span },
            }],
            entry: BlockId(0),
            suspendable: false,
        };
        let mut module = MirModule {
            functions: vec![
                MirFunction {
                    name: "main".into(),
                    params: vec![],
                    locals: vec![],
                    temps: vec![Temp {
                        ty: MirType::Unknown,
                    }],
                    blocks: vec![BasicBlock {
                        stmts: vec![Stmt::Assign {
                            place: Place::Temp(TempId(0)),
                            value: Rvalue::Call {
                                kind: CallKind::Sync,
                                target: CallTarget::Function("small".into()),
                                args: vec![],
                            },
                            span,
                        }],
                        terminator: Terminator::Return {
                            value: Some(Value::Temp(TempId(0))),
                            span,
                        },
                    }],
                    entry: BlockId(0),
                    suspendable: false,
                },
                MirFunction {
                    name: "helper".into(),
                    params: vec![],
                    locals: vec![],
                    temps: vec![Temp {
                        ty: MirType::Unknown,
                    }],
                    blocks: vec![BasicBlock {
                        stmts: vec![Stmt::Assign {
                            place: Place::Temp(TempId(0)),
                            value: Rvalue::Call {
                                kind: CallKind::Sync,
                                target: CallTarget::Function("small".into()),
                                args: vec![],
                            },
                            span,
                        }],
                        terminator: Terminator::Return {
                            value: Some(Value::Temp(TempId(0))),
                            span,
                        },
                    }],
                    entry: BlockId(0),
                    suspendable: false,
                },
                small,
            ],
            type_tags: vec![],
            classes: vec![],
        };

        run_module_passes(&mut module);
        let has_clone = module
            .functions
            .iter()
            .any(|f| f.name.as_str().contains("small__clone"));
        assert!(has_clone, "expected cloned function");
    }
}
