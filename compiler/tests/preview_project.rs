use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

const PREVIEW_TIMEOUT: Duration = Duration::from_secs(60);

fn preview_run_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn assert_contains_all(source: &str, path: &str, needles: &[&str]) {
    for needle in needles {
        assert!(
            source.contains(needle),
            "expected {path} to mention `{needle}`"
        );
    }
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler crate should have repo parent")
        .to_path_buf()
}

fn parse_ppm(data: &str) -> (usize, usize, Vec<[u8; 3]>) {
    let mut parts = data.split_whitespace();
    assert_eq!(parts.next(), Some("P3"), "missing PPM header: {data}");
    let width: usize = parts
        .next()
        .unwrap_or_else(|| panic!("missing width in PPM data: {data}"))
        .parse()
        .unwrap_or_else(|err| panic!("width parse failed in PPM data: {data}\nerror: {err:?}"));
    let height: usize = parts
        .next()
        .unwrap_or_else(|| panic!("missing height in PPM data: {data}"))
        .parse()
        .unwrap_or_else(|err| panic!("height parse failed in PPM data: {data}\nerror: {err:?}"));
    let max_value: usize = parts
        .next()
        .unwrap_or_else(|| panic!("missing max value in PPM data: {data}"))
        .parse()
        .unwrap_or_else(|err| panic!("max value parse failed in PPM data: {data}\nerror: {err:?}"));
    assert_eq!(max_value, 255, "unexpected PPM max value");

    let mut pixels = Vec::with_capacity(width * height);
    while let (Some(r), Some(g), Some(b)) = (parts.next(), parts.next(), parts.next()) {
        pixels.push([
            r.parse::<u8>()
                .unwrap_or_else(|err| panic!("red parse failed in PPM data: {data}\nerror: {err:?}")),
            g.parse::<u8>()
                .unwrap_or_else(|err| panic!("green parse failed in PPM data: {data}\nerror: {err:?}")),
            b.parse::<u8>()
                .unwrap_or_else(|err| panic!("blue parse failed in PPM data: {data}\nerror: {err:?}")),
        ]);
    }

    assert_eq!(pixels.len(), width * height, "unexpected pixel count");
    (width, height, pixels)
}

fn average_region(
    pixels: &[[u8; 3]],
    width: usize,
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
) -> [f32; 3] {
    let mut total = [0.0f32; 3];
    let mut count = 0.0f32;
    for y in y0..y1 {
        for x in x0..x1 {
            let px = pixels[y * width + x];
            total[0] += px[0] as f32;
            total[1] += px[1] as f32;
            total[2] += px[2] as f32;
            count += 1.0;
        }
    }
    [total[0] / count, total[1] / count, total[2] / count]
}

fn run_preview_project(project_root: &str) -> (usize, usize, Vec<[u8; 3]>) {
    let _guard = preview_run_lock()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let root = repo_root();
    let preview_root = root.join(project_root);
    let mut child = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("run")
        .arg(&preview_root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn preview project");
    let mut stdout_pipe = child.stdout.take().expect("preview stdout pipe");
    let mut stderr_pipe = child.stderr.take().expect("preview stderr pipe");
    let stdout_reader = thread::spawn(move || {
        let mut stdout = String::new();
        stdout_pipe
            .read_to_string(&mut stdout)
            .expect("read preview stdout");
        stdout
    });
    let stderr_reader = thread::spawn(move || {
        let mut stderr = String::new();
        stderr_pipe
            .read_to_string(&mut stderr)
            .expect("read preview stderr");
        stderr
    });

    let started_at = Instant::now();
    let status = loop {
        match child.try_wait().expect("poll preview project") {
            Some(status) => break status,
            None if started_at.elapsed() >= PREVIEW_TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait().expect("wait timed out preview project");
                let stdout = stdout_reader.join().expect("join preview stdout");
                let stderr = stderr_reader.join().expect("join preview stderr");
                panic!(
                    "preview project timed out after {:?}: project={project_root}\nstdout={}\nstderr={}",
                    PREVIEW_TIMEOUT, stdout, stderr
                );
            }
            None => thread::sleep(Duration::from_millis(25)),
        }
    };

    let stdout = stdout_reader.join().expect("join preview stdout");
    let stderr = stderr_reader.join().expect("join preview stderr");

    assert!(
        status.success(),
        "preview project failed: project={project_root}\ncode={:?}\nstdout={}\nstderr={}",
        status.code(),
        stdout,
        stderr
    );

    parse_ppm(&stdout)
}

