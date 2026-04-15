use crate::gpu_runtime::device::GpuLimitRequest;
use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, OnceLock};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadError {
    MissingScratchEncoder,
    MisalignedOffset { offset: u64, alignment: u64 },
}

pub const fn align_up(value: u64, alignment: u64) -> u64 {
    if alignment == 0 {
        value
    } else {
        let remainder = value % alignment;
        if remainder == 0 {
            value
        } else {
            value + (alignment - remainder)
        }
    }
}

pub const fn align_copy_buffer_size(size: u64) -> u64 {
    align_up(size, wgpu::COPY_BUFFER_ALIGNMENT)
}

pub const fn normalize_buffer_size(size: u64) -> u64 {
    if size == 0 { 1 } else { size }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BufferPoolKey {
    pub size: u64,
    pub usage: wgpu::BufferUsages,
}

impl BufferPoolKey {
    pub const fn new(size: u64, usage: wgpu::BufferUsages) -> Self {
        Self {
            size: normalize_buffer_size(size),
            usage,
        }
    }
}

#[derive(Debug, Default)]
pub struct GpuBufferPool {
    free: HashMap<BufferPoolKey, Vec<wgpu::Buffer>>,
}

impl GpuBufferPool {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.free.is_empty()
    }

    pub fn len(&self) -> usize {
        self.free.values().map(Vec::len).sum()
    }

    pub fn acquire(
        &mut self,
        device: &wgpu::Device,
        key: BufferPoolKey,
        label: Option<&str>,
    ) -> (wgpu::Buffer, bool) {
        if let Some(buffers) = self.free.get_mut(&key)
            && let Some(buffer) = buffers.pop()
        {
            return (buffer, false);
        }
        (
            device.create_buffer(&wgpu::BufferDescriptor {
                label,
                size: key.size,
                usage: key.usage,
                mapped_at_creation: false,
            }),
            true,
        )
    }

    pub fn release(&mut self, key: BufferPoolKey, buffer: wgpu::Buffer) {
        self.free.entry(key).or_default().push(buffer);
    }

    pub fn clear(&mut self) {
        self.free.clear();
    }
}

pub fn shared_buffer_pool(request: GpuLimitRequest) -> &'static Mutex<GpuBufferPool> {
    static POOLS: OnceLock<Mutex<HashMap<GpuLimitRequest, &'static Mutex<GpuBufferPool>>>> =
        OnceLock::new();
    let registry = POOLS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = registry.lock().unwrap_or_else(|poison| poison.into_inner());
    guard
        .entry(request)
        .or_insert_with(|| Box::leak(Box::new(Mutex::new(GpuBufferPool::new()))))
}

pub fn lock_shared_upload_arena(
    request: GpuLimitRequest,
    device: &wgpu::Device,
    chunk_size: u64,
) -> MutexGuard<'static, FrameUploadArena> {
    static ARENAS: OnceLock<Mutex<HashMap<GpuLimitRequest, &'static Mutex<FrameUploadArena>>>> =
        OnceLock::new();
    let registry = ARENAS.get_or_init(|| Mutex::new(HashMap::new()));
    let arena_mutex = {
        let mut guard = registry.lock().unwrap_or_else(|poison| poison.into_inner());
        *guard.entry(request).or_insert_with(|| {
            Box::leak(Box::new(Mutex::new(FrameUploadArena::new(
                device, chunk_size,
            ))))
        })
    };
    let mut arena = arena_mutex
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    if arena.chunk_size() < chunk_size {
        *arena = FrameUploadArena::new(device, chunk_size);
    }
    arena
}

pub struct FrameUploadArena {
    staging_belt: wgpu::util::StagingBelt,
    scratch_encoder: Option<wgpu::CommandEncoder>,
    chunk_size: u64,
}

impl FrameUploadArena {
    pub fn new(device: &wgpu::Device, chunk_size: u64) -> Self {
        Self {
            staging_belt: wgpu::util::StagingBelt::new(device.clone(), chunk_size),
            scratch_encoder: None,
            chunk_size,
        }
    }

    pub fn with_encoder(
        device: &wgpu::Device,
        chunk_size: u64,
        encoder: wgpu::CommandEncoder,
    ) -> Self {
        Self {
            staging_belt: wgpu::util::StagingBelt::new(device.clone(), chunk_size),
            scratch_encoder: Some(encoder),
            chunk_size,
        }
    }

    pub fn chunk_size(&self) -> u64 {
        self.chunk_size
    }

    pub fn scratch_encoder(&mut self) -> Option<&mut wgpu::CommandEncoder> {
        self.scratch_encoder.as_mut()
    }

    pub fn ensure_scratch_encoder<'a>(
        &'a mut self,
        device: &wgpu::Device,
        label: Option<&str>,
    ) -> &'a mut wgpu::CommandEncoder {
        self.scratch_encoder.get_or_insert_with(|| {
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label })
        })
    }

    pub fn set_scratch_encoder(&mut self, encoder: wgpu::CommandEncoder) {
        self.scratch_encoder = Some(encoder);
    }

    pub fn write_storage_bytes(
        &mut self,
        target: &wgpu::Buffer,
        offset: u64,
        bytes: &[u8],
    ) -> Result<u64, UploadError> {
        if bytes.is_empty() {
            return Ok(0);
        }
        if offset % wgpu::COPY_BUFFER_ALIGNMENT != 0 {
            return Err(UploadError::MisalignedOffset {
                offset,
                alignment: wgpu::COPY_BUFFER_ALIGNMENT,
            });
        }
        let padded_size = align_copy_buffer_size(bytes.len() as u64);
        let Some(encoder) = self.scratch_encoder.as_mut() else {
            return Err(UploadError::MissingScratchEncoder);
        };
        let mut view = self.staging_belt.write_buffer(
            encoder,
            target,
            offset,
            wgpu::BufferSize::new(padded_size).expect("aligned upload size"),
        );
        if padded_size == bytes.len() as u64 {
            view.copy_from_slice(bytes);
        } else {
            let mut padded = vec![0u8; padded_size as usize];
            padded[..bytes.len()].copy_from_slice(bytes);
            view.copy_from_slice(&padded);
        }
        Ok(padded_size)
    }

    pub fn finish(&mut self) -> Option<wgpu::CommandBuffer> {
        self.staging_belt.finish();
        self.scratch_encoder
            .take()
            .map(wgpu::CommandEncoder::finish)
    }

    pub fn recall(&mut self) {
        self.staging_belt.recall();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn align_up_rounds_up_to_next_multiple() {
        assert_eq!(align_up(0, 4), 0);
        assert_eq!(align_up(1, 4), 4);
        assert_eq!(align_up(4, 4), 4);
        assert_eq!(align_up(5, 4), 8);
    }

    #[test]
    fn copy_buffer_size_alignment_matches_alignment_rules() {
        assert_eq!(align_copy_buffer_size(0), 0);
        assert_eq!(align_copy_buffer_size(1), wgpu::COPY_BUFFER_ALIGNMENT);
        assert_eq!(
            align_copy_buffer_size(wgpu::COPY_BUFFER_ALIGNMENT),
            wgpu::COPY_BUFFER_ALIGNMENT
        );
    }

    #[test]
    fn buffer_pool_key_normalizes_zero_size() {
        let key = BufferPoolKey::new(0, wgpu::BufferUsages::COPY_DST);
        assert_eq!(key.size, 1);
        assert_eq!(key.usage, wgpu::BufferUsages::COPY_DST);
    }
}
