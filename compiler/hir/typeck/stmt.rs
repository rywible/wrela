use super::{
    BinaryOp, Body, ClassIndex, EnumIndex, Expr, FunctionIndex, Idx, InterfaceIndex, Pattern,
    SmolStr, SourceSpan, Stmt, TextRange, Type, TypeContext, TypeError, infer_expr, is_assignable,
    is_identity_primitive, is_numeric, is_result_type, span_from_range, type_label, types_known,
    valid_binary,
};
use std::collections::{HashMap, HashSet};

pub(super) fn check_stmt(
    body: &Body,
    stmt_id: Idx<Stmt>,
    ctx: &mut TypeContext,
    classes: &ClassIndex,
    enums: &EnumIndex,
    interfaces: &InterfaceIndex,
    functions: &FunctionIndex,
    errors: &mut Vec<TypeError>,
    ret_type: Option<&Type>,
    returns_result: bool,
    func_span: Option<rowan::TextRange>,
) {
    match &body.stmts[stmt_id] {
        Stmt::Expr(expr) => {
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
                returns_result,
                returns_result,
            );
        }
        Stmt::Assert {
            kind,
            expr,
            rhs,
            tolerance,
        } => {
            let _expr_ty = infer_expr(
                body,
                *expr,
                ctx,
                classes,
                enums,
                interfaces,
                functions,
                errors,
                false,
                returns_result,
                returns_result,
            );
            let span = span_from_range(body.stmt_span(stmt_id));
            match kind {
                crate::hir::AssertKind::Value => {
                    check_assert_expr(
                        body,
                        *expr,
                        AssertEqualityMode::Value,
                        ctx,
                        classes,
                        enums,
                        interfaces,
                        functions,
                        errors,
                        returns_result,
                        span,
                    );
                }
                crate::hir::AssertKind::Identity => {
                    check_assert_expr(
                        body,
                        *expr,
                        AssertEqualityMode::Identity,
                        ctx,
                        classes,
                        enums,
                        interfaces,
                        functions,
                        errors,
                        returns_result,
                        span,
                    );
                }
                crate::hir::AssertKind::Approx => {
                    let Some(rhs) = *rhs else {
                        errors.push(TypeError::AssertExpectedEquality {
                            mode: "approx",
                            span,
                        });
                        return;
                    };
                    let Some(tolerance) = *tolerance else {
                        errors.push(TypeError::AssertApproxRequiresNumeric { span });
                        return;
                    };
                    check_assert_approx(
                        body,
                        *expr,
                        rhs,
                        tolerance,
                        ctx,
                        classes,
                        enums,
                        interfaces,
                        functions,
                        errors,
                        returns_result,
                        span,
                    );
                }
            }
        }
        Stmt::Require { condition, message } => {
            let cond_ty = infer_expr(
                body,
                *condition,
                ctx,
                classes,
                enums,
                interfaces,
                functions,
                errors,
                false,
                returns_result,
                returns_result,
            );
            if types_known(&Type::Boolean, &cond_ty) && !matches!(cond_ty, Type::Boolean) {
                errors.push(TypeError::RequireConditionNotBoolean {
                    span: span_from_range(body.expr_span(*condition)),
                });
            }
            let msg_ty = infer_expr(
                body,
                *message,
                ctx,
                classes,
                enums,
                interfaces,
                functions,
                errors,
                false,
                returns_result,
                returns_result,
            );
            if types_known(&Type::String, &msg_ty) && !matches!(msg_ty, Type::String) {
                errors.push(TypeError::RequireMessageNotString {
                    span: span_from_range(body.expr_span(*message)),
                });
            }
        }
        Stmt::Let { name, value, .. } => {
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
                true,
                returns_result,
            );
            ctx.declare(name.clone(), value_ty);
        }
        Stmt::IgnoreResult { expr } => {
            let value_ty = infer_expr(
                body,
                *expr,
                ctx,
                classes,
                enums,
                interfaces,
                functions,
                errors,
                false,
                true,
                returns_result,
            );
            if !is_result_type(&value_ty) {
                errors.push(TypeError::IgnoreResultRequiresResult {
                    span: span_from_range(body.stmt_span(stmt_id)),
                });
            }
        }
        Stmt::Capture { name, value } => {
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
                true,
                returns_result,
            );
            if !is_result_type(&value_ty) {
                errors.push(TypeError::CaptureRequiresResult {
                    span: span_from_range(body.stmt_span(stmt_id)),
                });
            }
            ctx.declare(name.clone(), value_ty);
        }
        Stmt::Assign { name, value, .. } => {
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
                true,
                returns_result,
            );
            let span = body.stmt_span(stmt_id);
            if let Some(existing) = ctx.resolve(name)
                && types_known(&existing, &value_ty)
                && !is_assignable(&existing, &value_ty, classes, interfaces)
            {
                errors.push(TypeError::InvalidAssignment {
                    name: name.clone(),
                    expected: type_label(&existing),
                    found: type_label(&value_ty),
                    span: span_from_range(span),
                });
            }
            ctx.assign(name, value_ty);
        }
        Stmt::Optimize { body: opt_body, .. } => {
            ctx.enter_scope();
            for stmt in opt_body {
                check_stmt(
                    body,
                    *stmt,
                    ctx,
                    classes,
                    enums,
                    interfaces,
                    functions,
                    errors,
                    ret_type,
                    returns_result,
                    func_span,
                );
            }
            ctx.exit_scope();
        }
        Stmt::If {
            condition,
            then_branch,
            else_branch,
        } => {
            let cond_ty = infer_expr(
                body,
                *condition,
                ctx,
                classes,
                enums,
                interfaces,
                functions,
                errors,
                false,
                returns_result,
                returns_result,
            );
            if types_known(&Type::Boolean, &cond_ty) && !matches!(cond_ty, Type::Boolean) {
                errors.push(TypeError::IfConditionNotBoolean {
                    span: span_from_range(body.expr_span(*condition)),
                });
            }
            ctx.enter_scope();
            for stmt in then_branch {
                check_stmt(
                    body,
                    *stmt,
                    ctx,
                    classes,
                    enums,
                    interfaces,
                    functions,
                    errors,
                    ret_type,
                    returns_result,
                    func_span,
                );
            }
            ctx.exit_scope();
            if let Some(branch) = else_branch {
                ctx.enter_scope();
                for stmt in branch {
                    check_stmt(
                        body,
                        *stmt,
                        ctx,
                        classes,
                        enums,
                        interfaces,
                        functions,
                        errors,
                        ret_type,
                        returns_result,
                        func_span,
                    );
                }
                ctx.exit_scope();
            }
        }
        Stmt::For {
            value_name,
            key_name,
            index_name,
            iterable,
            body: loop_body,
        } => {
            let iterable_ty = infer_expr(
                body,
                *iterable,
                ctx,
                classes,
                enums,
                interfaces,
                functions,
                errors,
                false,
                returns_result,
                returns_result,
            );
            ctx.enter_scope();
            if key_name.is_some() && index_name.is_some() {
                errors.push(TypeError::ForMapWithIndexUnsupported {
                    span: span_from_range(body.stmt_span(stmt_id)),
                });
            }
            if let Some(key_name) = key_name {
                match iterable_ty {
                    Type::Map(key_ty, value_ty) => {
                        ctx.assign(key_name, (*key_ty).clone());
                        ctx.assign(value_name, (*value_ty).clone());
                    }
                    Type::Unknown => {}
                    _ => errors.push(TypeError::ForMapRequiresMap {
                        span: span_from_range(body.stmt_span(stmt_id)),
                    }),
                }
            } else {
                let mut value_binding_ty = Type::Unknown;
                let mut index_supported = false;
                match &iterable_ty {
                    Type::List(inner) => {
                        value_binding_ty = (**inner).clone();
                        index_supported = true;
                    }
                    Type::Unknown => {}
                    _ => {
                        if let Expr::Binary {
                            op: BinaryOp::Range,
                            lhs,
                            rhs,
                            ..
                        } = &body.exprs[*iterable]
                        {
                            let left_ty = infer_expr(
                                body,
                                *lhs,
                                ctx,
                                classes,
                                enums,
                                interfaces,
                                functions,
                                errors,
                                false,
                                returns_result,
                                returns_result,
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
                                returns_result,
                                returns_result,
                            );
                            if types_known(&left_ty, &right_ty)
                                && valid_binary(BinaryOp::Range, &left_ty, &right_ty)
                            {
                                value_binding_ty =
                                    if left_ty == Type::Float || right_ty == Type::Float {
                                        Type::Float
                                    } else {
                                        Type::Integer
                                    };
                                index_supported = true;
                            }
                        }
                    }
                }
                ctx.assign(value_name, value_binding_ty);
                if let Some(index_name) = index_name {
                    if index_supported || matches!(iterable_ty, Type::Unknown) {
                        ctx.assign(index_name, Type::Integer);
                    } else {
                        errors.push(TypeError::ForWithIndexRequiresListOrRange {
                            span: span_from_range(body.stmt_span(stmt_id)),
                        });
                    }
                }
            }
            for stmt in loop_body {
                check_stmt(
                    body,
                    *stmt,
                    ctx,
                    classes,
                    enums,
                    interfaces,
                    functions,
                    errors,
                    ret_type,
                    returns_result,
                    func_span,
                );
            }
            ctx.exit_scope();
        }
        Stmt::Match {
            subject,
            cases,
            otherwise,
        } => {
            let subject_ty = infer_expr(
                body,
                *subject,
                ctx,
                classes,
                enums,
                interfaces,
                functions,
                errors,
                false,
                true,
                returns_result,
            );
            let mut coverage = MatchCoverage::new(&subject_ty, enums);
            for case in cases {
                if coverage.fully_covered() {
                    errors.push(TypeError::MatchCaseUnreachable {
                        span: match_case_span(body, case, body.stmt_span(stmt_id)),
                    });
                }
                ctx.enter_scope();
                for label in &case.labels {
                    bind_pattern(label, &subject_ty, ctx, enums);
                }
                if let Some(guard) = case.guard {
                    let guard_ty = infer_expr(
                        body,
                        guard,
                        ctx,
                        classes,
                        enums,
                        interfaces,
                        functions,
                        errors,
                        false,
                        returns_result,
                        returns_result,
                    );
                    if types_known(&Type::Boolean, &guard_ty) && !matches!(guard_ty, Type::Boolean)
                    {
                        errors.push(TypeError::MatchGuardNotBoolean {
                            span: span_from_range(body.expr_span(guard)),
                        });
                    }
                }
                for stmt in &case.body {
                    check_stmt(
                        body,
                        *stmt,
                        ctx,
                        classes,
                        enums,
                        interfaces,
                        functions,
                        errors,
                        ret_type,
                        returns_result,
                        func_span,
                    );
                }
                ctx.exit_scope();
                if case.guard.is_none() {
                    coverage.observe_case(case);
                }
            }
            if otherwise.is_none() {
                if let Some(missing) = match_missing_variants(&subject_ty, cases, enums) {
                    errors.push(TypeError::MatchNonExhaustive {
                        missing_variants: missing,
                        span: span_from_range(body.stmt_span(stmt_id)),
                    });
                }
            }
            if let Some(branch) = otherwise {
                ctx.enter_scope();
                for stmt in branch {
                    check_stmt(
                        body,
                        *stmt,
                        ctx,
                        classes,
                        enums,
                        interfaces,
                        functions,
                        errors,
                        ret_type,
                        returns_result,
                        func_span,
                    );
                }
                ctx.exit_scope();
            }
        }
        Stmt::Defer { expr } => {
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
                returns_result,
                returns_result,
            );
        }
        Stmt::Use { .. } => {}
        Stmt::While {
            condition,
            body: loop_body,
        } => {
            let cond_ty = infer_expr(
                body,
                *condition,
                ctx,
                classes,
                enums,
                interfaces,
                functions,
                errors,
                false,
                returns_result,
                returns_result,
            );
            if types_known(&Type::Boolean, &cond_ty) && !matches!(cond_ty, Type::Boolean) {
                errors.push(TypeError::WhileConditionNotBoolean {
                    span: span_from_range(body.expr_span(*condition)),
                });
            }
            ctx.enter_scope();
            for stmt in loop_body {
                check_stmt(
                    body,
                    *stmt,
                    ctx,
                    classes,
                    enums,
                    interfaces,
                    functions,
                    errors,
                    ret_type,
                    returns_result,
                    func_span,
                );
            }
            ctx.exit_scope();
        }
        Stmt::Return(expr) => {
            if let Some(expr) = expr {
                let allow_pending = matches!(ret_type, Some(Type::Pending(_)));
                let value_ty = infer_expr(
                    body,
                    *expr,
                    ctx,
                    classes,
                    enums,
                    interfaces,
                    functions,
                    errors,
                    allow_pending,
                    returns_result,
                    returns_result,
                );
                if let Some(expected) = ret_type {
                    if types_known(expected, &value_ty)
                        && !is_assignable(expected, &value_ty, classes, interfaces)
                    {
                        errors.push(TypeError::ReturnTypeMismatch {
                            expected: type_label(expected),
                            found: type_label(&value_ty),
                            span: span_from_range(body.stmt_span(stmt_id)),
                        });
                    }
                } else if !returns_result && is_result_type(&value_ty) {
                    let span = func_span.unwrap_or_else(|| body.stmt_span(stmt_id));
                    errors.push(TypeError::MissingResultReturn {
                        span: span_from_range(span),
                        help: "This function uses fallible operations (await/Result). Either \
change the return type to Result[...] or handle results with `??`."
                            .to_string(),
                    });
                }
            } else if let Some(expected) = ret_type
                && *expected != Type::Nil
                && *expected != Type::Unknown
            {
                errors.push(TypeError::ReturnTypeMismatch {
                    expected: type_label(expected),
                    found: type_label(&Type::Nil),
                    span: span_from_range(body.stmt_span(stmt_id)),
                });
            }
        }
        Stmt::Break | Stmt::Continue => {}
    }
}

