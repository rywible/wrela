use smol_str::SmolStr;
use wrela::hir;
use wrela::hir::lower as hir_lower;
use wrela::kernel::{
    KernelStructValue, KernelValue, lower_capture_query_plan, lower_world_query_plan,
};
use wrela::parser::ast;
use wrela::parser::ast::AstNode;
use wrela::parser::parse;
use wrela::query_contract;
use wrela::query_exec::{
    DirectQueryExecutor, QueryExecContext, QueryExecError, SemanticCostCauseKind,
    execute_capture_query_with_trace_on, execute_world_query_with_trace_on,
    stable_region_scene_capture_id,
};
use wrela::query_plan::{
    ArtifactSchema, CandidateSource, CandidateStrategy, CaptureKind, CaptureQueryKind,
    CaptureQueryPlan, DerivedArtifact, DispatchBackend, PlanExecutor, PlanStage, PruningStrategy,
    QueryItemKind, QueryResultKind, SceneSummary, WinnerSelectionMode, WorldQueryKind,
    WorldQueryPlan,
};

const SUPPORT_CLASS_UNKNOWN: u32 = 0;
const SUPPORT_CLASS_BOUNDED: u32 = 1;
const SEMANTICS_EXACT_SIGNED_DISTANCE: u32 = 0;
const SEMANTICS_CONSERVATIVE_LOWER_BOUND: u32 = 1;
const SEMANTICS_UNKNOWN_OPAQUE: u32 = 2;

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

fn support_fixture_source() -> &'static str {
    r#"
field exact distance sphere_field(p: Vec3) -> F32 {
    sphere(radius = 1.0)
}

field exact distance translated_sphere_field(p: Vec3) -> F32 {
    translate = vec3(4.0, 0.0, 0.0) {
        sphere(radius = 0.5)
    }
}

field conservative distance opaque_box_field(p: Vec3) -> F32 {
    support = Support3(bounds=Bounds3(
        min=vec3(-2.5, -0.75, -0.75),
        max=vec3(-1.5, 0.75, 0.75)
    ))
    bounds = Bounds3(
        min=vec3(-2.5, -0.75, -0.75),
        max=vec3(-1.5, 0.75, 0.75)
    )
    return length(p - vec3(-2.0, 0.0, 0.0)) - 0.5
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
    payload = Payload()
}

shape translated_sphere_shape {
    field = translated_sphere_field
    material = shade
    payload = Payload()
}

shape opaque_box_shape {
    field = opaque_box_field
    material = shade
    payload = Payload()
}

region detail_region() {
    place coarse = sphere_shape
    place fine = translated_sphere_shape
}
"#
}

fn capture(name: &str) -> KernelValue {
    KernelValue::Capture(SmolStr::new(name))
}

fn region_scene_id(name: &str) -> u32 {
    stable_region_scene_capture_id(&SmolStr::new(name))
}

