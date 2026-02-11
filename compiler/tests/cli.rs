use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::Command;
use std::thread;

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
    assert!(stdout.contains("--list"));
    assert!(stdout.contains("--id=ID"));
    assert!(stdout.contains("--filter=PATTERN"));
    assert!(stdout.contains("run certification"));
    assert!(!stdout.contains("--no-certify"));
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
        "to test_basic() -> Nothing:\n    value = 1 + 1\n    assert value value == 2\n",
    )
    .unwrap();
}

fn write_failing_test_project(root: &std::path::Path) {
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
        tests_dir.join("failing.wr"),
        "to test_failing() -> Nothing:\n    value = 1 + 1\n    assert value value == 3\n",
    )
    .unwrap();
}

fn write_nondeterministic_cert_project(root: &std::path::Path) {
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
        tests_dir.join("nondeterministic_cert.wr"),
        "to test_nondeterministic_cert() -> Nothing:\n    assert value (__wr_clock_ns() % 2) == 0\n",
    )
    .unwrap();
}

fn write_oracle_gate_project(root: &std::path::Path, with_assert: bool) {
    let src_dir = root.join("src");
    let tests_dir = root.join("tests");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::create_dir_all(&tests_dir).unwrap();
    std::fs::write(
        src_dir.join("main.wr"),
        "to run() -> Integer:\n    return 0\n",
    )
    .unwrap();
    let body = if with_assert {
        "to compute_value() -> Integer:\n    return 1\n\nto test_oracle_gate() -> Nothing:\n    compute_value()\n    assert value compute_value() == 1\n"
    } else {
        "to compute_value() -> Integer:\n    return 1\n\nto test_oracle_gate() -> Nothing:\n    compute_value()\n"
    };
    std::fs::write(tests_dir.join("oracle_gate.wr"), body).unwrap();
}

fn write_test_registry_project(root: &std::path::Path) {
    let src_dir = root.join("src");
    let tests_dir = root.join("tests");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::create_dir_all(tests_dir.join("spec")).unwrap();
    std::fs::create_dir_all(tests_dir.join("integration")).unwrap();
    std::fs::create_dir_all(tests_dir.join("sim")).unwrap();
    std::fs::create_dir_all(tests_dir.join("model")).unwrap();
    std::fs::create_dir_all(tests_dir.join("misc")).unwrap();
    std::fs::write(
        src_dir.join("main.wr"),
        "to run() -> Integer:\n    return 0\n",
    )
    .unwrap();
    std::fs::write(
        tests_dir.join("spec").join("alpha.wr"),
        "to test_alpha() -> Nothing:\n    value = 1 + 1\n    assert value value == 2\n",
    )
    .unwrap();
    std::fs::write(
        tests_dir.join("integration").join("beta.wr"),
        "to test_beta() -> Nothing:\n    value = 1 + 1\n    assert value value == 2\n",
    )
    .unwrap();
    std::fs::write(
        tests_dir.join("sim").join("gamma.wr"),
        "to test_gamma() -> Nothing:\n    value = 1 + 1\n    assert value value == 2\n",
    )
    .unwrap();
    std::fs::write(
        tests_dir.join("model").join("delta.wr"),
        "to test_delta() -> Nothing:\n    value = 1 + 1\n    assert value value == 2\n",
    )
    .unwrap();
    std::fs::write(
        tests_dir.join("misc").join("epsilon.wr"),
        "to test_epsilon() -> Nothing:\n    value = 1 + 1\n    assert value value == 2\n",
    )
    .unwrap();
}

fn write_large_test_project(root: &std::path::Path) {
    let src_dir = root.join("src");
    let tests_dir = root.join("tests");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::create_dir_all(tests_dir.join("spec")).unwrap();
    std::fs::create_dir_all(tests_dir.join("integration")).unwrap();
    std::fs::create_dir_all(tests_dir.join("sim")).unwrap();
    std::fs::create_dir_all(tests_dir.join("model")).unwrap();
    std::fs::write(
        src_dir.join("main.wr"),
        "to run() -> Integer:\n    return 0\n",
    )
    .unwrap();

    for idx in 0..24 {
        let lane = match idx % 4 {
            0 => "spec",
            1 => "integration",
            2 => "sim",
            _ => "model",
        };
        let module = format!("{lane}_{idx:02}");
        let func = format!("test_{lane}_{idx:02}");
        std::fs::write(
            tests_dir.join(lane).join(format!("{module}.wr")),
            format!("to {func}() -> Nothing:\n    value = 1 + 1\n    assert value value == 2\n"),
        )
        .unwrap();
    }
}

fn write_certified_impact_project(root: &std::path::Path) {
    let src_dir = root.join("src");
    let core_dir = src_dir.join("core");
    let tests_dir = root.join("tests");
    std::fs::create_dir_all(&core_dir).unwrap();
    std::fs::create_dir_all(tests_dir.join("spec")).unwrap();
    std::fs::create_dir_all(tests_dir.join("integration")).unwrap();
    std::fs::create_dir_all(tests_dir.join("sim")).unwrap();
    std::fs::write(
        src_dir.join("main.wr"),
        "to run() -> Integer:\n    return 0\n",
    )
    .unwrap();
    std::fs::write(
        core_dir.join("math.wr"),
        "to compute_answer() -> Integer:\n    return 41\n",
    )
    .unwrap();
    std::fs::write(
        core_dir.join("independent.wr"),
        "to fetch_constant() -> Integer:\n    return 7\n",
    )
    .unwrap();
    std::fs::write(
        tests_dir.join("spec").join("sanity.wr"),
        "to compute_spec() -> Integer:\n    return 2\n\nto test_spec_sanity() -> Nothing:\n    assert value compute_spec() == 2\n",
    )
    .unwrap();
    std::fs::write(
        tests_dir.join("integration").join("math_flow.wr"),
        "use compute_answer from core/math\n\nto test_math_flow() -> Nothing:\n    assert value compute_answer() == 41\n",
    )
    .unwrap();
    std::fs::write(
        tests_dir.join("integration").join("independent_flow.wr"),
        "use fetch_constant from core/independent\n\nto test_independent_flow() -> Nothing:\n    assert value fetch_constant() == 7\n",
    )
    .unwrap();
    std::fs::write(
        tests_dir.join("sim").join("queue_sim.wr"),
        "to compute_sim() -> Integer:\n    return 4\n\nto test_queue_sim() -> Nothing:\n    assert value compute_sim() == 4\n",
    )
    .unwrap();
}

fn write_http_integration_test_project(root: &std::path::Path, url: &str) {
    let src_dir = root.join("src");
    let integrations_dir = src_dir.join("infrastructure").join("integrations");
    let integration_dir = root.join("tests").join("integration");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::create_dir_all(&integrations_dir).unwrap();
    std::fs::create_dir_all(&integration_dir).unwrap();
    std::fs::write(
        src_dir.join("main.wr"),
        "to run() -> Integer:\n    return 0\n",
    )
    .unwrap();
    std::fs::write(
        integrations_dir.join("http_client.wr"),
        format!(
            "use try_to_http_call from host/http\n\nto fetch_charge() -> Result[String]:\n    headers = __wr_map_new()\n    return try_to_http_call(\"billing\", \"charge\", \"GET\", \"{url}\", headers, \"\", 1500)\n"
        ),
    )
    .unwrap();
    std::fs::write(
        integration_dir.join("http_connector.wr"),
        "use fetch_charge from infrastructure/integrations/http_client\n\nto test_http_connector() -> Nothing:\n    ignore result fetch_charge()\n    assert value 1 == 1\n",
    )
    .unwrap();
}

fn write_http_missing_cassette_project(root: &std::path::Path, url: &str) {
    let src_dir = root.join("src");
    let integrations_dir = src_dir.join("infrastructure").join("integrations");
    let integration_dir = root.join("tests").join("integration");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::create_dir_all(&integrations_dir).unwrap();
    std::fs::create_dir_all(&integration_dir).unwrap();
    std::fs::write(
        src_dir.join("main.wr"),
        "to run() -> Integer:\n    return 0\n",
    )
    .unwrap();
    std::fs::write(
        integrations_dir.join("http_client.wr"),
        format!(
            "use try_to_http_call from host/http\n\nto fetch_charge() -> Result[String]:\n    headers = __wr_map_new()\n    return try_to_http_call(\"billing\", \"charge\", \"GET\", \"{url}\", headers, \"\", 1500)\n"
        ),
    )
    .unwrap();
    std::fs::write(
        integration_dir.join("http_missing.wr"),
        "use fetch_charge from infrastructure/integrations/http_client\n\nto test_http_missing_cassette() -> Nothing:\n    result = fetch_charge()\n    assert err result\n",
    )
    .unwrap();
}

