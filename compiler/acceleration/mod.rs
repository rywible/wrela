pub mod report;

use crate::artifact_contract::{
    AccelerationArtifactKind, AccelerationArtifactMetadata, ArtifactCompatibilityRelation,
    ArtifactEvidenceCompatibility, ArtifactLogicalField, ArtifactLogicalSchema, ArtifactObserver,
    ArtifactPolicyCompatibility, ArtifactResidency, ArtifactSnapshotRelation,
    ArtifactTransitionRelation, ArtifactValidityPredicate, ArtifactValidityRule,
    SemanticArtifactContract, SemanticArtifactKind, acceleration_artifact_kind_name,
    validate_acceleration_artifact_contract,
};
use crate::artifact_key::ArtifactPolicyDigestMode;
use crate::semantic_evidence::{EvidenceOrigin, EvidenceScope, SemanticEvidenceSummary};
use crate::state_advance::{ChangeClass, ChangeCompatibility};
use smol_str::SmolStr;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AccelerationObserver {
    Query,
    Presentation,
    Collision,
}

pub type AccelerationObserverKind = AccelerationObserver;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AccelerationForestContractKind {
    SharedAccelerationForest,
    SharedUnionSubtreeForest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AccelerationNodeKind {
    ForestRoot,
    UnionCluster,
    SupportSummary,
    BoundProxy,
    RepeatRegion,
    TransformRegion,
    LeafCandidate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AccelerationCandidateClass {
    SpatialRay,
    SpatialPoint,
    PrimaryVisibility,
    ParticipantQuery,
    CollisionBroadphase,
    CollisionRefinement,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SupportDescriptorKind {
    Unknown,
    ConservativeLowerBound,
    ExactIntervalBound,
    OpaqueBoundary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BoundDescriptorKind {
    AxisAlignedBounds,
    SupportRadius,
    ClipmapBand,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AccelerationCacheKind {
    DistanceBrickCache,
    SupportBrickCache,
    RayCandidateTable,
    TileCandidateTable,
    ViewDistanceClipmap,
    ContinuationSeedTable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CacheArtifactScope {
    SharedSnapshot,
    ObserverLocal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CertificateProvenanceKind {
    SupportInterval,
    DistanceBound,
    SolverCertificate,
    TemporalContinuation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FallbackExpectation {
    None,
    ConservativeOnly,
    ExplicitSemanticWeakening,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupportDescriptor {
    pub kind: SupportDescriptorKind,
    pub semantics: SmolStr,
    pub opaque_boundary: bool,
    pub can_coarse_prune: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundDescriptor {
    pub kind: BoundDescriptorKind,
    pub summary: SmolStr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertificateProvenanceHandle {
    pub kind: CertificateProvenanceKind,
    pub handle: SmolStr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineageRecord {
    pub semantic_id: SmolStr,
    pub source_path: SmolStr,
    pub stable_order: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RayInterval {
    pub start_t: f32,
    pub end_t: f32,
    pub conservative: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RayEntryResult {
    pub node_id: SmolStr,
    pub entry_t: f32,
    pub exit_t: f32,
    pub accepted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccelerationCacheDescriptor {
    pub id: SmolStr,
    pub kind: AccelerationCacheKind,
    pub scope: CacheArtifactScope,
    pub observer: Option<AccelerationObserver>,
    pub artifact_scope: SmolStr,
    pub fallback_expectation: FallbackExpectation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObserverUsageSummary {
    pub observer: AccelerationObserver,
    pub contract_id: SmolStr,
    pub used_caches: Vec<SmolStr>,
    pub candidate_classes: Vec<AccelerationCandidateClass>,
    pub notes: Vec<SmolStr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccelerationNode {
    pub id: SmolStr,
    pub stable_order: u32,
    pub kind: AccelerationNodeKind,
    pub candidate_class: AccelerationCandidateClass,
    pub support: Option<SupportDescriptor>,
    pub bounds: Vec<BoundDescriptor>,
    pub lineage: Vec<LineageRecord>,
    pub certificate_handles: Vec<CertificateProvenanceHandle>,
    pub child_ids: Vec<SmolStr>,
    pub notes: Vec<SmolStr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccelerationForestContract {
    pub id: SmolStr,
    pub kind: AccelerationForestContractKind,
    pub forest_version: u32,
    pub candidate_class: AccelerationCandidateClass,
    pub root_nodes: Vec<SmolStr>,
    pub fallback_expectation: FallbackExpectation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccelerationForest {
    pub contract: AccelerationForestContract,
    pub nodes: Vec<AccelerationNode>,
    pub caches: Vec<AccelerationCacheDescriptor>,
    pub observer_usage: Vec<ObserverUsageSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccelerationReport {
    pub observer: AccelerationObserver,
    pub forests: Vec<AccelerationForest>,
    pub notes: Vec<SmolStr>,
}

impl AccelerationNode {
    pub fn new(
        id: impl Into<SmolStr>,
        stable_order: u32,
        kind: AccelerationNodeKind,
        candidate_class: AccelerationCandidateClass,
    ) -> Self {
        Self {
            id: id.into(),
            stable_order,
            kind,
            candidate_class,
            support: None,
            bounds: Vec::new(),
            lineage: Vec::new(),
            certificate_handles: Vec::new(),
            child_ids: Vec::new(),
            notes: Vec::new(),
        }
    }
}

impl AccelerationForest {
    pub fn new(
        contract: AccelerationForestContract,
        mut nodes: Vec<AccelerationNode>,
        mut caches: Vec<AccelerationCacheDescriptor>,
        mut observer_usage: Vec<ObserverUsageSummary>,
    ) -> Self {
        nodes.sort_by(|left, right| {
            left.stable_order
                .cmp(&right.stable_order)
                .then(left.id.cmp(&right.id))
        });
        caches.sort_by(|left, right| left.id.cmp(&right.id));
        observer_usage.sort_by(|left, right| {
            left.observer
                .cmp(&right.observer)
                .then(left.contract_id.cmp(&right.contract_id))
        });
        Self {
            contract,
            nodes,
            caches,
            observer_usage,
        }
    }
}

impl AccelerationReport {
    pub fn new(
        observer: AccelerationObserver,
        mut forests: Vec<AccelerationForest>,
        mut notes: Vec<SmolStr>,
    ) -> Self {
        forests.sort_by(|left, right| left.contract.id.cmp(&right.contract.id));
        notes.sort();
        Self {
            observer,
            forests,
            notes,
        }
    }

    pub fn debug_dump(&self) -> String {
        report::render_report_debug_dump(self)
    }
}

pub fn acceleration_report(
    observer: AccelerationObserver,
    forests: Vec<AccelerationForest>,
    notes: Vec<SmolStr>,
) -> AccelerationReport {
    AccelerationReport::new(observer, forests, notes)
}

pub fn acceleration_observer_name(observer: AccelerationObserver) -> &'static str {
    match observer {
        AccelerationObserver::Query => "query",
        AccelerationObserver::Presentation => "presentation",
        AccelerationObserver::Collision => "collision",
    }
}

pub fn acceleration_cache_kind_name(kind: AccelerationCacheKind) -> &'static str {
    match kind {
        AccelerationCacheKind::DistanceBrickCache => "distance_brick_cache",
        AccelerationCacheKind::SupportBrickCache => "support_brick_cache",
        AccelerationCacheKind::RayCandidateTable => "ray_candidate_table",
        AccelerationCacheKind::TileCandidateTable => "tile_candidate_table",
        AccelerationCacheKind::ViewDistanceClipmap => "view_distance_clipmap",
        AccelerationCacheKind::ContinuationSeedTable => "continuation_seed_table",
    }
}

pub fn cache_artifact_scope_name(scope: CacheArtifactScope) -> &'static str {
    match scope {
        CacheArtifactScope::SharedSnapshot => "shared_snapshot",
        CacheArtifactScope::ObserverLocal => "observer_local",
    }
}

fn observer_artifact_kinds(observer: AccelerationObserver) -> &'static [AccelerationArtifactKind] {
    match observer {
        AccelerationObserver::Query => &[
            AccelerationArtifactKind::SharedAccelerationForest,
            AccelerationArtifactKind::SharedUnionSubtreeForest,
            AccelerationArtifactKind::DistanceBrickCache,
            AccelerationArtifactKind::SupportBrickCache,
            AccelerationArtifactKind::RayCandidateTable,
        ],
        AccelerationObserver::Presentation => &[
            AccelerationArtifactKind::SharedAccelerationForest,
            AccelerationArtifactKind::SharedUnionSubtreeForest,
            AccelerationArtifactKind::DistanceBrickCache,
            AccelerationArtifactKind::SupportBrickCache,
            AccelerationArtifactKind::TileCandidateTable,
            AccelerationArtifactKind::ViewDistanceClipmap,
            AccelerationArtifactKind::ContinuationSeedTable,
        ],
        AccelerationObserver::Collision => &[
            AccelerationArtifactKind::SharedAccelerationForest,
            AccelerationArtifactKind::SharedUnionSubtreeForest,
            AccelerationArtifactKind::DistanceBrickCache,
            AccelerationArtifactKind::SupportBrickCache,
            AccelerationArtifactKind::RayCandidateTable,
            AccelerationArtifactKind::ContinuationSeedTable,
        ],
    }
}

fn artifact_residency(kind: AccelerationArtifactKind) -> ArtifactResidency {
    match kind {
        AccelerationArtifactKind::SharedAccelerationForest
        | AccelerationArtifactKind::SharedUnionSubtreeForest
        | AccelerationArtifactKind::DistanceBrickCache
        | AccelerationArtifactKind::SupportBrickCache => ArtifactResidency::SharedSnapshot,
        AccelerationArtifactKind::RayCandidateTable
        | AccelerationArtifactKind::TileCandidateTable
        | AccelerationArtifactKind::ViewDistanceClipmap
        | AccelerationArtifactKind::ContinuationSeedTable => ArtifactResidency::ObserverLocal,
    }
}

fn artifact_compatibility(kind: AccelerationArtifactKind) -> ArtifactCompatibilityRelation {
    let evidence = SemanticEvidenceSummary {
        origin: EvidenceOrigin::RuntimeObserved,
        scope: match artifact_residency(kind) {
            ArtifactResidency::SharedSnapshot => EvidenceScope::SnapshotLocal,
            ArtifactResidency::ObserverLocal => EvidenceScope::ArtifactBound,
        },
        ..SemanticEvidenceSummary::contract_bound()
    };
    match kind {
        AccelerationArtifactKind::ContinuationSeedTable => ArtifactCompatibilityRelation {
            snapshot: ArtifactSnapshotRelation::PreviousSnapshotEpoch,
            transition: ArtifactTransitionRelation {
                compatibility: Some(ChangeCompatibility::new(ChangeClass::Presentation)),
                requires_previous_snapshot: true,
            },
            policy: ArtifactPolicyCompatibility {
                mode: ArtifactPolicyDigestMode::CompatibleRange,
            },
            evidence: ArtifactEvidenceCompatibility {
                origin: evidence.origin,
                scope: evidence.scope,
            },
        },
        _ => ArtifactCompatibilityRelation {
            snapshot: ArtifactSnapshotRelation::ExactSnapshot,
            transition: ArtifactTransitionRelation {
                compatibility: None,
                requires_previous_snapshot: false,
            },
            policy: ArtifactPolicyCompatibility {
                mode: ArtifactPolicyDigestMode::CompatibleRange,
            },
            evidence: ArtifactEvidenceCompatibility {
                origin: evidence.origin,
                scope: evidence.scope,
            },
        },
    }
}

fn artifact_validity(kind: AccelerationArtifactKind) -> ArtifactValidityRule {
    match kind {
        AccelerationArtifactKind::ContinuationSeedTable => ArtifactValidityRule::all(vec![
            ArtifactValidityRule::predicate(
                ArtifactValidityPredicate::PreviousSnapshotMatchesStored,
            ),
            ArtifactValidityRule::predicate(
                ArtifactValidityPredicate::SnapshotLineageMatchesCurrent,
            ),
            ArtifactValidityRule::predicate(ArtifactValidityPredicate::PolicyDigestMatches),
            ArtifactValidityRule::predicate(ArtifactValidityPredicate::HistoryCompatibilityMatches),
            ArtifactValidityRule::predicate(ArtifactValidityPredicate::EvidenceSummaryMatches),
            ArtifactValidityRule::predicate(ArtifactValidityPredicate::MaxSnapshotAge(1)),
        ]),
        _ => ArtifactValidityRule::all(vec![
            ArtifactValidityRule::predicate(
                ArtifactValidityPredicate::CurrentSnapshotMatchesStored,
            ),
            ArtifactValidityRule::predicate(ArtifactValidityPredicate::PolicyDigestMatches),
            ArtifactValidityRule::predicate(ArtifactValidityPredicate::EvidenceSummaryMatches),
        ]),
    }
}

pub fn observer_acceleration_contracts(
    observer: AccelerationObserverKind,
    owner: &str,
) -> Vec<SemanticArtifactContract> {
    observer_artifact_kinds(observer)
        .iter()
        .copied()
        .map(|kind| {
            let usage_site = SmolStr::new(owner);
            let id = SmolStr::new(acceleration_artifact_kind_name(kind));
            let evidence_summary = SemanticEvidenceSummary {
                origin: EvidenceOrigin::RuntimeObserved,
                scope: match artifact_residency(kind) {
                    ArtifactResidency::SharedSnapshot => EvidenceScope::SnapshotLocal,
                    ArtifactResidency::ObserverLocal => EvidenceScope::ArtifactBound,
                },
                ..SemanticEvidenceSummary::contract_bound()
            };
            SemanticArtifactContract {
                id,
                kind: SemanticArtifactKind::Query,
                logical_schema: ArtifactLogicalSchema {
                    namespace: SmolStr::new("acceleration"),
                    name: SmolStr::new(acceleration_artifact_kind_name(kind)),
                    fields: vec![
                        ArtifactLogicalField::new("observer", acceleration_observer_name(observer)),
                        ArtifactLogicalField::new("usage_site", usage_site.clone()),
                        ArtifactLogicalField::new(
                            "scope",
                            match artifact_residency(kind) {
                                ArtifactResidency::SharedSnapshot => "shared_snapshot",
                                ArtifactResidency::ObserverLocal => "observer_local",
                            },
                        ),
                    ],
                },
                compatibility: artifact_compatibility(kind),
                acceleration: Some(AccelerationArtifactMetadata {
                    kind,
                    observer: match observer {
                        AccelerationObserver::Query => ArtifactObserver::Query,
                        AccelerationObserver::Presentation => ArtifactObserver::Presentation,
                        AccelerationObserver::Collision => ArtifactObserver::Collision,
                    },
                    residency: artifact_residency(kind),
                    usage_site,
                }),
                validity: artifact_validity(kind),
                producer: SmolStr::new(owner),
                consumer: SmolStr::new("shared_acceleration"),
                deterministic: true,
                version: 1,
                transition: None,
                evidence_summary,
            }
        })
        .collect()
}

pub fn validate_observer_acceleration_contracts(
    observer: AccelerationObserverKind,
    owner: &str,
    contracts: &[SemanticArtifactContract],
) -> Vec<SmolStr> {
    let mut errors = Vec::new();
    let mut seen_ids = BTreeSet::new();
    let relevant = contracts
        .iter()
        .filter(|contract| {
            contract
                .acceleration
                .as_ref()
                .is_some_and(|acceleration| acceleration.usage_site.as_str() == owner)
        })
        .collect::<Vec<_>>();

    if relevant.is_empty() {
        return vec![SmolStr::new(format!(
            "observer '{}' declared no acceleration artifacts",
            acceleration_observer_name(observer)
        ))];
    }
    if !relevant.iter().any(|contract| {
        contract
            .acceleration
            .as_ref()
            .is_some_and(|acceleration| acceleration.residency == ArtifactResidency::SharedSnapshot)
    }) {
        errors.push(SmolStr::new(format!(
            "observer '{}' must declare at least one shared snapshot acceleration artifact",
            acceleration_observer_name(observer)
        )));
    }
    if !relevant.iter().any(|contract| {
        contract
            .acceleration
            .as_ref()
            .is_some_and(|acceleration| acceleration.residency == ArtifactResidency::ObserverLocal)
    }) {
        errors.push(SmolStr::new(format!(
            "observer '{}' must declare at least one observer-local acceleration artifact",
            acceleration_observer_name(observer)
        )));
    }
    for contract in relevant {
        if !seen_ids.insert(contract.id.clone()) {
            errors.push(SmolStr::new(format!(
                "observer '{}' declares acceleration artifact '{}' more than once",
                acceleration_observer_name(observer),
                contract.id
            )));
        }
        if let Some(acceleration) = contract.acceleration.as_ref() {
            if acceleration.usage_site.as_str() != owner {
                errors.push(SmolStr::new(format!(
                    "acceleration artifact '{}' usage_site '{}' does not match owner '{}'",
                    contract.id, acceleration.usage_site, owner
                )));
            }
            let expected_observer = match observer {
                AccelerationObserver::Query => ArtifactObserver::Query,
                AccelerationObserver::Presentation => ArtifactObserver::Presentation,
                AccelerationObserver::Collision => ArtifactObserver::Collision,
            };
            if acceleration.observer != expected_observer {
                errors.push(SmolStr::new(format!(
                    "acceleration artifact '{}' declares observer '{}' but owner '{}' expects '{}'",
                    contract.id,
                    crate::artifact_contract::artifact_observer_name(acceleration.observer),
                    owner,
                    acceleration_observer_name(observer)
                )));
            }
        }
        errors.extend(
            validate_acceleration_artifact_contract(contract)
                .into_iter()
                .map(SmolStr::new),
        );
    }
    errors
}
