use std::path::PathBuf;
use std::process::Command;

#[test]
fn help_text_matches_snapshot_exactly() {
    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("--help")
        .output()
        .expect("run wrela --help");
    assert!(output.status.success());

    let expected_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("help_text_snapshot.txt");
    let expected = std::fs::read_to_string(expected_path).expect("read help snapshot");
    let actual = String::from_utf8_lossy(&output.stdout).to_string();
    assert_eq!(actual, expected);
}
