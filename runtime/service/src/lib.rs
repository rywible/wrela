//! The service host: one server build (language.md §15.3) run as a long-lived program, with the
//! host services a program that isn't a game needs. It is a mechanism; what a service does with
//! it is the program's, in Wrela.
//!
//! The program runs in one room ([`wrela_host::server::Room`]): `serve_init` once, then `serve`
//! whenever something happens. It reads its connections' events (`std::input`) and makes
//! requests (`std::host::request`), which the host carries out after the call and answers for a
//! later call to read:
//!
//! | Services | What they do | Where |
//! |---|---|---|
//! | `fs/...` | Files under the directories the service was given | [`files`] |
//! | `proc/run`, `proc/kill` | Processes | [`process`] |
//! | `tcp/listen`, `tcp/connect`, `tcp/peer` | TCP connections, which are the program's connections | [`tcp`] |
//! | `time/now` | The time: milliseconds since 1970 (`u64`) | |
//! | `time/after` | An answer (no bytes) after a `u32` of milliseconds: a wake-up | |
//! | `service/args` | The program's arguments: a `u32` count, then each | |
//! | `service/stdout`, `service/stderr` | Writes the request's bytes there now | |
//! | `service/exit` | Ends the service after this call, with an `i32` status | |
//!
//! `platform::net`'s `send` and `close` write to a connection and end it. The host calls `serve`
//! again at once while answers or events wait, and otherwise when the next one comes; nothing
//! else wakes it, so a program that wants waking asks `time/after`. A call that fails ends the
//! service: the program's trap with status 70, the host's limits with 75.

pub mod body;
pub mod files;
pub mod process;
pub mod tcp;

use body::{Body, Out};
use std::collections::HashMap;
use std::io::Write;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use wrela_host::server::{Failure, Limits, Net as Command, Room, ServerBuild};

/// What reaches the service's loop from the host's threads.
pub enum Event {
    Net(tcp::Net),
    /// A request taken to answer later is answered.
    Answer {
        request: u32,
        answer: Result<Vec<u8>, String>,
    },
}

/// What a service runs: its build, the directories it may use, its arguments and its limits.
pub struct Config {
    pub build: ServerBuild,
    pub roots: files::Roots,
    pub args: Vec<String>,
    pub limits: Limits,
}

/// The default limits of a service: 2 GiB of memory, 10 s a call, and 64 MiB of a call's
/// network commands. A service's calls are its own work, so a call may be long; the deadline
/// stops one that never ends.
pub fn limits() -> Limits {
    Limits { memory: 2 << 30, deadline: Duration::from_secs(10), commands: 64 << 20 }
}

/// A request the program made, for the loop to carry out after the call.
struct Request {
    id: u32,
    service: String,
    body: Vec<u8>,
}

/// The services the host takes to answer later: every service it has.
fn ours(service: &str) -> bool {
    ["fs/", "proc/", "tcp/", "time/", "service/"].iter().any(|p| service.starts_with(p))
}

/// A connection the program has open.
struct Open {
    tag: u32,
    peer: String,
    writer: Sender<tcp::Outgoing>,
}

/// Runs `config` until the program ends it (its status) or a call fails (70 or 75). Its listeners
/// and connections are closed when it ends.
pub fn run(config: Config) -> i32 {
    let (to_loop, from_threads): (Sender<Event>, Receiver<Event>) = mpsc::channel();
    let conns = tcp::Conns::new(to_loop.clone());
    let mut open: HashMap<u32, Open> = HashMap::new();
    let code = serve(&config, &conns, &mut open, &to_loop, &from_threads);
    conns.stop();
    for o in open.values() {
        let _ = o.writer.send(tcp::Outgoing::Close);
    }
    code
}