fn write_public_surface_project(root: &std::path::Path, compute_source: &str) {
    let src_dir = root.join("src");
    let integrations_dir = src_dir.join("infrastructure").join("integrations");
    let tests_dir = root.join("tests");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::create_dir_all(&integrations_dir).unwrap();
    std::fs::create_dir_all(&tests_dir).unwrap();
    std::fs::write(
        src_dir.join("main.wr"),
        "to run() -> Integer:\n    return 0\n",
    )
    .unwrap();
    std::fs::write(src_dir.join("public_api.wr"), compute_source).unwrap();
    std::fs::write(
        integrations_dir.join("http_client.wr"),
        "use try_to_http_call from host/http\n\nto fetch_charge() -> Result[String]:\n    headers = __wr_map_new()\n    return try_to_http_call(\"billing\", \"charge\", \"GET\", \"https://api.example.com/charge\", headers, \"\", 1500)\n",
    )
    .unwrap();
    std::fs::write(
        tests_dir.join("basic.wr"),
        "to test_basic() -> Nothing:\n    assert value 1 == 1\n",
    )
    .unwrap();
}

fn write_importable_coverage_project(root: &std::path::Path, cover_importable_surface: bool) {
    let src_dir = root.join("src");
    let domain_dir = src_dir.join("domain");
    let application_dir = src_dir.join("application");
    let tests_dir = root.join("tests");
    std::fs::create_dir_all(&domain_dir).unwrap();
    std::fs::create_dir_all(&application_dir).unwrap();
    std::fs::create_dir_all(&tests_dir).unwrap();
    std::fs::write(
        src_dir.join("main.wr"),
        "to run() -> Integer:\n    return 0\n",
    )
    .unwrap();
    std::fs::write(
        domain_dir.join("pricing.wr"),
        "to compute_domain_total() -> Integer:\n    return 7\n",
    )
    .unwrap();
    std::fs::write(
        application_dir.join("orders.wr"),
        "use compute_domain_total from domain/pricing\n\nto calculate_invoice() -> Integer:\n    return compute_domain_total()\n",
    )
    .unwrap();
    let test_source = if cover_importable_surface {
        "use calculate_invoice from application/orders\n\nto test_importable_coverage() -> Nothing:\n    assert value calculate_invoice() == 7\n"
    } else {
        "to test_importable_coverage() -> Nothing:\n    assert value 1 == 1\n"
    };
    std::fs::write(tests_dir.join("coverage_gate.wr"), test_source).unwrap();
}

fn write_function_test_coverage_index_project(root: &std::path::Path) {
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
        src_dir.join("math.wr"),
        "to compute_alpha() -> Integer:\n    return 41\n\nto compute_beta() -> Integer:\n    return 7\n",
    )
    .unwrap();
    std::fs::write(
        tests_dir.join("alpha.wr"),
        "use compute_alpha from math\n\nto test_covers_alpha() -> Nothing:\n    assert value compute_alpha() == 41\n",
    )
    .unwrap();
    std::fs::write(
        tests_dir.join("beta.wr"),
        "use compute_beta from math\n\nto test_covers_beta() -> Nothing:\n    assert value compute_beta() == 7\n",
    )
    .unwrap();
}

fn write_non_importable_function_project(root: &std::path::Path) {
    let src_dir = root.join("src");
    let infra_dir = src_dir.join("infrastructure");
    let tests_dir = root.join("tests");
    std::fs::create_dir_all(&infra_dir).unwrap();
    std::fs::create_dir_all(&tests_dir).unwrap();
    std::fs::write(
        src_dir.join("main.wr"),
        "to run() -> Integer:\n    return 0\n",
    )
    .unwrap();
    std::fs::write(
        infra_dir.join("internal_tools.wr"),
        "to compute_internal_value() -> Integer:\n    return 99\n",
    )
    .unwrap();
    std::fs::write(
        tests_dir.join("basic.wr"),
        "to test_non_importable_scope() -> Nothing:\n    assert value 1 == 1\n",
    )
    .unwrap();
}

fn write_wrong_check_property_project(root: &std::path::Path) {
    let src_dir = root.join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::write(
        src_dir.join("main.wr"),
        "check value_is_positive(value: Integer) -> Boolean:\n    return value < 0\n\nto run() -> Integer:\n    return 0\n",
    )
    .unwrap();
}

fn write_sim_seed_project(root: &std::path::Path) {
    let src_dir = root.join("src");
    let sim_dir = root.join("tests").join("sim");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::create_dir_all(&sim_dir).unwrap();
    std::fs::write(
        src_dir.join("main.wr"),
        "to run() -> Integer:\n    return 0\n",
    )
    .unwrap();
    std::fs::write(
        sim_dir.join("seeded.wr"),
        "to test_seeded_interleaving() -> Nothing:\n    seed = __wr_env_get(\"WRELA_SCHED_SEED\")\n    if seed == \"7\":\n        assert value false == true\n    assert value true == true\n",
    )
    .unwrap();
}

fn write_model_seed_project(root: &std::path::Path) {
    let src_dir = root.join("src");
    let model_dir = root.join("tests").join("model");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::create_dir_all(&model_dir).unwrap();
    std::fs::write(
        src_dir.join("main.wr"),
        "to run() -> Integer:\n    return 0\n",
    )
    .unwrap();
    std::fs::write(
        model_dir.join("counter_model.wr"),
        "to test_model_counter() -> Nothing:\n    seed = __wr_env_get(\"WRELA_MODEL_SEED\")\n    if seed == \"9\":\n        assert value false == true\n    assert value true == true\n",
    )
    .unwrap();
}

fn write_differential_divergence_project(root: &std::path::Path) {
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
        tests_dir.join("diff_gate.wr"),
        "to test_pipeline_diff_gate() -> Nothing:\n    pipeline = __wr_env_get(\"WRELA_DIFF_PIPELINE\")\n    if pipeline == \"alt\":\n        assert value false == true\n    assert value true == true\n",
    )
    .unwrap();
}

fn write_test_attribute_project(root: &std::path::Path) {
    let src_dir = root.join("src");
    let spec_dir = root.join("tests").join("spec");
    let integration_dir = root.join("tests").join("integration");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::create_dir_all(&spec_dir).unwrap();
    std::fs::create_dir_all(&integration_dir).unwrap();
    std::fs::write(
        src_dir.join("main.wr"),
        "to run() -> Integer:\n    return 0\n",
    )
    .unwrap();
    std::fs::write(
        spec_dir.join("attr_reject.wr"),
        "@allows_env_set\nto test_spec_rejects_capability_attr() -> Nothing:\n    assert value true == true\n",
    )
    .unwrap();
    std::fs::write(
        integration_dir.join("serial_ok.wr"),
        "@serial\n@allows_env_set\nto test_integration_serial_attr() -> Nothing:\n    assert value true == true\n",
    )
    .unwrap();
}

fn write_serial_cap_seed_dilution_project(root: &std::path::Path) {
    let src_dir = root.join("src");
    let integration_dir = root.join("tests").join("integration");
    let sim_dir = root.join("tests").join("sim");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::create_dir_all(&integration_dir).unwrap();
    std::fs::create_dir_all(&sim_dir).unwrap();
    std::fs::write(
        src_dir.join("main.wr"),
        "to run() -> Integer:\n    return 0\n",
    )
    .unwrap();
    std::fs::write(
        integration_dir.join("serial_only.wr"),
        "@serial\nto test_integration_serial_only() -> Nothing:\n    assert value true == true\n",
    )
    .unwrap();
    std::fs::write(
        integration_dir.join("serial_only_2.wr"),
        "@serial\nto test_integration_serial_only_2() -> Nothing:\n    assert value true == true\n",
    )
    .unwrap();
    std::fs::write(
        sim_dir.join("seed_expansion.wr"),
        "to test_sim_seed_expansion() -> Nothing:\n    assert value true == true\n",
    )
    .unwrap();
}

fn write_non_test_attribute_misuse_project(root: &std::path::Path) {
    let src_dir = root.join("src");
    let tests_dir = root.join("tests");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::create_dir_all(&tests_dir).unwrap();
    std::fs::write(
        src_dir.join("main.wr"),
        "@serial\nto compute_helper() -> Integer:\n    return 1\n\nto run() -> Integer:\n    return compute_helper()\n",
    )
    .unwrap();
    std::fs::write(
        tests_dir.join("smoke.wr"),
        "to test_smoke() -> Nothing:\n    assert value true == true\n",
    )
    .unwrap();
}

fn write_fuzz_failure_project(root: &std::path::Path) {
    let src_dir = root.join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::write(
        src_dir.join("main.wr"),
        "to run() -> Integer:\n    return 0\n",
    )
    .unwrap();
    std::fs::write(
        src_dir.join("decode.wr"),
        "to try_to_decode_payload(input: String) -> Result[Integer]:\n    crash(\"fuzz crash\")\n",
    )
    .unwrap();
}

