//! Swell (games/swell): its own tests on the CPU; a scripted ride that launches, lands a clean
//! Shell Spin, bails, and starts again (the test checks it did), replaying from its tick log to
//! the same hashes with any number of helpers, a log with one record changed failing at that
//! tick; in Chrome (`--include-ignored`), the same ride recorded as a tick log whose records and
//! hashes are the native host's tick for tick, whatever the frames; and on a hardware GPU, the
//! face the shaders draw agreeing with the face the sim rides.

use crate::{a_flipped_record_fails_at_its_tick, replays_with_any_helpers};
use wrela_host::{CpuBuild, Host, Value};
use wrela_tests::{f32s, one_f32, one_vec4, repo_root};

const TICKS: u32 = 2400;

/// The package's own tests (`wrela test games/swell`): the wave's, the course's, the contact
/// rule's fixtures, a corridor bot on the hardest course, the grid's fit to the face, every
/// hazard's warning time, and one presented state between snapshots.
#[test]
fn swells_own_tests_pass() {
    assert_eq!(super::tests_pass(&repo_root().join("games/swell")), 12);
}

/// A ride at 120 ticks a second: Space to start; up the face and off the lip; a whole Shell
/// Spin (Space held 63 ticks at 12 radians a second), the board turned down the face, landed
/// clean; up and off again, the board still pointing up the face, a bail; R to start again.
fn script() -> String {
    let events = [
        (10, "keydown", "Space"),
        (14, "keyup", "Space"),
        (60, "keydown", "ArrowUp"),
        (166, "keyup", "ArrowUp"),
        (168, "keydown", "Space"),
        (231, "keyup", "Space"),
        (232, "keydown", "ArrowDown"),
        (272, "keyup", "ArrowDown"),
        (330, "keydown", "ArrowUp"),
        (600, "keyup", "ArrowUp"),
        (700, "keydown", "KeyR"),
        (704, "keyup", "KeyR"),
        (760, "keydown", "ArrowUp"),
        (790, "keyup", "ArrowUp"),
    ];
    let lines: Vec<String> = events
        .iter()
        .map(|(t, kind, key)| format!(r#"{{"tick": {t}, "type": "{kind}", "key": "{key}"}}"#))
        .collect();
    format!("[\n  {}\n]\n", lines.join(",\n  "))
}

/// The first tick at which `log` has a record.
fn first_record(log: &wrela_host::TickLog) -> usize {
    log.ticks.iter().position(|t| !t.records.is_empty()).expect("a tick with records")
}

#[test]
fn a_ride_does_what_it_should_replays_and_a_changed_record_fails_at_its_tick() {
    let (dir, _) = wrela_tests::page("games/swell", "swell-replay");
    let built = CpuBuild::load(&dir).expect("load");
    let script = wrela_host::parse_script(&script()).expect("a script");
    // The ride did what it should: run in lockstep, frames and all, then asked.
    let mut host = built.start_with(2).expect("start");
    for i in 0..TICKS / 2 {
        host.lockstep_frame(i, 60.0, 64, 64, &script).expect("a frame");
    }
    let m = one_vec4(&mut host, "milestones", &[]);
    let runs = one_f32(&mut host, "runs_started", &[]);
    assert!(m[0] >= 2.0, "launches: {m:?}");
    assert!(m[1] >= 1.0 && m[2] >= 1.0, "a clean Shell Spin: {m:?}");
    assert!(m[3] >= 1.0, "a bail: {m:?}");
    assert!(runs >= 2.0, "started again: {runs}");
    // Its tick log replays, and a changed record fails at its tick.
    let log = built.record_ticks(TICKS, &script, 2).expect("record");
    assert_eq!(log.ticks.len(), TICKS as usize);
    let read = replays_with_any_helpers(&built, &log);
    a_flipped_record_fails_at_its_tick(&built, &read, first_record(&read));
}

#[test]
#[ignore = "needs the runtime"]
fn a_ride_recorded_in_chrome_is_the_native_hosts_tick_for_tick() {
    let (dir, rel) = wrela_tests::page("games/swell", "swell-chrome");
    // Lockstep at 60 frames a second, back to back: frame i follows the ticks up to
    // 2 (i + 1), so the frames come at other ticks than the native host's (none).
    let run = wrela_tests::ChromeRun {
        workers: 4,
        saturate: true,
        script: Some(script()),
        ..wrela_tests::ChromeRun::new(TICKS / 2, 64, 64, 60.0)
    };
    let chrome = wrela_tests::run_in_chrome_with(&rel, run);
    let log = chrome.ticks.and_then(|t| t.log).expect("the ticker's tick log");
    assert_eq!(log.ticks.len(), TICKS as usize);
    let built = CpuBuild::load(&dir).expect("load");
    assert_eq!(built.replay(&log, 2).expect("the native host replays Chrome's log"), TICKS);
    let script = wrela_host::parse_script(&script()).expect("a script");
    let native = built.record_ticks(TICKS, &script, 1).expect("record");
    for (k, (a, b)) in log.ticks.iter().zip(&native.ticks).enumerate() {
        assert_eq!(a.records, b.records, "tick {k}'s records");
        assert_eq!(a.hash, b.hash, "tick {k}'s hash");
    }
}

/// The face the shaders draw is the face the sim rides: on a 64 × 64 grid across 14 of the line
/// (the curl's slump among it) and the face from the flat sea to past the lip, the GPU's height
/// is within 0.5 mm of the CPU's and its slope up the face within 1e-3 of its size. One
/// function, compiled to both; the GPU's arithmetic isn't the CPU's bits.
#[test]
#[ignore = "needs a GPU"]
fn the_face_drawn_on_the_gpu_is_the_face_the_sim_rides() {
    let (dir, _) = wrela_tests::page("games/swell", "swell-face");
    let mut host = Host::load(&dir).expect("load");
    host.frame(0.0, 64, 64).expect("a frame");
    let (across, count) = (64u32, 64 * 64u32);
    let (x, z, step) = (50.0f32, -2.0f32, 14.0 / 64.0);
    let args = [
        Value::F32(x),
        Value::F32(z),
        Value::F32(step),
        Value::I32(across as i32),
        Value::I32(count as i32),
    ];
    host.call_export("probe_face", &args).expect("probe_face");
    let out = *host.buffers().last().expect("the probe's buffer");
    let gpu = f32s(&host.read_buffer(out).expect("read"));
    let (mut height, mut slope) = (0.0f32, 0.0f32);
    for i in 0..count as usize {
        let g = &gpu[i * 4..i * 4 + 4];
        let c = one_vec4(&mut host, "face_on_cpu", &[Value::F32(g[0]), Value::F32(g[1])]);
        assert_eq!((c[0], c[1]), (g[0], g[1]), "sample {i}'s place");
        height = height.max((c[2] - g[2]).abs());
        slope = slope.max((c[3] - g[3]).abs() / c[3].abs().max(1.0));
    }
    println!("the face on the GPU against the CPU: height {height:e}, slope {slope:e}");
    assert!(height < 5e-4, "height differs by {height}");
    assert!(slope < 1e-3, "slope differs by {slope}");
}
