mod policy;
pub mod provider;
pub mod providers;

use self::policy::{
    CheckpointBackend, DeployPolicyOverrides, ResolvedDeployPolicy, resolve_deploy_policy,
};
use self::provider::{DeployProvider, DeployRequest};
use self::providers::fly::FlyProvider;
use super::contracts::{EXIT_CODEGEN, EXIT_OK, EXIT_USAGE};
use std::collections::BTreeMap;
use std::env;
use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct DeployCommandInput {
    pub trace: bool,
    pub path_arg: Option<String>,
    pub program_args: Vec<String>,
    pub target: Option<String>,
    pub app: Option<String>,
    pub region: Option<String>,
    pub machines: Option<usize>,
    pub deploy_policy: Option<String>,
    pub replication_factor: Option<u32>,
    pub write_quorum: Option<u32>,
    pub logical_shards: Option<u32>,
    pub active_groups: Option<u32>,
    pub force: bool,
    pub generate_only: bool,
}

pub fn execute_deploy_command(input: DeployCommandInput) -> i32 {
    if !input.program_args.is_empty() {
        eprintln!("error: unexpected extra arguments for deploy command");
        return EXIT_USAGE;
    }

    let target = input
        .target
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("fly");
    if target != "fly" {
        eprintln!("error: unsupported deploy target `{target}` (expected `fly`)");
        return EXIT_USAGE;
    }

    let project_root = match resolve_project_root(input.path_arg.as_deref()) {
        Ok(path) => path,
        Err(err) => {
            eprintln!("error: {err}");
            return EXIT_USAGE;
        }
    };

    let app = match input
        .app
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(value) => value.to_string(),
        None => {
            eprintln!("error: missing required --app value for `wrela deploy --target=fly`");
            return EXIT_USAGE;
        }
    };

    let default_region = input
        .region
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| {
            env::var("PRIMARY_REGION")
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
        .unwrap_or_else(|| "ord".to_string());

    let (deploy_context_root, build_mode) = match detect_docker_build_mode(&project_root) {
        Ok(result) => result,
        Err(err) => {
            eprintln!("error: {err}");
            return EXIT_USAGE;
        }
    };

    let deploy_policy = match resolve_deploy_policy(
        &project_root,
        &default_region,
        &DeployPolicyOverrides {
            policy_path: input.deploy_policy.clone(),
            region: input.region.clone(),
            machines: input.machines,
            replication_factor: input.replication_factor,
            write_quorum: input.write_quorum,
            logical_shards: input.logical_shards,
            active_groups: input.active_groups,
        },
    ) {
        Ok(policy) => policy,
        Err(err) => {
            eprintln!("error: {err}");
            return EXIT_USAGE;
        }
    };
    let assets = match generate_fly_assets(
        &project_root,
        &app,
        &deploy_policy,
        input.force,
        &build_mode,
    ) {
        Ok(assets) => assets,
        Err(err) => {
            eprintln!("deploy asset generation error: {err}");
            return EXIT_USAGE;
        }
    };

    if input.trace {
        eprintln!("deploy: generated {}", assets.config_path.display());
        eprintln!("deploy: generated {}", assets.dockerfile_path.display());
        eprintln!("deploy: context {}", deploy_context_root.display());
        eprintln!(
            "deploy: policy region_map={}",
            json_string_map_usize(&deploy_policy.region_machine_counts)
        );
        eprintln!(
            "deploy: policy rf={} wq={} logical_shards={} active_groups={}",
            deploy_policy.replication_factor,
            deploy_policy.write_quorum,
            deploy_policy.logical_shards,
            deploy_policy.active_groups
        );
    }

    if input.generate_only {
        println!(
            "generated deploy assets:\n- {}\n- {}",
            assets.config_path.display(),
            assets.dockerfile_path.display()
        );
        return EXIT_OK;
    }

    let request = DeployRequest {
        project_root: project_root.clone(),
        deploy_context_root,
        app: app.clone(),
        region: deploy_policy.primary_region.clone(),
        machines: deploy_policy.machine_count,
        region_machine_counts: deploy_policy.region_machine_counts.clone(),
        target_voters: deploy_policy.target_voters,
        replication_factor: deploy_policy.replication_factor,
        write_quorum: deploy_policy.write_quorum,
        config_path: assets.config_path.clone(),
        dockerfile_path: assets.dockerfile_path.clone(),
    };

    let provider = FlyProvider::new();
    let report = match provider.deploy(&request) {
        Ok(report) => report,
        Err(err) => {
            eprintln!("deploy failed: {err}");
            return EXIT_CODEGEN;
        }
    };

    if let Err(err) = write_report(&report) {
        eprintln!("deploy succeeded but failed to write report: {err}");
    }

    println!("deploy succeeded: {}", report.url);
    EXIT_OK
}

