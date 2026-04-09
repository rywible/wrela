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
        Expr::Variable(name) => match name.as_str() {
            "coarse" | "fine" => detail_tier_type(),
            _ => ctx.resolve(name).unwrap_or(Type::Unknown),
        },
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
                    Type::List(inner_ty) | Type::Array(inner_ty, _) => {
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
                if let Some(_) = vector_component_type(&object_ty, member) {
                    errors.push(TypeError::ImmutableFieldAssign {
                        member: member.clone(),
                        span: span_from_range(*op_span),
                        help: "Vector components are read-only projections; construct a new vector instead.".to_string(),
                    });
                    return Type::Unknown;
                }
                if matches!(
                    &object_ty,
                    Type::Vec2 | Type::Vec3 | Type::Vec4 | Type::Quat | Type::Mat3 | Type::Mat4
                ) {
                    errors.push(TypeError::UnknownMember {
                        object: type_label(&object_ty),
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
                if let Some(ret) = infer_math_builtin_call(
                    body,
                    expr_id,
                    name,
                    args,
                    ctx,
                    classes,
                    enums,
                    interfaces,
                    functions,
                    errors,
                    allow_result,
                    in_result_fn,
                ) {
                    ret_ty = Some(ret);
                    valid_callee = true;
                }
                if !valid_callee
                    && let Some(ret) = infer_compute_builtin_call(
                        body,
                        expr_id,
                        name,
                        args,
                        ctx,
                        classes,
                        enums,
                        interfaces,
                        functions,
                        errors,
                        allow_result,
                        in_result_fn,
                    )
                {
                    ret_ty = Some(ret);
                    valid_callee = true;
                }
                if classes.is_class(name) {
                    if let Some(record) = crate::portable::builtin_record(name.as_str())
                        && !record.constructible
                    {
                        errors.push(TypeError::OpaqueBuiltinConstructionForbidden {
                            name: name.clone(),
                            span: span_from_range(body.expr_span(expr_id)),
                            help: format!(
                                "Use the builtin entrypoint that produces `{}` instead of constructing it directly.",
                                name
                            ),
                        });
                        ret_ty = Some(Type::Named(name.clone(), Vec::new()));
                        valid_callee = true;
                    } else if let Some(class) = classes.get(name) {
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
                if !valid_callee && let Some(function) = functions.get(name) {
                    if ctx.in_portable_lane()
                        && !functions.is_portable(name)
                        && !(ctx.in_portable_query_kernel_lane() && functions.is_domain(name))
                    {
                        errors.push(TypeError::PortableHostCallForbidden {
                            function: ctx.current_function_name(),
                            callee: name.clone(),
                            span: span_from_range(body.expr_span(expr_id)),
                            help: "Portable declarations may only call other portable declarations or portable-safe intrinsics.".to_string(),
                        });
                    }
                    if ctx.in_portable_lane()
                        && matches!(ctx.current_function_role(), FunctionRole::Pure)
                        && functions.is_kernel(name)
                    {
                        errors.push(TypeError::PortableConstructForbidden {
                            function: ctx.current_function_name(),
                            construct: format!("calling kernel declaration '{}'", name),
                            span: span_from_range(body.expr_span(expr_id)),
                            help: "Pure helpers stay reusable across host code, semantic portable code, and kernels. Call other `pure fn` helpers instead of jumping into low-level `kernel fn` entry points.".to_string(),
                        });
                    }
                    if !type_args.is_empty() {
                        if function.type_params.is_empty() {
                            errors.push(TypeError::UnexpectedTypeArgs {
                                span: span_from_range(body.expr_span(expr_id)),
                            });
                        } else {
                            // Generic function call with explicit type args — check bounds
                            let resolved_type_args: Vec<Type> =
                                type_args.iter().map(type_from_ref).collect();
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
                            !builtin_allows_positional_args(name),
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
                            Type::Vec2 => {
                                if matches!(member.as_str(), "x" | "y") {
                                    errors.push(TypeError::CallField {
                                        member: member.clone(),
                                        span: span_from_range(*member_span),
                                    });
                                } else {
                                    errors.push(TypeError::UnknownMember {
                                        object: "Vec2".to_string(),
                                        member: member.clone(),
                                        span: span_from_range(*member_span),
                                    });
                                }
                                ret_ty = Some(Type::Unknown);
                                valid_callee = true;
                            }
                            Type::Vec3 => {
                                if matches!(member.as_str(), "x" | "y" | "z") {
                                    errors.push(TypeError::CallField {
                                        member: member.clone(),
                                        span: span_from_range(*member_span),
                                    });
                                } else {
                                    errors.push(TypeError::UnknownMember {
                                        object: "Vec3".to_string(),
                                        member: member.clone(),
                                        span: span_from_range(*member_span),
                                    });
                                }
                                ret_ty = Some(Type::Unknown);
                                valid_callee = true;
                            }
                            Type::Vec4 => {
                                if matches!(member.as_str(), "x" | "y" | "z" | "w") {
                                    errors.push(TypeError::CallField {
                                        member: member.clone(),
                                        span: span_from_range(*member_span),
                                    });
                                } else {
                                    errors.push(TypeError::UnknownMember {
                                        object: "Vec4".to_string(),
                                        member: member.clone(),
                                        span: span_from_range(*member_span),
                                    });
                                }
                                ret_ty = Some(Type::Unknown);
                                valid_callee = true;
                            }
                            Type::Quat => {
                                if matches!(member.as_str(), "x" | "y" | "z" | "w") {
                                    errors.push(TypeError::CallField {
                                        member: member.clone(),
                                        span: span_from_range(*member_span),
                                    });
                                } else {
                                    errors.push(TypeError::UnknownMember {
                                        object: "Quat".to_string(),
                                        member: member.clone(),
                                        span: span_from_range(*member_span),
                                    });
                                }
                                ret_ty = Some(Type::Unknown);
                                valid_callee = true;
                            }
                            Type::Mat3 => {
                                errors.push(TypeError::UnknownMember {
                                    object: "Mat3".to_string(),
                                    member: member.clone(),
                                    span: span_from_range(*member_span),
                                });
                                ret_ty = Some(Type::Unknown);
                                valid_callee = true;
                            }
                            Type::Mat4 => {
                                errors.push(TypeError::UnknownMember {
                                    object: "Mat4".to_string(),
                                    member: member.clone(),
                                    span: span_from_range(*member_span),
                                });
                                ret_ty = Some(Type::Unknown);
                                valid_callee = true;
                            }
                            Type::Named(class_name, class_args)
                                if class_args.is_empty()
                                    && is_portable_named_data_type_name(class_name.as_str()) =>
                            {
                                if portable_named_field_type(class_name.as_str(), member.as_str())
                                    .is_some()
                                {
                                    errors.push(TypeError::CallField {
                                        member: member.clone(),
                                        span: span_from_range(*member_span),
                                    });
                                } else {
                                    errors.push(TypeError::UnknownMember {
                                        object: class_name.to_string(),
                                        member: member.clone(),
                                        span: span_from_range(*member_span),
                                    });
                                }
                                ret_ty = Some(Type::Unknown);
                                valid_callee = true;
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
            } else if let Some(component_ty) = vector_component_type(&object_ty, member) {
                result = component_ty;
            } else if matches!(
                &object_ty,
                Type::Vec2 | Type::Vec3 | Type::Vec4 | Type::Quat | Type::Mat3 | Type::Mat4
            ) {
                errors.push(TypeError::UnknownMember {
                    object: type_label(&object_ty),
                    member: member.clone(),
                    span: span_from_range(*member_span),
                });
            } else if let Type::Named(class_name, class_args) = object_ty {
                if class_args.is_empty()
                    && let Some(field_ty) =
                        portable_named_field_type(class_name.as_str(), member.as_str())
                {
                    result = field_ty;
                } else if class_args.is_empty()
                    && is_portable_named_data_type_name(class_name.as_str())
                {
                    errors.push(TypeError::UnknownMember {
                        object: class_name.to_string(),
                        member: member.clone(),
                        span: span_from_range(*member_span),
                    });
                } else if interfaces.is_interface(&class_name) {
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
                Type::List(inner_ty) | Type::Array(inner_ty, _) => {
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
        Expr::Closure {
            params,
            body: closure_body,
        } => {
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
    ctx.record_expr(body, expr_id, ty.clone());
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

fn builtin_allows_positional_args(name: &SmolStr) -> bool {
    name.as_str().starts_with("__wr_")
        || matches!(
            name.as_str(),
            "assert"
                | "vec2"
                | "vec3"
                | "vec4"
                | "quat"
                | "mat3_identity"
                | "mat3_cols"
                | "mat4_identity"
                | "mat4_cols"
                | "dot"
                | "length"
                | "normalize"
                | "cross"
                | "min"
                | "max"
                | "clamp"
                | "mix"
                | "abs"
                | "sign"
                | "floor"
                | "ceil"
                | "fract"
                | "sin"
                | "cos"
                | "sqrt"
                | "pow"
                | "distance"
                | "reflect"
                | "f32"
                | "i32"
                | "i64"
                | "u32"
                | "u64"
                | "capture"
        )
}

fn call_named_arg_value(args: &[crate::hir::Arg], name: &str) -> Option<Idx<Expr>> {
    for arg in args {
        if let crate::hir::Arg::Named {
            name: arg_name,
            value,
            ..
        } = arg
            && arg_name.as_str() == name
        {
            return Some(*value);
        }
    }
    None
}

fn infer_compute_builtin_call(
    body: &Body,
    expr_id: Idx<Expr>,
    name: &SmolStr,
    args: &Vec<crate::hir::Arg>,
    ctx: &mut TypeContext,
    classes: &ClassIndex,
    enums: &EnumIndex,
    interfaces: &InterfaceIndex,
    functions: &FunctionIndex,
    errors: &mut Vec<TypeError>,
    allow_result: bool,
    in_result_fn: bool,
) -> Option<Type> {
    let span = span_from_range(body.expr_span(expr_id));
    match name.as_str() {
        "gpu_buffer_new" => {
            let params = vec![
                (SmolStr::new("length"), Type::Integer),
                (SmolStr::new("default_value"), Type::Unknown),
            ];
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
            let value_ty = call_named_arg_value(args, "default_value")
                .map(|value| {
                    infer_expr(
                        body,
                        value,
                        ctx,
                        classes,
                        enums,
                        interfaces,
                        functions,
                        errors,
                        false,
                        allow_result,
                        in_result_fn,
                    )
                })
                .unwrap_or(Type::Unknown);
            Some(Type::GpuBuffer(Box::new(value_ty)))
        }
        "gpu_buffer_len" => {
            let Some(buffer_value) = call_named_arg_value(args, "buffer") else {
                errors.push(TypeError::ArgumentCountMismatch {
                    expected: 1,
                    found: args.len(),
                    span,
                });
                return Some(Type::Unknown);
            };
            let buffer_ty = infer_expr(
                body,
                buffer_value,
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
            let params = vec![(SmolStr::new("buffer"), Type::Unknown)];
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
            if !matches!(buffer_ty, Type::GpuBuffer(_)) {
                errors.push(TypeError::ArgumentTypeMismatch {
                    name: SmolStr::new("buffer"),
                    expected: "GpuBuffer[unknown]".to_string(),
                    found: type_label(&buffer_ty),
                    span,
                });
                return Some(Type::Unknown);
            }
            Some(Type::Integer)
        }
        "gpu_buffer_get" => {
            let Some(buffer_value) = call_named_arg_value(args, "buffer") else {
                errors.push(TypeError::ArgumentCountMismatch {
                    expected: 2,
                    found: args.len(),
                    span,
                });
                return Some(Type::Unknown);
            };
            let buffer_ty = infer_expr(
                body,
                buffer_value,
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
            let Some(inner_ty) = (match buffer_ty {
                Type::GpuBuffer(inner) => Some(*inner),
                other => {
                    errors.push(TypeError::ArgumentTypeMismatch {
                        name: SmolStr::new("buffer"),
                        expected: "GpuBuffer[unknown]".to_string(),
                        found: type_label(&other),
                        span,
                    });
                    None
                }
            }) else {
                return Some(Type::Unknown);
            };
            let Some(index_expr) = call_named_arg_value(args, "index") else {
                errors.push(TypeError::ArgumentCountMismatch {
                    expected: 2,
                    found: args.len(),
                    span,
                });
                return Some(Type::Unknown);
            };
            let index_ty = infer_expr(
                body,
                index_expr,
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
            if !matches!(index_ty, Type::Unknown)
                && !matches!(
                    index_ty,
                    Type::Integer | Type::I32 | Type::U32 | Type::I64 | Type::U64
                )
            {
                errors.push(TypeError::ArgumentTypeMismatch {
                    name: SmolStr::new("index"),
                    expected: "Integer-like scalar".to_string(),
                    found: type_label(&index_ty),
                    span,
                });
            }
            let params = vec![
                (SmolStr::new("buffer"), Type::Unknown),
                (SmolStr::new("index"), Type::Unknown),
            ];
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
            Some(inner_ty)
        }
        "gpu_buffer_set" => {
            let Some(buffer_value) = call_named_arg_value(args, "buffer") else {
                errors.push(TypeError::ArgumentCountMismatch {
                    expected: 3,
                    found: args.len(),
                    span,
                });
                return Some(Type::Nil);
            };
            let buffer_ty = infer_expr(
                body,
                buffer_value,
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
            let Some(inner_ty) = (match buffer_ty {
                Type::GpuBuffer(inner) => Some(*inner),
                other => {
                    errors.push(TypeError::ArgumentTypeMismatch {
                        name: SmolStr::new("buffer"),
                        expected: "GpuBuffer[unknown]".to_string(),
                        found: type_label(&other),
                        span,
                    });
                    None
                }
            }) else {
                return Some(Type::Nil);
            };
            let Some(index_expr) = call_named_arg_value(args, "index") else {
                errors.push(TypeError::ArgumentCountMismatch {
                    expected: 3,
                    found: args.len(),
                    span,
                });
                return Some(Type::Nil);
            };
            let index_ty = infer_expr(
                body,
                index_expr,
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
            if !matches!(index_ty, Type::Unknown)
                && !matches!(
                    index_ty,
                    Type::Integer | Type::I32 | Type::U32 | Type::I64 | Type::U64
                )
            {
                errors.push(TypeError::ArgumentTypeMismatch {
                    name: SmolStr::new("index"),
                    expected: "Integer-like scalar".to_string(),
                    found: type_label(&index_ty),
                    span,
                });
            }
            let Some(value_expr) = call_named_arg_value(args, "value") else {
                errors.push(TypeError::ArgumentCountMismatch {
                    expected: 3,
                    found: args.len(),
                    span,
                });
                return Some(Type::Nil);
            };
            let value_ty = infer_expr(
                body,
                value_expr,
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
            if !matches!(inner_ty, Type::Unknown)
                && types_known(&inner_ty, &value_ty)
                && !is_assignable(&inner_ty, &value_ty, classes, interfaces)
            {
                errors.push(TypeError::ArgumentTypeMismatch {
                    name: SmolStr::new("value"),
                    expected: type_label(&inner_ty),
                    found: type_label(&value_ty),
                    span,
                });
            }
            let params = vec![
                (SmolStr::new("buffer"), Type::Unknown),
                (SmolStr::new("index"), Type::Unknown),
                (SmolStr::new("value"), inner_ty),
            ];
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
            Some(Type::Nil)
        }
        "gpu_atomic_i32_new" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[(SmolStr::new("initial"), Type::I32)],
            Type::GpuAtomicI32,
        ),
        "gpu_atomic_i32_drop" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[(SmolStr::new("atomic"), Type::GpuAtomicI32)],
            Type::Boolean,
        ),
        "gpu_atomic_i32_load" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[(SmolStr::new("atomic"), Type::GpuAtomicI32)],
            Type::I32,
        ),
        "gpu_atomic_i32_store" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[
                (SmolStr::new("atomic"), Type::GpuAtomicI32),
                (SmolStr::new("value"), Type::I32),
            ],
            Type::Nil,
        ),
        "gpu_atomic_i32_fetch_add" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[
                (SmolStr::new("atomic"), Type::GpuAtomicI32),
                (SmolStr::new("delta"), Type::I32),
            ],
            Type::I32,
        ),
        "gpu_atomic_u32_new" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[(SmolStr::new("initial"), Type::U32)],
            Type::GpuAtomicU32,
        ),
        "gpu_atomic_u32_drop" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[(SmolStr::new("atomic"), Type::GpuAtomicU32)],
            Type::Boolean,
        ),
        "gpu_atomic_u32_load" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[(SmolStr::new("atomic"), Type::GpuAtomicU32)],
            Type::U32,
        ),
        "gpu_atomic_u32_store" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[
                (SmolStr::new("atomic"), Type::GpuAtomicU32),
                (SmolStr::new("value"), Type::U32),
            ],
            Type::Nil,
        ),
        "gpu_atomic_u32_fetch_add" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[
                (SmolStr::new("atomic"), Type::GpuAtomicU32),
                (SmolStr::new("delta"), Type::U32),
            ],
            Type::U32,
        ),
        "gpu_schedule_deterministic" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[],
            Type::GpuDispatchSchedule,
        ),
        "gpu_schedule_reverse" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[],
            Type::GpuDispatchSchedule,
        ),
        "gpu_schedule_shuffle" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[(SmolStr::new("seed"), Type::U32)],
            Type::GpuDispatchSchedule,
        ),
        "gpu_schedule_workgroup_reverse" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[],
            Type::GpuDispatchSchedule,
        ),
        "gpu_schedule_workgroup_shuffle" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[(SmolStr::new("seed"), Type::U32)],
            Type::GpuDispatchSchedule,
        ),
        "gpu_schedule_round_robin_workgroups" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[],
            Type::GpuDispatchSchedule,
        ),
        "workgroup_barrier" => {
            errors.push(TypeError::UnsupportedComputeFeature {
                feature: "workgroup_barrier",
                span,
                help: "Barriered workgroup execution is the next virtual GPU cut; use data-parallel kernels plus atomics for now.".to_string(),
            });
            Some(Type::Nil)
        }
        "storage_barrier" => {
            errors.push(TypeError::UnsupportedComputeFeature {
                feature: "storage_barrier",
                span,
                help: "Storage barriers are not modeled in the CPU reference GPU yet; keep kernels order-independent or use explicit atomic coordination.".to_string(),
            });
            Some(Type::Nil)
        }
        "dispatch_compute" => {
            let Some(kernel_expr) = call_named_arg_value(args, "kernel") else {
                errors.push(TypeError::UnknownArgument {
                    name: SmolStr::new("kernel"),
                    span,
                });
                return Some(Type::Nil);
            };
            let Expr::Variable(kernel_name) = &body.exprs[kernel_expr] else {
                errors.push(TypeError::InvalidCallee {
                    span: span_from_range(body.expr_span(kernel_expr)),
                });
                return Some(Type::Nil);
            };
            let Some(kernel) = functions.get(kernel_name) else {
                errors.push(TypeError::InvalidCallee {
                    span: span_from_range(body.expr_span(kernel_expr)),
                });
                return Some(Type::Nil);
            };
            if !functions.is_kernel(kernel_name) {
                errors.push(TypeError::DispatchKernelMustBePortable {
                    callee: kernel_name.clone(),
                    span: span_from_range(body.expr_span(kernel_expr)),
                });
            }
            let mut params = vec![(SmolStr::new("kernel"), Type::Unknown)];
            params.extend([
                (SmolStr::new("workgroups_x"), Type::U32),
                (SmolStr::new("workgroups_y"), Type::U32),
                (SmolStr::new("workgroups_z"), Type::U32),
                (SmolStr::new("workgroup_size_x"), Type::U32),
                (SmolStr::new("workgroup_size_y"), Type::U32),
                (SmolStr::new("workgroup_size_z"), Type::U32),
            ]);
            if call_named_arg_value(args, "schedule").is_some() {
                params.push((SmolStr::new("schedule"), Type::GpuDispatchSchedule));
            }
            params.extend(kernel.params.clone());
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
            if kernel.ret != Type::Nil {
                errors.push(TypeError::ReturnTypeMismatch {
                    expected: "Nothing".to_string(),
                    found: type_label(&kernel.ret),
                    span: span_from_range(body.expr_span(kernel_expr)),
                });
            }
            Some(Type::Nil)
        }
        _ => None,
    }
}

fn infer_exact_builtin_call(
    body: &Body,
    expr_id: Idx<Expr>,
    args: &Vec<crate::hir::Arg>,
    ctx: &mut TypeContext,
    classes: &ClassIndex,
    enums: &EnumIndex,
    interfaces: &InterfaceIndex,
    functions: &FunctionIndex,
    errors: &mut Vec<TypeError>,
    allow_result: bool,
    in_result_fn: bool,
    params: &[(SmolStr, Type)],
    ret: Type,
) -> Option<Type> {
    let params_vec = params.to_vec();
    check_call_args(
        body,
        expr_id,
        args,
        &params_vec,
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
    Some(ret.clone())
}

fn infer_capture_builtin(
    body: &Body,
    expr_id: Idx<Expr>,
    args: &Vec<crate::hir::Arg>,
    ctx: &mut TypeContext,
    classes: &ClassIndex,
    enums: &EnumIndex,
    interfaces: &InterfaceIndex,
    functions: &FunctionIndex,
    errors: &mut Vec<TypeError>,
    allow_result: bool,
    in_result_fn: bool,
) -> Option<Type> {
    if ctx.in_portable_lane() && !ctx.in_portable_query_kernel_lane() {
        errors.push(TypeError::PortableHostCallForbidden {
            function: ctx.current_function_name(),
            callee: SmolStr::new("capture"),
            span: span_from_range(body.expr_span(expr_id)),
            help: "Captures are host-side execution boundaries; portable scene code must stay pure and capture-free.".to_string(),
        });
    }

    let params = vec![(SmolStr::new("scene"), Type::Unknown)];
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
        false,
        allow_result,
        in_result_fn,
    );

    let target_expr = call_named_arg_value(args, "scene");
    if let Some(target_expr) = target_expr {
        let is_valid = match &body.exprs[target_expr] {
            Expr::Variable(name) => {
                ctx.resolve(name).is_none()
                    && (functions.is_field(name) || functions.is_shape(name) || functions.is_region(name))
            }
            _ => false,
        };
        if !is_valid {
            errors.push(TypeError::CaptureTargetMustBeFieldOrShape {
                span: span_from_range(body.expr_span(target_expr)),
                help: "Pass a top-level field, shape, or region declaration, for example `capture scene_shape` or `capture Highlands`.".to_string(),
            });
        }
    }

    Some(match target_expr {
        Some(target_expr) => match &body.exprs[target_expr] {
            Expr::Variable(name) if functions.is_region(name) => region_capture_type(),
            Expr::Variable(name) if functions.is_shape(name) => shape_capture_type(),
            Expr::Variable(name) if functions.is_field(name) => field_capture_type(),
            _ => Type::Unknown,
        },
        None => Type::Unknown,
    })
}

fn infer_field_query_builtin(
    body: &Body,
    expr_id: Idx<Expr>,
    query_name: &SmolStr,
    args: &Vec<crate::hir::Arg>,
    ctx: &mut TypeContext,
    classes: &ClassIndex,
    enums: &EnumIndex,
    interfaces: &InterfaceIndex,
    functions: &FunctionIndex,
    errors: &mut Vec<TypeError>,
    allow_result: bool,
    in_result_fn: bool,
    ret: Type,
) -> Option<Type> {
    if ctx.in_portable_lane() && !ctx.in_portable_query_kernel_lane() {
        errors.push(TypeError::PortableHostCallForbidden {
            function: ctx.current_function_name(),
            callee: query_name.clone(),
            span: span_from_range(body.expr_span(expr_id)),
            help: "Field sampling queries belong in the host lane for now. Capture the scene first, then sample the capture with `distance_at` / `normal_at`.".to_string(),
        });
    }

    infer_exact_builtin_call(
        body,
        expr_id,
        args,
        ctx,
        classes,
        enums,
        interfaces,
        functions,
        errors,
        allow_result,
        in_result_fn,
        &[(SmolStr::new("capture"), Type::Unknown), (SmolStr::new("point"), Type::Vec3)],
        ret.clone(),
    )?;

    if let Some(capture_expr) = call_named_arg_value(args, "capture") {
        let found = infer_expr(
            body,
            capture_expr,
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
        if let Some(target) = direct_capture_target_name(body, capture_expr) {
            if !(functions.is_field(&target) || functions.is_shape(&target)) {
                errors.push(TypeError::ShapeQueryTargetMustBeShape {
                    query: query_name.clone(),
                    span: span_from_range(body.expr_span(capture_expr)),
                    help: "Pass a capture created from a top-level field or shape declaration, for example `distance_at(capture=capture sphere, ...)`.".to_string(),
                });
            }
        } else if !is_scene_sample_capture_type(&found) {
            errors.push(TypeError::ArgumentTypeMismatch {
                name: SmolStr::new("capture"),
                expected: "FieldCapture or ShapeCapture".to_string(),
                found: type_label(&found),
                span: span_from_range(body.expr_span(capture_expr)),
            });
        }
    }

    Some(ret)
}

fn direct_capture_target_name(body: &Body, expr_id: Idx<Expr>) -> Option<SmolStr> {
    let Expr::Call { callee, args, .. } = &body.exprs[expr_id] else {
        return None;
    };
    let Expr::Variable(name) = &body.exprs[*callee] else {
        return None;
    };
    if name.as_str() != "capture" {
        return None;
    }
    let scene_expr = call_named_arg_value(args, "scene")?;
    let Expr::Variable(target) = &body.exprs[scene_expr] else {
        return None;
    };
    Some(target.clone())
}

fn infer_shape_query_builtin(
    body: &Body,
    expr_id: Idx<Expr>,
    query_name: &SmolStr,
    args: &Vec<crate::hir::Arg>,
    ctx: &mut TypeContext,
    classes: &ClassIndex,
    enums: &EnumIndex,
    interfaces: &InterfaceIndex,
    functions: &FunctionIndex,
    errors: &mut Vec<TypeError>,
    allow_result: bool,
    in_result_fn: bool,
    ret: Type,
) -> Option<Type> {
    if ctx.in_portable_lane() && !ctx.in_portable_query_kernel_lane() {
        errors.push(TypeError::PortableHostCallForbidden {
            function: ctx.current_function_name(),
            callee: query_name.clone(),
            span: span_from_range(body.expr_span(expr_id)),
            help: "Shape tracing queries belong in the host lane for now. Capture the scene first, then trace or sample that capture with `trace_shape` / `surface_at`.".to_string(),
        });
    }

    let expected = match query_name.as_str() {
        "trace_shape" => vec![
            (SmolStr::new("capture"), Type::Unknown),
            (SmolStr::new("ray"), portable_named_type("RayQuery")),
        ],
        _ => vec![
            (SmolStr::new("capture"), Type::Unknown),
            (SmolStr::new("hit"), Type::Named(SmolStr::new("Hit3"), Vec::new())),
        ],
    };

    infer_exact_builtin_call(
        body,
        expr_id,
        args,
        ctx,
        classes,
        enums,
        interfaces,
        functions,
        errors,
        allow_result,
        in_result_fn,
        &expected,
        ret.clone(),
    )?;

    if let Some(capture_expr) = call_named_arg_value(args, "capture") {
        let found = infer_expr(
            body,
            capture_expr,
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
        if let Some(target) = direct_capture_target_name(body, capture_expr) {
            if !functions.is_shape(&target) {
                errors.push(TypeError::ShapeQueryTargetMustBeShape {
                    query: query_name.clone(),
                    span: span_from_range(body.expr_span(capture_expr)),
                    help: "Pass a capture created from a top-level shape declaration, for example `trace_shape(capture=capture scene_shape, ...)`.".to_string(),
                });
            }
        } else if types_known(&shape_capture_type(), &found)
            && !is_assignable(&shape_capture_type(), &found, classes, interfaces)
        {
            errors.push(TypeError::ShapeQueryTargetMustBeShape {
                query: query_name.clone(),
                span: span_from_range(body.expr_span(capture_expr)),
                help: "Store captures from top-level shapes in `ShapeCapture` values before using shape queries like `trace_shape` or `surface_at`.".to_string(),
            });
        } else if !types_known(&shape_capture_type(), &found) && found != Type::Unknown {
            errors.push(TypeError::ArgumentTypeMismatch {
                name: SmolStr::new("capture"),
                expected: type_label(&shape_capture_type()),
                found: type_label(&found),
                span: span_from_range(body.expr_span(capture_expr)),
            });
        }
    }

    Some(ret)
}

fn infer_shape_point_query_builtin(
    body: &Body,
    expr_id: Idx<Expr>,
    query_name: &SmolStr,
    args: &Vec<crate::hir::Arg>,
    ctx: &mut TypeContext,
    classes: &ClassIndex,
    enums: &EnumIndex,
    interfaces: &InterfaceIndex,
    functions: &FunctionIndex,
    errors: &mut Vec<TypeError>,
    allow_result: bool,
    in_result_fn: bool,
    ret: Type,
) -> Option<Type> {
    if ctx.in_portable_lane() && !ctx.in_portable_query_kernel_lane() {
        errors.push(TypeError::PortableHostCallForbidden {
            function: ctx.current_function_name(),
            callee: query_name.clone(),
            span: span_from_range(body.expr_span(expr_id)),
            help: "Scene sampling queries belong in the host lane for now. Capture the scene first, then sample that capture with `radiance_at` / `medium_at`.".to_string(),
        });
    }

    let expected = match query_name.as_str() {
        "radiance_at" => vec![
            (SmolStr::new("capture"), Type::Unknown),
            (
                SmolStr::new("sample"),
                portable_named_type("PointDirectionQuery"),
            ),
        ],
        _ => vec![
            (SmolStr::new("capture"), Type::Unknown),
            (SmolStr::new("point"), Type::Vec3),
        ],
    };

    infer_exact_builtin_call(
        body,
        expr_id,
        args,
        ctx,
        classes,
        enums,
        interfaces,
        functions,
        errors,
        allow_result,
        in_result_fn,
        &expected,
        ret.clone(),
    )?;

    if let Some(capture_expr) = call_named_arg_value(args, "capture") {
        let found = infer_expr(
            body,
            capture_expr,
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
        if let Some(target) = direct_capture_target_name(body, capture_expr) {
            if !functions.is_shape(&target) {
                errors.push(TypeError::ShapeQueryTargetMustBeShape {
                    query: query_name.clone(),
                    span: span_from_range(body.expr_span(capture_expr)),
                    help: "Pass a capture created from a top-level shape declaration, for example `trace_shape(capture=capture scene_shape, ...)`.".to_string(),
                });
            }
        } else if types_known(&shape_capture_type(), &found)
            && !is_assignable(&shape_capture_type(), &found, classes, interfaces)
        {
            errors.push(TypeError::ShapeQueryTargetMustBeShape {
                query: query_name.clone(),
                span: span_from_range(body.expr_span(capture_expr)),
                help: format!(
                    "Store captures from top-level shapes in `ShapeCapture` values before using `{query_name}`."
                ),
            });
        } else if !types_known(&shape_capture_type(), &found) && found != Type::Unknown {
            errors.push(TypeError::ArgumentTypeMismatch {
                name: SmolStr::new("capture"),
                expected: type_label(&shape_capture_type()),
                found: type_label(&found),
                span: span_from_range(body.expr_span(capture_expr)),
            });
        }
    }

    Some(ret)
}

fn infer_scene_backend_builtin(
    body: &Body,
    expr_id: Idx<Expr>,
    name: &SmolStr,
    args: &Vec<crate::hir::Arg>,
    ctx: &mut TypeContext,
    classes: &ClassIndex,
    enums: &EnumIndex,
    interfaces: &InterfaceIndex,
    functions: &FunctionIndex,
    errors: &mut Vec<TypeError>,
    allow_result: bool,
    in_result_fn: bool,
) -> Option<Type> {
    if ctx.in_portable_lane() && !ctx.in_portable_query_kernel_lane() {
        errors.push(TypeError::PortableHostCallForbidden {
            function: ctx.current_function_name(),
            callee: name.clone(),
            span: span_from_range(body.expr_span(expr_id)),
            help: "Bulk scene dispatch backends are host-side orchestration only.".to_string(),
        });
    }
    infer_exact_builtin_call(
        body,
        expr_id,
        args,
        ctx,
        classes,
        enums,
        interfaces,
        functions,
        errors,
        allow_result,
        in_result_fn,
        &[],
        dispatch_backend_type(),
    )
}

fn infer_world_distance_query_builtin(
    body: &Body,
    expr_id: Idx<Expr>,
    query_name: &SmolStr,
    args: &Vec<crate::hir::Arg>,
    ctx: &mut TypeContext,
    classes: &ClassIndex,
    enums: &EnumIndex,
    interfaces: &InterfaceIndex,
    functions: &FunctionIndex,
    errors: &mut Vec<TypeError>,
    allow_result: bool,
    in_result_fn: bool,
    ret: Type,
) -> Option<Type> {
    if ctx.in_portable_lane() && !ctx.in_portable_query_kernel_lane() {
        errors.push(TypeError::PortableHostCallForbidden {
            function: ctx.current_function_name(),
            callee: query_name.clone(),
            span: span_from_range(body.expr_span(expr_id)),
            help: "World queries belong in the host lane. Capture the region first, derive a domain plan, then query that captured world.".to_string(),
        });
    }
    let mut expected = vec![
        (SmolStr::new("capture"), region_capture_type()),
        (SmolStr::new("domain"), scene_domain_type()),
        (SmolStr::new("point"), Type::Vec3),
    ];
    if call_named_arg_value(args, "backend").is_some() {
        expected.push((
            SmolStr::new("backend"),
            portable_named_type("DispatchBackend"),
        ));
    }
    infer_exact_builtin_call(
        body,
        expr_id,
        args,
        ctx,
        classes,
        enums,
        interfaces,
        functions,
        errors,
        allow_result,
        in_result_fn,
        &expected,
        ret.clone(),
    )?;
    if let Some(capture_expr) = call_named_arg_value(args, "capture") {
        validate_region_capture_argument(
            body,
            capture_expr,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
        );
    }
    if let Some(domain_expr) = call_named_arg_value(args, "domain") {
        validate_scene_domain_argument(
            body,
            domain_expr,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
        );
    }
    Some(ret)
}

fn infer_world_shape_query_builtin(
    body: &Body,
    expr_id: Idx<Expr>,
    query_name: &SmolStr,
    args: &Vec<crate::hir::Arg>,
    ctx: &mut TypeContext,
    classes: &ClassIndex,
    enums: &EnumIndex,
    interfaces: &InterfaceIndex,
    functions: &FunctionIndex,
    errors: &mut Vec<TypeError>,
    allow_result: bool,
    in_result_fn: bool,
    ret: Type,
) -> Option<Type> {
    if ctx.in_portable_lane() && !ctx.in_portable_query_kernel_lane() {
        errors.push(TypeError::PortableHostCallForbidden {
            function: ctx.current_function_name(),
            callee: query_name.clone(),
            span: span_from_range(body.expr_span(expr_id)),
            help: "World queries belong in the host lane. Capture the region first, derive a domain plan, then query that captured world.".to_string(),
        });
    }
    let expected = match query_name.as_str() {
        "trace_world" => vec![
            (SmolStr::new("capture"), region_capture_type()),
            (SmolStr::new("domain"), scene_domain_type()),
            (SmolStr::new("ray"), portable_named_type("RayQuery")),
        ],
        _ => vec![
            (SmolStr::new("capture"), region_capture_type()),
            (SmolStr::new("domain"), scene_domain_type()),
            (SmolStr::new("hit"), portable_named_type("Hit3")),
        ],
    };
    let mut expected = expected;
    if call_named_arg_value(args, "backend").is_some() {
        expected.push((
            SmolStr::new("backend"),
            portable_named_type("DispatchBackend"),
        ));
    }
    infer_exact_builtin_call(
        body,
        expr_id,
        args,
        ctx,
        classes,
        enums,
        interfaces,
        functions,
        errors,
        allow_result,
        in_result_fn,
        &expected,
        ret.clone(),
    )?;
    if let Some(capture_expr) = call_named_arg_value(args, "capture") {
        validate_region_capture_argument(
            body,
            capture_expr,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
        );
    }
    if let Some(domain_expr) = call_named_arg_value(args, "domain") {
        validate_scene_domain_argument(
            body,
            domain_expr,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
        );
    }
    Some(ret)
}

fn infer_world_point_query_builtin(
    body: &Body,
    expr_id: Idx<Expr>,
    query_name: &SmolStr,
    args: &Vec<crate::hir::Arg>,
    ctx: &mut TypeContext,
    classes: &ClassIndex,
    enums: &EnumIndex,
    interfaces: &InterfaceIndex,
    functions: &FunctionIndex,
    errors: &mut Vec<TypeError>,
    allow_result: bool,
    in_result_fn: bool,
    ret: Type,
) -> Option<Type> {
    if ctx.in_portable_lane() && !ctx.in_portable_query_kernel_lane() {
        errors.push(TypeError::PortableHostCallForbidden {
            function: ctx.current_function_name(),
            callee: query_name.clone(),
            span: span_from_range(body.expr_span(expr_id)),
            help: "World queries belong in the host lane. Capture the region first, derive a domain plan, then query that captured world.".to_string(),
        });
    }
    let expected = match query_name.as_str() {
        "radiance_world" => vec![
            (SmolStr::new("capture"), region_capture_type()),
            (SmolStr::new("domain"), scene_domain_type()),
            (
                SmolStr::new("sample"),
                portable_named_type("PointDirectionQuery"),
            ),
        ],
        _ => vec![
            (SmolStr::new("capture"), region_capture_type()),
            (SmolStr::new("domain"), scene_domain_type()),
            (SmolStr::new("point"), Type::Vec3),
        ],
    };
    let mut expected = expected;
    if call_named_arg_value(args, "backend").is_some() {
        expected.push((
            SmolStr::new("backend"),
            portable_named_type("DispatchBackend"),
        ));
    }
    infer_exact_builtin_call(
        body,
        expr_id,
        args,
        ctx,
        classes,
        enums,
        interfaces,
        functions,
        errors,
        allow_result,
        in_result_fn,
        &expected,
        ret.clone(),
    )?;
    if let Some(capture_expr) = call_named_arg_value(args, "capture") {
        validate_region_capture_argument(
            body,
            capture_expr,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
        );
    }
    if let Some(domain_expr) = call_named_arg_value(args, "domain") {
        validate_scene_domain_argument(
            body,
            domain_expr,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
        );
    }
    Some(ret)
}

fn validate_region_capture_argument(
    body: &Body,
    capture_expr: Idx<Expr>,
    ctx: &mut TypeContext,
    classes: &ClassIndex,
    enums: &EnumIndex,
    interfaces: &InterfaceIndex,
    functions: &FunctionIndex,
    errors: &mut Vec<TypeError>,
    allow_result: bool,
    in_result_fn: bool,
) {
    let found = infer_expr(
        body,
        capture_expr,
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
    if let Some(target) = direct_capture_target_name(body, capture_expr) {
        if !functions.is_region(&target) {
            errors.push(TypeError::ArgumentTypeMismatch {
                name: SmolStr::new("capture"),
                expected: "RegionCapture".to_string(),
                found: type_label(&found),
                span: span_from_range(body.expr_span(capture_expr)),
            });
        }
    } else if !is_region_capture_type(&found) && found != Type::Unknown {
        errors.push(TypeError::ArgumentTypeMismatch {
            name: SmolStr::new("capture"),
            expected: "RegionCapture".to_string(),
            found: type_label(&found),
            span: span_from_range(body.expr_span(capture_expr)),
        });
    }
}

fn validate_scene_domain_argument(
    body: &Body,
    domain_expr: Idx<Expr>,
    ctx: &mut TypeContext,
    classes: &ClassIndex,
    enums: &EnumIndex,
    interfaces: &InterfaceIndex,
    functions: &FunctionIndex,
    errors: &mut Vec<TypeError>,
    allow_result: bool,
    in_result_fn: bool,
) {
    let found = infer_expr(
        body,
        domain_expr,
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
    if found != Type::Unknown && !is_assignable(&scene_domain_type(), &found, classes, interfaces)
    {
        errors.push(TypeError::ArgumentTypeMismatch {
            name: SmolStr::new("domain"),
            expected: "SceneDomain".to_string(),
            found: type_label(&found),
            span: span_from_range(body.expr_span(domain_expr)),
        });
    }
}

fn infer_shape_batch_query_builtin(
    body: &Body,
    expr_id: Idx<Expr>,
    query_name: &SmolStr,
    args: &Vec<crate::hir::Arg>,
    ctx: &mut TypeContext,
    classes: &ClassIndex,
    enums: &EnumIndex,
    interfaces: &InterfaceIndex,
    functions: &FunctionIndex,
    errors: &mut Vec<TypeError>,
    allow_result: bool,
    in_result_fn: bool,
    ret: Type,
) -> Option<Type> {
    if ctx.in_portable_lane() && !ctx.in_portable_query_kernel_lane() {
        errors.push(TypeError::PortableHostCallForbidden {
            function: ctx.current_function_name(),
            callee: query_name.clone(),
            span: span_from_range(body.expr_span(expr_id)),
            help: "Bulk scene dispatch belongs in the host lane; portable scene code stays pure.".to_string(),
        });
    }
    let expected = match query_name.as_str() {
        "trace_shape_batch" | "occluded_batch" => vec![
            (SmolStr::new("capture"), Type::Unknown),
            (SmolStr::new("rays"), Type::List(Box::new(portable_named_type("RayQuery")))),
            (SmolStr::new("backend"), dispatch_backend_type()),
        ],
        _ => vec![
            (SmolStr::new("capture"), Type::Unknown),
            (SmolStr::new("hits"), Type::List(Box::new(portable_named_type("Hit3")))),
            (SmolStr::new("backend"), dispatch_backend_type()),
        ],
    };
    infer_exact_builtin_call(
        body,
        expr_id,
        args,
        ctx,
        classes,
        enums,
        interfaces,
        functions,
        errors,
        allow_result,
        in_result_fn,
        &expected,
        ret.clone(),
    )?;
    if let Some(capture_expr) = call_named_arg_value(args, "capture") {
        let found = infer_expr(
            body,
            capture_expr,
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
        if let Some(target) = direct_capture_target_name(body, capture_expr) {
            if !functions.is_shape(&target) {
                errors.push(TypeError::ShapeQueryTargetMustBeShape {
                    query: query_name.clone(),
                    span: span_from_range(body.expr_span(capture_expr)),
                    help: "Pass a capture created from a top-level shape declaration, for example `trace_shape_batch(capture=capture scene_shape, rays=queries, backend=dispatch_backend_cpu())`.".to_string(),
                });
            }
        } else if types_known(&shape_capture_type(), &found)
            && !is_assignable(&shape_capture_type(), &found, classes, interfaces)
        {
            errors.push(TypeError::ShapeQueryTargetMustBeShape {
                query: query_name.clone(),
                span: span_from_range(body.expr_span(capture_expr)),
                help: "Store captures from top-level shapes in `ShapeCapture` values before using shape batch queries.".to_string(),
            });
        } else if !types_known(&shape_capture_type(), &found) && found != Type::Unknown {
            errors.push(TypeError::ArgumentTypeMismatch {
                name: SmolStr::new("capture"),
                expected: type_label(&shape_capture_type()),
                found: type_label(&found),
                span: span_from_range(body.expr_span(capture_expr)),
            });
        }
    }
    Some(ret)
}

fn infer_field_batch_query_builtin(
    body: &Body,
    expr_id: Idx<Expr>,
    query_name: &SmolStr,
    args: &Vec<crate::hir::Arg>,
    ctx: &mut TypeContext,
    classes: &ClassIndex,
    enums: &EnumIndex,
    interfaces: &InterfaceIndex,
    functions: &FunctionIndex,
    errors: &mut Vec<TypeError>,
    allow_result: bool,
    in_result_fn: bool,
    ret: Type,
) -> Option<Type> {
    if ctx.in_portable_lane() && !ctx.in_portable_query_kernel_lane() {
        errors.push(TypeError::PortableHostCallForbidden {
            function: ctx.current_function_name(),
            callee: query_name.clone(),
            span: span_from_range(body.expr_span(expr_id)),
            help: "Bulk field sampling belongs in the host lane; portable `field` declarations stay pure.".to_string(),
        });
    }
    infer_exact_builtin_call(
        body,
        expr_id,
        args,
        ctx,
        classes,
        enums,
        interfaces,
        functions,
        errors,
        allow_result,
        in_result_fn,
        &[
            (SmolStr::new("capture"), Type::Unknown),
            (
                SmolStr::new("points"),
                Type::List(Box::new(portable_named_type("PointQuery"))),
            ),
            (SmolStr::new("backend"), dispatch_backend_type()),
        ],
        ret.clone(),
    )?;
    if let Some(capture_expr) = call_named_arg_value(args, "capture") {
        let found = infer_expr(
            body,
            capture_expr,
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
        if let Some(target) = direct_capture_target_name(body, capture_expr) {
            if !(functions.is_field(&target) || functions.is_shape(&target)) {
                errors.push(TypeError::ShapeQueryTargetMustBeShape {
                    query: query_name.clone(),
                    span: span_from_range(body.expr_span(capture_expr)),
                    help: "Pass a capture created from a top-level field or shape declaration, for example `distance_at_batch(capture=capture sphere, points=queries, backend=dispatch_backend_cpu())`.".to_string(),
                });
            }
        } else if !is_scene_sample_capture_type(&found) {
            errors.push(TypeError::ArgumentTypeMismatch {
                name: SmolStr::new("capture"),
                expected: "FieldCapture or ShapeCapture".to_string(),
                found: type_label(&found),
                span: span_from_range(body.expr_span(capture_expr)),
            });
        }
    }
    Some(ret)
}

fn field_capture_type() -> Type {
    portable_named_type("FieldCapture")
}

fn shape_capture_type() -> Type {
    portable_named_type("ShapeCapture")
}

fn region_capture_type() -> Type {
    portable_named_type("RegionCapture")
}

fn detail_tier_type() -> Type {
    portable_named_type("DetailTier")
}

fn is_region_capture_type(ty: &Type) -> bool {
    *ty == region_capture_type()
}

fn dispatch_backend_type() -> Type {
    portable_named_type("DispatchBackend")
}

fn scene_domain_type() -> Type {
    portable_named_type("SceneDomain")
}

fn is_scene_sample_capture_type(found: &Type) -> bool {
    *found == field_capture_type() || *found == shape_capture_type()
}

fn infer_math_builtin_call(
    body: &Body,
    expr_id: Idx<Expr>,
    name: &SmolStr,
    args: &Vec<crate::hir::Arg>,
    ctx: &mut TypeContext,
    classes: &ClassIndex,
    enums: &EnumIndex,
    interfaces: &InterfaceIndex,
    functions: &FunctionIndex,
    errors: &mut Vec<TypeError>,
    allow_result: bool,
    in_result_fn: bool,
) -> Option<Type> {
    let span = span_from_range(body.expr_span(expr_id));
    match name.as_str() {
        "vec2" => {
            let params = vec![
                (SmolStr::new("x"), Type::F32),
                (SmolStr::new("y"), Type::F32),
            ];
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
                false,
                allow_result,
                in_result_fn,
            );
            Some(Type::Vec2)
        }
        "vec3" => {
            let params = vec![
                (SmolStr::new("x"), Type::F32),
                (SmolStr::new("y"), Type::F32),
                (SmolStr::new("z"), Type::F32),
            ];
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
                false,
                allow_result,
                in_result_fn,
            );
            Some(Type::Vec3)
        }
        "vec4" => {
            let params = vec![
                (SmolStr::new("x"), Type::F32),
                (SmolStr::new("y"), Type::F32),
                (SmolStr::new("z"), Type::F32),
                (SmolStr::new("w"), Type::F32),
            ];
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
                false,
                allow_result,
                in_result_fn,
            );
            Some(Type::Vec4)
        }
        "quat" => {
            let params = vec![
                (SmolStr::new("x"), Type::F32),
                (SmolStr::new("y"), Type::F32),
                (SmolStr::new("z"), Type::F32),
                (SmolStr::new("w"), Type::F32),
            ];
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
                false,
                allow_result,
                in_result_fn,
            );
            Some(Type::Quat)
        }
        "mat3_identity" => {
            if args.len() != 0 {
                errors.push(TypeError::ArgumentCountMismatch {
                    expected: 0,
                    found: args.len(),
                    span,
                });
            }
            Some(Type::Mat3)
        }
        "mat3_cols" => {
            let params = vec![
                (SmolStr::new("c0"), Type::Vec3),
                (SmolStr::new("c1"), Type::Vec3),
                (SmolStr::new("c2"), Type::Vec3),
            ];
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
                false,
                allow_result,
                in_result_fn,
            );
            Some(Type::Mat3)
        }
        "mat4_identity" => {
            if args.len() != 0 {
                errors.push(TypeError::ArgumentCountMismatch {
                    expected: 0,
                    found: args.len(),
                    span,
                });
            }
            Some(Type::Mat4)
        }
        "mat4_cols" => {
            let params = vec![
                (SmolStr::new("c0"), Type::Vec4),
                (SmolStr::new("c1"), Type::Vec4),
                (SmolStr::new("c2"), Type::Vec4),
                (SmolStr::new("c3"), Type::Vec4),
            ];
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
                false,
                allow_result,
                in_result_fn,
            );
            Some(Type::Mat4)
        }
        "bounds2" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[
                (SmolStr::new("min"), Type::Vec2),
                (SmolStr::new("max"), Type::Vec2),
            ],
            portable_named_type("Bounds2"),
        ),
        "bounds3" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[
                (SmolStr::new("min"), Type::Vec3),
                (SmolStr::new("max"), Type::Vec3),
            ],
            portable_named_type("Bounds3"),
        ),
        "ray3" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[
                (SmolStr::new("origin"), Type::Vec3),
                (SmolStr::new("direction"), Type::Vec3),
            ],
            portable_named_type("Ray3"),
        ),
        "transform3" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[
                (SmolStr::new("matrix"), Type::Mat4),
                (SmolStr::new("inverse"), Type::Mat4),
            ],
            portable_named_type("Transform3"),
        ),
        "transform3_identity" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[],
            portable_named_type("Transform3"),
        ),
        "dot" => {
            if args.len() != 2 {
                errors.push(TypeError::ArgumentCountMismatch {
                    expected: 2,
                    found: args.len(),
                    span,
                });
                return Some(Type::Unknown);
            }
            let left_ty = infer_call_arg_type(
                body,
                &args[0],
                ctx,
                classes,
                enums,
                interfaces,
                functions,
                errors,
                allow_result,
                in_result_fn,
            );
            let right_ty = infer_call_arg_type(
                body,
                &args[1],
                ctx,
                classes,
                enums,
                interfaces,
                functions,
                errors,
                allow_result,
                in_result_fn,
            );
            if same_vector_like_kind(&left_ty, &right_ty).is_none() {
                push_math_builtin_arg_mismatch(
                    "left",
                    &left_ty,
                    args,
                    0,
                    "Vec2, Vec3, Vec4, or Quat",
                    errors,
                );
                push_math_builtin_arg_mismatch(
                    "right",
                    &right_ty,
                    args,
                    1,
                    "Vec2, Vec3, Vec4, or Quat",
                    errors,
                );
                return Some(Type::Unknown);
            }
            Some(Type::F32)
        }
        "bounds2_center" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[(SmolStr::new("bounds"), portable_named_type("Bounds2"))],
            Type::Vec2,
        ),
        "bounds2_size" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[(SmolStr::new("bounds"), portable_named_type("Bounds2"))],
            Type::Vec2,
        ),
        "bounds3_center" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[(SmolStr::new("bounds"), portable_named_type("Bounds3"))],
            Type::Vec3,
        ),
        "bounds3_size" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[(SmolStr::new("bounds"), portable_named_type("Bounds3"))],
            Type::Vec3,
        ),
        "length" | "normalize" => {
            if args.len() != 1 {
                errors.push(TypeError::ArgumentCountMismatch {
                    expected: 1,
                    found: args.len(),
                    span,
                });
                return Some(Type::Unknown);
            }
            let value_ty = infer_call_arg_type(
                body,
                &args[0],
                ctx,
                classes,
                enums,
                interfaces,
                functions,
                errors,
                allow_result,
                in_result_fn,
            );
            if !is_vector_like_type(&value_ty) {
                push_math_builtin_arg_mismatch(
                    "value",
                    &value_ty,
                    args,
                    0,
                    "Vec2, Vec3, Vec4, or Quat",
                    errors,
                );
                return Some(Type::Unknown);
            }
            Some(if name.as_str() == "length" {
                Type::F32
            } else {
                value_ty
            })
        }
        "cross" => {
            let params = vec![
                (SmolStr::new("left"), Type::Vec3),
                (SmolStr::new("right"), Type::Vec3),
            ];
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
                false,
                allow_result,
                in_result_fn,
            );
            Some(Type::Vec3)
        }
        "distance" | "reflect" => {
            if args.len() != 2 {
                errors.push(TypeError::ArgumentCountMismatch {
                    expected: 2,
                    found: args.len(),
                    span,
                });
                return Some(Type::Unknown);
            }
            let left_ty = infer_call_arg_type(
                body,
                &args[0],
                ctx,
                classes,
                enums,
                interfaces,
                functions,
                errors,
                allow_result,
                in_result_fn,
            );
            let right_ty = infer_call_arg_type(
                body,
                &args[1],
                ctx,
                classes,
                enums,
                interfaces,
                functions,
                errors,
                allow_result,
                in_result_fn,
            );
            if !(is_vector_only_type(&left_ty) && same_vector_kind(&left_ty, &right_ty)) {
                push_math_builtin_arg_mismatch(
                    "left",
                    &left_ty,
                    args,
                    0,
                    "matching Vec2, Vec3, or Vec4 operands",
                    errors,
                );
                push_math_builtin_arg_mismatch(
                    "right",
                    &right_ty,
                    args,
                    1,
                    "matching Vec2, Vec3, or Vec4 operands",
                    errors,
                );
                return Some(Type::Unknown);
            }
            if name.as_str() == "distance" {
                Some(Type::F32)
            } else if is_vector_only_type(&left_ty) {
                Some(left_ty)
            } else {
                push_math_builtin_arg_mismatch(
                    "left",
                    &left_ty,
                    args,
                    0,
                    "Vec2, Vec3, or Vec4",
                    errors,
                );
                push_math_builtin_arg_mismatch(
                    "right",
                    &right_ty,
                    args,
                    1,
                    "Vec2, Vec3, or Vec4",
                    errors,
                );
                Some(Type::Unknown)
            }
        }
        "transform_point" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[
                (SmolStr::new("transform"), portable_named_type("Transform3")),
                (SmolStr::new("point"), Type::Vec3),
            ],
            Type::Vec3,
        ),
        "transform_vector" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[
                (SmolStr::new("transform"), portable_named_type("Transform3")),
                (SmolStr::new("vector"), Type::Vec3),
            ],
            Type::Vec3,
        ),
        "transform_normal" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[
                (SmolStr::new("transform"), portable_named_type("Transform3")),
                (SmolStr::new("normal"), Type::Vec3),
            ],
            Type::Vec3,
        ),
        "compose_transform3" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[
                (SmolStr::new("left"), portable_named_type("Transform3")),
                (SmolStr::new("right"), portable_named_type("Transform3")),
            ],
            portable_named_type("Transform3"),
        ),
        "inverse_transform3" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[(SmolStr::new("transform"), portable_named_type("Transform3"))],
            portable_named_type("Transform3"),
        ),
        "rounded_box" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[
                (SmolStr::new("p"), Type::Vec3),
                (SmolStr::new("half"), Type::Vec3),
                (SmolStr::new("radius"), Type::F32),
            ],
            Type::F32,
        ),
        "ellipsoid" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[
                (SmolStr::new("p"), Type::Vec3),
                (SmolStr::new("radii"), Type::Vec3),
            ],
            Type::F32,
        ),
        "cone" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[
                (SmolStr::new("p"), Type::Vec3),
                (SmolStr::new("radius"), Type::F32),
                (SmolStr::new("half_height"), Type::F32),
            ],
            Type::F32,
        ),
        "capped_cone" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[
                (SmolStr::new("p"), Type::Vec3),
                (SmolStr::new("radius_bottom"), Type::F32),
                (SmolStr::new("radius_top"), Type::F32),
                (SmolStr::new("half_height"), Type::F32),
            ],
            Type::F32,
        ),
        "box_frame" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[
                (SmolStr::new("p"), Type::Vec3),
                (SmolStr::new("half"), Type::Vec3),
                (SmolStr::new("thickness"), Type::F32),
            ],
            Type::F32,
        ),
        "slab" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[
                (SmolStr::new("p"), Type::Vec3),
                (SmolStr::new("thickness"), Type::F32),
            ],
            Type::F32,
        ),
        "triangle_prism" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[
                (SmolStr::new("p"), Type::Vec3),
                (SmolStr::new("half"), Type::Vec2),
                (SmolStr::new("half_height"), Type::F32),
            ],
            Type::F32,
        ),
        "hex_prism" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[
                (SmolStr::new("p"), Type::Vec3),
                (SmolStr::new("half"), Type::Vec2),
                (SmolStr::new("half_height"), Type::F32),
            ],
            Type::F32,
        ),
        "field_union" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[
                (SmolStr::new("left"), Type::F32),
                (SmolStr::new("right"), Type::F32),
            ],
            Type::F32,
        ),
        "field_intersection" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[
                (SmolStr::new("left"), Type::F32),
                (SmolStr::new("right"), Type::F32),
            ],
            Type::F32,
        ),
        "field_subtract" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[
                (SmolStr::new("left"), Type::F32),
                (SmolStr::new("right"), Type::F32),
            ],
            Type::F32,
        ),
        "field_translate_point" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[
                (SmolStr::new("translate"), Type::Vec3),
                (SmolStr::new("point"), Type::Vec3),
            ],
            Type::Vec3,
        ),
        "field_rotate_point" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[
                (SmolStr::new("rotate"), Type::Vec3),
                (SmolStr::new("point"), Type::Vec3),
            ],
            Type::Vec3,
        ),
        "field_uniform_scale_point" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[
                (SmolStr::new("scale"), Type::F32),
                (SmolStr::new("point"), Type::Vec3),
            ],
            Type::Vec3,
        ),
        "field_affine_transform_point" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[
                (SmolStr::new("transform"), Type::Vec3),
                (SmolStr::new("point"), Type::Vec3),
            ],
            Type::Vec3,
        ),
        "field_warp_point" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[
                (SmolStr::new("warp"), Type::Vec3),
                (SmolStr::new("point"), Type::Vec3),
            ],
            Type::Vec3,
        ),
        "field_repeat_linear_point" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[
                (SmolStr::new("repeat"), Type::Vec3),
                (SmolStr::new("point"), Type::Vec3),
            ],
            Type::Vec3,
        ),
        "field_repeat_grid_point" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[
                (SmolStr::new("repeat"), Type::Vec3),
                (SmolStr::new("point"), Type::Vec3),
            ],
            Type::Vec3,
        ),
        "field_radial_repeat_point" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[
                (SmolStr::new("radial"), Type::Vec3),
                (SmolStr::new("point"), Type::Vec3),
            ],
            Type::Vec3,
        ),
        "field_mirror_array_point" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[
                (SmolStr::new("mirror"), Type::Vec3),
                (SmolStr::new("point"), Type::Vec3),
            ],
            Type::Vec3,
        ),
        "field_instance_array_point" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[
                (SmolStr::new("instance"), portable_named_type("Transform3")),
                (SmolStr::new("point"), Type::Vec3),
            ],
            Type::Vec3,
        ),
        "field_smooth_union" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[
                (SmolStr::new("smoothing"), Type::F32),
                (SmolStr::new("left"), Type::F32),
                (SmolStr::new("right"), Type::F32),
            ],
            Type::F32,
        ),
        "field_smooth_intersection" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[
                (SmolStr::new("smoothing"), Type::F32),
                (SmolStr::new("left"), Type::F32),
                (SmolStr::new("right"), Type::F32),
            ],
            Type::F32,
        ),
        "field_smooth_subtract" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[
                (SmolStr::new("smoothing"), Type::F32),
                (SmolStr::new("left"), Type::F32),
                (SmolStr::new("right"), Type::F32),
            ],
            Type::F32,
        ),
        "field_bend_point" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[
                (SmolStr::new("bend"), Type::Vec3),
                (SmolStr::new("point"), Type::Vec3),
            ],
            Type::Vec3,
        ),
        "field_twist_point" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[
                (SmolStr::new("twist"), Type::Vec3),
                (SmolStr::new("point"), Type::Vec3),
            ],
            Type::Vec3,
        ),
        "field_taper_point" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[
                (SmolStr::new("taper"), Type::Vec3),
                (SmolStr::new("point"), Type::Vec3),
            ],
            Type::Vec3,
        ),
        "field_displace_point" => infer_exact_builtin_call(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            &[
                (SmolStr::new("displace"), Type::Vec3),
                (SmolStr::new("point"), Type::Vec3),
            ],
            Type::Vec3,
        ),
        "capture" => infer_capture_builtin(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
        ),
        "dispatch_backend_cpu"
        | "dispatch_backend_virtual_gpu"
        | "dispatch_backend_wgsl"
        | "dispatch_backend_auto" => {
            infer_scene_backend_builtin(
                body,
                expr_id,
                name,
                args,
                ctx,
                classes,
                enums,
                interfaces,
                functions,
                errors,
                allow_result,
                in_result_fn,
            )
        }
        "distance_at" => infer_field_query_builtin(
            body,
            expr_id,
            name,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            Type::F32,
        ),
        "normal_at" => infer_field_query_builtin(
            body,
            expr_id,
            name,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            Type::Vec3,
        ),
        "trace_shape" => infer_shape_query_builtin(
            body,
            expr_id,
            name,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            Type::Named(SmolStr::new("Hit3"), Vec::new()),
        ),
        "surface_at" => infer_shape_query_builtin(
            body,
            expr_id,
            name,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            Type::Named(SmolStr::new("Surface"), Vec::new()),
        ),
        "radiance_at" => infer_shape_point_query_builtin(
            body,
            expr_id,
            name,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            Type::Vec3,
        ),
        "medium_at" => infer_shape_point_query_builtin(
            body,
            expr_id,
            name,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            portable_named_type("Medium"),
        ),
        "distance_world" => infer_world_distance_query_builtin(
            body,
            expr_id,
            name,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            Type::F32,
        ),
        "normal_world" => infer_world_distance_query_builtin(
            body,
            expr_id,
            name,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            Type::Vec3,
        ),
        "trace_world" => infer_world_shape_query_builtin(
            body,
            expr_id,
            name,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            portable_named_type("Hit3"),
        ),
        "surface_world" => infer_world_shape_query_builtin(
            body,
            expr_id,
            name,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            portable_named_type("Surface"),
        ),
        "radiance_world" => infer_world_point_query_builtin(
            body,
            expr_id,
            name,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            Type::Vec3,
        ),
        "medium_world" => infer_world_point_query_builtin(
            body,
            expr_id,
            name,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            portable_named_type("Medium"),
        ),
        "trace_shape_batch" => infer_shape_batch_query_builtin(
            body,
            expr_id,
            name,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            Type::List(Box::new(portable_named_type("Hit3"))),
        ),
        "surface_at_batch" => infer_shape_batch_query_builtin(
            body,
            expr_id,
            name,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            Type::List(Box::new(portable_named_type("Surface"))),
        ),
        "occluded_batch" => infer_shape_batch_query_builtin(
            body,
            expr_id,
            name,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            Type::List(Box::new(portable_named_type("OcclusionResult"))),
        ),
        "distance_at_batch" => infer_field_batch_query_builtin(
            body,
            expr_id,
            name,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            Type::List(Box::new(portable_named_type("DistanceResult"))),
        ),
        "normal_at_batch" => infer_field_batch_query_builtin(
            body,
            expr_id,
            name,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            Type::List(Box::new(portable_named_type("NormalResult"))),
        ),
        "min" | "max" | "pow" => infer_componentwise_binary_builtin(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
        ),
        "clamp" | "mix" => infer_componentwise_ternary_builtin(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
        ),
        "abs" | "sign" | "floor" | "ceil" | "fract" | "sin" | "cos" | "sqrt" => {
            infer_componentwise_unary_builtin(
                body,
                expr_id,
                args,
                ctx,
                classes,
                enums,
                interfaces,
                functions,
                errors,
                allow_result,
                in_result_fn,
            )
        }
        "f32" => infer_scalar_cast_builtin(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            Type::F32,
        ),
        "i32" => infer_scalar_cast_builtin(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            Type::I32,
        ),
        "i64" => infer_scalar_cast_builtin(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            Type::I64,
        ),
        "u32" => infer_scalar_cast_builtin(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            Type::U32,
        ),
        "u64" => infer_scalar_cast_builtin(
            body,
            expr_id,
            args,
            ctx,
            classes,
            enums,
            interfaces,
            functions,
            errors,
            allow_result,
            in_result_fn,
            Type::U64,
        ),
        _ => None,
    }
}

