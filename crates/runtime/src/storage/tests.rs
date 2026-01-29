use std::collections::HashMap;

use crate::storage::config::StorageConfig;
use crate::storage::service::{StorageRequest, StorageResponse, StorageService};
use crate::storage::store::TypeConfig;
use crate::storage::transport::set_drop_replication;
use crate::string;
use crate::value::Value;
use openraft::BasicNode;
use openraft::Raft;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};

fn config_for_dir(path: String, snapshot_interval: u64) -> StorageConfig {
    StorageConfig {
        enabled: true,
        path,
        node_id: 1,
        bind_addr: "127.0.0.1:0".to_string(),
        http_enabled: false,
        peers: HashMap::new(),
        bootstrap: true,
        snapshot_interval,
        batch_max_ops: 2,
        batch_max_ms: 1,
        queue_cap: 32,
    }
}

fn config_for_node(
    path: String,
    node_id: u64,
    bind_addr: String,
    peers: HashMap<u64, String>,
    bootstrap: bool,
    snapshot_interval: u64,
) -> StorageConfig {
    let mut cfg = config_for_dir(path, snapshot_interval);
    cfg.node_id = node_id;
    cfg.bind_addr = bind_addr;
    cfg.http_enabled = true;
    cfg.peers = peers;
    cfg.bootstrap = bootstrap;
    cfg
}