#[derive(Debug, Clone)]
struct GeneratedAssets {
    config_path: PathBuf,
    dockerfile_path: PathBuf,
}

#[derive(Debug, Clone)]
enum DockerfileBuildMode {
    Workspace { project_path_from_context: String },
}

fn resolve_project_root(path_arg: Option<&str>) -> Result<PathBuf, String> {
    let path = PathBuf::from(path_arg.unwrap_or("."));
    let canonical = path
        .canonicalize()
        .map_err(|err| format!("failed to resolve deploy path {}: {}", path.display(), err))?;
    let mut cursor = if canonical.is_file() {
        canonical
            .parent()
            .ok_or_else(|| {
                format!(
                    "failed to resolve project root from {}",
                    canonical.display()
                )
            })?
            .to_path_buf()
    } else {
        canonical
    };

    loop {
        if cursor.join("src").join("main.wr").is_file() {
            return Ok(cursor);
        }
        if !cursor.pop() {
            break;
        }
    }

    Err("`wrela deploy` phase-1 requires an all-Wrela project root with `src/main.wr`".to_string())
}

fn generate_fly_assets(
    project_root: &Path,
    app: &str,
    deploy_policy: &ResolvedDeployPolicy,
    force: bool,
    build_mode: &DockerfileBuildMode,
) -> Result<GeneratedAssets, String> {
    let config_path = project_root.join("fly.toml");
    let dockerfile_path = project_root.join("Dockerfile");

    if !force {
        if config_path.exists() {
            return Err(format!(
                "refusing to overwrite existing {} (pass --force to replace)",
                config_path.display()
            ));
        }
        if dockerfile_path.exists() {
            return Err(format!(
                "refusing to overwrite existing {} (pass --force to replace)",
                dockerfile_path.display()
            ));
        }
    }

    let config = fly_toml_template(app, deploy_policy);
    fs::write(&config_path, config)
        .map_err(|err| format!("failed to write {}: {}", config_path.display(), err))?;

    fs::write(&dockerfile_path, dockerfile_template(build_mode))
        .map_err(|err| format!("failed to write {}: {}", dockerfile_path.display(), err))?;

    Ok(GeneratedAssets {
        config_path,
        dockerfile_path,
    })
}

