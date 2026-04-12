use wrela::hir;
use wrela::hir::lower as hir_lower;
use wrela::parser::ast;
use wrela::parser::ast::AstNode;
use wrela::parser::parse;
use wrela::query_contract;
use wrela::query_solver::{
    FactAvailability, RaySolverFallbackReason, RaySolverMethod, is_ray_shaped_spatial_contract,
};
use wrela::scene_ir;
use wrela::semantic_evidence::{EvidenceOrigin, EvidenceScope, SemanticEvidence};

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
fn ray_solver_plan_uses_semantic_evidence_for_fallbacks() {
    assert!(is_ray_shaped_spatial_contract(
        query_contract::SPATIAL_NEAREST_WORLD
    ));

    let module = lower_inline_module_from_source(fact_fixture_source());
    let scene = scene_ir::lower_module(&module);
    let sphere = scene.fields.get("sphere_field").expect("sphere field");
    let opaque = scene.fields.get("opaque_field").expect("opaque field");

    let sphere_evidence = SemanticEvidence::for_field_scene(sphere);
    let solver = wrela::query_solver::RaySolverPlan::for_contract(
        query_contract::SPATIAL_NEAREST_WORLD,
        Some(sphere_evidence),
    )
    .expect("ray solver plan");
    assert_eq!(
        solver.evidence.summary().origin,
        EvidenceOrigin::StaticCompiled
    );
    assert_eq!(
        solver.evidence.summary().scope,
        EvidenceScope::CompileInvariant
    );
    assert_eq!(
        solver.evidence.support.lower_bound_pruning,
        FactAvailability::Available
    );
    assert!(solver.method_enabled(RaySolverMethod::DenseSphereTracing));
    let summary = solver.diagnostic_summary();
    assert_eq!(
        summary.evidence_summary.origin,
        EvidenceOrigin::StaticCompiled
    );
    assert_eq!(
        summary.evidence_summary.scope,
        EvidenceScope::CompileInvariant
    );
    assert!(!summary.unavailable_facts.contains(&"analytic"));

    let opaque_solver = wrela::query_solver::RaySolverPlan::for_contract(
        query_contract::SPATIAL_OCCLUDED_WORLD,
        Some(SemanticEvidence::for_field_scene(opaque)),
    )
    .expect("opaque solver");
    assert!(
        opaque_solver
            .dense_fallback_reasons()
            .contains(&RaySolverFallbackReason::MissingFieldFacts),
        "opaque evidence must still surface missing field facts"
    );
    assert!(
        opaque_solver
            .dense_fallback_reasons()
            .contains(&RaySolverFallbackReason::AnalyticUnsupported),
        "opaque evidence must keep the analytic fallback reason visible"
    );

    let runtime_solver = wrela::query_solver::RaySolverPlan::for_contract(
        query_contract::SPATIAL_NEAREST_WORLD,
        None,
    )
    .expect("runtime solver");
    assert_eq!(
        runtime_solver.evidence.summary().origin,
        EvidenceOrigin::RuntimeObserved
    );
    assert_eq!(
        runtime_solver.evidence.summary().scope,
        EvidenceScope::SnapshotLocal
    );
}
