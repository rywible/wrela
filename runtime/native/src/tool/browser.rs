//! `std::tool::browser` (language.md §15.5): Chrome stable, headless, on the hardware GPU, with a
//! throwaway profile in the host's own directory for the run, driven over the DevTools protocol
//! (`wrela_chrome`). `open` starts a browser of its own for a page; `run` carries out a script's
//! typed steps; the observations read what the page shows; `capture` gives its screen as a PNG;
//! `close` ends it. Starting one needs the `--browser` grant.
//!
//! **A browser reaches only what the tool is granted.** Every request of its pages and workers
//! (service and shared workers too, each paused at its start until it's held) is paused by the
//! DevTools Fetch domain, and goes on only to an origin of one of the tool's running servers
//! (`std::tool::serve`, `http://127.0.0.1:<port>`) or to `https://` a host `--net` grants: any
//! other fails as blocked, and the page's console says so. A host name it isn't granted leads to
//! a dead proxy, so a WebSocket, which the Fetch domain doesn't see, reaches no other host either.
//! A tool test's fake sites aren't the browser's.
//!
//! **A browser is a resource of its request.** Its process group is recorded for the
//! supervisor (`run::start_owned`) before Chrome runs; it's registered under the `open` request
//! as soon as Chrome answers, before its page opens and before the answer that names it reaches
//! the tool. If the request fails, or the tool ends first, the host closes it: the group killed,
//! the profile removed. A start in progress when the tool ends is given up. The host closes every
//! browser when the tool ends: first every connection (each call waiting on Chrome fails at
//! once, so its worker returns), then every process group.
//!
//! **Deadlines count from the request.** Each request has one (60 s, or its `timeout`, at most
//! 600 s), from when the host took it; a script's waits and clicks are bounded by it too, and a
//! wait looks for the tool's end as it waits. A capture reserves its blob's most bytes (a PNG of
//! the viewport, stored) in the budget before it asks Chrome for the screen.

use super::services::{Answer, Body, Services, Work};
use super::toolchain::Out;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};
use wrela_chrome::script::{Element, Query, Seen};
use wrela_chrome::{Browser, Launch, TabOptions};

/// A request's deadline unless it asks for another.
const DEADLINE: Duration = Duration::from_secs(60);

/// The browsers of one run: each by its handle, and each request's that the tool hasn't been
/// told of.
#[derive(Default)]
pub(crate) struct Browsers {
    held: HashMap<u32, Arc<Held>>,
    next: u32,
    unpublished: HashMap<u32, Vec<u32>>,
}

impl Browsers {
    /// The tool has read request `request`'s answer: its browser is the tool's now.
    pub(crate) fn publish(&mut self, request: u32) {
        self.unpublished.remove(&request);
    }

    /// Request `request` failed: the browsers made for it, which no answer names, to close.
    pub(crate) fn rollback(&mut self, request: u32) -> Vec<Arc<Held>> {
        let ids = self.unpublished.remove(&request).unwrap_or_default();
        ids.iter().filter_map(|id| self.held.remove(id)).collect()
    }

    /// Takes every browser, to close: the tool has ended.
    pub(crate) fn take_all(&mut self) -> Vec<Arc<Held>> {
        self.unpublished.clear();
        self.held.drain().map(|(_, b)| b).collect()
    }

    /// Closes every browser's connection: each call waiting on Chrome fails at once.
    pub(crate) fn disconnect_all(&self) {
        for b in self.held.values() {
            b.browser.disconnect();
        }
    }
}

/// A browser and its page, and whether a request is using it.
pub(crate) struct Held {
    browser: Browser,
    tab: Mutex<Option<wrela_chrome::Tab>>,
    busy: AtomicBool,
}

impl Held {
    /// Closes it: its page, its process group and its profile. Whether that ended cleanly.
    pub(crate) fn close(&self) -> bool {
        // A request using the page fails at once, and lets it go.
        self.browser.disconnect();
        let tab = self.tab.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take();
        if let Some(t) = tab {
            let _ = t.close(Instant::now() + Duration::from_millis(500));
        }
        self.browser.close()
    }
}

fn failure(why: String) -> Answer {
    super::services::answer(Err(why))
}

