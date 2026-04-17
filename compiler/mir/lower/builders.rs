//! Owns `FunctionLowerer` helper builders for classes, pools, and synthesized
//! MIR values used during lowering.
//! Does not own statement/expression traversal orchestration.
//!
//! Key invariants:
//! - synthesized builder output must preserve authored type/tag identity.
//! - helper construction here must stay consistent with the portable/kernel ABI
//!   assumptions other lowering files rely on.
//!
//! Primary entrypoints:
//! - builder helpers on `FunctionLowerer`
//!
//! Failure modes / common pitfalls:
//! - constructing partially typed helper values here causes later lowering
//!   stages to fail far from the original authored site.

use super::*;

impl FunctionLowerer {
    pub(crate) fn class_target_info(
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
            Expr::Call { callee, args, .. } => {
                let Expr::Variable(name) = &body.exprs[*callee] else {
                    return None;
                };
                let class_name = builtin_record_by_function(name.as_str())
                    .map(|record| SmolStr::new(record.name))
                    .unwrap_or_else(|| name.clone());
                let class_id = self.type_tags.get(&class_name).copied()?;
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
                Some(ClassTargetInfo {
                    name: class_name,
                    class_id,
                    fields,
                    field_defaults,
                    field_values,
                })
            }
            _ => None,
        }
    }

    pub(crate) fn build_class_instance(
        &mut self,
        class: &ClassTargetInfo,
        span: TextRange,
    ) -> Value {
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
                && let Some(default) = default
            {
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
        if has_explicit_fields {
            self.maybe_call_configure(&class.name, Value::Temp(temp), span);
        }
        Value::Temp(temp)
    }

    pub(crate) fn build_ray_query_value(
        &mut self,
        origin: Value,
        direction: Value,
        max_distance: Value,
        min_step: Value,
        hit_epsilon: Value,
        max_steps: Value,
        span: TextRange,
    ) -> Value {
        let mut class = self.synthetic_class_target_info("RayQuery");
        Self::set_class_field_value(&mut class, "origin", origin);
        Self::set_class_field_value(&mut class, "direction", direction);
        Self::set_class_field_value(&mut class, "max_distance", max_distance);
        Self::set_class_field_value(&mut class, "min_step", min_step);
        Self::set_class_field_value(&mut class, "hit_epsilon", hit_epsilon);
        Self::set_class_field_value(&mut class, "max_steps", max_steps);
        self.build_class_instance(&class, span)
    }

    pub(crate) fn lower_field_default(
        &mut self,
        default: &hir::FieldDefault,
        span: TextRange,
    ) -> Value {
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

    pub(crate) fn maybe_call_configure(
        &mut self,
        class_name: &SmolStr,
        receiver: Value,
        span: TextRange,
    ) {
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

    pub(crate) fn parse_pool_of(
        &self,
        body: &hir::Body,
        expr_id: hir::Idx<Expr>,
    ) -> Option<PoolOfSpec> {
        let (callee, args) = match &body.exprs[expr_id] {
            Expr::Call { callee, args, .. } => (callee, args),
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

    pub(crate) fn field_slot(&self, class_name: &str, field_name: &str) -> Option<u32> {
        self.class_fields
            .get(&SmolStr::new(class_name))
            .and_then(|fields| fields.iter().position(|field| field.as_str() == field_name))
            .map(|idx| idx as u32)
    }

    pub(crate) fn assign_use(&mut self, place: Place, value: Value, span: TextRange) {
        self.push_stmt(MirStmt::Assign {
            place,
            value: Rvalue::Use(value),
            span,
        });
    }

    pub(crate) fn lower_binary_temp(
        &mut self,
        ty: MirType,
        op: BinaryOp,
        lhs: Value,
        rhs: Value,
        span: TextRange,
    ) -> Value {
        let temp = self.new_temp(ty);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(temp),
            value: Rvalue::Binary { op, lhs, rhs },
            span,
        });
        Value::Temp(temp)
    }

    pub(crate) fn lower_unary_temp(
        &mut self,
        ty: MirType,
        op: hir::UnaryOp,
        operand: Value,
        span: TextRange,
    ) -> Value {
        let temp = self.new_temp(ty);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(temp),
            value: Rvalue::Unary { op, operand },
            span,
        });
        Value::Temp(temp)
    }

    pub(crate) fn lower_call_temp(
        &mut self,
        ty: MirType,
        target: SmolStr,
        args: Vec<Value>,
        span: TextRange,
    ) -> Value {
        let temp = self.new_temp(ty);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(temp),
            value: Rvalue::Call {
                kind: CallKind::Sync,
                target: CallTarget::Function(target),
                args,
            },
            span,
        });
        Value::Temp(temp)
    }

    pub(crate) fn lower_string_interp_temp(
        &mut self,
        parts: Vec<StringPartValue>,
        span: TextRange,
    ) -> Value {
        let temp = self.new_temp(MirType::String);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(temp),
            value: Rvalue::StringInterp {
                parts,
                alloc: AllocKind::LocalTemp,
            },
            span,
        });
        Value::Temp(temp)
    }

    pub(crate) fn lower_string_concat_temp(
        &mut self,
        lhs: Value,
        rhs: Value,
        span: TextRange,
    ) -> Value {
        let temp = self.new_temp(MirType::String);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(temp),
            value: Rvalue::StrConcat {
                parts: vec![lhs, rhs],
                alloc: AllocKind::LocalTemp,
            },
            span,
        });
        Value::Temp(temp)
    }

    pub(crate) fn synthetic_class_target_info(&self, class_name: &str) -> ClassTargetInfo {
        let name = SmolStr::new(class_name);
        let fields = self.class_fields.get(&name).cloned().unwrap_or_default();
        let field_defaults = self
            .class_field_defaults
            .get(&name)
            .cloned()
            .unwrap_or_else(|| vec![None; fields.len()]);
        let field_values = vec![None; fields.len()];
        let class_id = self.type_tags.get(&name).copied().unwrap_or(TypeTagId(0));
        ClassTargetInfo {
            name,
            class_id,
            fields,
            field_defaults,
            field_values,
        }
    }

    pub(crate) fn set_class_field_value(
        class: &mut ClassTargetInfo,
        field_name: &str,
        value: Value,
    ) {
        if let Some(idx) = class
            .fields
            .iter()
            .position(|field| field.as_str() == field_name)
        {
            class.field_values[idx] = Some(value);
        }
    }

    pub(crate) fn set_class_field_value_at(
        class: &mut ClassTargetInfo,
        index: usize,
        value: Value,
    ) {
        if index < class.field_values.len() {
            class.field_values[index] = Some(value);
        }
    }

    pub(crate) fn lower_dispatch_compute_call(
        &mut self,
        body: &hir::Body,
        span: TextRange,
        spec: &ParsedKernelDispatch,
    ) -> Value {
        let workgroups_x = self.lower_expr(body, spec.workgroups[0]);
        let workgroups_y = self.lower_expr(body, spec.workgroups[1]);
        let workgroups_z = self.lower_expr(body, spec.workgroups[2]);
        let workgroup_size_x = self.lower_expr(body, spec.workgroup_size[0]);
        let workgroup_size_y = self.lower_expr(body, spec.workgroup_size[1]);
        let workgroup_size_z = self.lower_expr(body, spec.workgroup_size[2]);
        let schedule = spec
            .schedule
            .map(|expr| self.lower_expr(body, expr))
            .unwrap_or(Value::Const(Literal::Nil));
        let kernel_args = spec
            .kernel_args
            .iter()
            .map(|expr| self.lower_expr(body, *expr))
            .collect::<Vec<_>>();

        let workgroups_x_local = self.new_local(
            SmolStr::new(format!("$gpu_workgroups_x{}", self.locals.len())),
            false,
            MirType::Integer,
        );
        let workgroups_y_local = self.new_local(
            SmolStr::new(format!("$gpu_workgroups_y{}", self.locals.len())),
            false,
            MirType::Integer,
        );
        let workgroups_z_local = self.new_local(
            SmolStr::new(format!("$gpu_workgroups_z{}", self.locals.len())),
            false,
            MirType::Integer,
        );
        let workgroup_size_x_local = self.new_local(
            SmolStr::new(format!("$gpu_workgroup_size_x{}", self.locals.len())),
            false,
            MirType::Integer,
        );
        let workgroup_size_y_local = self.new_local(
            SmolStr::new(format!("$gpu_workgroup_size_y{}", self.locals.len())),
            false,
            MirType::Integer,
        );
        let workgroup_size_z_local = self.new_local(
            SmolStr::new(format!("$gpu_workgroup_size_z{}", self.locals.len())),
            false,
            MirType::Integer,
        );
        for (local, value) in [
            (workgroups_x_local, workgroups_x),
            (workgroups_y_local, workgroups_y),
            (workgroups_z_local, workgroups_z),
            (workgroup_size_x_local, workgroup_size_x),
            (workgroup_size_y_local, workgroup_size_y),
            (workgroup_size_z_local, workgroup_size_z),
        ] {
            self.push_stmt(MirStmt::Assign {
                place: Place::Local(local),
                value: Rvalue::Use(value),
                span,
            });
        }

        let total_x_local = self.new_local(
            SmolStr::new(format!("$gpu_total_x{}", self.locals.len())),
            false,
            MirType::Integer,
        );
        let total_y_local = self.new_local(
            SmolStr::new(format!("$gpu_total_y{}", self.locals.len())),
            false,
            MirType::Integer,
        );
        let total_z_local = self.new_local(
            SmolStr::new(format!("$gpu_total_z{}", self.locals.len())),
            false,
            MirType::Integer,
        );
        let total_xy_local = self.new_local(
            SmolStr::new(format!("$gpu_total_xy{}", self.locals.len())),
            false,
            MirType::Integer,
        );
        let total_count_local = self.new_local(
            SmolStr::new(format!("$gpu_total_count{}", self.locals.len())),
            false,
            MirType::Integer,
        );
        for (local, lhs, rhs) in [
            (
                total_x_local,
                Value::Local(workgroups_x_local),
                Value::Local(workgroup_size_x_local),
            ),
            (
                total_y_local,
                Value::Local(workgroups_y_local),
                Value::Local(workgroup_size_y_local),
            ),
            (
                total_z_local,
                Value::Local(workgroups_z_local),
                Value::Local(workgroup_size_z_local),
            ),
        ] {
            let temp = self.new_temp(MirType::Integer);
            self.push_stmt(MirStmt::Assign {
                place: Place::Temp(temp),
                value: Rvalue::Binary {
                    op: BinaryOp::Mul,
                    lhs,
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
        let total_xy_temp = self.new_temp(MirType::Integer);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(total_xy_temp),
            value: Rvalue::Binary {
                op: BinaryOp::Mul,
                lhs: Value::Local(total_x_local),
                rhs: Value::Local(total_y_local),
            },
            span,
        });
        self.push_stmt(MirStmt::Assign {
            place: Place::Local(total_xy_local),
            value: Rvalue::Use(Value::Temp(total_xy_temp)),
            span,
        });
        let total_count_temp = self.new_temp(MirType::Integer);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(total_count_temp),
            value: Rvalue::Binary {
                op: BinaryOp::Mul,
                lhs: Value::Local(total_xy_local),
                rhs: Value::Local(total_z_local),
            },
            span,
        });
        self.push_stmt(MirStmt::Assign {
            place: Place::Local(total_count_local),
            value: Rvalue::Use(Value::Temp(total_count_temp)),
            span,
        });

        let dispatch_begin = self.new_temp(MirType::Nil);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(dispatch_begin),
            value: Rvalue::Call {
                kind: CallKind::Sync,
                target: CallTarget::Function(SmolStr::new("__wr_gpu_dispatch_begin")),
                args: vec![
                    Value::Local(workgroups_x_local),
                    Value::Local(workgroups_y_local),
                    Value::Local(workgroups_z_local),
                    Value::Local(workgroup_size_x_local),
                    Value::Local(workgroup_size_y_local),
                    Value::Local(workgroup_size_z_local),
                    schedule,
                ],
            },
            span,
        });

        let loop_index_local = self.new_local(
            SmolStr::new(format!("$gpu_linear_index{}", self.locals.len())),
            true,
            MirType::Integer,
        );
        self.push_stmt(MirStmt::Assign {
            place: Place::Local(loop_index_local),
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
                lhs: Value::Local(loop_index_local),
                rhs: Value::Local(total_count_local),
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
        let dispatch_select = self.new_temp(MirType::Nil);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(dispatch_select),
            value: Rvalue::Call {
                kind: CallKind::Sync,
                target: CallTarget::Function(SmolStr::new("__wr_gpu_dispatch_select_invocation")),
                args: vec![Value::Local(loop_index_local)],
            },
            span,
        });

        let kernel_result = self.new_temp(MirType::Nil);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(kernel_result),
            value: Rvalue::Call {
                kind: CallKind::Sync,
                target: CallTarget::Function(spec.kernel.clone()),
                args: kernel_args,
            },
            span,
        });

        let next_index = self.new_temp(MirType::Integer);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(next_index),
            value: Rvalue::Binary {
                op: BinaryOp::Add,
                lhs: Value::Local(loop_index_local),
                rhs: Value::Const(Literal::Integer(1)),
            },
            span,
        });
        self.push_stmt(MirStmt::Assign {
            place: Place::Local(loop_index_local),
            value: Rvalue::Use(Value::Temp(next_index)),
            span,
        });
        self.set_terminator(Terminator::Jump {
            target: head_block,
            span,
        });

        self.current_block = exit_block;
        let dispatch_end = self.new_temp(MirType::Nil);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(dispatch_end),
            value: Rvalue::Call {
                kind: CallKind::Sync,
                target: CallTarget::Function(SmolStr::new("__wr_gpu_dispatch_end")),
                args: Vec::new(),
            },
            span,
        });
        Value::Const(Literal::Nil)
    }
}
