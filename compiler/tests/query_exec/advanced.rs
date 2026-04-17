use super::*;
use wrela::query_plan::PruningStrategy;

pub(super) fn direct_semantics_source() -> &'static str {
    r#"
field exact distance near_field(p: Vec3) -> F32 {
    sphere(radius = 0.6)
}
field exact distance far_field(p: Vec3) -> F32 {
    translate = vec3(0.0, 0.0, -0.35) {
        sphere(radius = 0.8)
    }
}

field conservative distance identity_field(p: Vec3) -> F32 {
    instance_array = Transform3(
        matrix=mat4_cols(
            vec4(1.0, 0.0, 0.0, 0.0),
            vec4(0.0, 1.0, 0.0, 0.0),
            vec4(0.0, 0.0, 1.0, 0.0),
            vec4(1.0, 0.0, 0.0, 1.0)
        ),
        inverse=mat4_cols(
            vec4(1.0, 0.0, 0.0, 0.0),
            vec4(0.0, 1.0, 0.0, 0.0),
            vec4(0.0, 0.0, 1.0, 0.0),
            vec4(-1.0, 0.0, 0.0, 1.0)
        )
    ) {
        repeat_linear = vec3(2.0, 0.0, 0.0) {
            translate = vec3(0.25, 0.0, 0.0) {
                sphere(radius = 0.5)
            }
        }
    }
}

field exact distance left_glow_field(p: Vec3) -> F32 {
    translate = vec3(-1.5, 0.0, 0.0) {
        sphere(radius = 0.25)
    }
}

field exact distance right_glow_field(p: Vec3) -> F32 {
    translate = vec3(0.25, 0.0, 0.0) {
        sphere(radius = 0.25)
    }
}

material near_surface(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.1, 0.2, 0.3),
        roughness=0.0,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

material far_surface(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.9, 0.1, 0.1),
        roughness=0.0,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

material identity_surface(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.3, 0.3, 0.3),
        roughness=0.0,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

radiance field glow_local(p: Vec3, direction: Vec3, feature_id: U32) -> Vec3 {
    return vec3(abs(p.x), abs(p.x) * 0.5, f32(feature_id) * 0.0) + direction * 0.0
}

volume field fog_local(p: Vec3, surface_distance: F32) -> Medium {
    return Medium(
        density=abs(p.x),
        emission=vec3(abs(surface_distance), 0.0, 0.0),
        anisotropy=0.25
    )
}

shape near_shape {
    field = near_field
    material = near_surface
    payload = Payload(
        entity_id=u32(101),
        material_id=u32(101),
        actor=ActorHandle(id=u32(101), generation=u32(0))
    )
}

shape far_shape {
    field = far_field
    material = far_surface
    payload = Payload(
        entity_id=u32(202),
        material_id=u32(202),
        actor=ActorHandle(id=u32(202), generation=u32(0))
    )
}

shape nearest_scene {
    union {
        provenance_policy = nearest
        use far_shape
        use near_shape
    }
}

shape ordered_scene {
    union {
        provenance_policy = ordered
        use far_shape
        use near_shape
    }
}

shape identity_shape {
    field = identity_field
    material = identity_surface
    payload = Payload(
        entity_id=u32(303),
        material_id=u32(303),
        actor=ActorHandle(id=u32(303), generation=u32(0))
    )
}

shape left_glow_shape {
    field = left_glow_field
    material = identity_surface
    radiance = glow_local
    volume = fog_local
    payload = Payload()
}

shape right_glow_shape {
    field = right_glow_field
    material = identity_surface
    radiance = glow_local
    volume = fog_local
    payload = Payload()
}

shape lighting_scene {
    union {
        provenance_policy = nearest
        use left_glow_shape
        use right_glow_shape
    }
}
"#
}

pub(super) fn world_support_cost_fixture_source() -> &'static str {
    r#"
field exact distance near_field(p: Vec3) -> F32 {
    sphere(radius = 0.6)
}

field conservative distance far_supported_field(p: Vec3) -> F32 {
    support = Support3(bounds=Bounds3(
        min=vec3(8.8, -0.8, -0.8),
        max=vec3(10.2, 0.8, 0.8)
    ))
    bounds = Bounds3(
        min=vec3(8.8, -0.8, -0.8),
        max=vec3(10.2, 0.8, 0.8)
    )
    return length(p - vec3(9.5, 0.0, 0.0)) - 0.5
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
    payload = Payload(entity_id=u32(1), material_id=u32(1), actor=ActorHandle(id=u32(1), generation=u32(0)))
}

