use crate::db::autopilot::compiler::{DbIntentConfig, DbIntentTopologyHints, compile_db_intent};
use crate::db::checkpoint::CheckpointConfig;
use crate::db::security::residency::ResidencyPolicy;
use crate::db::{CommitVisibilityMode, QuorumTransportMode, ReplicatedLogBackend};
use serde_json::Value;
use std::collections::BTreeMap;

const TOPOLOGY_MODE_ENV: &str = "WRELADB_TOPOLOGY_MODE";
const TOPOLOGY_REGION_AZ_NODE_MAP_ENV: &str = "WRELADB_TOPOLOGY_REGION_AZ_NODE_MAP_JSON";
const TOPOLOGY_REGION_NODE_MAP_ENV: &str = "WRELADB_TOPOLOGY_REGION_NODE_MAP_JSON";
const INTENT_JSON_ENV: &str = "WRELADB_INTENT_JSON";

/// Central typed configuration for WrelaDB. No env var reads -- production
/// defaults are compile-time constants; strict deploy cutover can opt into
/// env parsing via `DbConfig::from_env_strict()`.
#[derive(Debug, Clone)]
pub struct DbConfig {
    pub replication: ReplicationConfig,
    pub topology: TopologyConfig,
    pub sovereignty: SovereigntyConfig,
    pub intent: DbIntentConfig,
    pub checkpoint: CheckpointConfig,
    pub engine: EngineConfig,
    pub rpc: RpcConfig,
    pub restore_latest_checkpoint_on_open: bool,
    pub residency_policy: Option<ResidencyPolicy>,
}

#[derive(Debug, Clone)]
pub struct ReplicationConfig {
    pub factor: u32,
    pub write_quorum: u32,
    pub async_failover: bool,
    pub commit_visibility_mode: CommitVisibilityMode,
    pub log_backend: ReplicatedLogBackend,
    pub quorum_transport_mode: QuorumTransportMode,
}

#[derive(Debug, Clone)]
pub struct TopologyConfig {
    pub initial_logical_shards: u32,
    pub initial_active_groups: u32,
    pub autoscale_enabled: bool,
    pub autoscale_tick_ms: u64,
    pub autoscale_max_skew_ratio: f64,
    pub autoscale_target_shards_per_group: u32,
    pub autoscale_max_active_groups: u32,
    pub autoscale_max_logical_shards: u32,
    pub local_region: String,
    pub region_az_node_map: BTreeMap<String, BTreeMap<String, Vec<String>>>,
    pub residency_policy: Option<ResidencyPolicy>,
}

