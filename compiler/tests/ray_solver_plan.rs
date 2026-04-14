use wrela::hir;
use wrela::hir::lower as hir_lower;
use wrela::kernel::{lower_batch_query_plan, lower_world_query_plan, validate_world_query_plan};
use wrela::parser::ast;
use wrela::parser::ast::AstNode;
use wrela::parser::parse;
use wrela::query_contract;
use wrela::query_plan::{self, BatchQueryKind, DispatchBackend, WorldQueryKind};
use wrela::query_solver::{
    AnalyticIntersectionStatus, EvidenceOrigin, EvidenceScope, FactAvailability, LipschitzStatus,
    PrimitiveFact, RaySolverFallbackKind, RaySolverFallbackReason, RaySolverHitBracketStatus,
    RaySolverIntentDisposition, RaySolverMethod, RaySolverMethodStatus, RaySolverNoCloserHitProof,
    RequiredGuaranteeClass, SelectedMethodClass, is_ray_shaped_spatial_contract,
};
use wrela::scene_ir;
use wrela::semantic_evidence::SemanticEvidence;

fn lower_inline_module_from_source(source: &str) -> hir::Module {
    let node = parse(source);
    let root = ast::Root::cast(node).expect("root");
    hir_lower::lower(root)
}

fn fact_fixture_source() -> &'static str {
    r#"
field exact distance sphere_field(p: Vec3) -> F32 {
    sphere(radius = 1.0)
}

field exact distance translated_sphere_field(p: Vec3) -> F32 {
    translate = vec3(1.5, 0.0, 0.0) {
        sphere(radius = 1.0)
    }
}

field conservative distance smooth_field(p: Vec3) -> F32 {
    smooth_union {
        smoothing = f32(0.35)
        use translated_sphere_field
        translate = vec3(2.1, 0.0, 0.0) {
            sphere(radius = 1.0)
        }
    }
}

field conservative distance zero_smooth_field(p: Vec3) -> F32 {
    smooth_union {
        smoothing = f32(0.0)
        use translated_sphere_field
        translate = vec3(2.1, 0.0, 0.0) {
            sphere(radius = 1.0)
        }
    }
}

field conservative distance repeated_field(p: Vec3) -> F32 {
    repeat_linear = vec3(2.5, 0.0, 0.0) {
        sphere(radius = 1.0)
    }
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
"#
}

#[test]
fn semantic_evidence_make_available_and_unavailable_solver_inputs_explicit() {
    let module = lower_inline_module_from_source(fact_fixture_source());
    let scene = scene_ir::lower_module(&module);
    let sphere = scene.fields.get("sphere_field").expect("sphere field");
    let sphere_evidence = SemanticEvidence::for_field_scene(sphere);
    assert_eq!(
        sphere_evidence.differential.primitive,
        PrimitiveFact::Single(hir::FieldPrimitive::Sphere)
    );
    assert_eq!(
        sphere_evidence.support.semantics,
        scene_ir::DistanceSemantics::ExactSignedDistance
    );
    assert_eq!(
        sphere_evidence.support.conservative_bounds,
        FactAvailability::Available
    );
    assert_eq!(
        sphere_evidence.differential.derivative,
        FactAvailability::Available
    );
    assert_eq!(
        sphere_evidence.distance.lipschitz,
        LipschitzStatus::ExactKnown
    );
    assert_eq!(
        sphere_evidence.distance.analytic_intersection,
        AnalyticIntersectionStatus::CandidateOnly
    );
    assert_eq!(
        sphere_evidence.distance.interval_bounds,
        FactAvailability::Unavailable
    );
    assert_eq!(
        sphere_evidence.summary().origin,
        EvidenceOrigin::StaticCompiled
    );
    assert_eq!(
        sphere_evidence.summary().scope,
        EvidenceScope::CompileInvariant
    );
    assert_eq!(
        sphere_evidence.support.lower_bound_pruning,
        FactAvailability::Available
    );
    assert!(sphere_evidence.unavailable_labels().contains(&"interval"));

    let opaque = scene.fields.get("opaque_field").expect("opaque field");
    let opaque_evidence = SemanticEvidence::for_field_scene(opaque);
    assert_eq!(
        opaque_evidence.support.semantics,
        scene_ir::DistanceSemantics::UnknownOpaque
    );
    assert_eq!(
        opaque_evidence.support.lower_bound_pruning,
        FactAvailability::Unavailable
    );
    assert_eq!(
        opaque_evidence.differential.derivative,
        FactAvailability::Unavailable
    );
    assert_eq!(
        opaque_evidence.distance.lipschitz,
        LipschitzStatus::Unavailable
    );
    assert!(
        opaque_evidence.unavailable_labels().contains(&"analytic"),
        "unavailable analytic facts must be visible in reports"
    );
}

