#![allow(unused_assignments)]

use crate::hir::{
    BinaryOp, Body, Expr, Function, FunctionKind, Idx, InterfaceMethodKind, Literal, Module,
    Pattern, Stmt, TypeRef, UnaryOp,
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

    #[error("cannot call '{member}' because it is a derived property")]
    #[diagnostic(code(lang::ty::call_derived_property))]
    CallDerivedProperty {
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

    #[error("checks must be evaluated with `given`")]
    #[diagnostic(
        code(lang::ty::check_requires_given),
        help("Use `<check> given ...` instead of a normal call.")
    )]
    CheckRequiresGiven {
        #[label("check call here")]
        span: SourceSpan,
    },

    #[error("`given` can only be used with checks")]
    #[diagnostic(
        code(lang::ty::given_requires_check),
        help("Use a normal call for non-check functions or methods.")
    )]
    GivenRequiresCheck {
        #[label("given usage here")]
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
        "result must be handled with `otherwise`, `match`, `ignore result`, `capture`, or returned from a `Result` function"
    )]
    #[diagnostic(code(lang::ty::unhandled_result))]
    UnhandledResult {
        #[label("result here")]
        span: SourceSpan,
        #[help]
        help: String,
    },

    #[error("`otherwise` expects a Result on the left side")]
    #[diagnostic(code(lang::ty::invalid_otherwise))]
    InvalidOtherwiseOperand {
        #[label("otherwise here")]
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
        help("Add missing cases or add an `otherwise:` case.")
    )]
    MatchNonExhaustive {
        #[label("match here")]
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
            TypeError::CallDerivedProperty { span, .. } => *span,
            TypeError::InvalidCallee { span } => *span,
            TypeError::CheckRequiresGiven { span } => *span,
            TypeError::GivenRequiresCheck { span } => *span,
            TypeError::RequireConditionNotBoolean { span } => *span,
            TypeError::IfConditionNotBoolean { span } => *span,
            TypeError::WhileConditionNotBoolean { span } => *span,
            TypeError::RequireMessageNotString { span } => *span,
            TypeError::CaptureRequiresResult { span } => *span,
            TypeError::IgnoreResultRequiresResult { span } => *span,
            TypeError::ArgumentCountMismatch { span, .. } => *span,
            TypeError::UnknownArgument { span, .. } => *span,
            TypeError::ArgumentTypeMismatch { span, .. } => *span,
            TypeError::TypeArgCountMismatch { span, .. } => *span,
            TypeError::MissingTypeArgs { span, .. } => *span,
            TypeError::UnexpectedTypeArgs { span, .. } => *span,
            TypeError::TypeApplyWithoutCall { span, .. } => *span,
            TypeError::UnknownInterface { span, .. } => *span,
            TypeError::MissingInterfaceMethod { span, .. } => *span,
            TypeError::InterfaceMethodMismatch { span, .. } => *span,
            TypeError::AssertIdentityPrimitive { span } => *span,
            TypeError::AssertExpectedEquality { span, .. } => *span,
            TypeError::InvalidAwaitOperand { span } => *span,
            TypeError::InvalidFireOperand { span } => *span,
            TypeError::PendingNotAwaited { span, .. } => *span,
            TypeError::UnhandledResult { span, .. } => *span,
            TypeError::InvalidOtherwiseOperand { span } => *span,
            TypeError::ErrOutsideResult { span } => *span,
            TypeError::MissingResultReturn { span, .. } => *span,
            TypeError::ActorMemberAccess { span, .. } => *span,
            TypeError::AsyncClassRequiresActor { span, .. } => *span,
            TypeError::AsyncMethodRequiresActor { span, .. } => *span,
            TypeError::MatchNonExhaustive { span } => *span,
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

fn check_function(
    func: &Function,
    func_id: Idx<Function>,
    classes: &ClassIndex,
    enums: &EnumIndex,
    interfaces: &InterfaceIndex,
    functions: &FunctionIndex,
    errors: &mut Vec<TypeError>,
    method_class: Option<SmolStr>,
    info: &mut TypeInfo,
) {
    let mut fn_info = FunctionTypeInfo::default();
    let mut ctx = TypeContext::with_info(&mut fn_info);
    ctx.enter_scope();
    if let Some(class_name) = &method_class {
        if let Some(class_sig) = classes.get(class_name) {
            let self_ty = Type::Named(
                class_name.clone(),
                class_sig
                    .type_params
                    .iter()
                    .cloned()
                    .map(Type::Param)
                    .collect(),
            );
            ctx.enter_type_params(&class_sig.type_params);
            ctx.declare(class_name.clone(), self_ty.clone());
            ctx.declare(SmolStr::new("it"), self_ty.clone());
            ctx.declare(SmolStr::new("its"), self_ty);
        } else {
            ctx.declare(
                class_name.clone(),
                Type::Named(class_name.clone(), Vec::new()),
            );
            ctx.declare(
                SmolStr::new("it"),
                Type::Named(class_name.clone(), Vec::new()),
            );
            ctx.declare(
                SmolStr::new("its"),
                Type::Named(class_name.clone(), Vec::new()),
            );
        }
    }
    for param in &func.params {
        let ty = param
            .ty
            .as_ref()
            .map(|t| type_from_ref_in_ctx(t, &ctx))
            .unwrap_or(Type::Unknown);
        ctx.declare(param.name.clone(), ty);
    }
    let ret_type = func
        .ret_type
        .as_ref()
        .map(|t| type_from_ref_in_ctx(t, &ctx));
    let returns_result = matches!(ret_type, Some(Type::Result(_, _)));
    if let Some(body) = &func.body {
        for stmt in &body.root_stmts {
            check_stmt(
                body,
                *stmt,
                &mut ctx,
                classes,
                enums,
                interfaces,
                functions,
                errors,
                ret_type.as_ref(),
                returns_result,
                func.name_span,
            );
        }
    }
    ctx.exit_scope();
    if method_class.is_some() {
        ctx.exit_type_params();
    }
    info.functions.insert(func_id.into_raw(), fn_info);
}

#[derive(Debug, Clone)]
struct MethodSig {
    params: Vec<(SmolStr, Type)>,
    ret: Type,
    kind: FunctionKind,
}

#[derive(Debug, Clone)]
struct FunctionSig {
    params: Vec<(SmolStr, Type)>,
    ret: Type,
    kind: FunctionKind,
}

#[derive(Debug, Clone)]
struct ClassSig {
    type_params: Vec<SmolStr>,
    fields: HashMap<SmolStr, Type>,
    field_mutable: HashMap<SmolStr, bool>,
    methods: HashMap<SmolStr, MethodSig>,
    field_order: Vec<SmolStr>,
    implements: Vec<SmolStr>,
    name_span: Option<TextRange>,
}

struct ClassIndex {
    classes: HashMap<SmolStr, ClassSig>,
}

#[derive(Debug, Clone)]
struct EnumSig {
    type_params: Vec<SmolStr>,
    variants: HashMap<SmolStr, Vec<(SmolStr, Type)>>,
}

#[derive(Debug, Clone)]
struct InterfaceSig {
    methods: HashMap<SmolStr, InterfaceMethodSig>,
}

#[derive(Debug, Clone)]
struct InterfaceMethodSig {
    params: Vec<(SmolStr, Type)>,
    ret: Type,
    kind: InterfaceMethodKind,
}

struct EnumIndex {
    enums: HashMap<SmolStr, EnumSig>,
}

struct InterfaceIndex {
    interfaces: HashMap<SmolStr, InterfaceSig>,
}

impl ClassIndex {
    fn new(module: &Module) -> Self {
        let mut classes = HashMap::new();
        for (_idx, class) in module.classes.iter() {
            let type_params = class.type_params.clone();
            let param_set: HashSet<SmolStr> = type_params.iter().cloned().collect();
            let mut fields = HashMap::new();
            let mut field_mutable = HashMap::new();
            let mut field_order = Vec::new();
            for field in &class.fields {
                let ty = field
                    .ty
                    .as_ref()
                    .map(|t| type_from_ref_with_params(t, &param_set))
                    .unwrap_or(Type::Unknown);
                fields.insert(field.name.clone(), ty);
                field_mutable.insert(field.name.clone(), field.mutable);
                field_order.push(field.name.clone());
            }
            let mut methods = HashMap::new();
            for method_id in &class.methods {
                let method = &module.functions[*method_id];
                let params = method
                    .params
                    .iter()
                    .map(|param| {
                        (
                            param.name.clone(),
                            param
                                .ty
                                .as_ref()
                                .map(|t| type_from_ref_with_params(t, &param_set))
                                .unwrap_or(Type::Unknown),
                        )
                    })
                    .collect();
                let ret = method
                    .ret_type
                    .as_ref()
                    .map(|t| type_from_ref_with_params(t, &param_set))
                    .unwrap_or(Type::Unknown);
                methods.insert(
                    method.name.clone(),
                    MethodSig {
                        params,
                        ret,
                        kind: method.kind,
                    },
                );
            }
            classes.insert(
                class.name.clone(),
                ClassSig {
                    type_params: type_params.clone(),
                    fields,
                    methods,
                    field_order,
                    field_mutable,
                    implements: class.implements.clone(),
                    name_span: class.name_span,
                },
            );
        }
        Self { classes }
    }

    fn get(&self, name: &SmolStr) -> Option<&ClassSig> {
        self.classes.get(name)
    }

    fn is_class(&self, name: &SmolStr) -> bool {
        self.classes.contains_key(name)
    }
}

impl EnumIndex {
    fn new(module: &Module) -> Self {
        let mut enums = HashMap::new();
        for (_idx, en) in module.enums.iter() {
            let type_params = en.type_params.clone();
            let param_set: HashSet<SmolStr> = type_params.iter().cloned().collect();
            let mut variants = HashMap::new();
            for variant in &en.variants {
                let params = variant
                    .params
                    .iter()
                    .map(|param| {
                        (
                            param.name.clone(),
                            param
                                .ty
                                .as_ref()
                                .map(|t| type_from_ref_with_params(t, &param_set))
                                .unwrap_or(Type::Unknown),
                        )
                    })
                    .collect();
                variants.insert(variant.name.clone(), params);
            }
            enums.insert(
                en.name.clone(),
                EnumSig {
                    type_params: type_params.clone(),
                    variants,
                },
            );
        }
        Self { enums }
    }

    fn get(&self, name: &SmolStr) -> Option<&EnumSig> {
        self.enums.get(name)
    }
}

impl InterfaceIndex {
    fn new(module: &Module) -> Self {
        let mut interfaces = HashMap::new();
        for (_idx, interface) in module.interfaces.iter() {
            let type_params = interface.type_params.clone();
            let param_set: HashSet<SmolStr> = type_params.iter().cloned().collect();
            let mut methods = HashMap::new();
            for method in &interface.methods {
                let params = method
                    .params
                    .iter()
                    .map(|param| {
                        (
                            param.name.clone(),
                            param
                                .ty
                                .as_ref()
                                .map(|t| type_from_ref_with_params(t, &param_set))
                                .unwrap_or(Type::Unknown),
                        )
                    })
                    .collect();
                let ret = method
                    .ret_type
                    .as_ref()
                    .map(|t| type_from_ref_with_params(t, &param_set))
                    .unwrap_or(Type::Unknown);
                methods.insert(
                    method.name.clone(),
                    InterfaceMethodSig {
                        params,
                        ret,
                        kind: method.kind,
                    },
                );
            }
            interfaces.insert(interface.name.clone(), InterfaceSig { methods });
        }
        Self { interfaces }
    }

    fn get(&self, name: &SmolStr) -> Option<&InterfaceSig> {
        self.interfaces.get(name)
    }

    fn is_interface(&self, name: &SmolStr) -> bool {
        self.interfaces.contains_key(name)
    }
}

fn check_interface_conformance(
    classes: &ClassIndex,
    interfaces: &InterfaceIndex,
    errors: &mut Vec<TypeError>,
) {
    for (class_name, class) in classes.classes.iter() {
        for iface_name in &class.implements {
            let Some(iface) = interfaces.get(iface_name) else {
                errors.push(TypeError::UnknownInterface {
                    name: iface_name.clone(),
                    span: span_from_range(
                        class
                            .name_span
                            .unwrap_or_else(|| TextRange::empty(0.into())),
                    ),
                });
                continue;
            };
            for (method_name, iface_method) in &iface.methods {
                let Some(class_method) = class.methods.get(method_name) else {
                    errors.push(TypeError::MissingInterfaceMethod {
                        class: class_name.clone(),
                        interface: iface_name.clone(),
                        method: method_name.clone(),
                        span: span_from_range(
                            class
                                .name_span
                                .unwrap_or_else(|| TextRange::empty(0.into())),
                        ),
                    });
                    continue;
                };
                if class_method.kind == FunctionKind::Derived
                    || !interface_method_matches(iface_method, class_method)
                {
                    errors.push(TypeError::InterfaceMethodMismatch {
                        class: class_name.clone(),
                        interface: iface_name.clone(),
                        method: method_name.clone(),
                        span: span_from_range(
                            class
                                .name_span
                                .unwrap_or_else(|| TextRange::empty(0.into())),
                        ),
                    });
                }
            }
        }
    }
}

struct FunctionIndex {
    functions: HashMap<SmolStr, FunctionSig>,
}

impl FunctionIndex {
    fn new(module: &Module) -> Self {
        let mut method_ids = HashSet::new();
        for (_idx, class) in module.classes.iter() {
            for method_id in &class.methods {
                method_ids.insert(method_id.into_raw());
            }
        }

        let mut functions = HashMap::new();
        for (idx, func) in module.functions.iter() {
            if method_ids.contains(&idx.into_raw()) {
                continue;
            }
            let params = func
                .params
                .iter()
                .map(|param| {
                    (
                        param.name.clone(),
                        param
                            .ty
                            .as_ref()
                            .map(type_from_ref)
                            .unwrap_or(Type::Unknown),
                    )
                })
                .collect();
            let ret = func
                .ret_type
                .as_ref()
                .map(type_from_ref)
                .unwrap_or(Type::Unknown);
            functions.insert(
                func.name.clone(),
                FunctionSig {
                    params,
                    ret,
                    kind: func.kind,
                },
            );
        }
        for (name, sig) in builtin_functions() {
            functions.entry(name).or_insert(sig);
        }
        Self { functions }
    }

    fn get(&self, name: &SmolStr) -> Option<&FunctionSig> {
        self.functions.get(name)
    }
}

fn builtin_functions() -> Vec<(SmolStr, FunctionSig)> {
    let err = error_type();
    vec![
        (
            SmolStr::new("__wr_assert_err"),
            FunctionSig {
                params: vec![(
                    SmolStr::new("value"),
                    Type::Result(Box::new(Type::Unknown), Box::new(Type::Unknown)),
                )],
                ret: Type::Nil,
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_print"),
            FunctionSig {
                params: vec![(SmolStr::new("value"), Type::String)],
                ret: Type::Nil,
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_bytes_from_string"),
            FunctionSig {
                params: vec![(SmolStr::new("value"), Type::String)],
                ret: Type::Named(SmolStr::new("Bytes"), Vec::new()),
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_bytes_from_list"),
            FunctionSig {
                params: vec![(SmolStr::new("value"), Type::List(Box::new(Type::Integer)))],
                ret: Type::Named(SmolStr::new("Bytes"), Vec::new()),
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_bytes_to_string"),
            FunctionSig {
                params: vec![(
                    SmolStr::new("value"),
                    Type::Named(SmolStr::new("Bytes"), Vec::new()),
                )],
                ret: Type::String,
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_bytes_to_list"),
            FunctionSig {
                params: vec![(
                    SmolStr::new("value"),
                    Type::Named(SmolStr::new("Bytes"), Vec::new()),
                )],
                ret: Type::List(Box::new(Type::Integer)),
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_bytes_len"),
            FunctionSig {
                params: vec![(
                    SmolStr::new("value"),
                    Type::Named(SmolStr::new("Bytes"), Vec::new()),
                )],
                ret: Type::Integer,
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_fs_read_bytes"),
            FunctionSig {
                params: vec![(SmolStr::new("path"), Type::String)],
                ret: Type::Result(
                    Box::new(Type::Named(SmolStr::new("Bytes"), Vec::new())),
                    Box::new(err.clone()),
                ),
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_fs_write_bytes"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("path"), Type::String),
                    (
                        SmolStr::new("contents"),
                        Type::Named(SmolStr::new("Bytes"), Vec::new()),
                    ),
                ],
                ret: Type::Result(Box::new(Type::Nil), Box::new(err.clone())),
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_external_call"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("service"), Type::String),
                    (SmolStr::new("endpoint"), Type::String),
                    (SmolStr::new("method"), Type::String),
                    (SmolStr::new("url"), Type::String),
                    (SmolStr::new("headers"), Type::Unknown),
                    (SmolStr::new("body"), Type::String),
                    (SmolStr::new("timeout_ms"), Type::Integer),
                ],
                ret: Type::Result(Box::new(Type::String), Box::new(err.clone())),
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_http_call"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("service"), Type::String),
                    (SmolStr::new("endpoint"), Type::String),
                    (SmolStr::new("method"), Type::String),
                    (SmolStr::new("url"), Type::String),
                    (SmolStr::new("headers"), Type::Unknown),
                    (SmolStr::new("body"), Type::String),
                    (SmolStr::new("timeout_ms"), Type::Integer),
                ],
                ret: Type::Result(Box::new(Type::String), Box::new(err.clone())),
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_list_push"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("list"), Type::List(Box::new(Type::Unknown))),
                    (SmolStr::new("value"), Type::Unknown),
                ],
                ret: Type::Nil,
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_map_new"),
            FunctionSig {
                params: vec![],
                ret: Type::Map(Box::new(Type::Unknown), Box::new(Type::Unknown)),
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_map_get"),
            FunctionSig {
                params: vec![
                    (
                        SmolStr::new("map"),
                        Type::Map(Box::new(Type::Unknown), Box::new(Type::Unknown)),
                    ),
                    (SmolStr::new("key"), Type::Unknown),
                ],
                ret: Type::Unknown,
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_map_len"),
            FunctionSig {
                params: vec![(
                    SmolStr::new("map"),
                    Type::Map(Box::new(Type::Unknown), Box::new(Type::Unknown)),
                )],
                ret: Type::Integer,
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_map_set"),
            FunctionSig {
                params: vec![
                    (
                        SmolStr::new("map"),
                        Type::Map(Box::new(Type::Unknown), Box::new(Type::Unknown)),
                    ),
                    (SmolStr::new("key"), Type::Unknown),
                    (SmolStr::new("value"), Type::Unknown),
                ],
                ret: Type::Nil,
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_str_len"),
            FunctionSig {
                params: vec![(SmolStr::new("text"), Type::String)],
                ret: Type::Integer,
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_log"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("level"), Type::String),
                    (SmolStr::new("message"), Type::String),
                    (
                        SmolStr::new("fields"),
                        Type::Map(Box::new(Type::Unknown), Box::new(Type::Unknown)),
                    ),
                ],
                ret: Type::Boolean,
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_log_configure"),
            FunctionSig {
                params: vec![(SmolStr::new("config"), Type::Unknown)],
                ret: Type::Nil,
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_runtime_cpu_count"),
            FunctionSig {
                params: vec![],
                ret: Type::Integer,
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_reactor_new"),
            FunctionSig {
                params: vec![],
                ret: Type::Integer,
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_reactor_drop"),
            FunctionSig {
                params: vec![(SmolStr::new("reactor"), Type::Integer)],
                ret: Type::Boolean,
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_reactor_register"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("reactor"), Type::Integer),
                    (SmolStr::new("token"), Type::Integer),
                ],
                ret: Type::Boolean,
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_reactor_deregister"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("reactor"), Type::Integer),
                    (SmolStr::new("token"), Type::Integer),
                ],
                ret: Type::Boolean,
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_reactor_arm_timer"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("reactor"), Type::Integer),
                    (SmolStr::new("token"), Type::Integer),
                    (SmolStr::new("timeout_ms"), Type::Integer),
                ],
                ret: Type::Boolean,
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_task_signal_new"),
            FunctionSig {
                params: vec![],
                ret: Type::Integer,
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_task_signal_drop"),
            FunctionSig {
                params: vec![(SmolStr::new("signal"), Type::Integer)],
                ret: Type::Boolean,
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_task_unpark_one"),
            FunctionSig {
                params: vec![(SmolStr::new("signal"), Type::Integer)],
                ret: Type::Boolean,
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_task_unpark_all"),
            FunctionSig {
                params: vec![(SmolStr::new("signal"), Type::Integer)],
                ret: Type::Boolean,
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_task_epoch"),
            FunctionSig {
                params: vec![(SmolStr::new("signal"), Type::Integer)],
                ret: Type::Integer,
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_atomic_i64_new"),
            FunctionSig {
                params: vec![(SmolStr::new("initial"), Type::Integer)],
                ret: Type::Integer,
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_atomic_i64_drop"),
            FunctionSig {
                params: vec![(SmolStr::new("atomic"), Type::Integer)],
                ret: Type::Boolean,
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_atomic_i64_load"),
            FunctionSig {
                params: vec![(SmolStr::new("atomic"), Type::Integer)],
                ret: Type::Integer,
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_atomic_i64_store"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("atomic"), Type::Integer),
                    (SmolStr::new("value"), Type::Integer),
                ],
                ret: Type::Boolean,
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_atomic_i64_fetch_add"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("atomic"), Type::Integer),
                    (SmolStr::new("delta"), Type::Integer),
                ],
                ret: Type::Integer,
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_pool_size"),
            FunctionSig {
                params: vec![(SmolStr::new("handle"), Type::Unknown)],
                ret: Type::Integer,
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_pool_rr"),
            FunctionSig {
                params: vec![(SmolStr::new("handle"), Type::Unknown)],
                ret: Type::Integer,
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_pool_queue_len"),
            FunctionSig {
                params: vec![(SmolStr::new("handle"), Type::Unknown)],
                ret: Type::Integer,
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_actor_mailbox_len"),
            FunctionSig {
                params: vec![(SmolStr::new("handle"), Type::Unknown)],
                ret: Type::Integer,
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_actor_pause"),
            FunctionSig {
                params: vec![(SmolStr::new("handle"), Type::Unknown)],
                ret: Type::Nil,
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_actor_resume"),
            FunctionSig {
                params: vec![(SmolStr::new("handle"), Type::Unknown)],
                ret: Type::Nil,
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_actor_pause_wait"),
            FunctionSig {
                params: vec![(SmolStr::new("handle"), Type::Unknown)],
                ret: Type::Nil,
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_actor_fire_burst_begin"),
            FunctionSig {
                params: vec![(SmolStr::new("handle"), Type::Unknown)],
                ret: Type::Nil,
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_actor_fire_burst_end"),
            FunctionSig {
                params: vec![(SmolStr::new("handle"), Type::Unknown)],
                ret: Type::Nil,
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_actor_fire_burst_abort"),
            FunctionSig {
                params: vec![(SmolStr::new("handle"), Type::Unknown)],
                ret: Type::Nil,
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_metrics_get"),
            FunctionSig {
                params: vec![(SmolStr::new("id"), Type::Integer)],
                ret: Type::Integer,
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_metrics_dropped_paused_id"),
            FunctionSig {
                params: vec![],
                ret: Type::Integer,
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_metrics_messages_dropped_id"),
            FunctionSig {
                params: vec![],
                ret: Type::Integer,
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_clock_ns"),
            FunctionSig {
                params: vec![],
                ret: Type::Integer,
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_sleep_ms"),
            FunctionSig {
                params: vec![(SmolStr::new("ms"), Type::Integer)],
                ret: Type::Pending(Box::new(Type::Nil)),
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_env_get"),
            FunctionSig {
                params: vec![(SmolStr::new("key"), Type::String)],
                ret: Type::Unknown,
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_env_set"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("key"), Type::String),
                    (SmolStr::new("value"), Type::String),
                ],
                ret: Type::Boolean,
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_runtime_configure"),
            FunctionSig {
                params: vec![(SmolStr::new("config"), Type::Unknown)],
                ret: Type::Nil,
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_db_open"),
            FunctionSig {
                params: vec![(SmolStr::new("path"), Type::String)],
                ret: Type::Integer,
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_db_close"),
            FunctionSig {
                params: vec![(SmolStr::new("handle"), Type::Integer)],
                ret: Type::Boolean,
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_db_submit_batch"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("handle"), Type::Integer),
                    (SmolStr::new("namespace"), Type::String),
                    (SmolStr::new("key"), Type::String),
                    (SmolStr::new("value"), Type::String),
                    (SmolStr::new("expected_version"), Type::Unknown),
                ],
                ret: Type::Integer,
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_db_read_point"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("handle"), Type::Integer),
                    (SmolStr::new("namespace"), Type::String),
                    (SmolStr::new("key"), Type::String),
                ],
                ret: Type::Unknown,
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_db_read_range"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("handle"), Type::Integer),
                    (SmolStr::new("namespace"), Type::String),
                    (SmolStr::new("start_key"), Type::String),
                    (SmolStr::new("end_key"), Type::String),
                    (SmolStr::new("limit"), Type::Integer),
                ],
                ret: Type::List(Box::new(Type::Unknown)),
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_db_txn_begin"),
            FunctionSig {
                params: vec![(SmolStr::new("handle"), Type::Integer)],
                ret: Type::Integer,
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_db_txn_prepare"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("handle"), Type::Integer),
                    (SmolStr::new("txn"), Type::Integer),
                ],
                ret: Type::Boolean,
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_db_txn_commit"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("handle"), Type::Integer),
                    (SmolStr::new("txn"), Type::Integer),
                ],
                ret: Type::Boolean,
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_db_txn_abort"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("handle"), Type::Integer),
                    (SmolStr::new("txn"), Type::Integer),
                ],
                ret: Type::Boolean,
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_db_snapshot_start"),
            FunctionSig {
                params: vec![(SmolStr::new("handle"), Type::Integer)],
                ret: Type::Integer,
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_db_snapshot_status"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("handle"), Type::Integer),
                    (SmolStr::new("snapshot"), Type::Integer),
                ],
                ret: Type::Integer,
                kind: FunctionKind::Function,
            },
        ),
        (
            SmolStr::new("__wr_db_restore"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("handle"), Type::Integer),
                    (SmolStr::new("snapshot"), Type::Integer),
                ],
                ret: Type::Boolean,
                kind: FunctionKind::Function,
            },
        ),
    ]
}

fn is_pool_of_call(body: &Body, callee: Idx<Expr>) -> bool {
    match &body.exprs[callee] {
        Expr::Member { object, member, .. } => is_pool_of_member(body, *object, member),
        _ => false,
    }
}

fn is_pool_of_member(body: &Body, object: Idx<Expr>, member: &SmolStr) -> bool {
    if member.as_str() != "of" {
        return false;
    }
    matches!(&body.exprs[object], Expr::Variable(name) if name.as_str() == "Pool")
}

fn pool_of_class_name(
    body: &Body,
    args: &[crate::hir::Arg],
    classes: &ClassIndex,
) -> Option<SmolStr> {
    for arg in args {
        if let crate::hir::Arg::Positional { value, .. } = arg {
            if let Expr::Variable(name) = &body.exprs[*value]
                && classes.is_class(name)
            {
                return Some(name.clone());
            }
            break;
        }
    }
    None
}

struct TypeContext {
    scopes: Vec<HashMap<SmolStr, Type>>,
    type_params: Vec<HashSet<SmolStr>>,
    info: Option<*mut FunctionTypeInfo>,
}

impl TypeContext {
    fn with_info(info: &mut FunctionTypeInfo) -> Self {
        Self {
            scopes: Vec::new(),
            type_params: Vec::new(),
            info: Some(info as *mut FunctionTypeInfo),
        }
    }

    fn enter_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn exit_scope(&mut self) {
        self.scopes.pop();
    }

    fn enter_type_params(&mut self, params: &[SmolStr]) {
        let set = params.iter().cloned().collect();
        self.type_params.push(set);
    }

    fn exit_type_params(&mut self) {
        self.type_params.pop();
    }

    fn declare(&mut self, name: SmolStr, ty: Type) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.clone(), ty.clone());
        }
        if let Some(info) = self.info {
            unsafe {
                let entry = (*info).local_types.entry(name).or_insert(Type::Unknown);
                if matches!(entry, Type::Unknown) && !matches!(ty, Type::Unknown) {
                    *entry = ty;
                }
            }
        }
    }

    fn resolve(&self, name: &SmolStr) -> Option<Type> {
        for scope in self.scopes.iter().rev() {
            if let Some(ty) = scope.get(name) {
                return Some(ty.clone());
            }
        }
        None
    }

    fn assign(&mut self, name: &SmolStr, ty: Type) {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(existing) = scope.get_mut(name) {
                if matches!(existing, Type::Unknown) && !matches!(ty, Type::Unknown) {
                    *existing = ty.clone();
                }
                if let Some(info) = self.info {
                    unsafe {
                        let entry = (*info)
                            .local_types
                            .entry(name.clone())
                            .or_insert(Type::Unknown);
                        if matches!(entry, Type::Unknown) && !matches!(ty, Type::Unknown) {
                            *entry = ty.clone();
                        }
                    }
                }
                return;
            }
        }
    }

    fn record_expr(&mut self, expr_id: Idx<Expr>, ty: Type) {
        if let Some(info) = self.info {
            unsafe {
                (*info).expr_types.insert(expr_id.into_raw(), ty);
            }
        }
    }
}

fn check_stmt(
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
        Stmt::Assert { kind, expr } => {
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
            iterable,
            body: loop_body,
            ..
        } => {
            infer_expr(
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
            for case in cases {
                ctx.enter_scope();
                for label in &case.labels {
                    bind_pattern(label, &subject_ty, ctx, enums);
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
            }
            if otherwise.is_none() && !match_is_exhaustive(&subject_ty, cases, enums) {
                errors.push(TypeError::MatchNonExhaustive {
                    span: span_from_range(body.stmt_span(stmt_id)),
                });
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
change the return type to Result[...] or handle results with `otherwise`."
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

fn bind_pattern(pattern: &Pattern, subject_ty: &Type, ctx: &mut TypeContext, enums: &EnumIndex) {
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
    }
}

fn match_is_exhaustive(
    subject_ty: &Type,
    cases: &[crate::hir::MatchCase],
    enums: &EnumIndex,
) -> bool {
    let mut has_wildcard = false;
    let mut ok_covered = false;
    let mut err_covered = false;
    let mut enum_name: Option<&SmolStr> = None;
    let mut enum_variants_total = 0usize;
    let mut enum_variants_covered: HashSet<SmolStr> = HashSet::new();

    if let Type::Named(name, _) = subject_ty {
        enum_name = Some(name);
        if let Some(en) = enums.get(name) {
            enum_variants_total = en.variants.len();
        }
    }

    for case in cases {
        for label in &case.labels {
            match label {
                Pattern::Wildcard | Pattern::Binding(_) => {
                    has_wildcard = true;
                }
                Pattern::Path { parts, .. } => {
                    if parts.len() == 1 && parts[0].as_str() == "Ok" {
                        ok_covered = true;
                    } else if parts.len() == 1 && parts[0].as_str() == "Err" {
                        err_covered = true;
                    } else if parts.len() == 2
                        && let Some(en) = enum_name
                        && parts[0] == *en
                    {
                        enum_variants_covered.insert(parts[1].clone());
                    }
                }
                Pattern::Literal(_) => {}
            }
        }
    }

    if has_wildcard {
        return true;
    }

    match subject_ty {
        Type::Result(_, _) => ok_covered && err_covered,
        Type::Named(name, _) => {
            enums.get(name).is_some()
                && enum_variants_total > 0
                && enum_variants_covered.len() == enum_variants_total
        }
        _ => false,
    }
}

#[derive(Debug, Clone, Copy)]
enum AssertEqualityMode {
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

fn check_assert_expr(
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
                allow_result,
                in_result_fn,
            );
            if matches!(op, UnaryOp::Err) && !in_result_fn {
                errors.push(TypeError::ErrOutsideResult {
                    span: span_from_range(*op_span),
                });
                Type::Unknown
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
        Expr::Call { .. } | Expr::GivenCall { .. } => {
            let (callee, args, type_args, is_given) = match &body.exprs[expr_id] {
                Expr::Call {
                    callee,
                    args,
                    type_args,
                } => (callee, args, type_args, false),
                Expr::GivenCall {
                    callee,
                    args,
                    type_args,
                } => (callee, args, type_args, true),
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
                if is_given {
                    errors.push(TypeError::GivenRequiresCheck {
                        span: span_from_range(body.expr_span(expr_id)),
                    });
                }
            }
            if let Expr::Variable(name) = &body.exprs[*callee] {
                if classes.is_class(name) {
                    if is_given {
                        errors.push(TypeError::GivenRequiresCheck {
                            span: span_from_range(body.expr_span(expr_id)),
                        });
                    }
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
                    if function.kind == FunctionKind::Check && !is_given {
                        errors.push(TypeError::CheckRequiresGiven {
                            span: span_from_range(body.expr_span(expr_id)),
                        });
                    }
                    if function.kind != FunctionKind::Check && is_given {
                        errors.push(TypeError::GivenRequiresCheck {
                            span: span_from_range(body.expr_span(expr_id)),
                        });
                    }
                    if !type_args.is_empty() {
                        errors.push(TypeError::UnexpectedTypeArgs {
                            span: span_from_range(body.expr_span(expr_id)),
                        });
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
                    if is_given {
                        errors.push(TypeError::GivenRequiresCheck {
                            span: span_from_range(body.expr_span(expr_id)),
                        });
                    }
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
                    match object_ty {
                        Type::Actor(inner) => {
                            if let Type::Named(class_name, class_args) = *inner
                                && let Some(class) = classes.get(&class_name)
                            {
                                let method_params =
                                    instantiate_method_params(class, &class_args, member);
                                let method_ret = instantiate_method_ret(class, &class_args, member);
                                if let Some(method) = class.methods.get(member) {
                                    if method.kind == FunctionKind::Derived {
                                        errors.push(TypeError::CallDerivedProperty {
                                            member: member.clone(),
                                            span: span_from_range(*member_span),
                                        });
                                        ret_ty = Some(Type::Unknown);
                                        valid_callee = true;
                                    } else if method.kind == FunctionKind::CheckMethod && !is_given
                                    {
                                        errors.push(TypeError::CheckRequiresGiven {
                                            span: span_from_range(body.expr_span(expr_id)),
                                        });
                                        ret_ty = Some(Type::Unknown);
                                        valid_callee = true;
                                    } else if method.kind != FunctionKind::CheckMethod && is_given {
                                        errors.push(TypeError::GivenRequiresCheck {
                                            span: span_from_range(body.expr_span(expr_id)),
                                        });
                                        ret_ty = Some(Type::Unknown);
                                        valid_callee = true;
                                    } else {
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
                                            allow_result,
                                            in_result_fn,
                                        );
                                        let ret = method_ret.unwrap_or(method.ret.clone());
                                        ret_ty = Some(Type::Pending(Box::new(Type::Result(
                                            Box::new(ret),
                                            Box::new(error_type()),
                                        ))));
                                        valid_callee = true;
                                    }
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
                                    if method.kind == InterfaceMethodKind::Check && !is_given {
                                        errors.push(TypeError::CheckRequiresGiven {
                                            span: span_from_range(body.expr_span(expr_id)),
                                        });
                                        ret_ty = Some(Type::Unknown);
                                        valid_callee = true;
                                    } else if method.kind != InterfaceMethodKind::Check && is_given
                                    {
                                        errors.push(TypeError::GivenRequiresCheck {
                                            span: span_from_range(body.expr_span(expr_id)),
                                        });
                                        ret_ty = Some(Type::Unknown);
                                        valid_callee = true;
                                    } else {
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
                                            allow_result,
                                            in_result_fn,
                                        );
                                        ret_ty = Some(method.ret.clone());
                                        valid_callee = true;
                                    }
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
                                let method_ret = instantiate_method_ret(class, &class_args, member);
                                if let Some(method) = class.methods.get(member) {
                                    if method.kind == FunctionKind::Derived {
                                        errors.push(TypeError::CallDerivedProperty {
                                            member: member.clone(),
                                            span: span_from_range(*member_span),
                                        });
                                        ret_ty = Some(Type::Unknown);
                                        valid_callee = true;
                                    } else if method.kind == FunctionKind::CheckMethod && !is_given
                                    {
                                        errors.push(TypeError::CheckRequiresGiven {
                                            span: span_from_range(body.expr_span(expr_id)),
                                        });
                                        ret_ty = Some(Type::Unknown);
                                        valid_callee = true;
                                    } else if method.kind != FunctionKind::CheckMethod && is_given {
                                        errors.push(TypeError::GivenRequiresCheck {
                                            span: span_from_range(body.expr_span(expr_id)),
                                        });
                                        ret_ty = Some(Type::Unknown);
                                        valid_callee = true;
                                    } else {
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
                                            allow_result,
                                            in_result_fn,
                                        );
                                        ret_ty = Some(method_ret.unwrap_or(method.ret.clone()));
                                        valid_callee = true;
                                    }
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
                        if method.kind == FunctionKind::Derived {
                            result = substitute_type(&method.ret, &subst);
                        } else {
                            result = Type::Unknown;
                        }
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
            help: "Handle with `otherwise`, `match`, `ignore result`, `capture`, or return the \
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
    allow_result: bool,
    in_result_fn: bool,
) {
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

fn check_class_init_args(
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

fn infer_list(
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
    element_type
        .map(|ty| Type::List(Box::new(ty)))
        .unwrap_or(Type::Unknown)
}

fn infer_map(
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

fn literal_type(lit: &Literal) -> Type {
    match lit {
        Literal::Integer(_) => Type::Integer,
        Literal::Float(_) => Type::Float,
        Literal::Boolean(_) => Type::Boolean,
        Literal::String(_) => Type::String,
        Literal::Nil => Type::Nil,
    }
}

fn error_type() -> Type {
    Type::Named(SmolStr::new("Error"), Vec::new())
}

fn type_from_ref(ty: &TypeRef) -> Type {
    type_from_ref_with_params(ty, &HashSet::new())
}

fn type_from_ref_in_ctx(ty: &TypeRef, ctx: &TypeContext) -> Type {
    if ctx.type_params.is_empty() {
        return type_from_ref(ty);
    }
    let mut params = HashSet::new();
    for scope in &ctx.type_params {
        params.extend(scope.iter().cloned());
    }
    type_from_ref_with_params(ty, &params)
}

fn type_from_ref_with_params(ty: &TypeRef, params: &HashSet<SmolStr>) -> Type {
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
        "Any" => Type::Unknown,
        "Float" => Type::Float,
        "Number" => Type::Number,
        "Boolean" => Type::Boolean,
        "String" => Type::String,
        "Nothing" => Type::Nil,
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

fn build_type_subst(params: &[SmolStr], args: &[Type]) -> HashMap<SmolStr, Type> {
    let mut subst = HashMap::new();
    for (idx, name) in params.iter().enumerate() {
        if let Some(arg) = args.get(idx) {
            subst.insert(name.clone(), arg.clone());
        }
    }
    subst
}

fn substitute_type(ty: &Type, subst: &HashMap<SmolStr, Type>) -> Type {
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

fn resolve_type_args(
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
        UnaryOp::Resolve => is_stored_boolean(operand),
        UnaryOp::Err => !matches!(operand, Type::Never),
        UnaryOp::Await | UnaryOp::Spawn | UnaryOp::Fire => true,
    }
}

fn unary_result(op: UnaryOp, operand: &Type) -> Type {
    match op {
        UnaryOp::Neg => operand.clone(),
        UnaryOp::Not => Type::Boolean,
        UnaryOp::BitNot => Type::Integer,
        UnaryOp::Resolve => Type::Boolean,
        UnaryOp::Err => Type::Result(Box::new(Type::Unknown), Box::new(operand.clone())),
        UnaryOp::Await | UnaryOp::Spawn | UnaryOp::Fire => Type::Unknown,
    }
}

fn is_stored_boolean(operand: &Type) -> bool {
    matches!(operand, Type::Named(name, args) if name.as_str() == "StoredBoolean" && args.is_empty())
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
    }
}

fn unary_op_label(op: UnaryOp) -> &'static str {
    match op {
        UnaryOp::Neg => "-",
        UnaryOp::Not => "not",
        UnaryOp::BitNot => "~",
        UnaryOp::Resolve => "resolve",
        UnaryOp::Await => "await",
        UnaryOp::Spawn => "spawn",
        UnaryOp::Fire => "fire",
        UnaryOp::Err => "error",
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
        BinaryOp::Otherwise => "otherwise",
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
        Expr::GivenCall { callee, args, .. } => {
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

fn check_async_actor_usage(
    module: &Module,
    info: &TypeInfo,
    classes: &ClassIndex,
    errors: &mut Vec<TypeError>,
) {
    let (function_by_name, class_method_ids) = build_call_maps(module);
    let func_labels = build_func_labels(module, &class_method_ids);
    let mut direct_await = HashMap::new();
    let mut sync_calls: HashMap<usize, Vec<Idx<Function>>> = HashMap::new();
    let mut cause: HashMap<usize, Option<Idx<Function>>> = HashMap::new();

    for (func_id, func) in module.functions.iter() {
        let Some(body) = &func.body else {
            continue;
        };
        let fn_info = info.function(func_id);
        let mut has_await = false;
        let mut calls = Vec::new();
        collect_direct_await_and_sync_calls(
            body,
            fn_info,
            &function_by_name,
            &class_method_ids,
            &mut has_await,
            &mut calls,
        );
        direct_await.insert(func_id.into_raw(), has_await);
        if has_await {
            cause.insert(func_id.into_raw(), None);
        }
        sync_calls.insert(func_id.into_raw(), calls);
    }

    let mut requires_actor = direct_await
        .iter()
        .map(|(id, val)| (*id, *val))
        .collect::<HashMap<_, _>>();
    loop {
        let mut changed = false;
        for (func_id, _) in module.functions.iter() {
            let id = func_id.into_raw();
            if *requires_actor.get(&id).unwrap_or(&false) {
                continue;
            }
            let Some(calls) = sync_calls.get(&id) else {
                continue;
            };
            if let Some(callee) = calls
                .iter()
                .find(|callee| *requires_actor.get(&callee.into_raw()).unwrap_or(&false))
            {
                requires_actor.insert(id, true);
                cause.entry(id).or_insert(Some(*callee));
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    let mut class_requires_actor = HashMap::new();
    let mut class_trace = HashMap::new();
    for (_idx, class) in module.classes.iter() {
        let needs_actor = class
            .methods
            .iter()
            .any(|method_id| *requires_actor.get(&method_id.into_raw()).unwrap_or(&false));
        class_requires_actor.insert(class.name.clone(), needs_actor);
        if needs_actor
            && let Some(method_id) = class
                .methods
                .iter()
                .find(|method_id| *requires_actor.get(&method_id.into_raw()).unwrap_or(&false))
        {
            let trace = build_call_chain(
                *method_id,
                &cause,
                &func_labels,
                "Use `detach` or `Pool.of(...)` to create an actor instance.",
            );
            class_trace.insert(class.name.clone(), trace);
        }
    }

    for (func_id, func) in module.functions.iter() {
        let Some(body) = &func.body else {
            continue;
        };
        let fn_info = info.function(func_id);
        check_body_async_usage(
            body,
            fn_info,
            classes,
            &class_method_ids,
            &requires_actor,
            &class_requires_actor,
            &class_trace,
            &cause,
            &func_labels,
            errors,
        );
    }
}

fn build_call_maps(
    module: &Module,
) -> (
    HashMap<SmolStr, Idx<Function>>,
    HashMap<SmolStr, HashMap<SmolStr, Idx<Function>>>,
) {
    let mut method_ids = HashSet::new();
    for (_idx, class) in module.classes.iter() {
        for method_id in &class.methods {
            method_ids.insert(method_id.into_raw());
        }
    }

    let mut function_by_name = HashMap::new();
    for (idx, func) in module.functions.iter() {
        if method_ids.contains(&idx.into_raw()) {
            continue;
        }
        function_by_name.insert(func.name.clone(), idx);
    }

    let mut class_method_ids: HashMap<SmolStr, HashMap<SmolStr, Idx<Function>>> = HashMap::new();
    for (_idx, class) in module.classes.iter() {
        let mut methods = HashMap::new();
        for method_id in &class.methods {
            let method = &module.functions[*method_id];
            methods.insert(method.name.clone(), *method_id);
        }
        class_method_ids.insert(class.name.clone(), methods);
    }

    (function_by_name, class_method_ids)
}

fn build_func_labels(
    module: &Module,
    class_method_ids: &HashMap<SmolStr, HashMap<SmolStr, Idx<Function>>>,
) -> HashMap<usize, String> {
    let mut labels = HashMap::new();
    for (class_name, methods) in class_method_ids {
        for (method_name, method_id) in methods {
            labels.insert(
                method_id.into_raw(),
                format!("{}.{}", class_name, method_name),
            );
        }
    }
    for (func_id, func) in module.functions.iter() {
        labels
            .entry(func_id.into_raw())
            .or_insert_with(|| func.name.to_string());
    }
    labels
}

fn build_call_chain(
    start: Idx<Function>,
    cause: &HashMap<usize, Option<Idx<Function>>>,
    func_labels: &HashMap<usize, String>,
    hint: &str,
) -> String {
    let mut parts = Vec::new();
    let mut current = Some(start);
    let mut visited = HashSet::new();
    while let Some(func_id) = current {
        if !visited.insert(func_id.into_raw()) {
            break;
        }
        let label = func_labels
            .get(&func_id.into_raw())
            .cloned()
            .unwrap_or_else(|| "<unknown>".to_string());
        parts.push(label);
        current = *cause.get(&func_id.into_raw()).unwrap_or(&None);
    }
    parts.push("await".to_string());
    format!("{hint} Async call chain: {}.", parts.join(" -> "))
}

fn collect_direct_await_and_sync_calls(
    body: &Body,
    fn_info: Option<&FunctionTypeInfo>,
    function_by_name: &HashMap<SmolStr, Idx<Function>>,
    class_method_ids: &HashMap<SmolStr, HashMap<SmolStr, Idx<Function>>>,
    has_await: &mut bool,
    calls: &mut Vec<Idx<Function>>,
) {
    for stmt_id in &body.root_stmts {
        visit_stmt_for_async(
            body,
            *stmt_id,
            fn_info,
            function_by_name,
            class_method_ids,
            has_await,
            calls,
        );
    }
}

fn visit_stmt_for_async(
    body: &Body,
    stmt_id: Idx<Stmt>,
    fn_info: Option<&FunctionTypeInfo>,
    function_by_name: &HashMap<SmolStr, Idx<Function>>,
    class_method_ids: &HashMap<SmolStr, HashMap<SmolStr, Idx<Function>>>,
    has_await: &mut bool,
    calls: &mut Vec<Idx<Function>>,
) {
    match &body.stmts[stmt_id] {
        Stmt::Expr(expr) => visit_expr_for_async(
            body,
            *expr,
            fn_info,
            function_by_name,
            class_method_ids,
            has_await,
            calls,
        ),
        Stmt::Assert { expr, .. } => visit_expr_for_async(
            body,
            *expr,
            fn_info,
            function_by_name,
            class_method_ids,
            has_await,
            calls,
        ),
        Stmt::Require { condition, message } => {
            visit_expr_for_async(
                body,
                *condition,
                fn_info,
                function_by_name,
                class_method_ids,
                has_await,
                calls,
            );
            visit_expr_for_async(
                body,
                *message,
                fn_info,
                function_by_name,
                class_method_ids,
                has_await,
                calls,
            );
        }
        Stmt::Let { value, .. } | Stmt::Assign { value, .. } | Stmt::Capture { value, .. } => {
            visit_expr_for_async(
                body,
                *value,
                fn_info,
                function_by_name,
                class_method_ids,
                has_await,
                calls,
            )
        }
        Stmt::Defer { expr } => visit_expr_for_async(
            body,
            *expr,
            fn_info,
            function_by_name,
            class_method_ids,
            has_await,
            calls,
        ),
        Stmt::IgnoreResult { expr } => visit_expr_for_async(
            body,
            *expr,
            fn_info,
            function_by_name,
            class_method_ids,
            has_await,
            calls,
        ),
        Stmt::Optimize { body: stmts, .. } | Stmt::While { body: stmts, .. } => {
            for stmt in stmts {
                visit_stmt_for_async(
                    body,
                    *stmt,
                    fn_info,
                    function_by_name,
                    class_method_ids,
                    has_await,
                    calls,
                );
            }
        }
        Stmt::If {
            condition,
            then_branch,
            else_branch,
        } => {
            visit_expr_for_async(
                body,
                *condition,
                fn_info,
                function_by_name,
                class_method_ids,
                has_await,
                calls,
            );
            for stmt in then_branch {
                visit_stmt_for_async(
                    body,
                    *stmt,
                    fn_info,
                    function_by_name,
                    class_method_ids,
                    has_await,
                    calls,
                );
            }
            if let Some(stmts) = else_branch {
                for stmt in stmts {
                    visit_stmt_for_async(
                        body,
                        *stmt,
                        fn_info,
                        function_by_name,
                        class_method_ids,
                        has_await,
                        calls,
                    );
                }
            }
        }
        Stmt::For {
            iterable,
            body: stmts,
            ..
        } => {
            visit_expr_for_async(
                body,
                *iterable,
                fn_info,
                function_by_name,
                class_method_ids,
                has_await,
                calls,
            );
            for stmt in stmts {
                visit_stmt_for_async(
                    body,
                    *stmt,
                    fn_info,
                    function_by_name,
                    class_method_ids,
                    has_await,
                    calls,
                );
            }
        }
        Stmt::Match {
            subject,
            cases,
            otherwise,
        } => {
            visit_expr_for_async(
                body,
                *subject,
                fn_info,
                function_by_name,
                class_method_ids,
                has_await,
                calls,
            );
            for case in cases {
                for stmt in &case.body {
                    visit_stmt_for_async(
                        body,
                        *stmt,
                        fn_info,
                        function_by_name,
                        class_method_ids,
                        has_await,
                        calls,
                    );
                }
            }
            if let Some(stmts) = otherwise {
                for stmt in stmts {
                    visit_stmt_for_async(
                        body,
                        *stmt,
                        fn_info,
                        function_by_name,
                        class_method_ids,
                        has_await,
                        calls,
                    );
                }
            }
        }
        Stmt::Return(expr) => {
            if let Some(expr) = expr {
                visit_expr_for_async(
                    body,
                    *expr,
                    fn_info,
                    function_by_name,
                    class_method_ids,
                    has_await,
                    calls,
                );
            }
        }
        Stmt::Use { .. } | Stmt::Break | Stmt::Continue => {}
    }
}

fn visit_expr_for_async(
    body: &Body,
    expr_id: Idx<Expr>,
    fn_info: Option<&FunctionTypeInfo>,
    function_by_name: &HashMap<SmolStr, Idx<Function>>,
    class_method_ids: &HashMap<SmolStr, HashMap<SmolStr, Idx<Function>>>,
    has_await: &mut bool,
    calls: &mut Vec<Idx<Function>>,
) {
    match &body.exprs[expr_id] {
        Expr::Unary { op, expr, .. } => {
            if matches!(op, UnaryOp::Await | UnaryOp::Fire) {
                *has_await = true;
            }
            visit_expr_for_async(
                body,
                *expr,
                fn_info,
                function_by_name,
                class_method_ids,
                has_await,
                calls,
            );
        }
        Expr::TypeApply { callee, .. } => {
            visit_expr_for_async(
                body,
                *callee,
                fn_info,
                function_by_name,
                class_method_ids,
                has_await,
                calls,
            );
        }
        Expr::Call { callee, args, .. } => {
            if let Expr::Variable(name) = &body.exprs[*callee] {
                if let Some(target) = function_by_name.get(name) {
                    calls.push(*target);
                }
            } else if let Expr::Member { object, member, .. } = &body.exprs[*callee]
                && let Some(fn_info) = fn_info
                && let Some(obj_ty) = fn_info.expr_types.get(&object.into_raw())
                && let Type::Named(class_name, _) = obj_ty
                && let Some(methods) = class_method_ids.get(class_name)
                && let Some(method_id) = methods.get(member)
            {
                calls.push(*method_id);
            }
            visit_expr_for_async(
                body,
                *callee,
                fn_info,
                function_by_name,
                class_method_ids,
                has_await,
                calls,
            );
            for arg in args {
                match arg {
                    crate::hir::Arg::Positional { value, .. } => visit_expr_for_async(
                        body,
                        *value,
                        fn_info,
                        function_by_name,
                        class_method_ids,
                        has_await,
                        calls,
                    ),
                    crate::hir::Arg::Named { value, .. } => visit_expr_for_async(
                        body,
                        *value,
                        fn_info,
                        function_by_name,
                        class_method_ids,
                        has_await,
                        calls,
                    ),
                }
            }
        }
        Expr::GivenCall { callee, args, .. } => {
            if let Expr::Variable(name) = &body.exprs[*callee] {
                if let Some(target) = function_by_name.get(name) {
                    calls.push(*target);
                }
            } else if let Expr::Member { object, member, .. } = &body.exprs[*callee]
                && let Some(fn_info) = fn_info
                && let Some(obj_ty) = fn_info.expr_types.get(&object.into_raw())
                && let Type::Named(class_name, _) = obj_ty
                && let Some(methods) = class_method_ids.get(class_name)
                && let Some(method_id) = methods.get(member)
            {
                calls.push(*method_id);
            }
            visit_expr_for_async(
                body,
                *callee,
                fn_info,
                function_by_name,
                class_method_ids,
                has_await,
                calls,
            );
            for arg in args {
                match arg {
                    crate::hir::Arg::Positional { value, .. } => visit_expr_for_async(
                        body,
                        *value,
                        fn_info,
                        function_by_name,
                        class_method_ids,
                        has_await,
                        calls,
                    ),
                    crate::hir::Arg::Named { value, .. } => visit_expr_for_async(
                        body,
                        *value,
                        fn_info,
                        function_by_name,
                        class_method_ids,
                        has_await,
                        calls,
                    ),
                }
            }
        }
        Expr::Binary { lhs, rhs, .. } => {
            visit_expr_for_async(
                body,
                *lhs,
                fn_info,
                function_by_name,
                class_method_ids,
                has_await,
                calls,
            );
            visit_expr_for_async(
                body,
                *rhs,
                fn_info,
                function_by_name,
                class_method_ids,
                has_await,
                calls,
            );
        }
        Expr::Crash { expr } => visit_expr_for_async(
            body,
            *expr,
            fn_info,
            function_by_name,
            class_method_ids,
            has_await,
            calls,
        ),
        Expr::Detach { target, .. } => visit_expr_for_async(
            body,
            *target,
            fn_info,
            function_by_name,
            class_method_ids,
            has_await,
            calls,
        ),
        Expr::Member { object, .. } => visit_expr_for_async(
            body,
            *object,
            fn_info,
            function_by_name,
            class_method_ids,
            has_await,
            calls,
        ),
        Expr::List(items) => {
            for item in items {
                visit_expr_for_async(
                    body,
                    *item,
                    fn_info,
                    function_by_name,
                    class_method_ids,
                    has_await,
                    calls,
                );
            }
        }
        Expr::Map(items) => {
            for (key, value) in items {
                visit_expr_for_async(
                    body,
                    *key,
                    fn_info,
                    function_by_name,
                    class_method_ids,
                    has_await,
                    calls,
                );
                visit_expr_for_async(
                    body,
                    *value,
                    fn_info,
                    function_by_name,
                    class_method_ids,
                    has_await,
                    calls,
                );
            }
        }
        Expr::StringInterp(parts) => {
            for part in parts {
                if let crate::hir::StringPart::Expr(expr) = part {
                    visit_expr_for_async(
                        body,
                        *expr,
                        fn_info,
                        function_by_name,
                        class_method_ids,
                        has_await,
                        calls,
                    );
                }
            }
        }
        Expr::Literal(_) | Expr::Variable(_) => {}
    }
}

fn check_body_async_usage(
    body: &Body,
    fn_info: Option<&FunctionTypeInfo>,
    classes: &ClassIndex,
    class_method_ids: &HashMap<SmolStr, HashMap<SmolStr, Idx<Function>>>,
    requires_actor: &HashMap<usize, bool>,
    class_requires_actor: &HashMap<SmolStr, bool>,
    class_trace: &HashMap<SmolStr, String>,
    cause: &HashMap<usize, Option<Idx<Function>>>,
    func_labels: &HashMap<usize, String>,
    errors: &mut Vec<TypeError>,
) {
    for stmt_id in &body.root_stmts {
        check_stmt_async_usage(
            body,
            *stmt_id,
            fn_info,
            classes,
            class_method_ids,
            requires_actor,
            class_requires_actor,
            class_trace,
            cause,
            func_labels,
            errors,
            false,
        );
    }
}

fn check_stmt_async_usage(
    body: &Body,
    stmt_id: Idx<Stmt>,
    fn_info: Option<&FunctionTypeInfo>,
    classes: &ClassIndex,
    class_method_ids: &HashMap<SmolStr, HashMap<SmolStr, Idx<Function>>>,
    requires_actor: &HashMap<usize, bool>,
    class_requires_actor: &HashMap<SmolStr, bool>,
    class_trace: &HashMap<SmolStr, String>,
    cause: &HashMap<usize, Option<Idx<Function>>>,
    func_labels: &HashMap<usize, String>,
    errors: &mut Vec<TypeError>,
    in_detach: bool,
) {
    match &body.stmts[stmt_id] {
        Stmt::Expr(expr) => check_expr_async_usage(
            body,
            *expr,
            fn_info,
            classes,
            class_method_ids,
            requires_actor,
            class_requires_actor,
            class_trace,
            cause,
            func_labels,
            errors,
            in_detach,
        ),
        Stmt::Assert { expr, .. } => {
            check_expr_async_usage(
                body,
                *expr,
                fn_info,
                classes,
                class_method_ids,
                requires_actor,
                class_requires_actor,
                class_trace,
                cause,
                func_labels,
                errors,
                in_detach,
            );
        }
        Stmt::Require { condition, message } => {
            check_expr_async_usage(
                body,
                *condition,
                fn_info,
                classes,
                class_method_ids,
                requires_actor,
                class_requires_actor,
                class_trace,
                cause,
                func_labels,
                errors,
                in_detach,
            );
            check_expr_async_usage(
                body,
                *message,
                fn_info,
                classes,
                class_method_ids,
                requires_actor,
                class_requires_actor,
                class_trace,
                cause,
                func_labels,
                errors,
                in_detach,
            );
        }
        Stmt::Let { value, .. } | Stmt::Assign { value, .. } | Stmt::Capture { value, .. } => {
            check_expr_async_usage(
                body,
                *value,
                fn_info,
                classes,
                class_method_ids,
                requires_actor,
                class_requires_actor,
                class_trace,
                cause,
                func_labels,
                errors,
                in_detach,
            )
        }
        Stmt::Defer { expr } => check_expr_async_usage(
            body,
            *expr,
            fn_info,
            classes,
            class_method_ids,
            requires_actor,
            class_requires_actor,
            class_trace,
            cause,
            func_labels,
            errors,
            in_detach,
        ),
        Stmt::IgnoreResult { expr } => check_expr_async_usage(
            body,
            *expr,
            fn_info,
            classes,
            class_method_ids,
            requires_actor,
            class_requires_actor,
            class_trace,
            cause,
            func_labels,
            errors,
            in_detach,
        ),
        Stmt::Optimize { body: stmts, .. } | Stmt::While { body: stmts, .. } => {
            for stmt in stmts {
                check_stmt_async_usage(
                    body,
                    *stmt,
                    fn_info,
                    classes,
                    class_method_ids,
                    requires_actor,
                    class_requires_actor,
                    class_trace,
                    cause,
                    func_labels,
                    errors,
                    in_detach,
                );
            }
        }
        Stmt::If {
            condition,
            then_branch,
            else_branch,
        } => {
            check_expr_async_usage(
                body,
                *condition,
                fn_info,
                classes,
                class_method_ids,
                requires_actor,
                class_requires_actor,
                class_trace,
                cause,
                func_labels,
                errors,
                in_detach,
            );
            for stmt in then_branch {
                check_stmt_async_usage(
                    body,
                    *stmt,
                    fn_info,
                    classes,
                    class_method_ids,
                    requires_actor,
                    class_requires_actor,
                    class_trace,
                    cause,
                    func_labels,
                    errors,
                    in_detach,
                );
            }
            if let Some(stmts) = else_branch {
                for stmt in stmts {
                    check_stmt_async_usage(
                        body,
                        *stmt,
                        fn_info,
                        classes,
                        class_method_ids,
                        requires_actor,
                        class_requires_actor,
                        class_trace,
                        cause,
                        func_labels,
                        errors,
                        in_detach,
                    );
                }
            }
        }
        Stmt::For {
            iterable,
            body: stmts,
            ..
        } => {
            check_expr_async_usage(
                body,
                *iterable,
                fn_info,
                classes,
                class_method_ids,
                requires_actor,
                class_requires_actor,
                class_trace,
                cause,
                func_labels,
                errors,
                in_detach,
            );
            for stmt in stmts {
                check_stmt_async_usage(
                    body,
                    *stmt,
                    fn_info,
                    classes,
                    class_method_ids,
                    requires_actor,
                    class_requires_actor,
                    class_trace,
                    cause,
                    func_labels,
                    errors,
                    in_detach,
                );
            }
        }
        Stmt::Match {
            subject,
            cases,
            otherwise,
        } => {
            check_expr_async_usage(
                body,
                *subject,
                fn_info,
                classes,
                class_method_ids,
                requires_actor,
                class_requires_actor,
                class_trace,
                cause,
                func_labels,
                errors,
                in_detach,
            );
            for case in cases {
                for stmt in &case.body {
                    check_stmt_async_usage(
                        body,
                        *stmt,
                        fn_info,
                        classes,
                        class_method_ids,
                        requires_actor,
                        class_requires_actor,
                        class_trace,
                        cause,
                        func_labels,
                        errors,
                        in_detach,
                    );
                }
            }
            if let Some(stmts) = otherwise {
                for stmt in stmts {
                    check_stmt_async_usage(
                        body,
                        *stmt,
                        fn_info,
                        classes,
                        class_method_ids,
                        requires_actor,
                        class_requires_actor,
                        class_trace,
                        cause,
                        func_labels,
                        errors,
                        in_detach,
                    );
                }
            }
        }
        Stmt::Return(expr) => {
            if let Some(expr) = expr {
                check_expr_async_usage(
                    body,
                    *expr,
                    fn_info,
                    classes,
                    class_method_ids,
                    requires_actor,
                    class_requires_actor,
                    class_trace,
                    cause,
                    func_labels,
                    errors,
                    in_detach,
                );
            }
        }
        Stmt::Use { .. } | Stmt::Break | Stmt::Continue => {}
    }
}

fn check_expr_async_usage(
    body: &Body,
    expr_id: Idx<Expr>,
    fn_info: Option<&FunctionTypeInfo>,
    classes: &ClassIndex,
    class_method_ids: &HashMap<SmolStr, HashMap<SmolStr, Idx<Function>>>,
    requires_actor: &HashMap<usize, bool>,
    class_requires_actor: &HashMap<SmolStr, bool>,
    class_trace: &HashMap<SmolStr, String>,
    cause: &HashMap<usize, Option<Idx<Function>>>,
    func_labels: &HashMap<usize, String>,
    errors: &mut Vec<TypeError>,
    in_detach: bool,
) {
    match &body.exprs[expr_id] {
        Expr::Call { callee, args, .. } => {
            if let Expr::Variable(name) = &body.exprs[*callee] {
                if !in_detach
                    && classes.is_class(name)
                    && class_requires_actor.get(name).copied().unwrap_or(false)
                {
                    errors.push(TypeError::AsyncClassRequiresActor {
                        class: name.clone(),
                        span: span_from_range(body.expr_span(*callee)),
                        help: class_trace.get(name).cloned().unwrap_or_else(|| {
                            "Use `detach` or `Pool.of(...)` to create an actor instance."
                                .to_string()
                        }),
                    });
                }
            } else if let Expr::Member {
                object,
                member,
                member_span,
            } = &body.exprs[*callee]
                && let Some(fn_info) = fn_info
                && let Some(obj_ty) = fn_info.expr_types.get(&object.into_raw())
                && let Type::Named(class_name, _) = obj_ty
                && let Some(methods) = class_method_ids.get(class_name)
                && let Some(method_id) = methods.get(member)
                && *requires_actor.get(&method_id.into_raw()).unwrap_or(&false)
            {
                let hint = "Call this method on a detached or pooled actor instance.";
                let trace = build_call_chain(*method_id, cause, func_labels, hint);
                errors.push(TypeError::AsyncMethodRequiresActor {
                    class: class_name.clone(),
                    member: member.clone(),
                    span: span_from_range(*member_span),
                    help: trace,
                });
            }
            check_expr_async_usage(
                body,
                *callee,
                fn_info,
                classes,
                class_method_ids,
                requires_actor,
                class_requires_actor,
                class_trace,
                cause,
                func_labels,
                errors,
                in_detach,
            );
            for arg in args {
                match arg {
                    crate::hir::Arg::Positional { value, .. } => check_expr_async_usage(
                        body,
                        *value,
                        fn_info,
                        classes,
                        class_method_ids,
                        requires_actor,
                        class_requires_actor,
                        class_trace,
                        cause,
                        func_labels,
                        errors,
                        in_detach,
                    ),
                    crate::hir::Arg::Named { value, .. } => check_expr_async_usage(
                        body,
                        *value,
                        fn_info,
                        classes,
                        class_method_ids,
                        requires_actor,
                        class_requires_actor,
                        class_trace,
                        cause,
                        func_labels,
                        errors,
                        in_detach,
                    ),
                }
            }
        }
        Expr::GivenCall { callee, args, .. } => {
            if let Expr::Variable(name) = &body.exprs[*callee] {
                if !in_detach
                    && classes.is_class(name)
                    && class_requires_actor.get(name).copied().unwrap_or(false)
                {
                    errors.push(TypeError::AsyncClassRequiresActor {
                        class: name.clone(),
                        span: span_from_range(body.expr_span(*callee)),
                        help: class_trace.get(name).cloned().unwrap_or_else(|| {
                            "Use `detach` or `Pool.of(...)` to create an actor instance."
                                .to_string()
                        }),
                    });
                }
            } else if let Expr::Member {
                object,
                member,
                member_span,
            } = &body.exprs[*callee]
                && let Some(fn_info) = fn_info
                && let Some(obj_ty) = fn_info.expr_types.get(&object.into_raw())
                && let Type::Named(class_name, _) = obj_ty
                && let Some(methods) = class_method_ids.get(class_name)
                && let Some(method_id) = methods.get(member)
                && *requires_actor.get(&method_id.into_raw()).unwrap_or(&false)
            {
                let hint = "Call this method on a detached or pooled actor instance.";
                let trace = build_call_chain(*method_id, cause, func_labels, hint);
                errors.push(TypeError::AsyncMethodRequiresActor {
                    class: class_name.clone(),
                    member: member.clone(),
                    span: span_from_range(*member_span),
                    help: trace,
                });
            }
            check_expr_async_usage(
                body,
                *callee,
                fn_info,
                classes,
                class_method_ids,
                requires_actor,
                class_requires_actor,
                class_trace,
                cause,
                func_labels,
                errors,
                in_detach,
            );
            for arg in args {
                match arg {
                    crate::hir::Arg::Positional { value, .. } => check_expr_async_usage(
                        body,
                        *value,
                        fn_info,
                        classes,
                        class_method_ids,
                        requires_actor,
                        class_requires_actor,
                        class_trace,
                        cause,
                        func_labels,
                        errors,
                        in_detach,
                    ),
                    crate::hir::Arg::Named { value, .. } => check_expr_async_usage(
                        body,
                        *value,
                        fn_info,
                        classes,
                        class_method_ids,
                        requires_actor,
                        class_requires_actor,
                        class_trace,
                        cause,
                        func_labels,
                        errors,
                        in_detach,
                    ),
                }
            }
        }
        Expr::TypeApply { callee, .. } => check_expr_async_usage(
            body,
            *callee,
            fn_info,
            classes,
            class_method_ids,
            requires_actor,
            class_requires_actor,
            class_trace,
            cause,
            func_labels,
            errors,
            in_detach,
        ),
        Expr::Unary { expr, .. } => check_expr_async_usage(
            body,
            *expr,
            fn_info,
            classes,
            class_method_ids,
            requires_actor,
            class_requires_actor,
            class_trace,
            cause,
            func_labels,
            errors,
            in_detach,
        ),
        Expr::Binary { lhs, rhs, .. } => {
            check_expr_async_usage(
                body,
                *lhs,
                fn_info,
                classes,
                class_method_ids,
                requires_actor,
                class_requires_actor,
                class_trace,
                cause,
                func_labels,
                errors,
                in_detach,
            );
            check_expr_async_usage(
                body,
                *rhs,
                fn_info,
                classes,
                class_method_ids,
                requires_actor,
                class_requires_actor,
                class_trace,
                cause,
                func_labels,
                errors,
                in_detach,
            );
        }
        Expr::Crash { expr } => check_expr_async_usage(
            body,
            *expr,
            fn_info,
            classes,
            class_method_ids,
            requires_actor,
            class_requires_actor,
            class_trace,
            cause,
            func_labels,
            errors,
            in_detach,
        ),
        Expr::Detach { target, .. } => check_expr_async_usage(
            body,
            *target,
            fn_info,
            classes,
            class_method_ids,
            requires_actor,
            class_requires_actor,
            class_trace,
            cause,
            func_labels,
            errors,
            true,
        ),
        Expr::Member { object, .. } => check_expr_async_usage(
            body,
            *object,
            fn_info,
            classes,
            class_method_ids,
            requires_actor,
            class_requires_actor,
            class_trace,
            cause,
            func_labels,
            errors,
            in_detach,
        ),
        Expr::List(items) => {
            for item in items {
                check_expr_async_usage(
                    body,
                    *item,
                    fn_info,
                    classes,
                    class_method_ids,
                    requires_actor,
                    class_requires_actor,
                    class_trace,
                    cause,
                    func_labels,
                    errors,
                    in_detach,
                );
            }
        }
        Expr::Map(items) => {
            for (key, value) in items {
                check_expr_async_usage(
                    body,
                    *key,
                    fn_info,
                    classes,
                    class_method_ids,
                    requires_actor,
                    class_requires_actor,
                    class_trace,
                    cause,
                    func_labels,
                    errors,
                    in_detach,
                );
                check_expr_async_usage(
                    body,
                    *value,
                    fn_info,
                    classes,
                    class_method_ids,
                    requires_actor,
                    class_requires_actor,
                    class_trace,
                    cause,
                    func_labels,
                    errors,
                    in_detach,
                );
            }
        }
        Expr::StringInterp(parts) => {
            for part in parts {
                if let crate::hir::StringPart::Expr(expr) = part {
                    check_expr_async_usage(
                        body,
                        *expr,
                        fn_info,
                        classes,
                        class_method_ids,
                        requires_actor,
                        class_requires_actor,
                        class_trace,
                        cause,
                        func_labels,
                        errors,
                        in_detach,
                    );
                }
            }
        }
        Expr::Literal(_) | Expr::Variable(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hir::lower::lower;
    use crate::parser::ast::AstNode;
    use crate::parser::{ast, parse};

    #[test]
    fn test_type_error_binary() {
        let input = "to f() -> Integer:\n    return 1 + true";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::InvalidBinaryOperands { .. }))
        );
    }

    #[test]
    fn test_type_error_unary() {
        let input = "to f() -> Boolean:\n    return not 1";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::InvalidUnaryOperand { .. }))
        );
    }

    #[test]
    fn test_resolve_requires_stored_boolean_operand() {
        let input = "to f() -> Boolean:\n    return resolve true";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.iter().any(
            |err| matches!(err, TypeError::InvalidUnaryOperand { op, .. } if *op == "resolve")
        ));
    }

    #[test]
    fn test_resolve_accepts_stored_boolean_value() {
        let input = "\
to fetch_flag() -> StoredBoolean:
    return true

to f() -> Boolean:
    flag = fetch_flag()
    return resolve flag
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_param_type_used() {
        let input = "to f(x: Integer) -> Integer:\n    return x + 1";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_param_type_mismatch() {
        let input = "to f(x: Integer) -> Integer:\n    return x + true";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::InvalidBinaryOperands { .. }))
        );
    }

    #[test]
    fn test_match_without_otherwise_all_variants_enum_ok() {
        let input = "\
A Status is either:
    Pending
    Done

to f(s: Status) -> Integer:
    match s:
        Status.Pending: return 1
        Status.Done: return 2
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_match_without_otherwise_non_exhaustive_enum_error() {
        let input = "\
A Status is either:
    Pending
    Done

to f(s: Status) -> Integer:
    match s:
        Status.Pending: return 1
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::MatchNonExhaustive { .. }))
        );
    }

    #[test]
    fn test_match_without_otherwise_ok_err_result_ok() {
        let input = "\
to f(r: Result[Integer]) -> Integer:
    match r:
        Ok(x): return x
        Err(_): return 0
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_match_without_otherwise_non_exhaustive_result_error() {
        let input = "\
to f(r: Result[Integer]) -> Integer:
    match r:
        Ok(x): return x
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::MatchNonExhaustive { .. }))
        );
    }

    #[test]
    fn test_string_concat_allowed() {
        let input = "to f() -> String:\n    return \"a\" + \"b\"";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_assignment_type_mismatch() {
        let input = "to f(x: String) -> Nothing:\n    x += 1";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::InvalidAssignment { .. }))
        );
    }

    #[test]
    fn test_return_type_mismatch() {
        let input = "to f() -> Boolean:\n    return 1";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::ReturnTypeMismatch { .. }))
        );
    }

    #[test]
    fn test_if_condition_must_be_boolean() {
        let input = "to f() -> Integer:\n    if 1:\n        return 1\n    return 0";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::IfConditionNotBoolean { .. }))
        );
    }

    #[test]
    fn test_while_condition_must_be_boolean() {
        let input = "to f() -> Integer:\n    while 1:\n        return 1\n    return 0";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::WhileConditionNotBoolean { .. }))
        );
    }

    #[test]
    fn test_logical_and_requires_boolean_rhs() {
        let input = "to f() -> Boolean:\n    flag = true\n    return flag and 1";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::InvalidBinaryOperands { .. }))
        );
    }

    #[test]
    fn test_field_access_type() {
        let input = "\
A Whale:\n    has:\n        name: String\n\nto f(w: Whale) -> String:\n    return w.name\n";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_unknown_member() {
        let input = "\
A Whale:\n    has:\n        name: String\n\nto f(w: Whale) -> Integer:\n    return w.age\n";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::UnknownMember { .. }))
        );
    }

    #[test]
    fn test_method_call_checked() {
        let input = "\
A Whale:\n    can swim(distance: Integer) -> Boolean:\n        return true\n\nto f(w: Whale) -> Boolean:\n    return w.swim(true)\n";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::ArgumentTypeMismatch { .. }))
        );
    }

    #[test]
    fn test_missing_type_args_on_class_init() {
        let input = "\
A Box[T]:\n    has:\n        value: T\n\nto f() -> Integer:\n    b = Box(value=1)\n    return b.value\n";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::MissingTypeArgs { .. }))
        );
    }

    #[test]
    fn test_unexpected_type_args_on_class_init() {
        let input = "\
A Box:\n    has:\n        value: Integer\n\nto f() -> Integer:\n    b = Box[Integer](value=1)\n    return b.value\n";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::UnexpectedTypeArgs { .. }))
        );
    }

    #[test]
    fn test_interface_missing_method() {
        let input = "\
A Printable:\n    must show() -> String\n\nA Foo:\n    is a Printable\n    can other() -> String:\n        return \"x\"\n\nto f() -> String:\n    foo = Foo()\n    return foo.other()\n";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::MissingInterfaceMethod { .. }))
        );
    }

    #[test]
    fn test_interface_method_name_overlap() {
        let input = "\
A Printable:
    must render() -> String

A Jsonable:
    must render() -> String

A Report:
    is a Printable
    has:
        name: String
    can render() -> String:
        return its.name

A Blob:
    is a Jsonable
    can render() -> String:
        return \"blob\"

to f(p: Printable) -> String:
    return p.render()
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let (errors, _info) = check_module_with_info(&module);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_interface_must_check_requires_given() {
        let input = "\
A Pred:
    must check ready() -> Boolean

A Foo:
    is a Pred
    checks ready() -> Boolean:
        return true

to f(p: Pred) -> Boolean:
    return p.ready()
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::CheckRequiresGiven { .. }))
        );
    }

    #[test]
    fn test_interface_must_check_allows_given() {
        let input = "\
A Pred:
    must check ready() -> Boolean

A Foo:
    is a Pred
    checks ready() -> Boolean:
        return true

to f(p: Pred) -> Boolean:
    return p.ready given
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_interface_must_check_requires_checks_impl() {
        let input = "\
A Pred:
    must check ready() -> Boolean

A Foo:
    is a Pred
    can ready() -> Boolean:
        return true
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::InterfaceMethodMismatch { .. }))
        );
    }

    #[test]
    fn test_given_call_records_boolean_expr_type() {
        let input = r#"
check is_positive(value: Integer) -> Boolean:
    return value > 0

to f() -> Boolean:
    return is_positive given 3
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let (errors, info) = check_module_with_info(&module);
        assert!(errors.is_empty(), "{errors:?}");

        let (func_id, func) = module
            .functions
            .iter()
            .find(|(_, func)| func.name.as_str() == "f")
            .expect("missing function f");
        let body = func.body.as_ref().expect("missing function body");
        let given_expr = body
            .exprs
            .iter()
            .find_map(|(id, expr)| match expr {
                Expr::GivenCall { .. } => Some(id.into_raw()),
                _ => None,
            })
            .expect("missing given call");
        let fn_info = info
            .function(func_id)
            .expect("missing type info for function");
        assert_eq!(fn_info.expr_types.get(&given_expr), Some(&Type::Boolean));
    }

    #[test]
    fn test_match_result_bindings_flow() {
        let input = r#"
to f() -> Integer:
    match __wr_fs_read_bytes("x"):
        Ok(v): return __wr_bytes_len(v)
        Err(e): return 0
        otherwise: return 2
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let (errors, _info) = check_module_with_info(&module);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_nested_pattern_bindings() {
        let input = r#"
A Status is either:
    Pending
    Failed(error: String)

to f(s: Status) -> String:
    match s:
        Status.Failed(e): return e
        Status.Pending: return "ok"
        otherwise: return "bad"
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let (errors, _info) = check_module_with_info(&module);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_function_call_checked() {
        let input = "\
to add(a: Integer, b: Integer) -> Integer:\n    return a + b\n\nto f() -> Integer:\n    return add(1, true)\n";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::ArgumentTypeMismatch { .. }))
        );
    }

    #[test]
    fn test_calling_non_callable_errors() {
        let input = "\
to f() -> Nothing:\n    x = 1\n    x(2)\n";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::InvalidCallee { .. }))
        );
    }

    #[test]
    fn test_method_return_type_flow() {
        let input = "\
A Ocean:\n    has:\n        depth: Integer\n\nA Whale:\n    can ocean() -> Ocean:\n        return Ocean()\n\nto f(w: Whale) -> Integer:\n    return w.ocean().depth\n";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_derived_property_access_type_ok() {
        let input = "\
A Whale:\n    has:\n        age: Integer\n    derives next_age() -> Integer:\n        return its.age + 1\n\nto f(w: Whale) -> Integer:\n    return w.next_age\n";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_calling_derived_property_errors() {
        let input = "\
A Whale:\n    has:\n        age: Integer\n    derives next_age() -> Integer:\n        return its.age + 1\n\nto f(w: Whale) -> Integer:\n    return w.next_age()\n";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::CallDerivedProperty { .. }))
        );
    }

    #[test]
    fn test_actor_call_requires_await_or_fire() {
        let input = "\
A Whale:\n    can swim() -> Boolean:\n        return true\n\nto f() -> Nothing:\n    w = detach Whale() * 1\n    w.swim()\n";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::PendingNotAwaited { .. }))
        );
    }

    #[test]
    fn test_error_requires_result_function() {
        let input = "to f() -> Integer:\n    error \"nope\"";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::ErrOutsideResult { .. }))
        );
    }

    #[test]
    fn test_otherwise_handles_result() {
        let input = "to f() -> Result:\n    return error \"nope\" otherwise 0";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_invalid_otherwise_operand() {
        let input = "to f() -> Integer:\n    return 1 otherwise 0";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::InvalidOtherwiseOperand { .. }))
        );
    }

    #[test]
    fn test_invalid_unary_operand_span() {
        let input = "to f() -> Integer:\n    -true";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        let err = errors
            .iter()
            .find(|err| matches!(err, TypeError::InvalidUnaryOperand { .. }))
            .expect("missing invalid unary operand error");
        if let TypeError::InvalidUnaryOperand { span, .. } = err {
            let expected = input.rfind('-').unwrap();
            assert_eq!(span.offset(), expected);
            assert_eq!(span.len(), 1);
        }
    }

    #[test]
    fn test_invalid_binary_operand_span() {
        let input = "to f() -> Integer:\n    true + 1";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        let err = errors
            .iter()
            .find(|err| matches!(err, TypeError::InvalidBinaryOperands { .. }))
            .expect("missing invalid binary operands error");
        if let TypeError::InvalidBinaryOperands { span, .. } = err {
            let expected = input.find('+').unwrap();
            assert_eq!(span.offset(), expected);
            assert_eq!(span.len(), 1);
        }
    }

    #[test]
    fn test_unknown_member_span() {
        let input = "\
A Foo:\n    has:\n        x: Integer\n\nto f() -> Nothing:\n    foo = Foo(x=1)\n    foo.bar\n";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        let err = errors
            .iter()
            .find(|err| matches!(err, TypeError::UnknownMember { .. }))
            .expect("missing unknown member error");
        if let TypeError::UnknownMember { span, .. } = err {
            let expected = input.find("bar").unwrap();
            assert_eq!(span.offset(), expected);
            assert_eq!(span.len(), 3);
        }
    }

    #[test]
    fn test_actor_call_with_await_ok() {
        let input = "\
A Whale:\n    can swim() -> Boolean:\n        return true\n\nto f() -> Result:\n    w = detach Whale() * 1\n    return await w.swim()\n";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_builtin_fallible_requires_handling() {
        let input = "to f() -> Nothing:\n    __wr_fs_read_bytes(\"x\")";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::UnhandledResult { .. }))
        );
    }

    #[test]
    fn test_builtin_fallible_otherwise_ok() {
        let input = "to f() -> Integer:\n    return __wr_bytes_len(__wr_fs_read_bytes(\"x\") otherwise __wr_bytes_from_string(\"1\"))";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_builtin_external_call_requires_handling() {
        let input = "\
to f() -> Nothing:\n    headers = __wr_map_new()\n    __wr_external_call(\"svc\", \"ep\", \"GET\", \"https://example\", headers, \"\", 10)\n";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::UnhandledResult { .. }))
        );
    }

    #[test]
    fn test_builtin_external_call_otherwise_ok() {
        let input = "\
to f() -> String:\n    headers = __wr_map_new()\n    return __wr_external_call(\"svc\", \"ep\", \"GET\", \"https://example\", headers, \"\", 10) otherwise \"fallback\"\n";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_builtin_map_new_signature_ok() {
        let input = "\
to f() -> Nothing:\n    m = __wr_map_new()\n    __wr_map_set(m, \"k\", \"v\")\n    __wr_map_get(m, \"k\")\n";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_builtin_map_new_arg_count_mismatch() {
        let input = "to f() -> Nothing:\n    __wr_map_new(1)";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::ArgumentCountMismatch { .. }))
        );
    }

    #[test]
    fn test_await_on_pending_value_ok() {
        let input = "to f() -> Result:\n    return await __wr_sleep_ms(1)\n";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_fire_on_pending_value_ok() {
        let input = "to f() -> Nothing:\n    fire __wr_sleep_ms(1)\n";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_await_on_non_actor_call_errors() {
        let input = "\
A Whale:\n    can swim() -> Boolean:\n        return true\n\nto f(w: Whale) -> Result[Boolean]:\n    return await w.swim()\n";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::InvalidAwaitOperand { .. }))
        );
    }

    #[test]
    fn test_fire_actor_call_ok() {
        let input = "\
A Whale:\n    can swim() -> Boolean:\n        return true\n\nto f() -> Nothing:\n    w = detach Whale() * 1\n    fire w.swim()\n";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_fire_non_actor_call_errors() {
        let input = "\
A Whale:\n    can swim() -> Boolean:\n        return true\n\nto f(w: Whale) -> Nothing:\n    fire w.swim()\n";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::InvalidFireOperand { .. }))
        );
    }

    #[test]
    fn test_class_init_field_type_checked() {
        let input = "\
A Whale:\n    has:\n        name: String\n\n\
to f() -> Nothing:\n    Whale(name=1)\n";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::ArgumentTypeMismatch { .. }))
        );
    }

    #[test]
    fn test_class_init_unknown_field() {
        let input = "\
A Whale:\n    has:\n        name: String\n\n\
to f() -> Nothing:\n    Whale(age=\"old\")\n";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::UnknownArgument { .. }))
        );
    }

    #[test]
    fn test_await_on_actor_value_errors() {
        let input = "\
A Whale:\n    can swim() -> Boolean:\n        return true\n\nto f() -> Result:\n    w = detach Whale() * 1\n    return await w\n";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::InvalidAwaitOperand { .. }))
        );
    }

    #[test]
    fn test_fire_on_actor_value_errors() {
        let input = "\
A Whale:\n    can swim() -> Boolean:\n        return true\n\nto f() -> Nothing:\n    w = detach Whale() * 1\n    fire w\n";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::InvalidFireOperand { .. }))
        );
    }

    #[test]
    fn test_async_class_requires_actor() {
        let input = "\
A Whale:\n    can swim() -> Boolean:\n        return true\n\n\
A Boat:\n    can ride() -> Boolean:\n        return await Whale().swim()\n\n\
to f() -> Nothing:\n    Boat()\n";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::AsyncClassRequiresActor { .. }))
        );
    }

    #[test]
    fn test_async_method_requires_actor() {
        let input = "\
A Whale:\n    can swim() -> Boolean:\n        return true\n\n\
A Boat:\n    can ride() -> Boolean:\n        return await Whale().swim()\n\n\
to f() -> Boolean:\n    b = Boat()\n    return b.ride()\n";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::AsyncMethodRequiresActor { .. }))
        );
    }

    #[test]
    fn test_async_chain_requires_actor() {
        let input = "\
A Whale:\n    can swim() -> Boolean:\n        return true\n\n\
to helper() -> Boolean:\n    return await Whale().swim()\n\n\
A Boat:\n    can ride() -> Boolean:\n        return helper()\n\n\
to f() -> Nothing:\n    Boat()\n";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::AsyncClassRequiresActor { .. }))
        );
    }

    #[test]
    fn test_async_error_includes_chain_hint() {
        let input = "\
A Whale:\n    can swim() -> Boolean:\n        return true\n\n\
to helper() -> Boolean:\n    return await Whale().swim()\n\n\
A Boat:\n    can ride() -> Boolean:\n        return helper()\n\n\
to f() -> Boolean:\n    b = Boat()\n    return b.ride()\n";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        let mut saw = false;
        for err in &errors {
            if let TypeError::AsyncMethodRequiresActor { help, .. } = err {
                assert!(help.contains("Async call chain:"));
                assert!(help.contains("Boat.ride"));
                assert!(help.contains("helper"));
                saw = true;
                break;
            }
        }
        assert!(saw, "expected AsyncMethodRequiresActor error");
    }

    #[test]
    fn test_fire_chain_requires_actor() {
        let input = "\
A Whale:\n    can swim() -> Boolean:\n        return true\n\n\
to helper() -> Boolean:\n    fire Whale().swim()\n    return true\n\n\
A Boat:\n    can ride() -> Boolean:\n        return helper()\n\n\
to f() -> Nothing:\n    Boat()\n";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::AsyncClassRequiresActor { .. }))
        );
    }

    #[test]
    fn test_async_class_allowed_with_detach() {
        let input = "\
A Whale:\n    can swim() -> Boolean:\n        return true\n\n\
A Boat:\n    can ride() -> Boolean:\n        return await Whale().swim()\n\n\
to f() -> Result:\n    b = detach Boat() * 1\n    return await b.ride()\n";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .all(|err| !matches!(err, TypeError::AsyncClassRequiresActor { .. }))
        );
    }
}
