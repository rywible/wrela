use crate::artifact_contract::{
    ArtifactCompatibilityRelation, ArtifactEvidenceCompatibility, ArtifactLogicalField,
    ArtifactLogicalSchema, ArtifactPolicyCompatibility, ArtifactSnapshotRelation,
    ArtifactTransitionRelation, ArtifactUse, ArtifactUseKind, ArtifactUseSource,
    ArtifactValidityPredicate, ArtifactValidityRule, SemanticArtifactContract,
    SemanticArtifactKind,
};
use crate::artifact_key::{ArtifactPolicyDigestMode, ArtifactReuseKey};
use crate::artifact_store::{
    ArtifactInstanceMetadata, ArtifactLookupRequest, ArtifactStore, ArtifactStoreReport,
    store_backed_use,
};
use crate::collision_contract::{
    COLLISION_POINT_OCCUPANCY_WORLD, COLLISION_RAY_CAST_WORLD, COLLISION_SPHERE_OVERLAP_WORLD,
    CollisionContractDescriptor, CollisionContractId, CollisionExecutionPolicy, CollisionFamilyId,
    CollisionInputKind, CollisionOccupancyClass, CollisionOccupancyResult, CollisionOutputKind,
    CollisionPointInput, CollisionPointWitness, CollisionQuestionId, CollisionRayCastResult,
    CollisionRayInput, CollisionRayMissReason, CollisionRayWitness, CollisionResult,
    CollisionSphereOverlapResult, CollisionSphereProbe, CollisionSphereWitness,
    CollisionTargetKind, CollisionWitnessSchema, collision_contract, collision_input_kind_name,
    collision_output_kind_name,
};
use crate::execution_policy::{QueryExecutionPolicy, RequiredGuaranteeClass, SelectedMethodClass};
use crate::kernel::{KernelStructValue, KernelValue, lower_world_query_plan};
use crate::query_contract::{self, DispatchBackend, QueryContractId};
use crate::query_exec::{
    DirectQueryExecutionTrace, QueryExecContext, QueryExecError,
    execute_world_query_with_policy_with_snapshot_on, execute_world_query_with_snapshot_on,
};
use crate::query_plan::{WorldQueryKind, WorldQueryPlan};
use crate::semantic_evidence::SemanticEvidenceSummary;
use crate::world_identity::{SnapshotIdentityReport, WorldSnapshotHandle};
use smol_str::SmolStr;
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

