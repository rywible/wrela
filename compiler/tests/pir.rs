use std::fs;
use wrela::hir;
use wrela::hir::lower as hir_lower;
use wrela::hir::project::load_project;
use wrela::parser::ast;
use wrela::parser::ast::AstNode;
use wrela::parser::parse;
use wrela::pir;
use wrela::portable;

fn load_module_from_source(source: &str) -> hir::Module {
    let dir = tempfile::tempdir().expect("tempdir");
    let entry_path = dir.path().join("src").join("main.wr");
    fs::create_dir_all(entry_path.parent().expect("src parent")).expect("create src dir");
    fs::write(&entry_path, source).expect("write source");
    let project = load_project(&entry_path).expect("load project");
    project.module
}

fn lower_inline_module_from_source(source: &str) -> hir::Module {
    let node = parse(source);
    let root = ast::Root::cast(node).expect("root");
    hir_lower::lower(root)
}

fn lower_pir_inline(source: &str, entry: &str) -> pir::PirModule {
    let module = lower_inline_module_from_source(source);
    lower_pir_module(module, entry)
}

fn lower_pir_module(module: hir::Module, entry: &str) -> pir::PirModule {
    let semantic = hir::semantic::check_module(&module);
    assert!(
        semantic.errors.is_empty(),
        "semantic errors: {:?}",
        semantic.errors
    );
    let (type_errors, type_info) = hir::typeck::check_module_with_info(&module);
    assert!(type_errors.is_empty(), "type errors: {type_errors:?}");
    pir::lower_portable_entry_by_name(&module, &type_info, entry)
        .unwrap_or_else(|errors| panic!("pir lowering failed: {errors:?}"))
}

fn lower_pir_module_result(
    module: hir::Module,
    entry: &str,
) -> Result<pir::PirModule, Vec<pir::PirLowerError>> {
    let semantic = hir::semantic::check_module(&module);
    assert!(
        semantic.errors.is_empty(),
        "semantic errors: {:?}",
        semantic.errors
    );
    let (type_errors, type_info) = hir::typeck::check_module_with_info(&module);
    assert!(type_errors.is_empty(), "type errors: {type_errors:?}");
    pir::lower_portable_entry_by_name(&module, &type_info, entry)
}

fn assert_semantic_field_graph_backed_pir_body_empty<F>(
    module: &hir::Module,
    field_name: &str,
    check_root: F,
) where
    F: FnOnce(&hir::FieldExpr),
{
    let field = module
        .functions
        .iter()
        .find(|(_, func)| func.name == field_name)
        .map(|(_, func)| func)
        .unwrap_or_else(|| panic!("missing field function '{field_name}'"));
    assert!(
        field
            .body
            .as_ref()
            .expect("field body")
            .root_stmts
            .is_empty(),
        "expected semantic field '{field_name}' body to be empty"
    );
    check_root(&field.field_graph.as_ref().expect("field graph").root);

    let pir = lower_pir_module(module.clone(), field_name);
    let lowered = pir
        .function(field_name)
        .unwrap_or_else(|| panic!("missing PIR function '{field_name}'"));
    assert!(
        lowered.body.is_empty(),
        "expected PIR body for semantic field '{field_name}' to be empty"
    );
}

