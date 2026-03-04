use super::DebugViewMode;

/// WASM debug view system — manages GPU pipelines for debug overlays.
pub struct DebugViewSystem {
    mode: DebugViewMode,
}

impl DebugViewSystem {
    pub fn new() -> Self {
        Self {
            mode: DebugViewMode::Off,
        }
    }

    pub fn set_mode(&mut self, mode: DebugViewMode) {
        self.mode = mode;
    }

    pub fn mode(&self) -> DebugViewMode {
        self.mode
    }

    pub fn is_active(&self) -> bool {
        self.mode != DebugViewMode::Off
    }
}
