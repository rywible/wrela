use crate::db::snapshot::checksum::checksum;
use crate::db::time::hlc::HlTimestamp;

pub const SNAPSHOT_MANIFEST_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SnapshotManifestMetadata {
    pub format_version: u32,
}

#[derive(Debug, Clone)]
pub struct SnapshotManifest {
    pub metadata: SnapshotManifestMetadata,
    /// Legacy field kept for compatibility with older callsites/tests.
    pub version: u32,
    pub last_index: u64,
    pub last_term: u64,
    pub checksum: u64,
    pub hlc_watermark: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotValidationError {
    UnsupportedVersion { found: u32, supported: u32 },
    ChecksumMismatch { expected: u64, actual: u64 },
    InvalidWatermarkSemantics { watermark: u64 },
}

impl SnapshotManifest {
    pub fn format_version(&self) -> u32 {
        if self.metadata.format_version != 0 {
            self.metadata.format_version
        } else {
            self.version
        }
    }

    pub fn validate_payload(&self, payload: &[u8]) -> Result<(), SnapshotValidationError> {
        let format_version = self.format_version();
        if format_version != SNAPSHOT_MANIFEST_FORMAT_VERSION {
            return Err(SnapshotValidationError::UnsupportedVersion {
                found: format_version,
                supported: SNAPSHOT_MANIFEST_FORMAT_VERSION,
            });
        }

        let watermark = HlTimestamp::unpack(self.hlc_watermark);
        if watermark.physical_ms == 0 {
            return Err(SnapshotValidationError::InvalidWatermarkSemantics {
                watermark: self.hlc_watermark,
            });
        }

        let actual = checksum(payload);
        if actual != self.checksum {
            return Err(SnapshotValidationError::ChecksumMismatch {
                expected: self.checksum,
                actual,
            });
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::snapshot::builder::build_manifest;
    use crate::db::time::hlc::HlTimestamp;

    #[test]
    fn validates_matching_payload_checksum() {
        let payload = b"snapshot-bytes";
        let manifest = build_manifest(payload, 42, 7);
        assert_eq!(manifest.validate_payload(payload), Ok(()));
    }

    #[test]
    fn rejects_payload_checksum_mismatch() {
        let payload = b"snapshot-bytes";
        let manifest = build_manifest(payload, 42, 7);
        let err = manifest
            .validate_payload(b"snapshot-bytes-corrupted")
            .expect_err("checksum mismatch expected");
        assert!(matches!(
            err,
            SnapshotValidationError::ChecksumMismatch { .. }
        ));
    }

    #[test]
    fn rejects_unsupported_manifest_versions() {
        let payload = b"snapshot-bytes";
        let mut manifest = build_manifest(payload, 42, 7);
        manifest.metadata.format_version = 2;

        let err = manifest
            .validate_payload(payload)
            .expect_err("unsupported version expected");
        assert_eq!(
            err,
            SnapshotValidationError::UnsupportedVersion {
                found: 2,
                supported: SNAPSHOT_MANIFEST_FORMAT_VERSION,
            }
        );
    }

    #[test]
    fn rejects_invalid_watermark_semantics() {
        let payload = b"snapshot-bytes";
        let mut manifest = build_manifest(payload, 42, 7);
        manifest.hlc_watermark = 0;

        let err = manifest
            .validate_payload(payload)
            .expect_err("invalid watermark expected");
        assert_eq!(
            err,
            SnapshotValidationError::InvalidWatermarkSemantics { watermark: 0 }
        );
    }

    #[test]
    fn uses_legacy_version_when_metadata_version_unset() {
        let payload = b"snapshot-bytes";
        let mut manifest = build_manifest(payload, 42, 7);
        manifest.metadata.format_version = 0;
        manifest.version = SNAPSHOT_MANIFEST_FORMAT_VERSION;

        assert_eq!(manifest.validate_payload(payload), Ok(()));
    }

    #[test]
    fn roundtrips_hlc_watermark() {
        let payload = b"snapshot-bytes";
        let watermark = HlTimestamp {
            physical_ms: 1_710_000_000_000,
            logical: 77,
        }
        .pack();
        let mut manifest = build_manifest(payload, 42, 7);
        manifest.hlc_watermark = watermark;

        assert_eq!(manifest.hlc_watermark, watermark);
        assert_eq!(HlTimestamp::unpack(manifest.hlc_watermark).logical, 77);
        assert_eq!(manifest.validate_payload(payload), Ok(()));
    }
}
