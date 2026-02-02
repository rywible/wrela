use std::collections::HashMap;

use crate::metrics;
use crate::storage::backup::{backup_prefix, verify_checksum};
use crate::storage::blob::BlobBackend;
use crate::storage::config::{BackupConfig, BlobConfig, RestoreMode, StorageConfig};
use crate::storage::service::StorageError;
use crate::storage::service::{StorageRequest, StorageResponse, StorageService};
use crate::storage::store::{SerializableKvStateMachine, TypeConfig};
use crate::storage::transport::set_drop_replication;
use crate::string;
use crate::value::Value;
use openraft::BasicNode;
use openraft::Raft;
use sha2::Digest;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio::time::{Duration, sleep};

fn config_for_dir(path: String, snapshot_interval: u64) -> StorageConfig {
    let blob_path = format!("{path}.blobs");
    StorageConfig {
        enabled: true,
        path,
        node_id: 1,
        bind_addr: "127.0.0.1:0".to_string(),
        http_enabled: false,
        peer_token: None,
        peers: HashMap::new(),
        bootstrap: true,
        snapshot_interval,
        batch_max_ops: 2,
        batch_max_ms: 1,
        queue_cap: 32,
        blob: BlobConfig {
            threshold_bytes: 256 * 1024,
            file_path: blob_path,
            s3: None,
        },
        backup: BackupConfig {
            enabled: false,
            max_age_secs: 3600,
            max_logs: 100_000,
            retention_days: 7,
            max_keep: 0,
            prefix: "backups".to_string(),
            only_leader: true,
            restore_mode: RestoreMode::Single,
            restore_id: None,
        },
    }
}

fn config_for_dir_with_threshold(
    path: String,
    snapshot_interval: u64,
    threshold_bytes: usize,
) -> StorageConfig {
    let mut config = config_for_dir(path, snapshot_interval);
    config.blob.threshold_bytes = threshold_bytes;
    config
}

