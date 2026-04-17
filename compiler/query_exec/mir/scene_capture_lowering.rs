//! Owns MIR lowering for scene capture helpers that return scalar or geometric
//! world observations from captured scene state.
//! Does not own world capture helpers or batch-query lowering.
//!
//! Key invariants:
//! - each lowered helper preserves the scene capture contract it mirrors,
//!   including backend guards and result packing.
//! - region/domain dispatch stays explicit so capture lowering never assumes a
//!   default world scope silently.
//! - capture helper names remain aligned with the stable scene-capture ids used
//!   elsewhere in the compiler.
//!
//! Primary entrypoints:
//! - `lower_scene_distance_capture_helper`
//! - `lower_scene_trace_capture_helper`
//! - `lower_scene_radiance_capture_helper`
//!
//! Failure modes / common pitfalls:
//! - leaking world-helper assumptions into scene capture lowering can break
//!   authored capture identity.
//! - bypassing shared validation helpers makes CPU/WGSL lowering drift.

use super::{
    BinaryOp, CaptureQueryKind, FunctionLowerer, FunctionRole, HashMap, HashSet, Literal,
    MirFunction, MirStmt, MirType, PortableAbiType, SmolStr, TextRange, TypeTagId, Value,
    portable_abi_from_type_ref, scene_ir, stable_field_scene_capture_id, stable_shape_capture_id,
    stable_shape_scene_capture_id,
};
use crate::hir;
use crate::mir::ir::*;

