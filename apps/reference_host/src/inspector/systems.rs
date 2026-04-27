use wrela::engine_frame::EngineSubsystemReport;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemsPanel {
    pub executed_systems: u64,
    pub label: String,
}

impl SystemsPanel {
    pub fn from_report(report: &EngineSubsystemReport) -> Self {
        Self {
            executed_systems: report.work_items,
            label: report.label.clone(),
        }
    }

    /// One-line summary for the inspector top bar / deep-link tooltip
    /// (RFC 0011 L6).
    pub fn deep_link_summary(&self) -> String {
        format!("{} systems={}", self.label, self.executed_systems)
    }
}
