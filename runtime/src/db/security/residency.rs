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
        Self { rules }
    }

    pub fn authorize_egress(&self, shard: &[u8], sink_region: &str) -> Result<(), ResidencyError> {
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
            .any(|region| region == sink_region)
        {
            return Ok(());
        }
        Err(ResidencyError {
            token: ResidencyErrorToken::EgressDeny,
            reason: format!(
                "shard={} sink_region={} allowed={:?}",
                String::from_utf8_lossy(shard),
                sink_region,
                rule.allowed_regions
            ),
        })
    }
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
}
