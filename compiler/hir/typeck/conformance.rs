fn interface_method_matches(iface: &InterfaceMethodSig, class: &MethodSig) -> bool {
    if (iface.kind == InterfaceMethodKind::Check) != (class.kind == FunctionKind::CheckMethod) {
        return false;
    }
    if iface.params.len() != class.params.len() {
        return false;
    }
    for ((_, iface_ty), (_, class_ty)) in iface.params.iter().zip(class.params.iter()) {
        if !interface_type_compatible(iface_ty, class_ty) {
            return false;
        }
    }
    interface_type_compatible(&iface.ret, &class.ret)
}

fn interface_type_compatible(expected: &Type, actual: &Type) -> bool {
    if matches!(expected, Type::Param(_)) || matches!(actual, Type::Param(_)) {
        return true;
    }
    match (expected, actual) {
        (Type::List(a), Type::List(b)) => interface_type_compatible(a, b),
        (Type::Map(ak, av), Type::Map(bk, bv)) => {
            interface_type_compatible(ak, bk) && interface_type_compatible(av, bv)
        }
        (Type::Result(aok, aerr), Type::Result(bok, berr)) => {
            interface_type_compatible(aok, bok) && interface_type_compatible(aerr, berr)
        }
        (Type::Actor(a), Type::Actor(b)) => interface_type_compatible(a, b),
        (Type::Pending(a), Type::Pending(b)) => interface_type_compatible(a, b),
        (Type::Named(aname, aargs), Type::Named(bname, bargs)) => {
            if aname != bname || aargs.len() != bargs.len() {
                return false;
            }
            aargs
                .iter()
                .zip(bargs.iter())
                .all(|(a, b)| interface_type_compatible(a, b))
        }
        _ => expected == actual,
    }
}

fn class_subst(class: &ClassSig, class_args: &[Type]) -> HashMap<SmolStr, Type> {
    build_type_subst(&class.type_params, class_args)
}

fn instantiate_method_params(
    class: &ClassSig,
    class_args: &[Type],
    member: &SmolStr,
) -> Option<Vec<(SmolStr, Type)>> {
    let method = class.methods.get(member)?;
    if class.type_params.is_empty() {
        return Some(method.params.clone());
    }
    let subst = class_subst(class, class_args);
    Some(
        method
            .params
            .iter()
            .map(|(name, ty)| (name.clone(), substitute_type(ty, &subst)))
            .collect(),
    )
}

fn instantiate_method_ret(class: &ClassSig, class_args: &[Type], member: &SmolStr) -> Option<Type> {
    let method = class.methods.get(member)?;
    if class.type_params.is_empty() {
        return Some(method.ret.clone());
    }
    let subst = class_subst(class, class_args);
    Some(substitute_type(&method.ret, &subst))
}

fn valid_unary(op: UnaryOp, operand: &Type) -> bool {
    match op {
        UnaryOp::Neg => is_numeric(operand),
        UnaryOp::Not => *operand == Type::Boolean,
        UnaryOp::BitNot => *operand == Type::Integer,
        UnaryOp::Err => !matches!(operand, Type::Never),
        UnaryOp::Try => is_result_type(operand),
        UnaryOp::Await | UnaryOp::Spawn | UnaryOp::Fire => true,
    }
}

fn unary_result(op: UnaryOp, operand: &Type) -> Type {
    match op {
        UnaryOp::Neg => operand.clone(),
        UnaryOp::Not => Type::Boolean,
        UnaryOp::BitNot => Type::Integer,
        UnaryOp::Err => Type::Result(Box::new(Type::Unknown), Box::new(operand.clone())),
        UnaryOp::Try => match operand {
            Type::Result(ok, _) => *ok.clone(),
            _ => Type::Unknown,
        },
        UnaryOp::Await | UnaryOp::Spawn | UnaryOp::Fire => Type::Unknown,
    }
}

fn binary_from_assign(op: BinaryOp) -> BinaryOp {
    match op {
        BinaryOp::AddAssign => BinaryOp::Add,
        BinaryOp::SubAssign => BinaryOp::Sub,
        BinaryOp::MulAssign => BinaryOp::Mul,
        BinaryOp::DivAssign => BinaryOp::Div,
        other => other,
    }
}

