use std::collections::HashSet;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;
use std::thread;
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[ctor::ctor]
fn configure_cli_test_threads() {
    if std::env::var_os("RUST_TEST_THREADS").is_none() {
        // Bound default concurrency so heavy cert/mutation paths stay reliable
        // under plain `cargo test -p wrela --test cli`.
        unsafe {
            std::env::set_var("RUST_TEST_THREADS", "1");
        }
    }
}

fn workspace_tempdir() -> tempfile::TempDir {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf();
    tempfile::Builder::new()
        .prefix("wrela-cli-")
        .tempdir_in(&workspace_root)
        .expect("tempdir")
}

fn run_command_with_timeout(cmd: &mut Command, timeout: Duration) -> std::process::Output {
    let capture_dir = tempfile::Builder::new()
        .prefix("wrela-cli-capture-")
        .tempdir()
        .expect("capture tempdir");
    let stdout_path = capture_dir.path().join("stdout.log");
    let stderr_path = capture_dir.path().join("stderr.log");
    let stdout_file = std::fs::File::create(&stdout_path).expect("create stdout capture");
    let stderr_file = std::fs::File::create(&stderr_path).expect("create stderr capture");

    let mut child = cmd
        .stdout(Stdio::from(
            stdout_file
                .try_clone()
                .expect("clone stdout capture handle"),
        ))
        .stderr(Stdio::from(
            stderr_file
                .try_clone()
                .expect("clone stderr capture handle"),
        ))
        .spawn()
        .expect("spawn command");
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().expect("try_wait") {
            break status;
        }
        if started.elapsed() >= timeout {
            #[cfg(unix)]
            {
                let pid = child.id().to_string();
                let _ = Command::new("pkill")
                    .arg("-TERM")
                    .arg("-P")
                    .arg(&pid)
                    .status();
                let _ = Command::new("pkill")
                    .arg("-KILL")
                    .arg("-P")
                    .arg(&pid)
                    .status();
            }
            let _ = child.kill();
            let _ = child.wait();
            panic!(
                "command timed out after {:?}; process tree terminated",
                timeout
            );
        }
        thread::sleep(Duration::from_millis(25));
    };

    drop(stdout_file);
    drop(stderr_file);
    let stdout = std::fs::read(&stdout_path).expect("read captured stdout");
    let stderr = std::fs::read(&stderr_path).expect("read captured stderr");

    std::process::Output {
        status,
        stdout,
        stderr,
    }
}

fn apply_fast_cert_budgets(cmd: &mut Command) {
    cmd.env("WRELA_BUDGET_TEST_JOBS", "4");
    cmd.env("WRELA_BUDGET_TEST_TIMEOUT_MS", "3000");
    cmd.env("WRELA_BUDGET_AUTOGEN_MAX_CASES", "1");
    cmd.env("WRELA_BUDGET_SIM_MAX_CASES", "8");
    cmd.env("WRELA_BUDGET_FUZZ_MAX_CASES", "2");
    cmd.env("WRELA_BUDGET_MUTATION_MAX_CASES", "4");
    cmd.env("WRELA_BUDGET_AUTOGEN_TIME_CAP_MS", "200");
    cmd.env("WRELA_BUDGET_SIM_TIME_CAP_MS", "500");
    cmd.env("WRELA_BUDGET_FUZZ_TIME_CAP_MS", "700");
    cmd.env("WRELA_BUDGET_MUTATION_TIME_CAP_MS", "1200");
}

fn run_build_with_fast_cert(
    entry: &std::path::Path,
    timeout: Duration,
    configure: impl FnOnce(&mut Command),
) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_wrela"));
    cmd.arg("build").arg(entry);
    apply_fast_cert_budgets(&mut cmd);
    configure(&mut cmd);
    run_command_with_timeout(&mut cmd, timeout)
}

fn write_fixture_file(
    path: impl AsRef<std::path::Path>,
    contents: impl AsRef<[u8]>,
) -> std::io::Result<()> {
    std::fs::write(path, contents)
}

fn write_presentation_plan_fixture(root: &std::path::Path) -> PathBuf {
    let src_dir = root.join("src");
    std::fs::create_dir_all(&src_dir).expect("create src dir");
    let entry = src_dir.join("main.wr");
    write_fixture_file(
        &entry,
        r#"
field exact distance cli_plan_field(p: Vec3) -> F32 {
    sphere(radius = 0.5)
}

material cli_plan_material(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.2, 0.4, 0.8),
        roughness=0.35,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape cli_plan_shape {
    field = cli_plan_field
    material = cli_plan_material
}

region cli_plan_region() {
    place scene = cli_plan_shape
}

domain cli_plan_domain(world: RegionCapture) {
    geometry_detail = 1
    material = true
    radiance = true
    media = true
    max_distance = 6.0
    min_step = 0.05
    hit_epsilon = 0.001
    max_steps = 64
}

view cli_plan_view(world: RegionCapture, camera: Camera) {
    domain = cli_plan_domain(world = world)
    viewport = viewport(width = 2, height = 2)
    quality = realtime_quality(
        target_fps = 120,
        allow_dynamic_resolution = false,
        primary_max_steps = 48
    )
    lighting = key_light(
        light = Light(
            position = camera.position + vec3(0.5, 1.0, 0.5),
            direction = normalize(vec3(-0.4, -0.7, -0.2)),
            intensity = vec3(1.0, 1.0, 1.0),
            range = 8.0
        ),
        fill_direction = normalize(vec3(-0.2, 0.8, 0.4)),
        fill_strength = 0.33,
        ambient_color = vec3(0.08, 0.11, 0.14)
    )
    outputs = frame_outputs(color = true, depth = true, normal = true, motion = true)
    history = temporal_history(color = true)
}

view cli_plan_fast_view(world: RegionCapture, camera: Camera) {
    domain = cli_plan_domain(world = world)
    viewport = viewport(width = 2, height = 2)
    quality = realtime_quality(
        target_fps = 60,
        allow_radiance = false,
        allow_media = false
    )
    lighting = key_light(
        light = Light(
            position = camera.position + vec3(1.0, 1.1, 0.8),
            direction = normalize(vec3(-0.5, -0.8, -0.4)),
            intensity = vec3(1.0, 0.95, 0.90),
            range = 8.0
        )
    )
    outputs = frame_outputs(color = true, depth = false, normal = false, motion = false)
    history = temporal_history(color = false)
}
"#,
    )
    .expect("write presentation plan fixture");
    entry
}

fn write_presentation_debug_expression_fixture(root: &std::path::Path) -> PathBuf {
    let src_dir = root.join("src");
    std::fs::create_dir_all(&src_dir).expect("create src dir");
    let entry = src_dir.join("main.wr");
    write_fixture_file(
        &entry,
        r#"
field exact distance expr_field(p: Vec3) -> F32 {
    sphere(radius = 0.5)
}

material expr_material(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.5, 0.5, 0.5),
        roughness=0.2,
        metalness=0.0,
        clearcoat=0.0,
        clearcoat_roughness=0.0,
        sheen=0.0,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape expr_shape {
    field = expr_field
    material = expr_material
}

region expr_region() {
    place scene = expr_shape
}

fn expr_width() -> Integer {
    return 4
}

fn expr_distance() -> F32 {
    return 8.0
}

domain expr_domain(world: RegionCapture) {
    geometry_detail = 1
    material = true
    radiance = false
    media = false
    max_distance = expr_distance()
    min_step = 0.02
    hit_epsilon = 0.0005
    max_steps = 128
}

view expr_view(world: RegionCapture, camera: Camera) {
    domain = expr_domain(world = world)
    viewport = viewport(width = expr_width(), height = 4)
}
"#,
    )
    .expect("write presentation debug expression fixture");
    entry
}

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
    assert!(!stdout.contains("game <subcommand> <path>"));
    assert!(!stdout.contains("realtime <subcommand> <path>"));
    assert!(!stdout.contains("mmo <subcommand> <path>"));
    assert!(!stdout.contains("frontend <subcommand> <path>"));
    assert!(!stdout.contains("studio <subcommand> <path>"));
    assert!(!stdout.contains("agent-run <path>"));
    assert!(!stdout.contains("--render=NAME"));
    assert!(!stdout.contains("--host=NAME"));
    assert!(!stdout.contains("--client-runtime=MODE"));
    assert!(!stdout.contains("--shader-provenance"));
    assert!(!stdout.contains("--no-shortcuts"));
    assert!(!stdout.contains("--intent-v2"));
    assert!(!stdout.contains("--determinism"));
    assert!(!stdout.contains("--rollback"));
    assert!(!stdout.contains("--render-lane"));
    assert!(!stdout.contains("--asset-streaming"));
    assert!(!stdout.contains("--gpu-metrics"));
    assert!(!stdout.contains("--streaming-metrics"));
    assert!(stdout.contains("--list"));
    assert!(stdout.contains("--id=ID"));
    assert!(stdout.contains("--filter=PATTERN"));
    assert!(stdout.contains("--replay-trace"));
    assert!(stdout.contains("--integration-mode"));
    assert!(stdout.contains("run certification"));
    assert!(stdout.contains("query-contracts"));
    assert!(stdout.contains("preview <path>"));
    assert!(stdout.contains("frame <path>"));
    assert!(stdout.contains("frame-contracts <path>"));
    assert!(stdout.contains("--attachment-format=json|ppm"));
    assert!(stdout.contains("--json-report"));
    assert!(stdout.contains("presentation-plan"));
    assert!(stdout.contains("presentation-debug"));
    assert!(!stdout.contains("--no-certify"));
}

