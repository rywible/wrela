//! Owns perf-engine regression tests for collection, closure, perfcmp, and
//! matrix/reporting helpers.
//! Does not own the perf engine implementation itself.
//!
//! Key invariants:
//! - fixtures in this module should encode user-visible/reporting semantics, not
//!   just internal struct shape.
//! - scenario and lane identity used in assertions must match the typed runtime
//!   models the production code now uses.
//!
//! Primary entrypoints:
//! - perf-engine regression tests in this module
//!
//! Failure modes / common pitfalls:
//! - letting fixtures drift back to ad hoc strings hides the very protocol
//!   regressions Phase 54 is trying to close.

use super::collection::{PresentationBenchmarkCollectionMode, PresentationDebugCommandOutput};
use super::*;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

#[test]
fn sanitize_git_ref_for_filename_replaces_unsafe_chars() {
    assert_eq!(
        sanitize_git_ref_for_filename("feature/perf+gate@2026"),
        "feature_perf_gate_2026".to_string()
    );
    assert_eq!(sanitize_git_ref_for_filename(""), "unknown".to_string());
}

#[test]
fn collision_benchmark_warmup_run_count_only_warms_wgsl_reports() {
    assert_eq!(
        super::collection::collision_benchmark_warmup_run_count(
            wrela::query_plan::DispatchBackend::Wgsl
        ),
        1
    );
    assert_eq!(
        super::collection::collision_benchmark_warmup_run_count(
            wrela::query_plan::DispatchBackend::Cpu
        ),
        0
    );
}

#[test]
fn classify_perfcmp_verdict_respects_effect_threshold() {
    assert_eq!(classify_perfcmp_verdict(3.5, 8.0, 2.0), PerfCmpVerdict::Win);
    assert_eq!(
        classify_perfcmp_verdict(-8.0, -3.1, 2.0),
        PerfCmpVerdict::Regression
    );
    assert_eq!(
        classify_perfcmp_verdict(-1.0, 1.2, 2.0),
        PerfCmpVerdict::NoSignal
    );
}

#[test]
fn fnv1a64_is_deterministic() {
    let first = fnv1a64(b"wrela-perfcmp");
    let second = fnv1a64(b"wrela-perfcmp");
    let different = fnv1a64(b"wrela-perfcmp-2");
    assert_eq!(first, second);
    assert_ne!(first, different);
}

#[test]
fn coefficient_of_variation_handles_small_and_stable_sets() {
    assert_eq!(coefficient_of_variation(&[]), 0.0);
    assert_eq!(coefficient_of_variation(&[42.0]), 0.0);
    let cv = coefficient_of_variation(&[100.0, 100.0, 100.0, 100.0]);
    assert!(cv <= f64::EPSILON, "expected near-zero cv, got {cv}");
}

#[test]
fn bootstrap_ci_percentile_is_seed_deterministic() {
    let values = [1.0, 2.0, 3.0, 4.0, 5.0];
    let mut seed_a = 7u64;
    let mut seed_b = 7u64;
    let ci_a = bootstrap_ci_percentile(&values, 95.0, 128, &mut seed_a);
    let ci_b = bootstrap_ci_percentile(&values, 95.0, 128, &mut seed_b);
    assert_eq!(ci_a, ci_b);
}

#[cfg(unix)]
#[test]
fn run_command_with_timeout_aborts_long_running_process() {
    let mut command = Command::new("sh");
    command.arg("-c").arg("sleep 5 & wait");
    let started = Instant::now();
    let err = run_command_with_timeout(&mut command, Duration::from_millis(100))
        .expect_err("long-running command should time out");
    assert!(err.contains("timed out"), "{err}");
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "timeout returned too late: {:?}",
        started.elapsed()
    );
}

#[cfg(unix)]
#[test]
fn run_command_with_timeout_drains_large_stdout_while_child_runs() {
    let mut command = Command::new("sh");
    command
        .arg("-c")
        .arg("dd if=/dev/zero bs=65536 count=32 status=none");
    let output = run_command_with_timeout(&mut command, Duration::from_secs(2))
        .expect("large stdout should not deadlock command collection");
    assert!(output.status.success());
    assert_eq!(output.stdout.len(), 65_536 * 32);
    assert!(output.stderr.is_empty());
}

#[test]
fn real_realtime_presentation_closure_manifest_matches_expected_protocol() {
    let bench_root = workspace_root()
        .join("benchmarks")
        .join("realtime_presentation");
    let manifest_path = bench_root.join("1080p120_closure.toml");
    let raw_manifest = fs::read_to_string(&manifest_path).expect("read closure manifest");
    let manifest_toml: toml::Value = toml::from_str(&raw_manifest).expect("parse closure toml");
    let manifest = load_benchmark_manifest(&manifest_path).expect("load closure manifest");
    assert_eq!(manifest.suite, "realtime_presentation");
    assert_eq!(
        manifest_toml
            .get("profiles")
            .and_then(|value| value.get("closure_1080p120"))
            .and_then(|value| value.get("warmup_pairs"))
            .and_then(|value| value.as_integer()),
        Some(4)
    );
    assert_eq!(
        manifest_toml
            .get("profiles")
            .and_then(|value| value.get("closure_1080p120"))
            .and_then(|value| value.get("measure_pairs"))
            .and_then(|value| value.as_integer()),
        Some(12)
    );
    assert_eq!(
        manifest_toml
            .get("profiles")
            .and_then(|value| value.get("closure_1080p120"))
            .and_then(|value| value.get("coverage"))
            .and_then(|value| value.as_str()),
        Some("all")
    );
    assert_eq!(
        manifest_toml
            .get("profiles")
            .and_then(|value| value.get("closure_1080p120"))
            .and_then(|value| value.get("execution_story"))
            .and_then(|value| value.as_str()),
        Some("wgsl_resident")
    );
    assert_eq!(
        manifest_toml
            .get("profiles")
            .and_then(|value| value.get("closure_1080p120"))
            .and_then(|value| value.get("adapter_name"))
            .and_then(|value| value.as_str()),
        Some("wgsl_resident")
    );
    assert_eq!(
        manifest_toml
            .get("profiles")
            .and_then(|value| value.get("closure_1080p120"))
            .and_then(|value| value.get("warmup_protocol"))
            .and_then(|value| value.as_str()),
        Some("pipeline_and_resident_scene_upload")
    );
    assert_eq!(
        manifest_toml
            .get("profiles")
            .and_then(|value| value.get("closure_1080p120"))
            .and_then(|value| value.get("companion_profile"))
            .and_then(|value| value.as_str()),
        Some("canonical_1080p120_cpu_oracle")
    );

    let scenarios = manifest.scenarios_for_profile(PerfProfile::Closure1080p120);
    let scenario_ids = scenarios
        .iter()
        .map(|scenario| scenario.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        scenario_ids,
        vec![
            "closure_1080p120_dense_constructive",
            "closure_1080p120_repetition_heavy",
            "closure_1080p120_thin_stack_grazing",
            "closure_1080p120_media_radiance",
            "closure_1080p120_transformed_primitive_gallery",
            "closure_1080p120_mixed_opaque_conservative",
            "closure_1080p120_cache_stress_motion_path",
            "closure_1080p120_camera_motion_temporal_reuse_clipmap_churn",
        ]
    );
    assert!(
        scenarios
            .iter()
            .all(|scenario| { scenario.class == test_eval_perf::BenchmarkScenarioClass::Closure })
    );
    assert!(
        scenarios
            .iter()
            .all(|scenario| scenario.presentation.is_some())
    );
    assert!(scenarios.iter().all(|scenario| {
        let presentation = scenario.presentation.as_ref().expect("presentation spec");
        presentation.width == Some(1920)
            && presentation.height == Some(1080)
            && presentation.frames == Some(7)
    }));

    let selection = build_benchmark_selection(
        &TestTarget::ProjectRoot(bench_root),
        &manifest_path,
        PerfProfile::Closure1080p120,
    )
    .expect("build closure benchmark selection");
    assert_eq!(selection.len(), scenario_ids.len());
}

#[test]
fn real_field_engine_closure_manifest_matches_expected_protocol() {
    let bench_root = workspace_root().join("benchmarks").join("field_engine");
    let manifest_path = bench_root.join("1080p120_closure.toml");
    let raw_manifest = fs::read_to_string(&manifest_path).expect("read closure manifest");
    let manifest_toml: toml::Value = toml::from_str(&raw_manifest).expect("parse closure toml");
    let manifest = load_benchmark_manifest(&manifest_path).expect("load closure manifest");
    assert_eq!(manifest.suite, "field_engine");
    assert_eq!(
        manifest_toml
            .get("profiles")
            .and_then(|value| value.get("closure_1080p120"))
            .and_then(|value| value.get("warmup_pairs"))
            .and_then(|value| value.as_integer()),
        Some(4)
    );
    assert_eq!(
        manifest_toml
            .get("profiles")
            .and_then(|value| value.get("closure_1080p120"))
            .and_then(|value| value.get("measure_pairs"))
            .and_then(|value| value.as_integer()),
        Some(12)
    );
    assert_eq!(
        manifest_toml
            .get("profiles")
            .and_then(|value| value.get("closure_1080p120"))
            .and_then(|value| value.get("coverage"))
            .and_then(|value| value.as_str()),
        Some("all")
    );
    assert_eq!(
        manifest_toml
            .get("profiles")
            .and_then(|value| value.get("closure_1080p120"))
            .and_then(|value| value.get("execution_story"))
            .and_then(|value| value.as_str()),
        Some("cpu_oracle")
    );
    assert_eq!(
        manifest_toml
            .get("profiles")
            .and_then(|value| value.get("closure_1080p120"))
            .and_then(|value| value.get("adapter_name"))
            .and_then(|value| value.as_str()),
        Some("cpu_oracle")
    );
    assert_eq!(
        manifest_toml
            .get("profiles")
            .and_then(|value| value.get("closure_1080p120"))
            .and_then(|value| value.get("warmup_protocol"))
            .and_then(|value| value.as_str()),
        Some("cpu_oracle_baseline_warmup")
    );
    assert_eq!(
        manifest_toml
            .get("profiles")
            .and_then(|value| value.get("closure_1080p120"))
            .and_then(|value| value.get("companion_profile"))
            .and_then(|value| value.as_str()),
        Some("canonical_1080p120_wgsl_resident")
    );

    let scenarios = manifest.scenarios_for_profile(PerfProfile::Closure1080p120);
    let scenario_ids = scenarios
        .iter()
        .map(|scenario| scenario.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        scenario_ids,
        vec![
            "closure_1080p120_repetition_identity_stability",
            "closure_1080p120_mixed_solver_dense_oracle",
            "closure_1080p120_collision_heavy_transition",
            "closure_1080p120_transformed_primitive_gallery",
            "closure_1080p120_mixed_opaque_conservative",
        ]
    );
    assert!(
        scenarios
            .iter()
            .all(|scenario| { scenario.class == test_eval_perf::BenchmarkScenarioClass::Closure })
    );
    assert!(
        scenarios
            .iter()
            .all(|scenario| scenario.presentation.is_none())
    );

    let selection = build_benchmark_selection(
        &TestTarget::ProjectRoot(bench_root),
        &manifest_path,
        PerfProfile::Closure1080p120,
    )
    .expect("build closure benchmark selection");
    assert_eq!(selection.len(), scenario_ids.len());
}

#[test]
fn real_collision_perf_manifest_matches_expected_protocol() {
    let bench_root = workspace_root().join("benchmarks").join("collision_perf");
    let manifest_path = bench_root.join("bench.toml");
    let raw_manifest = fs::read_to_string(&manifest_path).expect("read collision manifest");
    let manifest_toml: toml::Value = toml::from_str(&raw_manifest).expect("parse collision toml");
    let manifest = load_benchmark_manifest(&manifest_path).expect("load collision manifest");
    assert_eq!(manifest.suite, "collision_perf");
    assert_eq!(
        manifest_toml
            .get("profiles")
            .and_then(|value| value.get("standard"))
            .and_then(|value| value.get("warmup_pairs"))
            .and_then(|value| value.as_integer()),
        Some(3)
    );
    assert_eq!(
        manifest_toml
            .get("profiles")
            .and_then(|value| value.get("standard"))
            .and_then(|value| value.get("measure_pairs"))
            .and_then(|value| value.as_integer()),
        Some(10)
    );
    let scenarios = manifest.scenarios_for_profile(PerfProfile::Standard);
    let scenario_ids = scenarios
        .iter()
        .map(|scenario| scenario.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        scenario_ids,
        vec![
            "collision_perf_point_occupancy_burst",
            "collision_perf_dense_ray_casts",
            "collision_perf_overlap_burst",
            "collision_perf_repeated_sweeps",
            "collision_perf_toi_transition_reuse",
        ]
    );
    assert!(
        scenarios
            .iter()
            .all(|scenario| scenario.class == test_eval_perf::BenchmarkScenarioClass::Critical)
    );
    let selection = build_benchmark_selection(
        &TestTarget::ProjectRoot(bench_root),
        &manifest_path,
        PerfProfile::Standard,
    )
    .expect("build collision benchmark selection");
    assert_eq!(selection.len(), scenario_ids.len());
}

#[test]
fn real_collision_perf_closure_manifest_matches_expected_protocol() {
    let bench_root = workspace_root().join("benchmarks").join("collision_perf");
    let manifest_path = bench_root.join("1080p120_closure.toml");
    let raw_manifest = fs::read_to_string(&manifest_path).expect("read closure manifest");
    let manifest_toml: toml::Value = toml::from_str(&raw_manifest).expect("parse closure toml");
    let manifest = load_benchmark_manifest(&manifest_path).expect("load closure manifest");
    assert_eq!(manifest.suite, "collision_perf");
    assert_eq!(
        manifest_toml
            .get("profiles")
            .and_then(|value| value.get("closure_1080p120"))
            .and_then(|value| value.get("warmup_pairs"))
            .and_then(|value| value.as_integer()),
        Some(4)
    );
    assert_eq!(
        manifest_toml
            .get("profiles")
            .and_then(|value| value.get("closure_1080p120"))
            .and_then(|value| value.get("measure_pairs"))
            .and_then(|value| value.as_integer()),
        Some(12)
    );
    let scenarios = manifest.scenarios_for_profile(PerfProfile::Closure1080p120);
    let scenario_ids = scenarios
        .iter()
        .map(|scenario| scenario.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        scenario_ids,
        vec![
            "closure_1080p120_point_occupancy_burst",
            "closure_1080p120_dense_ray_casts",
            "closure_1080p120_overlap_burst",
            "closure_1080p120_repeated_sweeps",
            "closure_1080p120_toi_transition_reuse",
        ]
    );
    assert!(
        scenarios
            .iter()
            .all(|scenario| { scenario.class == test_eval_perf::BenchmarkScenarioClass::Closure })
    );
    let selection = build_benchmark_selection(
        &TestTarget::ProjectRoot(bench_root),
        &manifest_path,
        PerfProfile::Closure1080p120,
    )
    .expect("build closure benchmark selection");
    assert_eq!(selection.len(), scenario_ids.len());
}

