use wrela::artifact_layout::PhysicalLayoutStrategy;
use wrela::execution_policy::{
    PresentationExecutionPolicy, QueryExecutionPolicy, RayBudgetPolicy, RequiredGuaranteeClass,
    SelectedMethodClass,
};
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
use wrela::presentation_contract::{
    AttachmentClearPolicy, AttachmentLifetime, FrameAttachmentContract, FrameContract,
    LightingContract, PresentationObservabilityProfile, RealtimeQualityContract,
    RealtimeQualityTier,
};
use wrela::presentation_exec::resources::{
    allocate_attachment_resources_with_history_and_strategy,
    frame_attachment_layout_plan_with_strategy,
};
use wrela::presentation_exec::{
    allocate_frame_attachment_resources, allocate_frame_attachment_resources_with_history,
    frame_attachment_layout,
};
use wrela::query_plan;
use wrela::scene_ir;

fn test_quality_contract() -> RealtimeQualityContract {
    RealtimeQualityContract::named(RealtimeQualityTier::Realtime60)
}

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

    assert_eq!(portable_abi_layout(&spatial).size, 4);
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
    assert_eq!(portable_abi_field_offset(fields, 2), 8);
    assert_eq!(portable_abi_field_offset(fields, 3), 12);

    let layout = portable_abi_layout(&scene_domain);
    assert_eq!(layout.size, 20);
    assert_eq!(layout.align, 4);
}

#[test]
fn execution_policy_records_have_stable_portable_layouts() {
    let required_guarantee =
        portable_builtin_record_abi("RequiredGuaranteeClass").expect("RequiredGuaranteeClass abi");
    let selected_method =
        portable_builtin_record_abi("SelectedMethodClass").expect("SelectedMethodClass abi");
    let ray_budget = portable_builtin_record_abi("RayBudgetPolicy").expect("RayBudgetPolicy abi");
    let query_policy =
        portable_builtin_record_abi("QueryExecutionPolicy").expect("QueryExecutionPolicy abi");
    let presentation_policy = portable_builtin_record_abi("PresentationExecutionPolicy")
        .expect("PresentationExecutionPolicy abi");

    let PortableAbiType::Struct {
        fields: required_guarantee_fields,
        ..
    } = &required_guarantee
    else {
        panic!("RequiredGuaranteeClass should lower to a struct ABI");
    };
    assert_eq!(
        required_guarantee_fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        vec!["id"]
    );
    assert_eq!(portable_abi_layout(&required_guarantee).size, 4);
    assert_eq!(portable_abi_layout(&required_guarantee).align, 4);

    let PortableAbiType::Struct {
        fields: selected_method_fields,
        ..
    } = &selected_method
    else {
        panic!("SelectedMethodClass should lower to a struct ABI");
    };
    assert_eq!(
        selected_method_fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        vec!["id"]
    );
    assert_eq!(portable_abi_layout(&selected_method).size, 4);
    assert_eq!(portable_abi_layout(&selected_method).align, 4);

    let PortableAbiType::Struct {
        fields: ray_budget_fields,
        ..
    } = &ray_budget
    else {
        panic!("RayBudgetPolicy should lower to a struct ABI");
    };
    assert_eq!(
        ray_budget_fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        vec!["max_distance", "min_step", "hit_epsilon", "max_steps"]
    );
    assert_eq!(portable_abi_layout(&ray_budget).size, 16);
    assert_eq!(portable_abi_layout(&ray_budget).align, 4);

    let PortableAbiType::Struct {
        fields: query_policy_fields,
        ..
    } = &query_policy
    else {
        panic!("QueryExecutionPolicy should lower to a struct ABI");
    };
    assert_eq!(
        query_policy_fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        vec![
            "backend_preference",
            "required_guarantee",
            "selected_method",
            "ray_budget_enabled",
            "ray_budget"
        ]
    );
    assert_eq!(portable_abi_layout(&query_policy).size, 32);
    assert_eq!(portable_abi_layout(&query_policy).align, 4);

    let PortableAbiType::Struct {
        fields: presentation_policy_fields,
        ..
    } = &presentation_policy
    else {
        panic!("PresentationExecutionPolicy should lower to a struct ABI");
    };
    assert_eq!(
        presentation_policy_fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        vec!["required_guarantee", "selected_method", "primary_rays"]
    );
    assert_eq!(portable_abi_layout(&presentation_policy).size, 24);
    assert_eq!(portable_abi_layout(&presentation_policy).align, 4);

    let query_policy = QueryExecutionPolicy {
        backend_preference: wrela::query_contract::DispatchBackend::Cpu,
        required_guarantee: RequiredGuaranteeClass::ConservativeNoFalseMiss,
        selected_method: SelectedMethodClass::ConservativeSolver,
        ray_budget: Some(RayBudgetPolicy {
            max_distance: 10.0,
            min_step: 0.5,
            hit_epsilon: 0.25,
            max_steps: 64,
        }),
    };
    let query_policy_clone = query_policy;
    assert_eq!(query_policy, query_policy_clone);
    let query_hash = {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        query_policy.hash(&mut hasher);
        hasher.finish()
    };
    let query_clone_hash = {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        query_policy_clone.hash(&mut hasher);
        hasher.finish()
    };
    assert_eq!(query_hash, query_clone_hash);

    let presentation_policy = PresentationExecutionPolicy {
        required_guarantee: RequiredGuaranteeClass::Exact,
        selected_method: SelectedMethodClass::ExactOracle,
        primary_rays: RayBudgetPolicy {
            max_distance: 8.0,
            min_step: 0.02,
            hit_epsilon: 0.0005,
            max_steps: 96,
        },
    };
    let presentation_policy_clone = presentation_policy;
    assert_eq!(presentation_policy, presentation_policy_clone);
    let presentation_hash = {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        presentation_policy.hash(&mut hasher);
        hasher.finish()
    };
    let presentation_clone_hash = {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        presentation_policy_clone.hash(&mut hasher);
        hasher.finish()
    };
    assert_eq!(presentation_hash, presentation_clone_hash);
    assert_eq!(
        SelectedMethodClass::ExactOracle,
        SelectedMethodClass::ExactOracle
    );
}

