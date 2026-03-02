use crate::types::{
    MipLevel, MipPreservationHooksV1, MipPreservationStatsV1, TextureCompressionMetadata,
    TextureFormat, TextureMipChain, TextureSource,
};
use crate::util::{deterministic_hash, expected_byte_len};

const SCHEMA_VERSION: u32 = 3;
const MIP_CHAIN_KIND: &str = "mip-geometry-v3";
const MIP_CODEC: &str = "zstd-fast";

fn source_hash(source: &TextureSource) -> String {
    let width_bytes = source.width.to_le_bytes();
    let height_bytes = source.height.to_le_bytes();
    deterministic_hash(&[
        width_bytes.as_slice(),
        height_bytes.as_slice(),
        source.pixels.as_slice(),
    ])
}

fn next_mip_dimension(value: u32) -> u32 {
    if value <= 1 { 1 } else { value / 2 }
}

fn downsample_rgba8(source: &TextureSource, next_width: u32, next_height: u32) -> TextureSource {
    let mut pixels = vec![0u8; (next_width as usize) * (next_height as usize) * 4];
    let src_w = source.width as usize;
    for y in 0..(next_height as usize) {
        for x in 0..(next_width as usize) {
            let src_x = (x * 2).min((source.width - 1) as usize);
            let src_y = (y * 2).min((source.height - 1) as usize);
            let src_index = (src_y * src_w + src_x) * 4;
            let dst_index = (y * (next_width as usize) + x) * 4;
            pixels[dst_index..dst_index + 4]
                .copy_from_slice(&source.pixels[src_index..src_index + 4]);
        }
    }
    TextureSource {
        width: next_width,
        height: next_height,
        format: TextureFormat::Rgba8,
        pixels,
    }
}

fn compute_preservation_stats(
    source: &TextureSource,
    hooks: &MipPreservationHooksV1,
) -> Option<MipPreservationStatsV1> {
    if hooks.roughness_channel.is_none() && hooks.normal_channels.is_none() {
        return None;
    }

    let mut roughness_sum: u128 = 0;
    let mut roughness_sq_sum: u128 = 0;
    let mut roughness_count: u128 = 0;

    if let Some(channel) = hooks.roughness_channel {
        for chunk in source.pixels.chunks_exact(4) {
            let sample = u128::from(chunk[channel as usize]);
            roughness_sum += sample;
            roughness_sq_sum += sample * sample;
            roughness_count += 1;
        }
    }

    let roughness_mean_milli = if roughness_count == 0 {
        0
    } else {
        ((roughness_sum * 1000) / roughness_count) as u32
    };
    let roughness_variance_milli = if roughness_count == 0 {
        0
    } else {
        let mean_sq = (roughness_sum * roughness_sum) / roughness_count;
        let variance = roughness_sq_sum.saturating_sub(mean_sq / roughness_count.max(1));
        ((variance * 1000) / roughness_count) as u32
    };

    let normal_length_mean_milli = if let Some([x, y, z]) = hooks.normal_channels {
        let mut total: u128 = 0;
        let mut count: u128 = 0;
        for chunk in source.pixels.chunks_exact(4) {
            let nx = i32::from(chunk[x as usize]) - 128;
            let ny = i32::from(chunk[y as usize]) - 128;
            let nz = i32::from(chunk[z as usize]) - 128;
            let magnitude_sq = (nx * nx + ny * ny + nz * nz) as u128;
            total += magnitude_sq;
            count += 1;
        }
        if count == 0 {
            0
        } else {
            ((total * 1000) / count) as u32
        }
    } else {
        0
    };

    Some(MipPreservationStatsV1 {
        roughness_mean_milli,
        roughness_variance_milli,
        normal_length_mean_milli,
    })
}

pub fn validate_texture(source: &TextureSource) -> Result<(), String> {
    if source.width == 0 || source.height == 0 {
        return Err("texture dimensions must be non-zero".to_string());
    }
    let expected_len =
        expected_byte_len(source.width, source.height, source.format.bytes_per_pixel())?;
    if source.pixels.len() != expected_len {
        return Err(format!(
            "pixel buffer length mismatch: expected {}, got {}",
            expected_len,
            source.pixels.len()
        ));
    }
    Ok(())
}

pub fn generate_mip_chain(source: &TextureSource) -> Result<TextureMipChain, String> {
    generate_mip_chain_with_hooks(source, &MipPreservationHooksV1::default())
}

