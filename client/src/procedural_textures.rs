#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use crate::material::TextureImage;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const TEX_SIZE: u32 = 512;
const TEX_PIXELS: usize = (TEX_SIZE * TEX_SIZE) as usize;
const TEX_BYTES: usize = TEX_PIXELS * 4;

// ---------------------------------------------------------------------------
// Deterministic hash / PRNG helpers
// ---------------------------------------------------------------------------

/// Simple deterministic hash (based on a variant of the Wang hash).
/// Produces a u32 from an arbitrary u64 input.
fn hash_u64(mut x: u64) -> u32 {
    x = x.wrapping_mul(0x517cc1b727220a95);
    x ^= x >> 32;
    x = x.wrapping_mul(0x6c62272e07bb0142);
    x ^= x >> 32;
    x as u32
}

/// Hash two i32 coordinates into a u32. Used for lattice noise.
fn hash_2d(x: i32, y: i32, seed: u32) -> u32 {
    let combined = ((x as u64) & 0xFFFF_FFFF)
        | (((y as u64) & 0xFFFF_FFFF) << 32);
    hash_u64(combined ^ (seed as u64))
}

/// Hash two i32 coordinates into a f32 in [0, 1).
fn hash_2d_f32(x: i32, y: i32, seed: u32) -> f32 {
    (hash_2d(x, y, seed) & 0x00FF_FFFF) as f32 / 16_777_216.0
}

/// Hash for Worley feature points — returns (fx, fy) in [0, 1) for a cell.
fn hash_cell_point(cx: i32, cy: i32, seed: u32) -> (f32, f32) {
    let h1 = hash_2d_f32(cx, cy, seed);
    let h2 = hash_2d_f32(cx, cy, seed.wrapping_add(0x9E3779B9));
    (h1, h2)
}

// ---------------------------------------------------------------------------
// Value Noise (tileable)
// ---------------------------------------------------------------------------

