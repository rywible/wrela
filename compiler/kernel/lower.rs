use crate::hir::typeck::{FunctionTypeInfo, Type, TypeInfo};
use crate::hir::{self, Arg, Body, Expr, Function, FunctionLane, FunctionRole, Idx, Module, Stmt};
use crate::kernel::ir::{
    KernelBatchQueryPlan, KernelBlock, KernelCaptureQueryPlan, KernelExpr, KernelFunction,
    KernelModule, KernelParam, KernelPlanStage, KernelStmt, KernelWorldQueryPlan,
    ParsedKernelDispatch,
};
use crate::kernel::program::KernelProgram;
use crate::portable;
use crate::query_exec::QueryExecContext;
use crate::query_plan::{
    BatchQueryKind, BatchQueryPlan, CaptureKind, CaptureQueryKind, CaptureQueryPlan,
    DispatchBackend, SceneSummary, WorldQueryKind, WorldQueryPlan,
};
use crate::scene_ir::{self, FieldScene, ShapeScene};
use rowan::TextRange;
use smol_str::SmolStr;
use std::collections::{BTreeMap, HashMap, HashSet};
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq)]
pub enum KernelLowerError {
    #[error("kernel entry '{name}' was not found")]
    MissingEntry { name: SmolStr },
    #[error("kernel entry '{name}' must be declared as `kernel fn`")]
    EntryNotKernel { name: SmolStr },
    #[error("kernel function '{caller}' calls non-portable function '{callee}'")]
    NonPortableCallee {
        caller: SmolStr,
        callee: SmolStr,
        span: TextRange,
    },
    #[error("missing type information for function '{name}'")]
    MissingTypeInfo { name: SmolStr },
    #[error("function '{name}' does not have a body")]
    MissingBody { name: SmolStr },
    #[error("unsupported statement '{kind}' in function '{function}'")]
    UnsupportedStatement {
        function: SmolStr,
        kind: &'static str,
        span: TextRange,
    },
    #[error("unsupported expression '{kind}' in function '{function}'")]
    UnsupportedExpression {
        function: SmolStr,
        kind: &'static str,
        span: TextRange,
    },
    #[error("unknown call target '{callee}' in function '{function}'")]
    UnknownCallTarget {
        function: SmolStr,
        callee: SmolStr,
        span: TextRange,
    },
    #[error("unknown local '{name}' in function '{function}'")]
    UnknownLocal {
        function: SmolStr,
        name: SmolStr,
        span: TextRange,
    },
    #[error("missing argument '{param}' for '{callee}' in function '{function}'")]
    MissingArgument {
        function: SmolStr,
        callee: SmolStr,
        param: SmolStr,
        span: TextRange,
    },
    #[error("unknown named argument '{arg}' for '{callee}' in function '{function}'")]
    UnknownNamedArgument {
        function: SmolStr,
        callee: SmolStr,
        arg: SmolStr,
        span: TextRange,
    },
    #[error("duplicate named argument '{arg}' for '{callee}' in function '{function}'")]
    DuplicateNamedArgument {
        function: SmolStr,
        callee: SmolStr,
        arg: SmolStr,
        span: TextRange,
    },
    #[error("missing expression type information in function '{function}'")]
    MissingExprType { function: SmolStr, span: TextRange },
}

pub fn lower_kernel_entry_by_name(
    module: &Module,
    type_info: &TypeInfo,
    entry: &str,
) -> Result<KernelProgram, Vec<KernelLowerError>> {
    let functions_by_name = top_level_function_indices(module);
    let entry_name = SmolStr::new(entry);
    let Some(entry_idx) = functions_by_name.get(&entry_name).copied() else {
        return Err(vec![KernelLowerError::MissingEntry { name: entry_name }]);
    };
    let function = &module.functions[entry_idx];
    if function.role != FunctionRole::Kernel {
        return Err(vec![KernelLowerError::EntryNotKernel {
            name: function.name.clone(),
        }]);
    }
    lower_kernel_function(module, type_info, entry_idx)
}

pub fn lower_kernel_function(
    module: &Module,
    type_info: &TypeInfo,
    entry: Idx<Function>,
) -> Result<KernelProgram, Vec<KernelLowerError>> {
    let context = KernelLowerContext::new(module, type_info);
    let mut lowerer = KernelModuleLowerer {
        context,
        lowered: BTreeMap::new(),
        errors: Vec::new(),
    };
    lowerer.lower_function_recursive(entry);
    if lowerer.errors.is_empty() {
        let kernel_module = KernelModule {
            entry: module.functions[entry].name.clone(),
            functions: lowerer.lowered.into_values().collect(),
        };
        Ok(KernelProgram::new(
            kernel_module,
            QueryExecContext::compile(module, type_info),
        ))
    } else {
        Err(lowerer.errors)
    }
}

pub fn parse_dispatch_compute(
    body: &hir::Body,
    expr_id: hir::Idx<Expr>,
) -> Option<ParsedKernelDispatch> {
    let (callee, args) = match &body.exprs[expr_id] {
        Expr::Call { callee, args, .. } => (callee, args),
        _ => return None,
    };
    let Expr::Variable(name) = &body.exprs[*callee] else {
        return None;
    };
    if name.as_str() != "dispatch_compute" {
        return None;
    }

    let mut kernel = None;
    let mut workgroups_x = None;
    let mut workgroups_y = None;
    let mut workgroups_z = None;
    let mut workgroup_size_x = None;
    let mut workgroup_size_y = None;
    let mut workgroup_size_z = None;
    let mut schedule = None;
    let mut kernel_args = Vec::new();

    for arg in args {
        match arg {
            hir::Arg::Positional { value, .. } => kernel_args.push(*value),
            hir::Arg::Named { name, value, .. } => match name.as_str() {
                "kernel" => {
                    if let Expr::Variable(func_name) = &body.exprs[*value] {
                        kernel = Some(func_name.clone());
                    } else {
                        return None;
                    }
                }
                "workgroups_x" => workgroups_x = Some(*value),
                "workgroups_y" => workgroups_y = Some(*value),
                "workgroups_z" => workgroups_z = Some(*value),
                "workgroup_size_x" => workgroup_size_x = Some(*value),
                "workgroup_size_y" => workgroup_size_y = Some(*value),
                "workgroup_size_z" => workgroup_size_z = Some(*value),
                "schedule" => schedule = Some(*value),
                _ => kernel_args.push(*value),
            },
        }
    }

    Some(ParsedKernelDispatch {
        kernel: kernel?,
        workgroups: [workgroups_x?, workgroups_y?, workgroups_z?],
        workgroup_size: [workgroup_size_x?, workgroup_size_y?, workgroup_size_z?],
        schedule,
        kernel_args,
    })
}

