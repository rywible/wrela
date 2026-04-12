use smol_str::SmolStr;
use wrela::artifact_key::ArtifactPolicyDigestMode;
use wrela::hir;
use wrela::hir::lower as hir_lower;
use wrela::kernel::{KernelStructValue, KernelValue, lower_world_query_plan};
use wrela::parser::ast;
use wrela::parser::ast::AstNode;
use wrela::parser::parse;
use wrela::presentation_contract::{
    CanonicalCameraInput, CanonicalLightInput, CanonicalViewportInput, PresentationLightingInputs,
};
use wrela::presentation_exec::{
    PresentationExecutionInput, PresentationExecutionPolicy, RayBudgetPolicy, execute_plan,
    frame_state_value, scene_domain_value,
};
use wrela::presentation_plan::PresentationPlan;
use wrela::query_exec::{
    QueryExecContext, QueryExecError, execute_world_query_with_snapshot_on,
    execute_world_query_with_trace_on, stable_region_snapshot_handle, stable_shape_snapshot_handle,
};
use wrela::query_plan::{DispatchBackend, WorldQueryKind, WorldQueryPlan};
use wrela::world_identity::{SnapshotEpoch, WorldSnapshotHandle};

fn lower_inline_module(source: &str) -> hir::Module {
    let node = parse(source);
    let root = ast::Root::cast(node).expect("root");
    hir_lower::lower(root)
}

fn typed_module(source: &str) -> (hir::Module, hir::TypeInfo, QueryExecContext) {
    let module = lower_inline_module(source);
    let semantic = hir::semantic::check_module(&module);
    assert!(
        semantic.errors.is_empty(),
        "semantic errors: {:?}",
        semantic.errors
    );
    let (type_errors, type_info) = hir::typeck::check_module_with_info(&module);
    assert!(type_errors.is_empty(), "type errors: {type_errors:?}");
    let ctx = QueryExecContext::compile(&module, &type_info);
    (module, type_info, ctx)
}

fn view_function<'a>(module: &'a hir::Module, name: &str) -> &'a hir::Function {
    module
        .functions
        .iter()
        .find(|(_, func)| func.name == name)
        .map(|(_, func)| func)
        .unwrap_or_else(|| panic!("missing view function '{name}'"))
}

fn presentation_execution_policy() -> PresentationExecutionPolicy {
    PresentationExecutionPolicy::conservative(RayBudgetPolicy {
        max_distance: 8.0,
        min_step: 0.02,
        hit_epsilon: 0.0005,
        max_steps: 128,
    })
}

fn snapshot_identity_source() -> &'static str {
    r#"
field exact distance snapshot_field(p: Vec3) -> F32 {
    sphere(radius = 0.6)
}

material snapshot_material(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.7, 0.4, 0.2),
        roughness=0.35,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape snapshot_shape {
    field = snapshot_field
    material = snapshot_material
    payload = Payload(
        entity_id=u32(7),
        material_id=u32(9),
        actor=ActorHandle(id=u32(11), generation=u32(0))
    )
}

region snapshot_region() {
    place scene = snapshot_shape
}

domain snapshot_domain(world: RegionCapture) {
    geometry_detail = 1
    material = true
    radiance = false
    media = false
    max_distance = 8.0
    min_step = 0.02
    hit_epsilon = 0.0005
    max_steps = 128
}

view snapshot_view(world: RegionCapture, camera: Camera) {
    domain = snapshot_domain(world = world)
    width = 4
    height = 4
    key_light = Light(
        position = vec3(1.8, 2.4, 2.2),
        direction = normalize(vec3(-0.5, -0.8, -0.6)),
        intensity = vec3(1.0, 0.98, 0.95),
        range = 8.0
    )
    fill_direction = normalize(vec3(-0.7, 0.45, 0.2))
    fill_strength = 0.22
    ambient_color = vec3(0.12, 0.12, 0.12)
}
"#
}

