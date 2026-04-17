use super::diagnostics::assert_sarif_log_contract;
use super::*;

#[test]
fn cli_test_single_file_is_rejected() {
    let dir = workspace_tempdir();
    let path = dir.path().join("spec.wr");
    write_fixture_file(
        &path,
        r#"fn compute_value() -> Integer {
    return 1

}
fn test_basic() -> Nothing {
    assert value compute_value() == 1
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("test")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(
        !output.status.success(),
        "single-file test target should fail"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("requires a project-root directory"));
}

#[test]
fn cli_test_single_file_without_tests_is_rejected() {
    let dir = workspace_tempdir();
    let path = dir.path().join("spec.wr");
    write_fixture_file(
        &path,
        r#"fn compute_value() -> Integer {
    return 1
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("test")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(
        !output.status.success(),
        "single-file test target should fail"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("requires a project-root directory"));
    assert!(stderr.contains(&path.display().to_string()));
}

#[test]
fn cli_build_single_file_is_rejected() {
    let dir = workspace_tempdir();
    let path = dir.path().join("spec.wr");
    write_fixture_file(
        &path,
        r#"fn run() -> Integer {
    return 1
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("build")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(
        !output.status.success(),
        "single-file build target should fail"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("requires project layout (`src/**`)"));
}

#[test]
fn cli_test_oracle_gate_fails_when_test_has_no_assert_or_require() {
    let dir = workspace_tempdir();
    write_oracle_gate_project(dir.path(), false);

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("test")
        .arg(".")
        .output()
        .expect("run wrela");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("oracle gate failed"));
    assert!(stderr.contains("tests/oracle_gate::test_oracle_gate"));
}

#[test]
fn cli_test_oracle_gate_passes_when_assert_is_present() {
    let dir = workspace_tempdir();
    write_oracle_gate_project(dir.path(), true);

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("test")
        .arg(".")
        .output()
        .expect("run wrela");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("tests/oracle_gate::test_oracle_gate"));
    assert!(stdout.contains("tests: 1 passed, 0 failed"));
}

#[test]
fn cli_check_rejects_trivial_assert_in_certified_flow() {
    let dir = workspace_tempdir();
    let path = dir.path().join("main.wr");
    write_fixture_file(
        &path,
        r#"fn run() -> Integer {
    return 0

}
fn test_trivial() -> Nothing {
    assert value 1 == 1
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("certified tests cannot compare two literals in an assert"));
}

#[test]
fn cli_check_accepts_meaningful_assert_in_certified_flow() {
    let dir = workspace_tempdir();
    let path = dir.path().join("main.wr");
    write_fixture_file(
        &path,
        r#"fn run() -> Integer {
    return 0

}
fn compute_value() -> Integer {
    return 1

}
fn test_meaningful() -> Nothing {
    assert value compute_value() == 1
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(output.status.success());
}

#[test]
fn cli_test_discovery_ignores_to_test_in_comments_and_strings() {
    let dir = workspace_tempdir();
    let src = dir.path().join("src");
    let tests = dir.path().join("tests");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::create_dir_all(&tests).unwrap();
    write_fixture_file(
        src.join("main.wr"),
        r#"fn run() -> Integer {
    return 0
}
"#,
    )
    .unwrap();
    write_fixture_file(
        tests.join("discovery_test.wr"),
        r#"fn helper() -> Integer {
    return 1

}
// to test_comment_fake() -> Nothing:

fn test_real() -> Nothing {
    assert value helper() == 1
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("test")
        .arg("--list")
        .arg(".")
        .output()
        .expect("run wrela");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("name=tests/discovery::test_real"));
    assert!(!stdout.contains("test_string_fake"));
    assert!(!stdout.contains("test_comment_fake"));
}

#[test]
fn cli_test_discovery_rejects_parse_invalid_test_files() {
    let dir = workspace_tempdir();
    write_parse_invalid_test_discovery_project(dir.path());

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("test")
        .arg("--list")
        .arg(".")
        .output()
        .expect("run wrela");
    assert!(
        !output.status.success(),
        "discovery should fail for parse-invalid test files:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("parse-invalid test file detected during discovery"),
        "stderr missing hard-cut discovery message:\n{}",
        stderr
    );
    assert!(
        stderr.contains("tests/spec/broken_test.wr:"),
        "stderr missing parse-invalid file+span diagnostics:\n{}",
        stderr
    );
}

#[test]
fn cli_test_list_includes_autogen_generated_spec_tests() {
    let dir = workspace_tempdir();
    write_wrong_check_property_project(dir.path());

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("test")
        .arg("--list")
        .arg(".")
        .output()
        .expect("run wrela");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout
            .lines()
            .any(|line| line.contains("lane=spec") && line.contains("autogen")),
        "expected autogen-generated spec lane test in --list output, got:\n{}",
        stdout
    );
}

#[test]
fn cli_test_list_respects_autogen_case_budget_cap() {
    let dir = workspace_tempdir();
    write_wrong_check_property_project(dir.path());

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .env("WRELA_BUDGET_AUTOGEN_MAX_CASES", "2")
        .arg("test")
        .arg("--list")
        .arg(".")
        .output()
        .expect("run wrela");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let autogen_count = stdout
        .lines()
        .filter(|line| line.contains("autogen_case_"))
        .count();
    assert_eq!(
        autogen_count, 2,
        "expected autogen count to respect budget cap, got output:\n{}",
        stdout
    );
}

#[test]
fn cli_test_list_has_deterministic_registry_ids_and_lanes() {
    let dir = workspace_tempdir();
    write_test_registry_project(dir.path());

    let first = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("test")
        .arg("--list")
        .arg(".")
        .output()
        .expect("run wrela list first");
    assert!(first.status.success());
    let second = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("test")
        .arg("--list")
        .arg(".")
        .output()
        .expect("run wrela list second");
    assert!(second.status.success());
    assert_eq!(
        first.stdout, second.stdout,
        "expected deterministic list output"
    );

    let stdout = String::from_utf8_lossy(&first.stdout);
    assert!(stdout.contains("lane=spec name=tests/spec/alpha::test_alpha"));
    assert!(stdout.contains("lane=integration name=tests/integration/beta::test_beta"));
    assert!(stdout.contains("lane=sim name=tests/sim/gamma::test_gamma"));
    assert!(stdout.contains("lane=model name=tests/model/delta::test_delta"));
    assert!(stdout.contains("lane=default name=tests/misc/epsilon::test_epsilon"));
    let alpha_id = fnv1a64_hex(b"tests/spec/alpha::test_alpha");
    assert!(stdout.contains(&format!(
        "id={alpha_id} lane=spec name=tests/spec/alpha::test_alpha"
    )));
}

