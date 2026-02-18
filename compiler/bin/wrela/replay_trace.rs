use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const REPLAY_TRACE_SCHEMA_VERSION: u32 = 1;

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
    let payload = fs::read(path)
        .map_err(|err| format!("failed to read replay artifact {}: {}", path.display(), err))?;
    let artifact: ReplayTraceArtifact = serde_json::from_slice(&payload)
        .map_err(|err| format!("invalid replay artifact {}: {}", path.display(), err))?;
    replay_signature(&artifact)
}

pub fn replay_signature(artifact: &ReplayTraceArtifact) -> Result<String, String> {
    let mut expected_seq = 0u64;
    let mut expected_step = 0u64;
    let mut out = String::new();
    out.push_str(&format!(
        "v={}|lane={}|seed={}|test={}|",
        artifact.version, artifact.lane, artifact.seed, artifact.canonical_test_id
    ));
    for event in &artifact.events {
        if event.seq != expected_seq {
            return Err(format!(
                "non-deterministic event sequence: got {}, expected {}",
                event.seq, expected_seq
            ));
        }
        if event.timing.logical_step != expected_step {
            return Err(format!(
                "non-deterministic logical step: got {}, expected {}",
                event.timing.logical_step, expected_step
            ));
        }
        if event.route.lane != artifact.lane {
            return Err(format!(
                "route lane mismatch: event lane '{}' != artifact lane '{}'",
                event.route.lane, artifact.lane
            ));
        }
        if event.route.scheduler_seed != artifact.seed {
            return Err(format!(
                "route seed mismatch: event seed {} != artifact seed {}",
                event.route.scheduler_seed, artifact.seed
            ));
        }
        if let Some(fault) = &event.fault
            && fault.seed != artifact.seed
        {
            return Err(format!(
                "fault seed mismatch: fault seed {} != artifact seed {}",
                fault.seed, artifact.seed
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
        let err = replay_signature(&artifact).expect_err("must reject sequence drift");
        assert!(err.contains("non-deterministic event sequence"));
    }
}