pub(super) fn bind_pattern(
    pattern: &Pattern,
    subject_ty: &Type,
    ctx: &mut TypeContext,
    enums: &EnumIndex,
) {
    match pattern {
        Pattern::Wildcard | Pattern::Literal(_) => {}
        Pattern::Binding(name) => {
            ctx.declare(name.clone(), subject_ty.clone());
        }
        Pattern::Path { parts, args } => {
            if parts.len() == 1
                && parts[0].as_str() == "Ok"
                && let Type::Result(ok, _) = subject_ty
                && let Some(arg) = args.first()
            {
                bind_pattern(arg, ok, ctx, enums);
                return;
            }
            if parts.len() == 1
                && parts[0].as_str() == "Err"
                && let Type::Result(_, err) = subject_ty
                && let Some(arg) = args.first()
            {
                bind_pattern(arg, err, ctx, enums);
                return;
            }
            if parts.len() == 2 {
                let enum_name = &parts[0];
                let variant_name = &parts[1];
                if let Some(en) = enums.get(enum_name)
                    && let Some(params) = en.variants.get(variant_name)
                {
                    for (idx, arg) in args.iter().enumerate() {
                        let ty = params.get(idx).map(|(_, ty)| ty).unwrap_or(&Type::Unknown);
                        bind_pattern(arg, ty, ctx, enums);
                    }
                    return;
                }
            }
            for arg in args {
                bind_pattern(arg, &Type::Unknown, ctx, enums);
            }
        }
        Pattern::Struct { parts, fields } => {
            if parts.len() == 1
                && parts[0].as_str() == "Ok"
                && let Type::Result(ok, _) = subject_ty
            {
                for (_field, pat) in fields {
                    bind_pattern(pat, ok, ctx, enums);
                }
                return;
            }
            if parts.len() == 1
                && parts[0].as_str() == "Err"
                && let Type::Result(_, err) = subject_ty
            {
                for (_field, pat) in fields {
                    bind_pattern(pat, err, ctx, enums);
                }
                return;
            }
            if parts.len() == 2 {
                let enum_name = &parts[0];
                let variant_name = &parts[1];
                if let Some(en) = enums.get(enum_name)
                    && let Some(params) = en.variants.get(variant_name)
                {
                    let param_map: HashMap<SmolStr, Type> = params
                        .iter()
                        .map(|(name, ty)| (name.clone(), ty.clone()))
                        .collect();
                    for (field_name, pat) in fields {
                        let ty = param_map.get(field_name).unwrap_or(&Type::Unknown);
                        bind_pattern(pat, ty, ctx, enums);
                    }
                    return;
                }
            }
            for (_field, pat) in fields {
                bind_pattern(pat, &Type::Unknown, ctx, enums);
            }
        }
    }
}

