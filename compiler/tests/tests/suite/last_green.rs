//! The Last Green's floor (M6: #54, #55): its map's anatomy, area and keepers, what the
//! interest check says of its bake, the preview's check beside the bake's, its build from the
//! constants' cache, and the floor's own tests (head room, ground cover).
//!
//! The map alone (map.wrela and what it reads) is tested in a scratch package of its own, so
//! those tests don't bake: they're the gate's. The bake's tests build the floor, which bakes
//! when the constants' cache is cold (minutes): they're long.

use crate::scratch;
use serde_json::Value;
use std::path::{Path, PathBuf};
use wrela_tests::repo_root;

/// The floor's files that make its map: copied into a package with `main` as its program.
const MAP_FILES: [&str; 4] = ["map.wrela", "ground.wrela", "species.wrela", "stones.wrela"];

/// A scratch package `name` holding the floor's map files and `main` as its `main.wrela`,
/// depending on the engine.
fn map_package(name: &str, main: &str) -> PathBuf {
    let dir = scratch(name);
    let floor = repo_root().join("examples/last-green");
    for f in MAP_FILES {
        std::fs::copy(floor.join(f), dir.join(f)).expect("copy the floor's map");
    }
    std::fs::write(dir.join("main.wrela"), main).expect("write main.wrela");
    let engine = repo_root().join("engine").display().to_string();
    let manifest = format!(
        "[package]\nname = \"floor_map\"\n\n[dependencies]\nengine = {{ path = {engine:?} }}\n"
    );
    std::fs::write(dir.join("wrela.toml"), manifest).expect("write wrela.toml");
    dir
}

/// AC1: the floor's map is wrela source written with the engine's types, and holds #44's
/// anatomy: a gate, a town's site, 3–5 anchors, a Warden's arena, rest points, waystones, 25–35
/// echoes' sites and views; its area is 3 km² within 10% (Q1); and the engine's own refusals
/// (`place::refusals`) find nothing to refuse in it.
#[test]
fn the_floors_map_has_its_anatomy_and_area() {
    let tests = r#"
use engine::interest::floor_area
use engine::place::{SiteKind, refusals}

@test
fn anatomy() {
    let m = map::map()
    assert(m.count(SiteKind::Gate) == 1, "one gate")
    assert(m.count(SiteKind::Town) == 1, "one town's site")
    let anchors = m.count(SiteKind::Anchor)
    assert(anchors >= 3 && anchors <= 5, f"3 to 5 anchors: {anchors}")
    assert(m.count(SiteKind::Arena) == 1, "one Warden's arena")
    assert(m.count(SiteKind::Rest) >= 1, "rest points")
    assert(m.count(SiteKind::Waystone) >= 1, "waystones")
    let echoes = m.count(SiteKind::Echo)
    assert(echoes >= 25 && echoes <= 35, f"25 to 35 echoes' sites: {echoes}")
    assert(m.count(SiteKind::View) >= 1, "views")
}

@test
fn area() {
    let a = floor_area(ground::ground(), map::map())
    assert(a > 2.7 && a < 3.3, f"about 3 km², within 10%: {a}")
}

@test
fn nothing_to_refuse() {
    assert(refusals(map::map()).len() == 0, "the floor's map has an opening or a route with no keeper at work")
}

pub fn frame(time: f32, width: u32, height: u32) {}
"#;
    let dir = map_package("last-green/anatomy", tests);
    assert_eq!(super::tests_pass(&dir), 3);
}

/// A package that grows the floor's map changed by `edit` (its `map()` bound to `m`, mutable),
/// at the preview's grain, in a constant.
fn grown_from(name: &str, edit: &str) -> PathBuf {
    let main = format!(
        r#"
use engine::grow::{{History, PREVIEW, simulate}}

const GROWN: u32 = grown()

fn grown() -> u32 {{
    var m = map::map()
    {edit}
    let h = simulate(ground::ground(), m, species::habits(), PREVIEW, map::LO, map::HI, stones::stones(m))
    h.trees.len()
}}

pub fn frame(time: f32, width: u32, height: u32) {{
    let _ = GROWN
}}
"#
    );
    map_package(name, &main)
}