#[test]
fn real_engine_frame_closure_manifest_matches_expected_protocol() {
    let bench_root = workspace_root().join("benchmarks").join("engine_frame");
    let manifest_path = bench_root.join("1080p120_closure.toml");
    let raw_manifest = fs::read_to_string(&manifest_path).expect("read closure manifest");
    let manifest_toml: toml::Value = toml::from_str(&raw_manifest).expect("parse closure toml");
    let manifest = load_benchmark_manifest(&manifest_path).expect("load closure manifest");
    assert_eq!(manifest.suite, "engine_frame");
    assert_eq!(
        manifest_toml
            .get("profiles")
            .and_then(|value| value.get("closure_1080p120"))
            .and_then(|value| value.get("warmup_pairs"))
            .and_then(|value| value.as_integer()),
        Some(4)
    );
    assert_eq!(
        manifest_toml
            .get("profiles")
            .and_then(|value| value.get("closure_1080p120"))
            .and_then(|value| value.get("measure_pairs"))
            .and_then(|value| value.as_integer()),
        Some(12)
    );
    let scenarios = manifest.scenarios_for_profile(PerfProfile::Closure1080p120);
    let scenario_ids = scenarios
        .iter()
        .map(|scenario| scenario.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        scenario_ids,
        vec![
            "closure_1080p120_dense_constructive_dense_ray_casts",
            "closure_1080p120_repetition_heavy_repeated_sweeps",
            "closure_1080p120_thin_stack_point_occupancy",
            "closure_1080p120_media_radiance_overlap_burst",
            "closure_1080p120_camera_motion_toi_reuse",
        ]
    );
    assert!(
        scenarios
            .iter()
            .all(|scenario| { scenario.class == test_eval_perf::BenchmarkScenarioClass::Closure })
    );
    assert!(
        scenarios
            .iter()
            .all(|scenario| scenario.presentation.is_some())
    );
    assert!(
        scenarios
            .iter()
            .all(|scenario| scenario.collision.is_some())
    );
    assert!(scenarios.iter().all(|scenario| {
        let presentation = scenario.presentation.as_ref().expect("presentation spec");
        let collision = scenario.collision.as_ref().expect("collision spec");
        presentation.width == Some(1920)
            && presentation.height == Some(1080)
            && presentation.frames == Some(7)
            && presentation.entry.as_deref() == Some("tests/whole_frame_test.wr")
            && collision.entry.as_deref() == Some("tests/whole_frame_test.wr")
    }));

    let selection = build_benchmark_selection(
        &TestTarget::ProjectRoot(bench_root),
        &manifest_path,
        PerfProfile::Closure1080p120,
    )
    .expect("build closure benchmark selection");
    assert_eq!(selection.len(), scenario_ids.len());
}

#[test]
fn execute_perf_command_reuses_the_loaded_manifest_for_selection() {
    let bench_root = workspace_root().join("benchmarks").join("micro");
    let temp = tempfile::tempdir().expect("tempdir");
    let manifest_path = temp.path().join("micro_smoke.toml");
    let baseline_path = temp.path().join("micro_smoke.json");
    fs::write(
        &manifest_path,
        r#"
version = 1
suite = "micro_perf_engine_test"

[profiles.smoke]
warmup_pairs = 1
measure_pairs = 1
coverage = "critical"

[[scenarios]]
id = "check_given_boolean_lane"
test_name = "tests/micro::test_check_given_boolean_lane_ops_12000000"
ops = 12000000
class = "critical"
min_runtime_ms = 1
timeout_ms = 120000
allow_unstable = false
"#,
    )
    .expect("write micro perf manifest");

    build_compile::reset_benchmark_manifest_load_count();
    let exit = execute_perf_command(PerfCommandInput {
        trace: false,
        program_args: Vec::new(),
        path_arg: Some(bench_root.display().to_string()),
        perf_runs: Some(1),
        test_jobs: None,
        test_timeout_ms: None,
        benchmark_manifest_path: Some(manifest_path.display().to_string()),
        perf_profile: PerfProfile::Smoke,
        perf_baseline_out: Some(baseline_path.display().to_string()),
        perf_gate_path: None,
        perf_max_regression_pct: None,
        perf_cv_max_pct: None,
        perf_why_not_120: false,
        kpi_thresholds: KpiThresholds::default(),
        output_format: OutputFormat::Json,
        perf_debug: false,
        test_selection: TestSelection::default(),
        query_backend: wrela::query_plan::DispatchBackend::Auto,
    });
    assert_eq!(exit, EXIT_OK);
    assert!(
        baseline_path.exists(),
        "expected perf baseline at {}",
        baseline_path.display()
    );
    assert_eq!(build_compile::benchmark_manifest_load_count(), 1);
}

#[test]
fn frame_closure_status_records_report_collection_failures_as_violations() {
    let profile = PerfClosureProfile::canonical_1080p120();
    let report = build_frame_closure_status(
        &profile,
        None,
        &[],
        &["scenario `dense` timed out".to_string()],
        &[],
        &[],
        &[],
        &[],
        0,
        1,
    );
    assert_eq!(report.status, PerfClosureLaneStatus::Violated);
    assert!(
        report
            .notes
            .iter()
            .any(|note| note.contains("presentation report collection failed"))
    );
}

#[test]
fn frame_closure_status_rejects_backend_mismatch_for_wgsl_resident_profile() {
    let mut profile = PerfClosureProfile::canonical_1080p120();
    profile.output_width = 64;
    profile.output_height = 64;
    profile.warmup_runs = 0;
    profile.measured_runs = 1;
    profile.frame_budget.median_ms = 100.0;
    profile.frame_budget.p95_ms = 100.0;
    profile.primary_visibility_budget.median_ms = 100.0;
    profile.primary_visibility_budget.p95_ms = 100.0;

    let report = build_frame_closure_status(
        &profile,
        None,
        &[PresentationBenchmarkReport {
            scenario_id: "closure_fixture".into(),
            test_name: "tests/fixture".to_string(),
            view: "view".to_string(),
            region: "region".to_string(),
            domain: "domain".to_string(),
            backend: "cpu".to_string(),
            observed_adapter_name: None,
            query_trace_solver_mode: "hybrid".to_string(),
            selected_workgroup_size: 0,
            frames_executed: 1,
            frame_time_ns: 1_000_000,
            steady_state_fps: 1000.0,
            field_samples: 128,
            quality_tier: "realtime_120".to_string(),
            target_fps: 120,
            internal_resolution_scale: 1.0,
            reconstructed_output: false,
            quality_history: vec!["realtime_120".to_string()],
            internal_resolution_history: vec![1.0],
            bottleneck_pass: Some("primary_visibility".to_string()),
            active_acceleration_artifacts: vec![],
            performance_gain_sources: vec!["backend_speed".to_string()],
            frame_cost: {
                let mut frame_cost =
                    sample_presentation_frame_cost(64, 64, 1.0, 128, 2.0, 16, 8, 1_000);
                frame_cost.quality.tier = "realtime_120".to_string();
                frame_cost.quality.target_fps = 120;
                frame_cost
            },
            frame_cost_history: vec![],
            wgsl_workgroup_comparison: None,
            ab_comparison: None,
        }],
        &[],
        &[],
        &[],
        &[],
        &[],
        0,
        1,
    );

    assert_eq!(report.status, PerfClosureLaneStatus::Violated);
    assert!(
        report
            .notes
            .iter()
            .any(|note| note.contains("presentation backends observed: cpu"))
    );
    assert!(report.notes.iter().any(|note| {
        note.contains("reported backend 'cpu'") && note.contains("closure backend 'wgsl'")
    }));
}