fn fly_toml_template(app: &str, deploy_policy: &ResolvedDeployPolicy) -> String {
    let mut out = String::new();
    writeln!(&mut out, "app = {}", toml_string(app)).expect("write app");
    writeln!(
        &mut out,
        "primary_region = {}",
        toml_string(&deploy_policy.primary_region)
    )
    .expect("write region");
    out.push_str("kill_signal = \"SIGINT\"\nkill_timeout = \"30s\"\n\n");

    out.push_str("[build]\n  dockerfile = \"Dockerfile\"\n\n");

    out.push_str("[deploy]\n");
    out.push_str("  strategy = \"rolling\"\n");
    out.push_str("  max_unavailable = 1\n");
    out.push_str("  wait_timeout = \"10m\"\n\n");

    out.push_str("[env]\n");
    push_toml_env(&mut out, "PRIMARY_REGION", &deploy_policy.primary_region);
    push_toml_env(&mut out, "WRELADB_REGION", &deploy_policy.primary_region);
    push_toml_env(
        &mut out,
        "WRELADB_INTENT_JSON",
        &json_intent_policy(&deploy_policy.intent),
    );
    push_toml_env(&mut out, "WRELADB_DATA_DIR", "/data/wreladb");
    push_toml_env(
        &mut out,
        "WRELADB_TARGET_VOTERS",
        &deploy_policy.target_voters.to_string(),
    );
    push_toml_env(
        &mut out,
        "WRELADB_REPLICATION_FACTOR",
        &deploy_policy.replication_factor.to_string(),
    );
    push_toml_env(
        &mut out,
        "WRELADB_WRITE_QUORUM",
        &deploy_policy.write_quorum.to_string(),
    );
    push_toml_env(
        &mut out,
        "WRELADB_REPLICATION_ASYNC_FAILOVER",
        if deploy_policy.replication_async_failover {
            "1"
        } else {
            "0"
        },
    );
    push_toml_env(
        &mut out,
        "WRELADB_LOGICAL_SHARDS",
        &deploy_policy.logical_shards.to_string(),
    );
    push_toml_env(
        &mut out,
        "WRELADB_ACTIVE_GROUPS",
        &deploy_policy.active_groups.to_string(),
    );
    push_toml_env(&mut out, "WRELADB_PRIVATE_RPC_ENABLED", "1");
    push_toml_env(&mut out, "WRELADB_PRIVATE_RPC_PORT", "19091");
    push_toml_env(
        &mut out,
        "WRELADB_PRIVATE_RPC_MTLS_MODE",
        &deploy_policy.mtls_mode,
    );
    push_toml_env(
        &mut out,
        "WRELADB_PRIVATE_RPC_TRUSTED_NETWORK",
        "fly-wireguard",
    );
    push_toml_env(
        &mut out,
        "WRELADB_PRIVATE_RPC_MIN_READY_NODES",
        &deploy_policy.target_voters.to_string(),
    );
    push_toml_env(&mut out, "WRELADB_PRIVATE_RPC_DISCOVERY_REFRESH_MS", "5000");
    push_toml_env(&mut out, "WRELADB_PRIVATE_RPC_DISCOVERY_TIMEOUT_MS", "1000");
    push_toml_env(
        &mut out,
        "WRELADB_REGION_MACHINE_MAP_JSON",
        &json_string_map_usize(&deploy_policy.region_machine_counts),
    );
    push_toml_env(
        &mut out,
        "WRELADB_TOPOLOGY_MODE",
        deploy_policy.topology_mode.as_env_value(),
    );
    push_toml_env(
        &mut out,
        "WRELADB_TOPOLOGY_REGION_NODE_MAP_JSON",
        &json_region_node_map(&deploy_policy.region_node_map),
    );
    push_toml_env(
        &mut out,
        "WRELADB_TOPOLOGY_REGION_AZ_NODE_MAP_JSON",
        &json_region_az_node_map(&deploy_policy.region_az_node_map),
    );
    push_toml_env(
        &mut out,
        "WRELADB_SOVEREIGNTY_ID",
        &deploy_policy.sovereignty_id,
    );
    push_toml_env(
        &mut out,
        "WRELADB_SOVEREIGNTY_ALLOWED_REGIONS",
        &deploy_policy.sovereignty_allowed_regions.join(","),
    );
    push_toml_env(
        &mut out,
        "WRELADB_SOVEREIGNTY_ENFORCE_ALL_COPIES",
        if deploy_policy.sovereignty_enforce_all_copies {
            "1"
        } else {
            "0"
        },
    );
    push_toml_env(
        &mut out,
        "WRELADB_CHECKPOINT_ALLOWED_REGIONS",
        &deploy_policy.checkpoint_allowed_regions.join(","),
    );
    if let Some(policy) = deploy_policy.residency_policy_json.as_deref() {
        push_toml_env(&mut out, "WRELADB_RESIDENCY_POLICY_JSON", policy);
    }
    if let Some(locality) = deploy_policy.shard_group_locality_json.as_deref() {
        push_toml_env(&mut out, "WRELADB_SHARD_GROUP_LOCALITY_JSON", locality);
    }

    push_toml_env(
        &mut out,
        "WRELADB_CHECKPOINT_BACKEND",
        deploy_policy.checkpoint.backend.as_env_value(),
    );
    if deploy_policy.checkpoint.backend == CheckpointBackend::S3 {
        push_toml_env(
            &mut out,
            "WRELADB_S3_PREFIX",
            &deploy_policy.checkpoint.s3_prefix,
        );
        push_toml_env(
            &mut out,
            "WRELADB_S3_PATH_STYLE",
            if deploy_policy.checkpoint.s3_path_style {
                "1"
            } else {
                "0"
            },
        );
        if let Some(bucket) = deploy_policy.checkpoint.s3_bucket.as_deref() {
            push_toml_env(&mut out, "WRELADB_S3_BUCKET", bucket);
        }
        if let Some(region) = deploy_policy.checkpoint.s3_region.as_deref() {
            push_toml_env(&mut out, "WRELADB_S3_REGION", region);
        }
        if let Some(endpoint) = deploy_policy.checkpoint.s3_endpoint.as_deref() {
            push_toml_env(&mut out, "WRELADB_S3_ENDPOINT", endpoint);
        }
        if !deploy_policy.checkpoint.s3_bucket_by_region.is_empty() {
            push_toml_env(
                &mut out,
                "WRELADB_S3_BUCKET_BY_REGION_JSON",
                &json_string_map(&deploy_policy.checkpoint.s3_bucket_by_region),
            );
        }
        if !deploy_policy.checkpoint.s3_region_by_region.is_empty() {
            push_toml_env(
                &mut out,
                "WRELADB_S3_REGION_BY_REGION_JSON",
                &json_string_map(&deploy_policy.checkpoint.s3_region_by_region),
            );
        }
        if !deploy_policy.checkpoint.s3_endpoint_by_region.is_empty() {
            push_toml_env(
                &mut out,
                "WRELADB_S3_ENDPOINT_BY_REGION_JSON",
                &json_string_map(&deploy_policy.checkpoint.s3_endpoint_by_region),
            );
        }
    }
    out.push('\n');

    out.push_str("[http_service]\n");
    out.push_str("  internal_port = 8080\n");
    out.push_str("  force_https = true\n");
    out.push_str("  auto_stop_machines = false\n");
    out.push_str("  auto_start_machines = true\n");
    writeln!(
        &mut out,
        "  min_machines_running = {}",
        deploy_policy.machine_count.max(1)
    )
    .expect("write min machines");
    out.push_str("  processes = [\"app\"]\n\n");

    out.push_str("  [http_service.concurrency]\n");
    out.push_str("    type = \"requests\"\n");
    out.push_str("    hard_limit = 100\n");
    out.push_str("    soft_limit = 50\n\n");

    out.push_str("  [[http_service.checks]]\n");
    out.push_str("    grace_period = \"30s\"\n");
    out.push_str("    interval = \"15s\"\n");
    out.push_str("    method = \"GET\"\n");
    out.push_str("    timeout = \"10s\"\n");
    out.push_str("    path = \"/api/live\"\n\n");

    out.push_str("  [[http_service.machine_checks]]\n");
    out.push_str("    image = \"curlimages/curl:8.7.1\"\n");
    out.push_str("    entrypoint = [\"/bin/sh\", \"-c\"]\n");
    out.push_str("    command = [\"for i in $(seq 1 60); do curl -fsS --connect-timeout 3 --max-time 5 http://[$FLY_TEST_MACHINE_IP]:8080/api/live >/dev/null && exit 0; sleep 1; done; exit 1\"]\n");
    out.push_str("    kill_signal = \"SIGKILL\"\n");
    out.push_str("    kill_timeout = \"30s\"\n\n");

    out.push_str("[[vm]]\n");
    out.push_str("  cpu_kind = \"shared\"\n");
    out.push_str("  cpus = 1\n");
    out.push_str("  memory_mb = 256\n");
    out
}