shape far_shape {
    field = far_supported_field
    material = shade
    payload = Payload(entity_id=u32(2), material_id=u32(2), actor=ActorHandle(id=u32(2), generation=u32(0)))
}

region scene_region() {
    place near = near_shape
    place far = far_shape
}

domain scene_domain(world: RegionCapture) {
    geometry_detail = 1
    material = false
    radiance = false
    media = false
    max_distance = 6.0
    min_step = 0.05
    hit_epsilon = 0.001
    max_steps = 96
}
"#
}

pub(super) fn world_ray_solver_support_fixture_source() -> &'static str {
    r#"
field exact distance near_field(p: Vec3) -> F32 {
    sphere(radius = 0.6)
}

field exact distance far_supported_field(p: Vec3) -> F32 {
    translate = vec3(9.5, 0.0, 0.0) {
        sphere(radius = 0.5)
    }
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
    payload = Payload(entity_id=u32(1), material_id=u32(1), actor=ActorHandle(id=u32(1), generation=u32(0)))
}

shape far_shape {
    field = far_supported_field
    material = shade
    payload = Payload(entity_id=u32(2), material_id=u32(2), actor=ActorHandle(id=u32(2), generation=u32(0)))
}

region scene_region() {
    place near = near_shape
    place far = far_shape
}

domain scene_domain(world: RegionCapture) {
    geometry_detail = 1
    material = false
    radiance = false
    media = false
    max_distance = 6.0
    min_step = 0.05
    hit_epsilon = 0.001
    max_steps = 96
}
"#
}

pub(super) fn world_ray_support_interval_fixture_source() -> &'static str {
    r#"
field exact distance jump_field(p: Vec3) -> F32 {
    translate = vec3(2.5, 0.0, 0.0) {
        sphere(radius = 0.5)
    }
}

field exact distance far_field(p: Vec3) -> F32 {
    translate = vec3(12.0, 0.0, 0.0) {
        sphere(radius = 0.75)
    }
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

shape jump_shape {
    field = jump_field
    material = shade
    payload = Payload(entity_id=u32(11), material_id=u32(11), actor=ActorHandle(id=u32(11), generation=u32(0)))
}

shape far_shape {
    field = far_field
    material = shade
    payload = Payload(entity_id=u32(22), material_id=u32(22), actor=ActorHandle(id=u32(22), generation=u32(0)))
}

region scene_region() {
    place jump = jump_shape
    place far = far_shape
}

domain scene_domain(world: RegionCapture) {
    geometry_detail = 1
    material = false
    radiance = false
    media = false
    max_distance = 6.0
    min_step = 0.05
    hit_epsilon = 0.001
    max_steps = 96
}
"#
}

pub(super) fn world_ray_support_interval_variants_fixture_source() -> &'static str {
    r#"
field exact distance inside_field(p: Vec3) -> F32 {
    sphere(radius = 1.0)
}

field exact distance tangent_field(p: Vec3) -> F32 {
    translate = vec3(2.5, 0.5, 0.0) {
        sphere(radius = 0.5)
    }
}

field exact distance translated_field(p: Vec3) -> F32 {
    translate = vec3(2.5, 0.0, 0.0) {
        sphere(radius = 0.5)
    }
}

field exact distance scaled_field(p: Vec3) -> F32 {
    uniform_scale = f32(2.0) {
        sphere(radius = 0.5)
    }
}

field conservative distance mirrored_field(p: Vec3) -> F32 {
    mirror_array = vec3(1.0, 0.0, 0.0) {
        translate = vec3(2.5, 0.0, 0.0) {
            sphere(radius = 0.5)
        }
    }
}

field conservative distance repeat_linear_field(p: Vec3) -> F32 {
    repeat_linear = vec3(2.5, 0.0, 0.0) {
        translate = vec3(0.25, 0.0, 0.0) {
            sphere(radius = 0.5)
        }
    }
}

field conservative distance repeat_grid_field(p: Vec3) -> F32 {
    repeat_grid = vec3(2.5, 2.5, 0.0) {
        translate = vec3(0.25, 0.25, 0.0) {
            sphere(radius = 0.4)
        }
    }
}

field conservative distance radial_repeat_field(p: Vec3) -> F32 {
    radial_repeat = vec3(4.0, 0.0, 0.0) {
        translate = vec3(2.5, 0.0, 0.0) {
            sphere(radius = 0.5)
        }
    }
}

