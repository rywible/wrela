use crate::db::time::hlc::HlTimestamp;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommitWaitResult {
    pub wait_ms: u64,
    pub external_consistency_ready: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommitWaitPolicy {
    pub max_wait_ms: u64,
}

impl CommitWaitPolicy {
    pub const DEFAULT: Self = Self { max_wait_ms: 5_000 };

    pub fn evaluate(
        &self,
        commit_ts_packed: u64,
        now_packed: u64,
        uncertainty_upper_packed: u64,
    ) -> CommitWaitResult {
        let commit = HlTimestamp::unpack(commit_ts_packed);
        let now = HlTimestamp::unpack(now_packed);
        let uncertainty_upper = HlTimestamp::unpack(uncertainty_upper_packed);

        let target_physical = commit.physical_ms.max(uncertainty_upper.physical_ms);
        if now.physical_ms >= target_physical {
            return CommitWaitResult {
                wait_ms: 0,
                external_consistency_ready: true,
            };
        }

        let raw_wait = target_physical.saturating_sub(now.physical_ms);
        CommitWaitResult {
            wait_ms: raw_wait.min(self.max_wait_ms),
            external_consistency_ready: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_zero_when_now_is_past_commit_and_uncertainty() {
        let policy = CommitWaitPolicy::DEFAULT;
        let commit = HlTimestamp {
            physical_ms: 100,
            logical: 1,
        }
        .pack();
        let uncertainty = HlTimestamp {
            physical_ms: 110,
            logical: u16::MAX,
        }
        .pack();
        let now = HlTimestamp {
            physical_ms: 111,
            logical: 0,
        }
        .pack();

        let result = policy.evaluate(commit, now, uncertainty);
        assert!(result.external_consistency_ready);
        assert_eq!(result.wait_ms, 0);
    }

    #[test]
    fn caps_wait_to_policy_limit() {
        let policy = CommitWaitPolicy { max_wait_ms: 25 };
        let commit = HlTimestamp {
            physical_ms: 100,
            logical: 0,
        }
        .pack();
        let uncertainty = HlTimestamp {
            physical_ms: 500,
            logical: 0,
        }
        .pack();
        let now = HlTimestamp {
            physical_ms: 100,
            logical: 0,
        }
        .pack();

        let result = policy.evaluate(commit, now, uncertainty);
        assert!(!result.external_consistency_ready);
        assert_eq!(result.wait_ms, 25);
    }
}
