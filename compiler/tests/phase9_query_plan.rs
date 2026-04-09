use std::collections::BTreeSet;
use wrela::hir;
use wrela::hir::lower as hir_lower;
use wrela::mir;
use wrela::parser::ast;
use wrela::parser::ast::AstNode;
use wrela::parser::parse;
use wrela::portable;
use wrela::query_contract;
use wrela::query_plan;
use wrela::scene_ir;

fn lower_inline_module_from_source(source: &str) -> hir::Module {
    let node = parse(source);
    let root = ast::Root::cast(node).expect("root");
    hir_lower::lower(root)
}

fn lower_mir_module_from_source(source: &str) -> mir::MirModule {
    let module = lower_inline_module_from_source(source);
    let semantic = hir::semantic::check_module(&module);
    assert!(
        semantic.errors.is_empty(),
        "semantic errors: {:?}",
        semantic.errors
    );
    let (type_errors, type_info) = hir::typeck::check_module_with_info(&module);
    assert!(type_errors.is_empty(), "type errors: {type_errors:?}");
    mir::lower::lower_module_with_types(&module, &type_info)
}

fn mir_function_names(module: &mir::MirModule) -> Vec<String> {
    module
        .functions
        .iter()
        .map(|func| func.name.to_string())
        .collect()
}

fn direct_call_targets(func: &mir::MirFunction) -> BTreeSet<String> {
    let mut targets = BTreeSet::new();
    for block in &func.blocks {
        for stmt in &block.stmts {
            if let mir::Stmt::Assign {
                value:
                    mir::Rvalue::Call {
                        target: mir::CallTarget::Function(name),
                        ..
                    },
                ..
            } = stmt
            {
                targets.insert(name.to_string());
            }
        }
    }
    targets
}

fn first_direct_call_args<'a>(
    func: &'a mir::MirFunction,
    target_name: &str,
) -> Option<&'a [mir::Value]> {
    for block in &func.blocks {
        for stmt in &block.stmts {
            if let mir::Stmt::Assign {
                value:
                    mir::Rvalue::Call {
                        target: mir::CallTarget::Function(name),
                        args,
                        ..
                    },
                ..
            } = stmt
                && name == target_name
            {
                return Some(args);
            }
        }
    }
    None
}

fn assert_float_const(value: &mir::Value, expected: f64) {
    let mir::Value::Const(hir::Literal::Float(actual)) = value else {
        panic!("expected float constant {expected}, got {value:?}");
    };
    assert!(
        (actual - expected).abs() <= 0.00001,
        "expected float constant {expected}, got {actual}"
    );
}

fn assert_integer_const(value: &mir::Value, expected: i64) {
    let mir::Value::Const(hir::Literal::Integer(actual)) = value else {
        panic!("expected integer constant {expected}, got {value:?}");
    };
    assert_eq!(*actual, expected);
}

fn set_field_local_name(func: &mir::MirFunction, field_name: &str) -> Option<String> {
    for block in &func.blocks {
        for stmt in &block.stmts {
            if let mir::Stmt::SetField {
                field,
                value: mir::Value::Local(local),
                ..
            } = stmt
                && field == field_name
            {
                return Some(func.locals[local.0].name.to_string());
            }
        }
    }
    None
}

#[test]
fn phase9_scene_ir_is_stable_for_semantic_and_opaque_fields() {
    let source = r#"
field exact distance sphere_field(p: Vec3) -> F32 {
    sphere(radius = 1.0)
}

field conservative distance opaque_field(p: Vec3) -> F32 {
    support = Support3(bounds=Bounds3(
        min=vec3(-1.0, -1.0, -1.0),
        max=vec3(1.0, 1.0, 1.0)
    ))
    bounds = Bounds3(
        min=vec3(-1.0, -1.0, -1.0),
        max=vec3(1.0, 1.0, 1.0)
    )
    return length(p - vec3(3.0, 0.0, 0.0)) - 0.5
}

material shade(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.25, 0.35, 0.45),
        roughness=0.5,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape sphere_shape {
    field = sphere_field
    material = shade
    payload = Payload(
        entity_id=u32(1),
        material_id=u32(1),
        actor=ActorHandle(id=u32(1), generation=u32(0))
    )
}

shape opaque_shape {
    field = opaque_field
    material = shade
    payload = Payload(
        entity_id=u32(2),
        material_id=u32(2),
        actor=ActorHandle(id=u32(2), generation=u32(0))
    )
}

shape scene_shape {
    union {
        provenance_policy = nearest
        use sphere_shape
        use opaque_shape
    }
}
"#;

    let module_a = lower_inline_module_from_source(source);
    let module_b = lower_inline_module_from_source(source);
    let scene_a = scene_ir::lower_module(&module_a);
    let scene_b = scene_ir::lower_module(&module_b);
    assert_eq!(scene_a, scene_b);

    let sphere_a = module_a
        .functions
        .iter()
        .find(|(_, func)| func.name == "sphere_field")
        .map(|(_, func)| func)
        .expect("sphere field");
    let sphere_b = module_b
        .functions
        .iter()
        .find(|(_, func)| func.name == "sphere_field")
        .map(|(_, func)| func)
        .expect("sphere field");
    assert_eq!(sphere_a.field_graph, sphere_b.field_graph);
    match &sphere_a.field_graph.as_ref().expect("sphere graph").root {
        hir::FieldExpr::Primitive {
            primitive: hir::FieldPrimitive::Sphere,
            ..
        } => {}
        other => panic!("expected semantic sphere primitive, got {other:?}"),
    }
    let sphere_scene = scene_a.fields.get("sphere_field").expect("scene-ir sphere");
    assert_eq!(
        sphere_scene.semantics,
        scene_ir::DistanceSemantics::ExactSignedDistance
    );
    assert!(!sphere_scene.node_records.is_empty());
    assert!(!sphere_scene.support_records.is_empty());
    match &sphere_scene.root {
        scene_ir::FieldNode::Primitive {
            primitive: hir::FieldPrimitive::Sphere,
            ..
        } => {}
        other => panic!("expected scene-ir sphere primitive, got {other:?}"),
    }

    let opaque = module_a
        .functions
        .iter()
        .find(|(_, func)| func.name == "opaque_field")
        .map(|(_, func)| func)
        .expect("opaque field");
    match &opaque.field_graph.as_ref().expect("opaque graph").root {
        hir::FieldExpr::Custom { .. } => {}
        other => panic!("expected opaque custom field graph, got {other:?}"),
    }
    assert_eq!(
        opaque.field_graph.as_ref().expect("opaque graph").trace,
        hir::GraphTraceMetadata::pessimistic()
    );
    let opaque_metadata = opaque.field.as_ref().expect("opaque metadata");
    assert!(
        opaque_metadata.authored_support.is_some(),
        "expected opaque field to preserve authored support"
    );
    assert!(
        opaque_metadata.authored_bounds.is_some(),
        "expected opaque field to preserve authored bounds"
    );
    let opaque_scene = scene_a.fields.get("opaque_field").expect("scene-ir opaque");
    assert_eq!(
        opaque_scene.semantics,
        scene_ir::DistanceSemantics::UnknownOpaque
    );
    match &opaque_scene.root {
        scene_ir::FieldNode::OpaqueLeaf => {}
        other => panic!("expected opaque scene-ir field, got {other:?}"),
    }
    assert!(
        scene_a
            .shapes
            .get("scene_shape")
            .map(|shape| shape.opaque_boundary)
            .unwrap_or(false),
        "expected scene shape to preserve opaque-leaf quarantine"
    );
    let shape_scene = scene_a.shapes.get("scene_shape").expect("scene shape");
    assert!(!shape_scene.node_records.is_empty());
    assert!(!shape_scene.support_records.is_empty());
    assert!(!shape_scene.feature_leaves.is_empty());
    assert_eq!(
        shape_scene,
        scene_b.shapes.get("scene_shape").expect("scene shape")
    );
}