#[test]
fn cli_test_fast_lane_alias_selects_spec_and_default_tests() {
    let dir = workspace_tempdir();
    write_test_registry_project(dir.path());

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("test")
        .arg("--lane=fast")
        .arg("--list")
        .arg(".")
        .output()
        .expect("run wrela fast lane list");
    assert!(output.status.success(), "{:?}", output);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("lane=spec name=tests/spec/alpha::test_alpha"));
    assert!(stdout.contains("lane=default name=tests/misc/epsilon::test_epsilon"));
    assert!(!stdout.contains("tests/integration/beta::test_beta"));
    assert!(!stdout.contains("tests/sim/gamma::test_gamma"));
    assert!(!stdout.contains("tests/model/delta::test_delta"));
}

#[test]
fn cli_test_full_lane_alias_selects_all_legacy_lanes() {
    let dir = workspace_tempdir();
    write_test_registry_project(dir.path());

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("test")
        .arg("--lane=full")
        .arg("--list")
        .arg(".")
        .output()
        .expect("run wrela full lane list");
    assert!(output.status.success(), "{:?}", output);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("lane=spec name=tests/spec/alpha::test_alpha"));
    assert!(stdout.contains("lane=integration name=tests/integration/beta::test_beta"));
    assert!(stdout.contains("lane=sim name=tests/sim/gamma::test_gamma"));
    assert!(stdout.contains("lane=model name=tests/model/delta::test_delta"));
    assert!(stdout.contains("lane=default name=tests/misc/epsilon::test_epsilon"));
}

#[test]
fn cli_test_rejects_unknown_lane_with_updated_alias_list() {
    let dir = workspace_tempdir();
    write_test_registry_project(dir.path());

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("test")
        .arg("--lane=bogus")
        .arg(".")
        .output()
        .expect("run wrela invalid lane");
    assert!(!output.status.success(), "unexpected success: {:?}", output);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("error: invalid --lane value `bogus`"),
        "{stderr}"
    );
    assert!(
        stderr.contains("fast|full|spec|integration|sim|model|default"),
        "{stderr}"
    );
}

#[test]
fn cli_test_id_and_filter_select_deterministically() {
    let dir = workspace_tempdir();
    write_test_registry_project(dir.path());
    let beta_id = fnv1a64_hex(b"tests/integration/beta::test_beta");

    let by_id = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("test")
        .arg(format!("--id={beta_id}"))
        .arg(".")
        .output()
        .expect("run wrela id");
    assert!(by_id.status.success());
    let by_id_stdout = String::from_utf8_lossy(&by_id.stdout);
    assert!(by_id_stdout.contains("tests/integration/beta::test_beta"));
    assert!(!by_id_stdout.contains("tests/spec/alpha::test_alpha"));
    assert!(by_id_stdout.contains("tests: 1 passed, 0 failed"));

    let by_filter = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("test")
        .arg("--filter=tests/sim")
        .arg(".")
        .output()
        .expect("run wrela filter");
    assert!(by_filter.status.success());
    let by_filter_stdout = String::from_utf8_lossy(&by_filter.stdout);
    assert!(by_filter_stdout.contains("tests/sim/gamma::test_gamma"));
    assert!(!by_filter_stdout.contains("tests/model/delta::test_delta"));
    assert!(by_filter_stdout.contains("tests: 1 passed, 0 failed"));
}

#[test]
fn cli_test_forces_runtime_deterministic_env() {
    let dir = workspace_tempdir();
    let src = dir.path().join("src");
    let tests = dir.path().join("tests").join("spec");
    std::fs::create_dir_all(&src).expect("create src");
    std::fs::create_dir_all(&tests).expect("create tests/spec");
    write_fixture_file(
        src.join("main.wr"),
        r#"fn run() -> Integer {
    return 0
}
"#,
    )
    .expect("write src/main.wr");
    write_fixture_file(
        tests.join("runtime_deterministic_test.wr"),
        r#"fn test_runtime_deterministic_env() -> Nothing {
    mode = __wr_env_get("WRELA_RUNTIME_DETERMINISTIC")
    assert value mode == "1"
}
"#,
    )
    .expect("write test");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("test")
        .arg("--filter=runtime_deterministic_env")
        .arg(".")
        .output()
        .expect("run wrela test");
    assert!(
        output.status.success(),
        "expected deterministic env test to pass\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("tests: 1 passed, 0 failed"));
}

