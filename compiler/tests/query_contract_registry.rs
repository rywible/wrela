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
            "support.summary.capture.field",
            "support.summary.capture.shape",
            "support.summary.world",
            "spatial.nearest.capture.shape",
            "spatial.nearest.batch.shape",
            "spatial.nearest.world",
            "spatial.occluded.capture.shape",
            "spatial.occluded.batch.shape",
            "spatial.occluded.world",
            "surface.sample.capture.shape",
            "surface.sample.batch.shape",
            "surface.sample.world",
            "participants.radiance.capture.shape",
            "participants.radiance.world",
            "participants.medium.capture.shape",
            "participants.medium.world",
        ]
    );
    assert_eq!(query_contract::query_contracts().len(), 26);
    assert!(
        query_contract::query_contracts()
            .iter()
            .all(|descriptor| descriptor.version == query_contract::QUERY_CONTRACT_VERSION)
    );
}

#[test]
fn nearest_contracts_are_canonical_and_trace_ids_are_compatibility_aliases() {
    let nearest_cases = [
        query_contract::SPATIAL_NEAREST_CAPTURE_SHAPE,
        query_contract::SPATIAL_NEAREST_BATCH_SHAPE,
        query_contract::SPATIAL_NEAREST_WORLD,
    ];
    for contract_id in nearest_cases {
        let descriptor = query_contract::query_contract(contract_id).unwrap();
        assert_eq!(descriptor.id, contract_id);
        assert_eq!(
            descriptor.question,
            query_contract::QueryQuestionId::Nearest
        );
    }
    assert!(
        query_contract::query_contracts()
            .iter()
            .all(|descriptor| descriptor.question != query_contract::QueryQuestionId::Trace)
    );

    let alias_cases = [
        (
            query_contract::LEGACY_SPATIAL_TRACE_CAPTURE_SHAPE,
            query_contract::SPATIAL_NEAREST_CAPTURE_SHAPE,
        ),
        (
            query_contract::LEGACY_SPATIAL_TRACE_BATCH_SHAPE,
            query_contract::SPATIAL_NEAREST_BATCH_SHAPE,
        ),
        (
            query_contract::LEGACY_SPATIAL_TRACE_WORLD,
            query_contract::SPATIAL_NEAREST_WORLD,
        ),
    ];
    assert_eq!(
        query_contract::query_contract_aliases().len(),
        alias_cases.len()
    );
    for (legacy_id, canonical_id) in alias_cases {
        assert!(
            query_contract::query_contract_aliases()
                .iter()
                .any(|alias| alias.alias_id == legacy_id && alias.canonical_id == canonical_id)
        );
        assert_eq!(
            query_contract::canonical_query_contract_id(legacy_id),
            canonical_id
        );
        let descriptor = query_contract::query_contract(legacy_id).unwrap();
        assert_eq!(descriptor.id, canonical_id);
        assert_eq!(
            descriptor.question,
            query_contract::QueryQuestionId::Nearest
        );
        assert!(
            query_contract::query_legacy_builtin_name(legacy_id).is_some(),
            "legacy id '{}' should resolve to the canonical contract's legacy execution name",
            legacy_id.as_str()
        );
    }
}

