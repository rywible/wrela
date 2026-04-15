use crate::acceleration::{AccelerationForest, AccelerationNode, BoundDescriptorKind};
use crate::artifact_key::ArtifactReuseKey;
use crate::artifact_store::{
    ArtifactInstanceMetadata, ArtifactLookupRequest, ArtifactStore, StoredArtifact,
};
use crate::collision_contract::{
    CollisionContactNormalFlavor, CollisionContactNormalProvenance, CollisionNoHitCertificate,
    CollisionOccupancyClass, CollisionOccupancyResult, CollisionPointInput, CollisionPointWitness,
    CollisionRayCastResult, CollisionRayInput, CollisionRayMissReason, CollisionRayWitness,
    CollisionResult, CollisionSnapshotTransitionInput, CollisionSphereOverlapResult,
    CollisionSphereProbe, CollisionSphereSweepInput, CollisionSphereWitness, CollisionSweepResult,
    CollisionSweepWitness, CollisionTargetKind, CollisionTimeOfImpactResult,
    CollisionTimeOfImpactWitness,
};
use crate::collision_plan::{
    CollisionArtifactBinding, CollisionExecError, CollisionExecutionTrace, CollisionPass,
    CollisionPassKind, CollisionPlan, CollisionReuseDecision, CollisionReuseMetrics,
    CollisionReuseReason, CollisionReuseVerdict, collision_artifact_kind_name,
    collision_reuse_reason_name, collision_reuse_verdict_name,
};
use crate::execution_policy::QueryExecutionPolicy;
use crate::kernel::{
    KernelCaptureQueryPlan, KernelStructValue, KernelValue, lower_capture_query_plan,
    lower_world_query_plan,
};
use crate::query_contract::{self, DispatchBackend, QueryContractId};
use crate::query_exec::cpu::DirectQueryOps;
use crate::query_exec::{
    DirectQueryExecutionTrace, QueryExecContext, execute_capture_query_with_snapshot_on,
    execute_world_query_with_policy_with_snapshot_on, execute_world_query_with_snapshot_on,
};
use crate::query_plan::{CaptureQueryPlan, WorldQueryKind, WorldQueryPlan};
use crate::query_solver::{
    CertificateReuseClass, RayStepCertificate, RayStepCertificateMetadata,
    RayStepCertificateSubjectKind, StepCertificateKind,
};
use crate::world_identity::WorldSnapshotHandle;
use smol_str::SmolStr;
use std::collections::{BTreeMap, BTreeSet, HashMap};

#[derive(Debug, Clone, PartialEq)]
pub enum CollisionArtifactPayload {
    SupportSummary(CollisionSupportSummary),
    BroadphaseCandidates(CollisionBroadphaseCandidates),
    WitnessCache(CollisionStoredWitness),
    ContinuationSeed(CollisionContinuationSeed),
}