#[test]
fn portable_builtin_catalog_matches_expected_surface() {
    let mut names = portable::builtin_records()
        .iter()
        .map(|record| record.name)
        .collect::<Vec<_>>();
    names.sort();
    assert_eq!(
        names,
        vec![
            "ActorHandle",
            "Bounds2",
            "Bounds3",
            "Camera",
            "Contact",
            "DispatchBackend",
            "DistanceResult",
            "FieldCapture",
            "Hit3",
            "Light",
            "Medium",
            "NormalResult",
            "OcclusionResult",
            "ParticipantDomainContract",
            "Payload",
            "PointDirectionQuery",
            "PointQuery",
            "Ray3",
            "RayQuery",
            "RegionCapture",
            "SceneDomain",
            "ShapeCapture",
            "SpatialDomainContract",
            "Support3",
            "Surface",
            "SurfaceDomainContract",
            "Transform3",
        ]
    );
    assert_eq!(
        portable::BUILTIN_HELPER_FUNCTIONS,
        &[
            "transform3_identity",
            "bounds2_center",
            "bounds2_size",
            "bounds3_center",
            "bounds3_size",
            "transform_point",
            "transform_vector",
            "transform_normal",
            "compose_transform3",
            "inverse_transform3",
            "capture",
            "circle2",
            "rect2",
            "rounded_rect2",
            "capsule2",
            "segment2",
            "polygon2",
            "polyline2",
            "field_translate_point",
            "field_rotate_point",
            "field_uniform_scale_point",
            "field_affine_transform_point",
            "field_warp_point",
            "field_repeat_linear_point",
            "field_repeat_grid_point",
            "field_radial_repeat_point",
            "field_mirror_array_point",
            "field_instance_array_point",
            "field_sweep_coords",
            "field_smooth_union",
            "field_smooth_intersection",
            "field_smooth_subtract",
            "field_bend_point",
            "field_twist_point",
            "field_taper_point",
            "field_displace_point",
            "field_union",
            "field_intersection",
            "field_subtract",
            "__wr_field_distance_capture",
            "__wr_field_normal_capture",
            "__wr_shape_distance_capture",
            "__wr_shape_normal_capture",
            "__wr_scene_trace_capture",
            "__wr_scene_occluded_capture",
            "__wr_scene_surface_capture",
            "__wr_scene_radiance_capture",
            "__wr_scene_medium_capture",
            "__wr_world_distance_capture",
            "__wr_world_normal_capture",
            "__wr_world_trace_capture",
            "__wr_world_occluded_capture",
            "__wr_world_surface_capture",
            "__wr_world_radiance_capture",
            "__wr_world_medium_capture",
            "__wr_field_distance_batch_queries",
            "__wr_shape_distance_batch_queries",
            "__wr_field_normal_batch_queries",
            "__wr_shape_normal_batch_queries",
            "__wr_scene_trace_batch_queries",
            "__wr_scene_surface_batch_queries",
            "__wr_scene_occluded_batch_queries",
            "__wr_scene_trace_queries",
            "__wr_scene_surface_queries",
        ]
    );

    let ray3 = portable::builtin_record("Ray3").expect("Ray3");
    assert_eq!(
        ray3.fields
            .iter()
            .map(|field| field.name)
            .collect::<Vec<_>>(),
        vec!["origin", "direction"]
    );
    let hit3 = portable::builtin_record("Hit3").expect("Hit3");
    assert_eq!(
        hit3.fields
            .iter()
            .map(|field| field.name)
            .collect::<Vec<_>>(),
        vec![
            "hit",
            "distance",
            "position",
            "normal",
            "local_position",
            "local_normal",
            "shading_frame",
            "steps",
            "feature_id",
            "instance_id",
            "repeat_id",
            "root_shape_id",
            "payload",
        ]
    );
    let payload = portable::builtin_record("Payload").expect("Payload");
    assert_eq!(
        payload
            .fields
            .iter()
            .map(|field| field.name)
            .collect::<Vec<_>>(),
        vec!["entity_id", "material_id", "actor"]
    );
    let support3 = portable::builtin_record("Support3").expect("Support3");
    assert_eq!(
        support3
            .fields
            .iter()
            .map(|field| field.name)
            .collect::<Vec<_>>(),
        vec!["bounds"]
    );
    let field_capture = portable::builtin_record("FieldCapture").expect("FieldCapture");
    assert_eq!(
        field_capture
            .fields
            .iter()
            .map(|field| field.name)
            .collect::<Vec<_>>(),
        vec!["scene_id", "epoch", "root_feature_id"]
    );
    let shape_capture = portable::builtin_record("ShapeCapture").expect("ShapeCapture");
    assert_eq!(
        shape_capture
            .fields
            .iter()
            .map(|field| field.name)
            .collect::<Vec<_>>(),
        vec!["scene_id", "epoch", "root_feature_id"]
    );
    assert!(portable::builtin_record("TraceQuery").is_none());
    assert!(portable::builtin_record("SurfaceQuery").is_none());
    let scene_domain = portable::builtin_record("SceneDomain").expect("SceneDomain");
    assert_eq!(
        scene_domain
            .fields
            .iter()
            .map(|field| field.name)
            .collect::<Vec<_>>(),
        vec!["scene_id", "spatial", "surface", "participants"]
    );
    let spatial = portable::builtin_record("SpatialDomainContract").expect("SpatialDomainContract");
    assert_eq!(
        spatial
            .fields
            .iter()
            .map(|field| field.name)
            .collect::<Vec<_>>(),
        vec!["geometry_detail", "guarantee"]
    );
    let surface_domain =
        portable::builtin_record("SurfaceDomainContract").expect("SurfaceDomainContract");
    assert_eq!(
        surface_domain
            .fields
            .iter()
            .map(|field| field.name)
            .collect::<Vec<_>>(),
        vec!["material"]
    );
    let participants =
        portable::builtin_record("ParticipantDomainContract").expect("ParticipantDomainContract");
    assert_eq!(
        participants
            .fields
            .iter()
            .map(|field| field.name)
            .collect::<Vec<_>>(),
        vec!["radiance", "media"]
    );
    let point_query = portable::builtin_record("PointQuery").expect("PointQuery");
    assert_eq!(
        point_query
            .fields
            .iter()
            .map(|field| field.name)
            .collect::<Vec<_>>(),
        vec!["point"]
    );
    let point_direction_query =
        portable::builtin_record("PointDirectionQuery").expect("PointDirectionQuery");
    assert_eq!(
        point_direction_query
            .fields
            .iter()
            .map(|field| field.name)
            .collect::<Vec<_>>(),
        vec!["point", "direction"]
    );
    let ray_query = portable::builtin_record("RayQuery").expect("RayQuery");
    assert_eq!(
        ray_query
            .fields
            .iter()
            .map(|field| field.name)
            .collect::<Vec<_>>(),
        vec![
            "origin",
            "direction",
            "max_distance",
            "min_step",
            "hit_epsilon",
            "max_steps"
        ]
    );
    let distance_result = portable::builtin_record("DistanceResult").expect("DistanceResult");
    assert_eq!(
        distance_result
            .fields
            .iter()
            .map(|field| field.name)
            .collect::<Vec<_>>(),
        vec!["distance"]
    );
    let normal_result = portable::builtin_record("NormalResult").expect("NormalResult");
    assert_eq!(
        normal_result
            .fields
            .iter()
            .map(|field| field.name)
            .collect::<Vec<_>>(),
        vec!["normal"]
    );
    let occlusion_result = portable::builtin_record("OcclusionResult").expect("OcclusionResult");
    assert_eq!(
        occlusion_result
            .fields
            .iter()
            .map(|field| field.name)
            .collect::<Vec<_>>(),
        vec!["occluded", "distance", "steps"]
    );
    let surface = portable::builtin_record("Surface").expect("Surface");
    assert_eq!(
        surface
            .fields
            .iter()
            .map(|field| field.name)
            .collect::<Vec<_>>(),
        vec![
            "albedo",
            "roughness",
            "metalness",
            "clearcoat",
            "clearcoat_roughness",
            "sheen",
            "emissive",
        ]
    );
    let camera = portable::builtin_record("Camera").expect("Camera");
    assert_eq!(
        camera
            .fields
            .iter()
            .map(|field| field.name)
            .collect::<Vec<_>>(),
        vec!["position", "forward", "up", "vertical_fov_degrees"]
    );
    let light = portable::builtin_record("Light").expect("Light");
    assert_eq!(
        light
            .fields
            .iter()
            .map(|field| field.name)
            .collect::<Vec<_>>(),
        vec!["position", "direction", "intensity", "range"]
    );
}

#[test]
fn lowers_only_reachable_portable_functions_for_entry() {
    let source = r#"
kernel fn helper(seed: I32) -> I32 {
    return seed + i32(1)
}

kernel fn portable_entry(seed: I32) -> I32 {
    return helper(seed=seed) * i32(2)
}

fn unused() -> I32 {
    return i32(99)
}
"#;

    let module = lower_pir_inline(source, "portable_entry");
    let mut names = module
        .functions
        .iter()
        .map(|function| function.name.to_string())
        .collect::<Vec<_>>();
    names.sort();
    assert_eq!(
        names,
        vec!["helper".to_string(), "portable_entry".to_string()]
    );
}

#[test]
fn executes_scalar_kernel_entry_on_cpu() {
    let source = r#"
kernel fn helper(seed: I32) -> I32 {
    pair = Pair(x=seed, y=i32(5))
    return pair.x + pair.y
}

value Pair {
    x: I32
    y: I32
}

kernel fn portable_entry(seed: I32) -> I32 {
    return helper(seed=seed) * i32(2)
}
"#;

    let module = lower_pir_inline(source, "portable_entry");
    let result = pir::execute_entry(&module, vec![pir::PirValue::I32(6)]).expect("execute");
    assert_eq!(result, pir::PirValue::I32(22));
}

#[test]
fn lowers_semantic_field_entry_as_graph_backed_pir_body() {
    let source = r#"
field exact distance sphere_field(p: Vec3) -> F32 {
    sphere(radius = 1.0)
}
"#;

    let module = lower_inline_module_from_source(source);
    assert_semantic_field_graph_backed_pir_body_empty(&module, "sphere_field", |root| {
        assert!(matches!(
            root,
            hir::FieldExpr::Primitive {
                primitive: hir::FieldPrimitive::Sphere,
                ..
            }
        ));
    });
}

