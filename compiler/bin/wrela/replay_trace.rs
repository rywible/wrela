//! Owns replay-trace artifact schema types plus validation helpers used by the
//! certification/debugging command surfaces.
//! Does not own CLI parsing or certification verdict policy.
//!
//! Key invariants:
//! - replay validation reports drift against the recorded artifact, not against
//!   ad hoc runtime state.
//! - schema versioning stays explicit so older traces fail clearly instead of
//!   being misinterpreted.
//! - mismatch kinds remain stable because downstream reports treat them as
//!   machine-readable evidence.
//!
//! Primary entrypoints:
//! - `validate_replay_trace`
//! - `write_replay_trace`
//! - `load_replay_trace`
//!
//! Failure modes / common pitfalls:
//! - collapsing distinct mismatch cases into one generic error makes replay
//!   regressions much harder to triage.
//! - silently accepting schema drift would undermine trace portability.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const REPLAY_TRACE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReplayMismatchKind {
    SchemaDrift,
    RouteDrift,
    SeedDrift,
    OperationOutcomeDrift,
    OrderingDrift,
    TimestampMonotonicityDrift,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplayTraceValidationError {
    pub kind: ReplayMismatchKind,
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_seq: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual: Option<String>,
}

impl ReplayTraceValidationError {
    fn new(
        kind: ReplayMismatchKind,
        code: &str,
        message: String,
        event_seq: Option<u64>,
        expected: Option<String>,
        actual: Option<String>,
    ) -> Self {
        Self {
            kind,
            code: code.to_string(),
            message,
            event_seq,
            expected,
            actual,
        }
    }

