use wrela_runtime::db::read::rejection::{StrongReadErrorCode, enforce_strong_read};
use wrela_runtime::db::time::hlc::HlTimestamp;
use wrela_runtime::db::time::safe_time::{SafeTimeLagBudget, SafeTimePropagator};
use wrela_runtime::db::time::uncertainty::UncertaintyWindow;

#[test]
fn safe_time_propagation_converges_and_emits_lag_violations() {
    let mut p = SafeTimePropagator::default();
    p.observe_shard_safe_time(
        "orders-us-1",
        "us",
        HlTimestamp {
            physical_ms: 100,
            logical: 0,
        }
        .pack(),
    );
    p.observe_shard_safe_time(
        "orders-us-2",
        "us",
        HlTimestamp {
            physical_ms: 90,
            logical: 0,
        }
        .pack(),
    );
    p.observe_shard_safe_time(
        "orders-eu-1",
        "eu",
        HlTimestamp {
            physical_ms: 105,
            logical: 0,
        }
        .pack(),
    );
    p.recompute_region_safe_times();

    let diag = p.diagnostics(
        HlTimestamp {
            physical_ms: 150,
            logical: 0,
        }
        .pack(),
        SafeTimeLagBudget {
            shard_lag_ms: 40,
            region_lag_ms: 45,
        },
    );

    assert!(diag.global_safe_time.is_some());
    assert!(!diag.violations.is_empty());
}

#[test]
fn strong_read_rejection_matrix_is_typed_and_deterministic() {
    let safe_time_lag_err = enforce_strong_read(
        HlTimestamp {
            physical_ms: 500,
            logical: 0,
        }
        .pack(),
        HlTimestamp {
            physical_ms: 450,
            logical: 0,
        }
        .pack(),
        UncertaintyWindow {
            lower_bound: HlTimestamp {
                physical_ms: 430,
                logical: 0,
            }
            .pack(),
            upper_bound: HlTimestamp {
                physical_ms: 470,
                logical: 0,
            }
            .pack(),
        },
    )
    .expect_err("must reject for lag");
    assert_eq!(safe_time_lag_err.code, StrongReadErrorCode::SafeTimeLag);

    let uncertainty_err = enforce_strong_read(
        HlTimestamp {
            physical_ms: 451,
            logical: 3,
        }
        .pack(),
        HlTimestamp {
            physical_ms: 500,
            logical: 0,
        }
        .pack(),
        UncertaintyWindow {
            lower_bound: HlTimestamp {
                physical_ms: 450,
                logical: 0,
            }
            .pack(),
            upper_bound: HlTimestamp {
                physical_ms: 480,
                logical: u16::MAX,
            }
            .pack(),
        },
    )
    .expect_err("must reject for uncertainty");
    assert_eq!(uncertainty_err.code, StrongReadErrorCode::UncertaintyWindow);
    assert!(uncertainty_err.explain.contains("RETRY_AFTER_MS="));
}