#[test]
fn cli_test_virtual_gpu_compute_project_runs_on_cpu() {
    let dir = workspace_tempdir();
    write_virtual_gpu_compute_project(dir.path());

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("test")
        .arg(".")
        .output()
        .expect("run wrela test");
    assert!(
        output.status.success(),
        "expected virtual GPU compute project to pass\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("tests: 3 passed, 0 failed"));
}

#[test]
fn cli_test_virtual_gpu_atomic_schedule_project_runs_on_cpu() {
    let dir = workspace_tempdir();
    write_virtual_gpu_atomic_schedule_project(dir.path());

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("test")
        .arg(".")
        .output()
        .expect("run wrela test");
    assert!(
        output.status.success(),
        "expected virtual GPU atomic schedule project to pass\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("tests: 1 passed, 0 failed"));
}

#[test]
fn cli_test_virtual_gpu_workgroup_schedule_project_runs_on_cpu() {
    let dir = workspace_tempdir();
    write_virtual_gpu_workgroup_schedule_project(dir.path());

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("test")
        .arg(".")
        .output()
        .expect("run wrela test");
    assert!(
        output.status.success(),
        "expected virtual GPU workgroup schedule project to pass\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("tests: 3 passed, 0 failed"));
}

#[test]
fn cli_test_project_harness_compiles_once_for_full_and_filtered_runs() {
    let dir = workspace_tempdir();
    write_large_test_project(dir.path());

    let full = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .env("WRELA_BUILD_TRACE", "1")
        .arg("test")
        .arg(".")
        .output()
        .expect("run full project tests");
    assert!(full.status.success());
    let full_stdout = String::from_utf8_lossy(&full.stdout);
    assert!(full_stdout.contains("tests: 24 passed, 0 failed"));
    let full_stderr = String::from_utf8_lossy(&full.stderr);
    assert_eq!(
        count_occurrences(&full_stderr, "build: test harness compile start"),
        1,
        "expected exactly one harness compile for full run"
    );

    let filtered = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .env("WRELA_BUILD_TRACE", "1")
        .arg("test")
        .arg("--filter=tests/spec")
        .arg(".")
        .output()
        .expect("run filtered project tests");
    assert!(filtered.status.success());
    let filtered_stdout = String::from_utf8_lossy(&filtered.stdout);
    assert!(filtered_stdout.contains("tests: 6 passed, 0 failed"));
    let filtered_stderr = String::from_utf8_lossy(&filtered.stderr);
    assert_eq!(
        count_occurrences(&filtered_stderr, "build: test harness compile start"),
        1,
        "expected exactly one harness compile for filtered run"
    );
}

#[test]
fn cli_test_json_summary_schema_and_id_selection() {
    let dir = workspace_tempdir();
    write_test_registry_project(dir.path());
    let beta_id = fnv1a64_hex(b"tests/integration/beta::test_beta");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("test")
        .arg("--error-format=json")
        .arg("--jobs=4")
        .arg(format!("--id={beta_id}"))
        .arg(".")
        .output()
        .expect("run wrela test json");
    assert!(output.status.success());

    let json = parse_single_json_stdout(&output.stdout);
    let run = json.get("run").expect("run metadata");
    assert!(run.get("seed").is_some());
    assert!(run.get("lane").is_some());
    assert_eq!(run.get("jobs").and_then(|value| value.as_u64()), Some(4));
    assert!(run.get("harness_cache_hit").is_some());
    assert_eq!(
        run.get("budgets_used")
            .and_then(|v| v.get("policy_version"))
            .and_then(|v| v.as_u64()),
        Some(1)
    );
    assert_eq!(
        run.get("budgets_used")
            .and_then(|v| v.get("test_jobs"))
            .and_then(|v| v.get("value"))
            .and_then(|v| v.as_u64()),
        Some(4)
    );
    assert_eq!(
        run.get("budgets_used")
            .and_then(|v| v.get("test_timeout_ms"))
            .and_then(|v| v.get("value"))
            .and_then(|v| v.as_u64()),
        Some(10000)
    );
    assert_eq!(
        run.get("budgets_used")
            .and_then(|v| v.get("sim_max_cases"))
            .and_then(|v| v.get("value"))
            .and_then(|v| v.as_u64()),
        Some(256)
    );
    assert_eq!(
        run.get("budgets_used")
            .and_then(|v| v.get("sim_max_cases"))
            .and_then(|v| v.get("provenance"))
            .and_then(|v| v.get("source"))
            .and_then(|v| v.as_str()),
        Some("default")
    );

    let tests = json
        .get("tests")
        .and_then(|value| value.as_array())
        .expect("tests array");
    assert_eq!(
        tests.len(),
        1,
        "id selection should execute exactly one test"
    );
    let only = &tests[0];
    assert_eq!(
        only.get("id").and_then(|value| value.as_str()),
        Some(beta_id.as_str())
    );
    assert_eq!(
        only.get("name").and_then(|value| value.as_str()),
        Some("tests/integration/beta::test_beta")
    );
    assert_eq!(
        only.get("lane").and_then(|value| value.as_str()),
        Some("integration")
    );
    assert_eq!(
        only.get("status").and_then(|value| value.as_str()),
        Some("ok")
    );
    assert!(only.get("duration_ms").is_some());
    assert!(only.get("error").is_none());

    let timings = json.get("timings").expect("timings");
    assert!(timings.get("discovery_ms").is_some());
    assert!(timings.get("selection_ms").is_some());
    assert!(timings.get("compile_harness_ms").is_some());
    assert!(timings.get("execution_ms").is_some());
    assert!(timings.get("total_ms").is_some());
}

#[test]
fn cli_test_json_reports_harness_cache_hit_on_warm_repeat() {
    let dir = workspace_tempdir();
    write_test_project(dir.path());

    let first = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("test")
        .arg("--error-format=json")
        .arg(".")
        .output()
        .expect("run first wrela test json");
    assert!(
        first.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&first.stdout),
        String::from_utf8_lossy(&first.stderr)
    );

    let second = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("test")
        .arg("--error-format=json")
        .arg(".")
        .output()
        .expect("run second wrela test json");
    assert!(
        second.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&second.stdout),
        String::from_utf8_lossy(&second.stderr)
    );

    let json = parse_single_json_stdout(&second.stdout);
    let run = json.get("run").expect("run metadata");
    assert_eq!(
        run.get("harness_cache_hit")
            .and_then(|value| value.as_bool()),
        Some(true)
    );
    let timings = json.get("timings").expect("timings");
    assert_eq!(
        timings
            .get("compile_harness_ms")
            .and_then(|value| value.as_u64()),
        Some(0)
    );
}

#[test]
fn cli_budget_override_env_is_auditable_in_json_and_cert_with_ceiling_enforcement() {
    let dir = workspace_tempdir();
    write_test_project(dir.path());
    let entry = dir.path().join("src").join("main.wr");
    let bin = dir.path().join("budget_override_build_bin");

    let json_output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .env("WRELA_BUDGET_SIM_MAX_CASES", "999999")
        .arg("test")
        .arg("--error-format=json")
        .arg(".")
        .output()
        .expect("run wrela test json with budget override");
    assert!(json_output.status.success());

    let json = parse_single_json_stdout(&json_output.stdout);
    let run = json.get("run").expect("run metadata");
    let budgets = run.get("budgets_used").expect("budgets_used");
    assert_eq!(
        budgets
            .get("sim_max_cases")
            .and_then(|v| v.get("value"))
            .and_then(|v| v.as_u64()),
        Some(4096)
    );
    assert_eq!(
        budgets
            .get("sim_max_cases")
            .and_then(|v| v.get("provenance"))
            .and_then(|v| v.get("source"))
            .and_then(|v| v.as_str()),
        Some("env")
    );
    assert_eq!(
        budgets
            .get("sim_max_cases")
            .and_then(|v| v.get("provenance"))
            .and_then(|v| v.get("key"))
            .and_then(|v| v.as_str()),
        Some("WRELA_BUDGET_SIM_MAX_CASES")
    );
    assert_eq!(
        budgets
            .get("sim_max_cases")
            .and_then(|v| v.get("provenance"))
            .and_then(|v| v.get("clamped_to_ceiling"))
            .and_then(|v| v.as_bool()),
        Some(true)
    );
    assert_eq!(
        budgets
            .get("autogen_max_cases")
            .and_then(|v| v.get("value"))
            .and_then(|v| v.as_u64()),
        Some(16)
    );

    let build_output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .env("WRELA_BUDGET_SIM_MAX_CASES", "999999")
        .arg("build")
        .arg(&entry)
        .arg("-o")
        .arg(&bin)
        .output()
        .expect("run build with budget override");
    assert!(build_output.status.success());

    let cert_path = dir.path().join("cert.json");
    let cert_payload = std::fs::read_to_string(&cert_path).expect("read cert");
    let cert: serde_json::Value = serde_json::from_str(&cert_payload).expect("parse cert");
    let cert_budgets = cert.get("budgets_used").expect("cert budgets");
    assert_eq!(
        cert_budgets
            .get("sim_max_cases")
            .and_then(|v| v.get("value"))
            .and_then(|v| v.as_u64()),
        Some(4096)
    );
    assert_eq!(
        cert_budgets
            .get("sim_max_cases")
            .and_then(|v| v.get("provenance"))
            .and_then(|v| v.get("source"))
            .and_then(|v| v.as_str()),
        Some("env")
    );
    assert_eq!(
        cert_budgets
            .get("autogen_max_cases")
            .and_then(|v| v.get("value"))
            .and_then(|v| v.as_u64()),
        Some(16)
    );
}

#[test]
fn cli_test_json_summary_ordering_is_deterministic_with_parallel_jobs() {
    let dir = workspace_tempdir();
    write_test_registry_project(dir.path());

    let first_output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("test")
        .arg("--error-format=json")
        .arg("--jobs=4")
        .arg(".")
        .output()
        .expect("run first json summary");
    assert!(first_output.status.success());

    let second_output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("test")
        .arg("--error-format=json")
        .arg("--jobs=4")
        .arg(".")
        .output()
        .expect("run second json summary");
    assert!(second_output.status.success());

    let first = parse_single_json_stdout(&first_output.stdout);
    let second = parse_single_json_stdout(&second_output.stdout);
    let first_ids: Vec<&str> = first
        .get("tests")
        .and_then(|value| value.as_array())
        .expect("first tests")
        .iter()
        .map(|test| {
            test.get("id")
                .and_then(|value| value.as_str())
                .expect("test id")
        })
        .collect();
    let second_ids: Vec<&str> = second
        .get("tests")
        .and_then(|value| value.as_array())
        .expect("second tests")
        .iter()
        .map(|test| {
            test.get("id")
                .and_then(|value| value.as_str())
                .expect("test id")
        })
        .collect();
    assert_eq!(first_ids, second_ids, "json test order should be stable");

    let mut sorted_ids = first_ids.clone();
    sorted_ids.sort_unstable();
    assert_eq!(
        first_ids, sorted_ids,
        "json tests should be sorted by stable id"
    );
}

#[test]
fn cli_test_json_naming_warning_paths_point_to_original_spec_files() {
    let dir = workspace_tempdir();
    std::fs::create_dir_all(dir.path().join("src")).expect("create src");
    std::fs::create_dir_all(dir.path().join("tests").join("spec")).expect("create spec tests");
    write_fixture_file(
        dir.path().join("src").join("main.wr"),
        r#"fn run() -> Integer {
    return 0
}
"#,
    )
    .expect("write main");
    write_fixture_file(
        dir.path()
            .join("tests")
            .join("spec")
            .join("counter_test.wr"),
        r#"class Counter {
    count: Integer
}

fn test_smoke() -> Nothing {
    assert value true == true
}
"#,
    )
    .expect("write spec test");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("test")
        .arg("--lane=spec")
        .arg("--error-format=json")
        .arg("--jobs=1")
        .arg(".")
        .output()
        .expect("run wrela test");
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let diagnostics = parse_json_stdout_lines(&output.stdout);
    let field_warning = diagnostics
        .iter()
        .find(|diag| {
            diag.get("code").and_then(|value| value.as_str())
                == Some("lang::naming::noun_only_required")
                && diag
                    .get("message")
                    .and_then(|value| value.as_str())
                    .is_some_and(|message| message.contains("field 'count'"))
        })
        .expect("field naming warning");
    let path = field_warning
        .get("path")
        .and_then(|value| value.as_str())
        .expect("warning path")
        .replace('\\', "/");
    assert!(
        path.ends_with("/tests/spec/counter_test.wr"),
        "expected original spec file path, got {path}"
    );
    assert!(
        !path.contains("/target/wrela_tests/"),
        "warning path should not point at generated harness: {path}"
    );
}

