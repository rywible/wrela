//! The tool services (`std::tool`'s modules, language.md §15.5): each request's service and
//! bytes, read as std writes them, and the work that answers it (`http` and `run` are modules
//! of their own; `testing` is a tool test's fakes, §13.2). std owns both sides of every
//! layout; this is the host's half. Every answer starts with a byte, 0 for success then what the
//! service gives, or 1 then why it failed, in UTF-8. A service the host doesn't have fails the
//! request instead, as any host's does.

use super::blobs::Blobs;
use super::caps::{Caps, Entry, MOST_BYTES};
use super::http::{Fake, Faked};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

/// What a request is answered with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Answer {
    /// The answer's bytes, starting with whether the service succeeded.
    Bytes(Vec<u8>),
    /// The host has no such service: the request fails (`REQUEST_FAILED`).
    Failed,
}

/// A success, giving `payload`.
fn ok(payload: &[u8]) -> Answer {
    let mut bytes = Vec::with_capacity(payload.len() + 1);
    bytes.push(0);
    bytes.extend_from_slice(payload);
    Answer::Bytes(bytes)
}

/// A failure, for `why`.
fn err(why: &str) -> Answer {
    let mut bytes = vec![1];
    bytes.extend_from_slice(why.as_bytes());
    Answer::Bytes(bytes)
}

fn answer(result: Result<Vec<u8>, String>) -> Answer {
    match result {
        Ok(payload) => ok(&payload),
        Err(why) => err(&why),
    }
}

/// What every service shares: the capability context, the blobs, a tool test's fakes, the
/// directory the host owns for the run's programs, and whether the tool has ended.
pub(crate) struct Services {
    pub caps: Caps,
    pub blobs: Mutex<Blobs>,
    /// How long a blob's write to a file waits before it writes: a test's slow disk.
    pub io_delay: Duration,
    /// Set once the tool has ended: a worker waiting on a program or a response stops.
    pub closing: Arc<AtomicBool>,
    /// Whether this is a tool test's run, which has fakes (`std::tool::testing`).
    pub testing: bool,
    /// A tool test's fakes for its fake sites' URLs.
    pub fakes: Mutex<HashMap<String, Fake>>,
    /// The run's own directory, made when a program first runs: its `HOME`, and its input and
    /// output files.
    pub work: OnceLock<Result<PathBuf, String>>,
}

impl Services {
    pub(crate) fn blobs(&self) -> std::sync::MutexGuard<'_, Blobs> {
        // A worker that panicked while it held them leaves them as they were.
        self.blobs.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The fakes a tool test set, by URL (none outside a test).
    pub(crate) fn fakes(&self) -> std::sync::MutexGuard<'_, HashMap<String, Fake>> {
        self.fakes.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The run's own directory, with `home/` in it, made the first time it's needed.
    pub(crate) fn work_dir(&self) -> Result<PathBuf, String> {
        self.work
            .get_or_init(|| {
                static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
                let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
                let nanos = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.subsec_nanos());
                let dir = std::env::temp_dir()
                    .join(format!("wrela-tool-{}-{n}-{nanos}", std::process::id()));
                std::fs::create_dir_all(dir.join("home"))
                    .map(|_| dir)
                    .map_err(|e| format!("can't make the run's directory: {e}"))
            })
            .clone()
    }

    /// Writes `bytes` to `out` as std's `blob::read_data` reads it: 0 and the bytes (a count,
    /// then each) if they fit an answer, else 1 and a new blob of them, made for `request`.
    pub(crate) fn put_data(
        &self,
        request: u32,
        out: &mut Vec<u8>,
        bytes: Vec<u8>,
    ) -> Result<(), String> {
        if bytes.len() <= MOST_BYTES {
            out.push(0);
            out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
            out.extend_from_slice(&bytes);
            return Ok(());
        }
        let mut blobs = self.blobs();
        blobs.fits(bytes.len() as u64)?;
        let id = blobs.create(request)?;
        if let Err(why) = blobs.append(id, &bytes) {
            let _ = blobs.release(id);
            return Err(why);
        }
        out.push(1);
        out.extend_from_slice(&id.to_le_bytes());
        Ok(())
    }
}

