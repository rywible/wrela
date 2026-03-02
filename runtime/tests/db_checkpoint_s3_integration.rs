use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use wrela_runtime::db::checkpoint::{CheckpointBackend, CheckpointConfig};

const MINIO_USER: &str = "minioadmin";
const MINIO_PASS: &str = "minioadmin";
const TEST_REGION: &str = "us-east-1";

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn unique_name(prefix: &str) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("epoch")
        .as_nanos();
    format!("{}-{}-{}", prefix, std::process::id(), now)
}

fn temp_dir(prefix: &str) -> PathBuf {
    let root = std::env::temp_dir().join(unique_name(prefix));
    std::fs::create_dir_all(&root).expect("mkdir");
    root
}

struct Minio {
    container_id: String,
    endpoint: String,
}

impl Minio {
    fn start() -> Option<Self> {
        let docker_ok = Command::new("docker")
            .args(["version", "--format", "{{.Server.Version}}"])
            .output()
            .ok()
            .map(|out| out.status.success())
            .unwrap_or(false);
        if !docker_ok {
            eprintln!("skipping: docker daemon unavailable");
            return None;
        }

        let run = Command::new("docker")
            .args([
                "run",
                "-d",
                "--rm",
                "-p",
                "127.0.0.1::9000",
                "-e",
                "MINIO_ROOT_USER=minioadmin",
                "-e",
                "MINIO_ROOT_PASSWORD=minioadmin",
                "-e",
                "MINIO_REGION_NAME=us-east-1",
                "minio/minio:latest",
                "server",
                "/data",
            ])
            .output()
            .expect("start minio");
        if !run.status.success() {
            eprintln!("skipping: failed to start minio");
            return None;
        }
        let container_id = String::from_utf8_lossy(&run.stdout).trim().to_string();

        let port_out = Command::new("docker")
            .args(["port", &container_id, "9000/tcp"])
            .output()
            .expect("port map");
        if !port_out.status.success() {
            let _ = Command::new("docker")
                .args(["kill", &container_id])
                .status();
            eprintln!("skipping: failed to resolve minio port");
            return None;
        }
        let port_line = String::from_utf8_lossy(&port_out.stdout).trim().to_string();
        let port = match port_line.rsplit(':').next() {
            Some(v) if !v.is_empty() => v,
            _ => {
                let _ = Command::new("docker")
                    .args(["kill", &container_id])
                    .status();
                eprintln!("skipping: malformed minio port mapping");
                return None;
            }
        };
        let endpoint = format!("http://127.0.0.1:{port}");

        let start = std::time::Instant::now();
        loop {
            let health = Command::new("curl")
                .args(["-fsS", &format!("{endpoint}/minio/health/live")])
                .output();
            if health.as_ref().map(|o| o.status.success()).unwrap_or(false) {
                break;
            }
            if start.elapsed() > Duration::from_secs(30) {
                let _ = Command::new("docker")
                    .args(["kill", &container_id])
                    .status();
                eprintln!("skipping: minio did not become healthy");
                return None;
            }
            std::thread::sleep(Duration::from_millis(250));
        }

        Some(Self {
            container_id,
            endpoint,
        })
    }

    fn bucket_client(&self) -> aws_sdk_s3::Client {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio");
        rt.block_on(async {
            let loader = aws_config::defaults(aws_config::BehaviorVersion::latest())
                .region(aws_sdk_s3::config::Region::new(TEST_REGION))
                .endpoint_url(self.endpoint.clone())
                .credentials_provider(aws_sdk_s3::config::Credentials::new(
                    MINIO_USER,
                    MINIO_PASS,
                    None,
                    None,
                    "integration-test",
                ));
            let loaded = loader.load().await;
            let conf = aws_sdk_s3::config::Builder::from(&loaded)
                .force_path_style(true)
                .endpoint_url(self.endpoint.clone())
                .build();
            aws_sdk_s3::Client::from_conf(conf)
        })
    }

    fn create_bucket(&self, bucket: &str) {
        let client = self.bucket_client();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio");
        rt.block_on(async {
            client
                .create_bucket()
                .bucket(bucket)
                .send()
                .await
                .expect("create bucket");
        });
    }

    fn delete_object(&self, bucket: &str, key: &str) {
        let client = self.bucket_client();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio");
        rt.block_on(async {
            client
                .delete_object()
                .bucket(bucket)
                .key(key)
                .send()
                .await
                .expect("delete object");
        });
    }
}

impl Drop for Minio {
    fn drop(&mut self) {
        let _ = Command::new("docker")
            .args(["kill", &self.container_id])
            .status();
    }
}

