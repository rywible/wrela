use crate::gpu_runtime::{GpuRuntimeContext, upload::normalize_buffer_size};
use smol_str::SmolStr;
use std::sync::mpsc;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ReadbackReason {
    Attachment { attachment: SmolStr },
    DebugExport { artifact: SmolStr },
    GpuTiming,
    QueryResult,
    Custom(SmolStr),
}

impl ReadbackReason {
    pub fn label(&self) -> SmolStr {
        match self {
            Self::Attachment { attachment } => SmolStr::new(format!("attachment:{attachment}")),
            Self::DebugExport { artifact } => SmolStr::new(format!("debug-export:{artifact}")),
            Self::GpuTiming => SmolStr::new("gpu-timing"),
            Self::QueryResult => SmolStr::new("query-result"),
            Self::Custom(label) => label.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ReadbackRequest {
    pub reason: ReadbackReason,
    pub label: SmolStr,
    pub size_bytes: u64,
}

impl ReadbackRequest {
    pub fn new(reason: ReadbackReason, label: impl Into<SmolStr>, size_bytes: u64) -> Self {
        Self {
            reason,
            label: label.into(),
            size_bytes,
        }
    }

    pub fn normalized_size_bytes(&self) -> u64 {
        normalize_buffer_size(self.size_bytes)
    }
}

#[derive(Debug)]
pub struct ReadbackTicket {
    pub request: ReadbackRequest,
    pub readback_buffer: wgpu::Buffer,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadbackResult {
    pub request: ReadbackRequest,
    pub bytes: Vec<u8>,
}

pub fn schedule_storage_buffer_readback(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    source: &wgpu::Buffer,
    request: ReadbackRequest,
) -> ReadbackTicket {
    let readback_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(&request.label),
        size: request.normalized_size_bytes(),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    if request.size_bytes > 0 {
        encoder.copy_buffer_to_buffer(source, 0, &readback_buffer, 0, request.size_bytes);
    }
    ReadbackTicket {
        request,
        readback_buffer,
    }
}

pub fn collect_storage_buffer_readback(
    context: &GpuRuntimeContext,
    ticket: ReadbackTicket,
) -> Result<ReadbackResult, String> {
    if ticket.request.size_bytes == 0 {
        return Ok(ReadbackResult {
            request: ticket.request,
            bytes: Vec::new(),
        });
    }

    let slice = ticket.readback_buffer.slice(..ticket.request.size_bytes);
    let (tx, rx): (
        mpsc::Sender<Result<(), wgpu::BufferAsyncError>>,
        mpsc::Receiver<Result<(), wgpu::BufferAsyncError>>,
    ) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    context
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|err| format!("device poll failed: {err}"))?;
    match rx.recv() {
        Ok(Ok(())) => {}
        Ok(Err(err)) => {
            return Err(format!("readback '{}' failed: {err}", ticket.request.label));
        }
        Err(err) => {
            return Err(format!(
                "readback '{}' channel failed: {err}",
                ticket.request.label
            ));
        }
    }

    let bytes = slice.get_mapped_range().to_vec();
    let _ = slice;
    ticket.readback_buffer.unmap();
    Ok(ReadbackResult {
        request: ticket.request,
        bytes,
    })
}

pub fn collect_storage_buffer_readback_bytes(
    context: &GpuRuntimeContext,
    ticket: ReadbackTicket,
) -> Result<Vec<u8>, String> {
    collect_storage_buffer_readback(context, ticket).map(|result| result.bytes)
}
