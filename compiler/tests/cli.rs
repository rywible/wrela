use std::process::Command;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

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
}

#[test]
fn cli_init_creates_project() {
    let dir = tempfile::tempdir().expect("tempdir");
    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("init")
        .arg(dir.path())
        .output()
        .expect("run wrela");
    assert!(output.status.success());
    let main_path = dir.path().join("src").join("main.wr");
    assert!(main_path.exists());
}

#[test]
fn cli_json_diagnostics() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("main.wr");
    std::fs::write(&path, "to run() -> Integer:\n    return 1 +\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("--format=json")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let first = stdout.lines().next().expect("json output");
    let value: serde_json::Value = serde_json::from_str(first).expect("valid json");
    assert!(value.get("message").is_some());
    assert!(value.get("span").is_some());
    assert!(value.get("code").is_none());
    assert!(value.get("rule").is_none());
    assert!(value.get("help").is_none());
    assert!(value.get("suggestions").is_none());
}

#[test]
fn cli_json_naming_diagnostics_include_metadata_fields_when_present() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("main.wr");
    std::fs::write(
        &path,
        "to BadName() -> Integer:\n    let AlsoBad = 1\n    return AlsoBad\n",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg("--format=json")
        .arg(&path)
        .output()
        .expect("run wrela");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let diagnostics: Vec<serde_json::Value> = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("valid json"))
        .collect();

    let naming: Vec<&serde_json::Value> = diagnostics
        .iter()
        .filter(|diag| {
            diag.get("code")
                .and_then(|value| value.as_str())
                .is_some_and(|code| code.starts_with("lang::naming::"))
        })
        .collect();

    if naming.is_empty() {
        return;
    }

    for diag in naming {
        let code = diag
            .get("code")
            .and_then(|value| value.as_str())
            .expect("naming diagnostic has code");
        assert!(code.starts_with("lang::naming::"));
        assert!(
            diag.get("rule")
                .and_then(|value| value.as_str())
                .is_some_and(|rule| !rule.is_empty())
        );
        assert!(diag.get("help").is_some());
        let suggestions = diag
            .get("suggestions")
            .and_then(|value| value.as_array())
            .expect("naming diagnostic has suggestions array");
        for suggestion in suggestions {
            assert!(suggestion.get("replacement").is_some());
            assert!(suggestion.get("span").is_some());
            assert!(suggestion.get("rationale").is_some());
            assert!(suggestion.get("confidence").is_some());
        }
    }
}

#[test]
fn cli_exit_code_parse_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "to run() -> Integer:\n    return 1 +\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg(&path)
        .output()
        .expect("run wrela");
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn cli_exit_code_type_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "to run() -> Integer:\n    return 1 + true\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg(&path)
        .output()
        .expect("run wrela");
    assert_eq!(output.status.code(), Some(3));
}

#[test]
fn cli_check_success() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "to run() -> Integer:\n    return 1\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(output.status.success());
}

#[test]
fn cli_check_without_run_is_ok() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("spec.wr");
    std::fs::write(&path, "to compute_value() -> Integer:\n    return 1\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(output.status.success());
}

fn write_test_project(root: &std::path::Path) {
    let src_dir = root.join("src");
    let tests_dir = root.join("tests");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::create_dir_all(&tests_dir).unwrap();
    std::fs::write(
        src_dir.join("main.wr"),
        "to run() -> Integer:\n    return 0\n",
    )
    .unwrap();
    std::fs::write(
        tests_dir.join("basic.wr"),
        "to test_basic() -> Nothing:\n    assert value 1 == 1\n",
    )
    .unwrap();
}

#[test]
fn cli_test_perf_summary() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_test_project(dir.path());
    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("test")
        .arg(dir.path())
        .output()
        .expect("run wrela");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("p50_ns="));
    assert!(stdout.contains("p95_ns="));
    assert!(stdout.contains("p99_ns="));
    assert!(stdout.contains("allocs/request="));
}

#[test]
fn cli_test_perf_debug() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_test_project(dir.path());
    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("test")
        .arg("--perf-debug")
        .arg(dir.path())
        .output()
        .expect("run wrela");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("perf-debug:"));
    assert!(stdout.contains("rc_inc="));
    assert!(stdout.contains("mailbox_enqueue_ok="));
    assert!(stdout.contains("alloc_list="));
}

