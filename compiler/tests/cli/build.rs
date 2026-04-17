use super::*;

#[test]
fn cli_build_blocks_artifact_when_certification_fails() {
    let dir = workspace_tempdir();
    write_failing_test_project(dir.path());
    let entry = dir.path().join("src").join("main.wr");
    let bin = dir.path().join("blocked_build_bin");

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_wrela"));
    cmd.arg("build").arg(&entry).arg("-o").arg(&bin);
    apply_fast_cert_budgets(&mut cmd);
    let output = run_command_with_timeout(&mut cmd, Duration::from_secs(120));

    assert!(!output.status.success());
    assert!(
        !bin.exists(),
        "artifact should not exist when certification fails"
    );
}

#[test]
fn cli_build_rejects_lexically_invalid_source() {
    let dir = workspace_tempdir();
    write_lexically_invalid_project(dir.path());
    let entry = dir.path().join("src").join("main.wr");
    let bin = dir.path().join("lex_invalid_build_bin");

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
        "artifact should not exist for lexically invalid source"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unexpected character '$'"), "{stderr}");
}

#[test]
fn cli_build_certification_is_stable_under_replay_marker_mutation() {
    let dir = workspace_tempdir();
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

    assert!(output.status.success(), "{:?}", output.status.code());
    assert!(bin.exists(), "expected build artifact");
}

#[test]
fn cli_build_certification_passes_for_repeatable_outcomes() {
    let dir = workspace_tempdir();
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
    let dir = workspace_tempdir();
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
    let dir = workspace_tempdir();
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
    let dir = workspace_tempdir();
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

    let alpha_function_id = stable_function_id("compute_alpha");
    let beta_function_id = stable_function_id("compute_beta");
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
    let dir = workspace_tempdir();
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
    let dir = workspace_tempdir();
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
    let dir = workspace_tempdir();
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
    let dir = workspace_tempdir();
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
        Some(4),
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
        Some(10000),
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
    let query_contracts = cert
        .get("query_contracts")
        .expect("expected query contract catalog in cert report");
    assert_eq!(
        query_contracts
            .get("schema_version")
            .and_then(|v| v.as_u64()),
        Some(1)
    );
    assert!(
        query_contracts
            .get("contracts")
            .and_then(|v| v.as_array())
            .is_some_and(|contracts| contracts.iter().any(|contract| {
                contract
                    .get("contract_id")
                    .and_then(|v| v.as_str())
                    .is_some_and(|id| id == "spatial.distance.world")
                    && contract
                        .get("call")
                        .and_then(|v| v.as_str())
                        .is_some_and(|call| call == "spatial.distance")
                    && contract
                        .get("target")
                        .and_then(|v| v.as_str())
                        .is_some_and(|target| target == "world")
                    && contract
                        .get("cardinality")
                        .and_then(|v| v.as_str())
                        .is_some_and(|cardinality| cardinality == "scalar")
            })),
        "expected cert report to expose family/query contract identity"
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
    let dir = workspace_tempdir();
    write_test_project(dir.path());
    let entry = dir.path().join("src").join("main.wr");
    let bin = dir.path().join("cached_build_bin");

    let first = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("build")
        .arg("--error-format=json")
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
        .arg("--error-format=json")
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
    let dir = workspace_tempdir();
    write_test_project(dir.path());
    let entry = dir.path().join("src").join("main.wr");
    let bin = dir.path().join("timed_build_bin");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("build")
        .arg("--error-format=json")
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
    assert!(timings.get("cert_collect_tests_ms").is_some());
    assert!(timings.get("cert_compile_harness_ms").is_some());
    assert!(timings.get("cert_determinism_ms").is_some());
    assert!(timings.get("cert_mutation_discovery_ms").is_some());
    assert!(timings.get("cert_mutation_execution_ms").is_some());
    assert!(timings.get("cert_diff_ms").is_some());
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
    let dir = workspace_tempdir();
    write_certified_impact_project(dir.path());
    let bin = dir.path().join("impact_bin");

    let first = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("build")
        .arg("--error-format=json")
        .arg("-o")
        .arg(&bin)
        .arg("src/main.wr")
        .output()
        .expect("run first certified build");
    assert!(first.status.success(), "first build failed");

    write_fixture_file(
        dir.path().join("src").join("core").join("math.wr"),
        r#"fn compute_answer() -> Integer {
    value = 41
    return value
}
"#,
    )
    .unwrap();

    let second = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("build")
        .arg("--error-format=json")
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
    assert_eq!(tests.len(), 1, "expected reduced test selection");
    let names: Vec<&str> = tests
        .iter()
        .filter_map(|value| value.get("name").and_then(|v| v.as_str()))
        .collect();
    assert!(
        names.contains(&"tests/integration/math_flow::test_math_flow")
            || names.contains(&"tests/spec/sanity::test_spec_sanity"),
        "expected impacted test selection, got: {names:?}"
    );
    assert!(!names.contains(&"tests/integration/independent_flow::test_independent_flow"));
    assert!(!names.contains(&"tests/sim/queue_sim::test_queue_sim"));
}