/// A request's deadline: the `f64` of seconds at its end (0: [`DEADLINE`]).
fn deadline(b: &mut Body) -> Result<Duration, String> {
    let d = super::http::deadline(f64::from_bits(b.u64()?), DEADLINE)?;
    b.end()?;
    Ok(d)
}

fn element(b: &mut Body) -> Result<Element, String> {
    let css = b.str()?;
    if css.is_empty() {
        return Err("an element's selector is empty".into());
    }
    let text = match b.u8()? {
        0 => None,
        1 => Some(b.str()?),
        k => return Err(format!("{k} isn't whether an element has a text")),
    };
    Ok(Element { css, text })
}

/// What an observation reads: a page's query, its console, or its result.
enum Look {
    Page(Query),
    Console,
    Results,
}

/// How request `name` of the browser service, with `b`, is carried out.
pub(crate) fn route(name: &str, b: &mut Body) -> Work {
    let entered = Instant::now();
    let parsed = (|| -> Result<Work, String> {
        Ok(match name {
            "open" => {
                let url = b.str()?;
                let (w, h) = (b.u32()?, b.u32()?);
                if !(1..=16384).contains(&w) || !(1..=16384).contains(&h) {
                    return Err(format!("a viewport of {w}×{h} isn't 1 to 16384 each way"));
                }
                let due = entered + deadline(b)?;
                Work::Long(Box::new(move |s, request| {
                    super::services::answer(open(s, request, &url, (w, h), due))
                }))
            }
            "run" => {
                let handle = b.u32()?;
                let steps = wrela_chrome::script::parse(&b.str()?)?;
                let limit = deadline(b)?;
                let due = entered + limit;
                Work::Long(Box::new(move |s, _| {
                    super::services::answer(using(s, handle, due, |_, tab| {
                        let closing = s.closing.clone();
                        let stop = move || {
                            closing.load(Ordering::Acquire).then(|| "the tool ended".to_string())
                        };
                        let ran = wrela_chrome::script::run(tab, &steps, due, &stop);
                        ran.map(|()| Vec::new()).map_err(|why| {
                            if Instant::now() >= due {
                                let s = limit.as_secs_f64();
                                format!("`run` ran past its deadline of {s} s: {why}")
                            } else {
                                why
                            }
                        })
                    }))
                }))
            }
            "observe" => {
                let handle = b.u32()?;
                let kind = b.u8()?;
                let timeout = f64::from_bits(b.u64()?);
                let look = match kind {
                    0 => Look::Page(Query::Title),
                    1 => Look::Page(Query::Location),
                    2 => Look::Page(Query::Clipboard),
                    3 => Look::Page(Query::Count(element(b)?)),
                    4 => Look::Page(Query::Texts(b.str()?)),
                    5 => {
                        let e = element(b)?;
                        Look::Page(Query::Attribute(e, b.str()?))
                    }
                    6 => Look::Page(Query::Active),
                    7 => Look::Console,
                    8 => Look::Results,
                    k => return Err(format!("{k} isn't an observation")),
                };
                b.end()?;
                let due = entered + super::http::deadline(timeout, DEADLINE)?;
                Work::Long(Box::new(move |s, _| {
                    super::services::answer(using(s, handle, due, |_, tab| {
                        observe(tab, &look, due)
                    }))
                }))
            }
            "capture" => {
                let handle = b.u32()?;
                let due = entered + deadline(b)?;
                Work::Long(Box::new(move |s, request| {
                    super::services::answer(using(s, handle, due, |_, tab| {
                        capture(s, request, tab, due)
                    }))
                }))
            }
            "close" => {
                let handle = b.u32()?;
                b.end()?;
                Work::Long(Box::new(move |s, _| {
                    let held = s.browsers().held.remove(&handle);
                    let Some(held) = held else {
                        return failure(format!("the tool holds no browser {handle}"));
                    };
                    if !held.close() {
                        let why = format!("the browser {handle} didn't end cleanly");
                        let _ = s.stuck.set(why.clone());
                        return failure(why);
                    }
                    super::services::answer(Ok(Vec::new()))
                }))
            }
            _ => return Err(format!("the browser service has no `{name}`")),
        })
    })();
    parsed.unwrap_or_else(|why| Work::Now(failure(why)))
}