#[test]
fn executes_typed_query_batch_records_on_cpu() {
    let source = r#"
value RaySample {
    origin: Vec3
    direction: Vec3
}

value RayBatch {
    r0: RaySample
    r1: RaySample
}

value PointBatch {
    p0: Vec3
    p1: Vec3
    p2: Vec3
}

value DistanceBatch {
    d0: F32
    d1: F32
    d2: F32
}

value HitBatch {
    h0: Hit3
    h1: Hit3
}

value SurfaceBatch {
    s0: Surface
    s1: Surface
}

value OcclusionBatch {
    o0: Boolean
    o1: Boolean
}

value QueryBatchProbe {
    rays: RayBatch
    points: PointBatch
    distances: DistanceBatch
    hits: HitBatch
    surfaces: SurfaceBatch
    occlusion: OcclusionBatch
}

kernel fn portable_entry() -> QueryBatchProbe {
    rays = RayBatch(
        r0=RaySample(
            origin=vec3(0.0, 0.0, 3.0),
            direction=vec3(0.0, 0.0, -1.0)
        ),
        r1=RaySample(
            origin=vec3(0.5, 0.0, 3.0),
            direction=vec3(0.0, 0.0, -1.0)
        )
    )
    points = PointBatch(
        p0=vec3(0.0, 0.0, 2.0),
        p1=vec3(0.0, 0.0, 3.0),
        p2=vec3(0.5, 0.0, 2.0)
    )
    distances = DistanceBatch(d0=1.0, d1=2.0, d2=-0.5)
    hit0 = Hit3(
        hit=true,
        distance=1.0,
        position=vec3(0.0, 0.0, 2.0),
        normal=vec3(0.0, 0.0, 1.0),
        local_position=vec3(0.0, 0.0, 2.0),
        local_normal=vec3(0.0, 0.0, 1.0),
        shading_frame=transform3_identity(),
        steps=12,
        feature_id=u32(7),
        instance_id=u32(11),
        repeat_id=u32(21),
        root_shape_id=u32(99),
        payload=Payload(
            entity_id=u32(7),
            material_id=u32(9),
            actor=ActorHandle(id=u32(1), generation=u32(0))
        )
    )
    hit1 = Hit3(
        hit=true,
        distance=2.0,
        position=vec3(0.5, 0.0, 2.0),
        normal=vec3(0.0, 0.0, 1.0),
        local_position=vec3(0.5, 0.0, 2.0),
        local_normal=vec3(0.0, 0.0, 1.0),
        shading_frame=transform3_identity(),
        steps=13,
        feature_id=u32(8),
        instance_id=u32(12),
        repeat_id=u32(22),
        root_shape_id=u32(99),
        payload=Payload(
            entity_id=u32(7),
            material_id=u32(9),
            actor=ActorHandle(id=u32(1), generation=u32(0))
        )
    )
    hits = HitBatch(h0=hit0, h1=hit1)
    surfaces = SurfaceBatch(
        s0=Surface(
            albedo=vec3(1.0, 0.0, 0.0),
            roughness=0.0,
            metalness=0.0,
            clearcoat=0.0,
            clearcoat_roughness=0.0,
            sheen=0.0,
            emissive=vec3(0.0, 0.0, 0.0)
        ),
        s1=Surface(
            albedo=vec3(1.0, 0.0, 0.0),
            roughness=0.0,
            metalness=0.0,
            clearcoat=0.0,
            clearcoat_roughness=0.0,
            sheen=0.0,
            emissive=vec3(0.0, 0.0, 0.0)
        )
    )
    occlusion = OcclusionBatch(o0=true, o1=false)
    return QueryBatchProbe(
        rays=rays,
        points=points,
        distances=distances,
        hits=hits,
        surfaces=surfaces,
        occlusion=occlusion
    )
}
"#;

    let module = lower_pir_inline(source, "portable_entry");
    let result = pir::execute_entry(&module, Vec::new()).expect("execute");
    let probe = match result {
        pir::PirValue::Struct(value) => value,
        other => panic!("expected QueryBatchProbe struct, got {other:?}"),
    };

    let rays = match probe.field("rays").expect("rays") {
        pir::PirValue::Struct(value) => value,
        other => panic!("expected RayBatch struct, got {other:?}"),
    };
    let points = match probe.field("points").expect("points") {
        pir::PirValue::Struct(value) => value,
        other => panic!("expected PointBatch struct, got {other:?}"),
    };
    let distances = match probe.field("distances").expect("distances") {
        pir::PirValue::Struct(value) => value,
        other => panic!("expected DistanceBatch struct, got {other:?}"),
    };
    let hits = match probe.field("hits").expect("hits") {
        pir::PirValue::Struct(value) => value,
        other => panic!("expected HitBatch struct, got {other:?}"),
    };
    let surfaces = match probe.field("surfaces").expect("surfaces") {
        pir::PirValue::Struct(value) => value,
        other => panic!("expected SurfaceBatch struct, got {other:?}"),
    };
    let occlusion = match probe.field("occlusion").expect("occlusion") {
        pir::PirValue::Struct(value) => value,
        other => panic!("expected OcclusionBatch struct, got {other:?}"),
    };

    let ray0 = match rays.field("r0").expect("r0") {
        pir::PirValue::Struct(value) => value,
        other => panic!("expected RaySample struct, got {other:?}"),
    };
    let hit0 = match hits.field("h0").expect("h0") {
        pir::PirValue::Struct(value) => value,
        other => panic!("expected Hit3 struct, got {other:?}"),
    };
    let hit1 = match hits.field("h1").expect("h1") {
        pir::PirValue::Struct(value) => value,
        other => panic!("expected Hit3 struct, got {other:?}"),
    };
    let surface0 = match surfaces.field("s0").expect("s0") {
        pir::PirValue::Struct(value) => value,
        other => panic!("expected Surface struct, got {other:?}"),
    };

    assert_eq!(
        ray0.field("origin"),
        Some(&pir::PirValue::Vec3([0.0, 0.0, 3.0]))
    );
    assert_eq!(
        points.field("p1"),
        Some(&pir::PirValue::Vec3([0.0, 0.0, 3.0]))
    );
    assert_eq!(distances.field("d0"), Some(&pir::PirValue::F32(1.0)));
    assert_eq!(distances.field("d1"), Some(&pir::PirValue::F32(2.0)));
    assert_eq!(distances.field("d2"), Some(&pir::PirValue::F32(-0.5)));
    assert_eq!(hit0.field("hit"), Some(&pir::PirValue::Bool(true)));
    assert_eq!(hit1.field("hit"), Some(&pir::PirValue::Bool(true)));
    assert_eq!(hit0.field("feature_id"), Some(&pir::PirValue::U32(7)));
    assert_eq!(hit1.field("feature_id"), Some(&pir::PirValue::U32(8)));
    assert_eq!(hit0.field("instance_id"), Some(&pir::PirValue::U32(11)));
    assert_eq!(hit1.field("repeat_id"), Some(&pir::PirValue::U32(22)));
    assert_eq!(
        surface0.field("albedo"),
        Some(&pir::PirValue::Vec3([1.0, 0.0, 0.0]))
    );
    assert_eq!(occlusion.field("o0"), Some(&pir::PirValue::Bool(true)));
    assert_eq!(occlusion.field("o1"), Some(&pir::PirValue::Bool(false)));
}

