//! Owns CPU evaluation of portable/payload bodies that are shared across query
//! execution paths.
//! Does not own query traversal, tracing, or backend selection.
//!
//! Key invariants:
//! - portable evaluation must preserve the same value semantics the MIR/kernel
//!   lowering path expects.
//! - scope handling here must mirror authored lexical intent because callers use
//!   these helpers as the CPU oracle for backend parity.
//!
//! Primary entrypoints:
//! - `DirectQueryOps::eval_payload_body`
//! - portable expression helpers in this module
//!
//! Failure modes / common pitfalls:
//! - treating missing payload state as an execution shortcut instead of a value
//!   semantics question causes silent CPU/GPU drift.

use super::*;

impl<'a> DirectQueryOps<'a> {
    pub(crate) fn eval_payload_body(
        &self,
        body: &hir::Body,
    ) -> Result<KernelValue, QueryExecError> {
        if body.root_stmts.is_empty() {
            return Ok(default_payload());
        }
        let mut scopes = vec![HashMap::new()];
        let value = self.eval_portable_body_expr(body, &mut scopes)?;
        Ok(match value {
            KernelValue::Nothing => default_payload(),
            other => other,
        })
    }

    pub(crate) fn eval_portable_body_expr(
        &self,
        body: &hir::Body,
        scopes: &mut Vec<HashMap<SmolStr, PortableVariable>>,
    ) -> Result<KernelValue, QueryExecError> {
        let (flow, last_value) =
            self.execute_portable_stmt_block(body, &body.root_stmts, scopes)?;
        match flow {
            PortableFlow::None => Ok(last_value),
            PortableFlow::Return(value) => Ok(value),
            PortableFlow::Break | PortableFlow::Continue => Err(QueryExecError::Unsupported {
                message: "loop control escaped a portable function body".to_string(),
            }),
        }
    }

    pub(crate) fn execute_portable_stmt_block(
        &self,
        body: &hir::Body,
        stmts: &[hir::Idx<hir::Stmt>],
        scopes: &mut Vec<HashMap<SmolStr, PortableVariable>>,
    ) -> Result<(PortableFlow, KernelValue), QueryExecError> {
        scopes.push(HashMap::new());
        let mut last_value = KernelValue::Nothing;
        for stmt in stmts {
            let (flow, value) = self.execute_portable_stmt(body, *stmt, scopes)?;
            if !matches!(flow, PortableFlow::None) {
                scopes.pop();
                return Ok((flow, value));
            }
            last_value = value;
        }
        scopes.pop();
        Ok((PortableFlow::None, last_value))
    }

