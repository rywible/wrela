use wrela::hir;
use wrela::hir::lower as hir_lower;
use wrela::parser::ast;
use wrela::parser::ast::AstNode;
use wrela::parser::parse;
use wrela::query_solver::{AnalyticIntersectionStatus, FactAvailability, LipschitzStatus};
use wrela::scene_ir;
use wrela::semantic_evidence::{
    EvidenceOrigin, EvidenceRefinementKind, EvidenceScope, RuntimeBoundsEvidence, SemanticEvidence,
    TemporalChangeClass,
};

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
fn semantic_evidence_refinement_is_monotone() {
    let runtime_unknown = SemanticEvidence::runtime_unknown("world.region.runtime");
    let refined = runtime_unknown.clone().refine_with_runtime_bounds(
        RuntimeBoundsEvidence {
            lower_bound_pruning: FactAvailability::Available,
            interval_bounds: FactAvailability::Available,
            lipschitz: Some(LipschitzStatus::ConservativeKnown),
        },
        "runtime bounds",
    );
    assert_eq!(
        refined.distance.lipschitz,
        LipschitzStatus::ConservativeKnown
    );
    assert_eq!(
        refined.support.lower_bound_pruning,
        FactAvailability::Available
    );
    assert_eq!(
        refined.distance.interval_bounds,
        FactAvailability::Available
    );
    assert_eq!(refined.summary().origin, EvidenceOrigin::RuntimeObserved);
    assert_eq!(refined.summary().scope, EvidenceScope::SnapshotLocal);
    assert_eq!(
        refined.summary().temporal.change_class,
        TemporalChangeClass::Unknown
    );
    assert_eq!(
        refined.summary().temporal.stationary,
        FactAvailability::Unknown
    );

    let weakened = refined.clone().weaken_for_warp("warp weakening");
    assert_eq!(
        weakened.support.lower_bound_pruning,
        FactAvailability::Unknown
    );
    assert_eq!(
        weakened.distance.analytic_intersection,
        AnalyticIntersectionStatus::Unavailable
    );
    assert_eq!(weakened.summary().origin, EvidenceOrigin::RuntimeObserved);
}

#[test]
fn field_scene_evidence_remains_static_after_subject_overlay() {
    let module = lower_inline_module_from_source(fact_fixture_source());
    let scene = scene_ir::lower_module(&module);
    let sphere = scene.fields.get("sphere_field").expect("sphere field");

    let evidence = SemanticEvidence::for_field_scene(sphere).with_subject("shape.sphere");

    assert_eq!(evidence.subject, "shape.sphere",);
    assert!(!evidence.identity.stable_feature_id);
    assert_eq!(evidence.summary().origin, EvidenceOrigin::StaticCompiled);
    assert_eq!(
        evidence.summary().temporal.topology_stable,
        FactAvailability::Available
    );
}

#[test]
fn semantic_evidence_summary_round_trip_preserves_refinement_history() {
    let summary = SemanticEvidence::runtime_unknown("world.region.runtime")
        .refine_with_runtime_bounds(
            RuntimeBoundsEvidence {
                lower_bound_pruning: FactAvailability::Available,
                interval_bounds: FactAvailability::Available,
                lipschitz: Some(LipschitzStatus::ConservativeKnown),
            },
            "runtime bounds",
        )
        .artifact_bound("artifact cache")
        .summary();
    let round_trip = SemanticEvidence::from_summary(&summary).summary();

    assert_eq!(round_trip, summary);
    assert_eq!(summary.refinement_path.len(), 3);
    assert_eq!(
        summary.refinement_path[0].kind,
        EvidenceRefinementKind::RuntimeObservation
    );
    assert_eq!(
        summary.refinement_path[1].kind,
        EvidenceRefinementKind::RuntimeBounds
    );
    assert_eq!(
        summary.refinement_path[2].kind,
        EvidenceRefinementKind::ArtifactBinding
    );
    assert_eq!(
        summary.distance.refinement_path,
        round_trip.distance.refinement_path
    );
    assert_eq!(
        summary.support.refinement_path,
        round_trip.support.refinement_path
    );
    assert_eq!(
        summary.differential.refinement_path,
        round_trip.differential.refinement_path
    );
    assert_eq!(
        summary.identity.refinement_path,
        round_trip.identity.refinement_path
    );
    assert_eq!(
        summary.temporal.refinement_path,
        round_trip.temporal.refinement_path
    );
    assert_eq!(
        summary.temporal.change_class,
        round_trip.temporal.change_class
    );
    assert_eq!(summary.temporal.stationary, round_trip.temporal.stationary);
    assert_eq!(
        summary.temporal.rigid_over_interval,
        round_trip.temporal.rigid_over_interval
    );
    assert_eq!(
        summary.temporal.topology_stable,
        round_trip.temporal.topology_stable
    );
    assert_eq!(
        summary.temporal.bounded_velocity,
        round_trip.temporal.bounded_velocity
    );
    assert_eq!(
        summary
            .distance
            .refinement_path
            .iter()
            .map(|step| step.kind)
            .collect::<Vec<_>>(),
        vec![
            EvidenceRefinementKind::RuntimeObservation,
            EvidenceRefinementKind::RuntimeBounds,
            EvidenceRefinementKind::ArtifactBinding,
        ]
    );
    assert_eq!(
        summary
            .support
            .refinement_path
            .iter()
            .map(|step| step.kind)
            .collect::<Vec<_>>(),
        vec![
            EvidenceRefinementKind::RuntimeObservation,
            EvidenceRefinementKind::RuntimeBounds,
            EvidenceRefinementKind::ArtifactBinding,
        ]
    );
    assert_eq!(
        summary
            .differential
            .refinement_path
            .iter()
            .map(|step| step.kind)
            .collect::<Vec<_>>(),
        vec![
            EvidenceRefinementKind::RuntimeObservation,
            EvidenceRefinementKind::ArtifactBinding,
        ]
    );
    assert_eq!(
        summary
            .identity
            .refinement_path
            .iter()
            .map(|step| step.kind)
            .collect::<Vec<_>>(),
        vec![
            EvidenceRefinementKind::RuntimeObservation,
            EvidenceRefinementKind::ArtifactBinding,
        ]
    );
    assert_eq!(
        summary
            .temporal
            .refinement_path
            .iter()
            .map(|step| step.kind)
            .collect::<Vec<_>>(),
        vec![
            EvidenceRefinementKind::RuntimeObservation,
            EvidenceRefinementKind::RuntimeBounds,
            EvidenceRefinementKind::ArtifactBinding,
        ]
    );
}

#[test]
fn temporal_stability_scope_and_change_class_remain_distinct() {
    let evidence = SemanticEvidence::runtime_unknown("world.region.runtime")
        .refine_with_temporal_stability(
            FactAvailability::Unavailable,
            FactAvailability::Available,
            FactAvailability::Available,
            FactAvailability::Available,
            "rigid interval evidence",
        )
        .summary();

    assert_eq!(evidence.scope, EvidenceScope::SnapshotLocal);
    assert_eq!(evidence.temporal.scope, EvidenceScope::TransitionCompatible);
    assert_eq!(evidence.temporal.change_class, TemporalChangeClass::Unknown);
    assert_eq!(evidence.temporal.stationary, FactAvailability::Unavailable);
    assert_eq!(
        evidence.temporal.rigid_over_interval,
        FactAvailability::Available
    );
    assert_eq!(
        evidence.temporal.topology_stable,
        FactAvailability::Available
    );
    assert_eq!(
        evidence.temporal.bounded_velocity,
        FactAvailability::Available
    );
}
