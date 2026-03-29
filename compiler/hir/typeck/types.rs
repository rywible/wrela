use crate::hir::{
    Arg, BinaryOp, Body, Expr, FieldDefault, Function, FunctionKind, FunctionRole, Idx,
    InterfaceMethodKind, Literal, Module, Pattern, Stmt, TypeRef, UnaryOp, Visibility,
};
use miette::{Diagnostic, SourceSpan};
use rowan::TextRange;
use smol_str::SmolStr;
use std::collections::{HashMap, HashSet};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    Unknown,
    Never,
    Integer,
    Float,
    Number,
    Boolean,
    String,
    Nil,
    List(Box<Type>),
    Map(Box<Type>, Box<Type>),
    Named(SmolStr, Vec<Type>),
    Param(SmolStr),
    Result(Box<Type>, Box<Type>),
    Actor(Box<Type>),
    Pending(Box<Type>),
    Vec2,
    Vec3,
    Vec4,
    Mat4,
    GpuBuffer(Box<Type>),
    Texture2D,
    Sampler,
}

#[derive(Debug, Error, Diagnostic, Clone)]
pub enum TypeError {
    #[error("invalid operand for unary operator '{op}'")]
    #[diagnostic(code(lang::ty::invalid_unary_operand))]
    InvalidUnaryOperand {
        op: &'static str,
        #[label("invalid operand")]
        span: SourceSpan,
    },

    #[error("invalid operands for binary operator '{op}'")]
    #[diagnostic(code(lang::ty::invalid_binary_operands))]
    InvalidBinaryOperands {
        op: &'static str,
        #[label("invalid operands")]
        span: SourceSpan,
    },

    #[error("cannot assign value of type '{found}' to '{name}' (expected '{expected}')")]
    #[diagnostic(code(lang::ty::invalid_assignment))]
    InvalidAssignment {
        name: SmolStr,
        expected: String,
        found: String,
        #[label("assignment here")]
        span: SourceSpan,
    },

    #[error("cannot assign to immutable field '{member}'")]
    #[diagnostic(code(lang::ty::immutable_field_assign))]
    ImmutableFieldAssign {
        member: SmolStr,
        #[label("assignment here")]
        span: SourceSpan,
        #[help]
        help: String,
    },

    #[error("return type '{found}' does not match expected '{expected}'")]
    #[diagnostic(code(lang::ty::return_mismatch))]
    ReturnTypeMismatch {
        expected: String,
        found: String,
        #[label("return here")]
        span: SourceSpan,
    },

    #[error("unknown member '{member}' on type '{object}'")]
    #[diagnostic(code(lang::ty::unknown_member))]
    UnknownMember {
        object: String,
        member: SmolStr,
        #[label("unknown member")]
        span: SourceSpan,
    },

    #[error("cannot call '{member}' because it is a field")]
    #[diagnostic(code(lang::ty::call_field))]
    CallField {
        member: SmolStr,
        #[label("called here")]
        span: SourceSpan,
    },

    #[error("cannot call this value (only functions, classes, and methods are callable)")]
    #[diagnostic(code(lang::ty::invalid_callee))]
    InvalidCallee {
        #[label("call target here")]
        span: SourceSpan,
    },

    #[error("require condition must be Boolean")]
    #[diagnostic(code(lang::ty::require_condition_boolean))]
    RequireConditionNotBoolean {
        #[label("condition here")]
        span: SourceSpan,
    },

    #[error("if condition must be Boolean")]
    #[diagnostic(code(lang::ty::if_condition_boolean))]
    IfConditionNotBoolean {
        #[label("condition here")]
        span: SourceSpan,
    },

    #[error("match guard condition must be Boolean")]
    #[diagnostic(code(lang::ty::match_guard_boolean))]
    MatchGuardNotBoolean {
        #[label("guard here")]
        span: SourceSpan,
    },

    #[error("while condition must be Boolean")]
    #[diagnostic(code(lang::ty::while_condition_boolean))]
    WhileConditionNotBoolean {
        #[label("condition here")]
        span: SourceSpan,
    },

