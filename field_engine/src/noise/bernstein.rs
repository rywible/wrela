use crate::lipschitz::types::BoundProvenance;

/// A certified polynomial patch over one noise lattice cell.
#[derive(Debug, Clone)]
pub struct CertifiedNoisePatch {
    /// Range of noise value over the cell: [value_min, value_max]
    pub value_range: [f32; 2],
    /// Per-axis derivative bounds from Bernstein coefficients.
    pub derivative_bound: [f32; 3],
    /// Always AnalyticBernstein.
    pub provenance: BoundProvenance,
}

/// Quintic fade coefficients: s(t) = 6t^5 - 15t^4 + 10t^3
/// In power basis: [0, 0, 0, 10, -15, 6]
const FADE_COEFFS: [f64; 6] = [0.0, 0.0, 0.0, 10.0, -15.0, 6.0];

/// Convert power basis polynomial to Bernstein basis over [0,1].
/// Input: coefficients c[0..=n] for p(t) = c[0] + c[1]*t + ... + c[n]*t^n
/// Output: Bernstein coefficients b[0..=n]
fn power_to_bernstein(coeffs: &[f64]) -> Vec<f64> {
    let n = coeffs.len() - 1;
    let mut b = vec![0.0_f64; n + 1];

    // b_k = sum_{j=0..k} C(k,j)/C(n,j) * c_j
    for k in 0..=n {
        let mut sum = 0.0_f64;
        for j in 0..=k {
            // C(k,j)/C(n,j) = (k! * (n-j)!) / (j! * (k-j)! * n!)
            let ratio = binom(k, j) as f64 / binom(n, j) as f64;
            sum += ratio * coeffs[j];
        }
        b[k] = sum;
    }
    b
}

/// Binomial coefficient C(n, k)
fn binom(n: usize, k: usize) -> u64 {
    if k > n {
        return 0;
    }
    let k = k.min(n - k);
    let mut result = 1u64;
    for i in 0..k {
        result = result * (n - i) as u64 / (i + 1) as u64;
    }
    result
}

/// Multiply two polynomials in power basis.
fn poly_mul(a: &[f64], b: &[f64]) -> Vec<f64> {
    if a.is_empty() || b.is_empty() {
        return vec![];
    }
    let n = a.len() + b.len() - 1;
    let mut result = vec![0.0_f64; n];
    for (i, &ai) in a.iter().enumerate() {
        for (j, &bj) in b.iter().enumerate() {
            result[i + j] += ai * bj;
        }
    }
    result
}

/// Differentiate a polynomial in power basis.
#[allow(dead_code)]
fn poly_diff(coeffs: &[f64]) -> Vec<f64> {
    if coeffs.len() <= 1 {
        return vec![0.0];
    }
    let mut result = vec![0.0_f64; coeffs.len() - 1];
    for i in 1..coeffs.len() {
        result[i - 1] = coeffs[i] * i as f64;
    }
    result
}

