//! Owns MIR lowering for shape-focused helper queries such as distance, trace,
//! and surface evaluation against authored shapes.
//! Does not own scene/world capture helpers or batch-query lowering.
//!
//! Key invariants:
//! - shape helper lowering preserves shape execution mode and contract-selected
//!   result schemas.
//! - helper lowering may branch by backend capability, but it must not hide
//!   unsupported paths from later observability.
//! - shape helper ids remain stable so downstream codegen and diagnostics can
//!   map back to authored semantics.
//!
//! Primary entrypoints:
//! - `lower_shape_distance_helper`
//! - `lower_shape_trace_helper`
//! - `lower_shape_surface_helper`
//!
//! Failure modes / common pitfalls:
//! - borrowing scene/world capture shortcuts here can silently change shape-only
//!   semantics.
//! - bypassing shared validation helpers weakens CPU/WGSL equivalence checks.

use super::{
    BinaryOp, FunctionLowerer, HashMap, HashSet, Literal, MirFunction, MirStmt, MirType,
    PortableAbiType, ShapeExecutionMode, SmolStr, TextRange, TypeTagId, UnaryOp, Value,
    portable_abi_from_type_ref, portable_value_struct_abi, scene_ir, stable_shape_capture_id,
};
use crate::hir;
use crate::mir::ir::*;

pub(crate) fn lower_shape_distance_helper(
    shape: &hir::Shape,
    mode: ShapeExecutionMode,
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
    let helper_name = mode.distance_helper_name(&shape.name);
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

    let point = lowerer.new_local(SmolStr::new("p"), false, MirType::Vec3);
    lowerer.declare_local(SmolStr::new("p"), point);
    lowerer.params.push(point);

    let entry = lowerer.new_block();
    lowerer.current_block = entry;
    let shape_scene = lowerer
        .shape_scene(&shape.name)
        .cloned()
        .expect("shape scene");
    let distance = lowerer.lower_shape_distance_scene_in_mode(
        &shape_scene.root,
        Value::Local(point),
        span,
        mode,
    );
    lowerer.set_terminator(Terminator::Return {
        value: Some(distance),
        span,
    });

    MirFunction {
        name: helper_name,
        params: lowerer.params,
        abi_params: vec![PortableAbiType::Vec3],
        abi_return: PortableAbiType::F32,
        locals: lowerer.locals,
        temps: lowerer.temps,
        blocks: lowerer.blocks,
        entry,
        suspendable: false,
    }
}

