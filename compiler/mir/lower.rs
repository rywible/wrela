use crate::hir::{
    self, AssignOp, BinaryOp, Expr, FunctionRole, FunctionTypeInfo, Literal, Module,
    Stmt as HirStmt, Type, TypeInfo, UnaryOp,
};
use crate::kernel::{
    KernelExpr, KernelStmt, ParsedKernelDispatch, lower_batch_query_plan, lower_kernel_function,
    lower_world_query_plan, parse_dispatch_compute as parse_kernel_dispatch_compute,
    validate_module as validate_kernel_module,
};
use crate::mir::ir::Stmt as MirStmt;
use crate::mir::ir::*;
use crate::portable::{
    PortableBuiltinType, all_builtin_records, any_builtin_record, builtin_record_by_function,
    portable_builtin_type_abi,
};
use crate::query_exec::mir::{
    lower_field_batch_queries_helper, lower_scene_distance_capture_helper,
    lower_scene_medium_capture_helper, lower_scene_normal_capture_helper,
    lower_scene_radiance_capture_helper, lower_scene_surface_capture_helper,
    lower_scene_surface_queries_helper, lower_scene_trace_capture_helper,
    lower_scene_trace_queries_helper, lower_shape_batch_queries_helper,
    lower_shape_distance_helper, lower_shape_surface_helper, lower_shape_trace_helper,
    lower_world_distance_capture_helper, lower_world_medium_capture_helper,
    lower_world_normal_capture_helper, lower_world_radiance_capture_helper,
    lower_world_surface_capture_helper, lower_world_trace_capture_helper,
};
use crate::query_exec::{QueryExecContext, wgsl};
use crate::query_plan::{
    BatchQueryPlan, CaptureKind, DispatchBackend, FieldBatchPlanKind, ShapeBatchPlanKind,
    WorldQueryKind, WorldQueryPlan,
};
use crate::scene_ir;
use rowan::TextRange;
use smol_str::SmolStr;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::env;

fn is_syntactic_stringish(body: &hir::Body, expr: hir::Idx<hir::Expr>) -> bool {
    match &body.exprs[expr] {
        Expr::Literal(Literal::String(_)) => true,
        Expr::StringInterp(_) => true,
        _ => false,
    }
}

pub fn lower_module(module: &Module) -> MirModule {
    let (_type_errors, type_info) = crate::hir::typeck::check_module_with_info(module);
    lower_module_with_types(module, &type_info)
}

pub fn lower_module_with_types(module: &Module, type_info: &TypeInfo) -> MirModule {
    lower_module_with_types_and_backend(module, type_info, DispatchBackend::Auto)
}

pub fn lower_module_with_types_and_backend(
    module: &Module,
    type_info: &TypeInfo,
    default_query_backend: DispatchBackend,
) -> MirModule {
    const CLASS_ID_BASE: usize = 100;
    let mut type_tags = Vec::new();
    let mut tag_map = HashMap::new();
    let mut class_fields = HashMap::new();
    let mut class_field_defaults = HashMap::new();
    let mut classes = Vec::new();
    let mut class_method_ids = HashMap::new();
    let mut interface_methods: HashMap<SmolStr, HashSet<SmolStr>> = HashMap::new();
    let mut interface_impls: HashMap<SmolStr, Vec<SmolStr>> = HashMap::new();
    let mut method_ids = HashSet::new();
    let mut method_qnames: HashMap<hir::Idx<hir::Function>, SmolStr> = HashMap::new();
    for record in all_builtin_records() {
        let id = TypeTagId(type_tags.len() + CLASS_ID_BASE);
        let name = SmolStr::new(record.name);
        let fields: Vec<SmolStr> = record
            .fields
            .iter()
            .map(|field| SmolStr::new(field.name))
            .collect();
        type_tags.push(name.clone());
        tag_map.insert(name.clone(), id);
        class_fields.insert(name.clone(), fields.clone());
        class_field_defaults.insert(name.clone(), vec![None; fields.len()]);
        classes.push(MirClassInfo {
            name,
            id,
            fields,
            methods: Vec::new(),
        });
    }
    for (_idx, class) in module.classes.iter() {
        let id = TypeTagId(type_tags.len() + CLASS_ID_BASE);
        type_tags.push(class.name.clone());
        tag_map.insert(class.name.clone(), id);
        let fields: Vec<SmolStr> = class
            .fields
            .iter()
            .map(|field| field.name.clone())
            .collect();
        let defaults: Vec<Option<hir::FieldDefault>> = class
            .fields
            .iter()
            .map(|field| field.default.clone())
            .collect();
        class_fields.insert(class.name.clone(), fields);
        class_field_defaults.insert(class.name.clone(), defaults);
        let mut methods = Vec::new();
        let mut method_map = HashMap::new();
        for (idx, method_id) in class.methods.iter().enumerate() {
            let method = &module.functions[*method_id];
            method_ids.insert(method_id.into_raw());
            method_map.insert(method.name.clone(), idx as u32);
            let qname = SmolStr::new(format!("{}.{}", class.name, method.name));
            method_qnames.insert(*method_id, qname.clone());
            methods.push(MirMethodInfo {
                name: method.name.clone(),
                func: qname,
                arity: method.params.len() + 1,
                id: idx as u32,
            });
        }
        class_method_ids.insert(class.name.clone(), method_map);
        classes.push(MirClassInfo {
            name: class.name.clone(),
            id,
            fields: class_fields.get(&class.name).cloned().unwrap_or_default(),
            methods,
        });
    }

    for (_idx, interface) in module.interfaces.iter() {
        let method_set = interface_methods.entry(interface.name.clone()).or_default();
        for method in &interface.methods {
            method_set.insert(method.name.clone());
        }
    }
    for (_idx, class) in module.classes.iter() {
        for iface in &class.implements {
            interface_impls
                .entry(iface.clone())
                .or_default()
                .push(class.name.clone());
        }
    }

    for (_idx, en) in module.enums.iter() {
        for variant in &en.variants {
            let name = SmolStr::new(format!("{}.{}", en.name, variant.name));
            let id = TypeTagId(type_tags.len() + CLASS_ID_BASE);
            type_tags.push(name.clone());
            tag_map.insert(name.clone(), id);
            let fields: Vec<SmolStr> = variant
                .params
                .iter()
                .map(|param| param.name.clone())
                .collect();
            class_fields.insert(name.clone(), fields.clone());
            class_field_defaults.insert(name.clone(), vec![None; fields.len()]);
            classes.push(MirClassInfo {
                name: name.clone(),
                id,
                fields,
                methods: Vec::new(),
            });
        }
    }

    let mut functions = Vec::new();
    let mut function_names: HashSet<SmolStr> = module
        .functions
        .iter()
        .filter_map(|(idx, func)| {
            if method_ids.contains(&idx.into_raw()) {
                None
            } else {
                Some(func.name.clone())
            }
        })
        .collect();
    for qname in method_qnames.values() {
        function_names.insert(qname.clone());
    }
    for name in builtin_function_names() {
        function_names.insert(name);
    }
    let field_names: HashSet<SmolStr> = module
        .functions
        .iter()
        .filter_map(|(idx, func)| {
            if method_ids.contains(&idx.into_raw()) || !matches!(func.role, FunctionRole::Field) {
                None
            } else {
                Some(func.name.clone())
            }
        })
        .collect();
    let shape_names: HashSet<SmolStr> = module
        .shapes
        .iter()
        .map(|(_, shape)| shape.name.clone())
        .collect();
    let result_functions: HashSet<SmolStr> = module
        .functions
        .iter()
        .filter_map(|(_, func)| {
            let ret = func.ret_type.as_ref()?;
            if ret.name == "Result" {
                Some(func.name.clone())
            } else {
                None
            }
        })
        .collect();
    let shape_graphs: HashMap<SmolStr, hir::ShapeGraph> = module
        .shapes
        .iter()
        .filter_map(|(_, shape)| {
            shape
                .graph
                .as_ref()
                .map(|graph| (shape.name.clone(), graph.clone()))
        })
        .collect();
    let field_graphs: HashMap<SmolStr, hir::FieldGraph> = module
        .functions
        .iter()
        .filter_map(|(idx, func)| {
            if method_ids.contains(&idx.into_raw()) || !matches!(func.role, FunctionRole::Field) {
                None
            } else {
                func.field_graph
                    .as_ref()
                    .map(|graph| (func.name.clone(), graph.clone()))
            }
        })
        .collect();
    let field_bodies: HashMap<SmolStr, hir::Body> = module
        .functions
        .iter()
        .filter_map(|(idx, func)| {
            if method_ids.contains(&idx.into_raw()) || !matches!(func.role, FunctionRole::Field) {
                None
            } else {
                func.body
                    .as_ref()
                    .map(|body| (func.name.clone(), body.clone()))
            }
        })
        .collect();
    let field_metadata: HashMap<SmolStr, hir::FieldMetadata> = module
        .functions
        .iter()
        .filter_map(|(idx, func)| {
            if method_ids.contains(&idx.into_raw()) || !matches!(func.role, FunctionRole::Field) {
                None
            } else {
                func.field
                    .as_ref()
                    .map(|metadata| (func.name.clone(), metadata.clone()))
            }
        })
        .collect();
    let radiance_param_counts: HashMap<SmolStr, usize> = module
        .functions
        .iter()
        .filter_map(|(idx, func)| {
            if method_ids.contains(&idx.into_raw()) || !matches!(func.role, FunctionRole::Radiance)
            {
                None
            } else {
                Some((func.name.clone(), func.params.len()))
            }
        })
        .collect();
    let volume_param_counts: HashMap<SmolStr, usize> = module
        .functions
        .iter()
        .filter_map(|(idx, func)| {
            if method_ids.contains(&idx.into_raw()) || !matches!(func.role, FunctionRole::Volume) {
                None
            } else {
                Some((func.name.clone(), func.params.len()))
            }
        })
        .collect();
    let wgsl_query_ctx = Some(QueryExecContext::compile(module, type_info));
    let wgsl_field_indices = wgsl_query_ctx
        .as_ref()
        .map(|ctx| {
            ctx.scene
                .fields
                .keys()
                .enumerate()
                .map(|(index, name)| (name.clone(), index as u32))
                .collect::<HashMap<_, _>>()
        })
        .unwrap_or_default();
    let wgsl_shape_indices = wgsl_query_ctx
        .as_ref()
        .map(|ctx| {
            ctx.scene
                .shapes
                .keys()
                .enumerate()
                .map(|(index, name)| (name.clone(), index as u32))
                .collect::<HashMap<_, _>>()
        })
        .unwrap_or_default();
    let world_wgsl_configs = wgsl_query_ctx
        .as_ref()
        .map(|ctx| {
            [
                WorldQueryKind::Distance,
                WorldQueryKind::Normal,
                WorldQueryKind::Trace,
                WorldQueryKind::Surface,
                WorldQueryKind::Radiance,
                WorldQueryKind::Medium,
            ]
            .into_iter()
            .map(|kind| {
                let plan = lower_world_query_plan(&WorldQueryPlan::for_query_with_backend(
                    kind,
                    DispatchBackend::Wgsl,
                ));
                (
                    kind,
                    wgsl::compile_world_shader(ctx, &plan)
                        .map(|shader| wgsl::bridge_config(&shader))
                        .map_err(|err| SmolStr::new(err.to_string())),
                )
            })
            .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let field_batch_plans = vec![
        BatchQueryPlan::for_field(FieldBatchPlanKind::Distance, CaptureKind::Field)
            .expect("field distance plan"),
        BatchQueryPlan::for_field(FieldBatchPlanKind::Distance, CaptureKind::Shape)
            .expect("shape distance plan"),
        BatchQueryPlan::for_field(FieldBatchPlanKind::Normal, CaptureKind::Field)
            .expect("field normal plan"),
        BatchQueryPlan::for_field(FieldBatchPlanKind::Normal, CaptureKind::Shape)
            .expect("shape normal plan"),
    ];
    let shape_batch_plans = vec![
        BatchQueryPlan::for_shape(ShapeBatchPlanKind::Trace),
        BatchQueryPlan::for_shape(ShapeBatchPlanKind::Surface),
        BatchQueryPlan::for_shape(ShapeBatchPlanKind::Occluded),
    ];
    let batch_wgsl_configs = wgsl_query_ctx
        .as_ref()
        .map(|ctx| {
            field_batch_plans
                .iter()
                .chain(shape_batch_plans.iter())
                .map(|plan| {
                    let lowered = lower_batch_query_plan(plan);
                    (
                        plan.helper_name.clone(),
                        wgsl::compile_batch_shader(ctx, &lowered)
                            .map(|shader| wgsl::bridge_config(&shader))
                            .map_err(|err| SmolStr::new(err.to_string())),
                    )
                })
                .collect::<HashMap<_, _>>()
        })
        .unwrap_or_default();
    for (_idx, func) in module.functions.iter() {
        let Some(body) = &func.body else {
            continue;
        };
        let is_method = method_ids.contains(&_idx.into_raw());
        let fn_types = type_info
            .function(_idx)
            .expect("missing type info for function during MIR lowering");
        let name = if is_method {
            method_qnames
                .get(&_idx)
                .cloned()
                .unwrap_or_else(|| func.name.clone())
        } else {
            func.name.clone()
        };
        functions.push(lower_function(
            module,
            _idx,
            func,
            name,
            body,
            type_info,
            default_query_backend,
            &tag_map,
            &class_fields,
            &class_field_defaults,
            &function_names,
            &field_names,
            &shape_names,
            &shape_graphs,
            &field_graphs,
            &field_bodies,
            &field_metadata,
            &radiance_param_counts,
            &volume_param_counts,
            &result_functions,
            &class_method_ids,
            &interface_methods,
            is_method,
            fn_types,
        ));
    }

    for (_, shape) in module.shapes.iter() {
        if shape.graph.is_none() {
            continue;
        }
        functions.push(lower_shape_distance_helper(
            shape,
            ShapeExecutionMode::SupportPruned,
            &tag_map,
            &class_fields,
            &class_field_defaults,
            &function_names,
            &field_names,
            &shape_names,
            &shape_graphs,
            &field_graphs,
            &field_bodies,
            &field_metadata,
            &radiance_param_counts,
            &volume_param_counts,
            &result_functions,
            &class_method_ids,
            &interface_methods,
        ));
        functions.push(lower_shape_distance_helper(
            shape,
            ShapeExecutionMode::Conservative,
            &tag_map,
            &class_fields,
            &class_field_defaults,
            &function_names,
            &field_names,
            &shape_names,
            &shape_graphs,
            &field_graphs,
            &field_bodies,
            &field_metadata,
            &radiance_param_counts,
            &volume_param_counts,
            &result_functions,
            &class_method_ids,
            &interface_methods,
        ));
        functions.push(lower_shape_trace_helper(
            shape,
            ShapeExecutionMode::SupportPruned,
            module,
            &tag_map,
            &class_fields,
            &class_field_defaults,
            &function_names,
            &field_names,
            &shape_names,
            &shape_graphs,
            &field_graphs,
            &field_bodies,
            &field_metadata,
            &radiance_param_counts,
            &volume_param_counts,
            &result_functions,
            &class_method_ids,
            &interface_methods,
        ));
        functions.push(lower_shape_trace_helper(
            shape,
            ShapeExecutionMode::Conservative,
            module,
            &tag_map,
            &class_fields,
            &class_field_defaults,
            &function_names,
            &field_names,
            &shape_names,
            &shape_graphs,
            &field_graphs,
            &field_bodies,
            &field_metadata,
            &radiance_param_counts,
            &volume_param_counts,
            &result_functions,
            &class_method_ids,
            &interface_methods,
        ));
        functions.push(lower_shape_surface_helper(
            shape,
            module,
            &tag_map,
            &class_fields,
            &class_field_defaults,
            &function_names,
            &field_names,
            &shape_names,
            &shape_graphs,
            &field_graphs,
            &field_bodies,
            &field_metadata,
            &radiance_param_counts,
            &volume_param_counts,
            &result_functions,
            &class_method_ids,
            &interface_methods,
        ));
    }

    functions.push(lower_scene_distance_capture_helper(
        module,
        &tag_map,
        &class_fields,
        &class_field_defaults,
        &function_names,
        &field_names,
        &shape_names,
        &shape_graphs,
        &field_graphs,
        &field_bodies,
        &field_metadata,
        &radiance_param_counts,
        &volume_param_counts,
        &result_functions,
        &class_method_ids,
        &interface_methods,
        "FieldCapture",
        "__wr_field_distance_capture",
    ));
    functions.push(lower_scene_normal_capture_helper(
        module,
        &tag_map,
        &class_fields,
        &class_field_defaults,
        &function_names,
        &field_names,
        &shape_names,
        &shape_graphs,
        &field_graphs,
        &field_bodies,
        &field_metadata,
        &radiance_param_counts,
        &volume_param_counts,
        &result_functions,
        &class_method_ids,
        &interface_methods,
        "FieldCapture",
        "__wr_field_normal_capture",
    ));
    functions.push(lower_scene_distance_capture_helper(
        module,
        &tag_map,
        &class_fields,
        &class_field_defaults,
        &function_names,
        &field_names,
        &shape_names,
        &shape_graphs,
        &field_graphs,
        &field_bodies,
        &field_metadata,
        &radiance_param_counts,
        &volume_param_counts,
        &result_functions,
        &class_method_ids,
        &interface_methods,
        "ShapeCapture",
        "__wr_shape_distance_capture",
    ));
    functions.push(lower_scene_normal_capture_helper(
        module,
        &tag_map,
        &class_fields,
        &class_field_defaults,
        &function_names,
        &field_names,
        &shape_names,
        &shape_graphs,
        &field_graphs,
        &field_bodies,
        &field_metadata,
        &radiance_param_counts,
        &volume_param_counts,
        &result_functions,
        &class_method_ids,
        &interface_methods,
        "ShapeCapture",
        "__wr_shape_normal_capture",
    ));
    functions.push(lower_scene_trace_capture_helper(
        module,
        &tag_map,
        &class_fields,
        &class_field_defaults,
        &function_names,
        &field_names,
        &shape_names,
        &shape_graphs,
        &field_graphs,
        &field_bodies,
        &field_metadata,
        &radiance_param_counts,
        &volume_param_counts,
        &result_functions,
        &class_method_ids,
        &interface_methods,
    ));
    functions.push(lower_scene_surface_capture_helper(
        module,
        &tag_map,
        &class_fields,
        &class_field_defaults,
        &function_names,
        &field_names,
        &shape_names,
        &shape_graphs,
        &field_graphs,
        &field_bodies,
        &field_metadata,
        &radiance_param_counts,
        &volume_param_counts,
        &result_functions,
        &class_method_ids,
        &interface_methods,
    ));
    functions.push(lower_scene_radiance_capture_helper(
        module,
        &tag_map,
        &class_fields,
        &class_field_defaults,
        &function_names,
        &field_names,
        &shape_names,
        &shape_graphs,
        &field_graphs,
        &field_bodies,
        &field_metadata,
        &radiance_param_counts,
        &volume_param_counts,
        &result_functions,
        &class_method_ids,
        &interface_methods,
    ));
    functions.push(lower_scene_medium_capture_helper(
        module,
        &tag_map,
        &class_fields,
        &class_field_defaults,
        &function_names,
        &field_names,
        &shape_names,
        &shape_graphs,
        &field_graphs,
        &field_bodies,
        &field_metadata,
        &radiance_param_counts,
        &volume_param_counts,
        &result_functions,
        &class_method_ids,
        &interface_methods,
    ));
    functions.push(lower_world_distance_capture_helper(
        module,
        &tag_map,
        &class_fields,
        &class_field_defaults,
        &function_names,
        &field_names,
        &shape_names,
        &shape_graphs,
        &field_graphs,
        &field_bodies,
        &field_metadata,
        &radiance_param_counts,
        &volume_param_counts,
        &result_functions,
        &class_method_ids,
        &interface_methods,
        default_query_backend,
        world_wgsl_configs.get(&WorldQueryKind::Distance),
        &wgsl_shape_indices,
    ));
    functions.push(lower_world_normal_capture_helper(
        module,
        &tag_map,
        &class_fields,
        &class_field_defaults,
        &function_names,
        &field_names,
        &shape_names,
        &shape_graphs,
        &field_graphs,
        &field_bodies,
        &field_metadata,
        &radiance_param_counts,
        &volume_param_counts,
        &result_functions,
        &class_method_ids,
        &interface_methods,
        default_query_backend,
        world_wgsl_configs.get(&WorldQueryKind::Normal),
        &wgsl_shape_indices,
    ));
    functions.push(lower_world_trace_capture_helper(
        module,
        &tag_map,
        &class_fields,
        &class_field_defaults,
        &function_names,
        &field_names,
        &shape_names,
        &shape_graphs,
        &field_graphs,
        &field_bodies,
        &field_metadata,
        &radiance_param_counts,
        &volume_param_counts,
        &result_functions,
        &class_method_ids,
        &interface_methods,
        default_query_backend,
        world_wgsl_configs.get(&WorldQueryKind::Trace),
        &wgsl_shape_indices,
    ));
    functions.push(lower_world_surface_capture_helper(
        module,
        &tag_map,
        &class_fields,
        &class_field_defaults,
        &function_names,
        &field_names,
        &shape_names,
        &shape_graphs,
        &field_graphs,
        &field_bodies,
        &field_metadata,
        &radiance_param_counts,
        &volume_param_counts,
        &result_functions,
        &class_method_ids,
        &interface_methods,
        default_query_backend,
        world_wgsl_configs.get(&WorldQueryKind::Surface),
        &wgsl_shape_indices,
    ));
    functions.push(lower_world_radiance_capture_helper(
        module,
        &tag_map,
        &class_fields,
        &class_field_defaults,
        &function_names,
        &field_names,
        &shape_names,
        &shape_graphs,
        &field_graphs,
        &field_bodies,
        &field_metadata,
        &radiance_param_counts,
        &volume_param_counts,
        &result_functions,
        &class_method_ids,
        &interface_methods,
        default_query_backend,
        world_wgsl_configs.get(&WorldQueryKind::Radiance),
        &wgsl_shape_indices,
    ));
    functions.push(lower_world_medium_capture_helper(
        module,
        &tag_map,
        &class_fields,
        &class_field_defaults,
        &function_names,
        &field_names,
        &shape_names,
        &shape_graphs,
        &field_graphs,
        &field_bodies,
        &field_metadata,
        &radiance_param_counts,
        &volume_param_counts,
        &result_functions,
        &class_method_ids,
        &interface_methods,
        default_query_backend,
        world_wgsl_configs.get(&WorldQueryKind::Medium),
        &wgsl_shape_indices,
    ));
    functions.push(lower_render_shadow_visibility_helper(
        module,
        default_query_backend,
        &tag_map,
        &class_fields,
        &class_field_defaults,
        &function_names,
        &field_names,
        &shape_names,
        &shape_graphs,
        &field_graphs,
        &field_bodies,
        &field_metadata,
        &radiance_param_counts,
        &volume_param_counts,
        &result_functions,
        &class_method_ids,
        &interface_methods,
    ));
    functions.push(lower_render_ambient_occlusion_helper(
        module,
        default_query_backend,
        &tag_map,
        &class_fields,
        &class_field_defaults,
        &function_names,
        &field_names,
        &shape_names,
        &shape_graphs,
        &field_graphs,
        &field_bodies,
        &field_metadata,
        &radiance_param_counts,
        &volume_param_counts,
        &result_functions,
        &class_method_ids,
        &interface_methods,
    ));
    functions.push(lower_render_scene_color_helper(
        module,
        default_query_backend,
        &tag_map,
        &class_fields,
        &class_field_defaults,
        &function_names,
        &field_names,
        &shape_names,
        &shape_graphs,
        &field_graphs,
        &field_bodies,
        &field_metadata,
        &radiance_param_counts,
        &volume_param_counts,
        &result_functions,
        &class_method_ids,
        &interface_methods,
    ));
    functions.push(lower_render_capture_to_ppm_helper(
        module,
        default_query_backend,
        &tag_map,
        &class_fields,
        &class_field_defaults,
        &function_names,
        &field_names,
        &shape_names,
        &shape_graphs,
        &field_graphs,
        &field_bodies,
        &field_metadata,
        &radiance_param_counts,
        &volume_param_counts,
        &result_functions,
        &class_method_ids,
        &interface_methods,
    ));
    functions.push(lower_scene_trace_queries_helper(
        module,
        &tag_map,
        &class_fields,
        &class_field_defaults,
        &function_names,
        &field_names,
        &shape_names,
        &shape_graphs,
        &field_graphs,
        &field_bodies,
        &field_metadata,
        &radiance_param_counts,
        &volume_param_counts,
        &result_functions,
        &class_method_ids,
        &interface_methods,
    ));
    functions.push(lower_scene_surface_queries_helper(
        module,
        &tag_map,
        &class_fields,
        &class_field_defaults,
        &function_names,
        &field_names,
        &shape_names,
        &shape_graphs,
        &field_graphs,
        &field_bodies,
        &field_metadata,
        &radiance_param_counts,
        &volume_param_counts,
        &result_functions,
        &class_method_ids,
        &interface_methods,
    ));
    for plan in &field_batch_plans {
        functions.push(lower_field_batch_queries_helper(
            &tag_map,
            &class_fields,
            &class_field_defaults,
            &function_names,
            &field_names,
            &shape_names,
            &shape_graphs,
            &field_graphs,
            &field_bodies,
            &field_metadata,
            &radiance_param_counts,
            &volume_param_counts,
            &result_functions,
            &class_method_ids,
            &interface_methods,
            plan,
            default_query_backend,
            batch_wgsl_configs.get(&plan.helper_name),
            match plan.capture_kind {
                CaptureKind::Field => &wgsl_field_indices,
                CaptureKind::Shape => &wgsl_shape_indices,
                CaptureKind::Region => unreachable!("field batch helpers do not support region"),
            },
        ));
    }
    for plan in &shape_batch_plans {
        functions.push(lower_shape_batch_queries_helper(
            &tag_map,
            &class_fields,
            &class_field_defaults,
            &function_names,
            &field_names,
            &shape_names,
            &shape_graphs,
            &field_graphs,
            &field_bodies,
            &field_metadata,
            &radiance_param_counts,
            &volume_param_counts,
            &result_functions,
            &class_method_ids,
            &interface_methods,
            plan,
            default_query_backend,
            batch_wgsl_configs.get(&plan.helper_name),
            &wgsl_shape_indices,
        ));
    }

    let dispatch_functions = build_interface_dispatch_functions(module, &interface_impls, &tag_map);
    for func in dispatch_functions {
        functions.push(func);
    }
    MirModule {
        functions,
        type_tags,
        classes,
    }
}

fn lower_function(
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
    if matches!(func.role, FunctionRole::Render) {
        return lower_render_function(
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

fn portable_abi_named_type(
    name: &str,
    module: &hir::Module,
    type_tags: &HashMap<SmolStr, TypeTagId>,
) -> PortableAbiType {
    let ty = hir::TypeRef {
        name: SmolStr::new(name),
        name_span: None,
        args: Vec::new(),
    };
    portable_abi_from_type_ref(Some(&ty), module, type_tags, &mut HashSet::new())
}

fn lower_render_function(
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

    let metadata = func.render.as_ref().expect("render metadata");
    let world_local = func
        .params
        .first()
        .and_then(|param| lowerer.resolve_local(&param.name))
        .expect("render world param");
    let camera_local = func
        .params
        .get(1)
        .and_then(|param| lowerer.resolve_local(&param.name))
        .expect("render camera param");

    let domain = metadata
        .domain
        .as_ref()
        .map(|body| lowerer.lower_wrapped_body_value(body, span))
        .unwrap_or_else(|| build_default_scene_domain_value(&mut lowerer, world_local, span));
    let trace_budget = lower_render_trace_budget_values(module, metadata, &mut lowerer, span);
    let light = metadata
        .light
        .as_ref()
        .map(|body| lowerer.lower_wrapped_body_value(body, span))
        .unwrap_or_else(|| build_default_render_light_value(&mut lowerer, span));
    let width = metadata
        .width
        .as_ref()
        .map(|body| lowerer.lower_wrapped_body_value(body, span))
        .unwrap_or(Value::Const(Literal::Integer(40)));
    let height = metadata
        .height
        .as_ref()
        .map(|body| lowerer.lower_wrapped_body_value(body, span))
        .unwrap_or(Value::Const(Literal::Integer(40)));
    let world_up = metadata
        .world_up
        .as_ref()
        .map(|body| lowerer.lower_wrapped_body_value(body, span))
        .unwrap_or_else(|| {
            lowerer.lower_get_named_field(
                Value::Local(camera_local),
                "Camera",
                "up",
                MirType::Vec3,
                span,
            )
        });
    let view_scale = metadata
        .view_scale
        .as_ref()
        .map(|body| lowerer.lower_wrapped_body_value(body, span))
        .unwrap_or(Value::Const(Literal::Float(0.72)));
    let fill_dir = metadata
        .fill_dir
        .as_ref()
        .map(|body| lowerer.lower_wrapped_body_value(body, span))
        .unwrap_or_else(|| build_default_render_fill_dir_value(&mut lowerer, span));

    let result = lowerer.lower_call_temp(
        MirType::String,
        SmolStr::new("__wr_render_capture_to_ppm"),
        vec![
            Value::Local(world_local),
            domain,
            Value::Local(camera_local),
            light,
            width,
            height,
            world_up,
            view_scale,
            fill_dir,
            trace_budget.max_distance,
            trace_budget.min_step,
            trace_budget.hit_epsilon,
            trace_budget.max_steps,
        ],
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

fn build_vec3_value(lowerer: &mut FunctionLowerer, values: [f64; 3], span: TextRange) -> Value {
    lowerer.lower_call_temp(
        MirType::Vec3,
        SmolStr::new("vec3"),
        vec![
            Value::Const(Literal::Float(values[0])),
            Value::Const(Literal::Float(values[1])),
            Value::Const(Literal::Float(values[2])),
        ],
        span,
    )
}

fn build_default_render_fill_dir_value(lowerer: &mut FunctionLowerer, span: TextRange) -> Value {
    let base = build_vec3_value(lowerer, [-0.9, 0.45, 0.2], span);
    lowerer.lower_call_temp(MirType::Vec3, SmolStr::new("normalize"), vec![base], span)
}

fn build_default_render_light_value(lowerer: &mut FunctionLowerer, span: TextRange) -> Value {
    let mut class = lowerer.synthetic_class_target_info("Light");
    FunctionLowerer::set_class_field_value(
        &mut class,
        "position",
        build_vec3_value(lowerer, [2.4, 2.8, 2.4], span),
    );
    let light_dir = build_vec3_value(lowerer, [-0.8, -0.9, -0.9], span);
    FunctionLowerer::set_class_field_value(
        &mut class,
        "direction",
        lowerer.lower_call_temp(
            MirType::Vec3,
            SmolStr::new("normalize"),
            vec![light_dir],
            span,
        ),
    );
    FunctionLowerer::set_class_field_value(
        &mut class,
        "intensity",
        build_vec3_value(lowerer, [1.0, 0.98, 0.95], span),
    );
    FunctionLowerer::set_class_field_value(&mut class, "range", Value::Const(Literal::Float(12.0)));
    lowerer.build_class_instance(&class, span)
}

fn default_trace_max_distance() -> Value {
    Value::Const(Literal::Float(12.0))
}

fn default_trace_min_step() -> Value {
    Value::Const(Literal::Float(0.02))
}

fn default_trace_hit_epsilon() -> Value {
    Value::Const(Literal::Float(0.001))
}

fn default_trace_max_steps() -> Value {
    Value::Const(Literal::Integer(96))
}

struct RenderTraceBudgetValues {
    max_distance: Value,
    min_step: Value,
    hit_epsilon: Value,
    max_steps: Value,
}

struct RenderDomainTraceSource<'a> {
    function: &'a hir::Function,
    metadata: &'a hir::DomainMetadata,
    call_body: &'a hir::Body,
    call_args: &'a [hir::Arg],
}

fn default_render_trace_budget_values() -> RenderTraceBudgetValues {
    RenderTraceBudgetValues {
        max_distance: default_trace_max_distance(),
        min_step: default_trace_min_step(),
        hit_epsilon: default_trace_hit_epsilon(),
        max_steps: default_trace_max_steps(),
    }
}

fn terminal_expr(body: &hir::Body) -> Option<hir::Idx<Expr>> {
    let stmt = *body.root_stmts.last()?;
    match &body.stmts[stmt] {
        HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => Some(*expr),
        _ => None,
    }
}

fn callee_name_from_expr(body: &hir::Body, expr: hir::Idx<Expr>) -> Option<&SmolStr> {
    match &body.exprs[expr] {
        Expr::Variable(name) => Some(name),
        Expr::TypeApply { callee, .. } => callee_name_from_expr(body, *callee),
        _ => None,
    }
}

fn render_domain_trace_source<'a>(
    module: &'a hir::Module,
    render_metadata: &'a hir::RenderMetadata,
) -> Option<RenderDomainTraceSource<'a>> {
    let call_body = render_metadata.domain.as_ref()?;
    let expr = terminal_expr(call_body)?;
    let Expr::Call { callee, args, .. } = &call_body.exprs[expr] else {
        return None;
    };
    let callee_name = callee_name_from_expr(call_body, *callee)?;
    let function = module.functions.iter().find_map(|(_, func)| {
        (func.role == FunctionRole::Domain && func.name == *callee_name).then_some(func)
    })?;
    let metadata = function.domain.as_ref()?;
    Some(RenderDomainTraceSource {
        function,
        metadata,
        call_body,
        call_args: args,
    })
}

fn domain_arg_expr_for_param(
    args: &[hir::Arg],
    param_index: usize,
    param_name: &SmolStr,
) -> Option<hir::Idx<Expr>> {
    if let Some(value) = args.iter().find_map(|arg| match arg {
        hir::Arg::Named { name, value, .. } if name == param_name => Some(*value),
        _ => None,
    }) {
        return Some(value);
    }

    args.iter()
        .filter_map(|arg| match arg {
            hir::Arg::Positional { value, .. } => Some(*value),
            hir::Arg::Named { .. } => None,
        })
        .nth(param_index)
}

fn lower_domain_budget_value(
    lowerer: &mut FunctionLowerer,
    budget: Option<&hir::Body>,
    default: fn() -> Value,
    span: TextRange,
) -> Value {
    budget
        .map(|body| lowerer.lower_wrapped_body_value(body, span))
        .unwrap_or_else(default)
}

fn lower_render_trace_budget_values(
    module: &hir::Module,
    render_metadata: &hir::RenderMetadata,
    lowerer: &mut FunctionLowerer,
    span: TextRange,
) -> RenderTraceBudgetValues {
    let Some(source) = render_domain_trace_source(module, render_metadata) else {
        return default_render_trace_budget_values();
    };

    let bound_args = source
        .function
        .params
        .iter()
        .enumerate()
        .filter_map(|(idx, param)| {
            let expr = domain_arg_expr_for_param(source.call_args, idx, &param.name)?;
            let value = lowerer.lower_expr(source.call_body, expr);
            Some((param.name.clone(), value))
        })
        .collect::<Vec<_>>();

    lowerer.scopes.push(HashMap::new());
    for (name, value) in bound_args {
        let local = lowerer.new_local(name.clone(), false, MirType::Unknown);
        lowerer.declare_local(name, local);
        lowerer.assign_use(Place::Local(local), value, span);
    }
    let values = RenderTraceBudgetValues {
        max_distance: lower_domain_budget_value(
            lowerer,
            source.metadata.max_distance.as_ref(),
            default_trace_max_distance,
            span,
        ),
        min_step: lower_domain_budget_value(
            lowerer,
            source.metadata.min_step.as_ref(),
            default_trace_min_step,
            span,
        ),
        hit_epsilon: lower_domain_budget_value(
            lowerer,
            source.metadata.hit_epsilon.as_ref(),
            default_trace_hit_epsilon,
            span,
        ),
        max_steps: lower_domain_budget_value(
            lowerer,
            source.metadata.max_steps.as_ref(),
            default_trace_max_steps,
            span,
        ),
    };
    lowerer.scopes.pop();
    values
}

fn build_scene_domain_contract_value(
    lowerer: &mut FunctionLowerer,
    scene_id: Value,
    geometry_detail: Value,
    material: Value,
    radiance: Value,
    media: Value,
    span: TextRange,
) -> Value {
    let mut spatial = lowerer.synthetic_class_target_info("SpatialDomainContract");
    FunctionLowerer::set_class_field_value(&mut spatial, "geometry_detail", geometry_detail);
    FunctionLowerer::set_class_field_value(
        &mut spatial,
        "guarantee",
        Value::Const(Literal::Integer(0)),
    );
    let spatial = lowerer.build_class_instance(&spatial, span);

    let mut surface = lowerer.synthetic_class_target_info("SurfaceDomainContract");
    FunctionLowerer::set_class_field_value(&mut surface, "material", material);
    let surface = lowerer.build_class_instance(&surface, span);

    let mut participants = lowerer.synthetic_class_target_info("ParticipantDomainContract");
    FunctionLowerer::set_class_field_value(&mut participants, "radiance", radiance);
    FunctionLowerer::set_class_field_value(&mut participants, "media", media);
    let participants = lowerer.build_class_instance(&participants, span);

    let mut domain = lowerer.synthetic_class_target_info("SceneDomain");
    FunctionLowerer::set_class_field_value(&mut domain, "scene_id", scene_id);
    FunctionLowerer::set_class_field_value(&mut domain, "spatial", spatial);
    FunctionLowerer::set_class_field_value(&mut domain, "surface", surface);
    FunctionLowerer::set_class_field_value(&mut domain, "participants", participants);
    lowerer.build_class_instance(&domain, span)
}

fn build_default_scene_domain_value(
    lowerer: &mut FunctionLowerer,
    world_local: LocalId,
    span: TextRange,
) -> Value {
    let scene_id = lowerer.lower_get_named_field(
        Value::Local(world_local),
        "RegionCapture",
        "scene_id",
        MirType::Integer,
        span,
    );
    build_scene_domain_contract_value(
        lowerer,
        scene_id,
        Value::Const(Literal::Integer(1)),
        Value::Const(Literal::Boolean(true)),
        Value::Const(Literal::Boolean(true)),
        Value::Const(Literal::Boolean(true)),
        span,
    )
}

fn declare_internal_param(lowerer: &mut FunctionLowerer, name: &str, ty: MirType) -> LocalId {
    let local = lowerer.new_local(SmolStr::new(name), false, ty);
    lowerer.declare_local(SmolStr::new(name), local);
    lowerer.params.push(local);
    local
}

fn lower_render_world_distance_call(
    lowerer: &mut FunctionLowerer,
    world: Value,
    domain: Value,
    point: Value,
    span: TextRange,
) -> Value {
    let plan = WorldQueryPlan::for_query(WorldQueryKind::Distance);
    let backend = Value::Const(Literal::Integer(
        match lowerer.resolve_default_query_backend(DispatchBackend::Auto) {
            DispatchBackend::Cpu => 0,
            DispatchBackend::VirtualGpu => 1,
            DispatchBackend::Wgsl => 2,
            DispatchBackend::Auto => 3,
        },
    ));
    lowerer.lower_call_temp(
        MirType::Float,
        plan.helper_name,
        vec![world, domain, point, backend],
        span,
    )
}

fn lower_render_world_trace_call(
    lowerer: &mut FunctionLowerer,
    world: Value,
    domain: Value,
    ray: Value,
    span: TextRange,
) -> Value {
    let plan = WorldQueryPlan::for_query(WorldQueryKind::Trace);
    let backend = Value::Const(Literal::Integer(
        match lowerer.resolve_default_query_backend(DispatchBackend::Auto) {
            DispatchBackend::Cpu => 0,
            DispatchBackend::VirtualGpu => 1,
            DispatchBackend::Wgsl => 2,
            DispatchBackend::Auto => 3,
        },
    ));
    lowerer.lower_call_temp(
        MirType::Named(SmolStr::new("Hit3")),
        plan.helper_name,
        vec![world, domain, ray, backend],
        span,
    )
}

fn lower_render_world_surface_call(
    lowerer: &mut FunctionLowerer,
    world: Value,
    domain: Value,
    hit: Value,
    span: TextRange,
) -> Value {
    let plan = WorldQueryPlan::for_query(WorldQueryKind::Surface);
    let backend = Value::Const(Literal::Integer(
        match lowerer.resolve_default_query_backend(DispatchBackend::Auto) {
            DispatchBackend::Cpu => 0,
            DispatchBackend::VirtualGpu => 1,
            DispatchBackend::Wgsl => 2,
            DispatchBackend::Auto => 3,
        },
    ));
    lowerer.lower_call_temp(
        MirType::Named(SmolStr::new("Surface")),
        plan.helper_name,
        vec![world, domain, hit, backend],
        span,
    )
}

fn lower_render_world_radiance_call(
    lowerer: &mut FunctionLowerer,
    world: Value,
    domain: Value,
    point: Value,
    direction: Value,
    span: TextRange,
) -> Value {
    let plan = WorldQueryPlan::for_query(WorldQueryKind::Radiance);
    let backend = Value::Const(Literal::Integer(
        match lowerer.resolve_default_query_backend(DispatchBackend::Auto) {
            DispatchBackend::Cpu => 0,
            DispatchBackend::VirtualGpu => 1,
            DispatchBackend::Wgsl => 2,
            DispatchBackend::Auto => 3,
        },
    ));
    let mut sample = lowerer.synthetic_class_target_info("PointDirectionQuery");
    FunctionLowerer::set_class_field_value(&mut sample, "point", point);
    FunctionLowerer::set_class_field_value(&mut sample, "direction", direction);
    let sample = lowerer.build_class_instance(&sample, span);
    lowerer.lower_call_temp(
        MirType::Vec3,
        plan.helper_name,
        vec![world, domain, sample, backend],
        span,
    )
}

fn lower_render_world_medium_call(
    lowerer: &mut FunctionLowerer,
    world: Value,
    domain: Value,
    point: Value,
    span: TextRange,
) -> Value {
    let plan = WorldQueryPlan::for_query(WorldQueryKind::Medium);
    let backend = Value::Const(Literal::Integer(
        match lowerer.resolve_default_query_backend(DispatchBackend::Auto) {
            DispatchBackend::Cpu => 0,
            DispatchBackend::VirtualGpu => 1,
            DispatchBackend::Wgsl => 2,
            DispatchBackend::Auto => 3,
        },
    ));
    lowerer.lower_call_temp(
        MirType::Named(SmolStr::new("Medium")),
        plan.helper_name,
        vec![world, domain, point, backend],
        span,
    )
}

fn lower_render_shadow_visibility_helper(
    module: &hir::Module,
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
) -> MirFunction {
    let span = TextRange::empty(0.into());
    let mut lowerer = FunctionLowerer::new(
        SmolStr::new("__wr_render_shadow_visibility_capture"),
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
        None,
    );
    lowerer.default_query_backend = default_query_backend;

    let world = declare_internal_param(
        &mut lowerer,
        "world",
        MirType::Named(SmolStr::new("RegionCapture")),
    );
    let domain = declare_internal_param(
        &mut lowerer,
        "domain",
        MirType::Named(SmolStr::new("SceneDomain")),
    );
    let hit_position = declare_internal_param(&mut lowerer, "hit_position", MirType::Vec3);
    let hit_normal = declare_internal_param(&mut lowerer, "hit_normal", MirType::Vec3);
    let light =
        declare_internal_param(&mut lowerer, "light", MirType::Named(SmolStr::new("Light")));
    let trace_max_distance =
        declare_internal_param(&mut lowerer, "trace_max_distance", MirType::Float);
    let trace_min_step = declare_internal_param(&mut lowerer, "trace_min_step", MirType::Float);
    let trace_hit_epsilon =
        declare_internal_param(&mut lowerer, "trace_hit_epsilon", MirType::Float);
    let trace_max_steps = declare_internal_param(&mut lowerer, "trace_max_steps", MirType::Integer);

    let entry = lowerer.new_block();
    let hit_block = lowerer.new_block();
    let miss_block = lowerer.new_block();
    let join_block = lowerer.new_block();
    lowerer.current_block = entry;

    let normal_bias = lowerer.lower_binary_temp(
        MirType::Vec3,
        BinaryOp::Mul,
        Value::Local(hit_normal),
        Value::Const(Literal::Float(0.01)),
        span,
    );
    let shadow_origin = lowerer.lower_binary_temp(
        MirType::Vec3,
        BinaryOp::Add,
        Value::Local(hit_position),
        normal_bias,
        span,
    );
    let light_position = lowerer.lower_get_named_field(
        Value::Local(light),
        "Light",
        "position",
        MirType::Vec3,
        span,
    );
    let light_range =
        lowerer.lower_get_named_field(Value::Local(light), "Light", "range", MirType::Float, span);
    let light_delta = lowerer.lower_binary_temp(
        MirType::Vec3,
        BinaryOp::Sub,
        light_position,
        shadow_origin.clone(),
        span,
    );
    let shadow_direction = lowerer.lower_call_temp(
        MirType::Vec3,
        SmolStr::new("normalize"),
        vec![light_delta.clone()],
        span,
    );
    let light_distance = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("length"),
        vec![light_delta],
        span,
    );
    let light_limit = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("min"),
        vec![light_distance, light_range],
        span,
    );
    let shadow_limit = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("min"),
        vec![light_limit, Value::Local(trace_max_distance)],
        span,
    );
    let shadow_ray = lowerer.build_ray_query_value(
        shadow_origin,
        shadow_direction,
        shadow_limit,
        Value::Local(trace_min_step),
        Value::Local(trace_hit_epsilon),
        Value::Local(trace_max_steps),
        span,
    );
    let shadow_hit = lower_render_world_trace_call(
        &mut lowerer,
        Value::Local(world),
        Value::Local(domain),
        shadow_ray,
        span,
    );
    let shadow_hit_flag =
        lowerer.lower_get_named_field(shadow_hit, "Hit3", "hit", MirType::Boolean, span);
    let result_local = lowerer.new_local(SmolStr::new("$shadow_visibility"), true, MirType::Float);
    lowerer.assign_use(
        Place::Local(result_local),
        Value::Const(Literal::Float(1.0)),
        span,
    );
    lowerer.set_terminator(Terminator::Branch {
        cond: shadow_hit_flag,
        then_target: hit_block,
        else_target: miss_block,
        span,
    });

    lowerer.current_block = hit_block;
    lowerer.assign_use(
        Place::Local(result_local),
        Value::Const(Literal::Float(0.0)),
        span,
    );
    lowerer.set_terminator(Terminator::Jump {
        target: join_block,
        span,
    });

    lowerer.current_block = miss_block;
    lowerer.assign_use(
        Place::Local(result_local),
        Value::Const(Literal::Float(1.0)),
        span,
    );
    lowerer.set_terminator(Terminator::Jump {
        target: join_block,
        span,
    });

    lowerer.current_block = join_block;
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Local(result_local)),
        span,
    });

    MirFunction {
        name: lowerer.name,
        params: lowerer.params,
        abi_params: vec![
            portable_abi_named_type("RegionCapture", module, type_tags),
            portable_abi_named_type("SceneDomain", module, type_tags),
            PortableAbiType::Vec3,
            PortableAbiType::Vec3,
            portable_abi_named_type("Light", module, type_tags),
            PortableAbiType::F32,
            PortableAbiType::F32,
            PortableAbiType::F32,
            PortableAbiType::I32,
        ],
        abi_return: PortableAbiType::F32,
        locals: lowerer.locals,
        temps: lowerer.temps,
        blocks: lowerer.blocks,
        entry,
        suspendable: false,
    }
}

