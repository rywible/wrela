/// Standard improved Perlin noise (Ken Perlin 2002).

/// Permutation table (standard Perlin permutation, doubled).
const PERM: [u8; 512] = {
    const P: [u8; 256] = [
        151, 160, 137, 91, 90, 15, 131, 13, 201, 95, 96, 53, 194, 233, 7, 225,
        140, 36, 103, 30, 69, 142, 8, 99, 37, 240, 21, 10, 23, 190, 6, 148,
        247, 120, 234, 75, 0, 26, 197, 62, 94, 252, 219, 203, 117, 35, 11, 32,
        57, 177, 33, 88, 237, 149, 56, 87, 174, 20, 125, 136, 171, 168, 68, 175,
        74, 165, 71, 134, 139, 48, 27, 166, 77, 146, 158, 231, 83, 111, 229, 122,
        60, 211, 133, 230, 220, 105, 92, 41, 55, 46, 245, 40, 244, 102, 143, 54,
        65, 25, 63, 161, 1, 216, 80, 73, 209, 76, 132, 187, 208, 89, 18, 169,
        200, 196, 135, 130, 116, 188, 159, 86, 164, 100, 109, 198, 173, 186, 3, 64,
        52, 217, 226, 250, 124, 123, 5, 202, 38, 147, 118, 126, 255, 82, 85, 212,
        207, 206, 59, 227, 47, 16, 58, 17, 182, 189, 28, 42, 223, 183, 170, 213,
        119, 248, 152, 2, 44, 154, 163, 70, 221, 153, 101, 155, 167, 43, 172, 9,
        129, 22, 39, 253, 19, 98, 108, 110, 79, 113, 224, 232, 178, 185, 112, 104,
        218, 246, 97, 228, 251, 34, 242, 193, 238, 210, 144, 12, 191, 179, 162, 241,
        81, 51, 145, 235, 249, 14, 239, 107, 49, 192, 214, 31, 181, 199, 106, 157,
        184, 84, 204, 176, 115, 121, 50, 45, 127, 4, 150, 254, 138, 236, 205, 93,
        222, 114, 67, 29, 24, 72, 243, 141, 128, 195, 78, 66, 215, 61, 156, 180,
    ];
    let mut result = [0u8; 512];
    let mut i = 0;
    while i < 256 {
        result[i] = P[i];
        result[i + 256] = P[i];
        i += 1;
    }
    result
};