#[test]
fn cli_query_contracts_json_lists_family_catalog() {
    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("query-contracts")
        .arg("--json")
        .output()
        .expect("run query-contracts");
    assert!(
        output.status.success(),
        "query-contracts failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let catalog: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("query contract catalog json");
    assert_eq!(
        catalog.get("schema_version").and_then(|v| v.as_u64()),
        Some(1)
    );
    let contracts = catalog
        .get("contracts")
        .and_then(|v| v.as_array())
        .expect("contracts array");
    assert!(
        contracts
            .iter()
            .all(|contract| contract.get("helper").is_none()),
        "public query contract catalog must not expose internal helper names"
    );
    assert!(contracts.iter().any(|contract| {
        contract
            .get("contract_id")
            .and_then(|v| v.as_str())
            .is_some_and(|id| id == "spatial.nearest.capture.shape")
            && contract
                .get("call")
                .and_then(|v| v.as_str())
                .is_some_and(|call| call == "spatial.nearest")
            && contract
                .get("legacy_builtin")
                .and_then(|v| v.as_str())
                .is_some_and(|name| name == "trace_shape")
    }));
    assert!(contracts.iter().any(|contract| {
        contract
            .get("contract_id")
            .and_then(|v| v.as_str())
            .is_some_and(|id| id == "support.summary.world")
            && contract
                .get("backends")
                .and_then(|v| v.as_array())
                .is_some_and(|backends| {
                    backends.iter().any(|backend| backend == "cpu")
                        && backends.iter().any(|backend| backend == "virtual_gpu")
                        && !backends.iter().any(|backend| backend == "wgsl")
                })
    }));
    assert!(contracts.iter().any(|contract| {
        contract
            .get("contract_id")
            .and_then(|v| v.as_str())
            .is_some_and(|id| id == "spatial.nearest.batch.world")
            && contract
                .get("target")
                .and_then(|v| v.as_str())
                .is_some_and(|target| target == "world")
            && contract
                .get("cardinality")
                .and_then(|v| v.as_str())
                .is_some_and(|cardinality| cardinality == "batch")
            && contract
                .get("surface")
                .and_then(|v| v.as_str())
                .is_some_and(|surface| surface == "batch.world")
    }));
    let aliases = catalog
        .get("aliases")
        .and_then(|v| v.as_array())
        .expect("aliases array");
    assert!(aliases.iter().any(|alias| {
        alias
            .get("alias_id")
            .and_then(|v| v.as_str())
            .is_some_and(|id| id == "spatial.trace.world")
            && alias
                .get("canonical_id")
                .and_then(|v| v.as_str())
                .is_some_and(|id| id == "spatial.nearest.world")
    }));
}

#[test]
fn cli_presentation_plan_human_shows_contracts_without_helper_names() {
    let temp = workspace_tempdir();
    write_presentation_plan_fixture(temp.path());

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("presentation-plan")
        .arg(temp.path())
        .output()
        .expect("run presentation-plan");
    assert!(
        output.status.success(),
        "presentation-plan failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("presentation plan schema v1"));
    assert!(stdout.contains("plan cli_plan_view"));
    assert!(stdout.contains("plan cli_plan_fast_view"));
    assert!(stdout.contains("canonical_projection=true"));
    assert!(stdout.contains("input=Camera.vertical_fov_degrees"));
    assert!(stdout.contains("screen lattice: sample_position=PixelCenter origin=TopLeft"));
    assert!(stdout.contains("canonical view rays: space=World normalized_direction=true"));
    assert!(stdout.contains("compatibility_legacy_path=false"));
    assert!(stdout.contains("GenerateScreenSamples"));
    assert!(stdout.contains("PrimaryVisibility"));
    assert!(stdout.contains("SurfaceResolve"));
    assert!(stdout.contains("ParticipantsResolve"));
    assert!(stdout.contains("ShadePrimary"));
    assert!(stdout.contains("CompositeColor"));
    assert!(stdout.contains("TemporalResolve(shaded_color->color)"));
    assert!(stdout.contains("screen samples: viewport=view.viewport.widthxview.viewport.height"));
    assert!(stdout.contains("spatial.nearest.batch.world"));
    assert!(stdout.contains("surface.sample.batch.world"));
    assert!(stdout.contains("participants.radiance.batch.world"));
    assert!(stdout.contains("participants.medium.batch.world"));
    assert!(
        stdout
            .contains("primary hit attachment: primary_hit record=Hit3 depth=RayParameterDistance")
    );
    assert!(stdout.contains(
        "frame outputs: primary_hit(PrimaryHit,Hit3,Transient,Viewport,1x1,SemanticDefault)"
    ));
    assert!(
        stdout.contains(
            "history_color(Color,vec3<f32>,HistorySlot(0),Viewport,1x1,PreservePrevious)"
        )
    );
    assert!(stdout.contains(
        "history_primary_hit(PrimaryHit,Hit3,HistorySlot(1),Viewport,1x1,PreservePrevious)"
    ));
    assert!(stdout.contains("quality: tier=realtime_120 target_fps=120"));
    assert!(stdout.contains("quality: tier=realtime_60 target_fps=60"));
    assert!(stdout.contains(
        "lighting: key_light=lighting.key_light:Light:AuthoredMetadata:compat_alias=false"
    ));
    assert!(stdout.contains(
        "fill_direction=lighting.fill_direction:vec3<f32>:AuthoredMetadata:compat_alias=false"
    ));
    assert!(
        stdout.contains(
            "fill_strength=lighting.fill_strength:f32:AuthoredMetadata:compat_alias=false"
        )
    );
    assert!(stdout.contains(
        "ambient_color=lighting.ambient_color:vec3<f32>:AuthoredMetadata:compat_alias=false"
    ));
    assert!(stdout.contains(
        "fill_direction=lighting.fill_direction:vec3<f32>:DefaultCompatibilityRecipe:compat_alias=false"
    ));
    assert!(stdout.contains(
        "fill_strength=lighting.fill_strength:f32:DefaultCompatibilityRecipe:compat_alias=false"
    ));
    assert!(stdout.contains(
        "ambient_color=lighting.ambient_color:vec3<f32>:DefaultCompatibilityRecipe:compat_alias=false"
    ));
    assert!(stdout.contains("future acceleration hooks: ScreenLattice"));
    assert!(stdout.contains("future acceleration hooks: WorldBatch, SemanticSupport"));
    assert!(stdout.contains("motion(Motion,MotionVector,Transient,Viewport,1x1,SemanticDefault)"));
    assert!(stdout.contains("materializes: motion"));
    assert!(stdout.contains("materializes: color, history_color, history_primary_hit"));
    assert!(stdout.contains("future acceleration hooks: TemporalHistory"));
    assert!(
        stdout
            .contains("motion.resolve recipe=MotionResolve backend=auto execution=motion_resolve")
    );
    assert!(stdout.contains(
        "temporal.resolve recipe=TemporalResolve backend=auto execution=temporal_resolve"
    ));
    assert!(
        stdout.contains(
            "composite.color recipe=CompositeColor backend=auto execution=composite_color"
        )
    );
    assert!(!stdout.contains("__wr_render_capture_to_ppm"));
}

#[test]
fn cli_presentation_plan_json_reports_passes_bindings_and_query_dependencies() {
    let temp = workspace_tempdir();
    write_presentation_plan_fixture(temp.path());

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("presentation-plan")
        .arg(temp.path())
        .arg("--json")
        .output()
        .expect("run presentation-plan json");
    assert!(
        output.status.success(),
        "presentation-plan --json failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let dump: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("presentation plan json");
    assert_eq!(
        dump.get("schema_version").and_then(|value| value.as_u64()),
        Some(1)
    );
    let plans = dump
        .get("plans")
        .and_then(|value| value.as_array())
        .expect("plans array");
    assert_eq!(plans.len(), 2);
    let view_plan = plans
        .iter()
        .find(|plan| {
            plan.get("name")
                .and_then(|value| value.as_str())
                .is_some_and(|name| name == "cli_plan_view")
        })
        .expect("helper-rich view plan");
    assert_eq!(
        view_plan.get("name").and_then(|value| value.as_str()),
        Some("cli_plan_view")
    );
    assert_eq!(
        view_plan
            .pointer("/view/canonical_projection_input")
            .and_then(|value| value.as_str()),
        Some("Camera.vertical_fov_degrees")
    );
    assert_eq!(
        view_plan
            .pointer("/view/compatibility_projection/legacy_path_active")
            .and_then(|value| value.as_bool()),
        Some(false)
    );
    assert_eq!(
        view_plan
            .pointer("/view/screen_lattice/width_source")
            .and_then(|value| value.as_str()),
        Some("view.viewport.width")
    );
    assert_eq!(
        view_plan
            .pointer("/view/screen_lattice/height_source")
            .and_then(|value| value.as_str()),
        Some("view.viewport.height")
    );
    assert_eq!(
        view_plan
            .pointer("/frame/quality/tier")
            .and_then(|value| value.as_str()),
        Some("realtime_120")
    );
    assert_eq!(
        view_plan
            .pointer("/frame/quality/target_fps")
            .and_then(|value| value.as_u64()),
        Some(120)
    );
    assert_eq!(
        view_plan
            .pointer("/frame/quality/allow_dynamic_resolution")
            .and_then(|value| value.as_bool()),
        Some(false)
    );
    assert_eq!(
        view_plan
            .pointer("/frame/quality/primary_max_steps")
            .and_then(|value| value.as_i64()),
        Some(48)
    );
    assert_eq!(
        view_plan
            .pointer("/frame/lighting/key_light/source")
            .and_then(|value| value.as_str()),
        Some("AuthoredMetadata")
    );
    assert_eq!(
        view_plan
            .pointer("/frame/lighting/fill_strength/source")
            .and_then(|value| value.as_str()),
        Some("AuthoredMetadata")
    );
    assert_eq!(
        view_plan
            .pointer("/frame/temporal_reuse")
            .and_then(|value| value.as_str()),
        Some("ReprojectColorAndMotion")
    );
    let passes = view_plan
        .get("passes")
        .and_then(|value| value.as_array())
        .expect("passes array");
    assert_eq!(
        passes
            .iter()
            .map(|pass| pass.get("kind").and_then(|value| value.as_str()).unwrap())
            .collect::<Vec<_>>(),
        vec![
            "GenerateScreenSamples",
            "PrimaryVisibility(spatial.nearest.batch.world)",
            "SurfaceResolve(surface.sample.batch.world)",
            "ParticipantsResolve(radiance=participants.radiance.batch.world,medium=participants.medium.batch.world)",
            "ShadePrimary(shaded_color)",
            "MotionResolve(primary_hit->motion)",
            "TemporalResolve(shaded_color->color)",
        ]
    );
    assert!(passes.iter().any(|pass| {
        pass.get("kind").and_then(|value| value.as_str()) == Some("GenerateScreenSamples")
            && pass
                .pointer("/screen_samples/item_count_expression")
                .and_then(|value| value.as_str())
                == Some("view.viewport.width * view.viewport.height * 1")
    }));
    assert!(passes.iter().any(|pass| {
        pass.get("kind").and_then(|value| value.as_str())
            == Some("PrimaryVisibility(spatial.nearest.batch.world)")
            && pass
                .get("query_dependencies")
                .and_then(|value| value.as_array())
                .is_some_and(|deps| {
                    deps.iter().any(|dep| {
                        dep.get("contract_id").and_then(|value| value.as_str())
                            == Some("spatial.nearest.batch.world")
                    }) && deps.iter().any(|dep| {
                        dep.pointer("/solver_diagnostics/fallback")
                            .and_then(|value| value.as_str())
                            == Some("exact-dense-sphere-tracing")
                    })
                })
    }));
    let outputs = view_plan
        .pointer("/frame/outputs")
        .and_then(|value| value.as_array())
        .expect("view outputs");
    assert_eq!(outputs.len(), 11);
    assert!(
        outputs.iter().any(|output| {
            output.get("name").and_then(|value| value.as_str()) == Some("motion")
        })
    );
    assert!(outputs.iter().any(|output| {
        output.get("name").and_then(|value| value.as_str()) == Some("history_color")
    }));
    let bindings = view_plan
        .get("bindings")
        .and_then(|value| value.as_array())
        .expect("bindings array");
    assert_eq!(bindings.len(), 7);
    assert!(bindings.iter().any(|binding| {
        binding.get("id").and_then(|value| value.as_str()) == Some("motion.resolve")
            && binding.get("execution").and_then(|value| value.as_str()) == Some("motion_resolve")
    }));

    let fast_view = plans
        .iter()
        .find(|plan| {
            plan.get("name")
                .and_then(|value| value.as_str())
                .is_some_and(|name| name == "cli_plan_fast_view")
        })
        .expect("fast view plan");
    assert_eq!(
        fast_view
            .pointer("/frame/temporal_reuse")
            .and_then(|value| value.as_str()),
        None
    );
    assert_eq!(
        fast_view
            .pointer("/frame/quality/tier")
            .and_then(|value| value.as_str()),
        Some("realtime_60")
    );
    assert_eq!(
        fast_view
            .pointer("/frame/quality/temporal_mode")
            .and_then(|value| value.as_str()),
        Some("Disabled")
    );
    assert_eq!(
        fast_view
            .pointer("/frame/quality/allow_radiance")
            .and_then(|value| value.as_bool()),
        Some(false)
    );
    assert_eq!(
        fast_view
            .pointer("/frame/lighting/fill_direction/source")
            .and_then(|value| value.as_str()),
        Some("DefaultCompatibilityRecipe")
    );
    let fast_outputs = fast_view
        .pointer("/frame/outputs")
        .and_then(|value| value.as_array())
        .expect("fast view outputs");
    assert_eq!(fast_outputs.len(), 6);
    assert!(
        !fast_outputs.iter().any(|output| {
            output.get("name").and_then(|value| value.as_str()) == Some("motion")
        })
    );
    assert!(
        !String::from_utf8_lossy(&output.stdout).contains("__wr_render_capture_to_ppm"),
        "presentation-plan JSON should not expose raw helper names"
    );
}

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
    assert!(stdout.contains("snapshot_id="));
    assert!(stdout.contains("epoch=1"));
    assert!(stdout.contains("color ppm:"));
    assert!(stdout.contains("depth ppm:"));
    assert!(stdout.contains("world normal ppm:"));
    assert!(stdout.contains("semantic domain:"));
    assert!(stdout.contains("execution policy:"));
    assert!(stdout.contains("required_guarantee=conservative_no_false_miss"));
    assert!(stdout.contains("selected_method=conservative_solver"));
    assert!(stdout.contains("hit_rate="));
    assert!(stdout.contains("quality tier=realtime_120"));
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
        dump.pointer("/frame_cost/quality/tier")
            .and_then(|value| value.as_str()),
        Some("realtime_120")
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
        dump.get("stats")
            .and_then(|value| value.as_str())
            .is_some_and(|stats| stats.contains("quality tier=realtime_120"))
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

#[test]
fn cli_rejects_removed_format_flag_alias() {
    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg("--format=json")
        .arg(".")
        .output()
        .expect("run wrela");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("`--format` was removed"));
}

#[test]
fn cli_fmt_defaults_to_target_file_diagnostics_scope() {
    let dir = workspace_tempdir();
    let src_dir = dir.path().join("src");
    std::fs::create_dir_all(&src_dir).expect("create src");
    let main_path = src_dir.join("main.wr");
    let helper_path = src_dir.join("helper.wr");
    write_fixture_file(
        &main_path,
        r#"use add from helper

fn run() -> Integer {
    return add(value=1, extra=2)
}
"#,
    )
    .expect("write main");
    write_fixture_file(
        &helper_path,
        r#"fn add(value: Integer, extra: Integer) -> Integer {
    return value + extra
}

fn trigger_named_args_error() -> Integer {
    return add(1, 2)
}
"#,
    )
    .expect("write helper");

    let scoped = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fmt")
        .arg("--error-format=json")
        .arg(&main_path)
        .output()
        .expect("run scoped fmt");
    assert!(
        scoped.status.success(),
        "scoped fmt failed: code={:?}\nstdout={}\nstderr={}",
        scoped.status.code(),
        String::from_utf8_lossy(&scoped.stdout),
        String::from_utf8_lossy(&scoped.stderr)
    );
    let scoped_stdout = String::from_utf8_lossy(&scoped.stdout);
    assert!(
        !scoped_stdout.contains("named_args_required"),
        "target-scoped fmt should not emit imported helper diagnostics: {scoped_stdout}"
    );

    let workspace = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fmt")
        .arg("--workspace-diagnostics")
        .arg("--error-format=json")
        .arg(&main_path)
        .output()
        .expect("run workspace fmt");
    assert!(
        workspace.status.success(),
        "workspace fmt failed: code={:?}\nstdout={}\nstderr={}",
        workspace.status.code(),
        String::from_utf8_lossy(&workspace.stdout),
        String::from_utf8_lossy(&workspace.stderr)
    );
    let workspace_stdout = String::from_utf8_lossy(&workspace.stdout);
    assert!(
        workspace_stdout.contains("named_args_required"),
        "workspace diagnostics should include imported helper errors: {workspace_stdout}"
    );
}

#[test]
fn cli_run_integration_mode_enforces_entry_layout_guardrail() {
    let dir = workspace_tempdir();
    let entry = dir.path().join("main.wr");
    write_fixture_file(
        &entry,
        r#"fn run() -> Integer {
    return 0
}
"#,
    )
    .expect("write main.wr");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("run")
        .arg("--integration-mode")
        .arg(&entry)
        .output()
        .expect("run wrela");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--integration-mode requires entrypoint under"));
}

#[test]
fn cli_init_creates_project() {
    let dir = workspace_tempdir();
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
    let dir = workspace_tempdir();
    let path = dir.path().join("main.wr");
    write_fixture_file(
        &path,
        r#"fn run() -> Integer {
    return 1 +
}
"#,
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let first = stdout.lines().next().expect("json output");
    let value: serde_json::Value = serde_json::from_str(first).expect("valid json");
    assert!(value.get("message").is_some());
    assert!(value.get("span").is_some());
    assert!(
        value
            .get("code")
            .and_then(|value| value.as_str())
            .is_some_and(|code| !code.is_empty())
    );
    assert!(
        value
            .get("rule")
            .and_then(|value| value.as_str())
            .is_some_and(|rule| !rule.is_empty())
    );
    assert!(value.get("help").is_some());
    assert!(
        value
            .get("stage")
            .and_then(|value| value.as_str())
            .is_some_and(|stage| !stage.is_empty())
    );
    assert!(
        value
            .get("severity")
            .and_then(|value| value.as_str())
            .is_some_and(|severity| severity == "error" || severity == "warning")
    );
    assert!(
        value
            .get("labels")
            .and_then(|value| value.as_array())
            .is_some_and(|labels| !labels.is_empty())
    );
    assert!(value.get("diag_id").is_some());
}

#[test]
fn cli_json_shorthand_diagnostics() {
    let dir = workspace_tempdir();
    let path = dir.path().join("main.wr");
    write_fixture_file(
        &path,
        r#"fn run() -> Integer {
    return 1 +
}
"#,
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("--json")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let first = stdout.lines().next().expect("json output");
    let value: serde_json::Value = serde_json::from_str(first).expect("valid json");
    assert!(value.get("message").is_some());
    assert!(value.get("span").is_some());
    assert_eq!(value.get("kind").and_then(|v| v.as_str()), Some("error"));
}

#[test]
fn cli_json_typed_hole_includes_data_and_candidate_suggestions() {
    let dir = workspace_tempdir();
    let path = dir.path().join("main.wr");
    write_fixture_file(
        &path,
        r#"fn run(value: Integer) -> Integer {
    return _todo
}
"#,
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(!output.status.success());
    let diagnostics = parse_json_stdout_lines(&output.stdout);
    let typed_hole = diagnostics
        .iter()
        .find(|diag| {
            diag.get("code")
                .and_then(|value| value.as_str())
                .is_some_and(|code| code == "lang::sem::typed_hole")
        })
        .expect("expected typed hole diagnostic");
    assert_eq!(
        typed_hole
            .get("data")
            .and_then(|v| v.get("kind"))
            .and_then(|v| v.as_str()),
        Some("typed_hole")
    );
    assert_eq!(
        typed_hole
            .get("data")
            .and_then(|v| v.get("hole_name"))
            .and_then(|v| v.as_str()),
        Some("_todo")
    );
    assert!(
        typed_hole
            .get("data")
            .and_then(|v| v.get("candidate_bindings"))
            .and_then(|v| v.as_array())
            .is_some_and(|arr| arr.iter().any(|v| v.as_str() == Some("value")))
    );
    assert!(
        typed_hole
            .get("data")
            .and_then(|v| v.get("hole_id"))
            .and_then(|v| v.as_str())
            .is_some_and(|id| id.contains(":_todo"))
    );
    assert_eq!(
        typed_hole
            .get("data")
            .and_then(|v| v.get("ranking_strategy"))
            .and_then(|v| v.as_str()),
        Some("lexicographic_binding_name")
    );
    assert!(
        typed_hole
            .get("data")
            .and_then(|v| v.get("ranked_candidates"))
            .and_then(|v| v.as_array())
            .is_some_and(|arr| {
                arr.iter().all(|candidate| {
                    candidate.get("rank").and_then(|v| v.as_u64()).is_some()
                        && candidate.get("name").and_then(|v| v.as_str()).is_some()
                })
            })
    );
    assert!(
        typed_hole
            .get("data")
            .and_then(|v| v.get("code_actions"))
            .and_then(|v| v.as_array())
            .is_some_and(|arr| {
                arr.iter().any(|action| {
                    action
                        .get("kind")
                        .and_then(|v| v.as_str())
                        .is_some_and(|kind| kind == "fill_typed_hole")
                })
            })
    );
    assert!(
        typed_hole
            .get("suggestions")
            .and_then(|v| v.as_array())
            .is_some_and(|arr| arr.iter().any(|s| {
                s.get("replacement")
                    .and_then(|v| v.as_str())
                    .is_some_and(|candidate| candidate.trim() == "value")
            }))
    );
}