#[test]
fn phase9_query_helpers_are_deterministic_and_identity_fields_are_public() {
    let source = r#"
field exact distance sphere_field(p: Vec3) -> F32 {
    sphere(radius = 1.0)
}

material shade(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.25, 0.35, 0.45),
        roughness=0.5,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape scene_shape {
    field = sphere_field
    material = shade
    payload = Payload(
        entity_id=u32(11),
        material_id=u32(11),
        actor=ActorHandle(id=u32(11), generation=u32(0))
    )
}

region scene_region() {
    place scene = scene_shape
}

domain phase8_coarse_domain(world: RegionCapture) {
    geometry_detail = 0
    material = false
    radiance = false
    media = false
    max_distance = 6.0
    min_step = 0.05
    hit_epsilon = 0.001
    max_steps = 96
}

domain phase8_fine_domain(world: RegionCapture) {
    geometry_detail = 1
    material = true
    radiance = true
    media = true
    max_distance = 6.0
    min_step = 0.05
    hit_epsilon = 0.001
    max_steps = 96
}

fn main() -> Integer {
    scene_capture = capture scene_shape
    world = capture scene_region
    coarse_domain = phase8_coarse_domain(world = world)
    fine_domain = phase8_fine_domain(world = world)
    points = [
        PointQuery(point=vec3(0.0, 0.0, 0.6)),
        PointQuery(point=vec3(0.5, 0.0, 0.6))
    ]
    rays = [
        RayQuery(
            origin=vec3(0.0, 0.0, 3.0),
            direction=vec3(0.0, 0.0, -1.0),
            max_distance=6.0,
            min_step=0.05,
            hit_epsilon=0.001,
            max_steps=96
        ),
        RayQuery(
            origin=vec3(0.5, 0.0, 3.0),
            direction=vec3(0.0, 0.0, -1.0),
            max_distance=6.0,
            min_step=0.05,
            hit_epsilon=0.001,
            max_steps=96
        )
    ]
    shadow_rays = [
        RayQuery(
            origin=vec3(0.0, 0.0, 3.0),
            direction=vec3(0.0, 0.0, -1.0),
            max_distance=6.0,
            min_step=0.05,
            hit_epsilon=0.001,
            max_steps=96
        ),
        RayQuery(
            origin=vec3(0.0, 0.0, 3.0),
            direction=vec3(0.0, 1.0, 0.0),
            max_distance=6.0,
            min_step=0.05,
            hit_epsilon=0.001,
            max_steps=96
        )
    ]

    hit = trace_shape(
        capture=scene_capture,
        ray=ray_query(
            origin=vec3(0.0, 0.0, 3.0),
            direction=vec3(0.0, 0.0, -1.0),
            max_distance=6.0,
            min_step=0.05,
            hit_epsilon=0.001,
            max_steps=96
        )
    )
    scene_surface = surface_at(capture=scene_capture, hit=hit)
    scene_radiance = radiance_at(
        capture=scene_capture,
        sample=point_direction_query(
            point=hit.position,
            direction=normalize(vec3(0.0, 1.0, 1.0))
        )
    )
    scene_medium = medium_at(capture=scene_capture, point=hit.position)
    cpu_distances = distance_at_batch(
        capture=scene_capture,
        points=points,
        backend=dispatch_backend_cpu()
    )
    vgpu_normals = normal_at_batch(
        capture=scene_capture,
        points=points,
        backend=dispatch_backend_virtual_gpu()
    )
    hits = trace_shape_batch(
        capture=scene_capture,
        rays=rays,
        backend=dispatch_backend_virtual_gpu()
    )
    batch_surfaces = surface_at_batch(
        capture=scene_capture,
        hits=hits,
        backend=dispatch_backend_auto()
    )
    batch_occlusion = occluded_batch(
        capture=scene_capture,
        rays=shadow_rays,
        backend=dispatch_backend_cpu()
    )

    coarse_distance = distance_world(
        capture=world,
        domain=coarse_domain,
        point=vec3(0.0, 0.0, 0.6)
    )
    fine_normal = normal_world(
        capture=world,
        domain=fine_domain,
        point=vec3(0.0, 0.0, 0.6)
    )
    world_hit = trace_world(
        capture=world,
        domain=fine_domain,
        ray=ray_query(
            origin=vec3(0.0, 0.0, 3.0),
            direction=vec3(0.0, 0.0, -1.0),
            max_distance=6.0,
            min_step=0.05,
            hit_epsilon=0.001,
            max_steps=96
        )
    )
    world_surface = surface_world(capture=world, domain=fine_domain, hit=world_hit)
    world_radiance = radiance_world(
        capture=world,
        domain=fine_domain,
        sample=point_direction_query(
            point=world_hit.position,
            direction=normalize(vec3(0.0, 1.0, 1.0))
        )
    )
    world_medium = medium_world(capture=world, domain=fine_domain, point=world_hit.position)
    return 0
}
"#;

    let mir_a = lower_mir_module_from_source(source);
    let mir_b = lower_mir_module_from_source(source);
    assert_eq!(mir_function_names(&mir_a), mir_function_names(&mir_b));
    let module = lower_inline_module_from_source(source);
    let scenes = scene_ir::lower_module(&module);
    let sphere_scene = scenes.fields.get("sphere_field").expect("sphere scene");
    let sphere_summary = query_plan::SceneSummary {
        name: Some("scene_shape".into()),
        semantics: scene_ir::DistanceSemantics::ExactSignedDistance,
        support_class: scene_ir::SupportClass::Bounded,
        can_coarse_support_pruning: true,
        opaque_boundary: false,
        semantic_root: 1,
        support_root: 1,
        node_count: 2,
        support_node_count: 1,
        leaf_count: 1,
        identity_source_count: 0,
    };
    let distance_plan = query_plan::BatchQueryPlan::for_field(
        query_plan::FieldBatchPlanKind::Distance,
        query_plan::CaptureKind::Field,
    )
    .expect("field plan");
    assert_eq!(
        distance_plan.helper_name,
        "__wr_field_distance_batch_queries"
    );
    assert_eq!(
        sphere_scene.semantics,
        scene_ir::DistanceSemantics::ExactSignedDistance
    );
    assert_eq!(
        distance_plan.candidate_strategy(),
        query_plan::CandidateStrategy::DirectFieldCapture
    );
    assert_eq!(
        distance_plan.pruning_strategy(),
        query_plan::PruningStrategy::None
    );
    assert!(
        matches!(
            distance_plan
                .stages
                .iter()
                .find(|stage| matches!(stage, query_plan::PlanStage::IterateItems { .. }))
                .expect("iterate stage"),
            query_plan::PlanStage::IterateItems {
                item_kind: query_plan::QueryItemKind::PointQuery
            }
        ),
        "expected field batch plan to load point queries"
    );

    let trace_plan = query_plan::BatchQueryPlan::for_shape_query(
        query_plan::BatchQueryKind::Trace,
        query_plan::DispatchBackend::Cpu,
        Some(sphere_summary),
    );
    assert!(trace_plan.preserves_local_hit_context);
    assert_eq!(
        trace_plan.candidate_strategy(),
        query_plan::CandidateStrategy::SupportAcceleratedShapeTraversal
    );
    assert_eq!(
        trace_plan.pruning_strategy(),
        query_plan::PruningStrategy::CullingTable
    );
    assert!(
        trace_plan
            .stages
            .iter()
            .any(|stage| matches!(stage, query_plan::PlanStage::AssembleHitContext))
    );

    let helper_names: BTreeSet<_> = mir_function_names(&mir_a).into_iter().collect();
    for helper in [
        "__wr_field_distance_batch_queries",
        "__wr_field_normal_batch_queries",
        "__wr_scene_trace_batch_queries",
        "__wr_scene_surface_batch_queries",
        "__wr_scene_occluded_batch_queries",
        "__wr_field_distance_capture",
        "__wr_field_normal_capture",
        "__wr_scene_trace_capture",
        "__wr_scene_surface_capture",
        "__wr_scene_radiance_capture",
        "__wr_scene_medium_capture",
        "__wr_world_distance_capture",
        "__wr_world_normal_capture",
        "__wr_world_trace_capture",
        "__wr_world_surface_capture",
        "__wr_world_radiance_capture",
        "__wr_world_medium_capture",
    ] {
        assert!(
            helper_names.contains(helper),
            "expected generated helper `{helper}` to be visible"
        );
    }

    let hit3 = portable::builtin_record("Hit3").expect("Hit3");
    let hit_fields = hit3
        .fields
        .iter()
        .map(|field| field.name)
        .collect::<Vec<_>>();
    for required in [
        "local_position",
        "local_normal",
        "shading_frame",
        "instance_id",
        "repeat_id",
    ] {
        assert!(
            hit_fields.contains(&required),
            "expected Hit3 to expose `{required}`"
        );
    }
}

