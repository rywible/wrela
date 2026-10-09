//! Spike 17's driver (throwaway): examples/region, loaded once on the native host; its map drawn,
//! with and without what the interest check found, and stills taken where the camera is held.
//! It writes images for a person to judge, and checks nothing, so each test is ignored unless
//! asked for: `cargo test --release -p wrela-tests --test suite region:: -- --ignored --nocapture`.
//! Images go to `target/tmp/region/`.

use std::path::PathBuf;
use std::time::Instant;
use wrela_host::{Host, Options, Value};
use wrela_tests::{page, repo_root};

const W: u32 = 1920;
const H: u32 = 1080;
/// The creature's bit in main.wrela's `System`: left out of the stills.
const OFF_CREATURE: i32 = 16;

fn floats(v: &[Value]) -> Vec<f64> {
    v.iter()
        .map(|x| match x {
            Value::F32(f) => f64::from(*f),
            Value::I32(i) => f64::from(*i),
            other => panic!("unexpected {other:?}"),
        })
        .collect()
}

pub(crate) struct Region {
    host: Host,
    frame: u32,
}

impl Region {
    pub(crate) fn load(name: &str) -> Region {
        let (dir, _) = page("examples/region", name);
        let t = Instant::now();
        let host = Host::load_with(&dir, &Options::default()).expect("load the region");
        eprintln!("region: loaded (init) in {:.2} s", t.elapsed().as_secs_f64());
        let mut r = Region { host, frame: 0 };
        r.call("test_off", &[Value::I32(OFF_CREATURE)]);
        r
    }

    pub(crate) fn step(&mut self) {
        self.host
            .lockstep_frame(self.frame, 60.0, W, H, &[])
            .unwrap_or_else(|e| panic!("frame {}: {e}", self.frame));
        self.frame += 1;
    }

    pub(crate) fn steps(&mut self, n: u32) {
        for _ in 0..n {
            self.step();
        }
    }

    pub(crate) fn call(&mut self, name: &str, args: &[Value]) -> Vec<f64> {
        floats(&self.host.call_export(name, args).unwrap_or_else(|e| panic!("{name}: {e}")))
    }

    /// Frames until what's cooked round the camera is done, then `extra` more (temporal AA's
    /// history settles).
    pub(crate) fn settle(&mut self, extra: u32) {
        let mut n = 0;
        while self.call("test_settled", &[])[0] < 0.5 {
            self.step();
            n += 1;
            assert!(n < 900, "not settled after {n} frames");
        }
        self.steps(extra);
    }

    /// The ground's height at (x, z), and how walkable it is.
    pub(crate) fn stand(&mut self, x: f32, z: f32) -> (f32, f32) {
        let v = self.call("test_stand", &[Value::F32(x), Value::F32(z)]);
        (v[0] as f32, v[1] as f32)
    }

    /// The camera held at (x, z), `up` metres above the ground there, looking at `at`.
    pub(crate) fn hold(&mut self, x: f32, z: f32, up: f32, at: [f32; 3]) {
        let (g, _) = self.stand(x, z);
        let args: Vec<Value> =
            [x, g + up, z, at[0], at[1], at[2]].iter().map(|&v| Value::F32(v)).collect();
        self.call("test_hold", &args);
    }

    /// The camera at (x, z) at eye height, looking along `yaw` (radians from +x toward +z),
    /// level, or toward a point `rise` metres above or below the eye 100 m out.
    pub(crate) fn look(&mut self, x: f32, z: f32, yaw: f32, rise: f32) {
        let (g, _) = self.stand(x, z);
        let at = [x + 100.0 * yaw.cos(), g + 1.6 + rise, z + 100.0 * yaw.sin()];
        self.hold(x, z, 1.6, at);
    }

    pub(crate) fn save(&mut self, file: &str) -> PathBuf {
        let rgba = self.host.read_screen().expect("read the screen");
        let dir = repo_root().join("target/tmp/region");
        std::fs::create_dir_all(&dir).expect("make the directory");
        let path = dir.join(file);
        wrela_host::image::write_png(&path, W, H, &rgba).expect("write the image");
        path
    }
}

/// A point on the plane, and what's there.
struct Shot {
    name: &'static str,
    eye: [f32; 2],
    /// Where it looks: a point on the plane and a height above the ground there.
    at: [f32; 2],
    above: f32,
}

