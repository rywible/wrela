//! Owns presentation framegraph submission structure, readback scheduling, and
//! attachment-slot lifetime bookkeeping.
//! Does not own shader execution or high-level plan selection.
//!
//! Key invariants:
//! - framegraph ordering may optimize submission structure, but resource
//!   lifetimes and readback boundaries must remain explicit.
//! - attachment slots must not be reused across incompatible pass contracts.
//!
//! Primary entrypoints:
//! - `PresentationFramegraph`
//! - framegraph submission/readback helpers in this module
//!
//! Failure modes / common pitfalls:
//! - hidden resource lifetime coupling here turns later rendering bugs into
//!   hard-to-debug framegraph corruption.

use crate::gpu_runtime::{
    GpuPassProfiler, GpuRuntimeContext, ReadbackRequest, ReadbackResult, ReadbackTicket,
    collect_storage_buffer_readback, schedule_storage_buffer_readback,
};
use crate::presentation_binding::PresentationBindingId;
use crate::presentation_exec::gpu_resources::{GpuAttachmentArena, GpuAttachmentSlot};
use crate::presentation_exec::resources::{
    AttachmentResourceSet, PresentationResourceError, allocate_attachment_resources_without_history,
};
use crate::presentation_plan::{PresentationPassKind, PresentationPlan};
use smol_str::SmolStr;
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug)]
pub struct PresentationFramegraphPass {
    pub id: SmolStr,
    pub kind: PresentationPassKind,
    pub consumes: Vec<SmolStr>,
    pub materializes: Vec<SmolStr>,
    pub binding: Option<PresentationBindingId>,
    pub query_dependencies: Vec<crate::query_plan::QueryContractId>,
}

pub struct PresentationFramegraph {
    pub plan: PresentationPlan,
    pub passes: Vec<PresentationFramegraphPass>,
    pub attachments: GpuAttachmentArena,
    native: Option<Arc<GpuRuntimeContext>>,
    encoder: Option<wgpu::CommandEncoder>,
    profiler: Option<GpuPassProfiler>,
    readbacks: Vec<ReadbackTicket>,
    max_timestamped_passes: u32,
    documented_exceptions: Vec<SmolStr>,
    queue_submit_count: u32,
}

pub struct PresentationFramegraphSubmission {
    pub readbacks: Vec<ReadbackResult>,
    pub gpu_elapsed_micros: Vec<u128>,
    pub timestamps_supported: bool,
    pub documented_exceptions: Vec<SmolStr>,
    pub queue_submit_count: u32,
}

#[derive(Debug, Error)]
pub enum PresentationFramegraphError {
    #[error("framegraph is not GPU-backed")]
    MissingGpuContext,
    #[error("{0}")]
    Runtime(String),
}

impl PresentationFramegraph {
    pub fn from_plan_and_resources(
        plan: PresentationPlan,
        attachments: AttachmentResourceSet,
    ) -> Self {
        let passes = plan
            .passes
            .iter()
            .map(|pass| PresentationFramegraphPass {
                id: pass.id.clone(),
                kind: pass.kind.clone(),
                consumes: pass.consumes.clone(),
                materializes: pass.materializes.clone(),
                binding: pass.binding.clone(),
                query_dependencies: pass.query_dependencies.clone(),
            })
            .collect();
        Self {
            plan,
            passes,
            attachments: GpuAttachmentArena::from_attachment_resources(&attachments),
            native: None,
            encoder: None,
            profiler: None,
            readbacks: Vec::new(),
            max_timestamped_passes: 0,
            documented_exceptions: Vec::new(),
            queue_submit_count: 0,
        }
    }

