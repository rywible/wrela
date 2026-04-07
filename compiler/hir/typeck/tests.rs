#[cfg(test)]
mod tests {
    use super::*;
    use crate::hir::lower::lower;
    use crate::parser::ast::AstNode;
    use crate::parser::{ast, parse};
    use miette::SourceSpan;

    fn check_source(input: &str) -> Vec<TypeError> {
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        check_module(&module)
    }

    #[test]
    fn test_type_error_binary() {
        let input = r#"fn f() -> Integer {
    return 1 + true
}"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::InvalidBinaryOperands { .. }))
        );
    }

    #[test]
    fn test_type_error_unary() {
        let input = r#"fn f() -> Boolean {
    return not 1
}"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::InvalidUnaryOperand { .. }))
        );
    }

    #[test]
    fn test_param_type_used() {
        let input = r#"fn f(x: Integer) -> Integer {
    return x + 1
}"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_param_type_mismatch() {
        let input = r#"fn f(x: Integer) -> Integer {
    return x + true
}"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::InvalidBinaryOperands { .. }))
        );
    }

    #[test]
    fn test_value_class_methods_are_forbidden() {
        let input = r#"value Pair {
    left: I32

    fn sum() -> I32 {
        return self.left
    }
}
"#;
        let errors = check_source(input);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::ValueMethodsForbidden { .. })),
            "expected ValueMethodsForbidden, got: {errors:?}"
        );
    }

    #[test]
    fn test_value_class_interfaces_are_forbidden() {
        let input = r#"interface Showable {
    must show() -> String
}

value Pair {
    is a Showable
    left: I32
}
"#;
        let errors = check_source(input);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::ValueInterfacesForbidden { .. })),
            "expected ValueInterfacesForbidden, got: {errors:?}"
        );
    }

    #[test]
    fn test_value_class_mutable_fields_are_forbidden() {
        let input = r#"value Pair {
    mutable left: I32
}
"#;
        let errors = check_source(input);
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::ValueFieldMutableForbidden { field, .. } if field.as_str() == "left"
            )),
            "expected ValueFieldMutableForbidden, got: {errors:?}"
        );
    }

    #[test]
    fn test_value_class_field_types_must_be_fixed_layout() {
        let input = r#"value Pair {
    left: Integer
    right: List[I32]
}
"#;
        let errors = check_source(input);
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::ValueFieldTypeForbidden { field, found, .. }
                    if field.as_str() == "left" && found == "Integer"
            )),
            "expected Integer field rejection, got: {errors:?}"
        );
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::ValueFieldTypeForbidden { field, found, .. }
                    if field.as_str() == "right" && found == "List[I32]"
            )),
            "expected List field rejection, got: {errors:?}"
        );
    }

    #[test]
    fn test_value_class_with_fixed_layout_fields_is_allowed() {
        let input = r#"value Sample {
    flag: Bool
    count: I32
    coords: Array[I32, 3]
}
"#;
        let errors = check_source(input);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_vec3_constructor_field_access_and_approx_typecheck() {
        let input = r#"fn f() -> Nothing {
    value = vec3(1.0, 2.0, 3.0)
    assert approx value.x ~= 1.0 within 0.001
}
"#;
        let errors = check_source(input);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_vec2_surface_typecheck() {
        let input = r#"fn f() -> Nothing {
    base = vec2(3.0, 4.0)
    unit = normalize(base)
    shifted = base + vec2(1.0, -1.0)
    restored = (shifted * 0.5) / 0.5
    assert approx base.x ~= 3.0 within 0.001
    assert approx base.y ~= 4.0 within 0.001
    assert approx length(base) ~= 5.0 within 0.001
    assert approx dot(unit, vec2(0.6, 0.8)) ~= 1.0 within 0.001
    assert approx restored.x ~= 4.0 within 0.001
}
"#;
        let errors = check_source(input);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_vec_math_intrinsics_typecheck() {
        let input = r#"fn f() -> Nothing {
    projection = dot(vec3(1.0, 0.0, 0.0), normalize(vec3(0.0, 2.0, 0.0)))
    size = length(vec3(3.0, 0.0, 4.0))
    axis = cross(vec3(1.0, 0.0, 0.0), vec3(0.0, 1.0, 0.0))
    assert approx projection ~= 0.0 within 1.0
    assert approx size ~= 5.0 within 0.001
    assert approx axis.z ~= 1.0 within 0.001
}
"#;
        let errors = check_source(input);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_workgroup_barrier_reports_unsupported_compute_feature() {
        let input = r#"kernel fn run_kernel() -> Nothing {
    workgroup_barrier()
}
"#;
        let errors = check_source(input);
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::UnsupportedComputeFeature { feature, .. }
                    if *feature == "workgroup_barrier"
            )),
            "expected UnsupportedComputeFeature, got: {errors:?}"
        );
    }

    #[test]
    fn test_workgroup_dispatch_schedules_typecheck() {
        let input = r#"kernel fn run_kernel() -> Nothing {
    noop = 0
}

fn run() -> Nothing {
    dispatch_compute(
        kernel=run_kernel,
        schedule=gpu_schedule_workgroup_reverse(),
        workgroups_x=u32(2),
        workgroups_y=u32(1),
        workgroups_z=u32(1),
        workgroup_size_x=u32(2),
        workgroup_size_y=u32(1),
        workgroup_size_z=u32(1)
    )
    shuffle = gpu_schedule_workgroup_shuffle(seed=u32(7))
    round_robin = gpu_schedule_round_robin_workgroups()
}
"#;
        let errors = check_source(input);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_dispatch_kernel_rejects_host_only_helper_calls() {
        let input = r#"fn host_count() -> Integer {
    return __wr_runtime_cpu_count()
}

kernel fn run_kernel(data: GpuBuffer[I32]) -> Nothing {
    observed = host_count()
    gpu_buffer_set(buffer=data, index=i32(0), value=i32(7))
}

fn run() -> Nothing {
    data = gpu_buffer_new(length=1, default_value=i32(0))
    dispatch_compute(
        kernel=run_kernel,
        data=data,
        schedule=gpu_schedule_deterministic(),
        workgroups_x=u32(1),
        workgroups_y=u32(1),
        workgroups_z=u32(1),
        workgroup_size_x=u32(1),
        workgroup_size_y=u32(1),
        workgroup_size_z=u32(1)
    )
}
"#;
        let errors = check_source(input);
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::PortableHostCallForbidden { callee, .. }
                    if callee.as_str() == "host_count"
            )),
            "expected PortableHostCallForbidden, got: {errors:?}"
        );
    }

    #[test]
    fn test_dispatch_kernel_rejects_host_only_boundary_types() {
        let input = r#"kernel fn run_kernel(label: String) -> Nothing {
    noop = 0
}

fn run() -> Nothing {
    dispatch_compute(
        kernel=run_kernel,
        label="bad",
        schedule=gpu_schedule_deterministic(),
        workgroups_x=u32(1),
        workgroups_y=u32(1),
        workgroups_z=u32(1),
        workgroup_size_x=u32(1),
        workgroup_size_y=u32(1),
        workgroup_size_z=u32(1)
    )
}
"#;
        let errors = check_source(input);
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::PortableBoundaryTypeForbidden { function, found, .. }
                    if function.as_str() == "run_kernel" && found == "String"
            )),
            "expected PortableBoundaryTypeForbidden, got: {errors:?}"
        );
    }

    #[test]
    fn test_host_may_call_portable_helper_shared_with_kernel() {
        let input = r#"kernel fn add_one(value: I32) -> I32 {
    return value + i32(1)
}

kernel fn run_kernel(data: GpuBuffer[I32]) -> Nothing {
    next = add_one(value=i32(4))
    gpu_buffer_set(buffer=data, index=i32(0), value=next)
}

fn run() -> Nothing {
    sample = add_one(value=i32(2))
    data = gpu_buffer_new(length=1, default_value=i32(0))
    dispatch_compute(
        kernel=run_kernel,
        data=data,
        schedule=gpu_schedule_deterministic(),
        workgroups_x=u32(1),
        workgroups_y=u32(1),
        workgroups_z=u32(1),
        workgroup_size_x=u32(1),
        workgroup_size_y=u32(1),
        workgroup_size_z=u32(1)
    )
}
"#;
        let errors = check_source(input);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_portable_data_primitives_and_value_fields_typecheck() {
        let input = r#"value SpatialProbe {
    bounds: Bounds3
    ray: Ray3
    transform: Transform3
}

fn f() -> Nothing {
    box = bounds3(
        min=vec3(0.0, 1.0, 2.0),
        max=vec3(2.0, 3.0, 4.0)
    )
    center = bounds3_center(bounds=box)
    size = bounds3_size(bounds=box)
    ray = ray3(origin=center, direction=normalize(vec3(0.0, 2.0, 0.0)))
    pose = transform3_identity()
    moved = transform_point(transform=pose, point=ray.origin)
    normal = transform_normal(transform=pose, normal=ray.direction)
    assert approx box.min.x ~= 0.0 within 0.001
    assert approx box.max.z ~= 4.0 within 0.001
    assert approx center.y ~= 2.0 within 0.001
    assert approx size.x ~= 2.0 within 0.001
    assert approx moved.z ~= 3.0 within 0.001
    assert approx normal.y ~= 1.0 within 0.001
    verts = [vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(1.0, 1.0)]
    profile = polygon2(p=vec2(0.5, 0.5), vertices=verts)
    sweep = field_sweep_coords(path=vec3(0.0, 0.0, 1.0), point=vec3(1.0, 2.0, 3.0))
    _ = profile
    assert approx sweep.x ~= 1.0 within 0.001
}
"#;
        let errors = check_source(input);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_field_query_builtins_typecheck_on_host() {
        let input = r#"field conservative distance sphere(p: Vec3) -> F32 {
    return length(p) - 1.0
}

fn f() -> Nothing {
    scene = capture sphere
    distance = distance_at(capture=scene, point=vec3(1.0, 2.0, 3.0))
    normal = normal_at(capture=scene, point=vec3(1.0, 2.0, 3.0))
    assert approx distance ~= 0.0 within 0.001
    assert approx normal.x ~= 0.0 within 0.001
}
"#;
        let errors = check_source(input);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_scene_capture_and_capture_queries_typecheck_on_host() {
        let input = r#"field exact distance sphere_field(p: Vec3) -> F32 {
    sphere(radius=1.0)
}

shape scene_shape {
    field = sphere_field
    material = shade
    payload = Payload(
        entity_id=u64(1),
        material_id=u64(2),
        actor=ActorHandle(id=u64(3), generation=u32(0))
    )
}

material shade(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(1.0, 0.0, 0.0),
        roughness=0.5,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

fn f() -> Nothing {
    scene = capture scene_shape
    distance = distance_at(capture=scene, point=vec3(1.0, 2.0, 3.0))
    normal = normal_at(capture=scene, point=vec3(1.0, 2.0, 3.0))
    hit = trace_shape(
        capture=scene,
        origin=vec3(0.0, 0.0, 3.0),
        direction=vec3(0.0, 0.0, -1.0),
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
    surface = surface_at(capture=scene, hit=hit)
    assert approx distance ~= 0.0 within 0.001
    assert approx normal.x ~= 0.0 within 0.001
    assert approx surface.albedo.x ~= 1.0 within 0.001
}
"#;
        let errors = check_source(input);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_field_query_builtins_are_rejected_in_portable_lane() {
        let input = r#"field conservative distance sphere(p: Vec3) -> F32 {
    return length(p) - 1.0
}

kernel fn run_kernel() -> Nothing {
    scene = capture sphere
    distance = distance_at(capture=scene, point=vec3(1.0, 2.0, 3.0))
    normal = normal_at(capture=scene, point=vec3(1.0, 2.0, 3.0))
}
"#;
        let errors = check_source(input);
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::PortableHostCallForbidden { callee, .. }
                    if callee.as_str() == "distance_at"
            )),
            "expected PortableHostCallForbidden(distance_at), got: {errors:?}"
        );
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::PortableHostCallForbidden { callee, .. }
                    if callee.as_str() == "normal_at"
            )),
            "expected PortableHostCallForbidden(normal_at), got: {errors:?}"
        );
    }

    #[test]
    fn test_capture_requires_top_level_field_or_shape_target() {
        let input = r#"fn helper(p: Vec3) -> F32 {
    return length(p)
}

fn f() -> Nothing {
    scene = capture helper
}
"#;
        let errors = check_source(input);
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::CaptureTargetMustBeFieldOrShape { .. }
            )),
            "expected CaptureTargetMustBeFieldOrShape, got: {errors:?}"
        );
    }

    #[test]
    fn test_capture_understands_region_targets() {
        let input = r#"region Highlands() {
}