fn assert_common_preview_signature(
    project_root: &str,
    expected_width: usize,
    expected_height: usize,
    expect_accent: bool,
) {
    let (width, height, pixels) = run_preview_project(project_root);
    assert_eq!(width, expected_width, "unexpected width for {project_root}");
    assert_eq!(
        height, expected_height,
        "unexpected height for {project_root}"
    );
    assert!(
        pixels
            .iter()
            .any(|px| px[0] > 55 || px[1] > 55 || px[2] > 65),
        "expected at least one clearly lit scene pixel in {project_root}"
    );
    assert!(
        pixels
            .iter()
            .any(|px| px[2] > px[0] + 20 && px[2] > px[1] + 8),
        "expected a cool sky or accent pixel in {project_root}"
    );
    assert!(
        pixels
            .iter()
            .any(|px| px[0] < 45 && px[1] < 40 && px[2] < 38),
        "expected a hard-shadow/contact-darkness pixel in {project_root}"
    );

    if expect_accent {
        assert!(
            pixels
                .iter()
                .any(|px| px[2] > px[0] + 12 && px[2] > px[1] - 8),
            "expected a blue accent material pixel in {project_root}"
        );
    }
}

fn assert_preview_is_stable_across_runs(project_root: &str) {
    let first = run_preview_project(project_root);
    let second = run_preview_project(project_root);
    assert_eq!(
        first.0, second.0,
        "unexpected width drift for {project_root}"
    );
    assert_eq!(
        first.1, second.1,
        "unexpected height drift for {project_root}"
    );
    assert_eq!(
        first.2, second.2,
        "preview output changed across runs for {project_root}"
    );
}

#[test]
fn preview_project_layout_exists() {
    let root = repo_root();
    assert!(
        root.join("language/preview/src/main.wr").is_file(),
        "missing language/preview/src/main.wr"
    );
    assert!(
        root.join("language/preview_repetition/src/main.wr")
            .is_file(),
        "missing language/preview_repetition/src/main.wr"
    );
    assert!(
        root.join("language/preview_boolean/src/main.wr").is_file(),
        "missing language/preview_boolean/src/main.wr"
    );
    assert!(
        root.join("language/preview_thinstack/src/main.wr")
            .is_file(),
        "missing language/preview_thinstack/src/main.wr"
    );
    assert!(
        !root.join("language/preview/src/render.wr").exists(),
        "legacy language/preview/src/render.wr should not exist"
    );
    assert!(
        !root.join("language/preview_repetition/src/render.wr").exists(),
        "legacy language/preview_repetition/src/render.wr should not exist"
    );
    assert!(
        !root.join("language/preview_boolean/src/render.wr").exists(),
        "legacy language/preview_boolean/src/render.wr should not exist"
    );
    assert!(
        !root.join("language/preview_thinstack/src/render.wr").exists(),
        "legacy language/preview_thinstack/src/render.wr should not exist"
    );
}

