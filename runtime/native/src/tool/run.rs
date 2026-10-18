//! `std::tool::run` (language.md §15.5): a program the grants name (`--allow-run`), run as a
//! process of its own, with its arguments, its standard input, and what it writes to its
//! standard output and error given back. No shell runs: the program is the executable found
//! when the grants were opened.
//!
//! **What it gets.** An empty environment, then a fixed baseline: `PATH` as it was when the
//! grants were opened, `HOME` a directory the host owns for this run, `LANG=C.UTF-8`; then the
//! request's variables, and the granted secrets it names, by name. It works in a write root the
//! request names. A program runs with the user's authority, natively: the working directory is
//! where it starts, not a confinement.
//!
//! **Its process group is the host's.** The program leads a process group of its own, and the
//! host kills that group (`SIGKILL`) when the program ends, so nothing it started outlives it;
//! past the request's deadline; past [`MOST_OUTPUT`] bytes of output; and when the tool ends.
//! The worker that runs it looks for the tool's end every few milliseconds, so the host's grace
//! is never spent waiting for a program. Its input and output are files in the host's own
//! directory for the run, so no pipe a program holds open can hold up the host.
//!
//! A `tool:` program is a tool package, which `wrela tool` runs on the tool host in its process,
//! as any program, with the request's working directory as its one write root (under the same
//! name), its arguments, the secrets the request names, and the request's deadline as its
//! `--timeout`.

use super::caps::Program;
use super::services::{Body, Data, Services};
use std::path::Path;
use std::process::Command;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

/// A program's deadline unless the request asks for another.
const DEADLINE: Duration = Duration::from_secs(60);

/// The most bytes a program may write to its standard output, and to its standard error.
pub(crate) const MOST_OUTPUT: u64 = 256 << 20;

/// How often the worker looks at the program.
const POLL: Duration = Duration::from_millis(10);

/// How long the worker waits for a killed program's exit to be reported.
const REAP: Duration = Duration::from_secs(1);

/// A request to run a program, as std's `run::run` writes it.
pub(crate) struct Run {
    program: String,
    args: Vec<String>,
    stdin: Data,
    env: Vec<(String, String)>,
    secrets: Vec<String>,
    cwd: String,
    deadline: Duration,
}

/// Reads a request: the program, its arguments, its standard input, its variables, the secrets
/// it's given, the write root it works in, and its deadline.
pub(crate) fn parse(b: &mut Body) -> Result<Run, String> {
    let program = b.str()?;
    let mut args = Vec::new();
    for _ in 0..b.u32()? {
        args.push(b.str()?);
    }
    let stdin = b.data()?;
    let mut env = Vec::new();
    for _ in 0..b.u32()? {
        env.push((b.str()?, b.str()?));
    }
    let mut secrets = Vec::new();
    for _ in 0..b.u32()? {
        secrets.push(b.str()?);
    }
    let cwd = b.str()?;
    let deadline = super::http::deadline(f64::from_bits(b.u64()?), DEADLINE)?;
    b.end()?;
    for (name, _) in &env {
        if name.is_empty() || name.contains(['=', '\0']) {
            return Err(format!("`{name}` can't name an environment variable"));
        }
    }
    if args.iter().chain(env.iter().map(|(_, v)| v)).any(|a| a.contains('\0')) {
        return Err("an argument or a variable holds a NUL byte".into());
    }
    Ok(Run { program, args, stdin, env, secrets, cwd, deadline })
}

/// Kills process group `group`, whatever is left of it: a group already gone is no error.
fn kill_group(group: i32) {
    if let Some(pid) = rustix::process::Pid::from_raw(group) {
        let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
    }
}

/// Kills `child`'s process group, and waits up to [`REAP`] for its exit to be reported.
fn stop(child: &mut std::process::Child, group: i32) {
    kill_group(group);
    let since = Instant::now();
    while since.elapsed() < REAP {
        if !matches!(child.try_wait(), Ok(None)) {
            return;
        }
        std::thread::sleep(POLL);
    }
}

/// The size of the file at `path`, 0 if it can't be read.
fn size(path: &Path) -> u64 {
    std::fs::metadata(path).map_or(0, |m| m.len())
}

