use super::*;
use wrela::collision_exec::execute_batch_cpu;
use wrela::collision_plan::{
    CollisionBatchItem, CollisionCandidateGroupingPolicy, CollisionCandidateTable,
    CollisionCertificationPolicy, CollisionExecutionTrace, CollisionWorkloadBatch,
};

fn collision_cpu_certification_query_count(trace: &CollisionExecutionTrace) -> u32 {
    trace
        .executed_query_contracts
        .iter()
        .filter(|contract| {
            matches!(
                **contract,
                wrela::query_contract::SPATIAL_DISTANCE_CAPTURE_SHAPE
                    | wrela::query_contract::SPATIAL_NORMAL_CAPTURE_SHAPE
                    | wrela::query_contract::SPATIAL_DISTANCE_WORLD
                    | wrela::query_contract::SPATIAL_NORMAL_WORLD
                    | wrela::query_contract::SPATIAL_TRACE_CAPTURE_SHAPE
            )
        })
        .count() as u32
}

#[test]
fn static_and_transition_collision_plans_execute_on_cpu() {
    let ctx = typed_query_module(collision_fixture_source());
    let scene_id = stable_region_scene_capture_id(&SmolStr::new("collision_region"));
    let domain = scene_domain(scene_id);
    let capture = region_capture(scene_id, 2);
    let transition = collision_transition_input(2, 1, ChangeClass::Presentation);
    let sweep = collision_sweep_input([0.0, 0.0, 2.0], [0.0, 0.0, -2.0], 0.25);

    let sweep_plan = CollisionPlan::for_query(CollisionQueryKind::SphereSweepTransition);
    let (result, trace) = sweep_plan
        .execute(
            &ctx,
            &[
                capture.clone(),
                domain.clone(),
                transition.clone(),
                sweep.clone(),
            ],
        )
        .expect("sphere sweep");
    assert_eq!(trace.reuse_metrics.unavailable_count, 2);
    assert!(trace.broadphase_candidate_count > 0);
    assert!(trace.interval_subdivisions > 0);
    assert!(trace.interval_refinements > 0);
    assert!(trace.certificate_successes > 0);
    let sweep_bracket = trace.interval_bracket.expect("sweep interval bracket");
    assert_approx_eq(sweep_bracket[0], 0.3125);
    assert_approx_eq(sweep_bracket[1], 0.3125);
    assert_eq!(trace.fallback_count, 0);
    assert!(
        trace
            .executed_query_contracts
            .contains(&wrela::query_contract::SUPPORT_SUMMARY_WORLD),
        "expected transition sweep trace to execute the support summary query: {trace:?}"
    );
    assert!(
        trace.executed_query_contracts.len() >= 3,
        "expected transition sweep trace to record support, distance, and normal queries: {trace:?}"
    );
    match result {
        wrela::collision_contract::CollisionResult::Sweep(value) => {
            assert!(value.hit);
            let witness = value.witness.expect("witness");
            assert_approx_eq(witness.contact_fraction_upper_bound, 0.3125);
            assert_eq!(
                witness.normal_flavor,
                wrela::collision_contract::CollisionContactNormalFlavor::SurfaceGradient
            );
            assert_eq!(
                witness.normal_provenance,
                CollisionContactNormalProvenance::FeatureNormal
            );
        }
        other => panic!("expected sweep result, got {other:?}"),
    }
    assert_eq!(
        trace.contact_normal_provenance,
        Some(CollisionContactNormalProvenance::FeatureNormal)
    );
    assert_eq!(trace.broadphase_candidate_count, 1);
    assert!(
        trace
            .executed_query_contracts
            .contains(&wrela::query_contract::SPATIAL_DISTANCE_CAPTURE_SHAPE),
        "expected sweep to evaluate the shape-capture distance contract: {trace:?}"
    );

    let toi_plan = CollisionPlan::for_query(CollisionQueryKind::SphereTimeOfImpactTransition);
    let (result, toi_trace) = toi_plan
        .execute(&ctx, &[capture, domain, transition, sweep])
        .expect("time of impact");
    let toi_bracket = toi_trace.interval_bracket.expect("toi interval bracket");
    assert_approx_eq(toi_bracket[0], 0.3125);
    assert_approx_eq(toi_bracket[1], 0.3125);
    assert_eq!(toi_trace.fallback_count, 0);
    assert!(
        toi_trace
            .executed_query_contracts
            .contains(&wrela::query_contract::SPATIAL_DISTANCE_CAPTURE_SHAPE),
        "expected TOI to evaluate the shape-capture distance contract: {toi_trace:?}"
    );
    match result {
        wrela::collision_contract::CollisionResult::TimeOfImpact(value) => {
            assert!(value.hit);
            assert_approx_eq(value.time_fraction_upper_bound.expect("toi"), 0.3125);
            let witness = value.witness.expect("toi witness");
            assert_eq!(
                witness.normal_provenance,
                CollisionContactNormalProvenance::FeatureNormal
            );
        }
        other => panic!("expected time-of-impact result, got {other:?}"),
    }
}