fn f() -> Nothing {
    world = capture Highlands
}
"#;
        let errors = check_source(input);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_legacy_capture_generic_is_rejected_in_world_boundaries() {
        let input = r#"domain Legacy(world: Capture) {
}
"#;
        let errors = check_source(input);
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::PortableBoundaryTypeForbidden { found, .. }
                    if found == "Capture"
            )),
            "expected legacy generic Capture boundary to be rejected, got: {errors:?}"
        );
    }

    #[test]
    fn test_parameterized_regions_are_rejected() {
        let input = r#"region Highlands(band: I32) {
}
"#;
        let errors = check_source(input);
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::PortableConstructForbidden { construct, .. }
                    if construct == "a parameterized region declaration"
            )),
            "expected parameterized regions to be rejected, got: {errors:?}"
        );
    }

    #[test]
    fn test_domain_and_render_accept_region_world_queries() {
        let input = r#"region Highlands() {
}

domain Combat(world: RegionCapture) {
}

render View(world: RegionCapture, camera: Camera) {
}

fn run() -> Nothing {
    world = capture Highlands
    domain = Combat(world=world)
    distance = distance_world(capture=world, domain=domain, point=vec3(0.0, 0.0, 0.0))
    normal = normal_world(capture=world, domain=domain, point=vec3(0.0, 0.0, 0.0))
    hit = trace_world(
        capture=world,
        domain=domain,
        origin=vec3(0.0, 0.0, 3.0),
        direction=vec3(0.0, 0.0, -1.0),
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
    surface = surface_world(capture=world, domain=domain, hit=hit)
    radiance = radiance_world(capture=world, domain=domain, point=hit.position, direction=vec3(0.0, 0.0, -1.0))
    medium = medium_world(capture=world, domain=domain, point=hit.position)
}
"#;
        let errors = check_source(input);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_region_scatter_and_conditional_items_are_rejected() {
        let input = r#"region Highlands() {
    scatter trees {
        place sapling = Oak()
    }
    if true {
        place fallback = Stone()
    }
}
"#;
        let errors = check_source(input);
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::PortableConstructForbidden { construct, .. }
                    if construct == "a scatter region item"
                        || construct == "a conditional region item"
            )),
            "expected unsupported region items to be rejected, got: {errors:?}"
        );
    }

    #[test]
    fn test_render_lights_metadata_is_rejected() {
        let input = r#"region Highlands() {
}

render View(world: RegionCapture, camera: Camera) {
    lights = Light(
        position=vec3(0.0, 1.0, 2.0),
        direction=vec3(0.0, -1.0, 0.0),
        intensity=vec3(1.0, 1.0, 1.0),
        range=10.0
    )
}
"#;
        let errors = check_source(input);
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::PortableConstructForbidden { construct, .. }
                    if construct == "render lights metadata"
            )),
            "expected render lights metadata to be rejected, got: {errors:?}"
        );
    }

    #[test]
    fn test_domain_and_render_reject_executable_statements() {
        let input = r#"region Highlands() {
}

domain Combat(world: RegionCapture) {
    return 1
}

render View(world: RegionCapture, camera: Camera) {
    while true {
    }
}
"#;
        let errors = check_source(input);
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::PortableConstructForbidden { construct, .. }
                    if construct.contains("domain declaration executable statement")
                        || construct.contains("render declaration executable statement")
            )),
            "expected executable world declarations to be rejected, got: {errors:?}"
        );
    }

    #[test]
    fn test_host_field_queries_reject_region_captures() {
        let input = r#"region Highlands() {
}

fn f() -> Nothing {
    world = capture Highlands
    distance = distance_at(capture=world, point=vec3(1.0, 2.0, 3.0))
}
"#;
        let errors = check_source(input);
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::ArgumentTypeMismatch { name, expected, .. }
                    if name.as_str() == "capture" && expected == "FieldCapture or ShapeCapture"
            )),
            "expected region captures to be rejected by host field queries, got: {errors:?}"
        );
    }

    #[test]
    fn test_shape_queries_typecheck_on_host() {
        let input = r#"field exact distance sphere_field(p: Vec3) -> F32 {
    sphere(radius=1.0)
}

material shade(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(1.0, 0.0, 0.0),
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
        entity_id=u64(1),
        material_id=u64(2),
        actor=ActorHandle(id=u64(3), generation=u32(0))
    )
}

fn run() -> Nothing {
    scene = capture scene_shape
    hit = trace_shape(
        capture=scene,
        origin=vec3(0.0, 0.0, 3.0),
        direction=vec3(0.0, 0.0, -1.0),
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
    surface = surface_at(capture=scene, hit=hit)
    assert approx surface.albedo.x ~= 1.0 within 0.001
}
"#;
        let errors = check_source(input);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_shape_queries_reject_field_captures() {
        let input = r#"field exact distance sphere_field(p: Vec3) -> F32 {
    sphere(radius=1.0)
}

fn run() -> Nothing {
    hit = trace_shape(
        capture=capture sphere_field,
        origin=vec3(0.0, 0.0, 3.0),
        direction=vec3(0.0, 0.0, -1.0),
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
}
"#;
        let errors = check_source(input);
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::ShapeQueryTargetMustBeShape { query, .. }
                    if query.as_str() == "trace_shape"
            )),
            "expected ShapeQueryTargetMustBeShape(trace_shape), got: {errors:?}"
        );
    }

    #[test]
    fn test_shape_queries_reject_stored_field_capture_variables() {
        let input = r#"field exact distance sphere_field(p: Vec3) -> F32 {
    sphere(radius=1.0)
}

fn run() -> Nothing {
    scene = capture sphere_field
    hit = trace_shape(
        capture=scene,
        origin=vec3(0.0, 0.0, 3.0),
        direction=vec3(0.0, 0.0, -1.0),
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
}
"#;
        let errors = check_source(input);
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::ShapeQueryTargetMustBeShape { query, .. }
                    if query.as_str() == "trace_shape"
            )),
            "expected stored field capture rejection, got: {errors:?}"
        );
    }

    #[test]
    fn test_batch_scene_queries_typecheck_on_host() {
        let input = r#"field exact distance sphere_field(p: Vec3) -> F32 {
    sphere(radius=1.0)
}

material shade(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(1.0, 0.0, 0.0),
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
        entity_id=u64(1),
        material_id=u64(2),
        actor=ActorHandle(id=u64(3), generation=u32(0))
    )
}

fn run() -> Nothing {
    scene = capture scene_shape
    rays = [
        RayQuery(
            origin=vec3(0.0, 0.0, 3.0),
            direction=vec3(0.0, 0.0, -1.0),
            max_distance=6.0,
            min_step=0.05,
            hit_epsilon=0.001,
            max_steps=96
        )
    ]
    points = [PointQuery(point=vec3(0.0, 0.0, 2.0))]
    hits = trace_shape_batch(
        capture=scene,
        rays=rays,
        backend=dispatch_backend_cpu()
    )
    surfaces = surface_at_batch(
        capture=scene,
        hits=hits,
        backend=dispatch_backend_auto()
    )
    distances = distance_at_batch(
        capture=scene,
        points=points,
        backend=dispatch_backend_cpu()
    )
    normals = normal_at_batch(
        capture=scene,
        points=points,
        backend=dispatch_backend_virtual_gpu()
    )
    occlusion = occluded_batch(
        capture=scene,
        rays=rays,
        backend=dispatch_backend_virtual_gpu()
    )
    assert value hits[0].hit == true
    assert approx surfaces[0].albedo.x ~= 1.0 within 0.001
    assert approx distances[0].distance ~= 1.0 within 0.001
    assert approx normals[0].normal.z ~= 1.0 within 0.001
    assert value occlusion[0].occluded == true
}
"#;
        let errors = check_source(input);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_batch_scene_queries_reject_raw_integer_backends() {
        let input = r#"field exact distance sphere_field(p: Vec3) -> F32 {
    sphere(radius=1.0)
}

shape scene_shape {
    field = sphere_field
    material = shade
    payload = Payload()
}

material shade(hit: Hit3) -> Surface {
    return Surface()
}

fn run() -> Nothing {
    scene = capture scene_shape
    points = [PointQuery(point=vec3(0.0, 0.0, 2.0))]
    _ = distance_at_batch(capture=scene, points=points, backend=99)
}
"#;
        let errors = check_source(input);
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::ArgumentTypeMismatch { name, expected, found, .. }
                    if name.as_str() == "backend"
                        && expected == "DispatchBackend"
                        && found == "Integer"
            )),
            "expected backend type mismatch, got: {errors:?}"
        );
    }

    #[test]
    fn test_capture_boundary_types_are_opaque() {
        let input = r#"fn run() -> Nothing {
    _ = FieldCapture(scene_id=u64(1), epoch=u64(0), root_feature_id=u64(0))
    _ = DispatchBackend(id=i64(0))
}
"#;
        let errors = check_source(input);
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::OpaqueBuiltinConstructionForbidden { name, .. }
                    if name.as_str() == "FieldCapture"
            )),
            "expected opaque FieldCapture constructor rejection, got: {errors:?}"
        );
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::OpaqueBuiltinConstructionForbidden { name, .. }
                    if name.as_str() == "DispatchBackend"
            )),
            "expected opaque DispatchBackend constructor rejection, got: {errors:?}"
        );
    }

    #[test]
    fn test_shape_leaf_requires_field_and_material_targets() {
        let input = r#"fn helper(p: Vec3) -> F32 {
    return length(p)
}

fn bad_material(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(1.0, 0.0, 0.0),
        roughness=0.5,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape broken_shape {
    field = helper
    material = bad_material
    payload = Payload(
        entity_id=u64(1),
        material_id=u64(2),
        actor=ActorHandle(id=u64(3), generation=u32(0))
    )
}
"#;
        let errors = check_source(input);
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::ShapeBindingTargetInvalid { shape, binding, expected, target, .. }
                    if shape.as_str() == "broken_shape"
                        && *binding == "`field = ...`"
                        && *expected == "field"
                        && target.as_str() == "helper"
            )),
            "expected field binding rejection, got: {errors:?}"
        );
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::ShapeBindingTargetInvalid { shape, binding, expected, target, .. }
                    if shape.as_str() == "broken_shape"
                        && *binding == "`material = ...`"
                        && *expected == "material"
                        && target.as_str() == "bad_material"
            )),
            "expected material binding rejection, got: {errors:?}"
        );
    }

    #[test]
    fn test_shape_payload_requires_payload_type() {
        let input = r#"field exact distance sphere_field(p: Vec3) -> F32 {
    sphere(radius=1.0)
}

material shade(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(1.0, 0.0, 0.0),
        roughness=0.5,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape broken_shape {
    field = sphere_field
    material = shade
    payload = u64(7)
}
"#;
        let errors = check_source(input);
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::ShapePayloadTypeForbidden { shape, found, .. }
                    if shape.as_str() == "broken_shape" && found == "U64"
            )),
            "expected payload type rejection, got: {errors:?}"
        );
    }

    #[test]
    fn test_shape_use_cycles_are_rejected() {
        let input = r#"field exact distance sphere_field(p: Vec3) -> F32 {
    sphere(radius=1.0)
}

material shade(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(1.0, 0.0, 0.0),
        roughness=0.5,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape leaf_shape {
    field = sphere_field
    material = shade
    payload = Payload(
        entity_id=u64(1),
        material_id=u64(2),
        actor=ActorHandle(id=u64(3), generation=u32(0))
    )
}

shape first_shape {
    union {
        use leaf_shape
        use second_shape
    }
}

shape second_shape {
    union {
        use leaf_shape
        use first_shape
    }
}
"#;
        let errors = check_source(input);
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::ShapeCycleDetected { shape, target, .. }
                    if shape.as_str() == "first_shape" && target.as_str() == "first_shape"
            )),
            "expected recursive shape cycle rejection, got: {errors:?}"
        );
    }

    #[test]
    fn test_field_declarations_require_single_vec3_p_and_f32_return() {
        let input = r#"field exact distance sphere(center: F32) -> Integer {
    return 0
}
"#;
        let errors = check_source(input);
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::PortableConstructForbidden { function, construct, .. }
                    if function.as_str() == "sphere"
                        && construct.contains("field parameter")
            )),
            "expected field parameter rejection, got: {errors:?}"
        );
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::PortableBoundaryTypeForbidden { function, site, .. }
                    if function.as_str() == "sphere"
                        && site.as_str() == "return type"
            )),
            "expected field return type rejection, got: {errors:?}"
        );
    }

    #[test]
    fn test_exact_field_rejects_conservative_field_calls() {
        let input = r#"field conservative distance shell(p: Vec3) -> F32 {
    return length(p)
}

