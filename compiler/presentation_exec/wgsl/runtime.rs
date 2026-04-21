use super::*;

pub(super) fn acquire_presentation_upload_buffer(
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

pub(super) fn release_presentation_upload_buffers(
    native: &crate::query_exec::wgsl::NativeWgpuContext,
    buffers: impl IntoIterator<Item = (BufferPoolKey, wgpu::Buffer)>,
) {
    let pool = shared_buffer_pool(native.limit_request);
    let mut guard = pool.lock().unwrap_or_else(|poison| poison.into_inner());
    for (key, buffer) in buffers {
        guard.release(key, buffer);
    }
}
pub(super) fn presentation_framegraph_error(
    error: PresentationFramegraphError,
) -> PresentationExecError {
    PresentationExecError::UnsupportedPlan {
        message: error.to_string(),
    }
}

pub(super) fn note_framegraph_submission_metrics(
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

pub(super) fn submission_readback_bytes<'a>(
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

pub(super) fn decode_primary_hit_attachment_bytes_from_name(
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

pub(super) fn sum_gpu_elapsed_micros(
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

pub(super) fn decode_u32_count(bytes: &[u8]) -> u32 {
    bytes
        .get(..4)
        .and_then(|value| value.try_into().ok())
        .map(u32::from_le_bytes)
        .unwrap_or_default()
}

pub(super) fn decode_motion_counts(bytes: &[u8]) -> (u32, u32, u32) {
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

pub(super) fn apply_attachment_readbacks(
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
            attachment.bytes = result.bytes.clone().into();
        }
    }
    Ok(())
}

pub(super) fn staging_attachment_resources(
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

pub(super) fn timed_attachment_readback_names(
    _plan: &PresentationPlan,
    explicitly_exported_attachments: &BTreeSet<SmolStr>,
) -> Vec<SmolStr> {
    explicitly_exported_attachments.iter().cloned().collect()
}

pub(super) fn temporal_history_attachment_names(plan: &PresentationPlan) -> Vec<SmolStr> {
    let mut names = BTreeSet::new();
    if let Some(temporal) = &plan.frame.temporal {
        for slot in &temporal.history_slots {
            names.insert(slot.attachment.clone());
        }
    }
    names.into_iter().collect()
}

pub(super) fn untimed_cpu_materialization_attachment_names(
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

pub(super) fn upload_attachment_to_gpu(
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
        native
            .queue
            .write_buffer(buffer, 0, cpu_attachment.bytes.as_slice());
        gpu_runtime.upload_bytes = gpu_runtime
            .upload_bytes
            .saturating_add(cpu_attachment.bytes.len() as u64);
    }
    Ok(())
}

pub(super) fn build_primary_batch_query_trace(
    contract_id: crate::query_contract::QueryContractId,
    snapshot: &crate::world_identity::WorldSnapshotHandle,
    plan: &crate::kernel::ir::KernelBatchQueryPlan,
    item_count: u32,
    summarize_iterations: bool,
    observability: QueryExecutionObservability,
) -> Result<BatchQueryExecutionTrace, PresentationExecError> {
    let descriptor = crate::query_contract::query_contract(contract_id).ok_or_else(|| {
        PresentationExecError::UnsupportedPlan {
            message: format!("missing query contract '{}'", contract_id.as_str()),
        }
    })?;
    let plan_trace = if summarize_iterations {
        crate::kernel::summarize_batch_query(plan, item_count)
    } else {
        interpret_batch_query(plan, item_count)
    };
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