/// Bytes a request carries, or a blob of them.
pub(crate) enum Data {
    Bytes(Vec<u8>),
    Blob(u32),
}

/// Work on a worker thread: given the services and the request's number.
pub(crate) type Task = Box<dyn FnOnce(&Services, u32) -> Answer + Send>;

/// How a request is carried out.
pub(crate) enum Work {
    /// Answered now, after the call that made it.
    Now(Answer),
    /// Answered, with no bytes, once this time has passed (on the run's clock: a test's is
    /// fake).
    Timer(Duration),
    /// Answered by a worker.
    Worker(Task),
    /// Answered by a worker, but given to the tool only once this much of a test's fake time
    /// has passed since the request: a fake site's delay.
    Held(Duration, Task),
    /// Answered with the date now, on the run's clock.
    Utc,
    /// A test sets its fake clock's date: seconds and nanoseconds since 1970.
    SetUtc(u64, u32),
}

/// A request's bytes, read in order as std's `Writer` wrote them.
pub(crate) struct Body<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Body<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.at.checked_add(n).filter(|&e| e <= self.bytes.len());
        let Some(end) = end else { return Err("the request's bytes end too soon".into()) };
        let out = &self.bytes[self.at..end];
        self.at = end;
        Ok(out)
    }

    pub(crate) fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }

    pub(crate) fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().expect("four bytes")))
    }

    pub(crate) fn u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().expect("eight bytes")))
    }

    pub(crate) fn str(&mut self) -> Result<String, String> {
        let n = self.u32()? as usize;
        String::from_utf8(self.take(n)?.to_vec()).map_err(|_| "text isn't UTF-8".to_string())
    }

    /// Bytes or a blob, as std's `blob::write_data` writes them: 0, a count and the bytes, or
    /// 1 and a blob's handle.
    pub(crate) fn data(&mut self) -> Result<Data, String> {
        match self.u8()? {
            0 => {
                let n = self.u32()? as usize;
                Ok(Data::Bytes(self.take(n)?.to_vec()))
            }
            1 => Ok(Data::Blob(self.u32()?)),
            k => Err(format!("{k} isn't bytes (0) or a blob (1)")),
        }
    }

    fn rest(&mut self) -> &'a [u8] {
        let out = &self.bytes[self.at..];
        self.at = self.bytes.len();
        out
    }

    pub(crate) fn end(&self) -> Result<(), String> {
        if self.at == self.bytes.len() { Ok(()) } else { Err("the request has bytes left".into()) }
    }
}

