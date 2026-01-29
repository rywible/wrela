use crate::hir::{
    self, AssignOp, BinaryOp, Expr, FunctionTypeInfo, Literal, Module, Stmt as HirStmt, Type,
    TypeInfo, UnaryOp,
};
use crate::mir::ir::Stmt as MirStmt;
use crate::mir::ir::*;
use rowan::TextRange;
use smol_str::SmolStr;
use std::collections::{HashMap, HashSet};

pub fn lower_module(module: &Module) -> MirModule {
    lower_module_with_types(module, None)
}

pub fn lower_module_with_types(module: &Module, type_info: Option<&TypeInfo>) -> MirModule {
    const CLASS_ID_BASE: usize = 100;
    let mut type_tags = Vec::new();
    let mut tag_map = HashMap::new();
    let mut class_fields = HashMap::new();
    let mut classes = Vec::new();
    let mut class_method_ids = HashMap::new();
    let mut method_ids = HashSet::new();
    for (_idx, class) in module.classes.iter() {
        let id = TypeTagId(type_tags.len() + CLASS_ID_BASE);
        type_tags.push(class.name.clone());
        tag_map.insert(class.name.clone(), id);
        let fields: Vec<SmolStr> = class
            .fields
            .iter()
            .map(|field| field.name.clone())
            .collect();
        class_fields.insert(class.name.clone(), fields);
        let mut methods = Vec::new();
        let mut method_map = HashMap::new();
        for (idx, method_id) in class.methods.iter().enumerate() {
            let method = &module.functions[*method_id];
            method_ids.insert(method_id.into_raw());
            method_map.insert(method.name.clone(), idx as u32);
            methods.push(MirMethodInfo {
                name: method.name.clone(),
                func: method.name.clone(),
                arity: method.params.len() + 1,
                id: idx as u32,
            });
        }
        class_method_ids.insert(class.name.clone(), method_map);
        classes.push(MirClassInfo {
            name: class.name.clone(),
            id,
            fields: class_fields.get(&class.name).cloned().unwrap_or_default(),
            methods,
        });
    }

    let mut functions = Vec::new();
    let mut function_names: HashSet<SmolStr> = module
        .functions
        .iter()
        .map(|(_, func)| func.name.clone())
        .collect();
    for name in builtin_function_names() {
        function_names.insert(name);
    }
    let result_functions: HashSet<SmolStr> = module
        .functions
        .iter()
        .filter_map(|(_, func)| {
            let ret = func.ret_type.as_ref()?;
            if ret.name == "Result" {
                Some(func.name.clone())
            } else {
                None
            }
        })
        .collect();
    for (_idx, func) in module.functions.iter() {
        let Some(body) = &func.body else {
            continue;
        };
        let is_method = method_ids.contains(&_idx.into_raw());
        let fn_types = type_info.and_then(|info| info.function(_idx));
        functions.push(lower_function(
            func,
            body,
            &tag_map,
            &class_fields,
            &function_names,
            &result_functions,
            &class_method_ids,
            is_method,
            fn_types,
        ));
    }
    MirModule {
        functions,
        type_tags,
        classes,
    }
}