fn write_mutation_project(root: &std::path::Path, strong_tests: bool) {
    let src_dir = root.join("src");
    let domain_dir = src_dir.join("domain");
    let tests_dir = root.join("tests");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::create_dir_all(&domain_dir).unwrap();
    std::fs::create_dir_all(&tests_dir).unwrap();
    std::fs::write(
        src_dir.join("main.wr"),
        "to run() -> Integer:\n    return 0\n",
    )
    .unwrap();
    std::fs::write(
        domain_dir.join("logic.wr"),
        "to compute_logic_value(input: Integer) -> Integer:\n    return input + 1\n\nto compute_logic_bonus(input: Integer) -> Integer:\n    return input + 2\n",
    )
    .unwrap();
    let test_body = if strong_tests {
        "use compute_logic_bonus, compute_logic_value from domain/logic\n\nto test_logic_behavior() -> Nothing:\n    assert value compute_logic_value(input=1) == 2\n    assert value compute_logic_bonus(input=1) == 3\n"
    } else {
        "use compute_logic_bonus, compute_logic_value from domain/logic\n\nto test_smoke() -> Nothing:\n    assert value compute_logic_value(input=1) == 2\n    assert value compute_logic_bonus(input=1) > 0\n"
    };
    std::fs::write(tests_dir.join("mutation.wr"), test_body).unwrap();
}

fn write_alias_collision_project(root: &std::path::Path) {
    let src_dir = root.join("src");
    let domain_dir = src_dir.join("domain");
    let app_dir = src_dir.join("application");
    let tests_dir = root.join("tests");
    std::fs::create_dir_all(&domain_dir).unwrap();
    std::fs::create_dir_all(&app_dir).unwrap();
    std::fs::create_dir_all(&tests_dir).unwrap();
    std::fs::write(
        src_dir.join("main.wr"),
        "use compute_shared from domain/logic\n\nto run() -> Integer:\n    return compute_shared(input=1)\n",
    )
    .unwrap();
    std::fs::write(
        domain_dir.join("logic.wr"),
        "to compute_shared(input: Integer) -> Integer:\n    return input + 1\n",
    )
    .unwrap();
    std::fs::write(
        app_dir.join("orders.wr"),
        "to compute_shared(input: Integer) -> Integer:\n    return input + 100\n",
    )
    .unwrap();
    std::fs::write(
        tests_dir.join("smoke.wr"),
        "use compute_shared from domain/logic\n\nto test_compute_shared() -> Nothing:\n    assert value compute_shared(input=1) == 2\n",
    )
    .unwrap();
}

fn write_parse_invalid_src_module_project(root: &std::path::Path) {
    let src_dir = root.join("src");
    let domain_dir = src_dir.join("domain");
    let tests_dir = root.join("tests");
    std::fs::create_dir_all(&domain_dir).unwrap();
    std::fs::create_dir_all(&tests_dir).unwrap();
    std::fs::write(
        src_dir.join("main.wr"),
        "to run() -> Integer:\n    return 0\n",
    )
    .unwrap();
    std::fs::write(
        domain_dir.join("broken.wr"),
        "to compute_broken() -> Integer:\n    return 1 +\n",
    )
    .unwrap();
    std::fs::write(
        tests_dir.join("smoke.wr"),
        "to test_smoke() -> Nothing:\n    assert value true == true\n",
    )
    .unwrap();
}

fn spawn_http_stub_once(body: &'static str) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
    let addr = listener.local_addr().expect("listener addr");
    let url = format!("http://{addr}/charge");
    let handle = thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut request_buf = [0u8; 4096];
            let _ = stream.read(&mut request_buf);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nDate: Wed, 01 Jan 2020 00:00:00 GMT\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });
    (url, handle)
}

fn collect_json_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    if !dir.is_dir() {
        return;
    }
    let entries = std::fs::read_dir(dir).expect("read cassette dir");
    for entry in entries {
        let path = entry.expect("cassette entry").path();
        if path.is_dir() {
            collect_json_files(&path, out);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
            out.push(path);
        }
    }
}

fn write_connector_cassette(root: &std::path::Path, name: &str, status: u16) {
    let cassettes_dir = root.join("tests").join("cassettes");
    std::fs::create_dir_all(&cassettes_dir).expect("create cassettes dir");
    let payload = format!(
        r#"{{
  "version": 1,
  "request": {{
    "service": "billing",
    "endpoint": "charge",
    "method": "GET",
    "url": "http://127.0.0.1:9/charge",
    "headers_redacted": {{}},
    "body_base64": ""
  }},
  "response": {{
    "status": {status},
    "headers": {{}},
    "body_base64": ""
  }}
}}"#
    );
    std::fs::write(cassettes_dir.join(name), payload).expect("write cassette");
}

fn fnv1a64_hex(bytes: &[u8]) -> String {
    const OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;
    let mut state = OFFSET_BASIS;
    for byte in bytes {
        state ^= *byte as u64;
        state = state.wrapping_mul(PRIME);
    }
    format!("{state:016x}")
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;
    let mut state = OFFSET_BASIS;
    for byte in bytes {
        state ^= *byte as u64;
        state = state.wrapping_mul(PRIME);
    }
    state
}

fn stable_function_id(function_identity: &str) -> String {
    fnv1a64(function_identity.as_bytes()).to_string()
}

fn extract_function_test_mapping(
    value: &serde_json::Value,
) -> Option<std::collections::BTreeMap<String, std::collections::BTreeSet<String>>> {
    if let Some(version) = value.get("schema_version").and_then(|v| v.as_u64()) {
        if version != 2 {
            return None;
        }
    }
    let mapping_value = if let Some(inner) = value.get("function_to_tests") {
        inner
    } else {
        value
    };
    let object = mapping_value.as_object()?;
    let mut mapping = std::collections::BTreeMap::new();
    for (function_id, tests_value) in object {
        let test_ids = tests_value.as_array()?.iter().try_fold(
            std::collections::BTreeSet::new(),
            |mut acc, item| {
                let test_id = item.as_str()?;
                acc.insert(test_id.to_string());
                Some(acc)
            },
        )?;
        mapping.insert(function_id.to_string(), test_ids);
    }
    Some(mapping)
}

fn certification_cache_hash(source_hash: &str, toolchain_version: &str) -> String {
    let mut payload = Vec::new();
    payload.extend_from_slice(b"wrela-cert-cache-v2");
    payload.push(0);
    payload.extend_from_slice(b"source_hash:");
    payload.extend_from_slice(source_hash.as_bytes());
    payload.push(0);
    payload.extend_from_slice(b"toolchain_version:");
    payload.extend_from_slice(toolchain_version.as_bytes());
    fnv1a64_hex(&payload)
}

fn parse_single_json_stdout(stdout: &[u8]) -> serde_json::Value {
    let stdout_text = String::from_utf8_lossy(stdout);
    let lines: Vec<&str> = stdout_text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    assert_eq!(lines.len(), 1, "expected one JSON line, got: {lines:?}");
    serde_json::from_str(lines[0]).expect("valid json")
}

fn count_occurrences(haystack: &str, needle: &str) -> usize {
    haystack.match_indices(needle).count()
}

#[test]
fn cli_build_blocks_artifact_when_certification_fails() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_failing_test_project(dir.path());
    let entry = dir.path().join("src").join("main.wr");
    let bin = dir.path().join("blocked_build_bin");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("build")
        .arg(&entry)
        .arg("-o")
        .arg(&bin)
        .output()
        .expect("run build");

    assert!(!output.status.success());
    assert!(
        !bin.exists(),
        "artifact should not exist when certification fails"
    );
}

#[test]
fn cli_build_certification_fails_when_outcome_signature_changes() {
    let mut saw_determinism_mismatch = false;
    for _ in 0..16 {
        let dir = tempfile::tempdir().expect("tempdir");
        write_nondeterministic_cert_project(dir.path());
        let entry = dir.path().join("src").join("main.wr");
        let bin = dir.path().join("nondeterministic_build_bin");

        let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
            .current_dir(dir.path())
            .arg("build")
            .arg(&entry)
            .arg("-o")
            .arg(&bin)
            .output()
            .expect("run build");

        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("determinism gate failed") {
            saw_determinism_mismatch = true;
            assert!(!output.status.success());
            assert!(
                !bin.exists(),
                "artifact should not exist on determinism gate failure"
            );
            assert!(
                stderr.contains("mismatch detail"),
                "expected mismatch details in stderr, got: {stderr}"
            );
            break;
        }
    }

    assert!(
        saw_determinism_mismatch,
        "expected at least one determinism mismatch across attempts"
    );
}