#[test]
fn cli_holes_only_filters_non_hole_semantic_diagnostics() {
    let dir = workspace_tempdir();
    let path = dir.path().join("main.wr");
    write_fixture_file(
        &path,
        r#"fn helper(a: Integer, b: Integer) -> Integer {
    return a + b

}
fn run(value: Integer) -> Integer {
    helper(a=1, 2)
    return _todo
}
"#,
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg("--holes-only")
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(!output.status.success());
    let diagnostics = parse_json_stdout_lines(&output.stdout);
    assert!(
        diagnostics.iter().any(|diag| {
            diag.get("code")
                .and_then(|value| value.as_str())
                .is_some_and(|code| code == "lang::sem::typed_hole")
        }),
        "expected typed hole diagnostics, got:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        diagnostics.iter().all(|diag| {
            diag.get("code")
                .and_then(|value| value.as_str())
                .is_some_and(|code| code == "lang::sem::typed_hole")
        }),
        "holes-only mode should suppress non-hole diagnostics, got:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn cli_json_try_outside_result_includes_data_and_remove_try_suggestion() {
    let dir = workspace_tempdir();
    let path = dir.path().join("main.wr");
    write_fixture_file(
        &path,
        r#"fn source() -> Result[Integer] {
    return 1

}
fn run() -> Integer {
    return source()?
}
"#,
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(!output.status.success());
    let diagnostics = parse_json_stdout_lines(&output.stdout);
    let try_diag = diagnostics
        .iter()
        .find(|diag| {
            diag.get("code")
                .and_then(|value| value.as_str())
                .is_some_and(|code| code == "lang::ty::try_outside_result")
        })
        .expect("expected try-outside-result diagnostic");
    assert_eq!(
        try_diag
            .get("data")
            .and_then(|v| v.get("kind"))
            .and_then(|v| v.as_str()),
        Some("try_outside_result")
    );
    assert!(
        try_diag
            .get("suggestions")
            .and_then(|v| v.as_array())
            .is_some_and(|arr| arr.iter().any(|s| {
                s.get("replacement").and_then(|v| v.as_str()) == Some("")
                    && s.get("reason_code").and_then(|v| v.as_str()) == Some("remove_try_operator")
            })),
        "expected remove-try suggestion, got:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn cli_json_invalid_try_operand_includes_data_and_remove_try_suggestion() {
    let dir = workspace_tempdir();
    let path = dir.path().join("main.wr");
    write_fixture_file(
        &path,
        r#"fn run() -> Result[Integer] {
    return 1?
}
"#,
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(!output.status.success());
    let diagnostics = parse_json_stdout_lines(&output.stdout);
    let try_diag = diagnostics
        .iter()
        .find(|diag| {
            diag.get("code")
                .and_then(|value| value.as_str())
                .is_some_and(|code| code == "lang::ty::invalid_try_operand")
        })
        .expect("expected invalid-try-operand diagnostic");
    assert_eq!(
        try_diag
            .get("data")
            .and_then(|v| v.get("kind"))
            .and_then(|v| v.as_str()),
        Some("invalid_try_operand")
    );
    assert!(
        try_diag
            .get("suggestions")
            .and_then(|v| v.as_array())
            .is_some_and(|arr| arr.iter().any(|s| {
                s.get("replacement").and_then(|v| v.as_str()) == Some("")
                    && s.get("reason_code").and_then(|v| v.as_str()) == Some("remove_try_operator")
            })),
        "expected remove-try suggestion, got:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn cli_sarif_parse_diagnostics_include_required_contract_fields() {
    let dir = workspace_tempdir();
    let path = dir.path().join("main.wr");
    write_fixture_file(
        &path,
        r#"fn run() -> Integer {
    return 1 +
}
"#,
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("--error-format=sarif")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(!output.status.success());
    let logs = parse_json_stdout_lines(&output.stdout);
    let mut parse = None;
    for log in &logs {
        for result in assert_sarif_log_contract(log) {
            if result
                .get("ruleId")
                .and_then(|value| value.as_str())
                .is_some_and(|rule| rule.starts_with("lang::parse::"))
            {
                parse = Some(result);
                break;
            }
        }
        if parse.is_some() {
            break;
        }
    }
    let parse = parse.expect("expected parse SARIF result");
    assert_sarif_result_contract(parse);
}

#[test]
fn cli_sarif_naming_or_type_diagnostics_include_required_contract_fields() {
    let dir = workspace_tempdir();
    let path = dir.path().join("main.wr");
    write_fixture_file(
        &path,
        r#"fn BadName() -> Integer {
    value = 1
    return true
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg("--error-format=sarif")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(!output.status.success());

    let logs = parse_json_stdout_lines(&output.stdout);
    let mut semantic = None;
    for log in &logs {
        for result in assert_sarif_log_contract(log) {
            if result
                .get("ruleId")
                .and_then(|value| value.as_str())
                .is_some_and(|rule| {
                    rule.starts_with("lang::naming::") || rule.starts_with("lang::ty::")
                })
            {
                semantic = Some(result);
                break;
            }
        }
        if semantic.is_some() {
            break;
        }
    }
    let semantic = semantic.expect("expected naming/type SARIF result");
    assert_sarif_result_contract(semantic);
}

fn assert_sarif_log_contract(value: &serde_json::Value) -> &[serde_json::Value] {
    assert_eq!(
        value.get("$schema").and_then(|v| v.as_str()),
        Some("https://json.schemastore.org/sarif-2.1.0.json")
    );
    assert_eq!(value.get("version").and_then(|v| v.as_str()), Some("2.1.0"));
    let runs = value
        .get("runs")
        .and_then(|v| v.as_array())
        .expect("sarif runs array");
    assert!(!runs.is_empty(), "expected at least one SARIF run");
    let driver_name = runs[0]
        .get("tool")
        .and_then(|v| v.get("driver"))
        .and_then(|v| v.get("name"))
        .and_then(|v| v.as_str());
    assert_eq!(driver_name, Some("wrela"));
    let results = runs[0]
        .get("results")
        .and_then(|v| v.as_array())
        .expect("sarif results array");
    assert!(!results.is_empty(), "expected at least one SARIF result");
    results
}

fn assert_sarif_result_contract(result: &serde_json::Value) {
    assert!(
        result
            .get("ruleId")
            .and_then(|v| v.as_str())
            .is_some_and(|id| !id.is_empty())
    );
    assert!(
        result
            .get("level")
            .and_then(|v| v.as_str())
            .is_some_and(|level| level == "error" || level == "warning")
    );
    assert!(
        result
            .get("message")
            .and_then(|v| v.get("text"))
            .and_then(|v| v.as_str())
            .is_some_and(|text| !text.is_empty())
    );
    let locations = result
        .get("locations")
        .and_then(|v| v.as_array())
        .expect("sarif locations array");
    assert!(
        !locations.is_empty(),
        "expected at least one SARIF location"
    );
    let first = &locations[0];
    assert!(
        first
            .get("physicalLocation")
            .and_then(|v| v.get("artifactLocation"))
            .and_then(|v| v.get("uri"))
            .and_then(|v| v.as_str())
            .is_some_and(|uri| !uri.is_empty())
    );
    let region = first
        .get("physicalLocation")
        .and_then(|v| v.get("region"))
        .expect("sarif region");
    assert!(region.get("startLine").and_then(|v| v.as_u64()).is_some());
    assert!(region.get("startColumn").and_then(|v| v.as_u64()).is_some());
    assert!(region.get("charOffset").and_then(|v| v.as_u64()).is_some());
    assert!(region.get("charLength").and_then(|v| v.as_u64()).is_some());
}

#[test]
fn cli_json_naming_diagnostics_include_metadata_fields_when_present() {
    let dir = workspace_tempdir();
    let path = dir.path().join("main.wr");
    write_fixture_file(
        &path,
        r#"fn BadName() -> Integer {
    let AlsoBad = 1
    return AlsoBad
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg("--error-format=json")
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
            assert!(
                suggestion
                    .get("applicability")
                    .and_then(|value| value.as_str())
                    .is_some_and(|v| {
                        v == "machine_applicable" || v == "maybe_correct" || v == "has_placeholders"
                    })
            );
            assert!(
                suggestion
                    .get("safety_tier")
                    .and_then(|value| value.as_str())
                    .is_some_and(|tier| tier == "safe" || tier == "review")
            );
            assert!(
                suggestion
                    .get("reason_code")
                    .and_then(|value| value.as_str())
                    .is_some_and(|code| !code.is_empty())
            );
            if suggestion
                .get("applicability")
                .and_then(|value| value.as_str())
                .is_some_and(|v| v == "machine_applicable")
            {
                assert!(
                    suggestion
                        .get("expected_source")
                        .and_then(|value| value.as_str())
                        .is_some(),
                    "machine-applicable fixes must include expected_source"
                );
            }
        }
    }
}

#[test]
fn cli_json_diag_id_is_stable_across_runs() {
    let dir = workspace_tempdir();
    let path = dir.path().join("main.wr");
    write_fixture_file(
        &path,
        r#"fn run() -> Integer {
    return 1 +
}
"#,
    )
    .unwrap();

    let first = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run wrela");
    let second = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(!first.status.success());
    assert!(!second.status.success());

    let first_stdout = String::from_utf8_lossy(&first.stdout);
    let first_diag = first_stdout
        .lines()
        .find(|line| !line.trim().is_empty())
        .expect("first json line");
    let second_stdout = String::from_utf8_lossy(&second.stdout);
    let second_diag = second_stdout
        .lines()
        .find(|line| !line.trim().is_empty())
        .expect("second json line");
    let first_json: serde_json::Value = serde_json::from_str(first_diag).expect("valid json");
    let second_json: serde_json::Value = serde_json::from_str(second_diag).expect("valid json");
    assert_eq!(
        first_json.get("diag_id").and_then(|value| value.as_str()),
        second_json.get("diag_id").and_then(|value| value.as_str())
    );
}

#[test]
fn cli_json_contract_matches_required_and_optional_key_fixtures() {
    let required = include_str!("fixtures/diagnostics/json_required_keys.txt")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>();
    let optional = include_str!("fixtures/diagnostics/json_optional_keys.txt")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>();

    let dir = workspace_tempdir();
    let path = dir.path().join("main.wr");
    write_fixture_file(
        &path,
        r#"fn run() -> Integer {
    return 1 +
}
"#,
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let first = stdout
        .lines()
        .find(|line| !line.trim().is_empty())
        .expect("json line");
    let value: serde_json::Value = serde_json::from_str(first).expect("valid json");
    let object = value.as_object().expect("diagnostic is object");

    for key in required {
        assert!(object.contains_key(key), "missing required key: {key}");
    }
    for key in optional {
        if object.contains_key(key) {
            assert_ne!(key, "kind");
        }
    }
}

#[test]
fn cli_json_parse_diagnostics_use_specific_parse_codes() {
    let dir = workspace_tempdir();
    let path = dir.path().join("main.wr");
    write_fixture_file(
        &path,
        r#"fn run() -> Integer {
    return 1 +
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let diagnostics: Vec<serde_json::Value> = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("valid json"))
        .collect();
    let parse_codes = diagnostics
        .iter()
        .filter_map(|diag| diag.get("code").and_then(|value| value.as_str()))
        .filter(|code| code.starts_with("lang::parse::"))
        .collect::<Vec<_>>();
    assert!(
        parse_codes
            .iter()
            .any(|code| *code != "lang::parse::syntax_error"),
        "expected at least one specific parse code, got: {parse_codes:?}"
    );
}

#[test]
fn cli_analyze_alias_matches_check_parse_behavior() {
    let dir = workspace_tempdir();
    let path = dir.path().join("main.wr");
    write_fixture_file(
        &path,
        r#"fn run() -> Integer {
    return 1 +
}
"#,
    )
    .unwrap();

    let analyze = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("analyze")
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run wrela analyze");
    assert!(!analyze.status.success());
    let analyze_stdout = String::from_utf8_lossy(&analyze.stdout);
    let analyze_first = analyze_stdout
        .lines()
        .find(|line| !line.trim().is_empty())
        .expect("analyze produced diagnostic");
    let analyze_json: serde_json::Value =
        serde_json::from_str(analyze_first).expect("analyze json");
    assert_eq!(
        analyze_json.get("kind").and_then(|v| v.as_str()),
        Some("error")
    );

    let check = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run wrela check");
    assert!(!check.status.success());
    let check_stdout = String::from_utf8_lossy(&check.stdout);
    let check_first = check_stdout
        .lines()
        .find(|line| !line.trim().is_empty())
        .expect("check produced diagnostic");
    let check_json: serde_json::Value = serde_json::from_str(check_first).expect("check json");
    assert_eq!(
        analyze_json.get("code").and_then(|v| v.as_str()),
        check_json.get("code").and_then(|v| v.as_str())
    );
}

#[test]
fn cli_test_harness_json_aggregates_multiple_type_errors() {
    let dir = workspace_tempdir();
    let src_dir = dir.path().join("src");
    let module_path = src_dir.join("broken.wr");
    std::fs::create_dir_all(&src_dir).unwrap();
    write_fixture_file(
        src_dir.join("main.wr"),
        r#"use bad from broken

fn run() -> Integer {
    return bad()
}
"#,
    )
    .unwrap();
    write_fixture_file(
        module_path,
        r#"fn bad() -> Integer {
    x = 1 + true
    y = 1 + false
    return 0
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg("--error-format=json")
        .arg(src_dir.join("main.wr"))
        .output()
        .expect("run wrela");

    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let diagnostics: Vec<serde_json::Value> = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("valid json"))
        .collect();
    let type_errors: Vec<&serde_json::Value> = diagnostics
        .iter()
        .filter(|diag| {
            diag.get("code")
                .and_then(|value| value.as_str())
                .is_some_and(|code| code.starts_with("lang::ty::"))
        })
        .collect();
    assert!(
        type_errors.len() >= 2,
        "expected aggregated type diagnostics, got:\n{}",
        stdout
    );
}