field exact distance sphere(p: Vec3) -> F32 {
    use shell
}
"#;
        let errors = check_source(input);
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::FieldExactnessCapabilityViolation { function, node, detail, .. }
                    if function.as_str() == "sphere"
                        && node.as_str() == "use"
                        && detail.contains("conservative field 'shell'")
            )),
            "expected exact field exactness rejection, got: {errors:?}"
        );
    }

    #[test]
    fn test_exact_field_allows_semantic_field_composition() {
        let input = r#"field exact distance orb(p: Vec3) -> F32 {
    sphere(radius=1.0)
}

field exact distance frame(p: Vec3) -> F32 {
    box(half=vec3(1.1, 1.1, 1.1))
}

field exact distance cap(p: Vec3) -> F32 {
    plane(normal=vec3(0.0, 0.0, 1.0), offset=0.0)
}

field exact distance notch(p: Vec3) -> F32 {
    torus(major_radius=1.5, minor_radius=0.25)
}

field exact distance sculpted(p: Vec3) -> F32 {
    subtract {
        intersection {
            union {
                use orb
                use frame
            }
            use cap
        }
        use notch
    }
}
"#;
        let errors = check_source(input);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_exact_field_allows_exact_warp_families_but_rejects_conservative_operators() {
        let input = r#"field exact distance source(p: Vec3) -> F32 {
    sphere(radius=1.0)
}

field exact distance mirrored(p: Vec3) -> F32 {
    mirror_array = vec3(0.0, 1.0, 0.0) {
        use source
    }
}

field exact distance rotated(p: Vec3) -> F32 {
    rotate = vec3(0.0, 1.0, 0.0) {
        use source
    }
}

field exact distance repeated(p: Vec3) -> F32 {
    repeat_linear = vec3(2.0, 0.0, 0.0) {
        use source
    }
}

field exact distance gridded(p: Vec3) -> F32 {
    repeat_grid = vec3(2.0, 2.0, 2.0) {
        use source
    }
}

field exact distance shifted(p: Vec3) -> F32 {
    translate = vec3(1.0, 0.0, 0.0) {
        use source
    }
}

field exact distance instanced(p: Vec3) -> F32 {
    instance_array = Transform3(
        matrix=mat4_identity(),
        inverse=mat4_identity()
    ) {
        use source
    }
}
"#;
        let errors = check_source(input);
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::FieldExactnessCapabilityViolation { function, node, detail, .. }
                    if function.as_str() == "instanced"
                        && node.as_str() == "instance_array"
                        && detail.contains("instance arrays are conservative-only")
            )),
            "expected exact field instance rejection, got: {errors:?}"
        );
        assert!(
            !errors.iter().any(|err| matches!(
                err,
                TypeError::FieldExactnessCapabilityViolation { function, node, .. }
                    if function.as_str() == "shifted"
                        && node.as_str() == "translate"
            )),
            "translation transform should stay exact-preserving, got: {errors:?}"
        );
        assert!(
            !errors.iter().any(|err| matches!(
                err,
                TypeError::FieldExactnessCapabilityViolation { function, node, .. }
                    if function.as_str() == "mirrored"
                        && node.as_str() == "mirror_array"
            )),
            "mirror should stay exact-preserving, got: {errors:?}"
        );
        assert!(
            !errors.iter().any(|err| matches!(
                err,
                TypeError::FieldExactnessCapabilityViolation { function, node, .. }
                    if function.as_str() == "repeated"
                        && node.as_str() == "repeat_linear"
            )),
            "repeat should stay exact-preserving, got: {errors:?}"
        );
        assert!(
            !errors.iter().any(|err| matches!(
                err,
                TypeError::FieldExactnessCapabilityViolation { function, node, .. }
                    if function.as_str() == "rotated"
                        && node.as_str() == "rotate"
            )),
            "rotate should stay exact-preserving, got: {errors:?}"
        );
        assert!(
            !errors.iter().any(|err| matches!(
                err,
                TypeError::FieldExactnessCapabilityViolation { function, node, .. }
                    if function.as_str() == "gridded"
                        && node.as_str() == "repeat_grid"
            )),
            "repeat_grid should stay exact-preserving, got: {errors:?}"
        );
    }

    #[test]
    fn test_exact_field_uniform_scale_requires_positive_scalar() {
        let input = r#"field exact distance source(p: Vec3) -> F32 {
    sphere(radius=1.0)
}

field exact distance scaled(p: Vec3) -> F32 {
    uniform_scale = f32(0.0) {
        use source
    }
}

field exact distance scaled_unknown(p: Vec3) -> F32 {
    uniform_scale = length(value=vec3(1.0, 0.0, 0.0)) {
        use source
    }
}
"#;
        let errors = check_source(input);
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::FieldExactnessCapabilityViolation { function, node, detail, .. }
                    if function.as_str() == "scaled"
                        && node.as_str() == "uniform_scale"
                        && detail.contains("positive")
            )),
            "expected uniform scale positivity rejection, got: {errors:?}"
        );
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::FieldExactnessCapabilityViolation { function, node, detail, .. }
                    if function.as_str() == "scaled_unknown"
                        && node.as_str() == "uniform_scale"
                        && detail.contains("prove")
            )),
            "expected uniform scale proof rejection, got: {errors:?}"
        );
    }

    #[test]
    fn test_exact_field_rejects_point_dependent_wrapper_operands() {
        let input = r#"field exact distance source(p: Vec3) -> F32 {
    sphere(radius=1.0)
}

field exact distance warped_translate(p: Vec3) -> F32 {
    translate = p {
        use source
    }
}

field exact distance warped_mirror(p: Vec3) -> F32 {
    mirror_array = p {
        use source
    }
}

field exact distance warped_repeat(p: Vec3) -> F32 {
    repeat_grid = p {
        use source
    }
}

field exact distance warped_scale(p: Vec3) -> F32 {
    uniform_scale = length(value=p) {
        use source
    }
}
"#;
        let errors = check_source(input);
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::FieldExactnessCapabilityViolation { function, node, detail, .. }
                    if function.as_str() == "warped_translate"
                        && node.as_str() == "translate"
                        && detail.contains("references sample point")
            )),
            "expected translate sample-point rejection, got: {errors:?}"
        );
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::FieldExactnessCapabilityViolation { function, node, detail, .. }
                    if function.as_str() == "warped_mirror"
                        && node.as_str() == "mirror_array"
                        && detail.contains("references sample point")
            )),
            "expected mirror_array sample-point rejection, got: {errors:?}"
        );
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::FieldExactnessCapabilityViolation { function, node, detail, .. }
                    if function.as_str() == "warped_repeat"
                        && node.as_str() == "repeat_grid"
                        && detail.contains("references sample point")
            )),
            "expected repeat_grid sample-point rejection, got: {errors:?}"
        );
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::FieldExactnessCapabilityViolation { function, node, detail, .. }
                    if function.as_str() == "warped_scale"
                        && node.as_str() == "uniform_scale"
                        && detail.contains("references the sample point")
            )),
            "expected uniform_scale sample-point rejection, got: {errors:?}"
        );
    }

    #[test]
    fn test_field_translate_wrapper_requires_vec3() {
        let input = r#"field conservative distance bad_transform(p: Vec3) -> F32 {
    translate = i32(7) {
        sphere(radius=1.0)
    }
}
"#;
        let errors = check_source(input);
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::PortableBoundaryTypeForbidden { function, site, found, .. }
                    if function.as_str() == "bad_transform"
                        && site.as_str() == "field `translate` operand"
                        && found.as_str() == "I32"
            )),
            "expected invalid translate wrapper operand rejection, got: {errors:?}"
        );
    }

    #[test]
    fn test_field_instance_array_wrapper_requires_vec3() {
        let input = r#"field conservative distance bad_instance(p: Vec3) -> F32 {
    instance_array = 1.0 {
        sphere(radius=1.0)
    }
}
"#;
        let errors = check_source(input);
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::PortableBoundaryTypeForbidden { function, site, found, .. }
                    if function.as_str() == "bad_instance"
                        && site.as_str() == "field `instance_array` operand"
                        && found.as_str() == "Float"
            )),
            "expected invalid instance_array wrapper operand rejection, got: {errors:?}"
        );
    }

    #[test]
    fn test_exact_field_rejects_conservative_phase_five_operators() {
        let input = r#"field exact distance source(p: Vec3) -> F32 {
    sphere(radius=1.0)
}

field exact distance stretched(p: Vec3) -> F32 {
    ellipsoid(radii=vec3(1.0, 0.5, 0.75))
}

field exact distance warped(p: Vec3) -> F32 {
    affine_transform = vec3(1.0, 0.0, 0.0) {
        use source
    }
}

field exact distance warped_warp(p: Vec3) -> F32 {
    warp = vec3(0.0, 0.0, 1.0) {
        use source
    }
}

field exact distance warped_radial(p: Vec3) -> F32 {
    radial_repeat = vec3(0.0, 1.0, 0.0) {
        use source
    }
}

field exact distance smooth(p: Vec3) -> F32 {
    smooth_union {
        smoothing = f32(0.2)
        use source
        use source
    }
}

field exact distance smooth_i(p: Vec3) -> F32 {
    smooth_intersection {
        smoothing = f32(0.2)
        use source
        use source
    }
}

field exact distance smooth_s(p: Vec3) -> F32 {
    smooth_subtract {
        smoothing = f32(0.2)
        use source
        use source
    }
}

field exact distance deformed(p: Vec3) -> F32 {
    bend = vec3(0.0, 1.0, 0.0) {
        use source
    }
}

field exact distance twisted(p: Vec3) -> F32 {
    twist = vec3(0.0, 1.0, 0.0) {
        use source
    }
}

field exact distance tapered(p: Vec3) -> F32 {
    taper = vec3(0.0, 1.0, 0.0) {
        use source
    }
}

