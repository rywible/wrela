use wrela::engine_frame::EngineSubsystemReport;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistencePanel {
    pub save_records: u64,
    pub label: String,
}

impl PersistencePanel {
    pub fn from_report(report: &EngineSubsystemReport) -> Self {
        Self {
            save_records: report.work_items,
            label: report.label.clone(),
        }
    }

    /// One-line summary for the inspector top bar / deep-link tooltip
    /// (RFC 0011 L6).
    pub fn deep_link_summary(&self) -> String {
        format!("{} records={}", self.label, self.save_records)
    }
}
