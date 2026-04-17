#[cfg(test)]
use crate::gpu_runtime::lock_shared_upload_arena;
use crate::gpu_runtime::{
    BufferPoolKey, GPU_RUNTIME_BIND_GROUP_COUNT, GPU_RUNTIME_PASS_BIND_GROUP_INDEX,
    GpuPassProfiler, GpuRuntimeMetrics, ReadbackReason, ReadbackRequest, shared_buffer_pool,
};
use crate::kernel::{
    KernelStructValue, KernelValue, interpret_batch_query, lower_batch_query_plan,
};
use crate::portable::{
    PortableAbiType, PortableStructField, portable_abi_decode_slice,
    portable_abi_emit_wgsl_structs, portable_builtin_record_abi,
};
#[cfg(test)]
use crate::portable::{portable_abi_array_stride, portable_abi_layout};
use crate::presentation_contract::{AttachmentLifetime, RealtimeRadianceMode};
use crate::presentation_exec::clipmap::{
    build_view_distance_clipmap_artifact, clipmap_pass_runtime,
};
use crate::presentation_exec::framegraph::{
    PresentationFramegraph, PresentationFramegraphError, PresentationFramegraphSubmission,
};
#[cfg(test)]
use crate::presentation_exec::resources::FrameAttachmentLayoutPlan;
use crate::presentation_exec::resources::{AttachmentResource, AttachmentResourceSet};
#[cfg(test)]
use crate::presentation_exec::temporal::temporal_resolve_kernel_values;
use crate::presentation_exec::temporal::{
    motion_resolve_assessment_summary, update_query_trace_continuation,
};
use crate::presentation_exec::{
    PassRuntimeStats, PresentationExecError, PresentationExecutionInput,
    PresentationExecutionResult, TileCullingStats, adjusted_ray_budget,
    allocate_execution_attachments, attachment_hit_work_items, build_frame_cost_report,
    build_temporal_history, build_tile_candidate_span_words, effective_plan_for_quality,
    encode_values_at_indices, execute_batch_contract, expect_array, expect_f32, expect_struct,
    field, frame_state_components, full_attachment_byte_size, internal_resolution_viewport,
    lighting_inputs_value, participant_query_work_items,
    participant_query_work_items_without_screen_samples, presentation_metrics,
    primary_hit_miss_value, resolved_quality_state, runtime_primary_solver_summary,
    select_presentation_workgroup_size, tile_candidate_dispatch_packets,
    tile_candidate_packet_fragment_count, tile_candidate_packet_sample_count, tile_candidate_stats,
    tile_culling_mask,
};
#[cfg(test)]
use crate::presentation_exec::{expect_vec3, shade_lookup_value};
use crate::presentation_plan::{
    CompositeColorPassContract, MotionResolvePassContract, ParticipantsResolvePassContract,
    PresentationPassKind, PresentationPlan, ShadePrimaryPassContract, SurfaceResolvePassContract,
    TemporalResolvePassContract,
};
use crate::query_exec::QueryExecContext;
use crate::query_exec::cpu::{default_medium, default_surface};
use crate::query_exec::execute_batch_query_with_solver_mode_with_snapshot_on;
use crate::query_exec::wgsl::encode_slice;
#[cfg(test)]
use crate::query_exec::wgsl::{compiled_pipeline, legacy_test_only_readback_storage_buffer};
use crate::query_exec::wgsl::{encode_value, native_wgpu_context};
use crate::query_exec::{BatchQueryExecutionTrace, QueryExecutionObservability};
use crate::query_plan::{BatchQueryPlan, DispatchBackend};
use smol_str::SmolStr;
use std::borrow::Cow;
use std::collections::{BTreeSet, HashMap};
use std::ops::Range;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use super::gpu_primary::{PrimaryVisibilityGpuDispatch, prepare_primary_visibility_dispatch};

#[path = "gpu_post.rs"]
mod gpu_post;

#[cfg(test)]
struct LegacyTestOnlyLinearShaderDispatchResult {
    bytes: Vec<u8>,
    dispatch_count: u32,
}

#[cfg(test)]
struct LegacyTestOnlyTemporalResolveDispatchResult {
    consumed_count: u32,
    dispatch_count: u32,
}

fn acquire_presentation_upload_buffer(
    native: &crate::query_exec::wgsl::NativeWgpuContext,
    size: u64,
    usage: wgpu::BufferUsages,
    label: Option<&str>,
    gpu_runtime: &mut GpuRuntimeMetrics,
) -> (wgpu::Buffer, BufferPoolKey) {
    let key = BufferPoolKey::new(size.max(4), usage);
    let pool = shared_buffer_pool(native.limit_request);
    let (buffer, created) = pool
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .acquire(&native.device, key, label);
    if created {
        gpu_runtime.transient_buffer_creations =
            gpu_runtime.transient_buffer_creations.saturating_add(1);
    }
    (buffer, key)
}

fn release_presentation_upload_buffers(
    native: &crate::query_exec::wgsl::NativeWgpuContext,
    buffers: impl IntoIterator<Item = (BufferPoolKey, wgpu::Buffer)>,
) {
    let pool = shared_buffer_pool(native.limit_request);
    let mut guard = pool.lock().unwrap_or_else(|poison| poison.into_inner());
    for (key, buffer) in buffers {
        guard.release(key, buffer);
    }
}