fn infer_call_arg_type(
    body: &Body,
    arg: &crate::hir::Arg,
    ctx: &mut TypeContext,
    classes: &ClassIndex,
    enums: &EnumIndex,
    interfaces: &InterfaceIndex,
    functions: &FunctionIndex,
    errors: &mut Vec<TypeError>,
    allow_result: bool,
    in_result_fn: bool,
) -> Type {
    let value = match arg {
        crate::hir::Arg::Positional { value, .. } | crate::hir::Arg::Named { value, .. } => *value,
    };
    infer_expr(
        body,
        value,
        ctx,
        classes,
        enums,
        interfaces,
        functions,
        errors,
        false,
        allow_result,
        in_result_fn,
    )
}

fn infer_componentwise_unary_builtin(
    body: &Body,
    expr_id: Idx<Expr>,
    args: &Vec<crate::hir::Arg>,
    ctx: &mut TypeContext,
    classes: &ClassIndex,
    enums: &EnumIndex,
    interfaces: &InterfaceIndex,
    functions: &FunctionIndex,
    errors: &mut Vec<TypeError>,
    allow_result: bool,
    in_result_fn: bool,
) -> Option<Type> {
    let span = span_from_range(body.expr_span(expr_id));
    if args.len() != 1 {
        errors.push(TypeError::ArgumentCountMismatch {
            expected: 1,
            found: args.len(),
            span,
        });
        return Some(Type::Unknown);
    }
    let value_ty = infer_call_arg_type(
        body,
        &args[0],
        ctx,
        classes,
        enums,
        interfaces,
        functions,
        errors,
        allow_result,
        in_result_fn,
    );
    if is_scalar_numeric_type(&value_ty) {
        return Some(Type::F32);
    }
    if is_vector_like_type(&value_ty) {
        return Some(value_ty);
    }
    push_math_builtin_arg_mismatch(
        "value",
        &value_ty,
        args,
        0,
        "scalar numeric, Vec2, Vec3, Vec4, or Quat",
        errors,
    );
    Some(Type::Unknown)
}

