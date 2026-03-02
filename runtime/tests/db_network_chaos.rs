use wrela_runtime::db::net::transport::{
    TransportChaosAction, TransportChaosConfig, TransportFlowConfig, TransportLane,
    TransportScheduler, classify_chaos_action,
};

fn lane_for(frame_id: u64) -> TransportLane {
    match frame_id % 4 {
        0 => TransportLane::Control,
        1 => TransportLane::Raft,
        2 => TransportLane::Snapshot,
        _ => TransportLane::Bulk,
    }
}

#[test]
fn chaos_classification_is_deterministic_and_non_trivial() {
    let config = TransportChaosConfig::new(0xC0FFEE, 20, 15, 25);
    let mut drop_count = 0usize;
    let mut delay_count = 0usize;

    for frame_id in 1..=400_u64 {
        let lane = lane_for(frame_id);
        let action = classify_chaos_action(config, frame_id, lane);
        if action == TransportChaosAction::Drop {
            drop_count += 1;
        }
        if action == TransportChaosAction::Delay {
            delay_count += 1;
        }
    }

    let again = (1..=400_u64)
        .map(|frame_id| classify_chaos_action(config, frame_id, lane_for(frame_id)))
        .collect::<Vec<_>>();
    let once = (1..=400_u64)
        .map(|frame_id| classify_chaos_action(config, frame_id, lane_for(frame_id)))
        .collect::<Vec<_>>();

    assert_eq!(once, again);
    assert!(drop_count > 0);
    assert!(delay_count > 0);
}

#[test]
fn scheduler_snapshot_pressure_keeps_control_and_raft_non_starving() {
    let mut scheduler = TransportScheduler::new(TransportFlowConfig::new(256, 64, 512));
    let mut dispatched = Vec::new();
    let mut control_last = None;
    let mut raft_last = None;
    let mut max_control_gap = 0usize;
    let mut max_raft_gap = 0usize;
    let mut snapshot_seq = 0u64;
    let mut control_seq = 0u64;
    let mut raft_seq = 0u64;

    for step in 0..140usize {
        while scheduler.pending_in_lane(TransportLane::Snapshot) < 64 {
            scheduler
                .enqueue(
                    TransportLane::Snapshot,
                    format!("snapshot-{snapshot_seq}"),
                    32,
                )
                .expect("enqueue snapshot");
            snapshot_seq += 1;
        }
        while scheduler.pending_in_lane(TransportLane::Control) < 2 {
            scheduler
                .enqueue(TransportLane::Control, format!("control-{control_seq}"), 16)
                .expect("enqueue control");
            control_seq += 1;
        }
        while scheduler.pending_in_lane(TransportLane::Raft) < 2 {
            scheduler
                .enqueue(TransportLane::Raft, format!("raft-{raft_seq}"), 16)
                .expect("enqueue raft");
            raft_seq += 1;
        }

        let frame = scheduler
            .try_dispatch()
            .expect("dispatch should progress under pressure");
        dispatched.push(frame.lane);
        scheduler.ack(frame.frame_id).expect("ack");

        if frame.lane == TransportLane::Control {
            if let Some(prev) = control_last {
                max_control_gap = max_control_gap.max(step - prev);
            }
            control_last = Some(step);
        }
        if frame.lane == TransportLane::Raft {
            if let Some(prev) = raft_last {
                max_raft_gap = max_raft_gap.max(step - prev);
            }
            raft_last = Some(step);
        }
    }

    assert!(
        dispatched.contains(&TransportLane::Snapshot),
        "snapshot traffic never dispatched"
    );
    assert!(
        control_last.is_some() && raft_last.is_some(),
        "control/raft never dispatched: {dispatched:?}"
    );
    assert!(
        max_control_gap <= 4,
        "control starved: gap={max_control_gap} trace={dispatched:?}"
    );
    assert!(
        max_raft_gap <= 5,
        "raft starved: gap={max_raft_gap} trace={dispatched:?}"
    );
}

#[test]
fn scheduler_backpressure_state_shows_snapshot_hol_block_without_starving_priority_lanes() {
    let mut scheduler = TransportScheduler::new(TransportFlowConfig::new(80, 80, 32));
    scheduler
        .enqueue(TransportLane::Snapshot, "snapshot-1".to_string(), 64)
        .expect("enqueue snapshot");
    scheduler
        .enqueue(TransportLane::Snapshot, "snapshot-2".to_string(), 64)
        .expect("enqueue snapshot");
    scheduler
        .enqueue(TransportLane::Control, "control-1".to_string(), 16)
        .expect("enqueue control");
    scheduler
        .enqueue(TransportLane::Raft, "raft-1".to_string(), 16)
        .expect("enqueue raft");

    let control = scheduler.try_dispatch().expect("dispatch control");
    assert_eq!(control.lane, TransportLane::Control);
    scheduler.ack(control.frame_id).expect("ack control");

    let raft = scheduler.try_dispatch().expect("dispatch raft");
    assert_eq!(raft.lane, TransportLane::Raft);
    scheduler.ack(raft.frame_id).expect("ack raft");

    let snapshot = scheduler.try_dispatch().expect("dispatch snapshot");
    assert_eq!(snapshot.lane, TransportLane::Snapshot);

    let blocked_state = scheduler.state();
    assert_eq!(blocked_state.available_credits, 16);
    assert_eq!(
        scheduler
            .lane_state(TransportLane::Snapshot)
            .in_flight_frames,
        1
    );
    assert_eq!(
        scheduler.lane_state(TransportLane::Snapshot).pending_frames,
        1
    );
    assert!(
        scheduler.try_dispatch().is_none(),
        "snapshot HOL should block when only oversize snapshot remains"
    );

    scheduler
        .enqueue(TransportLane::Control, "control-2".to_string(), 8)
        .expect("enqueue control");
    scheduler
        .enqueue(TransportLane::Raft, "raft-2".to_string(), 8)
        .expect("enqueue raft");
    let resumed_control = scheduler.try_dispatch().expect("resume control");
    assert_eq!(resumed_control.lane, TransportLane::Control);
    scheduler
        .ack(resumed_control.frame_id)
        .expect("ack resumed control");
    let resumed_raft = scheduler.try_dispatch().expect("resume raft");
    assert_eq!(resumed_raft.lane, TransportLane::Raft);
    scheduler
        .ack(resumed_raft.frame_id)
        .expect("ack resumed raft");

    scheduler.ack(snapshot.frame_id).expect("ack snapshot");
    let recovered_state = scheduler.state();
    assert_eq!(recovered_state.available_credits, 80);
    assert_eq!(
        scheduler
            .lane_state(TransportLane::Snapshot)
            .in_flight_frames,
        0
    );
    let next_snapshot = scheduler.try_dispatch().expect("dispatch pending snapshot");
    assert_eq!(next_snapshot.lane, TransportLane::Snapshot);
}
