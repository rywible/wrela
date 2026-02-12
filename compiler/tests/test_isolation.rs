use std::process::Command;

fn write_temp(path: &std::path::Path, contents: &str) {
    std::fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
    std::fs::write(path, contents).expect("write file");
}

fn latest_harness_dir(root: &std::path::Path) -> std::path::PathBuf {
    let base = root.join("target").join("wrela_tests");
    let mut dirs: Vec<(std::time::SystemTime, std::path::PathBuf)> = std::fs::read_dir(&base)
        .expect("read harness root")
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if !path.is_dir() {
                return None;
            }
            let modified = entry.metadata().ok()?.modified().ok()?;
            Some((modified, path))
        })
        .collect();
    dirs.sort_by_key(|(modified, _)| *modified);
    dirs.last()
        .map(|(_, path)| path.clone())
        .expect("at least one harness directory")
}

#[test]
fn test_runner_isolates_relative_paths_and_env_between_tests() {
    let root = tempfile::tempdir().expect("tempdir");
    write_temp(
        &root.path().join("src").join("main.wr"),
        "to run() -> Integer:\n    return 0\n",
    );
    write_temp(
        &root
            .path()
            .join("tests")
            .join("spec")
            .join("isolation_test.wr"),
        r#"to read_text_or(path: String, fallback: String) -> String:
    raw = __wr_fs_read_bytes(path) otherwise __wr_bytes_from_string(fallback)
    text = __wr_bytes_to_string(raw)
    match text:
        String:
            return text
        otherwise:
            return fallback

to read_env_or_default(key: String, fallback: String) -> String:
    raw = __wr_env_get(key)
    match raw:
        String:
            return raw
        otherwise:
            return fallback

to test_write_isolated_a() -> Nothing:
    __wr_env_set("WRELA_ISO_VAR", "from_a")
    payload = __wr_bytes_from_string("from_a")
    __wr_fs_write_bytes("out.txt", payload) otherwise nothing
    marker = read_text_or("out.txt", "missing")
    assert value marker == marker

to test_write_isolated_b() -> Nothing:
    observed = read_env_or_default("WRELA_ISO_VAR", "missing")
    observed_payload = __wr_bytes_from_string(observed)
    __wr_fs_write_bytes("observed.txt", observed_payload) otherwise nothing
    payload = __wr_bytes_from_string("from_b")
    __wr_fs_write_bytes("out.txt", payload) otherwise nothing
    marker = read_text_or("out.txt", "missing")
    assert value marker == marker
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("test")
        .arg(root.path())
        .arg("--jobs=1")
        .output()
        .expect("run wrela test");
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("tests: 2 passed, 0 failed"), "{stdout}");

    let harness_dir = latest_harness_dir(root.path());
    let cases_dir = harness_dir.join("cases");
    let mut out_values = Vec::new();
    let mut observed_values = Vec::new();
    for entry in std::fs::read_dir(&cases_dir).expect("read cases dir") {
        let entry = entry.expect("read case entry");
        if !entry.path().is_dir() {
            continue;
        }
        let out_path = entry.path().join("out.txt");
        if out_path.exists() {
            out_values.push(std::fs::read_to_string(out_path).expect("read out.txt"));
        }
        let observed_path = entry.path().join("observed.txt");
        if observed_path.exists() {
            observed_values
                .push(std::fs::read_to_string(observed_path).expect("read observed.txt"));
        }
    }
    out_values.sort();
    assert_eq!(out_values, vec!["from_a".to_string(), "from_b".to_string()]);
    assert_eq!(observed_values, vec!["missing".to_string()]);
}

#[test]
fn spec_lane_rejects_write_escape_outside_test_temp_root() {
    let root = tempfile::tempdir().expect("tempdir");
    write_temp(
        &root.path().join("src").join("main.wr"),
        "to run() -> Integer:\n    return 0\n",
    );
    write_temp(
        &root
            .path()
            .join("tests")
            .join("spec")
            .join("escape_test.wr"),
        r#"to test_escape_write_is_blocked() -> Nothing:
    payload = __wr_bytes_from_string("blocked")
    attempt = __wr_fs_write_bytes("../escape.txt", payload)
    match attempt:
        Err(err):
            assert value err == err
            return nothing
        otherwise:
            assert value 1 == 2
            return nothing
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("test")
        .arg(root.path())
        .output()
        .expect("run wrela test");
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !root.path().join("tests").join("escape.txt").exists(),
        "escape file should not have been created"
    );
}
