use smol_str::SmolStr;
use wrela::artifact_key::ArtifactReuseKey;
use wrela::artifact_store::{ArtifactInstanceMetadata, ArtifactLookupRequest, StoredArtifact};
use wrela::collision_contract::{
    CollisionContactNormalFlavor, CollisionContactNormalProvenance, CollisionResult,
};
use wrela::collision_exec::cpu::{
    CollisionArtifactPayload, CollisionArtifactStore, CollisionContinuationSeed,
    CollisionStoredWitness, execute_with_store,
};
use wrela::collision_plan::{
    CollisionArtifactKind, CollisionPlan, CollisionQueryKind, collision_history_compatibility_hash,
};
use wrela::hir;
use wrela::hir::lower as hir_lower;
use wrela::kernel::{KernelStructValue, KernelValue};
use wrela::parser::ast;
use wrela::parser::ast::AstNode;
use wrela::parser::parse;
use wrela::query_exec::{QueryExecContext, stable_region_scene_capture_id};
use wrela::query_solver::{
    CertificateReuseClass, RayStepCertificate, RayStepCertificateMetadata,
    RayStepCertificateSubjectKind, RequiredGuaranteeClass, StepCertificateKind,
};
use wrela::state_advance::ChangeClass;
use wrela::world_identity::SnapshotEpoch;

fn lower_inline_module_from_source(source: &str) -> hir::Module {
    let node = parse(source);
    let root = ast::Root::cast(node).expect("root");
    hir_lower::lower(root)
}

fn typed_query_module(source: &str) -> QueryExecContext {
    let module = lower_inline_module_from_source(source);
    let semantic = hir::semantic::check_module(&module);
    assert!(
        semantic.errors.is_empty(),
        "semantic errors: {:?}",
        semantic.errors
    );
    let (type_errors, type_info) = hir::typeck::check_module_with_info(&module);
    assert!(type_errors.is_empty(), "type errors: {type_errors:?}");
    QueryExecContext::compile(&module, &type_info)
}

fn collision_fixture_source() -> &'static str {
    r#"
field exact distance collision_field(p: Vec3) -> F32 {
    sphere(radius = 0.5)
}

material collision_surface(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.8, 0.3, 0.2),
        roughness=0.2,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape collision_shape {
    field = collision_field
    material = collision_surface
}

region collision_region() {
    place sample = collision_shape
}

domain collision_domain(world: RegionCapture) {
    geometry_detail = 1
    material = true
    radiance = false
    media = false
    max_distance = 6.0
    min_step = 0.05
    hit_epsilon = 0.001
    max_steps = 96
}

domain collision_domain_coarse(world: RegionCapture) {
    geometry_detail = 0
    material = true
    radiance = false
    media = false
    max_distance = 6.0
    min_step = 0.05
    hit_epsilon = 0.001
    max_steps = 96
}
"#
}

fn collision_clutter_fixture_source() -> &'static str {
    r#"
field exact distance collision_center_field(p: Vec3) -> F32 {
    sphere(radius = 0.5)
}

field exact distance collision_left_field(p: Vec3) -> F32 {
    translate = vec3(-4.0, 0.0, 0.0) {
        sphere(radius = 0.5)
    }
}

field exact distance collision_right_field(p: Vec3) -> F32 {
    translate = vec3(4.0, 0.0, 0.0) {
        sphere(radius = 0.5)
    }
}

material collision_surface(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.8, 0.3, 0.2),
        roughness=0.2,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape collision_center_shape {
    field = collision_center_field
    material = collision_surface
}

shape collision_left_shape {
    field = collision_left_field
    material = collision_surface
}

shape collision_right_shape {
    field = collision_right_field
    material = collision_surface
}

region collision_clutter_region() {
    place center = collision_center_shape
    place left = collision_left_shape
    place right = collision_right_shape
}

domain collision_domain(world: RegionCapture) {
    geometry_detail = 1
    material = true
    radiance = false
    media = false
    max_distance = 6.0
    min_step = 0.05
    hit_epsilon = 0.001
    max_steps = 96
}
"#
}

fn scene_domain(scene_id: u32) -> KernelValue {
    scene_domain_with_detail(scene_id, 1)
}

fn scene_domain_with_detail(scene_id: u32, geometry_detail: i32) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("SceneDomain"),
        fields: vec![
            (SmolStr::new("scene_id"), KernelValue::U32(scene_id)),
            (
                SmolStr::new("spatial"),
                KernelValue::Struct(KernelStructValue {
                    name: SmolStr::new("SpatialDomainContract"),
                    fields: vec![(
                        SmolStr::new("geometry_detail"),
                        KernelValue::I32(geometry_detail),
                    )],
                }),
            ),
            (
                SmolStr::new("surface"),
                KernelValue::Struct(KernelStructValue {
                    name: SmolStr::new("SurfaceDomainContract"),
                    fields: vec![(SmolStr::new("material"), KernelValue::Bool(true))],
                }),
            ),
            (
                SmolStr::new("participants"),
                KernelValue::Struct(KernelStructValue {
                    name: SmolStr::new("ParticipantDomainContract"),
                    fields: vec![
                        (SmolStr::new("radiance"), KernelValue::Bool(false)),
                        (SmolStr::new("media"), KernelValue::Bool(false)),
                    ],
                }),
            ),
        ],
    })
}