#[test]
fn cli_perf_writes_baseline_json() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_test_project(dir.path());
    let baseline = dir.path().join("baseline.json");
    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("perf")
        .arg("--runs=1")
        .arg(format!("--baseline-out={}", baseline.display()))
        .arg(".")
        .output()
        .expect("run wrela");
    assert!(output.status.success(), "{:?}", output);
    assert!(baseline.exists());

    let bytes = std::fs::read(&baseline).expect("read baseline");
    let json: serde_json::Value = serde_json::from_slice(&bytes).expect("valid baseline json");
    assert!(json.get("summary").is_some());
    let summary = json.get("summary").expect("summary");
    assert!(summary.get("compile_throughput_tests_per_sec").is_some());
    assert!(summary.get("runtime_p50_ns").is_some());
    assert!(summary.get("runtime_p95_ns").is_some());
    assert!(summary.get("runtime_p99_ns").is_some());
}

#[test]
fn cli_perf_gate_fails_with_synthetic_slowdown() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_test_project(dir.path());
    let baseline = dir.path().join("baseline.json");
    let baseline_output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("perf")
        .arg("--runs=1")
        .arg(format!("--baseline-out={}", baseline.display()))
        .arg(".")
        .output()
        .expect("run baseline");
    assert!(baseline_output.status.success());

    let pass_output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("test")
        .arg(format!("--perf-gate={}", baseline.display()))
        .arg("--perf-max-regression-pct=10000")
        .arg(".")
        .output()
        .expect("run pass gate");
    assert!(pass_output.status.success());

    let fail_output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .env("WRELA_TEST_SLOWDOWN_MS", "1200")
        .arg("test")
        .arg(format!("--perf-gate={}", baseline.display()))
        .arg("--perf-max-regression-pct=0")
        .arg(".")
        .output()
        .expect("run fail gate");
    assert!(
        !fail_output.status.success(),
        "gate should fail with slowdown"
    );
    let stderr = String::from_utf8_lossy(&fail_output.stderr);
    assert!(stderr.contains("perf gate failed"));
}

#[test]
fn cli_test_single_file_runs() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("spec.wr");
    std::fs::write(
        &path,
        "to compute_value() -> Integer:\n    return 1\n\nto test_basic() -> Nothing:\n    assert value compute_value() == 1\n",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("test")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("spec::test_basic"));
}

#[test]
fn cli_test_single_file_without_tests_is_ok() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("spec.wr");
    std::fs::write(&path, "to compute_value() -> Integer:\n    return 1\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("test")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no tests found"));
    assert!(stderr.contains(&path.display().to_string()));
}

#[test]
fn cli_test_rejects_non_wr_file_target() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("spec.txt");
    std::fs::write(&path, "not wr").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("test")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("test file must have .wr extension"));
}

#[test]
fn cli_thin_core_bootstrap_matrix() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("src");
    let tests = dir.path().join("tests");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::create_dir_all(&tests).unwrap();

    std::fs::write(src.join("main.wr"), "to run() -> Integer:\n    return 0\n").unwrap();
    let entry = src.join("main.wr");
    std::fs::write(
        tests.join("basic.wr"),
        "to test_basic() -> Nothing:\n    assert value 1 == 1\n",
    )
    .unwrap();

    let check = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg(&entry)
        .output()
        .expect("run check");
    assert!(
        check.status.success(),
        "check failed: code={:?}\nstdout={}\nstderr={}",
        check.status.code(),
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );

    let bin = dir.path().join("thin_core_matrix_bin");
    let build = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("build")
        .arg(&entry)
        .arg("-o")
        .arg(bin.as_os_str())
        .output()
        .expect("run build");
    assert!(
        build.status.success(),
        "build failed: {:?}",
        build.status.code()
    );
    assert!(bin.exists());

    let run_status = Command::new(&bin).status().expect("run built binary");
    assert_eq!(run_status.code(), Some(0));

    let test = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("test")
        .arg(dir.path())
        .output()
        .expect("run test");
    assert!(
        test.status.success(),
        "test failed: {:?}",
        test.status.code()
    );
}

