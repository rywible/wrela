#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactKind {
    WalRecord,
    SnapshotManifest,
    RpcFrame,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormatVersion {
    pub major: u16,
    pub minor: u16,
}

impl FormatVersion {
    pub const fn new(major: u16, minor: u16) -> Self {
        Self { major, minor }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompatibilityPolicy {
    pub min_readable_major: u16,
    pub current_major: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompatibilityError {
    TooOld {
        artifact: ArtifactKind,
        found_major: u16,
        min_readable_major: u16,
    },
    TooNew {
        artifact: ArtifactKind,
        found_major: u16,
        current_major: u16,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompatibilityDecision {
    pub artifact: ArtifactKind,
    pub version: FormatVersion,
    pub readable: bool,
    pub needs_migration: bool,
}

pub const WAL_FORMAT: FormatVersion = FormatVersion::new(1, 0);
pub const SNAPSHOT_FORMAT: FormatVersion = FormatVersion::new(1, 0);
pub const RPC_FORMAT: FormatVersion = FormatVersion::new(1, 0);

pub fn validate(
    artifact: ArtifactKind,
    found: FormatVersion,
    policy: CompatibilityPolicy,
) -> Result<CompatibilityDecision, CompatibilityError> {
    if found.major < policy.min_readable_major {
        return Err(CompatibilityError::TooOld {
            artifact,
            found_major: found.major,
            min_readable_major: policy.min_readable_major,
        });
    }
    if found.major > policy.current_major {
        return Err(CompatibilityError::TooNew {
            artifact,
            found_major: found.major,
            current_major: policy.current_major,
        });
    }
    Ok(CompatibilityDecision {
        artifact,
        version: found,
        readable: true,
        needs_migration: found.major < policy.current_major,
    })
}

pub fn default_policy() -> CompatibilityPolicy {
    CompatibilityPolicy {
        min_readable_major: 1,
        current_major: 1,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ArtifactKind, CompatibilityError, CompatibilityPolicy, FormatVersion, default_policy,
        validate,
    };

    #[test]
    fn current_format_is_readable_without_migration() {
        let decision = validate(
            ArtifactKind::WalRecord,
            FormatVersion::new(1, 2),
            default_policy(),
        )
        .expect("must read current major");
        assert!(decision.readable);
        assert!(!decision.needs_migration);
    }

    #[test]
    fn older_supported_major_requires_migration() {
        let decision = validate(
            ArtifactKind::SnapshotManifest,
            FormatVersion::new(1, 0),
            CompatibilityPolicy {
                min_readable_major: 1,
                current_major: 2,
            },
        )
        .expect("old major should be readable under policy");
        assert!(decision.needs_migration);
    }

    #[test]
    fn too_old_and_too_new_fail_fast() {
        let old = validate(
            ArtifactKind::RpcFrame,
            FormatVersion::new(0, 9),
            default_policy(),
        )
        .expect_err("too old should fail");
        assert!(matches!(old, CompatibilityError::TooOld { .. }));

        let new = validate(
            ArtifactKind::RpcFrame,
            FormatVersion::new(2, 0),
            default_policy(),
        )
        .expect_err("too new should fail");
        assert!(matches!(new, CompatibilityError::TooNew { .. }));
    }
}