fn push_toml_env(buffer: &mut String, key: &str, value: &str) {
    let _ = writeln!(buffer, "  {key} = {}", toml_string(value));
}

fn toml_string(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

fn json_string_map(values: &BTreeMap<String, String>) -> String {
    serde_json::to_string(values).unwrap_or_else(|_| "{}".to_string())
}

fn json_string_map_usize(values: &BTreeMap<String, usize>) -> String {
    serde_json::to_string(values).unwrap_or_else(|_| "{}".to_string())
}

fn json_region_az_node_map(values: &BTreeMap<String, BTreeMap<String, Vec<String>>>) -> String {
    serde_json::to_string(values).unwrap_or_else(|_| "{}".to_string())
}

fn json_region_node_map(values: &BTreeMap<String, Vec<String>>) -> String {
    serde_json::to_string(values).unwrap_or_else(|_| "{}".to_string())
}

fn json_intent_policy(value: &policy::ResolvedIntentPolicy) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string())
}

fn dockerfile_template(build_mode: &DockerfileBuildMode) -> String {
    match build_mode {
        DockerfileBuildMode::Workspace {
            project_path_from_context,
        } => format!(
            "FROM --platform=linux/amd64 rust:1.92-bookworm AS build\n\
WORKDIR /src\n\
COPY . .\n\
RUN cargo run -p wrela -- build {project_path_from_context} -o /tmp/wrela_app\n\
\n\
FROM --platform=linux/amd64 ubuntu:24.04\n\
RUN apt-get update \\\n\
 && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends ca-certificates \\\n\
 && rm -rf /var/lib/apt/lists/* \\\n\
 && useradd -m app\n\
WORKDIR /app\n\
COPY --from=build /tmp/wrela_app /app/wrela_app\n\
RUN mkdir -p /data/wreladb && chown -R app:app /app /data\n\
USER app\n\
ENV PORT=8080\n\
EXPOSE 8080\n\
CMD [\"/app/wrela_app\"]\n"
        ),
    }
}

