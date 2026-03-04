/// Provenance of derivative bounds — fail-closed to scalar L when not region-valid.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum BoundProvenance {
    /// Derived from analytic formula valid over declared region.
    Analytic,
    /// Derived from Bernstein basis convex hull property over lattice cell.
    /// Region-valid by construction — guaranteed convex-hull bounds; tightenable via subdivision.
    AnalyticBernstein,
    /// Computed from worst-case of sampled Jacobians over region grid.
    Sampled,
    /// Analytically derived but inflated by safety factor (noise bounds).
    Inflated,
    /// Provenance not established (user-supplied assume_bound, etc.).
    Unknown,
}

impl BoundProvenance {
    /// Returns true if this provenance is region-valid (safe for marching kernel).
    pub fn is_region_valid(&self) -> bool {
        !matches!(self, BoundProvenance::Unknown)
    }
}

/// Type-safe region-valid derivative bound. Cannot be constructed from Unknown provenance.
/// The marching kernel accepts this type, not raw `[f32; 3]`, preventing "local B as cell B" bugs.
#[derive(Debug, Copy, Clone)]
pub struct RegionValidBound {
    pub b: [f32; 3],
    pub provenance: BoundProvenance,
}

impl RegionValidBound {
    /// Construct from analytically derived bounds.
    pub fn from_analytic(b: [f32; 3], bernstein: bool) -> Self {
        assert!(b[0] >= 0.0 && b[1] >= 0.0 && b[2] >= 0.0, "bounds must be non-negative");
        Self {
            b,
            provenance: if bernstein {
                BoundProvenance::AnalyticBernstein
            } else {
                BoundProvenance::Analytic
            },
        }
    }

    /// Construct from worst-case sampled Jacobians over region grid.
    pub fn from_sampled(b: [f32; 3]) -> Self {
        assert!(b[0] >= 0.0 && b[1] >= 0.0 && b[2] >= 0.0, "bounds must be non-negative");
        Self {
            b,
            provenance: BoundProvenance::Sampled,
        }
    }

    /// Inflate a point-valid bound to region-valid by adding safety margin.
    pub fn inflate_point_to_region(point_b: [f32; 3], safety_factor: f32) -> Self {
        assert!(safety_factor >= 1.0, "safety factor must be >= 1.0");
        assert!(
            point_b[0] >= 0.0 && point_b[1] >= 0.0 && point_b[2] >= 0.0,
            "bounds must be non-negative"
        );
        Self {
            b: [
                point_b[0] * safety_factor,
                point_b[1] * safety_factor,
                point_b[2] * safety_factor,
            ],
            provenance: BoundProvenance::Inflated,
        }
    }
}

/// A bounded implicit field value with conservative tracking.
/// Every field computation produces one of these.
#[derive(Debug, Copy, Clone)]
pub struct FieldSample {
    /// Sampled distance-like value.
    pub distance: f32,
    /// Region-valid anisotropic derivative bound. None when provenance is Unknown
    /// (forces scalar L fallback).
    pub region_bound: Option<RegionValidBound>,
    /// Scalar Lipschitz bound L (Euclidean). For LCP, must be directly propagated
    /// through composition rules, not derived from B.
    pub lipschitz: f32,
    /// Tracks how L was derived.
    pub lipschitz_provenance: BoundProvenance,
    /// Sampling error bound.
    pub epsilon: f32,
}

impl FieldSample {
    /// Convenience: get the anisotropic bound array, or a zero array if absent.
    pub fn dfdxyz_bound(&self) -> [f32; 3] {
        match &self.region_bound {
            Some(rvb) => rvb.b,
            None => [0.0; 3],
        }
    }

    /// Whether this sample has a valid anisotropic bound.
    pub fn has_anisotropic_bound(&self) -> bool {
        self.region_bound.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bound_provenance_region_valid() {
        assert!(BoundProvenance::Analytic.is_region_valid());
        assert!(BoundProvenance::AnalyticBernstein.is_region_valid());
        assert!(BoundProvenance::Sampled.is_region_valid());
        assert!(BoundProvenance::Inflated.is_region_valid());
        assert!(!BoundProvenance::Unknown.is_region_valid());
    }

    #[test]
    fn region_valid_bound_from_analytic() {
        let rvb = RegionValidBound::from_analytic([1.0, 2.0, 3.0], false);
        assert_eq!(rvb.provenance, BoundProvenance::Analytic);
        assert_eq!(rvb.b, [1.0, 2.0, 3.0]);

        let rvb = RegionValidBound::from_analytic([1.0, 2.0, 3.0], true);
        assert_eq!(rvb.provenance, BoundProvenance::AnalyticBernstein);
    }

    #[test]
    fn region_valid_bound_from_sampled() {
        let rvb = RegionValidBound::from_sampled([0.5, 0.5, 0.5]);
        assert_eq!(rvb.provenance, BoundProvenance::Sampled);
    }

    #[test]
    fn region_valid_bound_inflate() {
        let rvb = RegionValidBound::inflate_point_to_region([1.0, 2.0, 3.0], 1.5);
        assert_eq!(rvb.provenance, BoundProvenance::Inflated);
        assert_eq!(rvb.b, [1.5, 3.0, 4.5]);
    }

    #[test]
    #[should_panic(expected = "bounds must be non-negative")]
    fn reject_negative_bounds() {
        RegionValidBound::from_analytic([-1.0, 0.0, 0.0], false);
    }

    #[test]
    #[should_panic(expected = "safety factor must be >= 1.0")]
    fn reject_low_safety_factor() {
        RegionValidBound::inflate_point_to_region([1.0, 1.0, 1.0], 0.5);
    }

    #[test]
    fn field_sample_accessors() {
        let sample = FieldSample {
            distance: 1.0,
            region_bound: Some(RegionValidBound::from_analytic([1.0, 1.0, 1.0], false)),
            lipschitz: 1.73,
            lipschitz_provenance: BoundProvenance::Analytic,
            epsilon: 0.001,
        };
        assert!(sample.has_anisotropic_bound());
        assert_eq!(sample.dfdxyz_bound(), [1.0, 1.0, 1.0]);

        let sample_no_bound = FieldSample {
            distance: 1.0,
            region_bound: None,
            lipschitz: 1.0,
            lipschitz_provenance: BoundProvenance::Unknown,
            epsilon: 0.0,
        };
        assert!(!sample_no_bound.has_anisotropic_bound());
        assert_eq!(sample_no_bound.dfdxyz_bound(), [0.0, 0.0, 0.0]);
    }
}
