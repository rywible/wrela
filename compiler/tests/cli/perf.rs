use super::*;

#[test]
fn cli_test_perf_summary() {
    let dir = workspace_tempdir();
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
    let dir = workspace_tempdir();
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
    let dir = workspace_tempdir();
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
    let closure = json.get("closure").expect("closure");
    assert_eq!(
        closure
            .pointer("/profile/name")
            .and_then(|value| value.as_str()),
        Some("canonical_1080p120_wgsl_resident")
    );
    assert_eq!(
        closure
            .pointer("/profile/execution_story")
            .and_then(|value| value.as_str()),
        Some("wgsl_resident")
    );
    assert_eq!(
        closure
            .pointer("/profile/backend")
            .and_then(|value| value.as_str()),
        Some("wgsl")
    );
    assert_eq!(
        closure
            .pointer("/profile/adapter_name")
            .and_then(|value| value.as_str()),
        Some("wgsl_resident")
    );
    assert_eq!(
        closure
            .pointer("/cpu_oracle_profile/name")
            .and_then(|value| value.as_str()),
        Some("canonical_1080p120_cpu_oracle")
    );
    assert_eq!(
        closure
            .pointer("/frame/status")
            .and_then(|value| value.as_str()),
        Some("not_sampled")
    );
    assert_eq!(
        closure
            .pointer("/collision/status")
            .and_then(|value| value.as_str()),
        Some("not_sampled")
    );
    assert_eq!(
        closure
            .pointer("/verdict/status")
            .and_then(|value| value.as_str()),
        Some("not_applicable")
    );
    let metrics = summary.get("metrics").expect("summary.metrics");
    assert!(metrics.get("scene_trace").is_some());
    assert!(metrics.get("field_sample").is_some());
    assert!(metrics.get("scene_trace_candidate_branch").is_some());
    assert!(metrics.get("scene_trace_support_pruned_branch").is_some());
    assert!(metrics.get("scene_trace_hit_count").is_some());
}

#[test]
fn cli_perf_runs_field_engine_manifest_smoke_on_wgsl() {
    let dir = workspace_tempdir();
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf();
    let bench_root = repo_root.join("benchmarks/field_engine");
    let manifest = dir.path().join("field_engine_smoke.toml");
    let baseline = dir.path().join("field_engine_smoke.json");
    write_fixture_file(
        &manifest,
        r#"
version = 1
suite = "field_engine_smoke"

[profiles.smoke]
warmup_pairs = 1
measure_pairs = 1
coverage = "all"

[[scenarios]]
id = "thin_nested_local_frame"
test_name = "tests/field_engine::test_field_thin_nested_local_frame_ops_100000"
ops = 100000
class = "critical"
min_runtime_ms = 1
timeout_ms = 120000
allow_unstable = false
"#,
    )
    .expect("write field-engine smoke manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(&repo_root)
        .arg("perf")
        .arg("--runs=1")
        .arg("--profile=smoke")
        .arg("--query-backend=wgsl")
        .arg(format!("--benchmark-manifest={}", manifest.display()))
        .arg(format!("--baseline-out={}", baseline.display()))
        .arg(&bench_root)
        .output()
        .expect("run field-engine perf smoke");
    assert!(
        output.status.success(),
        "field-engine perf smoke failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(baseline.exists(), "expected field-engine perf baseline");
    let json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&baseline).expect("read field-engine baseline"))
            .expect("parse field-engine baseline");
    let cases = json
        .get("summary")
        .and_then(|value| value.get("cases"))
        .and_then(|value| value.as_array())
        .expect("summary.cases array");
    assert_eq!(cases.len(), 1);
    assert_eq!(
        cases[0].get("name").and_then(|value| value.as_str()),
        Some("tests/field_engine::test_field_thin_nested_local_frame_ops_100000")
    );
    let metrics = json
        .get("summary")
        .and_then(|value| value.get("metrics"))
        .expect("summary.metrics");
    assert!(
        metrics
            .get("scene_trace")
            .and_then(|value| value.as_u64())
            .unwrap_or(0)
            > 0
    );
    assert!(
        metrics
            .get("field_sample")
            .and_then(|value| value.as_u64())
            .unwrap_or(0)
            > 0
    );
    assert!(
        metrics
            .get("scene_trace_hit_count")
            .and_then(|value| value.as_u64())
            .unwrap_or(0)
            > 0
    );
}

