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

fn write_realtime_presentation_closure_benchmark_project(root: &std::path::Path) {
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
        tests_dir.join("realtime_fixture_test.wr"),
        r#"
field exact distance fixture_field(p: Vec3) -> F32 {
    sphere(radius = 0.55)
}

material fixture_surface(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.3, 0.5, 0.8),
        roughness=0.28,
        metalness=0.0,
        clearcoat=0.08,
        clearcoat_roughness=0.06,
        sheen=0.02,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape fixture_shape {
    field = fixture_field
    material = fixture_surface
}

region fixture_region() {
    place primary = fixture_shape
}

domain fixture_domain(world: RegionCapture) {
    geometry_detail = 1
    material = true
    radiance = false
    media = false
    max_distance = 8.0
    min_step = 0.04
    hit_epsilon = 0.001
    max_steps = 64
}

view show_fixture_1080p120_closure_view(world: RegionCapture, camera: Camera) {
    domain = fixture_domain(world = world)
    viewport = viewport(width = 64, height = 64)
    quality = realtime_quality(
        target_fps = 120,
        allow_dynamic_resolution = false,
        primary_max_steps = 64
    )
    lighting = key_light(
        light = Light(
            position=vec3(-0.8, 1.2, 1.8),
            direction=normalize(vec3(-0.2, -0.4, -1.0)),
            intensity=vec3(1.0, 1.0, 1.0),
            range=8.0
        )
    )
    outputs = frame_outputs(color = true, depth = true, normal = true, motion = true)
    history = temporal_history(color = true)
}

fn test_realtime_fixture_1080p120_closure_ops_64() -> Nothing {
    world = capture fixture_region
    domain = fixture_domain(world = world)
    mutable rays = []
    mutable points = []
    for i in 1...65 {
        px = f32(i % 8) * 0.16 - 0.56
        py = f32((i / 8) % 8) * 0.14 - 0.49
        origin = vec3(px, py, 2.4)
        __wr_list_push(
            rays,
            ray_query(
                origin=origin,
                direction=normalize(vec3(-px * 0.08, -py * 0.08, -1.0)),
                max_distance=8.0,
                min_step=0.04,
                hit_epsilon=0.001,
                max_steps=64
            )
        )
        __wr_list_push(
            points,
            PointQuery(point=origin + vec3(0.0, 0.0, -0.8))
        )
    }

    hits = spatial.nearest_batch(
        capture=world,
        domain=domain,
        rays=rays,
        backend=dispatch_backend_cpu()
    )
    normals = spatial.normal_batch(
        capture=world,
        domain=domain,
        points=points,
        backend=dispatch_backend_cpu()
    )

    mutable checksum = 0
    mutable hit_count = 0
    for sample_i in 0...16 {
        sample_index = sample_i * 4
        hit = hits[sample_index]
        normal = normals[sample_index].normal
        if hit.hit {
            hit_count += 1
        }
        checksum += i32(hit.steps) + i32(abs(normal.x + normal.y + normal.z) * 100.0)
    }

    require hit_count > 0 else "fixture closure hits present"
    require checksum != 0 else "fixture closure checksum nonzero"
}
"#,
    )
    .unwrap();
    write_fixture_file(
        root.join("1080p120_closure.toml"),
        r#"
version = 1
suite = "realtime_presentation"

[profiles.closure_1080p120]
warmup_pairs = 1
measure_pairs = 1
coverage = "all"
execution_story = "wgsl_resident"
adapter_name = "wgsl_resident"
enabled_optional_features = []
timestamps_enabled = false
f16_enabled = false
indirect_dispatch_enabled = false
warmup_protocol = "pipeline_and_resident_scene_upload"
companion_profile = "canonical_1080p120_cpu_oracle"

[[scenarios]]
id = "closure_1080p120_fixture"
test_name = "tests/realtime_fixture::test_realtime_fixture_1080p120_closure_ops_64"
ops = 64
class = "closure"
min_runtime_ms = 1
timeout_ms = 20000
allow_unstable = false
presentation = { entry = "tests/realtime_fixture_test.wr", view = "show_fixture_1080p120_closure_view", region = "fixture_region", domain = "fixture_domain", width = 64, height = 64, frames = 1, camera_position = [0.0, 0.0, 2.4], camera_forward = [0.0, 0.0, -1.0], camera_up = [0.0, 1.0, 0.0], vertical_fov_degrees = 45.0 }
"#,
    )
    .unwrap();
}