#[test]
fn preview_project_phase8_semantic_region_domain_render_exists() {
    let root = repo_root();
    for (main_path, main_needles, main_forbidden) in [
        (
            "language/preview/src/main.wr",
            &[
                "region scene_region()",
                "domain scene_domain(world: RegionCapture)",
                "geometry_detail = 1",
                "material = true",
                "radiance = true",
                "media = true",
                "max_distance = 12.0",
                "min_step = 0.02",
                "hit_epsilon = 0.0008",
                "max_steps = 96",
                "render render_ppm(",
                "radiance field",
                "volume field",
                "world = capture scene_region",
                "world_up = camera.up",
                "view_scale = 0.72",
                "fill_dir = normalize(vec3(-0.9, 0.45, 0.2))",
                "render_ppm(",
            ][..],
            &[
                "use render_ppm from render",
                "scene_capture = capture scene_shape",
                "trace_shape(",
                "surface_at(",
                "distance_at(",
                "radiance_at(",
                "medium_at(",
                "compute_scene_surface(",
                "compute_scene_hit(",
                "compute_shadow_visibility(",
                "compute_ambient_occlusion(",
                "compute_scene_color(",
                "trace_world(",
                "surface_world(",
                "distance_world(",
                "normal_world(",
                "radiance_world(",
                "medium_world(",
                "while y <",
                "while x <",
                "mutable ppm",
            ][..],
        ),
        (
            "language/preview_boolean/src/main.wr",
            &[
                "region scene_region()",
                "domain scene_domain(world: RegionCapture)",
                "geometry_detail = 1",
                "material = true",
                "radiance = true",
                "media = true",
                "max_distance = 6.0",
                "min_step = 0.1",
                "hit_epsilon = 0.0008",
                "max_steps = 96",
                "render render_ppm(",
                "radiance field",
                "volume field",
                "world = capture scene_region",
                "world_up = camera.up",
                "view_scale = 0.70",
                "fill_dir = normalize(vec3(-0.7, 0.4, 0.2))",
                "render_ppm(",
            ][..],
            &[
                "use render_ppm from render",
                "scene_capture = capture scene_shape",
                "trace_shape(",
                "surface_at(",
                "distance_at(",
                "radiance_at(",
                "medium_at(",
                "compute_scene_surface(",
                "compute_scene_hit(",
                "compute_shadow_visibility(",
                "compute_ambient_occlusion(",
                "compute_scene_color(",
                "trace_world(",
                "surface_world(",
                "distance_world(",
                "normal_world(",
                "radiance_world(",
                "medium_world(",
                "while y <",
                "while x <",
                "mutable ppm",
            ][..],
        ),
        (
            "language/preview_repetition/src/main.wr",
            &[
                "region scene_region()",
                "domain scene_domain(world: RegionCapture)",
                "geometry_detail = 1",
                "material = true",
                "radiance = true",
                "media = true",
                "max_distance = 12.0",
                "min_step = 0.06",
                "hit_epsilon = 0.0011",
                "max_steps = 64",
                "render render_ppm(",
                "radiance field",
                "volume field",
                "world = capture scene_region",
                "world_up = camera.up",
                "view_scale = 0.76",
                "fill_dir = normalize(vec3(-0.55, 0.42, 0.28))",
                "render_ppm(",
            ][..],
            &[
                "use render_ppm from render",
                "scene_capture = capture scene_shape",
                "trace_shape(",
                "surface_at(",
                "distance_at(",
                "radiance_at(",
                "medium_at(",
                "compute_scene_surface(",
                "compute_scene_hit(",
                "compute_shadow_visibility(",
                "compute_ambient_occlusion(",
                "compute_scene_color(",
                "trace_world(",
                "surface_world(",
                "distance_world(",
                "normal_world(",
                "radiance_world(",
                "medium_world(",
                "while y <",
                "while x <",
                "mutable ppm",
            ][..],
        ),
        (
            "language/preview_thinstack/src/main.wr",
            &[
                "region scene_region()",
                "domain scene_domain(world: RegionCapture)",
                "geometry_detail = 1",
                "material = true",
                "radiance = true",
                "media = true",
                "max_distance = 14.0",
                "min_step = 0.04",
                "hit_epsilon = 0.0008",
                "max_steps = 88",
                "render render_ppm(",
                "radiance field",
                "volume field",
                "world = capture scene_region",
                "world_up = camera.up",
                "view_scale = 0.74",
                "fill_dir = normalize(vec3(-0.5, 0.42, 0.25))",
                "render_ppm(",
            ][..],
            &[
                "use render_ppm from render",
                "scene_capture = capture scene_shape",
                "trace_shape(",
                "surface_at(",
                "distance_at(",
                "radiance_at(",
                "medium_at(",
                "compute_scene_surface(",
                "compute_scene_hit(",
                "compute_shadow_visibility(",
                "compute_ambient_occlusion(",
                "compute_scene_color(",
                "trace_world(",
                "surface_world(",
                "distance_world(",
                "normal_world(",
                "radiance_world(",
                "medium_world(",
                "while y <",
                "while x <",
                "mutable ppm",
            ][..],
        ),
    ] {
        let main_source =
            std::fs::read_to_string(root.join(main_path)).expect("read preview main surface");
        assert_contains_all(&main_source, main_path, main_needles);
        for needle in main_forbidden {
            assert!(
                !main_source.contains(needle),
                "expected {main_path} to avoid renderer-side query shortcut {needle:?}, got:\n{main_source}"
            );
        }
    }
}

