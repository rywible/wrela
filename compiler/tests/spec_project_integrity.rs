use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler crate should have repo parent")
        .to_path_buf()
}

#[test]
fn spec_project_layout_exists() {
    let root = repo_root();
    assert!(
        root.join("language/spec/src/main.wr").is_file(),
        "missing language/spec/src/main.wr"
    );
    assert!(
        root.join("language/spec/tests/spec/language_spec_test.wr")
            .is_file(),
        "missing language/spec/tests/spec/language_spec_test.wr"
    );
    assert!(
        !root.join("language/spec/spec.wr").exists(),
        "legacy language/spec/spec.wr should not exist"
    );
}

#[test]
fn spec_project_check_and_discovery_commands_are_valid() {
    let root = repo_root();
    let spec_root = root.join("language/spec");

    let check_output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg(&spec_root)
        .arg("--error-format=json")
        .output()
        .expect("run wrela check on spec project");
    assert!(
        check_output.status.success(),
        "spec project check failed: code={:?}\nstdout={}\nstderr={}",
        check_output.status.code(),
        String::from_utf8_lossy(&check_output.stdout),
        String::from_utf8_lossy(&check_output.stderr)
    );

    let list_output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("test")
        .arg(&spec_root)
        .arg("--lane=spec")
        .arg("--list")
        .arg("--jobs=1")
        .output()
        .expect("run wrela test --list on spec project");
    assert!(
        list_output.status.success(),
        "spec project test listing failed: code={:?}\nstdout={}\nstderr={}",
        list_output.status.code(),
        String::from_utf8_lossy(&list_output.stdout),
        String::from_utf8_lossy(&list_output.stderr)
    );

    let stdout = String::from_utf8_lossy(&list_output.stdout);
    assert!(
        stdout.contains("lane=spec"),
        "expected spec lane listings, got:\n{}",
        stdout
    );
}
