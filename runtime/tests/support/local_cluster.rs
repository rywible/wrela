use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::ErrorKind;
use std::net::TcpListener;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use wrela_runtime::tokio_runtime;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_millis(150);
const LOCAL_NODE_IDS: [&str; 3] = ["node-a", "node-b", "node-c"];

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub body: Value,
    pub raw_body: String,
}

#[derive(Debug, Clone)]
struct NodeSpec {
    id: String,
    http_port: u16,
    private_rpc_port: u16,
    data_dir: PathBuf,
    stdout_log: PathBuf,
    stderr_log: PathBuf,
}

#[derive(Debug)]
struct NodeProcess {
    spec: NodeSpec,
    child: Child,
}

impl NodeProcess {
    fn try_wait(&mut self) -> Result<Option<ExitStatus>, String> {
        self.child
            .try_wait()
            .map_err(|err| format!("failed to check process state for {}: {err}", self.spec.id))
    }

    fn stop(&mut self) -> Result<(), String> {
        match self.try_wait()? {
            Some(_) => return Ok(()),
            None => {}
        }
        if let Err(err) = self.child.kill()
            && err.kind() != ErrorKind::InvalidInput
        {
            return Err(format!("failed to kill node {}: {err}", self.spec.id));
        }
        self.child
            .wait()
            .map_err(|err| format!("failed to wait for node {} shutdown: {err}", self.spec.id))?;
        Ok(())
    }
}

pub struct LocalCluster {
    _run_dir: tempfile::TempDir,
    node_specs: BTreeMap<String, NodeSpec>,
    nodes: BTreeMap<String, NodeProcess>,
    binary_path: PathBuf,
    cluster_nodes_env: String,
    address_map_env: String,
    node_extra_env: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct LocalClusterConfig {
    pub node_ids: Vec<String>,
    pub binary_path: Option<PathBuf>,
    pub extra_env: BTreeMap<String, String>,
}

impl Default for LocalClusterConfig {
    fn default() -> Self {
        Self {
            node_ids: LOCAL_NODE_IDS
                .iter()
                .map(|node| (*node).to_string())
                .collect(),
            binary_path: None,
            extra_env: BTreeMap::new(),
        }
    }
}

#[allow(dead_code)]
impl LocalCluster {
    pub fn boot_default() -> Result<Self, String> {
        Self::boot_with_config(LocalClusterConfig::default())
    }

    pub fn boot_with_config(config: LocalClusterConfig) -> Result<Self, String> {
        let workspace_root = workspace_root()?;
        let run_dir = tempfile::tempdir().map_err(|err| format!("tempdir failed: {err}"))?;
        let logs_dir = run_dir.path().join("logs");
        fs::create_dir_all(&logs_dir).map_err(|err| format!("create logs dir failed: {err}"))?;

        let binary_path = if let Some(path) = config.binary_path.clone() {
            path
        } else {
            let built = run_dir.path().join("wrela_local_cluster_node");
            build_smoke_binary(&workspace_root, &built)?;
            built
        };
        if !binary_path.is_file() {
            return Err(format!(
                "local cluster binary does not exist at {}",
                binary_path.display()
            ));
        }

        let mut node_ids = config.node_ids;
        if node_ids.is_empty() {
            node_ids = LOCAL_NODE_IDS
                .iter()
                .map(|node| (*node).to_string())
                .collect();
        }
        node_ids.sort();
        node_ids.dedup();

        let mut node_specs = BTreeMap::new();
        for node_id in node_ids {
            let spec = NodeSpec {
                id: node_id.clone(),
                http_port: reserve_local_port()?,
                private_rpc_port: reserve_local_port()?,
                data_dir: run_dir.path().join(format!("data-{node_id}")),
                stdout_log: logs_dir.join(format!("{node_id}.stdout.log")),
                stderr_log: logs_dir.join(format!("{node_id}.stderr.log")),
            };
            node_specs.insert(spec.id.clone(), spec);
        }

        let cluster_nodes_env = node_specs.keys().cloned().collect::<Vec<_>>().join(",");
        let address_map_env = node_specs
            .values()
            .map(|spec| format!("{}=127.0.0.1:{}", spec.id, spec.private_rpc_port))
            .collect::<Vec<_>>()
            .join(",");

        let mut nodes = BTreeMap::new();
        for spec in node_specs.values() {
            let process = spawn_node(
                &binary_path,
                spec.clone(),
                &cluster_nodes_env,
                &address_map_env,
                &config.extra_env,
            )?;
            nodes.insert(spec.id.clone(), process);
        }

        Ok(Self {
            _run_dir: run_dir,
            node_specs,
            nodes,
            binary_path,
            cluster_nodes_env,
            address_map_env,
            node_extra_env: config.extra_env,
        })
    }

