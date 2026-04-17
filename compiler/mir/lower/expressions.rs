//! Owns authored-expression lowering onto MIR values and blocks.
//! Does not own module setup, function entry scaffolding, or statement ordering.
//!
//! Key invariants:
//! - expression lowering must preserve authored evaluation meaning, especially
//!   around nil/default fallbacks and builtin lowering.
//! - expression helpers may synthesize MIR temporaries, but they must leave the
//!   current block state consistent for surrounding statements.
//!
//! Primary entrypoints:
//! - `FunctionLowerer::lower_expr`
//! - expression helpers on `FunctionLowerer`
//!
//! Failure modes / common pitfalls:
//! - leaking block/control-flow side effects out of an expression helper makes
//!   later statement lowering order-dependent in surprising ways.

use super::module_lower::is_syntactic_stringish;
use super::*;

impl FunctionLowerer {
    pub(crate) fn lower_expr(&mut self, body: &hir::Body, expr_id: hir::Idx<Expr>) -> Value {
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
                    UnaryOp::Try => {
                        let result_val = self.lower_expr(body, *expr);
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
                            value: Rvalue::ResultUnwrap {
                                value: result_val.clone(),
                            },
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
                        self.emit_defers(body, span);
                        self.set_terminator(Terminator::Return {
                            value: Some(result_val),
                            span,
                        });

                        self.current_block = join_block;
                        Value::Local(result_local)
                    }
                    UnaryOp::Fire => {
                        if let Expr::Call { callee, args, .. } = &body.exprs[*expr] {
                            if self.is_actor_call(body, *callee) {
                                let (target, arg_values) =
                                    self.lower_call_target(body, *callee, args);
                                self.push_stmt(MirStmt::ActorFire {
                                    target,
                                    args: arg_values,
                                    span,
                                });
                                return Value::Const(Literal::Nil);
                            }
                        }
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
                        let temp = self.new_temp_for_expr(body, expr_id);
                        self.push_stmt(MirStmt::Assign {
                            place: Place::Temp(temp),
                            value: Rvalue::ResultErr { value: operand },
                            span,
                        });
                        Value::Temp(temp)
                    }
                    _ => {
                        let operand = self.lower_expr(body, *expr);
                        let temp = self.new_temp_for_expr(body, expr_id);
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
                ) && let Expr::Index { object, index, .. } = &body.exprs[*lhs]
                {
                    let object_value = self.lower_expr(body, *object);
                    let index_value = self.lower_expr(body, *index);
                    let rhs_val = self.lower_expr(body, *rhs);
                    let set_name = match self.expr_type(body, *object) {
                        MirType::Named(name) if name.as_str() == "Map" => "__wr_map_set",
                        _ => "__wr_list_set",
                    };
                    let (new_val, args) = if *op == BinaryOp::Assign {
                        (
                            rhs_val.clone(),
                            vec![object_value, index_value, rhs_val.clone()],
                        )
                    } else {
                        let get_name = match self.expr_type(body, *object) {
                            MirType::Named(name) if name.as_str() == "Map" => "__wr_map_get",
                            _ => "__wr_list_get",
                        };
                        let current = self.new_temp(MirType::Unknown);
                        self.push_stmt(MirStmt::Assign {
                            place: Place::Temp(current),
                            value: Rvalue::Call {
                                kind: CallKind::Sync,
                                target: CallTarget::Function(SmolStr::new(get_name)),
                                args: vec![object_value.clone(), index_value.clone()],
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
                        let new_val = Value::Temp(temp);
                        (
                            new_val.clone(),
                            vec![object_value, index_value, new_val.clone()],
                        )
                    };
                    let ignored = self.new_temp(MirType::Unknown);
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Temp(ignored),
                        value: Rvalue::Call {
                            kind: CallKind::Sync,
                            target: CallTarget::Function(SmolStr::new(set_name)),
                            args,
                        },
                        span,
                    });
                    return new_val;
                } else if matches!(
                    op,
                    BinaryOp::Assign
                        | BinaryOp::AddAssign
                        | BinaryOp::SubAssign
                        | BinaryOp::MulAssign
                        | BinaryOp::DivAssign
                ) && let Expr::Member { object, member, .. } = &body.exprs[*lhs]
                {
                    let slot = self.member_slot_hint(body, *object, member);
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
                    if matches!(op, BinaryOp::Add)
                        && (is_syntactic_stringish(body, *lhs)
                            || is_syntactic_stringish(body, *rhs))
                    {
                        let lhs = self.lower_expr(body, *lhs);
                        let rhs = self.lower_expr(body, *rhs);
                        let temp = self.new_temp_for_expr(body, expr_id);
                        self.push_stmt(MirStmt::Assign {
                            place: Place::Temp(temp),
                            value: Rvalue::StrConcat {
                                parts: vec![lhs, rhs],
                                // Escape analysis will refine this to LocalTemp when possible.
                                alloc: AllocKind::Escaping,
                            },
                            span,
                        });
                        return Value::Temp(temp);
                    }
                    let lhs = self.lower_expr(body, *lhs);
                    let rhs = self.lower_expr(body, *rhs);
                    let temp = self.new_temp_for_expr(body, expr_id);
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
                let temp = self.new_temp_for_expr(body, expr_id);
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
                        let temp = self.new_temp_for_expr(body, expr_id);
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
                if let MirType::Named(class_name) = self.expr_type(body, *object)
                    && class_name.as_str() == "SceneDomain"
                    && let Some((contract_name, contract_field, nested_field, nested_ty)) =
                        scene_domain_compat_member(member.as_str())
                {
                    let base = self.lower_expr(body, *object);
                    let contract = self.lower_get_named_field(
                        base,
                        "SceneDomain",
                        contract_field,
                        MirType::Named(SmolStr::new(contract_name)),
                        span,
                    );
                    return self.lower_get_named_field(
                        contract,
                        contract_name,
                        nested_field,
                        nested_ty,
                        span,
                    );
                }
                if let Some(component_index) =
                    vector_component_index(self.expr_type(body, *object), member)
                {
                    let base = self.lower_expr(body, *object);
                    let temp = self.new_temp_for_expr(body, expr_id);
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Temp(temp),
                        value: Rvalue::Call {
                            kind: CallKind::Sync,
                            target: CallTarget::Function(SmolStr::new("__wr_vec_component")),
                            args: vec![
                                base,
                                Value::Const(Literal::Integer(component_index as i64)),
                            ],
                        },
                        span,
                    });
                    return Value::Temp(temp);
                }
                let base = self.lower_expr(body, *object);
                let slot = self.member_slot_hint(body, *object, member);
                let temp = self.new_temp_for_expr(body, expr_id);
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
            Expr::Index { object, index, .. } => {
                let object_value = self.lower_expr(body, *object);
                let index_value = self.lower_expr(body, *index);
                let target_name = match self.expr_type(body, *object) {
                    MirType::Named(name) if name.as_str() == "Map" => "__wr_map_get",
                    _ => "__wr_list_get",
                };
                let temp = self.new_temp_for_expr(body, expr_id);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(temp),
                    value: Rvalue::Call {
                        kind: CallKind::Sync,
                        target: CallTarget::Function(SmolStr::new(target_name)),
                        args: vec![object_value, index_value],
                    },
                    span,
                });
                Value::Temp(temp)
            }
            Expr::Call { callee, args, .. } => {
                if let Some(target) = self.parse_capture_builtin(body, expr_id) {
                    return self.build_scene_capture_value(&target, span);
                }
                if let Some(mode) = self.parse_dispatch_backend_builtin(body, expr_id) {
                    return self.build_dispatch_backend_value(mode, span);
                }
                if let Some(spec) = self.parse_scalar_query(body, expr_id) {
                    return self.lower_scalar_query_call(
                        body,
                        span,
                        self.expr_type(body, expr_id),
                        &spec,
                    );
                }
                if let Some(spec) = self.parse_batch_query(body, expr_id) {
                    return self.lower_batch_query_call(body, span, &spec);
                }
                if let Some(spec) = parse_kernel_dispatch_compute(body, expr_id) {
                    return self.lower_dispatch_compute_call(body, span, &spec);
                }
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
                    let temp = self.new_temp_for_expr(body, expr_id);
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
                        if field_values.get(idx).and_then(|val| val.as_ref()).is_none()
                            && let Some(default) = default
                        {
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
                    self.maybe_call_configure(&class_name, Value::Temp(temp), span);
                    return Value::Temp(temp);
                }
                let (target, args) = self.lower_call_target(body, *callee, args);
                let temp = self.new_temp_for_expr(body, expr_id);
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
                let temp = self.new_temp_for_expr(body, expr_id);
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
                let temp = self.new_temp_for_expr(body, expr_id);
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
                let temp = self.new_temp_for_expr(body, expr_id);
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
            Expr::Closure {
                body: closure_body, ..
            } => {
                // Lower the closure body expression; closures are not yet first-class
                // in the MIR, so we simply lower the body expression inline.
                self.lower_expr(body, *closure_body)
            }
        }
    }

    pub(crate) fn emit_defers(&mut self, body: &hir::Body, _span: TextRange) {
        let defers = self.defers.clone();
        for expr_id in defers.iter().rev() {
            let _ = self.lower_expr(body, *expr_id);
        }
    }

    pub(crate) fn lower_detach_expr(
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
        // If the detach site didn't specify an objective, inherit it from the nearest
        // surrounding `optimize <objective>:` block.
        let mut objective = objective.or_else(|| self.current_objective());
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
                if count > 1
                    && let Some(value) = self.lower_detach_pool_fixed(
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
                    )
                {
                    return value;
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
                    let temp = self.new_temp_for_expr(body, target_expr);
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
            Expr::Call { callee, .. } => {
                let mut handled = false;
                if let Expr::Variable(name) = &body.exprs[*callee]
                    && let Some(id) = self.type_tags.get(name).copied()
                {
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
        let temp = self.new_temp_for_expr(body, result_expr);
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

    pub(crate) fn lower_detach_pool_fixed(
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
        let objective = objective
            .or_else(|| self.current_objective())
            .unwrap_or(hir::Objective::Balance);
        let mut handles = Vec::with_capacity(count);
        for _ in 0..count {
            let instance = self.build_class_instance(&class, span);
            let target = Value::Const(Literal::Integer(class.class_id.0 as i64));
            let temp = self.new_temp_for_expr(body, result_expr);
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
        let list_temp = self.new_temp_for_expr(body, result_expr);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(list_temp),
            value: Rvalue::BuildList {
                items: handles,
                alloc: crate::mir::ir::AllocKind::LocalTemp,
            },
            span,
        });
        let pool_temp = self.new_temp_for_expr(body, result_expr);
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

    pub(crate) fn lower_detach_pool_auto(
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
        let objective = objective
            .or_else(|| self.current_objective())
            .unwrap_or(hir::Objective::Balance);
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
        let handle_temp = self.new_temp_for_expr(body, result_expr);
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
        let pool_temp = self.new_temp_for_expr(body, result_expr);
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

    pub(crate) fn lower_await(
        &mut self,
        body: &hir::Body,
        expr_id: hir::Idx<Expr>,
        span: TextRange,
    ) -> Value {
        let pending = self.lower_pending_call_or_value(body, expr_id, span);
        let temp = self.new_temp_for_expr(body, expr_id);
        self.push_stmt(MirStmt::Await {
            dst: Place::Temp(temp),
            pending,
            span,
        });
        Value::Temp(temp)
    }

    pub(crate) fn lower_pending_call_or_value(
        &mut self,
        body: &hir::Body,
        expr_id: hir::Idx<Expr>,
        span: TextRange,
    ) -> Value {
        if let Expr::Call { callee, args, .. } = &body.exprs[expr_id] {
            let kind = if self.is_actor_call(body, *callee) {
                CallKind::Actor
            } else {
                CallKind::Sync
            };
            let (target, args) = self.lower_call_target(body, *callee, args);
            let temp = self.new_temp_for_expr(body, expr_id);
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

    pub(crate) fn is_actor_call(&self, body: &hir::Body, callee: hir::Idx<Expr>) -> bool {
        if let Expr::Member { object, .. } = &body.exprs[callee] {
            matches!(self.expr_type(body, *object), MirType::Actor(_))
        } else {
            false
        }
    }

    pub(crate) fn lower_call_target(
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
                let collection_intrinsic = match self.expr_type(body, *object) {
                    MirType::Named(name) if name.as_str() == "List" => match member.as_str() {
                        "push" => Some("__wr_list_push"),
                        "len" => Some("__wr_list_len"),
                        _ => None,
                    },
                    MirType::Named(name) if name.as_str() == "Map" => match member.as_str() {
                        "set" => Some("__wr_map_set"),
                        "get" => Some("__wr_map_get"),
                        "len" => Some("__wr_map_len"),
                        _ => None,
                    },
                    _ => None,
                };
                let collection_intrinsic = collection_intrinsic.or_else(|| match member.as_str() {
                    "push" => Some("__wr_list_push"),
                    "set" => Some("__wr_map_set"),
                    "get" => Some("__wr_map_get"),
                    "len" => {
                        if matches!(&body.exprs[*object], Expr::List(_)) {
                            Some("__wr_list_len")
                        } else {
                            Some("__wr_map_len")
                        }
                    }
                    _ => None,
                });
                if let Some(intrinsic) = collection_intrinsic {
                    let mut intrinsic_args = Vec::with_capacity(values.len() + 1);
                    intrinsic_args.push(receiver);
                    intrinsic_args.extend(values);
                    return (
                        CallTarget::Function(SmolStr::new(intrinsic)),
                        intrinsic_args,
                    );
                }
                let class_hint = match &body.exprs[*object] {
                    Expr::Variable(name) if self.type_tags.contains_key(name) => Some(name.clone()),
                    _ => None,
                };
                if let MirType::Named(class_name) = self.expr_type(body, *object)
                    && let Some(methods) = self.interface_methods.get(&class_name)
                    && methods.contains(member)
                {
                    let mut args_with_recv = Vec::with_capacity(values.len() + 1);
                    args_with_recv.push(receiver.clone());
                    args_with_recv.extend(values);
                    let func_name = SmolStr::new(format!("{}.{}", class_name, member));
                    return (CallTarget::Function(func_name), args_with_recv);
                }
                let (method_id, method_name) = match self.expr_type(body, *object) {
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
                if method_id.is_none()
                    && !method_name.as_str().contains('.')
                    && let Some(interface_dispatch_target) =
                        self.resolve_unique_interface_dispatch_target(member)
                {
                    let mut args_with_recv = Vec::with_capacity(values.len() + 1);
                    args_with_recv.push(receiver.clone());
                    args_with_recv.extend(values);
                    return (
                        CallTarget::Function(interface_dispatch_target),
                        args_with_recv,
                    );
                }
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
                let mut call_args = values;
                if matches!(
                    name.as_str(),
                    "transform3_identity" | "compose_transform3" | "inverse_transform3"
                ) && let Some(class_id) = self.type_tags.get(&SmolStr::new("Transform3"))
                {
                    call_args.insert(0, Value::Const(Literal::Integer(class_id.0 as i64)));
                }
                if name.as_str() == "assert" && call_args.len() == 1 {
                    call_args.push(Value::Const(Literal::Nil));
                }
                (CallTarget::Function(name.clone()), call_args)
            }
            _ => {
                let callee_value = self.lower_expr(body, callee);
                (CallTarget::Indirect(callee_value), values)
            }
        }
    }

    pub(crate) fn method_id_for(&self, class_name: &SmolStr, method: &SmolStr) -> Option<u32> {
        self.class_method_ids
            .get(class_name)
            .and_then(|methods| methods.get(method).copied())
    }

    pub(crate) fn resolve_unique_interface_dispatch_target(
        &self,
        method: &SmolStr,
    ) -> Option<SmolStr> {
        let mut matched_interface: Option<&SmolStr> = None;
        for (interface_name, methods) in &self.interface_methods {
            if !methods.contains(method) {
                continue;
            }
            if matched_interface.is_some() {
                return None;
            }
            matched_interface = Some(interface_name);
        }
        matched_interface.map(|interface_name| SmolStr::new(format!("{interface_name}.{method}")))
    }

    pub(crate) fn member_slot_hint(
        &self,
        body: &hir::Body,
        object_expr: hir::Idx<Expr>,
        member: &SmolStr,
    ) -> Option<u32> {
        let MirType::Named(class_name) = self.expr_type(body, object_expr) else {
            return None;
        };
        self.class_fields
            .get(&class_name)
            .and_then(|fields| fields.iter().position(|field| field == member))
            .map(|idx| idx as u32)
    }

    pub(crate) fn resolve_class_init_target(
        &self,
        body: &hir::Body,
        callee: hir::Idx<Expr>,
    ) -> Option<(SmolStr, TypeTagId)> {
        match &body.exprs[callee] {
            Expr::Variable(name) => {
                let class_name = builtin_record_by_function(name.as_str())
                    .map(|record| SmolStr::new(record.name))
                    .unwrap_or_else(|| name.clone());
                self.type_tags
                    .get(&class_name)
                    .copied()
                    .map(|id| (class_name, id))
            }
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
    pub(crate) fn is_default_match_pattern(&self, pattern: &hir::Pattern) -> bool {
        match pattern {
            hir::Pattern::Wildcard => true,
            hir::Pattern::Binding(name) => {
                !self.type_tags.contains_key(name) && builtin_type_tag(name).is_none()
            }
            _ => false,
        }
    }
}