#[test]
fn cli_build_certification_passes_for_repeatable_outcomes() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_test_project(dir.path());
    let entry = dir.path().join("src").join("main.wr");
    let bin = dir.path().join("repeatable_build_bin");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("build")
        .arg(&entry)
        .arg("-o")
        .arg(&bin)
        .output()
        .expect("run build");

    assert!(output.status.success(), "{:?}", output.status.code());
    assert!(bin.exists(), "expected build artifact");
}

#[test]
fn cli_build_fails_when_importable_domain_application_function_is_uncovered() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_importable_coverage_project(dir.path(), false);
    let entry = dir.path().join("src").join("main.wr");
    let bin = dir.path().join("coverage_gate_blocked_bin");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("build")
        .arg(&entry)
        .arg("-o")
        .arg(&bin)
        .output()
        .expect("run build");

    assert!(!output.status.success(), "build unexpectedly passed");
    assert!(
        !bin.exists(),
        "artifact should not be emitted on gate failure"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("coverage gate failed"), "{stderr}");
    assert!(
        stderr.contains("domain/pricing::compute_domain_total"),
        "{stderr}"
    );
    assert!(
        stderr.contains("application/orders::calculate_invoice"),
        "{stderr}"
    );
    assert!(stderr.contains("add tests"), "{stderr}");
}

#[test]
fn cli_build_passes_when_importable_domain_application_surface_is_covered() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_importable_coverage_project(dir.path(), true);
    let entry = dir.path().join("src").join("main.wr");
    let bin = dir.path().join("coverage_gate_ok_bin");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("build")
        .arg(&entry)
        .arg("-o")
        .arg(&bin)
        .output()
        .expect("run build");

    assert!(
        output.status.success(),
        "build failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(bin.exists(), "expected build artifact");
}

#[test]
fn cli_build_writes_function_test_coverage_index_with_expected_mappings() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_function_test_coverage_index_project(dir.path());
    let entry = dir.path().join("src").join("main.wr");
    let bin = dir.path().join("coverage_index_build_bin");

    let list_output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("test")
        .arg("--list")
        .arg(".")
        .output()
        .expect("run wrela test --list");
    assert!(
        list_output.status.success(),
        "list failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&list_output.stdout),
        String::from_utf8_lossy(&list_output.stderr)
    );
    let list_stdout = String::from_utf8_lossy(&list_output.stdout);

    let alpha_name = "tests/alpha::test_covers_alpha";
    let beta_name = "tests/beta::test_covers_beta";
    let expected_alpha_test_id = fnv1a64_hex(b"tests/alpha::test_covers_alpha");
    let expected_beta_test_id = fnv1a64_hex(b"tests/beta::test_covers_beta");

    let mut discovered_ids = std::collections::BTreeMap::new();
    for line in list_stdout.lines() {
        if !line.starts_with("id=") || !line.contains(" name=") {
            continue;
        }
        let mut id: Option<String> = None;
        let mut name: Option<String> = None;
        for part in line.split_whitespace() {
            if let Some(value) = part.strip_prefix("id=") {
                id = Some(value.to_string());
            } else if let Some(value) = part.strip_prefix("name=") {
                name = Some(value.to_string());
            }
        }
        if let (Some(id), Some(name)) = (id, name) {
            discovered_ids.insert(name, id);
        }
    }

    assert_eq!(
        discovered_ids.get(alpha_name).map(String::as_str),
        Some(expected_alpha_test_id.as_str())
    );
    assert_eq!(
        discovered_ids.get(beta_name).map(String::as_str),
        Some(expected_beta_test_id.as_str())
    );

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("build")
        .arg(&entry)
        .arg("-o")
        .arg(&bin)
        .output()
        .expect("run build");
    assert!(
        output.status.success(),
        "build failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(bin.exists(), "expected build artifact");

    let index_dir = dir.path().join("target").join("wrela_cert").join("index");
    assert!(
        index_dir.is_dir(),
        "expected coverage index directory at {}",
        index_dir.display()
    );
    let mut index_files = std::fs::read_dir(&index_dir)
        .expect("read coverage index dir")
        .map(|entry| entry.expect("coverage index entry").path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
        .collect::<Vec<_>>();
    index_files.sort();
    assert!(
        !index_files.is_empty(),
        "expected at least one coverage index file under {}",
        index_dir.display()
    );

    let alpha_function_id = stable_function_id("math::compute_alpha");
    let beta_function_id = stable_function_id("math::compute_beta");
    let expected_alpha_tests = std::collections::BTreeSet::from([expected_alpha_test_id.clone()]);
    let expected_beta_tests = std::collections::BTreeSet::from([expected_beta_test_id.clone()]);

    let mut matched = false;
    for path in &index_files {
        let payload = std::fs::read_to_string(path).expect("read coverage index file");
        let value: serde_json::Value = serde_json::from_str(&payload).expect("parse index json");
        let Some(mapping) = extract_function_test_mapping(&value) else {
            continue;
        };
        let alpha_mapped = mapping.get(&alpha_function_id);
        let beta_mapped = mapping.get(&beta_function_id);
        if alpha_mapped == Some(&expected_alpha_tests) && beta_mapped == Some(&expected_beta_tests)
        {
            matched = true;
            break;
        }
    }

    assert!(
        matched,
        "expected a coverage index entry mapping {alpha_function_id}->{expected_alpha_test_id} and {beta_function_id}->{expected_beta_test_id}; files={:?}",
        index_files
    );
}

#[test]
fn cli_build_does_not_gate_on_uncovered_non_importable_function() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_non_importable_function_project(dir.path());
    let entry = dir.path().join("src").join("main.wr");
    let bin = dir.path().join("coverage_gate_non_importable_ok_bin");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("build")
        .arg(&entry)
        .arg("-o")
        .arg(&bin)
        .output()
        .expect("run build");

    assert!(
        output.status.success(),
        "build failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(bin.exists(), "expected build artifact");
}

#[test]
fn cli_perf_aggregates_function_coverage_from_metrics_dump() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_importable_coverage_project(dir.path(), true);
    let baseline = dir.path().join("perf-baseline.json");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("perf")
        .arg("--runs=1")
        .arg(format!("--baseline-out={}", baseline.display()))
        .arg(".")
        .output()
        .expect("run perf");

    assert!(
        output.status.success(),
        "perf failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&baseline).expect("read perf baseline"))
            .expect("parse perf baseline");
    let function_coverage = report
        .get("summary")
        .and_then(|value| value.get("metrics"))
        .and_then(|value| value.get("function_coverage"))
        .and_then(|value| value.as_object())
        .expect("summary.metrics.function_coverage object");
    let application_function = stable_function_id("calculate_invoice");
    let application_hits = function_coverage
        .get(&application_function)
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    assert!(
        application_hits > 0,
        "expected non-zero hits for application/orders::calculate_invoice"
    );
}

#[test]
fn cli_build_rejects_no_certification_bypass_flag() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_test_project(dir.path());
    let entry = dir.path().join("src").join("main.wr");
    let bin = dir.path().join("bypass_build_bin");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("build")
        .arg("--no-certify")
        .arg(&entry)
        .arg("-o")
        .arg(&bin)
        .output()
        .expect("run build");

    assert!(!output.status.success());
    assert!(!bin.exists(), "bypass flag must never emit artifact");
}

