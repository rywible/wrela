use crate::hir::def::{AssetDeclKind, HirAssetDecl, HirSceneDecl};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SceneIR {
    pub scene: String,
    pub entities: Vec<SceneEntity>,
    pub lighting: SceneLighting,
    pub camera: SceneCamera,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SceneEntity {
    pub name: String,
    pub mesh_asset: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mesh_path: Option<String>,
    pub position: [i64; 3],
    pub rotation: [i64; 3],
    pub scale: [i64; 3],
    pub asset_ref: AssetRef,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AssetRef {
    pub name: String,
    pub kind: String,
    pub prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub negative: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lod_budget: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SceneLighting {
    pub sun_direction: [i64; 3],
    pub sun_color: [u8; 3],
    pub sun_intensity: i64,
    pub ambient_color: [u8; 3],
    pub ambient_intensity: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SceneCamera {
    pub mode: String,
    pub target: String,
    pub distance: i64,
    pub pitch: i64,
}

impl SceneIR {
    pub fn from_hir(scene: &HirSceneDecl, assets: &[HirAssetDecl]) -> Self {
        let entities = scene
            .entities
            .iter()
            .map(|entity| {
                let asset_opt = assets.iter().find(|a| a.name == entity.mesh_asset);
                let asset_ref = match asset_opt {
                    Some(asset) => AssetRef {
                        name: asset.name.to_string(),
                        kind: match asset.kind {
                            AssetDeclKind::Mesh => "mesh".to_string(),
                            AssetDeclKind::Texture => "texture".to_string(),
                            AssetDeclKind::Audio => "audio".to_string(),
                        },
                        prompt: asset.prompt.to_string(),
                        style: asset.style.as_ref().map(|s| s.to_string()),
                        negative: asset.negative.as_ref().map(|s| s.to_string()),
                        lod_budget: asset.lod_budget,
                    },
                    None => AssetRef {
                        name: entity.mesh_asset.to_string(),
                        kind: "mesh".to_string(),
                        prompt: String::new(),
                        style: None,
                        negative: None,
                        lod_budget: None,
                    },
                };
                SceneEntity {
                    name: entity.name.to_string(),
                    mesh_asset: entity.mesh_asset.to_string(),
                    mesh_path: None,
                    position: entity.position,
                    rotation: entity.rotation,
                    scale: entity.scale,
                    asset_ref,
                }
            })
            .collect();

        let lighting = match &scene.lighting {
            Some(l) => SceneLighting {
                sun_direction: l.sun_direction,
                sun_color: l.sun_color,
                sun_intensity: l.sun_intensity,
                ambient_color: l.ambient_color,
                ambient_intensity: l.ambient_intensity,
            },
            None => SceneLighting {
                sun_direction: [0, -1000, 0],
                sun_color: [255, 255, 255],
                sun_intensity: 1000,
                ambient_color: [50, 50, 50],
                ambient_intensity: 500,
            },
        };

        let camera = match &scene.camera {
            Some(c) => SceneCamera {
                mode: c.mode.to_string(),
                target: c.target.to_string(),
                distance: c.distance,
                pitch: c.pitch,
            },
            None => SceneCamera {
                mode: "orbit".to_string(),
                target: String::new(),
                distance: 5000,
                pitch: 0,
            },
        };

        SceneIR {
            scene: scene.name.to_string(),
            entities,
            lighting,
            camera,
        }
    }

    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|e| e.to_string())
    }

    pub fn from_json(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hir::def::{
        AssetDeclKind, HirAssetDecl, HirSceneCamera, HirSceneDecl, HirSceneEntity, HirSceneLighting,
    };
    use rowan::TextRange;
    use smol_str::SmolStr;

    fn make_test_asset() -> HirAssetDecl {
        HirAssetDecl {
            name: SmolStr::new("keyblade_crystal"),
            kind: AssetDeclKind::Mesh,
            prompt: SmolStr::new("crystalline keyblade"),
            style: Some(SmolStr::new("anime_fantasy")),
            negative: Some(SmolStr::new("blurry")),
            lod_budget: Some(10000),
            span: TextRange::new(0.into(), 10.into()),
        }
    }

    fn make_test_scene() -> HirSceneDecl {
        HirSceneDecl {
            name: SmolStr::new("preview_arena"),
            entities: vec![HirSceneEntity {
                name: SmolStr::new("player_weapon"),
                mesh_asset: SmolStr::new("keyblade_crystal"),
                position: [0, 0, 0],
                rotation: [0, 1000, 0],
                scale: [1000, 1000, 1000],
            }],
            lighting: Some(HirSceneLighting {
                sun_direction: [500, -800, 300],
                sun_color: [255, 240, 220],
                sun_intensity: 2000,
                ambient_color: [40, 35, 50],
                ambient_intensity: 500,
            }),
            camera: Some(HirSceneCamera {
                mode: SmolStr::new("orbit"),
                target: SmolStr::new("player_weapon"),
                distance: 5000,
                pitch: -300,
            }),
            span: TextRange::new(0.into(), 100.into()),
        }
    }

    #[test]
    fn test_scene_ir_roundtrip() {
        let scene = make_test_scene();
        let assets = vec![make_test_asset()];
        let ir = SceneIR::from_hir(&scene, &assets);
        let json = ir.to_json().unwrap();
        let roundtripped = SceneIR::from_json(&json).unwrap();
        assert_eq!(ir, roundtripped);
    }

    #[test]
    fn test_scene_ir_from_hir() {
        let scene = make_test_scene();
        let assets = vec![make_test_asset()];
        let ir = SceneIR::from_hir(&scene, &assets);

        assert_eq!(ir.scene, "preview_arena");
        assert_eq!(ir.entities.len(), 1);
        assert_eq!(ir.entities[0].name, "player_weapon");
        assert_eq!(ir.entities[0].mesh_asset, "keyblade_crystal");
        assert_eq!(ir.entities[0].position, [0, 0, 0]);
        assert_eq!(ir.entities[0].rotation, [0, 1000, 0]);
        assert_eq!(ir.entities[0].scale, [1000, 1000, 1000]);
        assert_eq!(ir.entities[0].asset_ref.name, "keyblade_crystal");
        assert_eq!(ir.entities[0].asset_ref.kind, "mesh");
        assert_eq!(ir.entities[0].asset_ref.prompt, "crystalline keyblade");
        assert_eq!(
            ir.entities[0].asset_ref.style,
            Some("anime_fantasy".to_string())
        );
        assert_eq!(
            ir.entities[0].asset_ref.negative,
            Some("blurry".to_string())
        );
        assert_eq!(ir.entities[0].asset_ref.lod_budget, Some(10000));
        assert!(ir.entities[0].mesh_path.is_none());

        assert_eq!(ir.lighting.sun_direction, [500, -800, 300]);
        assert_eq!(ir.lighting.sun_color, [255, 240, 220]);
        assert_eq!(ir.lighting.sun_intensity, 2000);
        assert_eq!(ir.lighting.ambient_color, [40, 35, 50]);
        assert_eq!(ir.lighting.ambient_intensity, 500);

        assert_eq!(ir.camera.mode, "orbit");
        assert_eq!(ir.camera.target, "player_weapon");
        assert_eq!(ir.camera.distance, 5000);
        assert_eq!(ir.camera.pitch, -300);
    }

    #[test]
    fn test_scene_ir_unresolved() {
        let scene = HirSceneDecl {
            name: SmolStr::new("test"),
            entities: vec![HirSceneEntity {
                name: SmolStr::new("item"),
                mesh_asset: SmolStr::new("nonexistent"),
                position: [0, 0, 0],
                rotation: [0, 0, 0],
                scale: [1000, 1000, 1000],
            }],
            lighting: None,
            camera: None,
            span: TextRange::new(0.into(), 10.into()),
        };
        let ir = SceneIR::from_hir(&scene, &[]);
        assert!(ir.entities[0].mesh_path.is_none());
        assert_eq!(ir.entities[0].asset_ref.name, "nonexistent");
        assert_eq!(ir.entities[0].asset_ref.prompt, "");
    }
}