pub(super) fn execute_plan(
    ctx: &QueryExecContext,
    plan: &PresentationPlan,
    input: &PresentationExecutionInput,
) -> Result<PresentationExecutionResult, PresentationExecError> {
    let current_snapshot = &input.region_snapshot;
    let (camera, viewport, jitter_pixels) = frame_state_components(&input.frame_state)?;
    let quality = resolved_quality_state(plan, input);
    let effective_plan = effective_plan_for_quality(plan, &quality);
    let ray_budget = adjusted_ray_budget(input.execution_policy, &quality);
    let primary_viewport = internal_resolution_viewport(viewport, &quality);
    let mut attachments = allocate_execution_attachments(
        &effective_plan.frame,
        &input.frame_state,
        viewport.width,
        viewport.height,
        current_snapshot,
        input.history.as_ref(),
    )?;
    let native = native_wgpu_context().map_err(PresentationExecError::Query)?;
    let mut framegraph = PresentationFramegraph::from_plan_and_gpu_resources(
        effective_plan.clone(),
        attachments.clone(),
        native.clone(),
        (effective_plan.passes.len() as u32)
            .saturating_mul(6)
            .max(8),
    );
    let selected_workgroup_size = select_presentation_workgroup_size(&native.adapter_limits)?;
    let mut primary_solver_context = None;
    let mut continuation_counts = crate::presentation_exec::temporal::ContinuationCounts::default();
    let mut tile_cull = TileCullingStats::default();
    let mut tile_candidate = crate::presentation_exec::TileCandidateStats::default();
    let mut view_distance_clipmap = None;
    let mut candidate_table_active = false;
    let packet_scheduling_active = false;
    let mut gpu_runtime = GpuRuntimeMetrics::default();
    let mut surface_resolve_count = 0;
    let mut participant_resolve_count = 0;
    let mut pass_stats = Vec::new();
    let mut framegraph_exceptions = Vec::<String>::new();
    let mut pending_gpu_pass_ranges = Vec::<(usize, Range<usize>)>::new();
    let mut motion_counts_readback = None::<(wgpu::Buffer, String)>;
    let mut temporal_counts_readback = None::<(wgpu::Buffer, String)>;
    let mut explicitly_exported_attachments = BTreeSet::<SmolStr>::new();
    let mut primary_trace_seed = None::<(
        crate::query_contract::QueryContractId,
        crate::kernel::ir::KernelBatchQueryPlan,
        u32,
        SmolStr,
        SmolStr,
    )>;
    let mut primary_trace_dispatch = None::<PrimaryVisibilityGpuDispatch>;
    let mut primary_observability_buffer = None::<(wgpu::Buffer, u64)>;

    for pass in &effective_plan.passes {
        match &pass.kind {
            PresentationPassKind::GenerateScreenSamples { .. } => {}
            PresentationPassKind::PrimaryVisibility { contract } => {
                let batch_plan = lower_batch_query_plan(
                    &BatchQueryPlan::for_contract(
                        contract.query_contract,
                        DispatchBackend::Wgsl,
                        None,
                    )
                    .map_err(|message| {
                        PresentationExecError::UnsupportedPlan {
                            message: message.to_string(),
                        }
                    })?,
                );
                let cull_mask = tile_culling_mask(
                    ctx,
                    input,
                    camera,
                    primary_viewport,
                    effective_plan
                        .view
                        .compatibility_projection
                        .legacy_path_active,
                )?;
                let candidate_shape_names = cull_mask.as_ref().and_then(|mask| {
                    mask.candidate_table
                        .enabled
                        .then(|| mask.candidate_table.candidate_shapes.clone())
                });
                candidate_table_active = cull_mask
                    .as_ref()
                    .is_some_and(|mask| mask.candidate_table.enabled);
                let candidate_spans = if let Some(mask) = cull_mask.as_ref() {
                    let tile_candidate_packets = if mask.candidate_table.enabled {
                        tile_candidate_dispatch_packets(
                            &mask.candidate_table,
                            selected_workgroup_size,
                        )
                    } else {
                        Vec::new()
                    };
                    tile_candidate = tile_candidate_stats(
                        mask.active_samples.len(),
                        tile_candidate_packet_sample_count(&tile_candidate_packets),
                        tile_candidate_packet_fragment_count(
                            &tile_candidate_packets,
                            selected_workgroup_size,
                        ),
                        selected_workgroup_size,
                    );
                    build_tile_candidate_span_words(
                        &mask.candidate_table,
                        &mask.active_samples,
                        selected_workgroup_size,
                    )
                } else {
                    tile_candidate = tile_candidate_stats(
                        primary_viewport
                            .width
                            .saturating_mul(primary_viewport.height)
                            as usize,
                        primary_viewport
                            .width
                            .saturating_mul(primary_viewport.height)
                            as usize,
                        0,
                        selected_workgroup_size,
                    );
                    Vec::new()
                };
                if let Some(mask) = cull_mask.as_ref() {
                    tile_cull = mask.stats;
                }
                view_distance_clipmap = Some(build_view_distance_clipmap_artifact(
                    effective_plan.name.as_str(),
                    current_snapshot,
                    &crate::presentation_exec::frame_state_temporal_components(&input.frame_state)?,
                    &quality,
                    input.execution_policy,
                    cull_mask.as_ref(),
                    input
                        .history
                        .as_ref()
                        .and_then(|history| history.clipmap.as_ref()),
                ));
                if let Some(clipmap) = view_distance_clipmap.as_ref()
                    && (clipmap.usage_count > 0 || !clipmap.fallback_reasons.is_empty())
                {
                    pass_stats.push(clipmap_pass_runtime("view_distance_clipmap", clipmap));
                }
                let primary_dispatch = prepare_primary_visibility_dispatch(
                    ctx,
                    contract.query_contract,
                    input.region_capture_value(),
                    input.frame_domain.clone(),
                    candidate_shape_names,
                    candidate_spans,
                    camera,
                    primary_viewport,
                    viewport,
                    jitter_pixels,
                    ray_budget,
                    effective_plan
                        .view
                        .compatibility_projection
                        .legacy_path_active,
                    input.compatibility_projection,
                )?;
                gpu_runtime.merge_from(&primary_dispatch.initial_gpu_runtime());
                primary_solver_context = batch_plan
                    .ray_solver
                    .as_ref()
                    .map(|solver| (solver.clone(), batch_plan.artifact_contracts.clone()));
                let range_start = framegraph
                    .profiler_record_count()
                    .map_err(presentation_framegraph_error)?;
                let arena = framegraph.attachments.clone();
                let (encoder, profiler) = framegraph
                    .encoder_and_profiler_mut()
                    .map_err(presentation_framegraph_error)?;
                let primary_encode = primary_dispatch.encode_passes(
                    encoder,
                    profiler,
                    &arena,
                    contract,
                    &mut gpu_runtime,
                )?;
                let range_end = framegraph
                    .profiler_record_count()
                    .map_err(presentation_framegraph_error)?;
                let primary_result = primary_dispatch.dispatch_result();
                primary_trace_dispatch = Some(primary_dispatch.clone());
                primary_trace_seed = Some((
                    contract.query_contract,
                    batch_plan.clone(),
                    primary_result.item_count,
                    contract.primary_hit_attachment.clone(),
                    SmolStr::new("wrela.presentation.primary.observability"),
                ));
                primary_observability_buffer = primary_result
                    .metrics
                    .as_ref()
                    .map(|handle| (handle.buffer.clone(), handle.size_bytes));
                let materialize_dispatch_count = 1
                    + u32::from(contract.depth_attachment.is_some())
                    + u32::from(contract.world_normal_attachment.is_some());
                let mut notes = Vec::new();
                if let Some(mask) = cull_mask.as_ref() {
                    notes.push(format!(
                        "tile_cull active_tiles={}/{} skipped_samples={}",
                        mask.stats.active_tiles, mask.stats.total_tiles, mask.stats.skipped_samples
                    ));
                    if mask.candidate_table.enabled {
                        notes.push(format!(
                            "tile_candidate_table enabled=true active_samples={}/{} packet_count={} packet_size={}",
                            tile_candidate.active_samples,
                            tile_candidate.total_samples,
                            tile_candidate.packet_count,
                            tile_candidate.packet_size
                        ));
                    }
                }
                notes.push(format!(
                    "workgroup_size={} resident_primary_viewport={}x{}",
                    primary_dispatch.selected_workgroup_size(),
                    primary_viewport.width,
                    primary_viewport.height
                ));
                let resident_primary_samples = primary_viewport
                    .width
                    .saturating_mul(primary_viewport.height);
                if primary_result.item_count < resident_primary_samples {
                    notes.push(format!(
                        "active_dispatch_samples={}",
                        primary_result.item_count
                    ));
                }
                if candidate_table_active {
                    notes.push(
                        "packet_scheduling active=false reason=resident_primary_path".to_string(),
                    );
                }
                pass_stats.push(PassRuntimeStats {
                    pass_id: pass.id.to_string(),
                    pass_kind: "primary_visibility".to_string(),
                    work_items: primary_viewport
                        .width
                        .saturating_mul(primary_viewport.height),
                    dispatch_count: 2,
                    elapsed_micros: primary_encode.visibility_elapsed_micros,
                    notes: notes.clone(),
                    ..PassRuntimeStats::default()
                });
                pass_stats.push(PassRuntimeStats {
                    pass_id: format!("{}.writeout", pass.id),
                    pass_kind: "primary_writeout".to_string(),
                    work_items: viewport.width.saturating_mul(viewport.height),
                    attachment_bytes_written: full_attachment_byte_size(
                        &attachments,
                        contract.primary_hit_attachment.as_str(),
                    ) + contract
                        .depth_attachment
                        .as_ref()
                        .map(|name| full_attachment_byte_size(&attachments, name))
                        .unwrap_or_default()
                        + contract
                            .world_normal_attachment
                            .as_ref()
                            .map(|name| full_attachment_byte_size(&attachments, name))
                            .unwrap_or_default(),
                    dispatch_count: materialize_dispatch_count,
                    elapsed_micros: primary_encode.writeout_elapsed_micros,
                    notes,
                    ..PassRuntimeStats::default()
                });
                pending_gpu_pass_ranges.push((
                    pass_stats.len() - 2,
                    range_start..range_start.saturating_add(2),
                ));
                pending_gpu_pass_ranges.push((
                    pass_stats.len() - 1,
                    range_start.saturating_add(2)..range_end,
                ));
            }
            PresentationPassKind::SurfaceResolve { contract } => {
                primary_trace_seed.as_ref().ok_or_else(|| {
                    PresentationExecError::MissingPrimaryVisibilityPass {
                        plan: effective_plan.name.clone(),
                    }
                })?;
                let pass_start = Instant::now();
                let range_start = framegraph
                    .profiler_record_count()
                    .map_err(presentation_framegraph_error)?;
                let arena = framegraph.attachments.clone();
                let (encoder, profiler) = framegraph
                    .encoder_and_profiler_mut()
                    .map_err(presentation_framegraph_error)?;
                let (count, dispatch_count, notes) = gpu_post::encode_surface_resolve_gpu(
                    &native,
                    encoder,
                    profiler,
                    &arena,
                    ctx,
                    input,
                    contract,
                    quality.hit_compaction_enabled,
                    selected_workgroup_size,
                    &mut gpu_runtime,
                )?;
                let range_end = framegraph
                    .profiler_record_count()
                    .map_err(presentation_framegraph_error)?;
                surface_resolve_count = count;
                pass_stats.push(PassRuntimeStats {
                    pass_id: pass.id.to_string(),
                    pass_kind: "surface_resolve".to_string(),
                    work_items: count,
                    attachment_bytes_read: full_attachment_byte_size(
                        &attachments,
                        contract.primary_hit_attachment.as_str(),
                    ),
                    attachment_bytes_written: full_attachment_byte_size(
                        &attachments,
                        contract.surface_attachment.as_str(),
                    ),
                    dispatch_count,
                    notes,
                    elapsed_micros: pass_start.elapsed().as_micros(),
                    ..PassRuntimeStats::default()
                });
                pending_gpu_pass_ranges.push((pass_stats.len() - 1, range_start..range_end));
            }
            PresentationPassKind::ParticipantsResolve { contract } => {
                primary_trace_seed.as_ref().ok_or_else(|| {
                    PresentationExecError::MissingPrimaryVisibilityPass {
                        plan: effective_plan.name.clone(),
                    }
                })?;
                let pass_start = Instant::now();
                let range_start = framegraph
                    .profiler_record_count()
                    .map_err(presentation_framegraph_error)?;
                let arena = framegraph.attachments.clone();
                let (encoder, profiler) = framegraph
                    .encoder_and_profiler_mut()
                    .map_err(presentation_framegraph_error)?;
                let (radiance_count, medium_count, dispatch_count, notes) =
                    gpu_post::encode_participants_resolve_gpu(
                        &native,
                        encoder,
                        profiler,
                        &arena,
                        ctx,
                        input,
                        camera,
                        viewport,
                        jitter_pixels,
                        effective_plan
                            .view
                            .compatibility_projection
                            .legacy_path_active,
                        contract,
                        quality.radiance_mode,
                        selected_workgroup_size,
                        &mut gpu_runtime,
                    )?;
                let range_end = framegraph
                    .profiler_record_count()
                    .map_err(presentation_framegraph_error)?;
                participant_resolve_count = radiance_count + medium_count;
                pass_stats.push(PassRuntimeStats {
                    pass_id: pass.id.to_string(),
                    pass_kind: "participants_resolve".to_string(),
                    work_items: participant_resolve_count,
                    attachment_bytes_read: full_attachment_byte_size(
                        &attachments,
                        contract.primary_hit_attachment.as_str(),
                    ),
                    attachment_bytes_written: contract
                        .radiance_attachment
                        .as_ref()
                        .map(|name| full_attachment_byte_size(&attachments, name))
                        .unwrap_or_default()
                        + contract
                            .medium_attachment
                            .as_ref()
                            .map(|name| full_attachment_byte_size(&attachments, name))
                            .unwrap_or_default(),
                    dispatch_count,
                    notes,
                    elapsed_micros: pass_start.elapsed().as_micros(),
                    ..PassRuntimeStats::default()
                });
                pending_gpu_pass_ranges.push((pass_stats.len() - 1, range_start..range_end));
            }
            PresentationPassKind::ShadePrimary { contract } => {
                let pass_start = Instant::now();
                let range_start = framegraph
                    .profiler_record_count()
                    .map_err(presentation_framegraph_error)?;
                let arena = framegraph.attachments.clone();
                let (encoder, profiler) = framegraph
                    .encoder_and_profiler_mut()
                    .map_err(presentation_framegraph_error)?;
                let dispatch_count = encode_shade_primary_gpu(
                    &native,
                    encoder,
                    profiler,
                    &arena,
                    camera,
                    viewport,
                    jitter_pixels,
                    effective_plan
                        .view
                        .compatibility_projection
                        .legacy_path_active,
                    &input.lighting,
                    contract,
                    selected_workgroup_size,
                    &mut gpu_runtime,
                )?;
                let range_end = framegraph
                    .profiler_record_count()
                    .map_err(presentation_framegraph_error)?;
                pass_stats.push(PassRuntimeStats {
                    pass_id: pass.id.to_string(),
                    pass_kind: "shade_primary".to_string(),
                    work_items: viewport.width.saturating_mul(viewport.height),
                    attachment_bytes_read: full_attachment_byte_size(
                        &attachments,
                        contract.primary_hit_attachment.as_str(),
                    ) + full_attachment_byte_size(
                        &attachments,
                        contract.surface_attachment.as_str(),
                    ) + contract
                        .radiance_attachment
                        .as_ref()
                        .map(|name| full_attachment_byte_size(&attachments, name))
                        .unwrap_or_default()
                        + contract
                            .medium_attachment
                            .as_ref()
                            .map(|name| full_attachment_byte_size(&attachments, name))
                            .unwrap_or_default(),
                    attachment_bytes_written: full_attachment_byte_size(
                        &attachments,
                        contract.output_attachment.as_str(),
                    ),
                    dispatch_count,
                    elapsed_micros: pass_start.elapsed().as_micros(),
                    ..PassRuntimeStats::default()
                });
                pending_gpu_pass_ranges.push((pass_stats.len() - 1, range_start..range_end));
            }
            PresentationPassKind::MotionResolve { contract } => {
                let pass_start = Instant::now();
                let range_start = framegraph
                    .profiler_record_count()
                    .map_err(presentation_framegraph_error)?;
                let summary = motion_resolve_assessment_summary(&effective_plan, input)?;
                let arena = framegraph.attachments.clone();
                let (encoder, profiler) = framegraph
                    .encoder_and_profiler_mut()
                    .map_err(presentation_framegraph_error)?;
                let motion_dispatch = encode_motion_resolve_gpu(
                    &native,
                    encoder,
                    profiler,
                    &arena,
                    input,
                    viewport,
                    contract,
                    selected_workgroup_size,
                    &mut gpu_runtime,
                    &summary,
                )?;
                let range_end = framegraph
                    .profiler_record_count()
                    .map_err(presentation_framegraph_error)?;
                motion_counts_readback = Some((
                    motion_dispatch.counts_buffer.clone(),
                    motion_dispatch.counts_readback_label.clone(),
                ));
                continuation_counts
                    .diagnostics
                    .push(summary.diagnostic.clone());
                pass_stats.push(PassRuntimeStats {
                    pass_id: pass.id.to_string(),
                    pass_kind: "motion_resolve".to_string(),
                    work_items: viewport.width.saturating_mul(viewport.height),
                    attachment_bytes_read: full_attachment_byte_size(
                        &attachments,
                        contract.primary_hit_attachment.as_str(),
                    ),
                    attachment_bytes_written: full_attachment_byte_size(
                        &attachments,
                        contract.output_attachment.as_str(),
                    ) + contract
                        .history_primary_hit_attachment
                        .as_ref()
                        .map(|name| full_attachment_byte_size(&attachments, name))
                        .unwrap_or_default(),
                    dispatch_count: motion_dispatch.dispatch_count,
                    notes: vec![summary.diagnostic],
                    elapsed_micros: pass_start.elapsed().as_micros(),
                    ..PassRuntimeStats::default()
                });
                pending_gpu_pass_ranges.push((pass_stats.len() - 1, range_start..range_end));
            }
            PresentationPassKind::TemporalResolve { contract } => {
                let pass_start = Instant::now();
                let range_start = framegraph
                    .profiler_record_count()
                    .map_err(presentation_framegraph_error)?;
                let arena = framegraph.attachments.clone();
                let (encoder, profiler) = framegraph
                    .encoder_and_profiler_mut()
                    .map_err(presentation_framegraph_error)?;
                let temporal = encode_temporal_resolve_gpu(
                    &native,
                    encoder,
                    profiler,
                    &arena,
                    viewport.width,
                    viewport.height,
                    contract,
                    selected_workgroup_size,
                    &mut gpu_runtime,
                )?;
                let range_end = framegraph
                    .profiler_record_count()
                    .map_err(presentation_framegraph_error)?;
                temporal_counts_readback = Some((
                    temporal.counts_buffer.clone(),
                    temporal.counts_readback_label.clone(),
                ));
                pass_stats.push(PassRuntimeStats {
                    pass_id: pass.id.to_string(),
                    pass_kind: "temporal_resolve".to_string(),
                    work_items: viewport.width.saturating_mul(viewport.height),
                    attachment_bytes_read: full_attachment_byte_size(
                        &attachments,
                        contract.input_attachment.as_str(),
                    ) + full_attachment_byte_size(
                        &attachments,
                        contract.history_color_attachment.as_str(),
                    ) + full_attachment_byte_size(
                        &attachments,
                        contract.motion_attachment.as_str(),
                    ) + contract
                        .history_primary_hit_attachment
                        .as_ref()
                        .map(|name| full_attachment_byte_size(&attachments, name))
                        .unwrap_or_default(),
                    attachment_bytes_written: full_attachment_byte_size(
                        &attachments,
                        contract.output_attachment.as_str(),
                    ) + full_attachment_byte_size(
                        &attachments,
                        contract.history_color_attachment.as_str(),
                    ) + contract
                        .history_primary_hit_attachment
                        .as_ref()
                        .map(|name| full_attachment_byte_size(&attachments, name))
                        .unwrap_or_default(),
                    dispatch_count: temporal.dispatch_count,
                    elapsed_micros: pass_start.elapsed().as_micros(),
                    ..PassRuntimeStats::default()
                });
                pending_gpu_pass_ranges.push((pass_stats.len() - 1, range_start..range_end));
            }
            PresentationPassKind::CompositeColor { contract } => {
                let pass_start = Instant::now();
                let range_start = framegraph
                    .profiler_record_count()
                    .map_err(presentation_framegraph_error)?;
                let arena = framegraph.attachments.clone();
                let (encoder, profiler) = framegraph
                    .encoder_and_profiler_mut()
                    .map_err(presentation_framegraph_error)?;
                let dispatch_count = encode_composite_color_gpu(
                    &native,
                    encoder,
                    profiler,
                    &arena,
                    contract,
                    selected_workgroup_size,
                    &mut gpu_runtime,
                )?;
                let range_end = framegraph
                    .profiler_record_count()
                    .map_err(presentation_framegraph_error)?;
                pass_stats.push(PassRuntimeStats {
                    pass_id: pass.id.to_string(),
                    pass_kind: "composite_color".to_string(),
                    work_items: viewport.width.saturating_mul(viewport.height),
                    attachment_bytes_read: full_attachment_byte_size(
                        &attachments,
                        contract.input_attachment.as_str(),
                    ),
                    attachment_bytes_written: full_attachment_byte_size(
                        &attachments,
                        contract.output_attachment.as_str(),
                    ),
                    dispatch_count,
                    elapsed_micros: pass_start.elapsed().as_micros(),
                    ..PassRuntimeStats::default()
                });
                pending_gpu_pass_ranges.push((pass_stats.len() - 1, range_start..range_end));
            }
            PresentationPassKind::ExportAttachment { attachment } => {
                explicitly_exported_attachments.insert(attachment.clone());
                pass_stats.push(PassRuntimeStats {
                    pass_id: pass.id.to_string(),
                    pass_kind: "export_attachment".to_string(),
                    work_items: framegraph
                        .attachments
                        .attachment(attachment.as_str())
                        .map(|slot| slot.layout.width.saturating_mul(slot.layout.height))
                        .unwrap_or_default(),
                    attachment_bytes_read: full_attachment_byte_size(&attachments, attachment),
                    dispatch_count: 0,
                    notes: vec!["readback=explicit_export".to_string()],
                    ..PassRuntimeStats::default()
                });
            }
            other => {
                return Err(PresentationExecError::UnsupportedPlan {
                    message: format!("wgsl executor does not support pass kind {other:?}"),
                });
            }
        }
    }

    for attachment_name in
        timed_attachment_readback_names(&effective_plan, &explicitly_exported_attachments)
    {
        framegraph
            .schedule_attachment_readback(attachment_name.as_str())
            .map_err(presentation_framegraph_error)?;
    }
    let final_submission = framegraph
        .submit_segment()
        .map_err(presentation_framegraph_error)?;
    framegraph_exceptions.extend(
        final_submission
            .documented_exceptions
            .iter()
            .map(|exception| exception.to_string()),
    );
    note_framegraph_submission_metrics(&final_submission, &mut gpu_runtime);
    for (pass_index, range) in pending_gpu_pass_ranges {
        let gpu_elapsed_micros = sum_gpu_elapsed_micros(&final_submission, range);
        if gpu_elapsed_micros > 0 {
            pass_stats[pass_index].gpu_elapsed_micros = Some(gpu_elapsed_micros);
        }
    }
    let timed_gpu_runtime = gpu_runtime.clone();
    apply_attachment_readbacks(&mut attachments, &final_submission)?;
    gpu_runtime.dispatch_fragmentation_count = pass_stats
        .iter()
        .map(|pass| pass.dispatch_count.saturating_sub(1))
        .sum();
    let untimed_attachment_readbacks = if input.materialize_cpu_attachments {
        untimed_cpu_materialization_attachment_names(&attachments, &explicitly_exported_attachments)
    } else {
        temporal_history_attachment_names(&effective_plan)
    };
    let (
        primary_contract_id,
        primary_batch_plan,
        primary_item_count,
        primary_hit_attachment,
        primary_observability_label,
    ) = primary_trace_seed.ok_or_else(|| PresentationExecError::MissingPrimaryVisibilityPass {
        plan: effective_plan.name.clone(),
    })?;
    let primary_trace_dispatch = primary_trace_dispatch.ok_or_else(|| {
        PresentationExecError::MissingPrimaryVisibilityPass {
            plan: effective_plan.name.clone(),
        }
    })?;
    framegraph
        .schedule_attachment_readback(primary_hit_attachment.as_str())
        .map_err(presentation_framegraph_error)?;
    if let Some((buffer, size_bytes)) = primary_observability_buffer.as_ref() {
        framegraph
            .schedule_readback(
                buffer,
                ReadbackRequest::new(
                    ReadbackReason::QueryResult,
                    primary_observability_label.to_string(),
                    *size_bytes,
                ),
            )
            .map_err(presentation_framegraph_error)?;
    }
    if let Some((buffer, label)) = motion_counts_readback.as_ref() {
        framegraph
            .schedule_readback(
                buffer,
                ReadbackRequest::new(
                    ReadbackReason::Custom(SmolStr::new("motion_resolve_counts")),
                    label.clone(),
                    12,
                ),
            )
            .map_err(presentation_framegraph_error)?;
    }
    if let Some((buffer, label)) = temporal_counts_readback.as_ref() {
        framegraph
            .schedule_readback(
                buffer,
                ReadbackRequest::new(
                    ReadbackReason::Custom(SmolStr::new("temporal_resolve_counts")),
                    label.clone(),
                    4,
                ),
            )
            .map_err(presentation_framegraph_error)?;
    }
    for attachment_name in &untimed_attachment_readbacks {
        framegraph
            .schedule_attachment_readback(attachment_name.as_str())
            .map_err(presentation_framegraph_error)?;
    }
    let untimed_submission = framegraph
        .submit_segment()
        .map_err(presentation_framegraph_error)?;
    apply_attachment_readbacks(&mut attachments, &untimed_submission)?;
    if let Some((_, label)) = motion_counts_readback.as_ref() {
        let counts_bytes = submission_readback_bytes(&untimed_submission, label)?;
        let (available, rejected, unavailable) = decode_motion_counts(counts_bytes);
        continuation_counts.available = continuation_counts.available.saturating_add(available);
        continuation_counts.rejected = continuation_counts.rejected.saturating_add(rejected);
        continuation_counts.unavailable =
            continuation_counts.unavailable.saturating_add(unavailable);
    }
    if let Some((_, label)) = temporal_counts_readback.as_ref() {
        let counts_bytes = submission_readback_bytes(&untimed_submission, label)?;
        continuation_counts.consumed = continuation_counts
            .consumed
            .saturating_add(decode_u32_count(counts_bytes));
    }
    let query_observability = if primary_observability_buffer.is_some() {
        let observability_bytes =
            submission_readback_bytes(&untimed_submission, primary_observability_label.as_str())?;
        primary_trace_dispatch.decode_observability(observability_bytes, timed_gpu_runtime.clone())
    } else {
        QueryExecutionObservability::default()
    };
    let primary_hit_bytes = submission_readback_bytes(
        &untimed_submission,
        &format!("wrela.presentation.readback.{}", primary_hit_attachment),
    )?;
    let primary_hits = decode_primary_hit_attachment_bytes_from_name(
        &framegraph.attachments,
        primary_hit_attachment.as_str(),
        primary_hit_bytes,
    )?;
    let mut primary_trace = build_primary_batch_query_trace(
        primary_contract_id,
        current_snapshot,
        &primary_batch_plan,
        primary_item_count,
        query_observability,
    )?;
    let continuation_diagnostics = continuation_counts.diagnostics.clone();
    let primary_solver_summary =
        runtime_primary_solver_summary(primary_solver_context.as_ref(), &continuation_counts);
    update_query_trace_continuation(&mut primary_trace, continuation_counts);
    let metrics = presentation_metrics(
        &primary_hits,
        &primary_trace,
        primary_solver_summary,
        continuation_diagnostics,
        timed_gpu_runtime.clone(),
    );
    let frame_cost = build_frame_cost_report(
        &input.frame_domain,
        input.execution_policy,
        DispatchBackend::Wgsl,
        viewport.width,
        viewport.height,
        &effective_plan.frame.quality,
        &quality,
        &metrics,
        tile_cull,
        tile_candidate,
        packet_scheduling_active,
        selected_workgroup_size,
        surface_resolve_count,
        participant_resolve_count,
        crate::presentation_exec::attachment_byte_reports(
            &attachments,
            Some(&framegraph.attachments),
        ),
        pass_stats,
        framegraph_exceptions,
        if candidate_table_active {
            let mut artifacts = vec!["tile_candidate_table".to_string()];
            artifacts.push("view_distance_clipmap".to_string());
            artifacts
        } else if view_distance_clipmap.is_some() {
            vec!["view_distance_clipmap".to_string()]
        } else {
            Vec::new()
        },
    );
    let history = build_temporal_history(
        &effective_plan,
        &input.frame_state,
        &attachments,
        current_snapshot,
        view_distance_clipmap.as_ref(),
    )?;
    Ok(PresentationExecutionResult {
        plan_name: plan.name.clone(),
        backend: DispatchBackend::Wgsl,
        width: viewport.width,
        height: viewport.height,
        screen_samples: Vec::new(),
        attachments,
        history,
        metrics,
        frame_cost,
        query_trace: primary_trace,
    })
}

fn presentation_framegraph_error(error: PresentationFramegraphError) -> PresentationExecError {
    PresentationExecError::UnsupportedPlan {
        message: error.to_string(),
    }
}

fn note_framegraph_submission_metrics(
    submission: &PresentationFramegraphSubmission,
    gpu_runtime: &mut GpuRuntimeMetrics,
) {
    gpu_runtime.queue_submit_count = gpu_runtime.queue_submit_count.saturating_add(1);
    gpu_runtime.transient_buffer_creations = gpu_runtime
        .transient_buffer_creations
        .saturating_add(submission.readbacks.len() as u32)
        .saturating_add(u32::from(
            submission.timestamps_supported && !submission.gpu_elapsed_micros.is_empty(),
        ));
    gpu_runtime.readback_bytes = gpu_runtime.readback_bytes.saturating_add(
        submission
            .readbacks
            .iter()
            .map(|result| result.request.size_bytes)
            .sum::<u64>(),
    );
    if submission.timestamps_supported {
        gpu_runtime.readback_bytes = gpu_runtime
            .readback_bytes
            .saturating_add((submission.gpu_elapsed_micros.len() as u64) * 16);
        gpu_runtime.note_gpu_timings(true, &submission.gpu_elapsed_micros);
    }
}

