use smol_str::SmolStr;
use wrela::hir;
use wrela::hir::lower as hir_lower;
use wrela::kernel::{
    KernelStructValue, KernelValue, lower_capture_query_plan, lower_world_query_plan,
};
use wrela::parser::ast;
use wrela::parser::ast::AstNode;
use wrela::parser::parse;
use wrela::query_exec::{
    QueryExecContext, execute_capture_query_with_trace_on, execute_world_query_with_trace_on,
    render_semantic_cost_report, stable_region_scene_capture_id,
};
use wrela::query_plan::{
    CaptureKind, CaptureQueryKind, CaptureQueryPlan, DispatchBackend, WorldQueryKind,
    WorldQueryPlan,
};
use wrela::query_solver::{
    CertificateReuseClass, RayStepCertificateSubjectKind, StepCertificateKind,
    certificate_reuse_class_name, ray_step_certificate_kind_name,
};

fn lower_inline_module_from_source(source: &str) -> hir::Module {
    let node = parse(source);
    let root = ast::Root::cast(node).expect("root");
    hir_lower::lower(root)
}

fn typed_query_module(source: &str) -> (hir::Module, hir::TypeInfo, QueryExecContext) {
    let module = lower_inline_module_from_source(source);
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

fn world_trace_fixture_source() -> &'static str {
    r#"
field exact distance translated_field(p: Vec3) -> F32 {
    translate = vec3(2.5, 0.0, 0.0) {
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

shape translated_shape {
    field = translated_field
    material = shade
    payload = Payload(entity_id=u32(33), material_id=u32(33), actor=ActorHandle(id=u32(33), generation=u32(0)))
}

region scene_region() {
    place translated = translated_shape
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

fn scene_domain(
    scene_id: u32,
    detail: i32,
    material: bool,
    radiance: bool,
    media: bool,
) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("SceneDomain"),
        fields: vec![
            (SmolStr::new("scene_id"), KernelValue::U32(scene_id)),
            (
                SmolStr::new("spatial"),
                KernelValue::Struct(KernelStructValue {
                    name: SmolStr::new("SpatialDomainContract"),
                    fields: vec![(SmolStr::new("geometry_detail"), KernelValue::I32(detail))],
                }),
            ),
            (
                SmolStr::new("surface"),
                KernelValue::Struct(KernelStructValue {
                    name: SmolStr::new("SurfaceDomainContract"),
                    fields: vec![(SmolStr::new("material"), KernelValue::Bool(material))],
                }),
            ),
            (
                SmolStr::new("participants"),
                KernelValue::Struct(KernelStructValue {
                    name: SmolStr::new("ParticipantDomainContract"),
                    fields: vec![
                        (SmolStr::new("radiance"), KernelValue::Bool(radiance)),
                        (SmolStr::new("media"), KernelValue::Bool(media)),
                    ],
                }),
            ),
        ],
    })
}

fn ray_query_with_limits(
    origin: [f32; 3],
    direction: [f32; 3],
    max_distance: f32,
    min_step: f32,
    hit_epsilon: f32,
    max_steps: i32,
) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("RayQuery"),
        fields: vec![
            (SmolStr::new("origin"), KernelValue::Vec3(origin)),
            (SmolStr::new("direction"), KernelValue::Vec3(direction)),
            (SmolStr::new("max_distance"), KernelValue::F32(max_distance)),
            (SmolStr::new("min_step"), KernelValue::F32(min_step)),
            (SmolStr::new("hit_epsilon"), KernelValue::F32(hit_epsilon)),
            (SmolStr::new("max_steps"), KernelValue::I32(max_steps)),
        ],
    })
}

fn expect_struct<'a>(value: &'a KernelValue, name: &str) -> &'a KernelStructValue {
    match value {
        KernelValue::Struct(value) if value.name.as_str() == name => value,
        other => panic!("expected {name}, got {other:?}"),
    }
}

fn field<'a>(value: &'a KernelStructValue, name: &str) -> &'a KernelValue {
    value
        .fields
        .iter()
        .find(|(field_name, _)| field_name.as_str() == name)
        .map(|(_, value)| value)
        .unwrap_or_else(|| panic!("missing field {name} on {}", value.name))
}

fn expect_u32(value: &KernelValue) -> u32 {
    match value {
        KernelValue::U32(value) => *value,
        other => panic!("expected U32, got {other:?}"),
    }
}

fn expect_bool(value: &KernelValue) -> bool {
    match value {
        KernelValue::Bool(value) => *value,
        other => panic!("expected Bool, got {other:?}"),
    }
}

#[test]
fn ray_step_certificate_names_cover_public_kinds_and_reuse_classes() {
    assert_eq!(
        ray_step_certificate_kind_name(StepCertificateKind::DenseDistanceBound),
        "dense-distance-bound"
    );
    assert_eq!(
        ray_step_certificate_kind_name(StepCertificateKind::SupportEntryJump),
        "support-entry-jump"
    );
    assert_eq!(
        ray_step_certificate_kind_name(StepCertificateKind::AnalyticHit),
        "analytic-hit"
    );
    assert_eq!(
        ray_step_certificate_kind_name(StepCertificateKind::RelaxedConservativeJump),
        "relaxed-conservative-jump"
    );
    assert_eq!(
        ray_step_certificate_kind_name(StepCertificateKind::LipschitzBoundedJump),
        "lipschitz-bounded-jump"
    );
    assert_eq!(
        ray_step_certificate_kind_name(StepCertificateKind::IntervalNoRootProof),
        "interval-no-root-proof"
    );
    assert_eq!(
        ray_step_certificate_kind_name(StepCertificateKind::RefinementBracket),
        "refinement-bracket"
    );
    assert_eq!(
        certificate_reuse_class_name(CertificateReuseClass::RenderingOnly),
        "rendering-only"
    );
    assert_eq!(
        certificate_reuse_class_name(CertificateReuseClass::RenderingAndCollision),
        "rendering-and-collision"
    );
}