#[test]
fn cli_build_fails_when_differential_alt_pipeline_diverges() {
    let dir = workspace_tempdir();
    write_differential_divergence_project(dir.path());
    let entry = dir.path().join("src").join("main.wr");

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_wrela"));
    cmd.arg("build").arg(&entry);
    apply_fast_cert_budgets(&mut cmd);
    let output = run_command_with_timeout(&mut cmd, Duration::from_secs(60));
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
    let dir = workspace_tempdir();
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
    let dir = workspace_tempdir();
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
    let dir = workspace_tempdir();
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
    let dir = workspace_tempdir();
    write_fuzz_failure_project(dir.path());
    let entry = dir.path().join("src").join("main.wr");
    let bin = dir.path().join("fuzz_build_bin");

    let mut build_cmd = Command::new(env!("CARGO_BIN_EXE_wrela"));
    build_cmd.arg("build").arg(&entry).arg("-o").arg(&bin);
    apply_fast_cert_budgets(&mut build_cmd);
    build_cmd.env("WRELA_BUDGET_TEST_TIMEOUT_MS", "200");
    let output = run_command_with_timeout(&mut build_cmd, Duration::from_secs(90));
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
    let mut replay_cmd = Command::new(env!("CARGO_BIN_EXE_wrela"));
    replay_cmd
        .arg("test")
        .arg(dir.path())
        .arg("--repro")
        .arg(&artifacts[0]);
    let replay = run_command_with_timeout(&mut replay_cmd, Duration::from_secs(120));
    assert!(
        !replay.status.success(),
        "expected repro to replay fuzz failure"
    );
}

#[test]
fn cli_build_mutation_gate_fails_for_weak_tests_and_passes_for_strong_tests() {
    let weak = workspace_tempdir();
    write_mutation_project(weak.path(), false);
    let weak_entry = weak.path().join("src").join("main.wr");
    let weak_output = run_build_with_fast_cert(&weak_entry, Duration::from_secs(180), |_| {});
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
        Some(4),
        "expected mutation report schema version hard cutover"
    );
    assert!(
        weak_report_json
            .get("discovery_ms")
            .and_then(|v| v.as_u64())
            .is_some(),
        "expected mutation discovery timing in report"
    );
    assert!(
        weak_report_json
            .get("execution_ms")
            .and_then(|v| v.as_u64())
            .is_some(),
        "expected mutation execution timing in report"
    );
    assert!(
        weak_report_json
            .get("compile_total_ms")
            .and_then(|v| v.as_u64())
            .is_some(),
        "expected mutation compile total timing in report"
    );
    assert!(
        weak_report_json
            .get("test_run_total_ms")
            .and_then(|v| v.as_u64())
            .is_some(),
        "expected mutation run total timing in report"
    );
    assert!(
        weak_report_json
            .get("parallel_workers")
            .and_then(|v| v.as_u64())
            .is_some(),
        "expected mutation worker count in report"
    );
    assert!(
        weak_report_json
            .get("cache_hits")
            .and_then(|v| v.as_u64())
            .is_some(),
        "expected mutation cache hit counter in report"
    );
    assert!(
        weak_report_json
            .get("cache_misses")
            .and_then(|v| v.as_u64())
            .is_some(),
        "expected mutation cache miss counter in report"
    );
    assert!(
        weak_report_json
            .get("cache_invalidations")
            .and_then(|v| v.as_u64())
            .is_some(),
        "expected mutation cache invalidation counter in report"
    );
    assert!(
        weak_report_json
            .get("mutants")
            .and_then(|v| v.as_array())
            .is_some_and(|mutants| mutants.iter().all(|mutant| {
                mutant.get("compile_ms").and_then(|v| v.as_u64()).is_some()
                    && mutant.get("test_run_ms").and_then(|v| v.as_u64()).is_some()
            })),
        "expected per-mutant compile_ms/test_run_ms fields"
    );
    assert!(
        weak_report_json
            .get("survived_mutants")
            .and_then(|v| v.as_u64())
            .is_some_and(|count| count > 0),
        "expected surviving mutants in weak report"
    );

    let strong = workspace_tempdir();
    write_mutation_project(strong.path(), true);
    let strong_entry = strong.path().join("src").join("main.wr");
    let strong_output = run_build_with_fast_cert(&strong_entry, Duration::from_secs(180), |_| {});
    assert!(
        strong_output.status.success(),
        "strong project should pass mutation gate: stderr={}",
        String::from_utf8_lossy(&strong_output.stderr)
    );
}

