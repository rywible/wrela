#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

/// Curated anime palette colors and subtle noise overlay.
///
/// Provides palette lookup (nearest-in-hue) and procedural noise functions
/// for adding analog warmth to cel-shaded output. Supports deterministic
/// mode (fixed seed) and expressive mode (blue noise pattern).

/// A single palette swatch with HSL-keyed lookup metadata.
#[derive(Clone, Copy, Debug)]
pub struct PaletteSwatch {
    /// Linear RGB color.
    pub rgb: [f32; 3],
    /// Hue in [0, 1] for nearest-hue lookup.
    pub hue: f32,
    /// Semantic category.
    pub category: PaletteCategory,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PaletteCategory {
    Shadow,
    Mid,
    Highlight,
}

/// Curated anime palette: warm/cool/accent × shadow/mid/highlight.
/// Colors chosen for pleasing cel-shaded aesthetics across diverse scenes.
pub const ANIME_PALETTE: [PaletteSwatch; 12] = [
    // Warm shadow
    PaletteSwatch { rgb: [0.25, 0.12, 0.10], hue: 0.02, category: PaletteCategory::Shadow },
    // Warm mid
    PaletteSwatch { rgb: [0.75, 0.45, 0.30], hue: 0.06, category: PaletteCategory::Mid },
    // Warm highlight
    PaletteSwatch { rgb: [1.00, 0.85, 0.65], hue: 0.10, category: PaletteCategory::Highlight },
    // Cool shadow
    PaletteSwatch { rgb: [0.10, 0.12, 0.25], hue: 0.63, category: PaletteCategory::Shadow },
    // Cool mid
    PaletteSwatch { rgb: [0.35, 0.50, 0.75], hue: 0.61, category: PaletteCategory::Mid },
    // Cool highlight
    PaletteSwatch { rgb: [0.70, 0.85, 1.00], hue: 0.58, category: PaletteCategory::Highlight },
    // Accent shadow (purple)
    PaletteSwatch { rgb: [0.18, 0.08, 0.22], hue: 0.79, category: PaletteCategory::Shadow },
    // Accent mid (violet)
    PaletteSwatch { rgb: [0.55, 0.30, 0.65], hue: 0.78, category: PaletteCategory::Mid },
    // Accent highlight (lavender)
    PaletteSwatch { rgb: [0.85, 0.70, 0.95], hue: 0.77, category: PaletteCategory::Highlight },
    // Green shadow
    PaletteSwatch { rgb: [0.08, 0.15, 0.06], hue: 0.30, category: PaletteCategory::Shadow },
    // Green mid
    PaletteSwatch { rgb: [0.30, 0.55, 0.25], hue: 0.30, category: PaletteCategory::Mid },
    // Green highlight
    PaletteSwatch { rgb: [0.65, 0.90, 0.55], hue: 0.30, category: PaletteCategory::Highlight },
];

/// Find the nearest palette swatch by hue distance, optionally filtered by category.
pub fn nearest_by_hue(hue: f32, category: Option<PaletteCategory>) -> &'static PaletteSwatch {
    let mut best = &ANIME_PALETTE[0];
    let mut best_dist = f32::MAX;

    for swatch in &ANIME_PALETTE {
        if let Some(cat) = category {
            if swatch.category != cat {
                continue;
            }
        }
        // Circular hue distance
        let d = (swatch.hue - hue).abs();
        let dist = d.min(1.0 - d);
        if dist < best_dist {
            best_dist = dist;
            best = swatch;
        }
    }
    best
}

/// Deterministic hash noise: produces a value in [0, amplitude] given a seed.
/// Uses a simple integer hash (Wang hash variant) for reproducibility.
pub fn hash_noise(seed: u32, amplitude: f32) -> f32 {
    let mut h = seed;
    h = h.wrapping_mul(747796405).wrapping_add(2891336453);
    h = ((h >> 16) ^ h).wrapping_mul(2654435769);
    h = (h >> 16) ^ h;
    (h as f32 / u32::MAX as f32) * amplitude
}