#[test]
fn trace_shape_provenance_drives_surface_resolution_for_nested_shape_use() {
    let source = r#"
value TraceProbe {
    hit: Hit3
}

kernel fn portable_entry() -> TraceProbe {
    handle = ActorHandle(id=u32(1), generation=u32(0))
    payload = Payload(
        entity_id=u32(77),
        material_id=u32(99),
        actor=handle
    )
    hit = Hit3(
        hit=true,
        distance=1.5,
        position=vec3(-2.0, 0.0, 1.5),
        normal=vec3(0.0, 0.0, 1.0),
        local_position=vec3(-2.0, 0.0, 1.5),
        local_normal=vec3(0.0, 0.0, 1.0),
        shading_frame=transform3_identity(),
        steps=12,
        feature_id=u32(305419896),
        instance_id=u32(17),
        repeat_id=u32(23),
        root_shape_id=u32(77),
        payload=payload
    )
    return TraceProbe(hit=hit)
}
"#;

    let module = lower_pir_inline(source, "portable_entry");
    let result = pir::execute_entry(&module, Vec::new()).expect("execute");
    let probe = match result {
        pir::PirValue::Struct(value) => value,
        other => panic!("expected TraceProbe struct, got {other:?}"),
    };

    let hit = match probe.field("hit").expect("hit") {
        pir::PirValue::Struct(value) => value,
        other => panic!("expected Hit3 struct, got {other:?}"),
    };
    assert_ne!(hit.field("feature_id"), Some(&pir::PirValue::U32(0)));

    assert_eq!(
        hit.field("feature_id"),
        Some(&pir::PirValue::U32(305419896))
    );
    assert_eq!(hit.field("instance_id"), Some(&pir::PirValue::U32(17)));
    assert_eq!(hit.field("repeat_id"), Some(&pir::PirValue::U32(23)));
}

#[test]
fn lowers_semantic_field_composition_as_graph_backed_pir_body() {
    let source = r#"
field conservative distance left_x(p: Vec3) -> F32 {
    plane(normal = vec3(1.0, 0.0, 0.0), offset = 0.0)
}

field conservative distance left_y(p: Vec3) -> F32 {
    plane(normal = vec3(0.0, 1.0, 0.0), offset = 0.0)
}

field conservative distance cap_z(p: Vec3) -> F32 {
    plane(normal = vec3(0.0, 0.0, 1.0), offset = 0.0)
}

field conservative distance notch(p: Vec3) -> F32 {
    plane(normal = vec3(1.0, 0.0, 0.0), offset = -0.5)
}

field conservative distance composed(p: Vec3) -> F32 {
    subtract {
        intersection {
            union {
                use left_x
                use left_y
            }
            use cap_z
        }
        use notch
    }
}
"#;

    let module = lower_inline_module_from_source(source);
    assert_semantic_field_graph_backed_pir_body_empty(&module, "composed", |root| {
        let hir::FieldExpr::Subtract { left, right } = root else {
            panic!("expected subtract root, got {root:?}");
        };
        assert!(matches!(left.as_ref(), hir::FieldExpr::Intersection { .. }));
        assert!(matches!(
            right.as_ref(),
            hir::FieldExpr::Use { target } if target == "notch"
        ));
    });
}

#[test]
fn lowers_field_primitive_catalog_as_graph_backed_pir_bodies() {
    let source = r#"
field exact distance sphere_field(p: Vec3) -> F32 {
    sphere(radius = 1.0)
}

field exact distance box_field(p: Vec3) -> F32 {
    box(half = vec3(1.0, 2.0, 3.0))
}

field exact distance capsule_field(p: Vec3) -> F32 {
    capsule(a = vec3(0.0, -1.0, 0.0), b = vec3(0.0, 1.0, 0.0), radius = 0.5)
}

field exact distance cylinder_field(p: Vec3) -> F32 {
    cylinder(radius = 0.5, half_height = 1.0)
}

field exact distance plane_field(p: Vec3) -> F32 {
    plane(normal = vec3(0.0, 1.0, 0.0), offset = 0.25)
}

field exact distance torus_field(p: Vec3) -> F32 {
    torus(major_radius = 2.0, minor_radius = 0.5)
}
"#;

    let module = lower_inline_module_from_source(source);
    for (field_name, expected) in [
        ("sphere_field", hir::FieldPrimitive::Sphere),
        ("box_field", hir::FieldPrimitive::Box),
        ("capsule_field", hir::FieldPrimitive::Capsule),
        ("cylinder_field", hir::FieldPrimitive::Cylinder),
        ("plane_field", hir::FieldPrimitive::Plane),
        ("torus_field", hir::FieldPrimitive::Torus),
    ] {
        assert_semantic_field_graph_backed_pir_body_empty(&module, field_name, |root| {
            assert!(matches!(
                root,
                hir::FieldExpr::Primitive { primitive, .. } if *primitive == expected
            ));
        });
    }
}

#[test]
fn lowers_phase6_construction_operators_as_graph_backed_pir_bodies() {
    let source = r#"
field conservative distance extruded_disc(p: Vec3) -> F32 {
    extrude = f32(1.6) {
        circle2(radius = 0.75)
    }
}

field conservative distance revolved_orb(p: Vec3) -> F32 {
    revolve {
        circle2(radius = 0.5)
    }
}

field conservative distance swept_beam(p: Vec3) -> F32 {
    sweep = vec3(0.0, 1.6, 0.0) {
        circle2(radius = 0.15)
    }
}

field conservative distance lofted_form(p: Vec3) -> F32 {
    loft = f32(1.2) {
        from rect2(half = vec2(0.25, 0.18))
        to rounded_rect2(half = vec2(0.42, 0.28), radius = 0.08)
    }
}

field conservative distance polygon_plate(p: Vec3) -> F32 {
    extrude = f32(0.4) {
        polygon2(vertices = [
            vec2(-0.4, -0.3),
            vec2(0.5, -0.2),
            vec2(0.3, 0.4),
            vec2(-0.3, 0.35)
        ])
    }
}

field conservative distance capsule_rib(p: Vec3) -> F32 {
    extrude = f32(0.18) {
        capsule2(a = vec2(-0.24, 0.0), b = vec2(0.24, 0.0), radius = 0.06)
    }
}

field conservative distance segment_strip(p: Vec3) -> F32 {
    extrude = f32(0.12) {
        segment2(a = vec2(-0.28, 0.0), b = vec2(0.28, 0.0))
    }
}

field conservative distance polyline_strip(p: Vec3) -> F32 {
    extrude = f32(0.16) {
        polyline2(vertices = [
            vec2(-0.28, -0.10),
            vec2(0.0, 0.14),
            vec2(0.28, -0.10)
        ])
    }
}
"#;

    let module = lower_inline_module_from_source(source);
    for (field_name, expected_variant) in [
        ("extruded_disc", "extrude"),
        ("revolved_orb", "revolve"),
        ("swept_beam", "sweep"),
        ("lofted_form", "loft"),
        ("polygon_plate", "extrude"),
    ] {
        assert_semantic_field_graph_backed_pir_body_empty(&module, field_name, |root| {
            match (expected_variant, root) {
                ("extrude", hir::FieldExpr::Extrude { .. }) => {}
                ("revolve", hir::FieldExpr::Revolve { .. }) => {}
                ("sweep", hir::FieldExpr::Sweep { .. }) => {}
                ("loft", hir::FieldExpr::Loft { .. }) => {}
                other => panic!("unexpected field graph root: {other:?}"),
            }
        });
    }
}

