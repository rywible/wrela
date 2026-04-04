use super::ir::{
    PirBlock, PirCallTarget, PirExpr, PirFunction, PirIntrinsic, PirModule, PirParam, PirStmt,
    PirStructField, PirStructType, PirType, PirValue,
};
use crate::hir::typeck::{FunctionTypeInfo, Type, TypeInfo};
use crate::hir::{
    Arg, AssignOp, BinaryOp, Body, ClassRole, Expr, Function, FunctionLane, Idx, Literal, Module,
    Stmt, TypeRef,
};
use crate::portable;
use rowan::TextRange;
use smol_str::SmolStr;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq)]
pub enum PirLowerError {
    #[error("portable entry '{name}' was not found")]
    MissingEntry { name: SmolStr },
    #[error("portable entry '{name}' must be declared in the portable lane")]
    EntryNotPortable { name: SmolStr },
    #[error(
        "portable function '{caller}' calls host-lane function '{callee}'; portable lowering only accepts portable call graphs"
    )]
    NonPortableCallee {
        caller: SmolStr,
        callee: SmolStr,
        span: TextRange,
    },
    #[error("missing type information for function '{name}'")]
    MissingTypeInfo { name: SmolStr },
    #[error("function '{name}' does not have a body")]
    MissingBody { name: SmolStr },
    #[error("non-portable type '{ty}' in {context}")]
    NonPortableType { context: String, ty: String },
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
    #[error("unsupported call target in function '{function}'")]
    UnsupportedCallTarget { function: SmolStr, span: TextRange },
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

pub fn lower_portable_entry_by_name(
    module: &Module,
    type_info: &TypeInfo,
    entry: &str,
) -> Result<PirModule, Vec<PirLowerError>> {
    let portable_functions = portable_top_level_function_indices(module);
    let entry_name = SmolStr::new(entry);
    let Some(entry_idx) = portable_functions.get(entry).copied() else {
        if let Some(host_idx) = top_level_function_indices(module).get(&entry_name).copied() {
            let function = &module.functions[host_idx];
            return Err(vec![PirLowerError::EntryNotPortable {
                name: function.name.clone(),
            }]);
        }
        return Err(vec![PirLowerError::MissingEntry { name: entry_name }]);
    };
    lower_portable_function(module, type_info, entry_idx)
}

pub fn lower_portable_function(
    module: &Module,
    type_info: &TypeInfo,
    entry: Idx<Function>,
) -> Result<PirModule, Vec<PirLowerError>> {
    let entry_function = &module.functions[entry];
    if entry_function.lane() != FunctionLane::Portable {
        return Err(vec![PirLowerError::EntryNotPortable {
            name: entry_function.name.clone(),
        }]);
    }
    let value_types = match collect_value_types(module) {
        Ok(value_types) => value_types,
        Err(errors) => return Err(errors),
    };
    let functions_by_name = portable_top_level_function_indices(module);
    let mut lowerer = PortableLowerer {
        module,
        type_info,
        value_types,
        functions_by_name,
        lowered: BTreeMap::new(),
        errors: Vec::new(),
    };
    lowerer.lower_function_recursive(entry);
    if lowerer.errors.is_empty() {
        Ok(PirModule {
            entry: module.functions[entry].name.clone(),
            functions: lowerer.lowered.into_values().collect(),
        })
    } else {
        Err(lowerer.errors)
    }
}

fn collect_value_types(
    module: &Module,
) -> Result<HashMap<SmolStr, PirStructType>, Vec<PirLowerError>> {
    ValueTypeResolver::new(module).collect_all()
}

fn portable_top_level_function_indices(module: &Module) -> HashMap<SmolStr, Idx<Function>> {
    top_level_function_indices(module)
        .into_iter()
        .filter(|(_, idx)| module.functions[*idx].lane() == FunctionLane::Portable)
        .collect()
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

struct PortableLowerer<'a> {
    module: &'a Module,
    type_info: &'a TypeInfo,
    value_types: HashMap<SmolStr, PirStructType>,
    functions_by_name: HashMap<SmolStr, Idx<Function>>,
    lowered: BTreeMap<usize, PirFunction>,
    errors: Vec<PirLowerError>,
}

struct ValueTypeResolver<'a> {
    module: &'a Module,
    cache: HashMap<SmolStr, PirStructType>,
    visiting: HashSet<SmolStr>,
    errors: Vec<PirLowerError>,
}

impl<'a> ValueTypeResolver<'a> {
    fn new(module: &'a Module) -> Self {
        Self {
            module,
            cache: HashMap::new(),
            visiting: HashSet::new(),
            errors: Vec::new(),
        }
    }

