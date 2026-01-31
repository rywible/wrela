#![allow(unused_assignments)]

use crate::hir::{BinaryOp, Body, Expr, Function, Idx, Literal, Module, Stmt, TypeRef, UnaryOp};
use miette::{Diagnostic, SourceSpan};
use smol_str::SmolStr;
use std::collections::{HashMap, HashSet};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    Unknown,
    Never,
    Int,
    Float,
    Number,
    Bool,
    String,
    Nil,
    List(Box<Type>),
    Map(Box<Type>, Box<Type>),
    Named(SmolStr),
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

    #[error("argument '{name}' has type '{found}' but expected '{expected}'")]
    #[diagnostic(code(lang::ty::argument_type))]
    ArgumentTypeMismatch {
        name: SmolStr,
        expected: String,
        found: String,
        #[label("argument here")]
        span: SourceSpan,
    },

    #[error("await expects an actor method call")]
    #[diagnostic(code(lang::ty::invalid_await_operand))]
    InvalidAwaitOperand {
        #[label("await here")]
        span: SourceSpan,
    },

    #[error("fire expects an actor method call")]
    #[diagnostic(code(lang::ty::invalid_fire_operand))]
    InvalidFireOperand {
        #[label("fire here")]
        span: SourceSpan,
    },

    #[error("actor call must be awaited or fired")]
    #[diagnostic(code(lang::ty::pending_not_awaited))]
    PendingNotAwaited {
        #[label("actor call here")]
        span: SourceSpan,
    },

    #[error("result must be handled with `otherwise` or returned from a `Result` function")]
    #[diagnostic(code(lang::ty::unhandled_result))]
    UnhandledResult {
        #[label("result here")]
        span: SourceSpan,
    },

    #[error("`otherwise` expects a Result on the left side")]
    #[diagnostic(code(lang::ty::invalid_otherwise))]
    InvalidOtherwiseOperand {
        #[label("otherwise here")]
        span: SourceSpan,
    },

    #[error("`err` can only be used in functions that return Result")]
    #[diagnostic(code(lang::ty::err_outside_result))]
    ErrOutsideResult {
        #[label("err here")]
        span: SourceSpan,
    },

    #[error("function must return Result because it contains fallible operations")]
    #[diagnostic(code(lang::ty::missing_result_return))]
    MissingResultReturn {
        #[label("function here")]
        span: SourceSpan,
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
}

impl TypeError {
    pub fn primary_span(&self) -> SourceSpan {
        match self {
            TypeError::InvalidUnaryOperand { span, .. } => *span,
            TypeError::InvalidBinaryOperands { span, .. } => *span,
            TypeError::InvalidAssignment { span, .. } => *span,
            TypeError::ReturnTypeMismatch { span, .. } => *span,
            TypeError::UnknownMember { span, .. } => *span,
            TypeError::CallField { span, .. } => *span,
            TypeError::InvalidCallee { span } => *span,
            TypeError::ArgumentCountMismatch { span, .. } => *span,
            TypeError::UnknownArgument { span, .. } => *span,
            TypeError::ArgumentTypeMismatch { span, .. } => *span,
            TypeError::InvalidAwaitOperand { span } => *span,
            TypeError::InvalidFireOperand { span } => *span,
            TypeError::PendingNotAwaited { span } => *span,
            TypeError::UnhandledResult { span } => *span,
            TypeError::InvalidOtherwiseOperand { span } => *span,
            TypeError::ErrOutsideResult { span } => *span,
            TypeError::MissingResultReturn { span } => *span,
            TypeError::ActorMemberAccess { span, .. } => *span,
            TypeError::AsyncClassRequiresActor { span, .. } => *span,
            TypeError::AsyncMethodRequiresActor { span, .. } => *span,
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
            &function_index,
            &mut errors,
            method_class,
            &mut info,
        );
    }
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
    functions: &FunctionIndex,
    errors: &mut Vec<TypeError>,
    method_class: Option<SmolStr>,
    info: &mut TypeInfo,
) {
    let mut fn_info = FunctionTypeInfo::default();
    let mut ctx = TypeContext::with_info(&mut fn_info);
    ctx.enter_scope();
    ctx.declare(SmolStr::new("nil"), Type::Nil);
    if let Some(class_name) = &method_class {
        ctx.declare(class_name.clone(), Type::Named(class_name.clone()));
        ctx.declare(SmolStr::new("it"), Type::Named(class_name.clone()));
    }
    for param in &func.params {
        let ty = param
            .ty
            .as_ref()
            .map(type_from_ref)
            .unwrap_or(Type::Unknown);
        ctx.declare(param.name.clone(), ty);
    }
    let ret_type = func.ret_type.as_ref().map(type_from_ref);
    let returns_result = matches!(ret_type, Some(Type::Result(_, _)));
    if let Some(body) = &func.body {
        for stmt in &body.root_stmts {
            check_stmt(
                body,
                *stmt,
                &mut ctx,
                classes,
                functions,
                errors,
                ret_type.as_ref(),
                returns_result,
                func.name_span,
            );
        }
    }
    ctx.exit_scope();
    info.functions.insert(func_id.into_raw(), fn_info);
}

#[derive(Debug, Clone)]
struct MethodSig {
    params: Vec<(SmolStr, Type)>,
    ret: Type,
}

#[derive(Debug, Clone)]
struct FunctionSig {
    params: Vec<(SmolStr, Type)>,
    ret: Type,
}

#[derive(Debug, Clone)]
struct ClassSig {
    fields: HashMap<SmolStr, Type>,
    methods: HashMap<SmolStr, MethodSig>,
    field_order: Vec<SmolStr>,
}

struct ClassIndex {
    classes: HashMap<SmolStr, ClassSig>,
}

