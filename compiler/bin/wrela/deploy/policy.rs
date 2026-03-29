use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use wrela_runtime::db::autopilot::compiler::{
    CostGuardrailMode, DbIntentCompilerError, DbIntentConfig, DbIntentContradictionCode,
    DbIntentRemediationAction, DbIntentTopologyHints, DbWorkloadClass, NamespacePolicy,
    compile_db_intent,
};

const DEFAULT_MACHINES: usize = 3;
const DEFAULT_RF: u32 = 3;
const DEFAULT_WQ: u32 = 2;
const DEFAULT_LOGICAL_SHARDS: u32 = 16;
const DEFAULT_ACTIVE_GROUPS: u32 = 3;
const DEFAULT_S3_PREFIX: &str = "wreladb/checkpoints";
const DEFAULT_TIGRIS_ENDPOINT: &str = "https://fly.storage.tigris.dev";

#[derive(Debug, Clone, Default)]
pub struct DeployPolicyOverrides {
    pub policy_path: Option<String>,
    pub region: Option<String>,
    pub machines: Option<usize>,
    pub replication_factor: Option<u32>,
    pub write_quorum: Option<u32>,
    pub logical_shards: Option<u32>,
    pub active_groups: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct ResolvedDeployPolicy {
    pub primary_region: String,
    pub region_machine_counts: BTreeMap<String, usize>,
    pub topology_mode: TopologyMode,
    pub region_node_map: BTreeMap<String, Vec<String>>,
    pub region_az_node_map: BTreeMap<String, BTreeMap<String, Vec<String>>>,
    pub machine_count: usize,
    pub target_voters: u32,
    pub replication_factor: u32,
    pub write_quorum: u32,
    pub replication_async_failover: bool,
    pub logical_shards: u32,
    pub active_groups: u32,
    pub checkpoint: CheckpointPolicy,
    pub checkpoint_allowed_regions: Vec<String>,
    pub sovereignty_id: String,
    pub sovereignty_allowed_regions: Vec<String>,
    pub sovereignty_enforce_all_copies: bool,
    pub intent: ResolvedIntentPolicy,
    pub residency_policy_json: Option<String>,
    pub shard_group_locality_json: Option<String>,
    pub mtls_mode: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ResolvedIntentPolicy {
    pub policy_id: String,
    pub workload_class: String,
    pub latency_target_ms: u64,
    pub min_write_throughput_ops: u64,
    pub residency_scope: Vec<String>,
    pub namespace_policies: BTreeMap<String, String>,
    pub cost_guardrail_mode: String,
}

#[derive(Debug, Clone)]
pub struct CheckpointPolicy {
    pub backend: CheckpointBackend,
    pub s3_bucket: Option<String>,
    pub s3_region: Option<String>,
    pub s3_prefix: String,
    pub s3_endpoint: Option<String>,
    pub s3_path_style: bool,
    pub s3_bucket_by_region: BTreeMap<String, String>,
    pub s3_region_by_region: BTreeMap<String, String>,
    pub s3_endpoint_by_region: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckpointBackend {
    File,
    S3,
}

impl CheckpointBackend {
    pub fn as_env_value(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::S3 => "s3",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TopologyMode {
    Collapsed,
    Explicit,
    SingleDomain,
}

impl TopologyMode {
    pub fn as_env_value(self) -> &'static str {
        match self {
            Self::Collapsed => "collapsed",
            Self::Explicit => "explicit",
            Self::SingleDomain => "single_domain",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
struct DeployPolicyFile {
    cluster: ClusterPolicyFile,
    replication: ReplicationPolicyFile,
    topology: TopologyPolicyFile,
    regions: BTreeMap<String, usize>,
    checkpoint: CheckpointPolicyFile,
    sovereignty: SovereigntyPolicyFile,
    intent: Option<IntentPolicyFile>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
struct ClusterPolicyFile {
    target_voters: Option<u32>,
    replication_factor: Option<u32>,
    write_quorum: Option<u32>,
    logical_shards: Option<u32>,
    active_groups: Option<u32>,
    machines: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
struct ReplicationPolicyFile {
    async_failover: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
struct TopologyPolicyFile {
    mode: Option<String>,
    region_node_map: BTreeMap<String, Vec<String>>,
    region_az_node_map: BTreeMap<String, BTreeMap<String, Vec<String>>>,
    single_domain: Option<SingleDomainTopologyPolicyFile>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
struct SingleDomainTopologyPolicyFile {
    id: Option<String>,
    nodes: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
struct CheckpointPolicyFile {
    backend: Option<String>,
    s3_bucket: Option<String>,
    s3_region: Option<String>,
    s3_prefix: Option<String>,
    s3_endpoint: Option<String>,
    s3_path_style: Option<bool>,
    s3_bucket_by_region: BTreeMap<String, String>,
    s3_region_by_region: BTreeMap<String, String>,
    s3_endpoint_by_region: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
struct SovereigntyPolicyFile {
    id: Option<String>,
    allowed_regions: Vec<String>,
    enforce_all_copies: Option<bool>,
    residency_policy_json: Option<String>,
    shard_group_locality_json: Option<String>,
    mtls_mode: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
struct IntentPolicyFile {
    policy_id: Option<String>,
    workload_class: Option<String>,
    latency_target_ms: Option<u64>,
    min_write_throughput_ops: Option<u64>,
    residency_scope: Vec<String>,
    namespace_policies: BTreeMap<String, String>,
    cost_guardrail_mode: Option<String>,
}

pub fn resolve_deploy_policy(
    project_root: &Path,
    default_region: &str,
    overrides: &DeployPolicyOverrides,
) -> Result<ResolvedDeployPolicy, String> {
    let file = load_policy_file(project_root, overrides.policy_path.as_deref())?;

    let mut region_machine_counts = normalize_region_machine_counts(&file.regions);
    if !region_machine_counts.is_empty()
        && (overrides.region.is_some() || overrides.machines.is_some())
        && region_machine_counts.len() > 1
    {
        return Err(
            "cannot combine --region/--machines with multi-region `wrela.deploy.toml` regions; \
use one source of truth"
                .to_string(),
        );
    }

    let default_region = normalize_region(default_region)
        .ok_or_else(|| "deploy region resolved to empty value".to_string())?;

    let resolved_region = overrides
        .region
        .as_deref()
        .and_then(normalize_region)
        .or_else(|| {
            if region_machine_counts.is_empty() {
                None
            } else if region_machine_counts.contains_key(&default_region) {
                Some(default_region.clone())
            } else if region_machine_counts.len() == 1 {
                region_machine_counts.keys().next().cloned()
            } else {
                region_machine_counts.keys().next().cloned()
            }
        })
        .unwrap_or(default_region.clone());

    if region_machine_counts.is_empty() {
        let machines = overrides
            .machines
            .or(file.cluster.machines)
            .unwrap_or(DEFAULT_MACHINES)
            .max(1);
        region_machine_counts.insert(resolved_region.clone(), machines);
    } else if let Some(machines) = overrides.machines {
        region_machine_counts.insert(resolved_region.clone(), machines.max(1));
    }

    if let Some(cli_region) = overrides.region.as_deref().and_then(normalize_region) {
        if !region_machine_counts.contains_key(&cli_region) {
            let machines = overrides
                .machines
                .or(file.cluster.machines)
                .unwrap_or(DEFAULT_MACHINES)
                .max(1);
            region_machine_counts.clear();
            region_machine_counts.insert(cli_region, machines);
        }
    }

    if region_machine_counts.is_empty() {
        return Err("deploy requires at least one region with machine count".to_string());
    }

    let machine_count = region_machine_counts.values().copied().sum::<usize>();
    if machine_count == 0 {
        return Err("deploy machine count must be at least 1".to_string());
    }

    let (topology_mode, region_node_map, region_az_node_map) = resolve_topology(&file.topology)?;
    let single_domain_id = match topology_mode {
        TopologyMode::Collapsed => {
            for (region, az_map) in &region_az_node_map {
                if az_map.len() != 1 || !az_map.contains_key(region) {
                    return Err(format!(
                        "collapsed topology requires canonical az id `{region}` to match region id"
                    ));
                }
                if az_map.get(region) != region_node_map.get(region) {
                    return Err(format!(
                        "collapsed topology requires az `{region}` nodes to mirror topology.region_node_map"
                    ));
                }
            }
            None
        }
        TopologyMode::Explicit => None,
        TopologyMode::SingleDomain => {
            let domain_id =
                region_node_map.keys().next().cloned().ok_or_else(|| {
                    "single_domain topology failed to resolve domain id".to_string()
                })?;
            if region_node_map.len() != 1 || region_az_node_map.len() != 1 {
                return Err(
                    "single_domain topology requires one domain id reused for region and az"
                        .to_string(),
                );
            }
            let Some(az_map) = region_az_node_map.get(&domain_id) else {
                return Err(
                    "single_domain topology requires one domain id reused for region and az"
                        .to_string(),
                );
            };
            if az_map.len() != 1 || !az_map.contains_key(&domain_id) {
                return Err(
                    "single_domain topology requires one domain id reused for region and az"
                        .to_string(),
                );
            }
            if az_map.get(&domain_id) != region_node_map.get(&domain_id) {
                return Err(
                    "single_domain topology requires az nodes to mirror topology.single_domain.nodes"
                        .to_string(),
                );
            }
            if region_machine_counts.len() != 1 || !region_machine_counts.contains_key(&domain_id) {
                return Err(format!(
                    "single_domain topology requires [regions] to contain exactly `{domain_id}`"
                ));
            }
            Some(domain_id)
        }
    };
    for region in region_machine_counts.keys() {
        let Some(az_map) = region_az_node_map.get(region) else {
            return Err(format!(
                "topology.region_az_node_map missing entry for deploy region `{region}`"
            ));
        };
        let node_count = az_map.values().map(Vec::len).sum::<usize>();
        let expected = *region_machine_counts.get(region).unwrap_or(&0);
        if node_count < expected {
            return Err(format!(
                "topology.region_az_node_map region `{region}` has {node_count} nodes but deploy requires at least {expected}"
            ));
        }
    }
    for region in region_az_node_map.keys() {
        if !region_machine_counts.contains_key(region) {
            return Err(format!(
                "topology.region_az_node_map contains region `{region}` not present in [regions]"
            ));
        }
    }

    let target_voters = file
        .cluster
        .target_voters
        .unwrap_or(machine_count as u32)
        .max(1);
    if (target_voters as usize) > machine_count {
        return Err(format!(
            "target voters {} cannot exceed machine count {}",
            target_voters, machine_count
        ));
    }

    let replication_factor = overrides
        .replication_factor
        .or(file.cluster.replication_factor)
        .unwrap_or(DEFAULT_RF)
        .max(1);
    if replication_factor > target_voters {
        return Err(format!(
            "replication factor {} cannot exceed target voters {}",
            replication_factor, target_voters
        ));
    }

    let write_quorum = overrides
        .write_quorum
        .or(file.cluster.write_quorum)
        .unwrap_or(DEFAULT_WQ)
        .max(1);
    if write_quorum > replication_factor {
        return Err(format!(
            "write quorum {} cannot exceed replication factor {}",
            write_quorum, replication_factor
        ));
    }
    let majority = (replication_factor / 2) + 1;
    if write_quorum < majority {
        return Err(format!(
            "write quorum {} must be majority quorum for replication factor {} (min {})",
            write_quorum, replication_factor, majority
        ));
    }
    let replication_async_failover = file.replication.async_failover.ok_or_else(|| {
        "replication.async_failover is required (hard cutover, no implicit default)".to_string()
    })?;

    let logical_shards = overrides
        .logical_shards
        .or(file.cluster.logical_shards)
        .unwrap_or(DEFAULT_LOGICAL_SHARDS)
        .max(1);
    let active_groups = overrides
        .active_groups
        .or(file.cluster.active_groups)
        .unwrap_or(DEFAULT_ACTIVE_GROUPS)
        .max(1);
    if active_groups > logical_shards {
        return Err(format!(
            "active groups {} cannot exceed logical shards {}",
            active_groups, logical_shards
        ));
    }
    let intent = resolve_intent_policy(
        file.intent.as_ref(),
        DbIntentTopologyHints {
            available_nodes: u32::try_from(machine_count).unwrap_or(u32::MAX),
            logical_shards,
        },
    )?;

    let checkpoint = resolve_checkpoint_policy(&file.checkpoint, &region_machine_counts)?;

    let sovereignty_id = trim_non_empty(file.sovereignty.id.as_deref().unwrap_or(""))
        .ok_or_else(|| "sovereignty.id is required and must be non-empty".to_string())?;
    if let Some(domain_id) = single_domain_id.as_ref() {
        if sovereignty_id != *domain_id {
            return Err(format!(
                "single_domain topology requires sovereignty.id = `{domain_id}`"
            ));
        }
    }
    let sovereignty_enforce_all_copies = file.sovereignty.enforce_all_copies.ok_or_else(|| {
        "sovereignty.enforce_all_copies is required (hard cutover, no implicit default)".to_string()
    })?;
    let mut sovereignty_allowed_regions = file
        .sovereignty
        .allowed_regions
        .iter()
        .filter_map(|region| normalize_region(region))
        .collect::<Vec<_>>();
    if sovereignty_allowed_regions.is_empty() {
        return Err(
            "sovereignty.allowed_regions is required and must contain at least one region"
                .to_string(),
        );
    }
    sovereignty_allowed_regions.sort();
    sovereignty_allowed_regions.dedup();
    let mut checkpoint_allowed_regions = file
        .sovereignty
        .allowed_regions
        .iter()
        .filter_map(|region| normalize_region(region))
        .collect::<Vec<_>>();
    checkpoint_allowed_regions.sort();
    checkpoint_allowed_regions.dedup();
    if checkpoint_allowed_regions.is_empty() {
        return Err(
            "sovereignty.allowed_regions must include checkpoint-eligible regions".to_string(),
        );
    }
    if let Some(domain_id) = single_domain_id.as_ref() {
        let expected = vec![domain_id.clone()];
        if sovereignty_enforce_all_copies && checkpoint_allowed_regions != expected {
            return Err(format!(
                "single_domain topology with sovereignty.enforce_all_copies=true requires checkpoint allowed regions = [\"{domain_id}\"]"
            ));
        }
        if sovereignty_allowed_regions != expected {
            return Err(format!(
                "single_domain topology requires sovereignty.allowed_regions = [\"{domain_id}\"]"
            ));
        }
    }
    for region in &sovereignty_allowed_regions {
        if !region_machine_counts.contains_key(region) {
            return Err(format!(
                "sovereignty.allowed_regions includes `{region}` which is not present in [regions]"
            ));
        }
    }

    Ok(ResolvedDeployPolicy {
        primary_region: resolved_region,
        region_machine_counts,
        topology_mode,
        region_node_map,
        region_az_node_map,
        machine_count,
        target_voters,
        replication_factor,
        write_quorum,
        replication_async_failover,
        logical_shards,
        active_groups,
        checkpoint,
        checkpoint_allowed_regions,
        sovereignty_id,
        sovereignty_allowed_regions,
        sovereignty_enforce_all_copies,
        intent,
        residency_policy_json: file.sovereignty.residency_policy_json,
        shard_group_locality_json: file.sovereignty.shard_group_locality_json,
        mtls_mode: file
            .sovereignty
            .mtls_mode
            .as_deref()
            .and_then(|v| {
                let v = v.trim().to_ascii_lowercase();
                match v.as_str() {
                    "auto" | "on" | "off" => Some(v),
                    _ => None,
                }
            })
            .unwrap_or_else(|| "off".to_string()),
    })
}

fn resolve_intent_policy(
    intent: Option<&IntentPolicyFile>,
    topology_hints: DbIntentTopologyHints,
) -> Result<ResolvedIntentPolicy, String> {
    let Some(intent) = intent else {
        return Err(
            "intent block is required (hard cutover): add [intent] with policy_id, workload_class, latency_target_ms, min_write_throughput_ops, residency_scope, namespace_policies, and cost_guardrail_mode"
                .to_string(),
        );
    };

    let policy_id = intent
        .policy_id
        .as_deref()
        .and_then(trim_non_empty)
        .ok_or_else(|| "intent.policy_id is required and must be non-empty".to_string())?;

    let workload_class = intent
        .workload_class
        .as_deref()
        .and_then(trim_non_empty)
        .ok_or_else(|| "intent.workload_class is required".to_string())?;
    let workload_class = parse_workload_class(&workload_class)?;

    let latency_target_ms = intent
        .latency_target_ms
        .ok_or_else(|| "intent.latency_target_ms is required".to_string())?;

    let min_write_throughput_ops = intent
        .min_write_throughput_ops
        .ok_or_else(|| "intent.min_write_throughput_ops is required".to_string())?;

    let mut namespace_policies = BTreeMap::<String, NamespacePolicy>::new();
    for (namespace, policy_name) in &intent.namespace_policies {
        let namespace = trim_non_empty(namespace).ok_or_else(|| {
            "intent.namespace_policies includes invalid namespace `<empty>`. remediation: use lowercase namespace keys (letters, digits, dashes)"
                .to_string()
        })?;
        let normalized_namespace = namespace.to_ascii_lowercase();
        if !is_valid_namespace_key(&normalized_namespace) {
            return Err(format!(
                "intent.namespace_policies includes invalid namespace `{namespace}`. remediation: use lowercase namespace keys (letters, digits, dashes)"
            ));
        }
        let policy_name = trim_non_empty(policy_name).ok_or_else(|| {
            format!(
                "intent.namespace_policies.{normalized_namespace} must be non-empty. remediation: use one of hot_meta|warm|cold_tierable"
            )
        })?;
        let Some(policy) = parse_namespace_policy(&policy_name) else {
            return Err(format!(
                "intent.namespace_policies.{normalized_namespace} `{policy_name}` is invalid. remediation: use one of hot_meta|warm|cold_tierable"
            ));
        };
        if namespace_policies
            .insert(normalized_namespace.clone(), policy)
            .is_some()
        {
            return Err(format!(
                "intent.namespace_policies includes duplicate namespace `{normalized_namespace}` after normalization. remediation: remove duplicate case variants"
            ));
        }
    }
    if namespace_policies.is_empty() {
        return Err(
            "intent.namespace_policies is required and must include at least one namespace=>policy mapping"
                .to_string(),
        );
    }

    let cost_guardrail_mode = parse_cost_guardrail_mode(intent.cost_guardrail_mode.as_deref())?;
    let config = DbIntentConfig {
        policy_id,
        workload_class,
        latency_target_ms,
        min_write_throughput_ops,
        residency_scope: intent.residency_scope.clone(),
        namespace_policies,
        cost_guardrail_mode,
    };

    let compiled =
        compile_db_intent(&config, topology_hints).map_err(format_intent_compile_error)?;

    Ok(ResolvedIntentPolicy {
        policy_id: compiled.policy_id,
        workload_class: workload_class_code(compiled.workload_class).to_string(),
        latency_target_ms: compiled.latency_target_ms,
        min_write_throughput_ops: compiled.min_write_throughput_ops,
        residency_scope: compiled.residency_scope,
        namespace_policies: compiled
            .namespace_policies
            .into_iter()
            .map(|(namespace, policy)| (namespace, namespace_policy_code(policy).to_string()))
            .collect(),
        cost_guardrail_mode: cost_guardrail_mode_code(compiled.cost_guardrail_mode).to_string(),
    })
}

fn parse_workload_class(value: &str) -> Result<DbWorkloadClass, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "general_transactional" => Ok(DbWorkloadClass::GeneralTransactional),
        "analytics_heavy" => Ok(DbWorkloadClass::AnalyticsHeavy),
        _ => Err(format!(
            "intent.workload_class `{value}` is invalid. remediation: use one of general_transactional|analytics_heavy"
        )),
    }
}

fn parse_namespace_policy(value: &str) -> Option<NamespacePolicy> {
    match value.trim().to_ascii_lowercase().as_str() {
        "hot_meta" => Some(NamespacePolicy::HotMeta),
        "warm" => Some(NamespacePolicy::Warm),
        "cold_tierable" => Some(NamespacePolicy::ColdTierable),
        _ => None,
    }
}

fn parse_cost_guardrail_mode(value: Option<&str>) -> Result<CostGuardrailMode, String> {
    let Some(value) = value.and_then(trim_non_empty) else {
        return Err(
            "intent.cost_guardrail_mode is required. remediation: set estimate_and_warn_only"
                .to_string(),
        );
    };
    match value.to_ascii_lowercase().as_str() {
        "estimate_and_warn_only" => Ok(CostGuardrailMode::EstimateAndWarnOnly),
        _ => Err(format!(
            "intent.cost_guardrail_mode `{value}` is invalid. remediation: use estimate_and_warn_only"
        )),
    }
}

fn is_valid_namespace_key(namespace: &str) -> bool {
    namespace
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn format_intent_compile_error(err: DbIntentCompilerError) -> String {
    match err {
        DbIntentCompilerError::EmptyPolicyId => {
            "intent.policy_id must be non-empty. remediation: set intent.policy_id to a stable identifier"
                .to_string()
        }
        DbIntentCompilerError::EmptyNamespacePolicies => {
            "intent.namespace_policies must include at least one namespace=>policy mapping. remediation: add one or more namespace_policies entries"
                .to_string()
        }
        DbIntentCompilerError::InvalidNamespace(namespace) => format!(
            "intent.namespace_policies includes invalid namespace `{namespace}`. remediation: use lowercase namespace keys (letters, digits, dashes)"
        ),
        DbIntentCompilerError::InvalidResidencyRegion(region) => format!(
            "intent.residency_scope contains invalid region `{region}`. remediation: use lowercase region ids (letters, digits, dashes)"
        ),
        DbIntentCompilerError::DuplicateResidencyRegion(region) => format!(
            "intent.residency_scope contains duplicate region `{region}`. remediation: remove duplicate entries"
        ),
        DbIntentCompilerError::Contradiction(contradiction) => {
            let remediation_hints = contradiction
                .remediations
                .iter()
                .map(|remediation| {
                    format!(
                        "{}: {}",
                        remediation_action_code(remediation.action),
                        remediation.detail
                    )
                })
                .collect::<Vec<_>>()
                .join("; ");
            format!(
                "intent contradiction `{}`: {}. remediation hints: {}",
                contradiction_code(contradiction.code),
                contradiction.reason,
                remediation_hints
            )
        }
    }
}

fn contradiction_code(code: DbIntentContradictionCode) -> &'static str {
    match code {
        DbIntentContradictionCode::ImpossibleTopologyForHighThroughput => {
            "impossible_topology_for_high_throughput"
        }
        DbIntentContradictionCode::LatencyTargetInvalid => "latency_target_invalid",
        DbIntentContradictionCode::ResidencyScopeEmpty => "residency_scope_empty",
    }
}

fn remediation_action_code(action: DbIntentRemediationAction) -> &'static str {
    match action {
        DbIntentRemediationAction::IncreaseNodes => "increase_nodes",
        DbIntentRemediationAction::IncreaseShards => "increase_shards",
        DbIntentRemediationAction::RelaxThroughputTarget => "relax_throughput_target",
        DbIntentRemediationAction::SetPositiveLatencyTarget => "set_positive_latency_target",
        DbIntentRemediationAction::PopulateResidencyScope => "populate_residency_scope",
    }
}

fn workload_class_code(value: DbWorkloadClass) -> &'static str {
    match value {
        DbWorkloadClass::GeneralTransactional => "general_transactional",
        DbWorkloadClass::AnalyticsHeavy => "analytics_heavy",
    }
}

fn namespace_policy_code(value: NamespacePolicy) -> &'static str {
    match value {
        NamespacePolicy::HotMeta => "hot_meta",
        NamespacePolicy::Warm => "warm",
        NamespacePolicy::ColdTierable => "cold_tierable",
    }
}

fn cost_guardrail_mode_code(value: CostGuardrailMode) -> &'static str {
    match value {
        CostGuardrailMode::EstimateAndWarnOnly => "estimate_and_warn_only",
    }
}

fn load_policy_file(
    project_root: &Path,
    policy_path: Option<&str>,
) -> Result<DeployPolicyFile, String> {
    let path = match policy_path {
        Some(raw) => {
            let candidate = PathBuf::from(raw);
            if candidate.is_absolute() {
                candidate
            } else {
                project_root.join(candidate)
            }
        }
        None => project_root.join("wrela.deploy.toml"),
    };

    if !path.exists() {
        return if policy_path.is_some() {
            Err(format!(
                "deploy policy not found at {} (from --deploy-policy)",
                path.display()
            ))
        } else {
            Ok(DeployPolicyFile::default())
        };
    }

    let raw = fs::read_to_string(&path)
        .map_err(|err| format!("failed to read deploy policy {}: {}", path.display(), err))?;
    toml::from_str::<DeployPolicyFile>(&raw)
        .map_err(|err| format!("invalid deploy policy {}: {}", path.display(), err))
}

fn normalize_region_machine_counts(input: &BTreeMap<String, usize>) -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    for (region, machines) in input {
        let Some(normalized) = normalize_region(region) else {
            continue;
        };
        out.insert(normalized, (*machines).max(1));
    }
    out
}

fn normalize_region_az_node_map(
    input: &BTreeMap<String, BTreeMap<String, Vec<String>>>,
) -> BTreeMap<String, BTreeMap<String, Vec<String>>> {
    let mut out = BTreeMap::new();
    for (region, az_map) in input {
        let Some(normalized_region) = normalize_region(region) else {
            continue;
        };
        let mut normalized_az_map = BTreeMap::new();
        for (az, nodes) in az_map {
            let Some(normalized_az) = trim_non_empty(az).map(|value| value.to_ascii_lowercase())
            else {
                continue;
            };
            let mut normalized_nodes = nodes
                .iter()
                .filter_map(|node| trim_non_empty(node))
                .collect::<Vec<_>>();
            normalized_nodes.sort();
            normalized_nodes.dedup();
            if normalized_nodes.is_empty() {
                continue;
            }
            normalized_az_map.insert(normalized_az, normalized_nodes);
        }
        if normalized_az_map.is_empty() {
            continue;
        }
        out.insert(normalized_region, normalized_az_map);
    }
    out
}

fn normalize_region_node_map(
    input: &BTreeMap<String, Vec<String>>,
) -> BTreeMap<String, Vec<String>> {
    let mut out = BTreeMap::new();
    for (region, nodes) in input {
        let Some(normalized_region) = normalize_region(region) else {
            continue;
        };
        let normalized_nodes = normalize_nodes(nodes);
        if normalized_nodes.is_empty() {
            continue;
        }
        out.insert(normalized_region, normalized_nodes);
    }
    out
}

fn normalize_nodes(nodes: &[String]) -> Vec<String> {
    let mut normalized_nodes = nodes
        .iter()
        .filter_map(|node| trim_non_empty(node))
        .collect::<Vec<_>>();
    normalized_nodes.sort();
    normalized_nodes.dedup();
    normalized_nodes
}

fn resolve_topology(
    topology: &TopologyPolicyFile,
) -> Result<
    (
        TopologyMode,
        BTreeMap<String, Vec<String>>,
        BTreeMap<String, BTreeMap<String, Vec<String>>>,
    ),
    String,
> {
    let mode = match topology
        .mode
        .as_deref()
        .and_then(trim_non_empty)
        .map(|value| value.to_ascii_lowercase())
        .as_deref()
    {
        Some("collapsed") => TopologyMode::Collapsed,
        Some("explicit") => TopologyMode::Explicit,
        Some("single_domain") => TopologyMode::SingleDomain,
        _ => {
            return Err(
                "topology.mode is required and must be one of `collapsed|explicit|single_domain`"
                    .to_string(),
            );
        }
    };

    match mode {
        TopologyMode::Collapsed => {
            if !topology.region_az_node_map.is_empty() {
                return Err(
                    "topology.region_az_node_map is not allowed when topology.mode = collapsed"
                        .to_string(),
                );
            }
            if topology.single_domain.is_some() {
                return Err(
                    "topology.single_domain is not allowed when topology.mode = collapsed"
                        .to_string(),
                );
            }
            let region_node_map = normalize_region_node_map(&topology.region_node_map);
            if region_node_map.is_empty() {
                return Err(
                    "topology.region_node_map is required when topology.mode = collapsed"
                        .to_string(),
                );
            }
            let mut region_az_node_map = BTreeMap::new();
            for (region, nodes) in &region_node_map {
                region_az_node_map.insert(
                    region.clone(),
                    BTreeMap::from([(region.clone(), nodes.clone())]),
                );
            }
            Ok((mode, region_node_map, region_az_node_map))
        }
        TopologyMode::Explicit => {
            if !topology.region_node_map.is_empty() {
                return Err(
                    "topology.region_node_map is not allowed when topology.mode = explicit"
                        .to_string(),
                );
            }
            if topology.single_domain.is_some() {
                return Err(
                    "topology.single_domain is not allowed when topology.mode = explicit"
                        .to_string(),
                );
            }
            let region_az_node_map = normalize_region_az_node_map(&topology.region_az_node_map);
            if region_az_node_map.is_empty() {
                return Err(
                    "topology.region_az_node_map is required when topology.mode = explicit"
                        .to_string(),
                );
            }
            let mut region_node_map = BTreeMap::new();
            for (region, az_map) in &region_az_node_map {
                let mut nodes = az_map.values().flatten().cloned().collect::<Vec<_>>();
                nodes.sort();
                nodes.dedup();
                if !nodes.is_empty() {
                    region_node_map.insert(region.clone(), nodes);
                }
            }
            if region_node_map.is_empty() {
                return Err(
                    "topology.region_az_node_map is required when topology.mode = explicit"
                        .to_string(),
                );
            }
            Ok((mode, region_node_map, region_az_node_map))
        }
        TopologyMode::SingleDomain => {
            if !topology.region_node_map.is_empty() {
                return Err(
                    "topology.region_node_map is not allowed when topology.mode = single_domain"
                        .to_string(),
                );
            }
            if !topology.region_az_node_map.is_empty() {
                return Err(
                    "topology.region_az_node_map is not allowed when topology.mode = single_domain"
                        .to_string(),
                );
            }
            let single_domain = topology.single_domain.as_ref().ok_or_else(|| {
                "topology.single_domain is required when topology.mode = single_domain".to_string()
            })?;
            let domain_id = single_domain
                .id
                .as_deref()
                .and_then(trim_non_empty)
                .map(|value| value.to_ascii_lowercase())
                .ok_or_else(|| {
                    "topology.single_domain.id is required when topology.mode = single_domain"
                        .to_string()
                })?;
            let nodes = normalize_nodes(&single_domain.nodes);
            if nodes.is_empty() {
                return Err(
                    "topology.single_domain.nodes is required when topology.mode = single_domain"
                        .to_string(),
                );
            }
            let region_node_map = BTreeMap::from([(domain_id.clone(), nodes.clone())]);
            let region_az_node_map =
                BTreeMap::from([(domain_id.clone(), BTreeMap::from([(domain_id, nodes)]))]);
            Ok((mode, region_node_map, region_az_node_map))
        }
    }
}

fn resolve_checkpoint_policy(
    file: &CheckpointPolicyFile,
    region_machine_counts: &BTreeMap<String, usize>,
) -> Result<CheckpointPolicy, String> {
    let backend = match file
        .backend
        .as_deref()
        .unwrap_or("file")
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "file" => CheckpointBackend::File,
        "s3" | "tigris" => CheckpointBackend::S3,
        other => {
            return Err(format!(
                "invalid checkpoint.backend `{other}` (expected file|s3|tigris)"
            ));
        }
    };

    let s3_bucket = file.s3_bucket.as_ref().and_then(|v| trim_non_empty(v));
    let s3_region = file.s3_region.as_ref().and_then(|v| trim_non_empty(v));
    let s3_prefix = file
        .s3_prefix
        .as_ref()
        .and_then(|value| trim_non_empty(value))
        .unwrap_or_else(|| DEFAULT_S3_PREFIX.to_string());
    let s3_endpoint = file
        .s3_endpoint
        .as_ref()
        .and_then(|v| trim_non_empty(v))
        .or_else(|| {
            if backend == CheckpointBackend::S3 {
                Some(DEFAULT_TIGRIS_ENDPOINT.to_string())
            } else {
                None
            }
        });

    let s3_bucket_by_region = normalize_region_string_map(&file.s3_bucket_by_region);
    let s3_region_by_region = normalize_region_string_map(&file.s3_region_by_region);
    let s3_endpoint_by_region = normalize_region_string_map(&file.s3_endpoint_by_region);

    if backend == CheckpointBackend::S3 {
        if s3_bucket.is_none() && s3_bucket_by_region.is_empty() {
            return Err(
                "checkpoint backend `s3` requires `checkpoint.s3_bucket` or `checkpoint.s3_bucket_by_region`"
                    .to_string(),
            );
        }
        if s3_region.is_none() && s3_region_by_region.is_empty() {
            return Err(
                "checkpoint backend `s3` requires `checkpoint.s3_region` or `checkpoint.s3_region_by_region`"
                    .to_string(),
            );
        }
        if region_machine_counts.len() > 1 {
            for region in region_machine_counts.keys() {
                if !s3_bucket_by_region.contains_key(region) {
                    return Err(format!(
                        "multi-region deploy requires checkpoint.s3_bucket_by_region entry for region `{region}`"
                    ));
                }
                if !s3_region_by_region.contains_key(region) {
                    return Err(format!(
                        "multi-region deploy requires checkpoint.s3_region_by_region entry for region `{region}`"
                    ));
                }
            }
        }
    }

    Ok(CheckpointPolicy {
        backend,
        s3_bucket,
        s3_region,
        s3_prefix,
        s3_endpoint,
        s3_path_style: file
            .s3_path_style
            .unwrap_or(backend == CheckpointBackend::S3),
        s3_bucket_by_region,
        s3_region_by_region,
        s3_endpoint_by_region,
    })
}

fn normalize_region_string_map(input: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for (region, value) in input {
        let Some(region) = normalize_region(region) else {
            continue;
        };
        let Some(value) = trim_non_empty(value) else {
            continue;
        };
        out.insert(region, value);
    }
    out
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

#[cfg(test)]
mod tests {
    use super::*;

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

    fn with_required_intent(policy: &str) -> String {
        format!("{policy}\n{VALID_INTENT_BLOCK}")
    }

    fn write_policy_with_custom_intent(
        temp: &tempfile::TempDir,
        residency_scope_toml: &str,
        namespace_policies_toml: &str,
    ) {
        std::fs::write(
            temp.path().join("wrela.deploy.toml"),
            format!(
                r#"
[topology]
mode = "collapsed"

[cluster]
target_voters = 3
replication_factor = 3
write_quorum = 2
logical_shards = 16
active_groups = 3

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

[intent]
policy_id = "orders-v7"
workload_class = "general_transactional"
latency_target_ms = 90
min_write_throughput_ops = 12000
residency_scope = {residency_scope_toml}
cost_guardrail_mode = "estimate_and_warn_only"

[intent.namespace_policies]
{namespace_policies_toml}
"#
            ),
        )
        .expect("write policy");
    }

    #[test]
    fn rejects_missing_strict_schema_fields() {
        let temp = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            temp.path().join("wrela.deploy.toml"),
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
        )
        .expect("write policy");
        let err = resolve_deploy_policy(temp.path(), "ord", &DeployPolicyOverrides::default())
            .expect_err("must fail");
        assert!(err.contains("intent block is required"), "{err}");
    }

    #[test]
    fn parses_valid_intent_and_normalizes_fields() {
        let temp = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            temp.path().join("wrela.deploy.toml"),
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

[intent]
policy_id = "  orders-v7  "
workload_class = "  general_transactional  "
latency_target_ms = 90
min_write_throughput_ops = 12000
residency_scope = [" eu-west ", "us-east"]
cost_guardrail_mode = "estimate_and_warn_only"

[intent.namespace_policies]
ORDERS = " hot_meta "
"#,
        )
        .expect("write policy");

        let resolved = resolve_deploy_policy(temp.path(), "ord", &DeployPolicyOverrides::default())
            .expect("resolve policy");
        assert_eq!(resolved.intent.policy_id, "orders-v7");
        assert_eq!(resolved.intent.workload_class, "general_transactional");
        assert_eq!(resolved.intent.latency_target_ms, 90);
        assert_eq!(resolved.intent.min_write_throughput_ops, 12000);
        assert_eq!(
            resolved.intent.residency_scope,
            vec!["eu-west".to_string(), "us-east".to_string()]
        );
        assert_eq!(
            resolved.intent.namespace_policies,
            BTreeMap::from([("orders".to_string(), "hot_meta".to_string())])
        );
        assert_eq!(
            resolved.intent.cost_guardrail_mode,
            "estimate_and_warn_only".to_string()
        );
    }

    #[test]
    fn rejects_legacy_cost_guardrail_mode_aliases() {
        for alias in ["estimate_warn_only", "estimate+warn_only", "estimate+warn"] {
            let temp = tempfile::tempdir().expect("tempdir");
            std::fs::write(
                temp.path().join("wrela.deploy.toml"),
                format!(
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

[intent]
policy_id = "orders-v7"
workload_class = "general_transactional"
latency_target_ms = 90
min_write_throughput_ops = 12000
residency_scope = ["us-east", "eu-west"]
cost_guardrail_mode = "{alias}"

[intent.namespace_policies]
orders = "warm"
"#
                ),
            )
            .expect("write policy");

            let err = resolve_deploy_policy(temp.path(), "ord", &DeployPolicyOverrides::default())
                .expect_err("must fail");
            assert!(
                err.contains(&format!("intent.cost_guardrail_mode `{alias}` is invalid")),
                "{err}"
            );
        }
    }

    #[test]
    fn rejects_invalid_intent_namespace_entry() {
        let temp = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            temp.path().join("wrela.deploy.toml"),
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

[intent]
policy_id = "orders-v7"
workload_class = "general_transactional"
latency_target_ms = 90
min_write_throughput_ops = 12000
residency_scope = ["us-east", "eu-west"]
cost_guardrail_mode = "estimate_and_warn_only"

[intent.namespace_policies]
orders = "warm"
orders_meta = "hot_meta"
"#,
        )
        .expect("write policy");

        let err = resolve_deploy_policy(temp.path(), "ord", &DeployPolicyOverrides::default())
            .expect_err("must fail");
        assert!(
            err.contains("intent.namespace_policies includes invalid namespace `orders_meta`"),
            "{err}"
        );
    }

    #[test]
    fn rejects_empty_intent_namespace_policy_value() {
        let temp = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            temp.path().join("wrela.deploy.toml"),
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

[intent]
policy_id = "orders-v7"
workload_class = "general_transactional"
latency_target_ms = 90
min_write_throughput_ops = 12000
residency_scope = ["us-east", "eu-west"]
cost_guardrail_mode = "estimate_and_warn_only"

[intent.namespace_policies]
orders = "warm"
billing = "   "
"#,
        )
        .expect("write policy");

        let err = resolve_deploy_policy(temp.path(), "ord", &DeployPolicyOverrides::default())
            .expect_err("must fail");
        assert!(
            err.contains("intent.namespace_policies.billing must be non-empty"),
            "{err}"
        );
    }

    #[test]
    fn rejects_colliding_intent_namespace_entries_after_normalization() {
        let temp = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            temp.path().join("wrela.deploy.toml"),
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

[intent]
policy_id = "orders-v7"
workload_class = "general_transactional"
latency_target_ms = 90
min_write_throughput_ops = 12000
residency_scope = ["us-east", "eu-west"]
cost_guardrail_mode = "estimate_and_warn_only"

[intent.namespace_policies]
ORDERS = "warm"
orders = "hot_meta"
"#,
        )
        .expect("write policy");

        let err = resolve_deploy_policy(temp.path(), "ord", &DeployPolicyOverrides::default())
            .expect_err("must fail");
        assert!(
            err.contains(
                "intent.namespace_policies includes duplicate namespace `orders` after normalization"
            ),
            "{err}"
        );
    }

    #[test]
    fn fuzz_residency_scope_normalization_matrix() {
        let mut large_scope = Vec::new();
        for idx in 0..128 {
            large_scope.push(format!("\" r-{idx:03} \""));
        }
        let large_scope_toml = format!("[{}]", large_scope.join(", "));

        let cases = vec![
            (
                "[\" us-east \", \"\\teu-west\\t\", \"ap-01\"]".to_string(),
                true,
                Some(vec![
                    "ap-01".to_string(),
                    "eu-west".to_string(),
                    "us-east".to_string(),
                ]),
                None,
            ),
            (
                "[\" us-east \", \"US-EAST\"]".to_string(),
                false,
                None,
                Some("intent.residency_scope contains duplicate region `us-east`"),
            ),
            (
                "[\"us-east\", \"   \"]".to_string(),
                false,
                None,
                Some("intent.residency_scope contains invalid region"),
            ),
            (
                "[\"us-east\", \"eu_west\"]".to_string(),
                false,
                None,
                Some("intent.residency_scope contains invalid region `eu_west`"),
            ),
            (
                large_scope_toml,
                true,
                Some((0..128).map(|idx| format!("r-{idx:03}")).collect()),
                None,
            ),
        ];

        for (residency_scope_toml, should_pass, expected_scope, expected_error) in cases {
            let temp = tempfile::tempdir().expect("tempdir");
            write_policy_with_custom_intent(&temp, &residency_scope_toml, "orders = \"warm\"");
            let result =
                resolve_deploy_policy(temp.path(), "ord", &DeployPolicyOverrides::default());

            if should_pass {
                let resolved = result.expect("resolve policy");
                assert_eq!(
                    resolved.intent.residency_scope,
                    expected_scope.expect("expected scope")
                );
            } else {
                let err = result.expect_err("must fail");
                assert!(
                    err.contains(expected_error.expect("expected error substring")),
                    "{err}"
                );
            }
        }
    }

    #[test]
    fn fuzz_namespace_policies_normalization_matrix() {
        let mut large_namespace_entries = Vec::new();
        let mut expected_large = BTreeMap::new();
        for idx in 0..96 {
            let key = format!(" ns-{idx:03} ");
            let policy = if idx % 3 == 0 {
                "hot_meta"
            } else if idx % 3 == 1 {
                "warm"
            } else {
                "cold_tierable"
            };
            large_namespace_entries.push(format!("\"{key}\" = \" {policy} \""));
            expected_large.insert(format!("ns-{idx:03}"), policy.to_string());
        }

        let cases = vec![
            (
                "\" Orders \" = \" warm \"\n\" billing-core \" = \" HOT_META \"".to_string(),
                true,
                Some(BTreeMap::from([
                    ("billing-core".to_string(), "hot_meta".to_string()),
                    ("orders".to_string(), "warm".to_string()),
                ])),
                None,
            ),
            (
                "\"ORDERS\" = \"warm\"\n\" orders \" = \"hot_meta\"".to_string(),
                false,
                None,
                Some(
                    "intent.namespace_policies includes duplicate namespace `orders` after normalization",
                ),
            ),
            (
                "\"orders_meta\" = \"warm\"".to_string(),
                false,
                None,
                Some("intent.namespace_policies includes invalid namespace `orders_meta`"),
            ),
            (
                "\"orders\" = \"   \"".to_string(),
                false,
                None,
                Some("intent.namespace_policies.orders must be non-empty"),
            ),
            (
                large_namespace_entries.join("\n"),
                true,
                Some(expected_large),
                None,
            ),
        ];

        for (namespace_policies_toml, should_pass, expected_policies, expected_error) in cases {
            let temp = tempfile::tempdir().expect("tempdir");
            write_policy_with_custom_intent(
                &temp,
                "[\"us-east\", \"eu-west\"]",
                &namespace_policies_toml,
            );
            let result =
                resolve_deploy_policy(temp.path(), "ord", &DeployPolicyOverrides::default());

            if should_pass {
                let resolved = result.expect("resolve policy");
                assert_eq!(
                    resolved.intent.namespace_policies,
                    expected_policies.expect("expected namespace map")
                );
            } else {
                let err = result.expect_err("must fail");
                assert!(
                    err.contains(expected_error.expect("expected error substring")),
                    "{err}"
                );
            }
        }
    }

    #[test]
    fn rejects_missing_intent_block() {
        let temp = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            temp.path().join("wrela.deploy.toml"),
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
        )
        .expect("write policy");

        let err = resolve_deploy_policy(temp.path(), "ord", &DeployPolicyOverrides::default())
            .expect_err("must fail");
        assert!(err.contains("intent block is required"), "{err}");
    }

    #[test]
    fn rejects_contradictory_intent_with_remediation() {
        let temp = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            temp.path().join("wrela.deploy.toml"),
            r#"
[topology]
mode = "collapsed"

[cluster]
target_voters = 1
replication_factor = 1
write_quorum = 1
logical_shards = 1
active_groups = 1

[replication]
async_failover = true

[regions]
ord = 1

[topology.region_node_map]
ord = ["n1"]

[sovereignty]
id = "tenant-prod"
allowed_regions = ["ord"]
enforce_all_copies = true

[intent]
policy_id = "orders-v3"
workload_class = "analytics_heavy"
latency_target_ms = 80
min_write_throughput_ops = 120000
residency_scope = ["us-east", "eu-west"]
cost_guardrail_mode = "estimate_and_warn_only"

[intent.namespace_policies]
orders = "warm"
"#,
        )
        .expect("write policy");

        let err = resolve_deploy_policy(temp.path(), "ord", &DeployPolicyOverrides::default())
            .expect_err("must fail");
        assert!(
            err.contains("intent contradiction `impossible_topology_for_high_throughput`"),
            "{err}"
        );
        assert!(err.contains("remediation hints"));
        assert!(err.contains("increase_nodes"));
    }

