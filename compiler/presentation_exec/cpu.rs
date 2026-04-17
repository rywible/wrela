//! Owns the CPU-backed presentation execution path and its pass-level helpers.
//! Does not own high-level plan selection or WGSL runtime execution.
//!
//! Key invariants:
//! - CPU presentation remains the semantic oracle for pass ordering and
//!   attachment meaning when WGSL paths are compared.
//! - pass helpers may reuse attachments/history, but they must preserve the
//!   contract semantics the surrounding plan expects.
//!
//! Primary entrypoints:
//! - CPU presentation execution helpers in this module
//!
//! Failure modes / common pitfalls:
//! - letting CPU-only convenience behavior leak into shared report surfaces
//!   makes backend parity harder to reason about.

use crate::kernel::{KernelValue, lower_batch_query_plan};
use crate::presentation_contract::RealtimeRadianceMode;
use crate::presentation_exec::clipmap::{
    build_view_distance_clipmap_artifact, clipmap_pass_note, clipmap_pass_runtime,
};
use crate::presentation_exec::resources::AttachmentResourceSet;
use crate::presentation_exec::temporal::{
    motion_resolve, temporal_resolve_cpu, update_query_trace_continuation,
};
use crate::presentation_exec::{
    PassRuntimeStats, PresentationExecError, PresentationExecutionInput,
    PresentationExecutionResult, TileCullingStats, adjusted_ray_budget,
    allocate_execution_attachments, attachment_hit_work_items, build_frame_cost_report,
    build_temporal_history, effective_plan_for_quality, encode_values_at_indices,
    execute_batch_contract, expand_internal_hits, expect_array, expect_f32, expect_struct,
    expect_vec3, field, frame_state_components, full_attachment_byte_size, generate_screen_samples,
    hit_world_normal, internal_resolution_viewport, materialize_primary_visibility_attachments,
    participant_query_work_items, presentation_metrics, primary_hit_miss_value,
    resolved_quality_state, runtime_primary_solver_summary, screen_sample_ray, shade_lookup_value,
    tile_candidate_dispatch_packets, tile_candidate_packet_fragment_count,
    tile_candidate_packet_sample_count, tile_candidate_stats, tile_culling_mask,
};
use crate::presentation_plan::{
    CompositeColorPassContract, ParticipantsResolvePassContract, PresentationPassKind,
    PresentationPlan, ShadePrimaryPassContract, SurfaceResolvePassContract,
};
use crate::query_exec::cpu::{default_medium, default_surface};
use crate::query_exec::{QueryExecContext, execute_batch_query_with_solver_mode_with_snapshot_on};
use crate::query_plan::{BatchQueryPlan, DispatchBackend};
use std::time::Instant;

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
                        DispatchBackend::Cpu,
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
                        DispatchBackend::Cpu,
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
                let packet_size =
                    crate::presentation_exec::PRESENTATION_WORKGROUP_SIZE_CANDIDATES[0];
                let (hits, query_trace, tile_candidate_stats_result, queue_active, dispatch_count) =
                    if let Some(mask) = cull_mask.as_ref() {
                        tile_cull = mask.stats;
                        if mask.active_samples.len() < primary_screen_samples.len() {
                            runtime.notes.push(format!(
                                "tile_cull active_tiles={}/{} skipped_samples={}",
                                mask.stats.active_tiles,
                                mask.stats.total_tiles,
                                mask.stats.skipped_samples
                            ));
                        }
                        let mut hits = vec![primary_hit_miss_value(); primary_screen_samples.len()];
                        let (stats, skipped_samples, queue_active, mut query_trace, dispatch_count) =
                            if mask.candidate_table.enabled {
                                let packets = tile_candidate_dispatch_packets(
                                    &mask.candidate_table,
                                    packet_size,
                                );
                                let mut covered_samples = vec![false; primary_screen_samples.len()];
                                let mut dispatch_count = 1u32;
                                let (_, mut query_trace) =
                                    execute_batch_query_with_solver_mode_with_snapshot_on(
                                        ctx,
                                        DispatchBackend::Cpu,
                                        Some(current_snapshot),
                                        &batch_plan,
                                        &[
                                            input.region_capture_value(),
                                            input.frame_domain.clone(),
                                            KernelValue::Array(Vec::new()),
                                        ],
                                        input.query_trace_solver_mode,
                                    )?;
                                for packet in &packets {
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
                                        let shape_snapshot = ctx.shape_snapshot_handle(shape).ok_or_else(
                                    || PresentationExecError::UnsupportedPlan {
                                        message: format!(
                                            "missing shape snapshot for tile candidate '{shape}'"
                                        ),
                                    },
                                )?;
                                        let (shape_hits, shape_trace) =
                                            execute_batch_query_with_solver_mode_with_snapshot_on(
                                                ctx,
                                                DispatchBackend::Cpu,
                                                Some(shape_snapshot),
                                                &shape_batch_plan,
                                                &[
                                                    shape_snapshot.capture_value(),
                                                    KernelValue::Array(packet_rays.clone()),
                                                ],
                                                input.query_trace_solver_mode,
                                            )?;
                                        dispatch_count = dispatch_count.saturating_add(1);
                                        query_trace
                                            .observability
                                            .merge_from(&shape_trace.observability);
                                        for ((best_hit, best_distance), candidate_hit) in
                                            packet_hits
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
                                    for (index, hit) in
                                        packet.sample_indices.iter().zip(packet_hits.into_iter())
                                    {
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
                                            DispatchBackend::Cpu,
                                            Some(current_snapshot),
                                            &batch_plan,
                                            &[
                                                input.region_capture_value(),
                                                input.frame_domain.clone(),
                                                KernelValue::Array(fallback_rays),
                                            ],
                                            input.query_trace_solver_mode,
                                        )?;
                                    dispatch_count = dispatch_count.saturating_add(1);
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
                                        tile_candidate_packet_sample_count(&packets),
                                        tile_candidate_packet_fragment_count(&packets, packet_size),
                                        packet_size,
                                    ),
                                    mask.stats.skipped_samples,
                                    !packets.is_empty(),
                                    query_trace,
                                    dispatch_count,
                                )
                            } else {
                                let active_rays = mask
                                    .active_samples
                                    .iter()
                                    .map(|index| rays[*index].clone())
                                    .collect::<Vec<_>>();
                                let (active_hits, query_trace) =
                                    execute_batch_query_with_solver_mode_with_snapshot_on(
                                        ctx,
                                        DispatchBackend::Cpu,
                                        Some(current_snapshot),
                                        &batch_plan,
                                        &[
                                            input.region_capture_value(),
                                            input.frame_domain.clone(),
                                            KernelValue::Array(active_rays),
                                        ],
                                        input.query_trace_solver_mode,
                                    )?;
                                for (index, hit) in mask
                                    .active_samples
                                    .iter()
                                    .zip(expect_array(&active_hits)?.to_vec())
                                {
                                    hits[*index] = hit;
                                }
                                (
                                    crate::presentation_exec::TileCandidateStats::default(),
                                    mask.stats.skipped_samples,
                                    false,
                                    query_trace,
                                    1,
                                )
                            };
                        query_trace.observability.screen_sample_count = screen_samples.len() as u32;
                        query_trace.observability.miss_count = query_trace
                            .observability
                            .miss_count
                            .saturating_add(skipped_samples);
                        (
                            expand_internal_hits(&hits, viewport, primary_viewport),
                            query_trace,
                            stats,
                            queue_active,
                            dispatch_count,
                        )
                    } else {
                        let (hits, query_trace) =
                            execute_batch_query_with_solver_mode_with_snapshot_on(
                                ctx,
                                DispatchBackend::Cpu,
                                Some(current_snapshot),
                                &batch_plan,
                                &[
                                    input.region_capture_value(),
                                    input.frame_domain.clone(),
                                    KernelValue::Array(rays),
                                ],
                                input.query_trace_solver_mode,
                            )?;
                        let stats = tile_candidate_stats(
                            primary_screen_samples.len(),
                            primary_screen_samples.len(),
                            1,
                            primary_screen_samples.len().max(1) as u32,
                        );
                        (
                            expand_internal_hits(
                                &expect_array(&hits)?.to_vec(),
                                viewport,
                                primary_viewport,
                            ),
                            query_trace,
                            stats,
                            false,
                            1,
                        )
                    };
                tile_candidate = tile_candidate_stats_result;
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
                    "tile_candidate_table active_samples={}/{} packet_count={} packet_size={}",
                    tile_candidate.active_samples,
                    tile_candidate.total_samples,
                    tile_candidate.packet_count,
                    tile_candidate.packet_size
                ));
                materialize_primary_visibility_attachments(&mut attachments, &hits, contract)?;
                primary_solver_context = batch_plan
                    .ray_solver
                    .as_ref()
                    .map(|solver| (solver.clone(), batch_plan.artifact_contracts.clone()));
                primary_trace = Some(query_trace);
                primary_hits = Some(hits);
                runtime.dispatch_count = dispatch_count;
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
                let (count, notes) = execute_surface_resolve(
                    ctx,
                    current_snapshot,
                    input,
                    &mut attachments,
                    hits,
                    contract,
                    DispatchBackend::Cpu,
                    quality.hit_compaction_enabled,
                )?;
                surface_resolve_count = count;
                runtime.work_items = count;
                runtime.dispatch_count = u32::from(count > 0);
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
                let (radiance_count, medium_count, notes) = execute_participants_resolve(
                    ctx,
                    current_snapshot,
                    input,
                    &screen_samples,
                    &mut attachments,
                    hits,
                    contract,
                    DispatchBackend::Cpu,
                    quality.radiance_mode,
                )?;
                participant_resolve_count = radiance_count + medium_count;
                runtime.work_items = participant_resolve_count;
                runtime.dispatch_count =
                    u32::from(radiance_count > 0) + u32::from(medium_count > 0);
                runtime.notes = notes;
                runtime.elapsed_micros = pass_start.elapsed().as_micros();
                pass_stats.push(runtime);
            }
            PresentationPassKind::ShadePrimary { contract } => {
                let pass_start = Instant::now();
                shade_primary_cpu(
                    &screen_samples,
                    &mut attachments,
                    &input.lighting,
                    camera.position,
                    contract,
                )?;
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
                continuation_counts.consumed += temporal_resolve_cpu(
                    &mut attachments,
                    viewport.width,
                    viewport.height,
                    contract,
                )?;
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
                    elapsed_micros: pass_start.elapsed().as_micros(),
                    ..PassRuntimeStats::default()
                });
            }
            PresentationPassKind::CompositeColor { contract } => {
                let pass_start = Instant::now();
                composite_color_cpu(&mut attachments, contract)?;
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
                    elapsed_micros: pass_start.elapsed().as_micros(),
                    ..PassRuntimeStats::default()
                });
            }
            PresentationPassKind::ExportAttachment { .. } => {}
            other => {
                return Err(PresentationExecError::UnsupportedPlan {
                    message: format!("cpu executor does not support pass kind {other:?}"),
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
    let metrics = presentation_metrics(
        &primary_hits,
        &primary_trace,
        primary_solver_summary,
        continuation_diagnostics,
        primary_trace.observability.gpu_runtime.clone(),
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
        DispatchBackend::Cpu,
        viewport.width,
        viewport.height,
        &effective_plan.frame.quality,
        &quality,
        &metrics,
        tile_cull,
        tile_candidate,
        packet_scheduling_active,
        0,
        surface_resolve_count,
        participant_resolve_count,
        crate::presentation_exec::attachment_byte_reports(&attachments, None),
        pass_stats,
        Vec::new(),
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
        backend: DispatchBackend::Cpu,
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
) -> Result<(u32, Vec<String>), PresentationExecError> {
    let Some(_) = attachments.attachment(contract.surface_attachment.as_str()) else {
        return Ok((0, Vec::new()));
    };
    let default_surface = default_surface();
    let mut notes = Vec::new();
    if contract.explicit_miss_default {
        notes.push("explicit_miss_default".to_string());
    }
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
            return Ok((0, notes));
        }
        let hit_indices = work_items
            .iter()
            .map(|(index, _)| *index)
            .collect::<Vec<_>>();
        let hit_values = work_items
            .iter()
            .map(|(_, hit)| hit.clone())
            .collect::<Vec<_>>();
        let (surfaces, _) = execute_batch_contract(
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
        if compact_hits {
            encode_values_at_indices(
                attachments,
                contract.surface_attachment.as_str(),
                &hit_indices,
                &surfaces,
            )?;
        } else {
            let Some(surface_attachment) =
                attachments.attachment_mut(contract.surface_attachment.as_str())
            else {
                return Ok((0, notes));
            };
            for ((index, hit), surface) in work_items.iter().zip(surfaces.iter()) {
                if hit_flag(hit)? {
                    surface_attachment.encode(*index, surface)?;
                } else {
                    surface_attachment.encode(*index, &default_surface)?;
                }
            }
        }
        return Ok((hit_indices.len() as u32, notes));
    }

    let (surfaces, _) = execute_batch_contract(
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
        return Ok((0, notes));
    };
    for (index, (hit, surface)) in hits.iter().zip(surfaces.iter()).enumerate() {
        if hit_flag(hit)? {
            surface_attachment.encode(index, surface)?;
        } else {
            surface_attachment.encode(index, &default_surface)?;
        }
    }
    Ok((hits.len() as u32, notes))
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
) -> Result<(u32, u32, Vec<String>), PresentationExecError> {
    let mut radiance_count = 0;
    let mut medium_count = 0;
    let mut notes = Vec::new();

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
            let (radiance, _) = execute_batch_contract(
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
            encode_values_at_indices(attachments, attachment_name, &target_indices, &radiance)?;
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
            let (medium, _) = execute_batch_contract(
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
            encode_values_at_indices(attachments, attachment_name, &target_indices, &medium)?;
        }
    }

    Ok((radiance_count, medium_count, notes))
}

fn encode_attachment_values(
    attachments: &mut AttachmentResourceSet,
    name: &str,
    values: &[KernelValue],
) -> Result<(), PresentationExecError> {
    let Some(attachment) = attachments.attachment_mut(name) else {
        return Ok(());
    };
    for (index, value) in values.iter().enumerate() {
        attachment.encode(index, value)?;
    }
    Ok(())
}

fn shade_primary_cpu(
    screen_samples: &[KernelValue],
    attachments: &mut AttachmentResourceSet,
    lighting: &crate::presentation_contract::PresentationLightingInputs,
    camera_position: [f32; 3],
    contract: &ShadePrimaryPassContract,
) -> Result<(), PresentationExecError> {
    let primary_hits = attachments.decode_attachment(contract.primary_hit_attachment.as_str())?;
    let default_surface = default_surface();
    let default_medium = default_medium();
    for index in 0..primary_hits.len() {
        let sample =
            screen_samples
                .get(index)
                .ok_or_else(|| PresentationExecError::UnsupportedPlan {
                    message: "screen sample count drifted from primary-hit attachment".to_string(),
                })?;
        let ray = expect_struct(
            field(expect_struct(sample, "ScreenSampleQuery")?, "ray")?,
            "RayQuery",
        )?;
        let ray_direction = expect_vec3(field(ray, "direction")?)?;
        let default_radiance = KernelValue::Vec3([0.0, 0.0, 0.0]);
        let radiance = contract
            .radiance_attachment
            .as_ref()
            .map(|name| shade_lookup_value(attachments, name, index, &default_radiance))
            .transpose()?
            .unwrap_or(default_radiance);
        let medium = contract
            .medium_attachment
            .as_ref()
            .map(|name| shade_lookup_value(attachments, name, index, &default_medium))
            .transpose()?
            .unwrap_or_else(|| default_medium.clone());
        let surface = shade_lookup_value(
            attachments,
            contract.surface_attachment.as_str(),
            index,
            &default_surface,
        )?;
        let color = shade_compatibility_color(
            &primary_hits[index],
            &surface,
            &radiance,
            &medium,
            ray_direction,
            camera_position,
            lighting,
        )?;
        if let Some(output_attachment) =
            attachments.attachment_mut(contract.output_attachment.as_str())
        {
            output_attachment.encode(index, &KernelValue::Vec3(color))?;
        }
    }
    Ok(())
}

fn composite_color_cpu(
    attachments: &mut AttachmentResourceSet,
    contract: &CompositeColorPassContract,
) -> Result<(), PresentationExecError> {
    let input_values = attachments.decode_attachment(contract.input_attachment.as_str())?;
    encode_attachment_values(
        attachments,
        contract.output_attachment.as_str(),
        &input_values,
    )
}

fn shade_compatibility_color(
    hit: &KernelValue,
    surface: &KernelValue,
    radiance: &KernelValue,
    medium: &KernelValue,
    ray_direction: [f32; 3],
    camera_position: [f32; 3],
    lighting: &crate::presentation_contract::PresentationLightingInputs,
) -> Result<[f32; 3], PresentationExecError> {
    if hit_flag(hit)? {
        let hit_position = hit_position(hit)?;
        let hit_normal = hit_world_normal(hit)?;
        let key_dir = normalize3(sub3(lighting.key_light.position, hit_position));
        let view_dir = normalize3(sub3(camera_position, hit_position));
        let half_dir = normalize3(add3(key_dir, view_dir));
        let distance_to_light = length3(sub3(lighting.key_light.position, hit_position));
        let attenuation =
            clamp01(1.0 - (distance_to_light / lighting.key_light.range.max(f32::EPSILON)));
        let ndotl = clamp01(dot3(hit_normal, key_dir));
        let ndoth = clamp01(dot3(hit_normal, half_dir));
        let diffuse = ndotl * attenuation;
        let fill =
            clamp01(dot3(hit_normal, normalize3(lighting.fill_direction))) * lighting.fill_strength;
        let surface = expect_struct(surface, "Surface")?;
        let albedo = expect_vec3(field(surface, "albedo")?)?;
        let roughness = clamp01(expect_f32(field(surface, "roughness")?)?);
        let metalness = clamp01(expect_f32(field(surface, "metalness")?)?);
        let clearcoat = clamp01(expect_f32(field(surface, "clearcoat")?)?);
        let emissive = expect_vec3(field(surface, "emissive")?)?;
        let spec_power = mix(48.0, 8.0, roughness);
        let spec_raw = ndoth.powf(spec_power);
        let specular_strength = 0.10 + (metalness * 0.25) + (clearcoat * 0.20);
        let highlight = spec_raw * specular_strength;
        let lighting_rgb = add3(
            lighting.ambient_color,
            [diffuse + fill, diffuse + fill, diffuse + fill],
        );
        let direct = clamp_vec3(
            add3(
                mul3(mul3_componentwise(albedo, lighting_rgb), 1.0)
                    .zip_map(lighting.key_light.intensity, |lane, intensity| {
                        lane * intensity
                    }),
                [highlight * 220.0, highlight * 208.0, highlight * 196.0],
            ),
            0.0,
            255.0,
        );
        let medium = expect_struct(medium, "Medium")?;
        let medium_density = expect_f32(field(medium, "density")?)?;
        let medium_emission = expect_vec3(field(medium, "emission")?)?;
        let fog_strength = clamp(medium_density * distance_to_light * 0.18, 0.0, 0.55);
        let radiance = expect_vec3(radiance)?;
        let fog_color = add3(medium_emission, mul3(radiance, 0.22));
        let radiance_lit = mul3(radiance, 0.25 + (highlight * 0.15));
        let lit = add3(add3(direct, emissive), radiance_lit);
        Ok(mix3(lit, fog_color, fog_strength))
    } else {
        let radiance = expect_vec3(radiance)?;
        let medium = expect_struct(medium, "Medium")?;
        let density = expect_f32(field(medium, "density")?)?;
        let emission = expect_vec3(field(medium, "emission")?)?;
        let miss_fog = clamp(density * 3.0, 0.0, 0.45);
        let miss_mix_color = add3(emission, mul3(radiance, 0.28));
        let _ = ray_direction;
        Ok(mix3(radiance, miss_mix_color, miss_fog))
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

fn hit_position(value: &KernelValue) -> Result<[f32; 3], PresentationExecError> {
    expect_vec3(field(expect_struct(value, "Hit3")?, "position")?)
}

fn hit_distance(value: &KernelValue) -> Result<f32, PresentationExecError> {
    expect_f32(field(expect_struct(value, "Hit3")?, "distance")?)
}

trait ZipMap {
    fn zip_map(self, rhs: [f32; 3], f: impl Fn(f32, f32) -> f32) -> [f32; 3];
}

impl ZipMap for [f32; 3] {
    fn zip_map(self, rhs: [f32; 3], f: impl Fn(f32, f32) -> f32) -> [f32; 3] {
        [f(self[0], rhs[0]), f(self[1], rhs[1]), f(self[2], rhs[2])]
    }
}

fn add3(lhs: [f32; 3], rhs: [f32; 3]) -> [f32; 3] {
    [lhs[0] + rhs[0], lhs[1] + rhs[1], lhs[2] + rhs[2]]
}

fn sub3(lhs: [f32; 3], rhs: [f32; 3]) -> [f32; 3] {
    [lhs[0] - rhs[0], lhs[1] - rhs[1], lhs[2] - rhs[2]]
}

fn mul3(value: [f32; 3], scalar: f32) -> [f32; 3] {
    [value[0] * scalar, value[1] * scalar, value[2] * scalar]
}

fn mul3_componentwise(lhs: [f32; 3], rhs: [f32; 3]) -> [f32; 3] {
    [lhs[0] * rhs[0], lhs[1] * rhs[1], lhs[2] * rhs[2]]
}

fn dot3(lhs: [f32; 3], rhs: [f32; 3]) -> f32 {
    (lhs[0] * rhs[0]) + (lhs[1] * rhs[1]) + (lhs[2] * rhs[2])
}

fn length3(value: [f32; 3]) -> f32 {
    dot3(value, value).sqrt()
}

fn normalize3(value: [f32; 3]) -> [f32; 3] {
    let length = length3(value);
    if length <= f32::EPSILON {
        [0.0, 0.0, 0.0]
    } else {
        [value[0] / length, value[1] / length, value[2] / length]
    }
}

fn mix(lhs: f32, rhs: f32, t: f32) -> f32 {
    lhs * (1.0 - t) + rhs * t
}

fn mix3(lhs: [f32; 3], rhs: [f32; 3], t: f32) -> [f32; 3] {
    [
        mix(lhs[0], rhs[0], t),
        mix(lhs[1], rhs[1], t),
        mix(lhs[2], rhs[2], t),
    ]
}

fn clamp(value: f32, min: f32, max: f32) -> f32 {
    value.max(min).min(max)
}

fn clamp01(value: f32) -> f32 {
    clamp(value, 0.0, 1.0)
}

fn clamp_vec3(value: [f32; 3], min: f32, max: f32) -> [f32; 3] {
    [
        clamp(value[0], min, max),
        clamp(value[1], min, max),
        clamp(value[2], min, max),
    ]
}
