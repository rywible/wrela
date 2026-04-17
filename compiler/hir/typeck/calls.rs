use super::{
    Arg, Body, ClassIndex, ClassSig, EnumIndex, Expr, FunctionIndex, Idx, InterfaceIndex, Literal,
    SmolStr, SourceSpan, Type, TypeContext, TypeError, TypeRef, infer_expr, is_assignable,
    span_from_range, type_label, types_known,
};
use std::collections::{HashMap, HashSet};

pub(super) fn check_class_init_args(
    body: &Body,
    expr_id: Idx<Expr>,
    args: &Vec<crate::hir::Arg>,
    class: &ClassSig,
    class_args: &[Type],
    ctx: &mut TypeContext,
    classes: &ClassIndex,
    enums: &EnumIndex,
    interfaces: &InterfaceIndex,
    functions: &FunctionIndex,
    errors: &mut Vec<TypeError>,
    allow_result: bool,
    in_result_fn: bool,
) {
    let subst = build_type_subst(&class.type_params, class_args);
    let mut positional_index = 0usize;
    let total_fields = class.field_order.len();
    if requires_named_args(total_fields, args) {
        let arg_spans =
            args.iter()
                .map(|arg| match arg {
                    crate::hir::Arg::Positional { span, .. }
                    | crate::hir::Arg::Named { span, .. } => span_from_range(*span),
                })
                .collect::<Vec<_>>();
        errors.push(TypeError::NamedArgsRequired {
            span: span_from_range(body.expr_span(expr_id)),
            param_names: class.field_order.clone(),
            arg_spans,
        });
    }
    for arg in args {
        match arg {
            crate::hir::Arg::Positional { value, span } => {
                if positional_index >= total_fields {
                    errors.push(TypeError::ArgumentCountMismatch {
                        expected: total_fields,
                        found: args.len(),
                        span: span_from_range(*span),
                    });
                    let _ = infer_expr(
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
                    continue;
                }
                let field_name = &class.field_order[positional_index];
                let expected = class
                    .fields
                    .get(field_name)
                    .cloned()
                    .unwrap_or(Type::Unknown);
                let expected = substitute_type(&expected, &subst);
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
                if types_known(&expected, &found)
                    && !is_assignable(&expected, &found, classes, interfaces)
                {
                    errors.push(TypeError::ArgumentTypeMismatch {
                        name: field_name.clone(),
                        expected: type_label(&expected),
                        found: type_label(&found),
                        span: span_from_range(*span),
                    });
                }
                positional_index += 1;
            }
            crate::hir::Arg::Named {
                name,
                value,
                name_span,
                ..
            } => {
                let Some(expected) = class.fields.get(name).cloned() else {
                    errors.push(TypeError::UnknownArgument {
                        name: name.clone(),
                        span: span_from_range(*name_span),
                    });
                    let _ = infer_expr(
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
                    continue;
                };
                let expected = substitute_type(&expected, &subst);
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
                if types_known(&expected, &found)
                    && !is_assignable(&expected, &found, classes, interfaces)
                {
                    errors.push(TypeError::ArgumentTypeMismatch {
                        name: name.clone(),
                        expected: type_label(&expected),
                        found: type_label(&found),
                        span: span_from_range(*name_span),
                    });
                }
            }
        }
    }
    if args.len() > total_fields {
        errors.push(TypeError::ArgumentCountMismatch {
            expected: total_fields,
            found: args.len(),
            span: span_from_range(body.expr_span(expr_id)),
        });
    }
}

pub(super) fn requires_named_args(param_count: usize, args: &[crate::hir::Arg]) -> bool {
    param_count > 1
        && args
            .iter()
            .any(|arg| matches!(arg, crate::hir::Arg::Positional { .. }))
}

pub(super) fn infer_list(
    body: &Body,
    items: &Vec<Idx<Expr>>,
    ctx: &mut TypeContext,
    classes: &ClassIndex,
    enums: &EnumIndex,
    interfaces: &InterfaceIndex,
    functions: &FunctionIndex,
    errors: &mut Vec<TypeError>,
    allow_result: bool,
    in_result_fn: bool,
) -> Type {
    let mut element_type: Option<Type> = None;
    for item in items {
        let ty = infer_expr(
            body,
            *item,
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
        if ty == Type::Unknown {
            continue;
        }
        match &element_type {
            None => element_type = Some(ty),
            Some(existing) if *existing == ty => {}
            Some(_) => return Type::Unknown,
        }
    }
    let Some(element_type) = element_type else {
        return if ctx.in_portable_lane() {
            Type::Array(Box::new(Type::Unknown), 0)
        } else {
            Type::Unknown
        };
    };
    if ctx.in_portable_lane() {
        Type::Array(Box::new(element_type), items.len())
    } else {
        Type::List(Box::new(element_type))
    }
}

pub(super) fn infer_map(
    body: &Body,
    items: &Vec<(Idx<Expr>, Idx<Expr>)>,
    ctx: &mut TypeContext,
    classes: &ClassIndex,
    enums: &EnumIndex,
    interfaces: &InterfaceIndex,
    functions: &FunctionIndex,
    errors: &mut Vec<TypeError>,
    allow_result: bool,
    in_result_fn: bool,
) -> Type {
    let mut key_type: Option<Type> = None;
    let mut value_type: Option<Type> = None;
    for (key, value) in items {
        let key_ty = infer_expr(
            body,
            *key,
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
        let value_ty = infer_expr(
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
        if key_ty != Type::Unknown {
            match &key_type {
                None => key_type = Some(key_ty),
                Some(existing) if *existing == key_ty => {}
                Some(_) => return Type::Unknown,
            }
        }
        if value_ty != Type::Unknown {
            match &value_type {
                None => value_type = Some(value_ty),
                Some(existing) if *existing == value_ty => {}
                Some(_) => return Type::Unknown,
            }
        }
    }
    match (key_type, value_type) {
        (Some(k), Some(v)) => Type::Map(Box::new(k), Box::new(v)),
        _ => Type::Unknown,
    }
}

pub(super) fn literal_type(lit: &Literal) -> Type {
    match lit {
        Literal::Integer(_) => Type::Integer,
        Literal::Float(_) => Type::Float,
        Literal::Boolean(_) => Type::Boolean,
        Literal::String(_) => Type::String,
        Literal::Nil => Type::Nil,
    }
}

pub(super) fn error_type() -> Type {
    Type::Named(SmolStr::new("Error"), Vec::new())
}

pub(super) fn type_from_ref(ty: &TypeRef) -> Type {
    type_from_ref_with_params(ty, &HashSet::new())
}

pub(super) fn type_from_ref_in_ctx(ty: &TypeRef, ctx: &TypeContext) -> Type {
    if ctx.type_params.is_empty() {
        return type_from_ref(ty);
    }
    let mut params = HashSet::new();
    for scope in &ctx.type_params {
        params.extend(scope.iter().cloned());
    }
    type_from_ref_with_params(ty, &params)
}

pub(super) fn type_from_ref_with_params(ty: &TypeRef, params: &HashSet<SmolStr>) -> Type {
    let args: Vec<Type> = ty
        .args
        .iter()
        .map(|arg| type_from_ref_with_params(arg, params))
        .collect();
    if params.contains(&ty.name) && args.is_empty() {
        return Type::Param(ty.name.clone());
    }
    match ty.name.as_str() {
        "Integer" => Type::Integer,
        "I32" => Type::I32,
        "U32" => Type::U32,
        "I64" => Type::I64,
        "U64" => Type::U64,
        "Any" => Type::Unknown,
        "Float" => Type::Float,
        "F32" => Type::F32,
        "Vec2" => Type::Vec2,
        "Vec3" => Type::Vec3,
        "Vec4" => Type::Vec4,
        "Mat3" => Type::Mat3,
        "Mat4" => Type::Mat4,
        "Quat" => Type::Quat,
        "GpuBuffer" => match args.as_slice() {
            [inner] => Type::GpuBuffer(Box::new(inner.clone())),
            _ => Type::GpuBuffer(Box::new(Type::Unknown)),
        },
        "GpuAtomicI32" => Type::GpuAtomicI32,
        "GpuAtomicU32" => Type::GpuAtomicU32,
        "GpuDispatchSchedule" => Type::GpuDispatchSchedule,
        "Number" => Type::Number,
        "Boolean" => Type::Boolean,
        "Bool" => Type::Boolean,
        "String" => Type::String,
        "Nothing" => Type::Nil,
        "Array" => match ty.args.as_slice() {
            [inner, len] => {
                let inner = type_from_ref_with_params(inner, params);
                let len = len.name.as_str().parse::<usize>().ok().unwrap_or(0);
                Type::Array(Box::new(inner), len)
            }
            _ => Type::Array(Box::new(Type::Unknown), 0),
        },
        "List" => match args.as_slice() {
            [inner] => Type::List(Box::new(inner.clone())),
            _ => Type::List(Box::new(Type::Unknown)),
        },
        "Map" => match args.as_slice() {
            [key, value] => Type::Map(Box::new(key.clone()), Box::new(value.clone())),
            _ => Type::Map(Box::new(Type::Unknown), Box::new(Type::Unknown)),
        },
        "Actor" => match args.as_slice() {
            [inner] => Type::Actor(Box::new(inner.clone())),
            _ => Type::Actor(Box::new(Type::Unknown)),
        },
        "Pending" => match args.as_slice() {
            [inner] => Type::Pending(Box::new(inner.clone())),
            _ => Type::Pending(Box::new(Type::Unknown)),
        },
        "Result" => match args.as_slice() {
            [ok, err] => Type::Result(Box::new(ok.clone()), Box::new(err.clone())),
            [ok] => Type::Result(Box::new(ok.clone()), Box::new(error_type())),
            _ => Type::Result(Box::new(Type::Unknown), Box::new(error_type())),
        },
        _ => Type::Named(ty.name.clone(), args),
    }
}

pub(super) fn build_type_subst(params: &[SmolStr], args: &[Type]) -> HashMap<SmolStr, Type> {
    let mut subst = HashMap::new();
    for (idx, name) in params.iter().enumerate() {
        if let Some(arg) = args.get(idx) {
            subst.insert(name.clone(), arg.clone());
        }
    }
    subst
}

pub(super) fn substitute_type(ty: &Type, subst: &HashMap<SmolStr, Type>) -> Type {
    match ty {
        Type::Param(name) => subst.get(name).cloned().unwrap_or(Type::Unknown),
        Type::List(inner) => Type::List(Box::new(substitute_type(inner, subst))),
        Type::Map(key, value) => Type::Map(
            Box::new(substitute_type(key, subst)),
            Box::new(substitute_type(value, subst)),
        ),
        Type::Result(ok, err) => Type::Result(
            Box::new(substitute_type(ok, subst)),
            Box::new(substitute_type(err, subst)),
        ),
        Type::Actor(inner) => Type::Actor(Box::new(substitute_type(inner, subst))),
        Type::Pending(inner) => Type::Pending(Box::new(substitute_type(inner, subst))),
        Type::Named(name, args) => Type::Named(
            name.clone(),
            args.iter().map(|arg| substitute_type(arg, subst)).collect(),
        ),
        other => other.clone(),
    }
}

pub(super) fn resolve_type_args(
    name: &SmolStr,
    params: &[SmolStr],
    type_args: &[TypeRef],
    ctx: &TypeContext,
    errors: &mut Vec<TypeError>,
    span: SourceSpan,
) -> Vec<Type> {
    if params.is_empty() {
        if !type_args.is_empty() {
            errors.push(TypeError::UnexpectedTypeArgs { span });
        }
        return Vec::new();
    }
    if type_args.is_empty() {
        errors.push(TypeError::MissingTypeArgs {
            name: name.clone(),
            span,
        });
        return vec![Type::Unknown; params.len()];
    }
    if type_args.len() != params.len() {
        errors.push(TypeError::TypeArgCountMismatch {
            name: name.clone(),
            expected: params.len(),
            found: type_args.len(),
            span,
        });
    }
    let mut resolved: Vec<Type> = type_args
        .iter()
        .map(|arg| type_from_ref_in_ctx(arg, ctx))
        .collect();
    if resolved.len() < params.len() {
        resolved.extend(std::iter::repeat_n(
            Type::Unknown,
            params.len() - resolved.len(),
        ));
    }
    if resolved.len() > params.len() {
        resolved.truncate(params.len());
    }
    resolved
}
