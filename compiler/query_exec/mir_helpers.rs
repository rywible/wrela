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
    let origin = lowerer.lower_get_named_field(
        Value::Local(ray),
        "RayQuery",
        "origin",
        MirType::Vec3,
        span,
    );
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

pub(crate) fn lower_scene_medium_capture_helper(
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
    let helper_name = SmolStr::new("__wr_scene_medium_capture");
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
    let point = lowerer.new_local(SmolStr::new("point"), false, MirType::Vec3);
    lowerer.declare_local(SmolStr::new("capture"), capture);
    lowerer.declare_local(SmolStr::new("point"), point);
    lowerer.params.push(capture);
    lowerer.params.push(point);

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
                "medium_at requires a shape capture",
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
        SmolStr::new("$scene_medium_result"),
        true,
        MirType::Named(SmolStr::new("Medium")),
    );
    let default_medium = lowerer.build_default_medium(span);
    lowerer.assign_use(Place::Local(result), default_medium, span);
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
        let shape_scene = lowerer
            .shape_scene(&shape_name)
            .cloned()
            .expect("shape scene");
        let medium = lowerer.lower_shape_medium_participation_scene(
            &shape_scene.root,
            Value::Local(point),
            span,
        );
        lowerer.assign_use(Place::Local(result), medium, span);
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
                "medium_at requires a capture created by `capture`",
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
            PortableAbiType::Vec3,
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

fn lower_world_domain_validation(
    lowerer: &mut FunctionLowerer,
    capture: LocalId,
    domain: LocalId,
    query_name: &str,
    span: TextRange,
) -> (Value, Value) {
    let capture_scene_id = lowerer.lower_get_named_field(
        Value::Local(capture),
        "RegionCapture",
        "scene_id",
        MirType::Integer,
        span,
    );
    let domain_scene_id = lowerer.lower_get_named_field(
        Value::Local(domain),
        "SceneDomain",
        "scene_id",
        MirType::Integer,
        span,
    );
    let scene_ids_match = lowerer.lower_binary_temp(
        MirType::Boolean,
        BinaryOp::Eq,
        capture_scene_id.clone(),
        domain_scene_id,
        span,
    );
    let matched_block = lowerer.new_block();
    let mismatch_block = lowerer.new_block();
    lowerer.set_terminator(Terminator::Branch {
        cond: scene_ids_match,
        then_target: matched_block,
        else_target: mismatch_block,
        span,
    });

    lowerer.current_block = mismatch_block;
    let crash_temp = lowerer.new_temp(MirType::Unknown);
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(crash_temp),
        value: Rvalue::Crash {
            value: Value::Const(Literal::String(SmolStr::new(
                world_domain_mismatch_message(query_name),
            ))),
        },
        span,
    });
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Temp(crash_temp)),
        span,
    });

    lowerer.current_block = matched_block;
    let spatial = lowerer.lower_get_named_field(
        Value::Local(domain),
        "SceneDomain",
        "spatial",
        MirType::Named(SmolStr::new("SpatialDomainContract")),
        span,
    );
    let detail = lowerer.lower_get_named_field(
        spatial,
        "SpatialDomainContract",
        "geometry_detail",
        MirType::Integer,
        span,
    );
    (capture_scene_id, detail)
}

fn lower_world_domain_flag_guard(
    lowerer: &mut FunctionLowerer,
    domain: LocalId,
    flag: &str,
    disabled_return: Value,
    span: TextRange,
) {
    let (contract_name, contract_field) = match flag {
        "material" => ("SurfaceDomainContract", "surface"),
        "radiance" | "media" => ("ParticipantDomainContract", "participants"),
        other => panic!("unknown SceneDomain flag '{other}'"),
    };
    let contract = lowerer.lower_get_named_field(
        Value::Local(domain),
        "SceneDomain",
        contract_field,
        MirType::Named(SmolStr::new(contract_name)),
        span,
    );
    let enabled = lowerer.lower_get_named_field(
        contract,
        contract_name,
        flag,
        MirType::Boolean,
        span,
    );
    let enabled_block = lowerer.new_block();
    let disabled_block = lowerer.new_block();
    lowerer.set_terminator(Terminator::Branch {
        cond: enabled,
        then_target: enabled_block,
        else_target: disabled_block,
        span,
    });

    lowerer.current_block = disabled_block;
    lowerer.set_terminator(Terminator::Return {
        value: Some(disabled_return),
        span,
    });

    lowerer.current_block = enabled_block;
}

fn lower_world_region_dispatch<F>(
    lowerer: &mut FunctionLowerer,
    module: &hir::Module,
    capture_scene_id: Value,
    detail: Value,
    return_block: BlockId,
    invalid_message: &str,
    span: TextRange,
    mut emit_shapes: F,
) where
    F: FnMut(&mut FunctionLowerer, &[SmolStr], TextRange),
{
    let mut dispatch_block = lowerer.new_block();
    lowerer.set_terminator(Terminator::Jump {
        target: dispatch_block,
        span,
    });

    for region_case in build_region_exec_cases(module) {
        let match_block = lowerer.new_block();
        let next_block = lowerer.new_block();
        lowerer.current_block = dispatch_block;
        let matched = lowerer.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Eq,
            capture_scene_id.clone(),
            Value::Const(Literal::Integer(i64::from(region_case.scene_id))),
            span,
        );
        lowerer.set_terminator(Terminator::Branch {
            cond: matched,
            then_target: match_block,
            else_target: next_block,
            span,
        });

        lowerer.current_block = match_block;
        let (coarse_shapes, fine_shapes) = match &region_case.shapes {
            Ok(shapes) => (&shapes.coarse, &shapes.fine),
            Err(message) => {
                let crash_temp = lowerer.new_temp(MirType::Unknown);
                lowerer.push_stmt(MirStmt::Assign {
                    place: Place::Temp(crash_temp),
                    value: Rvalue::Crash {
                        value: Value::Const(Literal::String(message.clone())),
                    },
                    span,
                });
                lowerer.set_terminator(Terminator::Return {
                    value: Some(Value::Temp(crash_temp)),
                    span,
                });
                dispatch_block = next_block;
                continue;
            }
        };

        let coarse_block = lowerer.new_block();
        let fine_block = lowerer.new_block();
        let detail_is_coarse = lowerer.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Eq,
            detail.clone(),
            Value::Const(Literal::Integer(0)),
            span,
        );
        lowerer.set_terminator(Terminator::Branch {
            cond: detail_is_coarse,
            then_target: coarse_block,
            else_target: fine_block,
            span,
        });

        for (shapes, block) in [(coarse_shapes, coarse_block), (fine_shapes, fine_block)] {
            lowerer.current_block = block;
            emit_shapes(lowerer, shapes, span);
            lowerer.set_terminator(Terminator::Jump {
                target: return_block,
                span,
            });
        }

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
            value: Value::Const(Literal::String(SmolStr::new(invalid_message))),
        },
        span,
    });
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Temp(crash_temp)),
        span,
    });
}

