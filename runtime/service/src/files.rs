//! Files under the directories a service was given (`--root NAME=DIR`): `fs/...` requests.
//!
//! A path is a root's name, then a relative path inside it, with `/` between parts: `data` is the
//! root itself, `data/objects/ab` a directory in it. No part is empty, `.` or `..`, and a path
//! that passes through a symbolic link is refused, so a request can't reach outside its root.
//!
//! What each request's bytes hold (`crate::body`):
//!
//! | Service | Request | Answer |
//! |---|---|---|
//! | `fs/read` | path; optionally `u64` offset and `u64` length (`u64::MAX`: to the end) | the bytes |
//! | `fs/write` | path, bytes, optionally `u8` flags (1: make it executable; 2: don't flush) | nothing: the file holds exactly the bytes, on disk |
//! | `fs/append` | path, bytes | the file's new length, `u64`, on disk |
//! | `fs/truncate` | path, `u64` length | nothing |
//! | `fs/stat` | path | kind `u8` (0 none, 1 file, 2 directory, 3 executable file), size `u64`, modified `u64` ms |
//! | `fs/list` | path | `u32` count, then each entry's kind `u8`, name and size `u64`, by name |
//! | `fs/mkdir` | path | nothing: it and its parents exist |
//! | `fs/remove` | path, `u8` 1 to remove a directory and what it holds | nothing |
//! | `fs/rename` | path, path, in one root | nothing: the rename is on disk |
//! | `fs/lock` | path | nothing: the service holds the file's lock, alone, until it ends |
//!
//! "On disk" means flushed (`fsync`, a full flush on macOS) before the answer: a write goes to a
//! file beside its target, which is flushed and renamed into place, and the directory is flushed;
//! an append flushes the file. So an answered write survives a crash, and a write a crash cut
//! short leaves the old file whole. A write asked not to flush is renamed into place unflushed:
//! whole or old after a crash of the service, but not after a crash of the machine.
//!
//! The host carries out a call's files requests in the order the call made them, each before the
//! next starts.

use crate::body::{Body, Out};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::UNIX_EPOCH;

/// The directories a service may use, by name.
#[derive(Clone, Debug, Default)]
pub struct Roots {
    roots: BTreeMap<String, PathBuf>,
    /// The files whose locks the service holds (`fs/lock`): released when the roots are dropped,
    /// as the service ends.
    locks: std::sync::Arc<std::sync::Mutex<Vec<File>>>,
}

/// Whether `name` names a root: 1 to 64 of a-z, 0-9, `-` and `_`.
pub fn root_name(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

impl Roots {
    /// Adds root `name`, the directory `dir`, which must exist.
    pub fn add(&mut self, name: &str, dir: &Path) -> Result<(), String> {
        if !root_name(name) {
            return Err(format!("`{name}` isn't a root's name: a-z, 0-9, `-` and `_`"));
        }
        let dir =
            dir.canonicalize().map_err(|e| format!("root `{name}`, {}: {e}", dir.display()))?;
        if !dir.is_dir() {
            return Err(format!("root `{name}`, {}, isn't a directory", dir.display()));
        }
        self.roots.insert(name.to_string(), dir);
        Ok(())
    }

    /// The file a path names: in its root, through no symbolic link.
    pub fn resolve(&self, path: &str) -> Result<PathBuf, String> {
        let mut parts = path.split('/');
        let name = parts.next().unwrap_or_default();
        let Some(root) = self.roots.get(name) else {
            return Err(format!("`{path}` names no root of this service"));
        };
        let mut at = root.clone();
        for part in parts {
            if part.is_empty() || part == "." || part == ".." || part.contains(['\\', '\0']) {
                return Err(format!("`{path}` has an empty, `.` or `..` part"));
            }
            at.push(part);
            if fs::symlink_metadata(&at).is_ok_and(|m| m.file_type().is_symlink()) {
                return Err(format!("`{path}` passes through a symbolic link"));
            }
        }
        Ok(at)
    }

    /// The name of the root that holds path `path`, and the rest of it.
    pub fn root_of(path: &str) -> &str {
        path.split('/').next().unwrap_or_default()
    }
}

fn io(path: &Path, e: std::io::Error) -> String {
    format!("{}: {e}", path.display())
}

/// Flushes directory `dir`, so a rename or a new file in it survives a crash.
fn sync_dir(dir: &Path) -> Result<(), String> {
    File::open(dir).and_then(|d| d.sync_all()).map_err(|e| io(dir, e))
}

/// A name for a file beside `path` that no other write uses.
fn beside(path: &Path) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    path.with_file_name(format!(".{name}.{}-{n}.part", std::process::id()))
}

