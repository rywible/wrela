//! Owns per-function MIR entry lowering, including parameter, capture, and
//! helper setup before body lowering runs.
//! Does not own whole-module symbol collection or expression/statement lowering.
//!
//! Key invariants:
//! - lowered entry state must match the canonical function name, ABI, and query
//!   backend chosen for the function.
//! - body lowering assumes entry blocks and local bindings from this module are
//!   complete before it starts.
//!
//! Primary entrypoints:
//! - `lower_function`
//!
//! Failure modes / common pitfalls:
//! - mismatching entry locals/ABI layout here causes later MIR to look valid
//!   while binding arguments to the wrong semantic slot.

use super::render_helpers::build_scene_domain_contract_value;
use super::*;

pub(super) fn lower_function(
    module: &hir::Module,
    func_idx: hir::Idx<hir::Function>,
    func: &hir::Function,
    name: SmolStr,
    body: &hir::Body,
    module_type_info: &TypeInfo,
    default_query_backend: DispatchBackend,
    type_tags: &HashMap<SmolStr, TypeTagId>,
    class_fields: &HashMap<SmolStr, Vec<SmolStr>>,
    class_field_defaults: &HashMap<SmolStr, Vec<Option<hir::FieldDefault>>>,
    function_names: &HashSet<SmolStr>,
    field_names: &HashSet<SmolStr>,
    shape_names: &HashSet<SmolStr>,
    shape_graphs: &HashMap<SmolStr, hir::ShapeGraph>,
    field_graphs: &HashMap<SmolStr, hir::FieldGraph>,
    field_bodies: &HashMap<SmolStr, hir::Body>,
    field_metadata: &HashMap<SmolStr, hir::FieldMetadata>,
    radiance_param_counts: &HashMap<SmolStr, usize>,
    volume_param_counts: &HashMap<SmolStr, usize>,
    result_functions: &HashSet<SmolStr>,
    class_method_ids: &HashMap<SmolStr, HashMap<SmolStr, u32>>,
    interface_methods: &HashMap<SmolStr, HashSet<SmolStr>>,
    is_method: bool,
    type_info: &FunctionTypeInfo,
) -> MirFunction {
    if matches!(func.role, FunctionRole::Kernel) {
        return lower_kernel_ir_function(
            module,
            func_idx,
            func,
            name,
            module_type_info,
            default_query_backend,
            type_tags,
            class_fields,
            class_field_defaults,
            function_names,
            field_names,
            shape_names,
            shape_graphs,
            field_graphs,
            field_bodies,
            field_metadata,
            radiance_param_counts,
            volume_param_counts,
            result_functions,
            class_method_ids,
            interface_methods,
            is_method,
            type_info,
        );
    }
    if matches!(func.role, FunctionRole::Region) {
        return lower_region_function(
            module,
            func,
            name,
            default_query_backend,
            type_tags,
            class_fields,
            class_field_defaults,
            function_names,
            field_names,
            shape_names,
            shape_graphs,
            field_graphs,
            field_bodies,
            field_metadata,
            radiance_param_counts,
            volume_param_counts,
            result_functions,
            class_method_ids,
            interface_methods,
            type_info,
        );
    }
    if matches!(func.role, FunctionRole::Domain) {
        return lower_domain_function(
            module,
            func,
            name,
            default_query_backend,
            type_tags,
            class_fields,
            class_field_defaults,
            function_names,
            field_names,
            shape_names,
            shape_graphs,
            field_graphs,
            field_bodies,
            field_metadata,
            radiance_param_counts,
            volume_param_counts,
            result_functions,
            class_method_ids,
            interface_methods,
            type_info,
        );
    }
    if matches!(func.role, FunctionRole::Field) {
        if let Some(graph) = func.field_graph.as_ref() {
            if !matches!(&graph.root, hir::FieldExpr::Custom { .. }) {
                return lower_semantic_field_function(
                    module,
                    func,
                    name,
                    &graph.root,
                    body,
                    default_query_backend,
                    type_tags,
                    class_fields,
                    class_field_defaults,
                    function_names,
                    field_names,
                    shape_names,
                    shape_graphs,
                    field_graphs,
                    field_bodies,
                    field_metadata,
                    radiance_param_counts,
                    volume_param_counts,
                    result_functions,
                    class_method_ids,
                    interface_methods,
                    is_method,
                    type_info,
                );
            }
        }
    }
    let mut lowerer = FunctionLowerer::new(
        name,
        type_tags,
        class_fields,
        class_field_defaults,
        function_names,
        field_names,
        shape_names,
        shape_graphs,
        field_graphs,
        field_bodies,
        field_metadata,
        radiance_param_counts,
        volume_param_counts,
        result_functions,
        class_method_ids,
        interface_methods,
        matches!(
            func.ret_type.as_ref().map(|t| t.name.as_str()),
            Some("Result")
        ),
        Some(type_info),
    );
    lowerer.default_query_backend = default_query_backend;

    if is_method {
        let local = lowerer.new_local(
            SmolStr::new("self"),
            false,
            lowerer.local_type_for_name(&SmolStr::new("self")),
        );
        lowerer.declare_local(SmolStr::new("self"), local);
        lowerer.params.push(local);
    }
    for param in &func.params {
        let local = lowerer.new_local(
            param.name.clone(),
            false,
            lowerer.local_type_for_name(&param.name),
        );
        lowerer.declare_local(param.name.clone(), local);
        let is_result = matches!(param.ty.as_ref().map(|t| t.name.as_str()), Some("Result"));
        lowerer.declare_resultness(param.name.clone(), is_result);
        lowerer.params.push(local);
    }

    let entry = lowerer.new_block();
    lowerer.current_block = entry;
    lowerer.lower_stmt_block(body, &body.root_stmts);
    if lowerer.block_is_open(lowerer.current_block) {
        lowerer.set_terminator(Terminator::Return {
            value: None,
            span: TextRange::empty(0.into()),
        });
    }

    MirFunction {
        name: lowerer.name,
        params: lowerer.params,
        abi_params: {
            let mut abi_params = Vec::with_capacity(func.params.len() + usize::from(is_method));
            if is_method {
                abi_params.push(PortableAbiType::Value);
            }
            abi_params.extend(func.params.iter().map(|param| {
                portable_abi_from_type_ref(
                    param.ty.as_ref(),
                    module,
                    type_tags,
                    &mut HashSet::new(),
                )
            }));
            abi_params
        },
        abi_return: portable_abi_from_type_ref(
            func.ret_type.as_ref(),
            module,
            type_tags,
            &mut HashSet::new(),
        ),
        locals: lowerer.locals,
        temps: lowerer.temps,
        blocks: lowerer.blocks,
        entry,
        suspendable: lowerer.suspendable,
    }
}

