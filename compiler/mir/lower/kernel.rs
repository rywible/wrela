//! Owns lowering of kernel IR statements/expressions into MIR fragments.
//! Does not own kernel planning or whole-function/module setup.
//!
//! Key invariants:
//! - kernel lowering must stay ABI-compatible with the kernel/query contracts
//!   chosen upstream.
//! - block bookkeeping here must leave surrounding MIR lowering in a consistent
//!   state for later statements.
//!
//! Primary entrypoints:
//! - kernel lowering helpers on `FunctionLowerer`
//!
//! Failure modes / common pitfalls:
//! - treating kernel control flow as ordinary MIR without preserving dispatch
//!   semantics breaks backend equivalence.

use super::module_lower::kernel_world_query_input_count;
use super::*;

impl FunctionLowerer {
    pub(crate) fn lower_kernel_stmt_block(&mut self, stmts: &[KernelStmt]) {
        for stmt in stmts {
            if !self.block_is_open(self.current_block) {
                break;
            }
            self.lower_kernel_stmt(stmt);
        }
    }

    pub(crate) fn lower_kernel_stmt(&mut self, stmt: &KernelStmt) {
        match stmt {
            KernelStmt::Let {
                name,
                mutable,
                ty,
                value,
                span,
            } => {
                let lowered = self.lower_kernel_expr(value);
                let local = self.new_local(name.clone(), *mutable, mir_type_from_type(ty));
                self.declare_local(name.clone(), local);
                self.declare_resultness(name.clone(), false);
                self.assign_use(Place::Local(local), lowered, *span);
            }
            KernelStmt::Assign {
                name,
                op,
                value,
                span,
            } => {
                let Some(local) = self.resolve_local(name) else {
                    return;
                };
                let rhs = self.lower_kernel_expr(value);
                match op {
                    AssignOp::Assign => self.assign_use(Place::Local(local), rhs, *span),
                    AssignOp::AddAssign
                    | AssignOp::SubAssign
                    | AssignOp::MulAssign
                    | AssignOp::DivAssign => {
                        let binary = match op {
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
                                op: binary,
                                lhs: Value::Local(local),
                                rhs,
                            },
                            span: *span,
                        });
                        self.assign_use(Place::Local(local), Value::Temp(temp), *span);
                    }
                }
            }
            KernelStmt::Expr { value, .. } | KernelStmt::IgnoreResult { value, .. } => {
                let _ = self.lower_kernel_expr(value);
            }
            KernelStmt::If {
                condition,
                then_block,
                else_block,
                span,
            } => {
                let cond = self.lower_kernel_expr(condition);
                let then_target = self.new_block();
                let else_target = self.new_block();
                let join_target = self.new_block();
                self.set_terminator(Terminator::Branch {
                    cond,
                    then_target,
                    else_target,
                    span: *span,
                });

                self.current_block = then_target;
                self.enter_scope();
                self.lower_kernel_stmt_block(then_block);
                self.exit_scope();
                if self.block_is_open(self.current_block) {
                    self.set_terminator(Terminator::Jump {
                        target: join_target,
                        span: *span,
                    });
                }

                self.current_block = else_target;
                self.enter_scope();
                self.lower_kernel_stmt_block(else_block);
                self.exit_scope();
                if self.block_is_open(self.current_block) {
                    self.set_terminator(Terminator::Jump {
                        target: join_target,
                        span: *span,
                    });
                }

                self.current_block = join_target;
            }
            KernelStmt::While {
                condition,
                body,
                span,
            } => {
                let head_block = self.new_block();
                let body_block = self.new_block();
                let exit_block = self.new_block();
                self.set_terminator(Terminator::Jump {
                    target: head_block,
                    span: *span,
                });

                self.current_block = head_block;
                let cond = self.lower_kernel_expr(condition);
                self.set_terminator(Terminator::Branch {
                    cond,
                    then_target: body_block,
                    else_target: exit_block,
                    span: *span,
                });

                self.current_block = body_block;
                self.loop_stack.push(LoopTarget {
                    break_target: exit_block,
                    continue_target: head_block,
                });
                self.enter_scope();
                self.lower_kernel_stmt_block(body);
                self.exit_scope();
                self.loop_stack.pop();
                if self.block_is_open(self.current_block) {
                    self.set_terminator(Terminator::Jump {
                        target: head_block,
                        span: *span,
                    });
                }
                self.current_block = exit_block;
            }
            KernelStmt::Return { value, span } => {
                let value = value.as_ref().map(|expr| self.lower_kernel_expr(expr));
                self.set_terminator(Terminator::Return { value, span: *span });
            }
            KernelStmt::Break { span } => {
                if let Some(target) = self.loop_stack.last() {
                    self.set_terminator(Terminator::Jump {
                        target: target.break_target,
                        span: *span,
                    });
                }
            }
            KernelStmt::Continue { span } => {
                if let Some(target) = self.loop_stack.last() {
                    self.set_terminator(Terminator::Jump {
                        target: target.continue_target,
                        span: *span,
                    });
                }
            }
        }
    }

    pub(crate) fn lower_kernel_expr(&mut self, expr: &KernelExpr) -> Value {
        match expr {
            KernelExpr::Literal { value, .. } => Value::Const(value.clone()),
            KernelExpr::Var { name, .. } => self
                .resolve_local(name)
                .map(Value::Local)
                .unwrap_or(Value::Const(Literal::Nil)),
            KernelExpr::Unary { op, expr, ty, span } => {
                let operand = self.lower_kernel_expr(expr);
                self.lower_unary_temp(mir_type_from_type(ty), *op, operand, *span)
            }
            KernelExpr::Binary {
                op,
                lhs,
                rhs,
                ty,
                span,
            } => {
                if matches!(op, BinaryOp::And | BinaryOp::Or) {
                    let lhs_val = self.lower_kernel_expr(lhs);
                    let eval_block = self.new_block();
                    let short_block = self.new_block();
                    let join_block = self.new_block();
                    let result_local = self.new_temp_local();
                    self.assign_use(
                        Place::Local(result_local),
                        Value::Const(Literal::Nil),
                        *span,
                    );
                    let (then_target, else_target) = if matches!(op, BinaryOp::And) {
                        (eval_block, short_block)
                    } else {
                        (short_block, eval_block)
                    };
                    self.set_terminator(Terminator::Branch {
                        cond: lhs_val.clone(),
                        then_target,
                        else_target,
                        span: *span,
                    });

                    self.current_block = short_block;
                    self.assign_use(Place::Local(result_local), lhs_val, *span);
                    if self.block_is_open(self.current_block) {
                        self.set_terminator(Terminator::Jump {
                            target: join_block,
                            span: *span,
                        });
                    }

                    self.current_block = eval_block;
                    let rhs_val = self.lower_kernel_expr(rhs);
                    self.assign_use(Place::Local(result_local), rhs_val, *span);
                    if self.block_is_open(self.current_block) {
                        self.set_terminator(Terminator::Jump {
                            target: join_block,
                            span: *span,
                        });
                    }

                    self.current_block = join_block;
                    return Value::Local(result_local);
                }
                let lhs = self.lower_kernel_expr(lhs);
                let rhs = self.lower_kernel_expr(rhs);
                self.lower_binary_temp(mir_type_from_type(ty), *op, lhs, rhs, *span)
            }
            KernelExpr::Crash { expr, span, .. } => {
                let value = self.lower_kernel_expr(expr);
                let temp = self.new_temp(MirType::Unknown);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(temp),
                    value: Rvalue::Crash { value },
                    span: *span,
                });
                Value::Temp(temp)
            }
            KernelExpr::Call {
                target,
                args,
                ty,
                span,
            } => {
                let args = args
                    .iter()
                    .map(|arg| self.lower_kernel_expr(arg))
                    .collect::<Vec<_>>();
                self.lower_call_temp(mir_type_from_type(ty), target.clone(), args, *span)
            }
            KernelExpr::Capture { target, span, .. } => {
                self.build_scene_capture_value(target, *span)
            }
            KernelExpr::DispatchBackend { backend, span, .. } => {
                let id = match backend {
                    DispatchBackend::Cpu => 0,
                    DispatchBackend::VirtualGpu => 1,
                    DispatchBackend::Wgsl => 2,
                    DispatchBackend::Auto => 3,
                };
                self.build_dispatch_backend_value(id, *span)
            }
            KernelExpr::CaptureQuery {
                plan,
                args,
                ty,
                span,
            } => {
                let args = args
                    .iter()
                    .map(|arg| self.lower_kernel_expr(arg))
                    .collect::<Vec<_>>();
                self.lower_call_temp(
                    mir_type_from_type(ty),
                    plan.helper_name.clone(),
                    args,
                    *span,
                )
            }
            KernelExpr::WorldQuery {
                plan,
                args,
                ty,
                span,
            } => {
                let query_arg_count = kernel_world_query_input_count(plan);
                let mut lowered_args = args
                    .iter()
                    .take(query_arg_count)
                    .map(|arg| self.lower_kernel_expr(arg))
                    .collect::<Vec<_>>();
                let backend = match args.get(query_arg_count) {
                    Some(backend) => {
                        let backend = self.lower_kernel_expr(backend);
                        self.lower_dispatch_backend_id(backend, *span)
                    }
                    None => Value::Const(Literal::Integer(Self::dispatch_backend_id(plan.backend))),
                };
                lowered_args.push(backend);
                self.lower_call_temp(
                    mir_type_from_type(ty),
                    plan.helper_name.clone(),
                    lowered_args,
                    *span,
                )
            }
            KernelExpr::BatchQuery {
                plan,
                args,
                ty,
                span,
            } => {
                let lowered_args = match args.split_last() {
                    Some((backend, query_args)) => {
                        let mut lowered_args = query_args
                            .iter()
                            .map(|arg| self.lower_kernel_expr(arg))
                            .collect::<Vec<_>>();
                        let backend = self.lower_kernel_expr(backend);
                        lowered_args.push(self.lower_dispatch_backend_id(backend, *span));
                        lowered_args
                    }
                    None => vec![Value::Const(Literal::Integer(Self::dispatch_backend_id(
                        plan.backend,
                    )))],
                };
                self.lower_call_temp(
                    mir_type_from_type(ty),
                    plan.helper_name.clone(),
                    lowered_args,
                    *span,
                )
            }
            KernelExpr::Member {
                base,
                member,
                ty,
                span,
            } => {
                let base_value = self.lower_kernel_expr(base);
                if let Some(component_index) =
                    vector_component_index(mir_type_from_type(base.ty()), member)
                {
                    let temp = self.new_temp(mir_type_from_type(ty));
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Temp(temp),
                        value: Rvalue::Call {
                            kind: CallKind::Sync,
                            target: CallTarget::Function(SmolStr::new("__wr_vec_component")),
                            args: vec![
                                base_value,
                                Value::Const(Literal::Integer(component_index as i64)),
                            ],
                        },
                        span: *span,
                    });
                    return Value::Temp(temp);
                }
                let slot = match base.ty() {
                    Type::Named(name, _) => self.field_slot(name.as_str(), member.as_str()),
                    _ => None,
                };
                let temp = self.new_temp(mir_type_from_type(ty));
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(temp),
                    value: Rvalue::GetField {
                        base: base_value,
                        field: member.clone(),
                        slot,
                    },
                    span: *span,
                });
                Value::Temp(temp)
            }
            KernelExpr::Index {
                base,
                index,
                ty,
                span,
            } => {
                let base_value = self.lower_kernel_expr(base);
                let index_value = self.lower_kernel_expr(index);
                let target_name = match base.ty() {
                    Type::Map(_, _) => "__wr_map_get",
                    _ => "__wr_list_get",
                };
                let temp = self.new_temp(mir_type_from_type(ty));
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(temp),
                    value: Rvalue::Call {
                        kind: CallKind::Sync,
                        target: CallTarget::Function(SmolStr::new(target_name)),
                        args: vec![base_value, index_value],
                    },
                    span: *span,
                });
                Value::Temp(temp)
            }
            KernelExpr::ArrayLiteral { items, span, .. } => {
                let values = items
                    .iter()
                    .map(|item| self.lower_kernel_expr(item))
                    .collect::<Vec<_>>();
                let temp = self.new_temp(MirType::Named(SmolStr::new("List")));
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(temp),
                    value: Rvalue::BuildList {
                        items: values,
                        alloc: AllocKind::LocalTemp,
                    },
                    span: *span,
                });
                Value::Temp(temp)
            }
            KernelExpr::StructLiteral {
                name, fields, span, ..
            } => {
                let mut class = self.synthetic_class_target_info(name.as_str());
                for (field_name, value) in fields {
                    let value = self.lower_kernel_expr(value);
                    Self::set_class_field_value(&mut class, field_name.as_str(), value);
                }
                self.build_class_instance(&class, *span)
            }
        }
    }
}
