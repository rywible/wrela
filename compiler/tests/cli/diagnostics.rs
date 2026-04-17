use super::*;

#[test]
fn cli_rejects_removed_format_flag_alias() {
    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg("--format=json")
        .arg(".")
        .output()
        .expect("run wrela");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("`--format` was removed"));
}

#[test]
fn cli_fmt_defaults_to_target_file_diagnostics_scope() {
    let dir = workspace_tempdir();
    let src_dir = dir.path().join("src");
    std::fs::create_dir_all(&src_dir).expect("create src");
    let main_path = src_dir.join("main.wr");
    let helper_path = src_dir.join("helper.wr");
    write_fixture_file(
        &main_path,
        r#"use add from helper

fn run() -> Integer {
    return add(value=1, extra=2)
}
"#,
    )
    .expect("write main");
    write_fixture_file(
        &helper_path,
        r#"fn add(value: Integer, extra: Integer) -> Integer {
    return value + extra
}

fn trigger_named_args_error() -> Integer {
    return add(1, 2)
}
"#,
    )
    .expect("write helper");

    let scoped = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fmt")
        .arg("--error-format=json")
        .arg(&main_path)
        .output()
        .expect("run scoped fmt");
    assert!(
        scoped.status.success(),
        "scoped fmt failed: code={:?}\nstdout={}\nstderr={}",
        scoped.status.code(),
        String::from_utf8_lossy(&scoped.stdout),
        String::from_utf8_lossy(&scoped.stderr)
    );
    let scoped_stdout = String::from_utf8_lossy(&scoped.stdout);
    assert!(
        !scoped_stdout.contains("named_args_required"),
        "target-scoped fmt should not emit imported helper diagnostics: {scoped_stdout}"
    );

    let workspace = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fmt")
        .arg("--workspace-diagnostics")
        .arg("--error-format=json")
        .arg(&main_path)
        .output()
        .expect("run workspace fmt");
    assert!(
        workspace.status.success(),
        "workspace fmt failed: code={:?}\nstdout={}\nstderr={}",
        workspace.status.code(),
        String::from_utf8_lossy(&workspace.stdout),
        String::from_utf8_lossy(&workspace.stderr)
    );
    let workspace_stdout = String::from_utf8_lossy(&workspace.stdout);
    assert!(
        workspace_stdout.contains("named_args_required"),
        "workspace diagnostics should include imported helper errors: {workspace_stdout}"
    );
}

#[test]
fn cli_run_integration_mode_enforces_entry_layout_guardrail() {
    let dir = workspace_tempdir();
    let entry = dir.path().join("main.wr");
    write_fixture_file(
        &entry,
        r#"fn run() -> Integer {
    return 0
}
"#,
    )
    .expect("write main.wr");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("run")
        .arg("--integration-mode")
        .arg(&entry)
        .output()
        .expect("run wrela");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--integration-mode requires entrypoint under"));
}

#[test]
fn cli_init_creates_project() {
    let dir = workspace_tempdir();
    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("init")
        .arg(dir.path())
        .output()
        .expect("run wrela");
    assert!(output.status.success());
    let main_path = dir.path().join("src").join("main.wr");
    assert!(main_path.exists());
}

#[test]
fn cli_json_diagnostics() {
    let dir = workspace_tempdir();
    let path = dir.path().join("main.wr");
    write_fixture_file(
        &path,
        r#"fn run() -> Integer {
    return 1 +
}
"#,
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let first = stdout.lines().next().expect("json output");
    let value: serde_json::Value = serde_json::from_str(first).expect("valid json");
    assert!(value.get("message").is_some());
    assert!(value.get("span").is_some());
    assert!(
        value
            .get("code")
            .and_then(|value| value.as_str())
            .is_some_and(|code| !code.is_empty())
    );
    assert!(
        value
            .get("rule")
            .and_then(|value| value.as_str())
            .is_some_and(|rule| !rule.is_empty())
    );
    assert!(value.get("help").is_some());
    assert!(
        value
            .get("stage")
            .and_then(|value| value.as_str())
            .is_some_and(|stage| !stage.is_empty())
    );
    assert!(
        value
            .get("severity")
            .and_then(|value| value.as_str())
            .is_some_and(|severity| severity == "error" || severity == "warning")
    );
    assert!(
        value
            .get("labels")
            .and_then(|value| value.as_array())
            .is_some_and(|labels| !labels.is_empty())
    );
    assert!(value.get("diag_id").is_some());
}

