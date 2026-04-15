use crate::acceleration::{self, AccelerationObserverKind};
use crate::artifact_contract::{
    ArtifactCompatibilityRelation, ArtifactEvidenceCompatibility, ArtifactLogicalField,
    ArtifactLogicalSchema, ArtifactPolicyCompatibility, ArtifactSnapshotRelation,
    ArtifactTransitionRelation, ArtifactUse, ArtifactUseKind, ArtifactUseSource,
    ArtifactValidityPredicate, ArtifactValidityRule, SemanticArtifactContract,
    SemanticArtifactKind,
};
use crate::artifact_key::{ArtifactPolicyDigestMode, stable_history_compatibility_hash};
use crate::artifact_store::{ArtifactLookupReport, ArtifactStoreReport, store_backed_use};
use crate::collision_contract::{
    COLLISION_POINT_OCCUPANCY_WORLD, COLLISION_RAY_CAST_WORLD, COLLISION_SPHERE_OVERLAP_WORLD,
    COLLISION_SPHERE_SWEEP_TRANSITION, COLLISION_TIME_OF_IMPACT_TRANSITION,
    CollisionAuthorityScope, CollisionContactNormalFlavor, CollisionContractDescriptor,
    CollisionContractId, CollisionExecutionPolicy, CollisionFamilyId, CollisionInputKind,
    CollisionOutputKind, CollisionQuestionId, CollisionResult, CollisionSnapshotTransitionInput,
    CollisionTargetKind, CollisionWitnessSchema, collision_authority_scope_name,
    collision_contact_normal_flavor_name, collision_contract, collision_input_kind_name,
    collision_output_kind_name, collision_target_name,
};
use crate::execution_policy::{RequiredGuaranteeClass, SelectedMethodClass};
use crate::kernel::KernelValue;
use crate::query_contract::{self, DispatchBackend, QueryContractId};
use crate::query_exec::QueryExecContext;
use crate::semantic_evidence::{EvidenceScope, SemanticEvidenceSummary};
use crate::world_identity::SnapshotIdentityReport;
use smol_str::SmolStr;
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