    pub(crate) fn execute_portable_stmt(
        &self,
        body: &hir::Body,
        stmt_id: hir::Idx<hir::Stmt>,
        scopes: &mut Vec<HashMap<SmolStr, PortableVariable>>,
    ) -> Result<(PortableFlow, KernelValue), QueryExecError> {
        match &body.stmts[stmt_id] {
            hir::Stmt::Expr(expr) => Ok((
                PortableFlow::None,
                self.eval_portable_expr(body, *expr, scopes)?,
            )),
            hir::Stmt::IgnoreResult { expr } => {
                let _ = self.eval_portable_expr(body, *expr, scopes)?;
                Ok((PortableFlow::None, KernelValue::Nothing))
            }
            hir::Stmt::Let {
                name,
                value,
                mutable,
                ..
            } => {
                let value = self.eval_portable_expr(body, *value, scopes)?;
                scopes.last_mut().expect("portable scope").insert(
                    name.clone(),
                    PortableVariable {
                        value,
                        mutable: *mutable,
                    },
                );
                Ok((PortableFlow::None, KernelValue::Nothing))
            }
            hir::Stmt::Assign {
                name, op, value, ..
            } => {
                let value = self.eval_portable_expr(body, *value, scopes)?;
                self.assign_portable_local(name, *op, value, scopes)?;
                Ok((PortableFlow::None, KernelValue::Nothing))
            }
            hir::Stmt::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let condition = self.eval_portable_expr(body, *condition, scopes)?;
                match condition {
                    KernelValue::Bool(true) => {
                        self.execute_portable_stmt_block(body, then_branch, scopes)
                    }
                    KernelValue::Bool(false) => {
                        if let Some(else_branch) = else_branch {
                            self.execute_portable_stmt_block(body, else_branch, scopes)
                        } else {
                            Ok((PortableFlow::None, KernelValue::Nothing))
                        }
                    }
                    other => Err(QueryExecError::TypeMismatch {
                        expected: "Bool".to_string(),
                        found: value_label(&other),
                    }),
                }
            }
            hir::Stmt::While {
                condition,
                body: loop_body,
            } => {
                loop {
                    let condition = self.eval_portable_expr(body, *condition, scopes)?;
                    match condition {
                        KernelValue::Bool(true) => {
                            let (flow, _value) =
                                self.execute_portable_stmt_block(body, loop_body, scopes)?;
                            match flow {
                                PortableFlow::None | PortableFlow::Continue => {}
                                PortableFlow::Break => break,
                                PortableFlow::Return(value) => {
                                    return Ok((PortableFlow::Return(value.clone()), value));
                                }
                            }
                        }
                        KernelValue::Bool(false) => break,
                        other => {
                            return Err(QueryExecError::TypeMismatch {
                                expected: "Bool".to_string(),
                                found: value_label(&other),
                            });
                        }
                    }
                }
                Ok((PortableFlow::None, KernelValue::Nothing))
            }
            hir::Stmt::Return(Some(expr)) => {
                let value = self.eval_portable_expr(body, *expr, scopes)?;
                Ok((PortableFlow::Return(value.clone()), value))
            }
            hir::Stmt::Return(None) => Ok((
                PortableFlow::Return(KernelValue::Nothing),
                KernelValue::Nothing,
            )),
            hir::Stmt::Break => Ok((PortableFlow::Break, KernelValue::Nothing)),
            hir::Stmt::Continue => Ok((PortableFlow::Continue, KernelValue::Nothing)),
            other => Err(QueryExecError::Unsupported {
                message: format!(
                    "portable body statement '{other:?}' is not supported in query_exec::cpu"
                ),
            }),
        }
    }

    pub(crate) fn assign_portable_local(
        &self,
        name: &SmolStr,
        op: hir::AssignOp,
        value: KernelValue,
        scopes: &mut [HashMap<SmolStr, PortableVariable>],
    ) -> Result<(), QueryExecError> {
        for scope in scopes.iter_mut().rev() {
            if let Some(variable) = scope.get_mut(name) {
                if !variable.mutable {
                    return Err(QueryExecError::Unsupported {
                        message: format!("cannot assign to immutable local '{name}'"),
                    });
                }
                let next = match op {
                    hir::AssignOp::Assign => value,
                    hir::AssignOp::AddAssign => {
                        eval_binary_value(BinaryOp::Add, variable.value.clone(), value)?
                    }
                    hir::AssignOp::SubAssign => {
                        eval_binary_value(BinaryOp::Sub, variable.value.clone(), value)?
                    }
                    hir::AssignOp::MulAssign => {
                        eval_binary_value(BinaryOp::Mul, variable.value.clone(), value)?
                    }
                    hir::AssignOp::DivAssign => {
                        eval_binary_value(BinaryOp::Div, variable.value.clone(), value)?
                    }
                };
                variable.value = next;
                return Ok(());
            }
        }
        Err(QueryExecError::Unsupported {
            message: format!("portable body variable '{name}' is not available"),
        })
    }

    pub(crate) fn eval_portable_expr(
        &self,
        body: &hir::Body,
        expr_id: hir::Idx<Expr>,
        scopes: &[HashMap<SmolStr, PortableVariable>],
    ) -> Result<KernelValue, QueryExecError> {
        match &body.exprs[expr_id] {
            Expr::Literal(literal) => Ok(literal_to_kernel(literal)),
            Expr::Variable(name) => self
                .lookup_portable_local(name, scopes)
                .cloned()
                .ok_or_else(|| QueryExecError::Unsupported {
                    message: format!("portable body variable '{name}' is not available"),
                }),
            Expr::Unary { op, expr, .. } => {
                let value = self.eval_portable_expr(body, *expr, scopes)?;
                eval_unary_value(*op, value)
            }
            Expr::Binary { lhs, op, rhs, .. } => {
                let lhs = self.eval_portable_expr(body, *lhs, scopes)?;
                let rhs = self.eval_portable_expr(body, *rhs, scopes)?;
                eval_binary_value(*op, lhs, rhs)
            }
            Expr::Call {
                callee,
                args,
                type_args,
            } if type_args.is_empty() => {
                let Expr::Variable(name) = &body.exprs[*callee] else {
                    return Err(QueryExecError::Unsupported {
                        message: "portable body only supports named calls".to_string(),
                    });
                };
                let lowered = args
                    .iter()
                    .map(|arg| match arg {
                        hir::Arg::Positional { value, .. } | hir::Arg::Named { value, .. } => {
                            self.eval_portable_expr(body, *value, scopes)
                        }
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                self.eval_callable(name, lowered)
            }
            Expr::Member { object, member, .. } => {
                let base = self.eval_portable_expr(body, *object, scopes)?;
                eval_member_value(base, member)
            }
            Expr::Index { object, index, .. } => {
                let base = self.eval_portable_expr(body, *object, scopes)?;
                let index = self.eval_portable_expr(body, *index, scopes)?;
                eval_index_value(base, index)
            }
            Expr::List(items) => Ok(KernelValue::Array(
                items
                    .iter()
                    .map(|item| self.eval_portable_expr(body, *item, scopes))
                    .collect::<Result<Vec<_>, _>>()?,
            )),
            _ => Err(QueryExecError::Unsupported {
                message: "portable body expression is not supported in query_exec::cpu".to_string(),
            }),
        }
    }

    pub(crate) fn eval_scene_named_arg(
        &self,
        args: &[SceneArgExpr],
        name: &str,
    ) -> Result<KernelValue, QueryExecError> {
        self.eval_scene_named_arg_opt(args, name)?
            .ok_or_else(|| QueryExecError::Unsupported {
                message: format!("missing scene argument '{name}'"),
            })
    }

    pub(crate) fn eval_scene_named_arg_opt(
        &self,
        args: &[SceneArgExpr],
        name: &str,
    ) -> Result<Option<KernelValue>, QueryExecError> {
        args.iter()
            .find_map(|arg| match arg {
                SceneArgExpr::Named {
                    name: arg_name,
                    value,
                } if arg_name.as_str() == name => {
                    Some(self.eval_scene_value_expr(value, &HashMap::new()))
                }
                _ => None,
            })
            .transpose()
    }

    pub(crate) fn eval_scene_constant(
        &self,
        expr: &SceneValueExpr,
    ) -> Result<KernelValue, QueryExecError> {
        self.eval_scene_value_expr(expr, &HashMap::new())
    }

    pub(crate) fn eval_scene_value_expr(
        &self,
        expr: &SceneValueExpr,
        env: &HashMap<SmolStr, KernelValue>,
    ) -> Result<KernelValue, QueryExecError> {
        match expr {
            SceneValueExpr::Literal(literal) => Ok(literal_to_kernel(literal)),
            SceneValueExpr::List(items) => Ok(KernelValue::Array(
                items
                    .iter()
                    .map(|item| self.eval_scene_value_expr(item, env))
                    .collect::<Result<Vec<_>, _>>()?,
            )),
            SceneValueExpr::Unary { op, expr } => {
                let value = self.eval_scene_value_expr(expr, env)?;
                eval_unary_value(*op, value)
            }
            SceneValueExpr::Binary { lhs, op, rhs } => {
                let lhs = self.eval_scene_value_expr(lhs, env)?;
                let rhs = self.eval_scene_value_expr(rhs, env)?;
                eval_binary_value(*op, lhs, rhs)
            }
            SceneValueExpr::Call { callee, args } => {
                let lowered = args
                    .iter()
                    .map(|arg| match arg {
                        SceneArgExpr::Positional(value) | SceneArgExpr::Named { value, .. } => {
                            self.eval_scene_value_expr(value, env)
                        }
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                self.eval_callable(callee, lowered)
            }
        }
    }

    pub(crate) fn eval_callable(
        &self,
        name: &SmolStr,
        args: Vec<KernelValue>,
    ) -> Result<KernelValue, QueryExecError> {
        if let Some(builtin) = self.eval_builtin_or_value_constructor(name, &args)? {
            return Ok(builtin);
        }
        self.execute_portable_function(name, args)
    }

    pub(crate) fn eval_builtin_or_value_constructor(
        &self,
        name: &SmolStr,
        args: &[KernelValue],
    ) -> Result<Option<KernelValue>, QueryExecError> {
        if let Some(builtin) = eval_builtin_callable(name.as_str(), args)? {
            return Ok(Some(builtin));
        }
        if portable::builtin_record_is_constructible(name.as_str()) {
            let record = portable::builtin_record(name.as_str()).expect("constructible record");
            return Ok(Some(construct_builtin_record_value(record, args)?));
        }
        if let Some(field_names) = self.ctx.value_class_fields.get(name) {
            let fields = field_names
                .iter()
                .enumerate()
                .map(|(index, field)| {
                    let value =
                        args.get(index)
                            .cloned()
                            .ok_or_else(|| QueryExecError::Unsupported {
                                message: format!(
                                    "missing constructor arg {} for value '{}'",
                                    index, name
                                ),
                            })?;
                    Ok((field.clone(), value))
                })
                .collect::<Result<Vec<_>, QueryExecError>>()?;
            return Ok(Some(KernelValue::Struct(KernelStructValue {
                name: name.clone(),
                fields,
            })));
        }
        Ok(None)
    }

    pub(crate) fn execute_portable_function(
        &self,
        name: &SmolStr,
        args: Vec<KernelValue>,
    ) -> Result<KernelValue, QueryExecError> {
        let function = self
            .ctx
            .functions_by_name
            .get(name)
            .ok_or_else(|| QueryExecError::MissingFunction { name: name.clone() })?;
        if function.lane() != hir::FunctionLane::Portable {
            return Err(QueryExecError::Unsupported {
                message: format!(
                    "portable function execution cannot call non-portable function '{}'",
                    name
                ),
            });
        }
        let body = function
            .body
            .as_ref()
            .ok_or_else(|| QueryExecError::Unsupported {
                message: format!("portable function '{name}' does not have a body"),
            })?;
        if args.len() != function.params.len() {
            return Err(QueryExecError::Unsupported {
                message: format!(
                    "portable function '{}' expected {} arguments but received {}",
                    name,
                    function.params.len(),
                    args.len()
                ),
            });
        }
        let mut scopes = vec![HashMap::new()];
        for (param, value) in function.params.iter().zip(args) {
            scopes.last_mut().expect("portable scope").insert(
                param.name.clone(),
                PortableVariable {
                    value,
                    mutable: false,
                },
            );
        }
        self.eval_portable_body_expr(body, &mut scopes)
    }

    pub(crate) fn lookup_portable_local<'b>(
        &self,
        name: &SmolStr,
        scopes: &'b [HashMap<SmolStr, PortableVariable>],
    ) -> Option<&'b KernelValue> {
        scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).map(|variable| &variable.value))
    }
}
