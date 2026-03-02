use crate::types::{TextureNodeOpV1, TextureProgramV1, TextureSource};
use crate::util::expected_byte_len;

pub fn parse_texture_program_v1(source: &str) -> Result<TextureProgramV1, String> {
    let parsed = serde_json::from_str::<TextureProgramV1>(source)
        .map_err(|error| format!("texture program parse failed: {error}"))?;
    validate_program(&parsed)?;
    Ok(parsed)
}

pub fn compile_texture_program_v1(program: &TextureProgramV1) -> Result<TextureSource, String> {
    validate_program(program)?;
    let byte_len = expected_byte_len(program.width, program.height, 4)?;
    let mut pixels = vec![0u8; byte_len];

    // Deterministic baseline synthesis driven by node op ordering + seed.
    for (pixel_index, pixel) in pixels.iter_mut().enumerate() {
        let mut value = (program.seed as usize + pixel_index) as u64;
        for node in &program.nodes {
            value = value.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            value ^= node.seed_offset;
            let op_bias = match node.op {
                TextureNodeOpV1::Noise { .. } => 31,
                TextureNodeOpV1::Sdf { .. } => 43,
                TextureNodeOpV1::Warp => 59,
                TextureNodeOpV1::Tile => 67,
                TextureNodeOpV1::Mask => 79,
                TextureNodeOpV1::Blend { .. } => 97,
                TextureNodeOpV1::Grade => 109,
            };
            value = value.wrapping_add(op_bias);
        }
        *pixel = (value & 0xff) as u8;
    }

    Ok(TextureSource {
        width: program.width,
        height: program.height,
        format: crate::types::TextureFormat::Rgba8,
        pixels,
    })
}

fn validate_program(program: &TextureProgramV1) -> Result<(), String> {
    if program.schema_version != 1 {
        return Err(format!(
            "texture program schema mismatch: expected 1, got {}",
            program.schema_version
        ));
    }
    if program.kind != "texture_program" {
        return Err(format!(
            "texture program kind mismatch: expected 'texture_program', got '{}'",
            program.kind
        ));
    }
    if program.width == 0 || program.height == 0 {
        return Err("texture program dimensions must be non-zero".to_string());
    }
    if program.nodes.is_empty() {
        return Err("texture program must include at least one node".to_string());
    }
    if !program.nodes.iter().any(|node| node.id == program.output) {
        return Err(format!(
            "texture program output '{}' does not reference a known node id",
            program.output
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{compile_texture_program_v1, parse_texture_program_v1};
    use crate::types::{TextureNodeOpV1, TextureNodeV1, TextureNoiseKindV1, TextureProgramV1};
    use std::collections::BTreeMap;

    fn sample_program() -> TextureProgramV1 {
        TextureProgramV1 {
            schema_version: 1,
            kind: "texture_program".to_string(),
            width: 8,
            height: 8,
            seed: 42,
            nodes: vec![TextureNodeV1 {
                id: "n0".to_string(),
                op: TextureNodeOpV1::Noise {
                    kind: TextureNoiseKindV1::Perlin,
                },
                inputs: Vec::new(),
                params: BTreeMap::new(),
                seed_offset: 0,
            }],
            output: "n0".to_string(),
        }
    }

    mod parser {
        use super::{parse_texture_program_v1, sample_program};

        #[test]
        fn parses_valid_json_program() {
            let json = serde_json::to_string(&sample_program()).expect("serialize program");
            let parsed = parse_texture_program_v1(&json).expect("parse program");
            assert_eq!(parsed.output, "n0");
        }
    }

    mod determinism {
        use super::{compile_texture_program_v1, sample_program};

        #[test]
        fn deterministic_compilation_for_same_seed() {
            let program = sample_program();
            let a = compile_texture_program_v1(&program).expect("compile a");
            let b = compile_texture_program_v1(&program).expect("compile b");
            assert_eq!(a, b);
        }
    }

    mod diagnostics {
        use super::{parse_texture_program_v1, sample_program};

        #[test]
        fn rejects_unknown_output_node_with_precise_error() {
            let mut program = sample_program();
            program.output = "missing".to_string();
            let json = serde_json::to_string(&program).expect("serialize");
            let err = parse_texture_program_v1(&json).expect_err("must reject unknown output");
            assert!(err.contains("does not reference a known node id"));
        }
    }
}