#[test]
fn cli_build_mutation_gate_excludes_invalid_mutants_from_denominator() {
    let dir = workspace_tempdir();
    write_mutation_project(dir.path(), true);
    let mutation_root = dir.path().join("target").join("wrela_mutation");
    std::fs::create_dir_all(&mutation_root).expect("create mutation root");
    let blocked_component =
        mutation_root.join("compute_logic_value__integer_literal_perturbation__0");
    write_fixture_file(&blocked_component, r#"blocked"#).expect("write blocked mutation path");

    let entry = dir.path().join("src").join("main.wr");
    let output = run_build_with_fast_cert(&entry, Duration::from_secs(180), |_| {});
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
fn cli_build_mutation_gate_results_are_deterministic_across_worker_counts() {
    let dir = workspace_tempdir();
    write_mutation_project(dir.path(), true);
    let entry = dir.path().join("src").join("main.wr");

    let serial = run_build_with_fast_cert(&entry, Duration::from_secs(180), |cmd| {
        cmd.env("WRELA_MUTATION_WORKERS", "1");
    });
    assert!(
        serial.status.success(),
        "serial mutation build should pass: stdout={}\nstderr={}",
        String::from_utf8_lossy(&serial.stdout),
        String::from_utf8_lossy(&serial.stderr)
    );
    let report_path = dir
        .path()
        .join("tests")
        .join(".artifacts")
        .join("mutation")
        .join("report.json");
    let serial_report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&report_path).expect("read serial mutation report"))
            .expect("parse serial mutation report");

    let parallel = run_build_with_fast_cert(&entry, Duration::from_secs(180), |cmd| {
        cmd.env("WRELA_MUTATION_WORKERS", "4");
    });
    assert!(
        parallel.status.success(),
        "parallel mutation build should pass: stdout={}\nstderr={}",
        String::from_utf8_lossy(&parallel.stdout),
        String::from_utf8_lossy(&parallel.stderr)
    );
    let parallel_report: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&report_path).expect("read parallel mutation report"),
    )
    .expect("parse parallel mutation report");

    let serial_semantic = serial_report
        .get("mutants")
        .and_then(|v| v.as_array())
        .expect("serial mutants")
        .iter()
        .map(|mutant| {
            (
                mutant
                    .get("function")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                mutant
                    .get("function_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                mutant
                    .get("mutation_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                mutant
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                mutant
                    .get("reason")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                mutant
                    .get("tests_ran")
                    .and_then(|v| v.as_array())
                    .map(|tests| {
                        tests
                            .iter()
                            .filter_map(|test| test.as_str().map(str::to_string))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default(),
            )
        })
        .collect::<Vec<_>>();
    let parallel_semantic = parallel_report
        .get("mutants")
        .and_then(|v| v.as_array())
        .expect("parallel mutants")
        .iter()
        .map(|mutant| {
            (
                mutant
                    .get("function")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                mutant
                    .get("function_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                mutant
                    .get("mutation_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                mutant
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                mutant
                    .get("reason")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                mutant
                    .get("tests_ran")
                    .and_then(|v| v.as_array())
                    .map(|tests| {
                        tests
                            .iter()
                            .filter_map(|test| test.as_str().map(str::to_string))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default(),
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(
        serial_semantic, parallel_semantic,
        "mutation semantics should match across worker counts"
    );
}

#[test]
fn cli_build_mutation_cache_hits_on_second_build() {
    let dir = workspace_tempdir();
    write_mutation_project(dir.path(), true);
    let entry = dir.path().join("src").join("main.wr");
    let report_path = dir
        .path()
        .join("tests")
        .join(".artifacts")
        .join("mutation")
        .join("report.json");

    let first = run_build_with_fast_cert(&entry, Duration::from_secs(180), |cmd| {
        cmd.env("WRELA_MUTATION_CACHE", "on");
    });
    assert!(
        first.status.success(),
        "first build should pass: stdout={}\nstderr={}",
        String::from_utf8_lossy(&first.stdout),
        String::from_utf8_lossy(&first.stderr)
    );
    let first_report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&report_path).expect("read first report"))
            .expect("parse first report");
    let first_misses = first_report
        .get("cache_misses")
        .and_then(|v| v.as_u64())
        .expect("first cache misses");
    let first_compile_total = first_report
        .get("compile_total_ms")
        .and_then(|v| v.as_u64())
        .expect("first compile total");
    assert!(
        first_misses > 0,
        "expected first mutation build to compile mutants"
    );
    std::fs::remove_dir_all(dir.path().join("target").join("wrela_cert"))
        .expect("clear cert cache to force mutation rerun");

    let second = run_build_with_fast_cert(&entry, Duration::from_secs(180), |cmd| {
        cmd.env("WRELA_MUTATION_CACHE", "on");
    });
    assert!(
        second.status.success(),
        "second build should pass: stdout={}\nstderr={}",
        String::from_utf8_lossy(&second.stdout),
        String::from_utf8_lossy(&second.stderr)
    );
    let second_report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&report_path).expect("read second report"))
            .expect("parse second report");
    let second_hits = second_report
        .get("cache_hits")
        .and_then(|v| v.as_u64())
        .expect("second cache hits");
    let second_compile_total = second_report
        .get("compile_total_ms")
        .and_then(|v| v.as_u64())
        .expect("second compile total");
    assert!(second_hits > 0, "expected cache hits on second build");
    assert!(
        second_compile_total <= first_compile_total,
        "expected compile total to drop or remain equal with cache: first={first_compile_total} second={second_compile_total}"
    );
}

#[test]
fn cli_build_mutation_cache_invalidates_stale_metadata() {
    let dir = workspace_tempdir();
    write_mutation_project(dir.path(), true);
    let entry = dir.path().join("src").join("main.wr");
    let report_path = dir
        .path()
        .join("tests")
        .join(".artifacts")
        .join("mutation")
        .join("report.json");

    let first = run_build_with_fast_cert(&entry, Duration::from_secs(180), |cmd| {
        cmd.env("WRELA_MUTATION_CACHE", "on");
    });
    assert!(first.status.success(), "first build should pass");

    let cache_root = dir.path().join("target").join("wrela_mutation_cache");
    let metadata_path = std::fs::read_dir(&cache_root)
        .expect("read cache root")
        .filter_map(Result::ok)
        .map(|entry| entry.path().join("metadata.json"))
        .find(|path| path.is_file())
        .expect("expected at least one mutation cache metadata file");
    let mut metadata: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&metadata_path).expect("read metadata"))
            .expect("parse metadata");
    metadata["schema_version"] = serde_json::json!(0);
    write_fixture_file(
        &metadata_path,
        serde_json::to_vec_pretty(&metadata).expect("serialize corrupted metadata"),
    )
    .expect("write corrupted metadata");
    std::fs::remove_dir_all(dir.path().join("target").join("wrela_cert"))
        .expect("clear cert cache to force mutation rerun");

    let second = run_build_with_fast_cert(&entry, Duration::from_secs(180), |cmd| {
        cmd.env("WRELA_MUTATION_CACHE", "on");
    });
    assert!(
        second.status.success(),
        "second build should pass after invalidating stale metadata: stdout={}\nstderr={}",
        String::from_utf8_lossy(&second.stdout),
        String::from_utf8_lossy(&second.stderr)
    );
    let second_report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&report_path).expect("read second report"))
            .expect("parse second report");
    let invalidations = second_report
        .get("cache_invalidations")
        .and_then(|v| v.as_u64())
        .expect("cache invalidations");
    assert!(
        invalidations > 0,
        "expected stale cache metadata to trigger invalidation"
    );
}

#[test]
fn cli_build_mutation_kill_history_prioritizes_seeded_test() {
    let dir = workspace_tempdir();
    write_mutation_project(dir.path(), true);
    let entry = dir.path().join("src").join("main.wr");
    let report_path = dir
        .path()
        .join("tests")
        .join(".artifacts")
        .join("mutation")
        .join("report.json");

    let baseline = run_build_with_fast_cert(&entry, Duration::from_secs(180), |cmd| {
        cmd.env("WRELA_MUTATION_WORKERS", "1")
            .env("WRELA_MUTATION_CACHE", "off");
    });
    assert!(baseline.status.success(), "baseline build should pass");
    let baseline_report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&report_path).expect("read baseline report"))
            .expect("parse baseline report");
    let candidate = baseline_report
        .get("mutants")
        .and_then(|v| v.as_array())
        .and_then(|mutants| {
            mutants.iter().find_map(|mutant| {
                let tests = mutant.get("tests_ran").and_then(|v| v.as_array())?;
                if tests.is_empty() {
                    return None;
                }
                let first = tests.first()?.as_str()?.to_string();
                let last = tests.last()?.as_str()?.to_string();
                Some((
                    mutant.get("function_id")?.as_str()?.to_string(),
                    mutant.get("mutation_type")?.as_str()?.to_string(),
                    first,
                    last,
                ))
            })
        })
        .expect("expected baseline mutant with executed tests");
    let (function_id, mutation_type, baseline_first_test_id, baseline_last_test_id) = candidate;
    let preferred_test_id = if baseline_first_test_id == baseline_last_test_id {
        baseline_first_test_id
    } else {
        baseline_last_test_id
    };

    let history_key = format!("{function_id}|{mutation_type}|{preferred_test_id}");
    let history_payload = serde_json::json!({
        "schema_version": 1,
        "entries": {
            history_key: {
                "kills": 100,
                "attempts": 100,
                "last_seen_unix_ms": 1
            }
        }
    });
    let history_path = dir
        .path()
        .join("target")
        .join("wrela_mutation_cache")
        .join("kill_history.json");
    std::fs::create_dir_all(
        history_path
            .parent()
            .expect("kill history parent should exist"),
    )
    .expect("create kill history directory");
    write_fixture_file(
        &history_path,
        serde_json::to_vec_pretty(&history_payload).expect("serialize kill history"),
    )
    .expect("write kill history");

    let seeded = run_build_with_fast_cert(&entry, Duration::from_secs(180), |cmd| {
        cmd.env("WRELA_MUTATION_WORKERS", "1")
            .env("WRELA_MUTATION_CACHE", "off");
    });
    assert!(
        seeded.status.success(),
        "seeded build should pass: stdout={}\nstderr={}",
        String::from_utf8_lossy(&seeded.stdout),
        String::from_utf8_lossy(&seeded.stderr)
    );
    let seeded_report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&report_path).expect("read seeded report"))
            .expect("parse seeded report");
    let seeded_first_test = seeded_report
        .get("mutants")
        .and_then(|v| v.as_array())
        .and_then(|mutants| {
            mutants.iter().find_map(|mutant| {
                (mutant.get("function_id").and_then(|v| v.as_str()) == Some(function_id.as_str())
                    && mutant.get("mutation_type").and_then(|v| v.as_str())
                        == Some(mutation_type.as_str()))
                .then(|| {
                    mutant
                        .get("tests_ran")
                        .and_then(|v| v.as_array())
                        .and_then(|tests| tests.first())
                        .and_then(|value| value.as_str())
                        .map(str::to_string)
                })
                .flatten()
            })
        })
        .expect("expected seeded mutant result");
    assert_eq!(
        seeded_first_test, preferred_test_id,
        "expected seeded kill-history test to run first"
    );
}

