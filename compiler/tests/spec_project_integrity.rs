use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler crate should have repo parent")
        .to_path_buf()
}

fn assert_contains_all(source: &str, path: &str, needles: &[&str]) {
    for needle in needles {
        assert!(
            source.contains(needle),
            "expected {path} to mention `{needle}`"
        );
    }
}

#[test]
fn spec_project_layout_exists() {
    let root = repo_root();
    assert!(
        root.join("language/spec/src/main.wr").is_file(),
        "missing language/spec/src/main.wr"
    );
    assert!(
        root.join("language/spec/tests/spec/language_spec_test.wr")
            .is_file(),
        "missing language/spec/tests/spec/language_spec_test.wr"
    );
    assert!(
        !root.join("language/spec/spec.wr").exists(),
        "legacy language/spec/spec.wr should not exist"
    );
}

#[test]
fn spec_project_check_and_discovery_commands_are_valid() {
    let root = repo_root();
    let spec_root = root.join("language/spec");

    let check_output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg(&spec_root)
        .arg("--error-format=json")
        .output()
        .expect("run wrela check on spec project");
    assert!(
        check_output.status.success(),
        "spec project check failed: code={:?}\nstdout={}\nstderr={}",
        check_output.status.code(),
        String::from_utf8_lossy(&check_output.stdout),
        String::from_utf8_lossy(&check_output.stderr)
    );

    let list_output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("test")
        .arg(&spec_root)
        .arg("--lane=spec")
        .arg("--list")
        .arg("--jobs=1")
        .output()
        .expect("run wrela test --list on spec project");
    assert!(
        list_output.status.success(),
        "spec project test listing failed: code={:?}\nstdout={}\nstderr={}",
        list_output.status.code(),
        String::from_utf8_lossy(&list_output.stdout),
        String::from_utf8_lossy(&list_output.stderr)
    );

    let stdout = String::from_utf8_lossy(&list_output.stdout);
    assert!(
        stdout.contains("lane=spec"),
        "expected spec lane listings, got:\n{}",
        stdout
    );
}

#[test]
fn phase5_authored_surface_contains_new_names() {
    let root = repo_root();
    let spec_source =
        fs::read_to_string(root.join("language/spec/tests/spec/language_spec_test.wr"))
            .expect("read spec surface");
    for needle in [
        "rounded_box",
        "ellipsoid",
        "cone",
        "capped_cone",
        "box_frame",
        "slab",
        "triangle_prism",
        "hex_prism",
        "translate",
        "rotate",
        "uniform_scale",
        "affine_transform",
        "warp",
        "repeat_linear",
        "repeat_grid",
        "radial_repeat",
        "mirror_array",
        "instance_array",
        "smooth_union",
        "smooth_intersection",
        "smooth_subtract",
        "bend",
        "twist",
        "taper",
        "displace",
    ] {
        assert!(
            spec_source.contains(needle),
            "expected phase5 authored surface to mention `{needle}`"
        );
    }
}

#[test]
fn phase6_authored_surface_contains_construction_and_profile_names() {
    let root = repo_root();
    let spec_source =
        fs::read_to_string(root.join("language/spec/tests/spec/language_spec_test.wr"))
            .expect("read spec surface");
    for needle in [
        "extrude",
        "revolve",
        "sweep",
        "loft",
        "f32(",
        "circle2",
        "rect2",
        "rounded_rect2",
        "capsule2",
        "segment2",
        "polygon2",
        "polyline2",
    ] {
        assert!(
            spec_source.contains(needle),
            "expected phase6 authored surface to mention `{needle}`"
        );
    }

    for (path, needles) in [
        (
            "language/preview/src/main.wr",
            &["extrude = f32(", "loft = f32(", "polygon2", "revolve"][..],
        ),
        (
            "language/preview_boolean/src/main.wr",
            &["extrude = f32(", "sweep", "circle2", "rect2", "polygon2"][..],
        ),
        (
            "language/preview_repetition/src/main.wr",
            &[
                "sweep",
                "repeat_linear",
                "repeat_grid",
                "instance_array",
                "rect2",
                "mat4_cols",
            ][..],
        ),
        (
            "language/preview_thinstack/src/main.wr",
            &[
                "extrude = f32(",
                "loft = f32(",
                "revolve",
                "sweep",
                "capsule2",
            ][..],
        ),
    ] {
        let source = fs::read_to_string(root.join(path)).expect("read preview surface");
        for needle in needles {
            assert!(
                source.contains(needle),
                "expected {path} to mention `{needle}`"
            );
        }
    }

    for (path, needles) in [
        (
            "language/spec/tests/spec/language_spec_test.wr",
            &[
                "extrude = f32(",
                "loft = f32(",
                "polygon2",
                "circle2",
                "rounded_rect2",
                "capsule2",
                "segment2",
                "polyline2",
            ][..],
        ),
        (
            "compiler/tests/codegen_v2.rs",
            &[
                "extrude = f32(",
                "loft = f32(",
                "polygon2",
                "circle2",
                "rounded_rect2",
            ][..],
        ),
        (
            "compiler/tests/pir.rs",
            &[
                "extrude = f32(",
                "loft = f32(",
                "polygon2",
                "circle2",
                "rounded_rect2",
            ][..],
        ),
    ] {
        let source = fs::read_to_string(root.join(path)).expect("read phase6 surface");
        for needle in needles {
            assert!(
                source.contains(needle),
                "expected {path} to mention `{needle}`"
            );
        }
    }
}

#[test]
fn phase7_authored_surface_contains_radiance_and_volume_names() {
    let root = repo_root();
    let spec_source =
        fs::read_to_string(root.join("language/spec/tests/spec/language_spec_test.wr"))
            .expect("read spec surface");
    for needle in [
        "radiance field",
        "volume field",
        "radiance = phase7_radiance",
        "volume = phase7_volume",
        "radiance_at(",
        "medium_at(",
        "local_position",
        "local_normal",
        "feature_id",
    ] {
        assert!(
            spec_source.contains(needle),
            "expected phase7 authored surface to mention `{needle}`"
        );
    }

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
        (
            "compiler/tests/codegen_v2.rs",
            &[
                "radiance field",
                "volume field",
                "radiance = phase7_radiance",
                "volume = phase7_volume",
                "local_position",
                "local_normal",
            ][..],
            &["sample_radiance_at(", "sample_medium_at("][..],
        ),
        (
            "compiler/tests/pir.rs",
            &["radiance field", "volume field", "feature", "Medium("][..],
            &["sample_radiance_at(", "sample_medium_at("][..],
        ),
    ] {
        let main_source =
            fs::read_to_string(root.join(main_path)).expect("read phase8 authored surface");
        assert_contains_all(&main_source, main_path, main_needles);
        for needle in main_forbidden {
            assert!(
                !main_source.contains(needle),
                "expected {main_path} to avoid phase8 shortcut `{needle}`"
            );
        }
    }
}
