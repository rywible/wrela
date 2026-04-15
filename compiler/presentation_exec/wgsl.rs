use crate::gpu_runtime::{GpuPassProfiler, GpuRuntimeMetrics};
use crate::kernel::{KernelStructValue, KernelValue, lower_batch_query_plan};
use crate::portable::{
    PortableAbiType, PortableStructField, portable_abi_array_stride,
    portable_abi_emit_wgsl_structs, portable_abi_layout, portable_builtin_record_abi,
};
use crate::presentation_contract::RealtimeRadianceMode;
use crate::presentation_exec::clipmap::{
    build_view_distance_clipmap_artifact, clipmap_pass_note, clipmap_pass_runtime,
};
use crate::presentation_exec::resources::{AttachmentResourceSet, FrameAttachmentLayoutPlan};
use crate::presentation_exec::temporal::{
    motion_resolve, temporal_resolve_kernel_values, update_query_trace_continuation,
};
use crate::presentation_exec::{
    PassRuntimeStats, PresentationExecError, PresentationExecutionInput,
    PresentationExecutionResult, TileCullingStats, adjusted_ray_budget,
    allocate_execution_attachments, attachment_hit_work_items, build_frame_cost_report,
    build_temporal_history, effective_plan_for_quality, encode_values_at_indices,
    execute_batch_contract, expand_internal_hits, expect_array, expect_f32, expect_struct,
    expect_vec3, field, frame_state_components, full_attachment_byte_size, generate_screen_samples,
    internal_resolution_viewport, lighting_inputs_value,
    materialize_primary_visibility_attachments, participant_query_work_items, presentation_metrics,
    primary_hit_miss_value, resolved_quality_state, runtime_primary_solver_summary,
    screen_sample_ray, select_presentation_workgroup_size, shade_lookup_value,
    tile_candidate_dispatch_packets, tile_candidate_stats, tile_culling_mask,
};
use crate::presentation_plan::{
    CompositeColorPassContract, ParticipantsResolvePassContract, PresentationPassKind,
    PresentationPlan, ShadePrimaryPassContract, SurfaceResolvePassContract,
    TemporalResolvePassContract,
};
use crate::query_exec::BatchQueryExecutionTrace;
use crate::query_exec::QueryExecContext;
use crate::query_exec::cpu::{default_medium, default_surface};
use crate::query_exec::execute_batch_query_with_solver_mode_with_snapshot_on;
use crate::query_exec::wgsl::{
    compiled_pipeline, encode_slice, encode_value, native_wgpu_context, readback_storage_buffer,
};
use crate::query_plan::{BatchQueryPlan, DispatchBackend};
use smol_str::SmolStr;
use std::time::Instant;
use wgpu::util::DeviceExt;

struct LinearShaderDispatchResult {
    bytes: Vec<u8>,
    dispatch_count: u32,
}

struct TemporalResolveDispatchResult {
    consumed_count: u32,
    dispatch_count: u32,
}