#[test]
fn cli_perf_realtime_presentation_smoke_records_phase37_solver_counters() {
    let dir = workspace_tempdir();
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf();
    let bench_root = repo_root.join("benchmarks/realtime_presentation");
    let manifest = dir.path().join("realtime_presentation_phase37_smoke.toml");
    let baseline = dir.path().join("realtime_presentation_phase37_smoke.json");
    write_fixture_file(
        &manifest,
        r#"
version = 1
suite = "realtime_presentation_phase37_smoke"

[profiles.smoke]
warmup_pairs = 1
measure_pairs = 1
coverage = "all"

[[scenarios]]
id = "presentation_relaxed_torus_scene"
test_name = "tests/realtime_presentation::test_realtime_presentation_relaxed_torus_scene_ops_256"
ops = 256
class = "critical"
min_runtime_ms = 1
timeout_ms = 180000
allow_unstable = false
presentation = { entry = "tests/realtime_presentation_test.wr", view = "show_relaxed_torus_view", region = "relaxed_torus_region", domain = "relaxed_torus_domain", width = 64, height = 64, frames = 4, camera_position = [0.0, 0.0, 3.2], camera_forward = [0.0, 0.0, -1.0], camera_up = [0.0, 1.0, 0.0], vertical_fov_degrees = 46.0 }

[[scenarios]]
id = "presentation_repetition_heavy_scene"
test_name = "tests/realtime_presentation::test_realtime_presentation_repetition_heavy_scene_ops_512"
ops = 512
class = "critical"
min_runtime_ms = 1
timeout_ms = 180000
allow_unstable = false
presentation = { entry = "tests/realtime_presentation_test.wr", view = "show_repetition_view", region = "repetition_region", domain = "repetition_domain", width = 64, height = 64, frames = 4, camera_position = [0.0, 0.0, 3.0], camera_forward = [0.0, 0.0, -1.0], camera_up = [0.0, 1.0, 0.0], vertical_fov_degrees = 48.0 }

[[scenarios]]
id = "presentation_repeat_linear_solver_scene"
test_name = "tests/realtime_presentation::test_realtime_presentation_repeat_linear_solver_scene_ops_256"
ops = 256
class = "critical"
min_runtime_ms = 1
timeout_ms = 180000
allow_unstable = false
presentation = { entry = "tests/realtime_presentation_test.wr", view = "show_repeat_linear_solver_view", region = "repeat_linear_solver_region", domain = "repeat_linear_solver_domain", width = 64, height = 64, frames = 4, camera_position = [-15.0, 0.0, 0.0], camera_forward = [1.0, 0.0, 0.0], camera_up = [0.0, 1.0, 0.0], vertical_fov_degrees = 12.0 }
"#,
    )
    .expect("write realtime presentation phase37 smoke manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(&repo_root)
        .arg("perf")
        .arg("--runs=1")
        .arg("--profile=smoke")
        .arg("--query-backend=cpu")
        .arg(format!("--benchmark-manifest={}", manifest.display()))
        .arg(format!("--baseline-out={}", baseline.display()))
        .arg(&bench_root)
        .output()
        .expect("run realtime presentation phase37 perf smoke");
    assert!(
        output.status.success(),
        "realtime presentation phase37 perf smoke failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(baseline.exists(), "expected realtime perf baseline");

    let json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&baseline).expect("read realtime baseline"))
            .expect("parse realtime baseline");
    let presentation_reports = json
        .get("presentation_reports")
        .and_then(|value| value.as_array())
        .expect("presentation reports array");
    assert_eq!(presentation_reports.len(), 3);
    let report_for = |scenario_id: &str| {
        presentation_reports
            .iter()
            .find(|report| {
                report.get("scenario_id").and_then(|value| value.as_str()) == Some(scenario_id)
            })
            .unwrap_or_else(|| panic!("missing presentation report for {scenario_id}"))
    };
    let relaxed_torus = report_for("presentation_relaxed_torus_scene");
    assert_eq!(
        relaxed_torus
            .get("query_trace_solver_mode")
            .and_then(|value| value.as_str()),
        Some("hybrid")
    );
    assert!(
        relaxed_torus
            .pointer("/frame_cost/field_samples")
            .and_then(|value| value.as_u64())
            .unwrap_or(0)
            > 0,
        "expected field samples in relaxed torus benchmark report"
    );
    assert!(
        relaxed_torus
            .get("ab_comparison")
            .and_then(|value| value.as_object())
            .is_some_and(|comparison| {
                comparison
                    .get("dense_only_query_trace_solver_mode")
                    .and_then(|value| value.as_str())
                    == Some("dense-only")
                    && comparison
                        .get("frame_time_ns_delta_vs_dense_only")
                        .and_then(|value| value.as_i64())
                        .is_some()
                    && comparison
                        .get("average_trace_steps_delta_vs_dense_only")
                        .and_then(|value| value.as_f64())
                        .is_some()
                    && comparison
                        .get("field_samples_delta_vs_dense_only")
                        .and_then(|value| value.as_i64())
                        .is_some()
                    && comparison
                        .get("candidate_count_before_pruning_delta_vs_dense_only")
                        .and_then(|value| value.as_i64())
                        .is_some()
                    && comparison
                        .get("candidate_count_after_pruning_delta_vs_dense_only")
                        .and_then(|value| value.as_i64())
                        .is_some()
            }),
        "expected dense-only comparison payload in relaxed torus benchmark report"
    );
    assert!(
        relaxed_torus
            .pointer("/frame_cost/average_trace_steps")
            .and_then(|value| value.as_f64())
            .unwrap_or(0.0)
            > 0.0,
        "expected trace-step accounting in relaxed torus benchmark report"
    );
    assert_eq!(
        relaxed_torus
            .pointer("/frame_cost/accepted_relaxed_steps")
            .and_then(|value| value.as_u64()),
        Some(0)
    );
    assert_eq!(
        relaxed_torus
            .pointer("/frame_cost/solver_interval_attempts")
            .and_then(|value| value.as_u64()),
        Some(0)
    );
    assert_eq!(
        relaxed_torus
            .pointer("/frame_cost/solver_relaxed_attempts")
            .and_then(|value| value.as_u64()),
        Some(0)
    );
    assert_eq!(
        relaxed_torus
            .pointer("/frame_cost/solver_refinement_attempts")
            .and_then(|value| value.as_u64()),
        Some(0)
    );
    assert!(
        relaxed_torus
            .pointer("/ab_comparison/field_samples_delta_vs_dense_only")
            .and_then(|value| value.as_i64())
            .is_some_and(|delta| delta <= 0),
        "torus should not exceed dense-only field sampling in default hybrid mode"
    );
    assert_eq!(
        relaxed_torus
            .pointer("/frame_cost/solver_repeat_cells_enumerated")
            .and_then(|value| value.as_u64()),
        Some(0)
    );
    let repetition = report_for("presentation_repeat_linear_solver_scene");
    assert!(
        repetition
            .pointer("/frame_cost/repeat_cell_skips")
            .and_then(|value| value.as_u64())
            .unwrap_or(0)
            > 0,
        "expected repeat-aware traversal to skip repeated cells"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("solver counters relaxed_attempts=")
            && stdout.contains("relaxed_no_root_advances=")
            && stdout.contains("repeat_unsupported_form=")
            && stdout.contains("repeat_unsupported_bounds=")
            && stdout.contains("repeat_cells_enumerated="),
        "expected solver counter summary in perf stdout: {stdout}"
    );
}

