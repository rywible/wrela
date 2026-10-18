//! `std::tool::http` (language.md §15.5): a request over HTTPS to a host the grants name, and
//! its response, as std's `http` writes and reads them. Only `https://` URLs, to a host granted
//! exactly (`--net HOST`, or `--net HOST:PORT` for that port; no user), and no redirect is
//! followed: the tool sees it. A
//! body larger than one answer carries goes through a blob, and a response's body is at most
//! [`MOST_RESPONSE`] bytes. Every request has a deadline, 30 s unless it asks for another up
//! to [`LONGEST`].
//!
//! The client runs on a thread of the request's own, which the worker waits for. If the tool
//! ends first, the worker stops waiting and the request is abandoned: its thread ends by its
//! deadline, and `wrela tool` ends the process, which closes the connection, sooner.
//!
//! A tool test's hosts are fake sites (`Caps::host` says which): the host answers a `GET` (or a
//! `HEAD`) with the file at `scratch/sites/<host>/<path>`, through the capability context like
//! any file, or 404 if there's none, unless the test set a fake for that URL
//! (`std::tool::testing::fake`): a status, a missing page, other bytes, or a delay in the
//! test's fake time. No tool test reaches the network.

use super::services::{Body, Data, Services};
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::time::Duration;

/// A request's deadline unless it asks for another.
const DEADLINE: Duration = Duration::from_secs(30);

/// The longest deadline a request may ask for.
pub(crate) const LONGEST: f64 = 600.0;

/// The most bytes a response's body may have.
pub(crate) const MOST_RESPONSE: u64 = 256 << 20;

/// How often a worker waiting for a request looks for the tool's end.
const POLL: Duration = Duration::from_millis(20);

/// A request, as std's `http::fetch` writes it.
pub(crate) struct Fetch {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Data,
    pub deadline: Duration,
}

/// A request's deadline: `seconds`, or [`DEADLINE`] for 0.
pub(crate) fn deadline(seconds: f64, default: Duration) -> Result<Duration, String> {
    if seconds == 0.0 {
        return Ok(default);
    }
    if !(seconds > 0.0 && seconds <= LONGEST) {
        return Err(format!("a deadline is more than 0 seconds and at most {LONGEST}"));
    }
    Ok(Duration::from_secs_f64(seconds))
}

/// Reads a request: its method, URL, headers (a count, then each name and value), deadline (an
/// `f64`'s bits) and body.
pub(crate) fn parse(b: &mut Body) -> Result<Fetch, String> {
    let method = b.str()?;
    let url = b.str()?;
    let n = b.u32()?;
    let mut headers = Vec::new();
    for _ in 0..n {
        headers.push((b.str()?, b.str()?));
    }
    let deadline = deadline(f64::from_bits(b.u64()?), DEADLINE)?;
    let body = b.data()?;
    b.end()?;
    if method.is_empty() || !method.bytes().all(|c| c.is_ascii_uppercase()) {
        return Err(format!("`{method}` isn't a method: `GET`, `PUT`, ..."));
    }
    Ok(Fetch { method, url, headers, body, deadline })
}

/// The host (with its port, if the URL names one) and the path (from its `/`, without the
/// query) of an `https://` URL, which names no user: what a grant must match, or why it can't.
pub(crate) fn target(url: &str) -> Result<(&str, &str), String> {
    let Some(rest) = url.strip_prefix("https://") else {
        return Err(format!("`{url}` isn't an `https://` URL: a tool reaches hosts over HTTPS"));
    };
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (host, rest) = rest.split_at(end);
    if host.contains('@') {
        return Err(format!("`{url}` names a user: a host is granted as `--net HOST`, alone"));
    }
    let path = rest.split(['?', '#']).next().unwrap_or("");
    Ok((host, if path.is_empty() { "/" } else { path }))
}

/// What a test set for a fake site's URL (`std::tool::testing::fake`): what it answers in
/// place of the file, if anything, and after how long.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Fake {
    pub answer: Option<Faked>,
    /// Seconds of the test's fake time before the answer.
    pub delay: f64,
}

/// What a fake site answers for a URL in place of its file.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Faked {
    /// The file (if there is one) with this status.
    Status(u16),
    /// 404 with no body, though the file is there.
    Missing,
    /// 200 with these bytes, whatever the method.
    Bytes(Vec<u8>),
}

/// The URL a fake is kept under: the URL without its fragment.
pub(crate) fn fake_key(url: &str) -> &str {
    url.split('#').next().unwrap_or(url)
}

/// A response: its status, headers and body, as std's `http::read_response` reads it. A body
/// over 512 KiB is a blob, made for `request`.
fn respond(
    s: &Services,
    request: u32,
    status: u16,
    headers: &[(String, String)],
    body: Vec<u8>,
) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    out.extend_from_slice(&u32::from(status).to_le_bytes());
    out.extend_from_slice(&(headers.len() as u32).to_le_bytes());
    for (name, value) in headers {
        for t in [name, value] {
            out.extend_from_slice(&(t.len() as u32).to_le_bytes());
            out.extend_from_slice(t.as_bytes());
        }
    }
    s.put_data(request, &mut out, body)?;
    Ok(out)
}