fn lower_render_ambient_occlusion_helper(
    module: &hir::Module,
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
) -> MirFunction {
    let span = TextRange::empty(0.into());
    let mut lowerer = FunctionLowerer::new(
        SmolStr::new("__wr_render_ambient_occlusion_capture"),
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
        None,
    );
    lowerer.default_query_backend = default_query_backend;

    let world = declare_internal_param(
        &mut lowerer,
        "world",
        MirType::Named(SmolStr::new("RegionCapture")),
    );
    let domain = declare_internal_param(
        &mut lowerer,
        "domain",
        MirType::Named(SmolStr::new("SceneDomain")),
    );
    let hit_position = declare_internal_param(&mut lowerer, "hit_position", MirType::Vec3);
    let hit_normal = declare_internal_param(&mut lowerer, "hit_normal", MirType::Vec3);

    let entry = lowerer.new_block();
    lowerer.current_block = entry;
    let sample_a_offset = lowerer.lower_binary_temp(
        MirType::Vec3,
        BinaryOp::Mul,
        Value::Local(hit_normal),
        Value::Const(Literal::Float(0.06)),
        span,
    );
    let sample_a_point = lowerer.lower_binary_temp(
        MirType::Vec3,
        BinaryOp::Add,
        Value::Local(hit_position),
        sample_a_offset,
        span,
    );
    let sample_a = lower_render_world_distance_call(
        &mut lowerer,
        Value::Local(world),
        Value::Local(domain),
        sample_a_point,
        span,
    );
    let sample_b_offset = lowerer.lower_binary_temp(
        MirType::Vec3,
        BinaryOp::Mul,
        Value::Local(hit_normal),
        Value::Const(Literal::Float(0.14)),
        span,
    );
    let sample_b_point = lowerer.lower_binary_temp(
        MirType::Vec3,
        BinaryOp::Add,
        Value::Local(hit_position),
        sample_b_offset,
        span,
    );
    let sample_b = lower_render_world_distance_call(
        &mut lowerer,
        Value::Local(world),
        Value::Local(domain),
        sample_b_point,
        span,
    );
    let sample_c_offset = lowerer.lower_binary_temp(
        MirType::Vec3,
        BinaryOp::Mul,
        Value::Local(hit_normal),
        Value::Const(Literal::Float(0.28)),
        span,
    );
    let sample_c_point = lowerer.lower_binary_temp(
        MirType::Vec3,
        BinaryOp::Add,
        Value::Local(hit_position),
        sample_c_offset,
        span,
    );
    let sample_c = lower_render_world_distance_call(
        &mut lowerer,
        Value::Local(world),
        Value::Local(domain),
        sample_c_point,
        span,
    );

    let sample_a_gap = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Sub,
        Value::Const(Literal::Float(0.06)),
        sample_a,
        span,
    );
    let sample_b_gap = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Sub,
        Value::Const(Literal::Float(0.14)),
        sample_b,
        span,
    );
    let sample_c_gap = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Sub,
        Value::Const(Literal::Float(0.28)),
        sample_c,
        span,
    );
    let term_a = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Mul,
        sample_a_gap,
        Value::Const(Literal::Float(1.6)),
        span,
    );
    let term_b = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Mul,
        sample_b_gap,
        Value::Const(Literal::Float(1.1)),
        span,
    );
    let term_c = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Mul,
        sample_c_gap,
        Value::Const(Literal::Float(0.8)),
        span,
    );
    let term_ab = lowerer.lower_binary_temp(MirType::Float, BinaryOp::Add, term_a, term_b, span);
    let occlusion_sum =
        lowerer.lower_binary_temp(MirType::Float, BinaryOp::Add, term_ab, term_c, span);
    let occlusion = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("clamp"),
        vec![
            occlusion_sum,
            Value::Const(Literal::Float(0.0)),
            Value::Const(Literal::Float(1.0)),
        ],
        span,
    );
    let occlusion_scaled = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Mul,
        occlusion,
        Value::Const(Literal::Float(0.85)),
        span,
    );
    let ao = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Sub,
        Value::Const(Literal::Float(1.0)),
        occlusion_scaled,
        span,
    );
    lowerer.set_terminator(Terminator::Return {
        value: Some(ao),
        span,
    });

    MirFunction {
        name: lowerer.name,
        params: lowerer.params,
        abi_params: vec![
            portable_abi_named_type("RegionCapture", module, type_tags),
            portable_abi_named_type("SceneDomain", module, type_tags),
            PortableAbiType::Vec3,
            PortableAbiType::Vec3,
        ],
        abi_return: PortableAbiType::F32,
        locals: lowerer.locals,
        temps: lowerer.temps,
        blocks: lowerer.blocks,
        entry,
        suspendable: false,
    }
}

fn lower_render_scene_color_helper(
    module: &hir::Module,
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
) -> MirFunction {
    let span = TextRange::empty(0.into());
    let mut lowerer = FunctionLowerer::new(
        SmolStr::new("__wr_render_scene_color_capture"),
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
        None,
    );
    lowerer.default_query_backend = default_query_backend;

    let world = declare_internal_param(
        &mut lowerer,
        "world",
        MirType::Named(SmolStr::new("RegionCapture")),
    );
    let domain = declare_internal_param(
        &mut lowerer,
        "domain",
        MirType::Named(SmolStr::new("SceneDomain")),
    );
    let camera_position = declare_internal_param(&mut lowerer, "camera_position", MirType::Vec3);
    let light =
        declare_internal_param(&mut lowerer, "light", MirType::Named(SmolStr::new("Light")));
    let ray_direction = declare_internal_param(&mut lowerer, "ray_direction", MirType::Vec3);
    let fill_dir = declare_internal_param(&mut lowerer, "fill_dir", MirType::Vec3);
    let trace_max_distance =
        declare_internal_param(&mut lowerer, "trace_max_distance", MirType::Float);
    let trace_min_step = declare_internal_param(&mut lowerer, "trace_min_step", MirType::Float);
    let trace_hit_epsilon =
        declare_internal_param(&mut lowerer, "trace_hit_epsilon", MirType::Float);
    let trace_max_steps = declare_internal_param(&mut lowerer, "trace_max_steps", MirType::Integer);

    let entry = lowerer.new_block();
    let hit_block = lowerer.new_block();
    let miss_block = lowerer.new_block();
    let join_block = lowerer.new_block();
    lowerer.current_block = entry;

    let camera_ray = lowerer.build_ray_query_value(
        Value::Local(camera_position),
        Value::Local(ray_direction),
        Value::Local(trace_max_distance),
        Value::Local(trace_min_step),
        Value::Local(trace_hit_epsilon),
        Value::Local(trace_max_steps),
        span,
    );
    let hit = lower_render_world_trace_call(
        &mut lowerer,
        Value::Local(world),
        Value::Local(domain),
        camera_ray,
        span,
    );
    let hit_flag =
        lowerer.lower_get_named_field(hit.clone(), "Hit3", "hit", MirType::Boolean, span);
    let result_local = lowerer.new_local(SmolStr::new("$scene_color"), true, MirType::Vec3);
    let black = build_vec3_value(&mut lowerer, [0.0, 0.0, 0.0], span);
    lowerer.assign_use(Place::Local(result_local), black, span);
    lowerer.set_terminator(Terminator::Branch {
        cond: hit_flag,
        then_target: hit_block,
        else_target: miss_block,
        span,
    });

    lowerer.current_block = hit_block;
    let hit_position =
        lowerer.lower_get_named_field(hit.clone(), "Hit3", "position", MirType::Vec3, span);
    let hit_normal =
        lowerer.lower_get_named_field(hit.clone(), "Hit3", "normal", MirType::Vec3, span);
    let surface = lower_render_world_surface_call(
        &mut lowerer,
        Value::Local(world),
        Value::Local(domain),
        hit.clone(),
        span,
    );
    let captured_radiance = lower_render_world_radiance_call(
        &mut lowerer,
        Value::Local(world),
        Value::Local(domain),
        hit_position.clone(),
        Value::Local(ray_direction),
        span,
    );
    let medium = lower_render_world_medium_call(
        &mut lowerer,
        Value::Local(world),
        Value::Local(domain),
        hit_position.clone(),
        span,
    );
    let light_position = lowerer.lower_get_named_field(
        Value::Local(light),
        "Light",
        "position",
        MirType::Vec3,
        span,
    );
    let light_intensity = lowerer.lower_get_named_field(
        Value::Local(light),
        "Light",
        "intensity",
        MirType::Vec3,
        span,
    );
    let light_range =
        lowerer.lower_get_named_field(Value::Local(light), "Light", "range", MirType::Float, span);
    let key_delta = lowerer.lower_binary_temp(
        MirType::Vec3,
        BinaryOp::Sub,
        light_position,
        hit_position.clone(),
        span,
    );
    let key_dir = lowerer.lower_call_temp(
        MirType::Vec3,
        SmolStr::new("normalize"),
        vec![key_delta.clone()],
        span,
    );
    let view_delta = lowerer.lower_binary_temp(
        MirType::Vec3,
        BinaryOp::Sub,
        Value::Local(camera_position),
        hit_position.clone(),
        span,
    );
    let view_dir = lowerer.lower_call_temp(
        MirType::Vec3,
        SmolStr::new("normalize"),
        vec![view_delta],
        span,
    );
    let half_sum = lowerer.lower_binary_temp(
        MirType::Vec3,
        BinaryOp::Add,
        key_dir.clone(),
        view_dir.clone(),
        span,
    );
    let half_dir = lowerer.lower_call_temp(
        MirType::Vec3,
        SmolStr::new("normalize"),
        vec![half_sum],
        span,
    );
    let distance_to_light = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("length"),
        vec![key_delta],
        span,
    );
    let light_distance_ratio = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Div,
        distance_to_light.clone(),
        light_range,
        span,
    );
    let attenuation_base = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Sub,
        Value::Const(Literal::Float(1.0)),
        light_distance_ratio,
        span,
    );
    let attenuation = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("clamp"),
        vec![
            attenuation_base,
            Value::Const(Literal::Float(0.0)),
            Value::Const(Literal::Float(1.0)),
        ],
        span,
    );
    let ao = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("__wr_render_ambient_occlusion_capture"),
        vec![
            Value::Local(world),
            Value::Local(domain),
            hit_position.clone(),
            hit_normal.clone(),
        ],
        span,
    );
    let shadow = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("__wr_render_shadow_visibility_capture"),
        vec![
            Value::Local(world),
            Value::Local(domain),
            hit_position.clone(),
            hit_normal.clone(),
            Value::Local(light),
            Value::Local(trace_max_distance),
            Value::Local(trace_min_step),
            Value::Local(trace_hit_epsilon),
            Value::Local(trace_max_steps),
        ],
        span,
    );
    let ndotl_raw = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("dot"),
        vec![hit_normal.clone(), key_dir.clone()],
        span,
    );
    let ndotl = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("max"),
        vec![ndotl_raw, Value::Const(Literal::Float(0.0))],
        span,
    );
    let ndotv_raw = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("dot"),
        vec![hit_normal.clone(), view_dir.clone()],
        span,
    );
    let _ndotv = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("max"),
        vec![ndotv_raw, Value::Const(Literal::Float(0.0))],
        span,
    );
    let ndoth_raw = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("dot"),
        vec![hit_normal.clone(), half_dir.clone()],
        span,
    );
    let ndoth = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("max"),
        vec![ndoth_raw, Value::Const(Literal::Float(0.0))],
        span,
    );
    let diffuse_base =
        lowerer.lower_binary_temp(MirType::Float, BinaryOp::Mul, ndotl, attenuation, span);
    let diffuse =
        lowerer.lower_binary_temp(MirType::Float, BinaryOp::Mul, diffuse_base, shadow, span);
    let fill_dot_raw = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("dot"),
        vec![hit_normal.clone(), Value::Local(fill_dir)],
        span,
    );
    let fill_dot = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("max"),
        vec![fill_dot_raw, Value::Const(Literal::Float(0.0))],
        span,
    );
    let fill = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Mul,
        fill_dot,
        Value::Const(Literal::Float(0.22)),
        span,
    );
    let roughness = lowerer.lower_get_named_field(
        surface.clone(),
        "Surface",
        "roughness",
        MirType::Float,
        span,
    );
    let roughness_clamped = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("clamp"),
        vec![
            roughness,
            Value::Const(Literal::Float(0.0)),
            Value::Const(Literal::Float(1.0)),
        ],
        span,
    );
    let spec_power = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("mix"),
        vec![
            Value::Const(Literal::Float(48.0)),
            Value::Const(Literal::Float(8.0)),
            roughness_clamped,
        ],
        span,
    );
    let metalness = lowerer.lower_get_named_field(
        surface.clone(),
        "Surface",
        "metalness",
        MirType::Float,
        span,
    );
    let clearcoat = lowerer.lower_get_named_field(
        surface.clone(),
        "Surface",
        "clearcoat",
        MirType::Float,
        span,
    );
    let spec_raw = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("pow"),
        vec![ndoth, spec_power],
        span,
    );
    let metalness_term = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Mul,
        metalness.clone(),
        Value::Const(Literal::Float(0.25)),
        span,
    );
    let specular_strength_a = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Add,
        Value::Const(Literal::Float(0.10)),
        metalness_term,
        span,
    );
    let clearcoat_term = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Mul,
        clearcoat,
        Value::Const(Literal::Float(0.20)),
        span,
    );
    let specular_strength = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Add,
        specular_strength_a,
        clearcoat_term,
        span,
    );
    let highlight = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Mul,
        spec_raw,
        specular_strength.clone(),
        span,
    );
    let lighting_a = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Add,
        Value::Const(Literal::Float(0.12)),
        diffuse,
        span,
    );
    let lighting_b =
        lowerer.lower_binary_temp(MirType::Float, BinaryOp::Add, lighting_a, fill, span);
    let lighting =
        lowerer.lower_binary_temp(MirType::Float, BinaryOp::Mul, lighting_b, ao.clone(), span);
    let albedo =
        lowerer.lower_get_named_field(surface.clone(), "Surface", "albedo", MirType::Vec3, span);
    let intensity_x = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("__wr_vec_component"),
        vec![light_intensity.clone(), Value::Const(Literal::Integer(0))],
        span,
    );
    let intensity_y = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("__wr_vec_component"),
        vec![light_intensity.clone(), Value::Const(Literal::Integer(1))],
        span,
    );
    let intensity_z = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("__wr_vec_component"),
        vec![light_intensity, Value::Const(Literal::Integer(2))],
        span,
    );
    let albedo_x = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("__wr_vec_component"),
        vec![albedo.clone(), Value::Const(Literal::Integer(0))],
        span,
    );
    let albedo_y = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("__wr_vec_component"),
        vec![albedo.clone(), Value::Const(Literal::Integer(1))],
        span,
    );
    let albedo_z = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("__wr_vec_component"),
        vec![albedo, Value::Const(Literal::Integer(2))],
        span,
    );
    let direct_x_base = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Mul,
        albedo_x,
        lighting.clone(),
        span,
    );
    let direct_x_lit = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Mul,
        direct_x_base,
        intensity_x,
        span,
    );
    let direct_x_highlight = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Mul,
        highlight.clone(),
        Value::Const(Literal::Float(220.0)),
        span,
    );
    let direct_x_sum = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Add,
        direct_x_lit,
        direct_x_highlight,
        span,
    );
    let direct_x = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("clamp"),
        vec![
            direct_x_sum,
            Value::Const(Literal::Float(0.0)),
            Value::Const(Literal::Float(255.0)),
        ],
        span,
    );
    let direct_y_base = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Mul,
        albedo_y,
        lighting.clone(),
        span,
    );
    let direct_y_lit = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Mul,
        direct_y_base,
        intensity_y,
        span,
    );
    let direct_y_highlight = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Mul,
        highlight.clone(),
        Value::Const(Literal::Float(208.0)),
        span,
    );
    let direct_y_sum = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Add,
        direct_y_lit,
        direct_y_highlight,
        span,
    );
    let direct_y = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("clamp"),
        vec![
            direct_y_sum,
            Value::Const(Literal::Float(0.0)),
            Value::Const(Literal::Float(255.0)),
        ],
        span,
    );
    let direct_z_base =
        lowerer.lower_binary_temp(MirType::Float, BinaryOp::Mul, albedo_z, lighting, span);
    let direct_z_lit = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Mul,
        direct_z_base,
        intensity_z,
        span,
    );
    let direct_z_highlight = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Mul,
        highlight.clone(),
        Value::Const(Literal::Float(196.0)),
        span,
    );
    let direct_z_sum = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Add,
        direct_z_lit,
        direct_z_highlight,
        span,
    );
    let direct_z = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("clamp"),
        vec![
            direct_z_sum,
            Value::Const(Literal::Float(0.0)),
            Value::Const(Literal::Float(255.0)),
        ],
        span,
    );
    let direct = lowerer.lower_call_temp(
        MirType::Vec3,
        SmolStr::new("vec3"),
        vec![direct_x, direct_y, direct_z],
        span,
    );
    let medium_density =
        lowerer.lower_get_named_field(medium.clone(), "Medium", "density", MirType::Float, span);
    let fog_distance = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Mul,
        medium_density,
        distance_to_light.clone(),
        span,
    );
    let fog_distance_scaled = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Mul,
        fog_distance,
        Value::Const(Literal::Float(0.18)),
        span,
    );
    let one_minus_ao = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Sub,
        Value::Const(Literal::Float(1.0)),
        ao,
        span,
    );
    let fog_occlusion = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Mul,
        one_minus_ao,
        Value::Const(Literal::Float(0.08)),
        span,
    );
    let fog_sum = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Add,
        fog_distance_scaled,
        fog_occlusion,
        span,
    );
    let fog_strength = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("clamp"),
        vec![
            fog_sum,
            Value::Const(Literal::Float(0.0)),
            Value::Const(Literal::Float(0.55)),
        ],
        span,
    );
    let radiance_fog = lowerer.lower_binary_temp(
        MirType::Vec3,
        BinaryOp::Mul,
        captured_radiance.clone(),
        Value::Const(Literal::Float(0.22)),
        span,
    );
    let medium_emission =
        lowerer.lower_get_named_field(medium.clone(), "Medium", "emission", MirType::Vec3, span);
    let fog_color = lowerer.lower_binary_temp(
        MirType::Vec3,
        BinaryOp::Add,
        medium_emission.clone(),
        radiance_fog,
        span,
    );
    let highlight_radiance = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Mul,
        highlight,
        Value::Const(Literal::Float(0.15)),
        span,
    );
    let radiance_scale = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Add,
        Value::Const(Literal::Float(0.25)),
        highlight_radiance,
        span,
    );
    let radiance_lit = lowerer.lower_binary_temp(
        MirType::Vec3,
        BinaryOp::Mul,
        captured_radiance,
        radiance_scale,
        span,
    );
    let surface_emissive =
        lowerer.lower_get_named_field(surface.clone(), "Surface", "emissive", MirType::Vec3, span);
    let lit_base =
        lowerer.lower_binary_temp(MirType::Vec3, BinaryOp::Add, direct, surface_emissive, span);
    let lit = lowerer.lower_binary_temp(MirType::Vec3, BinaryOp::Add, lit_base, radiance_lit, span);
    let hit_color = lowerer.lower_call_temp(
        MirType::Vec3,
        SmolStr::new("mix"),
        vec![lit, fog_color, fog_strength],
        span,
    );
    lowerer.assign_use(Place::Local(result_local), hit_color, span);
    lowerer.set_terminator(Terminator::Jump {
        target: join_block,
        span,
    });

    lowerer.current_block = miss_block;
    let miss_offset = lowerer.lower_binary_temp(
        MirType::Vec3,
        BinaryOp::Mul,
        Value::Local(ray_direction),
        Value::Const(Literal::Float(4.0)),
        span,
    );
    let miss_point = lowerer.lower_binary_temp(
        MirType::Vec3,
        BinaryOp::Add,
        Value::Local(camera_position),
        miss_offset,
        span,
    );
    let miss_radiance = lower_render_world_radiance_call(
        &mut lowerer,
        Value::Local(world),
        Value::Local(domain),
        miss_point.clone(),
        Value::Local(ray_direction),
        span,
    );
    let miss_medium = lower_render_world_medium_call(
        &mut lowerer,
        Value::Local(world),
        Value::Local(domain),
        miss_point,
        span,
    );
    let miss_density = lowerer.lower_get_named_field(
        miss_medium.clone(),
        "Medium",
        "density",
        MirType::Float,
        span,
    );
    let miss_fog_raw = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Mul,
        miss_density,
        Value::Const(Literal::Float(3.0)),
        span,
    );
    let miss_fog = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("clamp"),
        vec![
            miss_fog_raw,
            Value::Const(Literal::Float(0.0)),
            Value::Const(Literal::Float(0.45)),
        ],
        span,
    );
    let miss_emission =
        lowerer.lower_get_named_field(miss_medium, "Medium", "emission", MirType::Vec3, span);
    let miss_radiance_scaled = lowerer.lower_binary_temp(
        MirType::Vec3,
        BinaryOp::Mul,
        miss_radiance.clone(),
        Value::Const(Literal::Float(0.28)),
        span,
    );
    let miss_mix_color = lowerer.lower_binary_temp(
        MirType::Vec3,
        BinaryOp::Add,
        miss_emission,
        miss_radiance_scaled,
        span,
    );
    let miss_color = lowerer.lower_call_temp(
        MirType::Vec3,
        SmolStr::new("mix"),
        vec![miss_radiance, miss_mix_color, miss_fog],
        span,
    );
    lowerer.assign_use(Place::Local(result_local), miss_color, span);
    lowerer.set_terminator(Terminator::Jump {
        target: join_block,
        span,
    });

    lowerer.current_block = join_block;
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Local(result_local)),
        span,
    });

    MirFunction {
        name: lowerer.name,
        params: lowerer.params,
        abi_params: vec![
            portable_abi_named_type("RegionCapture", module, type_tags),
            portable_abi_named_type("SceneDomain", module, type_tags),
            PortableAbiType::Vec3,
            portable_abi_named_type("Light", module, type_tags),
            PortableAbiType::Vec3,
            PortableAbiType::Vec3,
            PortableAbiType::F32,
            PortableAbiType::F32,
            PortableAbiType::F32,
            PortableAbiType::I32,
        ],
        abi_return: PortableAbiType::Vec3,
        locals: lowerer.locals,
        temps: lowerer.temps,
        blocks: lowerer.blocks,
        entry,
        suspendable: false,
    }
}