#[test]
fn cli_perf_runs_realtime_presentation_1080p120_closure_profile() {
    let dir = workspace_tempdir();
    let bench_root = dir.path().join("realtime_presentation_fixture");
    write_realtime_presentation_closure_benchmark_project(&bench_root);
    let baseline = dir.path().join("realtime_presentation_1080p120.json");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("perf")
        .arg("--runs=1")
        .arg("--profile=1080p120")
        .arg("--query-backend=wgsl")
        .arg(format!("--baseline-out={}", baseline.display()))
        .arg(&bench_root)
        .output()
        .expect("run realtime_presentation closure perf");
    assert!(
        output.status.success(),
        "realtime presentation closure perf failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&baseline).expect("read realtime baseline"))
            .expect("parse realtime baseline");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("presentation-scenario"));
    assert!(stdout.contains("fps="));
    let closure = json.get("closure").expect("closure report");
    assert_eq!(
        closure
            .pointer("/profile/name")
            .and_then(|value| value.as_str()),
        Some("canonical_1080p120_wgsl_resident")
    );
    assert_eq!(
        closure
            .pointer("/cpu_oracle_profile/name")
            .and_then(|value| value.as_str()),
        Some("canonical_1080p120_cpu_oracle")
    );
    assert_eq!(
        closure
            .pointer("/profile/output_width")
            .and_then(|value| value.as_u64()),
        Some(1920)
    );
    assert_eq!(
        closure
            .pointer("/profile/target_fps")
            .and_then(|value| value.as_u64()),
        Some(120)
    );
    assert_eq!(
        closure
            .pointer("/frame/status")
            .and_then(|value| value.as_str()),
        Some("not_sampled")
    );
    assert_eq!(
        closure
            .pointer("/collision/status")
            .and_then(|value| value.as_str()),
        Some("not_sampled")
    );
    assert_eq!(
        closure
            .pointer("/verdict/status")
            .and_then(|value| value.as_str()),
        Some("not_applicable")
    );
    assert!(
        closure
            .pointer("/verdict/summary")
            .and_then(|value| value.as_str())
            .is_some()
    );
    let presentation_reports = json
        .get("presentation_reports")
        .and_then(|value| value.as_array())
        .expect("presentation reports array");
    assert_eq!(presentation_reports.len(), 1);
    let report = presentation_reports
        .first()
        .expect("first presentation report");
    let observed_adapter_name = report
        .get("observed_adapter_name")
        .and_then(|value| value.as_str())
        .expect("observed adapter name");
    assert_eq!(
        report
            .pointer("/quality_tier")
            .and_then(|value| value.as_str()),
        Some("realtime_120")
    );
    assert_eq!(
        report.pointer("/backend").and_then(|value| value.as_str()),
        Some("wgsl")
    );
    assert!(
        report
            .pointer("/frame_time_ns")
            .and_then(|value| value.as_u64())
            .is_some_and(|value| value > 0)
    );
    assert!(
        report
            .pointer("/steady_state_fps")
            .and_then(|value| value.as_f64())
            .is_some_and(|value| value > 0.0)
    );
    assert!(report.get("wgsl_workgroup_comparison").is_none());
    assert!(report.get("ab_comparison").is_none());
    assert_eq!(
        closure
            .pointer("/profile/adapter_name")
            .and_then(|value| value.as_str()),
        Some(observed_adapter_name)
    );
    assert_eq!(
        closure
            .pointer("/profile/timestamps_enabled")
            .and_then(|value| value.as_bool()),
        report
            .pointer("/frame_cost/gpu_runtime/timestamps_supported")
            .and_then(|value| value.as_bool())
    );
    assert!(json.get("collision_reports").is_none());
    assert!(json.get("whole_frame_reports").is_none());
}

