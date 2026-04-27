//! Click-through inspector state for engine-frame reports.

use wrela::engine_frame::{EngineFrameReport, EngineSubsystemKind};

pub mod audio;
pub mod persistence;
pub mod physics;
pub mod residency;
pub mod systems;
pub mod timeline;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectorRow {
    pub kind: EngineSubsystemKind,
    pub label: String,
    pub work_items: u64,
    pub panel: InspectorPanel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InspectorPanel {
    Timeline(timeline::TimelinePanel),
    Systems(systems::SystemsPanel),
    Residency(residency::ResidencyPanel),
    Physics(physics::PhysicsPanel),
    Audio(audio::AudioPanel),
    Persistence(persistence::PersistencePanel),
}

impl InspectorPanel {
    /// Single-line summary suitable for the inspector top bar and the
    /// deep-link tooltip (RFC 0011 L6). Each panel decides what is most
    /// important to surface (voices for audio, body count for physics, etc.).
    pub fn deep_link_summary(&self) -> String {
        match self {
            InspectorPanel::Timeline(panel) => panel.deep_link_summary(),
            InspectorPanel::Systems(panel) => panel.deep_link_summary(),
            InspectorPanel::Residency(panel) => panel.deep_link_summary(),
            InspectorPanel::Physics(panel) => panel.deep_link_summary(),
            InspectorPanel::Audio(panel) => panel.deep_link_summary(),
            InspectorPanel::Persistence(panel) => panel.deep_link_summary(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InspectorState {
    pub rows: Vec<InspectorRow>,
}

impl InspectorState {
    pub fn from_report(report: &EngineFrameReport) -> Self {
        let rows = report
            .subsystems
            .iter()
            .map(|subsystem| {
                // RFC 0011 M6: match on a borrow so `FutureReserve(String)` does
                // not need to be cloned into the match scrutinee, and so the
                // wildcard arm is statically exhaustive over future variants.
                let panel = match &subsystem.kind {
                    EngineSubsystemKind::System => {
                        InspectorPanel::Systems(systems::SystemsPanel::from_report(subsystem))
                    }
                    EngineSubsystemKind::Residency => {
                        InspectorPanel::Residency(residency::ResidencyPanel::from_report(subsystem))
                    }
                    EngineSubsystemKind::Physics => {
                        InspectorPanel::Physics(physics::PhysicsPanel::from_report(subsystem))
                    }
                    EngineSubsystemKind::Audio => {
                        InspectorPanel::Audio(audio::AudioPanel::from_report(subsystem))
                    }
                    EngineSubsystemKind::Save => InspectorPanel::Persistence(
                        persistence::PersistencePanel::from_report(subsystem),
                    ),
                    EngineSubsystemKind::StateAdvance
                    | EngineSubsystemKind::Input
                    | EngineSubsystemKind::Presentation
                    | EngineSubsystemKind::Collision
                    | EngineSubsystemKind::Query
                    | EngineSubsystemKind::GpuRuntime
                    | EngineSubsystemKind::FutureReserve(_) => {
                        InspectorPanel::Timeline(timeline::TimelinePanel::from_report(subsystem))
                    }
                };
                InspectorRow {
                    kind: subsystem.kind.clone(),
                    label: subsystem.label.clone(),
                    work_items: subsystem.work_items,
                    panel,
                }
            })
            .collect();
        Self { rows }
    }
}