fn serve(
    config: &Config,
    conns: &tcp::Conns,
    open: &mut HashMap<u32, Open>,
    to_loop: &Sender<Event>,
    from_threads: &Receiver<Event>,
) -> i32 {
    let made: Arc<Mutex<Vec<Request>>> = Arc::default();
    let taking = Arc::clone(&made);
    let later: wrela_host::server::Later = Arc::new(move |id, service, body| {
        if !ours(service) {
            return false;
        }
        let request = Request { id, service: service.to_string(), body: body.to_vec() };
        taking.lock().expect("the requests' lock").push(request);
        true
    });
    let started = Instant::now();
    let mut room = match Room::start_with(&config.build, config.limits, None, Some(later)) {
        Ok(room) => room,
        Err(e) => return failed(&e),
    };
    let running = process::Running::default();
    let mut exit: Option<i32> = None;
    // The program's first `serve` comes at once, so it can start what it serves.
    let mut first = true;
    loop {
        // What the last call asked for, carried out; whether any of it is answered already.
        let mut answered = std::mem::take(&mut first);
        let requests = std::mem::take(&mut *made.lock().expect("the requests' lock"));
        for r in requests {
            let now = carry_out(config, &r, conns, &running, open, to_loop, &mut exit);
            if let Some(answer) = now {
                room.answer(r.id, answer);
                answered = true;
            }
        }
        for c in room.take_commands() {
            match c {
                Command::Send(conn, bytes) => {
                    if let Some(o) = open.get(&conn) {
                        let _ = o.writer.send(tcp::Outgoing::Bytes(bytes));
                    }
                }
                Command::Close(conn) => {
                    if let Some(o) = open.get(&conn) {
                        let _ = o.writer.send(tcp::Outgoing::Close);
                    }
                }
                // A service keeps no journal: its files are its own (`fs/...`).
                Command::Keep(_) | Command::Compact(_) | Command::Tag(..) => {}
            }
        }
        let (printed, dropped) = room.take_printed();
        for line in printed {
            eprintln!("{line}");
        }
        if dropped > 0 {
            eprintln!("({dropped} more lines)");
        }
        if let Some(code) = exit {
            let _ = std::io::stdout().flush();
            return code;
        }
        // Wait for something to happen, unless something already has.
        let mut events = Vec::new();
        if !answered && room.queued() == 0 {
            match from_threads.recv() {
                Ok(e) => events.push(e),
                Err(_) => return 0,
            }
        }
        events.extend(from_threads.try_iter());
        for e in events {
            match e {
                Event::Answer { request, answer } => room.answer(request, answer),
                Event::Net(tcp::Net::Opened { conn, tag, peer, writer }) => {
                    open.insert(conn, Open { tag, peer, writer });
                    room.push_connection(conn, true, tag);
                }
                Event::Net(tcp::Net::Data { conn, bytes }) => {
                    if open.contains_key(&conn) {
                        room.push_message(conn, bytes);
                    }
                }
                Event::Net(tcp::Net::Closed { conn }) => {
                    if let Some(o) = open.remove(&conn) {
                        room.push_connection(conn, false, o.tag);
                    }
                }
            }
        }
        if let Err(e) = room.serve(started.elapsed().as_secs_f32()) {
            return failed(&e);
        }
    }
}

fn failed(e: &Failure) -> i32 {
    eprintln!("wrela-service: {e}");
    if e.is_program() { 70 } else { 75 }
}

/// Carries out request `r`: its answer now, or `None` if it's answered later.
fn carry_out(
    config: &Config,
    r: &Request,
    conns: &tcp::Conns,
    running: &process::Running,
    open: &HashMap<u32, Open>,
    to_loop: &Sender<Event>,
    exit: &mut Option<i32>,
) -> Option<Result<Vec<u8>, String>> {
    if let Some(answer) = files::serve(&config.roots, &r.service, &r.body) {
        return Some(answer);
    }
    let id = r.id;
    let later = {
        let to_loop = to_loop.clone();
        move |answer| {
            let _ = to_loop.send(Event::Answer { request: id, answer });
        }
    };
    let mut b = Body::new(&r.body);
    Some(match r.service.as_str() {
        "time/now" => {
            let ms = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis());
            Ok(Out::default().u64(ms as u64).0)
        }
        "time/after" => match b.u32() {
            Ok(ms) => {
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(u64::from(ms)));
                    later(Ok(Vec::new()));
                });
                return None;
            }
            Err(e) => Err(e),
        },
        "service/args" => {
            let mut out = Out::default().u32(config.args.len() as u32);
            for a in &config.args {
                out = out.str(a);
            }
            Ok(out.0)
        }
        "service/stdout" => {
            let mut s = std::io::stdout();
            s.write_all(&r.body)
                .and_then(|()| s.flush())
                .map(|()| Vec::new())
                .map_err(|e| e.to_string())
        }
        "service/stderr" => {
            let mut s = std::io::stderr();
            s.write_all(&r.body).map(|()| Vec::new()).map_err(|e| e.to_string())
        }
        "service/exit" => match b.u32() {
            Ok(code) => {
                *exit = Some(code as i32);
                Ok(Vec::new())
            }
            Err(e) => Err(e),
        },
        "proc/run" => match process::Run::parse(&config.roots, &r.body) {
            Ok(run) => {
                running.start(run, later);
                return None;
            }
            Err(e) => Err(e),
        },
        "proc/kill" => b.u64().map(|tag| {
            running.kill(tag);
            Vec::new()
        }),
        "tcp/listen" => b.str().and_then(|a| conns.listen(a)).map(|p| p.to_le_bytes().to_vec()),
        "tcp/connect" => match b.str() {
            Ok(addr) => {
                conns.connect(addr.to_string(), later);
                return None;
            }
            Err(e) => Err(e),
        },
        "tcp/peer" => b.u32().and_then(|c| {
            open.get(&c)
                .map(|o| o.peer.as_bytes().to_vec())
                .ok_or(format!("connection {c} isn't open"))
        }),
        other => Err(format!("this service host has no service `{other}`")),
    })
}