#[test]
fn cli_test_rejects_non_wr_file_target() {
    let dir = workspace_tempdir();
    let path = dir.path().join("spec.txt");
    write_fixture_file(&path, r#"not wr"#).unwrap();

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
fn cli_build_fails_when_autogen_catches_wrong_check_property() {
    let dir = workspace_tempdir();
    write_wrong_check_property_project(dir.path());
    let entry = dir.path().join("src").join("main.wr");
    let bin = dir.path().join("autogen_wrong_check_bin");

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_wrela"));
    cmd.arg("build").arg(&entry).arg("-o").arg(&bin);
    apply_fast_cert_budgets(&mut cmd);
    cmd.env("WRELA_BUDGET_AUTOGEN_MAX_CASES", "1")
        .env("WRELA_BUDGET_AUTOGEN_TIME_CAP_MS", "200");
    let output = run_command_with_timeout(&mut cmd, Duration::from_secs(90));

    assert!(!output.status.success(), "build unexpectedly passed");
    assert!(
        !bin.exists(),
        "build should not emit artifact on autogen failure"
    );

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("autogen failure:")
            && combined.contains("is_value_positive")
            && combined.contains("seed=")
            && combined.contains("span=")
            && combined.contains("call=`"),
        "expected teacher diagnostics with check/seed/span/call, got:\n{}",
        combined
    );
}

#[test]
fn cli_build_writes_autogen_repro_artifact() {
    let dir = workspace_tempdir();
    write_wrong_check_property_project(dir.path());
    let entry = dir.path().join("src").join("main.wr");

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_wrela"));
    cmd.arg("build").arg(&entry);
    apply_fast_cert_budgets(&mut cmd);
    cmd.env("WRELA_BUDGET_AUTOGEN_MAX_CASES", "1")
        .env("WRELA_BUDGET_AUTOGEN_TIME_CAP_MS", "200");
    let output = run_command_with_timeout(&mut cmd, Duration::from_secs(90));
    assert!(!output.status.success(), "build unexpectedly passed");

    let autogen_artifacts = dir.path().join("tests").join(".artifacts").join("autogen");
    let mut files = Vec::new();
    collect_json_files(&autogen_artifacts, &mut files);
    assert!(
        !files.is_empty(),
        "expected autogen repro artifact under {}, got none",
        autogen_artifacts.display()
    );

    let artifact = &files[0];
    let payload = std::fs::read_to_string(artifact).expect("read repro artifact");
    let json: serde_json::Value = serde_json::from_str(&payload).expect("parse repro artifact");
    assert_eq!(json.get("kind").and_then(|v| v.as_str()), Some("autogen"));
    assert_eq!(json.get("version").and_then(|v| v.as_u64()), Some(2));
    assert!(json.get("module_path").and_then(|v| v.as_str()).is_some());
    assert!(json.get("func_name").and_then(|v| v.as_str()).is_some());
    assert!(json.get("original_call").and_then(|v| v.as_str()).is_some());
    assert!(json.get("replay_call").and_then(|v| v.as_str()).is_some());
}

#[test]
fn cli_test_repro_replays_single_autogen_case() {
    let dir = workspace_tempdir();
    write_wrong_check_property_project(dir.path());
    let entry = dir.path().join("src").join("main.wr");

    let mut build_cmd = Command::new(env!("CARGO_BIN_EXE_wrela"));
    build_cmd.arg("build").arg(&entry);
    apply_fast_cert_budgets(&mut build_cmd);
    build_cmd
        .env("WRELA_BUDGET_AUTOGEN_MAX_CASES", "1")
        .env("WRELA_BUDGET_AUTOGEN_TIME_CAP_MS", "200");
    let build = run_command_with_timeout(&mut build_cmd, Duration::from_secs(90));
    assert!(!build.status.success(), "build unexpectedly passed");

    let autogen_artifacts = dir.path().join("tests").join(".artifacts").join("autogen");
    let mut files = Vec::new();
    collect_json_files(&autogen_artifacts, &mut files);
    assert!(!files.is_empty(), "expected repro artifact");
    files.sort();
    let artifact = files.remove(0);

    let mut replay_cmd = Command::new(env!("CARGO_BIN_EXE_wrela"));
    replay_cmd
        .current_dir(dir.path())
        .arg("test")
        .arg("--repro")
        .arg(&artifact)
        .arg(".");
    apply_fast_cert_budgets(&mut replay_cmd);
    let replay = run_command_with_timeout(&mut replay_cmd, Duration::from_secs(120));
    assert!(
        !replay.status.success(),
        "repro should fail for wrong property"
    );
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&replay.stdout),
        String::from_utf8_lossy(&replay.stderr)
    );
    assert!(
        combined.contains("autogen failure:")
            && combined.contains("is_value_positive")
            && combined.contains("repro="),
        "expected repro failure diagnostics, got:\n{}",
        combined
    );
}

