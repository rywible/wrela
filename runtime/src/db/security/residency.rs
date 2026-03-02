use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResidencyRule {
    pub shard: Vec<u8>,
    pub allowed_regions: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadSovereigntyMode {
    Strict,
    StaleOk,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadSovereigntyCap {
    StrictOnly,
    PolicyCappedClientChoice,
}

impl Default for ReadSovereigntyCap {
    fn default() -> Self {
        Self::StrictOnly
    }
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
    read_cap: ReadSovereigntyCap,
    checkpoint_allowed_regions: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct EnvResidencyPolicy {
    rules: Vec<EnvResidencyRule>,
    #[serde(default)]
    read_cap: Option<String>,
    #[serde(default)]
    checkpoint_allowed_regions: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct EnvResidencyRule {
    shard: String,
    allowed_regions: Vec<String>,
}

impl ResidencyPolicy {
    pub fn with_rules(rules: Vec<ResidencyRule>) -> Self {
        Self::with_rules_and_options(rules, ReadSovereigntyCap::StrictOnly, Vec::new())
    }

    pub fn with_rules_and_options(
        rules: Vec<ResidencyRule>,
        read_cap: ReadSovereigntyCap,
        checkpoint_allowed_regions: Vec<String>,
    ) -> Self {
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
        let mut checkpoint_allowed_regions = checkpoint_allowed_regions
            .into_iter()
            .map(|region| normalize_region(&region))
            .filter(|region| !region.is_empty())
            .collect::<Vec<_>>();
        checkpoint_allowed_regions.sort();
        checkpoint_allowed_regions.dedup();
        Self {
            rules: normalized,
            read_cap,
            checkpoint_allowed_regions,
        }
    }

    pub fn from_env() -> Result<Option<Self>, String> {
        let Ok(raw) = std::env::var("WRELADB_RESIDENCY_POLICY_JSON") else {
            return Ok(None);
        };
        let parsed: EnvResidencyPolicy = serde_json::from_str(&raw)
            .map_err(|err| format!("invalid residency policy json: {err}"))?;
        let read_cap = match parsed
            .read_cap
            .as_deref()
            .unwrap_or("strict_only")
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "strict_only" => ReadSovereigntyCap::StrictOnly,
            "policy_capped_client_choice" | "allow_stale_outside_home" => {
                ReadSovereigntyCap::PolicyCappedClientChoice
            }
            other => {
                return Err(format!("invalid read_cap value: {other}"));
            }
        };

        let rules = parsed
            .rules
            .into_iter()
            .map(|rule| ResidencyRule {
                shard: rule.shard.into_bytes(),
                allowed_regions: rule.allowed_regions,
            })
            .collect::<Vec<_>>();
        Ok(Some(Self::with_rules_and_options(
            rules,
            read_cap,
            parsed.checkpoint_allowed_regions,
        )))
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

    pub fn authorize_write(&self, shard: &[u8], sink_region: &str) -> Result<(), ResidencyError> {
        self.authorize_egress(shard, sink_region)
    }

    pub fn authorize_read(
        &self,
        shard: &[u8],
        source_region: &str,
        requested_mode: ReadSovereigntyMode,
    ) -> Result<ReadSovereigntyMode, ResidencyError> {
        let normalized_source_region = normalize_region(source_region);
        let Some(rule) = self
            .rules
            .iter()
            .find(|rule| rule.shard.as_slice() == shard)
        else {
            return Err(ResidencyError {
                token: ResidencyErrorToken::EgressPolicyUnsat,
                reason: format!("shard={} has no read rule", String::from_utf8_lossy(shard)),
            });
        };

        if rule
            .allowed_regions
            .iter()
            .any(|region| region == &normalized_source_region)
        {
            return Ok(requested_mode);
        }

        match (self.read_cap, requested_mode) {
            (ReadSovereigntyCap::PolicyCappedClientChoice, ReadSovereigntyMode::StaleOk) => {
                Ok(ReadSovereigntyMode::StaleOk)
            }
            _ => Err(ResidencyError {
                token: ResidencyErrorToken::EgressDeny,
                reason: format!(
                    "shard={} source_region={} denied read_mode={:?} allowed={:?}",
                    String::from_utf8_lossy(shard),
                    normalized_source_region,
                    requested_mode,
                    rule.allowed_regions
                ),
            }),
        }
    }

    pub fn authorize_checkpoint_region(&self, region: &str) -> Result<(), ResidencyError> {
        let normalized = normalize_region(region);
        if self.checkpoint_allowed_regions.is_empty() {
            for rule in &self.rules {
                if rule.allowed_regions.iter().any(|r| r == &normalized) {
                    return Ok(());
                }
            }
            return Err(ResidencyError {
                token: ResidencyErrorToken::EgressDeny,
                reason: format!(
                    "checkpoint region {} not allowed by any residency rule",
                    normalized
                ),
            });
        }
        if self
            .checkpoint_allowed_regions
            .iter()
            .any(|region| region == &normalized)
        {
            return Ok(());
        }
        Err(ResidencyError {
            token: ResidencyErrorToken::EgressDeny,
            reason: format!(
                "checkpoint region {} denied; allowed={:?}",
                normalized, self.checkpoint_allowed_regions
            ),
        })
    }

    pub fn read_cap(&self) -> ReadSovereigntyCap {
        self.read_cap
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

    #[test]
    fn read_policy_capped_mode_allows_stale_when_client_requests_it() {
        let policy = ResidencyPolicy::with_rules_and_options(
            vec![ResidencyRule {
                shard: b"core".to_vec(),
                allowed_regions: vec!["ord".to_string()],
            }],
            ReadSovereigntyCap::PolicyCappedClientChoice,
            vec![],
        );
        let mode = policy
            .authorize_read(b"core", "iad", ReadSovereigntyMode::StaleOk)
            .expect("stale read allowed by cap");
        assert_eq!(mode, ReadSovereigntyMode::StaleOk);
    }

    #[test]
    fn strict_only_cap_denies_out_of_region_read_even_when_stale_requested() {
        let policy = ResidencyPolicy::with_rules(vec![ResidencyRule {
            shard: b"core".to_vec(),
            allowed_regions: vec!["ord".to_string()],
        }]);
        let err = policy
            .authorize_read(b"core", "iad", ReadSovereigntyMode::StaleOk)
            .expect_err("strict-only cap must deny");
        assert_eq!(err.token, ResidencyErrorToken::EgressDeny);
    }

    #[test]
    fn checkpoint_region_policy_is_enforced() {
        let policy = ResidencyPolicy::with_rules_and_options(
            vec![ResidencyRule {
                shard: b"core".to_vec(),
                allowed_regions: vec!["ord".to_string()],
            }],
            ReadSovereigntyCap::StrictOnly,
            vec!["ord".to_string()],
        );
        assert!(policy.authorize_checkpoint_region("ord").is_ok());
        let err = policy
            .authorize_checkpoint_region("iad")
            .expect_err("checkpoint region should be denied");
        assert_eq!(err.token, ResidencyErrorToken::EgressDeny);
    }
}
