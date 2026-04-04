use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

const PREVIEW_TIMEOUT: Duration = Duration::from_secs(20);

fn preview_run_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
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
        .expect("missing width")
        .parse()
        .expect("width parse");
    let height: usize = parts
        .next()
        .expect("missing height")
        .parse()
        .expect("height parse");
    let max_value: usize = parts
        .next()
        .expect("missing max value")
        .parse()
        .expect("max value parse");
    assert_eq!(max_value, 255, "unexpected PPM max value");

    let mut pixels = Vec::with_capacity(width * height);
    while let (Some(r), Some(g), Some(b)) = (parts.next(), parts.next(), parts.next()) {
        pixels.push([
            r.parse::<u8>().expect("red parse"),
            g.parse::<u8>().expect("green parse"),
            b.parse::<u8>().expect("blue parse"),
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
    assert_eq!(first.0, second.0, "unexpected width drift for {project_root}");
    assert_eq!(first.1, second.1, "unexpected height drift for {project_root}");
    assert_eq!(first.2, second.2, "preview output changed across runs for {project_root}");
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
}

#[test]
fn preview_project_renders_lit_cube_ppm() {
    let (width, _, pixels) = run_preview_project("language/preview");
    assert_eq!(width, 64, "unexpected width for language/preview");
    assert_eq!(
        pixels.len(),
        64 * 64,
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
    let center = average_region(&pixels, width, 22, 22, 42, 42);
    let corner = average_region(&pixels, width, 0, 0, 18, 18);
    assert!(
        center[0] > corner[0] + 20.0 && center[2] + 25.0 < corner[2],
        "expected a warm authored object region against a cooler sky corner for language/preview: center={center:?} corner={corner:?}"
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
    assert_eq!(first.2, second.2, "preview_boolean output changed across runs");
}

#[test]
fn preview_project_renders_repetition_scene_ppm() {
    assert_common_preview_signature("language/preview_repetition", 48, 48, true);
}

#[test]
fn preview_project_renders_repetition_scene_stably_across_runs() {
    assert_preview_is_stable_across_runs("language/preview_repetition");
}

#[test]
fn preview_project_renders_thinstack_scene_ppm() {
    assert_common_preview_signature("language/preview_thinstack", 48, 48, true);
}

#[test]
fn preview_project_renders_thinstack_scene_stably_across_runs() {
    assert_preview_is_stable_across_runs("language/preview_thinstack");
}