fn lower_function(
    func: &hir::Function,
    body: &hir::Body,
    type_tags: &HashMap<SmolStr, TypeTagId>,
    class_fields: &HashMap<SmolStr, Vec<SmolStr>>,
    function_names: &HashSet<SmolStr>,
    result_functions: &HashSet<SmolStr>,
    class_method_ids: &HashMap<SmolStr, HashMap<SmolStr, u32>>,
    is_method: bool,
    type_info: Option<&FunctionTypeInfo>,
) -> MirFunction {
    let mut lowerer = FunctionLowerer::new(
        func.name.clone(),
        type_tags,
        class_fields,
        function_names,
        result_functions,
        class_method_ids,
        matches!(
            func.ret_type.as_ref().map(|t| t.name.as_str()),
            Some("Result")
        ),
        type_info,
    );

    if is_method {
        let local = lowerer.new_local(
            SmolStr::new("it"),
            false,
            lowerer.local_type_for_name(&SmolStr::new("it")),
        );
        lowerer.declare_local(SmolStr::new("it"), local);
        lowerer.params.push(local);
    }
    for param in &func.params {
        let local = lowerer.new_local(
            param.name.clone(),
            false,
            lowerer.local_type_for_name(&param.name),
        );
        lowerer.declare_local(param.name.clone(), local);
        let is_result = matches!(param.ty.as_ref().map(|t| t.name.as_str()), Some("Result"));
        lowerer.declare_resultness(param.name.clone(), is_result);
        lowerer.params.push(local);
    }

    let entry = lowerer.new_block();
    lowerer.current_block = entry;
    lowerer.lower_stmt_block(body, &body.root_stmts);
    if lowerer.block_is_open(lowerer.current_block) {
        lowerer.set_terminator(Terminator::Return {
            value: None,
            span: TextRange::empty(0.into()),
        });
    }

    MirFunction {
        name: lowerer.name,
        params: lowerer.params,
        locals: lowerer.locals,
        temps: lowerer.temps,
        blocks: lowerer.blocks,
        entry,
        suspendable: lowerer.suspendable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hir::lower as hir_lower;
    use crate::parser::ast;
    use crate::parser::ast::AstNode;
    use crate::parser::parse;

    #[test]
    fn test_lower_marks_suspendable() {
        let input = "\
A Whale:\n    can swim() -> Bool:\n        return true\n\nto f():\n    w = detach Whale() * 1\n    return await w.swim()\n";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = hir_lower::lower(root);
        let mir = lower_module(&module);
        let func = mir.functions.iter().find(|f| f.name == "f").unwrap();
        assert!(func.suspendable);
    }

    #[test]
    fn test_lower_if_creates_blocks() {
        let input = "to f():\n    if true:\n        x = 1\n    else:\n        x = 2\n";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = hir_lower::lower(root);
        let mir = lower_module(&module);
        let func = mir.functions.iter().find(|f| f.name == "f").unwrap();
        assert!(func.blocks.len() >= 3);
    }
}

struct LoopTarget {
    break_target: BlockId,
    continue_target: BlockId,
}

struct FunctionLowerer {
    name: SmolStr,
    params: Vec<LocalId>,
    locals: Vec<Local>,
    temps: Vec<Temp>,
    blocks: Vec<BasicBlock>,
    current_block: BlockId,
    suspendable: bool,
    scopes: Vec<HashMap<SmolStr, LocalId>>,
    result_scopes: Vec<HashMap<SmolStr, bool>>,
    loop_stack: Vec<LoopTarget>,
    type_tags: HashMap<SmolStr, TypeTagId>,
    class_fields: HashMap<SmolStr, Vec<SmolStr>>,
    class_method_ids: HashMap<SmolStr, HashMap<SmolStr, u32>>,
    function_names: HashSet<SmolStr>,
    result_functions: HashSet<SmolStr>,
    returns_result: bool,
    type_info: Option<FunctionTypeInfo>,
}

impl FunctionLowerer {
    fn new(
        name: SmolStr,
        type_tags: &HashMap<SmolStr, TypeTagId>,
        class_fields: &HashMap<SmolStr, Vec<SmolStr>>,
        function_names: &HashSet<SmolStr>,
        result_functions: &HashSet<SmolStr>,
        class_method_ids: &HashMap<SmolStr, HashMap<SmolStr, u32>>,
        returns_result: bool,
        type_info: Option<&FunctionTypeInfo>,
    ) -> Self {
        Self {
            name,
            params: Vec::new(),
            locals: Vec::new(),
            temps: Vec::new(),
            blocks: Vec::new(),
            current_block: BlockId(0),
            suspendable: false,
            scopes: vec![HashMap::new()],
            result_scopes: vec![HashMap::new()],
            loop_stack: Vec::new(),
            type_tags: type_tags.clone(),
            class_fields: class_fields.clone(),
            class_method_ids: class_method_ids.clone(),
            function_names: function_names.clone(),
            result_functions: result_functions.clone(),
            returns_result,
            type_info: type_info.cloned(),
        }
    }

    fn new_block(&mut self) -> BlockId {
        let id = BlockId(self.blocks.len());
        self.blocks.push(BasicBlock {
            stmts: Vec::new(),
            terminator: Terminator::Unreachable {
                span: TextRange::empty(0.into()),
            },
        });
        id
    }

    fn block_is_open(&self, block: BlockId) -> bool {
        matches!(
            self.blocks[block.0].terminator,
            Terminator::Unreachable { .. }
        )
    }

    fn set_terminator(&mut self, term: Terminator) {
        self.blocks[self.current_block.0].terminator = term;
    }

    fn push_stmt(&mut self, stmt: Stmt) {
        self.blocks[self.current_block.0].stmts.push(stmt);
    }

    fn local_type_for_name(&self, name: &SmolStr) -> MirType {
        self.type_info
            .as_ref()
            .and_then(|info| info.local_types.get(name))
            .map(mir_type_from_type)
            .unwrap_or(MirType::Unknown)
    }

    fn expr_type(&self, expr_id: hir::Idx<Expr>) -> MirType {
        self.type_info
            .as_ref()
            .and_then(|info| info.expr_types.get(&expr_id.into_raw()))
            .map(mir_type_from_type)
            .unwrap_or(MirType::Unknown)
    }

    fn new_temp_for_expr(&mut self, expr_id: hir::Idx<Expr>) -> TempId {
        let ty = self.expr_type(expr_id);
        self.new_temp(ty)
    }

    fn new_temp(&mut self, ty: MirType) -> TempId {
        let id = TempId(self.temps.len());
        self.temps.push(Temp { ty });
        id
    }

    fn new_local(&mut self, name: SmolStr, mutable: bool, ty: MirType) -> LocalId {
        let id = LocalId(self.locals.len());
        self.locals.push(Local { name, mutable, ty });
        id
    }

    fn new_temp_local(&mut self) -> LocalId {
        let name = SmolStr::new(format!("$tmp{}", self.locals.len()));
        self.new_local(name, true, MirType::Unknown)
    }

    fn declare_local(&mut self, name: SmolStr, local: LocalId) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, local);
        }
    }

    fn declare_resultness(&mut self, name: SmolStr, is_result: bool) {
        if let Some(scope) = self.result_scopes.last_mut() {
            scope.insert(name, is_result);
        }
    }

    fn set_resultness(&mut self, name: &SmolStr, is_result: bool) {
        for scope in self.result_scopes.iter_mut().rev() {
            if let Some(entry) = scope.get_mut(name) {
                *entry = is_result;
                return;
            }
        }
    }

    fn resolve_resultness(&self, name: &SmolStr) -> Option<bool> {
        for scope in self.result_scopes.iter().rev() {
            if let Some(result) = scope.get(name) {
                return Some(*result);
            }
        }
        None
    }

    fn resolve_local(&self, name: &SmolStr) -> Option<LocalId> {
        for scope in self.scopes.iter().rev() {
            if let Some(local) = scope.get(name) {
                return Some(*local);
            }
        }
        None
    }

    fn expr_is_result(&self, body: &hir::Body, expr_id: hir::Idx<Expr>) -> bool {
        match &body.exprs[expr_id] {
            Expr::Unary { op, .. } => matches!(op, UnaryOp::Await | UnaryOp::Err),
            Expr::Binary { .. } => false,
            Expr::Crash { .. } => false,
            Expr::Call { callee, .. } => {
                if let Expr::Variable(name) = &body.exprs[*callee] {
                    return self.result_functions.contains(name);
                }
                false
            }
            Expr::Variable(name) => self.resolve_resultness(name).unwrap_or(false),
            _ => false,
        }
    }

    fn enter_scope(&mut self) {
        self.scopes.push(HashMap::new());
        self.result_scopes.push(HashMap::new());
    }

    fn exit_scope(&mut self) {
        self.scopes.pop();
        self.result_scopes.pop();
    }

    fn lower_stmt_block(&mut self, body: &hir::Body, stmts: &[hir::Idx<HirStmt>]) {
        for stmt in stmts {
            self.lower_stmt(body, *stmt);
        }
    }

    fn lower_stmt(&mut self, body: &hir::Body, stmt_id: hir::Idx<HirStmt>) {
        let span = body.stmt_span(stmt_id);
        match &body.stmts[stmt_id] {
            HirStmt::Expr(expr) => {
                let _ = self.lower_expr(body, *expr);
            }
            HirStmt::Let {
                name,
                value,
                mutable,
                ..
            } => {
                let is_result = self.expr_is_result(body, *value);
                let value = self.lower_expr(body, *value);
                let local = self.new_local(name.clone(), *mutable, self.local_type_for_name(name));
                self.declare_local(name.clone(), local);
                self.declare_resultness(name.clone(), is_result);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Local(local),
                    value: Rvalue::Use(value),
                    span,
                });
            }
            HirStmt::Assign {
                name, op, value, ..
            } => {
                let Some(local) = self.resolve_local(name) else {
                    return;
                };
                let is_result = self.expr_is_result(body, *value);
                let rhs = self.lower_expr(body, *value);
                self.set_resultness(name, is_result);
                match op {
                    AssignOp::Assign => {
                        self.push_stmt(MirStmt::Assign {
                            place: Place::Local(local),
                            value: Rvalue::Use(rhs),
                            span,
                        });
                    }
                    AssignOp::AddAssign
                    | AssignOp::SubAssign
                    | AssignOp::MulAssign
                    | AssignOp::DivAssign => {
                        let op = match op {
                            AssignOp::AddAssign => BinaryOp::Add,
                            AssignOp::SubAssign => BinaryOp::Sub,
                            AssignOp::MulAssign => BinaryOp::Mul,
                            AssignOp::DivAssign => BinaryOp::Div,
                            AssignOp::Assign => BinaryOp::Assign,
                        };
                        let temp = self.new_temp(MirType::Unknown);
                        self.push_stmt(MirStmt::Assign {
                            place: Place::Temp(temp),
                            value: Rvalue::Binary {
                                op,
                                lhs: Value::Local(local),
                                rhs,
                            },
                            span,
                        });
                        self.push_stmt(MirStmt::Assign {
                            place: Place::Local(local),
                            value: Rvalue::Use(Value::Temp(temp)),
                            span,
                        });
                    }
                }
            }
            HirStmt::Optimize { body: optimize_body, .. } => {
                self.enter_scope();
                self.lower_stmt_block(body, optimize_body);
                self.exit_scope();
            }
            HirStmt::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let cond = self.lower_expr(body, *condition);
                let then_block = self.new_block();
                let else_block = self.new_block();
                let join_block = self.new_block();
                self.set_terminator(Terminator::Branch {
                    cond,
                    then_target: then_block,
                    else_target: else_block,
                    span,
                });

                self.current_block = then_block;
                self.enter_scope();
                self.lower_stmt_block(body, then_branch);
                self.exit_scope();
                if self.block_is_open(self.current_block) {
                    self.set_terminator(Terminator::Jump {
                        target: join_block,
                        span,
                    });
                }

                self.current_block = else_block;
                if let Some(branch) = else_branch {
                    self.enter_scope();
                    self.lower_stmt_block(body, branch);
                    self.exit_scope();
                }
                if self.block_is_open(self.current_block) {
                    self.set_terminator(Terminator::Jump {
                        target: join_block,
                        span,
                    });
                }

                self.current_block = join_block;
            }
            HirStmt::While {
                condition,
                body: loop_body,
            } => {
                let head_block = self.new_block();
                let body_block = self.new_block();
                let exit_block = self.new_block();

                self.set_terminator(Terminator::Jump {
                    target: head_block,
                    span,
                });

                self.current_block = head_block;
                let cond = self.lower_expr(body, *condition);
                self.set_terminator(Terminator::Branch {
                    cond,
                    then_target: body_block,
                    else_target: exit_block,
                    span,
                });

                self.current_block = body_block;
                self.loop_stack.push(LoopTarget {
                    break_target: exit_block,
                    continue_target: head_block,
                });
                self.enter_scope();
                self.lower_stmt_block(body, loop_body);
                self.exit_scope();
                self.loop_stack.pop();
                if self.block_is_open(self.current_block) {
                    self.set_terminator(Terminator::Jump {
                        target: head_block,
                        span,
                    });
                }

                self.current_block = exit_block;
            }
            HirStmt::For {
                name,
                iterable,
                body: loop_body,
            } => {
                let iterable_value = self.lower_expr(body, *iterable);
                let iter_temp = self.new_temp(MirType::Unknown);
                self.push_stmt(MirStmt::IterInit {
                    dst: Place::Temp(iter_temp),
                    iterable: iterable_value,
                    span,
                });

                let head_block = self.new_block();
                let body_block = self.new_block();
                let exit_block = self.new_block();

                self.set_terminator(Terminator::Jump {
                    target: head_block,
                    span,
                });

                self.current_block = head_block;
                let value_temp = self.new_temp(MirType::Unknown);
                let done_temp = self.new_temp(MirType::Bool);
                self.push_stmt(MirStmt::IterNext {
                    iter: Value::Temp(iter_temp),
                    dst_value: Place::Temp(value_temp),
                    dst_done: Place::Temp(done_temp),
                    span,
                });
                self.set_terminator(Terminator::Branch {
                    cond: Value::Temp(done_temp),
                    then_target: exit_block,
                    else_target: body_block,
                    span,
                });

                self.current_block = body_block;
                self.enter_scope();
                let local = self.new_local(name.clone(), false, MirType::Unknown);
                self.declare_local(name.clone(), local);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Local(local),
                    value: Rvalue::Use(Value::Temp(value_temp)),
                    span,
                });
                self.loop_stack.push(LoopTarget {
                    break_target: exit_block,
                    continue_target: head_block,
                });
                self.lower_stmt_block(body, loop_body);
                self.loop_stack.pop();
                self.exit_scope();
                if self.block_is_open(self.current_block) {
                    self.set_terminator(Terminator::Jump {
                        target: head_block,
                        span,
                    });
                }

                self.current_block = exit_block;
            }
            HirStmt::Match {
                subject,
                cases,
                otherwise,
            } => {
                let scrutinee = self.lower_expr(body, *subject);
                let switch_block = self.current_block;
                let join_block = self.new_block();
                let default_block = self.new_block();
                let mut switch_cases = Vec::new();

                for case in cases {
                    let case_block = self.new_block();
                    for label in &case.labels {
                        if let Some(case_label) = self.lower_case_label(body, *label) {
                            switch_cases.push((case_label, case_block));
                        }
                    }
                    self.current_block = case_block;
                    self.enter_scope();
                    self.lower_stmt_block(body, &case.body);
                    self.exit_scope();
                    if self.block_is_open(self.current_block) {
                        self.set_terminator(Terminator::Jump {
                            target: join_block,
                            span,
                        });
                    }
                }

                self.current_block = default_block;
                if let Some(branch) = otherwise {
                    self.enter_scope();
                    self.lower_stmt_block(body, branch);
                    self.exit_scope();
                }
                if self.block_is_open(self.current_block) {
                    self.set_terminator(Terminator::Jump {
                        target: join_block,
                        span,
                    });
                }

                self.current_block = switch_block;
                self.set_terminator(Terminator::Switch {
                    scrutinee,
                    cases: switch_cases,
                    default: default_block,
                    span,
                });

                self.current_block = join_block;
            }
            HirStmt::Use { .. } => {}
            HirStmt::Return(expr) => {
                let value = match expr {
                    Some(expr_id) => {
                        let raw_value = self.lower_expr(body, *expr_id);
                        if self.returns_result && !self.expr_is_result(body, *expr_id) {
                            let temp = self.new_temp(MirType::Unknown);
                            self.push_stmt(MirStmt::Assign {
                                place: Place::Temp(temp),
                                value: Rvalue::ResultOk { value: raw_value },
                                span,
                            });
                            Some(Value::Temp(temp))
                        } else {
                            Some(raw_value)
                        }
                    }
                    None => {
                        if self.returns_result {
                            let temp = self.new_temp(MirType::Unknown);
                            self.push_stmt(MirStmt::Assign {
                                place: Place::Temp(temp),
                                value: Rvalue::ResultOk {
                                    value: Value::Const(Literal::Nil),
                                },
                                span,
                            });
                            Some(Value::Temp(temp))
                        } else {
                            None
                        }
                    }
                };
                self.set_terminator(Terminator::Return { value, span });
                let next = self.new_block();
                self.current_block = next;
            }
            HirStmt::Break => {
                if let Some(target) = self.loop_stack.last() {
                    self.set_terminator(Terminator::Jump {
                        target: target.break_target,
                        span,
                    });
                    let next = self.new_block();
                    self.current_block = next;
                }
            }
            HirStmt::Continue => {
                if let Some(target) = self.loop_stack.last() {
                    self.set_terminator(Terminator::Jump {
                        target: target.continue_target,
                        span,
                    });
                    let next = self.new_block();
                    self.current_block = next;
                }
            }
        }
    }

    fn lower_case_label(
        &mut self,
        body: &hir::Body,
        expr_id: hir::Idx<Expr>,
    ) -> Option<SwitchCase> {
        match &body.exprs[expr_id] {
            Expr::Literal(lit) => Some(SwitchCase::Literal(lit.clone())),
            Expr::Variable(name) => self.type_tags.get(name).copied().map(SwitchCase::Type),
            _ => None,
        }
    }

    fn lower_expr(&mut self, body: &hir::Body, expr_id: hir::Idx<Expr>) -> Value {
        let span = body.expr_span(expr_id);
        match &body.exprs[expr_id] {
            Expr::Literal(lit) => Value::Const(lit.clone()),
            Expr::Variable(name) => self
                .resolve_local(name)
                .map(Value::Local)
                .unwrap_or_else(|| Value::Const(Literal::Nil)),
            Expr::Detach {
                target,
                size,
                objective,
            } => self.lower_detach_expr(body, *target, *size, *objective, expr_id, span),
            Expr::Unary { op, expr, .. } => {
                if matches!(op, UnaryOp::Await) {
                    self.suspendable = true;
                }
                match op {
                    UnaryOp::Await => self.lower_await(body, *expr, span),
                    UnaryOp::Fire => {
                        let pending = self.lower_pending_call_or_value(body, *expr, span);
                        self.push_stmt(MirStmt::Fire { pending, span });
                        Value::Const(Literal::Nil)
                    }
                    UnaryOp::Spawn => self.lower_detach_expr(
                        body,
                        *expr,
                        hir::PoolSize::Fixed(1),
                        None,
                        expr_id,
                        span,
                    ),
                    UnaryOp::Err => {
                        let operand = self.lower_expr(body, *expr);
                        let temp = self.new_temp_for_expr(expr_id);
                        self.push_stmt(MirStmt::Assign {
                            place: Place::Temp(temp),
                            value: Rvalue::ResultErr { value: operand },
                            span,
                        });
                        Value::Temp(temp)
                    }
                    _ => {
                        let operand = self.lower_expr(body, *expr);
                        let temp = self.new_temp_for_expr(expr_id);
                        self.push_stmt(MirStmt::Assign {
                            place: Place::Temp(temp),
                            value: Rvalue::Unary { op: *op, operand },
                            span,
                        });
                        Value::Temp(temp)
                    }
                }
            }
            Expr::Binary { lhs, op, rhs, .. } => {
                if matches!(op, BinaryOp::Otherwise) {
                    let result_val = self.lower_expr(body, *lhs);
                    let ok_flag = self.new_temp(MirType::Bool);
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Temp(ok_flag),
                        value: Rvalue::ResultIsOk {
                            value: result_val.clone(),
                        },
                        span,
                    });

                    let then_block = self.new_block();
                    let else_block = self.new_block();
                    let join_block = self.new_block();
                    self.set_terminator(Terminator::Branch {
                        cond: Value::Temp(ok_flag),
                        then_target: then_block,
                        else_target: else_block,
                        span,
                    });

                    let result_local = self.new_temp_local();

                    self.current_block = then_block;
                    let ok_value = self.new_temp(MirType::Unknown);
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Temp(ok_value),
                        value: Rvalue::ResultUnwrap { value: result_val },
                        span,
                    });
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Local(result_local),
                        value: Rvalue::Use(Value::Temp(ok_value)),
                        span,
                    });
                    if self.block_is_open(self.current_block) {
                        self.set_terminator(Terminator::Jump {
                            target: join_block,
                            span,
                        });
                    }

                    self.current_block = else_block;
                    let handler_value = self.lower_expr(body, *rhs);
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Local(result_local),
                        value: Rvalue::Use(handler_value),
                        span,
                    });
                    if self.block_is_open(self.current_block) {
                        self.set_terminator(Terminator::Jump {
                            target: join_block,
                            span,
                        });
                    }

                    self.current_block = join_block;
                    Value::Local(result_local)
                } else if matches!(op, BinaryOp::And | BinaryOp::Or) {
                    let lhs_val = self.lower_expr(body, *lhs);
                    let eval_block = self.new_block();
                    let short_block = self.new_block();
                    let join_block = self.new_block();

                    let (then_target, else_target) = if matches!(op, BinaryOp::And) {
                        (eval_block, short_block)
                    } else {
                        (short_block, eval_block)
                    };

                    self.set_terminator(Terminator::Branch {
                        cond: lhs_val.clone(),
                        then_target,
                        else_target,
                        span,
                    });

                    let result_local = self.new_temp_local();

                    self.current_block = short_block;
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Local(result_local),
                        value: Rvalue::Use(lhs_val),
                        span,
                    });
                    if self.block_is_open(self.current_block) {
                        self.set_terminator(Terminator::Jump {
                            target: join_block,
                            span,
                        });
                    }

                    self.current_block = eval_block;
                    let rhs_val = self.lower_expr(body, *rhs);
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Local(result_local),
                        value: Rvalue::Use(rhs_val),
                        span,
                    });
                    if self.block_is_open(self.current_block) {
                        self.set_terminator(Terminator::Jump {
                            target: join_block,
                            span,
                        });
                    }

                    self.current_block = join_block;
                    Value::Local(result_local)
                } else {
                    let lhs = self.lower_expr(body, *lhs);
                    let rhs = self.lower_expr(body, *rhs);
                    let temp = self.new_temp_for_expr(expr_id);
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Temp(temp),
                        value: Rvalue::Binary { op: *op, lhs, rhs },
                        span,
                    });
                    Value::Temp(temp)
                }
            }
            Expr::Crash { expr } => {
                let value = self.lower_expr(body, *expr);
                let temp = self.new_temp_for_expr(expr_id);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(temp),
                    value: Rvalue::Crash { value },
                    span,
                });
                Value::Temp(temp)
            }
            Expr::Member { object, member, .. } => {
                let base = self.lower_expr(body, *object);
                let temp = self.new_temp_for_expr(expr_id);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(temp),
                    value: Rvalue::GetField {
                        base,
                        field: member.clone(),
                    },
                    span,
                });
                Value::Temp(temp)
            }
            Expr::Call { callee, args } => {
                if let Expr::Variable(name) = &body.exprs[*callee] {
                    if let Some(class_id) = self.type_tags.get(name).copied() {
                        let fields = self.class_fields.get(name).cloned().unwrap_or_default();
                        let mut field_values: Vec<Option<Value>> = vec![None; fields.len()];
                        let mut positional_index = 0usize;
                        for arg in args {
                            match arg {
                                hir::Arg::Positional { value, .. } => {
                                    let lowered = self.lower_expr(body, *value);
                                    if positional_index < field_values.len() {
                                        field_values[positional_index] = Some(lowered);
                                    }
                                    positional_index += 1;
                                }
                                hir::Arg::Named { name, value, .. } => {
                                    let lowered = self.lower_expr(body, *value);
                                    if let Some(idx) = fields.iter().position(|f| f == name) {
                                        field_values[idx] = Some(lowered);
                                    }
                                }
                            }
                        }
                        let temp = self.new_temp_for_expr(expr_id);
                        self.push_stmt(MirStmt::Assign {
                            place: Place::Temp(temp),
                            value: Rvalue::ClassInit {
                                class_id: class_id.0 as u32,
                                fields,
                            },
                            span,
                        });
                        for (idx, value) in field_values.into_iter().enumerate() {
                            if let Some(value) = value {
                                self.push_stmt(MirStmt::SetField {
                                    base: Value::Temp(temp),
                                    field: self
                                        .class_fields
                                        .get(name)
                                        .and_then(|fields| fields.get(idx).cloned())
                                        .unwrap_or_default(),
                                    value,
                                    span,
                                });
                            }
                        }
                        return Value::Temp(temp);
                    }
                }
                let (target, args) = self.lower_call_target(body, *callee, args);
                let temp = self.new_temp_for_expr(expr_id);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(temp),
                    value: Rvalue::Call {
                        kind: CallKind::Sync,
                        target,
                        args,
                    },
                    span,
                });
                Value::Temp(temp)
            }
            Expr::List(items) => {
                let mut values = Vec::with_capacity(items.len());
                for item in items {
                    values.push(self.lower_expr(body, *item));
                }
                let temp = self.new_temp_for_expr(expr_id);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(temp),
                    value: Rvalue::BuildList { items: values },
                    span,
                });
                Value::Temp(temp)
            }
            Expr::Map(items) => {
                let mut values = Vec::with_capacity(items.len());
                for (key, value) in items {
                    let key_val = self.lower_expr(body, *key);
                    let value_val = self.lower_expr(body, *value);
                    values.push((key_val, value_val));
                }
                let temp = self.new_temp_for_expr(expr_id);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(temp),
                    value: Rvalue::BuildMap { items: values },
                    span,
                });
                Value::Temp(temp)
            }
            Expr::StringInterp(parts) => {
                let mut values = Vec::with_capacity(parts.len());
                for part in parts {
                    match part {
                        hir::StringPart::Literal(value) => {
                            values.push(StringPartValue::Literal(value.clone()));
                        }
                        hir::StringPart::Expr(expr) => {
                            let value = self.lower_expr(body, *expr);
                            values.push(StringPartValue::Value(value));
                        }
                    }
                }
                let temp = self.new_temp_for_expr(expr_id);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(temp),
                    value: Rvalue::StringInterp { parts: values },
                    span,
                });
                Value::Temp(temp)
            }
        }
    }

    fn lower_detach_expr(
        &mut self,
        body: &hir::Body,
        target_expr: hir::Idx<Expr>,
        size: hir::PoolSize,
        objective: Option<hir::Objective>,
        result_expr: hir::Idx<Expr>,
        span: TextRange,
    ) -> Value {
        let mut target_expr = target_expr;
        let mut size = size;
        let mut objective = objective;
        let mut config = SpawnConfig::default();
        let mut min_size = None;
        let mut max_size = None;
        let mut weight = None;
        let mut queue_cap: Option<i64> = None;
        if let Some(spec) = self.parse_pool_of(body, target_expr) {
            target_expr = spec.class_expr;
            if let Some(pool_size) = spec.size {
                size = pool_size;
            }
            if let Some(pool_objective) = spec.objective {
                objective = Some(pool_objective);
            }
            config = spec.config;
            min_size = spec.min_size;
            max_size = spec.max_size;
            weight = spec.weight;
            queue_cap = spec.queue_cap;
        }
        match size {
            hir::PoolSize::Fixed(count) => {
                if count > 1 {
                    if let Some(value) = self.lower_detach_pool_fixed(
                        body,
                        target_expr,
                        count as usize,
                        objective,
                        config,
                        min_size,
                        max_size,
                        weight,
                        queue_cap,
                        result_expr,
                        span,
                    ) {
                        return value;
                    }
                }
            }
            hir::PoolSize::Auto => {
                if let Some(value) = self.lower_detach_pool_auto(
                    body,
                    target_expr,
                    objective,
                    config,
                    min_size,
                    max_size,
                    weight,
                    queue_cap,
                    result_expr,
                    span,
                ) {
                    return value;
                }
            }
        }
        let mut target = None;
        let mut instance = None;
        let mut lowered = None;
        match &body.exprs[target_expr] {
            Expr::Variable(name) => {
                if let Some(id) = self.type_tags.get(name).copied() {
                    target = Some(Value::Const(Literal::Int(id.0 as i64)));
                    let fields = self.class_fields.get(name).cloned().unwrap_or_default();
                    let temp = self.new_temp_for_expr(target_expr);
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Temp(temp),
                        value: Rvalue::ClassInit {
                            class_id: id.0 as u32,
                            fields,
                        },
                        span,
                    });
                    instance = Some(Value::Temp(temp));
                }
            }
            Expr::Call { callee, .. } => {
                if let Expr::Variable(name) = &body.exprs[*callee] {
                    if let Some(id) = self.type_tags.get(name).copied() {
                        target = Some(Value::Const(Literal::Int(id.0 as i64)));
                    }
                }
                let value = self.lower_expr(body, target_expr);
                instance = Some(value.clone());
                lowered = Some(value);
            }
            _ => {
                let value = self.lower_expr(body, target_expr);
                lowered = Some(value.clone());
                instance = Some(value);
            }
        }
        let target = target.unwrap_or_else(|| {
            lowered
                .clone()
                .unwrap_or_else(|| self.lower_expr(body, target_expr))
        });
        let instance = instance.unwrap_or(Value::Const(Literal::Nil));
        let objective = objective.unwrap_or(hir::Objective::Balance);
        let temp = self.new_temp_for_expr(result_expr);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(temp),
            value: Rvalue::Spawn {
                target,
                instance,
                size,
                objective,
                config,
            },
            span,
        });
        Value::Temp(temp)
    }

    fn lower_detach_pool_fixed(
        &mut self,
        body: &hir::Body,
        target_expr: hir::Idx<Expr>,
        count: usize,
        objective: Option<hir::Objective>,
        config: SpawnConfig,
        min_size: Option<i64>,
        max_size: Option<i64>,
        weight: Option<i64>,
        queue_cap: Option<i64>,
        result_expr: hir::Idx<Expr>,
        span: TextRange,
    ) -> Option<Value> {
        let class = self.class_target_info(body, target_expr)?;
        let objective = objective.unwrap_or(hir::Objective::Balance);
        let mut handles = Vec::with_capacity(count);
        for _ in 0..count {
            let instance = self.build_class_instance(&class, span);
            let target = Value::Const(Literal::Int(class.class_id.0 as i64));
            let temp = self.new_temp_for_expr(result_expr);
            self.push_stmt(MirStmt::Assign {
                place: Place::Temp(temp),
                value: Rvalue::Spawn {
                    target,
                    instance,
                    size: hir::PoolSize::Fixed(1),
                    objective,
                    config,
                },
                span,
            });
            handles.push(Value::Temp(temp));
        }
        let list_temp = self.new_temp_for_expr(result_expr);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(list_temp),
            value: Rvalue::BuildList { items: handles },
            span,
        });
        let pool_temp = self.new_temp_for_expr(result_expr);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(pool_temp),
            value: Rvalue::PoolNew {
                handles: Value::Temp(list_temp),
                objective,
                min_size: min_size.unwrap_or(0),
                max_size: max_size.unwrap_or(0),
                weight: weight.unwrap_or(0),
                queue_cap: queue_cap.unwrap_or(0),
            },
            span,
        });
        Some(Value::Temp(pool_temp))
    }

    fn lower_detach_pool_auto(
        &mut self,
        body: &hir::Body,
        target_expr: hir::Idx<Expr>,
        objective: Option<hir::Objective>,
        config: SpawnConfig,
        min_size: Option<i64>,
        max_size: Option<i64>,
        weight: Option<i64>,
        queue_cap: Option<i64>,
        result_expr: hir::Idx<Expr>,
        span: TextRange,
    ) -> Option<Value> {
        let class = self.class_target_info(body, target_expr)?;
        let objective = objective.unwrap_or(hir::Objective::Balance);
        let obj_code = objective_code(objective);

        let size_temp = self.new_temp(MirType::Int);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(size_temp),
            value: Rvalue::Call {
                kind: CallKind::Sync,
                target: CallTarget::Function(SmolStr::new("pool_auto_size")),
                args: vec![
                    Value::Const(Literal::Int(obj_code)),
                    Value::Const(Literal::Int(min_size.unwrap_or(0))),
                    Value::Const(Literal::Int(max_size.unwrap_or(0))),
                    Value::Const(Literal::Int(weight.unwrap_or(0))),
                ],
            },
            span,
        });

        let list_temp = self.new_temp(MirType::Unknown);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(list_temp),
            value: Rvalue::BuildList { items: Vec::new() },
            span,
        });

        let idx_local = self.new_temp_local();
        self.push_stmt(MirStmt::Assign {
            place: Place::Local(idx_local),
            value: Rvalue::Use(Value::Const(Literal::Int(0))),
            span,
        });

        let head_block = self.new_block();
        let body_block = self.new_block();
        let exit_block = self.new_block();

        self.set_terminator(Terminator::Jump {
            target: head_block,
            span,
        });

        self.current_block = head_block;
        let cond_temp = self.new_temp(MirType::Bool);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(cond_temp),
            value: Rvalue::Binary {
                op: BinaryOp::Lt,
                lhs: Value::Local(idx_local),
                rhs: Value::Temp(size_temp),
            },
            span,
        });
        self.set_terminator(Terminator::Branch {
            cond: Value::Temp(cond_temp),
            then_target: body_block,
            else_target: exit_block,
            span,
        });

        self.current_block = body_block;
        let instance = self.build_class_instance(&class, span);
        let target = Value::Const(Literal::Int(class.class_id.0 as i64));
        let handle_temp = self.new_temp_for_expr(result_expr);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(handle_temp),
            value: Rvalue::Spawn {
                target,
                instance,
                size: hir::PoolSize::Fixed(1),
                objective,
                config,
            },
            span,
        });
        let push_temp = self.new_temp(MirType::Unknown);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(push_temp),
            value: Rvalue::Call {
                kind: CallKind::Sync,
                target: CallTarget::Function(SmolStr::new("list_push")),
                args: vec![Value::Temp(list_temp), Value::Temp(handle_temp)],
            },
            span,
        });

        let next_temp = self.new_temp(MirType::Int);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(next_temp),
            value: Rvalue::Binary {
                op: BinaryOp::Add,
                lhs: Value::Local(idx_local),
                rhs: Value::Const(Literal::Int(1)),
            },
            span,
        });
        self.push_stmt(MirStmt::Assign {
            place: Place::Local(idx_local),
            value: Rvalue::Use(Value::Temp(next_temp)),
            span,
        });
        if self.block_is_open(self.current_block) {
            self.set_terminator(Terminator::Jump {
                target: head_block,
                span,
            });
        }

        self.current_block = exit_block;
        let pool_temp = self.new_temp_for_expr(result_expr);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(pool_temp),
            value: Rvalue::PoolNew {
                handles: Value::Temp(list_temp),
                objective,
                min_size: min_size.unwrap_or(0),
                max_size: max_size.unwrap_or(0),
                weight: weight.unwrap_or(0),
                queue_cap: queue_cap.unwrap_or(0),
            },
            span,
        });
        Some(Value::Temp(pool_temp))
    }

    fn class_target_info(
        &mut self,
        body: &hir::Body,
        target_expr: hir::Idx<Expr>,
    ) -> Option<ClassTargetInfo> {
        match &body.exprs[target_expr] {
            Expr::Variable(name) => {
                let class_id = self.type_tags.get(name).copied()?;
                let fields = self.class_fields.get(name).cloned().unwrap_or_default();
                Some(ClassTargetInfo {
                    class_id,
                    fields,
                    field_values: Vec::new(),
                })
            }
            Expr::Call { callee, args } => {
                let Expr::Variable(name) = &body.exprs[*callee] else {
                    return None;
                };
                let class_id = self.type_tags.get(name).copied()?;
                let fields = self.class_fields.get(name).cloned().unwrap_or_default();
                let mut field_values: Vec<Option<Value>> = vec![None; fields.len()];
                let mut positional_index = 0usize;
                for arg in args {
                    match arg {
                        hir::Arg::Positional { value, .. } => {
                            let lowered = self.lower_expr(body, *value);
                            if positional_index < field_values.len() {
                                field_values[positional_index] = Some(lowered);
                            }
                            positional_index += 1;
                        }
                        hir::Arg::Named { name, value, .. } => {
                            let lowered = self.lower_expr(body, *value);
                            if let Some(idx) = fields.iter().position(|f| f == name) {
                                field_values[idx] = Some(lowered);
                            }
                        }
                    }
                }
                Some(ClassTargetInfo {
                    class_id,
                    fields,
                    field_values,
                })
            }
            _ => None,
        }
    }

    fn build_class_instance(&mut self, class: &ClassTargetInfo, span: TextRange) -> Value {
        let temp = self.new_temp(MirType::Unknown);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(temp),
            value: Rvalue::ClassInit {
                class_id: class.class_id.0 as u32,
                fields: class.fields.clone(),
            },
            span,
        });
        for (idx, value) in class.field_values.iter().enumerate() {
            if let Some(value) = value {
                self.push_stmt(MirStmt::SetField {
                    base: Value::Temp(temp),
                    field: class
                        .fields
                        .get(idx)
                        .cloned()
                        .unwrap_or_default(),
                    value: value.clone(),
                    span,
                });
            }
        }
        Value::Temp(temp)
    }

    fn parse_pool_of(
        &self,
        body: &hir::Body,
        expr_id: hir::Idx<Expr>,
    ) -> Option<PoolOfSpec> {
        let Expr::Call { callee, args } = &body.exprs[expr_id] else {
            return None;
        };
        let Expr::Member { object, member, .. } = &body.exprs[*callee] else {
            return None;
        };
        if member.as_str() != "of" {
            return None;
        }
        if !matches!(&body.exprs[*object], Expr::Variable(name) if name.as_str() == "Pool") {
            return None;
        }
        let mut class_expr = None;
        let mut size = None;
        let mut objective = None;
        let mut config = SpawnConfig::default();
        let mut min_size = None;
        let mut max_size = None;
        let mut weight = None;
        let mut queue_cap: Option<i64> = None;
        for arg in args {
            match arg {
                hir::Arg::Positional { value, .. } => {
                    if class_expr.is_none() {
                        class_expr = Some(*value);
                    }
                }
                hir::Arg::Named { name, value, .. } => match name.as_str() {
                    "size" => {
                        if let Some(pool_size) = pool_size_from_expr(body, *value) {
                            size = Some(pool_size);
                        }
                    }
                    "objective" => {
                        if let Some(obj) = objective_from_expr(body, *value) {
                            objective = Some(obj);
                        }
                    }
                    "min" => {
                        min_size = int_literal_from_expr(body, *value);
                    }
                    "max" => {
                        max_size = int_literal_from_expr(body, *value);
                    }
                    "weight" => {
                        weight = int_literal_from_expr(body, *value);
                    }
                    "batch" => {
                        if let Some(limit) = batch_limit_from_expr(body, *value) {
                            config.batch_limit = Some(limit);
                        }
                    }
                    "backpressure" => {
                        if let Some(bp) = backpressure_from_expr(body, *value) {
                            config.mailbox_cap = bp.mailbox_cap;
                            config.enqueue_timeout_ms = bp.enqueue_timeout_ms;
                            queue_cap = bp.queue_cap;
                        }
                    }
                    _ => {}
                },
            }
        }
        class_expr.map(|expr| PoolOfSpec {
            class_expr: expr,
            size,
            objective,
            config,
            min_size,
            max_size,
            weight,
            queue_cap,
        })
    }

    fn lower_await(&mut self, body: &hir::Body, expr_id: hir::Idx<Expr>, span: TextRange) -> Value {
        let pending = self.lower_pending_call_or_value(body, expr_id, span);
        let temp = self.new_temp_for_expr(expr_id);
        self.push_stmt(MirStmt::Await {
            dst: Place::Temp(temp),
            pending,
            span,
        });
        Value::Temp(temp)
    }

    fn lower_pending_call_or_value(
        &mut self,
        body: &hir::Body,
        expr_id: hir::Idx<Expr>,
        span: TextRange,
    ) -> Value {
        if let Expr::Call { callee, args } = &body.exprs[expr_id] {
            let kind = if self.is_actor_call(body, *callee) {
                CallKind::Actor
            } else {
                CallKind::Sync
            };
            let (target, args) = self.lower_call_target(body, *callee, args);
            let temp = self.new_temp_for_expr(expr_id);
            self.push_stmt(MirStmt::Assign {
                place: Place::Temp(temp),
                value: Rvalue::Call { kind, target, args },
                span,
            });
            Value::Temp(temp)
        } else {
            self.lower_expr(body, expr_id)
        }
    }

    fn is_actor_call(&self, body: &hir::Body, callee: hir::Idx<Expr>) -> bool {
        if let Expr::Member { object, .. } = &body.exprs[callee] {
            matches!(self.expr_type(*object), MirType::Actor(_))
        } else {
            false
        }
    }

    fn lower_call_target(
        &mut self,
        body: &hir::Body,
        callee: hir::Idx<Expr>,
        args: &[hir::Arg],
    ) -> (CallTarget, Vec<Value>) {
        let mut values = Vec::with_capacity(args.len());
        for arg in args {
            match arg {
                hir::Arg::Positional { value, .. } => {
                    values.push(self.lower_expr(body, *value));
                }
                hir::Arg::Named { value, .. } => {
                    values.push(self.lower_expr(body, *value));
                }
            }
        }

        match &body.exprs[callee] {
            Expr::Member { object, member, .. } => {
                let receiver = self.lower_expr(body, *object);
                let method_id = match self.expr_type(*object) {
                    MirType::Actor(inner) => {
                        if let MirType::Named(class_name) = *inner {
                            self.method_id_for(&class_name, member)
                        } else {
                            None
                        }
                    }
                    MirType::Named(class_name) => self.method_id_for(&class_name, member),
                    _ => None,
                };
                (
                    CallTarget::Method {
                        receiver,
                        method: member.clone(),
                        method_id,
                    },
                    values,
                )
            }
            Expr::Variable(name) if self.function_names.contains(name) => {
                (CallTarget::Function(name.clone()), values)
            }
            _ => {
                let callee_value = self.lower_expr(body, callee);
                (CallTarget::Indirect(callee_value), values)
            }
        }
    }

    fn method_id_for(&self, class_name: &SmolStr, method: &SmolStr) -> Option<u32> {
        self.class_method_ids
            .get(class_name)
            .and_then(|methods| methods.get(method).copied())
    }
}