#[test]
fn cli_build_emits_cert_report_on_success() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_test_project(dir.path());
    let entry = dir.path().join("src").join("main.wr");
    let bin = dir.path().join("certified_build_bin");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("build")
        .arg(&entry)
        .arg("-o")
        .arg(&bin)
        .output()
        .expect("run build");

    assert!(output.status.success(), "{:?}", output.status.code());
    assert!(bin.exists(), "expected build artifact");
    let adjacent_cert_path = dir.path().join("cert.json");
    assert!(
        adjacent_cert_path.exists(),
        "expected adjacent cert.json next to binary"
    );
    let cert_payload = std::fs::read_to_string(&adjacent_cert_path).expect("read cert json");
    let cert: serde_json::Value = serde_json::from_str(&cert_payload).expect("parse cert json");

    assert_eq!(
        cert.get("cert_schema_version").and_then(|v| v.as_u64()),
        Some(3),
        "expected cert schema version"
    );
    assert_eq!(
        cert.get("toolchain_version").and_then(|v| v.as_str()),
        Some(env!("CARGO_PKG_VERSION")),
        "expected toolchain version"
    );
    assert_eq!(
        cert.get("compiler_version").and_then(|v| v.as_str()),
        Some(env!("CARGO_PKG_VERSION")),
        "expected compiler version"
    );
    assert!(
        cert.get("compiler_git_sha").is_some(),
        "expected compiler git sha field (nullable if unavailable)"
    );
    assert!(
        cert.get("runtime_version")
            .and_then(|v| v.as_str())
            .is_some_and(|v| !v.is_empty()),
        "expected non-empty runtime version"
    );
    assert_eq!(
        cert.get("gate_versions_marker").and_then(|v| v.as_str()),
        Some("wrela-cert-gates-v1"),
        "expected gate versions marker"
    );
    assert!(
        cert.get("source_hash")
            .and_then(|v| v.as_str())
            .is_some_and(|v| !v.is_empty()),
        "expected non-empty source hash"
    );
    assert_eq!(
        cert.get("seeds_used")
            .and_then(|v| v.get("sim"))
            .and_then(|v| v.as_u64()),
        Some(0x5A17),
        "expected deterministic sim seed"
    );
    assert_eq!(
        cert.get("seeds_used")
            .and_then(|v| v.get("autogen"))
            .and_then(|v| v.as_u64()),
        Some(0xA670),
        "expected deterministic autogen seed"
    );
    assert_eq!(
        cert.get("seeds_used")
            .and_then(|v| v.get("fuzz"))
            .and_then(|v| v.as_u64()),
        Some(0xF022),
        "expected deterministic fuzz seed"
    );
    assert_eq!(
        cert.get("budgets_used")
            .and_then(|v| v.get("policy_version"))
            .and_then(|v| v.as_u64()),
        Some(1),
        "expected budget policy version"
    );
    assert_eq!(
        cert.get("budgets_used")
            .and_then(|v| v.get("test_jobs"))
            .and_then(|v| v.get("value"))
            .and_then(|v| v.as_u64()),
        Some(1),
        "expected default test_jobs budget"
    );
    assert_eq!(
        cert.get("budgets_used")
            .and_then(|v| v.get("test_timeout_ms"))
            .and_then(|v| v.get("value"))
            .and_then(|v| v.as_u64()),
        Some(5000),
        "expected default test timeout budget"
    );
    assert_eq!(
        cert.get("budgets_used")
            .and_then(|v| v.get("sim_max_cases"))
            .and_then(|v| v.get("value"))
            .and_then(|v| v.as_u64()),
        Some(256),
        "expected default sim max cases budget"
    );
    assert!(
        cert.get("coverage_summary_hash")
            .is_some_and(serde_json::Value::is_null),
        "expected nullable coverage hash"
    );
    assert!(
        cert.get("mutation_summary_hash")
            .and_then(|v| v.as_str())
            .is_some_and(|v| !v.is_empty()),
        "expected non-empty mutation hash"
    );
    assert!(
        cert.get("differential_results_hash")
            .and_then(|v| v.as_str())
            .is_some_and(|v| !v.is_empty()),
        "expected non-empty differential hash"
    );

    let expected_binary_hash = fnv1a64_hex(&std::fs::read(&bin).expect("read binary"));
    let cert_binary_hash = cert
        .get("binary_hash")
        .and_then(|v| v.as_str())
        .expect("binary hash in cert");
    assert_eq!(
        cert_binary_hash, expected_binary_hash,
        "expected binary hash to match emitted artifact bytes"
    );

    let cert_source_hash = cert
        .get("source_hash")
        .and_then(|v| v.as_str())
        .expect("source hash in cert");
    let cert_toolchain_version = cert
        .get("toolchain_version")
        .and_then(|v| v.as_str())
        .expect("toolchain version in cert");
    let cert_cache_hash = certification_cache_hash(cert_source_hash, cert_toolchain_version);
    let cached_cert_path = dir
        .path()
        .join("target")
        .join("wrela_cert")
        .join(cert_cache_hash)
        .join("cert.json");
    assert!(
        cached_cert_path.exists(),
        "expected cached certification report at source/toolchain hash-keyed path"
    );
}

#[test]
fn cli_build_json_reports_certification_cache_hit_on_second_run() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_test_project(dir.path());
    let entry = dir.path().join("src").join("main.wr");
    let bin = dir.path().join("cached_build_bin");

    let first = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("build")
        .arg("--format=json")
        .arg(&entry)
        .arg("-o")
        .arg(&bin)
        .output()
        .expect("run first build");
    assert!(
        first.status.success(),
        "first build failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&first.stdout),
        String::from_utf8_lossy(&first.stderr)
    );

    let second = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("build")
        .arg("--format=json")
        .arg(&entry)
        .arg("-o")
        .arg(&bin)
        .output()
        .expect("run second build");
    assert!(
        second.status.success(),
        "second build failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&second.stdout),
        String::from_utf8_lossy(&second.stderr)
    );

    let stdout = String::from_utf8_lossy(&second.stdout);
    let diagnostics: Vec<serde_json::Value> = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("valid json"))
        .collect();
    let cache_hit = diagnostics.iter().find(|value| {
        value.get("event").and_then(|v| v.as_str()) == Some("certification_cache")
            && value.get("cache_hit").and_then(|v| v.as_bool()) == Some(true)
    });
    let cache_hit = cache_hit.expect("expected certification cache hit event in json output");
    let cache_hash = cache_hit
        .get("cache_hash")
        .and_then(|v| v.as_str())
        .expect("cache hash");
    let cache_cert = dir
        .path()
        .join("target")
        .join("wrela_cert")
        .join(cache_hash)
        .join("cert.json");
    assert!(cache_cert.exists(), "expected cached cert report");
}

#[test]
fn cli_build_json_emits_perf_timings_section() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_test_project(dir.path());
    let entry = dir.path().join("src").join("main.wr");
    let bin = dir.path().join("timed_build_bin");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("build")
        .arg("--format=json")
        .arg(&entry)
        .arg("-o")
        .arg(&bin)
        .output()
        .expect("run build");
    assert!(
        output.status.success(),
        "build failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let diagnostics: Vec<serde_json::Value> = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("valid json"))
        .collect();
    let perf = diagnostics
        .iter()
        .find(|value| value.get("event").and_then(|v| v.as_str()) == Some("build_perf"))
        .expect("expected build_perf event");
    let timings = perf
        .get("perf")
        .and_then(|v| v.get("timings"))
        .expect("perf.timings");
    assert!(timings.get("certification_ms").is_some());
    assert!(timings.get("mir_compile_ms").is_some());
    assert!(timings.get("codegen_ms").is_some());
    assert!(timings.get("cert_report_ms").is_some());
    assert!(timings.get("total_ms").is_some());

    let cache = perf
        .get("perf")
        .and_then(|v| v.get("cache"))
        .expect("perf.cache");
    assert!(cache.get("hit").is_some());
    assert!(cache.get("hash").is_some());
    assert!(cache.get("reason").is_some());
}

#[test]
fn cli_build_incremental_cert_impact_selection_reduces_tests_and_emits_reasons() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_certified_impact_project(dir.path());
    let bin = dir.path().join("impact_bin");

    let first = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("build")
        .arg("--format=json")
        .arg("-o")
        .arg(&bin)
        .arg("src/main.wr")
        .output()
        .expect("run first certified build");
    assert!(first.status.success(), "first build failed");

    std::fs::write(
        dir.path().join("src").join("core").join("math.wr"),
        "to compute_answer() -> Integer:\n    value = 41\n    return value\n",
    )
    .unwrap();

    let second = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("build")
        .arg("--format=json")
        .arg("-o")
        .arg(&bin)
        .arg("src/main.wr")
        .output()
        .expect("run second certified build");
    assert!(
        second.status.success(),
        "second build failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&second.stdout),
        String::from_utf8_lossy(&second.stderr)
    );

    let diagnostics: Vec<serde_json::Value> = String::from_utf8_lossy(&second.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("valid json line"))
        .collect();
    let selection = diagnostics
        .iter()
        .find(|value| {
            value.get("event").and_then(|v| v.as_str()) == Some("certification_selection")
        })
        .expect("expected certification selection event");
    assert_eq!(
        selection.get("mode").and_then(|v| v.as_str()),
        Some("incremental")
    );
    assert!(
        selection
            .get("changed_src_modules")
            .and_then(|v| v.as_array())
            .is_some_and(|mods| mods.iter().any(|m| m.as_str() == Some("core/math")))
    );
    let reasons = selection
        .get("reasons")
        .and_then(|v| v.as_array())
        .expect("selection reasons");
    assert!(!reasons.is_empty(), "expected non-empty selection reasons");

    let summary = diagnostics
        .iter()
        .find(|value| value.get("run").is_some() && value.get("tests").is_some())
        .expect("expected test summary json");
    let tests = summary
        .get("tests")
        .and_then(|value| value.as_array())
        .expect("tests array");
    assert_eq!(tests.len(), 2, "expected reduced test selection");
    let names: Vec<&str> = tests
        .iter()
        .filter_map(|value| value.get("name").and_then(|v| v.as_str()))
        .collect();
    assert!(names.contains(&"tests/spec/sanity::test_spec_sanity"));
    assert!(names.contains(&"tests/integration/math_flow::test_math_flow"));
    assert!(!names.contains(&"tests/integration/independent_flow::test_independent_flow"));
    assert!(!names.contains(&"tests/sim/queue_sim::test_queue_sim"));
}

