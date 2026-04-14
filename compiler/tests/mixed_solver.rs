use wrela::hir;
use wrela::hir::lower as hir_lower;
use wrela::parser::ast;
use wrela::parser::ast::AstNode;
use wrela::parser::parse;
use wrela::query_contract;
use wrela::query_solver::{
    AccelerationRejectionClass, RaySolverFallbackReason, RaySolverHitBracketStatus,
    RaySolverIntentDisposition, RaySolverMethod, RaySolverMethodStatus, RaySolverNoCloserHitProof,
    RequiredGuaranteeClass, SelectedMethodClass,
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

fn acceleration_rejection_fixture_source() -> &'static str {
    r#"
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

field conservative distance repeat_linear_field(p: Vec3) -> F32 {
    repeat_linear = vec3(2.0, 0.0, 0.0) {
        sphere(radius = 1.0)
    }
}

field conservative distance affine_field(p: Vec3) -> F32 {
    affine_transform = Transform3(
        matrix=mat4_cols(
            vec4(1.0, 0.0, 0.0, 0.0),
            vec4(0.0, 1.0, 0.0, 0.0),
            vec4(0.0, 0.0, 1.0, 0.0),
            vec4(1.5, 0.0, 0.0, 1.0)
        ),
        inverse=mat4_cols(
            vec4(1.0, 0.0, 0.0, 0.0),
            vec4(0.0, 1.0, 0.0, 0.0),
            vec4(0.0, 0.0, 1.0, 0.0),
            vec4(-1.5, 0.0, 0.0, 1.0)
        )
    ) {
        sphere(radius = 1.0)
    }
}

field exact distance plane_field(p: Vec3) -> F32 {
    plane(normal = vec3(0.0, 1.0, 0.0), offset = 0.0)
}
"#
}

