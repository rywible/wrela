use std::collections::{BTreeMap, HashSet};

pub mod health;
pub mod health_snapshot;
pub mod read_router;
pub mod telemetry;

const FNV64_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV64_PRIME: u64 = 0x0000_0100_0000_01b3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoutingPolicyError {
    EmptyShardKey,
    EmptyPolicyId,
    EmptyShardFieldName,
    DuplicateShardField(String),
    MissingSingleFieldWaiver,
    EmptySingleFieldWaiver,
    InvalidShardCount,
    MissingRouteField(String),
    EmptyRouteFieldValue(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutingPolicySpec {
    pub policy_id: String,
    pub shard_fields: Vec<String>,
    pub shard_count: usize,
    pub single_field_waiver_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledRoutingPolicy {
    pub policy_id: String,
    pub policy_hash: u64,
    shard_key_policy: ShardKeyPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShardKeyPolicy {
    shard_fields: Vec<String>,
    shard_count: usize,
    single_field_waiver_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShardRoute {
    pub shard_id: usize,
    pub shard_key: Vec<u8>,
}

impl ShardKeyPolicy {
    pub fn new(
        shard_fields: Vec<String>,
        shard_count: usize,
        single_field_waiver_reason: Option<String>,
    ) -> Result<Self, RoutingPolicyError> {
        if shard_fields.is_empty() {
            return Err(RoutingPolicyError::EmptyShardKey);
        }
        if shard_count == 0 {
            return Err(RoutingPolicyError::InvalidShardCount);
        }

        let mut seen = HashSet::new();
        for field in &shard_fields {
            if field.trim().is_empty() {
                return Err(RoutingPolicyError::EmptyShardFieldName);
            }
            if !seen.insert(field.clone()) {
                return Err(RoutingPolicyError::DuplicateShardField(field.clone()));
            }
        }

        if shard_fields.len() == 1 {
            match single_field_waiver_reason {
                None => return Err(RoutingPolicyError::MissingSingleFieldWaiver),
                Some(ref reason) if reason.trim().is_empty() => {
                    return Err(RoutingPolicyError::EmptySingleFieldWaiver);
                }
                Some(_) => {}
            }
        }

        Ok(Self {
            shard_fields,
            shard_count,
            single_field_waiver_reason,
        })
    }

    pub fn route_row(
        &self,
        row: &BTreeMap<String, Vec<u8>>,
    ) -> Result<ShardRoute, RoutingPolicyError> {
        let mut shard_key = Vec::new();
        for field in &self.shard_fields {
            let value = row
                .get(field)
                .ok_or_else(|| RoutingPolicyError::MissingRouteField(field.clone()))?;
            if value.is_empty() {
                return Err(RoutingPolicyError::EmptyRouteFieldValue(field.clone()));
            }
            shard_key.extend_from_slice(&(value.len() as u32).to_be_bytes());
            shard_key.extend_from_slice(value);
        }

        let hash = fnv64(&shard_key);
        Ok(ShardRoute {
            shard_id: (hash as usize) % self.shard_count,
            shard_key,
        })
    }

    pub fn shard_fields(&self) -> &[String] {
        &self.shard_fields
    }

    pub fn shard_count(&self) -> usize {
        self.shard_count
    }

    pub fn single_field_waiver_reason(&self) -> Option<&str> {
        self.single_field_waiver_reason.as_deref()
    }
}

impl CompiledRoutingPolicy {
    pub fn route_row(
        &self,
        row: &BTreeMap<String, Vec<u8>>,
    ) -> Result<ShardRoute, RoutingPolicyError> {
        self.shard_key_policy.route_row(row)
    }

    pub fn shard_key_policy(&self) -> &ShardKeyPolicy {
        &self.shard_key_policy
    }
}

pub fn compile_policy(
    spec: &RoutingPolicySpec,
) -> Result<CompiledRoutingPolicy, RoutingPolicyError> {
    let policy_id = spec.policy_id.trim();
    if policy_id.is_empty() {
        return Err(RoutingPolicyError::EmptyPolicyId);
    }

    let shard_fields: Vec<String> = spec
        .shard_fields
        .iter()
        .map(|field| field.trim().to_string())
        .collect();
    let waiver_reason = spec
        .single_field_waiver_reason
        .as_ref()
        .map(|reason| reason.trim().to_string());
    let shard_key_policy = ShardKeyPolicy::new(shard_fields, spec.shard_count, waiver_reason)?;

    let policy_hash = compile_policy_hash(policy_id, &shard_key_policy);
    Ok(CompiledRoutingPolicy {
        policy_id: policy_id.to_string(),
        policy_hash,
        shard_key_policy,
    })
}

fn compile_policy_hash(policy_id: &str, policy: &ShardKeyPolicy) -> u64 {
    let mut key = Vec::new();
    key.extend_from_slice(&(policy_id.len() as u32).to_be_bytes());
    key.extend_from_slice(policy_id.as_bytes());
    key.extend_from_slice(&(policy.shard_count as u64).to_be_bytes());
    for field in policy.shard_fields() {
        key.extend_from_slice(&(field.len() as u32).to_be_bytes());
        key.extend_from_slice(field.as_bytes());
    }
    match policy.single_field_waiver_reason() {
        Some(reason) => {
            key.push(1);
            key.extend_from_slice(&(reason.len() as u32).to_be_bytes());
            key.extend_from_slice(reason.as_bytes());
        }
        None => key.push(0),
    }
    fnv64(&key)
}

fn fnv64(bytes: &[u8]) -> u64 {
    let mut hash = FNV64_OFFSET_BASIS;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(FNV64_PRIME);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::{RoutingPolicyError, RoutingPolicySpec, ShardKeyPolicy, compile_policy};
    use std::collections::BTreeMap;

    #[test]
    fn rejects_single_field_policy_without_waiver() {
        let err = ShardKeyPolicy::new(vec!["tenant_id".to_string()], 16, None).expect_err("deny");
        assert_eq!(err, RoutingPolicyError::MissingSingleFieldWaiver);
    }

    #[test]
    fn deterministic_routing_for_same_row() {
        let policy = ShardKeyPolicy::new(
            vec!["tenant_id".to_string(), "order_id".to_string()],
            32,
            None,
        )
        .expect("policy");

        let mut row = BTreeMap::new();
        row.insert("tenant_id".to_string(), b"t-9".to_vec());
        row.insert("order_id".to_string(), b"o-200".to_vec());

        let route_a = policy.route_row(&row).expect("route");
        let route_b = policy.route_row(&row).expect("route");
        assert_eq!(route_a, route_b);
    }

    #[test]
    fn compile_policy_trims_inputs_and_is_deterministic() {
        let spec = RoutingPolicySpec {
            policy_id: "  orders-by-tenant  ".to_string(),
            shard_fields: vec![" tenant_id ".to_string(), " order_id ".to_string()],
            shard_count: 32,
            single_field_waiver_reason: None,
        };

        let compiled_a = compile_policy(&spec).expect("compile");
        let compiled_b = compile_policy(&spec).expect("compile");
        assert_eq!(compiled_a.policy_id, "orders-by-tenant");
        assert_eq!(compiled_a, compiled_b);
    }

    #[test]
    fn compile_policy_hash_changes_when_policy_shape_changes() {
        let baseline = compile_policy(&RoutingPolicySpec {
            policy_id: "orders".to_string(),
            shard_fields: vec!["tenant_id".to_string(), "order_id".to_string()],
            shard_count: 32,
            single_field_waiver_reason: None,
        })
        .expect("compile");

        let changed = compile_policy(&RoutingPolicySpec {
            policy_id: "orders".to_string(),
            shard_fields: vec!["tenant_id".to_string(), "order_id".to_string()],
            shard_count: 64,
            single_field_waiver_reason: None,
        })
        .expect("compile");

        assert_ne!(baseline.policy_hash, changed.policy_hash);
    }
}
