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
    PresentationExecutionResult, PresentationMetrics, PresentationRayStepDistribution,
    TileCullingStats, adjusted_ray_budget, allocate_execution_attachments,
    attachment_hit_work_items, build_frame_cost_report, build_temporal_history,
    build_tile_candidate_span_words, effective_plan_for_quality, encode_values_at_indices,
    execute_batch_contract, expect_array, expect_f32, expect_struct, field, frame_state_components,
    full_attachment_byte_size, internal_resolution_viewport, lighting_inputs_value,
    participant_query_work_items, participant_query_work_items_without_screen_samples,
    presentation_metrics, primary_hit_miss_value, resolved_quality_state,
    runtime_primary_solver_summary, select_presentation_workgroup_size,
    tile_candidate_dispatch_packets, tile_candidate_packet_fragment_count,
    tile_candidate_packet_sample_count, tile_candidate_stats, tile_culling_mask,
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

use super::gpu_primary::{PreparedPrimaryVisibilityQuery, PrimaryVisibilityGpuDispatch};

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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct PrimaryVisibilitySetupCacheKey {
    context_id: u64,
    contract_id: crate::query_contract::QueryContractId,
    capture_name: SmolStr,
    snapshot_id: u64,
    snapshot_epoch: u64,
    frame_domain_fingerprint: u64,
    camera_fingerprint: u64,
    viewport_fingerprint: u64,
    primary_viewport_fingerprint: u64,
    ray_budget_fingerprint: u64,
    legacy_projection: bool,
    compatibility_projection_fingerprint: u64,
}

#[derive(Clone)]
struct CachedPrimaryVisibilitySetup {
    cull_mask: Option<crate::presentation_exec::TileCullingMask>,
    prepared_query: PreparedPrimaryVisibilityQuery,
}

fn primary_visibility_setup_cache()
-> &'static Mutex<HashMap<PrimaryVisibilitySetupCacheKey, CachedPrimaryVisibilitySetup>> {
    static CACHE: OnceLock<
        Mutex<HashMap<PrimaryVisibilitySetupCacheKey, CachedPrimaryVisibilitySetup>>,
    > = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn debug_fingerprint(value: &impl std::fmt::Debug) -> u64 {
    let encoded = format!("{value:?}");
    crate::query_exec::ids::stable_semantic_id(&[encoded.as_bytes()])
}