fn infer_componentwise_binary_builtin(
    body: &Body,
    expr_id: Idx<Expr>,
    args: &Vec<crate::hir::Arg>,
    ctx: &mut TypeContext,
    classes: &ClassIndex,
    enums: &EnumIndex,
    interfaces: &InterfaceIndex,
    functions: &FunctionIndex,
    errors: &mut Vec<TypeError>,
    allow_result: bool,
    in_result_fn: bool,
) -> Option<Type> {
    let span = span_from_range(body.expr_span(expr_id));
    if args.len() != 2 {
        errors.push(TypeError::ArgumentCountMismatch {
            expected: 2,
            found: args.len(),
            span,
        });
        return Some(Type::Unknown);
    }
    let left_ty = infer_call_arg_type(
        body,
        &args[0],
        ctx,
        classes,
        enums,
        interfaces,
        functions,
        errors,
        allow_result,
        in_result_fn,
    );
    let right_ty = infer_call_arg_type(
        body,
        &args[1],
        ctx,
        classes,
        enums,
        interfaces,
        functions,
        errors,
        allow_result,
        in_result_fn,
    );
    match (left_ty, right_ty) {
        (left, right) if is_scalar_numeric_type(&left) && is_scalar_numeric_type(&right) => {
            Some(Type::F32)
        }
        (left, right) if is_vector_like_type(&left) && is_vector_like_type(&right) => {
            if same_vector_like_kind(&left, &right).is_some() {
                Some(left)
            } else {
                push_math_builtin_arg_mismatch(
                    "left",
                    &left,
                    args,
                    0,
                    "matching Vec2, Vec3, Vec4, or Quat operands",
                    errors,
                );
                push_math_builtin_arg_mismatch(
                    "right",
                    &right,
                    args,
                    1,
                    "matching Vec2, Vec3, Vec4, or Quat operands",
                    errors,
                );
                Some(Type::Unknown)
            }
        }
        (left, right) if is_vector_like_type(&left) && is_scalar_numeric_type(&right) => Some(left),
        (left, right) if is_scalar_numeric_type(&left) && is_vector_like_type(&right) => {
            Some(right)
        }
        (left, right) => {
            push_math_builtin_arg_mismatch(
                "left",
                &left,
                args,
                0,
                "scalar numeric, Vec2, Vec3, Vec4, or Quat",
                errors,
            );
            push_math_builtin_arg_mismatch(
                "right",
                &right,
                args,
                1,
                "scalar numeric, Vec2, Vec3, Vec4, or Quat",
                errors,
            );
            Some(Type::Unknown)
        }
    }
}