#[test]
fn frame_closure_status_applies_execution_model_hard_gates_and_records_observations() {
    let mut profile = PerfClosureProfile::canonical_1080p120();
    profile.output_width = 64;
    profile.output_height = 64;
    profile.warmup_runs = 0;
    profile.measured_runs = 1;
    profile.frame_budget.median_ms = 100.0;
    profile.frame_budget.p95_ms = 100.0;
    profile.primary_visibility_budget.median_ms = 100.0;
    profile.primary_visibility_budget.p95_ms = 100.0;

    let mut frame_cost = sample_presentation_frame_cost(64, 64, 1.0, 128, 2.0, 16, 8, 1_000);
    frame_cost.quality.tier = "realtime_120".to_string();
    frame_cost.quality.target_fps = 120;
    frame_cost.gpu_runtime.timestamps_supported = true;
    frame_cost.gpu_runtime.timestamped_pass_count = 1;
    frame_cost.gpu_runtime.readback_bytes = 32;
    frame_cost.gpu_runtime.scene_reupload_bytes = 1;
    frame_cost.gpu_runtime.cpu_screen_sample_allocations = 1;
    frame_cost.gpu_runtime.attachment_decode_count = 2;
    frame_cost.gpu_runtime.attachment_encode_count = 1;
    frame_cost.gpu_runtime.queue_submit_count = 65;
    frame_cost
        .gpu_runtime
        .primary_visibility_packet_fanout_count = 4097;

    let presentation_report = PresentationBenchmarkReport {
        scenario_id: "closure_fixture".into(),
        test_name: "tests/fixture".to_string(),
        view: "view".to_string(),
        region: "region".to_string(),
        domain: "domain".to_string(),
        backend: "wgsl".to_string(),
        observed_adapter_name: Some("Test Adapter".to_string()),
        query_trace_solver_mode: "hybrid".to_string(),
        selected_workgroup_size: 64,
        frames_executed: 1,
        frame_time_ns: 1_000_000,
        steady_state_fps: 1000.0,
        field_samples: 128,
        quality_tier: "realtime_120".to_string(),
        target_fps: 120,
        internal_resolution_scale: 1.0,
        reconstructed_output: false,
        quality_history: vec!["realtime_120".to_string()],
        internal_resolution_history: vec![1.0],
        bottleneck_pass: Some("primary_visibility".to_string()),
        active_acceleration_artifacts: vec![],
        performance_gain_sources: vec![],
        frame_cost: frame_cost.clone(),
        frame_cost_history: vec![],
        wgsl_workgroup_comparison: None,
        ab_comparison: None,
    };

    let report = build_frame_closure_status(
        &profile,
        None,
        std::slice::from_ref(&presentation_report),
        &[],
        &[],
        &[],
        &[],
        &[],
        0,
        1,
    );

    assert_eq!(report.status, PerfClosureLaneStatus::Violated);
    assert_eq!(report.total_frame_median_fps, Some(1000.0));
    assert_eq!(report.hot_path_readback_bytes, Some(16));
    assert_eq!(report.scene_reupload_bytes, Some(1));
    assert_eq!(report.cpu_screen_sample_allocations, Some(1));
    assert_eq!(report.attachment_cpu_bounce_count, Some(3));
    assert_eq!(report.queue_submit_count, Some(65));
    assert_eq!(report.primary_visibility_dispatch_count, Some(4097));
    assert_eq!(report.timestamps_supported, Some(true));
    assert_eq!(report.timestamped_pass_count, Some(1));
    assert!(
        report
            .notes
            .iter()
            .any(|note| note.contains("execution model observations"))
    );

    let collision_status = PerfClosureLaneStatusReport::unsampled(&profile.collision);
    let verdict = build_closure_verdict(
        &profile,
        &report,
        &collision_status,
        &wrela::perf_target::PerfClosureEngineFrameStatusReport::unsampled(),
        std::slice::from_ref(&presentation_report),
        &[],
    );
    let focuses = verdict
        .findings
        .iter()
        .map(|finding| finding.focus.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    assert!(focuses.contains("hot_path_readback_gate"));
    assert!(focuses.contains("scene_reupload_gate"));
    assert!(focuses.contains("cpu_screen_sample_allocation_gate"));
    assert!(focuses.contains("attachment_cpu_bounce_gate"));
    assert!(focuses.contains("queue_submit_gate"));
    assert!(focuses.contains("primary_visibility_dispatch_gate"));
}

#[test]
fn whole_frame_benchmark_reports_join_presentation_and_collision_by_scenario_id() {
    let presentation_reports = vec![PresentationBenchmarkReport {
        scenario_id: "closure_1080p120_dense_constructive_dense_ray_casts".into(),
        test_name:
            "tests/whole_frame::test_whole_frame_dense_constructive_dense_ray_casts_ops_7200"
                .to_string(),
        view: "show_dense_constructive_1080p120_closure_view".to_string(),
        region: "dense_constructive_region".to_string(),
        domain: "dense_constructive_domain".to_string(),
        backend: "wgsl".to_string(),
        observed_adapter_name: None,
        query_trace_solver_mode: "hybrid".to_string(),
        selected_workgroup_size: 64,
        frames_executed: 1,
        frame_time_ns: 6_000_000,
        steady_state_fps: fps_from_frame_time_ns(6_000_000, 1),
        field_samples: 128,
        quality_tier: "realtime_120".to_string(),
        target_fps: 120,
        internal_resolution_scale: 1.0,
        reconstructed_output: false,
        quality_history: vec!["realtime_120".to_string()],
        internal_resolution_history: vec![1.0],
        bottleneck_pass: Some("surface_resolve".to_string()),
        active_acceleration_artifacts: vec!["view_tile_culling".to_string()],
        performance_gain_sources: vec!["tile_culling".to_string()],
        frame_cost: sample_presentation_frame_cost(64, 64, 1.0, 128, 2.0, 16, 8, 6_000),
        frame_cost_history: vec![],
        wgsl_workgroup_comparison: None,
        ab_comparison: None,
    }];
    let collision_reports = vec![CollisionBenchmarkReport {
        suite: "whole_frame".to_string(),
        backend: "wgsl".to_string(),
        command: "collision-suite".to_string(),
        query_count_total: 7_200,
        total_runtime_ns: 2_500_000,
        queries_per_sec: 2_880_000.0,
        average_candidate_count: 5.0,
        average_rejected_candidate_count: 2.0,
        average_pruned_node_count: 1.0,
        average_interval_subdivisions: 0.5,
        average_interval_refinements: 0.2,
        average_certificate_successes: 0.0,
        witness_reuse_rate: 0.75,
        fallback_rate: 0.10,
        available_count_total: 10,
        consumed_count_total: 7,
        rejected_count_total: 2,
        unavailable_count_total: 1,
        executions: vec![test_eval_perf::CollisionBenchmarkExecutionReport {
            name: "closure_1080p120_dense_constructive_dense_ray_casts".into(),
            plan_name: "dense_ray_casts".to_string(),
            contract_id: "whole_frame_collision".to_string(),
            query_count: 7_200,
            batch_count: 1,
            dispatch_count: 4,
            dispatch_items: 256,
            average_items_per_dispatch: 64.0,
            runtime_ns: 2_500_000,
            timestamps_supported: false,
            timestamped_pass_count: 0,
            gpu_time_total_ns: 0,
            gpu_time_max_ns: 0,
            queries_per_sec: 2_880_000.0,
            broadphase_candidate_count: 5,
            broadphase_rejected_candidate_count: 2,
            broadphase_pruned_node_count: 1,
            candidate_reduction_effectiveness: 0.5,
            interval_subdivisions: 1,
            interval_refinements: 0,
            certificate_successes: 0,
            interval_bracket: None,
            fallback_count: 720,
            contact_normal_provenance: None,
            wgsl_dispatch_count: 4,
            wgsl_dispatch_items: 256,
            wgsl_selected_workgroup_size: 64,
            wgsl_resident_shared_snapshot_artifacts: 1,
            cpu_certification_query_count: 0,
            hot_path_readback_bytes: 0,
            queue_submit_count: 1,
            scene_reupload_bytes: 0,
            candidate_table_overflow_fallback_count: 0,
            available_count: 10,
            consumed_count: 7,
            rejected_count: 2,
            unavailable_count: 1,
            witness_reuse_rate: 0.70,
            fallback_rate: 0.10,
        }],
    }];

    let reports = build_whole_frame_benchmark_reports(&presentation_reports, &collision_reports)
        .expect("build whole-frame reports");
    assert_eq!(reports.len(), 1);
    assert_eq!(
        reports[0],
        WholeFrameBenchmarkReport {
            scenario_id: "closure_1080p120_dense_constructive_dense_ray_casts".into(),
            test_name:
                "tests/whole_frame::test_whole_frame_dense_constructive_dense_ray_casts_ops_7200"
                    .to_string(),
            presentation_frame_time_ns: 6_000_000,
            collision_runtime_ns: 2_500_000,
            total_runtime_ns: 8_500_000,
            steady_state_fps: fps_from_frame_time_ns(8_500_000, 1),
            presentation_bottleneck_pass: Some("surface_resolve".to_string()),
            collision_fallback_rate: 0.10,
            collision_witness_reuse_rate: 0.70,
        }
    );

    let runtime_cases = whole_frame_runtime_cases_from_reports(&reports);
    assert_eq!(
        runtime_cases,
        vec![(
            "closure_1080p120_dense_constructive_dense_ray_casts".into(),
            "tests/whole_frame::test_whole_frame_dense_constructive_dense_ray_casts_ops_7200"
                .to_string(),
            8_500_000,
        )]
    );
}

#[test]
fn engine_frame_benchmark_reports_assemble_presentation_and_collision_subsystems() {
    let mut frame_cost = sample_presentation_frame_cost(64, 64, 1.0, 128, 2.0, 16, 8, 6_000);
    frame_cost.gpu_runtime.queue_submit_count = 1;
    frame_cost.gpu_runtime.timestamps_supported = true;
    frame_cost.gpu_runtime.timestamped_pass_count = 2;
    frame_cost.gpu_runtime.readback_bytes = 32;
    frame_cost.gpu_runtime.scene_reupload_bytes = 64;
    let presentation_reports = vec![PresentationBenchmarkReport {
        scenario_id: "closure_1080p120_dense_constructive_dense_ray_casts".into(),
        test_name:
            "tests/whole_frame::test_whole_frame_dense_constructive_dense_ray_casts_ops_7200"
                .to_string(),
        view: "show_dense_constructive_1080p120_closure_view".to_string(),
        region: "dense_constructive_region".to_string(),
        domain: "dense_constructive_domain".to_string(),
        backend: "wgsl".to_string(),
        observed_adapter_name: None,
        query_trace_solver_mode: "hybrid".to_string(),
        selected_workgroup_size: 64,
        frames_executed: 7,
        frame_time_ns: 42_000_000,
        steady_state_fps: fps_from_frame_time_ns(42_000_000, 7),
        field_samples: 128,
        quality_tier: "realtime_120".to_string(),
        target_fps: 120,
        internal_resolution_scale: 1.0,
        reconstructed_output: false,
        quality_history: vec!["realtime_120".to_string()],
        internal_resolution_history: vec![1.0],
        bottleneck_pass: Some("surface_resolve".to_string()),
        active_acceleration_artifacts: vec!["view_tile_culling".to_string()],
        performance_gain_sources: vec!["tile_culling".to_string()],
        frame_cost,
        frame_cost_history: vec![],
        wgsl_workgroup_comparison: None,
        ab_comparison: None,
    }];
    let collision_reports = vec![CollisionBenchmarkReport {
        suite: "whole_frame".to_string(),
        backend: "wgsl".to_string(),
        command: "collision-suite".to_string(),
        query_count_total: 7_200,
        total_runtime_ns: 2_500_000,
        queries_per_sec: 2_880_000.0,
        average_candidate_count: 5.0,
        average_rejected_candidate_count: 2.0,
        average_pruned_node_count: 1.0,
        average_interval_subdivisions: 0.5,
        average_interval_refinements: 0.2,
        average_certificate_successes: 0.0,
        witness_reuse_rate: 0.75,
        fallback_rate: 0.10,
        available_count_total: 10,
        consumed_count_total: 7,
        rejected_count_total: 2,
        unavailable_count_total: 1,
        executions: vec![test_eval_perf::CollisionBenchmarkExecutionReport {
            name: "closure_1080p120_dense_constructive_dense_ray_casts".into(),
            plan_name: "dense_ray_casts".to_string(),
            contract_id: "whole_frame_collision".to_string(),
            query_count: 7_200,
            batch_count: 1,
            dispatch_count: 4,
            dispatch_items: 256,
            average_items_per_dispatch: 64.0,
            runtime_ns: 2_500_000,
            timestamps_supported: true,
            timestamped_pass_count: 4,
            gpu_time_total_ns: 1_250_000,
            gpu_time_max_ns: 450_000,
            queries_per_sec: 2_880_000.0,
            broadphase_candidate_count: 5,
            broadphase_rejected_candidate_count: 2,
            broadphase_pruned_node_count: 1,
            candidate_reduction_effectiveness: 0.5,
            interval_subdivisions: 1,
            interval_refinements: 0,
            certificate_successes: 0,
            interval_bracket: None,
            fallback_count: 720,
            contact_normal_provenance: None,
            wgsl_dispatch_count: 4,
            wgsl_dispatch_items: 256,
            wgsl_selected_workgroup_size: 64,
            wgsl_resident_shared_snapshot_artifacts: 1,
            cpu_certification_query_count: 0,
            hot_path_readback_bytes: 0,
            queue_submit_count: 1,
            scene_reupload_bytes: 64,
            candidate_table_overflow_fallback_count: 0,
            available_count: 10,
            consumed_count: 7,
            rejected_count: 2,
            unavailable_count: 1,
            witness_reuse_rate: 0.70,
            fallback_rate: 0.10,
        }],
    }];

    let profile = PerfClosureProfile::canonical_1080p120();
    let reports = build_engine_frame_benchmark_reports(
        &presentation_reports,
        &collision_reports,
        Some(&profile.engine_frame_budget),
    )
    .expect("build engine-frame reports");
    assert_eq!(reports.len(), 1);
    assert_eq!(
        reports[0].scenario_id,
        "closure_1080p120_dense_constructive_dense_ray_casts"
    );
    assert_eq!(reports[0].frame_count, 7);
    assert_eq!(reports[0].frame_wall_time_ns, 8_500_000);
    assert!(
        (reports[0].steady_state_fps - fps_from_frame_time_ns(8_500_000, 1)).abs() < f64::EPSILON
    );
    assert_eq!(reports[0].state_advance_runtime_ns, 0);
    assert_eq!(reports[0].queue_submit_count, 2);
    assert_eq!(reports[0].hot_path_readback_bytes, 0);
    assert_eq!(reports[0].scene_reupload_bytes, 128);
    assert_eq!(reports[0].gpu_critical_path_ns, Some(1_250_000));
    assert_eq!(reports[0].future_subsystem_reserve_ns, 0);
    assert_eq!(reports[0].subsystem_reports.len(), 3);
    assert_eq!(reports[0].subsystem_reports[0].label, "state_advance");
    assert_eq!(reports[0].subsystem_reports[0].cpu_critical_path_micros, 0);
    assert!(
        reports[0].subsystem_reports[0]
            .notes
            .iter()
            .any(|note| note == "reserved-slot-unsampled")
    );
    assert_eq!(reports[0].subsystem_reports[1].label, "presentation");
    assert_eq!(reports[0].subsystem_reports[1].hot_path_readback_bytes, 0);
    assert_eq!(reports[0].subsystem_reports[2].label, "collision");
    assert_eq!(
        reports[0].subsystem_reports[2].gpu_critical_path_micros,
        Some(1_250)
    );
    assert!(
        reports[0].subsystem_reports[2]
            .notes
            .iter()
            .any(|note| note == "gpu_timestamped_pass_count=4")
    );
    assert!(
        !reports[0].subsystem_reports[2]
            .notes
            .iter()
            .any(|note| note == "gpu_critical_path_proxy=runtime_ns")
    );
    assert!(
        !reports[0]
            .violations
            .iter()
            .any(|violation| violation == "engine_frame_hot_path_readback_budget_exceeded")
    );
    assert!(
        reports[0]
            .violations
            .iter()
            .any(|violation| violation == "engine_frame_future_reserve_exhausted")
    );
}

#[test]
fn engine_frame_benchmark_reports_preserve_exact_future_reserve_budget() {
    let mut frame_cost = sample_presentation_frame_cost(64, 64, 1.0, 128, 2.0, 16, 8, 6_000);
    frame_cost.gpu_runtime.queue_submit_count = 1;
    let presentation_reports = vec![PresentationBenchmarkReport {
        scenario_id: "closure_fixture".into(),
        test_name: "tests/whole_frame::test_fixture_ops_1".to_string(),
        view: "closure_fixture_view".to_string(),
        region: "closure_fixture_region".to_string(),
        domain: "closure_fixture_domain".to_string(),
        backend: "wgsl".to_string(),
        observed_adapter_name: None,
        query_trace_solver_mode: "hybrid".to_string(),
        selected_workgroup_size: 64,
        frames_executed: 7,
        frame_time_ns: 42_560_000,
        steady_state_fps: fps_from_frame_time_ns(42_560_000, 7),
        field_samples: 128,
        quality_tier: "realtime_120".to_string(),
        target_fps: 120,
        internal_resolution_scale: 1.0,
        reconstructed_output: false,
        quality_history: vec!["realtime_120".to_string()],
        internal_resolution_history: vec![1.0],
        bottleneck_pass: Some("surface_resolve".to_string()),
        active_acceleration_artifacts: vec![],
        performance_gain_sources: vec![],
        frame_cost,
        frame_cost_history: vec![],
        wgsl_workgroup_comparison: None,
        ab_comparison: None,
    }];
    let collision_reports = vec![CollisionBenchmarkReport {
        suite: "whole_frame".to_string(),
        backend: "wgsl".to_string(),
        command: "collision-suite".to_string(),
        query_count_total: 1,
        total_runtime_ns: 1_250_000,
        queries_per_sec: 0.0,
        average_candidate_count: 0.0,
        average_rejected_candidate_count: 0.0,
        average_pruned_node_count: 0.0,
        average_interval_subdivisions: 0.0,
        average_interval_refinements: 0.0,
        average_certificate_successes: 0.0,
        witness_reuse_rate: 0.0,
        fallback_rate: 0.0,
        available_count_total: 0,
        consumed_count_total: 0,
        rejected_count_total: 0,
        unavailable_count_total: 0,
        executions: vec![test_eval_perf::CollisionBenchmarkExecutionReport {
            name: "closure_fixture".into(),
            plan_name: "dense_ray_casts".to_string(),
            contract_id: "whole_frame_collision".to_string(),
            query_count: 1,
            batch_count: 1,
            dispatch_count: 1,
            dispatch_items: 1,
            average_items_per_dispatch: 1.0,
            runtime_ns: 1_250_000,
            timestamps_supported: false,
            timestamped_pass_count: 0,
            gpu_time_total_ns: 0,
            gpu_time_max_ns: 0,
            queries_per_sec: 800_000.0,
            broadphase_candidate_count: 1,
            broadphase_rejected_candidate_count: 0,
            broadphase_pruned_node_count: 0,
            candidate_reduction_effectiveness: 1.0,
            interval_subdivisions: 0,
            interval_refinements: 0,
            certificate_successes: 0,
            interval_bracket: None,
            fallback_count: 0,
            contact_normal_provenance: None,
            wgsl_dispatch_count: 1,
            wgsl_dispatch_items: 1,
            wgsl_selected_workgroup_size: 64,
            wgsl_resident_shared_snapshot_artifacts: 0,
            cpu_certification_query_count: 0,
            hot_path_readback_bytes: 0,
            queue_submit_count: 1,
            scene_reupload_bytes: 0,
            candidate_table_overflow_fallback_count: 0,
            available_count: 0,
            consumed_count: 0,
            rejected_count: 0,
            unavailable_count: 0,
            witness_reuse_rate: 0.0,
            fallback_rate: 0.0,
        }],
    }];

    let profile = PerfClosureProfile::canonical_1080p120();
    let reports = build_engine_frame_benchmark_reports(
        &presentation_reports,
        &collision_reports,
        Some(&profile.engine_frame_budget),
    )
    .expect("build engine-frame reports");

    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].frame_wall_time_ns, 7_330_000);
    assert_eq!(reports[0].future_subsystem_reserve_ns, 1_000_000);
    assert!(
        !reports[0]
            .violations
            .iter()
            .any(|violation| violation == "engine_frame_future_reserve_exhausted")
    );
}

#[test]
fn engine_frame_benchmark_reports_use_measured_presentation_frame_maxima() {
    let mut final_frame = sample_presentation_frame_cost(64, 64, 1.0, 128, 2.0, 16, 8, 6_000);
    final_frame.gpu_runtime.gpu_time_total_micros = 700;
    final_frame.gpu_runtime.queue_submit_count = 1;
    final_frame.gpu_runtime.timestamps_supported = true;
    final_frame.gpu_runtime.timestamped_pass_count = 2;
    final_frame.gpu_runtime.readback_bytes = 32;
    final_frame.gpu_runtime.scene_reupload_bytes = 64;

    let mut peak_frame = final_frame.clone();
    peak_frame.gpu_runtime.gpu_time_total_micros = 900;
    peak_frame.gpu_runtime.queue_submit_count = 3;
    peak_frame.gpu_runtime.timestamped_pass_count = 1;
    peak_frame.gpu_runtime.readback_bytes = 48;
    peak_frame.gpu_runtime.scene_reupload_bytes = 128;
    let final_history_frame = final_frame.clone();

    let presentation_reports = vec![PresentationBenchmarkReport {
        scenario_id: "closure_1080p120_dense_constructive_dense_ray_casts".into(),
        test_name:
            "tests/whole_frame::test_whole_frame_dense_constructive_dense_ray_casts_ops_7200"
                .to_string(),
        view: "show_dense_constructive_1080p120_closure_view".to_string(),
        region: "dense_constructive_region".to_string(),
        domain: "dense_constructive_domain".to_string(),
        backend: "wgsl".to_string(),
        observed_adapter_name: None,
        query_trace_solver_mode: "hybrid".to_string(),
        selected_workgroup_size: 64,
        frames_executed: 2,
        frame_time_ns: 12_000_000,
        steady_state_fps: fps_from_frame_time_ns(12_000_000, 2),
        field_samples: 128,
        quality_tier: "realtime_120".to_string(),
        target_fps: 120,
        internal_resolution_scale: 1.0,
        reconstructed_output: false,
        quality_history: vec!["realtime_120".to_string(), "realtime_120".to_string()],
        internal_resolution_history: vec![1.0, 1.0],
        bottleneck_pass: Some("surface_resolve".to_string()),
        active_acceleration_artifacts: vec!["view_tile_culling".to_string()],
        performance_gain_sources: vec!["tile_culling".to_string()],
        frame_cost: final_frame,
        frame_cost_history: vec![peak_frame, final_history_frame],
        wgsl_workgroup_comparison: None,
        ab_comparison: None,
    }];
    let collision_reports = vec![CollisionBenchmarkReport {
        suite: "whole_frame".to_string(),
        backend: "wgsl".to_string(),
        command: "collision-suite".to_string(),
        query_count_total: 7_200,
        total_runtime_ns: 2_500_000,
        queries_per_sec: 2_880_000.0,
        average_candidate_count: 5.0,
        average_rejected_candidate_count: 2.0,
        average_pruned_node_count: 1.0,
        average_interval_subdivisions: 0.5,
        average_interval_refinements: 0.2,
        average_certificate_successes: 0.0,
        witness_reuse_rate: 0.75,
        fallback_rate: 0.10,
        available_count_total: 10,
        consumed_count_total: 7,
        rejected_count_total: 2,
        unavailable_count_total: 1,
        executions: vec![test_eval_perf::CollisionBenchmarkExecutionReport {
            name: "closure_1080p120_dense_constructive_dense_ray_casts".into(),
            plan_name: "dense_ray_casts".to_string(),
            contract_id: "whole_frame_collision".to_string(),
            query_count: 7_200,
            batch_count: 1,
            dispatch_count: 4,
            dispatch_items: 256,
            average_items_per_dispatch: 64.0,
            runtime_ns: 2_500_000,
            timestamps_supported: false,
            timestamped_pass_count: 0,
            gpu_time_total_ns: 0,
            gpu_time_max_ns: 0,
            queries_per_sec: 2_880_000.0,
            broadphase_candidate_count: 5,
            broadphase_rejected_candidate_count: 2,
            broadphase_pruned_node_count: 1,
            candidate_reduction_effectiveness: 0.5,
            interval_subdivisions: 1,
            interval_refinements: 0,
            certificate_successes: 0,
            interval_bracket: None,
            fallback_count: 720,
            contact_normal_provenance: None,
            wgsl_dispatch_count: 4,
            wgsl_dispatch_items: 256,
            wgsl_selected_workgroup_size: 64,
            wgsl_resident_shared_snapshot_artifacts: 1,
            cpu_certification_query_count: 0,
            hot_path_readback_bytes: 0,
            queue_submit_count: 0,
            scene_reupload_bytes: 0,
            candidate_table_overflow_fallback_count: 0,
            available_count: 10,
            consumed_count: 7,
            rejected_count: 2,
            unavailable_count: 1,
            witness_reuse_rate: 0.70,
            fallback_rate: 0.10,
        }],
    }];

    let profile = PerfClosureProfile::canonical_1080p120();
    let reports = build_engine_frame_benchmark_reports(
        &presentation_reports,
        &collision_reports,
        Some(&profile.engine_frame_budget),
    )
    .expect("build engine-frame reports");

    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].queue_submit_count, 3);
    assert_eq!(reports[0].hot_path_readback_bytes, 32);
    assert_eq!(reports[0].scene_reupload_bytes, 128);
    assert_eq!(reports[0].gpu_critical_path_ns, Some(3_400_000));
    assert!(
        reports[0]
            .violations
            .iter()
            .any(|violation| violation == "engine_frame_hot_path_readback_budget_exceeded")
    );
    assert!(
        reports[0]
            .violations
            .iter()
            .any(|violation| violation == "engine_frame_queue_submit_budget_exceeded")
    );
}