#[test]
fn phase9_plan_driven_helpers_route_semantic_and_opaque_scenes_to_different_executors() {
    let source = r#"
field exact distance near_field(p: Vec3) -> F32 {
    sphere(radius = 0.65)
}

field conservative distance far_semantic(p: Vec3) -> F32 {
    translate = vec3(10.0, 0.0, 0.0) {
        sphere(radius = 0.5)
    }
}

field conservative distance far_opaque(p: Vec3) -> F32 {
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
        albedo=vec3(0.25, 0.35, 0.45),
        roughness=0.5,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape near_shape {
    field = near_field
    material = shade
    payload = Payload(
        entity_id=u32(1),
        material_id=u32(1),
        actor=ActorHandle(id=u32(1), generation=u32(0))
    )
}

shape far_semantic_shape {
    field = far_semantic
    material = shade
    payload = Payload(
        entity_id=u32(2),
        material_id=u32(2),
        actor=ActorHandle(id=u32(2), generation=u32(0))
    )
}

shape far_opaque_shape {
    field = far_opaque
    material = shade
    payload = Payload(
        entity_id=u32(3),
        material_id=u32(3),
        actor=ActorHandle(id=u32(3), generation=u32(0))
    )
}

shape semantic_scene {
    union {
        provenance_policy = nearest
        use near_shape
        use far_semantic_shape
    }
}

shape opaque_scene {
    union {
        provenance_policy = nearest
        use near_shape
        use far_opaque_shape
    }
}

fn main() -> Integer {
    semantic_capture = capture semantic_scene
    opaque_capture = capture opaque_scene
    point = vec3(0.0, 0.0, 3.0)
    ray = RayQuery(
        origin=vec3(0.0, 0.0, 3.0),
        direction=vec3(0.0, 0.0, -1.0),
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
    semantic_distance = distance_at(capture=semantic_capture, point=point)
    opaque_distance = distance_at(capture=opaque_capture, point=point)
    semantic_normal = normal_at(capture=semantic_capture, point=point)
    opaque_normal = normal_at(capture=opaque_capture, point=point)
    semantic_hit = trace_shape(
        capture=semantic_capture,
        ray=ray_query(
            origin=ray.origin,
            direction=ray.direction,
            max_distance=ray.max_distance,
            min_step=ray.min_step,
            hit_epsilon=ray.hit_epsilon,
            max_steps=ray.max_steps
        )
    )
    opaque_hit = trace_shape(
        capture=opaque_capture,
        ray=ray_query(
            origin=ray.origin,
            direction=ray.direction,
            max_distance=ray.max_distance,
            min_step=ray.min_step,
            hit_epsilon=ray.hit_epsilon,
            max_steps=ray.max_steps
        )
    )
    points = [PointQuery(point=point)]
    rays = [ray]
    semantic_distance_batch = distance_at_batch(
        capture=semantic_capture,
        points=points,
        backend=dispatch_backend_cpu()
    )
    opaque_distance_batch = distance_at_batch(
        capture=opaque_capture,
        points=points,
        backend=dispatch_backend_cpu()
    )
    semantic_trace_batch = trace_shape_batch(
        capture=semantic_capture,
        rays=rays,
        backend=dispatch_backend_cpu()
    )
    opaque_trace_batch = trace_shape_batch(
        capture=opaque_capture,
        rays=rays,
        backend=dispatch_backend_cpu()
    )
    return 0
}
"#;

    let mir_module = lower_mir_module_from_source(source);

    let shape_distance_capture = mir_module
        .functions
        .iter()
        .find(|func| func.name == "__wr_shape_distance_capture")
        .expect("shape distance capture helper");
    let shape_normal_capture = mir_module
        .functions
        .iter()
        .find(|func| func.name == "__wr_shape_normal_capture")
        .expect("shape normal capture helper");
    let shape_trace_capture = mir_module
        .functions
        .iter()
        .find(|func| func.name == "__wr_scene_trace_capture")
        .expect("shape trace capture helper");
    let shape_distance_batch = mir_module
        .functions
        .iter()
        .find(|func| func.name == "__wr_shape_distance_batch_queries")
        .expect("shape distance batch helper");
    let shape_trace_batch = mir_module
        .functions
        .iter()
        .find(|func| func.name == "__wr_scene_trace_batch_queries")
        .expect("shape trace batch helper");

    let distance_capture_targets = direct_call_targets(shape_distance_capture);
    let normal_capture_targets = direct_call_targets(shape_normal_capture);
    let trace_capture_targets = direct_call_targets(shape_trace_capture);
    let distance_batch_targets = direct_call_targets(shape_distance_batch);
    let trace_batch_targets = direct_call_targets(shape_trace_batch);

    assert!(distance_capture_targets.contains("__wr_shape_distance_semantic_scene"));
    assert!(distance_capture_targets.contains("__wr_shape_distance_conservative_opaque_scene"));
    assert!(normal_capture_targets.contains("__wr_shape_distance_semantic_scene"));
    assert!(normal_capture_targets.contains("__wr_shape_distance_conservative_opaque_scene"));
    assert!(trace_capture_targets.contains("__wr_shape_trace_semantic_scene"));
    assert!(trace_capture_targets.contains("__wr_shape_trace_conservative_opaque_scene"));
    assert!(distance_batch_targets.contains("__wr_shape_distance_semantic_scene"));
    assert!(distance_batch_targets.contains("__wr_shape_distance_conservative_opaque_scene"));
    assert!(trace_batch_targets.contains("__wr_shape_trace_semantic_scene"));
    assert!(trace_batch_targets.contains("__wr_shape_trace_conservative_opaque_scene"));
}

#[test]
fn render_implicit_ray_queries_preserve_legacy_domain_trace_budgets() {
    let source = r#"
field exact distance render_budget_field(p: Vec3) -> F32 {
    sphere(radius = 0.5)
}

material render_budget_material(hit: Hit3) -> Surface {
    return Surface(
        albedo = vec3(0.4, 0.5, 0.6),
        roughness = 0.3,
        metalness = 0.0,
        clearcoat = 0.0,
        clearcoat_roughness = 0.0,
        sheen = 0.0,
        emissive = vec3(0.0, 0.0, 0.0)
    )
}

shape render_budget_shape {
    field = render_budget_field
    material = render_budget_material
}

region render_budget_region() {
    place scene = render_budget_shape
}

domain render_budget_domain(world: RegionCapture) {
    geometry_detail = 1
    material = true
    radiance = true
    media = true
    max_distance = 4.5
    min_step = 0.07
    hit_epsilon = 0.003
    max_steps = 33
}

render render_budget_ppm(world: RegionCapture, camera: Camera) {
    domain = render_budget_domain(world = world)
    width = 1
    height = 1
}
"#;

    let mir_module = lower_mir_module_from_source(source);
    let render = mir_module
        .functions
        .iter()
        .find(|func| func.name == "render_budget_ppm")
        .expect("render function");
    let render_args = first_direct_call_args(render, "__wr_render_capture_to_ppm")
        .expect("render should call compiler-owned ppm helper");
    assert_eq!(render_args.len(), 13);
    assert_float_const(&render_args[9], 4.5);
    assert_float_const(&render_args[10], 0.07);
    assert_float_const(&render_args[11], 0.003);
    assert_integer_const(&render_args[12], 33);

    let scene_color = mir_module
        .functions
        .iter()
        .find(|func| func.name == "__wr_render_scene_color_capture")
        .expect("scene color render helper");
    assert_eq!(scene_color.params.len(), 10);
    assert_eq!(
        set_field_local_name(scene_color, "max_distance").as_deref(),
        Some("trace_max_distance")
    );
    assert_eq!(
        set_field_local_name(scene_color, "min_step").as_deref(),
        Some("trace_min_step")
    );
    assert_eq!(
        set_field_local_name(scene_color, "hit_epsilon").as_deref(),
        Some("trace_hit_epsilon")
    );
    assert_eq!(
        set_field_local_name(scene_color, "max_steps").as_deref(),
        Some("trace_max_steps")
    );
}

#[test]
fn phase9_query_plan_matrix_covers_every_batch_family() {
    let field_summary = query_plan::SceneSummary {
        name: Some("field_scene".into()),
        semantics: scene_ir::DistanceSemantics::ExactSignedDistance,
        support_class: scene_ir::SupportClass::Bounded,
        can_coarse_support_pruning: true,
        opaque_boundary: false,
        semantic_root: 1,
        support_root: 1,
        node_count: 1,
        support_node_count: 1,
        leaf_count: 0,
        identity_source_count: 0,
    };
    let shape_summary = query_plan::SceneSummary {
        name: Some("shape_scene".into()),
        semantics: scene_ir::DistanceSemantics::ConservativeLowerBound,
        support_class: scene_ir::SupportClass::Bounded,
        can_coarse_support_pruning: true,
        opaque_boundary: false,
        semantic_root: 2,
        support_root: 2,
        node_count: 2,
        support_node_count: 2,
        leaf_count: 1,
        identity_source_count: 0,
    };
    let opaque_summary = query_plan::SceneSummary {
        name: Some("opaque_shape_scene".into()),
        semantics: scene_ir::DistanceSemantics::UnknownOpaque,
        support_class: scene_ir::SupportClass::Bounded,
        can_coarse_support_pruning: false,
        opaque_boundary: true,
        semantic_root: 3,
        support_root: 3,
        node_count: 2,
        support_node_count: 2,
        leaf_count: 1,
        identity_source_count: 0,
    };

    let cases = [
        (
            query_plan::BatchQueryPlan::for_field_query(
                query_plan::BatchQueryKind::Distance,
                query_plan::CaptureKind::Field,
                query_plan::DispatchBackend::Cpu,
                Some(field_summary.clone()),
            ),
            field_summary.clone(),
            "__wr_field_distance_batch_queries",
            query_plan::BatchQueryKind::Distance,
            query_plan::QueryItemKind::PointQuery,
            query_plan::QueryResultKind::DistanceResult,
            query_plan::PlanExecutor::FieldDistanceCapture,
            query_plan::InternalKernelKind::FieldDistanceCapture,
            false,
            query_plan::CandidateStrategy::DirectFieldCapture,
            query_plan::PruningStrategy::None,
            false,
        ),
        (
            query_plan::BatchQueryPlan::for_field_query(
                query_plan::BatchQueryKind::Normal,
                query_plan::CaptureKind::Field,
                query_plan::DispatchBackend::Cpu,
                Some(field_summary.clone()),
            ),
            field_summary.clone(),
            "__wr_field_normal_batch_queries",
            query_plan::BatchQueryKind::Normal,
            query_plan::QueryItemKind::PointQuery,
            query_plan::QueryResultKind::NormalResult,
            query_plan::PlanExecutor::FieldNormalCapture,
            query_plan::InternalKernelKind::FieldNormalCapture,
            false,
            query_plan::CandidateStrategy::DirectFieldCapture,
            query_plan::PruningStrategy::None,
            false,
        ),
        (
            query_plan::BatchQueryPlan::for_field_query(
                query_plan::BatchQueryKind::Distance,
                query_plan::CaptureKind::Shape,
                query_plan::DispatchBackend::Auto,
                Some(shape_summary.clone()),
            ),
            shape_summary.clone(),
            "__wr_shape_distance_batch_queries",
            query_plan::BatchQueryKind::Distance,
            query_plan::QueryItemKind::PointQuery,
            query_plan::QueryResultKind::DistanceResult,
            query_plan::PlanExecutor::ShapeDistanceCapture,
            query_plan::InternalKernelKind::ShapeDistanceCapture,
            false,
            query_plan::CandidateStrategy::SupportAcceleratedShapeTraversal,
            query_plan::PruningStrategy::CullingTable,
            true,
        ),
        (
            query_plan::BatchQueryPlan::for_field_query(
                query_plan::BatchQueryKind::Normal,
                query_plan::CaptureKind::Shape,
                query_plan::DispatchBackend::Auto,
                Some(shape_summary.clone()),
            ),
            shape_summary.clone(),
            "__wr_shape_normal_batch_queries",
            query_plan::BatchQueryKind::Normal,
            query_plan::QueryItemKind::PointQuery,
            query_plan::QueryResultKind::NormalResult,
            query_plan::PlanExecutor::ShapeNormalCapture,
            query_plan::InternalKernelKind::ShapeNormalCapture,
            false,
            query_plan::CandidateStrategy::SupportAcceleratedShapeTraversal,
            query_plan::PruningStrategy::CullingTable,
            true,
        ),
        (
            query_plan::BatchQueryPlan::for_shape_query(
                query_plan::BatchQueryKind::Trace,
                query_plan::DispatchBackend::Cpu,
                Some(shape_summary.clone()),
            ),
            shape_summary.clone(),
            "__wr_scene_trace_batch_queries",
            query_plan::BatchQueryKind::Trace,
            query_plan::QueryItemKind::RayQuery,
            query_plan::QueryResultKind::Hit3,
            query_plan::PlanExecutor::SceneTraceCapture,
            query_plan::InternalKernelKind::ShapeTraceCapture,
            true,
            query_plan::CandidateStrategy::SupportAcceleratedShapeTraversal,
            query_plan::PruningStrategy::CullingTable,
            false,
        ),
        (
            query_plan::BatchQueryPlan::for_shape_query(
                query_plan::BatchQueryKind::Surface,
                query_plan::DispatchBackend::Cpu,
                Some(shape_summary.clone()),
            ),
            shape_summary.clone(),
            "__wr_scene_surface_batch_queries",
            query_plan::BatchQueryKind::Surface,
            query_plan::QueryItemKind::Hit3,
            query_plan::QueryResultKind::Surface,
            query_plan::PlanExecutor::SceneSurfaceCapture,
            query_plan::InternalKernelKind::ShapeSurfaceCapture,
            false,
            query_plan::CandidateStrategy::SurfaceHitReuse,
            query_plan::PruningStrategy::None,
            false,
        ),
        (
            query_plan::BatchQueryPlan::for_shape_query(
                query_plan::BatchQueryKind::Occluded,
                query_plan::DispatchBackend::VirtualGpu,
                Some(opaque_summary.clone()),
            ),
            opaque_summary.clone(),
            "__wr_scene_occluded_batch_queries",
            query_plan::BatchQueryKind::Occluded,
            query_plan::QueryItemKind::RayQuery,
            query_plan::QueryResultKind::OcclusionResult,
            query_plan::PlanExecutor::SceneTraceCapture,
            query_plan::InternalKernelKind::ShapeOccludedCapture,
            true,
            query_plan::CandidateStrategy::OpaqueFallback,
            query_plan::PruningStrategy::OpaquePessimizationBoundary,
            true,
        ),
    ];

    for (
        plan,
        expected_scene,
        helper_name,
        batch_kind,
        item_kind,
        result_kind,
        executor,
        kernel,
        preserves_local_hit_context,
        candidate_strategy,
        pruning_strategy,
        requires_virtual_gpu_scaffolding,
    ) in cases
    {
        let (expected_contract_id, expected_family, expected_surface) =
            match (batch_kind, plan.capture_kind) {
                (query_plan::BatchQueryKind::Distance, query_plan::CaptureKind::Field) => (
                    query_contract::SPATIAL_DISTANCE_BATCH_FIELD,
                    query_contract::QueryFamilyId::Spatial,
                    query_contract::QuerySurfaceKind::CaptureBatch,
                ),
                (query_plan::BatchQueryKind::Distance, query_plan::CaptureKind::Shape) => (
                    query_contract::SPATIAL_DISTANCE_BATCH_SHAPE,
                    query_contract::QueryFamilyId::Spatial,
                    query_contract::QuerySurfaceKind::CaptureBatch,
                ),
                (query_plan::BatchQueryKind::Normal, query_plan::CaptureKind::Field) => (
                    query_contract::SPATIAL_NORMAL_BATCH_FIELD,
                    query_contract::QueryFamilyId::Spatial,
                    query_contract::QuerySurfaceKind::CaptureBatch,
                ),
                (query_plan::BatchQueryKind::Normal, query_plan::CaptureKind::Shape) => (
                    query_contract::SPATIAL_NORMAL_BATCH_SHAPE,
                    query_contract::QueryFamilyId::Spatial,
                    query_contract::QuerySurfaceKind::CaptureBatch,
                ),
                (query_plan::BatchQueryKind::Trace, query_plan::CaptureKind::Shape) => (
                    query_contract::SPATIAL_TRACE_BATCH_SHAPE,
                    query_contract::QueryFamilyId::Spatial,
                    query_contract::QuerySurfaceKind::CaptureBatch,
                ),
                (query_plan::BatchQueryKind::Surface, query_plan::CaptureKind::Shape) => (
                    query_contract::SURFACE_SAMPLE_BATCH_SHAPE,
                    query_contract::QueryFamilyId::Surface,
                    query_contract::QuerySurfaceKind::CaptureBatch,
                ),
                (query_plan::BatchQueryKind::Occluded, query_plan::CaptureKind::Shape) => (
                    query_contract::SPATIAL_OCCLUDED_BATCH_SHAPE,
                    query_contract::QueryFamilyId::Spatial,
                    query_contract::QuerySurfaceKind::CaptureBatch,
                ),
                other => panic!("unexpected batch contract case: {other:?}"),
            };
        assert_eq!(plan.contract_id, expected_contract_id);
        assert_eq!(plan.family, expected_family);
        assert_eq!(plan.surface, expected_surface);
        assert_eq!(plan.helper_name, helper_name);
        assert_eq!(plan.kind, batch_kind);
        assert_eq!(plan.item_kind, item_kind);
        assert_eq!(plan.result_kind, result_kind);
        assert_eq!(plan.executor, executor);
        assert_eq!(plan.kernel, kernel);
        assert_eq!(plan.scene, Some(expected_scene));
        assert_eq!(
            plan.preserves_local_hit_context,
            preserves_local_hit_context
        );
        assert_eq!(plan.candidate_strategy(), candidate_strategy);
        assert_eq!(plan.pruning_strategy(), pruning_strategy);
        assert_eq!(
            plan.requires_virtual_gpu_scaffolding(),
            requires_virtual_gpu_scaffolding
        );
        assert!(matches!(
            plan.stages.first(),
            Some(query_plan::PlanStage::SelectBackend)
        ));
        assert!(
            plan.stages
                .iter()
                .any(|stage| matches!(stage, query_plan::PlanStage::LoadCapture))
        );
        assert!(plan.stages.iter().any(|stage| matches!(
            stage,
            query_plan::PlanStage::Execute { executor: stage_executor } if *stage_executor == executor
        )));
        assert!(plan.stages.iter().any(|stage| matches!(
            stage,
            query_plan::PlanStage::AppendResult { result_kind: stage_result } if *stage_result == result_kind
        )));
        assert!(plan.stages.iter().any(|stage| matches!(
            stage,
            query_plan::PlanStage::GenerateCandidates { strategy: stage_strategy }
                if *stage_strategy == candidate_strategy
        )));
        assert!(plan.stages.iter().any(|stage| matches!(
            stage,
            query_plan::PlanStage::PruneCandidates { strategy: stage_strategy }
                if *stage_strategy == pruning_strategy
        )));
        assert_eq!(
            plan.stages
                .iter()
                .any(|stage| matches!(stage, query_plan::PlanStage::AssembleHitContext)),
            preserves_local_hit_context
        );
        assert!(plan.derived_artifacts.iter().any(|artifact| matches!(
            artifact,
            query_plan::DerivedArtifact::SupportSummary { .. }
        )));
        assert!(
            plan.derived_artifacts.iter().any(|artifact| matches!(
                artifact,
                query_plan::DerivedArtifact::CaptureCache { .. }
            ))
        );
        if matches!(
            candidate_strategy,
            query_plan::CandidateStrategy::SupportAcceleratedShapeTraversal
        ) || matches!(pruning_strategy, query_plan::PruningStrategy::CullingTable)
        {
            assert!(plan.derived_artifacts.iter().any(|artifact| matches!(
                artifact,
                query_plan::DerivedArtifact::CullingTable { .. }
            )));
        }
        if matches!(
            pruning_strategy,
            query_plan::PruningStrategy::OpaquePessimizationBoundary
        ) {
            assert!(plan.derived_artifacts.iter().any(|artifact| matches!(
                artifact,
                query_plan::DerivedArtifact::OpaquePessimizationBoundary
            )));
        }
    }
}

#[test]
fn phase9_contract_accessors_follow_explicit_contracts() {
    let summary = query_plan::SceneSummary {
        name: Some("shape_scene".into()),
        semantics: scene_ir::DistanceSemantics::ConservativeLowerBound,
        support_class: scene_ir::SupportClass::Bounded,
        can_coarse_support_pruning: true,
        opaque_boundary: false,
        semantic_root: 1,
        support_root: 1,
        node_count: 2,
        support_node_count: 2,
        leaf_count: 1,
        identity_source_count: 0,
    };
    let mut batch_plan = query_plan::BatchQueryPlan::for_shape_query(
        query_plan::BatchQueryKind::Trace,
        query_plan::DispatchBackend::VirtualGpu,
        Some(summary.clone()),
    );
    assert!(batch_plan.requests_culling_table());

    batch_plan.candidate_contract.candidate_strategy =
        query_plan::CandidateStrategy::ShapeBranchTraversal;
    batch_plan.candidate_contract.pruning_strategy =
        query_plan::PruningStrategy::ConservativeTraversal;
    batch_plan.artifact_contracts.clear();
    assert_eq!(
        batch_plan.candidate_strategy(),
        query_plan::CandidateStrategy::ShapeBranchTraversal
    );
    assert_eq!(
        batch_plan.pruning_strategy(),
        query_plan::PruningStrategy::ConservativeTraversal
    );
    assert!(!batch_plan.requests_culling_table());
    assert!(!batch_plan.has_opaque_pessimization_boundary());

    let mut capture_plan = query_plan::CaptureQueryPlan::for_query(
        query_plan::CaptureQueryKind::Trace,
        query_plan::CaptureKind::Shape,
        Some(summary),
    )
    .expect("capture trace plan");
    capture_plan.candidate_contract.candidate_strategy =
        query_plan::CandidateStrategy::OpaqueFallback;
    capture_plan.candidate_contract.pruning_strategy =
        query_plan::PruningStrategy::OpaquePessimizationBoundary;
    capture_plan.artifact_contracts = vec![query_plan::ArtifactContract {
        id: "opaque-boundary".into(),
        schema: query_plan::ArtifactSchema::OpaquePessimizationBoundary {
            support_root: 1,
            support_node_count: 1,
        },
        producer: "test".into(),
        consumer: "test".into(),
        deterministic: true,
        version: query_plan::QUERY_PLAN_CONTRACT_VERSION,
    }];
    assert_eq!(
        capture_plan.candidate_strategy(),
        query_plan::CandidateStrategy::OpaqueFallback
    );
    assert_eq!(
        capture_plan.pruning_strategy(),
        query_plan::PruningStrategy::OpaquePessimizationBoundary
    );
    assert!(capture_plan.has_opaque_pessimization_boundary());
}

#[test]
fn phase9_shape_trace_plan_captures_support_pruning_when_scene_summary_allows_it() {
    let plan = query_plan::BatchQueryPlan::for_shape_query(
        query_plan::BatchQueryKind::Trace,
        query_plan::DispatchBackend::Cpu,
        Some(query_plan::SceneSummary {
            name: Some("bounded_scene".into()),
            semantics: scene_ir::DistanceSemantics::ConservativeLowerBound,
            support_class: scene_ir::SupportClass::Bounded,
            can_coarse_support_pruning: true,
            opaque_boundary: false,
            semantic_root: 1,
            support_root: 1,
            node_count: 2,
            support_node_count: 2,
            leaf_count: 1,
            identity_source_count: 0,
        }),
    );

    assert!(plan.scene.is_some());
    assert!(!plan.requires_virtual_gpu_scaffolding());
    assert_eq!(
        plan.candidate_strategy(),
        query_plan::CandidateStrategy::SupportAcceleratedShapeTraversal
    );
    assert_eq!(
        plan.pruning_strategy(),
        query_plan::PruningStrategy::CullingTable
    );
    assert!(plan.stages.iter().any(|stage| matches!(
        stage,
        query_plan::PlanStage::GenerateCandidates {
            strategy: query_plan::CandidateStrategy::SupportAcceleratedShapeTraversal
        }
    )));
    assert!(plan.stages.iter().any(|stage| matches!(
        stage,
        query_plan::PlanStage::PruneCandidates {
            strategy: query_plan::PruningStrategy::CullingTable
        }
    )));
    assert!(
        plan.derived_artifacts
            .iter()
            .any(|artifact| matches!(artifact, query_plan::DerivedArtifact::SupportSummary { .. }))
    );
    assert!(
        plan.derived_artifacts
            .iter()
            .any(|artifact| matches!(artifact, query_plan::DerivedArtifact::CaptureCache { .. }))
    );
    assert!(
        plan.derived_artifacts
            .iter()
            .any(|artifact| matches!(artifact, query_plan::DerivedArtifact::CullingTable { .. }))
    );
}

#[test]
fn phase9_capture_query_plans_cover_field_shape_and_participant_paths() {
    let field_summary = query_plan::SceneSummary {
        name: Some("field_scene".into()),
        semantics: scene_ir::DistanceSemantics::ExactSignedDistance,
        support_class: scene_ir::SupportClass::Bounded,
        can_coarse_support_pruning: true,
        opaque_boundary: false,
        semantic_root: 1,
        support_root: 1,
        node_count: 1,
        support_node_count: 1,
        leaf_count: 0,
        identity_source_count: 0,
    };
    let shape_summary = query_plan::SceneSummary {
        name: Some("shape_scene".into()),
        semantics: scene_ir::DistanceSemantics::ConservativeLowerBound,
        support_class: scene_ir::SupportClass::Bounded,
        can_coarse_support_pruning: true,
        opaque_boundary: false,
        semantic_root: 2,
        support_root: 2,
        node_count: 2,
        support_node_count: 2,
        leaf_count: 1,
        identity_source_count: 0,
    };
    let opaque_summary = query_plan::SceneSummary {
        name: Some("opaque_shape_scene".into()),
        semantics: scene_ir::DistanceSemantics::UnknownOpaque,
        support_class: scene_ir::SupportClass::Bounded,
        can_coarse_support_pruning: false,
        opaque_boundary: true,
        semantic_root: 3,
        support_root: 3,
        node_count: 2,
        support_node_count: 2,
        leaf_count: 1,
        identity_source_count: 0,
    };

    let field_distance = query_plan::CaptureQueryPlan::for_query(
        query_plan::CaptureQueryKind::Distance,
        query_plan::CaptureKind::Field,
        Some(field_summary.clone()),
    )
    .expect("field distance plan");
    let field_normal = query_plan::CaptureQueryPlan::for_query(
        query_plan::CaptureQueryKind::Normal,
        query_plan::CaptureKind::Field,
        Some(field_summary.clone()),
    )
    .expect("field normal plan");
    let shape_distance = query_plan::CaptureQueryPlan::for_query(
        query_plan::CaptureQueryKind::Distance,
        query_plan::CaptureKind::Shape,
        Some(shape_summary.clone()),
    )
    .expect("shape distance plan");
    let shape_normal = query_plan::CaptureQueryPlan::for_query(
        query_plan::CaptureQueryKind::Normal,
        query_plan::CaptureKind::Shape,
        Some(shape_summary.clone()),
    )
    .expect("shape normal plan");
    let trace = query_plan::CaptureQueryPlan::for_query(
        query_plan::CaptureQueryKind::Trace,
        query_plan::CaptureKind::Shape,
        Some(opaque_summary.clone()),
    )
    .expect("trace plan");
    let surface = query_plan::CaptureQueryPlan::for_query(
        query_plan::CaptureQueryKind::Surface,
        query_plan::CaptureKind::Shape,
        Some(shape_summary.clone()),
    )
    .expect("surface plan");
    let radiance = query_plan::CaptureQueryPlan::for_query(
        query_plan::CaptureQueryKind::Radiance,
        query_plan::CaptureKind::Shape,
        Some(shape_summary.clone()),
    )
    .expect("radiance plan");
    let medium = query_plan::CaptureQueryPlan::for_query(
        query_plan::CaptureQueryKind::Medium,
        query_plan::CaptureKind::Shape,
        Some(shape_summary.clone()),
    )
    .expect("medium plan");

    for (plan, contract_id, family) in [
        (
            &field_distance,
            query_contract::SPATIAL_DISTANCE_CAPTURE_FIELD,
            query_contract::QueryFamilyId::Spatial,
        ),
        (
            &field_normal,
            query_contract::SPATIAL_NORMAL_CAPTURE_FIELD,
            query_contract::QueryFamilyId::Spatial,
        ),
        (
            &shape_distance,
            query_contract::SPATIAL_DISTANCE_CAPTURE_SHAPE,
            query_contract::QueryFamilyId::Spatial,
        ),
        (
            &shape_normal,
            query_contract::SPATIAL_NORMAL_CAPTURE_SHAPE,
            query_contract::QueryFamilyId::Spatial,
        ),
        (
            &trace,
            query_contract::SPATIAL_TRACE_CAPTURE_SHAPE,
            query_contract::QueryFamilyId::Spatial,
        ),
        (
            &surface,
            query_contract::SURFACE_SAMPLE_CAPTURE_SHAPE,
            query_contract::QueryFamilyId::Surface,
        ),
        (
            &radiance,
            query_contract::PARTICIPANTS_RADIANCE_CAPTURE_SHAPE,
            query_contract::QueryFamilyId::Participants,
        ),
        (
            &medium,
            query_contract::PARTICIPANTS_MEDIUM_CAPTURE_SHAPE,
            query_contract::QueryFamilyId::Participants,
        ),
    ] {
        assert_eq!(plan.contract_id, contract_id);
        assert_eq!(plan.family, family);
        assert_eq!(
            plan.surface,
            query_contract::QuerySurfaceKind::CaptureScalar
        );
    }

    for plan in [&field_distance, &field_normal] {
        assert_eq!(plan.capture_kind, query_plan::CaptureKind::Field);
        assert_eq!(plan.scene, Some(field_summary.clone()));
        assert_eq!(
            plan.candidate_strategy(),
            query_plan::CandidateStrategy::DirectFieldCapture
        );
        assert_eq!(plan.pruning_strategy(), query_plan::PruningStrategy::None);
        assert!(plan.stages.iter().any(|stage| matches!(
            stage,
            query_plan::PlanStage::GenerateCandidates {
                strategy: query_plan::CandidateStrategy::DirectFieldCapture
            }
        )));
        assert!(plan.stages.iter().any(|stage| matches!(
            stage,
            query_plan::PlanStage::PruneCandidates {
                strategy: query_plan::PruningStrategy::None
            }
        )));
        assert!(plan.stages.iter().all(|stage| !matches!(
            stage,
            query_plan::PlanStage::SelectParticipants { .. }
                | query_plan::PlanStage::AssembleHitContext
        )));
        assert!(plan.derived_artifacts.iter().any(|artifact| matches!(
            artifact,
            query_plan::DerivedArtifact::SupportSummary { .. }
        )));
        assert!(plan.derived_artifacts.iter().any(|artifact| matches!(
            artifact,
            query_plan::DerivedArtifact::CaptureCache {
                capture_kind: query_plan::CaptureKind::Field
            }
        )));
        assert_eq!(
            plan.stages
                .iter()
                .filter(|stage| matches!(stage, query_plan::PlanStage::LoadDerivedArtifact { .. }))
                .count(),
            plan.derived_artifacts.len()
        );
    }

    for plan in [&shape_distance, &shape_normal, &radiance, &medium] {
        assert_eq!(plan.capture_kind, query_plan::CaptureKind::Shape);
        assert_eq!(plan.scene, Some(shape_summary.clone()));
        assert_eq!(
            plan.candidate_strategy(),
            query_plan::CandidateStrategy::SupportAcceleratedShapeTraversal
        );
        assert_eq!(
            plan.pruning_strategy(),
            query_plan::PruningStrategy::CullingTable
        );
        assert!(plan.requests_culling_table());
        assert!(!plan.has_opaque_pessimization_boundary());
        assert!(plan.stages.iter().any(|stage| matches!(
            stage,
            query_plan::PlanStage::GenerateCandidates {
                strategy: query_plan::CandidateStrategy::SupportAcceleratedShapeTraversal
            }
        )));
        assert!(plan.stages.iter().any(|stage| matches!(
            stage,
            query_plan::PlanStage::PruneCandidates {
                strategy: query_plan::PruningStrategy::CullingTable
            }
        )));
        assert!(plan.derived_artifacts.iter().any(|artifact| matches!(
            artifact,
            query_plan::DerivedArtifact::SupportSummary { .. }
        )));
        assert!(plan.derived_artifacts.iter().any(|artifact| matches!(
            artifact,
            query_plan::DerivedArtifact::CaptureCache {
                capture_kind: query_plan::CaptureKind::Shape
            }
        )));
        assert!(
            plan.derived_artifacts.iter().any(|artifact| matches!(
                artifact,
                query_plan::DerivedArtifact::CullingTable { .. }
            ))
        );
        assert_eq!(
            plan.stages
                .iter()
                .filter(|stage| matches!(stage, query_plan::PlanStage::LoadDerivedArtifact { .. }))
                .count(),
            plan.derived_artifacts.len()
        );
    }

    assert_eq!(trace.helper_name, "__wr_scene_trace_capture");
    assert_eq!(trace.scene, Some(opaque_summary.clone()));
    assert_eq!(trace.result_kind, query_plan::QueryResultKind::Hit3);
    assert_eq!(trace.executor, query_plan::PlanExecutor::SceneTraceCapture);
    assert!(trace.preserves_local_hit_context);
    assert_eq!(
        trace.candidate_strategy(),
        query_plan::CandidateStrategy::OpaqueFallback
    );
    assert_eq!(
        trace.pruning_strategy(),
        query_plan::PruningStrategy::OpaquePessimizationBoundary
    );
    assert!(trace.has_opaque_pessimization_boundary());
    assert!(!trace.requests_culling_table());
    assert!(trace.stages.iter().any(|stage| matches!(
        stage,
        query_plan::PlanStage::GenerateCandidates {
            strategy: query_plan::CandidateStrategy::OpaqueFallback
        }
    )));
    assert!(trace.stages.iter().any(|stage| matches!(
        stage,
        query_plan::PlanStage::PruneCandidates {
            strategy: query_plan::PruningStrategy::OpaquePessimizationBoundary
        }
    )));
    assert!(
        trace
            .stages
            .iter()
            .any(|stage| matches!(stage, query_plan::PlanStage::AssembleHitContext))
    );
    assert!(trace.derived_artifacts.iter().any(|artifact| matches!(
        artifact,
        query_plan::DerivedArtifact::OpaquePessimizationBoundary
    )));
    assert!(
        !trace
            .derived_artifacts
            .iter()
            .any(|artifact| matches!(artifact, query_plan::DerivedArtifact::CullingTable { .. }))
    );

    assert_eq!(surface.helper_name, "__wr_scene_surface_capture");
    assert_eq!(surface.scene, Some(shape_summary.clone()));
    assert_eq!(surface.result_kind, query_plan::QueryResultKind::Surface);
    assert_eq!(
        surface.executor,
        query_plan::PlanExecutor::SceneSurfaceCapture
    );
    assert!(!surface.preserves_local_hit_context);
    assert_eq!(
        surface.candidate_strategy(),
        query_plan::CandidateStrategy::SurfaceHitReuse
    );
    assert_eq!(
        surface.pruning_strategy(),
        query_plan::PruningStrategy::None
    );
    assert!(!surface.requests_culling_table());
    assert!(!surface.has_opaque_pessimization_boundary());
    assert!(surface.stages.iter().any(|stage| matches!(
        stage,
        query_plan::PlanStage::GenerateCandidates {
            strategy: query_plan::CandidateStrategy::SurfaceHitReuse
        }
    )));
    assert!(surface.stages.iter().any(|stage| matches!(
        stage,
        query_plan::PlanStage::PruneCandidates {
            strategy: query_plan::PruningStrategy::None
        }
    )));
    assert!(surface.stages.iter().all(|stage| !matches!(
        stage,
        query_plan::PlanStage::SelectParticipants { .. }
            | query_plan::PlanStage::AssembleHitContext
    )));

    assert_eq!(radiance.helper_name, "__wr_scene_radiance_capture");
    assert_eq!(
        radiance.result_kind,
        query_plan::QueryResultKind::RadianceResult
    );
    assert_eq!(
        radiance.executor,
        query_plan::PlanExecutor::SceneRadianceCapture
    );
    assert!(radiance.stages.iter().any(|stage| matches!(
        stage,
        query_plan::PlanStage::SelectParticipants {
            kind: query_plan::CaptureQueryKind::Radiance
        }
    )));

    assert_eq!(medium.helper_name, "__wr_scene_medium_capture");
    assert_eq!(
        medium.result_kind,
        query_plan::QueryResultKind::MediumResult
    );
    assert_eq!(
        medium.executor,
        query_plan::PlanExecutor::SceneMediumCapture
    );
    assert!(medium.stages.iter().any(|stage| matches!(
        stage,
        query_plan::PlanStage::SelectParticipants {
            kind: query_plan::CaptureQueryKind::Medium
        }
    )));
}

#[test]
fn phase9_world_query_plans_cover_domain_backed_queries() {
    let cases = [
        (
            query_plan::WorldQueryPlan::for_query(query_plan::WorldQueryKind::Distance),
            "__wr_world_distance_capture",
            query_plan::QueryResultKind::DistanceResult,
            query_plan::PlanExecutor::WorldDistanceCapture,
            false,
        ),
        (
            query_plan::WorldQueryPlan::for_query(query_plan::WorldQueryKind::Normal),
            "__wr_world_normal_capture",
            query_plan::QueryResultKind::NormalResult,
            query_plan::PlanExecutor::WorldNormalCapture,
            false,
        ),
        (
            query_plan::WorldQueryPlan::for_query(query_plan::WorldQueryKind::Trace),
            "__wr_world_trace_capture",
            query_plan::QueryResultKind::Hit3,
            query_plan::PlanExecutor::WorldTraceCapture,
            true,
        ),
        (
            query_plan::WorldQueryPlan::for_query(query_plan::WorldQueryKind::Surface),
            "__wr_world_surface_capture",
            query_plan::QueryResultKind::Surface,
            query_plan::PlanExecutor::WorldSurfaceCapture,
            false,
        ),
        (
            query_plan::WorldQueryPlan::for_query(query_plan::WorldQueryKind::Radiance),
            "__wr_world_radiance_capture",
            query_plan::QueryResultKind::RadianceResult,
            query_plan::PlanExecutor::WorldRadianceCapture,
            false,
        ),
        (
            query_plan::WorldQueryPlan::for_query(query_plan::WorldQueryKind::Medium),
            "__wr_world_medium_capture",
            query_plan::QueryResultKind::MediumResult,
            query_plan::PlanExecutor::WorldMediumCapture,
            false,
        ),
    ];

    for (plan, helper_name, result_kind, executor, preserves_local_hit_context) in cases {
        let (expected_contract_id, expected_family) = match plan.kind {
            query_plan::WorldQueryKind::Distance => (
                query_contract::SPATIAL_DISTANCE_WORLD,
                query_contract::QueryFamilyId::Spatial,
            ),
            query_plan::WorldQueryKind::Normal => (
                query_contract::SPATIAL_NORMAL_WORLD,
                query_contract::QueryFamilyId::Spatial,
            ),
            query_plan::WorldQueryKind::Trace => (
                query_contract::SPATIAL_TRACE_WORLD,
                query_contract::QueryFamilyId::Spatial,
            ),
            query_plan::WorldQueryKind::Surface => (
                query_contract::SURFACE_SAMPLE_WORLD,
                query_contract::QueryFamilyId::Surface,
            ),
            query_plan::WorldQueryKind::Radiance => (
                query_contract::PARTICIPANTS_RADIANCE_WORLD,
                query_contract::QueryFamilyId::Participants,
            ),
            query_plan::WorldQueryKind::Medium => (
                query_contract::PARTICIPANTS_MEDIUM_WORLD,
                query_contract::QueryFamilyId::Participants,
            ),
        };
        assert_eq!(plan.contract_id, expected_contract_id);
        assert_eq!(plan.family, expected_family);
        assert_eq!(plan.surface, query_contract::QuerySurfaceKind::WorldScalar);
        assert_eq!(plan.helper_name, helper_name);
        assert_eq!(plan.backend, query_plan::DispatchBackend::Auto);
        assert_eq!(plan.result_kind, result_kind);
        assert_eq!(plan.executor, executor);
        assert_eq!(
            plan.dispatch_contract.backend,
            query_plan::DispatchBackend::Auto
        );
        assert_eq!(
            plan.preserves_local_hit_context,
            preserves_local_hit_context
        );
        assert!(
            plan.stages
                .iter()
                .any(|stage| matches!(stage, query_plan::PlanStage::LoadDomainFlags))
        );
        if matches!(result_kind, query_plan::QueryResultKind::RadianceResult) {
            assert!(plan.stages.iter().any(|stage| matches!(
                stage,
                query_plan::PlanStage::SelectParticipants {
                    kind: query_plan::CaptureQueryKind::Radiance
                }
            )));
        }
        if matches!(result_kind, query_plan::QueryResultKind::MediumResult) {
            assert!(plan.stages.iter().any(|stage| matches!(
                stage,
                query_plan::PlanStage::SelectParticipants {
                    kind: query_plan::CaptureQueryKind::Medium
                }
            )));
        }
        if matches!(result_kind, query_plan::QueryResultKind::Hit3) {
            assert!(
                plan.stages
                    .iter()
                    .any(|stage| matches!(stage, query_plan::PlanStage::AssembleHitContext))
            );
        }
        assert!(plan.stages.iter().any(|stage| matches!(
            stage,
            query_plan::PlanStage::Execute { executor: stage_executor } if *stage_executor == executor
        )));
        assert!(plan.stages.iter().any(|stage| matches!(
            stage,
            query_plan::PlanStage::AppendResult { result_kind: stage_result } if *stage_result == result_kind
        )));
    }
}

#[test]
fn phase11_world_query_plans_preserve_specialization_and_artifact_contracts() {
    let distance = query_plan::WorldQueryPlan::for_query(query_plan::WorldQueryKind::Distance);
    assert_eq!(distance.contract_id, query_contract::SPATIAL_DISTANCE_WORLD);
    assert_eq!(distance.family, query_contract::QueryFamilyId::Spatial);
    assert_eq!(
        distance.surface,
        query_contract::QuerySurfaceKind::WorldScalar
    );
    assert_eq!(
        distance.candidate_strategy(),
        query_plan::CandidateStrategy::SupportAcceleratedShapeTraversal
    );
    assert_eq!(
        distance.pruning_strategy(),
        query_plan::PruningStrategy::SupportLowerBound
    );
    assert!(distance.requests_culling_table());
    assert!(distance.derived_artifacts.iter().any(|artifact| matches!(
        artifact,
        query_plan::DerivedArtifact::CaptureCache {
            capture_kind: query_plan::CaptureKind::Region
        }
    )));
    assert!(distance.artifact_contracts.iter().any(|artifact| matches!(
        artifact.schema,
        query_plan::ArtifactSchema::CullingTable {
            candidate_strategy: query_plan::CandidateStrategy::SupportAcceleratedShapeTraversal,
            pruning_strategy: query_plan::PruningStrategy::SupportLowerBound,
            ..
        }
    )));

    let trace = query_plan::WorldQueryPlan::for_query(query_plan::WorldQueryKind::Trace);
    assert!(trace.preserves_local_hit_context);
    assert_eq!(
        trace.candidate_contract.winner_mode,
        query_plan::WinnerSelectionMode::Nearest
    );
    assert!(
        trace
            .stages
            .iter()
            .any(|stage| matches!(stage, query_plan::PlanStage::AssembleHitContext))
    );

    let surface = query_plan::WorldQueryPlan::for_query(query_plan::WorldQueryKind::Surface);
    assert_eq!(
        surface.candidate_strategy(),
        query_plan::CandidateStrategy::SurfaceHitReuse
    );
    assert_eq!(
        surface.pruning_strategy(),
        query_plan::PruningStrategy::None
    );
    assert_eq!(
        surface.domain_flags,
        vec![query_plan::SceneDomainFlag::Material]
    );
    assert!(!surface.requests_culling_table());
    assert_eq!(
        surface.candidate_contract.winner_mode,
        query_plan::WinnerSelectionMode::SurfaceReuse
    );

    let radiance = query_plan::WorldQueryPlan::for_query(query_plan::WorldQueryKind::Radiance);
    assert_eq!(
        radiance.candidate_strategy(),
        query_plan::CandidateStrategy::ShapeBranchTraversal
    );
    assert_eq!(
        radiance.pruning_strategy(),
        query_plan::PruningStrategy::ConservativeTraversal
    );
    assert_eq!(
        radiance.domain_flags,
        vec![query_plan::SceneDomainFlag::Radiance]
    );
    assert!(radiance.participant_contract.is_some());
    assert!(radiance.stages.iter().any(|stage| matches!(
        stage,
        query_plan::PlanStage::SelectParticipants {
            kind: query_plan::CaptureQueryKind::Radiance
        }
    )));

    let medium = query_plan::WorldQueryPlan::for_query(query_plan::WorldQueryKind::Medium);
    assert_eq!(
        medium.candidate_strategy(),
        query_plan::CandidateStrategy::ShapeBranchTraversal
    );
    assert_eq!(
        medium.pruning_strategy(),
        query_plan::PruningStrategy::ConservativeTraversal
    );
    assert_eq!(
        medium.domain_flags,
        vec![query_plan::SceneDomainFlag::Media]
    );
    assert!(medium.participant_contract.is_some());
    assert!(medium.stages.iter().any(|stage| matches!(
        stage,
        query_plan::PlanStage::SelectParticipants {
            kind: query_plan::CaptureQueryKind::Medium
        }
    )));
}