struct PoolOfSpec {
    class_expr: hir::Idx<Expr>,
    size: Option<hir::PoolSize>,
    objective: Option<hir::Objective>,
    config: SpawnConfig,
    min_size: Option<i64>,
    max_size: Option<i64>,
    weight: Option<i64>,
    queue_cap: Option<i64>,
}

struct ClassTargetInfo {
    class_id: TypeTagId,
    fields: Vec<SmolStr>,
    field_values: Vec<Option<Value>>,
}

fn pool_size_from_expr(body: &hir::Body, expr_id: hir::Idx<Expr>) -> Option<hir::PoolSize> {
    match &body.exprs[expr_id] {
        Expr::Literal(hir::Literal::Int(value)) => Some(hir::PoolSize::Fixed(*value)),
        Expr::Variable(name) if name.as_str() == "n" => Some(hir::PoolSize::Auto),
        _ => None,
    }
}

fn objective_from_expr(body: &hir::Body, expr_id: hir::Idx<Expr>) -> Option<hir::Objective> {
    match &body.exprs[expr_id] {
        Expr::Variable(name) => hir::Objective::from_str(name.as_str()),
        _ => None,
    }
}

fn int_literal_from_expr(body: &hir::Body, expr_id: hir::Idx<Expr>) -> Option<i64> {
    match &body.exprs[expr_id] {
        Expr::Literal(hir::Literal::Int(value)) => Some(*value),
        _ => None,
    }
}

