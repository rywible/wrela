use super::*;

#[test]
fn static_collision_paths_execute_on_wgsl_and_use_world_queries() {
    let ctx = typed_query_module(collision_fixture_source());
    let scene_id = stable_region_scene_capture_id(&SmolStr::new("collision_region"));
    let domain = scene_domain(scene_id);
    let capture = region_capture(scene_id, 1);

    for (kind, args, expected_kind) in [
        (
            CollisionQueryKind::PointOccupancyWorld,
            vec![
                capture.clone(),
                domain.clone(),
                collision_point_input([0.0, 0.0, 0.25]),
            ],
            "occupancy",
        ),
        (
            CollisionQueryKind::RayCastWorld,
            vec![
                capture.clone(),
                domain.clone(),
                collision_ray_input([0.0, 0.0, 2.0], [0.0, 0.0, -1.0]),
            ],
            "ray",
        ),
        (
            CollisionQueryKind::SphereOverlapWorld,
            vec![
                capture.clone(),
                domain.clone(),
                collision_sphere_probe([0.0, 0.0, 0.9], 0.6),
            ],
            "overlap",
        ),
    ] {
        let plan = CollisionPlan::for_query_with_backend(
            kind,
            wrela::query_contract::DispatchBackend::Wgsl,
        );
        assert!(
            plan.validate().is_empty(),
            "expected Wgsl {expected_kind} plan to validate cleanly: {:?}",
            plan.validate()
        );
        let (result, trace) = plan.execute(&ctx, &args).expect("wgsl collision execution");
        assert_eq!(trace.backend, wrela::query_contract::DispatchBackend::Wgsl);
        let wgsl_metrics = trace.wgsl_metrics.as_ref().expect("wgsl collision metrics");
        assert!(wgsl_metrics.dispatch_count > 0);
        assert!(wgsl_metrics.dispatch_items > 0);
        assert!(wgsl_metrics.selected_workgroup_size > 0);
        assert_eq!(wgsl_metrics.cpu_certification_query_count, 0);
        match (kind, result) {
            (CollisionQueryKind::PointOccupancyWorld, CollisionResult::Occupancy(value)) => {
                assert!(value.occupied);
                assert!(value.signed_distance < 0.0);
                assert!(
                    trace
                        .executed_query_contracts
                        .contains(&wrela::query_contract::SPATIAL_DISTANCE_BATCH_WORLD),
                    "expected Wgsl occupancy to use the batch world distance contract: {trace:?}"
                );
                assert!(
                    trace
                        .executed_query_contracts
                        .contains(&wrela::query_contract::SPATIAL_NORMAL_BATCH_WORLD),
                    "expected Wgsl occupancy to use the batch world normal contract: {trace:?}"
                );
                assert!(
                    !trace
                        .executed_query_contracts
                        .contains(&wrela::query_contract::SPATIAL_DISTANCE_CAPTURE_SHAPE),
                    "expected Wgsl occupancy to avoid the capture distance contract: {trace:?}"
                );
                assert!(
                    !trace
                        .executed_query_contracts
                        .contains(&wrela::query_contract::SPATIAL_DISTANCE_WORLD),
                    "expected Wgsl occupancy to avoid the direct world distance contract: {trace:?}"
                );
            }
            (CollisionQueryKind::RayCastWorld, CollisionResult::RayCast(value)) => {
                assert!(value.hit);
                let witness = value.witness.expect("ray witness");
                assert!(witness.travel_distance > 1.0 && witness.travel_distance < 2.0);
                assert!(
                    trace
                        .executed_query_contracts
                        .contains(&wrela::query_contract::SPATIAL_NEAREST_BATCH_WORLD),
                    "expected Wgsl ray casting to use the batch world trace contract: {trace:?}"
                );
                assert!(
                    !trace
                        .executed_query_contracts
                        .contains(&wrela::query_contract::SPATIAL_TRACE_CAPTURE_SHAPE),
                    "expected Wgsl ray casting to avoid the capture trace contract: {trace:?}"
                );
                assert!(
                    !trace
                        .executed_query_contracts
                        .contains(&wrela::query_contract::SPATIAL_NEAREST_WORLD),
                    "expected Wgsl ray casting to avoid the direct world trace contract: {trace:?}"
                );
            }
            (CollisionQueryKind::SphereOverlapWorld, CollisionResult::SphereOverlap(value)) => {
                assert!(value.overlaps);
                assert!(value.signed_separation < 0.0);
                assert!(
                    trace
                        .executed_query_contracts
                        .contains(&wrela::query_contract::SPATIAL_DISTANCE_BATCH_WORLD),
                    "expected Wgsl overlap to use the batch world distance contract: {trace:?}"
                );
                assert!(
                    trace
                        .executed_query_contracts
                        .contains(&wrela::query_contract::SPATIAL_NORMAL_BATCH_WORLD),
                    "expected Wgsl overlap to use the batch world normal contract: {trace:?}"
                );
                assert!(
                    !trace
                        .executed_query_contracts
                        .contains(&wrela::query_contract::SPATIAL_DISTANCE_CAPTURE_SHAPE),
                    "expected Wgsl overlap to avoid the capture distance contract: {trace:?}"
                );
                assert!(
                    !trace
                        .executed_query_contracts
                        .contains(&wrela::query_contract::SPATIAL_DISTANCE_WORLD),
                    "expected Wgsl overlap to avoid the direct world distance contract: {trace:?}"
                );
            }
            other => panic!("unexpected collision result for {kind:?}: {other:?}"),
        }
    }
}

