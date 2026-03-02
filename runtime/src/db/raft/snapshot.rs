#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogTruncationPlan {
    Noop,
    TruncatePrefixTo {
        new_first_log_index: u64,
    },
    InvalidSnapshotIndex {
        snapshot_last_included_index: u64,
        committed_index: u64,
    },
}

pub fn plan_log_truncation(
    current_first_log_index: u64,
    snapshot_last_included_index: u64,
    committed_index: u64,
    retention_entries: u64,
) -> LogTruncationPlan {
    if snapshot_last_included_index > committed_index {
        return LogTruncationPlan::InvalidSnapshotIndex {
            snapshot_last_included_index,
            committed_index,
        };
    }

    // Keep a bounded tail behind the snapshot index for easier debugging and recovery.
    let retain_start = snapshot_last_included_index
        .saturating_add(1)
        .saturating_sub(retention_entries);
    let new_first_log_index = current_first_log_index.max(retain_start);

    if new_first_log_index <= current_first_log_index {
        LogTruncationPlan::Noop
    } else {
        LogTruncationPlan::TruncatePrefixTo {
            new_first_log_index,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_snapshot_index_beyond_committed_index() {
        let plan = plan_log_truncation(1, 101, 100, 0);
        assert_eq!(
            plan,
            LogTruncationPlan::InvalidSnapshotIndex {
                snapshot_last_included_index: 101,
                committed_index: 100,
            }
        );
    }

    #[test]
    fn noops_when_current_prefix_is_already_tight_enough() {
        let plan = plan_log_truncation(51, 100, 100, 50);
        assert_eq!(plan, LogTruncationPlan::Noop);
    }

    #[test]
    fn truncates_prefix_to_snapshot_plus_one_without_retention() {
        let plan = plan_log_truncation(1, 100, 100, 0);
        assert_eq!(
            plan,
            LogTruncationPlan::TruncatePrefixTo {
                new_first_log_index: 101,
            }
        );
    }

    #[test]
    fn respects_retention_window_when_planning_truncation() {
        let plan = plan_log_truncation(1, 100, 100, 25);
        assert_eq!(
            plan,
            LogTruncationPlan::TruncatePrefixTo {
                new_first_log_index: 76,
            }
        );
    }
}
