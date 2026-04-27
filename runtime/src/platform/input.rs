//! Raw platform input types with monotonic timestamps (RFC 0011 Phase 64).
//!
//! `source`/`detail`/symbolic codes use [`SmolStr`] so that interning is cheap
//! and the late-input pump can move events without heap allocation per drain.

use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RawInputKind {
    Key {
        code: SmolStr,
        pressed: bool,
    },
    MouseButton {
        button: SmolStr,
        pressed: bool,
    },
    MouseDelta {
        x: i32,
        y: i32,
    },
    GamepadAxis {
        axis: SmolStr,
        /// Value scaled by 1_000_000 to keep `Eq`/`Hash` available without
        /// committing to a specific float interpretation.
        value_micros: i32,
    },
    GamepadButton {
        button: SmolStr,
        pressed: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimestampedRawEvent {
    pub source: SmolStr,
    pub detail: SmolStr,
    pub kind: RawInputKind,
    pub wall_clock_micros: u64,
    pub monotonic_nanos: u64,
}

impl TimestampedRawEvent {
    pub fn new(
        source: impl Into<SmolStr>,
        detail: impl Into<SmolStr>,
        kind: RawInputKind,
        wall_clock_micros: u64,
        monotonic_nanos: u64,
    ) -> Self {
        Self {
            source: source.into(),
            detail: detail.into(),
            kind,
            wall_clock_micros,
            monotonic_nanos,
        }
    }
}