#[test]
fn engine_frame_benchmark_reports_keep_timestamped_collision_zeroes_off_runtime_proxy() {
    let mut frame_cost = sample_presentation_frame_cost(64, 64, 1.0, 128, 2.0, 16, 8, 6_000);
    frame_cost.gpu_runtime.queue_submit_count = 1;
    let presentation_reports = vec![PresentationBenchmarkReport {
        scenario_id: "closure_fixture".into(),
        test_name: "tests/whole_frame::test_fixture_ops_1".to_string(),
        view: "closure_fixture_view".to_string(),
        region: "closure_fixture_region".to_string(),
        domain: "closure_fixture_domain".to_string(),
        backend: "wgsl".to_string(),
        observed_adapter_name: None,
        query_trace_solver_mode: "hybrid".to_string(),
        selected_workgroup_size: 64,
        frames_executed: 1,
        frame_time_ns: 6_000_000,
        steady_state_fps: fps_from_frame_time_ns(6_000_000, 1),
        field_samples: 128,
        quality_tier: "realtime_120".to_string(),
        target_fps: 120,
        internal_resolution_scale: 1.0,
        reconstructed_output: false,
        quality_history: vec!["realtime_120".to_string()],
        internal_resolution_history: vec![1.0],
        bottleneck_pass: Some("surface_resolve".to_string()),
        active_acceleration_artifacts: vec!["view_tile_culling".to_string()],
        performance_gain_sources: vec!["tile_culling".to_string()],
        frame_cost,
        frame_cost_history: vec![],
        wgsl_workgroup_comparison: None,
        ab_comparison: None,
    }];
    let collision_reports = vec![CollisionBenchmarkReport {
        suite: "whole_frame".to_string(),
        backend: "wgsl".to_string(),
        command: "collision-suite".to_string(),
        query_count_total: 64,
        total_runtime_ns: 2_500_000,
        queries_per_sec: 25_600.0,
        average_candidate_count: 5.0,
        average_rejected_candidate_count: 2.0,
        average_pruned_node_count: 1.0,
        average_interval_subdivisions: 0.5,
        average_interval_refinements: 0.2,
        average_certificate_successes: 0.0,
        witness_reuse_rate: 0.75,
        fallback_rate: 0.10,
        available_count_total: 10,
        consumed_count_total: 7,
        rejected_count_total: 2,
        unavailable_count_total: 1,
        executions: vec![test_eval_perf::CollisionBenchmarkExecutionReport {
            name: "closure_fixture".into(),
            plan_name: "dense_ray_casts".to_string(),
            contract_id: "whole_frame_collision".to_string(),
            query_count: 64,
            batch_count: 1,
            dispatch_count: 1,
            dispatch_items: 64,
            average_items_per_dispatch: 64.0,
            runtime_ns: 2_500_000,
            timestamps_supported: true,
            timestamped_pass_count: 1,
            gpu_time_total_ns: 0,
            gpu_time_max_ns: 0,
            queries_per_sec: 25_600.0,
            broadphase_candidate_count: 5,
            broadphase_rejected_candidate_count: 2,
            broadphase_pruned_node_count: 1,
            candidate_reduction_effectiveness: 0.5,
            interval_subdivisions: 1,
            interval_refinements: 0,
            certificate_successes: 0,
            interval_bracket: None,
            fallback_count: 6,
            contact_normal_provenance: None,
            wgsl_dispatch_count: 1,
            wgsl_dispatch_items: 64,
            wgsl_selected_workgroup_size: 64,
            wgsl_resident_shared_snapshot_artifacts: 1,
            cpu_certification_query_count: 0,
            hot_path_readback_bytes: 0,
            queue_submit_count: 1,
            scene_reupload_bytes: 0,
            candidate_table_overflow_fallback_count: 0,
            available_count: 10,
            consumed_count: 7,
            rejected_count: 2,
            unavailable_count: 1,
            witness_reuse_rate: 0.75,
            fallback_rate: 0.10,
        }],
    }];

    let profile = PerfClosureProfile::canonical_1080p120();
    let reports = build_engine_frame_benchmark_reports(
        &presentation_reports,
        &collision_reports,
        Some(&profile.engine_frame_budget),
    )
    .expect("build engine-frame reports");

    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].subsystem_reports[2].label, "collision");
    assert_eq!(
        reports[0].subsystem_reports[2].gpu_critical_path_micros,
        Some(0)
    );
    assert!(
        reports[0].subsystem_reports[2]
            .notes
            .iter()
            .any(|note| note == "gpu_timestamped_pass_count=1")
    );
    assert!(
        !reports[0].subsystem_reports[2]
            .notes
            .iter()
            .any(|note| note == "gpu_critical_path_proxy=runtime_ns")
    );
}

#[test]
fn engine_frame_runtime_cases_follow_scheduler_wall_time() {
    let runtime_cases = engine_frame_runtime_cases_from_reports(&[EngineFrameBenchmarkReport {
        scenario_id: "closure_fixture".into(),
        test_name: "tests/whole_frame::test_fixture".to_string(),
        frame_count: 1,
        frame_wall_time_ns: 8_750_000,
        cpu_critical_path_ns: 8_750_000,
        gpu_critical_path_ns: Some(6_000_000),
        present_wait_ns: 0,
        readback_wait_ns: 0,
        steady_state_fps: fps_from_frame_time_ns(8_750_000, 1),
        presentation_runtime_ns: 6_000_000,
        collision_runtime_ns: 2_500_000,
        state_advance_runtime_ns: 250_000,
        future_subsystem_reserve_ns: 1_000_000,
        queue_submit_count: 2,
        hot_path_readback_bytes: 0,
        scene_reupload_bytes: 64,
        active_degradations: vec!["enable_hit_compaction".to_string()],
        violations: vec![],
        subsystem_reports: vec![],
    }]);
    assert_eq!(
        runtime_cases,
        vec![(
            "closure_fixture".into(),
            "tests/whole_frame::test_fixture".to_string(),
            8_750_000,
        )]
    );
}

#[test]
fn frame_closure_status_uses_engine_frame_per_frame_runtime_without_dividing_by_frame_count() {
    let mut profile = PerfClosureProfile::canonical_1080p120();
    profile.output_width = 64;
    profile.output_height = 64;
    profile.warmup_runs = 0;
    profile.measured_runs = 1;
    profile.frame_budget.median_ms = 100.0;
    profile.frame_budget.p95_ms = 100.0;
    profile.primary_visibility_budget.median_ms = 100.0;
    profile.primary_visibility_budget.p95_ms = 100.0;

    let mut frame_cost = sample_presentation_frame_cost(64, 64, 1.0, 128, 2.0, 16, 8, 1_000);
    frame_cost.quality.tier = "realtime_120".to_string();
    frame_cost.quality.target_fps = 120;

    let presentation_report = PresentationBenchmarkReport {
        scenario_id: "closure_fixture".into(),
        test_name: "tests/fixture".to_string(),
        view: "view".to_string(),
        region: "region".to_string(),
        domain: "domain".to_string(),
        backend: "wgsl".to_string(),
        observed_adapter_name: Some("Test Adapter".to_string()),
        query_trace_solver_mode: "hybrid".to_string(),
        selected_workgroup_size: 64,
        frames_executed: 7,
        frame_time_ns: 63_000_000,
        steady_state_fps: fps_from_frame_time_ns(63_000_000, 7),
        field_samples: 128,
        quality_tier: "realtime_120".to_string(),
        target_fps: 120,
        internal_resolution_scale: 1.0,
        reconstructed_output: false,
        quality_history: vec!["realtime_120".to_string()],
        internal_resolution_history: vec![1.0],
        bottleneck_pass: Some("primary_visibility".to_string()),
        active_acceleration_artifacts: vec![],
        performance_gain_sources: vec![],
        frame_cost,
        frame_cost_history: vec![],
        wgsl_workgroup_comparison: None,
        ab_comparison: None,
    };

    let report = build_frame_closure_status(
        &profile,
        Some("engine_frame"),
        std::slice::from_ref(&presentation_report),
        &[],
        &[],
        &[],
        &[EngineFrameBenchmarkReport {
            scenario_id: "closure_fixture".into(),
            test_name: "tests/fixture".to_string(),
            frame_count: 7,
            frame_wall_time_ns: 9_000_000,
            cpu_critical_path_ns: 9_000_000,
            gpu_critical_path_ns: Some(7_000_000),
            present_wait_ns: 0,
            readback_wait_ns: 0,
            steady_state_fps: fps_from_frame_time_ns(9_000_000, 1),
            presentation_runtime_ns: 6_000_000,
            collision_runtime_ns: 2_500_000,
            state_advance_runtime_ns: 500_000,
            future_subsystem_reserve_ns: 1_000_000,
            queue_submit_count: 1,
            hot_path_readback_bytes: 0,
            scene_reupload_bytes: 0,
            active_degradations: vec![],
            violations: vec![],
            subsystem_reports: vec![],
        }],
        &[],
        0,
        1,
    );

    assert_eq!(report.total_frame_median_ms, Some(9.0));
    assert_eq!(report.total_frame_p95_ms, Some(9.0));
    let total_frame_median_fps = report
        .total_frame_median_fps
        .expect("engine-frame suite should compute frame fps");
    assert!((total_frame_median_fps - (1000.0 / 9.0)).abs() < 0.01);
}

#[test]
fn engine_frame_closure_status_records_budget_and_scheduler_violations() {
    let mut profile = PerfClosureProfile::canonical_1080p120();
    profile.warmup_runs = 0;
    profile.measured_runs = 1;
    let report = build_engine_frame_closure_status(
        &profile,
        &[EngineFrameBenchmarkReport {
            scenario_id: "closure_fixture".into(),
            test_name: "tests/whole_frame::test_fixture".to_string(),
            frame_count: 1,
            frame_wall_time_ns: 9_000_000,
            cpu_critical_path_ns: 9_000_000,
            gpu_critical_path_ns: Some(6_500_000),
            present_wait_ns: 0,
            readback_wait_ns: 0,
            steady_state_fps: fps_from_frame_time_ns(9_000_000, 1),
            presentation_runtime_ns: 6_000_000,
            collision_runtime_ns: 2_000_000,
            state_advance_runtime_ns: 500_000,
            future_subsystem_reserve_ns: 500_000,
            queue_submit_count: 2,
            hot_path_readback_bytes: 32,
            scene_reupload_bytes: 64,
            active_degradations: vec!["enable_hit_compaction".to_string()],
            violations: vec!["engine_frame_future_reserve_exhausted".to_string()],
            subsystem_reports: vec![],
        }],
        &[],
        0,
        1,
    );
    assert_eq!(report.status, PerfClosureLaneStatus::Violated);
    assert_eq!(report.frame_wall_time_median_ms, Some(9.0));
    assert_eq!(report.presentation_median_ms, Some(6.0));
    assert_eq!(report.collision_median_ms, Some(2.0));
    assert_eq!(report.state_advance_median_ms, Some(0.5));
    assert_eq!(report.future_subsystem_reserve_ms, Some(0.5));
    assert_eq!(report.queue_submit_count, Some(2));
    assert_eq!(report.hot_path_readback_bytes, Some(32));
    assert!(
        report
            .active_degradations
            .contains(&"enable_hit_compaction".to_string())
    );
    assert!(
        report
            .violations
            .contains(&"engine_frame_future_reserve_exhausted".to_string())
    );
    assert!(
        report
            .notes
            .iter()
            .any(|note| note.contains("engine-frame reports collected"))
    );
    assert!(!report.notes.iter().any(|note| note.contains("not sampled")));
    assert!(
        report
            .notes
            .iter()
            .any(|note| note.contains("engine frame median"))
    );
    assert!(
        report
            .notes
            .iter()
            .any(|note| note.contains("engine-frame hot-path readback"))
    );
}

#[test]
fn engine_frame_closure_status_uses_true_percentiles_for_multi_sample_reports() {
    let mut profile = PerfClosureProfile::canonical_1080p120();
    profile.warmup_runs = 0;
    profile.measured_runs = 5;
    let reports = [3_000_000u128, 5_000_000, 7_000_000, 11_000_000, 13_000_000]
        .into_iter()
        .map(|frame_wall_time_ns| EngineFrameBenchmarkReport {
            scenario_id: format!("closure_fixture_{frame_wall_time_ns}").into(),
            test_name: "tests/whole_frame::test_fixture".to_string(),
            frame_count: 1,
            frame_wall_time_ns,
            cpu_critical_path_ns: frame_wall_time_ns,
            gpu_critical_path_ns: Some(frame_wall_time_ns.saturating_sub(1_000_000)),
            present_wait_ns: 0,
            readback_wait_ns: 0,
            steady_state_fps: fps_from_frame_time_ns(frame_wall_time_ns, 1),
            presentation_runtime_ns: 2_000_000,
            collision_runtime_ns: 1_000_000,
            state_advance_runtime_ns: 250_000,
            future_subsystem_reserve_ns: 2_000_000,
            queue_submit_count: 1,
            hot_path_readback_bytes: 0,
            scene_reupload_bytes: 0,
            active_degradations: vec![],
            violations: vec![],
            subsystem_reports: vec![],
        })
        .collect::<Vec<_>>();
    let report = build_engine_frame_closure_status(&profile, &reports, &[], 0, reports.len());

    assert_eq!(report.frame_wall_time_median_ms, Some(7.0));
    assert_eq!(report.frame_wall_time_p95_ms, Some(13.0));
    assert_eq!(report.cpu_critical_path_median_ms, Some(7.0));
    assert_eq!(report.gpu_critical_path_median_ms, Some(6.0));
}

