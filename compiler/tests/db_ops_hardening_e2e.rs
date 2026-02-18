use std::process::Command;

fn write_temp(path: &std::path::Path, contents: &str) {
    std::fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
    std::fs::write(path, contents).expect("write file");
}

#[test]
fn db_ops_hardening_packages_typecheck_from_project_code() {
    let root = tempfile::tempdir().expect("tempdir");

    write_temp(
        &root.path().join("src").join("main.wr"),
        r#"use create_throughput_gate_result, create_tail_latency_gate_result, create_phase_lineage from pkg/db/ops/perf_gate
use validate_secure_defaults, check_rotate_cert, check_rotate_key from pkg/db/ops/security
use create_checksum_mismatch_result, choose_target_severity, choose_repair_action, check_quarantine_target from pkg/db/ops/scrub
use validate_quota_inputs, create_retry_metadata, choose_fair_tenant from pkg/db/ops/tenant_qos

to run() -> Integer:
    perf_ok = create_throughput_gate_result(10000, 9800, 5)
    tail_ok = create_tail_latency_gate_result(2, 2, 15)
    lineage = create_phase_lineage("phase-9", "bench-001")
    secure_ok = validate_secure_defaults(1, 1, 1, 0)
    cert_due = check_rotate_cert(3600, 3300, 600)
    key_due = check_rotate_key(0, 90000, 86400)
    mismatch = create_checksum_mismatch_result(10, 11)
    severity = choose_target_severity("snapshot")
    repair = choose_repair_action("snapshot")
    quarantine = check_quarantine_target("snapshot", mismatch)
    quota_ok = validate_quota_inputs(2, 4, 100, 25)
    retry_contract = create_retry_metadata("QUOTA_WINDOW_EXCEEDED", 25)
    fair_pick = choose_fair_tenant("tenant-a", 10, 5, "tenant-b", 1, 0)
    if perf_ok == 1:
        if tail_ok == 1:
            if secure_ok == 1:
                if cert_due == 1:
                    if key_due == 1:
                        if mismatch == 1:
                            if severity == "critical":
                                if repair == "refetch_snapshot":
                                    if quarantine == 1:
                                        if lineage == "phase-9:bench-001":
                                            if quota_ok == 1:
                                                if retry_contract == "QUOTA_WINDOW_EXCEEDED:25":
                                                    if fair_pick == "tenant-b":
                                                        return 1
    return 0
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg(root.path())
        .output()
        .expect("run wrela check");

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
