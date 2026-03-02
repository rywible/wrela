use crate::hir::def::{Param, ShaderFunction};
use crate::hir::render_shader_ir::ShaderVariant;
use crate::hir::typeck::Type;
use crate::shader_compiler::types::{
    MaterialCompileReport, MaterialIrV1, MaterialQualityTierV1, MaterialShaderVariantV1,
};
use std::fmt::Write as _;

/// Convert a Wrela Type to a WGSL type string.
pub fn wgsl_type_name(ty: &Type) -> &'static str {
    match ty {
        Type::Float => "f32",
        Type::Integer => "i32",
        Type::Vec2 => "vec2<f32>",
        Type::Vec3 => "vec3<f32>",
        Type::Vec4 => "vec4<f32>",
        Type::Mat4 => "mat4x4<f32>",
        Type::Boolean => "bool",
        Type::Texture2D => "texture_2d<f32>",
        Type::Sampler => "sampler",
        _ => "f32", // fallback
    }
}

/// Convert a TypeRef name to a WGSL type string.
pub fn wgsl_type_from_ref_name(name: &str) -> &'static str {
    match name {
        "Float" => "f32",
        "Integer" => "i32",
        "Vec2" => "vec2<f32>",
        "Vec3" => "vec3<f32>",
        "Vec4" => "vec4<f32>",
        "Mat4" => "mat4x4<f32>",
        "Boolean" => "bool",
        "Texture2D" => "texture_2d<f32>",
        "Sampler" => "sampler",
        _ => "f32",
    }
}

/// Generate a WGSL function signature from a shader function.
pub fn generate_wgsl_signature(shader: &ShaderFunction) -> String {
    let params: Vec<String> = shader
        .params
        .iter()
        .map(|p| {
            let ty_name =
                p.ty.as_ref()
                    .map(|t| wgsl_type_from_ref_name(t.name.as_str()))
                    .unwrap_or("f32");
            format!("{}: {}", p.name, ty_name)
        })
        .collect();

    let ret = shader
        .ret_type
        .as_ref()
        .map(|t| wgsl_type_from_ref_name(t.name.as_str()))
        .unwrap_or("f32");

    format!("fn {}({}) -> {}", shader.name, params.join(", "), ret)
}

/// Generate a complete WGSL function stub from a shader function.
/// The body is a placeholder returning a zero-initialized value.
pub fn generate_wgsl_function(shader: &ShaderFunction) -> String {
    let sig = generate_wgsl_signature(shader);
    let ret_type = shader
        .ret_type
        .as_ref()
        .map(|t| t.name.as_str())
        .unwrap_or("Float");
    let zero_val = wgsl_zero_value(ret_type);
    format!("{} {{\n  return {};\n}}", sig, zero_val)
}

/// Generate a WGSL function with feature-gate preprocessor-style defines.
pub fn generate_wgsl_variant(shader: &ShaderFunction, variant: &ShaderVariant) -> String {
    let mut defines = String::new();
    for feature in &variant.enabled_features {
        defines.push_str(&format!("// #define {}\n", feature));
    }

    let ret_type = shader
        .ret_type
        .as_ref()
        .map(|t| t.name.as_str())
        .unwrap_or("Float");
    let zero_val = wgsl_zero_value(ret_type);

    let params_str: String = shader
        .params
        .iter()
        .map(|p| {
            let ty_name =
                p.ty.as_ref()
                    .map(|t| wgsl_type_from_ref_name(t.name.as_str()))
                    .unwrap_or("f32");
            format!("{}: {}", p.name, ty_name)
        })
        .collect::<Vec<_>>()
        .join(", ");

    let ret_wgsl = shader
        .ret_type
        .as_ref()
        .map(|t| wgsl_type_from_ref_name(t.name.as_str()))
        .unwrap_or("f32");

    format!(
        "{}fn {}({}) -> {} {{\n  return {};\n}}",
        defines, variant.variant_key, params_str, ret_wgsl, zero_val,
    )
}

fn material_feature_enabled(material: &MaterialIrV1, feature: &str) -> bool {
    material.feature_bits.iter().any(|entry| entry == feature)
}

