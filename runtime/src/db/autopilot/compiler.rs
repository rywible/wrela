use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

const FNV64_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV64_PRIME: u64 = 0x0000_0100_0000_01b3;
const EXPLAIN_SCHEMA_VERSION: u16 = 1;
const DB_INTENT_EXPLAIN_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurvivabilityIntent {
    SingleRegion,
    RegionFailure,
    TwoRegionFailure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CostTier {
    Economy,
    Balanced,
    Performance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetClass {
    Constrained,
    Standard,
    Flexible,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyIntentSpec {
    pub policy_id: String,
    pub survivability: SurvivabilityIntent,
    pub latency_target_ms: u64,
    pub residency_scope: Vec<String>,
    pub cost_tier: CostTier,
    pub budget_class: BudgetClass,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledPolicyIntent {
    pub policy_id: String,
    pub policy_hash: u64,
    pub survivability: SurvivabilityIntent,
    pub latency_target_ms: u64,
    pub residency_scope: Vec<String>,
    pub cost_tier: CostTier,
    pub budget_class: BudgetClass,
    pub explain: PolicyExplainMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyExplainMetadata {
    pub explain_schema_version: u16,
    pub hash_algorithm: &'static str,
    pub canonical_material: String,
    pub canonical_hash: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyCompilerError {
    EmptyPolicyId,
    ZeroLatencyTarget,
    EmptyResidencyScope,
    InvalidResidencyRegion(String),
    DuplicateResidencyRegion(String),
    Contradiction(PolicyContradiction),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyContradiction {
    pub code: PolicyContradictionCode,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyContradictionCode {
    SurvivabilityExceedsResidency,
    LatencyTooAggressiveForSurvivability,
    LatencyTooAggressiveForCostTier,
    BudgetConflictsWithCostTier,
}

pub fn compile_policy_intent(
    spec: &PolicyIntentSpec,
) -> Result<CompiledPolicyIntent, PolicyCompilerError> {
    let policy_id = spec.policy_id.trim();
    if policy_id.is_empty() {
        return Err(PolicyCompilerError::EmptyPolicyId);
    }
    if spec.latency_target_ms == 0 {
        return Err(PolicyCompilerError::ZeroLatencyTarget);
    }

    let residency_scope = normalize_residency_scope(&spec.residency_scope)?;
    validate_contradictions(
        spec.survivability,
        spec.latency_target_ms,
        &residency_scope,
        spec.cost_tier,
        spec.budget_class,
    )?;

    let canonical_material = canonical_material(
        policy_id,
        spec.survivability,
        spec.latency_target_ms,
        &residency_scope,
        spec.cost_tier,
        spec.budget_class,
    );
    let policy_hash = fnv64(canonical_material.as_bytes());

    Ok(CompiledPolicyIntent {
        policy_id: policy_id.to_string(),
        policy_hash,
        survivability: spec.survivability,
        latency_target_ms: spec.latency_target_ms,
        residency_scope,
        cost_tier: spec.cost_tier,
        budget_class: spec.budget_class,
        explain: PolicyExplainMetadata {
            explain_schema_version: EXPLAIN_SCHEMA_VERSION,
            hash_algorithm: "fnv64",
            canonical_material,
            canonical_hash: policy_hash,
        },
    })
}

fn normalize_residency_scope(scope: &[String]) -> Result<Vec<String>, PolicyCompilerError> {
    if scope.is_empty() {
        return Err(PolicyCompilerError::EmptyResidencyScope);
    }

    let mut seen = BTreeSet::new();
    let mut normalized = Vec::with_capacity(scope.len());
    for region in scope {
        let candidate = region.trim().to_ascii_lowercase();
        if candidate.is_empty() {
            return Err(PolicyCompilerError::InvalidResidencyRegion(region.clone()));
        }
        if !is_valid_region_code(&candidate) {
            return Err(PolicyCompilerError::InvalidResidencyRegion(region.clone()));
        }
        if !seen.insert(candidate.clone()) {
            return Err(PolicyCompilerError::DuplicateResidencyRegion(candidate));
        }
        normalized.push(candidate);
    }
    normalized.sort_unstable();
    Ok(normalized)
}

fn validate_contradictions(
    survivability: SurvivabilityIntent,
    latency_target_ms: u64,
    residency_scope: &[String],
    cost_tier: CostTier,
    budget_class: BudgetClass,
) -> Result<(), PolicyCompilerError> {
    let required_regions = match survivability {
        SurvivabilityIntent::SingleRegion => 1,
        SurvivabilityIntent::RegionFailure => 2,
        SurvivabilityIntent::TwoRegionFailure => 3,
    };
    if residency_scope.len() < required_regions {
        return Err(PolicyCompilerError::Contradiction(PolicyContradiction {
            code: PolicyContradictionCode::SurvivabilityExceedsResidency,
            reason: format!(
                "survivability intent {:?} requires at least {} residency regions, got {}",
                survivability,
                required_regions,
                residency_scope.len()
            ),
        }));
    }

    if latency_target_ms < 40 && survivability != SurvivabilityIntent::SingleRegion {
        return Err(PolicyCompilerError::Contradiction(PolicyContradiction {
            code: PolicyContradictionCode::LatencyTooAggressiveForSurvivability,
            reason: format!(
                "latency target {}ms is too aggressive for {:?}; require >= 40ms",
                latency_target_ms, survivability
            ),
        }));
    }

    if latency_target_ms < 20 && cost_tier == CostTier::Economy {
        return Err(PolicyCompilerError::Contradiction(PolicyContradiction {
            code: PolicyContradictionCode::LatencyTooAggressiveForCostTier,
            reason: format!(
                "latency target {}ms is incompatible with economy cost tier; require >= 20ms",
                latency_target_ms
            ),
        }));
    }

    if cost_tier == CostTier::Performance && budget_class == BudgetClass::Constrained {
        return Err(PolicyCompilerError::Contradiction(PolicyContradiction {
            code: PolicyContradictionCode::BudgetConflictsWithCostTier,
            reason: "performance cost tier cannot be paired with constrained budget class"
                .to_string(),
        }));
    }

    Ok(())
}

fn canonical_material(
    policy_id: &str,
    survivability: SurvivabilityIntent,
    latency_target_ms: u64,
    residency_scope: &[String],
    cost_tier: CostTier,
    budget_class: BudgetClass,
) -> String {
    format!(
        "v={}|policy_id={}|survivability={}|latency_target_ms={}|residency_scope={}|cost_tier={}|budget_class={}",
        EXPLAIN_SCHEMA_VERSION,
        policy_id,
        survivability.as_str(),
        latency_target_ms,
        residency_scope.join(","),
        cost_tier.as_str(),
        budget_class.as_str()
    )
}

fn is_valid_region_code(region: &str) -> bool {
    region
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn fnv64(bytes: &[u8]) -> u64 {
    let mut hash = FNV64_OFFSET_BASIS;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(FNV64_PRIME);
    }
    hash
}

impl SurvivabilityIntent {
    fn as_str(self) -> &'static str {
        match self {
            Self::SingleRegion => "single_region",
            Self::RegionFailure => "region_failure",
            Self::TwoRegionFailure => "two_region_failure",
        }
    }
}

impl CostTier {
    fn as_str(self) -> &'static str {
        match self {
            Self::Economy => "economy",
            Self::Balanced => "balanced",
            Self::Performance => "performance",
        }
    }
}

impl BudgetClass {
    fn as_str(self) -> &'static str {
        match self {
            Self::Constrained => "constrained",
            Self::Standard => "standard",
            Self::Flexible => "flexible",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbWorkloadClass {
    GeneralTransactional,
    AnalyticsHeavy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamespacePolicy {
    HotMeta,
    Warm,
    ColdTierable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CostGuardrailMode {
    EstimateAndWarnOnly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbIntentConfig {
    pub policy_id: String,
    pub workload_class: DbWorkloadClass,
    pub latency_target_ms: u64,
    pub min_write_throughput_ops: u64,
    pub residency_scope: Vec<String>,
    pub namespace_policies: BTreeMap<String, NamespacePolicy>,
    pub cost_guardrail_mode: CostGuardrailMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DbIntentTopologyHints {
    pub available_nodes: u32,
    pub logical_shards: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbIntentCompiled {
    pub policy_id: String,
    pub intent_hash: u64,
    pub workload_class: DbWorkloadClass,
    pub latency_target_ms: u64,
    pub min_write_throughput_ops: u64,
    pub residency_scope: Vec<String>,
    pub namespace_policies: BTreeMap<String, NamespacePolicy>,
    pub cost_guardrail_mode: CostGuardrailMode,
    pub topology_hints: DbIntentTopologyHints,
    pub explain: DbIntentExplainMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbIntentExplainMetadata {
    pub explain_schema_version: u16,
    pub hash_algorithm: &'static str,
    pub canonical_material: String,
    pub canonical_hash: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DbIntentCompilerError {
    EmptyPolicyId,
    EmptyNamespacePolicies,
    InvalidNamespace(String),
    InvalidResidencyRegion(String),
    DuplicateResidencyRegion(String),
    Contradiction(DbIntentContradiction),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbIntentContradiction {
    pub code: DbIntentContradictionCode,
    pub reason: String,
    pub remediations: Vec<DbIntentRemediation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbIntentContradictionCode {
    ImpossibleTopologyForHighThroughput,
    LatencyTargetInvalid,
    ResidencyScopeEmpty,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbIntentRemediation {
    pub action: DbIntentRemediationAction,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbIntentRemediationAction {
    IncreaseNodes,
    IncreaseShards,
    RelaxThroughputTarget,
    SetPositiveLatencyTarget,
    PopulateResidencyScope,
}

impl Default for DbIntentConfig {
    fn default() -> Self {
        Self {
            policy_id: "default-general-transactional".to_string(),
            workload_class: DbWorkloadClass::GeneralTransactional,
            latency_target_ms: 25,
            min_write_throughput_ops: 1_000,
            residency_scope: vec!["local".to_string()],
            namespace_policies: BTreeMap::from([
                ("core".to_string(), NamespacePolicy::Warm),
                ("meta".to_string(), NamespacePolicy::HotMeta),
            ]),
            cost_guardrail_mode: CostGuardrailMode::EstimateAndWarnOnly,
        }
    }
}

impl DbIntentConfig {
    pub fn from_json_str(raw: &str) -> Result<Self, String> {
        let parsed: Value =
            serde_json::from_str(raw).map_err(|err| format!("intent json must be valid: {err}"))?;
        let object = parsed
            .as_object()
            .ok_or_else(|| "intent json must be an object".to_string())?;

        let policy_id = parse_required_json_string(object, "policy_id")?;
        let workload_class =
            parse_db_workload_class(&parse_required_json_string(object, "workload_class")?)?;
        let latency_target_ms = parse_required_json_u64(object, "latency_target_ms")?;
        let min_write_throughput_ops = parse_required_json_u64(object, "min_write_throughput_ops")?;
        let residency_scope = parse_required_json_string_array(object, "residency_scope")?;
        let namespace_policies = parse_required_namespace_policy_map(object)?;
        let cost_guardrail_mode =
            parse_cost_guardrail_mode(&parse_required_json_string(object, "cost_guardrail_mode")?)?;

        Ok(Self {
            policy_id,
            workload_class,
            latency_target_ms,
            min_write_throughput_ops,
            residency_scope,
            namespace_policies,
            cost_guardrail_mode,
        })
    }
}

pub fn compile_db_intent(
    config: &DbIntentConfig,
    topology_hints: DbIntentTopologyHints,
) -> Result<DbIntentCompiled, DbIntentCompilerError> {
    let policy_id = config.policy_id.trim();
    if policy_id.is_empty() {
        return Err(DbIntentCompilerError::EmptyPolicyId);
    }

    if config.latency_target_ms == 0 {
        return Err(DbIntentCompilerError::Contradiction(
            DbIntentContradiction {
                code: DbIntentContradictionCode::LatencyTargetInvalid,
                reason: "latency_target_ms must be greater than 0".to_string(),
                remediations: vec![DbIntentRemediation {
                    action: DbIntentRemediationAction::SetPositiveLatencyTarget,
                    detail: "set latency_target_ms to at least 1".to_string(),
                }],
            },
        ));
    }
    if config.residency_scope.is_empty() {
        return Err(DbIntentCompilerError::Contradiction(
            DbIntentContradiction {
                code: DbIntentContradictionCode::ResidencyScopeEmpty,
                reason: "residency_scope must include at least one region".to_string(),
                remediations: vec![DbIntentRemediation {
                    action: DbIntentRemediationAction::PopulateResidencyScope,
                    detail: "add at least one normalized region code to residency_scope"
                        .to_string(),
                }],
            },
        ));
    }

    let residency_scope = normalize_db_residency_scope(&config.residency_scope)?;
    let namespace_policies = normalize_namespace_policies(&config.namespace_policies)?;
    validate_db_intent_contradictions(config.min_write_throughput_ops, topology_hints)?;

    let canonical_material = db_intent_canonical_material(
        policy_id,
        config.workload_class,
        config.latency_target_ms,
        config.min_write_throughput_ops,
        &residency_scope,
        &namespace_policies,
        config.cost_guardrail_mode,
        topology_hints,
    );
    let intent_hash = fnv64(canonical_material.as_bytes());

    Ok(DbIntentCompiled {
        policy_id: policy_id.to_string(),
        intent_hash,
        workload_class: config.workload_class,
        latency_target_ms: config.latency_target_ms,
        min_write_throughput_ops: config.min_write_throughput_ops,
        residency_scope,
        namespace_policies,
        cost_guardrail_mode: config.cost_guardrail_mode,
        topology_hints,
        explain: DbIntentExplainMetadata {
            explain_schema_version: DB_INTENT_EXPLAIN_SCHEMA_VERSION,
            hash_algorithm: "fnv64",
            canonical_material,
            canonical_hash: intent_hash,
        },
    })
}

fn normalize_namespace_policies(
    policies: &BTreeMap<String, NamespacePolicy>,
) -> Result<BTreeMap<String, NamespacePolicy>, DbIntentCompilerError> {
    if policies.is_empty() {
        return Err(DbIntentCompilerError::EmptyNamespacePolicies);
    }

    let mut out = BTreeMap::new();
    for (namespace, policy) in policies {
        let normalized = namespace.trim().to_ascii_lowercase();
        if normalized.is_empty() || !is_valid_namespace_key(&normalized) {
            return Err(DbIntentCompilerError::InvalidNamespace(namespace.clone()));
        }
        if out.insert(normalized.clone(), *policy).is_some() {
            return Err(DbIntentCompilerError::InvalidNamespace(normalized));
        }
    }
    Ok(out)
}

fn normalize_db_residency_scope(scope: &[String]) -> Result<Vec<String>, DbIntentCompilerError> {
    let mut seen = BTreeSet::new();
    let mut normalized = Vec::with_capacity(scope.len());
    for region in scope {
        let candidate = region.trim().to_ascii_lowercase();
        if candidate.is_empty() || !is_valid_region_code(&candidate) {
            return Err(DbIntentCompilerError::InvalidResidencyRegion(
                region.clone(),
            ));
        }
        if !seen.insert(candidate.clone()) {
            return Err(DbIntentCompilerError::DuplicateResidencyRegion(candidate));
        }
        normalized.push(candidate);
    }
    normalized.sort_unstable();
    Ok(normalized)
}

fn validate_db_intent_contradictions(
    min_write_throughput_ops: u64,
    topology_hints: DbIntentTopologyHints,
) -> Result<(), DbIntentCompilerError> {
    if min_write_throughput_ops < 10_000 {
        return Ok(());
    }

    let (min_nodes, min_shards) = if min_write_throughput_ops >= 50_000 {
        (3_u32, 4_u32)
    } else {
        (2_u32, 2_u32)
    };
    if topology_hints.available_nodes >= min_nodes && topology_hints.logical_shards >= min_shards {
        return Ok(());
    }

    let mut remediations = Vec::new();
    if topology_hints.available_nodes < min_nodes {
        remediations.push(DbIntentRemediation {
            action: DbIntentRemediationAction::IncreaseNodes,
            detail: format!("increase available_nodes to at least {min_nodes}"),
        });
    }
    if topology_hints.logical_shards < min_shards {
        remediations.push(DbIntentRemediation {
            action: DbIntentRemediationAction::IncreaseShards,
            detail: format!("increase logical_shards to at least {min_shards}"),
        });
    }
    remediations.push(DbIntentRemediation {
        action: DbIntentRemediationAction::RelaxThroughputTarget,
        detail: "lower min_write_throughput_ops to fit current topology capacity".to_string(),
    });

    Err(DbIntentCompilerError::Contradiction(
        DbIntentContradiction {
            code: DbIntentContradictionCode::ImpossibleTopologyForHighThroughput,
            reason: format!(
                "min_write_throughput_ops={} requires at least {} nodes and {} shards, got {} nodes and {} shards",
                min_write_throughput_ops,
                min_nodes,
                min_shards,
                topology_hints.available_nodes,
                topology_hints.logical_shards
            ),
            remediations,
        },
    ))
}

fn db_intent_canonical_material(
    policy_id: &str,
    workload_class: DbWorkloadClass,
    latency_target_ms: u64,
    min_write_throughput_ops: u64,
    residency_scope: &[String],
    namespace_policies: &BTreeMap<String, NamespacePolicy>,
    cost_guardrail_mode: CostGuardrailMode,
    topology_hints: DbIntentTopologyHints,
) -> String {
    let namespace_material = namespace_policies
        .iter()
        .map(|(namespace, policy)| format!("{namespace}:{}", policy.as_str()))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "v={}|policy_id={}|workload_class={}|latency_target_ms={}|min_write_throughput_ops={}|residency_scope={}|namespace_policies={}|cost_guardrail_mode={}|available_nodes={}|logical_shards={}",
        DB_INTENT_EXPLAIN_SCHEMA_VERSION,
        policy_id,
        workload_class.as_str(),
        latency_target_ms,
        min_write_throughput_ops,
        residency_scope.join(","),
        namespace_material,
        cost_guardrail_mode.as_str(),
        topology_hints.available_nodes,
        topology_hints.logical_shards
    )
}

fn parse_required_json_string(
    object: &serde_json::Map<String, Value>,
    field: &'static str,
) -> Result<String, String> {
    let value = object
        .get(field)
        .ok_or_else(|| format!("missing required field `{field}`"))?;
    let raw = value
        .as_str()
        .ok_or_else(|| format!("`{field}` must be a string"))?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(format!("`{field}` must be non-empty"));
    }
    Ok(trimmed.to_string())
}

fn parse_required_json_u64(
    object: &serde_json::Map<String, Value>,
    field: &'static str,
) -> Result<u64, String> {
    let value = object
        .get(field)
        .ok_or_else(|| format!("missing required field `{field}`"))?;
    value
        .as_u64()
        .ok_or_else(|| format!("`{field}` must be an unsigned integer"))
}

fn parse_required_json_string_array(
    object: &serde_json::Map<String, Value>,
    field: &'static str,
) -> Result<Vec<String>, String> {
    let value = object
        .get(field)
        .ok_or_else(|| format!("missing required field `{field}`"))?;
    let array = value
        .as_array()
        .ok_or_else(|| format!("`{field}` must be an array of strings"))?;
    let mut out = Vec::with_capacity(array.len());
    for item in array {
        let raw = item
            .as_str()
            .ok_or_else(|| format!("`{field}` entries must be strings"))?;
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(format!("`{field}` entries must be non-empty strings"));
        }
        out.push(trimmed.to_string());
    }
    Ok(out)
}

fn parse_required_namespace_policy_map(
    object: &serde_json::Map<String, Value>,
) -> Result<BTreeMap<String, NamespacePolicy>, String> {
    let value = object
        .get("namespace_policies")
        .ok_or_else(|| "missing required field `namespace_policies`".to_string())?;
    let map = value
        .as_object()
        .ok_or_else(|| "`namespace_policies` must be an object".to_string())?;
    if map.is_empty() {
        return Err("`namespace_policies` must include at least one namespace".to_string());
    }

    let mut out = BTreeMap::new();
    for (namespace, raw_policy) in map {
        let normalized_namespace = namespace.trim().to_ascii_lowercase();
        if normalized_namespace.is_empty() || !is_valid_namespace_key(&normalized_namespace) {
            return Err(format!(
                "`namespace_policies` includes invalid namespace `{namespace}`"
            ));
        }
        let policy_name = raw_policy.as_str().ok_or_else(|| {
            format!("`namespace_policies.{namespace}` must be a string enum value")
        })?;
        let policy = parse_namespace_policy(policy_name).ok_or_else(|| {
            format!("`namespace_policies.{namespace}` has unknown policy `{policy_name}`")
        })?;
        if out.insert(normalized_namespace.clone(), policy).is_some() {
            return Err(format!(
                "`namespace_policies` includes duplicate namespace `{normalized_namespace}` after normalization"
            ));
        }
    }
    Ok(out)
}

fn parse_db_workload_class(value: &str) -> Result<DbWorkloadClass, String> {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "general_transactional" => Ok(DbWorkloadClass::GeneralTransactional),
        "analytics_heavy" => Ok(DbWorkloadClass::AnalyticsHeavy),
        _ => Err(format!("unknown workload_class `{value}`")),
    }
}

fn parse_namespace_policy(value: &str) -> Option<NamespacePolicy> {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "hot_meta" => Some(NamespacePolicy::HotMeta),
        "warm" => Some(NamespacePolicy::Warm),
        "cold_tierable" => Some(NamespacePolicy::ColdTierable),
        _ => None,
    }
}

fn parse_cost_guardrail_mode(value: &str) -> Result<CostGuardrailMode, String> {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "estimate_and_warn_only" => Ok(CostGuardrailMode::EstimateAndWarnOnly),
        _ => Err(format!("unknown cost_guardrail_mode `{value}`")),
    }
}

fn is_valid_namespace_key(namespace: &str) -> bool {
    namespace
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

impl DbWorkloadClass {
    fn as_str(self) -> &'static str {
        match self {
            Self::GeneralTransactional => "general_transactional",
            Self::AnalyticsHeavy => "analytics_heavy",
        }
    }
}

impl NamespacePolicy {
    fn as_str(self) -> &'static str {
        match self {
            Self::HotMeta => "hot_meta",
            Self::Warm => "warm",
            Self::ColdTierable => "cold_tierable",
        }
    }
}

impl CostGuardrailMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::EstimateAndWarnOnly => "estimate_and_warn_only",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CostGuardrailMode, DbIntentCompilerError, DbIntentConfig, DbIntentContradictionCode,
        DbIntentTopologyHints, DbWorkloadClass, NamespacePolicy, compile_db_intent,
    };
    use std::collections::BTreeMap;

    fn baseline_intent() -> DbIntentConfig {
        DbIntentConfig {
            policy_id: "prod-intent".to_string(),
            workload_class: DbWorkloadClass::GeneralTransactional,
            latency_target_ms: 25,
            min_write_throughput_ops: 5_000,
            residency_scope: vec!["IAD".to_string(), "ord".to_string()],
            namespace_policies: BTreeMap::from([
                ("meta".to_string(), NamespacePolicy::HotMeta),
                ("core".to_string(), NamespacePolicy::Warm),
            ]),
            cost_guardrail_mode: CostGuardrailMode::EstimateAndWarnOnly,
        }
    }

    #[test]
    fn compile_db_intent_success_is_deterministic() {
        let intent = baseline_intent();
        let hints = DbIntentTopologyHints {
            available_nodes: 4,
            logical_shards: 8,
        };

        let first = compile_db_intent(&intent, hints).expect("compile first");
        let second = compile_db_intent(&intent, hints).expect("compile second");

        assert_eq!(first.intent_hash, second.intent_hash);
        assert_eq!(
            first.explain.canonical_material,
            second.explain.canonical_material
        );
        assert_eq!(
            first.residency_scope,
            vec!["iad".to_string(), "ord".to_string()]
        );
    }

    #[test]
    fn compile_db_intent_detects_impossible_topology_for_high_throughput() {
        let mut intent = baseline_intent();
        intent.min_write_throughput_ops = 120_000;
        let hints = DbIntentTopologyHints {
            available_nodes: 1,
            logical_shards: 1,
        };

        let err = compile_db_intent(&intent, hints).expect_err("must contradict");
        match err {
            DbIntentCompilerError::Contradiction(contradiction) => {
                assert_eq!(
                    contradiction.code,
                    DbIntentContradictionCode::ImpossibleTopologyForHighThroughput
                );
                assert!(
                    contradiction
                        .reason
                        .contains("requires at least 3 nodes and 4 shards")
                );
                assert!(!contradiction.remediations.is_empty());
            }
            other => panic!("expected contradiction, got {other:?}"),
        }
    }

    #[test]
    fn from_json_str_rejects_invalid_namespace_key() {
        let err = DbIntentConfig::from_json_str(
            r#"{
  "policy_id": "prod-intent",
  "workload_class": "general_transactional",
  "latency_target_ms": 25,
  "min_write_throughput_ops": 5000,
  "residency_scope": ["iad", "ord"],
  "namespace_policies": {
    "orders_meta": "warm"
  },
  "cost_guardrail_mode": "estimate_and_warn_only"
}"#,
        )
        .expect_err("must fail");

        assert!(
            err.contains("`namespace_policies` includes invalid namespace `orders_meta`"),
            "{err}"
        );
    }

    #[test]
    fn from_json_str_rejects_namespace_collision_after_normalization() {
        let err = DbIntentConfig::from_json_str(
            r#"{
  "policy_id": "prod-intent",
  "workload_class": "general_transactional",
  "latency_target_ms": 25,
  "min_write_throughput_ops": 5000,
  "residency_scope": ["iad", "ord"],
  "namespace_policies": {
    "ORDERS": "warm",
    "orders": "hot_meta"
  },
  "cost_guardrail_mode": "estimate_and_warn_only"
}"#,
        )
        .expect_err("must fail");

        assert!(
            err.contains(
                "`namespace_policies` includes duplicate namespace `orders` after normalization"
            ),
            "{err}"
        );
    }

    #[test]
    fn from_json_str_rejects_legacy_cost_guardrail_mode_aliases() {
        for alias in ["estimate_warn_only", "estimate+warn_only", "estimate+warn"] {
            let err = DbIntentConfig::from_json_str(&format!(
                r#"{{
  "policy_id": "prod-intent",
  "workload_class": "general_transactional",
  "latency_target_ms": 25,
  "min_write_throughput_ops": 5000,
  "residency_scope": ["iad", "ord"],
  "namespace_policies": {{
    "orders": "warm"
  }},
  "cost_guardrail_mode": "{alias}"
}}"#
            ))
            .expect_err("must fail");

            assert!(err.contains("unknown cost_guardrail_mode"), "{err}");
        }
    }

    fn json_with_intent_fields(
        residency_scope: &[String],
        namespace_policies: &[(String, String)],
    ) -> String {
        let residency_scope_json = residency_scope
            .iter()
            .map(|region| format!("\"{region}\""))
            .collect::<Vec<_>>()
            .join(", ");
        let namespace_json = namespace_policies
            .iter()
            .map(|(namespace, policy)| format!("\"{namespace}\": \"{policy}\""))
            .collect::<Vec<_>>()
            .join(", ");

        format!(
            r#"{{
  "policy_id": "prod-intent",
  "workload_class": "general_transactional",
  "latency_target_ms": 25,
  "min_write_throughput_ops": 5000,
  "residency_scope": [{residency_scope_json}],
  "namespace_policies": {{
    {namespace_json}
  }},
  "cost_guardrail_mode": "estimate_and_warn_only"
}}"#
        )
    }

    #[test]
    fn from_json_str_and_compile_residency_scope_normalization_fuzz_matrix() {
        let mut large_scope = Vec::new();
        for idx in 0..128 {
            large_scope.push(format!(" r-{idx:03} "));
        }

        let cases = vec![
            (
                vec![
                    "  us-east  ".to_string(),
                    "  EU-WEST  ".to_string(),
                    "ap-01".to_string(),
                ],
                true,
                Some(vec![
                    "ap-01".to_string(),
                    "eu-west".to_string(),
                    "us-east".to_string(),
                ]),
                None,
            ),
            (
                vec![" us-east ".to_string(), "US-EAST".to_string()],
                false,
                None,
                Some("DuplicateResidencyRegion(\"us-east\")"),
            ),
            (
                vec!["us-east".to_string(), "   ".to_string()],
                false,
                None,
                Some("`residency_scope` entries must be non-empty strings"),
            ),
            (
                vec!["us-east".to_string(), "eu_west".to_string()],
                false,
                None,
                Some("InvalidResidencyRegion(\"eu_west\")"),
            ),
            (
                large_scope,
                true,
                Some((0..128).map(|idx| format!("r-{idx:03}")).collect()),
                None,
            ),
        ];

        for (residency_scope, should_pass, expected_scope, expected_error) in cases {
            let raw = json_with_intent_fields(
                &residency_scope,
                &[("orders".to_string(), "warm".to_string())],
            );
            let parsed = DbIntentConfig::from_json_str(&raw);

            if should_pass {
                let config = parsed.expect("json parsing should succeed");
                let compiled = compile_db_intent(
                    &config,
                    DbIntentTopologyHints {
                        available_nodes: 4,
                        logical_shards: 8,
                    },
                )
                .expect("compile should pass");
                assert_eq!(
                    compiled.residency_scope,
                    expected_scope.expect("expected scope")
                );
            } else {
                let expected = expected_error.expect("expected error substring");
                match parsed {
                    Ok(config) => {
                        let err = compile_db_intent(
                            &config,
                            DbIntentTopologyHints {
                                available_nodes: 4,
                                logical_shards: 8,
                            },
                        )
                        .expect_err("compile should fail");
                        let rendered = format!("{err:?}");
                        assert!(rendered.contains(expected), "{rendered}");
                    }
                    Err(err) => {
                        assert!(err.contains(expected), "{err}");
                    }
                }
            }
        }
    }

    #[test]
    fn from_json_str_namespace_policies_normalization_fuzz_matrix() {
        let mut large_namespace_map = Vec::new();
        for idx in 0..96 {
            let policy = if idx % 3 == 0 {
                "hot_meta"
            } else if idx % 3 == 1 {
                "warm"
            } else {
                "cold_tierable"
            };
            large_namespace_map.push((format!(" ns-{idx:03} "), policy.to_string()));
        }

        let cases = vec![
            (
                vec![
                    (" Orders ".to_string(), " warm ".to_string()),
                    (" billing-core ".to_string(), " HOT_META ".to_string()),
                ],
                true,
                Some(BTreeMap::from([
                    ("billing-core".to_string(), NamespacePolicy::HotMeta),
                    ("orders".to_string(), NamespacePolicy::Warm),
                ])),
                None,
            ),
            (
                vec![
                    ("ORDERS".to_string(), "warm".to_string()),
                    (" orders ".to_string(), "hot_meta".to_string()),
                ],
                false,
                None,
                Some("duplicate namespace `orders` after normalization"),
            ),
            (
                vec![("orders_meta".to_string(), "warm".to_string())],
                false,
                None,
                Some("invalid namespace `orders_meta`"),
            ),
            (
                vec![("orders".to_string(), "   ".to_string())],
                false,
                None,
                Some("unknown policy"),
            ),
            (
                large_namespace_map,
                true,
                Some(
                    (0..96)
                        .map(|idx| {
                            let policy = if idx % 3 == 0 {
                                NamespacePolicy::HotMeta
                            } else if idx % 3 == 1 {
                                NamespacePolicy::Warm
                            } else {
                                NamespacePolicy::ColdTierable
                            };
                            (format!("ns-{idx:03}"), policy)
                        })
                        .collect(),
                ),
                None,
            ),
        ];

        for (namespace_policies, should_pass, expected_map, expected_error) in cases {
            let raw = json_with_intent_fields(
                &["iad".to_string(), "ord".to_string()],
                &namespace_policies,
            );
            let parsed = DbIntentConfig::from_json_str(&raw);
            if should_pass {
                let config = parsed.expect("json parsing should pass");
                assert_eq!(
                    config.namespace_policies,
                    expected_map.expect("expected normalized namespace map")
                );
            } else {
                let err = parsed.expect_err("json parsing should fail");
                assert!(
                    err.contains(expected_error.expect("expected error substring")),
                    "{err}"
                );
            }
        }
    }
}