pub const COLLISION_PLAN_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CollisionQueryKind {
    PointOccupancyWorld,
    RayCastWorld,
    SphereOverlapWorld,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CollisionPassKind {
    GatherCandidates {
        support_summary_contract: QueryContractId,
        artifact_id: SmolStr,
    },
    EvaluatePointOccupancy {
        distance_contract: QueryContractId,
        normal_contract: QueryContractId,
        support_artifact: SmolStr,
    },
    CastRayFirstHit {
        trace_contract: QueryContractId,
        support_artifact: SmolStr,
    },
    ResolveSphereOverlap {
        distance_contract: QueryContractId,
        normal_contract: QueryContractId,
        support_artifact: SmolStr,
        supported_shape: SmolStr,
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
    pub backend: DispatchBackend,
    pub policy: CollisionExecutionPolicy,
    pub inputs: Vec<CollisionInputBinding>,
    pub passes: Vec<CollisionPass>,
    pub artifacts: Vec<SemanticArtifactContract>,
    pub outputs: Vec<CollisionOutputBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollisionPlanValidationError {
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollisionExecutionTrace {
    pub contract_id: CollisionContractId,
    pub family: CollisionFamilyId,
    pub question: CollisionQuestionId,
    pub backend: DispatchBackend,
    pub snapshot: Option<SnapshotIdentityReport>,
    pub required_guarantee: RequiredGuaranteeClass,
    pub selected_method: SelectedMethodClass,
    pub executed_query_contracts: Vec<QueryContractId>,
    pub artifact_store: ArtifactStoreReport,
}

#[derive(Debug, Clone, PartialEq)]
enum CollisionPassValue {
    Occupancy(CollisionOccupancyResult),
    RayCast(CollisionRayCastResult),
    SphereOverlap(CollisionSphereOverlapResult),
}

#[derive(Debug, Error, Clone, PartialEq)]
pub enum CollisionExecError {
    #[error("collision plan validation failed: {messages:?}")]
    Validation { messages: Vec<String> },
    #[error("collision execution expected {expected}, found {found}")]
    TypeMismatch { expected: String, found: String },
    #[error("collision execution expected a region capture input")]
    MissingRegionCapture,
    #[error("collision snapshot handle could not be resolved")]
    MissingSnapshotHandle,
    #[error("collision plan is missing required input binding '{kind}'")]
    MissingInputBinding { kind: String },
    #[error("collision plan is missing required output binding '{output}'")]
    MissingOutputBinding { output: String },
    #[error("collision output artifact '{artifact_id}' was not declared")]
    MissingArtifact { artifact_id: SmolStr },
    #[error("collision value '{record}' is missing required field '{field}'")]
    MissingField { record: String, field: String },
    #[error("collision pass '{pass_id}' expected materialized value '{value_id}'")]
    MissingPassValue { pass_id: SmolStr, value_id: SmolStr },
    #[error("collision pass '{pass_id}' is not well-formed: {message}")]
    InvalidPass { pass_id: SmolStr, message: String },
    #[error("collision plan references unknown query contract '{contract_id}'")]
    UnknownQueryContract { contract_id: String },
    #[error(transparent)]
    Query(#[from] QueryExecError),
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
            } => vec![*distance_contract, *normal_contract],
            Self::CastRayFirstHit { trace_contract, .. } => vec![*trace_contract],
            Self::MaterializeOutput { .. } => Vec::new(),
        }
    }
}

impl CollisionPassValue {
    fn kind(&self) -> CollisionOutputKind {
        match self {
            Self::Occupancy(_) => CollisionOutputKind::Occupancy,
            Self::RayCast(_) => CollisionOutputKind::RayCast,
            Self::SphereOverlap(_) => CollisionOutputKind::SphereOverlap,
        }
    }

    fn into_result(self) -> CollisionResult {
        match self {
            Self::Occupancy(value) => CollisionResult::Occupancy(value),
            Self::RayCast(value) => CollisionResult::RayCast(value),
            Self::SphereOverlap(value) => CollisionResult::SphereOverlap(value),
        }
    }
}

impl CollisionPlan {
    pub fn for_query(kind: CollisionQueryKind) -> Self {
        Self::for_query_with_backend(kind, DispatchBackend::Auto)
    }

    pub fn for_query_with_backend(kind: CollisionQueryKind, backend: DispatchBackend) -> Self {
        match kind {
            CollisionQueryKind::PointOccupancyWorld => {
                let descriptor = descriptor(COLLISION_POINT_OCCUPANCY_WORLD);
                let support_artifact = support_summary_artifact(descriptor);
                Self {
                    name: SmolStr::new("collision.point_occupancy.world"),
                    contract_id: descriptor.id,
                    contract_version: descriptor.version,
                    family: descriptor.family,
                    question: descriptor.question,
                    target: descriptor.target,
                    backend,
                    policy: descriptor.policy,
                    inputs: vec![
                        input_binding("world", CollisionInputKind::WorldCapture, "RegionCapture"),
                        input_binding("domain", CollisionInputKind::SceneDomain, "SceneDomain"),
                        input_binding("point", CollisionInputKind::Point, descriptor.input_record),
                    ],
                    passes: vec![
                        CollisionPass {
                            id: SmolStr::new("candidate_gather"),
                            kind: CollisionPassKind::GatherCandidates {
                                support_summary_contract: query_contract::SUPPORT_SUMMARY_WORLD,
                                artifact_id: support_artifact.id.clone(),
                            },
                            consumes: Vec::new(),
                            materializes: vec![support_artifact.id.clone()],
                        },
                        CollisionPass {
                            id: SmolStr::new("point_occupancy"),
                            kind: CollisionPassKind::EvaluatePointOccupancy {
                                distance_contract: query_contract::SPATIAL_DISTANCE_WORLD,
                                normal_contract: query_contract::SPATIAL_NORMAL_WORLD,
                                support_artifact: support_artifact.id.clone(),
                            },
                            consumes: vec![support_artifact.id.clone()],
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
                    artifacts: vec![support_artifact],
                    outputs: vec![output_binding(
                        "occupancy",
                        descriptor.output_kind,
                        descriptor.output_record,
                        Some(descriptor.witness_schema),
                    )],
                }
            }
            CollisionQueryKind::RayCastWorld => {
                let descriptor = descriptor(COLLISION_RAY_CAST_WORLD);
                let support_artifact = support_summary_artifact(descriptor);
                Self {
                    name: SmolStr::new("collision.ray_cast.world"),
                    contract_id: descriptor.id,
                    contract_version: descriptor.version,
                    family: descriptor.family,
                    question: descriptor.question,
                    target: descriptor.target,
                    backend,
                    policy: descriptor.policy,
                    inputs: vec![
                        input_binding("world", CollisionInputKind::WorldCapture, "RegionCapture"),
                        input_binding("domain", CollisionInputKind::SceneDomain, "SceneDomain"),
                        input_binding("ray", CollisionInputKind::Ray, descriptor.input_record),
                    ],
                    passes: vec![
                        CollisionPass {
                            id: SmolStr::new("candidate_gather"),
                            kind: CollisionPassKind::GatherCandidates {
                                support_summary_contract: query_contract::SUPPORT_SUMMARY_WORLD,
                                artifact_id: support_artifact.id.clone(),
                            },
                            consumes: Vec::new(),
                            materializes: vec![support_artifact.id.clone()],
                        },
                        CollisionPass {
                            id: SmolStr::new("ray_cast"),
                            kind: CollisionPassKind::CastRayFirstHit {
                                trace_contract: query_contract::SPATIAL_NEAREST_WORLD,
                                support_artifact: support_artifact.id.clone(),
                            },
                            consumes: vec![support_artifact.id.clone()],
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
                    artifacts: vec![support_artifact],
                    outputs: vec![output_binding(
                        "ray_cast",
                        descriptor.output_kind,
                        descriptor.output_record,
                        Some(descriptor.witness_schema),
                    )],
                }
            }
            CollisionQueryKind::SphereOverlapWorld => {
                let descriptor = descriptor(COLLISION_SPHERE_OVERLAP_WORLD);
                let support_artifact = support_summary_artifact(descriptor);
                Self {
                    name: SmolStr::new("collision.sphere_overlap.world"),
                    contract_id: descriptor.id,
                    contract_version: descriptor.version,
                    family: descriptor.family,
                    question: descriptor.question,
                    target: descriptor.target,
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
                            id: SmolStr::new("candidate_gather"),
                            kind: CollisionPassKind::GatherCandidates {
                                support_summary_contract: query_contract::SUPPORT_SUMMARY_WORLD,
                                artifact_id: support_artifact.id.clone(),
                            },
                            consumes: Vec::new(),
                            materializes: vec![support_artifact.id.clone()],
                        },
                        CollisionPass {
                            id: SmolStr::new("sphere_overlap"),
                            kind: CollisionPassKind::ResolveSphereOverlap {
                                distance_contract: query_contract::SPATIAL_DISTANCE_WORLD,
                                normal_contract: query_contract::SPATIAL_NORMAL_WORLD,
                                support_artifact: support_artifact.id.clone(),
                                supported_shape: SmolStr::new("sphere"),
                            },
                            consumes: vec![support_artifact.id.clone()],
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
                    artifacts: vec![support_artifact],
                    outputs: vec![output_binding(
                        "sphere_overlap",
                        descriptor.output_kind,
                        descriptor.output_record,
                        Some(descriptor.witness_schema),
                    )],
                }
            }
        }
    }

    pub fn semantic_artifact_contracts(&self) -> Vec<SemanticArtifactContract> {
        self.artifacts.clone()
    }

    pub fn artifact_uses(&self) -> Vec<ArtifactUse> {
        let mut uses = Vec::new();
        for pass in &self.passes {
            match &pass.kind {
                CollisionPassKind::GatherCandidates { artifact_id, .. } => {
                    if let Some(artifact) = self
                        .artifacts
                        .iter()
                        .find(|candidate| candidate.id == *artifact_id)
                    {
                        uses.push(ArtifactUse {
                            actor: pass.id.clone(),
                            artifact_id: artifact.id.clone(),
                            kind: ArtifactUseKind::Produce,
                            source: ArtifactUseSource::Plan,
                            required_validity: None,
                        });
                    }
                }
                CollisionPassKind::EvaluatePointOccupancy {
                    support_artifact, ..
                }
                | CollisionPassKind::CastRayFirstHit {
                    support_artifact, ..
                }
                | CollisionPassKind::ResolveSphereOverlap {
                    support_artifact, ..
                } => {
                    if let Some(artifact) = self
                        .artifacts
                        .iter()
                        .find(|candidate| candidate.id == *support_artifact)
                    {
                        uses.push(store_backed_use(
                            pass.id.clone(),
                            artifact.id.clone(),
                            artifact.validity.clone(),
                        ));
                    }
                }
                CollisionPassKind::MaterializeOutput { .. } => {}
            }
        }
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
        if self.inputs.len() != 3 {
            errors.push(validation_error(format!(
                "collision plan '{}' must bind world, domain, and one collision input",
                self.name
            )));
        }
        let mut seen_input_kinds = BTreeSet::new();
        for input in &self.inputs {
            if !seen_input_kinds.insert(input.kind) {
                errors.push(validation_error(format!(
                    "collision plan '{}' binds collision input kind '{}' more than once",
                    self.name,
                    collision_input_kind_name(input.kind)
                )));
            }
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
        validate_input_binding(
            &self.name,
            &self.inputs,
            descriptor.input_kind,
            descriptor.input_record,
            &mut errors,
        );
        if self.outputs.len() != 1 {
            errors.push(validation_error(format!(
                "collision plan '{}' must expose exactly one typed output",
                self.name
            )));
        }
        let mut seen_output_kinds = BTreeSet::new();
        for output in &self.outputs {
            if !seen_output_kinds.insert(output.kind) {
                errors.push(validation_error(format!(
                    "collision plan '{}' binds collision output kind '{}' more than once",
                    self.name,
                    collision_output_kind_name(output.kind)
                )));
            }
            if output.witness_schema.is_none() {
                errors.push(validation_error(format!(
                    "collision output '{}' is missing an explicit witness schema",
                    output.name
                )));
            }
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
        let declared_artifacts = self
            .artifacts
            .iter()
            .map(|artifact| artifact.id.clone())
            .collect::<BTreeSet<_>>();
        let mut available_artifacts = BTreeSet::new();
        let mut available_values = BTreeSet::new();
        let mut gather_passes = 0usize;
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
                    artifact_id,
                } => {
                    gather_passes += 1;
                    if *support_summary_contract != query_contract::SUPPORT_SUMMARY_WORLD {
                        errors.push(validation_error(format!(
                            "collision pass '{}' must gather candidates through '{}'",
                            pass.id,
                            query_contract::SUPPORT_SUMMARY_WORLD.as_str()
                        )));
                    }
                    if !declared_artifacts.contains(artifact_id) {
                        errors.push(validation_error(format!(
                            "collision pass '{}' materializes undeclared artifact '{}'",
                            pass.id, artifact_id
                        )));
                    }
                    if !pass.consumes.is_empty() {
                        errors.push(validation_error(format!(
                            "collision pass '{}' should not consume values while gathering candidates",
                            pass.id
                        )));
                    }
                    if pass.materializes.len() != 1 || pass.materializes[0] != *artifact_id {
                        errors.push(validation_error(format!(
                            "collision pass '{}' must materialize support artifact '{}'",
                            pass.id, artifact_id
                        )));
                    } else {
                        available_artifacts.insert(artifact_id.clone());
                    }
                }
                CollisionPassKind::EvaluatePointOccupancy {
                    distance_contract,
                    normal_contract,
                    support_artifact,
                } => {
                    evaluation_passes += 1;
                    if descriptor.question != CollisionQuestionId::PointOccupancy {
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
                    validate_support_artifact_pass(
                        pass,
                        support_artifact,
                        &declared_artifacts,
                        &available_artifacts,
                        &available_values,
                        &mut errors,
                    );
                    if pass.materializes.len() == 1 {
                        available_values.insert(pass.materializes[0].clone());
                    }
                }
                CollisionPassKind::CastRayFirstHit {
                    trace_contract,
                    support_artifact,
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
                    validate_support_artifact_pass(
                        pass,
                        support_artifact,
                        &declared_artifacts,
                        &available_artifacts,
                        &available_values,
                        &mut errors,
                    );
                    if pass.materializes.len() == 1 {
                        available_values.insert(pass.materializes[0].clone());
                    }
                }
                CollisionPassKind::ResolveSphereOverlap {
                    distance_contract,
                    normal_contract,
                    support_artifact,
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
                    validate_support_artifact_pass(
                        pass,
                        support_artifact,
                        &declared_artifacts,
                        &available_artifacts,
                        &available_values,
                        &mut errors,
                    );
                    if pass.materializes.len() == 1 {
                        available_values.insert(pass.materializes[0].clone());
                    }
                }
                CollisionPassKind::MaterializeOutput { output } => {
                    materialization_passes += 1;
                    let Some(binding) = self
                        .outputs
                        .iter()
                        .find(|candidate| candidate.kind == *output)
                    else {
                        errors.push(validation_error(format!(
                            "collision pass '{}' materializes unknown collision output '{}'",
                            pass.id,
                            collision_output_kind_name(*output)
                        )));
                        continue;
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
                        available_values.insert(binding.name.clone());
                    }
                }
            }
        }
        if gather_passes != 1 {
            errors.push(validation_error(format!(
                "collision plan '{}' must declare exactly one candidate gathering pass",
                self.name
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
        for artifact_id in &declared_artifacts {
            if !available_artifacts.contains(artifact_id) {
                errors.push(validation_error(format!(
                    "collision artifact '{}' is declared but never materialized by a pass",
                    artifact_id
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
        errors
    }

    pub fn execute(
        &self,
        ctx: &QueryExecContext,
        args: &[KernelValue],
    ) -> Result<(CollisionResult, CollisionExecutionTrace), CollisionExecError> {
        let validation = self.validate();
        if !validation.is_empty() {
            return Err(CollisionExecError::Validation {
                messages: validation.into_iter().map(|err| err.message).collect(),
            });
        }

        let backend = resolve_backend(self.backend);
        let descriptor = descriptor(self.contract_id);
        let world_binding = input_binding_index(self, CollisionInputKind::WorldCapture)?;
        let domain_binding = input_binding_index(self, CollisionInputKind::SceneDomain)?;
        let collision_binding = input_binding_index(self, descriptor.input_kind)?;
        let (capture, snapshot) = resolve_region_capture(ctx, args.get(world_binding))?;
        let domain = args
            .get(domain_binding)
            .cloned()
            .ok_or_else(|| type_mismatch("SceneDomain", "missing"))?;
        let collision_input = args
            .get(collision_binding)
            .ok_or_else(|| type_mismatch(descriptor.input_record, "missing"))?;
        let policy = QueryExecutionPolicy::new(
            backend,
            self.policy.required_guarantee,
            self.policy.selected_method,
            None,
        );

        let mut executed_query_contracts = Vec::new();
        let mut artifact_store = ArtifactStore::<()>::default();
        let mut materialized_values = BTreeMap::<SmolStr, CollisionPassValue>::new();
        for pass in &self.passes {
            match &pass.kind {
                CollisionPassKind::GatherCandidates {
                    support_summary_contract,
                    artifact_id,
                } => {
                    let artifact = declared_artifact(self, artifact_id)?;
                    let (_support_value, support_trace) = execute_world_query_contract(
                        ctx,
                        backend,
                        None,
                        &snapshot,
                        *support_summary_contract,
                        &[capture.clone(), domain.clone()],
                    )?;
                    executed_query_contracts.push(support_trace.contract_id);
                    insert_artifact(
                        &mut artifact_store,
                        artifact,
                        &snapshot,
                        collision_policy_digest(self.policy),
                    );
                }
                CollisionPassKind::EvaluatePointOccupancy {
                    distance_contract,
                    normal_contract,
                    support_artifact,
                } => {
                    let artifact = declared_artifact(self, support_artifact)?;
                    ensure_artifact_available(&artifact_store, artifact, &snapshot, self)?;
                    let point = collision_point_input(collision_input)?;
                    let (distance, distance_trace) = execute_point_query(
                        ctx,
                        backend,
                        &policy,
                        &snapshot,
                        &capture,
                        &domain,
                        point.point,
                        *distance_contract,
                    )?;
                    executed_query_contracts.push(distance_trace.contract_id);
                    let (normal, normal_trace) = execute_point_query(
                        ctx,
                        backend,
                        &policy,
                        &snapshot,
                        &capture,
                        &domain,
                        point.point,
                        *normal_contract,
                    )?;
                    executed_query_contracts.push(normal_trace.contract_id);
                    let signed_distance = expect_f32(&distance)?;
                    let world_normal = expect_vec3(&normal)?;
                    materialize_pass_values(
                        pass,
                        CollisionPassValue::Occupancy(CollisionOccupancyResult {
                            classification: classify_occupancy(signed_distance),
                            occupied: signed_distance <= 0.0,
                            signed_distance,
                            witness: CollisionPointWitness {
                                sample_point: point.point,
                                nearest_point_on_world: offset_point(
                                    point.point,
                                    world_normal,
                                    -signed_distance,
                                ),
                                world_normal,
                                signed_distance,
                            },
                        }),
                        &mut materialized_values,
                    )?;
                }
                CollisionPassKind::CastRayFirstHit {
                    trace_contract,
                    support_artifact,
                } => {
                    let artifact = declared_artifact(self, support_artifact)?;
                    ensure_artifact_available(&artifact_store, artifact, &snapshot, self)?;
                    let ray = collision_ray_input(collision_input)?;
                    let (hit, trace) = execute_world_query_contract(
                        ctx,
                        backend,
                        Some(&policy),
                        &snapshot,
                        *trace_contract,
                        &[capture.clone(), domain.clone(), ray_query_value(ray)],
                    )?;
                    executed_query_contracts.push(trace.contract_id);
                    let hit_ref = expect_struct(&hit, "Hit3")?;
                    let hit_flag = expect_bool(field(hit_ref, "hit")?)?;
                    materialize_pass_values(
                        pass,
                        CollisionPassValue::RayCast(if hit_flag {
                            CollisionRayCastResult {
                                hit: true,
                                miss_reason: CollisionRayMissReason::None,
                                witness: Some(CollisionRayWitness {
                                    travel_distance: expect_f32(field(hit_ref, "distance")?)?,
                                    position: expect_vec3(field(hit_ref, "position")?)?,
                                    normal: expect_vec3(field(hit_ref, "normal")?)?,
                                    root_shape_id: expect_u32(field(hit_ref, "root_shape_id")?)?,
                                    feature_id: expect_u32(field(hit_ref, "feature_id")?)?,
                                }),
                            }
                        } else {
                            CollisionRayCastResult {
                                hit: false,
                                miss_reason: CollisionRayMissReason::NoHitWithinRange,
                                witness: None,
                            }
                        }),
                        &mut materialized_values,
                    )?;
                }
                CollisionPassKind::ResolveSphereOverlap {
                    distance_contract,
                    normal_contract,
                    support_artifact,
                    supported_shape,
                } => {
                    let artifact = declared_artifact(self, support_artifact)?;
                    ensure_artifact_available(&artifact_store, artifact, &snapshot, self)?;
                    let probe = match supported_shape.as_str() {
                        "sphere" => collision_sphere_input(collision_input)?,
                        _ => {
                            return Err(CollisionExecError::InvalidPass {
                                pass_id: pass.id.clone(),
                                message: format!(
                                    "unsupported shape '{}'; expected 'sphere'",
                                    supported_shape
                                ),
                            });
                        }
                    };
                    let (distance, distance_trace) = execute_point_query(
                        ctx,
                        backend,
                        &policy,
                        &snapshot,
                        &capture,
                        &domain,
                        probe.center,
                        *distance_contract,
                    )?;
                    executed_query_contracts.push(distance_trace.contract_id);
                    let (normal, normal_trace) = execute_point_query(
                        ctx,
                        backend,
                        &policy,
                        &snapshot,
                        &capture,
                        &domain,
                        probe.center,
                        *normal_contract,
                    )?;
                    executed_query_contracts.push(normal_trace.contract_id);
                    let center_distance = expect_f32(&distance)?;
                    let world_normal = expect_vec3(&normal)?;
                    let signed_separation = center_distance - probe.radius;
                    materialize_pass_values(
                        pass,
                        CollisionPassValue::SphereOverlap(CollisionSphereOverlapResult {
                            overlaps: signed_separation <= 0.0,
                            signed_separation,
                            witness: CollisionSphereWitness {
                                point_on_probe: offset_point(
                                    probe.center,
                                    world_normal,
                                    -probe.radius,
                                ),
                                point_on_world: offset_point(
                                    probe.center,
                                    world_normal,
                                    -center_distance,
                                ),
                                world_normal,
                                signed_separation,
                            },
                        }),
                        &mut materialized_values,
                    )?;
                }
                CollisionPassKind::MaterializeOutput { output } => {
                    let binding = output_binding_for_kind(self, *output)?;
                    let source_value =
                        pass.consumes
                            .first()
                            .ok_or_else(|| CollisionExecError::InvalidPass {
                                pass_id: pass.id.clone(),
                                message: "missing consumed intermediate value".to_string(),
                            })?;
                    let value =
                        materialized_values
                            .get(source_value)
                            .cloned()
                            .ok_or_else(|| CollisionExecError::MissingPassValue {
                                pass_id: pass.id.clone(),
                                value_id: source_value.clone(),
                            })?;
                    if value.kind() != *output {
                        return Err(CollisionExecError::InvalidPass {
                            pass_id: pass.id.clone(),
                            message: format!(
                                "output '{}' expects '{}', found '{}'",
                                binding.name,
                                collision_output_kind_name(*output),
                                collision_output_kind_name(value.kind())
                            ),
                        });
                    }
                    materialize_pass_values(pass, value, &mut materialized_values)?;
                }
            }
        }
        let output_binding =
            self.outputs
                .first()
                .ok_or_else(|| CollisionExecError::MissingOutputBinding {
                    output: collision_output_kind_name(descriptor.output_kind).to_string(),
                })?;
        let result = materialized_values
            .remove(&output_binding.name)
            .ok_or_else(|| CollisionExecError::MissingPassValue {
                pass_id: SmolStr::new("materialize_output"),
                value_id: output_binding.name.clone(),
            })?
            .into_result();

        Ok((
            result,
            CollisionExecutionTrace {
                contract_id: self.contract_id,
                family: self.family,
                question: self.question,
                backend,
                snapshot: Some(snapshot.report()),
                required_guarantee: self.policy.required_guarantee,
                selected_method: self.policy.selected_method,
                executed_query_contracts,
                artifact_store: artifact_store.report(),
            },
        ))
    }
}

pub fn collision_plans_with_backend(backend: DispatchBackend) -> Vec<CollisionPlan> {
    [
        CollisionQueryKind::PointOccupancyWorld,
        CollisionQueryKind::RayCastWorld,
        CollisionQueryKind::SphereOverlapWorld,
    ]
    .into_iter()
    .map(|kind| CollisionPlan::for_query_with_backend(kind, backend))
    .collect()
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

fn support_summary_artifact(descriptor: &CollisionContractDescriptor) -> SemanticArtifactContract {
    SemanticArtifactContract {
        id: SmolStr::new("artifact.support_summary"),
        kind: SemanticArtifactKind::Query,
        logical_schema: ArtifactLogicalSchema {
            namespace: SmolStr::new("collision"),
            name: SmolStr::new("support-summary"),
            fields: vec![
                ArtifactLogicalField::new("collision_contract", descriptor.id.as_str()),
                ArtifactLogicalField::new(
                    "query_contract",
                    query_contract::SUPPORT_SUMMARY_WORLD.as_str(),
                ),
                ArtifactLogicalField::new("target", "world_snapshot"),
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
                origin: SemanticEvidenceSummary::contract_bound().origin,
                scope: SemanticEvidenceSummary::contract_bound().scope,
            },
        },
        validity: ArtifactValidityRule::all(vec![
            ArtifactValidityRule::predicate(
                ArtifactValidityPredicate::CurrentSnapshotMatchesStored,
            ),
            ArtifactValidityRule::predicate(ArtifactValidityPredicate::PolicyDigestMatches),
            ArtifactValidityRule::predicate(ArtifactValidityPredicate::EvidenceSummaryMatches),
        ]),
        producer: SmolStr::new("candidate_gather"),
        consumer: SmolStr::new("collision.resolve"),
        deterministic: true,
        version: COLLISION_PLAN_SCHEMA_VERSION,
        transition: None,
        evidence_summary: SemanticEvidenceSummary::contract_bound(),
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

fn validate_support_artifact_pass(
    pass: &CollisionPass,
    support_artifact: &SmolStr,
    declared_artifacts: &BTreeSet<SmolStr>,
    available_artifacts: &BTreeSet<SmolStr>,
    available_values: &BTreeSet<SmolStr>,
    errors: &mut Vec<CollisionPlanValidationError>,
) {
    if !declared_artifacts.contains(support_artifact) {
        errors.push(validation_error(format!(
            "collision pass '{}' references undeclared support artifact '{}'",
            pass.id, support_artifact
        )));
    }
    if !pass
        .consumes
        .iter()
        .any(|value_id| value_id == support_artifact)
    {
        errors.push(validation_error(format!(
            "collision pass '{}' must consume support artifact '{}'",
            pass.id, support_artifact
        )));
    }
    for value_id in &pass.consumes {
        let available = if value_id == support_artifact {
            available_artifacts.contains(value_id)
        } else {
            available_values.contains(value_id)
        };
        if !available {
            errors.push(validation_error(format!(
                "collision pass '{}' consumes value '{}' before it is materialized",
                pass.id, value_id
            )));
        }
    }
    if pass.materializes.len() != 1 {
        errors.push(validation_error(format!(
            "collision pass '{}' must materialize exactly one collision intermediate",
            pass.id
        )));
    }
}

fn input_binding_index(
    plan: &CollisionPlan,
    kind: CollisionInputKind,
) -> Result<usize, CollisionExecError> {
    plan.inputs
        .iter()
        .position(|binding| binding.kind == kind)
        .ok_or_else(|| CollisionExecError::MissingInputBinding {
            kind: collision_input_kind_name(kind).to_string(),
        })
}

fn output_binding_for_kind<'a>(
    plan: &'a CollisionPlan,
    kind: CollisionOutputKind,
) -> Result<&'a CollisionOutputBinding, CollisionExecError> {
    plan.outputs
        .iter()
        .find(|binding| binding.kind == kind)
        .ok_or_else(|| CollisionExecError::MissingOutputBinding {
            output: collision_output_kind_name(kind).to_string(),
        })
}

fn declared_artifact<'a>(
    plan: &'a CollisionPlan,
    artifact_id: &SmolStr,
) -> Result<&'a SemanticArtifactContract, CollisionExecError> {
    plan.artifacts
        .iter()
        .find(|artifact| artifact.id == *artifact_id)
        .ok_or_else(|| CollisionExecError::MissingArtifact {
            artifact_id: artifact_id.clone(),
        })
}

fn materialize_pass_values(
    pass: &CollisionPass,
    value: CollisionPassValue,
    materialized_values: &mut BTreeMap<SmolStr, CollisionPassValue>,
) -> Result<(), CollisionExecError> {
    if pass.materializes.is_empty() {
        return Err(CollisionExecError::InvalidPass {
            pass_id: pass.id.clone(),
            message: "pass does not materialize any value ids".to_string(),
        });
    }
    for value_id in &pass.materializes {
        materialized_values.insert(value_id.clone(), value.clone());
    }
    Ok(())
}

fn resolve_backend(backend: DispatchBackend) -> DispatchBackend {
    match backend {
        DispatchBackend::Cpu | DispatchBackend::Auto => DispatchBackend::Cpu,
        DispatchBackend::VirtualGpu => DispatchBackend::VirtualGpu,
        DispatchBackend::Wgsl => DispatchBackend::Wgsl,
    }
}

fn resolve_region_capture(
    ctx: &QueryExecContext,
    value: Option<&KernelValue>,
) -> Result<(KernelValue, WorldSnapshotHandle), CollisionExecError> {
    let value = value.ok_or(CollisionExecError::MissingRegionCapture)?;
    match value {
        KernelValue::Capture(name) => {
            let snapshot = ctx
                .region_snapshot_handle(name)
                .cloned()
                .ok_or(CollisionExecError::MissingSnapshotHandle)?;
            Ok((KernelValue::Capture(name.clone()), snapshot))
        }
        KernelValue::Struct(struct_value) if struct_value.name.as_str() == "RegionCapture" => {
            let scene_id = expect_u32(field(struct_value, "scene_id")?)?;
            let epoch = expect_u32(field(struct_value, "epoch")?)?;
            let name = ctx
                .region_name_for_scene_id(scene_id)
                .cloned()
                .ok_or(CollisionExecError::MissingSnapshotHandle)?;
            let snapshot = ctx
                .region_snapshot_handle(&name)
                .map(|snapshot| {
                    snapshot.with_epoch(crate::world_identity::SnapshotEpoch(u64::from(epoch)))
                })
                .ok_or(CollisionExecError::MissingSnapshotHandle)?;
            Ok((KernelValue::Struct(struct_value.clone()), snapshot))
        }
        other => Err(type_mismatch("RegionCapture", kernel_value_kind(other))),
    }
}

fn execute_point_query(
    ctx: &QueryExecContext,
    backend: DispatchBackend,
    policy: &QueryExecutionPolicy,
    snapshot: &WorldSnapshotHandle,
    capture: &KernelValue,
    domain: &KernelValue,
    point: [f32; 3],
    contract_id: QueryContractId,
) -> Result<(KernelValue, DirectQueryExecutionTrace), CollisionExecError> {
    execute_world_query_contract(
        ctx,
        backend,
        Some(policy),
        snapshot,
        contract_id,
        &[capture.clone(), domain.clone(), KernelValue::Vec3(point)],
    )
}

fn execute_world_query_contract(
    ctx: &QueryExecContext,
    backend: DispatchBackend,
    policy: Option<&QueryExecutionPolicy>,
    snapshot: &WorldSnapshotHandle,
    contract_id: QueryContractId,
    args: &[KernelValue],
) -> Result<(KernelValue, DirectQueryExecutionTrace), CollisionExecError> {
    let kind = world_query_kind_for_contract(contract_id)?;
    let plan = lower_world_query_plan(&WorldQueryPlan::for_query_with_backend(kind, backend));
    match policy {
        Some(policy) => execute_world_query_with_policy_with_snapshot_on(
            ctx,
            backend,
            Some(snapshot),
            policy,
            &plan,
            args,
        )
        .map_err(CollisionExecError::from),
        None => execute_world_query_with_snapshot_on(ctx, backend, Some(snapshot), &plan, args)
            .map_err(CollisionExecError::from),
    }
}

fn world_query_kind_for_contract(
    contract_id: QueryContractId,
) -> Result<WorldQueryKind, CollisionExecError> {
    match contract_id {
        query_contract::SUPPORT_SUMMARY_WORLD => Ok(WorldQueryKind::SupportSummary),
        query_contract::SPATIAL_DISTANCE_WORLD => Ok(WorldQueryKind::Distance),
        query_contract::SPATIAL_NORMAL_WORLD => Ok(WorldQueryKind::Normal),
        query_contract::SPATIAL_NEAREST_WORLD => Ok(WorldQueryKind::Trace),
        _ => Err(CollisionExecError::UnknownQueryContract {
            contract_id: contract_id.as_str().to_string(),
        }),
    }
}

fn collision_policy_digest(policy: CollisionExecutionPolicy) -> u64 {
    let backend_tag = [match policy.backend_preference {
        DispatchBackend::Cpu => 0,
        DispatchBackend::VirtualGpu => 1,
        DispatchBackend::Wgsl => 2,
        DispatchBackend::Auto => 3,
    }];
    crate::query_exec::ids::stable_semantic_id(&[
        &policy.required_guarantee.id().to_le_bytes(),
        &policy.selected_method.id().to_le_bytes(),
        &backend_tag,
    ])
}

fn insert_artifact(
    store: &mut ArtifactStore<()>,
    artifact: &SemanticArtifactContract,
    snapshot: &WorldSnapshotHandle,
    policy_digest: u64,
) {
    let logical_schema = artifact.logical_schema.describe();
    let reuse_key = ArtifactReuseKey::new(
        snapshot,
        Some(artifact.id.clone()),
        logical_schema,
        artifact.logical_schema.stable_hash(),
        Some(policy_digest),
        artifact.compatibility.policy.mode,
    );
    store.insert(crate::artifact_store::StoredArtifact {
        contract: artifact.clone(),
        metadata: ArtifactInstanceMetadata {
            snapshot: snapshot.clone(),
            reuse_key,
            policy_digest: Some(policy_digest),
            presentation_frame: None,
            layout_signature: None,
            history_compatibility_hash: None,
            evidence_summary: artifact.evidence_summary.clone(),
        },
        payload: (),
    });
}

fn ensure_artifact_available(
    store: &ArtifactStore<()>,
    artifact: &SemanticArtifactContract,
    snapshot: &WorldSnapshotHandle,
    plan: &CollisionPlan,
) -> Result<(), CollisionExecError> {
    let request = ArtifactLookupRequest {
        contract: artifact.clone(),
        current_snapshot: snapshot.clone(),
        previous_snapshot_epoch: None,
        change_class: None,
        policy_digest: Some(collision_policy_digest(plan.policy)),
        presentation_frame: None,
        layout_signature: None,
        history_compatibility_hash: None,
        evidence_summary: Some(artifact.evidence_summary.clone()),
    };
    let (artifact, _) = store.lookup(&request);
    if artifact.is_some() {
        Ok(())
    } else {
        Err(CollisionExecError::MissingArtifact {
            artifact_id: request.contract.id,
        })
    }
}

fn classify_occupancy(signed_distance: f32) -> CollisionOccupancyClass {
    if signed_distance < 0.0 {
        CollisionOccupancyClass::Occupied
    } else if signed_distance <= 0.0001 {
        CollisionOccupancyClass::Boundary
    } else {
        CollisionOccupancyClass::Empty
    }
}

fn offset_point(point: [f32; 3], normal: [f32; 3], distance: f32) -> [f32; 3] {
    [
        point[0] + normal[0] * distance,
        point[1] + normal[1] * distance,
        point[2] + normal[2] * distance,
    ]
}

fn collision_point_input(value: &KernelValue) -> Result<CollisionPointInput, CollisionExecError> {
    let point = expect_struct(value, "CollisionPointInput")?;
    Ok(CollisionPointInput {
        point: expect_vec3(field(point, "point")?)?,
    })
}

fn collision_ray_input(value: &KernelValue) -> Result<CollisionRayInput, CollisionExecError> {
    let ray = expect_struct(value, "CollisionRayInput")?;
    Ok(CollisionRayInput {
        origin: expect_vec3(field(ray, "origin")?)?,
        direction: expect_vec3(field(ray, "direction")?)?,
        max_distance: expect_f32(field(ray, "max_distance")?)?,
        min_step: expect_f32(field(ray, "min_step")?)?,
        hit_epsilon: expect_f32(field(ray, "hit_epsilon")?)?,
        max_steps: expect_i32(field(ray, "max_steps")?)?,
    })
}

fn collision_sphere_input(value: &KernelValue) -> Result<CollisionSphereProbe, CollisionExecError> {
    let probe = expect_struct(value, "CollisionSphereProbe")?;
    Ok(CollisionSphereProbe {
        center: expect_vec3(field(probe, "center")?)?,
        radius: expect_f32(field(probe, "radius")?)?,
    })
}

fn ray_query_value(ray: CollisionRayInput) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("RayQuery"),
        fields: vec![
            (SmolStr::new("origin"), KernelValue::Vec3(ray.origin)),
            (SmolStr::new("direction"), KernelValue::Vec3(ray.direction)),
            (
                SmolStr::new("max_distance"),
                KernelValue::F32(ray.max_distance),
            ),
            (SmolStr::new("min_step"), KernelValue::F32(ray.min_step)),
            (
                SmolStr::new("hit_epsilon"),
                KernelValue::F32(ray.hit_epsilon),
            ),
            (SmolStr::new("max_steps"), KernelValue::I32(ray.max_steps)),
        ],
    })
}

fn expect_struct<'a>(
    value: &'a KernelValue,
    name: &str,
) -> Result<&'a KernelStructValue, CollisionExecError> {
    match value {
        KernelValue::Struct(value) if value.name.as_str() == name => Ok(value),
        other => Err(type_mismatch(name, kernel_value_kind(other))),
    }
}

fn field<'a>(
    value: &'a KernelStructValue,
    name: &str,
) -> Result<&'a KernelValue, CollisionExecError> {
    value
        .fields
        .iter()
        .find(|(field_name, _)| field_name.as_str() == name)
        .map(|(_, value)| value)
        .ok_or_else(|| CollisionExecError::MissingField {
            record: value.name.to_string(),
            field: name.to_string(),
        })
}

fn expect_bool(value: &KernelValue) -> Result<bool, CollisionExecError> {
    match value {
        KernelValue::Bool(value) => Ok(*value),
        other => Err(type_mismatch("Bool", kernel_value_kind(other))),
    }
}

fn expect_f32(value: &KernelValue) -> Result<f32, CollisionExecError> {
    match value {
        KernelValue::F32(value) => Ok(*value),
        other => Err(type_mismatch("F32", kernel_value_kind(other))),
    }
}

fn expect_i32(value: &KernelValue) -> Result<i32, CollisionExecError> {
    match value {
        KernelValue::I32(value) => Ok(*value),
        other => Err(type_mismatch("I32", kernel_value_kind(other))),
    }
}

fn expect_u32(value: &KernelValue) -> Result<u32, CollisionExecError> {
    match value {
        KernelValue::U32(value) => Ok(*value),
        other => Err(type_mismatch("U32", kernel_value_kind(other))),
    }
}

fn expect_vec3(value: &KernelValue) -> Result<[f32; 3], CollisionExecError> {
    match value {
        KernelValue::Vec3(value) => Ok(*value),
        other => Err(type_mismatch("Vec3", kernel_value_kind(other))),
    }
}

fn type_mismatch(expected: &str, found: impl Into<String>) -> CollisionExecError {
    CollisionExecError::TypeMismatch {
        expected: expected.to_string(),
        found: found.into(),
    }
}

fn kernel_value_kind(value: &KernelValue) -> String {
    match value {
        KernelValue::Bool(_) => "Bool".to_string(),
        KernelValue::I32(_) => "I32".to_string(),
        KernelValue::U32(_) => "U32".to_string(),
        KernelValue::F32(_) => "F32".to_string(),
        KernelValue::Vec3(_) => "Vec3".to_string(),
        KernelValue::Struct(value) => value.name.to_string(),
        KernelValue::Capture(name) => format!("Capture({name})"),
        KernelValue::Array(_) => "Array".to_string(),
        other => format!("{other:?}"),
    }
}
