use crate::db::security::residency::ResidencyPolicy;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FederatedResidencyMode {
    FederatedMergeNoRawExport,
    AllowRawExport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FederatedResidencyErrorCode {
    RawExportDenied,
    ResidencyUnsatisfied,
}

impl FederatedResidencyErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RawExportDenied => "FEDERATED_RAW_EXPORT_DENY",
            Self::ResidencyUnsatisfied => "FEDERATED_RESIDENCY_UNSAT",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FederatedResidencyError {
    pub code: FederatedResidencyErrorCode,
    pub reason: String,
}

impl FederatedResidencyError {
    pub fn fail_closed_message(&self) -> String {
        format!("{}: {}", self.code.as_str(), self.reason)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FederatedResidencyGuard {
    pub mode: FederatedResidencyMode,
}

impl Default for FederatedResidencyGuard {
    fn default() -> Self {
        Self {
            mode: FederatedResidencyMode::FederatedMergeNoRawExport,
        }
    }
}

impl FederatedResidencyGuard {
    pub fn validate(
        &self,
        residency: &ResidencyPolicy,
        shards: &[Vec<u8>],
        sink_region: &str,
        raw_export_requested: bool,
    ) -> Result<(), FederatedResidencyError> {
        if self.mode == FederatedResidencyMode::FederatedMergeNoRawExport && raw_export_requested {
            return Err(FederatedResidencyError {
                code: FederatedResidencyErrorCode::RawExportDenied,
                reason: "default mode forbids raw federated export".to_string(),
            });
        }

        for shard in shards {
            residency
                .authorize_egress(shard, sink_region)
                .map_err(|err| FederatedResidencyError {
                    code: FederatedResidencyErrorCode::ResidencyUnsatisfied,
                    reason: err.fail_closed_message(),
                })?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{FederatedResidencyErrorCode, FederatedResidencyGuard};
    use crate::db::security::residency::{ResidencyPolicy, ResidencyRule};

    #[test]
    fn default_mode_denies_raw_export() {
        let guard = FederatedResidencyGuard::default();
        let policy = ResidencyPolicy::with_rules(vec![ResidencyRule {
            shard: b"s1".to_vec(),
            allowed_regions: vec!["us".to_string()],
        }]);
        let err = guard
            .validate(&policy, &[b"s1".to_vec()], "us", true)
            .expect_err("raw export must be denied");
        assert_eq!(err.code, FederatedResidencyErrorCode::RawExportDenied);
    }

    #[test]
    fn residency_unsat_fails_closed() {
        let guard = FederatedResidencyGuard::default();
        let policy = ResidencyPolicy::with_rules(vec![ResidencyRule {
            shard: b"s1".to_vec(),
            allowed_regions: vec!["us".to_string()],
        }]);
        let err = guard
            .validate(&policy, &[b"s1".to_vec()], "eu", false)
            .expect_err("egress should fail closed");
        assert_eq!(err.code, FederatedResidencyErrorCode::ResidencyUnsatisfied);
        assert!(err.reason.contains("RESIDENCY_EGRESS_DENY"));
    }
}
