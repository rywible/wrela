use bytes::Bytes;
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CdcOpKind {
    Put,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CdcEvent {
    pub commit_seq: u64,
    pub shard: Bytes,
    pub key: Bytes,
    pub kind: CdcOpKind,
    pub value: Option<Bytes>,
    pub version: u64,
    pub erasure_proof: Option<CdcErasureProof>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CdcErasureProof {
    pub intent_id: String,
    pub proof_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CdcPage {
    pub events: Vec<CdcEvent>,
    pub next_commit_seq: u64,
    pub high_watermark: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CdcCorrectnessFailureCode {
    NonMonotonicCommitSeq,
    CursorRegression,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CdcGateFailure {
    pub code: CdcCorrectnessFailureCode,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CdcPerfGateInput {
    pub source_events_per_sec: f64,
    pub sink_events_per_sec: f64,
    pub backlog_events: u64,
    pub max_backlog_events: u64,
    pub replay_lag_seconds: u64,
    pub max_replay_lag_seconds: u64,
    pub min_sink_to_source_ratio: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CdcPerfFailureCode {
    ThroughputRegression,
    BacklogTooLarge,
    ReplayLagTooLarge,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CdcPerfFailure {
    pub code: CdcPerfFailureCode,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CdcPerfGateResult {
    pub passed: bool,
    pub failures: Vec<CdcPerfFailure>,
}

#[derive(Debug, Default)]
pub struct CdcEmitter {
    next_commit_seq: u64,
    events: Vec<CdcEvent>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct CdcCheckpointStore {
    checkpoints: HashMap<String, u64>,
}

impl CdcCheckpointStore {
    pub fn ack(&mut self, stream: &str, commit_seq: u64) -> u64 {
        let entry = self.checkpoints.entry(stream.to_string()).or_insert(0);
        if commit_seq > *entry {
            *entry = commit_seq;
        }
        *entry
    }

    pub fn checkpoint(&self, stream: &str) -> Option<u64> {
        self.checkpoints.get(stream).copied()
    }

    pub fn checkpoints(&self) -> &HashMap<String, u64> {
        &self.checkpoints
    }

    pub fn from_checkpoints(checkpoints: HashMap<String, u64>) -> Self {
        Self { checkpoints }
    }
}

impl CdcEmitter {
    fn next_seq(&mut self) -> u64 {
        self.next_commit_seq = self.next_commit_seq.saturating_add(1);
        self.next_commit_seq
    }

    pub fn emit_put(&mut self, shard: Bytes, key: Bytes, value: Bytes, version: u64) {
        let commit_seq = self.next_seq();
        self.events.push(CdcEvent {
            commit_seq,
            shard,
            key,
            kind: CdcOpKind::Put,
            value: Some(value),
            version,
            erasure_proof: None,
        });
    }

    pub fn emit_delete(&mut self, shard: Bytes, key: Bytes, version: u64) {
        let commit_seq = self.next_seq();
        self.events.push(CdcEvent {
            commit_seq,
            shard,
            key,
            kind: CdcOpKind::Delete,
            value: None,
            version,
            erasure_proof: None,
        });
    }

    pub fn events_since(&self, after_commit_seq: u64, limit: usize) -> Vec<CdcEvent> {
        if limit == 0 {
            return Vec::new();
        }
        self.events
            .iter()
            .filter(|event| event.commit_seq > after_commit_seq)
            .take(limit)
            .cloned()
            .collect()
    }

    pub fn page_since(
        &self,
        after_commit_seq: u64,
        limit: usize,
        shard_filter: Option<&[u8]>,
    ) -> CdcPage {
        if limit == 0 {
            return CdcPage {
                events: Vec::new(),
                next_commit_seq: after_commit_seq,
                high_watermark: self.next_commit_seq,
            };
        }

        let events: Vec<CdcEvent> = self
            .events
            .iter()
            .filter(|event| event.commit_seq > after_commit_seq)
            .filter(|event| {
                shard_filter
                    .map(|shard| event.shard.as_ref() == shard)
                    .unwrap_or(true)
            })
            .take(limit)
            .cloned()
            .collect();
        let next_commit_seq = events
            .last()
            .map(|event| event.commit_seq)
            .unwrap_or(after_commit_seq);
        CdcPage {
            events,
            next_commit_seq,
            high_watermark: self.next_commit_seq,
        }
    }
}

pub fn evaluate_cdc_correctness_gate(
    page: &CdcPage,
    after_commit_seq: u64,
) -> Result<(), Vec<CdcGateFailure>> {
    let mut failures = Vec::new();
    let mut last = after_commit_seq;
    for event in &page.events {
        if event.commit_seq <= last {
            failures.push(CdcGateFailure {
                code: CdcCorrectnessFailureCode::NonMonotonicCommitSeq,
                detail: format!("event commit_seq={} after={last}", event.commit_seq),
            });
        }
        last = event.commit_seq;
    }
    if page.next_commit_seq < after_commit_seq {
        failures.push(CdcGateFailure {
            code: CdcCorrectnessFailureCode::CursorRegression,
            detail: format!(
                "next_commit_seq={} after_commit_seq={after_commit_seq}",
                page.next_commit_seq
            ),
        });
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures)
    }
}

pub fn evaluate_cdc_perf_gate(input: &CdcPerfGateInput) -> CdcPerfGateResult {
    let mut failures = Vec::new();
    let ratio = if input.source_events_per_sec <= 0.0 {
        1.0
    } else {
        input.sink_events_per_sec / input.source_events_per_sec
    };
    if ratio < input.min_sink_to_source_ratio {
        failures.push(CdcPerfFailure {
            code: CdcPerfFailureCode::ThroughputRegression,
            detail: format!(
                "sink/source ratio {:.3} below threshold {:.3}",
                ratio, input.min_sink_to_source_ratio
            ),
        });
    }
    if input.backlog_events > input.max_backlog_events {
        failures.push(CdcPerfFailure {
            code: CdcPerfFailureCode::BacklogTooLarge,
            detail: format!(
                "backlog {} exceeds {}",
                input.backlog_events, input.max_backlog_events
            ),
        });
    }
    if input.replay_lag_seconds > input.max_replay_lag_seconds {
        failures.push(CdcPerfFailure {
            code: CdcPerfFailureCode::ReplayLagTooLarge,
            detail: format!(
                "replay lag {}s exceeds {}s",
                input.replay_lag_seconds, input.max_replay_lag_seconds
            ),
        });
    }
    CdcPerfGateResult {
        passed: failures.is_empty(),
        failures,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CdcCheckpointStore, CdcEvent, CdcGateFailure, CdcOpKind, CdcPage, CdcPerfFailureCode,
        CdcPerfGateInput, evaluate_cdc_correctness_gate, evaluate_cdc_perf_gate,
    };
    use bytes::Bytes;

    #[test]
    fn checkpoint_ack_is_monotonic_per_stream() {
        let mut checkpoints = CdcCheckpointStore::default();
        assert_eq!(checkpoints.ack("orders", 5), 5);
        assert_eq!(checkpoints.ack("orders", 3), 5);
        assert_eq!(checkpoints.ack("orders", 9), 9);
        assert_eq!(checkpoints.checkpoint("orders"), Some(9));
    }

    #[test]
    fn checkpoint_isolated_per_stream() {
        let mut checkpoints = CdcCheckpointStore::default();
        checkpoints.ack("orders", 11);
        checkpoints.ack("inventory", 7);
        assert_eq!(checkpoints.checkpoint("orders"), Some(11));
        assert_eq!(checkpoints.checkpoint("inventory"), Some(7));
        assert_eq!(checkpoints.checkpoint("missing"), None);
    }

    #[test]
    fn cdc_correctness_gate_accepts_monotonic_page() {
        let page = CdcPage {
            events: vec![
                CdcEvent {
                    commit_seq: 3,
                    shard: Bytes::from_static(b"core"),
                    key: Bytes::from_static(b"k1"),
                    kind: CdcOpKind::Put,
                    value: Some(Bytes::from_static(b"v1")),
                    version: 30,
                    erasure_proof: None,
                },
                CdcEvent {
                    commit_seq: 4,
                    shard: Bytes::from_static(b"core"),
                    key: Bytes::from_static(b"k2"),
                    kind: CdcOpKind::Put,
                    value: Some(Bytes::from_static(b"v2")),
                    version: 31,
                    erasure_proof: None,
                },
            ],
            next_commit_seq: 4,
            high_watermark: 4,
        };
        assert!(evaluate_cdc_correctness_gate(&page, 2).is_ok());
    }

    #[test]
    fn cdc_correctness_gate_rejects_non_monotonic_page() {
        let page = CdcPage {
            events: vec![
                CdcEvent {
                    commit_seq: 5,
                    shard: Bytes::from_static(b"core"),
                    key: Bytes::from_static(b"k1"),
                    kind: CdcOpKind::Put,
                    value: Some(Bytes::from_static(b"v1")),
                    version: 30,
                    erasure_proof: None,
                },
                CdcEvent {
                    commit_seq: 4,
                    shard: Bytes::from_static(b"core"),
                    key: Bytes::from_static(b"k2"),
                    kind: CdcOpKind::Put,
                    value: Some(Bytes::from_static(b"v2")),
                    version: 31,
                    erasure_proof: None,
                },
            ],
            next_commit_seq: 4,
            high_watermark: 5,
        };
        let err = evaluate_cdc_correctness_gate(&page, 2).expect_err("must fail");
        assert!(!err.is_empty());
        assert!(matches!(
            err.first(),
            Some(CdcGateFailure {
                code: super::CdcCorrectnessFailureCode::NonMonotonicCommitSeq,
                ..
            })
        ));
    }

    #[test]
    fn cdc_perf_gate_fails_on_throughput_backlog_and_replay_lag() {
        let result = evaluate_cdc_perf_gate(&CdcPerfGateInput {
            source_events_per_sec: 20_000.0,
            sink_events_per_sec: 14_000.0,
            backlog_events: 80_000,
            max_backlog_events: 50_000,
            replay_lag_seconds: 160,
            max_replay_lag_seconds: 120,
            min_sink_to_source_ratio: 0.9,
        });
        assert!(!result.passed);
        assert_eq!(result.failures.len(), 3);
        assert_eq!(
            result.failures[0].code,
            CdcPerfFailureCode::ThroughputRegression
        );
        assert_eq!(result.failures[1].code, CdcPerfFailureCode::BacklogTooLarge);
        assert_eq!(
            result.failures[2].code,
            CdcPerfFailureCode::ReplayLagTooLarge
        );
    }

    #[test]
    fn cdc_perf_gate_passes_when_thresholds_hold() {
        let result = evaluate_cdc_perf_gate(&CdcPerfGateInput {
            source_events_per_sec: 20_000.0,
            sink_events_per_sec: 19_500.0,
            backlog_events: 20_000,
            max_backlog_events: 50_000,
            replay_lag_seconds: 50,
            max_replay_lag_seconds: 120,
            min_sink_to_source_ratio: 0.9,
        });
        assert!(result.passed);
        assert!(result.failures.is_empty());
    }
}
