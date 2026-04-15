use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GpuRuntimeMetrics {
    pub timestamps_supported: bool,
    pub timestamped_pass_count: u32,
    pub gpu_time_total_micros: u128,
    pub gpu_time_max_micros: u128,
    pub queue_submit_count: u32,
    pub transient_buffer_creations: u32,
    pub transient_bind_group_creations: u32,
    pub upload_bytes: u64,
    pub readback_bytes: u64,
    pub cpu_screen_sample_allocations: u32,
    pub attachment_decode_count: u32,
    pub attachment_encode_count: u32,
    pub primary_visibility_packet_fanout_count: u32,
    pub dispatch_fragmentation_count: u32,
    pub scene_reupload_bytes: u64,
    pub pipeline_cache_hits: u32,
    pub pipeline_cache_misses: u32,
}

impl GpuRuntimeMetrics {
    pub fn merge_from(&mut self, other: &Self) {
        self.timestamps_supported |= other.timestamps_supported;
        self.timestamped_pass_count = self
            .timestamped_pass_count
            .saturating_add(other.timestamped_pass_count);
        self.gpu_time_total_micros = self
            .gpu_time_total_micros
            .saturating_add(other.gpu_time_total_micros);
        self.gpu_time_max_micros = self.gpu_time_max_micros.max(other.gpu_time_max_micros);
        self.queue_submit_count = self
            .queue_submit_count
            .saturating_add(other.queue_submit_count);
        self.transient_buffer_creations = self
            .transient_buffer_creations
            .saturating_add(other.transient_buffer_creations);
        self.transient_bind_group_creations = self
            .transient_bind_group_creations
            .saturating_add(other.transient_bind_group_creations);
        self.upload_bytes = self.upload_bytes.saturating_add(other.upload_bytes);
        self.readback_bytes = self.readback_bytes.saturating_add(other.readback_bytes);
        self.cpu_screen_sample_allocations = self
            .cpu_screen_sample_allocations
            .saturating_add(other.cpu_screen_sample_allocations);
        self.attachment_decode_count = self
            .attachment_decode_count
            .saturating_add(other.attachment_decode_count);
        self.attachment_encode_count = self
            .attachment_encode_count
            .saturating_add(other.attachment_encode_count);
        self.primary_visibility_packet_fanout_count = self
            .primary_visibility_packet_fanout_count
            .saturating_add(other.primary_visibility_packet_fanout_count);
        self.dispatch_fragmentation_count = self
            .dispatch_fragmentation_count
            .saturating_add(other.dispatch_fragmentation_count);
        self.scene_reupload_bytes = self
            .scene_reupload_bytes
            .saturating_add(other.scene_reupload_bytes);
        self.pipeline_cache_hits = self
            .pipeline_cache_hits
            .saturating_add(other.pipeline_cache_hits);
        self.pipeline_cache_misses = self
            .pipeline_cache_misses
            .saturating_add(other.pipeline_cache_misses);
    }

    pub fn note_gpu_timings(&mut self, timestamps_supported: bool, pass_elapsed_micros: &[u128]) {
        self.timestamps_supported |= timestamps_supported;
        self.timestamped_pass_count = self
            .timestamped_pass_count
            .saturating_add(pass_elapsed_micros.len() as u32);
        for elapsed in pass_elapsed_micros {
            self.gpu_time_total_micros = self.gpu_time_total_micros.saturating_add(*elapsed);
            self.gpu_time_max_micros = self.gpu_time_max_micros.max(*elapsed);
        }
    }
}

pub fn classify_execution_bound(
    cpu_time_total_micros: u128,
    metrics: &GpuRuntimeMetrics,
    pass_count: usize,
) -> &'static str {
    let gpu_time_total_micros = metrics.gpu_time_total_micros;
    let cpu_overhead_micros = cpu_time_total_micros.saturating_sub(gpu_time_total_micros);
    if metrics.timestamps_supported
        && gpu_time_total_micros > 0
        && gpu_time_total_micros >= cpu_overhead_micros
    {
        return "gpu_bound";
    }
    if metrics.dispatch_fragmentation_count > 0
        || metrics.primary_visibility_packet_fanout_count > pass_count as u32
    {
        return "dispatch_fragmentation";
    }
    if metrics.readback_bytes > 0
        && (cpu_overhead_micros > gpu_time_total_micros / 2
            || metrics.queue_submit_count > pass_count as u32)
    {
        return "cpu_submission_readback_bound";
    }
    if metrics.cpu_screen_sample_allocations > 0
        || metrics.attachment_decode_count > 0
        || metrics.attachment_encode_count > 0
    {
        return "cpu_primary_setup";
    }
    "cpu_wall_clock_only"
}