fn scene_domain(scene_id: u32, detail: i32) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("SceneDomain"),
        fields: vec![
            (SmolStr::new("scene_id"), KernelValue::U32(scene_id)),
            (
                SmolStr::new("spatial"),
                KernelValue::Struct(KernelStructValue {
                    name: SmolStr::new("SpatialDomainContract"),
                    fields: vec![
                        (SmolStr::new("geometry_detail"), KernelValue::I32(detail)),
                        (SmolStr::new("guarantee"), KernelValue::U32(0)),
                    ],
                }),
            ),
            (
                SmolStr::new("surface"),
                KernelValue::Struct(KernelStructValue {
                    name: SmolStr::new("SurfaceDomainContract"),
                    fields: vec![(SmolStr::new("material"), KernelValue::Bool(false))],
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

fn field_scene_summary(ctx: &QueryExecContext, name: &str) -> SceneSummary {
    let scene = ctx.scene.fields.get(name).expect("field scene");
    SceneSummary {
        name: Some(scene.name.clone()),
        semantics: scene.semantics,
        support_class: scene.support_class,
        can_coarse_support_pruning: scene.can_coarse_support_pruning,
        opaque_boundary: scene.opaque_boundary,
        semantic_root: scene.root_node_id.0,
        support_root: scene.root_support_id.0,
        node_count: scene.node_records.len() as u32,
        support_node_count: scene.support_records.len() as u32,
        leaf_count: 0,
        identity_source_count: scene.identity_sources.len() as u32,
    }
}

fn summary_struct(value: &KernelValue) -> &KernelStructValue {
    let KernelValue::Struct(summary) = value else {
        panic!("expected SupportSummaryResult, got {value:?}");
    };
    assert_eq!(summary.name.as_str(), "SupportSummaryResult");
    summary
}

fn summary_field<'a>(summary: &'a KernelStructValue, name: &str) -> &'a KernelValue {
    summary
        .fields
        .iter()
        .find(|(field_name, _)| field_name.as_str() == name)
        .map(|(_, value)| value)
        .unwrap_or_else(|| panic!("missing SupportSummaryResult field '{name}'"))
}

fn summary_u32(summary: &KernelStructValue, name: &str) -> u32 {
    match summary_field(summary, name) {
        KernelValue::U32(value) => *value,
        other => panic!("expected u32 field '{name}', got {other:?}"),
    }
}

fn summary_bool(summary: &KernelStructValue, name: &str) -> bool {
    match summary_field(summary, name) {
        KernelValue::Bool(value) => *value,
        other => panic!("expected bool field '{name}', got {other:?}"),
    }
}

fn summary_vec3(summary: &KernelStructValue, name: &str) -> [f32; 3] {
    match summary_field(summary, name) {
        KernelValue::Vec3(value) => *value,
        other => panic!("expected vec3 field '{name}', got {other:?}"),
    }
}

fn assert_vec3_close(actual: [f32; 3], expected: [f32; 3]) {
    for (actual, expected) in actual.iter().zip(expected) {
        assert!(
            (*actual - expected).abs() <= 0.0001,
            "expected {expected}, got {actual}"
        );
    }
}

fn assert_summary(
    value: &KernelValue,
    support_class: u32,
    semantics: u32,
    has_bounds: bool,
    opaque_boundary: bool,
    can_coarse_support_prune: bool,
    min: [f32; 3],
    max: [f32; 3],
) {
    let summary = summary_struct(value);
    assert_eq!(summary_u32(summary, "support_class"), support_class);
    assert_eq!(summary_u32(summary, "semantics"), semantics);
    assert_eq!(summary_bool(summary, "has_bounds"), has_bounds);
    assert_eq!(summary_bool(summary, "opaque_boundary"), opaque_boundary);
    assert_eq!(
        summary_bool(summary, "can_coarse_support_prune"),
        can_coarse_support_prune
    );
    assert_vec3_close(summary_vec3(summary, "min"), min);
    assert_vec3_close(summary_vec3(summary, "max"), max);
}

#[test]
fn support_summary_contracts_are_unit_queries_and_semantic_plans() {
    let ctx = typed_query_module(support_fixture_source());
    let scene_summary = field_scene_summary(&ctx, "sphere_field");
    let plan = CaptureQueryPlan::for_query(
        CaptureQueryKind::SupportSummary,
        CaptureKind::Field,
        Some(scene_summary.clone()),
    )
    .unwrap();

    let descriptor = query_contract::query_contract(query_contract::SUPPORT_SUMMARY_CAPTURE_FIELD)
        .expect("support summary field contract");
    assert_eq!(descriptor.item_kind, QueryItemKind::Unit);
    assert_eq!(
        descriptor.result_kind,
        QueryResultKind::SupportSummaryResult
    );
    assert!(descriptor.supported_backends.supports(DispatchBackend::Cpu));
    assert!(
        descriptor
            .supported_backends
            .supports(DispatchBackend::VirtualGpu)
    );
    assert!(
        !descriptor
            .supported_backends
            .supports(DispatchBackend::Wgsl)
    );

    assert_eq!(
        plan.contract_id,
        query_contract::SUPPORT_SUMMARY_CAPTURE_FIELD
    );
    assert_eq!(plan.kind, CaptureQueryKind::SupportSummary);
    assert_eq!(plan.executor, PlanExecutor::FieldSupportSummaryCapture);
    assert_eq!(
        plan.candidate_strategy,
        CandidateStrategy::SemanticSupportSummary
    );
    assert_eq!(plan.pruning_strategy, PruningStrategy::None);
    assert_eq!(
        plan.candidate_contract.source,
        CandidateSource::CaptureScene
    );
    assert_eq!(plan.candidate_contract.item_kind, QueryItemKind::Unit);
    assert_eq!(
        plan.candidate_contract.winner_mode,
        WinnerSelectionMode::None
    );
    assert_eq!(
        plan.result_contract.result_kind,
        QueryResultKind::SupportSummaryResult
    );
    assert!(plan.hit_context_contract.is_none());
    assert!(plan.stages.iter().all(|stage| !matches!(
        stage,
        PlanStage::GenerateCandidates { .. } | PlanStage::PruneCandidates { .. }
    )));
    assert_eq!(
        plan.derived_artifacts,
        vec![
            DerivedArtifact::SupportSummary {
                semantics: scene_summary.semantics,
                support_class: scene_summary.support_class,
                can_coarse_support_pruning: scene_summary.can_coarse_support_pruning,
            },
            DerivedArtifact::CaptureCache {
                capture_kind: CaptureKind::Field,
            },
        ]
    );
    match &plan.artifact_contracts[0].schema {
        ArtifactSchema::SupportSummary {
            semantic_root,
            support_root,
            node_count,
            support_node_count,
            ..
        } => {
            assert_eq!(*semantic_root, scene_summary.semantic_root);
            assert_eq!(*support_root, scene_summary.support_root);
            assert_eq!(*node_count, scene_summary.node_count);
            assert_eq!(*support_node_count, scene_summary.support_node_count);
        }
        other => panic!("expected support summary artifact, got {other:?}"),
    }

    let world_plan = WorldQueryPlan::for_query_with_backend(
        WorldQueryKind::SupportSummary,
        DispatchBackend::VirtualGpu,
    );
    assert_eq!(
        world_plan.contract_id,
        query_contract::SUPPORT_SUMMARY_WORLD
    );
    assert_eq!(world_plan.kind, WorldQueryKind::SupportSummary);
    assert_eq!(
        world_plan.executor,
        PlanExecutor::WorldSupportSummaryCapture
    );
    assert_eq!(
        world_plan.candidate_strategy,
        CandidateStrategy::SemanticSupportSummary
    );
    assert_eq!(world_plan.pruning_strategy, PruningStrategy::None);
    assert_eq!(world_plan.dispatch_contract.item_kind, QueryItemKind::Unit);
    assert_eq!(
        world_plan.dispatch_contract.result_kind,
        QueryResultKind::SupportSummaryResult
    );
    assert!(world_plan.stages.iter().all(|stage| !matches!(
        stage,
        PlanStage::GenerateCandidates { .. } | PlanStage::PruneCandidates { .. }
    )));
}

#[test]
fn capture_support_summary_uses_scene_semantics_without_sampling() {
    let ctx = typed_query_module(support_fixture_source());
    let field_plan = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(
            CaptureQueryKind::SupportSummary,
            CaptureKind::Field,
            Some(field_scene_summary(&ctx, "sphere_field")),
        )
        .unwrap(),
    );
    let shape_plan = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::SupportSummary, CaptureKind::Shape, None)
            .unwrap(),
    );

    for backend in [DispatchBackend::Cpu, DispatchBackend::VirtualGpu] {
        let (field_value, field_trace) = execute_capture_query_with_trace_on(
            &ctx,
            backend,
            &field_plan,
            &[capture("sphere_field")],
        )
        .unwrap();
        assert_eq!(
            field_trace.contract_id,
            query_contract::SUPPORT_SUMMARY_CAPTURE_FIELD
        );
        assert_eq!(
            field_trace.executor,
            match backend {
                DispatchBackend::VirtualGpu => DirectQueryExecutor::VirtualGpu,
                _ => DirectQueryExecutor::Cpu,
            }
        );
        assert_eq!(field_trace.observability.field_samples, 0);
        assert_eq!(field_trace.observability.trace_steps, 0);
        assert_eq!(field_trace.observability.candidate_count, 0);
        assert!(field_trace.observability.artifact_loads >= 1);
        assert!(
            field_trace
                .cost_report
                .causes
                .iter()
                .all(|cause| cause.kind != SemanticCostCauseKind::CandidateTraversal)
        );
        assert_summary(
            &field_value,
            SUPPORT_CLASS_BOUNDED,
            SEMANTICS_EXACT_SIGNED_DISTANCE,
            true,
            false,
            true,
            [-1.0, -1.0, -1.0],
            [1.0, 1.0, 1.0],
        );

        let (shape_value, shape_trace) = execute_capture_query_with_trace_on(
            &ctx,
            backend,
            &shape_plan,
            &[capture("translated_sphere_shape")],
        )
        .unwrap();
        assert_eq!(
            shape_trace.contract_id,
            query_contract::SUPPORT_SUMMARY_CAPTURE_SHAPE
        );
        assert_eq!(shape_trace.observability.field_samples, 0);
        assert_eq!(shape_trace.observability.trace_steps, 0);
        assert_eq!(shape_trace.observability.candidate_count, 0);
        assert_summary(
            &shape_value,
            SUPPORT_CLASS_BOUNDED,
            SEMANTICS_EXACT_SIGNED_DISTANCE,
            true,
            false,
            true,
            [3.5, -0.5, -0.5],
            [4.5, 0.5, 0.5],
        );

        let (opaque_value, opaque_trace) = execute_capture_query_with_trace_on(
            &ctx,
            backend,
            &field_plan,
            &[capture("opaque_box_field")],
        )
        .unwrap();
        assert_eq!(opaque_trace.observability.field_samples, 0);
        assert_eq!(opaque_trace.observability.trace_steps, 0);
        assert_summary(
            &opaque_value,
            SUPPORT_CLASS_UNKNOWN,
            SEMANTICS_UNKNOWN_OPAQUE,
            true,
            true,
            false,
            [-2.5, -0.75, -0.75],
            [-1.5, 0.75, 0.75],
        );
    }
}