fn ray_query(origin: [f32; 3], direction: [f32; 3]) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("RayQuery"),
        fields: vec![
            (SmolStr::new("origin"), KernelValue::Vec3(origin)),
            (SmolStr::new("direction"), KernelValue::Vec3(direction)),
            (SmolStr::new("max_distance"), KernelValue::F32(8.0)),
            (SmolStr::new("min_step"), KernelValue::F32(0.02)),
            (SmolStr::new("hit_epsilon"), KernelValue::F32(0.0005)),
            (SmolStr::new("max_steps"), KernelValue::I32(128)),
        ],
    })
}

fn presentation_input(
    region_snapshot: WorldSnapshotHandle,
    scene_id: u32,
    history: Option<wrela::presentation_exec::PresentationTemporalHistory>,
) -> PresentationExecutionInput {
    PresentationExecutionInput {
        region_snapshot,
        frame_domain: scene_domain_value(scene_id, 1, true, false, false),
        frame_state: frame_state_value(
            CanonicalCameraInput {
                position: [0.0, 0.0, 2.0],
                forward: [0.0, 0.0, -1.0],
                up: [0.0, 1.0, 0.0],
                vertical_fov_degrees: 75.0,
            },
            CanonicalCameraInput {
                position: [0.0, 0.0, 2.0],
                forward: [0.0, 0.0, -1.0],
                up: [0.0, 1.0, 0.0],
                vertical_fov_degrees: 75.0,
            },
            CanonicalViewportInput {
                width: 4,
                height: 4,
            },
            [0.0, 0.0],
            0,
            1.0 / 60.0,
        ),
        history,
        lighting: PresentationLightingInputs {
            key_light: CanonicalLightInput {
                position: [1.8, 2.4, 2.2],
                direction: [-0.5, -0.8, -0.6],
                intensity: [1.0, 0.98, 0.95],
                range: 8.0,
            },
            fill_direction: [-0.7, 0.45, 0.2],
            fill_strength: 0.22,
            ambient_color: [0.12, 0.12, 0.12],
        },
        compatibility_projection: None,
        execution_policy: presentation_execution_policy(),
        quality_override: None,
        backend: DispatchBackend::Cpu,
    }
}

#[test]
fn snapshot_handles_are_deterministic_and_layered() {
    let left = stable_shape_snapshot_handle(&SmolStr::new("snapshot_shape"));
    let right = stable_shape_snapshot_handle(&SmolStr::new("snapshot_shape"));
    let region = stable_region_snapshot_handle(&SmolStr::new("snapshot_region"));
    let next_epoch = left.with_epoch(SnapshotEpoch(2));

    assert_eq!(left, right);
    assert_eq!(left.epoch().0, 1);
    assert_ne!(left.snapshot_id().0, 0);
    assert_ne!(
        left.root_entity().authored_content_id().0,
        left.root_entity().lineage_id().0
    );
    assert_ne!(
        left.root_entity().lineage_id().0,
        left.root_entity().snapshot_entity_id().0
    );
    assert_ne!(
        left.snapshot_id().0,
        left.root_entity().snapshot_entity_id().0
    );
    assert_eq!(
        left.root_entity().lineage_id(),
        next_epoch.root_entity().lineage_id()
    );
    assert_ne!(
        left.root_entity().snapshot_entity_id(),
        next_epoch.root_entity().snapshot_entity_id()
    );
    assert_ne!(left.portable_scene_id(), 0);
    assert_ne!(left.portable_root_feature_id(), 0);
    assert_eq!(region.portable_root_feature_id(), 0);
}

