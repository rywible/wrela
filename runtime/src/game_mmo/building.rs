use crate::game_mmo::style_pack::{StylePackContractV1, validate_material_style};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrimitiveV1 {
    Box,
    Sphere,
    Cylinder,
    Ramp,
    Arch,
    Roof,
    Beam,
    Pillar,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CsgOperationV1 {
    Add,
    Subtract,
    Intersect,
    SmoothBlend,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildingDiffV1 {
    pub diff_id: String,
    pub operation: CsgOperationV1,
    pub primitive: PrimitiveV1,
    pub transform_milli: [i64; 9],
    pub params_milli: Vec<i64>,
    pub style_hue: u16,
    pub style_roughness_milli: u16,
    pub style_normal_intensity_milli: u16,
    pub style_emission_milli: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildingArtifactPlanV1 {
    pub render_meshlet_count: u32,
    pub collision_proxy_count: u32,
    pub navmesh_patch_count: u32,
    pub lod_count: u32,
}

pub fn compile_building_diff(
    diff: &BuildingDiffV1,
    style: &StylePackContractV1,
) -> Result<BuildingArtifactPlanV1, String> {
    if diff.params_milli.is_empty() {
        return Err(format!(
            "building diff '{}' must include at least one parameter",
            diff.diff_id
        ));
    }
    if let Err(violations) = validate_material_style(
        style,
        diff.style_hue,
        diff.style_roughness_milli,
        diff.style_normal_intensity_milli,
        diff.style_emission_milli,
    ) {
        let details = violations
            .iter()
            .map(|violation| format!("{}: {}", violation.field, violation.message))
            .collect::<Vec<_>>()
            .join("; ");
        return Err(format!(
            "building diff '{}' violates style pack '{}': {}",
            diff.diff_id, style.style_pack_id, details
        ));
    }

    let complexity = u32::try_from(diff.params_milli.len())
        .unwrap_or(u32::MAX)
        .max(1);
    Ok(BuildingArtifactPlanV1 {
        render_meshlet_count: complexity * 4,
        collision_proxy_count: complexity * 2,
        navmesh_patch_count: complexity,
        lod_count: 3,
    })
}

#[cfg(test)]
mod tests {
    use super::{BuildingDiffV1, CsgOperationV1, PrimitiveV1, compile_building_diff};
    use crate::game_mmo::style_pack::StylePackContractV1;

    fn style() -> StylePackContractV1 {
        StylePackContractV1 {
            schema_version: 1,
            style_pack_id: "style-01".to_string(),
            hue_min: 10,
            hue_max: 90,
            roughness_min_milli: 100,
            roughness_max_milli: 900,
            normal_intensity_max_milli: 950,
            emission_max_milli: 250,
        }
    }

    #[test]
    fn building_compile_produces_artifact_plan() {
        let diff = BuildingDiffV1 {
            diff_id: "diff-1".to_string(),
            operation: CsgOperationV1::Add,
            primitive: PrimitiveV1::Box,
            transform_milli: [0; 9],
            params_milli: vec![1000, 2000, 3000],
            style_hue: 40,
            style_roughness_milli: 500,
            style_normal_intensity_milli: 300,
            style_emission_milli: 100,
        };

        let plan = compile_building_diff(&diff, &style()).expect("building diff should compile");
        assert_eq!(plan.navmesh_patch_count, 3);
    }

    #[test]
    fn building_compile_rejects_style_violation() {
        let diff = BuildingDiffV1 {
            diff_id: "diff-2".to_string(),
            operation: CsgOperationV1::Add,
            primitive: PrimitiveV1::Pillar,
            transform_milli: [0; 9],
            params_milli: vec![1000],
            style_hue: 150,
            style_roughness_milli: 500,
            style_normal_intensity_milli: 300,
            style_emission_milli: 100,
        };

        let err = compile_building_diff(&diff, &style()).expect_err("must reject style violation");
        assert!(err.contains("violates style pack"));
    }
}