#[test]
fn collision_candidate_table_packs_gpu_candidate_spans() {
    let table = CollisionCandidateTable::from_shared_candidates(
        vec![
            SmolStr::new("shape.a"),
            SmolStr::new("shape.b"),
            SmolStr::new("shape.c"),
        ],
        2,
        8,
        6,
        2,
        1,
    );
    assert!(!table.overflowed);
    assert_eq!(table.item_ranges, vec![(0, 3), (0, 3)]);
    assert_eq!(table.gpu_candidate_spans(), vec![0, 3, 0, 3, 0, 1, 2]);
    assert_eq!(table.average_candidate_count(2), 3);
}

#[test]
fn collision_candidate_table_overflow_falls_back_cleanly() {
    let table = CollisionCandidateTable::from_shared_candidates(
        vec![
            SmolStr::new("shape.a"),
            SmolStr::new("shape.b"),
            SmolStr::new("shape.c"),
        ],
        2,
        2,
        6,
        2,
        1,
    );
    assert!(table.overflowed);
    assert_eq!(table.overflow_fallback_item_count, 2);
    assert!(table.item_ranges.is_empty());
    assert!(table.gpu_candidate_spans().is_empty());
}

#[test]
fn transition_collision_materializes_a_typed_broadphase_payload() {
    let ctx = typed_query_module(collision_fixture_source());
    let scene_id = stable_region_scene_capture_id(&SmolStr::new("collision_region"));
    let domain = scene_domain(scene_id);
    let transition = collision_transition_input(2, 1, ChangeClass::Presentation);
    let sweep = collision_sweep_input([0.0, 0.0, 2.0], [0.0, 0.0, -2.0], 0.25);
    let plan = CollisionPlan::for_query(CollisionQueryKind::SphereSweepTransition);
    let mut store = CollisionArtifactStore::default();

    let (_, trace) = execute_with_store(
        &plan,
        &ctx,
        &[
            region_capture(scene_id, 2),
            domain.clone(),
            transition,
            sweep,
        ],
        &mut store,
    )
    .expect("transition sweep with store");
    assert_eq!(trace.artifact_store.entries, 4);
    assert!(trace.broadphase_candidate_count > 0);
    assert!(trace.interval_subdivisions > 0);
    assert!(trace.interval_refinements > 0);
    assert!(trace.certificate_successes > 0);
    assert_eq!(trace.broadphase_candidate_count, 1);
}

#[test]
fn collision_broadphase_reuse_is_keyed_by_query_input() {
    let ctx = typed_query_module(collision_fixture_source());
    let scene_id = stable_region_scene_capture_id(&SmolStr::new("collision_region"));
    let domain = scene_domain(scene_id);
    let plan = CollisionPlan::for_query(CollisionQueryKind::PointOccupancyWorld);
    let mut store = CollisionArtifactStore::default();

    let (_, first_trace) = execute_with_store(
        &plan,
        &ctx,
        &[
            region_capture(scene_id, 2),
            domain.clone(),
            collision_point_input([0.0, 0.0, 0.25]),
        ],
        &mut store,
    )
    .expect("first point occupancy");
    assert!(
        first_trace
            .executed_query_contracts
            .contains(&wrela::query_contract::SPATIAL_DISTANCE_CAPTURE_SHAPE)
    );
    assert!(
        first_trace
            .executed_query_contracts
            .contains(&wrela::query_contract::SPATIAL_NORMAL_CAPTURE_SHAPE)
    );

    let (result, second_trace) = execute_with_store(
        &plan,
        &ctx,
        &[
            region_capture(scene_id, 2),
            domain,
            collision_point_input([20.0, 0.0, 20.0]),
        ],
        &mut store,
    )
    .expect("second point occupancy");
    assert_eq!(second_trace.broadphase_candidate_count, 0);
    match result {
        CollisionResult::Occupancy(value) => {
            assert!(!value.occupied);
            assert!(value.signed_distance > 0.0);
        }
        other => panic!("expected occupancy result, got {other:?}"),
    }
}