/// Replaces `path`'s bytes with `bytes`, on disk: written beside it, flushed, renamed into
/// place, and its directory flushed. `exec` makes it executable.
pub fn write_atomically(path: &Path, bytes: &[u8], exec: bool) -> Result<(), String> {
    write_file(path, bytes, exec, true)
}

/// Replaces `path`'s bytes with `bytes`, as [`write_atomically`] does; flushed only if `flush`.
fn write_file(path: &Path, bytes: &[u8], exec: bool, flush: bool) -> Result<(), String> {
    write_via(path, &beside(path), bytes, exec, flush)
}

/// [`write_file`], through the file `part` beside it, which must not be there yet.
fn write_via(path: &Path, part: &Path, bytes: &[u8], exec: bool, flush: bool) -> Result<(), String> {
    let part = part.to_path_buf();
    let written = (|| {
        // A new file, never one already there: not a link someone put in its place.
        let mut f = OpenOptions::new().write(true).create_new(true).open(&part)?;
        f.write_all(bytes)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            f.set_permissions(fs::Permissions::from_mode(if exec { 0o755 } else { 0o644 }))?;
        }
        if flush {
            f.sync_all()?;
        }
        fs::rename(&part, path)
    })();
    if let Err(e) = written {
        let _ = fs::remove_file(&part);
        return Err(io(path, e));
    }
    if flush { sync_dir(path.parent().unwrap_or(Path::new("."))) } else { Ok(()) }
}

fn millis(t: std::io::Result<std::time::SystemTime>) -> u64 {
    t.ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_millis() as u64)
}

fn kind(m: &fs::Metadata) -> u8 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if m.is_file() && m.permissions().mode() & 0o111 != 0 {
            return 3;
        }
    }
    if m.is_dir() { 2 } else { 1 }
}