fn submission_readback_bytes<'a>(
    submission: &'a PresentationFramegraphSubmission,
    label: &str,
) -> Result<&'a [u8], PresentationExecError> {
    submission
        .readbacks
        .iter()
        .find(|result| result.request.label.as_str() == label)
        .map(|result| result.bytes.as_slice())
        .ok_or_else(|| PresentationExecError::UnsupportedPlan {
            message: format!("missing presentation readback '{label}'"),
        })
}

fn decode_primary_hit_attachment_bytes_from_name(
    arena: &crate::presentation_exec::gpu_resources::GpuAttachmentArena,
    attachment_name: &str,
    bytes: &[u8],
) -> Result<Vec<KernelValue>, PresentationExecError> {
    let layout = arena
        .attachment(attachment_name)
        .ok_or_else(|| PresentationExecError::UnsupportedPlan {
            message: format!("missing GPU primary-hit attachment '{attachment_name}'"),
        })?
        .layout
        .clone();
    portable_abi_decode_slice(
        &layout.element_abi,
        bytes,
        (layout.width.saturating_mul(layout.height)) as usize,
    )
    .map_err(|err| PresentationExecError::UnsupportedPlan {
        message: format!("failed to decode GPU primary-hit attachment bytes: {err}"),
    })
}

fn sum_gpu_elapsed_micros(
    submission: &PresentationFramegraphSubmission,
    range: Range<usize>,
) -> u128 {
    let end = range.end.min(submission.gpu_elapsed_micros.len());
    if range.start >= end {
        return 0;
    }
    submission.gpu_elapsed_micros[range.start..end]
        .iter()
        .copied()
        .sum()
}

fn decode_u32_count(bytes: &[u8]) -> u32 {
    bytes
        .get(..4)
        .and_then(|value| value.try_into().ok())
        .map(u32::from_le_bytes)
        .unwrap_or_default()
}

fn decode_motion_counts(bytes: &[u8]) -> (u32, u32, u32) {
    let available = decode_u32_count(bytes);
    let rejected = bytes
        .get(4..8)
        .and_then(|value| value.try_into().ok())
        .map(u32::from_le_bytes)
        .unwrap_or_default();
    let unavailable = bytes
        .get(8..12)
        .and_then(|value| value.try_into().ok())
        .map(u32::from_le_bytes)
        .unwrap_or_default();
    (available, rejected, unavailable)
}

fn apply_attachment_readbacks(
    attachments: &mut AttachmentResourceSet,
    submission: &PresentationFramegraphSubmission,
) -> Result<(), PresentationExecError> {
    for result in &submission.readbacks {
        if let Some(attachment_name) = result
            .request
            .label
            .as_str()
            .strip_prefix("wrela.presentation.readback.")
            && let Some(attachment) = attachments.attachment_mut(attachment_name)
        {
            attachment.bytes = result.bytes.clone();
        }
    }
    Ok(())
}

fn staging_attachment_resources(
    attachments: &AttachmentResourceSet,
    names: &[&str],
) -> AttachmentResourceSet {
    let staged_attachments = names
        .iter()
        .filter_map(|name| {
            attachments
                .attachment(name)
                .map(|attachment| ((*name).into(), attachment.clone()))
        })
        .collect::<std::collections::BTreeMap<SmolStr, AttachmentResource>>();
    AttachmentResourceSet {
        width: attachments.width,
        height: attachments.height,
        attachments: staged_attachments,
    }
}

fn timed_attachment_readback_names(
    _plan: &PresentationPlan,
    explicitly_exported_attachments: &BTreeSet<SmolStr>,
) -> Vec<SmolStr> {
    explicitly_exported_attachments.iter().cloned().collect()
}

fn temporal_history_attachment_names(plan: &PresentationPlan) -> Vec<SmolStr> {
    let mut names = BTreeSet::new();
    if let Some(temporal) = &plan.frame.temporal {
        for slot in &temporal.history_slots {
            names.insert(slot.attachment.clone());
        }
    }
    names.into_iter().collect()
}

fn untimed_cpu_materialization_attachment_names(
    attachments: &AttachmentResourceSet,
    explicitly_exported_attachments: &BTreeSet<SmolStr>,
) -> Vec<SmolStr> {
    attachments
        .attachments
        .iter()
        .filter_map(|(name, attachment)| {
            if explicitly_exported_attachments.contains(name) {
                return None;
            }
            if attachment.layout.attachment.lifetime == AttachmentLifetime::Exported {
                return None;
            }
            Some(name.clone())
        })
        .collect()
}

fn upload_attachment_to_gpu(
    native: &crate::query_exec::wgsl::NativeWgpuContext,
    attachments: &AttachmentResourceSet,
    arena: &crate::presentation_exec::gpu_resources::GpuAttachmentArena,
    attachment_name: &str,
    gpu_runtime: &mut GpuRuntimeMetrics,
) -> Result<(), PresentationExecError> {
    let Some(cpu_attachment) = attachments.attachment(attachment_name) else {
        return Ok(());
    };
    let Some(buffer) = arena.attachment_buffer(attachment_name) else {
        return Ok(());
    };
    if !cpu_attachment.bytes.is_empty() {
        native.queue.write_buffer(buffer, 0, &cpu_attachment.bytes);
        gpu_runtime.upload_bytes = gpu_runtime
            .upload_bytes
            .saturating_add(cpu_attachment.bytes.len() as u64);
    }
    Ok(())
}

fn build_primary_batch_query_trace(
    contract_id: crate::query_contract::QueryContractId,
    snapshot: &crate::world_identity::WorldSnapshotHandle,
    plan: &crate::kernel::ir::KernelBatchQueryPlan,
    item_count: u32,
    observability: QueryExecutionObservability,
) -> Result<BatchQueryExecutionTrace, PresentationExecError> {
    let descriptor = crate::query_contract::query_contract(contract_id).ok_or_else(|| {
        PresentationExecError::UnsupportedPlan {
            message: format!("missing query contract '{}'", contract_id.as_str()),
        }
    })?;
    let plan_trace = interpret_batch_query(plan, item_count);
    let cost_report = crate::query_exec::cost::batch_cost_report(
        DispatchBackend::Wgsl,
        plan,
        &plan_trace,
        &observability,
    );
    Ok(BatchQueryExecutionTrace {
        contract_id: descriptor.id,
        family: descriptor.family,
        question: descriptor.question,
        surface: descriptor.surface,
        contract_version: descriptor.version,
        backend: DispatchBackend::Wgsl,
        snapshot: Some(snapshot.report()),
        plan_trace,
        observability,
        cost_report,
    })
}

fn execute_surface_resolve(
    ctx: &QueryExecContext,
    current_snapshot: &crate::world_identity::WorldSnapshotHandle,
    input: &PresentationExecutionInput,
    attachments: &mut AttachmentResourceSet,
    hits: &[KernelValue],
    contract: &SurfaceResolvePassContract,
    backend: DispatchBackend,
    compact_hits: bool,
) -> Result<(u32, u32, Vec<String>, GpuRuntimeMetrics), PresentationExecError> {
    let Some(_) = attachments.attachment(contract.surface_attachment.as_str()) else {
        return Ok((0, 0, Vec::new(), GpuRuntimeMetrics::default()));
    };
    let default_surface = default_surface();
    let mut notes = Vec::new();
    let scaled_attachment = attachments
        .attachment(contract.surface_attachment.as_str())
        .is_some_and(|attachment| {
            attachment.layout.width != attachments.width
                || attachment.layout.height != attachments.height
        });
    let work_items = attachment_hit_work_items(
        attachments,
        contract.surface_attachment.as_str(),
        hits,
        compact_hits,
    )?;
    if compact_hits && work_items.len() < hits.len() {
        notes.push(format!(
            "hit_compaction {} of {} samples",
            work_items.len(),
            hits.len()
        ));
    }
    if scaled_attachment {
        notes.push(format!("scaled_attachment={}", contract.surface_attachment));
    }
    if compact_hits || scaled_attachment {
        if work_items.is_empty() {
            return Ok((0, 0, notes, GpuRuntimeMetrics::default()));
        }
        let hit_indices = work_items
            .iter()
            .map(|(index, _)| *index)
            .collect::<Vec<_>>();
        let hit_values = work_items
            .iter()
            .map(|(_, hit)| hit.clone())
            .collect::<Vec<_>>();
        let (surfaces, trace) = execute_batch_contract(
            ctx,
            backend,
            current_snapshot,
            input.query_trace_solver_mode,
            contract.query_contract,
            &[
                input.region_capture_value(),
                input.frame_domain.clone(),
                KernelValue::Array(hit_values),
            ],
        )?;
        let dispatch_count = trace
            .observability
            .dispatch_count
            .max(u32::from(!hit_indices.is_empty()));
        let mut gpu_runtime = trace.observability.gpu_runtime.clone();
        if compact_hits {
            encode_values_at_indices(
                attachments,
                contract.surface_attachment.as_str(),
                &hit_indices,
                &surfaces,
            )?;
            gpu_runtime.attachment_encode_count = gpu_runtime
                .attachment_encode_count
                .saturating_add(hit_indices.len() as u32);
        } else {
            let Some(surface_attachment) =
                attachments.attachment_mut(contract.surface_attachment.as_str())
            else {
                return Ok((0, 0, notes, gpu_runtime));
            };
            for ((index, hit), surface) in work_items.iter().zip(surfaces.iter()) {
                if hit_flag(hit)? {
                    surface_attachment.encode(*index, surface)?;
                } else {
                    surface_attachment.encode(*index, &default_surface)?;
                }
            }
            gpu_runtime.attachment_encode_count = gpu_runtime
                .attachment_encode_count
                .saturating_add(work_items.len() as u32);
        }
        return Ok((hit_indices.len() as u32, dispatch_count, notes, gpu_runtime));
    }

    let (surfaces, trace) = execute_batch_contract(
        ctx,
        backend,
        current_snapshot,
        input.query_trace_solver_mode,
        contract.query_contract,
        &[
            input.region_capture_value(),
            input.frame_domain.clone(),
            KernelValue::Array(hits.to_vec()),
        ],
    )?;
    let Some(surface_attachment) = attachments.attachment_mut(contract.surface_attachment.as_str())
    else {
        return Ok((0, 0, notes, trace.observability.gpu_runtime.clone()));
    };
    for (index, (hit, surface)) in hits.iter().zip(surfaces.iter()).enumerate() {
        if hit_flag(hit)? {
            surface_attachment.encode(index, surface)?;
        } else {
            surface_attachment.encode(index, &default_surface)?;
        }
    }
    let mut gpu_runtime = trace.observability.gpu_runtime.clone();
    gpu_runtime.attachment_encode_count = gpu_runtime
        .attachment_encode_count
        .saturating_add(hits.len() as u32);
    Ok((
        hits.len() as u32,
        trace
            .observability
            .dispatch_count
            .max(u32::from(!hits.is_empty())),
        notes,
        gpu_runtime,
    ))
}

fn execute_participants_resolve(
    ctx: &QueryExecContext,
    current_snapshot: &crate::world_identity::WorldSnapshotHandle,
    input: &PresentationExecutionInput,
    screen_samples: &[KernelValue],
    attachments: &mut AttachmentResourceSet,
    hits: &[KernelValue],
    contract: &ParticipantsResolvePassContract,
    backend: DispatchBackend,
    radiance_mode: RealtimeRadianceMode,
) -> Result<(u32, u32, u32, Vec<String>, GpuRuntimeMetrics), PresentationExecError> {
    let mut radiance_count = 0;
    let mut medium_count = 0;
    let mut dispatch_count = 0u32;
    let mut notes = Vec::new();
    let mut gpu_runtime = GpuRuntimeMetrics::default();
    if let (Some(query_contract), Some(attachment_name)) = (
        contract.radiance_query_contract,
        contract.radiance_attachment.as_deref(),
    ) {
        let include_misses = radiance_mode == RealtimeRadianceMode::Full;
        let radiance_items = participant_query_work_items(
            input,
            screen_samples,
            hits,
            attachments,
            attachment_name,
            contract.miss_sample_distance,
            include_misses,
        )?;
        radiance_count = radiance_items.len() as u32;
        if radiance_mode == RealtimeRadianceMode::Reduced {
            notes.push(format!("radiance_mode=reduced items={radiance_count}"));
        }
        if attachments
            .attachment(attachment_name)
            .is_some_and(|attachment| {
                attachment.layout.width != attachments.width
                    || attachment.layout.height != attachments.height
            })
        {
            notes.push(format!("scaled_attachment={attachment_name}"));
        }
        if !radiance_items.is_empty() {
            let target_indices = radiance_items
                .iter()
                .map(|item| item.target_index)
                .collect::<Vec<_>>();
            let query_items = radiance_items
                .iter()
                .map(|item| item.point_direction_query.clone())
                .collect::<Vec<_>>();
            let (radiance, trace) = execute_batch_contract(
                ctx,
                backend,
                current_snapshot,
                input.query_trace_solver_mode,
                query_contract,
                &[
                    input.region_capture_value(),
                    input.frame_domain.clone(),
                    KernelValue::Array(query_items),
                ],
            )?;
            dispatch_count = dispatch_count.saturating_add(
                trace
                    .observability
                    .dispatch_count
                    .max(u32::from(!target_indices.is_empty())),
            );
            encode_values_at_indices(attachments, attachment_name, &target_indices, &radiance)?;
            gpu_runtime.merge_from(&trace.observability.gpu_runtime);
            gpu_runtime.attachment_encode_count = gpu_runtime
                .attachment_encode_count
                .saturating_add(target_indices.len() as u32);
        }
    }
    if let (Some(query_contract), Some(attachment_name)) = (
        contract.medium_query_contract,
        contract.medium_attachment.as_deref(),
    ) {
        let medium_items = participant_query_work_items(
            input,
            screen_samples,
            hits,
            attachments,
            attachment_name,
            contract.miss_sample_distance,
            true,
        )?;
        medium_count = medium_items.len() as u32;
        if attachments
            .attachment(attachment_name)
            .is_some_and(|attachment| {
                attachment.layout.width != attachments.width
                    || attachment.layout.height != attachments.height
            })
        {
            notes.push(format!("scaled_attachment={attachment_name}"));
        }
        if !medium_items.is_empty() {
            let target_indices = medium_items
                .iter()
                .map(|item| item.target_index)
                .collect::<Vec<_>>();
            let query_items = medium_items
                .iter()
                .map(|item| item.point_query.clone())
                .collect::<Vec<_>>();
            let (medium, trace) = execute_batch_contract(
                ctx,
                backend,
                current_snapshot,
                input.query_trace_solver_mode,
                query_contract,
                &[
                    input.region_capture_value(),
                    input.frame_domain.clone(),
                    KernelValue::Array(query_items),
                ],
            )?;
            dispatch_count = dispatch_count.saturating_add(
                trace
                    .observability
                    .dispatch_count
                    .max(u32::from(!target_indices.is_empty())),
            );
            encode_values_at_indices(attachments, attachment_name, &target_indices, &medium)?;
            gpu_runtime.merge_from(&trace.observability.gpu_runtime);
            gpu_runtime.attachment_encode_count = gpu_runtime
                .attachment_encode_count
                .saturating_add(target_indices.len() as u32);
        }
    }
    Ok((
        radiance_count,
        medium_count,
        dispatch_count,
        notes,
        gpu_runtime,
    ))
}

fn execute_participants_resolve_without_screen_samples(
    ctx: &QueryExecContext,
    current_snapshot: &crate::world_identity::WorldSnapshotHandle,
    input: &PresentationExecutionInput,
    camera: crate::presentation_contract::CanonicalCameraInput,
    viewport: crate::presentation_contract::CanonicalViewportInput,
    jitter_pixels: [f32; 2],
    legacy_projection: bool,
    attachments: &mut AttachmentResourceSet,
    hits: &[KernelValue],
    contract: &ParticipantsResolvePassContract,
    backend: DispatchBackend,
    radiance_mode: RealtimeRadianceMode,
) -> Result<(u32, u32, u32, Vec<String>, GpuRuntimeMetrics), PresentationExecError> {
    let mut radiance_count = 0;
    let mut medium_count = 0;
    let mut dispatch_count = 0u32;
    let mut notes = Vec::new();
    let mut gpu_runtime = GpuRuntimeMetrics::default();
    if let (Some(query_contract), Some(attachment_name)) = (
        contract.radiance_query_contract,
        contract.radiance_attachment.as_deref(),
    ) {
        let include_misses = radiance_mode == RealtimeRadianceMode::Full;
        let radiance_items = participant_query_work_items_without_screen_samples(
            input,
            camera,
            viewport,
            jitter_pixels,
            legacy_projection,
            hits,
            attachments,
            attachment_name,
            contract.miss_sample_distance,
            include_misses,
        )?;
        radiance_count = radiance_items.len() as u32;
        if radiance_mode == RealtimeRadianceMode::Reduced {
            notes.push(format!("radiance_mode=reduced items={radiance_count}"));
        }
        if attachments
            .attachment(attachment_name)
            .is_some_and(|attachment| {
                attachment.layout.width != attachments.width
                    || attachment.layout.height != attachments.height
            })
        {
            notes.push(format!("scaled_attachment={attachment_name}"));
        }
        if !radiance_items.is_empty() {
            let target_indices = radiance_items
                .iter()
                .map(|item| item.target_index)
                .collect::<Vec<_>>();
            let query_items = radiance_items
                .iter()
                .map(|item| item.point_direction_query.clone())
                .collect::<Vec<_>>();
            let (radiance, trace) = execute_batch_contract(
                ctx,
                backend,
                current_snapshot,
                input.query_trace_solver_mode,
                query_contract,
                &[
                    input.region_capture_value(),
                    input.frame_domain.clone(),
                    KernelValue::Array(query_items),
                ],
            )?;
            dispatch_count = dispatch_count.saturating_add(
                trace
                    .observability
                    .dispatch_count
                    .max(u32::from(!target_indices.is_empty())),
            );
            encode_values_at_indices(attachments, attachment_name, &target_indices, &radiance)?;
            gpu_runtime.merge_from(&trace.observability.gpu_runtime);
            gpu_runtime.attachment_encode_count = gpu_runtime
                .attachment_encode_count
                .saturating_add(target_indices.len() as u32);
        }
    }
    if let (Some(query_contract), Some(attachment_name)) = (
        contract.medium_query_contract,
        contract.medium_attachment.as_deref(),
    ) {
        let medium_items = participant_query_work_items_without_screen_samples(
            input,
            camera,
            viewport,
            jitter_pixels,
            legacy_projection,
            hits,
            attachments,
            attachment_name,
            contract.miss_sample_distance,
            true,
        )?;
        medium_count = medium_items.len() as u32;
        if attachments
            .attachment(attachment_name)
            .is_some_and(|attachment| {
                attachment.layout.width != attachments.width
                    || attachment.layout.height != attachments.height
            })
        {
            notes.push(format!("scaled_attachment={attachment_name}"));
        }
        if !medium_items.is_empty() {
            let target_indices = medium_items
                .iter()
                .map(|item| item.target_index)
                .collect::<Vec<_>>();
            let query_items = medium_items
                .iter()
                .map(|item| item.point_query.clone())
                .collect::<Vec<_>>();
            let (medium, trace) = execute_batch_contract(
                ctx,
                backend,
                current_snapshot,
                input.query_trace_solver_mode,
                query_contract,
                &[
                    input.region_capture_value(),
                    input.frame_domain.clone(),
                    KernelValue::Array(query_items),
                ],
            )?;
            dispatch_count = dispatch_count.saturating_add(
                trace
                    .observability
                    .dispatch_count
                    .max(u32::from(!target_indices.is_empty())),
            );
            encode_values_at_indices(attachments, attachment_name, &target_indices, &medium)?;
            gpu_runtime.merge_from(&trace.observability.gpu_runtime);
            gpu_runtime.attachment_encode_count = gpu_runtime
                .attachment_encode_count
                .saturating_add(target_indices.len() as u32);
        }
    }
    Ok((
        radiance_count,
        medium_count,
        dispatch_count,
        notes,
        gpu_runtime,
    ))
}

