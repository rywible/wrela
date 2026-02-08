use std::process::Command;

#[test]
fn cli_version() {
    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("--version")
        .output()
        .expect("run wrela");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.starts_with("wrela "));
}

#[test]
fn cli_help() {
    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("--help")
        .output()
        .expect("run wrela");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("usage: wrela"));
}

#[test]
fn cli_init_creates_project() {
    let dir = tempfile::tempdir().expect("tempdir");
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
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("main.wr");
    std::fs::write(&path, "to run() -> Integer:\n    return 1 +\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("--format=json")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let first = stdout.lines().next().expect("json output");
    let value: serde_json::Value = serde_json::from_str(first).expect("valid json");
    assert!(value.get("message").is_some());
    assert!(value.get("span").is_some());
}

#[test]
fn cli_exit_code_parse_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "to run() -> Integer:\n    return 1 +\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg(&path)
        .output()
        .expect("run wrela");
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn cli_exit_code_type_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "to run() -> Integer:\n    return 1 + true\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg(&path)
        .output()
        .expect("run wrela");
    assert_eq!(output.status.code(), Some(3));
}

#[test]
fn cli_check_success() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "to run() -> Integer:\n    return 1\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(output.status.success());
}

#[test]
fn cli_check_without_run_is_ok() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("spec.wr");
    std::fs::write(&path, "to helper() -> Integer:\n    return 1\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(output.status.success());
}

fn write_test_project(root: &std::path::Path) {
    let src_dir = root.join("src");
    let tests_dir = root.join("tests");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::create_dir_all(&tests_dir).unwrap();
    std::fs::write(
        src_dir.join("main.wr"),
        "to run() -> Integer:\n    return 0\n",
    )
    .unwrap();
    std::fs::write(
        tests_dir.join("basic.wr"),
        "to test_basic() -> Nothing:\n    assert value 1 == 1\n",
    )
    .unwrap();
}

#[test]
fn cli_test_perf_summary() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_test_project(dir.path());
    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("test")
        .arg(dir.path())
        .output()
        .expect("run wrela");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("perf: p50_ns="));
    assert!(stdout.contains("p99_ns="));
    assert!(stdout.contains("allocs/request="));
}

#[test]
fn cli_test_perf_debug() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_test_project(dir.path());
    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("test")
        .arg("--perf-debug")
        .arg(dir.path())
        .output()
        .expect("run wrela");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("perf-debug:"));
    assert!(stdout.contains("rc_inc="));
    assert!(stdout.contains("mailbox_enqueue_ok="));
    assert!(stdout.contains("alloc_list="));
}

#[test]
fn cli_test_single_file_runs() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("spec.wr");
    std::fs::write(
        &path,
        "to helper() -> Integer:\n    return 1\n\nto test_basic() -> Nothing:\n    assert value helper() == 1\n",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("test")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("spec::test_basic"));
}

#[test]
fn cli_test_single_file_without_tests_is_ok() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("spec.wr");
    std::fs::write(&path, "to helper() -> Integer:\n    return 1\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("test")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no tests found"));
    assert!(stderr.contains(&path.display().to_string()));
}

#[test]
fn cli_test_rejects_non_wr_file_target() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("spec.txt");
    std::fs::write(&path, "not wr").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("test")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("test file must have .wr extension"));
}

#[test]
fn cli_thin_core_bootstrap_matrix() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("src");
    let tests = dir.path().join("tests");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::create_dir_all(&tests).unwrap();

    std::fs::write(src.join("main.wr"), "to run() -> Integer:\n    return 0\n").unwrap();
    let entry = src.join("main.wr");
    std::fs::write(
        tests.join("basic.wr"),
        "to test_basic() -> Nothing:\n    assert value 1 == 1\n",
    )
    .unwrap();

    let check = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg(&entry)
        .output()
        .expect("run check");
    assert!(
        check.status.success(),
        "check failed: code={:?}\nstdout={}\nstderr={}",
        check.status.code(),
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );

    let bin = dir.path().join("thin_core_matrix_bin");
    let build = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("build")
        .arg(&entry)
        .arg("-o")
        .arg(bin.as_os_str())
        .output()
        .expect("run build");
    assert!(
        build.status.success(),
        "build failed: {:?}",
        build.status.code()
    );
    assert!(bin.exists());

    let run_status = Command::new(&bin).status().expect("run built binary");
    assert_eq!(run_status.code(), Some(0));

    let test = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("test")
        .arg(dir.path())
        .output()
        .expect("run test");
    assert!(
        test.status.success(),
        "test failed: {:?}",
        test.status.code()
    );
}