fn valid_binary(op: BinaryOp, left: &Type, right: &Type) -> bool {
    match op {
        BinaryOp::Add => {
            (is_numeric(left) && is_numeric(right))
                || (*left == Type::String && *right == Type::String)
        }
        BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod => {
            is_numeric(left) && is_numeric(right)
        }
        BinaryOp::Eq | BinaryOp::Ne => true,
        BinaryOp::Lt | BinaryOp::Gt | BinaryOp::Le | BinaryOp::Ge => {
            is_numeric(left) && is_numeric(right)
        }
        BinaryOp::And | BinaryOp::Or => *left == Type::Boolean && *right == Type::Boolean,
        BinaryOp::Otherwise => true,
        BinaryOp::BitAnd | BinaryOp::BitOr | BinaryOp::BitXor | BinaryOp::Shl | BinaryOp::Shr => {
            *left == Type::Integer && *right == Type::Integer
        }
        BinaryOp::Range => is_numeric(left) && is_numeric(right),
        BinaryOp::Assign
        | BinaryOp::AddAssign
        | BinaryOp::SubAssign
        | BinaryOp::MulAssign
        | BinaryOp::DivAssign => true,
    }
}

fn valid_equality_operands(
    left: &Type,
    right: &Type,
    classes: &ClassIndex,
    enums: &EnumIndex,
    interfaces: &InterfaceIndex,
) -> bool {
    if !is_assignable(left, right, classes, interfaces)
        && !is_assignable(right, left, classes, interfaces)
    {
        return false;
    }
    let mut left_visiting: HashSet<SmolStr> = HashSet::new();
    let mut right_visiting: HashSet<SmolStr> = HashSet::new();
    supports_structural_value_type(left, classes, enums, &mut left_visiting)
        && supports_structural_value_type(right, classes, enums, &mut right_visiting)
}

fn binary_result(op: BinaryOp, left: &Type, right: &Type) -> Type {
    match op {
        BinaryOp::Add => {
            if *left == Type::String && *right == Type::String {
                Type::String
            } else {
                numeric_result(left, right)
            }
        }
        BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod => {
            numeric_result(left, right)
        }
        BinaryOp::Eq | BinaryOp::Ne => Type::Boolean,
        BinaryOp::Lt | BinaryOp::Gt | BinaryOp::Le | BinaryOp::Ge => Type::Boolean,
        BinaryOp::And | BinaryOp::Or => Type::Boolean,
        BinaryOp::Otherwise => Type::Unknown,
        BinaryOp::BitAnd | BinaryOp::BitOr | BinaryOp::BitXor | BinaryOp::Shl | BinaryOp::Shr => {
            Type::Integer
        }
        BinaryOp::Range => Type::Unknown,
        BinaryOp::Assign
        | BinaryOp::AddAssign
        | BinaryOp::SubAssign
        | BinaryOp::MulAssign
        | BinaryOp::DivAssign => Type::Unknown,
    }
}

fn numeric_result(left: &Type, right: &Type) -> Type {
    if *left == Type::Float || *right == Type::Float {
        Type::Float
    } else if *left == Type::Number || *right == Type::Number {
        Type::Number
    } else if *left == Type::Integer && *right == Type::Integer {
        Type::Integer
    } else {
        Type::Unknown
    }
}

fn is_numeric(ty: &Type) -> bool {
    matches!(ty, Type::Integer | Type::Float | Type::Number)
}

fn is_assignable(
    expected: &Type,
    found: &Type,
    classes: &ClassIndex,
    interfaces: &InterfaceIndex,
) -> bool {
    if expected == found {
        return true;
    }
    if is_stored_boolean_named(expected) && *found == Type::Boolean {
        return true;
    }
    match (expected, found) {
        (_, Type::Never) => true,
        (Type::Param(_), _) => true,
        (_, Type::Param(_)) => true,
        (Type::Result(ok_e, err_e), Type::Result(ok_f, err_f)) => {
            is_assignable(ok_e, ok_f, classes, interfaces)
                && is_assignable(err_e, err_f, classes, interfaces)
        }
        (Type::Result(ok_e, _), other) => is_assignable(ok_e, other, classes, interfaces),
        (Type::Pending(exp), Type::Pending(found)) => {
            matches!(**exp, Type::Unknown) || is_assignable(exp, found, classes, interfaces)
        }
        (Type::Named(exp_name, exp_args), Type::Named(found_name, found_args))
            if interfaces.is_interface(exp_name) =>
        {
            let Some(class) = classes.get(found_name) else {
                return false;
            };
            if !class.implements.iter().any(|name| name == exp_name) {
                return false;
            }
            if !exp_args.is_empty() {
                // No interface type args supported yet; treat as unknown.
                return true;
            }
            true
        }
        (Type::Named(exp_name, exp_args), Type::Named(found_name, found_args)) => {
            if exp_name != found_name || exp_args.len() != found_args.len() {
                return false;
            }
            exp_args
                .iter()
                .zip(found_args.iter())
                .all(|(exp, found)| is_assignable(exp, found, classes, interfaces))
        }
        (Type::Number, ty) if is_numeric(ty) => true,
        (Type::Float, Type::Integer) => true,
        _ => false,
    }
}