#[test]
fn engine_frame_closure_status_keeps_reserved_state_advance_out_of_observed_medians() {
    let profile = PerfClosureProfile::canonical_1080p120();
    let report = build_engine_frame_closure_status(
        &profile,
        &[EngineFrameBenchmarkReport {
            scenario_id: "closure_fixture".into(),
            test_name: "tests/whole_frame::test_fixture".to_string(),
            frame_count: 1,
            frame_wall_time_ns: 8_000_000,
            cpu_critical_path_ns: 8_000_000,
            gpu_critical_path_ns: Some(3_000_000),
            present_wait_ns: 0,
            readback_wait_ns: 0,
            steady_state_fps: fps_from_frame_time_ns(8_000_000, 1),
            presentation_runtime_ns: 5_000_000,
            collision_runtime_ns: 3_000_000,
            state_advance_runtime_ns: 0,
            future_subsystem_reserve_ns: 1_000_000,
            queue_submit_count: 2,
            hot_path_readback_bytes: 0,
            scene_reupload_bytes: 0,
            active_degradations: vec![],
            violations: vec![],
            subsystem_reports: vec![
                wrela::engine_frame::EngineSubsystemReport {
                    kind: wrela::engine_frame::EngineSubsystemKind::StateAdvance,
                    label: "state_advance".into(),
                    work_items: 0,
                    cpu_critical_path_micros: 0,
                    gpu_critical_path_micros: None,
                    queue_submit_count: 0,
                    hot_path_readback_bytes: 0,
                    scene_reupload_bytes: 0,
                    wait_time_micros: 0,
                    notes: vec![
                        "reserved-slot-unsampled".to_string(),
                        "scheduler-adapter".to_string(),
                    ],
                },
                wrela::engine_frame::EngineSubsystemReport {
                    kind: wrela::engine_frame::EngineSubsystemKind::Collision,
                    label: "collision".into(),
                    work_items: 1,
                    cpu_critical_path_micros: 3_000,
                    gpu_critical_path_micros: Some(3_000),
                    queue_submit_count: 1,
                    hot_path_readback_bytes: 0,
                    scene_reupload_bytes: 0,
                    wait_time_micros: 0,
                    notes: vec![
                        "scheduler-adapter".to_string(),
                        "gpu_critical_path_proxy=runtime_ns".to_string(),
                    ],
                },
            ],
        }],
        &[],
        0,
        1,
    );

    assert_eq!(report.state_advance_median_ms, None);
    assert!(
        report
            .notes
            .iter()
            .any(|note| note.contains("reserve is accounted separately"))
    );
    assert!(
        report
            .notes
            .iter()
            .any(|note| note.contains("collision gpu critical path uses runtime proxy"))
    );
}

#[test]
fn closure_verdict_fails_when_engine_frame_lane_is_violated() {
    let profile = PerfClosureProfile::canonical_1080p120();
    let verdict = build_closure_verdict(
        &profile,
        &PerfClosureLaneStatusReport::unsampled(&profile.frame),
        &PerfClosureLaneStatusReport::unsampled(&profile.collision),
        &wrela::perf_target::PerfClosureEngineFrameStatusReport {
            status: PerfClosureLaneStatus::Violated,
            frame_wall_time_median_ms: Some(9.5),
            frame_wall_time_p95_ms: Some(10.0),
            cpu_critical_path_median_ms: Some(9.5),
            gpu_critical_path_median_ms: Some(6.0),
            presentation_median_ms: Some(5.0),
            collision_median_ms: Some(1.0),
            state_advance_median_ms: Some(0.25),
            future_subsystem_reserve_ms: Some(0.5),
            queue_submit_count: Some(2),
            hot_path_readback_bytes: Some(16),
            scene_reupload_bytes: Some(0),
            active_degradations: vec![],
            violations: vec!["engine_frame_future_reserve_exhausted".to_string()],
            notes: vec![],
        },
        &[],
        &[],
    );
    assert_eq!(verdict.status, PerfClosureVerdictStatus::Failed);
    assert_eq!(
        verdict.top_remaining_bottleneck.as_deref(),
        Some("engine_frame_hot_path_readback")
    );
    assert!(
        verdict
            .findings
            .iter()
            .any(|finding| finding.subsystem == "engine_frame"
                && finding.focus == "engine_frame_hot_path_readback_budget")
    );
}

#[test]
fn closure_verdict_keeps_frame_lane_as_compatibility_signal_for_engine_frame_suite() {
    let profile = PerfClosureProfile::canonical_1080p120();
    let frame_status = PerfClosureLaneStatusReport {
        lane: profile.frame.lane,
        protocol_id: profile.frame.protocol_id.clone(),
        suite: profile.frame.suite.clone(),
        status: PerfClosureLaneStatus::Violated,
        measured_output_width: Some(profile.output_width),
        measured_output_height: Some(profile.output_height),
        min_internal_resolution_scale_observed: Some(1.0),
        max_internal_resolution_scale_observed: Some(1.0),
        reconstructed_output_detected: Some(false),
        active_acceleration_artifacts: vec!["view_distance_clipmap".to_string()],
        active_degradations: vec![],
        hot_path_readback_bytes: Some(0),
        scene_reupload_bytes: Some(0),
        cpu_screen_sample_allocations: Some(0),
        attachment_cpu_bounce_count: Some(0),
        queue_submit_count: Some(1),
        primary_visibility_dispatch_count: Some(0),
        timestamps_supported: Some(false),
        timestamped_pass_count: Some(0),
        primary_visibility_median_ms: Some(0.1),
        primary_visibility_p95_ms: Some(0.2),
        total_frame_median_ms: Some(0.4),
        total_frame_median_fps: Some(2500.0),
        total_frame_p95_ms: Some(0.6),
        collision_runtime_median_ms: None,
        collision_runtime_p95_ms: None,
        collision_baseline_id: None,
        collision_runtime_regression_pct: None,
        dominant_bottleneck_pass: Some("surface_resolve".to_string()),
        notes: vec!["compatibility/debug frame note".to_string()],
    };
    let collision_status = PerfClosureLaneStatusReport {
        status: PerfClosureLaneStatus::Validated,
        notes: vec!["collision closure validated".to_string()],
        ..PerfClosureLaneStatusReport::unsampled(&profile.collision)
    };
    let engine_frame_status = wrela::perf_target::PerfClosureEngineFrameStatusReport {
        status: PerfClosureLaneStatus::Validated,
        frame_wall_time_median_ms: Some(2.0),
        frame_wall_time_p95_ms: Some(3.0),
        cpu_critical_path_median_ms: Some(2.0),
        gpu_critical_path_median_ms: None,
        presentation_median_ms: Some(0.8),
        collision_median_ms: Some(0.7),
        state_advance_median_ms: Some(0.25),
        future_subsystem_reserve_ms: Some(1.0),
        queue_submit_count: Some(1),
        hot_path_readback_bytes: Some(0),
        scene_reupload_bytes: Some(0),
        active_degradations: vec![],
        violations: vec![],
        notes: vec!["engine-frame closure met the canonical 1080p120 contract".to_string()],
    };

    let verdict = build_closure_verdict(
        &profile,
        &frame_status,
        &collision_status,
        &engine_frame_status,
        &[],
        &[],
    );

    assert_eq!(verdict.status, PerfClosureVerdictStatus::Met);
    assert!(verdict.top_remaining_bottleneck.is_none());
}

#[test]
fn high_volume_collision_collection_uses_batch_entrypoints_instead_of_per_query_execute_loops() {
    let source = include_str!("collection.rs");
    assert!(
        source.contains("execute_batch_metrics_only"),
        "expected batch-based WGSL collision collection in perf_engine::collection"
    );
    assert!(
        !source.contains("plan.execute("),
        "high-volume collision collection should not call plan.execute(...) directly anymore"
    );
}

#[test]
fn frame_closure_status_uses_whole_frame_totals_for_whole_frame_suite() {
    let mut profile = PerfClosureProfile::canonical_1080p120();
    profile.output_width = 64;
    profile.output_height = 64;
    profile.warmup_runs = 0;
    profile.measured_runs = 1;
    profile.frame_budget.median_ms = 100.0;
    profile.frame_budget.p95_ms = 100.0;
    profile.primary_visibility_budget.median_ms = 100.0;
    profile.primary_visibility_budget.p95_ms = 100.0;

    let mut frame_cost = sample_presentation_frame_cost(64, 64, 1.0, 128, 2.0, 16, 8, 3_000);
    frame_cost.quality.tier = "realtime_120".to_string();
    frame_cost.quality.target_fps = 120;
    frame_cost.tile_cull_total_tiles = 16;
    frame_cost.tile_cull_active_tiles = 8;
    frame_cost.tile_candidate_reduction = 64;
    frame_cost.passes.push(wrela::presentation_exec::PresentationPassCost {
        pass_id: "view_distance_clipmap".to_string(),
        pass_kind: "view_distance_clipmap".to_string(),
        work_items: 8,
        elapsed_micros: 0,
        gpu_elapsed_micros: None,
        dispatch_count: 1,
        attachment_bytes_read: 0,
        attachment_bytes_written: 0,
        clipmap: Some(wrela::presentation_exec::PresentationClipmapPassMetadata {
            status: wrela::acceleration::clipmap::ViewDistanceClipmapBuildMode::Reused,
            fallback_reasons: vec![],
        }),
        notes: vec![
            "view_distance_clipmap schema_version=1 semantic_root=show_dense_constructive_1080p120_closure_view status=reused resolution=64x64 internal=64x64 bricks=8,8,1 voxel_size=8 narrow_band_width=96 build=0 update=0 reuse=1 upload=0 build_bytes=0 upload_bytes=0 eviction=0 usage=8 fallback_reasons=none layout_signature=1 runtime_signature=2"
                .to_string(),
        ],
    });

    let presentation_report = PresentationBenchmarkReport {
        scenario_id: "closure_1080p120_dense_constructive_dense_ray_casts".into(),
        test_name:
            "tests/whole_frame::test_whole_frame_dense_constructive_dense_ray_casts_ops_7200"
                .to_string(),
        view: "show_dense_constructive_1080p120_closure_view".to_string(),
        region: "dense_constructive_region".to_string(),
        domain: "dense_constructive_domain".to_string(),
        backend: "wgsl".to_string(),
        observed_adapter_name: None,
        query_trace_solver_mode: "hybrid".to_string(),
        selected_workgroup_size: 64,
        frames_executed: 1,
        frame_time_ns: 3_000_000,
        steady_state_fps: fps_from_frame_time_ns(3_000_000, 1),
        field_samples: 128,
        quality_tier: "realtime_120".to_string(),
        target_fps: 120,
        internal_resolution_scale: 1.0,
        reconstructed_output: false,
        quality_history: vec!["realtime_120".to_string()],
        internal_resolution_history: vec![1.0],
        bottleneck_pass: Some("primary_visibility".to_string()),
        active_acceleration_artifacts: vec!["view_tile_culling".to_string()],
        performance_gain_sources: vec!["tile_culling".to_string()],
        frame_cost: frame_cost.clone(),
        frame_cost_history: vec![],
        wgsl_workgroup_comparison: None,
        ab_comparison: None,
    };
    let whole_frame_reports = vec![WholeFrameBenchmarkReport {
        scenario_id: "closure_1080p120_dense_constructive_dense_ray_casts".into(),
        test_name:
            "tests/whole_frame::test_whole_frame_dense_constructive_dense_ray_casts_ops_7200"
                .to_string(),
        presentation_frame_time_ns: 3_000_000,
        collision_runtime_ns: 5_000_000,
        total_runtime_ns: 8_000_000,
        steady_state_fps: fps_from_frame_time_ns(8_000_000, 1),
        presentation_bottleneck_pass: Some("primary_visibility".to_string()),
        collision_fallback_rate: 0.0,
        collision_witness_reuse_rate: 1.0,
    }];

    let report = build_frame_closure_status(
        &profile,
        Some("whole_frame"),
        &[presentation_report],
        &[],
        &whole_frame_reports,
        &[],
        &[],
        &[],
        0,
        1,
    );

    assert_eq!(report.status, PerfClosureLaneStatus::Validated);
    assert_eq!(report.total_frame_median_ms, Some(8.0));
    assert_eq!(report.total_frame_median_fps, fps_from_ms(8.0));
    assert_eq!(report.primary_visibility_median_ms, Some(3.0));
}

#[test]
fn frame_closure_status_fails_closed_for_unknown_whole_frame_scenario_ids() {
    let mut profile = PerfClosureProfile::canonical_1080p120();
    profile.output_width = 64;
    profile.output_height = 64;
    profile.warmup_runs = 0;
    profile.measured_runs = 1;
    profile.frame_budget.median_ms = 100.0;
    profile.frame_budget.p95_ms = 100.0;
    profile.primary_visibility_budget.median_ms = 100.0;
    profile.primary_visibility_budget.p95_ms = 100.0;

    let mut frame_cost = sample_presentation_frame_cost(64, 64, 1.0, 128, 2.0, 16, 8, 3_000);
    frame_cost.quality.tier = "realtime_120".to_string();
    frame_cost.quality.target_fps = 120;
    frame_cost.tile_cull_total_tiles = 16;
    frame_cost.tile_cull_active_tiles = 8;
    frame_cost.tile_candidate_reduction = 64;
    frame_cost
        .passes
        .push(wrela::presentation_exec::PresentationPassCost {
            pass_id: "view_distance_clipmap".to_string(),
            pass_kind: "view_distance_clipmap".to_string(),
            work_items: 8,
            elapsed_micros: 0,
            gpu_elapsed_micros: None,
            dispatch_count: 1,
            attachment_bytes_read: 0,
            attachment_bytes_written: 0,
            clipmap: Some(wrela::presentation_exec::PresentationClipmapPassMetadata {
                status: wrela::acceleration::clipmap::ViewDistanceClipmapBuildMode::Reused,
                fallback_reasons: vec![],
            }),
            notes: vec![],
        });

    let presentation_report = PresentationBenchmarkReport {
        scenario_id: "closure_fixture".into(),
        test_name: "tests/whole_frame::test_fixture_ops_1".to_string(),
        view: "show_dense_constructive_1080p120_closure_view".to_string(),
        region: "dense_constructive_region".to_string(),
        domain: "dense_constructive_domain".to_string(),
        backend: "wgsl".to_string(),
        observed_adapter_name: None,
        query_trace_solver_mode: "hybrid".to_string(),
        selected_workgroup_size: 64,
        frames_executed: 1,
        frame_time_ns: 3_000_000,
        steady_state_fps: fps_from_frame_time_ns(3_000_000, 1),
        field_samples: 128,
        quality_tier: "realtime_120".to_string(),
        target_fps: 120,
        internal_resolution_scale: 1.0,
        reconstructed_output: false,
        quality_history: vec!["realtime_120".to_string()],
        internal_resolution_history: vec![1.0],
        bottleneck_pass: Some("primary_visibility".to_string()),
        active_acceleration_artifacts: vec!["view_tile_culling".to_string()],
        performance_gain_sources: vec!["tile_culling".to_string()],
        frame_cost,
        frame_cost_history: vec![],
        wgsl_workgroup_comparison: None,
        ab_comparison: None,
    };
    let whole_frame_reports = vec![WholeFrameBenchmarkReport {
        scenario_id: "closure_fixture".into(),
        test_name: "tests/whole_frame::test_fixture_ops_1".to_string(),
        presentation_frame_time_ns: 3_000_000,
        collision_runtime_ns: 5_000_000,
        total_runtime_ns: 8_000_000,
        steady_state_fps: fps_from_frame_time_ns(8_000_000, 1),
        presentation_bottleneck_pass: Some("primary_visibility".to_string()),
        collision_fallback_rate: 0.0,
        collision_witness_reuse_rate: 1.0,
    }];

    let report = build_frame_closure_status(
        &profile,
        Some("whole_frame"),
        &[presentation_report],
        &[],
        &whole_frame_reports,
        &[],
        &[],
        &[],
        0,
        1,
    );

    assert_eq!(report.status, PerfClosureLaneStatus::Violated);
    assert!(report.notes.iter().any(|note| {
        note.contains("not a recognized canonical whole-frame closure scenario id")
    }));
}

