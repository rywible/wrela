//! forge end to end (spec/forge.md): the service built from `forge/` and run by the service host
//! on a small git repository, played here over HTTP as an agent and a lander are. It imports the
//! repository, serves forge's tools as its command calls them, runs a check of a change's tree in its worker, lands the change, stops,
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

/// The store each port's forge serves, so a test's calls can show its key, as the machine's own
/// command does (`x-forge-key`).
fn stores() -> &'static std::sync::Mutex<std::collections::HashMap<u16, PathBuf>> {
    static STORES: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<u16, PathBuf>>> =
        std::sync::OnceLock::new();
    STORES.get_or_init(Default::default)
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
    let listen = args.iter().position(|a| *a == "--listen").and_then(|i| args.get(i + 1));
    if let Some(port) = listen.and_then(|l| l.rsplit(':').next()?.parse::<u16>().ok()) {
        stores().lock().expect("the stores").insert(port, data.to_path_buf());
    }
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
    let store = stores().lock().expect("the stores").get(&port).cloned();
    let key = store.and_then(|d| std::fs::read_to_string(d.join("key")).ok()).unwrap_or_default();
    let req = format!(
        "{method} {path} HTTP/1.1\r\nhost: x\r\nx-forge-key: {key}\r\nconnection: close\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{body}",
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

    // An agent, as forge's command calls it: a change, an edit, its own view, a check of its tree.
    let opened = call(port, "open", serde_json::json!({"intent": "Two becomes 2"}));
    let change = opened["change"].as_str().expect("a change").to_string();
    call(
        port,
        "edit",
        serde_json::json!({"change": change, "path": "a.txt", "old": "two", "new": "2"}),
    );
    let mine = call(port, "read", serde_json::json!({"change": change, "path": "a.txt"}));
    assert_eq!(mine["text"], "one\n2\nthree\n");
    let main_side = call(port, "read", serde_json::json!({"path": "a.txt"}));
    assert_eq!(main_side["text"], "one\ntwo\nthree\n", "main is unchanged");
    let checked = call(port, "check", serde_json::json!({"change": change, "check": "rustfmt"}));
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
    // forge's own command: a call to the service, and its exit status.
    let listen = format!("127.0.0.1:{port}");
    let cli = run_forge(
        &build,
        &scratch.0.join("repo"),
        &scratch.0.join("repo"),
        &["log", "--server", &listen],
    );
    assert_eq!(cli.join().expect("the command's thread"), 0, "forge log exits 0");
    let missing = run_forge(
        &build,
        &scratch.0.join("repo"),
        &scratch.0.join("repo"),
        &["nope", "--server", &listen],
    );
    assert_eq!(missing.join().expect("the command's thread"), 2, "an unknown tool exits 2");
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
    let service = run_forge(
        build,
        data,
        work,
        &["serve", "--listen", &listen, "--machine", "test", "--no-worker", "--fleet", "127.0.0.1"],
    );
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
    // A program on the fleet's machine that doesn't show the fleet's key isn't the fleet, though
    // a rule names its address: a check there can't report work as a worker.
    let raw = |headers: &str| {
        let body = r#"{"worker":"far"}"#;
        let mut c = TcpStream::connect(("127.0.0.1", port)).expect("forge accepts");
        let req = format!(
            "POST /api/work/next HTTP/1.1\r\nhost: x\r\n{headers}connection: close\r\ncontent-length: {}\r\n\r\n{body}",
            body.len()
        );
        c.write_all(req.as_bytes()).expect("sent");
        let mut out = String::new();
        c.read_to_string(&mut out).expect("an answer");
        out
    };
    assert!(raw("").starts_with("HTTP/1.1 403"), "no fleet key, no work");
    let fleet = std::fs::read_to_string(data.join("fleet")).expect("the fleet's key");
    let bad =
        raw("x-forge-fleet: 0000000000000000000000000000000000000000000000000000000000000000\r\n");
    assert!(bad.starts_with("HTTP/1.1 403"), "another key, no work");
    assert_eq!(fleet.len(), 64);
    share_fleet_key(data, wdata);
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

#[test]
fn a_replica_holds_each_landing_before_it_is_answered_and_serves_once_promoted() {
    let scratch = Scratch::new("replica");
    let dirs: Vec<PathBuf> = ["repo", "build", "data", "work", "rdata", "rwork"]
        .iter()
        .map(|d| scratch.0.join(d))
        .collect();
    for d in &dirs {
        std::fs::create_dir_all(d).expect("a directory");
    }
    let (repo, build, data, work, rdata, rwork) =
        (&dirs[0], &dirs[1], &dirs[2], &dirs[3], &dirs[4], &dirs[5]);
    repository(repo);
    wrela_tests::must_build(&repo_root().join("forge"), build);
    let port = free_port();
    let listen = format!("127.0.0.1:{port}");
    // A landing is answered only once one replica holds it.
    let service = run_forge(
        build,
        data,
        work,
        &["serve", "--listen", &listen, "--machine", "test", "--acks", "1", "--fleet", "127.0.0.1"],
    );
    ready(port);
    share_fleet_key(data, rdata);
    let rport = free_port();
    let rlisten = format!("127.0.0.1:{rport}");
    let replica = run_forge(
        build,
        rdata,
        rwork,
        &["replica", "--server", &listen, "--machine", "r1", "--listen", &rlisten],
    );
    http(port, "POST", "/api/import", &serde_json::json!({"git": repo, "ref": "main"}).to_string());
    wait_until("the import", || call(port, "log", serde_json::json!({}))["main"] == 2);
    let change = call(port, "open", serde_json::json!({"intent": "Kept twice"}))["change"]
        .as_str()
        .expect("a change")
        .to_string();
    call(
        port,
        "write",
        serde_json::json!({"change": change, "path": "big.bin", "content": "x".repeat(3_000_000)}),
    );
    let suite = call(port, "submit", serde_json::json!({"change": change}))["job"]
        .as_str()
        .expect("a job")
        .to_string();
    wait_until("the suite", || job_state(port, &suite)["state"] == "passed");
    let landed = call(port, "land", serde_json::json!({"change": change}));
    assert_eq!(landed["seq"], 3);
    // The answer came: the replica's log holds the landing, as a prefix of the service's.
    let ours = std::fs::read(data.join("events")).expect("the service's log");
    wait_until("the replica's log", || {
        std::fs::read(rdata.join("events")).is_ok_and(|theirs| theirs.len() >= ours.len())
    });
    let theirs = std::fs::read(rdata.join("events")).expect("the replica's log");
    assert_eq!(&theirs[..ours.len()], &ours[..], "the replica's log is the service's");
    // The service stops; the replica, promoted, is the service as of its log.
    assert_eq!(http(port, "POST", "/api/stop", "").0, 200);
    assert_eq!(service.join().expect("the service's thread"), 0);
    // The replica holds its store until it stops: only then is the store another's to serve.
    stop(rport, replica);
    let port2 = free_port();
    let listen2 = format!("127.0.0.1:{port2}");
    let promoted = run_forge(
        build,
        rdata,
        rwork,
        &["serve", "--promote", "--listen", &listen2, "--machine", "promoted"],
    );
    ready(port2);
    assert_eq!(call(port2, "log", serde_json::json!({}))["main"], 3);
    let read = call(port2, "read", serde_json::json!({"path": "big.bin", "lines": 1}));
    assert_eq!(read["size"], 3_000_000, "a file larger than a request, whole on the replica");
    assert_eq!(call(port2, "read", serde_json::json!({"path": "README"}))["text"], "forge test\n");
    assert_eq!(http(port2, "POST", "/api/stop", "").0, 200);
    assert_eq!(promoted.join().expect("the promoted service's thread"), 0);
}

/// A change on `port` that writes `path`, its suite passed: landed, and the landing's place.
fn land_one(port: u16, path: &str, text: &str) -> u64 {
    let change =
        call(port, "open", serde_json::json!({"intent": format!("Adds {path}")}))["change"]
            .as_str()
            .expect("a change")
            .to_string();
    call(port, "write", serde_json::json!({"change": change, "path": path, "content": text}));
    let suite = call(port, "submit", serde_json::json!({"change": change}))["job"]
        .as_str()
        .expect("a job")
        .to_string();
    wait_until("the suite", || job_state(port, &suite)["state"] == "passed");
    call(port, "land", serde_json::json!({"change": change}))["seq"].as_u64().expect("a place")
}

/// Gives store or worker `to` the fleet's key that store `from` made: as an operator copies it.
fn share_fleet_key(from: &Path, to: &Path) {
    std::fs::copy(from.join("fleet"), to.join("fleet")).expect("the fleet's key");
}

fn position(port: u16) -> serde_json::Value {
    let (status, text) = http(port, "GET", "/api/position", "");
    assert_eq!(status, 200, "{text}");
    serde_json::from_str(&text).expect("a position")
}

fn stop(port: u16, thread: std::thread::JoinHandle<i32>) {
    assert_eq!(http(port, "POST", "/api/stop", "").0, 200);
    assert_eq!(thread.join().expect("its thread"), 0, "it stopped cleanly");
}

#[test]
fn three_stores_fail_over_without_losing_a_confirmed_landing_and_the_old_service_is_fenced() {
    let scratch = Scratch::new("fleet");
    let dir = |d: &str| {
        let p = scratch.0.join(d);
        std::fs::create_dir_all(&p).expect("a directory");
        p
    };
    let (repo, build) = (dir("repo"), dir("build"));
    let (pdata, pwork, adata, awork, bdata, bwork) =
        (dir("p"), dir("pw"), dir("a"), dir("aw"), dir("b"), dir("bw"));
    repository(&repo);
    wrela_tests::must_build(&repo_root().join("forge"), &build);
    let (pp, pa, pb) = (free_port(), free_port(), free_port());
    let (p, a, b) =
        (format!("127.0.0.1:{pp}"), format!("127.0.0.1:{pa}"), format!("127.0.0.1:{pb}"));
    let peers_of = |x: &str, y: &str| format!("{x},{y}");
    let replica = |data: &Path, work: &Path, name: &str, listen: &str, servers: String| {
        run_forge(
            &build,
            data,
            work,
            &[
                "replica",
                "--servers",
                &servers,
                "--machine",
                name,
                "--listen",
                listen,
                "--fleet",
                "127.0.0.1",
            ],
        )
    };

    // P serves, confirming each answer once one of the replicas A and B holds it.
    let service = run_forge(
        &build,
        &pdata,
        &pwork,
        &[
            "serve",
            "--listen",
            &p,
            "--machine",
            "p",
            "--acks",
            "1",
            "--peers",
            &peers_of(&a, &b),
            "--fleet",
            "127.0.0.1",
        ],
    );
    ready(pp);
    share_fleet_key(&pdata, &adata);
    share_fleet_key(&pdata, &bdata);
    let ra = replica(&adata, &awork, "a", &a, peers_of(&p, &b));
    let rb = replica(&bdata, &bwork, "b", &b, peers_of(&p, &a));
    ready(pa);
    ready(pb);
    http(pp, "POST", "/api/import", &serde_json::json!({"git": repo, "ref": "main"}).to_string());
    wait_until("the import", || call(pp, "log", serde_json::json!({}))["main"] == 2);
    assert_eq!(land_one(pp, "one.txt", "1\n"), 3);
    let held = position(pp)["size"].clone();
    wait_until("both replicas", || position(pa)["size"] == held && position(pb)["size"] == held);
    assert_eq!(position(pa)["chain"], position(pp)["chain"], "A's log is P's");
    assert_eq!(position(pa)["epoch"], 1, "P's first epoch");

    // A stops; P confirms a landing that only B holds, then P's machine is lost.
    stop(pa, ra);
    assert_eq!(land_one(pp, "two.txt", "2\n"), 4);
    let p_size = position(pp)["size"].clone();
    assert_eq!(position(pb)["size"], p_size, "B held the landing before it was answered");
    stop(pp, service);

    // A can't be promoted: B holds a landing A lacks.
    let a_log = std::fs::read(adata.join("events")).expect("A's log");
    let refused = run_forge(
        &build,
        &adata,
        &awork,
        &[
            "serve",
            "--promote",
            "--listen",
            &a,
            "--machine",
            "a",
            "--peers",
            &peers_of(&p, &b),
            "--fleet",
            "127.0.0.1",
        ],
    );
    assert_eq!(refused.join().expect("its thread"), 1, "the promotion is refused");
    assert_eq!(std::fs::read(adata.join("events")).expect("A's log"), a_log, "A is unchanged");

    // B is promoted, with A (a replica again) and B a majority of the three.
    let ra = replica(&adata, &awork, "a", &a, peers_of(&p, &b));
    ready(pa);
    stop(pb, rb);
    let promoted = run_forge(
        &build,
        &bdata,
        &bwork,
        &[
            "serve",
            "--promote",
            "--listen",
            &b,
            "--machine",
            "b",
            "--acks",
            "1",
            "--peers",
            &peers_of(&p, &a),
            "--fleet",
            "127.0.0.1",
        ],
    );
    ready(pb);
    assert_eq!(position(pb)["epoch"], 2);
    assert_eq!(call(pb, "read", serde_json::json!({"path": "two.txt"}))["text"], "2\n");
    // A follows B, told of the new epoch, and holds B's landings before they're answered.
    assert_eq!(land_one(pb, "three.txt", "3\n"), 5);
    let b_size = position(pb)["size"].clone();
    wait_until("A to follow B", || position(pa)["size"] == b_size);
    assert_eq!(position(pa)["epoch"], 2);
    assert_eq!(
        std::fs::read(adata.join("events")).expect("A's log"),
        std::fs::read(bdata.join("events")).expect("B's log"),
        "A's log is B's"
    );

    // P comes back: its peers tell it of epoch 2, and it's fenced before it takes a call.
    let back = run_forge(
        &build,
        &pdata,
        &pwork,
        &[
            "serve",
            "--listen",
            &p,
            "--machine",
            "p",
            "--peers",
            &peers_of(&a, &b),
            "--fleet",
            "127.0.0.1",
        ],
    );
    assert_eq!(back.join().expect("its thread"), 1, "the old service is fenced");
    let again = run_forge(&build, &pdata, &pwork, &["serve", "--listen", &p, "--machine", "p"]);
    assert_eq!(again.join().expect("its thread"), 1, "a fenced store doesn't serve again");
    stop(pb, promoted);
    stop(pa, ra);
}

/// What a test bucket holds, by key.
type Held = std::sync::Arc<std::sync::Mutex<std::collections::BTreeMap<String, Vec<u8>>>>;

/// A bucket as S3 serves one, for a backup's tests: PUT and GET of keys, each request refused
/// unless curl signed it (AWS signature version 4) with the test's key. Its port, and what it
/// holds.
fn bucket() -> (u16, Held) {
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
    let port = listener.local_addr().expect("its address").port();
    let held: Arc<Mutex<BTreeMap<String, Vec<u8>>>> = Arc::default();
    let keys = Arc::clone(&held);
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let keys = Arc::clone(&keys);
            std::thread::spawn(move || {
                let mut s = stream;
                let mut buf = Vec::new();
                let mut chunk = [0u8; 65536];
                let head_end = loop {
                    if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        break i + 4;
                    }
                    match s.read(&mut chunk) {
                        Ok(0) | Err(_) => return,
                        Ok(n) => buf.extend_from_slice(&chunk[..n]),
                    }
                };
                let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
                let mut lines = head.lines();
                let first: Vec<&str> = lines.next().unwrap_or("").split(' ').collect();
                let (method, path) = (first[0].to_string(), first[1].to_string());
                let header = |name: &str| {
                    head.lines()
                        .find_map(|l| {
                            let (k, v) = l.split_once(':')?;
                            k.eq_ignore_ascii_case(name).then(|| v.trim().to_string())
                        })
                        .unwrap_or_default()
                };
                let length: usize = header("content-length").parse().unwrap_or(0);
                while buf.len() < head_end + length {
                    match s.read(&mut chunk) {
                        Ok(0) | Err(_) => return,
                        Ok(n) => buf.extend_from_slice(&chunk[..n]),
                    }
                }
                let body = buf[head_end..head_end + length].to_vec();
                let signed = header("authorization")
                    .starts_with("AWS4-HMAC-SHA256 Credential=test/")
                    && !header("x-amz-date").is_empty();
                let key = path.trim_start_matches("/bucket/").to_string();
                let (status, out) = if !signed {
                    ("403 Forbidden", b"unsigned".to_vec())
                } else if method == "PUT" {
                    keys.lock().expect("the keys").insert(key, body);
                    ("200 OK", Vec::new())
                } else {
                    match keys.lock().expect("the keys").get(&key) {
                        Some(b) => ("200 OK", b.clone()),
                        None => ("404 Not Found", b"NoSuchKey".to_vec()),
                    }
                };
                let reply = format!(
                    "HTTP/1.1 {status}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                    out.len()
                );
                let _ = s.write_all(reply.as_bytes());
                let _ = s.write_all(&out);
            });
        }
    });
    (port, held)
}