#[test]
fn shared_broadphase_prunes_far_shapes_on_cluttered_collision_scene() {
    let ctx = typed_query_module(collision_clutter_fixture_source());
    let scene_id = stable_region_scene_capture_id(&SmolStr::new("collision_clutter_region"));
    let domain = scene_domain(scene_id);
    let plan = CollisionPlan::for_query(CollisionQueryKind::PointOccupancyWorld);

    let (_, trace) = plan
        .execute(
            &ctx,
            &[
                region_capture(scene_id, 2),
                domain,
                collision_point_input([0.0, 0.0, 0.25]),
            ],
        )
        .expect("cluttered point occupancy");
    assert_eq!(trace.broadphase_candidate_count, 1);
    assert!(
        trace.broadphase_rejected_candidate_count >= 2,
        "expected the shared broadphase to reject the distant clutter shapes: {trace:?}"
    );
}

#[test]
fn collision_artifact_reuse_distinguishes_scene_domain_detail() {
    let ctx = typed_query_module(collision_fixture_source());
    let scene_id = stable_region_scene_capture_id(&SmolStr::new("collision_region"));
    let fine_domain = scene_domain(scene_id);
    let coarse_domain = scene_domain_with_detail(scene_id, 0);
    let plan = CollisionPlan::for_query(CollisionQueryKind::PointOccupancyWorld);
    let mut store = CollisionArtifactStore::default();

    let (_, first_trace) = execute_with_store(
        &plan,
        &ctx,
        &[
            region_capture(scene_id, 2),
            fine_domain.clone(),
            collision_point_input([0.0, 0.0, 0.25]),
        ],
        &mut store,
    )
    .expect("fine point occupancy");
    assert_eq!(first_trace.broadphase_candidate_count, 1);

    let (_, second_trace) = execute_with_store(
        &plan,
        &ctx,
        &[
            region_capture(scene_id, 2),
            coarse_domain,
            collision_point_input([0.0, 0.0, 0.25]),
        ],
        &mut store,
    )
    .expect("coarse point occupancy");
    let support_artifact = plan
        .artifacts
        .iter()
        .find(|artifact| artifact.kind == CollisionArtifactKind::SupportSummary)
        .expect("support artifact");
    let broadphase_artifact = plan
        .artifacts
        .iter()
        .find(|artifact| artifact.kind == CollisionArtifactKind::BroadphaseCandidates)
        .expect("broadphase artifact");
    let support_bucket_count = second_trace
        .artifact_store
        .buckets
        .iter()
        .filter(|bucket| bucket.contract_id == support_artifact.id)
        .count();
    let broadphase_bucket_count = second_trace
        .artifact_store
        .buckets
        .iter()
        .filter(|bucket| bucket.contract_id == broadphase_artifact.id)
        .count();
    assert_eq!(support_bucket_count, 2);
    assert_eq!(broadphase_bucket_count, 2);
}

