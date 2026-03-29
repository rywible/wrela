fn supports_structural_value_type(
    ty: &Type,
    classes: &ClassIndex,
    enums: &EnumIndex,
    visiting: &mut HashSet<SmolStr>,
) -> bool {
    match ty {
        Type::Unknown => false,
        Type::Param(_) => true,
        Type::Never
        | Type::Integer
        | Type::Float
        | Type::Number
        | Type::Boolean
        | Type::String
        | Type::Nil => true,
        Type::List(inner) => supports_structural_value_type(inner, classes, enums, visiting),
        Type::Map(key, value) => {
            supports_structural_value_type(key, classes, enums, visiting)
                && supports_structural_value_type(value, classes, enums, visiting)
        }
        Type::Result(ok, err) => {
            supports_structural_value_type(ok, classes, enums, visiting)
                && supports_structural_value_type(err, classes, enums, visiting)
        }
        Type::Actor(_) | Type::Pending(_) => false,
        Type::Vec2 | Type::Vec3 | Type::Vec4 | Type::Mat4 => true,
        Type::GpuBuffer(_) | Type::Texture2D | Type::Sampler => true,
        Type::Named(name, args) => {
            if name.as_str() == "Bytes" {
                return true;
            }
            if let Some(class_sig) = classes.get(name) {
                if !visiting.insert(name.clone()) {
                    return true;
                }
                let mut subst = HashMap::new();
                for (param, arg) in class_sig.type_params.iter().zip(args.iter()) {
                    subst.insert(param.clone(), arg.clone());
                }
                for field_name in &class_sig.field_order {
                    let Some(field_ty) = class_sig.fields.get(field_name) else {
                        continue;
                    };
                    let resolved = substitute_type(field_ty, &subst);
                    if !supports_structural_value_type(&resolved, classes, enums, visiting) {
                        return false;
                    }
                }
                visiting.remove(name);
                return true;
            }
            if let Some(enum_sig) = enums.get(name) {
                if !visiting.insert(name.clone()) {
                    return true;
                }
                let mut subst = HashMap::new();
                for (param, arg) in enum_sig.type_params.iter().zip(args.iter()) {
                    subst.insert(param.clone(), arg.clone());
                }
                let mut variants: Vec<_> = enum_sig.variants.keys().cloned().collect();
                variants.sort_by(|a, b| a.as_str().cmp(b.as_str()));
                for variant in variants {
                    let Some(params) = enum_sig.variants.get(&variant) else {
                        continue;
                    };
                    for (_param_name, param_ty) in params {
                        let resolved = substitute_type(param_ty, &subst);
                        if !supports_structural_value_type(&resolved, classes, enums, visiting) {
                            return false;
                        }
                    }
                }
                visiting.remove(name);
                return true;
            }
            false
        }
    }
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
    let has_func_type_params = !func.type_params.is_empty();
    if has_func_type_params {
        let fn_tp_names: Vec<SmolStr> = func.type_params.iter().map(|tp| tp.name.clone()).collect();
        ctx.enter_type_params(&fn_tp_names);
    }
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
            ctx.declare(SmolStr::new("Self"), self_ty.clone());
            ctx.declare(SmolStr::new("self"), self_ty);
        } else {
            ctx.declare(
                class_name.clone(),
                Type::Named(class_name.clone(), Vec::new()),
            );
            ctx.declare(
                SmolStr::new("Self"),
                Type::Named(class_name.clone(), Vec::new()),
            );
            ctx.declare(
                SmolStr::new("self"),
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
    if has_func_type_params {
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
    #[allow(dead_code)]
    kind: FunctionKind,
    type_params: Vec<SmolStr>,
    type_param_bounds: Vec<Vec<SmolStr>>,
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
            let type_params: Vec<SmolStr> = class.type_params.iter().map(|tp| tp.name.clone()).collect();
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
            let type_params: Vec<SmolStr> = en.type_params.iter().map(|tp| tp.name.clone()).collect();
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
            let type_params: Vec<SmolStr> = interface.type_params.iter().map(|tp| tp.name.clone()).collect();
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
                if !interface_method_matches(iface_method, class_method) {
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
            let fn_type_params: Vec<SmolStr> = func.type_params.iter().map(|tp| tp.name.clone()).collect();
            let fn_type_param_set: std::collections::HashSet<SmolStr> = fn_type_params.iter().cloned().collect();
            let params = func
                .params
                .iter()
                .map(|param| {
                    (
                        param.name.clone(),
                        param
                            .ty
                            .as_ref()
                            .map(|t| type_from_ref_with_params(t, &fn_type_param_set))
                            .unwrap_or(Type::Unknown),
                    )
                })
                .collect();
            let ret = func
                .ret_type
                .as_ref()
                .map(|t| type_from_ref_with_params(t, &fn_type_param_set))
                .unwrap_or(Type::Unknown);
            let fn_type_param_bounds: Vec<Vec<SmolStr>> = func.type_params.iter().map(|tp| tp.bounds.clone()).collect();
            functions.insert(
                func.name.clone(),
                FunctionSig {
                    params,
                    ret,
                    kind: func.kind,
                    type_params: fn_type_params,
                    type_param_bounds: fn_type_param_bounds,
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
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_print"),
            FunctionSig {
                params: vec![(SmolStr::new("value"), Type::String)],
                ret: Type::Nil,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_bytes_from_string"),
            FunctionSig {
                params: vec![(SmolStr::new("value"), Type::String)],
                ret: Type::Named(SmolStr::new("Bytes"), Vec::new()),
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_bytes_from_list"),
            FunctionSig {
                params: vec![(SmolStr::new("value"), Type::List(Box::new(Type::Integer)))],
                ret: Type::Named(SmolStr::new("Bytes"), Vec::new()),
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
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
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
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
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
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
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
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
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
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
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
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
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
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
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_web_parse_json_text"),
            FunctionSig {
                params: vec![(SmolStr::new("text"), Type::String)],
                ret: Type::Result(
                    Box::new(Type::Map(Box::new(Type::Unknown), Box::new(Type::Unknown))),
                    Box::new(err.clone()),
                ),
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_web_render_json_text"),
            FunctionSig {
                params: vec![(SmolStr::new("value"), Type::Unknown)],
                ret: Type::Result(Box::new(Type::String), Box::new(err.clone())),
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_auth_hash_password"),
            FunctionSig {
                params: vec![(SmolStr::new("password"), Type::String)],
                ret: Type::Result(Box::new(Type::String), Box::new(err.clone())),
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_auth_verify_password_hash"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("password"), Type::String),
                    (SmolStr::new("hashed_password"), Type::String),
                ],
                ret: Type::Result(Box::new(Type::Boolean), Box::new(err.clone())),
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_auth_sign_jwt"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("claims_json"), Type::String),
                    (SmolStr::new("key_id"), Type::String),
                ],
                ret: Type::Result(Box::new(Type::String), Box::new(err.clone())),
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_auth_verify_jwt"),
            FunctionSig {
                params: vec![(SmolStr::new("token"), Type::String)],
                ret: Type::Result(Box::new(Type::String), Box::new(err.clone())),
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_auth_generate_secure_token"),
            FunctionSig {
                params: vec![(SmolStr::new("byte_length"), Type::Integer)],
                ret: Type::Result(Box::new(Type::String), Box::new(err.clone())),
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_auth_render_jwks_document"),
            FunctionSig {
                params: vec![],
                ret: Type::Result(Box::new(Type::String), Box::new(err.clone())),
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
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
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_list_get"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("list"), Type::List(Box::new(Type::Unknown))),
                    (SmolStr::new("index"), Type::Integer),
                ],
                ret: Type::Unknown,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_list_set"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("list"), Type::List(Box::new(Type::Unknown))),
                    (SmolStr::new("index"), Type::Integer),
                    (SmolStr::new("value"), Type::Unknown),
                ],
                ret: Type::Nil,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_list_len"),
            FunctionSig {
                params: vec![(SmolStr::new("list"), Type::List(Box::new(Type::Unknown)))],
                ret: Type::Integer,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_map_new"),
            FunctionSig {
                params: vec![],
                ret: Type::Map(Box::new(Type::Unknown), Box::new(Type::Unknown)),
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
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
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
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
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
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
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_str_len"),
            FunctionSig {
                params: vec![(SmolStr::new("text"), Type::String)],
                ret: Type::Integer,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
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
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_log_configure"),
            FunctionSig {
                params: vec![(SmolStr::new("config"), Type::Unknown)],
                ret: Type::Nil,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_runtime_cpu_count"),
            FunctionSig {
                params: vec![],
                ret: Type::Integer,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_reactor_new"),
            FunctionSig {
                params: vec![],
                ret: Type::Integer,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_reactor_drop"),
            FunctionSig {
                params: vec![(SmolStr::new("reactor"), Type::Integer)],
                ret: Type::Boolean,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
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
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
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
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
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
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_task_signal_new"),
            FunctionSig {
                params: vec![],
                ret: Type::Integer,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_task_signal_drop"),
            FunctionSig {
                params: vec![(SmolStr::new("signal"), Type::Integer)],
                ret: Type::Boolean,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_task_unpark_one"),
            FunctionSig {
                params: vec![(SmolStr::new("signal"), Type::Integer)],
                ret: Type::Boolean,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_task_unpark_all"),
            FunctionSig {
                params: vec![(SmolStr::new("signal"), Type::Integer)],
                ret: Type::Boolean,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_task_epoch"),
            FunctionSig {
                params: vec![(SmolStr::new("signal"), Type::Integer)],
                ret: Type::Integer,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_atomic_i64_new"),
            FunctionSig {
                params: vec![(SmolStr::new("initial"), Type::Integer)],
                ret: Type::Integer,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_atomic_i64_drop"),
            FunctionSig {
                params: vec![(SmolStr::new("atomic"), Type::Integer)],
                ret: Type::Boolean,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_atomic_i64_load"),
            FunctionSig {
                params: vec![(SmolStr::new("atomic"), Type::Integer)],
                ret: Type::Integer,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
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
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
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
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_pool_size"),
            FunctionSig {
                params: vec![(SmolStr::new("handle"), Type::Unknown)],
                ret: Type::Integer,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_pool_rr"),
            FunctionSig {
                params: vec![(SmolStr::new("handle"), Type::Unknown)],
                ret: Type::Integer,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_pool_queue_len"),
            FunctionSig {
                params: vec![(SmolStr::new("handle"), Type::Unknown)],
                ret: Type::Integer,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_actor_mailbox_len"),
            FunctionSig {
                params: vec![(SmolStr::new("handle"), Type::Unknown)],
                ret: Type::Integer,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_actor_pause"),
            FunctionSig {
                params: vec![(SmolStr::new("handle"), Type::Unknown)],
                ret: Type::Nil,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_actor_resume"),
            FunctionSig {
                params: vec![(SmolStr::new("handle"), Type::Unknown)],
                ret: Type::Nil,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_actor_pause_wait"),
            FunctionSig {
                params: vec![(SmolStr::new("handle"), Type::Unknown)],
                ret: Type::Nil,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_actor_fire_burst_begin"),
            FunctionSig {
                params: vec![(SmolStr::new("handle"), Type::Unknown)],
                ret: Type::Nil,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_actor_fire_burst_end"),
            FunctionSig {
                params: vec![(SmolStr::new("handle"), Type::Unknown)],
                ret: Type::Nil,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_actor_fire_burst_abort"),
            FunctionSig {
                params: vec![(SmolStr::new("handle"), Type::Unknown)],
                ret: Type::Nil,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_metrics_get"),
            FunctionSig {
                params: vec![(SmolStr::new("id"), Type::Integer)],
                ret: Type::Integer,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_metrics_dropped_paused_id"),
            FunctionSig {
                params: vec![],
                ret: Type::Integer,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_metrics_messages_dropped_id"),
            FunctionSig {
                params: vec![],
                ret: Type::Integer,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_metrics_web_writev_calls_id"),
            FunctionSig {
                params: vec![],
                ret: Type::Integer,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_metrics_web_sendfile_calls_id"),
            FunctionSig {
                params: vec![],
                ret: Type::Integer,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_clock_ns"),
            FunctionSig {
                params: vec![],
                ret: Type::Integer,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_sleep_ms"),
            FunctionSig {
                params: vec![(SmolStr::new("ms"), Type::Integer)],
                ret: Type::Pending(Box::new(Type::Nil)),
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_env_get"),
            FunctionSig {
                params: vec![(SmolStr::new("key"), Type::String)],
                ret: Type::Unknown,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
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
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_runtime_configure"),
            FunctionSig {
                params: vec![(SmolStr::new("config"), Type::Unknown)],
                ret: Type::Nil,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_db_core_open"),
            FunctionSig {
                params: vec![(SmolStr::new("path"), Type::String)],
                ret: Type::Integer,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_db_core_close"),
            FunctionSig {
                params: vec![(SmolStr::new("handle"), Type::Integer)],
                ret: Type::Boolean,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_db_core_submit_batch"),
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
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_db_core_read_point"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("handle"), Type::Integer),
                    (SmolStr::new("namespace"), Type::String),
                    (SmolStr::new("key"), Type::String),
                ],
                ret: Type::Unknown,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_db_core_read_range"),
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
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_db_core_txn_begin"),
            FunctionSig {
                params: vec![(SmolStr::new("handle"), Type::Integer)],
                ret: Type::Integer,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_db_core_txn_prepare"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("handle"), Type::Integer),
                    (SmolStr::new("txn"), Type::Integer),
                ],
                ret: Type::Boolean,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_db_core_txn_commit"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("handle"), Type::Integer),
                    (SmolStr::new("txn"), Type::Integer),
                ],
                ret: Type::Boolean,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_db_core_txn_abort"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("handle"), Type::Integer),
                    (SmolStr::new("txn"), Type::Integer),
                ],
                ret: Type::Boolean,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_db_admin_snapshot_start"),
            FunctionSig {
                params: vec![(SmolStr::new("handle"), Type::Integer)],
                ret: Type::Integer,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_db_admin_snapshot_status"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("handle"), Type::Integer),
                    (SmolStr::new("snapshot"), Type::Integer),
                ],
                ret: Type::Integer,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_db_admin_restore"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("handle"), Type::Integer),
                    (SmolStr::new("snapshot"), Type::Integer),
                ],
                ret: Type::Boolean,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_db_admin_checkpoint_create"),
            FunctionSig {
                params: vec![(SmolStr::new("handle"), Type::Integer)],
                ret: Type::Boolean,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_db_admin_checkpoint_restore_latest"),
            FunctionSig {
                params: vec![(SmolStr::new("handle"), Type::Integer)],
                ret: Type::Boolean,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_db_admin_schema_epoch_set"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("handle"), Type::Integer),
                    (SmolStr::new("epoch"), Type::Integer),
                ],
                ret: Type::Boolean,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_db_admin_schema_set_all_voters_on_target_binary"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("handle"), Type::Integer),
                    (SmolStr::new("ready"), Type::Boolean),
                ],
                ret: Type::Boolean,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_db_admin_autoscale_tick"),
            FunctionSig {
                params: vec![(SmolStr::new("handle"), Type::Integer)],
                ret: Type::Unknown,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_db_admin_plan_rehome"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("handle"), Type::Integer),
                    (SmolStr::new("keyrange"), Type::String),
                    (SmolStr::new("target_region"), Type::String),
                    (SmolStr::new("reason"), Type::String),
                ],
                ret: Type::String,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_db_admin_advance_rehome"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("handle"), Type::Integer),
                    (SmolStr::new("job_id"), Type::String),
                    (SmolStr::new("phase_ack"), Type::Unknown),
                ],
                ret: Type::String,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_db_admin_promote_async_failover"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("handle"), Type::Integer),
                    (SmolStr::new("keyrange"), Type::String),
                    (SmolStr::new("region"), Type::String),
                    (SmolStr::new("expected_epoch"), Type::Integer),
                ],
                ret: Type::String,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_db_explain_checkpoint_count"),
            FunctionSig {
                params: vec![(SmolStr::new("handle"), Type::Integer)],
                ret: Type::Integer,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_db_explain_schema_epoch_get"),
            FunctionSig {
                params: vec![(SmolStr::new("handle"), Type::Integer)],
                ret: Type::Integer,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_db_explain_health_has_checkpoint_or_schema_error"),
            FunctionSig {
                params: vec![(SmolStr::new("handle"), Type::Integer)],
                ret: Type::Boolean,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_db_explain_private_mesh_status"),
            FunctionSig {
                params: vec![(SmolStr::new("handle"), Type::Integer)],
                ret: Type::Map(Box::new(Type::Unknown), Box::new(Type::Unknown)),
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_db_explain_logical_shard_count"),
            FunctionSig {
                params: vec![(SmolStr::new("handle"), Type::Integer)],
                ret: Type::Integer,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_db_explain_active_group_count"),
            FunctionSig {
                params: vec![(SmolStr::new("handle"), Type::Integer)],
                ret: Type::Integer,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_db_explain_autoscale_status"),
            FunctionSig {
                params: vec![(SmolStr::new("handle"), Type::Integer)],
                ret: Type::Map(Box::new(Type::Unknown), Box::new(Type::Unknown)),
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_db_explain_topology_status"),
            FunctionSig {
                params: vec![(SmolStr::new("handle"), Type::Integer)],
                ret: Type::Map(Box::new(Type::Unknown), Box::new(Type::Unknown)),
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_db_explain_shard_map_epoch"),
            FunctionSig {
                params: vec![(SmolStr::new("handle"), Type::Integer)],
                ret: Type::Integer,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_db_explain_shard_for_key"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("handle"), Type::Integer),
                    (SmolStr::new("namespace"), Type::String),
                    (SmolStr::new("key"), Type::String),
                ],
                ret: Type::Integer,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_db_explain_resolve_owner"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("handle"), Type::Integer),
                    (SmolStr::new("namespace"), Type::String),
                    (SmolStr::new("key"), Type::String),
                ],
                ret: Type::String,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_db_explain_global_route_lookup"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("handle"), Type::Integer),
                    (SmolStr::new("namespace"), Type::String),
                    (SmolStr::new("key"), Type::String),
                ],
                ret: Type::String,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
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