/// Compute certified noise patch for one lattice cell.
/// Input: the 8 gradient vectors at cell corners (from Perlin hash table).
/// Output: tight range and derivative bounds via Bernstein conversion.
pub fn certify_noise_patch(
    _cell_origin: [i32; 3],
    gradients: &[[f32; 3]; 8],
) -> CertifiedNoisePatch {
    // Improved Perlin noise in one cell:
    // f(x,y,z) = trilinear interpolation of gradient dot products using quintic fades
    //
    // gradient dot at corner (i,j,k):
    //   d_{ijk} = g_{ijk} · (x-i, y-j, z-k)
    // where g_{ijk} is the gradient vector.
    //
    // The interpolated result is:
    //   f = sum over corners of weight(corner) * d_{corner}
    // where weight uses the quintic fade s(t) = 6t^5 - 15t^4 + 10t^3.
    //
    // For corner (i,j,k) ∈ {0,1}^3:
    //   w_{ijk}(x,y,z) = h_i(x) * h_j(y) * h_k(z)
    // where h_0(t) = 1 - s(t), h_1(t) = s(t)
    //
    // Each d_{ijk}(x,y,z) is linear in (x,y,z):
    //   d_{ijk} = gx*(x-i) + gy*(y-j) + gz*(z-k)
    //
    // Product of (degree 5 fade) × (degree 1 linear) = degree 6 per axis
    // Total tensor product: degree 6 × 6 × 6 → 7^3 = 343 Bernstein coefficients

    // Build the 1D polynomials for each axis
    // s(t) = 6t^5 - 15t^4 + 10t^3
    let s_coeffs: Vec<f64> = FADE_COEFFS.to_vec();
    // 1 - s(t)
    let one_minus_s: Vec<f64> = {
        let mut c = vec![0.0_f64; 6];
        c[0] = 1.0;
        for i in 0..6 {
            c[i] -= s_coeffs[i];
        }
        c
    };

    // For each axis, build the 1D factor polynomials
    // h_0(t) = 1-s(t), h_1(t) = s(t) — degree 5
    // For gradient dot contribution: (t - i) where i ∈ {0, 1}
    //   t - 0 = [0, 1] (power basis), t - 1 = [-1, 1] (power basis)
    let t_poly: Vec<f64> = vec![0.0, 1.0]; // t
    let t_minus_1: Vec<f64> = vec![-1.0, 1.0]; // t - 1

    // Build tensor product Bernstein coefficients
    // f(x,y,z) = sum_{ijk} w_{ijk}(x,y,z) * d_{ijk}(x,y,z)
    //
    // For each corner, the contribution in axis 'a' is:
    //   h_{corner_a}(t_a) * (g_a * (t_a - corner_a))
    // = g_a * h_{corner_a}(t_a) * (t_a - corner_a)

    // We'll compute the full 3D Bernstein coefficients by:
    // 1. For each corner, compute the 1D power-basis polynomial in each axis
    // 2. Convert each 1D polynomial to Bernstein
    // 3. Compute the 3D tensor product coefficients
    // 4. Sum contributions from all 8 corners

    let degree = 6; // degree per axis
    let n = degree + 1; // 7 coefficients per axis

    // 3D Bernstein coefficients
    let mut bern_3d = vec![0.0_f64; n * n * n];

    for corner in 0u32..8 {
        let ci = (corner & 1) as usize;
        let cj = ((corner >> 1) & 1) as usize;
        let ck = ((corner >> 2) & 1) as usize;
        let g = gradients[corner as usize];

        // For each axis a, the total 1D polynomial is:
        // p_a(t) = h_{c_a}(t) * g_a * (t - c_a)
        // where c_a ∈ {0, 1}
        //
        // But the full contribution of this corner is:
        // p_x(x) * p_y_weight(y) * p_z_weight(z)
        //   where the gradient dot is distributed:
        //   d = gx*(x-ci) + gy*(y-cj) + gz*(z-ck)
        //   weight = h_ci(x) * h_cj(y) * h_ck(z)
        //   contribution = weight * d
        //     = h_ci(x)*h_cj(y)*h_ck(z) * [gx*(x-ci) + gy*(y-cj) + gz*(z-ck)]
        //     = gx * [h_ci(x)*(x-ci)] * h_cj(y) * h_ck(z)
        //     + gy * h_ci(x) * [h_cj(y)*(y-cj)] * h_ck(z)
        //     + gz * h_ci(x) * h_cj(y) * [h_ck(z)*(z-ck)]

        let h_x = if ci == 0 { &one_minus_s } else { &s_coeffs };
        let h_y = if cj == 0 { &one_minus_s } else { &s_coeffs };
        let h_z = if ck == 0 { &one_minus_s } else { &s_coeffs };

        let lin_x = if ci == 0 { &t_poly } else { &t_minus_1 };
        let lin_y = if cj == 0 { &t_poly } else { &t_minus_1 };
        let lin_z = if ck == 0 { &t_poly } else { &t_minus_1 };

        // Term 1: gx * [h_ci(x)*(x-ci)] * h_cj(y) * h_ck(z)
        let hx_lin_x = poly_mul(h_x, lin_x); // degree 6
        let bern_x_term = pad_to_degree(&power_to_bernstein(&hx_lin_x), n);
        let bern_y_weight = pad_to_degree(&power_to_bernstein(h_y), n);
        let bern_z_weight = pad_to_degree(&power_to_bernstein(h_z), n);

        accumulate_tensor_product(
            &mut bern_3d,
            n,
            &bern_x_term,
            &bern_y_weight,
            &bern_z_weight,
            g[0] as f64,
        );

        // Term 2: gy * h_ci(x) * [h_cj(y)*(y-cj)] * h_ck(z)
        let hy_lin_y = poly_mul(h_y, lin_y);
        let bern_x_weight = pad_to_degree(&power_to_bernstein(h_x), n);
        let bern_y_term = pad_to_degree(&power_to_bernstein(&hy_lin_y), n);

        accumulate_tensor_product(
            &mut bern_3d,
            n,
            &bern_x_weight,
            &bern_y_term,
            &bern_z_weight,
            g[1] as f64,
        );

        // Term 3: gz * h_ci(x) * h_cj(y) * [h_ck(z)*(z-ck)]
        let hz_lin_z = poly_mul(h_z, lin_z);
        let bern_z_term = pad_to_degree(&power_to_bernstein(&hz_lin_z), n);

        accumulate_tensor_product(
            &mut bern_3d,
            n,
            &bern_x_weight,
            &bern_y_weight,
            &bern_z_term,
            g[2] as f64,
        );
    }

    // Range bounds = min/max of Bernstein coefficients (convex hull property).
    // For tighter bounds, de Casteljau subdivision can be applied (not yet — needs
    // the 3D split implementation verified against edge cases).
    let value_min = bern_3d.iter().cloned().fold(f64::INFINITY, f64::min) as f32;
    let value_max = bern_3d.iter().cloned().fold(f64::NEG_INFINITY, f64::max) as f32;

    // Derivative bounds per axis (from original unsplit coefficients — subdivision
    // doesn't help derivatives since we need the max over the whole cell).
    let deriv_bound_x = compute_deriv_bound_axis(&bern_3d, n, 0);
    let deriv_bound_y = compute_deriv_bound_axis(&bern_3d, n, 1);
    let deriv_bound_z = compute_deriv_bound_axis(&bern_3d, n, 2);

    CertifiedNoisePatch {
        value_range: [value_min, value_max],
        derivative_bound: [deriv_bound_x, deriv_bound_y, deriv_bound_z],
        provenance: BoundProvenance::AnalyticBernstein,
    }
}

