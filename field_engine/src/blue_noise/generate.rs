/// Blue noise texture generation using void-and-cluster algorithm.
/// Precomputed 128x128x64 texture for spatiotemporal sampling.

pub const BLUE_NOISE_WIDTH: usize = 128;
pub const BLUE_NOISE_HEIGHT: usize = 128;
pub const BLUE_NOISE_DEPTH: usize = 64;
pub const BLUE_NOISE_SIZE: usize = BLUE_NOISE_WIDTH * BLUE_NOISE_HEIGHT * BLUE_NOISE_DEPTH;

/// Generate a full-size blue noise texture (128x128x64).
/// This is expensive — intended for offline precomputation, not per-frame use.
pub fn generate_blue_noise(seed: u64) -> Vec<u8> {
    generate_blue_noise_custom(BLUE_NOISE_WIDTH, BLUE_NOISE_HEIGHT, BLUE_NOISE_DEPTH, seed)
}

/// Generate a blue noise texture with custom dimensions using void-and-cluster.
pub fn generate_blue_noise_custom(width: usize, height: usize, depth: usize, seed: u64) -> Vec<u8> {
    let mut frames = Vec::with_capacity(width * height * depth);

    for frame in 0..depth {
        let frame_seed = seed.wrapping_add(frame as u64).wrapping_mul(0x9E3779B97F4A7C15);
        let frame_data = generate_blue_noise_2d(width, height, frame_seed);
        frames.extend_from_slice(&frame_data);
    }

    frames
}

/// Generate a single 2D blue noise frame using the void-and-cluster algorithm.
///
/// Algorithm:
/// 1. Start with a small initial set of points (white noise)
/// 2. Remove the "tightest cluster" point (highest energy), assign it the next rank
/// 3. Continue until initial set is exhausted
/// 4. Then repeatedly find the "largest void" (lowest energy position), place a point,
///    assign the next rank
/// 5. Continue until all pixels are ranked
///
/// Energy is computed with a Gaussian kernel (toroidal wrapping for seamless tiling).
fn generate_blue_noise_2d(width: usize, height: usize, seed: u64) -> Vec<u8> {
    let total = width * height;
    let mut ranks = vec![0u32; total];
    let mut occupied = vec![false; total];

    // Energy accumulator: for each pixel, sum of Gaussian contributions from occupied pixels
    let mut energy = vec![0.0_f32; total];

    // Gaussian sigma: ~1.5% of image dimension gives good blue noise properties
    let sigma = (width.min(height) as f32) * 0.015;
    let sigma = sigma.max(1.0); // ensure minimum sigma for small textures
    let sigma_sq2 = 2.0 * sigma * sigma;
    // Kernel radius: beyond 3*sigma contribution is negligible
    let kernel_radius = (3.0 * sigma).ceil() as i32;

    // Step 1: Create initial seed pattern (~10% of pixels)
    let initial_count = (total / 10).max(1);
    let mut rng = seed;

    let mut initial_points = Vec::with_capacity(initial_count);
    for _ in 0..initial_count {
        loop {
            rng = xorshift64(rng);
            let idx = (rng as usize) % total;
            if !occupied[idx] {
                occupied[idx] = true;
                initial_points.push(idx);
                let px = (idx % width) as i32;
                let py = (idx / width) as i32;
                add_energy_contribution(&mut energy, width, height, px, py, sigma_sq2, kernel_radius, 1.0);
                break;
            }
        }
    }

    // Step 2: Remove tightest clusters (highest energy occupied pixels)
    // Assign ranks from initial_count-1 down to 0
    let mut rank_counter = initial_count as u32;
    let mut remaining_initial = initial_points.clone();

    for _ in 0..initial_count {
        let mut max_energy = f32::NEG_INFINITY;
        let mut max_idx_in_remaining = 0;
        for (i, &pixel_idx) in remaining_initial.iter().enumerate() {
            if occupied[pixel_idx] && energy[pixel_idx] > max_energy {
                max_energy = energy[pixel_idx];
                max_idx_in_remaining = i;
            }
        }

        let pixel_idx = remaining_initial.swap_remove(max_idx_in_remaining);
        rank_counter -= 1;
        ranks[pixel_idx] = rank_counter;
        occupied[pixel_idx] = false;

        let px = (pixel_idx % width) as i32;
        let py = (pixel_idx / width) as i32;
        add_energy_contribution(&mut energy, width, height, px, py, sigma_sq2, kernel_radius, -1.0);
    }

    // Reset: re-occupy initial points for void-filling phase
    energy.fill(0.0);
    for &idx in &initial_points {
        occupied[idx] = true;
        let px = (idx % width) as i32;
        let py = (idx / width) as i32;
        add_energy_contribution(&mut energy, width, height, px, py, sigma_sq2, kernel_radius, 1.0);
    }

    // Step 3: Fill voids (find lowest energy unoccupied pixel, assign next rank)
    rank_counter = initial_count as u32;
    for _ in initial_count..total {
        let mut min_energy = f32::INFINITY;
        let mut min_idx = 0;
        for idx in 0..total {
            if !occupied[idx] && energy[idx] < min_energy {
                min_energy = energy[idx];
                min_idx = idx;
            }
        }

        ranks[min_idx] = rank_counter;
        rank_counter += 1;
        occupied[min_idx] = true;

        let px = (min_idx % width) as i32;
        let py = (min_idx / width) as i32;
        add_energy_contribution(&mut energy, width, height, px, py, sigma_sq2, kernel_radius, 1.0);
    }

    // Convert ranks to [0, 255] range
    let scale = 256.0 / total as f64;
    ranks.iter().map(|&r| ((r as f64 * scale).min(255.0)) as u8).collect()
}

