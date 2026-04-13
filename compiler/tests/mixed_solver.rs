use wrela::hir;
use wrela::hir::lower as hir_lower;
use wrela::parser::ast;
use wrela::parser::ast::AstNode;
use wrela::parser::parse;
use wrela::query_contract;
use wrela::query_solver::{
    RaySolverIntentDisposition, RaySolverMethod, RequiredGuaranteeClass, SelectedMethodClass,
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
    assert_eq!(summary.mixed_selections.len(), 4);
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
    assert_eq!(summary.artifact_reuse_intents.len(), 1);
    assert_eq!(summary.continuation_intents.len(), 1);
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