#[derive(Debug, Clone)]
pub struct SovereigntyConfig {
    pub id: String,
    pub allowed_regions: Vec<String>,
    pub enforce_all_copies: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct EngineConfig {
    pub writer_lane_count: usize,
    pub block_cache_capacity: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct RpcConfig {
    pub channels_per_target: usize,
    pub io_timeout_ms: u64,
}

// ---------------------------------------------------------------------------
// Default impls -- production defaults, no env var reads, no cfg!(test)
// ---------------------------------------------------------------------------

impl Default for ReplicationConfig {
    fn default() -> Self {
        Self {
            factor: 3,
            write_quorum: 2,
            async_failover: false,
            commit_visibility_mode: CommitVisibilityMode::AsyncApply,
            log_backend: ReplicatedLogBackend::CanonicalOnly,
            quorum_transport_mode: QuorumTransportMode::RequirePrivateRpc,
        }
    }
}

impl Default for TopologyConfig {
    fn default() -> Self {
        let default_logical_shards = std::thread::available_parallelism()
            .map(|p| (p.get() as u32).min(16))
            .unwrap_or(8);
        Self {
            initial_logical_shards: default_logical_shards,
            initial_active_groups: 1,
            autoscale_enabled: false,
            autoscale_tick_ms: 2_000,
            autoscale_max_skew_ratio: 1.5,
            autoscale_target_shards_per_group: 4,
            autoscale_max_active_groups: 64,
            autoscale_max_logical_shards: 4096,
            local_region: "local".to_string(),
            region_az_node_map: BTreeMap::new(),
            residency_policy: None,
        }
    }
}

impl Default for SovereigntyConfig {
    fn default() -> Self {
        Self {
            id: "default".to_string(),
            allowed_regions: vec!["local".to_string()],
            enforce_all_copies: true,
        }
    }
}

impl Default for EngineConfig {
    fn default() -> Self {
        let n = std::thread::available_parallelism()
            .map(|p| p.get())
            .unwrap_or(1);
        Self {
            writer_lane_count: (n / 2).min(4).max(1),
            block_cache_capacity: 1024,
        }
    }
}

impl Default for RpcConfig {
    fn default() -> Self {
        let n = std::thread::available_parallelism()
            .map(|p| p.get())
            .unwrap_or(2);
        Self {
            channels_per_target: (n / 2).min(4).max(2),
            io_timeout_ms: 2000,
        }
    }
}

impl Default for DbConfig {
    fn default() -> Self {
        Self {
            replication: ReplicationConfig::default(),
            topology: TopologyConfig::default(),
            sovereignty: SovereigntyConfig::default(),
            intent: DbIntentConfig::default(),
            checkpoint: CheckpointConfig::default(),
            engine: EngineConfig::default(),
            rpc: RpcConfig::default(),
            restore_latest_checkpoint_on_open: true,
            residency_policy: None,
        }
    }
}

impl DbConfig {
    /// Test configuration: single-node, 1 shard, 1 lane, StrictApply, PreferPrivateRpc.
    pub fn for_testing() -> Self {
        Self {
            replication: ReplicationConfig {
                factor: 1,
                write_quorum: 1,
                async_failover: false,
                commit_visibility_mode: CommitVisibilityMode::StrictApply,
                log_backend: ReplicatedLogBackend::CanonicalOnly,
                quorum_transport_mode: QuorumTransportMode::PreferPrivateRpc,
            },
            topology: TopologyConfig {
                initial_logical_shards: 1,
                initial_active_groups: 1,
                autoscale_enabled: false,
                local_region: "local".to_string(),
                region_az_node_map: BTreeMap::from([(
                    "local".to_string(),
                    BTreeMap::from([("az1".to_string(), vec!["node-1".to_string()])]),
                )]),
                ..TopologyConfig::default()
            },
            sovereignty: SovereigntyConfig {
                id: "test-local".to_string(),
                allowed_regions: vec!["local".to_string()],
                enforce_all_copies: true,
            },
            intent: DbIntentConfig {
                policy_id: "test-local-intent".to_string(),
                latency_target_ms: 5,
                min_write_throughput_ops: 500,
                residency_scope: vec!["local".to_string()],
                ..DbIntentConfig::default()
            },
            engine: EngineConfig {
                writer_lane_count: 1,
                block_cache_capacity: 1024,
            },
            checkpoint: CheckpointConfig::default(),
            rpc: RpcConfig::default(),
            restore_latest_checkpoint_on_open: true,
            residency_policy: None,
        }
    }

    pub fn from_env_strict() -> Result<Self, String> {
        let strict = StrictDeployEnv::parse_from_env()?;
        let mut checkpoint = CheckpointConfig::from_env();
        checkpoint.allowed_regions = strict.checkpoint_allowed_regions.clone();
        let residency_policy = strict.residency_policy.clone();

        let cfg = Self {
            replication: ReplicationConfig {
                factor: strict.replication_factor,
                write_quorum: strict.write_quorum,
                async_failover: strict.replication_async_failover,
                ..ReplicationConfig::default()
            },
            topology: TopologyConfig {
                initial_logical_shards: strict.logical_shards,
                initial_active_groups: strict.active_groups,
                local_region: strict.region.clone(),
                region_az_node_map: strict.region_az_node_map.clone(),
                residency_policy: residency_policy.clone(),
                ..TopologyConfig::default()
            },
            sovereignty: SovereigntyConfig {
                id: strict.sovereignty_id.clone(),
                allowed_regions: strict.sovereignty_allowed_regions.clone(),
                enforce_all_copies: strict.sovereignty_enforce_all_copies,
            },
            intent: strict.intent.clone(),
            checkpoint,
            engine: EngineConfig::default(),
            rpc: RpcConfig::default(),
            restore_latest_checkpoint_on_open: true,
            residency_policy,
        };
        validate_strict_with_snapshot(&cfg, &strict)?;
        cfg.validate_strict()?;
        Ok(cfg)
    }

    pub fn validate_strict(&self) -> Result<(), String> {
        let strict = StrictDeployEnv::parse_from_env()?;
        validate_strict_with_snapshot(self, &strict)
    }

    pub fn with_replication(mut self, replication: ReplicationConfig) -> Self {
        self.replication = replication;
        self
    }

    pub fn with_topology(mut self, topology: TopologyConfig) -> Self {
        self.topology = topology;
        self
    }

    pub fn with_checkpoint(mut self, checkpoint: CheckpointConfig) -> Self {
        self.checkpoint = checkpoint;
        self
    }

    pub fn with_engine(mut self, engine: EngineConfig) -> Self {
        self.engine = engine;
        self
    }

    pub fn with_rpc(mut self, rpc: RpcConfig) -> Self {
        self.rpc = rpc;
        self
    }
}

#[derive(Debug, Clone)]
struct StrictDeployEnv {
    replication_factor: u32,
    write_quorum: u32,
    logical_shards: u32,
    active_groups: u32,
    target_voters: u32,
    region_machine_map: BTreeMap<String, usize>,
    checkpoint_allowed_regions: Vec<String>,
    sovereignty_id: String,
    sovereignty_allowed_regions: Vec<String>,
    sovereignty_enforce_all_copies: bool,
    intent: DbIntentConfig,
    topology_mode: TopologyMode,
    region_node_map: BTreeMap<String, Vec<String>>,
    region_az_node_map: BTreeMap<String, BTreeMap<String, Vec<String>>>,
    replication_async_failover: bool,
    shard_group_locality_json: Option<Value>,
    region: String,
    residency_policy: Option<ResidencyPolicy>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TopologyMode {
    Collapsed,
    Explicit,
    SingleDomain,
}

impl StrictDeployEnv {
    fn parse_from_env() -> Result<Self, String> {
        let replication_factor = parse_required_u32("WRELADB_REPLICATION_FACTOR")?;
        let write_quorum = parse_required_u32("WRELADB_WRITE_QUORUM")?;
        let logical_shards = parse_required_u32("WRELADB_LOGICAL_SHARDS")?;
        let active_groups = parse_required_u32("WRELADB_ACTIVE_GROUPS")?;
        let target_voters = parse_required_u32("WRELADB_TARGET_VOTERS")?;
        let region_machine_map = parse_region_machine_map_json("WRELADB_REGION_MACHINE_MAP_JSON")?;
        let available_nodes =
            saturating_u32_from_usize(region_machine_map.values().copied().sum::<usize>());
        let intent =
            parse_required_db_intent_json(INTENT_JSON_ENV, available_nodes, logical_shards)?;
        let topology_mode = parse_topology_mode(TOPOLOGY_MODE_ENV)?;
        let checkpoint_allowed_regions =
            parse_required_region_csv("WRELADB_CHECKPOINT_ALLOWED_REGIONS")?;
        let sovereignty_id = parse_required_string("WRELADB_SOVEREIGNTY_ID")?;
        let (region_node_map, region_az_node_map) =
            parse_topology_maps(topology_mode, &sovereignty_id)?;
        let sovereignty_allowed_regions =
            parse_required_region_csv("WRELADB_SOVEREIGNTY_ALLOWED_REGIONS")?;
        let sovereignty_enforce_all_copies =
            parse_required_bool("WRELADB_SOVEREIGNTY_ENFORCE_ALL_COPIES")?;
        let replication_async_failover = parse_required_bool("WRELADB_REPLICATION_ASYNC_FAILOVER")?;
        let shard_group_locality_json =
            parse_optional_json_object("WRELADB_SHARD_GROUP_LOCALITY_JSON")?;
        let region = normalize_region(&parse_required_string("WRELADB_REGION")?)
            .ok_or_else(|| "WRELADB_REGION must be non-empty".to_string())?;
        let residency_policy = ResidencyPolicy::from_env()?;

        Ok(Self {
            replication_factor,
            write_quorum,
            logical_shards,
            active_groups,
            target_voters,
            region_machine_map,
            checkpoint_allowed_regions,
            sovereignty_id,
            sovereignty_allowed_regions,
            sovereignty_enforce_all_copies,
            intent,
            topology_mode,
            region_node_map,
            region_az_node_map,
            replication_async_failover,
            shard_group_locality_json,
            region,
            residency_policy,
        })
    }
}

fn parse_topology_mode(name: &'static str) -> Result<TopologyMode, String> {
    let raw = parse_required_string(name)?;
    let normalized = raw.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "collapsed" => Ok(TopologyMode::Collapsed),
        "explicit" => Ok(TopologyMode::Explicit),
        "single_domain" => Ok(TopologyMode::SingleDomain),
        _ => Err(format!(
            "{name} must be one of collapsed|explicit|single_domain (got `{raw}`)"
        )),
    }
}

fn parse_topology_maps(
    topology_mode: TopologyMode,
    sovereignty_id: &str,
) -> Result<
    (
        BTreeMap<String, Vec<String>>,
        BTreeMap<String, BTreeMap<String, Vec<String>>>,
    ),
    String,
> {
    let region_node_map = if is_env_var_set(TOPOLOGY_REGION_NODE_MAP_ENV) {
        Some(parse_region_node_map_json(TOPOLOGY_REGION_NODE_MAP_ENV)?)
    } else {
        None
    };
    let region_az_node_map = if is_env_var_set(TOPOLOGY_REGION_AZ_NODE_MAP_ENV) {
        Some(parse_region_az_node_map_json(
            TOPOLOGY_REGION_AZ_NODE_MAP_ENV,
        )?)
    } else {
        None
    };

    match topology_mode {
        TopologyMode::Collapsed => {
            let region_node_map = region_node_map.ok_or_else(|| {
                format!(
                    "{TOPOLOGY_REGION_NODE_MAP_ENV} is required when {TOPOLOGY_MODE_ENV}=collapsed"
                )
            })?;
            let region_az_node_map = region_az_node_map.ok_or_else(|| {
                format!(
                    "{TOPOLOGY_REGION_AZ_NODE_MAP_ENV} is required when {TOPOLOGY_MODE_ENV}=collapsed"
                )
            })?;
            validate_collapsed_topology_maps(&region_node_map, &region_az_node_map)?;
            Ok((region_node_map, region_az_node_map))
        }
        TopologyMode::Explicit => {
            let region_az_node_map = region_az_node_map.ok_or_else(|| {
                format!(
                    "{TOPOLOGY_REGION_AZ_NODE_MAP_ENV} is required when {TOPOLOGY_MODE_ENV}=explicit"
                )
            })?;
            let derived_region_node_map =
                canonical_region_node_map_from_region_az_map(&region_az_node_map);
            if let Some(provided_region_node_map) = region_node_map.as_ref()
                && *provided_region_node_map != derived_region_node_map
            {
                return Err(format!(
                    "{TOPOLOGY_REGION_NODE_MAP_ENV} must equal the union of nodes from {TOPOLOGY_REGION_AZ_NODE_MAP_ENV} when {TOPOLOGY_MODE_ENV}=explicit"
                ));
            }
            Ok((derived_region_node_map, region_az_node_map))
        }
        TopologyMode::SingleDomain => {
            let region_node_map = region_node_map.ok_or_else(|| {
                format!(
                    "{TOPOLOGY_REGION_NODE_MAP_ENV} is required when {TOPOLOGY_MODE_ENV}=single_domain"
                )
            })?;
            let region_az_node_map = region_az_node_map.ok_or_else(|| {
                format!(
                    "{TOPOLOGY_REGION_AZ_NODE_MAP_ENV} is required when {TOPOLOGY_MODE_ENV}=single_domain"
                )
            })?;
            validate_single_domain_topology_maps(
                sovereignty_id,
                &region_node_map,
                &region_az_node_map,
            )?;
            Ok((region_node_map, region_az_node_map))
        }
    }
}

fn is_env_var_set(name: &str) -> bool {
    std::env::var_os(name).is_some()
}

fn parse_required_u32(name: &'static str) -> Result<u32, String> {
    let raw = parse_required_string(name)?;
    let value = raw
        .parse::<u32>()
        .map_err(|err| format!("{name} must be a positive integer: {err}"))?;
    if value == 0 {
        return Err(format!("{name} must be > 0"));
    }
    Ok(value)
}

fn saturating_u32_from_usize(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

fn parse_required_string(name: &'static str) -> Result<String, String> {
    std::env::var(name)
        .ok()
        .and_then(|value| trim_non_empty(&value))
        .ok_or_else(|| format!("{name} is required"))
}

fn parse_required_bool(name: &'static str) -> Result<bool, String> {
    let raw = parse_required_string(name)?;
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        other => Err(format!(
            "{name} must be one of 1|0|true|false|yes|no|on|off (got `{other}`)"
        )),
    }
}

fn parse_required_db_intent_json(
    name: &'static str,
    available_nodes: u32,
    logical_shards: u32,
) -> Result<DbIntentConfig, String> {
    let raw = parse_required_string(name)?;
    let intent = DbIntentConfig::from_json_str(&raw)
        .map_err(|err| format!("{name} invalid intent: {err}"))?;
    compile_db_intent(
        &intent,
        DbIntentTopologyHints {
            available_nodes,
            logical_shards,
        },
    )
    .map_err(|err| format!("{name} invalid intent: {err:?}"))?;
    Ok(intent)
}

fn parse_required_region_csv(name: &'static str) -> Result<Vec<String>, String> {
    let raw = parse_required_string(name)?;
    let mut out = raw
        .split(',')
        .filter_map(normalize_region)
        .collect::<Vec<_>>();
    out.sort();
    out.dedup();
    if out.is_empty() {
        return Err(format!("{name} must contain at least one region"));
    }
    Ok(out)
}

fn parse_region_machine_map_json(name: &'static str) -> Result<BTreeMap<String, usize>, String> {
    let raw = parse_required_string(name)?;
    let parsed: Value =
        serde_json::from_str(&raw).map_err(|err| format!("{name} must be valid json: {err}"))?;
    let object = parsed
        .as_object()
        .ok_or_else(|| format!("{name} must be a json object"))?;
    if object.is_empty() {
        return Err(format!("{name} must include at least one region"));
    }
    let mut out = BTreeMap::new();
    for (region, count) in object {
        let normalized_region =
            normalize_region(region).ok_or_else(|| format!("{name} contains empty region key"))?;
        let Some(count) = count.as_u64() else {
            return Err(format!(
                "{name} entry for region `{region}` must be a positive integer"
            ));
        };
        if count == 0 {
            return Err(format!(
                "{name} entry for region `{region}` must be at least 1"
            ));
        }
        out.insert(normalized_region, count as usize);
    }
    Ok(out)
}

fn parse_region_az_node_map_json(
    name: &'static str,
) -> Result<BTreeMap<String, BTreeMap<String, Vec<String>>>, String> {
    let raw = parse_required_string(name)?;
    let parsed: Value =
        serde_json::from_str(&raw).map_err(|err| format!("{name} must be valid json: {err}"))?;
    let regions = parsed
        .as_object()
        .ok_or_else(|| format!("{name} must be a json object"))?;
    if regions.is_empty() {
        return Err(format!("{name} must include at least one region"));
    }
    let mut out = BTreeMap::new();
    for (region, az_value) in regions {
        let normalized_region =
            normalize_region(region).ok_or_else(|| format!("{name} contains empty region key"))?;
        let az_object = az_value
            .as_object()
            .ok_or_else(|| format!("{name} region `{region}` must map to an object"))?;
        if az_object.is_empty() {
            return Err(format!(
                "{name} region `{region}` must include at least one AZ"
            ));
        }
        let mut az_map = BTreeMap::new();
        for (az, nodes_value) in az_object {
            let normalized_az = trim_non_empty(az)
                .map(|value| value.to_ascii_lowercase())
                .ok_or_else(|| format!("{name} region `{region}` includes empty AZ key"))?;
            let nodes_array = nodes_value.as_array().ok_or_else(|| {
                format!("{name} region `{region}` az `{az}` must be an array of node ids")
            })?;
            let mut nodes = nodes_array
                .iter()
                .map(|node| {
                    node.as_str().and_then(trim_non_empty).ok_or_else(|| {
                        format!("{name} region `{region}` az `{az}` includes invalid node id")
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            nodes.sort();
            nodes.dedup();
            if nodes.is_empty() {
                return Err(format!(
                    "{name} region `{region}` az `{az}` must include at least one node id"
                ));
            }
            az_map.insert(normalized_az, nodes);
        }
        out.insert(normalized_region, az_map);
    }
    Ok(out)
}

fn parse_region_node_map_json(name: &'static str) -> Result<BTreeMap<String, Vec<String>>, String> {
    let raw = parse_required_string(name)?;
    let parsed: Value =
        serde_json::from_str(&raw).map_err(|err| format!("{name} must be valid json: {err}"))?;
    let regions = parsed
        .as_object()
        .ok_or_else(|| format!("{name} must be a json object"))?;
    if regions.is_empty() {
        return Err(format!("{name} must include at least one region"));
    }
    let mut out = BTreeMap::new();
    for (region, nodes_value) in regions {
        let normalized_region =
            normalize_region(region).ok_or_else(|| format!("{name} contains empty region key"))?;
        let nodes_array = nodes_value
            .as_array()
            .ok_or_else(|| format!("{name} region `{region}` must map to an array of node ids"))?;
        let mut nodes = nodes_array
            .iter()
            .map(|node| {
                node.as_str()
                    .and_then(trim_non_empty)
                    .ok_or_else(|| format!("{name} region `{region}` includes invalid node id"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        nodes.sort();
        nodes.dedup();
        if nodes.is_empty() {
            return Err(format!(
                "{name} region `{region}` must include at least one node id"
            ));
        }
        out.insert(normalized_region, nodes);
    }
    Ok(out)
}

fn validate_collapsed_topology_maps(
    region_node_map: &BTreeMap<String, Vec<String>>,
    region_az_node_map: &BTreeMap<String, BTreeMap<String, Vec<String>>>,
) -> Result<(), String> {
    for (region, region_nodes) in region_node_map {
        let Some(az_map) = region_az_node_map.get(region) else {
            return Err(format!(
                "{TOPOLOGY_REGION_AZ_NODE_MAP_ENV} missing region `{region}` required by {TOPOLOGY_REGION_NODE_MAP_ENV} when {TOPOLOGY_MODE_ENV}=collapsed"
            ));
        };
        if az_map.len() != 1 {
            return Err(format!(
                "{TOPOLOGY_REGION_AZ_NODE_MAP_ENV} region `{region}` must contain exactly one AZ equal to region id when {TOPOLOGY_MODE_ENV}=collapsed"
            ));
        }
        let Some(az_nodes) = az_map.get(region) else {
            return Err(format!(
                "{TOPOLOGY_REGION_AZ_NODE_MAP_ENV} region `{region}` must use az id `{region}` when {TOPOLOGY_MODE_ENV}=collapsed"
            ));
        };
        if az_nodes != region_nodes {
            return Err(format!(
                "node parity mismatch for region `{region}` between {TOPOLOGY_REGION_NODE_MAP_ENV} and {TOPOLOGY_REGION_AZ_NODE_MAP_ENV} when {TOPOLOGY_MODE_ENV}=collapsed"
            ));
        }
    }
    for region in region_az_node_map.keys() {
        if !region_node_map.contains_key(region) {
            return Err(format!(
                "{TOPOLOGY_REGION_AZ_NODE_MAP_ENV} contains region `{region}` not present in {TOPOLOGY_REGION_NODE_MAP_ENV} when {TOPOLOGY_MODE_ENV}=collapsed"
            ));
        }
    }
    Ok(())
}

fn validate_single_domain_topology_maps(
    sovereignty_id: &str,
    region_node_map: &BTreeMap<String, Vec<String>>,
    region_az_node_map: &BTreeMap<String, BTreeMap<String, Vec<String>>>,
) -> Result<(), String> {
    let sovereignty_domain = trim_non_empty(sovereignty_id)
        .map(|value| value.to_ascii_lowercase())
        .ok_or_else(|| "WRELADB_SOVEREIGNTY_ID must be non-empty".to_string())?;
    if region_node_map.len() != 1 || region_az_node_map.len() != 1 {
        return Err(format!(
            "{TOPOLOGY_REGION_NODE_MAP_ENV} and {TOPOLOGY_REGION_AZ_NODE_MAP_ENV} must describe exactly one domain when {TOPOLOGY_MODE_ENV}=single_domain"
        ));
    }
    let (region_id, region_nodes) = region_node_map
        .iter()
        .next()
        .expect("single_domain requires one region entry");
    if region_id != &sovereignty_domain {
        return Err(format!(
            "single_domain requires sovereignty id `{}` to match region id `{}` in {TOPOLOGY_REGION_NODE_MAP_ENV}",
            sovereignty_domain, region_id
        ));
    }
    let az_map = region_az_node_map.get(region_id).ok_or_else(|| {
        format!("{TOPOLOGY_REGION_AZ_NODE_MAP_ENV} missing single_domain region `{region_id}`")
    })?;
    if az_map.len() != 1 {
        return Err(format!(
            "{TOPOLOGY_REGION_AZ_NODE_MAP_ENV} region `{region_id}` must contain exactly one AZ when {TOPOLOGY_MODE_ENV}=single_domain"
        ));
    }
    let Some(az_nodes) = az_map.get(region_id) else {
        return Err(format!(
            "{TOPOLOGY_REGION_AZ_NODE_MAP_ENV} requires AZ id `{region_id}` to match sovereignty/region id when {TOPOLOGY_MODE_ENV}=single_domain"
        ));
    };
    if az_nodes != region_nodes {
        return Err(format!(
            "single_domain requires node parity between {TOPOLOGY_REGION_NODE_MAP_ENV} and {TOPOLOGY_REGION_AZ_NODE_MAP_ENV} for domain `{region_id}`"
        ));
    }
    Ok(())
}

fn validate_explicit_topology_maps(
    region_node_map: &BTreeMap<String, Vec<String>>,
    region_az_node_map: &BTreeMap<String, BTreeMap<String, Vec<String>>>,
) -> Result<(), String> {
    let canonical = canonical_region_node_map_from_region_az_map(region_az_node_map);
    if *region_node_map != canonical {
        return Err(format!(
            "{TOPOLOGY_REGION_NODE_MAP_ENV} must equal the union of nodes from {TOPOLOGY_REGION_AZ_NODE_MAP_ENV} when {TOPOLOGY_MODE_ENV}=explicit"
        ));
    }
    Ok(())
}

fn canonical_region_node_map_from_region_az_map(
    region_az_node_map: &BTreeMap<String, BTreeMap<String, Vec<String>>>,
) -> BTreeMap<String, Vec<String>> {
    let mut canonical = BTreeMap::new();
    for (region, az_map) in region_az_node_map {
        let mut nodes = az_map.values().flatten().cloned().collect::<Vec<_>>();
        nodes.sort();
        nodes.dedup();
        if !nodes.is_empty() {
            canonical.insert(region.clone(), nodes);
        }
    }
    canonical
}

fn parse_optional_json_object(name: &'static str) -> Result<Option<Value>, String> {
    let Some(raw) = std::env::var(name).ok() else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let parsed: Value =
        serde_json::from_str(trimmed).map_err(|err| format!("{name} must be valid json: {err}"))?;
    if !parsed.is_object() {
        return Err(format!("{name} must be a json object"));
    }
    Ok(Some(parsed))
}

fn normalize_region(value: &str) -> Option<String> {
    trim_non_empty(value).map(|region| region.to_ascii_lowercase())
}

fn trim_non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn validate_strict_with_snapshot(cfg: &DbConfig, strict: &StrictDeployEnv) -> Result<(), String> {
    if cfg.replication.factor != strict.replication_factor {
        return Err(format!(
            "replication factor mismatch config={} env={}",
            cfg.replication.factor, strict.replication_factor
        ));
    }
    if cfg.replication.write_quorum != strict.write_quorum {
        return Err(format!(
            "write quorum mismatch config={} env={}",
            cfg.replication.write_quorum, strict.write_quorum
        ));
    }
    if cfg.topology.initial_logical_shards != strict.logical_shards {
        return Err(format!(
            "logical shards mismatch config={} env={}",
            cfg.topology.initial_logical_shards, strict.logical_shards
        ));
    }
    if cfg.topology.initial_active_groups != strict.active_groups {
        return Err(format!(
            "active groups mismatch config={} env={}",
            cfg.topology.initial_active_groups, strict.active_groups
        ));
    }
    if cfg.topology.local_region != strict.region {
        return Err(format!(
            "local region mismatch config={} env={}",
            cfg.topology.local_region, strict.region
        ));
    }
    if cfg.sovereignty.id != strict.sovereignty_id {
        return Err(format!(
            "sovereignty id mismatch config={} env={}",
            cfg.sovereignty.id, strict.sovereignty_id
        ));
    }
    if cfg.sovereignty.allowed_regions != strict.sovereignty_allowed_regions {
        return Err(format!(
            "sovereignty allowed regions mismatch config={:?} env={:?}",
            cfg.sovereignty.allowed_regions, strict.sovereignty_allowed_regions
        ));
    }
    if cfg.sovereignty.enforce_all_copies != strict.sovereignty_enforce_all_copies {
        return Err(format!(
            "sovereignty enforce_all_copies mismatch config={} env={}",
            cfg.sovereignty.enforce_all_copies, strict.sovereignty_enforce_all_copies
        ));
    }
    if cfg.intent != strict.intent {
        return Err(format!(
            "intent mismatch config={:?} env={:?}",
            cfg.intent, strict.intent
        ));
    }
    if cfg.replication.async_failover != strict.replication_async_failover {
        return Err(format!(
            "replication async_failover mismatch config={} env={}",
            cfg.replication.async_failover, strict.replication_async_failover
        ));
    }
    if cfg.topology.region_az_node_map != strict.region_az_node_map {
        return Err(format!(
            "region_az_node_map mismatch config={:?} env={:?}",
            cfg.topology.region_az_node_map, strict.region_az_node_map
        ));
    }
    if strict.region_az_node_map.is_empty() {
        return Err(format!(
            "{TOPOLOGY_REGION_AZ_NODE_MAP_ENV} resolved canonical map must include at least one region"
        ));
    }
    if strict.region_node_map.is_empty() {
        return Err(format!(
            "{TOPOLOGY_REGION_NODE_MAP_ENV} resolved canonical map must include at least one region"
        ));
    }
    match strict.topology_mode {
        TopologyMode::Collapsed => {
            validate_collapsed_topology_maps(&strict.region_node_map, &strict.region_az_node_map)?
        }
        TopologyMode::Explicit => {
            validate_explicit_topology_maps(&strict.region_node_map, &strict.region_az_node_map)?
        }
        TopologyMode::SingleDomain => validate_single_domain_topology_maps(
            &strict.sovereignty_id,
            &strict.region_node_map,
            &strict.region_az_node_map,
        )?,
    }
    if strict.write_quorum > strict.replication_factor {
        return Err(format!(
            "write quorum {} cannot exceed replication factor {}",
            strict.write_quorum, strict.replication_factor
        ));
    }
    let majority = (strict.replication_factor / 2) + 1;
    if strict.write_quorum < majority {
        return Err(format!(
            "write quorum {} must be majority quorum for replication factor {} (min {})",
            strict.write_quorum, strict.replication_factor, majority
        ));
    }
    if strict.active_groups > strict.logical_shards {
        return Err(format!(
            "active groups {} cannot exceed logical shards {}",
            strict.active_groups, strict.logical_shards
        ));
    }
    let machine_count = strict.region_machine_map.values().copied().sum::<usize>();
    if machine_count == 0 {
        return Err("WRELADB_REGION_MACHINE_MAP_JSON resolves to zero machines".to_string());
    }
    if strict.target_voters > machine_count as u32 {
        return Err(format!(
            "target voters {} cannot exceed machine count {}",
            strict.target_voters, machine_count
        ));
    }
    if strict.replication_factor > strict.target_voters {
        return Err(format!(
            "replication factor {} cannot exceed target voters {}",
            strict.replication_factor, strict.target_voters
        ));
    }
    if !strict.region_machine_map.contains_key(&strict.region) {
        return Err(format!(
            "WRELADB_REGION `{}` is not present in WRELADB_REGION_MACHINE_MAP_JSON",
            strict.region
        ));
    }
    for region in strict.region_machine_map.keys() {
        let Some(az_map) = strict.region_az_node_map.get(region) else {
            return Err(format!(
                "WRELADB_TOPOLOGY_REGION_AZ_NODE_MAP_JSON missing region `{region}`"
            ));
        };
        let node_count = az_map.values().map(Vec::len).sum::<usize>();
        let expected = strict
            .region_machine_map
            .get(region)
            .copied()
            .unwrap_or_default();
        if node_count < expected {
            return Err(format!(
                "region `{region}` requires at least {expected} nodes in region_az_node_map (got {node_count})"
            ));
        }
    }
    for region in strict.region_az_node_map.keys() {
        if !strict.region_machine_map.contains_key(region) {
            return Err(format!(
                "WRELADB_TOPOLOGY_REGION_AZ_NODE_MAP_JSON contains unknown region `{region}`"
            ));
        }
    }
    for region in strict.region_node_map.keys() {
        if !strict.region_machine_map.contains_key(region) {
            return Err(format!(
                "WRELADB_TOPOLOGY_REGION_NODE_MAP_JSON contains unknown region `{region}`"
            ));
        }
    }
    for region in &strict.checkpoint_allowed_regions {
        if !strict.region_machine_map.contains_key(region) {
            return Err(format!(
                "checkpoint allowed region `{region}` missing from WRELADB_REGION_MACHINE_MAP_JSON"
            ));
        }
    }
    for region in &strict.sovereignty_allowed_regions {
        if !strict.region_machine_map.contains_key(region) {
            return Err(format!(
                "sovereignty allowed region `{region}` missing from WRELADB_REGION_MACHINE_MAP_JSON"
            ));
        }
    }
    if strict.sovereignty_enforce_all_copies
        && strict.sovereignty_allowed_regions != strict.checkpoint_allowed_regions
    {
        return Err(
            "WRELADB_SOVEREIGNTY_ENFORCE_ALL_COPIES=1 requires sovereignty/checkpoint allowed regions to match"
                .to_string(),
        );
    }
    if strict.sovereignty_id.trim().is_empty() {
        return Err("WRELADB_SOVEREIGNTY_ID must be non-empty".to_string());
    }
    if strict.shard_group_locality_json.is_some() && strict.region_az_node_map.is_empty() {
        return Err(
            "WRELADB_SHARD_GROUP_LOCALITY_JSON requires WRELADB_TOPOLOGY_REGION_AZ_NODE_MAP_JSON"
                .to_string(),
        );
    }
    if strict.replication_async_failover && strict.target_voters < 2 {
        return Err(
            "WRELADB_REPLICATION_ASYNC_FAILOVER=1 requires WRELADB_TARGET_VOTERS >= 2".to_string(),
        );
    }
    let mut checkpoint_allowed = cfg
        .checkpoint
        .allowed_regions
        .iter()
        .filter_map(|region| normalize_region(region))
        .collect::<Vec<_>>();
    checkpoint_allowed.sort();
    checkpoint_allowed.dedup();
    if checkpoint_allowed != strict.checkpoint_allowed_regions {
        return Err(format!(
            "checkpoint allowed regions mismatch config={:?} env={:?}",
            checkpoint_allowed, strict.checkpoint_allowed_regions
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    const STRICT_KEYS: [&str; 17] = [
        "WRELADB_REPLICATION_FACTOR",
        "WRELADB_WRITE_QUORUM",
        "WRELADB_LOGICAL_SHARDS",
        "WRELADB_ACTIVE_GROUPS",
        "WRELADB_TARGET_VOTERS",
        "WRELADB_REGION_MACHINE_MAP_JSON",
        "WRELADB_INTENT_JSON",
        "WRELADB_TOPOLOGY_MODE",
        "WRELADB_TOPOLOGY_REGION_AZ_NODE_MAP_JSON",
        "WRELADB_TOPOLOGY_REGION_NODE_MAP_JSON",
        "WRELADB_CHECKPOINT_ALLOWED_REGIONS",
        "WRELADB_SOVEREIGNTY_ID",
        "WRELADB_SOVEREIGNTY_ALLOWED_REGIONS",
        "WRELADB_SOVEREIGNTY_ENFORCE_ALL_COPIES",
        "WRELADB_REPLICATION_ASYNC_FAILOVER",
        "WRELADB_SHARD_GROUP_LOCALITY_JSON",
        "WRELADB_REGION",
    ];

    struct EnvGuard {
        snapshots: Vec<(&'static str, Option<String>)>,
    }

    impl EnvGuard {
        fn set(vars: &[(&'static str, Option<&str>)]) -> Self {
            let mut snapshots = Vec::with_capacity(STRICT_KEYS.len() + 1);
            for key in STRICT_KEYS {
                snapshots.push((key, std::env::var(key).ok()));
            }
            snapshots.push((
                "WRELADB_RESIDENCY_POLICY_JSON",
                std::env::var("WRELADB_RESIDENCY_POLICY_JSON").ok(),
            ));
            for (key, value) in vars {
                match value {
                    Some(value) => {
                        // SAFETY: guarded by ENV_LOCK for test-local env mutation.
                        unsafe { std::env::set_var(key, value) };
                    }
                    None => {
                        // SAFETY: guarded by ENV_LOCK for test-local env mutation.
                        unsafe { std::env::remove_var(key) };
                    }
                }
            }
            Self { snapshots }
        }
    }

    fn strict_base_env_vars() -> Vec<(&'static str, Option<&'static str>)> {
        vec![
            ("WRELADB_REPLICATION_FACTOR", Some("3")),
            ("WRELADB_WRITE_QUORUM", Some("2")),
            ("WRELADB_LOGICAL_SHARDS", Some("16")),
            ("WRELADB_ACTIVE_GROUPS", Some("3")),
            ("WRELADB_TARGET_VOTERS", Some("3")),
            ("WRELADB_REGION_MACHINE_MAP_JSON", Some("{\"ord\":3}")),
            (
                "WRELADB_INTENT_JSON",
                Some(
                    "{\"policy_id\":\"prod-intent\",\"workload_class\":\"general_transactional\",\"latency_target_ms\":25,\"min_write_throughput_ops\":5000,\"residency_scope\":[\"ord\"],\"namespace_policies\":{\"core\":\"warm\",\"meta\":\"hot_meta\"},\"cost_guardrail_mode\":\"estimate_and_warn_only\"}",
                ),
            ),
            ("WRELADB_TOPOLOGY_MODE", Some("collapsed")),
            (
                "WRELADB_TOPOLOGY_REGION_AZ_NODE_MAP_JSON",
                Some("{\"ord\":{\"ord\":[\"n1\",\"n2\",\"n3\"]}}"),
            ),
            (
                "WRELADB_TOPOLOGY_REGION_NODE_MAP_JSON",
                Some("{\"ord\":[\"n1\",\"n2\",\"n3\"]}"),
            ),
            ("WRELADB_CHECKPOINT_ALLOWED_REGIONS", Some("ord")),
            ("WRELADB_SOVEREIGNTY_ID", Some("tenant-prod")),
            ("WRELADB_SOVEREIGNTY_ALLOWED_REGIONS", Some("ord")),
            ("WRELADB_SOVEREIGNTY_ENFORCE_ALL_COPIES", Some("1")),
            ("WRELADB_REPLICATION_ASYNC_FAILOVER", Some("true")),
            (
                "WRELADB_SHARD_GROUP_LOCALITY_JSON",
                Some("{\"g0\":[\"ord\"]}"),
            ),
            ("WRELADB_REGION", Some("ord")),
            (
                "WRELADB_RESIDENCY_POLICY_JSON",
                Some(
                    "{\"rules\":[{\"shard\":\"core\",\"allowed_regions\":[\"ord\"]}],\"checkpoint_allowed_regions\":[\"ord\"]}",
                ),
            ),
        ]
    }

    fn put_var(
        vars: &mut Vec<(&'static str, Option<&'static str>)>,
        key: &'static str,
        value: Option<&'static str>,
    ) {
        if let Some(entry) = vars.iter_mut().find(|(name, _)| *name == key) {
            entry.1 = value;
        } else {
            vars.push((key, value));
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for (key, value) in &self.snapshots {
                match value {
                    Some(value) => {
                        // SAFETY: guarded by ENV_LOCK for test-local env mutation.
                        unsafe { std::env::set_var(key, value) };
                    }
                    None => {
                        // SAFETY: guarded by ENV_LOCK for test-local env mutation.
                        unsafe { std::env::remove_var(key) };
                    }
                }
            }
        }
    }

    #[test]
    fn from_env_strict_parses_deploy_env_vars_in_collapsed_mode() {
        let _lock = ENV_LOCK.lock().expect("env lock");
        let vars = strict_base_env_vars();
        let _guard = EnvGuard::set(&vars);

        let cfg = DbConfig::from_env_strict().expect("strict env parse");
        assert_eq!(cfg.replication.factor, 3);
        assert_eq!(cfg.replication.write_quorum, 2);
        assert_eq!(cfg.topology.initial_logical_shards, 16);
        assert_eq!(cfg.topology.initial_active_groups, 3);
        assert_eq!(cfg.topology.local_region, "ord");
        assert_eq!(cfg.checkpoint.allowed_regions, vec!["ord".to_string()]);
        cfg.validate_strict().expect("strict validate");
    }

    #[test]
    fn from_env_strict_parses_explicit_mode_with_consistent_coexisting_payloads() {
        let _lock = ENV_LOCK.lock().expect("env lock");
        let mut vars = strict_base_env_vars();
        put_var(&mut vars, "WRELADB_TOPOLOGY_MODE", Some("explicit"));
        put_var(
            &mut vars,
            "WRELADB_TOPOLOGY_REGION_AZ_NODE_MAP_JSON",
            Some("{\"ord\":{\"ord-a\":[\"n1\"],\"ord-b\":[\"n2\",\"n3\"]}}"),
        );
        put_var(
            &mut vars,
            "WRELADB_TOPOLOGY_REGION_NODE_MAP_JSON",
            Some("{\"ord\":[\"n1\",\"n2\",\"n3\"]}"),
        );
        let _guard = EnvGuard::set(&vars);

        let cfg = DbConfig::from_env_strict().expect("strict env parse");
        let expected = BTreeMap::from([(
            "ord".to_string(),
            BTreeMap::from([
                ("ord-a".to_string(), vec!["n1".to_string()]),
                (
                    "ord-b".to_string(),
                    vec!["n2".to_string(), "n3".to_string()],
                ),
            ]),
        )]);
        assert_eq!(cfg.topology.region_az_node_map, expected);
        cfg.validate_strict().expect("strict validate");
    }

    #[test]
    fn from_env_strict_fails_when_required_region_is_missing() {
        let _lock = ENV_LOCK.lock().expect("env lock");
        let mut vars = strict_base_env_vars();
        put_var(&mut vars, "WRELADB_REGION", None);
        put_var(&mut vars, "WRELADB_RESIDENCY_POLICY_JSON", None);
        let _guard = EnvGuard::set(&vars);

        let err = DbConfig::from_env_strict().expect_err("missing region must fail");
        assert!(err.contains("WRELADB_REGION"));
    }

    #[test]
    fn for_testing_provides_explicit_intent() {
        let cfg = DbConfig::for_testing();
        assert_eq!(cfg.intent.policy_id, "test-local-intent");
        assert_eq!(cfg.intent.latency_target_ms, 5);
        assert_eq!(cfg.intent.residency_scope, vec!["local".to_string()]);
    }

    #[test]
    fn intent_topology_hint_node_count_conversion_saturates() {
        assert_eq!(saturating_u32_from_usize(7), 7);
        assert_eq!(saturating_u32_from_usize(usize::MAX), u32::MAX);
    }

    #[test]
    fn from_env_strict_fails_if_intent_env_missing() {
        let _lock = ENV_LOCK.lock().expect("env lock");
        let mut vars = strict_base_env_vars();
        put_var(&mut vars, "WRELADB_INTENT_JSON", None);
        let _guard = EnvGuard::set(&vars);

        let err = DbConfig::from_env_strict().expect_err("missing intent must fail");
        assert!(err.contains("WRELADB_INTENT_JSON"));
    }

    #[test]
    fn validate_strict_rejects_non_majority_quorum() {
        let _lock = ENV_LOCK.lock().expect("env lock");
        let mut vars = strict_base_env_vars();
        put_var(&mut vars, "WRELADB_WRITE_QUORUM", Some("1"));
        put_var(
            &mut vars,
            "WRELADB_REPLICATION_ASYNC_FAILOVER",
            Some("false"),
        );
        put_var(&mut vars, "WRELADB_SHARD_GROUP_LOCALITY_JSON", None);
        put_var(&mut vars, "WRELADB_RESIDENCY_POLICY_JSON", None);
        let _guard = EnvGuard::set(&vars);

        let err = DbConfig::from_env_strict().expect_err("non-majority must fail");
        assert!(err.contains("majority quorum"));
    }

    #[test]
    fn from_env_strict_fails_when_topology_mode_is_missing() {
        let _lock = ENV_LOCK.lock().expect("env lock");
        let mut vars = strict_base_env_vars();
        put_var(&mut vars, "WRELADB_TOPOLOGY_MODE", None);
        let _guard = EnvGuard::set(&vars);

        let err = DbConfig::from_env_strict().expect_err("missing mode must fail");
        assert!(err.contains("WRELADB_TOPOLOGY_MODE"));
    }

    #[test]
    fn from_env_strict_fails_when_explicit_region_node_payload_is_inconsistent() {
        let _lock = ENV_LOCK.lock().expect("env lock");
        let mut vars = strict_base_env_vars();
        put_var(&mut vars, "WRELADB_TOPOLOGY_MODE", Some("explicit"));
        put_var(
            &mut vars,
            "WRELADB_TOPOLOGY_REGION_AZ_NODE_MAP_JSON",
            Some("{\"ord\":{\"ord-a\":[\"n1\"],\"ord-b\":[\"n2\",\"n3\"]}}"),
        );
        put_var(
            &mut vars,
            "WRELADB_TOPOLOGY_REGION_NODE_MAP_JSON",
            Some("{\"ord\":[\"n1\",\"n2\"]}"),
        );
        let _guard = EnvGuard::set(&vars);

        let err = DbConfig::from_env_strict().expect_err("inconsistent payload must fail");
        assert!(err.contains("WRELADB_TOPOLOGY_REGION_NODE_MAP_JSON"));
        assert!(err.contains("must equal the union of nodes"));
    }

    #[test]
    fn from_env_strict_fails_when_collapsed_az_id_differs_from_region_id() {
        let _lock = ENV_LOCK.lock().expect("env lock");
        let mut vars = strict_base_env_vars();
        put_var(
            &mut vars,
            "WRELADB_TOPOLOGY_REGION_AZ_NODE_MAP_JSON",
            Some("{\"ord\":{\"ord-1\":[\"n1\",\"n2\",\"n3\"]}}"),
        );
        let _guard = EnvGuard::set(&vars);

        let err = DbConfig::from_env_strict().expect_err("collapsed az id mismatch must fail");
        assert!(err.contains("must use az id `ord`"));
    }

    #[test]
    fn from_env_strict_fails_when_collapsed_nodes_do_not_match_region_node_map() {
        let _lock = ENV_LOCK.lock().expect("env lock");
        let mut vars = strict_base_env_vars();
        put_var(
            &mut vars,
            "WRELADB_TOPOLOGY_REGION_AZ_NODE_MAP_JSON",
            Some("{\"ord\":{\"ord\":[\"n1\",\"n2\"]}}"),
        );
        let _guard = EnvGuard::set(&vars);

        let err =
            DbConfig::from_env_strict().expect_err("collapsed node parity mismatch must fail");
        assert!(err.contains("node parity mismatch"));
    }

    #[test]
    fn from_env_strict_rejects_single_domain_with_hyphenated_mode_value() {
        let _lock = ENV_LOCK.lock().expect("env lock");
        let mut vars = strict_base_env_vars();
        put_var(&mut vars, "WRELADB_TOPOLOGY_MODE", Some("single-domain"));
        put_var(&mut vars, "WRELADB_SOVEREIGNTY_ID", Some("ord"));
        let _guard = EnvGuard::set(&vars);

        let err = DbConfig::from_env_strict().expect_err("hyphenated mode must fail");
        assert!(err.contains("must be one of collapsed|explicit|single_domain"));
    }

    #[test]
    fn from_env_strict_fails_when_single_domain_region_does_not_match_sovereignty_id() {
        let _lock = ENV_LOCK.lock().expect("env lock");
        let mut vars = strict_base_env_vars();
        put_var(&mut vars, "WRELADB_TOPOLOGY_MODE", Some("single_domain"));
        let _guard = EnvGuard::set(&vars);

        let err = DbConfig::from_env_strict().expect_err("single_domain mismatch must fail");
        assert!(err.contains("single_domain requires sovereignty id"));
    }

    #[test]
    fn from_env_strict_fails_when_canonical_map_regions_do_not_match_machine_map() {
        let _lock = ENV_LOCK.lock().expect("env lock");
        let mut vars = strict_base_env_vars();
        put_var(
            &mut vars,
            "WRELADB_TOPOLOGY_REGION_AZ_NODE_MAP_JSON",
            Some("{\"dfw\":{\"dfw\":[\"n1\",\"n2\",\"n3\"]}}"),
        );
        put_var(
            &mut vars,
            "WRELADB_TOPOLOGY_REGION_NODE_MAP_JSON",
            Some("{\"dfw\":[\"n1\",\"n2\",\"n3\"]}"),
        );
        let _guard = EnvGuard::set(&vars);

        let err = DbConfig::from_env_strict().expect_err("region mismatch must fail");
        assert!(err.contains("missing region `ord`"));
    }
}