fn lower_render_capture_to_ppm_helper(
    module: &hir::Module,
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
) -> MirFunction {
    let span = TextRange::empty(0.into());
    let mut lowerer = FunctionLowerer::new(
        SmolStr::new("__wr_render_capture_to_ppm"),
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
        None,
    );
    lowerer.default_query_backend = default_query_backend;

    let world = declare_internal_param(
        &mut lowerer,
        "world",
        MirType::Named(SmolStr::new("RegionCapture")),
    );
    let domain = declare_internal_param(
        &mut lowerer,
        "domain",
        MirType::Named(SmolStr::new("SceneDomain")),
    );
    let camera = declare_internal_param(
        &mut lowerer,
        "camera",
        MirType::Named(SmolStr::new("Camera")),
    );
    let light =
        declare_internal_param(&mut lowerer, "light", MirType::Named(SmolStr::new("Light")));
    let width = declare_internal_param(&mut lowerer, "width", MirType::Integer);
    let height = declare_internal_param(&mut lowerer, "height", MirType::Integer);
    let world_up = declare_internal_param(&mut lowerer, "world_up", MirType::Vec3);
    let view_scale = declare_internal_param(&mut lowerer, "view_scale", MirType::Float);
    let fill_dir = declare_internal_param(&mut lowerer, "fill_dir", MirType::Vec3);
    let trace_max_distance =
        declare_internal_param(&mut lowerer, "trace_max_distance", MirType::Float);
    let trace_min_step = declare_internal_param(&mut lowerer, "trace_min_step", MirType::Float);
    let trace_hit_epsilon =
        declare_internal_param(&mut lowerer, "trace_hit_epsilon", MirType::Float);
    let trace_max_steps = declare_internal_param(&mut lowerer, "trace_max_steps", MirType::Integer);

    let entry = lowerer.new_block();
    let y_head = lowerer.new_block();
    let y_body = lowerer.new_block();
    let x_head = lowerer.new_block();
    let x_body = lowerer.new_block();
    let row_done = lowerer.new_block();
    let exit = lowerer.new_block();
    lowerer.current_block = entry;

    let camera_position = lowerer.lower_get_named_field(
        Value::Local(camera),
        "Camera",
        "position",
        MirType::Vec3,
        span,
    );
    let camera_forward = lowerer.lower_get_named_field(
        Value::Local(camera),
        "Camera",
        "forward",
        MirType::Vec3,
        span,
    );
    let width_float = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("f32"),
        vec![Value::Local(width)],
        span,
    );
    let height_float = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("f32"),
        vec![Value::Local(height)],
        span,
    );
    let aspect = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Div,
        width_float.clone(),
        height_float.clone(),
        span,
    );
    let right_cross = lowerer.lower_call_temp(
        MirType::Vec3,
        SmolStr::new("cross"),
        vec![camera_forward.clone(), Value::Local(world_up)],
        span,
    );
    let right = lowerer.lower_call_temp(
        MirType::Vec3,
        SmolStr::new("normalize"),
        vec![right_cross],
        span,
    );
    let up_cross = lowerer.lower_call_temp(
        MirType::Vec3,
        SmolStr::new("cross"),
        vec![right.clone(), camera_forward.clone()],
        span,
    );
    let up = lowerer.lower_call_temp(
        MirType::Vec3,
        SmolStr::new("normalize"),
        vec![up_cross],
        span,
    );
    let ppm_local = lowerer.new_local(SmolStr::new("$ppm"), true, MirType::String);
    let header = lowerer.lower_string_interp_temp(
        vec![
            StringPartValue::Literal(SmolStr::new("P3\n")),
            StringPartValue::Value(Value::Local(width)),
            StringPartValue::Literal(SmolStr::new(" ")),
            StringPartValue::Value(Value::Local(height)),
            StringPartValue::Literal(SmolStr::new("\n255\n")),
        ],
        span,
    );
    lowerer.assign_use(Place::Local(ppm_local), header, span);
    let y_local = lowerer.new_local(SmolStr::new("$y"), true, MirType::Integer);
    lowerer.assign_use(
        Place::Local(y_local),
        Value::Const(Literal::Integer(0)),
        span,
    );
    let x_local = lowerer.new_local(SmolStr::new("$x"), true, MirType::Integer);
    lowerer.assign_use(
        Place::Local(x_local),
        Value::Const(Literal::Integer(0)),
        span,
    );
    lowerer.set_terminator(Terminator::Jump {
        target: y_head,
        span,
    });

    lowerer.current_block = y_head;
    let y_cond = lowerer.lower_binary_temp(
        MirType::Boolean,
        BinaryOp::Lt,
        Value::Local(y_local),
        Value::Local(height),
        span,
    );
    lowerer.set_terminator(Terminator::Branch {
        cond: y_cond,
        then_target: y_body,
        else_target: exit,
        span,
    });

    lowerer.current_block = y_body;
    lowerer.assign_use(
        Place::Local(x_local),
        Value::Const(Literal::Integer(0)),
        span,
    );
    lowerer.set_terminator(Terminator::Jump {
        target: x_head,
        span,
    });

    lowerer.current_block = x_head;
    let x_cond = lowerer.lower_binary_temp(
        MirType::Boolean,
        BinaryOp::Lt,
        Value::Local(x_local),
        Value::Local(width),
        span,
    );
    lowerer.set_terminator(Terminator::Branch {
        cond: x_cond,
        then_target: x_body,
        else_target: row_done,
        span,
    });

    lowerer.current_block = x_body;
    let x_float = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("f32"),
        vec![Value::Local(x_local)],
        span,
    );
    let y_float = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("f32"),
        vec![Value::Local(y_local)],
        span,
    );
    let sample_u_num = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Add,
        x_float,
        Value::Const(Literal::Float(0.5)),
        span,
    );
    let sample_u = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Div,
        sample_u_num,
        width_float.clone(),
        span,
    );
    let sample_v_num = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Add,
        y_float,
        Value::Const(Literal::Float(0.5)),
        span,
    );
    let sample_v = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Div,
        sample_v_num,
        height_float.clone(),
        span,
    );
    let centered_u = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Sub,
        sample_u,
        Value::Const(Literal::Float(0.5)),
        span,
    );
    let doubled_u = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Mul,
        centered_u,
        Value::Const(Literal::Float(2.0)),
        span,
    );
    let aspect_u = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Mul,
        doubled_u,
        aspect.clone(),
        span,
    );
    let screen_x = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Mul,
        aspect_u,
        Value::Local(view_scale),
        span,
    );
    let centered_v = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Sub,
        Value::Const(Literal::Float(0.5)),
        sample_v,
        span,
    );
    let doubled_v = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Mul,
        centered_v,
        Value::Const(Literal::Float(2.0)),
        span,
    );
    let screen_y = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Mul,
        doubled_v,
        Value::Local(view_scale),
        span,
    );
    let ray_x =
        lowerer.lower_binary_temp(MirType::Vec3, BinaryOp::Mul, right.clone(), screen_x, span);
    let ray_xy = lowerer.lower_binary_temp(
        MirType::Vec3,
        BinaryOp::Add,
        camera_forward.clone(),
        ray_x,
        span,
    );
    let ray_y = lowerer.lower_binary_temp(MirType::Vec3, BinaryOp::Mul, up.clone(), screen_y, span);
    let ray_base = lowerer.lower_binary_temp(MirType::Vec3, BinaryOp::Add, ray_xy, ray_y, span);
    let ray = lowerer.lower_call_temp(
        MirType::Vec3,
        SmolStr::new("normalize"),
        vec![ray_base],
        span,
    );
    let shaded = lowerer.lower_call_temp(
        MirType::Vec3,
        SmolStr::new("__wr_render_scene_color_capture"),
        vec![
            Value::Local(world),
            Value::Local(domain),
            camera_position.clone(),
            Value::Local(light),
            ray,
            Value::Local(fill_dir),
            Value::Local(trace_max_distance),
            Value::Local(trace_min_step),
            Value::Local(trace_hit_epsilon),
            Value::Local(trace_max_steps),
        ],
        span,
    );
    let shaded_x = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("__wr_vec_component"),
        vec![shaded.clone(), Value::Const(Literal::Integer(0))],
        span,
    );
    let shaded_y = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("__wr_vec_component"),
        vec![shaded.clone(), Value::Const(Literal::Integer(1))],
        span,
    );
    let shaded_z = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("__wr_vec_component"),
        vec![shaded, Value::Const(Literal::Integer(2))],
        span,
    );
    let shaded_x_clamped = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("clamp"),
        vec![
            shaded_x,
            Value::Const(Literal::Float(0.0)),
            Value::Const(Literal::Float(255.0)),
        ],
        span,
    );
    let r = lowerer.lower_call_temp(
        MirType::Integer,
        SmolStr::new("i32"),
        vec![shaded_x_clamped],
        span,
    );
    let shaded_y_clamped = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("clamp"),
        vec![
            shaded_y,
            Value::Const(Literal::Float(0.0)),
            Value::Const(Literal::Float(255.0)),
        ],
        span,
    );
    let g = lowerer.lower_call_temp(
        MirType::Integer,
        SmolStr::new("i32"),
        vec![shaded_y_clamped],
        span,
    );
    let shaded_z_clamped = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("clamp"),
        vec![
            shaded_z,
            Value::Const(Literal::Float(0.0)),
            Value::Const(Literal::Float(255.0)),
        ],
        span,
    );
    let b = lowerer.lower_call_temp(
        MirType::Integer,
        SmolStr::new("i32"),
        vec![shaded_z_clamped],
        span,
    );
    let line = lowerer.lower_string_interp_temp(
        vec![
            StringPartValue::Value(r),
            StringPartValue::Literal(SmolStr::new(" ")),
            StringPartValue::Value(g),
            StringPartValue::Literal(SmolStr::new(" ")),
            StringPartValue::Value(b),
            StringPartValue::Literal(SmolStr::new("\n")),
        ],
        span,
    );
    let ppm_next = lowerer.lower_string_concat_temp(Value::Local(ppm_local), line, span);
    lowerer.assign_use(Place::Local(ppm_local), ppm_next, span);
    let x_next = lowerer.lower_binary_temp(
        MirType::Integer,
        BinaryOp::Add,
        Value::Local(x_local),
        Value::Const(Literal::Integer(1)),
        span,
    );
    lowerer.assign_use(Place::Local(x_local), x_next, span);
    lowerer.set_terminator(Terminator::Jump {
        target: x_head,
        span,
    });

    lowerer.current_block = row_done;
    let y_next = lowerer.lower_binary_temp(
        MirType::Integer,
        BinaryOp::Add,
        Value::Local(y_local),
        Value::Const(Literal::Integer(1)),
        span,
    );
    lowerer.assign_use(Place::Local(y_local), y_next, span);
    lowerer.set_terminator(Terminator::Jump {
        target: y_head,
        span,
    });

    lowerer.current_block = exit;
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Local(ppm_local)),
        span,
    });

    MirFunction {
        name: lowerer.name,
        params: lowerer.params,
        abi_params: vec![
            portable_abi_named_type("RegionCapture", module, type_tags),
            portable_abi_named_type("SceneDomain", module, type_tags),
            portable_abi_named_type("Camera", module, type_tags),
            portable_abi_named_type("Light", module, type_tags),
            PortableAbiType::I32,
            PortableAbiType::I32,
            PortableAbiType::Vec3,
            PortableAbiType::F32,
            PortableAbiType::Vec3,
            PortableAbiType::F32,
            PortableAbiType::F32,
            PortableAbiType::F32,
            PortableAbiType::I32,
        ],
        abi_return: PortableAbiType::Value,
        locals: lowerer.locals,
        temps: lowerer.temps,
        blocks: lowerer.blocks,
        entry,
        suspendable: false,
    }
}

pub(crate) struct LoopTarget {
    pub(crate) break_target: BlockId,
    pub(crate) continue_target: BlockId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ShapeExecutionMode {
    SupportPruned,
    Conservative,
}

impl ShapeExecutionMode {
    pub(crate) fn distance_helper_name(self, shape: &SmolStr) -> SmolStr {
        match self {
            ShapeExecutionMode::SupportPruned => {
                SmolStr::new(format!("__wr_shape_distance_{shape}"))
            }
            ShapeExecutionMode::Conservative => {
                SmolStr::new(format!("__wr_shape_distance_conservative_{shape}"))
            }
        }
    }

    pub(crate) fn trace_helper_name(self, shape: &SmolStr) -> SmolStr {
        match self {
            ShapeExecutionMode::SupportPruned => SmolStr::new(format!("__wr_shape_trace_{shape}")),
            ShapeExecutionMode::Conservative => {
                SmolStr::new(format!("__wr_shape_trace_conservative_{shape}"))
            }
        }
    }

    pub(crate) fn allows_support_pruning(self) -> bool {
        matches!(self, ShapeExecutionMode::SupportPruned)
    }
}

pub(crate) struct FunctionLowerer {
    pub(crate) name: SmolStr,
    pub(crate) params: Vec<LocalId>,
    pub(crate) locals: Vec<Local>,
    pub(crate) temps: Vec<Temp>,
    pub(crate) blocks: Vec<BasicBlock>,
    pub(crate) current_block: BlockId,
    pub(crate) suspendable: bool,
    pub(crate) scopes: Vec<HashMap<SmolStr, LocalId>>,
    pub(crate) result_scopes: Vec<HashMap<SmolStr, bool>>,
    pub(crate) loop_stack: Vec<LoopTarget>,
    pub(crate) type_tags: HashMap<SmolStr, TypeTagId>,
    pub(crate) class_fields: HashMap<SmolStr, Vec<SmolStr>>,
    pub(crate) class_field_defaults: HashMap<SmolStr, Vec<Option<hir::FieldDefault>>>,
    pub(crate) class_method_ids: HashMap<SmolStr, HashMap<SmolStr, u32>>,
    pub(crate) interface_methods: HashMap<SmolStr, HashSet<SmolStr>>,
    pub(crate) function_names: HashSet<SmolStr>,
    pub(crate) field_names: HashSet<SmolStr>,
    pub(crate) shape_names: HashSet<SmolStr>,
    pub(crate) shape_graphs: HashMap<SmolStr, hir::ShapeGraph>,
    pub(crate) field_graphs: HashMap<SmolStr, hir::FieldGraph>,
    pub(crate) field_bodies: HashMap<SmolStr, hir::Body>,
    pub(crate) field_metadata: HashMap<SmolStr, hir::FieldMetadata>,
    pub(crate) field_scenes: BTreeMap<SmolStr, scene_ir::FieldScene>,
    pub(crate) shape_scenes: BTreeMap<SmolStr, scene_ir::ShapeScene>,
    pub(crate) radiance_param_counts: HashMap<SmolStr, usize>,
    pub(crate) volume_param_counts: HashMap<SmolStr, usize>,
    pub(crate) result_functions: HashSet<SmolStr>,
    pub(crate) default_query_backend: DispatchBackend,
    pub(crate) returns_result: bool,
    pub(crate) type_info: Option<FunctionTypeInfo>,
    pub(crate) defers: Vec<hir::Idx<hir::Expr>>,
    pub(crate) objective_stack: Vec<hir::Objective>,
}

impl FunctionLowerer {
    pub(crate) fn new(
        name: SmolStr,
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
        returns_result: bool,
        type_info: Option<&FunctionTypeInfo>,
    ) -> Self {
        let scene_field_graphs = field_graphs
            .iter()
            .map(|(name, graph)| (name.clone(), graph.clone()))
            .collect::<BTreeMap<_, _>>();
        let scene_field_bodies = field_bodies
            .iter()
            .map(|(name, body)| (name.clone(), body.clone()))
            .collect::<BTreeMap<_, _>>();
        let scene_field_metadata = field_metadata
            .iter()
            .map(|(name, metadata)| (name.clone(), metadata.clone()))
            .collect::<BTreeMap<_, _>>();
        let scene_shape_graphs = shape_graphs
            .iter()
            .map(|(name, graph)| (name.clone(), graph.clone()))
            .collect::<BTreeMap<_, _>>();
        let field_scenes = scene_ir::lower_field_scenes(
            &scene_field_graphs,
            &scene_field_bodies,
            &scene_field_metadata,
        );
        let shape_scenes = scene_ir::lower_shape_scenes(&scene_shape_graphs, &field_scenes);
        Self {
            name,
            params: Vec::new(),
            locals: Vec::new(),
            temps: Vec::new(),
            blocks: Vec::new(),
            current_block: BlockId(0),
            suspendable: false,
            scopes: vec![HashMap::new()],
            result_scopes: vec![HashMap::new()],
            loop_stack: Vec::new(),
            type_tags: type_tags.clone(),
            class_fields: class_fields.clone(),
            class_field_defaults: class_field_defaults.clone(),
            class_method_ids: class_method_ids.clone(),
            interface_methods: interface_methods.clone(),
            function_names: function_names.clone(),
            field_names: field_names.clone(),
            shape_names: shape_names.clone(),
            shape_graphs: shape_graphs.clone(),
            field_graphs: field_graphs.clone(),
            field_bodies: field_bodies.clone(),
            field_metadata: field_metadata.clone(),
            field_scenes,
            shape_scenes,
            radiance_param_counts: radiance_param_counts.clone(),
            volume_param_counts: volume_param_counts.clone(),
            result_functions: result_functions.clone(),
            default_query_backend: DispatchBackend::Auto,
            returns_result,
            type_info: type_info.cloned(),
            defers: Vec::new(),
            objective_stack: Vec::new(),
        }
    }

    pub(crate) fn current_objective(&self) -> Option<hir::Objective> {
        self.objective_stack.last().copied()
    }

    pub(crate) fn resolve_default_query_backend(
        &self,
        backend: DispatchBackend,
    ) -> DispatchBackend {
        match backend {
            DispatchBackend::Auto => match self.default_query_backend {
                DispatchBackend::Auto | DispatchBackend::Cpu => DispatchBackend::Cpu,
                DispatchBackend::VirtualGpu => DispatchBackend::VirtualGpu,
                DispatchBackend::Wgsl => DispatchBackend::Wgsl,
            },
            explicit => explicit,
        }
    }

    pub(crate) fn dispatch_backend_id(backend: DispatchBackend) -> i64 {
        match backend {
            DispatchBackend::Cpu => 0,
            DispatchBackend::VirtualGpu => 1,
            DispatchBackend::Wgsl => 2,
            DispatchBackend::Auto => 3,
        }
    }

    pub(crate) fn world_query_plan_backend(
        &self,
        body: &hir::Body,
        backend_expr: Option<hir::Idx<hir::Expr>>,
    ) -> DispatchBackend {
        match backend_expr {
            Some(expr_id) => self
                .parse_dispatch_backend_builtin(body, expr_id)
                .and_then(|id| i32::try_from(id).ok().and_then(DispatchBackend::from_id))
                .map(|backend| self.resolve_default_query_backend(backend))
                .unwrap_or(DispatchBackend::Auto),
            None => self.resolve_default_query_backend(DispatchBackend::Auto),
        }
    }

    pub(crate) fn lower_world_query_backend_value(
        &mut self,
        body: &hir::Body,
        backend_expr: Option<hir::Idx<hir::Expr>>,
        span: TextRange,
    ) -> Value {
        match backend_expr {
            Some(expr_id) => {
                if let Some(backend) = self
                    .parse_dispatch_backend_builtin(body, expr_id)
                    .and_then(|id| i32::try_from(id).ok().and_then(DispatchBackend::from_id))
                    .map(|backend| self.resolve_default_query_backend(backend))
                {
                    Value::Const(Literal::Integer(Self::dispatch_backend_id(backend)))
                } else {
                    let backend = self.lower_expr(body, expr_id);
                    self.lower_dispatch_backend_id(backend, span)
                }
            }
            None => Value::Const(Literal::Integer(Self::dispatch_backend_id(
                self.resolve_default_query_backend(DispatchBackend::Auto),
            ))),
        }
    }

    pub(crate) fn new_block(&mut self) -> BlockId {
        let id = BlockId(self.blocks.len());
        self.blocks.push(BasicBlock {
            stmts: Vec::new(),
            terminator: Terminator::Unreachable {
                span: TextRange::empty(0.into()),
            },
        });
        id
    }

    pub(crate) fn block_is_open(&self, block: BlockId) -> bool {
        matches!(
            self.blocks[block.0].terminator,
            Terminator::Unreachable { .. }
        )
    }

    pub(crate) fn set_terminator(&mut self, term: Terminator) {
        self.blocks[self.current_block.0].terminator = term;
    }

    pub(crate) fn push_stmt(&mut self, stmt: Stmt) {
        self.blocks[self.current_block.0].stmts.push(stmt);
    }

    pub(crate) fn local_type_for_name(&self, name: &SmolStr) -> MirType {
        self.type_info
            .as_ref()
            .and_then(|info| info.local_types.get(name))
            .map(mir_type_from_type)
            .unwrap_or(MirType::Unknown)
    }

    pub(crate) fn expr_type(&self, body: &hir::Body, expr_id: hir::Idx<Expr>) -> MirType {
        self.type_info
            .as_ref()
            .and_then(|info| info.expr_type(body, expr_id))
            .map(mir_type_from_type)
            .unwrap_or(MirType::Unknown)
    }

    pub(crate) fn proven_range_induction_type(
        lhs_ty: &MirType,
        rhs_ty: &MirType,
    ) -> Option<MirType> {
        match (lhs_ty, rhs_ty) {
            (MirType::Integer, MirType::Integer) => Some(MirType::Integer),
            (MirType::Float, MirType::Float) => Some(MirType::Float),
            _ => None,
        }
    }

    pub(crate) fn new_temp_for_expr(
        &mut self,
        body: &hir::Body,
        expr_id: hir::Idx<Expr>,
    ) -> TempId {
        let ty = self.expr_type(body, expr_id);
        self.new_temp(ty)
    }

    pub(crate) fn new_temp(&mut self, ty: MirType) -> TempId {
        let id = TempId(self.temps.len());
        self.temps.push(Temp { ty });
        id
    }

    pub(crate) fn new_local(&mut self, name: SmolStr, mutable: bool, ty: MirType) -> LocalId {
        let id = LocalId(self.locals.len());
        self.locals.push(Local { name, mutable, ty });
        id
    }

    pub(crate) fn new_temp_local(&mut self) -> LocalId {
        let name = SmolStr::new(format!("$tmp{}", self.locals.len()));
        self.new_local(name, true, MirType::Unknown)
    }