fn infer_componentwise_ternary_builtin(
    body: &Body,
    expr_id: Idx<Expr>,
    args: &Vec<crate::hir::Arg>,
    ctx: &mut TypeContext,
    classes: &ClassIndex,
    enums: &EnumIndex,
    interfaces: &InterfaceIndex,
    functions: &FunctionIndex,
    errors: &mut Vec<TypeError>,
    allow_result: bool,
    in_result_fn: bool,
) -> Option<Type> {
    let span = span_from_range(body.expr_span(expr_id));
    if args.len() != 3 {
        errors.push(TypeError::ArgumentCountMismatch {
            expected: 3,
            found: args.len(),
            span,
        });
        return Some(Type::Unknown);
    }
    let value_ty = infer_call_arg_type(
        body,
        &args[0],
        ctx,
        classes,
        enums,
        interfaces,
        functions,
        errors,
        allow_result,
        in_result_fn,
    );
    let min_ty = infer_call_arg_type(
        body,
        &args[1],
        ctx,
        classes,
        enums,
        interfaces,
        functions,
        errors,
        allow_result,
        in_result_fn,
    );
    let max_ty = infer_call_arg_type(
        body,
        &args[2],
        ctx,
        classes,
        enums,
        interfaces,
        functions,
        errors,
        allow_result,
        in_result_fn,
    );
    if is_scalar_numeric_type(&value_ty) {
        if is_scalar_numeric_type(&min_ty) && is_scalar_numeric_type(&max_ty) {
            return Some(Type::F32);
        }
        push_math_builtin_arg_mismatch("min", &min_ty, args, 1, "scalar numeric", errors);
        push_math_builtin_arg_mismatch("max", &max_ty, args, 2, "scalar numeric", errors);
        return Some(Type::Unknown);
    }
    if !is_vector_like_type(&value_ty) {
        push_math_builtin_arg_mismatch(
            "value",
            &value_ty,
            args,
            0,
            "scalar numeric, Vec2, Vec3, Vec4, or Quat",
            errors,
        );
        return Some(Type::Unknown);
    }
    if !arg_matches_componentwise_shape(&min_ty, &value_ty) {
        push_math_builtin_arg_mismatch(
            "min",
            &min_ty,
            args,
            1,
            "scalar numeric or matching vector/quaternion",
            errors,
        );
        return Some(Type::Unknown);
    }
    if !arg_matches_componentwise_shape(&max_ty, &value_ty) {
        push_math_builtin_arg_mismatch(
            "max",
            &max_ty,
            args,
            2,
            "scalar numeric or matching vector/quaternion",
            errors,
        );
        return Some(Type::Unknown);
    }
    Some(value_ty)
}

