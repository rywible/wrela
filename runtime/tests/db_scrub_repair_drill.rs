use bytes::Bytes;
use wrela_runtime::db::lsm::sstable::{SsTableEntry, encode_block};
use wrela_runtime::db::scrub::{
    ForegroundLatencySample, RepairAction, ScrubBudget, ScrubTargetKind, SnapshotArtifact,
    SsTableArtifact, WalArtifact, node_quarantine_required, run_artifact_scrub,
};
use wrela_runtime::db::snapshot::builder::build_manifest;
use wrela_runtime::db::wal::format::{Record, RecordKind, encode};

#[test]
fn drill_corrupt_detect_repair_rejoin() {
    let clean_wal = encode(&Record {
        kind: RecordKind::Put,
        namespace: Bytes::from_static(b"core"),
        key: Bytes::from_static(b"k1"),
        value: Bytes::from_static(b"v1"),
        version: 1,
    });
    let mut corrupt_wal = clean_wal.clone();
    corrupt_wal[4] = 9;

    let clean_sst = encode_block(&[SsTableEntry::live(b"a".to_vec(), 1, b"v".to_vec(), None)]);
    let mut corrupt_sst = clean_sst.clone();
    corrupt_sst.truncate(corrupt_sst.len().saturating_sub(1));

    let clean_snapshot_payload = b"snapshot-ok".to_vec();
    let clean_snapshot_manifest = build_manifest(&clean_snapshot_payload, 11, 7);
    let mut corrupt_snapshot_payload = clean_snapshot_payload.clone();
    corrupt_snapshot_payload.push(1);

    let report = run_artifact_scrub(
        &[WalArtifact {
            id: "wal-1".to_string(),
            bytes: corrupt_wal,
        }],
        &[SsTableArtifact {
            id: "sst-1".to_string(),
            encoded_block: corrupt_sst,
        }],
        &[SnapshotArtifact {
            id: "snap-1".to_string(),
            manifest: clean_snapshot_manifest.clone(),
            payload: corrupt_snapshot_payload,
        }],
        ForegroundLatencySample {
            baseline_p99_ms: 5.0,
            observed_p99_ms: 5.8,
        },
        ScrubBudget {
            max_added_p99_ms: 1.0,
        },
    );

    assert_eq!(report.findings.len(), 3);
    assert_eq!(report.findings[0].target.kind, ScrubTargetKind::Wal);
    assert_eq!(report.findings[1].target.kind, ScrubTargetKind::Sstable);
    assert_eq!(report.findings[2].target.kind, ScrubTargetKind::Snapshot);
    assert!(node_quarantine_required(&report));
    assert!(
        report
            .repair_plan
            .iter()
            .any(|step| step.action == RepairAction::RefetchSnapshot)
    );
    assert!(
        report
            .repair_plan
            .iter()
            .any(|step| step.action == RepairAction::QuarantineNode)
    );

    // Simulate deterministic repair and node rejoin.
    let repaired = run_artifact_scrub(
        &[WalArtifact {
            id: "wal-1".to_string(),
            bytes: clean_wal,
        }],
        &[SsTableArtifact {
            id: "sst-1".to_string(),
            encoded_block: clean_sst,
        }],
        &[SnapshotArtifact {
            id: "snap-1".to_string(),
            manifest: clean_snapshot_manifest,
            payload: clean_snapshot_payload,
        }],
        ForegroundLatencySample {
            baseline_p99_ms: 5.0,
            observed_p99_ms: 5.7,
        },
        ScrubBudget {
            max_added_p99_ms: 1.0,
        },
    );

    assert!(
        repaired.findings.is_empty(),
        "repaired node should rejoin clean"
    );
}