fn cached_primary_visibility_setup(
    ctx: &QueryExecContext,
    input: &PresentationExecutionInput,
    contract: &crate::presentation_plan::PrimaryVisibilityPassContract,
    camera: crate::presentation_contract::CanonicalCameraInput,
    viewport: crate::presentation_contract::CanonicalViewportInput,
    primary_viewport: crate::presentation_contract::CanonicalViewportInput,
    ray_budget: crate::presentation_contract::CanonicalRayBudget,
    legacy_projection: bool,
) -> Result<CachedPrimaryVisibilitySetup, PresentationExecError> {
    let key = PrimaryVisibilitySetupCacheKey {
        context_id: ctx.wgsl_shader_cache_context_id,
        contract_id: contract.query_contract,
        capture_name: input.region_snapshot.capture_name().clone(),
        snapshot_id: input.region_snapshot.snapshot_id().0,
        snapshot_epoch: input.region_snapshot.epoch().0,
        frame_domain_fingerprint: debug_fingerprint(&input.frame_domain),
        camera_fingerprint: debug_fingerprint(&camera),
        viewport_fingerprint: debug_fingerprint(&viewport),
        primary_viewport_fingerprint: debug_fingerprint(&primary_viewport),
        ray_budget_fingerprint: debug_fingerprint(&ray_budget),
        legacy_projection,
        compatibility_projection_fingerprint: debug_fingerprint(&input.compatibility_projection),
    };
    if let Some(cached) = primary_visibility_setup_cache()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .get(&key)
        .cloned()
    {
        return Ok(cached);
    }
    let cull_mask = tile_culling_mask(ctx, input, camera, primary_viewport, legacy_projection)?;
    let candidate_shape_names = cull_mask.as_ref().and_then(|mask| {
        mask.candidate_table
            .enabled
            .then(|| mask.candidate_table.candidate_shapes.clone())
    });
    let candidate_spans = cull_mask
        .as_ref()
        .map(|mask| build_tile_candidate_span_words(&mask.candidate_table, &mask.active_samples, 1))
        .unwrap_or_default();
    let prepared_query = super::gpu_primary::prepare_primary_visibility_query(
        ctx,
        contract.query_contract,
        input.region_capture_value(),
        input.frame_domain.clone(),
        candidate_shape_names,
        candidate_spans,
        primary_viewport,
    )?;
    let cached = CachedPrimaryVisibilitySetup {
        cull_mask,
        prepared_query,
    };
    primary_visibility_setup_cache()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .insert(key, cached.clone());
    Ok(cached)
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
    let resident_history_compatible = input.runtime_summary_only
        && !input.materialize_cpu_attachments
        && input
            .resident_history_attachments
            .as_ref()
            .zip(input.history.as_ref())
            .is_some_and(|(_, history)| {
                crate::presentation_exec::history_slots_match(
                    &effective_plan.frame,
                    &input.frame_state,
                    viewport.width,
                    viewport.height,
                    current_snapshot,
                    history,
                )
                .unwrap_or(false)
            });
    let attachment_setup_start = Instant::now();
    let mut attachments = if resident_history_compatible {
        crate::presentation_exec::resources::allocate_attachment_layout_resources_without_history(
            &effective_plan.frame,
            viewport.width,
            viewport.height,
        )
        .map_err(PresentationExecError::Resource)?
    } else {
        allocate_execution_attachments(
            &effective_plan.frame,
            &input.frame_state,
            viewport.width,
            viewport.height,
            current_snapshot,
            input.history.as_ref(),
        )?
    };
    let attachment_setup_elapsed_micros = attachment_setup_start.elapsed().as_micros();
    let framegraph_setup_start = Instant::now();
    let native = native_wgpu_context().map_err(PresentationExecError::Query)?;
    let mut framegraph = if resident_history_compatible {
        PresentationFramegraph::from_plan_and_gpu_resources_with_previous(
            effective_plan.clone(),
            attachments.clone(),
            native.clone(),
            (effective_plan.passes.len() as u32)
                .saturating_mul(6)
                .max(8),
            input
                .resident_history_attachments
                .as_ref()
                .expect("resident history compatibility requires GPU history"),
        )
    } else {
        PresentationFramegraph::from_plan_and_gpu_resources(
            effective_plan.clone(),
            attachments.clone(),
            native.clone(),
            (effective_plan.passes.len() as u32)
                .saturating_mul(6)
                .max(8),
        )
    };
    let framegraph_setup_elapsed_micros = framegraph_setup_start.elapsed().as_micros();
    let mut gpu_runtime = framegraph.initial_gpu_runtime();
    let selected_workgroup_size = select_presentation_workgroup_size(&native.adapter_limits)?;
    let mut primary_solver_context = None;
    let mut continuation_counts = crate::presentation_exec::temporal::ContinuationCounts::default();
    let mut tile_cull = TileCullingStats::default();
    let mut tile_candidate = crate::presentation_exec::TileCandidateStats::default();
    let mut view_distance_clipmap = None;
    let mut candidate_table_active = false;
    let packet_scheduling_active = false;
    let mut surface_resolve_count = 0;
    let mut participant_resolve_count = 0;
    let mut pass_stats = Vec::new();
    let mut framegraph_exceptions = Vec::<String>::new();
    let mut pending_gpu_pass_ranges = Vec::<(usize, Range<usize>)>::new();
    let mut setup_stage_stats = vec![
        PassRuntimeStats {
            pass_id: "setup.attachments".to_string(),
            pass_kind: "setup_attachments".to_string(),
            work_items: effective_plan.frame.outputs.len() as u32,
            elapsed_micros: attachment_setup_elapsed_micros,
            notes: vec![format!(
                "resident_history_compatible={resident_history_compatible}"
            )],
            ..PassRuntimeStats::default()
        },
        PassRuntimeStats {
            pass_id: "setup.framegraph".to_string(),
            pass_kind: "setup_framegraph".to_string(),
            work_items: effective_plan.frame.outputs.len() as u32,
            elapsed_micros: framegraph_setup_elapsed_micros,
            notes: vec![format!(
                "max_timestamped_passes={}",
                (effective_plan.passes.len() as u32)
                    .saturating_mul(6)
                    .max(8)
            )],
            ..PassRuntimeStats::default()
        },
    ];
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
                let primary_setup_start = Instant::now();
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
                let cached_primary_setup = cached_primary_visibility_setup(
                    ctx,
                    input,
                    contract,
                    camera,
                    viewport,
                    primary_viewport,
                    ray_budget,
                    effective_plan
                        .view
                        .compatibility_projection
                        .legacy_path_active,
                )?;
                let cull_mask = cached_primary_setup.cull_mask.clone();
                candidate_table_active = cull_mask
                    .as_ref()
                    .is_some_and(|mask| mask.candidate_table.enabled);
                if let Some(mask) = cull_mask.as_ref() {
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
                }
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
                let primary_dispatch = cached_primary_setup.prepared_query.instantiate_dispatch(
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
                let primary_setup_elapsed_micros = primary_setup_start.elapsed().as_micros();
                let mut primary_setup_notes = Vec::new();
                primary_setup_notes
                    .push(format!("candidate_table_active={candidate_table_active}"));
                if let Some(mask) = cull_mask.as_ref() {
                    primary_setup_notes.push(format!(
                        "tile_cull_active_tiles={}/{}",
                        mask.stats.active_tiles, mask.stats.total_tiles
                    ));
                    primary_setup_notes
                        .push(format!("active_samples={}", mask.active_samples.len()));
                }
                setup_stage_stats.push(PassRuntimeStats {
                    pass_id: format!("{}.setup", pass.id),
                    pass_kind: "primary_setup".to_string(),
                    work_items: primary_viewport
                        .width
                        .saturating_mul(primary_viewport.height),
                    elapsed_micros: primary_setup_elapsed_micros,
                    notes: primary_setup_notes,
                    ..PassRuntimeStats::default()
                });
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
        .submit_segment(input.collect_gpu_timing_readback)
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
    if input.runtime_summary_only {
        let query_trace_start = Instant::now();
        let query_trace = build_primary_batch_query_trace(
            primary_contract_id,
            current_snapshot,
            &primary_batch_plan,
            primary_item_count,
            true,
            QueryExecutionObservability::default(),
        )?;
        let query_trace_elapsed_micros = query_trace_start.elapsed().as_micros();
        setup_stage_stats.push(PassRuntimeStats {
            pass_id: "runtime_summary.query_trace".to_string(),
            pass_kind: "runtime_summary_query_trace".to_string(),
            work_items: primary_item_count,
            elapsed_micros: query_trace_elapsed_micros,
            ..PassRuntimeStats::default()
        });
        let metrics = minimal_presentation_metrics(timed_gpu_runtime.clone());
        let history_build_start = Instant::now();
        let history = build_temporal_history(
            &effective_plan,
            &input.frame_state,
            &attachments,
            current_snapshot,
            view_distance_clipmap.as_ref(),
        )?;
        let history_build_elapsed_micros = history_build_start.elapsed().as_micros();
        setup_stage_stats.push(PassRuntimeStats {
            pass_id: "runtime_summary.history".to_string(),
            pass_kind: "runtime_summary_history".to_string(),
            work_items: history.as_ref().map(|entry| entry.slots.len()).unwrap_or(0) as u32,
            elapsed_micros: history_build_elapsed_micros,
            ..PassRuntimeStats::default()
        });
        pass_stats.extend(setup_stage_stats);
        let mut frame_cost = build_frame_cost_report(
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
        frame_cost.observability_sampled = false;
        frame_cost.observability_notes = vec![
            "runtime_summary_only".to_string(),
            "query_observability_unsampled".to_string(),
        ];
        return Ok(PresentationExecutionResult {
            plan_name: plan.name.clone(),
            backend: DispatchBackend::Wgsl,
            width: viewport.width,
            height: viewport.height,
            screen_samples: Vec::new(),
            attachments,
            history,
            resident_history_attachments: Some(framegraph.attachments.clone()),
            metrics,
            frame_cost,
            query_trace,
        });
    }
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
        .submit_segment(input.collect_gpu_timing_readback)
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
        false,
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
    pass_stats.extend(setup_stage_stats);
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
        resident_history_attachments: None,
        metrics,
        frame_cost,
        query_trace: primary_trace,
    })
}