/// Pad Bernstein coefficients to a given size (degree+1) via degree elevation.
/// Tighten 3D Bernstein range bounds via one level of de Casteljau subdivision.
/// NOTE: Currently unused — the 3D fiber-split has a bug that makes bounds
/// non-conservative for some cells. Left here for future investigation.
#[allow(dead_code)]
/// Splits each axis at t=0.5, evaluates min/max of Bernstein coefficients in each
/// of the 8 sub-cells, returns the overall [min, max].
fn subdivided_range_3d(bern_3d: &[f64], n: usize) -> (f32, f32) {
    // Extract 1D slices along each axis and split via de Casteljau
    // Strategy: split axis-by-axis. After splitting x, we have 2 sets of n*n*n coefficients.
    // After splitting y on each, 4 sets. After splitting z, 8 sets.

    // Split along x-axis first
    let halves_x = split_3d_axis(bern_3d, n, 0);

    // Split along y-axis
    let mut halves_xy = Vec::with_capacity(4);
    for half in &halves_x {
        let splits = split_3d_axis(half, n, 1);
        halves_xy.extend(splits);
    }

    // Split along z-axis
    let mut global_min = f64::INFINITY;
    let mut global_max = f64::NEG_INFINITY;
    for half in &halves_xy {
        let splits = split_3d_axis(half, n, 2);
        for sub_cell in &splits {
            for &c in sub_cell {
                global_min = global_min.min(c);
                global_max = global_max.max(c);
            }
        }
    }

    (global_min as f32, global_max as f32)
}