#[test]
fn lowers_phase7_field_entry_as_graph_backed_pir_body() {
    let source = r#"
field exact distance phase7_shell(p: Vec3) -> F32 {
    sphere(radius = 0.45)
}

radiance field phase7_radiance(p: Vec3, direction: Vec3, feature_id: U32) -> Vec3 {
    return vec3(0.25, 0.5, 0.75)
        + direction * 0.0
        + p * 0.0
        + vec3(f32(feature_id) * 0.0, 0.0, 0.0)
}

volume field phase7_volume(p: Vec3, surface_distance: F32) -> Medium {
    return Medium(
        density=clamp(0.12 + abs(surface_distance) * 0.0, 0.0, 1.0),
        emission=vec3(p.x * 0.0, 0.0, 0.0),
        anisotropy=0.0
    )
}

material phase7_surface(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.3, 0.4, 0.5),
        roughness=0.25,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape phase7_scene_shape {
    field = phase7_shell
    material = phase7_surface
    radiance = phase7_radiance
    volume = phase7_volume
    payload = Payload(
        entity_id=u32(901),
        material_id=u32(901),
        actor=ActorHandle(id=u32(901), generation=u32(0))
    )
}
"#;

    let module = lower_inline_module_from_source(source);
    assert_semantic_field_graph_backed_pir_body_empty(&module, "phase7_shell", |root| {
        assert!(matches!(
            root,
            hir::FieldExpr::Primitive {
                primitive: hir::FieldPrimitive::Sphere,
                ..
            }
        ));
    });
}

#[test]
fn executes_phase7_radiance_and_medium_queries_on_cpu() {
    let source = r#"
value Phase7Probe {
    radiance: Vec3
    medium: Medium
    surface: Surface
}

radiance field phase7_radiance(p: Vec3, direction: Vec3, feature_id: U32) -> Vec3 {
    feature_bias = clamp(f32(feature_id), 0.0, 1.0)
    return vec3(0.25, 0.5, 0.75)
        + direction * 0.0
        + p * 0.0
        + vec3(feature_bias * 0.05, 0.0, 0.0)
}

volume field phase7_volume(p: Vec3, surface_distance: F32) -> Medium {
    return Medium(
        density=clamp(0.10 + abs(surface_distance) * 0.2, 0.0, 1.0),
        emission=vec3(abs(p.x) * 0.0, 0.0, 0.0),
        anisotropy=0.0
    )
}

material phase7_surface(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.3, 0.4, 0.5),
        roughness=0.25,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

field exact distance phase7_shell(p: Vec3) -> F32 {
    sphere(radius = 0.45)
}

shape phase7_scene_shape {
    field = phase7_shell
    material = phase7_surface
    radiance = phase7_radiance
    volume = phase7_volume
    payload = Payload(
        entity_id=u32(901),
        material_id=u32(901),
        actor=ActorHandle(id=u32(901), generation=u32(0))
    )
}

kernel fn portable_entry() -> Phase7Probe {
    scene_capture = capture phase7_scene_shape
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
    surface = surface_at(capture=scene_capture, hit=hit)
    radiance_sample = radiance_at(
        capture=scene_capture,
        sample=point_direction_query(
            point=hit.position,
            direction=normalize(vec3(0.0, 1.0, 1.0))
        )
    )
    medium_sample = medium_at(
        capture=scene_capture,
        point=hit.position,
    )
    return Phase7Probe(
        radiance=radiance_sample,
        medium=medium_sample,
        surface=surface
    )
}
"#;

    let module = lower_inline_module_from_source(source);
    let (type_errors, _) = hir::typeck::check_module_with_info(&module);
    assert!(
        type_errors.is_empty(),
        "expected kernel query entry to typecheck, got: {type_errors:?}"
    );
}

#[test]
fn executes_builtin_record_transport_on_cpu() {
    let source = r#"
value SceneProbe {
    surface: Surface
    medium: Medium
    hit: Hit3
    contact: Contact
    light: Light
    support: Support3
    camera: Camera
    pose: Transform3
}

kernel fn portable_entry() -> SceneProbe {
    handle = ActorHandle(id=u32(7), generation=u32(3))
    payload = Payload(entity_id=u32(7), material_id=u32(11), actor=handle)
    hit = Hit3(
        hit=true,
        distance=f32(4.0),
        position=vec3(1.0, 2.0, 3.0),
        normal=vec3(0.0, 1.0, 0.0),
        local_position=vec3(1.0, 2.0, 3.0),
        local_normal=vec3(0.0, 1.0, 0.0),
        shading_frame=transform3_identity(),
        steps=0,
        feature_id=u32(0),
        instance_id=u32(0),
        repeat_id=u32(0),
        root_shape_id=u32(123),
        payload=payload
    )
    surface = Surface(
        albedo=vec3(0.25, 0.5, 0.75),
        roughness=f32(0.125),
        metalness=f32(0.25),
        clearcoat=f32(0.5),
        clearcoat_roughness=f32(0.75),
        sheen=f32(0.1),
        emissive=vec3(1.0, 0.0, 2.0)
    )
    medium = Medium(
        density=f32(0.5),
        emission=vec3(0.5, 0.25, 0.75),
        anisotropy=f32(-0.25)
    )
    bounds = Bounds3(
        min=vec3(0.0, 1.0, 2.0),
        max=vec3(6.0, 7.0, 8.0)
    )
    support = Support3(bounds=bounds)
    contact = Contact(
        hit=true,
        position=hit.position,
        normal=hit.normal,
        penetration=f32(0.5),
        payload=payload
    )
    light = Light(
        position=vec3(2.0, 4.0, 6.0),
        direction=vec3(0.0, -1.0, 0.0),
        intensity=vec3(8.0, 6.0, 4.0),
        range=f32(12.0)
    )
    camera = Camera(
        position=vec3(0.0, 1.0, 2.0),
        forward=vec3(0.0, 0.0, -1.0),
        up=vec3(0.0, 1.0, 0.0),
        vertical_fov_degrees=f32(60.0)
    )
    pose = Transform3(
        matrix=mat4_identity(),
        inverse=mat4_identity()
    )
    return SceneProbe(
        surface=surface,
        medium=medium,
        hit=hit,
        contact=contact,
        light=light,
        support=support,
        camera=camera,
        pose=pose
    )
}
"#;

    let module = lower_pir_inline(source, "portable_entry");
    let result = pir::execute_entry(&module, Vec::new()).expect("execute");
    let scene = match result {
        pir::PirValue::Struct(value) => value,
        other => panic!("expected struct result, got {other:?}"),
    };

    let surface = match scene.field("surface").expect("surface") {
        pir::PirValue::Struct(value) => value,
        other => panic!("expected Surface struct, got {other:?}"),
    };
    assert_eq!(surface.field("roughness"), Some(&pir::PirValue::F32(0.125)));
    assert_eq!(
        surface.field("emissive"),
        Some(&pir::PirValue::Vec3([1.0, 0.0, 2.0]))
    );

    let medium = match scene.field("medium").expect("medium") {
        pir::PirValue::Struct(value) => value,
        other => panic!("expected Medium struct, got {other:?}"),
    };
    assert_eq!(medium.field("density"), Some(&pir::PirValue::F32(0.5)));
    assert_eq!(medium.field("anisotropy"), Some(&pir::PirValue::F32(-0.25)));

    let hit = match scene.field("hit").expect("hit") {
        pir::PirValue::Struct(value) => value,
        other => panic!("expected Hit3 struct, got {other:?}"),
    };
    let payload = match hit.field("payload").expect("payload") {
        pir::PirValue::Struct(value) => value,
        other => panic!("expected Payload struct, got {other:?}"),
    };
    let actor = match payload.field("actor").expect("actor") {
        pir::PirValue::Struct(value) => value,
        other => panic!("expected ActorHandle struct, got {other:?}"),
    };
    assert_eq!(actor.field("id"), Some(&pir::PirValue::U32(7)));
    assert_eq!(actor.field("generation"), Some(&pir::PirValue::U32(3)));

    let support = match scene.field("support").expect("support") {
        pir::PirValue::Struct(value) => value,
        other => panic!("expected Support3 struct, got {other:?}"),
    };
    let bounds = match support.field("bounds").expect("bounds") {
        pir::PirValue::Struct(value) => value,
        other => panic!("expected Bounds3 struct, got {other:?}"),
    };
    assert_eq!(
        bounds.field("max"),
        Some(&pir::PirValue::Vec3([6.0, 7.0, 8.0]))
    );

    let camera = match scene.field("camera").expect("camera") {
        pir::PirValue::Struct(value) => value,
        other => panic!("expected Camera struct, got {other:?}"),
    };
    assert_eq!(
        camera.field("vertical_fov_degrees"),
        Some(&pir::PirValue::F32(60.0))
    );

    let pose = match scene.field("pose").expect("pose") {
        pir::PirValue::Struct(value) => value,
        other => panic!("expected Transform3 struct, got {other:?}"),
    };
    assert_eq!(
        pose.field("matrix"),
        Some(&pir::PirValue::Mat4([
            1.0, 0.0, 0.0, 0.0, //
            0.0, 1.0, 0.0, 0.0, //
            0.0, 0.0, 1.0, 0.0, //
            0.0, 0.0, 0.0, 1.0,
        ]))
    );
}