/// Whether a page of the tool's may reach `url`: an origin of one of its running servers, or
/// `https://` a host `--net` grants (not a tool test's fake). `data:`, `blob:` and `about:`
/// reach nothing.
fn allowed(s: &Services, url: &str) -> bool {
    if url.starts_with("data:") || url.starts_with("blob:") || url.starts_with("about:") {
        return true;
    }
    let Some((scheme, rest)) = url.split_once("://") else { return false };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.contains('@') {
        return false;
    }
    match scheme {
        "http" => {
            let Some(port) =
                authority.strip_prefix("127.0.0.1:").and_then(|p| p.parse::<u16>().ok())
            else {
                return false;
            };
            s.servers().ports().contains(&port)
        }
        "https" => {
            let host = authority.to_ascii_lowercase();
            let host = host.strip_suffix(":443").unwrap_or(&host);
            s.caps.host(host) == Some(false)
        }
        _ => false,
    }
}

/// The hosts `--net` grants: the dead proxy lets these by.
fn granted_hosts(s: &Services) -> Vec<String> {
    s.caps.net_hosts().iter().map(|h| h.split(':').next().unwrap_or(h).to_string()).collect()
}

/// Opens `url` in a browser of its own for `request`, by `due`: its handle.
fn open(
    s: &Arc<Services>,
    request: u32,
    url: &str,
    size: (u32, u32),
    due: Instant,
) -> Result<Vec<u8>, String> {
    if !s.caps.browser() {
        return Err("the browser isn't granted: `--browser` grants it".into());
    }
    if !allowed(s, url) {
        return Err(format!(
            "`{url}` isn't one of the tool's servers' origins or an `https://` host it's granted"
        ));
    }
    let left = due.saturating_duration_since(Instant::now());
    if left.is_zero() {
        return Err("`open` didn't start within its deadline".into());
    }
    let handle = {
        let mut b = s.browsers();
        b.next += 1;
        b.next
    };
    let profile = s.work_dir()?.join(format!("browser-{handle}"));
    let mut launch = Launch::new(profile)?;
    launch.window = size;
    launch.startup = launch.startup.min(left);
    launch.cancel = Some(s.closing.clone());
    let weak: Weak<Services> = Arc::downgrade(s);
    let policy: wrela_chrome::Policy =
        Arc::new(move |url: &str| weak.upgrade().is_some_and(|s| allowed(&s, url)));
    launch.policy = Some((policy, granted_hosts(s)));
    let go = s.work_dir()?.join(format!("{request}.browser.go"));
    let spawn = |exe: &std::path::Path, args: &[std::ffi::OsString], log: std::fs::File| {
        let err = log.try_clone().map_err(|e| format!("Chrome's log: {e}"))?;
        let mut command = super::run::owned_command(s, exe, args.to_vec(), &go)?;
        command.stdin(std::process::Stdio::null()).stdout(log).stderr(err);
        super::run::start_owned(s, &mut command, &go, "Chrome")
    };
    let launched = Browser::launch(launch, &spawn);
    // Chrome has started (or won't): the file that let it go has done its work.
    let _ = std::fs::remove_file(&go);
    let browser = launched?;
    let held = Arc::new(Held { browser, tab: Mutex::new(None), busy: AtomicBool::new(true) });
    // Registered under its request before its page opens, and before any answer names it.
    {
        let mut b = s.browsers();
        b.held.insert(handle, held.clone());
        b.unpublished.entry(request).or_default().push(handle);
    }
    if s.closing.load(Ordering::Acquire) {
        // The tool ended as it started: its end may have closed every connection already.
        return Err("the tool ended before the browser opened".into());
    }
    let tab = held.browser.open(url, TabOptions { size: Some(size), console: None }, due)?;
    // A page between documents has no context to ask yet: until it has one.
    loop {
        match tab.hardware_adapter(due) {
            Ok(()) => break,
            Err(why) if why.contains("xecution context") && Instant::now() < due => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(why) => return Err(why),
        }
    }
    *held.tab.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(tab);
    held.busy.store(false, Ordering::Release);
    s.signal("browser");
    let mut out = Out::default();
    out.u32(handle);
    Ok(out.0)
}

