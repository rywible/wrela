fn infer_expr(
    body: &Body,
    expr_id: Idx<Expr>,
    ctx: &mut TypeContext,
    classes: &ClassIndex,
    enums: &EnumIndex,
    interfaces: &InterfaceIndex,
    functions: &FunctionIndex,
    errors: &mut Vec<TypeError>,
    allow_pending: bool,
    allow_result: bool,
    in_result_fn: bool,
) -> Type {
    let ty = match &body.exprs[expr_id] {
        Expr::Literal(lit) => literal_type(lit),
        Expr::Variable(name) => ctx.resolve(name).unwrap_or(Type::Unknown),
        Expr::Detach { target, .. } => actor_type_for_detach_target(body, *target, classes),
        Expr::Unary { op, expr, op_span } => {
            let allow_pending_operand = matches!(op, UnaryOp::Await | UnaryOp::Fire);
            let allow_result_operand = allow_result || matches!(op, UnaryOp::Try);
            let operand = infer_expr(
                body,
                *expr,
                ctx,
                classes,
                enums,
                interfaces,
                functions,
                errors,
                allow_pending_operand,
                allow_result_operand,
                in_result_fn,
            );
            if matches!(op, UnaryOp::Err) && !in_result_fn {
                errors.push(TypeError::ErrOutsideResult {
                    span: span_from_range(*op_span),
                });
                Type::Unknown
            } else if matches!(op, UnaryOp::Try) && !in_result_fn {
                errors.push(TypeError::TryOutsideResult {
                    span: span_from_range(*op_span),
                });
                Type::Unknown
            } else if matches!(op, UnaryOp::Try) {
                match operand {
                    Type::Result(ok, _err) => *ok,
                    Type::Unknown => Type::Unknown,
                    _ => {
                        errors.push(TypeError::InvalidTryOperand {
                            span: span_from_range(*op_span),
                        });
                        Type::Unknown
                    }
                }
            } else if operand != Type::Unknown && !valid_unary(*op, &operand) {
                errors.push(TypeError::InvalidUnaryOperand {
                    op: unary_op_label(*op),
                    span: span_from_range(*op_span),
                });
                Type::Unknown
            } else if matches!(op, UnaryOp::Spawn) {
                actor_type_for_detach_target(body, *expr, classes)
            } else if matches!(op, UnaryOp::Await) {
                match operand {
                    Type::Pending(inner) => match *inner {
                        Type::Result(ok, err) => Type::Result(ok, err),
                        other => Type::Result(Box::new(other), Box::new(error_type())),
                    },
                    Type::Unknown => Type::Unknown,
                    _ => {
                        errors.push(TypeError::InvalidAwaitOperand {
                            span: span_from_range(*op_span),
                        });
                        Type::Unknown
                    }
                }
            } else if matches!(op, UnaryOp::Fire) {
                match operand {
                    Type::Pending(_) => Type::Nil,
                    Type::Unknown => Type::Unknown,
                    _ => {
                        errors.push(TypeError::InvalidFireOperand {
                            span: span_from_range(*op_span),
                        });
                        Type::Unknown
                    }
                }
            } else {
                unary_result(*op, &operand)
            }
        }
        Expr::Binary {
            lhs,
            op,
            rhs,
            op_span,
        } => {
            if matches!(
                op,
                BinaryOp::Assign
                    | BinaryOp::AddAssign
                    | BinaryOp::SubAssign
                    | BinaryOp::MulAssign
                    | BinaryOp::DivAssign
            ) && let Expr::Index {
                object,
                index,
                index_span,
            } = &body.exprs[*lhs]
            {
                let object_ty = infer_expr(
                    body,
                    *object,
                    ctx,
                    classes,
                    enums,
                    interfaces,
                    functions,
                    errors,
                    false,
                    allow_result,
                    in_result_fn,
                );
                let index_ty = infer_expr(
                    body,
                    *index,
                    ctx,
                    classes,
                    enums,
                    interfaces,
                    functions,
                    errors,
                    false,
                    allow_result,
                    in_result_fn,
                );
                let right_ty = infer_expr(
                    body,
                    *rhs,
                    ctx,
                    classes,
                    enums,
                    interfaces,
                    functions,
                    errors,
                    false,
                    allow_result,
                    in_result_fn,
                );
                match object_ty {
                    Type::List(inner_ty) => {
                        if types_known(&Type::Integer, &index_ty) && index_ty != Type::Integer {
                            errors.push(TypeError::InvalidIndexType {
                                expected: "Integer".to_string(),
                                found: type_label(&index_ty),
                                span: span_from_range(*index_span),
                            });
                        }
                        if matches!(op, BinaryOp::Assign) {
                            if types_known(&inner_ty, &right_ty)
                                && !is_assignable(&inner_ty, &right_ty, classes, interfaces)
                            {
                                errors.push(TypeError::InvalidAssignment {
                                    name: SmolStr::new("index"),
                                    expected: type_label(&inner_ty),
                                    found: type_label(&right_ty),
                                    span: span_from_range(*op_span),
                                });
                            }
                        } else if types_known(&inner_ty, &right_ty)
                            && !valid_binary(binary_from_assign(*op), &inner_ty, &right_ty)
                        {
                            errors.push(TypeError::InvalidBinaryOperands {
                                op: binary_op_label(binary_from_assign(*op)),
                                span: span_from_range(*op_span),
                            });
                        }
                    }
                    Type::Map(key_ty, value_ty) => {
                        if types_known(&key_ty, &index_ty)
                            && !is_assignable(&key_ty, &index_ty, classes, interfaces)
                        {
                            errors.push(TypeError::InvalidIndexType {
                                expected: type_label(&key_ty),
                                found: type_label(&index_ty),
                                span: span_from_range(*index_span),
                            });
                        }
                        if matches!(op, BinaryOp::Assign) {
                            if types_known(&value_ty, &right_ty)
                                && !is_assignable(&value_ty, &right_ty, classes, interfaces)
                            {
                                errors.push(TypeError::InvalidAssignment {
                                    name: SmolStr::new("index"),
                                    expected: type_label(&value_ty),
                                    found: type_label(&right_ty),
                                    span: span_from_range(*op_span),
                                });
                            }
                        } else if types_known(&value_ty, &right_ty)
                            && !valid_binary(binary_from_assign(*op), &value_ty, &right_ty)
                        {
                            errors.push(TypeError::InvalidBinaryOperands {
                                op: binary_op_label(binary_from_assign(*op)),
                                span: span_from_range(*op_span),
                            });
                        }
                    }
                    Type::Unknown => {}
                    _ => errors.push(TypeError::InvalidIndexTarget {
                        span: span_from_range(*index_span),
                    }),
                }
                return Type::Unknown;
            }
            if matches!(
                op,
                BinaryOp::Assign
                    | BinaryOp::AddAssign
                    | BinaryOp::SubAssign
                    | BinaryOp::MulAssign
                    | BinaryOp::DivAssign
            ) && let Expr::Member { object, member, .. } = &body.exprs[*lhs]
            {
                let object_ty = infer_expr(
                    body,
                    *object,
                    ctx,
                    classes,
                    enums,
                    interfaces,
                    functions,
                    errors,
                    false,
                    allow_result,
                    in_result_fn,
                );
                let right_ty = infer_expr(
                    body,
                    *rhs,
                    ctx,
                    classes,
                    enums,
                    interfaces,
                    functions,
                    errors,
                    false,
                    allow_result,
                    in_result_fn,
                );
                if let Type::Actor(_) = object_ty {
                    errors.push(TypeError::ActorMemberAccess {
                        member: member.clone(),
                        span: span_from_range(*op_span),
                    });
                    return Type::Unknown;
                }
                if let Type::Named(class_name, class_args) = object_ty {
                    if interfaces.is_interface(&class_name) {
                        errors.push(TypeError::UnknownMember {
                            object: class_name.to_string(),
                            member: member.clone(),
                            span: span_from_range(*op_span),
                        });
                    } else if let Some(class) = classes.get(&class_name) {
                        let subst = class_subst(class, &class_args);
                        if let Some(field_ty) = class.fields.get(member) {
                            let field_mutable =
                                class.field_mutable.get(member).copied().unwrap_or(false);
                            if !field_mutable {
                                errors.push(TypeError::ImmutableFieldAssign {
                                    member: member.clone(),
                                    span: span_from_range(*op_span),
                                    help: "Mark the field as mutable in the `has` block."
                                        .to_string(),
                                });
                            }
                            let field_ty = substitute_type(field_ty, &subst);
                            if matches!(op, BinaryOp::Assign) {
                                if types_known(&field_ty, &right_ty)
                                    && !is_assignable(&field_ty, &right_ty, classes, interfaces)
                                {
                                    errors.push(TypeError::InvalidAssignment {
                                        name: member.clone(),
                                        expected: type_label(&field_ty),
                                        found: type_label(&right_ty),
                                        span: span_from_range(*op_span),
                                    });
                                }
                            } else if types_known(&field_ty, &right_ty)
                                && !valid_binary(binary_from_assign(*op), &field_ty, &right_ty)
                            {
                                errors.push(TypeError::InvalidBinaryOperands {
                                    op: binary_op_label(binary_from_assign(*op)),
                                    span: span_from_range(*op_span),
                                });
                            }
                        } else if class.methods.contains_key(member) {
                            errors.push(TypeError::InvalidAssignment {
                                name: member.clone(),
                                expected: "field".to_string(),
                                found: "method".to_string(),
                                span: span_from_range(*op_span),
                            });
                        } else {
                            errors.push(TypeError::UnknownMember {
                                object: class_name.to_string(),
                                member: member.clone(),
                                span: span_from_range(*op_span),
                            });
                        }
                    }
                }
                return Type::Unknown;
            }
            if matches!(op, BinaryOp::Otherwise) {
                let left = infer_expr(
                    body,
                    *lhs,
                    ctx,
                    classes,
                    enums,
                    interfaces,
                    functions,
                    errors,
                    false,
                    true,
                    in_result_fn,
                );
                let right = infer_expr(
                    body,
                    *rhs,
                    ctx,
                    classes,
                    enums,
                    interfaces,
                    functions,
                    errors,
                    false,
                    allow_result,
                    in_result_fn,
                );
                match left {
                    Type::Result(ok, _err) => {
                        if types_known(&ok, &right)
                            && !is_assignable(&ok, &right, classes, interfaces)
                        {
                            errors.push(TypeError::InvalidBinaryOperands {
                                op: binary_op_label(*op),
                                span: span_from_range(*op_span),
                            });
                        }
                        if matches!(*ok, Type::Unknown) {
                            right
                        } else {
                            *ok
                        }
                    }
                    Type::Unknown => Type::Unknown,
                    _ => {
                        errors.push(TypeError::InvalidOtherwiseOperand {
                            span: span_from_range(*op_span),
                        });
                        Type::Unknown
                    }
                }
            } else {
                let left = infer_expr(
                    body,
                    *lhs,
                    ctx,
                    classes,
                    enums,
                    interfaces,
                    functions,
                    errors,
                    false,
                    allow_result,
                    in_result_fn,
                );
                let right = infer_expr(
                    body,
                    *rhs,
                    ctx,
                    classes,
                    enums,
                    interfaces,
                    functions,
                    errors,
                    false,
                    allow_result,
                    in_result_fn,
                );
                if matches!(op, BinaryOp::Eq | BinaryOp::Ne)
                    && types_known(&left, &right)
                    && !valid_equality_operands(&left, &right, classes, enums, interfaces)
                {
                    errors.push(TypeError::EqualityRequiresEq {
                        left: type_label(&left),
                        right: type_label(&right),
                        span: span_from_range(*op_span),
                    });
                    return Type::Unknown;
                }
                if types_known(&left, &right) && !valid_binary(*op, &left, &right) {
                    errors.push(TypeError::InvalidBinaryOperands {
                        op: binary_op_label(*op),
                        span: span_from_range(*op_span),
                    });
                    Type::Unknown
                } else {
                    binary_result(*op, &left, &right)
                }
            }
        }
        Expr::TypeApply { callee, type_args } => {
            let _ = infer_expr(
                body,
                *callee,
                ctx,
                classes,
                enums,
                interfaces,
                functions,
                errors,
                false,
                allow_result,
                in_result_fn,
            );
            if !type_args.is_empty() {
                errors.push(TypeError::TypeApplyWithoutCall {
                    span: span_from_range(body.expr_span(expr_id)),
                });
            }
            Type::Unknown
        }
        Expr::Crash { expr } => {
            let _ = infer_expr(
                body,
                *expr,
                ctx,
                classes,
                enums,
                interfaces,
                functions,
                errors,
                false,
                allow_result,
                in_result_fn,
            );
            Type::Never
        }
        Expr::Call { .. } => {
            let (callee, args, type_args) = match &body.exprs[expr_id] {
                Expr::Call {
                    callee,
                    args,
                    type_args,
                } => (callee, args, type_args),
                _ => unreachable!(),
            };
            for arg in args {
                match arg {
                    crate::hir::Arg::Positional { value, .. } => {
                        infer_expr(
                            body,
                            *value,
                            ctx,
                            classes,
                            enums,
                            interfaces,
                            functions,
                            errors,
                            false,
                            allow_result,
                            in_result_fn,
                        );
                    }
                    crate::hir::Arg::Named { value, .. } => {
                        infer_expr(
                            body,
                            *value,
                            ctx,
                            classes,
                            enums,
                            interfaces,
                            functions,
                            errors,
                            false,
                            allow_result,
                            in_result_fn,
                        );
                    }
                }
            }
            let mut ret_ty = None;
            let mut valid_callee = false;
            if is_pool_of_call(body, *callee) {
                ret_ty = Some(Type::Unknown);
                valid_callee = true;
            }
            if let Expr::Variable(name) = &body.exprs[*callee] {
                if classes.is_class(name) {
                    if let Some(class) = classes.get(name) {
                        let class_args = resolve_type_args(
                            name,
                            &class.type_params,
                            type_args,
                            ctx,
                            errors,
                            span_from_range(body.expr_span(expr_id)),
                        );
                        check_class_init_args(
                            body,
                            expr_id,
                            args,
                            class,
                            &class_args,
                            ctx,
                            classes,
                            enums,
                            interfaces,
                            functions,
                            errors,
                            allow_result,
                            in_result_fn,
                        );
                        ret_ty = Some(Type::Named(name.clone(), class_args));
                        valid_callee = true;
                    } else {
                        ret_ty = Some(Type::Named(name.clone(), Vec::new()));
                        valid_callee = true;
                    }
                }
                if let Some(function) = functions.get(name) {
                    if !type_args.is_empty() {
                        if function.type_params.is_empty() {
                            errors.push(TypeError::UnexpectedTypeArgs {
                                span: span_from_range(body.expr_span(expr_id)),
                            });
                        } else {
                            // Generic function call with explicit type args — check bounds
                            let resolved_type_args: Vec<Type> = type_args.iter().map(type_from_ref).collect();
                            check_type_param_bounds(
                                &function.type_params,
                                &function.type_param_bounds,
                                &resolved_type_args,
                                classes,
                                span_from_range(body.expr_span(expr_id)),
                                errors,
                            );
                        }
                    }
                    if name.as_str() == "assert" && args.len() == 1 && function.params.len() == 2 {
                        let mut params = Vec::new();
                        params.push(function.params[0].clone());
                        check_call_args(
                            body,
                            expr_id,
                            args,
                            &params,
                            ctx,
                            classes,
                            enums,
                            interfaces,
                            functions,
                            errors,
                            !name.as_str().starts_with("__wr_"),
                            allow_result,
                            in_result_fn,
                        );
                    } else {
                        check_call_args(
                            body,
                            expr_id,
                            args,
                            &function.params,
                            ctx,
                            classes,
                            enums,
                            interfaces,
                            functions,
                            errors,
                            !name.as_str().starts_with("__wr_"),
                            allow_result,
                            in_result_fn,
                        );
                    }
                    ret_ty = Some(function.ret.clone());
                    valid_callee = true;
                }
                if !valid_callee && ctx.resolve(name).is_some() {
                    errors.push(TypeError::InvalidCallee {
                        span: callee_error_span(body, *callee),
                    });
                }
            }
            let mut handled_member = false;
            if ret_ty.is_none()
                && let Expr::Member {
                    object,
                    member,
                    member_span,
                } = &body.exprs[*callee]
            {
                handled_member = true;
                if !type_args.is_empty() {
                    errors.push(TypeError::UnexpectedTypeArgs {
                        span: span_from_range(body.expr_span(expr_id)),
                    });
                }
                let mut enum_ctor_handled = false;
                let mut enum_name_opt: Option<SmolStr> = None;
                let mut enum_type_args: Vec<TypeRef> = Vec::new();
                if let Expr::Variable(enum_name) = &body.exprs[*object] {
                    enum_name_opt = Some(enum_name.clone());
                } else if let Expr::TypeApply { callee, type_args } = &body.exprs[*object]
                    && let Expr::Variable(enum_name) = &body.exprs[*callee]
                {
                    enum_name_opt = Some(enum_name.clone());
                    enum_type_args = type_args.clone();
                }
                if let Some(enum_name) = enum_name_opt
                    && let Some(en) = enums.get(&enum_name)
                    && let Some(params) = en.variants.get(member)
                {
                    let resolved_args = resolve_type_args(
                        &enum_name,
                        &en.type_params,
                        &enum_type_args,
                        ctx,
                        errors,
                        span_from_range(body.expr_span(*object)),
                    );
                    let subst = build_type_subst(&en.type_params, &resolved_args);
                    let params: Vec<(SmolStr, Type)> = params
                        .iter()
                        .map(|(name, ty)| (name.clone(), substitute_type(ty, &subst)))
                        .collect();
                    check_call_args(
                        body,
                        expr_id,
                        args,
                        &params,
                        ctx,
                        classes,
                        enums,
                        interfaces,
                        functions,
                        errors,
                        true,
                        allow_result,
                        in_result_fn,
                    );
                    ret_ty = Some(Type::Named(enum_name.clone(), resolved_args));
                    valid_callee = true;
                    enum_ctor_handled = true;
                }
                if is_pool_of_member(body, *object, member) {
                    ret_ty = Some(Type::Unknown);
                    valid_callee = true;
                }
                if !enum_ctor_handled {
                    let object_ty = infer_expr(
                        body,
                        *object,
                        ctx,
                        classes,
                        enums,
                        interfaces,
                        functions,
                        errors,
                        false,
                        allow_result,
                        in_result_fn,
                    );
                    if let Some((params, ret)) = collection_method_sig(&object_ty, member) {
                        check_call_args(
                            body,
                            expr_id,
                            args,
                            &params,
                            ctx,
                            classes,
                            enums,
                            interfaces,
                            functions,
                            errors,
                            true,
                            allow_result,
                            in_result_fn,
                        );
                        ret_ty = Some(ret);
                        valid_callee = true;
                    } else {
                        match object_ty {
                            Type::Actor(inner) => {
                                if let Type::Named(class_name, class_args) = *inner
                                    && let Some(class) = classes.get(&class_name)
                                {
                                    let method_params =
                                        instantiate_method_params(class, &class_args, member);
                                    let method_ret =
                                        instantiate_method_ret(class, &class_args, member);
                                    if let Some(method) = class.methods.get(member) {
                                        let params = method_params.unwrap_or(method.params.clone());
                                        check_call_args(
                                            body,
                                            expr_id,
                                            args,
                                            &params,
                                            ctx,
                                            classes,
                                            enums,
                                            interfaces,
                                            functions,
                                            errors,
                                            true,
                                            allow_result,
                                            in_result_fn,
                                        );
                                        let ret = method_ret.unwrap_or(method.ret.clone());
                                        ret_ty = Some(Type::Pending(Box::new(Type::Result(
                                            Box::new(ret),
                                            Box::new(error_type()),
                                        ))));
                                        valid_callee = true;
                                    } else {
                                        errors.push(TypeError::UnknownMember {
                                            object: class_name.to_string(),
                                            member: member.clone(),
                                            span: span_from_range(*member_span),
                                        });
                                        ret_ty = Some(Type::Unknown);
                                    }
                                }
                            }
                            Type::Named(class_name, class_args)
                                if interfaces.is_interface(&class_name) =>
                            {
                                if let Some(interface) = interfaces.get(&class_name) {
                                    if let Some(method) = interface.methods.get(member) {
                                        let params = method.params.clone();
                                        check_call_args(
                                            body,
                                            expr_id,
                                            args,
                                            &params,
                                            ctx,
                                            classes,
                                            enums,
                                            interfaces,
                                            functions,
                                            errors,
                                            true,
                                            allow_result,
                                            in_result_fn,
                                        );
                                        ret_ty = Some(method.ret.clone());
                                        valid_callee = true;
                                    } else {
                                        errors.push(TypeError::UnknownMember {
                                            object: class_name.to_string(),
                                            member: member.clone(),
                                            span: span_from_range(*member_span),
                                        });
                                        ret_ty = Some(Type::Unknown);
                                    }
                                }
                            }
                            Type::Named(class_name, class_args) => {
                                if let Some(class) = classes.get(&class_name) {
                                    let method_params =
                                        instantiate_method_params(class, &class_args, member);
                                    let method_ret =
                                        instantiate_method_ret(class, &class_args, member);
                                    if let Some(method) = class.methods.get(member) {
                                        let params = method_params.unwrap_or(method.params.clone());
                                        check_call_args(
                                            body,
                                            expr_id,
                                            args,
                                            &params,
                                            ctx,
                                            classes,
                                            enums,
                                            interfaces,
                                            functions,
                                            errors,
                                            true,
                                            allow_result,
                                            in_result_fn,
                                        );
                                        ret_ty = Some(method_ret.unwrap_or(method.ret.clone()));
                                        valid_callee = true;
                                    } else if class.fields.contains_key(member) {
                                        errors.push(TypeError::CallField {
                                            member: member.clone(),
                                            span: span_from_range(*member_span),
                                        });
                                        ret_ty = Some(Type::Unknown);
                                    } else {
                                        errors.push(TypeError::UnknownMember {
                                            object: class_name.to_string(),
                                            member: member.clone(),
                                            span: span_from_range(*member_span),
                                        });
                                        ret_ty = Some(Type::Unknown);
                                    }
                                }
                            }
                            Type::Unknown => {}
                            _ => {
                                errors.push(TypeError::InvalidCallee {
                                    span: span_from_range(*member_span),
                                });
                            }
                        }
                    }
                }
            }
            if ret_ty.is_none() && !handled_member {
                if !matches!(&body.exprs[*callee], Expr::Variable(_)) {
                    errors.push(TypeError::InvalidCallee {
                        span: callee_error_span(body, *callee),
                    });
                }
                let _ = infer_expr(
                    body,
                    *callee,
                    ctx,
                    classes,
                    enums,
                    interfaces,
                    functions,
                    errors,
                    false,
                    allow_result,
                    in_result_fn,
                );
            }
            ret_ty.unwrap_or(Type::Unknown)
        }
        Expr::Member {
            object,
            member,
            member_span,
        } => {
            let mut enum_name_opt: Option<SmolStr> = None;
            let mut enum_type_args: Vec<TypeRef> = Vec::new();
            if let Expr::Variable(enum_name) = &body.exprs[*object] {
                enum_name_opt = Some(enum_name.clone());
            } else if let Expr::TypeApply { callee, type_args } = &body.exprs[*object]
                && let Expr::Variable(enum_name) = &body.exprs[*callee]
            {
                enum_name_opt = Some(enum_name.clone());
                enum_type_args = type_args.clone();
            }
            if let Some(enum_name) = enum_name_opt
                && let Some(en) = enums.get(&enum_name)
                && let Some(params) = en.variants.get(member)
                && params.is_empty()
            {
                let resolved_args = resolve_type_args(
                    &enum_name,
                    &en.type_params,
                    &enum_type_args,
                    ctx,
                    errors,
                    span_from_range(body.expr_span(*object)),
                );
                return Type::Named(enum_name.clone(), resolved_args);
            }
            let object_ty = infer_expr(
                body,
                *object,
                ctx,
                classes,
                enums,
                interfaces,
                functions,
                errors,
                false,
                allow_result,
                in_result_fn,
            );
            let mut result = Type::Unknown;
            if let Type::Actor(_) = object_ty {
                errors.push(TypeError::ActorMemberAccess {
                    member: member.clone(),
                    span: span_from_range(*member_span),
                });
            } else if let Type::Named(class_name, class_args) = object_ty {
                if interfaces.is_interface(&class_name) {
                    errors.push(TypeError::UnknownMember {
                        object: class_name.to_string(),
                        member: member.clone(),
                        span: span_from_range(*member_span),
                    });
                } else if let Some(class) = classes.get(&class_name) {
                    let subst = class_subst(class, &class_args);
                    if let Some(field_ty) = class.fields.get(member) {
                        result = substitute_type(field_ty, &subst);
                    } else if let Some(method) = class.methods.get(member) {
                        let _ = method;
                        result = Type::Unknown;
                    } else {
                        errors.push(TypeError::UnknownMember {
                            object: class_name.to_string(),
                            member: member.clone(),
                            span: span_from_range(*member_span),
                        });
                    }
                }
            }
            result
        }
        Expr::Index {
            object,
            index,
            index_span,
        } => {
            let object_ty = infer_expr(
                body,
                *object,
                ctx,
                classes,
                enums,
                interfaces,
                functions,
                errors,
                false,
                allow_result,
                in_result_fn,
            );
            let index_ty = infer_expr(
                body,
                *index,
                ctx,
                classes,
                enums,
                interfaces,
                functions,
                errors,
                false,
                allow_result,
                in_result_fn,
            );
            match object_ty {
                Type::List(inner_ty) => {
                    if types_known(&Type::Integer, &index_ty) && index_ty != Type::Integer {
                        errors.push(TypeError::InvalidIndexType {
                            expected: "Integer".to_string(),
                            found: type_label(&index_ty),
                            span: span_from_range(*index_span),
                        });
                    }
                    (*inner_ty).clone()
                }
                Type::Map(key_ty, value_ty) => {
                    if types_known(&key_ty, &index_ty)
                        && !is_assignable(&key_ty, &index_ty, classes, interfaces)
                    {
                        errors.push(TypeError::InvalidIndexType {
                            expected: type_label(&key_ty),
                            found: type_label(&index_ty),
                            span: span_from_range(*index_span),
                        });
                    }
                    (*value_ty).clone()
                }
                Type::Unknown => Type::Unknown,
                _ => {
                    errors.push(TypeError::InvalidIndexTarget {
                        span: span_from_range(*index_span),
                    });
                    Type::Unknown
                }
            }
        }
        Expr::List(items) => infer_list(
            body,
            items,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
        ),
        Expr::Map(items) => infer_map(
            body,
            items,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
        ),
        Expr::StringInterp(parts) => {
            for part in parts {
                if let crate::hir::StringPart::Expr(expr) = part {
                    infer_expr(
                        body,
                        *expr,
                        ctx,
                        classes,
                        enums,
                        interfaces,
                        functions,
                        errors,
                        false,
                        allow_result,
                        in_result_fn,
                    );
                }
            }
            Type::String
        }
        Expr::Closure { params, body: closure_body } => {
            ctx.enter_scope();
            for param in params {
                let ty = param
                    .ty
                    .as_ref()
                    .map(|t| type_from_ref_in_ctx(t, ctx))
                    .unwrap_or(Type::Unknown);
                ctx.declare(param.name.clone(), ty);
            }
            let _ = infer_expr(
                body,
                *closure_body,
                ctx,
                classes,
                enums,
                interfaces,
                functions,
                errors,
                false,
                allow_result,
                in_result_fn,
            );
            ctx.exit_scope();
            Type::Unknown
        }
    };
    ctx.record_expr(expr_id, ty.clone());
    if matches!(ty, Type::Pending(_)) && !allow_pending {
        errors.push(TypeError::PendingNotAwaited {
            span: span_from_range(body.expr_span(expr_id)),
            help: "`await` yields Result, and `fire` is for fire-and-forget. Use `await` or \
`fire` here."
                .to_string(),
        });
    }
    if matches!(ty, Type::Result(_, _)) && !allow_result {
        errors.push(TypeError::UnhandledResult {
            span: span_from_range(body.expr_span(expr_id)),
            help: "Handle with `??`, `match`, `ignore result`, `capture`, or return the \
Result from a Result-returning function."
                .to_string(),
        });
    }
    ty
}