    pub(crate) fn declare_local(&mut self, name: SmolStr, local: LocalId) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, local);
        }
    }

    pub(crate) fn declare_resultness(&mut self, name: SmolStr, is_result: bool) {
        if let Some(scope) = self.result_scopes.last_mut() {
            scope.insert(name, is_result);
        }
    }

    pub(crate) fn set_resultness(&mut self, name: &SmolStr, is_result: bool) {
        for scope in self.result_scopes.iter_mut().rev() {
            if let Some(entry) = scope.get_mut(name) {
                *entry = is_result;
                return;
            }
        }
    }

    pub(crate) fn resolve_resultness(&self, name: &SmolStr) -> Option<bool> {
        for scope in self.result_scopes.iter().rev() {
            if let Some(result) = scope.get(name) {
                return Some(*result);
            }
        }
        None
    }

    pub(crate) fn resolve_local(&self, name: &SmolStr) -> Option<LocalId> {
        for scope in self.scopes.iter().rev() {
            if let Some(local) = scope.get(name) {
                return Some(*local);
            }
        }
        None
    }

    pub(crate) fn expr_is_result(&self, body: &hir::Body, expr_id: hir::Idx<Expr>) -> bool {
        match &body.exprs[expr_id] {
            Expr::Unary { op, .. } => matches!(op, UnaryOp::Await | UnaryOp::Err),
            Expr::Binary { .. } => false,
            Expr::Crash { .. } => false,
            Expr::Call { callee, .. } => {
                if let Expr::Variable(name) = &body.exprs[*callee] {
                    return self.result_functions.contains(name);
                }
                false
            }
            Expr::Variable(name) => self.resolve_resultness(name).unwrap_or(false),
            _ => false,
        }
    }

    pub(crate) fn enter_scope(&mut self) {
        self.scopes.push(HashMap::new());
        self.result_scopes.push(HashMap::new());
    }

    pub(crate) fn exit_scope(&mut self) {
        self.scopes.pop();
        self.result_scopes.pop();
    }

    pub(crate) fn lower_stmt_block(&mut self, body: &hir::Body, stmts: &[hir::Idx<HirStmt>]) {
        for stmt in stmts {
            if !self.block_is_open(self.current_block) {
                break;
            }
            self.lower_stmt(body, *stmt);
        }
    }

    pub(crate) fn lower_kernel_stmt_block(&mut self, stmts: &[KernelStmt]) {
        for stmt in stmts {
            if !self.block_is_open(self.current_block) {
                break;
            }
            self.lower_kernel_stmt(stmt);
        }
    }

    pub(crate) fn lower_kernel_stmt(&mut self, stmt: &KernelStmt) {
        match stmt {
            KernelStmt::Let {
                name,
                mutable,
                ty,
                value,
                span,
            } => {
                let lowered = self.lower_kernel_expr(value);
                let local = self.new_local(name.clone(), *mutable, mir_type_from_type(ty));
                self.declare_local(name.clone(), local);
                self.declare_resultness(name.clone(), false);
                self.assign_use(Place::Local(local), lowered, *span);
            }
            KernelStmt::Assign {
                name,
                op,
                value,
                span,
            } => {
                let Some(local) = self.resolve_local(name) else {
                    return;
                };
                let rhs = self.lower_kernel_expr(value);
                match op {
                    AssignOp::Assign => self.assign_use(Place::Local(local), rhs, *span),
                    AssignOp::AddAssign
                    | AssignOp::SubAssign
                    | AssignOp::MulAssign
                    | AssignOp::DivAssign => {
                        let binary = match op {
                            AssignOp::AddAssign => BinaryOp::Add,
                            AssignOp::SubAssign => BinaryOp::Sub,
                            AssignOp::MulAssign => BinaryOp::Mul,
                            AssignOp::DivAssign => BinaryOp::Div,
                            AssignOp::Assign => BinaryOp::Assign,
                        };
                        let temp = self.new_temp(MirType::Unknown);
                        self.push_stmt(MirStmt::Assign {
                            place: Place::Temp(temp),
                            value: Rvalue::Binary {
                                op: binary,
                                lhs: Value::Local(local),
                                rhs,
                            },
                            span: *span,
                        });
                        self.assign_use(Place::Local(local), Value::Temp(temp), *span);
                    }
                }
            }
            KernelStmt::Expr { value, .. } | KernelStmt::IgnoreResult { value, .. } => {
                let _ = self.lower_kernel_expr(value);
            }
            KernelStmt::If {
                condition,
                then_block,
                else_block,
                span,
            } => {
                let cond = self.lower_kernel_expr(condition);
                let then_target = self.new_block();
                let else_target = self.new_block();
                let join_target = self.new_block();
                self.set_terminator(Terminator::Branch {
                    cond,
                    then_target,
                    else_target,
                    span: *span,
                });

                self.current_block = then_target;
                self.enter_scope();
                self.lower_kernel_stmt_block(then_block);
                self.exit_scope();
                if self.block_is_open(self.current_block) {
                    self.set_terminator(Terminator::Jump {
                        target: join_target,
                        span: *span,
                    });
                }

                self.current_block = else_target;
                self.enter_scope();
                self.lower_kernel_stmt_block(else_block);
                self.exit_scope();
                if self.block_is_open(self.current_block) {
                    self.set_terminator(Terminator::Jump {
                        target: join_target,
                        span: *span,
                    });
                }

                self.current_block = join_target;
            }
            KernelStmt::While {
                condition,
                body,
                span,
            } => {
                let head_block = self.new_block();
                let body_block = self.new_block();
                let exit_block = self.new_block();
                self.set_terminator(Terminator::Jump {
                    target: head_block,
                    span: *span,
                });

                self.current_block = head_block;
                let cond = self.lower_kernel_expr(condition);
                self.set_terminator(Terminator::Branch {
                    cond,
                    then_target: body_block,
                    else_target: exit_block,
                    span: *span,
                });

                self.current_block = body_block;
                self.loop_stack.push(LoopTarget {
                    break_target: exit_block,
                    continue_target: head_block,
                });
                self.enter_scope();
                self.lower_kernel_stmt_block(body);
                self.exit_scope();
                self.loop_stack.pop();
                if self.block_is_open(self.current_block) {
                    self.set_terminator(Terminator::Jump {
                        target: head_block,
                        span: *span,
                    });
                }
                self.current_block = exit_block;
            }
            KernelStmt::Return { value, span } => {
                let value = value.as_ref().map(|expr| self.lower_kernel_expr(expr));
                self.set_terminator(Terminator::Return { value, span: *span });
            }
            KernelStmt::Break { span } => {
                if let Some(target) = self.loop_stack.last() {
                    self.set_terminator(Terminator::Jump {
                        target: target.break_target,
                        span: *span,
                    });
                }
            }
            KernelStmt::Continue { span } => {
                if let Some(target) = self.loop_stack.last() {
                    self.set_terminator(Terminator::Jump {
                        target: target.continue_target,
                        span: *span,
                    });
                }
            }
        }
    }

    pub(crate) fn lower_kernel_expr(&mut self, expr: &KernelExpr) -> Value {
        match expr {
            KernelExpr::Literal { value, .. } => Value::Const(value.clone()),
            KernelExpr::Var { name, .. } => self
                .resolve_local(name)
                .map(Value::Local)
                .unwrap_or(Value::Const(Literal::Nil)),
            KernelExpr::Unary { op, expr, ty, span } => {
                let operand = self.lower_kernel_expr(expr);
                self.lower_unary_temp(mir_type_from_type(ty), *op, operand, *span)
            }
            KernelExpr::Binary {
                op,
                lhs,
                rhs,
                ty,
                span,
            } => {
                if matches!(op, BinaryOp::And | BinaryOp::Or) {
                    let lhs_val = self.lower_kernel_expr(lhs);
                    let eval_block = self.new_block();
                    let short_block = self.new_block();
                    let join_block = self.new_block();
                    let result_local = self.new_temp_local();
                    self.assign_use(
                        Place::Local(result_local),
                        Value::Const(Literal::Nil),
                        *span,
                    );
                    let (then_target, else_target) = if matches!(op, BinaryOp::And) {
                        (eval_block, short_block)
                    } else {
                        (short_block, eval_block)
                    };
                    self.set_terminator(Terminator::Branch {
                        cond: lhs_val.clone(),
                        then_target,
                        else_target,
                        span: *span,
                    });

                    self.current_block = short_block;
                    self.assign_use(Place::Local(result_local), lhs_val, *span);
                    if self.block_is_open(self.current_block) {
                        self.set_terminator(Terminator::Jump {
                            target: join_block,
                            span: *span,
                        });
                    }

                    self.current_block = eval_block;
                    let rhs_val = self.lower_kernel_expr(rhs);
                    self.assign_use(Place::Local(result_local), rhs_val, *span);
                    if self.block_is_open(self.current_block) {
                        self.set_terminator(Terminator::Jump {
                            target: join_block,
                            span: *span,
                        });
                    }

                    self.current_block = join_block;
                    return Value::Local(result_local);
                }
                let lhs = self.lower_kernel_expr(lhs);
                let rhs = self.lower_kernel_expr(rhs);
                self.lower_binary_temp(mir_type_from_type(ty), *op, lhs, rhs, *span)
            }
            KernelExpr::Crash { expr, span, .. } => {
                let value = self.lower_kernel_expr(expr);
                let temp = self.new_temp(MirType::Unknown);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(temp),
                    value: Rvalue::Crash { value },
                    span: *span,
                });
                Value::Temp(temp)
            }
            KernelExpr::Call {
                target,
                args,
                ty,
                span,
            } => {
                let args = args
                    .iter()
                    .map(|arg| self.lower_kernel_expr(arg))
                    .collect::<Vec<_>>();
                self.lower_call_temp(mir_type_from_type(ty), target.clone(), args, *span)
            }
            KernelExpr::Capture { target, span, .. } => {
                self.build_scene_capture_value(target, *span)
            }
            KernelExpr::DispatchBackend { backend, span, .. } => {
                let id = match backend {
                    DispatchBackend::Cpu => 0,
                    DispatchBackend::VirtualGpu => 1,
                    DispatchBackend::Wgsl => 2,
                    DispatchBackend::Auto => 3,
                };
                self.build_dispatch_backend_value(id, *span)
            }
            KernelExpr::CaptureQuery {
                plan,
                args,
                ty,
                span,
            } => {
                let args = args
                    .iter()
                    .map(|arg| self.lower_kernel_expr(arg))
                    .collect::<Vec<_>>();
                self.lower_call_temp(
                    mir_type_from_type(ty),
                    plan.helper_name.clone(),
                    args,
                    *span,
                )
            }
            KernelExpr::WorldQuery {
                plan,
                args,
                ty,
                span,
            } => {
                let query_arg_count = match plan.kind {
                    WorldQueryKind::Distance | WorldQueryKind::Normal | WorldQueryKind::Medium => 3,
                    WorldQueryKind::Radiance => 4,
                    WorldQueryKind::Trace => 8,
                    WorldQueryKind::Surface => 3,
                };
                let mut lowered_args = args
                    .iter()
                    .take(query_arg_count)
                    .map(|arg| self.lower_kernel_expr(arg))
                    .collect::<Vec<_>>();
                let backend = match args.get(query_arg_count) {
                    Some(backend) => {
                        let backend = self.lower_kernel_expr(backend);
                        self.lower_dispatch_backend_id(backend, *span)
                    }
                    None => Value::Const(Literal::Integer(Self::dispatch_backend_id(plan.backend))),
                };
                lowered_args.push(backend);
                self.lower_call_temp(
                    mir_type_from_type(ty),
                    plan.helper_name.clone(),
                    lowered_args,
                    *span,
                )
            }
            KernelExpr::BatchQuery {
                plan,
                args,
                ty,
                span,
            } => {
                let lowered_args = match args.split_last() {
                    Some((backend, query_args)) => {
                        let mut lowered_args = query_args
                            .iter()
                            .map(|arg| self.lower_kernel_expr(arg))
                            .collect::<Vec<_>>();
                        let backend = self.lower_kernel_expr(backend);
                        lowered_args.push(self.lower_dispatch_backend_id(backend, *span));
                        lowered_args
                    }
                    None => vec![Value::Const(Literal::Integer(Self::dispatch_backend_id(
                        plan.backend,
                    )))],
                };
                self.lower_call_temp(
                    mir_type_from_type(ty),
                    plan.helper_name.clone(),
                    lowered_args,
                    *span,
                )
            }
            KernelExpr::Member {
                base,
                member,
                ty,
                span,
            } => {
                let base_value = self.lower_kernel_expr(base);
                if let Some(component_index) =
                    vector_component_index(mir_type_from_type(base.ty()), member)
                {
                    let temp = self.new_temp(mir_type_from_type(ty));
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Temp(temp),
                        value: Rvalue::Call {
                            kind: CallKind::Sync,
                            target: CallTarget::Function(SmolStr::new("__wr_vec_component")),
                            args: vec![
                                base_value,
                                Value::Const(Literal::Integer(component_index as i64)),
                            ],
                        },
                        span: *span,
                    });
                    return Value::Temp(temp);
                }
                let slot = match base.ty() {
                    Type::Named(name, _) => self.field_slot(name.as_str(), member.as_str()),
                    _ => None,
                };
                let temp = self.new_temp(mir_type_from_type(ty));
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(temp),
                    value: Rvalue::GetField {
                        base: base_value,
                        field: member.clone(),
                        slot,
                    },
                    span: *span,
                });
                Value::Temp(temp)
            }
            KernelExpr::Index {
                base,
                index,
                ty,
                span,
            } => {
                let base_value = self.lower_kernel_expr(base);
                let index_value = self.lower_kernel_expr(index);
                let target_name = match base.ty() {
                    Type::Map(_, _) => "__wr_map_get",
                    _ => "__wr_list_get",
                };
                let temp = self.new_temp(mir_type_from_type(ty));
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(temp),
                    value: Rvalue::Call {
                        kind: CallKind::Sync,
                        target: CallTarget::Function(SmolStr::new(target_name)),
                        args: vec![base_value, index_value],
                    },
                    span: *span,
                });
                Value::Temp(temp)
            }
            KernelExpr::ArrayLiteral { items, span, .. } => {
                let values = items
                    .iter()
                    .map(|item| self.lower_kernel_expr(item))
                    .collect::<Vec<_>>();
                let temp = self.new_temp(MirType::Named(SmolStr::new("List")));
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(temp),
                    value: Rvalue::BuildList {
                        items: values,
                        alloc: AllocKind::LocalTemp,
                    },
                    span: *span,
                });
                Value::Temp(temp)
            }
            KernelExpr::StructLiteral {
                name, fields, span, ..
            } => {
                let mut class = self.synthetic_class_target_info(name.as_str());
                for (field_name, value) in fields {
                    let value = self.lower_kernel_expr(value);
                    Self::set_class_field_value(&mut class, field_name.as_str(), value);
                }
                self.build_class_instance(&class, *span)
            }
        }
    }

    pub(crate) fn lower_stmt(&mut self, body: &hir::Body, stmt_id: hir::Idx<HirStmt>) {
        let span = body.stmt_span(stmt_id);
        match &body.stmts[stmt_id] {
            HirStmt::Expr(expr) => {
                let _ = self.lower_expr(body, *expr);
            }
            HirStmt::Assert {
                kind,
                expr,
                rhs,
                tolerance,
            } => {
                let cond = self.lower_assert_expr(body, *expr, *rhs, *kind, *tolerance);
                let func = SmolStr::new("assert");
                let args = vec![cond, Value::Const(Literal::Nil)];
                let temp = self.new_temp(MirType::Nil);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(temp),
                    value: Rvalue::Call {
                        kind: CallKind::Sync,
                        target: CallTarget::Function(func),
                        args,
                    },
                    span,
                });
            }
            HirStmt::Require { condition, message } => {
                let cond = self.lower_expr(body, *condition);
                let msg = self.lower_expr(body, *message);
                let func = SmolStr::new("assert");
                let args = vec![cond, msg];
                let temp = self.new_temp(MirType::Nil);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(temp),
                    value: Rvalue::Call {
                        kind: CallKind::Sync,
                        target: CallTarget::Function(func),
                        args,
                    },
                    span,
                });
            }
            HirStmt::Let {
                name,
                value,
                mutable,
                ..
            } => {
                let is_result = self.expr_is_result(body, *value);
                let value = self.lower_expr(body, *value);
                let local = self.new_local(name.clone(), *mutable, self.local_type_for_name(name));
                self.declare_local(name.clone(), local);
                self.declare_resultness(name.clone(), is_result);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Local(local),
                    value: Rvalue::Use(value),
                    span,
                });
            }
            HirStmt::Capture { name, value } => {
                let value = self.lower_expr(body, *value);
                let local = self.new_local(name.clone(), false, self.local_type_for_name(name));
                self.declare_local(name.clone(), local);
                self.declare_resultness(name.clone(), true);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Local(local),
                    value: Rvalue::Use(value),
                    span,
                });
            }
            HirStmt::Assign {
                name, op, value, ..
            } => {
                let Some(local) = self.resolve_local(name) else {
                    return;
                };
                let is_result = self.expr_is_result(body, *value);
                let rhs = self.lower_expr(body, *value);
                self.set_resultness(name, is_result);
                match op {
                    AssignOp::Assign => {
                        self.push_stmt(MirStmt::Assign {
                            place: Place::Local(local),
                            value: Rvalue::Use(rhs),
                            span,
                        });
                    }
                    AssignOp::AddAssign
                    | AssignOp::SubAssign
                    | AssignOp::MulAssign
                    | AssignOp::DivAssign => {
                        let op = match op {
                            AssignOp::AddAssign => BinaryOp::Add,
                            AssignOp::SubAssign => BinaryOp::Sub,
                            AssignOp::MulAssign => BinaryOp::Mul,
                            AssignOp::DivAssign => BinaryOp::Div,
                            AssignOp::Assign => BinaryOp::Assign,
                        };
                        let temp = self.new_temp(MirType::Unknown);
                        self.push_stmt(MirStmt::Assign {
                            place: Place::Temp(temp),
                            value: Rvalue::Binary {
                                op,
                                lhs: Value::Local(local),
                                rhs,
                            },
                            span,
                        });
                        self.push_stmt(MirStmt::Assign {
                            place: Place::Local(local),
                            value: Rvalue::Use(Value::Temp(temp)),
                            span,
                        });
                    }
                }
            }
            HirStmt::IgnoreResult { expr } => {
                let _ = self.lower_expr(body, *expr);
            }
            HirStmt::Optimize {
                objective,
                body: optimize_body,
                ..
            } => {
                self.objective_stack.push(*objective);
                self.enter_scope();
                self.lower_stmt_block(body, optimize_body);
                self.exit_scope();
                let popped = self.objective_stack.pop();
                debug_assert_eq!(popped, Some(*objective));
            }
            HirStmt::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let cond = self.lower_expr(body, *condition);
                let then_block = self.new_block();
                let else_block = self.new_block();
                let join_block = self.new_block();
                self.set_terminator(Terminator::Branch {
                    cond,
                    then_target: then_block,
                    else_target: else_block,
                    span,
                });

                self.current_block = then_block;
                self.enter_scope();
                self.lower_stmt_block(body, then_branch);
                self.exit_scope();
                if self.block_is_open(self.current_block) {
                    self.set_terminator(Terminator::Jump {
                        target: join_block,
                        span,
                    });
                }

                self.current_block = else_block;
                if let Some(branch) = else_branch {
                    self.enter_scope();
                    self.lower_stmt_block(body, branch);
                    self.exit_scope();
                }
                if self.block_is_open(self.current_block) {
                    self.set_terminator(Terminator::Jump {
                        target: join_block,
                        span,
                    });
                }

                self.current_block = join_block;
            }
            HirStmt::While {
                condition,
                body: loop_body,
            } => {
                let head_block = self.new_block();
                let body_block = self.new_block();
                let exit_block = self.new_block();

                self.set_terminator(Terminator::Jump {
                    target: head_block,
                    span,
                });

                self.current_block = head_block;
                let cond = self.lower_expr(body, *condition);
                self.set_terminator(Terminator::Branch {
                    cond,
                    then_target: body_block,
                    else_target: exit_block,
                    span,
                });

                self.current_block = body_block;
                self.loop_stack.push(LoopTarget {
                    break_target: exit_block,
                    continue_target: head_block,
                });
                self.enter_scope();
                self.lower_stmt_block(body, loop_body);
                self.exit_scope();
                self.loop_stack.pop();
                if self.block_is_open(self.current_block) {
                    self.set_terminator(Terminator::Jump {
                        target: head_block,
                        span,
                    });
                }

                self.current_block = exit_block;
            }
            HirStmt::For {
                value_name,
                key_name,
                index_name,
                iterable,
                body: loop_body,
            } => {
                if let Expr::Binary {
                    lhs,
                    op: BinaryOp::Range,
                    rhs,
                    ..
                } = &body.exprs[*iterable]
                    && key_name.is_none()
                    && self
                        .lower_range_for(body, value_name, index_name, *lhs, *rhs, loop_body, span)
                {
                    return;
                }

                let iterable_value = self.lower_expr(body, *iterable);
                let iter_temp = self.new_temp(MirType::Unknown);
                self.push_stmt(MirStmt::IterInit {
                    dst: Place::Temp(iter_temp),
                    iterable: iterable_value.clone(),
                    span,
                });
                let iter_count_local = index_name.as_ref().map(|_| {
                    self.new_local(
                        SmolStr::new(format!("$iter_count{}", self.locals.len())),
                        true,
                        MirType::Integer,
                    )
                });
                if let Some(iter_count_local) = iter_count_local {
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Local(iter_count_local),
                        value: Rvalue::Use(Value::Const(Literal::Integer(0))),
                        span,
                    });
                }

                let head_block = self.new_block();
                let body_block = self.new_block();
                let exit_block = self.new_block();

                self.set_terminator(Terminator::Jump {
                    target: head_block,
                    span,
                });

                self.current_block = head_block;
                let value_temp = self.new_temp(MirType::Unknown);
                let done_temp = self.new_temp(MirType::Boolean);
                self.push_stmt(MirStmt::IterNext {
                    iter: Value::Temp(iter_temp),
                    dst_value: Place::Temp(value_temp),
                    dst_done: Place::Temp(done_temp),
                    span,
                });
                self.set_terminator(Terminator::Branch {
                    cond: Value::Temp(done_temp),
                    then_target: exit_block,
                    else_target: body_block,
                    span,
                });

                self.current_block = body_block;
                self.enter_scope();
                if let Some(key_name) = key_name {
                    let key_local = self.new_local(key_name.clone(), false, MirType::Unknown);
                    self.declare_local(key_name.clone(), key_local);
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Local(key_local),
                        value: Rvalue::Use(Value::Temp(value_temp)),
                        span,
                    });
                    let map_get_temp = self.new_temp(MirType::Unknown);
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Temp(map_get_temp),
                        value: Rvalue::Call {
                            kind: CallKind::Sync,
                            target: CallTarget::Function(SmolStr::new("__wr_map_get")),
                            args: vec![iterable_value.clone(), Value::Local(key_local)],
                        },
                        span,
                    });
                    let value_local = self.new_local(value_name.clone(), false, MirType::Unknown);
                    self.declare_local(value_name.clone(), value_local);
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Local(value_local),
                        value: Rvalue::Use(Value::Temp(map_get_temp)),
                        span,
                    });
                } else {
                    let local = self.new_local(value_name.clone(), false, MirType::Unknown);
                    self.declare_local(value_name.clone(), local);
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Local(local),
                        value: Rvalue::Use(Value::Temp(value_temp)),
                        span,
                    });
                }
                if let Some(index_name) = index_name {
                    let index_local = self.new_local(index_name.clone(), false, MirType::Integer);
                    self.declare_local(index_name.clone(), index_local);
                    if let Some(iter_count_local) = iter_count_local {
                        self.push_stmt(MirStmt::Assign {
                            place: Place::Local(index_local),
                            value: Rvalue::Use(Value::Local(iter_count_local)),
                            span,
                        });
                    }
                }
                self.loop_stack.push(LoopTarget {
                    break_target: exit_block,
                    continue_target: head_block,
                });
                self.lower_stmt_block(body, loop_body);
                self.loop_stack.pop();
                self.exit_scope();
                if self.block_is_open(self.current_block) {
                    if let Some(iter_count_local) = iter_count_local {
                        let next_count = self.new_temp(MirType::Integer);
                        self.push_stmt(MirStmt::Assign {
                            place: Place::Temp(next_count),
                            value: Rvalue::Binary {
                                op: BinaryOp::Add,
                                lhs: Value::Local(iter_count_local),
                                rhs: Value::Const(Literal::Integer(1)),
                            },
                            span,
                        });
                        self.push_stmt(MirStmt::Assign {
                            place: Place::Local(iter_count_local),
                            value: Rvalue::Use(Value::Temp(next_count)),
                            span,
                        });
                    }
                    self.set_terminator(Terminator::Jump {
                        target: head_block,
                        span,
                    });
                }

                self.current_block = exit_block;
            }
            HirStmt::Match {
                subject,
                cases,
                otherwise,
            } => {
                let scrutinee = self.lower_expr(body, *subject);
                if self.match_has_result_patterns(cases) {
                    self.lower_result_match(body, span, scrutinee, cases, otherwise);
                } else {
                    let switch_block = self.current_block;
                    let join_block = self.new_block();
                    let default_block = self.new_block();
                    let default_case_idx = cases.iter().position(|case| {
                        case.labels
                            .iter()
                            .any(|label| self.is_default_match_pattern(label))
                    });
                    let mut switch_cases = Vec::new();

                    for (idx, case) in cases.iter().enumerate() {
                        if Some(idx) == default_case_idx {
                            continue;
                        }
                        let case_block = self.new_block();
                        for label in &case.labels {
                            if let Some(case_label) = self.lower_case_label(label) {
                                switch_cases.push((case_label, case_block));
                            }
                        }
                        self.current_block = case_block;
                        self.enter_scope();
                        if let Some(label) = case.labels.first() {
                            self.bind_pattern(body, label, scrutinee.clone(), span);
                        }
                        self.lower_stmt_block(body, &case.body);
                        self.exit_scope();
                        if self.block_is_open(self.current_block) {
                            self.set_terminator(Terminator::Jump {
                                target: join_block,
                                span,
                            });
                        }
                    }

                    self.current_block = default_block;
                    if let Some(idx) = default_case_idx {
                        let case = &cases[idx];
                        self.enter_scope();
                        if let Some(label) = case.labels.first() {
                            self.bind_pattern(body, label, scrutinee.clone(), span);
                        }
                        self.lower_stmt_block(body, &case.body);
                        self.exit_scope();
                    } else if let Some(branch) = otherwise {
                        self.enter_scope();
                        self.lower_stmt_block(body, branch);
                        self.exit_scope();
                    }
                    if self.block_is_open(self.current_block) {
                        self.set_terminator(Terminator::Jump {
                            target: join_block,
                            span,
                        });
                    }

                    self.current_block = switch_block;
                    self.set_terminator(Terminator::Switch {
                        scrutinee,
                        cases: switch_cases,
                        default: default_block,
                        span,
                    });

                    self.current_block = join_block;
                }
            }
            HirStmt::Use { .. } => {}
            HirStmt::Defer { expr } => {
                self.defers.push(*expr);
            }
            HirStmt::Return(expr) => {
                let value = match expr {
                    Some(expr_id) => {
                        let raw_value = self.lower_expr(body, *expr_id);
                        if self.returns_result && !self.expr_is_result(body, *expr_id) {
                            let temp = self.new_temp(MirType::Unknown);
                            self.push_stmt(MirStmt::Assign {
                                place: Place::Temp(temp),
                                value: Rvalue::ResultOk { value: raw_value },
                                span,
                            });
                            Some(Value::Temp(temp))
                        } else {
                            Some(raw_value)
                        }
                    }
                    None => {
                        if self.returns_result {
                            let temp = self.new_temp(MirType::Unknown);
                            self.push_stmt(MirStmt::Assign {
                                place: Place::Temp(temp),
                                value: Rvalue::ResultOk {
                                    value: Value::Const(Literal::Nil),
                                },
                                span,
                            });
                            Some(Value::Temp(temp))
                        } else {
                            None
                        }
                    }
                };
                self.emit_defers(body, span);
                self.set_terminator(Terminator::Return { value, span });
            }
            HirStmt::Break => {
                if let Some(target) = self.loop_stack.last() {
                    self.set_terminator(Terminator::Jump {
                        target: target.break_target,
                        span,
                    });
                }
            }
            HirStmt::Continue => {
                if let Some(target) = self.loop_stack.last() {
                    self.set_terminator(Terminator::Jump {
                        target: target.continue_target,
                        span,
                    });
                }
            }
        }
    }

    pub(crate) fn lower_range_for(
        &mut self,
        body: &hir::Body,
        value_name: &SmolStr,
        index_name: &Option<SmolStr>,
        lhs: hir::Idx<Expr>,
        rhs: hir::Idx<Expr>,
        loop_body: &[hir::Idx<hir::Stmt>],
        span: TextRange,
    ) -> bool {
        if env::var_os("WRELA_DISABLE_TYPED_RANGE_FASTPATH").is_some() {
            return false;
        }
        let lhs_ty = self.expr_type(body, lhs);
        let rhs_ty = self.expr_type(body, rhs);
        let Some(induction_ty) = Self::proven_range_induction_type(&lhs_ty, &rhs_ty) else {
            return false;
        };

        let start_val = self.lower_expr(body, lhs);
        let end_val = self.lower_expr(body, rhs);
        let constant_int_bounds = match (&body.exprs[lhs], &body.exprs[rhs]) {
            (
                Expr::Literal(hir::Literal::Integer(start)),
                Expr::Literal(hir::Literal::Integer(end)),
            ) if matches!(induction_ty, MirType::Integer) => Some((*start, *end)),
            _ => None,
        };

        if let Some((start, end)) = constant_int_bounds {
            let idx_local = self.new_local(
                SmolStr::new(format!("$range_idx{}", self.locals.len())),
                true,
                induction_ty.clone(),
            );
            let loop_var = self.new_local(value_name.clone(), false, induction_ty.clone());
            let loop_index = index_name
                .as_ref()
                .map(|name| self.new_local(name.clone(), false, MirType::Integer));
            let iter_count = self.new_local(
                SmolStr::new(format!("$range_count{}", self.locals.len())),
                true,
                MirType::Integer,
            );
            let head_block = self.new_block();
            let body_block = self.new_block();
            let exit_block = self.new_block();
            self.push_stmt(MirStmt::Assign {
                place: Place::Local(idx_local),
                value: Rvalue::Use(start_val.clone()),
                span,
            });
            self.push_stmt(MirStmt::Assign {
                place: Place::Local(iter_count),
                value: Rvalue::Use(Value::Const(Literal::Integer(0))),
                span,
            });
            self.set_terminator(Terminator::Jump {
                target: head_block,
                span,
            });

            self.current_block = head_block;
            let cond_temp = self.new_temp(MirType::Boolean);
            let cond_op = if start <= end {
                BinaryOp::Le
            } else {
                BinaryOp::Ge
            };
            self.push_stmt(MirStmt::Assign {
                place: Place::Temp(cond_temp),
                value: Rvalue::Binary {
                    op: cond_op,
                    lhs: Value::Local(idx_local),
                    rhs: end_val.clone(),
                },
                span,
            });
            self.set_terminator(Terminator::Branch {
                cond: Value::Temp(cond_temp),
                then_target: body_block,
                else_target: exit_block,
                span,
            });

            self.current_block = body_block;
            self.enter_scope();
            self.declare_local(value_name.clone(), loop_var);
            self.push_stmt(MirStmt::Assign {
                place: Place::Local(loop_var),
                value: Rvalue::Use(Value::Local(idx_local)),
                span,
            });
            if let Some(loop_index) = loop_index {
                if let Some(index_name) = index_name {
                    self.declare_local(index_name.clone(), loop_index);
                }
                self.push_stmt(MirStmt::Assign {
                    place: Place::Local(loop_index),
                    value: Rvalue::Use(Value::Local(iter_count)),
                    span,
                });
            }
            self.loop_stack.push(LoopTarget {
                break_target: exit_block,
                continue_target: head_block,
            });
            self.lower_stmt_block(body, loop_body);
            self.loop_stack.pop();
            self.exit_scope();
            if self.block_is_open(self.current_block) {
                let step_temp = self.new_temp(induction_ty);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(step_temp),
                    value: Rvalue::Binary {
                        op: BinaryOp::Add,
                        lhs: Value::Local(idx_local),
                        rhs: Value::Const(Literal::Integer(if start <= end { 1 } else { -1 })),
                    },
                    span,
                });
                self.push_stmt(MirStmt::Assign {
                    place: Place::Local(idx_local),
                    value: Rvalue::Use(Value::Temp(step_temp)),
                    span,
                });
                let count_temp = self.new_temp(MirType::Integer);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(count_temp),
                    value: Rvalue::Binary {
                        op: BinaryOp::Add,
                        lhs: Value::Local(iter_count),
                        rhs: Value::Const(Literal::Integer(1)),
                    },
                    span,
                });
                self.push_stmt(MirStmt::Assign {
                    place: Place::Local(iter_count),
                    value: Rvalue::Use(Value::Temp(count_temp)),
                    span,
                });
                self.set_terminator(Terminator::Jump {
                    target: head_block,
                    span,
                });
            }

            self.current_block = exit_block;
            return true;
        }

        let idx_local = self.new_local(
            SmolStr::new(format!("$range_idx{}", self.locals.len())),
            true,
            induction_ty.clone(),
        );
        let step_local = self.new_local(
            SmolStr::new(format!("$range_step{}", self.locals.len())),
            true,
            induction_ty.clone(),
        );
        let step_is_pos_local = self.new_local(
            SmolStr::new(format!("$range_pos{}", self.locals.len())),
            true,
            MirType::Boolean,
        );

        let loop_var = self.new_local(value_name.clone(), false, induction_ty.clone());
        let loop_index = index_name
            .as_ref()
            .map(|name| self.new_local(name.clone(), false, MirType::Integer));
        let iter_count = self.new_local(
            SmolStr::new(format!("$range_count{}", self.locals.len())),
            true,
            MirType::Integer,
        );

        let is_pos_temp = self.new_temp(MirType::Boolean);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(is_pos_temp),
            value: Rvalue::Binary {
                op: BinaryOp::Le,
                lhs: start_val.clone(),
                rhs: end_val.clone(),
            },
            span,
        });

        let asc_init = self.new_block();
        let desc_init = self.new_block();
        let head_block = self.new_block();
        let check_pos = self.new_block();
        let check_neg = self.new_block();
        let body_block = self.new_block();
        let exit_block = self.new_block();

        self.set_terminator(Terminator::Branch {
            cond: Value::Temp(is_pos_temp),
            then_target: asc_init,
            else_target: desc_init,
            span,
        });

        let step_value = if matches!(induction_ty, MirType::Float) {
            Value::Const(Literal::Float(1.0))
        } else {
            Value::Const(Literal::Integer(1))
        };
        let neg_step_value = if matches!(induction_ty, MirType::Float) {
            Value::Const(Literal::Float(-1.0))
        } else {
            Value::Const(Literal::Integer(-1))
        };

        self.current_block = asc_init;
        self.push_stmt(MirStmt::Assign {
            place: Place::Local(step_local),
            value: Rvalue::Use(step_value.clone()),
            span,
        });
        self.push_stmt(MirStmt::Assign {
            place: Place::Local(step_is_pos_local),
            value: Rvalue::Use(Value::Const(Literal::Boolean(true))),
            span,
        });
        self.push_stmt(MirStmt::Assign {
            place: Place::Local(idx_local),
            value: Rvalue::Use(start_val.clone()),
            span,
        });
        self.push_stmt(MirStmt::Assign {
            place: Place::Local(iter_count),
            value: Rvalue::Use(Value::Const(Literal::Integer(0))),
            span,
        });
        self.set_terminator(Terminator::Jump {
            target: head_block,
            span,
        });

        self.current_block = desc_init;
        self.push_stmt(MirStmt::Assign {
            place: Place::Local(step_local),
            value: Rvalue::Use(neg_step_value.clone()),
            span,
        });
        self.push_stmt(MirStmt::Assign {
            place: Place::Local(step_is_pos_local),
            value: Rvalue::Use(Value::Const(Literal::Boolean(false))),
            span,
        });
        self.push_stmt(MirStmt::Assign {
            place: Place::Local(idx_local),
            value: Rvalue::Use(start_val.clone()),
            span,
        });
        self.push_stmt(MirStmt::Assign {
            place: Place::Local(iter_count),
            value: Rvalue::Use(Value::Const(Literal::Integer(0))),
            span,
        });
        self.set_terminator(Terminator::Jump {
            target: head_block,
            span,
        });

        self.current_block = head_block;
        self.set_terminator(Terminator::Branch {
            cond: Value::Local(step_is_pos_local),
            then_target: check_pos,
            else_target: check_neg,
            span,
        });

        self.current_block = check_pos;
        let pos_cond = self.new_temp(MirType::Boolean);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(pos_cond),
            value: Rvalue::Binary {
                op: BinaryOp::Le,
                lhs: Value::Local(idx_local),
                rhs: end_val.clone(),
            },
            span,
        });
        self.set_terminator(Terminator::Branch {
            cond: Value::Temp(pos_cond),
            then_target: body_block,
            else_target: exit_block,
            span,
        });

        self.current_block = check_neg;
        let neg_cond = self.new_temp(MirType::Boolean);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(neg_cond),
            value: Rvalue::Binary {
                op: BinaryOp::Ge,
                lhs: Value::Local(idx_local),
                rhs: end_val.clone(),
            },
            span,
        });
        self.set_terminator(Terminator::Branch {
            cond: Value::Temp(neg_cond),
            then_target: body_block,
            else_target: exit_block,
            span,
        });

        self.current_block = body_block;
        self.enter_scope();
        self.declare_local(value_name.clone(), loop_var);
        self.push_stmt(MirStmt::Assign {
            place: Place::Local(loop_var),
            value: Rvalue::Use(Value::Local(idx_local)),
            span,
        });
        if let Some(loop_index) = loop_index {
            if let Some(index_name) = index_name {
                self.declare_local(index_name.clone(), loop_index);
            }
            self.push_stmt(MirStmt::Assign {
                place: Place::Local(loop_index),
                value: Rvalue::Use(Value::Local(iter_count)),
                span,
            });
        }
        self.loop_stack.push(LoopTarget {
            break_target: exit_block,
            continue_target: head_block,
        });
        self.lower_stmt_block(body, loop_body);
        self.loop_stack.pop();
        self.exit_scope();
        if self.block_is_open(self.current_block) {
            let step_temp = self.new_temp(induction_ty);
            self.push_stmt(MirStmt::Assign {
                place: Place::Temp(step_temp),
                value: Rvalue::Binary {
                    op: BinaryOp::Add,
                    lhs: Value::Local(idx_local),
                    rhs: Value::Local(step_local),
                },
                span,
            });
            self.push_stmt(MirStmt::Assign {
                place: Place::Local(idx_local),
                value: Rvalue::Use(Value::Temp(step_temp)),
                span,
            });
            let count_temp = self.new_temp(MirType::Integer);
            self.push_stmt(MirStmt::Assign {
                place: Place::Temp(count_temp),
                value: Rvalue::Binary {
                    op: BinaryOp::Add,
                    lhs: Value::Local(iter_count),
                    rhs: Value::Const(Literal::Integer(1)),
                },
                span,
            });
            self.push_stmt(MirStmt::Assign {
                place: Place::Local(iter_count),
                value: Rvalue::Use(Value::Temp(count_temp)),
                span,
            });
            self.set_terminator(Terminator::Jump {
                target: head_block,
                span,
            });
        }

        self.current_block = exit_block;
        true
    }

    pub(crate) fn lower_assert_expr(
        &mut self,
        body: &hir::Body,
        expr_id: hir::Idx<Expr>,
        rhs_id: Option<hir::Idx<Expr>>,
        kind: hir::AssertKind,
        tolerance: Option<hir::Idx<Expr>>,
    ) -> Value {
        if matches!(kind, hir::AssertKind::Approx) {
            let span = body.expr_span(expr_id);
            let left_id = match &body.exprs[expr_id] {
                Expr::Binary { lhs, .. } => *lhs,
                _ => expr_id,
            };
            let left = self.lower_expr(body, left_id);
            let right = rhs_id
                .map(|rhs| self.lower_expr(body, rhs))
                .unwrap_or(Value::Const(Literal::Nil));
            let tol = tolerance
                .map(|tol| self.lower_expr(body, tol))
                .unwrap_or(Value::Const(Literal::Nil));
            let temp = self.new_temp(MirType::Boolean);
            self.push_stmt(MirStmt::Assign {
                place: Place::Temp(temp),
                value: Rvalue::Call {
                    kind: CallKind::Sync,
                    target: CallTarget::Function(SmolStr::new("approx_eq")),
                    args: vec![left, right, tol],
                },
                span,
            });
            return Value::Temp(temp);
        }
        let span = body.expr_span(expr_id);
        if let Expr::Binary { lhs, op, rhs, .. } = &body.exprs[expr_id]
            && matches!(op, BinaryOp::Eq | BinaryOp::Ne)
        {
            let left = self.lower_expr(body, *lhs);
            let right = self.lower_expr(body, *rhs);
            let func = match kind {
                hir::AssertKind::Value => SmolStr::new("value_deep_eq"),
                hir::AssertKind::Identity => SmolStr::new("identity_eq"),
                hir::AssertKind::Approx => unreachable!(),
            };
            let temp = self.new_temp(MirType::Boolean);
            self.push_stmt(MirStmt::Assign {
                place: Place::Temp(temp),
                value: Rvalue::Call {
                    kind: CallKind::Sync,
                    target: CallTarget::Function(func),
                    args: vec![left, right],
                },
                span,
            });
            let mut result = Value::Temp(temp);
            if matches!(op, BinaryOp::Ne) {
                let not_temp = self.new_temp(MirType::Boolean);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(not_temp),
                    value: Rvalue::Unary {
                        op: UnaryOp::Not,
                        operand: result,
                    },
                    span,
                });
                result = Value::Temp(not_temp);
            }
            return result;
        }
        self.lower_expr(body, expr_id)
    }

    pub(crate) fn lower_case_label(&mut self, pattern: &hir::Pattern) -> Option<SwitchCase> {
        match pattern {
            hir::Pattern::Literal(lit) => Some(SwitchCase::Literal(lit.clone())),
            hir::Pattern::Binding(name) => {
                if let Some(tag) = self.type_tags.get(name).copied() {
                    return Some(SwitchCase::Type(tag));
                }
                if let Some(tag) = builtin_type_tag(name) {
                    return Some(SwitchCase::Type(tag));
                }
                None
            }
            hir::Pattern::Path { parts, args: _ } => {
                if parts.len() == 1 {
                    if let Some(tag) = self.type_tags.get(&parts[0]).copied() {
                        return Some(SwitchCase::Type(tag));
                    }
                    if let Some(tag) = builtin_type_tag(&parts[0]) {
                        return Some(SwitchCase::Type(tag));
                    }
                }
                if parts.len() == 2 {
                    let name = SmolStr::new(format!("{}.{}", parts[0], parts[1]));
                    return self.type_tags.get(&name).copied().map(SwitchCase::Type);
                }
                None
            }
            _ => None,
        }
    }

    pub(crate) fn match_has_result_patterns(&self, cases: &[hir::MatchCase]) -> bool {
        cases.iter().any(|case| {
            case.labels
                .iter()
                .any(|label| self.result_pattern_kind(label).is_some())
        })
    }

    pub(crate) fn result_pattern_kind(&self, pattern: &hir::Pattern) -> Option<bool> {
        if let hir::Pattern::Path { parts, .. } = pattern {
            if parts.len() == 1 && parts[0].as_str() == "Ok" {
                return Some(true);
            }
            if parts.len() == 1 && parts[0].as_str() == "Err" {
                return Some(false);
            }
        }
        None
    }

    pub(crate) fn lower_result_match(
        &mut self,
        body: &hir::Body,
        span: TextRange,
        scrutinee: Value,
        cases: &[hir::MatchCase],
        otherwise: &Option<Vec<hir::Idx<hir::Stmt>>>,
    ) {
        let join_block = self.new_block();
        let mut default_block = None;

        for case in cases {
            let case_block = self.new_block();
            let fallthrough_block = self.new_block();
            let label = case.labels.first();
            if let Some(label) = label {
                if let Some(is_ok) = self.result_pattern_kind(label) {
                    let is_ok_temp = self.new_temp(MirType::Boolean);
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Temp(is_ok_temp),
                        value: Rvalue::ResultIsOk {
                            value: scrutinee.clone(),
                        },
                        span,
                    });
                    let mut cond_val = Value::Temp(is_ok_temp);
                    if !is_ok {
                        let not_temp = self.new_temp(MirType::Boolean);
                        self.push_stmt(MirStmt::Assign {
                            place: Place::Temp(not_temp),
                            value: Rvalue::Unary {
                                op: UnaryOp::Not,
                                operand: cond_val,
                            },
                            span,
                        });
                        cond_val = Value::Temp(not_temp);
                    }
                    self.set_terminator(Terminator::Branch {
                        cond: cond_val,
                        then_target: case_block,
                        else_target: fallthrough_block,
                        span,
                    });
                } else {
                    default_block = Some(case_block);
                    self.set_terminator(Terminator::Jump {
                        target: case_block,
                        span,
                    });
                }
            }

            self.current_block = case_block;
            self.enter_scope();
            if let Some(label) = label {
                self.bind_result_pattern(body, label, scrutinee.clone(), span);
            }
            self.lower_stmt_block(body, &case.body);
            self.exit_scope();
            if self.block_is_open(self.current_block) {
                self.set_terminator(Terminator::Jump {
                    target: join_block,
                    span,
                });
            }

            self.current_block = fallthrough_block;
        }

        if let Some(branch) = otherwise {
            let otherwise_block = self.new_block();
            self.set_terminator(Terminator::Jump {
                target: otherwise_block,
                span,
            });
            self.current_block = otherwise_block;
            self.enter_scope();
            self.lower_stmt_block(body, branch);
            self.exit_scope();
            if self.block_is_open(self.current_block) {
                self.set_terminator(Terminator::Jump {
                    target: join_block,
                    span,
                });
            }
        } else if let Some(default_block) = default_block {
            self.set_terminator(Terminator::Jump {
                target: default_block,
                span,
            });
        }

        self.current_block = join_block;
    }

    pub(crate) fn bind_pattern(
        &mut self,
        body: &hir::Body,
        pattern: &hir::Pattern,
        value: Value,
        span: TextRange,
    ) {
        match pattern {
            hir::Pattern::Wildcard | hir::Pattern::Literal(_) => {}
            hir::Pattern::Binding(name) => {
                let local = self.new_local(name.clone(), false, MirType::Unknown);
                self.declare_local(name.clone(), local);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Local(local),
                    value: Rvalue::Use(value),
                    span,
                });
            }
            hir::Pattern::Path { parts, args } => {
                if parts.len() == 1 {
                    let class_name = parts[0].clone();
                    if let Some(fields) = self.class_fields.get(&class_name).cloned() {
                        for (idx, arg) in args.iter().enumerate() {
                            if let Some(field) = fields.get(idx) {
                                let temp = self.new_temp(MirType::Unknown);
                                self.push_stmt(MirStmt::Assign {
                                    place: Place::Temp(temp),
                                    value: Rvalue::GetField {
                                        base: value.clone(),
                                        field: field.clone(),
                                        slot: Some(idx as u32),
                                    },
                                    span,
                                });
                                self.bind_pattern(body, arg, Value::Temp(temp), span);
                            }
                        }
                        return;
                    }
                    if let Some(arg) = args.first() {
                        self.bind_pattern(body, arg, value, span);
                    }
                    return;
                }
                if parts.len() == 2 {
                    let class_name = SmolStr::new(format!("{}.{}", parts[0], parts[1]));
                    let Some(fields) = self.class_fields.get(&class_name).cloned() else {
                        return;
                    };
                    for (idx, arg) in args.iter().enumerate() {
                        if let Some(field) = fields.get(idx) {
                            let temp = self.new_temp(MirType::Unknown);
                            self.push_stmt(MirStmt::Assign {
                                place: Place::Temp(temp),
                                value: Rvalue::GetField {
                                    base: value.clone(),
                                    field: field.clone(),
                                    slot: Some(idx as u32),
                                },
                                span,
                            });
                            self.bind_pattern(body, arg, Value::Temp(temp), span);
                        }
                    }
                }
            }
            hir::Pattern::Struct {
                parts,
                fields: pattern_fields,
            } => {
                let class_name = if parts.len() == 1 {
                    Some(parts[0].clone())
                } else if parts.len() == 2 {
                    Some(SmolStr::new(format!("{}.{}", parts[0], parts[1])))
                } else {
                    None
                };
                let Some(class_name) = class_name else {
                    return;
                };
                let Some(fields) = self.class_fields.get(&class_name).cloned() else {
                    return;
                };
                for (field_name, field_pattern) in pattern_fields {
                    let Some(idx) = fields.iter().position(|f| f == field_name) else {
                        continue;
                    };
                    let temp = self.new_temp(MirType::Unknown);
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Temp(temp),
                        value: Rvalue::GetField {
                            base: value.clone(),
                            field: field_name.clone(),
                            slot: Some(idx as u32),
                        },
                        span,
                    });
                    self.bind_pattern(body, field_pattern, Value::Temp(temp), span);
                }
            }
        }
    }

    pub(crate) fn bind_result_pattern(
        &mut self,
        body: &hir::Body,
        pattern: &hir::Pattern,
        value: Value,
        span: TextRange,
    ) {
        let Some(kind) = self.result_pattern_kind(pattern) else {
            return;
        };
        if let hir::Pattern::Path { args, .. } = pattern {
            if args.is_empty() {
                return;
            }
            let temp = self.new_temp(MirType::Unknown);
            let rvalue = if kind {
                Rvalue::ResultUnwrap { value }
            } else {
                Rvalue::ResultErrUnwrap { value }
            };
            self.push_stmt(MirStmt::Assign {
                place: Place::Temp(temp),
                value: rvalue,
                span,
            });
            self.bind_pattern(body, &args[0], Value::Temp(temp), span);
        }
    }

    pub(crate) fn lower_expr(&mut self, body: &hir::Body, expr_id: hir::Idx<Expr>) -> Value {
        let span = body.expr_span(expr_id);
        match &body.exprs[expr_id] {
            Expr::Literal(lit) => Value::Const(lit.clone()),
            Expr::Variable(name) => self
                .resolve_local(name)
                .map(Value::Local)
                .unwrap_or_else(|| Value::Const(Literal::Nil)),
            Expr::Detach {
                target,
                size,
                objective,
            } => self.lower_detach_expr(body, *target, *size, *objective, expr_id, span),
            Expr::Unary { op, expr, .. } => {
                if matches!(op, UnaryOp::Await) {
                    self.suspendable = true;
                }
                match op {
                    UnaryOp::Await => self.lower_await(body, *expr, span),
                    UnaryOp::Try => {
                        let result_val = self.lower_expr(body, *expr);
                        let ok_flag = self.new_temp(MirType::Boolean);
                        self.push_stmt(MirStmt::Assign {
                            place: Place::Temp(ok_flag),
                            value: Rvalue::ResultIsOk {
                                value: result_val.clone(),
                            },
                            span,
                        });

                        let then_block = self.new_block();
                        let else_block = self.new_block();
                        let join_block = self.new_block();
                        let result_local = self.new_temp_local();
                        self.push_stmt(MirStmt::Assign {
                            place: Place::Local(result_local),
                            value: Rvalue::Use(Value::Const(Literal::Nil)),
                            span,
                        });
                        self.set_terminator(Terminator::Branch {
                            cond: Value::Temp(ok_flag),
                            then_target: then_block,
                            else_target: else_block,
                            span,
                        });

                        self.current_block = then_block;
                        let ok_value = self.new_temp(MirType::Unknown);
                        self.push_stmt(MirStmt::Assign {
                            place: Place::Temp(ok_value),
                            value: Rvalue::ResultUnwrap {
                                value: result_val.clone(),
                            },
                            span,
                        });
                        self.push_stmt(MirStmt::Assign {
                            place: Place::Local(result_local),
                            value: Rvalue::Use(Value::Temp(ok_value)),
                            span,
                        });
                        if self.block_is_open(self.current_block) {
                            self.set_terminator(Terminator::Jump {
                                target: join_block,
                                span,
                            });
                        }

                        self.current_block = else_block;
                        self.emit_defers(body, span);
                        self.set_terminator(Terminator::Return {
                            value: Some(result_val),
                            span,
                        });

                        self.current_block = join_block;
                        Value::Local(result_local)
                    }
                    UnaryOp::Fire => {
                        if let Expr::Call { callee, args, .. } = &body.exprs[*expr] {
                            if self.is_actor_call(body, *callee) {
                                let (target, arg_values) =
                                    self.lower_call_target(body, *callee, args);
                                self.push_stmt(MirStmt::ActorFire {
                                    target,
                                    args: arg_values,
                                    span,
                                });
                                return Value::Const(Literal::Nil);
                            }
                        }
                        let pending = self.lower_pending_call_or_value(body, *expr, span);
                        self.push_stmt(MirStmt::Fire { pending, span });
                        Value::Const(Literal::Nil)
                    }
                    UnaryOp::Spawn => self.lower_detach_expr(
                        body,
                        *expr,
                        hir::PoolSize::Fixed(1),
                        None,
                        expr_id,
                        span,
                    ),
                    UnaryOp::Err => {
                        let operand = self.lower_expr(body, *expr);
                        let temp = self.new_temp_for_expr(body, expr_id);
                        self.push_stmt(MirStmt::Assign {
                            place: Place::Temp(temp),
                            value: Rvalue::ResultErr { value: operand },
                            span,
                        });
                        Value::Temp(temp)
                    }
                    _ => {
                        let operand = self.lower_expr(body, *expr);
                        let temp = self.new_temp_for_expr(body, expr_id);
                        self.push_stmt(MirStmt::Assign {
                            place: Place::Temp(temp),
                            value: Rvalue::Unary { op: *op, operand },
                            span,
                        });
                        Value::Temp(temp)
                    }
                }
            }
            Expr::TypeApply { callee, .. } => self.lower_expr(body, *callee),
            Expr::Binary { lhs, op, rhs, .. } => {
                if matches!(
                    op,
                    BinaryOp::Assign
                        | BinaryOp::AddAssign
                        | BinaryOp::SubAssign
                        | BinaryOp::MulAssign
                        | BinaryOp::DivAssign
                ) && let Expr::Index { object, index, .. } = &body.exprs[*lhs]
                {
                    let object_value = self.lower_expr(body, *object);
                    let index_value = self.lower_expr(body, *index);
                    let rhs_val = self.lower_expr(body, *rhs);
                    let set_name = match self.expr_type(body, *object) {
                        MirType::Named(name) if name.as_str() == "Map" => "__wr_map_set",
                        _ => "__wr_list_set",
                    };
                    let (new_val, args) = if *op == BinaryOp::Assign {
                        (
                            rhs_val.clone(),
                            vec![object_value, index_value, rhs_val.clone()],
                        )
                    } else {
                        let get_name = match self.expr_type(body, *object) {
                            MirType::Named(name) if name.as_str() == "Map" => "__wr_map_get",
                            _ => "__wr_list_get",
                        };
                        let current = self.new_temp(MirType::Unknown);
                        self.push_stmt(MirStmt::Assign {
                            place: Place::Temp(current),
                            value: Rvalue::Call {
                                kind: CallKind::Sync,
                                target: CallTarget::Function(SmolStr::new(get_name)),
                                args: vec![object_value.clone(), index_value.clone()],
                            },
                            span,
                        });
                        let bin_op = match op {
                            BinaryOp::AddAssign => BinaryOp::Add,
                            BinaryOp::SubAssign => BinaryOp::Sub,
                            BinaryOp::MulAssign => BinaryOp::Mul,
                            BinaryOp::DivAssign => BinaryOp::Div,
                            _ => BinaryOp::Assign,
                        };
                        let temp = self.new_temp(MirType::Unknown);
                        self.push_stmt(MirStmt::Assign {
                            place: Place::Temp(temp),
                            value: Rvalue::Binary {
                                op: bin_op,
                                lhs: Value::Temp(current),
                                rhs: rhs_val,
                            },
                            span,
                        });
                        let new_val = Value::Temp(temp);
                        (
                            new_val.clone(),
                            vec![object_value, index_value, new_val.clone()],
                        )
                    };
                    let ignored = self.new_temp(MirType::Unknown);
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Temp(ignored),
                        value: Rvalue::Call {
                            kind: CallKind::Sync,
                            target: CallTarget::Function(SmolStr::new(set_name)),
                            args,
                        },
                        span,
                    });
                    return new_val;
                } else if matches!(
                    op,
                    BinaryOp::Assign
                        | BinaryOp::AddAssign
                        | BinaryOp::SubAssign
                        | BinaryOp::MulAssign
                        | BinaryOp::DivAssign
                ) && let Expr::Member { object, member, .. } = &body.exprs[*lhs]
                {
                    let slot = self.member_slot_hint(body, *object, member);
                    let base = self.lower_expr(body, *object);
                    let rhs_val = self.lower_expr(body, *rhs);
                    let new_val = if *op == BinaryOp::Assign {
                        rhs_val.clone()
                    } else {
                        let current = self.new_temp(MirType::Unknown);
                        self.push_stmt(MirStmt::Assign {
                            place: Place::Temp(current),
                            value: Rvalue::GetField {
                                base: base.clone(),
                                field: member.clone(),
                                slot,
                            },
                            span,
                        });
                        let bin_op = match op {
                            BinaryOp::AddAssign => BinaryOp::Add,
                            BinaryOp::SubAssign => BinaryOp::Sub,
                            BinaryOp::MulAssign => BinaryOp::Mul,
                            BinaryOp::DivAssign => BinaryOp::Div,
                            _ => BinaryOp::Assign,
                        };
                        let temp = self.new_temp(MirType::Unknown);
                        self.push_stmt(MirStmt::Assign {
                            place: Place::Temp(temp),
                            value: Rvalue::Binary {
                                op: bin_op,
                                lhs: Value::Temp(current),
                                rhs: rhs_val,
                            },
                            span,
                        });
                        Value::Temp(temp)
                    };
                    self.push_stmt(MirStmt::SetField {
                        base,
                        field: member.clone(),
                        slot,
                        value: new_val.clone(),
                        span,
                    });
                    return new_val;
                }
                if matches!(op, BinaryOp::Otherwise) {
                    let result_val = self.lower_expr(body, *lhs);
                    let ok_flag = self.new_temp(MirType::Boolean);
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Temp(ok_flag),
                        value: Rvalue::ResultIsOk {
                            value: result_val.clone(),
                        },
                        span,
                    });

                    let then_block = self.new_block();
                    let else_block = self.new_block();
                    let join_block = self.new_block();
                    let result_local = self.new_temp_local();
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Local(result_local),
                        value: Rvalue::Use(Value::Const(Literal::Nil)),
                        span,
                    });
                    self.set_terminator(Terminator::Branch {
                        cond: Value::Temp(ok_flag),
                        then_target: then_block,
                        else_target: else_block,
                        span,
                    });

                    self.current_block = then_block;
                    let ok_value = self.new_temp(MirType::Unknown);
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Temp(ok_value),
                        value: Rvalue::ResultUnwrap { value: result_val },
                        span,
                    });
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Local(result_local),
                        value: Rvalue::Use(Value::Temp(ok_value)),
                        span,
                    });
                    if self.block_is_open(self.current_block) {
                        self.set_terminator(Terminator::Jump {
                            target: join_block,
                            span,
                        });
                    }

                    self.current_block = else_block;
                    let handler_value = self.lower_expr(body, *rhs);
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Local(result_local),
                        value: Rvalue::Use(handler_value),
                        span,
                    });
                    if self.block_is_open(self.current_block) {
                        self.set_terminator(Terminator::Jump {
                            target: join_block,
                            span,
                        });
                    }

                    self.current_block = join_block;
                    Value::Local(result_local)
                } else if matches!(op, BinaryOp::And | BinaryOp::Or) {
                    let lhs_val = self.lower_expr(body, *lhs);
                    let eval_block = self.new_block();
                    let short_block = self.new_block();
                    let join_block = self.new_block();

                    let (then_target, else_target) = if matches!(op, BinaryOp::And) {
                        (eval_block, short_block)
                    } else {
                        (short_block, eval_block)
                    };
                    let result_local = self.new_temp_local();
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Local(result_local),
                        value: Rvalue::Use(Value::Const(Literal::Nil)),
                        span,
                    });

                    self.set_terminator(Terminator::Branch {
                        cond: lhs_val.clone(),
                        then_target,
                        else_target,
                        span,
                    });

                    self.current_block = short_block;
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Local(result_local),
                        value: Rvalue::Use(lhs_val),
                        span,
                    });
                    if self.block_is_open(self.current_block) {
                        self.set_terminator(Terminator::Jump {
                            target: join_block,
                            span,
                        });
                    }

                    self.current_block = eval_block;
                    let rhs_val = self.lower_expr(body, *rhs);
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Local(result_local),
                        value: Rvalue::Use(rhs_val),
                        span,
                    });
                    if self.block_is_open(self.current_block) {
                        self.set_terminator(Terminator::Jump {
                            target: join_block,
                            span,
                        });
                    }

                    self.current_block = join_block;
                    Value::Local(result_local)
                } else {
                    if matches!(op, BinaryOp::Add)
                        && (is_syntactic_stringish(body, *lhs)
                            || is_syntactic_stringish(body, *rhs))
                    {
                        let lhs = self.lower_expr(body, *lhs);
                        let rhs = self.lower_expr(body, *rhs);
                        let temp = self.new_temp_for_expr(body, expr_id);
                        self.push_stmt(MirStmt::Assign {
                            place: Place::Temp(temp),
                            value: Rvalue::StrConcat {
                                parts: vec![lhs, rhs],
                                // Escape analysis will refine this to LocalTemp when possible.
                                alloc: AllocKind::Escaping,
                            },
                            span,
                        });
                        return Value::Temp(temp);
                    }
                    let lhs = self.lower_expr(body, *lhs);
                    let rhs = self.lower_expr(body, *rhs);
                    let temp = self.new_temp_for_expr(body, expr_id);
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Temp(temp),
                        value: Rvalue::Binary { op: *op, lhs, rhs },
                        span,
                    });
                    Value::Temp(temp)
                }
            }
            Expr::Crash { expr } => {
                let value = self.lower_expr(body, *expr);
                let temp = self.new_temp_for_expr(body, expr_id);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(temp),
                    value: Rvalue::Crash { value },
                    span,
                });
                Value::Temp(temp)
            }
            Expr::Member { object, member, .. } => {
                if let Some((class_name, class_id)) = self.resolve_class_init_target(body, expr_id)
                {
                    let fields = self
                        .class_fields
                        .get(&class_name)
                        .cloned()
                        .unwrap_or_default();
                    if fields.is_empty() {
                        let temp = self.new_temp_for_expr(body, expr_id);
                        self.push_stmt(MirStmt::Assign {
                            place: Place::Temp(temp),
                            value: Rvalue::ClassInit {
                                class_id: class_id.0 as u32,
                                fields,
                            },
                            span,
                        });
                        self.maybe_call_configure(&class_name, Value::Temp(temp), span);
                        return Value::Temp(temp);
                    }
                }
                if let MirType::Named(class_name) = self.expr_type(body, *object)
                    && class_name.as_str() == "SceneDomain"
                    && let Some((contract_name, contract_field, nested_field, nested_ty)) =
                        scene_domain_compat_member(member.as_str())
                {
                    let base = self.lower_expr(body, *object);
                    let contract = self.lower_get_named_field(
                        base,
                        "SceneDomain",
                        contract_field,
                        MirType::Named(SmolStr::new(contract_name)),
                        span,
                    );
                    return self.lower_get_named_field(
                        contract,
                        contract_name,
                        nested_field,
                        nested_ty,
                        span,
                    );
                }
                if let Some(component_index) =
                    vector_component_index(self.expr_type(body, *object), member)
                {
                    let base = self.lower_expr(body, *object);
                    let temp = self.new_temp_for_expr(body, expr_id);
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Temp(temp),
                        value: Rvalue::Call {
                            kind: CallKind::Sync,
                            target: CallTarget::Function(SmolStr::new("__wr_vec_component")),
                            args: vec![
                                base,
                                Value::Const(Literal::Integer(component_index as i64)),
                            ],
                        },
                        span,
                    });
                    return Value::Temp(temp);
                }
                let base = self.lower_expr(body, *object);
                let slot = self.member_slot_hint(body, *object, member);
                let temp = self.new_temp_for_expr(body, expr_id);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(temp),
                    value: Rvalue::GetField {
                        base,
                        field: member.clone(),
                        slot,
                    },
                    span,
                });
                Value::Temp(temp)
            }
            Expr::Index { object, index, .. } => {
                let object_value = self.lower_expr(body, *object);
                let index_value = self.lower_expr(body, *index);
                let target_name = match self.expr_type(body, *object) {
                    MirType::Named(name) if name.as_str() == "Map" => "__wr_map_get",
                    _ => "__wr_list_get",
                };
                let temp = self.new_temp_for_expr(body, expr_id);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(temp),
                    value: Rvalue::Call {
                        kind: CallKind::Sync,
                        target: CallTarget::Function(SmolStr::new(target_name)),
                        args: vec![object_value, index_value],
                    },
                    span,
                });
                Value::Temp(temp)
            }
            Expr::Call { callee, args, .. } => {
                if let Some(target) = self.parse_capture_builtin(body, expr_id) {
                    return self.build_scene_capture_value(&target, span);
                }
                if let Some(mode) = self.parse_dispatch_backend_builtin(body, expr_id) {
                    return self.build_dispatch_backend_value(mode, span);
                }
                if let Some(spec) = self.parse_field_query(body, expr_id) {
                    return self.lower_field_query_call(body, span, &spec);
                }
                if let Some(spec) = self.parse_shape_query(body, expr_id) {
                    return self.lower_shape_query_call(body, span, &spec);
                }
                if let Some(spec) = self.parse_world_point_query(body, expr_id) {
                    return self.lower_world_point_query_call(body, span, &spec);
                }
                if let Some(spec) = self.parse_world_shape_query(body, expr_id) {
                    return self.lower_world_shape_query_call(body, span, &spec);
                }
                if let Some(spec) = self.parse_field_batch_query(body, expr_id) {
                    return self.lower_field_batch_query_call(body, span, &spec);
                }
                if let Some(spec) = self.parse_shape_batch_query(body, expr_id) {
                    return self.lower_shape_batch_query_call(body, span, &spec);
                }
                if let Some(spec) = parse_kernel_dispatch_compute(body, expr_id) {
                    return self.lower_dispatch_compute_call(body, span, &spec);
                }
                if let Some((class_name, class_id)) = self.resolve_class_init_target(body, *callee)
                {
                    let fields = self
                        .class_fields
                        .get(&class_name)
                        .cloned()
                        .unwrap_or_default();
                    let field_defaults = self
                        .class_field_defaults
                        .get(&class_name)
                        .cloned()
                        .unwrap_or_else(|| vec![None; fields.len()]);
                    let mut field_values: Vec<Option<Value>> = vec![None; fields.len()];
                    let mut positional_index = 0usize;
                    for arg in args {
                        match arg {
                            hir::Arg::Positional { value, .. } => {
                                let lowered = self.lower_expr(body, *value);
                                if positional_index < field_values.len() {
                                    field_values[positional_index] = Some(lowered);
                                }
                                positional_index += 1;
                            }
                            hir::Arg::Named { name, value, .. } => {
                                let lowered = self.lower_expr(body, *value);
                                if let Some(idx) = fields.iter().position(|f| f == name) {
                                    field_values[idx] = Some(lowered);
                                }
                            }
                        }
                    }
                    let temp = self.new_temp_for_expr(body, expr_id);
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Temp(temp),
                        value: Rvalue::ClassInit {
                            class_id: class_id.0 as u32,
                            fields,
                        },
                        span,
                    });
                    for idx in 0..field_values.len() {
                        if let Some(value) = field_values[idx].clone() {
                            self.push_stmt(MirStmt::SetField {
                                base: Value::Temp(temp),
                                field: self
                                    .class_fields
                                    .get(&class_name)
                                    .and_then(|fields| fields.get(idx).cloned())
                                    .unwrap_or_default(),
                                slot: Some(idx as u32),
                                value,
                                span,
                            });
                        }
                    }
                    for (idx, default) in field_defaults.iter().enumerate() {
                        if field_values.get(idx).and_then(|val| val.as_ref()).is_none()
                            && let Some(default) = default
                        {
                            let value = self.lower_field_default(default, span);
                            self.push_stmt(MirStmt::SetField {
                                base: Value::Temp(temp),
                                field: self
                                    .class_fields
                                    .get(&class_name)
                                    .and_then(|fields| fields.get(idx).cloned())
                                    .unwrap_or_default(),
                                slot: Some(idx as u32),
                                value,
                                span,
                            });
                        }
                    }
                    self.maybe_call_configure(&class_name, Value::Temp(temp), span);
                    return Value::Temp(temp);
                }
                let (target, args) = self.lower_call_target(body, *callee, args);
                let temp = self.new_temp_for_expr(body, expr_id);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(temp),
                    value: Rvalue::Call {
                        kind: CallKind::Sync,
                        target,
                        args,
                    },
                    span,
                });
                Value::Temp(temp)
            }
            Expr::List(items) => {
                let mut values = Vec::with_capacity(items.len());
                for item in items {
                    values.push(self.lower_expr(body, *item));
                }
                let temp = self.new_temp_for_expr(body, expr_id);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(temp),
                    value: Rvalue::BuildList {
                        items: values,
                        alloc: crate::mir::ir::AllocKind::LocalTemp,
                    },
                    span,
                });
                Value::Temp(temp)
            }
            Expr::Map(items) => {
                let mut values = Vec::with_capacity(items.len());
                for (key, value) in items {
                    let key_val = self.lower_expr(body, *key);
                    let value_val = self.lower_expr(body, *value);
                    values.push((key_val, value_val));
                }
                let temp = self.new_temp_for_expr(body, expr_id);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(temp),
                    value: Rvalue::BuildMap {
                        items: values,
                        alloc: crate::mir::ir::AllocKind::LocalTemp,
                    },
                    span,
                });
                Value::Temp(temp)
            }
            Expr::StringInterp(parts) => {
                let mut values = Vec::with_capacity(parts.len());
                for part in parts {
                    match part {
                        hir::StringPart::Literal(value) => {
                            values.push(StringPartValue::Literal(value.clone()));
                        }
                        hir::StringPart::Expr(expr) => {
                            let value = self.lower_expr(body, *expr);
                            values.push(StringPartValue::Value(value));
                        }
                    }
                }
                let temp = self.new_temp_for_expr(body, expr_id);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(temp),
                    value: Rvalue::StringInterp {
                        parts: values,
                        alloc: crate::mir::ir::AllocKind::LocalTemp,
                    },
                    span,
                });
                Value::Temp(temp)
            }
            Expr::Closure {
                body: closure_body, ..
            } => {
                // Lower the closure body expression; closures are not yet first-class
                // in the MIR, so we simply lower the body expression inline.
                self.lower_expr(body, *closure_body)
            }
        }
    }

    pub(crate) fn emit_defers(&mut self, body: &hir::Body, _span: TextRange) {
        let defers = self.defers.clone();
        for expr_id in defers.iter().rev() {
            let _ = self.lower_expr(body, *expr_id);
        }
    }

    pub(crate) fn lower_detach_expr(
        &mut self,
        body: &hir::Body,
        target_expr: hir::Idx<Expr>,
        size: hir::PoolSize,
        objective: Option<hir::Objective>,
        result_expr: hir::Idx<Expr>,
        span: TextRange,
    ) -> Value {
        let mut target_expr = target_expr;
        let mut size = size;
        // If the detach site didn't specify an objective, inherit it from the nearest
        // surrounding `optimize <objective>:` block.
        let mut objective = objective.or_else(|| self.current_objective());
        let mut config = SpawnConfig::default();
        let mut min_size = None;
        let mut max_size = None;
        let mut weight = None;
        let mut queue_cap: Option<i64> = None;
        if let Some(spec) = self.parse_pool_of(body, target_expr) {
            target_expr = spec.class_expr;
            if let Some(__wr_pool_size) = spec.size {
                size = __wr_pool_size;
            }
            if let Some(pool_objective) = spec.objective {
                objective = Some(pool_objective);
            }
            config = spec.config;
            min_size = spec.min_size;
            max_size = spec.max_size;
            weight = spec.weight;
            queue_cap = spec.queue_cap;
        }
        match size {
            hir::PoolSize::Fixed(count) => {
                if count > 1
                    && let Some(value) = self.lower_detach_pool_fixed(
                        body,
                        target_expr,
                        count as usize,
                        objective,
                        config,
                        min_size,
                        max_size,
                        weight,
                        queue_cap,
                        result_expr,
                        span,
                    )
                {
                    return value;
                }
            }
            hir::PoolSize::Auto => {
                if let Some(value) = self.lower_detach_pool_auto(
                    body,
                    target_expr,
                    objective,
                    config,
                    min_size,
                    max_size,
                    weight,
                    queue_cap,
                    result_expr,
                    span,
                ) {
                    return value;
                }
            }
        }
        let mut target = None;
        let mut instance = None;
        let mut lowered = None;
        match &body.exprs[target_expr] {
            Expr::Variable(name) => {
                if let Some(id) = self.type_tags.get(name).copied() {
                    target = Some(Value::Const(Literal::Integer(id.0 as i64)));
                    let fields = self.class_fields.get(name).cloned().unwrap_or_default();
                    let field_defaults = self
                        .class_field_defaults
                        .get(name)
                        .cloned()
                        .unwrap_or_else(|| vec![None; fields.len()]);
                    let temp = self.new_temp_for_expr(body, target_expr);
                    self.push_stmt(MirStmt::Assign {
                        place: Place::Temp(temp),
                        value: Rvalue::ClassInit {
                            class_id: id.0 as u32,
                            fields,
                        },
                        span,
                    });
                    for (idx, default) in field_defaults.iter().enumerate() {
                        if let Some(default) = default {
                            let value = self.lower_field_default(default, span);
                            self.push_stmt(MirStmt::SetField {
                                base: Value::Temp(temp),
                                field: self
                                    .class_fields
                                    .get(name)
                                    .and_then(|fields| fields.get(idx).cloned())
                                    .unwrap_or_default(),
                                slot: Some(idx as u32),
                                value,
                                span,
                            });
                        }
                    }
                    self.maybe_call_configure(name, Value::Temp(temp), span);
                    instance = Some(Value::Temp(temp));
                }
            }
            Expr::Call { callee, .. } => {
                let mut handled = false;
                if let Expr::Variable(name) = &body.exprs[*callee]
                    && let Some(id) = self.type_tags.get(name).copied()
                {
                    target = Some(Value::Const(Literal::Integer(id.0 as i64)));
                    // `detach` on actor classes should always have a concrete instance.
                    // Some actor-class "constructor" call shapes don't lower to a normal
                    // `ClassInit` expression here, so build the instance explicitly from
                    // class metadata (same strategy as Pool.of fast paths).
                    if let Some(class) = self.class_target_info(body, *callee) {
                        let value = self.build_class_instance(&class, span);
                        instance = Some(value.clone());
                        lowered = Some(value);
                        handled = true;
                    }
                }
                if !handled {
                    let value = self.lower_expr(body, target_expr);
                    instance = Some(value.clone());
                    lowered = Some(value);
                }
            }
            _ => {
                let value = self.lower_expr(body, target_expr);
                lowered = Some(value.clone());
                instance = Some(value);
            }
        }
        let target = target.unwrap_or_else(|| {
            lowered
                .clone()
                .unwrap_or_else(|| self.lower_expr(body, target_expr))
        });
        let instance = instance.unwrap_or(Value::Const(Literal::Nil));
        let objective = objective.unwrap_or(hir::Objective::Balance);
        let temp = self.new_temp_for_expr(body, result_expr);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(temp),
            value: Rvalue::Spawn {
                target,
                instance,
                size,
                objective,
                config,
            },
            span,
        });
        Value::Temp(temp)
    }

    pub(crate) fn lower_detach_pool_fixed(
        &mut self,
        body: &hir::Body,
        target_expr: hir::Idx<Expr>,
        count: usize,
        objective: Option<hir::Objective>,
        config: SpawnConfig,
        min_size: Option<i64>,
        max_size: Option<i64>,
        weight: Option<i64>,
        queue_cap: Option<i64>,
        result_expr: hir::Idx<Expr>,
        span: TextRange,
    ) -> Option<Value> {
        let class = self.class_target_info(body, target_expr)?;
        let objective = objective
            .or_else(|| self.current_objective())
            .unwrap_or(hir::Objective::Balance);
        let mut handles = Vec::with_capacity(count);
        for _ in 0..count {
            let instance = self.build_class_instance(&class, span);
            let target = Value::Const(Literal::Integer(class.class_id.0 as i64));
            let temp = self.new_temp_for_expr(body, result_expr);
            self.push_stmt(MirStmt::Assign {
                place: Place::Temp(temp),
                value: Rvalue::Spawn {
                    target,
                    instance,
                    size: hir::PoolSize::Fixed(1),
                    objective,
                    config,
                },
                span,
            });
            handles.push(Value::Temp(temp));
        }
        let list_temp = self.new_temp_for_expr(body, result_expr);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(list_temp),
            value: Rvalue::BuildList {
                items: handles,
                alloc: crate::mir::ir::AllocKind::LocalTemp,
            },
            span,
        });
        let pool_temp = self.new_temp_for_expr(body, result_expr);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(pool_temp),
            value: Rvalue::PoolNew {
                handles: Value::Temp(list_temp),
                objective,
                min_size: min_size.unwrap_or(0),
                max_size: max_size.unwrap_or(0),
                weight: weight.unwrap_or(0),
                queue_cap: queue_cap.unwrap_or(0),
            },
            span,
        });
        Some(Value::Temp(pool_temp))
    }

    pub(crate) fn lower_detach_pool_auto(
        &mut self,
        body: &hir::Body,
        target_expr: hir::Idx<Expr>,
        objective: Option<hir::Objective>,
        config: SpawnConfig,
        min_size: Option<i64>,
        max_size: Option<i64>,
        weight: Option<i64>,
        queue_cap: Option<i64>,
        result_expr: hir::Idx<Expr>,
        span: TextRange,
    ) -> Option<Value> {
        let class = self.class_target_info(body, target_expr)?;
        let objective = objective
            .or_else(|| self.current_objective())
            .unwrap_or(hir::Objective::Balance);
        let obj_code = objective_code(objective);
        let resolved_size = compile_time_auto_pool_size(
            obj_code,
            min_size.unwrap_or(0),
            max_size.unwrap_or(0),
            weight.unwrap_or(0),
        );

        let size_temp = self.new_temp(MirType::Integer);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(size_temp),
            value: Rvalue::Use(Value::Const(Literal::Integer(resolved_size))),
            span,
        });

        let list_temp = self.new_temp(MirType::Unknown);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(list_temp),
            value: Rvalue::BuildList {
                items: Vec::new(),
                alloc: crate::mir::ir::AllocKind::LocalTemp,
            },
            span,
        });

        let idx_local = self.new_temp_local();
        self.push_stmt(MirStmt::Assign {
            place: Place::Local(idx_local),
            value: Rvalue::Use(Value::Const(Literal::Integer(0))),
            span,
        });

        let head_block = self.new_block();
        let body_block = self.new_block();
        let exit_block = self.new_block();

        self.set_terminator(Terminator::Jump {
            target: head_block,
            span,
        });

        self.current_block = head_block;
        let cond_temp = self.new_temp(MirType::Boolean);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(cond_temp),
            value: Rvalue::Binary {
                op: BinaryOp::Lt,
                lhs: Value::Local(idx_local),
                rhs: Value::Temp(size_temp),
            },
            span,
        });
        self.set_terminator(Terminator::Branch {
            cond: Value::Temp(cond_temp),
            then_target: body_block,
            else_target: exit_block,
            span,
        });

        self.current_block = body_block;
        let instance = self.build_class_instance(&class, span);
        let target = Value::Const(Literal::Integer(class.class_id.0 as i64));
        let handle_temp = self.new_temp_for_expr(body, result_expr);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(handle_temp),
            value: Rvalue::Spawn {
                target,
                instance,
                size: hir::PoolSize::Fixed(1),
                objective,
                config,
            },
            span,
        });
        let push_temp = self.new_temp(MirType::Unknown);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(push_temp),
            value: Rvalue::Call {
                kind: CallKind::Sync,
                target: CallTarget::Function(SmolStr::new("__wr_list_push")),
                args: vec![Value::Temp(list_temp), Value::Temp(handle_temp)],
            },
            span,
        });

        let next_temp = self.new_temp(MirType::Integer);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(next_temp),
            value: Rvalue::Binary {
                op: BinaryOp::Add,
                lhs: Value::Local(idx_local),
                rhs: Value::Const(Literal::Integer(1)),
            },
            span,
        });
        self.push_stmt(MirStmt::Assign {
            place: Place::Local(idx_local),
            value: Rvalue::Use(Value::Temp(next_temp)),
            span,
        });
        if self.block_is_open(self.current_block) {
            self.set_terminator(Terminator::Jump {
                target: head_block,
                span,
            });
        }

        self.current_block = exit_block;
        let pool_temp = self.new_temp_for_expr(body, result_expr);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(pool_temp),
            value: Rvalue::PoolNew {
                handles: Value::Temp(list_temp),
                objective,
                min_size: min_size.unwrap_or(0),
                max_size: max_size.unwrap_or(0),
                weight: weight.unwrap_or(0),
                queue_cap: queue_cap.unwrap_or(0),
            },
            span,
        });
        Some(Value::Temp(pool_temp))
    }

    pub(crate) fn class_target_info(
        &mut self,
        body: &hir::Body,
        target_expr: hir::Idx<Expr>,
    ) -> Option<ClassTargetInfo> {
        match &body.exprs[target_expr] {
            Expr::Variable(name) => {
                let class_id = self.type_tags.get(name).copied()?;
                let fields = self.class_fields.get(name).cloned().unwrap_or_default();
                let field_defaults = self
                    .class_field_defaults
                    .get(name)
                    .cloned()
                    .unwrap_or_else(|| vec![None; fields.len()]);
                Some(ClassTargetInfo {
                    name: name.clone(),
                    class_id,
                    fields,
                    field_defaults,
                    field_values: Vec::new(),
                })
            }
            Expr::Call { callee, args, .. } => {
                let Expr::Variable(name) = &body.exprs[*callee] else {
                    return None;
                };
                let class_name = builtin_record_by_function(name.as_str())
                    .map(|record| SmolStr::new(record.name))
                    .unwrap_or_else(|| name.clone());
                let class_id = self.type_tags.get(&class_name).copied()?;
                let fields = self
                    .class_fields
                    .get(&class_name)
                    .cloned()
                    .unwrap_or_default();
                let field_defaults = self
                    .class_field_defaults
                    .get(&class_name)
                    .cloned()
                    .unwrap_or_else(|| vec![None; fields.len()]);
                let mut field_values: Vec<Option<Value>> = vec![None; fields.len()];
                let mut positional_index = 0usize;
                for arg in args {
                    match arg {
                        hir::Arg::Positional { value, .. } => {
                            let lowered = self.lower_expr(body, *value);
                            if positional_index < field_values.len() {
                                field_values[positional_index] = Some(lowered);
                            }
                            positional_index += 1;
                        }
                        hir::Arg::Named { name, value, .. } => {
                            let lowered = self.lower_expr(body, *value);
                            if let Some(idx) = fields.iter().position(|f| f == name) {
                                field_values[idx] = Some(lowered);
                            }
                        }
                    }
                }
                Some(ClassTargetInfo {
                    name: class_name,
                    class_id,
                    fields,
                    field_defaults,
                    field_values,
                })
            }
            _ => None,
        }
    }

    pub(crate) fn build_class_instance(
        &mut self,
        class: &ClassTargetInfo,
        span: TextRange,
    ) -> Value {
        let temp = self.new_temp(MirType::Unknown);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(temp),
            value: Rvalue::ClassInit {
                class_id: class.class_id.0 as u32,
                fields: class.fields.clone(),
            },
            span,
        });
        let has_explicit_fields = class.field_values.iter().any(|val| val.is_some());
        for (idx, value) in class.field_values.iter().enumerate() {
            if let Some(value) = value {
                self.push_stmt(MirStmt::SetField {
                    base: Value::Temp(temp),
                    field: class.fields.get(idx).cloned().unwrap_or_default(),
                    slot: Some(idx as u32),
                    value: value.clone(),
                    span,
                });
            }
        }
        for (idx, default) in class.field_defaults.iter().enumerate() {
            if class
                .field_values
                .get(idx)
                .and_then(|val| val.as_ref())
                .is_none()
                && let Some(default) = default
            {
                let value = self.lower_field_default(default, span);
                self.push_stmt(MirStmt::SetField {
                    base: Value::Temp(temp),
                    field: class.fields.get(idx).cloned().unwrap_or_default(),
                    slot: Some(idx as u32),
                    value,
                    span,
                });
            }
        }
        if has_explicit_fields {
            self.maybe_call_configure(&class.name, Value::Temp(temp), span);
        }
        Value::Temp(temp)
    }

    pub(crate) fn build_ray_query_value(
        &mut self,
        origin: Value,
        direction: Value,
        max_distance: Value,
        min_step: Value,
        hit_epsilon: Value,
        max_steps: Value,
        span: TextRange,
    ) -> Value {
        let mut class = self.synthetic_class_target_info("RayQuery");
        Self::set_class_field_value(&mut class, "origin", origin);
        Self::set_class_field_value(&mut class, "direction", direction);
        Self::set_class_field_value(&mut class, "max_distance", max_distance);
        Self::set_class_field_value(&mut class, "min_step", min_step);
        Self::set_class_field_value(&mut class, "hit_epsilon", hit_epsilon);
        Self::set_class_field_value(&mut class, "max_steps", max_steps);
        self.build_class_instance(&class, span)
    }

    pub(crate) fn lower_field_default(
        &mut self,
        default: &hir::FieldDefault,
        span: TextRange,
    ) -> Value {
        match default {
            hir::FieldDefault::Literal(lit) => Value::Const(lit.clone()),
            hir::FieldDefault::List(items) => {
                let values = items
                    .iter()
                    .map(|item| self.lower_field_default(item, span))
                    .collect();
                let temp = self.new_temp(MirType::Unknown);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(temp),
                    value: Rvalue::BuildList {
                        items: values,
                        alloc: crate::mir::ir::AllocKind::LocalTemp,
                    },
                    span,
                });
                Value::Temp(temp)
            }
            hir::FieldDefault::Map(items) => {
                let values = items
                    .iter()
                    .map(|(key, value)| {
                        let key = self.lower_field_default(key, span);
                        let value = self.lower_field_default(value, span);
                        (key, value)
                    })
                    .collect();
                let temp = self.new_temp(MirType::Unknown);
                self.push_stmt(MirStmt::Assign {
                    place: Place::Temp(temp),
                    value: Rvalue::BuildMap {
                        items: values,
                        alloc: crate::mir::ir::AllocKind::LocalTemp,
                    },
                    span,
                });
                Value::Temp(temp)
            }
        }
    }

    pub(crate) fn maybe_call_configure(
        &mut self,
        class_name: &SmolStr,
        receiver: Value,
        span: TextRange,
    ) {
        if self.name == SmolStr::new(format!("{}.{}", class_name, "__configure__")) {
            return;
        }
        let method_id = match self
            .class_method_ids
            .get(class_name)
            .and_then(|methods| methods.get(&SmolStr::new("__configure__")))
        {
            Some(method_id) => *method_id,
            None => return,
        };
        let temp = self.new_temp(MirType::Unknown);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(temp),
            value: Rvalue::Call {
                kind: CallKind::Sync,
                target: CallTarget::Method {
                    receiver,
                    method: SmolStr::new(format!("{}.{}", class_name, "__configure__")),
                    method_id: Some(method_id),
                },
                args: Vec::new(),
            },
            span,
        });
    }

    pub(crate) fn parse_pool_of(
        &self,
        body: &hir::Body,
        expr_id: hir::Idx<Expr>,
    ) -> Option<PoolOfSpec> {
        let (callee, args) = match &body.exprs[expr_id] {
            Expr::Call { callee, args, .. } => (callee, args),
            _ => return None,
        };
        let Expr::Member { object, member, .. } = &body.exprs[*callee] else {
            return None;
        };
        if member.as_str() != "of" {
            return None;
        }
        if !matches!(&body.exprs[*object], Expr::Variable(name) if name.as_str() == "Pool") {
            return None;
        }
        let mut class_expr = None;
        let mut size = None;
        let mut objective = None;
        let mut config = SpawnConfig::default();
        let mut min_size = None;
        let mut max_size = None;
        let mut weight = None;
        let mut queue_cap: Option<i64> = None;
        for arg in args {
            match arg {
                hir::Arg::Positional { value, .. } => {
                    if class_expr.is_none() {
                        class_expr = Some(*value);
                    }
                }
                hir::Arg::Named { name, value, .. } => match name.as_str() {
                    "size" => {
                        if let Some(__wr_pool_size) = pool_size_from_expr(body, *value) {
                            size = Some(__wr_pool_size);
                        }
                    }
                    "objective" => {
                        if let Some(obj) = objective_from_expr(body, *value) {
                            objective = Some(obj);
                        }
                    }
                    "min" => {
                        min_size = int_literal_from_expr(body, *value);
                    }
                    "max" => {
                        max_size = int_literal_from_expr(body, *value);
                    }
                    "weight" => {
                        weight = int_literal_from_expr(body, *value);
                    }
                    "batch" => {
                        if let Some(limit) = batch_limit_from_expr(body, *value) {
                            config.batch_limit = Some(limit);
                        }
                    }
                    "backpressure" => {
                        if let Some(bp) = backpressure_from_expr(body, *value) {
                            config.mailbox_cap = bp.mailbox_cap;
                            config.enqueue_timeout_ms = bp.enqueue_timeout_ms;
                            queue_cap = bp.queue_cap;
                        }
                    }
                    _ => {}
                },
            }
        }
        class_expr.map(|expr| PoolOfSpec {
            class_expr: expr,
            size,
            objective,
            config,
            min_size,
            max_size,
            weight,
            queue_cap,
        })
    }

    pub(crate) fn field_slot(&self, class_name: &str, field_name: &str) -> Option<u32> {
        self.class_fields
            .get(&SmolStr::new(class_name))
            .and_then(|fields| fields.iter().position(|field| field.as_str() == field_name))
            .map(|idx| idx as u32)
    }

    pub(crate) fn assign_use(&mut self, place: Place, value: Value, span: TextRange) {
        self.push_stmt(MirStmt::Assign {
            place,
            value: Rvalue::Use(value),
            span,
        });
    }

    pub(crate) fn lower_binary_temp(
        &mut self,
        ty: MirType,
        op: BinaryOp,
        lhs: Value,
        rhs: Value,
        span: TextRange,
    ) -> Value {
        let temp = self.new_temp(ty);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(temp),
            value: Rvalue::Binary { op, lhs, rhs },
            span,
        });
        Value::Temp(temp)
    }

    pub(crate) fn lower_unary_temp(
        &mut self,
        ty: MirType,
        op: hir::UnaryOp,
        operand: Value,
        span: TextRange,
    ) -> Value {
        let temp = self.new_temp(ty);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(temp),
            value: Rvalue::Unary { op, operand },
            span,
        });
        Value::Temp(temp)
    }

    pub(crate) fn lower_call_temp(
        &mut self,
        ty: MirType,
        target: SmolStr,
        args: Vec<Value>,
        span: TextRange,
    ) -> Value {
        let temp = self.new_temp(ty);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(temp),
            value: Rvalue::Call {
                kind: CallKind::Sync,
                target: CallTarget::Function(target),
                args,
            },
            span,
        });
        Value::Temp(temp)
    }

    pub(crate) fn lower_string_interp_temp(
        &mut self,
        parts: Vec<StringPartValue>,
        span: TextRange,
    ) -> Value {
        let temp = self.new_temp(MirType::String);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(temp),
            value: Rvalue::StringInterp {
                parts,
                alloc: AllocKind::LocalTemp,
            },
            span,
        });
        Value::Temp(temp)
    }

    pub(crate) fn lower_string_concat_temp(
        &mut self,
        lhs: Value,
        rhs: Value,
        span: TextRange,
    ) -> Value {
        let temp = self.new_temp(MirType::String);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(temp),
            value: Rvalue::StrConcat {
                parts: vec![lhs, rhs],
                alloc: AllocKind::LocalTemp,
            },
            span,
        });
        Value::Temp(temp)
    }

    pub(crate) fn synthetic_class_target_info(&self, class_name: &str) -> ClassTargetInfo {
        let name = SmolStr::new(class_name);
        let fields = self.class_fields.get(&name).cloned().unwrap_or_default();
        let field_defaults = self
            .class_field_defaults
            .get(&name)
            .cloned()
            .unwrap_or_else(|| vec![None; fields.len()]);
        let field_values = vec![None; fields.len()];
        let class_id = self.type_tags.get(&name).copied().unwrap_or(TypeTagId(0));
        ClassTargetInfo {
            name,
            class_id,
            fields,
            field_defaults,
            field_values,
        }
    }

    pub(crate) fn set_class_field_value(
        class: &mut ClassTargetInfo,
        field_name: &str,
        value: Value,
    ) {
        if let Some(idx) = class
            .fields
            .iter()
            .position(|field| field.as_str() == field_name)
        {
            class.field_values[idx] = Some(value);
        }
    }

    pub(crate) fn set_class_field_value_at(
        class: &mut ClassTargetInfo,
        index: usize,
        value: Value,
    ) {
        if index < class.field_values.len() {
            class.field_values[index] = Some(value);
        }
    }

    pub(crate) fn lower_dispatch_compute_call(
        &mut self,
        body: &hir::Body,
        span: TextRange,
        spec: &ParsedKernelDispatch,
    ) -> Value {
        let workgroups_x = self.lower_expr(body, spec.workgroups[0]);
        let workgroups_y = self.lower_expr(body, spec.workgroups[1]);
        let workgroups_z = self.lower_expr(body, spec.workgroups[2]);
        let workgroup_size_x = self.lower_expr(body, spec.workgroup_size[0]);
        let workgroup_size_y = self.lower_expr(body, spec.workgroup_size[1]);
        let workgroup_size_z = self.lower_expr(body, spec.workgroup_size[2]);
        let schedule = spec
            .schedule
            .map(|expr| self.lower_expr(body, expr))
            .unwrap_or(Value::Const(Literal::Nil));
        let kernel_args = spec
            .kernel_args
            .iter()
            .map(|expr| self.lower_expr(body, *expr))
            .collect::<Vec<_>>();

        let workgroups_x_local = self.new_local(
            SmolStr::new(format!("$gpu_workgroups_x{}", self.locals.len())),
            false,
            MirType::Integer,
        );
        let workgroups_y_local = self.new_local(
            SmolStr::new(format!("$gpu_workgroups_y{}", self.locals.len())),
            false,
            MirType::Integer,
        );
        let workgroups_z_local = self.new_local(
            SmolStr::new(format!("$gpu_workgroups_z{}", self.locals.len())),
            false,
            MirType::Integer,
        );
        let workgroup_size_x_local = self.new_local(
            SmolStr::new(format!("$gpu_workgroup_size_x{}", self.locals.len())),
            false,
            MirType::Integer,
        );
        let workgroup_size_y_local = self.new_local(
            SmolStr::new(format!("$gpu_workgroup_size_y{}", self.locals.len())),
            false,
            MirType::Integer,
        );
        let workgroup_size_z_local = self.new_local(
            SmolStr::new(format!("$gpu_workgroup_size_z{}", self.locals.len())),
            false,
            MirType::Integer,
        );
        for (local, value) in [
            (workgroups_x_local, workgroups_x),
            (workgroups_y_local, workgroups_y),
            (workgroups_z_local, workgroups_z),
            (workgroup_size_x_local, workgroup_size_x),
            (workgroup_size_y_local, workgroup_size_y),
            (workgroup_size_z_local, workgroup_size_z),
        ] {
            self.push_stmt(MirStmt::Assign {
                place: Place::Local(local),
                value: Rvalue::Use(value),
                span,
            });
        }

        let total_x_local = self.new_local(
            SmolStr::new(format!("$gpu_total_x{}", self.locals.len())),
            false,
            MirType::Integer,
        );
        let total_y_local = self.new_local(
            SmolStr::new(format!("$gpu_total_y{}", self.locals.len())),
            false,
            MirType::Integer,
        );
        let total_z_local = self.new_local(
            SmolStr::new(format!("$gpu_total_z{}", self.locals.len())),
            false,
            MirType::Integer,
        );
        let total_xy_local = self.new_local(
            SmolStr::new(format!("$gpu_total_xy{}", self.locals.len())),
            false,
            MirType::Integer,
        );
        let total_count_local = self.new_local(
            SmolStr::new(format!("$gpu_total_count{}", self.locals.len())),
            false,
            MirType::Integer,
        );
        for (local, lhs, rhs) in [
            (
                total_x_local,
                Value::Local(workgroups_x_local),
                Value::Local(workgroup_size_x_local),
            ),
            (
                total_y_local,
                Value::Local(workgroups_y_local),
                Value::Local(workgroup_size_y_local),
            ),
            (
                total_z_local,
                Value::Local(workgroups_z_local),
                Value::Local(workgroup_size_z_local),
            ),
        ] {
            let temp = self.new_temp(MirType::Integer);
            self.push_stmt(MirStmt::Assign {
                place: Place::Temp(temp),
                value: Rvalue::Binary {
                    op: BinaryOp::Mul,
                    lhs,
                    rhs,
                },
                span,
            });
            self.push_stmt(MirStmt::Assign {
                place: Place::Local(local),
                value: Rvalue::Use(Value::Temp(temp)),
                span,
            });
        }
        let total_xy_temp = self.new_temp(MirType::Integer);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(total_xy_temp),
            value: Rvalue::Binary {
                op: BinaryOp::Mul,
                lhs: Value::Local(total_x_local),
                rhs: Value::Local(total_y_local),
            },
            span,
        });
        self.push_stmt(MirStmt::Assign {
            place: Place::Local(total_xy_local),
            value: Rvalue::Use(Value::Temp(total_xy_temp)),
            span,
        });
        let total_count_temp = self.new_temp(MirType::Integer);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(total_count_temp),
            value: Rvalue::Binary {
                op: BinaryOp::Mul,
                lhs: Value::Local(total_xy_local),
                rhs: Value::Local(total_z_local),
            },
            span,
        });
        self.push_stmt(MirStmt::Assign {
            place: Place::Local(total_count_local),
            value: Rvalue::Use(Value::Temp(total_count_temp)),
            span,
        });

        let dispatch_begin = self.new_temp(MirType::Nil);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(dispatch_begin),
            value: Rvalue::Call {
                kind: CallKind::Sync,
                target: CallTarget::Function(SmolStr::new("__wr_gpu_dispatch_begin")),
                args: vec![
                    Value::Local(workgroups_x_local),
                    Value::Local(workgroups_y_local),
                    Value::Local(workgroups_z_local),
                    Value::Local(workgroup_size_x_local),
                    Value::Local(workgroup_size_y_local),
                    Value::Local(workgroup_size_z_local),
                    schedule,
                ],
            },
            span,
        });

        let loop_index_local = self.new_local(
            SmolStr::new(format!("$gpu_linear_index{}", self.locals.len())),
            true,
            MirType::Integer,
        );
        self.push_stmt(MirStmt::Assign {
            place: Place::Local(loop_index_local),
            value: Rvalue::Use(Value::Const(Literal::Integer(0))),
            span,
        });

        let head_block = self.new_block();
        let body_block = self.new_block();
        let exit_block = self.new_block();
        self.set_terminator(Terminator::Jump {
            target: head_block,
            span,
        });

        self.current_block = head_block;
        let cond_temp = self.new_temp(MirType::Boolean);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(cond_temp),
            value: Rvalue::Binary {
                op: BinaryOp::Lt,
                lhs: Value::Local(loop_index_local),
                rhs: Value::Local(total_count_local),
            },
            span,
        });
        self.set_terminator(Terminator::Branch {
            cond: Value::Temp(cond_temp),
            then_target: body_block,
            else_target: exit_block,
            span,
        });

        self.current_block = body_block;
        let dispatch_select = self.new_temp(MirType::Nil);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(dispatch_select),
            value: Rvalue::Call {
                kind: CallKind::Sync,
                target: CallTarget::Function(SmolStr::new("__wr_gpu_dispatch_select_invocation")),
                args: vec![Value::Local(loop_index_local)],
            },
            span,
        });

        let kernel_result = self.new_temp(MirType::Nil);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(kernel_result),
            value: Rvalue::Call {
                kind: CallKind::Sync,
                target: CallTarget::Function(spec.kernel.clone()),
                args: kernel_args,
            },
            span,
        });

        let next_index = self.new_temp(MirType::Integer);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(next_index),
            value: Rvalue::Binary {
                op: BinaryOp::Add,
                lhs: Value::Local(loop_index_local),
                rhs: Value::Const(Literal::Integer(1)),
            },
            span,
        });
        self.push_stmt(MirStmt::Assign {
            place: Place::Local(loop_index_local),
            value: Rvalue::Use(Value::Temp(next_index)),
            span,
        });
        self.set_terminator(Terminator::Jump {
            target: head_block,
            span,
        });

        self.current_block = exit_block;
        let dispatch_end = self.new_temp(MirType::Nil);
        self.push_stmt(MirStmt::Assign {
            place: Place::Temp(dispatch_end),
            value: Rvalue::Call {
                kind: CallKind::Sync,
                target: CallTarget::Function(SmolStr::new("__wr_gpu_dispatch_end")),
                args: Vec::new(),
            },
            span,
        });
        Value::Const(Literal::Nil)
    }

    pub(crate) fn lower_await(
        &mut self,
        body: &hir::Body,
        expr_id: hir::Idx<Expr>,
        span: TextRange,
    ) -> Value {
        let pending = self.lower_pending_call_or_value(body, expr_id, span);
        let temp = self.new_temp_for_expr(body, expr_id);
        self.push_stmt(MirStmt::Await {
            dst: Place::Temp(temp),
            pending,
            span,
        });
        Value::Temp(temp)
    }

    pub(crate) fn lower_pending_call_or_value(
        &mut self,
        body: &hir::Body,
        expr_id: hir::Idx<Expr>,
        span: TextRange,
    ) -> Value {
        if let Expr::Call { callee, args, .. } = &body.exprs[expr_id] {
            let kind = if self.is_actor_call(body, *callee) {
                CallKind::Actor
            } else {
                CallKind::Sync
            };
            let (target, args) = self.lower_call_target(body, *callee, args);
            let temp = self.new_temp_for_expr(body, expr_id);
            self.push_stmt(MirStmt::Assign {
                place: Place::Temp(temp),
                value: Rvalue::Call { kind, target, args },
                span,
            });
            Value::Temp(temp)
        } else {
            self.lower_expr(body, expr_id)
        }
    }

    pub(crate) fn is_actor_call(&self, body: &hir::Body, callee: hir::Idx<Expr>) -> bool {
        if let Expr::Member { object, .. } = &body.exprs[callee] {
            matches!(self.expr_type(body, *object), MirType::Actor(_))
        } else {
            false
        }
    }

    pub(crate) fn lower_call_target(
        &mut self,
        body: &hir::Body,
        callee: hir::Idx<Expr>,
        args: &[hir::Arg],
    ) -> (CallTarget, Vec<Value>) {
        let mut values = Vec::with_capacity(args.len());
        for arg in args {
            match arg {
                hir::Arg::Positional { value, .. } => {
                    values.push(self.lower_expr(body, *value));
                }
                hir::Arg::Named { value, .. } => {
                    values.push(self.lower_expr(body, *value));
                }
            }
        }

        match &body.exprs[callee] {
            Expr::Member { object, member, .. } => {
                let receiver = self.lower_expr(body, *object);
                let collection_intrinsic = match self.expr_type(body, *object) {
                    MirType::Named(name) if name.as_str() == "List" => match member.as_str() {
                        "push" => Some("__wr_list_push"),
                        "len" => Some("__wr_list_len"),
                        _ => None,
                    },
                    MirType::Named(name) if name.as_str() == "Map" => match member.as_str() {
                        "set" => Some("__wr_map_set"),
                        "get" => Some("__wr_map_get"),
                        "len" => Some("__wr_map_len"),
                        _ => None,
                    },
                    _ => None,
                };
                let collection_intrinsic = collection_intrinsic.or_else(|| match member.as_str() {
                    "push" => Some("__wr_list_push"),
                    "set" => Some("__wr_map_set"),
                    "get" => Some("__wr_map_get"),
                    "len" => {
                        if matches!(&body.exprs[*object], Expr::List(_)) {
                            Some("__wr_list_len")
                        } else {
                            Some("__wr_map_len")
                        }
                    }
                    _ => None,
                });
                if let Some(intrinsic) = collection_intrinsic {
                    let mut intrinsic_args = Vec::with_capacity(values.len() + 1);
                    intrinsic_args.push(receiver);
                    intrinsic_args.extend(values);
                    return (
                        CallTarget::Function(SmolStr::new(intrinsic)),
                        intrinsic_args,
                    );
                }
                let class_hint = match &body.exprs[*object] {
                    Expr::Variable(name) if self.type_tags.contains_key(name) => Some(name.clone()),
                    _ => None,
                };
                if let MirType::Named(class_name) = self.expr_type(body, *object)
                    && let Some(methods) = self.interface_methods.get(&class_name)
                    && methods.contains(member)
                {
                    let mut args_with_recv = Vec::with_capacity(values.len() + 1);
                    args_with_recv.push(receiver.clone());
                    args_with_recv.extend(values);
                    let func_name = SmolStr::new(format!("{}.{}", class_name, member));
                    return (CallTarget::Function(func_name), args_with_recv);
                }
                let (method_id, method_name) = match self.expr_type(body, *object) {
                    MirType::Actor(inner) => {
                        if let MirType::Named(class_name) = *inner {
                            (
                                self.method_id_for(&class_name, member),
                                SmolStr::new(format!("{}.{}", class_name, member)),
                            )
                        } else {
                            (None, member.clone())
                        }
                    }
                    MirType::Named(class_name) => (
                        self.method_id_for(&class_name, member),
                        SmolStr::new(format!("{}.{}", class_name, member)),
                    ),
                    _ => {
                        if let Some(class_name) = class_hint {
                            (
                                self.method_id_for(&class_name, member),
                                SmolStr::new(format!("{}.{}", class_name, member)),
                            )
                        } else {
                            (None, member.clone())
                        }
                    }
                };
                if method_id.is_none()
                    && !method_name.as_str().contains('.')
                    && let Some(interface_dispatch_target) =
                        self.resolve_unique_interface_dispatch_target(member)
                {
                    let mut args_with_recv = Vec::with_capacity(values.len() + 1);
                    args_with_recv.push(receiver.clone());
                    args_with_recv.extend(values);
                    return (
                        CallTarget::Function(interface_dispatch_target),
                        args_with_recv,
                    );
                }
                (
                    CallTarget::Method {
                        receiver,
                        method: method_name,
                        method_id,
                    },
                    values,
                )
            }
            Expr::Variable(name) if self.function_names.contains(name) => {
                let mut call_args = values;
                if matches!(
                    name.as_str(),
                    "transform3_identity" | "compose_transform3" | "inverse_transform3"
                ) && let Some(class_id) = self.type_tags.get(&SmolStr::new("Transform3"))
                {
                    call_args.insert(0, Value::Const(Literal::Integer(class_id.0 as i64)));
                }
                if name.as_str() == "assert" && call_args.len() == 1 {
                    call_args.push(Value::Const(Literal::Nil));
                }
                (CallTarget::Function(name.clone()), call_args)
            }
            _ => {
                let callee_value = self.lower_expr(body, callee);
                (CallTarget::Indirect(callee_value), values)
            }
        }
    }

    pub(crate) fn method_id_for(&self, class_name: &SmolStr, method: &SmolStr) -> Option<u32> {
        self.class_method_ids
            .get(class_name)
            .and_then(|methods| methods.get(method).copied())
    }

    pub(crate) fn resolve_unique_interface_dispatch_target(
        &self,
        method: &SmolStr,
    ) -> Option<SmolStr> {
        let mut matched_interface: Option<&SmolStr> = None;
        for (interface_name, methods) in &self.interface_methods {
            if !methods.contains(method) {
                continue;
            }
            if matched_interface.is_some() {
                return None;
            }
            matched_interface = Some(interface_name);
        }
        matched_interface.map(|interface_name| SmolStr::new(format!("{interface_name}.{method}")))
    }

    pub(crate) fn member_slot_hint(
        &self,
        body: &hir::Body,
        object_expr: hir::Idx<Expr>,
        member: &SmolStr,
    ) -> Option<u32> {
        let MirType::Named(class_name) = self.expr_type(body, object_expr) else {
            return None;
        };
        self.class_fields
            .get(&class_name)
            .and_then(|fields| fields.iter().position(|field| field == member))
            .map(|idx| idx as u32)
    }

    pub(crate) fn resolve_class_init_target(
        &self,
        body: &hir::Body,
        callee: hir::Idx<Expr>,
    ) -> Option<(SmolStr, TypeTagId)> {
        match &body.exprs[callee] {
            Expr::Variable(name) => {
                let class_name = builtin_record_by_function(name.as_str())
                    .map(|record| SmolStr::new(record.name))
                    .unwrap_or_else(|| name.clone());
                self.type_tags
                    .get(&class_name)
                    .copied()
                    .map(|id| (class_name, id))
            }
            Expr::Member { object, member, .. } => {
                let enum_name = match &body.exprs[*object] {
                    Expr::Variable(name) => Some(name.clone()),
                    Expr::TypeApply { callee, .. } => match &body.exprs[*callee] {
                        Expr::Variable(name) => Some(name.clone()),
                        _ => None,
                    },
                    _ => None,
                };
                let enum_name = enum_name?;
                let class_name = SmolStr::new(format!("{}.{}", enum_name, member));
                self.type_tags
                    .get(&class_name)
                    .copied()
                    .map(|id| (class_name, id))
            }
            _ => None,
        }
    }
    pub(crate) fn is_default_match_pattern(&self, pattern: &hir::Pattern) -> bool {
        match pattern {
            hir::Pattern::Wildcard => true,
            hir::Pattern::Binding(name) => {
                !self.type_tags.contains_key(name) && builtin_type_tag(name).is_none()
            }
            _ => false,
        }
    }
}

