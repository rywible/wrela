use super::*;

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