const TOUR: &[Shot] = &[
    Shot { name: "opening", eye: [-1.5, 0.0], at: [0.0, 40.0], above: 6.0 },
    Shot { name: "knoll-to-tower", eye: [4.0, 26.0], at: [560.0, -470.0], above: 18.0 },
    Shot { name: "tor-to-tower", eye: [333.0, -153.0], at: [560.0, -470.0], above: 18.0 },
    Shot { name: "tor-to-sink", eye: [328.0, -155.0], at: [255.0, -385.0], above: 2.0 },
    Shot { name: "tor-from-spine", eye: [255.0, -160.0], at: [330.0, -150.0], above: 14.0 },
    Shot { name: "crest-to-sink", eye: [300.0, -200.0], at: [255.0, -385.0], above: 2.0 },
    Shot { name: "beechwood", eye: [420.0, -580.0], at: [380.0, -520.0], above: 2.0 },
    Shot { name: "sink-shore", eye: [215.0, -370.0], at: [290.0, -390.0], above: 2.0 },
    Shot { name: "burn", eye: [600.0, -150.0], at: [680.0, -210.0], above: 4.0 },
    Shot { name: "ring", eye: [25.0, -440.0], at: [40.0, -455.0], above: 1.0 },
    Shot { name: "tower-foot", eye: [520.0, -430.0], at: [560.0, -470.0], above: 12.0 },
    Shot { name: "giant", eye: [60.0, -300.0], at: [40.0, -322.0], above: 0.0 },
    Shot { name: "lip", eye: [-190.0, -175.0], at: [-600.0, -100.0], above: -40.0 },
    Shot { name: "windthrow", eye: [380.0, 130.0], at: [420.0, 160.0], above: 1.0 },
    Shot { name: "road", eye: [150.0, -108.0], at: [268.0, -180.0], above: 2.0 },
];

/// The map, the check's map, and a still at each place of the tour.
#[test]
#[ignore = "spike 17: writes images to judge"]
fn region_tour() {
    let mut r = Region::load("region-tour");
    r.settle(10);
    let n = r.call("test_numbers", &[]);
    eprintln!(
        "region: {:.2} km² walkable; dead {:.1}%, drawn twice {:.1}%; {:.1} arrivals an hour, median gap {:.0} s, lost {:.0}%, {:.1} keystones an hour",
        n[0],
        n[1] * 100.0,
        n[2] * 100.0,
        n[3],
        n[4],
        n[5] * 100.0,
        n[6]
    );
    let t = Instant::now();
    r.call("test_chart", &[Value::I32(0)]);
    eprintln!("region: the map drawn in {:.2} s", t.elapsed().as_secs_f64());
    r.steps(2);
    r.save("map.png");
    r.call("test_chart", &[Value::I32(1)]);
    r.steps(2);
    r.save("map-check.png");
    r.call("test_view", &[Value::I32(0)]);
    for s in TOUR {
        let (g, _) = r.stand(s.at[0], s.at[1]);
        r.hold(s.eye[0], s.eye[1], 1.6, [s.at[0], g + s.above, s.at[1]]);
        r.settle(24);
        let p = r.save(&format!("{}.png", s.name));
        let yaw = (s.at[1] - s.eye[1]).atan2(s.at[0] - s.eye[0]);
        let d = r.call(
            "test_draw",
            &[Value::F32(s.eye[0]), Value::F32(s.eye[1]), Value::F32(yaw), Value::F32(0.4)],
        );
        eprintln!(
            "region: {} (draw ahead {:.2} kind {} to ({:.0}, {:.0}); best {:.2}, {} draws) -> {}",
            s.name,
            d[0],
            d[1],
            d[2],
            d[3],
            d[4],
            d[5],
            p.display()
        );
    }
}

/// A small, seeded generator for the spike's samples.
pub(crate) struct Rng(u64);

impl Rng {
    pub(crate) fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.0 >> 40) as f32) / ((1u64 << 24) as f32)
    }
}

