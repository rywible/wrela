use super::types::{BrickBudgetMeta, BudgetMarchResult};
use crate::lipschitz::types::BoundProvenance;

const MAX_BUDGET_TRAVERSALS: u32 = 16;

/// Traverse multiple bricks along a ray using a single conservative budget.
pub fn budget_march_traverse(
    p0: [f32; 3],
    v: [f32; 3],
    b_lower: f32,
    get_brick_meta: &dyn Fn([f32; 3]) -> Option<BrickBudgetMeta>,
    use_lcp: bool,
) -> BudgetMarchResult {
    let mut budget = f32::max(0.0, b_lower);
    let mut t_total = 0.0_f32;
    let mut traversals = 0u32;
    let mut stopped_by_unknown = false;

    while budget > 0.0 && traversals < MAX_BUDGET_TRAVERSALS {
        let p_current = [
            p0[0] + t_total * v[0],
            p0[1] + t_total * v[1],
            p0[2] + t_total * v[2],
        ];

        let meta = match get_brick_meta(p_current) {
            Some(m) => m,
            None => break,
        };

        if matches!(meta.provenance, BoundProvenance::Unknown) {
            stopped_by_unknown = true;
            break;
        }

        let abs_v = [v[0].abs(), v[1].abs(), v[2].abs()];
        let l_dir_l1 =
            abs_v[0] * meta.b_max[0] + abs_v[1] * meta.b_max[1] + abs_v[2] * meta.b_max[2];
        let l_dir = if use_lcp && meta.l_max.is_finite() && meta.l_max > 0.0 {
            f32::min(l_dir_l1, meta.l_max)
        } else {
            l_dir_l1
        };
        let l_safe = f32::max(l_dir, 1e-6);

        let dt_exit = ray_aabb_exit_distance(p_current, v, meta.aabb_min, meta.aabb_max);

        let cost_raw = l_safe * dt_exit;
        let pad_cost = 2.0 * f32::EPSILON * f32::max(cost_raw.abs(), 1.0);
        let cost = cost_raw + pad_cost;

        if cost <= budget {
            t_total += dt_exit;
            budget -= cost;
            traversals += 1;
        } else {
            let dt_partial = budget / l_safe;
            t_total += dt_partial;
            budget = 0.0;
            traversals += 1;
        }
    }

    BudgetMarchResult {
        t_advance: t_total,
        budget_remaining: budget,
        bricks_traversed: traversals,
        stopped_by_unknown,
    }
}

/// Ray-AABB exit distance.
pub fn ray_aabb_exit_distance(
    p: [f32; 3],
    v: [f32; 3],
    aabb_min: [f32; 3],
    aabb_max: [f32; 3],
) -> f32 {
    let mut t_exit = f32::INFINITY;
    for i in 0..3 {
        if v[i].abs() > 1e-12 {
            let t_min = (aabb_min[i] - p[i]) / v[i];
            let t_max = (aabb_max[i] - p[i]) / v[i];
            t_exit = f32::min(t_exit, f32::max(t_min, t_max));
        }
    }
    f32::max(t_exit, 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lipschitz::types::BoundProvenance;

    #[test]
    fn ray_aabb_exit_basic() {
        // Ray from center of unit AABB along +x
        let dist = ray_aabb_exit_distance(
            [0.5, 0.5, 0.5],
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [1.0, 1.0, 1.0],
        );
        assert!((dist - 0.5).abs() < 1e-5);
    }

    #[test]
    fn ray_aabb_exit_diagonal() {
        let dist = ray_aabb_exit_distance(
            [0.5, 0.5, 0.5],
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [2.0, 2.0, 2.0],
        );
        assert!((dist - 1.5).abs() < 1e-5);
    }

    #[test]
    fn budget_march_single_brick() {
        let meta = BrickBudgetMeta {
            b_max: [1.0, 1.0, 1.0],
            l_max: 1.0,
            provenance: BoundProvenance::Analytic,
            aabb_min: [0.0, 0.0, 0.0],
            aabb_max: [1.0, 1.0, 1.0],
        };

        let result = budget_march_traverse(
            [0.5, 0.5, 0.5],
            [1.0, 0.0, 0.0],
            5.0, // budget
            &|_p| Some(meta),
            false,
        );

        assert!(result.t_advance > 0.0);
        assert!(result.bricks_traversed > 0);
        assert!(!result.stopped_by_unknown);
    }

    #[test]
    fn budget_march_unknown_halts() {
        let meta = BrickBudgetMeta {
            b_max: [1.0, 1.0, 1.0],
            l_max: 1.0,
            provenance: BoundProvenance::Unknown,
            aabb_min: [0.0, 0.0, 0.0],
            aabb_max: [1.0, 1.0, 1.0],
        };

        let result = budget_march_traverse(
            [0.5, 0.5, 0.5],
            [1.0, 0.0, 0.0],
            5.0,
            &|_p| Some(meta),
            false,
        );

        assert!(result.stopped_by_unknown);
        assert_eq!(result.bricks_traversed, 0);
    }

    #[test]
    fn budget_march_no_brick() {
        let result = budget_march_traverse(
            [0.5, 0.5, 0.5],
            [1.0, 0.0, 0.0],
            5.0,
            &|_p| None, // no bricks
            false,
        );

        assert_eq!(result.t_advance, 0.0);
        assert_eq!(result.bricks_traversed, 0);
    }

    #[test]
    fn budget_march_zero_budget() {
        let meta = BrickBudgetMeta {
            b_max: [1.0, 1.0, 1.0],
            l_max: 1.0,
            provenance: BoundProvenance::Analytic,
            aabb_min: [0.0, 0.0, 0.0],
            aabb_max: [1.0, 1.0, 1.0],
        };

        let result = budget_march_traverse(
            [0.5, 0.5, 0.5],
            [1.0, 0.0, 0.0],
            0.0,
            &|_p| Some(meta),
            false,
        );

        assert_eq!(result.t_advance, 0.0);
        assert_eq!(result.bricks_traversed, 0);
    }

    #[test]
    fn no_surface_crossing_along_traversal() {
        // Sphere of radius 1 centered at origin. Point at (3,0,0), ray toward (-1,0,0).
        // Lower bound at start = sphere_distance(3,0,0) = 2.0
        fn sphere_dist(p: [f32; 3]) -> f32 {
            (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt() - 1.0
        }

        let meta = BrickBudgetMeta {
            b_max: [1.0, 1.0, 1.0],
            l_max: 1.0,
            provenance: BoundProvenance::Analytic,
            aabb_min: [-10.0, -10.0, -10.0],
            aabb_max: [10.0, 10.0, 10.0],
        };

        let p0 = [3.0, 0.0, 0.0];
        let v = [-1.0, 0.0, 0.0];
        let b_lower = sphere_dist(p0); // 2.0

        let result = budget_march_traverse(p0, v, b_lower, &|_p| Some(meta), false);

        // Check that at the final position, sphere distance is still >= 0
        let p_end = [
            p0[0] + result.t_advance * v[0],
            p0[1] + result.t_advance * v[1],
            p0[2] + result.t_advance * v[2],
        ];
        let f_end = sphere_dist(p_end);
        assert!(
            f_end >= -1e-4,
            "Surface crossing: t_advance={}, f={}",
            result.t_advance,
            f_end
        );
    }
}
