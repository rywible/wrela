//! Processes a service runs (`proc/run`) and stops (`proc/kill`).
//!
//! `proc/run`'s request: a `u64` tag that the program chooses (a kill names it), a `u32` count
//! and that many arguments (the first is the program, found on `PATH`), the working directory (a
//! files path, `crate::files`), a `u32` count and that many name and value pairs added to the
//! service's environment, a `u64` limit in milliseconds (0: none), a `u32` cap on the bytes of
//! output kept from each of stdout and stderr, and the bytes given on stdin.
//!
//! Its answer, once the process ends: its exit status (`i32`; -1 if a signal ended it), a `u8` of
//! flags (1: it ran past its limit; 2: a kill stopped it; 4: output past the cap was dropped), the
//! milliseconds it ran (`u64`), then its stdout and its stderr.
//!
//! A process runs in a process group of its own, and a limit or a kill stops the whole group, so
//! a test runner's children stop with it.

use crate::body::{Body, Out};
use crate::files::Roots;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// The processes running, by tag: what a kill finds.
#[derive(Clone, Default)]
pub struct Running {
    pids: Arc<Mutex<HashMap<u64, u32>>>,
    killed: Arc<Mutex<std::collections::HashSet<u64>>>,
}

/// A process to run, read from a `proc/run` request.
pub struct Run {
    pub tag: u64,
    pub argv: Vec<String>,
    pub cwd: std::path::PathBuf,
    pub env: Vec<(String, String)>,
    pub limit: Option<Duration>,
    pub cap: usize,
    pub stdin: Vec<u8>,
}

impl Run {
    pub fn parse(roots: &Roots, body: &[u8]) -> Result<Run, String> {
        let mut b = Body::new(body);
        let tag = b.u64()?;
        let n = b.u32()?;
        let mut argv = Vec::new();
        for _ in 0..n {
            argv.push(b.str()?.to_string());
        }
        if argv.is_empty() {
            return Err("`proc/run` needs a program to run".into());
        }
        let cwd = roots.resolve(b.str()?)?;
        let n = b.u32()?;
        let mut env = Vec::new();
        for _ in 0..n {
            env.push((b.str()?.to_string(), b.str()?.to_string()));
        }
        let ms = b.u64()?;
        let cap = b.u32()? as usize;
        let stdin = b.rest().to_vec();
        Ok(Run {
            tag,
            argv,
            cwd,
            env,
            limit: (ms > 0).then(|| Duration::from_millis(ms)),
            cap,
            stdin,
        })
    }
}

