use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StylePackContractV1 {
    pub schema_version: u32,
    pub style_pack_id: String,
    pub hue_min: u16,
    pub hue_max: u16,
    pub roughness_min_milli: u16,
    pub roughness_max_milli: u16,
    pub normal_intensity_max_milli: u16,
    pub emission_max_milli: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StyleViolationV1 {
    pub field: String,
    pub message: String,
}

pub fn validate_style_contract(contract: &StylePackContractV1) -> Result<(), String> {
    if contract.schema_version != 1 {
        return Err(format!(
            "unsupported style pack schema_version {}",
            contract.schema_version
        ));
    }
    if contract.hue_min > contract.hue_max {
        return Err("style hue_min must be <= hue_max".to_string());
    }
    if contract.roughness_min_milli > contract.roughness_max_milli {
        return Err("style roughness_min_milli must be <= roughness_max_milli".to_string());
    }
    Ok(())
}

pub fn validate_material_style(
    contract: &StylePackContractV1,
    hue: u16,
    roughness_milli: u16,
    normal_intensity_milli: u16,
    emission_milli: u16,
) -> Result<(), Vec<StyleViolationV1>> {
    let mut violations = Vec::<StyleViolationV1>::new();

    if hue < contract.hue_min || hue > contract.hue_max {
        violations.push(StyleViolationV1 {
            field: "hue".to_string(),
            message: format!(
                "hue {} outside style range [{}..={}]",
                hue, contract.hue_min, contract.hue_max
            ),
        });
    }
    if roughness_milli < contract.roughness_min_milli
        || roughness_milli > contract.roughness_max_milli
    {
        violations.push(StyleViolationV1 {
            field: "roughness".to_string(),
            message: format!(
                "roughness {} outside style range [{}..={}]",
                roughness_milli, contract.roughness_min_milli, contract.roughness_max_milli
            ),
        });
    }
    if normal_intensity_milli > contract.normal_intensity_max_milli {
        violations.push(StyleViolationV1 {
            field: "normal_intensity".to_string(),
            message: format!(
                "normal intensity {} exceeds style max {}",
                normal_intensity_milli, contract.normal_intensity_max_milli
            ),
        });
    }
    if emission_milli > contract.emission_max_milli {
        violations.push(StyleViolationV1 {
            field: "emission".to_string(),
            message: format!(
                "emission {} exceeds style max {}",
                emission_milli, contract.emission_max_milli
            ),
        });
    }

    if violations.is_empty() {
        Ok(())
    } else {
        Err(violations)
    }
}

#[cfg(test)]
mod tests {
    use super::{StylePackContractV1, validate_material_style, validate_style_contract};

    fn contract() -> StylePackContractV1 {
        StylePackContractV1 {
            schema_version: 1,
            style_pack_id: "style-a".to_string(),
            hue_min: 20,
            hue_max: 120,
            roughness_min_milli: 150,
            roughness_max_milli: 850,
            normal_intensity_max_milli: 900,
            emission_max_milli: 300,
        }
    }

    #[test]
    fn style_contract_validates() {
        validate_style_contract(&contract()).expect("contract should validate");
    }

    #[test]
    fn style_validation_rejects_out_of_bounds_material() {
        let err = validate_material_style(&contract(), 10, 900, 950, 400)
            .expect_err("style validation should fail");
        assert!(err.len() >= 3);
    }
}
