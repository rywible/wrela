use wrela::kernel::{KernelStructValue, KernelValue};
use wrela::portable::{
    PortableAbiError, PortableAbiType, PortableBuiltinType, PortableStructField, builtin_records,
    portable_abi_array_stride, portable_abi_decode_slice, portable_abi_decode_value,
    portable_abi_emit_wgsl_structs, portable_abi_encode_slice, portable_abi_encode_value,
    portable_abi_field_offset, portable_abi_layout, portable_artifact_contract_abi,
    portable_builtin_record_abi, portable_candidate_contract_abi, portable_dispatch_contract_abi,
    portable_hit_context_contract_abi, portable_participant_contract_abi, portable_query_item_abi,
    portable_query_result_abi, portable_result_contract_abi,
};
use wrela::query_plan;
use wrela::scene_ir;

#[test]
fn portable_abi_matches_wgsl_layout_rules_for_scalars_and_matrices() {
    let bool_layout = portable_abi_layout(&PortableAbiType::Bool);
    assert_eq!(bool_layout.size, 4);
    assert_eq!(bool_layout.align, 4);

    let vec3_layout = portable_abi_layout(&PortableAbiType::Vec3);
    assert_eq!(vec3_layout.size, 12);
    assert_eq!(vec3_layout.align, 16);

    let mat3_layout = portable_abi_layout(&PortableAbiType::Mat3);
    assert_eq!(mat3_layout.size, 48);
    assert_eq!(mat3_layout.align, 16);
}

#[test]
fn portable_abi_arrays_use_wgsl_stride_for_padded_elements() {
    let vec3_array = PortableAbiType::Array(Box::new(PortableAbiType::Vec3), 3);
    let vec3_array_layout = portable_abi_layout(&vec3_array);
    assert_eq!(portable_abi_array_stride(&PortableAbiType::Vec3), 16);
    assert_eq!(vec3_array_layout.size, 48);
    assert_eq!(vec3_array_layout.align, 16);

    let padded_struct = PortableAbiType::Struct {
        name: "PaddedStruct".into(),
        class_id: 0,
        fields: vec![
            PortableStructField {
                name: "basis".into(),
                ty: PortableAbiType::Mat3,
            },
            PortableStructField {
                name: "tag".into(),
                ty: PortableAbiType::U32,
            },
        ],
    };
    let padded_struct_layout = portable_abi_layout(&padded_struct);
    assert_eq!(padded_struct_layout.size, 64);
    assert_eq!(portable_abi_array_stride(&padded_struct), 64);

    let outer = PortableAbiType::Struct {
        name: "Outer".into(),
        class_id: 0,
        fields: vec![
            PortableStructField {
                name: "items".into(),
                ty: PortableAbiType::Array(Box::new(padded_struct.clone()), 2),
            },
            PortableStructField {
                name: "trailer".into(),
                ty: PortableAbiType::U32,
            },
        ],
    };
    let PortableAbiType::Struct { fields, .. } = &outer else {
        unreachable!();
    };
    assert_eq!(portable_abi_field_offset(fields, 1), 128);
    assert_eq!(portable_abi_layout(&outer).size, 144);
}

#[test]
fn portable_builtin_records_are_32_bit_clean() {
    for record in builtin_records() {
        for field in record.fields {
            match field.ty {
                PortableBuiltinType::Atom(_) => {}
                PortableBuiltinType::Named(name) => {
                    assert!(
                        portable_builtin_record_abi(name).is_some(),
                        "builtin record field {}.{} should resolve to a portable ABI type",
                        record.name,
                        field.name
                    );
                }
            }
        }
    }
}

