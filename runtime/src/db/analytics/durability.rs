use crate::db::backup::{
    BackupManifestEnvelope, RestorePlanError, SnapshotMetadata, build_restore_plan,
};
use crate::db::restore::{
    RestoreLoadRequest, RestoreValidationError, validate_restore_load_request,
};
use crate::db::snapshot::manifest::SnapshotManifest;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalyticsCheckpoint {
    pub stream: String,
    pub commit_seq: u64,
    pub watermark_packed: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalyticsDurabilityEnvelope {
    pub dataset: String,
    pub backup: BackupManifestEnvelope,
    pub checkpoint: AnalyticsCheckpoint,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalyticsRecoveryPlan {
    pub source_uri: String,
    pub restore_commit_seq: u64,
    pub restore_watermark_packed: u64,
    pub replay_from_commit_seq: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnalyticsDurabilityError {
    InvalidDataset,
    RestorePlan(RestorePlanError),
    RestoreValidation(RestoreValidationError),
    CheckpointRegression { expected_at_least: u64, actual: u64 },
}

pub fn build_envelope(
    dataset: impl Into<String>,
    target_uri: impl Into<String>,
    snapshot: SnapshotMetadata,
    checkpoint: AnalyticsCheckpoint,
) -> Result<AnalyticsDurabilityEnvelope, AnalyticsDurabilityError> {
    let dataset = dataset.into();
    if dataset.trim().is_empty() {
        return Err(AnalyticsDurabilityError::InvalidDataset);
    }
    Ok(AnalyticsDurabilityEnvelope {
        dataset,
        backup: BackupManifestEnvelope::new(target_uri, snapshot),
        checkpoint,
    })
}

pub fn plan_recovery(
    envelope: &AnalyticsDurabilityEnvelope,
    manifest: &SnapshotManifest,
    payload: &[u8],
    last_acked_commit_seq: u64,
) -> Result<AnalyticsRecoveryPlan, AnalyticsDurabilityError> {
    build_restore_plan(&envelope.backup, manifest)
        .map_err(AnalyticsDurabilityError::RestorePlan)?;

    let request = RestoreLoadRequest {
        source_uri: envelope.backup.target_uri.clone(),
        expected_snapshot: envelope.backup.snapshot,
        snapshot_manifest: manifest.clone(),
        snapshot_payload: payload.to_vec(),
    };

    validate_restore_load_request(&request).map_err(AnalyticsDurabilityError::RestoreValidation)?;

    if envelope.checkpoint.commit_seq < last_acked_commit_seq {
        return Err(AnalyticsDurabilityError::CheckpointRegression {
            expected_at_least: last_acked_commit_seq,
            actual: envelope.checkpoint.commit_seq,
        });
    }

    Ok(AnalyticsRecoveryPlan {
        source_uri: envelope.backup.target_uri.clone(),
        restore_commit_seq: envelope.checkpoint.commit_seq,
        restore_watermark_packed: envelope.checkpoint.watermark_packed,
        replay_from_commit_seq: envelope.checkpoint.commit_seq.saturating_add(1),
    })
}

#[cfg(test)]
mod tests {
    use super::{AnalyticsCheckpoint, AnalyticsDurabilityError, build_envelope, plan_recovery};
    use crate::db::backup::SnapshotMetadata;
    use crate::db::snapshot::builder::build_manifest;
    use crate::db::time::hlc::HlTimestamp;

    #[test]
    fn build_and_recover_is_checkpoint_aware() {
        let payload = b"analytics-checkpointed-backup";
        let manifest = build_manifest(payload, 128, 9);
        let envelope = build_envelope(
            "orders_analytics",
            "s3://analytics/backup/orders",
            SnapshotMetadata {
                last_index: manifest.last_index,
                last_term: manifest.last_term,
                checksum: manifest.checksum,
            },
            AnalyticsCheckpoint {
                stream: "orders".to_string(),
                commit_seq: 42,
                watermark_packed: HlTimestamp {
                    physical_ms: 2_000,
                    logical: 0,
                }
                .pack(),
            },
        )
        .expect("envelope should build");

        let plan = plan_recovery(&envelope, &manifest, payload, 40).expect("recovery should pass");
        assert_eq!(plan.restore_commit_seq, 42);
        assert_eq!(plan.replay_from_commit_seq, 43);
    }

    #[test]
    fn recovery_fails_on_checkpoint_regression() {
        let payload = b"analytics-checkpointed-backup";
        let manifest = build_manifest(payload, 128, 9);
        let envelope = build_envelope(
            "orders_analytics",
            "s3://analytics/backup/orders",
            SnapshotMetadata {
                last_index: manifest.last_index,
                last_term: manifest.last_term,
                checksum: manifest.checksum,
            },
            AnalyticsCheckpoint {
                stream: "orders".to_string(),
                commit_seq: 10,
                watermark_packed: 1,
            },
        )
        .expect("envelope should build");

        let err = plan_recovery(&envelope, &manifest, payload, 11).expect_err("must fail closed");
        assert!(matches!(
            err,
            AnalyticsDurabilityError::CheckpointRegression {
                expected_at_least: 11,
                actual: 10
            }
        ));
    }
}