#[test]
fn cli_perf_runs_whole_frame_1080p120_closure_profile() {
    let dir = workspace_tempdir();
    let bench_root = dir.path().join("whole_frame_fixture");
    write_whole_frame_closure_benchmark_project(&bench_root);
    let baseline = dir.path().join("whole_frame_1080p120.json");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("perf")
        .arg("--runs=1")
        .arg("--profile=1080p120")
        .arg(format!("--baseline-out={}", baseline.display()))
        .arg(&bench_root)
        .output()
        .expect("run whole_frame closure perf");
    assert!(
        output.status.success(),
        "whole frame closure perf failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&baseline).expect("read whole-frame baseline"))
            .expect("parse whole-frame baseline");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("presentation-scenario"));
    assert!(stdout.contains("collision-scenario"));
    assert!(stdout.contains("whole-frame-scenario"));

    let closure = json.get("closure").expect("closure report");
    assert_eq!(
        closure
            .pointer("/frame/suite")
            .and_then(|value| value.as_str()),
        Some("whole_frame")
    );
    assert_eq!(
        closure
            .pointer("/collision/suite")
            .and_then(|value| value.as_str()),
        Some("whole_frame")
    );
    assert!(matches!(
        closure
            .pointer("/frame/status")
            .and_then(|value| value.as_str()),
        Some("sampled") | Some("validated") | Some("violated")
    ));
    assert!(matches!(
        closure
            .pointer("/collision/status")
            .and_then(|value| value.as_str()),
        Some("sampled") | Some("validated") | Some("violated")
    ));
    assert!(matches!(
        closure
            .pointer("/verdict/status")
            .and_then(|value| value.as_str()),
        Some("met") | Some("failed")
    ));
    let presentation_reports = json
        .get("presentation_reports")
        .and_then(|value| value.as_array())
        .expect("presentation reports array");
    let collision_reports = json
        .get("collision_reports")
        .and_then(|value| value.as_array())
        .expect("collision reports array");
    let whole_frame_reports = json
        .get("whole_frame_reports")
        .and_then(|value| value.as_array())
        .expect("whole-frame reports array");
    assert_eq!(presentation_reports.len(), 1);
    assert_eq!(collision_reports.len(), 1);
    assert_eq!(whole_frame_reports.len(), 1);
    assert_eq!(
        presentation_reports[0]
            .pointer("/backend")
            .and_then(|value| value.as_str()),
        Some("wgsl")
    );
    assert_eq!(
        collision_reports[0]
            .pointer("/backend")
            .and_then(|value| value.as_str()),
        Some("wgsl")
    );
    assert!(
        whole_frame_reports[0]
            .pointer("/total_runtime_ns")
            .and_then(|value| value.as_u64())
            .is_some_and(|value| value > 0)
    );
    assert!(
        whole_frame_reports[0]
            .pointer("/steady_state_fps")
            .and_then(|value| value.as_f64())
            .is_some_and(|value| value > 0.0)
    );
}