field exact distance miss_field(p: Vec3) -> F32 {
    translate = vec3(0.0, 2.0, 0.0) {
        sphere(radius = 0.5)
    }
}

field exact distance distractor_field(p: Vec3) -> F32 {
    translate = vec3(0.0, 3.0, 0.0) {
        sphere(radius = 0.5)
    }
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

shape inside_shape {
    field = inside_field
    material = shade
    payload = Payload(entity_id=u32(31), material_id=u32(31), actor=ActorHandle(id=u32(31), generation=u32(0)))
}

shape tangent_shape {
    field = tangent_field
    material = shade
    payload = Payload(entity_id=u32(32), material_id=u32(32), actor=ActorHandle(id=u32(32), generation=u32(0)))
}

shape translated_shape {
    field = translated_field
    material = shade
    payload = Payload(entity_id=u32(33), material_id=u32(33), actor=ActorHandle(id=u32(33), generation=u32(0)))
}

shape scaled_shape {
    field = scaled_field
    material = shade
    payload = Payload(entity_id=u32(36), material_id=u32(36), actor=ActorHandle(id=u32(36), generation=u32(0)))
}

shape mirrored_shape {
    field = mirrored_field
    material = shade
    payload = Payload(entity_id=u32(34), material_id=u32(34), actor=ActorHandle(id=u32(34), generation=u32(0)))
}

shape repeat_linear_shape {
    field = repeat_linear_field
    material = shade
    payload = Payload(entity_id=u32(37), material_id=u32(37), actor=ActorHandle(id=u32(37), generation=u32(0)))
}

shape repeat_grid_shape {
    field = repeat_grid_field
    material = shade
    payload = Payload(entity_id=u32(38), material_id=u32(38), actor=ActorHandle(id=u32(38), generation=u32(0)))
}

shape radial_repeat_shape {
    field = radial_repeat_field
    material = shade
    payload = Payload(entity_id=u32(39), material_id=u32(39), actor=ActorHandle(id=u32(39), generation=u32(0)))
}

shape miss_shape {
    field = miss_field
    material = shade
    payload = Payload(entity_id=u32(35), material_id=u32(35), actor=ActorHandle(id=u32(35), generation=u32(0)))
}

shape distractor_shape {
    field = distractor_field
    material = shade
    payload = Payload(entity_id=u32(40), material_id=u32(40), actor=ActorHandle(id=u32(40), generation=u32(0)))
}

region inside_region() {
    place inside = inside_shape
}

region tangent_region() {
    place tangent = tangent_shape
}

region translated_region() {
    place translated = translated_shape
}

region scaled_region() {
    place scaled = scaled_shape
}

region mirrored_region() {
    place mirrored = mirrored_shape
}

region repeat_linear_region() {
    place repeated = repeat_linear_shape
}

region repeat_grid_region() {
    place repeated = repeat_grid_shape
}

region radial_repeat_region() {
    place repeated = radial_repeat_shape
}

region miss_region() {
    place miss = miss_shape
}

region mixed_repeat_region() {
    place distractor = distractor_shape
    place repeated = repeat_linear_shape
}

domain scene_domain(world: RegionCapture) {
    geometry_detail = 1
    material = false
    radiance = false
    media = false
    max_distance = 10.0
    min_step = 0.05
    hit_epsilon = 0.001
    max_steps = 96
}
"#
}

pub(super) fn transformed_analytic_primitives_fixture_source() -> &'static str {
    r#"
field exact distance translated_box_field(p: Vec3) -> F32 {
    translate = vec3(1.25, -0.10, 0.0) {
        box(half = vec3(0.35, 0.45, 0.25))
    }
}

field exact distance rotated_capsule_field(p: Vec3) -> F32 {
    rotate = vec3(0.55, 0.0, 0.0) {
        capsule(a = vec3(0.0, -0.8, 0.0), b = vec3(0.0, 0.8, 0.0), radius = 0.35)
    }
}

field exact distance rotated_cylinder_field(p: Vec3) -> F32 {
    rotate = vec3(0.55, 0.0, 0.0) {
        cylinder(radius = 0.45, half_height = 0.9)
    }
}