    pub fn data(&self) -> serde_json::Value {
        serde_json::json!({
            "kind": self.kind,
            "event_seq": self.event_seq,
            "expected": self.expected,
            "actual": self.actual,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplayTraceArtifact {
    pub version: u32,
    pub generated_at_unix_ms: u128,
    pub test_id: String,
    pub canonical_test_id: String,
    pub lane: String,
    pub seed: u64,
    pub failure: String,
    pub events: Vec<ReplayTraceEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplayTraceEvent {
    pub seq: u64,
    pub operation: TraceOperation,
    pub route: TraceRoute,
    pub timing: TraceTiming,
    pub fault: Option<TraceFault>,
    pub outcome: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TraceOperation {
    pub phase: String,
    pub action: String,
    pub commit_state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TraceRoute {
    pub lane: String,
    pub scheduler_seed: u64,
    pub target: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TraceTiming {
    pub logical_step: u64,
    pub observed_unix_ms: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TraceFault {
    pub kind: String,
    pub source: String,
    pub seed: u64,
    pub detail: String,
}

#[derive(Debug, Clone)]
pub struct ReplayTraceInput<'a> {
    pub test_id: &'a str,
    pub canonical_test_id: &'a str,
    pub lane: &'a str,
    pub seed: u64,
    pub failure: &'a str,
}

fn build_failure_trace(
    generated_at_unix_ms: u128,
    input: &ReplayTraceInput<'_>,
) -> ReplayTraceArtifact {
    ReplayTraceArtifact {
        version: REPLAY_TRACE_SCHEMA_VERSION,
        generated_at_unix_ms,
        test_id: input.test_id.to_string(),
        canonical_test_id: input.canonical_test_id.to_string(),
        lane: input.lane.to_string(),
        seed: input.seed,
        failure: input.failure.to_string(),
        events: vec![
            ReplayTraceEvent {
                seq: 0,
                operation: TraceOperation {
                    phase: "dispatch".to_string(),
                    action: "start".to_string(),
                    commit_state: "pre-commit".to_string(),
                },
                route: TraceRoute {
                    lane: input.lane.to_string(),
                    scheduler_seed: input.seed,
                    target: input.canonical_test_id.to_string(),
                },
                timing: TraceTiming {
                    logical_step: 0,
                    observed_unix_ms: generated_at_unix_ms,
                },
                fault: None,
                outcome: "started".to_string(),
            },
            ReplayTraceEvent {
                seq: 1,
                operation: TraceOperation {
                    phase: "dispatch".to_string(),
                    action: "commit".to_string(),
                    commit_state: "failed".to_string(),
                },
                route: TraceRoute {
                    lane: input.lane.to_string(),
                    scheduler_seed: input.seed,
                    target: input.canonical_test_id.to_string(),
                },
                timing: TraceTiming {
                    logical_step: 1,
                    observed_unix_ms: generated_at_unix_ms,
                },
                fault: Some(TraceFault {
                    kind: "injected_failure".to_string(),
                    source: "lane_runtime".to_string(),
                    seed: input.seed,
                    detail: input.failure.to_string(),
                }),
                outcome: "failed".to_string(),
            },
        ],
    }
}

pub fn write_failure_trace_artifact(
    workspace_root: &Path,
    lane_dir: &str,
    sanitized_canonical_id: &str,
    generated_at_unix_ms: u128,
    input: &ReplayTraceInput<'_>,
) -> Result<PathBuf, String> {
    let artifact_dir = workspace_root
        .join("tests")
        .join(".artifacts")
        .join(lane_dir)
        .join(sanitized_canonical_id);
    fs::create_dir_all(&artifact_dir).map_err(|err| {
        format!(
            "failed to create replay trace artifact directory {}: {}",
            artifact_dir.display(),
            err
        )
    })?;
    let artifact_path = artifact_dir.join(format!("{}.json", input.seed));
    let payload = serde_json::to_vec_pretty(&build_failure_trace(generated_at_unix_ms, input))
        .map_err(|err| err.to_string())?;
    fs::write(&artifact_path, payload).map_err(|err| {
        format!(
            "failed to write replay trace artifact {}: {}",
            artifact_path.display(),
            err
        )
    })?;
    Ok(artifact_path)
}

#[allow(dead_code)]
pub fn replay_signature_from_artifact(path: &Path) -> Result<String, String> {
    replay_signature_from_artifact_typed(path).map_err(|err| err.message)
}

pub fn replay_signature_from_artifact_typed(
    path: &Path,
) -> Result<String, ReplayTraceValidationError> {
    let payload = fs::read(path).map_err(|err| {
        ReplayTraceValidationError::new(
            ReplayMismatchKind::SchemaDrift,
            "lang::runtime::replay_schema_drift",
            format!("failed to read replay artifact {}: {}", path.display(), err),
            None,
            None,
            None,
        )
    })?;
    let artifact: ReplayTraceArtifact = serde_json::from_slice(&payload).map_err(|err| {
        ReplayTraceValidationError::new(
            ReplayMismatchKind::SchemaDrift,
            "lang::runtime::replay_schema_drift",
            format!("invalid replay artifact {}: {}", path.display(), err),
            None,
            None,
            None,
        )
    })?;
    replay_signature_typed(&artifact)
}

#[allow(dead_code)]
pub fn replay_signature(artifact: &ReplayTraceArtifact) -> Result<String, String> {
    replay_signature_typed(artifact).map_err(|err| err.message)
}

pub fn replay_signature_typed(
    artifact: &ReplayTraceArtifact,
) -> Result<String, ReplayTraceValidationError> {
    if artifact.version != REPLAY_TRACE_SCHEMA_VERSION {
        return Err(ReplayTraceValidationError::new(
            ReplayMismatchKind::SchemaDrift,
            "lang::runtime::replay_schema_drift",
            format!(
                "unsupported replay trace schema version: got {}, expected {}",
                artifact.version, REPLAY_TRACE_SCHEMA_VERSION
            ),
            None,
            Some(REPLAY_TRACE_SCHEMA_VERSION.to_string()),
            Some(artifact.version.to_string()),
        ));
    }
    if artifact.lane.trim().is_empty() {
        return Err(ReplayTraceValidationError::new(
            ReplayMismatchKind::RouteDrift,
            "lang::runtime::replay_route_drift",
            "replay trace lane must be non-empty".to_string(),
            None,
            Some("non-empty lane".to_string()),
            Some(artifact.lane.clone()),
        ));
    }
    if artifact.canonical_test_id.trim().is_empty() {
        return Err(ReplayTraceValidationError::new(
            ReplayMismatchKind::RouteDrift,
            "lang::runtime::replay_route_drift",
            "replay trace canonical_test_id must be non-empty".to_string(),
            None,
            Some("non-empty canonical_test_id".to_string()),
            Some(artifact.canonical_test_id.clone()),
        ));
    }
    if artifact.events.is_empty() {
        return Err(ReplayTraceValidationError::new(
            ReplayMismatchKind::OrderingDrift,
            "lang::runtime::replay_ordering_drift",
            "replay trace contains no events".to_string(),
            None,
            Some("at least one event".to_string()),
            Some("0".to_string()),
        ));
    }
    let mut expected_seq = 0u64;
    let mut expected_step = 0u64;
    let mut prev_observed_unix_ms: Option<u128> = None;
    let mut out = String::new();
    out.push_str(&format!(
        "v={}|lane={}|seed={}|test={}|",
        artifact.version, artifact.lane, artifact.seed, artifact.canonical_test_id
    ));
    for event in &artifact.events {
        if event.seq != expected_seq {
            return Err(ReplayTraceValidationError::new(
                ReplayMismatchKind::OrderingDrift,
                "lang::runtime::replay_ordering_drift",
                format!(
                    "non-deterministic event sequence: got {}, expected {}",
                    event.seq, expected_seq
                ),
                Some(event.seq),
                Some(expected_seq.to_string()),
                Some(event.seq.to_string()),
            ));
        }
        if event.timing.logical_step != expected_step {
            return Err(ReplayTraceValidationError::new(
                ReplayMismatchKind::OrderingDrift,
                "lang::runtime::replay_ordering_drift",
                format!(
                    "non-deterministic logical step: got {}, expected {}",
                    event.timing.logical_step, expected_step
                ),
                Some(event.seq),
                Some(expected_step.to_string()),
                Some(event.timing.logical_step.to_string()),
            ));
        }
        if event.route.lane != artifact.lane {
            return Err(ReplayTraceValidationError::new(
                ReplayMismatchKind::RouteDrift,
                "lang::runtime::replay_route_drift",
                format!(
                    "route lane mismatch: event lane '{}' != artifact lane '{}'",
                    event.route.lane, artifact.lane
                ),
                Some(event.seq),
                Some(artifact.lane.clone()),
                Some(event.route.lane.clone()),
            ));
        }
        if event.route.scheduler_seed != artifact.seed {
            return Err(ReplayTraceValidationError::new(
                ReplayMismatchKind::SeedDrift,
                "lang::runtime::replay_seed_drift",
                format!(
                    "route seed mismatch: event seed {} != artifact seed {}",
                    event.route.scheduler_seed, artifact.seed
                ),
                Some(event.seq),
                Some(artifact.seed.to_string()),
                Some(event.route.scheduler_seed.to_string()),
            ));
        }
        if event.route.target != artifact.canonical_test_id {
            return Err(ReplayTraceValidationError::new(
                ReplayMismatchKind::RouteDrift,
                "lang::runtime::replay_route_drift",
                format!(
                    "route target mismatch: event target '{}' != canonical test '{}'",
                    event.route.target, artifact.canonical_test_id
                ),
                Some(event.seq),
                Some(artifact.canonical_test_id.clone()),
                Some(event.route.target.clone()),
            ));
        }
        if let Some(prev) = prev_observed_unix_ms
            && event.timing.observed_unix_ms < prev
        {
            return Err(ReplayTraceValidationError::new(
                ReplayMismatchKind::TimestampMonotonicityDrift,
                "lang::runtime::replay_timestamp_monotonicity_drift",
                format!(
                    "non-monotonic observed time: got {}, previous {}",
                    event.timing.observed_unix_ms, prev
                ),
                Some(event.seq),
                Some(format!(">= {prev}")),
                Some(event.timing.observed_unix_ms.to_string()),
            ));
        }
        if event.operation.phase.trim().is_empty()
            || event.operation.action.trim().is_empty()
            || event.operation.commit_state.trim().is_empty()
            || event.outcome.trim().is_empty()
        {
            return Err(ReplayTraceValidationError::new(
                ReplayMismatchKind::OperationOutcomeDrift,
                "lang::runtime::replay_operation_outcome_drift",
                "invalid replay event: operation/outcome must be non-empty".to_string(),
                Some(event.seq),
                Some("non-empty phase/action/commit_state/outcome".to_string()),
                None,
            ));
        }
        if let Some(fault) = &event.fault
            && fault.seed != artifact.seed
        {
            return Err(ReplayTraceValidationError::new(
                ReplayMismatchKind::SeedDrift,
                "lang::runtime::replay_seed_drift",
                format!(
                    "fault seed mismatch: fault seed {} != artifact seed {}",
                    fault.seed, artifact.seed
                ),
                Some(event.seq),
                Some(artifact.seed.to_string()),
                Some(fault.seed.to_string()),
            ));
        }
        out.push_str(&format!(
            "#{}:{}:{}:{}:{}|",
            event.seq,
            event.operation.phase,
            event.operation.action,
            event.operation.commit_state,
            event.outcome
        ));
        expected_seq = expected_seq.saturating_add(1);
        expected_step = expected_step.saturating_add(1);
        prev_observed_unix_ms = Some(event.timing.observed_unix_ms);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replay_trace_round_trip_is_stable() {
        let input = ReplayTraceInput {
            test_id: "tests/sim/demo::test_demo",
            canonical_test_id: "tests/sim/demo::test_demo",
            lane: "sim",
            seed: 7,
            failure: "assertion failed",
        };
        let artifact = build_failure_trace(123, &input);
        let first = serde_json::to_string_pretty(&artifact).expect("serialize trace");
        let second = serde_json::to_string_pretty(&artifact).expect("serialize trace");
        assert_eq!(first, second, "serialization must be deterministic");
        let decoded: ReplayTraceArtifact = serde_json::from_str(&first).expect("deserialize trace");
        assert_eq!(decoded.version, REPLAY_TRACE_SCHEMA_VERSION);
        assert_eq!(decoded.events.len(), 2);
        assert_eq!(decoded.events[0].timing.logical_step, 0);
        assert_eq!(decoded.events[1].timing.logical_step, 1);
        assert_eq!(decoded.events[1].fault.as_ref().map(|f| f.seed), Some(7));
    }

    #[test]
    fn replay_signature_is_stable_and_rejects_sequence_drift() {
        let input = ReplayTraceInput {
            test_id: "tests/sim/demo::test_demo",
            canonical_test_id: "tests/sim/demo::test_demo",
            lane: "sim",
            seed: 9,
            failure: "assertion failed",
        };
        let mut artifact = build_failure_trace(321, &input);
        let sig_a = replay_signature(&artifact).expect("signature a");
        let sig_b = replay_signature(&artifact).expect("signature b");
        assert_eq!(sig_a, sig_b, "signature must be deterministic");

        artifact.events[1].seq = 3;
        let err = replay_signature_typed(&artifact).expect_err("must reject sequence drift");
        assert_eq!(err.kind, ReplayMismatchKind::OrderingDrift);
        assert_eq!(err.code, "lang::runtime::replay_ordering_drift");
        assert!(err.message.contains("non-deterministic event sequence"));
    }

    #[test]
    fn replay_signature_rejects_route_target_drift() {
        let input = ReplayTraceInput {
            test_id: "tests/sim/demo::test_demo",
            canonical_test_id: "tests/sim/demo::test_demo",
            lane: "sim",
            seed: 10,
            failure: "assertion failed",
        };
        let mut artifact = build_failure_trace(444, &input);
        artifact.events[0].route.target = "tests/sim/other::test_other".to_string();
        let err = replay_signature_typed(&artifact).expect_err("must reject target drift");
        assert_eq!(err.kind, ReplayMismatchKind::RouteDrift);
        assert_eq!(err.code, "lang::runtime::replay_route_drift");
        assert!(err.message.contains("route target mismatch"));
    }

    #[test]
    fn replay_signature_rejects_schema_version_drift() {
        let input = ReplayTraceInput {
            test_id: "tests/sim/demo::test_demo",
            canonical_test_id: "tests/sim/demo::test_demo",
            lane: "sim",
            seed: 11,
            failure: "assertion failed",
        };
        let mut artifact = build_failure_trace(555, &input);
        artifact.version = REPLAY_TRACE_SCHEMA_VERSION + 1;
        let err = replay_signature_typed(&artifact).expect_err("must reject schema drift");
        assert_eq!(err.kind, ReplayMismatchKind::SchemaDrift);
        assert_eq!(err.code, "lang::runtime::replay_schema_drift");
        assert!(
            err.message
                .contains("unsupported replay trace schema version")
        );
    }

    #[test]
    fn replay_signature_rejects_non_monotonic_observed_time() {
        let input = ReplayTraceInput {
            test_id: "tests/sim/demo::test_demo",
            canonical_test_id: "tests/sim/demo::test_demo",
            lane: "sim",
            seed: 12,
            failure: "assertion failed",
        };
        let mut artifact = build_failure_trace(666, &input);
        artifact.events[1].timing.observed_unix_ms = 100;
        artifact.events[0].timing.observed_unix_ms = 200;
        let err = replay_signature_typed(&artifact).expect_err("must reject time regression");
        assert_eq!(err.kind, ReplayMismatchKind::TimestampMonotonicityDrift);
        assert_eq!(
            err.code,
            "lang::runtime::replay_timestamp_monotonicity_drift"
        );
        assert!(err.message.contains("non-monotonic observed time"));
    }

    #[test]
    fn replay_signature_rejects_empty_events() {
        let input = ReplayTraceInput {
            test_id: "tests/sim/demo::test_demo",
            canonical_test_id: "tests/sim/demo::test_demo",
            lane: "sim",
            seed: 13,
            failure: "assertion failed",
        };
        let mut artifact = build_failure_trace(777, &input);
        artifact.events.clear();
        let err = replay_signature_typed(&artifact).expect_err("must reject empty trace");
        assert_eq!(err.kind, ReplayMismatchKind::OrderingDrift);
        assert_eq!(err.code, "lang::runtime::replay_ordering_drift");
        assert!(err.message.contains("contains no events"));
    }

    #[test]
    fn replay_signature_rejects_empty_operation_or_outcome() {
        let input = ReplayTraceInput {
            test_id: "tests/sim/demo::test_demo",
            canonical_test_id: "tests/sim/demo::test_demo",
            lane: "sim",
            seed: 14,
            failure: "assertion failed",
        };
        let mut artifact = build_failure_trace(888, &input);
        artifact.events[0].operation.phase = "  ".to_string();
        let err =
            replay_signature_typed(&artifact).expect_err("must reject empty operation fields");
        assert_eq!(err.kind, ReplayMismatchKind::OperationOutcomeDrift);
        assert_eq!(err.code, "lang::runtime::replay_operation_outcome_drift");
        assert!(err.message.contains("operation/outcome must be non-empty"));

        artifact.events[0].operation.phase = "dispatch".to_string();
        artifact.events[1].outcome = " ".to_string();
        let err = replay_signature_typed(&artifact).expect_err("must reject empty outcome");
        assert_eq!(err.kind, ReplayMismatchKind::OperationOutcomeDrift);
        assert_eq!(err.code, "lang::runtime::replay_operation_outcome_drift");
        assert!(err.message.contains("operation/outcome must be non-empty"));
    }

    #[test]
    fn replay_signature_rejects_fault_seed_mismatch() {
        let input = ReplayTraceInput {
            test_id: "tests/sim/demo::test_demo",
            canonical_test_id: "tests/sim/demo::test_demo",
            lane: "sim",
            seed: 15,
            failure: "assertion failed",
        };
        let mut artifact = build_failure_trace(999, &input);
        artifact.events[1].fault.as_mut().expect("fault").seed = 22;
        let err = replay_signature_typed(&artifact).expect_err("must reject fault seed drift");
        assert_eq!(err.kind, ReplayMismatchKind::SeedDrift);
        assert_eq!(err.code, "lang::runtime::replay_seed_drift");
        assert!(err.message.contains("fault seed mismatch"));
    }
}
