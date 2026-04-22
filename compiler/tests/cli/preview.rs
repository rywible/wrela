use super::*;

#[test]
fn cli_frame_contracts_reports_named_view_contracts() {
    let temp = workspace_tempdir();
    write_presentation_plan_fixture(temp.path());

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("frame-contracts")
        .arg(temp.path())
        .output()
        .expect("run frame-contracts");
    assert!(
        output.status.success(),
        "frame-contracts failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("frame contracts schema v1"));
    assert!(stdout.contains("view cli_plan_view"));
    assert!(stdout.contains("view cli_plan_fast_view"));
    assert!(stdout.contains("temporal reuse: ReprojectColorAndMotion"));
    assert!(stdout.contains("temporal change class: camera-motion"));
    assert!(stdout.contains("temporal reuse: Disabled"));
    assert!(stdout.contains("motion.resolve recipe=MotionResolve"));
    assert!(stdout.contains("composite.color recipe=CompositeColor"));
}

#[test]
fn cli_preview_exports_selected_attachment_ppm() {
    let temp = workspace_tempdir();
    write_presentation_plan_fixture(temp.path());

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("preview")
        .arg(temp.path())
        .arg("--view")
        .arg("cli_plan_view")
        .arg("--attachment")
        .arg("depth")
        .output()
        .expect("run preview");
    assert!(
        output.status.success(),
        "preview failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.starts_with("P3\n2 2\n255\n"));
}

#[test]
fn cli_preview_json_report_summarizes_execution() {
    let temp = workspace_tempdir();
    write_presentation_plan_fixture(temp.path());

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("preview")
        .arg(temp.path())
        .arg("--view")
        .arg("cli_plan_view")
        .arg("--json-report")
        .arg("--json")
        .output()
        .expect("run preview report");
    assert!(
        output.status.success(),
        "preview --json-report failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let dump: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("preview report json");
    assert_eq!(
        dump.get("schema_version").and_then(|value| value.as_u64()),
        Some(1)
    );
    assert_eq!(
        dump.get("view").and_then(|value| value.as_str()),
        Some("cli_plan_view")
    );
    assert_eq!(
        dump.get("backend").and_then(|value| value.as_str()),
        Some("cpu")
    );
    assert_eq!(
        dump.pointer("/snapshot/capture_name")
            .and_then(|value| value.as_str()),
        Some("cli_plan_region")
    );
    assert_eq!(
        dump.pointer("/snapshot/epoch")
            .and_then(|value| value.as_u64()),
        Some(1)
    );
    assert!(
        dump.get("stats")
            .and_then(|value| value.as_str())
            .is_some_and(|stats| stats.contains("quality tier=realtime_120"))
    );
}

#[test]
fn cli_frame_json_reports_typed_attachments() {
    let temp = workspace_tempdir();
    write_presentation_plan_fixture(temp.path());

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("frame")
        .arg(temp.path())
        .arg("--view")
        .arg("cli_plan_view")
        .arg("--attachment")
        .arg("color")
        .arg("--attachment")
        .arg("depth")
        .arg("--json")
        .output()
        .expect("run frame");
    assert!(
        output.status.success(),
        "frame --json failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let dump: serde_json::Value = serde_json::from_slice(&output.stdout).expect("frame json");
    assert_eq!(
        dump.get("schema_version").and_then(|value| value.as_u64()),
        Some(1)
    );
    assert_eq!(
        dump.get("view").and_then(|value| value.as_str()),
        Some("cli_plan_view")
    );
    assert_eq!(
        dump.pointer("/snapshot/capture_name")
            .and_then(|value| value.as_str()),
        Some("cli_plan_region")
    );
    assert_eq!(
        dump.pointer("/snapshot/epoch")
            .and_then(|value| value.as_u64()),
        Some(1)
    );
    let attachments = dump
        .get("attachments")
        .and_then(|value| value.as_array())
        .expect("attachments array");
    assert_eq!(attachments.len(), 2);
    assert!(attachments.iter().any(|attachment| {
        attachment.get("name").and_then(|value| value.as_str()) == Some("color")
            && attachment.get("kind").and_then(|value| value.as_str()) == Some("Color")
    }));
    assert!(attachments.iter().any(|attachment| {
        attachment.get("name").and_then(|value| value.as_str()) == Some("depth")
            && attachment
                .pointer("/element_schema/kind")
                .and_then(|value| value.as_str())
                == Some("scalar_f32")
    }));
    assert_eq!(
        dump.pointer("/frame_cost/quality/tier")
            .and_then(|value| value.as_str()),
        Some("realtime_120")
    );
}

#[test]
fn cli_frame_ppm_exports_selected_attachment() {
    let temp = workspace_tempdir();
    write_presentation_plan_fixture(temp.path());

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("frame")
        .arg(temp.path())
        .arg("--view")
        .arg("cli_plan_view")
        .arg("--attachment")
        .arg("depth")
        .arg("--attachment-format=ppm")
        .output()
        .expect("run frame ppm");
    assert!(
        output.status.success(),
        "frame ppm failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.starts_with("P3\n2 2\n255\n"));
}

#[test]
fn cli_frame_live_headless_emits_selection_json() {
    let temp = workspace_tempdir();
    write_presentation_plan_fixture(temp.path());

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_wrela"));
    cmd.arg("frame-live")
        .arg(temp.path())
        .arg("--view")
        .arg("cli_plan_view")
        .arg("--width")
        .arg("8")
        .arg("--height")
        .arg("8")
        .arg("--json")
        .env("WRELA_FRAME_LIVE_HEADLESS", "1");
    let output = run_command_with_timeout(&mut cmd, Duration::from_secs(10));
    assert!(
        output.status.success(),
        "frame-live headless failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let record: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("frame-live selection json");
    assert_eq!(
        record.get("generation").and_then(|value| value.as_u64()),
        Some(1)
    );
    assert_eq!(
        record.get("hit").and_then(|value| value.as_bool()),
        Some(true)
    );
    assert_eq!(
        record.get("field_name").and_then(|value| value.as_str()),
        Some("cli_plan_field")
    );
    assert_eq!(
        record
            .pointer("/primary_source/kind")
            .and_then(|value| value.as_str()),
        Some("field")
    );
}

#[test]
fn cli_check_hard_errors_legacy_render_declarations() {
    let temp = workspace_tempdir();
    let src_dir = temp.path().join("src");
    std::fs::create_dir_all(&src_dir).expect("create src dir");
    let entry = src_dir.join("main.wr");
    write_fixture_file(
        &entry,
        r#"
render legacy_preview(world: RegionCapture, camera: Camera) {
    width = 2
    height = 2
}
"#,
    )
    .expect("write legacy render fixture");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg(temp.path())
        .output()
        .expect("run check");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("legacy render declaration"));
    assert!(stderr.contains("Rewrite this authored surface as `view`"));
}

#[test]
fn cli_presentation_debug_exports_depth_normal_and_stats() {
    let temp = workspace_tempdir();
    write_presentation_plan_fixture(temp.path());
    let out_dir = temp.path().join("presentation-debug-output");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("presentation-debug")
        .arg(temp.path())
        .arg("--view")
        .arg("cli_plan_view")
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("--width")
        .arg("4")
        .arg("--height")
        .arg("4")
        .output()
        .expect("run presentation-debug");
    assert!(
        output.status.success(),
        "presentation-debug failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("presentation debug schema v1"));
    assert!(stdout.contains("presentation debug view=cli_plan_view backend=cpu"));
    assert!(stdout.contains("query trace solver mode: hybrid"));
    assert!(stdout.contains("snapshot_id="));
    assert!(stdout.contains("epoch=1"));
    assert!(stdout.contains("field samples:"));
    assert!(stdout.contains("color ppm:"));
    assert!(stdout.contains("depth ppm:"));
    assert!(stdout.contains("world normal ppm:"));
    assert!(stdout.contains("semantic domain:"));
    assert!(stdout.contains("execution policy:"));
    assert!(stdout.contains("required_guarantee=conservative_no_false_miss"));
    assert!(stdout.contains("selected_method=conservative_solver"));
    assert!(stdout.contains("hit_rate="));
    assert!(stdout.contains("quality tier=realtime_120"));
    assert!(stdout.contains("continuation_diagnostics="));
    assert!(
        stdout.contains("change_class=camera-motion") || stdout.contains("change_class=stable")
    );
    assert!(out_dir.join("color.ppm").exists());
    assert!(out_dir.join("depth.ppm").exists());
    assert!(out_dir.join("world_normal.ppm").exists());
    assert!(out_dir.join("stats.txt").exists());

    let stats = std::fs::read_to_string(out_dir.join("stats.txt")).expect("read stats");
    assert!(stats.contains("samples=16"));
    assert!(stats.contains("solver=ray-solver:spatial.nearest.batch.world:v1"));
    assert!(stats.contains("quality tier=realtime_120"));
    assert!(stats.contains("passes:"));
}

#[test]
fn cli_presentation_debug_json_reports_frame_cost_and_quality() {
    let temp = workspace_tempdir();
    write_presentation_plan_fixture(temp.path());

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("presentation-debug")
        .arg(temp.path())
        .arg("--view")
        .arg("cli_plan_view")
        .arg("--width")
        .arg("4")
        .arg("--height")
        .arg("4")
        .arg("--json")
        .output()
        .expect("run presentation-debug json");
    assert!(
        output.status.success(),
        "presentation-debug --json failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let dump: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("presentation-debug json");
    assert_eq!(
        dump.get("schema_version").and_then(|value| value.as_u64()),
        Some(1)
    );
    assert_eq!(
        dump.get("query_trace_solver_mode")
            .and_then(|value| value.as_str()),
        Some("hybrid")
    );
    assert_eq!(
        dump.pointer("/frame_cost/quality/tier")
            .and_then(|value| value.as_str()),
        Some("realtime_120")
    );
    assert!(
        dump.pointer("/frame_cost/continuation_diagnostics")
            .and_then(|value| value.as_array())
            .is_some_and(|entries| !entries.is_empty())
    );
    assert_eq!(
        dump.pointer("/frame_cost/quality/target_fps")
            .and_then(|value| value.as_u64()),
        Some(120)
    );
    assert_eq!(
        dump.get("semantic_domain")
            .and_then(|value| value.as_str())
            .is_some_and(|value| value.contains("geometry_detail=1")),
        true
    );
    assert_eq!(
        dump.get("execution_policy")
            .and_then(|value| value.as_str())
            .is_some_and(|value| value.contains("backend=cpu")
                && value.contains("required_guarantee=conservative_no_false_miss")
                && value.contains("selected_method=conservative_solver")),
        true
    );
    assert_eq!(
        dump.pointer("/snapshot/capture_name")
            .and_then(|value| value.as_str()),
        Some("cli_plan_region")
    );
    assert_eq!(
        dump.pointer("/snapshot/epoch")
            .and_then(|value| value.as_u64()),
        Some(1)
    );
    assert!(
        dump.pointer("/frame_cost/passes")
            .and_then(|value| value.as_array())
            .is_some_and(|passes| {
                passes.iter().any(|pass| {
                    pass.get("pass_kind")
                        .and_then(|value| value.as_str())
                        .is_some_and(|kind| kind == "primary_visibility")
                })
            })
    );
    assert!(
        dump.pointer("/frame_cost/solver_relaxed_attempts")
            .and_then(|value| value.as_u64())
            .is_some()
    );
    assert_eq!(
        dump.pointer("/frame_cost/solver_repeat_cells_enumerated")
            .and_then(|value| value.as_u64()),
        Some(0)
    );
    assert!(
        dump.get("stats")
            .and_then(|value| value.as_str())
            .is_some_and(|stats| {
                stats.contains("quality tier=realtime_120")
                    && stats.contains("solver_relaxed_attempts=")
                    && stats.contains("solver_repeat_attempts=")
            })
    );
}

#[test]
fn cli_presentation_debug_json_accepts_dense_only_solver_mode() {
    let temp = workspace_tempdir();
    write_presentation_plan_fixture(temp.path());

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("presentation-debug")
        .arg(temp.path())
        .arg("--view")
        .arg("cli_plan_view")
        .arg("--width")
        .arg("4")
        .arg("--height")
        .arg("4")
        .arg("--solver-mode")
        .arg("dense-only")
        .arg("--json")
        .output()
        .expect("run presentation-debug json dense-only");
    assert!(
        output.status.success(),
        "presentation-debug --solver-mode dense-only --json failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let dump: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("presentation-debug dense-only json");
    assert_eq!(
        dump.get("query_trace_solver_mode")
            .and_then(|value| value.as_str()),
        Some("dense-only")
    );
    assert!(
        dump.pointer("/frame_cost/field_samples")
            .and_then(|value| value.as_u64())
            .is_some_and(|samples| samples > 0)
    );
}

#[test]
fn cli_presentation_debug_handles_missing_optional_exports() {
    let temp = workspace_tempdir();
    write_presentation_plan_fixture(temp.path());
    let out_dir = temp.path().join("presentation-debug-fast-view");

    let seeded_output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("presentation-debug")
        .arg(temp.path())
        .arg("--view")
        .arg("cli_plan_view")
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("--width")
        .arg("4")
        .arg("--height")
        .arg("4")
        .output()
        .expect("seed presentation-debug output dir");
    assert!(
        seeded_output.status.success(),
        "presentation-debug seed failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&seeded_output.stdout),
        String::from_utf8_lossy(&seeded_output.stderr)
    );
    assert!(out_dir.join("depth.ppm").exists());
    assert!(out_dir.join("world_normal.ppm").exists());

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("presentation-debug")
        .arg(temp.path())
        .arg("--view")
        .arg("cli_plan_fast_view")
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("--width")
        .arg("4")
        .arg("--height")
        .arg("4")
        .output()
        .expect("run presentation-debug");
    assert!(
        output.status.success(),
        "presentation-debug fast view failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("presentation debug schema v1"));
    assert!(stdout.contains("presentation debug view=cli_plan_fast_view backend=cpu"));
    assert!(stdout.contains("color ppm:"));
    assert!(stdout.contains("depth ppm: not materialized"));
    assert!(stdout.contains("world normal ppm: not materialized"));
    assert!(out_dir.join("color.ppm").exists());
    assert!(!out_dir.join("depth.ppm").exists());
    assert!(!out_dir.join("world_normal.ppm").exists());
    assert!(out_dir.join("stats.txt").exists());
}

#[test]
fn cli_presentation_debug_json_reports_null_optional_exports() {
    let temp = workspace_tempdir();
    write_presentation_plan_fixture(temp.path());

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("presentation-debug")
        .arg(temp.path())
        .arg("--view")
        .arg("cli_plan_fast_view")
        .arg("--width")
        .arg("4")
        .arg("--height")
        .arg("4")
        .arg("--json")
        .output()
        .expect("run presentation-debug json");
    assert!(
        output.status.success(),
        "presentation-debug fast view --json failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let dump: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("presentation-debug fast view json");
    assert_eq!(
        dump.get("schema_version").and_then(|value| value.as_u64()),
        Some(1)
    );
    assert_eq!(
        dump.get("color_ppm"),
        Some(&serde_json::Value::String(
            temp.path()
                .join("src")
                .join("presentation_debug")
                .join("cli_plan_fast_view")
                .join("color.ppm")
                .display()
                .to_string()
        ))
    );
    assert_eq!(dump.get("depth_ppm"), Some(&serde_json::Value::Null));
    assert_eq!(dump.get("world_normal_ppm"), Some(&serde_json::Value::Null));
}

#[test]
fn cli_presentation_debug_no_export_skips_debug_artifacts() {
    let temp = workspace_tempdir();
    write_presentation_plan_fixture(temp.path());
    let default_out_dir = temp
        .path()
        .join("src")
        .join("presentation_debug")
        .join("cli_plan_view");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("presentation-debug")
        .arg(temp.path())
        .arg("--view")
        .arg("cli_plan_view")
        .arg("--width")
        .arg("4")
        .arg("--height")
        .arg("4")
        .arg("--no-export")
        .arg("--json")
        .output()
        .expect("run presentation-debug no-export json");
    assert!(
        output.status.success(),
        "presentation-debug --no-export --json failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let dump: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("presentation-debug no-export json");
    assert_eq!(dump.get("color_ppm"), Some(&serde_json::Value::Null));
    assert_eq!(dump.get("depth_ppm"), Some(&serde_json::Value::Null));
    assert_eq!(dump.get("world_normal_ppm"), Some(&serde_json::Value::Null));
    assert_eq!(
        dump.get("stats_path"),
        Some(&serde_json::Value::String("<not exported>".to_string()))
    );
    assert!(
        dump.get("stats")
            .and_then(|value| value.as_str())
            .is_some_and(|stats| stats.contains("quality tier=realtime_120"))
    );
    assert!(
        !default_out_dir.exists(),
        "presentation-debug --no-export should not materialize default debug artifacts"
    );
}

#[test]
fn cli_presentation_debug_rejects_non_literal_view_dimensions_without_override() {
    let temp = workspace_tempdir();
    write_presentation_debug_expression_fixture(temp.path());

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("presentation-debug")
        .arg(temp.path())
        .arg("--view")
        .arg("expr_view")
        .output()
        .expect("run presentation-debug");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("cannot evaluate non-literal view width"));
}

#[test]
fn cli_presentation_debug_accepts_non_literal_domain_budget_via_policy() {
    let temp = workspace_tempdir();
    write_presentation_debug_expression_fixture(temp.path());

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("presentation-debug")
        .arg(temp.path())
        .arg("--view")
        .arg("expr_view")
        .arg("--width")
        .arg("4")
        .arg("--height")
        .arg("4")
        .arg("--json")
        .output()
        .expect("run presentation-debug");
    assert!(
        output.status.success(),
        "presentation-debug should accept non-literal domain budgets via policy: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let dump: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("presentation-debug json");
    assert!(
        dump.get("semantic_domain")
            .and_then(|value| value.as_str())
            .is_some_and(|value| value.contains("geometry_detail=1"))
    );
    assert!(
        dump.get("execution_policy")
            .and_then(|value| value.as_str())
            .is_some_and(|value| {
                value.contains("required_guarantee=conservative_no_false_miss")
                    && value.contains("selected_method=conservative_solver")
                    && value.contains("primary_rays=max_distance=8")
            })
    );
}