/// Carries out files request `service` on `roots`: `None` if it isn't one.
pub fn serve(roots: &Roots, service: &str, body: &[u8]) -> Option<Result<Vec<u8>, String>> {
    let op = service.strip_prefix("fs/")?;
    let mut b = Body::new(body);
    Some((|| -> Result<Vec<u8>, String> {
        let path = roots.resolve(b.str()?)?;
        match op {
            "read" => {
                let (offset, len) = if b.is_empty() { (0, u64::MAX) } else { (b.u64()?, b.u64()?) };
                b.end()?;
                let mut f = File::open(&path).map_err(|e| io(&path, e))?;
                let mut out = Vec::new();
                f.seek(SeekFrom::Start(offset)).map_err(|e| io(&path, e))?;
                f.take(len).read_to_end(&mut out).map_err(|e| io(&path, e))?;
                Ok(out)
            }
            "write" => {
                let bytes = b.bytes()?;
                let flags = if b.is_empty() { 0 } else { b.u8()? };
                b.end()?;
                write_file(&path, bytes, flags & 1 != 0, flags & 2 == 0)?;
                Ok(Vec::new())
            }
            "append" => {
                let bytes = b.bytes()?;
                b.end()?;
                let new = !path.exists();
                let mut f = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
                    .map_err(|e| io(&path, e))?;
                f.write_all(bytes).and_then(|()| f.sync_all()).map_err(|e| io(&path, e))?;
                if new {
                    sync_dir(path.parent().unwrap_or(Path::new(".")))?;
                }
                let len = f.metadata().map_err(|e| io(&path, e))?.len();
                Ok(Out::default().u64(len).0)
            }
            "truncate" => {
                let len = b.u64()?;
                b.end()?;
                let f = OpenOptions::new().write(true).open(&path).map_err(|e| io(&path, e))?;
                f.set_len(len).and_then(|()| f.sync_all()).map_err(|e| io(&path, e))?;
                Ok(Vec::new())
            }
            "stat" => {
                b.end()?;
                Ok(match fs::metadata(&path) {
                    Ok(m) => Out::default().u8(kind(&m)).u64(m.len()).u64(millis(m.modified())).0,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        Out::default().u8(0).u64(0).u64(0).0
                    }
                    Err(e) => return Err(io(&path, e)),
                })
            }
            "list" => {
                b.end()?;
                let mut entries = Vec::new();
                for e in fs::read_dir(&path).map_err(|e| io(&path, e))? {
                    let e = e.map_err(|e| io(&path, e))?;
                    let name = e.file_name().to_string_lossy().into_owned();
                    // A write's file beside its target isn't one of the directory's own.
                    if name.starts_with('.') && name.ends_with(".part") {
                        continue;
                    }
                    let m = e.metadata().map_err(|e| io(&path, e))?;
                    entries.push((name, kind(&m), m.len()));
                }
                entries.sort();
                let mut out = Out::default().u32(entries.len() as u32);
                for (name, kind, size) in entries {
                    out = out.u8(kind).str(&name).u64(size);
                }
                Ok(out.0)
            }
            "mkdir" => {
                b.end()?;
                fs::create_dir_all(&path).map_err(|e| io(&path, e))?;
                sync_dir(path.parent().unwrap_or(Path::new(".")))?;
                Ok(Vec::new())
            }
            "remove" => {
                let all = b.u8()? == 1;
                b.end()?;
                let removed = match fs::symlink_metadata(&path) {
                    Ok(m) if m.is_dir() && all => fs::remove_dir_all(&path),
                    Ok(m) if m.is_dir() => fs::remove_dir(&path),
                    Ok(_) => fs::remove_file(&path),
                    Err(e) => Err(e),
                };
                removed.map_err(|e| io(&path, e))?;
                sync_dir(path.parent().unwrap_or(Path::new(".")))?;
                Ok(Vec::new())
            }
            "rename" => {
                let to_path = b.str()?;
                b.end()?;
                let to = roots.resolve(to_path)?;
                fs::rename(&path, &to).map_err(|e| io(&path, e))?;
                sync_dir(to.parent().unwrap_or(Path::new(".")))?;
                if path.parent() != to.parent() {
                    sync_dir(path.parent().unwrap_or(Path::new(".")))?;
                }
                Ok(Vec::new())
            }
            "lock" => {
                b.end()?;
                let f = OpenOptions::new()
                    .create(true)
                    .truncate(false)
                    .write(true)
                    .open(&path)
                    .map_err(|e| io(&path, e))?;
                match f.try_lock() {
                    Ok(()) => {
                        roots.locks.lock().expect("the locks' lock").push(f);
                        Ok(Vec::new())
                    }
                    Err(std::fs::TryLockError::WouldBlock) => {
                        Err(format!("{}: another program holds its lock", path.display()))
                    }
                    Err(std::fs::TryLockError::Error(e)) => Err(io(&path, e)),
                }
            }
            _ => Err(format!("`{service}` isn't a files service")),
        }
    })())
}

#[cfg(test)]
mod tests {
    use super::{Roots, serve};
    use crate::body::{Body, Out};

    fn roots() -> (Roots, tempdir::Dir) {
        let dir = tempdir::Dir::new();
        let mut roots = Roots::default();
        roots.add("data", dir.path()).expect("a root");
        (roots, dir)
    }

    mod tempdir {
        /// A directory removed when it's dropped.
        pub struct Dir(std::path::PathBuf);

        impl Dir {
            pub fn new() -> Dir {
                use std::sync::atomic::{AtomicU32, Ordering};
                static N: AtomicU32 = AtomicU32::new(0);
                let n = N.fetch_add(1, Ordering::Relaxed);
                let p = std::env::temp_dir()
                    .join(format!("wrela-service-test-{}-{n}", std::process::id()));
                let _ = std::fs::remove_dir_all(&p);
                std::fs::create_dir_all(&p).expect("a scratch directory");
                Dir(p)
            }

            pub fn path(&self) -> &std::path::Path {
                &self.0
            }
        }

        impl Drop for Dir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }

    fn call(roots: &Roots, service: &str, body: Out) -> Result<Vec<u8>, String> {
        serve(roots, service, &body.0).expect("a files service")
    }