#[test]
fn cli_test_repro_rejects_legacy_artifact_shape() {
    let dir = workspace_tempdir();
    write_wrong_check_property_project(dir.path());
    let legacy_artifact = dir.path().join("legacy_repro.json");
    write_fixture_file(
        &legacy_artifact,
        r#"{"version":1,"test_id":"x","module_path":"src/main","func_name":"f"}"#,
    )
    .expect("write legacy repro");

    let mut replay_cmd = Command::new(env!("CARGO_BIN_EXE_wrela"));
    replay_cmd
        .current_dir(dir.path())
        .arg("test")
        .arg("--repro")
        .arg(&legacy_artifact)
        .arg(".");
    apply_fast_cert_budgets(&mut replay_cmd);
    let replay = run_command_with_timeout(&mut replay_cmd, Duration::from_secs(120));
    assert!(
        !replay.status.success(),
        "legacy repro schema should be rejected"
    );
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&replay.stdout),
        String::from_utf8_lossy(&replay.stderr)
    );
    assert!(
        combined.contains("legacy repro artifacts are unsupported"),
        "expected legacy-schema rejection message, got:\n{}",
        combined
    );
}

#[test]
fn cli_test_sim_lane_seed_filter_and_trace_artifact() {
    let dir = workspace_tempdir();
    write_sim_seed_project(dir.path());

    let mut failing_cmd = Command::new(env!("CARGO_BIN_EXE_wrela"));
    failing_cmd
        .current_dir(dir.path())
        .arg("test")
        .arg("--lane=sim")
        .arg("--seed=7")
        .arg(".");
    apply_fast_cert_budgets(&mut failing_cmd);
    let failing = run_command_with_timeout(&mut failing_cmd, Duration::from_secs(120));
    assert!(!failing.status.success(), "seed 7 should fail");
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&failing.stdout),
        String::from_utf8_lossy(&failing.stderr)
    );
    assert!(
        combined.contains("--lane=sim --seed=7"),
        "expected replay command in output:\n{}",
        combined
    );

    let sim_artifacts = dir.path().join("tests").join(".artifacts").join("sim");
    let mut files = Vec::new();
    collect_json_files(&sim_artifacts, &mut files);
    assert!(!files.is_empty(), "expected sim trace artifact");
    let payload = std::fs::read_to_string(&files[0]).expect("read sim trace artifact");
    let json: serde_json::Value = serde_json::from_str(&payload).expect("parse sim trace artifact");
    assert_eq!(json.get("version").and_then(|v| v.as_u64()), Some(1));
    assert_eq!(json.get("lane").and_then(|v| v.as_str()), Some("sim"));
    let events = json
        .get("events")
        .and_then(|v| v.as_array())
        .expect("sim trace events");
    assert!(
        events.len() >= 2,
        "expected at least two trace events, got {}",
        events.len()
    );

    let mut passing_cmd = Command::new(env!("CARGO_BIN_EXE_wrela"));
    passing_cmd
        .current_dir(dir.path())
        .arg("test")
        .arg("--lane=sim")
        .arg("--seed=8")
        .arg(".");
    apply_fast_cert_budgets(&mut passing_cmd);
    let passing = run_command_with_timeout(&mut passing_cmd, Duration::from_secs(120));
    assert!(
        passing.status.success(),
        "seed 8 should pass; stderr:\n{}",
        String::from_utf8_lossy(&passing.stderr)
    );
}