    #[error("require message must be a String")]
    #[diagnostic(code(lang::ty::require_message_string))]
    RequireMessageNotString {
        #[label("message here")]
        span: SourceSpan,
    },

    #[error("capture requires a Result value")]
    #[diagnostic(code(lang::ty::capture_requires_result))]
    CaptureRequiresResult {
        #[label("capture here")]
        span: SourceSpan,
    },

    #[error("ignore result requires a Result value")]
    #[diagnostic(code(lang::ty::ignore_result_requires_result))]
    IgnoreResultRequiresResult {
        #[label("ignore result here")]
        span: SourceSpan,
    },

    #[error("argument count mismatch (expected {expected}, found {found})")]
    #[diagnostic(code(lang::ty::arg_count))]
    ArgumentCountMismatch {
        expected: usize,
        found: usize,
        #[label("call here")]
        span: SourceSpan,
    },

    #[error("unknown argument '{name}'")]
    #[diagnostic(code(lang::ty::unknown_argument))]
    UnknownArgument {
        name: SmolStr,
        #[label("argument here")]
        span: SourceSpan,
    },

    #[error("calls with more than one parameter require named arguments")]
    #[diagnostic(
        code(lang::ty::named_args_required),
        help(
            "Use named arguments when calling functions, methods, or class constructors with multiple parameters."
        )
    )]
    NamedArgsRequired {
        #[label("call here")]
        span: SourceSpan,
        param_names: Vec<SmolStr>,
        arg_spans: Vec<SourceSpan>,
    },

    #[error("assert identity does not allow primitive values")]
    #[diagnostic(code(lang::ty::assert_identity_primitive))]
    AssertIdentityPrimitive {
        #[label("assert identity here")]
        span: SourceSpan,
    },

    #[error("assert {mode} expects an equality expression")]
    #[diagnostic(code(lang::ty::assert_expected_equality))]
    AssertExpectedEquality {
        mode: &'static str,
        #[label("assert here")]
        span: SourceSpan,
    },

    #[error("equality operator requires Eq-compatible operands (found '{left}' and '{right}')")]
    #[diagnostic(
        code(lang::ty::equality_requires_eq),
        help("Use structurally comparable types. Actor and Pending values are not comparable.")
    )]
    EqualityRequiresEq {
        left: String,
        right: String,
        #[label("equality here")]
        span: SourceSpan,
    },

    #[error("argument '{name}' has type '{found}' but expected '{expected}'")]
    #[diagnostic(code(lang::ty::argument_type))]
    ArgumentTypeMismatch {
        name: SmolStr,
        expected: String,
        found: String,
        #[label("argument here")]
        span: SourceSpan,
    },

    #[error("type argument count mismatch for '{name}' (expected {expected}, found {found})")]
    #[diagnostic(code(lang::ty::type_arg_count))]
    TypeArgCountMismatch {
        name: SmolStr,
        expected: usize,
        found: usize,
        #[label("type arguments here")]
        span: SourceSpan,
    },

    #[error("missing type arguments for '{name}'")]
    #[diagnostic(code(lang::ty::missing_type_args))]
    MissingTypeArgs {
        name: SmolStr,
        #[label("type name here")]
        span: SourceSpan,
    },

    #[error("generic type '{name}' must declare type arguments at module boundaries")]
    #[diagnostic(
        code(lang::ty::boundary_missing_type_args),
        help("Use explicit type arguments in function signatures and class/interface fields.")
    )]
    BoundaryMissingTypeArgs {
        name: SmolStr,
        #[label("add type arguments here")]
        span: SourceSpan,
    },

    #[error("type arguments are not allowed here")]
    #[diagnostic(code(lang::ty::unexpected_type_args))]
    UnexpectedTypeArgs {
        #[label("type arguments here")]
        span: SourceSpan,
    },

    #[error("type application requires a call")]
    #[diagnostic(code(lang::ty::type_apply_without_call))]
    TypeApplyWithoutCall {
        #[label("type application here")]
        span: SourceSpan,
    },

    #[error("unknown interface '{name}'")]
    #[diagnostic(code(lang::ty::unknown_interface))]
    UnknownInterface {
        name: SmolStr,
        #[label("interface here")]
        span: SourceSpan,
    },

    #[error("class '{class}' is missing method '{interface}.{method}'")]
    #[diagnostic(code(lang::ty::missing_interface_method))]
    MissingInterfaceMethod {
        class: SmolStr,
        interface: SmolStr,
        method: SmolStr,
        #[label("class here")]
        span: SourceSpan,
    },

    #[error("class '{class}' does not match signature for '{interface}.{method}'")]
    #[diagnostic(code(lang::ty::interface_method_mismatch))]
    InterfaceMethodMismatch {
        class: SmolStr,
        interface: SmolStr,
        method: SmolStr,
        #[label("class here")]
        span: SourceSpan,
    },

    #[error("await expects a pending value")]
    #[diagnostic(code(lang::ty::invalid_await_operand))]
    InvalidAwaitOperand {
        #[label("await here")]
        span: SourceSpan,
    },

    #[error("`?` expects a Result value")]
    #[diagnostic(code(lang::ty::invalid_try_operand))]
    InvalidTryOperand {
        #[label("`?` here")]
        span: SourceSpan,
    },

    #[error("`?` can only be used in functions that return Result")]
    #[diagnostic(code(lang::ty::try_outside_result))]
    TryOutsideResult {
        #[label("`?` here")]
        span: SourceSpan,
    },

    #[error("fire expects a pending value")]
    #[diagnostic(code(lang::ty::invalid_fire_operand))]
    InvalidFireOperand {
        #[label("fire here")]
        span: SourceSpan,
    },

    #[error("pending value must be awaited or fired")]
    #[diagnostic(code(lang::ty::pending_not_awaited))]
    PendingNotAwaited {
        #[label("pending value here")]
        span: SourceSpan,
        #[help]
        help: String,
    },

    #[error(
        "result must be handled with `??`, `match`, `ignore result`, `capture`, or returned from a `Result` function"
    )]
    #[diagnostic(code(lang::ty::unhandled_result))]
    UnhandledResult {
        #[label("result here")]
        span: SourceSpan,
        #[help]
        help: String,
    },

    #[error("`??` expects a Result on the left side")]
    #[diagnostic(code(lang::ty::invalid_otherwise))]
    InvalidOtherwiseOperand {
        #[label("?? here")]
        span: SourceSpan,
    },

    #[error("`error` can only be used in functions that return Result")]
    #[diagnostic(code(lang::ty::err_outside_result))]
    ErrOutsideResult {
        #[label("error here")]
        span: SourceSpan,
    },

    #[error("function must return Result because it contains fallible operations")]
    #[diagnostic(code(lang::ty::missing_result_return))]
    MissingResultReturn {
        #[label("function here")]
        span: SourceSpan,
        #[help]
        help: String,
    },

    #[error("cannot access members on actor instances")]
    #[diagnostic(code(lang::ty::actor_member_access))]
    ActorMemberAccess {
        member: SmolStr,
        #[label("member access here")]
        span: SourceSpan,
    },

    #[error("class '{class}' contains await and must be instantiated as an actor")]
    #[diagnostic(code(lang::ty::async_class_requires_actor))]
    AsyncClassRequiresActor {
        class: SmolStr,
        #[label("class instantiation here")]
        span: SourceSpan,
        #[help]
        help: String,
    },

    #[error("method '{class}.{member}' contains await and must be called on an actor instance")]
    #[diagnostic(code(lang::ty::async_method_requires_actor))]
    AsyncMethodRequiresActor {
        class: SmolStr,
        member: SmolStr,
        #[label("method call here")]
        span: SourceSpan,
        #[help]
        help: String,
    },

    #[error("match must be exhaustive or include an `otherwise:` case")]
    #[diagnostic(
        code(lang::ty::match_non_exhaustive),
        help("Add missing cases: {}", missing_variants.join(", "))
    )]
    MatchNonExhaustive {
        missing_variants: Vec<SmolStr>,
        #[label("match here")]
        span: SourceSpan,
    },

    #[error("match case is unreachable because previous unguarded cases already cover all values")]
    #[diagnostic(code(lang::ty::match_case_unreachable))]
    MatchCaseUnreachable {
        #[label("unreachable case")]
        span: SourceSpan,
    },

    #[error("indexing is only supported on List and Map")]
    #[diagnostic(code(lang::ty::invalid_index_target))]
    InvalidIndexTarget {
        #[label("index here")]
        span: SourceSpan,
    },

    #[error("index type mismatch (expected '{expected}', found '{found}')")]
    #[diagnostic(code(lang::ty::invalid_index_type))]
    InvalidIndexType {
        expected: String,
        found: String,
        #[label("index here")]
        span: SourceSpan,
    },

    #[error("`with index` is only supported for list/range loops")]
    #[diagnostic(code(lang::ty::for_with_index_requires_list_or_range))]
    ForWithIndexRequiresListOrRange {
        #[label("for loop here")]
        span: SourceSpan,
    },

    #[error("`for key, value in ...` requires a map iterable")]
    #[diagnostic(code(lang::ty::for_map_requires_map))]
    ForMapRequiresMap {
        #[label("for loop here")]
        span: SourceSpan,
    },

    #[error("map loops do not support `with index`")]
    #[diagnostic(code(lang::ty::for_map_with_index_unsupported))]
    ForMapWithIndexUnsupported {
        #[label("for loop here")]
        span: SourceSpan,
    },

    #[error("type parameter '{param}' requires bound '{bound}' but '{found}' does not satisfy it")]
    #[diagnostic(code(lang::ty::type_param_bound_not_satisfied))]
    TypeParamBoundNotSatisfied {
        param: SmolStr,
        bound: SmolStr,
        found: String,
        #[label("bound not satisfied")]
        span: SourceSpan,
    },

    #[error("Float type is not allowed in deterministic modules")]
    #[diagnostic(
        code(lang::ty::deterministic_float_type_forbidden),
        help("Use scaled Integer values for deterministic arithmetic.")
    )]
    DeterministicFloatTypeForbidden {
        #[label("Float type is forbidden here")]
        span: SourceSpan,
    },

    #[error("float literals are not allowed in deterministic modules")]
    #[diagnostic(
        code(lang::ty::deterministic_float_literal_forbidden),
        help("Use scaled Integer values for deterministic arithmetic.")
    )]
    DeterministicFloatLiteralForbidden {
        #[label("float literal is forbidden here")]
        span: SourceSpan,
    },
}

