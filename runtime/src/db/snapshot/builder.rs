use crate::db::snapshot::checksum::checksum;
use crate::db::snapshot::manifest::{
    SNAPSHOT_MANIFEST_FORMAT_VERSION, SnapshotManifest, SnapshotManifestMetadata,
};
use crate::db::time::hlc::HlTimestamp;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SnapshotManifestBuildInput<'a> {
    pub payload: &'a [u8],
    pub last_index: u64,
    pub last_term: u64,
    pub hlc_watermark: u64,
}

pub fn build_manifest_from_input(input: SnapshotManifestBuildInput<'_>) -> SnapshotManifest {
    SnapshotManifest {
        metadata: SnapshotManifestMetadata {
            format_version: SNAPSHOT_MANIFEST_FORMAT_VERSION,
        },
        version: SNAPSHOT_MANIFEST_FORMAT_VERSION,
        last_index: input.last_index,
        last_term: input.last_term,
        checksum: checksum(input.payload),
        hlc_watermark: input.hlc_watermark,
    }
}

pub fn build_manifest(payload: &[u8], last_index: u64, last_term: u64) -> SnapshotManifest {
    build_manifest_from_input(SnapshotManifestBuildInput {
        payload,
        last_index,
        last_term,
        hlc_watermark: HlTimestamp {
            physical_ms: 1,
            logical: 0,
        }
        .pack(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::snapshot::checksum::checksum;

    #[test]
    fn typed_builder_populates_all_manifest_fields() {
        let payload = b"snapshot-bytes";
        let input = SnapshotManifestBuildInput {
            payload,
            last_index: 42,
            last_term: 7,
            hlc_watermark: 1_234_567,
        };

        let manifest = build_manifest_from_input(input);
        assert_eq!(
            manifest.metadata.format_version,
            SNAPSHOT_MANIFEST_FORMAT_VERSION
        );
        assert_eq!(manifest.version, SNAPSHOT_MANIFEST_FORMAT_VERSION);
        assert_eq!(manifest.last_index, 42);
        assert_eq!(manifest.last_term, 7);
        assert_eq!(manifest.checksum, checksum(payload));
        assert_eq!(manifest.hlc_watermark, 1_234_567);
    }

    #[test]
    fn legacy_helper_sets_compatible_default_watermark() {
        let manifest = build_manifest(b"snapshot-bytes", 5, 3);
        assert_eq!(manifest.hlc_watermark, 1 << 16);
    }
}