fn material_param_f32(material: &MaterialIrV1, name: &str, default: f32) -> f32 {
    material
        .params
        .iter()
        .find(|param| param.name == name)
        .and_then(|param| param.value.parse::<f32>().ok())
        .unwrap_or(default)
}

fn tier_energy_scale(tier: MaterialQualityTierV1) -> f32 {
    match tier {
        MaterialQualityTierV1::Hero => 1.0,
        MaterialQualityTierV1::Gameplay => 0.85,
        MaterialQualityTierV1::Low => 0.65,
    }
}

fn tier_color_boost(tier: MaterialQualityTierV1) -> f32 {
    match tier {
        MaterialQualityTierV1::Hero => 1.0,
        MaterialQualityTierV1::Gameplay => 0.9,
        MaterialQualityTierV1::Low => 0.8,
    }
}

pub fn generate_material_wgsl_v1(
    material: &MaterialIrV1,
    active_tier: MaterialQualityTierV1,
    variants: &[MaterialShaderVariantV1],
    report: &MaterialCompileReport,
) -> String {
    let metallic = material_param_f32(material, "metallic", 0.1).clamp(0.0, 1.0);
    let roughness = material_param_f32(material, "roughness", 0.55).clamp(0.045, 1.0);
    let clearcoat_weight = material_param_f32(material, "clearcoat_weight", 0.35).clamp(0.0, 1.0);
    let clearcoat_roughness =
        material_param_f32(material, "clearcoat_roughness", 0.2).clamp(0.045, 1.0);
    let sheen_weight = material_param_f32(material, "sheen_weight", 0.25).clamp(0.0, 1.0);
    let transmission = material_param_f32(material, "transmission", 0.5).clamp(0.0, 1.0);
    let alpha = material_param_f32(
        material,
        "alpha",
        match material.alpha_mode.as_str() {
            "blend" => 0.62,
            "mask" => 0.45,
            _ => 1.0,
        },
    )
    .clamp(0.0, 1.0);

    let mut output = String::new();
    let _ = writeln!(output, "// material shader wgsl v1");
    let _ = writeln!(output, "// material: {}", material.material_name);
    let _ = writeln!(output, "// surface_model: {}", material.surface_model);
    let _ = writeln!(output, "// alpha_mode: {}", material.alpha_mode);
    let _ = writeln!(output, "// active_quality_tier: {}", active_tier.as_str());
    let _ = writeln!(output, "// variant_count: {}", report.variant_count);
    for variant in variants {
        let _ = writeln!(
            output,
            "// variant {} => {}",
            variant.quality_tier.as_str(),
            variant.variant_key
        );
    }
    for line in &report.explain_lines {
        let _ = writeln!(output, "// explain: {}", line);
    }
    output.push('\n');
    output.push_str("struct VsOut {\n");
    output.push_str("  @builtin(position) position: vec4<f32>,\n");
    output.push_str("};\n\n");

    output.push_str("@vertex\n");
    output.push_str("fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VsOut {\n");
    output.push_str("  var out: VsOut;\n");
    output.push_str("  var x = f32((vertex_index << 1u) & 2u);\n");
    output.push_str("  var y = f32(vertex_index & 2u);\n");
    output.push_str("  out.position = vec4<f32>(x - 1.0, 1.0 - y, 0.0, 1.0);\n");
    output.push_str("  return out;\n");
    output.push_str("}\n\n");

    output.push_str("const PI: f32 = 3.141592653589793;\n\n");
    output.push_str("fn saturate1(value: f32) -> f32 {\n");
    output.push_str("  return clamp(value, 0.0, 1.0);\n");
    output.push_str("}\n\n");
    output.push_str("fn saturate3(value: vec3<f32>) -> vec3<f32> {\n");
    output.push_str(
        "  return vec3<f32>(saturate1(value.x), saturate1(value.y), saturate1(value.z));\n",
    );
    output.push_str("}\n\n");
    output.push_str("fn fresnel_schlick(cos_theta: f32, f0: vec3<f32>) -> vec3<f32> {\n");
    output.push_str("  let one_minus = saturate1(1.0 - cos_theta);\n");
    output.push_str("  return f0 + (vec3<f32>(1.0, 1.0, 1.0) - f0) * pow(one_minus, 5.0);\n");
    output.push_str("}\n\n");
    output.push_str(
        "fn distribution_ggx(normal: vec3<f32>, half_vector: vec3<f32>, roughness: f32) -> f32 {\n",
    );
    output.push_str("  let alpha = roughness * roughness;\n");
    output.push_str("  let alpha2 = alpha * alpha;\n");
    output.push_str("  let n_dot_h = max(dot(normal, half_vector), 0.0);\n");
    output.push_str("  let n_dot_h2 = n_dot_h * n_dot_h;\n");
    output.push_str("  let denom = (n_dot_h2 * (alpha2 - 1.0) + 1.0);\n");
    output.push_str("  return alpha2 / max(PI * denom * denom, 0.0001);\n");
    output.push_str("}\n\n");
    output.push_str("fn geometry_schlick_ggx(n_dot_v: f32, roughness: f32) -> f32 {\n");
    output.push_str("  let r = roughness + 1.0;\n");
    output.push_str("  let k = (r * r) / 8.0;\n");
    output.push_str("  return n_dot_v / max(n_dot_v * (1.0 - k) + k, 0.0001);\n");
    output.push_str("}\n\n");
    output.push_str("fn geometry_smith(normal: vec3<f32>, view: vec3<f32>, light: vec3<f32>, roughness: f32) -> f32 {\n");
    output.push_str("  let n_dot_v = max(dot(normal, view), 0.0);\n");
    output.push_str("  let n_dot_l = max(dot(normal, light), 0.0);\n");
    output.push_str("  return geometry_schlick_ggx(n_dot_v, roughness) * geometry_schlick_ggx(n_dot_l, roughness);\n");
    output.push_str("}\n\n");

    output.push_str("fn apply_clearcoat_layer(base: vec3<f32>, normal: vec3<f32>, view: vec3<f32>, light: vec3<f32>, weight: f32, roughness: f32) -> vec3<f32> {\n");
    output.push_str("  let half_vector = normalize(view + light);\n");
    output.push_str("  let d = distribution_ggx(normal, half_vector, roughness);\n");
    output.push_str("  let g = geometry_smith(normal, view, light, roughness);\n");
    output.push_str("  let f = fresnel_schlick(max(dot(half_vector, view), 0.0), vec3<f32>(0.04, 0.04, 0.04));\n");
    output.push_str("  let n_dot_l = max(dot(normal, light), 0.0);\n");
    output.push_str("  let n_dot_v = max(dot(normal, view), 0.0);\n");
    output.push_str("  let spec = (d * g * f) / max(4.0 * n_dot_l * n_dot_v, 0.0001);\n");
    output.push_str("  return base + spec * weight * n_dot_l;\n");
    output.push_str("}\n\n");
    output.push_str("fn apply_sheen_layer(base: vec3<f32>, normal: vec3<f32>, view: vec3<f32>, light: vec3<f32>, sheen_color: vec3<f32>, weight: f32) -> vec3<f32> {\n");
    output.push_str("  let n_dot_l = max(dot(normal, light), 0.0);\n");
    output.push_str("  let l_dot_h = max(dot(light, normalize(light + view)), 0.0);\n");
    output.push_str("  let sheen_term = pow(1.0 - l_dot_h, 5.0);\n");
    output.push_str("  return base + sheen_color * sheen_term * weight * n_dot_l;\n");
    output.push_str("}\n\n");
    output.push_str("fn apply_transmission_layer(base: vec3<f32>, normal: vec3<f32>, view: vec3<f32>, light: vec3<f32>, transmission: f32) -> vec3<f32> {\n");
    output.push_str("  let n_dot_l = max(dot(normal, light), 0.0);\n");
    output.push_str("  let transmission_color = vec3<f32>(0.88, 0.93, 1.0);\n");
    output.push_str("  let transmittance = transmission * (1.0 - n_dot_l * 0.55);\n");
    output.push_str("  return mix(base, transmission_color, saturate1(transmittance));\n");
    output.push_str("}\n\n");

    for tier in MaterialQualityTierV1::ALL {
        let _ = writeln!(
            output,
            "fn evaluate_brdf_{}() -> vec3<f32> {{",
            tier.as_str()
        );
        output.push_str("  let normal = normalize(vec3<f32>(0.0, 0.0, 1.0));\n");
        output.push_str("  let view = normalize(vec3<f32>(0.0, 0.0, 1.0));\n");
        output.push_str("  let light = normalize(vec3<f32>(0.45, 0.55, 0.73));\n");
        output.push_str("  let half_vector = normalize(view + light);\n");
        if material.surface_model == "unlit" {
            output.push_str("  var base_color = vec3<f32>(0.95, 0.95, 0.95);\n");
        } else {
            output.push_str("  var base_color = vec3<f32>(0.64, 0.62, 0.57);\n");
        }
        let _ = writeln!(
            output,
            "  base_color = base_color * vec3<f32>({:.3}, {:.3}, {:.3});",
            tier_color_boost(tier),
            tier_color_boost(tier),
            tier_color_boost(tier)
        );
        let _ = writeln!(output, "  let metallic = {:.5};", metallic);
        let _ = writeln!(output, "  let roughness = {:.5};", roughness);
        output.push_str("  let n_dot_l = max(dot(normal, light), 0.0);\n");
        output.push_str("  let n_dot_v = max(dot(normal, view), 0.0);\n");
        output.push_str("  let f0_dielectric = vec3<f32>(0.04, 0.04, 0.04);\n");
        output.push_str("  let f0 = mix(f0_dielectric, base_color, metallic);\n");
        output.push_str("  let fresnel = fresnel_schlick(max(dot(half_vector, view), 0.0), f0);\n");
        output.push_str("  let distribution = distribution_ggx(normal, half_vector, roughness);\n");
        output.push_str("  let geometry = geometry_smith(normal, view, light, roughness);\n");
        output.push_str("  let specular = (distribution * geometry * fresnel) / max(4.0 * n_dot_l * n_dot_v, 0.0001);\n");
        output.push_str("  let kd = (vec3<f32>(1.0, 1.0, 1.0) - fresnel) * (1.0 - metallic);\n");
        output.push_str("  let diffuse = kd * base_color / PI;\n");
        output.push_str("  let ambient = base_color * 0.02;\n");
        output.push_str("  var color = (diffuse + specular) * n_dot_l + ambient;\n");

        if material_feature_enabled(material, "HAS_NORMAL") {
            output.push_str("  color = color + vec3<f32>(0.01, 0.01, 0.01);\n");
        }
        if material_feature_enabled(material, "CLEARCOAT") {
            let _ = writeln!(
                output,
                "  color = apply_clearcoat_layer(color, normal, view, light, {:.5}, {:.5});",
                clearcoat_weight, clearcoat_roughness
            );
        }
        if material_feature_enabled(material, "SHEEN") {
            let _ = writeln!(
                output,
                "  color = apply_sheen_layer(color, normal, view, light, vec3<f32>(1.0, 0.2, 0.2), {:.5});",
                sheen_weight
            );
        }
        if material_feature_enabled(material, "ANISOTROPY") {
            output.push_str("  color = color + vec3<f32>(0.025, 0.008, 0.005);\n");
        }
        if material_feature_enabled(material, "SUBSURFACE_LITE") {
            output.push_str("  color = mix(color, vec3<f32>(0.95, 0.58, 0.52), 0.08);\n");
        }
        if material_feature_enabled(material, "TRANSMISSION") {
            let _ = writeln!(
                output,
                "  color = apply_transmission_layer(color, normal, view, light, {:.5});",
                transmission
            );
        }
        if material_feature_enabled(material, "HAS_EMISSIVE") {
            let _ = writeln!(
                output,
                "  color = color + vec3<f32>({:.3}, {:.3}, {:.3});",
                0.02 * tier_energy_scale(tier),
                0.03 * tier_energy_scale(tier),
                0.04 * tier_energy_scale(tier)
            );
        }
        output.push_str("  return saturate3(color);\n");
        output.push_str("}\n\n");
    }

    output.push_str("@fragment\n");
    output.push_str("fn fs_main() -> @location(0) vec4<f32> {\n");
    let _ = writeln!(
        output,
        "  var color = evaluate_brdf_{}();",
        active_tier.as_str()
    );
    let _ = writeln!(output, "  var alpha = {:.5};", alpha);
    if material.alpha_mode == "mask" {
        output.push_str("  if (alpha < 0.5) { discard; }\n");
    }
    output.push_str("  return vec4<f32>(saturate3(color), saturate1(alpha));\n");
    output.push_str("}\n");
    output
}

