use bytes::Bytes;
use wrela_runtime::db::DbConfig;
use wrela_runtime::db::config::ReplicationConfig;
use wrela_runtime::db::gateway::write_with_ownership_forwarding;
use wrela_runtime::db::rpc::errors::RpcStatusCode;
use wrela_runtime::db::rpc::grpc::{GrpcEdgeService, WriteBatchRequest};
use wrela_runtime::db::rpc::private_network::{
    NodeAddressResolver, build_private_write_transport, point_read_over_private_rpc,
    replicate_write_batch_over_private_rpc, start_private_rpc_server,
};
use wrela_runtime::db::shard::build_initial_shard_map;
use wrela_runtime::db::types::BatchOp;
use wrela_runtime::db::{close_db, open_db_with_config, read_point, resolve_owner, submit_put};

fn open_forwarding_db(path: &std::path::Path) -> i64 {
    let config = DbConfig::for_testing().with_replication(ReplicationConfig {
        factor: 3,
        write_quorum: 2,
        ..DbConfig::for_testing().replication
    });
    open_db_with_config(path, &config).expect("open db")
}

fn ownership_fence_for(handle: i64, key: &[u8]) -> (u64, u64, String) {
    let owner = resolve_owner(handle, b"core".to_vec(), key.to_vec()).expect("resolve owner");
    (
        owner.home_epoch,
        owner.shard_map_epoch,
        owner.ownership_token,
    )
}

