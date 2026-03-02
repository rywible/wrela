use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HierarchyError {
    EmptySovereigntyId,
    EmptyRegionId,
    EmptyAzId,
    EmptyNodeId,
    DuplicateRegion(String),
    DuplicateAz(String),
    DuplicateNode(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sovereignty {
    pub sovereignty_id: String,
    pub regions: Vec<Region>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Region {
    pub region_id: String,
    pub azs: Vec<AZ>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AZ {
    pub az_id: String,
    pub nodes: Vec<Node>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Node {
    pub node_id: String,
}

impl Sovereignty {
    pub fn new(
        sovereignty_id: impl AsRef<str>,
        mut regions: Vec<Region>,
    ) -> Result<Self, HierarchyError> {
        let sovereignty_id = normalize_id(sovereignty_id.as_ref());
        if sovereignty_id.is_empty() {
            return Err(HierarchyError::EmptySovereigntyId);
        }

        let mut seen_regions = BTreeSet::new();
        for region in &regions {
            if !seen_regions.insert(region.region_id.clone()) {
                return Err(HierarchyError::DuplicateRegion(region.region_id.clone()));
            }
        }
        regions.sort_by(|a, b| a.region_id.cmp(&b.region_id));

        Ok(Self {
            sovereignty_id,
            regions,
        })
    }

    pub fn resolve_region(&self, region_id: &str) -> Option<&Region> {
        let canonical = normalize_id(region_id);
        if canonical.is_empty() {
            return self.regions.first();
        }
        self.regions
            .iter()
            .find(|region| region.region_id == canonical)
    }

    pub fn has_region(&self, region_id: &str) -> bool {
        let canonical = normalize_id(region_id);
        self.regions
            .iter()
            .any(|region| region.region_id == canonical)
    }

    pub fn region_ids(&self) -> Vec<String> {
        self.regions
            .iter()
            .map(|region| region.region_id.clone())
            .collect()
    }
}

impl Region {
    pub fn new(region_id: impl AsRef<str>, mut azs: Vec<AZ>) -> Result<Self, HierarchyError> {
        let region_id = normalize_id(region_id.as_ref());
        if region_id.is_empty() {
            return Err(HierarchyError::EmptyRegionId);
        }

        let mut seen_azs = BTreeSet::new();
        for az in &azs {
            if !seen_azs.insert(az.az_id.clone()) {
                return Err(HierarchyError::DuplicateAz(az.az_id.clone()));
            }
        }
        azs.sort_by(|a, b| a.az_id.cmp(&b.az_id));

        Ok(Self { region_id, azs })
    }
}

impl AZ {
    pub fn new(az_id: impl AsRef<str>, mut nodes: Vec<Node>) -> Result<Self, HierarchyError> {
        let az_id = normalize_id(az_id.as_ref());
        if az_id.is_empty() {
            return Err(HierarchyError::EmptyAzId);
        }

        let mut seen_nodes = BTreeSet::new();
        for node in &nodes {
            if !seen_nodes.insert(node.node_id.clone()) {
                return Err(HierarchyError::DuplicateNode(node.node_id.clone()));
            }
        }
        nodes.sort_by(|a, b| a.node_id.cmp(&b.node_id));

        Ok(Self { az_id, nodes })
    }
}

impl Node {
    pub fn new(node_id: impl AsRef<str>) -> Result<Self, HierarchyError> {
        let node_id = normalize_id(node_id.as_ref());
        if node_id.is_empty() {
            return Err(HierarchyError::EmptyNodeId);
        }
        Ok(Self { node_id })
    }
}

fn normalize_id(input: &str) -> String {
    input.trim().to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::{AZ, HierarchyError, Node, Region, Sovereignty};

    #[test]
    fn hierarchy_normalizes_and_sorts_ids() {
        let sovereignty = Sovereignty::new(
            "  Core-NA  ",
            vec![
                Region::new(
                    " Us-West ",
                    vec![
                        AZ::new(
                            " AZ-2 ",
                            vec![
                                Node::new("n-2").expect("node"),
                                Node::new("n-1").expect("node"),
                            ],
                        )
                        .expect("az"),
                    ],
                )
                .expect("region"),
                Region::new(
                    " us-east ",
                    vec![AZ::new("az-1", vec![Node::new("n-9").expect("node")]).expect("az")],
                )
                .expect("region"),
            ],
        )
        .expect("sovereignty");

        assert_eq!(sovereignty.sovereignty_id, "core-na");
        assert_eq!(sovereignty.region_ids(), vec!["us-east", "us-west"]);
        assert_eq!(sovereignty.regions[1].azs[0].nodes[0].node_id, "n-1");
    }

    #[test]
    fn hierarchy_rejects_duplicate_ids() {
        let duplicate_region = Sovereignty::new(
            "core",
            vec![
                Region::new(
                    "us",
                    vec![AZ::new("az-1", vec![Node::new("n-1").expect("node")]).expect("az")],
                )
                .expect("region"),
                Region::new(
                    "US",
                    vec![AZ::new("az-2", vec![Node::new("n-2").expect("node")]).expect("az")],
                )
                .expect("region"),
            ],
        )
        .expect_err("must reject duplicate region");
        assert_eq!(
            duplicate_region,
            HierarchyError::DuplicateRegion("us".to_string())
        );
    }

    #[test]
    fn resolve_region_returns_primary_when_unspecified() {
        let sovereignty = Sovereignty::new(
            "core",
            vec![
                Region::new(
                    "us",
                    vec![AZ::new("az-1", vec![Node::new("n-1").expect("node")]).expect("az")],
                )
                .expect("region"),
            ],
        )
        .expect("sovereignty");
        assert_eq!(
            sovereignty.resolve_region("").expect("default").region_id,
            "us"
        );
    }
}
