use std::process::{Command, Output};

fn write_temp(path: &std::path::Path, contents: &str) {
    std::fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
    std::fs::write(path, contents).expect("write file");
}

fn run_check(project_root: &std::path::Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg(project_root)
        .output()
        .expect("run wrela check")
}

fn output_text(output: &Output) -> String {
    format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn db_package_entrypoints_typecheck_from_project_code() {
    let root = tempfile::tempdir().expect("tempdir");

    write_temp(
        &root.path().join("src").join("main.wr"),
        r#"use {
    open,
    close,
    begin_txn
}
from pkg/db/core/kv
use {
    encode_text,
    decode_text,
    count_bytes,
    convert_to_byte_list,
    convert_from_byte_list
}
from pkg/db/core/codec

fn run() -> Integer {
    handle = open(".data/db-pkg-check")
    txn = begin_txn(handle)
    encoded = encode_text("codec-value")
    encoded_len = count_bytes(encoded)
    encoded_items = convert_to_byte_list(encoded)
    encoded_round_trip = convert_from_byte_list(encoded_items)
    decoded_result = decode_text(encoded_round_trip) ?? "decode-failed"
    close_result = close(handle) ?? nothing
    return 0
}
"#,
    );

    let output = run_check(root.path());
    assert!(output.status.success(), "{}", output_text(&output));
}

#[test]
fn db_core_import_path_remains_allowed_for_app_modules() {
    let root = tempfile::tempdir().expect("tempdir");

    write_temp(
        &root.path().join("src").join("main.wr"),
        r#"use open, close from pkg/db/core/kv

fn run() -> Integer {
    handle = open(".data/db-core-import-check")
    close_result = close(handle) ?? nothing
    return 1
}
"#,
    );

    let output = run_check(root.path());
    assert!(output.status.success(), "{}", output_text(&output));
}

#[test]
fn db_core_modules_cannot_import_admin_or_explain_modules() {
    let root = tempfile::tempdir().expect("tempdir");

    write_temp(
        &root.path().join("src").join("main.wr"),
        r#"use run_leaky from pkg/db/core/leaky

fn run() -> Integer {
    return run_leaky()
}
"#,
    );
    write_temp(
        &root
            .path()
            .join("src")
            .join("pkg")
            .join("db")
            .join("core")
            .join("leaky.wr"),
        r#"use plan_rehome from pkg/db/admin/cluster
use explain_policy from pkg/db/explain/policy

fn run_leaky() -> Integer {
    return 1
}
"#,
    );

    let output = run_check(root.path());
    assert!(!output.status.success(), "expected check to fail");
    let output_text = output_text(&output);
    assert!(
        output_text.contains("module 'pkg/db/core/leaky' cannot import admin/explain module"),
        "{}",
        output_text
    );
    assert!(
        output_text.contains("pkg/db/admin/cluster"),
        "{}",
        output_text
    );
    assert!(
        output_text.contains("pkg/db/explain/policy"),
        "{}",
        output_text
    );
}

#[test]
fn app_modules_cannot_import_db_admin_or_explain_kernels() {
    let root = tempfile::tempdir().expect("tempdir");

    write_temp(
        &root.path().join("src").join("main.wr"),
        r#"use plan_rehome_db from db/admin_kernel
use explain_policy_db from db/explain_kernel

fn run() -> Integer {
    return 1
}
"#,
    );

    let output = run_check(root.path());
    assert!(!output.status.success(), "expected check to fail");
    let output_text = output_text(&output);
    assert!(
        output_text.contains("module 'main' cannot import 'db/admin_kernel'"),
        "{}",
        output_text
    );
    assert!(
        output_text.contains("module 'main' cannot import 'db/explain_kernel'"),
        "{}",
        output_text
    );
    assert!(
        output_text.contains("only pkg/db/admin/*"),
        "{}",
        output_text
    );
    assert!(
        output_text.contains("db/explain/* may import"),
        "{}",
        output_text
    );
}

#[test]
fn app_modules_cannot_import_db_core_kernel() {
    let root = tempfile::tempdir().expect("tempdir");

    write_temp(
        &root.path().join("src").join("main.wr"),
        r#"use open_db from db/core_kernel

fn run() -> Integer {
    handle = open_db(".data/blocked-core-kernel")
    return handle
}
"#,
    );

    let output = run_check(root.path());
    assert!(!output.status.success(), "expected check to fail");
    let output_text = output_text(&output);
    assert!(
        output_text.contains("module 'main' cannot import 'db/core_kernel'"),
        "{}",
        output_text
    );
    assert!(output_text.contains("only pkg/db/"), "{}", output_text);
    assert!(
        output_text.contains("approved stdlib internals"),
        "{}",
        output_text
    );
}

#[test]
fn non_core_packages_cannot_import_db_core_kernel() {
    let root = tempfile::tempdir().expect("tempdir");

    write_temp(
        &root.path().join("src").join("main.wr"),
        r#"use run_leaky from pkg/db/admin/leaky

fn run() -> Integer {
    return run_leaky()
}
"#,
    );
    write_temp(
        &root
            .path()
            .join("src")
            .join("pkg")
            .join("db")
            .join("admin")
            .join("leaky.wr"),
        r#"use open_db from db/core_kernel

fn run_leaky() -> Integer {
    return open_db(".data/non-core-package-kernel")
}
"#,
    );

    let output = run_check(root.path());
    assert!(!output.status.success(), "expected check to fail");
    let output_text = output_text(&output);
    assert!(
        output_text.contains("module 'pkg/db/admin/leaky' cannot import 'db/core_kernel'"),
        "{}",
        output_text
    );
    assert!(output_text.contains("only pkg/db/"), "{}", output_text);
    assert!(
        output_text.contains("approved stdlib internals"),
        "{}",
        output_text
    );
}

#[test]
fn db_explain_package_typechecks_from_project_code() {
    let root = tempfile::tempdir().expect("tempdir");

    write_temp(
        &root.path().join("src").join("main.wr"),
        r#"use open from pkg/db/core/kv
use explain_policy from pkg/db/explain/policy

fn run() -> Integer {
    handle = open(".data/db-explain-check")
    return 1
}
"#,
    );

    let output = run_check(root.path());
    assert!(output.status.success(), "{}", output_text(&output));
}