fn storage_buffer_size(bytes: &[u8]) -> u64 {
    bytes.len().max(4) as u64
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
    let screen_samples = generate_screen_samples(
        &effective_plan,
        input,
        camera,
        viewport,
        jitter_pixels,
        ray_budget,
    );
    let primary_viewport = internal_resolution_viewport(viewport, &quality);
    let primary_screen_samples = if primary_viewport == viewport {
        screen_samples.clone()
    } else {
        generate_screen_samples(
            &effective_plan,
            input,
            camera,
            primary_viewport,
            jitter_pixels,
            ray_budget,
        )
    };
    let mut attachments = allocate_execution_attachments(
        &effective_plan.frame,
        &input.frame_state,
        viewport.width,
        viewport.height,
        current_snapshot,
        input.history.as_ref(),
    )?;
    let mut primary_hits = None;
    let mut primary_trace = None;
    let mut primary_solver_context = None;
    let mut continuation_counts = crate::presentation_exec::temporal::ContinuationCounts::default();
    let mut tile_cull = TileCullingStats::default();
    let mut tile_candidate = crate::presentation_exec::TileCandidateStats::default();
    let mut view_distance_clipmap = None;
    let mut candidate_table_active = false;
    let mut packet_scheduling_active = false;
    let mut gpu_runtime = GpuRuntimeMetrics {
        cpu_screen_sample_allocations: screen_samples.len() as u32,
        ..GpuRuntimeMetrics::default()
    };
    if primary_viewport != viewport {
        gpu_runtime.cpu_screen_sample_allocations = gpu_runtime
            .cpu_screen_sample_allocations
            .saturating_add(primary_screen_samples.len() as u32);
    }
    let selected_workgroup_size = {
        let native = native_wgpu_context()?;
        select_presentation_workgroup_size(&native.adapter_limits)?
    };
    let mut surface_resolve_count = 0;
    let mut participant_resolve_count = 0;
    let mut pass_stats = Vec::new();

    for pass in &effective_plan.passes {
        match &pass.kind {
            PresentationPassKind::GenerateScreenSamples { .. } => {}
            PresentationPassKind::PrimaryVisibility { contract } => {
                let pass_start = Instant::now();
                let mut runtime = PassRuntimeStats {
                    pass_id: pass.id.to_string(),
                    pass_kind: "primary_visibility".to_string(),
                    work_items: primary_screen_samples.len() as u32,
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
                    ..PassRuntimeStats::default()
                };
                let rays = primary_screen_samples
                    .iter()
                    .map(screen_sample_ray)
                    .collect::<Result<Vec<_>, _>>()?;
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
                let shape_batch_plan = lower_batch_query_plan(
                    &BatchQueryPlan::for_contract(
                        crate::query_contract::SPATIAL_NEAREST_BATCH_SHAPE,
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
                candidate_table_active = cull_mask
                    .as_ref()
                    .is_some_and(|mask| mask.candidate_table.enabled);
                let primary_gpu_time_before = gpu_runtime.gpu_time_total_micros;
                let (hits, query_trace, tile_candidate_result, queue_active, dispatch_count) =
                    execute_packetized_primary_visibility_query(
                        ctx,
                        current_snapshot,
                        &batch_plan,
                        &shape_batch_plan,
                        input.region_capture_value(),
                        input.frame_domain.clone(),
                        &rays,
                        cull_mask.as_ref(),
                        primary_screen_samples.len(),
                        selected_workgroup_size,
                        input.query_trace_solver_mode,
                    )?;
                gpu_runtime.merge_from(&query_trace.observability.gpu_runtime);
                let primary_gpu_elapsed_micros = gpu_runtime
                    .gpu_time_total_micros
                    .saturating_sub(primary_gpu_time_before);
                let hits = expand_internal_hits(&hits, viewport, primary_viewport);
                if let Some(mask) = cull_mask.as_ref() {
                    tile_cull = mask.stats;
                    if mask.active_samples.len() < primary_screen_samples.len() {
                        runtime.work_items = mask.active_samples.len() as u32;
                        runtime.notes.push(format!(
                            "tile_cull active_tiles={}/{} skipped_samples={}",
                            mask.stats.active_tiles,
                            mask.stats.total_tiles,
                            mask.stats.skipped_samples
                        ));
                    }
                }
                tile_candidate = tile_candidate_result;
                gpu_runtime.primary_visibility_packet_fanout_count = gpu_runtime
                    .primary_visibility_packet_fanout_count
                    .saturating_add(tile_candidate.packet_count as u32);
                packet_scheduling_active = queue_active;
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
                if let Some(clipmap) = view_distance_clipmap.as_ref() {
                    if clipmap.usage_count > 0 || !clipmap.fallback_reasons.is_empty() {
                        runtime.notes.push(clipmap_pass_note(clipmap));
                    }
                    pass_stats.push(clipmap_pass_runtime("view_distance_clipmap", clipmap));
                }
                runtime.notes.push(format!(
                    "tile_candidate_table enabled={} active_samples={}/{} packet_count={} packet_size={}",
                    cull_mask.as_ref().is_some_and(|mask| mask.candidate_table.enabled),
                    tile_candidate.active_samples,
                    tile_candidate.total_samples,
                    tile_candidate.packet_count,
                    tile_candidate.packet_size
                ));
                runtime.notes.push(format!(
                    "packet_scheduling active={} packets={} workgroup_size={}",
                    packet_scheduling_active, tile_candidate.packet_count, selected_workgroup_size
                ));
                materialize_primary_visibility_attachments(&mut attachments, &hits, contract)?;
                gpu_runtime.attachment_encode_count =
                    gpu_runtime.attachment_encode_count.saturating_add(
                        hits.len() as u32
                            * (1 + u32::from(contract.depth_attachment.is_some())
                                + u32::from(contract.world_normal_attachment.is_some())),
                    );
                primary_solver_context = batch_plan
                    .ray_solver
                    .as_ref()
                    .map(|solver| (solver.clone(), batch_plan.artifact_contracts.clone()));
                primary_trace = Some(query_trace);
                primary_hits = Some(hits);
                runtime.notes.push(format!(
                    "workgroup_size={selected_workgroup_size} packet_scheduling_active={packet_scheduling_active}"
                ));
                runtime.dispatch_count = dispatch_count;
                runtime.gpu_elapsed_micros =
                    (primary_gpu_elapsed_micros > 0).then_some(primary_gpu_elapsed_micros);
                runtime.elapsed_micros = pass_start.elapsed().as_micros();
                pass_stats.push(runtime);
            }
            PresentationPassKind::SurfaceResolve { contract } => {
                let hits = primary_hits.as_ref().ok_or_else(|| {
                    PresentationExecError::MissingPrimaryVisibilityPass {
                        plan: effective_plan.name.clone(),
                    }
                })?;
                let pass_start = Instant::now();
                let mut runtime = PassRuntimeStats {
                    pass_id: pass.id.to_string(),
                    pass_kind: "surface_resolve".to_string(),
                    attachment_bytes_read: full_attachment_byte_size(
                        &attachments,
                        contract.primary_hit_attachment.as_str(),
                    ),
                    attachment_bytes_written: full_attachment_byte_size(
                        &attachments,
                        contract.surface_attachment.as_str(),
                    ),
                    ..PassRuntimeStats::default()
                };
                let (count, dispatch_count, notes, surface_gpu_runtime) = execute_surface_resolve(
                    ctx,
                    current_snapshot,
                    input,
                    &mut attachments,
                    hits,
                    contract,
                    DispatchBackend::Wgsl,
                    quality.hit_compaction_enabled,
                )?;
                surface_resolve_count = count;
                gpu_runtime.merge_from(&surface_gpu_runtime);
                runtime.work_items = count;
                runtime.dispatch_count = dispatch_count;
                runtime.gpu_elapsed_micros = (surface_gpu_runtime.gpu_time_total_micros > 0)
                    .then_some(surface_gpu_runtime.gpu_time_total_micros);
                runtime.notes = notes;
                runtime.elapsed_micros = pass_start.elapsed().as_micros();
                pass_stats.push(runtime);
            }
            PresentationPassKind::ParticipantsResolve { contract } => {
                let hits = primary_hits.as_ref().ok_or_else(|| {
                    PresentationExecError::MissingPrimaryVisibilityPass {
                        plan: effective_plan.name.clone(),
                    }
                })?;
                let pass_start = Instant::now();
                let mut runtime = PassRuntimeStats {
                    pass_id: pass.id.to_string(),
                    pass_kind: "participants_resolve".to_string(),
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
                    ..PassRuntimeStats::default()
                };
                let (radiance_count, medium_count, dispatch_count, notes, participants_gpu_runtime) =
                    execute_participants_resolve(
                        ctx,
                        current_snapshot,
                        input,
                        &screen_samples,
                        &mut attachments,
                        hits,
                        contract,
                        DispatchBackend::Wgsl,
                        quality.radiance_mode,
                    )?;
                participant_resolve_count = radiance_count + medium_count;
                gpu_runtime.merge_from(&participants_gpu_runtime);
                runtime.work_items = participant_resolve_count;
                runtime.dispatch_count = dispatch_count;
                runtime.gpu_elapsed_micros = (participants_gpu_runtime.gpu_time_total_micros > 0)
                    .then_some(participants_gpu_runtime.gpu_time_total_micros);
                runtime.notes = notes;
                runtime.elapsed_micros = pass_start.elapsed().as_micros();
                pass_stats.push(runtime);
            }
            PresentationPassKind::ShadePrimary { contract } => {
                let pass_start = Instant::now();
                let gpu_time_before = gpu_runtime.gpu_time_total_micros;
                let dispatch_count = shade_primary_wgsl(
                    &screen_samples,
                    &mut attachments,
                    &input.lighting,
                    camera.position,
                    contract,
                    selected_workgroup_size,
                    &mut gpu_runtime,
                )?;
                let gpu_elapsed_micros = gpu_runtime
                    .gpu_time_total_micros
                    .saturating_sub(gpu_time_before);
                pass_stats.push(PassRuntimeStats {
                    pass_id: pass.id.to_string(),
                    pass_kind: "shade_primary".to_string(),
                    work_items: screen_samples.len() as u32,
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
                    gpu_elapsed_micros: (gpu_elapsed_micros > 0).then_some(gpu_elapsed_micros),
                    elapsed_micros: pass_start.elapsed().as_micros(),
                    ..PassRuntimeStats::default()
                });
            }
            PresentationPassKind::MotionResolve { contract } => {
                let hits = primary_hits.as_ref().ok_or_else(|| {
                    PresentationExecError::MissingPrimaryVisibilityPass {
                        plan: effective_plan.name.clone(),
                    }
                })?;
                let pass_start = Instant::now();
                continuation_counts = motion_resolve(
                    &effective_plan,
                    input,
                    &mut attachments,
                    &screen_samples,
                    hits,
                    contract,
                )?;
                pass_stats.push(PassRuntimeStats {
                    pass_id: pass.id.to_string(),
                    pass_kind: "motion_resolve".to_string(),
                    work_items: screen_samples.len() as u32,
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
                    elapsed_micros: pass_start.elapsed().as_micros(),
                    ..PassRuntimeStats::default()
                });
            }
            PresentationPassKind::TemporalResolve { contract } => {
                let pass_start = Instant::now();
                let gpu_time_before = gpu_runtime.gpu_time_total_micros;
                let temporal = temporal_resolve_wgsl(
                    &mut attachments,
                    viewport.width,
                    viewport.height,
                    contract,
                    selected_workgroup_size,
                    &mut gpu_runtime,
                )?;
                continuation_counts.consumed += temporal.consumed_count;
                let gpu_elapsed_micros = gpu_runtime
                    .gpu_time_total_micros
                    .saturating_sub(gpu_time_before);
                pass_stats.push(PassRuntimeStats {
                    pass_id: pass.id.to_string(),
                    pass_kind: "temporal_resolve".to_string(),
                    work_items: screen_samples.len() as u32,
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
                    gpu_elapsed_micros: (gpu_elapsed_micros > 0).then_some(gpu_elapsed_micros),
                    elapsed_micros: pass_start.elapsed().as_micros(),
                    ..PassRuntimeStats::default()
                });
            }
            PresentationPassKind::CompositeColor { contract } => {
                let pass_start = Instant::now();
                let gpu_time_before = gpu_runtime.gpu_time_total_micros;
                let dispatch_count = composite_color_wgsl(
                    &mut attachments,
                    contract,
                    selected_workgroup_size,
                    &mut gpu_runtime,
                )?;
                let gpu_elapsed_micros = gpu_runtime
                    .gpu_time_total_micros
                    .saturating_sub(gpu_time_before);
                pass_stats.push(PassRuntimeStats {
                    pass_id: pass.id.to_string(),
                    pass_kind: "composite_color".to_string(),
                    work_items: screen_samples.len() as u32,
                    attachment_bytes_read: full_attachment_byte_size(
                        &attachments,
                        contract.input_attachment.as_str(),
                    ),
                    attachment_bytes_written: full_attachment_byte_size(
                        &attachments,
                        contract.output_attachment.as_str(),
                    ),
                    dispatch_count,
                    gpu_elapsed_micros: (gpu_elapsed_micros > 0).then_some(gpu_elapsed_micros),
                    elapsed_micros: pass_start.elapsed().as_micros(),
                    ..PassRuntimeStats::default()
                });
            }
            PresentationPassKind::ExportAttachment { .. } => {}
            other => {
                return Err(PresentationExecError::UnsupportedPlan {
                    message: format!("wgsl executor does not support pass kind {other:?}"),
                });
            }
        }
    }

    let primary_hits =
        primary_hits.ok_or_else(|| PresentationExecError::MissingPrimaryVisibilityPass {
            plan: effective_plan.name.clone(),
        })?;
    let mut primary_trace =
        primary_trace.ok_or_else(|| PresentationExecError::MissingPrimaryVisibilityPass {
            plan: effective_plan.name.clone(),
        })?;
    let continuation_diagnostics = continuation_counts.diagnostics.clone();
    let primary_solver_summary =
        runtime_primary_solver_summary(primary_solver_context.as_ref(), &continuation_counts);
    update_query_trace_continuation(&mut primary_trace, continuation_counts);
    gpu_runtime.dispatch_fragmentation_count = pass_stats
        .iter()
        .map(|pass| pass.dispatch_count.saturating_sub(1))
        .sum();
    let metrics = presentation_metrics(
        &primary_hits,
        &primary_trace,
        primary_solver_summary,
        continuation_diagnostics,
        gpu_runtime.clone(),
    );
    let history = build_temporal_history(
        &effective_plan,
        &input.frame_state,
        &attachments,
        current_snapshot,
        view_distance_clipmap.as_ref(),
    )?;
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
        &attachments,
        pass_stats,
        if candidate_table_active {
            let mut artifacts = vec!["tile_candidate_table".to_string()];
            artifacts.push("view_distance_clipmap".to_string());
            artifacts
        } else {
            if view_distance_clipmap.is_some() {
                vec!["view_distance_clipmap".to_string()]
            } else {
                Vec::new()
            }
        },
    );
    Ok(PresentationExecutionResult {
        plan_name: plan.name.clone(),
        backend: DispatchBackend::Wgsl,
        width: viewport.width,
        height: viewport.height,
        screen_samples,
        attachments,
        history,
        metrics,
        frame_cost,
        query_trace: primary_trace,
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

fn shade_primary_wgsl(
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
    let dispatch = dispatch_linear_shader(
        &shade_primary_shader_source(workgroup_size)?,
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

fn composite_color_wgsl(
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
    let dispatch = dispatch_linear_shader(
        &copy_vec3_shader_source(workgroup_size)?,
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

fn temporal_resolve_wgsl(
    attachments: &mut AttachmentResourceSet,
    width: u32,
    height: u32,
    contract: &TemporalResolvePassContract,
    workgroup_size: u32,
    gpu_runtime: &mut GpuRuntimeMetrics,
) -> Result<TemporalResolveDispatchResult, PresentationExecError> {
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
        return Ok(TemporalResolveDispatchResult {
            consumed_count,
            dispatch_count: 0,
        });
    }
    let dispatch = dispatch_linear_shader(
        &temporal_resolve_shader_source(contract, workgroup_size)?,
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
    Ok(TemporalResolveDispatchResult {
        consumed_count,
        dispatch_count: dispatch.dispatch_count,
    })
}

fn dispatch_linear_shader(
    source: &str,
    input_abi: &PortableAbiType,
    input_values: &[KernelValue],
    output_layout: &FrameAttachmentLayoutPlan,
    workgroup_size: u32,
    gpu_runtime: &mut GpuRuntimeMetrics,
) -> Result<LinearShaderDispatchResult, PresentationExecError> {
    dispatch_linear_shader_with_chunk_limit(
        source,
        input_abi,
        input_values,
        output_layout,
        workgroup_size,
        None,
        gpu_runtime,
    )
}

fn dispatch_linear_shader_with_chunk_limit(
    source: &str,
    input_abi: &PortableAbiType,
    input_values: &[KernelValue],
    output_layout: &FrameAttachmentLayoutPlan,
    workgroup_size: u32,
    per_storage_buffer_limit_override: Option<u64>,
    gpu_runtime: &mut GpuRuntimeMetrics,
) -> Result<LinearShaderDispatchResult, PresentationExecError> {
    if input_values.is_empty() {
        return Ok(LinearShaderDispatchResult {
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
    let aux_buffer = native
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("wrela.presentation.aux"),
            contents: &[0u8; 4],
            usage: wgpu::BufferUsages::STORAGE,
        });
    let mut local_gpu_runtime = GpuRuntimeMetrics {
        upload_bytes: 4,
        transient_buffer_creations: 1,
        ..GpuRuntimeMetrics::default()
    };
    let cached = compiled_pipeline(
        &native,
        source,
        workgroup_size,
        wgpu::BufferSize::new(portable_abi_layout(&dispatch_abi).size as u64),
        &mut local_gpu_runtime,
    )?;
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
        local_gpu_runtime.upload_bytes = local_gpu_runtime
            .upload_bytes
            .saturating_add(storage_buffer_size(&dispatch_bytes))
            .saturating_add(storage_buffer_size(&input_bytes));
        local_gpu_runtime.transient_buffer_creations = local_gpu_runtime
            .transient_buffer_creations
            .saturating_add(3);
        local_gpu_runtime.transient_bind_group_creations = local_gpu_runtime
            .transient_bind_group_creations
            .saturating_add(1);
        let dispatch_buffer = native
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("wrela.presentation.dispatch"),
                contents: &dispatch_bytes,
                usage: wgpu::BufferUsages::STORAGE,
            });
        let input_buffer = native
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("wrela.presentation.input"),
                contents: &input_bytes,
                usage: wgpu::BufferUsages::STORAGE,
            });
        let output_buffer = native.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("wrela.presentation.output"),
            size: chunk_dense_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
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
            pass.set_bind_group(0, &bind_group, &[]);
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
        let chunk_bytes =
            readback_storage_buffer(&output_buffer, chunk_dense_size).map_err(|message| {
                PresentationExecError::Query(crate::query_exec::cpu::QueryExecError::Unsupported {
                    message: format!("native WGSL readback failed: {message}"),
                })
            })?;
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
    Ok(LinearShaderDispatchResult {
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
        ],
    })
}

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

fn shade_primary_shader_source(workgroup_size: u32) -> Result<String, PresentationExecError> {
    let structs = emit_wgsl_structs(&[
        crate::query_exec::wgsl::codegen::wgsl_dispatch_config_abi(),
        shade_primary_input_abi(),
    ])?;
    Ok(format!(
        "{structs}

override WG_SIZE: u32 = {workgroup_size}u;

@group(0) @binding(0)
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

@group(0) @binding(1)
var<storage, read> input_items: InputBuffer;
@group(0) @binding(2)
var<storage, read_write> output_items: OutputBuffer;
@group(0) @binding(3)
var<storage, read> dummy_items: DummyBuffer;

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
    output_items.values[index] = mix(lit, fog_color, vec3<f32>(fog_strength));
  }} else {{
    let miss_fog = clamp(input.medium.density * 3.0, 0.0, 0.45);
    let miss_mix_color = input.medium.emission + (input.radiance * 0.28);
    output_items.values[index] = mix(input.radiance, miss_mix_color, vec3<f32>(miss_fog));
  }}
}}
"
    ))
}

