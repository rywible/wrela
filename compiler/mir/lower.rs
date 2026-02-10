use crate::hir::{
    self, AssignOp, BinaryOp, Expr, FunctionKind, FunctionTypeInfo, Literal, Module,
    Stmt as HirStmt, Type, TypeInfo, UnaryOp,
};
use crate::mir::ir::Stmt as MirStmt;
use crate::mir::ir::*;
use rowan::TextRange;
use smol_str::SmolStr;
use std::collections::{HashMap, HashSet};
use std::env;

pub fn lower_module(module: &Module) -> MirModule {
    lower_module_with_types(module, None)
}

pub fn lower_module_with_types(module: &Module, type_info: Option<&TypeInfo>) -> MirModule {
    const CLASS_ID_BASE: usize = 100;
    let mut type_tags = Vec::new();
    let mut tag_map = HashMap::new();
    let mut class_fields = HashMap::new();
    let mut class_field_defaults = HashMap::new();
    let mut classes = Vec::new();
    let mut class_method_ids = HashMap::new();
    let mut class_derived = HashMap::new();
    let mut interface_methods: HashMap<SmolStr, HashSet<SmolStr>> = HashMap::new();
    let mut interface_impls: HashMap<SmolStr, Vec<SmolStr>> = HashMap::new();
    let mut method_ids = HashSet::new();
    let mut method_qnames: HashMap<hir::Idx<hir::Function>, SmolStr> = HashMap::new();
    for (_idx, class) in module.classes.iter() {
        let id = TypeTagId(type_tags.len() + CLASS_ID_BASE);
        type_tags.push(class.name.clone());
        tag_map.insert(class.name.clone(), id);
        let fields: Vec<SmolStr> = class
            .fields
            .iter()
            .map(|field| field.name.clone())
            .collect();
        let defaults: Vec<Option<hir::FieldDefault>> = class
            .fields
            .iter()
            .map(|field| field.default.clone())
            .collect();
        class_fields.insert(class.name.clone(), fields);
        class_field_defaults.insert(class.name.clone(), defaults);
        let mut methods = Vec::new();
        let mut method_map = HashMap::new();
        let mut derived = HashSet::new();
        for (idx, method_id) in class.methods.iter().enumerate() {
            let method = &module.functions[*method_id];
            method_ids.insert(method_id.into_raw());
            method_map.insert(method.name.clone(), idx as u32);
            if method.kind == FunctionKind::Derived {
                derived.insert(method.name.clone());
            }
            let qname = SmolStr::new(format!("{}.{}", class.name, method.name));
            method_qnames.insert(*method_id, qname.clone());
            methods.push(MirMethodInfo {
                name: method.name.clone(),
                func: qname,
                arity: method.params.len() + 1,
                id: idx as u32,
            });
        }
        class_method_ids.insert(class.name.clone(), method_map);
        class_derived.insert(class.name.clone(), derived);
        classes.push(MirClassInfo {
            name: class.name.clone(),
            id,
            fields: class_fields.get(&class.name).cloned().unwrap_or_default(),
            methods,
        });
    }

    for (_idx, interface) in module.interfaces.iter() {
        let method_set = interface_methods.entry(interface.name.clone()).or_default();
        for method in &interface.methods {
            method_set.insert(method.name.clone());
        }
    }
    for (_idx, class) in module.classes.iter() {
        for iface in &class.implements {
            interface_impls
                .entry(iface.clone())
                .or_default()
                .push(class.name.clone());
        }
    }

    for (_idx, en) in module.enums.iter() {
        for variant in &en.variants {
            let name = SmolStr::new(format!("{}.{}", en.name, variant.name));
            let id = TypeTagId(type_tags.len() + CLASS_ID_BASE);
            type_tags.push(name.clone());
            tag_map.insert(name.clone(), id);
            let fields: Vec<SmolStr> = variant
                .params
                .iter()
                .map(|param| param.name.clone())
                .collect();
            class_fields.insert(name.clone(), fields.clone());
            class_field_defaults.insert(name.clone(), vec![None; fields.len()]);
            classes.push(MirClassInfo {
                name: name.clone(),
                id,
                fields,
                methods: Vec::new(),
            });
        }
    }

    let mut functions = Vec::new();
    let mut function_names: HashSet<SmolStr> = module
        .functions
        .iter()
        .filter_map(|(idx, func)| {
            if method_ids.contains(&idx.into_raw()) {
                None
            } else {
                Some(func.name.clone())
            }
        })
        .collect();
    for qname in method_qnames.values() {
        function_names.insert(qname.clone());
    }
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
        let name = if is_method {
            method_qnames
                .get(&_idx)
                .cloned()
                .unwrap_or_else(|| func.name.clone())
        } else {
            func.name.clone()
        };
        functions.push(lower_function(
            func,
            name,
            body,
            &tag_map,
            &class_fields,
            &class_field_defaults,
            &function_names,
            &result_functions,
            &class_method_ids,
            &class_derived,
            &interface_methods,
            is_method,
            fn_types,
        ));
    }

    let dispatch_functions = build_interface_dispatch_functions(module, &interface_impls, &tag_map);
    for func in dispatch_functions {
        functions.push(func);
    }
    MirModule {
        functions,
        type_tags,
        classes,
    }
}