pub(super) struct MatchCoverage {
    has_wildcard: bool,
    ok_covered: bool,
    err_covered: bool,
    enum_name: Option<SmolStr>,
    enum_variants_total: usize,
    enum_variants_covered: HashSet<SmolStr>,
    subject_is_result: bool,
    subject_is_enum: bool,
}

impl MatchCoverage {
    fn new(subject_ty: &Type, enums: &EnumIndex) -> Self {
        let mut enum_name = None;
        let mut enum_variants_total = 0usize;
        let mut subject_is_enum = false;
        if let Type::Named(name, _) = subject_ty
            && let Some(en) = enums.get(name)
        {
            enum_name = Some(name.clone());
            enum_variants_total = en.variants.len();
            subject_is_enum = true;
        }

        Self {
            has_wildcard: false,
            ok_covered: false,
            err_covered: false,
            enum_name,
            enum_variants_total,
            enum_variants_covered: HashSet::new(),
            subject_is_result: matches!(subject_ty, Type::Result(_, _)),
            subject_is_enum,
        }
    }

    fn observe_case(&mut self, case: &crate::hir::MatchCase) {
        for label in &case.labels {
            self.observe_label(label);
        }
    }

    fn observe_label(&mut self, label: &Pattern) {
        match label {
            Pattern::Wildcard | Pattern::Binding(_) => {
                self.has_wildcard = true;
            }
            Pattern::Path { parts, .. } => {
                if parts.len() == 1 && parts[0].as_str() == "Ok" {
                    self.ok_covered = true;
                } else if parts.len() == 1 && parts[0].as_str() == "Err" {
                    self.err_covered = true;
                } else if parts.len() == 2
                    && let Some(en) = &self.enum_name
                    && parts[0] == *en
                {
                    self.enum_variants_covered.insert(parts[1].clone());
                }
            }
            Pattern::Struct { parts, .. } => {
                if parts.len() == 1 && parts[0].as_str() == "Ok" {
                    self.ok_covered = true;
                } else if parts.len() == 1 && parts[0].as_str() == "Err" {
                    self.err_covered = true;
                } else if parts.len() == 2
                    && let Some(en) = &self.enum_name
                    && parts[0] == *en
                {
                    self.enum_variants_covered.insert(parts[1].clone());
                }
            }
            Pattern::Literal(_) => {}
        }
    }