#[test]
fn view_and_frame_state_records_have_stable_portable_layouts() {
    let viewport = portable_builtin_record_abi("Viewport").unwrap();
    let view_state = portable_builtin_record_abi("ViewState").unwrap();
    let snapshot_epoch = portable_builtin_record_abi("SnapshotEpoch").unwrap();
    let presentation_frame = portable_builtin_record_abi("PresentationFrame").unwrap();
    let simulation_tick = portable_builtin_record_abi("SimulationTick").unwrap();
    let wall_clock_stamp = portable_builtin_record_abi("WallClockStamp").unwrap();
    let transition_change_summary = portable_builtin_record_abi("TransitionChangeSummary").unwrap();
    let observer_time = portable_builtin_record_abi("ObserverTime").unwrap();
    let snapshot_transition_context =
        portable_builtin_record_abi("SnapshotTransitionContext").unwrap();
    let frame_state = portable_builtin_record_abi("FrameState").unwrap();

    let PortableAbiType::Struct {
        fields: viewport_fields,
        ..
    } = &viewport
    else {
        panic!("Viewport should lower to a struct ABI");
    };
    assert_eq!(
        viewport_fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        vec!["width", "height"]
    );
    assert_eq!(portable_abi_field_offset(viewport_fields, 0), 0);
    assert_eq!(portable_abi_field_offset(viewport_fields, 1), 4);
    assert_eq!(portable_abi_layout(&viewport).size, 8);
    assert_eq!(portable_abi_layout(&viewport).align, 4);

    let PortableAbiType::Struct {
        fields: view_fields,
        ..
    } = &view_state
    else {
        panic!("ViewState should lower to a struct ABI");
    };
    assert_eq!(
        view_fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        vec![
            "camera",
            "previous_camera",
            "viewport",
            "previous_viewport",
            "jitter",
            "previous_jitter"
        ]
    );
    assert_eq!(portable_abi_field_offset(view_fields, 0), 0);
    assert_eq!(portable_abi_field_offset(view_fields, 1), 48);
    assert_eq!(portable_abi_field_offset(view_fields, 2), 96);
    assert_eq!(portable_abi_field_offset(view_fields, 3), 104);
    assert_eq!(portable_abi_field_offset(view_fields, 4), 112);
    assert_eq!(portable_abi_field_offset(view_fields, 5), 120);
    assert_eq!(portable_abi_layout(&view_state).size, 128);
    assert_eq!(portable_abi_layout(&view_state).align, 16);

    let PortableAbiType::Struct {
        fields: snapshot_epoch_fields,
        ..
    } = &snapshot_epoch
    else {
        panic!("SnapshotEpoch should lower to a struct ABI");
    };
    assert_eq!(
        snapshot_epoch_fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        vec!["epoch"]
    );
    assert_eq!(portable_abi_field_offset(snapshot_epoch_fields, 0), 0);
    assert_eq!(portable_abi_layout(&snapshot_epoch).size, 4);
    assert_eq!(portable_abi_layout(&snapshot_epoch).align, 4);

    let PortableAbiType::Struct {
        fields: presentation_frame_fields,
        ..
    } = &presentation_frame
    else {
        panic!("PresentationFrame should lower to a struct ABI");
    };
    assert_eq!(
        presentation_frame_fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        vec!["index"]
    );
    assert_eq!(portable_abi_field_offset(presentation_frame_fields, 0), 0);
    assert_eq!(portable_abi_layout(&presentation_frame).size, 4);
    assert_eq!(portable_abi_layout(&presentation_frame).align, 4);

    let PortableAbiType::Struct {
        fields: simulation_tick_fields,
        ..
    } = &simulation_tick
    else {
        panic!("SimulationTick should lower to a struct ABI");
    };
    assert_eq!(
        simulation_tick_fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        vec!["tick"]
    );
    assert_eq!(portable_abi_field_offset(simulation_tick_fields, 0), 0);
    assert_eq!(portable_abi_layout(&simulation_tick).size, 4);
    assert_eq!(portable_abi_layout(&simulation_tick).align, 4);

    let PortableAbiType::Struct {
        fields: wall_clock_stamp_fields,
        ..
    } = &wall_clock_stamp
    else {
        panic!("WallClockStamp should lower to a struct ABI");
    };
    assert_eq!(
        wall_clock_stamp_fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        vec!["seconds"]
    );
    assert_eq!(portable_abi_field_offset(wall_clock_stamp_fields, 0), 0);
    assert_eq!(portable_abi_layout(&wall_clock_stamp).size, 4);
    assert_eq!(portable_abi_layout(&wall_clock_stamp).align, 4);

    let PortableAbiType::Struct {
        fields: transition_change_summary_fields,
        ..
    } = &transition_change_summary
    else {
        panic!("TransitionChangeSummary should lower to a struct ABI");
    };
    assert_eq!(
        transition_change_summary_fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        vec![
            "change_class",
            "compatible",
            "topology_changed",
            "identity_changed"
        ]
    );
    assert_eq!(
        portable_abi_field_offset(transition_change_summary_fields, 0),
        0
    );
    assert_eq!(
        portable_abi_field_offset(transition_change_summary_fields, 1),
        4
    );
    assert_eq!(
        portable_abi_field_offset(transition_change_summary_fields, 2),
        8
    );
    assert_eq!(
        portable_abi_field_offset(transition_change_summary_fields, 3),
        12
    );
    assert_eq!(portable_abi_layout(&transition_change_summary).size, 16);
    assert_eq!(portable_abi_layout(&transition_change_summary).align, 4);

    let PortableAbiType::Struct {
        fields: observer_time_fields,
        ..
    } = &observer_time
    else {
        panic!("ObserverTime should lower to a struct ABI");
    };
    assert_eq!(
        observer_time_fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        vec![
            "presentation_frame",
            "previous_presentation_frame",
            "simulation_tick",
            "wall_clock_stamp",
            "delta_seconds"
        ]
    );
    assert_eq!(portable_abi_field_offset(observer_time_fields, 0), 0);
    assert_eq!(portable_abi_field_offset(observer_time_fields, 1), 4);
    assert_eq!(portable_abi_field_offset(observer_time_fields, 2), 8);
    assert_eq!(portable_abi_field_offset(observer_time_fields, 3), 12);
    assert_eq!(portable_abi_field_offset(observer_time_fields, 4), 16);
    assert_eq!(portable_abi_layout(&observer_time).size, 20);
    assert_eq!(portable_abi_layout(&observer_time).align, 4);

    let PortableAbiType::Struct {
        fields: snapshot_transition_fields,
        ..
    } = &snapshot_transition_context
    else {
        panic!("SnapshotTransitionContext should lower to a struct ABI");
    };
    assert_eq!(
        snapshot_transition_fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        vec![
            "current_snapshot_epoch",
            "previous_snapshot_epoch",
            "has_change_summary",
            "change_summary"
        ]
    );
    assert_eq!(portable_abi_field_offset(snapshot_transition_fields, 0), 0);
    assert_eq!(portable_abi_field_offset(snapshot_transition_fields, 1), 4);
    assert_eq!(portable_abi_field_offset(snapshot_transition_fields, 2), 8);
    assert_eq!(portable_abi_field_offset(snapshot_transition_fields, 3), 12);
    assert_eq!(portable_abi_layout(&snapshot_transition_context).size, 28);
    assert_eq!(portable_abi_layout(&snapshot_transition_context).align, 4);

    let PortableAbiType::Struct {
        fields: frame_fields,
        ..
    } = &frame_state
    else {
        panic!("FrameState should lower to a struct ABI");
    };
    assert_eq!(
        frame_fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        vec![
            "view",
            "frame_index",
            "previous_frame_index",
            "delta_seconds",
            "history_reset",
            "observer_time",
            "snapshot_transition"
        ]
    );
    assert_eq!(portable_abi_field_offset(frame_fields, 0), 0);
    assert_eq!(portable_abi_field_offset(frame_fields, 1), 128);
    assert_eq!(portable_abi_field_offset(frame_fields, 2), 132);
    assert_eq!(portable_abi_field_offset(frame_fields, 3), 136);
    assert_eq!(portable_abi_field_offset(frame_fields, 4), 140);
    assert_eq!(portable_abi_field_offset(frame_fields, 5), 144);
    assert_eq!(portable_abi_field_offset(frame_fields, 6), 164);
    assert_eq!(portable_abi_layout(&frame_state).size, 192);
    assert_eq!(portable_abi_layout(&frame_state).align, 16);
}

