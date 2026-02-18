use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailureDomain {
    pub region: String,
    pub zone: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegionTopology {
    pub region: String,
    pub zones: Vec<String>,
}

pub fn build_region_topology(domains: &[FailureDomain]) -> Vec<RegionTopology> {
    let mut grouped: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for domain in domains {
        grouped
            .entry(domain.region.clone())
            .or_default()
            .push(domain.zone.clone());
    }
    let mut topologies = Vec::with_capacity(grouped.len());
    for (region, mut zones) in grouped {
        zones.sort();
        zones.dedup();
        topologies.push(RegionTopology { region, zones });
    }
    topologies
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_deterministic_region_topology() {
        let domains = vec![
            FailureDomain {
                region: "us-east".to_string(),
                zone: "b".to_string(),
            },
            FailureDomain {
                region: "us-east".to_string(),
                zone: "a".to_string(),
            },
            FailureDomain {
                region: "eu-west".to_string(),
                zone: "a".to_string(),
            },
        ];

        assert_eq!(
            build_region_topology(&domains),
            vec![
                RegionTopology {
                    region: "eu-west".to_string(),
                    zones: vec!["a".to_string()],
                },
                RegionTopology {
                    region: "us-east".to_string(),
                    zones: vec!["a".to_string(), "b".to_string()],
                }
            ]
        );
    }
}