#[test]
fn authored_field_support_and_bounds_survive_hir_lowering_for_pir_lowering() {
    let source = r#"
field conservative distance hinted(p: Vec3) -> F32 {
    support = Support3(bounds=Bounds3(
        min=vec3(8.0, -1.0, -1.0),
        max=vec3(12.0, 1.0, 1.0)
    ))
    bounds = Bounds3(
        min=vec3(8.0, -1.0, -1.0),
        max=vec3(12.0, 1.0, 1.0)
    )
    translate = vec3(10.0, 0.0, 0.0) {
        sphere(radius=0.5)
    }
}
"#;

    let module = lower_inline_module_from_source(source);
    let field = module
        .functions
        .iter()
        .find(|(_, func)| func.name == "hinted")
        .map(|(_, func)| func)
        .expect("field function");
    let metadata = field.field.as_ref().expect("field metadata");
    assert!(
        metadata.authored_support.is_some(),
        "expected authored support clause"
    );
    assert!(
        metadata.authored_bounds.is_some(),
        "expected authored bounds clause"
    );
    assert!(
        metadata.trace.can_coarse_support_pruning,
        "expected authored support/bounds clauses to keep pruning enabled"
    );
    assert!(matches!(
        field.field_graph.as_ref().expect("field graph").root,
        hir::FieldExpr::Translate { .. }
    ));
    assert_eq!(
        field.field_graph.as_ref().expect("field graph").trace,
        hir::GraphTraceMetadata::conservative(
            hir::FieldSupport::Bounded,
            hir::FieldBounds::Bounded,
            true,
        ),
        "expected the graph trace to reflect authored support/bounds"
    );

    let pir = lower_pir_module(module, "hinted");
    let lowered = pir.function("hinted").expect("pir function");
    assert!(
        lowered.body.is_empty(),
        "expected PIR body for semantic field to stay empty"
    );
}

#[test]
fn executes_bounds_and_transform_helpers_on_cpu() {
    let source = r#"
kernel fn portable_entry() -> I32 {
    bounds2_box = Bounds2(
        min=vec2(1.0, 2.0),
        max=vec2(5.0, 6.0)
    )
    bounds3_box = Bounds3(
        min=vec3(0.0, 1.0, 2.0),
        max=vec3(6.0, 7.0, 8.0)
    )
    ray = Ray3(
        origin=vec3(1.0, 2.0, 3.0),
        direction=normalize(vec3(0.0, 1.0, 0.0))
    )
    pose = Transform3(
        matrix=mat4_identity(),
        inverse=mat4_identity()
    )

    center2 = bounds2_center(bounds=bounds2_box)
    size2 = bounds2_size(bounds=bounds2_box)
    center3 = bounds3_center(bounds=bounds3_box)
    size3 = bounds3_size(bounds=bounds3_box)
    identity = transform3_identity()
    composed = compose_transform3(
        left=pose,
        right=inverse_transform3(transform=identity)
    )
    point = transform_point(transform=composed, point=ray.origin)
    vector = transform_vector(transform=composed, vector=ray.direction)
    normal = transform_normal(transform=composed, normal=vec3(0.0, 1.0, 0.0))

    return i32(center2.x) + i32(center2.y)
        + i32(size2.x) + i32(size2.y)
        + i32(center3.x) + i32(center3.y) + i32(center3.z)
        + i32(size3.x) + i32(size3.y) + i32(size3.z)
        + i32(point.x) + i32(point.y) + i32(point.z)
        + i32(vector.x) + i32(vector.y) + i32(vector.z)
        + i32(normal.x) + i32(normal.y) + i32(normal.z)
}
"#;

    let module = lower_pir_inline(source, "portable_entry");
    let result = pir::execute_entry(&module, Vec::new()).expect("execute");
    assert_eq!(result, pir::PirValue::I32(53));
}

