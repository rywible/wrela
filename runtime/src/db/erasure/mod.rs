use crate::db::backup::{BackupErasureProof, BackupManifestRecord};
use crate::db::cdc::{CdcErasureProof, CdcEvent, CdcOpKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErasureMode {
    HardDelete,
    CryptoShred,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErasureIntent {
    pub intent_id: String,
    pub shard: Vec<u8>,
    pub key: Vec<u8>,
    pub requested_at_commit_seq: u64,
    pub legal_hold_until_commit_seq: Option<u64>,
    pub tombstone_retention_commits: u64,
    pub residency_scope: String,
    pub mode: ErasureMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErasureState {
    Pending,
    BlockedByLegalHold { until_commit_seq: u64 },
    ReadyForPhysicalPrune,
}

pub fn evaluate_erasure_state(intent: &ErasureIntent, current_commit_seq: u64) -> ErasureState {
    if let Some(until_commit_seq) = intent.legal_hold_until_commit_seq {
        if current_commit_seq < until_commit_seq {
            return ErasureState::BlockedByLegalHold { until_commit_seq };
        }
    }

    let prune_ready_seq = intent
        .requested_at_commit_seq
        .saturating_add(intent.tombstone_retention_commits);
    if current_commit_seq >= prune_ready_seq {
        ErasureState::ReadyForPhysicalPrune
    } else {
        ErasureState::Pending
    }
}

pub fn build_delete_cdc_event(intent: &ErasureIntent, commit_seq: u64, version: u64) -> CdcEvent {
    let proof_hash = erasure_proof_hash(intent, commit_seq, version);
    CdcEvent {
        commit_seq,
        shard: intent.shard.clone(),
        key: intent.key.clone(),
        kind: CdcOpKind::Delete,
        value: None,
        version,
        erasure_proof: Some(CdcErasureProof {
            intent_id: intent.intent_id.clone(),
            proof_hash,
        }),
    }
}

pub fn to_backup_proof(
    intent: &ErasureIntent,
    commit_seq: u64,
    version: u64,
) -> BackupErasureProof {
    BackupErasureProof {
        intent_id: intent.intent_id.clone(),
        proof_hash: erasure_proof_hash(intent, commit_seq, version),
        residency_scope: intent.residency_scope.clone(),
        mode: match intent.mode {
            ErasureMode::HardDelete => "HARD_DELETE".to_string(),
            ErasureMode::CryptoShred => "CRYPTOSHRED".to_string(),
        },
        commit_seq,
        key_fingerprint: hex_fingerprint(&intent.key),
    }
}

pub fn attach_proof_to_backup_manifest(
    manifest: &mut BackupManifestRecord,
    proof: BackupErasureProof,
) {
    if manifest
        .erasure_proofs
        .iter()
        .any(|existing| existing.intent_id == proof.intent_id)
    {
        return;
    }
    manifest.erasure_proofs.push(proof);
    manifest
        .erasure_proofs
        .sort_by(|a, b| a.intent_id.cmp(&b.intent_id));
}

fn erasure_proof_hash(intent: &ErasureIntent, commit_seq: u64, version: u64) -> String {
    let raw = format!(
        "{}|{}|{}|{}|{}",
        intent.intent_id,
        hex_fingerprint(&intent.key),
        commit_seq,
        version,
        intent.residency_scope
    );
    hex_fingerprint(raw.as_bytes())
}

fn hex_fingerprint(bytes: &[u8]) -> String {
    let mut acc: u64 = 0xcbf29ce484222325;
    for byte in bytes {
        acc ^= u64::from(*byte);
        acc = acc.wrapping_mul(0x100000001b3);
    }
    format!("{acc:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::backup::{BackupManifestRecord, SnapshotMetadata};

    fn intent() -> ErasureIntent {
        ErasureIntent {
            intent_id: "erase-001".to_string(),
            shard: b"tenant-a".to_vec(),
            key: b"user:42".to_vec(),
            requested_at_commit_seq: 10,
            legal_hold_until_commit_seq: None,
            tombstone_retention_commits: 3,
            residency_scope: "US".to_string(),
            mode: ErasureMode::HardDelete,
        }
    }

    #[test]
    fn legal_hold_blocks_erasure() {
        let mut blocked = intent();
        blocked.legal_hold_until_commit_seq = Some(50);
        assert_eq!(
            evaluate_erasure_state(&blocked, 11),
            ErasureState::BlockedByLegalHold {
                until_commit_seq: 50
            }
        );
    }

    #[test]
    fn delete_event_and_backup_manifest_receive_same_proof_token() {
        let erase = intent();
        let cdc_event = build_delete_cdc_event(&erase, 12, 88);
        let mut manifest = BackupManifestRecord {
            manifest_id: "m-1".to_string(),
            target_uri: "s3://cluster-a/m-1".to_string(),
            created_at_epoch_day: 123,
            snapshot: SnapshotMetadata {
                last_index: 1,
                last_term: 1,
                checksum: 1,
            },
            erasure_proofs: Vec::new(),
        };

        let proof = to_backup_proof(&erase, 12, 88);
        attach_proof_to_backup_manifest(&mut manifest, proof.clone());

        assert_eq!(cdc_event.kind, CdcOpKind::Delete);
        assert_eq!(
            cdc_event.erasure_proof.as_ref().expect("proof").proof_hash,
            proof.proof_hash
        );
        assert_eq!(manifest.erasure_proofs, vec![proof]);
    }
}