fn compile_time_auto_pool_size(objective: i64, min: i64, max: i64, weight: i64) -> i64 {
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1) as i64;
    let base = match objective {
        0 => cores,
        1 => cores.saturating_mul(2),
        2 => (cores / 2).max(1),
        _ => cores,
    };
    let min = if min > 0 { min } else { 1 };
    let max = if max > 0 { max } else { cores.max(1) };
    let weight = if weight > 0 { weight } else { 1 };
    base.saturating_mul(weight).clamp(min, max.max(min))
}

fn builtin_type_tag(name: &SmolStr) -> Option<TypeTagId> {
    match name.as_str() {
        "Integer" => Some(TypeTagId(1)),
        "Boolean" => Some(TypeTagId(2)),
        "Nothing" | "Nil" => Some(TypeTagId(3)),
        "Float" => Some(TypeTagId(4)),
        "String" => Some(TypeTagId(5)),
        "List" => Some(TypeTagId(6)),
        "Map" => Some(TypeTagId(7)),
        "Actor" => Some(TypeTagId(8)),
        "Pending" => Some(TypeTagId(9)),
        "Iterator" => Some(TypeTagId(10)),
        "Result" => Some(TypeTagId(11)),
        "Pool" => Some(TypeTagId(12)),
        "Bytes" => Some(TypeTagId(13)),
        "Mat3" => Some(TypeTagId(14)),
        "Quat" => Some(TypeTagId(15)),
        _ => None,
    }
}

