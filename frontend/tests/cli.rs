use std::{
    io::Write,
    process::{Command, Stdio},
};

fn inspect(source: &[u8], json: bool) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_wrela_frontend"));
    if json {
        command.arg("--json");
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(source).unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn cli_acceptance_and_rejection_use_the_frontend_admission_contract() {
    let accepted = inspect(b"fn tick() -> Unit { return }\n", false);
    assert!(
        accepted.status.success(),
        "{}",
        String::from_utf8_lossy(&accepted.stderr)
    );
    assert!(
        String::from_utf8(accepted.stdout)
            .unwrap()
            .contains("syntax eligible: 1 declaration(s)")
    );
    let rejected = inspect(b"fn tick() -> Unit { let x = ; return }\n", false);
    assert_eq!(rejected.status.code(), Some(1));
    assert!(
        String::from_utf8(rejected.stdout)
            .unwrap()
            .contains("syntax rejected")
    );
    assert!(!rejected.stderr.is_empty());
}

#[test]
fn json_inspection_retains_invalid_bytes_without_claiming_eligibility() {
    let source = b"fn tick() -> Unit { return }\n\xff";
    let output = inspect(source, true);
    assert_eq!(output.status.code(), Some(1));
    let exported: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(exported["syntax_eligible"], false);
    let pieces = exported["source"]["pieces"].as_array().unwrap();
    let rendered: Vec<u8> = pieces
        .iter()
        .flat_map(|piece| {
            piece["bytes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|byte| byte.as_u64().unwrap() as u8)
        })
        .collect();
    assert_eq!(rendered, source);
    assert!(!exported["diagnostics"].as_array().unwrap().is_empty());
}

#[test]
fn nonexistent_file_reports_io_failure_separately() {
    let output = Command::new(env!("CARGO_BIN_EXE_wrela_frontend"))
        .arg("/wrela-test-path-that-does-not-exist/source.wr")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(!output.stderr.is_empty());
}