#[test]
fn collision_batch_cpu_matches_point_occupancy_single_query_baseline() {
    let ctx = typed_query_module(collision_fixture_source());
    let scene_id = stable_region_scene_capture_id(&SmolStr::new("collision_region"));
    let domain = scene_domain(scene_id);
    let plan = CollisionPlan::for_query(CollisionQueryKind::PointOccupancyWorld);
    let batch = CollisionWorkloadBatch::new(
        "point occupancy batch",
        "collision_perf_point_occupancy_batch",
        "collision_perf_point_occupancy_burst",
        plan.clone(),
        plan.contract_id,
        "snapshot:collision:point:2",
        region_capture(scene_id, 2),
        domain,
        CollisionCandidateGroupingPolicy::SharedCandidateDigest,
        CollisionCertificationPolicy::CpuOracleParity,
        vec![
            CollisionBatchItem::PointOccupancy {
                point: [0.0, 0.0, 0.25],
            },
            CollisionBatchItem::PointOccupancy {
                point: [20.0, 0.0, 20.0],
            },
        ],
        2,
    )
    .checked()
    .expect("valid point batch");

    let mut expected_store = CollisionArtifactStore::default();
    let mut expected_results = Vec::new();
    let mut expected_certification_queries = 0u32;
    for item in &batch.items {
        let (result, trace) =
            execute_with_store(&plan, &ctx, &batch.args_for_item(item), &mut expected_store)
                .expect("point occupancy baseline");
        expected_results.push(result);
        expected_certification_queries += collision_cpu_certification_query_count(&trace);
    }

    let batch_result = execute_batch_cpu(&batch, &ctx, None).expect("point batch");
    assert_eq!(batch_result.results, expected_results);
    assert_eq!(batch_result.report.workload, batch.workload_id);
    assert_eq!(batch_result.report.plan_name, batch.plan.name);
    assert_eq!(batch_result.report.contract_id, batch.contract_id.as_str());
    assert_eq!(batch_result.report.query_count, 2);
    assert_eq!(batch_result.report.batch_count, 1);
    assert_eq!(batch_result.report.dispatch_count, 1);
    assert_eq!(batch_result.report.dispatch_items, 2);
    assert_approx_eq(batch_result.report.average_items_per_dispatch, 2.0);
    assert_eq!(batch_result.report.hot_path_readback_bytes, 0);
    assert_eq!(batch_result.report.queue_submit_count, 0);
    assert_eq!(
        batch_result.report.cpu_certification_query_count,
        expected_certification_queries
    );
    assert_eq!(batch_result.report.fallback_count, 0);
    assert_eq!(batch_result.report.witness_reuse_rate, 0.0);
}

#[test]
fn collision_batch_cpu_matches_ray_cast_single_query_baseline() {
    let ctx = typed_query_module(collision_fixture_source());
    let scene_id = stable_region_scene_capture_id(&SmolStr::new("collision_region"));
    let domain = scene_domain(scene_id);
    let plan = CollisionPlan::for_query(CollisionQueryKind::RayCastWorld);
    let batch = CollisionWorkloadBatch::new(
        "ray cast batch",
        "collision_perf_dense_ray_casts",
        "collision_perf_dense_ray_casts",
        plan.clone(),
        plan.contract_id,
        "snapshot:collision:ray:2",
        region_capture(scene_id, 2),
        domain,
        CollisionCandidateGroupingPolicy::SharedCandidateDigest,
        CollisionCertificationPolicy::CpuOracleParity,
        vec![
            CollisionBatchItem::RayCast {
                ray: wrela::collision_contract::CollisionRayInput {
                    origin: [0.0, 0.0, 2.0],
                    direction: [0.0, 0.0, -1.0],
                    max_distance: 6.0,
                    min_step: 0.05,
                    hit_epsilon: 0.001,
                    max_steps: 96,
                },
            },
            CollisionBatchItem::RayCast {
                ray: wrela::collision_contract::CollisionRayInput {
                    origin: [0.2, 0.0, 2.0],
                    direction: [0.0, 0.0, -1.0],
                    max_distance: 6.0,
                    min_step: 0.05,
                    hit_epsilon: 0.001,
                    max_steps: 96,
                },
            },
        ],
        2,
    )
    .checked()
    .expect("valid ray batch");

    let mut expected_store = CollisionArtifactStore::default();
    let mut expected_results = Vec::new();
    let mut expected_certification_queries = 0u32;
    for item in &batch.items {
        let (result, trace) =
            execute_with_store(&plan, &ctx, &batch.args_for_item(item), &mut expected_store)
                .expect("ray cast baseline");
        expected_results.push(result);
        expected_certification_queries += collision_cpu_certification_query_count(&trace);
    }

    let batch_result = execute_batch_cpu(&batch, &ctx, None).expect("ray batch");
    assert_eq!(batch_result.results, expected_results);
    assert_eq!(batch_result.report.query_count, 2);
    assert_eq!(batch_result.report.batch_count, 1);
    assert_eq!(batch_result.report.dispatch_count, 1);
    assert_eq!(batch_result.report.dispatch_items, 2);
    assert_approx_eq(batch_result.report.average_items_per_dispatch, 2.0);
    assert_eq!(
        batch_result.report.cpu_certification_query_count,
        expected_certification_queries
    );
    assert_eq!(batch_result.report.fallback_count, 0);
    assert_eq!(batch_result.report.witness_reuse_rate, 0.0);
}

