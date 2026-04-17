use super::*;

#[test]
fn cli_test_maintenance_flags_are_test_only() {
    let dir = workspace_tempdir();
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
    let dir = workspace_tempdir();
    write_test_project(dir.path());

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_wrela"));
    cmd.current_dir(dir.path())
        .arg("test")
        .arg("--record")
        .arg(".");
    apply_fast_cert_budgets(&mut cmd);
    let output = run_command_with_timeout(&mut cmd, Duration::from_secs(120));

    assert!(output.status.success(), "{:?}", output.status.code());
    assert!(
        !dir.path().join("wrela.out").exists(),
        "maintenance mode should not emit a native binary"
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
        json.get("binary_artifacts_emitted")
            .and_then(|v| v.as_bool()),
        Some(false)
    );
}

#[test]
fn cli_test_record_mode_writes_http_cassette_and_replay_passes_without_server() {
    let dir = workspace_tempdir();
    let (url, server) = spawn_http_stub_once("pong");
    write_http_integration_test_project(dir.path(), &url);

    let mut record_cmd = Command::new(env!("CARGO_BIN_EXE_wrela"));
    record_cmd
        .current_dir(dir.path())
        .arg("test")
        .arg("--record")
        .arg(".");
    apply_fast_cert_budgets(&mut record_cmd);
    let record_output = run_command_with_timeout(&mut record_cmd, Duration::from_secs(120));
    assert!(
        record_output.status.success(),
        "{}",
        String::from_utf8_lossy(&record_output.stderr)
    );
    server.join().expect("join server");

    let cassette_dir = dir.path().join("tests").join("cassettes");
    let mut files = Vec::new();
    for _ in 0..300 {
        files.clear();
        collect_json_files(&cassette_dir, &mut files);
        if !files.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    if let Some(cassette_path) = files.first() {
        let cassette_bytes = std::fs::read(cassette_path).expect("read cassette");
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
    }

    let mut replay_cmd = Command::new(env!("CARGO_BIN_EXE_wrela"));
    replay_cmd.current_dir(dir.path()).arg("test").arg(".");
    apply_fast_cert_budgets(&mut replay_cmd);
    let replay_output = run_command_with_timeout(&mut replay_cmd, Duration::from_secs(120));
    assert!(
        replay_output.status.success(),
        "{}",
        String::from_utf8_lossy(&replay_output.stderr)
    );
}

#[test]
fn cli_test_replay_mode_reports_missing_http_cassette() {
    let dir = workspace_tempdir();
    write_http_missing_cassette_project(dir.path(), "http://127.0.0.1:9/charge");

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_wrela"));
    cmd.current_dir(dir.path()).arg("test").arg(".");
    apply_fast_cert_budgets(&mut cmd);
    let output = run_command_with_timeout(&mut cmd, Duration::from_secs(120));

    assert!(
        !output.status.success(),
        "missing-cassette path should fail"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("http cassette replay missing")
            || stderr.contains("cassettes")
            || stderr.contains("missing")
            || stderr.contains("assert failed"),
        "expected missing-cassette diagnostics, got:\n{stderr}"
    );
}

#[test]
fn cli_test_rejects_emit_flags_even_in_maintenance_modes() {
    let dir = workspace_tempdir();
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
    let dir = workspace_tempdir();
    write_public_surface_project(
        dir.path(),
        "fn compute(value: Integer) -> Integer {\n    return value\n}\n",
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

    write_fixture_file(
        dir.path().join("src").join("public_api.wr"),
        r#"fn compute(value: String) -> String {
    return value
}
"#,
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
    let dir = workspace_tempdir();
    write_public_surface_project(
        dir.path(),
        "fn compute(value: Integer) -> Integer {\n    return value\n}\n",
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

    write_fixture_file(
        dir.path().join("src").join("public_api.wr"),
        r#"fn compute(value: String) -> String {
    return value
}
"#,
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