pub(crate) struct PoolOfSpec {
    class_expr: hir::Idx<Expr>,
    size: Option<hir::PoolSize>,
    objective: Option<hir::Objective>,
    config: SpawnConfig,
    min_size: Option<i64>,
    max_size: Option<i64>,
    weight: Option<i64>,
    queue_cap: Option<i64>,
}

pub(crate) struct ClassTargetInfo {
    name: SmolStr,
    class_id: TypeTagId,
    fields: Vec<SmolStr>,
    field_defaults: Vec<Option<hir::FieldDefault>>,
    field_values: Vec<Option<Value>>,
}

fn pool_size_from_expr(body: &hir::Body, expr_id: hir::Idx<Expr>) -> Option<hir::PoolSize> {
    match &body.exprs[expr_id] {
        Expr::Literal(hir::Literal::Integer(value)) => Some(hir::PoolSize::Fixed(*value)),
        Expr::Variable(name) if name.as_str() == "n" => Some(hir::PoolSize::Auto),
        _ => None,
    }
}

fn objective_from_expr(body: &hir::Body, expr_id: hir::Idx<Expr>) -> Option<hir::Objective> {
    match &body.exprs[expr_id] {
        Expr::Variable(name) => hir::Objective::from_str(name.as_str()),
        _ => None,
    }
}