pub(crate) fn lower_scene_distance_capture_helper(
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
    let point = lowerer.new_local(SmolStr::new("point"), false, MirType::Vec3);
    lowerer.declare_local(SmolStr::new("capture"), capture);
    lowerer.declare_local(SmolStr::new("point"), point);
    lowerer.params.push(capture);
    lowerer.params.push(point);

    let entry = lowerer.new_block();
    lowerer.current_block = entry;
    let scene_id = lowerer.new_temp(MirType::Integer);
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(scene_id),
        value: Rvalue::GetField {
            base: Value::Local(capture),
            field: SmolStr::new("scene_id"),
            slot: lowerer.field_slot(capture_type_name, "scene_id"),
        },
        span,
    });

    let result = lowerer.new_local(SmolStr::new("$scene_distance"), true, MirType::Float);
    lowerer.assign_use(
        Place::Local(result),
        Value::Const(Literal::Float(0.0)),
        span,
    );
    let return_block = lowerer.new_block();
    let mut dispatch_block = lowerer.new_block();
    lowerer.set_terminator(Terminator::Jump {
        target: dispatch_block,
        span,
    });

    for (_, field) in module
        .functions
        .iter()
        .filter(|(_, func)| matches!(func.role, FunctionRole::Field))
    {
        let match_block = lowerer.new_block();
        let next_block = lowerer.new_block();
        lowerer.current_block = dispatch_block;
        let matched = lowerer.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Eq,
            Value::Temp(scene_id),
            Value::Const(Literal::Integer(stable_field_scene_capture_id(&field.name))),
            span,
        );
        lowerer.set_terminator(Terminator::Branch {
            cond: matched,
            then_target: match_block,
            else_target: next_block,
            span,
        });
        lowerer.current_block = match_block;
        let distance = lowerer.lower_field_distance_call(&field.name, Value::Local(point), span);
        lowerer.assign_use(Place::Local(result), distance, span);
        lowerer.set_terminator(Terminator::Jump {
            target: return_block,
            span,
        });
        dispatch_block = next_block;
    }

    let shapes_with_scene: Vec<SmolStr> = module
        .shapes
        .iter()
        .map(|(_, shape)| shape.name.clone())
        .filter(|shape_name| lowerer.shape_scene(shape_name).is_some())
        .collect();
    for shape_name in shapes_with_scene {
        let match_block = lowerer.new_block();
        let next_block = lowerer.new_block();
        lowerer.current_block = dispatch_block;
        let matched = lowerer.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Eq,
            Value::Temp(scene_id),
            Value::Const(Literal::Integer(stable_shape_scene_capture_id(&shape_name))),
            span,
        );
        lowerer.set_terminator(Terminator::Branch {
            cond: matched,
            then_target: match_block,
            else_target: next_block,
            span,
        });
        lowerer.current_block = match_block;
        let mode = lowerer.shape_capture_execution_mode(CaptureQueryKind::Distance, &shape_name);
        let distance = lowerer.lower_shape_distance_call_with_mode(
            &shape_name,
            Value::Local(point),
            span,
            mode,
        );
        lowerer.assign_use(Place::Local(result), distance, span);
        lowerer.set_terminator(Terminator::Jump {
            target: return_block,
            span,
        });
        dispatch_block = next_block;
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
                "distance_at requires a capture created by `capture`",
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
        abi_params: vec![
            portable_abi_from_type_ref(
                Some(&hir::TypeRef {
                    name: SmolStr::new(capture_type_name),
                    name_span: None,
                    args: Vec::new(),
                }),
                module,
                type_tags,
                &mut HashSet::new(),
            ),
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

#[derive(Debug, Clone, Copy)]
pub(super) struct MirStaticSupportBounds {
    pub(super) min: [f32; 3],
    pub(super) max: [f32; 3],
}

#[derive(Debug, Clone, Copy)]
pub(super) struct MirStaticSupportSummaryParts {
    pub(super) support_class: scene_ir::SupportClass,
    pub(super) semantics: scene_ir::DistanceSemantics,
    pub(super) has_bounds: bool,
    pub(super) opaque_boundary: bool,
    pub(super) can_coarse_support_prune: bool,
    pub(super) bounds: MirStaticSupportBounds,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum MirStaticSceneValue {
    I32(i32),
    F32(f32),
    Bool(bool),
    Vec3([f32; 3]),
    Bounds3 { min: [f32; 3], max: [f32; 3] },
}

pub(crate) fn lower_scene_normal_capture_helper(
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
    let point = lowerer.new_local(SmolStr::new("point"), false, MirType::Vec3);
    lowerer.declare_local(SmolStr::new("capture"), capture);
    lowerer.declare_local(SmolStr::new("point"), point);
    lowerer.params.push(capture);
    lowerer.params.push(point);

    let entry = lowerer.new_block();
    lowerer.current_block = entry;
    let scene_id = lowerer.new_temp(MirType::Integer);
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(scene_id),
        value: Rvalue::GetField {
            base: Value::Local(capture),
            field: SmolStr::new("scene_id"),
            slot: lowerer.field_slot(capture_type_name, "scene_id"),
        },
        span,
    });
    let result = lowerer.new_local(SmolStr::new("$scene_normal"), true, MirType::Vec3);
    let default_normal = lowerer.lower_call_temp(
        MirType::Vec3,
        SmolStr::new("vec3"),
        vec![
            Value::Const(Literal::Float(0.0)),
            Value::Const(Literal::Float(0.0)),
            Value::Const(Literal::Float(1.0)),
        ],
        span,
    );
    lowerer.assign_use(Place::Local(result), default_normal, span);
    let return_block = lowerer.new_block();
    let mut dispatch_block = lowerer.new_block();
    lowerer.set_terminator(Terminator::Jump {
        target: dispatch_block,
        span,
    });

    for (_, field) in module
        .functions
        .iter()
        .filter(|(_, func)| matches!(func.role, FunctionRole::Field))
    {
        let match_block = lowerer.new_block();
        let next_block = lowerer.new_block();
        lowerer.current_block = dispatch_block;
        let matched = lowerer.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Eq,
            Value::Temp(scene_id),
            Value::Const(Literal::Integer(stable_field_scene_capture_id(&field.name))),
            span,
        );
        lowerer.set_terminator(Terminator::Branch {
            cond: matched,
            then_target: match_block,
            else_target: next_block,
            span,
        });
        lowerer.current_block = match_block;
        let normal = lowerer.lower_field_normal_call(&field.name, Value::Local(point), span);
        lowerer.assign_use(Place::Local(result), normal, span);
        lowerer.set_terminator(Terminator::Jump {
            target: return_block,
            span,
        });
        dispatch_block = next_block;
    }

    let shapes_with_scene: Vec<SmolStr> = module
        .shapes
        .iter()
        .map(|(_, shape)| shape.name.clone())
        .filter(|shape_name| lowerer.shape_scene(shape_name).is_some())
        .collect();
    for shape_name in shapes_with_scene {
        let match_block = lowerer.new_block();
        let next_block = lowerer.new_block();
        lowerer.current_block = dispatch_block;
        let matched = lowerer.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Eq,
            Value::Temp(scene_id),
            Value::Const(Literal::Integer(stable_shape_scene_capture_id(&shape_name))),
            span,
        );
        lowerer.set_terminator(Terminator::Branch {
            cond: matched,
            then_target: match_block,
            else_target: next_block,
            span,
        });
        lowerer.current_block = match_block;
        let mode = lowerer.shape_capture_execution_mode(CaptureQueryKind::Normal, &shape_name);
        let normal =
            lowerer.lower_shape_normal_call_with_mode(&shape_name, Value::Local(point), span, mode);
        lowerer.assign_use(Place::Local(result), normal, span);
        lowerer.set_terminator(Terminator::Jump {
            target: return_block,
            span,
        });
        dispatch_block = next_block;
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
                "normal_at requires a capture created by `capture`",
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
        abi_params: vec![
            portable_abi_from_type_ref(
                Some(&hir::TypeRef {
                    name: SmolStr::new(capture_type_name),
                    name_span: None,
                    args: Vec::new(),
                }),
                module,
                type_tags,
                &mut HashSet::new(),
            ),
            PortableAbiType::Vec3,
        ],
        abi_return: PortableAbiType::Vec3,
        locals: lowerer.locals,
        temps: lowerer.temps,
        blocks: lowerer.blocks,
        entry,
        suspendable: false,
    }
}