fn infer_scalar_cast_builtin(
    body: &Body,
    expr_id: Idx<Expr>,
    args: &Vec<crate::hir::Arg>,
    ctx: &mut TypeContext,
    classes: &ClassIndex,
    enums: &EnumIndex,
    interfaces: &InterfaceIndex,
    functions: &FunctionIndex,
    errors: &mut Vec<TypeError>,
    allow_result: bool,
    in_result_fn: bool,
    target: Type,
) -> Option<Type> {
    let span = span_from_range(body.expr_span(expr_id));
    if args.len() != 1 {
        errors.push(TypeError::ArgumentCountMismatch {
            expected: 1,
            found: args.len(),
            span,
        });
        return Some(Type::Unknown);
    }
    let value_ty = infer_call_arg_type(
        body,
        &args[0],
        ctx,
        classes,
        enums,
        interfaces,
        functions,
        errors,
        allow_result,
        in_result_fn,
    );
    if matches!(value_ty, Type::Unknown) {
        return Some(Type::Unknown);
    }
    if is_scalar_numeric_type(&value_ty) {
        return Some(target);
    }
    push_math_builtin_arg_mismatch("value", &value_ty, args, 0, "scalar numeric", errors);
    Some(Type::Unknown)
}

fn push_math_builtin_arg_mismatch(
    name: &str,
    found: &Type,
    args: &[crate::hir::Arg],
    index: usize,
    expected: &str,
    errors: &mut Vec<TypeError>,
) {
    let Some(arg) = args.get(index) else {
        return;
    };
    let span = match arg {
        crate::hir::Arg::Positional { span, .. } | crate::hir::Arg::Named { span, .. } => {
            span_from_range(*span)
        }
    };
    errors.push(TypeError::ArgumentTypeMismatch {
        name: SmolStr::new(name),
        expected: expected.to_string(),
        found: type_label(found),
        span,
    });
}

