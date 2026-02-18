use wrela_runtime::db::gateway::write_with_ownership_forwarding;
use wrela_runtime::db::rpc::grpc::{GrpcEdgeService, WriteBatchRequest};
use wrela_runtime::db::shard::build_initial_shard_map;
use wrela_runtime::db::types::BatchOp;
use wrela_runtime::db::{close_db, open_db, read_point};

#[test]
fn gateway_forwards_write_to_owner_and_preserves_idempotency() {
    let dir = tempfile::tempdir().expect("tempdir");
    let handle = open_db(dir.path()).expect("open");
    let mut map = build_initial_shard_map(
        &[
            "node-a".to_string(),
            "node-b".to_string(),
            "node-c".to_string(),
        ],
        1,
        3,
    )
    .expect("map");
    map.assignments.get_mut(&0).expect("shard").leader = "node-b".to_string();

    let mut service = GrpcEdgeService::new("node-b", "node-b");
    let req = WriteBatchRequest {
        handle,
        ops: vec![BatchOp::Put {
            namespace: b"core".to_vec(),
            key: b"k-forward".to_vec(),
            value: b"v1".to_vec(),
            expected_version: None,
        }],
        idempotency_token: Some("tok-gw-1".to_string()),
    };

    let first = write_with_ownership_forwarding("node-a", 0, &map, &mut service, req.clone())
        .expect("forwarded write");
    let second = write_with_ownership_forwarding("node-a", 0, &map, &mut service, req)
        .expect("forwarded replay");

    assert!(first.metrics.forwarded);
    assert!(second.response.idempotent_replay);
    assert_eq!(
        first.response.commit_version,
        second.response.commit_version
    );

    let value = read_point(handle, b"core".to_vec(), b"k-forward".to_vec())
        .expect("read")
        .expect("value");
    assert_eq!(value, b"v1".to_vec());
    assert!(close_db(handle));
}