pub fn lower_batch_query_plan(plan: &BatchQueryPlan) -> KernelBatchQueryPlan {
    KernelBatchQueryPlan {
        helper_name: plan.helper_name.clone(),
        kind: plan.kind,
        capture_kind: plan.capture_kind,
        backend: plan.backend,
        kernel: plan.kernel,
        item_kind: plan.item_kind,
        result_kind: plan.result_kind,
        executor: plan.executor,
        scene: plan.scene.clone(),
        candidate_strategy: plan.candidate_strategy,
        pruning_strategy: plan.pruning_strategy,
        stages: plan.stages.iter().map(KernelPlanStage::from).collect(),
        derived_artifacts: plan.derived_artifacts.clone(),
        preserves_local_hit_context: plan.preserves_local_hit_context,
    }
}

pub fn lower_capture_query_plan(plan: &CaptureQueryPlan) -> KernelCaptureQueryPlan {
    plan.into()
}

pub fn lower_world_query_plan(plan: &WorldQueryPlan) -> KernelWorldQueryPlan {
    plan.into()
}

struct KernelLowerContext<'a> {
    module: &'a Module,
    type_info: &'a TypeInfo,
    functions_by_name: HashMap<SmolStr, Idx<Function>>,
    field_names: HashSet<SmolStr>,
    shape_names: HashSet<SmolStr>,
    field_scenes: BTreeMap<SmolStr, FieldScene>,
    shape_scenes: BTreeMap<SmolStr, ShapeScene>,
}

impl<'a> KernelLowerContext<'a> {
    fn new(module: &'a Module, type_info: &'a TypeInfo) -> Self {
        let functions_by_name = top_level_function_indices(module);
        let field_names = module
            .functions
            .iter()
            .filter_map(|(_, func)| (func.role == FunctionRole::Field).then_some(func.name.clone()))
            .collect::<HashSet<_>>();
        let field_graphs = module
            .functions
            .iter()
            .filter_map(|(_, func)| {
                func.field_graph
                    .as_ref()
                    .map(|graph| (func.name.clone(), graph.clone()))
            })
            .collect::<BTreeMap<_, _>>();
        let field_bodies = module
            .functions
            .iter()
            .filter_map(|(_, func)| {
                func.body
                    .as_ref()
                    .map(|body| (func.name.clone(), body.clone()))
            })
            .collect::<BTreeMap<_, _>>();
        let field_metadata = module
            .functions
            .iter()
            .filter_map(|(_, func)| {
                func.field
                    .as_ref()
                    .map(|metadata| (func.name.clone(), metadata.clone()))
            })
            .collect::<BTreeMap<_, _>>();
        let field_scenes =
            scene_ir::lower_field_scenes(&field_graphs, &field_bodies, &field_metadata);
        let shape_names = module
            .shapes
            .iter()
            .map(|(_, shape)| shape.name.clone())
            .collect::<HashSet<_>>();
        let shape_graphs = module
            .shapes
            .iter()
            .filter_map(|(_, shape)| {
                shape
                    .graph
                    .as_ref()
                    .map(|graph| (shape.name.clone(), graph.clone()))
            })
            .collect::<BTreeMap<_, _>>();
        let shape_scenes = scene_ir::lower_shape_scenes(&shape_graphs, &field_scenes);
        Self {
            module,
            type_info,
            functions_by_name,
            field_names,
            shape_names,
            field_scenes,
            shape_scenes,
        }
    }
}

struct KernelModuleLowerer<'a> {
    context: KernelLowerContext<'a>,
    lowered: BTreeMap<usize, KernelFunction>,
    errors: Vec<KernelLowerError>,
}

impl<'a> KernelModuleLowerer<'a> {
    fn lower_function_recursive(&mut self, function_idx: Idx<Function>) {
        if self.lowered.contains_key(&function_idx.into_raw()) {
            return;
        }
        let function = &self.context.module.functions[function_idx];
        let Some(body) = &function.body else {
            self.errors.push(KernelLowerError::MissingBody {
                name: function.name.clone(),
            });
            return;
        };
        let Some(fn_info) = self.context.type_info.function(function_idx) else {
            self.errors.push(KernelLowerError::MissingTypeInfo {
                name: function.name.clone(),
            });
            return;
        };
        let mut lowerer = KernelFunctionLowerer {
            parent: self,
            function,
            body,
            fn_info,
            callees: HashSet::new(),
        };
        if let Some(lowered) = lowerer.lower_function() {
            let callees = lowerer.callees.into_iter().collect::<Vec<_>>();
            self.lowered.insert(function_idx.into_raw(), lowered);
            for callee in callees {
                self.lower_function_recursive(callee);
            }
        }
    }
}

struct KernelFunctionLowerer<'a, 'b> {
    parent: &'b mut KernelModuleLowerer<'a>,
    function: &'a Function,
    body: &'a Body,
    fn_info: &'a FunctionTypeInfo,
    callees: HashSet<Idx<Function>>,
}

impl<'a, 'b> KernelFunctionLowerer<'a, 'b> {
    fn lower_function(&mut self) -> Option<KernelFunction> {
        let mut params = Vec::with_capacity(self.function.params.len());
        for param in &self.function.params {
            let ty = self.local_type(&param.name, TextRange::empty(0.into()))?;
            params.push(KernelParam {
                name: param.name.clone(),
                ty,
            });
        }
        let ret = self.lower_type_ref(self.function.ret_type.as_ref())?;
        let body = if self.function.role == FunctionRole::Domain {
            self.lower_domain_body()?
        } else {
            self.lower_stmt_block(&self.body.root_stmts)?
        };
        Some(KernelFunction {
            name: self.function.name.clone(),
            params,
            ret,
            body,
        })
    }

