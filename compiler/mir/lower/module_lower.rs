//! Owns whole-module MIR lowering orchestration and symbol/helper synthesis.
//! Does not own HIR parsing/typechecking or the execution of lowered MIR.
//!
//! Key invariants:
//! - module-wide symbol/type inventories must be complete before per-function
//!   lowering starts.
//! - helper synthesis and canonical naming here define the namespace other
//!   lowering files consume.
//!
//! Primary entrypoints:
//! - `lower_module`
//! - `lower_module_with_types_and_backend`
//!
//! Failure modes / common pitfalls:
//! - changing module-level naming or helper synthesis here without updating
//!   consumers creates repo-wide drift that looks like many small bugs.

use super::function_entry::lower_function;
use super::interface_dispatch::{build_interface_dispatch_functions, builtin_function_names};
use super::render_helpers::{
    lower_render_ambient_occlusion_helper, lower_render_capture_to_ppm_helper,
    lower_render_scene_color_helper, lower_render_shadow_visibility_helper,
};
use super::*;

pub(super) fn is_syntactic_stringish(body: &hir::Body, expr: hir::Idx<hir::Expr>) -> bool {
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

pub(super) fn kernel_world_query_input_count(plan: &crate::kernel::KernelWorldQueryPlan) -> usize {
    let descriptor = query_contract::query_contract(plan.contract_id)
        .expect("kernel world query plan must reference a registered query contract");
    let capture_count = 1;
    let domain_count = usize::from(descriptor.domain_contract.is_some());
    let item_count = match descriptor.item_kind {
        QueryItemKind::Unit => 0,
        QueryItemKind::PointQuery
        | QueryItemKind::PointDirectionQuery
        | QueryItemKind::RayQuery
        | QueryItemKind::Hit3 => 1,
    };
    capture_count + domain_count + item_count
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
    // Invariant: build the global lowered-function namespace only after method
    // qnames are finalized. Query/helper synthesis resolves through this set and
    // must see the canonical lowered symbol, not the authored leaf name.
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
    let world_batch_plans = vec![
        BatchQueryPlan::for_world_query(BatchQueryKind::Distance, DispatchBackend::Auto),
        BatchQueryPlan::for_world_query(BatchQueryKind::Normal, DispatchBackend::Auto),
        BatchQueryPlan::for_world_query(BatchQueryKind::Nearest, DispatchBackend::Auto),
        BatchQueryPlan::for_world_query(BatchQueryKind::Occluded, DispatchBackend::Auto),
        BatchQueryPlan::for_world_query(BatchQueryKind::Surface, DispatchBackend::Auto),
        BatchQueryPlan::for_world_query(BatchQueryKind::Radiance, DispatchBackend::Auto),
        BatchQueryPlan::for_world_query(BatchQueryKind::Medium, DispatchBackend::Auto),
    ];
    let batch_wgsl_configs = wgsl_query_ctx
        .as_ref()
        .map(|ctx| {
            field_batch_plans
                .iter()
                .chain(shape_batch_plans.iter())
                .chain(world_batch_plans.iter())
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
    functions.push(lower_scene_support_summary_capture_helper(
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
        "__wr_field_support_summary_capture",
    ));
    functions.push(lower_scene_support_summary_capture_helper(
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
        "__wr_shape_support_summary_capture",
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
    functions.push(lower_scene_occluded_capture_helper(
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
    functions.push(lower_world_support_summary_capture_helper(
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
    functions.push(lower_world_occluded_capture_helper(
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
    for plan in &world_batch_plans {
        functions.push(lower_world_batch_queries_helper(
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