#[allow(dead_code)]
/// Split a 3D tensor-product Bernstein polynomial along one axis at t=0.5
/// using de Casteljau's algorithm. Returns [left_half, right_half].
fn split_3d_axis(bern_3d: &[f64], n: usize, axis: usize) -> [Vec<f64>; 2] {
    let total = n * n * n;
    let mut left = vec![0.0_f64; total];
    let mut right = vec![0.0_f64; total];

    // For each line along the target axis, apply 1D de Casteljau split
    match axis {
        0 => {
            for iz in 0..n {
                for iy in 0..n {
                    let line: Vec<f64> = (0..n).map(|ix| bern_3d[iz * n * n + iy * n + ix]).collect();
                    let (l, r) = de_casteljau_split_1d(&line);
                    for ix in 0..n {
                        left[iz * n * n + iy * n + ix] = l[ix];
                        right[iz * n * n + iy * n + ix] = r[ix];
                    }
                }
            }
        }
        1 => {
            for iz in 0..n {
                for ix in 0..n {
                    let line: Vec<f64> = (0..n).map(|iy| bern_3d[iz * n * n + iy * n + ix]).collect();
                    let (l, r) = de_casteljau_split_1d(&line);
                    for iy in 0..n {
                        left[iz * n * n + iy * n + ix] = l[iy];
                        right[iz * n * n + iy * n + ix] = r[iy];
                    }
                }
            }
        }
        2 => {
            for iy in 0..n {
                for ix in 0..n {
                    let line: Vec<f64> = (0..n).map(|iz| bern_3d[iz * n * n + iy * n + ix]).collect();
                    let (l, r) = de_casteljau_split_1d(&line);
                    for iz in 0..n {
                        left[iz * n * n + iy * n + ix] = l[iz];
                        right[iz * n * n + iy * n + ix] = r[iz];
                    }
                }
            }
        }
        _ => unreachable!(),
    }

    [left, right]
}

#[allow(dead_code)]
/// De Casteljau split of 1D Bernstein coefficients at t=0.5.
/// Returns (left_half_coeffs, right_half_coeffs), both degree n.
fn de_casteljau_split_1d(coeffs: &[f64]) -> (Vec<f64>, Vec<f64>) {
    let n = coeffs.len();
    let mut left = vec![0.0_f64; n];
    let mut right = vec![0.0_f64; n];

    // Triangle of intermediate values
    let mut work = coeffs.to_vec();
    left[0] = work[0];
    right[n - 1] = work[n - 1];

    for level in 1..n {
        for i in 0..n - level {
            work[i] = 0.5 * (work[i] + work[i + 1]);
        }
        left[level] = work[0];
        right[n - 1 - level] = work[n - 1 - level];
    }

    (left, right)
}

/// Pad Bernstein coefficients to a given size (degree+1) via degree elevation.
fn pad_to_degree(coeffs: &[f64], target_size: usize) -> Vec<f64> {
    if coeffs.len() >= target_size {
        return coeffs[..target_size].to_vec();
    }
    // Degree elevation: to raise from degree m to degree n,
    // b'_k = sum_{j} C(n-m, k-j)*C(m,j) / C(n,k) * b_j
    // Simpler: elevate one degree at a time
    let mut current = coeffs.to_vec();
    while current.len() < target_size {
        let m = current.len() - 1;
        let new_n = m + 1;
        let mut elevated = vec![0.0_f64; new_n + 1];
        for k in 0..=new_n {
            let mut val = 0.0_f64;
            // b'_k = (k/(n+1)) * b_{k-1} + (1 - k/(n+1)) * b_k
            // where b_{-1} = 0, b_{m+1} = 0 (out of range treated as 0)
            let alpha = k as f64 / (new_n as f64);
            if k > 0 && k - 1 <= m {
                val += alpha * current[k - 1];
            }
            if k <= m {
                val += (1.0 - alpha) * current[k];
            }
            elevated[k] = val;
        }
        current = elevated;
    }
    current
}

/// Accumulate scale * tensor_product(bx, by, bz) into bern_3d.
fn accumulate_tensor_product(
    bern_3d: &mut [f64],
    n: usize,
    bx: &[f64],
    by: &[f64],
    bz: &[f64],
    scale: f64,
) {
    for iz in 0..n {
        for iy in 0..n {
            for ix in 0..n {
                bern_3d[iz * n * n + iy * n + ix] += scale * bx[ix] * by[iy] * bz[iz];
            }
        }
    }
}

