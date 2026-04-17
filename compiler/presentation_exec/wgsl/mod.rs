//! Owns the WGSL-backed execution path for presentation plans.
//! Does not own high-level presentation plan selection, artifact policy, or CPU
//! fallback orchestration.
//!
//! Key invariants:
//! - GPU resource state must stay aligned with the framegraph submission that
//!   produced it.
//! - readback, staging, and pass-profiler metadata must describe the executed
//!   WGSL path, especially when a pass falls back or is skipped.
//! - shader ABI layout must stay consistent with the portable/kernel structs the
//!   CPU path uses as the semantic oracle.
//!
//! Primary entrypoints:
//! - `execute_plan`
//! - `passes::*`
//! - `runtime::*`
//!
//! Failure modes / common pitfalls:
//! - stale staging buffers or pass bindings can look like rendering bugs while
//!   actually being lifetime/accounting bugs.
//! - changing shader-visible layouts without updating the portable ABI helpers
//!   breaks backend equivalence in ways the type system cannot catch.

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

mod abi;
mod passes;
mod pipelines;
mod runtime;
mod shaders;

#[cfg(test)]
mod tests;

use self::{abi::*, pipelines::*, runtime::*, shaders::*};

use super::gpu_primary::{PrimaryVisibilityGpuDispatch, prepare_primary_visibility_dispatch};

#[path = "../gpu_post.rs"]
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
