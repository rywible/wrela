//! winit / wgpu / input pump glue without compiler types (RFC 0011 Phase 64).
//!
//! Implementations stay free of `wrela::` imports so `just lint-layering` passes.

#![forbid(unsafe_code)]

pub mod frame_pacing;
pub mod input;
pub mod input_pump;
pub mod surface;
pub mod window;

pub trait PlatformBackend {
    fn backend_name(&self) -> &'static str;
}