field exact distance scaled_sphere_field(p: Vec3) -> F32 {
    uniform_scale = f32(2.0) {
        sphere(radius = 0.5)
    }
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

shape translated_box_shape {
    field = translated_box_field
    material = shade
    payload = Payload(entity_id=u32(71), material_id=u32(71), actor=ActorHandle(id=u32(71), generation=u32(0)))
}

shape rotated_capsule_shape {
    field = rotated_capsule_field
    material = shade
    payload = Payload(entity_id=u32(72), material_id=u32(72), actor=ActorHandle(id=u32(72), generation=u32(0)))
}

shape rotated_cylinder_shape {
    field = rotated_cylinder_field
    material = shade
    payload = Payload(entity_id=u32(73), material_id=u32(73), actor=ActorHandle(id=u32(73), generation=u32(0)))
}

shape scaled_sphere_shape {
    field = scaled_sphere_field
    material = shade
    payload = Payload(entity_id=u32(74), material_id=u32(74), actor=ActorHandle(id=u32(74), generation=u32(0)))
}

region translated_box_region() {
    place primary = translated_box_shape
}

region rotated_capsule_region() {
    place primary = rotated_capsule_shape
}

region rotated_cylinder_region() {
    place primary = rotated_cylinder_shape
}

region scaled_sphere_region() {
    place primary = scaled_sphere_shape
}

domain scene_domain(world: RegionCapture) {
    geometry_detail = 1
    material = false
    radiance = false
    media = false
    max_distance = 8.0
    min_step = 0.02
    hit_epsilon = 0.001
    max_steps = 128
}
"#
}

pub(super) fn relaxed_torus_solver_fixture_source() -> &'static str {
    r#"
field exact distance relaxed_torus_field(p: Vec3) -> F32 {
    torus(major_radius = 0.72, minor_radius = 0.22)
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

shape relaxed_torus_shape {
    field = relaxed_torus_field
    material = shade
    payload = Payload(entity_id=u32(81), material_id=u32(81), actor=ActorHandle(id=u32(81), generation=u32(0)))
}

region relaxed_torus_region() {
    place primary = relaxed_torus_shape
}

domain scene_domain(world: RegionCapture) {
    geometry_detail = 1
    material = false
    radiance = false
    media = false
    max_distance = 6.0
    min_step = 0.01
    hit_epsilon = 0.0005
    max_steps = 192
}
"#
}

pub(super) fn translated_repeat_linear_solver_fixture_source() -> &'static str {
    r#"
field conservative distance translated_repeat_field(p: Vec3) -> F32 {
    translate = vec3(1.5, -0.10, 0.0) {
        repeat_linear = vec3(12.0, 0.0, 0.0) {
            translate = vec3(6.0, 0.0, 0.0) {
                sphere(radius = 0.18)
            }
        }
    }
}

material shade(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.2, 0.3, 0.4),
        roughness=0.5,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape translated_repeat_shape {
    field = translated_repeat_field
    material = shade
    payload = Payload(
        entity_id=u32(89),
        material_id=u32(89),
        actor=ActorHandle(id=u32(89), generation=u32(0))
    )
}

region translated_repeat_region() {
    place repeated = translated_repeat_shape
}

domain scene_domain(world: RegionCapture) {
    geometry_detail = 1
    material = false
    radiance = false
    media = false
    max_distance = 30.0
    min_step = 0.02
    hit_epsilon = 0.001
    max_steps = 256
}
"#
}

pub(super) fn large_union_distance_fixture_source() -> &'static str {
    r#"
field exact distance leaf_0_field(p: Vec3) -> F32 {
    translate = vec3(0.0, 0.0, 0.0) {
        sphere(radius = 0.5)
    }
}

field exact distance leaf_1_field(p: Vec3) -> F32 {
    translate = vec3(5.0, 0.0, 0.0) {
        sphere(radius = 0.5)
    }
}

field exact distance leaf_2_field(p: Vec3) -> F32 {
    translate = vec3(10.0, 0.0, 0.0) {
        sphere(radius = 0.5)
    }
}

field exact distance leaf_3_field(p: Vec3) -> F32 {
    translate = vec3(15.0, 0.0, 0.0) {
        sphere(radius = 0.5)
    }
}

field exact distance leaf_4_field(p: Vec3) -> F32 {
    translate = vec3(20.0, 0.0, 0.0) {
        sphere(radius = 0.5)
    }
}

