//! Owns MIR lowering for world capture helpers across distance, trace, surface,
//! radiance, normal, occlusion, and medium queries.
//! Does not own scene capture helpers or batch-query lowering.
//!
//! Key invariants:
//! - lowered world helpers preserve backend selection/guard semantics from the
//!   chosen query contract.
//! - world domain validation must happen before bridge/native execution so
//!   runtime evidence matches authored world scope.
//! - helper-specific result packing stays aligned with the stable world capture
//!   ids used by codegen and reports.
//!
//! Primary entrypoints:
//! - `lower_world_distance_capture_helper`
//! - `lower_world_trace_capture_helper`
//! - `lower_world_medium_capture_helper`
//!
//! Failure modes / common pitfalls:
//! - letting one world helper bypass the shared guard/dispatch helpers can make
//!   backend behavior diverge by query kind.
//! - mixing scene capture assumptions into this file weakens the module split and
//!   makes future backend work harder to reason about.

use super::scene_medium_capture_lowering::{
    MirWorldDistanceBackend, MirWorldMediumBackend, MirWorldNormalBackend, MirWorldRadianceBackend,
    MirWorldSurfaceBackend, MirWorldTraceBackend, lower_native_world_backend_guard,
    lower_wgsl_bridge_failure, lower_world_domain_flag_guard, lower_world_domain_validation,
    lower_world_region_dispatch, lower_world_wgsl_bridge_call,
};
use super::{
    DispatchBackend, FunctionLowerer, HashMap, HashSet, Literal, MirFunction, MirType,
    NativeWgslBridgeConfig, PortableAbiType, SmolStr, TextRange, TypeTagId, Value, WorldQueryKind,
    execute_world_normal, portable_abi_from_type_ref, walk_world_distance_shapes,
    walk_world_medium_shapes, walk_world_radiance_shapes, walk_world_surface_shapes,
    walk_world_trace_shapes, world_query_semantics,
};
use crate::hir;
use crate::mir::ir::*;