struct MirWorldDistanceBackend<'a> {
    lowerer: &'a mut FunctionLowerer,
    point: Value,
    result: LocalId,
    span: TextRange,
}

impl WorldDistanceBackend for MirWorldDistanceBackend<'_> {
    type Error = std::convert::Infallible;

    fn init_world_distance(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn accumulate_world_distance_shape(&mut self, shape: &SmolStr) -> Result<(), Self::Error> {
        let distance = self
            .lowerer
            .lower_shape_distance_call(shape, self.point.clone(), self.span);
        let next = self.lowerer.lower_call_temp(
            MirType::Float,
            SmolStr::new("min"),
            vec![Value::Local(self.result), distance],
            self.span,
        );
        self.lowerer
            .assign_use(Place::Local(self.result), next, self.span);
        Ok(())
    }
}

struct MirWorldNormalBackend<'a> {
    lowerer: &'a mut FunctionLowerer,
    capture: LocalId,
    domain: LocalId,
    point: LocalId,
    backend: LocalId,
    span: TextRange,
}

impl WorldNormalBackend for MirWorldNormalBackend<'_> {
    type Error = std::convert::Infallible;
    type Point = Value;
    type Distance = Value;
    type Normal = Value;

    fn base_point(&mut self) -> Result<Self::Point, Self::Error> {
        Ok(Value::Local(self.point))
    }

    fn offset_point(
        &mut self,
        point: &Self::Point,
        axis: usize,
        delta: f32,
    ) -> Result<Self::Point, Self::Error> {
        let offset = match axis {
            0 => self.lowerer.lower_call_temp(
                MirType::Vec3,
                SmolStr::new("vec3"),
                vec![
                    Value::Const(Literal::Float(f64::from(delta))),
                    Value::Const(Literal::Float(0.0)),
                    Value::Const(Literal::Float(0.0)),
                ],
                self.span,
            ),
            1 => self.lowerer.lower_call_temp(
                MirType::Vec3,
                SmolStr::new("vec3"),
                vec![
                    Value::Const(Literal::Float(0.0)),
                    Value::Const(Literal::Float(f64::from(delta))),
                    Value::Const(Literal::Float(0.0)),
                ],
                self.span,
            ),
            _ => self.lowerer.lower_call_temp(
                MirType::Vec3,
                SmolStr::new("vec3"),
                vec![
                    Value::Const(Literal::Float(0.0)),
                    Value::Const(Literal::Float(0.0)),
                    Value::Const(Literal::Float(f64::from(delta))),
                ],
                self.span,
            ),
        };
        Ok(self.lowerer.lower_binary_temp(
            MirType::Vec3,
            BinaryOp::Add,
            point.clone(),
            offset,
            self.span,
        ))
    }

    fn sample_world_distance(&mut self, point: Self::Point) -> Result<Self::Distance, Self::Error> {
        Ok(self.lowerer.lower_call_temp(
            MirType::Float,
            SmolStr::new("__wr_world_distance_capture"),
            vec![
                Value::Local(self.capture),
                Value::Local(self.domain),
                point,
                Value::Local(self.backend),
            ],
            self.span,
        ))
    }

    fn subtract_distance(
        &mut self,
        positive: Self::Distance,
        negative: Self::Distance,
    ) -> Result<Self::Distance, Self::Error> {
        Ok(self
            .lowerer
            .lower_binary_temp(MirType::Float, BinaryOp::Sub, positive, negative, self.span))
    }

    fn compose_normal(
        &mut self,
        x: Self::Distance,
        y: Self::Distance,
        z: Self::Distance,
    ) -> Result<Self::Normal, Self::Error> {
        Ok(self.lowerer.lower_call_temp(
            MirType::Vec3,
            SmolStr::new("vec3"),
            vec![x, y, z],
            self.span,
        ))
    }

    fn normalize_normal(&mut self, normal: Self::Normal) -> Result<Self::Normal, Self::Error> {
        Ok(self.lowerer.lower_call_temp(
            MirType::Vec3,
            SmolStr::new("normalize"),
            vec![normal],
            self.span,
        ))
    }
}

struct MirWorldTraceBackend<'a> {
    lowerer: &'a mut FunctionLowerer,
    origin: Value,
    direction: Value,
    max_distance: Value,
    min_step: Value,
    hit_epsilon: Value,
    max_steps: Value,
    result: LocalId,
    span: TextRange,
}

impl WorldTraceBackend for MirWorldTraceBackend<'_> {
    type Error = std::convert::Infallible;

    fn init_world_trace(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn consider_world_trace_shape(&mut self, shape: &SmolStr) -> Result<(), Self::Error> {
        let candidate = self.lowerer.lower_call_temp(
            MirType::Named(SmolStr::new("Hit3")),
            SmolStr::new(format!("__wr_shape_trace_{}", shape)),
            vec![
                self.origin.clone(),
                self.direction.clone(),
                self.max_distance.clone(),
                self.min_step.clone(),
                self.hit_epsilon.clone(),
                self.max_steps.clone(),
            ],
            self.span,
        );
        let candidate_hit = self.lowerer.lower_get_named_field(
            candidate.clone(),
            "Hit3",
            "hit",
            MirType::Boolean,
            self.span,
        );
        let current_hit = self.lowerer.lower_get_named_field(
            Value::Local(self.result),
            "Hit3",
            "hit",
            MirType::Boolean,
            self.span,
        );
        let candidate_distance = self.lowerer.lower_get_named_field(
            candidate.clone(),
            "Hit3",
            "distance",
            MirType::Float,
            self.span,
        );
        let current_distance = self.lowerer.lower_get_named_field(
            Value::Local(self.result),
            "Hit3",
            "distance",
            MirType::Float,
            self.span,
        );
        let current_miss = self.lowerer.lower_unary_temp(
            MirType::Boolean,
            UnaryOp::Not,
            current_hit,
            self.span,
        );
        let candidate_nearer = self.lowerer.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Lt,
            candidate_distance,
            current_distance,
            self.span,
        );
        let replace = self.lowerer.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Or,
            current_miss,
            candidate_nearer,
            self.span,
        );
        let should_take = self.lowerer.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::And,
            candidate_hit,
            replace,
            self.span,
        );
        let take_block = self.lowerer.new_block();
        let skip_block = self.lowerer.new_block();
        let merge_block = self.lowerer.new_block();
        self.lowerer.set_terminator(Terminator::Branch {
            cond: should_take,
            then_target: take_block,
            else_target: skip_block,
            span: self.span,
        });
        self.lowerer.current_block = take_block;
        self.lowerer
            .assign_use(Place::Local(self.result), candidate, self.span);
        self.lowerer.set_terminator(Terminator::Jump {
            target: merge_block,
            span: self.span,
        });
        self.lowerer.current_block = skip_block;
        self.lowerer.set_terminator(Terminator::Jump {
            target: merge_block,
            span: self.span,
        });
        self.lowerer.current_block = merge_block;
        Ok(())
    }
}

