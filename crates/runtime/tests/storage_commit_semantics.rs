use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use reqwest::Client;
use serde::Deserialize;
use serde::Serialize;
use tempfile::TempDir;

#[derive(Deserialize)]
struct RpcEnvelope<T> {
    ok: bool,
    data: Option<T>,
    error: Option<String>,
}

#[derive(Deserialize)]
struct StorageReadResponse {
    value: Option<Vec<u8>>,
}

#[derive(Deserialize)]
struct StorageLeaderResponse {
    node_id: u64,
    leader_id: Option<u64>,
}

struct NodeProc {
    child: Child,
    rx: Receiver<String>,
}

impl NodeProc {
    fn wait_for(&mut self, needle: &str, timeout: Duration) -> String {
        let start = Instant::now();
        let mut last_line = None;
        while start.elapsed() < timeout {
            if let Ok(line) = self.rx.recv_timeout(Duration::from_millis(50)) {
                last_line = Some(line.clone());
                if line.contains(needle) {
                    return line;
                }
            }
            if let Ok(Some(status)) = self.child.try_wait() {
                let detail = last_line.unwrap_or_else(|| "<no output>".to_string());
                panic!(
                    "storage_node exited before {needle} (status: {status}): {detail}"
                );
            }
        }
        panic!("timed out waiting for {needle}");
    }

    fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for NodeProc {
    fn drop(&mut self) {
        self.kill();
    }
}

fn spawn_node(
    dir: &TempDir,
    node_id: u64,
    bind_addr: &str,
    peers: Vec<(u64, String)>,
    bootstrap: bool,
    init_cluster: bool,
    write: Option<(&str, &str)>,
    drop_replication: bool,
) -> NodeProc {
    let Some(bin) = option_env!("CARGO_BIN_EXE_storage_node") else {
        panic!(
            "storage_node binary missing; run with `--features test-utils` or ensure the bin is built"
        );
    };

    let mut cmd = Command::new(bin);
    cmd.arg("--node-id")
        .arg(node_id.to_string())
        .arg("--bind-addr")
        .arg(bind_addr)
        .arg("--path")
        .arg(dir.path().join(format!("db{node_id}")).to_string_lossy().to_string())
        .arg("--bootstrap")
        .arg(if bootstrap { "true" } else { "false" });

    for (id, addr) in peers {
        cmd.arg("--peer").arg(format!("{id}={addr}"));
    }

    if init_cluster {
        cmd.arg("--init-cluster");
    }

    if let Some((key, value)) = write {
        cmd.arg("--write").arg(key).arg(value);
    }

    if drop_replication {
        cmd.env("WRELA_TEST_DROP_REPLICATION", "1");
    }

    cmd.stdout(Stdio::piped()).stderr(Stdio::inherit());
    let mut child = cmd.spawn().expect("spawn storage_node");
    let stdout = child.stdout.take().expect("stdout");
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines().flatten() {
            let _ = tx.send(line);
        }
    });

    NodeProc { child, rx }
}

fn pick_free_addr() -> Option<String> {
    match std::net::TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => listener
            .local_addr()
            .ok()
            .map(|addr| addr.to_string()),
        Err(err) => {
            if err.kind() == std::io::ErrorKind::PermissionDenied {
                None
            } else {
                panic!("bind: {err}");
            }
        }
    }
}

