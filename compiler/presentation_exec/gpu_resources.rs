use crate::gpu_runtime::GpuRuntimeContext;
use crate::presentation_contract::FrameContract;
use crate::presentation_exec::resources::{
    AttachmentResource, AttachmentResourceSet, FrameAttachmentLayout, PresentationResourceError,
};
use smol_str::SmolStr;
use std::collections::BTreeMap;
use thiserror::Error;

use crate::gpu_runtime::readback::ReadbackReason;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum GpuAttachmentArenaError {
    #[error("attachment '{attachment}' is still GPU-resident and has no CPU mirror")]
    MissingCpuMirror { attachment: SmolStr },
}

#[derive(Debug, Clone)]
pub enum AttachmentBacking {
    CpuBytes(Vec<u8>),
    GpuBuffer {
        buffer: wgpu::Buffer,
        readback_reason: Option<ReadbackReason>,
    },
}

impl AttachmentBacking {
    pub fn is_cpu_backed(&self) -> bool {
        matches!(self, Self::CpuBytes(_))
    }

    pub fn readback_reason(&self) -> Option<&ReadbackReason> {
        match self {
            Self::CpuBytes(_) => None,
            Self::GpuBuffer {
                readback_reason, ..
            } => readback_reason.as_ref(),
        }
    }

    pub fn gpu_buffer(&self) -> Option<&wgpu::Buffer> {
        match self {
            Self::CpuBytes(_) => None,
            Self::GpuBuffer { buffer, .. } => Some(buffer),
        }
    }
}

#[derive(Debug, Clone)]
pub struct GpuAttachmentSlot {
    pub layout: FrameAttachmentLayout,
    pub backing: AttachmentBacking,
}

impl GpuAttachmentSlot {
    pub fn new_cpu(layout: FrameAttachmentLayout, bytes: Vec<u8>) -> Self {
        Self {
            layout,
            backing: AttachmentBacking::CpuBytes(bytes),
        }
    }

    pub fn new_gpu(
        layout: FrameAttachmentLayout,
        buffer: wgpu::Buffer,
        readback_reason: Option<ReadbackReason>,
    ) -> Self {
        Self {
            layout,
            backing: AttachmentBacking::GpuBuffer {
                buffer,
                readback_reason,
            },
        }
    }

    pub fn name(&self) -> &SmolStr {
        &self.layout.attachment.name
    }

    pub fn readback_reason(&self) -> ReadbackReason {
        self.backing
            .readback_reason()
            .cloned()
            .unwrap_or_else(|| ReadbackReason::Attachment {
                attachment: self.layout.attachment.name.clone(),
            })
    }

    pub fn cpu_bytes(&self) -> Option<&[u8]> {
        match &self.backing {
            AttachmentBacking::CpuBytes(bytes) => Some(bytes.as_slice()),
            AttachmentBacking::GpuBuffer { .. } => None,
        }
    }

    pub fn gpu_buffer(&self) -> Option<&wgpu::Buffer> {
        self.backing.gpu_buffer()
    }
}

#[derive(Debug, Clone, Default)]
pub struct GpuAttachmentArena {
    pub width: u32,
    pub height: u32,
    pub attachments: BTreeMap<SmolStr, GpuAttachmentSlot>,
}

impl GpuAttachmentArena {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            attachments: BTreeMap::new(),
        }
    }

    pub fn from_attachment_resources(resources: &AttachmentResourceSet) -> Self {
        let mut arena = Self::new(resources.width, resources.height);
        for (name, attachment) in &resources.attachments {
            arena.attachments.insert(
                name.clone(),
                GpuAttachmentSlot::new_cpu(attachment.layout.clone(), attachment.bytes.clone()),
            );
        }
        arena
    }

    pub fn from_attachment_resources_gpu(
        context: &GpuRuntimeContext,
        resources: &AttachmentResourceSet,
    ) -> Self {
        let mut arena = Self::new(resources.width, resources.height);
        for (name, attachment) in &resources.attachments {
            let size = attachment.bytes.len().max(4) as u64;
            let buffer = context.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(&format!("wrela.presentation.attachment.{name}")),
                size,
                usage: wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_SRC
                    | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            if !attachment.bytes.is_empty() {
                context.queue.write_buffer(&buffer, 0, &attachment.bytes);
            }
            arena.attachments.insert(
                name.clone(),
                GpuAttachmentSlot::new_gpu(
                    attachment.layout.clone(),
                    buffer,
                    Some(ReadbackReason::Attachment {
                        attachment: name.clone(),
                    }),
                ),
            );
        }
        arena
    }

    pub fn attachment(&self, name: &str) -> Option<&GpuAttachmentSlot> {
        self.attachments.get(name)
    }

    pub fn attachment_mut(&mut self, name: &str) -> Option<&mut GpuAttachmentSlot> {
        self.attachments.get_mut(name)
    }

    pub fn attachment_buffer(&self, name: &str) -> Option<&wgpu::Buffer> {
        self.attachment(name)
            .and_then(GpuAttachmentSlot::gpu_buffer)
    }

    pub fn insert_cpu_attachment(
        &mut self,
        attachment: AttachmentResource,
    ) -> Option<GpuAttachmentSlot> {
        let name = attachment.layout.attachment.name.clone();
        self.attachments.insert(
            name,
            GpuAttachmentSlot::new_cpu(attachment.layout, attachment.bytes),
        )
    }

    pub fn insert_gpu_attachment(
        &mut self,
        layout: FrameAttachmentLayout,
        buffer: wgpu::Buffer,
        readback_reason: Option<ReadbackReason>,
    ) -> Option<GpuAttachmentSlot> {
        let name = layout.attachment.name.clone();
        self.attachments.insert(
            name,
            GpuAttachmentSlot::new_gpu(layout, buffer, readback_reason),
        )
    }

    pub fn materialize_cpu_resources(
        &self,
    ) -> Result<AttachmentResourceSet, GpuAttachmentArenaError> {
        let mut attachments = BTreeMap::new();
        for (name, slot) in &self.attachments {
            let Some(bytes) = slot.cpu_bytes() else {
                return Err(GpuAttachmentArenaError::MissingCpuMirror {
                    attachment: name.clone(),
                });
            };
            attachments.insert(
                name.clone(),
                AttachmentResource {
                    layout: slot.layout.clone(),
                    bytes: bytes.to_vec(),
                },
            );
        }
        Ok(AttachmentResourceSet {
            width: self.width,
            height: self.height,
            attachments,
        })
    }

    pub fn into_cpu_resources(self) -> Result<AttachmentResourceSet, GpuAttachmentArenaError> {
        self.materialize_cpu_resources()
    }

    pub fn attachment_count(&self) -> usize {
        self.attachments.len()
    }

    pub fn is_empty(&self) -> bool {
        self.attachments.is_empty()
    }

    pub fn set_from_frame(
        &mut self,
        frame: &FrameContract,
        width: u32,
        height: u32,
    ) -> Result<(), PresentationResourceError> {
        let resources =
            crate::presentation_exec::resources::allocate_attachment_resources_without_history(
                frame, width, height,
            )?;
        *self = Self::from_attachment_resources(&resources);
        Ok(())
    }
}