#[test]
fn cpu_world_trace_records_step_certificate_kinds_and_metadata() {
    let (_module, _type_info, ctx) = typed_query_module(world_trace_fixture_source());
    let region_scene_id = stable_region_scene_capture_id(&SmolStr::new("scene_region"));
    let plan = lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::Trace));
    let (hit, trace) = execute_world_query_with_trace_on(
        &ctx,
        DispatchBackend::Cpu,
        &plan,
        &[
            KernelValue::Capture(SmolStr::new("scene_region")),
            scene_domain(region_scene_id, 1, false, false, false),
            ray_query_with_limits([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], 6.0, 0.05, 0.001, 96),
        ],
    )
    .expect("cpu world trace");

    let hit = expect_struct(&hit, "Hit3");
    assert!(expect_bool(field(hit, "hit")));
    let payload = expect_struct(field(hit, "payload"), "Payload");
    assert_eq!(expect_u32(field(payload, "entity_id")), 33);

    assert!(trace.observability.ray_support_entry_jumps > 0);
    assert!(
        trace
            .observability
            .step_certificate_kinds
            .get(&StepCertificateKind::SupportEntryJump)
            .copied()
            .unwrap_or(0)
            > 0
    );
    assert!(
        trace
            .observability
            .step_certificate_kinds
            .get(&StepCertificateKind::AnalyticHit)
            .copied()
            .unwrap_or(0)
            > 0
    );
    assert!(trace.observability.interval_proof_successes > 0);
    assert!(trace.observability.analytic_transformed_hits > 0);

    let support_metadata = trace
        .observability
        .step_certificate_metadata
        .iter()
        .find(|metadata| {
            metadata.subject == "translated_shape"
                && metadata.subject_kind == RayStepCertificateSubjectKind::SupportInterval
                && metadata.proof_family == "support-entry-jump"
        })
        .expect("support entry jump metadata");
    assert_eq!(
        support_metadata.guarantee.name(),
        "conservative_no_false_miss"
    );
    assert_eq!(
        support_metadata.reusable_by,
        CertificateReuseClass::RenderingAndCollision
    );
    assert_eq!(
        support_metadata.tolerance_context,
        "support-interval entry jump"
    );
    assert!(
        support_metadata
            .invalidation_reasons
            .iter()
            .any(|reason| reason.contains("support bounds changed"))
    );

    let analytic_metadata = trace
        .observability
        .step_certificate_metadata
        .iter()
        .find(|metadata| {
            metadata.subject == "translated_shape"
                && metadata.subject_kind == RayStepCertificateSubjectKind::Primitive
                && metadata.proof_family == "analytic-primitive-hit"
        })
        .expect("analytic hit metadata");
    assert_eq!(analytic_metadata.guarantee.name(), "exact");
    assert_eq!(
        analytic_metadata.reusable_by,
        CertificateReuseClass::RenderingAndCollision
    );
    assert!(
        analytic_metadata
            .tolerance_context
            .contains("hit_epsilon=0.001000")
    );
    assert!(
        analytic_metadata
            .invalidation_reasons
            .iter()
            .any(|reason| reason.contains("safe transform chain changed"))
    );

    let rendered = render_semantic_cost_report(&trace.cost_report);
    assert!(rendered.contains("step_certificate_kinds="));
    assert!(rendered.contains("step_certificate_metadata="));
}

#[test]
fn cpu_capture_trace_preserves_dense_only_certificates_when_solver_methods_are_disabled() {
    let (_module, _type_info, ctx) = typed_query_module(world_trace_fixture_source());
    let plan = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::Trace, CaptureKind::Shape, None)
            .expect("shape trace plan"),
    );
    let (hit, trace) = execute_capture_query_with_trace_on(
        &ctx,
        DispatchBackend::Cpu,
        &plan,
        &[
            KernelValue::Capture(SmolStr::new("translated_shape")),
            ray_query_with_limits([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], 6.0, 0.05, 0.001, 96),
        ],
    )
    .expect("cpu capture trace");

    let hit = expect_struct(&hit, "Hit3");
    assert!(expect_bool(field(hit, "hit")));
    assert_eq!(
        trace.observability.step_certificate_kinds.len(),
        1,
        "dense-only capture traces should not surface mixed solver certificates"
    );
    assert!(
        trace
            .observability
            .step_certificate_kinds
            .contains_key(&StepCertificateKind::DenseDistanceBound)
    );
    assert_eq!(trace.observability.ray_support_entry_jumps, 0);
    assert_eq!(trace.observability.analytic_transformed_hits, 0);
    assert_eq!(trace.observability.solver_analytic_hits, 0);
    assert!(
        trace
            .observability
            .step_certificate_metadata
            .iter()
            .all(|metadata| metadata.guarantee.name() == "exact")
    );
}
