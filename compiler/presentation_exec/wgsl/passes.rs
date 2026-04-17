//! Owns WGSL presentation pass execution helpers.
//! Does not own shader source generation or runtime resource staging.
//!
//! Key invariants:
//! - each pass helper must preserve attachment semantics promised by its pass
//!   contract.
//! - pass-local fallbacks still need to report the path that actually executed.
//!
//! Primary entrypoints:
//! - WGSL pass execution helpers in this module
//!
//! Failure modes / common pitfalls:
//! - treating compact/non-compact hit paths as interchangeable masks real pass
//!   contract differences.

use super::*;

pub(super) fn execute_surface_resolve(
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

pub(super) fn execute_participants_resolve(
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

pub(super) fn execute_participants_resolve_without_screen_samples(
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

pub(super) fn execute_packetized_primary_visibility_query(
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
