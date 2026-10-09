//! `wrela profile`: a program with a labelled dispatch every frame, a labelled one in its first
//! frame alone, and phases its frames time; the report says each, and its WGSL.

use crate::common;

const WORK: &str = "use std::gpu::{GlobalId, GpuBuffer, Slots, buffer, dispatch, label}
use std::time::phase

@compute(64)
fn fill(out: mut Slots<f32>, id: GlobalId) {
    out[id] = f32(id.x)
}

pub struct Work {
    points: GpuBuffer<f32>,
    frames: u32,
}

pub fn init() -> Work {
    Work { points: buffer(4096), frames: 0 }
}

pub fn frame(w: mut Work, time: f32, width: u32, height: u32) {
    phase(\"record\")
    if w.frames == 0 {
        label(\"setup\")
        dispatch(fill.bind(out: mut w.points), groups: 64)
    }
    label(\"each frame\")
    dispatch(fill.bind(out: mut w.points), groups: 64)
    w.frames += 1
    phase(\"\")
}
";

#[test]
fn profile_reports_the_gpu_by_label_the_cpu_and_the_wgsl() {
    let dir = common::package(
        "profile-work",
        &[("wrela.toml", &common::manifest("work")), ("main.wrela", WORK)],
    );
    let d = dir.to_str().unwrap();
    let out = common::wrela()
        .args(["profile", d, "--frames", "4", "--json"])
        .output()
        .expect("run wrela");
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(out.status.success(), "{text}{}", String::from_utf8_lossy(&out.stderr));
    let p: serde_json::Value = serde_json::from_str(&text).expect("JSON");
    assert_eq!(p["frames"], 4);
    let label = |name: &str| {
        p["gpu"]["labels"]
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["label"] == name)
            .unwrap_or_else(|| panic!("no `{name}` in {text}"))
            .clone()
    };
    assert_eq!(label("each frame")["frames"], 4);
    assert_eq!(label("setup")["frames"], 1);
    assert!(label("each frame")["median_ms"].as_f64().unwrap() >= 0.0);
    let phases = p["cpu"]["phases"].as_array().unwrap();
    assert_eq!(phases.len(), 1, "{text}");
    assert_eq!(phases[0]["name"], "record");
    assert_eq!(phases[0]["times"], 4);
    let wgsl = p["wgsl"].as_array().unwrap();
    assert_eq!(wgsl.len(), 1, "{text}");
    assert_eq!(wgsl[0]["pipeline"], "fill");
    assert!(wgsl[0]["bytes"].as_u64().unwrap() > 0);
}
