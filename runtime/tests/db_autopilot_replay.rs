use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use wrela_runtime::db::autopilot::{
    DEFAULT_MAX_SKEW_RATIO, SafetySimulationInput, evaluate_safety_simulation,
};
use wrela_runtime::db::routing::health::{HealthState, NodeHealth};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct ReplayScenarioReport {
    scenario_id: String,
    passed: bool,
    reasons: Vec<String>,
    timeline: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct ReplayReport {
    generated_at_epoch_ms: u64,
    all_passed: bool,
    scenarios: Vec<ReplayScenarioReport>,
}

fn run_replay() -> ReplayReport {
    let scenarios = vec![
        (
            "balanced-traffic",
            BTreeMap::from([
                ("shard-a".to_string(), 1200_u64),
                ("shard-b".to_string(), 1100_u64),
                ("shard-c".to_string(), 1000_u64),
            ]),
            vec![
                NodeHealth {
                    node_id: "n1".to_string(),
                    region: "us".to_string(),
                    state: HealthState::Healthy,
                    observed_at_ms: 1,
                },
                NodeHealth {
                    node_id: "n2".to_string(),
                    region: "eu".to_string(),
                    state: HealthState::Healthy,
                    observed_at_ms: 1,
                },
                NodeHealth {
                    node_id: "n3".to_string(),
                    region: "ap".to_string(),
                    state: HealthState::Degraded,
                    observed_at_ms: 1,
                },
            ],
            BTreeMap::from([
                ("n1".to_string(), 4_u64),
                ("n2".to_string(), 7_u64),
                ("n3".to_string(), 12_u64),
            ]),
            1_usize,
            1_usize,
        ),
        (
            "hotspot-and-degraded",
            BTreeMap::from([
                ("shard-a".to_string(), 5300_u64),
                ("shard-b".to_string(), 400_u64),
                ("shard-c".to_string(), 350_u64),
            ]),
            vec![
                NodeHealth {
                    node_id: "n1".to_string(),
                    region: "us".to_string(),
                    state: HealthState::Healthy,
                    observed_at_ms: 1,
                },
                NodeHealth {
                    node_id: "n2".to_string(),
                    region: "eu".to_string(),
                    state: HealthState::Degraded,
                    observed_at_ms: 1,
                },
                NodeHealth {
                    node_id: "n3".to_string(),
                    region: "ap".to_string(),
                    state: HealthState::Unavailable,
                    observed_at_ms: 1,
                },
            ],
            BTreeMap::from([("n1".to_string(), 5_u64), ("n2".to_string(), 8_u64)]),
            1_usize,
            0_usize,
        ),
    ];

    let mut reports = Vec::new();
    for (
        scenario_id,
        shard_loads,
        quorum_candidates,
        latency_hint_ms,
        required_additional_failures,
        max_degraded_selected,
    ) in scenarios
    {
        let decision = evaluate_safety_simulation(SafetySimulationInput {
            shard_loads: &shard_loads,
            skew_threshold: DEFAULT_MAX_SKEW_RATIO,
            quorum_candidates: &quorum_candidates,
            desired_voters: 3,
            latency_hint_ms: &latency_hint_ms,
            required_additional_failures,
            max_degraded_selected,
        })
        .expect("deterministic replay scenario");

        let mut timeline = Vec::new();
        timeline.push(format!("scenario={scenario_id}"));
        timeline.push(format!("passes={}", decision.passes));
        timeline.push(format!("hottest_shard={}", decision.skew.hottest_shard));
        timeline.push(format!(
            "max_to_mean_ratio={:.6}",
            decision.skew.max_to_mean_ratio
        ));
        timeline.extend(decision.quorum.timeline);

        reports.push(ReplayScenarioReport {
            scenario_id: scenario_id.to_string(),
            passed: decision.passes,
            reasons: decision.reasons,
            timeline,
        });
    }

    ReplayReport {
        generated_at_epoch_ms: 0,
        all_passed: reports.iter().all(|row| row.passed),
        scenarios: reports,
    }
}

fn temp_path(prefix: &str) -> std::path::PathBuf {
    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "{}_{}_{}_{}.json",
        prefix,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("epoch")
            .as_nanos(),
        id
    ))
}

#[test]
fn autopilot_replay_is_deterministic_and_machine_readable() {
    let first = run_replay();
    let second = run_replay();
    assert_eq!(first, second);

    let encoded = serde_json::to_string_pretty(&first).expect("json");
    let decoded: ReplayReport = serde_json::from_str(&encoded).expect("parse report");
    assert_eq!(decoded, first);
    assert_eq!(decoded.scenarios.len(), 2);
}

#[test]
fn autopilot_replay_includes_invariant_breach_details() {
    let report = run_replay();
    let failing = report
        .scenarios
        .iter()
        .find(|row| row.scenario_id == "hotspot-and-degraded")
        .expect("failing scenario present");
    assert!(!failing.passed);
    assert!(
        failing
            .reasons
            .iter()
            .any(|reason| reason.contains("skew ratio"))
    );
    assert!(
        failing
            .reasons
            .iter()
            .any(|reason| reason.contains("degraded selected"))
    );
}

#[test]
fn autopilot_replay_writes_artifact_for_ci_collection() {
    let report = run_replay();
    let out = temp_path("wrela_autopilot_replay");
    std::fs::write(
        &out,
        serde_json::to_vec_pretty(&report).expect("encode replay report"),
    )
    .expect("write report");

    let payload = std::fs::read_to_string(&out).expect("read report");
    let parsed: ReplayReport = serde_json::from_str(&payload).expect("parse report");
    assert_eq!(parsed.scenarios.len(), 2);
    assert_eq!(parsed.generated_at_epoch_ms, 0);

    std::fs::remove_file(out).expect("cleanup report file");
}
