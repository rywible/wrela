//! Chrome stable, driven over the DevTools protocol (CDP) from Rust: for the tool host's
//! browser service (`std::tool::browser`, language.md §15.5) and the compiler's own browser
//! tests. Chrome runs headless (`--headless=new`) on the machine's hardware GPU, with a
//! throwaway profile, in a process group of its own; a page's console, the requests it makes and
//! what it puts in `results/DONE` reach the driver as DevTools events.
//!
//! **Every end is bounded and clean.** [`Browser::close`] (and its drop, so a panic too) closes
//! the connection, kills the process group (`SIGKILL`: the profile is thrown away, so nothing is
//! lost), waits at most two seconds for the group to be gone, and removes the profile. Nothing it started outlives it unless its
//! own process is killed first; a supervisor that must cover that records the group, which the
//! [`Spawn`] hook is given the chance to do before Chrome runs.
//!
//! **A browser may be held to a policy** ([`Policy`]): every request of its pages and workers,
//! service and shared workers too, is paused by DevTools' Fetch domain (each worker is paused at
//! its start until its requests are), and one the policy refuses fails as blocked. A browser
//! with a policy also routes every host name it doesn't grant to a dead proxy, so a WebSocket,
//! which the Fetch domain doesn't see, reaches no other host either.
//!
//! **No JavaScript crosses this API.** Scripts and observations are typed ([`script`]); the
//! driver's one function in the page reads what an observation names.
//!
//! Chrome is `WRELA_CHROME` if set, else the macOS app or the first of Linux's names on `PATH`
//! ([`chrome_binary`]); `WRELA_CHROME_ARGS` adds arguments, split as a shell splits them. A
//! Chrome that isn't there, or that offers only a software WebGPU adapter, fails loudly
//! ([`Tab::hardware_adapter`]): the driver never renders in software.

pub mod cdp;
mod keys;
pub mod script;

use cdp::{Connection, Event};
use serde_json::{Value, json};
use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::time::{Duration, Instant};

/// How long Chrome may take to publish its debugging port.
pub const STARTUP: Duration = Duration::from_secs(15);

/// How long a killed group has to be seen gone.
const REAP: Duration = Duration::from_secs(2);

/// How long a DevTools call made while handling an event may take.
const EVENT_CALL: Duration = Duration::from_secs(10);

const LINUX_CHROMES: [&str; 4] =
    ["google-chrome-stable", "google-chrome", "chromium", "chromium-browser"];

/// Chrome's executable: `WRELA_CHROME`, else the macOS app, or the first of Linux's names on
/// `PATH`.
pub fn chrome_binary() -> PathBuf {
    chrome_binary_in(
        std::env::var_os("WRELA_CHROME"),
        std::env::consts::OS,
        std::env::var_os("PATH"),
    )
}

fn chrome_binary_in(set: Option<OsString>, os: &str, path: Option<OsString>) -> PathBuf {
    if let Some(chrome) = set.filter(|c| !c.is_empty()) {
        return PathBuf::from(chrome);
    }
    if os == "macos" {
        return PathBuf::from("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome");
    }
    let dirs: Vec<PathBuf> = path.map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    LINUX_CHROMES
        .iter()
        .find_map(|name| dirs.iter().map(|d| d.join(name)).find(|p| p.is_file()))
        .unwrap_or_else(|| PathBuf::from(LINUX_CHROMES[0]))
}

/// The arguments a platform needs for its hardware WebGPU adapter. Linux Chrome keeps WebGPU
/// behind flags, and without Vulkan through ANGLE it offers only its software adapter. It also
/// reads cookies only after the system keyring answers, which can take 25 s; a test profile is
/// thrown away and needs no keyring.
pub fn platform_args(os: &str) -> &'static [&'static str] {
    if os == "linux" {
        &[
            "--enable-unsafe-webgpu",
            "--enable-features=Vulkan",
            "--use-angle=vulkan",
            "--ignore-gpu-blocklist",
            "--password-store=basic",
        ]
    } else {
        &[]
    }
}

/// Whether a page's request for `url` may go: a browser's [`Policy`].
pub type Policy = Arc<dyn Fn(&str) -> bool + Send + Sync>;

