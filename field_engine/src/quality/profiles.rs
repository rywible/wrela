/// Shadow rendering mode.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ShadowMode {
    /// Low quality: use rasterized mesh shadows.
    RasterMesh,
    /// High quality: trace shadows through the field.
    FieldTrace,
}

/// Global illumination mode.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum GiMode {
    /// SH probe lookup only.
    ProbesOnly,
    /// SH probes + per-pixel cone tracing through radiance mips.
    ProbesPlusConeTrace,
    /// SH probes + cone tracing + irradiance PDE.
    ProbesPlusConeTracePlusPDE,
}

/// Quality profile controlling rendering fidelity vs performance.
#[derive(Debug, Clone)]
pub struct QualityProfile {
    // Ray marching
    pub ray_march_resolution_scale: f32,
    pub max_ray_steps: u32,

    // Brick pool
    pub brick_resolution: u32,
    pub max_resident_bricks: u32,
    pub brick_update_budget_ms: f32,
    pub clipmap_levels: u32,

    // Shadows
    pub shadow_mode: ShadowMode,
    pub shadow_cascade_count: u32,

    // Characters
    pub character_brick_resolution: u32,
    pub character_update_frequency: u32,

    // GI
    pub gi_mode: GiMode,
    pub gi_resolution_scale: f32,
    pub sh_probe_update_budget: u32,

    // Spectral (Phase 5+)
    pub spectral_basis_dimension: u32,

    // Painterly (Phase 6+)
    pub painterly_resolution_scale: f32,
    pub ink_wash_sim_steps: u32,

    // Simulation (Phase 10+)
    pub sim_update_budget_ms: f32,
    pub sim_clipmap_levels: u32,
}

/// Low quality preset — targeting integrated GPUs.
pub const LOW: QualityProfile = QualityProfile {
    ray_march_resolution_scale: 0.5,
    max_ray_steps: 48,
    brick_resolution: 8,
    max_resident_bricks: 1024,
    brick_update_budget_ms: 1.0,
    clipmap_levels: 3,
    shadow_mode: ShadowMode::RasterMesh,
    shadow_cascade_count: 2,
    character_brick_resolution: 32,
    character_update_frequency: 4,
    gi_mode: GiMode::ProbesOnly,
    gi_resolution_scale: 0.25,
    sh_probe_update_budget: 16,
    spectral_basis_dimension: 3,
    painterly_resolution_scale: 0.5,
    ink_wash_sim_steps: 2,
    sim_update_budget_ms: 1.0,
    sim_clipmap_levels: 2,
};

/// Medium quality preset — target for "60fps at 1080p".
pub const MEDIUM: QualityProfile = QualityProfile {
    ray_march_resolution_scale: 0.75,
    max_ray_steps: 80,
    brick_resolution: 12,
    max_resident_bricks: 2048,
    brick_update_budget_ms: 2.0,
    clipmap_levels: 4,
    shadow_mode: ShadowMode::RasterMesh,
    shadow_cascade_count: 3,
    character_brick_resolution: 48,
    character_update_frequency: 2,
    gi_mode: GiMode::ProbesPlusConeTrace,
    gi_resolution_scale: 0.33,
    sh_probe_update_budget: 32,
    spectral_basis_dimension: 4,
    painterly_resolution_scale: 0.75,
    ink_wash_sim_steps: 4,
    sim_update_budget_ms: 2.0,
    sim_clipmap_levels: 3,
};

/// High quality preset.
pub const HIGH: QualityProfile = QualityProfile {
    ray_march_resolution_scale: 1.0,
    max_ray_steps: 112,
    brick_resolution: 16,
    max_resident_bricks: 3072,
    brick_update_budget_ms: 2.5,
    clipmap_levels: 5,
    shadow_mode: ShadowMode::FieldTrace,
    shadow_cascade_count: 4,
    character_brick_resolution: 64,
    character_update_frequency: 1,
    gi_mode: GiMode::ProbesPlusConeTracePlusPDE,
    gi_resolution_scale: 0.5,
    sh_probe_update_budget: 64,
    spectral_basis_dimension: 6,
    painterly_resolution_scale: 1.0,
    ink_wash_sim_steps: 6,
    sim_update_budget_ms: 3.0,
    sim_clipmap_levels: 4,
};

/// Ultra quality preset — stretch target for discrete GPUs.
pub const ULTRA: QualityProfile = QualityProfile {
    ray_march_resolution_scale: 1.0,
    max_ray_steps: 128,
    brick_resolution: 16,
    max_resident_bricks: 4096,
    brick_update_budget_ms: 3.0,
    clipmap_levels: 5,
    shadow_mode: ShadowMode::FieldTrace,
    shadow_cascade_count: 4,
    character_brick_resolution: 64,
    character_update_frequency: 1,
    gi_mode: GiMode::ProbesPlusConeTracePlusPDE,
    gi_resolution_scale: 0.5,
    sh_probe_update_budget: 64,
    spectral_basis_dimension: 6,
    painterly_resolution_scale: 1.0,
    ink_wash_sim_steps: 8,
    sim_update_budget_ms: 3.0,
    sim_clipmap_levels: 4,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_have_valid_ranges() {
        for (name, profile) in [
            ("LOW", &LOW),
            ("MEDIUM", &MEDIUM),
            ("HIGH", &HIGH),
            ("ULTRA", &ULTRA),
        ] {
            assert!(
                profile.ray_march_resolution_scale > 0.0
                    && profile.ray_march_resolution_scale <= 1.0,
                "{name}: invalid ray march scale"
            );
            assert!(profile.max_ray_steps >= 48, "{name}: too few ray steps");
            assert!(profile.brick_resolution >= 8, "{name}: brick res too low");
            assert!(
                profile.clipmap_levels >= 3 && profile.clipmap_levels <= 5,
                "{name}: clipmap levels out of range"
            );
        }
    }

    #[test]
    fn presets_monotonically_increase() {
        assert!(LOW.max_ray_steps <= MEDIUM.max_ray_steps);
        assert!(MEDIUM.max_ray_steps <= HIGH.max_ray_steps);
        assert!(HIGH.max_ray_steps <= ULTRA.max_ray_steps);

        assert!(LOW.max_resident_bricks <= MEDIUM.max_resident_bricks);
        assert!(MEDIUM.max_resident_bricks <= HIGH.max_resident_bricks);
        assert!(HIGH.max_resident_bricks <= ULTRA.max_resident_bricks);
    }
}