    fn lower_domain_body(&mut self) -> Option<KernelBlock> {
        let span = TextRange::empty(0.into());
        let metadata = self.function.domain.as_ref()?;
        let scene_id = if let Some(world_param) = self.function.params.first() {
            KernelExpr::Member {
                base: Box::new(KernelExpr::Var {
                    name: world_param.name.clone(),
                    ty: self.local_type(&world_param.name, span)?,
                    span,
                }),
                member: SmolStr::new("scene_id"),
                ty: Type::U32,
                span,
            }
        } else {
            KernelExpr::Literal {
                value: hir::Literal::Integer(0),
                ty: Type::U32,
                span,
            }
        };
        let fields = vec![
            (SmolStr::new("scene_id"), scene_id),
            (
                SmolStr::new("geometry_detail"),
                KernelExpr::Literal {
                    value: hir::Literal::Integer(match metadata.geometry_detail {
                        hir::DomainGeometryDetail::Coarse => 0,
                        hir::DomainGeometryDetail::Fine => 1,
                    }),
                    ty: Type::I32,
                    span,
                },
            ),
            (
                SmolStr::new("material"),
                KernelExpr::Literal {
                    value: hir::Literal::Boolean(metadata.material),
                    ty: Type::Boolean,
                    span,
                },
            ),
            (
                SmolStr::new("radiance"),
                KernelExpr::Literal {
                    value: hir::Literal::Boolean(metadata.radiance),
                    ty: Type::Boolean,
                    span,
                },
            ),
            (
                SmolStr::new("media"),
                KernelExpr::Literal {
                    value: hir::Literal::Boolean(metadata.media),
                    ty: Type::Boolean,
                    span,
                },
            ),
            (
                SmolStr::new("max_distance"),
                metadata.max_distance.as_ref().map_or(
                    Some(KernelExpr::Literal {
                        value: hir::Literal::Float(12.0),
                        ty: Type::F32,
                        span,
                    }),
                    |body| self.lower_domain_body_value(body, Type::F32),
                )?,
            ),
            (
                SmolStr::new("min_step"),
                metadata.min_step.as_ref().map_or(
                    Some(KernelExpr::Literal {
                        value: hir::Literal::Float(0.02),
                        ty: Type::F32,
                        span,
                    }),
                    |body| self.lower_domain_body_value(body, Type::F32),
                )?,
            ),
            (
                SmolStr::new("hit_epsilon"),
                metadata.hit_epsilon.as_ref().map_or(
                    Some(KernelExpr::Literal {
                        value: hir::Literal::Float(0.001),
                        ty: Type::F32,
                        span,
                    }),
                    |body| self.lower_domain_body_value(body, Type::F32),
                )?,
            ),
            (
                SmolStr::new("max_steps"),
                metadata.max_steps.as_ref().map_or(
                    Some(KernelExpr::Literal {
                        value: hir::Literal::Integer(96),
                        ty: Type::I32,
                        span,
                    }),
                    |body| self.lower_domain_body_value(body, Type::I32),
                )?,
            ),
        ];
        Some(vec![KernelStmt::Return {
            value: Some(KernelExpr::StructLiteral {
                name: SmolStr::new("SceneDomain"),
                fields,
                ty: Type::Named(SmolStr::new("SceneDomain"), Vec::new()),
                span,
            }),
            span,
        }])
    }

    fn lower_stmt_block(&mut self, stmts: &[Idx<Stmt>]) -> Option<KernelBlock> {
        let mut out = Vec::with_capacity(stmts.len());
        for stmt_id in stmts {
            let stmt = self.lower_stmt(*stmt_id)?;
            out.push(stmt);
        }
        Some(out)
    }

    fn lower_domain_body_value(&mut self, body: &'a hir::Body, ty: Type) -> Option<KernelExpr> {
        let last = *body.root_stmts.last()?;
        let expr = match &body.stmts[last] {
            Stmt::Expr(expr) | Stmt::Return(Some(expr)) => *expr,
            _ => return None,
        };
        self.lower_domain_value_expr(body, expr, ty)
    }

    fn lower_domain_value_expr(
        &mut self,
        body: &'a hir::Body,
        expr_id: Idx<Expr>,
        ty: Type,
    ) -> Option<KernelExpr> {
        let span = body.expr_span(expr_id);
        match &body.exprs[expr_id] {
            Expr::Literal(value) => Some(KernelExpr::Literal {
                value: value.clone(),
                ty,
                span,
            }),
            Expr::Variable(name) => Some(KernelExpr::Var {
                name: name.clone(),
                ty,
                span,
            }),
            Expr::Unary { op, expr, .. } => Some(KernelExpr::Unary {
                op: *op,
                expr: Box::new(self.lower_domain_value_expr(body, *expr, ty.clone())?),
                ty,
                span,
            }),
            Expr::Binary { lhs, op, rhs, .. } => Some(KernelExpr::Binary {
                op: *op,
                lhs: Box::new(self.lower_domain_value_expr(body, *lhs, ty.clone())?),
                rhs: Box::new(self.lower_domain_value_expr(body, *rhs, ty.clone())?),
                ty,
                span,
            }),
            Expr::Call { callee, args, type_args } if type_args.is_empty() => {
                let Expr::Variable(target) = &body.exprs[*callee] else {
                    return None;
                };
                let args = args
                    .iter()
                    .map(|arg| match arg {
                        Arg::Positional { value, .. } | Arg::Named { value, .. } => {
                            self.lower_domain_value_expr(body, *value, ty.clone())
                        }
                    })
                    .collect::<Option<Vec<_>>>()?;
                Some(KernelExpr::Call {
                    target: target.clone(),
                    args,
                    ty,
                    span,
                })
            }
            _ => None,
        }
    }

    fn lower_stmt(&mut self, stmt_id: Idx<Stmt>) -> Option<KernelStmt> {
        let stmt = &self.body.stmts[stmt_id];
        let span = self.body.stmt_span(stmt_id);
        match stmt {
            Stmt::Expr(expr) => Some(KernelStmt::Expr {
                value: self.lower_expr(*expr)?,
                span,
            }),
            Stmt::IgnoreResult { expr } => Some(KernelStmt::IgnoreResult {
                value: self.lower_expr(*expr)?,
                span,
            }),
            Stmt::Let {
                name,
                value,
                mutable,
                ..
            } => Some(KernelStmt::Let {
                name: name.clone(),
                mutable: *mutable,
                ty: self.local_type(name, span)?,
                value: self.lower_expr(*value)?,
                span,
            }),
            Stmt::Assign {
                name, op, value, ..
            } => Some(KernelStmt::Assign {
                name: name.clone(),
                op: *op,
                value: self.lower_expr(*value)?,
                span,
            }),
            Stmt::If {
                condition,
                then_branch,
                else_branch,
            } => Some(KernelStmt::If {
                condition: self.lower_expr(*condition)?,
                then_block: self.lower_stmt_block(then_branch)?,
                else_block: self.lower_stmt_block(else_branch.as_deref().unwrap_or(&[]))?,
                span,
            }),
            Stmt::While { condition, body } => Some(KernelStmt::While {
                condition: self.lower_expr(*condition)?,
                body: self.lower_stmt_block(body)?,
                span,
            }),
            Stmt::Return(value) => Some(KernelStmt::Return {
                value: value.and_then(|expr| self.lower_expr(expr)),
                span,
            }),
            Stmt::Break => Some(KernelStmt::Break { span }),
            Stmt::Continue => Some(KernelStmt::Continue { span }),
            _ => {
                self.parent
                    .errors
                    .push(KernelLowerError::UnsupportedStatement {
                        function: self.function.name.clone(),
                        kind: stmt_kind(stmt),
                        span,
                    });
                None
            }
        }
    }