field exact distance leaf_5_field(p: Vec3) -> F32 {
    translate = vec3(25.0, 0.0, 0.0) {
        sphere(radius = 0.5)
    }
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

shape leaf_0_shape {
    field = leaf_0_field
    material = shade
    payload = Payload(entity_id=u32(101), material_id=u32(101), actor=ActorHandle(id=u32(101), generation=u32(0)))
}

shape leaf_1_shape {
    field = leaf_1_field
    material = shade
    payload = Payload(entity_id=u32(102), material_id=u32(102), actor=ActorHandle(id=u32(102), generation=u32(0)))
}

shape leaf_2_shape {
    field = leaf_2_field
    material = shade
    payload = Payload(entity_id=u32(103), material_id=u32(103), actor=ActorHandle(id=u32(103), generation=u32(0)))
}

shape leaf_3_shape {
    field = leaf_3_field
    material = shade
    payload = Payload(entity_id=u32(104), material_id=u32(104), actor=ActorHandle(id=u32(104), generation=u32(0)))
}

shape leaf_4_shape {
    field = leaf_4_field
    material = shade
    payload = Payload(entity_id=u32(105), material_id=u32(105), actor=ActorHandle(id=u32(105), generation=u32(0)))
}

shape leaf_5_shape {
    field = leaf_5_field
    material = shade
    payload = Payload(entity_id=u32(106), material_id=u32(106), actor=ActorHandle(id=u32(106), generation=u32(0)))
}

shape large_union_shape {
    union {
        provenance_policy = nearest
        use leaf_0_shape
        use leaf_1_shape
        use leaf_2_shape
        use leaf_3_shape
        use leaf_4_shape
        use leaf_5_shape
    }
}
"#
}

pub(super) fn ray_solver_opaque_fixture_source() -> &'static str {
    r#"
field conservative distance opaque_field(p: Vec3) -> F32 {
    support = Support3(bounds=Bounds3(
        min=vec3(-0.6, -0.6, -0.6),
        max=vec3(0.6, 0.6, 0.6)
    ))
    bounds = Bounds3(
        min=vec3(-0.6, -0.6, -0.6),
        max=vec3(0.6, 0.6, 0.6)
    )
    return length(p) - 0.6
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

shape opaque_shape {
    field = opaque_field
    material = shade
    payload = Payload(entity_id=u32(7), material_id=u32(8), actor=ActorHandle(id=u32(9), generation=u32(0)))
}

region scene_region() {
    place opaque = opaque_shape
}

domain scene_domain(world: RegionCapture) {
    geometry_detail = 1
    material = false
    radiance = false
    media = false
    max_distance = 6.0
    min_step = 0.05
    hit_epsilon = 0.001
    max_steps = 96
}
"#
}

fn wide_world_overflow_fixture_source(shape_count: usize) -> String {
    let mut source = String::new();
    for index in 0..shape_count {
        let x = index as f32 * 2.0;
        writeln!(
            source,
            "field exact distance leaf_{index}_field(p: Vec3) -> F32 {{
    translate = vec3({x:.1}, 0.0, 0.0) {{
        sphere(radius = 0.45)
    }}
}}
"
        )
        .expect("append field");
    }
    source.push_str(
        r#"
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

"#,
    );
    for index in 0..shape_count {
        let entity_id = 10_000 + index as u32;
        writeln!(
            source,
            "shape leaf_{index}_shape {{
    field = leaf_{index}_field
    material = shade
    payload = Payload(
        entity_id=u32({entity_id}),
        material_id=u32({entity_id}),
        actor=ActorHandle(id=u32({entity_id}), generation=u32(0))
    )
}}
"
        )
        .expect("append shape");
    }
    source.push_str("region overflow_region() {\n");
    for index in 0..shape_count {
        writeln!(source, "    place leaf_{index} = leaf_{index}_shape").expect("append place");
    }
    source.push_str(
        r#"}

domain overflow_domain(world: RegionCapture) {
    geometry_detail = 1
    material = false
    radiance = false
    media = false
    max_distance = 6.0
    min_step = 0.02
    hit_epsilon = 0.001
    max_steps = 128
}
"#,
    );
    source
}

fn accelerated_world_helper_fixture_source() -> &'static str {
    r#"
field exact distance near_field(p: Vec3) -> F32 {
    translate = vec3(-6.0, 0.0, 0.0) {
        sphere(radius = 0.45)
    }
}

field exact distance mid_a_field(p: Vec3) -> F32 {
    translate = vec3(-3.0, 0.0, 0.0) {
        sphere(radius = 0.45)
    }
}

field exact distance mid_b_field(p: Vec3) -> F32 {
    translate = vec3(0.0, 0.0, 0.0) {
        sphere(radius = 0.45)
    }
}

