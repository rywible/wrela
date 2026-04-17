//! Owns MIR helper synthesis for authored rendering convenience functions and
//! render-adjacent query bridges.
//! Does not own general expression lowering or presentation execution itself.
//!
//! Key invariants:
//! - generated render helpers must preserve the same contracts and ABI layouts
//!   the runtime/presentation layers expect.
//! - helper synthesis here may expand authored convenience calls, but it must
//!   not smuggle in new public semantics.
//!
//! Primary entrypoints:
//! - render helper lowerers in this module
//!
//! Failure modes / common pitfalls:
//! - letting render-only helper assumptions leak into general MIR lowering makes
//!   non-rendering queries depend on accidental presentation details.

use super::*;

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

pub(super) fn build_scene_domain_contract_value(
    lowerer: &mut FunctionLowerer,
    scene_id: Value,
    geometry_detail: Value,
    material: Value,
    radiance: Value,
    media: Value,
    span: TextRange,
) -> Value {
    let spatial = build_spatial_domain_contract_value(lowerer, geometry_detail, span);

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

fn build_spatial_domain_contract_value(
    lowerer: &mut FunctionLowerer,
    geometry_detail: Value,
    span: TextRange,
) -> Value {
    let mut spatial = lowerer.synthetic_class_target_info("SpatialDomainContract");
    FunctionLowerer::set_class_field_value(&mut spatial, "geometry_detail", geometry_detail);
    lowerer.build_class_instance(&spatial, span)
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

pub(super) fn lower_render_shadow_visibility_helper(
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

pub(super) fn lower_render_ambient_occlusion_helper(
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

// Shared color-capture helper for internal presentation export bindings. The
// canonical presentation path lives in `presentation_plan` and
// `presentation_exec`; this helper only serializes a prepared color attachment.
pub(super) fn lower_render_scene_color_helper(
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
        SmolStr::new("__wr_presentation_scene_color_capture"),
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
    let fill_strength = declare_internal_param(&mut lowerer, "fill_strength", MirType::Float);
    let ambient_color = declare_internal_param(&mut lowerer, "ambient_color", MirType::Vec3);
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
        Value::Local(fill_strength),
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
    let lighting_a = lowerer.lower_binary_temp(MirType::Float, BinaryOp::Add, diffuse, fill, span);
    let lighting =
        lowerer.lower_binary_temp(MirType::Float, BinaryOp::Mul, lighting_a, ao.clone(), span);
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
    let ambient_x = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("__wr_vec_component"),
        vec![
            Value::Local(ambient_color),
            Value::Const(Literal::Integer(0)),
        ],
        span,
    );
    let ambient_y = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("__wr_vec_component"),
        vec![
            Value::Local(ambient_color),
            Value::Const(Literal::Integer(1)),
        ],
        span,
    );
    let ambient_z = lowerer.lower_call_temp(
        MirType::Float,
        SmolStr::new("__wr_vec_component"),
        vec![
            Value::Local(ambient_color),
            Value::Const(Literal::Integer(2)),
        ],
        span,
    );
    let direct_x_base = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Mul,
        albedo_x.clone(),
        lighting.clone(),
        span,
    );
    let direct_x_ambient =
        lowerer.lower_binary_temp(MirType::Float, BinaryOp::Mul, albedo_x, ambient_x, span);
    let direct_x_unlit = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Add,
        direct_x_ambient,
        direct_x_base,
        span,
    );
    let direct_x_lit = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Mul,
        direct_x_unlit,
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
        albedo_y.clone(),
        lighting.clone(),
        span,
    );
    let direct_y_ambient =
        lowerer.lower_binary_temp(MirType::Float, BinaryOp::Mul, albedo_y, ambient_y, span);
    let direct_y_unlit = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Add,
        direct_y_ambient,
        direct_y_base,
        span,
    );
    let direct_y_lit = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Mul,
        direct_y_unlit,
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
    let direct_z_base = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Mul,
        albedo_z.clone(),
        lighting,
        span,
    );
    let direct_z_ambient =
        lowerer.lower_binary_temp(MirType::Float, BinaryOp::Mul, albedo_z, ambient_z, span);
    let direct_z_unlit = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Add,
        direct_z_ambient,
        direct_z_base,
        span,
    );
    let direct_z_lit = lowerer.lower_binary_temp(
        MirType::Float,
        BinaryOp::Mul,
        direct_z_unlit,
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

// Shared PPM export wrapper over the canonical presentation color path. This
// remains as a thin host/export helper rather than authored presentation syntax.
pub(super) fn lower_render_capture_to_ppm_helper(
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
        SmolStr::new("__wr_presentation_attachment_to_ppm"),
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
    let fill_strength = declare_internal_param(&mut lowerer, "fill_strength", MirType::Float);
    let ambient_color = declare_internal_param(&mut lowerer, "ambient_color", MirType::Vec3);
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
        SmolStr::new("__wr_presentation_scene_color_capture"),
        vec![
            Value::Local(world),
            Value::Local(domain),
            camera_position.clone(),
            Value::Local(light),
            ray,
            Value::Local(fill_dir),
            Value::Local(fill_strength),
            Value::Local(ambient_color),
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
