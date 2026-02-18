#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResidencyRule {
    pub shard: Vec<u8>,
    pub allowed_regions: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResidencyErrorToken {
    EgressDeny,
    EgressPolicyUnsat,
}

impl ResidencyErrorToken {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EgressDeny => "RESIDENCY_EGRESS_DENY",
            Self::EgressPolicyUnsat => "RESIDENCY_EGRESS_POLICY_UNSAT",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResidencyError {
    pub token: ResidencyErrorToken,
    pub reason: String,
}

impl ResidencyError {
    pub fn fail_closed_message(&self) -> String {
        format!("{}: {}", self.token.as_str(), self.reason)
    }
}

#[derive(Debug, Clone, Default)]
pub struct ResidencyPolicy {
    rules: Vec<ResidencyRule>,
}

impl ResidencyPolicy {
    pub fn with_rules(rules: Vec<ResidencyRule>) -> Self {
        let normalized = rules
            .into_iter()
            .map(|rule| {
                let mut allowed_regions = rule
                    .allowed_regions
                    .into_iter()
                    .map(|region| normalize_region(&region))
                    .filter(|region| !region.is_empty())
                    .collect::<Vec<_>>();
                allowed_regions.sort();
                allowed_regions.dedup();
                ResidencyRule {
                    shard: rule.shard,
                    allowed_regions,
                }
            })
            .collect();
        Self { rules: normalized }
    }

    pub fn authorize_egress(&self, shard: &[u8], sink_region: &str) -> Result<(), ResidencyError> {
        let normalized_sink_region = normalize_region(sink_region);
        let Some(rule) = self
            .rules
            .iter()
            .find(|rule| rule.shard.as_slice() == shard)
        else {
            return Err(ResidencyError {
                token: ResidencyErrorToken::EgressPolicyUnsat,
                reason: format!(
                    "shard={} has no egress rule",
                    String::from_utf8_lossy(shard)
                ),
            });
        };
        if rule
            .allowed_regions
            .iter()
            .any(|region| region == &normalized_sink_region)
        {
            return Ok(());
        }
        Err(ResidencyError {
            token: ResidencyErrorToken::EgressDeny,
            reason: format!(
                "shard={} sink_region={} allowed={:?}",
                String::from_utf8_lossy(shard),
                normalized_sink_region,
                rule.allowed_regions
            ),
        })
    }
}

fn normalize_region(region: &str) -> String {
    region.trim().to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_configured_region_for_shard() {
        let policy = ResidencyPolicy::with_rules(vec![ResidencyRule {
            shard: b"core".to_vec(),
            allowed_regions: vec!["us".to_string()],
        }]);
        assert!(policy.authorize_egress(b"core", "us").is_ok());
    }

    #[test]
    fn denies_unconfigured_region_with_typed_token() {
        let policy = ResidencyPolicy::with_rules(vec![ResidencyRule {
            shard: b"core".to_vec(),
            allowed_regions: vec!["us".to_string()],
        }]);
        let err = policy
            .authorize_egress(b"core", "eu")
            .expect_err("region must be denied");
        assert_eq!(err.token, ResidencyErrorToken::EgressDeny);
        assert_eq!(
            err.fail_closed_message()
                .split(':')
                .next()
                .expect("token prefix"),
            ResidencyErrorToken::EgressDeny.as_str()
        );
    }

    #[test]
    fn denies_missing_rule_with_unsat_token() {
        let policy = ResidencyPolicy::with_rules(vec![ResidencyRule {
            shard: b"core".to_vec(),
            allowed_regions: vec!["us".to_string()],
        }]);
        let err = policy
            .authorize_egress(b"aux", "us")
            .expect_err("missing shard rule must fail closed");
        assert_eq!(err.token, ResidencyErrorToken::EgressPolicyUnsat);
        assert_eq!(
            err.fail_closed_message()
                .split(':')
                .next()
                .expect("token prefix"),
            ResidencyErrorToken::EgressPolicyUnsat.as_str()
        );
    }

    #[test]
    fn allows_equivalent_region_with_case_and_whitespace_variants() {
        let policy = ResidencyPolicy::with_rules(vec![ResidencyRule {
            shard: b"core".to_vec(),
            allowed_regions: vec![" US-CENTRAL ".to_string()],
        }]);
        assert!(policy.authorize_egress(b"core", "us-central").is_ok());
        assert!(policy.authorize_egress(b"core", "  Us-Central ").is_ok());
    }
}
