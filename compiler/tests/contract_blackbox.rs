use std::path::PathBuf;
use std::process::Command;

fn is_darwin_arm64_host() -> bool {
    cfg!(target_os = "macos") && cfg!(target_arch = "aarch64")
}

#[test]
fn contract_cli_surface_includes_required_flags() {
    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("--help")
        .output()
        .expect("run wrela --help");
    if is_darwin_arm64_host() {
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("usage: wrela <command> [options] <path> [-- args]"));
        assert!(stdout.contains("--format=json"));
    } else {
        assert_eq!(output.status.code(), Some(1));
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("error: m10 cutover is darwin-arm64 only"));
    }
}

#[test]
fn contract_exit_codes_parse_and_type_errors() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src_dir = dir.path().join("src");
    std::fs::create_dir_all(&src_dir).expect("create src");

    let parse_file = src_dir.join("parse_fail.wr");
    std::fs::write(&parse_file, "to run() -> Integer:\n    return 1 +\n")
        .expect("write parse file");
    let parse_output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg(&parse_file)
        .output()
        .expect("run parse fail");
    if is_darwin_arm64_host() {
        assert_eq!(parse_output.status.code(), Some(2));
    } else {
        assert_eq!(parse_output.status.code(), Some(1));
        let stderr = String::from_utf8_lossy(&parse_output.stderr);
        assert!(stderr.contains("error: m10 cutover is darwin-arm64 only"));
    }

    let type_file = src_dir.join("type_fail.wr");
    std::fs::write(&type_file, "to run() -> Integer:\n    return 1 + true\n")
        .expect("write type file");
    let type_output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg(&type_file)
        .output()
        .expect("run type fail");
    if is_darwin_arm64_host() {
        assert_eq!(type_output.status.code(), Some(3));
    } else {
        assert_eq!(type_output.status.code(), Some(1));
        let stderr = String::from_utf8_lossy(&type_output.stderr);
        assert!(stderr.contains("error: m10 cutover is darwin-arm64 only"));
    }
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

#[test]
fn contract_phase0_fs_process_intrinsics_typecheck() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src_dir = dir.path().join("src");
    std::fs::create_dir_all(&src_dir).expect("create src");
    let source = r#"to run_helper() -> Nothing:
    __wr_process_exit(0)

to run() -> Integer:
    __wr_fs_mkdir_all("tmp") otherwise nothing
    __wr_fs_read_dir(".") otherwise []
    __wr_fs_metadata("tmp") otherwise __wr_map_new()
    payload = __wr_bytes_from_string("x")
    __wr_fs_write_bytes("tmp/file", payload) otherwise nothing
    __wr_fs_rename("tmp/file", "tmp/file2") otherwise nothing
    __wr_fs_set_executable("tmp/file2") otherwise nothing
    __wr_fs_remove_file("tmp/file2") otherwise nothing
    __wr_fs_remove_dir_all("tmp") otherwise nothing
    cwd = __wr_process_cwd() otherwise ""
    argument_values = __wr_process_argv()
    specs = __wr_map_new()
    __wr_map_set(specs, "args", argument_values)
    __wr_process_run(specs) otherwise __wr_map_new()
    if __wr_str_len(cwd) >= 0:
        return __wr_str_len(cwd) + __wr_map_len(specs)
    return 0
"#;
    let source_path = src_dir.join("main.wr");
    std::fs::write(&source_path, source).expect("write source");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg(&source_path)
        .output()
        .expect("run check");
    assert!(
        output.status.success(),
        "expected check success, stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
}
