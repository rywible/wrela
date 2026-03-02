use super::super::provider::{DeployProvider, DeployReport, DeployRequest};
use serde_json::Value;
use std::collections::BTreeMap;
use std::env;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub struct FlyProvider;

impl FlyProvider {
    pub fn new() -> Self {
        Self
    }
}

impl DeployProvider for FlyProvider {
    fn deploy(&self, request: &DeployRequest) -> Result<DeployReport, String> {
        let started = now_unix_ms();
        let app_url = format!("https://{}.fly.dev", request.app);

        run_flyctl(&["apps", "create", &request.app, "--machines"], true)?;
        let pre_existing_machine_ids = machine_ids(&request.app)?;
        let is_bootstrap = pre_existing_machine_ids.is_empty();
        let (pre_health, pre_probe, builder_mode) = if is_bootstrap {
            let builder_mode = deploy_rolling(request)?;
            reconcile_machine_topology(&request.app, &request.region_machine_counts)?;
            (
                serde_json::json!({
                    "ok": true,
                    "skipped": "bootstrap_no_existing_machines"
                }),
                serde_json::json!({
                    "ok": true,
                    "skipped": "bootstrap_no_existing_machines"
                }),
                builder_mode,
            )
        } else {
            reconcile_machine_topology(&request.app, &request.region_machine_counts)?;
            let (pre_health, pre_probe) = wait_for_gates(&request.app, &app_url, request.machines)?;
            let builder_mode = deploy_rolling(request)?;
            reconcile_machine_topology(&request.app, &request.region_machine_counts)?;
            (pre_health, pre_probe, builder_mode)
        };

        let machine_ids = machine_ids(&request.app)?;
        let machine_regions = machine_region_counts(&request.app)?;
        let (post_health, post_probe_global) =
            wait_for_gates(&request.app, &app_url, request.machines)?;
        let post_probe_per_machine =
            gate_probe_per_machine(&app_url, &machine_ids, request.machines)?;
        let post_probe = serde_json::json!({
            "global": post_probe_global,
            "perMachine": post_probe_per_machine
        });

        let mut notes = vec![
            format!("builder={builder_mode}"),
            "deploy strategy: rolling".to_string(),
            "pre/post strict health+probe gates passed".to_string(),
            format!(
                "region_machine_counts={}",
                serde_json::to_string(&request.region_machine_counts)
                    .unwrap_or_else(|_| "{}".to_string())
            ),
            format!(
                "quorum=target_voters:{} rf:{} wq:{}",
                request.target_voters, request.replication_factor, request.write_quorum
            ),
            format!(
                "post_region_machine_counts={}",
                serde_json::to_string(&machine_regions).unwrap_or_else(|_| "{}".to_string())
            ),
            "gate.phase.post_health=passed".to_string(),
            "gate.phase.post_probe_global=passed".to_string(),
            "gate.phase.post_probe_per_machine=passed".to_string(),
            format!("project_root={}", request.project_root.display()),
            format!(
                "deploy_context_root={}",
                request.deploy_context_root.display()
            ),
        ];
        if is_bootstrap {
            notes.push(
                "bootstrap mode: pre-deploy gates skipped because no machines existed".to_string(),
            );
            notes.push("gate.phase.pre_health=skipped_bootstrap".to_string());
            notes.push("gate.phase.pre_probe=skipped_bootstrap".to_string());
        } else {
            notes.push("gate.phase.pre_health=passed".to_string());
            notes.push("gate.phase.pre_probe=passed".to_string());
        }

        Ok(DeployReport {
            provider: "fly".to_string(),
            app: request.app.clone(),
            url: app_url,
            region: request.region.clone(),
            machines: request.machines,
            started_at_unix_ms: started,
            finished_at_unix_ms: now_unix_ms(),
            pre_health,
            post_health,
            pre_probe,
            post_probe,
            machine_ids,
            notes,
        })
    }
}

fn run_flyctl(args: &[&str], allow_exists: bool) -> Result<String, String> {
    run_flyctl_in(args, allow_exists, None)
}

fn run_flyctl_in(args: &[&str], allow_exists: bool, cwd: Option<&Path>) -> Result<String, String> {
    let mut command = Command::new(flyctl_binary());
    command.args(args);
    if let Some(path) = cwd {
        command.current_dir(path);
    }
    let output = command
        .output()
        .map_err(|err| format!("failed to run flyctl {:?}: {}", args, err))?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).to_string());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if allow_exists
        && (stderr.contains("already exists") || stderr.contains("has already been taken"))
    {
        return Ok(String::new());
    }
    Err(format!("flyctl {:?} failed: {}", args, stderr))
}

fn run_flyctl_in_with_timeout(
    args: &[&str],
    allow_exists: bool,
    cwd: Option<&Path>,
    timeout: Duration,
) -> Result<String, String> {
    let mut command = Command::new(flyctl_binary());
    command.args(args);
    // Deploy output can exceed pipe buffers. Capture into temp files so flyctl cannot deadlock
    // while emitting remote build/deploy logs, and we still preserve stderr for diagnostics.
    let stdout_path = temp_flyctl_capture_path("stdout");
    let stderr_path = temp_flyctl_capture_path("stderr");
    let stdout_file = File::create(&stdout_path)
        .map_err(|err| format!("failed to create capture file: {err}"))?;
    let stderr_file = File::create(&stderr_path)
        .map_err(|err| format!("failed to create capture file: {err}"))?;
    command.stdout(stdout_file);
    command.stderr(stderr_file);
    if let Some(path) = cwd {
        command.current_dir(path);
    }

    let mut child = command
        .spawn()
        .map_err(|err| format!("failed to run flyctl {:?}: {}", args, err))?;
    let started = Instant::now();
    loop {
        if started.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            let stderr = read_and_cleanup_capture(&stderr_path);
            let _ = fs::remove_file(&stdout_path);
            return Err(format!(
                "flyctl {:?} timed out after {}s: {}",
                args,
                timeout.as_secs(),
                stderr.trim()
            ));
        }

        match child
            .try_wait()
            .map_err(|err| format!("failed to poll flyctl {:?}: {}", args, err))?
        {
            Some(status) => {
                if status.success() {
                    let stdout = read_and_cleanup_capture(&stdout_path);
                    let _ = fs::remove_file(&stderr_path);
                    return Ok(stdout);
                }

                let stderr = read_and_cleanup_capture(&stderr_path);
                let _ = fs::remove_file(&stdout_path);
                if allow_exists
                    && (stderr.contains("already exists")
                        || stderr.contains("has already been taken"))
                {
                    return Ok(String::new());
                }
                return Err(format!("flyctl {:?} failed: {}", args, stderr.trim()));
            }
            None => thread::sleep(Duration::from_millis(200)),
        }
    }
}