#[test]
fn hit3_layout_preserves_wgsl_padding_boundaries() {
    let PortableAbiType::Struct { fields, .. } = portable_builtin_record_abi("Hit3").unwrap()
    else {
        panic!("Hit3 should lower to a struct ABI");
    };
    let layout = portable_abi_layout(&portable_builtin_record_abi("Hit3").unwrap());
    assert_eq!(layout.size, 256);
    assert_eq!(layout.align, 16);

    let position = fields
        .iter()
        .position(|field| field.name.as_str() == "position")
        .unwrap();
    let normal = fields
        .iter()
        .position(|field| field.name.as_str() == "normal")
        .unwrap();
    let shading_frame = fields
        .iter()
        .position(|field| field.name.as_str() == "shading_frame")
        .unwrap();
    let payload = fields
        .iter()
        .position(|field| field.name.as_str() == "payload")
        .unwrap();

    assert_eq!(portable_abi_field_offset(&fields, position), 16);
    assert_eq!(portable_abi_field_offset(&fields, normal), 32);
    assert_eq!(portable_abi_field_offset(&fields, shading_frame), 80);
    assert_eq!(portable_abi_field_offset(&fields, payload), 228);
}

#[test]
fn scene_domain_contract_layout_is_nested_and_budget_free() {
    let spatial = portable_builtin_record_abi("SpatialDomainContract").unwrap();
    let surface = portable_builtin_record_abi("SurfaceDomainContract").unwrap();
    let participants = portable_builtin_record_abi("ParticipantDomainContract").unwrap();
    let scene_domain = portable_builtin_record_abi("SceneDomain").unwrap();

    assert_eq!(portable_abi_layout(&spatial).size, 8);
    assert_eq!(portable_abi_layout(&surface).size, 4);
    assert_eq!(portable_abi_layout(&participants).size, 8);

    let PortableAbiType::Struct { fields, .. } = &scene_domain else {
        panic!("SceneDomain should lower to a struct ABI");
    };
    assert_eq!(
        fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        vec!["scene_id", "spatial", "surface", "participants"]
    );
    assert_eq!(portable_abi_field_offset(fields, 0), 0);
    assert_eq!(portable_abi_field_offset(fields, 1), 4);
    assert_eq!(portable_abi_field_offset(fields, 2), 12);
    assert_eq!(portable_abi_field_offset(fields, 3), 16);

    let layout = portable_abi_layout(&scene_domain);
    assert_eq!(layout.size, 24);
    assert_eq!(layout.align, 4);
}

#[test]
fn point_direction_query_layout_matches_two_vec3_samples() {
    let point_direction = portable_builtin_record_abi("PointDirectionQuery").unwrap();
    let PortableAbiType::Struct { fields, .. } = &point_direction else {
        panic!("PointDirectionQuery should lower to a struct ABI");
    };
    assert_eq!(
        fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        vec!["point", "direction"]
    );
    assert_eq!(portable_abi_field_offset(fields, 0), 0);
    assert_eq!(portable_abi_field_offset(fields, 1), 16);

    let layout = portable_abi_layout(&point_direction);
    assert_eq!(layout.size, 32);
    assert_eq!(layout.align, 16);
}

#[test]
fn unit_query_layout_is_a_stable_empty_public_record() {
    let unit = portable_builtin_record_abi("UnitQuery").unwrap();
    let PortableAbiType::Struct { fields, .. } = &unit else {
        panic!("UnitQuery should lower to a struct ABI");
    };
    assert!(fields.is_empty());
    assert_eq!(
        portable_query_item_abi(query_plan::QueryItemKind::Unit),
        Some(unit.clone())
    );

    let layout = portable_abi_layout(&unit);
    assert_eq!(layout.size, 4);
    assert_eq!(layout.align, 4);
}

