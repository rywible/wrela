//! The DevTools protocol over Chrome's WebSocket: one connection to the browser, its pages'
//! and workers' sessions flattened onto it (`Target.attachToTarget` with `flatten`), so each
//! message names its session.
//!
//! **One thread owns the socket.** It sends what callers queue and reads what Chrome sends, in
//! turn, with a short read timeout, so no caller ever holds the socket. An answer goes to the
//! caller waiting for its `id`; an event goes to the channel of its session, and a session that
//! a routed session's auto-attach makes is routed to the same channel before its first event can
//! arrive. A caller waits at most until its deadline. When the socket closes or fails, every
//! waiting caller is told why, and every channel ends.

use serde_json::{Value, json};
use std::collections::HashMap;
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};
use tungstenite::{Message, WebSocket};

/// How long the socket's thread reads before it looks for messages to send.
const TURN: Duration = Duration::from_millis(5);

/// The largest message Chrome may send: a screenshot's base64 at 16384 pixels each way.
const MOST_MESSAGE: usize = 1536 << 20;

/// An event of a session: its method and parameters.
#[derive(Clone, Debug)]
pub struct Event {
    pub session: String,
    pub method: String,
    pub params: Value,
}

type Reply = mpsc::Sender<Result<Value, String>>;

/// What the socket's thread and the callers share.
struct Shared {
    pending: Mutex<HashMap<u64, Reply>>,
    /// Each session's channel, with the tag of the route it came from.
    routes: Mutex<HashMap<String, (u64, mpsc::Sender<Event>)>>,
    /// Why the connection ended, once it has.
    ended: Mutex<Option<String>>,
    stop: AtomicBool,
}

impl Shared {
    fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
        m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Ends the connection for `why`: each waiting caller hears it, and each channel ends.
    fn end(&self, why: String) {
        let mut ended = Self::lock(&self.ended);
        if ended.is_none() {
            *ended = Some(why.clone());
        }
        drop(ended);
        for (_, reply) in Self::lock(&self.pending).drain() {
            let _ = reply.send(Err(why.clone()));
        }
        Self::lock(&self.routes).clear();
    }
}

