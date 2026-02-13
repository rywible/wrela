use crate::db::security::residency::{ResidencyErrorToken, ResidencyPolicy};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CostGuardrailInput {
    pub estimated_monthly_cost_cents: u64,
    pub max_monthly_budget_cents: u64,
    pub hard_stop_ratio_bps: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CostGuardrailAction {
    Allow,
    ReduceFanout,
    FreezeChanges,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CostGuardrailDecision {
    pub action: CostGuardrailAction,
    pub budget_utilization_bps: u32,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResidencyProofRow {
    pub shard: Vec<u8>,
    pub target_region: String,
    pub allowed: bool,
    pub token: Option<&'static str>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComplianceProof {
    pub policy_id: String,
    pub all_allowed: bool,
    pub rows: Vec<ResidencyProofRow>,
}

pub fn evaluate_cost_guardrail(input: CostGuardrailInput) -> CostGuardrailDecision {
    if input.max_monthly_budget_cents == 0 {
        return CostGuardrailDecision {
            action: CostGuardrailAction::FreezeChanges,
            budget_utilization_bps: u32::MAX,
            reason: "budget is zero; fail closed".to_string(),
        };
    }

    let utilization_bps = ((input.estimated_monthly_cost_cents as u128)
        .saturating_mul(10_000)
        .checked_div(input.max_monthly_budget_cents as u128)
        .unwrap_or(u128::MAX)) as u32;

    if utilization_bps >= input.hard_stop_ratio_bps {
        return CostGuardrailDecision {
            action: CostGuardrailAction::FreezeChanges,
            budget_utilization_bps: utilization_bps,
            reason: format!(
                "budget utilization {} bps exceeds hard stop {} bps",
                utilization_bps, input.hard_stop_ratio_bps
            ),
        };
    }

    if utilization_bps >= 9_000 {
        return CostGuardrailDecision {
            action: CostGuardrailAction::ReduceFanout,
            budget_utilization_bps: utilization_bps,
            reason: format!(
                "budget utilization {} bps exceeds 9000 bps",
                utilization_bps
            ),
        };
    }

    CostGuardrailDecision {
        action: CostGuardrailAction::Allow,
        budget_utilization_bps: utilization_bps,
        reason: "budget healthy".to_string(),
    }
}

pub fn build_residency_compliance_proof(
    policy_id: &str,
    policy: &ResidencyPolicy,
    placements: &[(Vec<u8>, String)],
) -> ComplianceProof {
    let mut rows = Vec::new();
    for (shard, region) in placements {
        match policy.authorize_egress(shard, region) {
            Ok(()) => rows.push(ResidencyProofRow {
                shard: shard.clone(),
                target_region: region.clone(),
                allowed: true,
                token: None,
                reason: "allowed".to_string(),
            }),
            Err(err) => rows.push(ResidencyProofRow {
                shard: shard.clone(),
                target_region: region.clone(),
                allowed: false,
                token: Some(match err.token {
                    ResidencyErrorToken::EgressDeny => ResidencyErrorToken::EgressDeny.as_str(),
                    ResidencyErrorToken::EgressPolicyUnsat => {
                        ResidencyErrorToken::EgressPolicyUnsat.as_str()
                    }
                }),
                reason: err.reason,
            }),
        }
    }

    let all_allowed = rows.iter().all(|row| row.allowed);
    ComplianceProof {
        policy_id: policy_id.to_string(),
        all_allowed,
        rows,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::security::residency::ResidencyRule;

    #[test]
    fn cost_guardrail_freezes_when_budget_exceeds_hard_stop() {
        let decision = evaluate_cost_guardrail(CostGuardrailInput {
            estimated_monthly_cost_cents: 15_000,
            max_monthly_budget_cents: 10_000,
            hard_stop_ratio_bps: 12_000,
        });
        assert_eq!(decision.action, CostGuardrailAction::FreezeChanges);
    }

    #[test]
    fn residency_compliance_proof_fails_closed_for_unsat_and_deny() {
        let policy = ResidencyPolicy::with_rules(vec![ResidencyRule {
            shard: b"orders".to_vec(),
            allowed_regions: vec!["us".to_string()],
        }]);

        let proof = build_residency_compliance_proof(
            "orders-policy-v1",
            &policy,
            &[
                (b"orders".to_vec(), "eu".to_string()),
                (b"billing".to_vec(), "us".to_string()),
            ],
        );

        assert!(!proof.all_allowed);
        assert_eq!(proof.rows.len(), 2);
        assert_eq!(
            proof.rows[0].token,
            Some(ResidencyErrorToken::EgressDeny.as_str())
        );
        assert_eq!(
            proof.rows[1].token,
            Some(ResidencyErrorToken::EgressPolicyUnsat.as_str())
        );
    }
}