    fn collect_all(mut self) -> Result<HashMap<SmolStr, PirStructType>, Vec<PirLowerError>> {
        for record in portable::builtin_records() {
            let _ = self.ensure_named(record.name);
        }
        for (_idx, class) in self.module.classes.iter() {
            if class.role == ClassRole::Value {
                let _ = self.ensure_named(&class.name);
            }
        }
        if self.errors.is_empty() {
            Ok(self.cache)
        } else {
            Err(self.errors)
        }
    }

    fn ensure_named(&mut self, name: &str) -> Option<PirStructType> {
        let name = SmolStr::new(name);
        if let Some(layout) = self.cache.get(&name).cloned() {
            return Some(layout);
        }
        if !self.visiting.insert(name.clone()) {
            self.errors.push(PirLowerError::NonPortableType {
                context: "recursive value type".to_string(),
                ty: name.to_string(),
            });
            return None;
        }

        let resolved = if let Some(record) = portable::builtin_record(name.as_str()) {
            self.build_builtin_record(record)
        } else if let Some(class) = self.module.classes.iter().find_map(|(_, class)| {
            (class.name == name && class.role == ClassRole::Value).then_some(class)
        }) {
            self.build_user_value_class(class)
        } else {
            self.errors.push(PirLowerError::NonPortableType {
                context: "type reference".to_string(),
                ty: name.to_string(),
            });
            None
        };

        self.visiting.remove(&name);
        if let Some(layout) = resolved {
            self.cache.insert(name.clone(), layout.clone());
            Some(layout)
        } else {
            None
        }
    }

    fn build_builtin_record(
        &mut self,
        record: &portable::PortableBuiltinRecord,
    ) -> Option<PirStructType> {
        let mut fields = Vec::with_capacity(record.fields.len());
        for field in record.fields {
            let ty = self.lower_portable_builtin_type(field.ty)?;
            fields.push(PirStructField {
                name: SmolStr::new(field.name),
                ty,
            });
        }
        Some(PirStructType {
            name: SmolStr::new(record.name),
            fields,
        })
    }

    fn build_user_value_class(&mut self, class: &crate::hir::Class) -> Option<PirStructType> {
        let mut fields = Vec::with_capacity(class.fields.len());
        for field in &class.fields {
            let Some(field_ty) = field.ty.as_ref() else {
                self.errors.push(PirLowerError::NonPortableType {
                    context: format!("field '{}.{}'", class.name, field.name),
                    ty: "<missing>".to_string(),
                });
                return None;
            };
            let ty = self.lower_type_ref(field_ty)?;
            fields.push(PirStructField {
                name: field.name.clone(),
                ty,
            });
        }
        Some(PirStructType {
            name: class.name.clone(),
            fields,
        })
    }

    fn lower_portable_builtin_type(
        &mut self,
        ty: portable::PortableBuiltinType,
    ) -> Option<PirType> {
        match ty {
            portable::PortableBuiltinType::Atom(atom) => Some(match atom {
                portable::PortableBuiltinAtom::Bool => PirType::Bool,
                portable::PortableBuiltinAtom::I32 => PirType::I32,
                portable::PortableBuiltinAtom::U32 => PirType::U32,
                portable::PortableBuiltinAtom::I64 => PirType::I64,
                portable::PortableBuiltinAtom::U64 => PirType::U64,
                portable::PortableBuiltinAtom::F32 => PirType::F32,
                portable::PortableBuiltinAtom::Vec2 => PirType::Vec2,
                portable::PortableBuiltinAtom::Vec3 => PirType::Vec3,
                portable::PortableBuiltinAtom::Vec4 => PirType::Vec4,
                portable::PortableBuiltinAtom::Mat3 => PirType::Mat3,
                portable::PortableBuiltinAtom::Mat4 => PirType::Mat4,
                portable::PortableBuiltinAtom::Quat => PirType::Quat,
            }),
            portable::PortableBuiltinType::Named(name) => {
                self.ensure_named(name).map(PirType::Struct)
            }
        }
    }

