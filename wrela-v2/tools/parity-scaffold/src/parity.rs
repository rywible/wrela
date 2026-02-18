use crate::contract::{
    ContractCaseResult, ContractCaseStatus, ContractCheck, ContractReport, ContractScenario,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParityDiffKind {
    MissingCase,
    ExitCodeMismatch,
    OutputMismatch,
    SchemaMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParityDiff {
    pub kind: ParityDiffKind,
    pub key: String,
    pub left: String,
    pub right: String,
}

pub trait ToolchainAdapter {
    fn name(&self) -> &'static str;
    fn run_contract_suite(&self) -> ContractReport;
}

pub struct V1Adapter {
    pub bin_path: PathBuf,
    pub workspace_root: PathBuf,
}

pub struct V2PlaceholderAdapter {
    pub shim: V1Adapter,
}

impl ToolchainAdapter for V1Adapter {
    fn name(&self) -> &'static str {
        "v1"
    }

    fn run_contract_suite(&self) -> ContractReport {
        run_scenarios(self.name(), self, default_scenarios())
    }
}

impl ToolchainAdapter for V2PlaceholderAdapter {
    fn name(&self) -> &'static str {
        "v2-placeholder"
    }

    fn run_contract_suite(&self) -> ContractReport {
        run_scenarios(self.name(), &self.shim, default_scenarios())
    }
}

fn default_scenarios() -> Vec<ContractScenario> {
    vec![
        ContractScenario::HelpSurface,
        ContractScenario::ParseErrorExitCode,
        ContractScenario::TypeErrorExitCode,
        ContractScenario::TestListLedgerLite,
        ContractScenario::CertSchemaFixtureFields,
    ]
}

fn run_scenarios(
    name: &str,
    adapter: &V1Adapter,
    scenarios: Vec<ContractScenario>,
) -> ContractReport {
    let mut results = Vec::new();
    let mut checks = Vec::new();

    for scenario in scenarios {
        let result = adapter.run_scenario(&scenario);
        checks.push(ContractCheck {
            name: scenario.id().to_string(),
            passed: matches!(result.status, ContractCaseStatus::Passed),
            detail: format!("exit_code={}", result.exit_code),
        });
        results.push(result);
    }

    ContractReport {
        adapter_name: name.to_string(),
        results,
        checks,
    }
}

impl V1Adapter {
    fn run_scenario(&self, scenario: &ContractScenario) -> ContractCaseResult {
        match scenario {
            ContractScenario::HelpSurface => self.exec_case(
                scenario.id(),
                CommandSpec {
                    args: vec!["--help".to_string()],
                    expected_exit: Some(0),
                },
            ),
            ContractScenario::ParseErrorExitCode => self.exec_temp_source_case(
                scenario.id(),
                "to run() -> Integer:\n    return 1 +\n",
                Some(2),
            ),
            ContractScenario::TypeErrorExitCode => self.exec_temp_source_case(
                scenario.id(),
                "to run() -> Integer:\n    return 1 + true\n",
                Some(3),
            ),
            ContractScenario::TestListLedgerLite => self.exec_case(
                scenario.id(),
                CommandSpec {
                    args: vec![
                        "test".to_string(),
                        "apps/ledger-lite".to_string(),
                        "--list".to_string(),
                    ],
                    expected_exit: Some(0),
                },
            ),
            ContractScenario::CertSchemaFixtureFields => self.eval_cert_fixture(scenario.id()),
        }
    }

    fn exec_temp_source_case(
        &self,
        scenario_id: &str,
        source: &str,
        expected_exit: Option<i32>,
    ) -> ContractCaseResult {
        let temp = tempfile::tempdir().expect("tempdir");
        let src = temp.path().join("main.wr");
        std::fs::write(&src, source).expect("write temp source");
        self.exec_case(
            scenario_id,
            CommandSpec {
                args: vec![src.display().to_string()],
                expected_exit,
            },
        )
    }