#[derive(Debug, Clone, PartialEq)]
pub struct CollisionSupportSummary {
    pub support_class: u32,
    pub semantics: u32,
    pub has_bounds: bool,
    pub opaque_boundary: bool,
    pub can_coarse_support_prune: bool,
    pub min: [f32; 3],
    pub max: [f32; 3],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollisionBroadphaseCandidates {
    pub candidate_shape_names: Vec<SmolStr>,
    pub rejected_candidate_count: u32,
    pub pruned_node_count: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CollisionStoredWitness {
    pub hit: bool,
    pub contact_fraction_upper_bound: Option<f32>,
    pub separation_upper_bound: Option<f32>,
    pub normal_provenance: Option<CollisionContactNormalProvenance>,
    pub normal_flavor: CollisionContactNormalFlavor,
    pub certificate: RayStepCertificate,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CollisionContinuationSeed {
    pub fraction_hint: f32,
    pub no_hit_certificate: bool,
    pub separation_upper_bound: Option<f32>,
    pub normal_provenance: Option<CollisionContactNormalProvenance>,
    pub normal_flavor: CollisionContactNormalFlavor,
    pub certificate: RayStepCertificate,
}

pub type CollisionArtifactStore = ArtifactStore<CollisionArtifactPayload>;

#[derive(Debug, Clone, PartialEq)]
enum CollisionMaterializedValue {
    Occupancy(CollisionOccupancyResult),
    RayCast(CollisionRayCastResult),
    SphereOverlap(CollisionSphereOverlapResult),
    Sweep(CollisionSweepResult),
    TimeOfImpact(CollisionTimeOfImpactResult),
}

#[derive(Debug, Clone, PartialEq)]
struct SweepOutcome {
    hit: bool,
    fraction_upper_bound: Option<f32>,
    separation_upper_bound: Option<f32>,
    point_on_probe: Option<[f32; 3]>,
    point_on_world: Option<[f32; 3]>,
    contact_normal: Option<[f32; 3]>,
    contact_normal_provenance: Option<CollisionContactNormalProvenance>,
    normal_flavor: CollisionContactNormalFlavor,
    no_hit_certificate: Option<CollisionNoHitCertificate>,
    certificate: RayStepCertificate,
    interval_subdivisions: u32,
    interval_refinements: u32,
    certificate_successes: u32,
    fallback_count: u32,
}

#[derive(Debug, Clone)]
struct CollisionTransitionReuseSeed {
    seed_fraction: Option<f32>,
    normal_flavor: CollisionContactNormalFlavor,
    normal_provenance: Option<CollisionContactNormalProvenance>,
    certificate: Option<RayStepCertificate>,
}

pub fn execute(
    plan: &CollisionPlan,
    ctx: &QueryExecContext,
    args: &[KernelValue],
) -> Result<(CollisionResult, CollisionExecutionTrace), CollisionExecError> {
    let mut store = CollisionArtifactStore::default();
    execute_with_store(plan, ctx, args, &mut store)
}

pub fn execute_with_store(
    plan: &CollisionPlan,
    ctx: &QueryExecContext,
    args: &[KernelValue],
    store: &mut CollisionArtifactStore,
) -> Result<(CollisionResult, CollisionExecutionTrace), CollisionExecError> {
    let validation = plan.validate();
    if !validation.is_empty() {
        return Err(CollisionExecError::Validation {
            messages: validation.into_iter().map(|error| error.message).collect(),
        });
    }

    let backend = resolve_backend(plan.backend)?;
    let world_index = input_binding_index(
        plan,
        crate::collision_contract::CollisionInputKind::WorldCapture,
    )?;
    let domain_index = input_binding_index(
        plan,
        crate::collision_contract::CollisionInputKind::SceneDomain,
    )?;
    let (capture, capture_name, snapshot) = resolve_region_capture(ctx, args.get(world_index))?;
    let domain = args
        .get(domain_index)
        .cloned()
        .ok_or_else(|| type_mismatch("SceneDomain", "missing"))?;
    let transition = if matches!(plan.target, CollisionTargetKind::WorldTransition) {
        let transition_index = input_binding_index(
            plan,
            crate::collision_contract::CollisionInputKind::SnapshotTransition,
        )?;
        let transition = collision_transition_input(
            args.get(transition_index)
                .ok_or(CollisionExecError::MissingTransitionInput)?,
        )?;
        if transition.current_snapshot_epoch != snapshot.epoch().0 as u32 {
            return Err(CollisionExecError::TransitionEpochMismatch {
                expected: transition.current_snapshot_epoch,
                found: snapshot.epoch().0 as u32,
            });
        }
        Some(transition)
    } else {
        None
    };
    let descriptor = crate::collision_contract::collision_contract(plan.contract_id)
        .expect("validated collision plan must reference a known contract");
    if let Some(transition) = transition {
        if let Some(compatibility) = descriptor.authority.transition_compatibility {
            if !compatibility.allows(transition.change_class) {
                return Err(CollisionExecError::TransitionAuthorityExceeded {
                    observed: transition.change_class,
                    maximum: compatibility.maximum,
                });
            }
        }
    }
    let collision_index = input_binding_index(plan, descriptor.input_kind)?;
    let collision_input = args
        .get(collision_index)
        .ok_or_else(|| type_mismatch(descriptor.input_record, "missing"))?;
    let policy = QueryExecutionPolicy::new(
        backend,
        plan.policy.required_guarantee,
        plan.policy.selected_method,
        None,
    );

    let mut executed_query_contracts = Vec::new();
    let mut reuse_metrics = CollisionReuseMetrics::default();
    let mut reuse_decisions = Vec::new();
    let mut broadphase_candidate_count = 0u32;
    let mut broadphase_rejected_candidate_count = 0u32;
    let mut broadphase_pruned_node_count = 0u32;
    let mut interval_bracket = None;
    let mut interval_subdivisions = 0u32;
    let mut interval_refinements = 0u32;
    let mut certificate_successes = 0u32;
    let mut fallback_count = 0u32;
    let mut contact_normal_provenance = None;
    let mut values = BTreeMap::<SmolStr, CollisionMaterializedValue>::new();

    for pass in &plan.passes {
        match &pass.kind {
            CollisionPassKind::GatherCandidates {
                support_summary_contract,
                support_artifact,
            } => {
                let artifact = artifact_binding_by_id(plan, support_artifact)?;
                let (support_summary, trace) = execute_world_query_contract(
                    ctx,
                    backend,
                    None,
                    &snapshot,
                    *support_summary_contract,
                    &[capture.clone(), domain.clone()],
                )?;
                executed_query_contracts.push(trace.contract_id);
                insert_artifact(
                    store,
                    artifact,
                    &snapshot,
                    collision_policy_digest(plan.policy),
                    collision_domain_compatibility_hash(&domain)?,
                    None,
                    CollisionArtifactPayload::SupportSummary(parse_collision_support_summary(
                        &support_summary,
                    )?),
                );
            }
            CollisionPassKind::BuildBroadphaseCandidates {
                support_artifact,
                artifact_id,
            } => {
                let artifact = artifact_binding_by_id(plan, artifact_id)?;
                let support_payload = current_artifact_payload(
                    store,
                    artifact_binding_by_id(plan, support_artifact)?,
                    &snapshot,
                    collision_policy_digest(plan.policy),
                    Some(artifact_reuse_key(
                        artifact_binding_by_id(plan, support_artifact)?,
                        &snapshot,
                        collision_policy_digest(plan.policy),
                        collision_domain_compatibility_hash(&domain)?,
                    )),
                )?;
                let support_summary = match support_payload {
                    CollisionArtifactPayload::SupportSummary(payload) => payload.clone(),
                    other => {
                        return Err(CollisionExecError::TypeMismatch {
                            expected: "SupportSummary".to_string(),
                            found: format!("{other:?}"),
                        });
                    }
                };
                let broadphase = build_broadphase_candidates(
                    ctx,
                    &capture_name,
                    &domain,
                    collision_input,
                    &support_summary,
                )?;
                insert_artifact(
                    store,
                    artifact,
                    &snapshot,
                    collision_policy_digest(plan.policy),
                    collision_broadphase_compatibility_hash(
                        &capture_name,
                        &domain,
                        collision_input,
                    )?,
                    None,
                    CollisionArtifactPayload::BroadphaseCandidates(broadphase),
                );
            }
            CollisionPassKind::EvaluatePointOccupancy {
                distance_contract,
                normal_contract,
                support_artifact,
                broadphase_artifact,
            } => {
                ensure_current_artifact_available(
                    store,
                    artifact_binding_by_id(plan, support_artifact)?,
                    &snapshot,
                    collision_policy_digest(plan.policy),
                    Some(artifact_reuse_key(
                        artifact_binding_by_id(plan, support_artifact)?,
                        &snapshot,
                        collision_policy_digest(plan.policy),
                        collision_domain_compatibility_hash(&domain)?,
                    )),
                )?;
                ensure_current_artifact_available(
                    store,
                    artifact_binding_by_id(plan, broadphase_artifact)?,
                    &snapshot,
                    collision_policy_digest(plan.policy),
                    Some(artifact_reuse_key(
                        artifact_binding_by_id(plan, broadphase_artifact)?,
                        &snapshot,
                        collision_policy_digest(plan.policy),
                        collision_broadphase_compatibility_hash(
                            &capture_name,
                            &domain,
                            collision_input,
                        )?,
                    )),
                )?;
                let broadphase = load_broadphase_candidates(
                    ctx,
                    plan,
                    store,
                    &snapshot,
                    &capture_name,
                    &domain,
                    collision_input,
                    support_artifact,
                    broadphase_artifact,
                )?;
                broadphase_candidate_count = broadphase.candidate_shape_names.len() as u32;
                broadphase_rejected_candidate_count = broadphase.rejected_candidate_count;
                broadphase_pruned_node_count = broadphase.pruned_node_count;
                let point = collision_point_input(collision_input)?;
                let distance_capture_plan =
                    lower_shape_capture_query_plan(query_contract::SPATIAL_DISTANCE_CAPTURE_SHAPE)?;
                let normal_capture_plan =
                    lower_shape_capture_query_plan(query_contract::SPATIAL_NORMAL_CAPTURE_SHAPE)?;
                let (distance, normal, provenance, _) = candidate_limited_point_query(
                    ctx,
                    backend,
                    &policy,
                    &snapshot,
                    &capture,
                    &domain,
                    broadphase.candidate_shape_names.as_slice(),
                    point.point,
                    &distance_capture_plan,
                    &normal_capture_plan,
                    *distance_contract,
                    *normal_contract,
                    &mut executed_query_contracts,
                )?;
                contact_normal_provenance = provenance;
                let signed_distance = expect_f32(&distance)?;
                let world_normal = expect_vec3(&normal)?;
                materialize_value(
                    pass,
                    CollisionMaterializedValue::Occupancy(CollisionOccupancyResult {
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
                            normal_provenance: provenance.unwrap_or(
                                CollisionContactNormalProvenance::HeuristicShadingNormal,
                            ),
                        },
                    }),
                    &mut values,
                )?;
            }
            CollisionPassKind::CastRayFirstHit {
                trace_contract: _trace_contract,
                support_artifact,
                broadphase_artifact,
            } => {
                ensure_current_artifact_available(
                    store,
                    artifact_binding_by_id(plan, support_artifact)?,
                    &snapshot,
                    collision_policy_digest(plan.policy),
                    Some(artifact_reuse_key(
                        artifact_binding_by_id(plan, support_artifact)?,
                        &snapshot,
                        collision_policy_digest(plan.policy),
                        collision_domain_compatibility_hash(&domain)?,
                    )),
                )?;
                ensure_current_artifact_available(
                    store,
                    artifact_binding_by_id(plan, broadphase_artifact)?,
                    &snapshot,
                    collision_policy_digest(plan.policy),
                    Some(artifact_reuse_key(
                        artifact_binding_by_id(plan, broadphase_artifact)?,
                        &snapshot,
                        collision_policy_digest(plan.policy),
                        collision_broadphase_compatibility_hash(
                            &capture_name,
                            &domain,
                            collision_input,
                        )?,
                    )),
                )?;
                let broadphase = load_broadphase_candidates(
                    ctx,
                    plan,
                    store,
                    &snapshot,
                    &capture_name,
                    &domain,
                    collision_input,
                    support_artifact,
                    broadphase_artifact,
                )?;
                broadphase_candidate_count = broadphase.candidate_shape_names.len() as u32;
                broadphase_rejected_candidate_count = broadphase.rejected_candidate_count;
                broadphase_pruned_node_count = broadphase.pruned_node_count;
                let value = if broadphase.candidate_shape_names.is_empty() {
                    CollisionRayCastResult {
                        hit: false,
                        miss_reason: CollisionRayMissReason::NoHitWithinRange,
                        witness: None,
                    }
                } else {
                    let trace_capture_plan = lower_shape_capture_query_plan(
                        query_contract::SPATIAL_TRACE_CAPTURE_SHAPE,
                    )?;
                    let ray = collision_ray_input(collision_input)?;
                    let (hit, trace) = candidate_limited_ray_query(
                        ctx,
                        backend,
                        &snapshot,
                        broadphase.candidate_shape_names.as_slice(),
                        &trace_capture_plan,
                        ray,
                        &mut executed_query_contracts,
                    )?;
                    let hit_ref = expect_struct(&hit, "Hit3")?;
                    let hit_flag = expect_bool(field(hit_ref, "hit")?)?;
                    if hit_flag {
                        let provenance = collision_contact_normal_provenance_from_trace(&trace);
                        contact_normal_provenance = provenance;
                        CollisionRayCastResult {
                            hit: true,
                            miss_reason: CollisionRayMissReason::None,
                            witness: Some(CollisionRayWitness {
                                travel_distance: expect_f32(field(hit_ref, "distance")?)?,
                                position: expect_vec3(field(hit_ref, "position")?)?,
                                normal: expect_vec3(field(hit_ref, "normal")?)?,
                                root_shape_id: expect_u32(field(hit_ref, "root_shape_id")?)?,
                                feature_id: expect_u32(field(hit_ref, "feature_id")?)?,
                                normal_provenance: provenance.unwrap_or(
                                    CollisionContactNormalProvenance::HeuristicShadingNormal,
                                ),
                            }),
                        }
                    } else {
                        CollisionRayCastResult {
                            hit: false,
                            miss_reason: CollisionRayMissReason::NoHitWithinRange,
                            witness: None,
                        }
                    }
                };
                materialize_value(
                    pass,
                    CollisionMaterializedValue::RayCast(value),
                    &mut values,
                )?;
            }
            CollisionPassKind::ResolveSphereOverlap {
                distance_contract,
                normal_contract,
                support_artifact,
                broadphase_artifact,
                ..
            } => {
                ensure_current_artifact_available(
                    store,
                    artifact_binding_by_id(plan, support_artifact)?,
                    &snapshot,
                    collision_policy_digest(plan.policy),
                    Some(artifact_reuse_key(
                        artifact_binding_by_id(plan, support_artifact)?,
                        &snapshot,
                        collision_policy_digest(plan.policy),
                        collision_domain_compatibility_hash(&domain)?,
                    )),
                )?;
                ensure_current_artifact_available(
                    store,
                    artifact_binding_by_id(plan, broadphase_artifact)?,
                    &snapshot,
                    collision_policy_digest(plan.policy),
                    Some(artifact_reuse_key(
                        artifact_binding_by_id(plan, broadphase_artifact)?,
                        &snapshot,
                        collision_policy_digest(plan.policy),
                        collision_broadphase_compatibility_hash(
                            &capture_name,
                            &domain,
                            collision_input,
                        )?,
                    )),
                )?;
                let broadphase = load_broadphase_candidates(
                    ctx,
                    plan,
                    store,
                    &snapshot,
                    &capture_name,
                    &domain,
                    collision_input,
                    support_artifact,
                    broadphase_artifact,
                )?;
                broadphase_candidate_count = broadphase.candidate_shape_names.len() as u32;
                broadphase_rejected_candidate_count = broadphase.rejected_candidate_count;
                broadphase_pruned_node_count = broadphase.pruned_node_count;
                let probe = collision_sphere_input(collision_input)?;
                let distance_capture_plan =
                    lower_shape_capture_query_plan(query_contract::SPATIAL_DISTANCE_CAPTURE_SHAPE)?;
                let normal_capture_plan =
                    lower_shape_capture_query_plan(query_contract::SPATIAL_NORMAL_CAPTURE_SHAPE)?;
                let (distance, normal, provenance, _) = candidate_limited_point_query(
                    ctx,
                    backend,
                    &policy,
                    &snapshot,
                    &capture,
                    &domain,
                    broadphase.candidate_shape_names.as_slice(),
                    probe.center,
                    &distance_capture_plan,
                    &normal_capture_plan,
                    *distance_contract,
                    *normal_contract,
                    &mut executed_query_contracts,
                )?;
                contact_normal_provenance = provenance;
                let center_distance = expect_f32(&distance)?;
                let world_normal = expect_vec3(&normal)?;
                let signed_separation = center_distance - probe.radius;
                materialize_value(
                    pass,
                    CollisionMaterializedValue::SphereOverlap(CollisionSphereOverlapResult {
                        overlaps: signed_separation <= 0.0,
                        signed_separation,
                        witness: CollisionSphereWitness {
                            point_on_probe: offset_point(probe.center, world_normal, -probe.radius),
                            point_on_world: offset_point(
                                probe.center,
                                world_normal,
                                -center_distance,
                            ),
                            world_normal,
                            signed_separation,
                            normal_provenance: provenance.unwrap_or(
                                CollisionContactNormalProvenance::HeuristicShadingNormal,
                            ),
                        },
                    }),
                    &mut values,
                )?;
            }
            CollisionPassKind::SweepSphereFirstContact {
                distance_contract,
                normal_contract,
                support_artifact,
                broadphase_artifact,
                witness_artifact,
                continuation_artifact,
            } => {
                let sweep = collision_sweep_input(collision_input)?;
                ensure_current_artifact_available(
                    store,
                    artifact_binding_by_id(plan, support_artifact)?,
                    &snapshot,
                    collision_policy_digest(plan.policy),
                    Some(artifact_reuse_key(
                        artifact_binding_by_id(plan, support_artifact)?,
                        &snapshot,
                        collision_policy_digest(plan.policy),
                        collision_domain_compatibility_hash(&domain)?,
                    )),
                )?;
                ensure_current_artifact_available(
                    store,
                    artifact_binding_by_id(plan, broadphase_artifact)?,
                    &snapshot,
                    collision_policy_digest(plan.policy),
                    Some(artifact_reuse_key(
                        artifact_binding_by_id(plan, broadphase_artifact)?,
                        &snapshot,
                        collision_policy_digest(plan.policy),
                        collision_broadphase_compatibility_hash(
                            &capture_name,
                            &domain,
                            collision_input,
                        )?,
                    )),
                )?;
                let seed = load_transition_reuse(
                    plan,
                    store,
                    &snapshot,
                    transition,
                    witness_artifact,
                    continuation_artifact,
                    &mut reuse_metrics,
                    &mut reuse_decisions,
                )?;
                let broadphase = load_broadphase_candidates(
                    ctx,
                    plan,
                    store,
                    &snapshot,
                    &capture_name,
                    &domain,
                    collision_input,
                    support_artifact,
                    broadphase_artifact,
                )?;
                broadphase_candidate_count = broadphase.candidate_shape_names.len() as u32;
                broadphase_rejected_candidate_count = broadphase.rejected_candidate_count;
                broadphase_pruned_node_count = broadphase.pruned_node_count;
                let distance_capture_plan =
                    lower_shape_capture_query_plan(query_contract::SPATIAL_DISTANCE_CAPTURE_SHAPE)?;
                let normal_capture_plan =
                    lower_shape_capture_query_plan(query_contract::SPATIAL_NORMAL_CAPTURE_SHAPE)?;
                let outcome = sweep_outcome(
                    ctx,
                    backend,
                    &policy,
                    &snapshot,
                    &capture,
                    &domain,
                    sweep,
                    *distance_contract,
                    *normal_contract,
                    &distance_capture_plan,
                    &normal_capture_plan,
                    seed,
                    broadphase.candidate_shape_names.as_slice(),
                    &mut executed_query_contracts,
                )?;
                interval_subdivisions = interval_subdivisions.max(outcome.interval_subdivisions);
                interval_refinements = interval_refinements.max(outcome.interval_refinements);
                certificate_successes = certificate_successes.max(outcome.certificate_successes);
                interval_bracket = outcome.certificate.bracket;
                fallback_count = fallback_count.max(outcome.fallback_count);
                contact_normal_provenance = outcome.contact_normal_provenance;
                store_transition_artifacts(
                    plan,
                    store,
                    &snapshot,
                    witness_artifact,
                    continuation_artifact,
                    outcome.clone(),
                )?;
                materialize_value(
                    pass,
                    CollisionMaterializedValue::Sweep(CollisionSweepResult {
                        hit: outcome.hit,
                        witness: outcome.fraction_upper_bound.map(|fraction| {
                            CollisionSweepWitness {
                                contact_fraction_upper_bound: fraction,
                                point_on_probe: outcome.point_on_probe.expect("hit witness"),
                                point_on_world: outcome.point_on_world.expect("hit witness"),
                                contact_normal: outcome.contact_normal.expect("hit witness"),
                                normal_flavor: outcome.normal_flavor,
                                normal_provenance: outcome.contact_normal_provenance.unwrap_or(
                                    CollisionContactNormalProvenance::HeuristicShadingNormal,
                                ),
                            }
                        }),
                        no_hit_certificate: outcome.no_hit_certificate,
                    }),
                    &mut values,
                )?;
            }
            CollisionPassKind::ResolveSphereTimeOfImpact {
                distance_contract,
                normal_contract,
                support_artifact,
                broadphase_artifact,
                witness_artifact,
                continuation_artifact,
            } => {
                let sweep = collision_sweep_input(collision_input)?;
                ensure_current_artifact_available(
                    store,
                    artifact_binding_by_id(plan, support_artifact)?,
                    &snapshot,
                    collision_policy_digest(plan.policy),
                    Some(artifact_reuse_key(
                        artifact_binding_by_id(plan, support_artifact)?,
                        &snapshot,
                        collision_policy_digest(plan.policy),
                        collision_domain_compatibility_hash(&domain)?,
                    )),
                )?;
                ensure_current_artifact_available(
                    store,
                    artifact_binding_by_id(plan, broadphase_artifact)?,
                    &snapshot,
                    collision_policy_digest(plan.policy),
                    Some(artifact_reuse_key(
                        artifact_binding_by_id(plan, broadphase_artifact)?,
                        &snapshot,
                        collision_policy_digest(plan.policy),
                        collision_broadphase_compatibility_hash(
                            &capture_name,
                            &domain,
                            collision_input,
                        )?,
                    )),
                )?;
                let seed = load_transition_reuse(
                    plan,
                    store,
                    &snapshot,
                    transition,
                    witness_artifact,
                    continuation_artifact,
                    &mut reuse_metrics,
                    &mut reuse_decisions,
                )?;
                let broadphase = load_broadphase_candidates(
                    ctx,
                    plan,
                    store,
                    &snapshot,
                    &capture_name,
                    &domain,
                    collision_input,
                    support_artifact,
                    broadphase_artifact,
                )?;
                broadphase_candidate_count = broadphase.candidate_shape_names.len() as u32;
                broadphase_rejected_candidate_count = broadphase.rejected_candidate_count;
                broadphase_pruned_node_count = broadphase.pruned_node_count;
                let distance_capture_plan =
                    lower_shape_capture_query_plan(query_contract::SPATIAL_DISTANCE_CAPTURE_SHAPE)?;
                let normal_capture_plan =
                    lower_shape_capture_query_plan(query_contract::SPATIAL_NORMAL_CAPTURE_SHAPE)?;
                let outcome = sweep_outcome(
                    ctx,
                    backend,
                    &policy,
                    &snapshot,
                    &capture,
                    &domain,
                    sweep,
                    *distance_contract,
                    *normal_contract,
                    &distance_capture_plan,
                    &normal_capture_plan,
                    seed,
                    broadphase.candidate_shape_names.as_slice(),
                    &mut executed_query_contracts,
                )?;
                interval_subdivisions = interval_subdivisions.max(outcome.interval_subdivisions);
                interval_refinements = interval_refinements.max(outcome.interval_refinements);
                certificate_successes = certificate_successes.max(outcome.certificate_successes);
                interval_bracket = outcome.certificate.bracket;
                fallback_count = fallback_count.max(outcome.fallback_count);
                contact_normal_provenance = outcome.contact_normal_provenance;
                store_transition_artifacts(
                    plan,
                    store,
                    &snapshot,
                    witness_artifact,
                    continuation_artifact,
                    outcome.clone(),
                )?;
                materialize_value(
                    pass,
                    CollisionMaterializedValue::TimeOfImpact(CollisionTimeOfImpactResult {
                        hit: outcome.hit,
                        time_fraction_upper_bound: outcome.fraction_upper_bound,
                        witness: outcome.fraction_upper_bound.map(|fraction| {
                            CollisionTimeOfImpactWitness {
                                time_fraction_upper_bound: fraction,
                                point_on_probe: outcome.point_on_probe.expect("hit witness"),
                                point_on_world: outcome.point_on_world.expect("hit witness"),
                                contact_normal: outcome.contact_normal.expect("hit witness"),
                                normal_flavor: outcome.normal_flavor,
                                normal_provenance: outcome.contact_normal_provenance.unwrap_or(
                                    CollisionContactNormalProvenance::HeuristicShadingNormal,
                                ),
                            }
                        }),
                        no_hit_certificate: outcome.no_hit_certificate,
                    }),
                    &mut values,
                )?;
            }
            CollisionPassKind::MaterializeOutput { output } => {
                let binding = output_binding_for_kind(plan, *output)?;
                let source =
                    pass.consumes
                        .first()
                        .ok_or_else(|| CollisionExecError::InvalidPass {
                            pass_id: pass.id.clone(),
                            message: "missing consumed intermediate value".to_string(),
                        })?;
                let value = values.get(source).cloned().ok_or_else(|| {
                    CollisionExecError::MissingPassValue {
                        pass_id: pass.id.clone(),
                        value_id: source.clone(),
                    }
                })?;
                if output_kind_for_value(&value) != *output {
                    return Err(CollisionExecError::InvalidPass {
                        pass_id: pass.id.clone(),
                        message: format!(
                            "output '{}' expects '{}'",
                            binding.name,
                            crate::collision_contract::collision_output_kind_name(*output)
                        ),
                    });
                }
                materialize_value(pass, value, &mut values)?;
            }
        }
    }

    let output = plan
        .outputs
        .first()
        .ok_or_else(|| CollisionExecError::MissingOutputBinding {
            output: "collision_output".to_string(),
        })?;
    let value =
        values
            .remove(&output.name)
            .ok_or_else(|| CollisionExecError::MissingPassValue {
                pass_id: SmolStr::new("materialize_output"),
                value_id: output.name.clone(),
            })?;
    let result = match value {
        CollisionMaterializedValue::Occupancy(value) => CollisionResult::Occupancy(value),
        CollisionMaterializedValue::RayCast(value) => CollisionResult::RayCast(value),
        CollisionMaterializedValue::SphereOverlap(value) => CollisionResult::SphereOverlap(value),
        CollisionMaterializedValue::Sweep(value) => CollisionResult::Sweep(value),
        CollisionMaterializedValue::TimeOfImpact(value) => CollisionResult::TimeOfImpact(value),
    };

    Ok((
        result,
        CollisionExecutionTrace {
            contract_id: plan.contract_id,
            family: plan.family,
            question: plan.question,
            backend,
            snapshot: Some(snapshot.report()),
            transition,
            required_guarantee: plan.policy.required_guarantee,
            selected_method: plan.policy.selected_method,
            executed_query_contracts,
            artifact_store: store.report(),
            broadphase_candidate_count,
            broadphase_rejected_candidate_count,
            broadphase_pruned_node_count,
            interval_bracket,
            interval_subdivisions,
            interval_refinements,
            certificate_successes,
            fallback_count,
            contact_normal_provenance,
            reuse_metrics,
            reuse_decisions,
        },
    ))
}

fn resolve_backend(backend: DispatchBackend) -> Result<DispatchBackend, CollisionExecError> {
    match backend {
        DispatchBackend::Cpu | DispatchBackend::Auto => Ok(DispatchBackend::Cpu),
        other => Err(CollisionExecError::UnsupportedBackend { backend: other }),
    }
}

fn input_binding_index(
    plan: &CollisionPlan,
    kind: crate::collision_contract::CollisionInputKind,
) -> Result<usize, CollisionExecError> {
    plan.inputs
        .iter()
        .position(|binding| binding.kind == kind)
        .ok_or_else(|| CollisionExecError::MissingInputBinding {
            kind: crate::collision_contract::collision_input_kind_name(kind).to_string(),
        })
}

fn output_binding_for_kind<'a>(
    plan: &'a CollisionPlan,
    kind: crate::collision_contract::CollisionOutputKind,
) -> Result<&'a crate::collision_plan::CollisionOutputBinding, CollisionExecError> {
    plan.outputs
        .iter()
        .find(|binding| binding.kind == kind)
        .ok_or_else(|| CollisionExecError::MissingOutputBinding {
            output: crate::collision_contract::collision_output_kind_name(kind).to_string(),
        })
}

