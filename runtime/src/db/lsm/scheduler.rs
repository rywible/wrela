#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompactionAdmissionDecision {
    Admit,
    Defer,
    Reject,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CompactionSchedulerMetrics {
    pub admitted: u64,
    pub deferred: u64,
    pub rejected: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompactionSchedulerConfig {
    pub max_debt_bytes: u64,
    pub max_in_flight_jobs: usize,
}

impl Default for CompactionSchedulerConfig {
    fn default() -> Self {
        Self {
            max_debt_bytes: 128 * 1024 * 1024,
            max_in_flight_jobs: 2,
        }
    }
}

pub fn decide_compaction_admission(
    debt_bytes: u64,
    in_flight_jobs: usize,
    config: CompactionSchedulerConfig,
) -> CompactionAdmissionDecision {
    if config.max_in_flight_jobs == 0 {
        return CompactionAdmissionDecision::Reject;
    }
    if in_flight_jobs >= config.max_in_flight_jobs {
        return CompactionAdmissionDecision::Defer;
    }
    if debt_bytes > config.max_debt_bytes {
        return CompactionAdmissionDecision::Admit;
    }
    if debt_bytes == 0 {
        return CompactionAdmissionDecision::Reject;
    }
    CompactionAdmissionDecision::Defer
}

pub fn record_scheduler_decision(
    metrics: &mut CompactionSchedulerMetrics,
    decision: CompactionAdmissionDecision,
) {
    match decision {
        CompactionAdmissionDecision::Admit => {
            metrics.admitted = metrics.admitted.saturating_add(1);
        }
        CompactionAdmissionDecision::Defer => {
            metrics.deferred = metrics.deferred.saturating_add(1);
        }
        CompactionAdmissionDecision::Reject => {
            metrics.rejected = metrics.rejected.saturating_add(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admits_when_debt_exceeds_threshold_and_capacity_available() {
        let cfg = CompactionSchedulerConfig {
            max_debt_bytes: 1024,
            max_in_flight_jobs: 2,
        };
        assert_eq!(
            decide_compaction_admission(2048, 1, cfg),
            CompactionAdmissionDecision::Admit
        );
    }

    #[test]
    fn defers_when_worker_slots_are_full() {
        let cfg = CompactionSchedulerConfig {
            max_debt_bytes: 1024,
            max_in_flight_jobs: 1,
        };
        assert_eq!(
            decide_compaction_admission(4096, 1, cfg),
            CompactionAdmissionDecision::Defer
        );
    }

    #[test]
    fn rejects_when_no_debt() {
        assert_eq!(
            decide_compaction_admission(
                0,
                0,
                CompactionSchedulerConfig {
                    max_debt_bytes: 1024,
                    max_in_flight_jobs: 1,
                }
            ),
            CompactionAdmissionDecision::Reject
        );
    }

    #[test]
    fn records_decision_counters() {
        let mut metrics = CompactionSchedulerMetrics::default();
        record_scheduler_decision(&mut metrics, CompactionAdmissionDecision::Admit);
        record_scheduler_decision(&mut metrics, CompactionAdmissionDecision::Defer);
        record_scheduler_decision(&mut metrics, CompactionAdmissionDecision::Reject);
        assert_eq!(metrics.admitted, 1);
        assert_eq!(metrics.deferred, 1);
        assert_eq!(metrics.rejected, 1);
    }
}