#[test]
fn cli_build_fails_when_differential_alt_pipeline_diverges() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_differential_divergence_project(dir.path());
    let entry = dir.path().join("src").join("main.wr");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("build")
        .arg(&entry)
        .output()
        .expect("run build");
    assert!(
        !output.status.success(),
        "build should fail differential gate"
    );
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("differential gate failed")
            && combined.contains("baseline and alt pipelines diverged"),
        "expected differential gate diagnostic, got:\n{}",
        combined
    );
}

#[test]
fn cli_test_spec_lane_rejects_allows_attributes() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_test_attribute_project(dir.path());

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("test")
        .arg(dir.path())
        .output()
        .expect("run test");
    assert!(!output.status.success(), "expected spec lane rejection");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("teacher: spec lane forbids capability exceptions"),
        "unexpected stderr:\n{stderr}"
    );
}

#[test]
fn cli_build_serial_cap_uses_canonical_authored_tests() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_serial_cap_seed_dilution_project(dir.path());
    let entry = dir.path().join("src").join("main.wr");
    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("build")
        .arg(&entry)
        .output()
        .expect("run build");
    assert!(!output.status.success(), "serial cap should fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("serial gate failed"),
        "expected serial gate failure, got:\n{stderr}"
    );
}

#[test]
fn cli_build_rejects_test_attributes_on_non_test_functions() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_non_test_attribute_misuse_project(dir.path());
    let entry = dir.path().join("src").join("main.wr");
    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("build")
        .arg(&entry)
        .output()
        .expect("run build");
    assert!(
        !output.status.success(),
        "attribute misuse on non-test function should fail"
    );
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("only valid on test_* functions"),
        "expected non-test attribute rejection, got:\n{combined}"
    );
}

#[test]
fn cli_build_fuzz_gate_writes_repro_artifact_on_failure() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_fuzz_failure_project(dir.path());
    let entry = dir.path().join("src").join("main.wr");
    let bin = dir.path().join("fuzz_build_bin");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("build")
        .arg(&entry)
        .arg("-o")
        .arg(&bin)
        .output()
        .expect("run build");
    assert!(!output.status.success(), "build should fail fuzz gate");
    let artifact_root = dir.path().join("tests").join(".artifacts").join("fuzz");
    let mut artifacts = Vec::new();
    collect_json_files(&artifact_root, &mut artifacts);
    assert!(
        !artifacts.is_empty(),
        "expected fuzz repro artifact under {}",
        artifact_root.display()
    );
    let fuzz_payload = std::fs::read_to_string(&artifacts[0]).expect("read fuzz repro artifact");
    let fuzz_json: serde_json::Value =
        serde_json::from_str(&fuzz_payload).expect("parse fuzz repro");
    assert_eq!(fuzz_json.get("kind").and_then(|v| v.as_str()), Some("fuzz"));
    assert_eq!(fuzz_json.get("version").and_then(|v| v.as_u64()), Some(2));
    assert!(fuzz_json.get("call").and_then(|v| v.as_str()).is_some());
    let replay = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("test")
        .arg(dir.path())
        .arg("--repro")
        .arg(&artifacts[0])
        .output()
        .expect("run repro");
    assert!(
        !replay.status.success(),
        "expected repro to replay fuzz failure"
    );
}

#[test]
fn cli_build_mutation_gate_fails_for_weak_tests_and_passes_for_strong_tests() {
    let weak = tempfile::tempdir().expect("tempdir");
    write_mutation_project(weak.path(), false);
    let weak_entry = weak.path().join("src").join("main.wr");
    let weak_output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("build")
        .arg(&weak_entry)
        .output()
        .expect("run weak build");
    assert!(
        !weak_output.status.success(),
        "weak project should fail mutation gate"
    );
    let weak_stderr = String::from_utf8_lossy(&weak_output.stderr);
    assert!(
        weak_stderr.contains("mutation gate failed"),
        "expected mutation gate failure, got:\n{weak_stderr}"
    );

    let weak_report = weak
        .path()
        .join("tests")
        .join(".artifacts")
        .join("mutation")
        .join("report.json");
    assert!(
        weak_report.exists(),
        "expected mutation report for weak project"
    );
    let weak_report_json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&weak_report).expect("read weak report"))
            .expect("parse weak report");
    assert_eq!(
        weak_report_json.get("version").and_then(|v| v.as_u64()),
        Some(3),
        "expected mutation report schema version hard cutover"
    );
    assert!(
        weak_report_json
            .get("survived_mutants")
            .and_then(|v| v.as_u64())
            .is_some_and(|count| count > 0),
        "expected surviving mutants in weak report"
    );

    let strong = tempfile::tempdir().expect("tempdir");
    write_mutation_project(strong.path(), true);
    let strong_entry = strong.path().join("src").join("main.wr");
    let strong_output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("build")
        .arg(&strong_entry)
        .output()
        .expect("run strong build");
    assert!(
        strong_output.status.success(),
        "strong project should pass mutation gate: stderr={}",
        String::from_utf8_lossy(&strong_output.stderr)
    );
}

#[test]
fn cli_build_mutation_gate_excludes_invalid_mutants_from_denominator() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_mutation_project(dir.path(), true);
    let mutation_root = dir.path().join("target").join("wrela_mutation");
    std::fs::create_dir_all(&mutation_root).expect("create mutation root");
    let blocked_component =
        mutation_root.join("compute_logic_value__integer_literal_perturbation__0");
    std::fs::write(&blocked_component, "blocked").expect("write blocked mutation path");

    let entry = dir.path().join("src").join("main.wr");
    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("build")
        .arg(&entry)
        .output()
        .expect("run build");
    assert!(
        output.status.success(),
        "strong project with invalid mutant should still pass (invalid excluded): stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let report_path = dir
        .path()
        .join("tests")
        .join(".artifacts")
        .join("mutation")
        .join("report.json");
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&report_path).expect("read mutation report"))
            .expect("parse mutation report");
    let total = report
        .get("total_mutants")
        .and_then(|v| v.as_u64())
        .expect("total mutants");
    let valid = report
        .get("valid_mutants")
        .and_then(|v| v.as_u64())
        .expect("valid mutants");
    let invalid = report
        .get("invalid_mutants")
        .and_then(|v| v.as_u64())
        .expect("invalid mutants");
    let killed = report
        .get("killed_mutants")
        .and_then(|v| v.as_u64())
        .expect("killed mutants");
    let kill_rate_pct = report
        .get("kill_rate_pct")
        .and_then(|v| v.as_f64())
        .expect("kill rate pct");
    assert!(invalid > 0, "expected at least one invalid mutant");
    assert_eq!(
        valid + invalid,
        total,
        "expected valid + invalid to equal total mutants"
    );
    let expected_kill_rate = if valid == 0 {
        100.0
    } else {
        (killed as f64 / valid as f64) * 100.0
    };
    let delta = (kill_rate_pct - expected_kill_rate).abs();
    assert!(
        delta <= 0.000_1,
        "expected kill rate on valid denominator only: got {kill_rate_pct}, expected {expected_kill_rate}"
    );
    let invalid_mutant_with_reason = report
        .get("mutants")
        .and_then(|v| v.as_array())
        .is_some_and(|mutants| {
            mutants.iter().any(|mutant| {
                mutant.get("status").and_then(|v| v.as_str()) == Some("invalid-mutant")
                    && mutant
                        .get("reason")
                        .and_then(|v| v.as_str())
                        .is_some_and(|reason| !reason.is_empty())
            })
        });
    assert!(
        invalid_mutant_with_reason,
        "expected invalid-mutant entries with actionable reason"
    );
}

#[test]
fn cli_build_rejects_coverage_id_alias_collisions() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_alias_collision_project(dir.path());
    let entry = dir.path().join("src").join("main.wr");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("build")
        .arg(&entry)
        .output()
        .expect("run build");
    assert!(
        !output.status.success(),
        "build should reject alias collisions"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("coverage id collision during hard cutover"),
        "expected alias collision hard-cutover error, got:\n{stderr}"
    );
}

#[test]
fn cli_build_rejects_parse_invalid_src_module_during_alias_mapping() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_parse_invalid_src_module_project(dir.path());
    let entry = dir.path().join("src").join("main.wr");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("build")
        .arg(&entry)
        .output()
        .expect("run build");
    assert!(
        !output.status.success(),
        "build should reject parse-invalid src module in alias mapping"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("coverage id mapping requires parse-clean src modules"),
        "expected parse-clean alias mapping failure, got:\n{stderr}"
    );
}