fn execute_packetized_primary_visibility_query(
    ctx: &QueryExecContext,
    current_snapshot: &crate::world_identity::WorldSnapshotHandle,
    batch_plan: &crate::kernel::ir::KernelBatchQueryPlan,
    shape_batch_plan: &crate::kernel::ir::KernelBatchQueryPlan,
    region_capture_value: KernelValue,
    frame_domain: KernelValue,
    rays: &[KernelValue],
    cull_mask: Option<&crate::presentation_exec::TileCullingMask>,
    total_samples: usize,
    selected_workgroup_size: u32,
    solver_mode: crate::query_exec::QueryTraceSolverMode,
) -> Result<
    (
        Vec<KernelValue>,
        BatchQueryExecutionTrace,
        crate::presentation_exec::TileCandidateStats,
        bool,
        u32,
    ),
    PresentationExecError,
> {
    if let Some(mask) = cull_mask {
        let mut hits = vec![primary_hit_miss_value(); total_samples];
        let (tile_candidate, skipped_samples, queue_active, mut query_trace, dispatch_count) =
            if mask.candidate_table.enabled {
                let packet_queue =
                    tile_candidate_dispatch_packets(&mask.candidate_table, selected_workgroup_size);
                let mut covered_samples = vec![false; total_samples];
                let (_, mut query_trace) = execute_batch_query_with_solver_mode_with_snapshot_on(
                    ctx,
                    DispatchBackend::Wgsl,
                    Some(current_snapshot),
                    batch_plan,
                    &[
                        region_capture_value.clone(),
                        frame_domain.clone(),
                        KernelValue::Array(Vec::new()),
                    ],
                    solver_mode,
                )?;
                let mut dispatch_count = query_trace.observability.dispatch_count;
                for packet in &packet_queue {
                    let packet_rays = packet
                        .sample_indices
                        .iter()
                        .map(|index| rays[*index].clone())
                        .collect::<Vec<_>>();
                    let mut packet_hits =
                        vec![primary_hit_miss_value(); packet.sample_indices.len()];
                    let mut packet_best_distances =
                        vec![f32::INFINITY; packet.sample_indices.len()];
                    for shape in &packet.candidate_shapes {
                        let shape_snapshot = ctx.shape_snapshot_handle(shape).ok_or_else(|| {
                            PresentationExecError::UnsupportedPlan {
                                message: format!(
                                    "missing shape snapshot for tile candidate '{shape}'"
                                ),
                            }
                        })?;
                        let (shape_hits, shape_trace) =
                            execute_batch_query_with_solver_mode_with_snapshot_on(
                                ctx,
                                DispatchBackend::Wgsl,
                                Some(shape_snapshot),
                                shape_batch_plan,
                                &[
                                    shape_snapshot.capture_value(),
                                    KernelValue::Array(packet_rays.clone()),
                                ],
                                solver_mode,
                            )?;
                        dispatch_count = dispatch_count.saturating_add(
                            shape_trace
                                .observability
                                .dispatch_count
                                .max(u32::from(!packet.sample_indices.is_empty())),
                        );
                        query_trace
                            .observability
                            .merge_from(&shape_trace.observability);
                        for ((best_hit, best_distance), candidate_hit) in packet_hits
                            .iter_mut()
                            .zip(packet_best_distances.iter_mut())
                            .zip(expect_array(&shape_hits)?.iter())
                        {
                            if !hit_flag(candidate_hit)? {
                                continue;
                            }
                            let candidate_distance = hit_distance(candidate_hit)?;
                            if candidate_distance < *best_distance {
                                *best_distance = candidate_distance;
                                *best_hit = candidate_hit.clone();
                            }
                        }
                    }
                    for (index, hit) in packet.sample_indices.iter().zip(packet_hits.into_iter()) {
                        covered_samples[*index] = true;
                        hits[*index] = hit;
                    }
                }
                let mut fallback_indices = Vec::new();
                for index in &mask.active_samples {
                    if !covered_samples[*index] || !hit_flag(&hits[*index])? {
                        fallback_indices.push(*index);
                    }
                }
                if !fallback_indices.is_empty() {
                    let fallback_rays = fallback_indices
                        .iter()
                        .map(|index| rays[*index].clone())
                        .collect::<Vec<_>>();
                    let (fallback_hits, fallback_trace) =
                        execute_batch_query_with_solver_mode_with_snapshot_on(
                            ctx,
                            DispatchBackend::Wgsl,
                            Some(current_snapshot),
                            batch_plan,
                            &[
                                region_capture_value.clone(),
                                frame_domain.clone(),
                                KernelValue::Array(fallback_rays),
                            ],
                            solver_mode,
                        )?;
                    dispatch_count = dispatch_count.saturating_add(
                        fallback_trace
                            .observability
                            .dispatch_count
                            .max(u32::from(!fallback_indices.is_empty())),
                    );
                    query_trace
                        .observability
                        .merge_from(&fallback_trace.observability);
                    for (index, hit) in fallback_indices
                        .iter()
                        .zip(expect_array(&fallback_hits)?.iter())
                    {
                        hits[*index] = hit.clone();
                    }
                }
                (
                    tile_candidate_stats(
                        mask.active_samples.len(),
                        fallback_indices.len(),
                        packet_queue.len(),
                        selected_workgroup_size,
                    ),
                    mask.stats.skipped_samples,
                    !packet_queue.is_empty(),
                    query_trace,
                    dispatch_count,
                )
            } else {
                let active_rays = mask
                    .active_samples
                    .iter()
                    .map(|index| rays[*index].clone())
                    .collect::<Vec<_>>();
                let (active_hits, packet_trace) =
                    execute_batch_query_with_solver_mode_with_snapshot_on(
                        ctx,
                        DispatchBackend::Wgsl,
                        Some(current_snapshot),
                        batch_plan,
                        &[
                            region_capture_value,
                            frame_domain,
                            KernelValue::Array(active_rays),
                        ],
                        solver_mode,
                    )?;
                for (index, hit) in mask
                    .active_samples
                    .iter()
                    .zip(expect_array(&active_hits)?.to_vec())
                {
                    hits[*index] = hit;
                }
                let dispatch_count = packet_trace
                    .observability
                    .dispatch_count
                    .max(u32::from(!mask.active_samples.is_empty()));
                (
                    crate::presentation_exec::TileCandidateStats::default(),
                    mask.stats.skipped_samples,
                    false,
                    packet_trace,
                    dispatch_count,
                )
            };
        query_trace.observability.screen_sample_count = total_samples as u32;
        query_trace.observability.miss_count = query_trace
            .observability
            .miss_count
            .saturating_add(skipped_samples);
        Ok((
            hits,
            query_trace,
            tile_candidate,
            queue_active,
            dispatch_count,
        ))
    } else {
        let (hits, query_trace) = execute_batch_query_with_solver_mode_with_snapshot_on(
            ctx,
            DispatchBackend::Wgsl,
            Some(current_snapshot),
            batch_plan,
            &[
                region_capture_value,
                frame_domain,
                KernelValue::Array(rays.to_vec()),
            ],
            solver_mode,
        )?;
        let tile_candidate =
            tile_candidate_stats(total_samples, total_samples, 1, total_samples.max(1) as u32);
        let dispatch_count = query_trace
            .observability
            .dispatch_count
            .max(u32::from(!rays.is_empty()));
        Ok((
            expect_array(&hits)?.to_vec(),
            query_trace,
            tile_candidate,
            false,
            dispatch_count,
        ))
    }
}

struct MotionResolveGpuDispatch {
    dispatch_count: u32,
    counts_buffer: wgpu::Buffer,
    counts_readback_label: String,
}

struct TemporalResolveGpuDispatch {
    dispatch_count: u32,
    counts_buffer: wgpu::Buffer,
    counts_readback_label: String,
}

#[derive(Clone)]
struct PresentationCustomPipeline {
    bind_group_layout: wgpu::BindGroupLayout,
    pipeline: wgpu::ComputePipeline,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct PresentationCustomPipelineCacheKey {
    limits: crate::gpu_runtime::GpuLimitRequest,
    source: String,
    label: String,
    workgroup_size: u32,
}

fn storage_buffer_with_usage_and_bytes(
    native: &crate::query_exec::wgsl::NativeWgpuContext,
    label: &str,
    bytes: &[u8],
    usage: wgpu::BufferUsages,
    gpu_runtime: &mut GpuRuntimeMetrics,
) -> wgpu::Buffer {
    let buffer = native.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: bytes.len().max(4) as u64,
        usage: usage | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    if !bytes.is_empty() {
        native.queue.write_buffer(&buffer, 0, bytes);
        gpu_runtime.upload_bytes = gpu_runtime.upload_bytes.saturating_add(bytes.len() as u64);
    }
    gpu_runtime.transient_buffer_creations =
        gpu_runtime.transient_buffer_creations.saturating_add(1);
    buffer
}

fn zeroed_storage_buffer(
    native: &crate::query_exec::wgsl::NativeWgpuContext,
    label: &str,
    size: u64,
    usage: wgpu::BufferUsages,
    gpu_runtime: &mut GpuRuntimeMetrics,
) -> wgpu::Buffer {
    let buffer = native.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: size.max(4),
        usage: usage | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let zeroes = vec![0u8; size.max(4) as usize];
    native.queue.write_buffer(&buffer, 0, &zeroes);
    gpu_runtime.upload_bytes = gpu_runtime.upload_bytes.saturating_add(zeroes.len() as u64);
    gpu_runtime.transient_buffer_creations =
        gpu_runtime.transient_buffer_creations.saturating_add(1);
    buffer
}

fn create_custom_pass_pipeline(
    native: &crate::query_exec::wgsl::NativeWgpuContext,
    source: &str,
    workgroup_size: u32,
    entries: &[wgpu::BindGroupLayoutEntry],
    label: &str,
    gpu_runtime: &mut GpuRuntimeMetrics,
) -> Result<PresentationCustomPipeline, PresentationExecError> {
    let key = PresentationCustomPipelineCacheKey {
        limits: native.limit_request,
        source: source.to_string(),
        label: label.to_string(),
        workgroup_size,
    };
    static PIPELINES: OnceLock<
        Mutex<HashMap<PresentationCustomPipelineCacheKey, PresentationCustomPipeline>>,
    > = OnceLock::new();
    let cache = PIPELINES.get_or_init(|| Mutex::new(HashMap::new()));

    {
        let guard = cache.lock().unwrap_or_else(|poison| poison.into_inner());
        if let Some(cached) = guard.get(&key) {
            gpu_runtime.pipeline_cache_hits = gpu_runtime.pipeline_cache_hits.saturating_add(1);
            return Ok(cached.clone());
        }
    }

    let bind_group_layout =
        native
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some(label),
                entries,
            });
    let pipeline_layout = native
        .device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some(label),
            bind_group_layouts: &{
                let mut layouts = [None; GPU_RUNTIME_BIND_GROUP_COUNT as usize];
                layouts[GPU_RUNTIME_PASS_BIND_GROUP_INDEX as usize] = Some(&bind_group_layout);
                layouts
            },
            immediate_size: 0,
        });
    let shader_module = native
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(label),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(source)),
        });
    let error_scope = native
        .device
        .push_error_scope(wgpu::ErrorFilter::Validation);
    let pipeline = native
        .device
        .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some(label),
            layout: Some(&pipeline_layout),
            module: &shader_module,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: &[("WG_SIZE", workgroup_size as f64)],
                zero_initialize_workgroup_memory: true,
            },
            cache: None,
        });
    native
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|err| PresentationExecError::UnsupportedPlan {
            message: format!("native WGSL device poll failed: {err}"),
        })?;
    if let Some(err) = pollster::block_on(error_scope.pop()) {
        return Err(PresentationExecError::Query(
            crate::query_exec::cpu::QueryExecError::Unsupported {
                message: format!("native WGSL validation failed: {err}"),
            },
        ));
    }
    gpu_runtime.pipeline_cache_misses = gpu_runtime.pipeline_cache_misses.saturating_add(1);
    let cached = PresentationCustomPipeline {
        bind_group_layout,
        pipeline,
    };
    let mut guard = cache.lock().unwrap_or_else(|poison| poison.into_inner());
    Ok(guard.entry(key).or_insert_with(|| cached.clone()).clone())
}

fn encode_shade_primary_gpu(
    native: &crate::query_exec::wgsl::NativeWgpuContext,
    encoder: &mut wgpu::CommandEncoder,
    profiler: &mut GpuPassProfiler,
    arena: &crate::presentation_exec::gpu_resources::GpuAttachmentArena,
    camera: crate::presentation_contract::CanonicalCameraInput,
    viewport: crate::presentation_contract::CanonicalViewportInput,
    jitter_pixels: [f32; 2],
    legacy_projection: bool,
    lighting: &crate::presentation_contract::PresentationLightingInputs,
    contract: &ShadePrimaryPassContract,
    workgroup_size: u32,
    gpu_runtime: &mut GpuRuntimeMetrics,
) -> Result<u32, PresentationExecError> {
    let shader_f16_enabled = native
        .requested_features
        .contains(wgpu::Features::SHADER_F16);
    let Some(primary_hit_buffer) =
        arena.attachment_buffer(contract.primary_hit_attachment.as_str())
    else {
        return Ok(0);
    };
    let Some(surface_buffer) = arena.attachment_buffer(contract.surface_attachment.as_str()) else {
        return Ok(0);
    };
    let Some(output_slot) = arena.attachment(contract.output_attachment.as_str()) else {
        return Ok(0);
    };
    let output_buffer =
        output_slot
            .gpu_buffer()
            .ok_or_else(|| PresentationExecError::UnsupportedPlan {
                message: format!(
                    "attachment '{}' is not GPU-backed",
                    contract.output_attachment
                ),
            })?;
    let radiance_bytes = encode_slice(
        &PortableAbiType::Vec3,
        &[KernelValue::Vec3([0.0, 0.0, 0.0])],
    )
    .map_err(PresentationExecError::Query)?;
    let medium_abi = portable_builtin_record_abi("Medium").expect("Medium abi");
    let medium_bytes =
        encode_slice(&medium_abi, &[default_medium()]).map_err(PresentationExecError::Query)?;
    let radiance_buffer = if let Some(name) = contract.radiance_attachment.as_deref() {
        arena
            .attachment_buffer(name)
            .map(|buffer| buffer.clone())
            .unwrap_or_else(|| {
                storage_buffer_with_usage_and_bytes(
                    native,
                    "wrela.presentation.shade.radiance_default",
                    &radiance_bytes,
                    wgpu::BufferUsages::STORAGE,
                    gpu_runtime,
                )
            })
    } else {
        storage_buffer_with_usage_and_bytes(
            native,
            "wrela.presentation.shade.radiance_default",
            &radiance_bytes,
            wgpu::BufferUsages::STORAGE,
            gpu_runtime,
        )
    };
    let medium_buffer = if let Some(name) = contract.medium_attachment.as_deref() {
        arena
            .attachment_buffer(name)
            .map(|buffer| buffer.clone())
            .unwrap_or_else(|| {
                storage_buffer_with_usage_and_bytes(
                    native,
                    "wrela.presentation.shade.medium_default",
                    &medium_bytes,
                    wgpu::BufferUsages::STORAGE,
                    gpu_runtime,
                )
            })
    } else {
        storage_buffer_with_usage_and_bytes(
            native,
            "wrela.presentation.shade.medium_default",
            &medium_bytes,
            wgpu::BufferUsages::STORAGE,
            gpu_runtime,
        )
    };
    let config_bytes = encode_value(
        &shade_primary_gpu_config_abi(),
        &shade_primary_gpu_config_value(
            camera,
            viewport,
            jitter_pixels,
            legacy_projection,
            lighting,
            arena,
            contract,
        ),
    )
    .map_err(PresentationExecError::Query)?;
    let config_buffer = storage_buffer_with_usage_and_bytes(
        native,
        "wrela.presentation.shade.config",
        &config_bytes,
        wgpu::BufferUsages::STORAGE,
        gpu_runtime,
    );
    let cached = create_custom_pass_pipeline(
        native,
        &shade_primary_gpu_shader_source(workgroup_size, shader_f16_enabled)?,
        workgroup_size,
        &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(config_bytes.len().max(4) as u64),
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 4,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 5,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
        "wrela.presentation.shade.pipeline",
        gpu_runtime,
    )?;
    gpu_runtime.transient_bind_group_creations =
        gpu_runtime.transient_bind_group_creations.saturating_add(1);
    let bind_group = native.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("wrela.presentation.shade.bind_group"),
        layout: &cached.bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: config_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: primary_hit_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: surface_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: radiance_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: medium_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: output_buffer.as_entire_binding(),
            },
        ],
    });
    let timestamp_writes = profiler.compute_pass_timestamp_writes();
    let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
        label: Some("wrela.presentation.shade.compute"),
        timestamp_writes,
    });
    pass.set_pipeline(&cached.pipeline);
    pass.set_bind_group(GPU_RUNTIME_PASS_BIND_GROUP_INDEX, &bind_group, &[]);
    pass.dispatch_workgroups(
        viewport
            .width
            .saturating_mul(viewport.height)
            .div_ceil(workgroup_size.max(1))
            .max(1),
        1,
        1,
    );
    Ok(1)
}