/// How request `service`, with `body`, is carried out, with what the services hold.
pub(crate) fn route(s: &Services, service: &str, body: &[u8]) -> Work {
    let Some(name) = service.strip_prefix("tool/") else { return Work::Now(Answer::Failed) };
    let mut b = Body { bytes: body, at: 0 };
    match name {
        "time/after" => match b.u64().and_then(|bits| b.end().map(|_| f64::from_bits(bits))) {
            Ok(s) if (0.0..1e9).contains(&s) => Work::Timer(Duration::from_secs_f64(s)),
            Ok(s) => Work::Now(err(&format!("can't wait {s} seconds"))),
            Err(why) => Work::Now(err(&why)),
        },
        "time/utc" => match b.end() {
            Ok(()) => Work::Utc,
            Err(why) => Work::Now(err(&why)),
        },
        "http/fetch" => {
            let f = match super::http::parse(&mut b) {
                Ok(f) => f,
                Err(why) => return Work::Now(err(&why)),
            };
            // A fake site's delay is the test's fake time, read as the request is made.
            let delay = match super::http::target(&f.url) {
                Ok((host, _)) if s.caps.host(host) == Some(true) => {
                    s.fakes().get(super::http::fake_key(&f.url)).map_or(0.0, |f| f.delay)
                }
                _ => 0.0,
            };
            let task: Task = Box::new(move |s, request| answer(super::http::fetch(s, request, f)));
            if delay > 0.0 {
                Work::Held(Duration::from_secs_f64(delay), task)
            } else {
                Work::Worker(task)
            }
        }
        "run/run" => match super::run::parse(&mut b) {
            Ok(r) => {
                Work::Worker(Box::new(move |s, request| answer(super::run::run(s, request, r))))
            }
            Err(why) => Work::Now(err(&why)),
        },
        "testing/fake" | "testing/utc" if !s.testing => Work::Now(err(
            "only a tool test's host has fakes: `std::tool::testing` is for `@test(tool)`",
        )),
        "testing/fake" => match testing_fake(&mut b) {
            Ok((url, change)) => {
                let mut fakes = s.fakes();
                let set = fakes.entry(super::http::fake_key(&url).to_string()).or_default();
                change(set);
                Work::Now(ok(&[]))
            }
            Err(why) => Work::Now(err(&why)),
        },
        "testing/utc" => {
            match (|| Ok::<_, String>((b.u64()?, b.u32()?)))().and_then(|v| b.end().map(|_| v)) {
                Ok((seconds, nanos)) if nanos < 1_000_000_000 => Work::SetUtc(seconds, nanos),
                Ok(_) => Work::Now(err("nanoseconds are fewer than 1000000000")),
                Err(why) => Work::Now(err(&why)),
            }
        }
        "files/read" | "files/list" | "files/stat" | "files/remove" | "files/remove_all"
        | "files/make_dirs" | "files/read_blob" => {
            let path = match b.str().and_then(|p| b.end().map(|_| p)) {
                Ok(p) => p,
                Err(why) => return Work::Now(err(&why)),
            };
            let name = name.to_string();
            Work::Worker(Box::new(move |s, request| files(s, request, &name, &path)))
        }
        "files/write" => {
            let path = match b.str() {
                Ok(p) => p,
                Err(why) => return Work::Now(err(&why)),
            };
            let bytes = b.rest().to_vec();
            if bytes.len() > MOST_BYTES {
                return Work::Now(err("more than 512 KiB at once: write a blob"));
            }
            Work::Worker(Box::new(move |s, _| answer(s.caps.write(&path, &bytes).map(|_| vec![]))))
        }
        "files/write_blob" => {
            let parsed = b.str().and_then(|p| Ok((p, b.u32()?))).and_then(|v| b.end().map(|_| v));
            let (path, id) = match parsed {
                Ok(v) => v,
                Err(why) => return Work::Now(err(&why)),
            };
            Work::Worker(Box::new(move |s, _| {
                // The bytes are shared, not copied, and written once the table's lock is let go.
                let bytes = match s.blobs().get(id) {
                    Ok(b) => b,
                    Err(why) => return err(&why),
                };
                answer(
                    s.caps
                        .write_with(&path, |f| {
                            std::thread::sleep(s.io_delay);
                            f.write_all(&bytes)
                        })
                        .map(|_| vec![]),
                )
            }))
        }
        "blob/new" => match b.end() {
            Ok(()) => Work::Worker(Box::new(|s, request| {
                answer(s.blobs().create(request).map(|id| id.to_le_bytes().to_vec()))
            })),
            Err(why) => Work::Now(err(&why)),
        },
        "blob/append" => {
            let id = match b.u32() {
                Ok(id) => id,
                Err(why) => return Work::Now(err(&why)),
            };
            let bytes = b.rest().to_vec();
            if bytes.len() > MOST_BYTES {
                return Work::Now(err("more than 512 KiB at once"));
            }
            Work::Worker(Box::new(move |s, _| answer(s.blobs().append(id, &bytes).map(|_| vec![]))))
        }
        "blob/read" => {
            let parsed = (|| Ok::<_, String>((b.u32()?, b.u64()?, b.u32()?)))();
            match parsed.and_then(|v| b.end().map(|_| v)) {
                Ok((_, _, len)) if len as usize > MOST_BYTES => {
                    Work::Now(err("more than 512 KiB at once"))
                }
                Ok((id, offset, len)) => Work::Worker(Box::new(move |s, _| {
                    answer(s.blobs().read(id, offset, len as usize))
                })),
                Err(why) => Work::Now(err(&why)),
            }
        }
        "blob/len" | "blob/release" => {
            let id = match b.u32().and_then(|id| b.end().map(|_| id)) {
                Ok(id) => id,
                Err(why) => return Work::Now(err(&why)),
            };
            if name == "blob/len" {
                Work::Worker(Box::new(move |s, _| {
                    answer(s.blobs().get(id).map(|b| (b.len() as u64).to_le_bytes().to_vec()))
                }))
            } else {
                Work::Worker(Box::new(move |s, _| answer(s.blobs().release(id).map(|_| vec![]))))
            }
        }
        _ => Work::Now(Answer::Failed),
    }
}