    fn lower_expr(&mut self, expr_id: Idx<Expr>) -> Option<KernelExpr> {
        let expr = &self.body.exprs[expr_id];
        let span = self.body.expr_span(expr_id);
        if let Some(target) = self.parse_capture_builtin(expr_id) {
            return Some(KernelExpr::Capture {
                target,
                ty: self.expr_type(expr_id, span)?,
                span,
            });
        }
        if let Some(backend) = self.parse_dispatch_backend_builtin(expr_id) {
            return Some(KernelExpr::DispatchBackend {
                backend,
                ty: self.expr_type(expr_id, span)?,
                span,
            });
        }
        if let Some((plan, args)) = self.parse_field_query(expr_id) {
            return Some(KernelExpr::CaptureQuery {
                plan,
                args,
                ty: self.expr_type(expr_id, span)?,
                span,
            });
        }
        if let Some((plan, args)) = self.parse_shape_query(expr_id) {
            return Some(KernelExpr::CaptureQuery {
                plan,
                args,
                ty: self.expr_type(expr_id, span)?,
                span,
            });
        }
        if let Some((plan, args)) = self.parse_world_point_query(expr_id) {
            return Some(KernelExpr::WorldQuery {
                plan,
                args,
                ty: self.expr_type(expr_id, span)?,
                span,
            });
        }
        if let Some((plan, args)) = self.parse_world_shape_query(expr_id) {
            return Some(KernelExpr::WorldQuery {
                plan,
                args,
                ty: self.expr_type(expr_id, span)?,
                span,
            });
        }
        if let Some((plan, args)) = self.parse_field_batch_query(expr_id) {
            return Some(KernelExpr::BatchQuery {
                plan,
                args,
                ty: self.expr_type(expr_id, span)?,
                span,
            });
        }
        if let Some((plan, args)) = self.parse_shape_batch_query(expr_id) {
            return Some(KernelExpr::BatchQuery {
                plan,
                args,
                ty: self.expr_type(expr_id, span)?,
                span,
            });
        }

        match expr {
            Expr::Literal(literal) => Some(KernelExpr::Literal {
                value: literal.clone(),
                ty: self.expr_type(expr_id, span)?,
                span,
            }),
            Expr::Variable(name) => Some(KernelExpr::Var {
                name: name.clone(),
                ty: self.expr_type(expr_id, span)?,
                span,
            }),
            Expr::Unary { op, expr, .. } => Some(KernelExpr::Unary {
                op: *op,
                expr: Box::new(self.lower_expr(*expr)?),
                ty: self.expr_type(expr_id, span)?,
                span,
            }),
            Expr::Binary { lhs, op, rhs, .. } => Some(KernelExpr::Binary {
                op: *op,
                lhs: Box::new(self.lower_expr(*lhs)?),
                rhs: Box::new(self.lower_expr(*rhs)?),
                ty: self.expr_type(expr_id, span)?,
                span,
            }),
            Expr::Crash { expr } => Some(KernelExpr::Crash {
                expr: Box::new(self.lower_expr(*expr)?),
                ty: self.expr_type(expr_id, span)?,
                span,
            }),
            Expr::TypeApply { callee, .. } => self.lower_expr(*callee),
            Expr::Call { callee, args, .. } => self.lower_call(expr_id, *callee, args, span),
            Expr::Member { object, member, .. } => Some(KernelExpr::Member {
                base: Box::new(self.lower_expr(*object)?),
                member: member.clone(),
                ty: self.expr_type(expr_id, span)?,
                span,
            }),
            Expr::Index { object, index, .. } => Some(KernelExpr::Index {
                base: Box::new(self.lower_expr(*object)?),
                index: Box::new(self.lower_expr(*index)?),
                ty: self.expr_type(expr_id, span)?,
                span,
            }),
            Expr::List(items) => Some(KernelExpr::ArrayLiteral {
                items: items
                    .iter()
                    .map(|item| self.lower_expr(*item))
                    .collect::<Option<Vec<_>>>()?,
                ty: self.expr_type(expr_id, span)?,
                span,
            }),
            _ => {
                self.parent
                    .errors
                    .push(KernelLowerError::UnsupportedExpression {
                        function: self.function.name.clone(),
                        kind: expr_kind(expr),
                        span,
                    });
                None
            }
        }
    }

    fn lower_call(
        &mut self,
        expr_id: Idx<Expr>,
        callee_id: Idx<Expr>,
        args: &[Arg],
        span: TextRange,
    ) -> Option<KernelExpr> {
        let Expr::Variable(callee_name) = &self.body.exprs[callee_id] else {
            self.parent
                .errors
                .push(KernelLowerError::UnknownCallTarget {
                    function: self.function.name.clone(),
                    callee: SmolStr::new("<dynamic>"),
                    span,
                });
            return None;
        };
        let ty = self.expr_type(expr_id, span)?;

        if let Some(record) = portable::builtin_record(callee_name.as_str())
            .or_else(|| portable::builtin_record_by_function(callee_name.as_str()))
        {
            let field_names = record
                .fields
                .iter()
                .map(|field| SmolStr::new(field.name))
                .collect::<Vec<_>>();
            let lowered_args =
                self.lower_ordered_args(callee_name, args, Some(&field_names), span)?;
            let fields = record
                .fields
                .iter()
                .map(|field| SmolStr::new(field.name))
                .zip(lowered_args)
                .collect::<Vec<_>>();
            return Some(KernelExpr::StructLiteral {
                name: SmolStr::new(record.name),
                fields,
                ty,
                span,
            });
        }

        if let Some(field_names) = self.value_class_fields(callee_name) {
            let lowered_args =
                self.lower_ordered_args(callee_name, args, Some(&field_names), span)?;
            let fields = field_names
                .into_iter()
                .zip(lowered_args)
                .collect::<Vec<_>>();
            return Some(KernelExpr::StructLiteral {
                name: callee_name.clone(),
                fields,
                ty,
                span,
            });
        }

        if let Some(function_idx) = self
            .parent
            .context
            .functions_by_name
            .get(callee_name)
            .copied()
        {
            let callee = &self.parent.context.module.functions[function_idx];
            if callee.lane() != FunctionLane::Portable && callee.role != FunctionRole::Domain {
                self.parent
                    .errors
                    .push(KernelLowerError::NonPortableCallee {
                        caller: self.function.name.clone(),
                        callee: callee_name.clone(),
                        span,
                    });
                return None;
            }
            let param_names = callee
                .params
                .iter()
                .map(|param| param.name.clone())
                .collect::<Vec<_>>();
            let lowered_args =
                self.lower_ordered_args(callee_name, args, Some(&param_names), span)?;
            self.callees.insert(function_idx);
            return Some(KernelExpr::Call {
                target: callee_name.clone(),
                args: lowered_args,
                ty,
                span,
            });
        }

        let param_names = builtin_param_names(callee_name.as_str());
        if param_names.is_some() || args.iter().all(|arg| matches!(arg, Arg::Positional { .. })) {
            let lowered_args =
                self.lower_ordered_args(callee_name, args, param_names.as_deref(), span)?;
            return Some(KernelExpr::Call {
                target: callee_name.clone(),
                args: lowered_args,
                ty,
                span,
            });
        }

        self.parent
            .errors
            .push(KernelLowerError::UnknownCallTarget {
                function: self.function.name.clone(),
                callee: callee_name.clone(),
                span,
            });
        None
    }