#[test]
fn query_traces_and_presentation_history_expose_snapshot_identity() {
    let (module, _type_info, ctx) = typed_module(snapshot_identity_source());
    let region_name = SmolStr::new("snapshot_region");
    let region_scene_id = ctx.region_scene_id(&region_name);
    let trace_plan = lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Trace));

    let (_value, trace) = execute_world_query_with_trace_on(
        &ctx,
        DispatchBackend::Cpu,
        &trace_plan,
        &[
            ctx.region_snapshot_handle(&region_name)
                .expect("region snapshot")
                .capture_value(),
            scene_domain_value(region_scene_id, 1, true, false, false),
            ray_query([0.0, 0.0, 3.0], [0.0, 0.0, -1.0]),
        ],
    )
    .expect("world trace");
    let snapshot = trace.snapshot.expect("trace snapshot");
    assert_eq!(snapshot.capture_name, "snapshot_region");
    assert_eq!(snapshot.epoch.0, 1);
    assert_eq!(
        snapshot,
        ctx.snapshot_report_for_capture_name(&region_name)
            .expect("context snapshot report")
    );

    let view = view_function(&module, "snapshot_view");
    let plan = PresentationPlan::from_view_function(view, DispatchBackend::Cpu)
        .expect("presentation plan");
    let input = presentation_input(
        stable_region_snapshot_handle(&region_name),
        region_scene_id,
        None,
    );
    let result = execute_plan(&ctx, &plan, &input).expect("presentation execute");
    let history = result.history.expect("temporal history");
    assert_eq!(history.snapshot.capture_name, "snapshot_region");
    assert_eq!(history.snapshot.epoch.0, 1);
    assert_eq!(history.snapshot.portable_scene_id, region_scene_id);
    assert_eq!(
        history.snapshot_handle,
        stable_region_snapshot_handle(&region_name)
    );
}

#[test]
fn epoch_mismatch_is_rejected_or_invalidated() {
    let (module, _type_info, ctx) = typed_module(snapshot_identity_source());
    let region_name = SmolStr::new("snapshot_region");
    let region_scene_id = ctx.region_scene_id(&region_name);
    let world_plan = lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Trace));

    let mismatch = execute_world_query_with_trace_on(
        &ctx,
        DispatchBackend::Cpu,
        &world_plan,
        &[
            KernelValue::Struct(KernelStructValue {
                name: SmolStr::new("RegionCapture"),
                fields: vec![
                    (SmolStr::new("scene_id"), KernelValue::U32(region_scene_id)),
                    (SmolStr::new("epoch"), KernelValue::U32(99)),
                    (SmolStr::new("root_feature_id"), KernelValue::U32(0)),
                ],
            }),
            scene_domain_value(region_scene_id, 1, true, false, false),
            ray_query([0.0, 0.0, 3.0], [0.0, 0.0, -1.0]),
        ],
    )
    .expect_err("epoch mismatch should fail");
    assert!(matches!(
        mismatch,
        QueryExecError::SnapshotEpochMismatch {
            kind: "region",
            expected: 1,
            found: 99,
            ..
        }
    ));

    let view = view_function(&module, "snapshot_view");
    let plan = PresentationPlan::from_view_function(view, DispatchBackend::Cpu)
        .expect("presentation plan");
    let frame0 = execute_plan(
        &ctx,
        &plan,
        &presentation_input(
            stable_region_snapshot_handle(&region_name),
            region_scene_id,
            None,
        ),
    )
    .expect("frame0");
    let mut mismatched_history = frame0.history.clone().expect("frame0 history");
    for slot in &mut mismatched_history.slots {
        slot.reuse_key.epoch = SnapshotEpoch(7);
    }
    let without_history = execute_plan(
        &ctx,
        &plan,
        &presentation_input(
            stable_region_snapshot_handle(&region_name),
            region_scene_id,
            None,
        ),
    )
    .expect("without history");
    let with_mismatched_history = execute_plan(
        &ctx,
        &plan,
        &presentation_input(
            stable_region_snapshot_handle(&region_name),
            region_scene_id,
            Some(mismatched_history),
        ),
    )
    .expect("mismatched history execution");
    assert_eq!(
        without_history
            .attachments
            .decode_attachment("color")
            .unwrap(),
        with_mismatched_history
            .attachments
            .decode_attachment("color")
            .unwrap()
    );

    let artifact_contract = world_plan
        .artifact_contracts
        .first()
        .expect("artifact contract");
    let stable_snapshot = stable_region_snapshot_handle(&region_name);
    let epoch_one_key = artifact_contract.reuse_key(
        &stable_snapshot,
        None,
        ArtifactPolicyDigestMode::CompatibleRange,
    );
    let newer_snapshot = stable_snapshot.with_epoch(SnapshotEpoch(2));
    let epoch_two_key = artifact_contract.reuse_key(
        &newer_snapshot,
        None,
        ArtifactPolicyDigestMode::CompatibleRange,
    );
    assert!(!epoch_one_key.compatible_with(&epoch_two_key));
}