    fn fully_covered(&self) -> bool {
        if self.has_wildcard {
            return true;
        }
        if self.subject_is_result {
            return self.ok_covered && self.err_covered;
        }
        if self.subject_is_enum {
            return self.enum_variants_total > 0
                && self.enum_variants_covered.len() == self.enum_variants_total;
        }
        false
    }
}

/// Returns None if the match is exhaustive, or Some(missing_variants) if not.
pub(super) fn match_missing_variants(
    subject_ty: &Type,
    cases: &[crate::hir::MatchCase],
    enums: &EnumIndex,
) -> Option<Vec<SmolStr>> {
    let mut coverage = MatchCoverage::new(subject_ty, enums);
    for case in cases {
        if case.guard.is_none() {
            coverage.observe_case(case);
        }
    }
    if coverage.fully_covered() {
        return None;
    }
    // Determine which variants are missing
    if coverage.has_wildcard {
        return None;
    }
    if coverage.subject_is_result {
        let mut missing = Vec::new();
        if !coverage.ok_covered {
            missing.push(SmolStr::new("Ok"));
        }
        if !coverage.err_covered {
            missing.push(SmolStr::new("Err"));
        }
        return Some(missing);
    }
    if coverage.subject_is_enum {
        if let Some(ref enum_name) = coverage.enum_name {
            if let Some(en) = enums.get(enum_name) {
                let missing: Vec<SmolStr> = en
                    .variants
                    .keys()
                    .filter(|v| !coverage.enum_variants_covered.contains(*v))
                    .cloned()
                    .collect();
                if missing.is_empty() {
                    return None;
                }
                return Some(missing);
            }
        }
    }
    Some(Vec::new())
}