    fn lower_type_ref(&mut self, ty: &TypeRef) -> Option<PirType> {
        match ty.name.as_str() {
            "Nothing" => Some(PirType::Nothing),
            "Bool" | "Boolean" => Some(PirType::Bool),
            "Integer" => Some(PirType::I64),
            "I32" => Some(PirType::I32),
            "U32" => Some(PirType::U32),
            "I64" => Some(PirType::I64),
            "U64" => Some(PirType::U64),
            "Float" | "F32" => Some(PirType::F32),
            "Vec2" => Some(PirType::Vec2),
            "Vec3" => Some(PirType::Vec3),
            "Vec4" => Some(PirType::Vec4),
            "Mat3" => Some(PirType::Mat3),
            "Mat4" => Some(PirType::Mat4),
            "Quat" => Some(PirType::Quat),
            "Array" => match ty.args.as_slice() {
                [inner, len] => {
                    let len = len.name.parse::<usize>().ok()?;
                    let inner = self.lower_type_ref(inner)?;
                    Some(PirType::Array(Box::new(inner), len))
                }
                _ => {
                    self.errors.push(PirLowerError::NonPortableType {
                        context: "type reference".to_string(),
                        ty: "Array".to_string(),
                    });
                    None
                }
            },
            name => self.ensure_named(name).map(PirType::Struct),
        }
    }
}

impl<'a> PortableLowerer<'a> {
    fn lower_function_recursive(&mut self, function_idx: Idx<Function>) {
        if self.lowered.contains_key(&function_idx.into_raw()) {
            return;
        }
        let function = &self.module.functions[function_idx];
        let Some(body) = &function.body else {
            self.errors.push(PirLowerError::MissingBody {
                name: function.name.clone(),
            });
            return;
        };
        let Some(fn_info) = self.type_info.function(function_idx) else {
            self.errors.push(PirLowerError::MissingTypeInfo {
                name: function.name.clone(),
            });
            return;
        };

        let params = function
            .params
            .iter()
            .map(|param| {
                let ty = param
                    .ty
                    .as_ref()
                    .map(|ty| lower_type_ref(ty, &self.value_types))
                    .transpose()
                    .map(|ty| ty.unwrap_or(PirType::Nothing));
                ty.map(|ty| PirParam {
                    name: param.name.clone(),
                    ty,
                })
            })
            .collect::<Result<Vec<_>, _>>();
        let ret = function
            .ret_type
            .as_ref()
            .map(|ty| lower_type_ref(ty, &self.value_types))
            .transpose()
            .map(|ty| ty.unwrap_or(PirType::Nothing));

        let (params, ret) = match (params, ret) {
            (Ok(params), Ok(ret)) => (params, ret),
            (Err(err), _) | (_, Err(err)) => {
                self.errors.push(err);
                return;
            }
        };

        let (pir_body, callees) = {
            let mut fn_lowerer = FunctionLowerer {
                parent: self,
                function,
                fn_info,
                body,
                callees: BTreeSet::new(),
            };
            let Some(pir_body) = fn_lowerer.lower_block(&body.root_stmts) else {
                return;
            };
            (pir_body, fn_lowerer.callees)
        };
        self.lowered.insert(
            function_idx.into_raw(),
            PirFunction {
                name: function.name.clone(),
                params,
                ret,
                body: pir_body,
            },
        );

        for callee in callees {
            self.lower_function_recursive(callee);
        }
    }
}

struct FunctionLowerer<'a, 'b> {
    parent: &'a mut PortableLowerer<'b>,
    function: &'b Function,
    fn_info: &'b FunctionTypeInfo,
    body: &'b Body,
    callees: BTreeSet<Idx<Function>>,
}

