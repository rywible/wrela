//! forge end to end (spec/forge.md): the service built from `forge/` and run by the service host
//! on a small git repository, played here over HTTP as an agent and a lander are. It imports the
//! repository, serves MCP, runs a check of a change's tree in its worker, lands the change, stops,
//! and starts again from its log, a torn last record cut off.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};
use wrela_host::server::ServerBuild;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A scratch directory, removed when it's dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Scratch {
        let p = std::env::temp_dir().join(format!("wrela-forge-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).expect("a scratch directory");
        // macOS keeps temporary files behind a symbolic link; the service's roots refuse links.
        Scratch(p.canonicalize().expect("its real path"))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(["-c", "user.name=Tester", "-c", "user.email=t@example.com"])
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git runs");
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
}

/// A repository of two commits, whose `tools/check.sh` says what it was asked and checks that
/// README is there.
fn repository(dir: &Path) {
    std::fs::create_dir_all(dir.join("tools")).expect("tools");
    git(dir, &["init", "-q", "-b", "main"]);
    std::fs::write(dir.join("README"), "forge test\n").expect("README");
    std::fs::write(dir.join("a.txt"), "one\ntwo\nthree\n").expect("a.txt");
    let check = dir.join("tools/check.sh");
    std::fs::write(&check, "#!/bin/sh\necho \"checked ${1:-suite}\"\ntest -f README\n")
        .expect("check.sh");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&check, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", "The start"]);
    std::fs::write(dir.join("b.txt"), "bee\n").expect("b.txt");
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", "A second file\n\nWith a body."]);
}

/// A free port on the loopback.
fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .expect("a port")
        .local_addr()
        .expect("its address")
        .port()
}

/// The service, on a thread of its own: its exit status when it ends.
fn start(build: &Path, data: &Path, work: &Path, port: u16) -> std::thread::JoinHandle<i32> {
    let listen = format!("127.0.0.1:{port}");
    run_forge(build, data, work, &["serve", "--listen", &listen, "--machine", "test"])
}

/// forge with `args`, on a thread of its own, with its two roots: its exit status.
fn run_forge(
    build: &Path,
    data: &Path,
    work: &Path,
    args: &[&str],
) -> std::thread::JoinHandle<i32> {
    let mut roots = wrela_service::files::Roots::default();
    roots.add("data", data).expect("the data root");
    roots.add("work", work).expect("the work root");
    let build = ServerBuild::load(build).expect("forge's server build");
    let args = args.iter().map(|s| s.to_string()).collect();
    std::thread::spawn(move || {
        wrela_service::run(wrela_service::Config {
            build,
            roots,
            args,
            limits: wrela_service::limits(),
        })
    })
}

/// One HTTP request: the status and the body.
fn http(port: u16, method: &str, path: &str, body: &str) -> (u32, String) {
    let mut s = TcpStream::connect(("127.0.0.1", port)).expect("forge accepts");
    s.set_read_timeout(Some(Duration::from_secs(60))).expect("a timeout");
    let req = format!(
        "{method} {path} HTTP/1.1\r\nhost: x\r\nconnection: close\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{body}",
        body.len()
    );
    s.write_all(req.as_bytes()).expect("the request is sent");
    let mut out = Vec::new();
    s.read_to_end(&mut out).expect("the answer");
    let text = String::from_utf8_lossy(&out).into_owned();
    let (head, rest) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    let status = head.split(' ').nth(1).and_then(|c| c.parse().ok()).unwrap_or(0);
    (status, rest.to_string())
}

fn call(port: u16, tool: &str, args: serde_json::Value) -> serde_json::Value {
    let body = serde_json::json!({"tool": tool, "args": args}).to_string();
    let (status, text) = http(port, "POST", "/api/call", &body);
    assert_eq!(status, 200, "{tool}: {text}");
    serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("{tool}'s answer isn't JSON ({e}): {text}"))
}

fn mcp(port: u16, id: u32, method: &str, params: serde_json::Value) -> serde_json::Value {
    let body = serde_json::json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
        .to_string();
    let (status, text) = http(port, "POST", "/mcp", &body);
    assert_eq!(status, 200, "{method}: {text}");
    let v: serde_json::Value = serde_json::from_str(&text).expect("a JSON-RPC answer");
    assert_eq!(v["id"], id, "{text}");
    v["result"].clone()
}