#[test]
fn wgsl_collision_trace_reports_candidate_reduction_effectiveness() {
    let ctx = typed_query_module(collision_clutter_fixture_source());
    let scene_id = stable_region_scene_capture_id(&SmolStr::new("collision_clutter_region"));
    let plan = CollisionPlan::for_query_with_backend(
        CollisionQueryKind::PointOccupancyWorld,
        wrela::query_contract::DispatchBackend::Wgsl,
    );
    let (_, trace) = plan
        .execute(
            &ctx,
            &[
                region_capture(scene_id, 1),
                scene_domain(scene_id),
                collision_point_input([0.0, 0.0, 0.25]),
            ],
        )
        .expect("wgsl clutter occupancy");
    let wgsl_metrics = trace.wgsl_metrics.as_ref().expect("wgsl metrics");
    assert_eq!(trace.broadphase_candidate_count, 1);
    assert!(trace.broadphase_rejected_candidate_count >= 2);
    assert!(wgsl_metrics.candidate_reduction_effectiveness > 0.5);
}

#[test]
fn transition_collision_wgsl_uses_gpu_bracket_and_cpu_certification() {
    let ctx = typed_query_module(collision_fixture_source());
    let scene_id = stable_region_scene_capture_id(&SmolStr::new("collision_region"));
    let args = vec![
        region_capture(scene_id, 1),
        scene_domain(scene_id),
        collision_transition_input(1, 0, ChangeClass::Presentation),
        collision_sweep_input([0.0, 0.0, 2.0], [0.0, 0.0, -2.0], 0.25),
    ];

    for kind in [
        CollisionQueryKind::SphereSweepTransition,
        CollisionQueryKind::SphereTimeOfImpactTransition,
    ] {
        let cpu_plan = CollisionPlan::for_query(kind);
        let wgsl_plan = CollisionPlan::for_query_with_backend(
            kind,
            wrela::query_contract::DispatchBackend::Wgsl,
        );
        let (cpu_result, _cpu_trace) = cpu_plan
            .execute(&ctx, &args)
            .expect("cpu transition collision");
        let (wgsl_result, wgsl_trace) = wgsl_plan
            .execute(&ctx, &args)
            .expect("wgsl transition collision");
        let wgsl_metrics = wgsl_trace
            .wgsl_metrics
            .as_ref()
            .expect("wgsl transition metrics");
        assert!(wgsl_metrics.dispatch_count > 0);
        assert!(wgsl_metrics.dispatch_items >= 4);
        assert!(wgsl_metrics.cpu_certification_query_count > 0);
        assert!(
            wgsl_trace
                .executed_query_contracts
                .contains(&wrela::query_contract::SPATIAL_DISTANCE_BATCH_WORLD)
        );
        assert!(
            wgsl_trace
                .executed_query_contracts
                .contains(&wrela::query_contract::SPATIAL_DISTANCE_CAPTURE_SHAPE)
        );
        match (cpu_result, wgsl_result) {
            (CollisionResult::Sweep(cpu), CollisionResult::Sweep(wgsl)) => {
                assert_eq!(cpu.hit, wgsl.hit);
                assert_approx_eq(
                    cpu.witness
                        .as_ref()
                        .expect("cpu sweep witness")
                        .contact_fraction_upper_bound,
                    wgsl.witness
                        .as_ref()
                        .expect("wgsl sweep witness")
                        .contact_fraction_upper_bound,
                );
            }
            (CollisionResult::TimeOfImpact(cpu), CollisionResult::TimeOfImpact(wgsl)) => {
                assert_eq!(cpu.hit, wgsl.hit);
                assert_approx_eq(
                    cpu.time_fraction_upper_bound.expect("cpu toi"),
                    wgsl.time_fraction_upper_bound.expect("wgsl toi"),
                );
            }
            other => panic!("unexpected transition result pairing for {kind:?}: {other:?}"),
        }
        assert!(wgsl_trace.interval_subdivisions > 0);
    }
}

