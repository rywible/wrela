//! Owns core MIR-lowering state shared across the lowering submodules.
//! Does not own the actual lowering traversal logic for statements or
//! expressions.
//!
//! Key invariants:
//! - shared lowering state must stay internally consistent across submodule
//!   helper calls.
//! - execution-mode and loop-target bookkeeping are semantic inputs to later
//!   lowering steps, not incidental control-flow details.
//!
//! Primary entrypoints:
//! - `FunctionLowerer`
//! - `LoopTarget`
//!
//! Failure modes / common pitfalls:
//! - mutating shared lowering state without keeping these core structs aligned
//!   can corrupt emitted MIR across many callers.

use super::*;

pub(crate) struct LoopTarget {
    pub(crate) break_target: BlockId,
    pub(crate) continue_target: BlockId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ShapeExecutionMode {
    SupportPruned,
    Conservative,
}

impl ShapeExecutionMode {
    pub(crate) fn distance_helper_name(self, shape: &SmolStr) -> SmolStr {
        match self {
            ShapeExecutionMode::SupportPruned => {
                SmolStr::new(format!("__wr_shape_distance_{shape}"))
            }
            ShapeExecutionMode::Conservative => {
                SmolStr::new(format!("__wr_shape_distance_conservative_{shape}"))
            }
        }
    }

    pub(crate) fn trace_helper_name(self, shape: &SmolStr) -> SmolStr {
        match self {
            ShapeExecutionMode::SupportPruned => SmolStr::new(format!("__wr_shape_trace_{shape}")),
            ShapeExecutionMode::Conservative => {
                SmolStr::new(format!("__wr_shape_trace_conservative_{shape}"))
            }
        }
    }

    pub(crate) fn allows_support_pruning(self) -> bool {
        matches!(self, ShapeExecutionMode::SupportPruned)
    }
}

pub(crate) struct FunctionLowerer {
    pub(crate) name: SmolStr,
    pub(crate) params: Vec<LocalId>,
    pub(crate) locals: Vec<Local>,
    pub(crate) temps: Vec<Temp>,
    pub(crate) blocks: Vec<BasicBlock>,
    pub(crate) current_block: BlockId,
    pub(crate) suspendable: bool,
    pub(crate) scopes: Vec<HashMap<SmolStr, LocalId>>,
    pub(crate) result_scopes: Vec<HashMap<SmolStr, bool>>,
    pub(crate) loop_stack: Vec<LoopTarget>,
    pub(crate) type_tags: HashMap<SmolStr, TypeTagId>,
    pub(crate) class_fields: HashMap<SmolStr, Vec<SmolStr>>,
    pub(crate) class_field_defaults: HashMap<SmolStr, Vec<Option<hir::FieldDefault>>>,
    pub(crate) class_method_ids: HashMap<SmolStr, HashMap<SmolStr, u32>>,
    pub(crate) interface_methods: HashMap<SmolStr, HashSet<SmolStr>>,
    pub(crate) function_names: HashSet<SmolStr>,
    pub(crate) field_names: HashSet<SmolStr>,
    pub(crate) shape_names: HashSet<SmolStr>,
    pub(crate) shape_graphs: HashMap<SmolStr, hir::ShapeGraph>,
    pub(crate) field_graphs: HashMap<SmolStr, hir::FieldGraph>,
    pub(crate) field_bodies: HashMap<SmolStr, hir::Body>,
    pub(crate) field_metadata: HashMap<SmolStr, hir::FieldMetadata>,
    pub(crate) field_scenes: BTreeMap<SmolStr, scene_ir::FieldScene>,
    pub(crate) shape_scenes: BTreeMap<SmolStr, scene_ir::ShapeScene>,
    pub(crate) radiance_param_counts: HashMap<SmolStr, usize>,
    pub(crate) volume_param_counts: HashMap<SmolStr, usize>,
    pub(crate) result_functions: HashSet<SmolStr>,
    pub(crate) default_query_backend: DispatchBackend,
    pub(crate) returns_result: bool,
    pub(crate) type_info: Option<FunctionTypeInfo>,
    pub(crate) defers: Vec<hir::Idx<hir::Expr>>,
    pub(crate) objective_stack: Vec<hir::Objective>,
}

impl FunctionLowerer {
    pub(crate) fn new(
        name: SmolStr,
        type_tags: &HashMap<SmolStr, TypeTagId>,
        class_fields: &HashMap<SmolStr, Vec<SmolStr>>,
        class_field_defaults: &HashMap<SmolStr, Vec<Option<hir::FieldDefault>>>,
        function_names: &HashSet<SmolStr>,
        field_names: &HashSet<SmolStr>,
        shape_names: &HashSet<SmolStr>,
        shape_graphs: &HashMap<SmolStr, hir::ShapeGraph>,
        field_graphs: &HashMap<SmolStr, hir::FieldGraph>,
        field_bodies: &HashMap<SmolStr, hir::Body>,
        field_metadata: &HashMap<SmolStr, hir::FieldMetadata>,
        radiance_param_counts: &HashMap<SmolStr, usize>,
        volume_param_counts: &HashMap<SmolStr, usize>,
        result_functions: &HashSet<SmolStr>,
        class_method_ids: &HashMap<SmolStr, HashMap<SmolStr, u32>>,
        interface_methods: &HashMap<SmolStr, HashSet<SmolStr>>,
        returns_result: bool,
        type_info: Option<&FunctionTypeInfo>,
    ) -> Self {
        let scene_field_graphs = field_graphs
            .iter()
            .map(|(name, graph)| (name.clone(), graph.clone()))
            .collect::<BTreeMap<_, _>>();
        let scene_field_bodies = field_bodies
            .iter()
            .map(|(name, body)| (name.clone(), body.clone()))
            .collect::<BTreeMap<_, _>>();
        let scene_field_metadata = field_metadata
            .iter()
            .map(|(name, metadata)| (name.clone(), metadata.clone()))
            .collect::<BTreeMap<_, _>>();
        let scene_shape_graphs = shape_graphs
            .iter()
            .map(|(name, graph)| (name.clone(), graph.clone()))
            .collect::<BTreeMap<_, _>>();
        let field_scenes = scene_ir::lower_field_scenes(
            &scene_field_graphs,
            &scene_field_bodies,
            &scene_field_metadata,
        );
        let shape_scenes = scene_ir::lower_shape_scenes(&scene_shape_graphs, &field_scenes);
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
            interface_methods: interface_methods.clone(),
            function_names: function_names.clone(),
            field_names: field_names.clone(),
            shape_names: shape_names.clone(),
            shape_graphs: shape_graphs.clone(),
            field_graphs: field_graphs.clone(),
            field_bodies: field_bodies.clone(),
            field_metadata: field_metadata.clone(),
            field_scenes,
            shape_scenes,
            radiance_param_counts: radiance_param_counts.clone(),
            volume_param_counts: volume_param_counts.clone(),
            result_functions: result_functions.clone(),
            default_query_backend: DispatchBackend::Auto,
            returns_result,
            type_info: type_info.cloned(),
            defers: Vec::new(),
            objective_stack: Vec::new(),
        }
    }

