use super::*;

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
    assert!(stdout.contains("--kpi-check-fallback-max"));
    assert!(stdout.contains("--kpi-check-batch-min"));
    assert!(stdout.contains("--kpi-scheduler-p99-improve-min-pct"));
    assert!(stdout.contains("--kpi-rewrite-overhead-max-pct"));
    assert!(!stdout.contains("game <subcommand> <path>"));
    assert!(!stdout.contains("realtime <subcommand> <path>"));
    assert!(!stdout.contains("mmo <subcommand> <path>"));
    assert!(!stdout.contains("frontend <subcommand> <path>"));
    assert!(!stdout.contains("studio <subcommand> <path>"));
    assert!(!stdout.contains("agent-run <path>"));
    assert!(!stdout.contains("--render=NAME"));
    assert!(!stdout.contains("--host=NAME"));
    assert!(!stdout.contains("--client-runtime=MODE"));
    assert!(!stdout.contains("--shader-provenance"));
    assert!(!stdout.contains("--no-shortcuts"));
    assert!(!stdout.contains("--intent-v2"));
    assert!(!stdout.contains("--determinism"));
    assert!(!stdout.contains("--rollback"));
    assert!(!stdout.contains("--render-lane"));
    assert!(!stdout.contains("--asset-streaming"));
    assert!(!stdout.contains("--gpu-metrics"));
    assert!(!stdout.contains("--streaming-metrics"));
    assert!(stdout.contains("--list"));
    assert!(stdout.contains("--id=ID"));
    assert!(stdout.contains("--filter=PATTERN"));
    assert!(stdout.contains("--replay-trace"));
    assert!(stdout.contains("--integration-mode"));
    assert!(stdout.contains("run certification"));
    assert!(stdout.contains("query-contracts"));
    assert!(stdout.contains("collision-contracts"));
    assert!(stdout.contains("collision-plan"));
    assert!(stdout.contains("collision-run"));
    assert!(stdout.contains("preview <path>"));
    assert!(stdout.contains("frame <path>"));
    assert!(stdout.contains("frame-contracts <path>"));
    assert!(stdout.contains("--attachment-format=json|ppm"));
    assert!(stdout.contains("--json-report"));
    assert!(stdout.contains("presentation-plan"));
    assert!(stdout.contains("presentation-debug"));
    assert!(!stdout.contains("--no-certify"));
}