#[test]
fn family_namespace_members_resolve_through_contract_registry() {
    assert_eq!(
        query_contract::query_family_namespace("spatial"),
        Some(query_contract::QueryFamilyId::Spatial)
    );
    assert_eq!(
        query_contract::query_family_namespace("surface"),
        Some(query_contract::QueryFamilyId::Surface)
    );
    assert_eq!(
        query_contract::query_family_namespace("participants"),
        Some(query_contract::QueryFamilyId::Participants)
    );
    assert_eq!(
        query_contract::query_family_namespace("support"),
        Some(query_contract::QueryFamilyId::Support)
    );

    let cases = [
        (
            query_contract::QueryFamilyId::Spatial,
            "distance",
            query_contract::QuerySurfaceKind::CaptureScalar,
            query_contract::CaptureKind::Field,
            query_contract::SPATIAL_DISTANCE_CAPTURE_FIELD,
            "spatial.distance",
        ),
        (
            query_contract::QueryFamilyId::Spatial,
            "distance_batch",
            query_contract::QuerySurfaceKind::CaptureBatch,
            query_contract::CaptureKind::Shape,
            query_contract::SPATIAL_DISTANCE_BATCH_SHAPE,
            "spatial.distance_batch",
        ),
        (
            query_contract::QueryFamilyId::Spatial,
            "nearest",
            query_contract::QuerySurfaceKind::WorldScalar,
            query_contract::CaptureKind::Region,
            query_contract::SPATIAL_NEAREST_WORLD,
            "spatial.nearest",
        ),
        (
            query_contract::QueryFamilyId::Surface,
            "sample_batch",
            query_contract::QuerySurfaceKind::CaptureBatch,
            query_contract::CaptureKind::Shape,
            query_contract::SURFACE_SAMPLE_BATCH_SHAPE,
            "surface.sample_batch",
        ),
        (
            query_contract::QueryFamilyId::Participants,
            "radiance",
            query_contract::QuerySurfaceKind::WorldScalar,
            query_contract::CaptureKind::Region,
            query_contract::PARTICIPANTS_RADIANCE_WORLD,
            "participants.radiance",
        ),
        (
            query_contract::QueryFamilyId::Support,
            "summary",
            query_contract::QuerySurfaceKind::CaptureScalar,
            query_contract::CaptureKind::Shape,
            query_contract::SUPPORT_SUMMARY_CAPTURE_SHAPE,
            "support.summary",
        ),
    ];

    for (family, member, surface, capture_kind, contract_id, call) in cases {
        let descriptor =
            query_contract::query_contract_for_family_member(family, member, surface, capture_kind)
                .unwrap_or_else(|| panic!("missing family member bundle for {call}"));
        assert_eq!(descriptor.id, contract_id);
        assert_eq!(
            call,
            format!(
                "{}.{}",
                query_contract::query_family_name(descriptor.family),
                query_contract::query_family_member_name(descriptor)
            )
        );
    }

    assert!(
        query_contract::query_contract_for_family_member(
            query_contract::QueryFamilyId::Spatial,
            "distance",
            query_contract::QuerySurfaceKind::CaptureBatch,
            query_contract::CaptureKind::Field,
        )
        .is_none(),
        "scalar family members must not resolve to batch contracts"
    );
    assert!(
        query_contract::query_contract_for_family_member(
            query_contract::QueryFamilyId::Spatial,
            "distance_batch",
            query_contract::QuerySurfaceKind::CaptureScalar,
            query_contract::CaptureKind::Field,
        )
        .is_none(),
        "batch family members must not resolve to scalar contracts"
    );
}

#[test]
fn query_family_member_names_round_trip_through_registry() {
    for descriptor in query_contract::query_contracts() {
        let member_name = query_contract::query_family_member_name(descriptor);
        let resolved = query_contract::query_contract_for_family_member(
            descriptor.family,
            member_name,
            descriptor.surface,
            descriptor.capture_kind,
        )
        .unwrap_or_else(|| {
            panic!(
                "descriptor '{}' should round-trip through family member '{}'",
                descriptor.id.as_str(),
                member_name
            )
        });
        assert_eq!(resolved.id, descriptor.id);
    }
}

#[test]
fn query_contract_registry_public_catalog_has_legacy_builtin_names() {
    for descriptor in query_contract::query_contracts() {
        assert!(
            query_contract::query_legacy_builtin_name(descriptor.id).is_some(),
            "public descriptor '{}' should expose its legacy authored builtin name",
            descriptor.id.as_str()
        );
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
            BatchQueryPlan::for_shape_query(BatchQueryKind::Nearest, DispatchBackend::Auto, None),
            query_contract::SPATIAL_NEAREST_BATCH_SHAPE,
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
        assert_eq!(plan.contract_id, contract_id);
        assert_eq!(plan.family, descriptor.family);
        assert_eq!(plan.surface, descriptor.surface);
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
            CaptureQueryPlan::for_query(CaptureQueryKind::SupportSummary, CaptureKind::Field, None)
                .unwrap(),
            query_contract::SUPPORT_SUMMARY_CAPTURE_FIELD,
        ),
        (
            CaptureQueryPlan::for_query(CaptureQueryKind::SupportSummary, CaptureKind::Shape, None)
                .unwrap(),
            query_contract::SUPPORT_SUMMARY_CAPTURE_SHAPE,
        ),
        (
            CaptureQueryPlan::for_query(CaptureQueryKind::Nearest, CaptureKind::Shape, None)
                .unwrap(),
            query_contract::SPATIAL_NEAREST_CAPTURE_SHAPE,
        ),
        (
            CaptureQueryPlan::for_query(CaptureQueryKind::Trace, CaptureKind::Shape, None).unwrap(),
            query_contract::SPATIAL_TRACE_CAPTURE_SHAPE,
        ),
        (
            CaptureQueryPlan::for_query(CaptureQueryKind::Occluded, CaptureKind::Shape, None)
                .unwrap(),
            query_contract::SPATIAL_OCCLUDED_CAPTURE_SHAPE,
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
        assert_eq!(plan.contract_id, contract_id);
        assert_eq!(plan.family, descriptor.family);
        assert_eq!(plan.surface, descriptor.surface);
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
            WorldQueryPlan::for_query(WorldQueryKind::SupportSummary),
            query_contract::SUPPORT_SUMMARY_WORLD,
        ),
        (
            WorldQueryPlan::for_query(WorldQueryKind::Nearest),
            query_contract::SPATIAL_NEAREST_WORLD,
        ),
        (
            WorldQueryPlan::for_query(WorldQueryKind::Trace),
            query_contract::SPATIAL_TRACE_WORLD,
        ),
        (
            WorldQueryPlan::for_query(WorldQueryKind::Occluded),
            query_contract::SPATIAL_OCCLUDED_WORLD,
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
        assert_eq!(plan.contract_id, contract_id);
        assert_eq!(plan.family, descriptor.family);
        assert_eq!(plan.surface, descriptor.surface);
    }
}