impl ClassIndex {
    fn new(module: &Module) -> Self {
        let mut classes = HashMap::new();
        for (_idx, class) in module.classes.iter() {
            let mut fields = HashMap::new();
            let mut field_order = Vec::new();
            for field in &class.fields {
                let ty = field
                    .ty
                    .as_ref()
                    .map(type_from_ref)
                    .unwrap_or(Type::Unknown);
                fields.insert(field.name.clone(), ty);
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
                                .map(type_from_ref)
                                .unwrap_or(Type::Unknown),
                        )
                    })
                    .collect();
                let ret = method
                    .ret_type
                    .as_ref()
                    .map(type_from_ref)
                    .unwrap_or(Type::Unknown);
                methods.insert(method.name.clone(), MethodSig { params, ret });
            }
            classes.insert(
                class.name.clone(),
                ClassSig {
                    fields,
                    methods,
                    field_order,
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
            functions.insert(func.name.clone(), FunctionSig { params, ret });
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
            SmolStr::new("print"),
            FunctionSig {
                params: vec![(SmolStr::new("value"), Type::String)],
                ret: Type::Nil,
            },
        ),
        (
            SmolStr::new("parse_int"),
            FunctionSig {
                params: vec![(SmolStr::new("value"), Type::String)],
                ret: Type::Result(Box::new(Type::Int), Box::new(err.clone())),
            },
        ),
        (
            SmolStr::new("parse_float"),
            FunctionSig {
                params: vec![(SmolStr::new("value"), Type::String)],
                ret: Type::Result(Box::new(Type::Float), Box::new(err.clone())),
            },
        ),
        (
            SmolStr::new("read_file"),
            FunctionSig {
                params: vec![(SmolStr::new("path"), Type::String)],
                ret: Type::Result(Box::new(Type::String), Box::new(err.clone())),
            },
        ),
        (
            SmolStr::new("bytes_from_string"),
            FunctionSig {
                params: vec![(SmolStr::new("value"), Type::String)],
                ret: Type::Named(SmolStr::new("Bytes")),
            },
        ),
        (
            SmolStr::new("bytes_to_string"),
            FunctionSig {
                params: vec![(SmolStr::new("value"), Type::Named(SmolStr::new("Bytes")))],
                ret: Type::String,
            },
        ),
        (
            SmolStr::new("bytes_len"),
            FunctionSig {
                params: vec![(SmolStr::new("value"), Type::Named(SmolStr::new("Bytes")))],
                ret: Type::Int,
            },
        ),
        (
            SmolStr::new("write_file"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("path"), Type::String),
                    (SmolStr::new("contents"), Type::String),
                ],
                ret: Type::Result(Box::new(Type::Nil), Box::new(err.clone())),
            },
        ),
        (
            SmolStr::new("list_push"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("list"), Type::List(Box::new(Type::Unknown))),
                    (SmolStr::new("value"), Type::Unknown),
                ],
                ret: Type::Nil,
            },
        ),
        (
            SmolStr::new("map_get"),
            FunctionSig {
                params: vec![
                    (
                        SmolStr::new("map"),
                        Type::Map(Box::new(Type::Unknown), Box::new(Type::Unknown)),
                    ),
                    (SmolStr::new("key"), Type::Unknown),
                ],
                ret: Type::Unknown,
            },
        ),
        (
            SmolStr::new("map_set"),
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
            },
        ),
        (
            SmolStr::new("pool_auto_size"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("objective"), Type::Int),
                    (SmolStr::new("min"), Type::Int),
                    (SmolStr::new("max"), Type::Int),
                    (SmolStr::new("weight"), Type::Int),
                ],
                ret: Type::Int,
            },
        ),
        (
            SmolStr::new("pool_size"),
            FunctionSig {
                params: vec![(SmolStr::new("handle"), Type::Unknown)],
                ret: Type::Int,
            },
        ),
        (
            SmolStr::new("pool_rr"),
            FunctionSig {
                params: vec![(SmolStr::new("handle"), Type::Unknown)],
                ret: Type::Int,
            },
        ),
        (
            SmolStr::new("pool_queue_len"),
            FunctionSig {
                params: vec![(SmolStr::new("handle"), Type::Unknown)],
                ret: Type::Int,
            },
        ),
        (
            SmolStr::new("actor_mailbox_len"),
            FunctionSig {
                params: vec![(SmolStr::new("handle"), Type::Unknown)],
                ret: Type::Int,
            },
        ),
        (
            SmolStr::new("actor_pause"),
            FunctionSig {
                params: vec![(SmolStr::new("handle"), Type::Unknown)],
                ret: Type::Nil,
            },
        ),
        (
            SmolStr::new("actor_resume"),
            FunctionSig {
                params: vec![(SmolStr::new("handle"), Type::Unknown)],
                ret: Type::Nil,
            },
        ),
        (
            SmolStr::new("actor_pause_wait"),
            FunctionSig {
                params: vec![(SmolStr::new("handle"), Type::Unknown)],
                ret: Type::Nil,
            },
        ),
        (
            SmolStr::new("metrics_get"),
            FunctionSig {
                params: vec![(SmolStr::new("id"), Type::Int)],
                ret: Type::Int,
            },
        ),
        (
            SmolStr::new("metrics_dropped_paused_id"),
            FunctionSig {
                params: vec![],
                ret: Type::Int,
            },
        ),
        (
            SmolStr::new("metrics_messages_dropped_id"),
            FunctionSig {
                params: vec![],
                ret: Type::Int,
            },
        ),
        (
            SmolStr::new("clock_ns"),
            FunctionSig {
                params: vec![],
                ret: Type::Int,
            },
        ),
        (
            SmolStr::new("sleep_ms"),
            FunctionSig {
                params: vec![(SmolStr::new("ms"), Type::Int)],
                ret: Type::Pending(Box::new(Type::Nil)),
            },
        ),
        (
            SmolStr::new("env_get"),
            FunctionSig {
                params: vec![(SmolStr::new("key"), Type::String)],
                ret: Type::Unknown,
            },
        ),
        (
            SmolStr::new("env_get_or"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("key"), Type::String),
                    (SmolStr::new("default"), Type::String),
                ],
                ret: Type::String,
            },
        ),
        (
            SmolStr::new("env_get_as_bool"),
            FunctionSig {
                params: vec![(SmolStr::new("key"), Type::String)],
                ret: Type::Unknown,
            },
        ),
        (
            SmolStr::new("env_get_as_int"),
            FunctionSig {
                params: vec![(SmolStr::new("key"), Type::String)],
                ret: Type::Unknown,
            },
        ),
        (
            SmolStr::new("env_set"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("key"), Type::String),
                    (SmolStr::new("value"), Type::String),
                ],
                ret: Type::Bool,
            },
        ),
        (
            SmolStr::new("env_load"),
            FunctionSig {
                params: vec![(SmolStr::new("path"), Type::String)],
                ret: Type::Bool,
            },
        ),
        (
            SmolStr::new("auth_create_user"),
            FunctionSig {
                params: vec![
                    (
                        SmolStr::new("storage"),
                        Type::Named(SmolStr::new("StorageClient")),
                    ),
                    (SmolStr::new("email"), Type::String),
                    (SmolStr::new("username"), Type::String),
                    (SmolStr::new("password"), Type::String),
                ],
                ret: Type::Pending(Box::new(Type::Unknown)),
            },
        ),
        (
            SmolStr::new("auth_verify_password"),
            FunctionSig {
                params: vec![
                    (
                        SmolStr::new("storage"),
                        Type::Named(SmolStr::new("StorageClient")),
                    ),
                    (SmolStr::new("user_id"), Type::String),
                    (SmolStr::new("password"), Type::String),
                ],
                ret: Type::Pending(Box::new(Type::Bool)),
            },
        ),
        (
            SmolStr::new("auth_issue_jwt"),
            FunctionSig {
                params: vec![
                    (
                        SmolStr::new("storage"),
                        Type::Named(SmolStr::new("StorageClient")),
                    ),
                    (SmolStr::new("user_id"), Type::String),
                    (
                        SmolStr::new("claims"),
                        Type::Map(Box::new(Type::Unknown), Box::new(Type::Unknown)),
                    ),
                    (SmolStr::new("ttl_secs"), Type::Int),
                ],
                ret: Type::Pending(Box::new(Type::String)),
            },
        ),
        (
            SmolStr::new("auth_verify_jwt"),
            FunctionSig {
                params: vec![
                    (
                        SmolStr::new("storage"),
                        Type::Named(SmolStr::new("StorageClient")),
                    ),
                    (SmolStr::new("token"), Type::String),
                ],
                ret: Type::Pending(Box::new(Type::Unknown)),
            },
        ),
        (
            SmolStr::new("auth_issue_email_token"),
            FunctionSig {
                params: vec![
                    (
                        SmolStr::new("storage"),
                        Type::Named(SmolStr::new("StorageClient")),
                    ),
                    (SmolStr::new("user_id"), Type::String),
                    (SmolStr::new("ttl_secs"), Type::Int),
                ],
                ret: Type::Pending(Box::new(Type::String)),
            },
        ),
        (
            SmolStr::new("auth_verify_email_token"),
            FunctionSig {
                params: vec![
                    (
                        SmolStr::new("storage"),
                        Type::Named(SmolStr::new("StorageClient")),
                    ),
                    (SmolStr::new("token"), Type::String),
                ],
                ret: Type::Pending(Box::new(Type::Unknown)),
            },
        ),
        (
            SmolStr::new("auth_oauth_login"),
            FunctionSig {
                params: vec![
                    (
                        SmolStr::new("storage"),
                        Type::Named(SmolStr::new("StorageClient")),
                    ),
                    (SmolStr::new("provider"), Type::String),
                    (SmolStr::new("code"), Type::String),
                ],
                ret: Type::Pending(Box::new(Type::Unknown)),
            },
        ),
        (
            SmolStr::new("rbac_create_role"),
            FunctionSig {
                params: vec![
                    (
                        SmolStr::new("storage"),
                        Type::Named(SmolStr::new("StorageClient")),
                    ),
                    (SmolStr::new("scope"), Type::String),
                    (SmolStr::new("name"), Type::String),
                    (
                        SmolStr::new("permissions"),
                        Type::List(Box::new(Type::String)),
                    ),
                ],
                ret: Type::Pending(Box::new(Type::String)),
            },
        ),
        (
            SmolStr::new("rbac_assign_role"),
            FunctionSig {
                params: vec![
                    (
                        SmolStr::new("storage"),
                        Type::Named(SmolStr::new("StorageClient")),
                    ),
                    (SmolStr::new("user_id"), Type::String),
                    (SmolStr::new("role_id"), Type::String),
                    (SmolStr::new("scope_id"), Type::String),
                ],
                ret: Type::Pending(Box::new(Type::Bool)),
            },
        ),
        (
            SmolStr::new("rbac_check"),
            FunctionSig {
                params: vec![
                    (
                        SmolStr::new("storage"),
                        Type::Named(SmolStr::new("StorageClient")),
                    ),
                    (SmolStr::new("user_id"), Type::String),
                    (SmolStr::new("permission"), Type::String),
                    (SmolStr::new("scope_id"), Type::String),
                ],
                ret: Type::Pending(Box::new(Type::Bool)),
            },
        ),
        (
            SmolStr::new("rbac_permissions_for"),
            FunctionSig {
                params: vec![
                    (
                        SmolStr::new("storage"),
                        Type::Named(SmolStr::new("StorageClient")),
                    ),
                    (SmolStr::new("user_id"), Type::String),
                    (SmolStr::new("scope_id"), Type::String),
                ],
                ret: Type::Pending(Box::new(Type::List(Box::new(Type::String)))),
            },
        ),
        (
            SmolStr::new("files_upload_stream"),
            FunctionSig {
                params: vec![
                    (
                        SmolStr::new("storage"),
                        Type::Named(SmolStr::new("StorageClient")),
                    ),
                    (SmolStr::new("stream"), Type::Named(SmolStr::new("Bytes"))),
                    (
                        SmolStr::new("opts"),
                        Type::Map(Box::new(Type::Unknown), Box::new(Type::Unknown)),
                    ),
                ],
                ret: Type::Pending(Box::new(Type::String)),
            },
        ),
        (
            SmolStr::new("files_signed_url"),
            FunctionSig {
                params: vec![
                    (
                        SmolStr::new("storage"),
                        Type::Named(SmolStr::new("StorageClient")),
                    ),
                    (SmolStr::new("file_id"), Type::String),
                    (
                        SmolStr::new("opts"),
                        Type::Map(Box::new(Type::Unknown), Box::new(Type::Unknown)),
                    ),
                ],
                ret: Type::Pending(Box::new(Type::String)),
            },
        ),
        (
            SmolStr::new("files_metadata"),
            FunctionSig {
                params: vec![
                    (
                        SmolStr::new("storage"),
                        Type::Named(SmolStr::new("StorageClient")),
                    ),
                    (SmolStr::new("file_id"), Type::String),
                ],
                ret: Type::Pending(Box::new(Type::Unknown)),
            },
        ),
        (
            SmolStr::new("files_delete"),
            FunctionSig {
                params: vec![
                    (
                        SmolStr::new("storage"),
                        Type::Named(SmolStr::new("StorageClient")),
                    ),
                    (SmolStr::new("file_id"), Type::String),
                ],
                ret: Type::Pending(Box::new(Type::Bool)),
            },
        ),
        (
            SmolStr::new("files_set_acl"),
            FunctionSig {
                params: vec![
                    (
                        SmolStr::new("storage"),
                        Type::Named(SmolStr::new("StorageClient")),
                    ),
                    (SmolStr::new("file_id"), Type::String),
                    (SmolStr::new("acl"), Type::String),
                ],
                ret: Type::Pending(Box::new(Type::Bool)),
            },
        ),
        (
            SmolStr::new("jobs_enqueue"),
            FunctionSig {
                params: vec![
                    (
                        SmolStr::new("storage"),
                        Type::Named(SmolStr::new("StorageClient")),
                    ),
                    (SmolStr::new("queue"), Type::String),
                    (
                        SmolStr::new("payload"),
                        Type::Map(Box::new(Type::Unknown), Box::new(Type::Unknown)),
                    ),
                    (
                        SmolStr::new("opts"),
                        Type::Map(Box::new(Type::Unknown), Box::new(Type::Unknown)),
                    ),
                ],
                ret: Type::Pending(Box::new(Type::String)),
            },
        ),
        (
            SmolStr::new("jobs_process"),
            FunctionSig {
                params: vec![
                    (
                        SmolStr::new("storage"),
                        Type::Named(SmolStr::new("StorageClient")),
                    ),
                    (SmolStr::new("queue"), Type::String),
                    (SmolStr::new("handler"), Type::Unknown),
                ],
                ret: Type::Pending(Box::new(Type::Bool)),
            },
        ),
        (
            SmolStr::new("jobs_dead_letter"),
            FunctionSig {
                params: vec![
                    (
                        SmolStr::new("storage"),
                        Type::Named(SmolStr::new("StorageClient")),
                    ),
                    (SmolStr::new("queue"), Type::String),
                ],
                ret: Type::Pending(Box::new(Type::List(Box::new(Type::Unknown)))),
            },
        ),
        (
            SmolStr::new("schedule_cron"),
            FunctionSig {
                params: vec![
                    (
                        SmolStr::new("storage"),
                        Type::Named(SmolStr::new("StorageClient")),
                    ),
                    (SmolStr::new("expr"), Type::String),
                    (SmolStr::new("job"), Type::Unknown),
                ],
                ret: Type::Pending(Box::new(Type::Bool)),
            },
        ),
        (
            SmolStr::new("schedule_every"),
            FunctionSig {
                params: vec![
                    (
                        SmolStr::new("storage"),
                        Type::Named(SmolStr::new("StorageClient")),
                    ),
                    (SmolStr::new("seconds"), Type::Int),
                    (SmolStr::new("job"), Type::Unknown),
                ],
                ret: Type::Pending(Box::new(Type::Bool)),
            },
        ),
        (
            SmolStr::new("schedule_at"),
            FunctionSig {
                params: vec![
                    (
                        SmolStr::new("storage"),
                        Type::Named(SmolStr::new("StorageClient")),
                    ),
                    (SmolStr::new("timestamp"), Type::Int),
                    (SmolStr::new("job"), Type::Unknown),
                ],
                ret: Type::Pending(Box::new(Type::Bool)),
            },
        ),
        (
            SmolStr::new("search_index"),
            FunctionSig {
                params: vec![
                    (
                        SmolStr::new("storage"),
                        Type::Named(SmolStr::new("StorageClient")),
                    ),
                    (SmolStr::new("collection"), Type::String),
                    (SmolStr::new("id"), Type::String),
                    (SmolStr::new("text"), Type::String),
                    (
                        SmolStr::new("fields"),
                        Type::Map(Box::new(Type::Unknown), Box::new(Type::Unknown)),
                    ),
                ],
                ret: Type::Pending(Box::new(Type::Bool)),
            },
        ),
        (
            SmolStr::new("search_remove"),
            FunctionSig {
                params: vec![
                    (
                        SmolStr::new("storage"),
                        Type::Named(SmolStr::new("StorageClient")),
                    ),
                    (SmolStr::new("collection"), Type::String),
                    (SmolStr::new("id"), Type::String),
                ],
                ret: Type::Pending(Box::new(Type::Bool)),
            },
        ),
        (
            SmolStr::new("search_query"),
            FunctionSig {
                params: vec![
                    (
                        SmolStr::new("storage"),
                        Type::Named(SmolStr::new("StorageClient")),
                    ),
                    (SmolStr::new("collection"), Type::String),
                    (SmolStr::new("query"), Type::String),
                    (
                        SmolStr::new("opts"),
                        Type::Map(Box::new(Type::Unknown), Box::new(Type::Unknown)),
                    ),
                ],
                ret: Type::Pending(Box::new(Type::List(Box::new(Type::Unknown)))),
            },
        ),
        (
            SmolStr::new("realtime_on_connect"),
            FunctionSig {
                params: vec![(SmolStr::new("handler"), Type::Unknown)],
                ret: Type::Pending(Box::new(Type::Bool)),
            },
        ),
        (
            SmolStr::new("realtime_join"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("socket_id"), Type::String),
                    (SmolStr::new("room"), Type::String),
                ],
                ret: Type::Pending(Box::new(Type::Bool)),
            },
        ),
        (
            SmolStr::new("realtime_leave"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("socket_id"), Type::String),
                    (SmolStr::new("room"), Type::String),
                ],
                ret: Type::Pending(Box::new(Type::Bool)),
            },
        ),
        (
            SmolStr::new("realtime_broadcast"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("room"), Type::String),
                    (SmolStr::new("message"), Type::Unknown),
                ],
                ret: Type::Pending(Box::new(Type::Bool)),
            },
        ),
        (
            SmolStr::new("realtime_send"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("socket_id"), Type::String),
                    (SmolStr::new("message"), Type::Unknown),
                ],
                ret: Type::Pending(Box::new(Type::Bool)),
            },
        ),
        (
            SmolStr::new("rate_check"),
            FunctionSig {
                params: vec![
                    (
                        SmolStr::new("storage"),
                        Type::Named(SmolStr::new("StorageClient")),
                    ),
                    (SmolStr::new("key"), Type::String),
                    (
                        SmolStr::new("opts"),
                        Type::Map(Box::new(Type::Unknown), Box::new(Type::Unknown)),
                    ),
                ],
                ret: Type::Pending(Box::new(Type::Bool)),
            },
        ),
        (
            SmolStr::new("rate_ip"),
            FunctionSig {
                params: vec![(
                    SmolStr::new("request"),
                    Type::Named(SmolStr::new("HttpRequest")),
                )],
                ret: Type::String,
            },
        ),
        (
            SmolStr::new("admin_enable"),
            FunctionSig {
                params: vec![(
                    SmolStr::new("opts"),
                    Type::Map(Box::new(Type::Unknown), Box::new(Type::Unknown)),
                )],
                ret: Type::Pending(Box::new(Type::Bool)),
            },
        ),
        (
            SmolStr::new("storage_get"),
            FunctionSig {
                params: vec![(SmolStr::new("key"), Type::String)],
                ret: Type::Pending(Box::new(Type::Result(
                    Box::new(Type::Unknown),
                    Box::new(err.clone()),
                ))),
            },
        ),
        (
            SmolStr::new("storage_get_with_version"),
            FunctionSig {
                params: vec![(SmolStr::new("key"), Type::String)],
                ret: Type::Pending(Box::new(Type::Result(
                    Box::new(Type::Unknown),
                    Box::new(err.clone()),
                ))),
            },
        ),
        (
            SmolStr::new("storage_scan"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("start"), Type::Unknown),
                    (SmolStr::new("end"), Type::Unknown),
                    (SmolStr::new("limit"), Type::Number),
                ],
                ret: Type::Pending(Box::new(Type::Result(
                    Box::new(Type::List(Box::new(Type::Unknown))),
                    Box::new(err.clone()),
                ))),
            },
        ),
        (
            SmolStr::new("storage_list_prefix"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("prefix"), Type::String),
                    (SmolStr::new("limit"), Type::Number),
                ],
                ret: Type::Pending(Box::new(Type::Result(
                    Box::new(Type::List(Box::new(Type::String))),
                    Box::new(err.clone()),
                ))),
            },
        ),
        (
            SmolStr::new("storage_configure"),
            FunctionSig {
                params: vec![(
                    SmolStr::new("config"),
                    Type::Named(SmolStr::new("StorageConfig")),
                )],
                ret: Type::Result(Box::new(Type::Nil), Box::new(err.clone())),
            },
        ),
        (
            SmolStr::new("storage_set"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("key"), Type::String),
                    (SmolStr::new("value"), Type::String),
                ],
                ret: Type::Pending(Box::new(Type::Result(
                    Box::new(Type::Nil),
                    Box::new(err.clone()),
                ))),
            },
        ),
        (
            SmolStr::new("storage_set_if_version"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("key"), Type::String),
                    (SmolStr::new("value"), Type::String),
                    (SmolStr::new("version"), Type::Number),
                ],
                ret: Type::Pending(Box::new(Type::Result(
                    Box::new(Type::Bool),
                    Box::new(err.clone()),
                ))),
            },
        ),
        (
            SmolStr::new("storage_delete_if_version"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("key"), Type::String),
                    (SmolStr::new("version"), Type::Number),
                ],
                ret: Type::Pending(Box::new(Type::Result(
                    Box::new(Type::Bool),
                    Box::new(err.clone()),
                ))),
            },
        ),
        (
            SmolStr::new("storage_delete"),
            FunctionSig {
                params: vec![(SmolStr::new("key"), Type::String)],
                ret: Type::Pending(Box::new(Type::Result(
                    Box::new(Type::Nil),
                    Box::new(err.clone()),
                ))),
            },
        ),
        (
            SmolStr::new("storage_batch_set"),
            FunctionSig {
                params: vec![(
                    SmolStr::new("items"),
                    Type::List(Box::new(Type::Map(
                        Box::new(Type::Unknown),
                        Box::new(Type::Unknown),
                    ))),
                )],
                ret: Type::Pending(Box::new(Type::Result(Box::new(Type::Bool), Box::new(err)))),
            },
        ),
        (
            SmolStr::new("http_server_serve_get_requests"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("path"), Type::String),
                    (SmolStr::new("handler"), Type::Unknown),
                ],
                ret: Type::Nil,
            },
        ),
        (
            SmolStr::new("http_server_serve_post_requests"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("path"), Type::String),
                    (SmolStr::new("handler"), Type::Unknown),
                ],
                ret: Type::Nil,
            },
        ),
        (
            SmolStr::new("http_server_serve_requests"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("method"), Type::String),
                    (SmolStr::new("path"), Type::String),
                    (SmolStr::new("handler"), Type::Unknown),
                ],
                ret: Type::Nil,
            },
        ),
        (
            SmolStr::new("http_server_serve_on"),
            FunctionSig {
                params: vec![(SmolStr::new("addr"), Type::String)],
                ret: Type::Nil,
            },
        ),
        (
            SmolStr::new("http_server_stop"),
            FunctionSig {
                params: vec![],
                ret: Type::Nil,
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
            if let Expr::Variable(name) = &body.exprs[*value] {
                if classes.is_class(name) {
                    return Some(name.clone());
                }
            }
            break;
        }
    }
    None
}