fn assert_visible_within(
    handle: i64,
    key: &[u8],
    expected_value: &[u8],
    timeout: std::time::Duration,
) {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        let value = read_point(handle, b"core".to_vec(), key.to_vec()).expect("read");
        if value.as_deref() == Some(expected_value) {
            return;
        }
        if std::time::Instant::now() >= deadline {
            panic!(
                "timed out waiting for read-your-write key={:?} expected={:?} got={:?}",
                key, expected_value, value
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}

#[test]
fn gateway_forwards_write_to_owner_and_preserves_idempotency() {
    let dir = tempfile::tempdir().expect("tempdir");
    let handle = open_forwarding_db(dir.path());
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
    let (expected_home_epoch, expected_shard_map_epoch, ownership_token) =
        ownership_fence_for(handle, b"k-forward");
    let req = WriteBatchRequest {
        handle,
        ops: vec![BatchOp::Put {
            namespace: Bytes::from_static(b"core"),
            key: Bytes::from_static(b"k-forward"),
            value: Bytes::from_static(b"v1"),
            expected_version: None,
        }],
        idempotency_token: Some("tok-gw-1".to_string()),
        expected_home_epoch,
        expected_shard_map_epoch,
        ownership_token,
    };

    let first = write_with_ownership_forwarding("node-a", 0, &map, &mut service, req.clone())
        .expect("forwarded write");
    let second = write_with_ownership_forwarding("node-a", 0, &map, &mut service, req)
        .expect("forwarded replay");

    assert!(first.metrics.forwarded);
    assert!(
        first.response.commit_version > 0,
        "forwarded write must not report false-success commit version"
    );
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

#[test]
fn gateway_forwarding_uses_remote_transport_when_owner_is_not_local() {
    let dir_owner = tempfile::tempdir().expect("owner tempdir");
    let dir_follower = tempfile::tempdir().expect("follower tempdir");
    let owner_handle = open_forwarding_db(dir_owner.path());
    let follower_handle = open_forwarding_db(dir_follower.path());

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

    let owner_service = std::sync::Arc::new(std::sync::RwLock::new(GrpcEdgeService::new(
        "node-b", "node-b",
    )));
    let mut follower_service = GrpcEdgeService::new("node-a", "node-a");
    let owner_transport = owner_service.clone();
    follower_service.set_remote_write_transport(Some(std::sync::Arc::new(move |_target, req| {
        owner_transport
            .write()
            .expect("owner service lock")
            .write_batch(req)
    })));

    let (expected_home_epoch, expected_shard_map_epoch, ownership_token) =
        ownership_fence_for(owner_handle, b"k-remote");
    let req = WriteBatchRequest {
        handle: owner_handle,
        ops: vec![BatchOp::Put {
            namespace: Bytes::from_static(b"core"),
            key: Bytes::from_static(b"k-remote"),
            value: Bytes::from_static(b"v-remote"),
            expected_version: None,
        }],
        idempotency_token: Some("tok-remote-1".to_string()),
        expected_home_epoch,
        expected_shard_map_epoch,
        ownership_token,
    };

    let first =
        write_with_ownership_forwarding("node-a", 0, &map, &mut follower_service, req.clone())
            .expect("forwarded write");
    let second = write_with_ownership_forwarding("node-a", 0, &map, &mut follower_service, req)
        .expect("forwarded replay");

    assert!(first.metrics.forwarded);
    assert!(
        first.response.commit_version > 0,
        "forwarded write must not report false-success commit version"
    );
    assert_eq!(
        first.response.commit_version,
        second.response.commit_version
    );
    assert!(second.response.idempotent_replay);

    assert_visible_within(
        owner_handle,
        b"k-remote",
        b"v-remote",
        std::time::Duration::from_secs(1),
    );
    let follower_value =
        read_point(follower_handle, b"core".to_vec(), b"k-remote".to_vec()).expect("read follower");
    assert!(
        follower_value.is_none(),
        "forwarded write must not apply locally"
    );

    assert!(close_db(owner_handle));
    assert!(close_db(follower_handle));
}

#[test]
fn gateway_forwarding_over_private_rpc_network_uses_leader_bound_handle() {
    let dir_owner = tempfile::tempdir().expect("owner tempdir");
    let dir_follower = tempfile::tempdir().expect("follower tempdir");
    let owner_handle = open_forwarding_db(dir_owner.path());
    let follower_handle = open_forwarding_db(dir_follower.path());

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

    let mut owner_svc = GrpcEdgeService::new("node-b", "node-b");
    owner_svc.bind_handle(owner_handle);
    let owner_service = std::sync::Arc::new(std::sync::RwLock::new(owner_svc));
    let mut private_server = start_private_rpc_server(
        "127.0.0.1:0",
        owner_service.clone(),
        std::time::Duration::from_millis(750),
    )
    .expect("start private rpc server");
    let owner_addr = private_server.listen_addr().to_string();

    let resolver: NodeAddressResolver = std::sync::Arc::new(move |node_id| {
        if node_id == "node-b" {
            Some(owner_addr.clone())
        } else {
            None
        }
    });

    let mut follower_service = GrpcEdgeService::new("node-a", "node-a");
    follower_service.bind_handle(follower_handle);
    follower_service.set_remote_write_transport(Some(build_private_write_transport(
        resolver,
        std::time::Duration::from_secs(1),
    )));

    let (expected_home_epoch, expected_shard_map_epoch, ownership_token) =
        ownership_fence_for(owner_handle, b"k-private-net");
    let request = WriteBatchRequest {
        handle: 0,
        ops: vec![BatchOp::Put {
            namespace: Bytes::from_static(b"core"),
            key: Bytes::from_static(b"k-private-net"),
            value: Bytes::from_static(b"v-private-net"),
            expected_version: None,
        }],
        idempotency_token: Some("tok-private-net-1".to_string()),
        expected_home_epoch,
        expected_shard_map_epoch,
        ownership_token: ownership_token.clone(),
    };
    let first = write_with_ownership_forwarding("node-a", 0, &map, &mut follower_service, request)
        .expect("forward first");
    let replay = write_with_ownership_forwarding(
        "node-a",
        0,
        &map,
        &mut follower_service,
        WriteBatchRequest {
            handle: 999_999,
            ops: vec![BatchOp::Put {
                namespace: Bytes::from_static(b"core"),
                key: Bytes::from_static(b"k-private-net"),
                value: Bytes::from_static(b"v-private-net"),
                expected_version: None,
            }],
            idempotency_token: Some("tok-private-net-1".to_string()),
            expected_home_epoch,
            expected_shard_map_epoch,
            ownership_token,
        },
    )
    .expect("forward replay");
    assert!(first.metrics.forwarded);
    assert!(
        first.response.commit_version > 0,
        "forwarded write must not report false-success commit version"
    );
    assert_eq!(
        first.response.commit_version, replay.response.commit_version,
        "idempotent replay should preserve commit version"
    );
    assert!(replay.response.idempotent_replay);

    assert_visible_within(
        owner_handle,
        b"k-private-net",
        b"v-private-net",
        std::time::Duration::from_secs(1),
    );
    let follower_value = read_point(follower_handle, b"core".to_vec(), b"k-private-net".to_vec())
        .expect("read follower");
    assert!(
        follower_value.is_none(),
        "follower should not apply forwarded write"
    );

    private_server.shutdown();
    assert!(close_db(owner_handle));
    assert!(close_db(follower_handle));
}

#[test]
fn gateway_forwarding_fails_closed_without_remote_transport() {
    let dir = tempfile::tempdir().expect("tempdir");
    let handle = open_forwarding_db(dir.path());
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

    let mut service = GrpcEdgeService::new("node-a", "node-a");
    let (expected_home_epoch, expected_shard_map_epoch, ownership_token) =
        ownership_fence_for(handle, b"k-fail");
    let err = write_with_ownership_forwarding(
        "node-a",
        0,
        &map,
        &mut service,
        WriteBatchRequest {
            handle,
            ops: vec![BatchOp::Put {
                namespace: Bytes::from_static(b"core"),
                key: Bytes::from_static(b"k-fail"),
                value: Bytes::from_static(b"v"),
                expected_version: None,
            }],
            idempotency_token: Some("tok-fail-1".to_string()),
            expected_home_epoch,
            expected_shard_map_epoch,
            ownership_token,
        },
    )
    .expect_err("forwarding without transport must fail closed");

    match err {
        wrela_runtime::db::gateway::GatewayWriteError::Rpc(rpc) => {
            assert_eq!(rpc.code, RpcStatusCode::Unavailable);
        }
        other => panic!("unexpected error: {other:?}"),
    }
    assert!(close_db(handle));
}

#[test]
fn private_point_read_uses_bound_leader_handle() {
    let dir_owner = tempfile::tempdir().expect("owner tempdir");
    let owner_handle = open_forwarding_db(dir_owner.path());
    submit_put(
        owner_handle,
        b"core".to_vec(),
        b"k-private-read".to_vec(),
        b"v-private-read".to_vec(),
        None,
    )
    .expect("seed put");

    let mut owner_svc = GrpcEdgeService::new("node-b", "node-b");
    owner_svc.bind_handle(owner_handle);
    let owner_service = std::sync::Arc::new(std::sync::RwLock::new(owner_svc));
    let mut private_server = start_private_rpc_server(
        "127.0.0.1:0",
        owner_service,
        std::time::Duration::from_millis(750),
    )
    .expect("start private rpc server");
    let owner_addr = private_server.listen_addr().to_string();

    let value = point_read_over_private_rpc(
        &owner_addr,
        wrela_runtime::db::rpc::grpc::PointReadRequest {
            handle: 0,
            namespace: b"core".to_vec(),
            key: b"k-private-read".to_vec(),
        },
        std::time::Duration::from_secs(1),
    )
    .expect("point read");
    assert_eq!(value, Some(b"v-private-read".to_vec()));

    private_server.shutdown();
    assert!(close_db(owner_handle));
}

#[test]
fn gateway_replica_forwarding_never_reports_success_when_apply_fails() {
    let dir_owner = tempfile::tempdir().expect("owner tempdir");
    let owner_handle = open_forwarding_db(dir_owner.path());

    let mut owner_svc = GrpcEdgeService::new("node-b", "node-b");
    owner_svc.bind_handle(owner_handle);
    let owner_service = std::sync::Arc::new(std::sync::RwLock::new(owner_svc));
    let mut private_server = start_private_rpc_server(
        "127.0.0.1:0",
        owner_service,
        std::time::Duration::from_millis(750),
    )
    .expect("start private rpc server");
    let owner_addr = private_server.listen_addr().to_string();
    let (expected_home_epoch, expected_shard_map_epoch, ownership_token) =
        ownership_fence_for(owner_handle, b"k-replica-fail");

    let err = replicate_write_batch_over_private_rpc(
        &owner_addr,
        WriteBatchRequest {
            handle: 0,
            ops: vec![BatchOp::Put {
                namespace: Bytes::from_static(b"core"),
                key: Bytes::from_static(b"k-replica-fail"),
                value: Bytes::from_static(b"v"),
                expected_version: Some(9),
            }],
            idempotency_token: Some("tok-replica-fail-1".to_string()),
            expected_home_epoch,
            expected_shard_map_epoch,
            ownership_token,
        },
        std::time::Duration::from_secs(1),
    )
    .expect_err("replica write with invalid expected version must fail");
    assert!(
        matches!(
            err.code,
            RpcStatusCode::OccMismatch | RpcStatusCode::InvalidArgument
        ),
        "unexpected replica error: {err:?}"
    );
    let value = read_point(owner_handle, b"core".to_vec(), b"k-replica-fail".to_vec())
        .expect("read value after failed replica write");
    assert!(
        value.is_none(),
        "failed replica forwarding must not report success with visible value"
    );

    private_server.shutdown();
    assert!(close_db(owner_handle));
}
