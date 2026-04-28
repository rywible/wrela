use super::*;

fn write_eval_case_workspace(
    root: &std::path::Path,
    case_id: &str,
    include_tests: bool,
) -> std::path::PathBuf {
    let case_dir = root.join("cases").join(case_id);
    std::fs::create_dir_all(case_dir.join("src")).expect("create eval case src");
    write_fixture_file(
        case_dir.join("src/main.wr"),
        r#"fn run() -> Integer {
    return 1
}
"#,
    )
    .expect("write eval case main");
    if include_tests {
        std::fs::create_dir_all(case_dir.join("tests/spec")).expect("create eval case tests");
        write_fixture_file(
            case_dir.join("tests/spec/eval_test.wr"),
            r#"fn test_eval_case() -> Nothing {
    assert value 1 == 1
}
"#,
        )
        .expect("write eval case test");
    }
    case_dir
}

fn write_eval_corpus_v2_fixture(root: &std::path::Path) -> std::path::PathBuf {
    write_eval_case_workspace(root, "check_case", false);
    write_eval_case_workspace(root, "check_case_non_machine_win", false);
    let manifest_path = root.join("one_shot_corpus_v2.json");
    write_fixture_file(
        &manifest_path,
        r#"{
  "schema_version": 2,
  "suite_id": "eval_cli_fixture_v2",
  "cases": [
    {
      "id": "check_case",
      "workspace_dir": "cases/check_case",
      "command": "check",
      "target": ".",
      "max_loops": 2,
      "attempts": [
        {
          "id": "a1",
          "visible_to_agent": false,
          "machine_applicable": false,
          "writes": [
            {
              "path": "src/main.wr",
              "content": "fn run() -> Integer {\n    return 1 +\n}\n"
            }
          ],
          "deletes": []
        },
        {
          "id": "a2",
          "visible_to_agent": true,
          "machine_applicable": true,
          "writes": [
            {
              "path": "src/main.wr",
              "content": "fn run() -> Integer {\n    return 1\n}\n"
            }
          ],
          "deletes": []
        }
      ]
    },
    {
      "id": "check_case_non_machine_win",
      "workspace_dir": "cases/check_case_non_machine_win",
      "command": "check",
      "target": ".",
      "max_loops": 2,
      "attempts": [
        {
          "id": "a1",
          "visible_to_agent": true,
          "machine_applicable": true,
          "writes": [
            {
              "path": "src/main.wr",
              "content": "fn run() -> Integer {\n    return true\n}\n"
            }
          ],
          "deletes": []
        },
        {
          "id": "a2",
          "visible_to_agent": true,
          "machine_applicable": false,
          "writes": [
            {
              "path": "src/main.wr",
              "content": "fn run() -> Integer {\n    return 1\n}\n"
            }
          ],
          "deletes": []
        }
      ]
    }
  ]
}"#,
    )
    .expect("write one-shot corpus v2 fixture");
    manifest_path
}

#[test]
fn cli_eval_one_shot_rejects_v1_corpus_shape() {
    let dir = workspace_tempdir();
    let corpus_path = dir.path().join("one_shot_v1.json");
    write_fixture_file(
        &corpus_path,
        r#"[
  {
    "id": "legacy",
    "passed": true
  }
]"#,
    )
    .expect("write one-shot v1 fixture");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("eval")
        .arg("one-shot")
        .arg(&corpus_path)
        .arg("--error-format=json")
        .output()
        .expect("run eval with v1 corpus");
    assert!(!output.status.success(), "v1 corpus should be rejected");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unsupported one-shot corpus schema v1"),
        "missing hard-cut message:\n{}",
        stderr
    );
}

#[test]
fn cli_eval_one_shot_rejects_malformed_v2_manifest() {
    let dir = workspace_tempdir();
    let cases = [
        (
            "duplicate_case_id",
            r#"{
  "schema_version": 2,
  "suite_id": "bad",
  "cases": [
    {"id": "dup", "workspace_dir": "cases/a", "command": "check", "target": ".", "attempts": [{"id":"a1","noop":true}]},
    {"id": "dup", "workspace_dir": "cases/b", "command": "check", "target": ".", "attempts": [{"id":"a1","noop":true}]}
  ]
}"#,
            "duplicate one-shot case id",
        ),
        (
            "unsafe_write_path",
            r#"{
  "schema_version": 2,
  "suite_id": "bad",
  "cases": [
    {
      "id": "safe",
      "workspace_dir": "cases/safe",
      "command": "check",
      "target": ".",
      "attempts": [{"id":"a1","writes":[{"path":"../escape.wr","content":"x"}]}]
    }
  ]
}"#,
            "unsafe write path",
        ),
        (
            "empty_attempt_payload",
            r#"{
  "schema_version": 2,
  "suite_id": "bad",
  "cases": [
    {
      "id": "safe",
      "workspace_dir": "cases/safe",
      "command": "check",
      "target": ".",
      "attempts": [{"id":"a1","writes":[],"deletes":[],"noop":false}]
    }
  ]
}"#,
            "must define writes/deletes or set noop=true",
        ),
    ];

    for (name, body, expected_error) in cases {
        let manifest = dir.path().join(format!("{name}.json"));
        write_fixture_file(&manifest, body).expect("write malformed manifest");
        let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
            .arg("eval")
            .arg("one-shot")
            .arg(&manifest)
            .arg("--error-format=json")
            .output()
            .expect("run eval on malformed v2 manifest");
        assert!(
            !output.status.success(),
            "expected malformed manifest '{}' to fail",
            name
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(expected_error),
            "expected '{}' error for '{}', stderr:\n{}",
            expected_error,
            name,
            stderr
        );
    }
}