struct TypeContext {
    scopes: Vec<HashMap<SmolStr, Type>>,
    info: Option<*mut FunctionTypeInfo>,
}

impl TypeContext {
    fn with_info(info: &mut FunctionTypeInfo) -> Self {
        Self {
            scopes: Vec::new(),
            info: Some(info as *mut FunctionTypeInfo),
        }
    }

    fn enter_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn exit_scope(&mut self) {
        self.scopes.pop();
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
                functions,
                errors,
                false,
                returns_result,
                returns_result,
            );
        }
        Stmt::Let { name, value, .. } => {
            let value_ty = infer_expr(
                body,
                *value,
                ctx,
                classes,
                functions,
                errors,
                false,
                returns_result,
                returns_result,
            );
            ctx.declare(name.clone(), value_ty);
        }
        Stmt::Assign { name, value, .. } => {
            let value_ty = infer_expr(
                body,
                *value,
                ctx,
                classes,
                functions,
                errors,
                false,
                returns_result,
                returns_result,
            );
            let span = body.stmt_span(stmt_id);
            if let Some(existing) = ctx.resolve(name) {
                if types_known(&existing, &value_ty) && !is_assignable(&existing, &value_ty) {
                    errors.push(TypeError::InvalidAssignment {
                        name: name.clone(),
                        expected: type_label(&existing),
                        found: type_label(&value_ty),
                        span: span_from_range(span),
                    });
                }
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
            infer_expr(
                body,
                *condition,
                ctx,
                classes,
                functions,
                errors,
                false,
                returns_result,
                returns_result,
            );
            ctx.enter_scope();
            for stmt in then_branch {
                check_stmt(
                    body,
                    *stmt,
                    ctx,
                    classes,
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
            infer_expr(
                body,
                *subject,
                ctx,
                classes,
                functions,
                errors,
                false,
                returns_result,
                returns_result,
            );
            for case in cases {
                ctx.enter_scope();
                for label in &case.labels {
                    infer_expr(
                        body,
                        *label,
                        ctx,
                        classes,
                        functions,
                        errors,
                        false,
                        returns_result,
                        returns_result,
                    );
                }
                for stmt in &case.body {
                    check_stmt(
                        body,
                        *stmt,
                        ctx,
                        classes,
                        functions,
                        errors,
                        ret_type,
                        returns_result,
                        func_span,
                    );
                }
                ctx.exit_scope();
            }
            if let Some(branch) = otherwise {
                ctx.enter_scope();
                for stmt in branch {
                    check_stmt(
                        body,
                        *stmt,
                        ctx,
                        classes,
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
        Stmt::Use { .. } => {}
        Stmt::While {
            condition,
            body: loop_body,
        } => {
            infer_expr(
                body,
                *condition,
                ctx,
                classes,
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
                let value_ty = infer_expr(
                    body,
                    *expr,
                    ctx,
                    classes,
                    functions,
                    errors,
                    false,
                    returns_result,
                    returns_result,
                );
                if let Some(expected) = ret_type {
                    if types_known(expected, &value_ty) && !is_assignable(expected, &value_ty) {
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
                    });
                }
            } else if let Some(expected) = ret_type {
                if *expected != Type::Nil && *expected != Type::Unknown {
                    errors.push(TypeError::ReturnTypeMismatch {
                        expected: type_label(expected),
                        found: type_label(&Type::Nil),
                        span: span_from_range(body.stmt_span(stmt_id)),
                    });
                }
            }
        }
        Stmt::Break | Stmt::Continue => {}
    }
}

fn infer_expr(
    body: &Body,
    expr_id: Idx<Expr>,
    ctx: &mut TypeContext,
    classes: &ClassIndex,
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
            if matches!(op, BinaryOp::Otherwise) {
                let left = infer_expr(
                    body,
                    *lhs,
                    ctx,
                    classes,
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
                    functions,
                    errors,
                    false,
                    allow_result,
                    in_result_fn,
                );
                match left {
                    Type::Result(ok, _err) => {
                        if types_known(&ok, &right) && !is_assignable(&ok, &right) {
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
        Expr::Crash { expr } => {
            let _ = infer_expr(
                body,
                *expr,
                ctx,
                classes,
                functions,
                errors,
                false,
                allow_result,
                in_result_fn,
            );
            Type::Never
        }
        Expr::Call { callee, args } => {
            for arg in args {
                match arg {
                    crate::hir::Arg::Positional { value, .. } => {
                        infer_expr(
                            body,
                            *value,
                            ctx,
                            classes,
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
                        check_class_init_args(
                            body,
                            expr_id,
                            args,
                            class,
                            ctx,
                            classes,
                            functions,
                            errors,
                            allow_result,
                            in_result_fn,
                        );
                    }
                    ret_ty = Some(Type::Named(name.clone()));
                    valid_callee = true;
                }
                if let Some(function) = functions.get(name) {
                    check_call_args(
                        body,
                        expr_id,
                        args,
                        &function.params,
                        ctx,
                        classes,
                        functions,
                        errors,
                        allow_result,
                        in_result_fn,
                    );
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
            if ret_ty.is_none() {
                if let Expr::Member {
                    object,
                    member,
                    member_span,
                } = &body.exprs[*callee]
                {
                    handled_member = true;
                    if is_pool_of_member(body, *object, member) {
                        ret_ty = Some(Type::Unknown);
                        valid_callee = true;
                    }
                    let object_ty = infer_expr(
                        body,
                        *object,
                        ctx,
                        classes,
                        functions,
                        errors,
                        false,
                        allow_result,
                        in_result_fn,
                    );
                    match object_ty {
                        Type::Actor(inner) => {
                            if let Type::Named(class_name) = *inner {
                                if let Some(class) = classes.get(&class_name) {
                                    if let Some(method) = class.methods.get(member) {
                                        check_call_args(
                                            body,
                                            expr_id,
                                            args,
                                            &method.params,
                                            ctx,
                                            classes,
                                            functions,
                                            errors,
                                            allow_result,
                                            in_result_fn,
                                        );
                                        ret_ty = Some(Type::Pending(Box::new(Type::Result(
                                            Box::new(method.ret.clone()),
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
                        }
                        Type::Named(class_name) => {
                            if let Some(class) = classes.get(&class_name) {
                                if let Some(method) = class.methods.get(member) {
                                    check_call_args(
                                        body,
                                        expr_id,
                                        args,
                                        &method.params,
                                        ctx,
                                        classes,
                                        functions,
                                        errors,
                                        allow_result,
                                        in_result_fn,
                                    );
                                    ret_ty = Some(method.ret.clone());
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
            let object_ty = infer_expr(
                body,
                *object,
                ctx,
                classes,
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
            } else if let Type::Named(class_name) = object_ty {
                if let Some(class) = classes.get(&class_name) {
                    if let Some(field_ty) = class.fields.get(member) {
                        result = field_ty.clone();
                    } else if class.methods.contains_key(member) {
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
        Expr::List(items) => infer_list(
            body,
            items,
            ctx,
            classes,
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
        });
    }
    if matches!(ty, Type::Result(_, _)) && !allow_result {
        errors.push(TypeError::UnhandledResult {
            span: span_from_range(body.expr_span(expr_id)),
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
                    functions,
                    errors,
                    false,
                    allow_result,
                    in_result_fn,
                );
                if types_known(expected, &found) && !is_assignable(expected, &found) {
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
                    functions,
                    errors,
                    false,
                    allow_result,
                    in_result_fn,
                ),
                *span,
            ),
        };
        if types_known(expected, &found) && !is_assignable(expected, &found) {
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
    ctx: &mut TypeContext,
    classes: &ClassIndex,
    functions: &FunctionIndex,
    errors: &mut Vec<TypeError>,
    allow_result: bool,
    in_result_fn: bool,
) {
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
                let found = infer_expr(
                    body,
                    *value,
                    ctx,
                    classes,
                    functions,
                    errors,
                    false,
                    allow_result,
                    in_result_fn,
                );
                if types_known(&expected, &found) && !is_assignable(&expected, &found) {
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
                        functions,
                        errors,
                        false,
                        allow_result,
                        in_result_fn,
                    );
                    continue;
                };
                let found = infer_expr(
                    body,
                    *value,
                    ctx,
                    classes,
                    functions,
                    errors,
                    false,
                    allow_result,
                    in_result_fn,
                );
                if types_known(&expected, &found) && !is_assignable(&expected, &found) {
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
        Literal::Int(_) => Type::Int,
        Literal::Float(_) => Type::Float,
        Literal::Bool(_) => Type::Bool,
        Literal::String(_) => Type::String,
        Literal::Nil => Type::Nil,
    }
}

fn error_type() -> Type {
    Type::Named(SmolStr::new("Error"))
}

fn type_from_ref(ty: &TypeRef) -> Type {
    let args: Vec<Type> = ty.args.iter().map(type_from_ref).collect();
    match ty.name.as_str() {
        "Int" => Type::Int,
        "Float" => Type::Float,
        "Number" => Type::Number,
        "Bool" => Type::Bool,
        "String" => Type::String,
        "Nothing" | "Nil" => Type::Nil,
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
        _ => Type::Named(ty.name.clone()),
    }
}

fn valid_unary(op: UnaryOp, operand: &Type) -> bool {
    match op {
        UnaryOp::Neg => is_numeric(operand),
        UnaryOp::Not => *operand == Type::Bool,
        UnaryOp::BitNot => *operand == Type::Int,
        UnaryOp::Err => !matches!(operand, Type::Never),
        UnaryOp::Await | UnaryOp::Spawn | UnaryOp::Fire => true,
    }
}

fn unary_result(op: UnaryOp, operand: &Type) -> Type {
    match op {
        UnaryOp::Neg => operand.clone(),
        UnaryOp::Not => Type::Bool,
        UnaryOp::BitNot => Type::Int,
        UnaryOp::Err => Type::Result(Box::new(Type::Unknown), Box::new(operand.clone())),
        UnaryOp::Await | UnaryOp::Spawn | UnaryOp::Fire => Type::Unknown,
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
        BinaryOp::And | BinaryOp::Or => *left == Type::Bool && *right == Type::Bool,
        BinaryOp::Otherwise => true,
        BinaryOp::BitAnd | BinaryOp::BitOr | BinaryOp::BitXor | BinaryOp::Shl | BinaryOp::Shr => {
            *left == Type::Int && *right == Type::Int
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
        BinaryOp::Eq | BinaryOp::Ne => Type::Bool,
        BinaryOp::Lt | BinaryOp::Gt | BinaryOp::Le | BinaryOp::Ge => Type::Bool,
        BinaryOp::And | BinaryOp::Or => Type::Bool,
        BinaryOp::Otherwise => Type::Unknown,
        BinaryOp::BitAnd | BinaryOp::BitOr | BinaryOp::BitXor | BinaryOp::Shl | BinaryOp::Shr => {
            Type::Int
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
    } else if *left == Type::Int && *right == Type::Int {
        Type::Int
    } else {
        Type::Unknown
    }
}

fn is_numeric(ty: &Type) -> bool {
    matches!(ty, Type::Int | Type::Float | Type::Number)
}

fn is_assignable(expected: &Type, found: &Type) -> bool {
    if expected == found {
        return true;
    }
    match (expected, found) {
        (_, Type::Never) => true,
        (Type::Result(ok_e, err_e), Type::Result(ok_f, err_f)) => {
            is_assignable(ok_e, ok_f) && is_assignable(err_e, err_f)
        }
        (Type::Number, ty) if is_numeric(ty) => true,
        (Type::Float, Type::Int) => true,
        _ => false,
    }
}

fn types_known(left: &Type, right: &Type) -> bool {
    is_known(left) && is_known(right)
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
        Type::Int => "Int".to_string(),
        Type::Float => "Float".to_string(),
        Type::Number => "Number".to_string(),
        Type::Bool => "Bool".to_string(),
        Type::String => "String".to_string(),
        Type::Nil => "Nothing".to_string(),
        Type::List(inner) => format!("List[{}]", type_label(inner)),
        Type::Map(key, value) => format!("Map[{}, {}]", type_label(key), type_label(value)),
        Type::Named(name) => name.to_string(),
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
        UnaryOp::Await => "await",
        UnaryOp::Spawn => "spawn",
        UnaryOp::Fire => "fire",
        UnaryOp::Err => "err",
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
                Type::Actor(Box::new(Type::Named(name.clone())))
            } else {
                Type::Unknown
            }
        }
        Expr::Call { callee, args } => {
            if is_pool_of_call(body, *callee) {
                if let Some(class_name) = pool_of_class_name(body, args, classes) {
                    return Type::Actor(Box::new(Type::Named(class_name)));
                }
            }
            if let Expr::Variable(name) = &body.exprs[*callee] {
                if classes.is_class(name) {
                    Type::Actor(Box::new(Type::Named(name.clone())))
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
        if needs_actor {
            if let Some(method_id) = class
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
        Stmt::Let { value, .. } | Stmt::Assign { value, .. } => visit_expr_for_async(
            body,
            *value,
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
                for label in &case.labels {
                    visit_expr_for_async(
                        body,
                        *label,
                        fn_info,
                        function_by_name,
                        class_method_ids,
                        has_await,
                        calls,
                    );
                }
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
        Expr::Call { callee, args } => {
            if let Expr::Variable(name) = &body.exprs[*callee] {
                if let Some(target) = function_by_name.get(name) {
                    calls.push(*target);
                }
            } else if let Expr::Member { object, member, .. } = &body.exprs[*callee] {
                if let Some(fn_info) = fn_info {
                    if let Some(obj_ty) = fn_info.expr_types.get(&object.into_raw()) {
                        if let Type::Named(class_name) = obj_ty {
                            if let Some(methods) = class_method_ids.get(class_name) {
                                if let Some(method_id) = methods.get(member) {
                                    calls.push(*method_id);
                                }
                            }
                        }
                    }
                }
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
        Stmt::Let { value, .. } | Stmt::Assign { value, .. } => check_expr_async_usage(
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
                for label in &case.labels {
                    check_expr_async_usage(
                        body,
                        *label,
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
        Expr::Call { callee, args } => {
            if let Expr::Variable(name) = &body.exprs[*callee] {
                if !in_detach && classes.is_class(name) {
                    if class_requires_actor.get(name).copied().unwrap_or(false) {
                        errors.push(TypeError::AsyncClassRequiresActor {
                            class: name.clone(),
                            span: span_from_range(body.expr_span(*callee)),
                            help: class_trace.get(name).cloned().unwrap_or_else(|| {
                                "Use `detach` or `Pool.of(...)` to create an actor instance."
                                    .to_string()
                            }),
                        });
                    }
                }
            } else if let Expr::Member {
                object,
                member,
                member_span,
            } = &body.exprs[*callee]
            {
                if let Some(fn_info) = fn_info {
                    if let Some(obj_ty) = fn_info.expr_types.get(&object.into_raw()) {
                        if let Type::Named(class_name) = obj_ty {
                            if let Some(methods) = class_method_ids.get(class_name) {
                                if let Some(method_id) = methods.get(member) {
                                    if *requires_actor.get(&method_id.into_raw()).unwrap_or(&false)
                                    {
                                        let hint = "Call this method on a detached or pooled actor instance.";
                                        let trace =
                                            build_call_chain(*method_id, cause, func_labels, hint);
                                        errors.push(TypeError::AsyncMethodRequiresActor {
                                            class: class_name.clone(),
                                            member: member.clone(),
                                            span: span_from_range(*member_span),
                                            help: trace,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
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
        let input = "to f():\n    return 1 + true";
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
        let input = "to f():\n    return not 1";
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
    fn test_param_type_used() {
        let input = "to f(x: Int):\n    return x + 1";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_param_type_mismatch() {
        let input = "to f(x: Int):\n    return x + true";
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
    fn test_string_concat_allowed() {
        let input = "to f():\n    return \"a\" + \"b\"";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_assignment_type_mismatch() {
        let input = "to f(x: String):\n    x += 1";
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
        let input = "to f() -> Bool:\n    return 1";
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
A Whale:\n    has:\n        name: String\n\nto f(w: Whale):\n    return w.age\n";
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
A Whale:\n    can swim(distance: Int) -> Bool:\n        return true\n\nto f(w: Whale):\n    return w.swim(true)\n";
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
    fn test_function_call_checked() {
        let input = "\
to add(a: Int, b: Int) -> Int:\n    return a + b\n\nto f():\n    return add(1, true)\n";
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
to f():\n    x = 1\n    x(2)\n";
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
A Ocean:\n    has:\n        depth: Int\n\nA Whale:\n    can ocean() -> Ocean:\n        return Ocean()\n\nto f(w: Whale) -> Int:\n    return w.ocean().depth\n";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_actor_call_requires_await_or_fire() {
        let input = "\
A Whale:\n    can swim() -> Bool:\n        return true\n\nto f():\n    w = detach Whale() * 1\n    w.swim()\n";
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
    fn test_err_requires_result_function() {
        let input = "to f():\n    err \"nope\"";
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
        let input = "to f() -> Result:\n    return err \"nope\" otherwise 0";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_invalid_otherwise_operand() {
        let input = "to f():\n    return 1 otherwise 0";
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
        let input = "to f():\n    -true";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        let err = errors
            .iter()
            .find(|err| matches!(err, TypeError::InvalidUnaryOperand { .. }))
            .expect("missing invalid unary operand error");
        if let TypeError::InvalidUnaryOperand { span, .. } = err {
            let expected = input.find('-').unwrap();
            assert_eq!(span.offset(), expected);
            assert_eq!(span.len(), 1);
        }
    }

    #[test]
    fn test_invalid_binary_operand_span() {
        let input = "to f():\n    true + 1";
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
A Foo:\n    has:\n        x: Int\n\nto f():\n    foo = Foo(x=1)\n    foo.bar\n";
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
A Whale:\n    can swim() -> Bool:\n        return true\n\nto f() -> Result:\n    w = detach Whale() * 1\n    return await w.swim()\n";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_builtin_fallible_requires_handling() {
        let input = "to f():\n    parse_int(\"1\")";
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
        let input = "to f() -> Int:\n    return parse_int(\"1\") otherwise 0";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_await_on_non_actor_call_errors() {
        let input = "\
A Whale:\n    can swim() -> Bool:\n        return true\n\nto f(w: Whale):\n    return await w.swim()\n";
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
A Whale:\n    can swim() -> Bool:\n        return true\n\nto f():\n    w = detach Whale() * 1\n    fire w.swim()\n";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_fire_non_actor_call_errors() {
        let input = "\
A Whale:\n    can swim() -> Bool:\n        return true\n\nto f(w: Whale):\n    fire w.swim()\n";
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
to f():\n    Whale(name=1)\n";
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
to f():\n    Whale(age=\"old\")\n";
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
A Whale:\n    can swim() -> Bool:\n        return true\n\nto f():\n    w = detach Whale() * 1\n    return await w\n";
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
A Whale:\n    can swim() -> Bool:\n        return true\n\nto f():\n    w = detach Whale() * 1\n    fire w\n";
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
A Whale:\n    can swim() -> Bool:\n        return true\n\n\
A Boat:\n    can ride() -> Bool:\n        return await Whale().swim()\n\n\
to f():\n    Boat()\n";
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
A Whale:\n    can swim() -> Bool:\n        return true\n\n\
A Boat:\n    can ride() -> Bool:\n        return await Whale().swim()\n\n\
to f():\n    b = Boat()\n    return b.ride()\n";
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
A Whale:\n    can swim() -> Bool:\n        return true\n\n\
to helper() -> Bool:\n    return await Whale().swim()\n\n\
A Boat:\n    can ride() -> Bool:\n        return helper()\n\n\
to f():\n    Boat()\n";
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
A Whale:\n    can swim() -> Bool:\n        return true\n\n\
to helper() -> Bool:\n    return await Whale().swim()\n\n\
A Boat:\n    can ride() -> Bool:\n        return helper()\n\n\
to f():\n    b = Boat()\n    return b.ride()\n";
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
A Whale:\n    can swim() -> Bool:\n        return true\n\n\
to helper() -> Bool:\n    fire Whale().swim()\n    return true\n\n\
A Boat:\n    can ride() -> Bool:\n        return helper()\n\n\
to f():\n    Boat()\n";
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
A Whale:\n    can swim() -> Bool:\n        return true\n\n\
A Boat:\n    can ride() -> Bool:\n        return await Whale().swim()\n\n\
to f():\n    b = detach Boat() * 1\n    return await b.ride()\n";
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
