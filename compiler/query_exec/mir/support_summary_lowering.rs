//! Owns MIR lowering for support-summary capture helpers and the static support
//! bounds folding used to answer them conservatively.
//! Does not own general scene/world capture lowering or batch query execution.
//!
//! Key invariants:
//! - static support summaries must stay conservative: unknown data widens or
//!   falls back instead of fabricating precise bounds.
//! - backend guards and summary result packing must match the capture contract
//!   that requested the summary.
//! - folded child/support bounds preserve ordering expected by downstream ABI
//!   consumers.
//!
//! Primary entrypoints:
//! - `lower_scene_support_summary_capture_helper`
//! - `lower_world_support_summary_capture_helper`
//! - `field_support_summary_parts`
//!
//! Failure modes / common pitfalls:
//! - treating partially-known support bounds as exact breaks collision and query
//!   conservatism guarantees.
//! - duplicating static-eval logic outside this file makes summary behavior drift
//!   between field and shape paths.

use super::scene_capture_lowering::{
    MirStaticSceneValue, MirStaticSupportBounds, MirStaticSupportSummaryParts,
};
use super::scene_medium_capture_lowering::{
    lower_wgsl_bridge_failure, lower_world_domain_validation, lower_world_region_dispatch,
};
use super::{
    BTreeMap, BinaryOp, FunctionLowerer, FunctionRole, HashMap, HashSet, Literal, MirFunction,
    MirStmt, MirType, PortableAbiType, SmolStr, TextRange, TypeTagId, UnaryOp, Value,
    WorldQueryKind, portable_abi_from_type_ref, scene_ir, stable_field_scene_capture_id,
    stable_shape_scene_capture_id,
};
use crate::hir;
use crate::mir::ir::*;