/// The build of a map the history refuses fails with E0704, naming the opening, and before the
/// history runs: well inside the constant's fuel and in under a few seconds.
fn refused(dir: &Path, opening: &str, why: &str) {
    let t = std::time::Instant::now();
    let out = wrela_driver::build(dir);
    let took = t.elapsed().as_secs_f64();
    let text = wrela_diag::render::render_all(&out.sources, &out.diagnostics);
    let codes: Vec<&str> = out.diagnostics.iter().map(|d| d.code.as_str()).collect();
    assert_eq!(codes, ["E0704"], "{text}");
    assert!(text.contains("the map can't be grown"), "{text}");
    assert!(text.contains(opening) && text.contains(why), "the refusal names `{opening}`: {text}");
    // The history at the preview's grain takes seconds of the build's time; the refusal, a
    // fraction of one (most of this is compiling).
    assert!(took < 20.0, "refused after {took:.1} s: did the history run?");
}

/// AC1, keepers: a map with an opening that has no keeper is refused before the history runs,
/// the opening named.
#[test]
fn an_opening_with_no_keeper_is_refused_by_name() {
    let edit = "m.openings[0].keeper = engine::place::Keeper::Nothing";
    let dir = grown_from("last-green/no-keeper", edit);
    refused(&dir, "the clearing's meadow", "no keeper");
}

/// AC1, keepers: a map with an opening whose keeper stops before the present (people who left)
/// is refused before the history runs, the opening named.
#[test]
fn an_opening_whose_keeper_stops_before_the_present_is_refused_by_name() {
    let edit = "m.openings[0].keeper = engine::place::Keeper::People { from: 300, to: map::PRESENT - 100 }";
    let dir = grown_from("last-green/keeper-gone", edit);
    refused(&dir, "the clearing's meadow", "left");
}

// ---- the bake --------------------------------------------------------------------------------

/// The floor built (its bake from the constants' cache when it's warm), once a test process:
/// its build's directory.
fn floor() -> PathBuf {
    static BUILT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    BUILT
        .get_or_init(|| {
            let dir = repo_root().join("examples/last-green");
            let out = wrela_driver::build(&dir);
            assert!(
                !wrela_diag::has_errors(&out.diagnostics),
                "{}",
                wrela_diag::render::render_all(&out.sources, &out.diagnostics)
            );
            let build = dir.join("build");
            for (path, bytes) in &out.files {
                let to = build.join(path);
                std::fs::create_dir_all(to.parent().expect("a parent")).expect("mkdir");
                std::fs::write(to, bytes).expect("write the build's file");
            }
            build
        })
        .clone()
}

/// The check's report on the bake (`files/check/report.json`).
fn report() -> Value {
    let text = std::fs::read_to_string(floor().join("files/check/report.json")).expect("report");
    serde_json::from_str(&text).expect("the report is JSON")
}

fn f(v: &Value) -> f64 {
    v.as_f64().expect("a number")
}

fn named(v: &Value) -> &str {
    v["name"].as_str().expect("a name")
}