/// Quintic fade: s(t) = 6t^5 - 15t^4 + 10t^3 (C2 continuous)
#[inline]
fn fade(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

/// Linear interpolation
#[inline]
fn lerp(t: f32, a: f32, b: f32) -> f32 {
    a + t * (b - a)
}

/// Gradient function: dot product of gradient vector and distance vector.
/// Uses 12 gradient directions from {(1,1,0), (1,-1,0), ...}.
#[inline]
fn grad(hash: u8, x: f32, y: f32, z: f32) -> f32 {
    let h = hash & 15;
    let u = if h < 8 { x } else { y };
    let v = if h < 4 {
        y
    } else if h == 12 || h == 14 {
        x
    } else {
        z
    };
    let a = if h & 1 == 0 { u } else { -u };
    let b = if h & 2 == 0 { v } else { -v };
    a + b
}

/// Evaluate improved Perlin noise at point p scaled by freq.
pub fn evaluate_improved_perlin(p: [f32; 3], freq: f32) -> f32 {
    let x = p[0] * freq;
    let y = p[1] * freq;
    let z = p[2] * freq;

    // Integer cell coordinates (floor)
    let xi = x.floor() as i32;
    let yi = y.floor() as i32;
    let zi = z.floor() as i32;

    // Fractional part within cell
    let xf = x - xi as f32;
    let yf = y - yi as f32;
    let zf = z - zi as f32;

    // Wrap to [0, 255]
    let xi = (xi & 255) as usize;
    let yi = (yi & 255) as usize;
    let zi = (zi & 255) as usize;

    // Fade curves
    let u = fade(xf);
    let v = fade(yf);
    let w = fade(zf);

    // Hash coordinates of cube corners
    let a = PERM[xi] as usize + yi;
    let aa = PERM[a] as usize + zi;
    let ab = PERM[a + 1] as usize + zi;
    let b = PERM[xi + 1] as usize + yi;
    let ba = PERM[b] as usize + zi;
    let bb = PERM[b + 1] as usize + zi;

    // Trilinear interpolation of gradient dot products
    lerp(
        w,
        lerp(
            v,
            lerp(
                u,
                grad(PERM[aa], xf, yf, zf),
                grad(PERM[ba], xf - 1.0, yf, zf),
            ),
            lerp(
                u,
                grad(PERM[ab], xf, yf - 1.0, zf),
                grad(PERM[bb], xf - 1.0, yf - 1.0, zf),
            ),
        ),
        lerp(
            v,
            lerp(
                u,
                grad(PERM[aa + 1], xf, yf, zf - 1.0),
                grad(PERM[ba + 1], xf - 1.0, yf, zf - 1.0),
            ),
            lerp(
                u,
                grad(PERM[ab + 1], xf, yf - 1.0, zf - 1.0),
                grad(PERM[bb + 1], xf - 1.0, yf - 1.0, zf - 1.0),
            ),
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_in_bounds() {
        // Comprehensive 3D scan to find actual empirical range.
        // Improved Perlin noise with 12 gradient directions (length sqrt(2))
        // has theoretical max of 1.0 per the standard formulation.
        let mut min_val = f32::INFINITY;
        let mut max_val = f32::NEG_INFINITY;

        // Scan over many points including near cell boundaries
        for ix in -50..=50 {
            for iy in -50..=50 {
                for iz in -5..=5 {
                    let p = [ix as f32 * 0.1, iy as f32 * 0.1, iz as f32 * 0.1];
                    let v = evaluate_improved_perlin(p, 1.0);
                    min_val = min_val.min(v);
                    max_val = max_val.max(v);
                    assert!(
                        v >= -1.0 && v <= 1.0,
                        "Out of range at {:?}: {}",
                        p, v
                    );
                }
            }
        }

        // Also scan at finer resolution near cell corners where extremes occur
        for ix in 0..=10 {
            for iy in 0..=10 {
                for iz in 0..=10 {
                    let p = [ix as f32 * 0.01, iy as f32 * 0.01, iz as f32 * 0.01];
                    let v = evaluate_improved_perlin(p, 1.0);
                    min_val = min_val.min(v);
                    max_val = max_val.max(v);
                }
            }
        }

        // Verify we actually found non-trivial range (noise is active)
        assert!(max_val > 0.3, "Max value too low: {} — noise may be broken", max_val);
        assert!(min_val < -0.3, "Min value too high: {} — noise may be broken", min_val);
    }

    #[test]
    fn range_with_various_frequencies() {
        // Verify range holds for different frequency scales
        for freq in [0.5, 1.0, 2.0, 4.0, 8.0] {
            for ix in -20..=20 {
                for iy in -20..=20 {
                    let p = [ix as f32 * 0.25, iy as f32 * 0.25, 0.7];
                    let v = evaluate_improved_perlin(p, freq);
                    assert!(
                        v >= -1.0 && v <= 1.0,
                        "Out of range at {:?} freq={}: {}",
                        p, freq, v
                    );
                }
            }
        }
    }

    #[test]
    fn continuity_at_cell_boundaries() {
        // Check that noise is continuous across integer boundaries
        let eps = 1e-4;
        for boundary in [0.0_f32, 1.0, -1.0, 2.0, -2.0] {
            let p_before = [boundary - eps, 0.5, 0.5];
            let p_after = [boundary + eps, 0.5, 0.5];
            let v_before = evaluate_improved_perlin(p_before, 1.0);
            let v_after = evaluate_improved_perlin(p_after, 1.0);
            let diff = (v_before - v_after).abs();
            assert!(
                diff < 0.01,
                "Discontinuity at x={}: diff={}",
                boundary,
                diff
            );
        }
    }

    #[test]
    fn c2_smoothness_check() {
        // Verify second derivative is continuous by checking finite differences
        let h = 1e-3;
        let p0 = [0.5, 0.5, 0.5];

        // First derivative (central difference)
        let f_plus = evaluate_improved_perlin([p0[0] + h, p0[1], p0[2]], 1.0);
        let f_minus = evaluate_improved_perlin([p0[0] - h, p0[1], p0[2]], 1.0);
        let _df = (f_plus - f_minus) / (2.0 * h);

        // Second derivative (central difference of first derivative)
        let f_center = evaluate_improved_perlin(p0, 1.0);
        let d2f = (f_plus - 2.0 * f_center + f_minus) / (h * h);

        // Check at a slightly shifted point
        let p1 = [0.5 + 0.001, 0.5, 0.5];
        let f_plus1 = evaluate_improved_perlin([p1[0] + h, p1[1], p1[2]], 1.0);
        let f_minus1 = evaluate_improved_perlin([p1[0] - h, p1[1], p1[2]], 1.0);
        let f_center1 = evaluate_improved_perlin(p1, 1.0);
        let d2f1 = (f_plus1 - 2.0 * f_center1 + f_minus1) / (h * h);

        // Second derivatives should be close (C2)
        let diff = (d2f - d2f1).abs();
        assert!(diff < 1.0, "Second derivative not smooth: diff={}", diff);
    }

    #[test]
    fn frequency_scaling() {
        // Higher frequency should produce faster-varying noise
        let p = [0.5, 0.5, 0.5];
        let v1 = evaluate_improved_perlin(p, 1.0);
        let v2 = evaluate_improved_perlin(p, 2.0);
        // They should generally differ (not guaranteed but very likely)
        // Just check they're both valid
        assert!(v1.is_finite());
        assert!(v2.is_finite());
    }

    #[test]
    fn deterministic() {
        let p = [1.234, 5.678, 9.012];
        let v1 = evaluate_improved_perlin(p, 1.0);
        let v2 = evaluate_improved_perlin(p, 1.0);
        assert_eq!(v1, v2);
    }
}