fn copy_vec3_shader_source(workgroup_size: u32) -> Result<String, PresentationExecError> {
    let structs =
        emit_wgsl_structs(&[crate::query_exec::wgsl::codegen::wgsl_dispatch_config_abi()])?;
    Ok(format!(
        "{structs}

override WG_SIZE: u32 = {workgroup_size}u;

@group(0) @binding(0)
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

@group(0) @binding(1)
var<storage, read> input_items: InputBuffer;
@group(0) @binding(2)
var<storage, read_write> output_items: OutputBuffer;
@group(0) @binding(3)
var<storage, read> dummy_items: DummyBuffer;

@compute @workgroup_size(WG_SIZE)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {{
  let index = global_id.x;
  _ = dummy_items.values[0];
  if (index >= dispatch_config.item_count) {{
    return;
  }}
  output_items.values[index] = input_items.values[index];
}}
"
    ))
}

fn temporal_resolve_shader_source(
    contract: &TemporalResolvePassContract,
    workgroup_size: u32,
) -> Result<String, PresentationExecError> {
    let structs = emit_wgsl_structs(&[
        crate::query_exec::wgsl::codegen::wgsl_dispatch_config_abi(),
        temporal_resolve_input_abi(),
    ])?;
    let history_weight = contract.history_weight_numerator as f32
        / contract.history_weight_denominator.max(1) as f32;
    Ok(format!(
        "{structs}

override WG_SIZE: u32 = {workgroup_size}u;

@group(0) @binding(0)
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

@group(0) @binding(1)
var<storage, read> input_items: InputBuffer;
@group(0) @binding(2)
var<storage, read_write> output_items: OutputBuffer;
@group(0) @binding(3)
var<storage, read> dummy_items: DummyBuffer;

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
    output_items.values[index] = mix(input.current_color, clamped_history, vec3<f32>({history_weight}));
  }} else {{
    output_items.values[index] = input.current_color;
  }}
}}
"
    ))
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

        let dispatch = dispatch_linear_shader(
            &copy_vec3_shader_source(64).expect("copy vec3 shader"),
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

        let dispatch = dispatch_linear_shader_with_chunk_limit(
            &copy_vec3_shader_source(64).expect("copy vec3 shader"),
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
        for (index, expected) in input_values.iter().enumerate() {
            assert_eq!(
                resource.decode(index).expect("decode row-aligned output"),
                *expected
            );
        }
    }
}
