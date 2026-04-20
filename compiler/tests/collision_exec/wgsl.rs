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
fn point_occupancy_batch_metrics_only_avoids_per_query_dispatch_and_readback() {
    let ctx = typed_query_module(collision_fixture_source());
    let scene_id = stable_region_scene_capture_id(&SmolStr::new("collision_region"));
    let plan = CollisionPlan::for_query_with_backend(
        CollisionQueryKind::PointOccupancyWorld,
        wrela::query_contract::DispatchBackend::Wgsl,
    );
    let batch = wrela::collision_exec::CollisionWorkloadBatch::new(
        "point occupancy metrics batch",
        "collision_perf_point_occupancy_batch",
        "collision_perf_point_occupancy_batch",
        plan,
        wrela::collision_contract::COLLISION_POINT_OCCUPANCY_WORLD,
        "snapshot:collision:point_metrics",
        region_capture(scene_id, 1),
        scene_domain(scene_id),
        wrela::collision_exec::CollisionCandidateGroupingPolicy::SharedCandidateDigest,
        wrela::collision_exec::CollisionCertificationPolicy::MetricsOnly,
        (0..32)
            .map(
                |index| wrela::collision_exec::CollisionBatchItem::PointOccupancy {
                    point: [index as f32 * 0.02 - 0.32, 0.0, 0.25],
                },
            )
            .collect(),
        16,
    )
    .checked()
    .expect("valid point occupancy metrics batch");

    let report =
        wrela::collision_exec::execute_batch_metrics_only(&batch, &ctx).expect("wgsl point batch");
    assert_eq!(report.query_count, 32);
    assert!(report.dispatch_count > 0);
    assert!(report.dispatch_count < report.query_count as u32);
    assert!(report.average_items_per_dispatch > 1.0);
    assert_eq!(report.hot_path_readback_bytes, 0);
    assert!(report.queue_submit_count > 0);
    assert_eq!(report.cpu_certification_query_count, 0);
}

#[test]
fn ray_cast_batch_metrics_only_uses_chunked_wgsl_dispatches() {
    let ctx = typed_query_module(collision_fixture_source());
    let scene_id = stable_region_scene_capture_id(&SmolStr::new("collision_region"));
    let plan = CollisionPlan::for_query_with_backend(
        CollisionQueryKind::RayCastWorld,
        wrela::query_contract::DispatchBackend::Wgsl,
    );
    let batch = wrela::collision_exec::CollisionWorkloadBatch::new(
        "ray cast metrics batch",
        "collision_perf_dense_ray_casts",
        "collision_perf_dense_ray_casts",
        plan,
        wrela::collision_contract::COLLISION_RAY_CAST_WORLD,
        "snapshot:collision:ray_metrics",
        region_capture(scene_id, 1),
        scene_domain(scene_id),
        wrela::collision_exec::CollisionCandidateGroupingPolicy::SharedCandidateDigest,
        wrela::collision_exec::CollisionCertificationPolicy::MetricsOnly,
        (0..24)
            .map(|index| wrela::collision_exec::CollisionBatchItem::RayCast {
                ray: wrela::collision_contract::CollisionRayInput {
                    origin: [index as f32 * 0.01 - 0.12, 0.0, 2.0],
                    direction: [0.0, 0.0, -1.0],
                    max_distance: 6.0,
                    min_step: 0.05,
                    hit_epsilon: 0.001,
                    max_steps: 96,
                },
            })
            .collect(),
        12,
    )
    .checked()
    .expect("valid ray cast metrics batch");

    let report =
        wrela::collision_exec::execute_batch_metrics_only(&batch, &ctx).expect("wgsl ray batch");
    assert_eq!(report.query_count, 24);
    assert!(report.dispatch_count > 0);
    assert!(report.dispatch_count < report.query_count as u32);
    assert!(report.average_items_per_dispatch > 1.0);
    assert_eq!(report.hot_path_readback_bytes, 0);
    assert!(report.queue_submit_count > 0);
    assert_eq!(report.cpu_certification_query_count, 0);
}

#[test]
fn sphere_overlap_batch_metrics_only_uses_batched_wgsl_distance_dispatches() {
    let ctx = typed_query_module(collision_fixture_source());
    let scene_id = stable_region_scene_capture_id(&SmolStr::new("collision_region"));
    let plan = CollisionPlan::for_query_with_backend(
        CollisionQueryKind::SphereOverlapWorld,
        wrela::query_contract::DispatchBackend::Wgsl,
    );
    let batch = wrela::collision_exec::CollisionWorkloadBatch::new(
        "sphere overlap metrics batch",
        "collision_perf_overlap_burst",
        "collision_perf_overlap_burst",
        plan,
        wrela::collision_contract::COLLISION_SPHERE_OVERLAP_WORLD,
        "snapshot:collision:overlap_metrics",
        region_capture(scene_id, 1),
        scene_domain(scene_id),
        wrela::collision_exec::CollisionCandidateGroupingPolicy::SharedCandidateDigest,
        wrela::collision_exec::CollisionCertificationPolicy::MetricsOnly,
        (0..48)
            .map(
                |index| wrela::collision_exec::CollisionBatchItem::SphereOverlap {
                    center: [index as f32 * 0.02 - 0.48, 0.0, 0.1],
                    radius: 0.20,
                },
            )
            .collect(),
        24,
    )
    .checked()
    .expect("valid sphere overlap metrics batch");

    let report = wrela::collision_exec::execute_batch_metrics_only(&batch, &ctx)
        .expect("wgsl sphere overlap batch");
    assert_eq!(report.query_count, 48);
    assert!(report.dispatch_count > 0);
    assert!(report.dispatch_count < report.query_count as u32);
    assert!(report.average_items_per_dispatch > 1.0);
    assert_eq!(report.hot_path_readback_bytes, 0);
    assert!(report.queue_submit_count > 0);
    assert_eq!(report.cpu_certification_query_count, 0);
}