/// A connection to a browser's DevTools.
pub struct Connection {
    shared: Arc<Shared>,
    out: mpsc::Sender<String>,
    next: AtomicU64,
    tags: AtomicU64,
    thread: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl Connection {
    /// Connects to the DevTools WebSocket at `ws://127.0.0.1:<port><path>` by `deadline`.
    pub fn open(port: u16, path: &str, deadline: Instant) -> Result<Connection, String> {
        let left = deadline.saturating_duration_since(Instant::now()).max(TURN);
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
        let stream = TcpStream::connect_timeout(&addr, left)
            .map_err(|e| format!("can't reach DevTools on port {port}: {e}"))?;
        stream.set_read_timeout(Some(left)).map_err(|e| e.to_string())?;
        stream.set_write_timeout(Some(Duration::from_secs(10))).map_err(|e| e.to_string())?;
        let _ = stream.set_nodelay(true);
        let config = tungstenite::protocol::WebSocketConfig::default()
            .max_message_size(Some(MOST_MESSAGE))
            .max_frame_size(Some(MOST_MESSAGE));
        let url = format!("ws://127.0.0.1:{port}{path}");
        let (socket, _) =
            tungstenite::client::client_with_config(url.as_str(), stream, Some(config))
                .map_err(|e| format!("DevTools refused the WebSocket: {e}"))?;
        socket.get_ref().set_read_timeout(Some(TURN)).map_err(|e| e.to_string())?;
        Ok(Connection::start(socket))
    }

    /// The connection over `socket`, whose handshake is done, and its thread.
    fn start(socket: WebSocket<TcpStream>) -> Connection {
        let shared = Arc::new(Shared {
            pending: Mutex::new(HashMap::new()),
            routes: Mutex::new(HashMap::new()),
            ended: Mutex::new(None),
            stop: AtomicBool::new(false),
        });
        let (out, queued) = mpsc::channel::<String>();
        let s = shared.clone();
        let thread = std::thread::Builder::new()
            .name("wrela-chrome-devtools".into())
            .spawn(move || serve(socket, &s, &queued));
        let thread = match thread {
            Ok(t) => Some(t),
            Err(e) => {
                shared.end(format!("can't start the DevTools thread: {e}"));
                None
            }
        };
        Connection {
            shared,
            out,
            next: AtomicU64::new(1),
            tags: AtomicU64::new(1),
            thread: Mutex::new(thread),
        }
    }

    /// Why the connection ended, if it has.
    pub fn ended(&self) -> Option<String> {
        Shared::lock(&self.shared.ended).clone()
    }

    /// Routes session `session`'s events (`""`: the browser's own) to `to`, and those of each
    /// session it attaches to: the route's tag, which [`Connection::unroute`] takes.
    pub fn route(&self, session: &str, to: mpsc::Sender<Event>) -> u64 {
        let tag = self.tags.fetch_add(1, Ordering::Relaxed);
        if self.ended().is_none() {
            Shared::lock(&self.shared.routes).insert(session.to_string(), (tag, to));
        }
        tag
    }

    /// Stops routing the sessions of route `tag`: its channel ends.
    pub fn unroute(&self, tag: u64) {
        Shared::lock(&self.shared.routes).retain(|_, (t, _)| *t != tag);
    }

    /// Stops routing session `session` (it has detached).
    pub fn unroute_session(&self, session: &str) {
        Shared::lock(&self.shared.routes).remove(session);
    }

    /// Calls `method` with `params` in `session` (`None`: the browser's), waiting for its answer
    /// until `deadline`: its result, or why there's none.
    pub fn call(
        &self,
        session: Option<&str>,
        method: &str,
        params: Value,
        deadline: Instant,
    ) -> Result<Value, String> {
        if let Some(why) = self.ended() {
            return Err(format!("{method}: {why}"));
        }
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let mut message = json!({"id": id, "method": method, "params": params});
        if let Some(s) = session {
            message["sessionId"] = Value::String(s.to_string());
        }
        let (reply, answer) = mpsc::channel();
        Shared::lock(&self.shared.pending).insert(id, reply);
        // The thread may have ended between the check and the insert: it drained `pending`.
        if let Some(why) = self.ended() {
            Shared::lock(&self.shared.pending).remove(&id);
            return Err(format!("{method}: {why}"));
        }
        if self.out.send(message.to_string()).is_err() {
            Shared::lock(&self.shared.pending).remove(&id);
            return Err(format!("{method}: the DevTools connection has closed"));
        }
        let left = deadline.saturating_duration_since(Instant::now());
        match answer.recv_timeout(left) {
            Ok(Ok(v)) => Ok(v),
            Ok(Err(why)) => Err(format!("{method}: {why}")),
            Err(_) => {
                Shared::lock(&self.shared.pending).remove(&id);
                Err(format!("{method}: no answer by its deadline"))
            }
        }
    }

    /// Closes the connection and joins its thread, waiting at most `grace`: whether it ended.
    pub fn close(&self, grace: Duration) -> bool {
        self.shared.stop.store(true, Ordering::Release);
        let Some(t) = Shared::lock(&self.thread).take() else { return true };
        let until = Instant::now() + grace;
        while !t.is_finished() {
            if Instant::now() >= until {
                return false;
            }
            std::thread::sleep(TURN);
        }
        let _ = t.join();
        true
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.close(Duration::from_secs(1));
    }
}

/// The socket's thread: sends what's queued, reads what comes, until it's stopped or the socket
/// ends.
fn serve(mut socket: WebSocket<TcpStream>, s: &Shared, queued: &mpsc::Receiver<String>) {
    let why = loop {
        if s.stop.load(Ordering::Acquire) {
            let _ = socket.close(None);
            let _ = socket.flush();
            break "the DevTools connection was closed".to_string();
        }
        let mut failed = None;
        for text in queued.try_iter() {
            if let Err(e) = socket.write(Message::text(text)) {
                failed = Some(format!("DevTools: {e}"));
                break;
            }
        }
        if let Some(why) = failed {
            break why;
        }
        // A pong to a ping is queued by the read, and sent by this flush.
        match socket.flush() {
            Ok(()) => {}
            Err(tungstenite::Error::Io(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(e) => break format!("DevTools: {e}"),
        }
        match socket.read() {
            Ok(Message::Text(t)) => dispatch(s, t.as_str()),
            Ok(Message::Close(_)) => break "DevTools closed the connection".into(),
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
                break "DevTools closed the connection".into();
            }
            Err(e) => break format!("DevTools: {e}"),
        }
    };
    s.end(why);
}

/// Gives a message to the caller waiting for its answer, or to its session's channel.
fn dispatch(s: &Shared, text: &str) {
    let Ok(message) = serde_json::from_str::<Value>(text) else { return };
    if let Some(id) = message.get("id").and_then(Value::as_u64) {
        let Some(reply) = Shared::lock(&s.pending).remove(&id) else { return };
        let answer = match message.get("error") {
            Some(e) => {
                Err(e.get("message").and_then(Value::as_str).unwrap_or("an error").to_string())
            }
            None => Ok(message.get("result").cloned().unwrap_or(Value::Null)),
        };
        let _ = reply.send(answer);
        return;
    }
    let Some(method) = message.get("method").and_then(Value::as_str) else { return };
    let session = message.get("sessionId").and_then(Value::as_str).unwrap_or("").to_string();
    let params = message.get("params").cloned().unwrap_or(Value::Null);
    let mut routes = Shared::lock(&s.routes);
    let Some((tag, to)) = routes.get(&session).cloned() else { return };
    // A session this one attached to is routed with it before any of its own events come.
    if method == "Target.attachedToTarget"
        && let Some(child) = params.get("sessionId").and_then(Value::as_str)
    {
        routes.insert(child.to_string(), (tag, to.clone()));
    }
    drop(routes);
    let key = session.clone();
    if to.send(Event { session, method: method.to_string(), params }).is_err() {
        // Nobody reads this session's events any more.
        Shared::lock(&s.routes).remove(&key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    /// A WebSocket server of the test's own, on loopback: for each text message it gets, it
    /// sends a ping and an event, then what `answer` gives for the message, with its `id`.
    fn serve_fake(
        answer: impl Fn(&Value) -> Value + Send + 'static,
    ) -> (u16, std::thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("listen");
        let port = listener.local_addr().expect("addr").port();
        let thread = std::thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept");
            let mut ws = tungstenite::accept(stream).expect("handshake");
            let mut seen = Vec::new();
            loop {
                match ws.read() {
                    Ok(Message::Text(t)) => {
                        let m: Value = serde_json::from_str(t.as_str()).expect("json");
                        seen.push(m["method"].as_str().unwrap_or("").to_string());
                        ws.send(Message::Ping(b"hi".to_vec().into())).expect("ping");
                        let event = json!({"method": "Page.event", "sessionId": "s", "params": {}});
                        ws.send(Message::text(event.to_string())).expect("event");
                        let mut a = answer(&m);
                        a["id"] = m["id"].clone();
                        ws.send(Message::text(a.to_string())).expect("answer");
                    }
                    Ok(Message::Pong(_)) => seen.push("pong".into()),
                    Ok(Message::Close(_)) | Err(_) => break,
                    Ok(_) => {}
                }
            }
            seen
        });
        (port, thread)
    }

    fn soon() -> Instant {
        Instant::now() + Duration::from_secs(5)
    }

    #[test]
    fn calls_are_answered_in_order_and_events_reach_their_session() {
        let (port, server) = serve_fake(|m| json!({"result": {"ok": m["method"]}}));
        let conn = Connection::open(port, "/devtools/browser/x", soon()).expect("connect");
        let (to, events) = mpsc::channel();
        conn.route("s", to);
        let a = conn.call(Some("s"), "Input.insertText", json!({"text": "か"}), soon());
        assert_eq!(a.expect("answered"), json!({"ok": "Input.insertText"}));
        let b = conn.call(None, "Browser.getVersion", json!({}), soon()).expect("answered");
        assert_eq!(b, json!({"ok": "Browser.getVersion"}));
        let e = events.recv_timeout(Duration::from_secs(5)).expect("an event");
        assert_eq!((e.session.as_str(), e.method.as_str()), ("s", "Page.event"));
        assert!(conn.close(Duration::from_secs(5)), "its thread ends");
        let seen = server.join().expect("the server");
        let calls: Vec<&String> = seen.iter().filter(|s| *s != "pong").collect();
        assert_eq!(calls, ["Input.insertText", "Browser.getVersion"]);
        assert!(seen.iter().any(|s| s == "pong"), "a ping is answered: {seen:?}");
    }

    #[test]
    fn an_error_says_why_and_a_closed_socket_fails_every_caller() {
        let (port, server) = serve_fake(|_| json!({"error": {"message": "no such key"}}));
        let conn = Connection::open(port, "/devtools/browser/x", soon()).expect("connect");
        let e = conn.call(None, "Input.dispatchKeyEvent", json!({}), soon()).expect_err("an error");
        assert_eq!(e, "Input.dispatchKeyEvent: no such key");
        conn.close(Duration::from_secs(5));
        server.join().expect("the server");
        let e = conn.call(None, "Page.navigate", json!({}), soon()).expect_err("closed");
        assert!(e.contains("closed"), "{e}");
    }

    #[test]
    fn a_call_nobody_answers_fails_at_its_deadline() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("listen");
        let port = listener.local_addr().expect("addr").port();
        let (done, finished) = mpsc::channel::<()>();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept");
            let mut ws = tungstenite::accept(stream).expect("handshake");
            ws.get_ref().set_read_timeout(Some(Duration::from_millis(10))).expect("timeout");
            // Reads and never answers, until the test is done.
            while matches!(finished.try_recv(), Err(mpsc::TryRecvError::Empty)) {
                let _ = ws.read();
            }
        });
        let conn = Connection::open(port, "/devtools/browser/x", soon()).expect("connect");
        let began = Instant::now();
        let e = conn
            .call(None, "Runtime.evaluate", json!({}), Instant::now() + Duration::from_millis(200))
            .expect_err("no answer");
        assert!(e.contains("deadline"), "{e}");
        assert!(began.elapsed() < Duration::from_secs(2), "{:?}", began.elapsed());
        drop(done);
        conn.close(Duration::from_secs(5));
        server.join().expect("the server");
    }
}