#[test]
fn transition_collision_wgsl_with_store_accepts_later_snapshot_epochs() {
    let ctx = typed_query_module(collision_fixture_source());
    let scene_id = stable_region_scene_capture_id(&SmolStr::new("collision_region"));
    let domain = scene_domain(scene_id);
    let sweep = collision_sweep_input([0.0, 0.0, 2.0], [0.0, 0.0, -2.0], 0.25);

    for kind in [
        CollisionQueryKind::SphereSweepTransition,
        CollisionQueryKind::SphereTimeOfImpactTransition,
    ] {
        let plan = CollisionPlan::for_query_with_backend(
            kind,
            wrela::query_contract::DispatchBackend::Wgsl,
        );
        let mut store = CollisionArtifactStore::default();

        let (_, first_trace) = execute_with_store(
            &plan,
            &ctx,
            &[
                region_capture(scene_id, 1),
                domain.clone(),
                collision_transition_input(1, 0, ChangeClass::Presentation),
                sweep.clone(),
            ],
            &mut store,
        )
        .expect("initial wgsl transition query");
        assert!(first_trace.wgsl_metrics.is_some());

        let (_, second_trace) = execute_with_store(
            &plan,
            &ctx,
            &[
                region_capture(scene_id, 2),
                domain.clone(),
                collision_transition_input(2, 1, ChangeClass::Presentation),
                sweep.clone(),
            ],
            &mut store,
        )
        .expect("follow-up wgsl transition query");
        assert!(second_trace.wgsl_metrics.is_some());
    }
}

