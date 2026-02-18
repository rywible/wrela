use std::process::Command;

fn write_temp(path: &std::path::Path, contents: &str) {
    std::fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
    std::fs::write(path, contents).expect("write file");
}

#[test]
fn db_package_entrypoints_typecheck_from_project_code() {
    let root = tempfile::tempdir().expect("tempdir");

    write_temp(
        &root.path().join("src").join("main.wr"),
        r#"use:
    open,
    close,
    put,
    get,
    scan,
    begin_txn,
    prepare_txn,
    commit_txn,
    abort_txn,
    start_snapshot,
    get_snapshot_status,
    restore_snapshot
from pkg/db/core/kv
use:
    encode_text,
    decode_text,
    count_bytes,
    convert_to_byte_list,
    convert_from_byte_list
from pkg/db/core/codec

to run() -> Integer:
    handle = open(".data/db-pkg-check")
    write_version = put(handle, "core", "key", "value", -1)
    read_result = get(handle, "core", "key")
    scan_rows = scan(handle, "core", "a", "z", 5)
    encoded = encode_text("codec-value")
    encoded_len = count_bytes(encoded)
    encoded_items = convert_to_byte_list(encoded)
    encoded_round_trip = convert_from_byte_list(encoded_items)
    decoded_result = decode_text(encoded_round_trip) otherwise "decode-failed"

    txn = begin_txn(handle)
    if txn > 0:
        prepare_result = prepare_txn(handle, txn) otherwise nothing
        commit_result = commit_txn(handle, txn) otherwise nothing
    otherwise:
        abort_result = abort_txn(handle, txn) otherwise nothing

    snapshot = start_snapshot(handle)
    if snapshot > 0:
        snapshot_progress = get_snapshot_status(handle, snapshot)
        restore_result = restore_snapshot(handle, snapshot) otherwise nothing

    close_result = close(handle) otherwise nothing
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

#[test]
fn db_admin_and_policy_packages_typecheck_from_project_code() {
    let root = tempfile::tempdir().expect("tempdir");

    write_temp(
        &root.path().join("src").join("main.wr"),
        r#"use derive_policy_hash_seed, validate_shard_count from pkg/db/policy/routing
use create_safety_gate_result from pkg/db/policy/safety
use compile_routing_policy_seed from pkg/db/admin/routing
use create_safety_review_result from pkg/db/admin/safety
use get_cluster_health_summary from pkg/db/admin/cluster
use is_residency_audit_allowed from pkg/db/admin/residency
use choose_strong_read_mode, choose_bounded_stale_read_mode from pkg/db/client/read_mode

to run() -> Integer:
    direct_seed = derive_policy_hash_seed("orders-v1", 64)
    valid_count = validate_shard_count(64)
    policy_safe = create_safety_gate_result(1, 1, 0, 0)
    admin_seed = compile_routing_policy_seed("orders-v1", 64)
    admin_safe = create_safety_review_result(1, 1, 0, 0)
    cluster_ok = get_cluster_health_summary(3, 2, 1, 0)
    residency_ok = is_residency_audit_allowed("")
    strong_mode = choose_strong_read_mode()
    stale_mode = choose_bounded_stale_read_mode(50)
    if valid_count == 1:
        if policy_safe == 1:
            if admin_safe == 1:
                if cluster_ok == 1:
                    if residency_ok == 1:
                        if strong_mode == "strong":
                            if stale_mode == "bounded_stale":
                                if admin_seed == direct_seed:
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
