use crate::portable::builtin_records;

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
        | Type::I32
        | Type::U32
        | Type::I64
        | Type::U64
        | Type::Float
        | Type::F32
        | Type::Number
        | Type::Boolean
        | Type::String
        | Type::Nil => true,
        Type::List(inner) => supports_structural_value_type(inner, classes, enums, visiting),
        Type::Array(inner, _) => supports_structural_value_type(inner, classes, enums, visiting),
        Type::Map(key, value) => {
            supports_structural_value_type(key, classes, enums, visiting)
                && supports_structural_value_type(value, classes, enums, visiting)
        }
        Type::Result(ok, err) => {
            supports_structural_value_type(ok, classes, enums, visiting)
                && supports_structural_value_type(err, classes, enums, visiting)
        }
        Type::Actor(_) | Type::Pending(_) => false,
        Type::Vec2 | Type::Vec3 | Type::Vec4 | Type::Mat3 | Type::Mat4 | Type::Quat => true,
        Type::GpuBuffer(_)
        | Type::GpuAtomicI32
        | Type::GpuAtomicU32
        | Type::GpuDispatchSchedule
        | Type::Texture2D
        | Type::Sampler => true,
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
    ctx.set_function_lane(func.lane());
    ctx.set_function_role(func.role);
    ctx.set_function_name(func.name.clone());
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
        if matches!(func.role, FunctionRole::Region) {
            if !body.root_stmts.is_empty() {
                errors.push(TypeError::PortableConstructForbidden {
                    function: func.name.clone(),
                    construct: "an executable region body".to_string(),
                    span: span_from_option_range(func.name_span),
                    help: "Region declarations are declarative scene partitions only; keep the body empty and move executable logic into ordinary functions.".to_string(),
                });
            }
        } else {
            let forbidden_world_return = match func.role {
                FunctionRole::Domain | FunctionRole::Render => {
                    body_contains_forbidden_world_return(body, false)
                }
                _ => false,
            };
            if forbidden_world_return {
                errors.push(TypeError::PortableConstructForbidden {
                    function: func.name.clone(),
                    construct: "an explicit `return` in a domain/render declaration".to_string(),
                    span: span_from_option_range(func.name_span),
                    help: "Domain and render declarations stay metadata-only. Keep the body to compiler-understood policy assignments only.".to_string(),
                });
            }
            func.visit_analysis_bodies(|body| {
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
            });
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

fn body_contains_forbidden_world_return(body: &Body, allow_terminal_top_level_return: bool) -> bool {
    body.root_stmts.iter().enumerate().any(|(index, stmt)| {
        let allow_here = allow_terminal_top_level_return && index + 1 == body.root_stmts.len();
        stmt_contains_forbidden_world_return(body, *stmt, allow_here, true)
    })
}

fn stmt_contains_forbidden_world_return(
    body: &Body,
    stmt_id: Idx<Stmt>,
    allow_here: bool,
    top_level: bool,
) -> bool {
    match &body.stmts[stmt_id] {
        Stmt::Return(_) => !(allow_here && top_level),
        Stmt::Optimize { body: inner, .. }
        | Stmt::If {
            then_branch: inner,
            else_branch: None,
            ..
        }
        | Stmt::For { body: inner, .. }
        | Stmt::While { body: inner, .. } => inner
            .iter()
            .copied()
            .any(|stmt| stmt_contains_forbidden_world_return(body, stmt, false, false)),
        Stmt::If {
            then_branch,
            else_branch: Some(else_branch),
            ..
        } => then_branch
            .iter()
            .copied()
            .any(|stmt| stmt_contains_forbidden_world_return(body, stmt, false, false))
            || else_branch
                .iter()
                .copied()
                .any(|stmt| stmt_contains_forbidden_world_return(body, stmt, false, false)),
        Stmt::Match {
            cases,
            otherwise,
            ..
        } => {
            cases.iter().any(|case| {
                case.body
                    .iter()
                    .copied()
                    .any(|stmt| stmt_contains_forbidden_world_return(body, stmt, false, false))
            }) || otherwise.as_ref().is_some_and(|branch| {
                branch
                    .iter()
                    .copied()
                    .any(|stmt| stmt_contains_forbidden_world_return(body, stmt, false, false))
            })
        }
        _ => false,
    }
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
    role: ClassRole,
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
            let type_params: Vec<SmolStr> =
                class.type_params.iter().map(|tp| tp.name.clone()).collect();
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
                    role: class.role,
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
        for record in builtin_records() {
            classes.entry(SmolStr::new(record.name)).or_insert_with(|| {
                let mut fields = HashMap::new();
                let mut field_mutable = HashMap::new();
                let mut field_order = Vec::new();
                for field in record.fields {
                    let field_name = SmolStr::new(field.name);
                    fields.insert(field_name.clone(), portable_builtin_type_to_type(field.ty));
                    field_mutable.insert(field_name.clone(), false);
                    field_order.push(field_name);
                }
                ClassSig {
                    role: ClassRole::Value,
                    type_params: Vec::new(),
                    fields,
                    field_mutable,
                    methods: HashMap::new(),
                    field_order,
                    implements: Vec::new(),
                    name_span: None,
                }
            });
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
            let type_params: Vec<SmolStr> =
                en.type_params.iter().map(|tp| tp.name.clone()).collect();
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
            let type_params: Vec<SmolStr> = interface
                .type_params
                .iter()
                .map(|tp| tp.name.clone())
                .collect();
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
    portable_functions: HashSet<SmolStr>,
    kernel_functions: HashSet<SmolStr>,
    field_functions: HashSet<SmolStr>,
    shape_functions: HashSet<SmolStr>,
    region_functions: HashSet<SmolStr>,
    domain_functions: HashSet<SmolStr>,
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
        let mut portable_functions = HashSet::new();
        let mut kernel_functions = HashSet::new();
        let mut field_functions = HashSet::new();
        let mut shape_functions = HashSet::new();
        let mut region_functions = HashSet::new();
        let mut domain_functions = HashSet::new();
        for (idx, func) in module.functions.iter() {
            if method_ids.contains(&idx.into_raw()) {
                continue;
            }
            let fn_type_params: Vec<SmolStr> =
                func.type_params.iter().map(|tp| tp.name.clone()).collect();
            let fn_type_param_set: std::collections::HashSet<SmolStr> =
                fn_type_params.iter().cloned().collect();
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
            let fn_type_param_bounds: Vec<Vec<SmolStr>> = func
                .type_params
                .iter()
                .map(|tp| tp.bounds.clone())
                .collect();
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
            if matches!(func.lane(), FunctionLane::Portable) {
                portable_functions.insert(func.name.clone());
            }
            if matches!(func.role, FunctionRole::Kernel) {
                kernel_functions.insert(func.name.clone());
            }
            if matches!(func.role, FunctionRole::Field) && func.field.is_some() {
                field_functions.insert(func.name.clone());
            }
            if matches!(func.role, FunctionRole::Region) {
                region_functions.insert(func.name.clone());
            }
            if matches!(func.role, FunctionRole::Domain) {
                domain_functions.insert(func.name.clone());
            }
        }
        for (_idx, shape) in module.shapes.iter() {
            shape_functions.insert(shape.name.clone());
        }
        for (name, sig) in builtin_functions() {
            if builtin_function_is_portable(name.as_str()) {
                portable_functions.insert(name.clone());
            }
            functions.entry(name).or_insert(sig);
        }
        Self {
            functions,
            portable_functions,
            kernel_functions,
            field_functions,
            shape_functions,
            region_functions,
            domain_functions,
        }
    }

    fn get(&self, name: &SmolStr) -> Option<&FunctionSig> {
        self.functions.get(name)
    }

    fn is_portable(&self, name: &SmolStr) -> bool {
        self.portable_functions.contains(name)
    }

    fn is_kernel(&self, name: &SmolStr) -> bool {
        self.kernel_functions.contains(name)
    }

    fn is_field(&self, name: &SmolStr) -> bool {
        self.field_functions.contains(name)
    }

    fn is_shape(&self, name: &SmolStr) -> bool {
        self.shape_functions.contains(name)
    }

    fn is_region(&self, name: &SmolStr) -> bool {
        self.region_functions.contains(name)
    }

    fn is_domain(&self, name: &SmolStr) -> bool {
        self.domain_functions.contains(name)
    }
}

fn builtin_function_is_portable(name: &str) -> bool {
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
            | "sphere"
            | "box"
            | "capsule"
            | "cylinder"
            | "plane"
            | "torus"
            | "circle2"
            | "rect2"
            | "rounded_rect2"
            | "capsule2"
            | "segment2"
            | "polygon2"
            | "polyline2"
            | "field_sweep_coords"
            | "__wr_primitive_sphere"
            | "__wr_primitive_box"
            | "__wr_primitive_capsule"
            | "__wr_primitive_cylinder"
            | "__wr_primitive_plane"
            | "__wr_primitive_torus"
            | "gpu_buffer_len"
            | "gpu_buffer_get"
            | "gpu_buffer_set"
            | "gpu_atomic_i32_load"
            | "gpu_atomic_i32_store"
            | "gpu_atomic_i32_fetch_add"
            | "gpu_atomic_u32_load"
            | "gpu_atomic_u32_store"
            | "gpu_atomic_u32_fetch_add"
            | "workgroup_barrier"
            | "storage_barrier"
            | "global_invocation_id"
            | "local_invocation_id"
            | "workgroup_id"
            | "num_workgroups"
            | "workgroup_size"
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
    )
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
            SmolStr::new("__wr_metrics_scene_trace_id"),
            FunctionSig {
                params: vec![],
                ret: Type::Integer,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_metrics_field_sample_id"),
            FunctionSig {
                params: vec![],
                ret: Type::Integer,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_metrics_scene_trace_support_pruned_branch_id"),
            FunctionSig {
                params: vec![],
                ret: Type::Integer,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_metrics_scene_trace_candidate_branch_id"),
            FunctionSig {
                params: vec![],
                ret: Type::Integer,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_metrics_scene_trace_exact_path_id"),
            FunctionSig {
                params: vec![],
                ret: Type::Integer,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_metrics_scene_trace_conservative_path_id"),
            FunctionSig {
                params: vec![],
                ret: Type::Integer,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_metrics_scene_trace_hit_count_id"),
            FunctionSig {
                params: vec![],
                ret: Type::Integer,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_metrics_scene_trace_hit_steps_total_id"),
            FunctionSig {
                params: vec![],
                ret: Type::Integer,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_metrics_scene_trace_hit_field_samples_total_id"),
            FunctionSig {
                params: vec![],
                ret: Type::Integer,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_metrics_scene_trace_steps_le_1_id"),
            FunctionSig {
                params: vec![],
                ret: Type::Integer,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_metrics_scene_trace_steps_le_4_id"),
            FunctionSig {
                params: vec![],
                ret: Type::Integer,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_metrics_scene_trace_steps_le_8_id"),
            FunctionSig {
                params: vec![],
                ret: Type::Integer,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_metrics_scene_trace_steps_le_16_id"),
            FunctionSig {
                params: vec![],
                ret: Type::Integer,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_metrics_scene_trace_steps_gt_16_id"),
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
            SmolStr::new("vec2"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("x"), Type::F32),
                    (SmolStr::new("y"), Type::F32),
                ],
                ret: Type::Vec2,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("vec3"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("x"), Type::F32),
                    (SmolStr::new("y"), Type::F32),
                    (SmolStr::new("z"), Type::F32),
                ],
                ret: Type::Vec3,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("vec4"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("x"), Type::F32),
                    (SmolStr::new("y"), Type::F32),
                    (SmolStr::new("z"), Type::F32),
                    (SmolStr::new("w"), Type::F32),
                ],
                ret: Type::Vec4,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("quat"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("x"), Type::F32),
                    (SmolStr::new("y"), Type::F32),
                    (SmolStr::new("z"), Type::F32),
                    (SmolStr::new("w"), Type::F32),
                ],
                ret: Type::Quat,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("mat3_identity"),
            FunctionSig {
                params: vec![],
                ret: Type::Mat3,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("mat3_cols"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("c0"), Type::Vec3),
                    (SmolStr::new("c1"), Type::Vec3),
                    (SmolStr::new("c2"), Type::Vec3),
                ],
                ret: Type::Mat3,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("mat4_identity"),
            FunctionSig {
                params: vec![],
                ret: Type::Mat4,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("mat4_cols"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("c0"), Type::Vec4),
                    (SmolStr::new("c1"), Type::Vec4),
                    (SmolStr::new("c2"), Type::Vec4),
                    (SmolStr::new("c3"), Type::Vec4),
                ],
                ret: Type::Mat4,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("bounds2"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("min"), Type::Vec2),
                    (SmolStr::new("max"), Type::Vec2),
                ],
                ret: portable_named_type("Bounds2"),
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("bounds3"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("min"), Type::Vec3),
                    (SmolStr::new("max"), Type::Vec3),
                ],
                ret: portable_named_type("Bounds3"),
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("ray3"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("origin"), Type::Vec3),
                    (SmolStr::new("direction"), Type::Vec3),
                ],
                ret: portable_named_type("Ray3"),
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("transform3"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("matrix"), Type::Mat4),
                    (SmolStr::new("inverse"), Type::Mat4),
                ],
                ret: portable_named_type("Transform3"),
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("capture"),
            FunctionSig {
                params: vec![(SmolStr::new("scene"), Type::Unknown)],
                ret: Type::Unknown,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("transform3_identity"),
            FunctionSig {
                params: vec![],
                ret: portable_named_type("Transform3"),
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("gpu_buffer_new"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("length"), Type::Integer),
                    (SmolStr::new("default_value"), Type::Unknown),
                ],
                ret: Type::GpuBuffer(Box::new(Type::Unknown)),
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("gpu_buffer_len"),
            FunctionSig {
                params: vec![(
                    SmolStr::new("buffer"),
                    Type::GpuBuffer(Box::new(Type::Unknown)),
                )],
                ret: Type::Integer,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("gpu_buffer_get"),
            FunctionSig {
                params: vec![
                    (
                        SmolStr::new("buffer"),
                        Type::GpuBuffer(Box::new(Type::Unknown)),
                    ),
                    (SmolStr::new("index"), Type::Unknown),
                ],
                ret: Type::Unknown,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("gpu_buffer_set"),
            FunctionSig {
                params: vec![
                    (
                        SmolStr::new("buffer"),
                        Type::GpuBuffer(Box::new(Type::Unknown)),
                    ),
                    (SmolStr::new("index"), Type::Unknown),
                    (SmolStr::new("value"), Type::Unknown),
                ],
                ret: Type::Nil,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("gpu_atomic_i32_new"),
            FunctionSig {
                params: vec![(SmolStr::new("initial"), Type::I32)],
                ret: Type::GpuAtomicI32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("gpu_atomic_i32_drop"),
            FunctionSig {
                params: vec![(SmolStr::new("atomic"), Type::GpuAtomicI32)],
                ret: Type::Boolean,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("gpu_atomic_i32_load"),
            FunctionSig {
                params: vec![(SmolStr::new("atomic"), Type::GpuAtomicI32)],
                ret: Type::I32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("gpu_atomic_i32_store"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("atomic"), Type::GpuAtomicI32),
                    (SmolStr::new("value"), Type::I32),
                ],
                ret: Type::Nil,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("gpu_atomic_i32_fetch_add"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("atomic"), Type::GpuAtomicI32),
                    (SmolStr::new("delta"), Type::I32),
                ],
                ret: Type::I32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("gpu_atomic_u32_new"),
            FunctionSig {
                params: vec![(SmolStr::new("initial"), Type::U32)],
                ret: Type::GpuAtomicU32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("gpu_atomic_u32_drop"),
            FunctionSig {
                params: vec![(SmolStr::new("atomic"), Type::GpuAtomicU32)],
                ret: Type::Boolean,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("gpu_atomic_u32_load"),
            FunctionSig {
                params: vec![(SmolStr::new("atomic"), Type::GpuAtomicU32)],
                ret: Type::U32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("gpu_atomic_u32_store"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("atomic"), Type::GpuAtomicU32),
                    (SmolStr::new("value"), Type::U32),
                ],
                ret: Type::Nil,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("gpu_atomic_u32_fetch_add"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("atomic"), Type::GpuAtomicU32),
                    (SmolStr::new("delta"), Type::U32),
                ],
                ret: Type::U32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("gpu_schedule_deterministic"),
            FunctionSig {
                params: vec![],
                ret: Type::GpuDispatchSchedule,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("gpu_schedule_reverse"),
            FunctionSig {
                params: vec![],
                ret: Type::GpuDispatchSchedule,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("gpu_schedule_shuffle"),
            FunctionSig {
                params: vec![(SmolStr::new("seed"), Type::U32)],
                ret: Type::GpuDispatchSchedule,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("gpu_schedule_workgroup_reverse"),
            FunctionSig {
                params: vec![],
                ret: Type::GpuDispatchSchedule,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("gpu_schedule_workgroup_shuffle"),
            FunctionSig {
                params: vec![(SmolStr::new("seed"), Type::U32)],
                ret: Type::GpuDispatchSchedule,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("gpu_schedule_round_robin_workgroups"),
            FunctionSig {
                params: vec![],
                ret: Type::GpuDispatchSchedule,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("workgroup_barrier"),
            FunctionSig {
                params: vec![],
                ret: Type::Nil,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("storage_barrier"),
            FunctionSig {
                params: vec![],
                ret: Type::Nil,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("global_invocation_id"),
            FunctionSig {
                params: vec![],
                ret: Type::Array(Box::new(Type::U32), 3),
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("local_invocation_id"),
            FunctionSig {
                params: vec![],
                ret: Type::Array(Box::new(Type::U32), 3),
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("workgroup_id"),
            FunctionSig {
                params: vec![],
                ret: Type::Array(Box::new(Type::U32), 3),
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("num_workgroups"),
            FunctionSig {
                params: vec![],
                ret: Type::Array(Box::new(Type::U32), 3),
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("workgroup_size"),
            FunctionSig {
                params: vec![],
                ret: Type::Array(Box::new(Type::U32), 3),
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("dispatch_compute"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("kernel"), Type::Unknown),
                    (SmolStr::new("workgroups_x"), Type::U32),
                    (SmolStr::new("workgroups_y"), Type::U32),
                    (SmolStr::new("workgroups_z"), Type::U32),
                    (SmolStr::new("workgroup_size_x"), Type::U32),
                    (SmolStr::new("workgroup_size_y"), Type::U32),
                    (SmolStr::new("workgroup_size_z"), Type::U32),
                ],
                ret: Type::Nil,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("dot"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("left"), Type::Unknown),
                    (SmolStr::new("right"), Type::Unknown),
                ],
                ret: Type::F32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("bounds2_center"),
            FunctionSig {
                params: vec![(SmolStr::new("bounds"), portable_named_type("Bounds2"))],
                ret: Type::Vec2,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("bounds2_size"),
            FunctionSig {
                params: vec![(SmolStr::new("bounds"), portable_named_type("Bounds2"))],
                ret: Type::Vec2,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("bounds3_center"),
            FunctionSig {
                params: vec![(SmolStr::new("bounds"), portable_named_type("Bounds3"))],
                ret: Type::Vec3,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("bounds3_size"),
            FunctionSig {
                params: vec![(SmolStr::new("bounds"), portable_named_type("Bounds3"))],
                ret: Type::Vec3,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("length"),
            FunctionSig {
                params: vec![(SmolStr::new("value"), Type::Unknown)],
                ret: Type::F32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("normalize"),
            FunctionSig {
                params: vec![(SmolStr::new("value"), Type::Unknown)],
                ret: Type::Unknown,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("cross"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("left"), Type::Unknown),
                    (SmolStr::new("right"), Type::Unknown),
                ],
                ret: Type::Vec3,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("min"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("left"), Type::Unknown),
                    (SmolStr::new("right"), Type::Unknown),
                ],
                ret: Type::Unknown,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("max"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("left"), Type::Unknown),
                    (SmolStr::new("right"), Type::Unknown),
                ],
                ret: Type::Unknown,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("clamp"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("value"), Type::Unknown),
                    (SmolStr::new("min"), Type::Unknown),
                    (SmolStr::new("max"), Type::Unknown),
                ],
                ret: Type::Unknown,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("mix"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("value"), Type::Unknown),
                    (SmolStr::new("other"), Type::Unknown),
                    (SmolStr::new("t"), Type::Unknown),
                ],
                ret: Type::Unknown,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("abs"),
            FunctionSig {
                params: vec![(SmolStr::new("value"), Type::Unknown)],
                ret: Type::Unknown,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("sign"),
            FunctionSig {
                params: vec![(SmolStr::new("value"), Type::Unknown)],
                ret: Type::Unknown,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("floor"),
            FunctionSig {
                params: vec![(SmolStr::new("value"), Type::Unknown)],
                ret: Type::Unknown,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("ceil"),
            FunctionSig {
                params: vec![(SmolStr::new("value"), Type::Unknown)],
                ret: Type::Unknown,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("fract"),
            FunctionSig {
                params: vec![(SmolStr::new("value"), Type::Unknown)],
                ret: Type::Unknown,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("sin"),
            FunctionSig {
                params: vec![(SmolStr::new("value"), Type::Unknown)],
                ret: Type::Unknown,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("cos"),
            FunctionSig {
                params: vec![(SmolStr::new("value"), Type::Unknown)],
                ret: Type::Unknown,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("sqrt"),
            FunctionSig {
                params: vec![(SmolStr::new("value"), Type::Unknown)],
                ret: Type::Unknown,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("pow"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("left"), Type::Unknown),
                    (SmolStr::new("right"), Type::Unknown),
                ],
                ret: Type::Unknown,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("distance"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("left"), Type::Unknown),
                    (SmolStr::new("right"), Type::Unknown),
                ],
                ret: Type::F32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("reflect"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("left"), Type::Unknown),
                    (SmolStr::new("right"), Type::Unknown),
                ],
                ret: Type::Unknown,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("transform_point"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("transform"), portable_named_type("Transform3")),
                    (SmolStr::new("point"), Type::Vec3),
                ],
                ret: Type::Vec3,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("transform_vector"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("transform"), portable_named_type("Transform3")),
                    (SmolStr::new("vector"), Type::Vec3),
                ],
                ret: Type::Vec3,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("transform_normal"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("transform"), portable_named_type("Transform3")),
                    (SmolStr::new("normal"), Type::Vec3),
                ],
                ret: Type::Vec3,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("compose_transform3"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("left"), portable_named_type("Transform3")),
                    (SmolStr::new("right"), portable_named_type("Transform3")),
                ],
                ret: portable_named_type("Transform3"),
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("inverse_transform3"),
            FunctionSig {
                params: vec![(SmolStr::new("transform"), portable_named_type("Transform3"))],
                ret: portable_named_type("Transform3"),
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("sphere"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("p"), Type::Vec3),
                    (SmolStr::new("radius"), Type::F32),
                ],
                ret: Type::F32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_primitive_sphere"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("p"), Type::Vec3),
                    (SmolStr::new("radius"), Type::F32),
                ],
                ret: Type::F32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("box"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("p"), Type::Vec3),
                    (SmolStr::new("half"), Type::Vec3),
                ],
                ret: Type::F32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_primitive_box"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("p"), Type::Vec3),
                    (SmolStr::new("half"), Type::Vec3),
                ],
                ret: Type::F32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("capsule"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("p"), Type::Vec3),
                    (SmolStr::new("a"), Type::Vec3),
                    (SmolStr::new("b"), Type::Vec3),
                    (SmolStr::new("radius"), Type::F32),
                ],
                ret: Type::F32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_primitive_capsule"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("p"), Type::Vec3),
                    (SmolStr::new("a"), Type::Vec3),
                    (SmolStr::new("b"), Type::Vec3),
                    (SmolStr::new("radius"), Type::F32),
                ],
                ret: Type::F32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("cylinder"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("p"), Type::Vec3),
                    (SmolStr::new("radius"), Type::F32),
                    (SmolStr::new("half_height"), Type::F32),
                ],
                ret: Type::F32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_primitive_cylinder"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("p"), Type::Vec3),
                    (SmolStr::new("radius"), Type::F32),
                    (SmolStr::new("half_height"), Type::F32),
                ],
                ret: Type::F32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("plane"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("p"), Type::Vec3),
                    (SmolStr::new("normal"), Type::Vec3),
                    (SmolStr::new("offset"), Type::F32),
                ],
                ret: Type::F32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_primitive_plane"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("p"), Type::Vec3),
                    (SmolStr::new("normal"), Type::Vec3),
                    (SmolStr::new("offset"), Type::F32),
                ],
                ret: Type::F32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("torus"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("p"), Type::Vec3),
                    (SmolStr::new("major_radius"), Type::F32),
                    (SmolStr::new("minor_radius"), Type::F32),
                ],
                ret: Type::F32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_primitive_torus"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("p"), Type::Vec3),
                    (SmolStr::new("major_radius"), Type::F32),
                    (SmolStr::new("minor_radius"), Type::F32),
                ],
                ret: Type::F32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("rounded_box"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("p"), Type::Vec3),
                    (SmolStr::new("half"), Type::Vec3),
                    (SmolStr::new("radius"), Type::F32),
                ],
                ret: Type::F32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_primitive_rounded_box"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("p"), Type::Vec3),
                    (SmolStr::new("half"), Type::Vec3),
                    (SmolStr::new("radius"), Type::F32),
                ],
                ret: Type::F32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("circle2"),
            FunctionSig {
                params: vec![(SmolStr::new("p"), Type::Vec2), (SmolStr::new("radius"), Type::F32)],
                ret: Type::F32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("rect2"),
            FunctionSig {
                params: vec![(SmolStr::new("p"), Type::Vec2), (SmolStr::new("half"), Type::Vec2)],
                ret: Type::F32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("rounded_rect2"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("p"), Type::Vec2),
                    (SmolStr::new("half"), Type::Vec2),
                    (SmolStr::new("radius"), Type::F32),
                ],
                ret: Type::F32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("capsule2"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("p"), Type::Vec2),
                    (SmolStr::new("a"), Type::Vec2),
                    (SmolStr::new("b"), Type::Vec2),
                    (SmolStr::new("radius"), Type::F32),
                ],
                ret: Type::F32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("segment2"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("p"), Type::Vec2),
                    (SmolStr::new("a"), Type::Vec2),
                    (SmolStr::new("b"), Type::Vec2),
                ],
                ret: Type::F32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("polygon2"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("p"), Type::Vec2),
                    (SmolStr::new("vertices"), Type::List(Box::new(Type::Vec2))),
                ],
                ret: Type::F32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("polyline2"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("p"), Type::Vec2),
                    (SmolStr::new("vertices"), Type::List(Box::new(Type::Vec2))),
                ],
                ret: Type::F32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("ellipsoid"),
            FunctionSig {
                params: vec![(SmolStr::new("p"), Type::Vec3), (SmolStr::new("radii"), Type::Vec3)],
                ret: Type::F32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("circle2"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("p"), Type::Vec2),
                    (SmolStr::new("radius"), Type::F32),
                ],
                ret: Type::F32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("rect2"),
            FunctionSig {
                params: vec![(SmolStr::new("p"), Type::Vec2), (SmolStr::new("half"), Type::Vec2)],
                ret: Type::F32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("rounded_rect2"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("p"), Type::Vec2),
                    (SmolStr::new("half"), Type::Vec2),
                    (SmolStr::new("radius"), Type::F32),
                ],
                ret: Type::F32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("capsule2"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("p"), Type::Vec2),
                    (SmolStr::new("a"), Type::Vec2),
                    (SmolStr::new("b"), Type::Vec2),
                    (SmolStr::new("radius"), Type::F32),
                ],
                ret: Type::F32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("segment2"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("p"), Type::Vec2),
                    (SmolStr::new("a"), Type::Vec2),
                    (SmolStr::new("b"), Type::Vec2),
                ],
                ret: Type::F32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("polygon2"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("p"), Type::Vec2),
                    (SmolStr::new("vertices"), Type::List(Box::new(Type::Vec2))),
                ],
                ret: Type::F32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("polyline2"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("p"), Type::Vec2),
                    (SmolStr::new("vertices"), Type::List(Box::new(Type::Vec2))),
                ],
                ret: Type::F32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_primitive_ellipsoid"),
            FunctionSig {
                params: vec![(SmolStr::new("p"), Type::Vec3), (SmolStr::new("radii"), Type::Vec3)],
                ret: Type::F32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("cone"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("p"), Type::Vec3),
                    (SmolStr::new("radius"), Type::F32),
                    (SmolStr::new("half_height"), Type::F32),
                ],
                ret: Type::F32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_primitive_cone"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("p"), Type::Vec3),
                    (SmolStr::new("radius"), Type::F32),
                    (SmolStr::new("half_height"), Type::F32),
                ],
                ret: Type::F32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("capped_cone"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("p"), Type::Vec3),
                    (SmolStr::new("radius_bottom"), Type::F32),
                    (SmolStr::new("radius_top"), Type::F32),
                    (SmolStr::new("half_height"), Type::F32),
                ],
                ret: Type::F32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_primitive_capped_cone"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("p"), Type::Vec3),
                    (SmolStr::new("radius_bottom"), Type::F32),
                    (SmolStr::new("radius_top"), Type::F32),
                    (SmolStr::new("half_height"), Type::F32),
                ],
                ret: Type::F32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("box_frame"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("p"), Type::Vec3),
                    (SmolStr::new("half"), Type::Vec3),
                    (SmolStr::new("thickness"), Type::F32),
                ],
                ret: Type::F32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_primitive_box_frame"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("p"), Type::Vec3),
                    (SmolStr::new("half"), Type::Vec3),
                    (SmolStr::new("thickness"), Type::F32),
                ],
                ret: Type::F32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("slab"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("p"), Type::Vec3),
                    (SmolStr::new("thickness"), Type::F32),
                ],
                ret: Type::F32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_primitive_slab"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("p"), Type::Vec3),
                    (SmolStr::new("thickness"), Type::F32),
                ],
                ret: Type::F32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("triangle_prism"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("p"), Type::Vec3),
                    (SmolStr::new("half"), Type::Vec2),
                    (SmolStr::new("half_height"), Type::F32),
                ],
                ret: Type::F32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_primitive_triangle_prism"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("p"), Type::Vec3),
                    (SmolStr::new("half"), Type::Vec2),
                    (SmolStr::new("half_height"), Type::F32),
                ],
                ret: Type::F32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("hex_prism"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("p"), Type::Vec3),
                    (SmolStr::new("half"), Type::Vec2),
                    (SmolStr::new("half_height"), Type::F32),
                ],
                ret: Type::F32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("__wr_primitive_hex_prism"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("p"), Type::Vec3),
                    (SmolStr::new("half"), Type::Vec2),
                    (SmolStr::new("half_height"), Type::F32),
                ],
                ret: Type::F32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("field_union"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("left"), Type::F32),
                    (SmolStr::new("right"), Type::F32),
                ],
                ret: Type::F32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("field_intersection"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("left"), Type::F32),
                    (SmolStr::new("right"), Type::F32),
                ],
                ret: Type::F32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("field_subtract"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("left"), Type::F32),
                    (SmolStr::new("right"), Type::F32),
                ],
                ret: Type::F32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("field_translate_point"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("translate"), Type::Vec3),
                    (SmolStr::new("point"), Type::Vec3),
                ],
                ret: Type::Vec3,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("field_rotate_point"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("rotate"), Type::Vec3),
                    (SmolStr::new("point"), Type::Vec3),
                ],
                ret: Type::Vec3,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("field_uniform_scale_point"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("scale"), Type::F32),
                    (SmolStr::new("point"), Type::Vec3),
                ],
                ret: Type::Vec3,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("field_affine_transform_point"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("transform"), portable_named_type("Transform3")),
                    (SmolStr::new("point"), Type::Vec3),
                ],
                ret: Type::Vec3,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("field_warp_point"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("warp"), Type::Vec3),
                    (SmolStr::new("point"), Type::Vec3),
                ],
                ret: Type::Vec3,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("field_repeat_linear_point"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("repeat"), Type::Vec3),
                    (SmolStr::new("point"), Type::Vec3),
                ],
                ret: Type::Vec3,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("field_repeat_grid_point"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("repeat"), Type::Vec3),
                    (SmolStr::new("point"), Type::Vec3),
                ],
                ret: Type::Vec3,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("field_radial_repeat_point"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("radial"), Type::Vec3),
                    (SmolStr::new("point"), Type::Vec3),
                ],
                ret: Type::Vec3,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("field_mirror_array_point"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("mirror"), Type::Vec3),
                    (SmolStr::new("point"), Type::Vec3),
                ],
                ret: Type::Vec3,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("field_instance_array_point"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("instance"), portable_named_type("Transform3")),
                    (SmolStr::new("point"), Type::Vec3),
                ],
                ret: Type::Vec3,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("field_sweep_coords"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("path"), Type::Vec3),
                    (SmolStr::new("point"), Type::Vec3),
                ],
                ret: Type::Vec3,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("field_smooth_union"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("smoothing"), Type::F32),
                    (SmolStr::new("left"), Type::F32),
                    (SmolStr::new("right"), Type::F32),
                ],
                ret: Type::F32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("field_smooth_intersection"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("smoothing"), Type::F32),
                    (SmolStr::new("left"), Type::F32),
                    (SmolStr::new("right"), Type::F32),
                ],
                ret: Type::F32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("field_smooth_subtract"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("smoothing"), Type::F32),
                    (SmolStr::new("left"), Type::F32),
                    (SmolStr::new("right"), Type::F32),
                ],
                ret: Type::F32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("field_bend_point"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("bend"), Type::Vec3),
                    (SmolStr::new("point"), Type::Vec3),
                ],
                ret: Type::Vec3,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("field_twist_point"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("twist"), Type::Vec3),
                    (SmolStr::new("point"), Type::Vec3),
                ],
                ret: Type::Vec3,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("field_taper_point"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("taper"), Type::Vec3),
                    (SmolStr::new("point"), Type::Vec3),
                ],
                ret: Type::Vec3,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("field_displace_point"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("displace"), Type::Vec3),
                    (SmolStr::new("point"), Type::Vec3),
                ],
                ret: Type::Vec3,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("distance_at"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("capture"), Type::Unknown),
                    (SmolStr::new("point"), Type::Vec3),
                ],
                ret: Type::F32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("normal_at"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("capture"), Type::Unknown),
                    (SmolStr::new("point"), Type::Vec3),
                ],
                ret: Type::Vec3,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("trace_shape"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("capture"), portable_named_type("ShapeCapture")),
                    (SmolStr::new("origin"), Type::Vec3),
                    (SmolStr::new("direction"), Type::Vec3),
                    (SmolStr::new("max_distance"), Type::F32),
                    (SmolStr::new("min_step"), Type::F32),
                    (SmolStr::new("hit_epsilon"), Type::F32),
                    (SmolStr::new("max_steps"), Type::Integer),
                ],
                ret: portable_named_type("Hit3"),
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("surface_at"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("capture"), portable_named_type("ShapeCapture")),
                    (SmolStr::new("hit"), portable_named_type("Hit3")),
                ],
                ret: portable_named_type("Surface"),
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("radiance_at"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("capture"), portable_named_type("ShapeCapture")),
                    (SmolStr::new("point"), Type::Vec3),
                    (SmolStr::new("direction"), Type::Vec3),
                ],
                ret: Type::Vec3,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("medium_at"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("capture"), portable_named_type("ShapeCapture")),
                    (SmolStr::new("point"), Type::Vec3),
                ],
                ret: portable_named_type("Medium"),
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("distance_world"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("capture"), portable_named_type("RegionCapture")),
                    (SmolStr::new("domain"), portable_named_type("SceneDomain")),
                    (SmolStr::new("point"), Type::Vec3),
                    (
                        SmolStr::new("backend"),
                        portable_named_type("DispatchBackend"),
                    ),
                ],
                ret: Type::F32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("normal_world"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("capture"), portable_named_type("RegionCapture")),
                    (SmolStr::new("domain"), portable_named_type("SceneDomain")),
                    (SmolStr::new("point"), Type::Vec3),
                    (
                        SmolStr::new("backend"),
                        portable_named_type("DispatchBackend"),
                    ),
                ],
                ret: Type::Vec3,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("trace_world"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("capture"), portable_named_type("RegionCapture")),
                    (SmolStr::new("domain"), portable_named_type("SceneDomain")),
                    (SmolStr::new("origin"), Type::Vec3),
                    (SmolStr::new("direction"), Type::Vec3),
                    (SmolStr::new("max_distance"), Type::F32),
                    (SmolStr::new("min_step"), Type::F32),
                    (SmolStr::new("hit_epsilon"), Type::F32),
                    (SmolStr::new("max_steps"), Type::Integer),
                    (
                        SmolStr::new("backend"),
                        portable_named_type("DispatchBackend"),
                    ),
                ],
                ret: portable_named_type("Hit3"),
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("surface_world"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("capture"), portable_named_type("RegionCapture")),
                    (SmolStr::new("domain"), portable_named_type("SceneDomain")),
                    (SmolStr::new("hit"), portable_named_type("Hit3")),
                    (
                        SmolStr::new("backend"),
                        portable_named_type("DispatchBackend"),
                    ),
                ],
                ret: portable_named_type("Surface"),
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("radiance_world"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("capture"), portable_named_type("RegionCapture")),
                    (SmolStr::new("domain"), portable_named_type("SceneDomain")),
                    (SmolStr::new("point"), Type::Vec3),
                    (SmolStr::new("direction"), Type::Vec3),
                    (
                        SmolStr::new("backend"),
                        portable_named_type("DispatchBackend"),
                    ),
                ],
                ret: Type::Vec3,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("medium_world"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("capture"), portable_named_type("RegionCapture")),
                    (SmolStr::new("domain"), portable_named_type("SceneDomain")),
                    (SmolStr::new("point"), Type::Vec3),
                    (
                        SmolStr::new("backend"),
                        portable_named_type("DispatchBackend"),
                    ),
                ],
                ret: portable_named_type("Medium"),
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("trace_shape_batch"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("capture"), portable_named_type("ShapeCapture")),
                    (
                        SmolStr::new("rays"),
                        Type::List(Box::new(portable_named_type("RayQuery"))),
                    ),
                    (SmolStr::new("backend"), portable_named_type("DispatchBackend")),
                ],
                ret: Type::List(Box::new(portable_named_type("Hit3"))),
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("surface_at_batch"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("capture"), portable_named_type("ShapeCapture")),
                    (
                        SmolStr::new("hits"),
                        Type::List(Box::new(portable_named_type("Hit3"))),
                    ),
                    (SmolStr::new("backend"), portable_named_type("DispatchBackend")),
                ],
                ret: Type::List(Box::new(portable_named_type("Surface"))),
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("distance_at_batch"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("capture"), Type::Unknown),
                    (
                        SmolStr::new("points"),
                        Type::List(Box::new(portable_named_type("PointQuery"))),
                    ),
                    (SmolStr::new("backend"), portable_named_type("DispatchBackend")),
                ],
                ret: Type::List(Box::new(portable_named_type("DistanceResult"))),
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("normal_at_batch"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("capture"), Type::Unknown),
                    (
                        SmolStr::new("points"),
                        Type::List(Box::new(portable_named_type("PointQuery"))),
                    ),
                    (SmolStr::new("backend"), portable_named_type("DispatchBackend")),
                ],
                ret: Type::List(Box::new(portable_named_type("NormalResult"))),
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("occluded_batch"),
            FunctionSig {
                params: vec![
                    (SmolStr::new("capture"), portable_named_type("ShapeCapture")),
                    (
                        SmolStr::new("rays"),
                        Type::List(Box::new(portable_named_type("RayQuery"))),
                    ),
                    (SmolStr::new("backend"), portable_named_type("DispatchBackend")),
                ],
                ret: Type::List(Box::new(portable_named_type("OcclusionResult"))),
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("dispatch_backend_cpu"),
            FunctionSig {
                params: Vec::new(),
                ret: portable_named_type("DispatchBackend"),
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("dispatch_backend_virtual_gpu"),
            FunctionSig {
                params: Vec::new(),
                ret: portable_named_type("DispatchBackend"),
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("dispatch_backend_wgsl"),
            FunctionSig {
                params: Vec::new(),
                ret: portable_named_type("DispatchBackend"),
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("dispatch_backend_auto"),
            FunctionSig {
                params: Vec::new(),
                ret: portable_named_type("DispatchBackend"),
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("f32"),
            FunctionSig {
                params: vec![(SmolStr::new("value"), Type::Unknown)],
                ret: Type::F32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("i32"),
            FunctionSig {
                params: vec![(SmolStr::new("value"), Type::Unknown)],
                ret: Type::I32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("i64"),
            FunctionSig {
                params: vec![(SmolStr::new("value"), Type::Unknown)],
                ret: Type::I64,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("u32"),
            FunctionSig {
                params: vec![(SmolStr::new("value"), Type::Unknown)],
                ret: Type::U32,
                kind: FunctionKind::Function,
                type_params: Vec::new(),
                type_param_bounds: Vec::new(),
            },
        ),
        (
            SmolStr::new("u64"),
            FunctionSig {
                params: vec![(SmolStr::new("value"), Type::Unknown)],
                ret: Type::U64,
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
    function_lane: FunctionLane,
    function_role: FunctionRole,
    function_name: SmolStr,
}

impl TypeContext {
    fn with_info(info: &mut FunctionTypeInfo) -> Self {
        Self {
            scopes: Vec::new(),
            type_params: Vec::new(),
            info: Some(info as *mut FunctionTypeInfo),
            function_lane: FunctionLane::Host,
            function_role: FunctionRole::Function,
            function_name: SmolStr::new(""),
        }
    }

    fn set_function_lane(&mut self, lane: FunctionLane) {
        self.function_lane = lane;
    }

    fn set_function_role(&mut self, role: FunctionRole) {
        self.function_role = role;
    }

    fn set_function_name(&mut self, name: SmolStr) {
        self.function_name = name;
    }

    fn in_portable_lane(&self) -> bool {
        matches!(self.function_lane, FunctionLane::Portable)
    }

    fn in_portable_query_kernel_lane(&self) -> bool {
        self.in_portable_lane() && matches!(self.function_role, FunctionRole::Kernel)
    }

    fn current_function_name(&self) -> SmolStr {
        if self.function_name.is_empty() {
            SmolStr::new("<portable>")
        } else {
            self.function_name.clone()
        }
    }

    fn current_function_role(&self) -> FunctionRole {
        self.function_role
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

    fn record_expr(&mut self, body: &Body, expr_id: Idx<Expr>, ty: Type) {
        if let Some(info) = self.info {
            unsafe {
                (*info)
                    .expr_types
                    .insert((crate::hir::body_key(body), expr_id.into_raw()), ty);
            }
        }
    }
}
