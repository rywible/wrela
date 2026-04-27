use wrela::engine_frame::EngineSubsystemReport;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhysicsPanel {
    pub body_work_items: u64,
    pub label: String,
}

impl PhysicsPanel {
    pub fn from_report(report: &EngineSubsystemReport) -> Self {
        Self {
            body_work_items: report.work_items,
            label: report.label.clone(),
        }
    }

    /// One-line summary for the inspector top bar / deep-link tooltip
    /// (RFC 0011 L6).
    pub fn deep_link_summary(&self) -> String {
        format!("{} bodies={}", self.label, self.body_work_items)
    }
}