#[test]
fn closure_profile_prefers_observed_wgsl_runtime_metadata() {
    let mut profile = PerfClosureProfile::canonical_1080p120();
    let mut frame_cost = sample_presentation_frame_cost(64, 64, 1.0, 128, 2.0, 16, 8, 1_000);
    frame_cost.gpu_runtime.timestamps_supported = true;
    frame_cost.gpu_runtime.requested_limits_profile =
            "storage_buffers_per_stage=10 storage_binding_bytes=268435456 bind_groups=4 workgroup_x=128"
                .to_string();
    frame_cost.gpu_runtime.enabled_optional_features =
        vec!["timestamp_query".to_string(), "shader_f16".to_string()];

    apply_observed_wgsl_runtime_metadata(
        &mut profile,
        &[PresentationBenchmarkReport {
            scenario_id: "closure_fixture".into(),
            test_name: "tests/fixture".to_string(),
            view: "view".to_string(),
            region: "region".to_string(),
            domain: "domain".to_string(),
            backend: "wgsl".to_string(),
            observed_adapter_name: Some("Test Adapter".to_string()),
            query_trace_solver_mode: "hybrid".to_string(),
            selected_workgroup_size: 64,
            frames_executed: 1,
            frame_time_ns: 1_000_000,
            steady_state_fps: 1000.0,
            field_samples: 128,
            quality_tier: "realtime_120".to_string(),
            target_fps: 120,
            internal_resolution_scale: 1.0,
            reconstructed_output: false,
            quality_history: vec!["realtime_120".to_string()],
            internal_resolution_history: vec![1.0],
            bottleneck_pass: Some("primary_visibility".to_string()),
            active_acceleration_artifacts: vec![],
            performance_gain_sources: vec![],
            frame_cost,
            frame_cost_history: vec![],
            wgsl_workgroup_comparison: None,
            ab_comparison: None,
        }],
    );

    assert_eq!(profile.adapter_name, "Test Adapter");
    assert_eq!(
        profile.requested_limits_profile,
        "storage_buffers_per_stage=10 storage_binding_bytes=268435456 bind_groups=4 workgroup_x=128"
    );
    assert_eq!(
        profile.enabled_optional_features,
        vec!["shader_f16".to_string(), "timestamp_query".to_string()]
    );
    assert!(profile.timestamps_enabled);
    assert!(profile.f16_enabled);
    assert!(!profile.indirect_dispatch_enabled);
}

#[test]
fn closure_verdict_surface_names_the_top_bottleneck_and_subsystem_findings() {
    let profile = PerfClosureProfile::canonical_1080p120();
    let mut frame_status = PerfClosureLaneStatusReport::unsampled(&profile.frame);
    frame_status.status = PerfClosureLaneStatus::Violated;
    frame_status.total_frame_median_ms = Some(11.0);
    frame_status.total_frame_median_fps = fps_from_ms(11.0);
    frame_status.primary_visibility_median_ms = Some(4.0);
    frame_status.dominant_bottleneck_pass = Some("postprocess_shading".to_string());

    let mut collision_status = PerfClosureLaneStatusReport::unsampled(&profile.collision);
    collision_status.status = PerfClosureLaneStatus::Violated;
    collision_status.collision_runtime_regression_pct = Some(6.5);

    let mut frame_cost = sample_presentation_frame_cost(64, 64, 1.0, 100, 16.0, 100, 98, 11_000);
    frame_cost.execution_policy =
        "required=best-effort selected=heuristic-solver backend=wgsl".to_string();
    frame_cost.primary_hit_rate = 0.62;
    frame_cost.average_trace_steps = 16.0;
    frame_cost.support_prune_effectiveness = 0.02;
    frame_cost.candidate_count_before_pruning = 100;
    frame_cost.candidate_count_after_pruning = 98;
    frame_cost.active_acceleration_artifacts = vec![];
    frame_cost.performance_gain_sources = vec![];
    frame_cost.cache_brick_visits = 0;
    frame_cost.cache_brick_hits = 0;
    frame_cost.cache_brick_misses = 0;
    frame_cost.cache_interval_advances = 0;
    frame_cost.bottleneck_pass = Some("postprocess_shading".to_string());
    frame_cost.passes = vec![
        wrela::presentation_exec::PresentationPassCost {
            pass_id: "primary.visibility".to_string(),
            pass_kind: "primary_visibility".to_string(),
            work_items: 1024,
            elapsed_micros: 4_000,
            gpu_elapsed_micros: None,
            dispatch_count: 1,
            attachment_bytes_read: 0,
            attachment_bytes_written: 4_096,
            clipmap: None,
            notes: vec![],
        },
        wrela::presentation_exec::PresentationPassCost {
            pass_id: "postprocess.shading".to_string(),
            pass_kind: "postprocess".to_string(),
            work_items: 1024,
            elapsed_micros: 7_000,
            gpu_elapsed_micros: None,
            dispatch_count: 1,
            attachment_bytes_read: 4_096,
            attachment_bytes_written: 4_096,
            clipmap: None,
            notes: vec![],
        },
    ];

    let presentation_report = PresentationBenchmarkReport {
        scenario_id: "closure_fixture".into(),
        test_name: "tests/closure_fixture::test_ops_1".to_string(),
        view: "view".to_string(),
        region: "region".to_string(),
        domain: "domain".to_string(),
        backend: "wgsl".to_string(),
        observed_adapter_name: None,
        query_trace_solver_mode: "hybrid".to_string(),
        selected_workgroup_size: 64,
        frames_executed: 1,
        frame_time_ns: 11_000_000,
        steady_state_fps: fps_from_frame_time_ns(11_000_000, 1),
        field_samples: 100,
        quality_tier: "realtime_120".to_string(),
        target_fps: 120,
        internal_resolution_scale: 1.0,
        reconstructed_output: false,
        quality_history: vec!["realtime_120".to_string()],
        internal_resolution_history: vec![1.0],
        bottleneck_pass: Some("postprocess_shading".to_string()),
        active_acceleration_artifacts: vec![],
        performance_gain_sources: vec![],
        frame_cost,
        frame_cost_history: vec![],
        wgsl_workgroup_comparison: None,
        ab_comparison: None,
    };

    let collision_report = CollisionBenchmarkReport {
        suite: "collision_perf".to_string(),
        backend: "cpu".to_string(),
        command: "collision-suite".to_string(),
        query_count_total: 4,
        total_runtime_ns: 10_000,
        queries_per_sec: 400.0,
        average_candidate_count: 32.0,
        average_rejected_candidate_count: 28.0,
        average_pruned_node_count: 4.0,
        average_interval_subdivisions: 2.0,
        average_interval_refinements: 1.0,
        average_certificate_successes: 0.0,
        witness_reuse_rate: 0.20,
        fallback_rate: 0.50,
        available_count_total: 2,
        consumed_count_total: 0,
        rejected_count_total: 1,
        unavailable_count_total: 1,
        executions: vec![],
    };

    let verdict = build_closure_verdict(
        &profile,
        &frame_status,
        &collision_status,
        &wrela::perf_target::PerfClosureEngineFrameStatusReport::unsampled(),
        &[presentation_report],
        &[collision_report],
    );
    assert_eq!(verdict.status, PerfClosureVerdictStatus::Failed);
    assert_eq!(
        verdict.top_remaining_bottleneck.as_deref(),
        Some("postprocess_shading")
    );
    assert!(
        verdict
            .findings
            .iter()
            .any(|finding| finding.subsystem == "presentation" && finding.focus == "dense_rays")
    );
    assert!(
        verdict.findings.iter().any(
            |finding| finding.subsystem == "presentation" && finding.focus == "pruning_failure"
        )
    );
    assert!(
        verdict
            .findings
            .iter()
            .any(|finding| finding.subsystem == "acceleration"
                && finding.focus == "caches_unavailable_or_invalid")
    );
    assert!(
        verdict
            .findings
            .iter()
            .any(|finding| finding.subsystem == "acceleration"
                && finding.focus == "wgsl_linear_traversal")
    );
    assert!(
        verdict
            .findings
            .iter()
            .any(|finding| finding.subsystem == "presentation"
                && finding.focus == "visibility_vs_shading_bound")
    );
    assert!(
        verdict
            .findings
            .iter()
            .any(|finding| finding.subsystem == "collision"
                && finding.focus == "witness_reuse_invalid_or_unsupported")
    );
}

#[test]
fn rendered_closure_verdict_prints_execution_model_gate_reasons_directly() {
    let profile = PerfClosureProfile::canonical_1080p120();
    let mut report = PerfClosureReport::unsampled(profile);
    report.frame.total_frame_median_ms = Some(9.5);
    report.frame.total_frame_median_fps = fps_from_ms(9.5);
    report.verdict = PerfClosureVerdict {
            status: PerfClosureVerdictStatus::Failed,
            summary: "execution-model gates failed".to_string(),
            top_remaining_bottleneck: Some("attachment_cpu_bounce_gate".to_string()),
            findings: vec![
                PerfClosureFinding {
                    subsystem: "presentation".to_string(),
                    focus: "hot_path_readback_gate".to_string(),
                    summary: "the resident frame still performs hot-path readback, so the closure is not purely GPU-resident yet".to_string(),
                    evidence: vec![
                        "hot_path_readback_bytes=256".to_string(),
                        "max_hot_path_readback_bytes_per_frame=0".to_string(),
                    ],
                    next_step: "move the timed path off CPU readback and leave only the explicit timestamp budget, if supported".to_string(),
                },
                PerfClosureFinding {
                    subsystem: "presentation".to_string(),
                    focus: "attachment_cpu_bounce_gate".to_string(),
                    summary: "the measured lane still bounces attachments through CPU-owned memory".to_string(),
                    evidence: vec![
                        "attachment_cpu_bounce_count=2".to_string(),
                        "max_attachment_cpu_bounce_count=0".to_string(),
                    ],
                    next_step: "keep steady-state attachments resident on GPU buffers and reserve CPU bounce for explicit export/debug paths".to_string(),
                },
            ],
        };

    let rendered = render_closure_verdict_report(&report, true);

    assert!(rendered.contains("closure verdict: failed"));
    assert!(rendered.contains("frame median: 9.50 ms (105.26 FPS)"));
    assert!(rendered.contains("why-not-120:"));
    assert!(rendered.contains("focus=hot_path_readback_gate"));
    assert!(rendered.contains(
            "the resident frame still performs hot-path readback, so the closure is not purely GPU-resident yet"
        ));
    assert!(rendered.contains("focus=attachment_cpu_bounce_gate"));
    assert!(
        rendered.contains("the measured lane still bounces attachments through CPU-owned memory")
    );
}

#[test]
fn checked_in_collision_baseline_fixture_loads() {
    let summary = load_collision_baseline_summary("collision_perf.phase40_cpu_oracle")
        .expect("load checked-in collision baseline");
    assert!(summary.runtime_p50_ns > 0);
    assert!(summary.runtime_p95_ns >= summary.runtime_p50_ns);
    assert!(summary.runtime_p99_ns >= summary.runtime_p95_ns);
}

#[test]
fn presentation_debug_args_default_dimensions_to_64() {
    let spec = test_eval_perf::BenchmarkPresentationSpec {
        view: "bench_view".to_string(),
        region: "bench_region".to_string(),
        entry: Some("tests/bench_fixture.wr".to_string()),
        domain: Some("bench_domain".to_string()),
        width: None,
        height: None,
        frames: Some(4),
        camera_position: [0.0, 1.0, 2.0],
        camera_forward: [0.0, 0.0, -1.0],
        camera_up: [0.0, 1.0, 0.0],
        vertical_fov_degrees: 48.0,
    };
    let args = presentation_debug_args(&spec, QueryTraceSolverMode::Hybrid);
    assert!(
        args.windows(2)
            .any(|pair| pair[0] == "--width" && pair[1] == "64")
    );
    assert!(
        args.windows(2)
            .any(|pair| pair[0] == "--height" && pair[1] == "64")
    );
    assert!(
        args.windows(2)
            .any(|pair| pair[0] == "--frames" && pair[1] == "4")
    );
    assert!(
        args.windows(2)
            .any(|pair| pair[0] == "--view" && pair[1] == "bench_view")
    );
    assert!(
        args.windows(2)
            .any(|pair| pair[0] == "--region" && pair[1] == "bench_region")
    );
    assert!(
        args.windows(2)
            .any(|pair| pair[0] == "--domain" && pair[1] == "bench_domain")
    );
    assert!(
        args.windows(2)
            .any(|pair| pair[0] == "--solver-mode" && pair[1] == "hybrid")
    );
}

#[test]
fn presentation_debug_command_uses_wgsl_backend_and_shared_workgroup_override() {
    let spec = test_eval_perf::BenchmarkPresentationSpec {
        view: "bench_view".to_string(),
        region: "bench_region".to_string(),
        entry: Some("tests/bench_fixture.wr".to_string()),
        domain: Some("bench_domain".to_string()),
        width: Some(96),
        height: Some(54),
        frames: Some(2),
        camera_position: [0.0, 1.0, 2.0],
        camera_forward: [0.0, 0.0, -1.0],
        camera_up: [0.0, 1.0, 0.0],
        vertical_fov_degrees: 48.0,
    };
    let command = build_presentation_debug_command(
        Path::new("/tmp/wrela"),
        Path::new("/tmp/bench_fixture.wr"),
        &spec,
        QueryTraceSolverMode::Hybrid,
        Some(64),
        wrela::query_plan::DispatchBackend::Cpu,
        false,
    );
    let args = command
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert!(args.iter().any(|arg| arg == "--query-backend=cpu"));
    assert!(args.iter().any(|arg| arg == "presentation-debug"));
    let envs = command
        .get_envs()
        .filter_map(|(key, value)| {
            value.map(|value| {
                (
                    key.to_string_lossy().into_owned(),
                    value.to_string_lossy().into_owned(),
                )
            })
        })
        .collect::<HashMap<_, _>>();
    assert_eq!(
        envs.get(WGSL_WORKGROUP_SIZE_OVERRIDE_ENV)
            .map(String::as_str),
        Some("64")
    );
    assert!(!envs.contains_key(WRELA_PRESENTATION_DEBUG_WARM_QUALITY_PIPELINES_ENV));
    assert!(!envs.contains_key(WRELA_PRESENTATION_DEBUG_ADAPTIVE_WINDOW_ENV));
}