fn encode_motion_resolve_gpu(
    native: &crate::query_exec::wgsl::NativeWgpuContext,
    encoder: &mut wgpu::CommandEncoder,
    profiler: &mut GpuPassProfiler,
    arena: &crate::presentation_exec::gpu_resources::GpuAttachmentArena,
    input: &PresentationExecutionInput,
    viewport: crate::presentation_contract::CanonicalViewportInput,
    contract: &MotionResolvePassContract,
    workgroup_size: u32,
    gpu_runtime: &mut GpuRuntimeMetrics,
    summary: &crate::presentation_exec::temporal::MotionResolveAssessmentSummary,
) -> Result<MotionResolveGpuDispatch, PresentationExecError> {
    let Some(primary_hit_buffer) =
        arena.attachment_buffer(contract.primary_hit_attachment.as_str())
    else {
        return Ok(MotionResolveGpuDispatch {
            dispatch_count: 0,
            counts_buffer: zeroed_storage_buffer(
                native,
                "wrela.presentation.motion.counts.empty",
                12,
                wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                gpu_runtime,
            ),
            counts_readback_label: "wrela.presentation.motion.counts".to_string(),
        });
    };
    let Some(output_buffer) = arena.attachment_buffer(contract.output_attachment.as_str()) else {
        return Ok(MotionResolveGpuDispatch {
            dispatch_count: 0,
            counts_buffer: zeroed_storage_buffer(
                native,
                "wrela.presentation.motion.counts.empty",
                12,
                wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                gpu_runtime,
            ),
            counts_readback_label: "wrela.presentation.motion.counts".to_string(),
        });
    };
    let components = crate::presentation_exec::frame_state_temporal_components(&input.frame_state)?;
    let history_hit_buffer = contract
        .history_primary_hit_attachment
        .as_ref()
        .and_then(|name| arena.attachment_buffer(name.as_str()))
        .map(|buffer| buffer.clone())
        .unwrap_or_else(|| primary_hit_buffer.clone());
    let config_bytes = encode_value(
        &motion_resolve_gpu_config_abi(),
        &motion_resolve_gpu_config_value(
            viewport,
            components.previous_camera,
            components.previous_viewport,
            components.previous_jitter,
            summary.history_available,
            summary.history_rejected,
            contract
                .history_primary_hit_attachment
                .as_ref()
                .is_some_and(|name| arena.attachment_buffer(name.as_str()).is_some()),
        ),
    )
    .map_err(PresentationExecError::Query)?;
    let config_buffer = storage_buffer_with_usage_and_bytes(
        native,
        "wrela.presentation.motion.config",
        &config_bytes,
        wgpu::BufferUsages::STORAGE,
        gpu_runtime,
    );
    let counts_buffer = zeroed_storage_buffer(
        native,
        "wrela.presentation.motion.counts",
        12,
        wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        gpu_runtime,
    );
    let cached = create_custom_pass_pipeline(
        native,
        &motion_resolve_gpu_shader_source(workgroup_size)?,
        workgroup_size,
        &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(config_bytes.len().max(4) as u64),
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 4,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
        "wrela.presentation.motion.pipeline",
        gpu_runtime,
    )?;
    gpu_runtime.transient_bind_group_creations =
        gpu_runtime.transient_bind_group_creations.saturating_add(1);
    let bind_group = native.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("wrela.presentation.motion.bind_group"),
        layout: &cached.bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: config_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: primary_hit_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: history_hit_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: output_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: counts_buffer.as_entire_binding(),
            },
        ],
    });
    let timestamp_writes = profiler.compute_pass_timestamp_writes();
    let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
        label: Some("wrela.presentation.motion.compute"),
        timestamp_writes,
    });
    pass.set_pipeline(&cached.pipeline);
    pass.set_bind_group(GPU_RUNTIME_PASS_BIND_GROUP_INDEX, &bind_group, &[]);
    pass.dispatch_workgroups(
        viewport
            .width
            .saturating_mul(viewport.height)
            .div_ceil(workgroup_size.max(1))
            .max(1),
        1,
        1,
    );
    Ok(MotionResolveGpuDispatch {
        dispatch_count: 1,
        counts_buffer,
        counts_readback_label: "wrela.presentation.motion.counts".to_string(),
    })
}

fn encode_temporal_resolve_gpu(
    native: &crate::query_exec::wgsl::NativeWgpuContext,
    encoder: &mut wgpu::CommandEncoder,
    profiler: &mut GpuPassProfiler,
    arena: &crate::presentation_exec::gpu_resources::GpuAttachmentArena,
    width: u32,
    height: u32,
    contract: &TemporalResolvePassContract,
    workgroup_size: u32,
    gpu_runtime: &mut GpuRuntimeMetrics,
) -> Result<TemporalResolveGpuDispatch, PresentationExecError> {
    let shader_f16_enabled = native
        .requested_features
        .contains(wgpu::Features::SHADER_F16);
    let Some(current_color_buffer) = arena.attachment_buffer(contract.input_attachment.as_str())
    else {
        return Ok(TemporalResolveGpuDispatch {
            dispatch_count: 0,
            counts_buffer: zeroed_storage_buffer(
                native,
                "wrela.presentation.temporal.counts.empty",
                4,
                wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                gpu_runtime,
            ),
            counts_readback_label: "wrela.presentation.temporal.counts".to_string(),
        });
    };
    let Some(history_color_buffer) =
        arena.attachment_buffer(contract.history_color_attachment.as_str())
    else {
        return Ok(TemporalResolveGpuDispatch {
            dispatch_count: 0,
            counts_buffer: zeroed_storage_buffer(
                native,
                "wrela.presentation.temporal.counts.empty",
                4,
                wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                gpu_runtime,
            ),
            counts_readback_label: "wrela.presentation.temporal.counts".to_string(),
        });
    };
    let Some(motion_buffer) = arena.attachment_buffer(contract.motion_attachment.as_str()) else {
        return Ok(TemporalResolveGpuDispatch {
            dispatch_count: 0,
            counts_buffer: zeroed_storage_buffer(
                native,
                "wrela.presentation.temporal.counts.empty",
                4,
                wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                gpu_runtime,
            ),
            counts_readback_label: "wrela.presentation.temporal.counts".to_string(),
        });
    };
    let Some(output_slot) = arena.attachment(contract.output_attachment.as_str()) else {
        return Ok(TemporalResolveGpuDispatch {
            dispatch_count: 0,
            counts_buffer: zeroed_storage_buffer(
                native,
                "wrela.presentation.temporal.counts.empty",
                4,
                wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                gpu_runtime,
            ),
            counts_readback_label: "wrela.presentation.temporal.counts".to_string(),
        });
    };
    let output_buffer =
        output_slot
            .gpu_buffer()
            .ok_or_else(|| PresentationExecError::UnsupportedPlan {
                message: format!(
                    "attachment '{}' is not GPU-backed",
                    contract.output_attachment
                ),
            })?;
    let config_bytes = encode_value(
        &temporal_resolve_gpu_config_abi(),
        &temporal_resolve_gpu_config_value(width, height, contract),
    )
    .map_err(PresentationExecError::Query)?;
    let config_buffer = storage_buffer_with_usage_and_bytes(
        native,
        "wrela.presentation.temporal.config",
        &config_bytes,
        wgpu::BufferUsages::STORAGE,
        gpu_runtime,
    );
    let counts_buffer = zeroed_storage_buffer(
        native,
        "wrela.presentation.temporal.counts",
        4,
        wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        gpu_runtime,
    );
    let cached = create_custom_pass_pipeline(
        native,
        &temporal_resolve_gpu_shader_source(workgroup_size, shader_f16_enabled)?,
        workgroup_size,
        &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(config_bytes.len().max(4) as u64),
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 4,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 5,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
        "wrela.presentation.temporal.pipeline",
        gpu_runtime,
    )?;
    gpu_runtime.transient_bind_group_creations =
        gpu_runtime.transient_bind_group_creations.saturating_add(1);
    let bind_group = native.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("wrela.presentation.temporal.bind_group"),
        layout: &cached.bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: config_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: current_color_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: history_color_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: motion_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: output_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: counts_buffer.as_entire_binding(),
            },
        ],
    });
    let timestamp_writes = profiler.compute_pass_timestamp_writes();
    let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
        label: Some("wrela.presentation.temporal.compute"),
        timestamp_writes,
    });
    pass.set_pipeline(&cached.pipeline);
    pass.set_bind_group(GPU_RUNTIME_PASS_BIND_GROUP_INDEX, &bind_group, &[]);
    pass.dispatch_workgroups(
        width
            .saturating_mul(height)
            .div_ceil(workgroup_size.max(1))
            .max(1),
        1,
        1,
    );
    drop(pass);
    if contract.output_attachment != contract.history_color_attachment {
        encoder.copy_buffer_to_buffer(
            output_buffer,
            0,
            history_color_buffer,
            0,
            output_slot.layout.total_size as u64,
        );
    }
    if let Some(history_primary_hit_attachment) = &contract.history_primary_hit_attachment
        && let (Some(primary_hit_buffer), Some(history_primary_hit_buffer)) = (
            arena.attachment_buffer(contract.primary_hit_attachment.as_str()),
            arena.attachment_buffer(history_primary_hit_attachment.as_str()),
        )
        && let Some(primary_hit_slot) = arena.attachment(contract.primary_hit_attachment.as_str())
    {
        encoder.copy_buffer_to_buffer(
            primary_hit_buffer,
            0,
            history_primary_hit_buffer,
            0,
            primary_hit_slot.layout.total_size as u64,
        );
    }
    Ok(TemporalResolveGpuDispatch {
        dispatch_count: 1,
        counts_buffer,
        counts_readback_label: "wrela.presentation.temporal.counts".to_string(),
    })
}

fn encode_composite_color_gpu(
    native: &crate::query_exec::wgsl::NativeWgpuContext,
    encoder: &mut wgpu::CommandEncoder,
    profiler: &mut GpuPassProfiler,
    arena: &crate::presentation_exec::gpu_resources::GpuAttachmentArena,
    contract: &CompositeColorPassContract,
    workgroup_size: u32,
    gpu_runtime: &mut GpuRuntimeMetrics,
) -> Result<u32, PresentationExecError> {
    let shader_f16_enabled = native
        .requested_features
        .contains(wgpu::Features::SHADER_F16);
    let Some(input_buffer) = arena.attachment_buffer(contract.input_attachment.as_str()) else {
        return Ok(0);
    };
    let Some(output_buffer) = arena.attachment_buffer(contract.output_attachment.as_str()) else {
        return Ok(0);
    };
    let item_count = arena
        .attachment(contract.output_attachment.as_str())
        .map(|slot| slot.layout.width.saturating_mul(slot.layout.height))
        .unwrap_or_default();
    let dispatch_bytes = encode_value(
        &crate::query_exec::wgsl::codegen::wgsl_dispatch_config_abi(),
        &presentation_dispatch_config(item_count),
    )
    .map_err(PresentationExecError::Query)?;
    let config_buffer = storage_buffer_with_usage_and_bytes(
        native,
        "wrela.presentation.composite.dispatch",
        &dispatch_bytes,
        wgpu::BufferUsages::STORAGE,
        gpu_runtime,
    );
    let dummy_buffer = zeroed_storage_buffer(
        native,
        "wrela.presentation.composite.dummy",
        4,
        wgpu::BufferUsages::STORAGE,
        gpu_runtime,
    );
    let cached = create_custom_pass_pipeline(
        native,
        &copy_vec3_shader_source(workgroup_size, shader_f16_enabled)?,
        workgroup_size,
        &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(dispatch_bytes.len().max(4) as u64),
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
        "wrela.presentation.composite.pipeline",
        gpu_runtime,
    )?;
    gpu_runtime.transient_bind_group_creations =
        gpu_runtime.transient_bind_group_creations.saturating_add(1);
    let bind_group = native.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("wrela.presentation.composite.bind_group"),
        layout: &cached.bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: config_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: input_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: output_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: dummy_buffer.as_entire_binding(),
            },
        ],
    });
    let timestamp_writes = profiler.compute_pass_timestamp_writes();
    let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
        label: Some("wrela.presentation.composite.compute"),
        timestamp_writes,
    });
    pass.set_pipeline(&cached.pipeline);
    pass.set_bind_group(GPU_RUNTIME_PASS_BIND_GROUP_INDEX, &bind_group, &[]);
    pass.dispatch_workgroups(item_count.div_ceil(workgroup_size.max(1)).max(1), 1, 1);
    Ok(1)
}

#[cfg(test)]
fn legacy_test_only_shade_primary_wgsl(
    screen_samples: &[KernelValue],
    attachments: &mut AttachmentResourceSet,
    lighting: &crate::presentation_contract::PresentationLightingInputs,
    camera_position: [f32; 3],
    contract: &ShadePrimaryPassContract,
    workgroup_size: u32,
    gpu_runtime: &mut GpuRuntimeMetrics,
) -> Result<u32, PresentationExecError> {
    let primary_hits = attachments.decode_attachment(contract.primary_hit_attachment.as_str())?;
    gpu_runtime.attachment_decode_count = gpu_runtime.attachment_decode_count.saturating_add(1);
    let default_surface = default_surface();
    let default_medium = default_medium();
    let shade_inputs = primary_hits
        .iter()
        .enumerate()
        .map(|(index, hit)| {
            let sample = screen_samples.get(index).expect("screen sample");
            let ray = expect_struct(
                field(expect_struct(sample, "ScreenSampleQuery")?, "ray")?,
                "RayQuery",
            )?;
            let ray_direction = expect_vec3(field(ray, "direction")?)?;
            let surface = shade_lookup_value(
                attachments,
                contract.surface_attachment.as_str(),
                index,
                &default_surface,
            )?;
            Ok(KernelValue::Struct(KernelStructValue {
                name: SmolStr::new("ShadePrimaryInput"),
                fields: vec![
                    (SmolStr::new("hit"), hit.clone()),
                    (SmolStr::new("surface"), surface),
                    (
                        SmolStr::new("radiance"),
                        contract
                            .radiance_attachment
                            .as_ref()
                            .map(|name| {
                                shade_lookup_value(
                                    attachments,
                                    name,
                                    index,
                                    &KernelValue::Vec3([0.0, 0.0, 0.0]),
                                )
                            })
                            .transpose()?
                            .unwrap_or(KernelValue::Vec3([0.0, 0.0, 0.0])),
                    ),
                    (
                        SmolStr::new("medium"),
                        contract
                            .medium_attachment
                            .as_ref()
                            .map(|name| {
                                shade_lookup_value(attachments, name, index, &default_medium)
                            })
                            .transpose()?
                            .unwrap_or_else(|| default_medium.clone()),
                    ),
                    (
                        SmolStr::new("ray_direction"),
                        KernelValue::Vec3(ray_direction),
                    ),
                    (
                        SmolStr::new("camera_position"),
                        KernelValue::Vec3(camera_position),
                    ),
                    (SmolStr::new("lighting"), lighting_inputs_value(*lighting)),
                ],
            }))
        })
        .collect::<Result<Vec<_>, PresentationExecError>>()?;

    let output_layout = attachments
        .attachment(contract.output_attachment.as_str())
        .ok_or_else(|| PresentationExecError::UnsupportedPlan {
            message: format!(
                "missing shade output attachment '{}'",
                contract.output_attachment
            ),
        })?
        .layout
        .plan
        .clone();
    let output_attachment = attachments
        .attachment_mut(contract.output_attachment.as_str())
        .ok_or_else(|| PresentationExecError::UnsupportedPlan {
            message: format!(
                "missing shade output attachment '{}'",
                contract.output_attachment
            ),
        })?;
    let dispatch = legacy_test_only_dispatch_linear_shader(
        &shade_primary_shader_source(workgroup_size, false)?,
        &shade_primary_input_abi(),
        &shade_inputs,
        &output_layout,
        workgroup_size,
        gpu_runtime,
    )?;
    output_attachment.bytes = dispatch.bytes;
    gpu_runtime.attachment_encode_count = gpu_runtime.attachment_encode_count.saturating_add(1);
    Ok(dispatch.dispatch_count)
}