field exact distance focus_field(p: Vec3) -> F32 {
    translate = vec3(6.0, 0.0, 0.0) {
        sphere(radius = 0.45)
    }
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

radiance field glow(p: Vec3, direction: Vec3, feature_id: U32) -> Vec3 {
    return vec3(0.1, 0.2, 0.3) + direction * 0.0 + vec3(f32(feature_id) * 0.0, 0.0, 0.0)
}

volume field fog(p: Vec3, surface_distance: F32) -> Medium {
    return Medium(
        density=0.2,
        emission=vec3(0.05, 0.06, 0.07) + vec3(abs(surface_distance) * 0.0, 0.0, 0.0),
        anisotropy=0.1
    )
}

shape near_shape {
    field = near_field
    material = shade
    radiance = glow
    volume = fog
    payload = Payload(
        entity_id=u32(101),
        material_id=u32(201),
        actor=ActorHandle(id=u32(301), generation=u32(0))
    )
}

shape mid_a_shape {
    field = mid_a_field
    material = shade
    radiance = glow
    volume = fog
    payload = Payload(
        entity_id=u32(102),
        material_id=u32(202),
        actor=ActorHandle(id=u32(302), generation=u32(0))
    )
}

shape mid_b_shape {
    field = mid_b_field
    material = shade
    radiance = glow
    volume = fog
    payload = Payload(
        entity_id=u32(103),
        material_id=u32(203),
        actor=ActorHandle(id=u32(303), generation=u32(0))
    )
}

shape focus_shape {
    field = focus_field
    material = shade
    radiance = glow
    volume = fog
    payload = Payload(
        entity_id=u32(104),
        material_id=u32(204),
        actor=ActorHandle(id=u32(304), generation=u32(0))
    )
}

region accelerated_region() {
    place near = near_shape
    place mid_a = mid_a_shape
    place mid_b = mid_b_shape
    place focus = focus_shape
}

domain accelerated_domain(world: RegionCapture) {
    geometry_detail = 1
    material = true
    radiance = true
    media = true
    max_distance = 12.0
    min_step = 0.05
    hit_epsilon = 0.001
    max_steps = 96
}
"#
}

#[test]
fn query_exec_wgsl_world_trace_dense_fallbacks_when_accel_stack_overflows() {
    let shape_count = 160usize;
    let target_index = shape_count - 1;
    let target_x = target_index as f32 * 2.0;
    let source = wide_world_overflow_fixture_source(shape_count);
    let (_, _, ctx) = typed_query_module(&source);
    let region_name = SmolStr::new("overflow_region");
    let region_scene_id = stable_region_scene_capture_id(&region_name);
    let args = [
        KernelValue::Capture(region_name.clone()),
        scene_domain(region_scene_id, 1, false, false, false),
        ray_query_with_limits(
            [target_x, 0.0, 3.0],
            [0.0, 0.0, -1.0],
            6.0,
            0.02,
            0.001,
            128,
        ),
    ];
    let plan = lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Trace));

    let (cpu_hit, _) = execute_world_query_with_trace_on(&ctx, DispatchBackend::Cpu, &plan, &args)
        .expect("cpu overflow trace");
    let (wgsl_hit, wgsl_trace) =
        execute_world_query_with_trace_on(&ctx, DispatchBackend::Wgsl, &plan, &args)
            .expect("wgsl overflow trace");

    let cpu_payload = expect_struct(field(expect_struct(&cpu_hit, "Hit3"), "payload"), "Payload");
    let wgsl_payload = expect_struct(
        field(expect_struct(&wgsl_hit, "Hit3"), "payload"),
        "Payload",
    );
    assert_eq!(
        expect_u32(field(cpu_payload, "entity_id")),
        10_000 + target_index as u32
    );
    assert_eq!(
        expect_u32(field(wgsl_payload, "entity_id")),
        10_000 + target_index as u32
    );
    assert!(
        wgsl_trace
            .observability
            .solver_generated_dense_fallback_rays
            > 0
    );
    assert!(wgsl_trace.observability.cache_dense_fallback_rays > 0);
}