fn is_stored_boolean_named(ty: &Type) -> bool {
    matches!(ty, Type::Named(name, args) if name.as_str() == "StoredBoolean" && args.is_empty())
}

fn types_known(left: &Type, right: &Type) -> bool {
    is_known(left) && is_known(right)
}

fn is_identity_primitive(ty: &Type) -> bool {
    match ty {
        Type::Integer | Type::Float | Type::Number | Type::Boolean | Type::String | Type::Nil => {
            true
        }
        Type::Named(name, _) => name.as_str() == "Bytes",
        _ => false,
    }
}

fn is_known(ty: &Type) -> bool {
    match ty {
        Type::Unknown => false,
        Type::Result(ok, err) => is_known(ok) && is_known(err),
        _ => true,
    }
}

fn is_result_type(ty: &Type) -> bool {
    matches!(ty, Type::Result(_, _))
}

fn type_label(ty: &Type) -> String {
    match ty {
        Type::Unknown => "unknown".to_string(),
        Type::Never => "never".to_string(),
        Type::Integer => "Integer".to_string(),
        Type::Float => "Float".to_string(),
        Type::Number => "Number".to_string(),
        Type::Boolean => "Boolean".to_string(),
        Type::String => "String".to_string(),
        Type::Nil => "Nothing".to_string(),
        Type::List(inner) => format!("List[{}]", type_label(inner)),
        Type::Map(key, value) => format!("Map[{}, {}]", type_label(key), type_label(value)),
        Type::Named(name, args) => {
            if args.is_empty() {
                name.to_string()
            } else {
                format!(
                    "{}[{}]",
                    name,
                    args.iter().map(type_label).collect::<Vec<_>>().join(", ")
                )
            }
        }
        Type::Param(name) => name.to_string(),
        Type::Result(ok, err) => format!("Result[{}, {}]", type_label(ok), type_label(err)),
        Type::Actor(inner) => format!("Actor[{}]", type_label(inner)),
        Type::Pending(inner) => format!("Pending[{}]", type_label(inner)),
        Type::Vec2 => "Vec2".to_string(),
        Type::Vec3 => "Vec3".to_string(),
        Type::Vec4 => "Vec4".to_string(),
        Type::Mat4 => "Mat4".to_string(),
        Type::GpuBuffer(inner) => format!("Buffer[{}]", type_label(inner)),
        Type::Texture2D => "Texture2D".to_string(),
        Type::Sampler => "Sampler".to_string(),
    }
}

fn collection_method_sig(
    object_ty: &Type,
    member: &SmolStr,
) -> Option<(Vec<(SmolStr, Type)>, Type)> {
    match object_ty {
        Type::List(inner_ty) => match member.as_str() {
            "push" => Some((
                vec![(SmolStr::new("value"), (*inner_ty.clone()))],
                Type::Nil,
            )),
            "len" => Some((vec![], Type::Integer)),
            _ => None,
        },
        Type::Map(key_ty, value_ty) => match member.as_str() {
            "set" => Some((
                vec![
                    (SmolStr::new("key"), (*key_ty.clone())),
                    (SmolStr::new("value"), (*value_ty.clone())),
                ],
                Type::Nil,
            )),
            "get" => Some((
                vec![(SmolStr::new("key"), (*key_ty.clone()))],
                (*value_ty.clone()),
            )),
            "len" => Some((vec![], Type::Integer)),
            _ => None,
        },
        _ => None,
    }
}