#[test]
fn support_summary_result_layout_preserves_wgsl_padding_boundaries() {
    let summary = portable_builtin_record_abi("SupportSummaryResult").unwrap();
    let PortableAbiType::Struct { fields, .. } = &summary else {
        panic!("SupportSummaryResult should lower to a struct ABI");
    };
    assert_eq!(
        fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        vec![
            "support_class",
            "semantics",
            "has_bounds",
            "opaque_boundary",
            "can_coarse_support_prune",
            "min",
            "max"
        ]
    );
    assert_eq!(portable_abi_field_offset(fields, 0), 0);
    assert_eq!(portable_abi_field_offset(fields, 1), 4);
    assert_eq!(portable_abi_field_offset(fields, 2), 8);
    assert_eq!(portable_abi_field_offset(fields, 3), 12);
    assert_eq!(portable_abi_field_offset(fields, 4), 16);
    assert_eq!(portable_abi_field_offset(fields, 5), 32);
    assert_eq!(portable_abi_field_offset(fields, 6), 48);
    assert_eq!(
        portable_query_result_abi(
            query_plan::QuerySurfaceKind::CaptureScalar,
            query_plan::QueryResultKind::SupportSummaryResult
        ),
        Some(summary.clone())
    );
    assert_eq!(
        portable_query_result_abi(
            query_plan::QuerySurfaceKind::WorldScalar,
            query_plan::QueryResultKind::SupportSummaryResult
        ),
        Some(summary.clone())
    );
    assert!(
        portable_query_result_abi(
            query_plan::QuerySurfaceKind::CaptureBatch,
            query_plan::QueryResultKind::SupportSummaryResult
        )
        .is_none()
    );

    let layout = portable_abi_layout(&summary);
    assert_eq!(layout.size, 64);
    assert_eq!(layout.align, 16);
}

#[test]
fn retired_query_adapter_records_do_not_leak_through_public_abi() {
    assert!(portable_builtin_record_abi("TraceQuery").is_none());
    assert!(portable_builtin_record_abi("SurfaceQuery").is_none());
}

#[test]
fn query_contract_records_have_stable_portable_layouts() {
    let dispatch_layout = portable_abi_layout(&portable_dispatch_contract_abi(
        &query_plan::DispatchRecordContract {
            backend: query_plan::DispatchBackend::VirtualGpu,
            kernel: query_plan::InternalKernelKind::ShapeTraceCapture,
            item_kind: query_plan::QueryItemKind::RayQuery,
            result_kind: query_plan::QueryResultKind::Hit3,
        },
    ));
    assert_eq!(dispatch_layout.size, 20);
    assert_eq!(dispatch_layout.align, 4);

    let result_layout = portable_abi_layout(&portable_result_contract_abi(
        &query_plan::ResultRecordContract {
            result_kind: query_plan::QueryResultKind::Hit3,
            preserves_local_hit_context: true,
            stable_feature_id: true,
            stable_instance_id: true,
            stable_repeat_id: true,
        },
    ));
    assert_eq!(result_layout.size, 24);
    assert_eq!(result_layout.align, 4);
}

#[test]
fn artifact_contract_records_encode_scene_roots_and_support_counts() {
    let abi = portable_artifact_contract_abi(&query_plan::ArtifactContract {
        id: "shape_trace::artifact::0".into(),
        schema: query_plan::ArtifactSchema::CullingTable {
            candidate_strategy: query_plan::CandidateStrategy::SupportAcceleratedShapeTraversal,
            pruning_strategy: query_plan::PruningStrategy::CullingTable,
            support_class: scene_ir::SupportClass::Bounded,
            semantics: scene_ir::DistanceSemantics::ConservativeLowerBound,
            support_root: 11,
            support_node_count: 7,
            leaf_count: 3,
            identity_source_count: 2,
        },
        producer: "shape_trace".into(),
        consumer: "shape_trace".into(),
        deterministic: true,
        version: query_plan::QUERY_PLAN_CONTRACT_VERSION,
    });
    let PortableAbiType::Struct { fields, .. } = abi else {
        panic!("artifact contract abi should lower to a struct");
    };
    let layout = portable_abi_layout(&PortableAbiType::Struct {
        name: "ArtifactContract".into(),
        class_id: 0,
        fields: fields.clone(),
    });
    assert_eq!(layout.align, 4);
    assert_eq!(portable_abi_field_offset(&fields, 0), 0);
    assert_eq!(portable_abi_field_offset(&fields, 4), 16);
    assert_eq!(portable_abi_field_offset(&fields, fields.len() - 1), 40);
}

