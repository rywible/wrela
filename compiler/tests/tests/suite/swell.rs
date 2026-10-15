//! Swell (games/swell): its own tests on the CPU; a scripted ride that replays from its tick log
//! to the same hashes with any number of helpers, a log with one record changed failing at that
//! tick; in Chrome (`--include-ignored`), the same ride recorded as a tick log, whose records and
//! hashes are the native host's tick for tick, whatever the frames; and on a hardware GPU, the
//! sea the shader draws agreeing with the sea the sim rides, at a busy moment.

use crate::{a_flipped_record_fails_at_its_tick, replays_with_any_helpers};
use wrela_host::{CpuBuild, Host, Value};
use wrela_tests::{f32s, one_vec4, repo_root};

const TICKS: u32 = 2400;

/// The package's own tests (`wrela test games/swell`): the sea's, the course's, the contact
/// rule's fixtures and the bots that finish both courses. About a second.
#[test]
fn swells_own_tests_pass() {
    assert_eq!(super::tests_pass(&repo_root().join("games/swell")), 12);
}

/// A ride over `ticks` ticks (at 120 a second): Space to start, then swells held and let go at
/// uneven times, a lean now and then.
fn script(ticks: u32) -> String {
    let mut events = vec![
        r#"{"tick": 10, "type": "keydown", "key": "Space"}"#.to_string(),
        r#"{"tick": 14, "type": "keyup", "key": "Space"}"#.to_string(),
    ];
    let mut t = 60;
    let mut i = 0u32;
    while t + 120 < ticks {
        let hold = 20 + (i * 17) % 70;
        events.push(format!(r#"{{"tick": {t}, "type": "keydown", "key": "Space"}}"#));
        events.push(format!(r#"{{"tick": {}, "type": "keyup", "key": "Space"}}"#, t + hold));
        if i % 3 == 2 {
            events.push(format!(
                r#"{{"tick": {}, "type": "keydown", "key": "ArrowRight"}}"#,
                t + hold + 5
            ));
            events.push(format!(
                r#"{{"tick": {}, "type": "keyup", "key": "ArrowRight"}}"#,
                t + hold + 40
            ));
        }
        t += hold + 60 + (i * 29) % 50;
        i += 1;
    }
    format!("[\n  {}\n]\n", events.join(",\n  "))
}

/// The first tick at which `log` has a record.
fn first_record(log: &wrela_host::TickLog) -> usize {
    log.ticks.iter().position(|t| !t.records.is_empty()).expect("a tick with records")
}

#[test]
fn a_ride_replays_and_a_changed_record_fails_at_its_tick() {
    let (dir, _) = wrela_tests::page("games/swell", "swell-replay");
    let built = CpuBuild::load(&dir).expect("load");
    let script = wrela_host::parse_script(&script(TICKS)).expect("a script");
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
        script: Some(script(TICKS)),
        ..wrela_tests::ChromeRun::new(TICKS / 2, 64, 64, 60.0)
    };
    let chrome = wrela_tests::run_in_chrome_with(&rel, run);
    let log = chrome.ticks.and_then(|t| t.log).expect("the ticker's tick log");
    assert_eq!(log.ticks.len(), TICKS as usize);
    let built = CpuBuild::load(&dir).expect("load");
    assert_eq!(built.replay(&log, 2).expect("the native host replays Chrome's log"), TICKS);
    let script = wrela_host::parse_script(&script(TICKS)).expect("a script");
    let native = built.record_ticks(TICKS, &script, 1).expect("record");
    for (k, (a, b)) in log.ticks.iter().zip(&native.ticks).enumerate() {
        assert_eq!(a.records, b.records, "tick {k}'s records");
        assert_eq!(a.hash, b.hash, "tick {k}'s hash");
    }
}

/// The sea the shader draws is the sea the sim rides: at 4096 points across three overlapping
/// swells (rising, surging, settling) on the fixed course's hills, the GPU's height is within
/// 0.5 mm of the CPU's (a hundredth of a pixel at 1080p), and its slope (the surface's facing)
/// within 1e-3 of its size. One function, compiled to both; the GPU's arithmetic isn't the CPU's
/// bits.
#[test]
#[ignore = "needs a GPU"]
fn the_sea_drawn_on_the_gpu_is_the_sea_the_sim_rides() {
    let (dir, _) = wrela_tests::page("games/swell", "swell-sea");
    let mut host = Host::load(&dir).expect("load");
    host.frame(0.0, 64, 64).expect("a frame");
    let count = 4096u32;
    let (from, step) = (30.0f32, 20.0 / count as f32);
    let args = [Value::F32(from), Value::F32(step), Value::I32(count as i32)];
    host.call_export("probe_sea", &args).expect("probe_sea");
    let out = *host.buffers().last().expect("the probe's buffer");
    let gpu = f32s(&host.read_buffer(out).expect("read"));
    let (mut height, mut slope) = (0.0f32, 0.0f32);
    for i in 0..count as usize {
        let g = &gpu[i * 4..i * 4 + 4];
        let c = one_vec4(&mut host, "sea_on_cpu", &[Value::F32(g[0])]);
        assert_eq!(c[0], g[0], "sample {i}'s x");
        height = height.max((c[1] - g[1]).abs());
        slope = slope.max((c[2] - g[2]).abs() / c[2].abs().max(1.0));
    }
    println!("the sea on the GPU against the CPU: height {height:e}, slope {slope:e}");
    assert!(height < 5e-4, "height differs by {height}");
    assert!(slope < 1e-3, "slope differs by {slope}");
}