/// Compute the max absolute Bernstein coefficient of the partial derivative
/// of a 3D tensor-product polynomial in the given axis.
///
/// For Bernstein degree n in axis 'axis':
///   d/dt B_{i}^n(t) = n * (B_{i}^{n-1}(t+1) - B_{i}^{n-1}(t))
/// So derivative Bernstein coefficients: d_i = n * (c_{i+1} - c_i) for i=0..n-2
fn compute_deriv_bound_axis(bern_3d: &[f64], n: usize, axis: usize) -> f32 {
    let degree = n - 1; // n = degree + 1
    let mut max_abs = 0.0_f64;

    match axis {
        0 => {
            // Differentiate in x: for each (iy, iz), compute d_ix = degree*(c[ix+1] - c[ix])
            for iz in 0..n {
                for iy in 0..n {
                    for ix in 0..degree {
                        let c_next = bern_3d[iz * n * n + iy * n + (ix + 1)];
                        let c_curr = bern_3d[iz * n * n + iy * n + ix];
                        let d = (degree as f64) * (c_next - c_curr);
                        max_abs = max_abs.max(d.abs());
                    }
                }
            }
        }
        1 => {
            // Differentiate in y
            for iz in 0..n {
                for iy in 0..degree {
                    for ix in 0..n {
                        let c_next = bern_3d[iz * n * n + (iy + 1) * n + ix];
                        let c_curr = bern_3d[iz * n * n + iy * n + ix];
                        let d = (degree as f64) * (c_next - c_curr);
                        max_abs = max_abs.max(d.abs());
                    }
                }
            }
        }
        2 => {
            // Differentiate in z
            for iz in 0..degree {
                for iy in 0..n {
                    for ix in 0..n {
                        let c_next = bern_3d[(iz + 1) * n * n + iy * n + ix];
                        let c_curr = bern_3d[iz * n * n + iy * n + ix];
                        let d = (degree as f64) * (c_next - c_curr);
                        max_abs = max_abs.max(d.abs());
                    }
                }
            }
        }
        _ => unreachable!(),
    }

    max_abs as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::noise::perlin::evaluate_improved_perlin;

    /// Get the gradient vectors at corners of a cell, using the same permutation as Perlin.
    fn get_cell_gradients(cell: [i32; 3]) -> [[f32; 3]; 8] {
        // The 12 gradient directions for improved Perlin
        const GRADS: [[f32; 3]; 16] = [
            [1.0, 1.0, 0.0],
            [-1.0, 1.0, 0.0],
            [1.0, -1.0, 0.0],
            [-1.0, -1.0, 0.0],
            [1.0, 0.0, 1.0],
            [-1.0, 0.0, 1.0],
            [1.0, 0.0, -1.0],
            [-1.0, 0.0, -1.0],
            [0.0, 1.0, 1.0],
            [0.0, -1.0, 1.0],
            [0.0, 1.0, -1.0],
            [0.0, -1.0, -1.0],
            [1.0, 1.0, 0.0],
            [-1.0, 1.0, 0.0],
            [0.0, -1.0, 1.0],
            [0.0, -1.0, -1.0],
        ];
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

        let mut grads = [[0.0_f32; 3]; 8];
        for corner in 0u32..8 {
            let ci = (corner & 1) as i32;
            let cj = ((corner >> 1) & 1) as i32;
            let ck = ((corner >> 2) & 1) as i32;

            let xi = ((cell[0] + ci) & 255) as usize;
            let yi = ((cell[1] + cj) & 255) as usize;
            let zi = ((cell[2] + ck) & 255) as usize;

            let a = PERM[xi] as usize + yi;
            let aa = PERM[a] as usize + zi;
            let hash = PERM[aa] & 15;
            grads[corner as usize] = GRADS[hash as usize];
        }
        grads
    }

    #[test]
    fn bernstein_bounds_never_violated() {
        // Test several cells
        for cx in 0..3 {
            for cy in 0..3 {
                for cz in 0..3 {
                    let cell = [cx, cy, cz];
                    let grads = get_cell_gradients(cell);
                    let patch = certify_noise_patch(cell, &grads);

                    // Dense sample within the cell
                    let steps = 10;
                    for ix in 0..=steps {
                        for iy in 0..=steps {
                            for iz in 0..=steps {
                                let x = cx as f32 + ix as f32 / steps as f32;
                                let y = cy as f32 + iy as f32 / steps as f32;
                                let z = cz as f32 + iz as f32 / steps as f32;
                                let v = evaluate_improved_perlin([x, y, z], 1.0);
                                assert!(
                                    v >= patch.value_range[0] - 1e-4,
                                    "Value {} below min {} at ({},{},{}) in cell {:?}",
                                    v, patch.value_range[0], x, y, z, cell
                                );
                                assert!(
                                    v <= patch.value_range[1] + 1e-4,
                                    "Value {} above max {} at ({},{},{}) in cell {:?}",
                                    v, patch.value_range[1], x, y, z, cell
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn bernstein_derivative_bounds_conservative() {
        // Check derivative bounds via finite differences
        let cell = [0, 0, 0];
        let grads = get_cell_gradients(cell);
        let patch = certify_noise_patch(cell, &grads);

        let h = 1e-4;
        let steps = 10;
        for ix in 1..steps {
            for iy in 1..steps {
                for iz in 1..steps {
                    let x = ix as f32 / steps as f32;
                    let y = iy as f32 / steps as f32;
                    let z = iz as f32 / steps as f32;

                    // Numerical partial derivatives
                    let dfdx = (evaluate_improved_perlin([x + h, y, z], 1.0)
                        - evaluate_improved_perlin([x - h, y, z], 1.0))
                        / (2.0 * h);
                    let dfdy = (evaluate_improved_perlin([x, y + h, z], 1.0)
                        - evaluate_improved_perlin([x, y - h, z], 1.0))
                        / (2.0 * h);
                    let dfdz = (evaluate_improved_perlin([x, y, z + h], 1.0)
                        - evaluate_improved_perlin([x, y, z - h], 1.0))
                        / (2.0 * h);

                    assert!(
                        dfdx.abs() <= patch.derivative_bound[0] + 0.1,
                        "dfdx {} exceeds bound {} at ({},{},{})",
                        dfdx.abs(), patch.derivative_bound[0], x, y, z
                    );
                    assert!(
                        dfdy.abs() <= patch.derivative_bound[1] + 0.1,
                        "dfdy {} exceeds bound {} at ({},{},{})",
                        dfdy.abs(), patch.derivative_bound[1], x, y, z
                    );
                    assert!(
                        dfdz.abs() <= patch.derivative_bound[2] + 0.1,
                        "dfdz {} exceeds bound {} at ({},{},{})",
                        dfdz.abs(), patch.derivative_bound[2], x, y, z
                    );
                }
            }
        }
    }

    #[test]
    fn bernstein_provenance_is_analytic() {
        let grads = [[1.0, 0.0, 0.0]; 8];
        let patch = certify_noise_patch([0, 0, 0], &grads);
        assert_eq!(patch.provenance, BoundProvenance::AnalyticBernstein);
    }

    #[test]
    fn tightening_ratio() {
        // Bernstein bounds provide per-cell tight bounds, verified by comparing
        // against dense-sampled actual ranges within each cell.
        // The ratio (Bernstein range / actual range) should be >= 1.0 (conservative)
        // and for most cells reasonably tight (< 5x).
        let mut total_ratio = 0.0_f32;
        let mut count = 0;
        for cx in 0..4 {
            for cy in 0..4 {
                for cz in 0..4 {
                    let cell = [cx, cy, cz];
                    let grads = get_cell_gradients(cell);
                    let patch = certify_noise_patch(cell, &grads);
                    let bern_range = patch.value_range[1] - patch.value_range[0];

                    // Dense-sample actual range
                    let mut actual_min = f32::INFINITY;
                    let mut actual_max = f32::NEG_INFINITY;
                    let steps = 20;
                    for ix in 0..=steps {
                        for iy in 0..=steps {
                            for iz in 0..=steps {
                                let x = cx as f32 + ix as f32 / steps as f32;
                                let y = cy as f32 + iy as f32 / steps as f32;
                                let z = cz as f32 + iz as f32 / steps as f32;
                                let v = evaluate_improved_perlin([x, y, z], 1.0);
                                actual_min = actual_min.min(v);
                                actual_max = actual_max.max(v);
                            }
                        }
                    }
                    let actual_range = actual_max - actual_min;
                    if actual_range > 1e-6 {
                        let ratio = bern_range / actual_range;
                        assert!(
                            ratio >= 0.99,
                            "Bernstein not conservative in cell {:?}: ratio={}",
                            cell, ratio
                        );
                        total_ratio += ratio;
                        count += 1;
                    }
                }
            }
        }
        let avg_ratio = total_ratio / count as f32;
        // Average tightening ratio should be reasonable (< 5x overestimate)
        assert!(
            avg_ratio < 5.0,
            "Average Bernstein/actual ratio {} too loose (> 5x)",
            avg_ratio
        );
    }
}