    #[test]
    fn resolves_collapsed_topology_and_normalizes_regions() {
        let temp = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            temp.path().join("wrela.deploy.toml"),
            with_required_intent(
                r#"
[topology]
mode = "collapsed"

[cluster]
target_voters = 3
replication_factor = 3
write_quorum = 2
logical_shards = 16
active_groups = 3

[replication]
async_failover = true

[regions]
ORD = 3

[topology.region_node_map]
ORD = ["node-a", "node-b", "node-c"]

[sovereignty]
id = "tenant-prod"
allowed_regions = ["ORD"]
enforce_all_copies = true
"#,
            ),
        )
        .expect("write policy");

        let resolved = resolve_deploy_policy(temp.path(), "ord", &DeployPolicyOverrides::default())
            .expect("resolve policy");
        assert_eq!(resolved.sovereignty_id, "tenant-prod");
        assert_eq!(
            resolved.sovereignty_allowed_regions,
            vec!["ord".to_string()]
        );
        assert!(resolved.sovereignty_enforce_all_copies);
        assert!(resolved.replication_async_failover);
        assert_eq!(resolved.topology_mode, TopologyMode::Collapsed);
        assert_eq!(
            resolved.region_node_map.get("ord"),
            Some(&vec![
                "node-a".to_string(),
                "node-b".to_string(),
                "node-c".to_string()
            ])
        );
        assert_eq!(
            resolved.region_az_node_map.get("ord"),
            Some(&BTreeMap::from([(
                "ord".to_string(),
                vec![
                    "node-a".to_string(),
                    "node-b".to_string(),
                    "node-c".to_string()
                ]
            )]))
        );
    }

    #[test]
    fn resolves_explicit_topology_mode() {
        let temp = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            temp.path().join("wrela.deploy.toml"),
            with_required_intent(
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
            ),
        )
        .expect("write policy");

        let resolved = resolve_deploy_policy(temp.path(), "ord", &DeployPolicyOverrides::default())
            .expect("resolve policy");
        assert_eq!(resolved.topology_mode, TopologyMode::Explicit);
        assert_eq!(
            resolved.region_node_map.get("ord"),
            Some(&vec!["n1".to_string(), "n2".to_string(), "n3".to_string()])
        );
        assert!(resolved.region_az_node_map.contains_key("ord"));
    }

    #[test]
    fn resolves_single_domain_topology_mode() {
        let temp = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            temp.path().join("wrela.deploy.toml"),
            with_required_intent(
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
            ),
        )
        .expect("write policy");

        let resolved =
            resolve_deploy_policy(temp.path(), "domain-a", &DeployPolicyOverrides::default())
                .expect("resolve policy");
        assert_eq!(resolved.topology_mode, TopologyMode::SingleDomain);
        assert_eq!(resolved.sovereignty_id, "domain-a");
        assert_eq!(
            resolved.sovereignty_allowed_regions,
            vec!["domain-a".to_string()]
        );
        assert_eq!(
            resolved.checkpoint_allowed_regions,
            vec!["domain-a".to_string()]
        );
        assert_eq!(
            resolved.region_az_node_map,
            BTreeMap::from([(
                "domain-a".to_string(),
                BTreeMap::from([(
                    "domain-a".to_string(),
                    vec!["n1".to_string(), "n2".to_string(), "n3".to_string()]
                )])
            )])
        );
    }

    #[test]
    fn rejects_non_majority_write_quorum() {
        let temp = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            temp.path().join("wrela.deploy.toml"),
            with_required_intent(
                r#"
[topology]
mode = "explicit"

[replication]
async_failover = true

[regions]
ord = 5

[topology.region_az_node_map.ord]
ord1 = ["n1", "n2", "n3", "n4", "n5"]

[sovereignty]
id = "tenant-prod"
allowed_regions = ["ord"]
enforce_all_copies = true
"#,
            ),
        )
        .expect("write policy");
        let err = resolve_deploy_policy(
            temp.path(),
            "ord",
            &DeployPolicyOverrides {
                machines: Some(5),
                replication_factor: Some(5),
                write_quorum: Some(2),
                ..DeployPolicyOverrides::default()
            },
        )
        .expect_err("must fail");
        assert!(err.contains("majority quorum"));
    }

    #[test]
    fn requires_per_region_checkpoint_maps_for_multi_region_s3() {
        let temp = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            temp.path().join("wrela.deploy.toml"),
            with_required_intent(
                r#"
[topology]
mode = "explicit"

[replication]
async_failover = true

[regions]
ord = 2
iad = 2

[topology.region_az_node_map.ord]
ord1 = ["ord-a", "ord-b"]

[topology.region_az_node_map.iad]
iad1 = ["iad-a", "iad-b"]

[sovereignty]
id = "tenant-prod"
allowed_regions = ["ord", "iad"]
enforce_all_copies = true

[checkpoint]
backend = "s3"
s3_bucket = "fallback"
s3_region = "us-east-1"
"#,
            ),
        )
        .expect("write policy");

        let err = resolve_deploy_policy(temp.path(), "ord", &DeployPolicyOverrides::default())
            .expect_err("must fail");
        assert!(err.contains("s3_bucket_by_region"));
    }

    #[test]
    fn rejects_missing_collapsed_region_node_map() {
        let temp = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            temp.path().join("wrela.deploy.toml"),
            with_required_intent(
                r#"
[topology]
mode = "collapsed"

[replication]
async_failover = true

[regions]
ord = 3

[sovereignty]
id = "tenant-prod"
allowed_regions = ["ord"]
enforce_all_copies = true
"#,
            ),
        )
        .expect("write policy");

        let err = resolve_deploy_policy(temp.path(), "ord", &DeployPolicyOverrides::default())
            .expect_err("must fail");
        assert!(err.contains("topology.region_node_map is required"));
    }

    #[test]
    fn rejects_missing_explicit_region_az_node_map() {
        let temp = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            temp.path().join("wrela.deploy.toml"),
            with_required_intent(
                r#"
[topology]
mode = "explicit"

[replication]
async_failover = true

[regions]
ord = 3

[sovereignty]
id = "tenant-prod"
allowed_regions = ["ord"]
enforce_all_copies = true
"#,
            ),
        )
        .expect("write policy");

        let err = resolve_deploy_policy(temp.path(), "ord", &DeployPolicyOverrides::default())
            .expect_err("must fail");
        assert!(err.contains("topology.region_az_node_map is required"));
    }

    #[test]
    fn rejects_missing_single_domain_payload() {
        let temp = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            temp.path().join("wrela.deploy.toml"),
            with_required_intent(
                r#"
[topology]
mode = "single_domain"

[replication]
async_failover = true

[regions]
ord = 3

[sovereignty]
id = "tenant-prod"
allowed_regions = ["ord"]
enforce_all_copies = true
"#,
            ),
        )
        .expect("write policy");

        let err = resolve_deploy_policy(temp.path(), "ord", &DeployPolicyOverrides::default())
            .expect_err("must fail");
        assert!(err.contains("topology.single_domain is required"));
    }

    #[test]
    fn rejects_unknown_topology_mode() {
        let temp = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            temp.path().join("wrela.deploy.toml"),
            with_required_intent(
                r#"
[topology]
mode = "hybrid"

[replication]
async_failover = true

[regions]
ord = 3

[sovereignty]
id = "tenant-prod"
allowed_regions = ["ord"]
enforce_all_copies = true
"#,
            ),
        )
        .expect("write policy");

        let err = resolve_deploy_policy(temp.path(), "ord", &DeployPolicyOverrides::default())
            .expect_err("must fail");
        assert!(err.contains("topology.mode is required"));
    }

    #[test]
    fn rejects_collapsed_mode_with_explicit_topology_payload() {
        let temp = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            temp.path().join("wrela.deploy.toml"),
            with_required_intent(
                r#"
[topology]
mode = "collapsed"

[replication]
async_failover = true

[regions]
ord = 3

[topology.region_node_map]
ord = ["n1", "n2", "n3"]

[topology.region_az_node_map.ord]
ord1 = ["n1", "n2", "n3"]

[sovereignty]
id = "tenant-prod"
allowed_regions = ["ord"]
enforce_all_copies = true
"#,
            ),
        )
        .expect("write policy");

        let err = resolve_deploy_policy(temp.path(), "ord", &DeployPolicyOverrides::default())
            .expect_err("must fail");
        assert!(err.contains("not allowed when topology.mode = collapsed"));
    }

    #[test]
    fn rejects_single_domain_with_non_domain_sovereignty_id() {
        let temp = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            temp.path().join("wrela.deploy.toml"),
            with_required_intent(
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
id = "tenant-prod"
allowed_regions = ["domain-a"]
enforce_all_copies = true
"#,
            ),
        )
        .expect("write policy");

        let err = resolve_deploy_policy(temp.path(), "domain-a", &DeployPolicyOverrides::default())
            .expect_err("must fail");
        assert!(err.contains("single_domain topology requires sovereignty.id"));
    }

    #[test]
    fn rejects_single_domain_with_non_domain_allowed_regions() {
        let temp = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            temp.path().join("wrela.deploy.toml"),
            with_required_intent(
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
allowed_regions = ["domain-a", "domain-b"]
enforce_all_copies = false
"#,
            ),
        )
        .expect("write policy");

        let err = resolve_deploy_policy(temp.path(), "domain-a", &DeployPolicyOverrides::default())
            .expect_err("must fail");
        assert!(err.contains(
            "single_domain topology requires sovereignty.allowed_regions = [\"domain-a\"]"
        ));
    }

    #[test]
    fn rejects_single_domain_enforce_all_copies_with_non_domain_checkpoint_regions() {
        let temp = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            temp.path().join("wrela.deploy.toml"),
            with_required_intent(
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
allowed_regions = ["domain-a", "domain-b"]
enforce_all_copies = true
"#,
            ),
        )
        .expect("write policy");

        let err = resolve_deploy_policy(temp.path(), "domain-a", &DeployPolicyOverrides::default())
            .expect_err("must fail");
        assert!(err.contains(
            "single_domain topology with sovereignty.enforce_all_copies=true requires checkpoint allowed regions = [\"domain-a\"]"
        ));
    }

    #[test]
    fn rejects_single_domain_regions_map_that_does_not_match_domain_id() {
        let temp = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            temp.path().join("wrela.deploy.toml"),
            with_required_intent(
                r#"
[topology]
mode = "single_domain"

[replication]
async_failover = true

[regions]
ord = 3

[topology.single_domain]
id = "domain-a"
nodes = ["n1", "n2", "n3"]

[sovereignty]
id = "domain-a"
allowed_regions = ["domain-a"]
enforce_all_copies = true
"#,
            ),
        )
        .expect("write policy");

        let err = resolve_deploy_policy(temp.path(), "ord", &DeployPolicyOverrides::default())
            .expect_err("must fail");
        assert!(
            err.contains("single_domain topology requires [regions] to contain exactly `domain-a`")
        );
    }

    #[test]
    fn rejects_topology_payload_without_mode_even_with_legacy_fields() {
        let temp = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            temp.path().join("wrela.deploy.toml"),
            with_required_intent(
                r#"
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
            ),
        )
        .expect("write policy");

        let err = resolve_deploy_policy(temp.path(), "ord", &DeployPolicyOverrides::default())
            .expect_err("must fail");
        assert!(err.contains("topology.mode is required"));
    }
}