struct MirWorldSurfaceBackend<'a> {
    lowerer: &'a mut FunctionLowerer,
    hit: LocalId,
    root_shape_id: Value,
    result: LocalId,
    span: TextRange,
}

impl WorldSurfaceBackend for MirWorldSurfaceBackend<'_> {
    type Error = std::convert::Infallible;

    fn init_world_surface(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn consider_world_surface_shape(&mut self, shape: &SmolStr) -> Result<(), Self::Error> {
        let match_block = self.lowerer.new_block();
        let next_block = self.lowerer.new_block();
        let matched = self.lowerer.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Eq,
            self.root_shape_id.clone(),
            Value::Const(Literal::Integer(stable_shape_capture_id(shape))),
            self.span,
        );
        self.lowerer.set_terminator(Terminator::Branch {
            cond: matched,
            then_target: match_block,
            else_target: next_block,
            span: self.span,
        });
        self.lowerer.current_block = match_block;
        let surface = self.lowerer.lower_call_temp(
            MirType::Named(SmolStr::new("Surface")),
            SmolStr::new(format!("__wr_shape_surface_{}", shape)),
            vec![Value::Local(self.hit)],
            self.span,
        );
        self.lowerer
            .assign_use(Place::Local(self.result), surface, self.span);
        let merge_block = self.lowerer.new_block();
        self.lowerer.set_terminator(Terminator::Jump {
            target: merge_block,
            span: self.span,
        });
        self.lowerer.current_block = next_block;
        self.lowerer.set_terminator(Terminator::Jump {
            target: merge_block,
            span: self.span,
        });
        self.lowerer.current_block = merge_block;
        Ok(())
    }
}

struct MirWorldRadianceBackend<'a> {
    lowerer: &'a mut FunctionLowerer,
    point: Value,
    direction: Value,
    result: LocalId,
    span: TextRange,
}

impl WorldRadianceBackend for MirWorldRadianceBackend<'_> {
    type Error = std::convert::Infallible;

    fn init_world_radiance(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn accumulate_world_radiance_shape(&mut self, shape: &SmolStr) -> Result<(), Self::Error> {
        if let Some(scene) = self.lowerer.shape_scene(shape).cloned() {
            let radiance = self.lowerer.lower_shape_radiance_participation_scene(
                &scene.root,
                self.point.clone(),
                self.direction.clone(),
                self.span,
            );
            let sum = self.lowerer.lower_binary_temp(
                MirType::Vec3,
                BinaryOp::Add,
                Value::Local(self.result),
                radiance,
                self.span,
            );
            self.lowerer
                .assign_use(Place::Local(self.result), sum, self.span);
        }
        Ok(())
    }
}

struct MirWorldMediumBackend<'a> {
    lowerer: &'a mut FunctionLowerer,
    point: LocalId,
    result: LocalId,
    span: TextRange,
}

impl WorldMediumBackend for MirWorldMediumBackend<'_> {
    type Error = std::convert::Infallible;

    fn init_world_medium(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn accumulate_world_medium_shape(&mut self, shape: &SmolStr) -> Result<(), Self::Error> {
        if let Some(scene) = self.lowerer.shape_scene(shape).cloned() {
            let medium = self.lowerer.lower_shape_medium_participation_scene(
                &scene.root,
                Value::Local(self.point),
                self.span,
            );
            let merged = self.lowerer.lower_additive_medium_combine(
                Value::Local(self.result),
                medium,
                self.span,
            );
            self.lowerer
                .assign_use(Place::Local(self.result), merged, self.span);
        }
        Ok(())
    }
}

fn lower_wgsl_bridge_failure(
    lowerer: &mut FunctionLowerer,
    message: SmolStr,
    span: TextRange,
) {
    let crash_temp = lowerer.new_temp(MirType::Unknown);
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(crash_temp),
        value: Rvalue::Crash {
            value: Value::Const(Literal::String(message)),
        },
        span,
    });
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Temp(crash_temp)),
        span,
    });
}

fn world_auto_backend(default_backend: DispatchBackend) -> DispatchBackend {
    match default_backend {
        DispatchBackend::Wgsl => DispatchBackend::Wgsl,
        DispatchBackend::Cpu | DispatchBackend::VirtualGpu | DispatchBackend::Auto => {
            DispatchBackend::Cpu
        }
    }
}

fn batch_auto_backend(default_backend: DispatchBackend) -> DispatchBackend {
    match default_backend {
        DispatchBackend::Cpu => DispatchBackend::Cpu,
        DispatchBackend::VirtualGpu => DispatchBackend::VirtualGpu,
        DispatchBackend::Wgsl => DispatchBackend::Wgsl,
        DispatchBackend::Auto => DispatchBackend::Cpu,
    }
}

fn lower_runtime_u32_list(
    lowerer: &mut FunctionLowerer,
    debug_name: &str,
    values: &[u32],
    span: TextRange,
) -> Value {
    let local = lowerer.new_local(
        SmolStr::new(format!("{debug_name}{}", lowerer.locals.len())),
        true,
        MirType::Named(SmolStr::new("List")),
    );
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Local(local),
        value: Rvalue::BuildList {
            items: values
                .iter()
                .map(|value| Value::Const(Literal::Integer(i64::from(*value))))
                .collect(),
            alloc: AllocKind::Escaping,
        },
        span,
    });
    Value::Local(local)
}