fn lower_function(
    func: &hir::Function,
    name: SmolStr,
    body: &hir::Body,
    type_tags: &HashMap<SmolStr, TypeTagId>,
    class_fields: &HashMap<SmolStr, Vec<SmolStr>>,
    class_field_defaults: &HashMap<SmolStr, Vec<Option<hir::FieldDefault>>>,
    function_names: &HashSet<SmolStr>,
    result_functions: &HashSet<SmolStr>,
    class_method_ids: &HashMap<SmolStr, HashMap<SmolStr, u32>>,
    class_derived: &HashMap<SmolStr, HashSet<SmolStr>>,
    interface_methods: &HashMap<SmolStr, HashSet<SmolStr>>,
    is_method: bool,
    type_info: Option<&FunctionTypeInfo>,
) -> MirFunction {
    let mut lowerer = FunctionLowerer::new(
        name,
        type_tags,
        class_fields,
        class_field_defaults,
        function_names,
        result_functions,
        class_method_ids,
        class_derived,
        interface_methods,
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
        lowerer.declare_local(SmolStr::new("its"), local);
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
    use crate::hir::typeck;
    use crate::parser::ast;
    use crate::parser::ast::AstNode;
    use crate::parser::parse;

    #[test]
    fn test_lower_marks_suspendable() {
        let input = "\
A Whale:\n    can swim() -> Boolean:\n        return true\n\nto f() -> Result[Boolean]:\n    w = detach Whale() * 1\n    return await w.swim()\n";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = hir_lower::lower(root);
        let mir = lower_module(&module);
        let func = mir.functions.iter().find(|f| f.name == "f").unwrap();
        assert!(func.suspendable);
    }

    #[test]
    fn test_lower_if_creates_blocks() {
        let input =
            "to f() -> Nothing:\n    if true:\n        x = 1\n    otherwise:\n        x = 2\n";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = hir_lower::lower(root);
        let mir = lower_module(&module);
        let func = mir.functions.iter().find(|f| f.name == "f").unwrap();
        assert!(func.blocks.len() >= 3);
    }

    #[test]
    fn test_lower_member_assign_sets_field() {
        let input = "\
A Counter:
    has:
        value: Integer
    can add(delta: Integer) -> Nothing:
        its.value += delta
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = hir_lower::lower(root);
        let (_type_errors, type_info) = typeck::check_module_with_info(&module);
        let mir_module = lower_module_with_types(&module, Some(&type_info));
        let func = mir_module
            .functions
            .iter()
            .find(|func| func.name == "Counter.add")
            .expect("missing Counter.add");
        let has_set_field = func.blocks.iter().any(|block| {
            block.stmts.iter().any(
                |stmt| matches!(stmt, MirStmt::SetField { field, .. } if field.as_str() == "value"),
            )
        });
        assert!(has_set_field, "expected SetField for member assign");
    }

    #[test]
    fn test_lower_field_defaults_emits_set_fields() {
        let input = "\
A Foo:
    has:
        x: Integer = 1
        y: List = [1, 2]
        z: Map = {\"a\": 1}

to run() -> Nothing:
    a = Foo()
    b = Foo(x=5)
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = hir_lower::lower(root);
        let mir_module = lower_module(&module);
        let func = mir_module
            .functions
            .iter()
            .find(|func| func.name == "run")
            .expect("missing run");

        let mut set_x = 0usize;
        let mut set_y = 0usize;
        let mut set_z = 0usize;
        let mut build_list = 0usize;
        let mut build_map = 0usize;

        for block in &func.blocks {
            for stmt in &block.stmts {
                match stmt {
                    MirStmt::SetField { field, .. } if field.as_str() == "x" => set_x += 1,
                    MirStmt::SetField { field, .. } if field.as_str() == "y" => set_y += 1,
                    MirStmt::SetField { field, .. } if field.as_str() == "z" => set_z += 1,
                    _ => {}
                }
                if let MirStmt::Assign { value, .. } = stmt {
                    match value {
                        Rvalue::BuildList { .. } => build_list += 1,
                        Rvalue::BuildMap { .. } => build_map += 1,
                        _ => {}
                    }
                }
            }
        }

        assert_eq!(set_x, 2, "expected default and override for x");
        assert_eq!(set_y, 2, "expected defaults for y in both instances");
        assert_eq!(set_z, 2, "expected defaults for z in both instances");
        assert!(build_list >= 1, "expected BuildList for default list");
        assert!(build_map >= 1, "expected BuildMap for default map");
    }

    #[test]
    fn test_lower_integer_range_for_uses_typed_induction_fast_path() {
        let input = "\
to run() -> Integer:
    start = 1
    stop = 4
    mutable total = 0
    for i in start...stop:
        total += i
    return total
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = hir_lower::lower(root);
        let (_type_errors, type_info) = typeck::check_module_with_info(&module);
        let mir_module = lower_module_with_types(&module, Some(&type_info));
        let func = mir_module
            .functions
            .iter()
            .find(|func| func.name == "run")
            .expect("missing run");

        assert!(
            func.locals
                .iter()
                .any(|local| local.name.as_str() == "i" && local.ty == MirType::Integer),
            "expected typed loop variable for integer range",
        );
        assert!(
            func.locals
                .iter()
                .any(|local| local.name.starts_with("$range_idx") && local.ty == MirType::Integer),
            "expected typed integer induction local",
        );
        assert!(
            func.locals
                .iter()
                .any(|local| local.name.starts_with("$range_step") && local.ty == MirType::Integer),
            "expected typed integer step local",
        );

        for block in &func.blocks {
            for stmt in &block.stmts {
                assert!(
                    !matches!(stmt, MirStmt::IterInit { .. } | MirStmt::IterNext { .. }),
                    "typed integer range loop should not use iterator protocol",
                );
                if let MirStmt::Assign { value, .. } = stmt {
                    assert!(
                        !matches!(
                            value,
                            Rvalue::Binary {
                                op: crate::hir::BinaryOp::Range,
                                ..
                            }
                        ),
                        "typed integer range loop should not materialize range object",
                    );
                }
            }
        }
    }

    #[test]
    fn test_lower_member_field_ops_emit_slot_hints() {
        let input = "\
A Counter:
    has:
        mutable value: Integer
        mutable other: Integer

    can bump() -> Nothing:
        its.value += 1
        its.other = 4

to run() -> Integer:
    c = Counter(value=1, other=2)
    c.value += 3
    return c.other
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = hir_lower::lower(root);
        let (_type_errors, type_info) = typeck::check_module_with_info(&module);
        let mir_module = lower_module_with_types(&module, Some(&type_info));

        let mut saw_get_value_slot = false;
        let mut saw_get_other_slot = false;
        let mut saw_set_value_slot = false;
        let mut saw_set_other_slot = false;

        for func in &mir_module.functions {
            for block in &func.blocks {
                for stmt in &block.stmts {
                    match stmt {
                        MirStmt::Assign {
                            value:
                                Rvalue::GetField {
                                    field,
                                    slot: Some(slot),
                                    ..
                                },
                            ..
                        } if field.as_str() == "value" && *slot == 0 => saw_get_value_slot = true,
                        MirStmt::Assign {
                            value:
                                Rvalue::GetField {
                                    field,
                                    slot: Some(slot),
                                    ..
                                },
                            ..
                        } if field.as_str() == "other" && *slot == 1 => saw_get_other_slot = true,
                        MirStmt::SetField {
                            field,
                            slot: Some(slot),
                            ..
                        } if field.as_str() == "value" && *slot == 0 => saw_set_value_slot = true,
                        MirStmt::SetField {
                            field,
                            slot: Some(slot),
                            ..
                        } if field.as_str() == "other" && *slot == 1 => saw_set_other_slot = true,
                        _ => {}
                    }
                }
            }
        }

        assert!(saw_get_value_slot, "expected slot-hinted get for value");
        assert!(saw_get_other_slot, "expected slot-hinted get for other");
        assert!(saw_set_value_slot, "expected slot-hinted set for value");
        assert!(saw_set_other_slot, "expected slot-hinted set for other");
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
    class_field_defaults: HashMap<SmolStr, Vec<Option<hir::FieldDefault>>>,
    class_method_ids: HashMap<SmolStr, HashMap<SmolStr, u32>>,
    class_derived: HashMap<SmolStr, HashSet<SmolStr>>,
    interface_methods: HashMap<SmolStr, HashSet<SmolStr>>,
    function_names: HashSet<SmolStr>,
    result_functions: HashSet<SmolStr>,
    returns_result: bool,
    type_info: Option<FunctionTypeInfo>,
    defers: Vec<hir::Idx<hir::Expr>>,
}

impl FunctionLowerer {
    fn new(
        name: SmolStr,
        type_tags: &HashMap<SmolStr, TypeTagId>,
        class_fields: &HashMap<SmolStr, Vec<SmolStr>>,
        class_field_defaults: &HashMap<SmolStr, Vec<Option<hir::FieldDefault>>>,
        function_names: &HashSet<SmolStr>,
        result_functions: &HashSet<SmolStr>,
        class_method_ids: &HashMap<SmolStr, HashMap<SmolStr, u32>>,
        class_derived: &HashMap<SmolStr, HashSet<SmolStr>>,
        interface_methods: &HashMap<SmolStr, HashSet<SmolStr>>,
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
            class_field_defaults: class_field_defaults.clone(),
            class_method_ids: class_method_ids.clone(),
            class_derived: class_derived.clone(),
            interface_methods: interface_methods.clone(),
            function_names: function_names.clone(),
            result_functions: result_functions.clone(),
            returns_result,
            type_info: type_info.cloned(),
            defers: Vec::new(),
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

    fn proven_range_induction_type(lhs_ty: &MirType, rhs_ty: &MirType) -> Option<MirType> {
        match (lhs_ty, rhs_ty) {
            (MirType::Integer, MirType::Integer) => Some(MirType::Integer),
            (MirType::Float, MirType::Float) => Some(MirType::Float),
            _ => None,
        }
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
            Expr::Call { callee, .. } | Expr::GivenCall { callee, .. } => {
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
            if !self.block_is_open(self.current_block) {
                break;
            }
            self.lower_stmt(body, *stmt);
        }
    }

    fn lower_stmt(&mut self, body: &hir::Body, stmt_id: hir::Idx<HirStmt>) {
        let span = body.stmt_span(stmt_id);
        match &body.stmts[stmt_id] {
            HirStmt::Expr(expr) => {
                let _ = self.lower_expr(body, *expr);
            }
            HirStmt::Assert { kind, expr } => {
                let cond = self.lower_assert_expr(body, *expr, *kind);
                let func = SmolStr::new("assert");
                let args = vec![cond, Value::Const(Literal::Nil)];
                let temp = self.new_temp(MirType::Nil);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(temp),
                    value: Rvalue::Call {
                        kind: CallKind::Sync,
                        target: CallTarget::Function(func),
                        args,
                    },
                    span,
                });
            }
            HirStmt::Require { condition, message } => {
                let cond = self.lower_expr(body, *condition);
                let msg = self.lower_expr(body, *message);
                let func = SmolStr::new("assert");
                let args = vec![cond, msg];
                let temp = self.new_temp(MirType::Nil);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(temp),
                    value: Rvalue::Call {
                        kind: CallKind::Sync,
                        target: CallTarget::Function(func),
                        args,
                    },
                    span,
                });
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
            HirStmt::Capture { name, value } => {
                let value = self.lower_expr(body, *value);
                let local = self.new_local(name.clone(), false, self.local_type_for_name(name));
                self.declare_local(name.clone(), local);
                self.declare_resultness(name.clone(), true);
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
            HirStmt::IgnoreResult { expr } => {
                let _ = self.lower_expr(body, *expr);
            }
            HirStmt::Optimize {
                body: optimize_body,
                ..
            } => {
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
                if let Expr::Binary {
                    lhs,
                    op: BinaryOp::Range,
                    rhs,
                    ..
                } = &body.exprs[*iterable]
                {
                    if self.lower_range_for(body, name, *lhs, *rhs, loop_body, span) {
                        return;
                    }
                }

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
                let done_temp = self.new_temp(MirType::Boolean);
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
                if self.match_has_result_patterns(cases) {
                    self.lower_result_match(body, span, scrutinee, cases, otherwise);
                } else {
                    let switch_block = self.current_block;
                    let join_block = self.new_block();
                    let default_block = self.new_block();
                    let default_case_idx = cases.iter().position(|case| {
                        case.labels
                            .iter()
                            .any(|label| self.is_default_match_pattern(label))
                    });
                    let mut switch_cases = Vec::new();

                    for (idx, case) in cases.iter().enumerate() {
                        if Some(idx) == default_case_idx {
                            continue;
                        }
                        let case_block = self.new_block();
                        for label in &case.labels {
                            if let Some(case_label) = self.lower_case_label(label) {
                                switch_cases.push((case_label, case_block));
                            }
                        }
                        self.current_block = case_block;
                        self.enter_scope();
                        if let Some(label) = case.labels.first() {
                            self.bind_pattern(body, label, scrutinee.clone(), span);
                        }
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
                    if let Some(idx) = default_case_idx {
                        let case = &cases[idx];
                        self.enter_scope();
                        if let Some(label) = case.labels.first() {
                            self.bind_pattern(body, label, scrutinee.clone(), span);
                        }
                        self.lower_stmt_block(body, &case.body);
                        self.exit_scope();
                    } else if let Some(branch) = otherwise {
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
            }
            HirStmt::Use { .. } => {}
            HirStmt::Defer { expr } => {
                self.defers.push(*expr);
            }
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
                self.emit_defers(body, span);
                self.set_terminator(Terminator::Return { value, span });
            }
            HirStmt::Break => {
                if let Some(target) = self.loop_stack.last() {
                    self.set_terminator(Terminator::Jump {
                        target: target.break_target,
                        span,
                    });
                }
            }
            HirStmt::Continue => {
                if let Some(target) = self.loop_stack.last() {
                    self.set_terminator(Terminator::Jump {
                        target: target.continue_target,
                        span,
                    });
                }
            }
        }
    }

    fn lower_range_for(
        &mut self,
        body: &hir::Body,
        name: &SmolStr,
        lhs: hir::Idx<Expr>,
        rhs: hir::Idx<Expr>,
        loop_body: &[hir::Idx<hir::Stmt>],
        span: TextRange,
    ) -> bool {
        if env::var_os("WRELA_DISABLE_TYPED_RANGE_FASTPATH").is_some() {
            return false;
        }
        let lhs_ty = self.expr_type(lhs);
        let rhs_ty = self.expr_type(rhs);
        let Some(induction_ty) = Self::proven_range_induction_type(&lhs_ty, &rhs_ty) else {
            return false;
        };

        let start_val = self.lower_expr(body, lhs);
        let end_val = self.lower_expr(body, rhs);
        let constant_int_bounds = match (&body.exprs[lhs], &body.exprs[rhs]) {
            (
                Expr::Literal(hir::Literal::Integer(start)),
                Expr::Literal(hir::Literal::Integer(end)),
            ) if matches!(induction_ty, MirType::Integer) => Some((*start, *end)),
            _ => None,
        };

        if let Some((start, end)) = constant_int_bounds {
            let idx_local = self.new_local(
                SmolStr::new(format!("$range_idx{}", self.locals.len())),
                true,
                induction_ty.clone(),
            );
            let loop_var = self.new_local(name.clone(), false, induction_ty.clone());
            let head_block = self.new_block();
            let body_block = self.new_block();
            let exit_block = self.new_block();
            self.push_stmt(MirStmt::Assign {
                place: Place::Local(idx_local),
                value: Rvalue::Use(start_val.clone()),
                span,
            });
            self.set_terminator(Terminator::Jump {
                target: head_block,
                span,
            });

            self.current_block = head_block;
            let cond_temp = self.new_temp(MirType::Boolean);
            let cond_op = if start <= end {
                BinaryOp::Le
            } else {
                BinaryOp::Ge
            };
            self.push_stmt(MirStmt::Assign {
                place: Place::Temp(cond_temp),
                value: Rvalue::Binary {
                    op: cond_op,
                    lhs: Value::Local(idx_local),
                    rhs: end_val.clone(),
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
            self.enter_scope();
            self.declare_local(name.clone(), loop_var);
            self.push_stmt(MirStmt::Assign {
                place: Place::Local(loop_var),
                value: Rvalue::Use(Value::Local(idx_local)),
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
                let step_temp = self.new_temp(induction_ty);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(step_temp),
                    value: Rvalue::Binary {
                        op: BinaryOp::Add,
                        lhs: Value::Local(idx_local),
                        rhs: Value::Const(Literal::Integer(if start <= end { 1 } else { -1 })),
                    },
                    span,
                });
                self.push_stmt(MirStmt::Assign {
                    place: Place::Local(idx_local),
                    value: Rvalue::Use(Value::Temp(step_temp)),
                    span,
                });
                self.set_terminator(Terminator::Jump {
                    target: head_block,
                    span,
                });
            }

            self.current_block = exit_block;
            return true;
        }

        let idx_local = self.new_local(
            SmolStr::new(format!("$range_idx{}", self.locals.len())),
            true,
            induction_ty.clone(),
        );
        let step_local = self.new_local(
            SmolStr::new(format!("$range_step{}", self.locals.len())),
            true,
            induction_ty.clone(),
        );
        let step_is_pos_local = self.new_local(
            SmolStr::new(format!("$range_pos{}", self.locals.len())),
            true,
            MirType::Boolean,
        );

        let loop_var = self.new_local(name.clone(), false, induction_ty.clone());

        let is_pos_temp = self.new_temp(MirType::Boolean);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(is_pos_temp),
            value: Rvalue::Binary {
                op: BinaryOp::Le,
                lhs: start_val.clone(),
                rhs: end_val.clone(),
            },
            span,
        });

        let asc_init = self.new_block();
        let desc_init = self.new_block();
        let head_block = self.new_block();
        let check_pos = self.new_block();
        let check_neg = self.new_block();
        let body_block = self.new_block();
        let exit_block = self.new_block();

        self.set_terminator(Terminator::Branch {
            cond: Value::Temp(is_pos_temp),
            then_target: asc_init,
            else_target: desc_init,
            span,
        });

        let step_value = if matches!(induction_ty, MirType::Float) {
            Value::Const(Literal::Float(1.0))
        } else {
            Value::Const(Literal::Integer(1))
        };
        let neg_step_value = if matches!(induction_ty, MirType::Float) {
            Value::Const(Literal::Float(-1.0))
        } else {
            Value::Const(Literal::Integer(-1))
        };

        self.current_block = asc_init;
        self.push_stmt(MirStmt::Assign {
            place: Place::Local(step_local),
            value: Rvalue::Use(step_value.clone()),
            span,
        });
        self.push_stmt(MirStmt::Assign {
            place: Place::Local(step_is_pos_local),
            value: Rvalue::Use(Value::Const(Literal::Boolean(true))),
            span,
        });
        self.push_stmt(MirStmt::Assign {
            place: Place::Local(idx_local),
            value: Rvalue::Use(start_val.clone()),
            span,
        });
        self.set_terminator(Terminator::Jump {
            target: head_block,
            span,
        });

        self.current_block = desc_init;
        self.push_stmt(MirStmt::Assign {
            place: Place::Local(step_local),
            value: Rvalue::Use(neg_step_value.clone()),
            span,
        });
        self.push_stmt(MirStmt::Assign {
            place: Place::Local(step_is_pos_local),
            value: Rvalue::Use(Value::Const(Literal::Boolean(false))),
            span,
        });
        self.push_stmt(MirStmt::Assign {
            place: Place::Local(idx_local),
            value: Rvalue::Use(start_val.clone()),
            span,
        });
        self.set_terminator(Terminator::Jump {
            target: head_block,
            span,
        });

        self.current_block = head_block;
        self.set_terminator(Terminator::Branch {
            cond: Value::Local(step_is_pos_local),
            then_target: check_pos,
            else_target: check_neg,
            span,
        });

        self.current_block = check_pos;
        let pos_cond = self.new_temp(MirType::Boolean);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(pos_cond),
            value: Rvalue::Binary {
                op: BinaryOp::Le,
                lhs: Value::Local(idx_local),
                rhs: end_val.clone(),
            },
            span,
        });
        self.set_terminator(Terminator::Branch {
            cond: Value::Temp(pos_cond),
            then_target: body_block,
            else_target: exit_block,
            span,
        });

        self.current_block = check_neg;
        let neg_cond = self.new_temp(MirType::Boolean);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(neg_cond),
            value: Rvalue::Binary {
                op: BinaryOp::Ge,
                lhs: Value::Local(idx_local),
                rhs: end_val.clone(),
            },
            span,
        });
        self.set_terminator(Terminator::Branch {
            cond: Value::Temp(neg_cond),
            then_target: body_block,
            else_target: exit_block,
            span,
        });

        self.current_block = body_block;
        self.enter_scope();
        self.declare_local(name.clone(), loop_var);
        self.push_stmt(MirStmt::Assign {
            place: Place::Local(loop_var),
            value: Rvalue::Use(Value::Local(idx_local)),
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
            let step_temp = self.new_temp(induction_ty);
            self.push_stmt(MirStmt::Assign {
                place: Place::Temp(step_temp),
                value: Rvalue::Binary {
                    op: BinaryOp::Add,
                    lhs: Value::Local(idx_local),
                    rhs: Value::Local(step_local),
                },
                span,
            });
            self.push_stmt(MirStmt::Assign {
                place: Place::Local(idx_local),
                value: Rvalue::Use(Value::Temp(step_temp)),
                span,
            });
            self.set_terminator(Terminator::Jump {
                target: head_block,
                span,
            });
        }

        self.current_block = exit_block;
        true
    }

    fn lower_assert_expr(
        &mut self,
        body: &hir::Body,
        expr_id: hir::Idx<Expr>,
        kind: hir::AssertKind,
    ) -> Value {
        let span = body.expr_span(expr_id);
        if let Expr::Binary { lhs, op, rhs, .. } = &body.exprs[expr_id] {
            if matches!(op, BinaryOp::Eq | BinaryOp::Ne) {
                let left = self.lower_expr(body, *lhs);
                let right = self.lower_expr(body, *rhs);
                let func = match kind {
                    hir::AssertKind::Value => SmolStr::new("value_deep_eq"),
                    hir::AssertKind::Identity => SmolStr::new("identity_eq"),
                };
                let temp = self.new_temp(MirType::Boolean);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(temp),
                    value: Rvalue::Call {
                        kind: CallKind::Sync,
                        target: CallTarget::Function(func),
                        args: vec![left, right],
                    },
                    span,
                });
                let mut result = Value::Temp(temp);
                if matches!(op, BinaryOp::Ne) {
                    let not_temp = self.new_temp(MirType::Boolean);
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Temp(not_temp),
                        value: Rvalue::Unary {
                            op: UnaryOp::Not,
                            operand: result,
                        },
                        span,
                    });
                    result = Value::Temp(not_temp);
                }
                return result;
            }
        }
        self.lower_expr(body, expr_id)
    }

    fn lower_case_label(&mut self, pattern: &hir::Pattern) -> Option<SwitchCase> {
        match pattern {
            hir::Pattern::Literal(lit) => Some(SwitchCase::Literal(lit.clone())),
            hir::Pattern::Binding(name) => {
                if let Some(tag) = self.type_tags.get(name).copied() {
                    return Some(SwitchCase::Type(tag));
                }
                if let Some(tag) = builtin_type_tag(name) {
                    return Some(SwitchCase::Type(tag));
                }
                None
            }
            hir::Pattern::Path { parts, args: _ } => {
                if parts.len() == 1 {
                    if let Some(tag) = self.type_tags.get(&parts[0]).copied() {
                        return Some(SwitchCase::Type(tag));
                    }
                    if let Some(tag) = builtin_type_tag(&parts[0]) {
                        return Some(SwitchCase::Type(tag));
                    }
                }
                if parts.len() == 2 {
                    let name = SmolStr::new(format!("{}.{}", parts[0], parts[1]));
                    return self.type_tags.get(&name).copied().map(SwitchCase::Type);
                }
                None
            }
            _ => None,
        }
    }

    fn match_has_result_patterns(&self, cases: &[hir::MatchCase]) -> bool {
        cases.iter().any(|case| {
            case.labels
                .iter()
                .any(|label| matches!(self.result_pattern_kind(label), Some(_)))
        })
    }

    fn result_pattern_kind(&self, pattern: &hir::Pattern) -> Option<bool> {
        if let hir::Pattern::Path { parts, .. } = pattern {
            if parts.len() == 1 && parts[0].as_str() == "Ok" {
                return Some(true);
            }
            if parts.len() == 1 && parts[0].as_str() == "Err" {
                return Some(false);
            }
        }
        None
    }

    fn lower_result_match(
        &mut self,
        body: &hir::Body,
        span: TextRange,
        scrutinee: Value,
        cases: &[hir::MatchCase],
        otherwise: &Option<Vec<hir::Idx<hir::Stmt>>>,
    ) {
        let join_block = self.new_block();
        let mut default_block = None;

        for case in cases {
            let case_block = self.new_block();
            let fallthrough_block = self.new_block();
            let label = case.labels.first();
            if let Some(label) = label {
                if let Some(is_ok) = self.result_pattern_kind(label) {
                    let is_ok_temp = self.new_temp(MirType::Boolean);
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Temp(is_ok_temp),
                        value: Rvalue::ResultIsOk {
                            value: scrutinee.clone(),
                        },
                        span,
                    });
                    let mut cond_val = Value::Temp(is_ok_temp);
                    if !is_ok {
                        let not_temp = self.new_temp(MirType::Boolean);
                        self.push_stmt(MirStmt::Assign {
                            place: Place::Temp(not_temp),
                            value: Rvalue::Unary {
                                op: UnaryOp::Not,
                                operand: cond_val,
                            },
                            span,
                        });
                        cond_val = Value::Temp(not_temp);
                    }
                    self.set_terminator(Terminator::Branch {
                        cond: cond_val,
                        then_target: case_block,
                        else_target: fallthrough_block,
                        span,
                    });
                } else {
                    default_block = Some(case_block);
                    self.set_terminator(Terminator::Jump {
                        target: case_block,
                        span,
                    });
                }
            }

            self.current_block = case_block;
            self.enter_scope();
            if let Some(label) = label {
                self.bind_result_pattern(body, label, scrutinee.clone(), span);
            }
            self.lower_stmt_block(body, &case.body);
            self.exit_scope();
            if self.block_is_open(self.current_block) {
                self.set_terminator(Terminator::Jump {
                    target: join_block,
                    span,
                });
            }

            self.current_block = fallthrough_block;
        }

        if let Some(branch) = otherwise {
            let otherwise_block = self.new_block();
            self.set_terminator(Terminator::Jump {
                target: otherwise_block,
                span,
            });
            self.current_block = otherwise_block;
            self.enter_scope();
            self.lower_stmt_block(body, branch);
            self.exit_scope();
            if self.block_is_open(self.current_block) {
                self.set_terminator(Terminator::Jump {
                    target: join_block,
                    span,
                });
            }
        } else if let Some(default_block) = default_block {
            self.set_terminator(Terminator::Jump {
                target: default_block,
                span,
            });
        }

        self.current_block = join_block;
    }

    fn bind_pattern(
        &mut self,
        body: &hir::Body,
        pattern: &hir::Pattern,
        value: Value,
        span: TextRange,
    ) {
        match pattern {
            hir::Pattern::Wildcard | hir::Pattern::Literal(_) => {}
            hir::Pattern::Binding(name) => {
                let local = self.new_local(name.clone(), false, MirType::Unknown);
                self.declare_local(name.clone(), local);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Local(local),
                    value: Rvalue::Use(value),
                    span,
                });
            }
            hir::Pattern::Path { parts, args } => {
                if parts.len() == 2 {
                    let class_name = SmolStr::new(format!("{}.{}", parts[0], parts[1]));
                    let Some(fields) = self.class_fields.get(&class_name).cloned() else {
                        return;
                    };
                    for (idx, arg) in args.iter().enumerate() {
                        if let Some(field) = fields.get(idx) {
                            let temp = self.new_temp(MirType::Unknown);
                            self.push_stmt(MirStmt::Assign {
                                place: Place::Temp(temp),
                                value: Rvalue::GetField {
                                    base: value.clone(),
                                    field: field.clone(),
                                    slot: Some(idx as u32),
                                },
                                span,
                            });
                            self.bind_pattern(body, arg, Value::Temp(temp), span);
                        }
                    }
                }
            }
        }
    }

    fn bind_result_pattern(
        &mut self,
        body: &hir::Body,
        pattern: &hir::Pattern,
        value: Value,
        span: TextRange,
    ) {
        let Some(kind) = self.result_pattern_kind(pattern) else {
            return;
        };
        if let hir::Pattern::Path { args, .. } = pattern {
            if args.is_empty() {
                return;
            }
            let temp = self.new_temp(MirType::Unknown);
            let rvalue = if kind {
                Rvalue::ResultUnwrap { value }
            } else {
                Rvalue::ResultErrUnwrap { value }
            };
            self.push_stmt(MirStmt::Assign {
                place: Place::Temp(temp),
                value: rvalue,
                span,
            });
            self.bind_pattern(body, &args[0], Value::Temp(temp), span);
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
            Expr::TypeApply { callee, .. } => self.lower_expr(body, *callee),
            Expr::Binary { lhs, op, rhs, .. } => {
                if matches!(
                    op,
                    BinaryOp::Assign
                        | BinaryOp::AddAssign
                        | BinaryOp::SubAssign
                        | BinaryOp::MulAssign
                        | BinaryOp::DivAssign
                ) {
                    if let Expr::Member { object, member, .. } = &body.exprs[*lhs] {
                        let slot = self.member_slot_hint(*object, member);
                        let base = self.lower_expr(body, *object);
                        let rhs_val = self.lower_expr(body, *rhs);
                        let new_val = if *op == BinaryOp::Assign {
                            rhs_val.clone()
                        } else {
                            let current = self.new_temp(MirType::Unknown);
                            self.push_stmt(MirStmt::Assign {
                                place: Place::Temp(current),
                                value: Rvalue::GetField {
                                    base: base.clone(),
                                    field: member.clone(),
                                    slot,
                                },
                                span,
                            });
                            let bin_op = match op {
                                BinaryOp::AddAssign => BinaryOp::Add,
                                BinaryOp::SubAssign => BinaryOp::Sub,
                                BinaryOp::MulAssign => BinaryOp::Mul,
                                BinaryOp::DivAssign => BinaryOp::Div,
                                _ => BinaryOp::Assign,
                            };
                            let temp = self.new_temp(MirType::Unknown);
                            self.push_stmt(MirStmt::Assign {
                                place: Place::Temp(temp),
                                value: Rvalue::Binary {
                                    op: bin_op,
                                    lhs: Value::Temp(current),
                                    rhs: rhs_val,
                                },
                                span,
                            });
                            Value::Temp(temp)
                        };
                        self.push_stmt(MirStmt::SetField {
                            base,
                            field: member.clone(),
                            slot,
                            value: new_val.clone(),
                            span,
                        });
                        return new_val;
                    }
                }
                if matches!(op, BinaryOp::Otherwise) {
                    let result_val = self.lower_expr(body, *lhs);
                    let ok_flag = self.new_temp(MirType::Boolean);
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
                    let result_local = self.new_temp_local();
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Local(result_local),
                        value: Rvalue::Use(Value::Const(Literal::Nil)),
                        span,
                    });
                    self.set_terminator(Terminator::Branch {
                        cond: Value::Temp(ok_flag),
                        then_target: then_block,
                        else_target: else_block,
                        span,
                    });

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
                    let result_local = self.new_temp_local();
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Local(result_local),
                        value: Rvalue::Use(Value::Const(Literal::Nil)),
                        span,
                    });

                    self.set_terminator(Terminator::Branch {
                        cond: lhs_val.clone(),
                        then_target,
                        else_target,
                        span,
                    });

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
                if let Some((class_name, class_id)) = self.resolve_class_init_target(body, expr_id)
                {
                    let fields = self
                        .class_fields
                        .get(&class_name)
                        .cloned()
                        .unwrap_or_default();
                    if fields.is_empty() {
                        let temp = self.new_temp_for_expr(expr_id);
                        self.push_stmt(MirStmt::Assign {
                            place: Place::Temp(temp),
                            value: Rvalue::ClassInit {
                                class_id: class_id.0 as u32,
                                fields,
                            },
                            span,
                        });
                        self.maybe_call_configure(&class_name, Value::Temp(temp), span);
                        return Value::Temp(temp);
                    }
                }
                let obj_ty = self.expr_type(*object);
                if let MirType::Named(class_name) = obj_ty {
                    if let Some(derived) = self.class_derived.get(&class_name) {
                        if derived.contains(member) {
                            let receiver = self.lower_expr(body, *object);
                            let method = SmolStr::new(format!("{}.{}", class_name, member));
                            let method_id = self.method_id_for(&class_name, member);
                            let temp = self.new_temp_for_expr(expr_id);
                            self.push_stmt(MirStmt::Assign {
                                place: Place::Temp(temp),
                                value: Rvalue::Call {
                                    kind: CallKind::Sync,
                                    target: CallTarget::Method {
                                        receiver,
                                        method,
                                        method_id,
                                    },
                                    args: Vec::new(),
                                },
                                span,
                            });
                            return Value::Temp(temp);
                        }
                    }
                }
                let base = self.lower_expr(body, *object);
                let slot = self.member_slot_hint(*object, member);
                let temp = self.new_temp_for_expr(expr_id);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(temp),
                    value: Rvalue::GetField {
                        base,
                        field: member.clone(),
                        slot,
                    },
                    span,
                });
                Value::Temp(temp)
            }
            Expr::Call { callee, args, .. } | Expr::GivenCall { callee, args, .. } => {
                if let Some((class_name, class_id)) = self.resolve_class_init_target(body, *callee)
                {
                    let fields = self
                        .class_fields
                        .get(&class_name)
                        .cloned()
                        .unwrap_or_default();
                    let field_defaults = self
                        .class_field_defaults
                        .get(&class_name)
                        .cloned()
                        .unwrap_or_else(|| vec![None; fields.len()]);
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
                    for idx in 0..field_values.len() {
                        if let Some(value) = field_values[idx].clone() {
                            self.push_stmt(MirStmt::SetField {
                                base: Value::Temp(temp),
                                field: self
                                    .class_fields
                                    .get(&class_name)
                                    .and_then(|fields| fields.get(idx).cloned())
                                    .unwrap_or_default(),
                                slot: Some(idx as u32),
                                value,
                                span,
                            });
                        }
                    }
                    for (idx, default) in field_defaults.iter().enumerate() {
                        if field_values.get(idx).and_then(|val| val.as_ref()).is_none() {
                            if let Some(default) = default {
                                let value = self.lower_field_default(default, span);
                                self.push_stmt(MirStmt::SetField {
                                    base: Value::Temp(temp),
                                    field: self
                                        .class_fields
                                        .get(&class_name)
                                        .and_then(|fields| fields.get(idx).cloned())
                                        .unwrap_or_default(),
                                    slot: Some(idx as u32),
                                    value,
                                    span,
                                });
                            }
                        }
                    }
                    self.maybe_call_configure(&class_name, Value::Temp(temp), span);
                    return Value::Temp(temp);
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
                    value: Rvalue::BuildList {
                        items: values,
                        alloc: crate::mir::ir::AllocKind::LocalTemp,
                    },
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
                    value: Rvalue::BuildMap {
                        items: values,
                        alloc: crate::mir::ir::AllocKind::LocalTemp,
                    },
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
                    value: Rvalue::StringInterp {
                        parts: values,
                        alloc: crate::mir::ir::AllocKind::LocalTemp,
                    },
                    span,
                });
                Value::Temp(temp)
            }
        }
    }

    fn emit_defers(&mut self, body: &hir::Body, _span: TextRange) {
        let defers = self.defers.clone();
        for expr_id in defers.iter().rev() {
            let _ = self.lower_expr(body, *expr_id);
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
            if let Some(__wr_pool_size) = spec.size {
                size = __wr_pool_size;
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
                    target = Some(Value::Const(Literal::Integer(id.0 as i64)));
                    let fields = self.class_fields.get(name).cloned().unwrap_or_default();
                    let field_defaults = self
                        .class_field_defaults
                        .get(name)
                        .cloned()
                        .unwrap_or_else(|| vec![None; fields.len()]);
                    let temp = self.new_temp_for_expr(target_expr);
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Temp(temp),
                        value: Rvalue::ClassInit {
                            class_id: id.0 as u32,
                            fields,
                        },
                        span,
                    });
                    for (idx, default) in field_defaults.iter().enumerate() {
                        if let Some(default) = default {
                            let value = self.lower_field_default(default, span);
                            self.push_stmt(MirStmt::SetField {
                                base: Value::Temp(temp),
                                field: self
                                    .class_fields
                                    .get(name)
                                    .and_then(|fields| fields.get(idx).cloned())
                                    .unwrap_or_default(),
                                slot: Some(idx as u32),
                                value,
                                span,
                            });
                        }
                    }
                    self.maybe_call_configure(name, Value::Temp(temp), span);
                    instance = Some(Value::Temp(temp));
                }
            }
            Expr::Call { callee, .. } | Expr::GivenCall { callee, .. } => {
                let mut handled = false;
                if let Expr::Variable(name) = &body.exprs[*callee] {
                    if let Some(id) = self.type_tags.get(name).copied() {
                        target = Some(Value::Const(Literal::Integer(id.0 as i64)));
                        // `detach` on actor classes should always have a concrete instance.
                        // Some actor-class "constructor" call shapes don't lower to a normal
                        // `ClassInit` expression here, so build the instance explicitly from
                        // class metadata (same strategy as Pool.of fast paths).
                        if let Some(class) = self.class_target_info(body, *callee) {
                            let value = self.build_class_instance(&class, span);
                            instance = Some(value.clone());
                            lowered = Some(value);
                            handled = true;
                        }
                    }
                }
                if !handled {
                    let value = self.lower_expr(body, target_expr);
                    instance = Some(value.clone());
                    lowered = Some(value);
                }
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
            let target = Value::Const(Literal::Integer(class.class_id.0 as i64));
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
            value: Rvalue::BuildList {
                items: handles,
                alloc: crate::mir::ir::AllocKind::LocalTemp,
            },
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
        let resolved_size = compile_time_auto_pool_size(
            obj_code,
            min_size.unwrap_or(0),
            max_size.unwrap_or(0),
            weight.unwrap_or(0),
        );

        let size_temp = self.new_temp(MirType::Integer);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(size_temp),
            value: Rvalue::Use(Value::Const(Literal::Integer(resolved_size))),
            span,
        });

        let list_temp = self.new_temp(MirType::Unknown);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(list_temp),
            value: Rvalue::BuildList {
                items: Vec::new(),
                alloc: crate::mir::ir::AllocKind::LocalTemp,
            },
            span,
        });

        let idx_local = self.new_temp_local();
        self.push_stmt(MirStmt::Assign {
            place: Place::Local(idx_local),
            value: Rvalue::Use(Value::Const(Literal::Integer(0))),
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
        let cond_temp = self.new_temp(MirType::Boolean);
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
        let target = Value::Const(Literal::Integer(class.class_id.0 as i64));
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
                target: CallTarget::Function(SmolStr::new("__wr_list_push")),
                args: vec![Value::Temp(list_temp), Value::Temp(handle_temp)],
            },
            span,
        });

        let next_temp = self.new_temp(MirType::Integer);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(next_temp),
            value: Rvalue::Binary {
                op: BinaryOp::Add,
                lhs: Value::Local(idx_local),
                rhs: Value::Const(Literal::Integer(1)),
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
                let field_defaults = self
                    .class_field_defaults
                    .get(name)
                    .cloned()
                    .unwrap_or_else(|| vec![None; fields.len()]);
                Some(ClassTargetInfo {
                    name: name.clone(),
                    class_id,
                    fields,
                    field_defaults,
                    field_values: Vec::new(),
                })
            }
            Expr::Call { callee, args, .. } | Expr::GivenCall { callee, args, .. } => {
                let Expr::Variable(name) = &body.exprs[*callee] else {
                    return None;
                };
                let class_id = self.type_tags.get(name).copied()?;
                let fields = self.class_fields.get(name).cloned().unwrap_or_default();
                let field_defaults = self
                    .class_field_defaults
                    .get(name)
                    .cloned()
                    .unwrap_or_else(|| vec![None; fields.len()]);
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
                    name: name.clone(),
                    class_id,
                    fields,
                    field_defaults,
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
        let has_explicit_fields = class.field_values.iter().any(|val| val.is_some());
        for (idx, value) in class.field_values.iter().enumerate() {
            if let Some(value) = value {
                self.push_stmt(MirStmt::SetField {
                    base: Value::Temp(temp),
                    field: class.fields.get(idx).cloned().unwrap_or_default(),
                    slot: Some(idx as u32),
                    value: value.clone(),
                    span,
                });
            }
        }
        for (idx, default) in class.field_defaults.iter().enumerate() {
            if class
                .field_values
                .get(idx)
                .and_then(|val| val.as_ref())
                .is_none()
            {
                if let Some(default) = default {
                    let value = self.lower_field_default(default, span);
                    self.push_stmt(MirStmt::SetField {
                        base: Value::Temp(temp),
                        field: class.fields.get(idx).cloned().unwrap_or_default(),
                        slot: Some(idx as u32),
                        value,
                        span,
                    });
                }
            }
        }
        if has_explicit_fields {
            self.maybe_call_configure(&class.name, Value::Temp(temp), span);
        }
        Value::Temp(temp)
    }

    fn lower_field_default(&mut self, default: &hir::FieldDefault, span: TextRange) -> Value {
        match default {
            hir::FieldDefault::Literal(lit) => Value::Const(lit.clone()),
            hir::FieldDefault::List(items) => {
                let values = items
                    .iter()
                    .map(|item| self.lower_field_default(item, span))
                    .collect();
                let temp = self.new_temp(MirType::Unknown);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(temp),
                    value: Rvalue::BuildList {
                        items: values,
                        alloc: crate::mir::ir::AllocKind::LocalTemp,
                    },
                    span,
                });
                Value::Temp(temp)
            }
            hir::FieldDefault::Map(items) => {
                let values = items
                    .iter()
                    .map(|(key, value)| {
                        let key = self.lower_field_default(key, span);
                        let value = self.lower_field_default(value, span);
                        (key, value)
                    })
                    .collect();
                let temp = self.new_temp(MirType::Unknown);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(temp),
                    value: Rvalue::BuildMap {
                        items: values,
                        alloc: crate::mir::ir::AllocKind::LocalTemp,
                    },
                    span,
                });
                Value::Temp(temp)
            }
        }
    }

    fn maybe_call_configure(&mut self, class_name: &SmolStr, receiver: Value, span: TextRange) {
        if self.name == SmolStr::new(format!("{}.{}", class_name, "__configure__")) {
            return;
        }
        let method_id = match self
            .class_method_ids
            .get(class_name)
            .and_then(|methods| methods.get(&SmolStr::new("__configure__")))
        {
            Some(method_id) => *method_id,
            None => return,
        };
        let temp = self.new_temp(MirType::Unknown);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(temp),
            value: Rvalue::Call {
                kind: CallKind::Sync,
                target: CallTarget::Method {
                    receiver,
                    method: SmolStr::new(format!("{}.{}", class_name, "__configure__")),
                    method_id: Some(method_id),
                },
                args: Vec::new(),
            },
            span,
        });
    }

    fn parse_pool_of(&self, body: &hir::Body, expr_id: hir::Idx<Expr>) -> Option<PoolOfSpec> {
        let (callee, args) = match &body.exprs[expr_id] {
            Expr::Call { callee, args, .. } => (callee, args),
            Expr::GivenCall { callee, args, .. } => (callee, args),
            _ => return None,
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
                        if let Some(__wr_pool_size) = pool_size_from_expr(body, *value) {
                            size = Some(__wr_pool_size);
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
        if let Expr::Call { callee, args, .. } | Expr::GivenCall { callee, args, .. } =
            &body.exprs[expr_id]
        {
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
                let class_hint = match &body.exprs[*object] {
                    Expr::Variable(name) if self.type_tags.contains_key(name) => Some(name.clone()),
                    _ => None,
                };
                if let MirType::Named(class_name) = self.expr_type(*object) {
                    if let Some(methods) = self.interface_methods.get(&class_name) {
                        if methods.contains(member) {
                            let mut args_with_recv = Vec::with_capacity(values.len() + 1);
                            args_with_recv.push(receiver.clone());
                            args_with_recv.extend(values);
                            let func_name = SmolStr::new(format!("{}.{}", class_name, member));
                            return (CallTarget::Function(func_name), args_with_recv);
                        }
                    }
                }
                let (method_id, method_name) = match self.expr_type(*object) {
                    MirType::Actor(inner) => {
                        if let MirType::Named(class_name) = *inner {
                            (
                                self.method_id_for(&class_name, member),
                                SmolStr::new(format!("{}.{}", class_name, member)),
                            )
                        } else {
                            (None, member.clone())
                        }
                    }
                    MirType::Named(class_name) => (
                        self.method_id_for(&class_name, member),
                        SmolStr::new(format!("{}.{}", class_name, member)),
                    ),
                    _ => {
                        if let Some(class_name) = class_hint {
                            (
                                self.method_id_for(&class_name, member),
                                SmolStr::new(format!("{}.{}", class_name, member)),
                            )
                        } else {
                            (None, member.clone())
                        }
                    }
                };
                (
                    CallTarget::Method {
                        receiver,
                        method: method_name,
                        method_id,
                    },
                    values,
                )
            }
            Expr::Variable(name) if self.function_names.contains(name) => {
                if name.as_str() == "assert" && values.len() == 1 {
                    values.push(Value::Const(Literal::Nil));
                }
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

    fn member_slot_hint(&self, object_expr: hir::Idx<Expr>, member: &SmolStr) -> Option<u32> {
        let MirType::Named(class_name) = self.expr_type(object_expr) else {
            return None;
        };
        self.class_fields
            .get(&class_name)
            .and_then(|fields| fields.iter().position(|field| field == member))
            .map(|idx| idx as u32)
    }

    fn resolve_class_init_target(
        &self,
        body: &hir::Body,
        callee: hir::Idx<Expr>,
    ) -> Option<(SmolStr, TypeTagId)> {
        match &body.exprs[callee] {
            Expr::Variable(name) => self
                .type_tags
                .get(name)
                .copied()
                .map(|id| (name.clone(), id)),
            Expr::Member { object, member, .. } => {
                let enum_name = match &body.exprs[*object] {
                    Expr::Variable(name) => Some(name.clone()),
                    Expr::TypeApply { callee, .. } => match &body.exprs[*callee] {
                        Expr::Variable(name) => Some(name.clone()),
                        _ => None,
                    },
                    _ => None,
                };
                let enum_name = enum_name?;
                let class_name = SmolStr::new(format!("{}.{}", enum_name, member));
                self.type_tags
                    .get(&class_name)
                    .copied()
                    .map(|id| (class_name, id))
            }
            _ => None,
        }
    }
    fn is_default_match_pattern(&self, pattern: &hir::Pattern) -> bool {
        match pattern {
            hir::Pattern::Wildcard => true,
            hir::Pattern::Binding(name) => {
                !self.type_tags.contains_key(name) && builtin_type_tag(name).is_none()
            }
            _ => false,
        }
    }
}

fn compile_time_auto_pool_size(objective: i64, min: i64, max: i64, weight: i64) -> i64 {
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1) as i64;
    let base = match objective {
        0 => cores,
        1 => cores.saturating_mul(2),
        2 => (cores / 2).max(1),
        _ => cores,
    };
    let min = if min > 0 { min } else { 1 };
    let max = if max > 0 { max } else { cores.max(1) };
    let weight = if weight > 0 { weight } else { 1 };
    base.saturating_mul(weight).clamp(min, max.max(min))
}

fn builtin_type_tag(name: &SmolStr) -> Option<TypeTagId> {
    match name.as_str() {
        "Integer" => Some(TypeTagId(1)),
        "Boolean" => Some(TypeTagId(2)),
        "Nothing" | "Nil" => Some(TypeTagId(3)),
        "Float" => Some(TypeTagId(4)),
        "String" => Some(TypeTagId(5)),
        "List" => Some(TypeTagId(6)),
        "Map" => Some(TypeTagId(7)),
        "Actor" => Some(TypeTagId(8)),
        "Pending" => Some(TypeTagId(9)),
        "Iterator" => Some(TypeTagId(10)),
        "Result" => Some(TypeTagId(11)),
        "Pool" => Some(TypeTagId(12)),
        "Bytes" => Some(TypeTagId(13)),
        _ => None,
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
    name: SmolStr,
    class_id: TypeTagId,
    fields: Vec<SmolStr>,
    field_defaults: Vec<Option<hir::FieldDefault>>,
    field_values: Vec<Option<Value>>,
}

fn pool_size_from_expr(body: &hir::Body, expr_id: hir::Idx<Expr>) -> Option<hir::PoolSize> {
    match &body.exprs[expr_id] {
        Expr::Literal(hir::Literal::Integer(value)) => Some(hir::PoolSize::Fixed(*value)),
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
        Expr::Literal(hir::Literal::Integer(value)) => Some(*value),
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
        Expr::Call { callee, args, .. } | Expr::GivenCall { callee, args, .. } => {
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
                Expr::Literal(hir::Literal::Integer(value)) => Some(BackpressureSpec {
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
        Type::Integer => MirType::Integer,
        Type::Float => MirType::Float,
        Type::Boolean => MirType::Boolean,
        Type::String => MirType::String,
        Type::Nil => MirType::Nil,
        Type::Named(name, _) => MirType::Named(name.clone()),
        Type::Param(_) => MirType::Unknown,
        Type::Result(ok, err) => MirType::Result(
            Box::new(mir_type_from_type(ok)),
            Box::new(mir_type_from_type(err)),
        ),
        Type::Actor(inner) => MirType::Actor(Box::new(mir_type_from_type(inner))),
        Type::Pending(inner) => MirType::Pending(Box::new(mir_type_from_type(inner))),
        _ => MirType::Unknown,
    }
}

fn build_interface_dispatch_functions(
    module: &hir::Module,
    interface_impls: &HashMap<SmolStr, Vec<SmolStr>>,
    type_tags: &HashMap<SmolStr, TypeTagId>,
) -> Vec<MirFunction> {
    let mut functions = Vec::new();
    for (_idx, interface) in module.interfaces.iter() {
        let impls = interface_impls
            .get(&interface.name)
            .cloned()
            .unwrap_or_default();
        for method in &interface.methods {
            let params: Vec<SmolStr> = method.params.iter().map(|p| p.name.clone()).collect();
            functions.push(build_interface_dispatch_function(
                &interface.name,
                &method.name,
                &params,
                &impls,
                type_tags,
            ));
        }
    }
    functions
}

fn build_interface_dispatch_function(
    interface: &SmolStr,
    method: &SmolStr,
    params: &[SmolStr],
    impls: &[SmolStr],
    type_tags: &HashMap<SmolStr, TypeTagId>,
) -> MirFunction {
    let span = TextRange::empty(0.into());
    let mut locals = Vec::new();
    let mut params_ids = Vec::new();
    let mut temps = Vec::new();

    let receiver_id = LocalId(0);
    locals.push(Local {
        name: SmolStr::new("it"),
        mutable: false,
        ty: MirType::Unknown,
    });
    params_ids.push(receiver_id);

    for (idx, name) in params.iter().enumerate() {
        let local_id = LocalId(idx + 1);
        locals.push(Local {
            name: name.clone(),
            mutable: false,
            ty: MirType::Unknown,
        });
        params_ids.push(local_id);
    }

    let mut blocks = Vec::new();
    blocks.push(BasicBlock {
        stmts: Vec::new(),
        terminator: Terminator::Unreachable { span },
    });

    let mut cases = Vec::new();
    let mut impls_with_tags = Vec::new();
    for class in impls {
        let Some(tag) = type_tags.get(class) else {
            continue;
        };
        let block_id = BlockId(blocks.len());
        blocks.push(BasicBlock {
            stmts: Vec::new(),
            terminator: Terminator::Unreachable { span },
        });
        cases.push((SwitchCase::Type(*tag), block_id));
        impls_with_tags.push(class.clone());
    }

    let default_block = BlockId(blocks.len());
    blocks.push(BasicBlock {
        stmts: Vec::new(),
        terminator: Terminator::Unreachable { span },
    });

    blocks[0].terminator = Terminator::Switch {
        scrutinee: Value::Local(receiver_id),
        cases,
        default: default_block,
        span,
    };

    let call_args: Vec<Value> = params_ids.iter().map(|id| Value::Local(*id)).collect();

    for (idx, class) in impls_with_tags.iter().enumerate() {
        let block_id = BlockId(idx + 1);
        if block_id.0 >= blocks.len() {
            continue;
        }
        let temp_id = TempId(temps.len());
        temps.push(Temp {
            ty: MirType::Unknown,
        });
        let func_name = SmolStr::new(format!("{}.{}", class, method));
        blocks[block_id.0].stmts.push(MirStmt::Assign {
            place: Place::Temp(temp_id),
            value: Rvalue::Call {
                kind: CallKind::Sync,
                target: CallTarget::Function(func_name),
                args: call_args.clone(),
            },
            span,
        });
        blocks[block_id.0].terminator = Terminator::Return {
            value: Some(Value::Temp(temp_id)),
            span,
        };
    }

    let crash_temp = TempId(temps.len());
    temps.push(Temp {
        ty: MirType::Unknown,
    });
    blocks[default_block.0].stmts.push(MirStmt::Assign {
        place: Place::Temp(crash_temp),
        value: Rvalue::Crash {
            value: Value::Const(Literal::String(SmolStr::new("interface dispatch failed"))),
        },
        span,
    });
    blocks[default_block.0].terminator = Terminator::Return {
        value: Some(Value::Temp(crash_temp)),
        span,
    };

    MirFunction {
        name: SmolStr::new(format!("{}.{}", interface, method)),
        params: params_ids,
        locals,
        temps,
        blocks,
        entry: BlockId(0),
        suspendable: false,
    }
}

fn builtin_function_names() -> Vec<SmolStr> {
    vec![
        SmolStr::new("__wr_assert_err"),
        SmolStr::new("__wr_print"),
        SmolStr::new("__wr_bytes_from_string"),
        SmolStr::new("__wr_bytes_from_list"),
        SmolStr::new("__wr_bytes_to_string"),
        SmolStr::new("__wr_bytes_to_list"),
        SmolStr::new("__wr_bytes_len"),
        SmolStr::new("__wr_fs_read_bytes"),
        SmolStr::new("__wr_fs_write_bytes"),
        SmolStr::new("__wr_map_new"),
        SmolStr::new("__wr_list_push"),
        SmolStr::new("__wr_map_get"),
        SmolStr::new("__wr_map_set"),
        SmolStr::new("__wr_log"),
        SmolStr::new("__wr_log_configure"),
        SmolStr::new("__wr_runtime_cpu_count"),
        SmolStr::new("__wr_reactor_new"),
        SmolStr::new("__wr_reactor_drop"),
        SmolStr::new("__wr_reactor_register"),
        SmolStr::new("__wr_reactor_deregister"),
        SmolStr::new("__wr_reactor_arm_timer"),
        SmolStr::new("__wr_task_signal_new"),
        SmolStr::new("__wr_task_signal_drop"),
        SmolStr::new("__wr_task_unpark_one"),
        SmolStr::new("__wr_task_unpark_all"),
        SmolStr::new("__wr_task_epoch"),
        SmolStr::new("__wr_atomic_i64_new"),
        SmolStr::new("__wr_atomic_i64_drop"),
        SmolStr::new("__wr_atomic_i64_load"),
        SmolStr::new("__wr_atomic_i64_store"),
        SmolStr::new("__wr_atomic_i64_fetch_add"),
        SmolStr::new("__wr_pool_size"),
        SmolStr::new("__wr_pool_rr"),
        SmolStr::new("__wr_pool_queue_len"),
        SmolStr::new("__wr_actor_mailbox_len"),
        SmolStr::new("__wr_actor_pause"),
        SmolStr::new("__wr_actor_resume"),
        SmolStr::new("__wr_actor_pause_wait"),
        SmolStr::new("__wr_actor_fire_burst_begin"),
        SmolStr::new("__wr_actor_fire_burst_end"),
        SmolStr::new("__wr_actor_fire_burst_abort"),
        SmolStr::new("__wr_metrics_get"),
        SmolStr::new("__wr_metrics_dropped_paused_id"),
        SmolStr::new("__wr_metrics_messages_dropped_id"),
        SmolStr::new("__wr_clock_ns"),
        SmolStr::new("__wr_sleep_ms"),
        SmolStr::new("__wr_env_get"),
        SmolStr::new("__wr_env_set"),
        SmolStr::new("__wr_runtime_configure"),
    ]
}