    pub(crate) fn current_objective(&self) -> Option<hir::Objective> {
        self.objective_stack.last().copied()
    }

    pub(crate) fn resolve_default_query_backend(
        &self,
        backend: DispatchBackend,
    ) -> DispatchBackend {
        match backend {
            DispatchBackend::Auto => match self.default_query_backend {
                DispatchBackend::Auto | DispatchBackend::Cpu => DispatchBackend::Cpu,
                DispatchBackend::VirtualGpu => DispatchBackend::VirtualGpu,
                DispatchBackend::Wgsl => DispatchBackend::Wgsl,
            },
            explicit => explicit,
        }
    }

    pub(crate) fn dispatch_backend_id(backend: DispatchBackend) -> i64 {
        match backend {
            DispatchBackend::Cpu => 0,
            DispatchBackend::VirtualGpu => 1,
            DispatchBackend::Wgsl => 2,
            DispatchBackend::Auto => 3,
        }
    }

    pub(crate) fn world_query_plan_backend(
        &self,
        body: &hir::Body,
        backend_expr: Option<hir::Idx<hir::Expr>>,
    ) -> DispatchBackend {
        match backend_expr {
            Some(expr_id) => self
                .parse_dispatch_backend_builtin(body, expr_id)
                .and_then(|id| i32::try_from(id).ok().and_then(DispatchBackend::from_id))
                .map(|backend| self.resolve_default_query_backend(backend))
                .unwrap_or(DispatchBackend::Auto),
            None => self.resolve_default_query_backend(DispatchBackend::Auto),
        }
    }

