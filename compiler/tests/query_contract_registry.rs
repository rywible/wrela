use wrela::query_contract;
use wrela::query_exec;
use wrela::query_plan::{
    BatchQueryKind, BatchQueryPlan, CaptureKind, CaptureQueryKind, CaptureQueryPlan,
    DispatchBackend, WorldQueryKind, WorldQueryPlan,
};

#[test]
fn query_contract_registry_has_stable_seed_order_and_versions() {
    let ids = query_contract::query_contracts()
        .iter()
        .map(|descriptor| descriptor.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        ids,
        vec![
            "spatial.distance.capture.field",
            "spatial.distance.capture.shape",
            "spatial.distance.world",
            "spatial.distance.batch.field",
            "spatial.distance.batch.shape",
            "spatial.normal.capture.field",
            "spatial.normal.capture.shape",
            "spatial.normal.world",
            "spatial.normal.batch.field",
            "spatial.normal.batch.shape",
            "spatial.trace.capture.shape",
            "spatial.trace.batch.shape",
            "spatial.trace.world",
            "spatial.occluded.batch.shape",
            "surface.sample.capture.shape",
            "surface.sample.batch.shape",
            "surface.sample.world",
            "participants.radiance.capture.shape",
            "participants.radiance.world",
            "participants.medium.capture.shape",
            "participants.medium.world",
        ]
    );
    assert_eq!(query_contract::query_contracts().len(), 21);
    assert!(
        query_contract::query_contracts()
            .iter()
            .all(|descriptor| descriptor.version == query_contract::QUERY_CONTRACT_VERSION)
    );
}

#[test]
fn query_contract_registry_bindings_cover_every_descriptor() {
    assert_eq!(
        query_contract::query_contracts().len(),
        query_contract::query_execution_bindings().len()
    );

    for descriptor in query_contract::query_contracts() {
        let binding = query_contract::query_execution_binding(descriptor.id)
            .expect("every descriptor should have an execution binding");
        assert_eq!(binding.contract_id, descriptor.id);
        assert!(!binding.legacy_builtin_name.is_empty());
        assert!(binding.helper_name.is_some());
        assert!(query_contract::query_contract_bundle(descriptor.id).is_some());
    }
}

