use wrela_runtime::db::backup::{BackupManifestRecord, SnapshotMetadata};
use wrela_runtime::db::erasure::{
    ErasureIntent, ErasureMode, ErasureState, attach_proof_to_backup_manifest,
    build_delete_cdc_event, evaluate_erasure_state, to_backup_proof,
};

fn base_intent() -> ErasureIntent {
    ErasureIntent {
        intent_id: "erase-tenant-a-user-7".to_string(),
        shard: b"tenant-a".to_vec(),
        key: b"user:7".to_vec(),
        requested_at_commit_seq: 100,
        legal_hold_until_commit_seq: None,
        tombstone_retention_commits: 5,
        residency_scope: "US".to_string(),
        mode: ErasureMode::HardDelete,
    }
}

#[test]
fn erasure_state_respects_legal_hold_and_retention_window() {
    let mut intent = base_intent();
    intent.legal_hold_until_commit_seq = Some(120);
    assert_eq!(
        evaluate_erasure_state(&intent, 110),
        ErasureState::BlockedByLegalHold {
            until_commit_seq: 120
        }
    );

    intent.legal_hold_until_commit_seq = None;
    assert_eq!(evaluate_erasure_state(&intent, 102), ErasureState::Pending);
    assert_eq!(
        evaluate_erasure_state(&intent, 105),
        ErasureState::ReadyForPhysicalPrune
    );
}

#[test]
fn delete_intent_propagates_to_cdc_and_backup_proof_artifacts() {
    let intent = base_intent();
    let cdc_event = build_delete_cdc_event(&intent, 105, 1_000_001);
    let proof = to_backup_proof(&intent, 105, 1_000_001);

    let mut manifest = BackupManifestRecord {
        manifest_id: "backup-2026-02-13-a".to_string(),
        target_uri: "s3://cluster-a/backups/backup-2026-02-13-a.tar".to_string(),
        created_at_epoch_day: 20_767,
        snapshot: SnapshotMetadata {
            last_index: 900,
            last_term: 4,
            checksum: 99,
        },
        erasure_proofs: Vec::new(),
    };

    attach_proof_to_backup_manifest(&mut manifest, proof.clone());

    assert_eq!(
        cdc_event
            .erasure_proof
            .as_ref()
            .expect("erasure proof")
            .intent_id,
        intent.intent_id
    );
    assert_eq!(manifest.erasure_proofs, vec![proof]);
}
