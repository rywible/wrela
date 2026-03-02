#![no_main]

use libfuzzer_sys::fuzz_target;
use wrela_runtime::db::net::transport::{
    TransportChaosConfig, TransportLane, classify_chaos_action,
};
use wrela_runtime_fuzz::{BoundedCursor, cap_input};

const MAX_INPUT_BYTES: usize = 128;

fn decode_lane(tag: u8) -> TransportLane {
    match tag % 4 {
        0 => TransportLane::Control,
        1 => TransportLane::Raft,
        2 => TransportLane::Snapshot,
        _ => TransportLane::Bulk,
    }
}

fuzz_target!(|data: &[u8]| {
    let bounded = cap_input(data, MAX_INPUT_BYTES);
    let mut cursor = BoundedCursor::new(bounded);
    let seed = cursor.take_u64().unwrap_or(0);
    let frame_id = cursor.take_u64().unwrap_or(0);
    let drop_percent = cursor.take_u8().unwrap_or(0);
    let duplicate_percent = cursor.take_u8().unwrap_or(0);
    let delay_percent = cursor.take_u8().unwrap_or(0);
    let lane = decode_lane(cursor.take_u8().unwrap_or(0));

    let cfg = TransportChaosConfig::new(seed, drop_percent, duplicate_percent, delay_percent);
    let _ = classify_chaos_action(cfg, frame_id, lane);
});