fn count_blob_files(path: &std::path::Path) -> usize {
    fn walk(dir: &std::path::Path, count: &mut usize) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, count);
            } else {
                *count += 1;
            }
        }
    }
    let mut count = 0;
    walk(path, &mut count);
    count
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

    add_learner_with_retry(
        service1.raft_ref(),
        2,
        BasicNode {
            addr: addr2.clone(),
        },
    )
    .await;
    add_learner_with_retry(
        service1.raft_ref(),
        3,
        BasicNode {
            addr: addr3.clone(),
        },
    )
    .await;
    change_membership_with_retry(service1.raft_ref(), [1u64, 2u64, 3u64]).await;

    let resp = service1
        .dispatch_to(StorageRequest::Put {
            key: b"cluster".to_vec(),
            value: b"ok".to_vec(),
        })
        .await
        .expect("put");
    matches_ok(resp);

    wait_for_value(&service2, b"cluster", b"ok".to_vec()).await;
    wait_for_value(&service3, b"cluster", b"ok".to_vec()).await;

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
            .dispatch_to(StorageRequest::Get {
                key: vec![b'k', idx],
            })
            .await
            .expect("get");
        let bytes = expect_ok_bytes(resp);
        assert_eq!(bytes, vec![b'v', idx]);
    }

    service.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn storage_backup_restore_from_blob() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("db").to_string_lossy().to_string();
    let blob_path = format!("{path}.blobs");

    let mut config = config_for_dir(path.clone(), 1);
    config.blob.file_path = blob_path.clone();
    config.backup.enabled = true;
    config.backup.max_logs = 1_000_000;
    config.backup.max_age_secs = 3600;
    config.backup.restore_mode = RestoreMode::Single;
    config.backup.only_leader = false;

    let service = StorageService::start_for_test(config.clone())
        .await
        .expect("start service");

    wait_for_leader(&[&service], &[1]).await;

    let resp = service
        .dispatch_to(StorageRequest::Put {
            key: b"k1".to_vec(),
            value: b"v1".to_vec(),
        })
        .await
        .expect("put");
    matches_ok(resp);
    wait_for_value(&service, b"k1", b"v1".to_vec()).await;
    let _ = service.raft_ref().trigger().snapshot().await;

    let blob = BlobBackend::from_config(&config.blob)
        .await
        .expect("blob backend");
    let prefix = backup_prefix(&config.backup, config.node_id);
    service.shutdown().await;

    let mut restore_id = None;
    for _ in 0..200 {
        let list = blob.list_prefix(&prefix).await.expect("list");
        for entry in list.into_iter().filter(|b| b.key.ends_with(".snap")) {
            if let Ok(snapshot_bytes) = blob.get_named(&entry.key).await {
                if verify_checksum(&blob, &entry.key, &snapshot_bytes, true)
                    .await
                    .is_err()
                {
                    continue;
                }
                if let Ok(snapshot) =
                    serde_json::from_slice::<SerializableKvStateMachine>(&snapshot_bytes)
                {
                    let saw_key = snapshot.data.iter().any(|(key, _)| key.as_slice() == b"k1");
                    if saw_key {
                        restore_id = Some(entry.key);
                        break;
                    }
                }
            }
        }
        if restore_id.is_some() {
            break;
        }
        sleep(Duration::from_millis(20)).await;
    }
    let restore_id = restore_id.expect("backup key");

    let new_dir = tempfile::tempdir().expect("temp dir");
    let new_path = new_dir.path().join("db").to_string_lossy().to_string();
    let mut restore_config = config_for_dir(new_path, 1);
    restore_config.blob.file_path = blob_path;
    restore_config.backup.enabled = true;
    restore_config.backup.restore_mode = RestoreMode::Single;
    restore_config.backup.max_logs = 1;
    restore_config.backup.restore_id = Some(restore_id);

    let restore_service = StorageService::start_for_test(restore_config)
        .await
        .expect("restore service");
    wait_for_leader(&[&restore_service], &[1]).await;
    let resp = restore_service
        .dispatch_to(StorageRequest::Get {
            key: b"k1".to_vec(),
        })
        .await
        .expect("get");
    let bytes = expect_ok_bytes(resp);
    assert_eq!(bytes, b"v1".to_vec());

    restore_service.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn storage_backup_restore_corrupt_id_errors() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("db").to_string_lossy().to_string();
    let blob_path = format!("{path}.blobs");

    let mut config = config_for_dir(path.clone(), 1);
    config.blob.file_path = blob_path.clone();
    config.backup.enabled = true;
    config.backup.max_logs = 1;
    config.backup.max_age_secs = 1;
    config.backup.restore_mode = RestoreMode::Single;
    config.backup.only_leader = false;

    let service = StorageService::start_for_test(config.clone())
        .await
        .expect("start service");
    wait_for_leader(&[&service], &[1]).await;

    let resp = service
        .dispatch_to(StorageRequest::Put {
            key: b"k1".to_vec(),
            value: b"v1".to_vec(),
        })
        .await
        .expect("put");
    matches_ok(resp);
    wait_for_value(&service, b"k1", b"v1".to_vec()).await;

    let blob = BlobBackend::from_config(&config.blob)
        .await
        .expect("blob backend");
    let prefix = backup_prefix(&config.backup, config.node_id);
    let corrupt_key = format!("{}/0000000000001-0-0.snap", prefix);
    blob.put_named(&corrupt_key, b"corrupt")
        .await
        .expect("put corrupt");

    service.shutdown().await;

    let new_dir = tempfile::tempdir().expect("temp dir");
    let new_path = new_dir.path().join("db").to_string_lossy().to_string();
    let mut restore_config = config_for_dir(new_path, 1);
    restore_config.blob.file_path = blob_path;
    restore_config.backup.enabled = true;
    restore_config.backup.restore_mode = RestoreMode::Single;
    restore_config.backup.restore_id = Some(corrupt_key);

    let err = match StorageService::start_for_test(restore_config).await {
        Ok(_) => panic!("expected restore error"),
        Err(err) => err,
    };
    match err {
        StorageError::Internal(_) => {}
        other => panic!("unexpected error: {other:?}"),
    }

    assert!(metrics::get(metrics::METRIC_STORAGE_BACKUP_RESTORE_FAILURE) > 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn storage_backup_restore_specific_id() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("db").to_string_lossy().to_string();
    let blob_path = format!("{path}.blobs");

    let blob = BlobBackend::from_config(&BlobConfig {
        threshold_bytes: 256 * 1024,
        file_path: blob_path.clone(),
        s3: None,
    })
    .await
    .expect("blob backend");

    let mut config = config_for_dir(path.clone(), 1);
    config.blob.file_path = blob_path.clone();
    config.backup.enabled = true;
    config.backup.restore_mode = RestoreMode::Single;

    let prefix = backup_prefix(&config.backup, config.node_id);
    let key_a = format!("{}/{}-0-0.snap", prefix, 1111u64);
    let key_b = format!("{}/{}-0-0.snap", prefix, 2222u64);

    let snap_a = SerializableKvStateMachine {
        last_applied_log: None,
        last_membership: Default::default(),
        data: vec![(
            b"k1".to_vec(),
            crate::storage::value::StoredRecord {
                version: 1,
                value: crate::storage::value::StoredValue::Inline(b"v1".to_vec()),
            },
        )],
    };
    let snap_b = SerializableKvStateMachine {
        last_applied_log: None,
        last_membership: Default::default(),
        data: vec![(
            b"k1".to_vec(),
            crate::storage::value::StoredRecord {
                version: 1,
                value: crate::storage::value::StoredValue::Inline(b"v2".to_vec()),
            },
        )],
    };
    blob.put_named(&key_a, &serde_json::to_vec(&snap_a).unwrap())
        .await
        .expect("put a");
    blob.put_named(&key_b, &serde_json::to_vec(&snap_b).unwrap())
        .await
        .expect("put b");
    let checksum_a = format!("{key_a}.sha256");
    let checksum_b = format!("{key_b}.sha256");
    let hash_a = format!(
        "{:x}",
        sha2::Sha256::digest(serde_json::to_vec(&snap_a).unwrap())
    );
    let hash_b = format!(
        "{:x}",
        sha2::Sha256::digest(serde_json::to_vec(&snap_b).unwrap())
    );
    blob.put_named(&checksum_a, hash_a.as_bytes())
        .await
        .expect("put a checksum");
    blob.put_named(&checksum_b, hash_b.as_bytes())
        .await
        .expect("put b checksum");

    config.backup.restore_id = Some(key_a.clone());

    let service = StorageService::start_for_test(config)
        .await
        .expect("restore service");
    wait_for_leader(&[&service], &[1]).await;

    let resp = service
        .dispatch_to(StorageRequest::Get {
            key: b"k1".to_vec(),
        })
        .await
        .expect("get");
    let bytes = expect_ok_bytes(resp);
    assert_eq!(bytes, b"v1".to_vec());

    service.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn storage_backup_restore_missing_meta() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("db").to_string_lossy().to_string();
    let blob_path = format!("{path}.blobs");

    let blob = BlobBackend::from_config(&BlobConfig {
        threshold_bytes: 256 * 1024,
        file_path: blob_path.clone(),
        s3: None,
    })
    .await
    .expect("blob backend");

    let mut config = config_for_dir(path.clone(), 1);
    config.blob.file_path = blob_path.clone();
    config.backup.enabled = true;
    config.backup.restore_mode = RestoreMode::Single;

    let prefix = backup_prefix(&config.backup, config.node_id);
    let key = format!("{}/{}-0-0.snap", prefix, 3333u64);
    let snap = SerializableKvStateMachine {
        last_applied_log: None,
        last_membership: Default::default(),
        data: vec![(
            b"k1".to_vec(),
            crate::storage::value::StoredRecord {
                version: 1,
                value: crate::storage::value::StoredValue::Inline(b"v1".to_vec()),
            },
        )],
    };
    blob.put_named(&key, &serde_json::to_vec(&snap).unwrap())
        .await
        .expect("put");
    let checksum = format!("{key}.sha256");
    let hash = format!(
        "{:x}",
        sha2::Sha256::digest(serde_json::to_vec(&snap).unwrap())
    );
    blob.put_named(&checksum, hash.as_bytes())
        .await
        .expect("put checksum");

    config.backup.restore_id = Some(key);

    let service = StorageService::start_for_test(config)
        .await
        .expect("restore service");
    wait_for_leader(&[&service], &[1]).await;
    let resp = service
        .dispatch_to(StorageRequest::Get {
            key: b"k1".to_vec(),
        })
        .await
        .expect("get");
    let bytes = expect_ok_bytes(resp);
    assert_eq!(bytes, b"v1".to_vec());

    service.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn storage_backup_retention_prunes() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("db").to_string_lossy().to_string();
    let blob_path = format!("{path}.blobs");

    let mut config = config_for_dir(path.clone(), 1);
    config.blob.file_path = blob_path.clone();
    config.backup.enabled = true;
    config.backup.max_logs = 1;
    config.backup.max_age_secs = 3600;
    config.backup.retention_days = 0;
    config.backup.max_keep = 2;

    let service = StorageService::start_for_test(config.clone())
        .await
        .expect("start service");
    wait_for_leader(&[&service], &[1]).await;

    for idx in 0..4u8 {
        let resp = service
            .dispatch_to(StorageRequest::Put {
                key: vec![b'k', idx],
                value: vec![b'v', idx],
            })
            .await
            .expect("put");
        matches_ok(resp);
        wait_for_value(&service, &[b'k', idx], vec![b'v', idx]).await;
        sleep(Duration::from_millis(30)).await;
    }

    let blob = BlobBackend::from_config(&config.blob)
        .await
        .expect("blob backend");
    let prefix = backup_prefix(&config.backup, config.node_id);
    for _ in 0..100 {
        let list = blob.list_prefix(&prefix).await.expect("list");
        if list.len() <= 2 {
            break;
        }
        sleep(Duration::from_millis(20)).await;
    }
    let list = blob.list_prefix(&prefix).await.expect("list");
    assert!(list.len() <= 2, "expected retention to prune");

    service.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn storage_backup_restore_skips_when_state_exists() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("db").to_string_lossy().to_string();
    let blob_path = format!("{path}.blobs");

    let mut config = config_for_dir(path.clone(), 1);
    config.blob.file_path = blob_path.clone();
    config.backup.enabled = true;
    config.backup.max_logs = 1;
    config.backup.max_age_secs = 1;
    config.backup.restore_mode = RestoreMode::Single;

    let service = StorageService::start_for_test(config.clone())
        .await
        .expect("start service");
    wait_for_leader(&[&service], &[1]).await;

    let resp = service
        .dispatch_to(StorageRequest::Put {
            key: b"k1".to_vec(),
            value: b"v1".to_vec(),
        })
        .await
        .expect("put");
    matches_ok(resp);
    wait_for_value(&service, b"k1", b"v1".to_vec()).await;
    let blob = BlobBackend::from_config(&config.blob)
        .await
        .expect("blob backend");
    let prefix = backup_prefix(&config.backup, config.node_id);
    for _ in 0..100 {
        let list = blob.list_prefix(&prefix).await.expect("list");
        if !list.is_empty() {
            break;
        }
        sleep(Duration::from_millis(20)).await;
    }

    let resp = service
        .dispatch_to(StorageRequest::Put {
            key: b"k2".to_vec(),
            value: b"v2".to_vec(),
        })
        .await
        .expect("put");
    matches_ok(resp);
    wait_for_value(&service, b"k2", b"v2".to_vec()).await;

    let resp = service
        .dispatch_to(StorageRequest::Get {
            key: b"k2".to_vec(),
        })
        .await
        .expect("get");
    let bytes = expect_ok_bytes(resp);
    assert_eq!(bytes, b"v2".to_vec());

    service.shutdown().await;

    let mut restore_config = config_for_dir(path, 1);
    restore_config.blob.file_path = blob_path;
    restore_config.backup.enabled = true;
    restore_config.backup.restore_mode = RestoreMode::Single;

    let restore_service = StorageService::start_for_test(restore_config)
        .await
        .expect("restore service");
    wait_for_leader(&[&restore_service], &[1]).await;
    let resp = restore_service
        .dispatch_to(StorageRequest::Get {
            key: b"k2".to_vec(),
        })
        .await
        .expect("get");
    let bytes = expect_ok_bytes(resp);
    assert_eq!(bytes, b"v2".to_vec());

    restore_service.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn storage_backup_time_trigger() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("db").to_string_lossy().to_string();
    let blob_path = format!("{path}.blobs");

    let mut config = config_for_dir(path.clone(), 10_000);
    config.blob.file_path = blob_path;
    config.backup.enabled = true;
    config.backup.max_logs = 10_000;
    config.backup.max_age_secs = 1;

    let service = StorageService::start_for_test(config.clone())
        .await
        .expect("start service");
    wait_for_leader(&[&service], &[1]).await;

    let resp = service
        .dispatch_to(StorageRequest::Put {
            key: b"k1".to_vec(),
            value: b"v1".to_vec(),
        })
        .await
        .expect("put");
    matches_ok(resp);
    wait_for_value(&service, b"k1", b"v1".to_vec()).await;

    let blob = BlobBackend::from_config(&config.blob)
        .await
        .expect("blob backend");
    let prefix = backup_prefix(&config.backup, config.node_id);
    let mut saw_backup = false;
    for _ in 0..50 {
        let list = blob.list_prefix(&prefix).await.expect("list");
        if !list.is_empty() {
            saw_backup = true;
            break;
        }
        sleep(Duration::from_millis(50)).await;
    }
    assert!(saw_backup, "expected time-based backup");

    service.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn storage_backup_leader_only() {
    let _lock = acquire_cluster_lock().await;
    let _guard = DropReplicationGuard::new();

    let dir = tempfile::tempdir().expect("temp dir");
    let base = dir.path().to_string_lossy().to_string();
    let node1_path = format!("{base}/node1");
    let node2_path = format!("{base}/node2");
    let blob_path = format!("{base}/blobs");

    let Some((listener1, addr1)) = try_bind("127.0.0.1:0").await else {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    };
    let Some((listener2, addr2)) = try_bind("127.0.0.1:0").await else {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    };

    let mut peers = HashMap::new();
    peers.insert(1, addr1.clone());
    peers.insert(2, addr2.clone());

    let mut cfg1 = config_for_node(node1_path, 1, addr1.clone(), peers_for(1, &peers), true, 1);
    cfg1.blob.file_path = blob_path.clone();
    cfg1.backup.enabled = true;
    cfg1.backup.only_leader = true;
    cfg1.backup.max_logs = 1;
    cfg1.backup.max_age_secs = 1;

    let mut cfg2 = config_for_node(node2_path, 2, addr2.clone(), peers_for(2, &peers), false, 1);
    cfg2.blob.file_path = blob_path.clone();
    cfg2.backup.enabled = true;
    cfg2.backup.only_leader = true;
    cfg2.backup.max_logs = 1;
    cfg2.backup.max_age_secs = 1;

    let service1 = StorageService::start_for_test_with_listener(cfg1.clone(), listener1)
        .await
        .expect("start service1");
    let service2 = StorageService::start_for_test_with_listener(cfg2.clone(), listener2)
        .await
        .expect("start service2");

    let leader_id = wait_for_leader(&[&service1, &service2], &[1, 2]).await;
    let leader = if leader_id == 1 { &service1 } else { &service2 };
    let mut leaders_seen = std::collections::HashSet::new();
    for _ in 0..50 {
        if let Some(id) = service1.raft_ref().metrics().borrow().current_leader {
            leaders_seen.insert(id);
        }
        if let Some(id) = service2.raft_ref().metrics().borrow().current_leader {
            leaders_seen.insert(id);
        }
        sleep(Duration::from_millis(20)).await;
    }

    let resp = leader
        .dispatch_to(StorageRequest::Put {
            key: b"k1".to_vec(),
            value: b"v1".to_vec(),
        })
        .await
        .expect("put");
    matches_ok(resp);
    wait_for_value(leader, b"k1", b"v1".to_vec()).await;
    let blob = BlobBackend::from_config(&cfg1.blob)
        .await
        .expect("blob backend");
    let prefix_leader = if leader_id == 1 {
        backup_prefix(&cfg1.backup, 1)
    } else {
        backup_prefix(&cfg2.backup, 2)
    };
    let prefix_follower = if leader_id == 1 {
        backup_prefix(&cfg2.backup, 2)
    } else {
        backup_prefix(&cfg1.backup, 1)
    };

    for _ in 0..50 {
        let list = blob.list_prefix(&prefix_leader).await.expect("list");
        if !list.is_empty() {
            break;
        }
        sleep(Duration::from_millis(20)).await;
    }

    let leader_list = blob.list_prefix(&prefix_leader).await.expect("list");
    let follower_list = blob.list_prefix(&prefix_follower).await.expect("list");
    assert!(
        !(leader_list.is_empty() && follower_list.is_empty()),
        "expected backups for some leader"
    );
    if !leader_list.is_empty() {
        assert!(
            leaders_seen.contains(&leader_id),
            "expected leader backups only for observed leaders"
        );
    }
    if !follower_list.is_empty() {
        let other = if leader_id == 1 { 2 } else { 1 };
        assert!(
            leaders_seen.contains(&other),
            "expected follower backups only if follower became leader"
        );
    }

    service1.shutdown().await;
    service2.shutdown().await;
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

#[tokio::test(flavor = "current_thread")]
async fn storage_large_values_many_keys() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("db").to_string_lossy().to_string();

    let service = StorageService::start_for_test(config_for_dir(path, 10))
        .await
        .expect("start service");

    let value_size = std::env::var("WRELA_STORAGE_LARGE_VALUE_SIZE")
        .ok()
        .and_then(|raw| raw.parse::<usize>().ok())
        .unwrap_or(32 * 1024)
        .max(1024);
    let key_count = std::env::var("WRELA_STORAGE_LARGE_KEY_COUNT")
        .ok()
        .and_then(|raw| raw.parse::<u16>().ok())
        .unwrap_or(64)
        .max(1);
    let probe_idx = key_count / 2;
    let big_value = vec![b'x'; value_size];
    for idx in 0..key_count {
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

    let probe_key = format!("key-{probe_idx}").into_bytes();
    let resp = service
        .dispatch_to(StorageRequest::Get { key: probe_key })
        .await
        .expect("get");
    let bytes = expect_ok_bytes(resp);
    assert_eq!(bytes.len(), big_value.len());

    service.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn storage_blob_roundtrip() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("db").to_string_lossy().to_string();
    let blob_path = format!("{path}.blobs");
    let config = config_for_dir_with_threshold(path, 10, 32);

    let service = StorageService::start_for_test(config)
        .await
        .expect("start service");

    let value = vec![b'x'; 1024];
    let resp = service
        .dispatch_to(StorageRequest::Put {
            key: b"blob-key".to_vec(),
            value: value.clone(),
        })
        .await
        .expect("put");
    matches_ok(resp);

    let resp = service
        .dispatch_to(StorageRequest::Get {
            key: b"blob-key".to_vec(),
        })
        .await
        .expect("get");
    let bytes = expect_ok_bytes(resp);
    assert_eq!(bytes, value);

    let files = count_blob_files(std::path::Path::new(&blob_path));
    assert!(files >= 1, "expected blob file to be created");

    service.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn storage_blob_delete_removes_object() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("db").to_string_lossy().to_string();
    let blob_path = format!("{path}.blobs");
    let config = config_for_dir_with_threshold(path, 10, 32);

    let service = StorageService::start_for_test(config)
        .await
        .expect("start service");

    let value = vec![b'y'; 1024];
    let resp = service
        .dispatch_to(StorageRequest::Put {
            key: b"delete-key".to_vec(),
            value: value.clone(),
        })
        .await
        .expect("put");
    matches_ok(resp);

    let resp = service
        .dispatch_to(StorageRequest::Delete {
            key: b"delete-key".to_vec(),
        })
        .await
        .expect("delete");
    matches_ok(resp);

    let files = count_blob_files(std::path::Path::new(&blob_path));
    assert_eq!(files, 0, "expected blob file to be deleted");

    service.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn storage_blob_overwrite_deletes_prior_object() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("db").to_string_lossy().to_string();
    let blob_path = format!("{path}.blobs");
    let config = config_for_dir_with_threshold(path, 10, 32);

    let service = StorageService::start_for_test(config)
        .await
        .expect("start service");

    let resp = service
        .dispatch_to(StorageRequest::Put {
            key: b"overwrite-key".to_vec(),
            value: vec![b'a'; 1024],
        })
        .await
        .expect("put");
    matches_ok(resp);

    let resp = service
        .dispatch_to(StorageRequest::Put {
            key: b"overwrite-key".to_vec(),
            value: vec![b'b'; 1024],
        })
        .await
        .expect("put2");
    matches_ok(resp);

    let files = count_blob_files(std::path::Path::new(&blob_path));
    assert_eq!(files, 1, "expected only one blob after overwrite");

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

    add_learner_with_retry(
        service1.raft_ref(),
        2,
        BasicNode {
            addr: addr2.clone(),
        },
    )
    .await;
    add_learner_with_retry(
        service1.raft_ref(),
        3,
        BasicNode {
            addr: addr3.clone(),
        },
    )
    .await;
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

    let active_refs: Vec<&StorageService> = active_services.iter().map(|(_, svc)| *svc).collect();
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

    add_learner_with_retry(
        service1.raft_ref(),
        2,
        BasicNode {
            addr: addr2.clone(),
        },
    )
    .await;
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

    add_learner_with_retry(
        service1.raft_ref(),
        3,
        BasicNode {
            addr: addr3.clone(),
        },
    )
    .await;
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

    add_learner_with_retry(
        service1.raft_ref(),
        2,
        BasicNode {
            addr: addr2.clone(),
        },
    )
    .await;
    add_learner_with_retry(
        service1.raft_ref(),
        3,
        BasicNode {
            addr: addr3.clone(),
        },
    )
    .await;
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

    let active_refs: Vec<&StorageService> = active_services.iter().map(|(_, svc)| *svc).collect();
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
async fn storage_partition_recovers_and_replicates() {
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

    add_learner_with_retry(
        service1.raft_ref(),
        2,
        BasicNode {
            addr: addr2.clone(),
        },
    )
    .await;
    add_learner_with_retry(
        service1.raft_ref(),
        3,
        BasicNode {
            addr: addr3.clone(),
        },
    )
    .await;
    change_membership_with_retry(service1.raft_ref(), [1u64, 2u64, 3u64]).await;
    wait_for_membership_contains(service2.raft_ref(), 2).await;
    wait_for_membership_contains(service3.raft_ref(), 3).await;

    let leader_id = wait_for_leader(&[&service1, &service2, &service3], &[1, 2, 3]).await;
    let leader = match leader_id {
        1 => &service1,
        2 => &service2,
        3 => &service3,
        _ => unreachable!(),
    };

    set_drop_replication(true);
    let result = tokio::time::timeout(
        Duration::from_millis(200),
        leader.dispatch_to(StorageRequest::Put {
            key: b"partitioned".to_vec(),
            value: b"nope".to_vec(),
        }),
    )
    .await;
    match result {
        Ok(Ok(StorageResponse::Ok(_))) => panic!("write unexpectedly committed"),
        _ => {}
    }
    set_drop_replication(false);

    let leader_id = wait_for_leader(&[&service1, &service2, &service3], &[1, 2, 3]).await;
    let leader = match leader_id {
        1 => &service1,
        2 => &service2,
        3 => &service3,
        _ => unreachable!(),
    };

    let resp = leader
        .dispatch_to(StorageRequest::Put {
            key: b"partitioned".to_vec(),
            value: b"ok".to_vec(),
        })
        .await
        .expect("put");
    matches_ok(resp);

    wait_for_value(&service1, b"partitioned", b"ok".to_vec()).await;
    wait_for_value(&service2, b"partitioned", b"ok".to_vec()).await;
    wait_for_value(&service3, b"partitioned", b"ok".to_vec()).await;

    service1.shutdown().await;
    service2.shutdown().await;
    service3.shutdown().await;
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

    add_learner_with_retry(
        service1.raft_ref(),
        2,
        BasicNode {
            addr: addr2.clone(),
        },
    )
    .await;
    add_learner_with_retry(
        service1.raft_ref(),
        3,
        BasicNode {
            addr: addr3.clone(),
        },
    )
    .await;
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

    let active_refs: Vec<&StorageService> = active_services.iter().map(|(_, svc)| *svc).collect();
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

    add_learner_with_retry(
        service1.raft_ref(),
        2,
        BasicNode {
            addr: addr2.clone(),
        },
    )
    .await;
    add_learner_with_retry(
        service1.raft_ref(),
        3,
        BasicNode {
            addr: addr3.clone(),
        },
    )
    .await;
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

    add_learner_with_retry(
        service1.raft_ref(),
        2,
        BasicNode {
            addr: addr2.clone(),
        },
    )
    .await;
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
        if tries > 200 {
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
        if tries > 600 {
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