struct BackpressureSpec {
    mailbox_cap: Option<i64>,
    enqueue_timeout_ms: Option<i64>,
    queue_cap: Option<i64>,
}

fn batch_limit_from_expr(body: &hir::Body, expr_id: hir::Idx<Expr>) -> Option<i64> {
    int_literal_from_expr(body, expr_id)
}

fn backpressure_from_expr(body: &hir::Body, expr_id: hir::Idx<Expr>) -> Option<BackpressureSpec> {
    match &body.exprs[expr_id] {
        Expr::Variable(name) if name.as_str() == "drop" => Some(BackpressureSpec {
            mailbox_cap: None,
            enqueue_timeout_ms: Some(0),
            queue_cap: Some(0),
        }),
        Expr::Call { callee, args } => {
            let Expr::Variable(name) = &body.exprs[*callee] else {
                return None;
            };
            if name.as_str() != "queue" || args.len() != 1 {
                return None;
            }
            let arg = match &args[0] {
                hir::Arg::Positional { value, .. } => *value,
                hir::Arg::Named { value, .. } => *value,
            };
            match &body.exprs[arg] {
                Expr::Literal(hir::Literal::Int(value)) => Some(BackpressureSpec {
                    mailbox_cap: Some(*value),
                    enqueue_timeout_ms: None,
                    queue_cap: Some(*value),
                }),
                _ => None,
            }
        }
        _ => None,
    }
}

