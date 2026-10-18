//! `std::tool::drive` (language.md §15.5): a tool loads a build on the native host and runs it:
//! the same frames and script give the same hash as the host run directly, a GPU frame's capture
//! comes back through a blob, an export is called, a trap says what and where, and a deadline
//! stops WASM that never ends.

use crate::common::{manifest, package, wrela};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

/// A tool that does what its arguments say with the build in `src/build` (or `fx`'s, for the
/// GPU), and prints what it got:
/// - `cpu`: frames with a script on the CPU (the script goes to `out/script.json`), the hash,
///   a call of `add`, a trap in `boom`, and a frame after the trap;
/// - `gpu`: three frames on the GPU, the screen captured and written to `out/screen.rgba`, the
///   hash;
/// - `spin`: a call of `spin`, which never ends, with a deadline of 1 s, then the hash;
/// - `spin-forever`: the same with no deadline of its own (the tool's `--timeout` ends it).
const TOOL: &str = r#"
use std::input::{Key, Script}
use std::io::{IoError, print}
use std::job::{Job, Step}
use std::tool::{RUNNING, ToolError, answer, args}
use std::tool::blob::Data
use std::tool::drive
use std::tool::files

const SCRIPT: Script = Script::frames()
    .move_to(1, vec2(10.0, 20.0))
    .key_down(2, Key::ShiftLeft)
    .move_to(4, vec2(30.0, 40.0))
    .key_up(5, Key::ShiftLeft)

fn failed(e: ToolError) -> u32 {
    print(f"error: {e.message()}")
    1
}

fn utf8(text: str) -> Vec<u8> {
    var out: Vec<u8> = Vec::new()
    for i in 0..text.len() {
        out.push(text.byte(i))
    }
    out
}

@job
fn main(given: take Vec<String>) -> u32 {
    let what = given[0].clone()
    var options = if what.as_str() == "gpu" { drive::gpu() } else { drive::cpu() }
    options.workers = 2
    let at = if what.as_str() == "gpu" { "fx" } else { "src/build" }
    var l = drive::load(at, options)
    var lg = l.poll()
    while lg.is_none() {
        yield
        lg = l.poll()
    }
    let Some(lp) = take lg else { return 90 }
    let loaded = match drive::read_loaded(take lp) {
        Ok(x) => x,
        Err(e) => return failed(e),
    }
    let b = loaded.build
    var i = drive::init(b)
    var ig = i.poll()
    while ig.is_none() {
        yield
        ig = i.poll()
    }
    let Some(ip) = take ig else { return 90 }
    let Ok(init) = drive::read_ran(take ip) else { return 91 }
    assert(init.trap.is_none(), "init")
    if what.as_str() == "cpu" {
        var w = files::write("out/script.json", utf8(SCRIPT.as_json()))
        while w.poll().is_none() { yield }
        var f = drive::frames(b, 8, SCRIPT)
        var fg = f.poll()
        while fg.is_none() {
            yield
            fg = f.poll()
        }
        let Some(fp) = take fg else { return 90 }
        let ran = match drive::read_ran(take fp) {
            Ok(r) => take r,
            Err(e) => return failed(e),
        }
        print(f"frames {ran.steps.len()}")
        var c = drive::call(b, "add", [drive::Scalar::I32(2), drive::Scalar::I32(40)])
        var cg = c.poll()
        while cg.is_none() {
            yield
            cg = c.poll()
        }
        let Some(cp) = take cg else { return 90 }
        let Ok(called) = drive::read_ran(take cp) else { return 92 }
        let drive::Scalar::I32(sum) = called.results[0] else { return 93 }
        print(f"sum {sum}")
        var t = drive::call(b, "boom", [drive::Scalar::I32(13)])
        var tg = t.poll()
        while tg.is_none() {
            yield
            tg = t.poll()
        }
        let Some(tp) = take tg else { return 90 }
        let Ok(boomed) = drive::read_ran(take tp) else { return 94 }
        let Some(trap) = boomed.trap else { return 95 }
        print(f"trap call={trap.stage == drive::Stage::Call} panic={trap.panic.as_str()} at={trap.at.as_str()}")
        var a = drive::frames(b, 1, Script::frames())
        var ag = a.poll()
        while ag.is_none() {
            yield
            ag = a.poll()
        }
        let Some(apol) = take ag else { return 90 }
        if let Err(e) = drive::read_ran(take apol) {
            print(f"after: {e.message()}")
        }
    } else if what.as_str() == "gpu" {
        var f = drive::frames(b, 3, Script::frames())
        while f.poll().is_none() { yield }
        var c = drive::capture(b)
        var cg = c.poll()
        while cg.is_none() {
            yield
            cg = c.poll()
        }
        let Some(cp) = take cg else { return 90 }
        let shot = match drive::read_capture(take cp) {
            Ok(s) => take s,
            Err(e) => return failed(e),
        }
        let Data::Blob(blob) = shot.rgba else { return 96 }
        var w = files::write_blob("out/screen.rgba", blob)
        var wg = w.poll()
        while wg.is_none() {
            yield
            wg = w.poll()
        }
        let Some(wp) = take wg else { return 90 }
        if let Err(e) = answer(take wp) {
            return failed(e)
        }
        print(f"captured {shot.width}x{shot.height}")
    } else {
        let timeout = if what.as_str() == "spin" { 1.0 } else { 0.0 }
        var s = drive::call(b, "spin", [drive::Scalar::I32(0)], timeout: timeout)
        var sg = s.poll()
        while sg.is_none() {
            yield
            sg = s.poll()
        }
        let Some(sp) = take sg else { return 90 }
        if let Err(e) = drive::read_ran(take sp) {
            print(f"spin: {e.message()}")
        }
    }
    var h = drive::hash(b)
    var hg = h.poll()
    while hg.is_none() {
        yield
        hg = h.poll()
    }
    let Some(hp) = take hg else { return 90 }
    let Ok(hash) = drive::read_hash(take hp) else { return 97 }
    print(f"hash {hash}")
    var x = drive::close(b)
    while x.poll().is_none() { yield }
    0
}