field exact distance displaced(p: Vec3) -> F32 {
    displace = vec3(0.0, 1.0, 0.0) {
        use source
    }
}
"#;
        let errors = check_source(input);
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::PortableConstructForbidden { function, construct, .. }
                    if function.as_str() == "stretched"
                        && construct.as_str() == "calling conservative field builtin 'ellipsoid'"
            )),
            "expected ellipsoid to be conservative-only, got: {errors:?}"
        );
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::FieldExactnessCapabilityViolation { function, node, .. }
                    if function.as_str() == "warped" && node.as_str() == "affine_transform"
            )),
            "expected affine_transform to be conservative-only, got: {errors:?}"
        );
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::FieldExactnessCapabilityViolation { function, node, .. }
                    if function.as_str() == "warped_warp" && node.as_str() == "warp"
            )),
            "expected warp to be conservative-only, got: {errors:?}"
        );
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::FieldExactnessCapabilityViolation { function, node, .. }
                    if function.as_str() == "warped_radial"
                        && node.as_str() == "radial_repeat"
            )),
            "expected radial_repeat to be conservative-only, got: {errors:?}"
        );
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::FieldExactnessCapabilityViolation { function, node, .. }
                    if function.as_str() == "smooth" && node.as_str() == "smooth_union"
            )),
            "expected smooth_union to be conservative-only, got: {errors:?}"
        );
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::FieldExactnessCapabilityViolation { function, node, .. }
                    if function.as_str() == "smooth_i"
                        && node.as_str() == "smooth_intersection"
            )),
            "expected smooth_intersection to be conservative-only, got: {errors:?}"
        );
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::FieldExactnessCapabilityViolation { function, node, .. }
                    if function.as_str() == "smooth_s"
                        && node.as_str() == "smooth_subtract"
            )),
            "expected smooth_subtract to be conservative-only, got: {errors:?}"
        );
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::FieldExactnessCapabilityViolation { function, node, .. }
                    if function.as_str() == "deformed" && node.as_str() == "bend"
            )),
            "expected bend to be conservative-only, got: {errors:?}"
        );
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::FieldExactnessCapabilityViolation { function, node, .. }
                    if function.as_str() == "twisted" && node.as_str() == "twist"
            )),
            "expected twist to be conservative-only, got: {errors:?}"
        );
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::FieldExactnessCapabilityViolation { function, node, .. }
                    if function.as_str() == "tapered" && node.as_str() == "taper"
            )),
            "expected taper to be conservative-only, got: {errors:?}"
        );
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::FieldExactnessCapabilityViolation { function, node, .. }
                    if function.as_str() == "displaced" && node.as_str() == "displace"
            )),
            "expected displace to be conservative-only, got: {errors:?}"
        );
    }

    #[test]
    fn test_exact_field_rejects_custom_field_bodies() {
        let input = r#"field exact distance suspect(p: Vec3) -> F32 {
    return sin(value=p.x)
}
"#;
        let errors = check_source(input);
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::FieldExactnessCapabilityViolation { function, node, detail, .. }
                    if function.as_str() == "suspect"
                        && node.as_str() == "custom"
                        && detail.contains("custom field bodies remain opaque")
            )),
            "expected exact custom field rejection, got: {errors:?}"
        );
    }

    #[test]
    fn test_field_support_clause_type_helper_requires_support3() {
        let err = validate_support_clause_type(
            &SmolStr::new("scene"),
            &Type::Vec3,
            SourceSpan::from((0usize, 1usize)),
        )
        .expect_err("support clause should reject non-Support3 types");
        assert!(matches!(
            err,
            TypeError::FieldClauseTypeForbidden { field, clause, expected, found, .. }
                if field.as_str() == "scene"
                    && clause == "support"
                    && expected == "Support3"
                    && found == "Vec3"
        ));
    }

    #[test]
    fn test_field_bounds_clause_type_helper_requires_bounds3() {
        let err = validate_bounds_clause_type(
            &SmolStr::new("scene"),
            &Type::F32,
            SourceSpan::from((0usize, 1usize)),
        )
        .expect_err("bounds clause should reject non-Bounds3 types");
        assert!(matches!(
            err,
            TypeError::FieldClauseTypeForbidden { field, clause, expected, found, .. }
                if field.as_str() == "scene"
                    && clause == "bounds"
                    && expected == "Bounds3"
                    && found == "F32"
        ));
    }

    #[test]
    fn test_field_support_clause_rejects_non_support3_source_values() {
        let input = r#"field conservative distance bad(p: Vec3) -> F32 {
    support = vec3(1.0, 2.0, 3.0)
    sphere(radius=1.0)
}
"#;
        let errors = check_source(input);
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::FieldClauseTypeForbidden { field, clause, expected, found, .. }
                    if field.as_str() == "bad"
                        && *clause == "support"
                        && *expected == "Support3"
                        && found == "Vec3"
            )),
            "expected support clause type rejection, got: {errors:?}"
        );
    }

    #[test]
    fn test_field_bounds_clause_rejects_non_bounds3_source_values() {
        let input = r#"field conservative distance bad(p: Vec3) -> F32 {
    bounds = 1.0
    sphere(radius=1.0)
}
"#;
        let errors = check_source(input);
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::FieldClauseTypeForbidden { field, clause, expected, found, .. }
                    if field.as_str() == "bad"
                        && *clause == "bounds"
                        && *expected == "Bounds3"
                        && found == "Float"
            )),
            "expected bounds clause type rejection, got: {errors:?}"
        );
    }

    #[test]
    fn test_field_support_metadata_conflict_reports_clause_mismatch() {
        let input = r#"field exact distance ground(p: Vec3) -> F32 {
    support = Support3(bounds=Bounds3(
        min=vec3(-1.0, -1.0, -1.0),
        max=vec3(1.0, 1.0, 1.0)
    ))
    bounds = Bounds3(
        min=vec3(-1.0, -1.0, -1.0),
        max=vec3(1.0, 1.0, 1.0)
    )
    plane(normal=vec3(0.0, 1.0, 0.0), offset=0.0)
}
"#;
        let errors = check_source(input);
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::FieldClauseConflict { field, clause, explicit, inferred, help, .. }
                    if field.as_str() == "ground"
                        && *clause == "support"
                        && explicit == "Bounded"
                        && inferred == "Unbounded"
                        && help.contains("primitive 'plane'")
            )),
            "expected authored support conflict to be reported, got: {errors:?}"
        );
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::FieldClauseConflict { field, clause, explicit, inferred, help, .. }
                    if field.as_str() == "ground"
                        && *clause == "bounds"
                        && explicit == "Bounded"
                        && inferred == "Unbounded"
                        && help.contains("primitive 'plane'")
            )),
            "expected authored bounds conflict to be reported, got: {errors:?}"
        );
    }

    #[test]
    fn test_conservative_field_support_metadata_conflict_reports_clause_mismatch() {
        let input = r#"field conservative distance ground(p: Vec3) -> F32 {
    support = Support3(bounds=Bounds3(
        min=vec3(-1.0, -1.0, -1.0),
        max=vec3(1.0, 1.0, 1.0)
    ))
    bounds = Bounds3(
        min=vec3(-1.0, -1.0, -1.0),
        max=vec3(1.0, 1.0, 1.0)
    )
    plane(normal=vec3(0.0, 1.0, 0.0), offset=0.0)
}
"#;
        let errors = check_source(input);
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::FieldClauseConflict { field, clause, explicit, inferred, help, .. }
                    if field.as_str() == "ground"
                        && *clause == "support"
                        && explicit == "Bounded"
                        && inferred == "Unbounded"
                        && help.contains("primitive 'plane'")
            )),
            "expected conservative support conflict to be reported, got: {errors:?}"
        );
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::FieldClauseConflict { field, clause, explicit, inferred, help, .. }
                    if field.as_str() == "ground"
                        && *clause == "bounds"
                        && explicit == "Bounded"
                        && inferred == "Unbounded"
                        && help.contains("primitive 'plane'")
            )),
            "expected conservative bounds conflict to be reported, got: {errors:?}"
        );
    }

    #[test]
    fn test_portable_functions_reject_field_composition_helpers() {
        let input = r#"kernel fn helper() -> F32 {
    return field_union(left=1.0, right=2.0)
}
"#;
        let errors = check_source(input);
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::PortableConstructForbidden { function, construct, .. }
                    if function.as_str() == "helper"
                        && construct.contains("field composition helper 'field_union'")
            )),
            "expected field-composition helper rejection outside field declarations, got: {errors:?}"
        );
    }

    #[test]
    fn test_material_declarations_reject_field_composition_helpers() {
        let input = r#"material surface(hit: Hit3) -> Surface {
    rough = field_union(left=0.25, right=0.5)
    return Surface(
        albedo=vec3(rough, rough, rough),
        roughness=rough,
        metalness=0.1,
        clearcoat=0.25,
        clearcoat_roughness=0.3,
        sheen=0.15,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}
"#;
        let errors = check_source(input);
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::PortableConstructForbidden { function, construct, .. }
                    if function.as_str() == "surface"
                        && construct.contains("field composition helper 'field_union'")
            )),
            "expected field-composition helper rejection in material, got: {errors:?}"
        );
    }

    #[test]
    fn test_field_declarations_reject_kernel_only_builtins() {
        let input = r#"field conservative distance sphere(p: Vec3) -> F32 {
    gid = global_invocation_id()
    return f32(gid[0])
}
"#;
        let errors = check_source(input);
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::PortableConstructForbidden { function, construct, .. }
                    if function.as_str() == "sphere"
                        && construct.contains("kernel-only builtin 'global_invocation_id'")
            )),
            "expected kernel-only builtin rejection in field, got: {errors:?}"
        );
    }

    #[test]
    fn test_material_declarations_require_hit3_and_surface_return() {
        let input = r#"material surface(hit: Vec3) -> Integer {
    return 0
}
"#;
        let errors = check_source(input);
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::PortableBoundaryTypeForbidden { function, site, .. }
                    if function.as_str() == "surface"
                        && site.as_str() == "parameter 'hit'"
            )),
            "expected material parameter type rejection, got: {errors:?}"
        );
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::PortableBoundaryTypeForbidden { function, site, .. }
                    if function.as_str() == "surface" && site.as_str() == "return type"
            )),
            "expected material return type rejection, got: {errors:?}"
        );
    }

    #[test]
    fn test_material_declarations_accept_hit3_and_surface_return() {
        let input = r#"material surface(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.2, 0.4, 0.6),
        roughness=0.5,
        metalness=0.1,
        clearcoat=0.25,
        clearcoat_roughness=0.3,
        sheen=0.15,
        emissive=vec3(1.0, 0.0, 0.0)
    )
}
"#;
        let errors = check_source(input);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_material_declarations_reject_kernel_only_builtins() {
        let input = r#"material surface(hit: Hit3) -> Surface {
    gid = global_invocation_id()
    return Surface(
        albedo=vec3(f32(gid[0]), 0.0, 0.0),
        roughness=0.5,
        metalness=0.1,
        clearcoat=0.25,
        clearcoat_roughness=0.3,
        sheen=0.15,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}
"#;
        let errors = check_source(input);
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::PortableConstructForbidden { function, construct, .. }
                    if function.as_str() == "surface"
                        && construct.contains("kernel-only builtin 'global_invocation_id'")
            )),
            "expected kernel-only builtin rejection in material, got: {errors:?}"
        );
    }

    #[test]
    fn test_material_declarations_reject_non_material_portable_calls() {
        let input = r#"kernel fn helper() -> I32 {
    return i32(7)
}

material surface(hit: Hit3) -> Surface {
    seed = helper()
    return Surface(
        albedo=vec3(f32(seed), 0.0, 0.0),
        roughness=0.5,
        metalness=0.1,
        clearcoat=0.25,
        clearcoat_roughness=0.3,
        sheen=0.15,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}
"#;
        let errors = check_source(input);
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::PortableConstructForbidden { function, construct, .. }
                    if function.as_str() == "surface"
                        && construct.contains("non-material portable declaration 'helper'")
            )),
            "expected non-material portable declaration rejection in material, got: {errors:?}"
        );
    }

    #[test]
    fn test_shape_leaf_requires_top_level_field_binding() {
        let input = r#"material shade(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(1.0, 0.0, 0.0),
        roughness=0.0,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape bad_shape {
    field = missing_field
    material = shade
    payload = Payload(
        entity_id=u64(1),
        material_id=u64(1),
        actor=ActorHandle(id=u64(1), generation=u32(0))
    )
}
"#;
        let errors = check_source(input);
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::ShapeBindingTargetInvalid { shape, binding, expected, target, .. }
                    if shape.as_str() == "bad_shape"
                        && *binding == "`field = ...`"
                        && *expected == "field"
                        && target.as_str() == "missing_field"
            )),
            "expected invalid shape field binding error, got: {errors:?}"
        );
    }

    #[test]
    fn test_shape_leaf_requires_top_level_material_binding() {
        let input = r#"field exact distance sphere(p: Vec3) -> F32 {
    sphere(radius = 1.0)
}

shape bad_shape {
    field = sphere
    material = missing_material
    payload = Payload(
        entity_id=u64(1),
        material_id=u64(1),
        actor=ActorHandle(id=u64(1), generation=u32(0))
    )
}
"#;
        let errors = check_source(input);
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::ShapeBindingTargetInvalid { shape, binding, expected, target, .. }
                    if shape.as_str() == "bad_shape"
                        && *binding == "`material = ...`"
                        && *expected == "material"
                        && target.as_str() == "missing_material"
            )),
            "expected invalid shape material binding error, got: {errors:?}"
        );
    }

    #[test]
    fn test_shape_payload_must_evaluate_to_payload() {
        let input = r#"field exact distance sphere(p: Vec3) -> F32 {
    sphere(radius = 1.0)
}

material shade(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(1.0, 0.0, 0.0),
        roughness=0.0,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape bad_shape {
    field = sphere
    material = shade
    payload = i32(7)
}
"#;
        let errors = check_source(input);
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::ShapePayloadTypeForbidden { shape, found, .. }
                    if shape.as_str() == "bad_shape" && found.as_str() == "I32"
            )),
            "expected invalid shape payload error, got: {errors:?}"
        );
    }

    #[test]
    fn test_shape_composition_rejects_cycles() {
        let input = r#"field exact distance sphere(p: Vec3) -> F32 {
    sphere(radius = 1.0)
}

material shade(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(1.0, 0.0, 0.0),
        roughness=0.0,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape leaf_shape {
    field = sphere
    material = shade
    payload = Payload(
        entity_id=u64(1),
        material_id=u64(1),
        actor=ActorHandle(id=u64(1), generation=u32(0))
    )
}

shape recursive_shape {
    union {
        use recursive_shape
        use leaf_shape
    }
}
"#;
        let errors = check_source(input);
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::ShapeCycleDetected { shape, target, .. }
                    if shape.as_str() == "recursive_shape"
                        && target.as_str() == "recursive_shape"
            )),
            "expected shape cycle error, got: {errors:?}"
        );
    }

    #[test]
    fn test_shape_boolean_provenance_policies_typecheck() {
        let input = r#"field exact distance left_field(p: Vec3) -> F32 {
    sphere(radius = 1.0)
}