    fn eval_cert_fixture(&self, scenario_id: &str) -> ContractCaseResult {
        let fixture = self
            .workspace_root
            .join("compiler/tests/fixtures/cert_schema_v3_example.json");
        let raw = std::fs::read_to_string(&fixture).expect("read cert schema fixture");
        let json: serde_json::Value = serde_json::from_str(&raw).expect("parse fixture json");
        let required = [
            "cert_schema_version",
            "entry_path",
            "workspace_root",
            "artifact_path",
            "tests_passed",
            "compiler_version",
            "runtime_version",
            "source_hash",
            "binary_hash",
        ];
        let missing: Vec<&str> = required
            .iter()
            .copied()
            .filter(|field| json.get(field).is_none())
            .collect();

        let normalized_stdout = if missing.is_empty() {
            "fixture_ok".to_string()
        } else {
            format!("missing={:?}", missing)
        };
        let hash = fnv1a64_hex(normalized_stdout.as_bytes());

        ContractCaseResult {
            scenario_id: scenario_id.to_string(),
            status: if missing.is_empty() {
                ContractCaseStatus::Passed
            } else {
                ContractCaseStatus::Failed
            },
            exit_code: if missing.is_empty() { 0 } else { 4 },
            normalized_stdout,
            normalized_stderr: String::new(),
            json_payload_hash: hash,
        }
    }

    fn exec_case(&self, scenario_id: &str, spec: CommandSpec) -> ContractCaseResult {
        let output = Command::new(&self.bin_path)
            .args(&spec.args)
            .current_dir(&self.workspace_root)
            .output()
            .expect("run contract scenario");

        let code = output.status.code().unwrap_or(4);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        let normalized_stdout = normalize_output(&stdout);
        let normalized_stderr = normalize_output(&stderr);
        let hash = fnv1a64_hex(format!("{}\n{}", normalized_stdout, normalized_stderr).as_bytes());

        let passed = spec
            .expected_exit
            .map(|expected| expected == code)
            .unwrap_or(true);

        ContractCaseResult {
            scenario_id: scenario_id.to_string(),
            status: if passed {
                ContractCaseStatus::Passed
            } else {
                ContractCaseStatus::Failed
            },
            exit_code: code,
            normalized_stdout,
            normalized_stderr,
            json_payload_hash: hash,
        }
    }
}

struct CommandSpec {
    args: Vec<String>,
    expected_exit: Option<i32>,
}

pub fn compare_reports(left: &ContractReport, right: &ContractReport) -> Vec<ParityDiff> {
    let mut diffs = Vec::new();
    let left_map: BTreeMap<&str, &ContractCaseResult> = left
        .results
        .iter()
        .map(|result| (result.scenario_id.as_str(), result))
        .collect();
    let right_map: BTreeMap<&str, &ContractCaseResult> = right
        .results
        .iter()
        .map(|result| (result.scenario_id.as_str(), result))
        .collect();

    for key in left_map.keys().chain(right_map.keys()) {
        let left_case = left_map.get(key);
        let right_case = right_map.get(key);
        match (left_case, right_case) {
            (Some(_), None) | (None, Some(_)) => diffs.push(ParityDiff {
                kind: ParityDiffKind::MissingCase,
                key: (*key).to_string(),
                left: left_case
                    .map(|case| format!("present(exit={})", case.exit_code))
                    .unwrap_or_else(|| "<missing>".to_string()),
                right: right_case
                    .map(|case| format!("present(exit={})", case.exit_code))
                    .unwrap_or_else(|| "<missing>".to_string()),
            }),
            (Some(lhs), Some(rhs)) => {
                if lhs.exit_code != rhs.exit_code {
                    diffs.push(ParityDiff {
                        kind: ParityDiffKind::ExitCodeMismatch,
                        key: (*key).to_string(),
                        left: lhs.exit_code.to_string(),
                        right: rhs.exit_code.to_string(),
                    });
                }
                if lhs.normalized_stdout != rhs.normalized_stdout
                    || lhs.normalized_stderr != rhs.normalized_stderr
                {
                    diffs.push(ParityDiff {
                        kind: ParityDiffKind::OutputMismatch,
                        key: (*key).to_string(),
                        left: format!(
                            "stdout={} stderr={}",
                            lhs.normalized_stdout, lhs.normalized_stderr
                        ),
                        right: format!(
                            "stdout={} stderr={}",
                            rhs.normalized_stdout, rhs.normalized_stderr
                        ),
                    });
                }
                if lhs.json_payload_hash != rhs.json_payload_hash {
                    diffs.push(ParityDiff {
                        kind: ParityDiffKind::SchemaMismatch,
                        key: (*key).to_string(),
                        left: lhs.json_payload_hash.clone(),
                        right: rhs.json_payload_hash.clone(),
                    });
                }
            }
            (None, None) => {}
        }
    }

    diffs
}