/// Does `work` with `handle`'s browser and page, by `due`, one request at a time.
fn using(
    s: &Services,
    handle: u32,
    due: Instant,
    work: impl FnOnce(&Held, &wrela_chrome::Tab) -> Result<Vec<u8>, String>,
) -> Result<Vec<u8>, String> {
    if Instant::now() >= due {
        return Err("the request didn't start within its deadline".into());
    }
    let held = s
        .browsers()
        .held
        .get(&handle)
        .cloned()
        .ok_or_else(|| format!("the tool holds no browser {handle}"))?;
    if held.busy.swap(true, Ordering::AcqRel) {
        return Err(format!("the browser {handle} is busy with another request"));
    }
    struct Idle<'a>(&'a AtomicBool);
    impl Drop for Idle<'_> {
        fn drop(&mut self) {
            self.0.store(false, Ordering::Release);
        }
    }
    let _idle = Idle(&held.busy);
    let tab = held.tab.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(tab) = tab.as_ref() else { return Err(format!("the browser {handle} has no page")) };
    work(&held, tab)
}

/// What `look` reads of `tab`'s page, as `std::tool::browser`'s readers read it.
fn observe(tab: &wrela_chrome::Tab, look: &Look, due: Instant) -> Result<Vec<u8>, String> {
    let mut out = Out::default();
    let texts = |out: &mut Out, lines: &[String]| {
        out.u32(lines.len() as u32);
        for l in lines {
            out.str(&super::toolchain::bounded(l));
        }
    };
    match look {
        Look::Console => {
            // At most what an answer holds: the last lines, each bounded.
            let lines = tab.console();
            let from = lines.len().saturating_sub(4096);
            texts(&mut out, &lines[from..]);
        }
        Look::Results => match tab.done() {
            Some(d) => {
                out.u8(1).str(&super::toolchain::bounded(&d));
            }
            None => {
                out.u8(0);
            }
        },
        Look::Page(q) => match wrela_chrome::script::observe(tab, q, due)? {
            Seen::Text(t) => {
                out.str(&super::toolchain::bounded(&t));
            }
            Seen::Texts(t) => {
                let from = t.len().saturating_sub(4096);
                texts(&mut out, &t[..t.len() - from]);
            }
            Seen::Count(n) => {
                out.u32(n);
            }
            Seen::Attribute(a) => match a {
                Some(a) => {
                    out.u8(1).str(&super::toolchain::bounded(&a));
                }
                None => {
                    out.u8(0);
                }
            },
            Seen::Active(a) => match a {
                Some((tag, id)) => {
                    out.u8(1).str(&tag).str(&super::toolchain::bounded(&id));
                }
                None => {
                    out.u8(0);
                }
            },
        },
    }
    Ok(out.0)
}

/// The most bytes a PNG of `w` × `h` RGBA pixels takes: its rows stored, each with its filter
/// byte, deflate's block headers, and its chunks.
fn png_bound(w: u32, h: u32) -> u64 {
    let raw = u64::from(h) * (1 + 4 * u64::from(w));
    raw + raw / 65535 * 5 + 64 * 1024
}

/// `tab`'s screen for `request`: the blob and its bytes reserved first, then filled.
fn capture(
    s: &Services,
    request: u32,
    tab: &wrela_chrome::Tab,
    due: Instant,
) -> Result<Vec<u8>, String> {
    let (w, h) = tab.size();
    let most = png_bound(w, h);
    let id = s
        .blobs()
        .create_reserved(request, most)
        .map_err(|why| format!("the screen's {most} bytes: {why}"))?;
    // The blob is the request's: a failure gives it back.
    let png = tab.screenshot(most as usize, due)?;
    s.blobs().fill(id, png)?;
    let mut out = Out::default();
    out.u32(w).u32(h).u8(1).u32(id);
    Ok(out.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pngs_bound_holds_its_rows_stored() {
        assert!(png_bound(1, 1) >= 5);
        let raw = 1080 * (1 + 4 * 1920);
        assert!(png_bound(1920, 1080) > raw && png_bound(1920, 1080) < raw + raw / 100 + 65536);
    }
}