#[test]
fn transition_collision_reuse_decisions_report_consumed_and_rejected_paths() {
    let ctx = typed_query_module(collision_fixture_source());
    let scene_id = stable_region_scene_capture_id(&SmolStr::new("collision_region"));
    let domain = scene_domain(scene_id);
    let sweep = collision_sweep_input([0.0, 0.0, 2.0], [0.0, 0.0, -2.0], 0.25);
    let plan = CollisionPlan::for_query(CollisionQueryKind::SphereSweepTransition);
    let mut store = CollisionArtifactStore::default();

    let (_, first_trace) = execute_with_store(
        &plan,
        &ctx,
        &[
            region_capture(scene_id, 1),
            domain.clone(),
            collision_transition_input(1, 0, ChangeClass::Presentation),
            sweep.clone(),
        ],
        &mut store,
    )
    .expect("first sweep");
    assert_eq!(first_trace.reuse_metrics.unavailable_count, 2);
    let current_snapshot =
        wrela::query_exec::stable_region_snapshot_handle(&SmolStr::new("collision_region"))
            .with_epoch(SnapshotEpoch(2));
    for (kind, artifact_id, normal_flavor) in [
        (
            CollisionArtifactKind::WitnessCache,
            "artifact.witness_cache.sphere_sweep",
            CollisionContactNormalFlavor::SurfaceGradient,
        ),
        (
            CollisionArtifactKind::ContinuationSeed,
            "artifact.continuation_seed.sphere_sweep",
            CollisionContactNormalFlavor::SurfaceGradient,
        ),
    ] {
        let artifact = plan
            .artifacts
            .iter()
            .find(|artifact| artifact.id == artifact_id)
            .expect("transition artifact");
        let (stored, report) = store.lookup(&ArtifactLookupRequest {
            contract: artifact.contract.clone(),
            reuse_key: None,
            current_snapshot: current_snapshot.clone(),
            previous_snapshot_epoch: Some(SnapshotEpoch(1)),
            change_class: Some(ChangeClass::Presentation),
            policy_digest: Some(policy_digest(plan.policy)),
            presentation_frame: None,
            layout_signature: None,
            history_compatibility_hash: Some(collision_history_compatibility_hash(
                plan.contract_id,
                kind,
                Some(normal_flavor),
            )),
            evidence_summary: Some(artifact.contract.evidence_summary.clone()),
        });
        assert!(
            stored.is_some(),
            "expected transition artifact with history hash to be reusable: {report:?}"
        );
    }

    let (_, second_trace) = execute_with_store(
        &plan,
        &ctx,
        &[
            region_capture(scene_id, 2),
            domain.clone(),
            collision_transition_input(2, 1, ChangeClass::Presentation),
            sweep.clone(),
        ],
        &mut store,
    )
    .expect("second sweep");
    assert!(second_trace.reuse_metrics.consumed_count >= 1);
    assert!(second_trace.reuse_metrics.diagnostics.iter().any(|entry| {
        entry.contains("artifact=artifact.witness_cache.sphere_sweep")
            && entry.contains("verdict=consumed")
    }));

    let (_, third_trace) = execute_with_store(
        &plan,
        &ctx,
        &[
            region_capture(scene_id, 3),
            domain,
            collision_transition_input(3, 1, ChangeClass::Presentation),
            sweep,
        ],
        &mut store,
    )
    .expect("third sweep");
    assert!(third_trace.reuse_metrics.rejected_count >= 1);
    assert!(third_trace.broadphase_candidate_count > 0);
    assert!(third_trace.interval_subdivisions > 0);
    assert!(third_trace.reuse_metrics.diagnostics.iter().any(|entry| {
        entry.contains("verdict=rejected") && entry.contains("reason=validity_rejected")
    }));
}

#[test]
fn transition_collision_reuse_reduces_followup_refinement_work() {
    let ctx = typed_query_module(collision_fixture_source());
    let scene_id = stable_region_scene_capture_id(&SmolStr::new("collision_region"));
    let domain = scene_domain(scene_id);
    let sweep = collision_sweep_input([0.0, 0.0, 2.0], [0.0, 0.0, -2.0], 0.25);
    let plan = CollisionPlan::for_query(CollisionQueryKind::SphereSweepTransition);
    let transition = collision_transition_input(2, 1, ChangeClass::Presentation);

    let (_, baseline_trace) = execute_with_store(
        &plan,
        &ctx,
        &[
            region_capture(scene_id, 2),
            domain.clone(),
            transition.clone(),
            sweep.clone(),
        ],
        &mut CollisionArtifactStore::default(),
    )
    .expect("baseline sweep without reuse");

    let mut store = CollisionArtifactStore::default();
    execute_with_store(
        &plan,
        &ctx,
        &[
            region_capture(scene_id, 1),
            domain.clone(),
            collision_transition_input(1, 0, ChangeClass::Presentation),
            sweep.clone(),
        ],
        &mut store,
    )
    .expect("seed sweep");
    let (_, reused_trace) = execute_with_store(
        &plan,
        &ctx,
        &[region_capture(scene_id, 2), domain, transition, sweep],
        &mut store,
    )
    .expect("followup sweep with reuse");

    assert!(reused_trace.reuse_metrics.consumed_count >= 1);
    assert!(reused_trace.interval_subdivisions <= baseline_trace.interval_subdivisions);
    assert!(reused_trace.interval_refinements <= baseline_trace.interval_refinements);
    assert!(
        reused_trace.interval_subdivisions < baseline_trace.interval_subdivisions
            || reused_trace.interval_refinements < baseline_trace.interval_refinements,
        "expected continuation reuse to reduce interval work: baseline={baseline_trace:?} reused={reused_trace:?}"
    );
}