#[test]
fn cli_exit_code_naming_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "to helper() -> Integer:\n    return 1\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert_eq!(output.status.code(), Some(3));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("must start with a verb"));
}

#[test]
fn cli_naming_bypass_allows_main_and_configure() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        &path,
        "A Logger:\n    can __configure__() -> Nothing:\n        return\n\nto main() -> Integer:\n    return 0\n",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(output.status.success());
}

#[cfg(unix)]
fn write_executable(path: &std::path::Path, body: &str) {
    std::fs::write(path, body).expect("write script");
    let mut perms = std::fs::metadata(path).expect("metadata").permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms).expect("chmod");
}

#[cfg(unix)]
fn setup_matrix_stubs(root: &std::path::Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let cargo_stub = root.join("cargo-stub.sh");
    let wrlea_stub = root.join("wrela-stub.sh");
    write_executable(
        &cargo_stub,
        r#"#!/bin/sh
set -eu
echo "cargo:$*" >> "$WRELA_MATRIX_STUB_LOG"
if [ "${WRELA_MATRIX_FAIL_STEP:-}" = "cargo" ]; then
  exit 9
fi
exit 0
"#,
    );
    write_executable(
        &wrlea_stub,
        r#"#!/bin/sh
set -eu
echo "wrela:$*" >> "$WRELA_MATRIX_STUB_LOG"
cmd="${1:-}"
if [ "${WRELA_MATRIX_FAIL_STEP:-}" = "$cmd" ]; then
  exit 7
fi
if [ "$cmd" = "perf" ]; then
  baseline=""
  for arg in "$@"; do
    case "$arg" in
      --baseline-out=*)
        baseline="${arg#--baseline-out=}"
        ;;
    esac
  done
  if [ -n "$baseline" ]; then
    mkdir -p "$(dirname "$baseline")"
    printf '{"sample_count":1,"compile_throughput_tests_per_sec":1.0,"runtime_p50_ns":1,"runtime_p95_ns":1,"runtime_p99_ns":1,"allocs_per_request":0.0,"rc_inc":0,"rc_dec":0,"rc_ops_total":0,"dispatch_hit_ratio":1.0,"check_fallback_rate":0.1,"avg_check_batch_size":8.0,"check_oracle_eval_ns_p50":50,"check_oracle_eval_ns_p95":90,"effect_annihilation_rewrite_count":2,"scheduler_dispatch_p99_ns":1000,"scheduler_starvation_violations":0,"rewrite_compile_overhead_pct":3.0,"rewrite_applied_count":12,"metrics":{"messages_sent":0,"messages_dropped":0,"pending_resolved":0,"pending_dropped":0,"mailbox_high_water":0,"rc_inc":0,"rc_dec":0,"alloc_list":0,"alloc_map":0,"alloc_string":0,"alloc_bytes":0,"alloc_result":0,"alloc_pending":0,"mailbox_enqueue_ok":0,"mailbox_enqueue_fail":0,"mailbox_dequeue":0,"sched_dispatched":0,"sched_skipped_no_credit":0,"sched_profile_switch":0,"sched_starvation_violation":0,"sched_cross_shard_migration":0,"abi_typed_lane":0,"abi_boxed_lane":0}}' > "$baseline"
  fi
fi
exit 0
"#,
    );
    (cargo_stub, wrlea_stub)
}