#[test]
fn collision_batch_cpu_matches_overlap_and_transition_workloads_with_store_reuse() {
    let ctx = typed_query_module(collision_fixture_source());
    let scene_id = stable_region_scene_capture_id(&SmolStr::new("collision_region"));
    let domain = scene_domain(scene_id);
    let overlap_plan = CollisionPlan::for_query(CollisionQueryKind::SphereOverlapWorld);
    let overlap_batch = CollisionWorkloadBatch::new(
        "overlap batch",
        "collision_perf_overlap_burst",
        "collision_perf_overlap_burst",
        overlap_plan.clone(),
        overlap_plan.contract_id,
        "snapshot:collision:overlap:2",
        region_capture(scene_id, 2),
        domain.clone(),
        CollisionCandidateGroupingPolicy::SharedCandidateDigest,
        CollisionCertificationPolicy::CpuOracleParity,
        vec![
            CollisionBatchItem::SphereOverlap {
                center: [0.0, 0.0, 0.9],
                radius: 0.6,
            },
            CollisionBatchItem::SphereOverlap {
                center: [20.0, 0.0, 20.0],
                radius: 0.6,
            },
        ],
        2,
    )
    .checked()
    .expect("valid overlap batch");

    let mut expected_store = CollisionArtifactStore::default();
    let mut expected_results = Vec::new();
    let mut expected_certification_queries = 0u32;
    for item in &overlap_batch.items {
        let (result, trace) = execute_with_store(
            &overlap_plan,
            &ctx,
            &overlap_batch.args_for_item(item),
            &mut expected_store,
        )
        .expect("overlap baseline");
        expected_results.push(result);
        expected_certification_queries += collision_cpu_certification_query_count(&trace);
    }

    let overlap_result =
        execute_batch_cpu(&overlap_batch, &ctx, None).expect("overlap batch execution");
    assert_eq!(overlap_result.results, expected_results);
    assert_eq!(overlap_result.report.query_count, 2);
    assert_eq!(overlap_result.report.dispatch_count, 1);
    assert_eq!(overlap_result.report.dispatch_items, 2);
    assert_eq!(
        overlap_result.report.cpu_certification_query_count,
        expected_certification_queries
    );
    assert_eq!(overlap_result.report.fallback_count, 0);
    assert_eq!(overlap_result.report.witness_reuse_rate, 0.0);

    let transition_plan = CollisionPlan::for_query(CollisionQueryKind::SphereSweepTransition);
    let transition_items = vec![
        CollisionBatchItem::SphereSweep {
            transition: wrela::collision_contract::CollisionSnapshotTransitionInput {
                current_snapshot_epoch: 2,
                previous_snapshot_epoch: 1,
                change_class: ChangeClass::Presentation,
            },
            sweep: wrela::collision_contract::CollisionSphereSweepInput {
                start_center: [0.0, 0.0, 2.0],
                end_center: [0.0, 0.0, -2.0],
                radius: 0.25,
                contact_tolerance: 0.001,
                max_iterations: 64,
            },
        },
        CollisionBatchItem::SphereSweep {
            transition: wrela::collision_contract::CollisionSnapshotTransitionInput {
                current_snapshot_epoch: 2,
                previous_snapshot_epoch: 1,
                change_class: ChangeClass::Presentation,
            },
            sweep: wrela::collision_contract::CollisionSphereSweepInput {
                start_center: [0.0, 0.0, 2.0],
                end_center: [0.0, 0.0, -2.0],
                radius: 0.25,
                contact_tolerance: 0.001,
                max_iterations: 64,
            },
        },
    ];
    let transition_batch = CollisionWorkloadBatch::new(
        "repeated sweep batch",
        "collision_perf_repeated_sweeps",
        "collision_perf_repeated_sweeps",
        transition_plan.clone(),
        transition_plan.contract_id,
        "snapshot:collision:sweep:2",
        region_capture(scene_id, 2),
        domain.clone(),
        CollisionCandidateGroupingPolicy::SharedBroadphaseRegion,
        CollisionCertificationPolicy::CpuOracleParity,
        transition_items.clone(),
        2,
    )
    .checked()
    .expect("valid transition batch");

    let mut expected_store = CollisionArtifactStore::default();
    let mut expected_results = Vec::new();
    let mut expected_certification_queries = 0u32;
    let mut expected_reuse_items = 0u32;
    for item in &transition_batch.items {
        let (result, trace) = execute_with_store(
            &transition_plan,
            &ctx,
            &transition_batch.args_for_item(item),
            &mut expected_store,
        )
        .expect("transition baseline");
        expected_results.push(result);
        expected_certification_queries += collision_cpu_certification_query_count(&trace);
        if trace.reuse_metrics.consumed_count > 0 {
            expected_reuse_items += 1;
        }
    }

    let transition_result =
        execute_batch_cpu(&transition_batch, &ctx, None).expect("transition batch execution");
    assert_eq!(transition_result.results, expected_results);
    assert_eq!(transition_result.report.query_count, 2);
    assert_eq!(transition_result.report.batch_count, 1);
    assert_eq!(transition_result.report.dispatch_count, 1);
    assert_eq!(transition_result.report.dispatch_items, 2);
    assert_approx_eq(transition_result.report.average_items_per_dispatch, 2.0);
    assert_eq!(
        transition_result.report.cpu_certification_query_count,
        expected_certification_queries
    );
    assert_eq!(transition_result.report.fallback_count, 0);
    assert_approx_eq(
        transition_result.report.witness_reuse_rate as f32,
        expected_reuse_items as f32 / transition_batch.items.len() as f32,
    );

    let toi_plan = CollisionPlan::for_query(CollisionQueryKind::SphereTimeOfImpactTransition);
    let toi_batch = CollisionWorkloadBatch::new(
        "time of impact batch",
        "collision_perf_toi_transition_reuse",
        "collision_perf_toi_transition_reuse",
        toi_plan.clone(),
        toi_plan.contract_id,
        "snapshot:collision:toi:2",
        region_capture(scene_id, 2),
        domain,
        CollisionCandidateGroupingPolicy::SharedBroadphaseRegion,
        CollisionCertificationPolicy::CpuOracleParity,
        vec![
            CollisionBatchItem::SphereTimeOfImpact {
                transition: wrela::collision_contract::CollisionSnapshotTransitionInput {
                    current_snapshot_epoch: 2,
                    previous_snapshot_epoch: 1,
                    change_class: ChangeClass::Presentation,
                },
                sweep: wrela::collision_contract::CollisionSphereSweepInput {
                    start_center: [0.0, 0.0, 2.0],
                    end_center: [0.0, 0.0, -2.0],
                    radius: 0.25,
                    contact_tolerance: 0.001,
                    max_iterations: 64,
                },
            },
            CollisionBatchItem::SphereTimeOfImpact {
                transition: wrela::collision_contract::CollisionSnapshotTransitionInput {
                    current_snapshot_epoch: 2,
                    previous_snapshot_epoch: 1,
                    change_class: ChangeClass::Presentation,
                },
                sweep: wrela::collision_contract::CollisionSphereSweepInput {
                    start_center: [0.0, 0.0, 2.0],
                    end_center: [0.0, 0.0, -2.0],
                    radius: 0.25,
                    contact_tolerance: 0.001,
                    max_iterations: 64,
                },
            },
        ],
        2,
    )
    .checked()
    .expect("valid toi batch");

    let mut expected_store = CollisionArtifactStore::default();
    let mut expected_results = Vec::new();
    let mut expected_certification_queries = 0u32;
    let mut expected_reuse_items = 0u32;
    for item in &toi_batch.items {
        let (result, trace) = execute_with_store(
            &toi_plan,
            &ctx,
            &toi_batch.args_for_item(item),
            &mut expected_store,
        )
        .expect("toi baseline");
        expected_results.push(result);
        expected_certification_queries += collision_cpu_certification_query_count(&trace);
        if trace.reuse_metrics.consumed_count > 0 {
            expected_reuse_items += 1;
        }
    }

    let toi_result = execute_batch_cpu(&toi_batch, &ctx, None).expect("toi batch execution");
    assert_eq!(toi_result.results, expected_results);
    assert_eq!(toi_result.report.query_count, 2);
    assert_eq!(toi_result.report.batch_count, 1);
    assert_eq!(toi_result.report.dispatch_count, 1);
    assert_eq!(toi_result.report.dispatch_items, 2);
    assert_approx_eq(toi_result.report.average_items_per_dispatch, 2.0);
    assert_eq!(
        toi_result.report.cpu_certification_query_count,
        expected_certification_queries
    );
    assert_eq!(toi_result.report.fallback_count, 0);
    assert_approx_eq(
        toi_result.report.witness_reuse_rate as f32,
        expected_reuse_items as f32 / toi_batch.items.len() as f32,
    );
}