fn lower_kernel_ir_function(
    module: &hir::Module,
    func_idx: hir::Idx<hir::Function>,
    func: &hir::Function,
    name: SmolStr,
    module_type_info: &TypeInfo,
    default_query_backend: DispatchBackend,
    type_tags: &HashMap<SmolStr, TypeTagId>,
    class_fields: &HashMap<SmolStr, Vec<SmolStr>>,
    class_field_defaults: &HashMap<SmolStr, Vec<Option<hir::FieldDefault>>>,
    function_names: &HashSet<SmolStr>,
    field_names: &HashSet<SmolStr>,
    shape_names: &HashSet<SmolStr>,
    shape_graphs: &HashMap<SmolStr, hir::ShapeGraph>,
    field_graphs: &HashMap<SmolStr, hir::FieldGraph>,
    field_bodies: &HashMap<SmolStr, hir::Body>,
    field_metadata: &HashMap<SmolStr, hir::FieldMetadata>,
    radiance_param_counts: &HashMap<SmolStr, usize>,
    volume_param_counts: &HashMap<SmolStr, usize>,
    result_functions: &HashSet<SmolStr>,
    class_method_ids: &HashMap<SmolStr, HashMap<SmolStr, u32>>,
    interface_methods: &HashMap<SmolStr, HashSet<SmolStr>>,
    is_method: bool,
    type_info: &FunctionTypeInfo,
) -> MirFunction {
    debug_assert!(!is_method, "kernel methods are not supported");
    let kernel_module = lower_kernel_function(module, module_type_info, func_idx)
        .unwrap_or_else(|errors| panic!("kernel lowering failed for '{}': {errors:?}", func.name));
    debug_assert!(
        validate_kernel_module(&kernel_module).is_ok(),
        "compiler-generated kernel modules must stay kernel-valid"
    );
    let kernel = kernel_module
        .function(func.name.as_str())
        .cloned()
        .unwrap_or_else(|| panic!("missing lowered kernel function '{}'", func.name));

    let mut lowerer = FunctionLowerer::new(
        name,
        type_tags,
        class_fields,
        class_field_defaults,
        function_names,
        field_names,
        shape_names,
        shape_graphs,
        field_graphs,
        field_bodies,
        field_metadata,
        radiance_param_counts,
        volume_param_counts,
        result_functions,
        class_method_ids,
        interface_methods,
        matches!(
            func.ret_type.as_ref().map(|t| t.name.as_str()),
            Some("Result")
        ),
        Some(type_info),
    );
    lowerer.default_query_backend = default_query_backend;
    for param in &kernel.params {
        let local = lowerer.new_local(param.name.clone(), false, mir_type_from_type(&param.ty));
        lowerer.declare_local(param.name.clone(), local);
        lowerer.declare_resultness(param.name.clone(), false);
        lowerer.params.push(local);
    }
    let entry = lowerer.new_block();
    lowerer.current_block = entry;
    lowerer.lower_kernel_stmt_block(&kernel.body);
    if lowerer.block_is_open(lowerer.current_block) {
        lowerer.set_terminator(Terminator::Return {
            value: None,
            span: TextRange::empty(0.into()),
        });
    }

    MirFunction {
        name: lowerer.name,
        params: lowerer.params,
        abi_params: func
            .params
            .iter()
            .map(|param| {
                portable_abi_from_type_ref(
                    param.ty.as_ref(),
                    module,
                    type_tags,
                    &mut HashSet::new(),
                )
            })
            .collect(),
        abi_return: portable_abi_from_type_ref(
            func.ret_type.as_ref(),
            module,
            type_tags,
            &mut HashSet::new(),
        ),
        locals: lowerer.locals,
        temps: lowerer.temps,
        blocks: lowerer.blocks,
        entry,
        suspendable: lowerer.suspendable,
    }
}