#[test]
fn differential_evidence_tracks_supported_smooth_propagation_and_repeat_fallbacks() {
    let module = lower_inline_module_from_source(fact_fixture_source());
    let scene = scene_ir::lower_module(&module);
    let smooth = scene.fields.get("smooth_field").expect("smooth field");
    let smooth_evidence = SemanticEvidence::for_field_scene(smooth);
    assert_eq!(
        smooth.analysis.differential_support,
        scene_ir::SceneDifferentialSupport::CertifiedGradient
    );
    assert_eq!(
        smooth_evidence.differential.derivative,
        FactAvailability::Available
    );

    let zero_smooth = scene
        .fields
        .get("zero_smooth_field")
        .expect("zero smooth field");
    let zero_smooth_evidence = SemanticEvidence::for_field_scene(zero_smooth);
    assert_eq!(
        zero_smooth.analysis.differential_support,
        scene_ir::SceneDifferentialSupport::FiniteDifferenceFallback
    );
    assert_eq!(
        zero_smooth_evidence.differential.derivative,
        FactAvailability::Unavailable
    );

    let repeated = scene.fields.get("repeated_field").expect("repeated field");
    let repeated_evidence = SemanticEvidence::for_field_scene(repeated);
    assert_eq!(
        repeated.analysis.differential_support,
        scene_ir::SceneDifferentialSupport::FiniteDifferenceFallback
    );
    assert_eq!(
        repeated_evidence.differential.derivative,
        FactAvailability::Unavailable
    );
}