fn write_field_engine_closure_benchmark_project(root: &std::path::Path) {
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
        tests_dir.join("field_fixture_test.wr"),
        r#"
field exact distance field_fixture(p: Vec3) -> F32 {
    sphere(radius = 0.6)
}

material field_fixture_surface(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(120.0, 156.0, 208.0),
        roughness=0.24,
        metalness=0.08,
        clearcoat=0.10,
        clearcoat_roughness=0.10,
        sheen=0.06,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape field_fixture_shape {
    field = field_fixture
    material = field_fixture_surface
}

fn test_field_engine_fixture_1080p120_closure_ops_64() -> Nothing {
    world = capture field_fixture_shape
    mutable checksum = 0
    for i in 1...65 {
        px = f32(i % 8) * 0.14 - 0.49
        py = f32((i / 8) % 8) * 0.12 - 0.42
        hit = spatial.nearest(
            capture=world,
            ray=ray_query(
                origin=vec3(px, py, 2.4),
                direction=normalize(vec3(-px * 0.08, -py * 0.08, -1.0)),
                max_distance=8.0,
                min_step=0.04,
                hit_epsilon=0.001,
                max_steps=64
            )
        )
        checksum += hit.steps + i32(abs(hit.distance) * 100.0)
    }
    require checksum != 0 else "field-engine fixture checksum nonzero"
}
"#,
    )
    .unwrap();
    write_fixture_file(
        root.join("1080p120_closure.toml"),
        r#"
version = 1
suite = "field_engine"

[profiles.closure_1080p120]
warmup_pairs = 1
measure_pairs = 1
coverage = "all"
execution_story = "wgsl_resident"
adapter_name = "wgsl_resident"
enabled_optional_features = []
timestamps_enabled = false
f16_enabled = false
indirect_dispatch_enabled = false
warmup_protocol = "pipeline_and_resident_scene_upload"
companion_profile = "canonical_1080p120_cpu_oracle"

[[scenarios]]
id = "closure_1080p120_field_fixture"
test_name = "tests/field_fixture::test_field_engine_fixture_1080p120_closure_ops_64"
ops = 64
class = "closure"
min_runtime_ms = 1
timeout_ms = 20000
allow_unstable = false
"#,
    )
    .unwrap();
}