pub const COLLISION_PLAN_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CollisionQueryKind {
    PointOccupancyWorld,
    RayCastWorld,
    SphereOverlapWorld,
    SphereSweepTransition,
    SphereTimeOfImpactTransition,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollisionInputBinding {
    pub name: SmolStr,
    pub kind: CollisionInputKind,
    pub record: SmolStr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollisionOutputBinding {
    pub name: SmolStr,
    pub kind: CollisionOutputKind,
    pub record: SmolStr,
    pub witness_schema: Option<&'static CollisionWitnessSchema>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CollisionArtifactKind {
    SupportSummary,
    BroadphaseCandidates,
    WitnessCache,
    ContinuationSeed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollisionArtifactBinding {
    pub id: SmolStr,
    pub kind: CollisionArtifactKind,
    pub record: SmolStr,
    pub contract: SemanticArtifactContract,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CollisionPassKind {
    GatherCandidates {
        support_summary_contract: QueryContractId,
        support_artifact: SmolStr,
    },
    BuildBroadphaseCandidates {
        support_artifact: SmolStr,
        artifact_id: SmolStr,
    },
    EvaluatePointOccupancy {
        distance_contract: QueryContractId,
        normal_contract: QueryContractId,
        support_artifact: SmolStr,
        broadphase_artifact: SmolStr,
    },
    CastRayFirstHit {
        trace_contract: QueryContractId,
        support_artifact: SmolStr,
        broadphase_artifact: SmolStr,
    },
    ResolveSphereOverlap {
        distance_contract: QueryContractId,
        normal_contract: QueryContractId,
        support_artifact: SmolStr,
        broadphase_artifact: SmolStr,
        supported_shape: SmolStr,
    },
    SweepSphereFirstContact {
        distance_contract: QueryContractId,
        normal_contract: QueryContractId,
        support_artifact: SmolStr,
        broadphase_artifact: SmolStr,
        witness_artifact: SmolStr,
        continuation_artifact: SmolStr,
    },
    ResolveSphereTimeOfImpact {
        distance_contract: QueryContractId,
        normal_contract: QueryContractId,
        support_artifact: SmolStr,
        broadphase_artifact: SmolStr,
        witness_artifact: SmolStr,
        continuation_artifact: SmolStr,
    },
    MaterializeOutput {
        output: CollisionOutputKind,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollisionPass {
    pub id: SmolStr,
    pub kind: CollisionPassKind,
    pub consumes: Vec<SmolStr>,
    pub materializes: Vec<SmolStr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollisionPlan {
    pub name: SmolStr,
    pub contract_id: CollisionContractId,
    pub contract_version: u32,
    pub family: CollisionFamilyId,
    pub question: CollisionQuestionId,
    pub target: CollisionTargetKind,
    pub authority_scope: CollisionAuthorityScope,
    pub backend: DispatchBackend,
    pub policy: CollisionExecutionPolicy,
    pub inputs: Vec<CollisionInputBinding>,
    pub passes: Vec<CollisionPass>,
    pub artifacts: Vec<CollisionArtifactBinding>,
    pub outputs: Vec<CollisionOutputBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollisionPlanValidationError {
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollisionReuseVerdict {
    Consumed,
    Rejected,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollisionReuseReason {
    None,
    MissingPreviousSnapshot,
    CompatibilityRejected,
    ValidityRejected,
    ArtifactUnavailable,
    RenderingOnlyCertificate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollisionReuseDecision {
    pub artifact_id: SmolStr,
    pub artifact_kind: CollisionArtifactKind,
    pub verdict: CollisionReuseVerdict,
    pub reason: CollisionReuseReason,
    pub detail: SmolStr,
    pub lookup: Option<ArtifactLookupReport>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CollisionReuseMetrics {
    pub available_count: u32,
    pub consumed_count: u32,
    pub rejected_count: u32,
    pub unavailable_count: u32,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CollisionExecutionTrace {
    pub contract_id: CollisionContractId,
    pub family: CollisionFamilyId,
    pub question: CollisionQuestionId,
    pub backend: DispatchBackend,
    pub snapshot: Option<SnapshotIdentityReport>,
    pub transition: Option<CollisionSnapshotTransitionInput>,
    pub required_guarantee: RequiredGuaranteeClass,
    pub selected_method: SelectedMethodClass,
    pub executed_query_contracts: Vec<QueryContractId>,
    pub artifact_store: ArtifactStoreReport,
    pub broadphase_candidate_count: u32,
    pub broadphase_rejected_candidate_count: u32,
    pub broadphase_pruned_node_count: u32,
    pub interval_bracket: Option<[f32; 2]>,
    pub interval_subdivisions: u32,
    pub interval_refinements: u32,
    pub certificate_successes: u32,
    pub fallback_count: u32,
    pub contact_normal_provenance:
        Option<crate::collision_contract::CollisionContactNormalProvenance>,
    pub reuse_metrics: CollisionReuseMetrics,
    pub reuse_decisions: Vec<CollisionReuseDecision>,
}

#[derive(Debug, Error, Clone, PartialEq)]
pub enum CollisionExecError {
    #[error("collision plan validation failed: {messages:?}")]
    Validation { messages: Vec<String> },
    #[error("collision execution expected {expected}, found {found}")]
    TypeMismatch { expected: String, found: String },
    #[error("collision execution expected a region capture input")]
    MissingRegionCapture,
    #[error("collision execution expected a transition input")]
    MissingTransitionInput,
    #[error("collision transition current epoch {expected} does not match snapshot epoch {found}")]
    TransitionEpochMismatch { expected: u32, found: u32 },
    #[error("collision transition authority exceeded: observed {observed:?}, maximum {maximum:?}")]
    TransitionAuthorityExceeded {
        observed: crate::state_advance::ChangeClass,
        maximum: crate::state_advance::ChangeClass,
    },
    #[error("collision snapshot handle could not be resolved")]
    MissingSnapshotHandle,
    #[error("collision plan is missing required input binding '{kind}'")]
    MissingInputBinding { kind: String },
    #[error("collision plan is missing required output binding '{output}'")]
    MissingOutputBinding { output: String },
    #[error("collision artifact '{artifact_id}' was not declared")]
    MissingArtifact { artifact_id: SmolStr },
    #[error("collision value '{record}' is missing required field '{field}'")]
    MissingField { record: String, field: String },
    #[error("collision pass '{pass_id}' expected materialized value '{value_id}'")]
    MissingPassValue { pass_id: SmolStr, value_id: SmolStr },
    #[error("collision pass '{pass_id}' is not well-formed: {message}")]
    InvalidPass { pass_id: SmolStr, message: String },
    #[error("collision plan references unknown query contract '{contract_id}'")]
    UnknownQueryContract { contract_id: String },
    #[error("collision execution backend '{backend:?}' is not implemented")]
    UnsupportedBackend { backend: DispatchBackend },
    #[error("collision plan execution is not available: {message}")]
    ExecutionUnavailable { message: String },
}

impl CollisionPassKind {
    pub fn query_dependencies(&self) -> Vec<QueryContractId> {
        match self {
            Self::GatherCandidates {
                support_summary_contract,
                ..
            } => vec![*support_summary_contract],
            Self::EvaluatePointOccupancy {
                distance_contract,
                normal_contract,
                ..
            }
            | Self::ResolveSphereOverlap {
                distance_contract,
                normal_contract,
                ..
            }
            | Self::SweepSphereFirstContact {
                distance_contract,
                normal_contract,
                ..
            }
            | Self::ResolveSphereTimeOfImpact {
                distance_contract,
                normal_contract,
                ..
            } => vec![*distance_contract, *normal_contract],
            Self::CastRayFirstHit { trace_contract, .. } => vec![*trace_contract],
            Self::BuildBroadphaseCandidates { .. } | Self::MaterializeOutput { .. } => Vec::new(),
        }
    }
}

impl CollisionPlan {
    pub fn for_query(kind: CollisionQueryKind) -> Self {
        Self::for_query_with_backend(kind, DispatchBackend::Auto)
    }

    pub fn for_query_with_backend(kind: CollisionQueryKind, backend: DispatchBackend) -> Self {
        match kind {
            CollisionQueryKind::PointOccupancyWorld => static_point_plan(backend),
            CollisionQueryKind::RayCastWorld => static_ray_plan(backend),
            CollisionQueryKind::SphereOverlapWorld => static_overlap_plan(backend),
            CollisionQueryKind::SphereSweepTransition => transition_sweep_plan(backend),
            CollisionQueryKind::SphereTimeOfImpactTransition => {
                transition_time_of_impact_plan(backend)
            }
        }
    }

    pub fn semantic_artifact_contracts(&self) -> Vec<SemanticArtifactContract> {
        let mut out = self
            .artifacts
            .iter()
            .map(|artifact| artifact.contract.clone())
            .collect::<Vec<_>>();
        out.extend(acceleration::observer_acceleration_contracts(
            AccelerationObserverKind::Collision,
            self.name.as_str(),
        ));
        out
    }

    pub fn artifact_uses(&self) -> Vec<ArtifactUse> {
        let mut uses = self
            .artifacts
            .iter()
            .map(|artifact| ArtifactUse {
                actor: artifact.contract.producer.clone(),
                artifact_id: artifact.id.clone(),
                kind: ArtifactUseKind::Produce,
                source: ArtifactUseSource::Plan,
                required_validity: None,
            })
            .collect::<Vec<_>>();
        for pass in &self.passes {
            match &pass.kind {
                CollisionPassKind::SweepSphereFirstContact {
                    witness_artifact,
                    continuation_artifact,
                    ..
                }
                | CollisionPassKind::ResolveSphereTimeOfImpact {
                    witness_artifact,
                    continuation_artifact,
                    ..
                } => {
                    for artifact_id in [witness_artifact, continuation_artifact] {
                        if let Some(binding) = self
                            .artifacts
                            .iter()
                            .find(|candidate| candidate.id == *artifact_id)
                        {
                            uses.push(store_backed_use(
                                pass.id.clone(),
                                binding.id.clone(),
                                binding.contract.validity.clone(),
                            ));
                        }
                    }
                }
                CollisionPassKind::GatherCandidates { .. }
                | CollisionPassKind::BuildBroadphaseCandidates { .. }
                | CollisionPassKind::EvaluatePointOccupancy { .. }
                | CollisionPassKind::CastRayFirstHit { .. }
                | CollisionPassKind::ResolveSphereOverlap { .. }
                | CollisionPassKind::MaterializeOutput { .. } => {}
            }
        }
        uses.extend(
            acceleration::observer_acceleration_contracts(
                AccelerationObserverKind::Collision,
                self.name.as_str(),
            )
            .into_iter()
            .map(|contract| ArtifactUse {
                actor: contract.producer.clone(),
                artifact_id: contract.id,
                kind: ArtifactUseKind::Produce,
                source: ArtifactUseSource::Plan,
                required_validity: None,
            }),
        );
        uses
    }

    pub fn validate(&self) -> Vec<CollisionPlanValidationError> {
        let mut errors = Vec::new();
        let descriptor = descriptor(self.contract_id);
        if self.contract_version != descriptor.version {
            errors.push(validation_error(format!(
                "collision plan '{}' version {} does not match contract version {}",
                self.name, self.contract_version, descriptor.version
            )));
        }
        if self.family != descriptor.family || self.question != descriptor.question {
            errors.push(validation_error(format!(
                "collision plan '{}' does not match contract '{}'",
                self.name, descriptor.id
            )));
        }
        if self.target != descriptor.target {
            errors.push(validation_error(format!(
                "collision plan '{}' target {:?} does not match contract target {:?}",
                self.name, self.target, descriptor.target
            )));
        }
        if self.authority_scope != descriptor.authority.scope {
            errors.push(validation_error(format!(
                "collision plan '{}' authority scope '{}' does not match contract scope '{}'",
                self.name,
                collision_authority_scope_name(self.authority_scope),
                collision_authority_scope_name(descriptor.authority.scope)
            )));
        }
        if self.policy != descriptor.policy {
            errors.push(validation_error(format!(
                "collision plan '{}' policy does not match contract '{}'",
                self.name, descriptor.id
            )));
        }
        if !descriptor.supported_backends.supports(self.backend) {
            errors.push(validation_error(format!(
                "collision plan '{}' backend {:?} is not supported by '{}' (required_guarantee={} selected_method={})",
                self.name,
                self.backend,
                descriptor.id,
                descriptor.policy.required_guarantee.name(),
                descriptor.policy.selected_method.name()
            )));
        }
        if matches!(
            self.backend,
            DispatchBackend::VirtualGpu | DispatchBackend::Wgsl
        ) && (matches!(
            self.policy.required_guarantee,
            RequiredGuaranteeClass::Exact
        ) || matches!(
            self.policy.selected_method,
            SelectedMethodClass::ExactOracle
        )) {
            errors.push(validation_error(format!(
                "collision plan '{}' cannot target {:?} with required_guarantee={} selected_method={}",
                self.name,
                self.backend,
                self.policy.required_guarantee.name(),
                self.policy.selected_method.name()
            )));
        }

        validate_input_binding(
            &self.name,
            &self.inputs,
            CollisionInputKind::WorldCapture,
            "RegionCapture",
            &mut errors,
        );
        validate_input_binding(
            &self.name,
            &self.inputs,
            CollisionInputKind::SceneDomain,
            "SceneDomain",
            &mut errors,
        );
        if matches!(
            descriptor.authority.scope,
            CollisionAuthorityScope::Transition
        ) {
            validate_input_binding(
                &self.name,
                &self.inputs,
                CollisionInputKind::SnapshotTransition,
                "CollisionSnapshotTransitionInput",
                &mut errors,
            );
        }
        validate_input_binding(
            &self.name,
            &self.inputs,
            descriptor.input_kind,
            descriptor.input_record,
            &mut errors,
        );
        let expected_input_count = if matches!(
            descriptor.authority.scope,
            CollisionAuthorityScope::Transition
        ) {
            4
        } else {
            3
        };
        if self.inputs.len() != expected_input_count {
            errors.push(validation_error(format!(
                "collision plan '{}' must bind {} inputs",
                self.name, expected_input_count
            )));
        }
        if self.outputs.len() != 1 {
            errors.push(validation_error(format!(
                "collision plan '{}' must expose exactly one typed output",
                self.name
            )));
        }
        if let Some(output) = self.outputs.first() {
            if output.kind != descriptor.output_kind || output.record != descriptor.output_record {
                errors.push(validation_error(format!(
                    "collision output '{}' does not match contract '{}'",
                    output.name, descriptor.id
                )));
            }
            if output.witness_schema != Some(descriptor.witness_schema) {
                errors.push(validation_error(format!(
                    "collision output '{}' does not declare the contract witness schema '{}'",
                    output.name, descriptor.witness_schema.name
                )));
            }
        }
        let pass_ids = self
            .passes
            .iter()
            .map(|pass| pass.id.clone())
            .collect::<BTreeSet<_>>();
        let mut seen_artifact_kinds = BTreeSet::new();
        let declared_artifacts = self
            .artifacts
            .iter()
            .map(|artifact| (artifact.id.clone(), artifact))
            .collect::<BTreeMap<_, _>>();
        for artifact in &self.artifacts {
            if artifact.id != artifact.contract.id {
                errors.push(validation_error(format!(
                    "collision artifact '{}' does not match contract id '{}'",
                    artifact.id, artifact.contract.id
                )));
            }
            if !seen_artifact_kinds.insert(artifact.kind) {
                errors.push(validation_error(format!(
                    "collision plan '{}' declares '{}' more than once",
                    self.name,
                    collision_artifact_kind_name(artifact.kind)
                )));
            }
            if !pass_ids.contains(&artifact.contract.producer) {
                errors.push(validation_error(format!(
                    "collision artifact '{}' references unknown producer pass '{}'",
                    artifact.id, artifact.contract.producer
                )));
            }
            validate_artifact_binding(descriptor, artifact, &mut errors);
        }
        for expected in expected_artifact_kinds(descriptor.target) {
            if !self
                .artifacts
                .iter()
                .any(|artifact| artifact.kind == expected)
            {
                errors.push(validation_error(format!(
                    "collision plan '{}' is missing '{}' artifact",
                    self.name,
                    collision_artifact_kind_name(expected)
                )));
            }
        }

        let mut available_artifacts = BTreeSet::new();
        let mut available_values = BTreeSet::new();
        let mut gather_passes = 0usize;
        let mut broadphase_passes = 0usize;
        let mut evaluation_passes = 0usize;
        let mut materialization_passes = 0usize;
        for pass in &self.passes {
            for dependency in pass.kind.query_dependencies() {
                if query_contract::query_contract(dependency).is_none() {
                    errors.push(validation_error(format!(
                        "collision pass '{}' references unknown query contract '{}'",
                        pass.id,
                        dependency.as_str()
                    )));
                }
            }
            match &pass.kind {
                CollisionPassKind::GatherCandidates {
                    support_summary_contract,
                    support_artifact,
                } => {
                    gather_passes += 1;
                    if *support_summary_contract != query_contract::SUPPORT_SUMMARY_WORLD {
                        errors.push(validation_error(format!(
                            "collision pass '{}' must gather candidates through '{}'",
                            pass.id,
                            query_contract::SUPPORT_SUMMARY_WORLD.as_str()
                        )));
                    }
                    validate_exact_snapshot_artifact(
                        pass,
                        support_artifact,
                        CollisionArtifactKind::SupportSummary,
                        &declared_artifacts,
                        &mut errors,
                    );
                    if !pass.consumes.is_empty() || !pass.materializes.is_empty() {
                        errors.push(validation_error(format!(
                            "collision pass '{}' should not materialize intermediate values while gathering candidates",
                            pass.id
                        )));
                    }
                    available_artifacts.insert(support_artifact.clone());
                }
                CollisionPassKind::BuildBroadphaseCandidates {
                    support_artifact,
                    artifact_id,
                } => {
                    broadphase_passes += 1;
                    if !available_artifacts.contains(support_artifact) {
                        errors.push(validation_error(format!(
                            "collision pass '{}' consumes undeclared support artifact '{}'",
                            pass.id, support_artifact
                        )));
                    }
                    validate_exact_snapshot_artifact(
                        pass,
                        artifact_id,
                        CollisionArtifactKind::BroadphaseCandidates,
                        &declared_artifacts,
                        &mut errors,
                    );
                    if pass.consumes.len() != 1 || pass.consumes[0] != *support_artifact {
                        errors.push(validation_error(format!(
                            "collision pass '{}' must consume support artifact '{}'",
                            pass.id, support_artifact
                        )));
                    }
                    if !pass.materializes.is_empty() {
                        errors.push(validation_error(format!(
                            "collision pass '{}' must materialize artifacts, not intermediate values",
                            pass.id
                        )));
                    }
                    available_artifacts.insert(artifact_id.clone());
                }
                CollisionPassKind::EvaluatePointOccupancy {
                    distance_contract,
                    normal_contract,
                    support_artifact,
                    broadphase_artifact,
                } => {
                    evaluation_passes += 1;
                    validate_static_distance_pass(
                        descriptor,
                        pass,
                        *distance_contract,
                        *normal_contract,
                        support_artifact,
                        broadphase_artifact,
                        &available_artifacts,
                        &mut available_values,
                        &mut errors,
                    );
                }
                CollisionPassKind::CastRayFirstHit {
                    trace_contract,
                    support_artifact,
                    broadphase_artifact,
                } => {
                    evaluation_passes += 1;
                    if descriptor.question != CollisionQuestionId::RayCastFirstHit {
                        errors.push(validation_error(format!(
                            "collision pass '{}' does not match collision question '{:?}'",
                            pass.id, descriptor.question
                        )));
                    }
                    if *trace_contract != query_contract::SPATIAL_NEAREST_WORLD {
                        errors.push(validation_error(format!(
                            "collision pass '{}' must use world nearest-hit tracing",
                            pass.id
                        )));
                    }
                    validate_broadphase_consumer(
                        pass,
                        support_artifact,
                        broadphase_artifact,
                        &available_artifacts,
                        &mut available_values,
                        &mut errors,
                    );
                }
                CollisionPassKind::ResolveSphereOverlap {
                    distance_contract,
                    normal_contract,
                    support_artifact,
                    broadphase_artifact,
                    supported_shape,
                } => {
                    evaluation_passes += 1;
                    if descriptor.question != CollisionQuestionId::SphereOverlap {
                        errors.push(validation_error(format!(
                            "collision pass '{}' does not match collision question '{:?}'",
                            pass.id, descriptor.question
                        )));
                    }
                    if *distance_contract != query_contract::SPATIAL_DISTANCE_WORLD
                        || *normal_contract != query_contract::SPATIAL_NORMAL_WORLD
                    {
                        errors.push(validation_error(format!(
                            "collision pass '{}' must use world distance and normal contracts",
                            pass.id
                        )));
                    }
                    if supported_shape.as_str() != "sphere" {
                        errors.push(validation_error(format!(
                            "collision pass '{}' references unsupported shape '{}'",
                            pass.id, supported_shape
                        )));
                    }
                    validate_broadphase_consumer(
                        pass,
                        support_artifact,
                        broadphase_artifact,
                        &available_artifacts,
                        &mut available_values,
                        &mut errors,
                    );
                }
                CollisionPassKind::SweepSphereFirstContact {
                    distance_contract,
                    normal_contract,
                    support_artifact,
                    broadphase_artifact,
                    witness_artifact,
                    continuation_artifact,
                }
                | CollisionPassKind::ResolveSphereTimeOfImpact {
                    distance_contract,
                    normal_contract,
                    support_artifact,
                    broadphase_artifact,
                    witness_artifact,
                    continuation_artifact,
                } => {
                    evaluation_passes += 1;
                    if !matches!(descriptor.target, CollisionTargetKind::WorldTransition) {
                        errors.push(validation_error(format!(
                            "collision pass '{}' requires a transition-scoped contract",
                            pass.id
                        )));
                    }
                    if *distance_contract != query_contract::SPATIAL_DISTANCE_WORLD
                        || *normal_contract != query_contract::SPATIAL_NORMAL_WORLD
                    {
                        errors.push(validation_error(format!(
                            "collision pass '{}' must use world distance and normal contracts",
                            pass.id
                        )));
                    }
                    let available_artifacts_snapshot = available_artifacts.clone();
                    validate_transition_evaluation_pass(
                        descriptor,
                        pass,
                        support_artifact,
                        broadphase_artifact,
                        witness_artifact,
                        continuation_artifact,
                        &declared_artifacts,
                        &available_artifacts_snapshot,
                        &mut available_values,
                        &mut available_artifacts,
                        &mut errors,
                    );
                }
                CollisionPassKind::MaterializeOutput { output } => {
                    materialization_passes += 1;
                    let available_values_snapshot = available_values.clone();
                    validate_output_materialization(
                        self,
                        pass,
                        *output,
                        &available_values_snapshot,
                        &mut available_values,
                        &mut errors,
                    );
                }
            }
        }
        if gather_passes != 1 {
            errors.push(validation_error(format!(
                "collision plan '{}' must declare exactly one candidate gathering pass",
                self.name
            )));
        }
        let expected_broadphase = 1;
        if broadphase_passes != expected_broadphase {
            errors.push(validation_error(format!(
                "collision plan '{}' must declare {} broadphase candidate pass(es)",
                self.name, expected_broadphase
            )));
        }
        if evaluation_passes != 1 {
            errors.push(validation_error(format!(
                "collision plan '{}' must declare exactly one collision evaluation pass",
                self.name
            )));
        }
        if materialization_passes != 1 {
            errors.push(validation_error(format!(
                "collision plan '{}' must declare exactly one output materialization pass",
                self.name
            )));
        }
        for artifact in &self.artifacts {
            if !available_artifacts.contains(&artifact.id) {
                errors.push(validation_error(format!(
                    "collision artifact '{}' is declared but never produced by its plan",
                    artifact.id
                )));
            }
        }
        for output in &self.outputs {
            if !available_values.contains(&output.name) {
                errors.push(validation_error(format!(
                    "collision output '{}' is never materialized by the plan",
                    output.name
                )));
            }
        }
        for use_record in self.artifact_uses() {
            if use_record.source == ArtifactUseSource::ArtifactStore
                && !self
                    .artifacts
                    .iter()
                    .any(|artifact| artifact.id == use_record.artifact_id)
            {
                errors.push(validation_error(format!(
                    "artifact use '{}' loads undeclared artifact '{}'",
                    use_record.actor, use_record.artifact_id
                )));
            }
        }
        errors.extend(
            self.validate_acceleration_contracts()
                .into_iter()
                .map(|error| validation_error(error.to_string())),
        );
        errors
    }

    pub fn validate_acceleration_contracts(&self) -> Vec<SmolStr> {
        acceleration::validate_observer_acceleration_contracts(
            AccelerationObserverKind::Collision,
            self.name.as_str(),
            &self.semantic_artifact_contracts(),
        )
    }

    pub fn execute(
        &self,
        ctx: &QueryExecContext,
        args: &[KernelValue],
    ) -> Result<(CollisionResult, CollisionExecutionTrace), CollisionExecError> {
        crate::collision_exec::cpu::execute(self, ctx, args)
    }
}

pub fn collision_plans_with_backend(backend: DispatchBackend) -> Vec<CollisionPlan> {
    [
        CollisionQueryKind::PointOccupancyWorld,
        CollisionQueryKind::RayCastWorld,
        CollisionQueryKind::SphereOverlapWorld,
        CollisionQueryKind::SphereSweepTransition,
        CollisionQueryKind::SphereTimeOfImpactTransition,
    ]
    .into_iter()
    .map(|kind| CollisionPlan::for_query_with_backend(kind, backend))
    .collect()
}

pub fn collision_history_compatibility_hash(
    contract_id: CollisionContractId,
    artifact_kind: CollisionArtifactKind,
    flavor: Option<CollisionContactNormalFlavor>,
) -> u64 {
    let flavor = flavor
        .map(collision_contact_normal_flavor_name)
        .unwrap_or("none");
    stable_history_compatibility_hash(&[
        contract_id.as_str().as_bytes(),
        collision_artifact_kind_name(artifact_kind).as_bytes(),
        flavor.as_bytes(),
    ])
}

fn static_point_plan(backend: DispatchBackend) -> CollisionPlan {
    let descriptor = descriptor(COLLISION_POINT_OCCUPANCY_WORLD);
    let support_id = SmolStr::new("artifact.support_summary.point_occupancy");
    let broadphase_id = SmolStr::new("artifact.broadphase_candidates.point_occupancy");
    let gather = SmolStr::new("candidate_gather");
    let broadphase = SmolStr::new("broadphase_candidates");
    CollisionPlan {
        name: SmolStr::new("collision.point_occupancy.world"),
        contract_id: descriptor.id,
        contract_version: descriptor.version,
        family: descriptor.family,
        question: descriptor.question,
        target: descriptor.target,
        authority_scope: descriptor.authority.scope,
        backend,
        policy: descriptor.policy,
        inputs: vec![
            input_binding("world", CollisionInputKind::WorldCapture, "RegionCapture"),
            input_binding("domain", CollisionInputKind::SceneDomain, "SceneDomain"),
            input_binding("point", CollisionInputKind::Point, descriptor.input_record),
        ],
        passes: vec![
            CollisionPass {
                id: gather.clone(),
                kind: CollisionPassKind::GatherCandidates {
                    support_summary_contract: query_contract::SUPPORT_SUMMARY_WORLD,
                    support_artifact: support_id.clone(),
                },
                consumes: Vec::new(),
                materializes: Vec::new(),
            },
            CollisionPass {
                id: broadphase.clone(),
                kind: CollisionPassKind::BuildBroadphaseCandidates {
                    support_artifact: support_id.clone(),
                    artifact_id: broadphase_id.clone(),
                },
                consumes: vec![support_id.clone()],
                materializes: Vec::new(),
            },
            CollisionPass {
                id: SmolStr::new("point_occupancy"),
                kind: CollisionPassKind::EvaluatePointOccupancy {
                    distance_contract: query_contract::SPATIAL_DISTANCE_WORLD,
                    normal_contract: query_contract::SPATIAL_NORMAL_WORLD,
                    support_artifact: support_id.clone(),
                    broadphase_artifact: broadphase_id.clone(),
                },
                consumes: vec![support_id.clone(), broadphase_id.clone()],
                materializes: vec![SmolStr::new("occupancy")],
            },
            CollisionPass {
                id: SmolStr::new("materialize_output"),
                kind: CollisionPassKind::MaterializeOutput {
                    output: CollisionOutputKind::Occupancy,
                },
                consumes: vec![SmolStr::new("occupancy")],
                materializes: vec![SmolStr::new("occupancy")],
            },
        ],
        artifacts: vec![
            support_summary_artifact(descriptor, gather, support_id),
            broadphase_candidates_artifact(descriptor, broadphase, broadphase_id),
        ],
        outputs: vec![output_binding(
            "occupancy",
            descriptor.output_kind,
            descriptor.output_record,
            Some(descriptor.witness_schema),
        )],
    }
}

fn static_ray_plan(backend: DispatchBackend) -> CollisionPlan {
    let descriptor = descriptor(COLLISION_RAY_CAST_WORLD);
    let support_id = SmolStr::new("artifact.support_summary.ray_cast");
    let broadphase_id = SmolStr::new("artifact.broadphase_candidates.ray_cast");
    let gather = SmolStr::new("candidate_gather");
    let broadphase = SmolStr::new("broadphase_candidates");
    CollisionPlan {
        name: SmolStr::new("collision.ray_cast.world"),
        contract_id: descriptor.id,
        contract_version: descriptor.version,
        family: descriptor.family,
        question: descriptor.question,
        target: descriptor.target,
        authority_scope: descriptor.authority.scope,
        backend,
        policy: descriptor.policy,
        inputs: vec![
            input_binding("world", CollisionInputKind::WorldCapture, "RegionCapture"),
            input_binding("domain", CollisionInputKind::SceneDomain, "SceneDomain"),
            input_binding("ray", CollisionInputKind::Ray, descriptor.input_record),
        ],
        passes: vec![
            CollisionPass {
                id: gather.clone(),
                kind: CollisionPassKind::GatherCandidates {
                    support_summary_contract: query_contract::SUPPORT_SUMMARY_WORLD,
                    support_artifact: support_id.clone(),
                },
                consumes: Vec::new(),
                materializes: Vec::new(),
            },
            CollisionPass {
                id: broadphase.clone(),
                kind: CollisionPassKind::BuildBroadphaseCandidates {
                    support_artifact: support_id.clone(),
                    artifact_id: broadphase_id.clone(),
                },
                consumes: vec![support_id.clone()],
                materializes: Vec::new(),
            },
            CollisionPass {
                id: SmolStr::new("ray_cast"),
                kind: CollisionPassKind::CastRayFirstHit {
                    trace_contract: query_contract::SPATIAL_NEAREST_WORLD,
                    support_artifact: support_id.clone(),
                    broadphase_artifact: broadphase_id.clone(),
                },
                consumes: vec![support_id.clone(), broadphase_id.clone()],
                materializes: vec![SmolStr::new("ray_cast")],
            },
            CollisionPass {
                id: SmolStr::new("materialize_output"),
                kind: CollisionPassKind::MaterializeOutput {
                    output: CollisionOutputKind::RayCast,
                },
                consumes: vec![SmolStr::new("ray_cast")],
                materializes: vec![SmolStr::new("ray_cast")],
            },
        ],
        artifacts: vec![
            support_summary_artifact(descriptor, gather, support_id),
            broadphase_candidates_artifact(descriptor, broadphase, broadphase_id),
        ],
        outputs: vec![output_binding(
            "ray_cast",
            descriptor.output_kind,
            descriptor.output_record,
            Some(descriptor.witness_schema),
        )],
    }
}

fn static_overlap_plan(backend: DispatchBackend) -> CollisionPlan {
    let descriptor = descriptor(COLLISION_SPHERE_OVERLAP_WORLD);
    let support_id = SmolStr::new("artifact.support_summary.sphere_overlap");
    let broadphase_id = SmolStr::new("artifact.broadphase_candidates.sphere_overlap");
    let gather = SmolStr::new("candidate_gather");
    let broadphase = SmolStr::new("broadphase_candidates");
    CollisionPlan {
        name: SmolStr::new("collision.sphere_overlap.world"),
        contract_id: descriptor.id,
        contract_version: descriptor.version,
        family: descriptor.family,
        question: descriptor.question,
        target: descriptor.target,
        authority_scope: descriptor.authority.scope,
        backend,
        policy: descriptor.policy,
        inputs: vec![
            input_binding("world", CollisionInputKind::WorldCapture, "RegionCapture"),
            input_binding("domain", CollisionInputKind::SceneDomain, "SceneDomain"),
            input_binding(
                "probe",
                CollisionInputKind::SphereProbe,
                descriptor.input_record,
            ),
        ],
        passes: vec![
            CollisionPass {
                id: gather.clone(),
                kind: CollisionPassKind::GatherCandidates {
                    support_summary_contract: query_contract::SUPPORT_SUMMARY_WORLD,
                    support_artifact: support_id.clone(),
                },
                consumes: Vec::new(),
                materializes: Vec::new(),
            },
            CollisionPass {
                id: broadphase.clone(),
                kind: CollisionPassKind::BuildBroadphaseCandidates {
                    support_artifact: support_id.clone(),
                    artifact_id: broadphase_id.clone(),
                },
                consumes: vec![support_id.clone()],
                materializes: Vec::new(),
            },
            CollisionPass {
                id: SmolStr::new("sphere_overlap"),
                kind: CollisionPassKind::ResolveSphereOverlap {
                    distance_contract: query_contract::SPATIAL_DISTANCE_WORLD,
                    normal_contract: query_contract::SPATIAL_NORMAL_WORLD,
                    support_artifact: support_id.clone(),
                    broadphase_artifact: broadphase_id.clone(),
                    supported_shape: SmolStr::new("sphere"),
                },
                consumes: vec![support_id.clone(), broadphase_id.clone()],
                materializes: vec![SmolStr::new("sphere_overlap")],
            },
            CollisionPass {
                id: SmolStr::new("materialize_output"),
                kind: CollisionPassKind::MaterializeOutput {
                    output: CollisionOutputKind::SphereOverlap,
                },
                consumes: vec![SmolStr::new("sphere_overlap")],
                materializes: vec![SmolStr::new("sphere_overlap")],
            },
        ],
        artifacts: vec![
            support_summary_artifact(descriptor, gather, support_id),
            broadphase_candidates_artifact(descriptor, broadphase, broadphase_id),
        ],
        outputs: vec![output_binding(
            "sphere_overlap",
            descriptor.output_kind,
            descriptor.output_record,
            Some(descriptor.witness_schema),
        )],
    }
}

fn transition_sweep_plan(backend: DispatchBackend) -> CollisionPlan {
    let descriptor = descriptor(COLLISION_SPHERE_SWEEP_TRANSITION);
    let support_id = SmolStr::new("artifact.support_summary.sphere_sweep");
    let broadphase_id = SmolStr::new("artifact.broadphase_candidates.sphere_sweep");
    let witness_id = SmolStr::new("artifact.witness_cache.sphere_sweep");
    let continuation_id = SmolStr::new("artifact.continuation_seed.sphere_sweep");
    let gather = SmolStr::new("candidate_gather");
    let broadphase = SmolStr::new("broadphase_candidates");
    let evaluate = SmolStr::new("sphere_sweep");
    CollisionPlan {
        name: SmolStr::new("collision.sphere_sweep.transition"),
        contract_id: descriptor.id,
        contract_version: descriptor.version,
        family: descriptor.family,
        question: descriptor.question,
        target: descriptor.target,
        authority_scope: descriptor.authority.scope,
        backend,
        policy: descriptor.policy,
        inputs: vec![
            input_binding("world", CollisionInputKind::WorldCapture, "RegionCapture"),
            input_binding("domain", CollisionInputKind::SceneDomain, "SceneDomain"),
            input_binding(
                "transition",
                CollisionInputKind::SnapshotTransition,
                "CollisionSnapshotTransitionInput",
            ),
            input_binding(
                "sweep",
                CollisionInputKind::SphereSweep,
                descriptor.input_record,
            ),
        ],
        passes: vec![
            CollisionPass {
                id: gather.clone(),
                kind: CollisionPassKind::GatherCandidates {
                    support_summary_contract: query_contract::SUPPORT_SUMMARY_WORLD,
                    support_artifact: support_id.clone(),
                },
                consumes: Vec::new(),
                materializes: Vec::new(),
            },
            CollisionPass {
                id: broadphase.clone(),
                kind: CollisionPassKind::BuildBroadphaseCandidates {
                    support_artifact: support_id.clone(),
                    artifact_id: broadphase_id.clone(),
                },
                consumes: vec![support_id.clone()],
                materializes: Vec::new(),
            },
            CollisionPass {
                id: evaluate.clone(),
                kind: CollisionPassKind::SweepSphereFirstContact {
                    distance_contract: query_contract::SPATIAL_DISTANCE_WORLD,
                    normal_contract: query_contract::SPATIAL_NORMAL_WORLD,
                    support_artifact: support_id.clone(),
                    broadphase_artifact: broadphase_id.clone(),
                    witness_artifact: witness_id.clone(),
                    continuation_artifact: continuation_id.clone(),
                },
                consumes: vec![support_id.clone(), broadphase_id.clone()],
                materializes: vec![SmolStr::new("sweep_contact")],
            },
            CollisionPass {
                id: SmolStr::new("materialize_output"),
                kind: CollisionPassKind::MaterializeOutput {
                    output: CollisionOutputKind::SweepContact,
                },
                consumes: vec![SmolStr::new("sweep_contact")],
                materializes: vec![SmolStr::new("sweep_contact")],
            },
        ],
        artifacts: vec![
            support_summary_artifact(descriptor, gather, support_id),
            broadphase_candidates_artifact(descriptor, broadphase, broadphase_id),
            witness_cache_artifact(descriptor, evaluate.clone(), witness_id),
            continuation_seed_artifact(descriptor, evaluate, continuation_id),
        ],
        outputs: vec![output_binding(
            "sweep_contact",
            descriptor.output_kind,
            descriptor.output_record,
            Some(descriptor.witness_schema),
        )],
    }
}

fn transition_time_of_impact_plan(backend: DispatchBackend) -> CollisionPlan {
    let descriptor = descriptor(COLLISION_TIME_OF_IMPACT_TRANSITION);
    let support_id = SmolStr::new("artifact.support_summary.time_of_impact");
    let broadphase_id = SmolStr::new("artifact.broadphase_candidates.time_of_impact");
    let witness_id = SmolStr::new("artifact.witness_cache.time_of_impact");
    let continuation_id = SmolStr::new("artifact.continuation_seed.time_of_impact");
    let gather = SmolStr::new("candidate_gather");
    let broadphase = SmolStr::new("broadphase_candidates");
    let evaluate = SmolStr::new("time_of_impact");
    CollisionPlan {
        name: SmolStr::new("collision.time_of_impact.transition"),
        contract_id: descriptor.id,
        contract_version: descriptor.version,
        family: descriptor.family,
        question: descriptor.question,
        target: descriptor.target,
        authority_scope: descriptor.authority.scope,
        backend,
        policy: descriptor.policy,
        inputs: vec![
            input_binding("world", CollisionInputKind::WorldCapture, "RegionCapture"),
            input_binding("domain", CollisionInputKind::SceneDomain, "SceneDomain"),
            input_binding(
                "transition",
                CollisionInputKind::SnapshotTransition,
                "CollisionSnapshotTransitionInput",
            ),
            input_binding(
                "sweep",
                CollisionInputKind::SphereSweep,
                descriptor.input_record,
            ),
        ],
        passes: vec![
            CollisionPass {
                id: gather.clone(),
                kind: CollisionPassKind::GatherCandidates {
                    support_summary_contract: query_contract::SUPPORT_SUMMARY_WORLD,
                    support_artifact: support_id.clone(),
                },
                consumes: Vec::new(),
                materializes: Vec::new(),
            },
            CollisionPass {
                id: broadphase.clone(),
                kind: CollisionPassKind::BuildBroadphaseCandidates {
                    support_artifact: support_id.clone(),
                    artifact_id: broadphase_id.clone(),
                },
                consumes: vec![support_id.clone()],
                materializes: Vec::new(),
            },
            CollisionPass {
                id: evaluate.clone(),
                kind: CollisionPassKind::ResolveSphereTimeOfImpact {
                    distance_contract: query_contract::SPATIAL_DISTANCE_WORLD,
                    normal_contract: query_contract::SPATIAL_NORMAL_WORLD,
                    support_artifact: support_id.clone(),
                    broadphase_artifact: broadphase_id.clone(),
                    witness_artifact: witness_id.clone(),
                    continuation_artifact: continuation_id.clone(),
                },
                consumes: vec![support_id.clone(), broadphase_id.clone()],
                materializes: vec![SmolStr::new("time_of_impact")],
            },
            CollisionPass {
                id: SmolStr::new("materialize_output"),
                kind: CollisionPassKind::MaterializeOutput {
                    output: CollisionOutputKind::TimeOfImpact,
                },
                consumes: vec![SmolStr::new("time_of_impact")],
                materializes: vec![SmolStr::new("time_of_impact")],
            },
        ],
        artifacts: vec![
            support_summary_artifact(descriptor, gather, support_id),
            broadphase_candidates_artifact(descriptor, broadphase, broadphase_id),
            witness_cache_artifact(descriptor, evaluate.clone(), witness_id),
            continuation_seed_artifact(descriptor, evaluate, continuation_id),
        ],
        outputs: vec![output_binding(
            "time_of_impact",
            descriptor.output_kind,
            descriptor.output_record,
            Some(descriptor.witness_schema),
        )],
    }
}

fn descriptor(id: CollisionContractId) -> &'static CollisionContractDescriptor {
    collision_contract(id).expect("collision contract must exist")
}

fn input_binding(name: &str, kind: CollisionInputKind, record: &str) -> CollisionInputBinding {
    CollisionInputBinding {
        name: SmolStr::new(name),
        kind,
        record: SmolStr::new(record),
    }
}

fn output_binding(
    name: &str,
    kind: CollisionOutputKind,
    record: &str,
    witness_schema: Option<&'static CollisionWitnessSchema>,
) -> CollisionOutputBinding {
    CollisionOutputBinding {
        name: SmolStr::new(name),
        kind,
        record: SmolStr::new(record),
        witness_schema,
    }
}

fn support_summary_artifact(
    descriptor: &CollisionContractDescriptor,
    producer_pass: SmolStr,
    artifact_id: SmolStr,
) -> CollisionArtifactBinding {
    let contract = exact_snapshot_artifact_contract(
        artifact_id.clone(),
        "CollisionSupportSummaryArtifact",
        collision_artifact_kind_name(CollisionArtifactKind::SupportSummary),
        descriptor,
        producer_pass,
    );
    CollisionArtifactBinding {
        id: artifact_id,
        kind: CollisionArtifactKind::SupportSummary,
        record: SmolStr::new("CollisionSupportSummaryArtifact"),
        contract,
    }
}

fn broadphase_candidates_artifact(
    descriptor: &CollisionContractDescriptor,
    producer_pass: SmolStr,
    artifact_id: SmolStr,
) -> CollisionArtifactBinding {
    let contract = exact_snapshot_artifact_contract(
        artifact_id.clone(),
        "CollisionBroadphaseCandidatesArtifact",
        collision_artifact_kind_name(CollisionArtifactKind::BroadphaseCandidates),
        descriptor,
        producer_pass,
    );
    CollisionArtifactBinding {
        id: artifact_id,
        kind: CollisionArtifactKind::BroadphaseCandidates,
        record: SmolStr::new("CollisionBroadphaseCandidatesArtifact"),
        contract,
    }
}

fn witness_cache_artifact(
    descriptor: &CollisionContractDescriptor,
    producer_pass: SmolStr,
    artifact_id: SmolStr,
) -> CollisionArtifactBinding {
    let compatibility = descriptor
        .authority
        .transition_compatibility
        .expect("transition collision contract must declare compatibility");
    let evidence_summary = SemanticEvidenceSummary::artifact_bound(false)
        .with_artifact_binding("collision.witness_cache");
    let contract = transition_history_artifact_contract(
        artifact_id.clone(),
        "CollisionWitnessCacheArtifact",
        collision_artifact_kind_name(CollisionArtifactKind::WitnessCache),
        descriptor,
        producer_pass,
        compatibility,
        evidence_summary,
    );
    CollisionArtifactBinding {
        id: artifact_id,
        kind: CollisionArtifactKind::WitnessCache,
        record: SmolStr::new("CollisionWitnessCacheArtifact"),
        contract,
    }
}

fn continuation_seed_artifact(
    descriptor: &CollisionContractDescriptor,
    producer_pass: SmolStr,
    artifact_id: SmolStr,
) -> CollisionArtifactBinding {
    let compatibility = descriptor
        .authority
        .transition_compatibility
        .expect("transition collision contract must declare compatibility");
    let evidence_summary = SemanticEvidenceSummary::artifact_bound(false)
        .with_artifact_binding("collision.continuation_seed");
    let contract = transition_history_artifact_contract(
        artifact_id.clone(),
        "CollisionContinuationSeedArtifact",
        collision_artifact_kind_name(CollisionArtifactKind::ContinuationSeed),
        descriptor,
        producer_pass,
        compatibility,
        evidence_summary,
    );
    CollisionArtifactBinding {
        id: artifact_id,
        kind: CollisionArtifactKind::ContinuationSeed,
        record: SmolStr::new("CollisionContinuationSeedArtifact"),
        contract,
    }
}

fn exact_snapshot_artifact_contract(
    artifact_id: SmolStr,
    record: &str,
    artifact_kind: &str,
    descriptor: &CollisionContractDescriptor,
    producer_pass: SmolStr,
) -> SemanticArtifactContract {
    let evidence_summary = SemanticEvidenceSummary::contract_bound();
    SemanticArtifactContract {
        id: artifact_id,
        kind: SemanticArtifactKind::Query,
        logical_schema: ArtifactLogicalSchema {
            namespace: SmolStr::new("collision"),
            name: SmolStr::new("artifact"),
            fields: vec![
                ArtifactLogicalField::new("collision_contract", descriptor.id.as_str()),
                ArtifactLogicalField::new("artifact_kind", artifact_kind),
                ArtifactLogicalField::new("record", record),
                ArtifactLogicalField::new("target", collision_target_name(descriptor.target)),
            ],
        },
        compatibility: ArtifactCompatibilityRelation {
            snapshot: ArtifactSnapshotRelation::ExactSnapshot,
            transition: ArtifactTransitionRelation {
                compatibility: None,
                requires_previous_snapshot: false,
            },
            policy: ArtifactPolicyCompatibility {
                mode: ArtifactPolicyDigestMode::Exact,
            },
            evidence: ArtifactEvidenceCompatibility {
                origin: evidence_summary.origin,
                scope: evidence_summary.scope,
            },
        },
        acceleration: None,
        validity: ArtifactValidityRule::all(vec![
            ArtifactValidityRule::predicate(
                ArtifactValidityPredicate::CurrentSnapshotMatchesStored,
            ),
            ArtifactValidityRule::predicate(ArtifactValidityPredicate::PolicyDigestMatches),
            ArtifactValidityRule::predicate(ArtifactValidityPredicate::EvidenceSummaryMatches),
        ]),
        producer: producer_pass,
        consumer: SmolStr::new("collision.resolve"),
        deterministic: true,
        version: COLLISION_PLAN_SCHEMA_VERSION,
        transition: None,
        evidence_summary,
    }
}

fn transition_history_artifact_contract(
    artifact_id: SmolStr,
    record: &str,
    artifact_kind: &str,
    descriptor: &CollisionContractDescriptor,
    producer_pass: SmolStr,
    compatibility: crate::state_advance::ChangeCompatibility,
    evidence_summary: SemanticEvidenceSummary,
) -> SemanticArtifactContract {
    SemanticArtifactContract {
        id: artifact_id,
        kind: SemanticArtifactKind::Query,
        logical_schema: ArtifactLogicalSchema {
            namespace: SmolStr::new("collision"),
            name: SmolStr::new("artifact"),
            fields: vec![
                ArtifactLogicalField::new("collision_contract", descriptor.id.as_str()),
                ArtifactLogicalField::new("artifact_kind", artifact_kind),
                ArtifactLogicalField::new("record", record),
                ArtifactLogicalField::new("target", collision_target_name(descriptor.target)),
            ],
        },
        compatibility: ArtifactCompatibilityRelation {
            snapshot: ArtifactSnapshotRelation::PreviousSnapshotEpoch,
            transition: ArtifactTransitionRelation {
                compatibility: Some(compatibility),
                requires_previous_snapshot: true,
            },
            policy: ArtifactPolicyCompatibility {
                mode: ArtifactPolicyDigestMode::CompatibleRange,
            },
            evidence: ArtifactEvidenceCompatibility {
                origin: evidence_summary.origin,
                scope: evidence_summary.scope,
            },
        },
        acceleration: None,
        validity: ArtifactValidityRule::all(vec![
            ArtifactValidityRule::predicate(
                ArtifactValidityPredicate::PreviousSnapshotMatchesStored,
            ),
            ArtifactValidityRule::predicate(
                ArtifactValidityPredicate::SnapshotLineageMatchesCurrent,
            ),
            ArtifactValidityRule::predicate(ArtifactValidityPredicate::CompatibleChange(
                compatibility,
            )),
            ArtifactValidityRule::predicate(ArtifactValidityPredicate::PolicyDigestMatches),
            ArtifactValidityRule::predicate(ArtifactValidityPredicate::HistoryCompatibilityMatches),
            ArtifactValidityRule::predicate(ArtifactValidityPredicate::EvidenceScopeMatches(
                EvidenceScope::ArtifactBound,
            )),
            ArtifactValidityRule::predicate(ArtifactValidityPredicate::EvidenceSummaryMatches),
            ArtifactValidityRule::predicate(ArtifactValidityPredicate::MaxSnapshotAge(1)),
        ]),
        producer: producer_pass,
        consumer: SmolStr::new("collision.resolve"),
        deterministic: true,
        version: COLLISION_PLAN_SCHEMA_VERSION,
        transition: None,
        evidence_summary,
    }
}

fn validation_error(message: impl Into<String>) -> CollisionPlanValidationError {
    CollisionPlanValidationError {
        message: message.into(),
    }
}

fn validate_input_binding(
    plan_name: &SmolStr,
    inputs: &[CollisionInputBinding],
    kind: CollisionInputKind,
    expected_record: &str,
    errors: &mut Vec<CollisionPlanValidationError>,
) {
    match inputs.iter().find(|binding| binding.kind == kind) {
        Some(binding) if binding.record.as_str() == expected_record => {}
        Some(binding) => errors.push(validation_error(format!(
            "collision plan '{}' binds '{}' with record '{}' instead of '{}'",
            plan_name,
            collision_input_kind_name(kind),
            binding.record,
            expected_record
        ))),
        None => errors.push(validation_error(format!(
            "collision plan '{}' is missing '{}' input binding",
            plan_name,
            collision_input_kind_name(kind)
        ))),
    }
}

fn validate_artifact_binding(
    descriptor: &CollisionContractDescriptor,
    artifact: &CollisionArtifactBinding,
    errors: &mut Vec<CollisionPlanValidationError>,
) {
    match artifact.kind {
        CollisionArtifactKind::SupportSummary | CollisionArtifactKind::BroadphaseCandidates => {
            if artifact.contract.compatibility.snapshot != ArtifactSnapshotRelation::ExactSnapshot {
                errors.push(validation_error(format!(
                    "collision artifact '{}' must be exact-snapshot scoped",
                    artifact.id
                )));
            }
            if artifact
                .contract
                .compatibility
                .transition
                .requires_previous_snapshot
            {
                errors.push(validation_error(format!(
                    "collision artifact '{}' must not require a previous snapshot",
                    artifact.id
                )));
            }
        }
        CollisionArtifactKind::WitnessCache | CollisionArtifactKind::ContinuationSeed => {
            if !matches!(descriptor.target, CollisionTargetKind::WorldTransition) {
                errors.push(validation_error(format!(
                    "collision artifact '{}' is transition-scoped on a snapshot-only plan",
                    artifact.id
                )));
            }
            if artifact.contract.compatibility.snapshot
                != ArtifactSnapshotRelation::PreviousSnapshotEpoch
            {
                errors.push(validation_error(format!(
                    "collision artifact '{}' must be previous-snapshot scoped",
                    artifact.id
                )));
            }
            if !artifact
                .contract
                .compatibility
                .transition
                .requires_previous_snapshot
            {
                errors.push(validation_error(format!(
                    "collision artifact '{}' must require a previous snapshot",
                    artifact.id
                )));
            }
            if artifact.contract.compatibility.transition.compatibility
                != descriptor.authority.transition_compatibility
            {
                errors.push(validation_error(format!(
                    "collision artifact '{}' does not match transition compatibility for '{}'",
                    artifact.id, descriptor.id
                )));
            }
            if artifact.contract.compatibility.policy.mode
                != ArtifactPolicyDigestMode::CompatibleRange
            {
                errors.push(validation_error(format!(
                    "collision artifact '{}' must use compatible-range policy reuse",
                    artifact.id
                )));
            }
            if !validity_contains_predicate(
                &artifact.contract.validity,
                &ArtifactValidityPredicate::PreviousSnapshotMatchesStored,
            ) || !validity_contains_predicate(
                &artifact.contract.validity,
                &ArtifactValidityPredicate::CompatibleChange(
                    descriptor
                        .authority
                        .transition_compatibility
                        .expect("transition collision contract"),
                ),
            ) || !validity_contains_predicate(
                &artifact.contract.validity,
                &ArtifactValidityPredicate::HistoryCompatibilityMatches,
            ) || !validity_contains_predicate(
                &artifact.contract.validity,
                &ArtifactValidityPredicate::EvidenceSummaryMatches,
            ) {
                errors.push(validation_error(format!(
                    "collision artifact '{}' is missing explicit witness reuse validity predicates",
                    artifact.id
                )));
            }
        }
    }
}

fn validate_exact_snapshot_artifact(
    pass: &CollisionPass,
    artifact_id: &SmolStr,
    kind: CollisionArtifactKind,
    declared_artifacts: &BTreeMap<SmolStr, &CollisionArtifactBinding>,
    errors: &mut Vec<CollisionPlanValidationError>,
) {
    let Some(artifact) = declared_artifacts.get(artifact_id) else {
        errors.push(validation_error(format!(
            "collision pass '{}' references undeclared artifact '{}'",
            pass.id, artifact_id
        )));
        return;
    };
    if artifact.kind != kind {
        errors.push(validation_error(format!(
            "collision pass '{}' expected '{}' artifact '{}'",
            pass.id,
            collision_artifact_kind_name(kind),
            artifact_id
        )));
    }
}

fn validate_static_distance_pass(
    descriptor: &CollisionContractDescriptor,
    pass: &CollisionPass,
    distance_contract: QueryContractId,
    normal_contract: QueryContractId,
    support_artifact: &SmolStr,
    broadphase_artifact: &SmolStr,
    available_artifacts: &BTreeSet<SmolStr>,
    available_values: &mut BTreeSet<SmolStr>,
    errors: &mut Vec<CollisionPlanValidationError>,
) {
    if !(matches!(descriptor.question, CollisionQuestionId::PointOccupancy)
        || matches!(descriptor.question, CollisionQuestionId::SphereOverlap))
    {
        errors.push(validation_error(format!(
            "collision pass '{}' does not match collision question '{:?}'",
            pass.id, descriptor.question
        )));
    }
    if distance_contract != query_contract::SPATIAL_DISTANCE_WORLD
        || normal_contract != query_contract::SPATIAL_NORMAL_WORLD
    {
        errors.push(validation_error(format!(
            "collision pass '{}' must use world distance and normal contracts",
            pass.id
        )));
    }
    validate_broadphase_consumer(
        pass,
        support_artifact,
        broadphase_artifact,
        available_artifacts,
        available_values,
        errors,
    );
}

fn validate_broadphase_consumer(
    pass: &CollisionPass,
    support_artifact: &SmolStr,
    broadphase_artifact: &SmolStr,
    available_artifacts: &BTreeSet<SmolStr>,
    available_values: &mut BTreeSet<SmolStr>,
    errors: &mut Vec<CollisionPlanValidationError>,
) {
    for artifact_id in [support_artifact, broadphase_artifact] {
        if !available_artifacts.contains(artifact_id) {
            errors.push(validation_error(format!(
                "collision pass '{}' consumes undeclared support artifact '{}'",
                pass.id, artifact_id
            )));
        }
    }
    if pass.consumes.len() != 2
        || pass.consumes[0] != *support_artifact
        || pass.consumes[1] != *broadphase_artifact
    {
        errors.push(validation_error(format!(
            "collision pass '{}' must consume support and broadphase artifacts in order",
            pass.id
        )));
    }
    if pass.materializes.len() != 1 {
        errors.push(validation_error(format!(
            "collision pass '{}' must materialize exactly one intermediate value",
            pass.id
        )));
    } else {
        available_values.insert(pass.materializes[0].clone());
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_transition_evaluation_pass(
    descriptor: &CollisionContractDescriptor,
    pass: &CollisionPass,
    support_artifact: &SmolStr,
    broadphase_artifact: &SmolStr,
    witness_artifact: &SmolStr,
    continuation_artifact: &SmolStr,
    declared_artifacts: &BTreeMap<SmolStr, &CollisionArtifactBinding>,
    available_artifacts: &BTreeSet<SmolStr>,
    available_values: &mut BTreeSet<SmolStr>,
    produced_artifacts: &mut BTreeSet<SmolStr>,
    errors: &mut Vec<CollisionPlanValidationError>,
) {
    match (&pass.kind, descriptor.question) {
        (
            CollisionPassKind::SweepSphereFirstContact { .. },
            CollisionQuestionId::SphereSweepFirstContact,
        )
        | (
            CollisionPassKind::ResolveSphereTimeOfImpact { .. },
            CollisionQuestionId::SphereTimeOfImpact,
        ) => {}
        _ => errors.push(validation_error(format!(
            "collision pass '{}' does not match collision question '{:?}'",
            pass.id, descriptor.question
        ))),
    }
    for artifact_id in [support_artifact, broadphase_artifact] {
        if !available_artifacts.contains(artifact_id) {
            errors.push(validation_error(format!(
                "collision pass '{}' consumes artifact '{}' before it is produced",
                pass.id, artifact_id
            )));
        }
    }
    if pass.consumes != vec![support_artifact.clone(), broadphase_artifact.clone()] {
        errors.push(validation_error(format!(
            "collision pass '{}' must consume support and broadphase artifacts in order",
            pass.id
        )));
    }
    if pass.materializes.len() != 1 {
        errors.push(validation_error(format!(
            "collision pass '{}' must materialize exactly one transition collision intermediate",
            pass.id
        )));
    } else {
        available_values.insert(pass.materializes[0].clone());
    }
    for (artifact_id, expected_kind) in [
        (witness_artifact, CollisionArtifactKind::WitnessCache),
        (
            continuation_artifact,
            CollisionArtifactKind::ContinuationSeed,
        ),
    ] {
        let Some(binding) = declared_artifacts.get(artifact_id) else {
            errors.push(validation_error(format!(
                "collision pass '{}' references undeclared artifact '{}'",
                pass.id, artifact_id
            )));
            continue;
        };
        if binding.kind != expected_kind {
            errors.push(validation_error(format!(
                "collision pass '{}' expected '{}' artifact '{}'",
                pass.id,
                collision_artifact_kind_name(expected_kind),
                artifact_id
            )));
        }
        if binding.contract.producer != pass.id {
            errors.push(validation_error(format!(
                "collision artifact '{}' must be produced by '{}'",
                artifact_id, pass.id
            )));
        }
        produced_artifacts.insert(artifact_id.clone());
    }
}

fn validate_output_materialization(
    plan: &CollisionPlan,
    pass: &CollisionPass,
    output: CollisionOutputKind,
    available_values: &BTreeSet<SmolStr>,
    produced_values: &mut BTreeSet<SmolStr>,
    errors: &mut Vec<CollisionPlanValidationError>,
) {
    let Some(binding) = plan.outputs.iter().find(|binding| binding.kind == output) else {
        errors.push(validation_error(format!(
            "collision pass '{}' materializes unknown collision output '{}'",
            pass.id,
            collision_output_kind_name(output)
        )));
        return;
    };
    if pass.consumes.len() != 1 {
        errors.push(validation_error(format!(
            "collision pass '{}' must consume exactly one intermediate value",
            pass.id
        )));
    } else if !available_values.contains(&pass.consumes[0]) {
        errors.push(validation_error(format!(
            "collision pass '{}' must consume a materialized collision intermediate, found '{}'",
            pass.id, pass.consumes[0]
        )));
    }
    if pass.materializes.len() != 1 || pass.materializes[0] != binding.name {
        errors.push(validation_error(format!(
            "collision pass '{}' must materialize collision output '{}'",
            pass.id, binding.name
        )));
    } else {
        produced_values.insert(binding.name.clone());
    }
}

fn expected_artifact_kinds(target: CollisionTargetKind) -> Vec<CollisionArtifactKind> {
    match target {
        CollisionTargetKind::WorldSnapshot => vec![
            CollisionArtifactKind::SupportSummary,
            CollisionArtifactKind::BroadphaseCandidates,
        ],
        CollisionTargetKind::WorldTransition => vec![
            CollisionArtifactKind::SupportSummary,
            CollisionArtifactKind::BroadphaseCandidates,
            CollisionArtifactKind::WitnessCache,
            CollisionArtifactKind::ContinuationSeed,
        ],
    }
}

fn validity_contains_predicate(
    rule: &ArtifactValidityRule,
    expected: &ArtifactValidityPredicate,
) -> bool {
    match rule {
        ArtifactValidityRule::Always => false,
        ArtifactValidityRule::All(rules) | ArtifactValidityRule::Any(rules) => rules
            .iter()
            .any(|rule| validity_contains_predicate(rule, expected)),
        ArtifactValidityRule::Predicate(predicate) => predicate == expected,
    }
}

pub fn collision_artifact_kind_name(kind: CollisionArtifactKind) -> &'static str {
    match kind {
        CollisionArtifactKind::SupportSummary => "support_summary",
        CollisionArtifactKind::BroadphaseCandidates => "broadphase_candidates",
        CollisionArtifactKind::WitnessCache => "witness_cache",
        CollisionArtifactKind::ContinuationSeed => "continuation_seed",
    }
}

pub fn collision_reuse_verdict_name(value: CollisionReuseVerdict) -> &'static str {
    match value {
        CollisionReuseVerdict::Consumed => "consumed",
        CollisionReuseVerdict::Rejected => "rejected",
        CollisionReuseVerdict::Unavailable => "unavailable",
    }
}

pub fn collision_reuse_reason_name(value: CollisionReuseReason) -> &'static str {
    match value {
        CollisionReuseReason::None => "none",
        CollisionReuseReason::MissingPreviousSnapshot => "missing_previous_snapshot",
        CollisionReuseReason::CompatibilityRejected => "compatibility_rejected",
        CollisionReuseReason::ValidityRejected => "validity_rejected",
        CollisionReuseReason::ArtifactUnavailable => "artifact_unavailable",
        CollisionReuseReason::RenderingOnlyCertificate => "rendering_only_certificate",
    }
}
