//! `wrela tool`: builds a package's tool and runs it with the command line's grants and
//! arguments; the process's exit status is the tool's.

use crate::common::{manifest, package, wrela};

/// A tool that copies the file its first argument names from `in` to `out`, prints its secret's
/// length, and exits with as many arguments as it was given.
const COPY: &str = r#"
use std::io::print
use std::job::{Job, Step}
use std::tool::{RUNNING, answer, args, secret}
use std::tool::files

@job
fn main(given: take Vec<String>) -> u32 {
    if given.len() == 0 {
        return 64
    }
    let name = given[0].clone()
    var p = files::read(f"in/{name.as_str()}")
    var got = p.poll()
    while got.is_none() {
        yield
        got = p.poll()
    }
    let Some(read) = take got else { return 90 }
    let Ok(bytes) = answer(take read) else { return 91 }
    var w = files::write(f"out/copy/{name.as_str()}", bytes)
    var wrote = w.poll()
    while wrote.is_none() {
        yield
        wrote = w.poll()
    }
    let Some(done) = take wrote else { return 92 }
    if answer(take done).is_err() {
        return 93
    }
    if let Some(s) = secret("WRELA_TOOL_TEST_TOKEN") {
        print(f"copied {name.as_str()}; the token is {s.len()} bytes")
    }
    u32(given.len())
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

#[test]
fn wrela_tool_runs_the_tool_with_its_grants_and_exits_with_its_status() {
    let dir = package(
        "tool-copy",
        &[
            ("wrela.toml", &manifest("tool_copy")),
            ("main.wrela", COPY),
            ("data/in/notes.txt", "kept"),
            ("data/out/.keep", ""),
        ],
    );
    let (input, output) = (dir.join("data/in"), dir.join("data/out"));
    let out = wrela()
        .arg("tool")
        .arg(&dir)
        .arg("--read")
        .arg(format!("in={}", input.display()))
        .arg("--write")
        .arg(format!("out={}", output.display()))
        .args(["--secret", "WRELA_TOOL_TEST_TOKEN", "--", "notes.txt", "b", "c"])
        .env("WRELA_TOOL_TEST_TOKEN", "abcd")
        .output()
        .expect("run wrela tool");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(3), "{stdout}{stderr}");
    assert_eq!(stdout, "copied notes.txt; the token is 4 bytes\n");
    assert!(!stdout.contains("abcd") && !stderr.contains("abcd"), "the secret isn't shown");
    assert_eq!(std::fs::read_to_string(output.join("copy/notes.txt")).expect("copied"), "kept");

    // Without the write root, the write fails and the tool says so with its status.
    let out = wrela()
        .arg("tool")
        .arg(&dir)
        .arg("--read")
        .arg(format!("in={}", input.display()))
        .args(["--", "notes.txt"])
        .output()
        .expect("run wrela tool");
    assert_eq!(out.status.code(), Some(93), "{}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn wrela_tool_ends_a_tool_at_its_timeout_and_refuses_bad_grants() {
    let main = r#"
pub fn tool() -> u32 {
    var n: u32 = 0
    loop {
        n = n.wrapping_add(1)
    }
    0
}
"#;
    let dir = package("tool-loop", &[("wrela.toml", &manifest("tool_loop")), ("main.wrela", main)]);
    let started = std::time::Instant::now();
    let out = wrela().arg("tool").arg(&dir).args(["--timeout", "0.3"]).output().expect("run");
    assert_eq!(out.status.code(), Some(124), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(String::from_utf8_lossy(&out.stderr).contains("ran past its timeout"));
    assert!(started.elapsed() < std::time::Duration::from_secs(60));
    // A timeout too large for the clock is a usage error, not a panic.
    for huge in ["1e100", "1e300", "inf", "NaN", "-1"] {
        let out = wrela().arg("tool").arg(&dir).args(["--timeout", huge]).output().expect("run");
        assert_eq!(out.status.code(), Some(2), "--timeout {huge}");
        assert!(String::from_utf8_lossy(&out.stderr).contains("--timeout takes seconds"));
    }
    // A grant that names no directory, or a secret that isn't set, stops it before it starts.
    let out = wrela().arg("tool").arg(&dir).args(["--read", "src"]).output().expect("run");
    assert_eq!(out.status.code(), Some(2));
    let out = wrela()
        .arg("tool")
        .arg(&dir)
        .args(["--secret", "WRELA_TOOL_TEST_UNSET"])
        .env_remove("WRELA_TOOL_TEST_UNSET")
        .output()
        .expect("run");
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("isn't set in the environment"));
}