#[test]
fn query_exec_wgsl_world_distance_prefers_accelerated_helpers_when_available() {
    let _lock = wgsl_resident_cache_test_lock();
    clear_native_wgsl_test_caches();
    let (_, _, ctx) = typed_query_module(accelerated_world_helper_fixture_source());
    let region_name = SmolStr::new("accelerated_region");
    let region_scene_id = stable_region_scene_capture_id(&region_name);
    let domain = scene_domain(region_scene_id, 1, true, true, true);
    let distance_plan =
        lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Distance));
    let radiance_plan =
        lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Radiance));
    let medium_plan = lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Medium));
    let sample_point = KernelValue::Vec3([6.45, 0.0, 0.0]);
    let distance_args = [
        KernelValue::Capture(region_name.clone()),
        domain.clone(),
        sample_point.clone(),
    ];

    let (cpu_distance, _) = execute_world_query_with_trace_on(
        &ctx,
        DispatchBackend::Cpu,
        &distance_plan,
        &distance_args,
    )
    .expect("cpu accelerated distance");
    let (wgsl_distance, distance_trace) = execute_world_query_with_trace_on(
        &ctx,
        DispatchBackend::Wgsl,
        &distance_plan,
        &distance_args,
    )
    .expect("wgsl accelerated distance");
    assert_approx_eq(expect_f32(&cpu_distance), expect_f32(&wgsl_distance));
    assert!(distance_trace.observability.acceleration_node_visits > 0);
    assert!(distance_trace.observability.shape_leaf_visits > 0);
    assert_eq!(distance_trace.observability.cache_budget_rejections, 0);

    let radiance_args = [
        KernelValue::Capture(region_name.clone()),
        domain.clone(),
        point_direction_query([6.45, 0.0, 0.0], [0.0, 0.0, 1.0]),
    ];
    let (cpu_radiance, _) = execute_world_query_with_trace_on(
        &ctx,
        DispatchBackend::Cpu,
        &radiance_plan,
        &radiance_args,
    )
    .expect("cpu accelerated radiance");
    let (wgsl_radiance, radiance_trace) = execute_world_query_with_trace_on(
        &ctx,
        DispatchBackend::Wgsl,
        &radiance_plan,
        &radiance_args,
    )
    .expect("wgsl accelerated radiance");
    assert_vec3_approx_eq(expect_vec3(&cpu_radiance), expect_vec3(&wgsl_radiance));
    assert!(radiance_trace.observability.acceleration_node_visits > 0);
    assert!(radiance_trace.observability.shape_leaf_visits > 0);
    assert_eq!(radiance_trace.observability.cache_budget_rejections, 0);

    let medium_args = [KernelValue::Capture(region_name), domain, sample_point];
    let (cpu_medium, _) =
        execute_world_query_with_trace_on(&ctx, DispatchBackend::Cpu, &medium_plan, &medium_args)
            .expect("cpu accelerated medium");
    let (wgsl_medium, medium_trace) =
        execute_world_query_with_trace_on(&ctx, DispatchBackend::Wgsl, &medium_plan, &medium_args)
            .expect("wgsl accelerated medium");
    assert_medium_approx_eq(&cpu_medium, &wgsl_medium);
    assert!(medium_trace.observability.acceleration_node_visits > 0);
    assert!(medium_trace.observability.shape_leaf_visits > 0);
    assert_eq!(medium_trace.observability.cache_budget_rejections, 0);

    let rendered_distance = render_semantic_cost_report(&distance_trace.cost_report);
    assert!(rendered_distance.contains("acceleration_node_visits="));
    assert!(rendered_distance.contains("acceleration_pruned_nodes="));
    assert!(rendered_distance.contains("wgsl_world_helper_path=accelerated"));
    let rendered_radiance = render_semantic_cost_report(&radiance_trace.cost_report);
    assert!(rendered_radiance.contains("acceleration_node_visits="));
    assert!(rendered_radiance.contains("wgsl_world_helper_path=accelerated"));
    let rendered_medium = render_semantic_cost_report(&medium_trace.cost_report);
    assert!(rendered_medium.contains("acceleration_node_visits="));
    assert!(rendered_medium.contains("wgsl_world_helper_path=accelerated"));
}