#[cfg(test)]
fn legacy_test_only_composite_color_wgsl(
    attachments: &mut AttachmentResourceSet,
    contract: &CompositeColorPassContract,
    workgroup_size: u32,
    gpu_runtime: &mut GpuRuntimeMetrics,
) -> Result<u32, PresentationExecError> {
    let input_values = attachments.decode_attachment(contract.input_attachment.as_str())?;
    gpu_runtime.attachment_decode_count = gpu_runtime.attachment_decode_count.saturating_add(1);
    let output_layout = attachments
        .attachment(contract.output_attachment.as_str())
        .ok_or_else(|| PresentationExecError::UnsupportedPlan {
            message: format!(
                "missing composite output attachment '{}'",
                contract.output_attachment
            ),
        })?
        .layout
        .plan
        .clone();
    let output_attachment = attachments
        .attachment_mut(contract.output_attachment.as_str())
        .ok_or_else(|| PresentationExecError::UnsupportedPlan {
            message: format!(
                "missing composite output attachment '{}'",
                contract.output_attachment
            ),
        })?;
    let dispatch = legacy_test_only_dispatch_linear_shader(
        &copy_vec3_shader_source(workgroup_size, false)?,
        &PortableAbiType::Vec3,
        &input_values,
        &output_layout,
        workgroup_size,
        gpu_runtime,
    )?;
    output_attachment.bytes = dispatch.bytes;
    gpu_runtime.attachment_encode_count = gpu_runtime.attachment_encode_count.saturating_add(1);
    Ok(dispatch.dispatch_count)
}

#[cfg(test)]
fn legacy_test_only_temporal_resolve_wgsl(
    attachments: &mut AttachmentResourceSet,
    width: u32,
    height: u32,
    contract: &TemporalResolvePassContract,
    workgroup_size: u32,
    gpu_runtime: &mut GpuRuntimeMetrics,
) -> Result<LegacyTestOnlyTemporalResolveDispatchResult, PresentationExecError> {
    let (input_values, consumed_count) =
        temporal_resolve_kernel_values(attachments, width, height, contract)?;
    let output_layout = attachments
        .attachment(contract.output_attachment.as_str())
        .ok_or_else(|| PresentationExecError::UnsupportedPlan {
            message: format!(
                "missing temporal resolve output attachment '{}'",
                contract.output_attachment
            ),
        })?
        .layout
        .plan
        .clone();
    if input_values.is_empty() {
        return Ok(LegacyTestOnlyTemporalResolveDispatchResult {
            consumed_count,
            dispatch_count: 0,
        });
    }
    let dispatch = legacy_test_only_dispatch_linear_shader(
        &temporal_resolve_shader_source(contract, workgroup_size, false)?,
        &temporal_resolve_input_abi(),
        &input_values,
        &output_layout,
        workgroup_size,
        gpu_runtime,
    )?;
    attachments
        .attachment_mut(contract.output_attachment.as_str())
        .expect("temporal output attachment")
        .bytes = dispatch.bytes.clone();
    gpu_runtime.attachment_encode_count = gpu_runtime.attachment_encode_count.saturating_add(2);
    if let Some(history_color) =
        attachments.attachment_mut(contract.history_color_attachment.as_str())
    {
        history_color.bytes = dispatch.bytes;
    }
    if let Some(history_primary_hit_attachment) = &contract.history_primary_hit_attachment {
        let primary_hits =
            attachments.decode_attachment(contract.primary_hit_attachment.as_str())?;
        gpu_runtime.attachment_decode_count = gpu_runtime.attachment_decode_count.saturating_add(1);
        if let Some(history_primary_hit) =
            attachments.attachment_mut(history_primary_hit_attachment.as_str())
        {
            for (index, hit) in primary_hits.iter().enumerate() {
                history_primary_hit.encode(index, hit)?;
                gpu_runtime.attachment_encode_count =
                    gpu_runtime.attachment_encode_count.saturating_add(1);
            }
        }
    }
    Ok(LegacyTestOnlyTemporalResolveDispatchResult {
        consumed_count,
        dispatch_count: dispatch.dispatch_count,
    })
}

#[cfg(test)]
fn legacy_test_only_dispatch_linear_shader(
    source: &str,
    input_abi: &PortableAbiType,
    input_values: &[KernelValue],
    output_layout: &FrameAttachmentLayoutPlan,
    workgroup_size: u32,
    gpu_runtime: &mut GpuRuntimeMetrics,
) -> Result<LegacyTestOnlyLinearShaderDispatchResult, PresentationExecError> {
    legacy_test_only_dispatch_linear_shader_with_chunk_limit(
        source,
        input_abi,
        input_values,
        output_layout,
        workgroup_size,
        None,
        gpu_runtime,
    )
}

// Legacy/test-only helper for CPU-bounce WGSL verification paths. The timed resident framegraph
// must not route through this immediate-readback loop, so we compile it only for tests.
#[cfg(test)]
fn legacy_test_only_dispatch_linear_shader_with_chunk_limit(
    source: &str,
    input_abi: &PortableAbiType,
    input_values: &[KernelValue],
    output_layout: &FrameAttachmentLayoutPlan,
    workgroup_size: u32,
    per_storage_buffer_limit_override: Option<u64>,
    gpu_runtime: &mut GpuRuntimeMetrics,
) -> Result<LegacyTestOnlyLinearShaderDispatchResult, PresentationExecError> {
    if input_values.is_empty() {
        return Ok(LegacyTestOnlyLinearShaderDispatchResult {
            bytes: Vec::new(),
            dispatch_count: 0,
        });
    }
    let dense_output_size = output_layout.dense_output_size() as u64;
    let native = native_wgpu_context()?;
    let dispatch_abi = crate::query_exec::wgsl::codegen::wgsl_dispatch_config_abi();
    let per_storage_buffer_limit = per_storage_buffer_limit_override.unwrap_or_else(|| {
        native
            .requested_limits
            .max_storage_buffer_binding_size
            .min(native.requested_limits.max_buffer_size)
    });
    let items_per_chunk = crate::query_exec::wgsl::max_chunk_item_count(
        per_storage_buffer_limit,
        portable_abi_array_stride(input_abi) as u64,
        output_layout.physical.element_stride as u64,
        None,
    )
    .map_err(PresentationExecError::Query)?;
    let chunk_count = input_values.len().div_ceil(items_per_chunk);
    let mut profiler = GpuPassProfiler::new(&native, chunk_count as u32);
    let mut local_gpu_runtime = GpuRuntimeMetrics {
        ..GpuRuntimeMetrics::default()
    };
    local_gpu_runtime.note_context_metadata(&native);
    let cached = compiled_pipeline(
        &native,
        source,
        workgroup_size,
        GPU_RUNTIME_PASS_BIND_GROUP_INDEX,
        wgpu::BufferSize::new(portable_abi_layout(&dispatch_abi).size as u64),
        &mut local_gpu_runtime,
    )?;
    let dispatch_bytes_size = portable_abi_layout(&dispatch_abi).size as u64;
    let input_buffer_size =
        ((items_per_chunk as u64) * portable_abi_array_stride(input_abi) as u64).max(4);
    let output_buffer_size =
        ((items_per_chunk as u64) * output_layout.physical.element_stride as u64).max(4);
    let mut leased_buffers = Vec::new();
    let (aux_buffer, aux_pool_key) = acquire_presentation_upload_buffer(
        &native,
        4,
        wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        Some("wrela.presentation.aux"),
        &mut local_gpu_runtime,
    );
    leased_buffers.push((aux_pool_key, aux_buffer.clone()));
    let (dispatch_buffer, dispatch_pool_key) = acquire_presentation_upload_buffer(
        &native,
        dispatch_bytes_size,
        wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        Some("wrela.presentation.dispatch"),
        &mut local_gpu_runtime,
    );
    leased_buffers.push((dispatch_pool_key, dispatch_buffer.clone()));
    let (input_buffer, input_pool_key) = acquire_presentation_upload_buffer(
        &native,
        input_buffer_size,
        wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        Some("wrela.presentation.input"),
        &mut local_gpu_runtime,
    );
    leased_buffers.push((input_pool_key, input_buffer.clone()));
    let (output_buffer, output_pool_key) = acquire_presentation_upload_buffer(
        &native,
        output_buffer_size,
        wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        Some("wrela.presentation.output"),
        &mut local_gpu_runtime,
    );
    leased_buffers.push((output_pool_key, output_buffer.clone()));
    let mut upload_arena = lock_shared_upload_arena(
        native.limit_request,
        &native.device,
        dispatch_bytes_size.max(input_buffer_size).max(4),
    );
    upload_arena.set_scratch_encoder(native.device.create_command_encoder(
        &wgpu::CommandEncoderDescriptor {
            label: Some("wrela.presentation.upload_init"),
        },
    ));
    local_gpu_runtime.upload_bytes = local_gpu_runtime.upload_bytes.saturating_add(
        upload_arena
            .write_storage_bytes(&aux_buffer, 0, &[0u8; 4])
            .map_err(|err| {
                PresentationExecError::Query(crate::query_exec::cpu::QueryExecError::Unsupported {
                    message: format!("presentation aux upload failed: {err:?}"),
                })
            })?,
    );
    if let Some(upload_commands) = upload_arena.finish() {
        native.queue.submit(Some(upload_commands));
        local_gpu_runtime.queue_submit_count =
            local_gpu_runtime.queue_submit_count.saturating_add(1);
    }
    local_gpu_runtime.transient_bind_group_creations = local_gpu_runtime
        .transient_bind_group_creations
        .saturating_add(1);
    let bind_group = native.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("wrela.presentation.bind_group"),
        layout: &cached.bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: dispatch_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: input_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: output_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: aux_buffer.as_entire_binding(),
            },
        ],
    });
    let mut dense_bytes = vec![0u8; dense_output_size as usize];
    let mut dispatch_count = 0u32;
    for (chunk_index, chunk) in input_values.chunks(items_per_chunk).enumerate() {
        let chunk_stride = output_layout.physical.element_stride as usize;
        let chunk_start = chunk_index * items_per_chunk;
        let chunk_dense_size = (chunk.len() * chunk_stride).max(4) as u64;
        let dispatch_bytes = encode_value(
            &dispatch_abi,
            &presentation_dispatch_config(chunk.len() as u32),
        )
        .map_err(PresentationExecError::Query)?;
        let input_bytes = encode_slice(input_abi, chunk).map_err(PresentationExecError::Query)?;
        upload_arena.set_scratch_encoder(native.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor {
                label: Some("wrela.presentation.upload_encoder"),
            },
        ));
        local_gpu_runtime.upload_bytes = local_gpu_runtime
            .upload_bytes
            .saturating_add(
                upload_arena
                    .write_storage_bytes(&dispatch_buffer, 0, &dispatch_bytes)
                    .map_err(|err| {
                        PresentationExecError::Query(
                            crate::query_exec::cpu::QueryExecError::Unsupported {
                                message: format!("presentation dispatch upload failed: {err:?}"),
                            },
                        )
                    })?,
            )
            .saturating_add(
                upload_arena
                    .write_storage_bytes(&input_buffer, 0, &input_bytes)
                    .map_err(|err| {
                        PresentationExecError::Query(
                            crate::query_exec::cpu::QueryExecError::Unsupported {
                                message: format!("presentation input upload failed: {err:?}"),
                            },
                        )
                    })?,
            );
        if let Some(upload_commands) = upload_arena.finish() {
            native.queue.submit(Some(upload_commands));
            local_gpu_runtime.queue_submit_count =
                local_gpu_runtime.queue_submit_count.saturating_add(1);
        }
        let mut encoder = native
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("wrela.presentation.encoder"),
            });
        {
            let timestamp_writes = profiler.compute_pass_timestamp_writes();
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("wrela.presentation.compute"),
                timestamp_writes,
            });
            pass.set_pipeline(&cached.pipeline);
            pass.set_bind_group(GPU_RUNTIME_PASS_BIND_GROUP_INDEX, &bind_group, &[]);
            pass.dispatch_workgroups(
                (chunk.len() as u32).div_ceil(workgroup_size.max(1)).max(1),
                1,
                1,
            );
        }
        profiler.resolve_into(&mut encoder);
        native.queue.submit(Some(encoder.finish()));
        dispatch_count = dispatch_count.saturating_add(1);
        local_gpu_runtime.queue_submit_count =
            local_gpu_runtime.queue_submit_count.saturating_add(1);
        let chunk_bytes = legacy_test_only_readback_storage_buffer(
            &output_buffer,
            chunk_dense_size,
        )
        .map_err(|message| {
            PresentationExecError::Query(crate::query_exec::cpu::QueryExecError::Unsupported {
                message: format!("native WGSL readback failed: {message}"),
            })
        })?;
        upload_arena.recall();
        local_gpu_runtime.queue_submit_count =
            local_gpu_runtime.queue_submit_count.saturating_add(1);
        local_gpu_runtime.transient_buffer_creations = local_gpu_runtime
            .transient_buffer_creations
            .saturating_add(1);
        local_gpu_runtime.readback_bytes = local_gpu_runtime
            .readback_bytes
            .saturating_add(chunk_dense_size);
        let chunk_byte_offset = chunk_start * chunk_stride;
        let chunk_byte_end = chunk_byte_offset + chunk.len() * chunk_stride;
        dense_bytes[chunk_byte_offset..chunk_byte_end]
            .copy_from_slice(&chunk_bytes[..chunk_byte_end - chunk_byte_offset]);
    }
    upload_arena.recall();
    release_presentation_upload_buffers(&native, leased_buffers);
    let gpu_elapsed_micros = profiler
        .readback_gpu_elapsed_micros(&native)
        .map_err(|message| {
            PresentationExecError::Query(crate::query_exec::cpu::QueryExecError::Unsupported {
                message: format!("native WGSL GPU timing readback failed: {message}"),
            })
        })?;
    local_gpu_runtime.note_gpu_timings(profiler.timestamps_supported(), &gpu_elapsed_micros);
    if profiler.timestamps_supported() {
        local_gpu_runtime.queue_submit_count =
            local_gpu_runtime.queue_submit_count.saturating_add(1);
        local_gpu_runtime.transient_buffer_creations = local_gpu_runtime
            .transient_buffer_creations
            .saturating_add(1);
        local_gpu_runtime.readback_bytes = local_gpu_runtime
            .readback_bytes
            .saturating_add((gpu_elapsed_micros.len() as u64) * 16);
    }
    gpu_runtime.merge_from(&local_gpu_runtime);
    Ok(LegacyTestOnlyLinearShaderDispatchResult {
        bytes: output_layout
            .pack_dense_output_bytes(&dense_bytes)
            .map_err(PresentationExecError::Resource)?,
        dispatch_count,
    })
}

fn presentation_dispatch_config(item_count: u32) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("WgslDispatchConfig"),
        fields: vec![
            (SmolStr::new("capture_kind"), KernelValue::U32(0)),
            (SmolStr::new("capture_index"), KernelValue::U32(0)),
            (SmolStr::new("item_count"), KernelValue::U32(item_count)),
            (SmolStr::new("shape_count"), KernelValue::U32(0)),
            (SmolStr::new("accel_root_index"), KernelValue::U32(0)),
            (SmolStr::new("accel_node_count"), KernelValue::U32(0)),
            (SmolStr::new("cache_brick_count"), KernelValue::U32(0)),
            (SmolStr::new("material_enabled"), KernelValue::Bool(false)),
            (SmolStr::new("radiance_enabled"), KernelValue::Bool(false)),
            (SmolStr::new("media_enabled"), KernelValue::Bool(false)),
            (
                SmolStr::new("candidate_spans_enabled"),
                KernelValue::Bool(false),
            ),
        ],
    })
}

#[cfg(test)]
fn shade_primary_input_abi() -> PortableAbiType {
    PortableAbiType::Struct {
        name: SmolStr::new("ShadePrimaryInput"),
        class_id: 0,
        fields: vec![
            PortableStructField {
                name: SmolStr::new("hit"),
                ty: portable_builtin_record_abi("Hit3").expect("Hit3 abi"),
            },
            PortableStructField {
                name: SmolStr::new("surface"),
                ty: portable_builtin_record_abi("Surface").expect("Surface abi"),
            },
            PortableStructField {
                name: SmolStr::new("radiance"),
                ty: PortableAbiType::Vec3,
            },
            PortableStructField {
                name: SmolStr::new("medium"),
                ty: portable_builtin_record_abi("Medium").expect("Medium abi"),
            },
            PortableStructField {
                name: SmolStr::new("ray_direction"),
                ty: PortableAbiType::Vec3,
            },
            PortableStructField {
                name: SmolStr::new("camera_position"),
                ty: PortableAbiType::Vec3,
            },
            PortableStructField {
                name: SmolStr::new("lighting"),
                ty: lighting_inputs_abi(),
            },
        ],
    }
}

fn lighting_inputs_abi() -> PortableAbiType {
    PortableAbiType::Struct {
        name: SmolStr::new("PresentationLightingInputs"),
        class_id: 0,
        fields: vec![
            PortableStructField {
                name: SmolStr::new("key_light"),
                ty: portable_builtin_record_abi("Light").expect("Light abi"),
            },
            PortableStructField {
                name: SmolStr::new("fill_direction"),
                ty: PortableAbiType::Vec3,
            },
            PortableStructField {
                name: SmolStr::new("fill_strength"),
                ty: PortableAbiType::F32,
            },
            PortableStructField {
                name: SmolStr::new("ambient_color"),
                ty: PortableAbiType::Vec3,
            },
        ],
    }
}

#[cfg(test)]
fn temporal_resolve_input_abi() -> PortableAbiType {
    PortableAbiType::Struct {
        name: SmolStr::new("TemporalResolveInput"),
        class_id: 0,
        fields: vec![
            PortableStructField {
                name: SmolStr::new("current_color"),
                ty: PortableAbiType::Vec3,
            },
            PortableStructField {
                name: SmolStr::new("history_color"),
                ty: PortableAbiType::Vec3,
            },
            PortableStructField {
                name: SmolStr::new("clamp_min"),
                ty: PortableAbiType::Vec3,
            },
            PortableStructField {
                name: SmolStr::new("clamp_max"),
                ty: PortableAbiType::Vec3,
            },
            PortableStructField {
                name: SmolStr::new("use_history"),
                ty: PortableAbiType::Bool,
            },
        ],
    }
}

