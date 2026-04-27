use wrela::engine_frame::EngineSubsystemReport;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelinePanel {
    pub label: String,
    pub cpu_micros: u128,
    pub gpu_micros: Option<u128>,
    pub queue_submits: u32,
}

impl TimelinePanel {
    pub fn from_report(report: &EngineSubsystemReport) -> Self {
        Self {
            label: report.label.clone(),
            cpu_micros: report.cpu_critical_path_micros,
            gpu_micros: report.gpu_critical_path_micros,
            queue_submits: report.queue_submit_count,
        }
    }

    /// One-line summary for the inspector top bar / deep-link tooltip
    /// (RFC 0011 L6).
    pub fn deep_link_summary(&self) -> String {
        match self.gpu_micros {
            Some(gpu) => format!(
                "{} cpu={}us gpu={}us submits={}",
                self.label, self.cpu_micros, gpu, self.queue_submits
            ),
            None => format!(
                "{} cpu={}us submits={}",
                self.label, self.cpu_micros, self.queue_submits
            ),
        }
    }
}