pub fn tool_init() -> Job<main> {
    main.start(args())
}

pub fn tool(job: mut Job<main>) -> u32 {
    match job.resume() {
        Step::Yielded => RUNNING,
        Step::Done(status) => status,
    }
}
"#;

/// A program that counts the events it reads and writes its count to a GPU buffer each frame
/// (the hash covers what it submits), with exports to call: `add`, `boom`, which panics on 13,
/// and `spin`, which never ends.
const SUBJECT: &str = r#"
use std::gpu::{GpuBuffer, buffer, write}
use std::input::events

pub struct Game {
    events: u32,
    counts: GpuBuffer<u32>,
}

pub fn init() -> Game {
    Game { events: 0, counts: buffer(1) }
}

pub fn frame(game: mut Game, time: f32, width: u32, height: u32) {
    for _ in events() {
        game.events += 1
    }
    write(mut game.counts, 0, [game.events])
}

pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

pub fn boom(n: i32) -> i32 {
    assert(n != 13, f"boom {n}")
    n
}

pub fn spin(n: i32) -> i32 {
    var i = n
    while i >= 0 {
        i = (i + 1) % 1000
    }
    i
}
"#;

/// The tool's package, the subject built into `src/build`, and the write root `out`, in a
/// scratch directory `name`.
fn setup(name: &str) -> (PathBuf, PathBuf, PathBuf) {
    let dir = package(
        name,
        &[
            ("tool/wrela.toml", &manifest("driver")),
            ("tool/main.wrela", TOOL),
            ("subject/wrela.toml", &manifest("subject")),
            ("subject/main.wrela", SUBJECT),
        ],
    );
    let (src, out) = (dir.join("src"), dir.join("out"));
    std::fs::create_dir_all(&out).expect("out");
    let built = wrela()
        .arg("build")
        .arg(dir.join("subject"))
        .arg("-o")
        .arg(src.join("build"))
        .output()
        .expect("wrela build");
    assert!(built.status.success(), "{}", String::from_utf8_lossy(&built.stderr));
    (dir.join("tool"), src, out)
}

/// `wrela tool` on `tool` with `src` read-only, `out` writable, the first-light fixture as `fx`,
/// and `args`.
fn drive(tool: &Path, src: &Path, out: &Path, timeout: &str, args: &[&str]) -> Command {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/fixtures/first-light");
    let mut c = wrela();
    c.arg("tool").arg(tool);
    c.arg("--read").arg(format!("src={}", src.display()));
    c.arg("--read").arg(format!("fx={}", fixture.display()));
    c.arg("--write").arg(format!("out={}", out.display()));
    c.arg("--timeout").arg(timeout).arg("--").args(args);
    c
}