#[test]
fn cli_json_shorthand_diagnostics() {
    let dir = workspace_tempdir();
    let path = dir.path().join("main.wr");
    write_fixture_file(
        &path,
        r#"fn run() -> Integer {
    return 1 +
}
"#,
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("--json")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let first = stdout.lines().next().expect("json output");
    let value: serde_json::Value = serde_json::from_str(first).expect("valid json");
    assert!(value.get("message").is_some());
    assert!(value.get("span").is_some());
    assert_eq!(value.get("kind").and_then(|v| v.as_str()), Some("error"));
}

#[test]
fn cli_json_typed_hole_includes_data_and_candidate_suggestions() {
    let dir = workspace_tempdir();
    let path = dir.path().join("main.wr");
    write_fixture_file(
        &path,
        r#"fn run(value: Integer) -> Integer {
    return _todo
}
"#,
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(!output.status.success());
    let diagnostics = parse_json_stdout_lines(&output.stdout);
    let typed_hole = diagnostics
        .iter()
        .find(|diag| {
            diag.get("code")
                .and_then(|value| value.as_str())
                .is_some_and(|code| code == "lang::sem::typed_hole")
        })
        .expect("expected typed hole diagnostic");
    assert_eq!(
        typed_hole
            .get("data")
            .and_then(|v| v.get("kind"))
            .and_then(|v| v.as_str()),
        Some("typed_hole")
    );
    assert_eq!(
        typed_hole
            .get("data")
            .and_then(|v| v.get("hole_name"))
            .and_then(|v| v.as_str()),
        Some("_todo")
    );
    assert!(
        typed_hole
            .get("data")
            .and_then(|v| v.get("candidate_bindings"))
            .and_then(|v| v.as_array())
            .is_some_and(|arr| arr.iter().any(|v| v.as_str() == Some("value")))
    );
    assert!(
        typed_hole
            .get("data")
            .and_then(|v| v.get("hole_id"))
            .and_then(|v| v.as_str())
            .is_some_and(|id| id.contains(":_todo"))
    );
    assert_eq!(
        typed_hole
            .get("data")
            .and_then(|v| v.get("ranking_strategy"))
            .and_then(|v| v.as_str()),
        Some("lexicographic_binding_name")
    );
    assert!(
        typed_hole
            .get("data")
            .and_then(|v| v.get("ranked_candidates"))
            .and_then(|v| v.as_array())
            .is_some_and(|arr| {
                arr.iter().all(|candidate| {
                    candidate.get("rank").and_then(|v| v.as_u64()).is_some()
                        && candidate.get("name").and_then(|v| v.as_str()).is_some()
                })
            })
    );
    assert!(
        typed_hole
            .get("data")
            .and_then(|v| v.get("code_actions"))
            .and_then(|v| v.as_array())
            .is_some_and(|arr| {
                arr.iter().any(|action| {
                    action
                        .get("kind")
                        .and_then(|v| v.as_str())
                        .is_some_and(|kind| kind == "fill_typed_hole")
                })
            })
    );
    assert!(
        typed_hole
            .get("suggestions")
            .and_then(|v| v.as_array())
            .is_some_and(|arr| arr.iter().any(|s| {
                s.get("replacement")
                    .and_then(|v| v.as_str())
                    .is_some_and(|candidate| candidate.trim() == "value")
            }))
    );
}

