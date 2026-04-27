use crate::hir::{
    BinaryOp, Body, Expr, Function, Module, RuntimeFunctionMetadata, Stmt, TypeRef, UnaryOp,
};
use smol_str::SmolStr;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AudioRtErrorKind {
    Allocation,
    UnboundedLoop,
    BlockingEffect,
    NonAudioRtCallee,
    UnboundedResultPropagation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioRtError {
    pub function: SmolStr,
    pub kind: AudioRtErrorKind,
    pub detail: String,
}

pub fn check_audio_rt_module(module: &Module) -> Vec<AudioRtError> {
    let functions = module
        .functions
        .iter()
        .map(|(_, function)| (function.name.clone(), function))
        .collect::<BTreeMap<_, _>>();
    let mut errors = Vec::new();
    for (_, function) in module.functions.iter() {
        if is_audio_rt(function) {
            check_function(function, &functions, &mut errors);
        }
    }
    errors
}

fn check_function(
    function: &Function,
    functions: &BTreeMap<SmolStr, &Function>,
    errors: &mut Vec<AudioRtError>,
) {
    if type_is_result(function.ret_type.as_ref()) {
        push_error(
            errors,
            function,
            AudioRtErrorKind::UnboundedResultPropagation,
            "audio_rt functions must return bounded numeric values, not Result",
        );
    }
    let Some(body) = &function.body else {
        return;
    };
    for stmt in &body.root_stmts {
        check_stmt(function, body, *stmt, functions, errors);
    }
}

fn check_stmt(
    function: &Function,
    body: &Body,
    stmt_id: crate::hir::Idx<Stmt>,
    functions: &BTreeMap<SmolStr, &Function>,
    errors: &mut Vec<AudioRtError>,
) {
    match &body.stmts[stmt_id] {
        Stmt::Expr(expr)
        | Stmt::Let { value: expr, .. }
        | Stmt::Assign { value: expr, .. }
        | Stmt::IgnoreResult { expr }
        | Stmt::Capture { value: expr, .. }
        | Stmt::Defer { expr }
        | Stmt::Require {
            condition: expr, ..
        }
        | Stmt::Return(Some(expr)) => check_expr(function, body, *expr, functions, errors),
        Stmt::Assert {
            expr,
            rhs,
            tolerance,
            ..
        } => {
            check_expr(function, body, *expr, functions, errors);
            if let Some(rhs) = rhs {
                check_expr(function, body, *rhs, functions, errors);
            }
            if let Some(tolerance) = tolerance {
                check_expr(function, body, *tolerance, functions, errors);
            }
        }
        Stmt::If {
            condition,
            then_branch,
            else_branch,
        } => {
            check_expr(function, body, *condition, functions, errors);
            check_stmt_list(function, body, then_branch, functions, errors);
            if let Some(else_branch) = else_branch {
                check_stmt_list(function, body, else_branch, functions, errors);
            }
        }
        Stmt::For {
            iterable,
            body: loop_body,
            ..
        } => {
            push_error(
                errors,
                function,
                AudioRtErrorKind::UnboundedLoop,
                "audio_rt functions must not depend on iterable loop bounds",
            );
            check_expr(function, body, *iterable, functions, errors);
            check_stmt_list(function, body, loop_body, functions, errors);
        }
        Stmt::While {
            condition,
            body: loop_body,
        } => {
            push_error(
                errors,
                function,
                AudioRtErrorKind::UnboundedLoop,
                "audio_rt functions must not use while loops",
            );
            check_expr(function, body, *condition, functions, errors);
            check_stmt_list(function, body, loop_body, functions, errors);
        }
        Stmt::Match {
            subject,
            cases,
            otherwise,
        } => {
            check_expr(function, body, *subject, functions, errors);
            for case in cases {
                if let Some(guard) = case.guard {
                    check_expr(function, body, guard, functions, errors);
                }
                check_stmt_list(function, body, &case.body, functions, errors);
            }
            if let Some(otherwise) = otherwise {
                check_stmt_list(function, body, otherwise, functions, errors);
            }
        }
        Stmt::Optimize { body: block, .. } => {
            check_stmt_list(function, body, block, functions, errors);
        }
        Stmt::Use { .. } | Stmt::Return(None) | Stmt::Break | Stmt::Continue => {}
    }
}

fn check_stmt_list(
    function: &Function,
    body: &Body,
    stmts: &[crate::hir::Idx<Stmt>],
    functions: &BTreeMap<SmolStr, &Function>,
    errors: &mut Vec<AudioRtError>,
) {
    for stmt in stmts {
        check_stmt(function, body, *stmt, functions, errors);
    }
}

fn check_expr(
    function: &Function,
    body: &Body,
    expr_id: crate::hir::Idx<Expr>,
    functions: &BTreeMap<SmolStr, &Function>,
    errors: &mut Vec<AudioRtError>,
) {
    match &body.exprs[expr_id] {
        Expr::List(items) => {
            push_error(
                errors,
                function,
                AudioRtErrorKind::Allocation,
                "audio_rt functions must not allocate lists",
            );
            for item in items {
                check_expr(function, body, *item, functions, errors);
            }
        }
        Expr::Map(items) => {
            push_error(
                errors,
                function,
                AudioRtErrorKind::Allocation,
                "audio_rt functions must not allocate maps",
            );
            for (key, value) in items {
                check_expr(function, body, *key, functions, errors);
                check_expr(function, body, *value, functions, errors);
            }
        }
        Expr::StringInterp(parts) => {
            push_error(
                errors,
                function,
                AudioRtErrorKind::Allocation,
                "audio_rt functions must not allocate interpolated strings",
            );
            for part in parts {
                if let crate::hir::StringPart::Expr(expr) = part {
                    check_expr(function, body, *expr, functions, errors);
                }
            }
        }
        Expr::Closure {
            body: closure_body, ..
        } => {
            push_error(
                errors,
                function,
                AudioRtErrorKind::Allocation,
                "audio_rt functions must not allocate closures",
            );
            check_expr(function, body, *closure_body, functions, errors);
        }
        Expr::Call { callee, args, .. } => {
            if let Expr::Variable(name) = &body.exprs[*callee] {
                check_callee(function, name, functions, errors);
            } else {
                push_error(
                    errors,
                    function,
                    AudioRtErrorKind::Allocation,
                    "audio_rt functions must use direct static callees",
                );
            }
            check_expr(function, body, *callee, functions, errors);
            for arg in args {
                match arg {
                    crate::hir::Arg::Positional { value, .. }
                    | crate::hir::Arg::Named { value, .. } => {
                        check_expr(function, body, *value, functions, errors)
                    }
                }
            }
        }
        Expr::Binary { lhs, op, rhs, .. } => {
            if *op == BinaryOp::Otherwise {
                push_error(
                    errors,
                    function,
                    AudioRtErrorKind::UnboundedResultPropagation,
                    "audio_rt functions must not use Result fallback propagation",
                );
            }
            check_expr(function, body, *lhs, functions, errors);
            check_expr(function, body, *rhs, functions, errors);
        }
        Expr::Unary { op, expr, .. } => {
            if *op == UnaryOp::Try {
                push_error(
                    errors,
                    function,
                    AudioRtErrorKind::UnboundedResultPropagation,
                    "audio_rt functions must not propagate Result values with try",
                );
            }
            check_expr(function, body, *expr, functions, errors);
        }
        Expr::TypeApply {
            callee,
            type_args: _,
            ..
        } => check_expr(function, body, *callee, functions, errors),
        Expr::Crash { expr }
        | Expr::Member { object: expr, .. }
        | Expr::Index { object: expr, .. }
        | Expr::Detach { target: expr, .. } => check_expr(function, body, *expr, functions, errors),
        Expr::Literal(_) | Expr::Variable(_) => {}
    }
}

fn check_callee(
    function: &Function,
    name: &SmolStr,
    functions: &BTreeMap<SmolStr, &Function>,
    errors: &mut Vec<AudioRtError>,
) {
    if is_blocking_call(name) {
        push_error(
            errors,
            function,
            AudioRtErrorKind::BlockingEffect,
            format!("audio_rt function calls blocking effect '{name}'"),
        );
    }
    if let Some(callee) = functions.get(name)
        && !is_audio_rt(callee)
    {
        push_error(
            errors,
            function,
            AudioRtErrorKind::NonAudioRtCallee,
            format!("audio_rt function calls non-audio_rt function '{name}'"),
        );
    }
}

fn is_audio_rt(function: &Function) -> bool {
    function
        .attributes
        .iter()
        .any(|attr| attr.name == "audio_rt")
        || match &function.runtime_metadata {
            Some(RuntimeFunctionMetadata::AudioField(metadata))
            | Some(RuntimeFunctionMetadata::MediaField(metadata)) => metadata.audio_rt,
            _ => false,
        }
}

fn is_blocking_call(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains("wait")
        || lower.contains("sleep")
        || lower.contains("block")
        || lower.contains("mutex")
        || lower.contains("lock")
        || lower.contains("http")
        || lower.contains("fs")
        || lower.contains("read_file")
        || lower.contains("write_file")
}

fn type_is_result(ty: Option<&TypeRef>) -> bool {
    let Some(ty) = ty else {
        return false;
    };
    ty.name == "Result" || ty.args.iter().any(|arg| type_is_result(Some(arg)))
}

fn push_error(
    errors: &mut Vec<AudioRtError>,
    function: &Function,
    kind: AudioRtErrorKind,
    detail: impl Into<String>,
) {
    errors.push(AudioRtError {
        function: function.name.clone(),
        kind,
        detail: detail.into(),
    });
}