#[test]
fn lowers_structural_field_point_helpers_as_graph_backed_pir_bodies() {
    let source = r#"
field conservative distance translated_sphere(p: Vec3) -> F32 {
    translate = vec3(2.0, 0.0, 0.0) {
        sphere(radius=1.0)
    }
}

field conservative distance mirrored_box(p: Vec3) -> F32 {
    mirror_array = vec3(1.0, 0.0, 0.0) {
        translate = vec3(1.0, 0.0, 0.0) {
            box(half=vec3(0.5, 0.5, 0.5))
        }
    }
}

field conservative distance repeated_sphere(p: Vec3) -> F32 {
    repeat_linear = vec3(2.0, 0.0, 0.0) {
        sphere(radius=0.5)
    }
}

field conservative distance instanced_sphere(p: Vec3) -> F32 {
    instance_array = Transform3(
        matrix=mat4_cols(
            vec4(1.0, 0.0, 0.0, 0.0),
            vec4(0.0, 1.0, 0.0, 0.0),
            vec4(0.0, 0.0, 1.0, 0.0),
            vec4(0.0, 0.0, 1.0, 1.0)
        ),
        inverse=mat4_cols(
            vec4(1.0, 0.0, 0.0, 0.0),
            vec4(0.0, 1.0, 0.0, 0.0),
            vec4(0.0, 0.0, 1.0, 0.0),
            vec4(0.0, 0.0, -1.0, 1.0)
        )
    ) {
        sphere(radius=0.5)
    }
}
"#;

    let module = lower_inline_module_from_source(source);
    for (field_name, expected_variant) in [
        ("translated_sphere", "translate"),
        ("mirrored_box", "mirror"),
        ("repeated_sphere", "repeat"),
        ("instanced_sphere", "instance"),
    ] {
        assert_semantic_field_graph_backed_pir_body_empty(&module, field_name, |root| {
            match (expected_variant, root) {
                ("translate", hir::FieldExpr::Translate { .. }) => {}
                ("mirror", hir::FieldExpr::MirrorArray { .. }) => {}
                ("repeat", hir::FieldExpr::RepeatLinear { .. }) => {}
                ("instance", hir::FieldExpr::InstanceArray { .. }) => {}
                other => panic!("unexpected field graph root: {other:?}"),
            }
        });
    }
}

#[test]
fn executes_arrays_and_value_structs_on_cpu() {
    let source = r#"
value Pair {
    left: I32
    right: I32
}

kernel fn sum(values: Array[I32, 3]) -> I32 {
    return values[0] + values[1] + values[2]
}

kernel fn portable_entry(values: Array[I32, 3]) -> I32 {
    pair = Pair(left=i32(4), right=i32(6))
    return pair.left + pair.right + sum(values=values)
}
"#;

    let module = lower_pir_inline(source, "portable_entry");
    let result = pir::execute_entry(
        &module,
        vec![pir::PirValue::Array(vec![
            pir::PirValue::I32(1),
            pir::PirValue::I32(2),
            pir::PirValue::I32(3),
        ])],
    )
    .expect("execute");
    assert_eq!(result, pir::PirValue::I32(16));
}

#[test]
fn reuses_runtime_vec_math_for_cpu_truth_execution() {
    let source = r#"
kernel fn portable_entry() -> F32 {
    direction = normalize(vec3(3.0, 0.0, 4.0))
    return dot(direction, vec3(0.6, 0.0, 0.8))
}
"#;

    let module = lower_pir_inline(source, "portable_entry");
    let result = pir::execute_entry(&module, Vec::new()).expect("execute");
    match result {
        pir::PirValue::F32(value) => assert!((value - 1.0).abs() < 0.0001, "value={value}"),
        other => panic!("expected F32 result, got {other:?}"),
    }
}

#[test]
fn lowers_project_module_through_portable_path_without_mir() {
    let source = r#"
value Pair {
    x: I32
    y: I32
}

kernel fn helper(pair: Pair) -> I32 {
    return pair.x + pair.y
}

kernel fn portable_entry(seed: I32) -> I32 {
    pair = Pair(x=seed, y=i32(4))
    return helper(pair=pair)
}

fn run() -> I32 {
    return portable_entry(seed=i32(8))
}
"#;

    let module = load_module_from_source(source);
    let semantic = hir::semantic::check_module(&module);
    assert!(
        semantic.errors.is_empty(),
        "semantic errors: {:?}",
        semantic.errors
    );
    let (type_errors, type_info) = hir::typeck::check_module_with_info(&module);
    assert!(type_errors.is_empty(), "type errors: {type_errors:?}");

    let pir = pir::lower_portable_entry_by_name(&module, &type_info, "portable_entry")
        .unwrap_or_else(|errors| panic!("pir lowering failed: {errors:?}"));
    let result = pir::execute_entry(&pir, vec![pir::PirValue::I32(8)]).expect("execute");
    assert_eq!(result, pir::PirValue::I32(12));
}

#[test]
fn rejects_host_function_as_portable_entry() {
    let source = r#"
fn portable_entry(seed: I32) -> I32 {
    return seed + i32(1)
}
"#;

    let module = lower_inline_module_from_source(source);
    let errors = lower_pir_module_result(module, "portable_entry")
        .expect_err("host-lane entry should be rejected");
    assert_eq!(
        errors,
        vec![pir::PirLowerError::EntryNotPortable {
            name: "portable_entry".into(),
        }]
    );
}

#[test]
fn lowers_top_level_kernel_entry_even_when_method_shares_name() {
    let source = r#"
class Shadow {
    fn portable_entry(seed: I32) -> I32 {
        return seed + i32(99)
    }
}

kernel fn portable_entry(seed: I32) -> I32 {
    return seed + i32(1)
}
"#;

    let module = lower_pir_inline(source, "portable_entry");
    let result = pir::execute_entry(&module, vec![pir::PirValue::I32(6)]).expect("execute");
    assert_eq!(result, pir::PirValue::I32(7));
}

#[test]
fn lowers_top_level_kernel_helper_even_when_method_shares_name() {
    let source = r#"
class Shadow {
    fn helper(seed: I32) -> I32 {
        return seed + i32(99)
    }
}

kernel fn helper(seed: I32) -> I32 {
    return seed + i32(1)
}

kernel fn portable_entry(seed: I32) -> I32 {
    return helper(seed=seed)
}
"#;

    let module = lower_pir_inline(source, "portable_entry");
    let result = pir::execute_entry(&module, vec![pir::PirValue::I32(6)]).expect("execute");
    assert_eq!(result, pir::PirValue::I32(7));
}

#[test]
fn prefers_top_level_portable_declarations_over_methods() {
    let source = r#"
class Shadow {
    private {
        fn portable_entry() -> I32 {
            return i32(100)
        }

        fn helper() -> I32 {
            return i32(200)
        }
    }
}

kernel fn helper() -> I32 {
    return i32(1)
}

kernel fn portable_entry() -> I32 {
    return helper()
}
"#;

    let module = lower_inline_module_from_source(source);
    let semantic = hir::semantic::check_module(&module);
    assert!(
        semantic.errors.is_empty(),
        "semantic errors: {:?}",
        semantic.errors
    );
    let (type_errors, type_info) = hir::typeck::check_module_with_info(&module);
    assert!(type_errors.is_empty(), "type errors: {type_errors:?}");

    let pir = pir::lower_portable_entry_by_name(&module, &type_info, "portable_entry")
        .unwrap_or_else(|errors| panic!("pir lowering failed: {errors:?}"));
    let result = pir::execute_entry(&pir, Vec::new()).expect("execute");
    assert_eq!(result, pir::PirValue::I32(1));
}