fn check_call_args(
    body: &Body,
    expr_id: Idx<Expr>,
    args: &Vec<crate::hir::Arg>,
    params: &Vec<(SmolStr, Type)>,
    ctx: &mut TypeContext,
    classes: &ClassIndex,
    enums: &EnumIndex,
    interfaces: &InterfaceIndex,
    functions: &FunctionIndex,
    errors: &mut Vec<TypeError>,
    enforce_named_args: bool,
    allow_result: bool,
    in_result_fn: bool,
) {
    if enforce_named_args && requires_named_args(params.len(), args) {
        let arg_spans =
            args.iter()
                .map(|arg| match arg {
                    crate::hir::Arg::Positional { span, .. }
                    | crate::hir::Arg::Named { span, .. } => span_from_range(*span),
                })
                .collect::<Vec<_>>();
        errors.push(TypeError::NamedArgsRequired {
            span: span_from_range(body.expr_span(expr_id)),
            param_names: params.iter().map(|(name, _)| name.clone()).collect(),
            arg_spans,
        });
    }

    if args
        .iter()
        .any(|arg| matches!(arg, crate::hir::Arg::Named { .. }))
    {
        let mut param_map = HashMap::new();
        for (name, ty) in params {
            param_map.insert(name.clone(), ty.clone());
        }
        for arg in args {
            if let crate::hir::Arg::Named {
                name,
                value,
                name_span,
                ..
            } = arg
            {
                let Some(expected) = param_map.get(name) else {
                    errors.push(TypeError::UnknownArgument {
                        name: name.clone(),
                        span: span_from_range(*name_span),
                    });
                    continue;
                };
                let found = infer_expr(
                    body,
                    *value,
                    ctx,
                    classes,
                    enums,
                    interfaces,
                    functions,
                    errors,
                    false,
                    allow_result,
                    in_result_fn,
                );
                if types_known(expected, &found)
                    && !is_assignable(expected, &found, classes, interfaces)
                {
                    errors.push(TypeError::ArgumentTypeMismatch {
                        name: name.clone(),
                        expected: type_label(expected),
                        found: type_label(&found),
                        span: span_from_range(*name_span),
                    });
                }
            }
        }
        if args.len() != params.len() {
            errors.push(TypeError::ArgumentCountMismatch {
                expected: params.len(),
                found: args.len(),
                span: span_from_range(body.expr_span(expr_id)),
            });
        }
        return;
    }

    if args.len() != params.len() {
        errors.push(TypeError::ArgumentCountMismatch {
            expected: params.len(),
            found: args.len(),
            span: span_from_range(body.expr_span(expr_id)),
        });
        return;
    }

    for (index, arg) in args.iter().enumerate() {
        let expected = &params[index].1;
        let (found, span) = match arg {
            crate::hir::Arg::Positional { value, span } => (
                infer_expr(
                    body,
                    *value,
                    ctx,
                    classes,
                    enums,
                    interfaces,
                    functions,
                    errors,
                    false,
                    allow_result,
                    in_result_fn,
                ),
                *span,
            ),
            crate::hir::Arg::Named { value, span, .. } => (
                infer_expr(
                    body,
                    *value,
                    ctx,
                    classes,
                    enums,
                    interfaces,
                    functions,
                    errors,
                    false,
                    allow_result,
                    in_result_fn,
                ),
                *span,
            ),
        };
        if types_known(expected, &found) && !is_assignable(expected, &found, classes, interfaces) {
            errors.push(TypeError::ArgumentTypeMismatch {
                name: params[index].0.clone(),
                expected: type_label(expected),
                found: type_label(&found),
                span: span_from_range(span),
            });
        }
    }
}