    fn lower_ordered_args(
        &mut self,
        callee_name: &SmolStr,
        args: &[Arg],
        params: Option<&[SmolStr]>,
        span: TextRange,
    ) -> Option<Vec<KernelExpr>> {
        let Some(params) = params else {
            if args.iter().any(|arg| matches!(arg, Arg::Named { .. })) {
                let arg_name = args
                    .iter()
                    .find_map(|arg| match arg {
                        Arg::Named { name, .. } => Some(name.clone()),
                        _ => None,
                    })
                    .unwrap_or_else(|| SmolStr::new("<named-arg>"));
                self.parent
                    .errors
                    .push(KernelLowerError::UnknownNamedArgument {
                        function: self.function.name.clone(),
                        callee: callee_name.clone(),
                        arg: arg_name,
                        span,
                    });
                return None;
            }
            return args
                .iter()
                .map(|arg| match arg {
                    Arg::Positional { value, .. } => self.lower_expr(*value),
                    Arg::Named { .. } => None,
                })
                .collect();
        };

        let mut ordered = vec![None; params.len()];
        let mut next_positional = 0usize;
        for arg in args {
            match arg {
                Arg::Positional { value, .. } => {
                    if next_positional >= ordered.len() {
                        self.parent
                            .errors
                            .push(KernelLowerError::UnknownNamedArgument {
                                function: self.function.name.clone(),
                                callee: callee_name.clone(),
                                arg: SmolStr::new("<extra-arg>"),
                                span,
                            });
                        return None;
                    }
                    ordered[next_positional] = self.lower_expr(*value);
                    next_positional += 1;
                }
                Arg::Named {
                    name, value, span, ..
                } => {
                    let Some(index) = params.iter().position(|param| param == name) else {
                        self.parent
                            .errors
                            .push(KernelLowerError::UnknownNamedArgument {
                                function: self.function.name.clone(),
                                callee: callee_name.clone(),
                                arg: name.clone(),
                                span: *span,
                            });
                        return None;
                    };
                    if ordered[index].is_some() {
                        self.parent
                            .errors
                            .push(KernelLowerError::DuplicateNamedArgument {
                                function: self.function.name.clone(),
                                callee: callee_name.clone(),
                                arg: name.clone(),
                                span: *span,
                            });
                        return None;
                    }
                    ordered[index] = self.lower_expr(*value);
                }
            }
        }

        let mut out = Vec::with_capacity(ordered.len());
        for (index, arg) in ordered.into_iter().enumerate() {
            match arg {
                Some(value) => out.push(value),
                None => {
                    self.parent.errors.push(KernelLowerError::MissingArgument {
                        function: self.function.name.clone(),
                        callee: callee_name.clone(),
                        param: params[index].clone(),
                        span,
                    });
                    return None;
                }
            }
        }
        Some(out)
    }

    fn parse_capture_builtin(&mut self, expr_id: Idx<Expr>) -> Option<SmolStr> {
        let (callee, args) = match &self.body.exprs[expr_id] {
            Expr::Call { callee, args, .. } => (callee, args),
            _ => return None,
        };
        let Expr::Variable(name) = &self.body.exprs[*callee] else {
            return None;
        };
        if name.as_str() != "capture" {
            return None;
        }
        let mut positional_target = None;
        for arg in args {
            match arg {
                Arg::Named { name, value, .. } if name.as_str() == "scene" => {
                    let Expr::Variable(target) = &self.body.exprs[*value] else {
                        return None;
                    };
                    if self.parent.context.shape_names.contains(target)
                        || self.parent.context.field_names.contains(target)
                        || matches!(
                            self.expr_type(expr_id, TextRange::empty(0.into())),
                            Some(Type::Named(name, _)) if name.as_str() == "RegionCapture"
                        )
                    {
                        return Some(target.clone());
                    }
                    return None;
                }
                Arg::Positional { value, .. } => positional_target = Some(*value),
                _ => {}
            }
        }
        let value = positional_target?;
        let Expr::Variable(target) = &self.body.exprs[value] else {
            return None;
        };
        if self.parent.context.shape_names.contains(target)
            || self.parent.context.field_names.contains(target)
            || matches!(
                self.expr_type(expr_id, TextRange::empty(0.into())),
                Some(Type::Named(name, _)) if name.as_str() == "RegionCapture"
            )
        {
            Some(target.clone())
        } else {
            None
        }
    }

    fn parse_dispatch_backend_builtin(&self, expr_id: Idx<Expr>) -> Option<DispatchBackend> {
        let (callee, args) = match &self.body.exprs[expr_id] {
            Expr::Call { callee, args, .. } => (callee, args),
            _ => return None,
        };
        if !args.is_empty() {
            return None;
        }
        let Expr::Variable(name) = &self.body.exprs[*callee] else {
            return None;
        };
        match name.as_str() {
            "dispatch_backend_cpu" => Some(DispatchBackend::Cpu),
            "dispatch_backend_virtual_gpu" => Some(DispatchBackend::VirtualGpu),
            "dispatch_backend_auto" => Some(DispatchBackend::Auto),
            _ => None,
        }
    }