pub(super) fn match_case_span(
    body: &Body,
    case: &crate::hir::MatchCase,
    fallback: TextRange,
) -> SourceSpan {
    if let Some(stmt) = case.body.first() {
        return span_from_range(body.stmt_span(*stmt));
    }
    if let Some(guard) = case.guard {
        return span_from_range(body.expr_span(guard));
    }
    span_from_range(fallback)
}

#[derive(Debug, Clone, Copy)]
pub(super) enum AssertEqualityMode {
    Value,
    Identity,
}

impl AssertEqualityMode {
    fn label(self) -> &'static str {
        match self {
            AssertEqualityMode::Value => "value",
            AssertEqualityMode::Identity => "identity",
        }
    }
}

pub(super) fn check_assert_approx(
    body: &Body,
    lhs_id: Idx<Expr>,
    rhs_id: Idx<Expr>,
    tolerance_id: Idx<Expr>,
    ctx: &mut TypeContext,
    classes: &ClassIndex,
    enums: &EnumIndex,
    interfaces: &InterfaceIndex,
    functions: &FunctionIndex,
    errors: &mut Vec<TypeError>,
    returns_result: bool,
    span: SourceSpan,
) {
    let left_ty = infer_expr(
        body,
        lhs_id,
        ctx,
        classes,
        enums,
        interfaces,
        functions,
        errors,
        false,
        returns_result,
        returns_result,
    );
    let right_ty = infer_expr(
        body,
        rhs_id,
        ctx,
        classes,
        enums,
        interfaces,
        functions,
        errors,
        false,
        returns_result,
        returns_result,
    );
    let tolerance_ty = infer_expr(
        body,
        tolerance_id,
        ctx,
        classes,
        enums,
        interfaces,
        functions,
        errors,
        false,
        returns_result,
        returns_result,
    );
    let numeric_ok = is_numeric(&left_ty)
        && is_numeric(&right_ty)
        && is_numeric(&tolerance_ty)
        && (is_assignable(&left_ty, &right_ty, classes, interfaces)
            || is_assignable(&right_ty, &left_ty, classes, interfaces));
    if types_known(&left_ty, &right_ty) && types_known(&left_ty, &tolerance_ty) && !numeric_ok {
        errors.push(TypeError::AssertApproxRequiresNumeric { span });
    }
}