/// What an MCP tool call said: its JSON.
fn mcp_tool(port: u16, id: u32, name: &str, args: serde_json::Value) -> serde_json::Value {
    let r = mcp(port, id, "tools/call", serde_json::json!({"name": name, "arguments": args}));
    assert_eq!(r["isError"], false, "{name}: {r}");
    serde_json::from_str(r["content"][0]["text"].as_str().expect("text")).expect("the tool's JSON")
}

fn wait_until(what: &str, mut ok: impl FnMut() -> bool) {
    let start = Instant::now();
    while !ok() {
        assert!(start.elapsed() < Duration::from_secs(60), "{what} didn't happen in a minute");
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn ready(port: u16) {
    wait_until("forge listening", || {
        TcpStream::connect(("127.0.0.1", port)).is_ok() && http(port, "GET", "/health", "").0 == 200
    });
}

fn job_state(port: u16, job: &str) -> serde_json::Value {
    call(port, "job", serde_json::json!({"job": job}))
}

#[test]
fn forges_own_tests_pass() {
    assert!(super::tests_pass(&repo_root().join("forge")) > 0);
}

#[test]
fn forge_imports_serves_checks_lands_and_starts_again_from_its_log() {
    let scratch = Scratch::new("e2e");
    let (repo, build, data, work) = (
        scratch.0.join("repo"),
        scratch.0.join("build"),
        scratch.0.join("data"),
        scratch.0.join("work"),
    );
    for d in [&repo, &data, &work] {
        std::fs::create_dir_all(d).expect("a directory");
    }
    repository(&repo);
    wrela_tests::must_build(&repo_root().join("forge"), &build);
    let port = free_port();
    let service = start(&build, &data, &work, port);
    ready(port);

    // The import: both commits, as revisions of main.
    let (status, text) = http(
        port,
        "POST",
        "/api/import",
        &serde_json::json!({"git": repo, "ref": "main"}).to_string(),
    );
    assert_eq!(status, 202, "{text}");
    wait_until("the import", || call(port, "log", serde_json::json!({}))["main"] == 2);
    let log = call(port, "log", serde_json::json!({}));
    assert_eq!(log["landings"][0]["intent"], "A second file");
    let readme = call(port, "read", serde_json::json!({"path": "README"}));
    assert_eq!(readme["text"], "forge test\n");
    let check = call(port, "list", serde_json::json!({"dir": "tools"}));
    assert_eq!(check["entries"][0]["exec"], true, "check.sh is executable: {check}");

    // An agent over MCP: a change, an edit, its own view, a check of its tree.
    let init = mcp(port, 1, "initialize", serde_json::json!({"protocolVersion": "2025-06-18"}));
    assert_eq!(init["serverInfo"]["name"], "forge");
    let tools = mcp(port, 2, "tools/list", serde_json::json!({}));
    assert!(tools["tools"].as_array().expect("tools").len() >= 10);
    let opened = mcp_tool(port, 3, "forge_open", serde_json::json!({"intent": "Two becomes 2"}));
    let change = opened["change"].as_str().expect("a change").to_string();
    mcp_tool(
        port,
        4,
        "forge_edit",
        serde_json::json!({"change": change, "path": "a.txt", "old": "two", "new": "2"}),
    );
    let mine =
        mcp_tool(port, 5, "forge_read", serde_json::json!({"change": change, "path": "a.txt"}));
    assert_eq!(mine["text"], "one\n2\nthree\n");
    let main_side = call(port, "read", serde_json::json!({"path": "a.txt"}));
    assert_eq!(main_side["text"], "one\ntwo\nthree\n", "main is unchanged");
    let checked =
        mcp_tool(port, 6, "forge_check", serde_json::json!({"change": change, "check": "rustfmt"}));
    let job = checked["job"].as_str().expect("a job").to_string();
    wait_until("the check", || job_state(port, &job)["state"] == "passed");
    let done = job_state(port, &job);
    assert!(
        done["output_tail"].as_str().expect("its output").contains("checked rustfmt"),
        "{done}"
    );
    assert_eq!(
        std::fs::read_to_string(work.join("a.txt")).expect("the checkout"),
        "one\n2\nthree\n"
    );

    // It can't land before its suite passes on its exact tree; then it can.
    let (refused, why) = http(
        port,
        "POST",
        "/api/call",
        &serde_json::json!({"tool": "land", "args": {"change": change}}).to_string(),
    );
    assert_eq!(refused, 422, "{why}");
    let suite = call(port, "submit", serde_json::json!({"change": change}));
    let suite_job = suite["job"].as_str().expect("the suite's job").to_string();
    wait_until("the suite", || job_state(port, &suite_job)["state"] == "passed");
    let landed = call(port, "land", serde_json::json!({"change": change}));
    assert_eq!(landed["seq"], 3);
    let after = call(port, "read", serde_json::json!({"path": "a.txt"}));
    assert_eq!(after["text"], "one\n2\nthree\n", "main holds the change");
    let digest = http(port, "GET", "/", "");
    assert!(digest.1.contains("Two becomes 2"), "the digest names the landing");

    // A stop, a torn record at the log's end, and a start: the state is the log's.
    assert_eq!(http(port, "POST", "/api/stop", "").0, 200);
    assert_eq!(service.join().expect("the service's thread"), 0);
    let events = data.join("events");
    let whole = std::fs::metadata(&events).expect("the log").len();
    let mut f = std::fs::OpenOptions::new().append(true).open(&events).expect("the log");
    f.write_all(&[40, 0, 0, 0, 1, 2, 3]).expect("a torn record");
    drop(f);
    let port2 = free_port();
    let again = start(&build, &data, &work, port2);
    ready(port2);
    assert_eq!(
        std::fs::metadata(&events).expect("the log").len(),
        whole,
        "the torn record is cut off"
    );
    let log2 = call(port2, "log", serde_json::json!({}));
    assert_eq!(log2["main"], 3);
    assert_eq!(log2["landings"][0]["intent"], "Two becomes 2");
    let read2 = call(port2, "read", serde_json::json!({"path": "a.txt"}));
    assert_eq!(read2["text"], "one\n2\nthree\n");
    assert_eq!(http(port2, "POST", "/api/stop", "").0, 200);
    assert_eq!(again.join().expect("the service's thread"), 0);
}

#[test]
fn a_worker_on_another_machine_runs_a_check_and_the_service_keeps_its_evidence() {
    let scratch = Scratch::new("remote");
    let dirs: Vec<PathBuf> = ["repo", "build", "data", "work", "wdata", "wwork"]
        .iter()
        .map(|d| scratch.0.join(d))
        .collect();
    for d in &dirs {
        std::fs::create_dir_all(d).expect("a directory");
    }
    let (repo, build, data, work, wdata, wwork) =
        (&dirs[0], &dirs[1], &dirs[2], &dirs[3], &dirs[4], &dirs[5]);
    repository(repo);
    wrela_tests::must_build(&repo_root().join("forge"), build);
    let port = free_port();
    let listen = format!("127.0.0.1:{port}");
    // The service runs no checks itself: its worker is elsewhere.
    let service = run_forge(build, data, work, &["serve", "--listen", &listen, "--no-worker"]);
    ready(port);
    http(port, "POST", "/api/import", &serde_json::json!({"git": repo, "ref": "main"}).to_string());
    wait_until("the import", || call(port, "log", serde_json::json!({}))["main"] == 2);
    let change = call(port, "open", serde_json::json!({"intent": "A remote check"}))["change"]
        .as_str()
        .expect("a change")
        .to_string();
    call(port, "write", serde_json::json!({"change": change, "path": "c.txt", "content": "sea\n"}));
    let job = call(port, "check", serde_json::json!({"change": change, "check": "wgsl"}))["job"]
        .as_str()
        .expect("a job")
        .to_string();
    let worker = run_forge(
        build,
        wdata,
        wwork,
        &["work", "--server", &listen, "--machine", "far", "--jobs", "1"],
    );
    wait_until("the remote check", || job_state(port, &job)["state"] == "passed");
    let done = job_state(port, &job);
    assert_eq!(done["worker"], "far");
    assert!(done["output_tail"].as_str().expect("its output").contains("checked wgsl"), "{done}");
    assert_eq!(std::fs::read_to_string(wwork.join("c.txt")).expect("the remote checkout"), "sea\n");
    assert!(!work.join("c.txt").exists(), "the service's own checkout ran nothing");
    assert_eq!(worker.join().expect("the worker's thread"), 0, "it stops after its one job");
    assert_eq!(http(port, "POST", "/api/stop", "").0, 200);
    assert_eq!(service.join().expect("the service's thread"), 0);
}
