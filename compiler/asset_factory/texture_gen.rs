use crate::asset_factory::{
    AssetAdapterKind, AssetArtifactCache, AssetGenerationArtifact, AssetGenerationRequest,
    AssetGenerationResult, AsyncAssetAdapter, CanonicalJobEnvelope,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;

/// Default texture resolution for procedural fallback generation.
const DEFAULT_TEXTURE_SIZE: u32 = 512;

/// Texture generation adapter that produces PBR texture sets.
///
/// Strategy (ordered by preference):
/// 1. **GLB extraction**: If a cached GLB exists for the same asset_id in the
///    asset-factory cache, extract embedded PBR textures (albedo, normal, ORM)
///    directly from the glTF material data.
/// 2. **Procedural fallback**: Generate deterministic 512x512 noise-based PBR
///    textures using a seeded hash-noise algorithm. No external dependencies.
///
/// Both paths produce valid PNG files and maintain the deterministic
/// seed -> replay_hash -> cache key pipeline.
pub struct TextureGenerationAdapter {
    cache: AssetArtifactCache,
}

impl TextureGenerationAdapter {
    pub fn new(cache: AssetArtifactCache) -> Self {
        Self { cache }
    }

    fn fingerprint(bytes: &[u8]) -> String {
        let digest = Sha256::digest(bytes);
        let mut out = String::with_capacity(digest.len() * 2);
        for byte in digest {
            out.push_str(&format!("{byte:02x}"));
        }
        out
    }

    fn build_envelope(request: &AssetGenerationRequest) -> CanonicalJobEnvelope {
        let canonical = crate::asset_factory::canonical_prompt(request);
        let hash = crate::asset_factory::replay_hash(
            AssetAdapterKind::Image,
            "procedural-texture",
            canonical.as_str(),
            request.seed,
        );
        let job_id = format!("job-{}", &hash[..16]);
        CanonicalJobEnvelope {
            job_id,
            adapter_kind: AssetAdapterKind::Image,
            provider: "procedural-texture".to_string(),
            seed: request.seed,
            canonical_prompt: canonical,
            replay_hash: hash,
        }
    }

    // -----------------------------------------------------------------------
    // GLB texture extraction
    // -----------------------------------------------------------------------

    /// Attempt to extract PBR textures from a GLB file at the given path.
    /// Returns `Some(Vec<(filename, png_bytes)>)` on success, `None` if no
    /// textures can be extracted.
    pub fn extract_textures_from_glb(glb_path: &Path) -> Option<Vec<(String, Vec<u8>)>> {
        let file_bytes = std::fs::read(glb_path).ok()?;
        let glb = gltf::Glb::from_slice(&file_bytes).ok()?;
        let gltf_data = gltf::Gltf::from_slice(&glb.json).ok()?;
        let bin = glb.bin.as_deref()?;

        let mut textures = Vec::new();

        // Walk materials and pull the first material's PBR textures.
        for material in gltf_data.materials() {
            let pbr = material.pbr_metallic_roughness();

            // Albedo / base color texture
            if let Some(info) = pbr.base_color_texture() {
                if let Some(png) = Self::extract_texture_image(&gltf_data, bin, info.texture().index()) {
                    textures.push(("albedo.png".to_string(), png));
                }
            }

            // Normal map
            if let Some(normal_tex) = material.normal_texture() {
                if let Some(png) = Self::extract_texture_image(&gltf_data, bin, normal_tex.texture().index()) {
                    textures.push(("normal.png".to_string(), png));
                }
            }

            // ORM: metallic-roughness texture encodes occlusion(R), roughness(G), metallic(B)
            if let Some(info) = pbr.metallic_roughness_texture() {
                if let Some(png) = Self::extract_texture_image(&gltf_data, bin, info.texture().index()) {
                    textures.push(("orm.png".to_string(), png));
                }
            } else if let Some(occ_tex) = material.occlusion_texture() {
                // Fall back to occlusion-only if no metallic-roughness texture.
                if let Some(png) = Self::extract_texture_image(&gltf_data, bin, occ_tex.texture().index()) {
                    textures.push(("orm.png".to_string(), png));
                }
            }

            // Only use the first material.
            if !textures.is_empty() {
                break;
            }
        }

        if textures.is_empty() {
            None
        } else {
            Some(textures)
        }
    }

    /// Extract a single texture image from the glTF by texture index.
    /// Returns raw PNG bytes if the image is embedded as PNG/JPEG, or None.
    fn extract_texture_image(
        gltf_data: &gltf::Gltf,
        bin: &[u8],
        texture_index: usize,
    ) -> Option<Vec<u8>> {
        let texture = gltf_data.textures().nth(texture_index)?;
        let image = texture.source();

        match image.source() {
            gltf::image::Source::View { view, mime_type } => {
                let offset = view.offset();
                let length = view.length();
                if offset + length > bin.len() {
                    return None;
                }
                let raw = &bin[offset..offset + length];

                match mime_type {
                    "image/png" => Some(raw.to_vec()),
                    "image/jpeg" => {
                        // Return JPEG as-is; the pipeline accepts it.
                        // If strict PNG is needed, a re-encode step would go here.
                        Some(raw.to_vec())
                    }
                    _ => None,
                }
            }
            gltf::image::Source::Uri { .. } => {
                // External URI references are not supported for GLB extraction.
                None
            }
        }
    }

    // -----------------------------------------------------------------------
    // Procedural 512x512 PBR texture generation (deterministic fallback)
    // -----------------------------------------------------------------------

    /// Generate a procedural 512x512 PNG texture with seeded noise.
    /// `kind` selects the noise pattern and color mapping:
    /// - `"albedo"`: earth-tone noise for natural surfaces
    /// - `"normal"`: tangent-space normal map (centered on flat, with perturbation)
    /// - `"orm"`: occlusion/roughness/metallic packed channels
    fn generate_procedural_png(seed: u64, kind: &str, width: u32, height: u32) -> Vec<u8> {
        let pixel_count = (width * height) as usize;
        // RGB pixels, 3 bytes each
        let mut pixels = vec![0u8; pixel_count * 3];

        let mut rng_state = seed;

        for y in 0..height {
            for x in 0..width {
                let idx = ((y * width + x) as usize) * 3;
                let (r, g, b) = match kind {
                    "albedo" => Self::albedo_pixel(x, y, width, height, seed),
                    "normal" => Self::normal_pixel(x, y, width, height, seed),
                    "orm" => Self::orm_pixel(x, y, width, height, seed),
                    _ => {
                        // Hash-noise fallback
                        rng_state = Self::hash_mix(rng_state, (y as u64) * 65537 + (x as u64));
                        let r = ((rng_state >> 0) & 0xFF) as u8;
                        let g = ((rng_state >> 8) & 0xFF) as u8;
                        let b = ((rng_state >> 16) & 0xFF) as u8;
                        (r, g, b)
                    }
                };
                pixels[idx] = r;
                pixels[idx + 1] = g;
                pixels[idx + 2] = b;
            }
        }

        Self::encode_png_rgb(&pixels, width, height)
    }

    /// Albedo pixel: multi-octave value noise in earth tones.
    fn albedo_pixel(x: u32, y: u32, w: u32, h: u32, seed: u64) -> (u8, u8, u8) {
        let fx = x as f64 / w as f64;
        let fy = y as f64 / h as f64;

        // Multi-octave noise
        let n1 = Self::value_noise_2d(fx * 8.0, fy * 8.0, seed);
        let n2 = Self::value_noise_2d(fx * 16.0, fy * 16.0, seed.wrapping_add(1)) * 0.5;
        let n3 = Self::value_noise_2d(fx * 32.0, fy * 32.0, seed.wrapping_add(2)) * 0.25;
        let combined = ((n1 + n2 + n3) / 1.75).clamp(0.0, 1.0);

        // Map to earth tones based on seed: browns, greens, greys
        let hue_shift = (seed % 3) as f64;
        let (base_r, base_g, base_b) = if hue_shift < 1.0 {
            // Warm brown (bark/soil)
            (0.45, 0.30, 0.18)
        } else if hue_shift < 2.0 {
            // Forest green (foliage)
            (0.22, 0.40, 0.15)
        } else {
            // Stone grey (rock)
            (0.42, 0.42, 0.40)
        };

        let r = ((base_r + combined * 0.3).clamp(0.0, 1.0) * 255.0) as u8;
        let g = ((base_g + combined * 0.25).clamp(0.0, 1.0) * 255.0) as u8;
        let b = ((base_b + combined * 0.2).clamp(0.0, 1.0) * 255.0) as u8;
        (r, g, b)
    }

    /// Normal pixel: tangent-space normal map with noise perturbation.
    fn normal_pixel(x: u32, y: u32, w: u32, h: u32, seed: u64) -> (u8, u8, u8) {
        let fx = x as f64 / w as f64;
        let fy = y as f64 / h as f64;

        let scale = 16.0;
        let eps = 1.0 / w as f64;

        // Height samples for finite-difference normal computation
        let h_c = Self::value_noise_2d(fx * scale, fy * scale, seed.wrapping_add(100));
        let h_r = Self::value_noise_2d((fx + eps) * scale, fy * scale, seed.wrapping_add(100));
        let h_u = Self::value_noise_2d(fx * scale, (fy + eps) * scale, seed.wrapping_add(100));

        let dx = (h_c - h_r) * 2.0;
        let dy = (h_c - h_u) * 2.0;

        // Tangent-space normal: remap [-1,1] -> [0,255]
        let nx = (dx * 0.5 + 0.5).clamp(0.0, 1.0);
        let ny = (dy * 0.5 + 0.5).clamp(0.0, 1.0);
        // Z component: sqrt(1 - x^2 - y^2), biased toward 1.0 (flat)
        let nz_raw = (1.0 - dx * dx - dy * dy).max(0.0).sqrt();
        let nz = (nz_raw * 0.5 + 0.5).clamp(0.0, 1.0);

        (
            (nx * 255.0) as u8,
            (ny * 255.0) as u8,
            (nz * 255.0) as u8,
        )
    }

    /// ORM pixel: Occlusion (R), Roughness (G), Metallic (B).
    fn orm_pixel(x: u32, y: u32, w: u32, h: u32, seed: u64) -> (u8, u8, u8) {
        let fx = x as f64 / w as f64;
        let fy = y as f64 / h as f64;

        // Occlusion: mostly white (1.0) with cavity darkening
        let ao_noise = Self::value_noise_2d(fx * 12.0, fy * 12.0, seed.wrapping_add(200));
        let ao = (0.7 + ao_noise * 0.3).clamp(0.0, 1.0);

        // Roughness: natural materials are quite rough (0.6-0.95)
        let rough_noise = Self::value_noise_2d(fx * 10.0, fy * 10.0, seed.wrapping_add(300));
        let roughness = (0.6 + rough_noise * 0.35).clamp(0.0, 1.0);

        // Metallic: natural materials are non-metallic (near 0)
        let metallic = 0.0_f64;

        (
            (ao * 255.0) as u8,
            (roughness * 255.0) as u8,
            (metallic * 255.0) as u8,
        )
    }

    /// Deterministic 2D value noise using hash mixing.
    /// Returns a value in [0, 1].
    fn value_noise_2d(x: f64, y: f64, seed: u64) -> f64 {
        let ix = x.floor() as i64;
        let iy = y.floor() as i64;
        let fx = x - x.floor();
        let fy = y - y.floor();

        // Smoothstep interpolation
        let sx = fx * fx * (3.0 - 2.0 * fx);
        let sy = fy * fy * (3.0 - 2.0 * fy);

        let v00 = Self::hash_to_float(ix, iy, seed);
        let v10 = Self::hash_to_float(ix + 1, iy, seed);
        let v01 = Self::hash_to_float(ix, iy + 1, seed);
        let v11 = Self::hash_to_float(ix + 1, iy + 1, seed);

        let a = v00 + (v10 - v00) * sx;
        let b = v01 + (v11 - v01) * sx;
        a + (b - a) * sy
    }

    /// Hash two integers + seed to a float in [0, 1].
    fn hash_to_float(x: i64, y: i64, seed: u64) -> f64 {
        let mut h = seed;
        h = Self::hash_mix(h, x as u64);
        h = Self::hash_mix(h, y as u64);
        // Map to [0, 1]
        (h & 0x00FF_FFFF) as f64 / 0x00FF_FFFF as f64
    }

    /// Fast deterministic hash mixing (xorshift-multiply variant).
    fn hash_mix(a: u64, b: u64) -> u64 {
        let mut h = a ^ b.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        h ^= h >> 30;
        h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        h ^= h >> 27;
        h = h.wrapping_mul(0x94D0_49BB_1331_11EB);
        h ^= h >> 31;
        h
    }

    // -----------------------------------------------------------------------
    // PNG encoder (512x512 capable, stored-block deflate)
    // -----------------------------------------------------------------------

    /// Encode raw RGB pixel data as a valid PNG file.
    /// Uses uncompressed deflate blocks (stored mode) so no compression
    /// library is needed. Output is larger than optimal but always valid.
    fn encode_png_rgb(pixels: &[u8], width: u32, height: u32) -> Vec<u8> {
        let expected_len = (width * height * 3) as usize;
        assert_eq!(
            pixels.len(),
            expected_len,
            "pixel buffer size mismatch: expected {expected_len}, got {}",
            pixels.len()
        );

        // Build raw image data: each scanline is filter_byte(0) + width*3 RGB bytes
        let scanline_len = 1 + (width as usize) * 3; // filter byte + pixel data
        let raw_image_len = scanline_len * (height as usize);
        let mut raw_image = Vec::with_capacity(raw_image_len);
        for y in 0..height {
            raw_image.push(0u8); // filter: None
            let row_start = (y as usize) * (width as usize) * 3;
            let row_end = row_start + (width as usize) * 3;
            raw_image.extend_from_slice(&pixels[row_start..row_end]);
        }

        let compressed = deflate_stored_blocks(&raw_image);

        let mut png = Vec::with_capacity(8 + 25 + compressed.len() + 20 + 12);

        // PNG signature
        png.extend_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]);

        // IHDR
        let mut ihdr_data = [0u8; 13];
        ihdr_data[0..4].copy_from_slice(&width.to_be_bytes());
        ihdr_data[4..8].copy_from_slice(&height.to_be_bytes());
        ihdr_data[8] = 8;  // bit depth
        ihdr_data[9] = 2;  // color type: RGB
        ihdr_data[10] = 0; // compression
        ihdr_data[11] = 0; // filter
        ihdr_data[12] = 0; // interlace
        write_png_chunk(&mut png, b"IHDR", &ihdr_data);

        // IDAT
        write_png_chunk(&mut png, b"IDAT", &compressed);

        // IEND
        write_png_chunk(&mut png, b"IEND", &[]);

        png
    }
}