/// Subtle noise overlay value for a pixel coordinate.
///
/// In deterministic mode (`deterministic = true`), uses `hash_noise` with
/// the frame seed for consistent results across runs.
///
/// In expressive mode, uses a blue-noise-like pattern via interleaved
/// gradient noise (Jorge Jimenez, Interleaved Gradient Noise).
pub fn pixel_noise(x: u32, y: u32, frame_seed: u32, deterministic: bool, amplitude: f32) -> f32 {
    if deterministic {
        // Deterministic: hash of pixel + seed
        let seed = x.wrapping_mul(73856093) ^ y.wrapping_mul(19349663) ^ frame_seed;
        hash_noise(seed, amplitude)
    } else {
        // Expressive: interleaved gradient noise (IGN)
        // Produces a well-distributed pattern that avoids banding
        let xf = x as f32;
        let yf = y as f32;
        let f = frame_seed as f32 * 0.618033988;
        let magic = ((xf + 5.588238 * yf + f) * 0.06711056).fract() * 52.9829189;
        magic.fract() * amplitude
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_has_12_entries() {
        assert_eq!(ANIME_PALETTE.len(), 12);
    }

    #[test]
    fn palette_has_all_categories() {
        let shadows = ANIME_PALETTE.iter().filter(|s| s.category == PaletteCategory::Shadow).count();
        let mids = ANIME_PALETTE.iter().filter(|s| s.category == PaletteCategory::Mid).count();
        let highlights = ANIME_PALETTE.iter().filter(|s| s.category == PaletteCategory::Highlight).count();
        assert_eq!(shadows, 4);
        assert_eq!(mids, 4);
        assert_eq!(highlights, 4);
    }

    #[test]
    fn nearest_by_hue_finds_warm() {
        let result = nearest_by_hue(0.05, Some(PaletteCategory::Mid));
        // Should find warm mid (hue 0.06)
        assert!((result.hue - 0.06).abs() < 0.05);
    }

    #[test]
    fn nearest_by_hue_finds_cool() {
        let result = nearest_by_hue(0.60, Some(PaletteCategory::Shadow));
        // Should find cool shadow (hue 0.63)
        assert!((result.hue - 0.63).abs() < 0.05);
    }

    #[test]
    fn nearest_by_hue_wraps_around() {
        // Hue 0.98 should be close to warm shadow (hue 0.02) via circular distance
        let result = nearest_by_hue(0.98, Some(PaletteCategory::Shadow));
        assert!((result.hue - 0.02).abs() < 0.05);
    }

    #[test]
    fn nearest_by_hue_no_category_filter() {
        let result = nearest_by_hue(0.30, None);
        assert!((result.hue - 0.30).abs() < 0.01);
    }

    #[test]
    fn hash_noise_is_deterministic() {
        let a = hash_noise(42, 0.02);
        let b = hash_noise(42, 0.02);
        assert_eq!(a, b);
    }

    #[test]
    fn hash_noise_in_range() {
        for seed in 0..1000u32 {
            let v = hash_noise(seed, 0.02);
            assert!(v >= 0.0 && v <= 0.02, "hash_noise({}) = {} out of range", seed, v);
        }
    }

    #[test]
    fn pixel_noise_deterministic_mode_is_reproducible() {
        let a = pixel_noise(100, 200, 42, true, 0.02);
        let b = pixel_noise(100, 200, 42, true, 0.02);
        assert_eq!(a, b);
    }

    #[test]
    fn pixel_noise_expressive_mode_in_range() {
        for x in 0..50u32 {
            for y in 0..50u32 {
                let v = pixel_noise(x, y, 0, false, 0.02);
                assert!(v >= 0.0 && v <= 0.02, "pixel_noise({},{}) = {} out of range", x, y, v);
            }
        }
    }

    #[test]
    fn pixel_noise_different_seeds_differ() {
        let a = pixel_noise(10, 20, 1, true, 0.02);
        let b = pixel_noise(10, 20, 2, true, 0.02);
        // Very unlikely to be identical with different seeds
        assert!(a != b || a == 0.0);
    }

    #[test]
    fn palette_rgb_values_non_negative() {
        for swatch in &ANIME_PALETTE {
            assert!(swatch.rgb[0] >= 0.0);
            assert!(swatch.rgb[1] >= 0.0);
            assert!(swatch.rgb[2] >= 0.0);
        }
    }

    #[test]
    fn palette_hue_values_in_range() {
        for swatch in &ANIME_PALETTE {
            assert!(swatch.hue >= 0.0 && swatch.hue <= 1.0);
        }
    }

    #[test]
    fn palette_rgb_values_at_most_one() {
        for swatch in &ANIME_PALETTE {
            assert!(swatch.rgb[0] <= 1.0, "R > 1.0 for hue {}", swatch.hue);
            assert!(swatch.rgb[1] <= 1.0, "G > 1.0 for hue {}", swatch.hue);
            assert!(swatch.rgb[2] <= 1.0, "B > 1.0 for hue {}", swatch.hue);
        }
    }

    #[test]
    fn nearest_by_hue_finds_green() {
        let result = nearest_by_hue(0.30, Some(PaletteCategory::Highlight));
        assert!((result.hue - 0.30).abs() < 0.05);
        assert_eq!(result.category, PaletteCategory::Highlight);
    }

    #[test]
    fn nearest_by_hue_finds_accent_purple() {
        let result = nearest_by_hue(0.78, Some(PaletteCategory::Mid));
        assert!((result.hue - 0.78).abs() < 0.05);
        assert_eq!(result.category, PaletteCategory::Mid);
    }

    #[test]
    fn hash_noise_different_seeds_vary() {
        // Collect unique noise values for 100 different seeds
        let mut values = std::collections::HashSet::new();
        for seed in 0..100u32 {
            let bits = hash_noise(seed, 1.0).to_bits();
            values.insert(bits);
        }
        // Should have high entropy — at least 90 unique values
        assert!(values.len() > 90, "hash_noise lacks entropy: only {} unique values", values.len());
    }

    #[test]
    fn hash_noise_zero_amplitude_is_zero() {
        for seed in 0..100u32 {
            assert_eq!(hash_noise(seed, 0.0), 0.0);
        }
    }

    #[test]
    fn pixel_noise_deterministic_different_positions() {
        let a = pixel_noise(0, 0, 42, true, 0.02);
        let b = pixel_noise(100, 200, 42, true, 0.02);
        // Different positions should (very likely) produce different noise
        assert!(a != b || (a == 0.0 && b == 0.0));
    }

    #[test]
    fn pixel_noise_expressive_covers_range() {
        // Check that expressive noise isn't stuck at a single value
        let mut min_val = f32::MAX;
        let mut max_val = f32::MIN;
        for x in 0..100u32 {
            for y in 0..100u32 {
                let v = pixel_noise(x, y, 0, false, 1.0);
                min_val = min_val.min(v);
                max_val = max_val.max(v);
            }
        }
        // Should cover a significant portion of the [0, 1] range
        assert!(max_val - min_val > 0.5, "expressive noise range too narrow: [{min_val}, {max_val}]");
    }

    #[test]
    fn each_category_has_hue_coverage() {
        // Shadows, mids, and highlights should each cover warm, cool, accent, green hue ranges
        for cat in [PaletteCategory::Shadow, PaletteCategory::Mid, PaletteCategory::Highlight] {
            let hues: Vec<f32> = ANIME_PALETTE
                .iter()
                .filter(|s| s.category == cat)
                .map(|s| s.hue)
                .collect();
            let min_hue = hues.iter().cloned().fold(f32::MAX, f32::min);
            let max_hue = hues.iter().cloned().fold(f32::MIN, f32::max);
            // Should span a wide hue range (warm ~0.02 to purple ~0.79)
            assert!(max_hue - min_hue > 0.2, "category {:?} hue range too narrow", cat);
        }
    }
}