fn lower_world_shape_index_list(
    lowerer: &mut FunctionLowerer,
    shapes: &[SmolStr],
    shape_indices: &HashMap<SmolStr, u32>,
    span: TextRange,
) -> Value {
    let values = shapes
        .iter()
        .map(|shape| {
            *shape_indices.get(shape).unwrap_or_else(|| {
                panic!("missing WGSL shape index for world shape '{}'", shape)
            })
        })
        .collect::<Vec<_>>();
    lower_runtime_u32_list(lowerer, "$world_shape_indices", &values, span)
}

fn lower_capture_index_lookup(
    lowerer: &mut FunctionLowerer,
    capture: LocalId,
    capture_type: &str,
    capture_field: &str,
    capture_indices: &HashMap<SmolStr, u32>,
    stable_capture_id: fn(&SmolStr) -> i64,
    invalid_message: &str,
    span: TextRange,
) -> Value {
    let capture_id = lowerer.lower_get_named_field(
        Value::Local(capture),
        capture_type,
        capture_field,
        MirType::Integer,
        span,
    );
    let result = lowerer.new_local(
        SmolStr::new(format!("$capture_index{}", lowerer.locals.len())),
        true,
        MirType::Integer,
    );
    let join_block = lowerer.new_block();
    let mut dispatch_block = lowerer.new_block();
    lowerer.set_terminator(Terminator::Jump {
        target: dispatch_block,
        span,
    });

    let mut cases = capture_indices
        .iter()
        .map(|(name, index)| (stable_capture_id(name), *index))
        .collect::<Vec<_>>();
    cases.sort_by_key(|(stable_id, _)| *stable_id);
    for (stable_id, index) in cases {
        let match_block = lowerer.new_block();
        let next_block = lowerer.new_block();
        lowerer.current_block = dispatch_block;
        let matched = lowerer.lower_binary_temp(
            MirType::Boolean,
            BinaryOp::Eq,
            capture_id.clone(),
            Value::Const(Literal::Integer(stable_id)),
            span,
        );
        lowerer.set_terminator(Terminator::Branch {
            cond: matched,
            then_target: match_block,
            else_target: next_block,
            span,
        });

        lowerer.current_block = match_block;
        lowerer.assign_use(
            Place::Local(result),
            Value::Const(Literal::Integer(i64::from(index))),
            span,
        );
        lowerer.set_terminator(Terminator::Jump {
            target: join_block,
            span,
        });
        dispatch_block = next_block;
    }

    lowerer.current_block = dispatch_block;
    lower_wgsl_bridge_failure(lowerer, SmolStr::new(invalid_message), span);
    lowerer.current_block = join_block;
    Value::Local(result)
}

fn lower_world_wgsl_bridge_call(
    lowerer: &mut FunctionLowerer,
    result_type: MirType,
    config: Option<&Result<NativeWgslBridgeConfig, SmolStr>>,
    bridge_symbol: &str,
    shapes: &[SmolStr],
    shape_indices: &HashMap<SmolStr, u32>,
    args: Vec<Value>,
    span: TextRange,
) -> Option<Value> {
    let config = match config {
        Some(Ok(config)) => config,
        Some(Err(err)) => {
            lower_wgsl_bridge_failure(lowerer, err.clone(), span);
            return None;
        }
        None => {
            lower_wgsl_bridge_failure(
                lowerer,
                SmolStr::new(format!("missing WGSL bridge config for {bridge_symbol}")),
                span,
            );
            return None;
        }
    };
    let world_shape_indices = lower_world_shape_index_list(lowerer, shapes, shape_indices, span);
    let mut call_args = vec![
        Value::Const(Literal::String(config.source.clone())),
        Value::Const(Literal::Integer(config.workgroup_size)),
        world_shape_indices,
    ];
    call_args.extend(args);
    Some(lowerer.lower_call_temp(
        result_type,
        SmolStr::new(bridge_symbol),
        call_args,
        span,
    ))
}

fn lower_batch_wgsl_bridge_call(
    lowerer: &mut FunctionLowerer,
    capture: LocalId,
    items: LocalId,
    config: Option<&Result<NativeWgslBridgeConfig, SmolStr>>,
    bridge_symbol: &str,
    capture_type: &str,
    capture_field: &str,
    capture_indices: &HashMap<SmolStr, u32>,
    stable_capture_id: fn(&SmolStr) -> i64,
    invalid_message: &str,
    span: TextRange,
) -> Option<Value> {
    let config = match config {
        Some(Ok(config)) => config,
        Some(Err(err)) => {
            lower_wgsl_bridge_failure(lowerer, err.clone(), span);
            return None;
        }
        None => {
            lower_wgsl_bridge_failure(
                lowerer,
                SmolStr::new(format!("missing WGSL bridge config for {bridge_symbol}")),
                span,
            );
            return None;
        }
    };
    let capture_index = lower_capture_index_lookup(
        lowerer,
        capture,
        capture_type,
        capture_field,
        capture_indices,
        stable_capture_id,
        invalid_message,
        span,
    );
    Some(lowerer.lower_call_temp(
        MirType::Named(SmolStr::new("List")),
        SmolStr::new(bridge_symbol),
        vec![
            Value::Const(Literal::String(config.source.clone())),
            Value::Const(Literal::Integer(config.workgroup_size)),
            capture_index,
            Value::Local(items),
        ],
        span,
    ))
}