#[test]
fn candidate_and_hit_contract_records_have_stable_layouts() {
    let candidate_layout = portable_abi_layout(&portable_candidate_contract_abi(
        &query_plan::CandidateRecordContract {
            source: query_plan::CandidateSource::CaptureScene,
            item_kind: query_plan::QueryItemKind::PointQuery,
            candidate_strategy: query_plan::CandidateStrategy::DirectFieldCapture,
            pruning_strategy: query_plan::PruningStrategy::None,
            winner_mode: query_plan::WinnerSelectionMode::Nearest,
            stable_leaf_identity: true,
        },
    ));
    assert_eq!(candidate_layout.size, 28);
    assert_eq!(candidate_layout.align, 4);

    let hit_context_layout = portable_abi_layout(&portable_hit_context_contract_abi(
        &query_plan::HitContextContract {
            world_position: true,
            world_normal: true,
            local_position: true,
            local_normal: true,
            shading_frame: true,
            payload: true,
        },
    ));
    assert_eq!(hit_context_layout.size, 28);
    assert_eq!(hit_context_layout.align, 4);

    let participant_layout = portable_abi_layout(&portable_participant_contract_abi(
        &query_plan::ParticipantSelectionContract {
            kind: query_plan::CaptureQueryKind::Radiance,
            provenance_aware: true,
            additive: true,
        },
    ));
    assert_eq!(participant_layout.size, 16);
    assert_eq!(participant_layout.align, 4);
}

#[test]
fn portable_abi_roundtrips_struct_bytes_with_padding_and_bool_packing() {
    let abi = PortableAbiType::Struct {
        name: "RoundtripRecord".into(),
        class_id: 0,
        fields: vec![
            PortableStructField {
                name: "enabled".into(),
                ty: PortableAbiType::Bool,
            },
            PortableStructField {
                name: "position".into(),
                ty: PortableAbiType::Vec3,
            },
            PortableStructField {
                name: "basis".into(),
                ty: PortableAbiType::Mat3,
            },
            PortableStructField {
                name: "tags".into(),
                ty: PortableAbiType::Array(Box::new(PortableAbiType::U32), 3),
            },
        ],
    };
    let value = KernelValue::Struct(KernelStructValue {
        name: "RoundtripRecord".into(),
        fields: vec![
            ("enabled".into(), KernelValue::Bool(true)),
            ("position".into(), KernelValue::Vec3([1.5, -2.0, 3.25])),
            (
                "basis".into(),
                KernelValue::Mat3([1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]),
            ),
            (
                "tags".into(),
                KernelValue::Array(vec![
                    KernelValue::U32(7),
                    KernelValue::U32(11),
                    KernelValue::U32(13),
                ]),
            ),
        ],
    });

    let bytes = portable_abi_encode_value(&abi, &value).expect("encode abi bytes");
    assert_eq!(bytes.len(), portable_abi_layout(&abi).size as usize);
    assert_eq!(u32::from_le_bytes(bytes[0..4].try_into().unwrap()), 1);
    assert!(bytes[4..16].iter().all(|byte| *byte == 0));

    let decoded = portable_abi_decode_value(&abi, &bytes).expect("decode abi bytes");
    assert_eq!(decoded, value);
}

