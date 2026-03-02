pub mod animation_graph;
pub mod audio;
pub mod blade_trail;
pub mod camera_math;
pub mod entity;
pub mod game_logic;
#[cfg(target_arch = "wasm32")]
mod hud;
pub mod input;
mod key_input_wiring;
mod manifest_validation;
pub mod material;
pub mod mesh;
pub mod particles;
pub mod postprocess;
pub mod procedural_animation;
pub mod procedural_meshes;
pub mod procedural_textures;
pub mod sky;
pub mod protocol;
pub mod protocol_metadata;
mod render_manifest;
mod restart_key_policy;
mod restart_latch;
pub mod shadows;
pub mod skeletal_animation;
pub mod ssao;
pub mod state;
pub mod vfx;
#[cfg(target_arch = "wasm32")]
pub mod wasm_bridge;

#[cfg(target_arch = "wasm32")]
mod web;

#[cfg(not(target_arch = "wasm32"))]
#[path = "web_host.rs"]
mod web;

pub use web::start_client;