fn lower_native_world_backend_guard(
    lowerer: &mut FunctionLowerer,
    backend: LocalId,
    auto_backend: DispatchBackend,
    cpu_block: BlockId,
    wgsl_block: BlockId,
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
    let is_auto = lowerer.lower_binary_temp(
        MirType::Boolean,
        BinaryOp::Eq,
        Value::Local(backend),
        Value::Const(Literal::Integer(3)),
        span,
    );
    let is_wgsl = lowerer.lower_binary_temp(
        MirType::Boolean,
        BinaryOp::Eq,
        Value::Local(backend),
        Value::Const(Literal::Integer(2)),
        span,
    );
    let auto_target = world_auto_backend(auto_backend);
    let auto_block = lowerer.new_block();
    let backend_check_block = lowerer.new_block();
    lowerer.set_terminator(Terminator::Branch {
        cond: is_auto,
        then_target: auto_block,
        else_target: backend_check_block,
        span,
    });

    lowerer.current_block = auto_block;
    lowerer.set_terminator(Terminator::Jump {
        target: match auto_target {
            DispatchBackend::Wgsl => wgsl_block,
            DispatchBackend::Cpu | DispatchBackend::VirtualGpu | DispatchBackend::Auto => cpu_block,
        },
        span,
    });

    lowerer.current_block = backend_check_block;
    let cpu_or_wgsl =
        lowerer.lower_binary_temp(MirType::Boolean, BinaryOp::Or, is_cpu.clone(), is_wgsl, span);
    let direct_block = lowerer.new_block();
    lowerer.set_terminator(Terminator::Branch {
        cond: cpu_or_wgsl,
        then_target: direct_block,
        else_target: unsupported_block,
        span,
    });

    lowerer.current_block = direct_block;
    lowerer.set_terminator(Terminator::Branch {
        cond: is_cpu,
        then_target: cpu_block,
        else_target: wgsl_block,
        span,
    });

    lowerer.current_block = unsupported_block;
    lower_wgsl_bridge_failure(
        lowerer,
        SmolStr::new("native MIR world queries currently support only cpu, wgsl, or auto backends"),
        span,
    );
}

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
    let origin = lowerer.lower_get_named_field(
        Value::Local(ray),
        "RayQuery",
        "origin",
        MirType::Vec3,
        span,
    );
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

pub(crate) fn lower_scene_trace_queries_helper(
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
    let helper_name = SmolStr::new("__wr_scene_trace_queries");
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

    let queries = lowerer.new_local(
        SmolStr::new("queries"),
        false,
        MirType::Named(SmolStr::new("List")),
    );
    lowerer.declare_local(SmolStr::new("queries"), queries);
    lowerer.params.push(queries);

    let entry = lowerer.new_block();
    lowerer.current_block = entry;
    let len = lowerer.lower_call_temp(
        MirType::Integer,
        SmolStr::new("__wr_list_len"),
        vec![Value::Local(queries)],
        span,
    );
    let result = lowerer.lower_call_temp(
        MirType::Named(SmolStr::new("List")),
        SmolStr::new("__wr_list_new"),
        vec![len.clone()],
        span,
    );
    let index = lowerer.new_local(SmolStr::new("$query_index"), true, MirType::Integer);
    lowerer.assign_use(Place::Local(index), Value::Const(Literal::Integer(0)), span);
    let head = lowerer.new_block();
    let body_block = lowerer.new_block();
    let exit = lowerer.new_block();
    lowerer.set_terminator(Terminator::Jump { target: head, span });

    lowerer.current_block = head;
    let within_bounds = lowerer.lower_binary_temp(
        MirType::Boolean,
        BinaryOp::Lt,
        Value::Local(index),
        len,
        span,
    );
    lowerer.set_terminator(Terminator::Branch {
        cond: within_bounds,
        then_target: body_block,
        else_target: exit,
        span,
    });

    lowerer.current_block = body_block;
    let query = lowerer.lower_call_temp(
        MirType::Named(SmolStr::new("TraceQuery")),
        SmolStr::new("__wr_list_get"),
        vec![Value::Local(queries), Value::Local(index)],
        span,
    );
    let capture = lowerer.new_temp(MirType::Named(SmolStr::new("ShapeCapture")));
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(capture),
        value: Rvalue::GetField {
            base: query.clone(),
            field: SmolStr::new("capture"),
            slot: lowerer.field_slot("TraceQuery", "capture"),
        },
        span,
    });
    let origin = lowerer.new_temp(MirType::Vec3);
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(origin),
        value: Rvalue::GetField {
            base: query.clone(),
            field: SmolStr::new("origin"),
            slot: lowerer.field_slot("TraceQuery", "origin"),
        },
        span,
    });
    let direction = lowerer.new_temp(MirType::Vec3);
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(direction),
        value: Rvalue::GetField {
            base: query.clone(),
            field: SmolStr::new("direction"),
            slot: lowerer.field_slot("TraceQuery", "direction"),
        },
        span,
    });
    let max_distance = lowerer.new_temp(MirType::Float);
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(max_distance),
        value: Rvalue::GetField {
            base: query.clone(),
            field: SmolStr::new("max_distance"),
            slot: lowerer.field_slot("TraceQuery", "max_distance"),
        },
        span,
    });
    let min_step = lowerer.new_temp(MirType::Float);
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(min_step),
        value: Rvalue::GetField {
            base: query.clone(),
            field: SmolStr::new("min_step"),
            slot: lowerer.field_slot("TraceQuery", "min_step"),
        },
        span,
    });
    let hit_epsilon = lowerer.new_temp(MirType::Float);
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(hit_epsilon),
        value: Rvalue::GetField {
            base: query.clone(),
            field: SmolStr::new("hit_epsilon"),
            slot: lowerer.field_slot("TraceQuery", "hit_epsilon"),
        },
        span,
    });
    let max_steps = lowerer.new_temp(MirType::Integer);
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(max_steps),
        value: Rvalue::GetField {
            base: query,
            field: SmolStr::new("max_steps"),
            slot: lowerer.field_slot("TraceQuery", "max_steps"),
        },
        span,
    });
    let ray = lowerer.build_ray_query_value(
        Value::Temp(origin),
        Value::Temp(direction),
        Value::Temp(max_distance),
        Value::Temp(min_step),
        Value::Temp(hit_epsilon),
        Value::Temp(max_steps),
        span,
    );
    let hit = lowerer.lower_call_temp(
        MirType::Named(SmolStr::new("Hit3")),
        SmolStr::new("__wr_scene_trace_capture"),
        vec![Value::Temp(capture), ray],
        span,
    );
    let _ = lowerer.lower_call_temp(
        MirType::Nil,
        SmolStr::new("__wr_list_set"),
        vec![result.clone(), Value::Local(index), hit],
        span,
    );
    let next = lowerer.lower_binary_temp(
        MirType::Integer,
        BinaryOp::Add,
        Value::Local(index),
        Value::Const(Literal::Integer(1)),
        span,
    );
    lowerer.assign_use(Place::Local(index), next, span);
    lowerer.set_terminator(Terminator::Jump { target: head, span });

    lowerer.current_block = exit;
    lowerer.set_terminator(Terminator::Return {
        value: Some(result),
        span,
    });

    MirFunction {
        name: helper_name,
        params: lowerer.params,
        abi_params: vec![PortableAbiType::Value],
        abi_return: PortableAbiType::Value,
        locals: lowerer.locals,
        temps: lowerer.temps,
        blocks: lowerer.blocks,
        entry,
        suspendable: false,
    }
}