fn lower_semantic_field_function(
    module: &hir::Module,
    func: &hir::Function,
    name: SmolStr,
    graph: &hir::FieldExpr,
    body: &hir::Body,
    default_query_backend: DispatchBackend,
    type_tags: &HashMap<SmolStr, TypeTagId>,
    class_fields: &HashMap<SmolStr, Vec<SmolStr>>,
    class_field_defaults: &HashMap<SmolStr, Vec<Option<hir::FieldDefault>>>,
    function_names: &HashSet<SmolStr>,
    field_names: &HashSet<SmolStr>,
    shape_names: &HashSet<SmolStr>,
    shape_graphs: &HashMap<SmolStr, hir::ShapeGraph>,
    field_graphs: &HashMap<SmolStr, hir::FieldGraph>,
    field_bodies: &HashMap<SmolStr, hir::Body>,
    field_metadata: &HashMap<SmolStr, hir::FieldMetadata>,
    radiance_param_counts: &HashMap<SmolStr, usize>,
    volume_param_counts: &HashMap<SmolStr, usize>,
    result_functions: &HashSet<SmolStr>,
    class_method_ids: &HashMap<SmolStr, HashMap<SmolStr, u32>>,
    interface_methods: &HashMap<SmolStr, HashSet<SmolStr>>,
    is_method: bool,
    type_info: &FunctionTypeInfo,
) -> MirFunction {
    let mut lowerer = FunctionLowerer::new(
        name,
        type_tags,
        class_fields,
        class_field_defaults,
        function_names,
        field_names,
        shape_names,
        shape_graphs,
        field_graphs,
        field_bodies,
        field_metadata,
        radiance_param_counts,
        volume_param_counts,
        result_functions,
        class_method_ids,
        interface_methods,
        matches!(
            func.ret_type.as_ref().map(|t| t.name.as_str()),
            Some("Result")
        ),
        Some(type_info),
    );
    lowerer.default_query_backend = default_query_backend;

    if is_method {
        let local = lowerer.new_local(
            SmolStr::new("self"),
            false,
            lowerer.local_type_for_name(&SmolStr::new("self")),
        );
        lowerer.declare_local(SmolStr::new("self"), local);
        lowerer.params.push(local);
    }
    for param in &func.params {
        let local = lowerer.new_local(
            param.name.clone(),
            false,
            lowerer.local_type_for_name(&param.name),
        );
        lowerer.declare_local(param.name.clone(), local);
        let is_result = matches!(param.ty.as_ref().map(|t| t.name.as_str()), Some("Result"));
        lowerer.declare_resultness(param.name.clone(), is_result);
        lowerer.params.push(local);
    }

    let entry = lowerer.new_block();
    lowerer.current_block = entry;
    let point = func
        .params
        .first()
        .and_then(|param| lowerer.resolve_local(&param.name))
        .map(Value::Local)
        .unwrap_or_else(|| {
            lowerer.lower_call_temp(
                MirType::Vec3,
                SmolStr::new("vec3"),
                vec![
                    Value::Const(Literal::Float(0.0)),
                    Value::Const(Literal::Float(0.0)),
                    Value::Const(Literal::Float(0.0)),
                ],
                TextRange::empty(0.into()),
            )
        });
    let result = lowerer.lower_field_distance_expr(graph, body, point, TextRange::empty(0.into()));
    lowerer.set_terminator(Terminator::Return {
        value: Some(result),
        span: TextRange::empty(0.into()),
    });

    MirFunction {
        name: lowerer.name,
        params: lowerer.params,
        abi_params: {
            let mut abi_params = Vec::with_capacity(func.params.len() + usize::from(is_method));
            if is_method {
                abi_params.push(PortableAbiType::Value);
            }
            abi_params.extend(func.params.iter().map(|param| {
                portable_abi_from_type_ref(
                    param.ty.as_ref(),
                    module,
                    type_tags,
                    &mut HashSet::new(),
                )
            }));
            abi_params
        },
        abi_return: portable_abi_from_type_ref(
            func.ret_type.as_ref(),
            module,
            type_tags,
            &mut HashSet::new(),
        ),
        locals: lowerer.locals,
        temps: lowerer.temps,
        blocks: lowerer.blocks,
        entry,
        suspendable: lowerer.suspendable,
    }
}

