use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IslandLeaseV1 {
    pub island_id: u64,
    pub owner_node: String,
    pub epoch: u64,
    pub lease_until_ms: u64,
}

pub fn claim_lease(
    leases: &mut BTreeMap<u64, IslandLeaseV1>,
    island_id: u64,
    owner_node: &str,
    now_ms: u64,
    ttl_ms: u64,
) -> IslandLeaseV1 {
    let next_epoch = leases
        .get(&island_id)
        .map(|lease| lease.epoch + 1)
        .unwrap_or(1);
    let lease = IslandLeaseV1 {
        island_id,
        owner_node: owner_node.to_string(),
        epoch: next_epoch,
        lease_until_ms: now_ms.saturating_add(ttl_ms),
    };
    leases.insert(island_id, lease.clone());
    lease
}

pub fn handoff_lease(
    leases: &mut BTreeMap<u64, IslandLeaseV1>,
    island_id: u64,
    from_node: &str,
    to_node: &str,
    now_ms: u64,
    ttl_ms: u64,
) -> Result<IslandLeaseV1, String> {
    let Some(current) = leases.get(&island_id) else {
        return Err("lease missing".to_string());
    };
    if current.owner_node != from_node {
        return Err("lease owner mismatch".to_string());
    }
    Ok(claim_lease(leases, island_id, to_node, now_ms, ttl_ms))
}

#[cfg(test)]
mod tests {
    use super::{claim_lease, handoff_lease};
    use std::collections::BTreeMap;

    #[test]
    fn lease_handoff_increments_epoch() {
        let mut leases = BTreeMap::new();
        let first = claim_lease(&mut leases, 1, "node-a", 1000, 5000);
        let second = handoff_lease(&mut leases, 1, "node-a", "node-b", 2000, 5000)
            .expect("handoff should succeed");
        assert_eq!(first.epoch, 1);
        assert_eq!(second.epoch, 2);
        assert_eq!(second.owner_node, "node-b");
    }
}