#[test]
fn cli_test_model_lane_seed_filter_and_artifact() {
    let dir = workspace_tempdir();
    write_model_seed_project(dir.path());

    let mut failing_cmd = Command::new(env!("CARGO_BIN_EXE_wrela"));
    failing_cmd
        .current_dir(dir.path())
        .arg("test")
        .arg("--lane=model")
        .arg("--seed=9")
        .arg(".");
    apply_fast_cert_budgets(&mut failing_cmd);
    let failing = run_command_with_timeout(&mut failing_cmd, Duration::from_secs(120));
    assert!(!failing.status.success(), "seed 9 should fail");

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&failing.stdout),
        String::from_utf8_lossy(&failing.stderr)
    );
    assert!(
        combined.contains("--lane=model --seed=9"),
        "expected model replay command in output:\n{}",
        combined
    );

    let model_artifacts = dir.path().join("tests").join(".artifacts").join("model");
    let mut files = Vec::new();
    collect_json_files(&model_artifacts, &mut files);
    assert!(!files.is_empty(), "expected model artifact");
    let payload = std::fs::read_to_string(&files[0]).expect("read model trace artifact");
    let json: serde_json::Value =
        serde_json::from_str(&payload).expect("parse model trace artifact");
    assert_eq!(json.get("version").and_then(|v| v.as_u64()), Some(1));
    assert_eq!(json.get("lane").and_then(|v| v.as_str()), Some("model"));
    let events = json
        .get("events")
        .and_then(|v| v.as_array())
        .expect("model trace events");
    assert!(
        events.len() >= 2,
        "expected at least two trace events, got {}",
        events.len()
    );

    let mut passing_cmd = Command::new(env!("CARGO_BIN_EXE_wrela"));
    passing_cmd
        .current_dir(dir.path())
        .arg("test")
        .arg("--lane=model")
        .arg("--seed=10")
        .arg(".");
    apply_fast_cert_budgets(&mut passing_cmd);
    passing_cmd.env("WRELA_BUDGET_TEST_TIMEOUT_MS", "1000");
    let passing = run_command_with_timeout(&mut passing_cmd, Duration::from_secs(120));
    assert!(passing.status.success(), "seed 10 should pass");
}

#[test]
fn cli_test_replay_trace_validation_emits_signature() {
    let dir = workspace_tempdir();
    std::fs::create_dir_all(dir.path().join("src")).expect("create src");
    write_fixture_file(
        dir.path().join("src").join("main.wr"),
        r#"fn run() -> Integer {
    return 0
}
"#,
    )
    .expect("write main");
    let trace_path = dir.path().join("trace.json");
    write_fixture_file(
        &trace_path,
        serde_json::json!({
            "version": 1,
            "generated_at_unix_ms": 1,
            "test_id": "tests/sim/demo::test_demo",
            "canonical_test_id": "tests/sim/demo::test_demo",
            "lane": "sim",
            "seed": 7,
            "failure": "fail",
            "events": [
                {
                    "seq": 0,
                    "operation": { "phase": "dispatch", "action": "start", "commit_state": "pre-commit" },
                    "route": { "lane": "sim", "scheduler_seed": 7, "target": "tests/sim/demo::test_demo" },
                    "timing": { "logical_step": 0, "observed_unix_ms": 1 },
                    "fault": null,
                    "outcome": "started"
                },
                {
                    "seq": 1,
                    "operation": { "phase": "dispatch", "action": "commit", "commit_state": "failed" },
                    "route": { "lane": "sim", "scheduler_seed": 7, "target": "tests/sim/demo::test_demo" },
                    "timing": { "logical_step": 1, "observed_unix_ms": 1 },
                    "fault": { "kind": "injected_failure", "source": "lane_runtime", "seed": 7, "detail": "fail" },
                    "outcome": "failed"
                }
            ]
        })
        .to_string(),
    )
    .expect("write trace");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("test")
        .arg("--replay-trace")
        .arg(&trace_path)
        .arg(".")
        .output()
        .expect("run replay trace");
    assert!(
        output.status.success(),
        "stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("replay trace verified"));
    assert!(stdout.contains("signature:"));
}

#[test]
fn cli_test_replay_trace_validation_rejects_sequence_drift() {
    let dir = workspace_tempdir();
    std::fs::create_dir_all(dir.path().join("src")).expect("create src");
    write_fixture_file(
        dir.path().join("src").join("main.wr"),
        r#"fn run() -> Integer {
    return 0
}
"#,
    )
    .expect("write main");
    let trace_path = dir.path().join("trace_bad.json");
    write_fixture_file(
        &trace_path,
        serde_json::json!({
            "version": 1,
            "generated_at_unix_ms": 1,
            "test_id": "tests/model/demo::test_demo",
            "canonical_test_id": "tests/model/demo::test_demo",
            "lane": "model",
            "seed": 9,
            "failure": "fail",
            "events": [
                {
                    "seq": 0,
                    "operation": { "phase": "dispatch", "action": "start", "commit_state": "pre-commit" },
                    "route": { "lane": "model", "scheduler_seed": 9, "target": "tests/model/demo::test_demo" },
                    "timing": { "logical_step": 0, "observed_unix_ms": 1 },
                    "fault": null,
                    "outcome": "started"
                },
                {
                    "seq": 3,
                    "operation": { "phase": "dispatch", "action": "commit", "commit_state": "failed" },
                    "route": { "lane": "model", "scheduler_seed": 9, "target": "tests/model/demo::test_demo" },
                    "timing": { "logical_step": 1, "observed_unix_ms": 1 },
                    "fault": { "kind": "injected_failure", "source": "lane_runtime", "seed": 9, "detail": "fail" },
                    "outcome": "failed"
                }
            ]
        })
        .to_string(),
    )
    .expect("write trace");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("test")
        .arg("--replay-trace")
        .arg(&trace_path)
        .arg(".")
        .output()
        .expect("run replay trace");
    assert!(!output.status.success(), "expected failure");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("replay trace error: non-deterministic event sequence"),
        "stderr:\n{}",
        stderr
    );
}