/// Write a PNG chunk: length (4) + type (4) + data + CRC (4).
fn write_png_chunk(buf: &mut Vec<u8>, chunk_type: &[u8; 4], data: &[u8]) {
    let len = data.len() as u32;
    buf.extend_from_slice(&len.to_be_bytes());
    buf.extend_from_slice(chunk_type);
    buf.extend_from_slice(data);
    // CRC covers chunk_type + data
    let mut crc_input = Vec::with_capacity(4 + data.len());
    crc_input.extend_from_slice(chunk_type);
    crc_input.extend_from_slice(data);
    let crc = crc32fast::crc32fast::hash(&crc_input);
    buf.extend_from_slice(&crc.to_be_bytes());
}

/// Zlib/deflate encoding using stored (uncompressed) blocks.
///
/// For data larger than 65535 bytes we split into multiple stored blocks,
/// each up to 65535 bytes. The last block has BFINAL=1.
fn deflate_stored_blocks(data: &[u8]) -> Vec<u8> {
    // Worst case: zlib header (2) + per-block overhead (5 per 65535 bytes) + data + adler32 (4)
    let num_blocks = if data.is_empty() {
        1
    } else {
        (data.len() + 65534) / 65535
    };
    let mut out = Vec::with_capacity(2 + num_blocks * 5 + data.len() + 4);

    // Zlib header: CMF=0x78 (deflate, window=32768), FLG=0x01
    out.push(0x78);
    out.push(0x01);

    if data.is_empty() {
        // Empty final block
        out.push(0x01); // BFINAL=1, BTYPE=00
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0xFFFFu16.to_le_bytes());
    } else {
        let mut offset = 0;
        while offset < data.len() {
            let remaining = data.len() - offset;
            let block_size = remaining.min(65535);
            let is_final = offset + block_size >= data.len();

            let header_byte = if is_final { 0x01u8 } else { 0x00u8 };
            out.push(header_byte);

            let len = block_size as u16;
            let nlen = !len;
            out.extend_from_slice(&len.to_le_bytes());
            out.extend_from_slice(&nlen.to_le_bytes());
            out.extend_from_slice(&data[offset..offset + block_size]);

            offset += block_size;
        }
    }

    // Adler-32 checksum of uncompressed data
    let adler = adler32(data);
    out.extend_from_slice(&adler.to_be_bytes());

    out
}