fn write_collision_closure_benchmark_project(root: &std::path::Path) {
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
        tests_dir.join("collision_perf_test.wr"),
        r#"
fn update_checksum(current: Integer, value: Integer) -> Integer {
    return ((current * 41) + value) % 2147483647
}

fn compute_transition_probe_offset(step: Integer) -> Vec3 {
    return vec3(
        f32(step % 9) * 0.04 - 0.16,
        f32((step / 9) % 5) * 0.03 - 0.06,
        f32(step % 4) * -0.02
    )
}

field exact distance collision_anchor(p: Vec3) -> F32 {
    sphere(radius = 0.42)
}

field conservative distance collision_column(p: Vec3) -> F32 {
    sweep = vec3(0.0, 1.76, 0.0) {
        circle2(radius = 0.10)
    }
}

field conservative distance collision_cap(p: Vec3) -> F32 {
    translate = vec3(0.0, 0.58, 0.0) {
        extrude = f32(0.14) {
            rect2(half = vec2(0.18, 0.06))
        }
    }
}

field conservative distance collision_cluster(p: Vec3) -> F32 {
    union {
        provenance_policy = nearest
        use collision_anchor
        translate = vec3(1.08, 0.02, 0.0) {
            use collision_column
        }
        translate = vec3(-1.38, -0.06, 0.0) {
            use collision_cap
        }
    }
}

field exact distance collision_left_guard(p: Vec3) -> F32 {
    translate = vec3(-2.55, 0.14, 0.0) {
        sphere(radius = 0.34)
    }
}

field exact distance collision_right_guard(p: Vec3) -> F32 {
    translate = vec3(2.45, -0.18, 0.0) {
        sphere(radius = 0.30)
    }
}

material collision_surface(hit: Hit3) -> Surface {
    shell = clamp(abs(hit.local_position.x) * 0.32 + abs(hit.local_normal.y) * 0.24, 0.0, 1.0)
    return Surface(
        albedo=vec3(164.0, 142.0, 102.0) + vec3(18.0, 12.0, 8.0) * shell,
        roughness=0.20 + shell * 0.14,
        metalness=0.08 + clamp(hit.local_normal.z, 0.0, 1.0) * 0.10,
        clearcoat=0.14 + clamp(hit.local_normal.y, 0.0, 1.0) * 0.12,
        clearcoat_roughness=0.08 + abs(hit.local_position.x) * 0.08,
        sheen=0.06 + abs(hit.local_normal.x) * 0.10,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape collision_perf_shape {
    field = collision_cluster
    material = collision_surface
}

shape collision_perf_left_shape {
    field = collision_left_guard
    material = collision_surface
}

shape collision_perf_right_shape {
    field = collision_right_guard
    material = collision_surface
}

region collision_perf_region() {
    place center = collision_perf_shape
    place left = collision_perf_left_shape
    place right = collision_perf_right_shape
}

domain collision_perf_domain(world: RegionCapture) {
    geometry_detail = 1
    material = true
    radiance = false
    media = false
    max_distance = 12.0
    min_step = 0.05
    hit_epsilon = 0.001
    max_steps = 96
}

fn load_collision_perf_scene() -> RegionCapture {
    return capture collision_perf_region
}

fn compute_collision_perf_point(world: RegionCapture, point: Vec3) -> F32 {
    domain = collision_perf_domain(world = world)
    return spatial.distance(capture = world, domain = domain, point = point)
}

fn compute_collision_perf_ray(world: RegionCapture, origin: Vec3, direction: Vec3) -> Hit3 {
    domain = collision_perf_domain(world = world)
    return spatial.nearest(
        capture=world,
        domain=domain,
        ray=ray_query(
            origin=origin,
            direction=direction,
            max_distance=12.0,
            min_step=0.05,
            hit_epsilon=0.001,
            max_steps=96
        )
    )
}

fn compute_collision_perf_transition(world: RegionCapture, step: Integer) -> Hit3 {
    offset = compute_transition_probe_offset(step)
    domain = collision_perf_domain(world = world)
    return spatial.nearest(
        capture=world,
        domain=domain,
        ray=ray_query(
            origin=vec3(0.0, 0.0, 3.0) + offset,
            direction=normalize(vec3(0.05, -0.03, -1.0)),
            max_distance=12.0,
            min_step=0.05,
            hit_epsilon=0.001,
            max_steps=96
        )
    )
}

fn test_collision_perf_point_occupancy_burst_ops_64() -> Nothing {
    world = load_collision_perf_scene()
    world_again = load_collision_perf_scene()
    assert value world.scene_id == world_again.scene_id

    anchor_distance = compute_collision_perf_point(world = world, point = vec3(0.0, 0.0, 0.0))
    cap_distance = compute_collision_perf_point(world = world, point = vec3(-1.35, -0.06, 0.0))

    mutable checksum = 0
    mutable occupied_count = 0
    for i in 1...64 {
        point = vec3(
            f32(i % 16) * 0.08 - 0.60,
            f32((i / 16) % 10) * 0.06 - 0.24,
            f32(i % 5) * 0.04 - 0.08
        )
        distance_sample = compute_collision_perf_point(world = world, point = point)
        if distance_sample <= 0.0 {
            occupied_count += 1
        }
        checksum = update_checksum(
            current=checksum,
            value=i32(abs(distance_sample) * 1000.0) + (i % 17)
        )
    }

    require checksum != 0 else "point occupancy checksum nonzero"
    require occupied_count > 0 else "point occupancy hit count present"
}

fn test_collision_perf_repeated_sweeps_ops_32() -> Nothing {
    world = load_collision_perf_scene()
    world_again = load_collision_perf_scene()
    assert value world.scene_id == world_again.scene_id

    anchor_hit = compute_collision_perf_transition(world = world, step = 1)
    require anchor_hit.hit == true else "repeated sweeps anchor hit"

    mutable checksum = 0
    mutable sweep_signal = 0
    for i in 1...32 {
        mutable probe_hit = compute_collision_perf_ray(
            world = world,
            origin = vec3(0.0, 0.0, 3.0) + compute_transition_probe_offset(i),
            direction = normalize(vec3(0.03, -0.04, -1.0))
        )
        if i % 4 == 0 {
            probe_hit = compute_collision_perf_transition(world = world, step = i)
        } else if i % 4 == 1 {
            probe_hit = compute_collision_perf_ray(
                world = world,
                origin = vec3(0.0, 0.0, 3.0) + compute_transition_probe_offset(i),
                direction = normalize(vec3(0.03, -0.04, -1.0))
            )
        } else if i % 4 == 2 {
            probe_hit = compute_collision_perf_ray(
                world = world,
                origin = vec3(1.08, 0.02, 3.0) + compute_transition_probe_offset(i),
                direction = normalize(vec3(-0.02, -0.03, -1.0))
            )
        } else {
            probe_hit = compute_collision_perf_ray(
                world = world,
                origin = vec3(-1.38, -0.06, 3.0) + compute_transition_probe_offset(i),
                direction = normalize(vec3(0.01, -0.04, -1.0))
            )
        }
        checksum = update_checksum(
            current=checksum,
            value=probe_hit.steps + i32(abs(probe_hit.distance) * 100.0)
        )
        if probe_hit.hit {
            sweep_signal += 1
        }
    }

    require checksum != 0 else "repeated sweeps checksum nonzero"
    require sweep_signal > 0 else "repeated sweeps hits present"
}
"#,
    )
    .unwrap();
    write_fixture_file(
        root.join("1080p120_closure.toml"),
        r#"
version = 1
suite = "collision_perf"

[profiles.closure_1080p120]
warmup_pairs = 1
measure_pairs = 1
coverage = "all"
execution_story = "wgsl_resident"
adapter_name = "wgsl_resident"
enabled_optional_features = []
timestamps_enabled = false
f16_enabled = false
indirect_dispatch_enabled = false
warmup_protocol = "pipeline_and_resident_scene_upload"
companion_profile = "canonical_1080p120_cpu_oracle"

[[scenarios]]
id = "closure_1080p120_point_occupancy_burst"
test_name = "tests/collision_perf::test_collision_perf_point_occupancy_burst_ops_64"
ops = 64
class = "closure"
min_runtime_ms = 1
timeout_ms = 120000
allow_unstable = false
collision = { entry = "tests/collision_perf_test.wr", region = "collision_perf_region", domain = "collision_perf_domain", workload = "point_occupancy_burst" }

"#,
    )
    .unwrap();
}

fn write_composite_frame_closure_benchmark_project(root: &std::path::Path, suite: &str) {
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
        tests_dir.join("whole_frame_test.wr"),
        r#"
fn update_checksum(current: Integer, value: Integer) -> Integer {
    return ((current * 41) + value) % 2147483647
}

field exact distance fixture_field(p: Vec3) -> F32 {
    sphere(radius = 0.55)
}

material fixture_surface(hit: Hit3) -> Surface {
    return Surface(
        albedo=vec3(0.3, 0.5, 0.8),
        roughness=0.28,
        metalness=0.0,
        clearcoat=0.08,
        clearcoat_roughness=0.06,
        sheen=0.02,
        emissive=vec3(0.0, 0.0, 0.0)
    )
}

shape fixture_shape {
    field = fixture_field
    material = fixture_surface
}

region fixture_region() {
    place primary = fixture_shape
}

domain fixture_domain(world: RegionCapture) {
    geometry_detail = 1
    material = true
    radiance = true
    media = true
    max_distance = 8.0
    min_step = 0.04
    hit_epsilon = 0.001
    max_steps = 96
}

view show_fixture_1080p120_closure_view(world: RegionCapture, camera: Camera) {
    domain = fixture_domain(world = world)
    viewport = viewport(width = 64, height = 64)
    quality = realtime_quality(
        target_fps = 120,
        allow_dynamic_resolution = false,
        primary_max_steps = 96
    )
    lighting = key_light(
        light = Light(
            position=vec3(-0.8, 1.2, 1.8),
            direction=normalize(vec3(-0.2, -0.4, -1.0)),
            intensity=vec3(1.0, 1.0, 1.0),
            range=8.0
        )
    )
    outputs = frame_outputs(color = true, depth = true, normal = true, motion = true)
    history = temporal_history(color = true)
}

fn load_whole_frame_scene() -> RegionCapture {
    return capture fixture_region
}

fn compute_whole_frame_point(world: RegionCapture, point: Vec3) -> F32 {
    domain = fixture_domain(world = world)
    return spatial.distance(capture = world, domain = domain, point = point)
}

fn compute_whole_frame_ray(world: RegionCapture, origin: Vec3, direction: Vec3) -> Hit3 {
    domain = fixture_domain(world = world)
    return spatial.nearest(
        capture=world,
        domain=domain,
        ray=ray_query(
            origin=origin,
            direction=direction,
            max_distance=8.0,
            min_step=0.04,
            hit_epsilon=0.001,
            max_steps=96
        )
    )
}

fn test_whole_frame_fixture_ops_64() -> Nothing {
    world = load_whole_frame_scene()
    world_again = load_whole_frame_scene()
    assert value world.scene_id == world_again.scene_id

    mutable checksum = 0
    mutable hit_count = 0
    for i in 1...65 {
        point = vec3(
            f32(i % 8) * 0.16 - 0.56,
            f32((i / 8) % 8) * 0.14 - 0.49,
            0.0
        )
        origin = point + vec3(0.0, 0.0, 2.4)
        hit = compute_whole_frame_ray(
            world = world,
            origin = origin,
            direction = normalize(vec3(-point.x * 0.08, -point.y * 0.08, -1.0))
        )
        distance_sample = compute_whole_frame_point(world = world, point = point)
        if hit.hit {
            hit_count += 1
        }
        checksum = update_checksum(
            current=checksum,
            value=hit.steps + i32(abs(hit.distance) * 100.0) + i32(abs(distance_sample) * 100.0)
        )
    }

    require checksum != 0 else "whole-frame fixture checksum nonzero"
    require hit_count > 0 else "whole-frame fixture hits present"
}
"#,
    )
    .unwrap();
    write_fixture_file(
        root.join("1080p120_closure.toml"),
        &r#"
version = 1
suite = "__SUITE__"

[profiles.closure_1080p120]
warmup_pairs = 1
measure_pairs = 1
coverage = "all"
execution_story = "wgsl_resident"
adapter_name = "wgsl_resident"
enabled_optional_features = []
timestamps_enabled = false
f16_enabled = false
indirect_dispatch_enabled = false
warmup_protocol = "pipeline_and_resident_scene_upload"
companion_profile = "canonical_1080p120_cpu_oracle"

[[scenarios]]
id = "closure_1080p120_fixture"
test_name = "tests/whole_frame::test_whole_frame_fixture_ops_64"
ops = 64
class = "closure"
min_runtime_ms = 1
timeout_ms = 20000
allow_unstable = false
presentation = { entry = "tests/whole_frame_test.wr", view = "show_fixture_1080p120_closure_view", region = "fixture_region", domain = "fixture_domain", width = 64, height = 64, frames = 1, camera_position = [0.0, 0.0, 2.4], camera_forward = [0.0, 0.0, -1.0], camera_up = [0.0, 1.0, 0.0], vertical_fov_degrees = 45.0 }
collision = { entry = "tests/whole_frame_test.wr", region = "fixture_region", domain = "fixture_domain", workload = "dense_ray_casts" }
"#
        .replace("__SUITE__", suite),
    )
    .unwrap();
}

fn write_whole_frame_closure_benchmark_project(root: &std::path::Path) {
    write_composite_frame_closure_benchmark_project(root, "whole_frame");
}

fn write_engine_frame_closure_benchmark_project(root: &std::path::Path) {
    write_composite_frame_closure_benchmark_project(root, "engine_frame");
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