#[test]
fn cli_test_replay_trace_validation_rejects_empty_events() {
    let dir = workspace_tempdir();
    std::fs::create_dir_all(dir.path().join("src")).expect("create src");
    write_fixture_file(
        dir.path().join("src").join("main.wr"),
        r#"fn run() -> Integer {
    return 0
}
"#,
    )
    .expect("write main");
    let trace_path = dir.path().join("trace_empty_events.json");
    write_fixture_file(
        &trace_path,
        serde_json::json!({
            "version": 1,
            "generated_at_unix_ms": 1,
            "test_id": "tests/sim/demo::test_demo",
            "canonical_test_id": "tests/sim/demo::test_demo",
            "lane": "sim",
            "seed": 7,
            "failure": "fail",
            "events": []
        })
        .to_string(),
    )
    .expect("write trace");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("test")
        .arg("--replay-trace")
        .arg(&trace_path)
        .arg(".")
        .output()
        .expect("run replay trace");
    assert!(!output.status.success(), "expected failure");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("replay trace error: replay trace contains no events"),
        "stderr:\n{}",
        stderr
    );
}

#[test]
fn cli_test_replay_trace_validation_rejects_fault_seed_drift() {
    let dir = workspace_tempdir();
    std::fs::create_dir_all(dir.path().join("src")).expect("create src");
    write_fixture_file(
        dir.path().join("src").join("main.wr"),
        r#"fn run() -> Integer {
    return 0
}
"#,
    )
    .expect("write main");
    let trace_path = dir.path().join("trace_fault_seed_drift.json");
    write_fixture_file(
        &trace_path,
        serde_json::json!({
            "version": 1,
            "generated_at_unix_ms": 1,
            "test_id": "tests/model/demo::test_demo",
            "canonical_test_id": "tests/model/demo::test_demo",
            "lane": "model",
            "seed": 9,
            "failure": "fail",
            "events": [
                {
                    "seq": 0,
                    "operation": { "phase": "dispatch", "action": "start", "commit_state": "pre-commit" },
                    "route": { "lane": "model", "scheduler_seed": 9, "target": "tests/model/demo::test_demo" },
                    "timing": { "logical_step": 0, "observed_unix_ms": 1 },
                    "fault": null,
                    "outcome": "started"
                },
                {
                    "seq": 1,
                    "operation": { "phase": "dispatch", "action": "commit", "commit_state": "failed" },
                    "route": { "lane": "model", "scheduler_seed": 9, "target": "tests/model/demo::test_demo" },
                    "timing": { "logical_step": 1, "observed_unix_ms": 1 },
                    "fault": { "kind": "injected_failure", "source": "lane_runtime", "seed": 11, "detail": "fail" },
                    "outcome": "failed"
                }
            ]
        })
        .to_string(),
    )
    .expect("write trace");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("test")
        .arg("--replay-trace")
        .arg(&trace_path)
        .arg(".")
        .output()
        .expect("run replay trace");
    assert!(!output.status.success(), "expected failure");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("replay trace error: fault seed mismatch"),
        "stderr:\n{}",
        stderr
    );
}

#[test]
fn cli_test_replay_trace_validation_rejects_empty_operation_or_outcome() {
    let dir = workspace_tempdir();
    std::fs::create_dir_all(dir.path().join("src")).expect("create src");
    write_fixture_file(
        dir.path().join("src").join("main.wr"),
        r#"fn run() -> Integer {
    return 0
}
"#,
    )
    .expect("write main");
    let trace_path = dir.path().join("trace_empty_operation.json");
    write_fixture_file(
        &trace_path,
        serde_json::json!({
            "version": 1,
            "generated_at_unix_ms": 1,
            "test_id": "tests/model/demo::test_demo",
            "canonical_test_id": "tests/model/demo::test_demo",
            "lane": "model",
            "seed": 9,
            "failure": "fail",
            "events": [
                {
                    "seq": 0,
                    "operation": { "phase": " ", "action": "start", "commit_state": "pre-commit" },
                    "route": { "lane": "model", "scheduler_seed": 9, "target": "tests/model/demo::test_demo" },
                    "timing": { "logical_step": 0, "observed_unix_ms": 1 },
                    "fault": null,
                    "outcome": "started"
                },
                {
                    "seq": 1,
                    "operation": { "phase": "dispatch", "action": "commit", "commit_state": "failed" },
                    "route": { "lane": "model", "scheduler_seed": 9, "target": "tests/model/demo::test_demo" },
                    "timing": { "logical_step": 1, "observed_unix_ms": 1 },
                    "fault": { "kind": "injected_failure", "source": "lane_runtime", "seed": 9, "detail": "fail" },
                    "outcome": "failed"
                }
            ]
        })
        .to_string(),
    )
    .expect("write trace");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("test")
        .arg("--replay-trace")
        .arg(&trace_path)
        .arg(".")
        .output()
        .expect("run replay trace");
    assert!(!output.status.success(), "expected failure");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(
            "replay trace error: invalid replay event: operation/outcome must be non-empty"
        ),
        "stderr:\n{}",
        stderr
    );
}

#[test]
fn cli_test_replay_trace_validation_rejects_schema_version_drift() {
    let dir = workspace_tempdir();
    std::fs::create_dir_all(dir.path().join("src")).expect("create src");
    write_fixture_file(
        dir.path().join("src").join("main.wr"),
        r#"fn run() -> Integer {
    return 0
}
"#,
    )
    .expect("write main");
    let trace_path = dir.path().join("trace_schema_drift.json");
    write_fixture_file(
        &trace_path,
        serde_json::json!({
            "version": 2,
            "generated_at_unix_ms": 1,
            "test_id": "tests/sim/demo::test_demo",
            "canonical_test_id": "tests/sim/demo::test_demo",
            "lane": "sim",
            "seed": 7,
            "failure": "fail",
            "events": [
                {
                    "seq": 0,
                    "operation": { "phase": "dispatch", "action": "start", "commit_state": "pre-commit" },
                    "route": { "lane": "sim", "scheduler_seed": 7, "target": "tests/sim/demo::test_demo" },
                    "timing": { "logical_step": 0, "observed_unix_ms": 1 },
                    "fault": null,
                    "outcome": "started"
                }
            ]
        })
        .to_string(),
    )
    .expect("write trace");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("test")
        .arg("--replay-trace")
        .arg(&trace_path)
        .arg(".")
        .output()
        .expect("run replay trace");
    assert!(!output.status.success(), "expected failure");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("replay trace error: unsupported replay trace schema version"),
        "stderr:\n{}",
        stderr
    );
}