fn int_literal_from_expr(body: &hir::Body, expr_id: hir::Idx<Expr>) -> Option<i64> {
    match &body.exprs[expr_id] {
        Expr::Literal(hir::Literal::Integer(value)) => Some(*value),
        _ => None,
    }
}

struct BackpressureSpec {
    mailbox_cap: Option<i64>,
    enqueue_timeout_ms: Option<i64>,
    queue_cap: Option<i64>,
}

fn batch_limit_from_expr(body: &hir::Body, expr_id: hir::Idx<Expr>) -> Option<i64> {
    int_literal_from_expr(body, expr_id)
}

fn backpressure_from_expr(body: &hir::Body, expr_id: hir::Idx<Expr>) -> Option<BackpressureSpec> {
    match &body.exprs[expr_id] {
        Expr::Variable(name) if name.as_str() == "drop" => Some(BackpressureSpec {
            mailbox_cap: None,
            enqueue_timeout_ms: Some(0),
            queue_cap: Some(0),
        }),
        Expr::Call { callee, args, .. } => {
            let Expr::Variable(name) = &body.exprs[*callee] else {
                return None;
            };
            if name.as_str() != "queue" || args.len() != 1 {
                return None;
            }
            let arg = match &args[0] {
                hir::Arg::Positional { value, .. } => *value,
                hir::Arg::Named { value, .. } => *value,
            };
            match &body.exprs[arg] {
                Expr::Literal(hir::Literal::Integer(value)) => Some(BackpressureSpec {
                    mailbox_cap: Some(*value),
                    enqueue_timeout_ms: None,
                    queue_cap: Some(*value),
                }),
                _ => None,
            }
        }
        _ => None,
    }
}

fn objective_code(objective: hir::Objective) -> i64 {
    match objective {
        hir::Objective::Latency => 0,
        hir::Objective::Throughput => 1,
        hir::Objective::Conservation => 2,
        hir::Objective::Balance => 3,
    }
}

fn mir_type_from_type(ty: &Type) -> MirType {
    match ty {
        Type::Unknown => MirType::Unknown,
        Type::Integer | Type::I32 | Type::U32 | Type::I64 | Type::U64 => MirType::Integer,
        Type::Float | Type::F32 => MirType::Float,
        Type::Boolean => MirType::Boolean,
        Type::String => MirType::String,
        Type::Nil => MirType::Nil,
        Type::List(_) | Type::Array(_, _) => MirType::Named(SmolStr::new("List")),
        Type::Map(_, _) => MirType::Named(SmolStr::new("Map")),
        Type::Named(name, _) => MirType::Named(name.clone()),
        Type::Param(_) => MirType::Unknown,
        Type::Result(ok, err) => MirType::Result(
            Box::new(mir_type_from_type(ok)),
            Box::new(mir_type_from_type(err)),
        ),
        Type::Actor(inner) => MirType::Actor(Box::new(mir_type_from_type(inner))),
        Type::Pending(inner) => MirType::Pending(Box::new(mir_type_from_type(inner))),
        Type::Vec2 => MirType::Vec2,
        Type::Vec3 => MirType::Vec3,
        Type::Vec4 => MirType::Vec4,
        Type::Mat3 => MirType::Mat3,
        Type::Mat4 => MirType::Mat4,
        Type::Quat => MirType::Quat,
        Type::GpuBuffer(_) => MirType::Named(SmolStr::new("Buffer")),
        Type::GpuAtomicI32 => MirType::Named(SmolStr::new("GpuAtomicI32")),
        Type::GpuAtomicU32 => MirType::Named(SmolStr::new("GpuAtomicU32")),
        Type::GpuDispatchSchedule => MirType::Named(SmolStr::new("GpuDispatchSchedule")),
        Type::Texture2D => MirType::Named(SmolStr::new("Texture2D")),
        Type::Sampler => MirType::Named(SmolStr::new("Sampler")),
        _ => MirType::Unknown,
    }
}

pub(crate) fn portable_abi_from_type_ref(
    ty: Option<&crate::hir::TypeRef>,
    module: &hir::Module,
    type_tags: &HashMap<SmolStr, TypeTagId>,
    visiting: &mut HashSet<SmolStr>,
) -> PortableAbiType {
    let Some(ty) = ty else {
        return PortableAbiType::Value;
    };

    match ty.name.as_str() {
        "Bool" => PortableAbiType::Bool,
        "I32" => PortableAbiType::I32,
        "U32" => PortableAbiType::U32,
        "F32" => PortableAbiType::F32,
        "Vec2" => PortableAbiType::Vec2,
        "Vec3" => PortableAbiType::Vec3,
        "Vec4" => PortableAbiType::Vec4,
        "Mat3" => PortableAbiType::Mat3,
        "Mat4" => PortableAbiType::Mat4,
        "Quat" => PortableAbiType::Quat,
        "Array" => match ty.args.as_slice() {
            [inner, len] => len
                .name
                .parse::<usize>()
                .ok()
                .map(|len| {
                    PortableAbiType::Array(
                        Box::new(portable_abi_from_type_ref(
                            Some(inner),
                            module,
                            type_tags,
                            visiting,
                        )),
                        len,
                    )
                })
                .unwrap_or(PortableAbiType::Value),
            _ => PortableAbiType::Value,
        },
        name => portable_value_struct_abi(name, module, type_tags, visiting)
            .unwrap_or(PortableAbiType::Value),
    }
}

pub(crate) fn portable_value_struct_abi(
    name: &str,
    module: &hir::Module,
    type_tags: &HashMap<SmolStr, TypeTagId>,
    visiting: &mut HashSet<SmolStr>,
) -> Option<PortableAbiType> {
    let name = SmolStr::new(name);
    if !visiting.insert(name.clone()) {
        return None;
    }
    let Some(class_id) = type_tags.get(&name).map(|id| id.0 as u32) else {
        visiting.remove(&name);
        return None;
    };
    let fields = if let Some(record) = any_builtin_record(name.as_str()) {
        record
            .fields
            .iter()
            .map(|field| PortableStructField {
                name: SmolStr::new(field.name),
                ty: portable_builtin_abi_from_type(field.ty, module, type_tags, visiting),
            })
            .collect::<Vec<_>>()
    } else {
        let Some(class) = module.classes.iter().find_map(|(_, class)| {
            (class.name == name && matches!(class.role, hir::ClassRole::Value)).then_some(class)
        }) else {
            visiting.remove(&name);
            return None;
        };
        class
            .fields
            .iter()
            .map(|field| PortableStructField {
                name: field.name.clone(),
                ty: portable_abi_from_type_ref(field.ty.as_ref(), module, type_tags, visiting),
            })
            .collect::<Vec<_>>()
    };
    visiting.remove(&name);
    Some(PortableAbiType::Struct {
        name,
        class_id,
        fields,
    })
}

fn portable_builtin_abi_from_type(
    ty: PortableBuiltinType,
    module: &hir::Module,
    type_tags: &HashMap<SmolStr, TypeTagId>,
    visiting: &mut HashSet<SmolStr>,
) -> PortableAbiType {
    match ty {
        PortableBuiltinType::Named(name) => {
            portable_value_struct_abi(name, module, type_tags, visiting)
                .unwrap_or(PortableAbiType::Value)
        }
        _ => portable_builtin_type_abi(ty).unwrap_or(PortableAbiType::Value),
    }
}

fn build_interface_dispatch_functions(
    module: &hir::Module,
    interface_impls: &HashMap<SmolStr, Vec<SmolStr>>,
    type_tags: &HashMap<SmolStr, TypeTagId>,
) -> Vec<MirFunction> {
    let mut functions = Vec::new();
    for (_idx, interface) in module.interfaces.iter() {
        let impls = interface_impls
            .get(&interface.name)
            .cloned()
            .unwrap_or_default();
        for method in &interface.methods {
            let params: Vec<SmolStr> = method.params.iter().map(|p| p.name.clone()).collect();
            functions.push(build_interface_dispatch_function(
                &interface.name,
                &method.name,
                &params,
                &impls,
                type_tags,
            ));
        }
    }
    functions
}

fn build_interface_dispatch_function(
    interface: &SmolStr,
    method: &SmolStr,
    params: &[SmolStr],
    impls: &[SmolStr],
    type_tags: &HashMap<SmolStr, TypeTagId>,
) -> MirFunction {
    let span = TextRange::empty(0.into());
    let mut locals = Vec::new();
    let mut params_ids = Vec::new();
    let mut temps = Vec::new();

    let receiver_id = LocalId(0);
    locals.push(Local {
        name: SmolStr::new("self"),
        mutable: false,
        ty: MirType::Unknown,
    });
    params_ids.push(receiver_id);

    for (idx, name) in params.iter().enumerate() {
        let local_id = LocalId(idx + 1);
        locals.push(Local {
            name: name.clone(),
            mutable: false,
            ty: MirType::Unknown,
        });
        params_ids.push(local_id);
    }

    let mut blocks = Vec::new();
    blocks.push(BasicBlock {
        stmts: Vec::new(),
        terminator: Terminator::Unreachable { span },
    });

    let mut cases = Vec::new();
    let mut impls_with_tags = Vec::new();
    for class in impls {
        let Some(tag) = type_tags.get(class) else {
            continue;
        };
        let block_id = BlockId(blocks.len());
        blocks.push(BasicBlock {
            stmts: Vec::new(),
            terminator: Terminator::Unreachable { span },
        });
        cases.push((SwitchCase::Type(*tag), block_id));
        impls_with_tags.push(class.clone());
    }

    let default_block = BlockId(blocks.len());
    blocks.push(BasicBlock {
        stmts: Vec::new(),
        terminator: Terminator::Unreachable { span },
    });

    blocks[0].terminator = Terminator::Switch {
        scrutinee: Value::Local(receiver_id),
        cases,
        default: default_block,
        span,
    };

    let call_args: Vec<Value> = params_ids.iter().map(|id| Value::Local(*id)).collect();

    for (idx, class) in impls_with_tags.iter().enumerate() {
        let block_id = BlockId(idx + 1);
        if block_id.0 >= blocks.len() {
            continue;
        }
        let temp_id = TempId(temps.len());
        temps.push(Temp {
            ty: MirType::Unknown,
        });
        let func_name = SmolStr::new(format!("{}.{}", class, method));
        blocks[block_id.0].stmts.push(MirStmt::Assign {
            place: Place::Temp(temp_id),
            value: Rvalue::Call {
                kind: CallKind::Sync,
                target: CallTarget::Function(func_name),
                args: call_args.clone(),
            },
            span,
        });
        blocks[block_id.0].terminator = Terminator::Return {
            value: Some(Value::Temp(temp_id)),
            span,
        };
    }

    let crash_temp = TempId(temps.len());
    temps.push(Temp {
        ty: MirType::Unknown,
    });
    blocks[default_block.0].stmts.push(MirStmt::Assign {
        place: Place::Temp(crash_temp),
        value: Rvalue::Crash {
            value: Value::Const(Literal::String(SmolStr::new("interface dispatch failed"))),
        },
        span,
    });
    blocks[default_block.0].terminator = Terminator::Return {
        value: Some(Value::Temp(crash_temp)),
        span,
    };

    MirFunction {
        name: SmolStr::new(format!("{}.{}", interface, method)),
        params: params_ids,
        abi_params: vec![PortableAbiType::Value; params.len() + 1],
        abi_return: PortableAbiType::Value,
        locals,
        temps,
        blocks,
        entry: BlockId(0),
        suspendable: false,
    }
}

fn builtin_function_names() -> Vec<SmolStr> {
    vec![
        SmolStr::new("__wr_assert_err"),
        SmolStr::new("__wr_print"),
        SmolStr::new("__wr_vec_component"),
        SmolStr::new("__wr_bytes_from_string"),
        SmolStr::new("__wr_bytes_from_list"),
        SmolStr::new("__wr_bytes_to_string"),
        SmolStr::new("__wr_bytes_to_list"),
        SmolStr::new("__wr_bytes_len"),
        SmolStr::new("__wr_fs_read_bytes"),
        SmolStr::new("__wr_fs_write_bytes"),
        SmolStr::new("__wr_external_call"),
        SmolStr::new("__wr_http_call"),
        SmolStr::new("__wr_web_parse_json_text"),
        SmolStr::new("__wr_web_render_json_text"),
        SmolStr::new("__wr_auth_hash_password"),
        SmolStr::new("__wr_auth_verify_password_hash"),
        SmolStr::new("__wr_auth_sign_jwt"),
        SmolStr::new("__wr_auth_verify_jwt"),
        SmolStr::new("__wr_auth_generate_secure_token"),
        SmolStr::new("__wr_auth_render_jwks_document"),
        SmolStr::new("vec2"),
        SmolStr::new("vec3"),
        SmolStr::new("vec4"),
        SmolStr::new("quat"),
        SmolStr::new("mat3_identity"),
        SmolStr::new("mat3_cols"),
        SmolStr::new("mat4_identity"),
        SmolStr::new("mat4_cols"),
        SmolStr::new("f32"),
        SmolStr::new("i32"),
        SmolStr::new("i64"),
        SmolStr::new("u32"),
        SmolStr::new("u64"),
        SmolStr::new("dot"),
        SmolStr::new("length"),
        SmolStr::new("normalize"),
        SmolStr::new("cross"),
        SmolStr::new("min"),
        SmolStr::new("max"),
        SmolStr::new("clamp"),
        SmolStr::new("mix"),
        SmolStr::new("abs"),
        SmolStr::new("sign"),
        SmolStr::new("floor"),
        SmolStr::new("ceil"),
        SmolStr::new("fract"),
        SmolStr::new("sin"),
        SmolStr::new("cos"),
        SmolStr::new("sqrt"),
        SmolStr::new("pow"),
        SmolStr::new("distance"),
        SmolStr::new("reflect"),
        SmolStr::new("bounds2_center"),
        SmolStr::new("bounds2_size"),
        SmolStr::new("bounds3_center"),
        SmolStr::new("bounds3_size"),
        SmolStr::new("transform3_identity"),
        SmolStr::new("transform_point"),
        SmolStr::new("transform_vector"),
        SmolStr::new("transform_normal"),
        SmolStr::new("compose_transform3"),
        SmolStr::new("inverse_transform3"),
        SmolStr::new("translate"),
        SmolStr::new("rotate"),
        SmolStr::new("uniform_scale"),
        SmolStr::new("affine_transform"),
        SmolStr::new("warp"),
        SmolStr::new("repeat_linear"),
        SmolStr::new("repeat_grid"),
        SmolStr::new("radial_repeat"),
        SmolStr::new("mirror_array"),
        SmolStr::new("instance_array"),
        SmolStr::new("field_translate_point"),
        SmolStr::new("field_rotate_point"),
        SmolStr::new("field_uniform_scale_point"),
        SmolStr::new("field_affine_transform_point"),
        SmolStr::new("field_warp_point"),
        SmolStr::new("field_repeat_linear_point"),
        SmolStr::new("field_repeat_grid_point"),
        SmolStr::new("field_radial_repeat_point"),
        SmolStr::new("field_mirror_array_point"),
        SmolStr::new("field_instance_array_point"),
        SmolStr::new("field_sweep_coords"),
        SmolStr::new("field_profile_vertices_bounds4"),
        SmolStr::new("field_smooth_union"),
        SmolStr::new("field_smooth_intersection"),
        SmolStr::new("field_smooth_subtract"),
        SmolStr::new("field_bend_point"),
        SmolStr::new("field_twist_point"),
        SmolStr::new("field_taper_point"),
        SmolStr::new("field_displace_point"),
        SmolStr::new("rounded_box"),
        SmolStr::new("ellipsoid"),
        SmolStr::new("cone"),
        SmolStr::new("capped_cone"),
        SmolStr::new("box_frame"),
        SmolStr::new("slab"),
        SmolStr::new("triangle_prism"),
        SmolStr::new("hex_prism"),
        SmolStr::new("sphere"),
        SmolStr::new("box"),
        SmolStr::new("capsule"),
        SmolStr::new("cylinder"),
        SmolStr::new("plane"),
        SmolStr::new("torus"),
        SmolStr::new("circle2"),
        SmolStr::new("rect2"),
        SmolStr::new("rounded_rect2"),
        SmolStr::new("capsule2"),
        SmolStr::new("segment2"),
        SmolStr::new("polygon2"),
        SmolStr::new("polyline2"),
        SmolStr::new("smooth_union"),
        SmolStr::new("smooth_intersection"),
        SmolStr::new("smooth_subtract"),
        SmolStr::new("__wr_primitive_sphere"),
        SmolStr::new("__wr_primitive_box"),
        SmolStr::new("__wr_primitive_capsule"),
        SmolStr::new("__wr_primitive_cylinder"),
        SmolStr::new("__wr_primitive_plane"),
        SmolStr::new("__wr_primitive_torus"),
        SmolStr::new("field_union"),
        SmolStr::new("field_intersection"),
        SmolStr::new("field_subtract"),
        SmolStr::new("bend"),
        SmolStr::new("twist"),
        SmolStr::new("taper"),
        SmolStr::new("displace"),
        SmolStr::new("__wr_field_distance_capture"),
        SmolStr::new("__wr_field_normal_capture"),
        SmolStr::new("__wr_shape_distance_capture"),
        SmolStr::new("__wr_shape_normal_capture"),
        SmolStr::new("__wr_scene_trace_capture"),
        SmolStr::new("__wr_scene_surface_capture"),
        SmolStr::new("__wr_scene_radiance_capture"),
        SmolStr::new("__wr_scene_medium_capture"),
        SmolStr::new("__wr_field_distance_batch_queries"),
        SmolStr::new("__wr_shape_distance_batch_queries"),
        SmolStr::new("__wr_field_normal_batch_queries"),
        SmolStr::new("__wr_shape_normal_batch_queries"),
        SmolStr::new("__wr_scene_trace_batch_queries"),
        SmolStr::new("__wr_scene_surface_batch_queries"),
        SmolStr::new("__wr_scene_occluded_batch_queries"),
        SmolStr::new("__wr_scene_trace_queries"),
        SmolStr::new("__wr_scene_surface_queries"),
        SmolStr::new("gpu_buffer_new"),
        SmolStr::new("gpu_buffer_len"),
        SmolStr::new("gpu_buffer_get"),
        SmolStr::new("gpu_buffer_set"),
        SmolStr::new("gpu_atomic_i32_new"),
        SmolStr::new("gpu_atomic_i32_drop"),
        SmolStr::new("gpu_atomic_i32_load"),
        SmolStr::new("gpu_atomic_i32_store"),
        SmolStr::new("gpu_atomic_i32_fetch_add"),
        SmolStr::new("gpu_atomic_u32_new"),
        SmolStr::new("gpu_atomic_u32_drop"),
        SmolStr::new("gpu_atomic_u32_load"),
        SmolStr::new("gpu_atomic_u32_store"),
        SmolStr::new("gpu_atomic_u32_fetch_add"),
        SmolStr::new("global_invocation_id"),
        SmolStr::new("local_invocation_id"),
        SmolStr::new("workgroup_id"),
        SmolStr::new("num_workgroups"),
        SmolStr::new("workgroup_size"),
        SmolStr::new("gpu_schedule_deterministic"),
        SmolStr::new("gpu_schedule_reverse"),
        SmolStr::new("gpu_schedule_shuffle"),
        SmolStr::new("gpu_schedule_workgroup_reverse"),
        SmolStr::new("gpu_schedule_workgroup_shuffle"),
        SmolStr::new("gpu_schedule_round_robin_workgroups"),
        SmolStr::new("dispatch_compute"),
        SmolStr::new("__wr_gpu_dispatch_begin"),
        SmolStr::new("__wr_gpu_dispatch_select_invocation"),
        SmolStr::new("__wr_gpu_dispatch_end"),
        SmolStr::new("__wr_map_new"),
        SmolStr::new("__wr_list_push"),
        SmolStr::new("__wr_list_get"),
        SmolStr::new("__wr_list_set"),
        SmolStr::new("__wr_list_len"),
        SmolStr::new("__wr_map_get"),
        SmolStr::new("__wr_map_len"),
        SmolStr::new("__wr_map_set"),
        SmolStr::new("__wr_str_len"),
        SmolStr::new("__wr_log"),
        SmolStr::new("__wr_log_configure"),
        SmolStr::new("__wr_runtime_cpu_count"),
        SmolStr::new("__wr_reactor_new"),
        SmolStr::new("__wr_reactor_drop"),
        SmolStr::new("__wr_reactor_register"),
        SmolStr::new("__wr_reactor_deregister"),
        SmolStr::new("__wr_reactor_arm_timer"),
        SmolStr::new("__wr_task_signal_new"),
        SmolStr::new("__wr_task_signal_drop"),
        SmolStr::new("__wr_task_unpark_one"),
        SmolStr::new("__wr_task_unpark_all"),
        SmolStr::new("__wr_task_epoch"),
        SmolStr::new("__wr_atomic_i64_new"),
        SmolStr::new("__wr_atomic_i64_drop"),
        SmolStr::new("__wr_atomic_i64_load"),
        SmolStr::new("__wr_atomic_i64_store"),
        SmolStr::new("__wr_atomic_i64_fetch_add"),
        SmolStr::new("__wr_pool_size"),
        SmolStr::new("__wr_pool_rr"),
        SmolStr::new("__wr_pool_queue_len"),
        SmolStr::new("__wr_actor_mailbox_len"),
        SmolStr::new("__wr_actor_pause"),
        SmolStr::new("__wr_actor_resume"),
        SmolStr::new("__wr_actor_pause_wait"),
        SmolStr::new("__wr_actor_fire_burst_begin"),
        SmolStr::new("__wr_actor_fire_burst_end"),
        SmolStr::new("__wr_actor_fire_burst_abort"),
        SmolStr::new("__wr_metrics_get"),
        SmolStr::new("__wr_metrics_dropped_paused_id"),
        SmolStr::new("__wr_metrics_messages_dropped_id"),
        SmolStr::new("__wr_metrics_scene_trace_id"),
        SmolStr::new("__wr_metrics_field_sample_id"),
        SmolStr::new("__wr_metrics_scene_trace_support_pruned_branch"),
        SmolStr::new("__wr_metrics_scene_trace_candidate_branch"),
        SmolStr::new("__wr_metrics_scene_trace_exact_path"),
        SmolStr::new("__wr_metrics_scene_trace_conservative_path"),
        SmolStr::new("__wr_metrics_scene_trace_hit"),
        SmolStr::new("__wr_metrics_scene_trace_support_pruned_branch_id"),
        SmolStr::new("__wr_metrics_scene_trace_candidate_branch_id"),
        SmolStr::new("__wr_metrics_scene_trace_exact_path_id"),
        SmolStr::new("__wr_metrics_scene_trace_conservative_path_id"),
        SmolStr::new("__wr_metrics_scene_trace_hit_count_id"),
        SmolStr::new("__wr_metrics_scene_trace_hit_steps_total_id"),
        SmolStr::new("__wr_metrics_scene_trace_hit_field_samples_total_id"),
        SmolStr::new("__wr_metrics_scene_trace_steps_le_1_id"),
        SmolStr::new("__wr_metrics_scene_trace_steps_le_4_id"),
        SmolStr::new("__wr_metrics_scene_trace_steps_le_8_id"),
        SmolStr::new("__wr_metrics_scene_trace_steps_le_16_id"),
        SmolStr::new("__wr_metrics_scene_trace_steps_gt_16_id"),
        SmolStr::new("__wr_metrics_scene_trace"),
        SmolStr::new("__wr_metrics_field_sample"),
        SmolStr::new("__wr_metrics_scene_trace_blend_cost"),
        SmolStr::new("__wr_metrics_scene_trace_deformation_cost"),
        SmolStr::new("__wr_metrics_scene_trace_blend_cost_id"),
        SmolStr::new("__wr_metrics_scene_trace_deformation_cost_id"),
        SmolStr::new("__wr_metrics_web_writev_calls_id"),
        SmolStr::new("__wr_metrics_web_sendfile_calls_id"),
        SmolStr::new("__wr_clock_ns"),
        SmolStr::new("__wr_sleep_ms"),
        SmolStr::new("__wr_env_get"),
        SmolStr::new("__wr_env_set"),
        SmolStr::new("__wr_runtime_configure"),
        SmolStr::new("__wr_db_core_open"),
        SmolStr::new("__wr_db_core_close"),
        SmolStr::new("__wr_db_core_submit_batch"),
        SmolStr::new("__wr_db_core_read_point"),
        SmolStr::new("__wr_db_core_read_range"),
        SmolStr::new("__wr_db_core_txn_begin"),
        SmolStr::new("__wr_db_core_txn_prepare"),
        SmolStr::new("__wr_db_core_txn_commit"),
        SmolStr::new("__wr_db_core_txn_abort"),
        SmolStr::new("__wr_db_admin_snapshot_start"),
        SmolStr::new("__wr_db_admin_snapshot_status"),
        SmolStr::new("__wr_db_admin_restore"),
        SmolStr::new("__wr_db_admin_checkpoint_create"),
        SmolStr::new("__wr_db_admin_checkpoint_restore_latest"),
        SmolStr::new("__wr_db_admin_schema_epoch_set"),
        SmolStr::new("__wr_db_admin_schema_set_all_voters_on_target_binary"),
        SmolStr::new("__wr_db_admin_autoscale_tick"),
        SmolStr::new("__wr_db_admin_plan_rehome"),
        SmolStr::new("__wr_db_admin_advance_rehome"),
        SmolStr::new("__wr_db_admin_promote_async_failover"),
        SmolStr::new("__wr_db_explain_checkpoint_count"),
        SmolStr::new("__wr_db_explain_schema_epoch_get"),
        SmolStr::new("__wr_db_explain_health_has_checkpoint_or_schema_error"),
        SmolStr::new("__wr_db_explain_private_mesh_status"),
        SmolStr::new("__wr_db_explain_logical_shard_count"),
        SmolStr::new("__wr_db_explain_active_group_count"),
        SmolStr::new("__wr_db_explain_autoscale_status"),
        SmolStr::new("__wr_db_explain_topology_status"),
        SmolStr::new("__wr_db_explain_shard_map_epoch"),
        SmolStr::new("__wr_db_explain_shard_for_key"),
        SmolStr::new("__wr_db_explain_resolve_owner"),
        SmolStr::new("__wr_db_explain_global_route_lookup"),
    ]
}

fn scene_domain_compat_member(
    member: &str,
) -> Option<(&'static str, &'static str, &'static str, MirType)> {
    match member {
        "geometry_detail" => Some((
            "SpatialDomainContract",
            "spatial",
            "geometry_detail",
            MirType::Integer,
        )),
        "material" => Some((
            "SurfaceDomainContract",
            "surface",
            "material",
            MirType::Boolean,
        )),
        "radiance" => Some((
            "ParticipantDomainContract",
            "participants",
            "radiance",
            MirType::Boolean,
        )),
        "media" => Some((
            "ParticipantDomainContract",
            "participants",
            "media",
            MirType::Boolean,
        )),
        _ => None,
    }
}

