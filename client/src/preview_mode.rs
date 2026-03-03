#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use crate::camera_math::{
    Mat4, OrbitCamera, mat4_mul, mat4_rotation_y, mat4_scale, mat4_translation,
};
use serde::Deserialize;

// ---------------------------------------------------------------------------
// Scene IR types (matching compiler's scene_ir/mod.rs output)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct SceneIR {
    pub scene: String,
    pub entities: Vec<SceneEntity>,
    pub lighting: SceneLighting,
    pub camera: SceneCamera,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SceneEntity {
    pub name: String,
    pub mesh_asset: String,
    pub mesh_path: Option<String>,
    pub position: [i64; 3],
    pub rotation: [i64; 3],
    pub scale: [i64; 3],
}

#[derive(Debug, Clone, Deserialize)]
pub struct SceneLighting {
    pub sun_direction: [i64; 3],
    pub sun_color: [u8; 3],
    pub sun_intensity: i64,
    pub ambient_color: [u8; 3],
    pub ambient_intensity: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SceneCamera {
    pub mode: String,
    pub target: String,
    pub distance: i64,
    pub pitch: i64,
}

// ---------------------------------------------------------------------------
// WebSocket message types from preview server
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum PreviewMessage {
    #[serde(rename = "scene_update")]
    SceneUpdate { scene: SceneIR },
    #[serde(rename = "status")]
    Status { message: String },
}

// ---------------------------------------------------------------------------
// Preview state
// ---------------------------------------------------------------------------

pub struct PreviewState {
    pub camera: OrbitCamera,
    pub scene: Option<SceneIR>,
    pub entities: Vec<PreviewEntity>,
    pub status_message: Option<String>,
    pub mouse_dragging: bool,
    pub last_mouse_x: f32,
    pub last_mouse_y: f32,
    pub needs_mesh_load: Vec<MeshLoadRequest>,
    pub grid_mesh_index: Option<usize>,
}

pub struct PreviewEntity {
    pub name: String,
    /// Index into the renderer's mesh list. `None` while loading.
    pub mesh_index: Option<usize>,
    pub model_matrix: Mat4,
    pub loading: bool,
}

pub struct MeshLoadRequest {
    pub entity_name: String,
    pub url: String,
}

impl PreviewState {
    pub fn new() -> Self {
        Self {
            camera: {
                let mut cam = OrbitCamera::default();
                cam.azimuth = 0.5;
                cam.elevation = -0.3;
                cam.distance = 5.0;
                cam.target = [0.0, 1.0, 0.0];
                cam.fov_y = std::f32::consts::FRAC_PI_4;
                cam
            },
            scene: None,
            entities: Vec::new(),
            status_message: None,
            mouse_dragging: false,
            last_mouse_x: 0.0,
            last_mouse_y: 0.0,
            needs_mesh_load: Vec::new(),
            grid_mesh_index: None,
        }
    }

    /// Apply a new scene IR update, reconciling existing entities and queueing
    /// mesh load requests for any entities that don't yet have GPU meshes.
    pub fn apply_scene_update(&mut self, scene: SceneIR) {
        // Update camera from scene IR
        self.camera.distance = scene.camera.distance as f32 / 1000.0;
        self.camera.elevation = scene.camera.pitch as f32 / 1000.0;

        // Build entity list, preserving loaded mesh indices where possible
        let mut new_entities = Vec::new();
        for entity in &scene.entities {
            let existing = self.entities.iter().find(|e| e.name == entity.name);
            let mesh_index = existing.and_then(|e| e.mesh_index);

            // Compute model matrix from position/rotation/scale (milli-scaled)
            let px = entity.position[0] as f32 / 1000.0;
            let py = entity.position[1] as f32 / 1000.0;
            let pz = entity.position[2] as f32 / 1000.0;
            let sx = entity.scale[0] as f32 / 1000.0;
            let sy = entity.scale[1] as f32 / 1000.0;
            let sz = entity.scale[2] as f32 / 1000.0;
            let ry = entity.rotation[1] as f32 / 1000.0;

            let t = mat4_translation(px, py, pz);
            let r = mat4_rotation_y(ry);
            let s = mat4_scale(sx, sy, sz);
            let model_matrix = mat4_mul(t, mat4_mul(r, s));

            let loading = mesh_index.is_none();

            // Queue mesh load if we have a path and no mesh yet
            if mesh_index.is_none() {
                if let Some(path) = &entity.mesh_path {
                    self.needs_mesh_load.push(MeshLoadRequest {
                        entity_name: entity.name.clone(),
                        url: format!("/assets/{}", path),
                    });
                }
            }

            new_entities.push(PreviewEntity {
                name: entity.name.clone(),
                mesh_index,
                model_matrix,
                loading,
            });
        }
        self.entities = new_entities;
        self.scene = Some(scene);
    }