pub fn write_parity_artifacts(
    workspace_root: &Path,
    left: &ContractReport,
    right: &ContractReport,
    diffs: &[ParityDiff],
) -> Result<(PathBuf, PathBuf), String> {
    let dir = workspace_root.join(".artifacts/parity");
    std::fs::create_dir_all(&dir)
        .map_err(|err| format!("create parity artifact dir {}: {err}", dir.display()))?;

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let json_path = dir.join(format!("report-{ts}.json"));
    let md_path = dir.join(format!("report-{ts}.md"));

    let payload = serde_json::json!({
        "left": left,
        "right": right,
        "diffs": diffs,
        "green": diffs.is_empty(),
    });
    let json = serde_json::to_vec_pretty(&payload).map_err(|err| err.to_string())?;
    std::fs::write(&json_path, json)
        .map_err(|err| format!("write {}: {err}", json_path.display()))?;

    let mut md = String::new();
    md.push_str("# Parity Report\n\n");
    md.push_str(&format!(
        "left={} right={}\n\n",
        left.adapter_name, right.adapter_name
    ));
    if diffs.is_empty() {
        md.push_str("status: green\n");
    } else {
        md.push_str("status: red\n\n");
        for diff in diffs {
            md.push_str(&format!(
                "- {:?} `{}`\n  left: `{}`\n  right: `{}`\n",
                diff.kind, diff.key, diff.left, diff.right
            ));
        }
    }
    std::fs::write(&md_path, md).map_err(|err| format!("write {}: {err}", md_path.display()))?;

    Ok((json_path, md_path))
}

fn normalize_output(raw: &str) -> String {
    let mut lines: Vec<String> = raw
        .lines()
        .map(strip_absolute_paths)
        .map(strip_timing_noise)
        .filter(|line| !line.trim().is_empty())
        .collect();

    // Normalize listed test order by sorting deterministic id/name/lane lines.
    if lines.iter().all(|line| line.starts_with("id=")) {
        lines.sort();
    }

    lines.join("\n")
}

fn strip_absolute_paths(line: &str) -> String {
    line.replace("/Users/", "<ABS>/")
}

fn strip_timing_noise(line: String) -> String {
    line.replace("ms", "<ms>")
}

fn fnv1a64_hex(input: &[u8]) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &byte in input {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v1_vs_v1_self_parity_is_green() {
        let report = ContractReport {
            adapter_name: "v1".to_string(),
            results: vec![ContractCaseResult {
                scenario_id: "help_surface".to_string(),
                status: ContractCaseStatus::Passed,
                exit_code: 0,
                normalized_stdout: "ok".to_string(),
                normalized_stderr: String::new(),
                json_payload_hash: "abc".to_string(),
            }],
            checks: vec![ContractCheck {
                name: "help_surface".to_string(),
                passed: true,
                detail: "exit_code=0".to_string(),
            }],
        };

        let diffs = compare_reports(&report, &report);
        assert!(diffs.is_empty());
    }

    #[test]
    fn intentional_diff_is_detected() {
        let left = ContractReport {
            adapter_name: "left".to_string(),
            results: vec![ContractCaseResult {
                scenario_id: "type_error_exit_code".to_string(),
                status: ContractCaseStatus::Passed,
                exit_code: 3,
                normalized_stdout: "left".to_string(),
                normalized_stderr: String::new(),
                json_payload_hash: "h1".to_string(),
            }],
            checks: vec![],
        };
        let right = ContractReport {
            adapter_name: "right".to_string(),
            results: vec![ContractCaseResult {
                scenario_id: "type_error_exit_code".to_string(),
                status: ContractCaseStatus::Failed,
                exit_code: 4,
                normalized_stdout: "right".to_string(),
                normalized_stderr: String::new(),
                json_payload_hash: "h2".to_string(),
            }],
            checks: vec![],
        };

        let diffs = compare_reports(&left, &right);
        assert!(!diffs.is_empty());
        assert!(
            diffs
                .iter()
                .any(|d| d.kind == ParityDiffKind::ExitCodeMismatch)
        );
        assert!(
            diffs
                .iter()
                .any(|d| d.kind == ParityDiffKind::OutputMismatch)
        );
        assert!(
            diffs
                .iter()
                .any(|d| d.kind == ParityDiffKind::SchemaMismatch)
        );
    }

    #[test]
    fn writes_parity_artifacts() {
        let left = ContractReport {
            adapter_name: "left".to_string(),
            results: vec![],
            checks: vec![],
        };
        let right = ContractReport {
            adapter_name: "right".to_string(),
            results: vec![],
            checks: vec![],
        };
        let diffs = Vec::new();
        let temp = tempfile::tempdir().expect("tempdir");
        let (json, md) =
            write_parity_artifacts(temp.path(), &left, &right, &diffs).expect("write artifacts");
        assert!(json.exists());
        assert!(md.exists());
    }
}
