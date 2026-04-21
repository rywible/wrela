use super::*;
use crate::artifact_layout::PhysicalLayoutStrategy;
use crate::presentation_contract::{
    FrameAttachmentContract, FrameContract, LightingContract, PresentationObservabilityProfile,
    RealtimeQualityContract, RealtimeQualityTier,
};
use crate::presentation_exec::resources::{
    AttachmentResource, frame_attachment_layout_plan_with_strategy,
};

fn test_frame_for_color() -> FrameContract {
    FrameContract {
        outputs: vec![FrameAttachmentContract::transient_color("color")],
        primary_hit: None,
        temporal: None,
        quality: RealtimeQualityContract::named(RealtimeQualityTier::Realtime60),
        lighting: LightingContract::legacy_preview(false),
        observability: PresentationObservabilityProfile::preview_compatibility(),
    }
}

#[test]
fn row_aligned_output_layout_packs_wgsl_results_into_attachment_plan() {
    let frame = test_frame_for_color();
    let layout = frame_attachment_layout_plan_with_strategy(
        &frame,
        &frame.outputs[0],
        3,
        2,
        PhysicalLayoutStrategy::RowAligned { row_alignment: 32 },
    )
    .expect("row-aligned layout plan");
    let input_values = vec![
        KernelValue::Vec3([1.0, 0.0, 0.0]),
        KernelValue::Vec3([0.0, 1.0, 0.0]),
        KernelValue::Vec3([0.0, 0.0, 1.0]),
        KernelValue::Vec3([1.0, 1.0, 0.0]),
        KernelValue::Vec3([1.0, 0.0, 1.0]),
        KernelValue::Vec3([0.0, 1.0, 1.0]),
    ];
    let mut gpu_runtime = GpuRuntimeMetrics::default();

    let dispatch = legacy_test_only_dispatch_linear_shader(
        &copy_vec3_shader_source(64, false).expect("copy vec3 shader"),
        &PortableAbiType::Vec3,
        &input_values,
        &layout,
        64,
        &mut gpu_runtime,
    )
    .expect("row-aligned wgsl dispatch");
    let resource = AttachmentResource {
        layout: layout.materialize(),
        bytes: dispatch.bytes.into(),
    };

    assert_eq!(
        resource.bytes.len(),
        layout.physical.total_size as usize,
        "packed output should honor the physical layout plan"
    );
    for (index, expected) in input_values.iter().enumerate() {
        assert_eq!(
            resource.decode(index).expect("decode row-aligned output"),
            *expected
        );
    }
    for row in 0..layout.physical.height as usize {
        let row_start = row * layout.physical.row_stride as usize;
        let padding_start =
            row_start + layout.physical.width as usize * layout.physical.element_stride as usize;
        let padding_end = row_start + layout.physical.row_stride as usize;
        assert!(
            resource.bytes[padding_start..padding_end]
                .iter()
                .all(|byte| *byte == 0),
            "row padding should remain untouched by dense shader output"
        );
    }
}

#[test]
fn forced_chunking_preserves_row_aligned_output_and_reports_dispatch_count() {
    let frame = test_frame_for_color();
    let layout = frame_attachment_layout_plan_with_strategy(
        &frame,
        &frame.outputs[0],
        3,
        2,
        PhysicalLayoutStrategy::RowAligned { row_alignment: 32 },
    )
    .expect("row-aligned layout plan");
    let input_values = vec![
        KernelValue::Vec3([1.0, 0.0, 0.0]),
        KernelValue::Vec3([0.0, 1.0, 0.0]),
        KernelValue::Vec3([0.0, 0.0, 1.0]),
        KernelValue::Vec3([1.0, 1.0, 0.0]),
        KernelValue::Vec3([1.0, 0.0, 1.0]),
        KernelValue::Vec3([0.0, 1.0, 1.0]),
    ];
    let mut gpu_runtime = GpuRuntimeMetrics::default();

    let dispatch = legacy_test_only_dispatch_linear_shader_with_chunk_limit(
        &copy_vec3_shader_source(64, false).expect("copy vec3 shader"),
        &PortableAbiType::Vec3,
        &input_values,
        &layout,
        64,
        Some(64),
        &mut gpu_runtime,
    )
    .expect("forced chunked wgsl dispatch");
    let resource = AttachmentResource {
        layout: layout.materialize(),
        bytes: dispatch.bytes.into(),
    };

    assert_eq!(dispatch.dispatch_count, 2);
    assert_eq!(
        gpu_runtime.transient_bind_group_creations, 1,
        "chunked presentation WGSL dispatches should reuse one bind group across chunks"
    );
    assert!(
        gpu_runtime.transient_buffer_creations <= 7,
        "chunked presentation WGSL dispatches should reuse persistent upload buffers, got {:?}",
        gpu_runtime
    );
    for (index, expected) in input_values.iter().enumerate() {
        assert_eq!(
            resource.decode(index).expect("decode row-aligned output"),
            *expected
        );
    }
}
