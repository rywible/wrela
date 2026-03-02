use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SchemaEpoch(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaEpochRange {
    pub min_supported: SchemaEpoch,
    pub max_supported: SchemaEpoch,
}

impl SchemaEpochRange {
    pub fn supports(&self, epoch: SchemaEpoch) -> bool {
        epoch >= self.min_supported && epoch <= self.max_supported
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SchemaCompatibilityMode {
    ExpandContract,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SchemaGateDecision {
    Allow,
    Deny { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaGateInput {
    pub mode: SchemaCompatibilityMode,
    pub committed_epoch: SchemaEpoch,
    pub target_write_epoch: SchemaEpoch,
    pub voter_ranges: Vec<SchemaEpochRange>,
    pub all_voters_on_target_binary: bool,
}

pub fn evaluate_schema_gate(input: &SchemaGateInput) -> SchemaGateDecision {
    match input.mode {
        SchemaCompatibilityMode::ExpandContract => {
            if input.target_write_epoch <= input.committed_epoch {
                return SchemaGateDecision::Allow;
            }

            if !input.all_voters_on_target_binary {
                return SchemaGateDecision::Deny {
                    reason: "SCHEMA_EPOCH_MIXED_BINARY: target epoch requires all voters upgraded"
                        .to_string(),
                };
            }

            if input
                .voter_ranges
                .iter()
                .all(|range| range.supports(input.target_write_epoch))
            {
                SchemaGateDecision::Allow
            } else {
                SchemaGateDecision::Deny {
                    reason: "SCHEMA_EPOCH_UNSUPPORTED_BY_VOTER".to_string(),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_contract_denies_new_epoch_until_all_voters_upgraded() {
        let decision = evaluate_schema_gate(&SchemaGateInput {
            mode: SchemaCompatibilityMode::ExpandContract,
            committed_epoch: SchemaEpoch(1),
            target_write_epoch: SchemaEpoch(2),
            voter_ranges: vec![SchemaEpochRange {
                min_supported: SchemaEpoch(1),
                max_supported: SchemaEpoch(2),
            }],
            all_voters_on_target_binary: false,
        });
        assert!(matches!(decision, SchemaGateDecision::Deny { .. }));
    }

    #[test]
    fn expand_contract_allows_when_voters_support_target_and_binary_converged() {
        let decision = evaluate_schema_gate(&SchemaGateInput {
            mode: SchemaCompatibilityMode::ExpandContract,
            committed_epoch: SchemaEpoch(1),
            target_write_epoch: SchemaEpoch(2),
            voter_ranges: vec![
                SchemaEpochRange {
                    min_supported: SchemaEpoch(1),
                    max_supported: SchemaEpoch(2),
                },
                SchemaEpochRange {
                    min_supported: SchemaEpoch(1),
                    max_supported: SchemaEpoch(3),
                },
            ],
            all_voters_on_target_binary: true,
        });
        assert_eq!(decision, SchemaGateDecision::Allow);
    }
}