    /// Mark an entity's mesh as loaded, assigning the GPU mesh index.
    pub fn set_entity_mesh(&mut self, entity_name: &str, mesh_index: usize) {
        if let Some(entity) = self.entities.iter_mut().find(|e| e.name == entity_name) {
            entity.mesh_index = Some(mesh_index);
            entity.loading = false;
        }
    }

    pub fn on_mouse_down(&mut self, x: f32, y: f32) {
        self.mouse_dragging = true;
        self.last_mouse_x = x;
        self.last_mouse_y = y;
    }

    pub fn on_mouse_up(&mut self) {
        self.mouse_dragging = false;
    }

    pub fn on_mouse_move(&mut self, x: f32, y: f32) {
        if self.mouse_dragging {
            let dx = (x - self.last_mouse_x) * 0.005;
            let dy = (y - self.last_mouse_y) * 0.005;
            self.camera.rotate(dx, dy);
            self.last_mouse_x = x;
            self.last_mouse_y = y;
        }
    }

    pub fn on_wheel(&mut self, delta: f32) {
        self.camera.zoom(delta * 0.001);
    }

    /// Parse and handle a WebSocket JSON text message.
    pub fn handle_message(&mut self, data: &str) {
        match serde_json::from_str::<PreviewMessage>(data) {
            Ok(PreviewMessage::SceneUpdate { scene }) => {
                self.apply_scene_update(scene);
            }
            Ok(PreviewMessage::Status { message }) => {
                self.status_message = Some(message);
            }
            Err(_e) => {
                // Logged via console in wasm context; in tests we just ignore
                #[cfg(target_arch = "wasm32")]
                web_sys::console::warn_1(&format!("Preview: invalid message: {_e}").into());
            }
        }
    }

    /// Get the current sun direction from the scene IR, or a sensible default.
    pub fn sun_direction(&self) -> [f32; 3] {
        if let Some(ref scene) = self.scene {
            let d = &scene.lighting.sun_direction;
            [
                d[0] as f32 / 1000.0,
                d[1] as f32 / 1000.0,
                d[2] as f32 / 1000.0,
            ]
        } else {
            [-0.432, -0.864, -0.259]
        }
    }

    /// Get the current sun color from the scene IR, or a sensible default.
    pub fn sun_color(&self) -> [f32; 3] {
        if let Some(ref scene) = self.scene {
            let c = &scene.lighting.sun_color;
            let intensity = scene.lighting.sun_intensity as f32 / 1000.0;
            [
                c[0] as f32 / 255.0 * intensity,
                c[1] as f32 / 255.0 * intensity,
                c[2] as f32 / 255.0 * intensity,
            ]
        } else {
            [2.0, 1.9, 1.7]
        }
    }