#[test]
fn world_support_summary_merges_visible_shapes_by_geometry_detail() {
    let ctx = typed_query_module(support_fixture_source());
    let region_scene_id = region_scene_id("detail_region");
    let world_plan =
        lower_world_query_plan(&WorldQueryPlan::for_query(WorldQueryKind::SupportSummary));
    let coarse_args = vec![capture("detail_region"), scene_domain(region_scene_id, 0)];
    let fine_args = vec![capture("detail_region"), scene_domain(region_scene_id, 1)];

    let (coarse_value, coarse_trace) =
        execute_world_query_with_trace_on(&ctx, DispatchBackend::Cpu, &world_plan, &coarse_args)
            .unwrap();
    assert_eq!(
        coarse_trace.contract_id,
        query_contract::SUPPORT_SUMMARY_WORLD
    );
    assert_eq!(coarse_trace.observability.field_samples, 0);
    assert_eq!(coarse_trace.observability.trace_steps, 0);
    assert_eq!(coarse_trace.observability.candidate_count, 0);
    assert!(coarse_trace.observability.artifact_loads >= 1);
    assert!(
        coarse_trace
            .cost_report
            .causes
            .iter()
            .all(|cause| cause.kind != SemanticCostCauseKind::CandidateTraversal)
    );
    assert_summary(
        &coarse_value,
        SUPPORT_CLASS_BOUNDED,
        SEMANTICS_EXACT_SIGNED_DISTANCE,
        true,
        false,
        true,
        [-1.0, -1.0, -1.0],
        [1.0, 1.0, 1.0],
    );

    let (fine_value, fine_trace) =
        execute_world_query_with_trace_on(&ctx, DispatchBackend::Cpu, &world_plan, &fine_args)
            .unwrap();
    assert_eq!(fine_trace.observability.field_samples, 0);
    assert_eq!(fine_trace.observability.trace_steps, 0);
    assert_eq!(fine_trace.observability.candidate_count, 0);
    assert_summary(
        &fine_value,
        SUPPORT_CLASS_BOUNDED,
        SEMANTICS_CONSERVATIVE_LOWER_BOUND,
        true,
        false,
        true,
        [-1.0, -1.0, -1.0],
        [4.5, 1.0, 1.0],
    );

    let vgpu_plan = lower_world_query_plan(&WorldQueryPlan::for_query_with_backend(
        WorldQueryKind::SupportSummary,
        DispatchBackend::VirtualGpu,
    ));
    let (vgpu_fine_value, vgpu_fine_trace) = execute_world_query_with_trace_on(
        &ctx,
        DispatchBackend::VirtualGpu,
        &vgpu_plan,
        &fine_args,
    )
    .unwrap();
    assert_eq!(vgpu_fine_trace.executor, DirectQueryExecutor::VirtualGpu);
    assert_eq!(vgpu_fine_trace.observability.field_samples, 0);
    assert_eq!(vgpu_fine_trace.observability.trace_steps, 0);
    assert_eq!(vgpu_fine_value, fine_value);
}