#[test]
fn presentation_debug_command_can_enable_quality_pipeline_warmup() {
    let spec = test_eval_perf::BenchmarkPresentationSpec {
        view: "bench_view".to_string(),
        region: "bench_region".to_string(),
        entry: Some("tests/bench_fixture.wr".to_string()),
        domain: Some("bench_domain".to_string()),
        width: Some(1920),
        height: Some(1080),
        frames: Some(7),
        camera_position: [0.0, 1.0, 2.0],
        camera_forward: [0.0, 0.0, -1.0],
        camera_up: [0.0, 1.0, 0.0],
        vertical_fov_degrees: 48.0,
    };
    let command = build_presentation_debug_command(
        Path::new("/tmp/wrela"),
        Path::new("/tmp/bench_fixture.wr"),
        &spec,
        QueryTraceSolverMode::Hybrid,
        None,
        wrela::query_plan::DispatchBackend::Wgsl,
        true,
    );
    let envs = command
        .get_envs()
        .filter_map(|(key, value)| {
            value.map(|value| {
                (
                    key.to_string_lossy().into_owned(),
                    value.to_string_lossy().into_owned(),
                )
            })
        })
        .collect::<HashMap<_, _>>();
    assert_eq!(
        envs.get(WRELA_PRESENTATION_DEBUG_WARM_QUALITY_PIPELINES_ENV)
            .map(String::as_str),
        Some("1")
    );
    assert_eq!(
        envs.get(WRELA_PRESENTATION_DEBUG_ADAPTIVE_WINDOW_ENV)
            .map(String::as_str),
        Some("1")
    );
}

fn sample_presentation_frame_cost(
    internal_width: u32,
    internal_height: u32,
    internal_resolution_scale: f32,
    field_samples: u32,
    average_trace_steps: f32,
    candidate_count_before_pruning: u32,
    candidate_count_after_pruning: u32,
    elapsed_micros: u128,
) -> wrela::presentation_exec::PresentationFrameCostReport {
    wrela::presentation_exec::PresentationFrameCostReport {
        semantic_domain: "bench_domain".to_string(),
        execution_policy: "required=best-effort selected=heuristic-solver backend=cpu".to_string(),
        legal_degradations: vec![],
        output_width: 64,
        output_height: 64,
        internal_width,
        internal_height,
        quality: wrela::presentation_exec::PresentationQualityReport {
            tier: "realtime_60".to_string(),
            target_fps: 60,
            output_width: 64,
            output_height: 64,
            internal_width,
            internal_height,
            internal_resolution_scale,
            achieved_native_output: internal_width == 64 && internal_height == 64,
            reconstructed_output: internal_width != 64 || internal_height != 64,
            temporal_mode: "TemporalAA".to_string(),
            radiance_mode: "full".to_string(),
            media_enabled: true,
            half_res_participants: false,
            hit_compaction_enabled: internal_resolution_scale < 1.0,
            active_degradations: Vec::new(),
        },
        primary_hit_rate: 0.75,
        average_trace_steps,
        max_trace_steps: 24,
        candidate_count_before_pruning,
        candidate_count_after_pruning,
        support_prune_effectiveness: 0.4,
        tile_cull_total_tiles: 16,
        tile_cull_active_tiles: 9,
        tile_cull_efficiency: 0.4375,
        tile_candidate_total_samples: 256,
        tile_candidate_active_samples: 128,
        tile_candidate_reduction: 128,
        tile_candidate_effectiveness: 0.5,
        tile_candidate_packet_count: 4,
        tile_candidate_packet_size: 32,
        packet_compaction_ratio: 1.0,
        packet_scheduling_active: true,
        selected_workgroup_size: 64,
        surface_resolve_count: 256,
        participant_resolve_count: 128,
        history_reuse_rate: 0.25,
        continuation_diagnostics: vec![],
        acceleration_node_visits: 0,
        union_cluster_visits: 0,
        ray_support_interval_rejections: 0,
        ray_support_entry_jumps: 0,
        repeat_cell_skips: 0,
        cache_brick_visits: 0,
        cache_brick_hits: 0,
        cache_brick_misses: 0,
        cache_interval_advances: 0,
        accepted_relaxed_steps: 0,
        rejected_relaxed_steps: 0,
        analytic_transformed_hits: 0,
        interval_subdivisions: 0,
        interval_proof_successes: 0,
        observer_continuation_seed_hits: 0,
        solver_relaxed_attempts: 0,
        solver_relaxed_no_root_advances: 0,
        solver_relaxed_brackets: 0,
        solver_relaxed_unresolved: 0,
        solver_interval_attempts: 0,
        solver_interval_no_root_advances: 0,
        solver_interval_brackets: 0,
        solver_interval_unresolved: 0,
        solver_refinement_attempts: 0,
        solver_refinement_failures: 0,
        solver_repeat_attempts: 0,
        solver_repeat_supported: 0,
        solver_repeat_inapplicable: 0,
        solver_repeat_unsupported: 0,
        solver_repeat_unsupported_form: 0,
        solver_repeat_unsupported_bounds: 0,
        solver_repeat_cells_enumerated: 0,
        field_samples,
        cpu_time_total_micros: 0,
        execution_bound: "cpu_wall_clock_only".to_string(),
        gpu_runtime: Default::default(),
        attachment_bytes: vec![],
        passes: vec![wrela::presentation_exec::PresentationPassCost {
            pass_id: "primary.visibility".to_string(),
            pass_kind: "primary_visibility".to_string(),
            work_items: 1024,
            elapsed_micros,
            gpu_elapsed_micros: None,
            dispatch_count: 1,
            attachment_bytes_read: 0,
            attachment_bytes_written: 8192,
            clipmap: None,
            notes: vec![],
        }],
        framegraph_exceptions: vec![],
        active_acceleration_artifacts: vec![],
        bottleneck_pass: Some("primary_visibility".to_string()),
        performance_gain_sources: vec!["backend_speed".to_string()],
    }
}

#[test]
fn closure_measured_presentation_frame_history_drops_cold_start_frame() {
    let cold = sample_presentation_frame_cost(64, 64, 1.0, 10, 2.0, 6, 4, 1_000);
    let warm = sample_presentation_frame_cost(32, 32, 0.5, 30, 4.0, 10, 7, 2_000);

    let effective = closure_measured_presentation_frame_history(&warm, &[cold, warm.clone()])
        .expect("stable closure history");

    assert_eq!(effective, vec![warm]);
}

#[test]
fn closure_measured_presentation_frame_history_keeps_trailing_stable_quality_suffix() {
    let mut warm_transition = sample_presentation_frame_cost(64, 64, 1.0, 30, 4.0, 10, 7, 9_500);
    warm_transition.quality.hit_compaction_enabled = true;
    warm_transition.quality.active_degradations = vec!["enable_hit_compaction".to_string()];
    warm_transition.gpu_runtime.pipeline_cache_misses = 1;

    let mut stable0 = warm_transition.clone();
    stable0.gpu_runtime.pipeline_cache_misses = 0;
    stable0.passes[0].elapsed_micros = 7_900;

    let mut stable1 = stable0.clone();
    stable1.passes[0].elapsed_micros = 7_800;

    let effective = closure_measured_presentation_frame_history(
        &stable1,
        &[
            sample_presentation_frame_cost(64, 64, 1.0, 10, 2.0, 6, 4, 1_000),
            warm_transition,
            stable0.clone(),
            stable1.clone(),
        ],
    )
    .expect("stable trailing closure history");

    assert_eq!(effective, vec![stable0, stable1]);
}

#[test]
fn closure_measured_presentation_frame_history_rejects_final_cache_miss_frame() {
    let mut unstable = sample_presentation_frame_cost(64, 64, 1.0, 30, 4.0, 10, 7, 9_500);
    unstable.quality.hit_compaction_enabled = true;
    unstable.quality.active_degradations = vec!["enable_hit_compaction".to_string()];
    unstable.gpu_runtime.pipeline_cache_misses = 2;

    let err = closure_measured_presentation_frame_history(&unstable, &[unstable.clone()])
        .expect_err("unstable closure frame should fail");

    assert!(err.contains("pipeline cache miss"));
}