pub(crate) fn lower_world_distance_capture_helper(
    module: &hir::Module,
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
    auto_backend: DispatchBackend,
    wgsl_config: Option<&Result<NativeWgslBridgeConfig, SmolStr>>,
    wgsl_shape_indices: &HashMap<SmolStr, u32>,
) -> MirFunction {
    let plan = crate::query_plan::WorldQueryPlan::for_query(WorldQueryKind::Distance);
    let helper_name = plan.helper_name.clone();
    let semantics = world_query_semantics(WorldQueryKind::Distance);
    let span = TextRange::empty(0.into());
    let mut lowerer = FunctionLowerer::new(
        helper_name.clone(),
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

    let capture = lowerer.new_local(
        SmolStr::new("capture"),
        false,
        MirType::Named(SmolStr::new("RegionCapture")),
    );
    let domain = lowerer.new_local(
        SmolStr::new("domain"),
        false,
        MirType::Named(SmolStr::new("SceneDomain")),
    );
    let point = lowerer.new_local(SmolStr::new("point"), false, MirType::Vec3);
    let backend = lowerer.new_local(SmolStr::new("backend"), false, MirType::Integer);
    for (name, local) in [
        (SmolStr::new("capture"), capture),
        (SmolStr::new("domain"), domain),
        (SmolStr::new("point"), point),
        (SmolStr::new("backend"), backend),
    ] {
        lowerer.declare_local(name, local);
        lowerer.params.push(local);
    }

    let entry = lowerer.new_block();
    let cpu_block = lowerer.new_block();
    let wgsl_block = lowerer.new_block();
    let unsupported_block = lowerer.new_block();
    lowerer.current_block = entry;
    let (capture_scene_id, detail) =
        lower_world_domain_validation(&mut lowerer, capture, domain, semantics.query_name, span);
    let result = lowerer.new_local(SmolStr::new("$world_distance_result"), true, MirType::Float);
    lowerer.assign_use(
        Place::Local(result),
        Value::Const(Literal::Float(1_000_000.0)),
        span,
    );
    let return_block = lowerer.new_block();
    lower_native_world_backend_guard(
        &mut lowerer,
        backend,
        auto_backend,
        cpu_block,
        wgsl_block,
        unsupported_block,
        span,
    );

    lowerer.current_block = cpu_block;
    lower_world_region_dispatch(
        &mut lowerer,
        module,
        capture_scene_id.clone(),
        detail.clone(),
        return_block,
        "distance_world requires a capture created from a region declaration",
        span,
        |lowerer, shapes, span| {
            let mut backend = MirWorldDistanceBackend {
                lowerer,
                point: Value::Local(point),
                result,
                span,
            };
            walk_world_distance_shapes(&mut backend, shapes).expect("mir world distance walk");
        },
    );

    lowerer.current_block = wgsl_block;
    if let Some(Ok(config)) = wgsl_config {
        lower_world_region_dispatch(
            &mut lowerer,
            module,
            capture_scene_id,
            detail,
            return_block,
            "distance_world requires a capture created from a region declaration",
            span,
            |lowerer, shapes, span| {
                let value = lower_world_wgsl_bridge_call(
                    lowerer,
                    MirType::Float,
                    Some(&Ok(config.clone())),
                    "__wr_wgsl_world_distance_capture",
                    shapes,
                    wgsl_shape_indices,
                    vec![Value::Local(point)],
                    span,
                )
                .expect("validated WGSL world distance config");
                lowerer.assign_use(Place::Local(result), value, span);
            },
        );
    } else if let Some(Err(err)) = wgsl_config {
        lower_wgsl_bridge_failure(&mut lowerer, err.clone(), span);
    } else {
        lower_wgsl_bridge_failure(
            &mut lowerer,
            SmolStr::new("missing WGSL bridge config for world distance"),
            span,
        );
    }

    lowerer.current_block = return_block;
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Local(result)),
        span,
    });

    MirFunction {
        name: helper_name,
        params: lowerer.params,
        abi_params: vec![
            portable_abi_from_type_ref(
                Some(&hir::TypeRef {
                    name: SmolStr::new("RegionCapture"),
                    name_span: None,
                    args: Vec::new(),
                }),
                module,
                type_tags,
                &mut HashSet::new(),
            ),
            portable_abi_from_type_ref(
                Some(&hir::TypeRef {
                    name: SmolStr::new("SceneDomain"),
                    name_span: None,
                    args: Vec::new(),
                }),
                module,
                type_tags,
                &mut HashSet::new(),
            ),
            PortableAbiType::Vec3,
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

pub(crate) fn lower_world_normal_capture_helper(
    module: &hir::Module,
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
    auto_backend: DispatchBackend,
    wgsl_config: Option<&Result<NativeWgslBridgeConfig, SmolStr>>,
    wgsl_shape_indices: &HashMap<SmolStr, u32>,
) -> MirFunction {
    let plan = crate::query_plan::WorldQueryPlan::for_query(WorldQueryKind::Normal);
    let helper_name = plan.helper_name.clone();
    let semantics = world_query_semantics(WorldQueryKind::Normal);
    let span = TextRange::empty(0.into());
    let mut lowerer = FunctionLowerer::new(
        helper_name.clone(),
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

    let capture = lowerer.new_local(
        SmolStr::new("capture"),
        false,
        MirType::Named(SmolStr::new("RegionCapture")),
    );
    let domain = lowerer.new_local(
        SmolStr::new("domain"),
        false,
        MirType::Named(SmolStr::new("SceneDomain")),
    );
    let point = lowerer.new_local(SmolStr::new("point"), false, MirType::Vec3);
    let backend = lowerer.new_local(SmolStr::new("backend"), false, MirType::Integer);
    for (name, local) in [
        (SmolStr::new("capture"), capture),
        (SmolStr::new("domain"), domain),
        (SmolStr::new("point"), point),
        (SmolStr::new("backend"), backend),
    ] {
        lowerer.declare_local(name, local);
        lowerer.params.push(local);
    }

    let entry = lowerer.new_block();
    let cpu_block = lowerer.new_block();
    let wgsl_block = lowerer.new_block();
    let unsupported_block = lowerer.new_block();
    lowerer.current_block = entry;
    let (capture_scene_id, detail) =
        lower_world_domain_validation(&mut lowerer, capture, domain, semantics.query_name, span);
    let result = lowerer.new_local(SmolStr::new("$world_normal_result"), true, MirType::Vec3);
    lower_native_world_backend_guard(
        &mut lowerer,
        backend,
        auto_backend,
        cpu_block,
        wgsl_block,
        unsupported_block,
        span,
    );

    lowerer.current_block = cpu_block;
    let mut backend = MirWorldNormalBackend {
        lowerer: &mut lowerer,
        capture,
        domain,
        point,
        backend,
        span,
    };
    let normal = execute_world_normal(&mut backend).expect("mir world normal");
    lowerer.assign_use(Place::Local(result), normal, span);
    let return_block = lowerer.new_block();
    lowerer.set_terminator(Terminator::Jump {
        target: return_block,
        span,
    });

    lowerer.current_block = wgsl_block;
    if let Some(Ok(config)) = wgsl_config {
        lower_world_region_dispatch(
            &mut lowerer,
            module,
            capture_scene_id,
            detail,
            return_block,
            "normal_world requires a capture created from a region declaration",
            span,
            |lowerer, shapes, span| {
                let value = lower_world_wgsl_bridge_call(
                    lowerer,
                    MirType::Vec3,
                    Some(&Ok(config.clone())),
                    "__wr_wgsl_world_normal_capture",
                    shapes,
                    wgsl_shape_indices,
                    vec![Value::Local(point)],
                    span,
                )
                .expect("validated WGSL world normal config");
                lowerer.assign_use(Place::Local(result), value, span);
            },
        );
    } else if let Some(Err(err)) = wgsl_config {
        lower_wgsl_bridge_failure(&mut lowerer, err.clone(), span);
    } else {
        lower_wgsl_bridge_failure(
            &mut lowerer,
            SmolStr::new("missing WGSL bridge config for world normal"),
            span,
        );
    }

    lowerer.current_block = return_block;
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Local(result)),
        span,
    });

    MirFunction {
        name: helper_name,
        params: lowerer.params,
        abi_params: vec![
            portable_abi_from_type_ref(
                Some(&hir::TypeRef {
                    name: SmolStr::new("RegionCapture"),
                    name_span: None,
                    args: Vec::new(),
                }),
                module,
                type_tags,
                &mut HashSet::new(),
            ),
            portable_abi_from_type_ref(
                Some(&hir::TypeRef {
                    name: SmolStr::new("SceneDomain"),
                    name_span: None,
                    args: Vec::new(),
                }),
                module,
                type_tags,
                &mut HashSet::new(),
            ),
            PortableAbiType::Vec3,
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

pub(crate) fn lower_world_trace_capture_helper(
    module: &hir::Module,
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
    auto_backend: DispatchBackend,
    wgsl_config: Option<&Result<NativeWgslBridgeConfig, SmolStr>>,
    wgsl_shape_indices: &HashMap<SmolStr, u32>,
) -> MirFunction {
    let plan = crate::query_plan::WorldQueryPlan::for_query(WorldQueryKind::Trace);
    let helper_name = plan.helper_name.clone();
    let semantics = world_query_semantics(WorldQueryKind::Trace);
    let span = TextRange::empty(0.into());
    let mut lowerer = FunctionLowerer::new(
        helper_name.clone(),
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

    let capture = lowerer.new_local(
        SmolStr::new("capture"),
        false,
        MirType::Named(SmolStr::new("RegionCapture")),
    );
    let domain = lowerer.new_local(
        SmolStr::new("domain"),
        false,
        MirType::Named(SmolStr::new("SceneDomain")),
    );
    let ray = lowerer.new_local(
        SmolStr::new("ray"),
        false,
        MirType::Named(SmolStr::new("RayQuery")),
    );
    let backend = lowerer.new_local(SmolStr::new("backend"), false, MirType::Integer);
    for (name, local) in [
        (SmolStr::new("capture"), capture),
        (SmolStr::new("domain"), domain),
        (SmolStr::new("ray"), ray),
        (SmolStr::new("backend"), backend),
    ] {
        lowerer.declare_local(name, local);
        lowerer.params.push(local);
    }

    let entry = lowerer.new_block();
    let cpu_block = lowerer.new_block();
    let wgsl_block = lowerer.new_block();
    let unsupported_block = lowerer.new_block();
    lowerer.current_block = entry;
    let (capture_scene_id, detail) =
        lower_world_domain_validation(&mut lowerer, capture, domain, semantics.query_name, span);
    let origin =
        lowerer.lower_get_named_field(Value::Local(ray), "RayQuery", "origin", MirType::Vec3, span);
    let direction = lowerer.lower_get_named_field(
        Value::Local(ray),
        "RayQuery",
        "direction",
        MirType::Vec3,
        span,
    );
    let max_distance = lowerer.lower_get_named_field(
        Value::Local(ray),
        "RayQuery",
        "max_distance",
        MirType::Float,
        span,
    );
    let min_step = lowerer.lower_get_named_field(
        Value::Local(ray),
        "RayQuery",
        "min_step",
        MirType::Float,
        span,
    );
    let hit_epsilon = lowerer.lower_get_named_field(
        Value::Local(ray),
        "RayQuery",
        "hit_epsilon",
        MirType::Float,
        span,
    );
    let max_steps = lowerer.lower_get_named_field(
        Value::Local(ray),
        "RayQuery",
        "max_steps",
        MirType::Integer,
        span,
    );
    let result = lowerer.new_local(
        SmolStr::new("$world_trace_result"),
        true,
        MirType::Named(SmolStr::new("Hit3")),
    );
    let default_hit = lowerer.build_default_hit(origin.clone(), span);
    lowerer.assign_use(Place::Local(result), default_hit, span);
    let return_block = lowerer.new_block();
    lower_native_world_backend_guard(
        &mut lowerer,
        backend,
        auto_backend,
        cpu_block,
        wgsl_block,
        unsupported_block,
        span,
    );

    lowerer.current_block = cpu_block;
    lower_world_region_dispatch(
        &mut lowerer,
        module,
        capture_scene_id.clone(),
        detail.clone(),
        return_block,
        "trace_world requires a capture created from a region declaration",
        span,
        |lowerer, shapes, span| {
            let mut backend = MirWorldTraceBackend {
                lowerer,
                origin: origin.clone(),
                direction: direction.clone(),
                max_distance: max_distance.clone(),
                min_step: min_step.clone(),
                hit_epsilon: hit_epsilon.clone(),
                max_steps: max_steps.clone(),
                result,
                span,
            };
            walk_world_trace_shapes(&mut backend, shapes).expect("mir world trace walk");
        },
    );

    lowerer.current_block = wgsl_block;
    if let Some(Ok(config)) = wgsl_config {
        lower_world_region_dispatch(
            &mut lowerer,
            module,
            capture_scene_id,
            detail,
            return_block,
            "trace_world requires a capture created from a region declaration",
            span,
            |lowerer, shapes, span| {
                let value = lower_world_wgsl_bridge_call(
                    lowerer,
                    MirType::Named(SmolStr::new("Hit3")),
                    Some(&Ok(config.clone())),
                    "__wr_wgsl_world_trace_capture",
                    shapes,
                    wgsl_shape_indices,
                    vec![Value::Local(ray)],
                    span,
                )
                .expect("validated WGSL world trace config");
                lowerer.assign_use(Place::Local(result), value, span);
            },
        );
    } else if let Some(Err(err)) = wgsl_config {
        lower_wgsl_bridge_failure(&mut lowerer, err.clone(), span);
    } else {
        lower_wgsl_bridge_failure(
            &mut lowerer,
            SmolStr::new("missing WGSL bridge config for world trace"),
            span,
        );
    }

    lowerer.current_block = return_block;
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Local(result)),
        span,
    });

    MirFunction {
        name: helper_name,
        params: lowerer.params,
        abi_params: vec![
            portable_abi_from_type_ref(
                Some(&hir::TypeRef {
                    name: SmolStr::new("RegionCapture"),
                    name_span: None,
                    args: Vec::new(),
                }),
                module,
                type_tags,
                &mut HashSet::new(),
            ),
            portable_abi_from_type_ref(
                Some(&hir::TypeRef {
                    name: SmolStr::new("SceneDomain"),
                    name_span: None,
                    args: Vec::new(),
                }),
                module,
                type_tags,
                &mut HashSet::new(),
            ),
            portable_abi_from_type_ref(
                Some(&hir::TypeRef {
                    name: SmolStr::new("RayQuery"),
                    name_span: None,
                    args: Vec::new(),
                }),
                module,
                type_tags,
                &mut HashSet::new(),
            ),
            PortableAbiType::I32,
        ],
        abi_return: portable_abi_from_type_ref(
            Some(&hir::TypeRef {
                name: SmolStr::new("Hit3"),
                name_span: None,
                args: Vec::new(),
            }),
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

pub(crate) fn lower_world_occluded_capture_helper(
    module: &hir::Module,
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
    let plan = crate::query_plan::WorldQueryPlan::for_query(WorldQueryKind::Occluded);
    let helper_name = plan.helper_name.clone();
    let span = TextRange::empty(0.into());
    let mut lowerer = FunctionLowerer::new(
        helper_name.clone(),
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

    let capture = lowerer.new_local(
        SmolStr::new("capture"),
        false,
        MirType::Named(SmolStr::new("RegionCapture")),
    );
    let domain = lowerer.new_local(
        SmolStr::new("domain"),
        false,
        MirType::Named(SmolStr::new("SceneDomain")),
    );
    let ray = lowerer.new_local(
        SmolStr::new("ray"),
        false,
        MirType::Named(SmolStr::new("RayQuery")),
    );
    let backend = lowerer.new_local(SmolStr::new("backend"), false, MirType::Integer);
    for (name, local) in [
        (SmolStr::new("capture"), capture),
        (SmolStr::new("domain"), domain),
        (SmolStr::new("ray"), ray),
        (SmolStr::new("backend"), backend),
    ] {
        lowerer.declare_local(name, local);
        lowerer.params.push(local);
    }

    let entry = lowerer.new_block();
    lowerer.current_block = entry;
    let hit = lowerer.lower_call_temp(
        MirType::Named(SmolStr::new("Hit3")),
        SmolStr::new("__wr_world_trace_capture"),
        vec![
            Value::Local(capture),
            Value::Local(domain),
            Value::Local(ray),
            Value::Local(backend),
        ],
        span,
    );
    let occlusion = lowerer.build_occlusion_result_value(hit, span);
    lowerer.set_terminator(Terminator::Return {
        value: Some(occlusion),
        span,
    });

    MirFunction {
        name: helper_name,
        params: lowerer.params,
        abi_params: vec![
            portable_abi_from_type_ref(
                Some(&hir::TypeRef {
                    name: SmolStr::new("RegionCapture"),
                    name_span: None,
                    args: Vec::new(),
                }),
                module,
                type_tags,
                &mut HashSet::new(),
            ),
            portable_abi_from_type_ref(
                Some(&hir::TypeRef {
                    name: SmolStr::new("SceneDomain"),
                    name_span: None,
                    args: Vec::new(),
                }),
                module,
                type_tags,
                &mut HashSet::new(),
            ),
            portable_abi_from_type_ref(
                Some(&hir::TypeRef {
                    name: SmolStr::new("RayQuery"),
                    name_span: None,
                    args: Vec::new(),
                }),
                module,
                type_tags,
                &mut HashSet::new(),
            ),
            PortableAbiType::I32,
        ],
        abi_return: portable_abi_from_type_ref(
            Some(&hir::TypeRef {
                name: SmolStr::new("OcclusionResult"),
                name_span: None,
                args: Vec::new(),
            }),
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

pub(crate) fn lower_world_surface_capture_helper(
    module: &hir::Module,
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
    auto_backend: DispatchBackend,
    wgsl_config: Option<&Result<NativeWgslBridgeConfig, SmolStr>>,
    wgsl_shape_indices: &HashMap<SmolStr, u32>,
) -> MirFunction {
    let plan = crate::query_plan::WorldQueryPlan::for_query(WorldQueryKind::Surface);
    let helper_name = plan.helper_name.clone();
    let semantics = world_query_semantics(WorldQueryKind::Surface);
    let span = TextRange::empty(0.into());
    let mut lowerer = FunctionLowerer::new(
        helper_name.clone(),
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

    let capture = lowerer.new_local(
        SmolStr::new("capture"),
        false,
        MirType::Named(SmolStr::new("RegionCapture")),
    );
    let domain = lowerer.new_local(
        SmolStr::new("domain"),
        false,
        MirType::Named(SmolStr::new("SceneDomain")),
    );
    let hit = lowerer.new_local(
        SmolStr::new("hit"),
        false,
        MirType::Named(SmolStr::new("Hit3")),
    );
    let backend = lowerer.new_local(SmolStr::new("backend"), false, MirType::Integer);
    for (name, local) in [
        (SmolStr::new("capture"), capture),
        (SmolStr::new("domain"), domain),
        (SmolStr::new("hit"), hit),
        (SmolStr::new("backend"), backend),
    ] {
        lowerer.declare_local(name, local);
        lowerer.params.push(local);
    }

    let entry = lowerer.new_block();
    let cpu_block = lowerer.new_block();
    let wgsl_block = lowerer.new_block();
    let unsupported_block = lowerer.new_block();
    lowerer.current_block = entry;
    let default_surface = lowerer.build_default_surface(span);
    let (capture_scene_id, detail) =
        lower_world_domain_validation(&mut lowerer, capture, domain, semantics.query_name, span);
    lower_world_domain_flag_guard(
        &mut lowerer,
        domain,
        semantics.domain_flag.expect("surface_world flag"),
        default_surface.clone(),
        span,
    );
    let root_shape_id = lowerer.lower_get_named_field(
        Value::Local(hit),
        "Hit3",
        "root_shape_id",
        MirType::Integer,
        span,
    );
    let result = lowerer.new_local(
        SmolStr::new("$world_surface_result"),
        true,
        MirType::Named(SmolStr::new("Surface")),
    );
    let default_surface = lowerer.build_default_surface(span);
    lowerer.assign_use(Place::Local(result), default_surface, span);
    let return_block = lowerer.new_block();
    lower_native_world_backend_guard(
        &mut lowerer,
        backend,
        auto_backend,
        cpu_block,
        wgsl_block,
        unsupported_block,
        span,
    );

    lowerer.current_block = cpu_block;
    lower_world_region_dispatch(
        &mut lowerer,
        module,
        capture_scene_id.clone(),
        detail.clone(),
        return_block,
        "surface_world requires a capture created from a region declaration",
        span,
        |lowerer, shapes, span| {
            let mut backend = MirWorldSurfaceBackend {
                lowerer,
                hit,
                root_shape_id: root_shape_id.clone(),
                result,
                span,
            };
            walk_world_surface_shapes(&mut backend, shapes).expect("mir world surface walk");
        },
    );

    lowerer.current_block = wgsl_block;
    if let Some(Ok(config)) = wgsl_config {
        lower_world_region_dispatch(
            &mut lowerer,
            module,
            capture_scene_id,
            detail,
            return_block,
            "surface_world requires a capture created from a region declaration",
            span,
            |lowerer, shapes, span| {
                let value = lower_world_wgsl_bridge_call(
                    lowerer,
                    MirType::Named(SmolStr::new("Surface")),
                    Some(&Ok(config.clone())),
                    "__wr_wgsl_world_surface_capture",
                    shapes,
                    wgsl_shape_indices,
                    vec![Value::Local(hit)],
                    span,
                )
                .expect("validated WGSL world surface config");
                lowerer.assign_use(Place::Local(result), value, span);
            },
        );
    } else if let Some(Err(err)) = wgsl_config {
        lower_wgsl_bridge_failure(&mut lowerer, err.clone(), span);
    } else {
        lower_wgsl_bridge_failure(
            &mut lowerer,
            SmolStr::new("missing WGSL bridge config for world surface"),
            span,
        );
    }

    lowerer.current_block = return_block;
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Local(result)),
        span,
    });

    MirFunction {
        name: helper_name,
        params: lowerer.params,
        abi_params: vec![
            portable_abi_from_type_ref(
                Some(&hir::TypeRef {
                    name: SmolStr::new("RegionCapture"),
                    name_span: None,
                    args: Vec::new(),
                }),
                module,
                type_tags,
                &mut HashSet::new(),
            ),
            portable_abi_from_type_ref(
                Some(&hir::TypeRef {
                    name: SmolStr::new("SceneDomain"),
                    name_span: None,
                    args: Vec::new(),
                }),
                module,
                type_tags,
                &mut HashSet::new(),
            ),
            portable_abi_from_type_ref(
                Some(&hir::TypeRef {
                    name: SmolStr::new("Hit3"),
                    name_span: None,
                    args: Vec::new(),
                }),
                module,
                type_tags,
                &mut HashSet::new(),
            ),
            PortableAbiType::I32,
        ],
        abi_return: portable_abi_from_type_ref(
            Some(&hir::TypeRef {
                name: SmolStr::new("Surface"),
                name_span: None,
                args: Vec::new(),
            }),
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

pub(crate) fn lower_world_radiance_capture_helper(
    module: &hir::Module,
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
    auto_backend: DispatchBackend,
    wgsl_config: Option<&Result<NativeWgslBridgeConfig, SmolStr>>,
    wgsl_shape_indices: &HashMap<SmolStr, u32>,
) -> MirFunction {
    let plan = crate::query_plan::WorldQueryPlan::for_query(WorldQueryKind::Radiance);
    let helper_name = plan.helper_name.clone();
    let semantics = world_query_semantics(WorldQueryKind::Radiance);
    let span = TextRange::empty(0.into());
    let mut lowerer = FunctionLowerer::new(
        helper_name.clone(),
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

    let capture = lowerer.new_local(
        SmolStr::new("capture"),
        false,
        MirType::Named(SmolStr::new("RegionCapture")),
    );
    let domain = lowerer.new_local(
        SmolStr::new("domain"),
        false,
        MirType::Named(SmolStr::new("SceneDomain")),
    );
    let sample = lowerer.new_local(
        SmolStr::new("sample"),
        false,
        MirType::Named(SmolStr::new("PointDirectionQuery")),
    );
    let backend = lowerer.new_local(SmolStr::new("backend"), false, MirType::Integer);
    for (name, local) in [
        (SmolStr::new("capture"), capture),
        (SmolStr::new("domain"), domain),
        (SmolStr::new("sample"), sample),
        (SmolStr::new("backend"), backend),
    ] {
        lowerer.declare_local(name, local);
        lowerer.params.push(local);
    }

    let entry = lowerer.new_block();
    let cpu_block = lowerer.new_block();
    let wgsl_block = lowerer.new_block();
    let unsupported_block = lowerer.new_block();
    lowerer.current_block = entry;
    let point = lowerer.lower_get_named_field(
        Value::Local(sample),
        "PointDirectionQuery",
        "point",
        MirType::Vec3,
        span,
    );
    let direction = lowerer.lower_get_named_field(
        Value::Local(sample),
        "PointDirectionQuery",
        "direction",
        MirType::Vec3,
        span,
    );
    let black = lowerer.lower_call_temp(
        MirType::Vec3,
        SmolStr::new("vec3"),
        vec![
            Value::Const(Literal::Float(0.0)),
            Value::Const(Literal::Float(0.0)),
            Value::Const(Literal::Float(0.0)),
        ],
        span,
    );
    let (capture_scene_id, detail) =
        lower_world_domain_validation(&mut lowerer, capture, domain, semantics.query_name, span);
    lower_world_domain_flag_guard(
        &mut lowerer,
        domain,
        semantics.domain_flag.expect("radiance_world flag"),
        black,
        span,
    );
    let result = lowerer.new_local(SmolStr::new("$world_radiance_result"), true, MirType::Vec3);
    let zero = lowerer.lower_call_temp(
        MirType::Vec3,
        SmolStr::new("vec3"),
        vec![
            Value::Const(Literal::Float(0.0)),
            Value::Const(Literal::Float(0.0)),
            Value::Const(Literal::Float(0.0)),
        ],
        span,
    );
    lowerer.assign_use(Place::Local(result), zero, span);
    let return_block = lowerer.new_block();
    lower_native_world_backend_guard(
        &mut lowerer,
        backend,
        auto_backend,
        cpu_block,
        wgsl_block,
        unsupported_block,
        span,
    );

    lowerer.current_block = cpu_block;
    lower_world_region_dispatch(
        &mut lowerer,
        module,
        capture_scene_id.clone(),
        detail.clone(),
        return_block,
        "radiance_world requires a capture created from a region declaration",
        span,
        |lowerer, shapes, span| {
            let mut backend = MirWorldRadianceBackend {
                lowerer,
                point: point.clone(),
                direction: direction.clone(),
                result,
                span,
            };
            walk_world_radiance_shapes(&mut backend, shapes).expect("mir world radiance walk");
        },
    );

    lowerer.current_block = wgsl_block;
    if let Some(Ok(config)) = wgsl_config {
        lower_world_region_dispatch(
            &mut lowerer,
            module,
            capture_scene_id,
            detail,
            return_block,
            "radiance_world requires a capture created from a region declaration",
            span,
            |lowerer, shapes, span| {
                let value = lower_world_wgsl_bridge_call(
                    lowerer,
                    MirType::Vec3,
                    Some(&Ok(config.clone())),
                    "__wr_wgsl_world_radiance_capture",
                    shapes,
                    wgsl_shape_indices,
                    vec![Value::Local(sample)],
                    span,
                )
                .expect("validated WGSL world radiance config");
                lowerer.assign_use(Place::Local(result), value, span);
            },
        );
    } else if let Some(Err(err)) = wgsl_config {
        lower_wgsl_bridge_failure(&mut lowerer, err.clone(), span);
    } else {
        lower_wgsl_bridge_failure(
            &mut lowerer,
            SmolStr::new("missing WGSL bridge config for world radiance"),
            span,
        );
    }

    lowerer.current_block = return_block;
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Local(result)),
        span,
    });

    MirFunction {
        name: helper_name,
        params: lowerer.params,
        abi_params: vec![
            portable_abi_from_type_ref(
                Some(&hir::TypeRef {
                    name: SmolStr::new("RegionCapture"),
                    name_span: None,
                    args: Vec::new(),
                }),
                module,
                type_tags,
                &mut HashSet::new(),
            ),
            portable_abi_from_type_ref(
                Some(&hir::TypeRef {
                    name: SmolStr::new("SceneDomain"),
                    name_span: None,
                    args: Vec::new(),
                }),
                module,
                type_tags,
                &mut HashSet::new(),
            ),
            portable_abi_from_type_ref(
                Some(&hir::TypeRef {
                    name: SmolStr::new("PointDirectionQuery"),
                    name_span: None,
                    args: Vec::new(),
                }),
                module,
                type_tags,
                &mut HashSet::new(),
            ),
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

pub(crate) fn lower_world_medium_capture_helper(
    module: &hir::Module,
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
    auto_backend: DispatchBackend,
    wgsl_config: Option<&Result<NativeWgslBridgeConfig, SmolStr>>,
    wgsl_shape_indices: &HashMap<SmolStr, u32>,
) -> MirFunction {
    let plan = crate::query_plan::WorldQueryPlan::for_query(WorldQueryKind::Medium);
    let helper_name = plan.helper_name.clone();
    let semantics = world_query_semantics(WorldQueryKind::Medium);
    let span = TextRange::empty(0.into());
    let mut lowerer = FunctionLowerer::new(
        helper_name.clone(),
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

    let capture = lowerer.new_local(
        SmolStr::new("capture"),
        false,
        MirType::Named(SmolStr::new("RegionCapture")),
    );
    let domain = lowerer.new_local(
        SmolStr::new("domain"),
        false,
        MirType::Named(SmolStr::new("SceneDomain")),
    );
    let point = lowerer.new_local(SmolStr::new("point"), false, MirType::Vec3);
    let backend = lowerer.new_local(SmolStr::new("backend"), false, MirType::Integer);
    for (name, local) in [
        (SmolStr::new("capture"), capture),
        (SmolStr::new("domain"), domain),
        (SmolStr::new("point"), point),
        (SmolStr::new("backend"), backend),
    ] {
        lowerer.declare_local(name, local);
        lowerer.params.push(local);
    }

    let entry = lowerer.new_block();
    let cpu_block = lowerer.new_block();
    let wgsl_block = lowerer.new_block();
    let unsupported_block = lowerer.new_block();
    lowerer.current_block = entry;
    let default_medium = lowerer.build_default_medium(span);
    let (capture_scene_id, detail) =
        lower_world_domain_validation(&mut lowerer, capture, domain, semantics.query_name, span);
    lower_world_domain_flag_guard(
        &mut lowerer,
        domain,
        semantics.domain_flag.expect("medium_world flag"),
        default_medium,
        span,
    );
    let result = lowerer.new_local(
        SmolStr::new("$world_medium_result"),
        true,
        MirType::Named(SmolStr::new("Medium")),
    );
    let default_medium = lowerer.build_default_medium(span);
    lowerer.assign_use(Place::Local(result), default_medium, span);
    let return_block = lowerer.new_block();
    lower_native_world_backend_guard(
        &mut lowerer,
        backend,
        auto_backend,
        cpu_block,
        wgsl_block,
        unsupported_block,
        span,
    );

    lowerer.current_block = cpu_block;
    lower_world_region_dispatch(
        &mut lowerer,
        module,
        capture_scene_id.clone(),
        detail.clone(),
        return_block,
        "medium_world requires a capture created from a region declaration",
        span,
        |lowerer, shapes, span| {
            let mut backend = MirWorldMediumBackend {
                lowerer,
                point,
                result,
                span,
            };
            walk_world_medium_shapes(&mut backend, shapes).expect("mir world medium walk");
        },
    );

    lowerer.current_block = wgsl_block;
    if let Some(Ok(config)) = wgsl_config {
        lower_world_region_dispatch(
            &mut lowerer,
            module,
            capture_scene_id,
            detail,
            return_block,
            "medium_world requires a capture created from a region declaration",
            span,
            |lowerer, shapes, span| {
                let value = lower_world_wgsl_bridge_call(
                    lowerer,
                    MirType::Named(SmolStr::new("Medium")),
                    Some(&Ok(config.clone())),
                    "__wr_wgsl_world_medium_capture",
                    shapes,
                    wgsl_shape_indices,
                    vec![Value::Local(point)],
                    span,
                )
                .expect("validated WGSL world medium config");
                lowerer.assign_use(Place::Local(result), value, span);
            },
        );
    } else if let Some(Err(err)) = wgsl_config {
        lower_wgsl_bridge_failure(&mut lowerer, err.clone(), span);
    } else {
        lower_wgsl_bridge_failure(
            &mut lowerer,
            SmolStr::new("missing WGSL bridge config for world medium"),
            span,
        );
    }

    lowerer.current_block = return_block;
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Local(result)),
        span,
    });

    MirFunction {
        name: helper_name,
        params: lowerer.params,
        abi_params: vec![
            portable_abi_from_type_ref(
                Some(&hir::TypeRef {
                    name: SmolStr::new("RegionCapture"),
                    name_span: None,
                    args: Vec::new(),
                }),
                module,
                type_tags,
                &mut HashSet::new(),
            ),
            portable_abi_from_type_ref(
                Some(&hir::TypeRef {
                    name: SmolStr::new("SceneDomain"),
                    name_span: None,
                    args: Vec::new(),
                }),
                module,
                type_tags,
                &mut HashSet::new(),
            ),
            PortableAbiType::Vec3,
            PortableAbiType::I32,
        ],
        abi_return: portable_abi_from_type_ref(
            Some(&hir::TypeRef {
                name: SmolStr::new("Medium"),
                name_span: None,
                args: Vec::new(),
            }),
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