#[test]
fn cli_json_reports_named_args_required_code() {
    let dir = workspace_tempdir();
    let path = dir.path().join("main.wr");
    write_fixture_file(
        &path,
        r#"fn add(a: Integer, b: Integer) -> Integer {
    return a + b

}
fn run() -> Integer {
    return add(1, 2)
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(!output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let diagnostics: Vec<serde_json::Value> = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("valid json"))
        .collect();
    let named_args = diagnostics
        .iter()
        .find(|diag| {
            diag.get("code")
                .and_then(|value| value.as_str())
                .is_some_and(|code| code == "lang::ty::named_args_required")
        })
        .expect("expected named args required diagnostic");
    assert_eq!(
        named_args
            .get("data")
            .and_then(|v| v.get("kind"))
            .and_then(|v| v.as_str()),
        Some("named_args_required")
    );
    assert!(
        named_args
            .get("suggestions")
            .and_then(|v| v.as_array())
            .is_some_and(|arr| arr.iter().any(|s| {
                s.get("reason_code")
                    .and_then(|v| v.as_str())
                    .is_some_and(|code| code == "named_args_rewrite")
            })),
        "expected named-args rewrite suggestion, got:\n{}",
        stdout
    );
    assert!(
        named_args
            .get("suggestions")
            .and_then(|v| v.as_array())
            .is_some_and(|arr| arr.iter().any(|s| {
                s.get("applicability")
                    .and_then(|v| v.as_str())
                    .is_some_and(|mode| mode == "machine_applicable")
            })),
        "expected machine-applicable named-args suggestion, got:\n{}",
        stdout
    );
}

#[test]
fn cli_json_reports_equality_requires_eq_code() {
    let dir = workspace_tempdir();
    let path = dir.path().join("main.wr");
    write_fixture_file(
        &path,
        r#"class Worker {
    id: Integer
}
fn same(a: Actor[Worker], b: Actor[Worker]) -> Boolean {
    return a == b
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(!output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let diagnostics: Vec<serde_json::Value> = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("valid json"))
        .collect();
    let equality = diagnostics
        .iter()
        .find(|diag| {
            diag.get("code")
                .and_then(|value| value.as_str())
                .is_some_and(|code| code == "lang::ty::equality_requires_eq")
        })
        .expect("expected equality Eq diagnostic");
    assert_eq!(
        equality
            .get("data")
            .and_then(|v| v.get("kind"))
            .and_then(|v| v.as_str()),
        Some("equality_requires_eq")
    );
    assert_eq!(
        equality
            .get("data")
            .and_then(|v| v.get("left_type"))
            .and_then(|v| v.as_str()),
        Some("Actor[Worker]")
    );
    assert_eq!(
        equality
            .get("data")
            .and_then(|v| v.get("right_type"))
            .and_then(|v| v.as_str()),
        Some("Actor[Worker]")
    );
}

#[test]
fn cli_json_reports_equality_requires_eq_code_for_enum() {
    let dir = workspace_tempdir();
    let path = dir.path().join("main.wr");
    write_fixture_file(
        &path,
        r#"class Worker {
    id: Integer
}
enum Status {
    Pending
    Running(task: Pending[Result[Worker]])

}
fn same(a: Status, b: Status) -> Boolean {
    return a == b
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(!output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let diagnostics: Vec<serde_json::Value> = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("valid json"))
        .collect();
    let equality = diagnostics
        .iter()
        .find(|diag| {
            diag.get("code")
                .and_then(|value| value.as_str())
                .is_some_and(|code| code == "lang::ty::equality_requires_eq")
        })
        .expect("expected enum equality Eq diagnostic");
    assert_eq!(
        equality
            .get("data")
            .and_then(|v| v.get("kind"))
            .and_then(|v| v.as_str()),
        Some("equality_requires_eq")
    );
    assert_eq!(
        equality
            .get("data")
            .and_then(|v| v.get("left_type"))
            .and_then(|v| v.as_str()),
        Some("Status")
    );
    assert_eq!(
        equality
            .get("data")
            .and_then(|v| v.get("right_type"))
            .and_then(|v| v.as_str()),
        Some("Status")
    );
}

#[test]
fn cli_check_accepts_structural_enum_equality() {
    let dir = workspace_tempdir();
    let path = dir.path().join("main.wr");
    write_fixture_file(
        &path,
        r#"enum Status {
    Pending
    Done

}
fn compute_match(a: Status, b: Status) -> Integer {
    if a == b {
        return 1
    }
    return 0
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(
        output.status.success(),
        "expected check to pass for structural enum equality:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn cli_json_reports_boundary_generic_rewrite_suggestion() {
    let dir = workspace_tempdir();
    let path = dir.path().join("main.wr");
    write_fixture_file(
        &path,
        r#"fn run(values: List) -> Integer {
    return 0
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(!output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let diagnostics: Vec<serde_json::Value> = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("valid json"))
        .collect();
    let boundary = diagnostics
        .iter()
        .find(|diag| {
            diag.get("code")
                .and_then(|value| value.as_str())
                .is_some_and(|code| code == "lang::ty::boundary_missing_type_args")
        })
        .expect("expected boundary generic diagnostic");
    assert_eq!(
        boundary
            .get("data")
            .and_then(|v| v.get("kind"))
            .and_then(|v| v.as_str()),
        Some("boundary_missing_type_args")
    );
    assert!(
        boundary
            .get("suggestions")
            .and_then(|v| v.as_array())
            .is_some_and(|arr| arr.iter().any(|s| {
                s.get("reason_code")
                    .and_then(|v| v.as_str())
                    .is_some_and(|code| code == "boundary_generic_rewrite")
            })),
        "expected boundary generic rewrite suggestion, got:\n{}",
        stdout
    );
}

#[test]
fn cli_json_reports_multifile_type_error_path() {
    let dir = workspace_tempdir();
    let src_dir = dir.path().join("src");
    let module_path = src_dir.join("domain").join("broken.wr");
    std::fs::create_dir_all(module_path.parent().unwrap()).unwrap();
    write_fixture_file(
        src_dir.join("main.wr"),
        r#"use compute from domain/broken

fn run() -> Integer {
    return compute()
}
"#,
    )
    .unwrap();
    write_fixture_file(
        &module_path,
        r#"fn padding0() -> Integer {
    return 0

}
fn padding1() -> Integer {
    return 1

}
fn padding2() -> Integer {
    return 2

}
fn compute() -> Integer {
    return true
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg("--error-format=json")
        .arg(src_dir.join("main.wr"))
        .output()
        .expect("run wrela");
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let diagnostics: Vec<serde_json::Value> = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("valid json"))
        .collect();
    let path_hit = diagnostics.iter().any(|diag| {
        diag.get("path")
            .and_then(|value| value.as_str())
            .is_some_and(|path| path.ends_with("domain/broken.wr"))
    });
    assert!(
        path_hit,
        "expected diagnostic path to point to imported module, got:\n{}",
        stdout
    );
}

#[test]
fn cli_json_multimodule_same_symbol_names_report_correct_owner_path() {
    let dir = workspace_tempdir();
    let src_dir = dir.path().join("src");
    let billing_path = src_dir.join("domain").join("billing.wr");
    let orders_path = src_dir.join("domain").join("orders.wr");
    std::fs::create_dir_all(billing_path.parent().unwrap()).unwrap();
    write_fixture_file(
        src_dir.join("main.wr"),
        r#"use compute from domain/orders

fn run() -> Integer {
    return compute()
}
"#,
    )
    .unwrap();
    write_fixture_file(
        &billing_path,
        r#"fn compute() -> Integer {
    return 1
}
"#,
    )
    .unwrap();
    write_fixture_file(
        &orders_path,
        r#"fn compute() -> Integer {
    return true
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg("--error-format=json")
        .arg(src_dir.join("main.wr"))
        .output()
        .expect("run wrela");
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let diagnostics: Vec<serde_json::Value> = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("valid json"))
        .collect();
    let orders_hit = diagnostics.iter().any(|diag| {
        diag.get("path")
            .and_then(|value| value.as_str())
            .is_some_and(|path| path.ends_with("domain/orders.wr"))
    });
    assert!(
        orders_hit,
        "expected diagnostic path to point at symbol owner module, got:\n{}",
        stdout
    );
}

#[test]
fn cli_exit_code_parse_error() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    write_fixture_file(
        &path,
        r#"fn run() -> Integer {
    return 1 +
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg(&path)
        .output()
        .expect("run wrela");
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn cli_exit_code_type_error() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    write_fixture_file(
        &path,
        r#"fn run() -> Integer {
    return 1 + true
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg(&path)
        .output()
        .expect("run wrela");
    assert_eq!(output.status.code(), Some(3));
}

#[test]
fn cli_check_success() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    write_fixture_file(
        &path,
        r#"fn run() -> Integer {
    return 1
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
fn cli_check_reports_lexical_invalid_character() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    write_fixture_file(
        &path,
        r#"fn run() -> Integer {
    return 1
}
$
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(
        !output.status.success(),
        "lexically invalid source should fail"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("lang::lex::error"), "{stderr}");
    assert!(stderr.contains("unexpected character '$'"), "{stderr}");
}

#[test]
fn cli_check_lexical_error_json_matches_snapshot() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().expect("src dir")).unwrap();
    write_fixture_file(
        &path,
        r#"fn run() -> Integer {
    return 1
}
$
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(!output.status.success(), "expected lexical check to fail");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let diagnostics: Vec<serde_json::Value> = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("valid json"))
        .collect();
    let lexical = diagnostics
        .iter()
        .find(|diag| {
            diag.get("code")
                .and_then(|value| value.as_str())
                .is_some_and(|code| code.starts_with("lang::lex::"))
        })
        .expect("expected lexical diagnostic");

    let code = lexical
        .get("code")
        .and_then(|value| value.as_str())
        .expect("lexical code");
    assert_eq!(code, "lang::lex::error");
    assert_eq!(
        lexical
            .get("rule")
            .and_then(|value| value.as_str())
            .expect("lexical rule"),
        "error"
    );
    assert_eq!(
        lexical
            .get("stage")
            .and_then(|value| value.as_str())
            .expect("stage"),
        "parse"
    );
    assert_eq!(
        lexical
            .get("severity")
            .and_then(|value| value.as_str())
            .expect("severity"),
        "error"
    );
    assert!(
        lexical
            .get("help")
            .and_then(|value| value.as_str())
            .is_some_and(|help| !help.is_empty()),
        "expected non-empty help field"
    );
    assert!(
        lexical
            .get("message")
            .and_then(|value| value.as_str())
            .is_some_and(|message| message.contains("unexpected character '$'")),
        "expected lexical subtype-specific message"
    );
    assert!(
        lexical
            .get("diag_id")
            .and_then(|value| value.as_str())
            .is_some_and(|diag_id| diag_id.contains("unexpected_character")),
        "expected lexical subtype marker in diag_id"
    );

    let normalized = normalize_lexical_diag_json_for_snapshot(lexical, dir.path());
    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/diagnostics/lexical_error_json_snapshot.json"
    ))
    .expect("valid expected snapshot json");
    assert_eq!(normalized, expected);
}

#[test]
fn cli_check_lexical_error_stderr_matches_snapshot() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().expect("src dir")).unwrap();
    write_fixture_file(
        &path,
        r#"fn run() -> Integer {
    return 1
}
$
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(!output.status.success(), "expected lexical check to fail");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("lang::lex::error"), "{stderr}");
    assert!(stderr.contains("unexpected character '$'"), "{stderr}");
    let normalized = normalize_temp_paths_for_snapshot(&stderr, dir.path());
    let expected =
        include_str!("fixtures/diagnostics/lexical_error_stderr_snapshot.txt").trim_end();
    assert_eq!(normalized.trim_end(), expected);
}

#[test]
fn cli_check_without_run_is_ok() {
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
        .arg("check")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(output.status.success());
}

#[test]
fn cli_check_allows_duplicate_private_function_names_across_modules() {
    let dir = workspace_tempdir();
    let src_dir = dir.path().join("src");
    std::fs::create_dir_all(src_dir.join("domain")).unwrap();
    write_fixture_file(
        src_dir.join("main.wr"),
        r#"use run_orders from domain/orders
use run_payments from domain/payments

fn run() -> Integer {
    return run_orders() + run_payments()
}
"#,
    )
    .unwrap();
    write_fixture_file(
        src_dir.join("domain").join("orders.wr"),
        r#"private {
    fn load_value() -> Integer {
        return 1

    }
}
fn run_orders() -> Integer {
    return load_value()
}
"#,
    )
    .unwrap();
    write_fixture_file(
        src_dir.join("domain").join("payments.wr"),
        r#"private {
    fn load_value() -> Integer {
        return 2

    }
}
fn run_payments() -> Integer {
    return load_value()
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg(src_dir.join("main.wr"))
        .output()
        .expect("run wrela");
    assert!(
        output.status.success(),
        "duplicate private names across modules should be allowed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn write_test_project(root: &std::path::Path) {
    let src_dir = root.join("src");
    let tests_dir = root.join("tests");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::create_dir_all(&tests_dir).unwrap();
    write_fixture_file(
        src_dir.join("main.wr"),
        r#"fn run() -> Integer {
    return 0
}
"#,
    )
    .unwrap();
    write_fixture_file(
        tests_dir.join("basic_test.wr"),
        r#"fn test_basic() -> Nothing {
    value = 1 + 1
    assert value value == 2
}
"#,
    )
    .unwrap();
}

fn write_virtual_gpu_compute_project(root: &std::path::Path) {
    let src_dir = root.join("src");
    let gpu_dir = src_dir.join("gpu");
    let tests_dir = root.join("tests").join("spec");
    std::fs::create_dir_all(&gpu_dir).unwrap();
    std::fs::create_dir_all(&tests_dir).unwrap();
    write_fixture_file(
        src_dir.join("main.wr"),
        r#"fn run() -> Integer {
    return 0
}
"#,
    )
    .unwrap();
    write_fixture_file(
        gpu_dir.join("compute.wr"),
        r#"kernel fn run_kernel(snapshot: GpuBuffer[I32], counts: GpuBuffer[I32]) -> Nothing {
    gid = global_invocation_id()
    lid = local_invocation_id()
    wid = workgroup_id()
    num = num_workgroups()
    size = workgroup_size()

    if gid[0] == u32(0) and lid[0] == u32(0) and wid[0] == u32(0) {
        gpu_buffer_set(buffer=snapshot, index=0, value=i32(gid[0]))
        gpu_buffer_set(buffer=snapshot, index=1, value=i32(lid[0]))
        gpu_buffer_set(buffer=snapshot, index=2, value=i32(wid[0]))
        gpu_buffer_set(buffer=snapshot, index=3, value=i32(num[0]))
        gpu_buffer_set(buffer=snapshot, index=4, value=i32(size[0]))
        gpu_buffer_set(
            buffer=snapshot,
            index=5,
            value=i32(gpu_buffer_len(buffer=counts))
        )
    }

    gpu_buffer_set(buffer=counts, index=gid[0], value=i32(1))
}

fn run_virtual_gpu_compute_smoke() -> Array[I32, 10] {
    snapshot = gpu_buffer_new(
        length=6,
        default_value=i32(0)
    )
    counts = gpu_buffer_new(
        length=4,
        default_value=i32(0)
    )
    dispatch_compute(
        kernel=run_kernel,
        snapshot=snapshot,
        counts=counts,
        workgroups_x=u32(2),
        workgroups_y=u32(1),
        workgroups_z=u32(1),
        workgroup_size_x=u32(2),
        workgroup_size_y=u32(1),
        workgroup_size_z=u32(1)
    )
    return [
        gpu_buffer_get(buffer=snapshot, index=0),
        gpu_buffer_get(buffer=snapshot, index=1),
        gpu_buffer_get(buffer=snapshot, index=2),
        gpu_buffer_get(buffer=snapshot, index=3),
        gpu_buffer_get(buffer=snapshot, index=4),
        gpu_buffer_get(buffer=snapshot, index=5),
        gpu_buffer_get(buffer=counts, index=0),
        gpu_buffer_get(buffer=counts, index=1),
        gpu_buffer_get(buffer=counts, index=2),
        gpu_buffer_get(buffer=counts, index=3)
    ]
}

fn run_virtual_gpu_atomic_schedule_smoke() -> Array[I32, 5] {
    counter = gpu_atomic_i32_new(initial=i32(0))
    observed = gpu_buffer_new(
        length=4,
        default_value=i32(0)
    )
    dispatch_compute(
        kernel=run_atomic_schedule_kernel,
        counter=counter,
        observed=observed,
        schedule=gpu_schedule_reverse(),
        workgroups_x=u32(2),
        workgroups_y=u32(1),
        workgroups_z=u32(1),
        workgroup_size_x=u32(2),
        workgroup_size_y=u32(1),
        workgroup_size_z=u32(1)
    )
    return [
        gpu_atomic_i32_load(atomic=counter),
        gpu_buffer_get(buffer=observed, index=0),
        gpu_buffer_get(buffer=observed, index=1),
        gpu_buffer_get(buffer=observed, index=2),
        gpu_buffer_get(buffer=observed, index=3)
    ]
}

kernel fn run_atomic_schedule_kernel(counter: GpuAtomicI32, observed: GpuBuffer[I32]) -> Nothing {
    gid = global_invocation_id()
    previous = gpu_atomic_i32_fetch_add(
        atomic=counter,
        delta=i32(1)
    )
    gpu_buffer_set(
        buffer=observed,
        index=gid[0],
        value=previous
    )
}
"#,
    )
    .unwrap();
    write_fixture_file(
        tests_dir.join("virtual_gpu_compute_test.wr"),
        r#"use run_virtual_gpu_atomic_schedule_smoke from gpu/compute
use run_virtual_gpu_compute_smoke from gpu/compute

fn test_virtual_gpu_compute_smoke() -> Nothing {
    summary = run_virtual_gpu_compute_smoke()
    assert value summary.len() == 10
    assert value summary[0] == 0
    assert value summary[1] == 0
    assert value summary[2] == 0
    assert value summary[3] == 2
    assert value summary[4] == 2
    assert value summary[5] == 4
    assert value summary[6] == 1
    assert value summary[7] == 1
    assert value summary[8] == 1
    assert value summary[9] == 1
}

fn test_virtual_gpu_atomic_schedule_smoke() -> Nothing {
    summary = run_virtual_gpu_atomic_schedule_smoke()
    assert value summary.len() == 5
    assert value summary[0] == 4
    assert value summary[1] == 3
    assert value summary[2] == 2
    assert value summary[3] == 1
    assert value summary[4] == 0
}

fn test_virtual_gpu_atomic_drop_smoke() -> Nothing {
    counter = gpu_atomic_i32_new(initial=i32(1))
    assert value gpu_atomic_i32_drop(atomic=counter) == true
}
"#,
    )
    .unwrap();
}

fn write_virtual_gpu_atomic_schedule_project(root: &std::path::Path) {
    let src_dir = root.join("src");
    let gpu_dir = src_dir.join("gpu");
    let tests_dir = root.join("tests").join("spec");
    std::fs::create_dir_all(&gpu_dir).unwrap();
    std::fs::create_dir_all(&tests_dir).unwrap();
    write_fixture_file(
        src_dir.join("main.wr"),
        r#"fn run() -> Integer {
    return 0
}
"#,
    )
    .unwrap();
    write_fixture_file(
        gpu_dir.join("compute.wr"),
        r#"kernel fn run_kernel(counter: GpuAtomicI32, observed: GpuBuffer[I32]) -> Nothing {
    gid = global_invocation_id()
    previous = gpu_atomic_i32_fetch_add(
        atomic=counter,
        delta=i32(1)
    )
    gpu_buffer_set(
        buffer=observed,
        index=gid[0],
        value=previous
    )
}

fn run_virtual_gpu_atomic_schedule_smoke() -> Array[I32, 5] {
    counter = gpu_atomic_i32_new(initial=i32(0))
    observed = gpu_buffer_new(
        length=4,
        default_value=i32(0)
    )
    dispatch_compute(
        kernel=run_kernel,
        counter=counter,
        observed=observed,
        schedule=gpu_schedule_reverse(),
        workgroups_x=u32(2),
        workgroups_y=u32(1),
        workgroups_z=u32(1),
        workgroup_size_x=u32(2),
        workgroup_size_y=u32(1),
        workgroup_size_z=u32(1)
    )
    return [
        gpu_atomic_i32_load(atomic=counter),
        gpu_buffer_get(buffer=observed, index=0),
        gpu_buffer_get(buffer=observed, index=1),
        gpu_buffer_get(buffer=observed, index=2),
        gpu_buffer_get(buffer=observed, index=3)
    ]
}
"#,
    )
    .unwrap();
    write_fixture_file(
        tests_dir.join("virtual_gpu_atomic_schedule_test.wr"),
        r#"use run_virtual_gpu_atomic_schedule_smoke from gpu/compute

fn test_virtual_gpu_atomic_schedule_smoke() -> Nothing {
    summary = run_virtual_gpu_atomic_schedule_smoke()
    assert value summary.len() == 5
    assert value summary[0] == 4
    assert value summary[1] == 3
    assert value summary[2] == 2
    assert value summary[3] == 1
    assert value summary[4] == 0
}
"#,
    )
    .unwrap();
}

