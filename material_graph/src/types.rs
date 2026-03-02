use serde::{Deserialize, Serialize, de};
use std::collections::BTreeMap;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MaterialNode {
    pub id: String,
    pub kind: String,
    pub params: BTreeMap<String, f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MaterialEdge {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MaterialGraphIRV1 {
    pub schema_version: u32,
    pub kind: String,
    pub graph_id: String,
    pub nodes: Vec<MaterialNode>,
    pub edges: Vec<MaterialEdge>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DefaultProfileContractsV1 {
    pub schema_version: u32,
    pub profile: String,
    pub lighting: LightingContractV1,
    pub reflections: ReflectionFallbackContractV1,
    pub temporal: TemporalStackContractV1,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LightingContractV1 {
    pub pbr_enabled: bool,
    pub hdr_enabled: bool,
    pub tonemap_operator: TonemapOperator,
    pub clustered_lighting: ClusteredLightingContractV1,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClusteredLightingContractV1 {
    pub enabled: bool,
    pub max_lights_per_cluster: u32,
    pub shadow: ShadowContractV1,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShadowContractV1 {
    pub enabled: bool,
    pub cascade_count: u32,
    pub atlas_resolution: u32,
    #[serde(default)]
    pub quality_tier: ShadowQualityTier,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReflectionFallbackContractV1 {
    pub fallback_chain: Vec<ReflectionFallbackMode>,
    pub planar_budget: ReflectionPlanarBudgetV1,
    pub ssr_budget: ReflectionSsrBudgetV1,
    pub probe_budget: ReflectionProbeBudgetV1,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReflectionPlanarBudgetV1 {
    pub max_planes: u32,
    pub resolution_scale: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReflectionSsrBudgetV1 {
    pub max_steps: u32,
    pub max_rays_per_pixel: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReflectionProbeBudgetV1 {
    pub max_active_probes: u32,
    pub update_ratio: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemporalStackContractV1 {
    pub motion_vectors_enabled: bool,
    pub taa_enabled: bool,
    pub temporal_upscaling_enabled: bool,
    pub temporal_upscaler_mode: TemporalUpscalerMode,
    pub reactive_mask_enabled: bool,
    pub disocclusion_mask_enabled: bool,
    pub dynamic_resolution_policy: DynamicResolutionPolicyV1,
    pub metrics: TemporalMetricsContractV1,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DynamicResolutionPolicyV1 {
    pub enabled: bool,
    pub min_scale: f32,
    pub max_scale: f32,
    pub target_frame_time_ms: f32,
    pub scale_step: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemporalMetricsContractV1 {
    pub window_frames: u32,
    pub report_interval_ms: u32,
    pub max_jitter_ms: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TonemapOperator {
    Aces,
    Reinhard,
    Filmic,
    Neutral,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReflectionFallbackMode {
    Planar,
    Ssr,
    Probe,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ShadowQualityTier {
    Low,
    Medium,
    #[default]
    High,
    Ultra,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TemporalUpscalerMode {
    Taa,
    Fsr2,
    Dlss,
    Xess,
    Native,
}

impl TonemapOperator {
    pub fn as_str(self) -> &'static str {
        match self {
            TonemapOperator::Aces => "aces",
            TonemapOperator::Reinhard => "reinhard",
            TonemapOperator::Filmic => "filmic",
            TonemapOperator::Neutral => "neutral",
        }
    }

    pub fn parse(raw: &str) -> Result<Self, String> {
        match normalize_token(raw).as_str() {
            "aces" => Ok(Self::Aces),
            "reinhard" => Ok(Self::Reinhard),
            "filmic" => Ok(Self::Filmic),
            "neutral" => Ok(Self::Neutral),
            other => Err(format!("unsupported tonemap operator `{other}`")),
        }
    }
}

impl ReflectionFallbackMode {
    pub fn as_str(self) -> &'static str {
        match self {
            ReflectionFallbackMode::Planar => "planar",
            ReflectionFallbackMode::Ssr => "ssr",
            ReflectionFallbackMode::Probe => "probe",
        }
    }

    pub fn parse(raw: &str) -> Result<Self, String> {
        match normalize_token(raw).as_str() {
            "planar" => Ok(Self::Planar),
            "ssr" => Ok(Self::Ssr),
            "probe" => Ok(Self::Probe),
            other => Err(format!("unsupported reflection fallback mode `{other}`")),
        }
    }
}

impl ShadowQualityTier {
    pub fn as_str(self) -> &'static str {
        match self {
            ShadowQualityTier::Low => "low",
            ShadowQualityTier::Medium => "medium",
            ShadowQualityTier::High => "high",
            ShadowQualityTier::Ultra => "ultra",
        }
    }

    pub fn parse(raw: &str) -> Result<Self, String> {
        match normalize_token(raw).as_str() {
            "0" | "low" => Ok(Self::Low),
            "1" | "med" | "medium" => Ok(Self::Medium),
            "2" | "hi" | "high" => Ok(Self::High),
            "3" | "ultra" => Ok(Self::Ultra),
            other => Err(format!("unsupported shadow quality tier `{other}`")),
        }
    }
}

impl TemporalUpscalerMode {
    pub fn as_str(self) -> &'static str {
        match self {
            TemporalUpscalerMode::Taa => "taa",
            TemporalUpscalerMode::Fsr2 => "fsr2",
            TemporalUpscalerMode::Dlss => "dlss",
            TemporalUpscalerMode::Xess => "xess",
            TemporalUpscalerMode::Native => "native",
        }
    }

    pub fn parse(raw: &str) -> Result<Self, String> {
        match normalize_token(raw).as_str() {
            "taa" | "taa_upscale" | "temporal" => Ok(Self::Taa),
            "fsr2" | "amd_fsr2" => Ok(Self::Fsr2),
            "dlss" | "nvidia_dlss" => Ok(Self::Dlss),
            "xess" | "intel_xess" => Ok(Self::Xess),
            "native" | "none" => Ok(Self::Native),
            other => Err(format!("unsupported temporal upscaler mode `{other}`")),
        }
    }
}

fn normalize_token(raw: &str) -> String {
    let mut normalized = raw.trim().to_ascii_lowercase().replace(['-', ' '], "_");
    while normalized.contains("__") {
        normalized = normalized.replace("__", "_");
    }
    normalized
}

macro_rules! impl_string_enum_serde {
    ($name:ty) => {
        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                serializer.serialize_str((*self).as_str())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                let raw = String::deserialize(deserializer)?;
                <$name>::parse(raw.as_str()).map_err(de::Error::custom)
            }
        }
    };
}

impl_string_enum_serde!(TonemapOperator);
impl_string_enum_serde!(ReflectionFallbackMode);
impl_string_enum_serde!(ShadowQualityTier);
impl_string_enum_serde!(TemporalUpscalerMode);

impl fmt::Display for TonemapOperator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Display for ReflectionFallbackMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Display for ShadowQualityTier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Display for TemporalUpscalerMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ClusteredLightingContractV1, DefaultProfileContractsV1, DynamicResolutionPolicyV1,
        LightingContractV1, ReflectionFallbackContractV1, ReflectionFallbackMode,
        ReflectionPlanarBudgetV1, ReflectionProbeBudgetV1, ReflectionSsrBudgetV1, ShadowContractV1,
        ShadowQualityTier, TemporalMetricsContractV1, TemporalStackContractV1,
        TemporalUpscalerMode, TonemapOperator,
    };
    use serde_json::json;

    #[test]
    fn enums_deserialize_legacy_aliases_and_case() {
        assert_eq!(
            serde_json::from_value::<TonemapOperator>(json!("ACES")).expect("parse tonemap"),
            TonemapOperator::Aces
        );
        assert_eq!(
            serde_json::from_value::<ReflectionFallbackMode>(json!("SSR"))
                .expect("parse fallback mode"),
            ReflectionFallbackMode::Ssr
        );
        assert_eq!(
            serde_json::from_value::<ShadowQualityTier>(json!("Hi")).expect("parse quality tier"),
            ShadowQualityTier::High
        );
        assert_eq!(
            serde_json::from_value::<TemporalUpscalerMode>(json!("nvidia-dlss"))
                .expect("parse upscaler"),
            TemporalUpscalerMode::Dlss
        );
        assert_eq!(
            serde_json::from_value::<TemporalUpscalerMode>(json!("temporal"))
                .expect("parse temporal alias"),
            TemporalUpscalerMode::Taa
        );
    }

    #[test]
    fn enums_reject_unknown_values() {
        let err = serde_json::from_value::<TonemapOperator>(json!("bogus"))
            .expect_err("unknown tonemap must fail");
        assert!(
            err.to_string().contains("unsupported tonemap operator"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn enums_serialize_to_canonical_tokens() {
        let value = serde_json::to_value(TemporalUpscalerMode::Fsr2).expect("serialize enum");
        assert_eq!(value, json!("fsr2"));
    }

    #[test]
    fn default_profile_contract_deserializes_and_serializes_with_canonical_enums() {
        let payload = json!({
            "schema_version": 1,
            "profile": "default",
            "lighting": {
                "pbr_enabled": true,
                "hdr_enabled": true,
                "tonemap_operator": "ACES",
                "clustered_lighting": {
                    "enabled": true,
                    "max_lights_per_cluster": 64,
                    "shadow": {
                        "enabled": true,
                        "cascade_count": 4,
                        "atlas_resolution": 2048,
                        "quality_tier": "Hi"
                    }
                }
            },
            "reflections": {
                "fallback_chain": ["Planar", "SSR", "Probe"],
                "planar_budget": {"max_planes": 2, "resolution_scale": 1.0},
                "ssr_budget": {"max_steps": 32, "max_rays_per_pixel": 1},
                "probe_budget": {"max_active_probes": 16, "update_ratio": 0.25}
            },
            "temporal": {
                "motion_vectors_enabled": true,
                "taa_enabled": true,
                "temporal_upscaling_enabled": true,
                "temporal_upscaler_mode": "temporal",
                "reactive_mask_enabled": true,
                "disocclusion_mask_enabled": true,
                "dynamic_resolution_policy": {
                    "enabled": true,
                    "min_scale": 0.6,
                    "max_scale": 1.0,
                    "target_frame_time_ms": 16.7,
                    "scale_step": 0.05
                },
                "metrics": {
                    "window_frames": 120,
                    "report_interval_ms": 1000,
                    "max_jitter_ms": 0.75
                }
            }
        });

        let contracts: DefaultProfileContractsV1 =
            serde_json::from_value(payload).expect("contract deserialization");
        assert_eq!(contracts.lighting.tonemap_operator, TonemapOperator::Aces);
        assert_eq!(
            contracts.lighting.clustered_lighting.shadow.quality_tier,
            ShadowQualityTier::High
        );
        assert_eq!(
            contracts.reflections.fallback_chain,
            vec![
                ReflectionFallbackMode::Planar,
                ReflectionFallbackMode::Ssr,
                ReflectionFallbackMode::Probe
            ]
        );
        assert_eq!(
            contracts.temporal.temporal_upscaler_mode,
            TemporalUpscalerMode::Taa
        );

        let serialized = serde_json::to_value(contracts).expect("contract serialization");
        assert_eq!(serialized["lighting"]["tonemap_operator"], json!("aces"));
        assert_eq!(
            serialized["lighting"]["clustered_lighting"]["shadow"]["quality_tier"],
            json!("high")
        );
        assert_eq!(
            serialized["temporal"]["temporal_upscaler_mode"],
            json!("taa")
        );
    }

    #[test]
    fn shadow_quality_defaults_to_high_when_omitted() {
        let contracts = DefaultProfileContractsV1 {
            schema_version: 1,
            profile: "default".to_string(),
            lighting: LightingContractV1 {
                pbr_enabled: true,
                hdr_enabled: true,
                tonemap_operator: TonemapOperator::Aces,
                clustered_lighting: ClusteredLightingContractV1 {
                    enabled: true,
                    max_lights_per_cluster: 64,
                    shadow: ShadowContractV1 {
                        enabled: true,
                        cascade_count: 4,
                        atlas_resolution: 2048,
                        quality_tier: ShadowQualityTier::default(),
                    },
                },
            },
            reflections: ReflectionFallbackContractV1 {
                fallback_chain: vec![
                    ReflectionFallbackMode::Planar,
                    ReflectionFallbackMode::Ssr,
                    ReflectionFallbackMode::Probe,
                ],
                planar_budget: ReflectionPlanarBudgetV1 {
                    max_planes: 2,
                    resolution_scale: 1.0,
                },
                ssr_budget: ReflectionSsrBudgetV1 {
                    max_steps: 32,
                    max_rays_per_pixel: 1,
                },
                probe_budget: ReflectionProbeBudgetV1 {
                    max_active_probes: 16,
                    update_ratio: 0.25,
                },
            },
            temporal: TemporalStackContractV1 {
                motion_vectors_enabled: true,
                taa_enabled: true,
                temporal_upscaling_enabled: true,
                temporal_upscaler_mode: TemporalUpscalerMode::Taa,
                reactive_mask_enabled: true,
                disocclusion_mask_enabled: true,
                dynamic_resolution_policy: DynamicResolutionPolicyV1 {
                    enabled: true,
                    min_scale: 0.6,
                    max_scale: 1.0,
                    target_frame_time_ms: 16.7,
                    scale_step: 0.05,
                },
                metrics: TemporalMetricsContractV1 {
                    window_frames: 120,
                    report_interval_ms: 1000,
                    max_jitter_ms: 0.75,
                },
            },
        };

        let mut value = serde_json::to_value(contracts).expect("serialize base contract");
        value["lighting"]["clustered_lighting"]["shadow"]
            .as_object_mut()
            .expect("shadow object")
            .remove("quality_tier");
        let reparsed: DefaultProfileContractsV1 =
            serde_json::from_value(value).expect("deserialize without quality_tier");
        assert_eq!(
            reparsed.lighting.clustered_lighting.shadow.quality_tier,
            ShadowQualityTier::High
        );
    }
}
