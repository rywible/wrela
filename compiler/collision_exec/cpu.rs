use crate::artifact_key::ArtifactReuseKey;
use crate::artifact_store::{
    ArtifactInstanceMetadata, ArtifactLookupRequest, ArtifactStore, StoredArtifact,
};
use crate::collision_contract::{
    CollisionContactNormalFlavor, CollisionNoHitCertificate, CollisionOccupancyClass,
    CollisionOccupancyResult, CollisionPointInput, CollisionPointWitness, CollisionRayCastResult,
    CollisionRayInput, CollisionRayMissReason, CollisionRayWitness, CollisionResult,
    CollisionSnapshotTransitionInput, CollisionSphereOverlapResult, CollisionSphereProbe,
    CollisionSphereSweepInput, CollisionSphereWitness, CollisionSweepResult, CollisionSweepWitness,
    CollisionTargetKind, CollisionTimeOfImpactResult, CollisionTimeOfImpactWitness,
};
use crate::collision_plan::{
    CollisionArtifactBinding, CollisionArtifactKind, CollisionExecError, CollisionExecutionTrace,
    CollisionPass, CollisionPassKind, CollisionPlan, CollisionReuseDecision, CollisionReuseMetrics,
    CollisionReuseReason, CollisionReuseVerdict, collision_artifact_kind_name,
    collision_history_compatibility_hash, collision_reuse_reason_name,
    collision_reuse_verdict_name,
};
use crate::execution_policy::QueryExecutionPolicy;
use crate::kernel::{KernelStructValue, KernelValue, lower_world_query_plan};
use crate::query_contract::{self, DispatchBackend, QueryContractId};
use crate::query_exec::{
    DirectQueryExecutionTrace, QueryExecContext, execute_world_query_with_policy_with_snapshot_on,
    execute_world_query_with_snapshot_on,
};
use crate::query_plan::{WorldQueryKind, WorldQueryPlan};
use crate::world_identity::WorldSnapshotHandle;
use smol_str::SmolStr;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub enum CollisionArtifactPayload {
    SupportSummary,
    BroadphaseCandidates { candidate_count: u32 },
    WitnessCache(CollisionStoredWitness),
    ContinuationSeed(CollisionContinuationSeed),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CollisionStoredWitness {
    pub hit: bool,
    pub contact_fraction_upper_bound: Option<f32>,
    pub normal_flavor: CollisionContactNormalFlavor,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CollisionContinuationSeed {
    pub fraction_hint: f32,
    pub no_hit_certificate: bool,
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

#[derive(Debug, Clone, Copy, PartialEq)]
struct SweepOutcome {
    hit: bool,
    fraction_upper_bound: Option<f32>,
    point_on_probe: Option<[f32; 3]>,
    point_on_world: Option<[f32; 3]>,
    contact_normal: Option<[f32; 3]>,
    normal_flavor: CollisionContactNormalFlavor,
    no_hit_certificate: Option<CollisionNoHitCertificate>,
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
    let (capture, snapshot) = resolve_region_capture(ctx, args.get(world_index))?;
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
    let mut values = BTreeMap::<SmolStr, CollisionMaterializedValue>::new();

    for pass in &plan.passes {
        match &pass.kind {
            CollisionPassKind::GatherCandidates {
                support_summary_contract,
                support_artifact,
            } => {
                let artifact = artifact_binding_by_id(plan, support_artifact)?;
                let (_support_value, trace) = execute_world_query_contract(
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
                    None,
                    CollisionArtifactPayload::SupportSummary,
                );
            }
            CollisionPassKind::BuildBroadphaseCandidates {
                support_artifact,
                artifact_id,
            } => {
                ensure_current_artifact_available(
                    store,
                    artifact_binding_by_id(plan, support_artifact)?,
                    &snapshot,
                    collision_policy_digest(plan.policy),
                )?;
                let artifact = artifact_binding_by_id(plan, artifact_id)?;
                insert_artifact(
                    store,
                    artifact,
                    &snapshot,
                    collision_policy_digest(plan.policy),
                    None,
                    CollisionArtifactPayload::BroadphaseCandidates { candidate_count: 1 },
                );
            }
            CollisionPassKind::EvaluatePointOccupancy {
                distance_contract,
                normal_contract,
                support_artifact,
            } => {
                ensure_current_artifact_available(
                    store,
                    artifact_binding_by_id(plan, support_artifact)?,
                    &snapshot,
                    collision_policy_digest(plan.policy),
                )?;
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
                        },
                    }),
                    &mut values,
                )?;
            }
            CollisionPassKind::CastRayFirstHit {
                trace_contract,
                support_artifact,
            } => {
                ensure_current_artifact_available(
                    store,
                    artifact_binding_by_id(plan, support_artifact)?,
                    &snapshot,
                    collision_policy_digest(plan.policy),
                )?;
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
                let value = if hit_flag {
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
                ..
            } => {
                ensure_current_artifact_available(
                    store,
                    artifact_binding_by_id(plan, support_artifact)?,
                    &snapshot,
                    collision_policy_digest(plan.policy),
                )?;
                let probe = collision_sphere_input(collision_input)?;
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
                )?;
                ensure_current_artifact_available(
                    store,
                    artifact_binding_by_id(plan, broadphase_artifact)?,
                    &snapshot,
                    collision_policy_digest(plan.policy),
                )?;
                let (seed_fraction, normal_flavor) = load_transition_reuse(
                    plan,
                    store,
                    &snapshot,
                    transition,
                    witness_artifact,
                    continuation_artifact,
                    &mut reuse_metrics,
                    &mut reuse_decisions,
                )?;
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
                    seed_fraction,
                    normal_flavor,
                    &mut executed_query_contracts,
                )?;
                store_transition_artifacts(
                    plan,
                    store,
                    &snapshot,
                    witness_artifact,
                    continuation_artifact,
                    outcome,
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
                )?;
                ensure_current_artifact_available(
                    store,
                    artifact_binding_by_id(plan, broadphase_artifact)?,
                    &snapshot,
                    collision_policy_digest(plan.policy),
                )?;
                let (seed_fraction, normal_flavor) = load_transition_reuse(
                    plan,
                    store,
                    &snapshot,
                    transition,
                    witness_artifact,
                    continuation_artifact,
                    &mut reuse_metrics,
                    &mut reuse_decisions,
                )?;
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
                    seed_fraction,
                    normal_flavor,
                    &mut executed_query_contracts,
                )?;
                store_transition_artifacts(
                    plan,
                    store,
                    &snapshot,
                    witness_artifact,
                    continuation_artifact,
                    outcome,
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

fn insert_artifact(
    store: &mut CollisionArtifactStore,
    artifact: &CollisionArtifactBinding,
    snapshot: &WorldSnapshotHandle,
    policy_digest: u64,
    history_compatibility_hash: Option<u64>,
    payload: CollisionArtifactPayload,
) {
    let logical_schema = artifact.contract.logical_schema.describe();
    let reuse_key = ArtifactReuseKey::new(
        snapshot,
        Some(artifact.id.clone()),
        logical_schema,
        artifact.contract.logical_schema.stable_hash(),
        Some(policy_digest),
        artifact.contract.compatibility.policy.mode,
    );
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
) -> Result<(), CollisionExecError> {
    let request = ArtifactLookupRequest {
        contract: artifact.contract.clone(),
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

fn load_transition_reuse(
    plan: &CollisionPlan,
    store: &CollisionArtifactStore,
    snapshot: &WorldSnapshotHandle,
    transition: Option<CollisionSnapshotTransitionInput>,
    witness_artifact: &SmolStr,
    continuation_artifact: &SmolStr,
    metrics: &mut CollisionReuseMetrics,
    decisions: &mut Vec<CollisionReuseDecision>,
) -> Result<(Option<f32>, CollisionContactNormalFlavor), CollisionExecError> {
    let mut seed_fraction = None;
    let mut normal_flavor = CollisionContactNormalFlavor::ConservativeUpperBound;
    for artifact_id in [witness_artifact, continuation_artifact] {
        let artifact = artifact_binding_by_id(plan, artifact_id)?;
        let decision = if let Some(transition) = transition {
            let history_hash = Some(collision_history_compatibility_hash(
                plan.contract_id,
                artifact.kind,
                None,
            ));
            let request = ArtifactLookupRequest {
                contract: artifact.contract.clone(),
                current_snapshot: snapshot.clone(),
                previous_snapshot_epoch: Some(crate::world_identity::SnapshotEpoch(u64::from(
                    transition.previous_snapshot_epoch,
                ))),
                change_class: Some(transition.change_class),
                policy_digest: Some(collision_policy_digest(plan.policy)),
                presentation_frame: None,
                layout_signature: None,
                history_compatibility_hash: history_hash,
                evidence_summary: Some(artifact.contract.evidence_summary.clone()),
            };
            let (candidate, report) = store.lookup(&request);
            let (verdict, reason) = if candidate.is_some() {
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
            if let Some(candidate) = candidate {
                match &candidate.payload {
                    CollisionArtifactPayload::WitnessCache(payload) => {
                        seed_fraction = payload.contact_fraction_upper_bound;
                        normal_flavor = payload.normal_flavor;
                    }
                    CollisionArtifactPayload::ContinuationSeed(payload) => {
                        seed_fraction = Some(payload.fraction_hint);
                    }
                    CollisionArtifactPayload::SupportSummary
                    | CollisionArtifactPayload::BroadphaseCandidates { .. } => {}
                }
            }
            CollisionReuseDecision {
                artifact_id: artifact.id.clone(),
                artifact_kind: artifact.kind,
                verdict,
                reason,
                detail: report
                    .primary_rejection_reason()
                    .unwrap_or_else(|| SmolStr::new("accepted")),
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
    Ok((seed_fraction, normal_flavor))
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
    let history_hash_witness = Some(collision_history_compatibility_hash(
        plan.contract_id,
        CollisionArtifactKind::WitnessCache,
        None,
    ));
    let history_hash_seed = Some(collision_history_compatibility_hash(
        plan.contract_id,
        CollisionArtifactKind::ContinuationSeed,
        None,
    ));
    insert_artifact(
        store,
        artifact_binding_by_id(plan, witness_artifact)?,
        snapshot,
        collision_policy_digest(plan.policy),
        history_hash_witness,
        CollisionArtifactPayload::WitnessCache(CollisionStoredWitness {
            hit: outcome.hit,
            contact_fraction_upper_bound: outcome.fraction_upper_bound,
            normal_flavor: outcome.normal_flavor,
        }),
    );
    insert_artifact(
        store,
        artifact_binding_by_id(plan, continuation_artifact)?,
        snapshot,
        collision_policy_digest(plan.policy),
        history_hash_seed,
        CollisionArtifactPayload::ContinuationSeed(CollisionContinuationSeed {
            fraction_hint: outcome.fraction_upper_bound.unwrap_or(1.0),
            no_hit_certificate: outcome.no_hit_certificate.is_some(),
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
    seed_fraction: Option<f32>,
    seed_normal_flavor: CollisionContactNormalFlavor,
    executed_query_contracts: &mut Vec<QueryContractId>,
) -> Result<SweepOutcome, CollisionExecError> {
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
            sweep.start_center,
            sweep.radius,
            distance_contract,
            normal_contract,
            seed_normal_flavor,
            plan_no_hit_guarantee(policy),
            sweep.contact_tolerance,
            executed_query_contracts,
        );
    }
    let mut fraction = seed_fraction.unwrap_or(0.0).clamp(0.0, 1.0);
    let mut iterations = 0;
    while iterations < sweep.max_iterations.max(1) {
        let center = lerp(sweep.start_center, sweep.end_center, fraction);
        let (distance_value, distance_trace) = execute_point_query(
            ctx,
            backend,
            policy,
            snapshot,
            capture,
            domain,
            center,
            distance_contract,
        )?;
        executed_query_contracts.push(distance_trace.contract_id);
        let separation = expect_f32(&distance_value)? - sweep.radius;
        if separation <= sweep.contact_tolerance {
            let (normal_value, normal_trace) = execute_point_query(
                ctx,
                backend,
                policy,
                snapshot,
                capture,
                domain,
                center,
                normal_contract,
            )?;
            executed_query_contracts.push(normal_trace.contract_id);
            let world_normal = expect_vec3(&normal_value)?;
            let point_on_probe = offset_point(center, world_normal, -sweep.radius);
            let point_on_world = offset_point(center, world_normal, -(separation + sweep.radius));
            return Ok(SweepOutcome {
                hit: true,
                fraction_upper_bound: Some(fraction),
                point_on_probe: Some(point_on_probe),
                point_on_world: Some(point_on_world),
                contact_normal: Some(world_normal),
                normal_flavor: CollisionContactNormalFlavor::ConservativeUpperBound,
                no_hit_certificate: None,
            });
        }
        let remaining = length * (1.0 - fraction);
        if separation >= remaining {
            return Ok(SweepOutcome {
                hit: false,
                fraction_upper_bound: None,
                point_on_probe: None,
                point_on_world: None,
                contact_normal: None,
                normal_flavor: seed_normal_flavor,
                no_hit_certificate: Some(CollisionNoHitCertificate {
                    valid_through_fraction: 1.0,
                    guarantee: plan_no_hit_guarantee(policy),
                }),
            });
        }
        let step_fraction = (separation.max(sweep.contact_tolerance) / length).max(0.0005);
        let next_fraction = (fraction + step_fraction).min(1.0);
        if next_fraction <= fraction + f32::EPSILON {
            break;
        }
        fraction = next_fraction;
        iterations += 1;
    }
    sphere_overlap_like_outcome(
        ctx,
        backend,
        policy,
        snapshot,
        capture,
        domain,
        sweep.end_center,
        sweep.radius,
        distance_contract,
        normal_contract,
        seed_normal_flavor,
        plan_no_hit_guarantee(policy),
        sweep.contact_tolerance,
        executed_query_contracts,
    )
}

#[allow(clippy::too_many_arguments)]
fn sphere_overlap_like_outcome(
    ctx: &QueryExecContext,
    backend: DispatchBackend,
    policy: &QueryExecutionPolicy,
    snapshot: &WorldSnapshotHandle,
    capture: &KernelValue,
    domain: &KernelValue,
    center: [f32; 3],
    radius: f32,
    distance_contract: QueryContractId,
    normal_contract: QueryContractId,
    normal_flavor: CollisionContactNormalFlavor,
    no_hit_guarantee: crate::execution_policy::RequiredGuaranteeClass,
    tolerance: f32,
    executed_query_contracts: &mut Vec<QueryContractId>,
) -> Result<SweepOutcome, CollisionExecError> {
    let (distance_value, distance_trace) = execute_point_query(
        ctx,
        backend,
        policy,
        snapshot,
        capture,
        domain,
        center,
        distance_contract,
    )?;
    executed_query_contracts.push(distance_trace.contract_id);
    let separation = expect_f32(&distance_value)? - radius;
    if separation <= tolerance {
        let (normal_value, normal_trace) = execute_point_query(
            ctx,
            backend,
            policy,
            snapshot,
            capture,
            domain,
            center,
            normal_contract,
        )?;
        executed_query_contracts.push(normal_trace.contract_id);
        let world_normal = expect_vec3(&normal_value)?;
        Ok(SweepOutcome {
            hit: true,
            fraction_upper_bound: Some(1.0),
            point_on_probe: Some(offset_point(center, world_normal, -radius)),
            point_on_world: Some(offset_point(center, world_normal, -(separation + radius))),
            contact_normal: Some(world_normal),
            normal_flavor,
            no_hit_certificate: None,
        })
    } else {
        Ok(SweepOutcome {
            hit: false,
            fraction_upper_bound: None,
            point_on_probe: None,
            point_on_world: None,
            contact_normal: None,
            normal_flavor,
            no_hit_certificate: Some(CollisionNoHitCertificate {
                valid_through_fraction: 1.0,
                guarantee: no_hit_guarantee,
            }),
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