pub(crate) fn lower_scene_support_summary_capture_helper(
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
    capture_type_name: &'static str,
    helper_name: &'static str,
) -> MirFunction {
    let helper_name = SmolStr::new(helper_name);
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
        MirType::Named(SmolStr::new(capture_type_name)),
    );
    lowerer.declare_local(SmolStr::new("capture"), capture);
    lowerer.params.push(capture);

    let entry = lowerer.new_block();
    lowerer.current_block = entry;
    let scene_id = lowerer.lower_get_named_field(
        Value::Local(capture),
        capture_type_name,
        "scene_id",
        MirType::Integer,
        span,
    );
    let result = lowerer.new_local(
        SmolStr::new("$support_summary_result"),
        true,
        MirType::Named(SmolStr::new("SupportSummaryResult")),
    );
    let default_summary =
        lower_support_summary_result_value(&mut lowerer, unknown_support_summary(), span);
    lowerer.assign_use(Place::Local(result), default_summary, span);
    let return_block = lowerer.new_block();
    let mut dispatch_block = lowerer.new_block();
    lowerer.set_terminator(Terminator::Jump {
        target: dispatch_block,
        span,
    });

    match capture_type_name {
        "FieldCapture" => {
            for (_, field) in module
                .functions
                .iter()
                .filter(|(_, func)| matches!(func.role, FunctionRole::Field))
            {
                let Some(scene) = lowerer.field_scene(&field.name) else {
                    continue;
                };
                let parts = field_support_summary_parts(&lowerer.field_scenes, scene);
                dispatch_support_summary_case(
                    &mut lowerer,
                    &mut dispatch_block,
                    result,
                    return_block,
                    scene_id.clone(),
                    stable_field_scene_capture_id(&field.name),
                    parts,
                    span,
                );
            }
        }
        "ShapeCapture" => {
            let mut scene_names = lowerer.shape_scenes.keys().cloned().collect::<Vec<_>>();
            scene_names.sort();
            for shape_name in scene_names {
                let scene = lowerer
                    .shape_scene(&shape_name)
                    .expect("shape summary dispatch uses known shape scenes");
                let parts = shape_support_summary_parts(&lowerer.shape_scenes, scene);
                dispatch_support_summary_case(
                    &mut lowerer,
                    &mut dispatch_block,
                    result,
                    return_block,
                    scene_id.clone(),
                    stable_shape_scene_capture_id(&shape_name),
                    parts,
                    span,
                );
            }
        }
        other => panic!("unsupported support summary capture type {other}"),
    }

    lowerer.current_block = dispatch_block;
    let invalid_scene_block = lowerer.new_block();
    lowerer.set_terminator(Terminator::Jump {
        target: invalid_scene_block,
        span,
    });
    lowerer.current_block = invalid_scene_block;
    let crash_temp = lowerer.new_temp(MirType::Unknown);
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(crash_temp),
        value: Rvalue::Crash {
            value: Value::Const(Literal::String(SmolStr::new(
                "support.summary requires a capture created by `capture`",
            ))),
        },
        span,
    });
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Temp(crash_temp)),
        span,
    });

    lowerer.current_block = return_block;
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Local(result)),
        span,
    });

    MirFunction {
        name: helper_name,
        params: lowerer.params,
        abi_params: vec![portable_abi_from_type_ref(
            Some(&hir::TypeRef {
                name: SmolStr::new(capture_type_name),
                name_span: None,
                args: Vec::new(),
            }),
            module,
            type_tags,
            &mut HashSet::new(),
        )],
        abi_return: portable_abi_from_type_ref(
            Some(&hir::TypeRef {
                name: SmolStr::new("SupportSummaryResult"),
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

pub(crate) fn lower_world_support_summary_capture_helper(
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
    let plan = crate::query_plan::WorldQueryPlan::for_query(WorldQueryKind::SupportSummary);
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
    let backend = lowerer.new_local(SmolStr::new("backend"), false, MirType::Integer);
    for (name, local) in [
        (SmolStr::new("capture"), capture),
        (SmolStr::new("domain"), domain),
        (SmolStr::new("backend"), backend),
    ] {
        lowerer.declare_local(name, local);
        lowerer.params.push(local);
    }

    let entry = lowerer.new_block();
    let compute_block = lowerer.new_block();
    let unsupported_block = lowerer.new_block();
    lowerer.current_block = entry;
    lower_support_summary_backend_guard(
        &mut lowerer,
        backend,
        compute_block,
        unsupported_block,
        span,
    );

    lowerer.current_block = compute_block;
    let result = lowerer.new_local(
        SmolStr::new("$world_support_summary_result"),
        true,
        MirType::Named(SmolStr::new("SupportSummaryResult")),
    );
    let default_summary =
        lower_support_summary_result_value(&mut lowerer, unknown_support_summary(), span);
    lowerer.assign_use(Place::Local(result), default_summary, span);
    let (capture_scene_id, detail) =
        lower_world_domain_validation(&mut lowerer, capture, domain, "support.summary", span);
    let return_block = lowerer.new_block();
    lower_world_region_dispatch(
        &mut lowerer,
        module,
        capture_scene_id,
        detail,
        return_block,
        "support.summary requires a capture created from a region declaration",
        span,
        |lowerer, shapes, span| {
            let parts = world_support_summary_parts(&lowerer.shape_scenes, shapes);
            let summary = lower_support_summary_result_value(lowerer, parts, span);
            lowerer.assign_use(Place::Local(result), summary, span);
        },
    );

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
            PortableAbiType::I32,
        ],
        abi_return: portable_abi_from_type_ref(
            Some(&hir::TypeRef {
                name: SmolStr::new("SupportSummaryResult"),
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

fn dispatch_support_summary_case(
    lowerer: &mut FunctionLowerer,
    dispatch_block: &mut BlockId,
    result: LocalId,
    return_block: BlockId,
    scene_id: Value,
    expected_scene_id: i64,
    parts: MirStaticSupportSummaryParts,
    span: TextRange,
) {
    let match_block = lowerer.new_block();
    let next_block = lowerer.new_block();
    lowerer.current_block = *dispatch_block;
    let matched = lowerer.lower_binary_temp(
        MirType::Boolean,
        BinaryOp::Eq,
        scene_id,
        Value::Const(Literal::Integer(expected_scene_id)),
        span,
    );
    lowerer.set_terminator(Terminator::Branch {
        cond: matched,
        then_target: match_block,
        else_target: next_block,
        span,
    });
    lowerer.current_block = match_block;
    let result_value = lower_support_summary_result_value(lowerer, parts, span);
    lowerer.assign_use(Place::Local(result), result_value, span);
    lowerer.set_terminator(Terminator::Jump {
        target: return_block,
        span,
    });
    *dispatch_block = next_block;
}

fn lower_support_summary_backend_guard(
    lowerer: &mut FunctionLowerer,
    backend: LocalId,
    compute_block: BlockId,
    unsupported_block: BlockId,
    span: TextRange,
) {
    let is_cpu = lowerer.lower_binary_temp(
        MirType::Boolean,
        BinaryOp::Eq,
        Value::Local(backend),
        Value::Const(Literal::Integer(0)),
        span,
    );
    let is_vgpu = lowerer.lower_binary_temp(
        MirType::Boolean,
        BinaryOp::Eq,
        Value::Local(backend),
        Value::Const(Literal::Integer(1)),
        span,
    );
    let is_auto = lowerer.lower_binary_temp(
        MirType::Boolean,
        BinaryOp::Eq,
        Value::Local(backend),
        Value::Const(Literal::Integer(3)),
        span,
    );
    let cpu_or_vgpu =
        lowerer.lower_binary_temp(MirType::Boolean, BinaryOp::Or, is_cpu, is_vgpu, span);
    let supported =
        lowerer.lower_binary_temp(MirType::Boolean, BinaryOp::Or, cpu_or_vgpu, is_auto, span);
    lowerer.set_terminator(Terminator::Branch {
        cond: supported,
        then_target: compute_block,
        else_target: unsupported_block,
        span,
    });
    lowerer.current_block = unsupported_block;
    lower_wgsl_bridge_failure(
        lowerer,
        SmolStr::new("support.summary supports cpu, virtual_gpu, or auto backends"),
        span,
    );
}

fn lower_support_summary_result_value(
    lowerer: &mut FunctionLowerer,
    parts: MirStaticSupportSummaryParts,
    span: TextRange,
) -> Value {
    let mut class = lowerer.synthetic_class_target_info("SupportSummaryResult");
    FunctionLowerer::set_class_field_value(
        &mut class,
        "support_class",
        Value::Const(Literal::Integer(i64::from(support_class_code(
            parts.support_class,
        )))),
    );
    FunctionLowerer::set_class_field_value(
        &mut class,
        "semantics",
        Value::Const(Literal::Integer(i64::from(distance_semantics_code(
            parts.semantics,
        )))),
    );
    FunctionLowerer::set_class_field_value(
        &mut class,
        "has_bounds",
        Value::Const(Literal::Boolean(parts.has_bounds)),
    );
    FunctionLowerer::set_class_field_value(
        &mut class,
        "opaque_boundary",
        Value::Const(Literal::Boolean(parts.opaque_boundary)),
    );
    FunctionLowerer::set_class_field_value(
        &mut class,
        "can_coarse_support_prune",
        Value::Const(Literal::Boolean(parts.can_coarse_support_prune)),
    );
    let min = lower_vec3_const(lowerer, parts.bounds.min, span);
    let max = lower_vec3_const(lowerer, parts.bounds.max, span);
    FunctionLowerer::set_class_field_value(&mut class, "min", min);
    FunctionLowerer::set_class_field_value(&mut class, "max", max);
    lowerer.build_class_instance(&class, span)
}

fn lower_vec3_const(lowerer: &mut FunctionLowerer, value: [f32; 3], span: TextRange) -> Value {
    lowerer.lower_call_temp(
        MirType::Vec3,
        SmolStr::new("vec3"),
        vec![
            Value::Const(Literal::Float(f64::from(value[0]))),
            Value::Const(Literal::Float(f64::from(value[1]))),
            Value::Const(Literal::Float(f64::from(value[2]))),
        ],
        span,
    )
}

fn unknown_support_summary() -> MirStaticSupportSummaryParts {
    MirStaticSupportSummaryParts {
        support_class: scene_ir::SupportClass::Unknown,
        semantics: scene_ir::DistanceSemantics::ConservativeLowerBound,
        has_bounds: false,
        opaque_boundary: false,
        can_coarse_support_prune: false,
        bounds: empty_static_support_bounds(),
    }
}

fn field_support_summary_parts(
    field_scenes: &BTreeMap<SmolStr, scene_ir::FieldScene>,
    scene: &scene_ir::FieldScene,
) -> MirStaticSupportSummaryParts {
    let bounds = field_static_support_bounds(
        field_scenes,
        scene,
        scene.root_support_id,
        &mut HashSet::new(),
    );
    MirStaticSupportSummaryParts {
        support_class: scene.support_class,
        semantics: scene.semantics,
        has_bounds: bounds.is_some(),
        opaque_boundary: scene.opaque_boundary,
        can_coarse_support_prune: scene.can_coarse_support_pruning,
        bounds: bounds.unwrap_or_else(empty_static_support_bounds),
    }
}

fn shape_support_summary_parts(
    shape_scenes: &BTreeMap<SmolStr, scene_ir::ShapeScene>,
    scene: &scene_ir::ShapeScene,
) -> MirStaticSupportSummaryParts {
    let bounds = shape_static_support_bounds(
        shape_scenes,
        scene,
        scene.root_support_id,
        &mut HashSet::new(),
    );
    MirStaticSupportSummaryParts {
        support_class: scene.support_class,
        semantics: scene.semantics,
        has_bounds: bounds.is_some(),
        opaque_boundary: scene.opaque_boundary,
        can_coarse_support_prune: scene.can_coarse_support_pruning,
        bounds: bounds.unwrap_or_else(empty_static_support_bounds),
    }
}

fn world_support_summary_parts(
    shape_scenes: &BTreeMap<SmolStr, scene_ir::ShapeScene>,
    shapes: &[SmolStr],
) -> MirStaticSupportSummaryParts {
    let items = shapes
        .iter()
        .filter_map(|shape| shape_scenes.get(shape))
        .map(|scene| shape_support_summary_parts(shape_scenes, scene))
        .collect::<Vec<_>>();
    merge_world_support_summaries(&items)
}

fn field_static_support_bounds(
    field_scenes: &BTreeMap<SmolStr, scene_ir::FieldScene>,
    scene: &scene_ir::FieldScene,
    id: scene_ir::SupportNodeId,
    visiting: &mut HashSet<(SmolStr, u32)>,
) -> Option<MirStaticSupportBounds> {
    if !visiting.insert((scene.name.clone(), id.0)) {
        return None;
    }
    let record = scene.support_node_record(id)?;
    let bounds = match record.kind {
        scene_ir::SupportNodeKindSummary::Unknown
        | scene_ir::SupportNodeKindSummary::Unbounded
        | scene_ir::SupportNodeKindSummary::Periodic(_) => None,
        scene_ir::SupportNodeKindSummary::Use => {
            let target = record.target.as_ref()?;
            let target_scene = field_scenes.get(target)?;
            field_static_support_bounds(
                field_scenes,
                target_scene,
                target_scene.root_support_id,
                visiting,
            )
        }
        scene_ir::SupportNodeKindSummary::Aabb
        | scene_ir::SupportNodeKindSummary::Sphere
        | scene_ir::SupportNodeKindSummary::OpaqueBoundary => static_support_payload_bounds(record),
        scene_ir::SupportNodeKindSummary::Union => field_static_support_children_bounds(
            field_scenes,
            scene,
            &record.children,
            merge_union_static_support_bounds,
            false,
            visiting,
        ),
        scene_ir::SupportNodeKindSummary::Intersection => field_static_support_children_bounds(
            field_scenes,
            scene,
            &record.children,
            merge_intersection_static_support_bounds,
            true,
            visiting,
        ),
        scene_ir::SupportNodeKindSummary::Difference => record
            .children
            .first()
            .copied()
            .and_then(|child| field_static_support_bounds(field_scenes, scene, child, visiting)),
        scene_ir::SupportNodeKindSummary::Transform(kind) => {
            let child = record.children.first().copied()?;
            let bounds = field_static_support_bounds(field_scenes, scene, child, visiting)?;
            let param = match record.payload.as_ref() {
                Some(scene_ir::SupportPayload::Transform { param }) => param.as_ref(),
                _ => None,
            };
            transform_static_support_bounds(kind, param, bounds)
        }
        scene_ir::SupportNodeKindSummary::Repeat(_) => None,
    };
    visiting.remove(&(scene.name.clone(), id.0));
    bounds
}

fn shape_static_support_bounds(
    shape_scenes: &BTreeMap<SmolStr, scene_ir::ShapeScene>,
    scene: &scene_ir::ShapeScene,
    id: scene_ir::SupportNodeId,
    visiting: &mut HashSet<(SmolStr, u32)>,
) -> Option<MirStaticSupportBounds> {
    if !visiting.insert((scene.name.clone(), id.0)) {
        return None;
    }
    let record = scene.support_node_record(id)?;
    let bounds = match record.kind {
        scene_ir::SupportNodeKindSummary::Unknown
        | scene_ir::SupportNodeKindSummary::Unbounded
        | scene_ir::SupportNodeKindSummary::Periodic(_) => None,
        scene_ir::SupportNodeKindSummary::Use => {
            let target = record.target.as_ref()?;
            let target_scene = shape_scenes.get(target)?;
            shape_static_support_bounds(
                shape_scenes,
                target_scene,
                target_scene.root_support_id,
                visiting,
            )
        }
        scene_ir::SupportNodeKindSummary::Aabb
        | scene_ir::SupportNodeKindSummary::Sphere
        | scene_ir::SupportNodeKindSummary::OpaqueBoundary => static_support_payload_bounds(record),
        scene_ir::SupportNodeKindSummary::Union => shape_static_support_children_bounds(
            shape_scenes,
            scene,
            &record.children,
            merge_union_static_support_bounds,
            false,
            visiting,
        ),
        scene_ir::SupportNodeKindSummary::Intersection => shape_static_support_children_bounds(
            shape_scenes,
            scene,
            &record.children,
            merge_intersection_static_support_bounds,
            true,
            visiting,
        ),
        scene_ir::SupportNodeKindSummary::Difference => record
            .children
            .first()
            .copied()
            .and_then(|child| shape_static_support_bounds(shape_scenes, scene, child, visiting)),
        scene_ir::SupportNodeKindSummary::Transform(kind) => {
            let child = record.children.first().copied()?;
            let bounds = shape_static_support_bounds(shape_scenes, scene, child, visiting)?;
            let param = match record.payload.as_ref() {
                Some(scene_ir::SupportPayload::Transform { param }) => param.as_ref(),
                _ => None,
            };
            transform_static_support_bounds(kind, param, bounds)
        }
        scene_ir::SupportNodeKindSummary::Repeat(_) => None,
    };
    visiting.remove(&(scene.name.clone(), id.0));
    bounds
}

fn field_static_support_children_bounds(
    field_scenes: &BTreeMap<SmolStr, scene_ir::FieldScene>,
    scene: &scene_ir::FieldScene,
    children: &[scene_ir::SupportNodeId],
    merge: fn(MirStaticSupportBounds, MirStaticSupportBounds) -> MirStaticSupportBounds,
    allow_partial: bool,
    visiting: &mut HashSet<(SmolStr, u32)>,
) -> Option<MirStaticSupportBounds> {
    let mut out = None;
    for child in children {
        match field_static_support_bounds(field_scenes, scene, *child, visiting) {
            Some(bounds) => {
                out = Some(match out {
                    Some(current) => merge(current, bounds),
                    None => bounds,
                });
            }
            None if !allow_partial => return None,
            None => {}
        }
    }
    out
}

fn shape_static_support_children_bounds(
    shape_scenes: &BTreeMap<SmolStr, scene_ir::ShapeScene>,
    scene: &scene_ir::ShapeScene,
    children: &[scene_ir::SupportNodeId],
    merge: fn(MirStaticSupportBounds, MirStaticSupportBounds) -> MirStaticSupportBounds,
    allow_partial: bool,
    visiting: &mut HashSet<(SmolStr, u32)>,
) -> Option<MirStaticSupportBounds> {
    let mut out = None;
    for child in children {
        match shape_static_support_bounds(shape_scenes, scene, *child, visiting) {
            Some(bounds) => {
                out = Some(match out {
                    Some(current) => merge(current, bounds),
                    None => bounds,
                });
            }
            None if !allow_partial => return None,
            None => {}
        }
    }
    out
}

fn static_support_payload_bounds(
    record: &scene_ir::SupportNodeRecord,
) -> Option<MirStaticSupportBounds> {
    match record.payload.as_ref()? {
        scene_ir::SupportPayload::Aabb { min, max } => Some(MirStaticSupportBounds {
            min: static_vec3(min)?,
            max: static_vec3(max)?,
        }),
        scene_ir::SupportPayload::Sphere { center, radius } => {
            let center = static_vec3(center)?;
            let radius = static_f32(radius)?.abs();
            Some(MirStaticSupportBounds {
                min: [center[0] - radius, center[1] - radius, center[2] - radius],
                max: [center[0] + radius, center[1] + radius, center[2] + radius],
            })
        }
        scene_ir::SupportPayload::OpaqueBoundary {
            bounds: Some(bounds),
        } => {
            let MirStaticSceneValue::Bounds3 { min, max } = eval_static_scene_value(bounds)? else {
                return None;
            };
            Some(MirStaticSupportBounds { min, max })
        }
        _ => None,
    }
}

fn transform_static_support_bounds(
    kind: scene_ir::TransformKind,
    param: Option<&scene_ir::SceneValueExpr>,
    bounds: MirStaticSupportBounds,
) -> Option<MirStaticSupportBounds> {
    let Some(param) = param else {
        return Some(bounds);
    };
    match kind {
        scene_ir::TransformKind::Translate => {
            let offset = static_vec3(param)?;
            Some(MirStaticSupportBounds {
                min: add3(bounds.min, offset),
                max: add3(bounds.max, offset),
            })
        }
        scene_ir::TransformKind::UniformScale => {
            let scale = static_f32(param)?;
            Some(normalize_static_support_bounds(MirStaticSupportBounds {
                min: mul3_scalar(bounds.min, scale),
                max: mul3_scalar(bounds.max, scale),
            }))
        }
        scene_ir::TransformKind::Rotate
        | scene_ir::TransformKind::AffineTransform
        | scene_ir::TransformKind::Warp
        | scene_ir::TransformKind::Bend
        | scene_ir::TransformKind::Twist
        | scene_ir::TransformKind::Taper
        | scene_ir::TransformKind::Displace => None,
    }
}

fn eval_static_scene_value(expr: &scene_ir::SceneValueExpr) -> Option<MirStaticSceneValue> {
    match expr {
        scene_ir::SceneValueExpr::Literal(literal) => match literal {
            Literal::Integer(value) => Some(MirStaticSceneValue::I32(*value as i32)),
            Literal::Float(value) => Some(MirStaticSceneValue::F32(*value as f32)),
            Literal::Boolean(value) => Some(MirStaticSceneValue::Bool(*value)),
            Literal::String(_) | Literal::Nil => None,
        },
        scene_ir::SceneValueExpr::List(_) => None,
        scene_ir::SceneValueExpr::Unary { op, expr } => {
            let value = eval_static_scene_value(expr)?;
            match (*op, value) {
                (UnaryOp::Neg, MirStaticSceneValue::I32(value)) => {
                    Some(MirStaticSceneValue::I32(-value))
                }
                (UnaryOp::Neg, MirStaticSceneValue::F32(value)) => {
                    Some(MirStaticSceneValue::F32(-value))
                }
                (UnaryOp::Neg, MirStaticSceneValue::Vec3(value)) => {
                    Some(MirStaticSceneValue::Vec3([-value[0], -value[1], -value[2]]))
                }
                (UnaryOp::Not, MirStaticSceneValue::Bool(value)) => {
                    Some(MirStaticSceneValue::Bool(!value))
                }
                _ => None,
            }
        }
        scene_ir::SceneValueExpr::Binary { lhs, op, rhs } => {
            let lhs = eval_static_scene_value(lhs)?;
            let rhs = eval_static_scene_value(rhs)?;
            eval_static_binary(*op, lhs, rhs)
        }
        scene_ir::SceneValueExpr::Call { callee, args } => eval_static_call(callee.as_str(), args),
    }
}

fn eval_static_call(name: &str, args: &[scene_ir::SceneArgExpr]) -> Option<MirStaticSceneValue> {
    match name {
        "f32" => static_arg_f32(args.first()?).map(MirStaticSceneValue::F32),
        "i32" => static_arg_f32(args.first()?).map(|value| MirStaticSceneValue::I32(value as i32)),
        "vec3" => Some(MirStaticSceneValue::Vec3([
            static_arg_f32(args.first()?)?,
            static_arg_f32(args.get(1)?)?,
            static_arg_f32(args.get(2)?)?,
        ])),
        "Bounds3" | "bounds3" => {
            let min = static_named_or_pos_vec3(args, "min", 0)?;
            let max = static_named_or_pos_vec3(args, "max", 1)?;
            Some(MirStaticSceneValue::Bounds3 { min, max })
        }
        _ => None,
    }
}

fn eval_static_binary(
    op: BinaryOp,
    lhs: MirStaticSceneValue,
    rhs: MirStaticSceneValue,
) -> Option<MirStaticSceneValue> {
    match (op, lhs, rhs) {
        (BinaryOp::Add, MirStaticSceneValue::F32(lhs), MirStaticSceneValue::F32(rhs)) => {
            Some(MirStaticSceneValue::F32(lhs + rhs))
        }
        (BinaryOp::Sub, MirStaticSceneValue::F32(lhs), MirStaticSceneValue::F32(rhs)) => {
            Some(MirStaticSceneValue::F32(lhs - rhs))
        }
        (BinaryOp::Mul, MirStaticSceneValue::F32(lhs), MirStaticSceneValue::F32(rhs)) => {
            Some(MirStaticSceneValue::F32(lhs * rhs))
        }
        (BinaryOp::Div, MirStaticSceneValue::F32(lhs), MirStaticSceneValue::F32(rhs)) => {
            Some(MirStaticSceneValue::F32(lhs / rhs))
        }
        (BinaryOp::Add, MirStaticSceneValue::I32(lhs), MirStaticSceneValue::I32(rhs)) => {
            Some(MirStaticSceneValue::I32(lhs.saturating_add(rhs)))
        }
        (BinaryOp::Sub, MirStaticSceneValue::I32(lhs), MirStaticSceneValue::I32(rhs)) => {
            Some(MirStaticSceneValue::I32(lhs.saturating_sub(rhs)))
        }
        (BinaryOp::Mul, MirStaticSceneValue::I32(lhs), MirStaticSceneValue::I32(rhs)) => {
            Some(MirStaticSceneValue::I32(lhs.saturating_mul(rhs)))
        }
        (BinaryOp::Add, MirStaticSceneValue::Vec3(lhs), MirStaticSceneValue::Vec3(rhs)) => {
            Some(MirStaticSceneValue::Vec3(add3(lhs, rhs)))
        }
        (BinaryOp::Sub, MirStaticSceneValue::Vec3(lhs), MirStaticSceneValue::Vec3(rhs)) => {
            Some(MirStaticSceneValue::Vec3(add3(lhs, mul3_scalar(rhs, -1.0))))
        }
        (BinaryOp::Mul, MirStaticSceneValue::Vec3(lhs), MirStaticSceneValue::F32(rhs))
        | (BinaryOp::Mul, MirStaticSceneValue::F32(rhs), MirStaticSceneValue::Vec3(lhs)) => {
            Some(MirStaticSceneValue::Vec3(mul3_scalar(lhs, rhs)))
        }
        (BinaryOp::Div, MirStaticSceneValue::Vec3(lhs), MirStaticSceneValue::F32(rhs)) => {
            Some(MirStaticSceneValue::Vec3(mul3_scalar(lhs, 1.0 / rhs)))
        }
        _ => None,
    }
}

fn static_named_or_pos_vec3(
    args: &[scene_ir::SceneArgExpr],
    name: &str,
    position: usize,
) -> Option<[f32; 3]> {
    args.iter()
        .find_map(|arg| match arg {
            scene_ir::SceneArgExpr::Named {
                name: arg_name,
                value,
            } if arg_name.as_str() == name => static_vec3(value),
            _ => None,
        })
        .or_else(|| args.get(position).and_then(static_arg_vec3))
}

fn static_arg_f32(arg: &scene_ir::SceneArgExpr) -> Option<f32> {
    match arg {
        scene_ir::SceneArgExpr::Positional(value) | scene_ir::SceneArgExpr::Named { value, .. } => {
            static_f32(value)
        }
    }
}

fn static_arg_vec3(arg: &scene_ir::SceneArgExpr) -> Option<[f32; 3]> {
    match arg {
        scene_ir::SceneArgExpr::Positional(value) | scene_ir::SceneArgExpr::Named { value, .. } => {
            static_vec3(value)
        }
    }
}

fn static_f32(expr: &scene_ir::SceneValueExpr) -> Option<f32> {
    match eval_static_scene_value(expr)? {
        MirStaticSceneValue::F32(value) => Some(value),
        MirStaticSceneValue::I32(value) => Some(value as f32),
        _ => None,
    }
}

fn static_vec3(expr: &scene_ir::SceneValueExpr) -> Option<[f32; 3]> {
    match eval_static_scene_value(expr)? {
        MirStaticSceneValue::Vec3(value) => Some(value),
        _ => None,
    }
}

fn merge_world_support_summaries(
    items: &[MirStaticSupportSummaryParts],
) -> MirStaticSupportSummaryParts {
    if items.is_empty() {
        return unknown_support_summary();
    }
    let support_class = if items
        .iter()
        .any(|item| matches!(item.support_class, scene_ir::SupportClass::Unbounded))
    {
        scene_ir::SupportClass::Unbounded
    } else if items
        .iter()
        .any(|item| matches!(item.support_class, scene_ir::SupportClass::Periodic))
    {
        scene_ir::SupportClass::Periodic
    } else if items
        .iter()
        .any(|item| matches!(item.support_class, scene_ir::SupportClass::Unknown))
    {
        scene_ir::SupportClass::Unknown
    } else {
        scene_ir::SupportClass::Bounded
    };
    let semantics = if items
        .iter()
        .any(|item| matches!(item.semantics, scene_ir::DistanceSemantics::UnknownOpaque))
    {
        scene_ir::DistanceSemantics::UnknownOpaque
    } else if items.len() == 1 {
        items[0].semantics
    } else {
        scene_ir::DistanceSemantics::ConservativeLowerBound
    };
    let has_bounds = items.iter().all(|item| item.has_bounds);
    let bounds = if has_bounds {
        items
            .iter()
            .map(|item| item.bounds)
            .reduce(merge_union_static_support_bounds)
            .unwrap_or_else(empty_static_support_bounds)
    } else {
        empty_static_support_bounds()
    };
    let opaque_boundary = items.iter().any(|item| item.opaque_boundary);
    let can_coarse_support_prune = !opaque_boundary
        && matches!(support_class, scene_ir::SupportClass::Bounded)
        && items.iter().all(|item| item.can_coarse_support_prune);
    MirStaticSupportSummaryParts {
        support_class,
        semantics,
        has_bounds,
        opaque_boundary,
        can_coarse_support_prune,
        bounds,
    }
}

fn empty_static_support_bounds() -> MirStaticSupportBounds {
    MirStaticSupportBounds {
        min: [0.0, 0.0, 0.0],
        max: [0.0, 0.0, 0.0],
    }
}

fn normalize_static_support_bounds(bounds: MirStaticSupportBounds) -> MirStaticSupportBounds {
    MirStaticSupportBounds {
        min: [
            bounds.min[0].min(bounds.max[0]),
            bounds.min[1].min(bounds.max[1]),
            bounds.min[2].min(bounds.max[2]),
        ],
        max: [
            bounds.min[0].max(bounds.max[0]),
            bounds.min[1].max(bounds.max[1]),
            bounds.min[2].max(bounds.max[2]),
        ],
    }
}

fn merge_union_static_support_bounds(
    lhs: MirStaticSupportBounds,
    rhs: MirStaticSupportBounds,
) -> MirStaticSupportBounds {
    MirStaticSupportBounds {
        min: [
            lhs.min[0].min(rhs.min[0]),
            lhs.min[1].min(rhs.min[1]),
            lhs.min[2].min(rhs.min[2]),
        ],
        max: [
            lhs.max[0].max(rhs.max[0]),
            lhs.max[1].max(rhs.max[1]),
            lhs.max[2].max(rhs.max[2]),
        ],
    }
}

fn merge_intersection_static_support_bounds(
    lhs: MirStaticSupportBounds,
    rhs: MirStaticSupportBounds,
) -> MirStaticSupportBounds {
    normalize_static_support_bounds(MirStaticSupportBounds {
        min: [
            lhs.min[0].max(rhs.min[0]),
            lhs.min[1].max(rhs.min[1]),
            lhs.min[2].max(rhs.min[2]),
        ],
        max: [
            lhs.max[0].min(rhs.max[0]),
            lhs.max[1].min(rhs.max[1]),
            lhs.max[2].min(rhs.max[2]),
        ],
    })
}

fn add3(lhs: [f32; 3], rhs: [f32; 3]) -> [f32; 3] {
    [lhs[0] + rhs[0], lhs[1] + rhs[1], lhs[2] + rhs[2]]
}

fn mul3_scalar(value: [f32; 3], scalar: f32) -> [f32; 3] {
    [value[0] * scalar, value[1] * scalar, value[2] * scalar]
}

fn support_class_code(class: scene_ir::SupportClass) -> u32 {
    match class {
        scene_ir::SupportClass::Unknown => 0,
        scene_ir::SupportClass::Bounded => 1,
        scene_ir::SupportClass::Periodic => 2,
        scene_ir::SupportClass::Unbounded => 3,
    }
}

fn distance_semantics_code(semantics: scene_ir::DistanceSemantics) -> u32 {
    match semantics {
        scene_ir::DistanceSemantics::ExactSignedDistance => 0,
        scene_ir::DistanceSemantics::ConservativeLowerBound => 1,
        scene_ir::DistanceSemantics::UnknownOpaque => 2,
    }
}
