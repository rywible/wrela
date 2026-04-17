//! Owns authored-statement lowering and block/control-flow construction.
//! Does not own module setup or standalone expression semantics.
//!
//! Key invariants:
//! - statement lowering must preserve authored control-flow shape even when MIR
//!   blocks are split or synthesized.
//! - block-open checks guard against emitting unreachable MIR into closed blocks.
//!
//! Primary entrypoints:
//! - `FunctionLowerer::lower_stmt_block`
//! - statement helpers on `FunctionLowerer`
//!
//! Failure modes / common pitfalls:
//! - continuing to emit statements after a block closes creates subtle CFG bugs
//!   that are hard to trace back to the authored source.

use super::*;

impl FunctionLowerer {
    pub(crate) fn lower_stmt_block(&mut self, body: &hir::Body, stmts: &[hir::Idx<HirStmt>]) {
        for stmt in stmts {
            if !self.block_is_open(self.current_block) {
                break;
            }
            self.lower_stmt(body, *stmt);
        }
    }

    pub(crate) fn lower_stmt(&mut self, body: &hir::Body, stmt_id: hir::Idx<HirStmt>) {
        let span = body.stmt_span(stmt_id);
        match &body.stmts[stmt_id] {
            HirStmt::Expr(expr) => {
                let _ = self.lower_expr(body, *expr);
            }
            HirStmt::Assert {
                kind,
                expr,
                rhs,
                tolerance,
            } => {
                let cond = self.lower_assert_expr(body, *expr, *rhs, *kind, *tolerance);
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
                objective,
                body: optimize_body,
                ..
            } => {
                self.objective_stack.push(*objective);
                self.enter_scope();
                self.lower_stmt_block(body, optimize_body);
                self.exit_scope();
                let popped = self.objective_stack.pop();
                debug_assert_eq!(popped, Some(*objective));
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
                value_name,
                key_name,
                index_name,
                iterable,
                body: loop_body,
            } => {
                if let Expr::Binary {
                    lhs,
                    op: BinaryOp::Range,
                    rhs,
                    ..
                } = &body.exprs[*iterable]
                    && key_name.is_none()
                    && self
                        .lower_range_for(body, value_name, index_name, *lhs, *rhs, loop_body, span)
                {
                    return;
                }

                let iterable_value = self.lower_expr(body, *iterable);
                let iter_temp = self.new_temp(MirType::Unknown);
                self.push_stmt(MirStmt::IterInit {
                    dst: Place::Temp(iter_temp),
                    iterable: iterable_value.clone(),
                    span,
                });
                let iter_count_local = index_name.as_ref().map(|_| {
                    self.new_local(
                        SmolStr::new(format!("$iter_count{}", self.locals.len())),
                        true,
                        MirType::Integer,
                    )
                });
                if let Some(iter_count_local) = iter_count_local {
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Local(iter_count_local),
                        value: Rvalue::Use(Value::Const(Literal::Integer(0))),
                        span,
                    });
                }

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
                if let Some(key_name) = key_name {
                    let key_local = self.new_local(key_name.clone(), false, MirType::Unknown);
                    self.declare_local(key_name.clone(), key_local);
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Local(key_local),
                        value: Rvalue::Use(Value::Temp(value_temp)),
                        span,
                    });
                    let map_get_temp = self.new_temp(MirType::Unknown);
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Temp(map_get_temp),
                        value: Rvalue::Call {
                            kind: CallKind::Sync,
                            target: CallTarget::Function(SmolStr::new("__wr_map_get")),
                            args: vec![iterable_value.clone(), Value::Local(key_local)],
                        },
                        span,
                    });
                    let value_local = self.new_local(value_name.clone(), false, MirType::Unknown);
                    self.declare_local(value_name.clone(), value_local);
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Local(value_local),
                        value: Rvalue::Use(Value::Temp(map_get_temp)),
                        span,
                    });
                } else {
                    let local = self.new_local(value_name.clone(), false, MirType::Unknown);
                    self.declare_local(value_name.clone(), local);
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Local(local),
                        value: Rvalue::Use(Value::Temp(value_temp)),
                        span,
                    });
                }
                if let Some(index_name) = index_name {
                    let index_local = self.new_local(index_name.clone(), false, MirType::Integer);
                    self.declare_local(index_name.clone(), index_local);
                    if let Some(iter_count_local) = iter_count_local {
                        self.push_stmt(MirStmt::Assign {
                            place: Place::Local(index_local),
                            value: Rvalue::Use(Value::Local(iter_count_local)),
                            span,
                        });
                    }
                }
                self.loop_stack.push(LoopTarget {
                    break_target: exit_block,
                    continue_target: head_block,
                });
                self.lower_stmt_block(body, loop_body);
                self.loop_stack.pop();
                self.exit_scope();
                if self.block_is_open(self.current_block) {
                    if let Some(iter_count_local) = iter_count_local {
                        let next_count = self.new_temp(MirType::Integer);
                        self.push_stmt(MirStmt::Assign {
                            place: Place::Temp(next_count),
                            value: Rvalue::Binary {
                                op: BinaryOp::Add,
                                lhs: Value::Local(iter_count_local),
                                rhs: Value::Const(Literal::Integer(1)),
                            },
                            span,
                        });
                        self.push_stmt(MirStmt::Assign {
                            place: Place::Local(iter_count_local),
                            value: Rvalue::Use(Value::Temp(next_count)),
                            span,
                        });
                    }
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

    pub(crate) fn lower_range_for(
        &mut self,
        body: &hir::Body,
        value_name: &SmolStr,
        index_name: &Option<SmolStr>,
        lhs: hir::Idx<Expr>,
        rhs: hir::Idx<Expr>,
        loop_body: &[hir::Idx<hir::Stmt>],
        span: TextRange,
    ) -> bool {
        if env::var_os("WRELA_DISABLE_TYPED_RANGE_FASTPATH").is_some() {
            return false;
        }
        let lhs_ty = self.expr_type(body, lhs);
        let rhs_ty = self.expr_type(body, rhs);
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
            let loop_var = self.new_local(value_name.clone(), false, induction_ty.clone());
            let loop_index = index_name
                .as_ref()
                .map(|name| self.new_local(name.clone(), false, MirType::Integer));
            let iter_count = self.new_local(
                SmolStr::new(format!("$range_count{}", self.locals.len())),
                true,
                MirType::Integer,
            );
            let head_block = self.new_block();
            let body_block = self.new_block();
            let exit_block = self.new_block();
            self.push_stmt(MirStmt::Assign {
                place: Place::Local(idx_local),
                value: Rvalue::Use(start_val.clone()),
                span,
            });
            self.push_stmt(MirStmt::Assign {
                place: Place::Local(iter_count),
                value: Rvalue::Use(Value::Const(Literal::Integer(0))),
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
            self.declare_local(value_name.clone(), loop_var);
            self.push_stmt(MirStmt::Assign {
                place: Place::Local(loop_var),
                value: Rvalue::Use(Value::Local(idx_local)),
                span,
            });
            if let Some(loop_index) = loop_index {
                if let Some(index_name) = index_name {
                    self.declare_local(index_name.clone(), loop_index);
                }
                self.push_stmt(MirStmt::Assign {
                    place: Place::Local(loop_index),
                    value: Rvalue::Use(Value::Local(iter_count)),
                    span,
                });
            }
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
                let count_temp = self.new_temp(MirType::Integer);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(count_temp),
                    value: Rvalue::Binary {
                        op: BinaryOp::Add,
                        lhs: Value::Local(iter_count),
                        rhs: Value::Const(Literal::Integer(1)),
                    },
                    span,
                });
                self.push_stmt(MirStmt::Assign {
                    place: Place::Local(iter_count),
                    value: Rvalue::Use(Value::Temp(count_temp)),
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

        let loop_var = self.new_local(value_name.clone(), false, induction_ty.clone());
        let loop_index = index_name
            .as_ref()
            .map(|name| self.new_local(name.clone(), false, MirType::Integer));
        let iter_count = self.new_local(
            SmolStr::new(format!("$range_count{}", self.locals.len())),
            true,
            MirType::Integer,
        );

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
        self.push_stmt(MirStmt::Assign {
            place: Place::Local(iter_count),
            value: Rvalue::Use(Value::Const(Literal::Integer(0))),
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
        self.push_stmt(MirStmt::Assign {
            place: Place::Local(iter_count),
            value: Rvalue::Use(Value::Const(Literal::Integer(0))),
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
        self.declare_local(value_name.clone(), loop_var);
        self.push_stmt(MirStmt::Assign {
            place: Place::Local(loop_var),
            value: Rvalue::Use(Value::Local(idx_local)),
            span,
        });
        if let Some(loop_index) = loop_index {
            if let Some(index_name) = index_name {
                self.declare_local(index_name.clone(), loop_index);
            }
            self.push_stmt(MirStmt::Assign {
                place: Place::Local(loop_index),
                value: Rvalue::Use(Value::Local(iter_count)),
                span,
            });
        }
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
            let count_temp = self.new_temp(MirType::Integer);
            self.push_stmt(MirStmt::Assign {
                place: Place::Temp(count_temp),
                value: Rvalue::Binary {
                    op: BinaryOp::Add,
                    lhs: Value::Local(iter_count),
                    rhs: Value::Const(Literal::Integer(1)),
                },
                span,
            });
            self.push_stmt(MirStmt::Assign {
                place: Place::Local(iter_count),
                value: Rvalue::Use(Value::Temp(count_temp)),
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

    pub(crate) fn lower_assert_expr(
        &mut self,
        body: &hir::Body,
        expr_id: hir::Idx<Expr>,
        rhs_id: Option<hir::Idx<Expr>>,
        kind: hir::AssertKind,
        tolerance: Option<hir::Idx<Expr>>,
    ) -> Value {
        if matches!(kind, hir::AssertKind::Approx) {
            let span = body.expr_span(expr_id);
            let left_id = match &body.exprs[expr_id] {
                Expr::Binary { lhs, .. } => *lhs,
                _ => expr_id,
            };
            let left = self.lower_expr(body, left_id);
            let right = rhs_id
                .map(|rhs| self.lower_expr(body, rhs))
                .unwrap_or(Value::Const(Literal::Nil));
            let tol = tolerance
                .map(|tol| self.lower_expr(body, tol))
                .unwrap_or(Value::Const(Literal::Nil));
            let temp = self.new_temp(MirType::Boolean);
            self.push_stmt(MirStmt::Assign {
                place: Place::Temp(temp),
                value: Rvalue::Call {
                    kind: CallKind::Sync,
                    target: CallTarget::Function(SmolStr::new("approx_eq")),
                    args: vec![left, right, tol],
                },
                span,
            });
            return Value::Temp(temp);
        }
        let span = body.expr_span(expr_id);
        if let Expr::Binary { lhs, op, rhs, .. } = &body.exprs[expr_id]
            && matches!(op, BinaryOp::Eq | BinaryOp::Ne)
        {
            let left = self.lower_expr(body, *lhs);
            let right = self.lower_expr(body, *rhs);
            let func = match kind {
                hir::AssertKind::Value => SmolStr::new("value_deep_eq"),
                hir::AssertKind::Identity => SmolStr::new("identity_eq"),
                hir::AssertKind::Approx => unreachable!(),
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
        self.lower_expr(body, expr_id)
    }

    pub(crate) fn lower_case_label(&mut self, pattern: &hir::Pattern) -> Option<SwitchCase> {
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

    pub(crate) fn match_has_result_patterns(&self, cases: &[hir::MatchCase]) -> bool {
        cases.iter().any(|case| {
            case.labels
                .iter()
                .any(|label| self.result_pattern_kind(label).is_some())
        })
    }

    pub(crate) fn result_pattern_kind(&self, pattern: &hir::Pattern) -> Option<bool> {
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

    pub(crate) fn lower_result_match(
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

    pub(crate) fn bind_pattern(
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
                if parts.len() == 1 {
                    let class_name = parts[0].clone();
                    if let Some(fields) = self.class_fields.get(&class_name).cloned() {
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
                        return;
                    }
                    if let Some(arg) = args.first() {
                        self.bind_pattern(body, arg, value, span);
                    }
                    return;
                }
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
            hir::Pattern::Struct {
                parts,
                fields: pattern_fields,
            } => {
                let class_name = if parts.len() == 1 {
                    Some(parts[0].clone())
                } else if parts.len() == 2 {
                    Some(SmolStr::new(format!("{}.{}", parts[0], parts[1])))
                } else {
                    None
                };
                let Some(class_name) = class_name else {
                    return;
                };
                let Some(fields) = self.class_fields.get(&class_name).cloned() else {
                    return;
                };
                for (field_name, field_pattern) in pattern_fields {
                    let Some(idx) = fields.iter().position(|f| f == field_name) else {
                        continue;
                    };
                    let temp = self.new_temp(MirType::Unknown);
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Temp(temp),
                        value: Rvalue::GetField {
                            base: value.clone(),
                            field: field_name.clone(),
                            slot: Some(idx as u32),
                        },
                        span,
                    });
                    self.bind_pattern(body, field_pattern, Value::Temp(temp), span);
                }
            }
        }
    }

    pub(crate) fn bind_result_pattern(
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
}