    pub(crate) fn lower_world_query_backend_value(
        &mut self,
        body: &hir::Body,
        backend_expr: Option<hir::Idx<hir::Expr>>,
        span: TextRange,
    ) -> Value {
        match backend_expr {
            Some(expr_id) => {
                if let Some(backend) = self
                    .parse_dispatch_backend_builtin(body, expr_id)
                    .and_then(|id| i32::try_from(id).ok().and_then(DispatchBackend::from_id))
                    .map(|backend| self.resolve_default_query_backend(backend))
                {
                    Value::Const(Literal::Integer(Self::dispatch_backend_id(backend)))
                } else {
                    let backend = self.lower_expr(body, expr_id);
                    self.lower_dispatch_backend_id(backend, span)
                }
            }
            None => Value::Const(Literal::Integer(Self::dispatch_backend_id(
                self.resolve_default_query_backend(DispatchBackend::Auto),
            ))),
        }
    }

    pub(crate) fn new_block(&mut self) -> BlockId {
        let id = BlockId(self.blocks.len());
        self.blocks.push(BasicBlock {
            stmts: Vec::new(),
            terminator: Terminator::Unreachable {
                span: TextRange::empty(0.into()),
            },
        });
        id
    }

    pub(crate) fn block_is_open(&self, block: BlockId) -> bool {
        matches!(
            self.blocks[block.0].terminator,
            Terminator::Unreachable { .. }
        )
    }

    pub(crate) fn set_terminator(&mut self, term: Terminator) {
        self.blocks[self.current_block.0].terminator = term;
    }

    pub(crate) fn push_stmt(&mut self, stmt: Stmt) {
        self.blocks[self.current_block.0].stmts.push(stmt);
    }

    pub(crate) fn local_type_for_name(&self, name: &SmolStr) -> MirType {
        self.type_info
            .as_ref()
            .and_then(|info| info.local_types.get(name))
            .map(mir_type_from_type)
            .unwrap_or(MirType::Unknown)
    }

    pub(crate) fn expr_type(&self, body: &hir::Body, expr_id: hir::Idx<Expr>) -> MirType {
        self.type_info
            .as_ref()
            .and_then(|info| info.expr_type(body, expr_id))
            .map(mir_type_from_type)
            .unwrap_or(MirType::Unknown)
    }

    pub(crate) fn proven_range_induction_type(
        lhs_ty: &MirType,
        rhs_ty: &MirType,
    ) -> Option<MirType> {
        match (lhs_ty, rhs_ty) {
            (MirType::Integer, MirType::Integer) => Some(MirType::Integer),
            (MirType::Float, MirType::Float) => Some(MirType::Float),
            _ => None,
        }
    }

    pub(crate) fn new_temp_for_expr(
        &mut self,
        body: &hir::Body,
        expr_id: hir::Idx<Expr>,
    ) -> TempId {
        let ty = self.expr_type(body, expr_id);
        self.new_temp(ty)
    }

    pub(crate) fn new_temp(&mut self, ty: MirType) -> TempId {
        let id = TempId(self.temps.len());
        self.temps.push(Temp { ty });
        id
    }

    pub(crate) fn new_local(&mut self, name: SmolStr, mutable: bool, ty: MirType) -> LocalId {
        let id = LocalId(self.locals.len());
        self.locals.push(Local { name, mutable, ty });
        id
    }

    pub(crate) fn new_temp_local(&mut self) -> LocalId {
        let name = SmolStr::new(format!("$tmp{}", self.locals.len()));
        self.new_local(name, true, MirType::Unknown)
    }

    pub(crate) fn declare_local(&mut self, name: SmolStr, local: LocalId) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, local);
        }
    }

    pub(crate) fn declare_resultness(&mut self, name: SmolStr, is_result: bool) {
        if let Some(scope) = self.result_scopes.last_mut() {
            scope.insert(name, is_result);
        }
    }

    pub(crate) fn set_resultness(&mut self, name: &SmolStr, is_result: bool) {
        for scope in self.result_scopes.iter_mut().rev() {
            if let Some(entry) = scope.get_mut(name) {
                *entry = is_result;
                return;
            }
        }
    }

    pub(crate) fn resolve_resultness(&self, name: &SmolStr) -> Option<bool> {
        for scope in self.result_scopes.iter().rev() {
            if let Some(result) = scope.get(name) {
                return Some(*result);
            }
        }
        None
    }

    pub(crate) fn resolve_local(&self, name: &SmolStr) -> Option<LocalId> {
        for scope in self.scopes.iter().rev() {
            if let Some(local) = scope.get(name) {
                return Some(*local);
            }
        }
        None
    }

    pub(crate) fn expr_is_result(&self, body: &hir::Body, expr_id: hir::Idx<Expr>) -> bool {
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

    pub(crate) fn enter_scope(&mut self) {
        self.scopes.push(HashMap::new());
        self.result_scopes.push(HashMap::new());
    }

    pub(crate) fn exit_scope(&mut self) {
        self.scopes.pop();
        self.result_scopes.pop();
    }
}