/// The interest model's occlusion against the renderer's: at seeded places in the forest, a level
/// camera's depth, in a band just above the horizon (above the grass), gives the share of
/// sightlines that reach past 15, 30, 60 and 120 m; the model's `test_seen` predicts the same
/// shares. Prints each pair, and the scale on the model's extinction that fits them best.
#[test]
#[ignore = "spike 17: calibrates the interest model"]
fn region_calibrate() {
    let mut r = Region::load("region-calibrate");
    r.settle(4);
    let mut rng = Rng(17);
    let dists = [15.0f64, 30.0, 60.0, 120.0];
    let (mut num, mut den) = (0.0f64, 0.0f64);
    let mut pairs = Vec::new();
    let mut tried = 0;
    while pairs.len() < 24 && tried < 400 {
        tried += 1;
        let x = -150.0 + 900.0 * rng.next();
        let z = -650.0 + 900.0 * rng.next();
        let (_, walk) = r.stand(x, z);
        if walk < 0.5 || r.call("test_brush", &[Value::F32(x), Value::F32(z)])[0] > 0.08 {
            continue;
        }
        let yaw = std::f32::consts::TAU * rng.next();
        r.look(x, z, yaw, 0.0);
        r.settle(8);
        // The view's depth: reversed, at the scene's size; the camera that drew it.
        let n = r.call("test_capture", &[Value::I32(1)]);
        assert!(n[0] > 0.0);
        let newest = *r.host.buffers().last().expect("a buffer");
        let bytes = r.host.read_buffer(newest).expect("read the depth");
        let depth: Vec<f32> =
            bytes.chunks(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
        let cam = r.call("test_camera", &[]);
        let (near, tan_half, aspect) = (cam[14], cam[12], cam[13]);
        let (sw, sh) = (cam[17] as usize, cam[18] as usize);
        let mut beyond = [0usize; 4];
        let mut count = 0usize;
        // Rows from 1° to 4° above the horizon: over the grass, under most crowns.
        for row in 0..sh {
            let ndc_y = 1.0 - (row as f64 + 0.5) / sh as f64 * 2.0;
            let up = (ndc_y * tan_half).atan().to_degrees();
            if !(1.0..=4.0).contains(&up) {
                continue;
            }
            for col in 0..sw {
                let ndc_x = (col as f64 + 0.5) / sw as f64 * 2.0 - 1.0;
                let z = f64::from(depth[row * sw + col]);
                // Along the ray: the view depth over the cosine to the forward axis.
                let rx = ndc_x * tan_half * aspect;
                let ry = ndc_y * tan_half;
                let dist = if z <= 1e-9 { f64::INFINITY } else { near / z * (1.0 + rx * rx + ry * ry).sqrt() };
                count += 1;
                for (k, d) in dists.iter().enumerate() {
                    if dist > *d {
                        beyond[k] += 1;
                    }
                }
            }
        }
        let half = (tan_half * aspect).atan() as f32;
        let seen = r.call(
            "test_seen",
            &[Value::F32(x), Value::F32(z), Value::F32(yaw), Value::F32(half * 0.9)],
        );
        let shares: Vec<f64> = beyond.iter().map(|&b| b as f64 / count.max(1) as f64).collect();
        eprintln!(
            "calibrate ({x:.0}, {z:.0}) yaw {yaw:.2}: rendered {:.2} {:.2} {:.2} {:.2}; model {:.2} {:.2} {:.2} {:.2}",
            shares[0], shares[1], shares[2], shares[3], seen[0], seen[1], seen[2], seen[3]
        );
        for k in 0..4 {
            let (a, m) = (shares[k].clamp(0.01, 1.0), seen[k].clamp(0.01, 1.0));
            // rendered = exp(-s τ): s = ln a / ln m, weighted by τ.
            num += -a.ln() * -m.ln();
            den += m.ln() * m.ln();
        }
        pairs.push((shares, seen));
    }
    eprintln!("calibrate: {} places; the model's extinction fits the renderer's times {:.2}", pairs.len(), num / den.max(1e-9));
}

/// Stills for a blind test of the interest check: seeded places and directions, ten where the
/// check says nothing draws ahead (under 0.12), ten faint (0.12 to 0.3), ten strong (over 0.3),
/// shuffled and named by number. The check's values go to a key file beside them, which the
/// rater doesn't see. `REGION_BLIND` names the directory (default `blind`).
#[test]
#[ignore = "spike 17: writes images to judge"]
fn region_blind() {
    let dir_name = std::env::var("REGION_BLIND").unwrap_or_else(|_| "blind".to_string());
    let mut r = Region::load("region-blind");
    r.settle(4);
    let mut rng = Rng(1700);
    let half = 0.55f32;
    let mut buckets: [Vec<(f32, f32, f32, Vec<f64>)>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    let mut tried = 0;
    while buckets.iter().any(|b| b.len() < 10) && tried < 4000 {
        tried += 1;
        let x = -150.0 + 900.0 * rng.next();
        let z = -650.0 + 900.0 * rng.next();
        let (_, walk) = r.stand(x, z);
        // Where a player stands: walkable, and not in a thicket or a sapling's crown.
        if walk < 0.5 || r.call("test_brush", &[Value::F32(x), Value::F32(z)])[0] > 0.08 {
            continue;
        }
        let yaw = std::f32::consts::TAU * rng.next();
        let d = r.call("test_draw", &[Value::F32(x), Value::F32(z), Value::F32(yaw), Value::F32(half)]);
        let b = if d[0] < 0.12 { 0 } else if d[0] < 0.3 { 1 } else { 2 };
        if buckets[b].len() < 10 {
            buckets[b].push((x, z, yaw, d));
        }
    }
    let mut all: Vec<(f32, f32, f32, Vec<f64>)> = buckets.into_iter().flatten().collect();
    // Shuffled, so the names say nothing.
    for i in (1..all.len()).rev() {
        let j = (rng.next() * (i + 1) as f32) as usize % (i + 1);
        all.swap(i, j);
    }
    let dir = repo_root().join("target/tmp/region").join(&dir_name);
    std::fs::create_dir_all(&dir).expect("make the directory");
    let mut key = String::from("id,x,z,yaw,ahead,kind,target_x,target_z,best,count\n");
    for (i, (x, z, yaw, d)) in all.iter().enumerate() {
        r.look(*x, *z, *yaw, 0.0);
        r.settle(24);
        r.save(&format!("{dir_name}/{:02}.png", i + 1));
        key.push_str(&format!(
            "{:02},{x:.1},{z:.1},{yaw:.3},{:.3},{},{:.1},{:.1},{:.3},{}\n",
            i + 1,
            d[0],
            d[1],
            d[2],
            d[3],
            d[4],
            d[5]
        ));
    }
    std::fs::write(repo_root().join("target/tmp/region").join(format!("{dir_name}-key.csv")), key)
        .expect("write the key");
    eprintln!("blind: {} stills ({tried} places tried)", all.len());
}

/// Where each relation's sightline loses its light.
#[test]
#[ignore = "spike 17: prints numbers"]
fn region_lines() {
    let mut r = Region::load("region-lines");
    for (name, x, z, key) in [
        ("knoll to tower", 4.0f32, 26.0f32, 1i32),
        ("tor to tower", 330.0, -150.0, 1),
        ("tor to sink", 330.0, -150.0, 7),
        ("crest to sink", 300.0, -200.0, 7),
        ("clearing to tor", 14.0, 18.0, 2),
    ] {
        let v = r.call("test_line", &[Value::F32(x), Value::F32(z), Value::I32(key)]);
        let t: Vec<String> = v[..10].iter().map(|x| format!("{x:.2}")).collect();
        eprintln!("line {name}: light at each tenth {}; least clearance over the crowns {:.1} m", t.join(" "), v[10]);
    }
}

/// The endless forest's numbers: precision at distances, the wildwood's seams and cost, and the
/// endless window's check.
#[test]
#[ignore = "spike 17: prints numbers"]
fn region_endless() {
    let mut r = Region::load("region-endless");
    for km in [0.0f32, 1.0, 10.0, 50.0, 200.0, 1000.0, 5000.0] {
        let v = r.call("test_precision", &[Value::F32(km)]);
        eprintln!(
            "precision at {km} km: f32 walk off by {:.4} m, steps {:.5} to {:.5} m; tiled off by {:.6} m, steps {:.5} to {:.5} m; grass noise {} values in 10 cm",
            v[0], v[1], v[2], v[3], v[4], v[5], v[6]
        );
    }
    for margin in [0u32, 8, 16, 32, 64] {
        let t = Instant::now();
        let v = r.call("test_seam", &[Value::I32(margin as i32)]);
        eprintln!(
            "seam with a margin of {margin} cells ({} m): {:.1}% of cells another species, height off by {:.2} m on average ({:.1} s with the truth's wood)",
            margin * 4,
            v[0] * 100.0,
            v[1],
            t.elapsed().as_secs_f64()
        );
    }
    for margin in [0u32, 32, 64] {
        let t = Instant::now();
        r.call("test_tile", &[Value::I32(margin as i32)]);
        eprintln!("a 256 m tile with a margin of {margin} cells grows in {:.3} s", t.elapsed().as_secs_f64());
    }
    let t = Instant::now();
    let w = r.call("test_window", &[]);
    eprintln!(
        "endless window ({:.1} s): dead {:.1}% (no draw but features {:.1}%); the tower in sight from {:.1}%; the road's path draws from {:.1}%; {} of {} walkers met the road, kept to it {:.0}% of their steps after, {} left the window along it",
        t.elapsed().as_secs_f64(),
        w[0] * 100.0,
        w[1] * 100.0,
        w[2] * 100.0,
        w[3] * 100.0,
        w[4],
        w[7],
        w[5] * 100.0,
        w[6]
    );
}

/// Seeded random views where a player could stand, not chosen by the check: for a blind
/// comparison of two rounds. Writes `<REGION_BLIND>/NN.png` and a key with the check's values.
#[test]
#[ignore = "spike 17: writes images to judge"]
fn region_random() {
    let dir_name = std::env::var("REGION_BLIND").unwrap_or_else(|_| "random".to_string());
    let mut r = Region::load("region-random");
    r.settle(4);
    let mut rng = Rng(4242);
    let dir = repo_root().join("target/tmp/region").join(&dir_name);
    std::fs::create_dir_all(&dir).expect("make the directory");
    let mut key = String::from("id,x,z,yaw,ahead,kind,best,count\n");
    let mut n = 0;
    let mut tried = 0;
    while n < 20 && tried < 2000 {
        tried += 1;
        let x = -150.0 + 900.0 * rng.next();
        let z = -650.0 + 900.0 * rng.next();
        let yaw = std::f32::consts::TAU * rng.next();
        let (_, walk) = r.stand(x, z);
        if walk < 0.5 || r.call("test_brush", &[Value::F32(x), Value::F32(z)])[0] > 0.08 {
            continue;
        }
        n += 1;
        let d = r.call("test_draw", &[Value::F32(x), Value::F32(z), Value::F32(yaw), Value::F32(0.55)]);
        r.look(x, z, yaw, 0.0);
        r.settle(24);
        r.save(&format!("{dir_name}/{n:02}.png"));
        key.push_str(&format!("{n:02},{x:.1},{z:.1},{yaw:.3},{:.3},{},{:.3},{}\n", d[0], d[1], d[4], d[5]));
    }
    std::fs::write(repo_root().join("target/tmp/region").join(format!("{dir_name}-key.csv")), key)
        .expect("write the key");
    eprintln!("random: {n} stills ({tried} places tried)");
}

/// What each part costs: the history whole, the bake's decisions read, their consequences, the
/// check.
#[test]
#[ignore = "spike 17: prints numbers"]
fn region_costs() {
    let mut r = Region::load("region-costs");
    for name in ["test_time_decode", "test_time_consequences", "test_time_check", "test_time_history"] {
        let t = Instant::now();
        let v = r.call(name, &[]);
        eprintln!("cost {name}: {:.2} s ({:.3})", t.elapsed().as_secs_f64(), v[0]);
    }
}

/// The endless window's check alone.
#[test]
#[ignore = "spike 17: prints numbers"]
fn region_window() {
    let mut r = Region::load("region-window");
    let t = Instant::now();
    let w = r.call("test_window", &[]);
    eprintln!(
        "endless window ({:.1} s): dead {:.1}% (no draw but features {:.1}%); the tower in sight from {:.1}%; the road's path draws from {:.1}%; {} of {} walkers met the road, kept to it {:.0}% of their steps after, {} left the window along it",
        t.elapsed().as_secs_f64(), w[0] * 100.0, w[1] * 100.0, w[2] * 100.0, w[3] * 100.0, w[4], w[7], w[5] * 100.0, w[6]
    );
}

/// The map and the check's map alone.
#[test]
#[ignore = "spike 17: writes images to judge"]
fn region_maps() {
    let mut r = Region::load("region-maps");
    r.settle(2);
    r.call("test_chart", &[Value::I32(0)]);
    r.steps(2);
    r.save("map.png");
    r.call("test_chart", &[Value::I32(1)]);
    r.steps(2);
    r.save("map-check.png");
}