#[test]
fn non_initial_snapshot_handles_execute_as_authoritative_targets() {
    let (module, _type_info, ctx) = typed_module(snapshot_identity_source());
    let region_name = SmolStr::new("snapshot_region");
    let region_scene_id = ctx.region_scene_id(&region_name);
    let world_plan = lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Trace));
    let region_snapshot = stable_region_snapshot_handle(&region_name).with_epoch(SnapshotEpoch(2));

    let (_value, trace) = execute_world_query_with_snapshot_on(
        &ctx,
        DispatchBackend::Cpu,
        Some(&region_snapshot),
        &world_plan,
        &[
            region_snapshot.capture_value(),
            scene_domain_value(region_scene_id, 1, true, false, false),
            ray_query([0.0, 0.0, 3.0], [0.0, 0.0, -1.0]),
        ],
    )
    .expect("world trace with non-initial snapshot");
    let snapshot = trace.snapshot.expect("trace snapshot");
    assert_eq!(snapshot.epoch.0, 2);
    assert_eq!(
        snapshot.snapshot_entity_id,
        region_snapshot.report().snapshot_entity_id
    );

    let view = view_function(&module, "snapshot_view");
    let plan = PresentationPlan::from_view_function(view, DispatchBackend::Cpu)
        .expect("presentation plan");
    let result = execute_plan(
        &ctx,
        &plan,
        &presentation_input(region_snapshot.clone(), region_scene_id, None),
    )
    .expect("presentation execute");
    let history = result.history.expect("temporal history");
    assert_eq!(history.snapshot.epoch.0, 2);
    assert_eq!(history.snapshot_handle, region_snapshot);
}

#[test]
fn artifact_reuse_keys_require_matching_policy_mode_and_digest() {
    let snapshot = stable_region_snapshot_handle(&SmolStr::new("snapshot_region"));
    let exact = wrela::artifact_key::ArtifactReuseKey::new(
        &snapshot,
        Some(SmolStr::new("contract")),
        SmolStr::new("schema"),
        17,
        Some(33),
        ArtifactPolicyDigestMode::Exact,
    );
    let same_exact = wrela::artifact_key::ArtifactReuseKey::new(
        &snapshot,
        Some(SmolStr::new("contract")),
        SmolStr::new("schema"),
        17,
        Some(33),
        ArtifactPolicyDigestMode::Exact,
    );
    let different_digest = wrela::artifact_key::ArtifactReuseKey::new(
        &snapshot,
        Some(SmolStr::new("contract")),
        SmolStr::new("schema"),
        17,
        Some(44),
        ArtifactPolicyDigestMode::Exact,
    );
    let different_mode = wrela::artifact_key::ArtifactReuseKey::new(
        &snapshot,
        Some(SmolStr::new("contract")),
        SmolStr::new("schema"),
        17,
        Some(33),
        ArtifactPolicyDigestMode::CompatibleRange,
    );

    assert!(exact.compatible_with(&same_exact));
    assert!(!exact.compatible_with(&different_digest));
    assert!(!exact.compatible_with(&different_mode));
}