    pub fn node_ids(&self) -> Vec<String> {
        self.node_specs.keys().cloned().collect()
    }

    pub fn node_endpoints(&self) -> BTreeMap<String, String> {
        self.node_specs
            .iter()
            .map(|(id, spec)| (id.clone(), format!("http://127.0.0.1:{}", spec.http_port)))
            .collect()
    }

    pub fn request_json_url(
        base_url: &str,
        method: &str,
        path: &str,
    ) -> Result<HttpResponse, String> {
        let full_url = format!("{base_url}{path}");
        request_json(method, &full_url)
    }

    pub fn probe_health(&mut self, node_id: &str) -> Result<HttpResponse, String> {
        self.request_node(node_id, "GET", "/api/health")
    }

    pub fn probe_live(&mut self, node_id: &str) -> Result<HttpResponse, String> {
        self.request_node(node_id, "GET", "/api/live")
    }

    pub fn probe_mesh(&mut self, node_id: &str) -> Result<HttpResponse, String> {
        self.request_node(node_id, "GET", "/api/probe/mesh")
    }

    pub fn probe_write(&mut self, node_id: &str) -> Result<HttpResponse, String> {
        self.request_node(node_id, "POST", "/api/probe/write")
    }

    pub fn probe_read(&mut self, node_id: &str) -> Result<HttpResponse, String> {
        self.request_node(node_id, "GET", "/api/probe/read")
    }

    pub fn probe_read_direct(&mut self, node_id: &str) -> Result<HttpResponse, String> {
        self.request_node(node_id, "GET", "/api/probe/read_direct")
    }

    pub fn load_write(
        &mut self,
        node_id: &str,
        worker: &str,
        sequence: usize,
    ) -> Result<HttpResponse, String> {
        self.request_node(
            node_id,
            "POST",
            &format!("/api/load/write?worker={worker}&seq={sequence}"),
        )
    }

    pub fn load_write_raw(
        &mut self,
        node_id: &str,
        worker: &str,
        sequence: usize,
    ) -> Result<HttpResponse, String> {
        self.request_node(
            node_id,
            "POST",
            &format!("/api/load/write_raw?worker={worker}&seq={sequence}"),
        )
    }

    pub fn load_read(&mut self, node_id: &str, key: &str) -> Result<HttpResponse, String> {
        self.request_node(node_id, "GET", &format!("/api/load/read?key={key}"))
    }