/// A change to a fake site's URL, as std's `testing::fake` writes it: the URL, then what to
/// change: 0 a status (a `u32`), 1 missing, 2 bytes (the rest), each in place of the one set
/// before; 3 a delay (an `f64`'s bits); 4 nothing faked.
#[allow(clippy::type_complexity)]
fn testing_fake(b: &mut Body) -> Result<(String, Box<dyn FnOnce(&mut Fake)>), String> {
    let url = b.str()?;
    super::http::target(&url)?;
    let change: Box<dyn FnOnce(&mut Fake)> = match b.u8()? {
        0 => {
            let status = b.u32()?;
            if !(100..=999).contains(&status) {
                return Err(format!("{status} isn't a status: 100 to 999"));
            }
            Box::new(move |f| f.answer = Some(Faked::Status(status as u16)))
        }
        1 => Box::new(|f| f.answer = Some(Faked::Missing)),
        2 => {
            let bytes = b.rest().to_vec();
            if bytes.len() > MOST_BYTES {
                return Err("a fake's bytes are at most 512 KiB".into());
            }
            Box::new(move |f| f.answer = Some(Faked::Bytes(bytes)))
        }
        3 => {
            let delay = f64::from_bits(b.u64()?);
            if !(0.0..=super::http::LONGEST).contains(&delay) {
                return Err(format!("a delay is 0 to {} seconds", super::http::LONGEST));
            }
            Box::new(move |f| f.delay = delay)
        }
        4 => Box::new(|f| *f = Fake::default()),
        k => return Err(format!("{k} isn't a fake")),
    };
    b.end()?;
    Ok((url, change))
}

/// An entry as `files::read_list` and `read_entry` read it: (the name), the kind, the size.
fn write_entry(out: &mut Vec<u8>, e: &Entry, named: bool) {
    if named {
        out.extend_from_slice(&(e.name.len() as u32).to_le_bytes());
        out.extend_from_slice(e.name.as_bytes());
    }
    out.push(e.kind as u8);
    out.extend_from_slice(&e.size.to_le_bytes());
}

/// The file services that take a path alone.
fn files(s: &Services, request: u32, name: &str, path: &str) -> Answer {
    let c = &s.caps;
    answer(match name {
        "files/read" => c.read(path),
        "files/list" => c.list(path).map(|entries| {
            let mut out = (entries.len() as u32).to_le_bytes().to_vec();
            for e in &entries {
                write_entry(&mut out, e, true);
            }
            out
        }),
        "files/stat" => c.stat(path).map(|e| {
            let mut out = Vec::new();
            write_entry(&mut out, &e, false);
            out
        }),
        "files/remove" => c.remove(path).map(|_| vec![]),
        "files/remove_all" => c.remove_all(path).map(|_| vec![]),
        "files/make_dirs" => c.make_dirs(path).map(|_| vec![]),
        "files/read_blob" => read_blob(s, request, path),
        _ => unreachable!("routed only the file services"),
    })
}

/// The file at `path` as a new blob, made for `request`: its handle. A file that doesn't fit the
/// budget is refused before it's read; one that fails part way releases its blob.
fn read_blob(s: &Services, request: u32, path: &str) -> Result<Vec<u8>, String> {
    let id = s.blobs().create(request)?;
    let read =
        s.caps.read_chunks(path, |size| s.blobs().fits(size), |chunk| s.blobs().append(id, chunk));
    match read {
        Ok(()) => Ok(id.to_le_bytes().to_vec()),
        Err(why) => {
            let _ = s.blobs().release(id);
            Err(why)
        }
    }
}