    #[test]
    fn a_file_is_written_appended_read_and_listed() {
        let (r, _dir) = roots();
        call(&r, "fs/mkdir", Out::default().str("data/a/b")).expect("mkdir");
        call(&r, "fs/write", Out::default().str("data/a/b/f").bytes(b"hello")).expect("write");
        let len = call(&r, "fs/append", Out::default().str("data/a/b/f").bytes(b" world"));
        assert_eq!(Body::new(&len.expect("append")).u64(), Ok(11));
        let all = call(&r, "fs/read", Out::default().str("data/a/b/f")).expect("read");
        assert_eq!(all, b"hello world");
        let part = call(&r, "fs/read", Out::default().str("data/a/b/f").u64(6).u64(3));
        assert_eq!(part.expect("a range"), b"wor");
        let listed = call(&r, "fs/list", Out::default().str("data/a/b")).expect("list");
        let mut b = Body::new(&listed);
        assert_eq!((b.u32(), b.u8(), b.str(), b.u64()), (Ok(1), Ok(1), Ok("f"), Ok(11)));
        call(&r, "fs/truncate", Out::default().str("data/a/b/f").u64(5)).expect("truncate");
        assert_eq!(call(&r, "fs/read", Out::default().str("data/a/b/f")).expect("read"), b"hello");
    }

    #[test]
    #[cfg(unix)]
    fn a_file_written_executable_says_so() {
        let (r, _dir) = roots();
        call(&r, "fs/write", Out::default().str("data/x").bytes(b"#!/bin/sh").u8(1))
            .expect("write");
        let st = call(&r, "fs/stat", Out::default().str("data/x")).expect("stat");
        assert_eq!(Body::new(&st).u8(), Ok(3));
        call(&r, "fs/write", Out::default().str("data/x").bytes(b"plain")).expect("write");
        let st = call(&r, "fs/stat", Out::default().str("data/x")).expect("stat");
        assert_eq!(Body::new(&st).u8(), Ok(1));
    }

    #[test]
    fn stat_says_what_a_path_is() {
        let (r, _dir) = roots();
        let none = call(&r, "fs/stat", Out::default().str("data/missing")).expect("stat");
        assert_eq!(Body::new(&none).u8(), Ok(0));
        let dir = call(&r, "fs/stat", Out::default().str("data")).expect("stat");
        assert_eq!(Body::new(&dir).u8(), Ok(2));
    }

    #[test]
    fn a_rename_and_a_remove_take_effect() {
        let (r, _dir) = roots();
        call(&r, "fs/write", Out::default().str("data/x").bytes(b"1")).expect("write");
        call(&r, "fs/rename", Out::default().str("data/x").str("data/y")).expect("rename");
        assert!(call(&r, "fs/read", Out::default().str("data/x")).is_err());
        call(&r, "fs/remove", Out::default().str("data/y").u8(0)).expect("remove");
        assert!(call(&r, "fs/read", Out::default().str("data/y")).is_err());
    }

    #[test]
    fn a_path_cant_leave_its_root() {
        let (r, dir) = roots();
        for bad in ["other/x", "data/../x", "data//x", "data/./x"] {
            assert!(r.resolve(bad).is_err(), "{bad}");
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("/", dir.path().join("out")).expect("a link");
            assert!(r.resolve("data/out/etc").is_err());
        }
    }

    #[test]
    fn a_lock_is_held_by_one_service_at_a_time() {
        let (r, _dir) = roots();
        call(&r, "fs/lock", Out::default().str("data/lock")).expect("the first holds it");
        let again = Roots { roots: r.roots.clone(), locks: Default::default() };
        let refused = call(&again, "fs/lock", Out::default().str("data/lock"));
        assert!(refused.is_err_and(|e| e.contains("holds its lock")), "a second is refused");
        drop(r);
        call(&again, "fs/lock", Out::default().str("data/lock")).expect("free once the first ends");
    }

    #[test]
    fn a_write_never_follows_a_link_put_where_its_file_beside_goes() {
        let (_r, dir) = roots();
        let outside = dir.path().join("outside");
        std::fs::write(&outside, b"keep").expect("a file outside");
        let part = dir.path().join(".f.part");
        std::os::unix::fs::symlink(&outside, &part).expect("a link");
        let target = dir.path().join("f");
        assert!(super::write_via(&target, &part, b"new", false, true).is_err(), "refused");
        assert_eq!(std::fs::read(&outside).expect("still there"), b"keep");
    }

    #[test]
    fn a_failed_write_leaves_no_file_beside_its_target() {
        let (r, dir) = roots();
        // The target's directory doesn't exist: the write fails, and nothing is left.
        assert!(call(&r, "fs/write", Out::default().str("data/no/f").bytes(b"x")).is_err());
        assert_eq!(std::fs::read_dir(dir.path()).expect("a dir").count(), 0);
    }
}