#[test]
fn ray_solver_plans_attach_only_to_ray_shaped_world_spatial_contracts() {
    assert!(is_ray_shaped_spatial_contract(
        query_contract::SPATIAL_NEAREST_WORLD
    ));
    assert!(is_ray_shaped_spatial_contract(
        query_contract::SPATIAL_NEAREST_BATCH_WORLD
    ));
    assert!(is_ray_shaped_spatial_contract(
        query_contract::SPATIAL_OCCLUDED_WORLD
    ));
    assert!(is_ray_shaped_spatial_contract(
        query_contract::SPATIAL_OCCLUDED_BATCH_WORLD
    ));
    assert!(!is_ray_shaped_spatial_contract(
        query_contract::SPATIAL_DISTANCE_WORLD
    ));

    let nearest = query_plan::WorldQueryPlan::for_query(WorldQueryKind::Nearest);
    let solver = nearest.ray_solver.as_ref().expect("nearest solver");
    assert_eq!(solver.contract_id, query_contract::SPATIAL_NEAREST_WORLD);
    assert!(solver.correctness.preserve_hit3_identity);
    assert!(solver.correctness.dense_cpu_oracle_required);
    assert_eq!(
        solver.evidence.summary().origin,
        EvidenceOrigin::RuntimeObserved
    );
    assert_eq!(
        solver.evidence.summary().scope,
        EvidenceScope::SnapshotLocal
    );
    assert_eq!(
        solver.fallback.kind,
        RaySolverFallbackKind::ExactDenseSphereTracing
    );
    assert_eq!(
        solver.certificate.method,
        RaySolverMethod::AnalyticPrimitiveIntersection
    );
    assert!(solver.certificate.hit_or_miss_recorded);
    assert_eq!(
        solver.certificate.hit_bracket,
        RaySolverHitBracketStatus::Available
    );
    assert_eq!(
        solver.certificate.no_closer_hit_proof,
        RaySolverNoCloserHitProof::Available
    );
    assert_eq!(
        solver.certificate.fallback_reason,
        Some(RaySolverFallbackReason::ContractRequiresDenseOracle)
    );
    assert!(
        solver.method_enabled(RaySolverMethod::DenseSphereTracing),
        "dense marching must be an explicit solver fallback method"
    );
    for method in [
        RaySolverMethod::SupportBoundCandidateRejection,
        RaySolverMethod::AnalyticPrimitiveIntersection,
        RaySolverMethod::LipschitzSafeStepping,
        RaySolverMethod::IntervalNewtonIsolation,
        RaySolverMethod::SafeguardedNewtonRefinement,
        RaySolverMethod::RepeatAwareTraversal,
    ] {
        assert!(
            solver.portfolio.entries.iter().any(|entry| {
                entry.method == method && entry.status == RaySolverMethodStatus::Available
            }),
            "runtime-unknown plans should surface {method:?} as an available conditional method"
        );
    }
    assert!(!solver.method_enabled(RaySolverMethod::RepeatAwareTraversal));
    assert!(!solver.method_enabled(RaySolverMethod::AffineArithmeticBounds));
    assert!(!solver.method_enabled(RaySolverMethod::TilePacketSolving));
    assert!(!solver.method_enabled(RaySolverMethod::NeighborFrameContinuation));
    assert_eq!(solver.subject.as_str(), solver.contract_id.as_str());
    assert_eq!(solver.portfolio.entries.len(), 10);
    assert_eq!(solver.mixed_selections().len(), 7);
    assert!(
        solver
            .mixed_selections()
            .iter()
            .all(|selection| selection.subject.as_str() == solver.subject.as_str()),
        "mixed selections must remain anchored to the solver subject"
    );
    assert!(solver.mixed_selections().iter().any(|selection| {
        selection.method == RaySolverMethod::DenseSphereTracing
            && selection.candidate_class == "dense-oracle"
            && selection.required_guarantee == RequiredGuaranteeClass::Exact
            && selection.selected_method_class == SelectedMethodClass::ExactOracle
    }));
    assert!(solver.mixed_selections().iter().any(|selection| {
        selection.method == RaySolverMethod::SupportBoundCandidateRejection
            && selection.candidate_class == "support-bounded-candidates"
            && selection.required_guarantee == RequiredGuaranteeClass::ConservativeNoFalseMiss
            && selection.selected_method_class == SelectedMethodClass::ConservativeSolver
    }));
    assert!(solver.mixed_selections().iter().any(|selection| {
        selection.method == RaySolverMethod::AnalyticPrimitiveIntersection
            && selection.candidate_class == "analytic-primitive-candidates"
            && selection.selected_method_class == SelectedMethodClass::ExactOracle
    }));
    assert!(solver.mixed_selections().iter().any(|selection| {
        selection.method == RaySolverMethod::LipschitzSafeStepping
            && selection.candidate_class == "lipschitz-safe-candidates"
            && selection.selected_method_class == SelectedMethodClass::IntervalSolver
    }));
    assert!(solver.mixed_selections().iter().any(|selection| {
        selection.method == RaySolverMethod::IntervalNewtonIsolation
            && selection.candidate_class == "interval-isolation-candidates"
            && selection.required_guarantee == RequiredGuaranteeClass::IntervalBounded
            && selection.selected_method_class == SelectedMethodClass::IntervalSolver
    }));
    assert!(solver.mixed_selections().iter().any(|selection| {
        selection.method == RaySolverMethod::SafeguardedNewtonRefinement
            && selection.candidate_class == "newton-refinement-candidates"
            && selection.required_guarantee == RequiredGuaranteeClass::IntervalBounded
            && selection.selected_method_class == SelectedMethodClass::IntervalSolver
    }));
    assert!(
        !solver
            .mixed_selections()
            .iter()
            .any(|selection| selection.evidence_policy_summary.is_empty())
    );
    assert_eq!(solver.artifact_reuse_intents().len(), 1);
    assert_eq!(solver.continuation_intents().len(), 1);
    let summary = solver.diagnostic_summary();
    assert_eq!(summary.subject.as_str(), solver.subject.as_str());
    assert_eq!(
        summary.evidence_summary.origin,
        EvidenceOrigin::RuntimeObserved
    );
    assert_eq!(summary.evidence_summary.scope, EvidenceScope::SnapshotLocal);
    assert!(
        summary
            .methods
            .contains(&RaySolverMethod::AnalyticPrimitiveIntersection),
        "runtime-unknown world plans should report conditional analytic hooks"
    );
    assert_eq!(summary.mixed_selections, solver.mixed_selections().to_vec());
    assert_eq!(
        summary.methods,
        vec![
            RaySolverMethod::DenseSphereTracing,
            RaySolverMethod::SupportBoundCandidateRejection,
            RaySolverMethod::AnalyticPrimitiveIntersection,
            RaySolverMethod::LipschitzSafeStepping,
            RaySolverMethod::IntervalNewtonIsolation,
            RaySolverMethod::SafeguardedNewtonRefinement,
            RaySolverMethod::RepeatAwareTraversal,
        ]
    );
    assert_eq!(
        summary.artifact_reuse_intents,
        solver.artifact_reuse_intents().to_vec()
    );
    assert_eq!(
        summary.continuation_intents,
        solver.continuation_intents().to_vec()
    );
    assert_eq!(
        summary.artifact_reuse_intents[0].disposition,
        RaySolverIntentDisposition::Unavailable
    );
    assert_eq!(
        summary.continuation_intents[0].disposition,
        RaySolverIntentDisposition::Unavailable
    );
    assert!(
        summary.artifact_reuse_intents[0]
            .reasons
            .iter()
            .any(|reason| reason.contains("runtime artifact instance")),
        "artifact reuse diagnostics should explain the missing compatible runtime artifact"
    );
    assert!(
        summary.continuation_intents[0]
            .reasons
            .iter()
            .any(|reason| reason.contains("runtime transition context")),
        "continuation diagnostics should explain the missing compatible runtime context"
    );
    assert!(
        !summary.unavailable_facts.contains(&"analytic"),
        "runtime-unknown world plans must not report analytic facts as permanently unavailable"
    );

    let occluded = query_plan::WorldQueryPlan::for_query(WorldQueryKind::Occluded);
    assert!(occluded.ray_solver.is_some());
    let distance = query_plan::WorldQueryPlan::for_query(WorldQueryKind::Distance);
    assert!(distance.ray_solver.is_none());

    let nearest_batch =
        query_plan::BatchQueryPlan::for_world_query(BatchQueryKind::Nearest, DispatchBackend::Wgsl);
    assert!(nearest_batch.ray_solver.is_some());
    let query_plan::BatchItemContract::WorldQuery { plan: item_plan } =
        &nearest_batch.item_contract
    else {
        panic!("world-batch nearest must execute through a world-query item plan");
    };
    assert!(
        item_plan.ray_solver.is_some(),
        "world batch items must keep the query-owned solver boundary"
    );
}

#[test]
fn kernel_validation_rejects_ray_world_plans_without_solver_boundary() {
    let mut plan = lower_world_query_plan(&query_plan::WorldQueryPlan::for_query(
        WorldQueryKind::Nearest,
    ));
    plan.ray_solver = None;
    let errors = validate_world_query_plan(&plan).expect_err("missing solver should fail");
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("must route through a RaySolverPlan")),
        "expected solver-boundary validation error, got {errors:?}"
    );

    let batch_plan = lower_batch_query_plan(&query_plan::BatchQueryPlan::for_world_query(
        BatchQueryKind::Occluded,
        DispatchBackend::Cpu,
    ));
    assert!(
        batch_plan.ray_solver.is_some(),
        "lowering must preserve world-batch solver diagnostics"
    );
}