/// Smoothstep (Hermite interpolation).
fn smoothstep(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// Single-octave value noise, tileable over `period` in both axes.
/// `x`, `y` are in texture space [0, period).
fn value_noise(x: f32, y: f32, period: i32, seed: u32) -> f32 {
    let ix = x.floor() as i32;
    let iy = y.floor() as i32;
    let fx = x - x.floor();
    let fy = y - y.floor();

    let sx = smoothstep(fx);
    let sy = smoothstep(fy);

    let wrap = |v: i32| ((v % period) + period) % period;

    let x0 = wrap(ix);
    let x1 = wrap(ix + 1);
    let y0 = wrap(iy);
    let y1 = wrap(iy + 1);

    let v00 = hash_2d_f32(x0, y0, seed);
    let v10 = hash_2d_f32(x1, y0, seed);
    let v01 = hash_2d_f32(x0, y1, seed);
    let v11 = hash_2d_f32(x1, y1, seed);

    let a = v00 + (v10 - v00) * sx;
    let b = v01 + (v11 - v01) * sx;
    a + (b - a) * sy
}

/// Fractal Brownian Motion (FBM) using value noise.
/// Returns a value roughly in [0, 1].
fn fbm(x: f32, y: f32, octaves: u32, base_period: i32, lacunarity: f32, gain: f32, seed: u32) -> f32 {
    let mut sum = 0.0f32;
    let mut amplitude = 1.0f32;
    let mut frequency = 1.0f32;
    let mut max_val = 0.0f32;

    for i in 0..octaves {
        let period = (base_period as f32 * frequency) as i32;
        let period = if period < 1 { 1 } else { period };
        let oct_seed = seed.wrapping_add(i * 0x12345678);
        sum += amplitude * value_noise(x * frequency, y * frequency, period, oct_seed);
        max_val += amplitude;
        amplitude *= gain;
        frequency *= lacunarity;
    }

    sum / max_val
}

// ---------------------------------------------------------------------------
// Worley / Cellular Noise (tileable)
// ---------------------------------------------------------------------------

/// Worley (cellular) noise returning the distance to the nearest feature point.
/// Tileable over `period` cells.
/// Returns a value in approximately [0, 1].
fn worley(x: f32, y: f32, period: i32, seed: u32) -> f32 {
    let ix = x.floor() as i32;
    let iy = y.floor() as i32;
    let fx = x - x.floor();
    let fy = y - y.floor();

    let wrap = |v: i32| ((v % period) + period) % period;

    let mut min_dist = 2.0f32;

    for dy in -1..=1 {
        for dx in -1..=1 {
            let cx = wrap(ix + dx);
            let cy = wrap(iy + dy);
            let (px, py) = hash_cell_point(cx, cy, seed);
            let diff_x = dx as f32 + px - fx;
            let diff_y = dy as f32 + py - fy;
            let dist = (diff_x * diff_x + diff_y * diff_y).sqrt();
            if dist < min_dist {
                min_dist = dist;
            }
        }
    }

    // Normalize: max possible distance in a 3x3 neighbourhood is sqrt(2) ~ 1.414
    (min_dist / 1.414).min(1.0)
}

/// Worley noise returning the distance to the second-nearest feature point
/// minus the nearest, giving a "cell edge" pattern. Tileable.
fn worley_edge(x: f32, y: f32, period: i32, seed: u32) -> f32 {
    let ix = x.floor() as i32;
    let iy = y.floor() as i32;
    let fx = x - x.floor();
    let fy = y - y.floor();

    let wrap = |v: i32| ((v % period) + period) % period;

    let mut d1 = 2.0f32;
    let mut d2 = 2.0f32;

    for dy in -1..=1 {
        for dx in -1..=1 {
            let cx = wrap(ix + dx);
            let cy = wrap(iy + dy);
            let (px, py) = hash_cell_point(cx, cy, seed);
            let diff_x = dx as f32 + px - fx;
            let diff_y = dy as f32 + py - fy;
            let dist = (diff_x * diff_x + diff_y * diff_y).sqrt();
            if dist < d1 {
                d2 = d1;
                d1 = dist;
            } else if dist < d2 {
                d2 = dist;
            }
        }
    }

    ((d2 - d1) / 1.0).min(1.0)
}

// ---------------------------------------------------------------------------
// Utility helpers
// ---------------------------------------------------------------------------

fn clamp(v: f32, lo: f32, hi: f32) -> f32 {
    if v < lo { lo } else if v > hi { hi } else { v }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn to_u8(v: f32) -> u8 {
    clamp(v * 255.0, 0.0, 255.0) as u8
}

/// Map a float [0,1] into a u8 range [lo, hi].
fn map_range_u8(v: f32, lo: u8, hi: u8) -> u8 {
    let lo_f = lo as f32;
    let hi_f = hi as f32;
    clamp(lo_f + v * (hi_f - lo_f), lo_f, hi_f) as u8
}

/// Convert a heightfield to a tangent-space normal map.
/// `heights` is TEX_SIZE x TEX_SIZE floats in [0,1].
/// `strength` controls the bumpiness.
fn heightfield_to_normal_map(heights: &[f32], strength: f32) -> Vec<u8> {
    let w = TEX_SIZE as usize;
    let h = TEX_SIZE as usize;
    let mut data = vec![0u8; TEX_BYTES];

    for y in 0..h {
        for x in 0..w {
            let idx = y * w + x;
            // Tileable sampling
            let left = heights[y * w + ((x + w - 1) % w)];
            let right = heights[y * w + ((x + 1) % w)];
            let up = heights[((y + h - 1) % h) * w + x];
            let down = heights[((y + 1) % h) * w + x];

            let dx = (left - right) * strength;
            let dy = (up - down) * strength;
            // Tangent-space normal (Z-up)
            let len = (dx * dx + dy * dy + 1.0).sqrt();
            let nx = dx / len;
            let ny = dy / len;
            let nz = 1.0 / len;

            // Map [-1,1] to [0,255] for X and Y, Z is always positive
            let pixel = idx * 4;
            data[pixel] = to_u8(nx * 0.5 + 0.5);
            data[pixel + 1] = to_u8(ny * 0.5 + 0.5);
            data[pixel + 2] = to_u8(nz * 0.5 + 0.5); // always >= 128 since nz >= 0
            data[pixel + 3] = 255;
        }
    }
    data
}

/// Create a flat normal map (all normals pointing straight up).
fn flat_normal_map() -> Vec<u8> {
    let mut data = vec![0u8; TEX_BYTES];
    for i in 0..TEX_PIXELS {
        let p = i * 4;
        data[p] = 128;
        data[p + 1] = 128;
        data[p + 2] = 255;
        data[p + 3] = 255;
    }
    data
}

/// Generate a heightfield (TEX_SIZE x TEX_SIZE) using FBM.
fn generate_heightfield(octaves: u32, base_period: i32, lacunarity: f32, gain: f32, seed: u32) -> Vec<f32> {
    let w = TEX_SIZE as usize;
    let h = TEX_SIZE as usize;
    let mut heights = vec![0.0f32; w * h];

    for y in 0..h {
        for x in 0..w {
            let nx = x as f32 / TEX_SIZE as f32 * base_period as f32;
            let ny = y as f32 / TEX_SIZE as f32 * base_period as f32;
            heights[y * w + x] = fbm(nx, ny, octaves, base_period, lacunarity, gain, seed);
        }
    }
    heights
}

fn make_texture_image(data: Vec<u8>) -> TextureImage {
    debug_assert_eq!(data.len(), TEX_BYTES);
    TextureImage {
        width: TEX_SIZE,
        height: TEX_SIZE,
        rgba_data: data,
    }
}

// ---------------------------------------------------------------------------
// ProceduralTexture output struct
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct ProceduralTexture {
    pub albedo: TextureImage,
    pub normal: TextureImage,
    pub orm: TextureImage,
}

// ---------------------------------------------------------------------------
// 1. Grass Ground
// ---------------------------------------------------------------------------

pub fn generate_grass_textures() -> ProceduralTexture {
    let seed: u32 = 0x64A55_u32.wrapping_mul(0x9E37);
    let w = TEX_SIZE as usize;
    let h = TEX_SIZE as usize;

    // Heightfield for normal map
    let heights = generate_heightfield(6, 8, 2.0, 0.5, seed);

    let mut albedo = vec![0u8; TEX_BYTES];

    for y in 0..h {
        for x in 0..w {
            let nx = x as f32 / w as f32 * 8.0;
            let ny = y as f32 / h as f32 * 8.0;

            // Multi-octave green noise
            let grass_val = fbm(nx, ny, 6, 8, 2.0, 0.5, seed);
            // Brown dirt patches — lower frequency
            let dirt_val = fbm(nx * 0.5, ny * 0.5, 4, 4, 2.0, 0.6, seed.wrapping_add(100));
            // Decide if dirt or grass
            let dirt_mask = clamp((dirt_val - 0.55) * 5.0, 0.0, 1.0);

            // Grass color: varied greens
            let gr = lerp(0.15, 0.25, grass_val);
            let gg = lerp(0.35, 0.60, grass_val);
            let gb = lerp(0.08, 0.15, grass_val);

            // Dirt color: browns
            let dr = lerp(0.35, 0.50, dirt_val);
            let dg = lerp(0.25, 0.35, dirt_val);
            let db = lerp(0.10, 0.18, dirt_val);

            let r = lerp(gr, dr, dirt_mask);
            let g = lerp(gg, dg, dirt_mask);
            let b = lerp(gb, db, dirt_mask);

            let p = (y * w + x) * 4;
            albedo[p] = to_u8(r);
            albedo[p + 1] = to_u8(g);
            albedo[p + 2] = to_u8(b);
            albedo[p + 3] = 255;
        }
    }

    // ORM: AO 0.8-1.0, Roughness 0.85-0.95, Metallic 0.0
    let mut orm = vec![0u8; TEX_BYTES];
    for y in 0..h {
        for x in 0..w {
            let idx = y * w + x;
            let ao_noise = heights[idx]; // reuse heightfield for AO variation
            let p = idx * 4;
            orm[p] = map_range_u8(ao_noise, 204, 255);       // AO: 0.8-1.0
            orm[p + 1] = map_range_u8(ao_noise, 217, 242);   // R: 0.85-0.95
            orm[p + 2] = 0;                                   // M: 0.0
            orm[p + 3] = 255;
        }
    }

    let normal_data = heightfield_to_normal_map(&heights, 2.0);

    ProceduralTexture {
        albedo: make_texture_image(albedo),
        normal: make_texture_image(normal_data),
        orm: make_texture_image(orm),
    }
}

// ---------------------------------------------------------------------------
// 2. Tree Bark
// ---------------------------------------------------------------------------

pub fn generate_bark_textures() -> ProceduralTexture {
    let seed: u32 = 0xBA4C_u32.wrapping_mul(0x1337);
    let w = TEX_SIZE as usize;
    let h = TEX_SIZE as usize;

    let mut albedo = vec![0u8; TEX_BYTES];
    let mut heights = vec![0.0f32; TEX_PIXELS];

    for y in 0..h {
        for x in 0..w {
            let nx = x as f32 / w as f32 * 4.0;
            let ny = y as f32 / h as f32 * 16.0; // stretched vertically for bark streaks

            // Vertical streaks using stretched FBM
            let streak = fbm(nx, ny, 5, 4, 2.0, 0.5, seed);
            // Crevice detail — high frequency
            let crevice = fbm(nx * 2.0, ny * 2.0, 4, 8, 2.0, 0.5, seed.wrapping_add(50));
            let crevice_mask = clamp((crevice - 0.4) * 3.0, 0.0, 1.0);

            // Bark color: browns
            let base_r = lerp(0.30, 0.45, streak);
            let base_g = lerp(0.18, 0.30, streak);
            let base_b = lerp(0.08, 0.15, streak);

            // Dark crevices
            let r = lerp(base_r, base_r * 0.3, crevice_mask);
            let g = lerp(base_g, base_g * 0.3, crevice_mask);
            let b = lerp(base_b, base_b * 0.3, crevice_mask);

            let idx = y * w + x;
            heights[idx] = streak * (1.0 - crevice_mask * 0.6);

            let p = idx * 4;
            albedo[p] = to_u8(r);
            albedo[p + 1] = to_u8(g);
            albedo[p + 2] = to_u8(b);
            albedo[p + 3] = 255;
        }
    }

    // ORM: AO 0.7-1.0, Roughness 0.8-0.95, Metallic 0.0
    let mut orm = vec![0u8; TEX_BYTES];
    for y in 0..h {
        for x in 0..w {
            let idx = y * w + x;
            let v = heights[idx];
            let p = idx * 4;
            orm[p] = map_range_u8(v, 179, 255);       // AO: 0.7-1.0
            orm[p + 1] = map_range_u8(v, 204, 242);   // R: 0.8-0.95
            orm[p + 2] = 0;                            // M: 0.0
            orm[p + 3] = 255;
        }
    }

    let normal_data = heightfield_to_normal_map(&heights, 3.0);

    ProceduralTexture {
        albedo: make_texture_image(albedo),
        normal: make_texture_image(normal_data),
        orm: make_texture_image(orm),
    }
}

// ---------------------------------------------------------------------------
// 3. Leaf Canopy
// ---------------------------------------------------------------------------

pub fn generate_leaf_textures() -> ProceduralTexture {
    let seed: u32 = 0x1EAF_u32.wrapping_mul(0xBEEF);
    let w = TEX_SIZE as usize;
    let h = TEX_SIZE as usize;

    let mut albedo = vec![0u8; TEX_BYTES];
    let mut heights = vec![0.0f32; TEX_PIXELS];

    for y in 0..h {
        for x in 0..w {
            let nx = x as f32 / w as f32 * 8.0;
            let ny = y as f32 / h as f32 * 8.0;

            // Scattered leaf clusters using Worley
            let cell = worley(nx, ny, 8, seed);
            // Green variation with FBM
            let green_var = fbm(nx, ny, 5, 8, 2.0, 0.5, seed.wrapping_add(200));
            // Seasonal yellowing patches
            let yellow_mask = fbm(nx * 0.5, ny * 0.5, 3, 4, 2.0, 0.5, seed.wrapping_add(300));
            let yellow_t = clamp((yellow_mask - 0.6) * 4.0, 0.0, 1.0);

            // Leaf vein pattern for heightfield
            let vein = worley_edge(nx * 2.0, ny * 2.0, 16, seed.wrapping_add(400));
            heights[y * w + x] = cell * 0.6 + vein * 0.4;

            // Base green leaf color
            let lr = lerp(0.10, 0.20, green_var);
            let lg = lerp(0.40, 0.60, green_var);
            let lb = lerp(0.05, 0.12, green_var);

            // Yellow autumn color
            let yr = 0.60;
            let yg = 0.55;
            let yb = 0.10;

            // Blend based on cell distance (leaf edges darker)
            let edge_dark = clamp(1.0 - cell * 1.5, 0.0, 0.3);
            let r = lerp(lr, yr, yellow_t) - edge_dark * 0.1;
            let g = lerp(lg, yg, yellow_t) - edge_dark * 0.1;
            let b = lerp(lb, yb, yellow_t);

            let p = (y * w + x) * 4;
            albedo[p] = to_u8(clamp(r, 0.0, 1.0));
            albedo[p + 1] = to_u8(clamp(g, 0.0, 1.0));
            albedo[p + 2] = to_u8(clamp(b, 0.0, 1.0));
            albedo[p + 3] = 255;
        }
    }

    // ORM: AO varies, Roughness 0.6-0.8, Metallic 0.0
    let mut orm = vec![0u8; TEX_BYTES];
    for y in 0..h {
        for x in 0..w {
            let idx = y * w + x;
            let v = heights[idx];
            let p = idx * 4;
            orm[p] = map_range_u8(v, 200, 255);       // AO: ~0.8-1.0
            orm[p + 1] = map_range_u8(v, 153, 204);   // R: 0.6-0.8
            orm[p + 2] = 0;                            // M: 0.0
            orm[p + 3] = 255;
        }
    }

    let normal_data = heightfield_to_normal_map(&heights, 1.5);

    ProceduralTexture {
        albedo: make_texture_image(albedo),
        normal: make_texture_image(normal_data),
        orm: make_texture_image(orm),
    }
}

// ---------------------------------------------------------------------------
// 4. Rock Surface
// ---------------------------------------------------------------------------

pub fn generate_rock_textures() -> ProceduralTexture {
    let seed: u32 = 0x40CC_u32.wrapping_mul(0xDEAD);
    let w = TEX_SIZE as usize;
    let h = TEX_SIZE as usize;

    let mut albedo = vec![0u8; TEX_BYTES];
    let mut heights = vec![0.0f32; TEX_PIXELS];

    for y in 0..h {
        for x in 0..w {
            let nx = x as f32 / w as f32 * 8.0;
            let ny = y as f32 / h as f32 * 8.0;

            // Fractal grey rock
            let rock_val = fbm(nx, ny, 7, 8, 2.0, 0.5, seed);
            // Crack pattern using Worley edge
            let crack = worley_edge(nx, ny, 8, seed.wrapping_add(500));
            let crack_depth = clamp(1.0 - crack * 3.0, 0.0, 1.0);

            // Moss at edges (low-frequency patches) — use integer-scaled coords for tileability
            let moss_mask = fbm(nx * 0.5, ny * 0.5, 3, 4, 2.0, 0.5, seed.wrapping_add(600));
            let moss_t = clamp((moss_mask - 0.5) * 3.0, 0.0, 1.0);

            // Height combines rock surface and cracks
            heights[y * w + x] = rock_val * (1.0 - crack_depth * 0.5);

            // Grey rock base
            let grey = lerp(0.35, 0.55, rock_val);
            let rr = grey - crack_depth * 0.15;
            let rg = grey - crack_depth * 0.15;
            let rb = grey - crack_depth * 0.12;

            // Moss green overlay
            let mr = 0.15;
            let mg = 0.35;
            let mb = 0.08;

            let r = lerp(rr, mr, moss_t);
            let g = lerp(rg, mg, moss_t);
            let b = lerp(rb, mb, moss_t);

            let p = (y * w + x) * 4;
            albedo[p] = to_u8(clamp(r, 0.0, 1.0));
            albedo[p + 1] = to_u8(clamp(g, 0.0, 1.0));
            albedo[p + 2] = to_u8(clamp(b, 0.0, 1.0));
            albedo[p + 3] = 255;
        }
    }

    // ORM: AO varies, Roughness 0.6-0.9, Metallic 0.0
    let mut orm = vec![0u8; TEX_BYTES];
    for y in 0..h {
        for x in 0..w {
            let idx = y * w + x;
            let v = heights[idx];
            let p = idx * 4;
            orm[p] = map_range_u8(v, 180, 255);       // AO: ~0.7-1.0
            orm[p + 1] = map_range_u8(v, 153, 230);   // R: 0.6-0.9
            orm[p + 2] = 0;                            // M: 0.0
            orm[p + 3] = 255;
        }
    }

    let normal_data = heightfield_to_normal_map(&heights, 4.0);

    ProceduralTexture {
        albedo: make_texture_image(albedo),
        normal: make_texture_image(normal_data),
        orm: make_texture_image(orm),
    }
}

// ---------------------------------------------------------------------------
// 5. Player Skin
// ---------------------------------------------------------------------------

pub fn generate_player_skin_textures() -> ProceduralTexture {
    let seed: u32 = 0x5C10_u32.wrapping_mul(0xFACE);
    let w = TEX_SIZE as usize;
    let h = TEX_SIZE as usize;

    let mut albedo = vec![0u8; TEX_BYTES];
    let mut heights = vec![0.0f32; TEX_PIXELS];

    for y in 0..h {
        for x in 0..w {
            let nx = x as f32 / w as f32 * 16.0;
            let ny = y as f32 / h as f32 * 16.0;

            // Subtle skin noise
            let skin_noise = fbm(nx, ny, 4, 16, 2.0, 0.45, seed);
            // Very fine pore detail
            let pore = fbm(nx * 4.0, ny * 4.0, 3, 64, 2.0, 0.5, seed.wrapping_add(700));

            heights[y * w + x] = pore * 0.3 + skin_noise * 0.7;

            // Warm skin tones
            let r = lerp(0.72, 0.82, skin_noise);
            let g = lerp(0.55, 0.62, skin_noise);
            let b = lerp(0.45, 0.52, skin_noise);

            // Subtle pore darkening
            let pore_dark = clamp((0.5 - pore) * 0.08, 0.0, 0.05);

            let p = (y * w + x) * 4;
            albedo[p] = to_u8(clamp(r - pore_dark, 0.0, 1.0));
            albedo[p + 1] = to_u8(clamp(g - pore_dark, 0.0, 1.0));
            albedo[p + 2] = to_u8(clamp(b - pore_dark, 0.0, 1.0));
            albedo[p + 3] = 255;
        }
    }

    // ORM: AO ~0.9-1.0, Roughness 0.4-0.6, Metallic 0.0
    let mut orm = vec![0u8; TEX_BYTES];
    for y in 0..h {
        for x in 0..w {
            let idx = y * w + x;
            let v = heights[idx];
            let p = idx * 4;
            orm[p] = map_range_u8(v, 230, 255);       // AO: ~0.9-1.0
            orm[p + 1] = map_range_u8(v, 102, 153);   // R: 0.4-0.6
            orm[p + 2] = 0;                            // M: 0.0
            orm[p + 3] = 255;
        }
    }

    // Near-flat normal with pore detail (low strength)
    let normal_data = heightfield_to_normal_map(&heights, 0.5);

    ProceduralTexture {
        albedo: make_texture_image(albedo),
        normal: make_texture_image(normal_data),
        orm: make_texture_image(orm),
    }
}

// ---------------------------------------------------------------------------
// 6. Player Armor
// ---------------------------------------------------------------------------

pub fn generate_player_armor_textures() -> ProceduralTexture {
    let seed: u32 = 0xA440_u32.wrapping_mul(0xBEEF);
    let w = TEX_SIZE as usize;
    let h = TEX_SIZE as usize;

    let mut albedo = vec![0u8; TEX_BYTES];
    let mut heights = vec![0.0f32; TEX_PIXELS];

    for y in 0..h {
        for x in 0..w {
            let nx = x as f32 / w as f32 * 8.0;
            let ny = y as f32 / h as f32 * 8.0;

            // Plate pattern using Worley cells
            let plate = worley(nx, ny, 8, seed);
            let plate_edge = worley_edge(nx, ny, 8, seed);

            // Edge wear — noise at edges
            let wear_noise = fbm(nx * 2.0, ny * 2.0, 4, 16, 2.0, 0.5, seed.wrapping_add(800));
            let edge_wear = clamp((1.0 - plate_edge * 4.0) * wear_noise, 0.0, 1.0);

            // Rivet pattern at regular intervals (use simple grid hash)
            let rivet_x = (nx * 2.0).fract();
            let rivet_y = (ny * 2.0).fract();
            let rivet_dist = ((rivet_x - 0.5) * (rivet_x - 0.5) + (rivet_y - 0.5) * (rivet_y - 0.5)).sqrt();
            let rivet = clamp(1.0 - rivet_dist * 8.0, 0.0, 1.0);

            heights[y * w + x] = plate * 0.5 + (1.0 - plate_edge.min(1.0)) * 0.3 + rivet * 0.2;

            // Dark steel blue base
            let base_r = lerp(0.12, 0.18, plate);
            let base_g = lerp(0.15, 0.22, plate);
            let base_b = lerp(0.25, 0.35, plate);

            // Edge wear reveals brighter metal
            let r = lerp(base_r, 0.45, edge_wear);
            let g = lerp(base_g, 0.42, edge_wear);
            let b = lerp(base_b, 0.40, edge_wear);

            // Rivets are slightly brighter
            let r = lerp(r, 0.35, rivet * 0.5);
            let g = lerp(g, 0.35, rivet * 0.5);
            let b = lerp(b, 0.38, rivet * 0.5);

            let p = (y * w + x) * 4;
            albedo[p] = to_u8(clamp(r, 0.0, 1.0));
            albedo[p + 1] = to_u8(clamp(g, 0.0, 1.0));
            albedo[p + 2] = to_u8(clamp(b, 0.0, 1.0));
            albedo[p + 3] = 255;
        }
    }

    // ORM: AO varies, Roughness 0.3-0.5, Metallic 0.6-0.8
    let mut orm = vec![0u8; TEX_BYTES];
    for y in 0..h {
        for x in 0..w {
            let idx = y * w + x;
            let v = heights[idx];
            let p = idx * 4;
            orm[p] = map_range_u8(v, 180, 255);       // AO: ~0.7-1.0
            orm[p + 1] = map_range_u8(v, 77, 128);    // R: 0.3-0.5
            orm[p + 2] = map_range_u8(v, 153, 204);   // M: 0.6-0.8
            orm[p + 3] = 255;
        }
    }

    let normal_data = heightfield_to_normal_map(&heights, 3.0);

    ProceduralTexture {
        albedo: make_texture_image(albedo),
        normal: make_texture_image(normal_data),
        orm: make_texture_image(orm),
    }
}

// ---------------------------------------------------------------------------
// 7. Enemy Skin
// ---------------------------------------------------------------------------

pub fn generate_enemy_skin_textures() -> ProceduralTexture {
    let seed: u32 = 0xE4E3_u32.wrapping_mul(0xD00D);
    let w = TEX_SIZE as usize;
    let h = TEX_SIZE as usize;

    let mut albedo = vec![0u8; TEX_BYTES];
    let mut heights = vec![0.0f32; TEX_PIXELS];

    for y in 0..h {
        for x in 0..w {
            let nx = x as f32 / w as f32 * 8.0;
            let ny = y as f32 / h as f32 * 8.0;

            // Organic base using FBM
            let organic = fbm(nx, ny, 6, 8, 2.0, 0.5, seed);
            // Vein pattern using Worley edge (inverted = veins are bright on dark)
            let vein = worley_edge(nx * 1.5, ny * 1.5, 12, seed.wrapping_add(900));
            let vein_intensity = clamp(1.0 - vein * 5.0, 0.0, 1.0);

            // Tendril ridges for normal map
            let tendril = worley_edge(nx * 2.0, ny * 2.0, 16, seed.wrapping_add(1000));
            heights[y * w + x] = organic * 0.5 + (1.0 - tendril) * 0.3 + vein_intensity * 0.2;

            // Dark red/purple organic base
            let base_r = lerp(0.30, 0.45, organic);
            let base_g = lerp(0.05, 0.12, organic);
            let base_b = lerp(0.12, 0.25, organic);

            // Veins are darker/more purple
            let r = lerp(base_r, 0.20, vein_intensity);
            let g = lerp(base_g, 0.02, vein_intensity);
            let b = lerp(base_b, 0.18, vein_intensity);

            let p = (y * w + x) * 4;
            albedo[p] = to_u8(clamp(r, 0.0, 1.0));
            albedo[p + 1] = to_u8(clamp(g, 0.0, 1.0));
            albedo[p + 2] = to_u8(clamp(b, 0.0, 1.0));
            albedo[p + 3] = 255;
        }
    }

    // ORM: AO varies, Roughness 0.5-0.7, Metallic 0.1
    let mut orm = vec![0u8; TEX_BYTES];
    for y in 0..h {
        for x in 0..w {
            let idx = y * w + x;
            let v = heights[idx];
            let p = idx * 4;
            orm[p] = map_range_u8(v, 190, 255);       // AO: ~0.75-1.0
            orm[p + 1] = map_range_u8(v, 128, 179);   // R: 0.5-0.7
            orm[p + 2] = 26;                           // M: ~0.1
            orm[p + 3] = 255;
        }
    }

    let normal_data = heightfield_to_normal_map(&heights, 3.5);

    ProceduralTexture {
        albedo: make_texture_image(albedo),
        normal: make_texture_image(normal_data),
        orm: make_texture_image(orm),
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn assert_dimensions(img: &TextureImage) {
        assert_eq!(img.width, TEX_SIZE, "width must be {}", TEX_SIZE);
        assert_eq!(img.height, TEX_SIZE, "height must be {}", TEX_SIZE);
        assert_eq!(
            img.rgba_data.len(),
            TEX_BYTES,
            "data length must be width*height*4 = {}",
            TEX_BYTES
        );
    }

    fn assert_normal_z_ge_128(img: &TextureImage) {
        for (i, chunk) in img.rgba_data.chunks(4).enumerate() {
            assert!(
                chunk[2] >= 128,
                "Normal Z at pixel {} is {} (must be >= 128)",
                i,
                chunk[2]
            );
        }
    }

    fn assert_orm_ranges(img: &TextureImage, ao_min: u8, ao_max: u8, r_min: u8, r_max: u8, m_min: u8, m_max: u8) {
        for (i, chunk) in img.rgba_data.chunks(4).enumerate() {
            assert!(
                chunk[0] >= ao_min && chunk[0] <= ao_max,
                "ORM AO at pixel {} is {} (expected {}-{})",
                i, chunk[0], ao_min, ao_max
            );
            assert!(
                chunk[1] >= r_min && chunk[1] <= r_max,
                "ORM Roughness at pixel {} is {} (expected {}-{})",
                i, chunk[1], r_min, r_max
            );
            assert!(
                chunk[2] >= m_min && chunk[2] <= m_max,
                "ORM Metallic at pixel {} is {} (expected {}-{})",
                i, chunk[2], m_min, m_max
            );
        }
    }

    fn assert_alpha_opaque(img: &TextureImage) {
        for (i, chunk) in img.rgba_data.chunks(4).enumerate() {
            assert_eq!(
                chunk[3], 255,
                "Alpha at pixel {} is {} (expected 255)",
                i, chunk[3]
            );
        }
    }

    /// Check that the texture tiles by comparing a strip at the left edge
    /// with a strip near the right edge. Due to tileable noise, the values
    /// should be within a tolerance (since adjacent pixels should be
    /// continuous).
    fn assert_tileable(img: &TextureImage, tolerance: u8) {
        let w = img.width as usize;
        let h = img.height as usize;
        // Check horizontal tiling: compare pixel at x=0 with x=width-1
        // They should be close since noise is continuous and wraps.
        let mut max_diff: u8 = 0;
        for y in 0..h {
            let p0 = (y * w) * 4;
            let p1 = (y * w + (w - 1)) * 4;
            for c in 0..3 {
                let diff = (img.rgba_data[p0 + c] as i16 - img.rgba_data[p1 + c] as i16).unsigned_abs() as u8;
                if diff > max_diff {
                    max_diff = diff;
                }
            }
        }
        // For tileable noise at 512px, adjacent-pixel diffs shouldn't be huge.
        // We use a generous tolerance since the edge is 1 pixel apart in wrapped space.
        assert!(
            max_diff <= tolerance,
            "Horizontal tile seam max diff {} exceeds tolerance {}",
            max_diff, tolerance
        );

        // Check vertical tiling
        max_diff = 0;
        for x in 0..w {
            let p0 = x * 4;
            let p1 = ((h - 1) * w + x) * 4;
            for c in 0..3 {
                let diff = (img.rgba_data[p0 + c] as i16 - img.rgba_data[p1 + c] as i16).unsigned_abs() as u8;
                if diff > max_diff {
                    max_diff = diff;
                }
            }
        }
        assert!(
            max_diff <= tolerance,
            "Vertical tile seam max diff {} exceeds tolerance {}",
            max_diff, tolerance
        );
    }

    fn assert_deterministic<F: Fn() -> ProceduralTexture>(generator: F) {
        let a = generator();
        let b = generator();
        assert_eq!(a.albedo.rgba_data, b.albedo.rgba_data, "Albedo not deterministic");
        assert_eq!(a.normal.rgba_data, b.normal.rgba_data, "Normal not deterministic");
        assert_eq!(a.orm.rgba_data, b.orm.rgba_data, "ORM not deterministic");
    }

    fn full_check(
        tex: &ProceduralTexture,
        ao_min: u8, ao_max: u8,
        r_min: u8, r_max: u8,
        m_min: u8, m_max: u8,
        tile_tolerance: u8,
    ) {
        // Dimensions
        assert_dimensions(&tex.albedo);
        assert_dimensions(&tex.normal);
        assert_dimensions(&tex.orm);

        // Alpha
        assert_alpha_opaque(&tex.albedo);
        assert_alpha_opaque(&tex.normal);
        assert_alpha_opaque(&tex.orm);

        // Normal Z
        assert_normal_z_ge_128(&tex.normal);

        // ORM ranges
        assert_orm_ranges(&tex.orm, ao_min, ao_max, r_min, r_max, m_min, m_max);

        // Tileability
        assert_tileable(&tex.albedo, tile_tolerance);
        assert_tileable(&tex.normal, tile_tolerance);
    }

    // -----------------------------------------------------------------------
    // Noise unit tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_value_noise_range() {
        for i in 0..1000 {
            let x = (i as f32) * 0.13;
            let y = (i as f32) * 0.07;
            let v = value_noise(x, y, 8, 42);
            assert!(v >= 0.0 && v <= 1.0, "value_noise out of range: {}", v);
        }
    }

    #[test]
    fn test_value_noise_deterministic() {
        let a = value_noise(1.5, 2.3, 8, 42);
        let b = value_noise(1.5, 2.3, 8, 42);
        assert_eq!(a, b);
    }

    #[test]
    fn test_fbm_range() {
        for i in 0..1000 {
            let x = (i as f32) * 0.1;
            let y = (i as f32) * 0.2;
            let v = fbm(x, y, 6, 8, 2.0, 0.5, 0);
            assert!(v >= 0.0 && v <= 1.0, "fbm out of range: {}", v);
        }
    }

    #[test]
    fn test_worley_range() {
        for i in 0..1000 {
            let x = (i as f32) * 0.05;
            let y = (i as f32) * 0.03;
            let v = worley(x, y, 8, 42);
            assert!(v >= 0.0 && v <= 1.0, "worley out of range: {}", v);
        }
    }

    #[test]
    fn test_worley_edge_range() {
        for i in 0..1000 {
            let x = (i as f32) * 0.05;
            let y = (i as f32) * 0.03;
            let v = worley_edge(x, y, 8, 42);
            assert!(v >= 0.0 && v <= 1.0, "worley_edge out of range: {}", v);
        }
    }

    #[test]
    fn test_hash_deterministic() {
        let a = hash_2d(10, 20, 42);
        let b = hash_2d(10, 20, 42);
        assert_eq!(a, b);
    }

    #[test]
    fn test_hash_different_seeds_differ() {
        let a = hash_2d(10, 20, 42);
        let b = hash_2d(10, 20, 43);
        assert_ne!(a, b);
    }

    #[test]
    fn test_smoothstep_boundaries() {
        assert_eq!(smoothstep(0.0), 0.0);
        assert_eq!(smoothstep(1.0), 1.0);
        let mid = smoothstep(0.5);
        assert!((mid - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_heightfield_to_normal_flat() {
        let heights = vec![0.5; TEX_PIXELS];
        let normal = heightfield_to_normal_map(&heights, 1.0);
        // Flat heightfield should produce all (128, 128, 255) normals
        for chunk in normal.chunks(4) {
            // Small tolerance for floating point
            assert!((chunk[0] as i16 - 128).unsigned_abs() <= 1);
            assert!((chunk[1] as i16 - 128).unsigned_abs() <= 1);
            assert_eq!(chunk[2], 255);
            assert_eq!(chunk[3], 255);
        }
    }

    // -----------------------------------------------------------------------
    // Grass textures
    // -----------------------------------------------------------------------

    #[test]
    fn test_grass_dimensions_and_normals() {
        let tex = generate_grass_textures();
        full_check(&tex, 204, 255, 217, 242, 0, 0, 40);
    }

    #[test]
    fn test_grass_deterministic() {
        assert_deterministic(generate_grass_textures);
    }

    // -----------------------------------------------------------------------
    // Bark textures
    // -----------------------------------------------------------------------

    #[test]
    fn test_bark_dimensions_and_normals() {
        let tex = generate_bark_textures();
        full_check(&tex, 179, 255, 204, 242, 0, 0, 60);
    }

    #[test]
    fn test_bark_deterministic() {
        assert_deterministic(generate_bark_textures);
    }

    // -----------------------------------------------------------------------
    // Leaf textures
    // -----------------------------------------------------------------------

    #[test]
    fn test_leaf_dimensions_and_normals() {
        let tex = generate_leaf_textures();
        full_check(&tex, 200, 255, 153, 204, 0, 0, 50);
    }

    #[test]
    fn test_leaf_deterministic() {
        assert_deterministic(generate_leaf_textures);
    }

    // -----------------------------------------------------------------------
    // Rock textures
    // -----------------------------------------------------------------------

    #[test]
    fn test_rock_dimensions_and_normals() {
        let tex = generate_rock_textures();
        full_check(&tex, 180, 255, 153, 230, 0, 0, 50);
    }

    #[test]
    fn test_rock_deterministic() {
        assert_deterministic(generate_rock_textures);
    }

    // -----------------------------------------------------------------------
    // Player skin textures
    // -----------------------------------------------------------------------

    #[test]
    fn test_player_skin_dimensions_and_normals() {
        let tex = generate_player_skin_textures();
        full_check(&tex, 230, 255, 102, 153, 0, 0, 30);
    }

    #[test]
    fn test_player_skin_deterministic() {
        assert_deterministic(generate_player_skin_textures);
    }

    // -----------------------------------------------------------------------
    // Player armor textures
    // -----------------------------------------------------------------------

    #[test]
    fn test_player_armor_dimensions_and_normals() {
        let tex = generate_player_armor_textures();
        full_check(&tex, 180, 255, 77, 128, 153, 204, 50);
    }

    #[test]
    fn test_player_armor_deterministic() {
        assert_deterministic(generate_player_armor_textures);
    }

    // -----------------------------------------------------------------------
    // Enemy skin textures
    // -----------------------------------------------------------------------

    #[test]
    fn test_enemy_skin_dimensions_and_normals() {
        let tex = generate_enemy_skin_textures();
        full_check(&tex, 190, 255, 128, 179, 26, 26, 50);
    }

    #[test]
    fn test_enemy_skin_deterministic() {
        assert_deterministic(generate_enemy_skin_textures);
    }

    // -----------------------------------------------------------------------
    // Cross-texture visual distinction
    // -----------------------------------------------------------------------

    #[test]
    fn test_all_textures_visually_distinct() {
        // Generate all textures and check that their average albedo colors differ
        let textures: Vec<ProceduralTexture> = vec![
            generate_grass_textures(),
            generate_bark_textures(),
            generate_leaf_textures(),
            generate_rock_textures(),
            generate_player_skin_textures(),
            generate_player_armor_textures(),
            generate_enemy_skin_textures(),
        ];

        let averages: Vec<(f64, f64, f64)> = textures
            .iter()
            .map(|t| {
                let mut r_sum = 0u64;
                let mut g_sum = 0u64;
                let mut b_sum = 0u64;
                for chunk in t.albedo.rgba_data.chunks(4) {
                    r_sum += chunk[0] as u64;
                    g_sum += chunk[1] as u64;
                    b_sum += chunk[2] as u64;
                }
                let n = TEX_PIXELS as f64;
                (r_sum as f64 / n, g_sum as f64 / n, b_sum as f64 / n)
            })
            .collect();

        // Each pair of textures should differ by at least some amount in average color
        for i in 0..averages.len() {
            for j in (i + 1)..averages.len() {
                let dr = averages[i].0 - averages[j].0;
                let dg = averages[i].1 - averages[j].1;
                let db = averages[i].2 - averages[j].2;
                let dist = (dr * dr + dg * dg + db * db).sqrt();
                assert!(
                    dist > 3.0,
                    "Textures {} and {} have too-similar average color (dist={})",
                    i, j, dist
                );
            }
        }
    }

    // -----------------------------------------------------------------------
    // RGBA data length sanity
    // -----------------------------------------------------------------------

    #[test]
    fn test_all_rgba_data_lengths() {
        let expected = (TEX_SIZE * TEX_SIZE * 4) as usize;
        let generators: Vec<fn() -> ProceduralTexture> = vec![
            generate_grass_textures,
            generate_bark_textures,
            generate_leaf_textures,
            generate_rock_textures,
            generate_player_skin_textures,
            generate_player_armor_textures,
            generate_enemy_skin_textures,
        ];
        for (i, generator) in generators.iter().enumerate() {
            let tex = generator();
            assert_eq!(tex.albedo.rgba_data.len(), expected, "Albedo len wrong for texture {}", i);
            assert_eq!(tex.normal.rgba_data.len(), expected, "Normal len wrong for texture {}", i);
            assert_eq!(tex.orm.rgba_data.len(), expected, "ORM len wrong for texture {}", i);
        }
    }
}
