#![no_main]

use libfuzzer_sys::fuzz_target;
use wrela_runtime::db::rpc::grpc::{GrpcEdgeService, WriteBatchRequest};
use wrela_runtime::db::types::BatchOp;
use wrela_runtime_fuzz::{BoundedCursor, cap_input};

const MAX_INPUT_BYTES: usize = 8192;
const MAX_OPS: usize = 32;
const MAX_FIELD_BYTES: usize = 256;

fn decode_ops(cursor: &mut BoundedCursor<'_>) -> Vec<BatchOp> {
    let count = cursor.take_u8().unwrap_or(0) as usize;
    let count = count.min(MAX_OPS);
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let kind = cursor.take_u8().unwrap_or(0);
        let namespace = cursor.take_vec_bounded(MAX_FIELD_BYTES);
        let key = cursor.take_vec_bounded(MAX_FIELD_BYTES);
        let expected_version = match cursor.take_u8().unwrap_or(0) % 3 {
            0 => None,
            _ => Some(cursor.take_u64().unwrap_or(0)),
        };
        if kind % 2 == 0 {
            let value = cursor.take_vec_bounded(MAX_FIELD_BYTES);
            out.push(BatchOp::Put {
                namespace,
                key,
                value,
                expected_version,
            });
        } else {
            out.push(BatchOp::Delete {
                namespace,
                key,
                expected_version,
            });
        }
    }
    out
}

fuzz_target!(|data: &[u8]| {
    let bounded = cap_input(data, MAX_INPUT_BYTES);
    let mut cursor = BoundedCursor::new(bounded);

    let handle = cursor.take_u64().unwrap_or(0) as i64;
    let token_mode = cursor.take_u8().unwrap_or(0) % 3;
    let token = match token_mode {
        0 => None,
        1 => Some(String::new()),
        _ => Some(String::from_utf8_lossy(&cursor.take_vec_bounded(MAX_FIELD_BYTES)).to_string()),
    };
    let ops = decode_ops(&mut cursor);

    // Follower-only service forces validation and redirect handling without mutating engine state.
    let mut svc = GrpcEdgeService::new("node-a", "node-b");
    let _ = svc.write_batch(WriteBatchRequest {
        handle,
        ops,
        idempotency_token: token,
    });
});