#[test]
fn support_summary_rejects_wgsl_through_contract_support() {
    let ctx = typed_query_module(support_fixture_source());
    let field_plan = lower_capture_query_plan(
        &CaptureQueryPlan::for_query(CaptureQueryKind::SupportSummary, CaptureKind::Field, None)
            .unwrap(),
    );
    let err = execute_capture_query_with_trace_on(
        &ctx,
        DispatchBackend::Wgsl,
        &field_plan,
        &[capture("sphere_field")],
    )
    .unwrap_err();
    match err {
        QueryExecError::Unsupported { message } => {
            assert!(message.contains("support.summary.capture.field"));
            assert!(message.contains("does not support backend Wgsl"));
        }
        other => panic!("expected contract-driven WGSL rejection, got {other:?}"),
    }

    let region_scene_id = region_scene_id("detail_region");
    let world_plan = lower_world_query_plan(&WorldQueryPlan::for_query_with_backend(
        WorldQueryKind::SupportSummary,
        DispatchBackend::Wgsl,
    ));
    let err = execute_world_query_with_trace_on(
        &ctx,
        DispatchBackend::Wgsl,
        &world_plan,
        &[capture("detail_region"), scene_domain(region_scene_id, 1)],
    )
    .unwrap_err();
    match err {
        QueryExecError::Unsupported { message } => {
            assert!(message.contains("support.summary.world"));
            assert!(message.contains("does not support backend Wgsl"));
        }
        other => panic!("expected contract-driven WGSL rejection, got {other:?}"),
    }
}