#[test]
fn query_exec_wgsl_participant_batches_prune_bounded_world_support() {
    let _lock = wgsl_resident_cache_test_lock();
    clear_native_wgsl_test_caches();
    let (_, _, ctx) = typed_query_module(accelerated_world_helper_fixture_source());
    let region_name = SmolStr::new("accelerated_region");
    let region_scene_id = stable_region_scene_capture_id(&region_name);
    let domain = scene_domain(region_scene_id, 1, true, true, true);
    let scene_summary = ctx
        .region_scene_summary(&region_name, 1)
        .expect("bounded region summary");
    let radiance_plan = lower_batch_query_plan(
        &BatchQueryPlan::for_contract(
            query_contract::PARTICIPANTS_RADIANCE_BATCH_WORLD,
            DispatchBackend::Wgsl,
            Some(scene_summary.clone()),
        )
        .expect("radiance batch plan"),
    );
    let medium_plan = lower_batch_query_plan(
        &BatchQueryPlan::for_contract(
            query_contract::PARTICIPANTS_MEDIUM_BATCH_WORLD,
            DispatchBackend::Wgsl,
            Some(scene_summary),
        )
        .expect("medium batch plan"),
    );
    assert_eq!(
        radiance_plan.pruning_strategy,
        PruningStrategy::ConservativeTraversal
    );
    assert_eq!(
        medium_plan.pruning_strategy,
        PruningStrategy::ConservativeTraversal
    );
    let pruned_point = [30.0, 0.0, 0.0];

    let radiance_args = [
        KernelValue::Capture(region_name.clone()),
        domain.clone(),
        KernelValue::Array(vec![point_direction_query(pruned_point, [0.0, 0.0, 1.0])]),
    ];
    let (cpu_radiance, _) = execute_batch_query_with_trace_on(
        &ctx,
        DispatchBackend::Cpu,
        &radiance_plan,
        &radiance_args,
    )
    .expect("cpu radiance batch");
    let (wgsl_radiance, wgsl_radiance_trace) = execute_batch_query_with_trace_on(
        &ctx,
        DispatchBackend::Wgsl,
        &radiance_plan,
        &radiance_args,
    )
    .expect("wgsl radiance batch");
    assert_eq!(cpu_radiance, wgsl_radiance);
    assert!(wgsl_radiance_trace.observability.acceleration_node_visits > 0);
    assert!(wgsl_radiance_trace.observability.shape_leaf_visits > 0);
    assert_eq!(
        wgsl_radiance_trace.observability.acceleration_pruned_nodes,
        0
    );
    let rendered_radiance = render_semantic_cost_report(&wgsl_radiance_trace.cost_report);
    assert!(rendered_radiance.contains("pruning_strategy=conservative-traversal"));
    assert!(rendered_radiance.contains("wgsl_world_helper_path=accelerated"));

    let medium_args = [
        KernelValue::Capture(region_name),
        domain,
        KernelValue::Array(vec![point_query(pruned_point)]),
    ];
    let (cpu_medium, _) =
        execute_batch_query_with_trace_on(&ctx, DispatchBackend::Cpu, &medium_plan, &medium_args)
            .expect("cpu medium batch");
    let (wgsl_medium, wgsl_medium_trace) =
        execute_batch_query_with_trace_on(&ctx, DispatchBackend::Wgsl, &medium_plan, &medium_args)
            .expect("wgsl medium batch");
    assert_eq!(cpu_medium, wgsl_medium);
    assert!(wgsl_medium_trace.observability.acceleration_node_visits > 0);
    assert!(wgsl_medium_trace.observability.shape_leaf_visits > 0);
    assert_eq!(wgsl_medium_trace.observability.acceleration_pruned_nodes, 0);
    let rendered_medium = render_semantic_cost_report(&wgsl_medium_trace.cost_report);
    assert!(rendered_medium.contains("pruning_strategy=conservative-traversal"));
    assert!(rendered_medium.contains("wgsl_world_helper_path=accelerated"));
}

#[test]
fn query_exec_wgsl_world_distance_keeps_parity_on_wide_worlds() {
    let shape_count = 160usize;
    let target_index = shape_count - 1;
    let target_x = target_index as f32 * 2.0;
    let source = wide_world_overflow_fixture_source(shape_count);
    let (_, _, ctx) = typed_query_module(&source);
    let region_name = SmolStr::new("overflow_region");
    let region_scene_id = stable_region_scene_capture_id(&region_name);
    let distance_plan =
        lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Distance));
    let args = [
        KernelValue::Capture(region_name),
        scene_domain(region_scene_id, 1, false, false, false),
        KernelValue::Vec3([target_x, 0.0, 0.0]),
    ];

    let (cpu_distance, _) =
        execute_world_query_with_trace_on(&ctx, DispatchBackend::Cpu, &distance_plan, &args)
            .expect("cpu wide-world distance");
    let (wgsl_distance, wgsl_trace) =
        execute_world_query_with_trace_on(&ctx, DispatchBackend::Wgsl, &distance_plan, &args)
            .expect("wgsl wide-world distance");

    assert_approx_eq(expect_f32(&cpu_distance), expect_f32(&wgsl_distance));
    assert!(
        wgsl_trace.observability.acceleration_node_visits > 0
            || wgsl_trace.observability.shape_leaf_visits > 0
    );
    let rendered = render_semantic_cost_report(&wgsl_trace.cost_report);
    assert!(rendered.contains("cache_budget_rejections="));
    assert!(rendered.contains("wgsl_world_helper_path=accelerated"));
}