fn unary_op_label(op: UnaryOp) -> &'static str {
    match op {
        UnaryOp::Neg => "-",
        UnaryOp::Not => "not",
        UnaryOp::BitNot => "~",
        UnaryOp::Await => "await",
        UnaryOp::Spawn => "spawn",
        UnaryOp::Fire => "fire",
        UnaryOp::Err => "error",
        UnaryOp::Try => "?",
    }
}

fn binary_op_label(op: BinaryOp) -> &'static str {
    match op {
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div => "/",
        BinaryOp::Mod => "%",
        BinaryOp::Eq => "==",
        BinaryOp::Ne => "!=",
        BinaryOp::Lt => "<",
        BinaryOp::Gt => ">",
        BinaryOp::Le => "<=",
        BinaryOp::Ge => ">=",
        BinaryOp::And => "and",
        BinaryOp::Or => "or",
        BinaryOp::Otherwise => "??",
        BinaryOp::BitAnd => "&",
        BinaryOp::BitOr => "|",
        BinaryOp::BitXor => "^",
        BinaryOp::Shl => "<<",
        BinaryOp::Shr => ">>",
        BinaryOp::Range => "...",
        BinaryOp::Assign => "=",
        BinaryOp::AddAssign => "+=",
        BinaryOp::SubAssign => "-=",
        BinaryOp::MulAssign => "*=",
        BinaryOp::DivAssign => "/=",
    }
}

fn span_from_range(range: rowan::TextRange) -> SourceSpan {
    let start: usize = range.start().into();
    let len: usize = range.len().into();
    SourceSpan::from((start, len))
}

fn span_from_option_range(range: Option<rowan::TextRange>) -> SourceSpan {
    range
        .map(span_from_range)
        .unwrap_or_else(|| SourceSpan::from((0usize, 0usize)))
}

fn actor_type_for_detach_target(body: &Body, target: Idx<Expr>, classes: &ClassIndex) -> Type {
    match &body.exprs[target] {
        Expr::Variable(name) => {
            if classes.is_class(name) {
                Type::Actor(Box::new(Type::Named(name.clone(), Vec::new())))
            } else {
                Type::Unknown
            }
        }
        Expr::Call { callee, args, .. } => {
            if is_pool_of_call(body, *callee)
                && let Some(class_name) = pool_of_class_name(body, args, classes)
            {
                return Type::Actor(Box::new(Type::Named(class_name, Vec::new())));
            }
            if let Expr::Variable(name) = &body.exprs[*callee] {
                if classes.is_class(name) {
                    Type::Actor(Box::new(Type::Named(name.clone(), Vec::new())))
                } else {
                    Type::Unknown
                }
            } else {
                Type::Unknown
            }
        }
        _ => Type::Unknown,
    }
}

fn callee_error_span(body: &Body, callee: Idx<Expr>) -> SourceSpan {
    match &body.exprs[callee] {
        Expr::Binary { op_span, .. } => span_from_range(*op_span),
        Expr::Unary { op_span, .. } => span_from_range(*op_span),
        Expr::Member { member_span, .. } => span_from_range(*member_span),
        _ => span_from_range(body.expr_span(callee)),
    }
}

fn check_type_param_bounds(
    type_param_names: &[SmolStr],
    type_param_bounds: &[Vec<SmolStr>],
    type_args: &[Type],
    classes: &ClassIndex,
    span: SourceSpan,
    errors: &mut Vec<TypeError>,
) {
    for (i, param_name) in type_param_names.iter().enumerate() {
        let Some(bounds) = type_param_bounds.get(i) else {
            continue;
        };
        let Some(arg) = type_args.get(i) else {
            continue;
        };
        for bound in bounds {
            if !type_satisfies_bound(arg, bound, classes) {
                errors.push(TypeError::TypeParamBoundNotSatisfied {
                    param: param_name.clone(),
                    bound: bound.clone(),
                    found: type_label(arg),
                    span,
                });
            }
        }
    }
}

fn type_satisfies_bound(ty: &Type, bound: &str, classes: &ClassIndex) -> bool {
    match ty {
        Type::Named(name, _) => {
            if let Some(class) = classes.get(name) {
                class.implements.iter().any(|iface| iface.as_str() == bound)
            } else {
                false
            }
        }
        Type::Unknown => true,
        Type::Param(_) => true,
        _ => false,
    }
}