pub(crate) fn lower_scene_surface_queries_helper(
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
    let helper_name = SmolStr::new("__wr_scene_surface_queries");
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

    let queries = lowerer.new_local(
        SmolStr::new("queries"),
        false,
        MirType::Named(SmolStr::new("List")),
    );
    lowerer.declare_local(SmolStr::new("queries"), queries);
    lowerer.params.push(queries);

    let entry = lowerer.new_block();
    lowerer.current_block = entry;
    let len = lowerer.lower_call_temp(
        MirType::Integer,
        SmolStr::new("__wr_list_len"),
        vec![Value::Local(queries)],
        span,
    );
    let result = lowerer.lower_call_temp(
        MirType::Named(SmolStr::new("List")),
        SmolStr::new("__wr_list_new"),
        vec![len.clone()],
        span,
    );
    let index = lowerer.new_local(SmolStr::new("$query_index"), true, MirType::Integer);
    lowerer.assign_use(Place::Local(index), Value::Const(Literal::Integer(0)), span);
    let head = lowerer.new_block();
    let body_block = lowerer.new_block();
    let exit = lowerer.new_block();
    lowerer.set_terminator(Terminator::Jump { target: head, span });

    lowerer.current_block = head;
    let within_bounds = lowerer.lower_binary_temp(
        MirType::Boolean,
        BinaryOp::Lt,
        Value::Local(index),
        len,
        span,
    );
    lowerer.set_terminator(Terminator::Branch {
        cond: within_bounds,
        then_target: body_block,
        else_target: exit,
        span,
    });

    lowerer.current_block = body_block;
    let query = lowerer.lower_call_temp(
        MirType::Named(SmolStr::new("SurfaceQuery")),
        SmolStr::new("__wr_list_get"),
        vec![Value::Local(queries), Value::Local(index)],
        span,
    );
    let capture = lowerer.new_temp(MirType::Named(SmolStr::new("ShapeCapture")));
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(capture),
        value: Rvalue::GetField {
            base: query.clone(),
            field: SmolStr::new("capture"),
            slot: lowerer.field_slot("SurfaceQuery", "capture"),
        },
        span,
    });
    let hit = lowerer.new_temp(MirType::Named(SmolStr::new("Hit3")));
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Temp(hit),
        value: Rvalue::GetField {
            base: query,
            field: SmolStr::new("hit"),
            slot: lowerer.field_slot("SurfaceQuery", "hit"),
        },
        span,
    });
    let surface = lowerer.lower_call_temp(
        MirType::Named(SmolStr::new("Surface")),
        SmolStr::new("__wr_scene_surface_capture"),
        vec![Value::Temp(capture), Value::Temp(hit)],
        span,
    );
    let _ = lowerer.lower_call_temp(
        MirType::Nil,
        SmolStr::new("__wr_list_set"),
        vec![result.clone(), Value::Local(index), surface],
        span,
    );
    let next = lowerer.lower_binary_temp(
        MirType::Integer,
        BinaryOp::Add,
        Value::Local(index),
        Value::Const(Literal::Integer(1)),
        span,
    );
    lowerer.assign_use(Place::Local(index), next, span);
    lowerer.set_terminator(Terminator::Jump { target: head, span });

    lowerer.current_block = exit;
    lowerer.set_terminator(Terminator::Return {
        value: Some(result),
        span,
    });

    MirFunction {
        name: helper_name,
        params: lowerer.params,
        abi_params: vec![PortableAbiType::Value],
        abi_return: PortableAbiType::Value,
        locals: lowerer.locals,
        temps: lowerer.temps,
        blocks: lowerer.blocks,
        entry,
        suspendable: false,
    }
}

fn lower_native_batch_backend_guard(
    lowerer: &mut FunctionLowerer,
    backend: LocalId,
    auto_backend: DispatchBackend,
    cpu_block: BlockId,
    vgpu_block: BlockId,
    wgsl_block: BlockId,
    invalid_backend_block: BlockId,
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
    let is_wgsl = lowerer.lower_binary_temp(
        MirType::Boolean,
        BinaryOp::Eq,
        Value::Local(backend),
        Value::Const(Literal::Integer(2)),
        span,
    );
    let is_auto = lowerer.lower_binary_temp(
        MirType::Boolean,
        BinaryOp::Eq,
        Value::Local(backend),
        Value::Const(Literal::Integer(3)),
        span,
    );

    let auto_block = lowerer.new_block();
    let vgpu_check_block = lowerer.new_block();
    lowerer.set_terminator(Terminator::Branch {
        cond: is_auto,
        then_target: auto_block,
        else_target: vgpu_check_block,
        span,
    });

    lowerer.current_block = auto_block;
    lowerer.set_terminator(Terminator::Jump {
        target: match batch_auto_backend(auto_backend) {
            DispatchBackend::Cpu => cpu_block,
            DispatchBackend::VirtualGpu => vgpu_block,
            DispatchBackend::Wgsl => wgsl_block,
            DispatchBackend::Auto => cpu_block,
        },
        span,
    });

    let cpu_check_block = lowerer.new_block();
    lowerer.current_block = vgpu_check_block;
    lowerer.set_terminator(Terminator::Branch {
        cond: is_vgpu,
        then_target: vgpu_block,
        else_target: cpu_check_block,
        span,
    });

    let wgsl_check_block = lowerer.new_block();
    lowerer.current_block = cpu_check_block;
    lowerer.set_terminator(Terminator::Branch {
        cond: is_cpu,
        then_target: cpu_block,
        else_target: wgsl_check_block,
        span,
    });

    lowerer.current_block = wgsl_check_block;
    lowerer.set_terminator(Terminator::Branch {
        cond: is_wgsl,
        then_target: wgsl_block,
        else_target: invalid_backend_block,
        span,
    });
}

