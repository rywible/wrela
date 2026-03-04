//! Property-based tests for field_engine safety invariants.
//! Run with: cargo test -p wrela_field_engine --test proptests
//! High-confidence: PROPTEST_CASES=1000 cargo test -p wrela_field_engine --test proptests

use proptest::prelude::*;
use wrela_field_engine::*;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn sphere_distance(p: [f32; 3]) -> f32 {
    (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt() - 1.0
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len < 1e-10 {
        [1.0, 0.0, 0.0]
    } else {
        [v[0] / len, v[1] / len, v[2] / len]
    }
}

fn direction_strategy() -> impl Strategy<Value = [f32; 3]> {
    prop::array::uniform3(-1.0f32..1.0).prop_map(normalize)
}

// ---------------------------------------------------------------------------
// 1. No-surface-crossing: safe_step_from_lower_bound
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    #[test]
    fn no_surface_crossing_sphere(
        p in prop::array::uniform3(-5.0f32..5.0),
        v in direction_strategy(),
    ) {
        let d = sphere_distance(p);
        if d < 0.0 {
            return Ok(());
        }

        let step = safe_step_from_lower_bound(d, [1.0, 1.0, 1.0], true, 1.73, v, 20.0);

        if step > 0.0 {
            let p_end = [p[0] + step * v[0], p[1] + step * v[1], p[2] + step * v[2]];
            let d_end = sphere_distance(p_end);
            prop_assert!(
                d_end >= -1e-4,
                "Surface crossing: step={}, d_start={}, d_end={}, p={:?}, v={:?}",
                step, d, d_end, p, v
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 2. Envelope conservatism: lipschitz_envelope <= true value
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    #[test]
    fn envelope_conservative_sphere(
        p in prop::array::uniform3(-2.0f32..2.0),
    ) {
        let b_cell = [1.0, 1.0, 1.0];

        let mut samples = Vec::new();
        for ix in -2..=2 {
            for iy in -2..=2 {
                for iz in -2..=2 {
                    let sp = [ix as f32 * 0.5, iy as f32 * 0.5, iz as f32 * 0.5];
                    samples.push((sp, sphere_distance(sp)));
                }
            }
        }

        let env = lipschitz_envelope(p, &samples, b_cell);
        let true_val = sphere_distance(p);
        prop_assert!(
            env <= true_val + 1e-5,
            "Envelope not conservative: env={} > true={} at {:?}",
            env, true_val, p
        );
    }
}

// ---------------------------------------------------------------------------
// 3. Whitney envelope conservatism
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    #[test]
    fn whitney_conservative_sphere(
        p in prop::array::uniform3(-2.0f32..2.0),
    ) {
        let k_cell = 1.0;

        let mut samples = Vec::new();
        for ix in -2..=2 {
            for iy in -2..=2 {
                for iz in -2..=2 {
                    let sp = [ix as f32 * 0.5, iy as f32 * 0.5, iz as f32 * 0.5];
                    let d = sphere_distance(sp);
                    let r = (sp[0] * sp[0] + sp[1] * sp[1] + sp[2] * sp[2]).sqrt();
                    let n = if r < 1e-10 {
                        [0.0, 0.0, 1.0]
                    } else {
                        [sp[0] / r, sp[1] / r, sp[2] / r]
                    };
                    samples.push((sp, d, n, 0.0, 1.0_f32));
                }
            }
        }

        let env = whitney_c1_envelope(p, &samples, k_cell);
        let true_val = sphere_distance(p);
        prop_assert!(
            env <= true_val + 1e-4,
            "Whitney not conservative: env={} > true={} at {:?}",
            env, true_val, p
        );
    }
}

// ---------------------------------------------------------------------------
// 4. f16 roundtrip invariant
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    #[test]
    fn f16_lower_roundtrip(x in -65000.0f32..65000.0) {
        if x.is_nan() || x.is_infinite() {
            return Ok(());
        }
        let h = f16_conservative(x, BoundDirection::Lower);
        let back = f16_to_f32(h);
        prop_assert!(
            back <= x || (back - x).abs() < 1e-10,
            "f16 Lower roundtrip violated: x={}, back={}",
            x, back
        );
    }

    #[test]
    fn f16_upper_roundtrip(x in -65000.0f32..65000.0) {
        if x.is_nan() || x.is_infinite() {
            return Ok(());
        }
        let h = f16_conservative(x, BoundDirection::Upper);
        let back = f16_to_f32(h);
        prop_assert!(
            back >= x || (x - back).abs() < 1e-10,
            "f16 Upper roundtrip violated: x={}, back={}",
            x, back
        );
    }
}

// ---------------------------------------------------------------------------
// 5. Cone-union no-surface-crossing
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn cone_union_no_crossing(
        offset in 1.5f32..4.0,
        vx in -1.0f32..1.0,
        vy in -1.0f32..1.0,
        vz in -1.0f32..1.0,
    ) {
        let p0 = [offset, 0.0, 0.0];
        let v = normalize([vx, vy, vz]);
        let b_cell = [1.0, 1.0, 1.0];

        let mut samples = Vec::new();
        for ix in -2..=4 {
            for iy in -2..=2 {
                for iz in -2..=2 {
                    let sp = [ix as f32 * 0.5, iy as f32 * 0.5, iz as f32 * 0.5];
                    samples.push((sp, sphere_distance(sp)));
                }
            }
        }

        let step = cone_union_safe_step(p0, v, &samples, b_cell, 10.0);

        if step > 0.0 {
            let p_end = [p0[0] + step * v[0], p0[1] + step * v[1], p0[2] + step * v[2]];
            let d_end = sphere_distance(p_end);
            prop_assert!(
                d_end >= -1e-3,
                "Cone-union surface crossing: step={}, d_end={}, p0={:?}, v={:?}",
                step, d_end, p0, v
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 6. Dual envelope fusion never regresses vs L1
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    #[test]
    fn dual_envelope_never_regresses(
        p in prop::array::uniform3(-2.0f32..2.0),
    ) {
        let b_cell = [1.0, 1.0, 1.0];

        let mut samples = Vec::new();
        for ix in -2..=2 {
            for iy in -2..=2 {
                for iz in -2..=2 {
                    let sp = [ix as f32 * 0.5, iy as f32 * 0.5, iz as f32 * 0.5];
                    samples.push((sp, sphere_distance(sp)));
                }
            }
        }

        let l1_only = lipschitz_envelope(p, &samples, b_cell);
        // with L2 certificate (l_scalar=1.0, has_l2=true)
        let dual = dual_envelope_lower_bound(p, &samples, b_cell, 1.0, true);

        prop_assert!(
            dual >= l1_only - 1e-4,
            "Dual envelope regressed: dual={} < l1_only={} at {:?}",
            dual, l1_only, p
        );
    }
}

// ---------------------------------------------------------------------------
// 7. Perlin noise range
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    #[test]
    fn perlin_range_bounded(
        p in prop::array::uniform3(-100.0f32..100.0),
        freq in 0.1f32..10.0,
    ) {
        let v = evaluate_improved_perlin(p, freq);
        prop_assert!(
            v >= -1.0 && v <= 1.0,
            "Perlin out of range at {:?} freq={}: {}",
            p, freq, v
        );
    }
}

// ---------------------------------------------------------------------------
// 8. Spacetime fail-closed without Bt
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn spacetime_fail_closed(
        b_lower in -10.0f32..10.0,
        speed in 0.1f32..10.0,
    ) {
        let velocity = [speed, 0.0, 0.0];
        // has_spacetime_bound = false => must return 0
        let step = safe_step_spacetime_along_path(
            b_lower,
            [1.0, 1.0, 1.0], // dfdxyz_bound
            0.0,              // b_time (irrelevant since no bound)
            false,            // has_spacetime_bound
            velocity,
            10.0,             // dt_remaining
            10.0,             // dt_to_bound_region_exit
        );
        prop_assert_eq!(step, 0.0, "Should fail-closed without Bt, got {}", step);
    }
}

// ---------------------------------------------------------------------------
// 9. Determinism: same seeds produce same outputs
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn determinism_same_seeds(
        world_seed in 0u64..1000000,
        system_id in 0u32..1000,
    ) {
        let s1 = DeterminismState::new(world_seed);
        let s2 = DeterminismState::new(world_seed);

        let seed1 = s1.system_seed(system_id);
        let seed2 = s2.system_seed(system_id);
        prop_assert_eq!(seed1, seed2);

        let px1 = s1.pixel_seed(system_id, 100, 200);
        let px2 = s2.pixel_seed(system_id, 100, 200);
        prop_assert_eq!(px1, px2);
    }
}