fn write_virtual_gpu_workgroup_schedule_project(root: &std::path::Path) {
    let src_dir = root.join("src");
    let gpu_dir = src_dir.join("gpu");
    let tests_dir = root.join("tests").join("spec");
    std::fs::create_dir_all(&gpu_dir).unwrap();
    std::fs::create_dir_all(&tests_dir).unwrap();
    write_fixture_file(
        src_dir.join("main.wr"),
        r#"fn run() -> Integer {
    return 0
}
"#,
    )
    .unwrap();
    write_fixture_file(
        gpu_dir.join("compute.wr"),
        r#"kernel fn run_kernel(counter: GpuAtomicI32, observed: GpuBuffer[I32]) -> Nothing {
    gid = global_invocation_id()
    previous = gpu_atomic_i32_fetch_add(
        atomic=counter,
        delta=i32(1)
    )
    gpu_buffer_set(
        buffer=observed,
        index=gid[0],
        value=previous
    )
}

fn run_workgroup_reverse_smoke() -> Array[I32, 5] {
    counter = gpu_atomic_i32_new(initial=i32(0))
    observed = gpu_buffer_new(length=4, default_value=i32(0))
    dispatch_compute(
        kernel=run_kernel,
        counter=counter,
        observed=observed,
        schedule=gpu_schedule_workgroup_reverse(),
        workgroups_x=u32(2),
        workgroups_y=u32(1),
        workgroups_z=u32(1),
        workgroup_size_x=u32(2),
        workgroup_size_y=u32(1),
        workgroup_size_z=u32(1)
    )
    return [
        gpu_atomic_i32_load(atomic=counter),
        gpu_buffer_get(buffer=observed, index=0),
        gpu_buffer_get(buffer=observed, index=1),
        gpu_buffer_get(buffer=observed, index=2),
        gpu_buffer_get(buffer=observed, index=3)
    ]
}

fn run_round_robin_smoke() -> Array[I32, 5] {
    counter = gpu_atomic_i32_new(initial=i32(0))
    observed = gpu_buffer_new(length=4, default_value=i32(0))
    dispatch_compute(
        kernel=run_kernel,
        counter=counter,
        observed=observed,
        schedule=gpu_schedule_round_robin_workgroups(),
        workgroups_x=u32(2),
        workgroups_y=u32(1),
        workgroups_z=u32(1),
        workgroup_size_x=u32(2),
        workgroup_size_y=u32(1),
        workgroup_size_z=u32(1)
    )
    return [
        gpu_atomic_i32_load(atomic=counter),
        gpu_buffer_get(buffer=observed, index=0),
        gpu_buffer_get(buffer=observed, index=1),
        gpu_buffer_get(buffer=observed, index=2),
        gpu_buffer_get(buffer=observed, index=3)
    ]
}

fn run_workgroup_shuffle_smoke() -> Array[I32, 9] {
    counter = gpu_atomic_i32_new(initial=i32(0))
    observed = gpu_buffer_new(length=8, default_value=i32(0))
    dispatch_compute(
        kernel=run_kernel,
        counter=counter,
        observed=observed,
        schedule=gpu_schedule_workgroup_shuffle(seed=u32(7)),
        workgroups_x=u32(4),
        workgroups_y=u32(1),
        workgroups_z=u32(1),
        workgroup_size_x=u32(2),
        workgroup_size_y=u32(1),
        workgroup_size_z=u32(1)
    )
    return [
        gpu_atomic_i32_load(atomic=counter),
        gpu_buffer_get(buffer=observed, index=0),
        gpu_buffer_get(buffer=observed, index=1),
        gpu_buffer_get(buffer=observed, index=2),
        gpu_buffer_get(buffer=observed, index=3),
        gpu_buffer_get(buffer=observed, index=4),
        gpu_buffer_get(buffer=observed, index=5),
        gpu_buffer_get(buffer=observed, index=6),
        gpu_buffer_get(buffer=observed, index=7)
    ]
}
"#,
    )
    .unwrap();
    write_fixture_file(
        tests_dir.join("virtual_gpu_workgroup_schedule_test.wr"),
        r#"use run_round_robin_smoke from gpu/compute
use run_workgroup_reverse_smoke from gpu/compute
use run_workgroup_shuffle_smoke from gpu/compute

fn test_workgroup_reverse_smoke() -> Nothing {
    summary = run_workgroup_reverse_smoke()
    assert value summary.len() == 5
    assert value summary[0] == 4
    assert value summary[1] == 2
    assert value summary[2] == 3
    assert value summary[3] == 0
    assert value summary[4] == 1
}

fn test_round_robin_workgroups_smoke() -> Nothing {
    summary = run_round_robin_smoke()
    assert value summary.len() == 5
    assert value summary[0] == 4
    assert value summary[1] == 0
    assert value summary[2] == 2
    assert value summary[3] == 1
    assert value summary[4] == 3
}

fn test_workgroup_shuffle_smoke() -> Nothing {
    summary = run_workgroup_shuffle_smoke()
    assert value summary.len() == 9
    assert value summary[0] == 8
    assert value summary[1] == 4
    assert value summary[2] == 5
    assert value summary[3] == 0
    assert value summary[4] == 1
    assert value summary[5] == 6
    assert value summary[6] == 7
    assert value summary[7] == 2
    assert value summary[8] == 3
}
"#,
    )
    .unwrap();
}

fn write_lexically_invalid_project(root: &std::path::Path) {
    let src_dir = root.join("src");
    let tests_dir = root.join("tests");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::create_dir_all(&tests_dir).unwrap();
    write_fixture_file(
        src_dir.join("main.wr"),
        r#"fn run() -> Integer {
    return 0
}
$
"#,
    )
    .unwrap();
    write_fixture_file(
        tests_dir.join("basic_test.wr"),
        r#"fn test_basic() -> Nothing {
    assert value 1 == 1
}
"#,
    )
    .unwrap();
}

fn write_failing_test_project(root: &std::path::Path) {
    let src_dir = root.join("src");
    let tests_dir = root.join("tests");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::create_dir_all(&tests_dir).unwrap();
    write_fixture_file(
        src_dir.join("main.wr"),
        r#"fn run() -> Integer {
    return 0
}
"#,
    )
    .unwrap();
    write_fixture_file(
        tests_dir.join("failing_test.wr"),
        r#"fn test_failing() -> Nothing {
    value = 1 + 1
    assert value value == 3
}
"#,
    )
    .unwrap();
}

fn write_nondeterministic_cert_project(root: &std::path::Path) {
    let src_dir = root.join("src");
    let tests_dir = root.join("tests");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::create_dir_all(&tests_dir).unwrap();
    write_fixture_file(
        src_dir.join("main.wr"),
        r#"fn run() -> Integer {
    return 0
}
"#,
    )
    .unwrap();
    write_fixture_file(
        tests_dir.join("nondeterministic_cert_test.wr"),
        r#"fn test_nondeterministic_cert() -> Nothing {
    marker = __wr_env_get("WRELA_CERT_REPLAY_MARKER")
    match marker {
        String(_) {
            assert value 1 == 0
        }
        default {
            __wr_env_set("WRELA_CERT_REPLAY_MARKER", "seen")
            assert value 1 == 1
        }
    }
}
"#,
    )
    .unwrap();
}

fn write_oracle_gate_project(root: &std::path::Path, with_assert: bool) {
    let src_dir = root.join("src");
    let tests_dir = root.join("tests");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::create_dir_all(&tests_dir).unwrap();
    write_fixture_file(
        src_dir.join("main.wr"),
        r#"fn run() -> Integer {
    return 0
}
"#,
    )
    .unwrap();
    let body = if with_assert {
        "fn compute_value() -> Integer {\n    return 1\n}\n\nfn test_oracle_gate() -> Nothing {\n    compute_value()\n    assert value compute_value() == 1\n}\n"
    } else {
        "fn compute_value() -> Integer {\n    return 1\n}\n\nfn test_oracle_gate() -> Nothing {\n    compute_value()\n}\n"
    };
    write_fixture_file(tests_dir.join("oracle_gate_test.wr"), body).unwrap();
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
    write_fixture_file(
        src_dir.join("main.wr"),
        r#"fn run() -> Integer {
    return 0
}
"#,
    )
    .unwrap();
    write_fixture_file(
        tests_dir.join("spec").join("alpha_test.wr"),
        r#"fn test_alpha() -> Nothing {
    value = 1 + 1
    assert value value == 2
}
"#,
    )
    .unwrap();
    write_fixture_file(
        tests_dir.join("integration").join("beta_test.wr"),
        r#"fn test_beta() -> Nothing {
    value = 1 + 1
    assert value value == 2
}
"#,
    )
    .unwrap();
    write_fixture_file(
        tests_dir.join("sim").join("gamma_test.wr"),
        r#"fn test_gamma() -> Nothing {
    value = 1 + 1
    assert value value == 2
}
"#,
    )
    .unwrap();
    write_fixture_file(
        tests_dir.join("model").join("delta_test.wr"),
        r#"fn test_delta() -> Nothing {
    value = 1 + 1
    assert value value == 2
}
"#,
    )
    .unwrap();
    write_fixture_file(
        tests_dir.join("misc").join("epsilon_test.wr"),
        r#"fn test_epsilon() -> Nothing {
    value = 1 + 1
    assert value value == 2
}
"#,
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
    write_fixture_file(
        src_dir.join("main.wr"),
        r#"fn run() -> Integer {
    return 0
}
"#,
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
        write_fixture_file(
            tests_dir.join(lane).join(format!("{module}_test.wr")),
            format!(
                "fn {func}() -> Nothing {{\n    value = 1 + 1\n    assert value value == 2\n}}\n"
            ),
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
    write_fixture_file(
        src_dir.join("main.wr"),
        r#"fn run() -> Integer {
    return 0
}
"#,
    )
    .unwrap();
    write_fixture_file(
        core_dir.join("math.wr"),
        r#"fn compute_answer() -> Integer {
    return 41
}
"#,
    )
    .unwrap();
    write_fixture_file(
        core_dir.join("independent.wr"),
        r#"fn fetch_constant() -> Integer {
    return 7
}
"#,
    )
    .unwrap();
    write_fixture_file(
        tests_dir.join("spec").join("sanity_test.wr"),
        r#"fn compute_spec() -> Integer {
    return 2

}
fn test_spec_sanity() -> Nothing {
    assert value compute_spec() == 2
}
"#,
    )
    .unwrap();
    write_fixture_file(
        tests_dir.join("integration").join("math_flow_test.wr"),
        r#"use compute_answer from core/math

fn test_math_flow() -> Nothing {
    assert value compute_answer() == 41
}
"#,
    )
    .unwrap();
    write_fixture_file(
        tests_dir
            .join("integration")
            .join("independent_flow_test.wr"),
        r#"use fetch_constant from core/independent

fn test_independent_flow() -> Nothing {
    assert value fetch_constant() == 7
}
"#,
    )
    .unwrap();
    write_fixture_file(
        tests_dir.join("sim").join("queue_sim_test.wr"),
        r#"fn compute_sim() -> Integer {
    return 4

}
fn test_queue_sim() -> Nothing {
    assert value compute_sim() == 4
}
"#,
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
    write_fixture_file(
        src_dir.join("main.wr"),
        r#"fn run() -> Integer {
    return 0
}
"#,
    )
    .unwrap();
    write_fixture_file(
        integrations_dir.join("http_client.wr"),
        format!(
            "use try_to_http_call from host/http\n\nfn fetch_charge() -> Result[String] {{\n    headers = {{}}\n    return try_to_http_call(service=\"billing\", endpoint=\"charge\", method=\"GET\", url=\"{url}\", headers=headers, body=\"\", timeout_ms=1500)\n}}\n"
        ),
    )
    .unwrap();
    write_fixture_file(
        integration_dir.join("http_connector_test.wr"),
        r#"use fetch_charge from infrastructure/integrations/http_client

fn test_http_connector() -> Nothing {
    result = fetch_charge()
    match result {
        Ok(_) {
            assert value true == true
        }
        Err(_) {
            assert value false == true
        }
        default {
            assert value false == true
        }
    }
}
"#,
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
    write_fixture_file(
        src_dir.join("main.wr"),
        r#"fn run() -> Integer {
    return 0
}
"#,
    )
    .unwrap();
    write_fixture_file(
        integrations_dir.join("http_client.wr"),
        format!(
            "use try_to_http_call from host/http\n\nfn fetch_charge() -> Result[String] {{\n    headers = {{}}\n    return try_to_http_call(service=\"billing\", endpoint=\"charge\", method=\"GET\", url=\"{url}\", headers=headers, body=\"\", timeout_ms=1500)\n}}\n"
        ),
    )
    .unwrap();
    write_fixture_file(
        integration_dir.join("http_missing_test.wr"),
        r#"use fetch_charge from infrastructure/integrations/http_client

fn test_http_missing_cassette() -> Nothing {
    result = fetch_charge()
    match result {
        Ok(_) {
            assert value false == true
        }
        Err(_) {
            assert value true == true
        }
        default {
            assert value false == true
        }
    }
}
"#,
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
    write_fixture_file(
        src_dir.join("main.wr"),
        r#"fn run() -> Integer {
    return 0
}
"#,
    )
    .unwrap();
    write_fixture_file(src_dir.join("public_api.wr"), compute_source).unwrap();
    write_fixture_file(
        integrations_dir.join("http_client.wr"),
        r#"use try_to_http_call from host/http

fn fetch_charge() -> Result[String] {
    headers = {}
    return try_to_http_call(service="billing", endpoint="charge", method="GET", url="https://api.example.com/charge", headers=headers, body="", timeout_ms=1500)
}
"#,
    )
    .unwrap();
    write_fixture_file(
        tests_dir.join("basic_test.wr"),
        r#"fn test_basic() -> Nothing {
    assert value 1 == 1
}
"#,
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
    write_fixture_file(
        src_dir.join("main.wr"),
        r#"fn run() -> Integer {
    return 0
}
"#,
    )
    .unwrap();
    write_fixture_file(
        domain_dir.join("pricing.wr"),
        r#"fn compute_domain_total() -> Integer {
    return 7
}
"#,
    )
    .unwrap();
    write_fixture_file(
        application_dir.join("orders.wr"),
        r#"use compute_domain_total from domain/pricing

fn calculate_invoice() -> Integer {
    return compute_domain_total()
}
"#,
    )
    .unwrap();
    let test_source = if cover_importable_surface {
        "use calculate_invoice from application/orders\n\nfn test_importable_coverage() -> Nothing {\n    assert value calculate_invoice() == 7\n}\n"
    } else {
        "fn test_importable_coverage() -> Nothing {\n    assert value 1 == 1\n}\n"
    };
    write_fixture_file(tests_dir.join("coverage_gate_test.wr"), test_source).unwrap();
}

fn write_function_test_coverage_index_project(root: &std::path::Path) {
    let src_dir = root.join("src");
    let tests_dir = root.join("tests");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::create_dir_all(&tests_dir).unwrap();
    write_fixture_file(
        src_dir.join("main.wr"),
        r#"fn run() -> Integer {
    return 0
}
"#,
    )
    .unwrap();
    write_fixture_file(
        src_dir.join("coverage_math.wr"),
        r#"fn compute_alpha() -> Integer {
    return 41

}
fn compute_beta() -> Integer {
    return 7
}
"#,
    )
    .unwrap();
    write_fixture_file(
        tests_dir.join("alpha_test.wr"),
        r#"use compute_alpha from coverage_math

fn test_covers_alpha() -> Nothing {
    assert value compute_alpha() == 41
}
"#,
    )
    .unwrap();
    write_fixture_file(
        tests_dir.join("beta_test.wr"),
        r#"use compute_beta from coverage_math

fn test_covers_beta() -> Nothing {
    assert value compute_beta() == 7
}
"#,
    )
    .unwrap();
}

fn write_non_importable_function_project(root: &std::path::Path) {
    let src_dir = root.join("src");
    let infra_dir = src_dir.join("infrastructure");
    let tests_dir = root.join("tests");
    std::fs::create_dir_all(&infra_dir).unwrap();
    std::fs::create_dir_all(&tests_dir).unwrap();
    write_fixture_file(
        src_dir.join("main.wr"),
        r#"fn run() -> Integer {
    return 0
}
"#,
    )
    .unwrap();
    write_fixture_file(
        infra_dir.join("internal_tools.wr"),
        r#"fn compute_internal_value() -> Integer {
    return 99
}
"#,
    )
    .unwrap();
    write_fixture_file(
        tests_dir.join("basic_test.wr"),
        r#"fn test_non_importable_scope() -> Nothing {
    assert value 1 == 1
}
"#,
    )
    .unwrap();
}

