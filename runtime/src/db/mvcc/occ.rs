use crate::db::types::DbError;

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
