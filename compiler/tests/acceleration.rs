use smol_str::SmolStr;
use wrela::acceleration::report::{AccelerationForest, AccelerationNode, AccelerationReport};
use wrela::acceleration::{
    self, AccelerationCacheDescriptor, AccelerationCacheKind, AccelerationCandidateClass,
    AccelerationForestContract, AccelerationForestContractKind, AccelerationNodeKind,
    AccelerationObserver, CacheArtifactScope, FallbackExpectation, ObserverUsageSummary,
};
use wrela::semantic_evidence::EvidenceScope;

#[test]
fn acceleration_report_dumps_deterministically() {
    let _query_contracts =
        acceleration::observer_acceleration_contracts(AccelerationObserver::Query, "query_helper");
    let _collision_contracts = acceleration::observer_acceleration_contracts(
        AccelerationObserver::Collision,
        "collision_plan",
    );

    let report = AccelerationReport::new(
        AccelerationObserver::Collision,
        vec![
            AccelerationForest::new(
                AccelerationForestContract {
                    id: SmolStr::new("z_forest"),
                    kind: AccelerationForestContractKind::SharedUnionSubtreeForest,
                    forest_version: 1,
                    candidate_class: AccelerationCandidateClass::CollisionRefinement,
                    root_nodes: vec![SmolStr::new("node_b"), SmolStr::new("node_a")],
                    fallback_expectation: FallbackExpectation::ExplicitSemanticWeakening,
                },
                vec![
                    AccelerationNode::new(
                        "node_b",
                        20,
                        AccelerationNodeKind::LeafCandidate,
                        AccelerationCandidateClass::CollisionRefinement,
                    ),
                    AccelerationNode::new(
                        "node_a",
                        10,
                        AccelerationNodeKind::ForestRoot,
                        AccelerationCandidateClass::CollisionBroadphase,
                    ),
                ],
                vec![AccelerationCacheDescriptor {
                    id: SmolStr::new("support_brick_cache"),
                    kind: AccelerationCacheKind::SupportBrickCache,
                    scope: CacheArtifactScope::ObserverLocal,
                    observer: Some(AccelerationObserver::Collision),
                    artifact_scope: SmolStr::new("collision_plan"),
                    fallback_expectation: FallbackExpectation::ConservativeOnly,
                }],
                vec![ObserverUsageSummary {
                    observer: AccelerationObserver::Collision,
                    contract_id: SmolStr::new("collision_plan"),
                    used_caches: vec![SmolStr::new("support_brick_cache")],
                    candidate_classes: vec![AccelerationCandidateClass::CollisionBroadphase],
                    notes: vec![SmolStr::new("z")],
                }],
            ),
            AccelerationForest::new(
                AccelerationForestContract {
                    id: SmolStr::new("a_forest"),
                    kind: AccelerationForestContractKind::SharedAccelerationForest,
                    forest_version: 1,
                    candidate_class: AccelerationCandidateClass::SpatialRay,
                    root_nodes: vec![SmolStr::new("node_z"), SmolStr::new("node_a")],
                    fallback_expectation: FallbackExpectation::None,
                },
                vec![
                    AccelerationNode::new(
                        "node_z",
                        2,
                        AccelerationNodeKind::RepeatRegion,
                        AccelerationCandidateClass::SpatialRay,
                    ),
                    AccelerationNode::new(
                        "node_a",
                        1,
                        AccelerationNodeKind::UnionCluster,
                        AccelerationCandidateClass::PrimaryVisibility,
                    ),
                ],
                vec![
                    AccelerationCacheDescriptor {
                        id: SmolStr::new("ray_candidate_table"),
                        kind: AccelerationCacheKind::RayCandidateTable,
                        scope: CacheArtifactScope::SharedSnapshot,
                        observer: Some(AccelerationObserver::Query),
                        artifact_scope: SmolStr::new("query_helper"),
                        fallback_expectation: FallbackExpectation::None,
                    },
                    AccelerationCacheDescriptor {
                        id: SmolStr::new("distance_brick_cache"),
                        kind: AccelerationCacheKind::DistanceBrickCache,
                        scope: CacheArtifactScope::ObserverLocal,
                        observer: Some(AccelerationObserver::Query),
                        artifact_scope: SmolStr::new("query_helper"),
                        fallback_expectation: FallbackExpectation::ConservativeOnly,
                    },
                ],
                vec![ObserverUsageSummary {
                    observer: AccelerationObserver::Query,
                    contract_id: SmolStr::new("query_helper"),
                    used_caches: vec![
                        SmolStr::new("distance_brick_cache"),
                        SmolStr::new("ray_candidate_table"),
                    ],
                    candidate_classes: vec![AccelerationCandidateClass::SpatialRay],
                    notes: vec![SmolStr::new("q")],
                }],
            ),
        ],
        vec![SmolStr::new("beta"), SmolStr::new("alpha")],
    );

    let dump = report.debug_dump();
    assert_eq!(dump, format!("{}", report));
    assert!(dump.contains("acceleration-report observer=collision"));
    assert!(dump.contains("note alpha"));
    assert!(dump.contains("note beta"));
    assert!(dump.contains("forest id=a_forest"));
    assert!(dump.contains("forest id=z_forest"));
    assert!(dump.contains("node id=node_a order=1 kind=UnionCluster"));
    assert!(dump.contains("node id=node_b order=20 kind=LeafCandidate"));
    assert!(dump.contains("cache id=distance_brick_cache"));
    assert!(dump.contains("usage observer=query contract=query_helper"));
    assert!(format!("{}", report.forests[0]).contains("forest id=a_forest"));
    assert!(format!("{}", report.forests[0].nodes[0]).contains("node id=node_a"));
}

#[test]
fn acceleration_contracts_validate_observer_scope_pairs() {
    let contracts =
        acceleration::observer_acceleration_contracts(AccelerationObserver::Presentation, "view");
    assert!(
        contracts
            .iter()
            .any(|contract| contract.compatibility.evidence.scope == EvidenceScope::SnapshotLocal)
    );
    assert!(
        contracts
            .iter()
            .any(|contract| contract.compatibility.evidence.scope == EvidenceScope::ArtifactBound)
    );
    assert!(
        acceleration::validate_observer_acceleration_contracts(
            AccelerationObserver::Presentation,
            "view",
            &contracts,
        )
        .is_empty()
    );

    let mut incompatible = contracts.clone();
    incompatible[0]
        .acceleration
        .as_mut()
        .expect("acceleration")
        .observer = wrela::artifact_contract::ArtifactObserver::Collision;
    assert!(
        !acceleration::validate_observer_acceleration_contracts(
            AccelerationObserver::Presentation,
            "view",
            &incompatible,
        )
        .is_empty()
    );
}