    fn parse_field_query(
        &mut self,
        expr_id: Idx<Expr>,
    ) -> Option<(KernelCaptureQueryPlan, Vec<KernelExpr>)> {
        let span = self.body.expr_span(expr_id);
        let (callee, args) = match &self.body.exprs[expr_id] {
            Expr::Call { callee, args, .. } => (callee, args),
            _ => return None,
        };
        let Expr::Variable(name) = &self.body.exprs[*callee] else {
            return None;
        };
        let (kind, expects_direction) = match name.as_str() {
            "distance_at" => (CaptureQueryKind::Distance, false),
            "normal_at" => (CaptureQueryKind::Normal, false),
            "radiance_at" => (CaptureQueryKind::Radiance, true),
            "medium_at" => (CaptureQueryKind::Medium, false),
            _ => return None,
        };
        let named = self.collect_named_expr_args(args)?;
        let capture = named.get("capture").copied()?;
        let point = named.get("point").copied()?;
        let capture_kind = self.capture_kind_for_expr(capture);
        let scene = self.batch_capture_scene_summary(capture, capture_kind);
        let plan = lower_capture_query_plan(
            &CaptureQueryPlan::for_query(kind, capture_kind, scene)
                .expect("kernel capture query plan"),
        );
        let mut ordered_args = vec![self.lower_expr(capture)?, self.lower_expr(point)?];
        if expects_direction {
            ordered_args.push(self.lower_expr(*named.get("direction")?)?);
        }
        if ordered_args.iter().any(|arg| {
            arg.span() == TextRange::empty(0.into()) && span == TextRange::empty(0.into())
        }) {
            return None;
        }
        Some((plan, ordered_args))
    }

    fn parse_shape_query(
        &mut self,
        expr_id: Idx<Expr>,
    ) -> Option<(KernelCaptureQueryPlan, Vec<KernelExpr>)> {
        let (callee, args) = match &self.body.exprs[expr_id] {
            Expr::Call { callee, args, .. } => (callee, args),
            _ => return None,
        };
        let Expr::Variable(name) = &self.body.exprs[*callee] else {
            return None;
        };
        let kind = match name.as_str() {
            "trace_shape" => CaptureQueryKind::Trace,
            "surface_at" => CaptureQueryKind::Surface,
            _ => return None,
        };
        let named = self.collect_named_expr_args(args)?;
        let capture = named.get("capture").copied()?;
        let scene = self.batch_capture_scene_summary(capture, CaptureKind::Shape);
        let plan = lower_capture_query_plan(
            &CaptureQueryPlan::for_query(kind, CaptureKind::Shape, scene)
                .expect("kernel shape query plan"),
        );
        let ordered_args = match kind {
            CaptureQueryKind::Trace => vec![
                self.lower_expr(capture)?,
                self.lower_expr(*named.get("origin")?)?,
                self.lower_expr(*named.get("direction")?)?,
                self.lower_expr(*named.get("max_distance")?)?,
                self.lower_expr(*named.get("min_step")?)?,
                self.lower_expr(*named.get("hit_epsilon")?)?,
                self.lower_expr(*named.get("max_steps")?)?,
            ],
            CaptureQueryKind::Surface => vec![
                self.lower_expr(capture)?,
                self.lower_expr(*named.get("hit")?)?,
            ],
            _ => return None,
        };
        Some((plan, ordered_args))
    }

    fn parse_world_point_query(
        &mut self,
        expr_id: Idx<Expr>,
    ) -> Option<(KernelWorldQueryPlan, Vec<KernelExpr>)> {
        let (callee, args) = match &self.body.exprs[expr_id] {
            Expr::Call { callee, args, .. } => (callee, args),
            _ => return None,
        };
        let Expr::Variable(name) = &self.body.exprs[*callee] else {
            return None;
        };
        let (kind, expects_direction) = match name.as_str() {
            "distance_world" => (WorldQueryKind::Distance, false),
            "normal_world" => (WorldQueryKind::Normal, false),
            "radiance_world" => (WorldQueryKind::Radiance, true),
            "medium_world" => (WorldQueryKind::Medium, false),
            _ => return None,
        };
        let named = self.collect_named_expr_args(args)?;
        let plan = lower_world_query_plan(&WorldQueryPlan::for_query(kind));
        let mut ordered_args = vec![
            self.lower_expr(*named.get("capture")?)?,
            self.lower_expr(*named.get("domain")?)?,
            self.lower_expr(*named.get("point")?)?,
        ];
        if expects_direction {
            ordered_args.push(self.lower_expr(*named.get("direction")?)?);
        }
        Some((plan, ordered_args))
    }

    fn parse_world_shape_query(
        &mut self,
        expr_id: Idx<Expr>,
    ) -> Option<(KernelWorldQueryPlan, Vec<KernelExpr>)> {
        let (callee, args) = match &self.body.exprs[expr_id] {
            Expr::Call { callee, args, .. } => (callee, args),
            _ => return None,
        };
        let Expr::Variable(name) = &self.body.exprs[*callee] else {
            return None;
        };
        let kind = match name.as_str() {
            "trace_world" => WorldQueryKind::Trace,
            "surface_world" => WorldQueryKind::Surface,
            _ => return None,
        };
        let named = self.collect_named_expr_args(args)?;
        let plan = lower_world_query_plan(&WorldQueryPlan::for_query(kind));
        let ordered_args = match kind {
            WorldQueryKind::Trace => vec![
                self.lower_expr(*named.get("capture")?)?,
                self.lower_expr(*named.get("domain")?)?,
                self.lower_expr(*named.get("origin")?)?,
                self.lower_expr(*named.get("direction")?)?,
                self.lower_expr(*named.get("max_distance")?)?,
                self.lower_expr(*named.get("min_step")?)?,
                self.lower_expr(*named.get("hit_epsilon")?)?,
                self.lower_expr(*named.get("max_steps")?)?,
            ],
            WorldQueryKind::Surface => vec![
                self.lower_expr(*named.get("capture")?)?,
                self.lower_expr(*named.get("domain")?)?,
                self.lower_expr(*named.get("hit")?)?,
            ],
            _ => return None,
        };
        Some((plan, ordered_args))
    }

