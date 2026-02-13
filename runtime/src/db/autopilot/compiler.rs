use std::collections::BTreeSet;

const FNV64_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV64_PRIME: u64 = 0x0000_0100_0000_01b3;
const EXPLAIN_SCHEMA_VERSION: u16 = 1;

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