fn region_capture(scene_id: u32, epoch: u32) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("RegionCapture"),
        fields: vec![
            (SmolStr::new("scene_id"), KernelValue::U32(scene_id)),
            (SmolStr::new("epoch"), KernelValue::U32(epoch)),
        ],
    })
}

fn collision_transition_input(
    current_epoch: u32,
    previous_epoch: u32,
    change_class: ChangeClass,
) -> KernelValue {
    let change_class_id = match change_class {
        ChangeClass::None => 0,
        ChangeClass::Presentation => 1,
        ChangeClass::Structural => 2,
        ChangeClass::Topology => 3,
        ChangeClass::Identity => 4,
        ChangeClass::Behavior => 5,
        ChangeClass::Incompatible => 6,
    };
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("CollisionSnapshotTransitionInput"),
        fields: vec![
            (
                SmolStr::new("current_snapshot_epoch"),
                KernelValue::U32(current_epoch),
            ),
            (
                SmolStr::new("previous_snapshot_epoch"),
                KernelValue::U32(previous_epoch),
            ),
            (
                SmolStr::new("change_class"),
                KernelValue::U32(change_class_id),
            ),
        ],
    })
}

fn collision_point_input(point: [f32; 3]) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("CollisionPointInput"),
        fields: vec![(SmolStr::new("point"), KernelValue::Vec3(point))],
    })
}

fn collision_ray_input(origin: [f32; 3], direction: [f32; 3]) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("CollisionRayInput"),
        fields: vec![
            (SmolStr::new("origin"), KernelValue::Vec3(origin)),
            (SmolStr::new("direction"), KernelValue::Vec3(direction)),
            (SmolStr::new("max_distance"), KernelValue::F32(6.0)),
            (SmolStr::new("min_step"), KernelValue::F32(0.05)),
            (SmolStr::new("hit_epsilon"), KernelValue::F32(0.001)),
            (SmolStr::new("max_steps"), KernelValue::I32(96)),
        ],
    })
}

fn collision_sphere_probe(center: [f32; 3], radius: f32) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("CollisionSphereProbe"),
        fields: vec![
            (SmolStr::new("center"), KernelValue::Vec3(center)),
            (SmolStr::new("radius"), KernelValue::F32(radius)),
        ],
    })
}

fn collision_sweep_input(start_center: [f32; 3], end_center: [f32; 3], radius: f32) -> KernelValue {
    collision_sweep_input_with_iterations(start_center, end_center, radius, 64)
}

fn collision_sweep_input_with_iterations(
    start_center: [f32; 3],
    end_center: [f32; 3],
    radius: f32,
    max_iterations: i32,
) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("CollisionSphereSweepInput"),
        fields: vec![
            (
                SmolStr::new("start_center"),
                KernelValue::Vec3(start_center),
            ),
            (SmolStr::new("end_center"), KernelValue::Vec3(end_center)),
            (SmolStr::new("radius"), KernelValue::F32(radius)),
            (SmolStr::new("contact_tolerance"), KernelValue::F32(0.001)),
            (
                SmolStr::new("max_iterations"),
                KernelValue::I32(max_iterations),
            ),
        ],
    })
}

fn policy_digest(policy: wrela::collision_contract::CollisionExecutionPolicy) -> u64 {
    let backend_tag = [match policy.backend_preference {
        wrela::query_contract::DispatchBackend::Cpu => 0,
        wrela::query_contract::DispatchBackend::VirtualGpu => 1,
        wrela::query_contract::DispatchBackend::Wgsl => 2,
        wrela::query_contract::DispatchBackend::Auto => 3,
    }];
    wrela::query_exec::ids::stable_semantic_id(&[
        &policy.required_guarantee.id().to_le_bytes(),
        &policy.selected_method.id().to_le_bytes(),
        &backend_tag,
    ])
}

fn transition_certificate(reusable_by: CertificateReuseClass) -> RayStepCertificate {
    RayStepCertificate {
        kind: StepCertificateKind::RefinementBracket,
        metadata: RayStepCertificateMetadata {
            guarantee: RequiredGuaranteeClass::ConservativeNoFalseMiss,
            proof_family: SmolStr::new("collision.transition"),
            subject: SmolStr::new("collision.test"),
            subject_kind: RayStepCertificateSubjectKind::Interval,
            tolerance_context: SmolStr::new("collision test certificate"),
            reusable_by,
            invalidation_reasons: vec![SmolStr::new("collision test changed")],
        },
        t_start: 0.25,
        t_end: 0.3125,
        no_hit_before_t_end: true,
        bracket: Some([0.25, 0.3125]),
        provenance: None,
    }
}

fn assert_approx_eq(lhs: f32, rhs: f32) {
    assert!(
        (lhs - rhs).abs() < 0.02,
        "expected {lhs} ~= {rhs}, delta={}",
        (lhs - rhs).abs()
    );
}