#[test]
fn motion_vector_record_has_stable_portable_layout() {
    let motion_vector = portable_builtin_record_abi("MotionVector").unwrap();
    let PortableAbiType::Struct { fields, .. } = &motion_vector else {
        panic!("MotionVector should lower to a struct ABI");
    };
    assert_eq!(
        fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        vec!["delta_pixels", "previous_sample", "valid", "disoccluded"]
    );
    assert_eq!(portable_abi_field_offset(fields, 0), 0);
    assert_eq!(portable_abi_field_offset(fields, 1), 8);
    assert_eq!(portable_abi_field_offset(fields, 2), 16);
    assert_eq!(portable_abi_field_offset(fields, 3), 20);
    assert_eq!(portable_abi_layout(&motion_vector).size, 24);
    assert_eq!(portable_abi_layout(&motion_vector).align, 8);
}

#[test]
fn screen_sample_query_records_canonical_pixel_uv_and_view_ray() {
    let sample = portable_builtin_record_abi("ScreenSampleQuery").unwrap();
    let PortableAbiType::Struct { fields, .. } = &sample else {
        panic!("ScreenSampleQuery should lower to a struct ABI");
    };
    assert_eq!(
        fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        vec!["pixel", "uv", "ray"]
    );
    assert_eq!(portable_abi_field_offset(fields, 0), 0);
    assert_eq!(portable_abi_field_offset(fields, 1), 8);
    assert_eq!(portable_abi_field_offset(fields, 2), 16);
    assert_eq!(portable_abi_layout(&sample).size, 64);
    assert_eq!(portable_abi_layout(&sample).align, 16);
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
fn presentation_attachment_layouts_use_portable_row_major_storage() {
    let frame = FrameContract {
        outputs: vec![
            FrameAttachmentContract::primary_hit("primary_hit"),
            FrameAttachmentContract::depth("depth"),
            FrameAttachmentContract::world_normal("world_normal"),
        ],
        primary_hit: None,
        temporal: None,
        quality: test_quality_contract(),
        lighting: LightingContract::legacy_preview(false),
        observability: PresentationObservabilityProfile::preview_compatibility(),
    };
    let primary_hit = frame_attachment_layout(&frame, &frame.outputs[0], 4, 3).unwrap();
    let depth = frame_attachment_layout(&frame, &frame.outputs[1], 4, 3).unwrap();
    let world_normal = frame_attachment_layout(&frame, &frame.outputs[2], 4, 3).unwrap();

    assert_eq!(
        primary_hit.element_stride,
        portable_abi_array_stride(&primary_hit.element_abi)
    );
    assert_eq!(primary_hit.total_size, primary_hit.element_stride * 12);
    assert_eq!(primary_hit.wgsl_storage_type, "Hit3");

    assert_eq!(depth.element_stride, 4);
    assert_eq!(depth.total_size, 48);
    assert_eq!(depth.wgsl_storage_type, "f32");

    assert_eq!(world_normal.element_stride, 16);
    assert_eq!(world_normal.total_size, 192);
    assert_eq!(world_normal.wgsl_storage_type, "vec3<f32>");
}

#[test]
fn presentation_attachment_resources_allocate_dense_buffers_for_all_declared_outputs() {
    let frame = FrameContract {
        outputs: vec![
            FrameAttachmentContract::primary_hit("primary_hit"),
            FrameAttachmentContract::depth("depth"),
            FrameAttachmentContract::world_normal("world_normal"),
        ],
        primary_hit: None,
        temporal: None,
        quality: test_quality_contract(),
        lighting: LightingContract::legacy_preview(false),
        observability: PresentationObservabilityProfile::preview_compatibility(),
    };
    let resources = allocate_frame_attachment_resources(&frame, 2, 2).unwrap();
    assert_eq!(resources.width, 2);
    assert_eq!(resources.height, 2);
    assert_eq!(resources.attachments.len(), 3);
    assert_eq!(
        resources
            .attachment("primary_hit")
            .expect("primary_hit")
            .bytes
            .len(),
        4 * portable_abi_array_stride(&portable_builtin_record_abi("Hit3").expect("Hit3 abi"))
            as usize
    );
    assert_eq!(
        resources.attachment("depth").expect("depth").bytes.len(),
        16
    );
    assert_eq!(
        resources
            .attachment("world_normal")
            .expect("world_normal")
            .bytes
            .len(),
        64
    );
}

#[test]
fn presentation_attachment_layouts_apply_resolution_scale_to_physical_dimensions() {
    let mut half_depth = FrameAttachmentContract::depth("half_depth");
    half_depth.resolution = wrela::presentation_contract::AttachmentResolutionClass::HalfViewport;
    half_depth.scale = wrela::presentation_contract::AttachmentResolutionScale::half();
    let frame = FrameContract {
        outputs: vec![half_depth.clone()],
        primary_hit: None,
        temporal: None,
        quality: test_quality_contract(),
        lighting: LightingContract::legacy_preview(false),
        observability: PresentationObservabilityProfile::preview_compatibility(),
    };
    let layout = frame_attachment_layout(&frame, &half_depth, 5, 3).unwrap();
    assert_eq!(layout.width, 3);
    assert_eq!(layout.height, 2);
    assert_eq!(layout.total_size, layout.element_stride * 6);
}

#[test]
fn presentation_attachment_resources_seed_semantic_defaults_and_preserve_history() {
    let primary_hit = FrameAttachmentContract::primary_hit("primary_hit");
    let mut history_depth = FrameAttachmentContract::depth("history_depth");
    history_depth.lifetime = AttachmentLifetime::HistorySlot(0);
    history_depth.clear_policy = AttachmentClearPolicy::PreservePrevious;

    let history_seed = FrameContract {
        outputs: vec![{
            let mut attachment = history_depth.clone();
            attachment.clear_policy = AttachmentClearPolicy::Zero;
            attachment
        }],
        primary_hit: None,
        temporal: None,
        quality: test_quality_contract(),
        lighting: LightingContract::legacy_preview(false),
        observability: PresentationObservabilityProfile::preview_compatibility(),
    };
    let mut previous = allocate_frame_attachment_resources(&history_seed, 2, 2).unwrap();
    let prior_pattern = vec![7u8; previous.attachment("history_depth").unwrap().bytes.len()];
    previous
        .attachment_mut("history_depth")
        .unwrap()
        .bytes
        .copy_from_slice(&prior_pattern);

    let frame = FrameContract {
        outputs: vec![primary_hit.clone(), history_depth.clone()],
        primary_hit: None,
        temporal: None,
        quality: test_quality_contract(),
        lighting: LightingContract::legacy_preview(false),
        observability: PresentationObservabilityProfile::preview_compatibility(),
    };
    let resources =
        allocate_frame_attachment_resources_with_history(&frame, 2, 2, Some(&previous)).unwrap();
    let primary_hit_values = resources.decode_attachment("primary_hit").unwrap();
    assert_eq!(primary_hit_values.len(), 4);
    let KernelValue::Struct(KernelStructValue { fields, .. }) = &primary_hit_values[0] else {
        panic!("expected Hit3 semantic default");
    };
    assert!(
        fields
            .iter()
            .any(|(name, value)| name == "hit" && matches!(value, KernelValue::Bool(false)))
    );
    assert!(fields.iter().any(|(name, value)| {
        name == "distance" && matches!(value, KernelValue::F32(distance) if distance.is_infinite())
    }));
    assert_eq!(
        resources.attachment("history_depth").unwrap().bytes,
        prior_pattern
    );
}

#[test]
fn presentation_attachment_resources_can_materialize_a_row_aligned_layout_plan() {
    let frame = FrameContract {
        outputs: vec![FrameAttachmentContract::depth("depth")],
        primary_hit: None,
        temporal: None,
        quality: test_quality_contract(),
        lighting: LightingContract::legacy_preview(false),
        observability: PresentationObservabilityProfile::preview_compatibility(),
    };

    let plan = frame_attachment_layout_plan_with_strategy(
        &frame,
        &frame.outputs[0],
        3,
        2,
        PhysicalLayoutStrategy::RowAligned { row_alignment: 32 },
    )
    .unwrap();
    assert_eq!(
        plan.physical.strategy,
        PhysicalLayoutStrategy::RowAligned { row_alignment: 32 }
    );
    assert_eq!(plan.physical.width, 3);
    assert_eq!(plan.physical.height, 2);
    assert_eq!(plan.physical.row_stride, 32);
    assert_eq!(plan.physical.total_size, 64);

    let resources = allocate_attachment_resources_with_history_and_strategy(
        &frame,
        3,
        2,
        None,
        PhysicalLayoutStrategy::RowAligned { row_alignment: 32 },
    )
    .unwrap();
    let depth = resources.attachment("depth").unwrap();
    assert_eq!(
        depth.layout.plan.physical.strategy,
        PhysicalLayoutStrategy::RowAligned { row_alignment: 32 }
    );
    assert_eq!(depth.bytes.len(), 64);
    assert_eq!(depth.element_count(), 6);
    assert_eq!(depth.decode(0).unwrap(), KernelValue::F32(f32::INFINITY));
    assert_eq!(depth.decode(5).unwrap(), KernelValue::F32(f32::INFINITY));
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
        evidence_summary: query_plan::SemanticEvidenceSummary::artifact_bound(false),
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
        transition: None,
    });
    let PortableAbiType::Struct { fields, .. } = abi else {
        panic!("artifact contract abi should lower to a struct");
    };
    assert_eq!(fields[3].name.as_str(), "evidence_summary");
    let PortableAbiType::Struct {
        fields: evidence_fields,
        ..
    } = &fields[3].ty
    else {
        panic!("artifact evidence summary should lower to a struct");
    };
    assert_eq!(
        evidence_fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        vec![
            "subject",
            "distance",
            "support",
            "differential",
            "identity",
            "temporal",
            "origin",
            "scope",
            "refinement_path",
        ]
    );
    let PortableAbiType::Struct {
        fields: subject_fields,
        ..
    } = &evidence_fields[0].ty
    else {
        panic!("artifact evidence subject should lower to a fixed text record");
    };
    assert_eq!(
        subject_fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        vec!["len", "code_units"]
    );
    match &subject_fields[1].ty {
        PortableAbiType::Array(inner, len) => {
            assert_eq!(inner.as_ref(), &PortableAbiType::U32);
            assert!(
                *len >= 64,
                "subject text capacity should preserve contract ids"
            );
        }
        other => panic!("expected fixed code-unit array for evidence subject, got {other:?}"),
    }
    let PortableAbiType::Struct {
        fields: distance_fields,
        ..
    } = &evidence_fields[1].ty
    else {
        panic!("artifact distance evidence should lower to a struct");
    };
    assert_eq!(
        distance_fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        vec![
            "semantics",
            "lipschitz",
            "interval_bounds",
            "analytic_intersection",
            "origin",
            "scope",
            "refinement_path",
        ]
    );
    let PortableAbiType::Struct {
        fields: support_fields,
        ..
    } = &evidence_fields[2].ty
    else {
        panic!("artifact support evidence should lower to a struct");
    };
    assert_eq!(
        support_fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        vec![
            "support_class",
            "semantics",
            "conservative_bounds",
            "lower_bound_pruning",
            "can_coarse_prune",
            "opaque_boundary",
            "origin",
            "scope",
            "refinement_path",
        ]
    );
    let PortableAbiType::Struct {
        fields: refinement_path_fields,
        ..
    } = &evidence_fields[8].ty
    else {
        panic!("artifact evidence refinement path should lower to a struct");
    };
    assert_eq!(
        refinement_path_fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        vec!["len", "entries"]
    );
    match &refinement_path_fields[1].ty {
        PortableAbiType::Array(inner, len) => {
            assert!(
                *len >= 8,
                "refinement path capacity should preserve multi-step weakening histories"
            );
            let PortableAbiType::Struct {
                fields: step_fields,
                ..
            } = inner.as_ref()
            else {
                panic!("refinement path entries should lower to a struct");
            };
            assert_eq!(
                step_fields
                    .iter()
                    .map(|field| field.name.as_str())
                    .collect::<Vec<_>>(),
                vec!["class", "kind", "detail"]
            );
        }
        other => panic!("expected refinement path entry array, got {other:?}"),
    }
    let layout = portable_abi_layout(&PortableAbiType::Struct {
        name: "ArtifactContract".into(),
        class_id: 0,
        fields: fields.clone(),
    });
    assert_eq!(layout.align, 4);
    assert_eq!(portable_abi_field_offset(&fields, 0), 0);
    assert!(
        portable_abi_field_offset(&fields, 4) > 36,
        "full evidence summaries should occupy more ABI space than the legacy stub"
    );
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
    let required_guarantee =
        portable_builtin_record_abi("RequiredGuaranteeClass").expect("RequiredGuaranteeClass abi");
    let selected_method =
        portable_builtin_record_abi("SelectedMethodClass").expect("SelectedMethodClass abi");
    let ray_budget = portable_builtin_record_abi("RayBudgetPolicy").expect("RayBudgetPolicy abi");
    let query_policy =
        portable_builtin_record_abi("QueryExecutionPolicy").expect("QueryExecutionPolicy abi");
    let presentation_policy = portable_builtin_record_abi("PresentationExecutionPolicy")
        .expect("PresentationExecutionPolicy abi");
    let point_direction =
        portable_builtin_record_abi("PointDirectionQuery").expect("PointDirectionQuery abi");
    let unit_query = portable_builtin_record_abi("UnitQuery").expect("UnitQuery abi");
    let frame_state = portable_builtin_record_abi("FrameState").expect("FrameState abi");
    let screen_sample =
        portable_builtin_record_abi("ScreenSampleQuery").expect("ScreenSampleQuery abi");
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
        required_guarantee.clone(),
        selected_method.clone(),
        ray_budget.clone(),
        query_policy.clone(),
        presentation_policy.clone(),
        point_direction.clone(),
        unit_query.clone(),
        frame_state.clone(),
        screen_sample.clone(),
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
    let guarantee_index = rendered
        .find("struct RequiredGuaranteeClass")
        .expect("RequiredGuaranteeClass in wgsl");
    let ray_budget_index = rendered
        .find("struct RayBudgetPolicy")
        .expect("RayBudgetPolicy in wgsl");
    let query_policy_index = rendered
        .find("struct QueryExecutionPolicy")
        .expect("QueryExecutionPolicy in wgsl");
    let presentation_policy_index = rendered
        .find("struct PresentationExecutionPolicy")
        .expect("PresentationExecutionPolicy in wgsl");
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
    let camera_index = rendered.find("struct Camera").expect("Camera in wgsl");
    let viewport_index = rendered.find("struct Viewport").expect("Viewport in wgsl");
    let view_state_index = rendered
        .find("struct ViewState")
        .expect("ViewState in wgsl");
    let observer_time_index = rendered
        .find("struct ObserverTime")
        .expect("ObserverTime in wgsl");
    let snapshot_transition_index = rendered
        .find("struct SnapshotTransitionContext")
        .expect("SnapshotTransitionContext in wgsl");
    let frame_state_index = rendered
        .find("struct FrameState")
        .expect("FrameState in wgsl");
    let screen_sample_index = rendered
        .find("struct ScreenSampleQuery")
        .expect("ScreenSampleQuery in wgsl");
    assert!(transform_index < hit_index);
    assert!(actor_index < payload_index);
    assert!(payload_index < hit_index);
    assert!(hit_index < dispatch_index || dispatch_index < hit_index);
    assert!(guarantee_index < query_policy_index);
    assert!(guarantee_index < presentation_policy_index);
    assert!(rendered.contains("struct SelectedMethodClass"));
    assert!(ray_budget_index < query_policy_index);
    assert!(query_policy_index < presentation_policy_index);
    assert!(spatial_index < scene_domain_index);
    assert!(surface_domain_index < scene_domain_index);
    assert!(participants_index < scene_domain_index);
    assert!(point_direction_index < rendered.len());
    assert!(unit_query_index < rendered.len());
    assert!(camera_index < view_state_index);
    assert!(viewport_index < view_state_index);
    assert!(view_state_index < frame_state_index);
    assert!(observer_time_index < frame_state_index);
    assert!(snapshot_transition_index < frame_state_index);
    assert!(screen_sample_index < rendered.len());
    assert!(rendered.contains("ray: RayQuery"));
    assert!(rendered.contains("hit: u32,") || rendered.contains("hit: u32"));
    assert!(rendered.contains("struct UnitQuery {\n  _unit: u32,\n}"));

    let err = portable_abi_emit_wgsl_structs(&[PortableAbiType::Value]).expect_err("Value reject");
    assert_eq!(err, PortableAbiError::UnsupportedValueType);
}