#[test]
fn portable_abi_slice_roundtrips_dispatch_result_and_hit_records() {
    let dispatch_abi = portable_dispatch_contract_abi(&query_plan::DispatchRecordContract {
        backend: query_plan::DispatchBackend::Wgsl,
        kernel: query_plan::InternalKernelKind::ShapeTraceCapture,
        item_kind: query_plan::QueryItemKind::RayQuery,
        result_kind: query_plan::QueryResultKind::Hit3,
    });
    let dispatch_values = vec![
        KernelValue::Struct(KernelStructValue {
            name: "DispatchRecordContract".into(),
            fields: vec![
                ("backend".into(), KernelValue::U32(2)),
                ("kernel".into(), KernelValue::U32(9)),
                ("item_kind".into(), KernelValue::U32(2)),
                ("result_kind".into(), KernelValue::U32(3)),
                (
                    "contract_version".into(),
                    KernelValue::U32(query_plan::QUERY_PLAN_CONTRACT_VERSION),
                ),
            ],
        }),
        KernelValue::Struct(KernelStructValue {
            name: "DispatchRecordContract".into(),
            fields: vec![
                ("backend".into(), KernelValue::U32(1)),
                ("kernel".into(), KernelValue::U32(4)),
                ("item_kind".into(), KernelValue::U32(1)),
                ("result_kind".into(), KernelValue::U32(1)),
                (
                    "contract_version".into(),
                    KernelValue::U32(query_plan::QUERY_PLAN_CONTRACT_VERSION),
                ),
            ],
        }),
    ];
    let dispatch_bytes =
        portable_abi_encode_slice(&dispatch_abi, &dispatch_values).expect("encode dispatch slice");
    assert_eq!(
        dispatch_bytes.len(),
        portable_abi_array_stride(&dispatch_abi) as usize * dispatch_values.len()
    );
    let decoded_dispatch =
        portable_abi_decode_slice(&dispatch_abi, &dispatch_bytes, dispatch_values.len())
            .expect("decode dispatch slice");
    assert_eq!(decoded_dispatch, dispatch_values);

    let result_abi = portable_result_contract_abi(&query_plan::ResultRecordContract {
        result_kind: query_plan::QueryResultKind::Hit3,
        preserves_local_hit_context: true,
        stable_feature_id: true,
        stable_instance_id: true,
        stable_repeat_id: true,
    });
    let result_values = vec![KernelValue::Struct(KernelStructValue {
        name: "ResultRecordContract".into(),
        fields: vec![
            ("result_kind".into(), KernelValue::U32(3)),
            (
                "preserves_local_hit_context".into(),
                KernelValue::Bool(true),
            ),
            ("stable_feature_id".into(), KernelValue::Bool(true)),
            ("stable_instance_id".into(), KernelValue::Bool(true)),
            ("stable_repeat_id".into(), KernelValue::Bool(true)),
            (
                "contract_version".into(),
                KernelValue::U32(query_plan::QUERY_PLAN_CONTRACT_VERSION),
            ),
        ],
    })];
    let result_bytes =
        portable_abi_encode_slice(&result_abi, &result_values).expect("encode result slice");
    assert_eq!(
        result_bytes.len(),
        portable_abi_array_stride(&result_abi) as usize * result_values.len()
    );
    let decoded_result = portable_abi_decode_slice(&result_abi, &result_bytes, result_values.len())
        .expect("decode result slice");
    assert_eq!(decoded_result, result_values);

    let hit_abi = portable_builtin_record_abi("Hit3").expect("Hit3 abi");
    let transform = KernelValue::Struct(KernelStructValue {
        name: "Transform3".into(),
        fields: vec![
            (
                "matrix".into(),
                KernelValue::Mat4([
                    1.0, 0.0, 0.0, 0.0, //
                    0.0, 1.0, 0.0, 0.0, //
                    0.0, 0.0, 1.0, 0.0, //
                    0.5, -0.25, 1.0, 1.0,
                ]),
            ),
            (
                "inverse".into(),
                KernelValue::Mat4([
                    1.0, 0.0, 0.0, 0.0, //
                    0.0, 1.0, 0.0, 0.0, //
                    0.0, 0.0, 1.0, 0.0, //
                    -0.5, 0.25, -1.0, 1.0,
                ]),
            ),
        ],
    });
    let payload = KernelValue::Struct(KernelStructValue {
        name: "Payload".into(),
        fields: vec![
            ("entity_id".into(), KernelValue::U32(7)),
            ("material_id".into(), KernelValue::U32(8)),
            (
                "actor".into(),
                KernelValue::Struct(KernelStructValue {
                    name: "ActorHandle".into(),
                    fields: vec![
                        ("id".into(), KernelValue::U32(9)),
                        ("generation".into(), KernelValue::U32(1)),
                    ],
                }),
            ),
        ],
    });
    let hit_values = vec![KernelValue::Struct(KernelStructValue {
        name: "Hit3".into(),
        fields: vec![
            ("hit".into(), KernelValue::Bool(true)),
            ("distance".into(), KernelValue::F32(2.0)),
            ("position".into(), KernelValue::Vec3([0.0, 0.0, 1.0])),
            ("normal".into(), KernelValue::Vec3([0.0, 0.0, 1.0])),
            ("local_position".into(), KernelValue::Vec3([0.0, 0.0, 0.5])),
            ("local_normal".into(), KernelValue::Vec3([0.0, 0.0, 1.0])),
            ("shading_frame".into(), transform),
            ("steps".into(), KernelValue::I32(12)),
            ("feature_id".into(), KernelValue::U32(13)),
            ("instance_id".into(), KernelValue::U32(14)),
            ("repeat_id".into(), KernelValue::U32(15)),
            ("root_shape_id".into(), KernelValue::U32(16)),
            ("payload".into(), payload),
        ],
    })];
    let hit_bytes = portable_abi_encode_slice(&hit_abi, &hit_values).expect("encode hit slice");
    assert_eq!(
        hit_bytes.len(),
        portable_abi_array_stride(&hit_abi) as usize * hit_values.len()
    );
    let decoded_hits = portable_abi_decode_slice(&hit_abi, &hit_bytes, hit_values.len())
        .expect("decode hit slice");
    assert_eq!(decoded_hits, hit_values);
}