/// Carries out `f` for `request`: from a fake site if its host is one, else over the network.
pub(crate) fn fetch(s: &Services, request: u32, f: Fetch) -> Result<Vec<u8>, String> {
    let (host, path) = target(&f.url)?;
    match s.caps.host(host) {
        None => Err(format!("the host `{host}` isn't granted: `--net {host}` grants it, exactly")),
        Some(true) => fake(s, request, &f, host, path),
        Some(false) => real(s, request, f),
    }
}

/// A fake site's answer: the test's fake for the URL, else the file under
/// `scratch/sites/<host>/` (a path ending in `/` is its `index.html`), for `GET` and `HEAD`.
fn fake(s: &Services, request: u32, f: &Fetch, host: &str, path: &str) -> Result<Vec<u8>, String> {
    let set = s.fakes().get(fake_key(&f.url)).cloned().unwrap_or_default();
    let length = |n: usize| vec![("content-length".to_string(), n.to_string())];
    let status = match set.answer {
        Some(Faked::Missing) => return respond(s, request, 404, &length(0), Vec::new()),
        Some(Faked::Bytes(bytes)) => return respond(s, request, 200, &length(bytes.len()), bytes),
        Some(Faked::Status(n)) => Some(n),
        None => None,
    };
    if f.method != "GET" && f.method != "HEAD" {
        return respond(s, request, status.unwrap_or(405), &length(0), Vec::new());
    }
    let mut file = path.trim_start_matches('/').to_string();
    if file.is_empty() || file.ends_with('/') {
        file.push_str("index.html");
    }
    let mut bytes = Vec::new();
    let read = s.caps.read_chunks(
        &format!("scratch/sites/{host}/{file}"),
        |size| s.blobs().fits(size),
        |chunk| {
            bytes.extend_from_slice(chunk);
            Ok(())
        },
    );
    let (found, body) = match read {
        Ok(()) => (200, bytes),
        Err(_) => (404, Vec::new()),
    };
    let headers = length(body.len());
    let body = if f.method == "HEAD" { Vec::new() } else { body };
    respond(s, request, status.unwrap_or(found), &headers, body)
}

/// A response the client gave: status, headers, body.
type Got = Result<(u16, Vec<(String, String)>, Vec<u8>), String>;

/// `f` over the network, on a thread of its own, which the worker waits for until it answers
/// or the tool ends.
fn real(s: &Services, request: u32, f: Fetch) -> Result<Vec<u8>, String> {
    let body: std::sync::Arc<Vec<u8>> = match &f.body {
        Data::Bytes(b) => std::sync::Arc::new(b.clone()),
        Data::Blob(id) => s.blobs().get(*id)?,
    };
    let (done, wait) = mpsc::channel::<Got>();
    let spawned = std::thread::Builder::new().name("wrela-tool-http".into()).spawn(move || {
        let _ = done.send(send(&f, &body));
    });
    spawned.map_err(|e| format!("can't start the request's thread: {e}"))?;
    let got = loop {
        match wait.recv_timeout(POLL) {
            Ok(got) => break got,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if s.closing.load(Ordering::Acquire) {
                    return Err("the tool ended before the response".into());
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err("the request's thread stopped".into());
            }
        }
    }?;
    let (status, headers, body) = got;
    respond(s, request, status, &headers, body)
}

/// Sends `f` with `body` and reads the response, within its deadline: no proxy, no redirect
/// followed, any status a response (not an error).
fn send(f: &Fetch, body: &[u8]) -> Got {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .https_only(true)
        .max_redirects(0)
        .http_status_as_error(false)
        .proxy(None)
        .timeout_global(Some(f.deadline))
        .build()
        .into();
    let mut builder = ureq::http::Request::builder().method(f.method.as_str()).uri(&f.url);
    for (name, value) in &f.headers {
        builder = builder.header(name, value);
    }
    let failed = |e: &dyn std::fmt::Display| format!("{}: {e}", f.url);
    let response = if body.is_empty() {
        let r = builder.body(ureq::SendBody::none()).map_err(|e| failed(&e))?;
        agent.run(r)
    } else {
        let r = builder.body(body).map_err(|e| failed(&e))?;
        agent.run(r)
    };
    let mut response = response.map_err(|e| failed(&e))?;
    let status = response.status().as_u16();
    let headers = response
        .headers()
        .iter()
        .map(|(n, v)| (n.as_str().to_string(), String::from_utf8_lossy(v.as_bytes()).into_owned()))
        .collect();
    let body = response
        .body_mut()
        .with_config()
        .limit(MOST_RESPONSE)
        .read_to_vec()
        .map_err(|e| failed(&e))?;
    Ok((status, headers, body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_https_urls_without_a_user_are_targets() {
        assert_eq!(target("https://game.example/a/b?x=1"), Ok(("game.example", "/a/b")));
        assert_eq!(target("https://game.example"), Ok(("game.example", "/")));
        assert_eq!(target("https://game.example:8443/x"), Ok(("game.example:8443", "/x")));
        for bad in [
            "http://game.example/",
            "ftp://game.example/",
            "https://user@game.example/",
            "game.example/",
        ] {
            assert!(target(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_deadline_is_the_default_or_within_the_longest() {
        assert_eq!(deadline(0.0, DEADLINE), Ok(DEADLINE));
        assert_eq!(deadline(2.5, DEADLINE), Ok(Duration::from_millis(2500)));
        for bad in [-1.0, f64::NAN, f64::INFINITY, LONGEST + 1.0] {
            assert!(deadline(bad, DEADLINE).is_err(), "{bad}");
        }
    }
}