fn write_fixture_data(data_dir: &Path, wal: &[u8]) {
    std::fs::create_dir_all(data_dir).expect("mkdir data");
    std::fs::write(data_dir.join("wal.log"), wal).expect("wal");
    std::fs::write(data_dir.join("raft_state.json"), br#"{"term":1}"#).expect("raft");
    std::fs::write(data_dir.join("hlc_state.json"), br#"{"hlc":1}"#).expect("hlc");
    std::fs::write(data_dir.join("cdc_checkpoints.json"), br#"{"orders":1}"#).expect("cdc");
    std::fs::write(data_dir.join("schema_epoch.json"), br#"{"epoch":1}"#).expect("schema");
    std::fs::write(data_dir.join("snapshot.bin"), b"snapshot").expect("snapshot");
}

fn build_s3_manager(
    checkpoint_dir: PathBuf,
    bucket: String,
    prefix: String,
    endpoint: String,
) -> wrela_runtime::db::checkpoint::CheckpointManager {
    let cfg = CheckpointConfig {
        backend: CheckpointBackend::S3,
        checkpoint_dir,
        local_region: None,
        s3_bucket: Some(bucket),
        s3_prefix: Some(prefix),
        s3_region: Some(TEST_REGION.to_string()),
        s3_endpoint: Some(endpoint),
        s3_path_style: true,
        s3_bucket_by_region: std::collections::BTreeMap::new(),
        s3_region_by_region: std::collections::BTreeMap::new(),
        s3_endpoint_by_region: std::collections::BTreeMap::new(),
        env_parse_error: None,
        interval_secs: 60,
        retain_local: 3,
        allowed_regions: Vec::new(),
    };
    cfg.build_manager().expect("build manager")
}

#[test]
fn s3_restore_recovers_from_local_checksum_corruption() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(minio) = Minio::start() else {
        return;
    };

    let bucket = unique_name("wreladb-ckpt-bucket");
    let prefix = unique_name("wreladb/ckpt");
    minio.create_bucket(&bucket);

    unsafe {
        std::env::set_var("AWS_ACCESS_KEY_ID", MINIO_USER);
        std::env::set_var("AWS_SECRET_ACCESS_KEY", MINIO_PASS);
        std::env::set_var("AWS_EC2_METADATA_DISABLED", "true");
    }

    let data_dir = temp_dir("wreladb-data");
    let checkpoint_dir = temp_dir("wreladb-checkpoints");
    write_fixture_data(&data_dir, b"wal-original");

    let manager = build_s3_manager(
        checkpoint_dir.clone(),
        bucket.clone(),
        prefix.clone(),
        minio.endpoint.clone(),
    );
    let info = manager
        .create_checkpoint(&data_dir)
        .expect("checkpoint create");

    let local_wal = checkpoint_dir
        .join("checkpoints")
        .join(&info.checkpoint_id)
        .join("wal.log");
    std::fs::write(&local_wal, b"wal-corrupted").expect("corrupt local wal");
    std::fs::remove_file(data_dir.join("wal.log")).expect("delete wal");

    manager.restore_latest(&data_dir).expect("restore latest");
    let wal = std::fs::read(data_dir.join("wal.log")).expect("read restored wal");
    assert_eq!(wal, b"wal-original");
    let repaired_local = std::fs::read(local_wal).expect("repaired local wal");
    assert_eq!(repaired_local, b"wal-original");
}

#[test]
fn s3_restore_skips_partial_latest_and_uses_previous_valid_checkpoint() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(minio) = Minio::start() else {
        return;
    };

    let bucket = unique_name("wreladb-ckpt-bucket");
    let prefix = unique_name("wreladb/ckpt");
    minio.create_bucket(&bucket);

    unsafe {
        std::env::set_var("AWS_ACCESS_KEY_ID", MINIO_USER);
        std::env::set_var("AWS_SECRET_ACCESS_KEY", MINIO_PASS);
        std::env::set_var("AWS_EC2_METADATA_DISABLED", "true");
    }

    let data_dir = temp_dir("wreladb-data");
    let checkpoint_dir = temp_dir("wreladb-checkpoints");
    let manager = build_s3_manager(
        checkpoint_dir.clone(),
        bucket.clone(),
        prefix.clone(),
        minio.endpoint.clone(),
    );

    write_fixture_data(&data_dir, b"wal-v1");
    let c1 = manager.create_checkpoint(&data_dir).expect("c1");
    std::thread::sleep(Duration::from_secs(1));

    write_fixture_data(&data_dir, b"wal-v2");
    let c2 = manager.create_checkpoint(&data_dir).expect("c2");

    let bad_manifest_key = format!("{prefix}/checkpoints/{}/manifest.json", c2.checkpoint_id);
    minio.delete_object(&bucket, &bad_manifest_key);

    let _ = std::fs::remove_file(checkpoint_dir.join("LATEST"));
    let _ = std::fs::remove_dir_all(checkpoint_dir.join("checkpoints"));
    std::fs::remove_file(data_dir.join("wal.log")).expect("drop wal before restore");

    let restored = manager
        .restore_latest(&data_dir)
        .expect("restore from prior valid");
    assert_eq!(restored.checkpoint_id, c1.checkpoint_id);
    let wal = std::fs::read(data_dir.join("wal.log")).expect("read restored wal");
    assert_eq!(wal, b"wal-v1");
}