pub(crate) fn lower_shape_trace_helper(
    shape: &hir::Shape,
    mode: ShapeExecutionMode,
    _module: &hir::Module,
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
    let helper_name = mode.trace_helper_name(&shape.name);
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

    let origin = lowerer.new_local(SmolStr::new("origin"), false, MirType::Vec3);
    let direction = lowerer.new_local(SmolStr::new("direction"), false, MirType::Vec3);
    let max_distance = lowerer.new_local(SmolStr::new("max_distance"), false, MirType::Float);
    let min_step = lowerer.new_local(SmolStr::new("min_step"), false, MirType::Float);
    let hit_epsilon = lowerer.new_local(SmolStr::new("hit_epsilon"), false, MirType::Float);
    let max_steps = lowerer.new_local(SmolStr::new("max_steps"), false, MirType::Integer);
    for (name, local) in [
        (SmolStr::new("origin"), origin),
        (SmolStr::new("direction"), direction),
        (SmolStr::new("max_distance"), max_distance),
        (SmolStr::new("min_step"), min_step),
        (SmolStr::new("hit_epsilon"), hit_epsilon),
        (SmolStr::new("max_steps"), max_steps),
    ] {
        lowerer.declare_local(name, local);
        lowerer.params.push(local);
    }

    let entry = lowerer.new_block();
    lowerer.current_block = entry;

    let _ = lowerer.lower_call_temp(
        MirType::Nil,
        SmolStr::new("__wr_metrics_scene_trace"),
        vec![],
        span,
    );
    let shape_scene = lowerer
        .shape_scene(&shape.name)
        .cloned()
        .expect("shape scene");
    match shape_scene.analysis.trace_safety {
        scene_ir::SceneTraceSafety::Exact => {
            let _ = lowerer.lower_call_temp(
                MirType::Nil,
                SmolStr::new("__wr_metrics_scene_trace_exact_path"),
                vec![],
                span,
            );
        }
        _ => {
            let _ = lowerer.lower_call_temp(
                MirType::Nil,
                SmolStr::new("__wr_metrics_scene_trace_conservative_path"),
                vec![],
                span,
            );
        }
    }
    let field_sample_metric_id = lowerer.lower_call_temp(
        MirType::Integer,
        SmolStr::new("__wr_metrics_field_sample_id"),
        vec![],
        span,
    );
    let field_samples_before = lowerer.new_local(
        SmolStr::new("$shape_field_samples_before"),
        true,
        MirType::Integer,
    );
    let field_samples_start = lowerer.lower_call_temp(
        MirType::Integer,
        SmolStr::new("__wr_metrics_get"),
        vec![field_sample_metric_id.clone()],
        span,
    );
    lowerer.assign_use(
        Place::Local(field_samples_before),
        field_samples_start,
        span,
    );

    let total = lowerer.new_local(SmolStr::new("$shape_total"), true, MirType::Float);
    lowerer.assign_use(Place::Local(total), Value::Const(Literal::Float(0.0)), span);

    let position = lowerer.new_local(SmolStr::new("$shape_position"), true, MirType::Vec3);
    lowerer.assign_use(Place::Local(position), Value::Local(origin), span);

    let step_count = lowerer.new_local(SmolStr::new("$shape_steps"), true, MirType::Integer);
    lowerer.assign_use(
        Place::Local(step_count),
        Value::Const(Literal::Integer(0)),
        span,
    );

    let loop_check = lowerer.new_block();
    let loop_body = lowerer.new_block();
    let hit_block = lowerer.new_block();
    let advance_block = lowerer.new_block();
    let continue_block = lowerer.new_block();
    let end_block = lowerer.new_block();
    lowerer.set_terminator(Terminator::Jump {
        target: loop_check,
        span,
    });

    lowerer.current_block = loop_check;
    let within_distance = lowerer.lower_binary_temp(
        MirType::Boolean,
        BinaryOp::Le,
        Value::Local(total),
        Value::Local(max_distance),
        span,
    );
    let within_steps = lowerer.lower_binary_temp(
        MirType::Boolean,
        BinaryOp::Lt,
        Value::Local(step_count),
        Value::Local(max_steps),
        span,
    );
    let cond = lowerer.lower_binary_temp(
        MirType::Boolean,
        BinaryOp::And,
        within_distance,
        within_steps,
        span,
    );
    lowerer.set_terminator(Terminator::Branch {
        cond,
        then_target: loop_body,
        else_target: end_block,
        span,
    });

    lowerer.current_block = loop_body;
    let scaled_direction = lowerer.lower_binary_temp(
        MirType::Vec3,
        BinaryOp::Mul,
        Value::Local(direction),
        Value::Local(total),
        span,
    );
    let next_position = lowerer.lower_binary_temp(
        MirType::Vec3,
        BinaryOp::Add,
        Value::Local(origin),
        scaled_direction,
        span,
    );
    lowerer.assign_use(Place::Local(position), next_position, span);
    let sampled_distance = lowerer.lower_shape_distance_call_with_mode(
        &shape.name,
        Value::Local(position),
        span,
        mode,
    );
    let is_hit = lowerer.lower_binary_temp(
        MirType::Boolean,
        BinaryOp::Le,
        sampled_distance.clone(),
        Value::Local(hit_epsilon),
        span,
    );
    lowerer.set_terminator(Terminator::Branch {
        cond: is_hit,
        then_target: hit_block,
        else_target: advance_block,
        span,
    });

    lowerer.current_block = hit_block;
    let (_, payload_value, feature_id_value) = lowerer.lower_shape_payload_selection_scene(
        &shape_scene.root,
        shape_scene.provenance.as_ref(),
        Value::Local(position),
        Value::Local(hit_epsilon),
        span,
    );
    let normal_value =
        lowerer.lower_shape_normal_call_with_mode(&shape.name, Value::Local(position), span, mode);
    let (_, local_position_value, local_normal_value, instance_id_value, repeat_id_value) = lowerer
        .lower_shape_hit_context_selection_scene(
            &shape_scene.root,
            feature_id_value.clone(),
            Value::Local(position),
            span,
        );
    let mut hit_class = lowerer.synthetic_class_target_info("Hit3");
    FunctionLowerer::set_class_field_value(
        &mut hit_class,
        "hit",
        Value::Const(Literal::Boolean(true)),
    );
    FunctionLowerer::set_class_field_value(&mut hit_class, "distance", Value::Local(total));
    FunctionLowerer::set_class_field_value(&mut hit_class, "position", Value::Local(position));
    FunctionLowerer::set_class_field_value(&mut hit_class, "normal", normal_value.clone());
    FunctionLowerer::set_class_field_value(&mut hit_class, "local_position", local_position_value);
    FunctionLowerer::set_class_field_value(&mut hit_class, "local_normal", local_normal_value);
    let shading_frame =
        lowerer.lower_stable_surface_frame(Value::Local(position), normal_value, span);
    FunctionLowerer::set_class_field_value(&mut hit_class, "shading_frame", shading_frame);
    FunctionLowerer::set_class_field_value(&mut hit_class, "steps", Value::Local(step_count));
    FunctionLowerer::set_class_field_value(&mut hit_class, "feature_id", feature_id_value);
    FunctionLowerer::set_class_field_value(&mut hit_class, "instance_id", instance_id_value);
    FunctionLowerer::set_class_field_value(&mut hit_class, "repeat_id", repeat_id_value);
    FunctionLowerer::set_class_field_value(
        &mut hit_class,
        "root_shape_id",
        Value::Const(Literal::Integer(stable_shape_capture_id(&shape.name))),
    );
    FunctionLowerer::set_class_field_value(&mut hit_class, "payload", payload_value);
    let hit_value = lowerer.build_class_instance(&hit_class, span);
    let field_samples_after = lowerer.lower_call_temp(
        MirType::Integer,
        SmolStr::new("__wr_metrics_get"),
        vec![field_sample_metric_id.clone()],
        span,
    );
    let field_sample_delta = lowerer.lower_binary_temp(
        MirType::Integer,
        BinaryOp::Sub,
        field_samples_after,
        Value::Local(field_samples_before),
        span,
    );
    let _ = lowerer.lower_call_temp(
        MirType::Nil,
        SmolStr::new("__wr_metrics_scene_trace_hit"),
        vec![Value::Local(step_count), field_sample_delta],
        span,
    );
    lowerer.set_terminator(Terminator::Return {
        value: Some(hit_value),
        span,
    });

    lowerer.current_block = advance_block;
    let step = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("max"),
        vec![sampled_distance, Value::Local(min_step)],
        span,
    );
    let next_total = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Add,
        Value::Local(total),
        step,
        span,
    );
    lowerer.assign_use(Place::Local(total), next_total, span);
    let next_steps = lowerer.lower_binary_temp(
        MirType::Integer,
        BinaryOp::Add,
        Value::Local(step_count),
        Value::Const(Literal::Integer(1)),
        span,
    );
    lowerer.assign_use(Place::Local(step_count), next_steps, span);
    lowerer.set_terminator(Terminator::Jump {
        target: continue_block,
        span,
    });

    lowerer.current_block = continue_block;
    lowerer.set_terminator(Terminator::Jump {
        target: loop_check,
        span,
    });

    lowerer.current_block = end_block;
    let miss_value = lowerer.build_default_hit(Value::Local(origin), span);
    lowerer.set_terminator(Terminator::Return {
        value: Some(miss_value),
        span,
    });

    MirFunction {
        name: helper_name,
        params: lowerer.params,
        abi_params: vec![
            PortableAbiType::Vec3,
            PortableAbiType::Vec3,
            PortableAbiType::F32,
            PortableAbiType::F32,
            PortableAbiType::F32,
            PortableAbiType::I32,
        ],
        abi_return: PortableAbiType::Struct {
            name: SmolStr::new("Hit3"),
            class_id: type_tags
                .get(&SmolStr::new("Hit3"))
                .map(|id| id.0 as u32)
                .unwrap_or_default(),
            fields: portable_value_struct_abi("Hit3", _module, type_tags, &mut HashSet::new())
                .and_then(|abi| match abi {
                    PortableAbiType::Struct { fields, .. } => Some(fields),
                    _ => None,
                })
                .unwrap_or_default(),
        },
        locals: lowerer.locals,
        temps: lowerer.temps,
        blocks: lowerer.blocks,
        entry,
        suspendable: false,
    }
}

