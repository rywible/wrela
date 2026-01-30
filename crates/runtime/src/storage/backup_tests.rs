use std::sync::Arc;

use tokio::time::Duration;

use super::backup::{backup_prefix, prune_backups_for_test, BackupSink, BackupState};
use super::blob::BlobBackend;
use super::config::{BackupConfig, RestoreMode};
use crate::metrics;
use openraft::SnapshotMeta;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn backup_prunes_by_age() {
    let dir = tempfile::tempdir().expect("temp dir");
    let blob_path = dir.path().join("blobs").to_string_lossy().to_string();
    let blob = BlobBackend::from_config(&super::config::BlobConfig {
        threshold_bytes: 256 * 1024,
        file_path: blob_path,
        s3: None,
    })
    .await
    .expect("blob backend");

    let config = BackupConfig {
        enabled: true,
        max_age_secs: 3600,
        max_logs: 100,
        retention_days: 1,
        max_keep: 0,
        prefix: "backups".to_string(),
        only_leader: true,
        restore_mode: RestoreMode::Single,
        restore_id: None,
    };
    let prefix = backup_prefix(&config, 1);

    let old_key = format!("{}/{}-0-0.snap", prefix, 0);
    let new_key = format!("{}/{}-0-0.snap", prefix, 325036800000u64); // year 3000
    let _ = blob.put_named(&old_key, b"old").await.expect("put");
    let _ = blob.put_named(&new_key, b"new").await.expect("put");

    prune_backups_for_test(&blob, &config, 1)
        .await
        .expect("prune");

    let list = blob.list_prefix(&prefix).await.expect("list");
    let keys: Vec<_> = list.into_iter().map(|b| b.key).collect();
    assert!(keys.iter().any(|k| k == &new_key));
    assert!(!keys.iter().any(|k| k == &old_key));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn backup_skips_upload_when_not_leader() {
    let dir = tempfile::tempdir().expect("temp dir");
    let blob_path = dir.path().join("blobs").to_string_lossy().to_string();
    let blob = BlobBackend::from_config(&super::config::BlobConfig {
        threshold_bytes: 256 * 1024,
        file_path: blob_path,
        s3: None,
    })
    .await
    .expect("blob backend");

    let config = BackupConfig {
        enabled: true,
        max_age_secs: 1,
        max_logs: 1,
        retention_days: 7,
        max_keep: 0,
        prefix: "backups".to_string(),
        only_leader: true,
        restore_mode: RestoreMode::Single,
        restore_id: None,
    };
    let state = Arc::new(BackupState::new());
    state.set_leader_id(Some(2));
    let sink = BackupSink::new(blob.clone(), config.clone(), state, 1);

    let meta = SnapshotMeta {
        last_log_id: None,
        last_membership: Default::default(),
        snapshot_id: "test".to_string(),
    };
    sink.on_snapshot(meta, b"data".to_vec());
    tokio::time::sleep(Duration::from_millis(50)).await;

    let prefix = backup_prefix(&config, 1);
    let list = blob.list_prefix(&prefix).await.expect("list");
    assert!(list.is_empty(), "expected upload skipped");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn backup_prunes_by_age_and_keep() {
    let dir = tempfile::tempdir().expect("temp dir");
    let blob_path = dir.path().join("blobs").to_string_lossy().to_string();
    let blob = BlobBackend::from_config(&super::config::BlobConfig {
        threshold_bytes: 256 * 1024,
        file_path: blob_path,
        s3: None,
    })
    .await
    .expect("blob backend");

    let config = BackupConfig {
        enabled: true,
        max_age_secs: 3600,
        max_logs: 100,
        retention_days: 1,
        max_keep: 1,
        prefix: "backups".to_string(),
        only_leader: true,
        restore_mode: RestoreMode::Single,
        restore_id: None,
    };
    let prefix = backup_prefix(&config, 1);

    let old_key = format!("{}/{}-0-0.snap", prefix, 0u64);
    let mid_key = format!("{}/{}-0-0.snap", prefix, 1000u64);
    let new_key = format!("{}/{}-0-0.snap", prefix, 325036800000u64);
    let _ = blob.put_named(&old_key, b"old").await.expect("put");
    let _ = blob.put_named(&mid_key, b"mid").await.expect("put");
    let _ = blob.put_named(&new_key, b"new").await.expect("put");

    prune_backups_for_test(&blob, &config, 1)
        .await
        .expect("prune");

    let list = blob.list_prefix(&prefix).await.expect("list");
    let keys: Vec<_> = list.into_iter().map(|b| b.key).collect();
    assert!(keys.iter().any(|k| k == &new_key));
    assert_eq!(keys.len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn backup_upload_failure_does_not_panic() {
    let dir = tempfile::tempdir().expect("temp dir");
    let base = dir.path().join("blobs");
    std::fs::write(&base, b"not a dir").expect("write base file");

    let blob = BlobBackend::from_config(&super::config::BlobConfig {
        threshold_bytes: 256 * 1024,
        file_path: base.to_string_lossy().to_string(),
        s3: None,
    })
    .await
    .expect("blob backend");

    let config = BackupConfig {
        enabled: true,
        max_age_secs: 1,
        max_logs: 1,
        retention_days: 7,
        max_keep: 0,
        prefix: "backups".to_string(),
        only_leader: true,
        restore_mode: RestoreMode::Single,
        restore_id: None,
    };
    let state = Arc::new(BackupState::new());
    state.set_leader_id(Some(1));
    let sink = BackupSink::new(blob.clone(), config.clone(), state, 1);

    let meta = SnapshotMeta {
        last_log_id: None,
        last_membership: Default::default(),
        snapshot_id: "test".to_string(),
    };
    sink.on_snapshot(meta, b"data".to_vec());
    tokio::time::sleep(Duration::from_millis(50)).await;

    let prefix = backup_prefix(&config, 1);
    assert!(blob.list_prefix(&prefix).await.is_err());
    assert!(metrics::get(metrics::METRIC_STORAGE_BACKUP_FAILURE) > 0);
}