#[test]
fn cli_holes_only_filters_non_hole_semantic_diagnostics() {
    let dir = workspace_tempdir();
    let path = dir.path().join("main.wr");
    write_fixture_file(
        &path,
        r#"fn helper(a: Integer, b: Integer) -> Integer {
    return a + b

}
fn run(value: Integer) -> Integer {
    helper(a=1, 2)
    return _todo
}
"#,
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg("--holes-only")
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(!output.status.success());
    let diagnostics = parse_json_stdout_lines(&output.stdout);
    assert!(
        diagnostics.iter().any(|diag| {
            diag.get("code")
                .and_then(|value| value.as_str())
                .is_some_and(|code| code == "lang::sem::typed_hole")
        }),
        "expected typed hole diagnostics, got:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        diagnostics.iter().all(|diag| {
            diag.get("code")
                .and_then(|value| value.as_str())
                .is_some_and(|code| code == "lang::sem::typed_hole")
        }),
        "holes-only mode should suppress non-hole diagnostics, got:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn cli_json_try_outside_result_includes_data_and_remove_try_suggestion() {
    let dir = workspace_tempdir();
    let path = dir.path().join("main.wr");
    write_fixture_file(
        &path,
        r#"fn source() -> Result[Integer] {
    return 1

}
fn run() -> Integer {
    return source()?
}
"#,
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(!output.status.success());
    let diagnostics = parse_json_stdout_lines(&output.stdout);
    let try_diag = diagnostics
        .iter()
        .find(|diag| {
            diag.get("code")
                .and_then(|value| value.as_str())
                .is_some_and(|code| code == "lang::ty::try_outside_result")
        })
        .expect("expected try-outside-result diagnostic");
    assert_eq!(
        try_diag
            .get("data")
            .and_then(|v| v.get("kind"))
            .and_then(|v| v.as_str()),
        Some("try_outside_result")
    );
    assert!(
        try_diag
            .get("suggestions")
            .and_then(|v| v.as_array())
            .is_some_and(|arr| arr.iter().any(|s| {
                s.get("replacement").and_then(|v| v.as_str()) == Some("")
                    && s.get("reason_code").and_then(|v| v.as_str()) == Some("remove_try_operator")
            })),
        "expected remove-try suggestion, got:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn cli_json_invalid_try_operand_includes_data_and_remove_try_suggestion() {
    let dir = workspace_tempdir();
    let path = dir.path().join("main.wr");
    write_fixture_file(
        &path,
        r#"fn run() -> Result[Integer] {
    return 1?
}
"#,
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(!output.status.success());
    let diagnostics = parse_json_stdout_lines(&output.stdout);
    let try_diag = diagnostics
        .iter()
        .find(|diag| {
            diag.get("code")
                .and_then(|value| value.as_str())
                .is_some_and(|code| code == "lang::ty::invalid_try_operand")
        })
        .expect("expected invalid-try-operand diagnostic");
    assert_eq!(
        try_diag
            .get("data")
            .and_then(|v| v.get("kind"))
            .and_then(|v| v.as_str()),
        Some("invalid_try_operand")
    );
    assert!(
        try_diag
            .get("suggestions")
            .and_then(|v| v.as_array())
            .is_some_and(|arr| arr.iter().any(|s| {
                s.get("replacement").and_then(|v| v.as_str()) == Some("")
                    && s.get("reason_code").and_then(|v| v.as_str()) == Some("remove_try_operator")
            })),
        "expected remove-try suggestion, got:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn cli_sarif_parse_diagnostics_include_required_contract_fields() {
    let dir = workspace_tempdir();
    let path = dir.path().join("main.wr");
    write_fixture_file(
        &path,
        r#"fn run() -> Integer {
    return 1 +
}
"#,
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("--error-format=sarif")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(!output.status.success());
    let logs = parse_json_stdout_lines(&output.stdout);
    let mut parse = None;
    for log in &logs {
        for result in assert_sarif_log_contract(log) {
            if result
                .get("ruleId")
                .and_then(|value| value.as_str())
                .is_some_and(|rule| rule.starts_with("lang::parse::"))
            {
                parse = Some(result);
                break;
            }
        }
        if parse.is_some() {
            break;
        }
    }
    let parse = parse.expect("expected parse SARIF result");
    assert_sarif_result_contract(parse);
}

#[test]
fn cli_sarif_naming_or_type_diagnostics_include_required_contract_fields() {
    let dir = workspace_tempdir();
    let path = dir.path().join("main.wr");
    write_fixture_file(
        &path,
        r#"fn BadName() -> Integer {
    value = 1
    return true
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg("--error-format=sarif")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(!output.status.success());

    let logs = parse_json_stdout_lines(&output.stdout);
    let mut semantic = None;
    for log in &logs {
        for result in assert_sarif_log_contract(log) {
            if result
                .get("ruleId")
                .and_then(|value| value.as_str())
                .is_some_and(|rule| {
                    rule.starts_with("lang::naming::") || rule.starts_with("lang::ty::")
                })
            {
                semantic = Some(result);
                break;
            }
        }
        if semantic.is_some() {
            break;
        }
    }
    let semantic = semantic.expect("expected naming/type SARIF result");
    assert_sarif_result_contract(semantic);
}

pub(super) fn assert_sarif_log_contract(value: &serde_json::Value) -> &[serde_json::Value] {
    assert_eq!(
        value.get("$schema").and_then(|v| v.as_str()),
        Some("https://json.schemastore.org/sarif-2.1.0.json")
    );
    assert_eq!(value.get("version").and_then(|v| v.as_str()), Some("2.1.0"));
    let runs = value
        .get("runs")
        .and_then(|v| v.as_array())
        .expect("sarif runs array");
    assert!(!runs.is_empty(), "expected at least one SARIF run");
    let driver_name = runs[0]
        .get("tool")
        .and_then(|v| v.get("driver"))
        .and_then(|v| v.get("name"))
        .and_then(|v| v.as_str());
    assert_eq!(driver_name, Some("wrela"));
    let results = runs[0]
        .get("results")
        .and_then(|v| v.as_array())
        .expect("sarif results array");
    assert!(!results.is_empty(), "expected at least one SARIF result");
    results
}

fn assert_sarif_result_contract(result: &serde_json::Value) {
    assert!(
        result
            .get("ruleId")
            .and_then(|v| v.as_str())
            .is_some_and(|id| !id.is_empty())
    );
    assert!(
        result
            .get("level")
            .and_then(|v| v.as_str())
            .is_some_and(|level| level == "error" || level == "warning")
    );
    assert!(
        result
            .get("message")
            .and_then(|v| v.get("text"))
            .and_then(|v| v.as_str())
            .is_some_and(|text| !text.is_empty())
    );
    let locations = result
        .get("locations")
        .and_then(|v| v.as_array())
        .expect("sarif locations array");
    assert!(
        !locations.is_empty(),
        "expected at least one SARIF location"
    );
    let first = &locations[0];
    assert!(
        first
            .get("physicalLocation")
            .and_then(|v| v.get("artifactLocation"))
            .and_then(|v| v.get("uri"))
            .and_then(|v| v.as_str())
            .is_some_and(|uri| !uri.is_empty())
    );
    let region = first
        .get("physicalLocation")
        .and_then(|v| v.get("region"))
        .expect("sarif region");
    assert!(region.get("startLine").and_then(|v| v.as_u64()).is_some());
    assert!(region.get("startColumn").and_then(|v| v.as_u64()).is_some());
    assert!(region.get("charOffset").and_then(|v| v.as_u64()).is_some());
    assert!(region.get("charLength").and_then(|v| v.as_u64()).is_some());
}

#[test]
fn cli_json_naming_diagnostics_include_metadata_fields_when_present() {
    let dir = workspace_tempdir();
    let path = dir.path().join("main.wr");
    write_fixture_file(
        &path,
        r#"fn BadName() -> Integer {
    let AlsoBad = 1
    return AlsoBad
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run wrela");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let diagnostics: Vec<serde_json::Value> = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("valid json"))
        .collect();

    let naming: Vec<&serde_json::Value> = diagnostics
        .iter()
        .filter(|diag| {
            diag.get("code")
                .and_then(|value| value.as_str())
                .is_some_and(|code| code.starts_with("lang::naming::"))
        })
        .collect();

    if naming.is_empty() {
        return;
    }

    for diag in naming {
        let code = diag
            .get("code")
            .and_then(|value| value.as_str())
            .expect("naming diagnostic has code");
        assert!(code.starts_with("lang::naming::"));
        assert!(
            diag.get("rule")
                .and_then(|value| value.as_str())
                .is_some_and(|rule| !rule.is_empty())
        );
        assert!(diag.get("help").is_some());
        let suggestions = diag
            .get("suggestions")
            .and_then(|value| value.as_array())
            .expect("naming diagnostic has suggestions array");
        for suggestion in suggestions {
            assert!(suggestion.get("replacement").is_some());
            assert!(suggestion.get("span").is_some());
            assert!(suggestion.get("rationale").is_some());
            assert!(suggestion.get("confidence").is_some());
            assert!(
                suggestion
                    .get("applicability")
                    .and_then(|value| value.as_str())
                    .is_some_and(|v| {
                        v == "machine_applicable" || v == "maybe_correct" || v == "has_placeholders"
                    })
            );
            assert!(
                suggestion
                    .get("safety_tier")
                    .and_then(|value| value.as_str())
                    .is_some_and(|tier| tier == "safe" || tier == "review")
            );
            assert!(
                suggestion
                    .get("reason_code")
                    .and_then(|value| value.as_str())
                    .is_some_and(|code| !code.is_empty())
            );
            if suggestion
                .get("applicability")
                .and_then(|value| value.as_str())
                .is_some_and(|v| v == "machine_applicable")
            {
                assert!(
                    suggestion
                        .get("expected_source")
                        .and_then(|value| value.as_str())
                        .is_some(),
                    "machine-applicable fixes must include expected_source"
                );
            }
        }
    }
}

#[test]
fn cli_json_diag_id_is_stable_across_runs() {
    let dir = workspace_tempdir();
    let path = dir.path().join("main.wr");
    write_fixture_file(
        &path,
        r#"fn run() -> Integer {
    return 1 +
}
"#,
    )
    .unwrap();

    let first = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run wrela");
    let second = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(!first.status.success());
    assert!(!second.status.success());

    let first_stdout = String::from_utf8_lossy(&first.stdout);
    let first_diag = first_stdout
        .lines()
        .find(|line| !line.trim().is_empty())
        .expect("first json line");
    let second_stdout = String::from_utf8_lossy(&second.stdout);
    let second_diag = second_stdout
        .lines()
        .find(|line| !line.trim().is_empty())
        .expect("second json line");
    let first_json: serde_json::Value = serde_json::from_str(first_diag).expect("valid json");
    let second_json: serde_json::Value = serde_json::from_str(second_diag).expect("valid json");
    assert_eq!(
        first_json.get("diag_id").and_then(|value| value.as_str()),
        second_json.get("diag_id").and_then(|value| value.as_str())
    );
}

#[test]
fn cli_json_contract_matches_required_and_optional_key_fixtures() {
    let required = include_str!("../fixtures/diagnostics/json_required_keys.txt")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>();
    let optional = include_str!("../fixtures/diagnostics/json_optional_keys.txt")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>();

    let dir = workspace_tempdir();
    let path = dir.path().join("main.wr");
    write_fixture_file(
        &path,
        r#"fn run() -> Integer {
    return 1 +
}
"#,
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let first = stdout
        .lines()
        .find(|line| !line.trim().is_empty())
        .expect("json line");
    let value: serde_json::Value = serde_json::from_str(first).expect("valid json");
    let object = value.as_object().expect("diagnostic is object");

    for key in required {
        assert!(object.contains_key(key), "missing required key: {key}");
    }
    for key in optional {
        if object.contains_key(key) {
            assert_ne!(key, "kind");
        }
    }
}

#[test]
fn cli_json_parse_diagnostics_use_specific_parse_codes() {
    let dir = workspace_tempdir();
    let path = dir.path().join("main.wr");
    write_fixture_file(
        &path,
        r#"fn run() -> Integer {
    return 1 +
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let diagnostics: Vec<serde_json::Value> = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("valid json"))
        .collect();
    let parse_codes = diagnostics
        .iter()
        .filter_map(|diag| diag.get("code").and_then(|value| value.as_str()))
        .filter(|code| code.starts_with("lang::parse::"))
        .collect::<Vec<_>>();
    assert!(
        parse_codes
            .iter()
            .any(|code| *code != "lang::parse::syntax_error"),
        "expected at least one specific parse code, got: {parse_codes:?}"
    );
}

#[test]
fn cli_analyze_alias_matches_check_parse_behavior() {
    let dir = workspace_tempdir();
    let path = dir.path().join("main.wr");
    write_fixture_file(
        &path,
        r#"fn run() -> Integer {
    return 1 +
}
"#,
    )
    .unwrap();

    let analyze = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("analyze")
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run wrela analyze");
    assert!(!analyze.status.success());
    let analyze_stdout = String::from_utf8_lossy(&analyze.stdout);
    let analyze_first = analyze_stdout
        .lines()
        .find(|line| !line.trim().is_empty())
        .expect("analyze produced diagnostic");
    let analyze_json: serde_json::Value =
        serde_json::from_str(analyze_first).expect("analyze json");
    assert_eq!(
        analyze_json.get("kind").and_then(|v| v.as_str()),
        Some("error")
    );

    let check = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run wrela check");
    assert!(!check.status.success());
    let check_stdout = String::from_utf8_lossy(&check.stdout);
    let check_first = check_stdout
        .lines()
        .find(|line| !line.trim().is_empty())
        .expect("check produced diagnostic");
    let check_json: serde_json::Value = serde_json::from_str(check_first).expect("check json");
    assert_eq!(
        analyze_json.get("code").and_then(|v| v.as_str()),
        check_json.get("code").and_then(|v| v.as_str())
    );
}

#[test]
fn cli_test_harness_json_aggregates_multiple_type_errors() {
    let dir = workspace_tempdir();
    let src_dir = dir.path().join("src");
    let module_path = src_dir.join("broken.wr");
    std::fs::create_dir_all(&src_dir).unwrap();
    write_fixture_file(
        src_dir.join("main.wr"),
        r#"use bad from broken

fn run() -> Integer {
    return bad()
}
"#,
    )
    .unwrap();
    write_fixture_file(
        module_path,
        r#"fn bad() -> Integer {
    x = 1 + true
    y = 1 + false
    return 0
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg("--error-format=json")
        .arg(src_dir.join("main.wr"))
        .output()
        .expect("run wrela");

    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let diagnostics: Vec<serde_json::Value> = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("valid json"))
        .collect();
    let type_errors: Vec<&serde_json::Value> = diagnostics
        .iter()
        .filter(|diag| {
            diag.get("code")
                .and_then(|value| value.as_str())
                .is_some_and(|code| code.starts_with("lang::ty::"))
        })
        .collect();
    assert!(
        type_errors.len() >= 2,
        "expected aggregated type diagnostics, got:\n{}",
        stdout
    );
}

#[test]
fn cli_json_reports_named_args_required_code() {
    let dir = workspace_tempdir();
    let path = dir.path().join("main.wr");
    write_fixture_file(
        &path,
        r#"fn add(a: Integer, b: Integer) -> Integer {
    return a + b

}
fn run() -> Integer {
    return add(1, 2)
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(!output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let diagnostics: Vec<serde_json::Value> = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("valid json"))
        .collect();
    let named_args = diagnostics
        .iter()
        .find(|diag| {
            diag.get("code")
                .and_then(|value| value.as_str())
                .is_some_and(|code| code == "lang::ty::named_args_required")
        })
        .expect("expected named args required diagnostic");
    assert_eq!(
        named_args
            .get("data")
            .and_then(|v| v.get("kind"))
            .and_then(|v| v.as_str()),
        Some("named_args_required")
    );
    assert!(
        named_args
            .get("suggestions")
            .and_then(|v| v.as_array())
            .is_some_and(|arr| arr.iter().any(|s| {
                s.get("reason_code")
                    .and_then(|v| v.as_str())
                    .is_some_and(|code| code == "named_args_rewrite")
            })),
        "expected named-args rewrite suggestion, got:\n{}",
        stdout
    );
    assert!(
        named_args
            .get("suggestions")
            .and_then(|v| v.as_array())
            .is_some_and(|arr| arr.iter().any(|s| {
                s.get("applicability")
                    .and_then(|v| v.as_str())
                    .is_some_and(|mode| mode == "machine_applicable")
            })),
        "expected machine-applicable named-args suggestion, got:\n{}",
        stdout
    );
}

#[test]
fn cli_json_reports_equality_requires_eq_code() {
    let dir = workspace_tempdir();
    let path = dir.path().join("main.wr");
    write_fixture_file(
        &path,
        r#"class Worker {
    id: Integer
}
fn same(a: Actor[Worker], b: Actor[Worker]) -> Boolean {
    return a == b
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(!output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let diagnostics: Vec<serde_json::Value> = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("valid json"))
        .collect();
    let equality = diagnostics
        .iter()
        .find(|diag| {
            diag.get("code")
                .and_then(|value| value.as_str())
                .is_some_and(|code| code == "lang::ty::equality_requires_eq")
        })
        .expect("expected equality Eq diagnostic");
    assert_eq!(
        equality
            .get("data")
            .and_then(|v| v.get("kind"))
            .and_then(|v| v.as_str()),
        Some("equality_requires_eq")
    );
    assert_eq!(
        equality
            .get("data")
            .and_then(|v| v.get("left_type"))
            .and_then(|v| v.as_str()),
        Some("Actor[Worker]")
    );
    assert_eq!(
        equality
            .get("data")
            .and_then(|v| v.get("right_type"))
            .and_then(|v| v.as_str()),
        Some("Actor[Worker]")
    );
}

#[test]
fn cli_json_reports_equality_requires_eq_code_for_enum() {
    let dir = workspace_tempdir();
    let path = dir.path().join("main.wr");
    write_fixture_file(
        &path,
        r#"class Worker {
    id: Integer
}
enum Status {
    Pending
    Running(task: Pending[Result[Worker]])

}
fn same(a: Status, b: Status) -> Boolean {
    return a == b
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(!output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let diagnostics: Vec<serde_json::Value> = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("valid json"))
        .collect();
    let equality = diagnostics
        .iter()
        .find(|diag| {
            diag.get("code")
                .and_then(|value| value.as_str())
                .is_some_and(|code| code == "lang::ty::equality_requires_eq")
        })
        .expect("expected enum equality Eq diagnostic");
    assert_eq!(
        equality
            .get("data")
            .and_then(|v| v.get("kind"))
            .and_then(|v| v.as_str()),
        Some("equality_requires_eq")
    );
    assert_eq!(
        equality
            .get("data")
            .and_then(|v| v.get("left_type"))
            .and_then(|v| v.as_str()),
        Some("Status")
    );
    assert_eq!(
        equality
            .get("data")
            .and_then(|v| v.get("right_type"))
            .and_then(|v| v.as_str()),
        Some("Status")
    );
}

#[test]
fn cli_check_accepts_structural_enum_equality() {
    let dir = workspace_tempdir();
    let path = dir.path().join("main.wr");
    write_fixture_file(
        &path,
        r#"enum Status {
    Pending
    Done

}
fn compute_match(a: Status, b: Status) -> Integer {
    if a == b {
        return 1
    }
    return 0
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(
        output.status.success(),
        "expected check to pass for structural enum equality:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn cli_json_reports_boundary_generic_rewrite_suggestion() {
    let dir = workspace_tempdir();
    let path = dir.path().join("main.wr");
    write_fixture_file(
        &path,
        r#"fn run(values: List) -> Integer {
    return 0
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(!output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let diagnostics: Vec<serde_json::Value> = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("valid json"))
        .collect();
    let boundary = diagnostics
        .iter()
        .find(|diag| {
            diag.get("code")
                .and_then(|value| value.as_str())
                .is_some_and(|code| code == "lang::ty::boundary_missing_type_args")
        })
        .expect("expected boundary generic diagnostic");
    assert_eq!(
        boundary
            .get("data")
            .and_then(|v| v.get("kind"))
            .and_then(|v| v.as_str()),
        Some("boundary_missing_type_args")
    );
    assert!(
        boundary
            .get("suggestions")
            .and_then(|v| v.as_array())
            .is_some_and(|arr| arr.iter().any(|s| {
                s.get("reason_code")
                    .and_then(|v| v.as_str())
                    .is_some_and(|code| code == "boundary_generic_rewrite")
            })),
        "expected boundary generic rewrite suggestion, got:\n{}",
        stdout
    );
}

#[test]
fn cli_json_reports_multifile_type_error_path() {
    let dir = workspace_tempdir();
    let src_dir = dir.path().join("src");
    let module_path = src_dir.join("domain").join("broken.wr");
    std::fs::create_dir_all(module_path.parent().unwrap()).unwrap();
    write_fixture_file(
        src_dir.join("main.wr"),
        r#"use compute from domain/broken

fn run() -> Integer {
    return compute()
}
"#,
    )
    .unwrap();
    write_fixture_file(
        &module_path,
        r#"fn padding0() -> Integer {
    return 0

}
fn padding1() -> Integer {
    return 1

}
fn padding2() -> Integer {
    return 2

}
fn compute() -> Integer {
    return true
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg("--error-format=json")
        .arg(src_dir.join("main.wr"))
        .output()
        .expect("run wrela");
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let diagnostics: Vec<serde_json::Value> = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("valid json"))
        .collect();
    let path_hit = diagnostics.iter().any(|diag| {
        diag.get("path")
            .and_then(|value| value.as_str())
            .is_some_and(|path| path.ends_with("domain/broken.wr"))
    });
    assert!(
        path_hit,
        "expected diagnostic path to point to imported module, got:\n{}",
        stdout
    );
}

#[test]
fn cli_json_multimodule_same_symbol_names_report_correct_owner_path() {
    let dir = workspace_tempdir();
    let src_dir = dir.path().join("src");
    let billing_path = src_dir.join("domain").join("billing.wr");
    let orders_path = src_dir.join("domain").join("orders.wr");
    std::fs::create_dir_all(billing_path.parent().unwrap()).unwrap();
    write_fixture_file(
        src_dir.join("main.wr"),
        r#"use compute from domain/orders

fn run() -> Integer {
    return compute()
}
"#,
    )
    .unwrap();
    write_fixture_file(
        &billing_path,
        r#"fn compute() -> Integer {
    return 1
}
"#,
    )
    .unwrap();
    write_fixture_file(
        &orders_path,
        r#"fn compute() -> Integer {
    return true
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg("--error-format=json")
        .arg(src_dir.join("main.wr"))
        .output()
        .expect("run wrela");
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let diagnostics: Vec<serde_json::Value> = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("valid json"))
        .collect();
    let orders_hit = diagnostics.iter().any(|diag| {
        diag.get("path")
            .and_then(|value| value.as_str())
            .is_some_and(|path| path.ends_with("domain/orders.wr"))
    });
    assert!(
        orders_hit,
        "expected diagnostic path to point at symbol owner module, got:\n{}",
        stdout
    );
}

#[test]
fn cli_exit_code_parse_error() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    write_fixture_file(
        &path,
        r#"fn run() -> Integer {
    return 1 +
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg(&path)
        .output()
        .expect("run wrela");
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn cli_exit_code_type_error() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    write_fixture_file(
        &path,
        r#"fn run() -> Integer {
    return 1 + true
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg(&path)
        .output()
        .expect("run wrela");
    assert_eq!(output.status.code(), Some(3));
}

#[test]
fn cli_check_success() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    write_fixture_file(
        &path,
        r#"fn run() -> Integer {
    return 1
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(output.status.success());
}

#[test]
fn cli_check_reports_lexical_invalid_character() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    write_fixture_file(
        &path,
        r#"fn run() -> Integer {
    return 1
}
$
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(
        !output.status.success(),
        "lexically invalid source should fail"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("lang::lex::error"), "{stderr}");
    assert!(stderr.contains("unexpected character '$'"), "{stderr}");
}

#[test]
fn cli_check_lexical_error_json_matches_snapshot() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().expect("src dir")).unwrap();
    write_fixture_file(
        &path,
        r#"fn run() -> Integer {
    return 1
}
$
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(!output.status.success(), "expected lexical check to fail");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let diagnostics: Vec<serde_json::Value> = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("valid json"))
        .collect();
    let lexical = diagnostics
        .iter()
        .find(|diag| {
            diag.get("code")
                .and_then(|value| value.as_str())
                .is_some_and(|code| code.starts_with("lang::lex::"))
        })
        .expect("expected lexical diagnostic");

    let code = lexical
        .get("code")
        .and_then(|value| value.as_str())
        .expect("lexical code");
    assert_eq!(code, "lang::lex::error");
    assert_eq!(
        lexical
            .get("rule")
            .and_then(|value| value.as_str())
            .expect("lexical rule"),
        "error"
    );
    assert_eq!(
        lexical
            .get("stage")
            .and_then(|value| value.as_str())
            .expect("stage"),
        "parse"
    );
    assert_eq!(
        lexical
            .get("severity")
            .and_then(|value| value.as_str())
            .expect("severity"),
        "error"
    );
    assert!(
        lexical
            .get("help")
            .and_then(|value| value.as_str())
            .is_some_and(|help| !help.is_empty()),
        "expected non-empty help field"
    );
    assert!(
        lexical
            .get("message")
            .and_then(|value| value.as_str())
            .is_some_and(|message| message.contains("unexpected character '$'")),
        "expected lexical subtype-specific message"
    );
    assert!(
        lexical
            .get("diag_id")
            .and_then(|value| value.as_str())
            .is_some_and(|diag_id| diag_id.contains("unexpected_character")),
        "expected lexical subtype marker in diag_id"
    );

    let normalized = normalize_lexical_diag_json_for_snapshot(lexical, dir.path());
    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "../fixtures/diagnostics/lexical_error_json_snapshot.json"
    ))
    .expect("valid expected snapshot json");
    assert_eq!(normalized, expected);
}

#[test]
fn cli_check_lexical_error_stderr_matches_snapshot() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().expect("src dir")).unwrap();
    write_fixture_file(
        &path,
        r#"fn run() -> Integer {
    return 1
}
$
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(!output.status.success(), "expected lexical check to fail");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("lang::lex::error"), "{stderr}");
    assert!(stderr.contains("unexpected character '$'"), "{stderr}");
    let normalized = normalize_temp_paths_for_snapshot(&stderr, dir.path());
    let expected =
        include_str!("../fixtures/diagnostics/lexical_error_stderr_snapshot.txt").trim_end();
    assert_eq!(normalized.trim_end(), expected);
}