fn peers_for(node_id: u64, all: &HashMap<u64, String>) -> HashMap<u64, String> {
    let mut peers = all.clone();
    peers.remove(&node_id);
    peers
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn storage_three_node_cluster() {
    let _lock = acquire_cluster_lock().await;
    let _guard = DropReplicationGuard::new();
    let dir = tempfile::tempdir().expect("temp dir");
    let base = dir.path().to_string_lossy().to_string();

    let Some((listener1, addr1)) = try_bind("127.0.0.1:0").await else {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    };
    let Some((listener2, addr2)) = try_bind("127.0.0.1:0").await else {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    };
    let Some((listener3, addr3)) = try_bind("127.0.0.1:0").await else {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    };

    let mut all_peers = HashMap::new();
    all_peers.insert(1, addr1.clone());
    all_peers.insert(2, addr2.clone());
    all_peers.insert(3, addr3.clone());

    let mut cfg1 = config_for_dir(format!("{}/db1", base), 50);
    cfg1.node_id = 1;
    cfg1.bind_addr = addr1.clone();
    cfg1.http_enabled = true;
    cfg1.peers = peers_for(1, &all_peers);
    let service1 = StorageService::start_for_test_with_listener(cfg1, listener1)
        .await
        .expect("start node1");

    let mut cfg2 = config_for_dir(format!("{}/db2", base), 50);
    cfg2.node_id = 2;
    cfg2.bind_addr = addr2.clone();
    cfg2.http_enabled = true;
    cfg2.bootstrap = false;
    cfg2.peers = peers_for(2, &all_peers);
    let service2 = StorageService::start_for_test_with_listener(cfg2, listener2)
        .await
        .expect("start node2");

    let mut cfg3 = config_for_dir(format!("{}/db3", base), 50);
    cfg3.node_id = 3;
    cfg3.bind_addr = addr3.clone();
    cfg3.http_enabled = true;
    cfg3.bootstrap = false;
    cfg3.peers = peers_for(3, &all_peers);
    let service3 = StorageService::start_for_test_with_listener(cfg3, listener3)
        .await
        .expect("start node3");

    add_learner_with_retry(service1.raft_ref(), 2, BasicNode { addr: addr2.clone() }).await;
    add_learner_with_retry(service1.raft_ref(), 3, BasicNode { addr: addr3.clone() }).await;
    change_membership_with_retry(service1.raft_ref(), [1u64, 2u64, 3u64]).await;

    let resp = service1
        .dispatch_to(StorageRequest::Put {
            key: b"cluster".to_vec(),
            value: b"ok".to_vec(),
        })
        .await
        .expect("put");
    matches_ok(resp);

    let mut tries = 0u32;
    loop {
        if let Some(val) = service2.local_get(b"cluster").await {
            assert_eq!(val, b"ok".to_vec());
            break;
        }
        tries += 1;
        if tries > 50 {
            panic!("timed out waiting for replication");
        }
        sleep(Duration::from_millis(20)).await;
    }

    let val3 = service3.local_get(b"cluster").await;
    assert_eq!(val3, Some(b"ok".to_vec()));

    service1.shutdown().await;
    service2.shutdown().await;
    service3.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn storage_crash_restart_unflushed_logs() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("db").to_string_lossy().to_string();

    let service = StorageService::start_for_test(config_for_dir(path.clone(), 10))
        .await
        .expect("start service");

    let resp = service
        .dispatch_to(StorageRequest::Put {
            key: b"crash".to_vec(),
            value: b"ok".to_vec(),
        })
        .await
        .expect("put");
    matches_ok(resp);

    drop(service);
    sleep(Duration::from_millis(50)).await;

    let service = StorageService::start_for_test(config_for_dir(path, 10))
        .await
        .expect("restart service");

    let resp = service
        .dispatch_to(StorageRequest::Get {
            key: b"crash".to_vec(),
        })
        .await
        .expect("get");
    let bytes = expect_ok_bytes(resp);
    assert_eq!(bytes, b"ok".to_vec());

    service.shutdown().await;
    sleep(Duration::from_millis(50)).await;
    sleep(Duration::from_millis(50)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn storage_single_node_durability() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("db").to_string_lossy().to_string();

    let service = StorageService::start_for_test(config_for_dir(path.clone(), 10))
        .await
        .expect("start service");

    let resp = service
        .dispatch_to(StorageRequest::Put {
            key: b"k".to_vec(),
            value: b"v".to_vec(),
        })
        .await
        .expect("put");
    matches_ok(resp);

    service.shutdown().await;
    sleep(Duration::from_millis(50)).await;
    sleep(Duration::from_millis(50)).await;

    let service = StorageService::start_for_test(config_for_dir(path, 10))
        .await
        .expect("restart service");

    let resp = service
        .dispatch_to(StorageRequest::Get { key: b"k".to_vec() })
        .await
        .expect("get");

    let value = expect_ok_value(resp);
    let bytes = string::with_string_bytes(value, |b| b.to_vec()).expect("string value");
    assert_eq!(bytes, b"v".to_vec());

    service.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn storage_snapshot_compaction() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("db").to_string_lossy().to_string();

    let service = StorageService::start_for_test(config_for_dir(path, 1))
        .await
        .expect("start service");

    for idx in 0..5u8 {
        let key = vec![b'k', idx];
        let val = vec![b'v', idx];
        let resp = service
            .dispatch_to(StorageRequest::Put { key, value: val })
            .await
            .expect("put");
        matches_ok(resp);
    }

    let snapshot = service
        .raft_ref()
        .get_snapshot()
        .await
        .expect("get snapshot");

    assert!(snapshot.is_some(), "expected snapshot after writes");

    service.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn storage_snapshot_recovery_restart() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("db").to_string_lossy().to_string();

    let service = StorageService::start_for_test(config_for_dir(path.clone(), 1))
        .await
        .expect("start service");

    for idx in 0..10u8 {
        let key = vec![b'k', idx];
        let val = vec![b'v', idx];
        let resp = service
            .dispatch_to(StorageRequest::Put { key, value: val })
            .await
            .expect("put");
        matches_ok(resp);
    }

    let snapshot = service
        .raft_ref()
        .get_snapshot()
        .await
        .expect("get snapshot");
    assert!(snapshot.is_some(), "expected snapshot before restart");

    service.shutdown().await;

    let service = StorageService::start_for_test(config_for_dir(path, 1))
        .await
        .expect("restart service");

    for idx in 0..10u8 {
        let resp = service
            .dispatch_to(StorageRequest::Get { key: vec![b'k', idx] })
            .await
            .expect("get");
        let bytes = expect_ok_bytes(resp);
        assert_eq!(bytes, vec![b'v', idx]);
    }

    service.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn storage_log_compaction_reduces_log_len() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("db").to_string_lossy().to_string();

    let service = StorageService::start_for_test(config_for_dir(path, 1))
        .await
        .expect("start service");

    for idx in 0..30u8 {
        let key = vec![b'k', idx];
        let val = vec![b'v', idx];
        let resp = service
            .dispatch_to(StorageRequest::Put { key, value: val })
            .await
            .expect("put");
        matches_ok(resp);
    }

    let snapshot = service
        .raft_ref()
        .get_snapshot()
        .await
        .expect("get snapshot");
    assert!(snapshot.is_some(), "expected snapshot for compaction");

    if let Some(last_applied) = service.raft_ref().metrics().borrow().last_applied {
        let upto = last_applied.index.saturating_sub(1);
        let _ = service.raft_ref().trigger().purge_log(upto).await;
    }

    let mut tries = 0u32;
    loop {
        let log_len = service.log_len();
        if log_len < 30 {
            break;
        }
        tries += 1;
        if tries > 50 {
            panic!("expected log compaction, log_len={log_len}");
        }
        sleep(Duration::from_millis(20)).await;
    }

    service.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn storage_batch_atomicity() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("db").to_string_lossy().to_string();

    let service = StorageService::start_for_test(config_for_dir(path, 10))
        .await
        .expect("start service");

    let key1 = b"a".to_vec();
    let key2 = b"b".to_vec();

    let r1 = service
        .dispatch_to(StorageRequest::Put {
            key: key1.clone(),
            value: b"1".to_vec(),
        })
        .await
        .expect("put1");
    let r2 = service
        .dispatch_to(StorageRequest::Put {
            key: key2.clone(),
            value: b"2".to_vec(),
        })
        .await
        .expect("put2");

    matches_ok(r1);
    matches_ok(r2);

    let resp = service
        .dispatch_to(StorageRequest::Get { key: key1 })
        .await
        .expect("get1");
    let value = expect_ok_value(resp);
    let bytes = string::with_string_bytes(value, |b| b.to_vec()).expect("string value");
    assert_eq!(bytes, b"1".to_vec());

    let resp = service
        .dispatch_to(StorageRequest::Get { key: key2 })
        .await
        .expect("get2");
    let value = expect_ok_value(resp);
    let bytes = string::with_string_bytes(value, |b| b.to_vec()).expect("string value");
    assert_eq!(bytes, b"2".to_vec());

    service.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn storage_batch_atomicity_mixed_ops() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("db").to_string_lossy().to_string();

    let mut cfg = config_for_dir(path, 10);
    cfg.batch_max_ops = 2;
    cfg.batch_max_ms = 5;

    let service = StorageService::start_for_test(cfg)
        .await
        .expect("start service");

    let key = b"mixed".to_vec();

    let r1 = service
        .dispatch_to(StorageRequest::Put {
            key: key.clone(),
            value: b"v1".to_vec(),
        })
        .await
        .expect("put");
    let r2 = service
        .dispatch_to(StorageRequest::Delete { key: key.clone() })
        .await
        .expect("delete");

    matches_ok(r1);
    matches_ok(r2);

    let resp = service
        .dispatch_to(StorageRequest::Get { key })
        .await
        .expect("get");
    let value = expect_ok_value(resp);
    assert!(value.is_nil(), "expected delete to win in batch");

    service.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn storage_large_values_many_keys() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("db").to_string_lossy().to_string();

    let service = StorageService::start_for_test(config_for_dir(path, 10))
        .await
        .expect("start service");

    let big_value = vec![b'x'; 64 * 1024];
    for idx in 0..200u16 {
        let key = format!("key-{idx}").into_bytes();
        let resp = service
            .dispatch_to(StorageRequest::Put {
                key: key.clone(),
                value: big_value.clone(),
            })
            .await
            .expect("put");
        matches_ok(resp);
    }

    let resp = service
        .dispatch_to(StorageRequest::Get {
            key: b"key-42".to_vec(),
        })
        .await
        .expect("get");
    let bytes = expect_ok_bytes(resp);
    assert_eq!(bytes.len(), big_value.len());

    service.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn storage_leader_failover_writes() {
    let _lock = acquire_cluster_lock().await;
    let _guard = DropReplicationGuard::new();
    let dir = tempfile::tempdir().expect("temp dir");
    let base = dir.path().to_string_lossy().to_string();

    let Some((listener1, addr1)) = try_bind("127.0.0.1:0").await else {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    };
    let Some((listener2, addr2)) = try_bind("127.0.0.1:0").await else {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    };
    let Some((listener3, addr3)) = try_bind("127.0.0.1:0").await else {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    };

    let mut all_peers = HashMap::new();
    all_peers.insert(1, addr1.clone());
    all_peers.insert(2, addr2.clone());
    all_peers.insert(3, addr3.clone());

    let service1 = StorageService::start_for_test_with_listener(
        config_for_node(
            format!("{}/db1", base),
            1,
            addr1.clone(),
            peers_for(1, &all_peers),
            true,
            50,
        ),
        listener1,
    )
    .await
    .expect("start node1");

    let service2 = StorageService::start_for_test_with_listener(
        config_for_node(
            format!("{}/db2", base),
            2,
            addr2.clone(),
            peers_for(2, &all_peers),
            false,
            50,
        ),
        listener2,
    )
    .await
    .expect("start node2");

    let service3 = StorageService::start_for_test_with_listener(
        config_for_node(
            format!("{}/db3", base),
            3,
            addr3.clone(),
            peers_for(3, &all_peers),
            false,
            50,
        ),
        listener3,
    )
    .await
    .expect("start node3");

    add_learner_with_retry(service1.raft_ref(), 2, BasicNode { addr: addr2.clone() }).await;
    add_learner_with_retry(service1.raft_ref(), 3, BasicNode { addr: addr3.clone() }).await;
    change_membership_with_retry(service1.raft_ref(), [1u64, 2u64, 3u64]).await;

    let mut service1 = Some(service1);
    let mut service2 = Some(service2);
    let mut service3 = Some(service3);

    let leader_id = wait_for_leader(
        &[
            service1.as_ref().expect("service1"),
            service2.as_ref().expect("service2"),
            service3.as_ref().expect("service3"),
        ],
        &[1, 2, 3],
    )
    .await;
    let leader = match leader_id {
        1 => service1.as_ref().expect("service1"),
        2 => service2.as_ref().expect("service2"),
        3 => service3.as_ref().expect("service3"),
        _ => panic!("unexpected leader id {leader_id}"),
    };

    let resp = leader
        .dispatch_to(StorageRequest::Put {
            key: b"failover".to_vec(),
            value: b"pre".to_vec(),
        })
        .await
        .expect("put");
    matches_ok(resp);

    match leader_id {
        1 => service1.take().expect("service1").shutdown().await,
        2 => service2.take().expect("service2").shutdown().await,
        3 => service3.take().expect("service3").shutdown().await,
        _ => unreachable!(),
    }

    let mut active_services: Vec<(u64, &StorageService)> = Vec::new();
    if let Some(service) = service1.as_ref() {
        active_services.push((1, service));
    }
    if let Some(service) = service2.as_ref() {
        active_services.push((2, service));
    }
    if let Some(service) = service3.as_ref() {
        active_services.push((3, service));
    }

    let active_refs: Vec<&StorageService> =
        active_services.iter().map(|(_, svc)| *svc).collect();
    let allowed: Vec<u64> = active_services.iter().map(|(id, _)| *id).collect();
    let new_leader_id = wait_for_leader(&active_refs, &allowed).await;
    let new_leader = active_services
        .iter()
        .find(|(id, _)| *id == new_leader_id)
        .expect("new leader")
        .1;

    let resp = new_leader
        .dispatch_to(StorageRequest::Put {
            key: b"failover".to_vec(),
            value: b"post".to_vec(),
        })
        .await
        .expect("put");
    matches_ok(resp);

    let follower = active_services
        .iter()
        .find(|(id, _)| *id != new_leader_id)
        .expect("follower")
        .1;
    wait_for_value(follower, b"failover", b"post".to_vec()).await;

    if let Some(service) = service1.take() {
        service.shutdown().await;
    }
    if let Some(service) = service2.take() {
        service.shutdown().await;
    }
    if let Some(service) = service3.take() {
        service.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn storage_follower_catchup_after_lag() {
    let _lock = acquire_cluster_lock().await;
    let _guard = DropReplicationGuard::new();
    let dir = tempfile::tempdir().expect("temp dir");
    let base = dir.path().to_string_lossy().to_string();

    let Some((listener1, addr1)) = try_bind("127.0.0.1:0").await else {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    };
    let Some((listener2, addr2)) = try_bind("127.0.0.1:0").await else {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    };

    let mut all_peers = HashMap::new();
    all_peers.insert(1, addr1.clone());
    all_peers.insert(2, addr2.clone());

    let service1 = StorageService::start_for_test_with_listener(
        config_for_node(
            format!("{}/db1", base),
            1,
            addr1.clone(),
            peers_for(1, &all_peers),
            true,
            50,
        ),
        listener1,
    )
    .await
    .expect("start node1");

    let service2 = StorageService::start_for_test_with_listener(
        config_for_node(
            format!("{}/db2", base),
            2,
            addr2.clone(),
            peers_for(2, &all_peers),
            false,
            50,
        ),
        listener2,
    )
    .await
    .expect("start node2");

    add_learner_with_retry(service1.raft_ref(), 2, BasicNode { addr: addr2.clone() }).await;
    change_membership_with_retry(service1.raft_ref(), [1u64, 2u64]).await;
    wait_for_membership_contains(service2.raft_ref(), 2).await;

    for idx in 0..5u8 {
        let resp = service1
            .dispatch_to(StorageRequest::Put {
                key: vec![b'k', idx],
                value: vec![b'v', idx],
            })
            .await
            .expect("put");
        matches_ok(resp);
    }

    let Some((listener3, addr3)) = try_bind("127.0.0.1:0").await else {
        eprintln!("skipping: unable to bind sockets in this environment");
        service1.shutdown().await;
        service2.shutdown().await;
        return;
    };

    all_peers.insert(3, addr3.clone());
    let service3 = StorageService::start_for_test_with_listener(
        config_for_node(
            format!("{}/db3", base),
            3,
            addr3.clone(),
            peers_for(3, &all_peers),
            false,
            50,
        ),
        listener3,
    )
    .await
    .expect("start node3");

    add_learner_with_retry(service1.raft_ref(), 3, BasicNode { addr: addr3.clone() }).await;
    change_membership_with_retry(service1.raft_ref(), [1u64, 2u64, 3u64]).await;
    wait_for_membership_contains(service3.raft_ref(), 3).await;

    for idx in 0..5u8 {
        wait_for_value(&service3, &[b'k', idx], vec![b'v', idx]).await;
    }

    service1.shutdown().await;
    service2.shutdown().await;
    service3.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn storage_uncommitted_write_discarded_on_leader_crash() {
    let _lock = acquire_cluster_lock().await;
    let _guard = DropReplicationGuard::new();
    let dir = tempfile::tempdir().expect("temp dir");
    let base = dir.path().to_string_lossy().to_string();

    let Some((listener1, addr1)) = try_bind("127.0.0.1:0").await else {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    };
    let Some((listener2, addr2)) = try_bind("127.0.0.1:0").await else {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    };
    let Some((listener3, addr3)) = try_bind("127.0.0.1:0").await else {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    };

    let mut all_peers = HashMap::new();
    all_peers.insert(1, addr1.clone());
    all_peers.insert(2, addr2.clone());
    all_peers.insert(3, addr3.clone());

    let service1 = StorageService::start_for_test_with_listener(
        config_for_node(
            format!("{}/db1", base),
            1,
            addr1.clone(),
            peers_for(1, &all_peers),
            true,
            50,
        ),
        listener1,
    )
    .await
    .expect("start node1");

    let service2 = StorageService::start_for_test_with_listener(
        config_for_node(
            format!("{}/db2", base),
            2,
            addr2.clone(),
            peers_for(2, &all_peers),
            false,
            50,
        ),
        listener2,
    )
    .await
    .expect("start node2");

    let service3 = StorageService::start_for_test_with_listener(
        config_for_node(
            format!("{}/db3", base),
            3,
            addr3.clone(),
            peers_for(3, &all_peers),
            false,
            50,
        ),
        listener3,
    )
    .await
    .expect("start node3");

    add_learner_with_retry(service1.raft_ref(), 2, BasicNode { addr: addr2.clone() }).await;
    add_learner_with_retry(service1.raft_ref(), 3, BasicNode { addr: addr3.clone() }).await;
    change_membership_with_retry(service1.raft_ref(), [1u64, 2u64, 3u64]).await;
    wait_for_membership_contains(service2.raft_ref(), 2).await;
    wait_for_membership_contains(service3.raft_ref(), 3).await;

    let mut service1 = Some(service1);
    let mut service2 = Some(service2);
    let mut service3 = Some(service3);

    let leader_id = wait_for_leader(
        &[
            service1.as_ref().expect("service1"),
            service2.as_ref().expect("service2"),
            service3.as_ref().expect("service3"),
        ],
        &[1, 2, 3],
    )
    .await;
    let leader = match leader_id {
        1 => service1.as_ref().expect("service1"),
        2 => service2.as_ref().expect("service2"),
        3 => service3.as_ref().expect("service3"),
        _ => panic!("unexpected leader id {leader_id}"),
    };

    let _drop_guard = drop_replication();
    let result = tokio::time::timeout(
        Duration::from_millis(200),
        leader.dispatch_to(StorageRequest::Put {
            key: b"pending".to_vec(),
            value: b"nope".to_vec(),
        }),
    )
    .await;
    match result {
        Ok(Ok(StorageResponse::Ok(_))) => panic!("write unexpectedly committed"),
        _ => {}
    }

    let leader_value = leader.local_get(b"pending").await;
    assert!(leader_value.is_none(), "uncommitted write applied");

    match leader_id {
        1 => service1.take().expect("service1").shutdown().await,
        2 => service2.take().expect("service2").shutdown().await,
        3 => service3.take().expect("service3").shutdown().await,
        _ => unreachable!(),
    }

    set_drop_replication(false);

    let mut active_services: Vec<(u64, &StorageService)> = Vec::new();
    if let Some(service) = service1.as_ref() {
        active_services.push((1, service));
    }
    if let Some(service) = service2.as_ref() {
        active_services.push((2, service));
    }
    if let Some(service) = service3.as_ref() {
        active_services.push((3, service));
    }

    let active_refs: Vec<&StorageService> =
        active_services.iter().map(|(_, svc)| *svc).collect();
    let allowed: Vec<u64> = active_services.iter().map(|(id, _)| *id).collect();
    let new_leader_id = wait_for_leader(&active_refs, &allowed).await;
    let new_leader = active_services
        .iter()
        .find(|(id, _)| *id == new_leader_id)
        .expect("new leader")
        .1;

    wait_for_absent(new_leader, b"pending").await;

    if let Some(service) = service1.take() {
        service.shutdown().await;
    }
    if let Some(service) = service2.take() {
        service.shutdown().await;
    }
    if let Some(service) = service3.take() {
        service.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn storage_committed_write_survives_leader_crash() {
    let _lock = acquire_cluster_lock().await;
    let _guard = DropReplicationGuard::new();
    let dir = tempfile::tempdir().expect("temp dir");
    let base = dir.path().to_string_lossy().to_string();

    let Some((listener1, addr1)) = try_bind("127.0.0.1:0").await else {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    };
    let Some((listener2, addr2)) = try_bind("127.0.0.1:0").await else {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    };
    let Some((listener3, addr3)) = try_bind("127.0.0.1:0").await else {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    };

    let mut all_peers = HashMap::new();
    all_peers.insert(1, addr1.clone());
    all_peers.insert(2, addr2.clone());
    all_peers.insert(3, addr3.clone());

    let service1 = StorageService::start_for_test_with_listener(
        config_for_node(
            format!("{}/db1", base),
            1,
            addr1.clone(),
            peers_for(1, &all_peers),
            true,
            50,
        ),
        listener1,
    )
    .await
    .expect("start node1");

    let service2 = StorageService::start_for_test_with_listener(
        config_for_node(
            format!("{}/db2", base),
            2,
            addr2.clone(),
            peers_for(2, &all_peers),
            false,
            50,
        ),
        listener2,
    )
    .await
    .expect("start node2");

    let service3 = StorageService::start_for_test_with_listener(
        config_for_node(
            format!("{}/db3", base),
            3,
            addr3.clone(),
            peers_for(3, &all_peers),
            false,
            50,
        ),
        listener3,
    )
    .await
    .expect("start node3");

    add_learner_with_retry(service1.raft_ref(), 2, BasicNode { addr: addr2.clone() }).await;
    add_learner_with_retry(service1.raft_ref(), 3, BasicNode { addr: addr3.clone() }).await;
    change_membership_with_retry(service1.raft_ref(), [1u64, 2u64, 3u64]).await;
    wait_for_membership_contains(service2.raft_ref(), 2).await;
    wait_for_membership_contains(service3.raft_ref(), 3).await;

    let mut service1 = Some(service1);
    let mut service2 = Some(service2);
    let mut service3 = Some(service3);

    let leader_id = wait_for_leader(
        &[
            service1.as_ref().expect("service1"),
            service2.as_ref().expect("service2"),
            service3.as_ref().expect("service3"),
        ],
        &[1, 2, 3],
    )
    .await;
    let leader = match leader_id {
        1 => service1.as_ref().expect("service1"),
        2 => service2.as_ref().expect("service2"),
        3 => service3.as_ref().expect("service3"),
        _ => panic!("unexpected leader id {leader_id}"),
    };

    let resp = leader
        .dispatch_to(StorageRequest::Put {
            key: b"survive".to_vec(),
            value: b"yes".to_vec(),
        })
        .await
        .expect("put");
    matches_ok(resp);

    let follower = if leader_id == 1 {
        service2.as_ref().expect("service2")
    } else {
        service1.as_ref().expect("service1")
    };
    wait_for_value(follower, b"survive", b"yes".to_vec()).await;

    match leader_id {
        1 => service1.take().expect("service1").shutdown().await,
        2 => service2.take().expect("service2").shutdown().await,
        3 => service3.take().expect("service3").shutdown().await,
        _ => unreachable!(),
    }

    let mut active_services: Vec<(u64, &StorageService)> = Vec::new();
    if let Some(service) = service1.as_ref() {
        active_services.push((1, service));
    }
    if let Some(service) = service2.as_ref() {
        active_services.push((2, service));
    }
    if let Some(service) = service3.as_ref() {
        active_services.push((3, service));
    }

    let active_refs: Vec<&StorageService> =
        active_services.iter().map(|(_, svc)| *svc).collect();
    let allowed: Vec<u64> = active_services.iter().map(|(id, _)| *id).collect();
    let new_leader_id = wait_for_leader(&active_refs, &allowed).await;
    let new_leader = active_services
        .iter()
        .find(|(id, _)| *id == new_leader_id)
        .expect("new leader")
        .1;

    wait_for_value(new_leader, b"survive", b"yes".to_vec()).await;

    if let Some(service) = service1.take() {
        service.shutdown().await;
    }
    if let Some(service) = service2.take() {
        service.shutdown().await;
    }
    if let Some(service) = service3.take() {
        service.shutdown().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn storage_membership_remove_node() {
    let _lock = acquire_cluster_lock().await;
    let _guard = DropReplicationGuard::new();
    let dir = tempfile::tempdir().expect("temp dir");
    let base = dir.path().to_string_lossy().to_string();

    let Some((listener1, addr1)) = try_bind("127.0.0.1:0").await else {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    };
    let Some((listener2, addr2)) = try_bind("127.0.0.1:0").await else {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    };
    let Some((listener3, addr3)) = try_bind("127.0.0.1:0").await else {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    };

    let mut all_peers = HashMap::new();
    all_peers.insert(1, addr1.clone());
    all_peers.insert(2, addr2.clone());
    all_peers.insert(3, addr3.clone());

    let service1 = StorageService::start_for_test_with_listener(
        config_for_node(
            format!("{}/db1", base),
            1,
            addr1.clone(),
            peers_for(1, &all_peers),
            true,
            50,
        ),
        listener1,
    )
    .await
    .expect("start node1");

    let service2 = StorageService::start_for_test_with_listener(
        config_for_node(
            format!("{}/db2", base),
            2,
            addr2.clone(),
            peers_for(2, &all_peers),
            false,
            50,
        ),
        listener2,
    )
    .await
    .expect("start node2");

    let service3 = StorageService::start_for_test_with_listener(
        config_for_node(
            format!("{}/db3", base),
            3,
            addr3.clone(),
            peers_for(3, &all_peers),
            false,
            50,
        ),
        listener3,
    )
    .await
    .expect("start node3");

    wait_for_leader(&[&service1], &[1]).await;

    add_learner_with_retry(service1.raft_ref(), 2, BasicNode { addr: addr2.clone() }).await;
    add_learner_with_retry(service1.raft_ref(), 3, BasicNode { addr: addr3.clone() }).await;
    change_membership_with_retry(service1.raft_ref(), [1u64, 2u64, 3u64]).await;
    wait_for_membership_contains(service2.raft_ref(), 2).await;
    wait_for_membership_contains(service3.raft_ref(), 3).await;

    wait_for_leader(&[&service1], &[1]).await;
    let mut attempts = 0u32;
    loop {
        change_membership_with_retry(service1.raft_ref(), [1u64, 2u64]).await;
        let leader_id = wait_for_leader(&[&service1, &service2], &[1, 2]).await;
        let leader = if leader_id == 1 { &service1 } else { &service2 };
        if wait_for_membership_not_contains(leader.raft_ref(), 3).await {
            break;
        }
        attempts += 1;
        if attempts > 5 {
            panic!("timed out waiting for membership to remove 3");
        }
    }

    let resp = service1
        .dispatch_to(StorageRequest::Put {
            key: b"removed".to_vec(),
            value: b"nope".to_vec(),
        })
        .await
        .expect("put");
    matches_ok(resp);

    wait_for_absent(&service3, b"removed").await;

    service1.shutdown().await;
    service2.shutdown().await;
    service3.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn storage_linearizable_read_via_follower() {
    let _lock = acquire_cluster_lock().await;
    let _guard = DropReplicationGuard::new();
    let dir = tempfile::tempdir().expect("temp dir");
    let base = dir.path().to_string_lossy().to_string();

    let Some((listener1, addr1)) = try_bind("127.0.0.1:0").await else {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    };
    let Some((listener2, addr2)) = try_bind("127.0.0.1:0").await else {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    };

    let mut all_peers = HashMap::new();
    all_peers.insert(1, addr1.clone());
    all_peers.insert(2, addr2.clone());

    let service1 = StorageService::start_for_test_with_listener(
        config_for_node(
            format!("{}/db1", base),
            1,
            addr1.clone(),
            peers_for(1, &all_peers),
            true,
            50,
        ),
        listener1,
    )
    .await
    .expect("start node1");

    let service2 = StorageService::start_for_test_with_listener(
        config_for_node(
            format!("{}/db2", base),
            2,
            addr2.clone(),
            peers_for(2, &all_peers),
            false,
            50,
        ),
        listener2,
    )
    .await
    .expect("start node2");

    wait_for_leader(&[&service1], &[1]).await;

    add_learner_with_retry(service1.raft_ref(), 2, BasicNode { addr: addr2.clone() }).await;
    change_membership_with_retry(service1.raft_ref(), [1u64, 2u64]).await;
    wait_for_membership_contains(service2.raft_ref(), 2).await;

    for idx in 0..5u8 {
        let resp = service1
            .dispatch_to(StorageRequest::Put {
                key: b"linear".to_vec(),
                value: vec![b'v', idx],
            })
            .await
            .expect("put");
        matches_ok(resp);

        let resp = service2.forward_read_for_test(b"linear".to_vec()).await;
        let bytes = expect_ok_bytes(resp);
        assert_eq!(bytes, vec![b'v', idx]);
    }

    service1.shutdown().await;
    service2.shutdown().await;
}

fn matches_ok(resp: StorageResponse) {
    match resp {
        StorageResponse::Ok(_) => {}
        StorageResponse::Err(err) => panic!("unexpected error: {err}"),
    }
}

fn expect_ok_value(resp: StorageResponse) -> Value {
    match resp {
        StorageResponse::Ok(value) => value,
        StorageResponse::Err(err) => panic!("unexpected error: {err}"),
    }
}

fn expect_ok_bytes(resp: StorageResponse) -> Vec<u8> {
    let value = expect_ok_value(resp);
    string::with_string_bytes(value, |b| b.to_vec()).expect("string value")
}

async fn try_bind(addr: &str) -> Option<(TcpListener, String)> {
    match TcpListener::bind(addr).await {
        Ok(listener) => {
            let addr = listener.local_addr().ok()?.to_string();
            Some((listener, addr))
        }
        Err(err) => {
            if matches!(err.kind(), std::io::ErrorKind::PermissionDenied) {
                None
            } else {
                panic!("bind failed: {err}");
            }
        }
    }
}

async fn wait_for_leader(services: &[&StorageService], allowed: &[u64]) -> u64 {
    let mut tries = 0u32;
    loop {
        for service in services {
            if let Some(id) = service.raft_ref().metrics().borrow().current_leader {
                if allowed.is_empty() || allowed.contains(&id) {
                    return id;
                }
            }
        }
        tries += 1;
        if tries > 100 {
            panic!("timed out waiting for leader");
        }
        sleep(Duration::from_millis(20)).await;
    }
}

async fn wait_for_value(service: &StorageService, key: &[u8], expected: Vec<u8>) {
    let mut tries = 0u32;
    loop {
        if let Some(val) = service.local_get(key).await {
            if val == expected {
                return;
            }
        }
        tries += 1;
        if tries > 100 {
            panic!("timed out waiting for value");
        }
        sleep(Duration::from_millis(20)).await;
    }
}

async fn wait_for_absent(service: &StorageService, key: &[u8]) {
    let mut tries = 0u32;
    loop {
        if service.local_get(key).await.is_none() {
            return;
        }
        tries += 1;
        if tries > 200 {
            panic!("timed out waiting for key removal");
        }
        sleep(Duration::from_millis(20)).await;
    }
}

async fn add_learner_with_retry(raft: &Raft<TypeConfig>, id: u64, node: BasicNode) {
    let mut tries = 0u32;
    loop {
        match raft.add_learner(id, node.clone(), true).await {
            Ok(_) => return,
            Err(err) => {
                let msg = err.to_string();
                if is_membership_in_progress(&msg) && tries < 200 {
                    tries += 1;
                    sleep(Duration::from_millis(25)).await;
                    continue;
                }
                panic!("add learner {id}: {err}");
            }
        }
    }
}

async fn change_membership_with_retry<const N: usize>(raft: &Raft<TypeConfig>, members: [u64; N]) {
    let mut tries = 0u32;
    loop {
        match raft.change_membership(members, false).await {
            Ok(_) => return,
            Err(err) => {
                let msg = err.to_string();
                if is_membership_in_progress(&msg) && tries < 200 {
                    tries += 1;
                    sleep(Duration::from_millis(25)).await;
                    continue;
                }
                panic!("change membership: {err}");
            }
        }
    }
}

fn is_membership_in_progress(message: &str) -> bool {
    message.contains("InProgress") || message.contains("configuration change")
}

async fn wait_for_membership_contains(raft: &Raft<TypeConfig>, node_id: u64) {
    let mut tries = 0u32;
    loop {
        let metrics = raft.metrics().borrow().clone();
        let has = metrics
            .membership_config
            .membership()
            .nodes()
            .any(|(id, _)| *id == node_id);
        if has {
            return;
        }
        tries += 1;
        if tries > 200 {
            panic!("timed out waiting for membership to include {node_id}");
        }
        sleep(Duration::from_millis(25)).await;
    }
}

async fn wait_for_membership_not_contains(raft: &Raft<TypeConfig>, node_id: u64) -> bool {
    let mut tries = 0u32;
    loop {
        let metrics = raft.metrics().borrow().clone();
        let has = metrics
            .membership_config
            .membership()
            .nodes()
            .any(|(id, _)| *id == node_id);
        if !has {
            return true;
        }
        tries += 1;
        if tries > 200 {
            return false;
        }
        sleep(Duration::from_millis(25)).await;
    }
}

async fn acquire_cluster_lock() -> tokio::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::OnceLock<Mutex<()>> = std::sync::OnceLock::new();
    let lock = LOCK.get_or_init(|| Mutex::new(()));
    lock.lock().await
}

struct ReplicationDropGuard;

impl Drop for ReplicationDropGuard {
    fn drop(&mut self) {
        set_drop_replication(false);
    }
}

fn drop_replication() -> ReplicationDropGuard {
    set_drop_replication(true);
    ReplicationDropGuard
}

struct DropReplicationGuard;

impl DropReplicationGuard {
    fn new() -> Self {
        set_drop_replication(false);
        Self
    }
}

impl Drop for DropReplicationGuard {
    fn drop(&mut self) {
        set_drop_replication(false);
    }
}