#[test]
fn collision_batch_validation_rejects_mixed_item_kinds() {
    let ctx = typed_query_module(collision_fixture_source());
    let scene_id = stable_region_scene_capture_id(&SmolStr::new("collision_region"));
    let domain = scene_domain(scene_id);
    let plan = CollisionPlan::for_query(CollisionQueryKind::PointOccupancyWorld);
    let batch = CollisionWorkloadBatch::new(
        "mixed batch",
        "mixed",
        "mixed",
        plan,
        CollisionPlan::for_query(CollisionQueryKind::PointOccupancyWorld).contract_id,
        "snapshot:mixed",
        region_capture(scene_id, 2),
        domain,
        CollisionCandidateGroupingPolicy::PerItem,
        CollisionCertificationPolicy::MetricsOnly,
        vec![
            CollisionBatchItem::PointOccupancy {
                point: [0.0, 0.0, 0.25],
            },
            CollisionBatchItem::RayCast {
                ray: wrela::collision_contract::CollisionRayInput {
                    origin: [0.0, 0.0, 2.0],
                    direction: [0.0, 0.0, -1.0],
                    max_distance: 6.0,
                    min_step: 0.05,
                    hit_epsilon: 0.001,
                    max_steps: 96,
                },
            },
        ],
        2,
    );

    let errors = batch.validate();
    assert!(
        !errors.is_empty(),
        "expected mixed item batch validation to report errors"
    );
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("does not match contract")),
        "expected a contract mismatch error, got {errors:?}"
    );
    assert!(
        execute_batch_cpu(&batch, &ctx, None).is_err(),
        "expected invalid mixed item batch to fail execution"
    );
}