fn artifact_binding_by_id<'a>(
    plan: &'a CollisionPlan,
    artifact_id: &SmolStr,
) -> Result<&'a CollisionArtifactBinding, CollisionExecError> {
    plan.artifacts
        .iter()
        .find(|artifact| artifact.id == *artifact_id)
        .ok_or_else(|| CollisionExecError::MissingArtifact {
            artifact_id: artifact_id.clone(),
        })
}

fn resolve_region_capture(
    ctx: &QueryExecContext,
    value: Option<&KernelValue>,
) -> Result<(KernelValue, SmolStr, WorldSnapshotHandle), CollisionExecError> {
    let value = value.ok_or(CollisionExecError::MissingRegionCapture)?;
    match value {
        KernelValue::Capture(name) => {
            let snapshot = ctx
                .region_snapshot_handle(name)
                .cloned()
                .ok_or(CollisionExecError::MissingSnapshotHandle)?;
            Ok((KernelValue::Capture(name.clone()), name.clone(), snapshot))
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
            Ok((
                KernelValue::Struct(struct_value.clone()),
                name.clone(),
                snapshot,
            ))
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

fn lower_shape_capture_query_plan(
    contract_id: QueryContractId,
) -> Result<KernelCaptureQueryPlan, CollisionExecError> {
    let plan = CaptureQueryPlan::for_contract(contract_id, None).map_err(|_| {
        CollisionExecError::UnknownQueryContract {
            contract_id: contract_id.as_str().to_string(),
        }
    })?;
    Ok(lower_capture_query_plan(&plan))
}

fn execute_shape_capture_query_plan(
    ctx: &QueryExecContext,
    backend: DispatchBackend,
    snapshot: &WorldSnapshotHandle,
    plan: &KernelCaptureQueryPlan,
    args: &[KernelValue],
) -> Result<(KernelValue, DirectQueryExecutionTrace), CollisionExecError> {
    execute_capture_query_with_snapshot_on(ctx, backend, Some(snapshot), plan, args).map_err(
        |error| CollisionExecError::ExecutionUnavailable {
            message: error.to_string(),
        },
    )
}

fn candidate_limited_point_query(
    ctx: &QueryExecContext,
    backend: DispatchBackend,
    policy: &QueryExecutionPolicy,
    snapshot: &WorldSnapshotHandle,
    capture: &KernelValue,
    domain: &KernelValue,
    candidates: &[SmolStr],
    point: [f32; 3],
    distance_capture_plan: &KernelCaptureQueryPlan,
    normal_capture_plan: &KernelCaptureQueryPlan,
    world_distance_contract: QueryContractId,
    world_normal_contract: QueryContractId,
    executed_query_contracts: &mut Vec<QueryContractId>,
) -> Result<
    (
        KernelValue,
        KernelValue,
        Option<CollisionContactNormalProvenance>,
        bool,
    ),
    CollisionExecError,
> {
    let mut best_candidate: Option<(f32, SmolStr)> = None;
    for candidate in candidates {
        let (distance, trace) = execute_shape_capture_query_plan(
            ctx,
            backend,
            snapshot,
            distance_capture_plan,
            &[
                KernelValue::Capture(candidate.clone()),
                KernelValue::Vec3(point),
            ],
        )?;
        executed_query_contracts.push(trace.contract_id);
        let distance = expect_f32(&distance)?;
        if best_candidate
            .as_ref()
            .map(|(best_distance, _)| distance < *best_distance)
            .unwrap_or(true)
        {
            best_candidate = Some((distance, candidate.clone()));
        }
    }

    if let Some((best_distance, candidate)) = best_candidate {
        let (normal, normal_trace) = execute_shape_capture_query_plan(
            ctx,
            backend,
            snapshot,
            normal_capture_plan,
            &[KernelValue::Capture(candidate), KernelValue::Vec3(point)],
        )?;
        executed_query_contracts.push(normal_trace.contract_id);
        let provenance = collision_contact_normal_provenance_from_trace(&normal_trace);
        return Ok((KernelValue::F32(best_distance), normal, provenance, true));
    }

    let (distance, distance_trace) = execute_point_query(
        ctx,
        backend,
        policy,
        snapshot,
        capture,
        domain,
        point,
        world_distance_contract,
    )?;
    executed_query_contracts.push(distance_trace.contract_id);
    let (normal, normal_trace) = execute_point_query(
        ctx,
        backend,
        policy,
        snapshot,
        capture,
        domain,
        point,
        world_normal_contract,
    )?;
    executed_query_contracts.push(normal_trace.contract_id);
    let provenance = collision_contact_normal_provenance_from_trace(&normal_trace);
    Ok((distance, normal, provenance, false))
}

fn candidate_limited_ray_query(
    ctx: &QueryExecContext,
    backend: DispatchBackend,
    snapshot: &WorldSnapshotHandle,
    candidates: &[SmolStr],
    trace_capture_plan: &KernelCaptureQueryPlan,
    ray: CollisionRayInput,
    executed_query_contracts: &mut Vec<QueryContractId>,
) -> Result<(KernelValue, DirectQueryExecutionTrace), CollisionExecError> {
    let mut best_hit: Option<(f32, KernelValue, DirectQueryExecutionTrace)> = None;
    let mut first_miss: Option<(KernelValue, DirectQueryExecutionTrace)> = None;
    for candidate in candidates {
        let (hit, trace) = execute_shape_capture_query_plan(
            ctx,
            backend,
            snapshot,
            trace_capture_plan,
            &[
                KernelValue::Capture(candidate.clone()),
                ray_query_value(&ray),
            ],
        )?;
        executed_query_contracts.push(trace.contract_id);
        let hit_ref = expect_struct(&hit, "Hit3")?;
        if expect_bool(field(hit_ref, "hit")?)? {
            let distance = expect_f32(field(hit_ref, "distance")?)?;
            let replace = best_hit
                .as_ref()
                .map(|(best_distance, _, _)| distance < *best_distance)
                .unwrap_or(true);
            if replace {
                best_hit = Some((distance, hit, trace));
            }
        } else if first_miss.is_none() {
            first_miss = Some((hit, trace));
        }
    }
    if let Some((_, hit, trace)) = best_hit {
        return Ok((hit, trace));
    }
    first_miss.ok_or_else(|| CollisionExecError::ExecutionUnavailable {
        message: "candidate-limited ray query requires at least one candidate".to_string(),
    })
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
        .map_err(|error| CollisionExecError::ExecutionUnavailable {
            message: error.to_string(),
        }),
        None => execute_world_query_with_snapshot_on(ctx, backend, Some(snapshot), &plan, args)
            .map_err(|error| CollisionExecError::ExecutionUnavailable {
                message: error.to_string(),
            }),
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

fn collision_policy_digest(policy: crate::collision_contract::CollisionExecutionPolicy) -> u64 {
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

fn artifact_reuse_key(
    artifact: &CollisionArtifactBinding,
    snapshot: &WorldSnapshotHandle,
    policy_digest: u64,
    compatibility_hash: u64,
) -> ArtifactReuseKey {
    ArtifactReuseKey::new(
        snapshot,
        Some(artifact.id.clone()),
        artifact.contract.logical_schema.describe(),
        compatibility_hash,
        Some(policy_digest),
        artifact.contract.compatibility.policy.mode,
    )
}

fn collision_broadphase_compatibility_hash(
    capture: &SmolStr,
    domain: &KernelValue,
    collision_input: &KernelValue,
) -> Result<u64, CollisionExecError> {
    let kind_tag = if collision_point_input(collision_input).is_ok() {
        "point"
    } else if collision_ray_input(collision_input).is_ok() {
        "ray"
    } else if collision_sphere_input(collision_input).is_ok() {
        "sphere"
    } else if collision_sweep_input(collision_input).is_ok() {
        "sweep"
    } else {
        "collision"
    };
    let capture_bytes = capture.as_str().as_bytes();
    let domain_hash = collision_domain_compatibility_hash(domain)?.to_le_bytes();
    let debug_input;
    let Some((min, max)) = collision_query_bounds(collision_input).ok().flatten() else {
        debug_input = format!("{collision_input:?}");
        return Ok(crate::query_exec::ids::stable_semantic_id(&[
            kind_tag.as_bytes(),
            capture_bytes,
            &domain_hash,
            debug_input.as_bytes(),
        ]));
    };
    let min0 = min[0].to_le_bytes();
    let min1 = min[1].to_le_bytes();
    let min2 = min[2].to_le_bytes();
    let max0 = max[0].to_le_bytes();
    let max1 = max[1].to_le_bytes();
    let max2 = max[2].to_le_bytes();
    Ok(crate::query_exec::ids::stable_semantic_id(&[
        kind_tag.as_bytes(),
        capture_bytes,
        &domain_hash,
        &min0,
        &min1,
        &min2,
        &max0,
        &max1,
        &max2,
    ]))
}

fn collision_domain_compatibility_hash(domain: &KernelValue) -> Result<u64, CollisionExecError> {
    let domain = expect_struct(domain, "SceneDomain")?;
    let scene_id = expect_u32(field(domain, "scene_id")?)?.to_le_bytes();
    let spatial = expect_struct(field(domain, "spatial")?, "SpatialDomainContract")?;
    let geometry_detail = expect_i32(field(spatial, "geometry_detail")?)?.to_le_bytes();
    let surface = expect_struct(field(domain, "surface")?, "SurfaceDomainContract")?;
    let material = [u8::from(expect_bool(field(surface, "material")?)?)];
    let participants = expect_struct(field(domain, "participants")?, "ParticipantDomainContract")?;
    let radiance = [u8::from(expect_bool(field(participants, "radiance")?)?)];
    let media = [u8::from(expect_bool(field(participants, "media")?)?)];
    Ok(crate::query_exec::ids::stable_semantic_id(&[
        b"collision-scene-domain",
        &scene_id,
        &geometry_detail,
        &material,
        &radiance,
        &media,
    ]))
}

fn insert_artifact(
    store: &mut CollisionArtifactStore,
    artifact: &CollisionArtifactBinding,
    snapshot: &WorldSnapshotHandle,
    policy_digest: u64,
    compatibility_hash: u64,
    history_compatibility_hash: Option<u64>,
    payload: CollisionArtifactPayload,
) {
    let reuse_key = artifact_reuse_key(artifact, snapshot, policy_digest, compatibility_hash);
    store.insert(StoredArtifact {
        contract: artifact.contract.clone(),
        metadata: ArtifactInstanceMetadata {
            snapshot: snapshot.clone(),
            reuse_key,
            policy_digest: Some(policy_digest),
            presentation_frame: None,
            layout_signature: None,
            history_compatibility_hash,
            evidence_summary: artifact.contract.evidence_summary.clone(),
        },
        payload,
    });
}

fn ensure_current_artifact_available(
    store: &CollisionArtifactStore,
    artifact: &CollisionArtifactBinding,
    snapshot: &WorldSnapshotHandle,
    policy_digest: u64,
    reuse_key: Option<ArtifactReuseKey>,
) -> Result<(), CollisionExecError> {
    let request = ArtifactLookupRequest {
        contract: artifact.contract.clone(),
        reuse_key,
        current_snapshot: snapshot.clone(),
        previous_snapshot_epoch: None,
        change_class: None,
        policy_digest: Some(policy_digest),
        presentation_frame: None,
        layout_signature: None,
        history_compatibility_hash: None,
        evidence_summary: Some(artifact.contract.evidence_summary.clone()),
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

fn collision_history_hash_candidates(
    plan: &CollisionPlan,
    artifact_kind: crate::collision_plan::CollisionArtifactKind,
    preferred_flavor: CollisionContactNormalFlavor,
) -> Vec<Option<u64>> {
    let mut hashes = Vec::new();
    let mut push = |flavor: Option<CollisionContactNormalFlavor>| {
        let hash = flavor.map(|value| {
            crate::collision_plan::collision_history_compatibility_hash(
                plan.contract_id,
                artifact_kind,
                Some(value),
            )
        });
        if !hashes.contains(&hash) {
            hashes.push(hash);
        }
    };
    push(Some(preferred_flavor));
    push(Some(CollisionContactNormalFlavor::SurfaceGradient));
    push(Some(CollisionContactNormalFlavor::ConservativeUpperBound));
    if !hashes.contains(&None) {
        hashes.push(None);
    }
    hashes
}

fn current_artifact_payload<'a>(
    store: &'a CollisionArtifactStore,
    artifact: &CollisionArtifactBinding,
    snapshot: &WorldSnapshotHandle,
    policy_digest: u64,
    reuse_key: Option<ArtifactReuseKey>,
) -> Result<&'a CollisionArtifactPayload, CollisionExecError> {
    let request = ArtifactLookupRequest {
        contract: artifact.contract.clone(),
        reuse_key,
        current_snapshot: snapshot.clone(),
        previous_snapshot_epoch: None,
        change_class: None,
        policy_digest: Some(policy_digest),
        presentation_frame: None,
        layout_signature: None,
        history_compatibility_hash: None,
        evidence_summary: Some(artifact.contract.evidence_summary.clone()),
    };
    let (artifact, _) = store.lookup(&request);
    artifact
        .map(|artifact| &artifact.payload)
        .ok_or_else(|| CollisionExecError::MissingArtifact {
            artifact_id: request.contract.id.clone(),
        })
}

fn build_broadphase_candidates(
    ctx: &QueryExecContext,
    capture: &SmolStr,
    domain: &KernelValue,
    collision_input: &KernelValue,
    support_summary: &CollisionSupportSummary,
) -> Result<CollisionBroadphaseCandidates, CollisionExecError> {
    let domain = expect_struct(domain, "SceneDomain")?;
    let ops = DirectQueryOps::new(ctx);
    let detail = ops
        .validate_world_domain(capture, domain, "collision broadphase")
        .map_err(|error| CollisionExecError::ExecutionUnavailable {
            message: error.to_string(),
        })?;
    let Some((query_min, query_max)) = collision_query_bounds(collision_input)? else {
        return build_broadphase_candidates_without_query_bounds(ctx, capture, detail);
    };
    if support_summary.can_coarse_support_prune
        && support_summary.has_bounds
        && !aabb_intersects(
            (support_summary.min, support_summary.max),
            (query_min, query_max),
        )
    {
        let rejected_candidate_count = ctx
            .world_acceleration_forest(capture, detail)
            .map(broadphase_leaf_count)
            .unwrap_or(0);
        return Ok(CollisionBroadphaseCandidates {
            candidate_shape_names: Vec::new(),
            rejected_candidate_count,
            pruned_node_count: if rejected_candidate_count > 0 { 1 } else { 0 },
        });
    }
    if let Some(forest) = ctx.world_acceleration_forest(capture, detail) {
        return Ok(traverse_collision_broadphase_forest(
            forest,
            Some((query_min, query_max)),
        ));
    }
    build_broadphase_candidates_without_forest(ctx, capture, detail, Some((query_min, query_max)))
}

fn load_broadphase_candidates(
    _ctx: &QueryExecContext,
    plan: &CollisionPlan,
    store: &CollisionArtifactStore,
    snapshot: &WorldSnapshotHandle,
    capture: &SmolStr,
    domain: &KernelValue,
    collision_input: &KernelValue,
    _support_artifact_id: &SmolStr,
    artifact_id: &SmolStr,
) -> Result<CollisionBroadphaseCandidates, CollisionExecError> {
    let artifact = artifact_binding_by_id(plan, artifact_id)?;
    let reuse_key = Some(artifact_reuse_key(
        artifact,
        snapshot,
        collision_policy_digest(plan.policy),
        collision_broadphase_compatibility_hash(capture, domain, collision_input)?,
    ));
    let payload = current_artifact_payload(
        store,
        artifact,
        snapshot,
        collision_policy_digest(plan.policy),
        reuse_key,
    )?;
    match payload {
        CollisionArtifactPayload::BroadphaseCandidates(payload) => Ok(payload.clone()),
        other => Err(CollisionExecError::TypeMismatch {
            expected: "BroadphaseCandidates".to_string(),
            found: format!("{other:?}"),
        }),
    }
}

fn build_broadphase_candidates_without_query_bounds(
    ctx: &QueryExecContext,
    capture: &SmolStr,
    detail: i32,
) -> Result<CollisionBroadphaseCandidates, CollisionExecError> {
    if let Some(forest) = ctx.world_acceleration_forest(capture, detail) {
        return Ok(traverse_collision_broadphase_forest(forest, None));
    }
    build_broadphase_candidates_without_forest(ctx, capture, detail, None)
}

fn build_broadphase_candidates_without_forest(
    ctx: &QueryExecContext,
    capture: &SmolStr,
    detail: i32,
    query_bounds: Option<([f32; 3], [f32; 3])>,
) -> Result<CollisionBroadphaseCandidates, CollisionExecError> {
    let ops = DirectQueryOps::new(ctx);
    let candidate_shape_names =
        ops.resolve_world_shapes(capture, detail, None)
            .map_err(|error| CollisionExecError::ExecutionUnavailable {
                message: error.to_string(),
            })?;
    let candidate_shape_names = if let Some(query_bounds) = query_bounds {
        let bounded_shapes = ops
            .region_shape_support_bounds(capture, detail)
            .map_err(|error| CollisionExecError::ExecutionUnavailable {
                message: error.to_string(),
            })?
            .into_iter()
            .map(|(shape, min, max)| (shape, (min, max)))
            .collect::<BTreeMap<_, _>>();
        candidate_shape_names
            .into_iter()
            .filter(|shape| match bounded_shapes.get(shape) {
                Some(bounds) => aabb_intersects(*bounds, query_bounds),
                None => true,
            })
            .collect()
    } else {
        candidate_shape_names
    };
    Ok(CollisionBroadphaseCandidates {
        candidate_shape_names,
        rejected_candidate_count: 0,
        pruned_node_count: 0,
    })
}

fn traverse_collision_broadphase_forest(
    forest: &AccelerationForest,
    query_bounds: Option<([f32; 3], [f32; 3])>,
) -> CollisionBroadphaseCandidates {
    let node_lookup = forest
        .nodes
        .iter()
        .map(|node| (node.id.clone(), node))
        .collect::<HashMap<_, _>>();
    let mut candidates = BTreeSet::new();
    let mut rejected_candidate_count = 0u32;
    let mut pruned_node_count = 0u32;
    let mut leaf_count_cache = HashMap::<SmolStr, u32>::new();
    for root in forest.root_nodes() {
        traverse_collision_broadphase_node(
            root,
            &node_lookup,
            query_bounds,
            &mut candidates,
            &mut rejected_candidate_count,
            &mut pruned_node_count,
            &mut leaf_count_cache,
        );
    }
    CollisionBroadphaseCandidates {
        candidate_shape_names: candidates.into_iter().collect(),
        rejected_candidate_count,
        pruned_node_count,
    }
}

fn traverse_collision_broadphase_node<'a>(
    node_id: &SmolStr,
    node_lookup: &HashMap<SmolStr, &'a AccelerationNode>,
    query_bounds: Option<([f32; 3], [f32; 3])>,
    candidates: &mut BTreeSet<SmolStr>,
    rejected_candidate_count: &mut u32,
    pruned_node_count: &mut u32,
    leaf_count_cache: &mut HashMap<SmolStr, u32>,
) {
    let Some(node) = node_lookup.get(node_id).copied() else {
        return;
    };
    if let (Some(bounds), Some(query_bounds)) = (forest_node_bounds(node), query_bounds) {
        if !aabb_intersects(bounds, query_bounds) {
            *pruned_node_count += 1;
            *rejected_candidate_count +=
                broadphase_leaf_count_from_node(node_id, node_lookup, leaf_count_cache);
            return;
        }
    }
    if node.child_ids.is_empty() {
        if let Some(leaf) = &node.leaf_payload {
            candidates.insert(leaf.semantic_id.clone());
        }
        return;
    }
    for child_id in &node.child_ids {
        traverse_collision_broadphase_node(
            child_id,
            node_lookup,
            query_bounds,
            candidates,
            rejected_candidate_count,
            pruned_node_count,
            leaf_count_cache,
        );
    }
}

fn broadphase_leaf_count(forest: &AccelerationForest) -> u32 {
    let node_lookup = forest
        .nodes
        .iter()
        .map(|node| (node.id.clone(), node))
        .collect::<HashMap<_, _>>();
    let mut leaf_count_cache = HashMap::<SmolStr, u32>::new();
    forest
        .root_nodes()
        .iter()
        .map(|root| broadphase_leaf_count_from_node(root, &node_lookup, &mut leaf_count_cache))
        .sum()
}

fn broadphase_leaf_count_from_node<'a>(
    node_id: &SmolStr,
    node_lookup: &HashMap<SmolStr, &'a AccelerationNode>,
    leaf_count_cache: &mut HashMap<SmolStr, u32>,
) -> u32 {
    if let Some(count) = leaf_count_cache.get(node_id) {
        return *count;
    }
    let Some(node) = node_lookup.get(node_id).copied() else {
        return 0;
    };
    let count = if node.child_ids.is_empty() {
        u32::from(node.leaf_payload.is_some())
    } else {
        node.child_ids
            .iter()
            .map(|child_id| {
                broadphase_leaf_count_from_node(child_id, node_lookup, leaf_count_cache)
            })
            .sum()
    };
    leaf_count_cache.insert(node_id.clone(), count);
    count
}

fn forest_node_bounds(node: &AccelerationNode) -> Option<([f32; 3], [f32; 3])> {
    node.bounds.iter().find_map(|bound| {
        if !matches!(bound.kind, BoundDescriptorKind::AxisAlignedBounds) {
            return None;
        }
        parse_support_bounds_summary(&bound.summary).map(|bounds| (bounds.min, bounds.max))
    })
}

fn parse_support_bounds_summary(summary: &str) -> Option<SupportBoundsSummary> {
    let (min, max) = summary.split_once("|max=")?;
    let min = min.strip_prefix("min=")?;
    Some(SupportBoundsSummary {
        min: parse_summary_vec3(min)?,
        max: parse_summary_vec3(max)?,
    })
}

fn parse_summary_vec3(summary: &str) -> Option<[f32; 3]> {
    let parts = summary
        .split(',')
        .map(|part| part.trim().parse::<f32>().ok())
        .collect::<Option<Vec<_>>>()?;
    let [x, y, z] = parts.try_into().ok()?;
    Some([x, y, z])
}

#[derive(Debug, Clone, Copy)]
struct SupportBoundsSummary {
    min: [f32; 3],
    max: [f32; 3],
}

fn load_transition_reuse(
    plan: &CollisionPlan,
    store: &CollisionArtifactStore,
    snapshot: &WorldSnapshotHandle,
    transition: Option<CollisionSnapshotTransitionInput>,
    witness_artifact: &SmolStr,
    continuation_artifact: &SmolStr,
    metrics: &mut CollisionReuseMetrics,
    decisions: &mut Vec<CollisionReuseDecision>,
) -> Result<CollisionTransitionReuseSeed, CollisionExecError> {
    let mut seed = CollisionTransitionReuseSeed {
        seed_fraction: None,
        normal_flavor: CollisionContactNormalFlavor::ConservativeUpperBound,
        normal_provenance: None,
        certificate: None,
    };
    for artifact_id in [witness_artifact, continuation_artifact] {
        let artifact = artifact_binding_by_id(plan, artifact_id)?;
        let decision = if let Some(transition) = transition {
            let previous_snapshot_epoch =
                crate::world_identity::SnapshotEpoch(u64::from(transition.previous_snapshot_epoch));
            let mut candidate = None;
            let mut report = None;
            for history_hash in
                collision_history_hash_candidates(plan, artifact.kind, seed.normal_flavor)
            {
                let request = ArtifactLookupRequest {
                    contract: artifact.contract.clone(),
                    reuse_key: None,
                    current_snapshot: snapshot.clone(),
                    previous_snapshot_epoch: Some(previous_snapshot_epoch),
                    change_class: Some(transition.change_class),
                    policy_digest: Some(collision_policy_digest(plan.policy)),
                    presentation_frame: None,
                    layout_signature: None,
                    history_compatibility_hash: history_hash,
                    evidence_summary: Some(artifact.contract.evidence_summary.clone()),
                };
                let (lookup_candidate, lookup_report) = store.lookup(&request);
                let stop = lookup_candidate.is_some() || lookup_report.index_candidates == 0;
                candidate = lookup_candidate;
                report = Some(lookup_report);
                if stop {
                    break;
                }
            }
            let report = report.expect("collision transition reuse lookup should produce a report");
            let mut verdict = if candidate.is_some() {
                (CollisionReuseVerdict::Consumed, CollisionReuseReason::None)
            } else if report.index_candidates == 0 {
                (
                    CollisionReuseVerdict::Unavailable,
                    CollisionReuseReason::ArtifactUnavailable,
                )
            } else if !report.validity_reports.is_empty() {
                (
                    CollisionReuseVerdict::Rejected,
                    CollisionReuseReason::ValidityRejected,
                )
            } else if !report.compatibility_rejections.is_empty() {
                (
                    CollisionReuseVerdict::Rejected,
                    CollisionReuseReason::CompatibilityRejected,
                )
            } else {
                (
                    CollisionReuseVerdict::Rejected,
                    CollisionReuseReason::ValidityRejected,
                )
            };
            let mut detail = report
                .primary_rejection_reason()
                .unwrap_or_else(|| SmolStr::new("accepted"));
            if let Some(candidate) = candidate {
                match &candidate.payload {
                    CollisionArtifactPayload::WitnessCache(payload) => {
                        if payload.certificate.metadata.reusable_by
                            == CertificateReuseClass::RenderingOnly
                        {
                            verdict = (
                                CollisionReuseVerdict::Rejected,
                                CollisionReuseReason::RenderingOnlyCertificate,
                            );
                            detail = SmolStr::new("rendering-only-certificate");
                        } else {
                            seed.seed_fraction = payload
                                .certificate
                                .bracket
                                .map(|bracket| bracket[0])
                                .or(payload.contact_fraction_upper_bound);
                            seed.normal_flavor = payload.normal_flavor;
                            seed.normal_provenance = payload.normal_provenance;
                            seed.certificate = Some(payload.certificate.clone());
                        }
                    }
                    CollisionArtifactPayload::ContinuationSeed(payload) => {
                        if payload.certificate.metadata.reusable_by
                            == CertificateReuseClass::RenderingOnly
                        {
                            verdict = (
                                CollisionReuseVerdict::Rejected,
                                CollisionReuseReason::RenderingOnlyCertificate,
                            );
                            detail = SmolStr::new("rendering-only-certificate");
                        } else {
                            seed.seed_fraction = payload
                                .certificate
                                .bracket
                                .map(|bracket| bracket[0])
                                .or(Some(payload.fraction_hint));
                            seed.normal_flavor = payload.normal_flavor;
                            seed.normal_provenance = payload.normal_provenance;
                            seed.certificate = Some(payload.certificate.clone());
                        }
                    }
                    CollisionArtifactPayload::SupportSummary(_)
                    | CollisionArtifactPayload::BroadphaseCandidates(_) => {}
                }
            }
            CollisionReuseDecision {
                artifact_id: artifact.id.clone(),
                artifact_kind: artifact.kind,
                verdict: verdict.0,
                reason: verdict.1,
                detail,
                lookup: Some(report),
            }
        } else {
            CollisionReuseDecision {
                artifact_id: artifact.id.clone(),
                artifact_kind: artifact.kind,
                verdict: CollisionReuseVerdict::Unavailable,
                reason: CollisionReuseReason::MissingPreviousSnapshot,
                detail: SmolStr::new("transition input missing"),
                lookup: None,
            }
        };
        update_reuse_metrics(metrics, &decision);
        decisions.push(decision);
    }
    Ok(seed)
}

fn update_reuse_metrics(metrics: &mut CollisionReuseMetrics, decision: &CollisionReuseDecision) {
    match decision.verdict {
        CollisionReuseVerdict::Consumed => {
            metrics.available_count += 1;
            metrics.consumed_count += 1;
        }
        CollisionReuseVerdict::Rejected => {
            metrics.rejected_count += 1;
        }
        CollisionReuseVerdict::Unavailable => {
            metrics.unavailable_count += 1;
        }
    }
    metrics.diagnostics.push(format!(
        "artifact={} kind={} verdict={} reason={} detail={}",
        decision.artifact_id,
        collision_artifact_kind_name(decision.artifact_kind),
        collision_reuse_verdict_name(decision.verdict),
        collision_reuse_reason_name(decision.reason),
        decision.detail
    ));
}

fn store_transition_artifacts(
    plan: &CollisionPlan,
    store: &mut CollisionArtifactStore,
    snapshot: &WorldSnapshotHandle,
    witness_artifact: &SmolStr,
    continuation_artifact: &SmolStr,
    outcome: SweepOutcome,
) -> Result<(), CollisionExecError> {
    let normal_provenance = outcome.contact_normal_provenance;
    insert_artifact(
        store,
        artifact_binding_by_id(plan, witness_artifact)?,
        snapshot,
        collision_policy_digest(plan.policy),
        artifact_binding_by_id(plan, witness_artifact)?
            .contract
            .logical_schema
            .stable_hash(),
        Some(crate::collision_plan::collision_history_compatibility_hash(
            plan.contract_id,
            crate::collision_plan::CollisionArtifactKind::WitnessCache,
            Some(outcome.normal_flavor),
        )),
        CollisionArtifactPayload::WitnessCache(CollisionStoredWitness {
            hit: outcome.hit,
            contact_fraction_upper_bound: outcome.fraction_upper_bound,
            separation_upper_bound: outcome.separation_upper_bound,
            normal_provenance,
            normal_flavor: outcome.normal_flavor,
            certificate: outcome.certificate.clone(),
        }),
    );
    insert_artifact(
        store,
        artifact_binding_by_id(plan, continuation_artifact)?,
        snapshot,
        collision_policy_digest(plan.policy),
        artifact_binding_by_id(plan, continuation_artifact)?
            .contract
            .logical_schema
            .stable_hash(),
        Some(crate::collision_plan::collision_history_compatibility_hash(
            plan.contract_id,
            crate::collision_plan::CollisionArtifactKind::ContinuationSeed,
            Some(outcome.normal_flavor),
        )),
        CollisionArtifactPayload::ContinuationSeed(CollisionContinuationSeed {
            fraction_hint: outcome.fraction_upper_bound.unwrap_or(1.0),
            no_hit_certificate: outcome.no_hit_certificate.is_some(),
            separation_upper_bound: outcome.separation_upper_bound,
            normal_provenance,
            normal_flavor: outcome.normal_flavor,
            certificate: outcome.certificate,
        }),
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn sweep_outcome(
    ctx: &QueryExecContext,
    backend: DispatchBackend,
    policy: &QueryExecutionPolicy,
    snapshot: &WorldSnapshotHandle,
    capture: &KernelValue,
    domain: &KernelValue,
    sweep: CollisionSphereSweepInput,
    distance_contract: QueryContractId,
    normal_contract: QueryContractId,
    distance_capture_plan: &KernelCaptureQueryPlan,
    normal_capture_plan: &KernelCaptureQueryPlan,
    seed: CollisionTransitionReuseSeed,
    candidate_shape_names: &[SmolStr],
    executed_query_contracts: &mut Vec<QueryContractId>,
) -> Result<SweepOutcome, CollisionExecError> {
    let seed_normal_flavor = seed.normal_flavor;
    if candidate_shape_names.is_empty() {
        let certificate = build_collision_no_hit_certificate(
            plan_no_hit_guarantee(policy),
            "collision.sweep.no_hit",
            sweep.contact_tolerance,
            0.0,
            1.0,
            seed_normal_flavor,
            seed.normal_provenance,
        );
        return Ok(SweepOutcome {
            hit: false,
            fraction_upper_bound: None,
            separation_upper_bound: None,
            point_on_probe: None,
            point_on_world: None,
            contact_normal: None,
            contact_normal_provenance: None,
            normal_flavor: seed_normal_flavor,
            no_hit_certificate: Some(CollisionNoHitCertificate {
                valid_through_fraction: certificate.t_end,
                guarantee: plan_no_hit_guarantee(policy),
            }),
            certificate,
            interval_subdivisions: 0,
            interval_refinements: 0,
            certificate_successes: 1,
            fallback_count: 0,
        });
    }
    let travel = subtract(sweep.end_center, sweep.start_center);
    let length = magnitude(travel);
    if length <= f32::EPSILON {
        return sphere_overlap_like_outcome(
            ctx,
            backend,
            policy,
            snapshot,
            capture,
            domain,
            candidate_shape_names,
            sweep.start_center,
            sweep.radius,
            distance_capture_plan,
            normal_capture_plan,
            distance_contract,
            normal_contract,
            seed.clone(),
            sweep.contact_tolerance,
            executed_query_contracts,
        );
    }
    let mut fraction = seed
        .certificate
        .as_ref()
        .and_then(|certificate| certificate.bracket.map(|bracket| bracket[0]))
        .or(seed.seed_fraction)
        .unwrap_or(0.0)
        .clamp(0.0, 1.0);
    let mut iterations = 0;
    let mut interval_subdivisions = 0;
    let mut interval_refinements = 0;
    while iterations < sweep.max_iterations.max(1) {
        interval_subdivisions += 1;
        let center = lerp(sweep.start_center, sweep.end_center, fraction);
        let (distance_value, normal_value, provenance, _) = candidate_limited_point_query(
            ctx,
            backend,
            policy,
            snapshot,
            capture,
            domain,
            candidate_shape_names,
            center,
            distance_capture_plan,
            normal_capture_plan,
            distance_contract,
            normal_contract,
            executed_query_contracts,
        )?;
        let separation = expect_f32(&distance_value)? - sweep.radius;
        if separation <= sweep.contact_tolerance {
            let world_normal = expect_vec3(&normal_value)?;
            let point_on_probe = offset_point(center, world_normal, -sweep.radius);
            let point_on_world = offset_point(center, world_normal, -(separation + sweep.radius));
            return Ok(SweepOutcome {
                hit: true,
                fraction_upper_bound: Some(fraction),
                separation_upper_bound: Some(separation),
                point_on_probe: Some(point_on_probe),
                point_on_world: Some(point_on_world),
                contact_normal: Some(world_normal),
                contact_normal_provenance: provenance,
                normal_flavor: collision_contact_normal_flavor_from_provenance(provenance),
                no_hit_certificate: None,
                certificate: build_collision_contact_certificate(
                    "collision.sweep.contact",
                    plan_contact_guarantee(policy),
                    fraction,
                    fraction,
                    Some([fraction, fraction]),
                    separation,
                    provenance,
                    collision_contact_normal_flavor_from_provenance(provenance),
                ),
                interval_subdivisions,
                interval_refinements: interval_refinements + 1,
                certificate_successes: 1,
                fallback_count: 0,
            });
        }
        let remaining = length * (1.0 - fraction);
        if separation >= remaining {
            let certificate = build_collision_no_hit_certificate(
                plan_no_hit_guarantee(policy),
                "collision.sweep.no_hit",
                sweep.contact_tolerance,
                fraction,
                1.0,
                seed.normal_flavor,
                seed.normal_provenance,
            );
            return Ok(SweepOutcome {
                hit: false,
                fraction_upper_bound: None,
                separation_upper_bound: Some(separation),
                point_on_probe: None,
                point_on_world: None,
                contact_normal: None,
                normal_flavor: seed_normal_flavor,
                contact_normal_provenance: seed.normal_provenance,
                no_hit_certificate: Some(CollisionNoHitCertificate {
                    valid_through_fraction: certificate.t_end,
                    guarantee: plan_no_hit_guarantee(policy),
                }),
                certificate,
                interval_subdivisions,
                interval_refinements: interval_refinements + 1,
                certificate_successes: 1,
                fallback_count: 0,
            });
        }
        let step_fraction = (separation.max(sweep.contact_tolerance) / length).max(0.0005);
        let next_fraction = (fraction + step_fraction).min(1.0);
        if next_fraction <= fraction + f32::EPSILON {
            break;
        }
        fraction = next_fraction;
        interval_refinements += 1;
        iterations += 1;
    }
    dense_fallback_sweep_outcome(
        ctx,
        backend,
        policy,
        snapshot,
        capture,
        domain,
        sweep,
        candidate_shape_names,
        distance_capture_plan,
        normal_capture_plan,
        distance_contract,
        normal_contract,
        fraction,
        seed,
        interval_subdivisions,
        interval_refinements,
        executed_query_contracts,
    )
}

#[allow(clippy::too_many_arguments)]
fn dense_fallback_sweep_outcome(
    ctx: &QueryExecContext,
    backend: DispatchBackend,
    policy: &QueryExecutionPolicy,
    snapshot: &WorldSnapshotHandle,
    capture: &KernelValue,
    domain: &KernelValue,
    sweep: CollisionSphereSweepInput,
    candidate_shape_names: &[SmolStr],
    distance_capture_plan: &KernelCaptureQueryPlan,
    normal_capture_plan: &KernelCaptureQueryPlan,
    distance_contract: QueryContractId,
    normal_contract: QueryContractId,
    certified_start_fraction: f32,
    seed: CollisionTransitionReuseSeed,
    mut interval_subdivisions: u32,
    mut interval_refinements: u32,
    executed_query_contracts: &mut Vec<QueryContractId>,
) -> Result<SweepOutcome, CollisionExecError> {
    let dense_budget = sweep.max_iterations.max(1).saturating_mul(8).max(16) as u32;
    let remaining_fraction = (1.0 - certified_start_fraction).max(0.0);
    let dense_step = if remaining_fraction <= f32::EPSILON {
        0.0
    } else {
        (remaining_fraction / dense_budget as f32).max(0.0005)
    };
    let mut fraction = certified_start_fraction;
    let mut last_separation = None;
    while fraction < 1.0 - f32::EPSILON && dense_step > 0.0 {
        fraction = (fraction + dense_step).min(1.0);
        interval_subdivisions += 1;
        let center = lerp(sweep.start_center, sweep.end_center, fraction);
        let (distance_value, normal_value, provenance, _) = candidate_limited_point_query(
            ctx,
            backend,
            policy,
            snapshot,
            capture,
            domain,
            candidate_shape_names,
            center,
            distance_capture_plan,
            normal_capture_plan,
            distance_contract,
            normal_contract,
            executed_query_contracts,
        )?;
        interval_refinements += 1;
        let separation = expect_f32(&distance_value)? - sweep.radius;
        last_separation = Some(separation);
        if separation <= sweep.contact_tolerance {
            let world_normal = expect_vec3(&normal_value)?;
            let point_on_probe = offset_point(center, world_normal, -sweep.radius);
            let point_on_world = offset_point(center, world_normal, -(separation + sweep.radius));
            let normal_flavor = collision_contact_normal_flavor_from_provenance(provenance);
            return Ok(SweepOutcome {
                hit: true,
                fraction_upper_bound: Some(fraction),
                separation_upper_bound: Some(separation),
                point_on_probe: Some(point_on_probe),
                point_on_world: Some(point_on_world),
                contact_normal: Some(world_normal),
                contact_normal_provenance: provenance,
                normal_flavor,
                no_hit_certificate: None,
                certificate: build_collision_contact_certificate(
                    "collision.sweep.fallback_contact",
                    plan_contact_guarantee(policy),
                    certified_start_fraction,
                    fraction,
                    Some([certified_start_fraction, fraction]),
                    separation,
                    provenance,
                    normal_flavor,
                ),
                interval_subdivisions,
                interval_refinements,
                certificate_successes: 0,
                fallback_count: 1,
            });
        }
    }

    let certificate = build_collision_no_hit_certificate(
        seed.certificate
            .as_ref()
            .map(|certificate| certificate.metadata.guarantee)
            .unwrap_or_else(|| plan_no_hit_guarantee(policy)),
        "collision.sweep.partial_no_hit",
        sweep.contact_tolerance,
        certified_start_fraction,
        certified_start_fraction,
        seed.normal_flavor,
        seed.normal_provenance,
    );
    Ok(SweepOutcome {
        hit: false,
        fraction_upper_bound: None,
        separation_upper_bound: last_separation,
        point_on_probe: None,
        point_on_world: None,
        contact_normal: None,
        contact_normal_provenance: seed.normal_provenance,
        normal_flavor: seed.normal_flavor,
        no_hit_certificate: Some(CollisionNoHitCertificate {
            valid_through_fraction: certificate.t_end,
            guarantee: certificate.metadata.guarantee,
        }),
        certificate,
        interval_subdivisions,
        interval_refinements,
        certificate_successes: 0,
        fallback_count: 1,
    })
}

#[allow(clippy::too_many_arguments)]
fn sphere_overlap_like_outcome(
    ctx: &QueryExecContext,
    backend: DispatchBackend,
    policy: &QueryExecutionPolicy,
    snapshot: &WorldSnapshotHandle,
    capture: &KernelValue,
    domain: &KernelValue,
    candidate_shape_names: &[SmolStr],
    center: [f32; 3],
    radius: f32,
    distance_capture_plan: &KernelCaptureQueryPlan,
    normal_capture_plan: &KernelCaptureQueryPlan,
    distance_contract: QueryContractId,
    normal_contract: QueryContractId,
    seed: CollisionTransitionReuseSeed,
    tolerance: f32,
    executed_query_contracts: &mut Vec<QueryContractId>,
) -> Result<SweepOutcome, CollisionExecError> {
    let (distance_value, normal_value, provenance, _) = candidate_limited_point_query(
        ctx,
        backend,
        policy,
        snapshot,
        capture,
        domain,
        candidate_shape_names,
        center,
        distance_capture_plan,
        normal_capture_plan,
        distance_contract,
        normal_contract,
        executed_query_contracts,
    )?;
    let separation = expect_f32(&distance_value)? - radius;
    if separation <= tolerance {
        let world_normal = expect_vec3(&normal_value)?;
        let normal_flavor = collision_contact_normal_flavor_from_provenance(provenance);
        Ok(SweepOutcome {
            hit: true,
            fraction_upper_bound: Some(1.0),
            separation_upper_bound: Some(separation),
            point_on_probe: Some(offset_point(center, world_normal, -radius)),
            point_on_world: Some(offset_point(center, world_normal, -(separation + radius))),
            contact_normal: Some(world_normal),
            contact_normal_provenance: provenance,
            normal_flavor,
            no_hit_certificate: None,
            certificate: build_collision_contact_certificate(
                "collision.sweep.contact",
                plan_contact_guarantee(policy),
                1.0,
                1.0,
                Some([1.0, 1.0]),
                separation,
                provenance,
                normal_flavor,
            ),
            interval_subdivisions: 1,
            interval_refinements: 1,
            certificate_successes: 1,
            fallback_count: 0,
        })
    } else {
        let certificate = build_collision_no_hit_certificate(
            seed.certificate
                .as_ref()
                .map(|certificate| certificate.metadata.guarantee)
                .unwrap_or_else(|| plan_no_hit_guarantee(policy)),
            "collision.sweep.no_hit",
            tolerance,
            1.0,
            1.0,
            seed.normal_flavor,
            seed.normal_provenance,
        );
        Ok(SweepOutcome {
            hit: false,
            fraction_upper_bound: None,
            separation_upper_bound: Some(separation),
            point_on_probe: None,
            point_on_world: None,
            contact_normal: None,
            contact_normal_provenance: seed.normal_provenance,
            normal_flavor: seed.normal_flavor,
            no_hit_certificate: Some(CollisionNoHitCertificate {
                valid_through_fraction: certificate.t_end,
                guarantee: certificate.metadata.guarantee,
            }),
            certificate,
            interval_subdivisions: 1,
            interval_refinements: 1,
            certificate_successes: 1,
            fallback_count: 0,
        })
    }
}

fn plan_no_hit_guarantee(
    policy: &QueryExecutionPolicy,
) -> crate::execution_policy::RequiredGuaranteeClass {
    match policy.required_guarantee {
        crate::execution_policy::RequiredGuaranteeClass::IntervalBounded => {
            crate::execution_policy::RequiredGuaranteeClass::IntervalBounded
        }
        _ => crate::execution_policy::RequiredGuaranteeClass::ConservativeNoFalseMiss,
    }
}

fn plan_contact_guarantee(
    policy: &QueryExecutionPolicy,
) -> crate::execution_policy::RequiredGuaranteeClass {
    policy.required_guarantee
}

fn collision_contact_normal_provenance_from_trace(
    trace: &DirectQueryExecutionTrace,
) -> Option<CollisionContactNormalProvenance> {
    match trace.observability.normal_role.as_deref() {
        Some("normal_role::certified_field_gradient") => {
            Some(CollisionContactNormalProvenance::CertifiedFieldGradient)
        }
        Some("normal_role::feature_normal") => {
            Some(CollisionContactNormalProvenance::FeatureNormal)
        }
        Some("normal_role::heuristic_shading_normal") => {
            Some(CollisionContactNormalProvenance::HeuristicShadingNormal)
        }
        _ => None,
    }
}

fn collision_contact_normal_flavor_from_provenance(
    provenance: Option<CollisionContactNormalProvenance>,
) -> CollisionContactNormalFlavor {
    match provenance {
        Some(CollisionContactNormalProvenance::HeuristicShadingNormal) | None => {
            CollisionContactNormalFlavor::ConservativeUpperBound
        }
        Some(CollisionContactNormalProvenance::CertifiedFieldGradient)
        | Some(CollisionContactNormalProvenance::FeatureNormal) => {
            CollisionContactNormalFlavor::SurfaceGradient
        }
    }
}

fn build_collision_contact_certificate(
    proof_family: impl Into<SmolStr>,
    guarantee: crate::execution_policy::RequiredGuaranteeClass,
    t_start: f32,
    t_end: f32,
    bracket: Option<[f32; 2]>,
    separation_upper_bound: f32,
    provenance: Option<CollisionContactNormalProvenance>,
    normal_flavor: CollisionContactNormalFlavor,
) -> RayStepCertificate {
    let provenance = provenance
        .map(crate::collision_contract::collision_contact_normal_provenance_name)
        .unwrap_or("none");
    let tolerance_context = format!(
        "separation_upper_bound={separation_upper_bound:.6}; normal_flavor={}; normal_provenance={provenance}",
        crate::collision_contract::collision_contact_normal_flavor_name(normal_flavor)
    );
    RayStepCertificate {
        kind: StepCertificateKind::RefinementBracket,
        metadata: RayStepCertificateMetadata {
            guarantee,
            proof_family: proof_family.into(),
            subject: SmolStr::new("collision.transition"),
            subject_kind: RayStepCertificateSubjectKind::Interval,
            tolerance_context: SmolStr::new(tolerance_context),
            reusable_by: CertificateReuseClass::RenderingAndCollision,
            invalidation_reasons: vec![
                SmolStr::new("collision distance semantics changed"),
                SmolStr::new("collision normal provenance changed"),
            ],
        },
        t_start,
        t_end,
        no_hit_before_t_end: true,
        bracket,
        provenance: None,
    }
}

fn build_collision_no_hit_certificate(
    guarantee: crate::execution_policy::RequiredGuaranteeClass,
    proof_family: impl Into<SmolStr>,
    tolerance: f32,
    t_start: f32,
    t_end: f32,
    normal_flavor: CollisionContactNormalFlavor,
    provenance: Option<CollisionContactNormalProvenance>,
) -> RayStepCertificate {
    let provenance = provenance
        .map(crate::collision_contract::collision_contact_normal_provenance_name)
        .unwrap_or("none");
    let tolerance_context = format!(
        "tolerance={tolerance:.6}; normal_flavor={}; normal_provenance={provenance}",
        crate::collision_contract::collision_contact_normal_flavor_name(normal_flavor)
    );
    RayStepCertificate {
        kind: StepCertificateKind::IntervalNoRootProof,
        metadata: RayStepCertificateMetadata {
            guarantee,
            proof_family: proof_family.into(),
            subject: SmolStr::new("collision.transition"),
            subject_kind: RayStepCertificateSubjectKind::Interval,
            tolerance_context: SmolStr::new(tolerance_context),
            reusable_by: CertificateReuseClass::RenderingAndCollision,
            invalidation_reasons: vec![
                SmolStr::new("collision distance semantics changed"),
                SmolStr::new("collision support forest changed"),
            ],
        },
        t_start,
        t_end,
        no_hit_before_t_end: true,
        bracket: Some([t_start, t_end]),
        provenance: None,
    }
}

fn output_kind_for_value(
    value: &CollisionMaterializedValue,
) -> crate::collision_contract::CollisionOutputKind {
    match value {
        CollisionMaterializedValue::Occupancy(_) => {
            crate::collision_contract::CollisionOutputKind::Occupancy
        }
        CollisionMaterializedValue::RayCast(_) => {
            crate::collision_contract::CollisionOutputKind::RayCast
        }
        CollisionMaterializedValue::SphereOverlap(_) => {
            crate::collision_contract::CollisionOutputKind::SphereOverlap
        }
        CollisionMaterializedValue::Sweep(_) => {
            crate::collision_contract::CollisionOutputKind::SweepContact
        }
        CollisionMaterializedValue::TimeOfImpact(_) => {
            crate::collision_contract::CollisionOutputKind::TimeOfImpact
        }
    }
}

fn materialize_value(
    pass: &CollisionPass,
    value: CollisionMaterializedValue,
    values: &mut BTreeMap<SmolStr, CollisionMaterializedValue>,
) -> Result<(), CollisionExecError> {
    if pass.materializes.is_empty() {
        return Err(CollisionExecError::InvalidPass {
            pass_id: pass.id.clone(),
            message: "pass does not materialize any values".to_string(),
        });
    }
    for id in &pass.materializes {
        values.insert(id.clone(), value.clone());
    }
    Ok(())
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

fn subtract(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn magnitude(value: [f32; 3]) -> f32 {
    (value[0] * value[0] + value[1] * value[1] + value[2] * value[2]).sqrt()
}

fn lerp(start: [f32; 3], end: [f32; 3], t: f32) -> [f32; 3] {
    [
        start[0] + (end[0] - start[0]) * t,
        start[1] + (end[1] - start[1]) * t,
        start[2] + (end[2] - start[2]) * t,
    ]
}

fn offset_point(point: [f32; 3], normal: [f32; 3], distance: f32) -> [f32; 3] {
    [
        point[0] + normal[0] * distance,
        point[1] + normal[1] * distance,
        point[2] + normal[2] * distance,
    ]
}

fn collision_transition_input(
    value: &KernelValue,
) -> Result<CollisionSnapshotTransitionInput, CollisionExecError> {
    let transition = expect_struct(value, "CollisionSnapshotTransitionInput")?;
    Ok(CollisionSnapshotTransitionInput {
        current_snapshot_epoch: expect_u32(field(transition, "current_snapshot_epoch")?)?,
        previous_snapshot_epoch: expect_u32(field(transition, "previous_snapshot_epoch")?)?,
        change_class: expect_change_class(field(transition, "change_class")?)?,
    })
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

fn collision_sweep_input(
    value: &KernelValue,
) -> Result<CollisionSphereSweepInput, CollisionExecError> {
    let sweep = expect_struct(value, "CollisionSphereSweepInput")?;
    Ok(CollisionSphereSweepInput {
        start_center: expect_vec3(field(sweep, "start_center")?)?,
        end_center: expect_vec3(field(sweep, "end_center")?)?,
        radius: expect_f32(field(sweep, "radius")?)?,
        contact_tolerance: expect_f32(field(sweep, "contact_tolerance")?)?,
        max_iterations: expect_i32(field(sweep, "max_iterations")?)?,
    })
}

fn parse_collision_support_summary(
    value: &KernelValue,
) -> Result<CollisionSupportSummary, CollisionExecError> {
    let summary = expect_struct(value, "SupportSummaryResult")?;
    Ok(CollisionSupportSummary {
        support_class: expect_u32(field(summary, "support_class")?)?,
        semantics: expect_u32(field(summary, "semantics")?)?,
        has_bounds: expect_bool(field(summary, "has_bounds")?)?,
        opaque_boundary: expect_bool(field(summary, "opaque_boundary")?)?,
        can_coarse_support_prune: expect_bool(field(summary, "can_coarse_support_prune")?)?,
        min: expect_vec3(field(summary, "min")?)?,
        max: expect_vec3(field(summary, "max")?)?,
    })
}

fn collision_query_bounds(
    collision_input: &KernelValue,
) -> Result<Option<([f32; 3], [f32; 3])>, CollisionExecError> {
    if let Ok(point) = collision_point_input(collision_input) {
        return Ok(Some((point.point, point.point)));
    }
    if let Ok(ray) = collision_ray_input(collision_input) {
        let end = [
            ray.origin[0] + ray.direction[0] * ray.max_distance,
            ray.origin[1] + ray.direction[1] * ray.max_distance,
            ray.origin[2] + ray.direction[2] * ray.max_distance,
        ];
        return Ok(Some((
            component_min(ray.origin, end),
            component_max(ray.origin, end),
        )));
    }
    if let Ok(probe) = collision_sphere_input(collision_input) {
        let radius = [probe.radius; 3];
        return Ok(Some((
            subtract(probe.center, radius),
            [
                probe.center[0] + radius[0],
                probe.center[1] + radius[1],
                probe.center[2] + radius[2],
            ],
        )));
    }
    if let Ok(sweep) = collision_sweep_input(collision_input) {
        let radius = [sweep.radius; 3];
        let min = subtract(component_min(sweep.start_center, sweep.end_center), radius);
        let extent = component_max(sweep.start_center, sweep.end_center);
        let max = [
            extent[0] + radius[0],
            extent[1] + radius[1],
            extent[2] + radius[2],
        ];
        return Ok(Some((min, max)));
    }
    Ok(None)
}

fn aabb_intersects(lhs: ([f32; 3], [f32; 3]), rhs: ([f32; 3], [f32; 3])) -> bool {
    (0..3).all(|axis| lhs.0[axis] <= rhs.1[axis] && rhs.0[axis] <= lhs.1[axis])
}

fn component_min(lhs: [f32; 3], rhs: [f32; 3]) -> [f32; 3] {
    [lhs[0].min(rhs[0]), lhs[1].min(rhs[1]), lhs[2].min(rhs[2])]
}

fn component_max(lhs: [f32; 3], rhs: [f32; 3]) -> [f32; 3] {
    [lhs[0].max(rhs[0]), lhs[1].max(rhs[1]), lhs[2].max(rhs[2])]
}

fn ray_query_value(ray: &CollisionRayInput) -> KernelValue {
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

fn expect_change_class(
    value: &KernelValue,
) -> Result<crate::state_advance::ChangeClass, CollisionExecError> {
    let id = expect_u32(value)?;
    match id {
        0 => Ok(crate::state_advance::ChangeClass::None),
        1 => Ok(crate::state_advance::ChangeClass::Presentation),
        2 => Ok(crate::state_advance::ChangeClass::Structural),
        3 => Ok(crate::state_advance::ChangeClass::Topology),
        4 => Ok(crate::state_advance::ChangeClass::Identity),
        5 => Ok(crate::state_advance::ChangeClass::Incompatible),
        other => Err(type_mismatch(
            "ChangeClass",
            format!("unknown_change_class_id={other}"),
        )),
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
