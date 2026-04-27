//! Raw winit window description helpers (RFC 0011 Phase 64).

use winit::dpi::PhysicalSize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowConfig {
    pub title: String,
    pub width: u32,
    pub height: u32,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            title: "Wrela Reference Host".to_string(),
            width: 1280,
            height: 720,
        }
    }
}

impl WindowConfig {
    pub fn physical_size(&self) -> PhysicalSize<u32> {
        PhysicalSize::new(self.width, self.height)
    }
}