#[test]
fn transition_collision_dense_fallback_detects_contact_after_iteration_budget_exhaustion() {
    let ctx = typed_query_module(collision_fixture_source());
    let scene_id = stable_region_scene_capture_id(&SmolStr::new("collision_region"));
    let domain = scene_domain(scene_id);
    let transition = collision_transition_input(2, 1, ChangeClass::Presentation);
    let sweep = collision_sweep_input_with_iterations([-1.0, 0.74, 0.0], [1.0, 0.74, 0.0], 0.25, 1);
    let plan = CollisionPlan::for_query(CollisionQueryKind::SphereSweepTransition);

    let (result, trace) = execute_with_store(
        &plan,
        &ctx,
        &[region_capture(scene_id, 2), domain, transition, sweep],
        &mut CollisionArtifactStore::default(),
    )
    .expect("dense fallback sweep");
    assert_eq!(trace.fallback_count, 1);
    assert_eq!(trace.certificate_successes, 0);
    match result {
        CollisionResult::Sweep(value) => {
            assert!(value.hit);
            let witness = value.witness.expect("fallback sweep witness");
            assert!(
                witness.contact_fraction_upper_bound > 0.2
                    && witness.contact_fraction_upper_bound < 0.8
            );
        }
        other => panic!("expected sweep result, got {other:?}"),
    }
}

#[test]
fn transition_collision_dense_fallback_only_certifies_proven_prefix_on_no_hit() {
    let ctx = typed_query_module(collision_fixture_source());
    let scene_id = stable_region_scene_capture_id(&SmolStr::new("collision_region"));
    let domain = scene_domain(scene_id);
    let transition = collision_transition_input(2, 1, ChangeClass::Presentation);
    let sweep =
        collision_sweep_input_with_iterations([-1.0, 0.54, 0.54], [1.0, 0.54, 0.54], 0.25, 1);
    let plan = CollisionPlan::for_query(CollisionQueryKind::SphereTimeOfImpactTransition);

    let (result, trace) = execute_with_store(
        &plan,
        &ctx,
        &[region_capture(scene_id, 2), domain, transition, sweep],
        &mut CollisionArtifactStore::default(),
    )
    .expect("dense fallback toi");
    assert!(trace.broadphase_candidate_count > 0);
    assert_eq!(trace.fallback_count, 1);
    assert_eq!(trace.certificate_successes, 0);
    match result {
        CollisionResult::TimeOfImpact(value) => {
            assert!(!value.hit);
            let certificate = value
                .no_hit_certificate
                .expect("partial no-hit certificate");
            assert!(certificate.valid_through_fraction < 1.0);
        }
        other => panic!("expected time-of-impact result, got {other:?}"),
    }
}