fn lower_region_function(
    module: &hir::Module,
    func: &hir::Function,
    name: SmolStr,
    default_query_backend: DispatchBackend,
    type_tags: &HashMap<SmolStr, TypeTagId>,
    class_fields: &HashMap<SmolStr, Vec<SmolStr>>,
    class_field_defaults: &HashMap<SmolStr, Vec<Option<hir::FieldDefault>>>,
    function_names: &HashSet<SmolStr>,
    field_names: &HashSet<SmolStr>,
    shape_names: &HashSet<SmolStr>,
    shape_graphs: &HashMap<SmolStr, hir::ShapeGraph>,
    field_graphs: &HashMap<SmolStr, hir::FieldGraph>,
    field_bodies: &HashMap<SmolStr, hir::Body>,
    field_metadata: &HashMap<SmolStr, hir::FieldMetadata>,
    radiance_param_counts: &HashMap<SmolStr, usize>,
    volume_param_counts: &HashMap<SmolStr, usize>,
    result_functions: &HashSet<SmolStr>,
    class_method_ids: &HashMap<SmolStr, HashMap<SmolStr, u32>>,
    interface_methods: &HashMap<SmolStr, HashSet<SmolStr>>,
    type_info: &FunctionTypeInfo,
) -> MirFunction {
    let span = TextRange::empty(0.into());
    let mut lowerer = FunctionLowerer::new(
        name,
        type_tags,
        class_fields,
        class_field_defaults,
        function_names,
        field_names,
        shape_names,
        shape_graphs,
        field_graphs,
        field_bodies,
        field_metadata,
        radiance_param_counts,
        volume_param_counts,
        result_functions,
        class_method_ids,
        interface_methods,
        false,
        Some(type_info),
    );
    lowerer.default_query_backend = default_query_backend;

    for param in &func.params {
        let local = lowerer.new_local(
            param.name.clone(),
            false,
            lowerer.local_type_for_name(&param.name),
        );
        lowerer.declare_local(param.name.clone(), local);
        lowerer.params.push(local);
    }

    let entry = lowerer.new_block();
    lowerer.current_block = entry;
    let result = if func.params.is_empty() {
        lowerer.build_scene_capture_value(&func.name, span)
    } else {
        let crash_temp = lowerer.new_temp(MirType::Unknown);
        lowerer.push_stmt(MirStmt::Assign {
            place: Place::Temp(crash_temp),
            value: Rvalue::Crash {
                value: Value::Const(Literal::String(SmolStr::new(
                    "parameterized regions are not executable yet; capture a zero-argument region",
                ))),
            },
            span,
        });
        Value::Temp(crash_temp)
    };
    lowerer.set_terminator(Terminator::Return {
        value: Some(result),
        span,
    });

    MirFunction {
        name: lowerer.name,
        params: lowerer.params,
        abi_params: func
            .params
            .iter()
            .map(|param| {
                portable_abi_from_type_ref(
                    param.ty.as_ref(),
                    module,
                    type_tags,
                    &mut HashSet::new(),
                )
            })
            .collect(),
        abi_return: portable_abi_from_type_ref(
            func.ret_type.as_ref(),
            module,
            type_tags,
            &mut HashSet::new(),
        ),
        locals: lowerer.locals,
        temps: lowerer.temps,
        blocks: lowerer.blocks,
        entry,
        suspendable: false,
    }
}