/// AC1's reach and AC3's verdicts, from the check on the bake: every anchor's site, the town's
/// site, the Warden's arena and every relation's viewpoint reached on foot from the gate (on the
/// check's grid, and on the walkable mask a metre a cell, as the sim walks); the
/// dead share ≤ 15% of the walkable floor; every relation holds; every opening is open in the
/// present; walkers lost ≤ 25%; the critical path's pilgrims reach every anchor. And measured,
/// not gated: the share with no draw of the map's own, draws an hour and the median gap, and
/// keystones an hour, for the floor and each region; the critical path's median time.
#[test]
#[ignore = "long: bakes the floor (minutes when the constants' cache is cold)"]
fn what_the_check_says_of_the_floor() {
    let r = report();
    let area = f(&r["area_km2"]);
    assert!((2.7..3.3).contains(&area), "the floor's area: {area} km²");
    for s in r["sites"].as_array().expect("sites") {
        if s["critical"].as_bool() == Some(true) {
            assert_eq!(
                s["reached"].as_bool(),
                Some(true),
                "{} can't be reached from the gate",
                named(s)
            );
        }
    }
    let floor = &r["floor"];
    let dead = f(&floor["dead"]);
    let lost = f(&floor["pilgrims"]["lost"]);
    let magpies_lost = f(&floor["magpies"]["lost"]);
    eprintln!(
        "the floor: {:.2} km² walkable; dead {:.1}% (spike 17: 14.4%); no draw of the map's own {:.1}% (75.1%); lost {:.1}% (pilgrims; magpies {:.1}%; spike 17: 23%); draws {:.0} an hour, median gap {:.0} s (38, 62 s); keystones {:.1} an hour (2.7)",
        f(&floor["walkable_km2"]),
        100.0 * dead,
        100.0 * f(&floor["quiet"]),
        100.0 * lost,
        100.0 * magpies_lost,
        f(&floor["pilgrims"]["draws_per_hour"]),
        f(&floor["pilgrims"]["median_gap"]),
        f(&floor["pilgrims"]["keystones_per_hour"]),
    );
    for region in r["regions"].as_array().expect("regions") {
        eprintln!(
            "  {}: {:.2} km², dead {:.1}%, no draw of the map's own {:.1}%, draws {:.0} an hour, median gap {:.0} s, keystones {:.1} an hour",
            named(region),
            f(&region["walkable_km2"]),
            100.0 * f(&region["dead"]),
            100.0 * f(&region["quiet"]),
            f(&region["pilgrims"]["draws_per_hour"]),
            f(&region["pilgrims"]["median_gap"]),
            f(&region["pilgrims"]["keystones_per_hour"]),
        );
    }
    assert!(dead <= 0.15, "dead {:.1}% of the walkable floor (≤ 15%)", 100.0 * dead);
    assert!(lost <= 0.25, "walkers lost {:.1}% (≤ 25%)", 100.0 * lost);
    for rel in r["relations"].as_array().expect("relations") {
        eprintln!("  {}: draw {:.2}, before {:.2}", named(rel), f(&rel["draw"]), f(&rel["before"]));
        assert_eq!(rel["holds"].as_bool(), Some(true), "{} doesn't hold", named(rel));
        assert_eq!(
            rel["reached"].as_bool(),
            Some(true),
            "{}'s viewpoint can't be reached",
            named(rel)
        );
    }
    for o in r["openings"].as_array().expect("openings") {
        assert_eq!(
            o["open"].as_bool(),
            Some(true),
            "{} isn't open: {:.2} covered",
            named(o),
            f(&o["cover"])
        );
    }
    // On foot as the sim walks: the walkable mask a metre a cell, its logs, walls and thickets
    // closed (`footing::Walkable`; the check's own grid is 4 m a cell).
    let text = std::fs::read_to_string(super::last_green::floor().join("files/check/reach.json"))
        .expect("reach.json");
    let on_foot: Value = serde_json::from_str(&text).expect("reach.json is JSON");
    for part in ["sites", "relations"] {
        for s in on_foot[part].as_array().expect("its list") {
            assert_eq!(
                s["reached"].as_bool(),
                Some(true),
                "{} can't be walked to from the gate (the walkable mask)",
                named(s)
            );
        }
    }
    let c = &r["critical"];
    eprintln!(
        "the critical path: {} of {} pilgrims reached every anchor; the median {:.0} min from the gate at a run (#44: 2–4 hours for the whole path)",
        c["reached"],
        c["of"],
        f(&c["median_s"]) / 60.0
    );
    assert_eq!(c["reached"], c["of"], "pilgrims on the trails reach every anchor: {c}");
}

