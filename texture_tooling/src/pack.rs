use crate::types::{
    ChannelPackLayoutV1, PackedChannelBindingV1, PackedChannelSourceV1, TextureFormat,
    TextureSource,
};
use crate::util::{deterministic_hash, expected_byte_len};
use std::collections::BTreeMap;

pub fn default_orm_pack_layout_v1() -> ChannelPackLayoutV1 {
    ChannelPackLayoutV1 {
        schema_version: 1,
        kind: "channel-pack-layout".to_string(),
        id: "orm-rgba".to_string(),
        bindings: vec![
            PackedChannelBindingV1 {
                target_channel: 0,
                source_kind: PackedChannelSourceV1::TextureChannel,
                source_texture: Some("occlusion".to_string()),
                source_channel: Some(0),
                constant: None,
            },
            PackedChannelBindingV1 {
                target_channel: 1,
                source_kind: PackedChannelSourceV1::TextureChannel,
                source_texture: Some("roughness".to_string()),
                source_channel: Some(1),
                constant: None,
            },
            PackedChannelBindingV1 {
                target_channel: 2,
                source_kind: PackedChannelSourceV1::TextureChannel,
                source_texture: Some("metallic".to_string()),
                source_channel: Some(2),
                constant: None,
            },
            PackedChannelBindingV1 {
                target_channel: 3,
                source_kind: PackedChannelSourceV1::Constant,
                source_texture: None,
                source_channel: None,
                constant: Some(255),
            },
        ],
    }
}

pub fn pack_channels_v1(
    layout: &ChannelPackLayoutV1,
    sources: &BTreeMap<String, TextureSource>,
    width: u32,
    height: u32,
) -> Result<TextureSource, String> {
    let byte_len = expected_byte_len(width, height, 4)?;
    let mut pixels = vec![0u8; byte_len];

    for binding in &layout.bindings {
        if binding.target_channel > 3 {
            return Err(format!(
                "target channel {} out of range [0,3]",
                binding.target_channel
            ));
        }
        match binding.source_kind {
            PackedChannelSourceV1::Constant => {
                let Some(constant) = binding.constant else {
                    return Err(format!(
                        "binding for target channel {} missing constant value",
                        binding.target_channel
                    ));
                };
                for chunk in pixels.chunks_exact_mut(4) {
                    chunk[binding.target_channel as usize] = constant;
                }
            }
            PackedChannelSourceV1::TextureChannel => {
                let source_name = binding
                    .source_texture
                    .as_ref()
                    .ok_or_else(|| "texture source binding missing source_texture".to_string())?;
                let source_channel = binding
                    .source_channel
                    .ok_or_else(|| "texture source binding missing source_channel".to_string())?;
                if source_channel > 3 {
                    return Err(format!(
                        "source channel {} out of range [0,3]",
                        source_channel
                    ));
                }
                let source = sources.get(source_name).ok_or_else(|| {
                    format!(
                        "texture source '{}' missing for pack layout '{}'",
                        source_name, layout.id
                    )
                })?;
                if source.width != width || source.height != height {
                    return Err(format!(
                        "source '{}' dimensions {}x{} do not match target {}x{}",
                        source_name, source.width, source.height, width, height
                    ));
                }
                for (dst, src) in pixels
                    .chunks_exact_mut(4)
                    .zip(source.pixels.chunks_exact(4))
                {
                    dst[binding.target_channel as usize] = src[source_channel as usize];
                }
            }
        }
    }

    Ok(TextureSource {
        width,
        height,
        format: TextureFormat::Rgba8,
        pixels,
    })
}

pub fn packed_texture_hash(texture: &TextureSource) -> String {
    deterministic_hash(&[
        texture.width.to_le_bytes().as_slice(),
        texture.height.to_le_bytes().as_slice(),
        texture.pixels.as_slice(),
    ])
}

#[cfg(test)]
mod tests {
    use super::{default_orm_pack_layout_v1, pack_channels_v1, packed_texture_hash};
    use crate::types::{TextureFormat, TextureSource};
    use std::collections::BTreeMap;

    fn tex(fill: [u8; 4]) -> TextureSource {
        TextureSource {
            width: 2,
            height: 1,
            format: TextureFormat::Rgba8,
            pixels: vec![
                fill[0], fill[1], fill[2], fill[3], fill[0], fill[1], fill[2], fill[3],
            ],
        }
    }

    #[test]
    fn orm_channel_layout() {
        let mut sources = BTreeMap::new();
        sources.insert("occlusion".to_string(), tex([10, 11, 12, 13]));
        sources.insert("roughness".to_string(), tex([20, 21, 22, 23]));
        sources.insert("metallic".to_string(), tex([30, 31, 32, 33]));

        let packed = pack_channels_v1(&default_orm_pack_layout_v1(), &sources, 2, 1)
            .expect("pack should succeed");
        assert_eq!(packed.pixels[0], 10);
        assert_eq!(packed.pixels[1], 21);
        assert_eq!(packed.pixels[2], 32);
        assert_eq!(packed.pixels[3], 255);
    }

    #[test]
    fn deterministic_hash_stability() {
        let texture = tex([1, 2, 3, 4]);
        assert_eq!(packed_texture_hash(&texture), packed_texture_hash(&texture));
    }
}