/// Starts Chrome's executable with its arguments, its output going to the log: the child, and
/// the process group it leads. The tool host's records the group for its supervisor first.
pub type Spawn<'a> = &'a dyn Fn(&Path, &[OsString], std::fs::File) -> Result<(Child, i32), String>;

/// [`Spawn`] for a process of no supervisor's: Chrome leads a new process group.
pub fn spawn_in_own_group(
    exe: &Path,
    args: &[OsString],
    log: std::fs::File,
) -> Result<(Child, i32), String> {
    let err = log.try_clone().map_err(|e| format!("Chrome's log: {e}"))?;
    let mut c = std::process::Command::new(exe);
    c.args(args).stdin(std::process::Stdio::null()).stdout(log).stderr(err);
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut c, 0);
    let child = c.spawn().map_err(|e| format!("can't run Chrome ({}): {e}", exe.display()))?;
    let group = child.id() as i32;
    Ok((child, group))
}

/// How to start a browser.
pub struct Launch {
    pub binary: PathBuf,
    /// The profile's directory, which the launch makes (it mustn't be there) and the close
    /// removes.
    pub profile: PathBuf,
    /// The window's size, for each page.
    pub window: (u32, u32),
    /// More arguments: `WRELA_CHROME_ARGS`'s.
    pub args: Vec<String>,
    pub startup: Duration,
    /// What its pages may reach, if it's held to a policy; and, for the dead proxy, the host
    /// names it grants (`127.0.0.1` always).
    pub policy: Option<(Policy, Vec<String>)>,
    /// Set to give up the start: the browser is ended at once.
    pub cancel: Option<Arc<AtomicBool>>,
}

impl Launch {
    /// A launch of [`chrome_binary`] with `WRELA_CHROME_ARGS`, a profile at `profile` and a
    /// 1920×1080 window: an error if there's no Chrome there to run, or the arguments don't
    /// split.
    pub fn new(profile: PathBuf) -> Result<Launch, String> {
        let binary = chrome_binary();
        if !runnable(&binary) {
            return Err(format!(
                "Chrome isn't at {} (set WRELA_CHROME): the browser tests need Chrome stable",
                binary.display()
            ));
        }
        let extra = std::env::var("WRELA_CHROME_ARGS").unwrap_or_default();
        let args = shlex::split(&extra)
            .ok_or_else(|| format!("WRELA_CHROME_ARGS doesn't split as a shell splits: {extra}"))?;
        Ok(Launch {
            binary,
            profile,
            window: (1920, 1080),
            args,
            startup: STARTUP,
            policy: None,
            cancel: None,
        })
    }

    /// The command line: Chrome's flags, then the platform's and the launch's.
    fn argv(&self) -> Vec<OsString> {
        let mut a: Vec<OsString> = [
            "--headless=new",
            "--remote-debugging-port=0",
            "--no-first-run",
            "--no-default-browser-check",
            "--disable-extensions",
            "--disable-background-timer-throttling",
            "--disable-renderer-backgrounding",
            "--disable-backgrounding-occluded-windows",
            // A test page plays its voice without a click (test mode's `live`).
            "--autoplay-policy=no-user-gesture-required",
            // Timestamp queries at full resolution, not quantized to 100 µs.
            "--enable-webgpu-developer-features",
        ]
        .into_iter()
        .map(OsString::from)
        .collect();
        a.push(format!("--user-data-dir={}", self.profile.display()).into());
        a.push(format!("--window-size={},{}", self.window.0, self.window.1).into());
        if let Some((_, hosts)) = &self.policy {
            // What the Fetch domain doesn't see (a WebSocket) finds no other host either.
            let mut bypass = vec!["127.0.0.1".to_string()];
            bypass.extend(hosts.iter().cloned());
            a.push("--proxy-server=127.0.0.1:9".into());
            a.push(format!("--proxy-bypass-list={}", bypass.join(";")).into());
        }
        a.extend(platform_args(std::env::consts::OS).iter().map(OsString::from));
        a.extend(self.args.iter().map(OsString::from));
        a.push("about:blank".into());
        a
    }
}