/// Stops process group `pid`.
fn stop_group(pid: u32) {
    let _ = Command::new("kill")
        .args(["-KILL", &format!("-{pid}")])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// Reads all of `r` on a thread of its own, keeping at most `cap` bytes: the bytes, and whether
/// any were dropped.
fn drain(mut r: impl Read + Send + 'static, cap: usize) -> thread::JoinHandle<(Vec<u8>, bool)> {
    thread::spawn(move || {
        let (mut kept, mut dropped, mut buf) = (Vec::new(), false, vec![0u8; 64 << 10]);
        loop {
            match r.read(&mut buf) {
                Ok(0) | Err(_) => return (kept, dropped),
                Ok(n) => {
                    let room = cap.saturating_sub(kept.len());
                    kept.extend_from_slice(&buf[..n.min(room)]);
                    dropped |= n > room;
                }
            }
        }
    })
}

impl Running {
    /// Runs `run` on a thread of its own; `done` gets its answer when it ends.
    pub fn start(&self, run: Run, done: impl FnOnce(Result<Vec<u8>, String>) + Send + 'static) {
        let running = self.clone();
        thread::spawn(move || {
            let answer = running.run(&run);
            running.pids.lock().expect("the processes' lock").remove(&run.tag);
            done(answer);
        });
    }

    fn run(&self, run: &Run) -> Result<Vec<u8>, String> {
        let mut cmd = Command::new(&run.argv[0]);
        cmd.args(&run.argv[1..])
            .current_dir(&run.cwd)
            .envs(run.env.iter().map(|(k, v)| (k, v)))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }
        let started = Instant::now();
        let mut child: Child = cmd.spawn().map_err(|e| format!("{}: {e}", run.argv[0]))?;
        self.pids.lock().expect("the processes' lock").insert(run.tag, child.id());
        let killed_early = self.killed.lock().expect("the kills' lock").remove(&run.tag);
        if killed_early {
            stop_group(child.id());
        }
        let mut stdin = child.stdin.take().expect("a piped stdin");
        let input = run.stdin.clone();
        let feeding = thread::spawn(move || {
            let _ = stdin.write_all(&input);
        });
        let out = drain(child.stdout.take().expect("a piped stdout"), run.cap);
        let err = drain(child.stderr.take().expect("a piped stderr"), run.cap);
        let mut flags = 0u8;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => {}
                Err(e) => return Err(format!("{}: {e}", run.argv[0])),
            }
            if run.limit.is_some_and(|l| started.elapsed() > l) {
                flags |= 1;
                stop_group(child.id());
                break child.wait().map_err(|e| format!("{}: {e}", run.argv[0]))?;
            }
            thread::sleep(Duration::from_millis(5));
        };
        if killed_early || self.killed.lock().expect("the kills' lock").remove(&run.tag) {
            flags |= 2;
        }
        // The group may hold children that outlive the process: they stop with it.
        stop_group(child.id());
        let _ = feeding.join();
        let (stdout, d1) = out.join().unwrap_or_default();
        let (stderr, d2) = err.join().unwrap_or_default();
        if d1 || d2 {
            flags |= 4;
        }
        Ok(Out::default()
            .i32(status.code().unwrap_or(-1))
            .u8(flags)
            .u64(started.elapsed().as_millis() as u64)
            .bytes(&stdout)
            .bytes(&stderr)
            .0)
    }

    /// Stops the process tagged `tag` and its group: now if it runs, or as it starts.
    pub fn kill(&self, tag: u64) {
        self.killed.lock().expect("the kills' lock").insert(tag);
        if let Some(&pid) = self.pids.lock().expect("the processes' lock").get(&tag) {
            stop_group(pid);
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::{Run, Running};
    use crate::body::Body;
    use std::sync::mpsc;
    use std::time::Duration;

    fn run(argv: &[&str], limit: Option<Duration>, stdin: &[u8]) -> Run {
        Run {
            tag: 1,
            argv: argv.iter().map(|s| s.to_string()).collect(),
            cwd: std::env::temp_dir(),
            env: vec![("FORGE_TEST".into(), "yes".into())],
            limit,
            cap: 16,
            stdin: stdin.to_vec(),
        }
    }

    fn answer(r: &Running, run: Run) -> (i32, u8, Vec<u8>) {
        let (tx, rx) = mpsc::channel();
        r.start(run, move |a| tx.send(a).expect("the test waits"));
        let a = rx.recv_timeout(Duration::from_secs(20)).expect("an answer").expect("it ran");
        let mut b = Body::new(&a);
        let (status, flags, _ms) =
            (b.u32().expect("a status") as i32, b.u8().expect("flags"), b.u64());
        (status, flags, b.bytes().expect("stdout").to_vec())
    }

    #[test]
    fn a_process_gets_its_input_environment_and_status() {
        let r = Running::default();
        let (status, flags, out) =
            answer(&r, run(&["sh", "-c", "read x; echo $x $FORGE_TEST; exit 3"], None, b"hi\n"));
        assert_eq!((status, flags, out), (3, 0, b"hi yes\n".to_vec()));
    }

    #[test]
    fn output_past_the_cap_is_dropped_and_said() {
        let r = Running::default();
        let (status, flags, out) = answer(&r, run(&["sh", "-c", "yes | head -c 1000"], None, b""));
        assert_eq!((status, flags & 4, out.len()), (0, 4, 16));
    }

    #[test]
    fn a_limit_stops_the_whole_group() {
        let r = Running::default();
        let (_, flags, _) = answer(
            &r,
            run(&["sh", "-c", "sleep 30 & sleep 30"], Some(Duration::from_millis(200)), b""),
        );
        assert_eq!(flags & 1, 1);
    }

    #[test]
    fn a_kill_stops_a_process() {
        let r = Running::default();
        let (tx, rx) = mpsc::channel();
        r.start(run(&["sleep", "30"], None, b""), move |a| tx.send(a).expect("the test waits"));
        std::thread::sleep(Duration::from_millis(100));
        r.kill(1);
        let a = rx.recv_timeout(Duration::from_secs(10)).expect("an answer").expect("it ran");
        let mut b = Body::new(&a);
        let _ = b.u32();
        assert_eq!(b.u8().map(|f| f & 2), Ok(2));
    }
}
