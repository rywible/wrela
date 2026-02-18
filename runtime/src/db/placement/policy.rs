use crate::db::placement::failure_domains::{FailureDomain, build_region_topology};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlacementProfile {
    ThreeRegionSurvivability,
    SingleRegion,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplicaPlacement {
    pub region: String,
    pub zone: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlacementPlan {
    pub profile: PlacementProfile,
    pub replicas: Vec<ReplicaPlacement>,
    pub commit_quorum: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlacementPolicyError {
    EmptyFailureDomains,
    InsufficientRegionsForSurvivability { required: usize, available: usize },
    RegionUnavailable(String),
}

pub fn plan_placement(
    profile: PlacementProfile,
    domains: &[FailureDomain],
    preferred_region: Option<&str>,
) -> Result<PlacementPlan, PlacementPolicyError> {
    if domains.is_empty() {
        return Err(PlacementPolicyError::EmptyFailureDomains);
    }

    let topology = build_region_topology(domains);
    match profile {
        PlacementProfile::ThreeRegionSurvivability => {
            if topology.len() < 3 {
                return Err(PlacementPolicyError::InsufficientRegionsForSurvivability {
                    required: 3,
                    available: topology.len(),
                });
            }
            let replicas = topology
                .iter()
                .take(3)
                .map(|region| ReplicaPlacement {
                    region: region.region.clone(),
                    zone: region
                        .zones
                        .first()
                        .cloned()
                        .unwrap_or_else(|| "a".to_string()),
                })
                .collect();

            Ok(PlacementPlan {
                profile,
                replicas,
                commit_quorum: 2,
            })
        }
        PlacementProfile::SingleRegion => {
            let chosen = preferred_region.unwrap_or(&topology[0].region);
            let region = topology
                .iter()
                .find(|entry| entry.region == chosen)
                .ok_or_else(|| PlacementPolicyError::RegionUnavailable(chosen.to_string()))?;
            let mut zones = region.zones.clone();
            if zones.is_empty() {
                zones.push("a".to_string());
            }
            while zones.len() < 3 {
                zones.push(format!("{}-{}", region.region, zones.len() + 1));
            }
            let replicas = zones
                .into_iter()
                .take(3)
                .map(|zone| ReplicaPlacement {
                    region: region.region.clone(),
                    zone,
                })
                .collect();
            Ok(PlacementPlan {
                profile,
                replicas,
                commit_quorum: 2,
            })
        }
    }
}

pub fn survives_region_loss(plan: &PlacementPlan, failed_region: &str) -> bool {
    let survivors = plan
        .replicas
        .iter()
        .filter(|replica| replica.region != failed_region)
        .count();
    survivors >= plan.commit_quorum
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_domains() -> Vec<FailureDomain> {
        vec![
            FailureDomain {
                region: "us-central".to_string(),
                zone: "a".to_string(),
            },
            FailureDomain {
                region: "eu-west".to_string(),
                zone: "a".to_string(),
            },
            FailureDomain {
                region: "ap-south".to_string(),
                zone: "a".to_string(),
            },
        ]
    }

    #[test]
    fn three_region_plan_is_deterministic_and_survivable() {
        let plan = plan_placement(
            PlacementProfile::ThreeRegionSurvivability,
            &sample_domains(),
            None,
        )
        .expect("plan");
        assert_eq!(plan.replicas.len(), 3);
        assert_eq!(plan.commit_quorum, 2);
        assert!(survives_region_loss(&plan, "eu-west"));
    }

    #[test]
    fn single_region_requires_known_region() {
        let err = plan_placement(
            PlacementProfile::SingleRegion,
            &sample_domains(),
            Some("missing"),
        )
        .expect_err("must fail");
        assert_eq!(
            err,
            PlacementPolicyError::RegionUnavailable("missing".to_string())
        );
    }
}