    pub fn validate_successful_write(
        &self,
        response: &HttpResponse,
    ) -> Result<(String, i64), String> {
        if response.status != 200 {
            return Err(format!(
                "write status must be 200, got {} body={}",
                response.status, response.raw_body
            ));
        }
        if !response
            .body
            .get("ok")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            return Err(format!("write payload not ok: {}", response.raw_body));
        }
        let value = response
            .body
            .get("value")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("write payload missing value: {}", response.raw_body))?
            .to_string();
        let version = response
            .body
            .get("version")
            .and_then(Value::as_i64)
            .ok_or_else(|| {
                format!(
                    "write payload missing integer version: {}",
                    response.raw_body
                )
            })?;
        if version <= 0 {
            return Err(format!(
                "write payload version must be > 0, got {} payload={}",
                version, response.raw_body
            ));
        }
        Ok((value, version))
    }

    pub fn wait_for_all_healthy(&mut self, timeout: Duration) -> Result<(), String> {
        let deadline = Instant::now() + timeout;
        let mut last_issue = "health checks have not passed yet".to_string();
        while Instant::now() < deadline {
            self.assert_all_processes_alive()?;
            let mut all_ok = true;
            for node_id in self.node_ids() {
                match self.probe_health(&node_id) {
                    Ok(response)
                        if response.status == 200
                            && response
                                .body
                                .get("ok")
                                .and_then(Value::as_bool)
                                .unwrap_or(false) => {}
                    Ok(response) => {
                        all_ok = false;
                        last_issue = format!(
                            "health check failed for {node_id}: status={} body={}",
                            response.status, response.raw_body
                        );
                        break;
                    }
                    Err(err) => {
                        all_ok = false;
                        last_issue = format!("health request failed for {node_id}: {err}");
                        break;
                    }
                }
            }
            if all_ok {
                return Ok(());
            }
            thread::sleep(POLL_INTERVAL);
        }
        Err(format!(
            "timed out waiting for healthy cluster: {}\n{}",
            last_issue,
            self.diagnostics_report()
        ))
    }

    pub fn wait_for_mesh_ready(
        &mut self,
        timeout: Duration,
    ) -> Result<BTreeMap<String, Value>, String> {
        let expected_nodes = self
            .node_specs
            .keys()
            .cloned()
            .collect::<BTreeSet<String>>();
        let deadline = Instant::now() + timeout;
        let mut last_issue = "mesh not ready".to_string();
        while Instant::now() < deadline {
            self.assert_all_processes_alive()?;
            let mut snapshots = BTreeMap::new();
            let mut all_ready = true;
            for node_id in self.node_ids() {
                let response = match self.probe_mesh(&node_id) {
                    Ok(response) => response,
                    Err(err) => {
                        last_issue = format!("mesh request failed for {node_id}: {err}");
                        all_ready = false;
                        break;
                    }
                };
                if response.status != 200 {
                    last_issue = format!(
                        "mesh status != 200 for {node_id}: status={} body={}",
                        response.status, response.raw_body
                    );
                    all_ready = false;
                    break;
                }
                if !response
                    .body
                    .get("ok")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    last_issue =
                        format!("mesh payload not ok for {node_id}: {}", response.raw_body);
                    all_ready = false;
                    break;
                }
                if !response
                    .body
                    .get("meshReady")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    last_issue = format!("mesh not ready for {node_id}: {}", response.raw_body);
                    all_ready = false;
                    break;
                }
                let node_count = response
                    .body
                    .get("nodeCount")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| {
                        format!("mesh payload missing nodeCount: {}", response.raw_body)
                    })?;
                if node_count != expected_nodes.len() as u64 {
                    last_issue = format!(
                        "unexpected mesh nodeCount for {node_id}: {} payload={}",
                        node_count, response.raw_body
                    );
                    all_ready = false;
                    break;
                }
                let discovered = parse_node_list(response.body.get("nodes"))?;
                if discovered != expected_nodes {
                    last_issue = format!(
                        "mesh node list mismatch for {node_id}: expected={expected_nodes:?} actual={discovered:?}"
                    );
                    all_ready = false;
                    break;
                }
                snapshots.insert(node_id, response.body);
            }
            if !all_ready {
                thread::sleep(POLL_INTERVAL);
                continue;
            }

            let leaders = snapshots
                .values()
                .filter_map(|value| value.get("leaderId").and_then(Value::as_str))
                .map(ToString::to_string)
                .collect::<BTreeSet<_>>();
            if leaders.len() != 1 {
                last_issue = format!("mesh leader disagreement across nodes: {leaders:?}");
                thread::sleep(POLL_INTERVAL);
                continue;
            }
            return Ok(snapshots);
        }
        Err(format!(
            "timed out waiting for mesh readiness: {}\n{}",
            last_issue,
            self.diagnostics_report()
        ))
    }

    pub fn current_leader_id(&mut self) -> Result<String, String> {
        let snapshots = self.wait_for_mesh_ready(Duration::from_secs(10))?;
        let mut leaders = snapshots
            .values()
            .filter_map(|value| value.get("leaderId").and_then(Value::as_str))
            .map(ToString::to_string)
            .collect::<BTreeSet<_>>();
        leaders
            .pop_first()
            .ok_or_else(|| "mesh snapshots missing leaderId".to_string())
    }

    pub fn wait_for_node_value(
        &mut self,
        node_id: &str,
        expected_value: &str,
        direct: bool,
        timeout: Duration,
    ) -> Result<(), String> {
        let deadline = Instant::now() + timeout;
        let mut last_issue = "node value not visible".to_string();
        while Instant::now() < deadline {
            self.assert_all_processes_alive()?;
            let response = if direct {
                self.probe_read_direct(node_id)
            } else {
                self.probe_read(node_id)
            };
            match response {
                Ok(response)
                    if response.status == 200
                        && response
                            .body
                            .get("ok")
                            .and_then(Value::as_bool)
                            .unwrap_or(false)
                        && response
                            .body
                            .get("value")
                            .and_then(Value::as_str)
                            .map(|value| value == expected_value)
                            .unwrap_or(false) =>
                {
                    return Ok(());
                }
                Ok(response) => {
                    last_issue = format!(
                        "node {} value mismatch on {} read: status={} body={}",
                        node_id,
                        if direct { "direct" } else { "pooled" },
                        response.status,
                        response.raw_body
                    );
                }
                Err(err) => {
                    last_issue = format!(
                        "node {} read request failed on {} path: {}",
                        node_id,
                        if direct { "direct" } else { "pooled" },
                        err
                    );
                }
            }
            thread::sleep(POLL_INTERVAL);
        }
        Err(format!(
            "timed out waiting for node value on {node_id}: {}\n{}",
            last_issue,
            self.diagnostics_report()
        ))
    }

    pub fn wait_for_cluster_value(
        &mut self,
        expected_value: &str,
        timeout: Duration,
    ) -> Result<(), String> {
        let deadline = Instant::now() + timeout;
        let mut last_issue = "cluster value not converged".to_string();
        while Instant::now() < deadline {
            self.assert_all_processes_alive()?;
            let mut all_ok = true;
            for node_id in self.node_ids() {
                let read = self.probe_read(&node_id);
                match read {
                    Ok(read) if response_has_value(&read, expected_value) => {}
                    Ok(read) => {
                        all_ok = false;
                        last_issue =
                            format!("value mismatch for {node_id} on read: {}", read.raw_body);
                        break;
                    }
                    Err(err) => {
                        all_ok = false;
                        last_issue = format!("read request failed for {node_id}: {err}");
                        break;
                    }
                }
            }
            if all_ok {
                return Ok(());
            }
            thread::sleep(POLL_INTERVAL);
        }
        Err(format!(
            "timed out waiting for cluster value convergence: {}\n{}",
            last_issue,
            self.diagnostics_report()
        ))
    }

    pub fn restart_node(&mut self, node_id: &str) -> Result<(), String> {
        if let Some(mut process) = self.nodes.remove(node_id) {
            process.stop()?;
        }
        let spec = self
            .node_specs
            .get(node_id)
            .ok_or_else(|| format!("missing node spec for `{node_id}`"))?
            .clone();
        let restarted = spawn_node(
            &self.binary_path,
            spec,
            &self.cluster_nodes_env,
            &self.address_map_env,
            &self.node_extra_env,
        )?;
        self.nodes.insert(node_id.to_string(), restarted);
        Ok(())
    }

    pub fn stop_node(&mut self, node_id: &str) -> Result<(), String> {
        let mut process = self
            .nodes
            .remove(node_id)
            .ok_or_else(|| format!("unknown node id `{node_id}`"))?;
        process.stop()
    }

    pub fn assert_all_processes_alive(&mut self) -> Result<(), String> {
        let mut exited = None;
        for (node_id, process) in &mut self.nodes {
            if let Some(status) = process.try_wait()? {
                exited = Some((node_id.clone(), status));
                break;
            }
        }
        if let Some((node_id, status)) = exited {
            return Err(format!(
                "node {node_id} exited unexpectedly with {status}\n{}",
                self.diagnostics_report()
            ));
        }
        Ok(())
    }

    pub fn diagnostics_report(&self) -> String {
        let mut out = String::new();
        for (node_id, spec) in &self.node_specs {
            let stdout_tail = read_tail(&spec.stdout_log, 3_000).unwrap_or_else(|_| "".to_string());
            let stderr_tail = read_tail(&spec.stderr_log, 3_000).unwrap_or_else(|_| "".to_string());
            out.push_str(&format!(
                "\n[{node_id}] stdout_log={}\n{stdout_tail}\n[{node_id}] stderr_log={}\n{stderr_tail}\n",
                spec.stdout_log.display(),
                spec.stderr_log.display()
            ));
        }
        out
    }

    fn request_node(
        &mut self,
        node_id: &str,
        method: &str,
        path: &str,
    ) -> Result<HttpResponse, String> {
        self.assert_all_processes_alive()?;
        let spec = self
            .node_specs
            .get(node_id)
            .ok_or_else(|| format!("unknown node id `{node_id}`"))?;
        let base = format!("http://127.0.0.1:{}", spec.http_port);
        Self::request_json_url(&base, method, path)
    }
}

