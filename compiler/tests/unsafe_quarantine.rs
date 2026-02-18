use std::path::PathBuf;
use std::process::Command;

#[test]
fn unsafe_allowlist_gate_passes() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let script = root
        .join("scripts")
        .join("governance")
        .join("check_unsafe_allowlist.sh");
    let output = Command::new("bash")
        .arg(script)
        .current_dir(&root)
        .output()
        .expect("run unsafe allowlist check");

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn public_api_quarantine_gate_passes() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let script = root
        .join("scripts")
        .join("governance")
        .join("check_public_api_quarantine.sh");
    let output = Command::new("bash")
        .arg(script)
        .current_dir(&root)
        .output()
        .expect("run public api quarantine check");

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