#[test]
fn preview_project_renders_lit_cube_ppm() {
    let (width, height, pixels) = run_preview_project("language/preview");
    assert_eq!(width, 40, "unexpected width for language/preview");
    assert_eq!(height, 40, "unexpected height for language/preview");
    assert_eq!(
        pixels.len(),
        40 * 40,
        "unexpected pixel count for language/preview"
    );
    assert!(
        pixels
            .iter()
            .any(|px| px[0] > 55 || px[1] > 55 || px[2] > 65),
        "expected at least one clearly lit scene pixel in language/preview"
    );
    assert!(
        pixels
            .iter()
            .any(|px| px[2] > px[0] + 20 && px[2] > px[1] + 8),
        "expected a cool sky or accent pixel in language/preview"
    );
    assert!(
        pixels
            .iter()
            .any(|px| px[0] < 25 && px[1] < 35 && px[2] < 50),
        "expected a darker contact-shadow pixel in language/preview"
    );
    let object = average_region(&pixels, width, 12, 28, 24, 36);
    let corner = average_region(&pixels, width, 0, 0, 12, 12);
    assert!(
        (object[0] - object[2]) > (corner[0] - corner[2]) + 15.0,
        "expected a warmer authored object region against a cooler sky corner for language/preview: object={object:?} corner={corner:?}"
    );
    assert!(
        object[0] + object[1] + object[2] < corner[0] + corner[1] + corner[2],
        "expected the authored object region to stay darker than the sky corner for language/preview: object={object:?} corner={corner:?}"
    );
    assert!(
        corner[2] > corner[0] + 20.0,
        "expected a cool sky corner for language/preview: corner={corner:?}"
    );
}

#[test]
fn preview_project_renders_lit_cube_stably_across_runs() {
    assert_preview_is_stable_across_runs("language/preview");
}

#[test]
fn preview_project_renders_boolean_scene_ppm() {
    let (width, height, pixels) = run_preview_project("language/preview_boolean");
    assert_eq!(width, 8, "unexpected width for language/preview_boolean");
    assert_eq!(height, 8, "unexpected height for language/preview_boolean");
    assert!(
        pixels
            .iter()
            .any(|px| px[0] > 55 || px[1] > 55 || px[2] > 65),
        "expected at least one lit pixel in language/preview_boolean"
    );
    assert!(
        pixels
            .iter()
            .any(|px| px[0] < 30 && px[1] < 40 && px[2] < 60),
        "expected at least one dark pixel in language/preview_boolean"
    );
}

#[test]
fn preview_project_renders_boolean_scene_stably_across_runs() {
    let first = run_preview_project("language/preview_boolean");
    let second = run_preview_project("language/preview_boolean");
    assert_eq!(first.0, second.0, "unexpected width drift");
    assert_eq!(first.1, second.1, "unexpected height drift");
    assert_eq!(
        first.2, second.2,
        "preview_boolean output changed across runs"
    );
}

#[test]
fn preview_project_renders_repetition_scene_ppm() {
    assert_common_preview_signature("language/preview_repetition", 32, 32, true);
}

#[test]
fn preview_project_renders_repetition_scene_stably_across_runs() {
    assert_preview_is_stable_across_runs("language/preview_repetition");
}

#[test]
fn preview_project_renders_thinstack_scene_ppm() {
    assert_common_preview_signature("language/preview_thinstack", 40, 40, true);
}

#[test]
fn preview_project_renders_thinstack_scene_stably_across_runs() {
    assert_preview_is_stable_across_runs("language/preview_thinstack");
}

#[test]
fn preview_project_renders_phase6_scene_set_consistently() {
    for project_root in [
        "language/preview",
        "language/preview_boolean",
        "language/preview_repetition",
        "language/preview_thinstack",
    ] {
        let first = run_preview_project(project_root);
        let second = run_preview_project(project_root);
        assert_eq!(
            first.0, second.0,
            "unexpected width drift for {project_root}"
        );
        assert_eq!(
            first.1, second.1,
            "unexpected height drift for {project_root}"
        );
        assert_eq!(
            first.2, second.2,
            "preview output changed across runs for {project_root}"
        );
    }
}