impl Drop for LocalCluster {
    fn drop(&mut self) {
        for process in self.nodes.values_mut() {
            let _ = process.stop();
        }
    }
}

fn workspace_root() -> Result<PathBuf, String> {
    let runtime_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    runtime_root
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "failed to resolve workspace root from runtime crate".to_string())
}

fn build_smoke_binary(workspace_root: &Path, output_path: &Path) -> Result<(), String> {
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "failed to create build output dir {}: {err}",
                parent.display()
            )
        })?;
    }
    let manifest_path = workspace_root
        .join("apps")
        .join("wreladb-lab")
        .join("Cargo.toml");
    let manifest_path_text = manifest_path.to_string_lossy().to_string();
    let output = Command::new("cargo")
        .args([
            "build",
            "--manifest-path",
            &manifest_path_text,
            "--bin",
            "wreladb_lab",
        ])
        .current_dir(workspace_root)
        .output()
        .map_err(|err| format!("failed to invoke `cargo build --manifest-path ...`: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "failed to build local cluster node binary:\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let built_binary = workspace_root
        .join("apps")
        .join("wreladb-lab")
        .join("target")
        .join("debug")
        .join(if cfg!(windows) {
            "wreladb_lab.exe"
        } else {
            "wreladb_lab"
        });
    if !built_binary.is_file() {
        return Err(format!(
            "expected built cluster node binary at {}, but file is missing",
            built_binary.display()
        ));
    }
    fs::copy(&built_binary, output_path).map_err(|err| {
        format!(
            "failed to copy built binary from {} to {}: {err}",
            built_binary.display(),
            output_path.display()
        )
    })?;
    #[cfg(unix)]
    {
        let permissions = fs::Permissions::from_mode(0o755);
        fs::set_permissions(output_path, permissions).map_err(|err| {
            format!(
                "failed to set executable permissions on {}: {err}",
                output_path.display()
            )
        })?;
    }
    if !output_path.is_file() {
        return Err(format!(
            "cluster node build reported success but output binary is missing: {}",
            output_path.display()
        ));
    }
    Ok(())
}

fn spawn_node(
    binary_path: &Path,
    spec: NodeSpec,
    cluster_nodes: &str,
    address_map: &str,
    extra_env: &BTreeMap<String, String>,
) -> Result<NodeProcess, String> {
    fs::create_dir_all(&spec.data_dir).map_err(|err| {
        format!(
            "failed to create node data dir {}: {err}",
            spec.data_dir.display()
        )
    })?;
    if let Some(parent) = spec.stdout_log.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create log dir {}: {err}", parent.display()))?;
    }
    fs::write(&spec.stdout_log, b"").map_err(|err| {
        format!(
            "failed to initialize stdout log {}: {err}",
            spec.stdout_log.display()
        )
    })?;
    fs::write(&spec.stderr_log, b"").map_err(|err| {
        format!(
            "failed to initialize stderr log {}: {err}",
            spec.stderr_log.display()
        )
    })?;
    let stdout = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&spec.stdout_log)
        .map_err(|err| {
            format!(
                "open stdout log failed {}: {err}",
                spec.stdout_log.display()
            )
        })?;
    let stderr = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&spec.stderr_log)
        .map_err(|err| {
            format!(
                "open stderr log failed {}: {err}",
                spec.stderr_log.display()
            )
        })?;

    let mut command = Command::new(binary_path);
    command
        .env("PORT", spec.http_port.to_string())
        .env(
            "WRELADB_DATA_DIR",
            spec.data_dir.to_string_lossy().to_string(),
        )
        .env("WRELADB_PRIVATE_RPC_ENABLED", "1")
        .env(
            "WRELADB_PRIVATE_RPC_PORT",
            spec.private_rpc_port.to_string(),
        )
        .env(
            "WRELADB_PRIVATE_RPC_BIND",
            format!("127.0.0.1:{}", spec.private_rpc_port),
        )
        .env("WRELADB_PRIVATE_RPC_MTLS_MODE", "off")
        .env("WRELADB_PRIVATE_RPC_TRUSTED_NETWORK", "local-loopback")
        .env("WRELADB_NODE_ID", &spec.id)
        .env("FLY_MACHINE_ID", &spec.id)
        .env("WRELADB_CLUSTER_NODES", cluster_nodes)
        .env("WRELADB_PRIVATE_RPC_ADDRESS_MAP", address_map)
        .env("WRELADB_REPLICATION_FACTOR", "1")
        .env("WRELADB_WRITE_QUORUM", "1")
        .env("WRELADB_TARGET_VOTERS", "3")
        .env("WRELADB_PRIVATE_RPC_MIN_READY_NODES", "1")
        .env("WRELADB_PRIVATE_RPC_TIMEOUT_MS", "1000")
        .env("RUST_BACKTRACE", "1")
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    for (key, value) in extra_env {
        command.env(key, value);
    }

    let child = command.spawn().map_err(|err| {
        format!(
            "failed to spawn node {} with binary {}: {err}",
            spec.id,
            binary_path.display()
        )
    })?;
    Ok(NodeProcess { spec, child })
}