pub fn generate_mip_chain_with_hooks(
    source: &TextureSource,
    hooks: &MipPreservationHooksV1,
) -> Result<TextureMipChain, String> {
    validate_texture(source)?;
    let source_hash = source_hash(source);

    let mut levels = Vec::new();
    let mut level_index = 0_u32;
    let mut current = source.clone();

    loop {
        let byte_len = expected_byte_len(
            current.width,
            current.height,
            current.format.bytes_per_pixel(),
        )?;
        let compressed_bytes = ((byte_len as u128 * 3 + 3) / 4) as usize;
        let ratio_milli = if byte_len == 0 {
            0
        } else {
            ((compressed_bytes as u128 * 1000) / byte_len as u128) as u32
        };

        let level_bytes = level_index.to_le_bytes();
        let width_bytes = current.width.to_le_bytes();
        let height_bytes = current.height.to_le_bytes();
        let hash = deterministic_hash(&[
            source_hash.as_bytes(),
            level_bytes.as_slice(),
            width_bytes.as_slice(),
            height_bytes.as_slice(),
            current.pixels.as_slice(),
        ]);

        levels.push(MipLevel {
            level: level_index,
            width: current.width,
            height: current.height,
            byte_len,
            pixel_hash: hash,
            pixels: current.pixels.clone(),
            compression: TextureCompressionMetadata {
                codec: MIP_CODEC.to_string(),
                uncompressed_bytes: byte_len,
                compressed_bytes,
                ratio_milli,
            },
            preservation: compute_preservation_stats(&current, hooks),
        });

        if current.width == 1 && current.height == 1 {
            break;
        }

        let next_width = next_mip_dimension(current.width);
        let next_height = next_mip_dimension(current.height);
        current = downsample_rgba8(&current, next_width, next_height);
        level_index = level_index.saturating_add(1);
    }

    Ok(TextureMipChain {
        schema_version: SCHEMA_VERSION,
        kind: MIP_CHAIN_KIND.to_string(),
        source_hash,
        levels,
    })
}

#[cfg(test)]
mod tests {
    use super::{generate_mip_chain, generate_mip_chain_with_hooks, validate_texture};
    use crate::types::{MipPreservationHooksV1, TextureFormat, TextureSource};

    fn make_source(width: u32, height: u32) -> TextureSource {
        let len = usize::try_from(u64::from(width) * u64::from(height) * 4)
            .expect("test texture size should fit in usize");
        TextureSource {
            width,
            height,
            format: TextureFormat::Rgba8,
            pixels: vec![128; len],
        }
    }

    #[test]
    fn validate_texture_accepts_matching_rgba8_buffer() {
        let source = make_source(4, 2);
        assert!(validate_texture(&source).is_ok());
    }

    #[test]
    fn validate_texture_rejects_length_mismatch() {
        let mut source = make_source(2, 2);
        source.pixels.pop();
        let error = validate_texture(&source).expect_err("mismatched byte length should fail");
        assert!(error.contains("expected 16, got 15"));
    }

    #[test]
    fn generate_mip_chain_depth_for_power_of_two_dimensions() {
        let source = make_source(8, 8);
        let chain = generate_mip_chain(&source).expect("mip generation should succeed");
        assert_eq!(chain.schema_version, 3);
        assert_eq!(chain.kind, "mip-geometry-v3");
        assert_eq!(chain.levels.len(), 4);
        assert_eq!((chain.levels[0].width, chain.levels[0].height), (8, 8));
        assert_eq!((chain.levels[3].width, chain.levels[3].height), (1, 1));
    }

    #[test]
    fn preserves_roughness_statistics() {
        let mut source = make_source(4, 4);
        for (i, chunk) in source.pixels.chunks_exact_mut(4).enumerate() {
            chunk[1] = (i as u8).saturating_mul(8);
        }
        let chain = generate_mip_chain_with_hooks(
            &source,
            &MipPreservationHooksV1 {
                roughness_channel: Some(1),
                normal_channels: None,
            },
        )
        .expect("mip chain");
        assert!(chain.levels[0].preservation.is_some());
        assert!(
            chain.levels[0]
                .preservation
                .as_ref()
                .expect("stats")
                .roughness_mean_milli
                > 0
        );
    }

    #[test]
    fn generate_mip_chain_is_deterministic() {
        let source = make_source(9, 3);
        let a = generate_mip_chain(&source).expect("first generation should succeed");
        let b = generate_mip_chain(&source).expect("second generation should succeed");
        assert_eq!(a, b);
    }
}
