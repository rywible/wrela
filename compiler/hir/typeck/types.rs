use crate::hir::{
    Arg, BinaryOp, Body, ClassRole, Expr, FieldDefault, Function, FunctionKind, FunctionLane,
    FunctionRole, Idx, InterfaceMethodKind, Literal, Module, Pattern, Stmt, TypeRef, UnaryOp,
    Visibility,
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
    I32,
    U32,
    I64,
    U64,
    Float,
    F32,
    Number,
    Boolean,
    String,
    Nil,
    List(Box<Type>),
    Array(Box<Type>, usize),
    Map(Box<Type>, Box<Type>),
    Named(SmolStr, Vec<Type>),
    Param(SmolStr),
    Result(Box<Type>, Box<Type>),
    Actor(Box<Type>),
    Pending(Box<Type>),
    Vec2,
    Vec3,
    Vec4,
    Mat3,
    Mat4,
    Quat,
    GpuBuffer(Box<Type>),
    GpuAtomicI32,
    GpuAtomicU32,
    GpuDispatchSchedule,
    Texture2D,
    Sampler,
}

fn portable_named_type(name: &str) -> Type {
    Type::Named(SmolStr::new(name), Vec::new())
}

fn is_portable_named_data_type_name(name: &str) -> bool {
    matches!(name, "Bounds2" | "Bounds3" | "Ray3" | "Transform3")
}