fn temp_flyctl_capture_path(kind: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "wrela-flyctl-{}-{}-{}.log",
        kind,
        std::process::id(),
        now_unix_ms()
    ))
}

fn read_and_cleanup_capture(path: &Path) -> String {
    let data = fs::read_to_string(path).unwrap_or_default();
    let _ = fs::remove_file(path);
    data
}

fn run_curl_json(args: &[&str]) -> Result<Value, String> {
    let connect_timeout = env::var("WRELA_DEPLOY_CURL_CONNECT_TIMEOUT_SEC")
        .ok()
        .and_then(|raw| raw.parse::<u64>().ok())
        .unwrap_or(2)
        .max(1);
    let max_time = env::var("WRELA_DEPLOY_CURL_MAX_TIME_SEC")
        .ok()
        .and_then(|raw| raw.parse::<u64>().ok())
        .unwrap_or(8)
        .max(connect_timeout);
    let mut full_args = vec![
        "--connect-timeout".to_string(),
        connect_timeout.to_string(),
        "--max-time".to_string(),
        max_time.to_string(),
    ];
    full_args.extend(args.iter().map(|arg| (*arg).to_string()));
    let output = Command::new(curl_binary())
        .args(&full_args)
        .output()
        .map_err(|err| format!("failed to run curl {:?}: {}", args, err))?;
    if !output.status.success() {
        return Err(format!(
            "curl {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    serde_json::from_slice::<Value>(&output.stdout).map_err(|err| {
        format!(
            "invalid json from curl {:?}: {}\n{}",
            args,
            err,
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

fn deploy_rolling(request: &DeployRequest) -> Result<String, String> {
    let config = request.config_path.to_string_lossy().to_string();
    let dockerfile = request.dockerfile_path.to_string_lossy().to_string();
    let depot_timeout_seconds = env::var("WRELA_DEPLOY_FLY_DEPOT_TIMEOUT_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|seconds| *seconds > 0)
        .unwrap_or(900);
    let depot_args = vec![
        "deploy".to_string(),
        "--remote-only".to_string(),
        "--depot=true".to_string(),
        "--config".to_string(),
        config,
        "--dockerfile".to_string(),
        dockerfile,
        "-a".to_string(),
        request.app.clone(),
        "--strategy".to_string(),
        "rolling".to_string(),
        "--yes".to_string(),
    ];
    let depot_refs = depot_args.iter().map(String::as_str).collect::<Vec<_>>();
    run_flyctl_in_with_timeout(
        &depot_refs,
        false,
        Some(&request.deploy_context_root),
        Duration::from_secs(depot_timeout_seconds),
    )?;
    Ok("fly-remote-depot".to_string())
}

fn machine_ids(app: &str) -> Result<Vec<String>, String> {
    let mut ids = machine_infos(app)?
        .into_iter()
        .map(|machine| machine.id)
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    Ok(ids)
}

#[derive(Debug, Clone)]
struct MachineInfo {
    id: String,
    region: String,
}

fn machine_infos(app: &str) -> Result<Vec<MachineInfo>, String> {
    let raw = run_flyctl(&["machines", "list", "-a", app, "--json"], false)?;
    let parsed = serde_json::from_str::<Value>(&raw)
        .map_err(|err| format!("invalid fly machines json: {}", err))?;
    Ok(parsed
        .as_array()
        .ok_or_else(|| "fly machines list did not return array".to_string())?
        .iter()
        .filter_map(|machine| {
            let id = machine
                .get("id")
                .or_else(|| machine.get("ID"))
                .and_then(Value::as_str)
                .map(ToString::to_string)?;
            let region = machine
                .get("region")
                .or_else(|| machine.get("Region"))
                .or_else(|| machine.get("config").and_then(|cfg| cfg.get("region")))
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_ascii_lowercase();
            Some(MachineInfo { id, region })
        })
        .collect::<Vec<_>>())
}

fn machine_region_counts(app: &str) -> Result<BTreeMap<String, usize>, String> {
    let mut counts = BTreeMap::new();
    for machine in machine_infos(app)? {
        *counts.entry(machine.region).or_insert(0usize) += 1;
    }
    Ok(counts)
}

fn reconcile_machine_topology(
    app: &str,
    region_machine_counts: &BTreeMap<String, usize>,
) -> Result<(), String> {
    if region_machine_counts.is_empty() {
        return Err("deploy machine topology cannot be empty".to_string());
    }

    if region_machine_counts.len() == 1 {
        let (region, count) = region_machine_counts.iter().next().expect("single region");
        run_flyctl(
            &[
                "scale",
                "count",
                &count.to_string(),
                "--region",
                region,
                "-a",
                app,
                "--yes",
            ],
            false,
        )?;
        return Ok(());
    }

    for (region, count) in region_machine_counts {
        run_flyctl(
            &[
                "scale",
                "count",
                &count.to_string(),
                "--region",
                region,
                "-a",
                app,
                "--yes",
            ],
            false,
        )?;
    }

    let existing = machine_region_counts(app)?;
    for region in existing.keys() {
        if region == "unknown" {
            continue;
        }
        if !region_machine_counts.contains_key(region) {
            run_flyctl(
                &[
                    "scale", "count", "0", "--region", region, "-a", app, "--yes",
                ],
                false,
            )?;
        }
    }
    Ok(())
}

fn gate_health(url: &str, machine_count: usize, expected_machines: usize) -> Result<Value, String> {
    if machine_count < expected_machines {
        return Err(format!(
            "machine count below expected threshold: {} < {}",
            machine_count, expected_machines
        ));
    }
    let health = run_curl_json(&["-fsS", &format!("{url}/api/health")])?;
    if !health.get("ok").and_then(Value::as_bool).unwrap_or(false) {
        return Err(format!("health gate failed: {}", health));
    }
    Ok(health)
}

fn gate_probe(url: &str) -> Result<Value, String> {
    let write = run_curl_json(&["-fsS", "-X", "POST", &format!("{url}/api/probe/write")])?;
    if !write.get("ok").and_then(Value::as_bool).unwrap_or(false) {
        return Err(format!("probe write failed: {}", write));
    }
    let committed_version = write.get("version").and_then(Value::as_i64).unwrap_or(-1);
    if committed_version <= 0 {
        return Err(format!(
            "probe write returned invalid commit version {} payload={}",
            committed_version, write
        ));
    }
    if let Some(required_acks) = write.get("requiredAcks").and_then(Value::as_u64) {
        let got = write
            .get("replicationAcks")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        if got < required_acks {
            return Err(format!(
                "probe write quorum failed: replicationAcks={} requiredAcks={} payload={}",
                got, required_acks, write
            ));
        }
    }
    let expected = write
        .get("value")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("probe write missing value: {}", write))?
        .to_string();

    let cluster = run_curl_json(&["-fsS", &format!("{url}/api/probe/cluster_read")])?;
    if !cluster.get("ok").and_then(Value::as_bool).unwrap_or(false) {
        return Err(format!("cluster probe failed: {}", cluster));
    }

    let mut observed_match = false;
    if let Some(readings) = cluster.get("readings").and_then(Value::as_array) {
        let invalid = readings.iter().any(|item| {
            let ok = item.get("ok").and_then(Value::as_bool).unwrap_or(false);
            let value = item.get("value").and_then(Value::as_str).unwrap_or("");
            if value == expected {
                observed_match = true;
            }
            !ok || value.is_empty()
        });
        if invalid {
            return Err(format!(
                "cluster probe returned invalid readings payload={}",
                cluster
            ));
        }
    } else if let Some(value) = cluster.get("value").and_then(Value::as_str) {
        if value.is_empty() {
            return Err(format!(
                "cluster probe returned empty value payload={}",
                cluster
            ));
        }
        observed_match = value == expected;
    }
    if !observed_match {
        return Err(format!(
            "cluster probe did not observe written value expected={} payload={}",
            expected, cluster
        ));
    }

    Ok(serde_json::json!({
        "write": write,
        "cluster": cluster,
        "expectedValue": expected,
        "observedMatch": observed_match
    }))
}

fn run_targeted_curl_json(
    url: &str,
    machine_id: &str,
    method: Option<&str>,
) -> Result<Value, String> {
    let mut args = vec![
        "-fsS".to_string(),
        "-H".to_string(),
        format!("fly-force-instance-id: {machine_id}"),
    ];
    if let Some(method) = method {
        args.push("-X".to_string());
        args.push(method.to_string());
    }
    args.push(url.to_string());
    let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    run_curl_json(&refs)
}

fn validate_machine_binding(
    payload: &Value,
    expected_machine: &str,
    label: &str,
) -> Result<(), String> {
    let payload_machine = payload
        .get("machineId")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{label} missing machineId field: {payload}"))?;
    if payload_machine != expected_machine {
        return Err(format!(
            "{label} routed to wrong machine: expected={} actual={} payload={}",
            expected_machine, payload_machine, payload
        ));
    }
    Ok(())
}

fn gate_probe_per_machine(
    url: &str,
    machine_ids: &[String],
    expected_machines: usize,
) -> Result<Value, String> {
    if machine_ids.len() < expected_machines {
        return Err(format!(
            "machine count below expected threshold for per-machine gate: {} < {}",
            machine_ids.len(),
            expected_machines
        ));
    }
    if machine_ids.is_empty() {
        return Err("per-machine gate requires at least one machine".to_string());
    }

    let seed_machine = machine_ids[0].as_str();
    let write = run_targeted_curl_json(
        &format!("{url}/api/probe/write"),
        seed_machine,
        Some("POST"),
    )?;
    if !write.get("ok").and_then(Value::as_bool).unwrap_or(false) {
        return Err(format!("per-machine seed write failed: {}", write));
    }
    let committed_version = write.get("version").and_then(Value::as_i64).unwrap_or(-1);
    if committed_version <= 0 {
        return Err(format!(
            "per-machine seed write returned invalid commit version {} payload={}",
            committed_version, write
        ));
    }
    validate_machine_binding(&write, seed_machine, "per-machine seed write")?;
    let expected = write
        .get("value")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("per-machine seed write missing value: {}", write))?
        .to_string();
    let attempts = gate_attempts();
    let sleep = gate_sleep_duration();
    let mut last_error = "unknown per-machine probe failure".to_string();
    for _ in 0..attempts {
        let mut reads = Vec::with_capacity(machine_ids.len());
        let mut all_converged = true;
        for machine_id in machine_ids {
            let read =
                match run_targeted_curl_json(&format!("{url}/api/probe/read"), machine_id, None) {
                    Ok(payload) => payload,
                    Err(err) => {
                        last_error = format!(
                            "per-machine read request failed for {}: {}",
                            machine_id, err
                        );
                        all_converged = false;
                        break;
                    }
                };
            if !read.get("ok").and_then(Value::as_bool).unwrap_or(false) {
                last_error = format!(
                    "per-machine read payload not ok for {}: {}",
                    machine_id, read
                );
                all_converged = false;
                break;
            }
            if let Err(err) = validate_machine_binding(&read, machine_id, "per-machine read") {
                last_error = err;
                all_converged = false;
                break;
            }
            let value = read
                .get("value")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if value.is_empty() {
                last_error = format!(
                    "per-machine read returned empty value for {} payload={}",
                    machine_id, read
                );
                all_converged = false;
            } else if value != expected {
                last_error = format!(
                    "per-machine read value mismatch for {} expected={} actual={} payload={}",
                    machine_id, expected, value, read
                );
                all_converged = false;
            }
            reads.push(read);
        }
        if all_converged && reads.len() == machine_ids.len() {
            return Ok(serde_json::json!({
                "seedWrite": write,
                "reads": reads
            }));
        }
        thread::sleep(sleep);
    }

    Err(format!(
        "timed out waiting for per-machine probe convergence: {}",
        last_error
    ))
}

fn wait_for_gates(
    app: &str,
    url: &str,
    expected_machines: usize,
) -> Result<(Value, Value), String> {
    let attempts = gate_attempts();
    let sleep = gate_sleep_duration();
    let mut last_error = "unknown gate failure".to_string();

    for _ in 0..attempts {
        let machine_count = machine_ids(app).map(|ids| ids.len());
        let health = match machine_count {
            Ok(count) => gate_health(url, count, expected_machines),
            Err(err) => Err(err),
        };
        match health {
            Ok(health_json) => match gate_probe(url) {
                Ok(probe_json) => return Ok((health_json, probe_json)),
                Err(err) => last_error = err,
            },
            Err(err) => last_error = err,
        }
        thread::sleep(sleep);
    }

    Err(format!(
        "timed out waiting for fly deploy gates: {}",
        last_error
    ))
}

fn gate_attempts() -> usize {
    env::var("WRELA_DEPLOY_GATE_ATTEMPTS")
        .ok()
        .and_then(|raw| raw.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(75)
}

fn gate_sleep_duration() -> Duration {
    let ms = env::var("WRELA_DEPLOY_GATE_SLEEP_MS")
        .ok()
        .and_then(|raw| raw.parse::<u64>().ok())
        .unwrap_or(2_000);
    Duration::from_millis(ms.max(1))
}

fn flyctl_binary() -> String {
    env::var("WRELA_DEPLOY_FLYCTL_BIN").unwrap_or_else(|_| "flyctl".to_string())
}

fn curl_binary() -> String {
    env::var("WRELA_DEPLOY_CURL_BIN").unwrap_or_else(|_| "curl".to_string())
}

fn now_unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|dur| dur.as_millis())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct EnvSnapshot {
        key: &'static str,
        value: Option<String>,
    }

    struct EnvGuard {
        snapshots: Vec<EnvSnapshot>,
    }

    impl EnvGuard {
        fn set(vars: &[(&'static str, String)]) -> Self {
            let mut snapshots = Vec::with_capacity(vars.len());
            for (key, value) in vars {
                snapshots.push(EnvSnapshot {
                    key,
                    value: env::var(key).ok(),
                });
                // SAFETY: guarded by ENV_LOCK for test-local mutation.
                unsafe { env::set_var(key, value) };
            }
            Self { snapshots }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for snapshot in &self.snapshots {
                match &snapshot.value {
                    Some(value) => {
                        // SAFETY: guarded by ENV_LOCK for test-local mutation.
                        unsafe { env::set_var(snapshot.key, value) };
                    }
                    None => {
                        // SAFETY: guarded by ENV_LOCK for test-local mutation.
                        unsafe { env::remove_var(snapshot.key) };
                    }
                }
            }
        }
    }

    fn write_executable(path: &Path, contents: &str) {
        fs::write(path, contents).expect("write executable");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(path).expect("metadata").permissions();
            perms.set_mode(0o755);
            fs::set_permissions(path, perms).expect("chmod");
        }
    }

    fn deploy_request(dir: &tempfile::TempDir) -> DeployRequest {
        let config_path = dir.path().join("fly.toml");
        let dockerfile_path = dir.path().join("Dockerfile");
        fs::write(&config_path, "app = \"test\"\n").expect("write config");
        fs::write(&dockerfile_path, "FROM scratch\n").expect("write dockerfile");
        let mut region_machine_counts = std::collections::BTreeMap::new();
        region_machine_counts.insert("ord".to_string(), 3);
        DeployRequest {
            project_root: dir.path().to_path_buf(),
            deploy_context_root: dir.path().to_path_buf(),
            app: "wrela-test-app".to_string(),
            region: "ord".to_string(),
            machines: 3,
            region_machine_counts,
            target_voters: 3,
            replication_factor: 3,
            write_quorum: 2,
            config_path,
            dockerfile_path,
        }
    }

    fn set_env_for_test(vars: &[(&'static str, String)]) -> EnvGuard {
        EnvGuard::set(vars)
    }

    #[test]
    fn deploy_bootstrap_skips_pre_gate_only_when_no_existing_machines() {
        let _lock = ENV_LOCK.lock().expect("env lock");
        let dir = tempfile::tempdir().expect("tempdir");
        let flyctl = dir.path().join("fake-flyctl.sh");
        let curl = dir.path().join("fake-curl.sh");
        let flyctl_log = dir.path().join("flyctl.log");
        let flyctl_state = dir.path().join("flyctl.state");
        let curl_log = dir.path().join("curl.log");
        let request = deploy_request(&dir);

        write_executable(
            &flyctl,
            r#"#!/usr/bin/env bash
set -euo pipefail
log="${WRELA_TEST_FLYCTL_LOG:?}"
state="${WRELA_TEST_FLYCTL_STATE:?}"
echo "$*" >> "$log"
if [[ "$1" == "apps" && "$2" == "create" ]]; then exit 0; fi
if [[ "$1" == "machines" && "$2" == "list" ]]; then
  count=0
  if [[ -f "$state" ]]; then count="$(cat "$state")"; fi
  count=$((count + 1))
  echo "$count" > "$state"
  if [[ "$count" -eq 1 ]]; then
    echo "[]"
  else
    echo '[{"id":"m-1"},{"id":"m-2"},{"id":"m-3"}]'
  fi
  exit 0
fi
if [[ "$1" == "deploy" ]]; then exit 0; fi
if [[ "$1" == "scale" && "$2" == "count" ]]; then exit 0; fi
if [[ "$1" == "secrets" && "$2" == "set" ]]; then exit 0; fi
echo "{}"
"#,
        );
        write_executable(
            &curl,
            r#"#!/usr/bin/env bash
set -euo pipefail
url="${@: -1}"
if [[ "$url" == *"/api/health" ]]; then
  echo '{"ok":true}'
  exit 0
fi
if [[ "$url" == *"/api/probe/write" ]]; then
  target=""
  prev=""
  for arg in "$@"; do
    if [[ "$prev" == "-H" && "$arg" == fly-force-instance-id:* ]]; then
      target="${arg#fly-force-instance-id: }"
    fi
    prev="$arg"
  done
  if [[ -n "$target" ]]; then
    echo "{\"ok\":true,\"machineId\":\"$target\",\"value\":\"seed\",\"requiredAcks\":1,\"replicationAcks\":1,\"version\":1}"
  else
    echo '{"ok":true,"value":"seed","requiredAcks":1,"replicationAcks":1,"version":1}'
  fi
  exit 0
fi
if [[ "$url" == *"/api/probe/read" ]]; then
  target=""
  prev=""
  for arg in "$@"; do
    if [[ "$prev" == "-H" && "$arg" == fly-force-instance-id:* ]]; then
      target="${arg#fly-force-instance-id: }"
    fi
    prev="$arg"
  done
  if [[ -z "$target" ]]; then target="m-1"; fi
  echo "{\"ok\":true,\"machineId\":\"$target\",\"value\":\"seed\"}"
  exit 0
fi
if [[ "$url" == *"/api/probe/cluster_read" ]]; then
  echo '{"ok":true,"readings":[{"ok":true,"value":"seed"},{"ok":true,"value":"seed"},{"ok":true,"value":"seed"}],"value":"seed"}'
  exit 0
fi
echo '{}'
"#,
        );

        let _env = set_env_for_test(&[
            (
                "WRELA_DEPLOY_FLYCTL_BIN",
                flyctl.to_string_lossy().to_string(),
            ),
            ("WRELA_DEPLOY_CURL_BIN", curl.to_string_lossy().to_string()),
            (
                "WRELA_TEST_FLYCTL_LOG",
                flyctl_log.to_string_lossy().to_string(),
            ),
            (
                "WRELA_TEST_FLYCTL_STATE",
                flyctl_state.to_string_lossy().to_string(),
            ),
            (
                "WRELA_TEST_CURL_LOG",
                curl_log.to_string_lossy().to_string(),
            ),
            ("WRELA_DEPLOY_GATE_ATTEMPTS", "2".to_string()),
            ("WRELA_DEPLOY_GATE_SLEEP_MS", "1".to_string()),
        ]);

        let report = FlyProvider::new()
            .deploy(&request)
            .expect("deploy should pass");
        assert_eq!(
            report.pre_health.get("skipped").and_then(Value::as_str),
            Some("bootstrap_no_existing_machines")
        );
        assert!(
            report
                .notes
                .iter()
                .any(|note| note == "builder=fly-remote-depot")
        );
        assert!(report.post_probe.get("perMachine").is_some());
        let flyctl_invocations = std::fs::read_to_string(&flyctl_log).expect("read flyctl log");
        assert!(!flyctl_invocations.contains("secrets set"));
    }

    #[test]
    fn deploy_non_bootstrap_enforces_pre_and_post_gates() {
        let _lock = ENV_LOCK.lock().expect("env lock");
        let dir = tempfile::tempdir().expect("tempdir");
        let request = deploy_request(&dir);
        let flyctl = dir.path().join("flyctl.sh");
        let curl = dir.path().join("curl.sh");
        let flyctl_log = dir.path().join("flyctl.log");
        let flyctl_state = dir.path().join("flyctl.state");
        let curl_log = dir.path().join("curl.log");

        write_executable(
            &flyctl,
            r#"#!/usr/bin/env bash
set -euo pipefail
log="${WRELA_TEST_FLYCTL_LOG:?}"
echo "$*" >> "$log"
if [[ "$1" == "apps" && "$2" == "create" ]]; then exit 0; fi
if [[ "$1" == "machines" && "$2" == "list" ]]; then
  echo '[{"id":"m-1"},{"id":"m-2"},{"id":"m-3"}]'
  exit 0
fi
if [[ "$1" == "deploy" ]]; then
  joined="$*"
  [[ "$joined" == *"--remote-only"* ]] || exit 1
  [[ "$joined" == *"--depot=true"* ]] || exit 1
  [[ "$joined" != *"--depot=false"* ]] || exit 1
  [[ "$joined" == *"--strategy rolling"* ]] || exit 1
  exit 0
fi
if [[ "$1" == "scale" && "$2" == "count" ]]; then exit 0; fi
if [[ "$1" == "secrets" && "$2" == "set" ]]; then exit 0; fi
echo "{}"
"#,
        );
        write_executable(
            &curl,
            r#"#!/usr/bin/env bash
set -euo pipefail
log="${WRELA_TEST_CURL_LOG:?}"
echo "$*" >> "$log"
url="${@: -1}"
target=""
prev=""
for arg in "$@"; do
  if [[ "$prev" == "-H" && "$arg" == fly-force-instance-id:* ]]; then
    target="${arg#fly-force-instance-id: }"
  fi
  prev="$arg"
done
if [[ "$url" == *"/api/health" ]]; then
  echo '{"ok":true}'
  exit 0
fi
if [[ "$url" == *"/api/probe/write" ]]; then
  if [[ -n "$target" ]]; then
    echo "{\"ok\":true,\"machineId\":\"$target\",\"value\":\"seed\",\"requiredAcks\":1,\"replicationAcks\":1,\"version\":1}"
  else
    echo '{"ok":true,"value":"seed","requiredAcks":1,"replicationAcks":1,"version":1}'
  fi
  exit 0
fi
if [[ "$url" == *"/api/probe/read" ]]; then
  if [[ -z "$target" ]]; then target="m-1"; fi
  echo "{\"ok\":true,\"machineId\":\"$target\",\"value\":\"seed\"}"
  exit 0
fi
if [[ "$url" == *"/api/probe/cluster_read" ]]; then
  echo '{"ok":true,"readings":[{"ok":true,"value":"seed"},{"ok":true,"value":"seed"},{"ok":true,"value":"seed"}],"value":"seed"}'
  exit 0
fi
echo '{}'
"#,
        );

        let _env = set_env_for_test(&[
            (
                "WRELA_DEPLOY_FLYCTL_BIN",
                flyctl.to_string_lossy().to_string(),
            ),
            ("WRELA_DEPLOY_CURL_BIN", curl.to_string_lossy().to_string()),
            (
                "WRELA_TEST_FLYCTL_LOG",
                flyctl_log.to_string_lossy().to_string(),
            ),
            (
                "WRELA_TEST_FLYCTL_STATE",
                flyctl_state.to_string_lossy().to_string(),
            ),
            (
                "WRELA_TEST_CURL_LOG",
                curl_log.to_string_lossy().to_string(),
            ),
            ("WRELA_DEPLOY_GATE_ATTEMPTS", "2".to_string()),
            ("WRELA_DEPLOY_GATE_SLEEP_MS", "1".to_string()),
        ]);

        let report = FlyProvider::new()
            .deploy(&request)
            .expect("deploy should pass");
        assert_eq!(
            report.pre_health.get("ok").and_then(Value::as_bool),
            Some(true)
        );
        assert!(report.pre_health.get("skipped").is_none());
        assert!(report.post_probe.get("global").is_some());
        assert!(report.post_probe.get("perMachine").is_some());
        let flyctl_invocations = std::fs::read_to_string(&flyctl_log).expect("read flyctl log");
        assert!(!flyctl_invocations.contains("secrets set"));
    }

    #[test]
    fn deploy_reconciles_multi_region_machine_topology() {
        let _lock = ENV_LOCK.lock().expect("env lock");
        let dir = tempfile::tempdir().expect("tempdir");
        let mut request = deploy_request(&dir);
        request.machines = 5;
        request.target_voters = 5;
        request.replication_factor = 5;
        request.write_quorum = 3;
        request.region_machine_counts.clear();
        request.region_machine_counts.insert("ord".to_string(), 3);
        request.region_machine_counts.insert("iad".to_string(), 2);

        let flyctl = dir.path().join("flyctl.sh");
        let curl = dir.path().join("curl.sh");
        let flyctl_log = dir.path().join("flyctl.log");
        let flyctl_state = dir.path().join("flyctl.state");
        let curl_log = dir.path().join("curl.log");

        write_executable(
            &flyctl,
            r#"#!/usr/bin/env bash
set -euo pipefail
log="${WRELA_TEST_FLYCTL_LOG:?}"
state="${WRELA_TEST_FLYCTL_STATE:?}"
echo "$*" >> "$log"
if [[ "$1" == "apps" && "$2" == "create" ]]; then exit 0; fi
if [[ "$1" == "machines" && "$2" == "list" ]]; then
  count=0
  if [[ -f "$state" ]]; then count="$(cat "$state")"; fi
  count=$((count + 1))
  echo "$count" > "$state"
  if [[ "$count" -eq 1 ]]; then
    echo "[]"
  else
    echo '[{"id":"m-1","region":"ord"},{"id":"m-2","region":"ord"},{"id":"m-3","region":"ord"},{"id":"m-4","region":"iad"},{"id":"m-5","region":"iad"}]'
  fi
  exit 0
fi
if [[ "$1" == "deploy" ]]; then exit 0; fi
if [[ "$1" == "scale" && "$2" == "count" ]]; then
  joined="$*"
  if [[ "$joined" == *"--region ord"* || "$joined" == *"--region iad"* ]]; then
    exit 0
  fi
  exit 1
fi
if [[ "$1" == "secrets" && "$2" == "set" ]]; then exit 0; fi
echo "{}"
"#,
        );
        write_executable(
            &curl,
            r#"#!/usr/bin/env bash
set -euo pipefail
log="${WRELA_TEST_CURL_LOG:?}"
echo "$*" >> "$log"
url="${@: -1}"
target=""
prev=""
for arg in "$@"; do
  if [[ "$prev" == "-H" && "$arg" == fly-force-instance-id:* ]]; then
    target="${arg#fly-force-instance-id: }"
  fi
  prev="$arg"
done
if [[ "$url" == *"/api/health" ]]; then
  echo '{"ok":true}'
  exit 0
fi
if [[ "$url" == *"/api/probe/write" ]]; then
  if [[ -n "$target" ]]; then
    echo "{\"ok\":true,\"machineId\":\"$target\",\"value\":\"seed\",\"requiredAcks\":1,\"replicationAcks\":1,\"version\":1}"
  else
    echo '{"ok":true,"value":"seed","requiredAcks":1,"replicationAcks":1,"version":1}'
  fi
  exit 0
fi
if [[ "$url" == *"/api/probe/read" ]]; then
  if [[ -z "$target" ]]; then target="m-1"; fi
  echo "{\"ok\":true,\"machineId\":\"$target\",\"value\":\"seed\"}"
  exit 0
fi
if [[ "$url" == *"/api/probe/cluster_read" ]]; then
  echo '{"ok":true,"readings":[{"ok":true,"value":"seed"},{"ok":true,"value":"seed"},{"ok":true,"value":"seed"}],"value":"seed"}'
  exit 0
fi
echo '{}'
"#,
        );

        let _env = set_env_for_test(&[
            (
                "WRELA_DEPLOY_FLYCTL_BIN",
                flyctl.to_string_lossy().to_string(),
            ),
            ("WRELA_DEPLOY_CURL_BIN", curl.to_string_lossy().to_string()),
            (
                "WRELA_TEST_FLYCTL_LOG",
                flyctl_log.to_string_lossy().to_string(),
            ),
            (
                "WRELA_TEST_FLYCTL_STATE",
                flyctl_state.to_string_lossy().to_string(),
            ),
            (
                "WRELA_TEST_CURL_LOG",
                curl_log.to_string_lossy().to_string(),
            ),
            ("WRELA_DEPLOY_GATE_ATTEMPTS", "2".to_string()),
            ("WRELA_DEPLOY_GATE_SLEEP_MS", "1".to_string()),
        ]);

        FlyProvider::new()
            .deploy(&request)
            .expect("deploy should pass");

        let flyctl_invocations = std::fs::read_to_string(&flyctl_log).expect("read flyctl log");
        assert!(flyctl_invocations.contains("scale count 3 --region ord"));
        assert!(flyctl_invocations.contains("scale count 2 --region iad"));
        assert!(!flyctl_invocations.contains("secrets set"));
    }

    #[test]
    fn deploy_uses_remote_depot_builder_policy() {
        let _lock = ENV_LOCK.lock().expect("env lock");
        let dir = tempfile::tempdir().expect("tempdir");
        let request = deploy_request(&dir);
        let flyctl = dir.path().join("flyctl.sh");
        let curl = dir.path().join("curl.sh");
        let flyctl_log = dir.path().join("flyctl.log");
        let flyctl_state = dir.path().join("flyctl.state");
        let curl_log = dir.path().join("curl.log");

        write_executable(
            &flyctl,
            r#"#!/usr/bin/env bash
set -euo pipefail
log="${WRELA_TEST_FLYCTL_LOG:?}"
echo "$*" >> "$log"
if [[ "$1" == "apps" && "$2" == "create" ]]; then exit 0; fi
if [[ "$1" == "machines" && "$2" == "list" ]]; then
  echo '[{"id":"m-1"},{"id":"m-2"},{"id":"m-3"}]'
  exit 0
fi
if [[ "$1" == "deploy" ]]; then
  joined="$*"
  [[ "$joined" == *"--remote-only"* ]] || exit 1
  [[ "$joined" == *"--depot=true"* ]] || exit 1
  [[ "$joined" != *"--depot=false"* ]] || exit 1
  exit 0
fi
if [[ "$1" == "scale" && "$2" == "count" ]]; then exit 0; fi
if [[ "$1" == "secrets" && "$2" == "set" ]]; then exit 0; fi
echo "{}"
"#,
        );
        write_executable(
            &curl,
            r#"#!/usr/bin/env bash
set -euo pipefail
log="${WRELA_TEST_CURL_LOG:?}"
echo "$*" >> "$log"
url="${@: -1}"
target=""
prev=""
for arg in "$@"; do
  if [[ "$prev" == "-H" && "$arg" == fly-force-instance-id:* ]]; then
    target="${arg#fly-force-instance-id: }"
  fi
  prev="$arg"
done
if [[ "$url" == *"/api/health" ]]; then
  echo '{"ok":true}'
  exit 0
fi
if [[ "$url" == *"/api/probe/write" ]]; then
  if [[ -n "$target" ]]; then
    echo "{\"ok\":true,\"machineId\":\"$target\",\"value\":\"seed\",\"requiredAcks\":1,\"replicationAcks\":1,\"version\":1}"
  else
    echo '{"ok":true,"value":"seed","requiredAcks":1,"replicationAcks":1,"version":1}'
  fi
  exit 0
fi
if [[ "$url" == *"/api/probe/read" ]]; then
  if [[ -z "$target" ]]; then target="m-1"; fi
  echo "{\"ok\":true,\"machineId\":\"$target\",\"value\":\"seed\"}"
  exit 0
fi
if [[ "$url" == *"/api/probe/cluster_read" ]]; then
  echo '{"ok":true,"readings":[{"ok":true,"value":"seed"},{"ok":true,"value":"seed"},{"ok":true,"value":"seed"}],"value":"seed"}'
  exit 0
fi
echo '{}'
"#,
        );

        let _env = set_env_for_test(&[
            (
                "WRELA_DEPLOY_FLYCTL_BIN",
                flyctl.to_string_lossy().to_string(),
            ),
            ("WRELA_DEPLOY_CURL_BIN", curl.to_string_lossy().to_string()),
            (
                "WRELA_TEST_FLYCTL_LOG",
                flyctl_log.to_string_lossy().to_string(),
            ),
            (
                "WRELA_TEST_FLYCTL_STATE",
                flyctl_state.to_string_lossy().to_string(),
            ),
            (
                "WRELA_TEST_CURL_LOG",
                curl_log.to_string_lossy().to_string(),
            ),
            ("WRELA_DEPLOY_GATE_ATTEMPTS", "2".to_string()),
            ("WRELA_DEPLOY_GATE_SLEEP_MS", "1".to_string()),
        ]);

        let report = FlyProvider::new()
            .deploy(&request)
            .expect("deploy should pass via remote depot policy");
        assert!(
            report
                .notes
                .iter()
                .any(|note| note == "builder=fly-remote-depot")
        );
        let flyctl_invocations = std::fs::read_to_string(&flyctl_log).expect("read flyctl log");
        assert!(flyctl_invocations.contains("--depot=true"));
        assert!(!flyctl_invocations.contains("--depot=false"));
        assert!(!flyctl_invocations.contains("secrets set"));
    }

    #[test]
    fn deploy_fails_closed_when_remote_depot_deploy_fails() {
        let _lock = ENV_LOCK.lock().expect("env lock");
        let dir = tempfile::tempdir().expect("tempdir");
        let request = deploy_request(&dir);
        let flyctl = dir.path().join("flyctl.sh");
        let curl = dir.path().join("curl.sh");
        let flyctl_log = dir.path().join("flyctl.log");
        let flyctl_state = dir.path().join("flyctl.state");
        let curl_log = dir.path().join("curl.log");

        write_executable(
            &flyctl,
            r#"#!/usr/bin/env bash
set -euo pipefail
log="${WRELA_TEST_FLYCTL_LOG:?}"
echo "$*" >> "$log"
if [[ "$1" == "apps" && "$2" == "create" ]]; then exit 0; fi
if [[ "$1" == "machines" && "$2" == "list" ]]; then
  echo '[{"id":"m-1"},{"id":"m-2"},{"id":"m-3"}]'
  exit 0
fi
if [[ "$1" == "deploy" ]]; then
  echo 'Error: depot remote builder unavailable' >&2
  exit 1
fi
if [[ "$1" == "scale" && "$2" == "count" ]]; then exit 0; fi
if [[ "$1" == "secrets" && "$2" == "set" ]]; then exit 0; fi
echo "{}"
"#,
        );
        write_executable(
            &curl,
            r#"#!/usr/bin/env bash
set -euo pipefail
log="${WRELA_TEST_CURL_LOG:?}"
echo "$*" >> "$log"
url="${@: -1}"
target=""
prev=""
for arg in "$@"; do
  if [[ "$prev" == "-H" && "$arg" == fly-force-instance-id:* ]]; then
    target="${arg#fly-force-instance-id: }"
  fi
  prev="$arg"
done
if [[ "$url" == *"/api/health" ]]; then
  echo '{"ok":true}'
  exit 0
fi
if [[ "$url" == *"/api/probe/write" ]]; then
  if [[ -n "$target" ]]; then
    echo "{\"ok\":true,\"machineId\":\"$target\",\"value\":\"seed\",\"requiredAcks\":1,\"replicationAcks\":1,\"version\":1}"
  else
    echo '{"ok":true,"value":"seed","requiredAcks":1,"replicationAcks":1,"version":1}'
  fi
  exit 0
fi
if [[ "$url" == *"/api/probe/read" ]]; then
  if [[ -z "$target" ]]; then target="m-1"; fi
  echo "{\"ok\":true,\"machineId\":\"$target\",\"value\":\"seed\"}"
  exit 0
fi
if [[ "$url" == *"/api/probe/cluster_read" ]]; then
  echo '{"ok":true,"readings":[{"ok":true,"value":"seed"},{"ok":true,"value":"seed"},{"ok":true,"value":"seed"}],"value":"seed"}'
  exit 0
fi
echo '{}'
"#,
        );

        let _env = set_env_for_test(&[
            (
                "WRELA_DEPLOY_FLYCTL_BIN",
                flyctl.to_string_lossy().to_string(),
            ),
            ("WRELA_DEPLOY_CURL_BIN", curl.to_string_lossy().to_string()),
            (
                "WRELA_TEST_FLYCTL_LOG",
                flyctl_log.to_string_lossy().to_string(),
            ),
            (
                "WRELA_TEST_FLYCTL_STATE",
                flyctl_state.to_string_lossy().to_string(),
            ),
            (
                "WRELA_TEST_CURL_LOG",
                curl_log.to_string_lossy().to_string(),
            ),
            ("WRELA_DEPLOY_GATE_ATTEMPTS", "2".to_string()),
            ("WRELA_DEPLOY_GATE_SLEEP_MS", "1".to_string()),
        ]);

        let err = FlyProvider::new()
            .deploy(&request)
            .expect_err("deploy should fail closed");
        assert!(err.contains("depot remote builder unavailable"));
        let flyctl_invocations = std::fs::read_to_string(&flyctl_log).expect("read flyctl log");
        assert!(flyctl_invocations.contains("--depot=true"));
        assert!(!flyctl_invocations.contains("--depot=false"));
        assert!(!flyctl_invocations.contains("secrets set"));
    }

    #[test]
    fn deploy_fails_closed_when_health_gate_never_passes() {
        let _lock = ENV_LOCK.lock().expect("env lock");
        let dir = tempfile::tempdir().expect("tempdir");
        let request = deploy_request(&dir);
        let flyctl = dir.path().join("flyctl.sh");
        let curl = dir.path().join("curl.sh");
        let flyctl_log = dir.path().join("flyctl.log");
        let flyctl_state = dir.path().join("flyctl.state");
        let curl_log = dir.path().join("curl.log");

        write_executable(
            &flyctl,
            r#"#!/usr/bin/env bash
set -euo pipefail
if [[ "$1" == "apps" && "$2" == "create" ]]; then exit 0; fi
if [[ "$1" == "machines" && "$2" == "list" ]]; then
  echo '[{"id":"m-1"},{"id":"m-2"},{"id":"m-3"}]'
  exit 0
fi
if [[ "$1" == "scale" && "$2" == "count" ]]; then exit 0; fi
if [[ "$1" == "secrets" && "$2" == "set" ]]; then exit 0; fi
if [[ "$1" == "deploy" ]]; then exit 0; fi
echo "{}"
"#,
        );
        write_executable(
            &curl,
            r#"#!/usr/bin/env bash
set -euo pipefail
url="${@: -1}"
if [[ "$url" == *"/api/health" ]]; then
  echo '{"ok":false}'
  exit 0
fi
echo '{"ok":true}'
"#,
        );

        let _env = set_env_for_test(&[
            (
                "WRELA_DEPLOY_FLYCTL_BIN",
                flyctl.to_string_lossy().to_string(),
            ),
            ("WRELA_DEPLOY_CURL_BIN", curl.to_string_lossy().to_string()),
            (
                "WRELA_TEST_FLYCTL_LOG",
                flyctl_log.to_string_lossy().to_string(),
            ),
            (
                "WRELA_TEST_FLYCTL_STATE",
                flyctl_state.to_string_lossy().to_string(),
            ),
            (
                "WRELA_TEST_CURL_LOG",
                curl_log.to_string_lossy().to_string(),
            ),
            ("WRELA_DEPLOY_GATE_ATTEMPTS", "1".to_string()),
            ("WRELA_DEPLOY_GATE_SLEEP_MS", "1".to_string()),
        ]);

        let err = FlyProvider::new()
            .deploy(&request)
            .expect_err("deploy should fail");
        assert!(
            err.contains("timed out waiting for fly deploy gates"),
            "unexpected error: {err}"
        );
    }
}