#[test]
fn cli_build_cache_invalidates_when_relevant_wr_source_changes() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_test_project(dir.path());
    let entry = dir.path().join("src").join("main.wr");
    let bin = dir.path().join("cache_invalidation_build_bin");

    let first = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("build")
        .arg("--format=json")
        .arg(&entry)
        .arg("-o")
        .arg(&bin)
        .output()
        .expect("run first build");
    assert!(first.status.success());

    std::fs::write(&entry, "to run() -> Integer:\n    return 1\n").expect("mutate source");

    let second = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("build")
        .arg("--format=json")
        .arg(&entry)
        .arg("-o")
        .arg(&bin)
        .output()
        .expect("run second build");
    assert!(
        second.status.success(),
        "second build failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&second.stdout),
        String::from_utf8_lossy(&second.stderr)
    );

    let stdout = String::from_utf8_lossy(&second.stdout);
    let diagnostics: Vec<serde_json::Value> = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("valid json"))
        .collect();
    let cache_hit = diagnostics.iter().any(|value| {
        value.get("event").and_then(|v| v.as_str()) == Some("certification_cache")
            && value.get("cache_hit").and_then(|v| v.as_bool()) == Some(true)
    });
    assert!(
        !cache_hit,
        "expected cache miss after mutating src/**/*.wr inputs"
    );
}

#[test]
fn cli_build_connector_contract_gate_fails_without_failure_cassette() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_test_project(dir.path());
    write_connector_cassette(dir.path(), "success_only.json", 200);
    let entry = dir.path().join("src").join("main.wr");
    let bin = dir.path().join("wrela.out");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("build")
        .arg(&entry)
        .arg("-o")
        .arg(&bin)
        .output()
        .expect("run build");

    assert!(!output.status.success(), "build should fail contract gate");
    assert!(
        !bin.exists(),
        "artifact should not exist on contract gate failure"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("connector contract gate failed"));
    assert!(stderr.contains("success_replay=true failure_replay=false"));
}

#[test]
fn cli_build_connector_contract_gate_passes_with_success_and_failure_cassettes() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_test_project(dir.path());
    write_connector_cassette(dir.path(), "success.json", 200);
    write_connector_cassette(dir.path(), "failure.json", 429);
    let entry = dir.path().join("src").join("main.wr");
    let bin = dir.path().join("wrela.out");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("build")
        .arg(&entry)
        .arg("-o")
        .arg(&bin)
        .output()
        .expect("run build");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(bin.exists(), "build should emit artifact");
}

#[test]
fn cli_verify_cert_passes_for_fresh_build() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_test_project(dir.path());
    let entry = dir.path().join("src").join("main.wr");
    let bin = dir.path().join("verify_cert_ok_bin");
    let cert = dir.path().join("cert.json");

    let build_output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("build")
        .arg(&entry)
        .arg("-o")
        .arg(&bin)
        .output()
        .expect("run build");
    assert!(
        build_output.status.success(),
        "{:?}",
        build_output.status.code()
    );
    assert!(cert.exists(), "expected cert.json");

    let verify_output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("verify-cert")
        .arg(&cert)
        .output()
        .expect("run verify-cert");
    assert!(
        verify_output.status.success(),
        "verify-cert failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&verify_output.stdout),
        String::from_utf8_lossy(&verify_output.stderr)
    );
}

#[test]
fn cli_verify_cert_fails_when_binary_is_tampered() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_test_project(dir.path());
    let entry = dir.path().join("src").join("main.wr");
    let bin = dir.path().join("verify_cert_tamper_bin");
    let cert = dir.path().join("cert.json");

    let build_output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("build")
        .arg(&entry)
        .arg("-o")
        .arg(&bin)
        .output()
        .expect("run build");
    assert!(
        build_output.status.success(),
        "{:?}",
        build_output.status.code()
    );
    assert!(cert.exists(), "expected cert.json");

    let mut bytes = std::fs::read(&bin).expect("read binary");
    bytes.push(0x00);
    std::fs::write(&bin, bytes).expect("tamper binary");

    let verify_output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("verify-cert")
        .arg(&cert)
        .output()
        .expect("run verify-cert");
    assert!(!verify_output.status.success(), "verify-cert should fail");
    let stderr = String::from_utf8_lossy(&verify_output.stderr);
    assert!(
        stderr.contains("binary hash mismatch"),
        "stderr was: {stderr}"
    );
}

#[test]
fn cli_test_maintenance_flags_are_test_only() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_test_project(dir.path());
    let entry = dir.path().join("src").join("main.wr");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("build")
        .arg("--record")
        .arg(&entry)
        .output()
        .expect("run build");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("only valid with `wrela test`"));
}

#[test]
fn cli_test_record_mode_writes_maintenance_summary_without_binary() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_test_project(dir.path());

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("test")
        .arg("--record")
        .arg(".")
        .output()
        .expect("run test");

    assert!(output.status.success(), "{:?}", output.status.code());
    assert!(
        !dir.path().join("wrela.out").exists(),
        "maintenance mode should not emit a deployable binary"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("maintenance mode: --record"));

    let summary_path = dir
        .path()
        .join("tests/.artifacts/maintenance/maintenance-latest.json");
    assert!(summary_path.exists(), "expected maintenance summary json");
    let bytes = std::fs::read(&summary_path).expect("read maintenance summary");
    let json: serde_json::Value = serde_json::from_slice(&bytes).expect("valid maintenance json");
    assert_eq!(
        json.get("mode_record").and_then(|v| v.as_bool()),
        Some(true)
    );
    assert_eq!(
        json.get("mode_update_public_surface")
            .and_then(|v| v.as_bool()),
        Some(false)
    );
    assert_eq!(
        json.get("deployable_artifacts_emitted")
            .and_then(|v| v.as_bool()),
        Some(false)
    );
}

#[test]
fn cli_test_record_mode_writes_http_cassette_and_replay_passes_without_server() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (url, server) = spawn_http_stub_once("pong");
    write_http_integration_test_project(dir.path(), &url);

    let record_output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("test")
        .arg("--record")
        .arg(".")
        .output()
        .expect("run test --record");
    assert!(
        record_output.status.success(),
        "{}",
        String::from_utf8_lossy(&record_output.stderr)
    );
    server.join().expect("join server");

    let cassette_dir = dir.path().join("tests").join("cassettes");
    let mut files = Vec::new();
    collect_json_files(&cassette_dir, &mut files);
    assert_eq!(files.len(), 1, "expected one cassette file, got {files:?}");
    let cassette_bytes = std::fs::read(&files[0]).expect("read cassette");
    let cassette_json: serde_json::Value =
        serde_json::from_slice(&cassette_bytes).expect("valid cassette json");
    assert_eq!(
        cassette_json.get("version").and_then(|v| v.as_u64()),
        Some(1)
    );
    assert_eq!(
        cassette_json
            .get("request")
            .and_then(|v| v.get("method"))
            .and_then(|v| v.as_str()),
        Some("GET")
    );

    let replay_output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("test")
        .arg(".")
        .output()
        .expect("run replay test");
    assert!(
        replay_output.status.success(),
        "{}",
        String::from_utf8_lossy(&replay_output.stderr)
    );
}

#[test]
fn cli_test_replay_mode_reports_missing_http_cassette() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_http_missing_cassette_project(dir.path(), "http://127.0.0.1:9/charge");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("test")
        .arg(".")
        .output()
        .expect("run replay test");

    assert!(
        output.status.success(),
        "missing-cassette path should return Err"
    );
}

#[test]
fn cli_test_rejects_emit_flags_even_in_maintenance_modes() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_test_project(dir.path());
    let out_path = dir.path().join("should_not_exist_bin");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("test")
        .arg("--update-public-surface")
        .arg("-o")
        .arg(&out_path)
        .arg(".")
        .output()
        .expect("run test");

    assert!(!output.status.success());
    assert!(!out_path.exists());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not valid with `wrela test`"));
}

#[test]
fn cli_build_fails_when_public_surface_differs_from_baseline() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_public_surface_project(
        dir.path(),
        "to compute(value: Integer) -> Integer:\n    return value\n",
    );

    let update = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("test")
        .arg("--update-public-surface")
        .arg(".")
        .output()
        .expect("seed public surface baseline");
    assert!(
        update.status.success(),
        "{}",
        String::from_utf8_lossy(&update.stderr)
    );

    std::fs::write(
        dir.path().join("src").join("public_api.wr"),
        "to compute(value: String) -> String:\n    return value\n",
    )
    .expect("mutate public signature");

    let build = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("build")
        .arg("src/main.wr")
        .output()
        .expect("run build");
    assert!(
        !build.status.success(),
        "build unexpectedly passed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );

    let stderr = String::from_utf8_lossy(&build.stderr);
    assert!(stderr.contains("public surface gate failed"), "{stderr}");
    assert!(stderr.contains("changed importable items"));
    assert!(stderr.contains("public_api::compute"));
}

