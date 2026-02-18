use crate::db::read::strong::{StrongReadDecision, StrongReadRejectReason, evaluate_strong_read};
use crate::db::time::uncertainty::UncertaintyWindow;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrongReadErrorCode {
    SafeTimeLag,
    UncertaintyWindow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrongReadError {
    pub code: StrongReadErrorCode,
    pub retry_after_ms: u64,
    pub explain: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrongReadErrorContract {
    pub version: &'static str,
    pub code: StrongReadErrorCode,
    pub reason: &'static str,
}

pub fn error_contracts() -> &'static [StrongReadErrorContract] {
    const CONTRACTS: &[StrongReadErrorContract] = &[
        StrongReadErrorContract {
            version: "v1",
            code: StrongReadErrorCode::SafeTimeLag,
            reason: "requested timestamp is ahead of propagated safe-time",
        },
        StrongReadErrorContract {
            version: "v1",
            code: StrongReadErrorCode::UncertaintyWindow,
            reason: "requested timestamp is inside uncertainty window",
        },
    ];
    CONTRACTS
}

pub fn enforce_strong_read(
    requested_ts_packed: u64,
    safe_time_packed: u64,
    uncertainty: UncertaintyWindow,
) -> Result<(), StrongReadError> {
    match evaluate_strong_read(requested_ts_packed, safe_time_packed, uncertainty) {
        StrongReadDecision::Serve => Ok(()),
        StrongReadDecision::RetryAfter { wait_ms, reason } => {
            let (code, text) = match reason {
                StrongReadRejectReason::SafeTimeLag => (
                    StrongReadErrorCode::SafeTimeLag,
                    "requested timestamp is ahead of propagated safe-time",
                ),
                StrongReadRejectReason::UncertaintyWindow => (
                    StrongReadErrorCode::UncertaintyWindow,
                    "requested timestamp is inside uncertainty window",
                ),
            };
            Err(StrongReadError {
                code,
                retry_after_ms: wait_ms,
                explain: format!("{text}; RETRY_AFTER_MS={wait_ms}"),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::time::hlc::HlTimestamp;

    #[test]
    fn contracts_are_stable_and_versioned() {
        let contracts = error_contracts();
        assert_eq!(contracts.len(), 2);
        assert_eq!(contracts[0].version, "v1");
        assert_eq!(contracts[1].version, "v1");
    }

    #[test]
    fn safe_time_lag_maps_to_typed_error() {
        let err = enforce_strong_read(
            HlTimestamp {
                physical_ms: 200,
                logical: 0,
            }
            .pack(),
            HlTimestamp {
                physical_ms: 100,
                logical: 0,
            }
            .pack(),
            UncertaintyWindow {
                lower_bound: HlTimestamp {
                    physical_ms: 90,
                    logical: 0,
                }
                .pack(),
                upper_bound: HlTimestamp {
                    physical_ms: 120,
                    logical: 0,
                }
                .pack(),
            },
        )
        .expect_err("must reject");

        assert_eq!(err.code, StrongReadErrorCode::SafeTimeLag);
        assert!(err.explain.contains("RETRY_AFTER_MS="));
    }
}