impl<'a, 'b> FunctionLowerer<'a, 'b> {
    fn lower_block(&mut self, stmts: &[Idx<Stmt>]) -> Option<PirBlock> {
        let mut out = Vec::with_capacity(stmts.len());
        for stmt_idx in stmts {
            let stmt = &self.body.stmts[*stmt_idx];
            let span = self.body.stmt_span(*stmt_idx);
            match stmt {
                Stmt::Expr(expr) | Stmt::IgnoreResult { expr } => {
                    let value = self.lower_expr(*expr)?;
                    out.push(PirStmt::Expr { value, span });
                }
                Stmt::Let {
                    name,
                    value,
                    mutable,
                    ..
                } => {
                    let Some(ty) = self.local_type(name, span) else {
                        return None;
                    };
                    let value = self.lower_expr(*value)?;
                    out.push(PirStmt::Let {
                        name: name.clone(),
                        mutable: *mutable,
                        ty,
                        value,
                        span,
                    });
                }
                Stmt::Assign {
                    name, op, value, ..
                } => {
                    let lowered = match op {
                        AssignOp::Assign => self.lower_expr(*value)?,
                        AssignOp::AddAssign
                        | AssignOp::SubAssign
                        | AssignOp::MulAssign
                        | AssignOp::DivAssign => {
                            let binary_op = match op {
                                AssignOp::AddAssign => BinaryOp::Add,
                                AssignOp::SubAssign => BinaryOp::Sub,
                                AssignOp::MulAssign => BinaryOp::Mul,
                                AssignOp::DivAssign => BinaryOp::Div,
                                AssignOp::Assign => unreachable!(),
                            };
                            let ty = self.local_type(name, span)?;
                            PirExpr::Binary {
                                op: binary_op,
                                lhs: Box::new(PirExpr::Var {
                                    name: name.clone(),
                                    ty: ty.clone(),
                                }),
                                rhs: Box::new(self.lower_expr(*value)?),
                                ty,
                            }
                        }
                    };
                    out.push(PirStmt::Assign {
                        name: name.clone(),
                        value: lowered,
                        span,
                    });
                }
                Stmt::If {
                    condition,
                    then_branch,
                    else_branch,
                } => {
                    let condition = self.lower_expr(*condition)?;
                    let then_block = self.lower_block(then_branch)?;
                    let else_block = if let Some(else_branch) = else_branch {
                        self.lower_block(else_branch)?
                    } else {
                        Vec::new()
                    };
                    out.push(PirStmt::If {
                        condition,
                        then_block,
                        else_block,
                        span,
                    });
                }
                Stmt::Return(value) => {
                    let value = value.as_ref().and_then(|expr| self.lower_expr(*expr));
                    if value.is_none() && value.is_some() {
                        return None;
                    }
                    out.push(PirStmt::Return { value, span });
                }
                _ => {
                    self.parent
                        .errors
                        .push(PirLowerError::UnsupportedStatement {
                            function: self.function.name.clone(),
                            kind: stmt_kind(stmt),
                            span,
                        });
                    return None;
                }
            }
        }
        Some(out)
    }