field exact distance right_field(p: Vec3) -> F32 {
    sphere(radius = 0.5)
}

material shade(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(1.0, 0.0, 0.0),
        roughness=0.0,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape left_shape {
    field = left_field
    material = shade
    payload = Payload(
        entity_id=u64(1),
        material_id=u64(1),
        actor=ActorHandle(id=u64(1), generation=u32(0))
    )
}

shape right_shape {
    field = right_field
    material = shade
    payload = Payload(
        entity_id=u64(2),
        material_id=u64(2),
        actor=ActorHandle(id=u64(2), generation=u32(0))
    )
}

shape union_shape {
    union {
        provenance_policy = nearest
        use left_shape
        use right_shape
    }
}

shape intersection_shape {
    intersection {
        provenance_policy = ordered
        use left_shape
        use right_shape
    }
}

shape subtract_shape {
    subtract {
        provenance_policy = right
        use left_shape
        use right_shape
    }
}
"#;
        let errors = check_source(input);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_field_boolean_provenance_policies_typecheck() {
        let input = r#"field conservative distance composed(p: Vec3) -> F32 {
    subtract {
        provenance_policy = right
        intersection {
            provenance_policy = nearest
            union {
                provenance_policy = nearest
                use left_x
                use left_y
            }
            use cap_z
        }
        use notch
    }
}

field exact distance left_x(p: Vec3) -> F32 {
    sphere(radius = 1.0)
}

field exact distance left_y(p: Vec3) -> F32 {
    sphere(radius = 1.0)
}

field exact distance cap_z(p: Vec3) -> F32 {
    sphere(radius = 1.0)
}

field exact distance notch(p: Vec3) -> F32 {
    sphere(radius = 1.0)
}

material shade(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(1.0, 0.0, 0.0),
        roughness=0.0,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}
"#;
        let errors = check_source(input);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_host_functions_can_call_material_declarations() {
        let input = r#"material shade(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.2, 0.4, 0.6),
        roughness=0.5,
        metalness=0.1,
        clearcoat=0.25,
        clearcoat_roughness=0.3,
        sheen=0.15,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

fn run() -> Nothing {
    handle = ActorHandle(id=u64(1), generation=u32(0))
    payload = Payload(entity_id=u64(2), material_id=u64(3), actor=handle)
    hit = Hit3(
        hit=true,
        distance=1.25,
        position=vec3(0.0, 0.0, 1.0),
        normal=vec3(0.0, 0.0, 1.0),
        steps=0,
        feature_id=u64(0),
        payload=payload
    )
    surface = shade(hit=hit)
    assert approx surface.albedo.x ~= 0.2 within 0.001
    assert approx surface.albedo.y ~= 0.4 within 0.001
}
"#;
        let errors = check_source(input);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_builtin_portable_record_constructors_and_nested_fields_typecheck() {
        let input = r#"kernel fn portable_entry() -> U64 {
    handle = ActorHandle(id=u64(9), generation=u32(2))
    payload = Payload(entity_id=u64(7), material_id=u64(11), actor=handle)
    hit = Hit3(
        hit=true,
        distance=4.25,
        position=vec3(1.0, 2.0, 3.0),
        normal=normalize(vec3(0.0, 2.0, 0.0)),
        steps=0,
        feature_id=u64(0),
        payload=payload
    )
    surface = Surface(
        albedo=vec3(0.2, 0.4, 0.6),
        roughness=0.5,
        metalness=0.1,
        clearcoat=0.25,
        clearcoat_roughness=0.3,
        sheen=0.15,
        emissive=vec3(1.0, 0.0, 0.0)
    )
    medium = Medium(density=0.2, emission=surface.emissive, anisotropy=0.0)
    support = Support3(bounds=Bounds3(
        min=vec3(-1.0, -1.0, -1.0),
        max=vec3(1.0, 1.0, 1.0)
    ))
    camera = Camera(
        position=hit.position,
        forward=vec3(0.0, 0.0, -1.0),
        up=vec3(0.0, 1.0, 0.0),
        vertical_fov_degrees=55.0
    )
    contact = Contact(
        hit=hit.hit,
        position=hit.position,
        normal=hit.normal,
        penetration=0.0,
        payload=hit.payload
    )
    light = Light(
        position=camera.position,
        direction=camera.forward,
        intensity=surface.emissive,
        range=25.0
    )
    assert approx medium.emission.x ~= 1.0 within 0.001
    assert approx support.bounds.max.y ~= 1.0 within 0.001
    assert approx light.direction.z ~= -1.0 within 0.001
    assert approx contact.normal.y ~= 1.0 within 0.001
    return hit.payload.actor.id
}
"#;
        let errors = check_source(input);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_quat_and_mat3_surface_typecheck() {
        let input = r#"fn f() -> Nothing {
    q = quat(1.0, 2.0, 3.0, 4.0)
    assert approx q.x ~= 1.0 within 0.001
    assert approx q.w ~= 4.0 within 0.001
    assert approx length(q) ~= 5.477 within 0.01
    scaled = mix(q, quat(4.0, 3.0, 2.0, 1.0), 0.5)
    clamped = clamp(scaled, 0.0, 10.0)
    rooted = sqrt(9.0)
    casted = f32(i32(u32(rooted)))
    assert approx casted ~= 3.0 within 0.001

    basis = mat3_cols(
        vec3(1.0, 0.0, 0.0),
        vec3(0.0, 1.0, 0.0),
        vec3(0.0, 0.0, 1.0)
    )
    assert approx clamped.x ~= 2.5 within 0.001
}
"#;
        let errors = check_source(input);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_quat_components_are_read_only() {
        let input = r#"fn f() -> Nothing {
    mutable q = quat(1.0, 2.0, 3.0, 4.0)
    q.x = 5.0
}
"#;
        let errors = check_source(input);
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::ImmutableFieldAssign { member, .. } if member.as_str() == "x"
            )),
            "expected ImmutableFieldAssign(x), got: {errors:?}"
        );
    }

    #[test]
    fn test_scalar_math_intrinsics_typecheck() {
        let input = r#"fn f() -> Nothing {
    low = min(1.0, 2.0)
    high = max(1.0, 2.0)
    bounded = clamp(1.5, 0.0, 2.0)
    blended = mix(1.0, 2.0, 0.5)
    absolute = abs(-1.25)
    signed = sign(-1.25)
    floored = floor(1.9)
    ceiled = ceil(1.1)
    fractional = fract(1.25)
    s = sin(0.0)
    c = cos(0.0)
    root = sqrt(9.0)
    power = pow(2.0, 3.0)
    assert approx low ~= 1.0 within 0.001
    assert approx high ~= 2.0 within 0.001
    assert approx bounded ~= 1.5 within 0.001
    assert approx blended ~= 1.5 within 0.001
    assert approx absolute ~= 1.25 within 0.001
    assert approx signed ~= -1.0 within 0.001
    assert approx floored ~= 1.0 within 0.001
    assert approx ceiled ~= 2.0 within 0.001
    assert approx fractional ~= 0.25 within 0.001
    assert approx s ~= 0.0 within 0.001
    assert approx c ~= 1.0 within 0.001
    assert approx root ~= 3.0 within 0.001
    assert approx power ~= 8.0 within 0.001
}
"#;
        let errors = check_source(input);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_mat3_has_no_component_members() {
        let input = r#"fn f() -> Nothing {
    value = mat3_identity()
    assert approx value.x ~= 0.0 within 0.001
}
"#;
        let errors = check_source(input);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::UnknownMember { member, .. } if member.as_str() == "x")),
            "expected UnknownMember(x), got: {errors:?}"
        );
    }

    #[test]
    fn test_distance_and_reflect_reject_quat_operands() {
        let input = r#"fn f() -> Nothing {
    q = quat(1.0, 0.0, 0.0, 1.0)
    assert approx distance(q, q) ~= 0.0 within 0.001
    assert approx reflect(q, q).w ~= 1.0 within 0.001
}
"#;
        let errors = check_source(input);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::ArgumentTypeMismatch { .. })),
            "expected ArgumentTypeMismatch for vector-only math, got: {errors:?}"
        );
    }

    #[test]
    fn test_componentwise_math_rejects_mismatched_shapes() {
        let input = r#"fn f() -> Nothing {
    value = min(vec2(1.0, 2.0), vec3(3.0, 4.0, 5.0))
    other = clamp(vec2(1.0, 2.0), vec3(0.0, 0.0, 0.0), vec2(2.0, 2.0))
    assert approx value.x ~= 1.0 within 0.001
    assert approx other.y ~= 2.0 within 0.001
}
"#;
        let errors = check_source(input);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::ArgumentTypeMismatch { .. })),
            "expected ArgumentTypeMismatch for mismatched math shapes, got: {errors:?}"
        );
    }

    #[test]
    fn test_cast_builtins_reject_vector_inputs() {
        let input = r#"fn f() -> Nothing {
    value = vec3(1.0, 2.0, 3.0)
    casted = f32(value)
    assert approx casted ~= 1.0 within 0.001
}
"#;
        let errors = check_source(input);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::ArgumentTypeMismatch { .. })),
            "expected ArgumentTypeMismatch for scalar casts, got: {errors:?}"
        );
    }

    #[test]
    fn test_mat4_and_vec4_typecheck() {
        let input = r#"fn f() -> Nothing {
    mutable m = mat4_cols(
        vec4(1.0, 0.0, 0.0, 0.0),
        vec4(0.0, 1.0, 0.0, 0.0),
        vec4(0.0, 0.0, 1.0, 0.0),
        vec4(0.0, 0.0, 0.0, 1.0)
    )
    value = m * vec4(1.0, 2.0, 3.0, 1.0)
}
"#;
        let errors = check_source(input);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_vec_and_mat_arithmetic_typecheck() {
        let input = r#"fn f() -> Nothing {
    left = vec3(1.0, 2.0, 3.0)
    right = vec3(4.0, 5.0, 6.0)
    sum = left + right
    delta = right - left
    scaled = sum * 0.5
    restored = scaled / 0.5

    basis = mat4_cols(
        vec4(1.0, 0.0, 0.0, 0.0),
        vec4(0.0, 1.0, 0.0, 0.0),
        vec4(0.0, 0.0, 1.0, 0.0),
        vec4(4.0, 5.0, 6.0, 1.0)
    )
    shifted = basis + mat4_identity()
    lowered = shifted - mat4_identity()
    halved = lowered * 0.5
    restored_matrix = halved / 0.5

    assert approx restored.x ~= 5.0 within 0.001
    assert approx delta.y ~= 3.0 within 0.001
    point = restored_matrix * vec4(1.0, 2.0, 3.0, 1.0)
    assert approx point.w ~= 1.0 within 0.001
}
"#;
        let errors = check_source(input);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_vec_intrinsics_require_matching_dimensions() {
        let input = r#"fn f() -> Nothing {
    value = dot(vec3(1.0, 0.0, 0.0), vec4(1.0, 0.0, 0.0, 0.0))
    assert approx value ~= 1.0 within 0.001
}
"#;
        let errors = check_source(input);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::ArgumentTypeMismatch { .. })),
            "expected ArgumentTypeMismatch, got: {errors:?}"
        );
    }

    #[test]
    fn test_vec_components_reject_unknown_members() {
        let input = r#"fn f() -> Nothing {
    value = vec3(1.0, 2.0, 3.0)
    assert approx value.q ~= 0.0 within 0.001
}
"#;
        let errors = check_source(input);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::UnknownMember { member, .. } if member.as_str() == "q")),
            "expected UnknownMember(q), got: {errors:?}"
        );
    }

    #[test]
    fn test_assert_approx_accepts_numeric_equality_with_numeric_tolerance() {
        let input = r#"fn f() -> Nothing {
    assert approx 1.0 ~= 1.001 within 0.01
}
"#;
        let errors = check_source(input);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_assert_approx_requires_equality_expression() {
        let input = r#"fn f() -> Nothing {
    assert approx 1.0 + 2.0 within 0.01
}
"#;
        let errors = check_source(input);
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::AssertExpectedEquality { mode, .. } if *mode == "approx"
            )),
            "expected AssertExpectedEquality for approx, got: {errors:?}"
        );
    }

    #[test]
    fn test_assert_approx_requires_numeric_operands_and_tolerance() {
        let input = r#"fn f() -> Nothing {
    assert approx true ~= false within 0.01
    assert approx 1.0 ~= 1.0 within "tight"
}
"#;
        let errors = check_source(input);
        let approx_errors = errors
            .iter()
            .filter(|err| matches!(err, TypeError::AssertApproxRequiresNumeric { .. }))
            .count();
        assert_eq!(
            approx_errors, 2,
            "expected two AssertApproxRequiresNumeric errors, got: {errors:?}"
        );
    }

    #[test]
    fn test_match_without_otherwise_all_variants_enum_ok() {
        let input = r#"enum Status {
    Pending
    Done

}
fn f(s: Status) -> Integer {
    match s {
        Status.Pending: return 1
        Status.Done: return 2
    }
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_match_or_pattern_pipe_all_variants_enum_ok() {
        let input = r#"enum Status {
    Pending
    Done

}
fn f(s: Status) -> Integer {
    match s {
        Status.Pending | Status.Done: return 1
    }
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_match_structural_pattern_binds_class_fields() {
        let input = r#"class User {
    has {
        id: Integer
        name: String

    }
}
fn f(user: User) -> Integer {
    match user {
        User { id }: return id
        otherwise: return 0
    }
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_match_structural_pattern_covers_enum_variant() {
        let input = r#"enum Status {
    Pending
    Processing(worker_id: Integer)

}
fn f(status: Status) -> Integer {
    match status {
        Status.Pending: return 0
        Status.Processing { worker_id }: return worker_id
    }
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_match_guard_must_be_boolean() {
        let input = r#"enum Status {
    Pending
    Done

}
fn f(s: Status) -> Integer {
    match s {
        Status.Pending if 1: return 1
        Status.Done: return 2
    }
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::MatchGuardNotBoolean { .. }))
        );
    }

    #[test]
    fn test_match_guarded_cases_are_not_exhaustive_without_otherwise() {
        let input = r#"enum Status {
    Pending
    Done

}
fn f(s: Status, is_ready: Boolean) -> Integer {
    match s {
        Status.Pending | Status.Done if is_ready: return 1
    }
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::MatchNonExhaustive { .. }))
        );
    }

    #[test]
    fn test_match_case_unreachable_after_wildcard() {
        let input = r#"fn f(r: Result[Integer]) -> Integer {
    match r {
        _: return 0
        Ok(value): return value
    }
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::MatchCaseUnreachable { .. }))
        );
    }

    #[test]
    fn test_match_case_unreachable_after_full_enum_coverage() {
        let input = r#"enum Status {
    Pending
    Done

}
fn f(s: Status) -> Integer {
    match s {
        Status.Pending: return 1
        Status.Done: return 2
        Status.Pending: return 3
    }
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::MatchCaseUnreachable { .. }))
        );
    }

    #[test]
    fn test_match_without_otherwise_non_exhaustive_enum_error() {
        let input = r#"enum Status {
    Pending
    Done

}
fn f(s: Status) -> Integer {
    match s {
        Status.Pending: return 1
    }
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::MatchNonExhaustive { .. }))
        );
    }

    #[test]
    fn test_match_without_otherwise_ok_err_result_ok() {
        let input = r#"fn f(r: Result[Integer]) -> Integer {
    match r {
        Ok(x): return x
        Err(_): return 0
    }
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_match_without_otherwise_non_exhaustive_result_error() {
        let input = r#"fn f(r: Result[Integer]) -> Integer {
    match r {
        Ok(x): return x
    }
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::MatchNonExhaustive { .. }))
        );
    }

    #[test]
    fn test_string_concat_allowed() {
        let input = r#"fn f() -> String {
    return "a" + "b"
}"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_assignment_type_mismatch() {
        let input = r#"fn f(x: String) -> Nothing {
    x += 1
}"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::InvalidAssignment { .. }))
        );
    }

    #[test]
    fn test_return_type_mismatch() {
        let input = r#"fn f() -> Boolean {
    return 1
}"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::ReturnTypeMismatch { .. }))
        );
    }

    #[test]
    fn test_if_condition_must_be_boolean() {
        let input = r#"fn f() -> Integer {
    if 1 {
        return 1
    }
    return 0
}"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::IfConditionNotBoolean { .. }))
        );
    }

    #[test]
    fn test_while_condition_must_be_boolean() {
        let input = r#"fn f() -> Integer {
    while 1 {
        return 1
    }
    return 0
}"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::WhileConditionNotBoolean { .. }))
        );
    }

    #[test]
    fn test_logical_and_requires_boolean_rhs() {
        let input = r#"fn f() -> Boolean {
    flag = true
    return flag and 1
}"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::InvalidBinaryOperands { .. }))
        );
    }

    #[test]
    fn test_field_access_type() {
        let input = r#"class Whale {
    name: String
}
fn f(w: Whale) -> String {
    return w.name
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_unknown_member() {
        let input = r#"class Whale {
    has {
        name: String

    }
}
fn f(w: Whale) -> Integer {
    return w.age
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::UnknownMember { .. }))
        );
    }

    #[test]
    fn test_method_call_checked() {
        let input = r#"class Whale {
    fn swim(distance: Integer) -> Boolean {
        return true

    }
}
fn f(w: Whale) -> Boolean {
    return w.swim(true)
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::ArgumentTypeMismatch { .. }))
        );
    }

    #[test]
    fn test_multi_param_method_call_requires_named_args() {
        let input = r#"class Whale {
    fn swim(distance: Integer, speed: Integer) -> Boolean {
        return true

    }
}
fn f(w: Whale) -> Boolean {
    return w.swim(1, 2)
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::NamedArgsRequired { .. }))
        );
    }

    #[test]
    fn test_missing_type_args_on_class_init() {
        let input = r#"class Box[T] {
    has {
        value: T

    }
}
fn f() -> Integer {
    b = Box(value=1)
    return b.value
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::MissingTypeArgs { .. }))
        );
    }

    #[test]
    fn test_unexpected_type_args_on_class_init() {
        let input = r#"class Box {
    has {
        value: Integer

    }
}
fn f() -> Integer {
    b = Box[Integer](value=1)
    return b.value
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::UnexpectedTypeArgs { .. }))
        );
    }

    #[test]
    fn test_interface_missing_method() {
        let input = r#"class Printable {
    must show() -> String

}
class Foo {
    is a Printable
    fn other() -> String {
        return "x"

    }
}
fn f() -> String {
    foo = Foo()
    return foo.other()
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::MissingInterfaceMethod { .. }))
        );
    }

    #[test]
    fn test_interface_method_name_overlap() {
        let input = r#"class Printable {
    must render() -> String

}
class Jsonable {
    must render() -> String

}
class Report {
    is a Printable
    name: String
    fn render() -> String {
        return self.name

    }
}
class Blob {
    is a Jsonable
    fn render() -> String {
        return "blob"

    }
}
fn f(p: Printable) -> String {
    return p.render()
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let (errors, _info) = check_module_with_info(&module);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_interface_boolean_method_allows_direct_call() {
        let input = r#"class Pred {
    must ready() -> Boolean

}
class Foo {
    is a Pred
    fn ready() -> Boolean {
        return true

    }
}
fn f(p: Pred) -> Boolean {
    return p.ready()
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_interface_boolean_method_allows_call_without_legacy_given() {
        let input = r#"class Pred {
    must ready() -> Boolean

}
class Foo {
    is a Pred
    fn ready() -> Boolean {
        return true

    }
}
fn f(p: Pred) -> Boolean {
    return p.ready()
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_interface_must_check_requires_checks_impl() {
        let input = r#"class Pred {
    must check ready() -> Boolean

}
class Foo {
    is a Pred
    fn ready() -> Boolean {
        return true
    }
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::InterfaceMethodMismatch { .. }))
        );
    }

    #[test]
    fn test_given_call_records_boolean_expr_type() {
        let input = r#"
fn is_positive(value: Integer) -> Boolean {
    return value > 0

}
fn f() -> Boolean {
    return is_positive(3)
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let (errors, info) = check_module_with_info(&module);
        assert!(errors.is_empty(), "{errors:?}");

        let (func_id, func) = module
            .functions
            .iter()
            .find(|(_, func)| func.name.as_str() == "f")
            .expect("missing function f");
        let body = func.body.as_ref().expect("missing function body");
        let call_expr = body
            .exprs
            .iter()
            .find_map(|(id, expr)| match expr {
                Expr::Call { .. } => Some(id.into_raw()),
                _ => None,
            })
            .expect("missing call");
        let fn_info = info
            .function(func_id)
            .expect("missing type info for function");
        assert_eq!(fn_info.expr_types.get(&call_expr), Some(&Type::Boolean));
    }

    #[test]
    fn test_given_call_aliases_normal_call_for_non_check_function() {
        let input = r#"
fn add(a: Integer, b: Integer) -> Integer {
    return a + b

}
fn f() -> Integer {
    return add(a=2, b=3)
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let (errors, info) = check_module_with_info(&module);
        assert!(errors.is_empty(), "{errors:?}");

        let (func_id, func) = module
            .functions
            .iter()
            .find(|(_, func)| func.name.as_str() == "f")
            .expect("missing function f");
        let body = func.body.as_ref().expect("missing function body");
        let call_expr = body
            .exprs
            .iter()
            .find_map(|(id, expr)| match expr {
                Expr::Call { .. } => Some(id.into_raw()),
                _ => None,
            })
            .expect("missing call");
        let fn_info = info
            .function(func_id)
            .expect("missing type info for function");
        assert_eq!(fn_info.expr_types.get(&call_expr), Some(&Type::Integer));
    }

    #[test]
    fn test_match_result_bindings_flow() {
        let input = r#"
fn f() -> Integer {
    match __wr_fs_read_bytes("x") {
        Ok(v): return __wr_bytes_len(v)
        Err(e): return 0
        otherwise: return 2
    }
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let (errors, _info) = check_module_with_info(&module);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_nested_pattern_bindings() {
        let input = r#"
enum Status {
    Pending
    Failed(error: String)

}
fn f(s: Status) -> String {
    match s {
        Status.Failed(e): return e
        Status.Pending: return "ok"
        otherwise: return "bad"
    }
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let (errors, _info) = check_module_with_info(&module);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_function_call_checked() {
        let input = r#"fn add(a: Integer, b: Integer) -> Integer {
    return a + b

}
fn f() -> Integer {
    return add(1, true)
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::ArgumentTypeMismatch { .. }))
        );
    }

    #[test]
    fn test_multi_param_function_call_requires_named_args() {
        let input = r#"fn add(a: Integer, b: Integer) -> Integer {
    return a + b

}
fn f() -> Integer {
    return add(1, 2)
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::NamedArgsRequired { .. }))
        );
    }

    #[test]
    fn test_calling_non_callable_errors() {
        let input = r#"fn f() -> Nothing {
    x = 1
    x(2)
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::InvalidCallee { .. }))
        );
    }

    #[test]
    fn test_method_return_type_flow() {
        let input = r#"class Ocean {
    depth: Integer
}
class Whale {
    fn ocean() -> Ocean {
        return Ocean()

    }
}
fn f(w: Whale) -> Integer {
    return w.ocean().depth
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_equality_allows_structural_class_types() {
        let input = r#"class User {
    has {
        id: Integer

    }
}
fn same(a: User, b: User) -> Boolean {
    return a == b
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            !errors
                .iter()
                .any(|err| matches!(err, TypeError::EqualityRequiresEq { .. })),
            "{errors:?}"
        );
    }

    #[test]
    fn test_equality_rejects_class_with_actor_field() {
        let input = r#"class Worker {
    id: Integer
}
class Job {
    worker: Actor[Worker]
}
fn same(a: Job, b: Job) -> Boolean {
    return a == b
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::EqualityRequiresEq { .. })),
            "{errors:?}"
        );
    }

    #[test]
    fn test_equality_allows_structural_nested_class_types() {
        let input = r#"class User {
    has {
        id: Integer
    }
}
class Wrapper {
    has {
        user: User
    }
}
fn same(a: Wrapper, b: Wrapper) -> Boolean {
    return a == b
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            !errors
                .iter()
                .any(|err| matches!(err, TypeError::EqualityRequiresEq { .. })),
            "{errors:?}"
        );
    }

    #[test]
    fn test_equality_rejects_list_of_non_eq_class() {
        let input = r#"class Worker {
    id: Integer
}
class User {
    worker: Actor[Worker]
}
fn same(a: List[User], b: List[User]) -> Boolean {
    return a == b
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::EqualityRequiresEq { left, right, .. }
                    if left == "List[User]" && right == "List[User]"
            )),
            "{errors:?}"
        );
    }

    #[test]
    fn test_equality_allows_structural_enum_types() {
        let input = r#"enum Status {
    Pending
    Done

}
fn same(a: Status, b: Status) -> Boolean {
    return a == b
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            !errors
                .iter()
                .any(|err| matches!(err, TypeError::EqualityRequiresEq { .. })),
            "{errors:?}"
        );
    }

    #[test]
    fn test_equality_rejects_nested_enum_with_pending_payload() {
        let input = r#"class Worker {
    id: Integer
}
enum Status {
    Pending
    Running(task: Pending[Result[Worker]])
}
class Ticket {
    status: Status
}
fn same(a: Ticket, b: Ticket) -> Boolean {
    return a == b
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::EqualityRequiresEq { .. })),
            "{errors:?}"
        );
    }

    #[test]
    fn test_actor_call_requires_await_or_fire() {
        let input = r#"class Whale {
    fn swim() -> Boolean {
        return true

    }
}
fn f() -> Nothing {
    w = detach Whale() * 1
    w.swim()
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::PendingNotAwaited { .. }))
        );
    }

    #[test]
    fn test_error_requires_result_function() {
        let input = r#"fn f() -> Integer {
    error "nope"
}"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::ErrOutsideResult { .. }))
        );
    }

    #[test]
    fn test_try_unwraps_result_in_result_function() {
        let input = r#"fn source() -> Result[Integer] {
    return 1

}
fn f() -> Result[Integer] {
    value = source()?
    return value
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_try_requires_result_returning_function() {
        let input = r#"fn source() -> Result[Integer] {
    return 1

}
fn f() -> Integer {
    return source()?
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::TryOutsideResult { .. }))
        );
    }

    #[test]
    fn test_try_requires_result_operand() {
        let input = r#"fn f() -> Result[Integer] {
    return 1?
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::InvalidTryOperand { .. }))
        );
    }

    #[test]
    fn test_try_then_or_else_is_invalid() {
        let input = r#"fn source() -> Result[Integer] {
    return 1

}
fn f() -> Result[Integer] {
    return source()? ?? 0
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::InvalidOtherwiseOperand { .. }))
        );
    }

    #[test]
    fn test_result_fallback_handles_result() {
        let input = r#"fn f() -> Result[Integer, RuntimeError] {
    return error "nope" ?? 0
}"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_or_else_handles_result() {
        let input = r#"fn f() -> Result[Integer, RuntimeError] {
    return error "nope" ?? 0
}"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_invalid_result_fallback_operand() {
        let input = r#"fn f() -> Integer {
    return 1 ?? 0
}"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::InvalidOtherwiseOperand { .. }))
        );
    }

    #[test]
    fn test_invalid_or_else_operand() {
        let input = r#"fn f() -> Integer {
    return 1 ?? 0
}"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::InvalidOtherwiseOperand { .. }))
        );
    }

    #[test]
    fn test_boundary_list_requires_type_args() {
        let input = r#"fn f(items: List) -> Integer {
    return 1
}"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.iter().any(
            |err| matches!(err, TypeError::BoundaryMissingTypeArgs { name, .. } if name == "List")
        ));
    }

    #[test]
    fn test_boundary_result_requires_type_args() {
        let input = r#"fn f() -> Result {
    return error "nope"
}"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.iter().any(
            |err| matches!(err, TypeError::BoundaryMissingTypeArgs { name, .. } if name == "Result")
        ));
    }

    #[test]
    fn test_boundary_pending_requires_type_args() {
        let input = r#"fn f(task: Pending) -> Integer {
    return 1
}"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.iter().any(
            |err| matches!(err, TypeError::BoundaryMissingTypeArgs { name, .. } if name == "Pending")
        ));
    }

    #[test]
    fn test_invalid_unary_operand_span() {
        let input = r#"fn f() -> Integer {
    -true
}"#;
        let canonical = input.to_string();
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        let err = errors
            .iter()
            .find(|err| matches!(err, TypeError::InvalidUnaryOperand { .. }))
            .expect("missing invalid unary operand error");
        if let TypeError::InvalidUnaryOperand { span, .. } = err {
            let expected = canonical.rfind('-').unwrap();
            assert_eq!(span.offset(), expected);
            assert_eq!(span.len(), 1);
        }
    }

    #[test]
    fn test_invalid_binary_operand_span() {
        let input = r#"fn f() -> Integer {
    true + 1
}"#;
        let canonical = input.to_string();
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        let err = errors
            .iter()
            .find(|err| matches!(err, TypeError::InvalidBinaryOperands { .. }))
            .expect("missing invalid binary operands error");
        if let TypeError::InvalidBinaryOperands { span, .. } = err {
            let expected = canonical.find('+').unwrap();
            assert_eq!(span.offset(), expected);
            assert_eq!(span.len(), 1);
        }
    }

    #[test]
    fn test_unknown_member_span() {
        let input = r#"class Foo {
    has {
        x: Integer

    }
}
fn f() -> Nothing {
    foo = Foo(x=1)
    foo.bar
}
"#;
        let canonical = input.to_string();
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        let err = errors
            .iter()
            .find(|err| matches!(err, TypeError::UnknownMember { .. }))
            .expect("missing unknown member error");
        if let TypeError::UnknownMember { span, .. } = err {
            let expected = canonical.find("bar").unwrap();
            assert_eq!(span.offset(), expected);
            assert_eq!(span.len(), 3);
        }
    }

    #[test]
    fn test_actor_call_with_await_ok() {
        let input = r#"class Whale {
    fn swim() -> Boolean {
        return true

    }
}
fn f() -> Result[Boolean, Error] {
    w = detach Whale() * 1
    return await w.swim()
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_builtin_fallible_requires_handling() {
        let input = r#"fn f() -> Nothing {
    __wr_fs_read_bytes("x")
}"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::UnhandledResult { .. }))
        );
    }

    #[test]
    fn test_builtin_fallible_or_else_ok() {
        let input = r#"fn f() -> Integer {
    return __wr_bytes_len(__wr_fs_read_bytes("x") ?? __wr_bytes_from_string("1"))
}"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_builtin_external_call_requires_handling() {
        let input = r#"fn f() -> Nothing {
    headers = __wr_map_new()
    __wr_external_call(service="svc", endpoint="ep", method="GET", url="https://example", headers=headers, body="", timeout_ms=10)
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::UnhandledResult { .. }))
        );
    }

    #[test]
    fn test_builtin_external_call_or_else_ok() {
        let input = r#"fn f() -> String {
    headers = __wr_map_new()
    return __wr_external_call(service="svc", endpoint="ep", method="GET", url="https://example", headers=headers, body="", timeout_ms=10) ?? "fallback"
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_builtin_map_new_signature_ok() {
        let input = r#"fn f() -> Nothing {
    m = __wr_map_new()
    __wr_map_set(map=m, key="k", value="v")
    __wr_map_get(map=m, key="k")
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_collection_methods_and_index_typecheck() {
        let input = r#"fn f() -> Integer {
    xs = [1]
    m = {"a": 2}
    xs.push(3)
    m.set(key="b", value=4)
    left = xs[0]
    right = m["a"]
    xs[1] = 5
    m.set(key="b", value=6)
    return left + right + xs.len() + m.len()
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_for_with_index_requires_list_or_range() {
        let input = r#"fn f() -> Nothing {
    m = {"k": 1}
    for value in m with index idx {
        nothing
    }
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.iter().any(|err| {
            matches!(
                err,
                TypeError::ForWithIndexRequiresListOrRange { .. }
                    | TypeError::ForMapWithIndexUnsupported { .. }
            )
        }));
    }

    #[test]
    fn test_for_map_binding_requires_map_iterable() {
        let input = r#"fn f() -> Nothing {
    xs = [1]
    for key, value in xs {
        nothing
    }
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::ForMapRequiresMap { .. }))
        );
    }

    #[test]
    fn test_index_type_mismatch_reports_error() {
        let input = r#"fn f() -> Integer {
    xs = [1]
    return xs["bad"]
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::InvalidIndexType { .. }))
        );
    }

    #[test]
    fn test_builtin_map_new_arg_count_mismatch() {
        let input = r#"fn f() -> Nothing {
    __wr_map_new(1)
}"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::ArgumentCountMismatch { .. }))
        );
    }

    #[test]
    fn test_await_on_pending_value_ok() {
        let input = r#"fn f() -> Result[Nothing, Error] {
    return await __wr_sleep_ms(1)
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_fire_on_pending_value_ok() {
        let input = r#"fn f() -> Nothing {
    fire __wr_sleep_ms(1)
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_await_on_non_actor_call_errors() {
        let input = r#"class Whale {
    fn swim() -> Boolean {
        return true

    }
}
fn f(w: Whale) -> Result[Boolean] {
    return await w.swim()
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::InvalidAwaitOperand { .. }))
        );
    }

    #[test]
    fn test_fire_actor_call_ok() {
        let input = r#"class Whale {
    fn swim() -> Boolean {
        return true

    }
}
fn f() -> Nothing {
    w = detach Whale() * 1
    fire w.swim()
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_fire_non_actor_call_errors() {
        let input = r#"class Whale {
    fn swim() -> Boolean {
        return true

    }
}
fn f(w: Whale) -> Nothing {
    fire w.swim()
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::InvalidFireOperand { .. }))
        );
    }

    #[test]
    fn test_class_init_field_type_checked() {
        let input = r#"class Whale {
    name: String
}
fn f() -> Nothing {
    Whale(name=1)
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::ArgumentTypeMismatch { .. }))
        );
    }

    #[test]
    fn test_class_init_unknown_field() {
        let input = r#"class Whale {
    has {
        name: String

    }
}
fn f() -> Nothing {
    Whale(age="old")
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::UnknownArgument { .. }))
        );
    }

    #[test]
    fn test_multi_field_class_init_requires_named_args() {
        let input = r#"class Whale {
    name: String
    age: Integer
}
fn f() -> Nothing {
    Whale("orca", 7)
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::NamedArgsRequired { .. }))
        );
    }

    #[test]
    fn test_await_on_actor_value_errors() {
        let input = r#"class Whale {
    fn swim() -> Boolean {
        return true

    }
}
fn f() -> Result {
    w = detach Whale() * 1
    return await w
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::InvalidAwaitOperand { .. }))
        );
    }

    #[test]
    fn test_fire_on_actor_value_errors() {
        let input = r#"class Whale {
    fn swim() -> Boolean {
        return true

    }
}
fn f() -> Nothing {
    w = detach Whale() * 1
    fire w
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::InvalidFireOperand { .. }))
        );
    }

    #[test]
    fn test_async_class_requires_actor() {
        let input = r#"class Whale {
    fn swim() -> Boolean {
        return true

    }
}
class Boat {
    fn ride() -> Boolean {
        return await Whale().swim()

    }
}
fn f() -> Nothing {
    Boat()
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::AsyncClassRequiresActor { .. }))
        );
    }

    #[test]
    fn test_async_method_requires_actor() {
        let input = r#"class Whale {
    fn swim() -> Boolean {
        return true

    }
}
class Boat {
    fn ride() -> Boolean {
        return await Whale().swim()

    }
}
fn f() -> Boolean {
    b = Boat()
    return b.ride()
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::AsyncMethodRequiresActor { .. }))
        );
    }

    #[test]
    fn test_async_chain_requires_actor() {
        let input = r#"class Whale {
    fn swim() -> Boolean {
        return true

    }
}
fn helper() -> Boolean {
    return await Whale().swim()

}
class Boat {
    fn ride() -> Boolean {
        return helper()

    }
}
fn f() -> Nothing {
    Boat()
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::AsyncClassRequiresActor { .. }))
        );
    }

    #[test]
    fn test_async_error_includes_chain_hint() {
        let input = r#"class Whale {
    fn swim() -> Boolean {
        return true

    }
}
fn helper() -> Boolean {
    return await Whale().swim()

}
class Boat {
    fn ride() -> Boolean {
        return helper()

    }
}
fn f() -> Boolean {
    b = Boat()
    return b.ride()
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        let mut saw = false;
        for err in &errors {
            if let TypeError::AsyncMethodRequiresActor { help, .. } = err {
                assert!(help.contains("Async call chain:"));
                assert!(help.contains("Boat.ride"));
                assert!(help.contains("helper"));
                saw = true;
                break;
            }
        }
        assert!(saw, "expected AsyncMethodRequiresActor error");
    }

    #[test]
    fn test_fire_chain_requires_actor() {
        let input = r#"class Whale {
    fn swim() -> Boolean {
        return true

    }
}
fn helper() -> Boolean {
    fire Whale().swim()
    return true

}
class Boat {
    fn ride() -> Boolean {
        return helper()

    }
}
fn f() -> Nothing {
    Boat()
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::AsyncClassRequiresActor { .. }))
        );
    }

    #[test]
    fn test_async_class_allowed_with_detach() {
        let input = r#"class Whale {
    fn swim() -> Boolean {
        return true

    }
}
class Boat {
    fn ride() -> Boolean {
        return await Whale().swim()

    }
}
fn f() -> Result {
    b = detach Boat() * 1
    return await b.ride()
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .all(|err| !matches!(err, TypeError::AsyncClassRequiresActor { .. }))
        );
    }

    #[test]
    fn test_deterministic_game_module_rejects_float_literal() {
        let input = r#"node PositionNode profile world {
    x: Integer
}
system tick[stage=fixed, reads=[PositionNode], writes=[PositionNode]]() -> Nothing {
    value = 1.5
    return
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::DeterministicFloatLiteralForbidden { .. }))
        );
    }

    #[test]
    fn test_deterministic_game_module_rejects_float_type_refs() {
        let input = r#"class PositionNode {
    x: Float
}
system tick[stage=fixed, reads=[PositionNode], writes=[PositionNode]]() -> Nothing {
    return
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::DeterministicFloatTypeForbidden { .. }))
        );
    }

    #[test]
    fn test_node_only_module_is_still_deterministic() {
        let input = r#"resource PositionNode {
    x: Float
}
system tick[stage=fixed, reads=[PositionNode], writes=[PositionNode]]() -> Nothing {
    return
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::DeterministicFloatTypeForbidden { .. }))
        );
    }

    #[test]
    fn test_non_game_module_allows_float_type_and_literals() {
        let input = r#"fn lerp(a: Float, b: Float) -> Float {
    return 1.5
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .all(|err| !matches!(err, TypeError::DeterministicFloatTypeForbidden { .. }))
        );
        assert!(
            errors
                .iter()
                .all(|err| !matches!(err, TypeError::DeterministicFloatLiteralForbidden { .. }))
        );
    }

    #[test]
    fn test_generic_function_type_param_parsed() {
        // A generic function with a type parameter should lower and type-check without errors
        let input = r#"fn identity[T](x: T) -> T {
    return x
}"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        // Verify type_params were lowered
        let func = module
            .functions
            .iter()
            .next()
            .expect("expected a function")
            .1;
        assert_eq!(func.type_params.len(), 1, "Expected 1 type param");
        assert_eq!(func.type_params[0].name, "T");
        let errors = check_module(&module);
        assert!(errors.is_empty(), "Expected no errors, got: {:?}", errors);
    }

    #[test]
    fn test_generic_function_multiple_type_params() {
        // A generic function with multiple type parameters
        let input = r#"fn swap[A, B](a: A, b: B) -> A {
    return a
}"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let func = module
            .functions
            .iter()
            .next()
            .expect("expected a function")
            .1;
        assert_eq!(func.type_params.len(), 2, "Expected 2 type params");
        assert_eq!(func.type_params[0].name, "A");
        assert_eq!(func.type_params[1].name, "B");
        let errors = check_module(&module);
        assert!(errors.is_empty(), "Expected no errors, got: {:?}", errors);
    }

    #[test]
    fn test_generic_function_bound_syntax_parses() {
        // A generic function with a type bound should parse, lower, and store the bound
        let input = r#"fn constrained[T: Hashable](x: T) -> T {
    return x
}"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let func = module
            .functions
            .iter()
            .next()
            .expect("expected a function")
            .1;
        assert_eq!(
            func.type_params.len(),
            1,
            "Expected 1 type param, got {:?}",
            func.type_params
        );
        assert_eq!(func.type_params[0].name, "T");
        assert_eq!(
            func.type_params[0].bounds,
            vec!["Hashable"],
            "Expected bound 'Hashable'"
        );
        let errors = check_module(&module);
        assert!(errors.is_empty(), "Expected no errors, got: {:?}", errors);
    }

    #[test]
    fn test_non_generic_function_unexpected_type_args() {
        // Passing explicit type args to a non-generic function should produce an error
        let input = r#"fn plain(x: Integer) -> Integer {
    return x
}
fn caller() -> Integer {
    return plain[Integer](1)
}"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::UnexpectedTypeArgs { .. })),
            "Expected UnexpectedTypeArgs error, got: {:?}",
            errors
        );
    }

    #[test]
    fn test_type_param_bound_violation() {
        // Calling a generic function with a bound, passing a type that does not satisfy it
        let input = r#"class Foo {
    x: Integer
}
fn bounded[T: Hashable](x: T) -> T {
    return x
}
fn caller() -> Foo {
    return bounded[Foo](Foo(x: 1))
}"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::TypeParamBoundNotSatisfied { .. })),
            "Expected TypeParamBoundNotSatisfied error, got: {:?}",
            errors
        );
    }

    #[test]
    fn test_radiance_and_volume_declarations_and_shape_leaf_bindings_typecheck() {
        let input = r#"radiance field emit_sky(p: Vec3, direction: Vec3, feature_id: U64) -> Vec3 {
    return p * 0.0 + direction + vec3(f32(feature_id) * 0.0, 0.0, 0.0)
}

