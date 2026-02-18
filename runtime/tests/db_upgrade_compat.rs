use wrela_runtime::db::versioning::{
    ArtifactKind, CompatibilityError, CompatibilityPolicy, FormatVersion, default_policy, validate,
};

#[test]
fn format_versions_are_explicit_and_validated_at_boundaries() {
    let wal = validate(
        ArtifactKind::WalRecord,
        FormatVersion::new(1, 0),
        default_policy(),
    )
    .expect("wal format should be readable");
    assert!(wal.readable);
}

#[test]
fn incompatible_versions_fail_fast_with_typed_errors() {
    let too_old = validate(
        ArtifactKind::SnapshotManifest,
        FormatVersion::new(0, 5),
        default_policy(),
    )
    .expect_err("too old should fail fast");
    assert!(matches!(too_old, CompatibilityError::TooOld { .. }));

    let too_new = validate(
        ArtifactKind::RpcFrame,
        FormatVersion::new(9, 0),
        default_policy(),
    )
    .expect_err("too new should fail fast");
    assert!(matches!(too_new, CompatibilityError::TooNew { .. }));
}

#[test]
fn compatibility_policy_allows_declared_backward_window() {
    let decision = validate(
        ArtifactKind::SnapshotManifest,
        FormatVersion::new(2, 1),
        CompatibilityPolicy {
            min_readable_major: 2,
            current_major: 3,
        },
    )
    .expect("version in policy window should read");
    assert!(decision.needs_migration);
}