impl TypeError {
    pub fn primary_span(&self) -> SourceSpan {
        match self {
            TypeError::InvalidUnaryOperand { span, .. } => *span,
            TypeError::InvalidBinaryOperands { span, .. } => *span,
            TypeError::InvalidAssignment { span, .. } => *span,
            TypeError::ImmutableFieldAssign { span, .. } => *span,
            TypeError::ReturnTypeMismatch { span, .. } => *span,
            TypeError::UnknownMember { span, .. } => *span,
            TypeError::CallField { span, .. } => *span,
            TypeError::InvalidCallee { span } => *span,
            TypeError::RequireConditionNotBoolean { span } => *span,
            TypeError::IfConditionNotBoolean { span } => *span,
            TypeError::MatchGuardNotBoolean { span } => *span,
            TypeError::WhileConditionNotBoolean { span } => *span,
            TypeError::RequireMessageNotString { span } => *span,
            TypeError::CaptureRequiresResult { span } => *span,
            TypeError::IgnoreResultRequiresResult { span } => *span,
            TypeError::ArgumentCountMismatch { span, .. } => *span,
            TypeError::UnknownArgument { span, .. } => *span,
            TypeError::NamedArgsRequired { span, .. } => *span,
            TypeError::ArgumentTypeMismatch { span, .. } => *span,
            TypeError::TypeArgCountMismatch { span, .. } => *span,
            TypeError::MissingTypeArgs { span, .. } => *span,
            TypeError::BoundaryMissingTypeArgs { span, .. } => *span,
            TypeError::UnexpectedTypeArgs { span, .. } => *span,
            TypeError::TypeApplyWithoutCall { span, .. } => *span,
            TypeError::UnknownInterface { span, .. } => *span,
            TypeError::MissingInterfaceMethod { span, .. } => *span,
            TypeError::InterfaceMethodMismatch { span, .. } => *span,
            TypeError::AssertIdentityPrimitive { span } => *span,
            TypeError::AssertExpectedEquality { span, .. } => *span,
            TypeError::EqualityRequiresEq { span, .. } => *span,
            TypeError::InvalidAwaitOperand { span } => *span,
            TypeError::InvalidTryOperand { span } => *span,
            TypeError::TryOutsideResult { span } => *span,
            TypeError::InvalidFireOperand { span } => *span,
            TypeError::PendingNotAwaited { span, .. } => *span,
            TypeError::UnhandledResult { span, .. } => *span,
            TypeError::InvalidOtherwiseOperand { span } => *span,
            TypeError::ErrOutsideResult { span } => *span,
            TypeError::MissingResultReturn { span, .. } => *span,
            TypeError::ActorMemberAccess { span, .. } => *span,
            TypeError::AsyncClassRequiresActor { span, .. } => *span,
            TypeError::AsyncMethodRequiresActor { span, .. } => *span,
            TypeError::MatchNonExhaustive { span, .. } => *span,
            TypeError::MatchCaseUnreachable { span } => *span,
            TypeError::InvalidIndexTarget { span } => *span,
            TypeError::InvalidIndexType { span, .. } => *span,
            TypeError::ForWithIndexRequiresListOrRange { span } => *span,
            TypeError::ForMapRequiresMap { span } => *span,
            TypeError::ForMapWithIndexUnsupported { span } => *span,
            TypeError::TypeParamBoundNotSatisfied { span, .. } => *span,
            TypeError::DeterministicFloatTypeForbidden { span } => *span,
            TypeError::DeterministicFloatLiteralForbidden { span } => *span,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct FunctionTypeInfo {
    pub expr_types: HashMap<usize, Type>,
    pub local_types: HashMap<SmolStr, Type>,
}

#[derive(Debug, Clone, Default)]
pub struct TypeInfo {
    pub functions: HashMap<usize, FunctionTypeInfo>,
}

impl TypeInfo {
    pub fn function(&self, idx: Idx<Function>) -> Option<&FunctionTypeInfo> {
        self.functions.get(&idx.into_raw())
    }
}

pub fn check_module_with_info(module: &Module) -> (Vec<TypeError>, TypeInfo) {
    let mut errors = Vec::new();
    let mut info = TypeInfo::default();
    if module_is_deterministic_game_module(module) {
        enforce_deterministic_fixed_lane_policy(module, &mut errors);
    }
    validate_boundary_type_refs(module, &mut errors);
    let class_index = ClassIndex::new(module);
    let enum_index = EnumIndex::new(module);
    let interface_index = InterfaceIndex::new(module);
    let function_index = FunctionIndex::new(module);
    let mut method_map = HashMap::new();
    for (_idx, class) in module.classes.iter() {
        for method_id in &class.methods {
            method_map.insert(method_id.into_raw(), class.name.clone());
        }
    }
    for (idx, func) in module.functions.iter() {
        let method_class = method_map.get(&idx.into_raw()).cloned();
        check_function(
            func,
            idx,
            &class_index,
            &enum_index,
            &interface_index,
            &function_index,
            &mut errors,
            method_class,
            &mut info,
        );
    }
    check_interface_conformance(&class_index, &interface_index, &mut errors);
    check_async_actor_usage(module, &info, &class_index, &mut errors);
    (errors, info)
}

pub fn check_module(module: &Module) -> Vec<TypeError> {
    let (errors, _info) = check_module_with_info(module);
    errors
}

fn module_is_deterministic_game_module(module: &Module) -> bool {
    module
        .functions
        .iter()
        .any(|(_, func)| matches!(func.role, FunctionRole::System))
}

fn enforce_deterministic_fixed_lane_policy(module: &Module, errors: &mut Vec<TypeError>) {
    for (_idx, function) in module.functions.iter() {
        for param in &function.params {
            if let Some(ty) = &param.ty {
                collect_forbidden_float_type_refs(ty, errors);
            }
        }
        if let Some(ret) = &function.ret_type {
            collect_forbidden_float_type_refs(ret, errors);
        }
        if let Some(body) = &function.body {
            collect_forbidden_float_literals_in_stmts(body, &body.root_stmts, errors);
        }
    }

    for (_idx, class) in module.classes.iter() {
        for field in &class.fields {
            if let Some(ty) = &field.ty {
                collect_forbidden_float_type_refs(ty, errors);
            }
            if let Some(default) = &field.default {
                collect_forbidden_float_literals_in_field_default(
                    default,
                    field
                        .name_span
                        .map(span_from_range)
                        .unwrap_or_else(|| SourceSpan::from((0usize, 0usize))),
                    errors,
                );
            }
        }
    }

    for (_idx, en) in module.enums.iter() {
        for variant in &en.variants {
            for param in &variant.params {
                if let Some(ty) = &param.ty {
                    collect_forbidden_float_type_refs(ty, errors);
                }
            }
        }
    }

    for (_idx, interface) in module.interfaces.iter() {
        for method in &interface.methods {
            for param in &method.params {
                if let Some(ty) = &param.ty {
                    collect_forbidden_float_type_refs(ty, errors);
                }
            }
            if let Some(ret) = &method.ret_type {
                collect_forbidden_float_type_refs(ret, errors);
            }
        }
    }
}

fn collect_forbidden_float_type_refs(ty: &TypeRef, errors: &mut Vec<TypeError>) {
    if ty.name.as_str() == "Float" {
        errors.push(TypeError::DeterministicFloatTypeForbidden {
            span: span_from_option_range(ty.name_span),
        });
    }
    for arg in &ty.args {
        collect_forbidden_float_type_refs(arg, errors);
    }
}

fn collect_forbidden_float_literals_in_field_default(
    default: &FieldDefault,
    fallback_span: SourceSpan,
    errors: &mut Vec<TypeError>,
) {
    match default {
        FieldDefault::Literal(Literal::Float(_)) => {
            errors.push(TypeError::DeterministicFloatLiteralForbidden {
                span: fallback_span,
            });
        }
        FieldDefault::Literal(_) => {}
        FieldDefault::List(items) => {
            for item in items {
                collect_forbidden_float_literals_in_field_default(item, fallback_span, errors);
            }
        }
        FieldDefault::Map(items) => {
            for (key, value) in items {
                collect_forbidden_float_literals_in_field_default(key, fallback_span, errors);
                collect_forbidden_float_literals_in_field_default(value, fallback_span, errors);
            }
        }
    }
}

fn collect_forbidden_float_literals_in_stmts(
    body: &Body,
    stmts: &[Idx<Stmt>],
    errors: &mut Vec<TypeError>,
) {
    for stmt_id in stmts {
        collect_forbidden_float_literals_in_stmt(body, *stmt_id, errors);
    }
}

fn collect_forbidden_float_literals_in_stmt(
    body: &Body,
    stmt_id: Idx<Stmt>,
    errors: &mut Vec<TypeError>,
) {
    match &body.stmts[stmt_id] {
        Stmt::Expr(expr) => collect_forbidden_float_literals_in_expr(body, *expr, errors),
        Stmt::Assert { expr, .. } => collect_forbidden_float_literals_in_expr(body, *expr, errors),
        Stmt::Require { condition, message } => {
            collect_forbidden_float_literals_in_expr(body, *condition, errors);
            collect_forbidden_float_literals_in_expr(body, *message, errors);
        }
        Stmt::Let { value, .. } => collect_forbidden_float_literals_in_expr(body, *value, errors),
        Stmt::Assign { value, .. } => {
            collect_forbidden_float_literals_in_expr(body, *value, errors);
        }
        Stmt::Optimize { body: opt_body, .. } => {
            collect_forbidden_float_literals_in_stmts(body, opt_body, errors);
        }
        Stmt::If {
            condition,
            then_branch,
            else_branch,
        } => {
            collect_forbidden_float_literals_in_expr(body, *condition, errors);
            collect_forbidden_float_literals_in_stmts(body, then_branch, errors);
            if let Some(branch) = else_branch {
                collect_forbidden_float_literals_in_stmts(body, branch, errors);
            }
        }
        Stmt::For {
            iterable,
            body: loop_body,
            ..
        } => {
            collect_forbidden_float_literals_in_expr(body, *iterable, errors);
            collect_forbidden_float_literals_in_stmts(body, loop_body, errors);
        }
        Stmt::Match {
            subject,
            cases,
            otherwise,
        } => {
            collect_forbidden_float_literals_in_expr(body, *subject, errors);
            for case in cases {
                let case_span = match_case_span(body, case, body.stmt_span(stmt_id));
                for label in &case.labels {
                    collect_forbidden_float_literals_in_pattern(label, case_span, errors);
                }
                if let Some(guard) = case.guard {
                    collect_forbidden_float_literals_in_expr(body, guard, errors);
                }
                collect_forbidden_float_literals_in_stmts(body, &case.body, errors);
            }
            if let Some(branch) = otherwise {
                collect_forbidden_float_literals_in_stmts(body, branch, errors);
            }
        }
        Stmt::IgnoreResult { expr } => {
            collect_forbidden_float_literals_in_expr(body, *expr, errors)
        }
        Stmt::Capture { value, .. } => {
            collect_forbidden_float_literals_in_expr(body, *value, errors)
        }
        Stmt::Defer { expr } => collect_forbidden_float_literals_in_expr(body, *expr, errors),
        Stmt::Use { .. } => {}
        Stmt::While {
            condition,
            body: loop_body,
        } => {
            collect_forbidden_float_literals_in_expr(body, *condition, errors);
            collect_forbidden_float_literals_in_stmts(body, loop_body, errors);
        }
        Stmt::Return(expr) => {
            if let Some(expr) = expr {
                collect_forbidden_float_literals_in_expr(body, *expr, errors);
            }
        }
        Stmt::Break | Stmt::Continue => {}
    }
}

fn collect_forbidden_float_literals_in_pattern(
    pattern: &Pattern,
    fallback_span: SourceSpan,
    errors: &mut Vec<TypeError>,
) {
    match pattern {
        Pattern::Literal(Literal::Float(_)) => {
            errors.push(TypeError::DeterministicFloatLiteralForbidden {
                span: fallback_span,
            });
        }
        Pattern::Path { args, .. } => {
            for arg in args {
                collect_forbidden_float_literals_in_pattern(arg, fallback_span, errors);
            }
        }
        Pattern::Struct { fields, .. } => {
            for (_name, value) in fields {
                collect_forbidden_float_literals_in_pattern(value, fallback_span, errors);
            }
        }
        Pattern::Wildcard | Pattern::Binding(_) | Pattern::Literal(_) => {}
    }
}

fn collect_forbidden_float_literals_in_expr(
    body: &Body,
    expr_id: Idx<Expr>,
    errors: &mut Vec<TypeError>,
) {
    match &body.exprs[expr_id] {
        Expr::Literal(Literal::Float(_)) => {
            errors.push(TypeError::DeterministicFloatLiteralForbidden {
                span: span_from_range(body.expr_span(expr_id)),
            });
        }
        Expr::Literal(_) | Expr::Variable(_) => {}
        Expr::Detach { target, .. } => {
            collect_forbidden_float_literals_in_expr(body, *target, errors)
        }
        Expr::Binary { lhs, rhs, .. } => {
            collect_forbidden_float_literals_in_expr(body, *lhs, errors);
            collect_forbidden_float_literals_in_expr(body, *rhs, errors);
        }
        Expr::Unary { expr, .. } => collect_forbidden_float_literals_in_expr(body, *expr, errors),
        Expr::TypeApply { callee, type_args } => {
            collect_forbidden_float_literals_in_expr(body, *callee, errors);
            for ty in type_args {
                collect_forbidden_float_type_refs(ty, errors);
            }
        }
        Expr::Crash { expr } => collect_forbidden_float_literals_in_expr(body, *expr, errors),
        Expr::Call {
            callee,
            args,
            type_args,
        } => {
            collect_forbidden_float_literals_in_expr(body, *callee, errors);
            for arg in args {
                let value = match arg {
                    Arg::Positional { value, .. } => value,
                    Arg::Named { value, .. } => value,
                };
                collect_forbidden_float_literals_in_expr(body, *value, errors);
            }
            for ty in type_args {
                collect_forbidden_float_type_refs(ty, errors);
            }
        }
        Expr::Member { object, .. } => {
            collect_forbidden_float_literals_in_expr(body, *object, errors);
        }
        Expr::Index { object, index, .. } => {
            collect_forbidden_float_literals_in_expr(body, *object, errors);
            collect_forbidden_float_literals_in_expr(body, *index, errors);
        }
        Expr::List(items) => {
            for item in items {
                collect_forbidden_float_literals_in_expr(body, *item, errors);
            }
        }
        Expr::Map(items) => {
            for (key, value) in items {
                collect_forbidden_float_literals_in_expr(body, *key, errors);
                collect_forbidden_float_literals_in_expr(body, *value, errors);
            }
        }
        Expr::StringInterp(parts) => {
            for part in parts {
                if let crate::hir::StringPart::Expr(expr) = part {
                    collect_forbidden_float_literals_in_expr(body, *expr, errors);
                }
            }
        }
        Expr::Closure { body: closure_body, .. } => {
            collect_forbidden_float_literals_in_expr(body, *closure_body, errors);
        }
    }
}

fn validate_boundary_type_refs(module: &Module, errors: &mut Vec<TypeError>) {
    for (_idx, func) in module.functions.iter() {
        if func.visibility != Visibility::Public {
            continue;
        }
        for param in &func.params {
            if let Some(ty) = &param.ty {
                collect_boundary_missing_type_args(ty, errors);
            }
        }
        if let Some(ret) = &func.ret_type {
            collect_boundary_missing_type_args(ret, errors);
        }
    }

    for (_idx, class) in module.classes.iter() {
        for field in &class.fields {
            if field.visibility != Visibility::Public {
                continue;
            }
            if let Some(ty) = &field.ty {
                collect_boundary_missing_type_args(ty, errors);
            }
        }
    }

    for (_idx, interface) in module.interfaces.iter() {
        if interface.visibility != Visibility::Public {
            continue;
        }
        for method in &interface.methods {
            for param in &method.params {
                if let Some(ty) = &param.ty {
                    collect_boundary_missing_type_args(ty, errors);
                }
            }
            if let Some(ret) = &method.ret_type {
                collect_boundary_missing_type_args(ret, errors);
            }
        }
    }
}

fn collect_boundary_missing_type_args(ty: &TypeRef, errors: &mut Vec<TypeError>) {
    if is_boundary_generic_name(ty.name.as_str()) && ty.args.is_empty() {
        errors.push(TypeError::BoundaryMissingTypeArgs {
            name: ty.name.clone(),
            span: span_from_option_range(ty.name_span),
        });
    }
    for arg in &ty.args {
        collect_boundary_missing_type_args(arg, errors);
    }
}

fn is_boundary_generic_name(name: &str) -> bool {
    matches!(name, "List" | "Map" | "Result" | "Actor" | "Pending")
}