volume field accumulate_fog(p: Vec3, surface_distance: F32) -> Medium {
    return Medium(
        density=clamp(0.1 + abs(surface_distance) * 0.0, 0.0, 1.0),
        emission=vec3(0.0, 0.0, 0.0),
        anisotropy=0.0
    )
}

field exact distance sphere_field(p: Vec3) -> F32 {
    sphere(radius=1.0)
}

material shade_surface(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(1.0, 0.5, 0.25),
        roughness=0.5,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

field exact distance sphere_field(p: Vec3) -> F32 {
    sphere(radius=1.0)
}

shape scene_shape {
    field = sphere_field
    material = shade_surface
    radiance = emit_sky
    volume = accumulate_fog
    payload = Payload(
        entity_id=u64(1),
        material_id=u64(2),
        actor=ActorHandle(id=u64(3), generation=u32(0))
    )
}
"#;
        let errors = check_source(input);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_radiance_and_volume_declarations_enforce_return_boundaries() {
        let input = r#"radiance field bad_radiance(direction: Vec3) -> Surface {
    return Surface(
        albedo=vec3(1.0, 1.0, 1.0),
        roughness=0.0,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

volume field bad_volume(p: Vec3) -> Vec3 {
    return vec3(1.0, 0.0, 0.0)
}
"#;
        let errors = check_source(input);
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::PortableBoundaryTypeForbidden { function, found, .. }
                    if function.as_str() == "bad_radiance" && found == "Surface"
            )),
            "expected bad_radiance return-type rejection, got: {errors:?}"
        );
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::PortableBoundaryTypeForbidden { function, found, .. }
                    if function.as_str() == "bad_volume" && found == "Vec3"
            )),
            "expected bad_volume return-type rejection, got: {errors:?}"
        );
    }

    #[test]
    fn test_radiance_and_volume_declarations_enforce_parameter_boundaries() {
        let input = r#"radiance field missing_point() -> Vec3 {
    return vec3(0.0, 0.0, 0.0)
}

