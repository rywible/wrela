use wrela::hir;
use wrela::hir::lower as hir_lower;
use wrela::parser::ast;
use wrela::parser::ast::AstNode;
use wrela::parser::parse;
use wrela::query_contract;
use wrela::query_solver::{
    RaySolverFallbackReason, RaySolverHitBracketStatus, RaySolverMethod, RaySolverNoCloserHitProof,
    RequiredGuaranteeClass, SelectedMethodClass,
};
use wrela::scene_ir;
use wrela::semantic_evidence::SemanticEvidence;

fn lower_inline_module_from_source(source: &str) -> hir::Module {
    let node = parse(source);
    let root = ast::Root::cast(node).expect("root");
    hir_lower::lower(root)
}

fn interval_fixture_source() -> &'static str {
    r#"
field exact distance sphere_field(p: Vec3) -> F32 {
    sphere(radius = 1.0)
}

field exact distance torus_field(p: Vec3) -> F32 {
    torus(major_radius = 0.72, minor_radius = 0.22)
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
fn ray_solver_plan_reports_interval_certificate_shape_for_trusted_exact_sphere() {
    let module = lower_inline_module_from_source(interval_fixture_source());
    let scene = scene_ir::lower_module(&module);
    let sphere = scene.fields.get("sphere_field").expect("sphere field");

    let solver = wrela::query_solver::RaySolverPlan::for_contract(
        query_contract::SPATIAL_NEAREST_WORLD,
        Some(SemanticEvidence::for_field_scene(sphere)),
    )
    .expect("sphere solver plan");

    assert!(solver.method_enabled(RaySolverMethod::IntervalNewtonIsolation));
    assert!(solver.method_enabled(RaySolverMethod::SafeguardedNewtonRefinement));
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
    assert!(solver.mixed_selections().iter().any(|selection| {
        selection.method == RaySolverMethod::IntervalNewtonIsolation
            && selection.required_guarantee == RequiredGuaranteeClass::IntervalBounded
            && selection.selected_method_class == SelectedMethodClass::IntervalSolver
            && selection.candidate_class == "interval-isolation-candidates"
    }));
    assert!(solver.mixed_selections().iter().any(|selection| {
        selection.method == RaySolverMethod::SafeguardedNewtonRefinement
            && selection.required_guarantee == RequiredGuaranteeClass::IntervalBounded
            && selection.selected_method_class == SelectedMethodClass::IntervalSolver
            && selection.candidate_class == "newton-refinement-candidates"
    }));
    assert_eq!(
        solver.diagnostic_summary().methods,
        vec![
            RaySolverMethod::DenseSphereTracing,
            RaySolverMethod::SupportBoundCandidateRejection,
            RaySolverMethod::AnalyticPrimitiveIntersection,
            RaySolverMethod::LipschitzSafeStepping,
            RaySolverMethod::IntervalNewtonIsolation,
            RaySolverMethod::SafeguardedNewtonRefinement,
        ]
    );
}

#[test]
fn ray_solver_plan_keeps_torus_on_dense_default_path() {
    let module = lower_inline_module_from_source(interval_fixture_source());
    let scene = scene_ir::lower_module(&module);
    let torus = scene.fields.get("torus_field").expect("torus field");

    let solver = wrela::query_solver::RaySolverPlan::for_contract(
        query_contract::SPATIAL_NEAREST_WORLD,
        Some(SemanticEvidence::for_field_scene(torus)),
    )
    .expect("torus solver plan");

    assert!(!solver.method_enabled(RaySolverMethod::LipschitzSafeStepping));
    assert!(!solver.method_enabled(RaySolverMethod::IntervalNewtonIsolation));
    assert!(!solver.method_enabled(RaySolverMethod::SafeguardedNewtonRefinement));
    assert_eq!(
        solver.certificate.method,
        RaySolverMethod::DenseSphereTracing
    );
    assert_eq!(
        solver.certificate.hit_bracket,
        RaySolverHitBracketStatus::Unavailable
    );
    assert_eq!(
        solver.certificate.no_closer_hit_proof,
        RaySolverNoCloserHitProof::Available
    );
    assert!(
        !solver.mixed_selections().iter().any(|selection| {
            matches!(
                selection.method,
                RaySolverMethod::LipschitzSafeStepping
                    | RaySolverMethod::IntervalNewtonIsolation
                    | RaySolverMethod::SafeguardedNewtonRefinement
            )
        }),
        "torus should not advertise the generic relaxed/interval/refinement stack by default"
    );
    assert_eq!(
        solver.diagnostic_summary().methods,
        vec![
            RaySolverMethod::DenseSphereTracing,
            RaySolverMethod::SupportBoundCandidateRejection,
        ]
    );
}

#[test]
fn ray_solver_plan_marks_interval_proof_as_unavailable_for_opaque_fields() {
    let module = lower_inline_module_from_source(interval_fixture_source());
    let scene = scene_ir::lower_module(&module);
    let opaque = scene.fields.get("opaque_field").expect("opaque field");

    let solver = wrela::query_solver::RaySolverPlan::for_contract(
        query_contract::SPATIAL_NEAREST_WORLD,
        Some(SemanticEvidence::for_field_scene(opaque)),
    )
    .expect("opaque solver plan");

    assert!(!solver.method_enabled(RaySolverMethod::IntervalNewtonIsolation));
    assert!(!solver.method_enabled(RaySolverMethod::SafeguardedNewtonRefinement));
    assert_eq!(
        solver.certificate.hit_bracket,
        RaySolverHitBracketStatus::Unavailable
    );
    assert_eq!(
        solver.certificate.no_closer_hit_proof,
        RaySolverNoCloserHitProof::Unavailable
    );
    assert_eq!(
        solver.dense_fallback_reasons(),
        &[
            RaySolverFallbackReason::ContractRequiresDenseOracle,
            RaySolverFallbackReason::AnalyticUnsupported,
            RaySolverFallbackReason::MissingFieldFacts,
        ]
    );
    assert_eq!(
        solver.diagnostic_summary().methods,
        vec![RaySolverMethod::DenseSphereTracing]
    );
}