fn is_scalar_numeric_type(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Integer
            | Type::I32
            | Type::U32
            | Type::I64
            | Type::U64
            | Type::Float
            | Type::F32
            | Type::Number
    )
}

fn is_vector_like_type(ty: &Type) -> bool {
    matches!(ty, Type::Vec2 | Type::Vec3 | Type::Vec4 | Type::Quat)
}

fn is_vector_only_type(ty: &Type) -> bool {
    matches!(ty, Type::Vec2 | Type::Vec3 | Type::Vec4)
}

fn is_same_vector_kind(left: &Type, right: &Type) -> bool {
    matches!(
        (left, right),
        (Type::Vec2, Type::Vec2)
            | (Type::Vec3, Type::Vec3)
            | (Type::Vec4, Type::Vec4)
            | (Type::Quat, Type::Quat)
    )
}

fn same_vector_like_kind(left: &Type, right: &Type) -> Option<Type> {
    match (left, right) {
        (Type::Vec2, Type::Vec2) => Some(Type::Vec2),
        (Type::Vec3, Type::Vec3) => Some(Type::Vec3),
        (Type::Vec4, Type::Vec4) => Some(Type::Vec4),
        (Type::Quat, Type::Quat) => Some(Type::Quat),
        _ => None,
    }
}

fn arg_matches_componentwise_shape(arg: &Type, shape: &Type) -> bool {
    matches!(arg, Type::Unknown)
        || is_scalar_numeric_type(arg)
        || (is_vector_like_type(arg) && is_same_vector_kind(arg, shape))
}

fn vector_component_type(object_ty: &Type, member: &SmolStr) -> Option<Type> {
    let member = member.as_str();
    match object_ty {
        Type::Vec2 => match member {
            "x" | "y" => Some(Type::F32),
            _ => None,
        },
        Type::Vec3 => match member {
            "x" | "y" | "z" => Some(Type::F32),
            _ => None,
        },
        Type::Vec4 | Type::Quat => match member {
            "x" | "y" | "z" | "w" => Some(Type::F32),
            _ => None,
        },
        _ => None,
    }
}