async fn wait_for_leader(addrs: &[String]) -> String {
    let client = Client::new();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        for addr in addrs {
            let url = format!("http://{addr}/storage/leader");
            let resp = client.post(url).send().await;
            if let Ok(resp) = resp {
                if let Ok(env) = resp.json::<RpcEnvelope<StorageLeaderResponse>>().await {
                    if env.ok {
                        if let Some(data) = env.data {
                            if data.leader_id == Some(data.node_id) {
                                return addr.clone();
                            }
                        }
                    }
                }
            }
        }
        if Instant::now() > deadline {
            panic!("timed out waiting for leader");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn read_value_once(addr: &str, key: &[u8]) -> Result<Option<Vec<u8>>, String> {
    let client = Client::new();
    let url = format!("http://{addr}/storage/read");
    let resp = client
        .post(url)
        .json(&StorageReadRequest { key: key.to_vec() })
        .send()
        .await
        .map_err(|err| format!("read request: {err}"))?;
    let env: RpcEnvelope<StorageReadResponse> = resp
        .json()
        .await
        .map_err(|err| format!("read response: {err}"))?;
    if env.ok {
        Ok(env.data.and_then(|d| d.value))
    } else {
        Err(format!("read failed: {:?}", env.error))
    }
}

async fn read_value_with_retry(addrs: &[String], key: &[u8]) -> Option<Vec<u8>> {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut last_err = None::<String>;
    loop {
        let leader_addr = wait_for_leader(addrs).await;
        match read_value_once(&leader_addr, key).await {
            Ok(value) => return value,
            Err(err) => last_err = Some(err),
        }
        if Instant::now() > deadline {
            panic!(
                "read failed after retries: {}",
                last_err.unwrap_or_else(|| "unknown error".to_string())
            );
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[derive(Serialize)]
struct StorageReadRequest {
    key: Vec<u8>,
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn integration_uncommitted_write_is_discarded() {
    let dir = tempfile::tempdir().expect("temp dir");

    let Some(addr1) = pick_free_addr() else {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    };
    let Some(addr2) = pick_free_addr() else {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    };
    let Some(addr3) = pick_free_addr() else {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    };

    let mut node2 = spawn_node(
        &dir,
        2,
        &addr2,
        vec![],
        false,
        false,
        None,
        false,
    );
    node2.wait_for("READY", Duration::from_secs(5));

    let mut node3 = spawn_node(
        &dir,
        3,
        &addr3,
        vec![],
        false,
        false,
        None,
        false,
    );
    node3.wait_for("READY", Duration::from_secs(5));

    let mut node1 = spawn_node(
        &dir,
        1,
        &addr1,
        vec![(2, addr2.clone()), (3, addr3.clone())],
        true,
        true,
        Some(("pending", "nope")),
        true,
    );
    node1.wait_for("READY", Duration::from_secs(5));
    node1.wait_for("CLUSTER_READY", Duration::from_secs(5));
    node1.wait_for("WRITE_STARTED", Duration::from_secs(5));

    tokio::time::sleep(Duration::from_millis(200)).await;
    node1.kill();

    let addrs = [addr1, addr2.clone(), addr3.clone()];
    let value = read_value_with_retry(&addrs, b"pending").await;
    assert!(value.is_none(), "uncommitted write became visible");

    node2.kill();
    node3.kill();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn integration_committed_write_survives_leader_crash() {
    let dir = tempfile::tempdir().expect("temp dir");

    let Some(addr1) = pick_free_addr() else {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    };
    let Some(addr2) = pick_free_addr() else {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    };
    let Some(addr3) = pick_free_addr() else {
        eprintln!("skipping: unable to bind sockets in this environment");
        return;
    };

    let mut node2 = spawn_node(
        &dir,
        2,
        &addr2,
        vec![],
        false,
        false,
        None,
        false,
    );
    node2.wait_for("READY", Duration::from_secs(5));

    let mut node3 = spawn_node(
        &dir,
        3,
        &addr3,
        vec![],
        false,
        false,
        None,
        false,
    );
    node3.wait_for("READY", Duration::from_secs(5));

    let mut node1 = spawn_node(
        &dir,
        1,
        &addr1,
        vec![(2, addr2.clone()), (3, addr3.clone())],
        true,
        true,
        Some(("survive", "yes")),
        false,
    );
    node1.wait_for("READY", Duration::from_secs(5));
    node1.wait_for("CLUSTER_READY", Duration::from_secs(5));
    node1.wait_for("WRITE_OK", Duration::from_secs(5));

    tokio::time::sleep(Duration::from_millis(200)).await;
    node1.kill();

    let addrs = [addr1, addr2.clone(), addr3.clone()];
    let value = read_value_with_retry(&addrs, b"survive").await;
    assert_eq!(value, Some(b"yes".to_vec()));

    node2.kill();
    node3.kill();
}