pub(crate) fn vector_component_index(ty: MirType, member: &SmolStr) -> Option<usize> {
    match (ty, member.as_str()) {
        (MirType::Vec2, "x") => Some(0),
        (MirType::Vec2, "y") => Some(1),
        (MirType::Vec3, "x") => Some(0),
        (MirType::Vec3, "y") => Some(1),
        (MirType::Vec3, "z") => Some(2),
        (MirType::Vec4, "x") => Some(0),
        (MirType::Vec4, "y") => Some(1),
        (MirType::Vec4, "z") => Some(2),
        (MirType::Vec4, "w") => Some(3),
        (MirType::Quat, "x") => Some(0),
        (MirType::Quat, "y") => Some(1),
        (MirType::Quat, "z") => Some(2),
        (MirType::Quat, "w") => Some(3),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hir::lower as hir_lower;
    use crate::hir::typeck;
    use crate::parser::ast;
    use crate::parser::ast::AstNode;
    use crate::parser::parse;
    use std::collections::BTreeSet;

    fn direct_call_targets(func: &MirFunction) -> BTreeSet<SmolStr> {
        let mut targets = BTreeSet::new();
        for block in &func.blocks {
            for stmt in &block.stmts {
                if let MirStmt::Assign {
                    value:
                        Rvalue::Call {
                            target: CallTarget::Function(name),
                            ..
                        },
                    ..
                } = stmt
                {
                    targets.insert(name.clone());
                }
            }
        }
        targets
    }

    #[test]
    fn test_lower_marks_suspendable() {
        let input = "\
class Whale {\n    fn swim() -> Boolean {\n        return true\n    }\n}\n\nfn f() -> Result[Boolean] {\n    w = detach Whale() * 1\n    return await w.swim()\n}\n";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = hir_lower::lower(root);
        let mir = lower_module(&module);
        let func = mir.functions.iter().find(|f| f.name == "f").unwrap();
        assert!(func.suspendable);
    }

    #[test]
    fn test_lower_if_creates_blocks() {
        let input = "fn f() -> Nothing {\n    if true {\n        x = 1\n    } else {\n        x = 2\n    }\n}\n";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = hir_lower::lower(root);
        let mir = lower_module(&module);
        let func = mir.functions.iter().find(|f| f.name == "f").unwrap();
        assert!(func.blocks.len() >= 3);
    }

    #[test]
    fn test_lower_member_assign_sets_field() {
        let input = "\
class Counter {
    mutable value: Integer
    fn add(delta: Integer) -> Nothing {
        self.value += delta
    }
}
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = hir_lower::lower(root);
        let (_type_errors, type_info) = typeck::check_module_with_info(&module);
        let mir_module = lower_module_with_types(&module, &type_info);
        let func = mir_module
            .functions
            .iter()
            .find(|func| func.name == "Counter.add")
            .expect("missing Counter.add");
        let has_set_field = func.blocks.iter().any(|block| {
            block.stmts.iter().any(
                |stmt| matches!(stmt, MirStmt::SetField { field, .. } if field.as_str() == "value"),
            )
        });
        assert!(has_set_field, "expected SetField for member assign");
    }

    #[test]
    fn test_lower_field_defaults_emits_set_fields() {
        let input = "\
class Foo {
    x: Integer = 1
    y: List[Integer] = [1, 2]
    z: Map[String, Integer] = {\"a\": 1}
}

fn run() -> Nothing {
    a = Foo()
    b = Foo(x=5)
}
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = hir_lower::lower(root);
        let mir_module = lower_module(&module);
        let func = mir_module
            .functions
            .iter()
            .find(|func| func.name == "run")
            .expect("missing run");

        let mut set_x = 0usize;
        let mut set_y = 0usize;
        let mut set_z = 0usize;
        let mut build_list = 0usize;
        let mut build_map = 0usize;

        for block in &func.blocks {
            for stmt in &block.stmts {
                match stmt {
                    MirStmt::SetField { field, .. } if field.as_str() == "x" => set_x += 1,
                    MirStmt::SetField { field, .. } if field.as_str() == "y" => set_y += 1,
                    MirStmt::SetField { field, .. } if field.as_str() == "z" => set_z += 1,
                    _ => {}
                }
                if let MirStmt::Assign { value, .. } = stmt {
                    match value {
                        Rvalue::BuildList { .. } => build_list += 1,
                        Rvalue::BuildMap { .. } => build_map += 1,
                        _ => {}
                    }
                }
            }
        }

        assert_eq!(set_x, 2, "expected default and override for x");
        assert_eq!(set_y, 2, "expected defaults for y in both instances");
        assert_eq!(set_z, 2, "expected defaults for z in both instances");
        assert!(build_list >= 1, "expected BuildList for default list");
        assert!(build_map >= 1, "expected BuildMap for default map");
    }

    #[test]
    fn test_capture_field_queries_lower_without_indirect_calls() {
        let input = r#"field exact distance sphere_field(p: Vec3) -> F32 {
    sphere(radius = 1.0)
}

fn run() -> Nothing {
    scene = capture sphere_field
    distance = distance_at(capture=scene, point=vec3(0.0, 0.0, 2.0))
    normal = normal_at(capture=scene, point=vec3(0.0, 0.0, 2.0))
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = hir_lower::lower(root);
        let (_type_errors, type_info) = typeck::check_module_with_info(&module);
        let mir_module = lower_module_with_types(&module, &type_info);

        let mut indirect_calls = Vec::new();
        for func in &mir_module.functions {
            for block in &func.blocks {
                for stmt in &block.stmts {
                    if let MirStmt::Assign {
                        value: Rvalue::Call { target, .. },
                        ..
                    } = stmt
                        && matches!(target, CallTarget::Indirect(_))
                    {
                        indirect_calls.push((func.name.clone(), target.clone()));
                    }
                }
            }
        }

        assert!(
            indirect_calls.is_empty(),
            "unexpected indirect calls: {indirect_calls:?}"
        );
    }

    #[test]
    fn test_batch_query_calls_route_through_phase9_generated_helpers() {
        let input = r#"field exact distance sphere_field(p: Vec3) -> F32 {
    sphere(radius = 1.0)
}

material shade(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(1.0, 0.0, 0.0),
        roughness=0.4,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape orb_shape {
    field = sphere_field
    material = shade
    payload = Payload(entity_id=u32(1), material_id=u32(1), actor=ActorHandle(id=u32(1), generation=u32(0)))
}

fn run() -> Nothing {
    field_scene = capture sphere_field
    shape_scene = capture orb_shape
    points = [PointQuery(point=vec3(0.0, 0.0, 2.0))]
    rays = [RayQuery(
        origin=vec3(0.0, 0.0, 3.0),
        direction=vec3(0.0, 0.0, -1.0),
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )]
    distance_results = distance_at_batch(capture=field_scene, points=points, backend=dispatch_backend_cpu())
    trace_results = trace_shape_batch(capture=shape_scene, rays=rays, backend=dispatch_backend_virtual_gpu())
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = hir_lower::lower(root);
        let (_type_errors, type_info) = typeck::check_module_with_info(&module);
        let mir_module = lower_module_with_types(&module, &type_info);
        let run = mir_module
            .functions
            .iter()
            .find(|func| func.name == "run")
            .expect("missing run");
        let targets = direct_call_targets(run);
        assert!(targets.contains("__wr_field_distance_batch_queries"));
        assert!(targets.contains("__wr_scene_trace_batch_queries"));
        assert!(
            mir_module
                .functions
                .iter()
                .any(|func| func.name == "__wr_field_distance_batch_queries")
        );
        assert!(
            mir_module
                .functions
                .iter()
                .any(|func| func.name == "__wr_scene_trace_batch_queries")
        );
    }

    #[test]
    fn test_generated_batch_helpers_execute_concrete_scene_paths() {
        let input = r#"field exact distance sphere_field(p: Vec3) -> F32 {
    sphere(radius = 1.0)
}

material shade(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(1.0, 0.0, 0.0),
        roughness=0.4,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape orb_shape {
    field = sphere_field
    material = shade
    payload = Payload(entity_id=u32(1), material_id=u32(1), actor=ActorHandle(id=u32(1), generation=u32(0)))
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = hir_lower::lower(root);
        let (_type_errors, type_info) = typeck::check_module_with_info(&module);
        let mir_module = lower_module_with_types(&module, &type_info);

        let distance_helper = mir_module
            .functions
            .iter()
            .find(|func| func.name == "__wr_field_distance_batch_queries")
            .expect("distance batch helper");
        let trace_helper = mir_module
            .functions
            .iter()
            .find(|func| func.name == "__wr_scene_trace_batch_queries")
            .expect("trace batch helper");

        let distance_targets = direct_call_targets(distance_helper);
        let trace_targets = direct_call_targets(trace_helper);
        assert!(distance_targets.contains("sphere_field"));
        assert!(trace_targets.contains("__wr_shape_trace_orb_shape"));
        assert!(!distance_targets.contains("__wr_field_distance_capture"));
        assert!(!distance_targets.contains("__wr_field_normal_capture"));
        assert!(!distance_targets.contains("__wr_shape_distance_capture"));
        assert!(!distance_targets.contains("__wr_shape_normal_capture"));
        assert!(!trace_targets.contains("__wr_scene_trace_capture"));
        assert!(!trace_targets.contains("__wr_scene_surface_capture"));
        assert!(trace_targets.contains("__wr_gpu_dispatch_begin"));
        assert!(trace_targets.contains("__wr_gpu_dispatch_select_invocation"));
        assert!(trace_targets.contains("__wr_gpu_dispatch_end"));
    }

    #[test]
    fn test_phase9_helpers_route_opaque_scenes_to_conservative_kernels() {
        let input = r#"field exact distance semantic_field(p: Vec3) -> F32 {
    sphere(radius = 1.0)
}

field conservative distance opaque_field(p: Vec3) -> F32 {
    support = Support3(bounds = Bounds3(
        min = vec3(8.0, -1.0, -1.0),
        max = vec3(12.0, 1.0, 1.0)
    ))
    bounds = Bounds3(
        min = vec3(8.0, -1.0, -1.0),
        max = vec3(12.0, 1.0, 1.0)
    )
    return length(p - vec3(10.0, 0.0, 0.0)) - 0.5
}

material shade(hit: Hit3) -> Surface {
    return Surface(
        albedo = vec3(1.0, 0.0, 0.0),
        roughness = 0.4,
        metalness = 0.0,
        clearcoat = 0.0,
        clearcoat_roughness = 0.0,
        sheen = 0.0,
        emissive = vec3(0.0, 0.0, 0.0)
    )
}

shape semantic_scene {
    field = semantic_field
    material = shade
    payload = Payload(entity_id = u32(1), material_id = u32(1), actor = ActorHandle(id = u32(1), generation = u32(0)))
}

shape opaque_scene {
    field = opaque_field
    material = shade
    payload = Payload(entity_id = u32(2), material_id = u32(2), actor = ActorHandle(id = u32(2), generation = u32(0)))
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = hir_lower::lower(root);
        let (_type_errors, type_info) = typeck::check_module_with_info(&module);
        let mir_module = lower_module_with_types(&module, &type_info);

        for helper in [
            "__wr_shape_distance_semantic_scene",
            "__wr_shape_distance_conservative_semantic_scene",
            "__wr_shape_distance_opaque_scene",
            "__wr_shape_distance_conservative_opaque_scene",
            "__wr_shape_trace_semantic_scene",
            "__wr_shape_trace_conservative_semantic_scene",
            "__wr_shape_trace_opaque_scene",
            "__wr_shape_trace_conservative_opaque_scene",
        ] {
            assert!(
                mir_module.functions.iter().any(|func| func.name == helper),
                "expected generated helper `{helper}` to exist"
            );
        }

        let shape_distance_capture = mir_module
            .functions
            .iter()
            .find(|func| func.name == "__wr_shape_distance_capture")
            .expect("shape distance capture helper");
        let scene_trace_capture = mir_module
            .functions
            .iter()
            .find(|func| func.name == "__wr_scene_trace_capture")
            .expect("scene trace capture helper");
        let shape_distance_batch = mir_module
            .functions
            .iter()
            .find(|func| func.name == "__wr_shape_distance_batch_queries")
            .expect("shape distance batch helper");
        let scene_trace_batch = mir_module
            .functions
            .iter()
            .find(|func| func.name == "__wr_scene_trace_batch_queries")
            .expect("scene trace batch helper");

        let shape_distance_capture_targets = direct_call_targets(shape_distance_capture);
        let scene_trace_capture_targets = direct_call_targets(scene_trace_capture);
        let shape_distance_batch_targets = direct_call_targets(shape_distance_batch);
        let scene_trace_batch_targets = direct_call_targets(scene_trace_batch);

        for targets in [
            &shape_distance_capture_targets,
            &shape_distance_batch_targets,
        ] {
            assert!(targets.contains("__wr_shape_distance_semantic_scene"));
            assert!(targets.contains("__wr_shape_distance_conservative_opaque_scene"));
            assert!(!targets.contains("__wr_shape_distance_conservative_semantic_scene"));
            assert!(!targets.contains("__wr_shape_distance_opaque_scene"));
        }

        for targets in [&scene_trace_capture_targets, &scene_trace_batch_targets] {
            assert!(targets.contains("__wr_shape_trace_semantic_scene"));
            assert!(targets.contains("__wr_shape_trace_conservative_opaque_scene"));
            assert!(!targets.contains("__wr_shape_trace_conservative_semantic_scene"));
            assert!(!targets.contains("__wr_shape_trace_opaque_scene"));
        }
    }

    #[test]
    fn test_scalar_shape_trace_skips_support_prune_scaffold_for_opaque_branches() {
        let input = r#"field exact distance near_field(p: Vec3) -> F32 {
    sphere(radius = 0.65)
}

field conservative distance far_custom(p: Vec3) -> F32 {
    support = Support3(bounds = Bounds3(
        min = vec3(8.0, -1.0, -1.0),
        max = vec3(12.0, 1.0, 1.0)
    ))
    bounds = Bounds3(
        min = vec3(8.0, -1.0, -1.0),
        max = vec3(12.0, 1.0, 1.0)
    )
    return length(p - vec3(10.0, 0.0, 0.0)) - 0.5
}

field conservative distance far_semantic(p: Vec3) -> F32 {
    translate = vec3(10.0, 0.0, 0.0) {
        sphere(radius = 0.5)
    }
}

material shade(hit: Hit3) -> Surface {
    return Surface(
        albedo = vec3(1.0, 0.0, 0.0),
        roughness = 0.2,
        metalness = 0.0,
        clearcoat = 0.0,
        clearcoat_roughness = 0.0,
        sheen = 0.0,
        emissive = vec3(0.0, 0.0, 0.0)
    )
}

shape near_shape {
    field = near_field
    material = shade
    payload = Payload(entity_id = u32(1), material_id = u32(1), actor = ActorHandle(id = u32(1), generation = u32(0)))
}

shape far_custom_shape {
    field = far_custom
    material = shade
    payload = Payload(entity_id = u32(2), material_id = u32(2), actor = ActorHandle(id = u32(2), generation = u32(0)))
}

shape far_semantic_shape {
    field = far_semantic
    material = shade
    payload = Payload(entity_id = u32(3), material_id = u32(3), actor = ActorHandle(id = u32(3), generation = u32(0)))
}

shape supported_scene {
    union {
        provenance_policy = nearest
        use near_shape
        use far_custom_shape
    }
}

shape semantic_scene {
    union {
        provenance_policy = nearest
        use near_shape
        use far_semantic_shape
    }
}

fn main() -> Integer {
    supported = trace_shape(
        capture = capture supported_scene,
        ray = ray_query(
            origin = vec3(0.0, 0.0, 3.0),
            direction = vec3(0.0, 0.0, -1.0),
            max_distance = 6.0,
            min_step = 0.05,
            hit_epsilon = 0.001,
            max_steps = 96
        )
    )
    semantic = trace_shape(
        capture = capture semantic_scene,
        ray = ray_query(
            origin = vec3(0.0, 0.0, 3.0),
            direction = vec3(0.0, 0.0, -1.0),
            max_distance = 6.0,
            min_step = 0.05,
            hit_epsilon = 0.001,
            max_steps = 96
        )
    )
    if supported.hit && semantic.hit {
        return 0
    }
    return 1
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = hir_lower::lower(root);
        let (_type_errors, type_info) = typeck::check_module_with_info(&module);
        let mir_module = lower_module_with_types(&module, &type_info);

        let function_names = mir_module
            .functions
            .iter()
            .map(|func| func.name.clone())
            .collect::<Vec<_>>();
        let metric_callers = mir_module
            .functions
            .iter()
            .filter_map(|func| {
                direct_call_targets(func)
                    .contains("__wr_metrics_scene_trace_support_pruned_branch")
                    .then_some(func.name.clone())
            })
            .collect::<Vec<_>>();

        assert!(
            !metric_callers
                .iter()
                .any(|name| name.contains("supported_scene")),
            "optimized MIR still prunes opaque scene branches: callers={metric_callers:?} functions={function_names:?}"
        );
        assert!(
            metric_callers
                .iter()
                .any(|name| name.contains("semantic_scene")),
            "optimized MIR lost semantic support pruning: callers={metric_callers:?} functions={function_names:?}"
        );
    }

    #[test]
    fn test_opt_scalar_shape_trace_skips_support_prune_scaffold_for_opaque_branches() {
        let input = r#"field exact distance near_field(p: Vec3) -> F32 {
    sphere(radius = 0.65)
}

field conservative distance far_custom(p: Vec3) -> F32 {
    support = Support3(bounds = Bounds3(
        min = vec3(8.0, -1.0, -1.0),
        max = vec3(12.0, 1.0, 1.0)
    ))
    bounds = Bounds3(
        min = vec3(8.0, -1.0, -1.0),
        max = vec3(12.0, 1.0, 1.0)
    )
    return length(p - vec3(10.0, 0.0, 0.0)) - 0.5
}

field conservative distance far_semantic(p: Vec3) -> F32 {
    translate = vec3(10.0, 0.0, 0.0) {
        sphere(radius = 0.5)
    }
}

material shade(hit: Hit3) -> Surface {
    return Surface(
        albedo = vec3(1.0, 0.0, 0.0),
        roughness = 0.2,
        metalness = 0.0,
        clearcoat = 0.0,
        clearcoat_roughness = 0.0,
        sheen = 0.0,
        emissive = vec3(0.0, 0.0, 0.0)
    )
}

shape near_shape {
    field = near_field
    material = shade
    payload = Payload(entity_id = u32(1), material_id = u32(1), actor = ActorHandle(id = u32(1), generation = u32(0)))
}

shape far_custom_shape {
    field = far_custom
    material = shade
    payload = Payload(entity_id = u32(2), material_id = u32(2), actor = ActorHandle(id = u32(2), generation = u32(0)))
}

shape far_semantic_shape {
    field = far_semantic
    material = shade
    payload = Payload(entity_id = u32(3), material_id = u32(3), actor = ActorHandle(id = u32(3), generation = u32(0)))
}

shape supported_scene {
    union {
        provenance_policy = nearest
        use near_shape
        use far_custom_shape
    }
}

shape semantic_scene {
    union {
        provenance_policy = nearest
        use near_shape
        use far_semantic_shape
    }
}

fn main() -> Integer {
    supported = trace_shape(
        capture = capture supported_scene,
        ray = ray_query(
            origin = vec3(0.0, 0.0, 3.0),
            direction = vec3(0.0, 0.0, -1.0),
            max_distance = 6.0,
            min_step = 0.05,
            hit_epsilon = 0.001,
            max_steps = 96
        )
    )
    semantic = trace_shape(
        capture = capture semantic_scene,
        ray = ray_query(
            origin = vec3(0.0, 0.0, 3.0),
            direction = vec3(0.0, 0.0, -1.0),
            max_distance = 6.0,
            min_step = 0.05,
            hit_epsilon = 0.001,
            max_steps = 96
        )
    )
    if supported.hit && semantic.hit {
        return 0
    }
    return 1
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = hir_lower::lower(root);
        let (_type_errors, type_info) = typeck::check_module_with_info(&module);
        let check_ir = crate::hir::checkir::extract_module(&module);
        let mut mir_module = lower_module_with_types(&module, &type_info);
        let analysis = crate::mir::analysis::analyze_module(&mir_module);
        for func in &mut mir_module.functions {
            let types = analysis.type_map.function(&func.name);
            crate::mir::opt::run_function_passes_with_types(func, types);
        }
        let _ = crate::mir::opt::run_module_passes_with_rulepack(&mut mir_module, Some(&check_ir));

        let function_names = mir_module
            .functions
            .iter()
            .map(|func| func.name.clone())
            .collect::<Vec<_>>();
        let metric_callers = mir_module
            .functions
            .iter()
            .filter_map(|func| {
                direct_call_targets(func)
                    .contains("__wr_metrics_scene_trace_support_pruned_branch")
                    .then_some(func.name.clone())
            })
            .collect::<Vec<_>>();

        assert!(
            !metric_callers
                .iter()
                .any(|name| name.contains("supported_scene")),
            "optimized MIR still prunes opaque scene branches: callers={metric_callers:?} functions={function_names:?}"
        );
        assert!(
            metric_callers
                .iter()
                .any(|name| name.contains("semantic_scene")),
            "optimized MIR lost semantic support pruning: callers={metric_callers:?} functions={function_names:?}"
        );
    }

    #[test]
    fn test_shape_queries_with_semantic_wrappers_lower_without_indirect_calls() {
        let input = r#"field conservative distance translated_sphere(p: Vec3) -> F32 {
    translate = vec3(0.8, 0.0, 0.0) {
        sphere(radius=0.65)
    }
}

material orb_surface(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(32.0, 64.0, 255.0),
        roughness=0.2,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape orb_shape {
    field = translated_sphere
    material = orb_surface
    payload = Payload(
        entity_id=u32(2),
        material_id=u32(22),
        actor=ActorHandle(id=u32(202), generation=u32(0))
    )
}

fn run() -> Nothing {
    scene = capture orb_shape
    hit = trace_shape(
        capture=scene,
        ray=ray_query(
            origin=vec3(0.8, 0.0, 3.0),
            direction=vec3(0.0, 0.0, -1.0),
            max_distance=6.0,
            min_step=0.05,
            hit_epsilon=0.001,
            max_steps=96
        )
    )
    surface = surface_at(capture=scene, hit=hit)
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = hir_lower::lower(root);
        let (_type_errors, type_info) = typeck::check_module_with_info(&module);
        let mir_module = lower_module_with_types(&module, &type_info);

        let mut indirect_calls = Vec::new();
        for func in &mir_module.functions {
            for block in &func.blocks {
                for stmt in &block.stmts {
                    if let MirStmt::Assign {
                        value: Rvalue::Call { target, .. },
                        ..
                    } = stmt
                        && matches!(target, CallTarget::Indirect(_))
                    {
                        indirect_calls.push((func.name.clone(), target.clone()));
                    }
                }
            }
        }

        assert!(
            indirect_calls.is_empty(),
            "unexpected indirect calls: {indirect_calls:?}"
        );
    }

    #[test]
    fn test_shape_queries_with_semantic_wrappers_stay_direct_after_opt() {
        let input = r#"field conservative distance translated_sphere(p: Vec3) -> F32 {
    translate = vec3(0.8, 0.0, 0.0) {
        sphere(radius=0.65)
    }
}

material orb_surface(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(32.0, 64.0, 255.0),
        roughness=0.2,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape orb_shape {
    field = translated_sphere
    material = orb_surface
    payload = Payload(
        entity_id=u32(2),
        material_id=u32(22),
        actor=ActorHandle(id=u32(202), generation=u32(0))
    )
}

fn run() -> Nothing {
    scene = capture orb_shape
    hit = trace_shape(
        capture=scene,
        ray=ray_query(
            origin=vec3(0.8, 0.0, 3.0),
            direction=vec3(0.0, 0.0, -1.0),
            max_distance=6.0,
            min_step=0.05,
            hit_epsilon=0.001,
            max_steps=96
        )
    )
    surface = surface_at(capture=scene, hit=hit)
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = hir_lower::lower(root);
        let (_type_errors, type_info) = typeck::check_module_with_info(&module);
        let check_ir = crate::hir::checkir::extract_module(&module);
        let mut mir_module = lower_module_with_types(&module, &type_info);
        let analysis = crate::mir::analysis::analyze_module(&mir_module);
        for func in &mut mir_module.functions {
            let types = analysis.type_map.function(&func.name);
            crate::mir::opt::run_function_passes_with_types(func, types);
        }
        let _ = crate::mir::opt::run_module_passes_with_rulepack(&mut mir_module, Some(&check_ir));

        let mut indirect_calls = Vec::new();
        for func in &mir_module.functions {
            for block in &func.blocks {
                for stmt in &block.stmts {
                    if let MirStmt::Assign {
                        value: Rvalue::Call { target, .. },
                        ..
                    } = stmt
                        && matches!(target, CallTarget::Indirect(_))
                    {
                        indirect_calls.push((func.name.clone(), target.clone()));
                    }
                }
            }
        }

        assert!(
            indirect_calls.is_empty(),
            "unexpected indirect calls after optimization: {indirect_calls:?}"
        );
    }

    #[test]
    fn test_trace_metrics_shape_query_path_stays_direct_after_opt() {
        let input = r#"field exact distance orb(p: Vec3) -> F32 {
    translate = vec3(0.8, 0.0, 0.0) {
        sphere(radius=0.65)
    }
}

material orb_surface(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(32.0, 64.0, 255.0),
        roughness=0.2,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape orb_shape {
    field = orb
    material = orb_surface
    payload = Payload(
        entity_id=u32(2),
        material_id=u32(22),
        actor=ActorHandle(id=u32(202), generation=u32(0))
    )
}

fn run() -> Nothing {
    orb_scene = capture orb_shape
    exact_before = __wr_metrics_get(__wr_metrics_scene_trace_exact_path_id())
    hit = trace_shape(
        capture=orb_scene,
        ray=ray_query(
            origin=vec3(0.8, 0.0, 3.0),
            direction=vec3(0.0, 0.0, -1.0),
            max_distance=6.0,
            min_step=0.05,
            hit_epsilon=0.001,
            max_steps=96
        )
    )
    field_samples_after_trace = __wr_metrics_get(__wr_metrics_field_sample_id())
    surface = surface_at(capture=orb_scene, hit=hit)
    exact_after = __wr_metrics_get(__wr_metrics_scene_trace_exact_path_id())
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = hir_lower::lower(root);
        let (_type_errors, type_info) = typeck::check_module_with_info(&module);
        let check_ir = crate::hir::checkir::extract_module(&module);
        let mut mir_module = lower_module_with_types(&module, &type_info);
        let analysis = crate::mir::analysis::analyze_module(&mir_module);
        for func in &mut mir_module.functions {
            let types = analysis.type_map.function(&func.name);
            crate::mir::opt::run_function_passes_with_types(func, types);
        }
        let _ = crate::mir::opt::run_module_passes_with_rulepack(&mut mir_module, Some(&check_ir));

        let mut indirect_calls = Vec::new();
        for func in &mir_module.functions {
            for block in &func.blocks {
                for stmt in &block.stmts {
                    if let MirStmt::Assign {
                        value: Rvalue::Call { target, .. },
                        ..
                    } = stmt
                        && matches!(target, CallTarget::Indirect(_))
                    {
                        indirect_calls.push((func.name.clone(), target.clone()));
                    }
                }
            }
        }

        assert!(
            indirect_calls.is_empty(),
            "unexpected indirect calls after optimization: {indirect_calls:?}"
        );
    }

    #[test]
    fn test_trace_metrics_assertions_stay_direct_after_opt() {
        let input = r#"field exact distance orb(p: Vec3) -> F32 {
    translate = vec3(0.8, 0.0, 0.0) {
        sphere(radius=0.65)
    }
}

material orb_surface(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(32.0, 64.0, 255.0),
        roughness=0.2,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape orb_shape {
    field = orb
    material = orb_surface
    payload = Payload(
        entity_id=u32(2),
        material_id=u32(22),
        actor=ActorHandle(id=u32(202), generation=u32(0))
    )
}

fn run() -> Nothing {
    orb_scene = capture orb_shape
    exact_before = __wr_metrics_get(__wr_metrics_scene_trace_exact_path_id())
    conservative_before = __wr_metrics_get(__wr_metrics_scene_trace_conservative_path_id())
    hit_count_before = __wr_metrics_get(__wr_metrics_scene_trace_hit_count_id())
    hit_steps_before = __wr_metrics_get(__wr_metrics_scene_trace_hit_steps_total_id())
    hit_samples_before = __wr_metrics_get(__wr_metrics_scene_trace_hit_field_samples_total_id())
    field_samples_before = __wr_metrics_get(__wr_metrics_field_sample_id())
    bucket_before = __wr_metrics_get(__wr_metrics_scene_trace_steps_le_1_id())
        + __wr_metrics_get(__wr_metrics_scene_trace_steps_le_4_id())
        + __wr_metrics_get(__wr_metrics_scene_trace_steps_le_8_id())
        + __wr_metrics_get(__wr_metrics_scene_trace_steps_le_16_id())
        + __wr_metrics_get(__wr_metrics_scene_trace_steps_gt_16_id())

    hit = trace_shape(
        capture=orb_scene,
        ray=ray_query(
            origin=vec3(0.8, 0.0, 3.0),
            direction=vec3(0.0, 0.0, -1.0),
            max_distance=6.0,
            min_step=0.05,
            hit_epsilon=0.001,
            max_steps=96
        )
    )
    field_samples_after_trace = __wr_metrics_get(__wr_metrics_field_sample_id())
    surface = surface_at(capture=orb_scene, hit=hit)

    exact_after = __wr_metrics_get(__wr_metrics_scene_trace_exact_path_id())
    conservative_after = __wr_metrics_get(__wr_metrics_scene_trace_conservative_path_id())
    hit_count_after = __wr_metrics_get(__wr_metrics_scene_trace_hit_count_id())
    hit_steps_after = __wr_metrics_get(__wr_metrics_scene_trace_hit_steps_total_id())
    hit_samples_after = __wr_metrics_get(__wr_metrics_scene_trace_hit_field_samples_total_id())
    field_samples_after = __wr_metrics_get(__wr_metrics_field_sample_id())
    bucket_after = __wr_metrics_get(__wr_metrics_scene_trace_steps_le_1_id())
        + __wr_metrics_get(__wr_metrics_scene_trace_steps_le_4_id())
        + __wr_metrics_get(__wr_metrics_scene_trace_steps_le_8_id())
        + __wr_metrics_get(__wr_metrics_scene_trace_steps_le_16_id())
        + __wr_metrics_get(__wr_metrics_scene_trace_steps_gt_16_id())

    assert value hit.hit == true
    assert value hit.payload.material_id == u32(22)
    assert approx surface.albedo.x ~= 32.0 within 0.001
    assert approx surface.albedo.z ~= 255.0 within 0.001
    assert value exact_after - exact_before == 1
    assert value conservative_after - conservative_before == 0
    assert value hit_count_after - hit_count_before == 1
    assert value hit_steps_after - hit_steps_before == hit.steps
    assert value hit_samples_after - hit_samples_before == field_samples_after_trace - field_samples_before
    assert value bucket_after - bucket_before == 1
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = hir_lower::lower(root);
        let (_type_errors, type_info) = typeck::check_module_with_info(&module);
        let check_ir = crate::hir::checkir::extract_module(&module);
        let mut mir_module = lower_module_with_types(&module, &type_info);
        let analysis = crate::mir::analysis::analyze_module(&mir_module);
        for func in &mut mir_module.functions {
            let types = analysis.type_map.function(&func.name);
            crate::mir::opt::run_function_passes_with_types(func, types);
        }
        let _ = crate::mir::opt::run_module_passes_with_rulepack(&mut mir_module, Some(&check_ir));

        let mut indirect_calls = Vec::new();
        for func in &mir_module.functions {
            for block in &func.blocks {
                for stmt in &block.stmts {
                    if let MirStmt::Assign {
                        value: Rvalue::Call { target, .. },
                        ..
                    } = stmt
                        && matches!(target, CallTarget::Indirect(_))
                    {
                        indirect_calls.push((func.name.clone(), target.clone()));
                    }
                }
            }
        }

        assert!(
            indirect_calls.is_empty(),
            "unexpected indirect calls after optimization: {indirect_calls:?}"
        );
    }

    #[test]
    fn test_lower_integer_range_for_uses_typed_induction_fast_path() {
        let input = "\
fn run() -> Integer {
    start = 1
    stop = 4
    mutable total = 0
    for i in start...stop {
        total += i
    }
    return total
}
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = hir_lower::lower(root);
        let (_type_errors, type_info) = typeck::check_module_with_info(&module);
        let mir_module = lower_module_with_types(&module, &type_info);
        let func = mir_module
            .functions
            .iter()
            .find(|func| func.name == "run")
            .expect("missing run");

        assert!(
            func.locals
                .iter()
                .any(|local| local.name.as_str() == "i" && local.ty == MirType::Integer),
            "expected typed loop variable for integer range",
        );
        assert!(
            func.locals
                .iter()
                .any(|local| local.name.starts_with("$range_idx") && local.ty == MirType::Integer),
            "expected typed integer induction local",
        );
        assert!(
            func.locals
                .iter()
                .any(|local| local.name.starts_with("$range_step") && local.ty == MirType::Integer),
            "expected typed integer step local",
        );

        for block in &func.blocks {
            for stmt in &block.stmts {
                assert!(
                    !matches!(stmt, MirStmt::IterInit { .. } | MirStmt::IterNext { .. }),
                    "typed integer range loop should not use iterator protocol",
                );
                if let MirStmt::Assign { value, .. } = stmt {
                    assert!(
                        !matches!(
                            value,
                            Rvalue::Binary {
                                op: crate::hir::BinaryOp::Range,
                                ..
                            }
                        ),
                        "typed integer range loop should not materialize range object",
                    );
                }
            }
        }
    }

    #[test]
    fn test_lower_member_field_ops_emit_slot_hints() {
        let input = "\
class Counter {
    mutable value: Integer
    mutable other: Integer

    fn bump() -> Nothing {
        self.value += 1
        self.other = 4
    }
}

fn run() -> Integer {
    c = Counter(value=1, other=2)
    c.value += 3
    return c.other
}
";
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = hir_lower::lower(root);
        let (_type_errors, type_info) = typeck::check_module_with_info(&module);
        let mir_module = lower_module_with_types(&module, &type_info);

        let mut saw_get_value_slot = false;
        let mut saw_get_other_slot = false;
        let mut saw_set_value_slot = false;
        let mut saw_set_other_slot = false;

        for func in &mir_module.functions {
            for block in &func.blocks {
                for stmt in &block.stmts {
                    match stmt {
                        MirStmt::Assign {
                            value:
                                Rvalue::GetField {
                                    field,
                                    slot: Some(slot),
                                    ..
                                },
                            ..
                        } if field.as_str() == "value" && *slot == 0 => saw_get_value_slot = true,
                        MirStmt::Assign {
                            value:
                                Rvalue::GetField {
                                    field,
                                    slot: Some(slot),
                                    ..
                                },
                            ..
                        } if field.as_str() == "other" && *slot == 1 => saw_get_other_slot = true,
                        MirStmt::SetField {
                            field,
                            slot: Some(slot),
                            ..
                        } if field.as_str() == "value" && *slot == 0 => saw_set_value_slot = true,
                        MirStmt::SetField {
                            field,
                            slot: Some(slot),
                            ..
                        } if field.as_str() == "other" && *slot == 1 => saw_set_other_slot = true,
                        _ => {}
                    }
                }
            }
        }

        assert!(saw_get_value_slot, "expected slot-hinted get for value");
        assert!(saw_get_other_slot, "expected slot-hinted get for other");
        assert!(saw_set_value_slot, "expected slot-hinted set for value");
        assert!(saw_set_other_slot, "expected slot-hinted set for other");
    }
}
