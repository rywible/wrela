use wrela::hir;
use wrela::hir::lower as hir_lower;
use wrela::kernel::{lower_batch_query_plan, lower_world_query_plan, validate_world_query_plan};
use wrela::parser::ast;
use wrela::parser::ast::AstNode;
use wrela::parser::parse;
use wrela::query_contract;
use wrela::query_plan::{self, BatchQueryKind, DispatchBackend, WorldQueryKind};
use wrela::query_solver::{
    AnalyticIntersectionStatus, FactAvailability, FieldFacts, LipschitzStatus, PrimitiveFact,
    RaySolverFallbackKind, RaySolverMethod, is_ray_shaped_spatial_contract,
};
use wrela::scene_ir;

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
fn field_facts_make_available_and_unavailable_solver_inputs_explicit() {
    let module = lower_inline_module_from_source(fact_fixture_source());
    let scene = scene_ir::lower_module(&module);
    let sphere = scene.fields.get("sphere_field").expect("sphere field");
    let sphere_facts = FieldFacts::for_field_scene(sphere);
    assert_eq!(
        sphere_facts.primitive,
        PrimitiveFact::Single(hir::FieldPrimitive::Sphere)
    );
    assert_eq!(
        sphere_facts.support.semantics,
        scene_ir::DistanceSemantics::ExactSignedDistance
    );
    assert_eq!(
        sphere_facts.support.conservative_bounds,
        FactAvailability::Available
    );
    assert_eq!(sphere_facts.derivative, FactAvailability::Available);
    assert_eq!(sphere_facts.lipschitz, LipschitzStatus::ExactKnown);
    assert_eq!(
        sphere_facts.analytic_intersection,
        AnalyticIntersectionStatus::CandidateOnly
    );
    assert_eq!(sphere_facts.interval_bounds, FactAvailability::Unavailable);
    assert!(sphere_facts.unavailable_labels().contains(&"interval"));

    let opaque = scene.fields.get("opaque_field").expect("opaque field");
    let opaque_facts = FieldFacts::for_field_scene(opaque);
    assert_eq!(
        opaque_facts.support.semantics,
        scene_ir::DistanceSemantics::UnknownOpaque
    );
    assert_eq!(
        opaque_facts.support.lower_bound_pruning,
        FactAvailability::Unavailable
    );
    assert_eq!(opaque_facts.derivative, FactAvailability::Unavailable);
    assert_eq!(opaque_facts.lipschitz, LipschitzStatus::Unavailable);
    assert!(
        opaque_facts.unavailable_labels().contains(&"analytic"),
        "unavailable analytic facts must be visible in reports"
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
        solver.fallback.kind,
        RaySolverFallbackKind::ExactDenseSphereTracing
    );
    assert!(
        solver.method_enabled(RaySolverMethod::DenseSphereTracing),
        "dense marching must be an explicit solver fallback method"
    );
    let summary = solver.diagnostic_summary();
    assert!(
        summary
            .methods
            .contains(&RaySolverMethod::AnalyticPrimitiveIntersection),
        "runtime-unknown world plans should report conditional analytic hooks"
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
