//! TCP for a service: listening (`tcp/listen`), connecting (`tcp/connect`) and a connection's peer
//! (`tcp/peer`). A connection is one of the program's connections (language.md §17.2): a
//! `Connection` event when it opens and exactly one when it closes, its bytes as `Message`s
//! between, and `platform::net`'s `send` and `close` to write and end it.
//!
//! - `tcp/listen`: an address, `host:port` (port 0: any free one). The answer is the `u16` port
//!   it listens on. Each connection it accepts opens with that port as its event's tag.
//! - `tcp/connect`: an address. The answer, once connected, is the connection's number (`u32`);
//!   its `Connection` event, tagged 0, follows.
//! - `tcp/peer`: a connection's number. The answer is its peer's address, `ip:port`.

use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

/// What a connection's writer is told.
pub enum Outgoing {
    Bytes(Vec<u8>),
    Close,
}

/// What reaches a service's loop from its connections.
pub enum Net {
    /// Connection `conn` opened, tagged `tag`, from `peer`; `writer` writes to it.
    Opened {
        conn: u32,
        tag: u32,
        peer: String,
        writer: Sender<Outgoing>,
    },
    Data {
        conn: u32,
        bytes: Vec<u8>,
    },
    Closed {
        conn: u32,
    },
}

/// The numbers connections get, from 1, shared by a service's listeners and its connects; and
/// where their events go.
#[derive(Clone)]
pub struct Conns {
    next: Arc<AtomicU32>,
    to_loop: Sender<crate::Event>,
    /// Each listener's address, and whether it is to stop: a service that ends stops its
    /// listeners, so its addresses are free again even while its process goes on.
    listeners: Arc<Mutex<Vec<(std::net::SocketAddr, Arc<AtomicBool>)>>>,
}

impl Conns {
    pub fn new(to_loop: Sender<crate::Event>) -> Conns {
        Conns { next: Arc::new(AtomicU32::new(1)), to_loop, listeners: Arc::default() }
    }

    /// Stops every listener: each is woken by a connection of its own and closes.
    pub fn stop(&self) {
        let all = std::mem::take(&mut *self.listeners.lock().expect("the listeners' lock"));
        for (addr, stopping) in all {
            stopping.store(true, Ordering::SeqCst);
            let _ = TcpStream::connect_timeout(&addr, std::time::Duration::from_secs(1));
        }
    }

    fn number(&self) -> u32 {
        self.next.fetch_add(1, Ordering::Relaxed)
    }

    /// Takes `stream` as connection `conn`: its `Opened` event, then a writer and a reader on
    /// threads of their own. The reader's end, or the writer's close, ends it, with one `Closed`.
    fn attach(&self, stream: TcpStream, conn: u32, tag: u32) {
        let send = self.to_loop.clone();
        let peer = stream.peer_addr().map(|a| a.to_string()).unwrap_or_default();
        let _ = stream.set_nodelay(true);
        let (writer, writes) = mpsc::channel::<Outgoing>();
        let _ = send.send(crate::Event::Net(Net::Opened { conn, tag, peer, writer }));
        let Ok(mut out) = stream.try_clone() else {
            let _ = send.send(crate::Event::Net(Net::Closed { conn }));
            return;
        };
        thread::spawn(move || {
            for w in writes {
                match w {
                    Outgoing::Bytes(b) if out.write_all(&b).is_ok() => {}
                    _ => break,
                }
            }
            let _ = out.shutdown(Shutdown::Both);
        });
        let mut input = stream;
        thread::spawn(move || {
            let mut buf = vec![0u8; 64 << 10];
            while let Ok(n @ 1..) = input.read(&mut buf) {
                let data = Net::Data { conn, bytes: buf[..n].to_vec() };
                if send.send(crate::Event::Net(data)).is_err() {
                    break;
                }
            }
            let _ = send.send(crate::Event::Net(Net::Closed { conn }));
        });
    }

    /// Listens on `addr`: the port, and a thread that accepts each connection.
    pub fn listen(&self, addr: &str) -> Result<u16, String> {
        let listener = TcpListener::bind(addr).map_err(|e| format!("listening on {addr}: {e}"))?;
        let mut local = listener.local_addr().map_err(|e| format!("{addr}: {e}"))?;
        let port = local.port();
        if local.ip().is_unspecified() {
            local.set_ip(if local.is_ipv4() {
                std::net::Ipv4Addr::LOCALHOST.into()
            } else {
                std::net::Ipv6Addr::LOCALHOST.into()
            });
        }
        let stopping = Arc::new(AtomicBool::new(false));
        self.listeners.lock().expect("the listeners' lock").push((local, Arc::clone(&stopping)));
        let conns = self.clone();
        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                if stopping.load(Ordering::SeqCst) {
                    break;
                }
                conns.attach(stream, conns.number(), u32::from(port));
            }
        });
        Ok(port)
    }

    /// Connects to `addr` on a thread of its own; `done` gets the connection's number, before
    /// the connection's events.
    pub fn connect(
        &self,
        addr: String,
        done: impl FnOnce(Result<Vec<u8>, String>) + Send + 'static,
    ) {
        let conns = self.clone();
        thread::spawn(move || match connect_within(&addr, CONNECT_TIMEOUT) {
            Ok(stream) => {
                let conn = conns.number();
                done(Ok(conn.to_le_bytes().to_vec()));
                conns.attach(stream, conn, 0);
            }
            Err(e) => done(Err(format!("connecting to {addr}: {e}"))),
        });
    }
}

/// How long a connection may take to open: a peer that is off the network fails in this, rather
/// than in the system's minutes.
const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// Connects to `addr` (`host:port`), trying each of its addresses for at most `limit`.
fn connect_within(addr: &str, limit: std::time::Duration) -> std::io::Result<TcpStream> {
    use std::net::ToSocketAddrs;
    let mut last = std::io::Error::new(std::io::ErrorKind::NotFound, "no address");
    for a in addr.to_socket_addrs()? {
        match TcpStream::connect_timeout(&a, limit) {
            Ok(s) => return Ok(s),
            Err(e) => last = e,
        }
    }
    Err(last)
}
