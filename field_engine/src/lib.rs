pub mod lipschitz;
pub mod lcp;
pub mod spacetime;
pub mod f16;
pub mod noise;
pub mod budget_march;
pub mod quality;
pub mod determinism;
pub mod blue_noise;

// Re-exports for convenience
pub use lipschitz::types::{BoundProvenance, FieldSample, RegionValidBound};
pub use lipschitz::stepping::{safe_step_from_lower_bound, safe_step_from_sample};
pub use lipschitz::composition::*;
pub use lipschitz::anisotropic::{bound_add, bound_chain, bound_max};
pub use lipschitz::envelope::{lipschitz_envelope, lipschitz_upper_envelope, separable_lipschitz_closure_1d};
pub use lipschitz::whitney::whitney_c1_envelope;
pub use lipschitz::cone_union::cone_union_safe_step;

pub use lcp::dual_envelope::dual_envelope_lower_bound;
pub use lcp::fused_stepping::fused_directional_bound;
pub use lcp::mixed_cone_union::mixed_cone_union_safe_step;

pub use spacetime::types::FieldSample4;
pub use spacetime::stepping::safe_step_spacetime_along_path;
pub use spacetime::rigid_body::compute_bt_rigid;
pub use spacetime::swept_volume::swept_volume_lower_bound;

pub use f16::convert::{BoundDirection, f16_conservative, f16_from_f32_rne, f16_next_down, f16_next_up, f16_to_f32};

pub use noise::perlin::evaluate_improved_perlin;
pub use noise::bernstein::{CertifiedNoisePatch, certify_noise_patch};
pub use noise::fbm::{FbmResult, fbm_frequency_bounded};

pub use budget_march::types::{BrickBudgetMeta, BudgetMarchResult};
pub use budget_march::traverse::{budget_march_traverse, ray_aabb_exit_distance};

pub use quality::profiles::{GiMode, QualityProfile, ShadowMode};
pub use quality::budget::GpuMemoryBudget;

pub use determinism::seed::DeterminismState;
pub use determinism::replay::{RegionHash, ReplayFrame, ReplayRecording};

pub use blue_noise::service::BlueNoiseService;