pub(super) fn check_assert_expr(
    body: &Body,
    expr_id: Idx<Expr>,
    mode: AssertEqualityMode,
    ctx: &mut TypeContext,
    classes: &ClassIndex,
    enums: &EnumIndex,
    interfaces: &InterfaceIndex,
    functions: &FunctionIndex,
    errors: &mut Vec<TypeError>,
    returns_result: bool,
    span: SourceSpan,
) {
    match &body.exprs[expr_id] {
        Expr::Binary { lhs, op, rhs, .. } => {
            let allowed = match mode {
                AssertEqualityMode::Identity => matches!(op, BinaryOp::Eq | BinaryOp::Ne),
                AssertEqualityMode::Value => {
                    matches!(
                        op,
                        BinaryOp::Eq
                            | BinaryOp::Ne
                            | BinaryOp::Lt
                            | BinaryOp::Le
                            | BinaryOp::Gt
                            | BinaryOp::Ge
                    )
                }
            };
            if !allowed {
                errors.push(TypeError::AssertExpectedEquality {
                    mode: mode.label(),
                    span,
                });
                return;
            }
            let left_ty = infer_expr(
                body,
                *lhs,
                ctx,
                classes,
                enums,
                interfaces,
                functions,
                errors,
                false,
                returns_result,
                returns_result,
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
                returns_result,
                returns_result,
            );
            if matches!(mode, AssertEqualityMode::Identity)
                && types_known(&left_ty, &right_ty)
                && (is_identity_primitive(&left_ty) || is_identity_primitive(&right_ty))
            {
                errors.push(TypeError::AssertIdentityPrimitive { span });
            }
        }
        _ => {
            errors.push(TypeError::AssertExpectedEquality {
                mode: mode.label(),
                span,
            });
        }
    }
}