fn portable_named_field_type(name: &str, member: &str) -> Option<Type> {
    match (name, member) {
        ("Bounds2", "min" | "max") => Some(Type::Vec2),
        ("Bounds3", "min" | "max") => Some(Type::Vec3),
        ("Ray3", "origin" | "direction") => Some(Type::Vec3),
        ("Transform3", "matrix" | "inverse") => Some(Type::Mat4),
        _ => None,
    }
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

    #[error("assert approx requires numeric operands and a numeric tolerance")]
    #[diagnostic(code(lang::ty::assert_approx_numeric))]
    AssertApproxRequiresNumeric {
        #[label("assert approx here")]
        span: SourceSpan,
    },

    #[error("value '{name}' cannot declare methods in the substrate lane")]
    #[diagnostic(code(lang::ty::value_methods_forbidden))]
    ValueMethodsForbidden {
        name: SmolStr,
        #[label("method declared here")]
        span: SourceSpan,
    },

    #[error("value '{name}' cannot implement interfaces")]
    #[diagnostic(code(lang::ty::value_interfaces_forbidden))]
    ValueInterfacesForbidden {
        name: SmolStr,
        #[label("value declared here")]
        span: SourceSpan,
    },

    #[error("value field '{field}' cannot be mutable")]
    #[diagnostic(code(lang::ty::value_field_mutable_forbidden))]
    ValueFieldMutableForbidden {
        field: SmolStr,
        #[label("mutable field here")]
        span: SourceSpan,
    },

    #[error("value field '{field}' must use fixed-layout substrate types (found '{found}')")]
    #[diagnostic(code(lang::ty::value_field_type_forbidden))]
    ValueFieldTypeForbidden {
        field: SmolStr,
        found: String,
        #[label("field type here")]
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

    #[error("'{feature}' is not supported by the CPU reference GPU yet")]
    #[diagnostic(code(lang::ty::unsupported_compute_feature))]
    UnsupportedComputeFeature {
        feature: &'static str,
        #[label("unsupported here")]
        span: SourceSpan,
        #[help]
        help: String,
    },

    #[error("portable function '{function}' cannot use boundary type '{found}' for {site}")]
    #[diagnostic(code(lang::ty::portable_boundary_type_forbidden))]
    PortableBoundaryTypeForbidden {
        function: SmolStr,
        site: String,
        found: String,
        #[label("non-portable boundary here")]
        span: SourceSpan,
        #[help]
        help: String,
    },

    #[error("portable function '{function}' cannot call host-only operation '{callee}'")]
    #[diagnostic(code(lang::ty::portable_host_call_forbidden))]
    PortableHostCallForbidden {
        function: SmolStr,
        callee: SmolStr,
        #[label("host-only call here")]
        span: SourceSpan,
        #[help]
        help: String,
    },

    #[error("dispatch_compute requires `kernel fn`; '{callee}' is not portable-lane code")]
    #[diagnostic(code(lang::ty::dispatch_kernel_must_be_portable))]
    DispatchKernelMustBePortable {
        callee: SmolStr,
        #[label("kernel argument here")]
        span: SourceSpan,
    },

    #[error("portable function '{function}' cannot use {construct}")]
    #[diagnostic(code(lang::ty::portable_construct_forbidden))]
    PortableConstructForbidden {
        function: SmolStr,
        construct: String,
        #[label("non-portable construct here")]
        span: SourceSpan,
        #[help]
        help: String,
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
            TypeError::UnsupportedComputeFeature { span, .. } => *span,
            TypeError::PortableBoundaryTypeForbidden { span, .. } => *span,
            TypeError::PortableHostCallForbidden { span, .. } => *span,
            TypeError::DispatchKernelMustBePortable { span, .. } => *span,
            TypeError::PortableConstructForbidden { span, .. } => *span,
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
            TypeError::AssertApproxRequiresNumeric { span } => *span,
            TypeError::ValueMethodsForbidden { span, .. } => *span,
            TypeError::ValueInterfacesForbidden { span, .. } => *span,
            TypeError::ValueFieldMutableForbidden { span, .. } => *span,
            TypeError::ValueFieldTypeForbidden { span, .. } => *span,
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
    validate_value_classes(module, &class_index, &mut errors);
    validate_portable_lane_functions(module, &class_index, &mut errors);
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
        Stmt::Assert {
            expr,
            rhs,
            tolerance,
            ..
        } => {
            collect_forbidden_float_literals_in_expr(body, *expr, errors);
            if let Some(rhs) = rhs {
                collect_forbidden_float_literals_in_expr(body, *rhs, errors);
            }
            if let Some(tolerance) = tolerance {
                collect_forbidden_float_literals_in_expr(body, *tolerance, errors);
            }
        }
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
        Expr::Closure {
            body: closure_body, ..
        } => {
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

fn validate_value_classes(module: &Module, classes: &ClassIndex, errors: &mut Vec<TypeError>) {
    for (_idx, class) in module.classes.iter() {
        if !matches!(class.role, ClassRole::Value) {
            continue;
        }
        if !class.implements.is_empty() {
            errors.push(TypeError::ValueInterfacesForbidden {
                name: class.name.clone(),
                span: span_from_option_range(class.name_span),
            });
        }
        for method_id in &class.methods {
            let method = &module.functions[*method_id];
            errors.push(TypeError::ValueMethodsForbidden {
                name: class.name.clone(),
                span: span_from_option_range(method.name_span),
            });
        }
        for field in &class.fields {
            if field.mutable {
                errors.push(TypeError::ValueFieldMutableForbidden {
                    field: field.name.clone(),
                    span: span_from_option_range(field.name_span),
                });
            }
            let Some(field_ty) = &field.ty else {
                continue;
            };
            let ty = type_from_ref(field_ty);
            let mut visiting = HashSet::new();
            if !supports_fixed_value_type(&ty, classes, &mut visiting) {
                errors.push(TypeError::ValueFieldTypeForbidden {
                    field: field.name.clone(),
                    found: type_label(&ty),
                    span: span_from_option_range(field_ty.name_span),
                });
            }
        }
    }
}

fn supports_fixed_value_type(
    ty: &Type,
    classes: &ClassIndex,
    visiting: &mut HashSet<SmolStr>,
) -> bool {
    match ty {
        Type::Unknown
        | Type::Never
        | Type::Integer
        | Type::Float
        | Type::Number
        | Type::String
        | Type::Nil
        | Type::List(_)
        | Type::Map(_, _)
        | Type::Result(_, _)
        | Type::Actor(_)
        | Type::Pending(_)
        | Type::GpuBuffer(_)
        | Type::GpuAtomicI32
        | Type::GpuAtomicU32
        | Type::GpuDispatchSchedule
        | Type::Texture2D
        | Type::Sampler => false,
        Type::Boolean | Type::I32 | Type::U32 | Type::I64 | Type::U64 | Type::F32 => true,
        Type::Vec2 | Type::Vec3 | Type::Vec4 | Type::Mat3 | Type::Mat4 | Type::Quat => true,
        Type::Param(_) => false,
        Type::Array(inner, len) => *len > 0 && supports_fixed_value_type(inner, classes, visiting),
        Type::Named(name, args) => {
            if args.is_empty() && is_portable_named_data_type_name(name.as_str()) {
                return true;
            }
            if !args.is_empty() {
                return false;
            }
            let Some(class_sig) = classes.get(name) else {
                return false;
            };
            if !matches!(class_sig.role, ClassRole::Value) {
                return false;
            }
            if !visiting.insert(name.clone()) {
                return true;
            }
            let ok = class_sig
                .field_order
                .iter()
                .filter_map(|field| class_sig.fields.get(field))
                .all(|field_ty| supports_fixed_value_type(field_ty, classes, visiting));
            visiting.remove(name);
            ok
        }
    }
}

fn validate_portable_lane_functions(
    module: &Module,
    classes: &ClassIndex,
    errors: &mut Vec<TypeError>,
) {
    let top_level = portable_function_sets(module);
    for (_func_idx, func) in module.functions.iter() {
        if !matches!(func.lane(), FunctionLane::Portable) {
            continue;
        }
        validate_portable_function_boundary(func, classes, errors);
        if let Some(body) = &func.body {
            validate_portable_block(
                body,
                &body.root_stmts,
                &func.name,
                &top_level,
                classes,
                errors,
            );
        }
    }
}

struct PortableFunctionSets {
    all: HashSet<SmolStr>,
    portable: HashSet<SmolStr>,
}

fn portable_function_sets(module: &Module) -> PortableFunctionSets {
    let mut method_ids = HashSet::new();
    for (_idx, class) in module.classes.iter() {
        for method_id in &class.methods {
            method_ids.insert(*method_id);
        }
    }

    let mut all = HashSet::new();
    let mut portable = HashSet::new();
    for (idx, func) in module.functions.iter() {
        if method_ids.contains(&idx) {
            continue;
        }
        all.insert(func.name.clone());
        if matches!(func.lane(), FunctionLane::Portable) {
            portable.insert(func.name.clone());
        }
    }

    PortableFunctionSets { all, portable }
}

fn validate_portable_function_boundary(
    func: &Function,
    classes: &ClassIndex,
    errors: &mut Vec<TypeError>,
) {
    for param in &func.params {
        let (found, label) = match &param.ty {
            Some(ty) => {
                let found = type_from_ref(ty);
                (found.clone(), type_label(&found))
            }
            None => (Type::Unknown, "inferred".to_string()),
        };
        let mut visiting = HashSet::new();
        if !supports_portable_boundary_type(&found, classes, &mut visiting) {
            errors.push(TypeError::PortableBoundaryTypeForbidden {
                function: func.name.clone(),
                site: format!("parameter '{}'", param.name),
                found: label,
                span: param
                    .ty
                    .as_ref()
                    .and_then(|ty| ty.name_span)
                    .map(span_from_range)
                    .unwrap_or_else(|| span_from_option_range(param.name_span)),
                help: "Portable boundaries must use explicit-width scalars, vector/matrix math types, fixed-layout values, arrays, and kernel-safe buffer/atomic handles.".to_string(),
            });
        }
    }

    let (ret_ty, ret_label, ret_span) = match &func.ret_type {
        Some(ret) => {
            let ty = type_from_ref(ret);
            (
                ty.clone(),
                type_label(&ty),
                ret.name_span
                    .map(span_from_range)
                    .unwrap_or_else(|| span_from_option_range(func.name_span)),
            )
        }
        None => (
            Type::Unknown,
            "inferred".to_string(),
            span_from_option_range(func.name_span),
        ),
    };
    let mut visiting = HashSet::new();
    if !supports_portable_boundary_type(&ret_ty, classes, &mut visiting) {
        errors.push(TypeError::PortableBoundaryTypeForbidden {
            function: func.name.clone(),
            site: "return type".to_string(),
            found: ret_label,
            span: ret_span,
            help: "Portable functions need explicit portable return types so the CPU reference path and future GPU backends share the same ABI.".to_string(),
        });
    }
}

fn supports_portable_boundary_type(
    ty: &Type,
    classes: &ClassIndex,
    visiting: &mut HashSet<SmolStr>,
) -> bool {
    match ty {
        Type::Unknown
        | Type::Integer
        | Type::Float
        | Type::Number
        | Type::String
        | Type::List(_)
        | Type::Map(_, _)
        | Type::Result(_, _)
        | Type::Actor(_)
        | Type::Pending(_)
        | Type::GpuDispatchSchedule
        | Type::Texture2D
        | Type::Sampler
        | Type::Param(_) => false,
        Type::Never
        | Type::Boolean
        | Type::I32
        | Type::U32
        | Type::I64
        | Type::U64
        | Type::F32
        | Type::Nil => true,
        Type::Vec2 | Type::Vec3 | Type::Vec4 | Type::Mat3 | Type::Mat4 | Type::Quat => true,
        Type::GpuBuffer(inner) => supports_portable_boundary_type(inner, classes, visiting),
        Type::GpuAtomicI32 | Type::GpuAtomicU32 => true,
        Type::Array(inner, len) => {
            *len > 0 && supports_portable_boundary_type(inner, classes, visiting)
        }
        Type::Named(name, args) => {
            if args.is_empty() && is_portable_named_data_type_name(name.as_str()) {
                return true;
            }
            if !args.is_empty() {
                return false;
            }
            let Some(class_sig) = classes.get(name) else {
                return false;
            };
            if !matches!(class_sig.role, ClassRole::Value) {
                return false;
            }
            if !visiting.insert(name.clone()) {
                return true;
            }
            let ok = class_sig
                .field_order
                .iter()
                .filter_map(|field| class_sig.fields.get(field))
                .all(|field_ty| supports_portable_boundary_type(field_ty, classes, visiting));
            visiting.remove(name);
            ok
        }
    }
}

fn validate_portable_block(
    body: &Body,
    stmts: &[Idx<Stmt>],
    function: &SmolStr,
    functions: &PortableFunctionSets,
    classes: &ClassIndex,
    errors: &mut Vec<TypeError>,
) {
    for stmt_id in stmts {
        match &body.stmts[*stmt_id] {
            Stmt::Expr(expr) => {
                validate_portable_expr(body, *expr, function, functions, classes, errors);
            }
            Stmt::Assert {
                expr,
                rhs,
                tolerance,
                ..
            } => {
                validate_portable_expr(body, *expr, function, functions, classes, errors);
                if let Some(rhs) = rhs {
                    validate_portable_expr(body, *rhs, function, functions, classes, errors);
                }
                if let Some(tolerance) = tolerance {
                    validate_portable_expr(body, *tolerance, function, functions, classes, errors);
                }
            }
            Stmt::Require { condition, message } => {
                validate_portable_expr(body, *condition, function, functions, classes, errors);
                validate_portable_expr(body, *message, function, functions, classes, errors);
            }
            Stmt::Let { value, .. } | Stmt::Assign { value, .. } => {
                validate_portable_expr(body, *value, function, functions, classes, errors);
            }
            Stmt::Optimize { body: inner, .. } => {
                errors.push(TypeError::PortableConstructForbidden {
                    function: function.clone(),
                    construct: "optimization objective blocks".to_string(),
                    span: span_from_range(body.stmt_span(*stmt_id)),
                    help: "Keep portable kernels focused on deterministic data-parallel work; orchestration stays in the host lane.".to_string(),
                });
                validate_portable_block(body, inner, function, functions, classes, errors);
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
            } => {
                validate_portable_expr(body, *condition, function, functions, classes, errors);
                validate_portable_block(body, then_branch, function, functions, classes, errors);
                if let Some(branch) = else_branch {
                    validate_portable_block(body, branch, function, functions, classes, errors);
                }
            }
            Stmt::For {
                iterable,
                body: inner,
                ..
            } => {
                validate_portable_expr(body, *iterable, function, functions, classes, errors);
                validate_portable_block(body, inner, function, functions, classes, errors);
            }
            Stmt::Match {
                subject,
                cases,
                otherwise,
            } => {
                validate_portable_expr(body, *subject, function, functions, classes, errors);
                for case in cases {
                    if let Some(guard) = case.guard {
                        validate_portable_expr(body, guard, function, functions, classes, errors);
                    }
                    validate_portable_block(body, &case.body, function, functions, classes, errors);
                }
                if let Some(otherwise) = otherwise {
                    validate_portable_block(body, otherwise, function, functions, classes, errors);
                }
            }
            Stmt::IgnoreResult { expr } => {
                errors.push(TypeError::PortableConstructForbidden {
                    function: function.clone(),
                    construct: "`ignore result`".to_string(),
                    span: span_from_range(body.stmt_span(*stmt_id)),
                    help: "Portable code should stay free of host-style result side channels; return portable data explicitly instead.".to_string(),
                });
                validate_portable_expr(body, *expr, function, functions, classes, errors);
            }
            Stmt::Capture { value, .. } => {
                errors.push(TypeError::PortableConstructForbidden {
                    function: function.clone(),
                    construct: "`capture`".to_string(),
                    span: span_from_range(body.stmt_span(*stmt_id)),
                    help: "Captures belong in higher-level field/query semantics, not the kernel portability substrate.".to_string(),
                });
                validate_portable_expr(body, *value, function, functions, classes, errors);
            }
            Stmt::Defer { expr } => {
                errors.push(TypeError::PortableConstructForbidden {
                    function: function.clone(),
                    construct: "`defer`".to_string(),
                    span: span_from_range(body.stmt_span(*stmt_id)),
                    help: "Portable kernels cannot rely on host-style deferred cleanup. Pass handles in and keep execution order-independent.".to_string(),
                });
                validate_portable_expr(body, *expr, function, functions, classes, errors);
            }
            Stmt::While {
                condition,
                body: inner,
            } => {
                validate_portable_expr(body, *condition, function, functions, classes, errors);
                validate_portable_block(body, inner, function, functions, classes, errors);
            }
            Stmt::Return(expr) => {
                if let Some(expr) = expr {
                    validate_portable_expr(body, *expr, function, functions, classes, errors);
                }
            }
            Stmt::Use { .. } | Stmt::Break | Stmt::Continue => {}
        }
    }
}

fn validate_portable_expr(
    body: &Body,
    expr_id: Idx<Expr>,
    function: &SmolStr,
    functions: &PortableFunctionSets,
    classes: &ClassIndex,
    errors: &mut Vec<TypeError>,
) {
    match &body.exprs[expr_id] {
        Expr::Literal(Literal::String(_)) => {
            errors.push(TypeError::PortableConstructForbidden {
                function: function.clone(),
                construct: "String literals".to_string(),
                span: span_from_range(body.expr_span(expr_id)),
                help: "Portable kernels should operate on fixed-layout numeric data, not heap-backed text.".to_string(),
            });
        }
        Expr::Literal(_) | Expr::Variable(_) => {}
        Expr::Detach { target, .. } => {
            errors.push(TypeError::PortableConstructForbidden {
                function: function.clone(),
                construct: "`detach`".to_string(),
                span: span_from_range(body.expr_span(expr_id)),
                help: "Portable kernels cannot spawn host concurrency; dispatch from the host and keep kernel helpers synchronous.".to_string(),
            });
            validate_portable_expr(body, *target, function, functions, classes, errors);
        }
        Expr::Binary { lhs, rhs, .. } => {
            validate_portable_expr(body, *lhs, function, functions, classes, errors);
            validate_portable_expr(body, *rhs, function, functions, classes, errors);
        }
        Expr::Unary { op, expr, .. } => {
            if let Some((construct, help)) = portable_unary_rejection(*op) {
                errors.push(TypeError::PortableConstructForbidden {
                    function: function.clone(),
                    construct: construct.to_string(),
                    span: span_from_range(body.expr_span(expr_id)),
                    help: help.to_string(),
                });
            }
            validate_portable_expr(body, *expr, function, functions, classes, errors);
        }
        Expr::TypeApply { callee, .. } => {
            validate_portable_expr(body, *callee, function, functions, classes, errors);
        }
        Expr::Crash { expr } => {
            errors.push(TypeError::PortableConstructForbidden {
                function: function.clone(),
                construct: "`crash`".to_string(),
                span: span_from_range(body.expr_span(expr_id)),
                help: "Portable kernels should communicate failure through explicit host-side orchestration, not trap semantics.".to_string(),
            });
            validate_portable_expr(body, *expr, function, functions, classes, errors);
        }
        Expr::Call { callee, args, .. } => {
            validate_portable_call(
                body, expr_id, callee, args, function, functions, classes, errors,
            );
            validate_portable_expr(body, *callee, function, functions, classes, errors);
            for arg in args {
                match arg {
                    Arg::Positional { value, .. } | Arg::Named { value, .. } => {
                        validate_portable_expr(body, *value, function, functions, classes, errors);
                    }
                }
            }
        }
        Expr::Member { object, .. } => {
            validate_portable_expr(body, *object, function, functions, classes, errors);
        }
        Expr::Index { object, index, .. } => {
            validate_portable_expr(body, *object, function, functions, classes, errors);
            validate_portable_expr(body, *index, function, functions, classes, errors);
        }
        Expr::List(items) => {
            errors.push(TypeError::PortableConstructForbidden {
                function: function.clone(),
                construct: "List literals".to_string(),
                span: span_from_range(body.expr_span(expr_id)),
                help: "Portable kernels cannot allocate dynamic lists. Use fixed-size arrays or buffers instead.".to_string(),
            });
            for item in items {
                validate_portable_expr(body, *item, function, functions, classes, errors);
            }
        }
        Expr::Map(items) => {
            errors.push(TypeError::PortableConstructForbidden {
                function: function.clone(),
                construct: "Map literals".to_string(),
                span: span_from_range(body.expr_span(expr_id)),
                help: "Portable kernels cannot allocate hash maps. Flatten the data into arrays, values, or buffers instead.".to_string(),
            });
            for (key, value) in items {
                validate_portable_expr(body, *key, function, functions, classes, errors);
                validate_portable_expr(body, *value, function, functions, classes, errors);
            }
        }
        Expr::StringInterp(parts) => {
            errors.push(TypeError::PortableConstructForbidden {
                function: function.clone(),
                construct: "string interpolation".to_string(),
                span: span_from_range(body.expr_span(expr_id)),
                help: "Portable kernels cannot build heap-backed strings. Keep diagnostic formatting in the host lane.".to_string(),
            });
            for part in parts {
                if let crate::hir::StringPart::Expr(expr) = part {
                    validate_portable_expr(body, *expr, function, functions, classes, errors);
                }
            }
        }
        Expr::Closure {
            body: closure_body, ..
        } => {
            errors.push(TypeError::PortableConstructForbidden {
                function: function.clone(),
                construct: "closures".to_string(),
                span: span_from_range(body.expr_span(expr_id)),
                help: "Portable kernels need a direct, backend-neutral call graph. Hoist helper logic into named functions instead.".to_string(),
            });
            validate_portable_expr(body, *closure_body, function, functions, classes, errors);
        }
    }
}

fn validate_portable_call(
    body: &Body,
    expr_id: Idx<Expr>,
    callee: &Idx<Expr>,
    _args: &[Arg],
    function: &SmolStr,
    functions: &PortableFunctionSets,
    classes: &ClassIndex,
    errors: &mut Vec<TypeError>,
) {
    match &body.exprs[*callee] {
        Expr::Variable(name) => {
            if is_portable_safe_builtin_call(name.as_str()) {
                return;
            }
            if functions.portable.contains(name) {
                return;
            }
            if classes
                .get(name)
                .is_some_and(|class| matches!(class.role, ClassRole::Value))
            {
                return;
            }
            if functions.all.contains(name) || is_host_only_builtin_call(name.as_str()) {
                errors.push(TypeError::PortableHostCallForbidden {
                    function: function.clone(),
                    callee: name.clone(),
                    span: span_from_range(body.expr_span(expr_id)),
                    help: "Portable code may use pure math intrinsics and kernel-safe buffer/atomic access, but host orchestration and I/O stay outside the portable lane.".to_string(),
                });
            }
        }
        Expr::Member { object, member, .. } => {
            let object_ty = portable_member_object_type(body, *object, classes);
            if let Type::Named(name, args) = object_ty
                && args.is_empty()
                && is_portable_named_data_type_name(name.as_str())
                && portable_named_field_type(name.as_str(), member.as_str()).is_some()
            {
                errors.push(TypeError::PortableConstructForbidden {
                    function: function.clone(),
                    construct: format!("calling field '{}.{}(...)'", name, member),
                    span: span_from_range(body.expr_span(expr_id)),
                    help: "Portable data primitives expose fields directly; access the field value instead of calling it like a method.".to_string(),
                });
            }
        }
        _ => {}
    }
}

fn portable_member_object_type(body: &Body, expr_id: Idx<Expr>, classes: &ClassIndex) -> Type {
    match &body.exprs[expr_id] {
        Expr::Variable(_) => Type::Unknown,
        Expr::Member { object, member, .. } => {
            let object_ty = portable_member_object_type(body, *object, classes);
            match object_ty {
                Type::Named(name, args) if args.is_empty() => {
                    portable_named_field_type(name.as_str(), member.as_str())
                        .unwrap_or(Type::Unknown)
                }
                _ => Type::Unknown,
            }
        }
        Expr::Call { callee, .. } => {
            if let Expr::Variable(name) = &body.exprs[*callee] {
                if classes
                    .get(name)
                    .is_some_and(|class| matches!(class.role, ClassRole::Value))
                {
                    return Type::Named(name.clone(), Vec::new());
                }
                if is_portable_named_data_type_name(name.as_str()) {
                    return portable_named_type(name.as_str());
                }
            }
            Type::Unknown
        }
        _ => Type::Unknown,
    }
}

fn portable_unary_rejection(op: UnaryOp) -> Option<(&'static str, &'static str)> {
    match op {
        UnaryOp::Await => Some((
            "`await`",
            "Portable kernels are synchronous. Await work in the host lane and pass the resolved data in.",
        )),
        UnaryOp::Spawn => Some((
            "`spawn`",
            "Portable kernels cannot spawn host tasks. Split host orchestration from portable compute helpers.",
        )),
        UnaryOp::Fire => Some((
            "`fire`",
            "Portable kernels cannot enqueue fire-and-forget work. Keep side-effect scheduling in the host lane.",
        )),
        UnaryOp::Err => Some((
            "`error`",
            "Portable kernels do not expose host-style Result error channels yet. Return portable data and let the host orchestrate failures.",
        )),
        UnaryOp::Try => Some((
            "`?`",
            "Portable kernels do not expose host-style Result propagation yet. Keep failure handling in the host lane for now.",
        )),
        _ => None,
    }
}

fn is_portable_safe_builtin_call(name: &str) -> bool {
    matches!(
        name,
        "vec2"
            | "vec3"
            | "vec4"
            | "quat"
            | "mat3_identity"
            | "mat3_cols"
            | "mat4_identity"
            | "mat4_cols"
            | "bounds2"
            | "bounds3"
            | "ray3"
            | "transform3"
            | "transform3_identity"
            | "bounds2_center"
            | "bounds2_size"
            | "bounds3_center"
            | "bounds3_size"
            | "transform_point"
            | "transform_vector"
            | "transform_normal"
            | "compose_transform3"
            | "inverse_transform3"
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
            | "u32"
            | "gpu_buffer_len"
            | "gpu_buffer_get"
            | "gpu_buffer_set"
            | "gpu_atomic_i32_load"
            | "gpu_atomic_i32_store"
            | "gpu_atomic_i32_fetch_add"
            | "gpu_atomic_u32_load"
            | "gpu_atomic_u32_store"
            | "gpu_atomic_u32_fetch_add"
            | "global_invocation_id"
            | "local_invocation_id"
            | "workgroup_id"
            | "num_workgroups"
            | "workgroup_size"
            | "workgroup_barrier"
            | "storage_barrier"
    )
}

fn is_host_only_builtin_call(name: &str) -> bool {
    name.starts_with("__wr_")
        || matches!(
            name,
            "try_to_call_external"
                | "try_to_http_call"
                | "dispatch_compute"
                | "gpu_buffer_new"
                | "gpu_atomic_i32_new"
                | "gpu_atomic_i32_drop"
                | "gpu_atomic_u32_new"
                | "gpu_atomic_u32_drop"
                | "gpu_schedule_deterministic"
                | "gpu_schedule_reverse"
                | "gpu_schedule_shuffle"
                | "gpu_schedule_workgroup_reverse"
                | "gpu_schedule_workgroup_shuffle"
                | "gpu_schedule_round_robin_workgroups"
        )
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