#[test]
fn ray_solver_plan_exposes_mixed_selection_and_intent_summary_surface() {
    let module = lower_inline_module_from_source(fact_fixture_source());
    let scene = scene_ir::lower_module(&module);
    let sphere = scene.fields.get("sphere_field").expect("sphere field");

    let solver = wrela::query_solver::RaySolverPlan::for_contract(
        query_contract::SPATIAL_NEAREST_WORLD,
        Some(SemanticEvidence::for_field_scene(sphere)),
    )
    .expect("ray solver plan");
    let summary = solver.diagnostic_summary();

    assert_eq!(solver.subject.as_str(), solver.contract_id.as_str());
    assert_eq!(summary.subject, solver.subject);
    assert_eq!(summary.mixed_selections.len(), 6);
    assert_eq!(
        summary.methods,
        vec![
            RaySolverMethod::DenseSphereTracing,
            RaySolverMethod::SupportBoundCandidateRejection,
            RaySolverMethod::AnalyticPrimitiveIntersection,
            RaySolverMethod::LipschitzSafeStepping,
            RaySolverMethod::IntervalNewtonIsolation,
            RaySolverMethod::SafeguardedNewtonRefinement,
        ]
    );
    assert!(
        summary
            .mixed_selections
            .iter()
            .all(|selection| selection.subject == solver.subject)
    );
    assert!(summary.mixed_selections.iter().any(|selection| {
        selection.method == RaySolverMethod::DenseSphereTracing
            && selection.candidate_class == "dense-oracle"
            && selection.required_guarantee == RequiredGuaranteeClass::Exact
            && selection.selected_method_class == SelectedMethodClass::ExactOracle
    }));
    assert!(summary.mixed_selections.iter().any(|selection| {
        selection.method == RaySolverMethod::SupportBoundCandidateRejection
            && selection.candidate_class == "support-bounded-candidates"
            && selection.required_guarantee == RequiredGuaranteeClass::ConservativeNoFalseMiss
            && selection.selected_method_class == SelectedMethodClass::ConservativeSolver
    }));
    assert!(summary.mixed_selections.iter().any(|selection| {
        selection.method == RaySolverMethod::LipschitzSafeStepping
            && selection.candidate_class == "lipschitz-safe-candidates"
            && selection.selected_method_class == SelectedMethodClass::IntervalSolver
    }));
    assert!(summary.mixed_selections.iter().any(|selection| {
        selection.method == RaySolverMethod::IntervalNewtonIsolation
            && selection.candidate_class == "interval-isolation-candidates"
            && selection.required_guarantee == RequiredGuaranteeClass::IntervalBounded
            && selection.selected_method_class == SelectedMethodClass::IntervalSolver
    }));
    assert!(summary.mixed_selections.iter().any(|selection| {
        selection.method == RaySolverMethod::SafeguardedNewtonRefinement
            && selection.candidate_class == "newton-refinement-candidates"
            && selection.required_guarantee == RequiredGuaranteeClass::IntervalBounded
            && selection.selected_method_class == SelectedMethodClass::IntervalSolver
    }));
    assert!(
        summary
            .acceleration_rejection_classes
            .contains(&AccelerationRejectionClass::ArtifactUnavailable)
    );
    assert_eq!(summary.artifact_reuse_intents.len(), 1);
    assert_eq!(summary.continuation_intents.len(), 1);
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
    assert_eq!(solver.portfolio.entries.len(), 10);
    assert_eq!(
        solver
            .portfolio
            .entries
            .iter()
            .filter(|entry| matches!(entry.status, RaySolverMethodStatus::Enabled))
            .count(),
        6
    );
    assert!(solver.portfolio.entries.iter().any(|entry| {
        entry.method == RaySolverMethod::IntervalNewtonIsolation
            && entry.status == RaySolverMethodStatus::Enabled
    }));
    assert!(solver.portfolio.entries.iter().any(|entry| {
        entry.method == RaySolverMethod::SafeguardedNewtonRefinement
            && entry.status == RaySolverMethodStatus::Enabled
    }));
    assert!(solver.portfolio.entries.iter().any(|entry| {
        entry.method == RaySolverMethod::AffineArithmeticBounds
            && entry.status == RaySolverMethodStatus::Reserved
    }));
    assert!(solver.portfolio.entries.iter().any(|entry| {
        entry.method == RaySolverMethod::TilePacketSolving
            && entry.status == RaySolverMethodStatus::Reserved
    }));
    assert!(solver.portfolio.entries.iter().any(|entry| {
        entry.method == RaySolverMethod::NeighborFrameContinuation
            && entry.status == RaySolverMethodStatus::Reserved
    }));
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
            .any(|reason| reason.contains("runtime artifact instance"))
    );
    assert!(
        summary.continuation_intents[0]
            .reasons
            .iter()
            .any(|reason| reason.contains("runtime transition context"))
    );
    assert!(
        summary.artifact_reuse_intents[0]
            .selection
            .evidence_policy_summary
            .contains("artifact reuse candidate")
    );
    assert!(
        summary.continuation_intents[0]
            .selection
            .evidence_policy_summary
            .contains("continuation candidate")
    );

    let subtree_solver = solver.with_subject("shape.scene_branch");
    let subtree_summary = subtree_solver.diagnostic_summary();
    assert_eq!(subtree_solver.subject.as_str(), "shape.scene_branch");
    assert_eq!(subtree_summary.subject.as_str(), "shape.scene_branch");
    assert!(
        subtree_summary
            .acceleration_rejection_classes
            .contains(&AccelerationRejectionClass::ArtifactUnavailable)
    );
    assert!(
        subtree_solver
            .mixed_selections()
            .iter()
            .all(|selection| selection.subject.as_str() == "shape.scene_branch")
    );
    assert!(
        subtree_summary
            .artifact_reuse_intents
            .iter()
            .all(|intent| intent.selection.subject.as_str() == "shape.scene_branch")
    );
    assert!(
        subtree_summary
            .continuation_intents
            .iter()
            .all(|intent| intent.selection.subject.as_str() == "shape.scene_branch")
    );

    let constructor_solver = wrela::query_solver::RaySolverPlan::for_contract_with_subject(
        query_contract::SPATIAL_NEAREST_WORLD,
        "shape.scene_branch",
        Some(SemanticEvidence::for_field_scene(sphere)),
    )
    .expect("subject-aware constructor");
    let constructor_summary = constructor_solver.diagnostic_summary();
    assert_eq!(constructor_solver.subject.as_str(), "shape.scene_branch");
    assert!(
        constructor_solver
            .mixed_selections()
            .iter()
            .all(|selection| selection.subject.as_str() == "shape.scene_branch")
    );
    assert!(
        constructor_summary
            .acceleration_rejection_classes
            .contains(&AccelerationRejectionClass::ArtifactUnavailable)
    );
    assert!(
        constructor_summary
            .artifact_reuse_intents
            .iter()
            .all(|intent| {
                intent.selection.subject.as_str() == "shape.scene_branch"
                    && intent
                        .selection
                        .evidence_policy_summary
                        .contains("subject=shape.scene_branch")
            })
    );
    assert!(
        constructor_summary
            .continuation_intents
            .iter()
            .all(|intent| {
                intent.selection.subject.as_str() == "shape.scene_branch"
                    && intent
                        .selection
                        .evidence_policy_summary
                        .contains("subject=shape.scene_branch")
            })
    );
}

