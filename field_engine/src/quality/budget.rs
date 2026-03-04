/// GPU memory budget constants (in megabytes).
/// Target: 512 MB total GPU memory budget for Low/Medium/High.

#[derive(Debug, Copy, Clone)]
pub struct GpuMemoryBudget {
    pub brick_pool_distance_mb: u32,
    pub brick_pool_fields_mb: u32,
    pub radiance_mip_chain_mb: u32,
    pub page_tables_metadata_mb: u32,
    pub dual_contour_meshes_mb: u32,
    pub character_volumes_mb: u32,
    pub neural_field_weights_mb: u32,
    pub render_targets_mb: u32,
    pub post_process_buffers_mb: u32,
    pub blue_noise_textures_mb: u32,
    pub uniform_storage_buffers_mb: u32,
}

impl GpuMemoryBudget {
    pub fn total_mb(&self) -> u32 {
        self.brick_pool_distance_mb
            + self.brick_pool_fields_mb
            + self.radiance_mip_chain_mb
            + self.page_tables_metadata_mb
            + self.dual_contour_meshes_mb
            + self.character_volumes_mb
            + self.neural_field_weights_mb
            + self.render_targets_mb
            + self.post_process_buffers_mb
            + self.blue_noise_textures_mb
            + self.uniform_storage_buffers_mb
    }
}

pub const LOW_BUDGET: GpuMemoryBudget = GpuMemoryBudget {
    brick_pool_distance_mb: 20,
    brick_pool_fields_mb: 2,
    radiance_mip_chain_mb: 2,
    page_tables_metadata_mb: 2,
    dual_contour_meshes_mb: 4,
    character_volumes_mb: 5,
    neural_field_weights_mb: 1,
    render_targets_mb: 32,
    post_process_buffers_mb: 16,
    blue_noise_textures_mb: 1,
    uniform_storage_buffers_mb: 4,
};

pub const MEDIUM_BUDGET: GpuMemoryBudget = GpuMemoryBudget {
    brick_pool_distance_mb: 96,
    brick_pool_fields_mb: 2,
    radiance_mip_chain_mb: 4,
    page_tables_metadata_mb: 4,
    dual_contour_meshes_mb: 8,
    character_volumes_mb: 10,
    neural_field_weights_mb: 2,
    render_targets_mb: 48,
    post_process_buffers_mb: 24,
    blue_noise_textures_mb: 1,
    uniform_storage_buffers_mb: 8,
};

pub const HIGH_BUDGET: GpuMemoryBudget = GpuMemoryBudget {
    brick_pool_distance_mb: 160,
    brick_pool_fields_mb: 4,
    radiance_mip_chain_mb: 8,
    page_tables_metadata_mb: 8,
    dual_contour_meshes_mb: 16,
    character_volumes_mb: 20,
    neural_field_weights_mb: 4,
    render_targets_mb: 64,
    post_process_buffers_mb: 32,
    blue_noise_textures_mb: 1,
    uniform_storage_buffers_mb: 16,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budgets_under_512mb() {
        assert!(
            LOW_BUDGET.total_mb() <= 512,
            "Low budget {} MB > 512 MB",
            LOW_BUDGET.total_mb()
        );
        assert!(
            MEDIUM_BUDGET.total_mb() <= 512,
            "Medium budget {} MB > 512 MB",
            MEDIUM_BUDGET.total_mb()
        );
        assert!(
            HIGH_BUDGET.total_mb() <= 512,
            "High budget {} MB > 512 MB",
            HIGH_BUDGET.total_mb()
        );
    }

    #[test]
    fn budgets_match_plan_totals() {
        assert_eq!(LOW_BUDGET.total_mb(), 89);
        assert_eq!(MEDIUM_BUDGET.total_mb(), 207);
        assert_eq!(HIGH_BUDGET.total_mb(), 333);
    }
}