    fn parse_shape_batch_query(
        &mut self,
        expr_id: Idx<Expr>,
    ) -> Option<(KernelBatchQueryPlan, Vec<KernelExpr>)> {
        let (callee, args) = match &self.body.exprs[expr_id] {
            Expr::Call { callee, args, .. } => (callee, args),
            _ => return None,
        };
        let Expr::Variable(name) = &self.body.exprs[*callee] else {
            return None;
        };
        let kind = match name.as_str() {
            "trace_shape_batch" => BatchQueryKind::Trace,
            "surface_at_batch" => BatchQueryKind::Surface,
            "occluded_batch" => BatchQueryKind::Occluded,
            _ => return None,
        };
        let named = self.collect_named_expr_args(args)?;
        let capture = *named.get("capture")?;
        let items_key = match kind {
            BatchQueryKind::Surface => "hits",
            _ => "rays",
        };
        let backend_expr = *named.get("backend")?;
        let backend = self
            .parse_dispatch_backend_builtin(backend_expr)
            .unwrap_or(DispatchBackend::Auto);
        let scene = self.batch_capture_scene_summary(capture, CaptureKind::Shape);
        let plan = lower_batch_query_plan(&BatchQueryPlan::for_shape_query(kind, backend, scene));
        let ordered_args = vec![
            self.lower_expr(capture)?,
            self.lower_expr(*named.get(items_key)?)?,
            self.lower_expr(backend_expr)?,
        ];
        Some((plan, ordered_args))
    }

    fn parse_field_batch_query(
        &mut self,
        expr_id: Idx<Expr>,
    ) -> Option<(KernelBatchQueryPlan, Vec<KernelExpr>)> {
        let (callee, args) = match &self.body.exprs[expr_id] {
            Expr::Call { callee, args, .. } => (callee, args),
            _ => return None,
        };
        let Expr::Variable(name) = &self.body.exprs[*callee] else {
            return None;
        };
        let kind = match name.as_str() {
            "distance_at_batch" => BatchQueryKind::Distance,
            "normal_at_batch" => BatchQueryKind::Normal,
            _ => return None,
        };
        let named = self.collect_named_expr_args(args)?;
        let capture = *named.get("capture")?;
        let backend_expr = *named.get("backend")?;
        let backend = self
            .parse_dispatch_backend_builtin(backend_expr)
            .unwrap_or(DispatchBackend::Auto);
        let capture_kind = self.capture_kind_for_expr(capture);
        let scene = self.batch_capture_scene_summary(capture, capture_kind);
        let plan = lower_batch_query_plan(&BatchQueryPlan::for_field_query(
            kind,
            capture_kind,
            backend,
            scene,
        ));
        let ordered_args = vec![
            self.lower_expr(capture)?,
            self.lower_expr(*named.get("points")?)?,
            self.lower_expr(backend_expr)?,
        ];
        Some((plan, ordered_args))
    }

    fn collect_named_expr_args(&self, args: &[Arg]) -> Option<HashMap<SmolStr, Idx<Expr>>> {
        let mut named = HashMap::new();
        for arg in args {
            let Arg::Named { name, value, .. } = arg else {
                return None;
            };
            named.insert(name.clone(), *value);
        }
        Some(named)
    }

    fn capture_kind_for_expr(&mut self, expr_id: Idx<Expr>) -> CaptureKind {
        match self
            .expr_type(expr_id, TextRange::empty(0.into()))
            .unwrap_or(Type::Unknown)
        {
            Type::Named(name, _) if name.as_str() == "ShapeCapture" => CaptureKind::Shape,
            Type::Named(name, _) if name.as_str() == "RegionCapture" => CaptureKind::Region,
            _ => CaptureKind::Field,
        }
    }

    fn batch_capture_scene_summary(
        &mut self,
        capture_expr: Idx<Expr>,
        capture_kind: CaptureKind,
    ) -> Option<SceneSummary> {
        let target = self.parse_capture_builtin(capture_expr)?;
        match capture_kind {
            CaptureKind::Field => {
                self.parent
                    .context
                    .field_scenes
                    .get(&target)
                    .map(|scene| SceneSummary {
                        name: Some(target),
                        semantics: scene.semantics,
                        support_class: scene.support_class,
                        can_coarse_support_pruning: scene.can_coarse_support_pruning,
                        opaque_boundary: scene.opaque_boundary,
                    })
            }
            CaptureKind::Shape => {
                self.parent
                    .context
                    .shape_scenes
                    .get(&target)
                    .map(|scene| SceneSummary {
                        name: Some(target),
                        semantics: scene.semantics,
                        support_class: scene.support_class,
                        can_coarse_support_pruning: scene.can_coarse_support_pruning,
                        opaque_boundary: scene.opaque_boundary,
                    })
            }
            CaptureKind::Region => None,
        }
    }

    fn expr_type(&mut self, expr_id: Idx<Expr>, span: TextRange) -> Option<Type> {
        let Some(ty) = self.fn_info.expr_type(self.body, expr_id) else {
            self.parent.errors.push(KernelLowerError::MissingExprType {
                function: self.function.name.clone(),
                span,
            });
            return None;
        };
        Some(ty.clone())
    }

    fn local_type(&mut self, name: &SmolStr, span: TextRange) -> Option<Type> {
        let Some(ty) = self.fn_info.local_types.get(name) else {
            self.parent.errors.push(KernelLowerError::UnknownLocal {
                function: self.function.name.clone(),
                name: name.clone(),
                span,
            });
            return None;
        };
        Some(ty.clone())
    }

    fn lower_type_ref(&self, ty: Option<&hir::TypeRef>) -> Option<Type> {
        let ty = ty?;
        Some(match ty.name.as_str() {
            "Nothing" => Type::Nil,
            "Bool" | "Boolean" => Type::Boolean,
            "Integer" => Type::I32,
            "I32" => Type::I32,
            "U32" => Type::U32,
            "Float" | "F32" => Type::F32,
            "Vec2" => Type::Vec2,
            "Vec3" => Type::Vec3,
            "Vec4" => Type::Vec4,
            "Mat3" => Type::Mat3,
            "Mat4" => Type::Mat4,
            "Quat" => Type::Quat,
            "Array" => match ty.args.as_slice() {
                [inner, len] => {
                    let len = len.name.parse::<usize>().ok()?;
                    Type::Array(Box::new(self.lower_type_ref(Some(inner))?), len)
                }
                _ => return None,
            },
            name => Type::Named(SmolStr::new(name), Vec::new()),
        })
    }

    fn value_class_fields(&self, name: &SmolStr) -> Option<Vec<SmolStr>> {
        self.parent
            .context
            .module
            .classes
            .iter()
            .find_map(|(_, class)| {
                (class.name == *name && class.role == hir::ClassRole::Value).then(|| {
                    class
                        .fields
                        .iter()
                        .map(|field| field.name.clone())
                        .collect::<Vec<_>>()
                })
            })
    }
}