#[test]
fn portable_abi_emits_deterministic_wgsl_structs_and_rejects_runtime_value() {
    let hit3 = portable_builtin_record_abi("Hit3").expect("Hit3 abi");
    let scene_domain = portable_builtin_record_abi("SceneDomain").expect("SceneDomain abi");
    let point_direction =
        portable_builtin_record_abi("PointDirectionQuery").expect("PointDirectionQuery abi");
    let unit_query = portable_builtin_record_abi("UnitQuery").expect("UnitQuery abi");
    let dispatch = portable_dispatch_contract_abi(&query_plan::DispatchRecordContract {
        backend: query_plan::DispatchBackend::Wgsl,
        kernel: query_plan::InternalKernelKind::ShapeTraceCapture,
        item_kind: query_plan::QueryItemKind::RayQuery,
        result_kind: query_plan::QueryResultKind::Hit3,
    });
    let rendered = portable_abi_emit_wgsl_structs(&[
        dispatch.clone(),
        hit3.clone(),
        scene_domain.clone(),
        point_direction.clone(),
        unit_query.clone(),
    ])
    .expect("emit wgsl");
    let transform_index = rendered
        .find("struct Transform3")
        .expect("Transform3 in wgsl");
    let actor_index = rendered
        .find("struct ActorHandle")
        .expect("ActorHandle in wgsl");
    let payload_index = rendered.find("struct Payload").expect("Payload in wgsl");
    let hit_index = rendered.find("struct Hit3").expect("Hit3 in wgsl");
    let dispatch_index = rendered
        .find("struct DispatchRecordContract")
        .expect("DispatchRecordContract in wgsl");
    let spatial_index = rendered
        .find("struct SpatialDomainContract")
        .expect("SpatialDomainContract in wgsl");
    let surface_domain_index = rendered
        .find("struct SurfaceDomainContract")
        .expect("SurfaceDomainContract in wgsl");
    let participants_index = rendered
        .find("struct ParticipantDomainContract")
        .expect("ParticipantDomainContract in wgsl");
    let scene_domain_index = rendered
        .find("struct SceneDomain")
        .expect("SceneDomain in wgsl");
    let point_direction_index = rendered
        .find("struct PointDirectionQuery")
        .expect("PointDirectionQuery in wgsl");
    let unit_query_index = rendered
        .find("struct UnitQuery")
        .expect("UnitQuery in wgsl");
    assert!(transform_index < hit_index);
    assert!(actor_index < payload_index);
    assert!(payload_index < hit_index);
    assert!(hit_index < dispatch_index || dispatch_index < hit_index);
    assert!(spatial_index < scene_domain_index);
    assert!(surface_domain_index < scene_domain_index);
    assert!(participants_index < scene_domain_index);
    assert!(point_direction_index < rendered.len());
    assert!(unit_query_index < rendered.len());
    assert!(rendered.contains("hit: u32,") || rendered.contains("hit: u32"));
    assert!(rendered.contains("struct UnitQuery {\n  _unit: u32,\n}"));

    let err = portable_abi_emit_wgsl_structs(&[PortableAbiType::Value]).expect_err("Value reject");
    assert_eq!(err, PortableAbiError::UnsupportedValueType);
}