#[test]
fn cli_build_rejects_coverage_id_alias_collisions() {
    let dir = workspace_tempdir();
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
        stderr.contains("import 'compute_shared' conflicts")
            || stderr.contains("previous import of 'compute_shared'"),
        "expected duplicate import conflict error, got:\n{stderr}"
    );
}

#[test]
fn cli_build_ignores_fake_alias_signatures_in_non_code_text() {
    let dir = workspace_tempdir();
    write_alias_noise_project(dir.path());
    let entry = dir.path().join("src").join("main.wr");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("build")
        .arg(&entry)
        .output()
        .expect("run build");
    assert!(
        output.status.success(),
        "build should ignore fake signatures in comments/strings; stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn cli_build_rejects_parse_invalid_src_module_during_alias_mapping() {
    let dir = workspace_tempdir();
    write_parse_invalid_src_module_project(dir.path());
    let entry = dir.path().join("src").join("main.wr");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("build")
        .arg(&entry)
        .output()
        .expect("run build");
    assert!(
        output.status.success(),
        "build should ignore parse-invalid sibling module now that legacy alias mapping is removed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn cli_build_cache_invalidates_when_relevant_wr_source_changes() {
    let dir = workspace_tempdir();
    write_test_project(dir.path());
    let entry = dir.path().join("src").join("main.wr");
    let bin = dir.path().join("cache_invalidation_build_bin");

    let first = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("build")
        .arg("--error-format=json")
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

    write_fixture_file(
        &entry,
        r#"fn run() -> Integer {
    return 1
}
"#,
    )
    .expect("mutate source");

    let second = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("build")
        .arg("--error-format=json")
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
    let dir = workspace_tempdir();
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
    let dir = workspace_tempdir();
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
    let dir = workspace_tempdir();
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
    let dir = workspace_tempdir();
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
    write_fixture_file(&bin, bytes).expect("tamper binary");

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