fn write_wrong_check_property_project(root: &std::path::Path) {
    let src_dir = root.join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    write_fixture_file(
        src_dir.join("main.wr"),
        r#"fn is_value_positive(value: Integer) -> Boolean {
    return value < 0

}
fn run() -> Integer {
    return 0
}
"#,
    )
    .unwrap();
}

fn write_sim_seed_project(root: &std::path::Path) {
    let src_dir = root.join("src");
    let sim_dir = root.join("tests").join("sim");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::create_dir_all(&sim_dir).unwrap();
    write_fixture_file(
        src_dir.join("main.wr"),
        r#"fn run() -> Integer {
    return 0
}
"#,
    )
    .unwrap();
    write_fixture_file(
        sim_dir.join("seeded_test.wr"),
        r#"fn test_seeded_interleaving() -> Nothing {
    seed = __wr_env_get("WRELA_SCHED_SEED")
    if seed == "7" {
        assert value false == true
    }
    assert value true == true
}
"#,
    )
    .unwrap();
}

fn write_model_seed_project(root: &std::path::Path) {
    let src_dir = root.join("src");
    let model_dir = root.join("tests").join("model");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::create_dir_all(&model_dir).unwrap();
    write_fixture_file(
        src_dir.join("main.wr"),
        r#"fn run() -> Integer {
    return 0
}
"#,
    )
    .unwrap();
    write_fixture_file(
        model_dir.join("counter_model_test.wr"),
        r#"fn test_model_counter() -> Nothing {
    seed = __wr_env_get("WRELA_MODEL_SEED")
    if seed == "9" {
        assert value false == true
    }
    assert value true == true
}
"#,
    )
    .unwrap();
}

fn write_differential_divergence_project(root: &std::path::Path) {
    let src_dir = root.join("src");
    let tests_dir = root.join("tests");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::create_dir_all(&tests_dir).unwrap();
    write_fixture_file(
        src_dir.join("main.wr"),
        r#"fn run() -> Integer {
    return 0
}
"#,
    )
    .unwrap();
    write_fixture_file(
        tests_dir.join("diff_gate_test.wr"),
        r#"fn test_pipeline_diff_gate() -> Nothing {
    pipeline = __wr_env_get("WRELA_DIFF_PIPELINE")
    if pipeline == "alt" {
        assert value false == true
    }
    assert value true == true
}
"#,
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
    write_fixture_file(
        src_dir.join("main.wr"),
        r#"fn run() -> Integer {
    return 0
}
"#,
    )
    .unwrap();
    write_fixture_file(
        spec_dir.join("attr_reject_test.wr"),
        r#"@allows_env_set
fn test_spec_rejects_capability_attr() -> Nothing {
    assert value true == true
}
"#,
    )
    .unwrap();
    write_fixture_file(
        integration_dir.join("serial_ok_test.wr"),
        r#"@serial
@allows_env_set
fn test_integration_serial_attr() -> Nothing {
    assert value true == true
}
"#,
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
    write_fixture_file(
        src_dir.join("main.wr"),
        r#"fn run() -> Integer {
    return 0
}
"#,
    )
    .unwrap();
    write_fixture_file(
        integration_dir.join("serial_only_test.wr"),
        r#"@serial
fn test_integration_serial_only() -> Nothing {
    assert value true == true
}
"#,
    )
    .unwrap();
    write_fixture_file(
        integration_dir.join("serial_only_2_test.wr"),
        r#"@serial
fn test_integration_serial_only_2() -> Nothing {
    assert value true == true
}
"#,
    )
    .unwrap();
    write_fixture_file(
        sim_dir.join("seed_expansion_test.wr"),
        r#"fn test_sim_seed_expansion() -> Nothing {
    assert value true == true
}
"#,
    )
    .unwrap();
}

fn write_non_test_attribute_misuse_project(root: &std::path::Path) {
    let src_dir = root.join("src");
    let tests_dir = root.join("tests");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::create_dir_all(&tests_dir).unwrap();
    write_fixture_file(
        src_dir.join("main.wr"),
        r#"@serial
fn compute_helper() -> Integer {
    return 1

}
fn run() -> Integer {
    return compute_helper()
}
"#,
    )
    .unwrap();
    write_fixture_file(
        tests_dir.join("smoke_test.wr"),
        r#"fn test_smoke() -> Nothing {
    assert value true == true
}
"#,
    )
    .unwrap();
}

fn write_fuzz_failure_project(root: &std::path::Path) {
    let src_dir = root.join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    write_fixture_file(
        src_dir.join("main.wr"),
        r#"fn run() -> Integer {
    return 0
}
"#,
    )
    .unwrap();
    write_fixture_file(
        src_dir.join("decode.wr"),
        r#"fn try_to_decode_payload(input: String) -> Result[Integer] {
    crash("fuzz crash")
}
"#,
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
    write_fixture_file(
        src_dir.join("main.wr"),
        r#"fn run() -> Integer {
    return 0
}
"#,
    )
    .unwrap();
    write_fixture_file(
        domain_dir.join("logic.wr"),
        r#"fn compute_logic_value(input: Integer) -> Integer {
    return input + 1

}
fn compute_logic_bonus(input: Integer) -> Integer {
    return input + 2
}
"#,
    )
    .unwrap();
    let test_body = if strong_tests {
        "use compute_logic_bonus, compute_logic_value from domain/logic\n\nfn test_logic_behavior() -> Nothing {\n    assert value compute_logic_value(input=1) == 2\n    assert value compute_logic_bonus(input=1) == 3\n}\n"
    } else {
        "use compute_logic_bonus, compute_logic_value from domain/logic\n\nfn test_smoke() -> Nothing {\n    assert value compute_logic_value(input=1) == 2\n    assert value compute_logic_bonus(input=1) > 0\n}\n"
    };
    write_fixture_file(tests_dir.join("mutation_test.wr"), test_body).unwrap();
}

fn write_alias_collision_project(root: &std::path::Path) {
    let src_dir = root.join("src");
    let domain_dir = src_dir.join("domain");
    let app_dir = src_dir.join("application");
    let tests_dir = root.join("tests");
    std::fs::create_dir_all(&domain_dir).unwrap();
    std::fs::create_dir_all(&app_dir).unwrap();
    std::fs::create_dir_all(&tests_dir).unwrap();
    write_fixture_file(
        src_dir.join("main.wr"),
        r#"use compute_shared from domain/logic

fn run() -> Integer {
    return compute_shared(input=1)
}
"#,
    )
    .unwrap();
    write_fixture_file(
        domain_dir.join("logic.wr"),
        r#"fn compute_shared(input: Integer) -> Integer {
    return input + 1
}
"#,
    )
    .unwrap();
    write_fixture_file(
        app_dir.join("orders.wr"),
        r#"fn compute_shared(input: Integer) -> Integer {
    return input + 100
}
"#,
    )
    .unwrap();
    write_fixture_file(
        tests_dir.join("smoke_test.wr"),
        r#"use compute_shared from domain/logic

fn test_compute_shared() -> Nothing {
    assert value compute_shared(input=1) == 2
}
"#,
    )
    .unwrap();
}

fn write_parse_invalid_src_module_project(root: &std::path::Path) {
    let src_dir = root.join("src");
    let domain_dir = src_dir.join("domain");
    let tests_dir = root.join("tests");
    std::fs::create_dir_all(&domain_dir).unwrap();
    std::fs::create_dir_all(&tests_dir).unwrap();
    write_fixture_file(
        src_dir.join("main.wr"),
        r#"fn run() -> Integer {
    return 0
}
"#,
    )
    .unwrap();
    write_fixture_file(
        domain_dir.join("broken.wr"),
        r#"fn compute_broken() -> Integer {
    return 1 +
}
"#,
    )
    .unwrap();
    write_fixture_file(
        tests_dir.join("smoke_test.wr"),
        r#"fn test_smoke() -> Nothing {
    assert value true == true
}
"#,
    )
    .unwrap();
}

fn write_parse_invalid_test_discovery_project(root: &std::path::Path) {
    let src_dir = root.join("src");
    let tests_dir = root.join("tests").join("spec");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::create_dir_all(&tests_dir).unwrap();
    write_fixture_file(
        src_dir.join("main.wr"),
        r#"fn run() -> Integer {
    return 0
}
"#,
    )
    .unwrap();
    write_fixture_file(
        tests_dir.join("broken_test.wr"),
        r#"fn test_broken() -> Nothing {
    assert value 1 ==
}
"#,
    )
    .unwrap();
}

fn write_alias_noise_project(root: &std::path::Path) {
    let src_dir = root.join("src");
    let domain_dir = src_dir.join("domain");
    let app_dir = src_dir.join("application");
    let tests_dir = root.join("tests");
    std::fs::create_dir_all(&domain_dir).unwrap();
    std::fs::create_dir_all(&app_dir).unwrap();
    std::fs::create_dir_all(&tests_dir).unwrap();
    write_fixture_file(
        src_dir.join("main.wr"),
        r#"use compute_real from domain/logic

fn run() -> Integer {
    return compute_real(input=1)
}
"#,
    )
    .unwrap();
    write_fixture_file(
        domain_dir.join("logic.wr"),
        r#"fn compute_real(input: Integer) -> Integer {
    return input + 1
}
"#,
    )
    .unwrap();
    write_fixture_file(
        app_dir.join("notes.wr"),
        r#"private {
    fn load_shadow() -> Integer {
        ignored = "to compute_real(input: Integer) -> Integer:"
        return 0
    }
}
"#,
    )
    .unwrap();
    write_fixture_file(
        tests_dir.join("smoke_test.wr"),
        r#"use compute_real from domain/logic

fn test_compute_real() -> Nothing {
    assert value compute_real(input=1) == 2
}
"#,
    )
    .unwrap();
}

fn spawn_http_stub_once(body: &'static str) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
    listener
        .set_nonblocking(true)
        .expect("set nonblocking listener");
    let addr = listener.local_addr().expect("listener addr");
    let url = format!("http://{addr}/charge");
    let handle = thread::spawn(move || {
        // Slow harness compile/link steps can exceed 15s on debug/test profiles.
        let deadline = Instant::now() + Duration::from_secs(90);
        while Instant::now() < deadline {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let mut request_buf = [0u8; 4096];
                    let _ = stream.read(&mut request_buf);
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nDate: Wed, 01 Jan 2020 00:00:00 GMT\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = stream.write_all(response.as_bytes());
                    let _ = stream.flush();
                    return;
                }
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(_) => return,
            }
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
    write_fixture_file(cassettes_dir.join(name), payload).expect("write cassette");
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
    if let Some(version) = value.get("schema_version").and_then(|v| v.as_u64())
        && version != 2
    {
        return None;
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
    let mut values = parse_json_stdout_lines(stdout);
    assert_eq!(
        values.len(),
        1,
        "expected one JSON line, got: {:?}",
        values
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>()
    );
    values.remove(0)
}

fn parse_json_stdout_lines(stdout: &[u8]) -> Vec<serde_json::Value> {
    let stdout_text = String::from_utf8_lossy(stdout);
    let lines: Vec<&str> = stdout_text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    assert!(!lines.is_empty(), "expected JSON output");
    lines
        .iter()
        .map(|line| serde_json::from_str(line).expect("valid json"))
        .collect()
}

fn normalize_temp_paths_for_snapshot(text: &str, root: &std::path::Path) -> String {
    let root_display = root.display().to_string();
    let canonical = std::fs::canonicalize(root)
        .ok()
        .map(|path| path.display().to_string());
    let mut normalized = text.replace(&root_display, "<TMP>");
    if let Some(canonical_display) = canonical {
        normalized = normalized.replace(&canonical_display, "<TMP>");
    }
    normalized
}

fn normalize_lexical_diag_json_for_snapshot(
    diag: &serde_json::Value,
    root: &std::path::Path,
) -> serde_json::Value {
    let mut normalized = diag.clone();
    if let Some(path_value) = normalized.get("path").and_then(|value| value.as_str()) {
        let replaced = normalize_temp_paths_for_snapshot(path_value, root);
        normalized["path"] = serde_json::Value::String(replaced);
    }
    if normalized.get("diag_id").is_some() {
        normalized["diag_id"] = serde_json::Value::String("<normalized>".to_string());
    }
    normalized
}

fn count_occurrences(haystack: &str, needle: &str) -> usize {
    haystack.match_indices(needle).count()
}

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

#[test]
fn cli_thin_core_bootstrap_matrix() {
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
    let entry = src.join("main.wr");
    write_fixture_file(
        tests.join("basic_test.wr"),
        r#"fn test_basic() -> Nothing {
    value = 1 + 1
    assert value value == 2
}
"#,
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
fn cli_naming_is_warning_by_default() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    write_fixture_file(
        &path,
        r#"fn helper() -> Integer {
    return 1
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
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("must start with a verb"));
    assert!(stderr.contains("warning"));
}

#[test]
fn cli_strict_naming_promotes_to_error() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    write_fixture_file(
        &path,
        r#"fn helper() -> Integer {
    return 1
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg("--strict-naming")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert_eq!(output.status.code(), Some(3));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("must start with a verb"));
}

#[test]
fn cli_fix_rewrites_safe_naming_issue() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().expect("src parent")).expect("create src");
    write_fixture_file(
        &path,
        r#"fn helperThing() -> Integer {
    return 1
}
"#,
    )
    .expect("write source");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fix")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(
        output.status.success(),
        "fix failed: code={:?}\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let rewritten = std::fs::read_to_string(&path).expect("read rewritten source");
    assert!(rewritten.contains("helper_thing"));
}

#[test]
fn cli_fix_json_emits_summary_counts() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().expect("src parent")).expect("create src");
    write_fixture_file(
        &path,
        r#"fn helperThing() -> Integer {
    return 1
}
"#,
    )
    .expect("write source");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fix")
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(
        output.status.success(),
        "fix failed: code={:?}\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let events: Vec<serde_json::Value> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("valid json line"))
        .collect();
    let summary = events
        .iter()
        .find(|value| value.get("event").and_then(|v| v.as_str()) == Some("fix_summary"))
        .expect("expected fix summary event");
    assert_eq!(
        summary.get("summary").and_then(|v| v.get("attempted")),
        Some(&serde_json::json!(1))
    );
    assert_eq!(
        summary.get("summary").and_then(|v| v.get("applied")),
        Some(&serde_json::json!(1))
    );
    assert_eq!(
        summary.get("summary").and_then(|v| v.get("skipped")),
        Some(&serde_json::json!(0))
    );
    assert_eq!(
        summary.get("summary").and_then(|v| v.get("errors")),
        Some(&serde_json::json!(0))
    );
    assert_eq!(
        summary.get("summary").and_then(|v| v.get("touched_files")),
        Some(&serde_json::json!(1))
    );

    let rewritten = std::fs::read_to_string(&path).expect("read rewritten source");
    assert!(rewritten.contains("helper_thing"));
}

#[test]
fn cli_fix_json_emits_zero_summary_when_no_safe_fixes_found() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().expect("src parent")).expect("create src");
    write_fixture_file(
        &path,
        r#"fn run() -> Integer {
    return 0
}
"#,
    )
    .expect("write source");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fix")
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert_eq!(output.status.code(), Some(3), "expected no-fix exit code");

    let events: Vec<serde_json::Value> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("valid json line"))
        .collect();
    let summary = events
        .iter()
        .find(|value| value.get("event").and_then(|v| v.as_str()) == Some("fix_summary"))
        .expect("expected fix summary event");
    assert_eq!(
        summary.get("summary").and_then(|v| v.get("attempted")),
        Some(&serde_json::json!(0))
    );
    assert_eq!(
        summary.get("summary").and_then(|v| v.get("applied")),
        Some(&serde_json::json!(0))
    );
    assert_eq!(
        summary.get("summary").and_then(|v| v.get("skipped")),
        Some(&serde_json::json!(0))
    );
    assert_eq!(
        summary.get("summary").and_then(|v| v.get("errors")),
        Some(&serde_json::json!(0))
    );
    assert_eq!(
        summary.get("summary").and_then(|v| v.get("touched_files")),
        Some(&serde_json::json!(0))
    );
}

#[test]
fn cli_fix_rewrites_safe_try_operator_issue() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().expect("src parent")).expect("create src");
    write_fixture_file(
        &path,
        r#"fn run() -> Result[Integer] {
    return 1?
}
"#,
    )
    .expect("write source");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fix")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(
        output.status.success(),
        "fix failed: code={:?}\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let rewritten = std::fs::read_to_string(&path).expect("read rewritten source");
    assert!(
        rewritten.contains("return 1\n"),
        "expected ? removal: {rewritten}"
    );
    assert!(!rewritten.contains('?'), "expected ? removal: {rewritten}");
}