#[test]
fn cli_test_replay_trace_validation_rejects_route_target_drift() {
    let dir = workspace_tempdir();
    std::fs::create_dir_all(dir.path().join("src")).expect("create src");
    write_fixture_file(
        dir.path().join("src").join("main.wr"),
        r#"fn run() -> Integer {
    return 0
}
"#,
    )
    .expect("write main");
    let trace_path = dir.path().join("trace_target_drift.json");
    write_fixture_file(
        &trace_path,
        serde_json::json!({
            "version": 1,
            "generated_at_unix_ms": 1,
            "test_id": "tests/model/demo::test_demo",
            "canonical_test_id": "tests/model/demo::test_demo",
            "lane": "model",
            "seed": 9,
            "failure": "fail",
            "events": [
                {
                    "seq": 0,
                    "operation": { "phase": "dispatch", "action": "start", "commit_state": "pre-commit" },
                    "route": { "lane": "model", "scheduler_seed": 9, "target": "tests/model/other::test_other" },
                    "timing": { "logical_step": 0, "observed_unix_ms": 1 },
                    "fault": null,
                    "outcome": "started"
                }
            ]
        })
        .to_string(),
    )
    .expect("write trace");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("test")
        .arg("--replay-trace")
        .arg(&trace_path)
        .arg(".")
        .output()
        .expect("run replay trace");
    assert!(!output.status.success(), "expected failure");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("replay trace error: route target mismatch"),
        "stderr:\n{}",
        stderr
    );
}

#[test]
fn cli_test_replay_trace_validation_json_emits_typed_mismatch_payload() {
    let dir = workspace_tempdir();
    std::fs::create_dir_all(dir.path().join("src")).expect("create src");
    write_fixture_file(
        dir.path().join("src").join("main.wr"),
        r#"fn run() -> Integer {
    return 0
}
"#,
    )
    .expect("write main");
    let trace_path = dir.path().join("trace_bad_json.json");
    write_fixture_file(
        &trace_path,
        serde_json::json!({
            "version": 1,
            "generated_at_unix_ms": 1,
            "test_id": "tests/model/demo::test_demo",
            "canonical_test_id": "tests/model/demo::test_demo",
            "lane": "model",
            "seed": 9,
            "failure": "fail",
            "events": [
                {
                    "seq": 0,
                    "operation": { "phase": "dispatch", "action": "start", "commit_state": "pre-commit" },
                    "route": { "lane": "model", "scheduler_seed": 9, "target": "tests/model/demo::test_demo" },
                    "timing": { "logical_step": 0, "observed_unix_ms": 1 },
                    "fault": null,
                    "outcome": "started"
                },
                {
                    "seq": 3,
                    "operation": { "phase": "dispatch", "action": "commit", "commit_state": "failed" },
                    "route": { "lane": "model", "scheduler_seed": 9, "target": "tests/model/demo::test_demo" },
                    "timing": { "logical_step": 1, "observed_unix_ms": 1 },
                    "fault": { "kind": "injected_failure", "source": "lane_runtime", "seed": 9, "detail": "fail" },
                    "outcome": "failed"
                }
            ]
        })
        .to_string(),
    )
    .expect("write trace");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("test")
        .arg("--json")
        .arg("--replay-trace")
        .arg(&trace_path)
        .arg(".")
        .output()
        .expect("run replay trace");
    assert!(!output.status.success(), "expected failure");
    let value = parse_single_json_stdout(&output.stdout);
    assert_eq!(value.get("kind").and_then(|v| v.as_str()), Some("error"));
    assert_eq!(
        value.get("code").and_then(|v| v.as_str()),
        Some("lang::runtime::replay_ordering_drift")
    );
    assert!(
        value
            .get("message")
            .and_then(|v| v.as_str())
            .is_some_and(|msg| msg.contains("replay trace error: non-deterministic event sequence"))
    );
    assert_eq!(
        value
            .get("data")
            .and_then(|v| v.get("kind"))
            .and_then(|v| v.as_str()),
        Some("replay_trace_validation")
    );
    assert_eq!(
        value
            .get("data")
            .and_then(|v| v.get("mismatch_kind"))
            .and_then(|v| v.as_str()),
        Some("ordering_drift")
    );
    assert_eq!(
        value
            .get("data")
            .and_then(|v| v.get("mismatch_code"))
            .and_then(|v| v.as_str()),
        Some("lang::runtime::replay_ordering_drift")
    );
}

#[test]
fn cli_test_replay_trace_validation_sarif_emits_mismatch_rule_id() {
    let dir = workspace_tempdir();
    std::fs::create_dir_all(dir.path().join("src")).expect("create src");
    write_fixture_file(
        dir.path().join("src").join("main.wr"),
        r#"fn run() -> Integer {
    return 0
}
"#,
    )
    .expect("write main");
    let trace_path = dir.path().join("trace_bad_sarif.json");
    write_fixture_file(
        &trace_path,
        serde_json::json!({
            "version": 2,
            "generated_at_unix_ms": 1,
            "test_id": "tests/sim/demo::test_demo",
            "canonical_test_id": "tests/sim/demo::test_demo",
            "lane": "sim",
            "seed": 7,
            "failure": "fail",
            "events": [
                {
                    "seq": 0,
                    "operation": { "phase": "dispatch", "action": "start", "commit_state": "pre-commit" },
                    "route": { "lane": "sim", "scheduler_seed": 7, "target": "tests/sim/demo::test_demo" },
                    "timing": { "logical_step": 0, "observed_unix_ms": 1 },
                    "fault": null,
                    "outcome": "started"
                }
            ]
        })
        .to_string(),
    )
    .expect("write trace");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("test")
        .arg("--error-format=sarif")
        .arg("--replay-trace")
        .arg(&trace_path)
        .arg(".")
        .output()
        .expect("run replay trace");
    assert!(!output.status.success(), "expected failure");
    let log = parse_single_json_stdout(&output.stdout);
    let results = assert_sarif_log_contract(&log);
    assert_eq!(results.len(), 1, "expected single replay mismatch result");
    let result = &results[0];
    assert_eq!(
        result.get("ruleId").and_then(|v| v.as_str()),
        Some("lang::runtime::replay_schema_drift")
    );
    assert!(
        result
            .get("message")
            .and_then(|v| v.get("text"))
            .and_then(|v| v.as_str())
            .is_some_and(
                |msg| msg.contains("replay trace error: unsupported replay trace schema version")
            )
    );
}
