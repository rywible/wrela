use bytes::Bytes;
use wrela_runtime::db::cdc::{CdcEmitter, evaluate_cdc_correctness_gate};

/// Simulates CDC event emission across a leader failover boundary and verifies
/// that `commit_seq` remains strictly monotonic throughout.
#[test]
fn cdc_commit_seq_monotonic_across_simulated_failover() {
    // Phase 1: Pre-failover writes on original leader's emitter.
    let mut leader_a = CdcEmitter::default();
    for i in 0..50u64 {
        leader_a.emit_put(
            Bytes::from_static(b"core"),
            Bytes::from(format!("key-pre-{i}")),
            Bytes::from(format!("val-pre-{i}")),
            i + 1,
        );
    }
    let pre_page = leader_a.page_since(0, 100, None);
    assert_eq!(pre_page.events.len(), 50);
    evaluate_cdc_correctness_gate(&pre_page, 0).expect("pre-failover monotonicity");

    let pre_high_watermark = pre_page.high_watermark;

    // Phase 2: New leader picks up from the high watermark and continues.
    // In a real failover, the new leader's CdcEmitter is initialized with the
    // commit_seq from the replicated state. Simulate this by creating a new
    // emitter and manually advancing past the old watermark.
    let mut leader_b = CdcEmitter::default();
    // "Fast-forward" the emitter past the pre-failover watermark by emitting
    // (and discarding) placeholder events. This simulates the new leader
    // restoring its commit_seq from replicated state.
    for _ in 0..pre_high_watermark {
        leader_b.emit_put(
            Bytes::from_static(b"_skip"),
            Bytes::from_static(b"_"),
            Bytes::from_static(b"_"),
            0,
        );
    }

    // Now emit real post-failover events.
    for i in 0..50u64 {
        leader_b.emit_put(
            Bytes::from_static(b"core"),
            Bytes::from(format!("key-post-{i}")),
            Bytes::from(format!("val-post-{i}")),
            pre_high_watermark + i + 1,
        );
    }

    // Collect only the post-failover events (after the watermark).
    let post_page = leader_b.page_since(pre_high_watermark, 100, None);
    assert_eq!(post_page.events.len(), 50);
    evaluate_cdc_correctness_gate(&post_page, pre_high_watermark)
        .expect("post-failover monotonicity");

    // Verify cross-boundary monotonicity: last pre-failover seq < first
    // post-failover seq.
    let last_pre_seq = pre_page.events.last().unwrap().commit_seq;
    let first_post_seq = post_page.events.first().unwrap().commit_seq;
    assert!(
        first_post_seq > last_pre_seq,
        "commit_seq must be monotonic across failover boundary: last_pre={last_pre_seq} first_post={first_post_seq}"
    );
}

/// Verifies that the CDC correctness gate rejects non-monotonic sequences that
/// could occur if a failover re-emits events with duplicate or regressed
/// commit_seq values.
#[test]
fn cdc_correctness_gate_catches_regression_across_failover() {
    use wrela_runtime::db::cdc::{CdcEvent, CdcOpKind, CdcPage};

    // Simulate a bug where the new leader starts from a stale commit_seq.
    let page = CdcPage {
        events: vec![
            CdcEvent {
                commit_seq: 10,
                shard: Bytes::from_static(b"core"),
                key: Bytes::from_static(b"pre"),
                kind: CdcOpKind::Put,
                value: Some(Bytes::from_static(b"v")),
                version: 10,
                erasure_proof: None,
            },
            // Regression: new leader emits seq=5 after seq=10.
            CdcEvent {
                commit_seq: 5,
                shard: Bytes::from_static(b"core"),
                key: Bytes::from_static(b"post"),
                kind: CdcOpKind::Put,
                value: Some(Bytes::from_static(b"v2")),
                version: 11,
                erasure_proof: None,
            },
        ],
        next_commit_seq: 5,
        high_watermark: 10,
    };
    let result = evaluate_cdc_correctness_gate(&page, 0);
    assert!(result.is_err(), "must detect non-monotonic commit_seq");
}