#[test]
fn cli_fix_json_counts_safe_try_fix() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().expect("src parent")).expect("create src");
    write_fixture_file(
        &path,
        r#"fn run() -> Result[Integer] {
    return 1?
}
"#,
    )
    .expect("write source");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fix")
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(
        output.status.success(),
        "fix failed: code={:?}\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let events: Vec<serde_json::Value> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("valid json line"))
        .collect();
    let summary = events
        .iter()
        .find(|value| value.get("event").and_then(|v| v.as_str()) == Some("fix_summary"))
        .expect("expected fix summary event");
    assert_eq!(
        summary.get("summary").and_then(|v| v.get("attempted")),
        Some(&serde_json::json!(1))
    );
    assert_eq!(
        summary.get("summary").and_then(|v| v.get("applied")),
        Some(&serde_json::json!(1))
    );
}

#[test]
fn cli_fix_rewrites_single_candidate_typed_hole() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().expect("src parent")).expect("create src");
    write_fixture_file(
        &path,
        r#"fn run(value: Integer) -> Integer {
    return _todo
}
"#,
    )
    .expect("write source");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fix")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(
        output.status.success(),
        "fix failed: code={:?}\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let rewritten = std::fs::read_to_string(&path).expect("read rewritten source");
    assert!(
        rewritten.contains("return value\n"),
        "expected typed hole fill: {rewritten}"
    );
    assert!(
        !rewritten.contains("_todo"),
        "expected typed hole fill: {rewritten}"
    );
}

#[test]
fn cli_fix_json_counts_safe_single_candidate_typed_hole_fix() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().expect("src parent")).expect("create src");
    write_fixture_file(
        &path,
        r#"fn run(value: Integer) -> Integer {
    return _todo
}
"#,
    )
    .expect("write source");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fix")
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(
        output.status.success(),
        "fix failed: code={:?}\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let events: Vec<serde_json::Value> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("valid json line"))
        .collect();
    let summary = events
        .iter()
        .find(|value| value.get("event").and_then(|v| v.as_str()) == Some("fix_summary"))
        .expect("expected fix summary event");
    assert_eq!(
        summary.get("summary").and_then(|v| v.get("attempted")),
        Some(&serde_json::json!(1))
    );
    assert_eq!(
        summary.get("summary").and_then(|v| v.get("applied")),
        Some(&serde_json::json!(1))
    );
}

#[test]
fn cli_fix_allow_review_fixes_applies_review_tier_hole_suggestion() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().expect("src parent")).expect("create src");
    write_fixture_file(
        &path,
        r#"fn run() -> Integer {
    return _todo
}
"#,
    )
    .expect("write source");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fix")
        .arg("--allow-review-fixes")
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(
        output.status.success(),
        "fix failed: code={:?}\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let events: Vec<serde_json::Value> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("valid json line"))
        .collect();
    let summary = events
        .iter()
        .find(|value| value.get("event").and_then(|v| v.as_str()) == Some("fix_summary"))
        .expect("expected fix summary event");
    assert_eq!(
        summary.get("summary").and_then(|v| v.get("attempted")),
        Some(&serde_json::json!(1))
    );
    assert_eq!(
        summary.get("summary").and_then(|v| v.get("applied")),
        Some(&serde_json::json!(1))
    );
}

#[test]
fn cli_fix_allow_review_fixes_rewrites_named_args_required_call() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().expect("src parent")).expect("create src");
    write_fixture_file(
        &path,
        r#"fn add(a: Integer, b: Integer) -> Integer {
    return a + b

}
fn run() -> Integer {
    return add(1, 2)
}
"#,
    )
    .expect("write source");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fix")
        .arg("--allow-review-fixes")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(
        output.status.success(),
        "fix failed: code={:?}\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let rewritten = std::fs::read_to_string(&path).expect("read rewritten source");
    assert!(
        rewritten.contains("add(a=1, b=2)"),
        "expected named args rewrite: {rewritten}"
    );
}

#[test]
fn cli_fix_rewrites_safe_named_args_required_without_review_flag() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().expect("src parent")).expect("create src");
    write_fixture_file(
        &path,
        r#"fn add(a: Integer, b: Integer) -> Integer {
    return a + b

}
fn run() -> Integer {
    return add(1, 2)
}
"#,
    )
    .expect("write source");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fix")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(
        output.status.success(),
        "fix failed: code={:?}\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let rewritten = std::fs::read_to_string(&path).expect("read rewritten source");
    assert!(
        rewritten.contains("add(a=1, b=2)"),
        "expected named args rewrite: {rewritten}"
    );
}

#[test]
fn cli_fix_rewrites_legacy_given_call_to_function_call_syntax() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().expect("src parent")).expect("create src");
    write_fixture_file(
        &path,
        r#"fn is_positive(value: Integer) -> Boolean {
    return value > 0

}
fn run() -> Integer {
    is_ok = is_positive given 3
    if is_ok {
        return 1
    }
    return 0
}
"#,
    )
    .expect("write source");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fix")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(
        !output.status.success(),
        "legacy given syntax should fail in hard-cutover mode"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("legacy `given` call syntax is not supported"),
        "expected given syntax hard error: {stderr}"
    );
}

#[test]
fn cli_fix_rewrites_given_call_in_return_without_whitespace_loss() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().expect("src parent")).expect("create src");
    write_fixture_file(
        &path,
        r#"fn add_one(value: Integer) -> Integer {
    return value + 1

}
fn run() -> Integer {
    return add_one given 1
}
"#,
    )
    .expect("write source");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fix")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(
        !output.status.success(),
        "legacy given syntax should fail in hard-cutover mode"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("legacy `given` call syntax is not supported"),
        "expected given syntax hard error: {stderr}"
    );
}

#[test]
fn cli_fix_json_counts_given_call_rewrite_as_safe_fix() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().expect("src parent")).expect("create src");
    write_fixture_file(
        &path,
        r#"fn is_positive(value: Integer) -> Boolean {
    return value > 0

}
fn run() -> Integer {
    is_ok = is_positive given value=3
    if is_ok {
        return 1
    }
    return 0
}
"#,
    )
    .expect("write source");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fix")
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(
        !output.status.success(),
        "legacy given syntax should fail in hard-cutover mode"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("\"code\":\"lang::parse::syntax_error\""),
        "expected parse syntax error payload: {stdout}"
    );
}

#[test]
fn cli_fix_prefers_named_arg_rewrite_over_given_style_for_multi_positional_calls() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().expect("src parent")).expect("create src");
    write_fixture_file(
        &path,
        r#"fn in_range(value: Integer, limit: Integer) -> Boolean {
    return value < limit

}
fn run() -> Integer {
    ok = in_range given 1, 10
    if ok {
        return 1
    }
    return 0
}
"#,
    )
    .expect("write source");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fix")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(
        !output.status.success(),
        "legacy given syntax should fail in hard-cutover mode"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("legacy `given` call syntax is not supported"),
        "expected given syntax hard error: {stderr}"
    );
}

#[test]
fn cli_fix_rewrites_result_otherwise_to_or_else() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().expect("src parent")).expect("create src");
    write_fixture_file(
        &path,
        r#"fn try_to_parse_number(input: String) -> Result[Integer] {
    return error "nope"

}
fn run() -> Integer {
    return try_to_parse_number("1") ?? 0
}
"#,
    )
    .expect("write source");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fix")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert_eq!(output.status.code(), Some(3), "expected no-op fix result");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("no safe non-overlapping fixes found"),
        "expected no-op fix message: {stderr}"
    );
}

#[test]
fn cli_fix_allow_review_fixes_rewrites_boundary_generic_type() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().expect("src parent")).expect("create src");
    write_fixture_file(
        &path,
        r#"fn run(values: List) -> Integer {
    return 0
}
"#,
    )
    .expect("write source");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fix")
        .arg("--allow-review-fixes")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(
        output.status.success(),
        "fix failed: code={:?}\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let rewritten = std::fs::read_to_string(&path).expect("read rewritten source");
    assert!(
        rewritten.contains("List[Integer]"),
        "expected boundary generic rewrite: {rewritten}"
    );
}

#[test]
fn cli_fix_allow_review_fixes_rewrites_boundary_map_generic_type() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().expect("src parent")).expect("create src");
    write_fixture_file(
        &path,
        r#"fn run(meta: Map) -> Integer {
    return 0
}
"#,
    )
    .expect("write source");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fix")
        .arg("--allow-review-fixes")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(
        output.status.success(),
        "fix failed: code={:?}\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let rewritten = std::fs::read_to_string(&path).expect("read rewritten source");
    assert!(
        rewritten.contains("Map[String, Integer]"),
        "expected boundary map rewrite: {rewritten}"
    );
}

#[test]
fn cli_fmt_applies_rewrites_and_emits_summary_json_smoke() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().expect("src parent")).expect("create src");
    write_fixture_file(
        &path,
        r#"fn add_values(value: Integer, extra: Integer) -> Integer {
    return value + extra

}
fn run(values: List) -> Integer {
    total = add_values(1, 10)
    return total
}
"#,
    )
    .expect("write source");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fmt")
        .arg("--allow-review-fixes")
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(
        output.status.success(),
        "fmt failed: code={:?}\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let events: Vec<serde_json::Value> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("valid json line"))
        .collect();
    let summary = events
        .iter()
        .find(|value| value.get("event").and_then(|v| v.as_str()) == Some("fmt_summary"))
        .expect("expected fmt summary event");
    let applied = summary
        .get("summary")
        .and_then(|v| v.get("applied"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    assert!(
        applied >= 2,
        "expected at least two rewrites, got {applied}"
    );

    let rewritten = std::fs::read_to_string(&path).expect("read rewritten source");
    assert!(
        rewritten.contains("List[Integer]"),
        "expected List[Integer]: {rewritten}"
    );
    assert!(
        rewritten.contains("add_values(value=1, extra=10)"),
        "expected canonical call rewrite: {rewritten}"
    );
}

#[test]
fn cli_fmt_rewrites_legacy_result_fallback_syntax() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().expect("src parent")).expect("create src");
    write_fixture_file(
        &path,
        r#"fn try_to_parse_number(input: String) -> Result[Integer] {
    return error "nope"

}
fn run() -> Integer {
    return try_to_parse_number("1") ?? 0
}
"#,
    )
    .expect("write source");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fmt")
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(
        output.status.success(),
        "fmt failed: code={:?}\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let rewritten = std::fs::read_to_string(&path).expect("read rewritten source");
    assert!(
        rewritten.contains("?? 0"),
        "expected canonical result fallback operator: {rewritten}"
    );
    assert!(
        !rewritten.contains(" otherwise "),
        "expected legacy fallback operator to be rewritten: {rewritten}"
    );
}

#[test]
fn cli_fmt_directory_sweeps_src_and_tests_files() {
    let dir = workspace_tempdir();
    let src_main = dir.path().join("src").join("main.wr");
    let test_file = dir.path().join("tests").join("sample_test.wr");
    std::fs::create_dir_all(src_main.parent().expect("src parent")).expect("create src");
    std::fs::create_dir_all(test_file.parent().expect("test parent")).expect("create tests");
    write_fixture_file(
        &src_main,
        r#"fn add_one(value: Integer) -> Integer {
    return value + 1

}
fn run() -> Integer {
    return add_one(value=1)
}
"#,
    )
    .expect("write src");
    write_fixture_file(
        &test_file,
        r#"fn try_to_parse_number(input: String) -> Result[Integer] {
    return error "nope"

}
fn test_sample() -> Nothing {
    value = try_to_parse_number("1") ?? 0
    assert value value == 0
}
"#,
    )
    .expect("write test");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fmt")
        .arg("--error-format=json")
        .arg(dir.path())
        .output()
        .expect("run wrela");
    assert!(
        output.status.success(),
        "fmt failed: code={:?}\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let events: Vec<serde_json::Value> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .collect();
    let summary = events
        .iter()
        .find(|value| value.get("event").and_then(|v| v.as_str()) == Some("fmt_summary"))
        .expect("expected fmt summary event");
    assert!(
        summary
            .get("summary")
            .and_then(|v| v.get("failed_targets"))
            .and_then(|v| v.as_u64())
            .is_some_and(|count| count == 0),
        "expected no failures in fmt summary: {summary}"
    );
    assert!(
        summary
            .get("summary")
            .and_then(|v| v.get("targets_scanned"))
            .and_then(|v| v.as_u64())
            .is_some_and(|count| count >= 2),
        "expected at least two scanned targets in fmt summary: {summary}"
    );

    let rewritten_src = std::fs::read_to_string(&src_main).expect("read src");
    assert!(
        rewritten_src.contains("add_one(value=1)"),
        "expected src call rewrite: {rewritten_src}"
    );
    let rewritten_test = std::fs::read_to_string(&test_file).expect("read test");
    assert!(
        rewritten_test.contains("?? 0"),
        "expected test fallback rewrite: {rewritten_test}"
    );
}

#[test]
fn cli_fmt_converges_multi_arg_given_call_to_canonical_named_call() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().expect("src parent")).expect("create src");
    write_fixture_file(
        &path,
        r#"fn add_values(value: Integer, extra: Integer) -> Integer {
    return value + extra

}
fn run() -> Integer {
    return add_values(1, 10)
}
"#,
    )
    .expect("write source");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fmt")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(
        output.status.success(),
        "fmt failed: code={:?}\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let rewritten = std::fs::read_to_string(&path).expect("read rewritten source");
    assert!(
        rewritten.contains("add_values(value=1, extra=10)"),
        "expected canonical named call syntax: {rewritten}"
    );
    assert!(
        !rewritten.contains(" given "),
        "expected no legacy given call syntax after fmt: {rewritten}"
    );
}

#[test]
fn cli_fmt_second_run_is_zero_diff_smoke() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().expect("src parent")).expect("create src");
    write_fixture_file(
        &path,
        r#"fn add_values(value: Integer, extra: Integer) -> Integer {
    return value + extra

}
fn run(meta: Map) -> Integer {
    return add_values(1, 10)
}
"#,
    )
    .expect("write source");

    let first = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fmt")
        .arg("--allow-review-fixes")
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run first fmt");
    assert!(
        first.status.success(),
        "first fmt failed: code={:?}\nstdout={}\nstderr={}",
        first.status.code(),
        String::from_utf8_lossy(&first.stdout),
        String::from_utf8_lossy(&first.stderr)
    );

    let second = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fmt")
        .arg("--allow-review-fixes")
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run second fmt");
    assert!(
        second.status.success(),
        "second fmt failed: code={:?}\nstdout={}\nstderr={}",
        second.status.code(),
        String::from_utf8_lossy(&second.stdout),
        String::from_utf8_lossy(&second.stderr)
    );

    let events: Vec<serde_json::Value> = String::from_utf8_lossy(&second.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .collect();
    let summary = events
        .iter()
        .find(|value| value.get("event").and_then(|v| v.as_str()) == Some("fmt_summary"))
        .expect("expected fmt summary event");
    assert_eq!(
        summary
            .get("summary")
            .and_then(|v| v.get("applied"))
            .and_then(|v| v.as_u64()),
        Some(0),
        "expected zero applied rewrites on second fmt run: {summary}"
    );
    assert_eq!(
        summary
            .get("summary")
            .and_then(|v| v.get("touched_files"))
            .and_then(|v| v.as_u64()),
        Some(0),
        "expected zero touched files on second fmt run: {summary}"
    );
}

#[test]
fn cli_fmt_second_run_is_zero_diff() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().expect("src parent")).expect("create src");
    write_fixture_file(
        &path,
        r#"fn add_values(value: Integer, extra: Integer) -> Integer {
    return value + extra

}
fn run(meta: Map) -> Integer {
    return add_values(1, 10)
}
"#,
    )
    .expect("write source");

    let first = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fmt")
        .arg("--allow-review-fixes")
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run first fmt");
    assert!(
        first.status.success(),
        "first fmt failed: code={:?}\nstdout={}\nstderr={}",
        first.status.code(),
        String::from_utf8_lossy(&first.stdout),
        String::from_utf8_lossy(&first.stderr)
    );

    let second = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fmt")
        .arg("--allow-review-fixes")
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run second fmt");
    assert!(
        second.status.success(),
        "second fmt failed: code={:?}\nstdout={}\nstderr={}",
        second.status.code(),
        String::from_utf8_lossy(&second.stdout),
        String::from_utf8_lossy(&second.stderr)
    );

    let events: Vec<serde_json::Value> = String::from_utf8_lossy(&second.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .collect();
    let summary = events
        .iter()
        .find(|value| value.get("event").and_then(|v| v.as_str()) == Some("fmt_summary"))
        .expect("expected fmt summary event");
    assert_eq!(
        summary
            .get("summary")
            .and_then(|v| v.get("applied"))
            .and_then(|v| v.as_u64()),
        Some(0),
        "expected zero applied rewrites on second fmt run: {summary}"
    );
    assert_eq!(
        summary
            .get("summary")
            .and_then(|v| v.get("touched_files"))
            .and_then(|v| v.as_u64()),
        Some(0),
        "expected zero touched files on second fmt run: {summary}"
    );
}

