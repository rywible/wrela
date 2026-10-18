//! Tool tests (`@test(tool, ...)`, language.md §13.2): `wrela test` runs each in a `wrela`
//! process of its own, on the package's tool build. The fixture compiler/tests/tool_tests
//! passes (its scratch, fake clock, fake sites and fake program); here, the ways a tool test
//! fails, each within its budget, and the attribute's rules.

use crate::common::{json, manifest, package, wrela};

/// A package with a tool build, of `tests` (each a tool test or not).
fn tool_package(name: &str, tests: &str) -> std::path::PathBuf {
    let main = format!("pub fn tool() -> u32 {{\n    0\n}}\n{tests}");
    package(name, &[("wrela.toml", &manifest(&name.replace('-', "_"))), ("main.wrela", &main)])
}

/// Each test `wrela test --json` ran: its name, whether it passed, and its failure's message.
fn ran(dir: &std::path::Path) -> Vec<(String, bool, String)> {
    let out = json(wrela().arg("test").arg(dir).arg("--json"));
    let tests = out["tests"].as_array().unwrap_or_else(|| panic!("{out}"));
    tests
        .iter()
        .map(|t| {
            let message = t["failure"]["message"].as_str().unwrap_or("").to_string();
            (t["name"].as_str().expect("a name").to_string(), t["passed"] == true, message)
        })
        .collect()
}

#[test]
fn the_tool_tests_fixture_passes() {
    let dir = wrela_root().join("compiler/tests/tool_tests");
    let tests = ran(&dir);
    assert_eq!(tests.len(), 8, "{tests:?}");
    for (name, passed, message) in &tests {
        assert!(passed, "{name}: {message}");
    }
}

/// The repository's root.
fn wrela_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn a_tool_test_fails_on_a_panic_a_trap_or_its_budget_and_passes_otherwise() {
    let tests = r#"
use std::tool::files
use std::tool::time

@test(tool)
@job
fn waits_an_hour_of_fake_time() {
    var t = time::after(3600.0)
    while t.poll().is_none() { yield }
}

@test(tool)
@job
fn panics() {
    var t = time::after(1.0)
    while t.poll().is_none() { yield }
    assert(1 + 1 == 3, "on purpose")
}

@test(tool)
@job
fn traps() {
    var l = files::list("scratch")
    var got = l.poll()
    while got.is_none() {
        yield
        got = l.poll()
    }
    // An overflow, with a count the build can't know.
    var n: u32 = 4294967295
    if got.is_some() {
        n += 1
    }
}

@test(tool, budget: 1)
@job
fn spins_past_its_budget() {
    var n: u32 = 0
    loop {
        n = n.wrapping_add(1)
    }
}
"#;
    let dir = tool_package("tool-tests-ends", tests);
    let started = std::time::Instant::now();
    let tests = ran(&dir);
    // The spinning one is stopped at its budget of 1 s, not its process's bound.
    assert!(started.elapsed() < std::time::Duration::from_secs(20), "{:?}", started.elapsed());
    let get = |name: &str| tests.iter().find(|t| t.0 == name).expect(name).clone();
    assert!(get("waits_an_hour_of_fake_time").1, "{tests:?}");
    let (_, passed, message) = get("panics");
    assert!(!passed && message.contains("on purpose"), "{message}");
    let (_, passed, message) = get("traps");
    assert!(!passed && message.contains("trapped"), "{message}");
    let (_, passed, message) = get("spins_past_its_budget");
    assert!(!passed && message.contains("past its budget of 1 s"), "{message}");
}

#[test]
fn a_tool_test_is_a_job_of_no_parameters_in_a_package_with_a_tool_build() {
    let check = |name: &str, main: &str| -> Vec<(String, String)> {
        let dir = package(
            name,
            &[("wrela.toml", &manifest(&name.replace('-', "_"))), ("main.wrela", main)],
        );
        let out = json(wrela().arg("check").arg(&dir).arg("--json"));
        out["diagnostics"]
            .as_array()
            .unwrap_or_else(|| panic!("{out}"))
            .iter()
            .filter(|d| d["severity"] == "error")
            .map(|d| {
                (
                    d["code"].as_str().unwrap_or("").to_string(),
                    d["message"].as_str().unwrap_or("").to_string(),
                )
            })
            .collect()
    };
    let tool = "pub fn tool() -> u32 {\n    0\n}\n";
    let cases = [
        ("@test(tool)\nfn not_a_job() {}\n", "E0222", "is a `@job fn`"),
        ("@test(tool)\n@job\nfn takes(n: take u32) {}\n", "E0222", "takes parameters"),
        ("@test(tool, budget: 0)\n@job\nfn none() {}\n", "E0222", "`budget` is seconds"),
        ("@test(tool, net: [\"Game.Example\"])\n@job\nfn upper() {}\n", "E0222", "`net`"),
        ("@test(tool, run: [(\"x\", \"../out\")])\n@job\nfn out() {}\n", "E0222", "`run`"),
        ("@test(tool, frames: 3)\n@job\nfn frames() {}\n", "E0222", "takes `budget`"),
        ("@test(tool)\n@fuel(100)\n@job\nfn fuel() {}\n", "E0108", "`@fuel`"),
        // A test of code is no job, and keeps a constant's effects.
        ("@test\n@job\nfn plain() {}\n", "E0336", "can't be a job"),
        (
            "@test\nfn reads() {\n    let p = std::tool::time::utc_now()\n}\n",
            "E0600",
            "a test, which the build runs",
        ),
    ];
    for (i, (text, code, words)) in cases.iter().enumerate() {
        let errors = check(&format!("tool-tests-rules-{i}"), &format!("{tool}{text}"));
        assert!(errors.iter().any(|(c, m)| c == code && m.contains(words)), "{text}: {errors:?}");
    }
    // Without a tool build, there's nothing to run it on.
    let errors = check(
        "tool-tests-rules-no-tool",
        "pub fn frame(time: f32, width: u32, height: u32) {}\n@test(tool)\n@job\nfn t() {}\n",
    );
    assert!(errors.iter().any(|(c, m)| c == "E0222" && m.contains("no tool build")), "{errors:?}");
    // A good one checks clean.
    let errors = check(
        "tool-tests-rules-ok",
        &format!("{tool}@test(tool, budget: 2.5, net: [\"a.example\"])\n@job\nfn t() {{}}\n"),
    );
    assert!(errors.is_empty(), "{errors:?}");
}