    /// Get the current ambient color from the scene IR, or a sensible default.
    pub fn ambient_color(&self) -> [f32; 3] {
        if let Some(ref scene) = self.scene {
            let c = &scene.lighting.ambient_color;
            let intensity = scene.lighting.ambient_intensity as f32 / 1000.0;
            [
                c[0] as f32 / 255.0 * intensity,
                c[1] as f32 / 255.0 * intensity,
                c[2] as f32 / 255.0 * intensity,
            ]
        } else {
            [0.35, 0.38, 0.42]
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_scene() -> SceneIR {
        SceneIR {
            scene: "test_scene".to_string(),
            entities: vec![
                SceneEntity {
                    name: "tree_01".to_string(),
                    mesh_asset: "tree".to_string(),
                    mesh_path: Some("meshes/tree.glb".to_string()),
                    position: [1000, 0, 2000],
                    rotation: [0, 500, 0],
                    scale: [1000, 1000, 1000],
                },
                SceneEntity {
                    name: "rock_01".to_string(),
                    mesh_asset: "rock".to_string(),
                    mesh_path: Some("meshes/rock.glb".to_string()),
                    position: [-3000, 0, 1000],
                    rotation: [0, 0, 0],
                    scale: [2000, 2000, 2000],
                },
            ],
            lighting: SceneLighting {
                sun_direction: [-432, -864, -259],
                sun_color: [255, 240, 220],
                sun_intensity: 2000,
                ambient_color: [89, 97, 107],
                ambient_intensity: 1000,
            },
            camera: SceneCamera {
                mode: "orbit".to_string(),
                target: "tree_01".to_string(),
                distance: 5000,
                pitch: -300,
            },
        }
    }

    #[test]
    fn test_preview_state_apply_scene_update() {
        let mut state = PreviewState::new();
        let scene = make_test_scene();

        state.apply_scene_update(scene);

        assert!(state.scene.is_some());
        assert_eq!(state.entities.len(), 2);
        assert_eq!(state.entities[0].name, "tree_01");
        assert_eq!(state.entities[1].name, "rock_01");

        // Both entities should be loading (no mesh loaded yet)
        assert!(state.entities[0].loading);
        assert!(state.entities[1].loading);
        assert!(state.entities[0].mesh_index.is_none());

        // Mesh load requests should be queued
        assert_eq!(state.needs_mesh_load.len(), 2);
        assert_eq!(state.needs_mesh_load[0].entity_name, "tree_01");
        assert_eq!(state.needs_mesh_load[0].url, "/assets/meshes/tree.glb");
        assert_eq!(state.needs_mesh_load[1].entity_name, "rock_01");
        assert_eq!(state.needs_mesh_load[1].url, "/assets/meshes/rock.glb");
    }

    #[test]
    fn test_preview_state_camera_update() {
        let mut state = PreviewState::new();
        let scene = make_test_scene();

        state.apply_scene_update(scene);

        // Camera distance: 5000 / 1000.0 = 5.0
        assert!((state.camera.distance - 5.0).abs() < 0.01);
        // Camera pitch: -300 / 1000.0 = -0.3
        assert!((state.camera.elevation - (-0.3)).abs() < 0.01);
    }

    #[test]
    fn test_preview_state_mouse_drag() {
        let mut state = PreviewState::new();

        let initial_azimuth = state.camera.azimuth;
        let initial_elevation = state.camera.elevation;

        // Start drag
        state.on_mouse_down(100.0, 100.0);
        assert!(state.mouse_dragging);

        // Move mouse 200 pixels right, 100 down
        state.on_mouse_move(300.0, 200.0);

        // Camera should have rotated
        let expected_daz = (300.0 - 100.0) * 0.005;
        let expected_del = (200.0 - 100.0) * 0.005;
        assert!((state.camera.azimuth - (initial_azimuth + expected_daz)).abs() < 0.001);
        assert!((state.camera.elevation - (initial_elevation + expected_del)).abs() < 0.001);

        // End drag
        state.on_mouse_up();
        assert!(!state.mouse_dragging);

        // Moving after mouse up should not affect camera
        let az_after_up = state.camera.azimuth;
        state.on_mouse_move(500.0, 500.0);
        assert!((state.camera.azimuth - az_after_up).abs() < 0.001);
    }

    #[test]
    fn test_preview_state_wheel_zoom() {
        let mut state = PreviewState::new();
        let initial_distance = state.camera.distance;

        state.on_wheel(1000.0);
        assert!((state.camera.distance - (initial_distance + 1.0)).abs() < 0.01);

        state.on_wheel(-2000.0);
        assert!(state.camera.distance < initial_distance);
    }

    #[test]
    fn test_preview_state_message_parsing_scene_update() {
        let mut state = PreviewState::new();

        let json = r#"{
            "type": "scene_update",
            "scene": {
                "scene": "demo",
                "entities": [
                    {
                        "name": "box_01",
                        "mesh_asset": "box",
                        "mesh_path": "meshes/box.glb",
                        "position": [0, 0, 0],
                        "rotation": [0, 0, 0],
                        "scale": [1000, 1000, 1000]
                    }
                ],
                "lighting": {
                    "sun_direction": [-500, -866, -250],
                    "sun_color": [255, 255, 255],
                    "sun_intensity": 1500,
                    "ambient_color": [100, 100, 100],
                    "ambient_intensity": 500
                },
                "camera": {
                    "mode": "orbit",
                    "target": "box_01",
                    "distance": 3000,
                    "pitch": -200
                }
            }
        }"#;

        state.handle_message(json);

        assert!(state.scene.is_some());
        assert_eq!(state.entities.len(), 1);
        assert_eq!(state.entities[0].name, "box_01");
        assert!((state.camera.distance - 3.0).abs() < 0.01);
    }

    #[test]
    fn test_preview_state_message_parsing_status() {
        let mut state = PreviewState::new();

        let json = r#"{"type": "status", "message": "Compiling scene..."}"#;
        state.handle_message(json);

        assert_eq!(state.status_message.as_deref(), Some("Compiling scene..."));
    }

    #[test]
    fn test_preview_state_message_parsing_invalid() {
        let mut state = PreviewState::new();

        state.handle_message("not valid json");
        // Should not panic, scene should remain None
        assert!(state.scene.is_none());
    }

    #[test]
    fn test_preview_state_preserves_loaded_meshes_across_updates() {
        let mut state = PreviewState::new();

        // First update: load scene
        let scene = make_test_scene();
        state.apply_scene_update(scene);
        state.needs_mesh_load.clear();

        // Simulate mesh loaded for tree_01
        state.set_entity_mesh("tree_01", 42);
        assert_eq!(state.entities[0].mesh_index, Some(42));
        assert!(!state.entities[0].loading);

        // Second update: same entities
        let scene2 = make_test_scene();
        state.apply_scene_update(scene2);

        // tree_01 should still have its mesh
        assert_eq!(state.entities[0].mesh_index, Some(42));
        assert!(!state.entities[0].loading);

        // rock_01 should still be loading
        assert!(state.entities[1].mesh_index.is_none());
        assert!(state.entities[1].loading);
    }

    #[test]
    fn test_preview_state_model_matrix_computation() {
        let mut state = PreviewState::new();

        let scene = SceneIR {
            scene: "test".to_string(),
            entities: vec![SceneEntity {
                name: "obj".to_string(),
                mesh_asset: "cube".to_string(),
                mesh_path: None,
                position: [2000, 1000, 3000],
                rotation: [0, 0, 0],
                scale: [1000, 1000, 1000],
            }],
            lighting: SceneLighting {
                sun_direction: [0, -1000, 0],
                sun_color: [255, 255, 255],
                sun_intensity: 1000,
                ambient_color: [50, 50, 50],
                ambient_intensity: 500,
            },
            camera: SceneCamera {
                mode: "orbit".to_string(),
                target: "obj".to_string(),
                distance: 5000,
                pitch: -300,
            },
        };

        state.apply_scene_update(scene);

        // With scale 1.0 and no rotation, the model matrix should be a pure translation
        let m = state.entities[0].model_matrix;
        // Column-major: translation in column 3
        assert!((m[3][0] - 2.0).abs() < 0.001);
        assert!((m[3][1] - 1.0).abs() < 0.001);
        assert!((m[3][2] - 3.0).abs() < 0.001);
    }

    #[test]
    fn test_preview_state_lighting_defaults() {
        let state = PreviewState::new();

        let sun_dir = state.sun_direction();
        assert!((sun_dir[0] - (-0.432)).abs() < 0.01);
        assert!((sun_dir[1] - (-0.864)).abs() < 0.01);

        let sun_col = state.sun_color();
        assert!((sun_col[0] - 2.0).abs() < 0.01);

        let ambient = state.ambient_color();
        assert!((ambient[0] - 0.35).abs() < 0.01);
    }

    #[test]
    fn test_preview_state_lighting_from_scene() {
        let mut state = PreviewState::new();
        let scene = make_test_scene();
        state.apply_scene_update(scene);

        let sun_dir = state.sun_direction();
        assert!((sun_dir[0] - (-0.432)).abs() < 0.01);

        let sun_col = state.sun_color();
        // sun_color [255, 240, 220], intensity 2000/1000 = 2.0
        assert!((sun_col[0] - 2.0).abs() < 0.01);
    }

    #[test]
    fn test_preview_state_no_mesh_load_without_path() {
        let mut state = PreviewState::new();

        let scene = SceneIR {
            scene: "test".to_string(),
            entities: vec![SceneEntity {
                name: "pathless".to_string(),
                mesh_asset: "cube".to_string(),
                mesh_path: None,
                position: [0, 0, 0],
                rotation: [0, 0, 0],
                scale: [1000, 1000, 1000],
            }],
            lighting: SceneLighting {
                sun_direction: [0, -1000, 0],
                sun_color: [255, 255, 255],
                sun_intensity: 1000,
                ambient_color: [50, 50, 50],
                ambient_intensity: 500,
            },
            camera: SceneCamera {
                mode: "orbit".to_string(),
                target: "pathless".to_string(),
                distance: 5000,
                pitch: -300,
            },
        };

        state.apply_scene_update(scene);

        // No mesh_path -> no load request
        assert!(state.needs_mesh_load.is_empty());
    }
}