/// AC2: a second build of the unchanged floor computes no constant.
#[test]
#[ignore = "long: bakes the floor (minutes when the constants' cache is cold)"]
fn the_floor_builds_again_from_its_cache() {
    let _ = floor();
    let t = std::time::Instant::now();
    let out = wrela_driver::build(&repo_root().join("examples/last-green"));
    assert!(!wrela_diag::has_errors(&out.diagnostics));
    eprintln!(
        "the floor built again in {:.2} s: {} computed, {} from the cache",
        t.elapsed().as_secs_f64(),
        out.consts.computed,
        out.consts.cached
    );
    assert_eq!(out.consts.computed, 0, "the second build computed constants");
    assert!(out.consts.cached > 0);
}

/// AC2: the bake is ≤ 0.7 MB a km² compressed (each file gzipped, over the floor's area), each
/// tile a file read alone (`bake::decode_tile` reads one with no other).
#[test]
#[ignore = "long: bakes the floor (minutes when the constants' cache is cold)"]
fn the_bake_is_small_and_a_file_a_tile() {
    let dir = floor().join("files/floor");
    let mut raw = 0usize;
    let mut packed = 0usize;
    let mut tiles = 0;
    for e in std::fs::read_dir(&dir).expect("the bake's files") {
        let path = e.expect("entry").path();
        raw += std::fs::metadata(&path).expect("size").len() as usize;
        let z = std::process::Command::new("gzip")
            .args(["-9", "-c"])
            .arg(&path)
            .output()
            .expect("gzip");
        packed += z.stdout.len();
        if path.file_name().is_some_and(|n| n.to_string_lossy().starts_with('t')) {
            tiles += 1;
        }
    }
    let area = f(&report()["area_km2"]);
    let per = packed as f64 / 1.0e6 / area;
    eprintln!(
        "the bake: {tiles} tiles and the far layer, {raw} bytes, {packed} gzipped: {per:.2} MB a km² of {area:.2} km² (#44's estimate for the floor: 16 MB)"
    );
    assert!(per <= 0.7, "{per:.2} MB a km² (≤ 0.7)");
    assert!(tiles >= 48, "a file a tile: {tiles}");
}

/// AC2: the preview's check is within 5 points of the bake's dead share, with the same verdict
/// on every relation. The preview's history grows in a package of its own (the map's files),
/// checked with the same settings.
#[test]
#[ignore = "long: bakes the floor and grows its preview (minutes when the constants' cache is cold)"]
fn the_previews_check_agrees_with_the_bakes() {
    let main = r#"
use engine::footing::obstacles_of
use engine::grow::{PREVIEW, simulate}
use engine::interest::{Evidence, FULL, check, report}
use std::io::{Shipped, ship}

@fuel(2 ** 44)
const PREVIEWED: Shipped = ship("preview", previewed())

fn previewed() -> Vec<(String, Vec<u8>)> {
    let m = map::map()
    let lo = map::LO - vec2(64.0)
    let hi = map::HI + vec2(64.0)
    let st = stones::stones(m)
    let h = simulate(ground::ground(), m, species::habits(), PREVIEW, lo, hi, st)
    let o = obstacles_of(h.trees, species::habits(), h.streams, st, stones::walls(m), lo, hi)
    let c = check(ground::ground(), m, species::habits(), Evidence::of(h), o, st, FULL)
    let text = report(ground::ground(), m, c)
    Vec::from([(String::from("report.json"), Vec::from(text.as_bytes()))])
}

pub fn frame(time: f32, width: u32, height: u32) {
    let _ = PREVIEWED.len()
}
"#;
    let dir = map_package("last-green/preview", main);
    let out = wrela_driver::build(&dir);
    assert!(
        !wrela_diag::has_errors(&out.diagnostics),
        "{}",
        wrela_diag::render::render_all(&out.sources, &out.diagnostics)
    );
    let (_, text) = out
        .files
        .iter()
        .find(|(p, _)| p == "files/preview/report.json")
        .expect("the preview's report");
    let preview: Value = serde_json::from_slice(text).expect("JSON");
    let bake = report();
    let (a, b) = (f(&preview["floor"]["dead"]), f(&bake["floor"]["dead"]));
    eprintln!("dead: the preview's {:.1}%, the bake's {:.1}%", 100.0 * a, 100.0 * b);
    assert!(
        (a - b).abs() <= 0.05,
        "the preview's dead share {:.1}%, the bake's {:.1}%",
        100.0 * a,
        100.0 * b
    );
    let rels = |r: &Value| -> Vec<(String, bool)> {
        r["relations"]
            .as_array()
            .expect("relations")
            .iter()
            .map(|x| (named(x).to_string(), x["holds"].as_bool().expect("holds")))
            .collect()
    };
    assert_eq!(rels(&preview), rels(&bake), "a relation's verdict differs");
}

