use crate::execution_policy::{RequiredGuaranteeClass, SelectedMethodClass};
use crate::query_contract::DispatchBackend;
use crate::semantic_evidence::EvidenceScope;
use crate::state_advance::{ChangeClass, ChangeCompatibility};
use std::fmt;

pub const COLLISION_CONTRACT_VERSION: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CollisionContractId(&'static str);

impl CollisionContractId {
    pub const fn new(value: &'static str) -> Self {
        Self(value)
    }

    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

impl fmt::Display for CollisionContractId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CollisionFamilyId {
    Occupancy,
    RayCast,
    Overlap,
    Sweep,
    TimeOfImpact,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CollisionQuestionId {
    PointOccupancy,
    RayCastFirstHit,
    SphereOverlap,
    SphereSweepFirstContact,
    SphereTimeOfImpact,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CollisionTargetKind {
    WorldSnapshot,
    WorldTransition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CollisionInputKind {
    WorldCapture,
    SceneDomain,
    SnapshotTransition,
    Point,
    Ray,
    SphereProbe,
    SphereSweep,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CollisionOutputKind {
    Occupancy,
    RayCast,
    SphereOverlap,
    SweepContact,
    TimeOfImpact,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CollisionWitnessKind {
    PointContainment,
    RayHit,
    SphereWorldPair,
    SweepContact,
    TimeOfImpactContact,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CollisionAuthorityScope {
    Snapshot,
    Transition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CollisionContactNormalFlavor {
    SurfaceGradient,
    ConservativeUpperBound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CollisionContactNormalProvenance {
    CertifiedFieldGradient,
    FeatureNormal,
    HeuristicShadingNormal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CollisionAuthorityRequirement {
    pub scope: CollisionAuthorityScope,
    pub requires_previous_snapshot: bool,
    pub required_evidence_scope: EvidenceScope,
    pub transition_compatibility: Option<ChangeCompatibility>,
}

impl CollisionAuthorityRequirement {
    pub const fn snapshot(required_evidence_scope: EvidenceScope) -> Self {
        Self {
            scope: CollisionAuthorityScope::Snapshot,
            requires_previous_snapshot: false,
            required_evidence_scope,
            transition_compatibility: None,
        }
    }

    pub const fn transition(
        compatibility: ChangeCompatibility,
        required_evidence_scope: EvidenceScope,
    ) -> Self {
        Self {
            scope: CollisionAuthorityScope::Transition,
            requires_previous_snapshot: true,
            required_evidence_scope,
            transition_compatibility: Some(compatibility),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CollisionWitnessField {
    pub name: &'static str,
    pub ty: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CollisionWitnessSchema {
    pub name: &'static str,
    pub kind: CollisionWitnessKind,
    pub fields: &'static [CollisionWitnessField],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CollisionExecutionPolicy {
    pub backend_preference: DispatchBackend,
    pub required_guarantee: RequiredGuaranteeClass,
    pub selected_method: SelectedMethodClass,
}

impl CollisionExecutionPolicy {
    pub const fn exact_oracle(backend_preference: DispatchBackend) -> Self {
        Self {
            backend_preference,
            required_guarantee: RequiredGuaranteeClass::Exact,
            selected_method: SelectedMethodClass::ExactOracle,
        }
    }

    pub const fn conservative(backend_preference: DispatchBackend) -> Self {
        Self {
            backend_preference,
            required_guarantee: RequiredGuaranteeClass::ConservativeNoFalseMiss,
            selected_method: SelectedMethodClass::ConservativeSolver,
        }
    }

    pub const fn interval_bounded(backend_preference: DispatchBackend) -> Self {
        Self {
            backend_preference,
            required_guarantee: RequiredGuaranteeClass::IntervalBounded,
            selected_method: SelectedMethodClass::IntervalSolver,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CollisionBackendSupport {
    pub cpu: bool,
    pub virtual_gpu: bool,
    pub wgsl: bool,
}

impl CollisionBackendSupport {
    pub const fn cpu_only() -> Self {
        Self {
            cpu: true,
            virtual_gpu: false,
            wgsl: false,
        }
    }

    pub const fn cpu_and_wgsl() -> Self {
        Self {
            cpu: true,
            virtual_gpu: false,
            wgsl: true,
        }
    }

    pub const fn supports(self, backend: DispatchBackend) -> bool {
        match backend {
            DispatchBackend::Cpu => self.cpu,
            DispatchBackend::VirtualGpu => self.virtual_gpu,
            DispatchBackend::Wgsl => self.wgsl,
            DispatchBackend::Auto => self.cpu || self.virtual_gpu || self.wgsl,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CollisionContractDescriptor {
    pub id: CollisionContractId,
    pub version: u32,
    pub family: CollisionFamilyId,
    pub question: CollisionQuestionId,
    pub target: CollisionTargetKind,
    pub authority: CollisionAuthorityRequirement,
    pub input_kind: CollisionInputKind,
    pub input_record: &'static str,
    pub output_kind: CollisionOutputKind,
    pub output_record: &'static str,
    pub witness_schema: &'static CollisionWitnessSchema,
    pub policy: CollisionExecutionPolicy,
    pub supported_backends: CollisionBackendSupport,
}

const TRANSITION_COMPATIBILITY: ChangeCompatibility =
    ChangeCompatibility::new(ChangeClass::Presentation);

pub const COLLISION_POINT_OCCUPANCY_WORLD: CollisionContractId =
    CollisionContractId::new("collision.point_occupancy.world");
pub const COLLISION_RAY_CAST_WORLD: CollisionContractId =
    CollisionContractId::new("collision.ray_cast.world");
pub const COLLISION_SPHERE_OVERLAP_WORLD: CollisionContractId =
    CollisionContractId::new("collision.sphere_overlap.world");
pub const COLLISION_SPHERE_SWEEP_TRANSITION: CollisionContractId =
    CollisionContractId::new("collision.sphere_sweep.transition");
pub const COLLISION_TIME_OF_IMPACT_TRANSITION: CollisionContractId =
    CollisionContractId::new("collision.time_of_impact.transition");

const POINT_WITNESS_FIELDS: &[CollisionWitnessField] = &[
    CollisionWitnessField {
        name: "sample_point",
        ty: "vec3<f32>",
    },
    CollisionWitnessField {
        name: "nearest_point_on_world",
        ty: "vec3<f32>",
    },
    CollisionWitnessField {
        name: "world_normal",
        ty: "vec3<f32>",
    },
    CollisionWitnessField {
        name: "signed_distance",
        ty: "f32",
    },
    CollisionWitnessField {
        name: "normal_provenance",
        ty: "CollisionContactNormalProvenance",
    },
];

const RAY_WITNESS_FIELDS: &[CollisionWitnessField] = &[
    CollisionWitnessField {
        name: "travel_distance",
        ty: "f32",
    },
    CollisionWitnessField {
        name: "position",
        ty: "vec3<f32>",
    },
    CollisionWitnessField {
        name: "normal",
        ty: "vec3<f32>",
    },
    CollisionWitnessField {
        name: "root_shape_id",
        ty: "u32",
    },
    CollisionWitnessField {
        name: "feature_id",
        ty: "u32",
    },
    CollisionWitnessField {
        name: "normal_provenance",
        ty: "CollisionContactNormalProvenance",
    },
];

const SPHERE_WITNESS_FIELDS: &[CollisionWitnessField] = &[
    CollisionWitnessField {
        name: "point_on_probe",
        ty: "vec3<f32>",
    },
    CollisionWitnessField {
        name: "point_on_world",
        ty: "vec3<f32>",
    },
    CollisionWitnessField {
        name: "world_normal",
        ty: "vec3<f32>",
    },
    CollisionWitnessField {
        name: "signed_separation",
        ty: "f32",
    },
    CollisionWitnessField {
        name: "normal_provenance",
        ty: "CollisionContactNormalProvenance",
    },
];

const SWEEP_WITNESS_FIELDS: &[CollisionWitnessField] = &[
    CollisionWitnessField {
        name: "contact_fraction_upper_bound",
        ty: "f32",
    },
    CollisionWitnessField {
        name: "point_on_probe",
        ty: "vec3<f32>",
    },
    CollisionWitnessField {
        name: "point_on_world",
        ty: "vec3<f32>",
    },
    CollisionWitnessField {
        name: "contact_normal",
        ty: "vec3<f32>",
    },
    CollisionWitnessField {
        name: "normal_flavor",
        ty: "CollisionContactNormalFlavor",
    },
    CollisionWitnessField {
        name: "normal_provenance",
        ty: "CollisionContactNormalProvenance",
    },
];

const TIME_OF_IMPACT_WITNESS_FIELDS: &[CollisionWitnessField] = &[
    CollisionWitnessField {
        name: "time_fraction_upper_bound",
        ty: "f32",
    },
    CollisionWitnessField {
        name: "point_on_probe",
        ty: "vec3<f32>",
    },
    CollisionWitnessField {
        name: "point_on_world",
        ty: "vec3<f32>",
    },
    CollisionWitnessField {
        name: "contact_normal",
        ty: "vec3<f32>",
    },
    CollisionWitnessField {
        name: "normal_flavor",
        ty: "CollisionContactNormalFlavor",
    },
    CollisionWitnessField {
        name: "normal_provenance",
        ty: "CollisionContactNormalProvenance",
    },
];

pub const POINT_WITNESS_SCHEMA: CollisionWitnessSchema = CollisionWitnessSchema {
    name: "CollisionPointWitness",
    kind: CollisionWitnessKind::PointContainment,
    fields: POINT_WITNESS_FIELDS,
};

pub const RAY_WITNESS_SCHEMA: CollisionWitnessSchema = CollisionWitnessSchema {
    name: "CollisionRayWitness",
    kind: CollisionWitnessKind::RayHit,
    fields: RAY_WITNESS_FIELDS,
};

pub const SPHERE_WITNESS_SCHEMA: CollisionWitnessSchema = CollisionWitnessSchema {
    name: "CollisionSphereWitness",
    kind: CollisionWitnessKind::SphereWorldPair,
    fields: SPHERE_WITNESS_FIELDS,
};

pub const SWEEP_WITNESS_SCHEMA: CollisionWitnessSchema = CollisionWitnessSchema {
    name: "CollisionSweepWitness",
    kind: CollisionWitnessKind::SweepContact,
    fields: SWEEP_WITNESS_FIELDS,
};

pub const TIME_OF_IMPACT_WITNESS_SCHEMA: CollisionWitnessSchema = CollisionWitnessSchema {
    name: "CollisionTimeOfImpactWitness",
    kind: CollisionWitnessKind::TimeOfImpactContact,
    fields: TIME_OF_IMPACT_WITNESS_FIELDS,
};

const COLLISION_CONTRACTS: [CollisionContractDescriptor; 5] = [
    CollisionContractDescriptor {
        id: COLLISION_POINT_OCCUPANCY_WORLD,
        version: COLLISION_CONTRACT_VERSION,
        family: CollisionFamilyId::Occupancy,
        question: CollisionQuestionId::PointOccupancy,
        target: CollisionTargetKind::WorldSnapshot,
        authority: CollisionAuthorityRequirement::snapshot(EvidenceScope::SnapshotLocal),
        input_kind: CollisionInputKind::Point,
        input_record: "CollisionPointInput",
        output_kind: CollisionOutputKind::Occupancy,
        output_record: "CollisionOccupancyResult",
        witness_schema: &POINT_WITNESS_SCHEMA,
        policy: CollisionExecutionPolicy::exact_oracle(DispatchBackend::Cpu),
        supported_backends: CollisionBackendSupport::cpu_and_wgsl(),
    },
    CollisionContractDescriptor {
        id: COLLISION_RAY_CAST_WORLD,
        version: COLLISION_CONTRACT_VERSION,
        family: CollisionFamilyId::RayCast,
        question: CollisionQuestionId::RayCastFirstHit,
        target: CollisionTargetKind::WorldSnapshot,
        authority: CollisionAuthorityRequirement::snapshot(EvidenceScope::SnapshotLocal),
        input_kind: CollisionInputKind::Ray,
        input_record: "CollisionRayInput",
        output_kind: CollisionOutputKind::RayCast,
        output_record: "CollisionRayCastResult",
        witness_schema: &RAY_WITNESS_SCHEMA,
        policy: CollisionExecutionPolicy::exact_oracle(DispatchBackend::Cpu),
        supported_backends: CollisionBackendSupport::cpu_and_wgsl(),
    },
    CollisionContractDescriptor {
        id: COLLISION_SPHERE_OVERLAP_WORLD,
        version: COLLISION_CONTRACT_VERSION,
        family: CollisionFamilyId::Overlap,
        question: CollisionQuestionId::SphereOverlap,
        target: CollisionTargetKind::WorldSnapshot,
        authority: CollisionAuthorityRequirement::snapshot(EvidenceScope::SnapshotLocal),
        input_kind: CollisionInputKind::SphereProbe,
        input_record: "CollisionSphereProbe",
        output_kind: CollisionOutputKind::SphereOverlap,
        output_record: "CollisionSphereOverlapResult",
        witness_schema: &SPHERE_WITNESS_SCHEMA,
        policy: CollisionExecutionPolicy::exact_oracle(DispatchBackend::Cpu),
        supported_backends: CollisionBackendSupport::cpu_and_wgsl(),
    },
    CollisionContractDescriptor {
        id: COLLISION_SPHERE_SWEEP_TRANSITION,
        version: COLLISION_CONTRACT_VERSION,
        family: CollisionFamilyId::Sweep,
        question: CollisionQuestionId::SphereSweepFirstContact,
        target: CollisionTargetKind::WorldTransition,
        authority: CollisionAuthorityRequirement::transition(
            TRANSITION_COMPATIBILITY,
            EvidenceScope::TransitionCompatible,
        ),
        input_kind: CollisionInputKind::SphereSweep,
        input_record: "CollisionSphereSweepInput",
        output_kind: CollisionOutputKind::SweepContact,
        output_record: "CollisionSweepResult",
        witness_schema: &SWEEP_WITNESS_SCHEMA,
        policy: CollisionExecutionPolicy::conservative(DispatchBackend::Cpu),
        supported_backends: CollisionBackendSupport::cpu_only(),
    },
    CollisionContractDescriptor {
        id: COLLISION_TIME_OF_IMPACT_TRANSITION,
        version: COLLISION_CONTRACT_VERSION,
        family: CollisionFamilyId::TimeOfImpact,
        question: CollisionQuestionId::SphereTimeOfImpact,
        target: CollisionTargetKind::WorldTransition,
        authority: CollisionAuthorityRequirement::transition(
            TRANSITION_COMPATIBILITY,
            EvidenceScope::TransitionCompatible,
        ),
        input_kind: CollisionInputKind::SphereSweep,
        input_record: "CollisionSphereSweepInput",
        output_kind: CollisionOutputKind::TimeOfImpact,
        output_record: "CollisionTimeOfImpactResult",
        witness_schema: &TIME_OF_IMPACT_WITNESS_SCHEMA,
        policy: CollisionExecutionPolicy::interval_bounded(DispatchBackend::Cpu),
        supported_backends: CollisionBackendSupport::cpu_only(),
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CollisionSnapshotTransitionInput {
    pub current_snapshot_epoch: u32,
    pub previous_snapshot_epoch: u32,
    pub change_class: ChangeClass,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CollisionPointInput {
    pub point: [f32; 3],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CollisionRayInput {
    pub origin: [f32; 3],
    pub direction: [f32; 3],
    pub max_distance: f32,
    pub min_step: f32,
    pub hit_epsilon: f32,
    pub max_steps: i32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CollisionSphereProbe {
    pub center: [f32; 3],
    pub radius: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CollisionSphereSweepInput {
    pub start_center: [f32; 3],
    pub end_center: [f32; 3],
    pub radius: f32,
    pub contact_tolerance: f32,
    pub max_iterations: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollisionOccupancyClass {
    Empty,
    Boundary,
    Occupied,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollisionRayMissReason {
    None,
    NoHitWithinRange,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CollisionPointWitness {
    pub sample_point: [f32; 3],
    pub nearest_point_on_world: [f32; 3],
    pub world_normal: [f32; 3],
    pub signed_distance: f32,
    pub normal_provenance: CollisionContactNormalProvenance,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CollisionRayWitness {
    pub travel_distance: f32,
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub root_shape_id: u32,
    pub feature_id: u32,
    pub normal_provenance: CollisionContactNormalProvenance,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CollisionSphereWitness {
    pub point_on_probe: [f32; 3],
    pub point_on_world: [f32; 3],
    pub world_normal: [f32; 3],
    pub signed_separation: f32,
    pub normal_provenance: CollisionContactNormalProvenance,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CollisionSweepWitness {
    pub contact_fraction_upper_bound: f32,
    pub point_on_probe: [f32; 3],
    pub point_on_world: [f32; 3],
    pub contact_normal: [f32; 3],
    pub normal_flavor: CollisionContactNormalFlavor,
    pub normal_provenance: CollisionContactNormalProvenance,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CollisionTimeOfImpactWitness {
    pub time_fraction_upper_bound: f32,
    pub point_on_probe: [f32; 3],
    pub point_on_world: [f32; 3],
    pub contact_normal: [f32; 3],
    pub normal_flavor: CollisionContactNormalFlavor,
    pub normal_provenance: CollisionContactNormalProvenance,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CollisionNoHitCertificate {
    pub valid_through_fraction: f32,
    pub guarantee: RequiredGuaranteeClass,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CollisionOccupancyResult {
    pub classification: CollisionOccupancyClass,
    pub occupied: bool,
    pub signed_distance: f32,
    pub witness: CollisionPointWitness,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CollisionRayCastResult {
    pub hit: bool,
    pub miss_reason: CollisionRayMissReason,
    pub witness: Option<CollisionRayWitness>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CollisionSphereOverlapResult {
    pub overlaps: bool,
    pub signed_separation: f32,
    pub witness: CollisionSphereWitness,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CollisionSweepResult {
    pub hit: bool,
    pub witness: Option<CollisionSweepWitness>,
    pub no_hit_certificate: Option<CollisionNoHitCertificate>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CollisionTimeOfImpactResult {
    pub hit: bool,
    pub time_fraction_upper_bound: Option<f32>,
    pub witness: Option<CollisionTimeOfImpactWitness>,
    pub no_hit_certificate: Option<CollisionNoHitCertificate>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CollisionResult {
    Occupancy(CollisionOccupancyResult),
    RayCast(CollisionRayCastResult),
    SphereOverlap(CollisionSphereOverlapResult),
    Sweep(CollisionSweepResult),
    TimeOfImpact(CollisionTimeOfImpactResult),
}

pub fn collision_contracts() -> &'static [CollisionContractDescriptor] {
    &COLLISION_CONTRACTS
}

pub fn collision_contract(id: CollisionContractId) -> Option<&'static CollisionContractDescriptor> {
    COLLISION_CONTRACTS
        .iter()
        .find(|descriptor| descriptor.id == id)
}

pub fn collision_family_name(value: CollisionFamilyId) -> &'static str {
    match value {
        CollisionFamilyId::Occupancy => "occupancy",
        CollisionFamilyId::RayCast => "ray_cast",
        CollisionFamilyId::Overlap => "overlap",
        CollisionFamilyId::Sweep => "sweep",
        CollisionFamilyId::TimeOfImpact => "time_of_impact",
    }
}

pub fn collision_question_name(value: CollisionQuestionId) -> &'static str {
    match value {
        CollisionQuestionId::PointOccupancy => "point_occupancy",
        CollisionQuestionId::RayCastFirstHit => "ray_cast_first_hit",
        CollisionQuestionId::SphereOverlap => "sphere_overlap",
        CollisionQuestionId::SphereSweepFirstContact => "sphere_sweep_first_contact",
        CollisionQuestionId::SphereTimeOfImpact => "sphere_time_of_impact",
    }
}

pub fn collision_target_name(value: CollisionTargetKind) -> &'static str {
    match value {
        CollisionTargetKind::WorldSnapshot => "world_snapshot",
        CollisionTargetKind::WorldTransition => "world_transition",
    }
}

pub fn collision_input_kind_name(value: CollisionInputKind) -> &'static str {
    match value {
        CollisionInputKind::WorldCapture => "world_capture",
        CollisionInputKind::SceneDomain => "scene_domain",
        CollisionInputKind::SnapshotTransition => "snapshot_transition",
        CollisionInputKind::Point => "point",
        CollisionInputKind::Ray => "ray",
        CollisionInputKind::SphereProbe => "sphere_probe",
        CollisionInputKind::SphereSweep => "sphere_sweep",
    }
}

pub fn collision_output_kind_name(value: CollisionOutputKind) -> &'static str {
    match value {
        CollisionOutputKind::Occupancy => "occupancy",
        CollisionOutputKind::RayCast => "ray_cast",
        CollisionOutputKind::SphereOverlap => "sphere_overlap",
        CollisionOutputKind::SweepContact => "sweep_contact",
        CollisionOutputKind::TimeOfImpact => "time_of_impact",
    }
}

pub fn collision_witness_kind_name(value: CollisionWitnessKind) -> &'static str {
    match value {
        CollisionWitnessKind::PointContainment => "point_containment",
        CollisionWitnessKind::RayHit => "ray_hit",
        CollisionWitnessKind::SphereWorldPair => "sphere_world_pair",
        CollisionWitnessKind::SweepContact => "sweep_contact",
        CollisionWitnessKind::TimeOfImpactContact => "time_of_impact_contact",
    }
}

pub fn collision_authority_scope_name(value: CollisionAuthorityScope) -> &'static str {
    match value {
        CollisionAuthorityScope::Snapshot => "snapshot",
        CollisionAuthorityScope::Transition => "transition",
    }
}

pub fn collision_contact_normal_flavor_name(value: CollisionContactNormalFlavor) -> &'static str {
    match value {
        CollisionContactNormalFlavor::SurfaceGradient => "surface_gradient",
        CollisionContactNormalFlavor::ConservativeUpperBound => "conservative_upper_bound",
    }
}

pub fn collision_contact_normal_provenance_name(
    value: CollisionContactNormalProvenance,
) -> &'static str {
    match value {
        CollisionContactNormalProvenance::CertifiedFieldGradient => "certified_field_gradient",
        CollisionContactNormalProvenance::FeatureNormal => "feature_normal",
        CollisionContactNormalProvenance::HeuristicShadingNormal => "heuristic_shading_normal",
    }
}

pub fn collision_backend_support_names(value: CollisionBackendSupport) -> Vec<&'static str> {
    let mut names = Vec::new();
    if value.cpu {
        names.push("cpu");
    }
    if value.virtual_gpu {
        names.push("virtual_gpu");
    }
    if value.wgsl {
        names.push("wgsl");
    }
    names
}

pub fn collision_call_name(value: CollisionContractId) -> &'static str {
    match value {
        COLLISION_POINT_OCCUPANCY_WORLD => "collision_point_occupancy_world",
        COLLISION_RAY_CAST_WORLD => "collision_ray_cast_world",
        COLLISION_SPHERE_OVERLAP_WORLD => "collision_sphere_overlap_world",
        COLLISION_SPHERE_SWEEP_TRANSITION => "collision_sphere_sweep_transition",
        COLLISION_TIME_OF_IMPACT_TRANSITION => "collision_time_of_impact_transition",
        _ => "collision_unknown",
    }
}
