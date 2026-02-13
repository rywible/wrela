use crate::db::snapshot::manifest::SnapshotManifest;
use serde::{Deserialize, Serialize};
use std::fmt;

pub const BACKUP_MANIFEST_VERSION: u32 = 1;
pub const MULTIPART_UPLOAD_SESSION_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotMetadata {
    pub last_index: u64,
    pub last_term: u64,
    pub checksum: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupManifestEnvelope {
    pub version: u32,
    pub target_uri: String,
    pub snapshot: SnapshotMetadata,
}

impl BackupManifestEnvelope {
    pub fn new(target_uri: impl Into<String>, snapshot: SnapshotMetadata) -> Self {
        Self {
            version: BACKUP_MANIFEST_VERSION,
            target_uri: target_uri.into(),
            snapshot,
        }
    }

    pub fn canonical_bytes(&self) -> Vec<u8> {
        format!(
            "version={};target_uri={};last_index={};last_term={};checksum={}",
            self.version,
            self.target_uri,
            self.snapshot.last_index,
            self.snapshot.last_term,
            self.snapshot.checksum
        )
        .into_bytes()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestorePlanError {
    UnsupportedEnvelopeVersion { found: u32 },
    InvalidTargetUri,
    SnapshotVersionUnsupported { found: u32 },
    SnapshotIndexMismatch { expected: u64, actual: u64 },
    SnapshotTermMismatch { expected: u64, actual: u64 },
    SnapshotChecksumMismatch { expected: u64, actual: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestorePlan {
    pub source_uri: String,
    pub snapshot: SnapshotMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestorePlanOutcome {
    PlanReady(RestorePlan),
}

pub fn verify_snapshot_manifest_consistency(
    envelope: &BackupManifestEnvelope,
    snapshot_manifest: &SnapshotManifest,
) -> Result<(), RestorePlanError> {
    if envelope.version != BACKUP_MANIFEST_VERSION {
        return Err(RestorePlanError::UnsupportedEnvelopeVersion {
            found: envelope.version,
        });
    }

    if envelope.target_uri.trim().is_empty() {
        return Err(RestorePlanError::InvalidTargetUri);
    }

    if snapshot_manifest.version != 1 {
        return Err(RestorePlanError::SnapshotVersionUnsupported {
            found: snapshot_manifest.version,
        });
    }

    if snapshot_manifest.last_index != envelope.snapshot.last_index {
        return Err(RestorePlanError::SnapshotIndexMismatch {
            expected: envelope.snapshot.last_index,
            actual: snapshot_manifest.last_index,
        });
    }

    if snapshot_manifest.last_term != envelope.snapshot.last_term {
        return Err(RestorePlanError::SnapshotTermMismatch {
            expected: envelope.snapshot.last_term,
            actual: snapshot_manifest.last_term,
        });
    }

    if snapshot_manifest.checksum != envelope.snapshot.checksum {
        return Err(RestorePlanError::SnapshotChecksumMismatch {
            expected: envelope.snapshot.checksum,
            actual: snapshot_manifest.checksum,
        });
    }

    Ok(())
}

pub fn build_restore_plan(
    envelope: &BackupManifestEnvelope,
    snapshot_manifest: &SnapshotManifest,
) -> Result<RestorePlanOutcome, RestorePlanError> {
    verify_snapshot_manifest_consistency(envelope, snapshot_manifest)?;

    Ok(RestorePlanOutcome::PlanReady(RestorePlan {
        source_uri: envelope.target_uri.clone(),
        snapshot: envelope.snapshot,
    }))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MultipartPartState {
    Pending,
    Uploaded,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MultipartUploadPart {
    pub part_number: u32,
    pub checksum: String,
    pub state: MultipartPartState,
    pub attempt_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MultipartUploadSession {
    pub version: u32,
    pub target_uri: String,
    pub upload_id: String,
    pub parts: Vec<MultipartUploadPart>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub initial_backoff_ms: u64,
    pub backoff_multiplier: u32,
    pub max_backoff_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartRetrySchedule {
    pub part_number: u32,
    pub attempt: u32,
    pub delay_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultipartProgress {
    pub total_parts: usize,
    pub pending_parts: usize,
    pub uploaded_parts: usize,
    pub failed_parts: usize,
    pub status: MultipartProgressStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MultipartProgressStatus {
    InProgress,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MultipartUploadError {
    UnsupportedSessionVersion {
        found: u32,
    },
    InvalidTargetUri,
    InvalidUploadId,
    EmptyParts,
    DuplicatePartNumber {
        part_number: u32,
    },
    InvalidChecksum {
        part_number: u32,
    },
    UnknownPart {
        part_number: u32,
    },
    RetryPolicyInvalid,
    RetryExhausted {
        part_number: u32,
        attempts: u32,
        max_attempts: u32,
    },
    SessionEncoding(String),
    SessionDecoding(String),
}

impl fmt::Display for MultipartUploadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedSessionVersion { found } => {
                write!(f, "unsupported multipart session version {found}")
            }
            Self::InvalidTargetUri => write!(f, "multipart session target_uri is empty"),
            Self::InvalidUploadId => write!(f, "multipart session upload_id is empty"),
            Self::EmptyParts => write!(f, "multipart session has no parts"),
            Self::DuplicatePartNumber { part_number } => {
                write!(f, "duplicate multipart part_number={part_number}")
            }
            Self::InvalidChecksum { part_number } => {
                write!(f, "empty checksum for part_number={part_number}")
            }
            Self::UnknownPart { part_number } => write!(f, "unknown part_number={part_number}"),
            Self::RetryPolicyInvalid => write!(f, "retry policy must have non-zero max_attempts"),
            Self::RetryExhausted {
                part_number,
                attempts,
                max_attempts,
            } => write!(
                f,
                "retry exhausted for part_number={part_number} attempts={attempts} max_attempts={max_attempts}"
            ),
            Self::SessionEncoding(message) => write!(f, "session encoding failed: {message}"),
            Self::SessionDecoding(message) => write!(f, "session decoding failed: {message}"),
        }
    }
}

impl std::error::Error for MultipartUploadError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupManifestRecord {
    pub manifest_id: String,
    pub target_uri: String,
    pub created_at_epoch_day: u64,
    pub snapshot: SnapshotMetadata,
    #[serde(default)]
    pub erasure_proofs: Vec<BackupErasureProof>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupErasureProof {
    pub intent_id: String,
    pub proof_hash: String,
    pub residency_scope: String,
    pub mode: String,
    pub commit_seq: u64,
    pub key_fingerprint: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetentionPolicy {
    pub keep_last_n: usize,
    pub min_age_days: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RetentionDecisionReason {
    KeepLastN,
    AgeBelowMin { age_days: u64, min_age_days: u64 },
    AgeEligible { age_days: u64, min_age_days: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetentionDecision {
    pub manifest_id: String,
    pub should_prune: bool,
    pub reason: RetentionDecisionReason,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetentionPrunePlan {
    pub decisions: Vec<RetentionDecision>,
    pub retained_manifest_ids: Vec<String>,
    pub pruned_manifest_ids: Vec<String>,
}

pub fn plan_retention_prune(
    policy: RetentionPolicy,
    manifests: &[BackupManifestRecord],
    now_epoch_day: u64,
) -> RetentionPrunePlan {
    let mut ordered = manifests.to_vec();
    ordered.sort_by(|a, b| {
        b.created_at_epoch_day
            .cmp(&a.created_at_epoch_day)
            .then_with(|| a.manifest_id.cmp(&b.manifest_id))
            .then_with(|| a.target_uri.cmp(&b.target_uri))
    });

    let mut decisions = Vec::with_capacity(ordered.len());
    let mut retained_manifest_ids = Vec::new();
    let mut pruned_manifest_ids = Vec::new();

    for (idx, manifest) in ordered.iter().enumerate() {
        let (should_prune, reason) = if idx < policy.keep_last_n {
            (false, RetentionDecisionReason::KeepLastN)
        } else {
            let age_days = now_epoch_day.saturating_sub(manifest.created_at_epoch_day);
            if age_days >= policy.min_age_days {
                (
                    true,
                    RetentionDecisionReason::AgeEligible {
                        age_days,
                        min_age_days: policy.min_age_days,
                    },
                )
            } else {
                (
                    false,
                    RetentionDecisionReason::AgeBelowMin {
                        age_days,
                        min_age_days: policy.min_age_days,
                    },
                )
            }
        };

        let manifest_id = manifest.manifest_id.clone();
        decisions.push(RetentionDecision {
            manifest_id: manifest_id.clone(),
            should_prune,
            reason,
        });
        if should_prune {
            pruned_manifest_ids.push(manifest_id);
        } else {
            retained_manifest_ids.push(manifest_id);
        }
    }

    RetentionPrunePlan {
        decisions,
        retained_manifest_ids,
        pruned_manifest_ids,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationSamplingPolicy {
    pub sample_size: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationSamplePlan {
    pub sampled_manifest_ids: Vec<String>,
}

pub fn plan_verification_sample(
    policy: VerificationSamplingPolicy,
    manifests: &[BackupManifestRecord],
) -> VerificationSamplePlan {
    let mut ordered: Vec<(&BackupManifestRecord, u64)> = manifests
        .iter()
        .map(|manifest| {
            let key = format!(
                "{}|{}|{}|{}|{}|{}",
                manifest.manifest_id,
                manifest.target_uri,
                manifest.created_at_epoch_day,
                manifest.snapshot.last_index,
                manifest.snapshot.last_term,
                manifest.snapshot.checksum
            );
            (manifest, deterministic_hash64(key.as_bytes()))
        })
        .collect();

    ordered.sort_by(|(a_manifest, a_hash), (b_manifest, b_hash)| {
        a_hash
            .cmp(b_hash)
            .then_with(|| a_manifest.manifest_id.cmp(&b_manifest.manifest_id))
    });

    let sampled_manifest_ids = ordered
        .into_iter()
        .take(policy.sample_size)
        .map(|(manifest, _)| manifest.manifest_id.clone())
        .collect();

    VerificationSamplePlan {
        sampled_manifest_ids,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VerificationStatus {
    Verified,
    Corrupt,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationResult {
    pub manifest_id: String,
    pub status: VerificationStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VerificationSummaryStatus {
    Empty,
    Healthy,
    Degraded,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationStatusSummary {
    pub total_manifests: usize,
    pub verified_manifests: usize,
    pub corrupt_manifests: usize,
    pub skipped_manifests: usize,
    pub status: VerificationSummaryStatus,
}

pub fn summarize_verification_results(results: &[VerificationResult]) -> VerificationStatusSummary {
    let total_manifests = results.len();
    let mut verified_manifests = 0usize;
    let mut corrupt_manifests = 0usize;
    let mut skipped_manifests = 0usize;

    for result in results {
        match result.status {
            VerificationStatus::Verified => verified_manifests += 1,
            VerificationStatus::Corrupt => corrupt_manifests += 1,
            VerificationStatus::Skipped => skipped_manifests += 1,
        }
    }

    let status = if total_manifests == 0 {
        VerificationSummaryStatus::Empty
    } else if corrupt_manifests > 0 {
        VerificationSummaryStatus::Failed
    } else if skipped_manifests > 0 {
        VerificationSummaryStatus::Degraded
    } else {
        VerificationSummaryStatus::Healthy
    };

    VerificationStatusSummary {
        total_manifests,
        verified_manifests,
        corrupt_manifests,
        skipped_manifests,
        status,
    }
}

fn deterministic_hash64(bytes: &[u8]) -> u64 {
    const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    let mut hash = FNV_OFFSET_BASIS;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

impl RetryPolicy {
    pub fn deterministic_backoff_schedule(&self) -> Result<Vec<u64>, MultipartUploadError> {
        if self.max_attempts == 0 {
            return Err(MultipartUploadError::RetryPolicyInvalid);
        }
        let mut schedule = Vec::with_capacity(self.max_attempts as usize);
        let mut current = self.initial_backoff_ms;
        for _ in 0..self.max_attempts {
            let bounded = current.min(self.max_backoff_ms);
            schedule.push(bounded);
            if self.backoff_multiplier <= 1 {
                continue;
            }
            current = current
                .saturating_mul(self.backoff_multiplier as u64)
                .min(self.max_backoff_ms);
        }
        Ok(schedule)
    }
}

impl MultipartUploadSession {
    pub fn new(
        target_uri: impl Into<String>,
        upload_id: impl Into<String>,
        chunk_checksums: Vec<String>,
    ) -> Result<Self, MultipartUploadError> {
        let parts = chunk_checksums
            .into_iter()
            .enumerate()
            .map(|(idx, checksum)| MultipartUploadPart {
                part_number: (idx as u32) + 1,
                checksum,
                state: MultipartPartState::Pending,
                attempt_count: 0,
            })
            .collect();

        let session = Self {
            version: MULTIPART_UPLOAD_SESSION_VERSION,
            target_uri: target_uri.into(),
            upload_id: upload_id.into(),
            parts,
        };
        session.validate()?;
        Ok(session)
    }

    pub fn to_persisted_bytes(&self) -> Result<Vec<u8>, MultipartUploadError> {
        self.validate()?;
        serde_json::to_vec(self)
            .map_err(|err| MultipartUploadError::SessionEncoding(err.to_string()))
    }

    pub fn from_persisted_bytes(bytes: &[u8]) -> Result<Self, MultipartUploadError> {
        let session = serde_json::from_slice::<Self>(bytes)
            .map_err(|err| MultipartUploadError::SessionDecoding(err.to_string()))?;
        session.validate()?;
        Ok(session)
    }

    pub fn progress(&self) -> MultipartProgress {
        let total_parts = self.parts.len();
        let mut pending_parts = 0usize;
        let mut uploaded_parts = 0usize;
        let mut failed_parts = 0usize;

        for part in &self.parts {
            match part.state {
                MultipartPartState::Pending => pending_parts += 1,
                MultipartPartState::Uploaded => uploaded_parts += 1,
                MultipartPartState::Failed => failed_parts += 1,
            }
        }

        let status = if uploaded_parts == total_parts && total_parts > 0 {
            MultipartProgressStatus::Completed
        } else if failed_parts > 0 {
            MultipartProgressStatus::Failed
        } else {
            MultipartProgressStatus::InProgress
        };

        MultipartProgress {
            total_parts,
            pending_parts,
            uploaded_parts,
            failed_parts,
            status,
        }
    }

    pub fn mark_part_uploaded(&mut self, part_number: u32) -> Result<(), MultipartUploadError> {
        let part = self.part_mut(part_number)?;
        part.state = MultipartPartState::Uploaded;
        Ok(())
    }

    pub fn mark_part_failed(&mut self, part_number: u32) -> Result<(), MultipartUploadError> {
        let part = self.part_mut(part_number)?;
        part.state = MultipartPartState::Failed;
        Ok(())
    }

    pub fn retry_failed_parts(
        &mut self,
        policy: RetryPolicy,
    ) -> Result<Vec<PartRetrySchedule>, MultipartUploadError> {
        let schedule = policy.deterministic_backoff_schedule()?;
        let mut retry_plan = Vec::new();

        for part in &mut self.parts {
            if part.state != MultipartPartState::Failed {
                continue;
            }

            if part.attempt_count >= policy.max_attempts {
                return Err(MultipartUploadError::RetryExhausted {
                    part_number: part.part_number,
                    attempts: part.attempt_count,
                    max_attempts: policy.max_attempts,
                });
            }

            let attempt = part.attempt_count + 1;
            let delay_ms = schedule[(attempt - 1) as usize];
            part.attempt_count = attempt;
            part.state = MultipartPartState::Pending;
            retry_plan.push(PartRetrySchedule {
                part_number: part.part_number,
                attempt,
                delay_ms,
            });
        }

        retry_plan.sort_by_key(|entry| entry.part_number);
        Ok(retry_plan)
    }

    fn validate(&self) -> Result<(), MultipartUploadError> {
        if self.version != MULTIPART_UPLOAD_SESSION_VERSION {
            return Err(MultipartUploadError::UnsupportedSessionVersion {
                found: self.version,
            });
        }
        if self.target_uri.trim().is_empty() {
            return Err(MultipartUploadError::InvalidTargetUri);
        }
        if self.upload_id.trim().is_empty() {
            return Err(MultipartUploadError::InvalidUploadId);
        }
        if self.parts.is_empty() {
            return Err(MultipartUploadError::EmptyParts);
        }

        let mut previous_part_number = 0u32;
        for part in &self.parts {
            if part.part_number <= previous_part_number {
                return Err(MultipartUploadError::DuplicatePartNumber {
                    part_number: part.part_number,
                });
            }
            if part.checksum.trim().is_empty() {
                return Err(MultipartUploadError::InvalidChecksum {
                    part_number: part.part_number,
                });
            }
            previous_part_number = part.part_number;
        }

        Ok(())
    }

    fn part_mut(
        &mut self,
        part_number: u32,
    ) -> Result<&mut MultipartUploadPart, MultipartUploadError> {
        self.parts
            .iter_mut()
            .find(|part| part.part_number == part_number)
            .ok_or(MultipartUploadError::UnknownPart { part_number })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(last_index: u64, last_term: u64, checksum: u64) -> SnapshotManifest {
        SnapshotManifest {
            metadata: crate::db::snapshot::manifest::SnapshotManifestMetadata { format_version: 1 },
            version: 1,
            last_index,
            last_term,
            checksum,
            hlc_watermark: 1,
        }
    }

    fn backup_record(
        manifest_id: &str,
        created_at_epoch_day: u64,
        last_index: u64,
    ) -> BackupManifestRecord {
        BackupManifestRecord {
            manifest_id: manifest_id.to_string(),
            target_uri: format!("s3://cluster-a/backups/{manifest_id}.tar"),
            created_at_epoch_day,
            snapshot: SnapshotMetadata {
                last_index,
                last_term: 1,
                checksum: last_index + 100,
            },
            erasure_proofs: Vec::new(),
        }
    }

    #[test]
    fn build_restore_plan_succeeds_for_consistent_manifest() {
        let envelope = BackupManifestEnvelope::new(
            "s3://cluster-a/backups/2026-02-13/full-0001",
            SnapshotMetadata {
                last_index: 42,
                last_term: 7,
                checksum: 9001,
            },
        );

        let snapshot_manifest = manifest(42, 7, 9001);

        let outcome = build_restore_plan(&envelope, &snapshot_manifest).expect("plan should build");
        assert_eq!(
            outcome,
            RestorePlanOutcome::PlanReady(RestorePlan {
                source_uri: "s3://cluster-a/backups/2026-02-13/full-0001".to_string(),
                snapshot: SnapshotMetadata {
                    last_index: 42,
                    last_term: 7,
                    checksum: 9001,
                },
            })
        );
    }

    #[test]
    fn build_restore_plan_rejects_checksum_mismatch() {
        let envelope = BackupManifestEnvelope::new(
            "s3://cluster-a/backups/2026-02-13/full-0002",
            SnapshotMetadata {
                last_index: 52,
                last_term: 8,
                checksum: 111,
            },
        );

        let snapshot_manifest = manifest(52, 8, 222);

        let err = build_restore_plan(&envelope, &snapshot_manifest)
            .expect_err("checksum mismatch should fail");

        assert_eq!(
            err,
            RestorePlanError::SnapshotChecksumMismatch {
                expected: 111,
                actual: 222,
            }
        );
    }

    #[test]
    fn build_restore_plan_rejects_unsupported_envelope_version() {
        let mut envelope = BackupManifestEnvelope::new(
            "s3://cluster-a/backups/2026-02-13/full-0003",
            SnapshotMetadata {
                last_index: 1,
                last_term: 1,
                checksum: 1,
            },
        );
        envelope.version = 2;

        let snapshot_manifest = manifest(1, 1, 1);

        let err = build_restore_plan(&envelope, &snapshot_manifest)
            .expect_err("unsupported version should fail");

        assert_eq!(
            err,
            RestorePlanError::UnsupportedEnvelopeVersion { found: 2 }
        );
    }

    #[test]
    fn multipart_upload_resume_recovers_interrupted_session_and_completes() {
        let mut session = MultipartUploadSession::new(
            "s3://cluster-a/backups/2026-02-13/full-0004",
            "upload-0004",
            vec![
                "chunk-1-sha256".to_string(),
                "chunk-2-sha256".to_string(),
                "chunk-3-sha256".to_string(),
            ],
        )
        .expect("session");

        session.mark_part_uploaded(1).expect("part 1 uploaded");
        session.mark_part_failed(2).expect("part 2 failed");

        let persisted = session.to_persisted_bytes().expect("persist");
        let mut resumed = MultipartUploadSession::from_persisted_bytes(&persisted).expect("resume");

        assert_eq!(resumed.progress().status, MultipartProgressStatus::Failed);
        let retry_plan = resumed
            .retry_failed_parts(RetryPolicy {
                max_attempts: 3,
                initial_backoff_ms: 100,
                backoff_multiplier: 2,
                max_backoff_ms: 400,
            })
            .expect("retry schedule");

        assert_eq!(
            retry_plan,
            vec![PartRetrySchedule {
                part_number: 2,
                attempt: 1,
                delay_ms: 100,
            }]
        );
        resumed
            .mark_part_uploaded(2)
            .expect("part 2 uploaded after retry");
        resumed.mark_part_uploaded(3).expect("part 3 uploaded");

        let progress = resumed.progress();
        assert_eq!(progress.status, MultipartProgressStatus::Completed);
        assert_eq!(progress.uploaded_parts, 3);
        assert_eq!(progress.failed_parts, 0);
    }

    #[test]
    fn multipart_upload_retry_exhaustion_returns_typed_failure() {
        let mut session = MultipartUploadSession::new(
            "s3://cluster-a/backups/2026-02-13/full-0005",
            "upload-0005",
            vec!["chunk-1-sha256".to_string()],
        )
        .expect("session");
        session.mark_part_failed(1).expect("initial failure");
        session.parts[0].attempt_count = 2;

        let err = session
            .retry_failed_parts(RetryPolicy {
                max_attempts: 2,
                initial_backoff_ms: 50,
                backoff_multiplier: 2,
                max_backoff_ms: 200,
            })
            .expect_err("retry must be exhausted");

        assert_eq!(
            err,
            MultipartUploadError::RetryExhausted {
                part_number: 1,
                attempts: 2,
                max_attempts: 2,
            }
        );
    }

    #[test]
    fn multipart_upload_progress_reports_successful_completion() {
        let mut session = MultipartUploadSession::new(
            "s3://cluster-a/backups/2026-02-13/full-0006",
            "upload-0006",
            vec![
                "chunk-1-sha256".to_string(),
                "chunk-2-sha256".to_string(),
                "chunk-3-sha256".to_string(),
            ],
        )
        .expect("session");

        session.mark_part_uploaded(1).expect("part 1");
        session.mark_part_uploaded(2).expect("part 2");
        session.mark_part_uploaded(3).expect("part 3");

        let progress = session.progress();
        assert_eq!(
            progress,
            MultipartProgress {
                total_parts: 3,
                pending_parts: 0,
                uploaded_parts: 3,
                failed_parts: 0,
                status: MultipartProgressStatus::Completed,
            }
        );
    }

    #[test]
    fn retention_prune_plan_is_deterministic() {
        let manifests = vec![
            backup_record("m-003", 200, 30),
            backup_record("m-001", 210, 10),
            backup_record("m-002", 205, 20),
            backup_record("m-004", 170, 40),
        ];
        let mut reversed = manifests.clone();
        reversed.reverse();

        let policy = RetentionPolicy {
            keep_last_n: 2,
            min_age_days: 20,
        };

        let plan_a = plan_retention_prune(policy, &manifests, 220);
        let plan_b = plan_retention_prune(policy, &reversed, 220);

        assert_eq!(plan_a, plan_b);
        assert_eq!(
            plan_a.retained_manifest_ids,
            vec!["m-001".to_string(), "m-002".to_string()]
        );
        assert_eq!(
            plan_a.pruned_manifest_ids,
            vec!["m-003".to_string(), "m-004".to_string()]
        );
    }

    #[test]
    fn verification_sampler_is_deterministic_for_sample_size() {
        let manifests = vec![
            backup_record("m-010", 100, 10),
            backup_record("m-011", 101, 11),
            backup_record("m-012", 102, 12),
            backup_record("m-013", 103, 13),
            backup_record("m-014", 104, 14),
        ];
        let mut shuffled = manifests.clone();
        shuffled.swap(0, 4);
        shuffled.swap(1, 3);

        let policy = VerificationSamplingPolicy { sample_size: 3 };
        let sample_a = plan_verification_sample(policy, &manifests);
        let sample_b = plan_verification_sample(policy, &shuffled);

        assert_eq!(sample_a, sample_b);
        assert_eq!(sample_a.sampled_manifest_ids.len(), 3);
    }

    #[test]
    fn verification_summary_reports_expected_status() {
        let healthy = summarize_verification_results(&[
            VerificationResult {
                manifest_id: "m-001".to_string(),
                status: VerificationStatus::Verified,
            },
            VerificationResult {
                manifest_id: "m-002".to_string(),
                status: VerificationStatus::Verified,
            },
        ]);
        assert_eq!(
            healthy,
            VerificationStatusSummary {
                total_manifests: 2,
                verified_manifests: 2,
                corrupt_manifests: 0,
                skipped_manifests: 0,
                status: VerificationSummaryStatus::Healthy,
            }
        );

        let degraded = summarize_verification_results(&[
            VerificationResult {
                manifest_id: "m-003".to_string(),
                status: VerificationStatus::Verified,
            },
            VerificationResult {
                manifest_id: "m-004".to_string(),
                status: VerificationStatus::Skipped,
            },
        ]);
        assert_eq!(degraded.status, VerificationSummaryStatus::Degraded);

        let failed = summarize_verification_results(&[
            VerificationResult {
                manifest_id: "m-005".to_string(),
                status: VerificationStatus::Corrupt,
            },
            VerificationResult {
                manifest_id: "m-006".to_string(),
                status: VerificationStatus::Verified,
            },
        ]);
        assert_eq!(failed.status, VerificationSummaryStatus::Failed);

        let empty = summarize_verification_results(&[]);
        assert_eq!(empty.status, VerificationSummaryStatus::Empty);
    }
}
