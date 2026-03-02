use std::path::PathBuf;
use std::process::Command;

#[test]
fn contract_cli_surface_includes_required_flags() {
    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("--help")
        .output()
        .expect("run wrela --help");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("usage: wrela"));
    assert!(stdout.contains("analyze <path>"));
    assert!(stdout.contains("--json"));
    assert!(stdout.contains("--error-format=human|json|sarif"));
    assert!(stdout.contains("--workspace-diagnostics"));
    assert!(stdout.contains("--list"));
    assert!(stdout.contains("--id=ID"));
    assert!(stdout.contains("--filter=PATTERN"));
}

#[test]
fn contract_error_format_rejects_unknown_values() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src_dir = dir.path().join("src");
    std::fs::create_dir_all(&src_dir).expect("create src");
    std::fs::write(
        src_dir.join("main.wr"),
        "to run() -> Integer:\n    return 0\n",
    )
    .expect("write main.wr");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg(dir.path())
        .arg("--error-format=wat")
        .output()
        .expect("run check with invalid --error-format");

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("invalid --error-format value"));
    assert!(stderr.contains("expected one of"));
}

#[test]
fn contract_exit_codes_parse_and_type_errors() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src_dir = dir.path().join("src");
    std::fs::create_dir_all(&src_dir).expect("create src");

    let parse_file = src_dir.join("parse_fail.wr");
    std::fs::write(&parse_file, "fn run() -> Integer {\n    return 1 +\n}\n")
        .expect("write parse file");
    let parse_output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg(&parse_file)
        .output()
        .expect("run parse fail");
    assert_eq!(parse_output.status.code(), Some(2));

    let type_file = src_dir.join("type_fail.wr");
    std::fs::write(
        &type_file,
        "fn run() -> Integer {\n    return 1 + true\n}\n",
    )
    .expect("write type file");
    let type_output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg(&type_file)
        .output()
        .expect("run type fail");
    assert_eq!(type_output.status.code(), Some(3));
}

#[test]
fn contract_cert_report_schema_has_required_fields() {
    let cert_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("cert_schema_v3_example.json");
    let contents = std::fs::read_to_string(&cert_path).expect("read cert.json");
    let json: serde_json::Value = serde_json::from_str(&contents).expect("parse cert.json");

    for field in [
        "cert_schema_version",
        "entry_path",
        "workspace_root",
        "artifact_path",
        "tests_passed",
        "compiler_version",
        "runtime_version",
        "source_hash",
        "binary_hash",
    ] {
        assert!(
            json.get(field).is_some(),
            "missing required cert field: {field}"
        );
    }
}