fn lower_domain_function(
    module: &hir::Module,
    func: &hir::Function,
    name: SmolStr,
    default_query_backend: DispatchBackend,
    type_tags: &HashMap<SmolStr, TypeTagId>,
    class_fields: &HashMap<SmolStr, Vec<SmolStr>>,
    class_field_defaults: &HashMap<SmolStr, Vec<Option<hir::FieldDefault>>>,
    function_names: &HashSet<SmolStr>,
    field_names: &HashSet<SmolStr>,
    shape_names: &HashSet<SmolStr>,
    shape_graphs: &HashMap<SmolStr, hir::ShapeGraph>,
    field_graphs: &HashMap<SmolStr, hir::FieldGraph>,
    field_bodies: &HashMap<SmolStr, hir::Body>,
    field_metadata: &HashMap<SmolStr, hir::FieldMetadata>,
    radiance_param_counts: &HashMap<SmolStr, usize>,
    volume_param_counts: &HashMap<SmolStr, usize>,
    result_functions: &HashSet<SmolStr>,
    class_method_ids: &HashMap<SmolStr, HashMap<SmolStr, u32>>,
    interface_methods: &HashMap<SmolStr, HashSet<SmolStr>>,
    type_info: &FunctionTypeInfo,
) -> MirFunction {
    let span = TextRange::empty(0.into());
    let mut lowerer = FunctionLowerer::new(
        name,
        type_tags,
        class_fields,
        class_field_defaults,
        function_names,
        field_names,
        shape_names,
        shape_graphs,
        field_graphs,
        field_bodies,
        field_metadata,
        radiance_param_counts,
        volume_param_counts,
        result_functions,
        class_method_ids,
        interface_methods,
        false,
        Some(type_info),
    );
    lowerer.default_query_backend = default_query_backend;

    for param in &func.params {
        let local = lowerer.new_local(
            param.name.clone(),
            false,
            lowerer.local_type_for_name(&param.name),
        );
        lowerer.declare_local(param.name.clone(), local);
        lowerer.params.push(local);
    }

    let entry = lowerer.new_block();
    lowerer.current_block = entry;
    let metadata = func.domain.as_ref().expect("domain metadata");
    let world_local = func
        .params
        .first()
        .and_then(|param| lowerer.resolve_local(&param.name));
    let world_scene_id = world_local.map(|world| {
        lowerer.lower_get_named_field(
            Value::Local(world),
            "RegionCapture",
            "scene_id",
            MirType::Integer,
            span,
        )
    });

    let result = build_scene_domain_contract_value(
        &mut lowerer,
        world_scene_id.unwrap_or(Value::Const(Literal::Integer(0))),
        Value::Const(Literal::Integer(match metadata.geometry_detail {
            hir::DomainGeometryDetail::Coarse => 0,
            hir::DomainGeometryDetail::Fine => 1,
        })),
        Value::Const(Literal::Boolean(metadata.material)),
        Value::Const(Literal::Boolean(metadata.radiance)),
        Value::Const(Literal::Boolean(metadata.media)),
        span,
    );
    lowerer.set_terminator(Terminator::Return {
        value: Some(result),
        span,
    });

    MirFunction {
        name: lowerer.name,
        params: lowerer.params,
        abi_params: func
            .params
            .iter()
            .map(|param| {
                portable_abi_from_type_ref(
                    param.ty.as_ref(),
                    module,
                    type_tags,
                    &mut HashSet::new(),
                )
            })
            .collect(),
        abi_return: portable_abi_from_type_ref(
            func.ret_type.as_ref(),
            module,
            type_tags,
            &mut HashSet::new(),
        ),
        locals: lowerer.locals,
        temps: lowerer.temps,
        blocks: lowerer.blocks,
        entry,
        suspendable: false,
    }
}