#[test]
fn cli_eval_one_shot_json_hash_is_stable() {
    let dir = workspace_tempdir();
    let corpus_path = write_eval_corpus_v2_fixture(dir.path());

    let first = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("eval")
        .arg("one-shot")
        .arg(&corpus_path)
        .arg("--runs=3")
        .arg("--error-format=json")
        .output()
        .expect("run first eval one-shot");
    assert!(
        first.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&first.stdout),
        String::from_utf8_lossy(&first.stderr)
    );
    let first_json = parse_single_json_stdout(&first.stdout);

    let second = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("eval")
        .arg("one-shot")
        .arg(&corpus_path)
        .arg("--runs=3")
        .arg("--error-format=json")
        .output()
        .expect("run second eval one-shot");
    assert!(
        second.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&second.stdout),
        String::from_utf8_lossy(&second.stderr)
    );
    let second_json = parse_single_json_stdout(&second.stdout);

    assert_eq!(
        first_json.get("report_hash"),
        second_json.get("report_hash"),
        "eval one-shot hash should be stable for deterministic reruns"
    );
    assert_eq!(
        first_json
            .get("schema_version")
            .and_then(|value| value.as_u64()),
        Some(2)
    );
    assert_eq!(
        first_json.get("suite_id").and_then(|value| value.as_str()),
        Some("eval_cli_fixture_v2")
    );
    assert_eq!(
        first_json.get("runs").and_then(|value| value.as_u64()),
        Some(3)
    );
    assert_eq!(
        first_json.get("pass_rate").and_then(|value| value.as_f64()),
        Some(0.5)
    );
    assert_eq!(
        first_json
            .get("machine_applicable_fix_apply_rate")
            .and_then(|value| value.as_f64()),
        Some(0.5)
    );
    let cases = first_json
        .get("cases")
        .and_then(|value| value.as_array())
        .expect("cases array");
    assert_eq!(cases.len(), 2);
    for case in cases {
        assert!(case.get("execution_ms_total").is_some());
    }
}

#[test]
fn cli_eval_one_shot_pretty_and_sarif_outputs_include_v2_contract() {
    let dir = workspace_tempdir();
    let corpus_path = write_eval_corpus_v2_fixture(dir.path());

    let pretty = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("eval")
        .arg("one-shot")
        .arg(&corpus_path)
        .arg("--error-format=human")
        .output()
        .expect("run eval one-shot pretty");
    assert!(
        pretty.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&pretty.stdout),
        String::from_utf8_lossy(&pretty.stderr)
    );
    let pretty_stdout = String::from_utf8_lossy(&pretty.stdout);
    assert!(pretty_stdout.contains("suite_id: eval_cli_fixture_v2"));
    assert!(pretty_stdout.contains("case=check_case"));

    let sarif = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("eval")
        .arg("one-shot")
        .arg(&corpus_path)
        .arg("--error-format=sarif")
        .output()
        .expect("run eval one-shot sarif");
    assert!(
        sarif.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&sarif.stdout),
        String::from_utf8_lossy(&sarif.stderr)
    );
    let sarif_json = parse_single_json_stdout(&sarif.stdout);
    assert_eq!(
        sarif_json.get("version").and_then(|value| value.as_str()),
        Some("2.1.0")
    );
    let message = sarif_json
        .get("runs")
        .and_then(|value| value.as_array())
        .and_then(|runs| runs.first())
        .and_then(|run| run.get("results"))
        .and_then(|value| value.as_array())
        .and_then(|results| results.first())
        .and_then(|result| result.get("message"))
        .and_then(|message| message.get("text"))
        .and_then(|value| value.as_str())
        .expect("sarif message text");
    assert!(message.contains("report_hash="));
    assert!(message.contains("suite=eval_cli_fixture_v2"));
}

#[test]
fn cli_save_refuses_static_epoch_zero_metadata_save() {
    let temp = workspace_tempdir();
    let project = temp.path().join("main.wr");
    write_fixture_file(
        &project,
        r#"
fn run() -> Integer {
    return 0
}
"#,
    )
    .expect("write project");
    let out = temp.path().join("save.cbor");

    let output = run_command_with_timeout(
        Command::new(env!("CARGO_BIN_EXE_wrela"))
            .arg("save")
            .arg(&project)
            .arg("--out")
            .arg(&out),
        std::time::Duration::from_secs(10),
    );

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("epoch 1") || stdout.contains("\"snapshot_epoch\":1"),
        "save should report a live non-zero epoch, stdout: {stdout}"
    );
    assert!(out.exists(), "live save should write {out:?}");
}
