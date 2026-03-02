use crate::mips::generate_mip_chain_with_hooks;
use crate::pack::{pack_channels_v1, packed_texture_hash};
use crate::texture_lang::compile_texture_program_v1;
use crate::types::{
    ChannelPackLayoutV1, MipPreservationHooksV1, TextureArtifactStatsV1, TextureArtifactV1,
    TextureProgramV1, TextureSource, TextureStreamingMetadataV1,
};
use crate::util::deterministic_hash;
use std::collections::BTreeMap;

pub fn build_texture_artifact_v1(
    program: &TextureProgramV1,
    layout: &ChannelPackLayoutV1,
    hooks: &MipPreservationHooksV1,
    streaming: TextureStreamingMetadataV1,
) -> Result<TextureArtifactV1, String> {
    let base = compile_texture_program_v1(program)?;
    let mut sources = BTreeMap::<String, TextureSource>::new();

    // Hard-cut default source routing: one generated texture feeds all named channels unless
    // an explicit source was materialized upstream.
    for binding in &layout.bindings {
        if let Some(source_name) = &binding.source_texture {
            sources
                .entry(source_name.clone())
                .or_insert_with(|| base.clone());
        }
    }

    let packed_texture = pack_channels_v1(layout, &sources, program.width, program.height)?;
    let mip_chain = generate_mip_chain_with_hooks(&packed_texture, hooks)?;

    let first_stats = mip_chain
        .levels
        .first()
        .and_then(|level| level.preservation.clone())
        .unwrap_or_default();
    let last_stats = mip_chain
        .levels
        .last()
        .and_then(|level| level.preservation.clone())
        .unwrap_or_default();

    let program_hash = deterministic_hash(&[
        program.width.to_le_bytes().as_slice(),
        program.height.to_le_bytes().as_slice(),
        program.output.as_bytes(),
    ]);
    let content_hash = deterministic_hash(&[
        program_hash.as_bytes(),
        packed_texture_hash(&packed_texture).as_bytes(),
        mip_chain.source_hash.as_bytes(),
    ]);

    Ok(TextureArtifactV1 {
        schema_version: 1,
        kind: "texture_artifact_v1".to_string(),
        program_hash,
        content_hash,
        packed_layout: layout.clone(),
        packed_texture,
        mip_chain,
        streaming,
        stats: TextureArtifactStatsV1 {
            roughness_base_mean_milli: first_stats.roughness_mean_milli,
            roughness_tail_mean_milli: last_stats.roughness_mean_milli,
            normal_base_mean_milli: first_stats.normal_length_mean_milli,
            normal_tail_mean_milli: last_stats.normal_length_mean_milli,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::build_texture_artifact_v1;
    use crate::pack::default_orm_pack_layout_v1;
    use crate::types::{
        MipPreservationHooksV1, TextureNodeOpV1, TextureNodeV1, TextureNoiseKindV1,
        TextureProgramV1, TextureStreamingMetadataV1,
    };
    use std::collections::BTreeMap;

    #[test]
    fn builds_content_addressed_artifact() {
        let program = TextureProgramV1 {
            schema_version: 1,
            kind: "texture_program".to_string(),
            width: 4,
            height: 4,
            seed: 7,
            nodes: vec![TextureNodeV1 {
                id: "n0".to_string(),
                op: TextureNodeOpV1::Noise {
                    kind: TextureNoiseKindV1::Simplex,
                },
                inputs: Vec::new(),
                params: BTreeMap::new(),
                seed_offset: 0,
            }],
            output: "n0".to_string(),
        };
        let artifact = build_texture_artifact_v1(
            &program,
            &default_orm_pack_layout_v1(),
            &MipPreservationHooksV1 {
                roughness_channel: Some(1),
                normal_channels: Some([0, 1, 2]),
            },
            TextureStreamingMetadataV1 {
                tile_width: 128,
                tile_height: 128,
                min_resident_mip: 1,
                prefetch_mips: 2,
                residency_priority: 5,
            },
        )
        .expect("artifact should build");
        assert_eq!(artifact.schema_version, 1);
        assert_eq!(artifact.kind, "texture_artifact_v1");
        assert!(!artifact.content_hash.is_empty());
        assert!(!artifact.mip_chain.levels.is_empty());
    }
}