pub(crate) fn lower_field_batch_queries_helper(
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
    plan: &BatchQueryPlan,
    auto_backend: DispatchBackend,
    wgsl_config: Option<&Result<NativeWgslBridgeConfig, SmolStr>>,
    capture_indices: &HashMap<SmolStr, u32>,
) -> MirFunction {
    debug_assert!(matches!(
        plan.kernel,
        InternalKernelKind::FieldDistanceCapture
            | InternalKernelKind::ShapeDistanceCapture
            | InternalKernelKind::FieldNormalCapture
            | InternalKernelKind::ShapeNormalCapture
    ));
    match plan.capture_kind {
        CaptureKind::Field => {
            debug_assert_eq!(
                plan.candidate_strategy(),
                CandidateStrategy::DirectFieldCapture
            );
            debug_assert_eq!(plan.pruning_strategy(), PruningStrategy::None);
            debug_assert!(!plan.has_opaque_pessimization_boundary());
        }
        CaptureKind::Shape => {
            debug_assert!(matches!(
                plan.candidate_strategy(),
                CandidateStrategy::ShapeBranchTraversal
                    | CandidateStrategy::SupportAcceleratedShapeTraversal
                    | CandidateStrategy::OpaqueFallback
            ));
            debug_assert!(matches!(
                plan.pruning_strategy(),
                PruningStrategy::ConservativeTraversal
                    | PruningStrategy::SupportLowerBound
                    | PruningStrategy::CullingTable
                    | PruningStrategy::OpaquePessimizationBoundary
            ));
            if matches!(plan.candidate_strategy(), CandidateStrategy::OpaqueFallback) {
                debug_assert!(plan.has_opaque_pessimization_boundary());
            }
        }
        CaptureKind::Region => panic!("field batch helper does not support region captures"),
    }
    debug_assert!(!matches!(plan.capture_kind, CaptureKind::Region));
    debug_assert!(!plan.preserves_local_hit_context);
    let helper_name = plan.helper_name.clone();
    let (capture_type, capture_field, stable_capture_id, invalid_capture_message) =
        match plan.capture_kind {
            CaptureKind::Field => (
                "FieldCapture",
                "scene_id",
                stable_field_scene_capture_id as fn(&SmolStr) -> i64,
                "field batch WGSL dispatch requires a known field capture",
            ),
            CaptureKind::Shape => (
                "ShapeCapture",
                "scene_id",
                stable_shape_scene_capture_id as fn(&SmolStr) -> i64,
                "shape batch WGSL dispatch requires a known shape scene capture",
            ),
            CaptureKind::Region => panic!("field batch helper does not support region captures"),
        };
    let wgsl_bridge_symbol = match plan.kernel {
        InternalKernelKind::FieldDistanceCapture => "__wr_wgsl_field_distance_batch_queries",
        InternalKernelKind::ShapeDistanceCapture => "__wr_wgsl_shape_distance_batch_queries",
        InternalKernelKind::FieldNormalCapture => "__wr_wgsl_field_normal_batch_queries",
        InternalKernelKind::ShapeNormalCapture => "__wr_wgsl_shape_normal_batch_queries",
        other => panic!("unexpected field batch kernel for WGSL bridge: {other:?}"),
    };
    debug_assert!(matches!(plan.item_kind, QueryItemKind::PointQuery));
    debug_assert!(matches!(
        plan.result_kind,
        QueryResultKind::DistanceResult | QueryResultKind::NormalResult
    ));
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
        MirType::Named(SmolStr::new(capture_type)),
    );
    lowerer.declare_local(SmolStr::new("capture"), capture);
    lowerer.params.push(capture);

    let items = lowerer.new_local(
        SmolStr::new("items"),
        false,
        MirType::Named(SmolStr::new("List")),
    );
    lowerer.declare_local(SmolStr::new("items"), items);
    lowerer.params.push(items);

    let backend = lowerer.new_local(SmolStr::new("backend"), false, MirType::Integer);
    lowerer.declare_local(SmolStr::new("backend"), backend);
    lowerer.params.push(backend);

    let result = lowerer.new_local(
        SmolStr::new("$field_batch_result"),
        true,
        MirType::Named(SmolStr::new("List")),
    );

    let entry = lowerer.new_block();
    lowerer.current_block = entry;
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Local(result),
        value: Rvalue::BuildList {
            items: Vec::new(),
            alloc: AllocKind::Escaping,
        },
        span,
    });

    let cpu_block = lowerer.new_block();
    let vgpu_block = lowerer.new_block();
    let wgsl_block = lowerer.new_block();
    let invalid_backend_block = lowerer.new_block();
    let merge_block = lowerer.new_block();
    lower_native_batch_backend_guard(
        &mut lowerer,
        backend,
        auto_backend,
        cpu_block,
        vgpu_block,
        wgsl_block,
        invalid_backend_block,
        span,
    );

    lowerer.current_block = invalid_backend_block;
    lower_wgsl_bridge_failure(
        &mut lowerer,
        SmolStr::new(
            "scene batch dispatch backend must be cpu, virtual_gpu, wgsl, or auto",
        ),
        span,
    );

    lowerer.current_block = cpu_block;
    lowerer.lower_batch_query_loop(
        plan,
        Value::Local(items),
        Value::Local(capture),
        result,
        span,
        false,
        merge_block,
    );

    lowerer.current_block = vgpu_block;
    lowerer.lower_batch_query_loop(
        plan,
        Value::Local(items),
        Value::Local(capture),
        result,
        span,
        true,
        merge_block,
    );

    lowerer.current_block = wgsl_block;
    if let Some(value) = lower_batch_wgsl_bridge_call(
        &mut lowerer,
        capture,
        items,
        wgsl_config,
        wgsl_bridge_symbol,
        capture_type,
        capture_field,
        capture_indices,
        stable_capture_id,
        invalid_capture_message,
        span,
    ) {
        lowerer.assign_use(Place::Local(result), value, span);
        lowerer.set_terminator(Terminator::Jump {
            target: merge_block,
            span,
        });
    }

    lowerer.current_block = merge_block;
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Local(result)),
        span,
    });

    MirFunction {
        name: helper_name,
        params: lowerer.params,
        abi_params: vec![
            PortableAbiType::Value,
            PortableAbiType::Value,
            PortableAbiType::Value,
        ],
        abi_return: PortableAbiType::Value,
        locals: lowerer.locals,
        temps: lowerer.temps,
        blocks: lowerer.blocks,
        entry,
        suspendable: false,
    }
}