#[test]
fn a_store_backed_up_to_a_bucket_is_restored_whole() {
    let scratch = Scratch::new("backup");
    let dir = |d: &str| {
        let p = scratch.0.join(d);
        std::fs::create_dir_all(&p).expect("a directory");
        p
    };
    let (repo, build, data, work, bwork) =
        (dir("repo"), dir("build"), dir("data"), dir("work"), dir("bwork"));
    let (restored, rwork, other) = (dir("restored"), dir("rwork"), dir("other"));
    repository(&repo);
    wrela_tests::must_build(&repo_root().join("forge"), &build);
    let auth = scratch.0.join("r2.conf");
    std::fs::write(&auth, "user = \"test:secret\"\n").expect("curl's credentials");
    let (bport, held) = bucket();
    let url = format!("http://127.0.0.1:{bport}/bucket");
    let backup = |data: &Path, extra: &[&str]| {
        let mut args = vec!["backup", "--to", &url, "--auth", auth.to_str().expect("a path")];
        args.extend_from_slice(extra);
        run_forge(&build, data, &bwork, &args).join().expect("the backup's thread")
    };

    let port = free_port();
    let service = start(&build, &data, &work, port);
    ready(port);
    http(port, "POST", "/api/import", &serde_json::json!({"git": repo, "ref": "main"}).to_string());
    wait_until("the import", || call(port, "log", serde_json::json!({}))["main"] == 2);
    land_one(port, "big.bin", &"x".repeat(3_000_000));
    // Backed up while the service runs, then again once it has landed more.
    assert_eq!(backup(&data, &["--once"]), 0);
    let first: Vec<String> = held.lock().expect("the keys").keys().cloned().collect();
    assert!(first.contains(&"head".to_string()), "{first:?}");
    assert!(
        first.iter().any(|k| k.starts_with("pack/"))
            && first.contains(&"log/0000000000000000".into())
    );
    land_one(port, "more.txt", "more\n");
    assert_eq!(backup(&data, &["--once"]), 0);
    let logs = held.lock().expect("the keys").keys().filter(|k| k.starts_with("log/")).count();
    assert_eq!(logs, 2, "a second segment, from where the first ended");
    let big = held.lock().expect("the keys").values().map(Vec::len).max().unwrap_or(0);
    assert!(big > 3_000_000, "the big file is in a pack");
    stop(port, service);

    // Another store's backup isn't this bucket's.
    assert_eq!(backup(&other, &["--once"]), 1, "a store whose log isn't the backup's is refused");

    // Restored into an empty store: the same log, and a service that serves it once promoted.
    let restore = |into: &Path| {
        let args = ["restore", "--from", &url, "--auth", auth.to_str().expect("a path")];
        run_forge(&build, into, &rwork, &args).join().expect("the restore's thread")
    };
    assert_eq!(restore(&restored), 0);
    assert_eq!(
        std::fs::read(restored.join("events")).expect("the restored log"),
        std::fs::read(data.join("events")).expect("the log"),
        "the restored log is the store's"
    );
    assert_eq!(restore(&restored), 1, "a restore into a store that isn't empty is refused");
    // A pack that lacks a tree inside another (tools/) is caught: every object the log names,
    // however deep, is checked to be there.
    {
        let mut keys = held.lock().expect("the keys");
        let pack = keys.get("pack/0000000000000000").expect("the first pack").clone();
        let (mut out, mut at, mut dropped) = (Vec::new(), 0, false);
        while at + 36 <= pack.len() {
            let n = u32::from_le_bytes(pack[at + 32..at + 36].try_into().unwrap()) as usize;
            let raw = &pack[at + 36..at + 36 + n];
            let tools = raw[0] == 2 && raw.windows(8).any(|w| w == b"check.sh");
            if tools && !dropped {
                dropped = true;
            } else {
                out.extend_from_slice(&pack[at..at + 36 + n]);
            }
            at += 36 + n;
        }
        assert!(dropped, "the pack held tools/'s tree");
        keys.insert("pack/0000000000000000".into(), out);
    }
    let lacking = dir("lacking");
    assert_eq!(restore(&lacking), 1, "a backup that lacks an object isn't restored");
    let port2 = free_port();
    let listen2 = format!("127.0.0.1:{port2}");
    let promoted = run_forge(
        &build,
        &restored,
        &rwork,
        &["serve", "--promote", "--listen", &listen2, "--machine", "promoted"],
    );
    ready(port2);
    assert_eq!(call(port2, "read", serde_json::json!({"path": "more.txt"}))["text"], "more\n");
    let read = call(port2, "read", serde_json::json!({"path": "big.bin", "lines": 1}));
    assert_eq!(read["size"], 3_000_000);
    stop(port2, promoted);
}