#[cfg(unix)]
#[test]
fn cli_matrix_writes_evidence_bundle() {
    let dir = tempfile::tempdir().expect("tempdir");
    let log_path = dir.path().join("matrix-stub.log");
    let (cargo_stub, wrlea_stub) = setup_matrix_stubs(dir.path());

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("matrix")
        .env("WRELA_MATRIX_CARGO_BIN", &cargo_stub)
        .env("WRELA_MATRIX_SELF_BIN", &wrlea_stub)
        .env("WRELA_MATRIX_STUB_LOG", &log_path)
        .output()
        .expect("run matrix");
    assert!(
        output.status.success(),
        "matrix failed: code={:?}\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let latest = dir.path().join(".artifacts/matrix/matrix-latest.json");
    assert!(latest.exists());
    let json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&latest).expect("read bundle")).expect("bundle json");
    assert_eq!(json.get("success").and_then(|v| v.as_bool()), Some(true));
    assert_eq!(json.get("exit_code").and_then(|v| v.as_i64()), Some(0));
    assert_eq!(
        json.get("steps")
            .and_then(|v| v.as_array())
            .map(|steps| steps.len()),
        Some(3)
    );
    assert!(
        json.get("perf_summary")
            .and_then(|v| v.as_object())
            .is_some()
    );
    assert!(
        json.get("check_lane_kpis")
            .and_then(|v| v.as_object())
            .is_some()
    );
    let baseline = json
        .get("perf_baseline_path")
        .and_then(|v| v.as_str())
        .expect("baseline path");
    assert!(std::path::Path::new(baseline).exists());

    let invocations = std::fs::read_to_string(log_path).expect("read invocation log");
    assert!(invocations.contains("cargo:test --workspace"));
    assert!(invocations.contains("wrela:test language/spec/spec.wr"));
    assert!(invocations.contains("wrela:perf --runs=1"));
}

#[cfg(unix)]
#[test]
fn cli_matrix_forwards_perf_gate_flags() {
    let dir = tempfile::tempdir().expect("tempdir");
    let log_path = dir.path().join("matrix-stub.log");
    let gate = dir.path().join("gate-baseline.json");
    std::fs::write(&gate, "{}").expect("write gate");
    let (cargo_stub, wrlea_stub) = setup_matrix_stubs(dir.path());

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("matrix")
        .arg(format!("--perf-gate={}", gate.display()))
        .arg("--perf-max-regression-pct=12.5")
        .arg("--kpi-check-fallback-max=0.20")
        .arg("--kpi-check-batch-min=6")
        .arg("--kpi-scheduler-p99-improve-min-pct=10")
        .arg("--kpi-rewrite-overhead-max-pct=5")
        .env("WRELA_MATRIX_CARGO_BIN", &cargo_stub)
        .env("WRELA_MATRIX_SELF_BIN", &wrlea_stub)
        .env("WRELA_MATRIX_STUB_LOG", &log_path)
        .output()
        .expect("run matrix");
    assert!(output.status.success());
    let invocations = std::fs::read_to_string(log_path).expect("read invocation log");
    assert!(invocations.contains(&format!("--perf-gate={}", gate.display())));
    assert!(invocations.contains("--perf-max-regression-pct=12.5"));
    assert!(invocations.contains("--kpi-check-fallback-max=0.2"));
    assert!(invocations.contains("--kpi-check-batch-min=6"));
    assert!(invocations.contains("--kpi-scheduler-p99-improve-min-pct=10"));
    assert!(invocations.contains("--kpi-rewrite-overhead-max-pct=5"));

    let latest = dir.path().join(".artifacts/matrix/matrix-latest.json");
    let json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&latest).expect("read bundle")).expect("bundle");
    assert_eq!(
        json.get("kpi_thresholds")
            .and_then(|v| v.get("check_fallback_max"))
            .and_then(|v| v.as_f64()),
        Some(0.2)
    );
}

#[cfg(unix)]
#[test]
fn cli_matrix_stops_on_failed_step_and_persists_evidence() {
    let dir = tempfile::tempdir().expect("tempdir");
    let log_path = dir.path().join("matrix-stub.log");
    let (cargo_stub, wrlea_stub) = setup_matrix_stubs(dir.path());

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("matrix")
        .env("WRELA_MATRIX_CARGO_BIN", &cargo_stub)
        .env("WRELA_MATRIX_SELF_BIN", &wrlea_stub)
        .env("WRELA_MATRIX_STUB_LOG", &log_path)
        .env("WRELA_MATRIX_FAIL_STEP", "test")
        .output()
        .expect("run matrix");
    assert!(!output.status.success());

    let latest = dir.path().join(".artifacts/matrix/matrix-latest.json");
    assert!(latest.exists());
    let json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&latest).expect("read bundle")).expect("bundle json");
    assert_eq!(json.get("success").and_then(|v| v.as_bool()), Some(false));
    assert_eq!(
        json.get("steps")
            .and_then(|v| v.as_array())
            .map(|steps| steps.len()),
        Some(2)
    );
}