    fn lower_expr(&mut self, expr_id: Idx<Expr>) -> Option<PirExpr> {
        let expr = &self.body.exprs[expr_id];
        let span = self.body.expr_span(expr_id);
        match expr {
            Expr::Literal(literal) => {
                let ty = self.expr_type(expr_id, span)?;
                let value =
                    literal_to_value(literal, &ty).map_err(|err| self.parent.errors.push(err));
                value.ok().map(PirExpr::Literal)
            }
            Expr::Variable(name) => {
                let ty = self.expr_type(expr_id, span)?;
                Some(PirExpr::Var {
                    name: name.clone(),
                    ty,
                })
            }
            Expr::Unary { op, expr, .. } => {
                let ty = self.expr_type(expr_id, span)?;
                Some(PirExpr::Unary {
                    op: *op,
                    expr: Box::new(self.lower_expr(*expr)?),
                    ty,
                })
            }
            Expr::Binary { lhs, op, rhs, .. } => {
                let ty = self.expr_type(expr_id, span)?;
                Some(PirExpr::Binary {
                    op: *op,
                    lhs: Box::new(self.lower_expr(*lhs)?),
                    rhs: Box::new(self.lower_expr(*rhs)?),
                    ty,
                })
            }
            Expr::Call { callee, args, .. } => self.lower_call(expr_id, *callee, args, span),
            Expr::Member { object, member, .. } => {
                let ty = self.expr_type(expr_id, span)?;
                Some(PirExpr::Member {
                    base: Box::new(self.lower_expr(*object)?),
                    member: member.clone(),
                    ty,
                })
            }
            Expr::Index { object, index, .. } => {
                let ty = self.expr_type(expr_id, span)?;
                Some(PirExpr::Index {
                    base: Box::new(self.lower_expr(*object)?),
                    index: Box::new(self.lower_expr(*index)?),
                    ty,
                })
            }
            Expr::List(items) => {
                let ty = self.expr_type(expr_id, span)?;
                match ty {
                    PirType::Array(_, _) => Some(PirExpr::ArrayLiteral {
                        items: items
                            .iter()
                            .map(|item| self.lower_expr(*item))
                            .collect::<Option<Vec<_>>>()?,
                        ty,
                    }),
                    _ => {
                        self.parent
                            .errors
                            .push(PirLowerError::UnsupportedExpression {
                                function: self.function.name.clone(),
                                kind: "list literal",
                                span,
                            });
                        None
                    }
                }
            }
            _ => {
                self.parent
                    .errors
                    .push(PirLowerError::UnsupportedExpression {
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
    ) -> Option<PirExpr> {
        let Expr::Variable(callee_name) = &self.body.exprs[callee_id] else {
            self.parent
                .errors
                .push(PirLowerError::UnsupportedCallTarget {
                    function: self.function.name.clone(),
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
            let lowered_args = self.lower_ordered_args(callee_name, args, &field_names, span)?;
            let fields = record
                .fields
                .iter()
                .map(|field| SmolStr::new(field.name))
                .zip(lowered_args)
                .collect::<Vec<_>>();
            return Some(PirExpr::StructLiteral {
                name: SmolStr::new(record.name),
                fields,
                ty,
            });
        }

        if let Some(layout) = self.parent.value_types.get(callee_name) {
            let layout = layout.clone();
            let field_names = layout
                .fields
                .iter()
                .map(|field| field.name.clone())
                .collect::<Vec<_>>();
            let lowered_args = self.lower_ordered_args(callee_name, args, &field_names, span)?;
            let fields = layout
                .fields
                .iter()
                .map(|field| field.name.clone())
                .zip(lowered_args)
                .collect::<Vec<_>>();
            return Some(PirExpr::StructLiteral {
                name: layout.name,
                fields,
                ty,
            });
        }

        if let Some(function_idx) = self.parent.functions_by_name.get(callee_name) {
            let function_idx = *function_idx;
            let callee = &self.parent.module.functions[function_idx];
            if callee.lane() != FunctionLane::Portable {
                self.parent.errors.push(PirLowerError::NonPortableCallee {
                    caller: self.function.name.clone(),
                    callee: callee_name.clone(),
                    span,
                });
                return None;
            }
            let param_names = self.parent.module.functions[function_idx]
                .params
                .iter()
                .map(|param| param.name.clone())
                .collect::<Vec<_>>();
            let lowered_args = self.lower_ordered_args(callee_name, args, &param_names, span)?;
            self.callees.insert(function_idx);
            return Some(PirExpr::Call {
                target: PirCallTarget::Function(callee_name.clone()),
                args: lowered_args,
                ty,
            });
        }

        if let Some(intrinsic) = intrinsic_from_name(callee_name.as_str()) {
            let param_names = intrinsic_param_names(intrinsic);
            let lowered_args = self.lower_ordered_args(callee_name, args, &param_names, span)?;
            return Some(PirExpr::Call {
                target: PirCallTarget::Intrinsic(intrinsic),
                args: lowered_args,
                ty,
            });
        }

        self.parent.errors.push(PirLowerError::UnknownCallTarget {
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
        params: &[SmolStr],
        span: TextRange,
    ) -> Option<Vec<PirExpr>> {
        if params.is_empty() {
            if args.is_empty() {
                return Some(Vec::new());
            }
            self.parent
                .errors
                .push(PirLowerError::UnknownNamedArgument {
                    function: self.function.name.clone(),
                    callee: callee_name.clone(),
                    arg: SmolStr::new("<extra-arg>"),
                    span,
                });
            return None;
        }

        let mut ordered = vec![None; params.len()];
        let mut next_positional = 0usize;
        for arg in args {
            match arg {
                Arg::Positional { value, .. } => {
                    if next_positional >= ordered.len() {
                        self.parent
                            .errors
                            .push(PirLowerError::UnknownNamedArgument {
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
                            .push(PirLowerError::UnknownNamedArgument {
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
                            .push(PirLowerError::DuplicateNamedArgument {
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
                    self.parent.errors.push(PirLowerError::MissingArgument {
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

    fn expr_type(&mut self, expr_id: Idx<Expr>, span: TextRange) -> Option<PirType> {
        let Some(ty) = self.fn_info.expr_types.get(&expr_id.into_raw()) else {
            self.parent.errors.push(PirLowerError::MissingExprType {
                function: self.function.name.clone(),
                span,
            });
            return None;
        };
        match lower_type(ty, &self.parent.value_types) {
            Ok(ty) => Some(ty),
            Err(err) => {
                self.parent.errors.push(err);
                None
            }
        }
    }

    fn local_type(&mut self, name: &SmolStr, span: TextRange) -> Option<PirType> {
        let Some(ty) = self.fn_info.local_types.get(name) else {
            self.parent.errors.push(PirLowerError::UnknownLocal {
                function: self.function.name.clone(),
                name: name.clone(),
                span,
            });
            return None;
        };
        match lower_type(ty, &self.parent.value_types) {
            Ok(ty) => Some(ty),
            Err(err) => {
                self.parent.errors.push(err);
                None
            }
        }
    }
}

fn lower_type(
    ty: &Type,
    value_types: &HashMap<SmolStr, PirStructType>,
) -> Result<PirType, PirLowerError> {
    match ty {
        Type::Nil => Ok(PirType::Nothing),
        Type::Boolean => Ok(PirType::Bool),
        Type::Integer => Ok(PirType::I64),
        Type::I32 => Ok(PirType::I32),
        Type::U32 => Ok(PirType::U32),
        Type::I64 => Ok(PirType::I64),
        Type::U64 => Ok(PirType::U64),
        Type::Float => Ok(PirType::F32),
        Type::F32 => Ok(PirType::F32),
        Type::Vec2 => Ok(PirType::Vec2),
        Type::Vec3 => Ok(PirType::Vec3),
        Type::Vec4 => Ok(PirType::Vec4),
        Type::Mat3 => Ok(PirType::Mat3),
        Type::Mat4 => Ok(PirType::Mat4),
        Type::Quat => Ok(PirType::Quat),
        Type::Array(inner, len) => Ok(PirType::Array(
            Box::new(lower_type(inner, value_types)?),
            *len,
        )),
        Type::Named(name, _) => value_types
            .get(name)
            .cloned()
            .map(PirType::Struct)
            .ok_or_else(|| PirLowerError::NonPortableType {
                context: "type".to_string(),
                ty: name.to_string(),
            }),
        _ => Err(PirLowerError::NonPortableType {
            context: "type".to_string(),
            ty: format!("{ty:?}"),
        }),
    }
}

fn lower_type_ref(
    ty: &TypeRef,
    value_types: &HashMap<SmolStr, PirStructType>,
) -> Result<PirType, PirLowerError> {
    match ty.name.as_str() {
        "Nothing" => Ok(PirType::Nothing),
        "Bool" | "Boolean" => Ok(PirType::Bool),
        "Integer" => Ok(PirType::I64),
        "I32" => Ok(PirType::I32),
        "U32" => Ok(PirType::U32),
        "I64" => Ok(PirType::I64),
        "U64" => Ok(PirType::U64),
        "Float" | "F32" => Ok(PirType::F32),
        "Vec2" => Ok(PirType::Vec2),
        "Vec3" => Ok(PirType::Vec3),
        "Vec4" => Ok(PirType::Vec4),
        "Mat3" => Ok(PirType::Mat3),
        "Mat4" => Ok(PirType::Mat4),
        "Quat" => Ok(PirType::Quat),
        "Array" => match ty.args.as_slice() {
            [inner, len] => {
                let len =
                    len.name
                        .parse::<usize>()
                        .map_err(|_| PirLowerError::NonPortableType {
                            context: "type reference".to_string(),
                            ty: len.name.to_string(),
                        })?;
                Ok(PirType::Array(
                    Box::new(lower_type_ref(inner, value_types)?),
                    len,
                ))
            }
            _ => Err(PirLowerError::NonPortableType {
                context: "type reference".to_string(),
                ty: "Array".to_string(),
            }),
        },
        name => value_types
            .get(name)
            .cloned()
            .map(PirType::Struct)
            .ok_or_else(|| PirLowerError::NonPortableType {
                context: "type reference".to_string(),
                ty: name.to_string(),
            }),
    }
}

fn literal_to_value(literal: &Literal, ty: &PirType) -> Result<PirValue, PirLowerError> {
    match (literal, ty) {
        (Literal::Boolean(value), PirType::Bool) => Ok(PirValue::Bool(*value)),
        (Literal::Integer(value), PirType::I32) => Ok(PirValue::I32(*value as i32)),
        (Literal::Integer(value), PirType::U32) => Ok(PirValue::U32(*value as u32)),
        (Literal::Integer(value), PirType::I64) => Ok(PirValue::I64(*value)),
        (Literal::Integer(value), PirType::U64) => Ok(PirValue::U64(*value as u64)),
        (Literal::Float(value), PirType::F32) => Ok(PirValue::F32(*value as f32)),
        (Literal::Integer(value), PirType::F32) => Ok(PirValue::F32(*value as f32)),
        (Literal::Nil, PirType::Nothing) => Ok(PirValue::Nothing),
        (Literal::String(value), _) => Err(PirLowerError::NonPortableType {
            context: "string literal".to_string(),
            ty: value.to_string(),
        }),
        (Literal::Integer(value), _) => Ok(PirValue::I64(*value)),
        (Literal::Float(value), _) => Ok(PirValue::F32(*value as f32)),
        (Literal::Boolean(value), _) => Ok(PirValue::Bool(*value)),
        (Literal::Nil, _) => Ok(PirValue::Nothing),
    }
}

fn intrinsic_from_name(name: &str) -> Option<PirIntrinsic> {
    match name {
        "i32" => Some(PirIntrinsic::CastI32),
        "u32" => Some(PirIntrinsic::CastU32),
        "i64" => Some(PirIntrinsic::CastI64),
        "u64" => Some(PirIntrinsic::CastU64),
        "f32" => Some(PirIntrinsic::CastF32),
        "vec2" => Some(PirIntrinsic::Vec2),
        "vec3" => Some(PirIntrinsic::Vec3),
        "vec4" => Some(PirIntrinsic::Vec4),
        "quat" => Some(PirIntrinsic::Quat),
        "mat3_identity" => Some(PirIntrinsic::Mat3Identity),
        "mat3_cols" => Some(PirIntrinsic::Mat3Cols),
        "mat4_identity" => Some(PirIntrinsic::Mat4Identity),
        "mat4_cols" => Some(PirIntrinsic::Mat4Cols),
        "bounds2_center" => Some(PirIntrinsic::Bounds2Center),
        "bounds2_size" => Some(PirIntrinsic::Bounds2Size),
        "bounds3_center" => Some(PirIntrinsic::Bounds3Center),
        "bounds3_size" => Some(PirIntrinsic::Bounds3Size),
        "transform3_identity" => Some(PirIntrinsic::Transform3Identity),
        "transform_point" => Some(PirIntrinsic::TransformPoint),
        "transform_vector" => Some(PirIntrinsic::TransformVector),
        "transform_normal" => Some(PirIntrinsic::TransformNormal),
        "compose_transform3" => Some(PirIntrinsic::ComposeTransform3),
        "inverse_transform3" => Some(PirIntrinsic::InverseTransform3),
        "field_transform_point" => Some(PirIntrinsic::FieldTransformPoint),
        "field_instance_point" => Some(PirIntrinsic::FieldInstancePoint),
        "field_mirror_point" => Some(PirIntrinsic::FieldMirrorPoint),
        "field_repeat_point" => Some(PirIntrinsic::FieldRepeatPoint),
        "sphere" => Some(PirIntrinsic::Sphere),
        "box" => Some(PirIntrinsic::Box),
        "capsule" => Some(PirIntrinsic::Capsule),
        "cylinder" => Some(PirIntrinsic::Cylinder),
        "plane" => Some(PirIntrinsic::Plane),
        "torus" => Some(PirIntrinsic::Torus),
        "__wr_primitive_sphere" => Some(PirIntrinsic::Sphere),
        "__wr_primitive_box" => Some(PirIntrinsic::Box),
        "__wr_primitive_capsule" => Some(PirIntrinsic::Capsule),
        "__wr_primitive_cylinder" => Some(PirIntrinsic::Cylinder),
        "__wr_primitive_plane" => Some(PirIntrinsic::Plane),
        "__wr_primitive_torus" => Some(PirIntrinsic::Torus),
        "field_union" => Some(PirIntrinsic::FieldUnion),
        "field_intersection" => Some(PirIntrinsic::FieldIntersection),
        "field_subtract" => Some(PirIntrinsic::FieldSubtract),
        "dot" => Some(PirIntrinsic::Dot),
        "length" => Some(PirIntrinsic::Length),
        "normalize" => Some(PirIntrinsic::Normalize),
        "cross" => Some(PirIntrinsic::Cross),
        "min" => Some(PirIntrinsic::Min),
        "max" => Some(PirIntrinsic::Max),
        "clamp" => Some(PirIntrinsic::Clamp),
        "mix" => Some(PirIntrinsic::Mix),
        "abs" => Some(PirIntrinsic::Abs),
        "sign" => Some(PirIntrinsic::Sign),
        "floor" => Some(PirIntrinsic::Floor),
        "ceil" => Some(PirIntrinsic::Ceil),
        "fract" => Some(PirIntrinsic::Fract),
        "sin" => Some(PirIntrinsic::Sin),
        "cos" => Some(PirIntrinsic::Cos),
        "sqrt" => Some(PirIntrinsic::Sqrt),
        "pow" => Some(PirIntrinsic::Pow),
        "distance" => Some(PirIntrinsic::Distance),
        "reflect" => Some(PirIntrinsic::Reflect),
        _ => None,
    }
}

fn intrinsic_param_names(intrinsic: PirIntrinsic) -> Vec<SmolStr> {
    match intrinsic {
        PirIntrinsic::CastI32
        | PirIntrinsic::CastU32
        | PirIntrinsic::CastI64
        | PirIntrinsic::CastU64
        | PirIntrinsic::CastF32
        | PirIntrinsic::Length
        | PirIntrinsic::Normalize
        | PirIntrinsic::Abs
        | PirIntrinsic::Sign
        | PirIntrinsic::Floor
        | PirIntrinsic::Ceil
        | PirIntrinsic::Fract
        | PirIntrinsic::Sin
        | PirIntrinsic::Cos
        | PirIntrinsic::Sqrt => vec![SmolStr::new("value")],
        PirIntrinsic::Vec2 => vec![SmolStr::new("x"), SmolStr::new("y")],
        PirIntrinsic::Vec3 => vec![SmolStr::new("x"), SmolStr::new("y"), SmolStr::new("z")],
        PirIntrinsic::Vec4 | PirIntrinsic::Quat => vec![
            SmolStr::new("x"),
            SmolStr::new("y"),
            SmolStr::new("z"),
            SmolStr::new("w"),
        ],
        PirIntrinsic::Mat3Identity | PirIntrinsic::Mat4Identity => Vec::new(),
        PirIntrinsic::Mat3Cols => vec![SmolStr::new("c0"), SmolStr::new("c1"), SmolStr::new("c2")],
        PirIntrinsic::Mat4Cols => vec![
            SmolStr::new("c0"),
            SmolStr::new("c1"),
            SmolStr::new("c2"),
            SmolStr::new("c3"),
        ],
        PirIntrinsic::Bounds2Center
        | PirIntrinsic::Bounds2Size
        | PirIntrinsic::Bounds3Center
        | PirIntrinsic::Bounds3Size => vec![SmolStr::new("bounds")],
        PirIntrinsic::Transform3Identity => Vec::new(),
        PirIntrinsic::TransformPoint => vec![SmolStr::new("transform"), SmolStr::new("point")],
        PirIntrinsic::TransformVector => vec![SmolStr::new("transform"), SmolStr::new("vector")],
        PirIntrinsic::TransformNormal => vec![SmolStr::new("transform"), SmolStr::new("normal")],
        PirIntrinsic::ComposeTransform3 => vec![SmolStr::new("left"), SmolStr::new("right")],
        PirIntrinsic::InverseTransform3 => vec![SmolStr::new("transform")],
        PirIntrinsic::FieldTransformPoint => {
            vec![SmolStr::new("transform"), SmolStr::new("point")]
        }
        PirIntrinsic::FieldInstancePoint => {
            vec![SmolStr::new("instance"), SmolStr::new("point")]
        }
        PirIntrinsic::FieldMirrorPoint => vec![SmolStr::new("mirror"), SmolStr::new("point")],
        PirIntrinsic::FieldRepeatPoint => vec![SmolStr::new("period"), SmolStr::new("point")],
        PirIntrinsic::Sphere => vec![SmolStr::new("p"), SmolStr::new("radius")],
        PirIntrinsic::Box => vec![SmolStr::new("p"), SmolStr::new("half")],
        PirIntrinsic::Capsule => vec![
            SmolStr::new("p"),
            SmolStr::new("a"),
            SmolStr::new("b"),
            SmolStr::new("radius"),
        ],
        PirIntrinsic::Cylinder => vec![
            SmolStr::new("p"),
            SmolStr::new("radius"),
            SmolStr::new("half_height"),
        ],
        PirIntrinsic::Plane => vec![
            SmolStr::new("p"),
            SmolStr::new("normal"),
            SmolStr::new("offset"),
        ],
        PirIntrinsic::Torus => vec![
            SmolStr::new("p"),
            SmolStr::new("major_radius"),
            SmolStr::new("minor_radius"),
        ],
        PirIntrinsic::FieldUnion
        | PirIntrinsic::FieldIntersection
        | PirIntrinsic::FieldSubtract => vec![SmolStr::new("left"), SmolStr::new("right")],
        PirIntrinsic::Dot
        | PirIntrinsic::Cross
        | PirIntrinsic::Min
        | PirIntrinsic::Max
        | PirIntrinsic::Pow
        | PirIntrinsic::Distance
        | PirIntrinsic::Reflect => vec![SmolStr::new("left"), SmolStr::new("right")],
        PirIntrinsic::Clamp | PirIntrinsic::Mix => vec![
            SmolStr::new("value"),
            SmolStr::new("min"),
            SmolStr::new("max"),
        ],
    }
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
        Stmt::IgnoreResult { .. } => "ignore_result",
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
        Expr::TypeApply { .. } => "type_apply",
        Expr::Crash { .. } => "crash",
        Expr::Call { .. } => "call",
        Expr::Member { .. } => "member",
        Expr::Index { .. } => "index",
        Expr::List(_) => "list",
        Expr::Map(_) => "map",
        Expr::StringInterp(_) => "string_interp",
        Expr::Closure { .. } => "closure",
    }
}
