#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

/// Debug view modes for visualizing field engine internals.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum DebugViewMode {
    /// No debug overlay.
    Off,
    /// Step count heatmap: blue=few, red=many.
    StepCount,
    /// Bound magnitude heatmap: local L and anisotropic bound.
    BoundMagnitude,
    /// Error epsilon visualization.
    ErrorEpsilon,
    /// Brick residency: populated bricks, clipmap level, dirty status.
    BrickResidency,
    /// Field channel overlays: any scalar field as false-color.
    FieldChannel,
    /// Surface ID: which field graph node generated nearest surface.
    SurfaceId,
    /// Normal stability: derivative-to-bound ratio.
    NormalStability,
    /// Curvature overlay: blue=convex, red=concave.
    Curvature,
    /// Envelope slack: green=envelope tighter, red=fallback tighter.
    EnvelopeSlack,
    /// Uncertainty interval width.
    UncertaintyWidth,
    /// B provenance overlay: color-coded per-brick provenance.
    BProvenance,
    /// FBM octave utilization heatmap.
    FbmOctaveUtilization,
    /// Budget marching traversal count heatmap.
    BudgetMarchTraversal,
}

impl DebugViewMode {
    /// All available debug view modes (excluding Off).
    pub const ALL: &[DebugViewMode] = &[
        DebugViewMode::StepCount,
        DebugViewMode::BoundMagnitude,
        DebugViewMode::ErrorEpsilon,
        DebugViewMode::BrickResidency,
        DebugViewMode::FieldChannel,
        DebugViewMode::SurfaceId,
        DebugViewMode::NormalStability,
        DebugViewMode::Curvature,
        DebugViewMode::EnvelopeSlack,
        DebugViewMode::UncertaintyWidth,
        DebugViewMode::BProvenance,
        DebugViewMode::FbmOctaveUtilization,
        DebugViewMode::BudgetMarchTraversal,
    ];
}

// ── Non-WASM stub ─────────────────────────────────────────────────────
#[cfg(not(target_arch = "wasm32"))]
pub struct DebugViewSystem {
    mode: DebugViewMode,
}

#[cfg(not(target_arch = "wasm32"))]
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

// ── WASM implementation ───────────────────────────────────────────────
#[cfg(target_arch = "wasm32")]
mod wasm_impl;

#[cfg(target_arch = "wasm32")]
pub use wasm_impl::DebugViewSystem;

pub mod shaders;
#[cfg(target_arch = "wasm32")]
pub mod pipeline;
