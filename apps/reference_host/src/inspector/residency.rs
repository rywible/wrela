use wrela::engine_frame::EngineSubsystemReport;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResidencyPanel {
    pub resident_work_items: u64,
    pub upload_bytes: u64,
    pub label: String,
}

impl ResidencyPanel {
    pub fn from_report(report: &EngineSubsystemReport) -> Self {
        Self {
            resident_work_items: report.work_items,
            upload_bytes: report.scene_reupload_bytes,
            label: report.label.clone(),
        }
    }

    /// One-line summary for the inspector top bar / deep-link tooltip
    /// (RFC 0011 L6).
    pub fn deep_link_summary(&self) -> String {
        format!(
            "{} resident={} upload={}B",
            self.label, self.resident_work_items, self.upload_bytes
        )
    }
}
