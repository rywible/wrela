use crate::hir::def::MaterialDeclarationSurface;
use crate::shader_compiler::types::{
    MaterialCompileBudgetGatesV1, MaterialCompileReport, MaterialIrV1, MaterialParamV1,
    MaterialQualityTierCost, MaterialQualityTierV1, MaterialShaderVariantV1,
    MaterialTextureBindingSummaryV1, MaterialTextureColorSpaceV1, ShaderBindingV1,
    ShaderEntryPointV1, ShaderProgramIRV1, ShaderStageV1,
};
use crate::shader_compiler::{canonical_wgsl_binding_ident, validate_shader_program};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::Path;

pub fn lower_to_wgsl_v2(program: &ShaderProgramIRV1) -> Result<String, String> {
    validate_shader_program(program)?;

    let bindings = canonical_bindings(program.bindings.as_slice());
    let entry_points = canonical_entry_points(program.entry_points.as_slice());
    let binding_kinds = bindings
        .iter()
        .map(|binding| {
            BindingKind::parse(binding.resource_kind.as_str())
                .ok_or_else(|| invalid_resource_kind_error(binding))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let mut output = String::new();
    output.push_str("// wrela shader wgsl v2\n");
    output.push_str(&format!("// program_id: {}\n", program.program_id));
    output.push_str(&format!("// schema_version: {}\n", program.schema_version));
    output.push('\n');

    output.push_str("struct WrelaVertexOut {\n");
    output.push_str("    @builtin(position) position: vec4<f32>,\n");
    output.push_str("}\n\n");

    for (binding, binding_kind) in bindings.iter().zip(binding_kinds.iter().copied()) {
        if let Some(type_definition) = binding_kind.type_definition(binding) {
            output.push_str(type_definition.as_str());
            output.push_str("\n\n");
        }
    }

    for (binding, binding_kind) in bindings.iter().zip(binding_kinds.iter().copied()) {
        output.push_str(&binding_comment(binding));
        output.push_str(binding_kind.declaration(binding).as_str());
        output.push_str("\n\n");
    }

    for entry_point in &entry_points {
        output.push_str(&entry_comment(entry_point));
        match entry_point.stage {
            ShaderStageV1::Vertex => {
                output.push_str("@vertex\n");
                output.push_str(&format!(
                    "fn {}() -> WrelaVertexOut {{\n",
                    entry_point.function_name
                ));
                output.push_str("    var out: WrelaVertexOut;\n");
                output.push_str("    out.position = vec4<f32>(0.0, 0.0, 0.0, 1.0);\n");
                output.push_str("    return out;\n");
                output.push_str("}\n\n");
            }
            ShaderStageV1::Fragment => {
                output.push_str("@fragment\n");
                output.push_str(&format!(
                    "fn {}() -> @location(0) vec4<f32> {{\n",
                    entry_point.function_name
                ));
                output.push_str("    return vec4<f32>(1.0, 1.0, 1.0, 1.0);\n");
                output.push_str("}\n\n");
            }
            ShaderStageV1::Compute => {
                output.push_str("@compute @workgroup_size(1)\n");
                output.push_str(&format!("fn {}() {{\n", entry_point.function_name));
                output.push_str("}\n\n");
            }
        }
    }

    Ok(output)
}

pub fn shader_program_fingerprint(program: &ShaderProgramIRV1) -> Result<String, String> {
    validate_shader_program(program)?;

    let canonical = canonical_program(program);
    let serialized = serde_json::to_vec(&canonical)
        .map_err(|err| format!("failed to serialize shader program for fingerprinting: {err}"))?;

    let mut hasher = Sha256::new();
    hasher.update(serialized);
    Ok(format!("{:x}", hasher.finalize()))
}

fn canonical_program(program: &ShaderProgramIRV1) -> ShaderProgramIRV1 {
    let mut canonical = program.clone();
    canonical.bindings = canonical_bindings(canonical.bindings.as_slice());
    canonical.entry_points = canonical_entry_points(canonical.entry_points.as_slice());
    canonical
}

fn stage_rank(stage: ShaderStageV1) -> u8 {
    match stage {
        ShaderStageV1::Vertex => 0,
        ShaderStageV1::Fragment => 1,
        ShaderStageV1::Compute => 2,
    }
}

fn stage_name(stage: ShaderStageV1) -> &'static str {
    match stage {
        ShaderStageV1::Vertex => "vertex",
        ShaderStageV1::Fragment => "fragment",
        ShaderStageV1::Compute => "compute",
    }
}

fn canonical_bindings(bindings: &[ShaderBindingV1]) -> Vec<ShaderBindingV1> {
    let mut sorted = bindings.to_vec();
    sorted.sort_by(|left, right| {
        (
            left.group,
            left.binding,
            stage_rank(left.stage),
            left.id.as_str(),
        )
            .cmp(&(
                right.group,
                right.binding,
                stage_rank(right.stage),
                right.id.as_str(),
            ))
    });
    sorted
}

fn canonical_entry_points(entry_points: &[ShaderEntryPointV1]) -> Vec<ShaderEntryPointV1> {
    let mut sorted = entry_points.to_vec();
    sorted.sort_by(|left, right| {
        (
            stage_rank(left.stage),
            left.id.as_str(),
            left.function_name.as_str(),
        )
            .cmp(&(
                stage_rank(right.stage),
                right.id.as_str(),
                right.function_name.as_str(),
            ))
    });
    sorted
}

fn binding_comment(binding: &ShaderBindingV1) -> String {
    format!(
        "// binding_id: {}, stage: {}, slot: {}:{}\n",
        binding.id,
        stage_name(binding.stage),
        binding.group,
        binding.binding
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BindingKind {
    Uniform,
    Storage,
    Texture,
    Sampler,
}

impl BindingKind {
    fn parse(raw: &str) -> Option<Self> {
        let normalized = raw.trim().to_ascii_lowercase().replace('-', "_");
        match normalized.as_str() {
            "uniform" | "uniform_buffer" | "ubo" => Some(Self::Uniform),
            "storage" | "storage_buffer" | "ssbo" => Some(Self::Storage),
            "texture" | "texture_2d" | "sampled_texture" => Some(Self::Texture),
            "sampler" => Some(Self::Sampler),
            _ => None,
        }
    }

    fn type_definition(self, binding: &ShaderBindingV1) -> Option<String> {
        let type_name = binding_type_name(binding);
        match self {
            Self::Uniform => Some(format!(
                "struct {} {{\n    value: vec4<f32>,\n}}",
                type_name
            )),
            Self::Storage => Some(format!(
                "struct {} {{\n    data: array<u32>,\n}}",
                type_name
            )),
            Self::Texture | Self::Sampler => None,
        }
    }

    fn declaration(self, binding: &ShaderBindingV1) -> String {
        let ident = canonical_wgsl_binding_ident(binding.id.as_str());
        match self {
            Self::Uniform => format!(
                "@group({}) @binding({}) var<uniform> {}: {};",
                binding.group,
                binding.binding,
                ident,
                binding_type_name(binding)
            ),
            Self::Storage => format!(
                "@group({}) @binding({}) var<storage, read_write> {}: {};",
                binding.group,
                binding.binding,
                ident,
                binding_type_name(binding)
            ),
            Self::Texture => format!(
                "@group({}) @binding({}) var {}: texture_2d<f32>;",
                binding.group, binding.binding, ident
            ),
            Self::Sampler => format!(
                "@group({}) @binding({}) var {}: sampler;",
                binding.group, binding.binding, ident
            ),
        }
    }
}

fn binding_type_name(binding: &ShaderBindingV1) -> String {
    format!(
        "WrelaBindingType_{}",
        canonical_wgsl_binding_ident(binding.id.as_str())
    )
}

fn invalid_resource_kind_error(binding: &ShaderBindingV1) -> String {
    format!(
        "binding '{}' has unsupported resource_kind '{}' (supported: uniform, storage, texture, sampler)",
        binding.id, binding.resource_kind
    )
}

fn entry_comment(entry: &ShaderEntryPointV1) -> String {
    format!(
        "// entry_point_id: {}, stage: {}\n",
        entry.id,
        stage_name(entry.stage)
    )
}

fn normalize_material_token(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        out.push_str("unknown");
    }
    out
}

const STANDARD_MATERIAL_TEXTURE_SLOTS_V1: &[&str] = &[
    "albedo",
    "normal",
    "orm",
    "emissive",
    "thickness",
    "detail_normal",
];
const KTX2_RUNTIME_TEXTURE_FORMAT_V1: &str = "ktx2";

fn parse_boolish(raw: &str) -> bool {
    matches!(
        normalize_material_token(raw).as_str(),
        "1" | "true" | "yes" | "on" | "enabled"
    )
}

fn feature_enabled(features: &[String], feature: &str) -> bool {
    features.iter().any(|entry| entry == feature)
}

fn normalize_alpha_mode(raw: &str) -> String {
    match normalize_material_token(raw).as_str() {
        "blend" | "transparent" => "blend".to_string(),
        "mask" | "cutout" => "mask".to_string(),
        _ => "opaque".to_string(),
    }
}

fn expected_texture_color_space(slot: &str) -> Option<MaterialTextureColorSpaceV1> {
    match slot {
        "albedo" | "emissive" => Some(MaterialTextureColorSpaceV1::Srgb),
        "normal" | "orm" | "thickness" | "detail_normal" => {
            Some(MaterialTextureColorSpaceV1::Linear)
        }
        _ => None,
    }
}

fn infer_source_extension(source_reference: &str) -> Option<String> {
    Path::new(source_reference)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
}

fn runtime_reference_for_ktx2(source_reference: &str) -> String {
    let mut runtime_path = Path::new(source_reference).to_path_buf();
    runtime_path.set_extension(KTX2_RUNTIME_TEXTURE_FORMAT_V1);
    runtime_path.to_string_lossy().into_owned()
}

pub fn apply_material_texture_policy_to_report_v1(
    material: &MaterialDeclarationSurface,
    material_ir: &MaterialIrV1,
    report: &mut MaterialCompileReport,
) -> Result<(), String> {
    let mut lint_lines = Vec::new();
    let mut texture_bindings = Vec::new();
    let mut seen_slots = BTreeSet::new();
    let mut has_orm = false;

    for texture in &material.textures {
        let slot = normalize_material_token(texture.slot.value.as_str());
        if !STANDARD_MATERIAL_TEXTURE_SLOTS_V1
            .iter()
            .any(|allowed| *allowed == slot.as_str())
        {
            return Err(format!(
                "material '{}' uses unsupported texture slot '{}' (allowed: {})",
                material.name,
                texture.slot.value,
                STANDARD_MATERIAL_TEXTURE_SLOTS_V1.join(", ")
            ));
        }
        if !seen_slots.insert(slot.clone()) {
            return Err(format!(
                "material '{}' declares duplicate texture slot '{}' in texture policy stage",
                material.name, slot
            ));
        }
        if slot == "orm" {
            has_orm = true;
        }

        let source_reference = texture.value.value.to_string();
        let ext = infer_source_extension(source_reference.as_str()).ok_or_else(|| {
            format!(
                "material '{}' texture slot '{}' source '{}' is missing a file extension; supported: ktx2, png, tga, exr, jpg, jpeg",
                material.name, slot, source_reference
            )
        })?;
        let color_space = expected_texture_color_space(slot.as_str()).ok_or_else(|| {
            format!(
                "material '{}' texture slot '{}' has no color-space policy mapping",
                material.name, slot
            )
        })?;

        let runtime_reference = match ext.as_str() {
            "ktx2" => {
                lint_lines.push(format!(
                    "slot '{}' uses runtime-ready ktx2 source '{}' (color_space={})",
                    slot,
                    source_reference,
                    color_space.as_str()
                ));
                source_reference.clone()
            }
            "png" | "tga" | "exr" | "jpg" | "jpeg" => {
                let runtime_target = runtime_reference_for_ktx2(source_reference.as_str());
                lint_lines.push(format!(
                    "slot '{}' source '{}' ({}) will compile to ktx2 runtime target '{}' (color_space={})",
                    slot,
                    source_reference,
                    ext,
                    runtime_target,
                    color_space.as_str()
                ));
                runtime_target
            }
            _ => {
                return Err(format!(
                    "material '{}' texture slot '{}' source '{}' has unsupported extension '{}'; supported: ktx2, png, tga, exr, jpg, jpeg",
                    material.name, slot, source_reference, ext
                ));
            }
        };

        texture_bindings.push(MaterialTextureBindingSummaryV1 {
            slot,
            source_reference,
            runtime_reference,
            color_space,
        });
    }

    if matches!(
        material_ir.surface_model.as_str(),
        "pbr" | "pbr_metal_rough"
    ) && !has_orm
    {
        return Err(format!(
            "material '{}' surface_model '{}' requires texture slot 'orm'",
            material.name, material_ir.surface_model
        ));
    }
    if has_orm {
        lint_lines.push(format!(
            "surface_model '{}' includes required ORM texture slot",
            material_ir.surface_model
        ));
    }

    texture_bindings.sort_by(|left, right| left.slot.cmp(&right.slot));
    report.runtime_texture_format = KTX2_RUNTIME_TEXTURE_FORMAT_V1.to_string();
    report.texture_lint_lines = lint_lines;
    report.texture_bindings = texture_bindings;
    Ok(())
}

pub fn lower_material_to_ir_v1(material: &MaterialDeclarationSurface) -> MaterialIrV1 {
    let mut feature_map = std::collections::HashMap::<String, bool>::new();
    for feature in &material.features {
        feature_map.insert(
            normalize_material_token(feature.name.value.as_str()),
            parse_boolish(feature.value.value.as_str()),
        );
    }

    let mut texture_slots = material
        .textures
        .iter()
        .map(|texture| normalize_material_token(texture.slot.value.as_str()))
        .collect::<Vec<_>>();
    texture_slots.sort();
    texture_slots.dedup();

    let has_normal = texture_slots.iter().any(|slot| slot == "normal");
    let has_emissive = texture_slots.iter().any(|slot| slot == "emissive");
    let clearcoat = feature_map.get("clearcoat").copied().unwrap_or(false);
    let transmission = feature_map.get("transmission").copied().unwrap_or(false);
    let anisotropy = feature_map.get("anisotropy").copied().unwrap_or(false);
    let subsurface_lite = feature_map
        .get("subsurface_lite")
        .copied()
        .unwrap_or_else(|| feature_map.get("subsurface").copied().unwrap_or(false));
    let double_sided = material
        .render
        .double_sided
        .as_ref()
        .is_some_and(|value| parse_boolish(value.value.as_str()));
    let receives_decals = material
        .render
        .receives_decals
        .as_ref()
        .is_some_and(|value| parse_boolish(value.value.as_str()));

    let mut feature_bits = Vec::new();
    if has_normal {
        feature_bits.push("HAS_NORMAL".to_string());
    }
    if has_emissive {
        feature_bits.push("HAS_EMISSIVE".to_string());
    }
    if clearcoat {
        feature_bits.push("CLEARCOAT".to_string());
    }
    if transmission {
        feature_bits.push("TRANSMISSION".to_string());
    }
    if anisotropy {
        feature_bits.push("ANISOTROPY".to_string());
    }
    if subsurface_lite {
        feature_bits.push("SUBSURFACE_LITE".to_string());
    }
    if double_sided {
        feature_bits.push("DOUBLE_SIDED".to_string());
    }
    if receives_decals {
        feature_bits.push("RECEIVES_DECALS".to_string());
    }

    MaterialIrV1 {
        schema_version: 1,
        material_name: material.name.to_string(),
        surface_model: material
            .surface_model
            .as_ref()
            .map(|value| normalize_material_token(value.value.as_str()))
            .unwrap_or_else(|| "pbr".to_string()),
        alpha_mode: material
            .render
            .alpha
            .as_ref()
            .map(|value| normalize_alpha_mode(value.value.as_str()))
            .unwrap_or_else(|| "opaque".to_string()),
        feature_bits,
        texture_slots,
        params: material
            .params
            .iter()
            .map(|param| MaterialParamV1 {
                name: normalize_material_token(param.name.value.as_str()),
                value: param.value.value.to_string(),
            })
            .collect(),
    }
}

pub fn stable_material_feature_bitset_key(material: &MaterialIrV1) -> String {
    let alpha_code = match material.alpha_mode.as_str() {
        "mask" => "m",
        "blend" => "b",
        _ => "o",
    };
    format!(
        "N{}-E{}-A{}-C{}-T{}-AN{}-S{}-D{}-R{}",
        feature_enabled(&material.feature_bits, "HAS_NORMAL") as u8,
        feature_enabled(&material.feature_bits, "HAS_EMISSIVE") as u8,
        alpha_code,
        feature_enabled(&material.feature_bits, "CLEARCOAT") as u8,
        feature_enabled(&material.feature_bits, "TRANSMISSION") as u8,
        feature_enabled(&material.feature_bits, "ANISOTROPY") as u8,
        feature_enabled(&material.feature_bits, "SUBSURFACE_LITE") as u8,
        feature_enabled(&material.feature_bits, "DOUBLE_SIDED") as u8,
        feature_enabled(&material.feature_bits, "RECEIVES_DECALS") as u8,
    )
}

pub fn build_material_variants_v1(
    material: &MaterialIrV1,
    max_variants: usize,
) -> Result<Vec<MaterialShaderVariantV1>, String> {
    let bitset = stable_material_feature_bitset_key(material);
    let material_key = normalize_material_token(material.material_name.as_str());
    let variants = MaterialQualityTierV1::ALL
        .iter()
        .map(|quality_tier| MaterialShaderVariantV1 {
            variant_key: format!(
                "mat:{}:tier:{}:{}",
                material_key,
                quality_tier.as_str(),
                bitset
            ),
            quality_tier: *quality_tier,
            feature_bits: material.feature_bits.clone(),
        })
        .collect::<Vec<_>>();

    if variants.len() > max_variants {
        return Err(format!(
            "material '{}' variant count {} exceeds max {}",
            material.material_name,
            variants.len(),
            max_variants
        ));
    }

    Ok(variants)
}

fn estimate_material_cost(material: &MaterialIrV1, tier: MaterialQualityTierV1) -> (u32, u32) {
    let mut texture_fetch_count = 1u32 + material.texture_slots.len() as u32;
    let mut estimated_alu_ops = match material.surface_model.as_str() {
        "unlit" => 8u32,
        _ => 28u32,
    };

    if feature_enabled(&material.feature_bits, "HAS_NORMAL") {
        texture_fetch_count += 1;
        estimated_alu_ops += 12;
    }
    if feature_enabled(&material.feature_bits, "HAS_EMISSIVE") {
        texture_fetch_count += 1;
        estimated_alu_ops += 4;
    }
    if feature_enabled(&material.feature_bits, "CLEARCOAT") {
        texture_fetch_count += 1;
        estimated_alu_ops += 18;
    }
    if feature_enabled(&material.feature_bits, "TRANSMISSION") {
        texture_fetch_count += 1;
        estimated_alu_ops += 15;
    }
    if feature_enabled(&material.feature_bits, "ANISOTROPY") {
        estimated_alu_ops += 10;
    }
    if feature_enabled(&material.feature_bits, "SUBSURFACE_LITE") {
        estimated_alu_ops += 9;
    }

    let scale = match tier {
        MaterialQualityTierV1::Hero => 1.0,
        MaterialQualityTierV1::Gameplay => 0.8,
        MaterialQualityTierV1::Low => 0.6,
    };

    let tier_fetches = (texture_fetch_count as f32 * scale).ceil() as u32;
    let tier_alu = (estimated_alu_ops as f32 * scale).ceil() as u32;
    (tier_fetches.max(1), tier_alu.max(1))
}

pub fn compute_material_compile_report_v1(
    material: &MaterialIrV1,
    variants: &[MaterialShaderVariantV1],
) -> MaterialCompileReport {
    let mut quality_tier_costs = Vec::new();
    let mut max_fetch = 0u32;
    let mut max_alu = 0u32;
    for tier in MaterialQualityTierV1::ALL {
        let (texture_fetch_count, estimated_alu_ops) = estimate_material_cost(material, tier);
        max_fetch = max_fetch.max(texture_fetch_count);
        max_alu = max_alu.max(estimated_alu_ops);
        quality_tier_costs.push(MaterialQualityTierCost {
            quality_tier: tier,
            texture_fetch_count,
            estimated_alu_ops,
        });
    }
    let mut features_enabled = material.feature_bits.clone();
    features_enabled.push(format!("ALPHA_MODE={}", material.alpha_mode));
    features_enabled.sort();
    features_enabled.dedup();

    let explain_lines = vec![
        format!("surface_model={} drives BRDF path", material.surface_model),
        format!(
            "alpha_mode={} drives blend/discard behavior",
            material.alpha_mode
        ),
        format!(
            "features={} influence tiered ALU/fetch estimates",
            if material.feature_bits.is_empty() {
                "<none>".to_string()
            } else {
                material.feature_bits.join(",")
            }
        ),
        format!("generated {} quality-tier variants", variants.len()),
    ];

    MaterialCompileReport {
        texture_fetch_count: max_fetch,
        estimated_alu_ops: max_alu,
        variant_count: variants.len(),
        features_enabled,
        quality_tier_costs,
        explain_lines,
        runtime_texture_format: KTX2_RUNTIME_TEXTURE_FORMAT_V1.to_string(),
        texture_lint_lines: Vec::new(),
        texture_bindings: Vec::new(),
    }
}

pub fn validate_material_compile_report_v1(
    report: &MaterialCompileReport,
    gates: &MaterialCompileBudgetGatesV1,
) -> Result<(), String> {
    if report.texture_fetch_count > gates.max_texture_fetch_count {
        return Err(format!(
            "texture fetch budget exceeded: observed {}, max {}",
            report.texture_fetch_count, gates.max_texture_fetch_count
        ));
    }
    if report.estimated_alu_ops > gates.max_estimated_alu_ops {
        return Err(format!(
            "ALU budget exceeded: observed {}, max {}",
            report.estimated_alu_ops, gates.max_estimated_alu_ops
        ));
    }

    let observed_tiers = report
        .quality_tier_costs
        .iter()
        .map(|entry| entry.quality_tier)
        .collect::<BTreeSet<_>>();
    let expected_tiers = MaterialQualityTierV1::ALL
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    if report.quality_tier_costs.len() != gates.required_quality_tier_count
        || observed_tiers != expected_tiers
    {
        let observed = observed_tiers
            .iter()
            .map(|tier| tier.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let expected = MaterialQualityTierV1::ALL
            .iter()
            .map(|tier| tier.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "quality tier coverage invalid: expected exactly {} tiers [{}], observed {} entries [{}]",
            gates.required_quality_tier_count,
            expected,
            report.quality_tier_costs.len(),
            observed
        ));
    }

    if gates.require_explain_lines
        && report
            .explain_lines
            .iter()
            .all(|line| line.trim().is_empty())
    {
        return Err("compile explain report must include at least one non-empty line".to_string());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        build_material_variants_v1, compute_material_compile_report_v1, lower_to_wgsl_v2,
        shader_program_fingerprint, stable_material_feature_bitset_key,
        validate_material_compile_report_v1,
    };
    use crate::shader_compiler::types::{
        MaterialCompileBudgetGatesV1, MaterialIrV1, MaterialQualityTierCost, MaterialQualityTierV1,
        ShaderBindingV1, ShaderEntryPointV1, ShaderProgramIRV1, ShaderStageV1,
    };

    fn base_program() -> ShaderProgramIRV1 {
        ShaderProgramIRV1 {
            schema_version: 1,
            kind: "shader_program".to_string(),
            program_id: "prog_1".to_string(),
            bindings: vec![
                ShaderBindingV1 {
                    id: "camera".to_string(),
                    group: 0,
                    binding: 1,
                    resource_kind: "uniform_buffer".to_string(),
                    stage: ShaderStageV1::Vertex,
                },
                ShaderBindingV1 {
                    id: "particles".to_string(),
                    group: 0,
                    binding: 0,
                    resource_kind: "storage_buffer".to_string(),
                    stage: ShaderStageV1::Fragment,
                },
            ],
            entry_points: vec![
                ShaderEntryPointV1 {
                    id: "main_frag".to_string(),
                    function_name: "main_frag".to_string(),
                    stage: ShaderStageV1::Fragment,
                },
                ShaderEntryPointV1 {
                    id: "main_vert".to_string(),
                    function_name: "main_vert".to_string(),
                    stage: ShaderStageV1::Vertex,
                },
            ],
        }
    }

    #[test]
    fn lowering_is_deterministic_for_permuted_input_order() {
        let program_a = base_program();

        let mut program_b = base_program();
        program_b.bindings.reverse();
        program_b.entry_points.reverse();

        let wgsl_a = lower_to_wgsl_v2(&program_a).expect("lowering should succeed");
        let wgsl_b = lower_to_wgsl_v2(&program_b).expect("lowering should succeed");

        assert_eq!(wgsl_a, wgsl_b);
        assert!(wgsl_a.contains("var<uniform> camera: WrelaBindingType_camera;"));
        assert!(wgsl_a.contains("var<storage, read_write> particles: WrelaBindingType_particles;"));
        assert!(wgsl_a.contains("// entry_point_id: main_vert, stage: vertex"));
        assert!(!wgsl_a.contains("stub"));
    }

    #[test]
    fn lowering_emits_texture_and_sampler_bindings() {
        let mut program = base_program();
        program.bindings.push(ShaderBindingV1 {
            id: "albedo_tex".to_string(),
            group: 0,
            binding: 2,
            resource_kind: "texture".to_string(),
            stage: ShaderStageV1::Fragment,
        });
        program.bindings.push(ShaderBindingV1 {
            id: "linear_sampler".to_string(),
            group: 0,
            binding: 3,
            resource_kind: "sampler".to_string(),
            stage: ShaderStageV1::Fragment,
        });

        let wgsl = lower_to_wgsl_v2(&program).expect("lowering should succeed");
        assert!(wgsl.contains("@group(0) @binding(2) var albedo_tex: texture_2d<f32>;"));
        assert!(wgsl.contains("@group(0) @binding(3) var linear_sampler: sampler;"));
    }

    #[test]
    fn lowering_rejects_unsupported_resource_kind() {
        let mut program = base_program();
        program.bindings[0].resource_kind = "acceleration_structure".to_string();

        let err = lower_to_wgsl_v2(&program).expect_err("lowering should reject resource kind");
        assert!(
            err.contains("unsupported resource_kind"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn fingerprint_is_deterministic_for_permuted_input_order() {
        let program_a = base_program();

        let mut program_b = base_program();
        program_b.bindings.reverse();
        program_b.entry_points.reverse();

        let fingerprint_a =
            shader_program_fingerprint(&program_a).expect("fingerprint should succeed");
        let fingerprint_b =
            shader_program_fingerprint(&program_b).expect("fingerprint should succeed");

        assert_eq!(fingerprint_a, fingerprint_b);
        assert_eq!(fingerprint_a.len(), 64);
    }

    #[test]
    fn material_variant_keys_are_deterministic() {
        let material = MaterialIrV1 {
            schema_version: 1,
            material_name: "CarPaint".to_string(),
            surface_model: "pbr".to_string(),
            alpha_mode: "blend".to_string(),
            feature_bits: vec![
                "CLEARCOAT".to_string(),
                "HAS_EMISSIVE".to_string(),
                "ANISOTROPY".to_string(),
            ],
            texture_slots: vec!["albedo".to_string(), "normal".to_string()],
            params: Vec::new(),
        };
        let key_a = stable_material_feature_bitset_key(&material);
        let key_b = stable_material_feature_bitset_key(&material);
        assert_eq!(key_a, key_b);

        let variants_a =
            build_material_variants_v1(&material, 8).expect("variants should be generated");
        let variants_b =
            build_material_variants_v1(&material, 8).expect("variants should be generated");
        assert_eq!(variants_a, variants_b);
        assert_eq!(variants_a.len(), 3);
    }

    #[test]
    fn material_variant_generation_fails_when_threshold_exceeded() {
        let material = MaterialIrV1 {
            schema_version: 1,
            material_name: "Cloth".to_string(),
            surface_model: "pbr".to_string(),
            alpha_mode: "opaque".to_string(),
            feature_bits: Vec::new(),
            texture_slots: vec!["albedo".to_string()],
            params: Vec::new(),
        };
        let err = build_material_variants_v1(&material, 2)
            .expect_err("variant threshold should fail closed");
        assert!(
            err.contains("variant count 3 exceeds max 2"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn material_compile_report_contains_all_tiers() {
        let material = MaterialIrV1 {
            schema_version: 1,
            material_name: "Skin".to_string(),
            surface_model: "pbr".to_string(),
            alpha_mode: "opaque".to_string(),
            feature_bits: vec!["SUBSURFACE_LITE".to_string(), "HAS_NORMAL".to_string()],
            texture_slots: vec!["albedo".to_string(), "normal".to_string()],
            params: Vec::new(),
        };
        let variants = build_material_variants_v1(&material, 8).expect("variants should succeed");
        let report = compute_material_compile_report_v1(&material, variants.as_slice());
        assert_eq!(report.variant_count, 3);
        assert_eq!(report.quality_tier_costs.len(), 3);
        assert!(
            report
                .features_enabled
                .iter()
                .any(|feature| feature == "ALPHA_MODE=opaque")
        );
    }

    #[test]
    fn material_compile_report_budget_validation_passes_for_standard_report() {
        let material = MaterialIrV1 {
            schema_version: 1,
            material_name: "Rock".to_string(),
            surface_model: "pbr".to_string(),
            alpha_mode: "opaque".to_string(),
            feature_bits: vec!["HAS_NORMAL".to_string()],
            texture_slots: vec!["albedo".to_string(), "normal".to_string()],
            params: Vec::new(),
        };
        let variants = build_material_variants_v1(&material, 8).expect("variants should succeed");
        let report = compute_material_compile_report_v1(&material, variants.as_slice());
        let gates = MaterialCompileBudgetGatesV1 {
            max_texture_fetch_count: 12,
            max_estimated_alu_ops: 128,
            required_quality_tier_count: 3,
            require_explain_lines: true,
        };
        validate_material_compile_report_v1(&report, &gates)
            .expect("standard report should satisfy compile gates");
    }

    #[test]
    fn material_compile_report_budget_validation_fails_texture_fetch_budget() {
        let report = crate::shader_compiler::types::MaterialCompileReport {
            texture_fetch_count: 9,
            estimated_alu_ops: 64,
            variant_count: 3,
            features_enabled: vec!["HAS_NORMAL".to_string()],
            quality_tier_costs: vec![
                MaterialQualityTierCost {
                    quality_tier: MaterialQualityTierV1::Hero,
                    texture_fetch_count: 9,
                    estimated_alu_ops: 64,
                },
                MaterialQualityTierCost {
                    quality_tier: MaterialQualityTierV1::Gameplay,
                    texture_fetch_count: 7,
                    estimated_alu_ops: 52,
                },
                MaterialQualityTierCost {
                    quality_tier: MaterialQualityTierV1::Low,
                    texture_fetch_count: 5,
                    estimated_alu_ops: 40,
                },
            ],
            explain_lines: vec!["ok".to_string()],
            runtime_texture_format: "ktx2".to_string(),
            texture_lint_lines: Vec::new(),
            texture_bindings: Vec::new(),
        };
        let gates = MaterialCompileBudgetGatesV1 {
            max_texture_fetch_count: 8,
            max_estimated_alu_ops: 128,
            required_quality_tier_count: 3,
            require_explain_lines: true,
        };
        let err = validate_material_compile_report_v1(&report, &gates)
            .expect_err("texture budget violation must fail closed");
        assert!(
            err.contains("texture fetch budget exceeded"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn material_compile_report_budget_validation_fails_alu_budget() {
        let report = crate::shader_compiler::types::MaterialCompileReport {
            texture_fetch_count: 5,
            estimated_alu_ops: 101,
            variant_count: 3,
            features_enabled: vec!["CLEARCOAT".to_string()],
            quality_tier_costs: vec![
                MaterialQualityTierCost {
                    quality_tier: MaterialQualityTierV1::Hero,
                    texture_fetch_count: 5,
                    estimated_alu_ops: 101,
                },
                MaterialQualityTierCost {
                    quality_tier: MaterialQualityTierV1::Gameplay,
                    texture_fetch_count: 4,
                    estimated_alu_ops: 81,
                },
                MaterialQualityTierCost {
                    quality_tier: MaterialQualityTierV1::Low,
                    texture_fetch_count: 3,
                    estimated_alu_ops: 61,
                },
            ],
            explain_lines: vec!["ok".to_string()],
            runtime_texture_format: "ktx2".to_string(),
            texture_lint_lines: Vec::new(),
            texture_bindings: Vec::new(),
        };
        let gates = MaterialCompileBudgetGatesV1 {
            max_texture_fetch_count: 8,
            max_estimated_alu_ops: 100,
            required_quality_tier_count: 3,
            require_explain_lines: true,
        };
        let err = validate_material_compile_report_v1(&report, &gates)
            .expect_err("ALU budget violation must fail closed");
        assert!(
            err.contains("ALU budget exceeded"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn material_compile_report_budget_validation_fails_missing_quality_tier_coverage() {
        let report = crate::shader_compiler::types::MaterialCompileReport {
            texture_fetch_count: 5,
            estimated_alu_ops: 60,
            variant_count: 3,
            features_enabled: vec![],
            quality_tier_costs: vec![
                MaterialQualityTierCost {
                    quality_tier: MaterialQualityTierV1::Hero,
                    texture_fetch_count: 5,
                    estimated_alu_ops: 60,
                },
                MaterialQualityTierCost {
                    quality_tier: MaterialQualityTierV1::Gameplay,
                    texture_fetch_count: 4,
                    estimated_alu_ops: 48,
                },
            ],
            explain_lines: vec!["ok".to_string()],
            runtime_texture_format: "ktx2".to_string(),
            texture_lint_lines: Vec::new(),
            texture_bindings: Vec::new(),
        };
        let gates = MaterialCompileBudgetGatesV1 {
            max_texture_fetch_count: 8,
            max_estimated_alu_ops: 100,
            required_quality_tier_count: 3,
            require_explain_lines: true,
        };
        let err = validate_material_compile_report_v1(&report, &gates)
            .expect_err("missing tier coverage must fail closed");
        assert!(
            err.contains("quality tier coverage invalid"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn material_compile_report_budget_validation_fails_without_explain_lines() {
        let report = crate::shader_compiler::types::MaterialCompileReport {
            texture_fetch_count: 5,
            estimated_alu_ops: 60,
            variant_count: 3,
            features_enabled: vec![],
            quality_tier_costs: vec![
                MaterialQualityTierCost {
                    quality_tier: MaterialQualityTierV1::Hero,
                    texture_fetch_count: 5,
                    estimated_alu_ops: 60,
                },
                MaterialQualityTierCost {
                    quality_tier: MaterialQualityTierV1::Gameplay,
                    texture_fetch_count: 4,
                    estimated_alu_ops: 48,
                },
                MaterialQualityTierCost {
                    quality_tier: MaterialQualityTierV1::Low,
                    texture_fetch_count: 3,
                    estimated_alu_ops: 36,
                },
            ],
            explain_lines: vec![],
            runtime_texture_format: "ktx2".to_string(),
            texture_lint_lines: Vec::new(),
            texture_bindings: Vec::new(),
        };
        let gates = MaterialCompileBudgetGatesV1 {
            max_texture_fetch_count: 8,
            max_estimated_alu_ops: 100,
            required_quality_tier_count: 3,
            require_explain_lines: true,
        };
        let err = validate_material_compile_report_v1(&report, &gates)
            .expect_err("missing explain lines must fail closed");
        assert!(
            err.contains("compile explain report must include"),
            "unexpected error: {err}"
        );
    }
}
