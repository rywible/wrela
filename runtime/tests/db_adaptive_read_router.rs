use wrela_runtime::db::routing::read_router::{
    ReadConsistencyMode, ReadRouteCandidate, choose_read_region,
};
use wrela_runtime::db::routing::telemetry::{ReadSample, ReadTelemetryStore};

#[test]
fn bounded_stale_mode_shifts_route_when_telemetry_changes() {
    let mut telemetry = ReadTelemetryStore::new(4_000);
    telemetry.record(ReadSample {
        shard: b"orders".to_vec(),
        region: "us".to_string(),
        latency_ms: 6,
        reads: 10,
    });
    telemetry.record(ReadSample {
        shard: b"orders".to_vec(),
        region: "eu".to_string(),
        latency_ms: 18,
        reads: 10,
    });

    let candidates = vec![
        ReadRouteCandidate {
            region: "us".to_string(),
            strong_consistent: true,
            estimated_staleness_ms: 4,
        },
        ReadRouteCandidate {
            region: "eu".to_string(),
            strong_consistent: true,
            estimated_staleness_ms: 4,
        },
    ];

    let first = choose_read_region(
        b"orders",
        ReadConsistencyMode::BoundedStale {
            max_staleness_ms: 20,
        },
        &candidates,
        &telemetry,
    )
    .expect("first");
    assert_eq!(first.selected_region, "us");

    for _ in 0..8 {
        telemetry.record(ReadSample {
            shard: b"orders".to_vec(),
            region: "eu".to_string(),
            latency_ms: 5,
            reads: 40,
        });
    }

    let second = choose_read_region(
        b"orders",
        ReadConsistencyMode::BoundedStale {
            max_staleness_ms: 20,
        },
        &candidates,
        &telemetry,
    )
    .expect("second");
    assert_eq!(second.selected_region, "eu");
}

#[test]
fn strong_mode_keeps_consistency_guardrail() {
    let telemetry = ReadTelemetryStore::new(3_000);
    let candidates = vec![
        ReadRouteCandidate {
            region: "us".to_string(),
            strong_consistent: false,
            estimated_staleness_ms: 1,
        },
        ReadRouteCandidate {
            region: "eu".to_string(),
            strong_consistent: true,
            estimated_staleness_ms: 2,
        },
    ];

    let decision = choose_read_region(
        b"orders",
        ReadConsistencyMode::Strong,
        &candidates,
        &telemetry,
    )
    .expect("strong decision");
    assert_eq!(decision.selected_region, "eu");
    assert!(
        decision
            .reasons
            .iter()
            .any(|line| line.contains("strong guardrail"))
    );
}