#[test]
fn cli_perf_why_not_120_mode_prints_closure_verdict_and_diagnostics() {
    let dir = workspace_tempdir();
    let bench_root = dir.path().join("whole_frame_fixture");
    write_whole_frame_closure_benchmark_project(&bench_root);
    let baseline = dir.path().join("whole_frame_1080p120_diagnostics.json");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("perf")
        .arg("--runs=1")
        .arg("--profile=1080p120")
        .arg("--query-backend=wgsl")
        .arg("--why-not-120")
        .arg(format!("--baseline-out={}", baseline.display()))
        .arg(&bench_root)
        .output()
        .expect("run whole_frame closure perf diagnostics");
    assert!(
        output.status.success(),
        "whole-frame closure diagnostics failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("closure verdict:"));
    assert!(stdout.contains("wgsl_resident"));
    assert!(stdout.contains("cpu-oracle companion:"));
    assert!(stdout.contains("why-not-120:"));
    assert!(stdout.contains("frame median:"));
    assert!(stdout.contains("FPS)"));
    let json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&baseline).expect("read diagnostics baseline"))
            .expect("parse diagnostics baseline");
    let closure = json.get("closure").expect("closure report");
    assert!(closure.pointer("/verdict/status").is_some());
    assert!(closure.pointer("/verdict/summary").is_some());
    assert!(
        closure
            .pointer("/frame/total_frame_median_fps")
            .and_then(|value| value.as_f64())
            .is_some_and(|value| value > 0.0)
    );
    let findings = closure
        .pointer("/verdict/findings")
        .and_then(|value| value.as_array());
    if let Some(findings) = findings {
        if findings.is_empty() {
            assert!(
                stdout.contains(
                    "no specific subsystem finding was inferred from the sampled reports"
                )
            );
        } else {
            for finding in findings {
                let focus = finding
                    .pointer("/focus")
                    .and_then(|value| value.as_str())
                    .expect("finding focus");
                let summary = finding
                    .pointer("/summary")
                    .and_then(|value| value.as_str())
                    .expect("finding summary");
                assert!(stdout.contains(&format!("focus={focus}")));
                assert!(stdout.contains(summary));
            }
        }
    } else {
        assert!(
            stdout.contains("no specific subsystem finding was inferred from the sampled reports")
        );
    }
    let presentation_reports = json
        .get("presentation_reports")
        .and_then(|value| value.as_array())
        .expect("presentation reports array");
    let first_report = presentation_reports
        .first()
        .expect("first presentation report");
    assert_eq!(
        first_report
            .pointer("/backend")
            .and_then(|value| value.as_str()),
        Some("wgsl")
    );
    assert!(
        first_report
            .pointer("/steady_state_fps")
            .and_then(|value| value.as_f64())
            .is_some_and(|value| value > 0.0)
    );
    assert!(
        first_report
            .get("wgsl_workgroup_comparison")
            .and_then(|value| value.as_object())
            .is_some()
    );
    assert!(
        first_report
            .get("ab_comparison")
            .and_then(|value| value.as_object())
            .is_some()
    );
}

