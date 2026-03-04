use crate::lipschitz::types::BoundProvenance;

/// Metadata stored per brick for budget marching.
#[derive(Debug, Copy, Clone)]
pub struct BrickBudgetMeta {
    /// Region-valid anisotropic derivative bound for this brick.
    pub b_max: [f32; 3],
    /// Scalar Lipschitz bound (for LCP).
    pub l_max: f32,
    /// Provenance of the bounds.
    pub provenance: BoundProvenance,
    /// AABB minimum corner.
    pub aabb_min: [f32; 3],
    /// AABB maximum corner.
    pub aabb_max: [f32; 3],
}

/// Budget march result.
#[derive(Debug, Copy, Clone)]
pub struct BudgetMarchResult {
    /// Total distance advanced along ray.
    pub t_advance: f32,
    /// Remaining budget (0 = fully spent).
    pub budget_remaining: f32,
    /// Number of bricks traversed.
    pub bricks_traversed: u32,
    /// True if traversal hit Unknown provenance.
    pub stopped_by_unknown: bool,
}
