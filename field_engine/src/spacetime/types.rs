use crate::lipschitz::types::BoundProvenance;

/// A field sample extended with time-derivative bound for spacetime operations.
#[derive(Debug, Copy, Clone)]
pub struct FieldSample4 {
    /// Canonical lower bound at (p, t).
    pub b_lower: f32,
    /// Spatial derivative bounds B over region.
    pub dfdxyz_bound: [f32; 3],
    /// Time-derivative bound Bt over region and time interval.
    pub b_time: f32,
    /// True when Bt is region-valid; false => fail closed.
    pub has_spacetime_bound: bool,
    /// Tracks how Bt was derived.
    pub spacetime_provenance: BoundProvenance,
}