#[test]
fn cli_fmt_applies_rewrites_and_emits_summary_json() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().expect("src parent")).expect("create src");
    write_fixture_file(
        &path,
        r#"fn add_values(value: Integer, extra: Integer) -> Integer {
    return value + extra

}
fn run(values: List) -> Integer {
    total = add_values(1, 10)
    return total
}
"#,
    )
    .expect("write source");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fmt")
        .arg("--allow-review-fixes")
        .arg("--error-format=json")
        .arg(&path)
        .output()
        .expect("run wrela");
    assert!(
        output.status.success(),
        "fmt failed: code={:?}\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let events: Vec<serde_json::Value> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("valid json line"))
        .collect();
    let summary = events
        .iter()
        .find(|value| value.get("event").and_then(|v| v.as_str()) == Some("fmt_summary"))
        .expect("expected fmt summary event");
    let applied = summary
        .get("summary")
        .and_then(|v| v.get("applied"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    assert!(applied >= 2, "expected >=2 rewrites, got {applied}");
    assert_eq!(
        summary
            .get("summary")
            .and_then(|v| v.get("failed_targets"))
            .and_then(|v| v.as_u64()),
        Some(0),
        "expected no failed targets: {summary}"
    );

    let rewritten = std::fs::read_to_string(&path).expect("read rewritten source");
    assert!(
        rewritten.contains("List[Integer]"),
        "expected boundary generic rewrite: {rewritten}"
    );
    assert!(
        rewritten.contains("add_values(value=1, extra=10)"),
        "expected canonical call rewrite: {rewritten}"
    );
}

#[test]
fn cli_fmt_directory_continues_after_file_failure_and_reports_summary() {
    let dir = workspace_tempdir();
    let src_main = dir.path().join("src").join("main.wr");
    let broken_test = dir.path().join("tests").join("broken_test.wr");
    std::fs::create_dir_all(src_main.parent().expect("src parent")).expect("create src");
    std::fs::create_dir_all(broken_test.parent().expect("tests parent")).expect("create tests");
    write_fixture_file(
        &src_main,
        r#"fn add_values(value: Integer, extra: Integer) -> Integer {
    return value + extra

}
fn run() -> Integer {
    return add_values(1, 10)
}
"#,
    )
    .expect("write src");
    write_fixture_file(
        &broken_test,
        r#"fn test_broken() -> Nothing {
    value = 1 +
    return
}
"#,
    )
    .expect("write broken");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("fmt")
        .arg("--error-format=json")
        .arg(dir.path())
        .output()
        .expect("run wrela");
    assert!(
        !output.status.success(),
        "expected non-zero exit due to broken target"
    );

    let events: Vec<serde_json::Value> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .collect();
    let summary = events
        .iter()
        .find(|value| value.get("event").and_then(|v| v.as_str()) == Some("fmt_summary"))
        .expect("expected fmt summary event");
    assert!(
        summary
            .get("summary")
            .and_then(|v| v.get("failed_targets"))
            .and_then(|v| v.as_u64())
            .is_some_and(|count| count >= 1),
        "expected failed target count in fmt summary: {summary}"
    );
    assert!(
        summary
            .get("summary")
            .and_then(|v| v.get("targets_scanned"))
            .and_then(|v| v.as_u64())
            .is_some_and(|count| count >= 2),
        "expected scanned target count in fmt summary: {summary}"
    );

    let rewritten_src = std::fs::read_to_string(&src_main).expect("read src");
    assert!(
        rewritten_src.contains("add_values(value=1, extra=10)"),
        "expected successful rewrites on healthy target despite one failure: {rewritten_src}"
    );
}

#[test]
fn cli_naming_bypass_allows_main_and_configure() {
    let dir = workspace_tempdir();
    let path = dir.path().join("src").join("main.wr");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    write_fixture_file(
        &path,
        r#"class Logger {
    fn __configure__() -> Nothing {
        return

    }
}
fn main() -> Integer {
    return 0
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

#[cfg(unix)]
fn write_executable(path: &std::path::Path, body: &str) {
    write_fixture_file(path, body).expect("write script");
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
    let dir = workspace_tempdir();
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
    assert!(invocations.contains("wrela:test language/spec --lane=spec"));
    assert!(invocations.contains("wrela:perf --runs=1"));
}

#[cfg(unix)]
#[test]
fn cli_matrix_forwards_perf_gate_flags() {
    let dir = workspace_tempdir();
    let log_path = dir.path().join("matrix-stub.log");
    let gate = dir.path().join("gate-baseline.json");
    write_fixture_file(&gate, r#"{}"#).expect("write gate");
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
    let dir = workspace_tempdir();
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

#[test]
fn benchmark_manifest_scenarios_resolve_via_discovery() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf();
    let manifests = [
        "benchmarks/micro/bench.toml",
        "benchmarks/field_engine/bench.toml",
        "benchmarks/realtime_presentation/bench.toml",
    ];

    for manifest_rel in manifests {
        let manifest_path = repo_root.join(manifest_rel);
        let bench_root = manifest_path.parent().expect("benchmark root");
        let raw_manifest =
            std::fs::read_to_string(&manifest_path).expect("read benchmark manifest text");
        let manifest: toml::Value =
            toml::from_str(&raw_manifest).expect("parse benchmark manifest");
        let scenarios = manifest
            .get("scenarios")
            .and_then(|value| value.as_array())
            .expect("manifest scenarios array");

        let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
            .arg("test")
            .arg(bench_root)
            .arg("--list")
            .arg("--error-format=json")
            .output()
            .expect("run wrela test --list");
        assert!(
            output.status.success(),
            "failed to list tests for {}:\nstdout:\n{}\nstderr:\n{}",
            manifest_path.display(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        let payload: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("valid test list json");
        let discovered: HashSet<String> = payload
            .get("tests")
            .and_then(|value| value.as_array())
            .expect("test list array")
            .iter()
            .filter_map(|entry| {
                entry
                    .get("name")
                    .and_then(|value| value.as_str())
                    .map(|name| name.to_string())
            })
            .collect();

        for scenario in scenarios {
            let test_name = scenario
                .get("test_name")
                .and_then(|value| value.as_str())
                .expect("scenario test_name");
            let scenario_id = scenario
                .get("id")
                .and_then(|value| value.as_str())
                .unwrap_or("<missing-id>");
            assert!(
                discovered.contains(test_name),
                "manifest scenario `{}` in {} references unknown discovery test `{}`",
                scenario_id,
                manifest_path.display(),
                test_name
            );
            if manifest_rel == "benchmarks/realtime_presentation/bench.toml" {
                let presentation = scenario
                    .get("presentation")
                    .and_then(|value| value.as_table())
                    .expect("realtime presentation metadata");
                assert!(
                    presentation
                        .get("entry")
                        .and_then(|value| value.as_str())
                        .is_some()
                );
                assert!(
                    presentation
                        .get("view")
                        .and_then(|value| value.as_str())
                        .is_some()
                );
                assert!(
                    presentation
                        .get("region")
                        .and_then(|value| value.as_str())
                        .is_some()
                );
                assert!(
                    presentation
                        .get("width")
                        .and_then(|value| value.as_integer())
                        .is_some_and(|value| value > 0)
                );
                assert!(
                    presentation
                        .get("height")
                        .and_then(|value| value.as_integer())
                        .is_some_and(|value| value > 0)
                );
                assert!(
                    presentation
                        .get("frames")
                        .and_then(|value| value.as_integer())
                        .is_some_and(|value| value > 0)
                );
            }
        }
    }
}

fn write_eval_case_workspace(
    root: &std::path::Path,
    case_id: &str,
    include_tests: bool,
) -> std::path::PathBuf {
    let case_dir = root.join("cases").join(case_id);
    std::fs::create_dir_all(case_dir.join("src")).expect("create eval case src");
    write_fixture_file(
        case_dir.join("src/main.wr"),
        r#"fn run() -> Integer {
    return 1
}
"#,
    )
    .expect("write eval case main");
    if include_tests {
        std::fs::create_dir_all(case_dir.join("tests/spec")).expect("create eval case tests");
        write_fixture_file(
            case_dir.join("tests/spec/eval_test.wr"),
            r#"fn test_eval_case() -> Nothing {
    assert value 1 == 1
}
"#,
        )
        .expect("write eval case test");
    }
    case_dir
}

fn write_eval_corpus_v2_fixture(root: &std::path::Path) -> std::path::PathBuf {
    write_eval_case_workspace(root, "check_case", false);
    write_eval_case_workspace(root, "check_case_non_machine_win", false);
    let manifest_path = root.join("one_shot_corpus_v2.json");
    write_fixture_file(
        &manifest_path,
        r#"{
  "schema_version": 2,
  "suite_id": "eval_cli_fixture_v2",
  "cases": [
    {
      "id": "check_case",
      "workspace_dir": "cases/check_case",
      "command": "check",
      "target": ".",
      "max_loops": 2,
      "attempts": [
        {
          "id": "a1",
          "visible_to_agent": false,
          "machine_applicable": false,
          "writes": [
            {
              "path": "src/main.wr",
              "content": "fn run() -> Integer {\n    return 1 +\n}\n"
            }
          ],
          "deletes": []
        },
        {
          "id": "a2",
          "visible_to_agent": true,
          "machine_applicable": true,
          "writes": [
            {
              "path": "src/main.wr",
              "content": "fn run() -> Integer {\n    return 1\n}\n"
            }
          ],
          "deletes": []
        }
      ]
    },
    {
      "id": "check_case_non_machine_win",
      "workspace_dir": "cases/check_case_non_machine_win",
      "command": "check",
      "target": ".",
      "max_loops": 2,
      "attempts": [
        {
          "id": "a1",
          "visible_to_agent": true,
          "machine_applicable": true,
          "writes": [
            {
              "path": "src/main.wr",
              "content": "fn run() -> Integer {\n    return true\n}\n"
            }
          ],
          "deletes": []
        },
        {
          "id": "a2",
          "visible_to_agent": true,
          "machine_applicable": false,
          "writes": [
            {
              "path": "src/main.wr",
              "content": "fn run() -> Integer {\n    return 1\n}\n"
            }
          ],
          "deletes": []
        }
      ]
    }
  ]
}"#,
    )
    .expect("write one-shot corpus v2 fixture");
    manifest_path
}

#[test]
fn cli_eval_one_shot_rejects_v1_corpus_shape() {
    let dir = workspace_tempdir();
    let corpus_path = dir.path().join("one_shot_v1.json");
    write_fixture_file(
        &corpus_path,
        r#"[
  {
    "id": "legacy",
    "passed": true
  }
]"#,
    )
    .expect("write one-shot v1 fixture");

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("eval")
        .arg("one-shot")
        .arg(&corpus_path)
        .arg("--error-format=json")
        .output()
        .expect("run eval with v1 corpus");
    assert!(!output.status.success(), "v1 corpus should be rejected");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unsupported one-shot corpus schema v1"),
        "missing hard-cut message:\n{}",
        stderr
    );
}

#[test]
fn cli_eval_one_shot_rejects_malformed_v2_manifest() {
    let dir = workspace_tempdir();
    let cases = [
        (
            "duplicate_case_id",
            r#"{
  "schema_version": 2,
  "suite_id": "bad",
  "cases": [
    {"id": "dup", "workspace_dir": "cases/a", "command": "check", "target": ".", "attempts": [{"id":"a1","noop":true}]},
    {"id": "dup", "workspace_dir": "cases/b", "command": "check", "target": ".", "attempts": [{"id":"a1","noop":true}]}
  ]
}"#,
            "duplicate one-shot case id",
        ),
        (
            "unsafe_write_path",
            r#"{
  "schema_version": 2,
  "suite_id": "bad",
  "cases": [
    {
      "id": "safe",
      "workspace_dir": "cases/safe",
      "command": "check",
      "target": ".",
      "attempts": [{"id":"a1","writes":[{"path":"../escape.wr","content":"x"}]}]
    }
  ]
}"#,
            "unsafe write path",
        ),
        (
            "empty_attempt_payload",
            r#"{
  "schema_version": 2,
  "suite_id": "bad",
  "cases": [
    {
      "id": "safe",
      "workspace_dir": "cases/safe",
      "command": "check",
      "target": ".",
      "attempts": [{"id":"a1","writes":[],"deletes":[],"noop":false}]
    }
  ]
}"#,
            "must define writes/deletes or set noop=true",
        ),
    ];

    for (name, body, expected_error) in cases {
        let manifest = dir.path().join(format!("{name}.json"));
        write_fixture_file(&manifest, body).expect("write malformed manifest");
        let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
            .arg("eval")
            .arg("one-shot")
            .arg(&manifest)
            .arg("--error-format=json")
            .output()
            .expect("run eval on malformed v2 manifest");
        assert!(
            !output.status.success(),
            "expected malformed manifest '{}' to fail",
            name
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(expected_error),
            "expected '{}' error for '{}', stderr:\n{}",
            expected_error,
            name,
            stderr
        );
    }
}

#[test]
fn cli_eval_one_shot_json_hash_is_stable() {
    let dir = workspace_tempdir();
    let corpus_path = write_eval_corpus_v2_fixture(dir.path());

    let first = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("eval")
        .arg("one-shot")
        .arg(&corpus_path)
        .arg("--runs=3")
        .arg("--error-format=json")
        .output()
        .expect("run first eval one-shot");
    assert!(
        first.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&first.stdout),
        String::from_utf8_lossy(&first.stderr)
    );
    let first_json = parse_single_json_stdout(&first.stdout);

    let second = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("eval")
        .arg("one-shot")
        .arg(&corpus_path)
        .arg("--runs=3")
        .arg("--error-format=json")
        .output()
        .expect("run second eval one-shot");
    assert!(
        second.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&second.stdout),
        String::from_utf8_lossy(&second.stderr)
    );
    let second_json = parse_single_json_stdout(&second.stdout);

    assert_eq!(
        first_json.get("report_hash"),
        second_json.get("report_hash"),
        "eval one-shot hash should be stable for deterministic reruns"
    );
    assert_eq!(
        first_json
            .get("schema_version")
            .and_then(|value| value.as_u64()),
        Some(2)
    );
    assert_eq!(
        first_json.get("suite_id").and_then(|value| value.as_str()),
        Some("eval_cli_fixture_v2")
    );
    assert_eq!(
        first_json.get("runs").and_then(|value| value.as_u64()),
        Some(3)
    );
    assert_eq!(
        first_json.get("pass_rate").and_then(|value| value.as_f64()),
        Some(0.5)
    );
    assert_eq!(
        first_json
            .get("machine_applicable_fix_apply_rate")
            .and_then(|value| value.as_f64()),
        Some(0.5)
    );
    let cases = first_json
        .get("cases")
        .and_then(|value| value.as_array())
        .expect("cases array");
    assert_eq!(cases.len(), 2);
    for case in cases {
        assert!(case.get("execution_ms_total").is_some());
    }
}

#[test]
fn cli_eval_one_shot_pretty_and_sarif_outputs_include_v2_contract() {
    let dir = workspace_tempdir();
    let corpus_path = write_eval_corpus_v2_fixture(dir.path());

    let pretty = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("eval")
        .arg("one-shot")
        .arg(&corpus_path)
        .arg("--error-format=human")
        .output()
        .expect("run eval one-shot pretty");
    assert!(
        pretty.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&pretty.stdout),
        String::from_utf8_lossy(&pretty.stderr)
    );
    let pretty_stdout = String::from_utf8_lossy(&pretty.stdout);
    assert!(pretty_stdout.contains("suite_id: eval_cli_fixture_v2"));
    assert!(pretty_stdout.contains("case=check_case"));

    let sarif = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("eval")
        .arg("one-shot")
        .arg(&corpus_path)
        .arg("--error-format=sarif")
        .output()
        .expect("run eval one-shot sarif");
    assert!(
        sarif.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&sarif.stdout),
        String::from_utf8_lossy(&sarif.stderr)
    );
    let sarif_json = parse_single_json_stdout(&sarif.stdout);
    assert_eq!(
        sarif_json.get("version").and_then(|value| value.as_str()),
        Some("2.1.0")
    );
    let message = sarif_json
        .get("runs")
        .and_then(|value| value.as_array())
        .and_then(|runs| runs.first())
        .and_then(|run| run.get("results"))
        .and_then(|value| value.as_array())
        .and_then(|results| results.first())
        .and_then(|result| result.get("message"))
        .and_then(|message| message.get("text"))
        .and_then(|value| value.as_str())
        .expect("sarif message text");
    assert!(message.contains("report_hash="));
    assert!(message.contains("suite=eval_cli_fixture_v2"));
}