#[test]
fn cli_perf_1080p120_closure_collection_failure_exits_nonzero_but_preserves_baseline() {
    let dir = workspace_tempdir();
    let bench_root = dir.path().join("whole_frame_fixture");
    write_whole_frame_closure_benchmark_project(&bench_root);
    let manifest_path = bench_root.join("1080p120_closure.toml");
    let broken_manifest = std::fs::read_to_string(&manifest_path)
        .expect("read closure manifest")
        .replace(
            "show_fixture_1080p120_closure_view",
            "show_missing_fixture_1080p120_closure_view",
        );
    write_fixture_file(&manifest_path, &broken_manifest).expect("rewrite broken closure manifest");
    let baseline = dir
        .path()
        .join("whole_frame_1080p120_unstable_collection.json");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("perf")
        .arg("--runs=1")
        .arg("--profile=1080p120")
        .arg("--query-backend=wgsl")
        .arg(format!("--baseline-out={}", baseline.display()))
        .arg(&bench_root)
        .output()
        .expect("run whole-frame closure perf with broken view");
    assert!(
        !output.status.success(),
        "expected whole-frame closure with broken view to fail: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        baseline.exists(),
        "expected closure baseline despite command failure"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("perf harness failed: unstable benchmark collection"));
    let json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&baseline).expect("read unstable baseline"))
            .expect("parse unstable baseline");
    let closure = json.get("closure").expect("closure report");
    assert_eq!(
        closure
            .pointer("/frame/status")
            .and_then(|value| value.as_str()),
        Some("violated")
    );
    let notes = closure
        .pointer("/frame/notes")
        .and_then(|value| value.as_array())
        .expect("frame notes");
    assert!(notes.iter().any(|note| {
        note.as_str().is_some_and(|text| {
            text.contains("presentation report collection failed")
                || text.contains("whole-frame report collection failed")
        })
    }));
    let presentation_reports = json
        .get("presentation_reports")
        .and_then(|value| value.as_array())
        .expect("presentation reports array");
    assert!(presentation_reports.is_empty());
    let whole_frame_reports = json
        .get("whole_frame_reports")
        .and_then(|value| value.as_array())
        .expect("whole-frame reports array");
    assert!(whole_frame_reports.is_empty());
}

