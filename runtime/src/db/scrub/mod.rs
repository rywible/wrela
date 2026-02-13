use crate::db::lsm::sstable::decode_block;
use crate::db::snapshot::manifest::SnapshotManifest;
use crate::db::wal::format::decode_at;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrubTargetKind {
    Wal,
    Sstable,
    Snapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScrubTarget {
    pub kind: ScrubTargetKind,
    pub id: String,
    pub expected_checksum: u64,
    pub observed_checksum: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalArtifact {
    pub id: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SsTableArtifact {
    pub id: String,
    pub encoded_block: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct SnapshotArtifact {
    pub id: String,
    pub manifest: SnapshotManifest,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrubBudget {
    pub max_added_p99_ms: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ForegroundLatencySample {
    pub baseline_p99_ms: f64,
    pub observed_p99_ms: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrubFindingSeverity {
    Warning,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScrubFinding {
    pub target: ScrubTarget,
    pub severity: ScrubFindingSeverity,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepairAction {
    RefetchSnapshot,
    RebuildFollower,
    QuarantineNode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepairStep {
    pub action: RepairAction,
    pub target_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScrubReport {
    pub findings: Vec<ScrubFinding>,
    pub repair_plan: Vec<RepairStep>,
    pub trace_id: String,
}

pub fn within_latency_budget(sample: ForegroundLatencySample, budget: ScrubBudget) -> bool {
    if sample.baseline_p99_ms < 0.0 || sample.observed_p99_ms < 0.0 || budget.max_added_p99_ms < 0.0
    {
        return false;
    }
    (sample.observed_p99_ms - sample.baseline_p99_ms) <= budget.max_added_p99_ms
}

pub fn run_scrub(targets: &[ScrubTarget]) -> ScrubReport {
    let findings: Vec<ScrubFinding> = targets
        .iter()
        .filter(|target| target.expected_checksum != target.observed_checksum)
        .map(|target| ScrubFinding {
            target: target.clone(),
            severity: if matches!(target.kind, ScrubTargetKind::Snapshot) {
                ScrubFindingSeverity::Critical
            } else {
                ScrubFindingSeverity::Warning
            },
            detail: format!(
                "checksum mismatch expected={} observed={}",
                target.expected_checksum, target.observed_checksum
            ),
        })
        .collect();

    build_report(findings)
}

pub fn run_artifact_scrub(
    wal_segments: &[WalArtifact],
    sstables: &[SsTableArtifact],
    snapshots: &[SnapshotArtifact],
    sample: ForegroundLatencySample,
    budget: ScrubBudget,
) -> ScrubReport {
    if !within_latency_budget(sample, budget) {
        let findings = vec![ScrubFinding {
            target: ScrubTarget {
                kind: ScrubTargetKind::Wal,
                id: "scrubber".to_string(),
                expected_checksum: 0,
                observed_checksum: 1,
            },
            severity: ScrubFindingSeverity::Warning,
            detail: format!(
                "scrub deferred: latency budget exceeded baseline_p99_ms={} observed_p99_ms={} max_added_p99_ms={}",
                sample.baseline_p99_ms, sample.observed_p99_ms, budget.max_added_p99_ms
            ),
        }];
        return build_report(findings);
    }

    let mut findings = Vec::new();

    for wal in wal_segments {
        if let Some(finding) = inspect_wal(wal) {
            findings.push(finding);
        }
    }

    for table in sstables {
        if let Some(finding) = inspect_sstable(table) {
            findings.push(finding);
        }
    }

    for snapshot in snapshots {
        if let Some(finding) = inspect_snapshot(snapshot) {
            findings.push(finding);
        }
    }

    build_report(findings)
}

fn build_report(findings: Vec<ScrubFinding>) -> ScrubReport {
    let mut repair_plan: Vec<RepairStep> = findings
        .iter()
        .map(|finding| RepairStep {
            action: match finding.target.kind {
                ScrubTargetKind::Wal => RepairAction::RebuildFollower,
                ScrubTargetKind::Sstable => RepairAction::RebuildFollower,
                ScrubTargetKind::Snapshot => RepairAction::RefetchSnapshot,
            },
            target_id: finding.target.id.clone(),
            reason: finding.detail.clone(),
        })
        .collect();

    if findings
        .iter()
        .any(|finding| finding.severity == ScrubFindingSeverity::Critical)
    {
        repair_plan.push(RepairStep {
            action: RepairAction::QuarantineNode,
            target_id: "local-node".to_string(),
            reason: "critical corruption detected".to_string(),
        });
    }

    let trace_id = build_trace_id(&findings);
    ScrubReport {
        findings,
        repair_plan,
        trace_id,
    }
}

fn build_trace_id(findings: &[ScrubFinding]) -> String {
    let mut acc: u64 = 0xcbf2_9ce4_8422_2325;
    for finding in findings {
        for byte in finding.target.id.as_bytes() {
            acc ^= *byte as u64;
            acc = acc.wrapping_mul(0x0000_0001_0000_01b3);
        }
        acc ^= finding.target.kind as u64;
        acc = acc.wrapping_mul(0x0000_0001_0000_01b3);
        for byte in finding.detail.as_bytes() {
            acc ^= *byte as u64;
            acc = acc.wrapping_mul(0x0000_0001_0000_01b3);
        }
    }
    format!("scrub-{acc:016x}")
}

fn inspect_wal(wal: &WalArtifact) -> Option<ScrubFinding> {
    let mut offset = 0usize;
    while offset < wal.bytes.len() {
        match decode_at(&wal.bytes, offset) {
            Ok(Some((_record, next))) if next > offset => offset = next,
            Ok(Some((_record, _next))) => {
                return Some(ScrubFinding {
                    target: ScrubTarget {
                        kind: ScrubTargetKind::Wal,
                        id: wal.id.clone(),
                        expected_checksum: 1,
                        observed_checksum: 0,
                    },
                    severity: ScrubFindingSeverity::Warning,
                    detail: "WAL decode produced non-forward progress".to_string(),
                });
            }
            Ok(None) => {
                return Some(ScrubFinding {
                    target: ScrubTarget {
                        kind: ScrubTargetKind::Wal,
                        id: wal.id.clone(),
                        expected_checksum: 1,
                        observed_checksum: 0,
                    },
                    severity: ScrubFindingSeverity::Warning,
                    detail: "WAL truncated or malformed frame".to_string(),
                });
            }
            Err(err) => {
                return Some(ScrubFinding {
                    target: ScrubTarget {
                        kind: ScrubTargetKind::Wal,
                        id: wal.id.clone(),
                        expected_checksum: 1,
                        observed_checksum: 0,
                    },
                    severity: ScrubFindingSeverity::Warning,
                    detail: format!("WAL decode failed: {err}"),
                });
            }
        }
    }
    None
}

fn inspect_sstable(table: &SsTableArtifact) -> Option<ScrubFinding> {
    if decode_block(&table.encoded_block).is_ok() {
        return None;
    }

    Some(ScrubFinding {
        target: ScrubTarget {
            kind: ScrubTargetKind::Sstable,
            id: table.id.clone(),
            expected_checksum: 1,
            observed_checksum: 0,
        },
        severity: ScrubFindingSeverity::Warning,
        detail: "SST decode failed".to_string(),
    })
}

fn inspect_snapshot(snapshot: &SnapshotArtifact) -> Option<ScrubFinding> {
    if snapshot
        .manifest
        .validate_payload(&snapshot.payload)
        .is_ok()
    {
        return None;
    }

    Some(ScrubFinding {
        target: ScrubTarget {
            kind: ScrubTargetKind::Snapshot,
            id: snapshot.id.clone(),
            expected_checksum: snapshot.manifest.checksum,
            observed_checksum: 0,
        },
        severity: ScrubFindingSeverity::Critical,
        detail: "snapshot manifest validation failed".to_string(),
    })
}

pub fn node_quarantine_required(report: &ScrubReport) -> bool {
    report
        .findings
        .iter()
        .any(|finding| finding.severity == ScrubFindingSeverity::Critical)
}

#[cfg(test)]
mod tests {
    use super::{
        ForegroundLatencySample, RepairAction, ScrubBudget, ScrubFindingSeverity, ScrubTarget,
        ScrubTargetKind, SsTableArtifact, WalArtifact, node_quarantine_required,
        run_artifact_scrub, run_scrub, within_latency_budget,
    };
    use crate::db::lsm::sstable::{SsTableEntry, encode_block};
    use crate::db::snapshot::builder::build_manifest;
    use crate::db::wal::format::{Record, RecordKind, encode};

    #[test]
    fn scrub_detects_corruption_and_builds_repair_plan() {
        let report = run_scrub(&[
            ScrubTarget {
                kind: ScrubTargetKind::Wal,
                id: "wal-1".to_string(),
                expected_checksum: 10,
                observed_checksum: 11,
            },
            ScrubTarget {
                kind: ScrubTargetKind::Snapshot,
                id: "snap-1".to_string(),
                expected_checksum: 20,
                observed_checksum: 21,
            },
        ]);

        assert_eq!(report.findings.len(), 2);
        assert_eq!(report.repair_plan[0].action, RepairAction::RebuildFollower);
        assert_eq!(report.repair_plan[1].action, RepairAction::RefetchSnapshot);
        assert_eq!(report.repair_plan[2].action, RepairAction::QuarantineNode);
        assert!(node_quarantine_required(&report));
        assert!(report.trace_id.starts_with("scrub-"));
    }

    #[test]
    fn artifact_scrub_detects_wal_sst_and_snapshot_corruption() {
        let valid_wal = encode(&Record {
            kind: RecordKind::Put,
            namespace: b"core".to_vec(),
            key: b"k1".to_vec(),
            value: b"v1".to_vec(),
            version: 1,
        });
        let mut corrupt_wal = valid_wal.clone();
        corrupt_wal[5] ^= 0xff;

        let valid_sst = encode_block(&[SsTableEntry::live(b"a".to_vec(), 1, b"v".to_vec(), None)]);
        let mut corrupt_sst = valid_sst.clone();
        corrupt_sst.truncate(corrupt_sst.len().saturating_sub(2));

        let snapshot_payload = b"snapshot-ok".to_vec();
        let mut bad_snapshot_payload = snapshot_payload.clone();
        bad_snapshot_payload.push(7);
        let manifest = build_manifest(&snapshot_payload, 10, 3);

        let report = run_artifact_scrub(
            &[WalArtifact {
                id: "wal-1".to_string(),
                bytes: corrupt_wal,
            }],
            &[SsTableArtifact {
                id: "sst-1".to_string(),
                encoded_block: corrupt_sst,
            }],
            &[super::SnapshotArtifact {
                id: "snap-1".to_string(),
                manifest,
                payload: bad_snapshot_payload,
            }],
            ForegroundLatencySample {
                baseline_p99_ms: 4.0,
                observed_p99_ms: 4.8,
            },
            ScrubBudget {
                max_added_p99_ms: 1.0,
            },
        );

        assert_eq!(report.findings.len(), 3);
        assert_eq!(report.findings[0].target.kind, ScrubTargetKind::Wal);
        assert_eq!(report.findings[1].target.kind, ScrubTargetKind::Sstable);
        assert_eq!(report.findings[2].severity, ScrubFindingSeverity::Critical);
        assert!(
            report
                .repair_plan
                .iter()
                .any(|step| step.action == RepairAction::QuarantineNode)
        );
    }

    #[test]
    fn artifact_scrub_defers_when_latency_budget_exceeded() {
        let report = run_artifact_scrub(
            &[],
            &[],
            &[],
            ForegroundLatencySample {
                baseline_p99_ms: 2.0,
                observed_p99_ms: 4.5,
            },
            ScrubBudget {
                max_added_p99_ms: 1.0,
            },
        );
        assert_eq!(report.findings.len(), 1);
        assert!(report.findings[0].detail.contains("scrub deferred"));
    }

    #[test]
    fn latency_budget_math_is_fail_closed() {
        assert!(within_latency_budget(
            ForegroundLatencySample {
                baseline_p99_ms: 2.0,
                observed_p99_ms: 2.5
            },
            ScrubBudget {
                max_added_p99_ms: 0.7
            }
        ));
        assert!(!within_latency_budget(
            ForegroundLatencySample {
                baseline_p99_ms: 2.0,
                observed_p99_ms: 2.9
            },
            ScrubBudget {
                max_added_p99_ms: 0.7
            }
        ));
        assert!(!within_latency_budget(
            ForegroundLatencySample {
                baseline_p99_ms: -1.0,
                observed_p99_ms: 2.0
            },
            ScrubBudget {
                max_added_p99_ms: 0.5
            }
        ));
    }
}