/// Whether the file at `path` is one that runs: a regular file with an execute bit.
fn runnable(path: &Path) -> bool {
    #[cfg(unix)]
    let runs = |m: &std::fs::Metadata| {
        std::os::unix::fs::PermissionsExt::mode(&m.permissions()) & 0o111 != 0
    };
    #[cfg(not(unix))]
    let runs = |_: &std::fs::Metadata| true;
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && runs(&m))
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The process and profile of a browser: what [`Browser::close`] ends, also while it's still
/// starting.
struct Process {
    child: Mutex<Option<Child>>,
    group: i32,
    profile: PathBuf,
    ended: Mutex<Option<bool>>,
}

impl Process {
    /// Ends the group and removes the profile, once: whether the group is gone and the profile
    /// with it.
    fn end(&self) -> bool {
        let mut ended = lock(&self.ended);
        if let Some(clean) = *ended {
            return clean;
        }
        let mut child = lock(&self.child).take();
        let group = rustix::process::Pid::from_raw(self.group);
        let exited =
            |c: &mut Option<Child>| c.as_mut().is_none_or(|c| !matches!(c.try_wait(), Ok(None)));
        // The whole group, also what Chrome left when it exited first.
        if let Some(pid) = group {
            let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
        }
        if let Some(c) = child.as_mut() {
            let _ = c.kill();
        }
        let since = Instant::now();
        let mut gone = false;
        while since.elapsed() < REAP {
            let reaped = exited(&mut child);
            // macOS refuses (EPERM) to signal a group whose processes have all exited but not
            // been reaped: that group is gone too.
            let empty = group.is_none_or(|pid| {
                rustix::process::test_kill_process_group(pid)
                    .is_err_and(|e| e == rustix::io::Errno::SRCH || e == rustix::io::Errno::PERM)
            });
            if reaped && empty {
                gone = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let mut removed = false;
        for _ in 0..50 {
            match std::fs::remove_dir_all(&self.profile) {
                Ok(()) => {
                    removed = true;
                    break;
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    removed = true;
                    break;
                }
                // A process of the group writing as it died: once more.
                Err(_) => std::thread::sleep(Duration::from_millis(20)),
            }
        }
        *ended = Some(gone && removed);
        gone && removed
    }
}

/// A running Chrome.
pub struct Browser {
    inner: Arc<Inner>,
}

struct Inner {
    conn: Connection,
    process: Process,
    policy: Option<Policy>,
    window: (u32, u32),
    /// The thread that handles the browser's own events: its service and shared workers'.
    events: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl Browser {
    /// Starts Chrome as `launch` says, through `spawn`, and connects to it. A browser that
    /// doesn't start is ended, and its profile removed, before this returns.
    pub fn launch(launch: Launch, spawn: Spawn) -> Result<Browser, String> {
        std::fs::create_dir(&launch.profile)
            .map_err(|e| format!("Chrome's profile ({}): {e}", launch.profile.display()))?;
        let log_path = launch.profile.join("chrome.log");
        let log = match std::fs::File::create(&log_path) {
            Ok(f) => f,
            Err(e) => {
                let _ = std::fs::remove_dir_all(&launch.profile);
                return Err(format!("Chrome's log: {e}"));
            }
        };
        let (child, group) = match spawn(&launch.binary, &launch.argv(), log) {
            Ok(c) => c,
            Err(why) => {
                let _ = std::fs::remove_dir_all(&launch.profile);
                return Err(why);
            }
        };
        let process = Process {
            child: Mutex::new(Some(child)),
            group,
            profile: launch.profile.clone(),
            ended: Mutex::new(None),
        };
        let started = (|| {
            let deadline = Instant::now() + launch.startup;
            let (port, path) = loop {
                // Chrome may not have finished writing the file: until the deadline.
                let text = std::fs::read_to_string(launch.profile.join("DevToolsActivePort"));
                let mut lines = text.as_deref().unwrap_or("").lines();
                if let (Some(Ok(port)), Some(path)) =
                    (lines.next().map(str::parse::<u16>), lines.next())
                    && port > 0
                {
                    break (port, path.to_string());
                }
                if let Some(c) = lock(&process.child).as_mut()
                    && let Ok(Some(status)) = c.try_wait()
                {
                    return Err(format!("it exited with {status}"));
                }
                if Instant::now() >= deadline {
                    return Err(format!("no debugging port after {:?}", launch.startup));
                }
                if launch.cancel.as_ref().is_some_and(|c| c.load(Ordering::Acquire)) {
                    return Err("its start was given up".into());
                }
                std::thread::sleep(Duration::from_millis(20));
            };
            // On loopback a handshake is quick: one that isn't has no answer coming.
            Connection::open(port, &path, deadline.min(Instant::now() + Duration::from_secs(5)))
        })();
        let conn = match started {
            Ok(c) => c,
            Err(why) => {
                let log = std::fs::read_to_string(&log_path).unwrap_or_default();
                let tail: Vec<&str> = log.lines().rev().take(12).collect();
                process.end();
                let tail: Vec<&str> = tail.into_iter().rev().collect();
                return Err(format!(
                    "Chrome ({}) didn't start: {why}\n{}",
                    launch.binary.display(),
                    tail.join("\n")
                ));
            }
        };
        let inner = Arc::new(Inner {
            conn,
            process,
            policy: launch.policy.map(|(p, _)| p),
            window: launch.window,
            events: Mutex::new(None),
        });
        let browser = Browser { inner };
        if browser.inner.policy.is_some() {
            browser.hold_workers()?;
        }
        Ok(browser)
    }

    /// Pauses each service and shared worker at its start until its requests are held to the
    /// policy too: the browser's own auto-attach, its events handled on a thread of its own.
    fn hold_workers(&self) -> Result<(), String> {
        let (to, events) = mpsc::channel();
        let _ = self.inner.conn.route("", to);
        let inner = Arc::downgrade(&self.inner);
        let thread = std::thread::Builder::new()
            .name("wrela-chrome-workers".into())
            .spawn(move || {
                while let Ok(e) = events.recv() {
                    let Some(inner) = inner.upgrade() else { break };
                    handle(&inner, None, &e);
                }
            })
            .map_err(|e| format!("can't start the browser's events thread: {e}"))?;
        *lock(&self.inner.events) = Some(thread);
        let filter = json!([{"type": "service_worker"}, {"type": "shared_worker"}]);
        self.inner.conn.call(
            None,
            "Target.setAutoAttach",
            json!({"autoAttach": true, "waitForDebuggerOnStart": true, "flatten": true, "filter": filter}),
            Instant::now() + EVENT_CALL,
        )?;
        Ok(())
    }

    /// Whether Chrome still runs and its connection is open.
    pub fn alive(&self) -> bool {
        if self.inner.conn.ended().is_some() {
            return false;
        }
        lock(&self.inner.process.child).as_mut().is_some_and(|c| matches!(c.try_wait(), Ok(None)))
    }

    /// The process group Chrome leads.
    pub fn group(&self) -> i32 {
        self.inner.process.group
    }

    /// Opens `url` in a new window of its own, by `deadline`: its tab. With a size, its viewport
    /// is that size at a device scale of 1; with a console file, each console line of its page
    /// and workers is appended there as it comes.
    pub fn open(&self, url: &str, options: TabOptions, deadline: Instant) -> Result<Tab, String> {
        let conn = &self.inner.conn;
        let made = conn.call(
            None,
            "Target.createTarget",
            json!({"url": "about:blank", "newWindow": true}),
            deadline,
        )?;
        let target = made["targetId"].as_str().ok_or("Target.createTarget gave no target")?;
        let state = Arc::new(TabState::new(options.console.as_deref())?);
        let mut tab = Tab {
            inner: self.inner.clone(),
            target: target.to_string(),
            session: String::new(),
            state: state.clone(),
            thread: None,
            tag: 0,
            closed: AtomicBool::new(false),
        };
        let attached = conn.call(
            None,
            "Target.attachToTarget",
            json!({"targetId": target, "flatten": true}),
            deadline,
        )?;
        let session =
            attached["sessionId"].as_str().ok_or("Target.attachToTarget gave no session")?;
        tab.session = session.to_string();
        let (to, events) = mpsc::channel();
        tab.tag = conn.route(session, to);
        let (inner, st) = (Arc::downgrade(&self.inner), state.clone());
        let thread = std::thread::Builder::new()
            .name("wrela-chrome-tab".into())
            .spawn(move || {
                while let Ok(e) = events.recv() {
                    let Some(inner) = inner.upgrade() else { break };
                    handle(&inner, Some(&st), &e);
                }
            })
            .map_err(|e| format!("can't start the tab's events thread: {e}"))?;
        tab.thread = Some(thread);
        let s = Some(session);
        conn.call(s, "Runtime.enable", json!({}), deadline)?;
        conn.call(s, "Log.enable", json!({}), deadline)?;
        conn.call(s, "Inspector.enable", json!({}), deadline)?;
        if self.inner.policy.is_some() {
            conn.call(s, "Fetch.enable", fetch_patterns(), deadline)?;
        }
        conn.call(
            s,
            "Target.setAutoAttach",
            json!({"autoAttach": true, "waitForDebuggerOnStart": true, "flatten": true}),
            deadline,
        )?;
        if let Some((w, h)) = options.size {
            tab.resize(w, h, deadline)?;
        }
        let went = conn.call(s, "Page.navigate", json!({"url": url}), deadline)?;
        if let Some(why) = went.get("errorText").and_then(Value::as_str) {
            return Err(format!("Chrome couldn't open {url}: {why}"));
        }
        Ok(tab)
    }

    /// Closes the DevTools connection alone: every call waiting on it fails at once, and none can
    /// be made. The process stays until [`Browser::close`].
    pub fn disconnect(&self) {
        self.inner.conn.close(Duration::from_secs(1));
    }

    /// Ends Chrome: its connection, then its process group, then its profile. Whether it ended
    /// cleanly (the group gone and the profile removed) within its bounded grace.
    pub fn close(&self) -> bool {
        let closed = self.inner.conn.close(Duration::from_secs(1));
        let ended = self.inner.process.end();
        let events = lock(&self.inner.events).take();
        if let Some(t) = events {
            // The connection's end ends its channel, and so the thread.
            let since = Instant::now();
            while !t.is_finished() && since.elapsed() < Duration::from_secs(1) {
                std::thread::sleep(Duration::from_millis(5));
            }
            if t.is_finished() {
                let _ = t.join();
            }
        }
        closed && ended
    }
}

impl Drop for Browser {
    fn drop(&mut self) {
        self.close();
    }
}

fn fetch_patterns() -> Value {
    json!({"patterns": [{"urlPattern": "*", "requestStage": "Request"}]})
}

/// How to open a tab.
#[derive(Clone, Debug, Default)]
pub struct TabOptions {
    pub size: Option<(u32, u32)>,
    pub console: Option<PathBuf>,
}

/// What a tab's events tell: its console, the program's printed lines, its result, the
/// requests its policy refused, whether it crashed.
struct TabState {
    seen: Mutex<Seen>,
    changed: Condvar,
    console: Option<Mutex<std::fs::File>>,
}

#[derive(Default)]
struct Seen {
    console: Vec<String>,
    printed: Vec<String>,
    done: Option<String>,
    refused: Vec<String>,
    crashed: bool,
    size: Option<(u32, u32)>,
}

/// The most console lines a tab keeps (the file gets them all).
const MOST_LINES: usize = 100_000;

impl TabState {
    fn new(console: Option<&Path>) -> Result<TabState, String> {
        let console = match console {
            None => None,
            Some(p) => Some(Mutex::new(
                std::fs::File::create(p).map_err(|e| format!("{}: {e}", p.display()))?,
            )),
        };
        Ok(TabState { seen: Mutex::new(Seen::default()), changed: Condvar::new(), console })
    }

    fn line(&self, text: String) {
        if let Some(f) = &self.console {
            let mut f = lock(f);
            let _ = f.write_all(text.as_bytes());
            let _ = f.write_all(b"\n");
            let _ = f.flush();
        }
        let mut seen = lock(&self.seen);
        if let Some(p) = text.strip_prefix("wrela: ")
            && seen.printed.len() < MOST_LINES
        {
            seen.printed.push(p.to_string());
        }
        if seen.console.len() < MOST_LINES {
            seen.console.push(text);
        }
        drop(seen);
        self.changed.notify_all();
    }

    fn change(&self, f: impl FnOnce(&mut Seen)) {
        f(&mut lock(&self.seen));
        self.changed.notify_all();
    }

    /// Waits at most `wait` for anything new.
    fn wait(&self, wait: Duration) {
        let seen = lock(&self.seen);
        let _ = self.changed.wait_timeout(seen, wait);
    }
}

/// A console message's text: its arguments, each as the console shows it, between spaces.
fn console_text(params: &Value) -> String {
    let args = params["args"].as_array().cloned().unwrap_or_default();
    let parts: Vec<String> = args
        .iter()
        .map(|a| match &a["value"] {
            Value::String(s) => s.clone(),
            Value::Null => {
                a["description"].as_str().or(a["type"].as_str()).unwrap_or("").to_string()
            }
            v => v.to_string(),
        })
        .collect();
    parts.join(" ")
}

/// Handles event `e` of a tab's page or one of its workers (`state` is the tab's), or of the
/// browser's own workers.
fn handle(inner: &Inner, state: Option<&TabState>, e: &Event) {
    let conn = &inner.conn;
    let soon = || Instant::now() + EVENT_CALL;
    let s = Some(e.session.as_str());
    match e.method.as_str() {
        "Runtime.consoleAPICalled" => {
            if let Some(st) = state {
                st.line(console_text(&e.params));
            }
        }
        "Runtime.exceptionThrown" => {
            if let Some(st) = state {
                let d = &e.params["exceptionDetails"];
                let text = d["exception"]["description"].as_str().or(d["text"].as_str());
                st.line(format!("Uncaught {}", text.unwrap_or("an exception")));
            }
        }
        "Log.entryAdded" => {
            let entry = &e.params["entry"];
            // A worker's console is its own session's, which the page's log repeats.
            if let Some(st) = state
                && entry["source"] != "worker"
            {
                st.line(entry["text"].as_str().unwrap_or("").to_string());
            }
        }
        "Inspector.targetCrashed" => {
            if let Some(st) = state {
                st.change(|seen| seen.crashed = true);
            }
        }
        "Target.attachedToTarget" => {
            // A worker (or a frame of another process) the page or the browser started, paused
            // until its requests are held to the policy too.
            let Some(child) = e.params["sessionId"].as_str() else { return };
            let c = Some(child);
            if inner.policy.is_some() {
                let _ = conn.call(c, "Fetch.enable", fetch_patterns(), soon());
            }
            if state.is_some() {
                let _ = conn.call(c, "Runtime.enable", json!({}), soon());
                let _ = conn.call(c, "Log.enable", json!({}), soon());
                let _ = conn.call(
                    c,
                    "Target.setAutoAttach",
                    json!({"autoAttach": true, "waitForDebuggerOnStart": true, "flatten": true}),
                    soon(),
                );
            }
            let _ = conn.call(c, "Runtime.runIfWaitingForDebugger", json!({}), soon());
        }
        "Target.detachedFromTarget" => {
            if let Some(child) = e.params["sessionId"].as_str() {
                conn.unroute_session(child);
            }
        }
        "Fetch.requestPaused" => {
            let Some(id) = e.params["requestId"].as_str() else { return };
            let request = &e.params["request"];
            let url = request["url"].as_str().unwrap_or("");
            let allowed = inner.policy.as_ref().is_none_or(|p| p(url));
            if allowed {
                if let Some(st) = state
                    && request["method"] == "PUT"
                    && url.split(['?', '#']).next().is_some_and(|u| u.ends_with("/results/DONE"))
                {
                    let body = request["postData"].as_str().unwrap_or("").trim().to_string();
                    st.change(|seen| seen.done = Some(body));
                }
                let _ = conn.call(s, "Fetch.continueRequest", json!({"requestId": id}), soon());
            } else {
                let _ = conn.call(
                    s,
                    "Fetch.failRequest",
                    json!({"requestId": id, "errorReason": "BlockedByClient"}),
                    soon(),
                );
                if let Some(st) = state {
                    let method = request["method"].as_str().unwrap_or("GET");
                    st.change(|seen| seen.refused.push(format!("{method} {url}")));
                    st.line(format!("the host refused {method} {url}: it isn't a granted origin"));
                }
            }
        }
        _ => {}
    }
}

/// A page in a window of its own.
pub struct Tab {
    inner: Arc<Inner>,
    target: String,
    session: String,
    state: Arc<TabState>,
    thread: Option<std::thread::JoinHandle<()>>,
    /// The tag of its sessions' route.
    tag: u64,
    closed: AtomicBool,
}

impl Tab {
    /// Calls `method` with `params` in the tab's page, by `deadline`.
    pub fn call(&self, method: &str, params: Value, deadline: Instant) -> Result<Value, String> {
        self.inner.conn.call(Some(&self.session), method, params, deadline)
    }

    /// Calls `method` with `params` on the browser, by `deadline`.
    pub fn call_browser(
        &self,
        method: &str,
        params: Value,
        deadline: Instant,
    ) -> Result<Value, String> {
        self.inner.conn.call(None, method, params, deadline)
    }

    /// Evaluates the driver's own `expression` in the page, awaiting a promise: its value, or why
    /// there is none (the page threw, or has no context to evaluate in yet).
    pub(crate) fn evaluate(&self, expression: &str, deadline: Instant) -> Result<Value, String> {
        let r = self.call(
            "Runtime.evaluate",
            json!({"expression": expression, "returnByValue": true, "awaitPromise": true}),
            deadline,
        )?;
        if let Some(d) = r.get("exceptionDetails") {
            let why = d["exception"]["description"].as_str().or(d["text"].as_str());
            return Err(format!("the page threw: {}", why.unwrap_or("an exception")));
        }
        Ok(r["result"]["value"].clone())
    }

    /// Sets the viewport to `width` × `height` at a device scale of 1.
    pub fn resize(&self, width: u32, height: u32, deadline: Instant) -> Result<(), String> {
        self.call(
            "Emulation.setDeviceMetricsOverride",
            json!({"width": width, "height": height, "deviceScaleFactor": 1, "mobile": false}),
            deadline,
        )?;
        self.state.change(|seen| seen.size = Some((width, height)));
        Ok(())
    }

    /// The viewport's size: what [`Tab::resize`] set, or the window's.
    pub fn size(&self) -> (u32, u32) {
        lock(&self.state.seen).size.unwrap_or(self.inner.window)
    }

    /// Whether the page offers a hardware WebGPU adapter: an error saying why not, if not.
    pub fn hardware_adapter(&self, deadline: Instant) -> Result<(), String> {
        let found = self.evaluate(script::ADAPTER, deadline)?;
        match found.as_str() {
            Some("") => Ok(()),
            Some(why) => Err(format!("{why}: the browser tests need a hardware GPU")),
            None => Err("the page didn't say what adapter it has".into()),
        }
    }

    /// Every console line of the page and its workers so far (at most 100 000).
    pub fn console(&self) -> Vec<String> {
        lock(&self.state.seen).console.clone()
    }

    /// The lines the program printed (`std::io::print`, which the browser runtime shows on the
    /// console as `wrela: <line>`).
    pub fn printed(&self) -> Vec<String> {
        lock(&self.state.seen).printed.clone()
    }

    /// What the page put in `results/DONE`, if it has (seen only by a browser with a policy,
    /// whose requests the driver pauses).
    pub fn done(&self) -> Option<String> {
        lock(&self.state.seen).done.clone()
    }

    /// The requests the policy refused: each method and URL.
    pub fn refused(&self) -> Vec<String> {
        lock(&self.state.seen).refused.clone()
    }

    /// Whether the page's process crashed.
    pub fn crashed(&self) -> bool {
        lock(&self.state.seen).crashed
    }

    /// Whether the tab's browser holds its requests to a policy.
    pub fn held(&self) -> bool {
        self.inner.policy.is_some()
    }

    /// Waits at most `wait` for anything new to be seen.
    pub(crate) fn wait_for_change(&self, wait: Duration) {
        self.state.wait(wait);
    }

    /// The page's screen, as a PNG at most `most` bytes long: the viewport at its size.
    pub fn screenshot(&self, most: usize, deadline: Instant) -> Result<Vec<u8>, String> {
        let (w, h) = self.size();
        let shot = self.call(
            "Page.captureScreenshot",
            json!({"format": "png", "fromSurface": true, "captureBeyondViewport": false,
                   "clip": {"x": 0, "y": 0, "width": w, "height": h, "scale": 1}}),
            deadline,
        )?;
        let data = shot["data"].as_str().ok_or("Page.captureScreenshot gave no data")?;
        if data.len() / 4 * 3 > most {
            return Err(format!("the screenshot is more than {most} bytes"));
        }
        use base64::Engine;
        let png = base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|e| format!("the screenshot's base64: {e}"))?;
        if png.len() > most {
            return Err(format!("the screenshot is more than {most} bytes"));
        }
        Ok(png)
    }

    /// Closes the tab's window by `deadline`.
    pub fn close(&self, deadline: Instant) -> Result<(), String> {
        if self.closed.swap(true, Ordering::AcqRel) {
            return Ok(());
        }
        let closed = self.inner.conn.call(
            None,
            "Target.closeTarget",
            json!({"targetId": self.target}),
            deadline,
        );
        self.inner.conn.unroute(self.tag);
        closed.map(|_| ())
    }
}

impl Drop for Tab {
    fn drop(&mut self) {
        let _ = self.close(Instant::now() + Duration::from_secs(5));
        if let Some(t) = self.thread.take() {
            // Unrouted, its channel ends, and the thread with it, once an event it's handling
            // is done.
            let since = Instant::now();
            while !t.is_finished() && since.elapsed() < EVENT_CALL {
                std::thread::sleep(Duration::from_millis(2));
            }
            if t.is_finished() {
                let _ = t.join();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chrome_is_found_by_platform_and_linux_gets_its_gpu_flags() {
        let set = chrome_binary_in(Some("/x/chrome".into()), "linux", None);
        assert_eq!(set, Path::new("/x/chrome"));
        assert!(chrome_binary_in(None, "macos", None).starts_with("/Applications/"));
        let none = chrome_binary_in(None, "linux", Some("/nowhere".into()));
        assert_eq!(none, Path::new("google-chrome-stable"));
        assert!(platform_args("linux").contains(&"--use-angle=vulkan"));
        assert!(platform_args("linux").contains(&"--password-store=basic"));
        assert!(platform_args("macos").is_empty());
    }

    #[test]
    fn a_launch_names_its_profile_window_and_a_policys_dead_proxy() {
        let mut l = Launch {
            binary: "chrome".into(),
            profile: "/p".into(),
            window: (640, 480),
            args: vec!["--x".into()],
            startup: STARTUP,
            policy: None,
            cancel: None,
        };
        let argv = |l: &Launch| -> Vec<String> {
            l.argv().iter().map(|a| a.to_string_lossy().into_owned()).collect()
        };
        let a = argv(&l);
        assert!(a.contains(&"--headless=new".to_string()));
        assert!(a.contains(&"--user-data-dir=/p".to_string()));
        assert!(a.contains(&"--window-size=640,480".to_string()));
        assert!(!a.iter().any(|x| x.starts_with("--proxy")), "{a:?}");
        assert_eq!(a.last().map(String::as_str), Some("about:blank"));
        assert!(a.contains(&"--x".to_string()));
        l.policy = Some((Arc::new(|_| true), vec!["example.org".into()]));
        let a = argv(&l);
        assert!(a.contains(&"--proxy-bypass-list=127.0.0.1;example.org".to_string()), "{a:?}");
    }

    #[test]
    fn a_console_message_reads_as_the_console_shows_it() {
        let p = json!({"args": [{"type": "string", "value": "wrela: text"}, {"type": "number", "value": 3},
                                 {"type": "undefined"}, {"type": "object", "description": "Array(2)"}]});
        assert_eq!(console_text(&p), "wrela: text 3 undefined Array(2)");
    }
}
