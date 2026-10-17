//! `wrela diff-behaviour`: its report and exit status. What it finds is tested in the driver
//! (`wrela_driver::behaviour`).

use crate::common;

const OLD: &str = "pub fn damp(x: f32) -> f32 {\n    x * 0.5\n}\n\npub fn same(x: f32) -> f32 {\n    x + 1.0\n}\n";
const NEW: &str = "pub fn damp(x: f32) -> f32 {\n    if x > 1e20 {\n        return x\n    }\n    x * 0.5\n}\n\npub fn same(x: f32) -> f32 {\n    let one = 1.0\n    x + one\n}\n";

#[test]
fn it_reports_the_samples_and_exits_by_whether_they_differ() {
    let old = common::package("diff-behaviour-old", &[("calc.wrela", OLD)]);
    let new = common::package("diff-behaviour-new", &[("calc.wrela", NEW)]);
    let run = |args: &[&str]| {
        common::wrela().arg("diff-behaviour").arg(&old).arg(&new).args(args).output().expect("run")
    };
    let same = run(&["same", "--samples", "16", "--seed", "3"]);
    assert_eq!(same.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&same.stdout),
        "`calc::same`: no differences on 24 samples (seed 3: 8 samples of boundary values, 16 seeded; floats compared bit for bit)\n"
    );
    let damp = run(&["damp", "--samples", "16"]);
    assert_eq!(damp.status.code(), Some(1));
    let text = String::from_utf8_lossy(&damp.stdout);
    assert!(text.starts_with("`calc::damp`: 2 of 24 samples differ"), "{text}");
    assert!(text.contains("x = 1e30\n  old: returned 5e29\n  new: returned 1e30\n"), "{text}");
    let json = common::json(
        common::wrela().arg("diff-behaviour").arg(&old).arg(&new).args(["damp", "--json"]),
    );
    assert_eq!(json["differing"], 2);
    assert_eq!(json["floats"], "bits");
    assert_eq!(json["differences"][0]["new"]["ending"], "returned");
    assert_eq!(json["differences"][0]["inputs"][0]["name"], "x");
    let within = run(&["same", "--within", "0.001", "--json"]);
    assert_eq!(within.status.code(), Some(0));
    let missing = run(&["nothing_here"]);
    assert_eq!(missing.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&missing.stderr).contains("nothing is named `nothing_here`"));
    let usage = common::wrela().arg("diff-behaviour").arg(&old).output().expect("run");
    assert_eq!(usage.status.code(), Some(2));
}
