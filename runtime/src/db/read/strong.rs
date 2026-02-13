use crate::db::time::hlc::HlTimestamp;
use crate::db::time::uncertainty::UncertaintyWindow;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrongReadRejectReason {
    SafeTimeLag,
    UncertaintyWindow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrongReadDecision {
    Serve,
    RetryAfter {
        wait_ms: u64,
        reason: StrongReadRejectReason,
    },
}

pub fn evaluate_strong_read(
    requested_ts_packed: u64,
    safe_time_packed: u64,
    uncertainty: UncertaintyWindow,
) -> StrongReadDecision {
    let requested = HlTimestamp::unpack(requested_ts_packed);
    let safe_time = HlTimestamp::unpack(safe_time_packed);
    let uncertainty_lower = HlTimestamp::unpack(uncertainty.lower_bound);
    let uncertainty_upper = HlTimestamp::unpack(uncertainty.upper_bound);

    if requested.physical_ms > safe_time.physical_ms {
        return StrongReadDecision::RetryAfter {
            wait_ms: requested.physical_ms.saturating_sub(safe_time.physical_ms),
            reason: StrongReadRejectReason::SafeTimeLag,
        };
    }

    if requested_ts_packed > uncertainty_lower.pack() {
        return StrongReadDecision::RetryAfter {
            wait_ms: uncertainty_upper
                .physical_ms
                .saturating_sub(uncertainty_lower.physical_ms),
            reason: StrongReadRejectReason::UncertaintyWindow,
        };
    }

    StrongReadDecision::Serve
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_when_requested_is_past_safe_time() {
        let decision = evaluate_strong_read(
            HlTimestamp {
                physical_ms: 210,
                logical: 0,
            }
            .pack(),
            HlTimestamp {
                physical_ms: 200,
                logical: 0,
            }
            .pack(),
            UncertaintyWindow {
                lower_bound: HlTimestamp {
                    physical_ms: 150,
                    logical: 0,
                }
                .pack(),
                upper_bound: HlTimestamp {
                    physical_ms: 175,
                    logical: 0,
                }
                .pack(),
            },
        );
        assert_eq!(
            decision,
            StrongReadDecision::RetryAfter {
                wait_ms: 10,
                reason: StrongReadRejectReason::SafeTimeLag,
            }
        );
    }

    #[test]
    fn rejects_inside_uncertainty_window() {
        let decision = evaluate_strong_read(
            HlTimestamp {
                physical_ms: 160,
                logical: 1,
            }
            .pack(),
            HlTimestamp {
                physical_ms: 200,
                logical: 0,
            }
            .pack(),
            UncertaintyWindow {
                lower_bound: HlTimestamp {
                    physical_ms: 150,
                    logical: 0,
                }
                .pack(),
                upper_bound: HlTimestamp {
                    physical_ms: 170,
                    logical: u16::MAX,
                }
                .pack(),
            },
        );
        assert_eq!(
            decision,
            StrongReadDecision::RetryAfter {
                wait_ms: 20,
                reason: StrongReadRejectReason::UncertaintyWindow,
            }
        );
    }

    #[test]
    fn serves_when_safe_and_outside_uncertainty() {
        let decision = evaluate_strong_read(
            HlTimestamp {
                physical_ms: 120,
                logical: 0,
            }
            .pack(),
            HlTimestamp {
                physical_ms: 200,
                logical: 0,
            }
            .pack(),
            UncertaintyWindow {
                lower_bound: HlTimestamp {
                    physical_ms: 150,
                    logical: 0,
                }
                .pack(),
                upper_bound: HlTimestamp {
                    physical_ms: 170,
                    logical: 0,
                }
                .pack(),
            },
        );

        assert_eq!(decision, StrongReadDecision::Serve);
    }
}