#[test]
fn transition_batch_metrics_only_batches_gpu_sampling_and_counts_cpu_certification() {
    let ctx = typed_query_module(collision_fixture_source());
    let scene_id = stable_region_scene_capture_id(&SmolStr::new("collision_region"));

    for kind in [
        CollisionQueryKind::SphereSweepTransition,
        CollisionQueryKind::SphereTimeOfImpactTransition,
    ] {
        let plan = CollisionPlan::for_query_with_backend(
            kind,
            wrela::query_contract::DispatchBackend::Wgsl,
        );
        let items = (0..64)
            .map(|index| {
                let start_center = [index as f32 * 0.01 - 0.32, 0.0, 2.0];
                let end_center = [start_center[0], start_center[1], -2.0];
                let transition = wrela::collision_contract::CollisionSnapshotTransitionInput {
                    current_snapshot_epoch: 2,
                    previous_snapshot_epoch: 1,
                    change_class: ChangeClass::Presentation,
                };
                let sweep = wrela::collision_contract::CollisionSphereSweepInput {
                    start_center,
                    end_center,
                    radius: 0.25,
                    contact_tolerance: 0.001,
                    max_iterations: 64,
                };
                match kind {
                    CollisionQueryKind::SphereSweepTransition => {
                        wrela::collision_exec::CollisionBatchItem::SphereSweep { transition, sweep }
                    }
                    CollisionQueryKind::SphereTimeOfImpactTransition => {
                        wrela::collision_exec::CollisionBatchItem::SphereTimeOfImpact {
                            transition,
                            sweep,
                        }
                    }
                    _ => unreachable!("transition-only loop"),
                }
            })
            .collect::<Vec<_>>();
        let batch = wrela::collision_exec::CollisionWorkloadBatch::new(
            format!("transition metrics batch {kind:?}"),
            format!("transition_metrics_{kind:?}"),
            format!("transition_metrics_{kind:?}"),
            plan,
            match kind {
                CollisionQueryKind::SphereSweepTransition => {
                    wrela::collision_contract::COLLISION_SPHERE_SWEEP_TRANSITION
                }
                CollisionQueryKind::SphereTimeOfImpactTransition => {
                    wrela::collision_contract::COLLISION_TIME_OF_IMPACT_TRANSITION
                }
                _ => unreachable!("transition-only loop"),
            },
            "snapshot:collision:transition_metrics",
            region_capture(scene_id, 2),
            scene_domain(scene_id),
            wrela::collision_exec::CollisionCandidateGroupingPolicy::SharedBroadphaseRegion,
            wrela::collision_exec::CollisionCertificationPolicy::MetricsOnly,
            items,
            32,
        )
        .checked()
        .expect("valid transition metrics batch");

        let report = wrela::collision_exec::execute_batch_metrics_only(&batch, &ctx)
            .expect("wgsl transition metrics batch");
        assert_eq!(report.query_count, 64);
        assert!(report.dispatch_count > 0);
        assert!(report.dispatch_count < report.query_count as u32);
        assert!(report.average_items_per_dispatch > 1.0);
        assert_eq!(report.hot_path_readback_bytes, 0);
        assert!(report.queue_submit_count > 0);
        assert!(report.cpu_certification_query_count > 0);
        assert!(report.total_interval_subdivisions > 0);
    }
}

#[test]
fn transition_batch_metrics_only_reuses_identical_transition_items() {
    let ctx = typed_query_module(collision_fixture_source());
    let scene_id = stable_region_scene_capture_id(&SmolStr::new("collision_region"));
    let plan = CollisionPlan::for_query_with_backend(
        CollisionQueryKind::SphereSweepTransition,
        wrela::query_contract::DispatchBackend::Wgsl,
    );
    let transition = wrela::collision_contract::CollisionSnapshotTransitionInput {
        current_snapshot_epoch: 2,
        previous_snapshot_epoch: 1,
        change_class: ChangeClass::Presentation,
    };
    let sweep = wrela::collision_contract::CollisionSphereSweepInput {
        start_center: [0.0, 0.0, 2.0],
        end_center: [0.0, 0.0, -2.0],
        radius: 0.25,
        contact_tolerance: 0.001,
        max_iterations: 64,
    };
    let batch = wrela::collision_exec::CollisionWorkloadBatch::new(
        "transition metrics reuse batch",
        "transition_metrics_reuse",
        "transition_metrics_reuse",
        plan,
        wrela::collision_contract::COLLISION_SPHERE_SWEEP_TRANSITION,
        "snapshot:collision:transition_metrics_reuse",
        region_capture(scene_id, 2),
        scene_domain(scene_id),
        wrela::collision_exec::CollisionCandidateGroupingPolicy::SharedBroadphaseRegion,
        wrela::collision_exec::CollisionCertificationPolicy::MetricsOnly,
        (0..64)
            .map(|_| wrela::collision_exec::CollisionBatchItem::SphereSweep { transition, sweep })
            .collect(),
        64,
    )
    .checked()
    .expect("valid repeated transition metrics batch");

    let report = wrela::collision_exec::execute_batch_metrics_only(&batch, &ctx)
        .expect("wgsl repeated transition metrics batch");
    assert_eq!(report.query_count, 64);
    assert!(report.cpu_certification_query_count > 0);
    assert!(report.cpu_certification_query_count < report.query_count as u32);
    assert!(report.available_count_total > 0);
    assert_eq!(report.available_count_total, report.consumed_count_total);
    assert!(report.witness_reuse_rate > 0.9);
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