/// The floor's own tests (`wrela test examples/last-green`): AC2's head room over the baked
/// trees (tests.wrela), AC11's ground cover against the history's light (no meadow grass under a
/// closed canopy; the fields hold every tile kept), and its first frames on the GPU.
#[test]
#[ignore = "long: bakes the floor (minutes when the constants' cache is cold)"]
fn the_floors_own_tests_pass() {
    let _ = floor();
    assert_eq!(super::tests_pass(&repo_root().join("examples/last-green")), 6);
}

/// The body of WGSL function `name` in `text` (to its closing brace at the line's start).
fn wgsl_body<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    let start = text.find(&format!("fn {name}("))?;
    let end = text[start..].find("\n}\n").map_or(text.len(), |e| start + e + 3);
    Some(&text[start..end])
}

/// AC9's bounds (spike 08's): the water's pass traces no ray through the scene. Its pipeline
/// binds no buffer of the scene's geometry: its storage is the water's mesh and the probes'
/// columns alone, so what it reflects comes from the screen (the scene under it, `below`), the
/// probes and the sky. And AC12's cooked materials: the fragments of bark (the trees', the
/// snags' and the logs' limbs), rock and masonry (the stones, the tower, the walls) evaluate no
/// field: none holds the value noise's lattice hash (`engine::noise`'s, which their fields'
/// noise is made of). Bark reads its ridges from a cooked tile, stone its cooked colours.
#[test]
#[ignore = "long: bakes the floor (minutes when the constants' cache is cold)"]
fn the_floors_pipelines_keep_their_bounds() {
    let build = floor();
    let (mut water, mut cooked) = (0, 0);
    for entry in std::fs::read_dir(&build).expect("the build") {
        let path = entry.expect("an entry").path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !(name.starts_with("pipeline_") && name.ends_with(".wgsl")) {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("the pipeline's WGSL");
        if text.contains("fn water_shade(") {
            water += 1;
            let lines: Vec<&str> = text.lines().collect();
            for (i, line) in lines.iter().enumerate() {
                if line.starts_with("var<storage") {
                    let bound = line.split_whitespace().nth(1).unwrap_or("").trim_end_matches(':');
                    assert!(
                        ["verts", "tris", "lt_grounds"].contains(&bound),
                        "{name}: the water's pass binds `{bound}` ({}), not its mesh or the probes'",
                        lines[i]
                    );
                }
            }
        }
        for f in ["bark_shade", "stone_shade"] {
            if let Some(body) = wgsl_body(&text, f) {
                cooked += 1;
                for hash in ["1597334677u", "3812015801u", "2798796415u"] {
                    assert!(!body.contains(hash), "{name}: `{f}` evaluates value noise ({hash})");
                }
            }
        }
    }
    assert_eq!(water, 1, "the water's pipelines");
    assert!(cooked >= 2, "only {cooked} of bark's and stone's pipelines");
}