#[test]
fn presentation_report_from_debug_output_carries_quality_and_pass_data() {
    let scenario = test_eval_perf::BenchmarkScenario {
        id: "presentation_fixture".into(),
        test_name: "tests/fixture::test_ops_64".to_string(),
        ops: 64,
        class: test_eval_perf::BenchmarkScenarioClass::Critical,
        min_runtime_ms: None,
        timeout_ms: None,
        allow_unstable: false,
        presentation: None,
        collision: None,
    };
    let dump = PresentationDebugCommandOutput {
            view: "bench_view".to_string(),
            region: "bench_region".to_string(),
            domain: "bench_domain".to_string(),
            backend: "cpu".to_string(),
            query_trace_solver_mode: "hybrid".to_string(),
            frames_executed: 2,
            frame_cost: wrela::presentation_exec::PresentationFrameCostReport {
                semantic_domain: "bench_domain".to_string(),
                execution_policy: "required=best-effort selected=heuristic-solver backend=cpu"
                    .to_string(),
                legal_degradations: vec!["reduce_internal_resolution".to_string()],
                output_width: 64,
                output_height: 64,
                internal_width: 32,
                internal_height: 32,
                field_samples: 512,
                cpu_time_total_micros: 0,
                execution_bound: "cpu_wall_clock_only".to_string(),
                gpu_runtime: Default::default(),
                quality: wrela::presentation_exec::PresentationQualityReport {
                    tier: "realtime_60".to_string(),
                    target_fps: 60,
                    output_width: 64,
                    output_height: 64,
                    internal_width: 32,
                    internal_height: 32,
                    internal_resolution_scale: 0.5,
                    achieved_native_output: false,
                    reconstructed_output: true,
                    temporal_mode: "TemporalAA".to_string(),
                    radiance_mode: "full".to_string(),
                    media_enabled: true,
                    half_res_participants: false,
                    hit_compaction_enabled: true,
                    active_degradations: vec!["reduce_internal_resolution".to_string()],
                },
                primary_hit_rate: 0.75,
                average_trace_steps: 12.0,
                max_trace_steps: 24,
                candidate_count_before_pruning: 100,
                candidate_count_after_pruning: 60,
                support_prune_effectiveness: 0.4,
                tile_cull_total_tiles: 16,
                tile_cull_active_tiles: 9,
                tile_cull_efficiency: 0.4375,
                tile_candidate_total_samples: 256,
                tile_candidate_active_samples: 128,
                tile_candidate_reduction: 128,
                tile_candidate_effectiveness: 0.5,
                tile_candidate_packet_count: 4,
                tile_candidate_packet_size: 32,
                packet_compaction_ratio: 1.0,
                packet_scheduling_active: true,
                selected_workgroup_size: 64,
                surface_resolve_count: 256,
                participant_resolve_count: 128,
                history_reuse_rate: 0.5,
                continuation_diagnostics: vec![
                    "continuation verdict=available reason=none change_class=stable accepted_change_class=camera-motion"
                        .to_string()
                ],
                acceleration_node_visits: 0,
                union_cluster_visits: 0,
                ray_support_interval_rejections: 0,
                ray_support_entry_jumps: 0,
                repeat_cell_skips: 0,
                cache_brick_visits: 0,
                cache_brick_hits: 0,
                cache_brick_misses: 0,
                cache_interval_advances: 0,
                accepted_relaxed_steps: 0,
                rejected_relaxed_steps: 0,
                analytic_transformed_hits: 0,
                interval_subdivisions: 0,
                interval_proof_successes: 0,
                observer_continuation_seed_hits: 0,
                solver_relaxed_attempts: 0,
                solver_relaxed_no_root_advances: 0,
                solver_relaxed_brackets: 0,
                solver_relaxed_unresolved: 0,
                solver_interval_attempts: 0,
                solver_interval_no_root_advances: 0,
                solver_interval_brackets: 0,
                solver_interval_unresolved: 0,
                solver_refinement_attempts: 0,
                solver_refinement_failures: 0,
                solver_repeat_attempts: 0,
                solver_repeat_supported: 0,
                solver_repeat_inapplicable: 0,
                solver_repeat_unsupported: 0,
                solver_repeat_unsupported_form: 0,
                solver_repeat_unsupported_bounds: 0,
                solver_repeat_cells_enumerated: 0,
                attachment_bytes: vec![wrela::presentation_exec::PresentationAttachmentBytes {
                    attachment: "color".to_string(),
                    width: 64,
                    height: 64,
                    total_size_bytes: 16384,
                    backing: "cpu_bytes".to_string(),
                }],
                passes: vec![wrela::presentation_exec::PresentationPassCost {
                    pass_id: "primary.visibility".to_string(),
                    pass_kind: "primary_visibility".to_string(),
                    work_items: 1024,
                    elapsed_micros: 3300,
                    gpu_elapsed_micros: None,
                    dispatch_count: 1,
                    attachment_bytes_read: 0,
                    attachment_bytes_written: 8192,
                    clipmap: None,
                    notes: vec!["dynamic_resolution".to_string()],
                }],
                framegraph_exceptions: vec![],
                active_acceleration_artifacts: vec![
                    "tile_candidate_table".to_string(),
                    "packet_scheduling".to_string(),
                ],
                bottleneck_pass: Some("primary_visibility".to_string()),
                performance_gain_sources: vec![
                    "support_pruning".to_string(),
                    "tile_culling".to_string(),
                    "tile_candidate_table".to_string(),
                    "packet_scheduling".to_string(),
                    "quality_degradation_active".to_string(),
                ],
            },
            frame_cost_history: vec![
                wrela::presentation_exec::PresentationFrameCostReport {
                    semantic_domain: "bench_domain".to_string(),
                    execution_policy: "required=best-effort selected=heuristic-solver backend=cpu"
                        .to_string(),
                    legal_degradations: vec![],
                    output_width: 64,
                    output_height: 64,
                    internal_width: 64,
                    internal_height: 64,
                    field_samples: 512,
                    cpu_time_total_micros: 0,
                    execution_bound: "cpu_wall_clock_only".to_string(),
                    gpu_runtime: Default::default(),
                    quality: wrela::presentation_exec::PresentationQualityReport {
                        tier: "realtime_60".to_string(),
                        target_fps: 60,
                        output_width: 64,
                        output_height: 64,
                        internal_width: 64,
                        internal_height: 64,
                        internal_resolution_scale: 1.0,
                        achieved_native_output: true,
                        reconstructed_output: false,
                        temporal_mode: "TemporalAA".to_string(),
                        radiance_mode: "full".to_string(),
                        media_enabled: true,
                        half_res_participants: false,
                        hit_compaction_enabled: false,
                        active_degradations: vec![],
                    },
                    primary_hit_rate: 0.8,
                    average_trace_steps: 14.0,
                    max_trace_steps: 24,
                    candidate_count_before_pruning: 100,
                    candidate_count_after_pruning: 60,
                    support_prune_effectiveness: 0.4,
                    tile_cull_total_tiles: 16,
                    tile_cull_active_tiles: 9,
                    tile_cull_efficiency: 0.4375,
                    tile_candidate_total_samples: 256,
                    tile_candidate_active_samples: 256,
                    tile_candidate_reduction: 0,
                    tile_candidate_effectiveness: 0.0,
                    tile_candidate_packet_count: 1,
                    tile_candidate_packet_size: 256,
                    packet_compaction_ratio: 1.0,
                    packet_scheduling_active: false,
                    selected_workgroup_size: 0,
                    surface_resolve_count: 256,
                    participant_resolve_count: 128,
                    history_reuse_rate: 0.0,
                    continuation_diagnostics: vec![],
                    acceleration_node_visits: 0,
                    union_cluster_visits: 0,
                    ray_support_interval_rejections: 0,
                    ray_support_entry_jumps: 0,
                    repeat_cell_skips: 0,
                    cache_brick_visits: 0,
                    cache_brick_hits: 0,
                    cache_brick_misses: 0,
                    cache_interval_advances: 0,
                    accepted_relaxed_steps: 0,
                    rejected_relaxed_steps: 0,
                    analytic_transformed_hits: 0,
                    interval_subdivisions: 0,
                    interval_proof_successes: 0,
                    observer_continuation_seed_hits: 0,
                    solver_relaxed_attempts: 0,
                    solver_relaxed_no_root_advances: 0,
                    solver_relaxed_brackets: 0,
                    solver_relaxed_unresolved: 0,
                    solver_interval_attempts: 0,
                    solver_interval_no_root_advances: 0,
                    solver_interval_brackets: 0,
                    solver_interval_unresolved: 0,
                    solver_refinement_attempts: 0,
                    solver_refinement_failures: 0,
                    solver_repeat_attempts: 0,
                    solver_repeat_supported: 0,
                    solver_repeat_inapplicable: 0,
                    solver_repeat_unsupported: 0,
                    solver_repeat_unsupported_form: 0,
                    solver_repeat_unsupported_bounds: 0,
                    solver_repeat_cells_enumerated: 0,
                    attachment_bytes: vec![],
                    passes: vec![wrela::presentation_exec::PresentationPassCost {
                        pass_id: "primary.visibility".to_string(),
                        pass_kind: "primary_visibility".to_string(),
                        work_items: 1024,
                        elapsed_micros: 1200,
                        gpu_elapsed_micros: None,
                        dispatch_count: 1,
                        attachment_bytes_read: 0,
                        attachment_bytes_written: 4096,
                        clipmap: None,
                        notes: vec![],
                    }],
                    framegraph_exceptions: vec![],
                    active_acceleration_artifacts: vec![],
                    bottleneck_pass: Some("primary_visibility".to_string()),
                    performance_gain_sources: vec!["backend_speed".to_string()],
                },
                wrela::presentation_exec::PresentationFrameCostReport {
                    semantic_domain: "bench_domain".to_string(),
                    execution_policy: "required=best-effort selected=heuristic-solver backend=cpu"
                        .to_string(),
                    legal_degradations: vec!["reduce_internal_resolution".to_string()],
                    output_width: 64,
                    output_height: 64,
                    internal_width: 32,
                    internal_height: 32,
                    field_samples: 512,
                    cpu_time_total_micros: 0,
                    execution_bound: "cpu_wall_clock_only".to_string(),
                    gpu_runtime: Default::default(),
                    quality: wrela::presentation_exec::PresentationQualityReport {
                        tier: "realtime_60".to_string(),
                        target_fps: 60,
                        output_width: 64,
                        output_height: 64,
                        internal_width: 32,
                        internal_height: 32,
                        internal_resolution_scale: 0.5,
                        achieved_native_output: false,
                        reconstructed_output: true,
                        temporal_mode: "TemporalAA".to_string(),
                        radiance_mode: "full".to_string(),
                        media_enabled: true,
                        half_res_participants: false,
                        hit_compaction_enabled: true,
                        active_degradations: vec!["reduce_internal_resolution".to_string()],
                    },
                    primary_hit_rate: 0.75,
                    average_trace_steps: 12.0,
                    max_trace_steps: 24,
                    candidate_count_before_pruning: 100,
                    candidate_count_after_pruning: 60,
                    support_prune_effectiveness: 0.4,
                    tile_cull_total_tiles: 16,
                    tile_cull_active_tiles: 9,
                    tile_cull_efficiency: 0.4375,
                    tile_candidate_total_samples: 256,
                    tile_candidate_active_samples: 256,
                    tile_candidate_reduction: 0,
                    tile_candidate_effectiveness: 0.0,
                    tile_candidate_packet_count: 1,
                    tile_candidate_packet_size: 256,
                    packet_compaction_ratio: 1.0,
                    packet_scheduling_active: false,
                    selected_workgroup_size: 0,
                    surface_resolve_count: 256,
                    participant_resolve_count: 128,
                    history_reuse_rate: 0.25,
                    continuation_diagnostics: vec![],
                    acceleration_node_visits: 0,
                    union_cluster_visits: 0,
                    ray_support_interval_rejections: 0,
                    ray_support_entry_jumps: 0,
                    repeat_cell_skips: 0,
                    cache_brick_visits: 0,
                    cache_brick_hits: 0,
                    cache_brick_misses: 0,
                    cache_interval_advances: 0,
                    accepted_relaxed_steps: 0,
                    rejected_relaxed_steps: 0,
                    analytic_transformed_hits: 0,
                    interval_subdivisions: 0,
                    interval_proof_successes: 0,
                    observer_continuation_seed_hits: 0,
                    solver_relaxed_attempts: 0,
                    solver_relaxed_no_root_advances: 0,
                    solver_relaxed_brackets: 0,
                    solver_relaxed_unresolved: 0,
                    solver_interval_attempts: 0,
                    solver_interval_no_root_advances: 0,
                    solver_interval_brackets: 0,
                    solver_interval_unresolved: 0,
                    solver_refinement_attempts: 0,
                    solver_refinement_failures: 0,
                    solver_repeat_attempts: 0,
                    solver_repeat_supported: 0,
                    solver_repeat_inapplicable: 0,
                    solver_repeat_unsupported: 0,
                    solver_repeat_unsupported_form: 0,
                    solver_repeat_unsupported_bounds: 0,
                    solver_repeat_cells_enumerated: 0,
                    attachment_bytes: vec![],
                    passes: vec![wrela::presentation_exec::PresentationPassCost {
                        pass_id: "primary.visibility".to_string(),
                        pass_kind: "primary_visibility".to_string(),
                        work_items: 1024,
                        elapsed_micros: 2100,
                        gpu_elapsed_micros: None,
                        dispatch_count: 1,
                        attachment_bytes_read: 0,
                        attachment_bytes_written: 8192,
                        clipmap: None,
                        notes: vec!["dynamic_resolution".to_string()],
                    }],
                    framegraph_exceptions: vec![],
                    active_acceleration_artifacts: vec![
                        "tile_candidate_table".to_string(),
                        "packet_scheduling".to_string(),
                    ],
                    bottleneck_pass: Some("primary_visibility".to_string()),
                    performance_gain_sources: vec![
                        "support_pruning".to_string(),
                        "tile_culling".to_string(),
                        "tile_candidate_table".to_string(),
                        "packet_scheduling".to_string(),
                        "quality_degradation_active".to_string(),
                    ],
                },
            ],
        };

    let report = presentation_report_from_debug_output(&scenario, dump)
        .expect("non-closure report should parse");
    assert_eq!(report.scenario_id, "presentation_fixture");
    assert_eq!(report.frames_executed, 2);
    assert_eq!(report.frame_time_ns, 3_300_000);
    assert!((report.steady_state_fps - (2.0 / 0.0033)).abs() < 0.01);
    assert_eq!(report.field_samples, 1024);
    assert_eq!(report.query_trace_solver_mode, "hybrid");
    assert_eq!(report.selected_workgroup_size, 64);
    assert_eq!(report.quality_tier, "realtime_60");
    assert_eq!(report.internal_resolution_scale, 0.5);
    assert!(report.reconstructed_output);
    assert_eq!(report.internal_resolution_history, vec![1.0, 0.5]);
    assert_eq!(
        report.bottleneck_pass.as_deref(),
        Some("primary_visibility")
    );
    assert!(report.wgsl_workgroup_comparison.is_none());
    assert_eq!(report.frame_cost.passes.len(), 1);
    assert_eq!(report.frame_cost.passes[0].pass_kind, "primary_visibility");
}

#[test]
fn presentation_report_from_debug_output_drops_cold_start_frame_for_closure_scenarios() {
    let scenario = test_eval_perf::BenchmarkScenario {
        id: "closure_fixture".into(),
        test_name: "tests/fixture::closure_ops_64".to_string(),
        ops: 64,
        class: test_eval_perf::BenchmarkScenarioClass::Closure,
        min_runtime_ms: None,
        timeout_ms: None,
        allow_unstable: false,
        presentation: None,
        collision: None,
    };
    let warm = sample_presentation_frame_cost(32, 32, 0.5, 30, 4.0, 10, 7, 2_000);
    let dump = PresentationDebugCommandOutput {
        view: "bench_view".to_string(),
        region: "bench_region".to_string(),
        domain: "bench_domain".to_string(),
        backend: "cpu".to_string(),
        query_trace_solver_mode: "hybrid".to_string(),
        frames_executed: 2,
        frame_cost: warm.clone(),
        frame_cost_history: vec![
            sample_presentation_frame_cost(64, 64, 1.0, 10, 2.0, 6, 4, 1_000),
            warm.clone(),
        ],
    };

    let report = presentation_report_from_debug_output(&scenario, dump)
        .expect("closure report should parse");

    assert_eq!(report.frames_executed, 1);
    assert_eq!(report.frame_time_ns, 2_000_000);
    assert!((report.steady_state_fps - 500.0).abs() < f64::EPSILON);
    assert_eq!(report.field_samples, 30);
    assert_eq!(report.internal_resolution_history, vec![0.5]);
    assert_eq!(report.frame_cost_history, vec![warm]);
}

#[test]
fn closure_profile_defaults_to_measurement_collection_until_why_not_is_requested() {
    assert_eq!(
        presentation_benchmark_collection_mode(PerfProfile::Closure1080p120, false),
        PresentationBenchmarkCollectionMode::Measurement
    );
    assert_eq!(
        presentation_benchmark_collection_mode(PerfProfile::Closure1080p120, true),
        PresentationBenchmarkCollectionMode::Diagnostic
    );
    assert_eq!(
        presentation_benchmark_collection_mode(PerfProfile::Standard, false),
        PresentationBenchmarkCollectionMode::Diagnostic
    );
}

#[test]
fn canonical_1080p120_auto_backend_resolves_to_wgsl() {
    assert_eq!(
        effective_perf_query_backend(
            PerfProfile::Closure1080p120,
            wrela::query_plan::DispatchBackend::Auto,
        ),
        wrela::query_plan::DispatchBackend::Wgsl
    );
}

#[test]
fn nonclosure_auto_backend_preserves_auto_selection() {
    assert_eq!(
        effective_perf_query_backend(
            PerfProfile::Standard,
            wrela::query_plan::DispatchBackend::Auto,
        ),
        wrela::query_plan::DispatchBackend::Auto
    );
}

#[test]
fn presentation_comparison_aggregates_multi_frame_solver_metrics() {
    let scenario = test_eval_perf::BenchmarkScenario {
        id: "presentation_fixture".into(),
        test_name: "tests/fixture::test_ops_64".to_string(),
        ops: 64,
        class: test_eval_perf::BenchmarkScenarioClass::Critical,
        min_runtime_ms: None,
        timeout_ms: None,
        allow_unstable: false,
        presentation: None,
        collision: None,
    };
    let hybrid_dump = PresentationDebugCommandOutput {
        view: "bench_view".to_string(),
        region: "bench_region".to_string(),
        domain: "bench_domain".to_string(),
        backend: "cpu".to_string(),
        query_trace_solver_mode: "hybrid".to_string(),
        frames_executed: 2,
        frame_cost: sample_presentation_frame_cost(32, 32, 0.5, 30, 4.0, 10, 7, 2_000),
        frame_cost_history: vec![
            sample_presentation_frame_cost(64, 64, 1.0, 10, 2.0, 6, 4, 1_000),
            sample_presentation_frame_cost(32, 32, 0.5, 30, 4.0, 10, 7, 2_000),
        ],
    };
    let dense_only_dump = PresentationDebugCommandOutput {
        query_trace_solver_mode: "dense-only".to_string(),
        frame_cost: sample_presentation_frame_cost(32, 32, 0.5, 40, 5.0, 12, 9, 2_500),
        frame_cost_history: vec![
            sample_presentation_frame_cost(64, 64, 1.0, 20, 3.0, 8, 6, 1_500),
            sample_presentation_frame_cost(32, 32, 0.5, 40, 5.0, 12, 9, 2_500),
        ],
        ..hybrid_dump.clone()
    };

    let hybrid_report = presentation_report_from_debug_output(&scenario, hybrid_dump)
        .expect("hybrid report should parse");
    let comparison = presentation_comparison_from_debug_reports(&hybrid_report, &dense_only_dump);

    assert_eq!(hybrid_report.field_samples, 40);
    assert_eq!(comparison.dense_only_field_samples, 60);
    assert_eq!(comparison.field_samples_delta_vs_dense_only, -20);
    assert_eq!(comparison.dense_only_candidate_count_before_pruning, 20);
    assert_eq!(
        comparison.candidate_count_before_pruning_delta_vs_dense_only,
        -4
    );
    assert_eq!(comparison.dense_only_candidate_count_after_pruning, 15);
    assert_eq!(
        comparison.candidate_count_after_pruning_delta_vs_dense_only,
        -4
    );
    assert!((comparison.dense_only_average_trace_steps - 4.0).abs() < f32::EPSILON);
    assert!((comparison.average_trace_steps_delta_vs_dense_only + 1.0).abs() < f32::EPSILON);
    assert_eq!(comparison.dense_only_frame_time_ns, 4_000_000);
    assert_eq!(comparison.frame_time_ns_delta_vs_dense_only, -1_000_000);
}

#[test]
fn presentation_workgroup_comparison_tracks_candidate_deltas() {
    let make_report = |workgroup_size: u32, frame_time_ns: u128| PresentationBenchmarkReport {
        scenario_id: "scenario".into(),
        test_name: "tests/fixture".to_string(),
        view: "bench_view".to_string(),
        region: "bench_region".to_string(),
        domain: "bench_domain".to_string(),
        backend: "wgsl".to_string(),
        observed_adapter_name: None,
        query_trace_solver_mode: "hybrid".to_string(),
        selected_workgroup_size: workgroup_size,
        frames_executed: 1,
        frame_time_ns,
        steady_state_fps: fps_from_frame_time_ns(frame_time_ns, 1),
        field_samples: 512,
        quality_tier: "realtime_120".to_string(),
        target_fps: 120,
        internal_resolution_scale: 1.0,
        reconstructed_output: false,
        quality_history: vec!["realtime_120".to_string()],
        internal_resolution_history: vec![1.0],
        bottleneck_pass: Some("primary_visibility".to_string()),
        active_acceleration_artifacts: vec!["packet_scheduling".to_string()],
        performance_gain_sources: vec!["packet_scheduling".to_string()],
        frame_cost: sample_presentation_frame_cost(64, 64, 1.0, 512, 4.0, 10, 8, 1_000),
        frame_cost_history: vec![],
        wgsl_workgroup_comparison: None,
        ab_comparison: None,
    };
    let reports = vec![
        make_report(32, 7_500_000),
        make_report(64, 6_000_000),
        make_report(128, 6_500_000),
    ];
    let comparison = presentation_workgroup_comparison_from_reports(&reports, &reports[1]);
    assert_eq!(comparison.selected_workgroup_size, 64);
    assert_eq!(comparison.candidate_workgroup_sizes, vec![32, 64, 128]);
    assert_eq!(
        comparison.frame_time_ns_delta_vs_selected,
        vec![1_500_000, 0, 500_000]
    );
    assert_eq!(
        format_workgroup_comparison(&comparison),
        "32:7500000ns(+25.00%) 64:6000000ns(+0.00%) 128:6500000ns(+8.33%)"
    );
}