#[test]
fn cli_perf_runs_field_engine_1080p120_closure_profile() {
    let dir = workspace_tempdir();
    let bench_root = dir.path().join("field_engine_fixture");
    write_field_engine_closure_benchmark_project(&bench_root);
    let baseline = dir.path().join("field_engine_1080p120.json");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("perf")
        .arg("--runs=1")
        .arg("--profile=1080p120")
        .arg("--query-backend=cpu")
        .arg(format!("--baseline-out={}", baseline.display()))
        .arg(&bench_root)
        .output()
        .expect("run field_engine closure perf");
    assert!(
        output.status.success(),
        "field engine closure perf failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&baseline).expect("read field_engine baseline"))
            .expect("parse field_engine baseline");
    let closure = json.get("closure").expect("closure report");
    assert_eq!(
        closure
            .pointer("/verdict/status")
            .and_then(|value| value.as_str()),
        Some("not_applicable")
    );
    assert_eq!(
        closure
            .pointer("/profile/name")
            .and_then(|value| value.as_str()),
        Some("canonical_1080p120_wgsl_resident")
    );
    assert_eq!(
        closure
            .pointer("/cpu_oracle_profile/name")
            .and_then(|value| value.as_str()),
        Some("canonical_1080p120_cpu_oracle")
    );
    assert_eq!(
        closure
            .pointer("/frame/status")
            .and_then(|value| value.as_str()),
        Some("not_sampled")
    );
    let collision_status = closure
        .pointer("/collision/status")
        .and_then(|value| value.as_str())
        .expect("collision status");
    assert!(
        matches!(collision_status, "not_sampled"),
        "unexpected collision status: {collision_status}"
    );
    assert!(
        json.get("presentation_reports").is_none(),
        "field engine closure profile should not emit presentation reports"
    );
    assert!(
        json.get("collision_reports").is_none(),
        "field engine closure profile should not emit collision reports"
    );
}

#[test]
fn cli_collision_perf_fixture_compiles_with_region_domain_contract() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf();
    let fixture = repo_root.join("benchmarks/collision_perf/tests/collision_perf_test.wr");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg(&fixture)
        .output()
        .expect("compile collision perf fixture");
    assert!(
        output.status.success(),
        "collision perf fixture failed to compile: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn cli_perf_runs_collision_perf_1080p120_closure_profile() {
    let dir = workspace_tempdir();
    write_collision_closure_benchmark_project(dir.path());
    let bench_root = dir.path();
    let baseline = dir.path().join("collision_perf_1080p120.json");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("perf")
        .arg("--runs=1")
        .arg("--profile=1080p120")
        .arg("--query-backend=wgsl")
        .arg(format!("--baseline-out={}", baseline.display()))
        .arg(&bench_root)
        .output()
        .expect("run collision perf closure");
    assert!(
        output.status.success(),
        "collision perf closure failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&baseline).expect("read collision baseline"))
            .expect("parse collision baseline");
    let closure = json.get("closure").expect("closure report");
    assert_eq!(
        closure
            .pointer("/profile/name")
            .and_then(|value| value.as_str()),
        Some("canonical_1080p120_wgsl_resident")
    );
    assert_eq!(
        closure
            .pointer("/cpu_oracle_profile/name")
            .and_then(|value| value.as_str()),
        Some("canonical_1080p120_cpu_oracle")
    );
    assert_eq!(
        closure
            .pointer("/frame/status")
            .and_then(|value| value.as_str()),
        Some("not_sampled")
    );
    assert_eq!(
        closure
            .pointer("/collision/status")
            .and_then(|value| value.as_str()),
        Some("not_sampled")
    );
    assert_eq!(
        closure
            .pointer("/verdict/status")
            .and_then(|value| value.as_str()),
        Some("not_applicable")
    );
    let collision_reports = json
        .get("collision_reports")
        .and_then(|value| value.as_array())
        .expect("collision reports array");
    assert!(!collision_reports.is_empty());
    let report = &collision_reports[0];
    assert_eq!(
        report.pointer("/suite").and_then(|value| value.as_str()),
        Some("collision_perf")
    );
    assert_eq!(
        report.pointer("/command").and_then(|value| value.as_str()),
        Some("collision-suite")
    );
    assert_eq!(
        report
            .pointer("/query_count_total")
            .and_then(|value| value.as_u64()),
        Some(64)
    );
    assert!(
        report
            .pointer("/queries_per_sec")
            .and_then(|value| value.as_f64())
            .is_some()
    );
    assert!(
        report
            .pointer("/average_rejected_candidate_count")
            .and_then(|value| value.as_f64())
            .is_some_and(|value| value > 0.0)
    );
    assert!(
        report
            .pointer("/average_pruned_node_count")
            .and_then(|value| value.as_f64())
            .is_some_and(|value| value > 0.0)
    );
    assert!(
        report
            .pointer("/witness_reuse_rate")
            .and_then(|value| value.as_f64())
            .is_some()
    );
    assert!(
        report
            .pointer("/fallback_rate")
            .and_then(|value| value.as_f64())
            .is_some()
    );
    assert!(
        report
            .pointer("/executions/0/broadphase_rejected_candidate_count")
            .and_then(|value| value.as_u64())
            .is_some()
    );
    assert!(
        report
            .pointer("/executions/0/broadphase_pruned_node_count")
            .and_then(|value| value.as_u64())
            .is_some()
    );
    let executions = report
        .get("executions")
        .and_then(|value| value.as_array())
        .expect("collision benchmark executions");
    assert_eq!(executions.len(), 1);
    let point_burst = executions.first().expect("point occupancy execution");
    assert_eq!(
        point_burst
            .pointer("/name")
            .and_then(|value| value.as_str()),
        Some("closure_1080p120_point_occupancy_burst")
    );
    assert_eq!(
        point_burst
            .pointer("/plan_name")
            .and_then(|value| value.as_str()),
        Some("collision.point_occupancy.world")
    );
    assert_eq!(
        point_burst
            .pointer("/query_count")
            .and_then(|value| value.as_u64()),
        Some(64)
    );
    assert!(
        point_burst
            .pointer("/wgsl_dispatch_count")
            .and_then(|value| value.as_u64())
            .is_some_and(|value| value > 0)
    );
    assert!(
        point_burst
            .pointer("/wgsl_selected_workgroup_size")
            .and_then(|value| value.as_u64())
            .is_some_and(|value| value > 0)
    );
    assert!(json.get("presentation_reports").is_none());
    assert!(json.get("whole_frame_reports").is_none());
}

#[test]
fn cli_perf_collision_closure_marks_cpu_backend_mismatch_as_violated() {
    let dir = workspace_tempdir();
    let bench_root = dir.path().join("whole_frame_fixture");
    write_whole_frame_closure_benchmark_project(&bench_root);
    let baseline = dir.path().join("whole_frame_1080p120_cpu_mismatch.json");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(dir.path())
        .arg("perf")
        .arg("--runs=1")
        .arg("--profile=1080p120")
        .arg("--query-backend=cpu")
        .arg(format!("--baseline-out={}", baseline.display()))
        .arg(&bench_root)
        .output()
        .expect("run whole-frame closure with cpu backend");
    assert!(
        output.status.success(),
        "whole-frame closure cpu mismatch failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&baseline).expect("read whole-frame baseline"))
            .expect("parse whole-frame baseline");
    let closure = json.get("closure").expect("closure report");
    assert_eq!(
        closure
            .pointer("/collision/status")
            .and_then(|value| value.as_str()),
        Some("violated")
    );
    let notes = closure
        .pointer("/collision/notes")
        .and_then(|value| value.as_array())
        .expect("collision notes");
    assert!(notes.iter().any(|note| {
        note.as_str()
            .is_some_and(|text| text.contains("collision backends observed: cpu"))
    }));
    assert!(notes.iter().any(|note| {
        note.as_str().is_some_and(|text| {
            text.contains("collision report backend 'cpu'")
                && text.contains("closure backend 'wgsl'")
        })
    }));
}

#[test]
fn cli_perf_runs_field_engine_regression_smoke_on_cpu() {
    let dir = workspace_tempdir();
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf();
    let bench_root = repo_root.join("benchmarks/field_engine");
    let manifest = dir.path().join("field_engine_cpu_smoke.toml");
    let baseline = dir.path().join("field_engine_cpu_smoke.json");
    write_fixture_file(
        &manifest,
        r#"
version = 1
suite = "field_engine_cpu_smoke"

[profiles.smoke]
warmup_pairs = 1
measure_pairs = 1
coverage = "all"

[[scenarios]]
id = "hard_repetition_identity_stability"
test_name = "tests/field_engine::test_field_repetition_identity_stability_ops_120000"
ops = 120000
class = "critical"
min_runtime_ms = 1
timeout_ms = 120000
allow_unstable = false

[[scenarios]]
id = "opaque_leaf_pessimization"
test_name = "tests/field_engine::test_field_opaque_leaf_pessimization_ops_4000"
ops = 4000
class = "critical"
min_runtime_ms = 1
timeout_ms = 120000
allow_unstable = false

[[scenarios]]
id = "region_domain_media_radiance"
test_name = "tests/field_engine::test_field_region_domain_media_radiance_ops_60000"
ops = 60000
class = "critical"
min_runtime_ms = 1
timeout_ms = 120000
allow_unstable = false
"#,
    )
    .expect("write field-engine cpu smoke manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .current_dir(&repo_root)
        .arg("perf")
        .arg("--runs=1")
        .arg("--profile=smoke")
        .arg("--query-backend=cpu")
        .arg(format!("--benchmark-manifest={}", manifest.display()))
        .arg(format!("--baseline-out={}", baseline.display()))
        .arg(&bench_root)
        .output()
        .expect("run field-engine cpu perf smoke");
    assert!(
        output.status.success(),
        "field-engine cpu perf smoke failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(baseline.exists(), "expected field-engine cpu perf baseline");
    let json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&baseline).expect("read field-engine cpu baseline"))
            .expect("parse field-engine cpu baseline");
    let cases = json
        .get("summary")
        .and_then(|value| value.get("cases"))
        .and_then(|value| value.as_array())
        .expect("summary.cases array");
    assert_eq!(cases.len(), 3);
    let case_names: std::collections::BTreeSet<_> = cases
        .iter()
        .filter_map(|case| case.get("name").and_then(|value| value.as_str()))
        .collect();
    assert!(
        case_names
            .contains("tests/field_engine::test_field_repetition_identity_stability_ops_120000")
    );
    assert!(
        case_names.contains("tests/field_engine::test_field_opaque_leaf_pessimization_ops_4000")
    );
    assert!(
        case_names
            .contains("tests/field_engine::test_field_region_domain_media_radiance_ops_60000")
    );
    let metrics = json
        .get("summary")
        .and_then(|value| value.get("metrics"))
        .expect("summary.metrics");
    assert!(
        metrics
            .get("scene_trace")
            .and_then(|value| value.as_u64())
            .unwrap_or(0)
            > 0
    );
    assert!(
        metrics
            .get("scene_trace_candidate_branch")
            .and_then(|value| value.as_u64())
            .unwrap_or(0)
            > 0
    );
    assert!(
        metrics
            .get("scene_trace_hit_count")
            .and_then(|value| value.as_u64())
            .unwrap_or(0)
            > 0
    );
}

#[test]
fn cli_perf_gate_fails_with_synthetic_slowdown() {
    let dir = workspace_tempdir();
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
        .env("WRELA_TEST_SLOWDOWN_MS", "6000")
        .arg("test")
        .arg(format!("--perf-gate={}", baseline.display()))
        .arg("--perf-max-regression-pct=0")
        .arg("--test-timeout-ms=20000")
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
