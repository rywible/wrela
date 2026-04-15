use crate::gpu_runtime::device::{GpuRuntimeContext, readback_storage_buffer_on};

#[derive(Debug, Clone)]
struct GpuPassRecord {
    start_query: u32,
    end_query: u32,
}

#[derive(Debug, Clone)]
pub struct GpuPassProfiler {
    timestamps_supported: bool,
    query_set: Option<wgpu::QuerySet>,
    resolve_buffer: Option<wgpu::Buffer>,
    timestamp_period_ns: f64,
    query_capacity: u32,
    next_query: u32,
    records: Vec<GpuPassRecord>,
}

impl GpuPassProfiler {
    pub fn new(context: &GpuRuntimeContext, max_timestamped_passes: u32) -> Self {
        let timestamps_supported = context.timestamps_supported();
        let query_capacity = max_timestamped_passes.saturating_mul(2).max(2);
        let (query_set, resolve_buffer) = if timestamps_supported {
            let query_set = context.device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("wrela.gpu_runtime.timestamps"),
                ty: wgpu::QueryType::Timestamp,
                count: query_capacity,
            });
            let resolve_size = (u64::from(query_capacity) * u64::from(wgpu::QUERY_SIZE))
                .next_multiple_of(wgpu::QUERY_RESOLVE_BUFFER_ALIGNMENT);
            let resolve_buffer = context.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("wrela.gpu_runtime.timestamp_resolve"),
                size: resolve_size,
                usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });
            (Some(query_set), Some(resolve_buffer))
        } else {
            (None, None)
        };
        Self {
            timestamps_supported,
            query_set,
            resolve_buffer,
            timestamp_period_ns: f64::from(context.queue.get_timestamp_period()),
            query_capacity,
            next_query: 0,
            records: Vec::new(),
        }
    }

    pub fn timestamps_supported(&self) -> bool {
        self.timestamps_supported
    }

    pub fn compute_pass_timestamp_writes<'a>(
        &'a mut self,
    ) -> Option<wgpu::ComputePassTimestampWrites<'a>> {
        if !self.timestamps_supported || self.next_query + 1 >= self.query_capacity {
            return None;
        }
        let query_set = self.query_set.as_ref()?;
        let start_query = self.next_query;
        let end_query = start_query + 1;
        self.next_query += 2;
        self.records.push(GpuPassRecord {
            start_query,
            end_query,
        });
        Some(wgpu::ComputePassTimestampWrites {
            query_set,
            beginning_of_pass_write_index: Some(start_query),
            end_of_pass_write_index: Some(end_query),
        })
    }

    pub fn resolve_into(&self, encoder: &mut wgpu::CommandEncoder) {
        if let (Some(query_set), Some(resolve_buffer)) = (&self.query_set, &self.resolve_buffer)
            && self.next_query > 0
        {
            encoder.resolve_query_set(query_set, 0..self.next_query, resolve_buffer, 0);
        }
    }

    pub fn readback_gpu_elapsed_micros(
        &self,
        context: &GpuRuntimeContext,
    ) -> Result<Vec<u128>, String> {
        if !self.timestamps_supported || self.records.is_empty() {
            return Ok(Vec::new());
        }
        let Some(resolve_buffer) = &self.resolve_buffer else {
            return Ok(Vec::new());
        };
        let bytes =
            readback_storage_buffer_on(context, resolve_buffer, u64::from(self.next_query) * 8)?;
        let mut elapsed = Vec::with_capacity(self.records.len());
        for record in &self.records {
            let start = read_u64(&bytes, record.start_query as usize);
            let end = read_u64(&bytes, record.end_query as usize);
            let diff_ns = end.saturating_sub(start) as f64 * self.timestamp_period_ns;
            let diff_us = (diff_ns / 1_000.0).max(0.0).round() as u128;
            elapsed.push(diff_us);
        }
        Ok(elapsed)
    }
}

fn read_u64(bytes: &[u8], index: usize) -> u64 {
    let start = index.saturating_mul(std::mem::size_of::<u64>());
    let end = start + std::mem::size_of::<u64>();
    bytes
        .get(start..end)
        .and_then(|slice| slice.try_into().ok())
        .map(u64::from_le_bytes)
        .unwrap_or_default()
}
