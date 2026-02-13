use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractCheck {
    pub name: String,
    pub passed: bool,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContractScenario {
    HelpSurface,
    ParseErrorExitCode,
    TypeErrorExitCode,
    TestListLedgerLite,
    CertSchemaFixtureFields,
}

impl ContractScenario {
    pub fn id(&self) -> &'static str {
        match self {
            Self::HelpSurface => "help_surface",
            Self::ParseErrorExitCode => "parse_error_exit_code",
            Self::TypeErrorExitCode => "type_error_exit_code",
            Self::TestListLedgerLite => "test_list_ledger_lite",
            Self::CertSchemaFixtureFields => "cert_schema_fixture_fields",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContractCaseStatus {
    Passed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractCaseResult {
    pub scenario_id: String,
    pub status: ContractCaseStatus,
    pub exit_code: i32,
    pub normalized_stdout: String,
    pub normalized_stderr: String,
    pub json_payload_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractReport {
    pub adapter_name: String,
    pub results: Vec<ContractCaseResult>,
    pub checks: Vec<ContractCheck>,
}

impl ContractReport {
    pub fn is_green(&self) -> bool {
        self.results
            .iter()
            .all(|result| matches!(result.status, ContractCaseStatus::Passed))
            && self.checks.iter().all(|check| check.passed)
    }
}
