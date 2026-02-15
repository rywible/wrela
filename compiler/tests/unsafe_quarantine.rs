use std::path::PathBuf;
use std::process::Command;

fn run_governance_script(script_name: &str) -> std::process::Output {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let script = root.join("scripts").join("governance").join(script_name);
    Command::new("bash")
        .arg(script)
        .current_dir(&root)
        .output()
        .unwrap_or_else(|_| panic!("run governance script {script_name}"))
}

#[test]
fn unsafe_allowlist_gate_passes() {
    let output = run_governance_script("check_unsafe_allowlist.sh");

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn public_api_quarantine_gate_passes() {
    let output = run_governance_script("check_public_api_quarantine.sh");

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn v2_purity_gate_passes() {
    let output = run_governance_script("check_v2_purity.sh");

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn v2_no_cheating_gate_passes() {
    let output = run_governance_script("check_v2_no_cheating.sh");

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn v2_platform_boundary_gate_passes() {
    let output = run_governance_script("check_v2_platform_boundaries.sh");

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn v2_platform_contracts_gate_passes() {
    let output = run_governance_script("check_v2_platform_contracts.sh");

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn v2_parity_bootstrap_gate_passes() {
    let output = run_governance_script("check_v2_parity_bootstrap.sh");

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn v2_cli_bootstrap_gate_passes() {
    let output = run_governance_script("check_v2_cli_bootstrap.sh");

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn v2_check_pipeline_bootstrap_gate_passes() {
    let output = run_governance_script("check_v2_check_pipeline_bootstrap.sh");

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn phase0_abi_snapshot_gate_passes() {
    let output = run_governance_script("check_phase0_abi_snapshot.sh");

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn phase0_surface_wiring_gate_passes() {
    let output = run_governance_script("check_phase0_surface_wiring.sh");

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn v2_guardrails_gate_passes() {
    let output = run_governance_script("check_v2_guardrails.sh");

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