/// Runs `command`: its exit code and what it printed.
fn run(command: &mut Command) -> (Option<i32>, String) {
    let out = command.output().expect("run wrela tool");
    let text =
        format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    (out.status.code(), text)
}

/// The number after `key ` in `text`'s line that starts with it.
fn value<'a>(text: &'a str, key: &str) -> &'a str {
    let line = text.lines().find(|l| l.starts_with(key)).unwrap_or_else(|| panic!("{text}"));
    line[key.len()..].trim()
}

#[test]
fn frames_on_the_cpu_give_the_hosts_hash_and_a_trap_says_what_and_where() {
    let (tool, src, out) = setup("drive-cpu");
    let (code, printed) = run(&mut drive(&tool, &src, &out, "60", &["cpu"]));
    assert_eq!(code, Some(0), "{printed}");
    assert!(printed.contains("frames 8"), "{printed}");
    assert!(printed.contains("sum 42"), "an export's result: {printed}");
    assert!(printed.contains("trap call=true panic=boom 13 at=main.wrela:"), "{printed}");
    assert!(printed.contains("after: the build runs nothing more: it trapped"), "{printed}");
    // The same build, script and frames on the host, directly. The trap's call added nothing.
    let text = std::fs::read_to_string(out.join("script.json")).expect("the tool's script");
    let script = wrela_host::parse_script(&text).expect("a script");
    let mut host = wrela_host::CpuBuild::load(src.join("build"))
        .expect("load")
        .quiet()
        .start_with(2)
        .expect("start");
    for i in 0..8 {
        host.lockstep_frame(i, 60.0, 640, 360, &script).expect("a frame");
    }
    assert_eq!(value(&printed, "hash"), host.hash().to_string(), "{printed}");
    let mut plain =
        wrela_host::CpuBuild::load(src.join("build")).expect("load").start().expect("start");
    for i in 0..8 {
        plain.lockstep_frame(i, 60.0, 640, 360, &[]).expect("a frame");
    }
    assert_ne!(plain.hash(), host.hash(), "the script changes what the program prints");
}

#[test]
fn a_gpu_frames_capture_comes_back_through_a_blob() {
    let (tool, src, out) = setup("drive-gpu");
    let (code, printed) = run(&mut drive(&tool, &src, &out, "60", &["gpu"]));
    assert_eq!(code, Some(0), "{printed}");
    assert!(printed.contains("captured 640x360"), "{printed}");
    let screen = std::fs::read(out.join("screen.rgba")).expect("the capture");
    assert_eq!(screen.len(), 640 * 360 * 4, "more than a request holds: a blob");
    // The same frames on the host, directly, on this machine's GPU.
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/fixtures/first-light");
    let storage = out.join("storage");
    let options = wrela_host::Options { storage: Some(storage), workers: 2, ..Default::default() };
    let mut host = wrela_host::Host::load_with(&fixture, &options).expect("a hardware GPU");
    let (run, _) = host.run_lockstep(3, 60.0, 640, 360, &[], false).expect("frames");
    assert_eq!(value(&printed, "hash"), run.hash.to_string(), "{printed}");
    assert_eq!(screen, run.frame, "the capture is the screen");
    assert!(screen.chunks(4).any(|p| *p != screen[..4]), "it drew something");
}

#[test]
fn a_deadline_stops_driven_wasm_that_never_ends() {
    let (tool, src, out) = setup("drive-spin");
    let started = Instant::now();
    let (code, printed) = run(&mut drive(&tool, &src, &out, "60", &["spin"]));
    assert_eq!(code, Some(0), "{printed}");
    assert!(printed.contains("spin: `call` ran past its deadline of 1 s"), "{printed}");
    assert!(printed.contains("hash "), "the build answers after its call was stopped: {printed}");
    assert!(started.elapsed() < Duration::from_secs(20), "{:?}", started.elapsed());
    // With no deadline of its own, the tool's end stops it.
    let started = Instant::now();
    let (code, printed) = run(&mut drive(&tool, &src, &out, "2", &["spin-forever"]));
    assert_eq!(code, Some(124), "{printed}");
    assert!(started.elapsed() < Duration::from_secs(20), "{:?}", started.elapsed());
}