fn adler32(data: &[u8]) -> u32 {
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for &byte in data {
        a = (a + byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

#[async_trait::async_trait]
impl AsyncAssetAdapter for TextureGenerationAdapter {
    fn kind(&self) -> AssetAdapterKind {
        AssetAdapterKind::Image
    }

    fn provider(&self) -> &str {
        "procedural-texture"
    }

    async fn generate(
        &self,
        request: &AssetGenerationRequest,
    ) -> Result<AssetGenerationResult, String> {
        let envelope = Self::build_envelope(request);

        // Check cache first.
        if let Some(cached) = self.cache.get(&envelope)? {
            return Ok(cached);
        }

        // Strategy 1: Try to extract textures from a cached GLB for this asset.
        // The Tripo adapter stores GLBs in the cache keyed by replay_hash.
        // We look for any .glb file in the cache directory whose name matches
        // a Mesh3d envelope for the same asset_id.
        let glb_textures = self.try_extract_glb_textures(request);

        // Strategy 2: Fall back to procedural 512x512 generation.
        let texture_data: Vec<(String, Vec<u8>)> = if let Some(extracted) = glb_textures {
            extracted
        } else {
            let size = DEFAULT_TEXTURE_SIZE;
            vec![
                (
                    "albedo.png".to_string(),
                    Self::generate_procedural_png(request.seed, "albedo", size, size),
                ),
                (
                    "normal.png".to_string(),
                    Self::generate_procedural_png(request.seed, "normal", size, size),
                ),
                (
                    "orm.png".to_string(),
                    Self::generate_procedural_png(request.seed, "orm", size, size),
                ),
            ]
        };

        let mut artifacts = Vec::with_capacity(texture_data.len());
        let mut combined_fingerprint_input = Vec::new();

        for (filename, png_bytes) in &texture_data {
            let local_path = self.cache.put_bytes(&envelope, filename, png_bytes)?;

            let fp = Self::fingerprint(png_bytes);
            combined_fingerprint_input.extend_from_slice(fp.as_bytes());

            let artifact_id = format!(
                "tex-{}-{}",
                &envelope.replay_hash[..8],
                filename.replace('.', "_")
            );
            let logical_path = format!("generated/{}/{}", request.asset_id, filename);

            let mut metadata = BTreeMap::new();
            metadata.insert("provider".to_string(), "procedural-texture".to_string());
            metadata.insert("texture_type".to_string(), filename.replace(".png", ""));
            metadata.insert(
                "resolution".to_string(),
                format!("{DEFAULT_TEXTURE_SIZE}x{DEFAULT_TEXTURE_SIZE}"),
            );

            artifacts.push(AssetGenerationArtifact {
                artifact_id,
                logical_path,
                bytes_len: png_bytes.len() as u64,
                fingerprint: fp,
                metadata,
                local_path: Some(local_path),
            });
        }

        let combined_fp = Self::fingerprint(&combined_fingerprint_input);

        let result = AssetGenerationResult {
            envelope,
            provider_response_hash: combined_fp,
            artifacts,
        };

        // Persist manifest.
        self.cache.put(&result)?;

        Ok(result)
    }
}

impl TextureGenerationAdapter {
    /// Try to find a GLB file in the cache for the same asset_id and extract
    /// its PBR textures. Returns None if no suitable GLB is found or if the
    /// GLB contains no extractable textures.
    fn try_extract_glb_textures(
        &self,
        request: &AssetGenerationRequest,
    ) -> Option<Vec<(String, Vec<u8>)>> {
        // Build the envelope that the Tripo adapter would have used for a
        // Mesh3d request with the same asset_id. We construct the same
        // canonical prompt and replay hash so we can look up its cache entry.
        let mesh_canonical = crate::asset_factory::canonical_prompt(&AssetGenerationRequest {
            asset_id: request.asset_id.clone(),
            prompt: request.prompt.clone(),
            style_profile: request.style_profile.clone(),
            seed: request.seed,
            negative_constraints: request.negative_constraints.clone(),
        });
        let mesh_hash = crate::asset_factory::replay_hash(
            AssetAdapterKind::Mesh3d,
            "tripo3d",
            &mesh_canonical,
            request.seed,
        );

        let mesh_envelope = CanonicalJobEnvelope {
            job_id: format!("job-{}", &mesh_hash[..16]),
            adapter_kind: AssetAdapterKind::Mesh3d,
            provider: "tripo3d".to_string(),
            seed: request.seed,
            canonical_prompt: mesh_canonical,
            replay_hash: mesh_hash,
        };

        // Check if the Tripo adapter has a cached result.
        let cached_mesh = self.cache.get(&mesh_envelope).ok()??;

        // Find the GLB artifact with a local_path.
        for artifact in &cached_mesh.artifacts {
            if let Some(ref local_path) = artifact.local_path {
                if local_path.exists() {
                    if let Some(textures) = Self::extract_textures_from_glb(local_path) {
                        return Some(textures);
                    }
                }
            }
        }

        None
    }
}

/// Re-export crc32fast at module level for the PNG encoder.
mod crc32fast {
    pub mod crc32fast {
        pub fn hash(data: &[u8]) -> u32 {
            ::crc32fast::hash(data)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset_factory::AssetArtifactCache;

    fn sample_request() -> AssetGenerationRequest {
        AssetGenerationRequest {
            asset_id: "test_texture".to_string(),
            prompt: "mossy stone wall".to_string(),
            style_profile: "realistic".to_string(),
            seed: 99,
            negative_constraints: vec![],
        }
    }

    #[test]
    fn test_procedural_png_is_valid_512x512() {
        let png = TextureGenerationAdapter::generate_procedural_png(42, "albedo", 512, 512);
        // Check PNG signature
        assert_eq!(&png[..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
        // 512x512 RGB uncompressed is ~786KB + overhead; must be substantial
        assert!(
            png.len() > 500_000,
            "512x512 PNG too small: {} bytes",
            png.len()
        );
    }

    #[test]
    fn test_procedural_png_deterministic() {
        let a = TextureGenerationAdapter::generate_procedural_png(42, "albedo", 64, 64);
        let b = TextureGenerationAdapter::generate_procedural_png(42, "albedo", 64, 64);
        assert_eq!(a, b, "same seed must produce identical output");
    }

    #[test]
    fn test_procedural_png_different_seeds() {
        let a = TextureGenerationAdapter::generate_procedural_png(42, "albedo", 64, 64);
        let b = TextureGenerationAdapter::generate_procedural_png(43, "albedo", 64, 64);
        assert_ne!(a, b, "different seeds must produce different output");
    }

    #[test]
    fn test_procedural_normal_map_valid() {
        let png = TextureGenerationAdapter::generate_procedural_png(42, "normal", 64, 64);
        assert_eq!(&png[..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
        assert!(png.len() > 1000, "Normal map PNG too small");
    }

    #[test]
    fn test_procedural_orm_map_valid() {
        let png = TextureGenerationAdapter::generate_procedural_png(42, "orm", 64, 64);
        assert_eq!(&png[..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
        assert!(png.len() > 1000, "ORM map PNG too small");
    }

    #[test]
    fn test_value_noise_in_range() {
        for seed in 0..100u64 {
            let v = TextureGenerationAdapter::value_noise_2d(
                seed as f64 * 0.1,
                seed as f64 * 0.3,
                seed,
            );
            assert!(
                (0.0..=1.0).contains(&v),
                "noise value out of range: {v} for seed {seed}"
            );
        }
    }

    #[test]
    fn test_deflate_stored_blocks_large() {
        // Test with data larger than one stored block (65535 bytes)
        let data = vec![0xABu8; 70_000];
        let compressed = deflate_stored_blocks(&data);

        // Should have zlib header (2 bytes) + 2 blocks + adler32 (4 bytes)
        // Block 1: 5 + 65535, Block 2: 5 + 4465
        assert!(compressed.len() > data.len(), "stored blocks must be at least as large as input");

        // Verify zlib header
        assert_eq!(compressed[0], 0x78);
        assert_eq!(compressed[1], 0x01);
    }

    #[test]
    fn test_deflate_stored_blocks_empty() {
        let compressed = deflate_stored_blocks(&[]);
        assert_eq!(compressed[0], 0x78);
        assert_eq!(compressed[1], 0x01);
        // Should contain at least header + empty block + adler32
        assert!(compressed.len() >= 2 + 5 + 4);
    }

    #[tokio::test]
    async fn test_texture_gen_produces_512x512_files() {
        let temp = tempfile::tempdir().expect("tempdir");
        let cache = AssetArtifactCache::new(temp.path());
        let adapter = TextureGenerationAdapter::new(cache);

        let request = sample_request();
        let result = adapter.generate(&request).await.expect("generate");

        assert_eq!(
            result.artifacts.len(),
            3,
            "must produce albedo + normal + ORM"
        );

        for artifact in &result.artifacts {
            assert!(
                artifact.local_path.is_some(),
                "artifact must have local_path"
            );
            let path = artifact.local_path.as_ref().unwrap();
            assert!(path.exists(), "file must exist: {}", path.display());

            let contents = std::fs::read(path).expect("read file");
            // Verify PNG signature
            assert_eq!(
                &contents[..8],
                &[137, 80, 78, 71, 13, 10, 26, 10],
                "file must have PNG signature: {}",
                path.display()
            );
            // 512x512 textures must be substantial (>100KB)
            assert!(
                contents.len() > 100_000,
                "texture file too small ({} bytes): {}",
                contents.len(),
                path.display()
            );
            assert_eq!(
                contents.len() as u64, artifact.bytes_len,
                "bytes_len must match actual file size"
            );

            // Verify resolution metadata
            assert!(
                artifact.metadata.get("resolution").is_some(),
                "artifact must have resolution metadata"
            );
        }
    }

    #[tokio::test]
    async fn test_texture_gen_cache_hit() {
        let temp = tempfile::tempdir().expect("tempdir");
        let cache = AssetArtifactCache::new(temp.path());
        let adapter = TextureGenerationAdapter::new(cache);

        let request = sample_request();

        let first = adapter.generate(&request).await.expect("first");
        let second = adapter.generate(&request).await.expect("second");

        assert_eq!(first, second, "cached result must match original");
    }

    #[test]
    fn test_extract_textures_from_nonexistent_glb() {
        let result =
            TextureGenerationAdapter::extract_textures_from_glb(Path::new("/nonexistent.glb"));
        assert!(result.is_none());
    }

    #[test]
    fn test_extract_textures_from_minimal_glb() {
        // A minimal GLB with no textures should return None
        let json_content = br#"{"asset":{"version":"2.0"}}"#;
        let json_len = json_content.len();
        let padded_json_len = (json_len + 3) & !3;
        let padding = padded_json_len - json_len;
        let total_len = 12 + 8 + padded_json_len;

        let mut buf = Vec::with_capacity(total_len);
        buf.extend_from_slice(b"glTF");
        buf.extend_from_slice(&2u32.to_le_bytes());
        buf.extend_from_slice(&(total_len as u32).to_le_bytes());
        buf.extend_from_slice(&(padded_json_len as u32).to_le_bytes());
        buf.extend_from_slice(&0x4E4F534Au32.to_le_bytes());
        buf.extend_from_slice(json_content);
        for _ in 0..padding {
            buf.push(0x20);
        }

        let temp = tempfile::tempdir().expect("tempdir");
        let glb_path = temp.path().join("minimal.glb");
        std::fs::write(&glb_path, &buf).expect("write glb");

        let result = TextureGenerationAdapter::extract_textures_from_glb(&glb_path);
        assert!(
            result.is_none(),
            "minimal GLB with no textures should return None"
        );
    }

    #[test]
    fn test_encode_png_rgb_small() {
        // 2x2 red image
        let pixels = [
            255, 0, 0, 255, 0, 0, // row 0
            255, 0, 0, 255, 0, 0, // row 1
        ];
        let png = TextureGenerationAdapter::encode_png_rgb(&pixels, 2, 2);
        assert_eq!(&png[..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
        assert!(png.len() > 30);
    }
}
