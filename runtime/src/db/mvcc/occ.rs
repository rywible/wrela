//! Optimistic Concurrency Control (OCC) for the Wrela database.
//!
//! Provides version-check–based conflict detection: each write carries an
//! optional `expected_version` that is compared against the current version of
//! the key at commit time. If the versions disagree the write is rejected with
//! an OCC error, signaling the caller to re-read and retry.
//!
//! # Isolation Level
//!
//! **Protected against:**
//! - Lost updates — concurrent writes to the same key are serialized by version
//!   check; at most one writer succeeds per version.
//!
//! **NOT protected against:**
//! - Write skew — two transactions can read overlapping key sets and each write
//!   to a disjoint key, violating an application-level invariant that spans
//!   both keys.
//! - Phantom reads — new keys inserted by a concurrent transaction are not
//!   detected by a transaction that previously scanned the range.
//!
//! Callers requiring stronger isolation (e.g. serializable) must layer
//! additional read-set validation or predicate locking on top of this module.
//!
//! **Retry semantics**: On an OCC rejection the caller should re-read the
//! current value/version and retry the write with the updated
//! `expected_version`. There is no automatic retry inside the database engine.

use crate::db::types::DbError;

/// Validates that the caller's expected version matches the current stored
/// version, returning an OCC mismatch error if they differ.
///
/// When `expected` is `None` the write is unconditional and always succeeds.
/// When `expected` is `Some(v)`, `current` must also be `Some(v)` or the
/// write is rejected.
///
/// This check prevents lost-update anomalies but does **not** guard against
/// write-skew: two transactions reading overlapping keys and writing to
/// disjoint keys can both pass validation. See the module-level docs for
/// the full isolation-level description.
pub fn validate_expected_version(
    expected: Option<u64>,
    current: Option<u64>,
) -> Result<(), DbError> {
    match expected {
        Some(ver) if Some(ver) != current => Err(DbError::occ(format!(
            "expected_version mismatch (expected {}, found {:?})",
            ver, current
        ))),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::types::ErrorCode;

    #[test]
    fn version_mismatch_is_rejected() {
        let err = validate_expected_version(Some(1), Some(2)).expect_err("must reject");
        assert_eq!(err.code, ErrorCode::OccMismatch);
        assert!(err.message.contains("expected 1"));
        assert!(err.message.contains("Some(2)"));
    }

    #[test]
    fn version_mismatch_against_none_is_rejected() {
        let err = validate_expected_version(Some(1), None).expect_err("must reject");
        assert_eq!(err.code, ErrorCode::OccMismatch);
    }

    #[test]
    fn matching_version_passes() {
        validate_expected_version(Some(5), Some(5)).expect("matching version must pass");
    }

    #[test]
    fn none_expected_is_unconditional_write() {
        validate_expected_version(None, Some(42)).expect("None expected is unconditional");
        validate_expected_version(None, None).expect("None expected with no current");
    }

    #[test]
    fn concurrent_version_bump_detected() {
        // Simulate: reader sees version 3, concurrent writer bumps to 4.
        let reader_expected = Some(3);
        let current_after_bump = Some(4);
        let err = validate_expected_version(reader_expected, current_after_bump)
            .expect_err("must detect concurrent bump");
        assert_eq!(err.code, ErrorCode::OccMismatch);
    }
}