fn objective_code(objective: hir::Objective) -> i64 {
    match objective {
        hir::Objective::Latency => 0,
        hir::Objective::Throughput => 1,
        hir::Objective::Conservation => 2,
        hir::Objective::Balance => 3,
    }
}

fn mir_type_from_type(ty: &Type) -> MirType {
    match ty {
        Type::Unknown => MirType::Unknown,
        Type::Int => MirType::Int,
        Type::Float => MirType::Float,
        Type::Bool => MirType::Bool,
        Type::String => MirType::String,
        Type::Nil => MirType::Nil,
        Type::Named(name) => MirType::Named(name.clone()),
        Type::Result(ok, err) => MirType::Result(
            Box::new(mir_type_from_type(ok)),
            Box::new(mir_type_from_type(err)),
        ),
        Type::Actor(inner) => MirType::Actor(Box::new(mir_type_from_type(inner))),
        Type::Pending(inner) => MirType::Pending(Box::new(mir_type_from_type(inner))),
        _ => MirType::Unknown,
    }
}

fn builtin_function_names() -> Vec<SmolStr> {
    vec![
        SmolStr::new("print"),
        SmolStr::new("parse_int"),
        SmolStr::new("parse_float"),
        SmolStr::new("read_file"),
        SmolStr::new("write_file"),
        SmolStr::new("list_push"),
        SmolStr::new("pool_auto_size"),
        SmolStr::new("pool_size"),
        SmolStr::new("pool_rr"),
        SmolStr::new("pool_queue_len"),
        SmolStr::new("actor_mailbox_len"),
        SmolStr::new("actor_pause"),
        SmolStr::new("actor_resume"),
        SmolStr::new("actor_pause_wait"),
        SmolStr::new("metrics_get"),
        SmolStr::new("metrics_dropped_paused_id"),
        SmolStr::new("metrics_messages_dropped_id"),
        SmolStr::new("clock_ns"),
        SmolStr::new("sleep_ms"),
        SmolStr::new("storage_get"),
        SmolStr::new("storage_set"),
        SmolStr::new("storage_delete"),
    ]
}