fn minimal_presentation_metrics(gpu_runtime: GpuRuntimeMetrics) -> PresentationMetrics {
    PresentationMetrics {
        sample_count: 0,
        hit_count: 0,
        miss_count: 0,
        candidate_count: 0,
        candidates_before_pruning: 0,
        candidates_after_pruning: 0,
        candidate_reduction: 0,
        trace_steps_total: 0,
        trace_steps_max: 0,
        ray_step_distribution: PresentationRayStepDistribution {
            zero: 0,
            short: 0,
            medium: 0,
            long: 0,
            extreme: 0,
        },
        dispatch_items: 0,
        dispatch_workgroups: [0, 0, 0],
        solver_summary: None,
        solver_methods: Vec::new(),
        dense_fallback_count: 0,
        continuation_available_count: 0,
        continuation_consumed_count: 0,
        continuation_rejected_count: 0,
        continuation_unavailable_count: 0,
        continuation_diagnostics: Vec::new(),
        acceleration_node_visits: 0,
        union_cluster_visits: 0,
        ray_support_interval_rejections: 0,
        ray_support_entry_jumps: 0,
        repeat_cell_skips: 0,
        cache_brick_visits: 0,
        cache_brick_hits: 0,
        cache_brick_misses: 0,
        cache_interval_advances: 0,
        accepted_relaxed_steps: 0,
        rejected_relaxed_steps: 0,
        solver_relaxed_attempts: 0,
        solver_relaxed_no_root_advances: 0,
        solver_relaxed_brackets: 0,
        solver_relaxed_unresolved: 0,
        solver_interval_attempts: 0,
        solver_interval_no_root_advances: 0,
        solver_interval_brackets: 0,
        solver_interval_unresolved: 0,
        solver_refinement_attempts: 0,
        solver_refinement_failures: 0,
        solver_repeat_attempts: 0,
        solver_repeat_supported: 0,
        solver_repeat_inapplicable: 0,
        solver_repeat_unsupported: 0,
        solver_repeat_unsupported_form: 0,
        solver_repeat_unsupported_bounds: 0,
        solver_repeat_cells_enumerated: 0,
        analytic_transformed_hits: 0,
        interval_subdivisions: 0,
        interval_proof_successes: 0,
        observer_continuation_seed_hits: 0,
        field_samples: 0,
        gpu_runtime,
    }
}