/// Carries out `r` for `request`: the program's status (its exit code, or minus the signal that
/// ended it), then its standard output and standard error, each bytes or a blob.
pub(crate) fn run(s: &Services, request: u32, r: Run) -> Result<Vec<u8>, String> {
    let program = s.caps.program(&r.program)?.clone();
    let cwd = s.caps.write_root_dir(&r.cwd)?.to_path_buf();
    let mut secrets = Vec::new();
    for name in &r.secrets {
        let Some(value) = s.caps.secret(name) else {
            return Err(format!("the secret `{name}` isn't granted: `--secret {name}` grants it"));
        };
        secrets.push((name.clone(), value.to_string()));
    }
    let dir = s.work_dir()?;
    let (input, output, errors) = (
        dir.join(format!("{request}.in")),
        dir.join(format!("{request}.out")),
        dir.join(format!("{request}.err")),
    );
    let stdin = match &r.stdin {
        Data::Bytes(b) => std::fs::write(&input, b),
        Data::Blob(id) => {
            let bytes = s.blobs().get(*id)?;
            std::fs::write(&input, bytes.as_slice())
        }
    };
    stdin.map_err(|e| format!("the program's input: {e}"))?;
    let opened = |path: &Path, create: bool| {
        let f = if create { std::fs::File::create(path) } else { std::fs::File::open(path) };
        f.map_err(|e| format!("the program's input or output: {e}"))
    };
    let mut command = match &program {
        Program::Native(path) => {
            let mut c = Command::new(path);
            c.args(&r.args);
            c
        }
        Program::Tool { runner, package } => {
            let mut c = Command::new(runner);
            c.arg("tool").arg(package);
            c.arg("--write").arg(format!("{}={}", r.cwd, cwd.display()));
            c.arg("--timeout").arg(format!("{}", r.deadline.as_secs_f64()));
            for (name, _) in &secrets {
                c.arg("--secret").arg(name);
            }
            c.arg("--").args(&r.args);
            c
        }
    };
    command.env_clear();
    command.env("PATH", s.caps.path_baseline());
    command.env("HOME", dir.join("home"));
    command.env("LANG", "C.UTF-8");
    for (name, value) in r.env.iter().chain(&secrets) {
        command.env(name, value);
    }
    command
        .current_dir(&cwd)
        .stdin(opened(&input, false)?)
        .stdout(opened(&output, true)?)
        .stderr(opened(&errors, true)?);
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    let mut child = command.spawn().map_err(|e| format!("can't run `{}`: {e}", r.program))?;
    let group = child.id() as i32;
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                // What it started and left running goes with it.
                kill_group(group);
                break status;
            }
            Ok(None) => {}
            Err(e) => {
                stop(&mut child, group);
                return Err(format!("waiting for `{}`: {e}", r.program));
            }
        }
        if s.closing.load(Ordering::Acquire) {
            stop(&mut child, group);
            return Err(format!("the tool ended before `{}` did", r.program));
        }
        if started.elapsed() > r.deadline {
            stop(&mut child, group);
            return Err(format!(
                "`{}` ran past its deadline of {} s, and the host killed its process group",
                r.program,
                r.deadline.as_secs_f64()
            ));
        }
        if size(&output) > MOST_OUTPUT || size(&errors) > MOST_OUTPUT {
            stop(&mut child, group);
            return Err(format!(
                "`{}` wrote more than {MOST_OUTPUT} bytes of output, and the host killed its process group",
                r.program
            ));
        }
        std::thread::sleep(POLL);
    };
    let code = match status.code() {
        Some(c) => c,
        None => {
            #[cfg(unix)]
            {
                -std::os::unix::process::ExitStatusExt::signal(&status).unwrap_or(0)
            }
            #[cfg(not(unix))]
            {
                -1
            }
        }
    };
    let mut out = (code as u32).to_le_bytes().to_vec();
    for path in [&output, &errors] {
        let bytes = std::fs::read(path).map_err(|e| format!("the program's output: {e}"))?;
        if bytes.len() as u64 > MOST_OUTPUT {
            return Err(format!("`{}` wrote more than {MOST_OUTPUT} bytes of output", r.program));
        }
        s.put_data(request, &mut out, bytes)?;
    }
    for path in [&input, &output, &errors] {
        let _ = std::fs::remove_file(path);
    }
    Ok(out)
}