#[test]
fn cli_test_update_public_surface_updates_baseline() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_public_surface_project(
        dir.path(),
        "to compute(value: Integer) -> Integer:\n    return value\n",
    );

    let first = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("test")
        .arg("--update-public-surface")
        .arg(".")
        .output()
        .expect("run first baseline update");
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );

    let baseline_path = dir
        .path()
        .join("tests")
        .join("public_surface.baseline.json");
    let current_path = dir
        .path()
        .join("tests")
        .join(".artifacts")
        .join("public_surface")
        .join("current.json");
    assert!(baseline_path.exists());
    assert!(current_path.exists());

    let baseline_v1: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&baseline_path).expect("read baseline v1"))
            .expect("parse baseline v1");
    assert_eq!(baseline_v1.get("version").and_then(|v| v.as_u64()), Some(1));
    let items_v1 = baseline_v1
        .get("items")
        .and_then(|v| v.as_array())
        .expect("items");
    let compute_v1 = items_v1
        .iter()
        .find(|item| {
            item.get("qualified_name").and_then(|v| v.as_str()) == Some("public_api::compute")
        })
        .expect("compute item present");
    assert_eq!(
        compute_v1.get("signature").and_then(|v| v.as_str()),
        Some("(value: Integer) -> Integer")
    );
    let connector_v1 = items_v1
        .iter()
        .find(|item| {
            item.get("qualified_name").and_then(|v| v.as_str())
                == Some("infrastructure/integrations/http_client::fetch_charge")
        })
        .expect("connector function present");
    assert_eq!(
        connector_v1
            .get("connector_literals")
            .and_then(|v| v.as_array())
            .map(|arr| arr.len()),
        Some(1)
    );

    std::fs::write(
        dir.path().join("src").join("public_api.wr"),
        "to compute(value: String) -> String:\n    return value\n",
    )
    .expect("mutate signature");

    let second = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("test")
        .arg("--update-public-surface")
        .arg(".")
        .output()
        .expect("run second baseline update");
    assert!(
        second.status.success(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );

    let baseline_v2: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&baseline_path).expect("read baseline v2"))
            .expect("parse baseline v2");
    let items_v2 = baseline_v2
        .get("items")
        .and_then(|v| v.as_array())
        .expect("items");
    let compute_v2 = items_v2
        .iter()
        .find(|item| {
            item.get("qualified_name").and_then(|v| v.as_str()) == Some("public_api::compute")
        })
        .expect("compute item present");
    assert_eq!(
        compute_v2.get("signature").and_then(|v| v.as_str()),
        Some("(value: String) -> String")
    );
    let current_v2: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&current_path).expect("read current v2"))
            .expect("parse current v2");
    assert_eq!(baseline_v2, current_v2);
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
fn cli_test_oracle_gate_fails_when_test_has_no_assert_or_require() {
    let dir = tempfile::tempdir().expect("tempdir");
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
    let dir = tempfile::tempdir().expect("tempdir");
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
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("main.wr");
    std::fs::write(
        &path,
        "to run() -> Integer:\n    return 0\n\nto test_trivial() -> Nothing:\n    assert value 1 == 1\n",
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
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("main.wr");
    std::fs::write(
        &path,
        "to run() -> Integer:\n    return 0\n\nto compute_value() -> Integer:\n    return 1\n\nto test_meaningful() -> Nothing:\n    assert value compute_value() == 1\n",
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
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("src");
    let tests = dir.path().join("tests");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::create_dir_all(&tests).unwrap();
    std::fs::write(src.join("main.wr"), "to run() -> Integer:\n    return 0\n").unwrap();
    std::fs::write(
        tests.join("discovery.wr"),
        "to helper() -> Integer:\n    return 1\n\nso: to test_comment_fake() -> Nothing:\n\nto test_real() -> Nothing:\n    assert value helper() == 1\n",
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
fn cli_test_list_includes_autogen_generated_spec_tests() {
    let dir = tempfile::tempdir().expect("tempdir");
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
    let dir = tempfile::tempdir().expect("tempdir");
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
    let dir = tempfile::tempdir().expect("tempdir");
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
fn cli_test_id_and_filter_select_deterministically() {
    let dir = tempfile::tempdir().expect("tempdir");
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
fn cli_test_project_harness_compiles_once_for_full_and_filtered_runs() {
    let dir = tempfile::tempdir().expect("tempdir");
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
    let dir = tempfile::tempdir().expect("tempdir");
    write_test_registry_project(dir.path());
    let beta_id = fnv1a64_hex(b"tests/integration/beta::test_beta");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("test")
        .arg("--format=json")
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
        Some(5000)
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
    assert!(timings.get("execution_ms").is_some());
    assert!(timings.get("total_ms").is_some());
}

#[test]
fn cli_budget_override_env_is_auditable_in_json_and_cert_with_ceiling_enforcement() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_test_project(dir.path());
    let entry = dir.path().join("src").join("main.wr");
    let bin = dir.path().join("budget_override_build_bin");

    let json_output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .env("WRELA_BUDGET_SIM_MAX_CASES", "999999")
        .arg("test")
        .arg("--format=json")
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
        Some(64)
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
        Some(64)
    );
}

#[test]
fn cli_test_json_summary_ordering_is_deterministic_with_parallel_jobs() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_test_registry_project(dir.path());

    let first_output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("test")
        .arg("--format=json")
        .arg("--jobs=4")
        .arg(".")
        .output()
        .expect("run first json summary");
    assert!(first_output.status.success());

    let second_output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("test")
        .arg("--format=json")
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
fn cli_build_fails_when_autogen_catches_wrong_check_property() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_wrong_check_property_project(dir.path());
    let entry = dir.path().join("src").join("main.wr");
    let bin = dir.path().join("autogen_wrong_check_bin");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("build")
        .arg(&entry)
        .arg("-o")
        .arg(&bin)
        .output()
        .expect("run build");

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
            && combined.contains("value_is_positive")
            && combined.contains("seed=")
            && combined.contains("span=")
            && combined.contains("call=`"),
        "expected teacher diagnostics with check/seed/span/call, got:\n{}",
        combined
    );
}

#[test]
fn cli_build_writes_autogen_repro_artifact() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_wrong_check_property_project(dir.path());
    let entry = dir.path().join("src").join("main.wr");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("build")
        .arg(&entry)
        .output()
        .expect("run build");
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
    let dir = tempfile::tempdir().expect("tempdir");
    write_wrong_check_property_project(dir.path());
    let entry = dir.path().join("src").join("main.wr");

    let build = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("build")
        .arg(&entry)
        .output()
        .expect("run build");
    assert!(!build.status.success(), "build unexpectedly passed");

    let autogen_artifacts = dir.path().join("tests").join(".artifacts").join("autogen");
    let mut files = Vec::new();
    collect_json_files(&autogen_artifacts, &mut files);
    assert!(!files.is_empty(), "expected repro artifact");
    files.sort();
    let artifact = files.remove(0);

    let replay = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("test")
        .arg("--repro")
        .arg(&artifact)
        .arg(".")
        .output()
        .expect("run repro");
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
            && combined.contains("value_is_positive")
            && combined.contains("repro="),
        "expected repro failure diagnostics, got:\n{}",
        combined
    );
}

#[test]
fn cli_test_repro_rejects_legacy_artifact_shape() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_wrong_check_property_project(dir.path());
    let legacy_artifact = dir.path().join("legacy_repro.json");
    std::fs::write(
        &legacy_artifact,
        r#"{"version":1,"test_id":"x","module_path":"src/main","func_name":"f"}"#,
    )
    .expect("write legacy repro");

    let replay = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("test")
        .arg("--repro")
        .arg(&legacy_artifact)
        .arg(".")
        .output()
        .expect("run legacy repro");
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
    let dir = tempfile::tempdir().expect("tempdir");
    write_sim_seed_project(dir.path());

    let failing = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("test")
        .arg("--lane=sim")
        .arg("--seed=7")
        .arg(".")
        .output()
        .expect("run sim seed=7");
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

    let passing = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("test")
        .arg("--lane=sim")
        .arg("--seed=8")
        .arg(".")
        .output()
        .expect("run sim seed=8");
    assert!(
        passing.status.success(),
        "seed 8 should pass; stderr:\n{}",
        String::from_utf8_lossy(&passing.stderr)
    );
}

#[test]
fn cli_test_model_lane_seed_filter_and_artifact() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_model_seed_project(dir.path());

    let failing = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("test")
        .arg("--lane=model")
        .arg("--seed=9")
        .arg(".")
        .output()
        .expect("run model seed=9");
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

    let passing = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("test")
        .arg("--lane=model")
        .arg("--seed=10")
        .arg(".")
        .output()
        .expect("run model seed=10");
    assert!(passing.status.success(), "seed 10 should pass");
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
        "to test_basic() -> Nothing:\n    value = 1 + 1\n    assert value value == 2\n",
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