#[test]
fn static_collision_paths_use_candidate_capture_queries() {
    let ctx = typed_query_module(collision_fixture_source());
    let scene_id = stable_region_scene_capture_id(&SmolStr::new("collision_region"));
    let domain = scene_domain(scene_id);
    let capture = region_capture(scene_id, 2);

    let occupancy_plan = CollisionPlan::for_query(CollisionQueryKind::PointOccupancyWorld);
    let (_, occupancy_trace) = occupancy_plan
        .execute(
            &ctx,
            &[
                capture.clone(),
                domain.clone(),
                collision_point_input([0.0, 0.0, 0.25]),
            ],
        )
        .expect("point occupancy");
    assert!(
        occupancy_trace
            .executed_query_contracts
            .contains(&wrela::query_contract::SPATIAL_DISTANCE_CAPTURE_SHAPE),
        "expected point occupancy to evaluate the shape-capture distance contract: {occupancy_trace:?}"
    );
    assert!(
        occupancy_trace
            .executed_query_contracts
            .contains(&wrela::query_contract::SPATIAL_NORMAL_CAPTURE_SHAPE),
        "expected point occupancy to evaluate the shape-capture normal contract: {occupancy_trace:?}"
    );

    let ray_plan = CollisionPlan::for_query(CollisionQueryKind::RayCastWorld);
    let (_, ray_trace) = ray_plan
        .execute(
            &ctx,
            &[
                capture.clone(),
                domain.clone(),
                collision_ray_input([0.0, 0.0, 2.0], [0.0, 0.0, -1.0]),
            ],
        )
        .expect("ray cast");
    assert!(
        ray_trace
            .executed_query_contracts
            .contains(&wrela::query_contract::SPATIAL_TRACE_CAPTURE_SHAPE),
        "expected ray casting to evaluate the shape-capture trace contract: {ray_trace:?}"
    );

    let overlap_plan = CollisionPlan::for_query(CollisionQueryKind::SphereOverlapWorld);
    let (_, overlap_trace) = overlap_plan
        .execute(
            &ctx,
            &[
                capture,
                domain,
                collision_sphere_probe([0.0, 0.0, 0.9], 0.6),
            ],
        )
        .expect("sphere overlap");
    assert!(
        overlap_trace
            .executed_query_contracts
            .contains(&wrela::query_contract::SPATIAL_DISTANCE_CAPTURE_SHAPE),
        "expected sphere overlap to evaluate the shape-capture distance contract: {overlap_trace:?}"
    );
}

#[test]
fn transition_collision_rejects_out_of_authority_change_class() {
    let ctx = typed_query_module(collision_fixture_source());
    let scene_id = stable_region_scene_capture_id(&SmolStr::new("collision_region"));
    let domain = scene_domain(scene_id);
    let plan = CollisionPlan::for_query(CollisionQueryKind::SphereSweepTransition);
    let result = plan.execute(
        &ctx,
        &[
            region_capture(scene_id, 2),
            domain,
            collision_transition_input(2, 1, ChangeClass::Topology),
            collision_sweep_input([0.0, 0.0, 2.0], [0.0, 0.0, -2.0], 0.25),
        ],
    );
    assert!(
        matches!(
            result,
            Err(
                wrela::collision_plan::CollisionExecError::TransitionAuthorityExceeded {
                    observed: ChangeClass::Topology,
                    maximum: ChangeClass::Presentation,
                }
            )
        ),
        "expected topology transition to exceed declared authority, got {result:?}"
    );
}

#[test]
fn no_hit_certificate_is_reported_for_clear_transition_sweep() {
    let ctx = typed_query_module(collision_fixture_source());
    let scene_id = stable_region_scene_capture_id(&SmolStr::new("collision_region"));
    let domain = scene_domain(scene_id);
    let plan = CollisionPlan::for_query(CollisionQueryKind::SphereSweepTransition);
    let (result, trace) = plan
        .execute(
            &ctx,
            &[
                region_capture(scene_id, 2),
                domain,
                collision_transition_input(2, 1, ChangeClass::Presentation),
                collision_sweep_input([20.0, 0.0, 20.0], [20.0, 0.0, -20.0], 0.25),
            ],
        )
        .expect("clear sweep");
    assert_eq!(trace.reuse_metrics.unavailable_count, 2);
    match result {
        wrela::collision_contract::CollisionResult::Sweep(value) => {
            assert!(!value.hit);
            let certificate = value.no_hit_certificate.expect("no-hit certificate");
            assert_eq!(certificate.valid_through_fraction, 1.0);
        }
        other => panic!("expected sweep result, got {other:?}"),
    }
}