pub(crate) fn lower_shape_batch_queries_helper(
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
    plan: &BatchQueryPlan,
    auto_backend: DispatchBackend,
    wgsl_config: Option<&Result<NativeWgslBridgeConfig, SmolStr>>,
    capture_indices: &HashMap<SmolStr, u32>,
) -> MirFunction {
    debug_assert!(matches!(
        plan.kernel,
        InternalKernelKind::ShapeTraceCapture
            | InternalKernelKind::ShapeSurfaceCapture
            | InternalKernelKind::ShapeOccludedCapture
    ));
    debug_assert_eq!(plan.capture_kind, CaptureKind::Shape);
    let helper_name = plan.helper_name.clone();
    match plan.kind {
        BatchQueryKind::Nearest | BatchQueryKind::Trace | BatchQueryKind::Occluded => {
            debug_assert!(matches!(
                plan.candidate_strategy(),
                CandidateStrategy::ShapeBranchTraversal
                    | CandidateStrategy::SupportAcceleratedShapeTraversal
                    | CandidateStrategy::OpaqueFallback
            ));
            debug_assert!(matches!(
                plan.pruning_strategy(),
                PruningStrategy::ConservativeTraversal
                    | PruningStrategy::SupportLowerBound
                    | PruningStrategy::CullingTable
                    | PruningStrategy::OpaquePessimizationBoundary
            ));
            debug_assert!(plan.preserves_local_hit_context);
            if matches!(plan.candidate_strategy(), CandidateStrategy::OpaqueFallback) {
                debug_assert!(plan.has_opaque_pessimization_boundary());
            }
            if matches!(
                plan.pruning_strategy(),
                PruningStrategy::OpaquePessimizationBoundary
            ) {
                debug_assert!(matches!(
                    plan.candidate_strategy(),
                    CandidateStrategy::OpaqueFallback
                ));
                debug_assert!(plan.has_opaque_pessimization_boundary());
            }
        }
        BatchQueryKind::Surface => {
            debug_assert_eq!(
                plan.candidate_strategy(),
                CandidateStrategy::SurfaceHitReuse
            );
            debug_assert_eq!(plan.pruning_strategy(), PruningStrategy::None);
            debug_assert!(!plan.preserves_local_hit_context);
            debug_assert!(!plan.has_opaque_pessimization_boundary());
        }
        other => panic!("shape batch helper does not support {other:?}"),
    }
    debug_assert!(matches!(
        plan.item_kind,
        QueryItemKind::RayQuery | QueryItemKind::Hit3
    ));
    debug_assert!(matches!(
        plan.result_kind,
        QueryResultKind::Hit3 | QueryResultKind::Surface | QueryResultKind::OcclusionResult
    ));
    let wgsl_bridge_symbol = match plan.kernel {
        InternalKernelKind::ShapeTraceCapture => "__wr_wgsl_shape_trace_batch_queries",
        InternalKernelKind::ShapeSurfaceCapture => "__wr_wgsl_shape_surface_batch_queries",
        InternalKernelKind::ShapeOccludedCapture => "__wr_wgsl_shape_occluded_batch_queries",
        other => panic!("unexpected shape batch kernel for WGSL bridge: {other:?}"),
    };
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
    lowerer.declare_local(SmolStr::new("capture"), capture);
    lowerer.params.push(capture);

    let items = lowerer.new_local(
        SmolStr::new("items"),
        false,
        MirType::Named(SmolStr::new("List")),
    );
    lowerer.declare_local(SmolStr::new("items"), items);
    lowerer.params.push(items);

    let backend = lowerer.new_local(SmolStr::new("backend"), false, MirType::Integer);
    lowerer.declare_local(SmolStr::new("backend"), backend);
    lowerer.params.push(backend);

    let result = lowerer.new_local(
        SmolStr::new("$shape_batch_result"),
        true,
        MirType::Named(SmolStr::new("List")),
    );

    let entry = lowerer.new_block();
    lowerer.current_block = entry;
    lowerer.push_stmt(MirStmt::Assign {
        place: Place::Local(result),
        value: Rvalue::BuildList {
            items: Vec::new(),
            alloc: AllocKind::Escaping,
        },
        span,
    });

    let cpu_block = lowerer.new_block();
    let vgpu_block = lowerer.new_block();
    let wgsl_block = lowerer.new_block();
    let invalid_backend_block = lowerer.new_block();
    let merge_block = lowerer.new_block();
    lower_native_batch_backend_guard(
        &mut lowerer,
        backend,
        auto_backend,
        cpu_block,
        vgpu_block,
        wgsl_block,
        invalid_backend_block,
        span,
    );

    lowerer.current_block = invalid_backend_block;
    lower_wgsl_bridge_failure(
        &mut lowerer,
        SmolStr::new(
            "scene batch dispatch backend must be cpu, virtual_gpu, wgsl, or auto",
        ),
        span,
    );

    lowerer.current_block = cpu_block;
    lowerer.lower_batch_query_loop(
        plan,
        Value::Local(items),
        Value::Local(capture),
        result,
        span,
        false,
        merge_block,
    );

    lowerer.current_block = vgpu_block;
    lowerer.lower_batch_query_loop(
        plan,
        Value::Local(items),
        Value::Local(capture),
        result,
        span,
        true,
        merge_block,
    );

    lowerer.current_block = wgsl_block;
    if let Some(value) = lower_batch_wgsl_bridge_call(
        &mut lowerer,
        capture,
        items,
        wgsl_config,
        wgsl_bridge_symbol,
        "ShapeCapture",
        "root_feature_id",
        capture_indices,
        stable_shape_capture_id,
        "shape batch WGSL dispatch requires a known shape capture",
        span,
    ) {
        lowerer.assign_use(Place::Local(result), value, span);
        lowerer.set_terminator(Terminator::Jump {
            target: merge_block,
            span,
        });
    }

    lowerer.current_block = merge_block;
    lowerer.set_terminator(Terminator::Return {
        value: Some(Value::Local(result)),
        span,
    });

    MirFunction {
        name: helper_name,
        params: lowerer.params,
        abi_params: vec![
            PortableAbiType::Value,
            PortableAbiType::Value,
            PortableAbiType::Value,
        ],
        abi_return: PortableAbiType::Value,
        locals: lowerer.locals,
        temps: lowerer.temps,
        blocks: lowerer.blocks,
        entry,
        suspendable: false,
    }
}