#[test]
fn descriptor_driven_plan_builders_cover_every_registered_contract() {
    for descriptor in query_contract::query_contracts() {
        match descriptor.surface {
            query_contract::QuerySurfaceKind::CaptureScalar => {
                let plan = CaptureQueryPlan::for_contract(descriptor.id, None)
                    .expect("capture descriptor should build a plan");
                assert_eq!(plan.contract_id, descriptor.id);
                assert_eq!(plan.contract_version, descriptor.version);
                assert_eq!(plan.family, descriptor.family);
                assert_eq!(plan.surface, descriptor.surface);
                assert_eq!(plan.capture_kind, descriptor.capture_kind);
                assert_eq!(plan.result_kind, descriptor.result_kind);
                assert_eq!(plan.candidate_contract.item_kind, descriptor.item_kind);
                assert_eq!(plan.result_contract.result_kind, descriptor.result_kind);
                assert_eq!(
                    plan.preserves_local_hit_context,
                    descriptor.preserves_local_hit_context
                );
                assert_eq!(
                    plan.participant_contract
                        .as_ref()
                        .map(|contract| contract.kind),
                    descriptor.participant_kind.map(|kind| match kind {
                        query_contract::ParticipantContractKind::Radiance => {
                            CaptureQueryKind::Radiance
                        }
                        query_contract::ParticipantContractKind::Medium => CaptureQueryKind::Medium,
                    })
                );
            }
            query_contract::QuerySurfaceKind::WorldScalar => {
                let plan =
                    WorldQueryPlan::for_contract_with_backend(descriptor.id, DispatchBackend::Auto)
                        .expect("world descriptor should build a plan");
                assert_eq!(plan.contract_id, descriptor.id);
                assert_eq!(plan.contract_version, descriptor.version);
                assert_eq!(plan.family, descriptor.family);
                assert_eq!(plan.surface, descriptor.surface);
                assert_eq!(plan.result_kind, descriptor.result_kind);
                assert_eq!(plan.backend, DispatchBackend::Auto);
                assert_eq!(plan.dispatch_contract.item_kind, descriptor.item_kind);
                assert_eq!(plan.dispatch_contract.result_kind, descriptor.result_kind);
                assert_eq!(plan.candidate_contract.item_kind, descriptor.item_kind);
                assert_eq!(plan.result_contract.result_kind, descriptor.result_kind);
                assert_eq!(
                    plan.domain_flags.as_slice(),
                    descriptor.required_domain_flags
                );
                assert_eq!(
                    plan.preserves_local_hit_context,
                    descriptor.preserves_local_hit_context
                );
            }
            query_contract::QuerySurfaceKind::CaptureBatch => {
                let plan = BatchQueryPlan::for_contract(descriptor.id, DispatchBackend::Auto, None)
                    .expect("batch descriptor should build a plan");
                assert_eq!(plan.contract_id, descriptor.id);
                assert_eq!(plan.contract_version, descriptor.version);
                assert_eq!(plan.family, descriptor.family);
                assert_eq!(plan.surface, descriptor.surface);
                assert_eq!(plan.capture_kind, descriptor.capture_kind);
                assert_eq!(plan.backend, DispatchBackend::Auto);
                assert_eq!(plan.dispatch_contract.item_kind, descriptor.item_kind);
                assert_eq!(plan.dispatch_contract.result_kind, descriptor.result_kind);
                assert_eq!(plan.candidate_contract.item_kind, descriptor.item_kind);
                assert_eq!(plan.result_contract.result_kind, descriptor.result_kind);
                assert_eq!(
                    plan.domain_flags.as_slice(),
                    descriptor.required_domain_flags
                );
                assert_eq!(
                    plan.preserves_local_hit_context,
                    descriptor.preserves_local_hit_context
                );
            }
        }
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
        (
            WorldQueryKind::SupportSummary,
            query_contract::SUPPORT_SUMMARY_WORLD,
        ),
        (
            WorldQueryKind::Nearest,
            query_contract::SPATIAL_NEAREST_WORLD,
        ),
        (WorldQueryKind::Trace, query_contract::SPATIAL_TRACE_WORLD),
        (
            WorldQueryKind::Occluded,
            query_contract::SPATIAL_OCCLUDED_WORLD,
        ),
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
        let descriptor = query_contract::query_contract(contract_id).unwrap();
        let legacy_builtin = query_contract::query_legacy_builtin_name(contract_id).unwrap();
        let expected_query_name = match kind {
            WorldQueryKind::Nearest => "nearest_world",
            _ => legacy_builtin,
        };
        assert_eq!(semantics.query_name, expected_query_name);
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