#[test]
fn transition_collision_rejects_rendering_only_certificates_for_reuse() {
    let ctx = typed_query_module(collision_fixture_source());
    let scene_id = stable_region_scene_capture_id(&SmolStr::new("collision_region"));
    let domain = scene_domain(scene_id);
    let sweep = collision_sweep_input([0.0, 0.0, 2.0], [0.0, 0.0, -2.0], 0.25);
    let plan = CollisionPlan::for_query(CollisionQueryKind::SphereSweepTransition);
    let mut store = CollisionArtifactStore::default();
    let previous_snapshot =
        wrela::query_exec::stable_region_snapshot_handle(&SmolStr::new("collision_region"))
            .with_epoch(SnapshotEpoch(1));
    let policy_digest = policy_digest(plan.policy);
    let witness = plan
        .artifacts
        .iter()
        .find(|artifact| artifact.kind == CollisionArtifactKind::WitnessCache)
        .expect("witness artifact");
    let continuation = plan
        .artifacts
        .iter()
        .find(|artifact| artifact.kind == CollisionArtifactKind::ContinuationSeed)
        .expect("continuation artifact");
    let rendering_only = transition_certificate(CertificateReuseClass::RenderingOnly);

    for artifact in [witness, continuation] {
        store.insert(StoredArtifact {
            contract: artifact.contract.clone(),
            metadata: ArtifactInstanceMetadata {
                snapshot: previous_snapshot.clone(),
                reuse_key: ArtifactReuseKey::new(
                    &previous_snapshot,
                    Some(artifact.id.clone()),
                    artifact.contract.logical_schema.describe(),
                    artifact.contract.logical_schema.stable_hash(),
                    Some(policy_digest),
                    artifact.contract.compatibility.policy.mode,
                ),
                policy_digest: Some(policy_digest),
                presentation_frame: None,
                layout_signature: None,
                history_compatibility_hash: None,
                evidence_summary: artifact.contract.evidence_summary.clone(),
            },
            payload: if artifact.kind == CollisionArtifactKind::WitnessCache {
                CollisionArtifactPayload::WitnessCache(CollisionStoredWitness {
                    hit: true,
                    contact_fraction_upper_bound: Some(0.3125),
                    separation_upper_bound: Some(-0.2),
                    normal_provenance: Some(
                        CollisionContactNormalProvenance::CertifiedFieldGradient,
                    ),
                    normal_flavor: CollisionContactNormalFlavor::SurfaceGradient,
                    certificate: rendering_only.clone(),
                })
            } else {
                CollisionArtifactPayload::ContinuationSeed(CollisionContinuationSeed {
                    fraction_hint: 0.3125,
                    no_hit_certificate: true,
                    separation_upper_bound: Some(-0.2),
                    normal_provenance: Some(
                        CollisionContactNormalProvenance::CertifiedFieldGradient,
                    ),
                    normal_flavor: CollisionContactNormalFlavor::SurfaceGradient,
                    certificate: rendering_only.clone(),
                })
            },
        });
    }

    let (_, trace) = execute_with_store(
        &plan,
        &ctx,
        &[
            region_capture(scene_id, 2),
            domain,
            wrela::kernel::KernelValue::Struct(KernelStructValue {
                name: SmolStr::new("CollisionSnapshotTransitionInput"),
                fields: vec![
                    (SmolStr::new("current_snapshot_epoch"), KernelValue::U32(2)),
                    (SmolStr::new("previous_snapshot_epoch"), KernelValue::U32(1)),
                    (SmolStr::new("change_class"), KernelValue::U32(1)),
                ],
            }),
            sweep,
        ],
        &mut store,
    )
    .expect("rendering-only sweep");
    assert!(trace.reuse_metrics.rejected_count >= 1);
    assert!(
        trace
            .reuse_metrics
            .diagnostics
            .iter()
            .any(|entry| entry.contains("reason=rendering_only_certificate"))
    );
}