fn top_level_function_indices(module: &Module) -> HashMap<SmolStr, Idx<Function>> {
    let mut method_ids = HashSet::new();
    for (_idx, class) in module.classes.iter() {
        for method_id in &class.methods {
            method_ids.insert(*method_id);
        }
    }
    let mut out = HashMap::new();
    for (idx, function) in module.functions.iter() {
        if method_ids.contains(&idx) {
            continue;
        }
        out.insert(function.name.clone(), idx);
    }
    out
}

fn builtin_param_names(name: &str) -> Option<Vec<SmolStr>> {
    let names: &[&str] = match name {
        "i32" | "u32" | "f32" | "abs" | "sign" | "floor" | "ceil" | "fract" | "sin" | "cos"
        | "sqrt" | "length" | "normalize" => &["value"],
        "vec2" => &["x", "y"],
        "vec3" => &["x", "y", "z"],
        "vec4" | "quat" => &["x", "y", "z", "w"],
        "mat3_identity" | "mat4_identity" | "transform3_identity" => &[],
        "mat3_cols" => &["c0", "c1", "c2"],
        "mat4_cols" => &["c0", "c1", "c2", "c3"],
        "bounds2_center" | "bounds2_size" | "bounds3_center" | "bounds3_size" => &["bounds"],
        "transform_point" => &["transform", "point"],
        "transform_vector" => &["transform", "vector"],
        "transform_normal" => &["transform", "normal"],
        "compose_transform3" => &["left", "right"],
        "inverse_transform3" => &["transform"],
        "translate" => &["translate", "point"],
        "rotate" => &["rotate", "point"],
        "uniform_scale" => &["scale", "point"],
        "affine_transform" => &["transform", "point"],
        "warp" => &["warp", "point"],
        "repeat_linear" => &["repeat", "point"],
        "repeat_grid" => &["repeat", "point"],
        "radial_repeat" => &["radial", "point"],
        "mirror_array" => &["mirror", "point"],
        "instance_array" => &["instance", "point"],
        "field_rotate_point" => &["rotation", "point"],
        "field_transform_point" => &["transform", "point"],
        "field_instance_point" => &["instance", "point"],
        "field_mirror_point" => &["mirror", "point"],
        "field_repeat_point" => &["period", "point"],
        "field_sweep_coords" => &["path", "point"],
        "rounded_box" => &["p", "half", "radius"],
        "circle2" => &["p", "radius"],
        "rect2" => &["p", "half"],
        "rounded_rect2" => &["p", "half", "radius"],
        "capsule2" | "capsule" => &["p", "a", "b", "radius"],
        "segment2" => &["p", "a", "b"],
        "polygon2" | "polyline2" => &["p", "vertices"],
        "ellipsoid" => &["p", "radii"],
        "cone" | "cylinder" => &["p", "radius", "half_height"],
        "capped_cone" => &["p", "radius_bottom", "radius_top", "half_height"],
        "box_frame" => &["p", "half", "thickness"],
        "slab" => &["p", "thickness"],
        "triangle_prism" | "hex_prism" => &["p", "half", "half_height"],
        "sphere" => &["p", "radius"],
        "box" => &["p", "half"],
        "plane" => &["p", "normal", "offset"],
        "torus" => &["p", "major_radius", "minor_radius"],
        "smooth_union"
        | "smooth_intersection"
        | "smooth_subtract"
        | "field_smooth_union"
        | "field_smooth_intersection"
        | "field_smooth_subtract" => &["left", "right", "k"],
        "field_union" | "field_intersection" | "field_subtract" | "dot" | "cross" | "min"
        | "max" | "pow" | "distance" | "reflect" => &["left", "right"],
        "clamp" => &["value", "min", "max"],
        "mix" => &["value", "other", "t"],
        "gpu_buffer_len" => &["buffer"],
        "gpu_buffer_get" => &["buffer", "index"],
        "gpu_buffer_set" => &["buffer", "index", "value"],
        "gpu_atomic_i32_new" | "gpu_atomic_u32_new" => &["initial"],
        "gpu_atomic_i32_drop"
        | "gpu_atomic_i32_load"
        | "gpu_atomic_u32_drop"
        | "gpu_atomic_u32_load" => &["atomic"],
        "gpu_atomic_i32_store" | "gpu_atomic_u32_store" => &["atomic", "value"],
        "gpu_atomic_i32_fetch_add" | "gpu_atomic_u32_fetch_add" => &["atomic", "delta"],
        "global_invocation_id"
        | "local_invocation_id"
        | "workgroup_id"
        | "num_workgroups"
        | "workgroup_size"
        | "workgroup_barrier"
        | "storage_barrier"
        | "dispatch_backend_cpu"
        | "dispatch_backend_virtual_gpu"
        | "dispatch_backend_auto" => &[],
        _ => return None,
    };
    Some(names.iter().map(|name| SmolStr::new(*name)).collect())
}

fn stmt_kind(stmt: &Stmt) -> &'static str {
    match stmt {
        Stmt::Expr(_) => "expr",
        Stmt::Assert { .. } => "assert",
        Stmt::Require { .. } => "require",
        Stmt::Let { .. } => "let",
        Stmt::Assign { .. } => "assign",
        Stmt::Optimize { .. } => "optimize",
        Stmt::If { .. } => "if",
        Stmt::For { .. } => "for",
        Stmt::Match { .. } => "match",
        Stmt::IgnoreResult { .. } => "ignore-result",
        Stmt::Capture { .. } => "capture",
        Stmt::Defer { .. } => "defer",
        Stmt::Use { .. } => "use",
        Stmt::While { .. } => "while",
        Stmt::Return(_) => "return",
        Stmt::Break => "break",
        Stmt::Continue => "continue",
    }
}

fn expr_kind(expr: &Expr) -> &'static str {
    match expr {
        Expr::Literal(_) => "literal",
        Expr::Variable(_) => "variable",
        Expr::Detach { .. } => "detach",
        Expr::Binary { .. } => "binary",
        Expr::Unary { .. } => "unary",
        Expr::TypeApply { .. } => "type-apply",
        Expr::Crash { .. } => "crash",
        Expr::Call { .. } => "call",
        Expr::Member { .. } => "member",
        Expr::Index { .. } => "index",
        Expr::List(_) => "list",
        Expr::Map(_) => "map",
        Expr::StringInterp(_) => "string-interp",
        Expr::Closure { .. } => "closure",
    }
}