    pub fn from_plan_and_gpu_resources(
        plan: PresentationPlan,
        attachments: AttachmentResourceSet,
        native: Arc<GpuRuntimeContext>,
        max_timestamped_passes: u32,
    ) -> Self {
        let passes = plan
            .passes
            .iter()
            .map(|pass| PresentationFramegraphPass {
                id: pass.id.clone(),
                kind: pass.kind.clone(),
                consumes: pass.consumes.clone(),
                materializes: pass.materializes.clone(),
                binding: pass.binding.clone(),
                query_dependencies: pass.query_dependencies.clone(),
            })
            .collect();
        let encoder = native
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("wrela.presentation.framegraph.encoder"),
            });
        Self {
            plan,
            passes,
            attachments: GpuAttachmentArena::from_attachment_resources_gpu(&native, &attachments),
            native: Some(native.clone()),
            encoder: Some(encoder),
            profiler: Some(GpuPassProfiler::new(&native, max_timestamped_passes)),
            readbacks: Vec::new(),
            max_timestamped_passes,
            documented_exceptions: Vec::new(),
            queue_submit_count: 0,
        }
    }

    pub fn new(
        plan: PresentationPlan,
        width: u32,
        height: u32,
    ) -> Result<Self, PresentationResourceError> {
        let attachments =
            allocate_attachment_resources_without_history(&plan.frame, width, height)?;
        Ok(Self::from_plan_and_resources(plan, attachments))
    }

    pub fn plan(&self) -> &PresentationPlan {
        &self.plan
    }

    pub fn attachment(&self, name: &str) -> Option<&GpuAttachmentSlot> {
        self.attachments.attachment(name)
    }

    pub fn attachment_mut(&mut self, name: &str) -> Option<&mut GpuAttachmentSlot> {
        self.attachments.attachment_mut(name)
    }

    pub fn attachment_resources(
        &self,
    ) -> Result<
        AttachmentResourceSet,
        crate::presentation_exec::gpu_resources::GpuAttachmentArenaError,
    > {
        self.attachments.materialize_cpu_resources()
    }

    pub fn pass(&self, id: &str) -> Option<&PresentationFramegraphPass> {
        self.passes.iter().find(|pass| pass.id == id)
    }

    pub fn native(&self) -> Result<&Arc<GpuRuntimeContext>, PresentationFramegraphError> {
        self.native
            .as_ref()
            .ok_or(PresentationFramegraphError::MissingGpuContext)
    }

    pub fn encoder_mut(
        &mut self,
    ) -> Result<&mut wgpu::CommandEncoder, PresentationFramegraphError> {
        self.encoder
            .as_mut()
            .ok_or(PresentationFramegraphError::MissingGpuContext)
    }

    pub fn profiler(&self) -> Result<&GpuPassProfiler, PresentationFramegraphError> {
        self.profiler
            .as_ref()
            .ok_or(PresentationFramegraphError::MissingGpuContext)
    }

    pub fn profiler_mut(&mut self) -> Result<&mut GpuPassProfiler, PresentationFramegraphError> {
        self.profiler
            .as_mut()
            .ok_or(PresentationFramegraphError::MissingGpuContext)
    }

    pub fn encoder_and_profiler_mut(
        &mut self,
    ) -> Result<(&mut wgpu::CommandEncoder, &mut GpuPassProfiler), PresentationFramegraphError>
    {
        let encoder = self
            .encoder
            .as_mut()
            .ok_or(PresentationFramegraphError::MissingGpuContext)?;
        let profiler = self
            .profiler
            .as_mut()
            .ok_or(PresentationFramegraphError::MissingGpuContext)?;
        Ok((encoder, profiler))
    }

    pub fn profiler_record_count(&self) -> Result<usize, PresentationFramegraphError> {
        Ok(self.profiler()?.record_count())
    }

    pub fn document_exception(&mut self, label: impl Into<SmolStr>) {
        self.documented_exceptions.push(label.into());
    }

    pub fn schedule_readback(
        &mut self,
        source: &wgpu::Buffer,
        request: ReadbackRequest,
    ) -> Result<(), PresentationFramegraphError> {
        let native = self.native()?.clone();
        let ticket = {
            let encoder = self
                .encoder
                .as_mut()
                .ok_or(PresentationFramegraphError::MissingGpuContext)?;
            schedule_storage_buffer_readback(&native.device, encoder, source, request)
        };
        self.readbacks.push(ticket);
        Ok(())
    }

    pub fn schedule_attachment_readback(
        &mut self,
        attachment: &str,
    ) -> Result<(), PresentationFramegraphError> {
        let (buffer, reason, size) = {
            let slot = self.attachments.attachment(attachment).ok_or_else(|| {
                PresentationFramegraphError::Runtime(format!("missing attachment '{attachment}'"))
            })?;
            let buffer = slot.gpu_buffer().ok_or_else(|| {
                PresentationFramegraphError::Runtime(format!(
                    "attachment '{attachment}' is not GPU-backed"
                ))
            })?;
            (
                buffer.clone(),
                slot.readback_reason(),
                slot.layout.total_size as u64,
            )
        };
        self.schedule_readback(
            &buffer,
            ReadbackRequest::new(
                reason,
                format!("wrela.presentation.readback.{attachment}"),
                size,
            ),
        )
    }

    pub fn submit_segment(
        &mut self,
    ) -> Result<PresentationFramegraphSubmission, PresentationFramegraphError> {
        let native = self.native()?.clone();
        let mut encoder = self
            .encoder
            .take()
            .ok_or(PresentationFramegraphError::MissingGpuContext)?;
        let profiler = self
            .profiler
            .take()
            .ok_or(PresentationFramegraphError::MissingGpuContext)?;
        profiler.resolve_into(&mut encoder);
        let timing_ticket = profiler.schedule_readback(&native.device, &mut encoder);
        native.queue.submit(Some(encoder.finish()));
        self.queue_submit_count = self.queue_submit_count.saturating_add(1);

        let mut readbacks = Vec::new();
        for ticket in self.readbacks.drain(..) {
            readbacks.push(
                collect_storage_buffer_readback(&native, ticket)
                    .map_err(PresentationFramegraphError::Runtime)?,
            );
        }
        let timing_bytes = timing_ticket
            .map(|ticket| {
                collect_storage_buffer_readback(&native, ticket)
                    .map_err(PresentationFramegraphError::Runtime)
            })
            .transpose()?
            .map(|result| result.bytes)
            .unwrap_or_default();
        let timestamps_supported = profiler.timestamps_supported();
        let gpu_elapsed_micros = profiler.decode_elapsed_micros(&timing_bytes);
        let documented_exceptions = std::mem::take(&mut self.documented_exceptions);
        self.encoder = Some(native.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor {
                label: Some("wrela.presentation.framegraph.encoder"),
            },
        ));
        self.profiler = Some(GpuPassProfiler::new(&native, self.max_timestamped_passes));
        Ok(PresentationFramegraphSubmission {
            readbacks,
            gpu_elapsed_micros,
            timestamps_supported,
            documented_exceptions,
            queue_submit_count: self.queue_submit_count,
        })
    }

    pub fn take_documented_exceptions(&mut self) -> Vec<SmolStr> {
        std::mem::take(&mut self.documented_exceptions)
    }

    pub fn queue_submit_count(&self) -> u32 {
        self.queue_submit_count
    }
}