radiance field wrong_feature(p: Vec3, direction: Vec3, feature_id: Integer) -> Vec3 {
    return direction
}

volume field too_many_volume_params(p: Vec3, surface_distance: F32, extra: F32) -> Medium {
    return Medium()
}

volume field wrong_volume_distance(p: Vec3, surface_distance: Integer) -> Medium {
    return Medium()
}
"#;
        let errors = check_source(input);
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::PortableConstructForbidden { function, .. }
                    if function.as_str() == "missing_point"
            )),
            "expected radiance arity rejection, got: {errors:?}"
        );
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::PortableBoundaryTypeForbidden { function, site, found, .. }
                    if function.as_str() == "wrong_feature"
                        && site == "parameter 'feature_id'"
                        && found == "Integer"
            )),
            "expected radiance feature-id rejection, got: {errors:?}"
        );
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::PortableConstructForbidden { function, .. }
                    if function.as_str() == "too_many_volume_params"
            )),
            "expected volume arity rejection, got: {errors:?}"
        );
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::PortableBoundaryTypeForbidden { function, site, found, .. }
                    if function.as_str() == "wrong_volume_distance"
                        && site == "parameter 'surface_distance'"
                        && found == "Integer"
            )),
            "expected volume parameter rejection, got: {errors:?}"
        );
    }

    #[test]
    fn test_shape_leaf_radiance_and_volume_bindings_require_top_level_declarations() {
        let input = r#"radiance field emit_sky(p: Vec3) -> Vec3 {
    return p
}