#[test]
fn ray_solver_diagnostics_cover_major_acceleration_rejection_classes() {
    let module = lower_inline_module_from_source(acceleration_rejection_fixture_source());
    let scene = scene_ir::lower_module(&module);

    let opaque_summary = wrela::query_solver::RaySolverPlan::for_contract(
        query_contract::SPATIAL_NEAREST_WORLD,
        Some(SemanticEvidence::for_field_scene(
            scene.fields.get("opaque_field").expect("opaque field"),
        )),
    )
    .expect("opaque solver plan")
    .diagnostic_summary();
    assert!(
        opaque_summary
            .acceleration_rejection_classes
            .contains(&AccelerationRejectionClass::OpaqueBoundary)
    );

    let repeat_summary = wrela::query_solver::RaySolverPlan::for_contract(
        query_contract::SPATIAL_NEAREST_WORLD,
        Some(SemanticEvidence::for_field_scene(
            scene
                .fields
                .get("repeat_linear_field")
                .expect("repeat linear field"),
        )),
    )
    .expect("repeat solver plan")
    .diagnostic_summary();
    assert!(
        repeat_summary
            .acceleration_rejection_classes
            .contains(&AccelerationRejectionClass::UnsupportedRepeatForm)
    );

    let affine_summary = wrela::query_solver::RaySolverPlan::for_contract(
        query_contract::SPATIAL_NEAREST_WORLD,
        Some(SemanticEvidence::for_field_scene(
            scene.fields.get("affine_field").expect("affine field"),
        )),
    )
    .expect("affine solver plan")
    .diagnostic_summary();
    assert!(
        affine_summary
            .acceleration_rejection_classes
            .contains(&AccelerationRejectionClass::UnsupportedTransform)
    );

    let runtime_unknown_summary = wrela::query_solver::RaySolverPlan::for_contract(
        query_contract::SPATIAL_NEAREST_WORLD,
        Some(SemanticEvidence::runtime_unknown("runtime.unknown")),
    )
    .expect("runtime unknown solver plan")
    .diagnostic_summary();
    assert!(
        runtime_unknown_summary
            .acceleration_rejection_classes
            .contains(&AccelerationRejectionClass::UnboundedSupport)
    );
}

#[test]
fn ray_solver_diagnostics_surface_artifact_invalid_rejection_details() {
    let module = lower_inline_module_from_source(fact_fixture_source());
    let scene = scene_ir::lower_module(&module);
    let sphere = scene.fields.get("sphere_field").expect("sphere field");

    let artifact_bound = SemanticEvidence::for_field_scene(sphere)
        .with_subject("shape.artifact_bound")
        .artifact_bound("artifact cache");

    let summary = wrela::query_solver::RaySolverPlan::for_contract(
        query_contract::SPATIAL_NEAREST_WORLD,
        Some(artifact_bound),
    )
    .expect("artifact-bound solver plan")
    .diagnostic_summary();

    assert!(
        summary
            .acceleration_rejection_classes
            .contains(&AccelerationRejectionClass::ArtifactInvalid)
    );
    assert_eq!(
        summary.artifact_reuse_intents[0].disposition,
        RaySolverIntentDisposition::Rejected
    );
    assert!(
        summary.artifact_reuse_intents[0]
            .reasons
            .iter()
            .any(|reason| reason.contains("artifact-derived evidence"))
    );
    assert!(
        summary.artifact_reuse_intents[0]
            .reasons
            .iter()
            .any(|reason| reason.contains("artifact provenance"))
    );
    assert!(
        summary.artifact_reuse_intents[0]
            .selection
            .evidence_policy_summary
            .contains("ArtifactDerived")
    );
}