#[test]
fn lowers_phase5_primitive_catalog_as_graph_backed_pir_bodies() {
    let source = r#"
field conservative distance phase5_rounded_box_field(p: Vec3) -> F32 {
    rounded_box(half=vec3(0.60, 0.48, 0.36), radius=0.10)
}

field conservative distance phase5_ellipsoid_field(p: Vec3) -> F32 {
    ellipsoid(radii=vec3(0.58, 0.38, 0.46))
}

field conservative distance phase5_cone_field(p: Vec3) -> F32 {
    cone(radius=0.48, half_height=0.88)
}

field conservative distance phase5_capped_cone_field(p: Vec3) -> F32 {
    capped_cone(radius_bottom=0.54, radius_top=0.16, half_height=0.34)
}

field conservative distance phase5_box_frame_field(p: Vec3) -> F32 {
    box_frame(half=vec3(0.54, 0.54, 0.54), thickness=0.10)
}

field conservative distance phase5_slab_field(p: Vec3) -> F32 {
    slab(thickness=0.18)
}

field conservative distance phase5_triangle_prism_field(p: Vec3) -> F32 {
    triangle_prism(half=vec2(0.32, 0.28), half_height=0.36)
}

field conservative distance phase5_hex_prism_field(p: Vec3) -> F32 {
    hex_prism(half=vec2(0.32, 0.46), half_height=0.30)
}
"#;
    let module = lower_inline_module_from_source(source);
    for (field_name, expected) in [
        ("phase5_rounded_box_field", hir::FieldPrimitive::RoundedBox),
        ("phase5_ellipsoid_field", hir::FieldPrimitive::Ellipsoid),
        ("phase5_cone_field", hir::FieldPrimitive::Cone),
        ("phase5_capped_cone_field", hir::FieldPrimitive::CappedCone),
        ("phase5_box_frame_field", hir::FieldPrimitive::BoxFrame),
        ("phase5_slab_field", hir::FieldPrimitive::Slab),
        (
            "phase5_triangle_prism_field",
            hir::FieldPrimitive::TrianglePrism,
        ),
        ("phase5_hex_prism_field", hir::FieldPrimitive::HexPrism),
    ] {
        assert_semantic_field_graph_backed_pir_body_empty(&module, field_name, |root| {
            assert!(matches!(
                root,
                hir::FieldExpr::Primitive { primitive, .. } if *primitive == expected
            ));
        });
    }
}

#[test]
fn executes_phase7_radiance_volume_and_material_context_on_cpu() {
    let source = r#"
value Phase7Probe {
    radiance: Vec3
    medium: Medium
    surface: Surface
}

radiance field phase7_radiance(p: Vec3, direction: Vec3, feature_id: U32) -> Vec3 {
    sky_t = clamp(0.5 + direction.y * 0.5, 0.0, 1.0)
    mutable feature_bias = 0.0
    if feature_id != u32(0) {
        feature_bias = 1.0
    }
    return vec3(0.10, 0.18, 0.28) * (1.0 - sky_t)
        + vec3(0.36, 0.54, 0.82) * sky_t
        + vec3(0.08, 0.04, 0.02) * clamp(0.2 - abs(p.z), 0.0, 0.2) * 5.0 * feature_bias
}

volume field phase7_volume(p: Vec3, surface_distance: F32) -> Medium {
    density = clamp(0.08 + clamp(0.2 - abs(surface_distance), 0.0, 0.2) * 0.4, 0.0, 0.16)
    return Medium(
        density=density,
        emission=vec3(0.06, 0.08, 0.10) * density + vec3(abs(p.x) * 0.0, 0.0, 0.0),
        anisotropy=0.12
    )
}

material phase7_surface(hit: Hit3) -> Surface {
    ridge = clamp(abs(hit.local_position.y) * 0.8 + abs(hit.local_normal.x) * 0.2, 0.0, 1.0)
    return Surface(
        albedo=vec3(0.26, 0.34, 0.44) + vec3(0.08, 0.06, 0.04) * ridge,
        roughness=0.16 + ridge * 0.18,
        metalness=0.12 + clamp(hit.local_normal.z, 0.0, 1.0) * 0.18,
        clearcoat=0.14 + clamp(hit.local_normal.y, 0.0, 1.0) * 0.16,
        clearcoat_roughness=0.08 + abs(hit.local_position.x) * 0.12,
        sheen=0.06 + abs(hit.local_normal.x) * 0.10,
        emissive=vec3(0.04, 0.02, 0.01) * clamp(hit.local_normal.z, 0.0, 1.0)
    )
}

kernel fn portable_entry() -> Phase7Probe {
    payload = Payload(
        entity_id=u32(901),
        material_id=u32(901),
        actor=ActorHandle(id=u32(901), generation=u32(0))
    )
    hit = Hit3(
        hit=true,
        distance=1.0,
        position=vec3(0.0, 0.0, 0.45),
        normal=vec3(0.0, 0.0, 1.0),
        local_position=vec3(0.0, 0.0, 0.45),
        local_normal=vec3(0.0, 0.0, 1.0),
        shading_frame=transform3_identity(),
        steps=i32(4),
        feature_id=u32(901),
        instance_id=u32(0),
        repeat_id=u32(0),
        root_shape_id=u32(901),
        payload=payload
    )
    return Phase7Probe(
        radiance=phase7_radiance(
            p=hit.position,
            direction=normalize(vec3(0.0, 1.0, 1.0)),
            feature_id=hit.feature_id
        ),
        medium=phase7_volume(
            p=hit.position,
            surface_distance=0.1
        ),
        surface=phase7_surface(hit=hit)
    )
}
"#;

    let module = lower_pir_inline(source, "portable_entry");
    let result = pir::execute_entry(&module, Vec::new()).expect("execute");
    let probe = match result {
        pir::PirValue::Struct(value) => value,
        other => panic!("expected struct result, got {other:?}"),
    };
    let radiance = match probe.field("radiance").expect("radiance") {
        pir::PirValue::Vec3(value) => *value,
        other => panic!("expected radiance vec3, got {other:?}"),
    };
    let medium = match probe.field("medium").expect("medium") {
        pir::PirValue::Struct(value) => value,
        other => panic!("expected medium struct, got {other:?}"),
    };
    let surface = match probe.field("surface").expect("surface") {
        pir::PirValue::Struct(value) => value,
        other => panic!("expected surface struct, got {other:?}"),
    };

    assert!(
        radiance[2] > radiance[0],
        "expected cool radiance bias: {radiance:?}"
    );
    assert_eq!(medium.field("anisotropy"), Some(&pir::PirValue::F32(0.12)));
    assert!(matches!(medium.field("density"), Some(pir::PirValue::F32(v)) if *v > 0.08));
    assert!(matches!(surface.field("metalness"), Some(pir::PirValue::F32(v)) if *v > 0.12));
    assert!(matches!(surface.field("clearcoat"), Some(pir::PirValue::F32(v)) if *v >= 0.14));
    assert!(matches!(surface.field("sheen"), Some(pir::PirValue::F32(v)) if *v >= 0.06));
    assert!(matches!(surface.field("emissive"), Some(pir::PirValue::Vec3(v)) if v[0] > 0.0));
}