material shade_surface(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(1.0, 1.0, 1.0),
        roughness=0.0,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape scene_shape {
    field = sphere_field
    material = shade_surface
    radiance = shade_surface
    volume = emit_sky
    payload = Payload(
        entity_id=u64(1),
        material_id=u64(2),
        actor=ActorHandle(id=u64(3), generation=u32(0))
    )
}
"#;
        let errors = check_source(input);
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::ShapeBindingTargetInvalid { binding, expected, target, .. }
                    if *binding == "`radiance = ...`"
                        && *expected == "radiance field"
                        && target.as_str() == "shade_surface"
            )),
            "expected radiance binding rejection, got: {errors:?}"
        );
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::ShapeBindingTargetInvalid { binding, expected, target, .. }
                    if *binding == "`volume = ...`"
                        && *expected == "volume field"
                        && target.as_str() == "emit_sky"
            )),
            "expected volume binding rejection, got: {errors:?}"
        );
    }

    #[test]
    fn test_radiance_and_volume_queries_follow_the_phase7_helper_surface() {
        let input = r#"field exact distance phase7_shell(p: Vec3) -> F32 {
    sphere(radius = 0.45)
}

radiance field phase7_radiance(p: Vec3, direction: Vec3, feature_id: U64) -> Vec3 {
    return vec3(0.25, 0.5, 0.75) + direction * 0.0 + vec3(f32(feature_id) * 0.0 + p.x * 0.0, 0.0, 0.0)
}

volume field phase7_volume(p: Vec3, surface_distance: F32) -> Medium {
    return Medium(
        density=clamp(0.1 + abs(surface_distance) * 0.0, 0.0, 1.0),
        emission=vec3(0.0, 0.0, 0.0),
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
        entity_id=u64(901),
        material_id=u64(901),
        actor=ActorHandle(id=u64(901), generation=u32(0))
    )
}

fn render() -> Nothing {
    scene_capture = capture phase7_scene_shape
    hit = trace_shape(
        capture=scene_capture,
        origin=vec3(0.0, 0.0, 3.0),
        direction=vec3(0.0, 0.0, -1.0),
        max_distance=6.0,
        min_step=0.05,
        hit_epsilon=0.001,
        max_steps=96
    )
    surface = surface_at(capture=scene_capture, hit=hit)
    radiance_sample = radiance_at(
        capture=scene_capture,
        point=hit.position,
        direction=vec3(0.0, 0.0, -1.0)
    )
    medium_sample = medium_at(capture=scene_capture, point=hit.position)

    assert value hit.hit == true
    assert approx surface.albedo.x ~= 0.3 within 0.001
    assert approx radiance_sample.y ~= 0.2 within 0.001
    assert approx medium_sample.anisotropy ~= 0.0 within 0.001
}
"#;
        let errors = check_source(input);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_radiance_and_volume_queries_require_shape_capture() {
        let input = r#"field exact distance shell_field(p: Vec3) -> F32 {
    sphere(radius=1.0)
}

fn render() -> Nothing {
    scene_capture = capture shell_field
    _ = radiance_at(
        capture=scene_capture,
        point=vec3(0.0, 0.0, 0.0),
        direction=vec3(0.0, 0.0, -1.0)
    )
    _ = medium_at(capture=scene_capture, point=vec3(0.0, 0.0, 0.0))
}
"#;
        let errors = check_source(input);
        let rejection_count = errors
            .iter()
            .filter(|err| matches!(
                err,
                TypeError::ShapeQueryTargetMustBeShape { query, .. }
                    if query.as_str() == "radiance_at" || query.as_str() == "medium_at"
            ))
            .count();
        assert_eq!(
            rejection_count, 2,
            "expected radiance_at and medium_at to reject field captures, got: {errors:?}"
        );
    }
}