#[test]
fn query_contract_registry_exhaustively_maps_current_plan_surfaces() {
    let batch_cases = [
        (
            BatchQueryPlan::for_field_query(
                BatchQueryKind::Distance,
                CaptureKind::Field,
                DispatchBackend::Auto,
                None,
            ),
            query_contract::SPATIAL_DISTANCE_BATCH_FIELD,
        ),
        (
            BatchQueryPlan::for_field_query(
                BatchQueryKind::Distance,
                CaptureKind::Shape,
                DispatchBackend::Auto,
                None,
            ),
            query_contract::SPATIAL_DISTANCE_BATCH_SHAPE,
        ),
        (
            BatchQueryPlan::for_field_query(
                BatchQueryKind::Normal,
                CaptureKind::Field,
                DispatchBackend::Auto,
                None,
            ),
            query_contract::SPATIAL_NORMAL_BATCH_FIELD,
        ),
        (
            BatchQueryPlan::for_field_query(
                BatchQueryKind::Normal,
                CaptureKind::Shape,
                DispatchBackend::Auto,
                None,
            ),
            query_contract::SPATIAL_NORMAL_BATCH_SHAPE,
        ),
        (
            BatchQueryPlan::for_shape_query(BatchQueryKind::Trace, DispatchBackend::Auto, None),
            query_contract::SPATIAL_TRACE_BATCH_SHAPE,
        ),
        (
            BatchQueryPlan::for_shape_query(BatchQueryKind::Surface, DispatchBackend::Auto, None),
            query_contract::SURFACE_SAMPLE_BATCH_SHAPE,
        ),
        (
            BatchQueryPlan::for_shape_query(BatchQueryKind::Occluded, DispatchBackend::Auto, None),
            query_contract::SPATIAL_OCCLUDED_BATCH_SHAPE,
        ),
    ];
    for (plan, contract_id) in batch_cases {
        let descriptor = query_contract::query_contract(contract_id).unwrap();
        let binding = query_contract::query_execution_binding(contract_id).unwrap();
        assert_eq!(plan.contract_id, contract_id);
        assert_eq!(plan.family, descriptor.family);
        assert_eq!(plan.surface, descriptor.surface);
        assert_eq!(binding.helper_name, Some(plan.helper_name.as_str()));
    }

    let capture_cases = [
        (
            CaptureQueryPlan::for_query(CaptureQueryKind::Distance, CaptureKind::Field, None)
                .unwrap(),
            query_contract::SPATIAL_DISTANCE_CAPTURE_FIELD,
        ),
        (
            CaptureQueryPlan::for_query(CaptureQueryKind::Distance, CaptureKind::Shape, None)
                .unwrap(),
            query_contract::SPATIAL_DISTANCE_CAPTURE_SHAPE,
        ),
        (
            CaptureQueryPlan::for_query(CaptureQueryKind::Normal, CaptureKind::Field, None)
                .unwrap(),
            query_contract::SPATIAL_NORMAL_CAPTURE_FIELD,
        ),
        (
            CaptureQueryPlan::for_query(CaptureQueryKind::Normal, CaptureKind::Shape, None)
                .unwrap(),
            query_contract::SPATIAL_NORMAL_CAPTURE_SHAPE,
        ),
        (
            CaptureQueryPlan::for_query(CaptureQueryKind::Trace, CaptureKind::Shape, None).unwrap(),
            query_contract::SPATIAL_TRACE_CAPTURE_SHAPE,
        ),
        (
            CaptureQueryPlan::for_query(CaptureQueryKind::Surface, CaptureKind::Shape, None)
                .unwrap(),
            query_contract::SURFACE_SAMPLE_CAPTURE_SHAPE,
        ),
        (
            CaptureQueryPlan::for_query(CaptureQueryKind::Radiance, CaptureKind::Shape, None)
                .unwrap(),
            query_contract::PARTICIPANTS_RADIANCE_CAPTURE_SHAPE,
        ),
        (
            CaptureQueryPlan::for_query(CaptureQueryKind::Medium, CaptureKind::Shape, None)
                .unwrap(),
            query_contract::PARTICIPANTS_MEDIUM_CAPTURE_SHAPE,
        ),
    ];
    for (plan, contract_id) in capture_cases {
        let descriptor = query_contract::query_contract(contract_id).unwrap();
        let binding = query_contract::query_execution_binding(contract_id).unwrap();
        assert_eq!(plan.contract_id, contract_id);
        assert_eq!(plan.family, descriptor.family);
        assert_eq!(plan.surface, descriptor.surface);
        assert_eq!(binding.helper_name, Some(plan.helper_name.as_str()));
    }

    let world_cases = [
        (
            WorldQueryPlan::for_query(WorldQueryKind::Distance),
            query_contract::SPATIAL_DISTANCE_WORLD,
        ),
        (
            WorldQueryPlan::for_query(WorldQueryKind::Normal),
            query_contract::SPATIAL_NORMAL_WORLD,
        ),
        (
            WorldQueryPlan::for_query(WorldQueryKind::Trace),
            query_contract::SPATIAL_TRACE_WORLD,
        ),
        (
            WorldQueryPlan::for_query(WorldQueryKind::Surface),
            query_contract::SURFACE_SAMPLE_WORLD,
        ),
        (
            WorldQueryPlan::for_query(WorldQueryKind::Radiance),
            query_contract::PARTICIPANTS_RADIANCE_WORLD,
        ),
        (
            WorldQueryPlan::for_query(WorldQueryKind::Medium),
            query_contract::PARTICIPANTS_MEDIUM_WORLD,
        ),
    ];
    for (plan, contract_id) in world_cases {
        let descriptor = query_contract::query_contract(contract_id).unwrap();
        let binding = query_contract::query_execution_binding(contract_id).unwrap();
        assert_eq!(plan.contract_id, contract_id);
        assert_eq!(plan.family, descriptor.family);
        assert_eq!(plan.surface, descriptor.surface);
        assert_eq!(binding.helper_name, Some(plan.helper_name.as_str()));
    }
}

#[test]
fn world_query_semantics_is_a_registry_wrapper() {
    let cases = [
        (
            WorldQueryKind::Distance,
            query_contract::SPATIAL_DISTANCE_WORLD,
        ),
        (WorldQueryKind::Normal, query_contract::SPATIAL_NORMAL_WORLD),
        (WorldQueryKind::Trace, query_contract::SPATIAL_TRACE_WORLD),
        (
            WorldQueryKind::Surface,
            query_contract::SURFACE_SAMPLE_WORLD,
        ),
        (
            WorldQueryKind::Radiance,
            query_contract::PARTICIPANTS_RADIANCE_WORLD,
        ),
        (
            WorldQueryKind::Medium,
            query_contract::PARTICIPANTS_MEDIUM_WORLD,
        ),
    ];

    for (kind, contract_id) in cases {
        let semantics = query_exec::world_query_semantics(kind);
        let (descriptor, binding) = query_contract::query_contract_bundle(contract_id).unwrap();
        assert_eq!(semantics.query_name, binding.legacy_builtin_name);
        assert_eq!(
            semantics.domain_flag,
            descriptor
                .required_domain_flags
                .first()
                .copied()
                .map(query_contract::scene_domain_flag_name)
        );
    }
}