#[test]
fn cli_check_without_run_is_ok() {
    let dir = workspace_tempdir();
    let path = dir.path().join("spec.wr");
    write_fixture_file(
        &path,
        r#"fn compute_value() -> Integer {
    return 1
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(output.status.success());
}

#[test]
fn cli_check_allows_duplicate_private_function_names_across_modules() {
    let dir = workspace_tempdir();
    let src_dir = dir.path().join("src");
    std::fs::create_dir_all(src_dir.join("domain")).unwrap();
    write_fixture_file(
        src_dir.join("main.wr"),
        r#"use run_orders from domain/orders
use run_payments from domain/payments

fn run() -> Integer {
    return run_orders() + run_payments()
}
"#,
    )
    .unwrap();
    write_fixture_file(
        src_dir.join("domain").join("orders.wr"),
        r#"private {
    fn load_value() -> Integer {
        return 1

    }
}
fn run_orders() -> Integer {
    return load_value()
}
"#,
    )
    .unwrap();
    write_fixture_file(
        src_dir.join("domain").join("payments.wr"),
        r#"private {
    fn load_value() -> Integer {
        return 2

    }
}
fn run_payments() -> Integer {
    return load_value()
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg(src_dir.join("main.wr"))
        .output()
        .expect("run wrela");
    assert!(
        output.status.success(),
        "duplicate private names across modules should be allowed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
