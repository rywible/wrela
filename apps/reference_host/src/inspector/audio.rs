use wrela::engine_frame::EngineSubsystemReport;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioPanel {
    pub active_voices: u64,
    pub label: String,
}

impl AudioPanel {
    pub fn from_report(report: &EngineSubsystemReport) -> Self {
        Self {
            active_voices: report.work_items,
            label: report.label.clone(),
        }
    }

    /// One-line summary for the inspector top bar / deep-link tooltip
    /// (RFC 0011 L6).
    pub fn deep_link_summary(&self) -> String {
        format!("{} voices={}", self.label, self.active_voices)
    }
}