fn watched(port: u16, query: &str) -> serde_json::Value {
    let (status, text) = http(port, "GET", &format!("/api/watch?{query}"), "");
    assert_eq!(status, 200, "{text}");
    serde_json::from_str(&text).expect("a watch's answer")
}

#[test]
fn a_rooms_watchers_see_what_happens_in_it_as_it_happens() {
    let scratch = Scratch::new("rooms");
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
    http(port, "POST", "/api/import", &serde_json::json!({"git": repo, "ref": "main"}).to_string());
    wait_until("the import", || call(port, "log", serde_json::json!({}))["main"] == 2);
    let c1 = call(port, "open", serde_json::json!({"intent": "Watched"}))["change"]
        .as_str()
        .expect("a change")
        .to_string();
    call(port, "write", serde_json::json!({"change": c1, "path": "a.txt", "content": "uno\n"}));

    // From now: a watch of c1 is held until c1 writes, and its answer names who watches.
    let now = position(port)["size"].as_u64().expect("the log's end");
    let room = c1.clone();
    let held = std::thread::spawn(move || watched(port, &format!("room={room}&from={now}")));
    std::thread::sleep(Duration::from_millis(300));
    call(port, "write", serde_json::json!({"change": c1, "path": "new.txt", "content": "n\n"}));
    let got = held.join().expect("the watch");
    assert_eq!(got["notices"].as_array().expect("notices").len(), 1, "{got}");
    assert_eq!(got["notices"][0]["kind"], "wrote");
    assert_eq!(got["notices"][0]["path"], "new.txt");
    assert_eq!(got["present"], serde_json::json!(["local"]), "the watcher is in the room");
    // From the start, every notice of c1 in order; from its `next`, none twice.
    let all = watched(port, &format!("room={c1}&from=0"));
    let kinds: Vec<&str> = all["notices"]
        .as_array()
        .expect("notices")
        .iter()
        .map(|n| n["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["opened", "wrote", "wrote"]);
    // Main moves under c1: c1's room hears of the carry, and main's of the landing.
    let next = all["next"].as_u64().expect("next");
    let c1_room = c1.clone();
    let carried = std::thread::spawn(move || watched(port, &format!("room={c1_room}&from={next}")));
    let main_room = std::thread::spawn(move || watched(port, &format!("room=main&from={next}")));
    std::thread::sleep(Duration::from_millis(300));
    land_one(port, "a.txt", "one\ntwo\nthree\nfour\n");
    let main_said = main_room.join().expect("main's watch");
    assert!(
        main_said["notices"].as_array().expect("notices").iter().any(|n| n["kind"] == "landed"),
        "{main_said}"
    );
    // The landing changed a.txt, which c1 changed too: c1's room hears main moved under it.
    let c1_said = carried.join().expect("c1's watch");
    let kind = c1_said["notices"][0]["kind"].as_str().unwrap_or("").to_string();
    assert!(kind == "carried" || kind == "conflict", "{c1_said}");
    assert_eq!(c1_said["notices"][0]["path"], "a.txt");
    // Bad asks.
    assert_eq!(http(port, "GET", "/api/watch?room=x9", "").0, 400);
    assert_eq!(http(port, "GET", "/api/watch?room=main&from=1", "").0, 400, "not a record's end");
    // The studio: who is where, and the page holds until something happens.
    let (status, page) = http(port, "GET", "/studio", "");
    assert_eq!(status, 200);
    assert!(page.contains("In the rooms") && page.contains("http-equiv=\"refresh\""), "{page}");
    let end = position(port)["size"].as_u64().expect("the log's end");
    let studio = std::thread::spawn(move || http(port, "GET", &format!("/studio?after={end}"), ""));
    std::thread::sleep(Duration::from_millis(300));
    assert!(!studio.is_finished(), "the page waits for something to happen");
    call(port, "write", serde_json::json!({"change": c1, "path": "seen.txt", "content": "s\n"}));
    let (status, page) = studio.join().expect("the studio");
    assert_eq!(status, 200);
    assert!(page.contains("seen.txt"), "the page shows the write");
    // forge's command watches a room: it prints the next notices and, once, stops.
    let listen = format!("127.0.0.1:{port}");
    let end = position(port)["size"].as_u64().expect("the log's end");
    let (room, from) = (format!("room={c1}"), format!("from={end}"));
    let watcher = run_forge(
        &build,
        &scratch.0,
        &work,
        &["watch", &room, &from, "once=true", "--server", &listen],
    );
    std::thread::sleep(Duration::from_millis(300));
    call(port, "write", serde_json::json!({"change": c1, "path": "cli.txt", "content": "c\n"}));
    assert_eq!(watcher.join().expect("the command's thread"), 0, "the command saw the write");
    stop(port, service);
}

#[test]
fn a_file_a_check_changed_is_written_again_before_the_next_check() {
    let scratch = Scratch::new("poison");
    let dir = |d: &str| {
        let p = scratch.0.join(d);
        std::fs::create_dir_all(&p).expect("a directory");
        p
    };
    let (repo, build, data, work) = (dir("repo"), dir("build"), dir("data"), dir("work"));
    repository(&repo);
    // A check that spoils README, which every check reads.
    let check = repo.join("tools/check.sh");
    std::fs::write(
        &check,
        "#!/bin/sh\ntest \"$(cat README)\" = \"forge test\" || exit 1\nif [ \"$1\" = spoil ]; then echo spoiled > README; fi\n",
    )
    .expect("check.sh");
    git(&repo, &["commit", "-qam", "A check that spoils README"]);
    wrela_tests::must_build(&repo_root().join("forge"), &build);
    let port = free_port();
    let service = start(&build, &data, &work, port);
    ready(port);
    http(port, "POST", "/api/import", &serde_json::json!({"git": repo, "ref": "main"}).to_string());
    wait_until("the import", || call(port, "log", serde_json::json!({}))["main"] == 3);
    let change = call(port, "open", serde_json::json!({"intent": "Checked twice"}))["change"]
        .as_str()
        .expect("a change")
        .to_string();
    call(port, "write", serde_json::json!({"change": change, "path": "x.txt", "content": "x\n"}));
    let check = |name: &str| {
        let job = call(port, "check", serde_json::json!({"change": change, "check": name}))["job"]
            .as_str()
            .expect("a job")
            .to_string();
        wait_until("the check", || {
            let st = job_state(port, &job)["state"].clone();
            st == "passed" || st == "failed"
        });
        job_state(port, &job)["state"].clone()
    };
    assert_eq!(check("spoil"), "passed", "it spoils README after it reads it");
    assert_eq!(std::fs::read_to_string(work.join("README")).expect("README"), "spoiled\n");
    // The next check of the same tree sees README as the tree holds it, not as the last check left it.
    assert_eq!(check("rustfmt"), "passed");
    assert_eq!(std::fs::read_to_string(work.join("README")).expect("README"), "forge test\n");
    stop(port, service);
}

#[test]
fn a_program_on_the_machine_is_the_machine_only_with_the_stores_key() {
    let scratch = Scratch::new("key");
    let dir = |d: &str| {
        let p = scratch.0.join(d);
        std::fs::create_dir_all(&p).expect("a directory");
        p
    };
    let (build, data, work) = (dir("build"), dir("data"), dir("work"));
    wrela_tests::must_build(&repo_root().join("forge"), &build);
    let port = free_port();
    let service = start(&build, &data, &work, port);
    ready(port);
    let key = std::fs::read_to_string(data.join("key")).expect("the store's key");
    assert_eq!(key.len(), 64);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(data.join("key")).expect("the key").permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "the key is its owner's alone");
    }
    // Without the key: a writer, not the machine.
    let without = |method: &str, path: &str, body: &str| {
        let mut s = TcpStream::connect(("127.0.0.1", port)).expect("forge accepts");
        let req = format!(
            "{method} {path} HTTP/1.1\r\nhost: x\r\nconnection: close\r\ncontent-length: {}\r\n\r\n{body}",
            body.len()
        );
        s.write_all(req.as_bytes()).expect("sent");
        let mut out = String::new();
        s.read_to_string(&mut out).expect("an answer");
        out
    };
    let opened = without("POST", "/api/call", r#"{"tool":"open","args":{"intent":"Mine"}}"#);
    assert!(opened.starts_with("HTTP/1.1 200"), "a writer opens a change: {opened}");
    let changes = call(port, "changes", serde_json::json!({}));
    assert_eq!(changes["changes"][0]["author"], "local@test", "named for the machine");
    assert!(without("POST", "/api/stop", "").starts_with("HTTP/1.1 403"), "it can't stop forge");
    let import = without("POST", "/api/import", r#"{"git":"/nowhere"}"#);
    assert!(import.starts_with("HTTP/1.1 403"), "nor import: {import}");
    // With it, the machine itself.
    stop(port, service);
    let again = start(&build, &data, &work, port);
    ready(port);
    assert_eq!(std::fs::read_to_string(data.join("key")).expect("the key"), key, "kept");
    stop(port, again);
}

#[test]
fn answers_larger_than_one_host_call_sends_go_over_several() {
    let scratch = Scratch::new("budget");
    let dir = |d: &str| {
        let p = scratch.0.join(d);
        std::fs::create_dir_all(&p).expect("a directory");
        p
    };
    let (build, data, work) = (dir("build"), dir("data"), dir("work"));
    wrela_tests::must_build(&repo_root().join("forge"), &build);
    let port = free_port();
    let service = start(&build, &data, &work, port);
    ready(port);
    // A 24 MiB blob, then three asks for it pipelined on one connection: 72 MiB of answers,
    // more than one host call may send.
    let big: Vec<u8> = (0..24usize << 20).map(|i| (i % 251) as u8).collect();
    let mut c = TcpStream::connect(("127.0.0.1", port)).expect("forge accepts");
    c.set_read_timeout(Some(Duration::from_secs(60))).expect("a timeout");
    let put =
        format!("POST /api/object HTTP/1.1\r\nhost: x\r\ncontent-length: {}\r\n\r\n", big.len());
    c.write_all(put.as_bytes()).expect("sent");
    c.write_all(&big).expect("sent");
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        c.read_exact(&mut byte).expect("an answer");
        head.push(byte[0]);
    }
    let mut id_hex = [0u8; 64];
    c.read_exact(&mut id_hex).expect("its id");
    let id: Vec<u8> = (0..32)
        .map(|i| {
            u8::from_str_radix(std::str::from_utf8(&id_hex[2 * i..2 * i + 2]).unwrap(), 16).unwrap()
        })
        .collect();
    for _ in 0..3 {
        let ask = b"POST /api/objects HTTP/1.1\r\nhost: x\r\ncontent-length: 32\r\n\r\n";
        c.write_all(ask).expect("sent");
        c.write_all(&id).expect("sent");
    }
    for n in 0..3 {
        let mut head = Vec::new();
        while !head.ends_with(b"\r\n\r\n") {
            c.read_exact(&mut byte).expect("an answer");
            head.push(byte[0]);
        }
        let text = String::from_utf8_lossy(&head).to_lowercase();
        assert!(text.starts_with("http/1.1 200"), "answer {n}: {text}");
        let len: usize = text
            .lines()
            .find_map(|l| l.strip_prefix("content-length: "))
            .and_then(|v| v.trim().parse().ok())
            .expect("a length");
        let mut body = vec![0u8; len];
        c.read_exact(&mut body).expect("its body");
        assert_eq!(&body[..32], &id[..], "answer {n} names the blob");
        assert_eq!(&body[37..], &big[..], "answer {n} holds it whole");
    }
    assert_eq!(http(port, "GET", "/health", "").0, 200, "the service still serves");
    stop(port, service);
}

/// forge with `args` and the roots `roots`, on a thread of its own: its exit status.
fn run_with_roots(
    build: &Path,
    roots: &[(&str, &Path)],
    args: &[&str],
) -> std::thread::JoinHandle<i32> {
    let mut r = wrela_service::files::Roots::default();
    for (name, dir) in roots {
        r.add(name, dir).expect("a root");
    }
    let build = ServerBuild::load(build).expect("forge's server build");
    let args = args.iter().map(|s| s.to_string()).collect();
    std::thread::spawn(move || {
        wrela_service::run(wrela_service::Config {
            build,
            roots: r,
            args,
            limits: wrela_service::limits(),
        })
    })
}

#[test]
fn a_workspace_pulls_a_change_and_pushes_back_what_changed_in_it() {
    let scratch = Scratch::new("workspace");
    let dirs: Vec<PathBuf> =
        ["repo", "build", "data", "work", "desk"].iter().map(|d| scratch.0.join(d)).collect();
    for d in &dirs {
        std::fs::create_dir_all(d).expect("a directory");
    }
    let (repo, build, data, work, desk) = (&dirs[0], &dirs[1], &dirs[2], &dirs[3], &dirs[4]);
    repository(repo);
    wrela_tests::must_build(&repo_root().join("forge"), build);
    let port = free_port();
    let listen = format!("127.0.0.1:{port}");
    let service = start(build, data, work, port);
    ready(port);
    http(port, "POST", "/api/import", &serde_json::json!({"git": repo, "ref": "main"}).to_string());
    wait_until("the import", || call(port, "log", serde_json::json!({}))["main"] == 2);
    let change = call(port, "open", serde_json::json!({"intent": "Edited on disk"}))["change"]
        .as_str()
        .expect("a change")
        .to_string();
    let pull = |what: &str| {
        let arg = format!("change={change}");
        let args: Vec<&str> = if what == "pull" {
            vec!["pull", &arg, "--server", &listen]
        } else {
            vec!["push", "--server", &listen]
        };
        run_with_roots(build, &[("here", desk.as_path())], &args)
            .join()
            .expect("the command's thread")
    };
    assert_eq!(pull("pull"), 0, "the pull");
    assert_eq!(std::fs::read_to_string(desk.join("a.txt")).expect("a.txt"), "one\ntwo\nthree\n");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode =
            std::fs::metadata(desk.join("tools/check.sh")).expect("check.sh").permissions().mode();
        assert_ne!(mode & 0o111, 0, "check.sh is executable");
    }
    // Edits on disk: one changed, one new and binary, one removed, and build output to ignore.
    std::fs::write(desk.join("a.txt"), "one\nTWO\nthree\n").expect("an edit");
    std::fs::write(desk.join("new.bin"), (0..=255u8).collect::<Vec<u8>>()).expect("a binary file");
    std::fs::remove_file(desk.join("b.txt")).expect("a removal");
    std::fs::write(desk.join(".forgeignore"), "**/build/\n").expect("the ignore rules");
    std::fs::create_dir_all(desk.join("pkg/build")).expect("build output");
    std::fs::write(desk.join("pkg/build/out.wasm"), "not source").expect("build output");
    assert_eq!(pull("push"), 0, "the push");
    let status = call(port, "status", serde_json::json!({"change": change}));
    let files: Vec<(String, String)> = status["files"]
        .as_array()
        .expect("files")
        .iter()
        .map(|f| {
            (
                f["path"].as_str().unwrap_or("").to_string(),
                f["state"].as_str().unwrap_or("").to_string(),
            )
        })
        .collect();
    let want: Vec<(String, String)> = [
        (".forgeignore", "added"),
        ("a.txt", "changed"),
        ("b.txt", "removed"),
        ("new.bin", "added"),
    ]
    .iter()
    .map(|(a, b)| (a.to_string(), b.to_string()))
    .collect();
    assert_eq!(files, want, "{status}");
    let read = call(port, "read", serde_json::json!({"change": change, "path": "a.txt"}));
    assert_eq!(read["text"], "one\nTWO\nthree\n");
    let bin =
        call(port, "read", serde_json::json!({"change": change, "path": "new.bin", "lines": 1}));
    assert_eq!(bin["size"], 256);
    // Nothing changed since: a second push sends nothing.
    assert_eq!(pull("push"), 0, "a second push");
    assert_eq!(http(port, "POST", "/api/stop", "").0, 200);
    assert_eq!(service.join().expect("the service's thread"), 0);
}
