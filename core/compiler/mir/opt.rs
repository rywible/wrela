use crate::hir::{BinaryOp, Literal, UnaryOp};
use crate::mir::analysis::{CallGraph, FunctionTypes, analyze_module};
use crate::mir::ir::{
    AllocKind, BasicBlock, CallKind, CallTarget, Local, LocalId, MirFunction, MirModule, MirType,
    Place, Rvalue, Stmt, Terminator, Value,
};
use rowan::TextRange;
use smol_str::SmolStr;
use std::collections::{HashMap, HashSet, VecDeque};

pub fn run_function_passes(func: &mut MirFunction) {
    run_function_passes_with_types(func, None);
}

pub fn run_function_passes_with_types(func: &mut MirFunction, types: Option<&FunctionTypes>) {
    devirtualize_calls(func, types);
    specialize_container_ops(func, types);
    annotate_allocs(func);
    constant_fold(func);
    simplify_branches(func);
    dead_code_elim(func);
    convert_to_ssa(func);
    result_peephole(func);
    scalar_replace_literals(func);
    strength_reduce_mods(func);
    insert_rc(func);
}

pub fn run_module_passes(module: &mut MirModule) {
    let analysis = analyze_module(module);
    clone_small_hot_functions(module, &analysis.call_graph);
    let analysis = analyze_module(module);
    tree_shake_unused_functions(module, &analysis.call_graph);
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
            let Stmt::Assign { value, .. } = stmt else { continue };
            let Rvalue::Call { kind, target, args } = value else { continue };
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
            let MirType::Named(class_name) = recv_ty else { continue };
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
            let Stmt::Assign { value, .. } = stmt else { continue };
            let Rvalue::Call { kind, target, args } = value else { continue };
            if *kind != CallKind::Sync {
                continue;
            }
            let CallTarget::Method { receiver, method, .. } = target else { continue };
            let recv_ty = value_ty_with_slices(receiver, &locals, &temps);
            let MirType::Named(class_name) = recv_ty else { continue };
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
                let Stmt::Assign { value, .. } = stmt else { continue };
                let Rvalue::Call { target, .. } = value else { continue };
                let CallTarget::Function(name) = target else { continue };
                if graph.call_count(name) < 2 {
                    continue;
                }
                let Some(callee) = func_map.get(name) else { continue };
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
            if let Stmt::Assign { place, value, .. } = stmt {
                if let Place::Temp(dst) = place {
                    let mut used = Vec::new();
                    collect_temp_ids_rvalue(value, &mut used);
                    deps[dst.0].extend(used);
                }
            }
            if let Stmt::SetField { value, .. } = stmt {
                collect_temp_ids_value(value, &mut escapes);
            }
        }
        match &block.terminator {
            Terminator::Return { value: Some(value), .. } => {
                collect_temp_ids_value(value, &mut escapes);
            }
            _ => {}
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
                    Rvalue::Spawn { target, instance, .. } => {
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
            if let Stmt::Assign { place, value, .. } = stmt {
                if let Place::Temp(dst) = place {
                    let alloc = if escapes[dst.0] {
                        AllocKind::Escaping
                    } else {
                        AllocKind::LocalTemp
                    };
                    match value {
                        Rvalue::BuildList { alloc: slot, .. }
                        | Rvalue::BuildMap { alloc: slot, .. }
                        | Rvalue::StringInterp { alloc: slot, .. } => {
                            *slot = alloc;
                        }
                        _ => {}
                    }
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
                def_blocks[local.0].push(block_idx);
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
        let mut new_stmts = Vec::with_capacity(
            block.stmts.len() + phi_locals_per_block[block_idx].len(),
        );
        for local in &phi_locals_per_block[block_idx] {
            new_stmts.push(Stmt::Phi {
                place: Place::Local(*local),
                sources: Vec::new(),
                original: *local,
                span: TextRange::empty(0.into()),
            });
        }
        new_stmts.extend(block.stmts.drain(..));
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

fn compute_dominators(
    blocks_len: usize,
    entry: usize,
    preds: &[Vec<usize>],
) -> Vec<Vec<bool>> {
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
        if let Some(p) = parent {
            if *p != block {
                tree[*p].push(block);
            }
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
        Stmt::SetField { .. }
        | Stmt::RcInc { .. }
        | Stmt::RcDec { .. }
        | Stmt::Fire { .. } => Vec::new(),
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
                place,
                original,
                ..
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
                sources,
                original,
                ..
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
    let orig = locals
        .get(original.0)
        .cloned()
        .unwrap_or(Local {
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
    if let Value::Local(local) = value {
        if let Some(current) = stacks.get(local.0).and_then(|stack| stack.last()) {
            *value = Value::Local(*current);
        }
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
        Rvalue::Spawn { target, instance, .. } => {
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
    }
}

fn rename_call_target(target: &mut CallTarget, stacks: &[Vec<LocalId>]) {
    match target {
        CallTarget::Function(_) => {}
        CallTarget::Method { receiver, .. } => rename_value(receiver, stacks),
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
    for block in &mut func.blocks {
        let mut result_sources: HashMap<usize, (bool, Value)> = HashMap::new();
        for stmt in &mut block.stmts {
            if let Stmt::Assign { place, value, .. } = stmt {
                if let Place::Temp(temp) = place {
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
            }
        }

        for stmt in &mut block.stmts {
            if let Stmt::Assign { value, .. } = stmt {
                match value {
                    Rvalue::ResultUnwrap { value: inner } => {
                        if let Value::Temp(temp) = inner {
                            if let Some((is_ok, src)) = result_sources.get(&temp.0) {
                                if *is_ok {
                                    *value = Rvalue::Use(src.clone());
                                }
                            }
                        }
                    }
                    Rvalue::ResultErrUnwrap { value: inner } => {
                        if let Value::Temp(temp) = inner {
                            if let Some((is_ok, src)) = result_sources.get(&temp.0) {
                                if !*is_ok {
                                    *value = Rvalue::Use(src.clone());
                                }
                            }
                        }
                    }
                    Rvalue::ResultIsOk { value: inner } => {
                        if let Value::Temp(temp) = inner {
                            if let Some((is_ok, _)) = result_sources.get(&temp.0) {
                                *value =
                                    Rvalue::Use(Value::Const(Literal::Boolean(*is_ok)));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

fn scalar_replace_literals(func: &mut MirFunction) {
    let mut list_literals: HashMap<usize, Vec<Value>> = HashMap::new();
    let mut map_literals: HashMap<usize, Vec<(Literal, Value)>> = HashMap::new();

    for block in &func.blocks {
        for stmt in &block.stmts {
            if let Stmt::Assign { place, value, .. } = stmt {
                if let Place::Temp(temp) = place {
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
                        let temp_arg = args.get(0);
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
                    if let Value::Temp(temp) = value {
                        if !list_literals.contains_key(&temp.0) && !map_literals.contains_key(&temp.0)
                        {
                            replaceable.remove(&temp.0);
                        }
                    }
                }
                Stmt::Await { pending, .. }
                | Stmt::Fire { pending, .. }
                | Stmt::IterInit { iterable: pending, .. } => {
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
                    if let Place::Temp(temp) = place {
                        if replaceable.contains(&temp.0) {
                            if matches!(value, Rvalue::BuildList { .. } | Rvalue::BuildMap { .. }) {
                                skip = true;
                            }
                        }
                    }
                    if let Rvalue::Call { target, args, .. } = value {
                        if let CallTarget::Function(name) = target {
                            let arg0 = args.get(0).cloned();
                            let arg1 = args.get(1).cloned();
                            let mut replacement = None;
                            if name.as_str() == "__wr_list_get" {
                                if let (
                                    Some(Value::Temp(temp)),
                                    Some(Value::Const(Literal::Integer(idx))),
                                ) = (arg0, arg1)
                                {
                                    if replaceable.contains(&temp.0) {
                                        if let Some(items) = list_literals.get(&temp.0) {
                                            let idx = idx as isize;
                                            let value = if idx >= 0
                                                && (idx as usize) < items.len()
                                            {
                                                items[idx as usize].clone()
                                            } else {
                                                Value::Const(Literal::Nil)
                                            };
                                            replacement = Some(value);
                                        }
                                    }
                                }
                            } else if name.as_str() == "__wr_map_get" {
                                if let (Some(Value::Temp(temp)), Some(Value::Const(key_lit))) =
                                    (arg0, arg1)
                                {
                                    if replaceable.contains(&temp.0) {
                                        if let Some(items) = map_literals.get(&temp.0) {
                                            let mut found = None;
                                            for (key, val) in items {
                                                if key == &key_lit {
                                                    found = Some(val.clone());
                                                    break;
                                                }
                                            }
                                            replacement =
                                                Some(found.unwrap_or(Value::Const(Literal::Nil)));
                                        }
                                    }
                                }
                            }
                            if let Some(replacement_value) = replacement {
                                *value = Rvalue::Use(replacement_value);
                            }
                        }
                    }
                }
                Stmt::RcInc { value, .. } | Stmt::RcDec { value, .. } => {
                    if let Value::Temp(temp) = value {
                        if replaceable.contains(&temp.0) {
                            skip = true;
                        }
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
            if let Stmt::Assign { place, value, .. } = stmt {
                if let Rvalue::Binary {
                    op: BinaryOp::Mod,
                    lhs,
                    rhs,
                } = value
                {
                    if let Some((_, _, prev)) =
                        seen.iter().find(|(l, r, _)| l == lhs && r == rhs)
                    {
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
}

fn collect_temp_ids_value(value: &Value, escapes: &mut Vec<bool>) {
    if let Value::Temp(id) = value {
        if let Some(slot) = escapes.get_mut(id.0) {
            *slot = true;
        }
    }
}

fn collect_temp_ids_call_target(target: &CallTarget, escapes: &mut Vec<bool>) {
    match target {
        CallTarget::Function(_) => {}
        CallTarget::Method { receiver, .. } => collect_temp_ids_value(receiver, escapes),
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
        Rvalue::Spawn { target, instance, .. } => {
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
        CallTarget::Indirect(value) => collect_temp_ids_in_value(value, out),
    }
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
            if let Value::Const(Literal::Boolean(flag)) = cond {
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
            if let Value::Const(Literal::Boolean(flag)) = cond {
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
                Stmt::Phi { .. } => {}
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
        {
            if let Some(idx) = value_idx(value, locals_len) {
                exclude[idx] = true;
                init[idx] = false;
            }
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
            let mut combined = Vec::with_capacity(
                edge_rcdec_prepend[block_idx].len() + new_stmts.len(),
            );
            combined.extend(edge_rcdec_prepend[block_idx].drain(..));
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
                    if let Some(idx) = value_idx(value, locals_len) {
                        if !block_defs[idx] {
                            block_uses[idx] = true;
                        }
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
                if let Stmt::Assign { value, .. } = stmt {
                    if matches!(value, Rvalue::Binary { .. }) {
                        has_binary = true;
                    }
                }
            }
        }
        assert!(!has_binary, "expected dead code elim to remove binary");
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
            temps: vec![Temp { ty: MirType::Unknown }, Temp { ty: MirType::Unknown }],
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
            temps: vec![Temp { ty: MirType::Unknown }],
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
            temps: vec![Temp { ty: MirType::Unknown }],
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
                    temps: vec![Temp { ty: MirType::Unknown }],
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
                    temps: vec![Temp { ty: MirType::Unknown }],
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
                    temps: vec![Temp { ty: MirType::Unknown }],
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