/// Add (or subtract) the Gaussian energy contribution of a point at (px, py)
/// to all pixels within kernel_radius, with toroidal wrapping.
fn add_energy_contribution(
    energy: &mut [f32],
    width: usize,
    height: usize,
    px: i32,
    py: i32,
    sigma_sq2: f32,
    kernel_radius: i32,
    sign: f32,
) {
    let w = width as i32;
    let h = height as i32;

    for dy in -kernel_radius..=kernel_radius {
        let ny = ((py + dy) % h + h) % h;
        let dy_f = dy as f32;
        let dy_sq = dy_f * dy_f;

        for dx in -kernel_radius..=kernel_radius {
            let nx = ((px + dx) % w + w) % w;
            let dx_f = dx as f32;
            let dist_sq = dx_f * dx_f + dy_sq;

            let contrib = (-dist_sq / sigma_sq2).exp();
            energy[ny as usize * width + nx as usize] += sign * contrib;
        }
    }
}

fn xorshift64(mut state: u64) -> u64 {
    state ^= state << 13;
    state ^= state >> 7;
    state ^= state << 17;
    state
}

#[cfg(test)]
mod tests {
    use super::*;

    // Use small sizes for fast tests. The algorithm is the same.
    const TEST_W: usize = 32;
    const TEST_H: usize = 32;
    const TEST_D: usize = 4;

    fn test_texture(seed: u64) -> Vec<u8> {
        generate_blue_noise_custom(TEST_W, TEST_H, TEST_D, seed)
    }

    #[test]
    fn correct_size() {
        let data = test_texture(42);
        assert_eq!(data.len(), TEST_W * TEST_H * TEST_D);
    }

    #[test]
    fn mean_approximately_half() {
        let data = test_texture(42);
        let sum: u64 = data.iter().map(|&v| v as u64).sum();
        let mean = sum as f64 / data.len() as f64;
        assert!(
            (mean - 127.5).abs() < 10.0,
            "Mean {} too far from 127.5",
            mean
        );
    }

    #[test]
    fn deterministic() {
        let a = test_texture(42);
        let b = test_texture(42);
        assert_eq!(a, b);
    }

    #[test]
    fn different_seeds_differ() {
        let a = test_texture(42);
        let b = test_texture(43);
        assert_ne!(a, b);
    }

    #[test]
    fn low_frequency_power_suppressed() {
        // True blue noise should have suppressed low-frequency energy.
        // We check by computing variance of local block averages —
        // blue noise should have much lower block-average variance than white noise.
        let data = generate_blue_noise_custom(64, 64, 1, 42);
        let frame = &data[..];

        // Compute 8x8 block averages
        let block_size = 8;
        let blocks_x = 64 / block_size;
        let blocks_y = 64 / block_size;
        let mut block_means = Vec::with_capacity(blocks_x * blocks_y);

        for by in 0..blocks_y {
            for bx in 0..blocks_x {
                let mut sum = 0.0_f64;
                for dy in 0..block_size {
                    for dx in 0..block_size {
                        let px = bx * block_size + dx;
                        let py = by * block_size + dy;
                        sum += frame[py * 64 + px] as f64;
                    }
                }
                block_means.push(sum / (block_size * block_size) as f64);
            }
        }

        let global_mean: f64 = block_means.iter().sum::<f64>() / block_means.len() as f64;
        let variance: f64 = block_means.iter().map(|&m| (m - global_mean).powi(2)).sum::<f64>()
            / block_means.len() as f64;

        // White noise block-average variance ≈ pixel_variance / block_size^2 ≈ 85
        // Blue noise should be much lower
        assert!(
            variance < 60.0,
            "Block-average variance {} too high — not blue noise quality (expected < 60)",
            variance
        );
    }

    #[test]
    fn uniform_histogram() {
        let data = test_texture(42);
        let frame = &data[0..TEST_W * TEST_H];

        let mut histogram = [0u32; 8]; // 8 bins for small texture
        for &v in frame {
            histogram[(v as usize) / 32] += 1;
        }

        let expected = (TEST_W * TEST_H) as f64 / 8.0;
        for (i, &count) in histogram.iter().enumerate() {
            let ratio = count as f64 / expected;
            assert!(
                ratio > 0.5 && ratio < 1.5,
                "Bin {} has {} entries ({:.1}% of expected) — distribution not uniform",
                i, count, ratio * 100.0
            );
        }
    }
}