pub(crate) fn lower_shape_surface_helper(
    shape: &hir::Shape,
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
    let helper_name = SmolStr::new(format!("__wr_shape_surface_{}", shape.name));
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

    let hit = lowerer.new_local(
        SmolStr::new("hit"),
        false,
        MirType::Named(SmolStr::new("Hit3")),
    );
    lowerer.declare_local(SmolStr::new("hit"), hit);
    lowerer.params.push(hit);

    let entry = lowerer.new_block();
    lowerer.current_block = entry;
    let feature_id_temp = lowerer.new_temp(MirType::Integer);
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(feature_id_temp),
        value: Rvalue::GetField {
            base: Value::Local(hit),
            field: SmolStr::new("feature_id"),
            slot: lowerer.field_slot("Hit3", "feature_id"),
        },
        span,
    });
    let shape_scene = lowerer
        .shape_scene(&shape.name)
        .cloned()
        .expect("shape scene");
    let (_, surface) = lowerer.lower_shape_surface_selection_scene(
        &shape_scene.root,
        Value::Temp(feature_id_temp),
        Value::Local(hit),
        span,
    );
    lowerer.set_terminator(Terminator::Return {
        value: Some(surface),
        span,
    });

    MirFunction {
        name: helper_name,
        params: lowerer.params,
        abi_params: vec![portable_abi_from_type_ref(
            Some(&hir::TypeRef {
                name: SmolStr::new("Hit3"),
                name_span: None,
                args: Vec::new(),
            }),
            module,
            type_tags,
            &mut HashSet::new(),
        )],
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

impl FunctionLowerer {
    pub(crate) fn lower_stable_surface_frame(
        &mut self,
        position: Value,
        normal: Value,
        span: TextRange,
    ) -> Value {
        let zero = Value::Const(Literal::Float(0.0));
        let one = Value::Const(Literal::Float(1.0));

        let unit_normal =
            self.lower_call_temp(MirType::Vec3, SmolStr::new("normalize"), vec![normal], span);
        let world_up = self.lower_call_temp(
            MirType::Vec3,
            SmolStr::new("vec3"),
            vec![zero.clone(), one.clone(), zero.clone()],
            span,
        );
        let world_right = self.lower_call_temp(
            MirType::Vec3,
            SmolStr::new("vec3"),
            vec![one.clone(), zero.clone(), zero.clone()],
            span,
        );
        let tangent_seed = self.lower_call_temp(
            MirType::Vec3,
            SmolStr::new("cross"),
            vec![world_up, unit_normal.clone()],
            span,
        );
        let tangent_seed_len = self.lower_call_temp(
            MirType::Float,
            SmolStr::new("length"),
            vec![tangent_seed.clone()],
            span,
        );
        let tangent = self.new_local(SmolStr::new("$surface_frame_tangent"), true, MirType::Vec3);
        self.assign_use(Place::Local(tangent), tangent_seed, span);
        let tangent_fallback_block = self.new_block();
        let tangent_normalize_block = self.new_block();
        let tangent_merge_block = self.new_block();
        let needs_fallback = self.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Eq,
            tangent_seed_len,
            zero.clone(),
            span,
        );
        self.set_terminator(Terminator::Branch {
            cond: needs_fallback,
            then_target: tangent_fallback_block,
            else_target: tangent_normalize_block,
            span,
        });
        self.current_block = tangent_fallback_block;
        let tangent_fallback = self.lower_call_temp(
            MirType::Vec3,
            SmolStr::new("cross"),
            vec![world_right, unit_normal.clone()],
            span,
        );
        self.assign_use(Place::Local(tangent), tangent_fallback, span);
        self.set_terminator(Terminator::Jump {
            target: tangent_normalize_block,
            span,
        });
        self.current_block = tangent_normalize_block;
        let tangent_normalized = self.lower_call_temp(
            MirType::Vec3,
            SmolStr::new("normalize"),
            vec![Value::Local(tangent)],
            span,
        );
        self.assign_use(Place::Local(tangent), tangent_normalized, span);
        self.set_terminator(Terminator::Jump {
            target: tangent_merge_block,
            span,
        });
        self.current_block = tangent_merge_block;

        let bitangent = self.lower_call_temp(
            MirType::Vec3,
            SmolStr::new("cross"),
            vec![unit_normal.clone(), Value::Local(tangent)],
            span,
        );

        let tangent_x = self.lower_call_temp(
            MirType::Float,
            SmolStr::new("__wr_vec_component"),
            vec![Value::Local(tangent), Value::Const(Literal::Integer(0))],
            span,
        );
        let tangent_y = self.lower_call_temp(
            MirType::Float,
            SmolStr::new("__wr_vec_component"),
            vec![Value::Local(tangent), Value::Const(Literal::Integer(1))],
            span,
        );
        let tangent_z = self.lower_call_temp(
            MirType::Float,
            SmolStr::new("__wr_vec_component"),
            vec![Value::Local(tangent), Value::Const(Literal::Integer(2))],
            span,
        );
        let bitangent_x = self.lower_call_temp(
            MirType::Float,
            SmolStr::new("__wr_vec_component"),
            vec![bitangent.clone(), Value::Const(Literal::Integer(0))],
            span,
        );
        let bitangent_y = self.lower_call_temp(
            MirType::Float,
            SmolStr::new("__wr_vec_component"),
            vec![bitangent.clone(), Value::Const(Literal::Integer(1))],
            span,
        );
        let bitangent_z = self.lower_call_temp(
            MirType::Float,
            SmolStr::new("__wr_vec_component"),
            vec![bitangent.clone(), Value::Const(Literal::Integer(2))],
            span,
        );
        let normal_x = self.lower_call_temp(
            MirType::Float,
            SmolStr::new("__wr_vec_component"),
            vec![unit_normal.clone(), Value::Const(Literal::Integer(0))],
            span,
        );
        let normal_y = self.lower_call_temp(
            MirType::Float,
            SmolStr::new("__wr_vec_component"),
            vec![unit_normal.clone(), Value::Const(Literal::Integer(1))],
            span,
        );
        let normal_z = self.lower_call_temp(
            MirType::Float,
            SmolStr::new("__wr_vec_component"),
            vec![unit_normal.clone(), Value::Const(Literal::Integer(2))],
            span,
        );
        let position_x = self.lower_call_temp(
            MirType::Float,
            SmolStr::new("__wr_vec_component"),
            vec![position.clone(), Value::Const(Literal::Integer(0))],
            span,
        );
        let position_y = self.lower_call_temp(
            MirType::Float,
            SmolStr::new("__wr_vec_component"),
            vec![position.clone(), Value::Const(Literal::Integer(1))],
            span,
        );
        let position_z = self.lower_call_temp(
            MirType::Float,
            SmolStr::new("__wr_vec_component"),
            vec![position, Value::Const(Literal::Integer(2))],
            span,
        );
        let position_vec = self.lower_call_temp(
            MirType::Vec3,
            SmolStr::new("vec3"),
            vec![position_x.clone(), position_y.clone(), position_z.clone()],
            span,
        );

        let column_0 = self.lower_call_temp(
            MirType::Vec4,
            SmolStr::new("vec4"),
            vec![
                tangent_x.clone(),
                tangent_y.clone(),
                tangent_z.clone(),
                zero.clone(),
            ],
            span,
        );
        let column_1 = self.lower_call_temp(
            MirType::Vec4,
            SmolStr::new("vec4"),
            vec![
                bitangent_x.clone(),
                bitangent_y.clone(),
                bitangent_z.clone(),
                zero.clone(),
            ],
            span,
        );
        let column_2 = self.lower_call_temp(
            MirType::Vec4,
            SmolStr::new("vec4"),
            vec![
                normal_x.clone(),
                normal_y.clone(),
                normal_z.clone(),
                zero.clone(),
            ],
            span,
        );
        let dot_tangent = self.lower_call_temp(
            MirType::Float,
            SmolStr::new("dot"),
            vec![Value::Local(tangent), position_vec.clone()],
            span,
        );
        let dot_bitangent = self.lower_call_temp(
            MirType::Float,
            SmolStr::new("dot"),
            vec![bitangent, position_vec.clone()],
            span,
        );
        let dot_normal = self.lower_call_temp(
            MirType::Float,
            SmolStr::new("dot"),
            vec![unit_normal.clone(), position_vec],
            span,
        );
        let neg_dot_tangent =
            self.lower_unary_temp(MirType::Float, UnaryOp::Neg, dot_tangent, span);
        let neg_dot_bitangent =
            self.lower_unary_temp(MirType::Float, UnaryOp::Neg, dot_bitangent, span);
        let neg_dot_normal = self.lower_unary_temp(MirType::Float, UnaryOp::Neg, dot_normal, span);
        let column_3 = self.lower_call_temp(
            MirType::Vec4,
            SmolStr::new("vec4"),
            vec![
                neg_dot_tangent.clone(),
                neg_dot_bitangent.clone(),
                neg_dot_normal.clone(),
                one.clone(),
            ],
            span,
        );
        let matrix = self.lower_call_temp(
            MirType::Mat4,
            SmolStr::new("mat4_cols"),
            vec![column_0, column_1, column_2, column_3],
            span,
        );
        let inverse_col_0 = self.lower_call_temp(
            MirType::Vec4,
            SmolStr::new("vec4"),
            vec![tangent_x, bitangent_x, normal_x, zero.clone()],
            span,
        );
        let inverse_col_1 = self.lower_call_temp(
            MirType::Vec4,
            SmolStr::new("vec4"),
            vec![tangent_y, bitangent_y, normal_y, zero.clone()],
            span,
        );
        let inverse_col_2 = self.lower_call_temp(
            MirType::Vec4,
            SmolStr::new("vec4"),
            vec![tangent_z, bitangent_z, normal_z, zero.clone()],
            span,
        );
        let inverse_col_3 = self.lower_call_temp(
            MirType::Vec4,
            SmolStr::new("vec4"),
            vec![neg_dot_tangent, neg_dot_bitangent, neg_dot_normal, one],
            span,
        );
        let inverse = self.lower_call_temp(
            MirType::Mat4,
            SmolStr::new("mat4_cols"),
            vec![inverse_col_0, inverse_col_1, inverse_col_2, inverse_col_3],
            span,
        );

        let mut class = self.synthetic_class_target_info("Transform3");
        Self::set_class_field_value(&mut class, "matrix", matrix);
        Self::set_class_field_value(&mut class, "inverse", inverse);
        self.build_class_instance(&class, span)
    }
}