fn wgsl_zero_value(wrela_type_name: &str) -> &'static str {
    match wrela_type_name {
        "Float" => "0.0",
        "Integer" => "0",
        "Vec2" => "vec2<f32>(0.0, 0.0)",
        "Vec3" => "vec3<f32>(0.0, 0.0, 0.0)",
        "Vec4" => "vec4<f32>(0.0, 0.0, 0.0, 0.0)",
        "Mat4" => "mat4x4<f32>()",
        "Boolean" => "false",
        _ => "0.0",
    }
}

/// Generate a WGSL struct definition from a list of params (used for
/// uniform buffer binding structs).
pub fn generate_wgsl_struct(name: &str, params: &[Param]) -> String {
    let mut out = format!("struct {} {{\n", name);
    for param in params {
        let ty = param
            .ty
            .as_ref()
            .map(|t| wgsl_type_from_ref_name(t.name.as_str()))
            .unwrap_or("f32");
        out.push_str(&format!("  {}: {},\n", param.name, ty));
    }
    out.push_str("}\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hir::def::{Param, ShaderFunction, TypeRef};
    use smol_str::SmolStr;

    fn make_type_ref(name: &str) -> TypeRef {
        TypeRef {
            name: SmolStr::new(name),
            name_span: None,
            args: Vec::new(),
        }
    }

    fn make_param(name: &str, ty_name: &str) -> Param {
        Param {
            name: SmolStr::new(name),
            name_span: None,
            ty: Some(make_type_ref(ty_name)),
        }
    }

    fn make_shader(name: &str, params: Vec<Param>, ret: &str) -> ShaderFunction {
        ShaderFunction {
            name: SmolStr::new(name),
            name_span: None,
            params,
            ret_type: Some(make_type_ref(ret)),
            body: None,
        }
    }

    #[test]
    fn wgsl_type_mapping() {
        assert_eq!(wgsl_type_name(&Type::Float), "f32");
        assert_eq!(wgsl_type_name(&Type::Integer), "i32");
        assert_eq!(wgsl_type_name(&Type::Vec2), "vec2<f32>");
        assert_eq!(wgsl_type_name(&Type::Vec3), "vec3<f32>");
        assert_eq!(wgsl_type_name(&Type::Vec4), "vec4<f32>");
        assert_eq!(wgsl_type_name(&Type::Mat4), "mat4x4<f32>");
        assert_eq!(wgsl_type_name(&Type::Texture2D), "texture_2d<f32>");
        assert_eq!(wgsl_type_name(&Type::Sampler), "sampler");
    }

    #[test]
    fn generate_signature_basic() {
        let shader = make_shader(
            "vertex_main",
            vec![make_param("pos", "Vec3"), make_param("mvp", "Mat4")],
            "Vec4",
        );
        let sig = generate_wgsl_signature(&shader);
        assert_eq!(
            sig,
            "fn vertex_main(pos: vec3<f32>, mvp: mat4x4<f32>) -> vec4<f32>"
        );
    }

    #[test]
    fn generate_function_returns_zero() {
        let shader = make_shader("frag", vec![make_param("uv", "Vec2")], "Vec4");
        let code = generate_wgsl_function(&shader);
        assert!(code.contains("fn frag(uv: vec2<f32>) -> vec4<f32>"));
        assert!(code.contains("return vec4<f32>(0.0, 0.0, 0.0, 0.0)"));
    }

    #[test]
    fn generate_struct_definition() {
        let params = vec![
            make_param("model", "Mat4"),
            make_param("color", "Vec4"),
            make_param("time", "Float"),
        ];
        let code = generate_wgsl_struct("Uniforms", &params);
        assert!(code.contains("struct Uniforms {"));
        assert!(code.contains("model: mat4x4<f32>,"));
        assert!(code.contains("color: vec4<f32>,"));
        assert!(code.contains("time: f32,"));
    }

    #[test]
    fn scalar_return_types() {
        let shader = make_shader("intensity", vec![], "Float");
        let code = generate_wgsl_function(&shader);
        assert!(code.contains("-> f32"));
        assert!(code.contains("return 0.0"));

        let shader = make_shader("count", vec![], "Integer");
        let code = generate_wgsl_function(&shader);
        assert!(code.contains("-> i32"));
        assert!(code.contains("return 0"));
    }

    #[test]
    fn material_codegen_changes_with_surface_and_alpha() {
        let pbr = MaterialIrV1 {
            schema_version: 1,
            material_name: "PbrMat".to_string(),
            surface_model: "pbr".to_string(),
            alpha_mode: "opaque".to_string(),
            feature_bits: vec![
                "HAS_NORMAL".to_string(),
                "CLEARCOAT".to_string(),
                "TRANSMISSION".to_string(),
            ],
            texture_slots: vec!["albedo".to_string()],
            params: vec![
                crate::shader_compiler::types::MaterialParamV1 {
                    name: "metallic".to_string(),
                    value: "0.4".to_string(),
                },
                crate::shader_compiler::types::MaterialParamV1 {
                    name: "roughness".to_string(),
                    value: "0.35".to_string(),
                },
            ],
        };
        let unlit = MaterialIrV1 {
            schema_version: 1,
            material_name: "UiMat".to_string(),
            surface_model: "unlit".to_string(),
            alpha_mode: "mask".to_string(),
            feature_bits: Vec::new(),
            texture_slots: vec!["albedo".to_string()],
            params: Vec::new(),
        };
        let variants = vec![MaterialShaderVariantV1 {
            variant_key: "v".to_string(),
            quality_tier: MaterialQualityTierV1::Hero,
            feature_bits: Vec::new(),
        }];
        let report = MaterialCompileReport {
            texture_fetch_count: 2,
            estimated_alu_ops: 16,
            variant_count: 1,
            features_enabled: Vec::new(),
            quality_tier_costs: Vec::new(),
            explain_lines: vec!["ok".to_string()],
            runtime_texture_format: "ktx2".to_string(),
            texture_lint_lines: Vec::new(),
            texture_bindings: Vec::new(),
        };
        let pbr_code = generate_material_wgsl_v1(
            &pbr,
            MaterialQualityTierV1::Hero,
            variants.as_slice(),
            &report,
        );
        let unlit_code = generate_material_wgsl_v1(
            &unlit,
            MaterialQualityTierV1::Low,
            variants.as_slice(),
            &report,
        );
        assert_ne!(pbr_code, unlit_code);
        assert!(unlit_code.contains("discard"));
        assert!(pbr_code.contains("distribution_ggx"));
        assert!(pbr_code.contains("geometry_smith"));
        assert!(pbr_code.contains("fresnel_schlick"));
        assert!(pbr_code.contains("evaluate_brdf_hero"));
        assert!(pbr_code.contains("apply_clearcoat_layer"));
        assert!(pbr_code.contains("apply_transmission_layer"));
    }

    #[test]
    fn pbr_brdf_reference_parity() {
        let material = MaterialIrV1 {
            schema_version: 1,
            material_name: "ParityMat".to_string(),
            surface_model: "pbr".to_string(),
            alpha_mode: "opaque".to_string(),
            feature_bits: vec!["HAS_NORMAL".to_string()],
            texture_slots: vec!["albedo".to_string(), "orm".to_string()],
            params: vec![
                crate::shader_compiler::types::MaterialParamV1 {
                    name: "metallic".to_string(),
                    value: "0.33".to_string(),
                },
                crate::shader_compiler::types::MaterialParamV1 {
                    name: "roughness".to_string(),
                    value: "0.47".to_string(),
                },
            ],
        };
        let variants = vec![MaterialShaderVariantV1 {
            variant_key: "hero".to_string(),
            quality_tier: MaterialQualityTierV1::Hero,
            feature_bits: vec!["HAS_NORMAL".to_string()],
        }];
        let report = MaterialCompileReport {
            texture_fetch_count: 3,
            estimated_alu_ops: 42,
            variant_count: 1,
            features_enabled: vec!["HAS_NORMAL".to_string()],
            quality_tier_costs: Vec::new(),
            explain_lines: Vec::new(),
            runtime_texture_format: "ktx2".to_string(),
            texture_lint_lines: Vec::new(),
            texture_bindings: Vec::new(),
        };
        let code = generate_material_wgsl_v1(
            &material,
            MaterialQualityTierV1::Hero,
            variants.as_slice(),
            &report,
        );
        assert!(code.contains("fn fresnel_schlick"));
        assert!(code.contains("fn distribution_ggx"));
        assert!(code.contains("fn geometry_smith"));
        assert!(code.contains("let specular = (distribution * geometry * fresnel)"));
        assert!(code.contains("let kd = (vec3<f32>(1.0, 1.0, 1.0) - fresnel) * (1.0 - metallic);"));
    }

    #[test]
    fn material_feature_matrix_compiles() {
        let variants = vec![MaterialShaderVariantV1 {
            variant_key: "g".to_string(),
            quality_tier: MaterialQualityTierV1::Gameplay,
            feature_bits: Vec::new(),
        }];
        let report = MaterialCompileReport {
            texture_fetch_count: 1,
            estimated_alu_ops: 12,
            variant_count: 1,
            features_enabled: Vec::new(),
            quality_tier_costs: Vec::new(),
            explain_lines: Vec::new(),
            runtime_texture_format: "ktx2".to_string(),
            texture_lint_lines: Vec::new(),
            texture_bindings: Vec::new(),
        };

        let matrix = vec![
            vec!["CLEARCOAT".to_string()],
            vec!["SHEEN".to_string()],
            vec!["TRANSMISSION".to_string()],
            vec![
                "CLEARCOAT".to_string(),
                "SHEEN".to_string(),
                "TRANSMISSION".to_string(),
            ],
        ];
        for features in matrix {
            let material = MaterialIrV1 {
                schema_version: 1,
                material_name: "FeatureMat".to_string(),
                surface_model: "pbr".to_string(),
                alpha_mode: "opaque".to_string(),
                feature_bits: features.clone(),
                texture_slots: vec!["albedo".to_string(), "orm".to_string()],
                params: Vec::new(),
            };
            let code = generate_material_wgsl_v1(
                &material,
                MaterialQualityTierV1::Gameplay,
                variants.as_slice(),
                &report,
            );
            assert!(code.contains("@fragment"));
            if features.iter().any(|f| f == "CLEARCOAT") {
                assert!(code.contains("apply_clearcoat_layer"));
            }
            if features.iter().any(|f| f == "SHEEN") {
                assert!(code.contains("apply_sheen_layer"));
            }
            if features.iter().any(|f| f == "TRANSMISSION") {
                assert!(code.contains("apply_transmission_layer"));
            }
        }
    }

    #[test]
    fn alpha_modes_emit_expected_paths() {
        let make = |alpha_mode: &str| MaterialIrV1 {
            schema_version: 1,
            material_name: format!("Alpha{alpha_mode}"),
            surface_model: "pbr".to_string(),
            alpha_mode: alpha_mode.to_string(),
            feature_bits: Vec::new(),
            texture_slots: vec!["albedo".to_string()],
            params: Vec::new(),
        };
        let variants = vec![MaterialShaderVariantV1 {
            variant_key: "low".to_string(),
            quality_tier: MaterialQualityTierV1::Low,
            feature_bits: Vec::new(),
        }];
        let report = MaterialCompileReport {
            texture_fetch_count: 0,
            estimated_alu_ops: 8,
            variant_count: 1,
            features_enabled: Vec::new(),
            quality_tier_costs: Vec::new(),
            explain_lines: Vec::new(),
            runtime_texture_format: "ktx2".to_string(),
            texture_lint_lines: Vec::new(),
            texture_bindings: Vec::new(),
        };

        let opaque = generate_material_wgsl_v1(
            &make("opaque"),
            MaterialQualityTierV1::Low,
            variants.as_slice(),
            &report,
        );
        let mask = generate_material_wgsl_v1(
            &make("mask"),
            MaterialQualityTierV1::Low,
            variants.as_slice(),
            &report,
        );
        let blend = generate_material_wgsl_v1(
            &make("blend"),
            MaterialQualityTierV1::Low,
            variants.as_slice(),
            &report,
        );

        assert!(opaque.contains("var alpha = 1.00000;"));
        assert!(blend.contains("var alpha = 0.62000;"));
        assert!(mask.contains("if (alpha < 0.5) { discard; }"));
        assert!(!opaque.contains("discard"));
        assert!(!blend.contains("discard"));
    }
}