fn attachment_dims(
    arena: &crate::presentation_exec::gpu_resources::GpuAttachmentArena,
    name: Option<&str>,
) -> (u32, u32) {
    name.and_then(|attachment| arena.attachment(attachment))
        .map(|slot| (slot.layout.width.max(1), slot.layout.height.max(1)))
        .unwrap_or((1, 1))
}

fn shade_primary_gpu_config_abi() -> PortableAbiType {
    PortableAbiType::Struct {
        name: SmolStr::new("ShadePrimaryGpuConfig"),
        class_id: 0,
        fields: vec![
            PortableStructField {
                name: SmolStr::new("item_count"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("viewport_width"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("viewport_height"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("surface_width"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("surface_height"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("radiance_width"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("radiance_height"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("medium_width"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("medium_height"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("camera_position"),
                ty: PortableAbiType::Vec3,
            },
            PortableStructField {
                name: SmolStr::new("forward"),
                ty: PortableAbiType::Vec3,
            },
            PortableStructField {
                name: SmolStr::new("up"),
                ty: PortableAbiType::Vec3,
            },
            PortableStructField {
                name: SmolStr::new("vertical_fov_degrees"),
                ty: PortableAbiType::F32,
            },
            PortableStructField {
                name: SmolStr::new("jitter"),
                ty: PortableAbiType::Vec2,
            },
            PortableStructField {
                name: SmolStr::new("legacy_world_up"),
                ty: PortableAbiType::Vec3,
            },
            PortableStructField {
                name: SmolStr::new("legacy_view_scale"),
                ty: PortableAbiType::F32,
            },
            PortableStructField {
                name: SmolStr::new("legacy_active"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("radiance_active"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("medium_active"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("lighting"),
                ty: lighting_inputs_abi(),
            },
        ],
    }
}

fn shade_primary_gpu_config_value(
    camera: crate::presentation_contract::CanonicalCameraInput,
    viewport: crate::presentation_contract::CanonicalViewportInput,
    jitter_pixels: [f32; 2],
    legacy_projection: bool,
    lighting: &crate::presentation_contract::PresentationLightingInputs,
    arena: &crate::presentation_exec::gpu_resources::GpuAttachmentArena,
    contract: &ShadePrimaryPassContract,
) -> KernelValue {
    let compatibility = crate::presentation_contract::LegacyCompatibilityProjectionInput {
        world_up: camera.up,
        view_scale: 0.72,
    };
    let (surface_width, surface_height) =
        attachment_dims(arena, Some(contract.surface_attachment.as_str()));
    let (radiance_width, radiance_height) =
        attachment_dims(arena, contract.radiance_attachment.as_deref());
    let (medium_width, medium_height) =
        attachment_dims(arena, contract.medium_attachment.as_deref());
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("ShadePrimaryGpuConfig"),
        fields: vec![
            (
                SmolStr::new("item_count"),
                KernelValue::U32(viewport.width.saturating_mul(viewport.height)),
            ),
            (
                SmolStr::new("viewport_width"),
                KernelValue::U32(viewport.width),
            ),
            (
                SmolStr::new("viewport_height"),
                KernelValue::U32(viewport.height),
            ),
            (
                SmolStr::new("surface_width"),
                KernelValue::U32(surface_width),
            ),
            (
                SmolStr::new("surface_height"),
                KernelValue::U32(surface_height),
            ),
            (
                SmolStr::new("radiance_width"),
                KernelValue::U32(radiance_width),
            ),
            (
                SmolStr::new("radiance_height"),
                KernelValue::U32(radiance_height),
            ),
            (SmolStr::new("medium_width"), KernelValue::U32(medium_width)),
            (
                SmolStr::new("medium_height"),
                KernelValue::U32(medium_height),
            ),
            (
                SmolStr::new("camera_position"),
                KernelValue::Vec3(camera.position),
            ),
            (SmolStr::new("forward"), KernelValue::Vec3(camera.forward)),
            (SmolStr::new("up"), KernelValue::Vec3(camera.up)),
            (
                SmolStr::new("vertical_fov_degrees"),
                KernelValue::F32(camera.vertical_fov_degrees),
            ),
            (SmolStr::new("jitter"), KernelValue::Vec2(jitter_pixels)),
            (
                SmolStr::new("legacy_world_up"),
                KernelValue::Vec3(compatibility.world_up),
            ),
            (
                SmolStr::new("legacy_view_scale"),
                KernelValue::F32(compatibility.view_scale),
            ),
            (
                SmolStr::new("legacy_active"),
                KernelValue::U32(u32::from(legacy_projection)),
            ),
            (
                SmolStr::new("radiance_active"),
                KernelValue::U32(u32::from(contract.radiance_attachment.is_some())),
            ),
            (
                SmolStr::new("medium_active"),
                KernelValue::U32(u32::from(contract.medium_attachment.is_some())),
            ),
            (SmolStr::new("lighting"), lighting_inputs_value(*lighting)),
        ],
    })
}

fn motion_resolve_gpu_config_abi() -> PortableAbiType {
    PortableAbiType::Struct {
        name: SmolStr::new("MotionResolveGpuConfig"),
        class_id: 0,
        fields: vec![
            PortableStructField {
                name: SmolStr::new("item_count"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("viewport_width"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("viewport_height"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("previous_viewport_width"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("previous_viewport_height"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("previous_camera_position"),
                ty: PortableAbiType::Vec3,
            },
            PortableStructField {
                name: SmolStr::new("previous_forward"),
                ty: PortableAbiType::Vec3,
            },
            PortableStructField {
                name: SmolStr::new("previous_up"),
                ty: PortableAbiType::Vec3,
            },
            PortableStructField {
                name: SmolStr::new("previous_vertical_fov_degrees"),
                ty: PortableAbiType::F32,
            },
            PortableStructField {
                name: SmolStr::new("previous_jitter"),
                ty: PortableAbiType::Vec2,
            },
            PortableStructField {
                name: SmolStr::new("history_available"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("history_rejected"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("has_history_primary_hit"),
                ty: PortableAbiType::U32,
            },
        ],
    }
}

fn motion_resolve_gpu_config_value(
    viewport: crate::presentation_contract::CanonicalViewportInput,
    previous_camera: crate::presentation_contract::CanonicalCameraInput,
    previous_viewport: crate::presentation_contract::CanonicalViewportInput,
    previous_jitter: [f32; 2],
    history_available: bool,
    history_rejected: bool,
    has_history_primary_hit: bool,
) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("MotionResolveGpuConfig"),
        fields: vec![
            (
                SmolStr::new("item_count"),
                KernelValue::U32(viewport.width.saturating_mul(viewport.height)),
            ),
            (
                SmolStr::new("viewport_width"),
                KernelValue::U32(viewport.width),
            ),
            (
                SmolStr::new("viewport_height"),
                KernelValue::U32(viewport.height),
            ),
            (
                SmolStr::new("previous_viewport_width"),
                KernelValue::U32(previous_viewport.width),
            ),
            (
                SmolStr::new("previous_viewport_height"),
                KernelValue::U32(previous_viewport.height),
            ),
            (
                SmolStr::new("previous_camera_position"),
                KernelValue::Vec3(previous_camera.position),
            ),
            (
                SmolStr::new("previous_forward"),
                KernelValue::Vec3(previous_camera.forward),
            ),
            (
                SmolStr::new("previous_up"),
                KernelValue::Vec3(previous_camera.up),
            ),
            (
                SmolStr::new("previous_vertical_fov_degrees"),
                KernelValue::F32(previous_camera.vertical_fov_degrees),
            ),
            (
                SmolStr::new("previous_jitter"),
                KernelValue::Vec2(previous_jitter),
            ),
            (
                SmolStr::new("history_available"),
                KernelValue::U32(u32::from(history_available)),
            ),
            (
                SmolStr::new("history_rejected"),
                KernelValue::U32(u32::from(history_rejected)),
            ),
            (
                SmolStr::new("has_history_primary_hit"),
                KernelValue::U32(u32::from(has_history_primary_hit)),
            ),
        ],
    })
}

fn temporal_resolve_gpu_config_abi() -> PortableAbiType {
    PortableAbiType::Struct {
        name: SmolStr::new("TemporalResolveGpuConfig"),
        class_id: 0,
        fields: vec![
            PortableStructField {
                name: SmolStr::new("item_count"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("width"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("height"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("history_weight_numerator"),
                ty: PortableAbiType::U32,
            },
            PortableStructField {
                name: SmolStr::new("history_weight_denominator"),
                ty: PortableAbiType::U32,
            },
        ],
    }
}

fn temporal_resolve_gpu_config_value(
    width: u32,
    height: u32,
    contract: &TemporalResolvePassContract,
) -> KernelValue {
    KernelValue::Struct(KernelStructValue {
        name: SmolStr::new("TemporalResolveGpuConfig"),
        fields: vec![
            (
                SmolStr::new("item_count"),
                KernelValue::U32(width.saturating_mul(height)),
            ),
            (SmolStr::new("width"), KernelValue::U32(width)),
            (SmolStr::new("height"), KernelValue::U32(height)),
            (
                SmolStr::new("history_weight_numerator"),
                KernelValue::U32(contract.history_weight_numerator),
            ),
            (
                SmolStr::new("history_weight_denominator"),
                KernelValue::U32(contract.history_weight_denominator),
            ),
        ],
    })
}

fn shade_primary_gpu_shader_source(
    workgroup_size: u32,
    shader_f16_enabled: bool,
) -> Result<String, PresentationExecError> {
    let structs = emit_wgsl_structs(&[
        shade_primary_gpu_config_abi(),
        portable_builtin_record_abi("Hit3").expect("Hit3 abi"),
        portable_builtin_record_abi("Surface").expect("Surface abi"),
        portable_builtin_record_abi("Medium").expect("Medium abi"),
        lighting_inputs_abi(),
    ])?;
    let f16_preamble = wgsl_shader_f16_preamble(shader_f16_enabled);
    let vec3_narrow = wgsl_vec3_narrow_helper(shader_f16_enabled);
    Ok(format!(
        "{f16_preamble}
{structs}

override WG_SIZE: u32 = {workgroup_size}u;

@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(0)
var<storage, read> config: Abi_ShadePrimaryGpuConfig;

struct HitBuffer {{
  values: array<Abi_Hit3>,
}}
struct SurfaceBuffer {{
  values: array<Abi_Surface>,
}}
struct RadianceBuffer {{
  values: array<vec3<f32>>,
}}
struct MediumBuffer {{
  values: array<Abi_Medium>,
}}
struct OutputBuffer {{
  values: array<vec3<f32>>,
}}

@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(1)
var<storage, read> primary_hits: HitBuffer;
@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(2)
var<storage, read> surfaces: SurfaceBuffer;
@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(3)
var<storage, read> radiance_values: RadianceBuffer;
@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(4)
var<storage, read> medium_values: MediumBuffer;
@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(5)
var<storage, read_write> output_values: OutputBuffer;

{vec3_narrow}

fn wr_normalize_or(value: vec3<f32>, fallback: vec3<f32>) -> vec3<f32> {{
  let len_sq = dot(value, value);
  if (len_sq <= 0.0000001) {{
    return fallback;
  }}
  return normalize(value);
}}

fn scaled_index(index: u32, output_width: u32, output_height: u32, source_width: u32, source_height: u32) -> u32 {{
  let x = index % max(output_width, 1u);
  let y = index / max(output_width, 1u);
  let source_x = (x * max(source_width, 1u)) / max(output_width, 1u);
  let source_y = (y * max(source_height, 1u)) / max(output_height, 1u);
  return min(
    source_y * max(source_width, 1u) + source_x,
    max(source_width * source_height, 1u) - 1u
  );
}}

fn shade_ray_direction(index: u32) -> vec3<f32> {{
  let width = max(config.viewport_width, 1u);
  let height = max(config.viewport_height, 1u);
  let x = index % width;
  let y = index / width;
  let uv = vec2<f32>(
    (f32(x) + 0.5 + config.jitter.x) / f32(width),
    (f32(y) + 0.5 + config.jitter.y) / f32(height)
  );
  let forward = wr_normalize_or(config.forward, vec3<f32>(0.0, 0.0, -1.0));
  if (config.legacy_active != 0u) {{
    let right = wr_normalize_or(cross(forward, config.legacy_world_up), vec3<f32>(1.0, 0.0, 0.0));
    let up = wr_normalize_or(cross(right, forward), vec3<f32>(0.0, 1.0, 0.0));
    let aspect = f32(width) / f32(height);
    let screen_x = (uv.x * 2.0 - 1.0) * aspect * config.legacy_view_scale;
    let screen_y = (1.0 - uv.y * 2.0) * config.legacy_view_scale;
    return wr_normalize_or(forward + (right * screen_x) + (up * screen_y), forward);
  }}
  let right = wr_normalize_or(cross(forward, config.up), vec3<f32>(1.0, 0.0, 0.0));
  let up = wr_normalize_or(cross(right, forward), vec3<f32>(0.0, 1.0, 0.0));
  let aspect = f32(width) / f32(height);
  let vertical_scale = tan(radians(config.vertical_fov_degrees) * 0.5);
  let screen_x = (uv.x * 2.0 - 1.0) * aspect * vertical_scale;
  let screen_y = (1.0 - uv.y * 2.0) * vertical_scale;
  return wr_normalize_or(forward + (right * screen_x) + (up * screen_y), forward);
}}

fn clamp_vec3(value: vec3<f32>, min_value: f32, max_value: f32) -> vec3<f32> {{
  return vec3<f32>(
    clamp(value.x, min_value, max_value),
    clamp(value.y, min_value, max_value),
    clamp(value.z, min_value, max_value)
  );
}}

@compute @workgroup_size(WG_SIZE)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {{
  let index = global_id.x;
  if (index >= config.item_count) {{
    return;
  }}
  let hit = primary_hits.values[index];
  let surface = surfaces.values[scaled_index(
    index,
    config.viewport_width,
    config.viewport_height,
    config.surface_width,
    config.surface_height
  )];
  var radiance = vec3<f32>(0.0, 0.0, 0.0);
  if (config.radiance_active != 0u) {{
    radiance = radiance_values.values[scaled_index(
      index,
      config.viewport_width,
      config.viewport_height,
      config.radiance_width,
      config.radiance_height
    )];
  }}
  var medium = medium_values.values[0];
  if (config.medium_active != 0u) {{
    medium = medium_values.values[scaled_index(
      index,
      config.viewport_width,
      config.viewport_height,
      config.medium_width,
      config.medium_height
    )];
  }}
  _ = shade_ray_direction(index);
  if (hit.hit != 0u) {{
    let key_delta = config.lighting.key_light.position - hit.position;
    let key_dir = normalize(key_delta);
    let view_dir = normalize(config.camera_position - hit.position);
    let half_dir = normalize(key_dir + view_dir);
    let distance_to_light = length(key_delta);
    let attenuation = clamp(1.0 - (distance_to_light / max(config.lighting.key_light.range, 0.00001)), 0.0, 1.0);
    let ndotl = max(dot(hit.normal, key_dir), 0.0);
    let ndoth = max(dot(hit.normal, half_dir), 0.0);
    let diffuse = ndotl * attenuation;
    let fill = max(dot(hit.normal, normalize(config.lighting.fill_direction)), 0.0) * config.lighting.fill_strength;
    let roughness = clamp(surface.roughness, 0.0, 1.0);
    let spec_power = mix(48.0, 8.0, roughness);
    let metalness = clamp(surface.metalness, 0.0, 1.0);
    let clearcoat = clamp(surface.clearcoat, 0.0, 1.0);
    let highlight = pow(ndoth, spec_power) * (0.10 + (metalness * 0.25) + (clearcoat * 0.20));
    let lighting_rgb = config.lighting.ambient_color + vec3<f32>(diffuse + fill);
    let direct = clamp_vec3(
      (surface.albedo * lighting_rgb * config.lighting.key_light.intensity)
        + vec3<f32>(highlight * 220.0, highlight * 208.0, highlight * 196.0),
      0.0,
      255.0
    );
    let fog_strength = clamp(medium.density * distance_to_light * 0.18, 0.0, 0.55);
    let fog_color = medium.emission + (radiance * 0.22);
    let radiance_lit = radiance * (0.25 + (highlight * 0.15));
    let lit = direct + surface.emissive + radiance_lit;
    output_values.values[index] = narrow_vec3(mix(lit, fog_color, vec3<f32>(fog_strength)));
  }} else {{
    let miss_fog = clamp(medium.density * 3.0, 0.0, 0.45);
    let miss_mix_color = medium.emission + (radiance * 0.28);
    output_values.values[index] = narrow_vec3(mix(radiance, miss_mix_color, vec3<f32>(miss_fog)));
  }}
}}
"
    ))
}

fn motion_resolve_gpu_shader_source(workgroup_size: u32) -> Result<String, PresentationExecError> {
    let structs = emit_wgsl_structs(&[
        motion_resolve_gpu_config_abi(),
        portable_builtin_record_abi("Hit3").expect("Hit3 abi"),
        portable_builtin_record_abi("MotionVector").expect("MotionVector abi"),
    ])?;
    Ok(format!(
        "{structs}

override WG_SIZE: u32 = {workgroup_size}u;

@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(0)
var<storage, read> config: Abi_MotionResolveGpuConfig;

struct HitBuffer {{
  values: array<Abi_Hit3>,
}}
struct MotionBuffer {{
  values: array<Abi_MotionVector>,
}}
struct StatsBuffer {{
  counts: array<atomic<u32>, 3>,
}}

@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(1)
var<storage, read> current_hits: HitBuffer;
@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(2)
var<storage, read> previous_hits: HitBuffer;
@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(3)
var<storage, read_write> output_motion: MotionBuffer;
@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(4)
var<storage, read_write> stats: StatsBuffer;

fn wr_normalize_or(value: vec3<f32>, fallback: vec3<f32>) -> vec3<f32> {{
  let len_sq = dot(value, value);
  if (len_sq <= 0.0000001) {{
    return fallback;
  }}
  return normalize(value);
}}

fn project_to_previous_sample(point: vec3<f32>) -> vec2<f32> {{
  let forward = wr_normalize_or(config.previous_forward, vec3<f32>(0.0, 0.0, -1.0));
  let right = wr_normalize_or(cross(forward, config.previous_up), vec3<f32>(1.0, 0.0, 0.0));
  let up = wr_normalize_or(cross(right, forward), vec3<f32>(0.0, 1.0, 0.0));
  let rel = point - config.previous_camera_position;
  let depth = dot(rel, forward);
  if (depth <= 0.0001) {{
    return vec2<f32>(-1.0, -1.0);
  }}
  let width = max(config.previous_viewport_width, 1u);
  let height = max(config.previous_viewport_height, 1u);
  let aspect = f32(width) / f32(height);
  let vertical_scale = max(tan(radians(config.previous_vertical_fov_degrees) * 0.5), 0.0001);
  let screen_x = dot(rel, right) / (depth * aspect * vertical_scale);
  let screen_y = dot(rel, up) / (depth * vertical_scale);
  let uv = vec2<f32>((screen_x + 1.0) * 0.5, (1.0 - screen_y) * 0.5);
  return vec2<f32>(
    (uv.x * f32(width)) - 0.5 - config.previous_jitter.x,
    (uv.y * f32(height)) - 0.5 - config.previous_jitter.y
  );
}}

fn sample_in_view(sample: vec2<f32>) -> bool {{
  return sample.x >= 0.0
    && sample.y >= 0.0
    && sample.x < f32(config.previous_viewport_width)
    && sample.y < f32(config.previous_viewport_height);
}}

fn previous_index(sample: vec2<f32>) -> u32 {{
  let x = u32(clamp(round(sample.x), 0.0, f32(max(config.previous_viewport_width, 1u) - 1u)));
  let y = u32(clamp(round(sample.y), 0.0, f32(max(config.previous_viewport_height, 1u) - 1u)));
  return y * max(config.previous_viewport_width, 1u) + x;
}}

fn same_identity(current: Abi_Hit3, previous: Abi_Hit3) -> bool {{
  return current.hit != 0u
    && previous.hit != 0u
    && current.root_shape_id == previous.root_shape_id
    && current.feature_id == previous.feature_id
    && current.instance_id == previous.instance_id
    && current.repeat_id == previous.repeat_id;
}}

@compute @workgroup_size(WG_SIZE)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {{
  let index = global_id.x;
  if (index >= config.item_count) {{
    return;
  }}
  let hit = current_hits.values[index];
  let current_pixel = vec2<f32>(
    f32(index % max(config.viewport_width, 1u)),
    f32(index / max(config.viewport_width, 1u))
  );
  var motion = Abi_MotionVector(
    vec2<f32>(0.0, 0.0),
    vec2<f32>(0.0, 0.0),
    0u,
    0u
  );
  if (hit.hit != 0u) {{
    let previous_sample = project_to_previous_sample(hit.position);
    if (config.history_available != 0u && config.has_history_primary_hit != 0u) {{
      if (sample_in_view(previous_sample)) {{
        let previous_hit = previous_hits.values[previous_index(previous_sample)];
        if (same_identity(hit, previous_hit)) {{
          motion = Abi_MotionVector(previous_sample - current_pixel, previous_sample, 1u, 0u);
          atomicAdd(&stats.counts[0], 1u);
        }} else {{
          motion = Abi_MotionVector(previous_sample - current_pixel, previous_sample, 0u, 1u);
          atomicAdd(&stats.counts[1], 1u);
        }}
      }} else {{
        motion = Abi_MotionVector(previous_sample - current_pixel, previous_sample, 0u, 1u);
        atomicAdd(&stats.counts[1], 1u);
      }}
    }} else {{
      motion = Abi_MotionVector(
        vec2<f32>(0.0, 0.0),
        select(vec2<f32>(0.0, 0.0), previous_sample, all(previous_sample >= vec2<f32>(0.0, 0.0))),
        0u,
        0u
      );
      if (config.history_rejected != 0u) {{
        atomicAdd(&stats.counts[1], 1u);
      }} else {{
        atomicAdd(&stats.counts[2], 1u);
      }}
    }}
  }} else {{
    atomicAdd(&stats.counts[2], 1u);
  }}
  output_motion.values[index] = motion;
}}
"
    ))
}

fn temporal_resolve_gpu_shader_source(
    workgroup_size: u32,
    shader_f16_enabled: bool,
) -> Result<String, PresentationExecError> {
    let structs = emit_wgsl_structs(&[
        temporal_resolve_gpu_config_abi(),
        portable_builtin_record_abi("MotionVector").expect("MotionVector abi"),
    ])?;
    let f16_preamble = wgsl_shader_f16_preamble(shader_f16_enabled);
    let vec3_narrow = wgsl_vec3_narrow_helper(shader_f16_enabled);
    Ok(format!(
        "{f16_preamble}
{structs}

override WG_SIZE: u32 = {workgroup_size}u;

@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(0)
var<storage, read> config: Abi_TemporalResolveGpuConfig;

struct ColorBuffer {{
  values: array<vec3<f32>>,
}}
struct MotionBuffer {{
  values: array<Abi_MotionVector>,
}}
struct StatsBuffer {{
  consumed: atomic<u32>,
}}

@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(1)
var<storage, read> current_color: ColorBuffer;
@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(2)
var<storage, read> history_color: ColorBuffer;
@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(3)
var<storage, read> motion_values: MotionBuffer;
@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(4)
var<storage, read_write> output_color: ColorBuffer;
@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(5)
var<storage, read_write> stats: StatsBuffer;

{vec3_narrow}

fn previous_index(sample: vec2<f32>) -> u32 {{
  let x = u32(clamp(round(sample.x), 0.0, f32(max(config.width, 1u) - 1u)));
  let y = u32(clamp(round(sample.y), 0.0, f32(max(config.height, 1u) - 1u)));
  return y * max(config.width, 1u) + x;
}}

fn neighborhood_bounds(index: u32) -> array<vec3<f32>, 2> {{
  let width = max(config.width, 1u);
  let height = max(config.height, 1u);
  let x = index % width;
  let y = index / width;
  var clamp_min = vec3<f32>(999999.0, 999999.0, 999999.0);
  var clamp_max = vec3<f32>(-999999.0, -999999.0, -999999.0);
  for (var dy: i32 = -1; dy <= 1; dy = dy + 1) {{
    for (var dx: i32 = -1; dx <= 1; dx = dx + 1) {{
      let sample_x = u32(clamp(i32(x) + dx, 0, i32(width) - 1));
      let sample_y = u32(clamp(i32(y) + dy, 0, i32(height) - 1));
      let sample = current_color.values[sample_y * width + sample_x];
      clamp_min = min(clamp_min, sample);
      clamp_max = max(clamp_max, sample);
    }}
  }}
  return array<vec3<f32>, 2>(clamp_min, clamp_max);
}}

@compute @workgroup_size(WG_SIZE)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {{
  let index = global_id.x;
  if (index >= config.item_count) {{
    return;
  }}
  let current = current_color.values[index];
  let motion = motion_values.values[index];
  let bounds = neighborhood_bounds(index);
  let clamp_min = bounds[0];
  let clamp_max = bounds[1];
  var history = vec3<f32>(0.0, 0.0, 0.0);
  let use_history = motion.valid != 0u && motion.disoccluded == 0u;
  if (motion.valid != 0u) {{
    history = history_color.values[previous_index(motion.previous_sample)];
  }}
  if (use_history) {{
    atomicAdd(&stats.consumed, 1u);
  }}
  let clamped_history = vec3<f32>(
    clamp(history.x, clamp_min.x, clamp_max.x),
    clamp(history.y, clamp_min.y, clamp_max.y),
    clamp(history.z, clamp_min.z, clamp_max.z)
  );
  let history_weight = f32(config.history_weight_numerator) / f32(max(config.history_weight_denominator, 1u));
  let resolved = select(
    current,
    (current * (1.0 - history_weight)) + (clamped_history * history_weight),
    use_history
  );
  output_color.values[index] = narrow_vec3(resolved);
}}
"
    ))
}

#[cfg(test)]
fn shade_primary_shader_source(
    workgroup_size: u32,
    shader_f16_enabled: bool,
) -> Result<String, PresentationExecError> {
    let structs = emit_wgsl_structs(&[
        crate::query_exec::wgsl::codegen::wgsl_dispatch_config_abi(),
        shade_primary_input_abi(),
    ])?;
    let f16_preamble = wgsl_shader_f16_preamble(shader_f16_enabled);
    let vec3_narrow = wgsl_vec3_narrow_helper(shader_f16_enabled);
    Ok(format!(
        "{f16_preamble}
{structs}

override WG_SIZE: u32 = {workgroup_size}u;

@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(0)
var<storage, read> dispatch_config: Abi_WgslDispatchConfig;

struct InputBuffer {{
  values: array<Abi_ShadePrimaryInput>,
}}

struct OutputBuffer {{
  values: array<vec3<f32>>,
}}

struct DummyBuffer {{
  values: array<u32>,
}}

@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(1)
var<storage, read> input_items: InputBuffer;
@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(2)
var<storage, read_write> output_items: OutputBuffer;
@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(3)
var<storage, read> dummy_items: DummyBuffer;

{vec3_narrow}

fn clamp_vec3(value: vec3<f32>, min_value: f32, max_value: f32) -> vec3<f32> {{
  return vec3<f32>(
    clamp(value.x, min_value, max_value),
    clamp(value.y, min_value, max_value),
    clamp(value.z, min_value, max_value)
  );
}}

@compute @workgroup_size(WG_SIZE)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {{
  let index = global_id.x;
  _ = dummy_items.values[0];
  if (index >= dispatch_config.item_count) {{
    return;
  }}
  let input = input_items.values[index];
  if (input.hit.hit != 0u) {{
    let key_delta = input.lighting.key_light.position - input.hit.position;
    let key_dir = normalize(key_delta);
    let view_dir = normalize(input.camera_position - input.hit.position);
    let half_dir = normalize(key_dir + view_dir);
    let distance_to_light = length(key_delta);
    let attenuation = clamp(1.0 - (distance_to_light / max(input.lighting.key_light.range, 0.00001)), 0.0, 1.0);
    let ndotl = max(dot(input.hit.normal, key_dir), 0.0);
    let ndoth = max(dot(input.hit.normal, half_dir), 0.0);
    let diffuse = ndotl * attenuation;
    let fill = max(dot(input.hit.normal, normalize(input.lighting.fill_direction)), 0.0) * input.lighting.fill_strength;
    let roughness = clamp(input.surface.roughness, 0.0, 1.0);
    let spec_power = mix(48.0, 8.0, roughness);
    let metalness = clamp(input.surface.metalness, 0.0, 1.0);
    let clearcoat = clamp(input.surface.clearcoat, 0.0, 1.0);
    let highlight = pow(ndoth, spec_power) * (0.10 + (metalness * 0.25) + (clearcoat * 0.20));
    let lighting_rgb = input.lighting.ambient_color + vec3<f32>(diffuse + fill);
    let direct = clamp_vec3(
      (input.surface.albedo * lighting_rgb * input.lighting.key_light.intensity)
        + vec3<f32>(highlight * 220.0, highlight * 208.0, highlight * 196.0),
      0.0,
      255.0,
    );
    let fog_strength = clamp(input.medium.density * distance_to_light * 0.18, 0.0, 0.55);
    let fog_color = input.medium.emission + (input.radiance * 0.22);
    let radiance_lit = input.radiance * (0.25 + (highlight * 0.15));
    let lit = direct + input.surface.emissive + radiance_lit;
    output_items.values[index] = narrow_vec3(mix(lit, fog_color, vec3<f32>(fog_strength)));
  }} else {{
    let miss_fog = clamp(input.medium.density * 3.0, 0.0, 0.45);
    let miss_mix_color = input.medium.emission + (input.radiance * 0.28);
    output_items.values[index] =
      narrow_vec3(mix(input.radiance, miss_mix_color, vec3<f32>(miss_fog)));
  }}
}}
"
    ))
}

fn copy_vec3_shader_source(
    workgroup_size: u32,
    shader_f16_enabled: bool,
) -> Result<String, PresentationExecError> {
    let structs =
        emit_wgsl_structs(&[crate::query_exec::wgsl::codegen::wgsl_dispatch_config_abi()])?;
    let f16_preamble = wgsl_shader_f16_preamble(shader_f16_enabled);
    let vec3_narrow = wgsl_vec3_narrow_helper(shader_f16_enabled);
    Ok(format!(
        "{f16_preamble}
{structs}

override WG_SIZE: u32 = {workgroup_size}u;

@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(0)
var<storage, read> dispatch_config: Abi_WgslDispatchConfig;

struct InputBuffer {{
  values: array<vec3<f32>>,
}}

struct OutputBuffer {{
  values: array<vec3<f32>>,
}}

struct DummyBuffer {{
  values: array<u32>,
}}

@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(1)
var<storage, read> input_items: InputBuffer;
@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(2)
var<storage, read_write> output_items: OutputBuffer;
@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(3)
var<storage, read> dummy_items: DummyBuffer;

{vec3_narrow}

@compute @workgroup_size(WG_SIZE)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {{
  let index = global_id.x;
  _ = dummy_items.values[0];
  if (index >= dispatch_config.item_count) {{
    return;
  }}
  output_items.values[index] = narrow_vec3(input_items.values[index]);
}}
"
    ))
}

#[cfg(test)]
fn temporal_resolve_shader_source(
    contract: &TemporalResolvePassContract,
    workgroup_size: u32,
    shader_f16_enabled: bool,
) -> Result<String, PresentationExecError> {
    let structs = emit_wgsl_structs(&[
        crate::query_exec::wgsl::codegen::wgsl_dispatch_config_abi(),
        temporal_resolve_input_abi(),
    ])?;
    let history_weight = contract.history_weight_numerator as f32
        / contract.history_weight_denominator.max(1) as f32;
    let f16_preamble = wgsl_shader_f16_preamble(shader_f16_enabled);
    let vec3_narrow = wgsl_vec3_narrow_helper(shader_f16_enabled);
    Ok(format!(
        "{f16_preamble}
{structs}

override WG_SIZE: u32 = {workgroup_size}u;

@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(0)
var<storage, read> dispatch_config: Abi_WgslDispatchConfig;

struct InputBuffer {{
  values: array<Abi_TemporalResolveInput>,
}}

struct OutputBuffer {{
  values: array<vec3<f32>>,
}}

struct DummyBuffer {{
  values: array<u32>,
}}

@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(1)
var<storage, read> input_items: InputBuffer;
@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(2)
var<storage, read_write> output_items: OutputBuffer;
@group({GPU_RUNTIME_PASS_BIND_GROUP_INDEX}) @binding(3)
var<storage, read> dummy_items: DummyBuffer;

{vec3_narrow}

fn clamp_vec3(value: vec3<f32>, min_value: vec3<f32>, max_value: vec3<f32>) -> vec3<f32> {{
  return vec3<f32>(
    clamp(value.x, min_value.x, max_value.x),
    clamp(value.y, min_value.y, max_value.y),
    clamp(value.z, min_value.z, max_value.z)
  );
}}

@compute @workgroup_size(WG_SIZE)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {{
  let index = global_id.x;
  _ = dummy_items.values[0];
  if (index >= dispatch_config.item_count) {{
    return;
  }}
  let input = input_items.values[index];
  if (input.use_history != 0u) {{
    let clamped_history = clamp_vec3(input.history_color, input.clamp_min, input.clamp_max);
    output_items.values[index] =
      narrow_vec3(mix(input.current_color, clamped_history, vec3<f32>({history_weight})));
  }} else {{
    output_items.values[index] = narrow_vec3(input.current_color);
  }}
}}
"
    ))
}

fn wgsl_shader_f16_preamble(shader_f16_enabled: bool) -> &'static str {
    if shader_f16_enabled {
        "enable f16;"
    } else {
        ""
    }
}

fn wgsl_vec3_narrow_helper(shader_f16_enabled: bool) -> &'static str {
    if shader_f16_enabled {
        r#"
fn narrow_vec3(value: vec3<f32>) -> vec3<f32> {
  let narrowed = vec3<f16>(value);
  return vec3<f32>(narrowed);
}
"#
    } else {
        r#"
fn narrow_vec3(value: vec3<f32>) -> vec3<f32> {
  return value;
}
"#
    }
}

fn emit_wgsl_structs(roots: &[PortableAbiType]) -> Result<String, PresentationExecError> {
    let prefixed = roots
        .iter()
        .cloned()
        .map(prefix_abi_name)
        .collect::<Vec<_>>();
    portable_abi_emit_wgsl_structs(&prefixed).map_err(|err| {
        PresentationExecError::UnsupportedPlan {
            message: err.to_string(),
        }
    })
}

fn prefix_abi_name(abi: PortableAbiType) -> PortableAbiType {
    match abi {
        PortableAbiType::Struct {
            name,
            class_id,
            fields,
        } => PortableAbiType::Struct {
            name: SmolStr::new(format!("Abi_{name}")),
            class_id,
            fields: fields
                .into_iter()
                .map(|field| PortableStructField {
                    name: field.name,
                    ty: prefix_abi_name(field.ty),
                })
                .collect(),
        },
        PortableAbiType::Array(inner, len) => {
            PortableAbiType::Array(Box::new(prefix_abi_name(*inner)), len)
        }
        other => other,
    }
}

fn hit_flag(value: &KernelValue) -> Result<bool, PresentationExecError> {
    match field(expect_struct(value, "Hit3")?, "hit")? {
        KernelValue::Bool(value) => Ok(*value),
        other => Err(PresentationExecError::TypeMismatch {
            expected: "Boolean".to_string(),
            found: format!("{other:?}"),
        }),
    }
}

fn hit_distance(value: &KernelValue) -> Result<f32, PresentationExecError> {
    expect_f32(field(expect_struct(value, "Hit3")?, "distance")?)
}

#[cfg(test)]
mod tests {
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
            bytes: dispatch.bytes,
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
            let padding_start = row_start
                + layout.physical.width as usize * layout.physical.element_stride as usize;
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
            bytes: dispatch.bytes,
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
}