pub(crate) fn lower_scene_trace_capture_helper(
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
    let helper_name = SmolStr::new("__wr_scene_trace_capture");
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
        MirType::Named(SmolStr::new("ShapeCapture")),
    );
    let ray = lowerer.new_local(
        SmolStr::new("ray"),
        false,
        MirType::Named(SmolStr::new("RayQuery")),
    );
    for (name, local) in [
        (SmolStr::new("capture"), capture),
        (SmolStr::new("ray"), ray),
    ] {
        lowerer.declare_local(name, local);
        lowerer.params.push(local);
    }

    let entry = lowerer.new_block();
    lowerer.current_block = entry;
    let root_feature_id = lowerer.new_temp(MirType::Integer);
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(root_feature_id),
        value: Rvalue::GetField {
            base: Value::Local(capture),
            field: SmolStr::new("root_feature_id"),
            slot: lowerer.field_slot("ShapeCapture", "root_feature_id"),
        },
        span,
    });
    let invalid_capture_block = lowerer.new_block();
    let shape_capture_block = lowerer.new_block();
    let field_capture = lowerer.lower_binary_temp(
        MirType::Boolean,
        BinaryOp::Eq,
        Value::Temp(root_feature_id),
        Value::Const(Literal::Integer(0)),
        span,
    );
    lowerer.set_terminator(Terminator::Branch {
        cond: field_capture,
        then_target: invalid_capture_block,
        else_target: shape_capture_block,
        span,
    });

    lowerer.current_block = invalid_capture_block;
    let crash_temp = lowerer.new_temp(MirType::Unknown);
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(crash_temp),
        value: Rvalue::Crash {
            value: Value::Const(Literal::String(SmolStr::new(
                "trace_shape requires a shape capture",
            ))),
        },
        span,
    });
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Temp(crash_temp)),
        span,
    });

    lowerer.current_block = shape_capture_block;
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
        SmolStr::new("$scene_trace_result"),
        true,
        MirType::Named(SmolStr::new("Hit3")),
    );
    let default_hit = lowerer.build_default_hit(origin.clone(), span);
    lowerer.assign_use(Place::Local(result), default_hit, span);
    let return_block = lowerer.new_block();

    let mut dispatch_block = lowerer.new_block();
    lowerer.set_terminator(Terminator::Jump {
        target: dispatch_block,
        span,
    });
    let shapes_with_scene: Vec<SmolStr> = module
        .shapes
        .iter()
        .map(|(_, shape)| shape.name.clone())
        .filter(|shape_name| lowerer.shape_scene(shape_name).is_some())
        .collect();
    for shape_name in shapes_with_scene {
        let match_block = lowerer.new_block();
        let next_block = lowerer.new_block();
        lowerer.current_block = dispatch_block;
        let matched = lowerer.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Eq,
            Value::Temp(root_feature_id),
            Value::Const(Literal::Integer(stable_shape_capture_id(&shape_name))),
            span,
        );
        lowerer.set_terminator(Terminator::Branch {
            cond: matched,
            then_target: match_block,
            else_target: next_block,
            span,
        });
        lowerer.current_block = match_block;
        let mode = lowerer.shape_capture_execution_mode(CaptureQueryKind::Trace, &shape_name);
        let hit = lowerer.lower_shape_trace_call_with_mode(
            &shape_name,
            origin.clone(),
            direction.clone(),
            max_distance.clone(),
            min_step.clone(),
            hit_epsilon.clone(),
            max_steps.clone(),
            span,
            mode,
        );
        lowerer.assign_use(Place::Local(result), hit, span);
        lowerer.set_terminator(Terminator::Jump {
            target: return_block,
            span,
        });
        dispatch_block = next_block;
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
                "trace_shape requires a capture created by `capture`",
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
        abi_params: vec![
            portable_abi_from_type_ref(
                Some(&hir::TypeRef {
                    name: SmolStr::new("ShapeCapture"),
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

pub(crate) fn lower_scene_occluded_capture_helper(
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
    let helper_name = SmolStr::new("__wr_scene_occluded_capture");
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
        MirType::Named(SmolStr::new("ShapeCapture")),
    );
    let ray = lowerer.new_local(
        SmolStr::new("ray"),
        false,
        MirType::Named(SmolStr::new("RayQuery")),
    );
    for (name, local) in [
        (SmolStr::new("capture"), capture),
        (SmolStr::new("ray"), ray),
    ] {
        lowerer.declare_local(name, local);
        lowerer.params.push(local);
    }

    let entry = lowerer.new_block();
    lowerer.current_block = entry;
    let hit = lowerer.lower_call_temp(
        MirType::Named(SmolStr::new("Hit3")),
        SmolStr::new("__wr_scene_trace_capture"),
        vec![Value::Local(capture), Value::Local(ray)],
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
                    name: SmolStr::new("ShapeCapture"),
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

pub(crate) fn lower_scene_surface_capture_helper(
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
    let helper_name = SmolStr::new("__wr_scene_surface_capture");
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
        MirType::Named(SmolStr::new("ShapeCapture")),
    );
    let hit = lowerer.new_local(
        SmolStr::new("hit"),
        false,
        MirType::Named(SmolStr::new("Hit3")),
    );
    lowerer.declare_local(SmolStr::new("capture"), capture);
    lowerer.declare_local(SmolStr::new("hit"), hit);
    lowerer.params.push(capture);
    lowerer.params.push(hit);

    let entry = lowerer.new_block();
    lowerer.current_block = entry;
    let root_feature_id = lowerer.new_temp(MirType::Integer);
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(root_feature_id),
        value: Rvalue::GetField {
            base: Value::Local(capture),
            field: SmolStr::new("root_feature_id"),
            slot: lowerer.field_slot("ShapeCapture", "root_feature_id"),
        },
        span,
    });
    let invalid_capture_block = lowerer.new_block();
    let shape_capture_block = lowerer.new_block();
    let field_capture = lowerer.lower_binary_temp(
        MirType::Boolean,
        BinaryOp::Eq,
        Value::Temp(root_feature_id),
        Value::Const(Literal::Integer(0)),
        span,
    );
    lowerer.set_terminator(Terminator::Branch {
        cond: field_capture,
        then_target: invalid_capture_block,
        else_target: shape_capture_block,
        span,
    });

    lowerer.current_block = invalid_capture_block;
    let crash_temp = lowerer.new_temp(MirType::Unknown);
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(crash_temp),
        value: Rvalue::Crash {
            value: Value::Const(Literal::String(SmolStr::new(
                "surface_at requires a shape capture",
            ))),
        },
        span,
    });
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Temp(crash_temp)),
        span,
    });

    lowerer.current_block = shape_capture_block;
    let result = lowerer.new_local(
        SmolStr::new("$scene_surface_result"),
        true,
        MirType::Named(SmolStr::new("Surface")),
    );
    let default_surface = lowerer.build_default_surface(span);
    lowerer.assign_use(Place::Local(result), default_surface, span);
    let return_block = lowerer.new_block();

    let shapes_with_scene: Vec<SmolStr> = module
        .shapes
        .iter()
        .map(|(_, shape)| shape.name.clone())
        .filter(|shape_name| lowerer.shape_scene(shape_name).is_some())
        .collect();
    let mut dispatch_block = lowerer.new_block();
    lowerer.set_terminator(Terminator::Jump {
        target: dispatch_block,
        span,
    });
    for shape_name in shapes_with_scene {
        let match_block = lowerer.new_block();
        let next_block = lowerer.new_block();
        lowerer.current_block = dispatch_block;
        let matched = lowerer.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Eq,
            Value::Temp(root_feature_id),
            Value::Const(Literal::Integer(stable_shape_capture_id(&shape_name))),
            span,
        );
        lowerer.set_terminator(Terminator::Branch {
            cond: matched,
            then_target: match_block,
            else_target: next_block,
            span,
        });
        lowerer.current_block = match_block;
        let surface = lowerer.lower_call_temp(
            MirType::Named(SmolStr::new("Surface")),
            SmolStr::new(format!("__wr_shape_surface_{}", shape_name)),
            vec![Value::Local(hit)],
            span,
        );
        lowerer.assign_use(Place::Local(result), surface, span);
        lowerer.set_terminator(Terminator::Jump {
            target: return_block,
            span,
        });
        dispatch_block = next_block;
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
                "surface_at requires a capture created by `capture`",
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
        abi_params: vec![
            portable_abi_from_type_ref(
                Some(&hir::TypeRef {
                    name: SmolStr::new("ShapeCapture"),
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

pub(crate) fn lower_scene_radiance_capture_helper(
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
    let helper_name = SmolStr::new("__wr_scene_radiance_capture");
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
        MirType::Named(SmolStr::new("ShapeCapture")),
    );
    let sample = lowerer.new_local(
        SmolStr::new("sample"),
        false,
        MirType::Named(SmolStr::new("PointDirectionQuery")),
    );
    for (name, local) in [
        (SmolStr::new("capture"), capture),
        (SmolStr::new("sample"), sample),
    ] {
        lowerer.declare_local(name, local);
        lowerer.params.push(local);
    }

    let entry = lowerer.new_block();
    lowerer.current_block = entry;
    let root_feature_id = lowerer.new_temp(MirType::Integer);
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(root_feature_id),
        value: Rvalue::GetField {
            base: Value::Local(capture),
            field: SmolStr::new("root_feature_id"),
            slot: lowerer.field_slot("ShapeCapture", "root_feature_id"),
        },
        span,
    });
    let invalid_capture_block = lowerer.new_block();
    let shape_capture_block = lowerer.new_block();
    let field_capture = lowerer.lower_binary_temp(
        MirType::Boolean,
        BinaryOp::Eq,
        Value::Temp(root_feature_id),
        Value::Const(Literal::Integer(0)),
        span,
    );
    lowerer.set_terminator(Terminator::Branch {
        cond: field_capture,
        then_target: invalid_capture_block,
        else_target: shape_capture_block,
        span,
    });

    lowerer.current_block = invalid_capture_block;
    let crash_temp = lowerer.new_temp(MirType::Unknown);
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(crash_temp),
        value: Rvalue::Crash {
            value: Value::Const(Literal::String(SmolStr::new(
                "radiance_at requires a shape capture",
            ))),
        },
        span,
    });
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Temp(crash_temp)),
        span,
    });

    lowerer.current_block = shape_capture_block;
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
    let result = lowerer.new_local(SmolStr::new("$scene_radiance_result"), true, MirType::Vec3);
    let default_radiance = lowerer.lower_call_temp(
        MirType::Vec3,
        SmolStr::new("vec3"),
        vec![
            Value::Const(Literal::Float(0.0)),
            Value::Const(Literal::Float(0.0)),
            Value::Const(Literal::Float(0.0)),
        ],
        span,
    );
    lowerer.assign_use(Place::Local(result), default_radiance, span);
    let return_block = lowerer.new_block();

    let shapes_with_scene: Vec<SmolStr> = module
        .shapes
        .iter()
        .map(|(_, shape)| shape.name.clone())
        .filter(|shape_name| lowerer.shape_scene(shape_name).is_some())
        .collect();
    let mut dispatch_block = lowerer.new_block();
    lowerer.set_terminator(Terminator::Jump {
        target: dispatch_block,
        span,
    });
    for shape_name in shapes_with_scene {
        let match_block = lowerer.new_block();
        let next_block = lowerer.new_block();
        lowerer.current_block = dispatch_block;
        let matched = lowerer.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Eq,
            Value::Temp(root_feature_id),
            Value::Const(Literal::Integer(stable_shape_capture_id(&shape_name))),
            span,
        );
        lowerer.set_terminator(Terminator::Branch {
            cond: matched,
            then_target: match_block,
            else_target: next_block,
            span,
        });
        lowerer.current_block = match_block;
        let shape_scene = lowerer
            .shape_scene(&shape_name)
            .cloned()
            .expect("shape scene");
        let radiance = lowerer.lower_shape_radiance_participation_scene(
            &shape_scene.root,
            point.clone(),
            direction.clone(),
            span,
        );
        lowerer.assign_use(Place::Local(result), radiance, span);
        lowerer.set_terminator(Terminator::Jump {
            target: return_block,
            span,
        });
        dispatch_block = next_block;
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
                "radiance_at requires a capture created by `capture`",
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
        abi_params: vec![
            portable_abi_from_type_ref(
                Some(&hir::TypeRef {
                    name: SmolStr::new("ShapeCapture"),
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
        ],
        abi_return: PortableAbiType::Vec3,
        locals: lowerer.locals,
        temps: lowerer.temps,
        blocks: lowerer.blocks,
        entry,
        suspendable: false,
    }
}