fn reserve_local_port() -> Result<u16, String> {
    for _ in 0..32 {
        let listener = TcpListener::bind("127.0.0.1:0")
            .map_err(|err| format!("failed to reserve local port: {err}"))?;
        let port = listener
            .local_addr()
            .map_err(|err| format!("failed to read local addr for reserved port: {err}"))?
            .port();
        drop(listener);
        if port != 0 {
            return Ok(port);
        }
    }
    Err("failed to reserve non-zero localhost port".to_string())
}

fn request_json(method: &str, url: &str) -> Result<HttpResponse, String> {
    let method = method.to_string();
    let url = url.to_string();
    let (status, body_text) = tokio_runtime().block_on(async {
        let client = reqwest::Client::builder()
            .connect_timeout(REQUEST_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|err| format!("failed to build HTTP client: {err}"))?;
        let method = method.as_str();
        let request = match method {
            "GET" => client.get(&url),
            "POST" => client.post(&url),
            other => return Err(format!("unsupported HTTP method `{other}`")),
        };
        let response = request
            .send()
            .await
            .map_err(|err| format!("request failed {method} {url}: {err}"))?;
        let status = response.status().as_u16();
        let text = response
            .text()
            .await
            .map_err(|err| format!("failed reading response body from {url}: {err}"))?;
        Ok::<_, String>((status, text))
    })?;

    let body = serde_json::from_str::<Value>(&body_text).unwrap_or_else(|_| {
        serde_json::json!({
            "ok": false,
            "rawBody": body_text
        })
    });
    Ok(HttpResponse {
        status,
        body,
        raw_body: body_text,
    })
}

fn response_has_value(response: &HttpResponse, expected_value: &str) -> bool {
    response.status == 200
        && response
            .body
            .get("ok")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && response
            .body
            .get("value")
            .and_then(Value::as_str)
            .map(|value| value == expected_value)
            .unwrap_or(false)
}

fn parse_node_list(nodes: Option<&Value>) -> Result<BTreeSet<String>, String> {
    let Some(nodes) = nodes else {
        return Err("mesh payload missing nodes field".to_string());
    };
    let values = nodes
        .as_array()
        .ok_or_else(|| format!("mesh nodes field is not an array: {nodes}"))?;
    let mut out = BTreeSet::new();
    for value in values {
        let node_id = value
            .as_str()
            .ok_or_else(|| format!("mesh node entry is not a string: {value}"))?;
        out.insert(node_id.to_string());
    }
    Ok(out)
}

fn read_tail(path: &Path, max_bytes: usize) -> Result<String, String> {
    let bytes =
        fs::read(path).map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    let start = bytes.len().saturating_sub(max_bytes);
    String::from_utf8(bytes[start..].to_vec())
        .map_err(|err| format!("log tail utf8 decode failed for {}: {err}", path.display()))
}