fn detect_docker_build_mode(project_root: &Path) -> Result<(PathBuf, DockerfileBuildMode), String> {
    if let Some(workspace_root) = find_wrela_workspace_root(project_root)
        && let Ok(relative) = project_root.strip_prefix(&workspace_root)
    {
        let relative_text = relative.to_string_lossy().replace('\\', "/");
        if !relative_text.is_empty() {
            return Ok((
                workspace_root,
                DockerfileBuildMode::Workspace {
                    project_path_from_context: relative_text,
                },
            ));
        }
    }
    Err(
        "`wrela deploy` phase-1 requires a project inside the Wrela workspace so Fly remote \
builder can compile from source; standalone release-tar deploy is disabled"
            .to_string(),
    )
}

fn find_wrela_workspace_root(start: &Path) -> Option<PathBuf> {
    for ancestor in start.ancestors() {
        if ancestor.join("compiler").join("Cargo.toml").is_file()
            && ancestor.join("runtime").join("Cargo.toml").is_file()
        {
            return Some(ancestor.to_path_buf());
        }
    }
    None
}

fn write_report(report: &provider::DeployReport) -> Result<(), String> {
    let dir = PathBuf::from("artifacts").join("fly");
    fs::create_dir_all(&dir)
        .map_err(|err| format!("failed to create {}: {}", dir.display(), err))?;
    let path = dir.join(format!("{}-deploy-report.json", report.app));
    let payload = serde_json::to_vec_pretty(report).map_err(|err| err.to_string())?;
    fs::write(&path, payload)
        .map_err(|err| format!("failed to write report {}: {}", path.display(), err))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::policy::{
        CheckpointPolicy, DeployPolicyOverrides, TopologyMode, resolve_deploy_policy,
    };
    use super::*;
    use std::sync::Mutex;
    use tempfile::tempdir;
    use wrela_runtime::db::DbConfig;
    use wrela_runtime::db::autopilot::compiler::{
        CostGuardrailMode, DbIntentConfig, DbWorkloadClass, NamespacePolicy,
    };

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    const VALID_INTENT_BLOCK: &str = r#"
[intent]
policy_id = "orders-v3"
workload_class = "general_transactional"
latency_target_ms = 80
min_write_throughput_ops = 5000
residency_scope = ["us-east", "eu-west"]
cost_guardrail_mode = "estimate_and_warn_only"

[intent.namespace_policies]
orders = "warm"
"#;

    struct EnvGuard {
        snapshot: Vec<(String, String)>,
    }

    impl EnvGuard {
        fn set(env_map: &BTreeMap<String, String>) -> Self {
            let snapshot = std::env::vars()
                .filter(|(key, _)| key.starts_with("WRELADB_"))
                .collect::<Vec<_>>();
            for (key, _) in &snapshot {
                // SAFETY: guarded by ENV_LOCK for test-local env mutation.
                unsafe { std::env::remove_var(key) };
            }
            for (key, value) in env_map {
                // SAFETY: guarded by ENV_LOCK for test-local env mutation.
                unsafe { std::env::set_var(key, value) };
            }
            Self { snapshot }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for (key, _) in std::env::vars().filter(|(key, _)| key.starts_with("WRELADB_")) {
                // SAFETY: guarded by ENV_LOCK for test-local env mutation.
                unsafe { std::env::remove_var(key) };
            }
            for (key, value) in &self.snapshot {
                // SAFETY: guarded by ENV_LOCK for test-local env mutation.
                unsafe { std::env::set_var(key, value) };
            }
        }
    }

    fn resolve_policy_and_emit_env(
        topology_block: &str,
        default_region: &str,
    ) -> (ResolvedDeployPolicy, BTreeMap<String, String>) {
        let temp = tempdir().expect("tempdir");
        let policy = format!("{topology_block}\n{VALID_INTENT_BLOCK}");
        std::fs::write(temp.path().join("wrela.deploy.toml"), policy).expect("write policy");
        let resolved = resolve_deploy_policy(
            temp.path(),
            default_region,
            &DeployPolicyOverrides::default(),
        )
        .expect("resolve deploy policy");
        let fly_toml = fly_toml_template("integration-app", &resolved);
        let parsed_toml: toml::Value = toml::from_str(&fly_toml).expect("parse fly.toml");
        let env_table = parsed_toml
            .get("env")
            .and_then(toml::Value::as_table)
            .expect("env table");
        let env_map = env_table
            .iter()
            .map(|(key, value)| {
                (
                    key.to_string(),
                    value.as_str().expect("env var string").to_string(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        (resolved, env_map)
    }

    #[test]
    fn fly_template_emits_strict_cutover_env_vars() {
        let policy = ResolvedDeployPolicy {
            primary_region: "ord".to_string(),
            region_machine_counts: BTreeMap::from([("ord".to_string(), 3usize)]),
            topology_mode: self::policy::TopologyMode::Collapsed,
            region_node_map: BTreeMap::from([(
                "ord".to_string(),
                vec!["n1".to_string(), "n2".to_string(), "n3".to_string()],
            )]),
            region_az_node_map: BTreeMap::from([(
                "ord".to_string(),
                BTreeMap::from([(
                    "ord".to_string(),
                    vec!["n1".to_string(), "n2".to_string(), "n3".to_string()],
                )]),
            )]),
            machine_count: 3,
            target_voters: 3,
            replication_factor: 3,
            write_quorum: 2,
            replication_async_failover: true,
            logical_shards: 16,
            active_groups: 3,
            checkpoint: CheckpointPolicy {
                backend: CheckpointBackend::File,
                s3_bucket: None,
                s3_region: None,
                s3_prefix: "wreladb/checkpoints".to_string(),
                s3_endpoint: None,
                s3_path_style: false,
                s3_bucket_by_region: BTreeMap::new(),
                s3_region_by_region: BTreeMap::new(),
                s3_endpoint_by_region: BTreeMap::new(),
            },
            checkpoint_allowed_regions: vec!["ord".to_string()],
            sovereignty_id: "tenant-prod".to_string(),
            sovereignty_allowed_regions: vec!["ord".to_string()],
            sovereignty_enforce_all_copies: true,
            intent: self::policy::ResolvedIntentPolicy {
                policy_id: "orders-v3".to_string(),
                workload_class: "general_transactional".to_string(),
                latency_target_ms: 80,
                min_write_throughput_ops: 5000,
                residency_scope: vec!["eu-west".to_string(), "us-east".to_string()],
                namespace_policies: BTreeMap::from([("orders".to_string(), "warm".to_string())]),
                cost_guardrail_mode: "estimate_and_warn_only".to_string(),
            },
            residency_policy_json: None,
            shard_group_locality_json: Some("{\"group_a\":[\"ord\"]}".to_string()),
            mtls_mode: "off".to_string(),
        };

        let fly_toml = fly_toml_template("unit-test-app", &policy);
        assert!(fly_toml.contains("WRELADB_REGION = \"ord\""));
        assert!(fly_toml.contains("WRELADB_REPLICATION_ASYNC_FAILOVER = \"1\""));
        assert!(fly_toml.contains("WRELADB_SOVEREIGNTY_ID = \"tenant-prod\""));
        assert!(fly_toml.contains("WRELADB_SOVEREIGNTY_ALLOWED_REGIONS = \"ord\""));
        assert!(fly_toml.contains("WRELADB_SOVEREIGNTY_ENFORCE_ALL_COPIES = \"1\""));
        assert!(fly_toml.contains("WRELADB_TOPOLOGY_MODE = \"collapsed\""));
        assert!(fly_toml
            .contains("WRELADB_TOPOLOGY_REGION_NODE_MAP_JSON = \"{\\\"ord\\\":[\\\"n1\\\",\\\"n2\\\",\\\"n3\\\"]}\""));
        assert!(fly_toml.contains(
            "WRELADB_TOPOLOGY_REGION_AZ_NODE_MAP_JSON = \"{\\\"ord\\\":{\\\"ord\\\":[\\\"n1\\\",\\\"n2\\\",\\\"n3\\\"]}}\""
        ));
        assert!(fly_toml.contains("\\\"cost_guardrail_mode\\\":\\\"estimate_and_warn_only\\\""));
        assert!(!fly_toml.contains("\\\"cost_mode\\\""));

        let parsed_toml: toml::Value = toml::from_str(&fly_toml).expect("parse fly.toml");
        let intent_json = parsed_toml
            .get("env")
            .and_then(|env| env.get("WRELADB_INTENT_JSON"))
            .and_then(|value| value.as_str())
            .expect("WRELADB_INTENT_JSON env");
        let parsed_intent = DbIntentConfig::from_json_str(intent_json).expect("parse db intent");

        assert_eq!(parsed_intent.policy_id, "orders-v3");
        assert_eq!(
            parsed_intent.workload_class,
            DbWorkloadClass::GeneralTransactional
        );
        assert_eq!(parsed_intent.latency_target_ms, 80);
        assert_eq!(parsed_intent.min_write_throughput_ops, 5000);
        assert_eq!(
            parsed_intent.residency_scope,
            vec!["eu-west".to_string(), "us-east".to_string()]
        );
        assert_eq!(
            parsed_intent.namespace_policies,
            BTreeMap::from([("orders".to_string(), NamespacePolicy::Warm)])
        );
        assert_eq!(
            parsed_intent.cost_guardrail_mode,
            CostGuardrailMode::EstimateAndWarnOnly
        );
    }

    #[test]
    fn collapsed_mode_chain_resolve_emit_and_runtime_strict_parse() {
        let (resolved, env_map) = resolve_policy_and_emit_env(
            r#"
[topology]
mode = "collapsed"

[replication]
async_failover = true

[regions]
ord = 3

[topology.region_node_map]
ord = ["n1", "n2", "n3"]

[sovereignty]
id = "tenant-prod"
allowed_regions = ["ord"]
enforce_all_copies = true
"#,
            "ord",
        );
        assert_eq!(resolved.topology_mode, TopologyMode::Collapsed);
        assert_eq!(
            env_map.get("WRELADB_TOPOLOGY_MODE").map(String::as_str),
            Some("collapsed")
        );

        let _lock = ENV_LOCK.lock().expect("env lock");
        let _guard = EnvGuard::set(&env_map);
        let cfg = DbConfig::from_env_strict().expect("strict parse from emitted env");
        assert_eq!(cfg.topology.local_region, "ord");
        assert_eq!(cfg.topology.region_az_node_map, resolved.region_az_node_map);
    }

    #[test]
    fn explicit_mode_chain_resolve_emit_and_runtime_strict_parse() {
        let (resolved, env_map) = resolve_policy_and_emit_env(
            r#"
[topology]
mode = "explicit"

[replication]
async_failover = true

[regions]
ord = 3

[topology.region_az_node_map.ord]
ord-a = ["n1", "n2"]
ord-b = ["n3"]

[sovereignty]
id = "tenant-prod"
allowed_regions = ["ord"]
enforce_all_copies = true
"#,
            "ord",
        );
        assert_eq!(resolved.topology_mode, TopologyMode::Explicit);
        assert_eq!(
            env_map.get("WRELADB_TOPOLOGY_MODE").map(String::as_str),
            Some("explicit")
        );

        let _lock = ENV_LOCK.lock().expect("env lock");
        let _guard = EnvGuard::set(&env_map);
        let cfg = DbConfig::from_env_strict().expect("strict parse from emitted env");
        assert_eq!(cfg.topology.local_region, "ord");
        assert_eq!(cfg.topology.region_az_node_map, resolved.region_az_node_map);
    }

    #[test]
    fn single_domain_mode_chain_resolve_emit_and_runtime_strict_parse() {
        let (resolved, env_map) = resolve_policy_and_emit_env(
            r#"
[topology]
mode = "single_domain"

[replication]
async_failover = true

[regions]
domain-a = 3

[topology.single_domain]
id = "domain-a"
nodes = ["n1", "n2", "n3"]

[sovereignty]
id = "domain-a"
allowed_regions = ["domain-a"]
enforce_all_copies = true
"#,
            "domain-a",
        );
        assert_eq!(resolved.topology_mode, TopologyMode::SingleDomain);
        assert_eq!(
            env_map.get("WRELADB_TOPOLOGY_MODE").map(String::as_str),
            Some("single_domain")
        );

        let _lock = ENV_LOCK.lock().expect("env lock");
        let _guard = EnvGuard::set(&env_map);
        let cfg = DbConfig::from_env_strict().expect("strict parse from emitted env");
        assert_eq!(cfg.topology.local_region, "domain-a");
        assert_eq!(cfg.sovereignty.id, "domain-a");
        assert_eq!(cfg.topology.region_az_node_map, resolved.region_az_node_map);
    }

    #[test]
    fn topology_env_tamper_is_fail_closed_across_all_modes() {
        let scenarios = [
            (
                "collapsed",
                r#"
[topology]
mode = "collapsed"

[replication]
async_failover = true

[regions]
ord = 3

[topology.region_node_map]
ord = ["n1", "n2", "n3"]

[sovereignty]
id = "tenant-prod"
allowed_regions = ["ord"]
enforce_all_copies = true
"#,
                "ord",
            ),
            (
                "explicit",
                r#"
[topology]
mode = "explicit"

[replication]
async_failover = true

[regions]
ord = 3

[topology.region_az_node_map.ord]
ord-a = ["n1", "n2"]
ord-b = ["n3"]

[sovereignty]
id = "tenant-prod"
allowed_regions = ["ord"]
enforce_all_copies = true
"#,
                "ord",
            ),
            (
                "single_domain",
                r#"
[topology]
mode = "single_domain"

[replication]
async_failover = true

[regions]
domain-a = 3

[topology.single_domain]
id = "domain-a"
nodes = ["n1", "n2", "n3"]

[sovereignty]
id = "domain-a"
allowed_regions = ["domain-a"]
enforce_all_copies = true
"#,
                "domain-a",
            ),
        ];

        let _lock = ENV_LOCK.lock().expect("env lock");
        for (mode, topology_block, default_region) in scenarios {
            let (_, mut env_map) = resolve_policy_and_emit_env(topology_block, default_region);
            env_map.insert(
                "WRELADB_TOPOLOGY_REGION_NODE_MAP_JSON".to_string(),
                "{}".to_string(),
            );
            let _guard = EnvGuard::set(&env_map);
            let err =
                DbConfig::from_env_strict().expect_err("tampered topology env should fail closed");
            assert!(
                err.contains("WRELADB_TOPOLOGY_REGION_NODE_MAP_JSON")
                    || err.contains("WRELADB_TOPOLOGY_REGION_AZ_NODE_MAP_JSON"),
                "mode {mode} should fail on topology payload mismatch, got: {err}"
            );
        }
    }
}
