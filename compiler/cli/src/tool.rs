//! `wrela tool <package-dir> [--read NAME=DIR]... [--write NAME=DIR]... [--secret NAME]...
//! [--timeout S] [--blob-budget B] [-- args...]`: builds the package's tool build and runs it on
//! the native host's tool profile (language.md §15.5, `wrela_host::tool`), with what the command
//! line grants it and nothing else. The process's exit status is the tool's: its own, or 70 if
//! it trapped or waits on nothing, 71 if the host couldn't run it, 124 past `--timeout`, 130 on
//! Ctrl-C. A package that doesn't build exits 1 with its diagnostics, and a usage error 2.
//!
//! The build is made in memory and its WASM compiled for this run: nothing is written beside
//! the package but the constants' cache that every build keeps.

use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use wrela_host::tool::{BlobBudget, Caps, Grants, Options, ToolBuild};

pub fn run(args: &[String]) -> ExitCode {
    let mut grants = Grants::default();
    let mut timeout = None;
    let mut budget = BlobBudget::default();
    let mut dir = None;
    let mut tool_args = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let flag = a.as_str();
        if flag == "--" {
            tool_args = it.by_ref().cloned().collect();
            break;
        }
        if !flag.starts_with('-') && dir.is_none() {
            dir = Some(std::path::PathBuf::from(a));
            continue;
        }
        if !matches!(flag, "--read" | "--write" | "--secret" | "--timeout" | "--blob-budget") {
            return crate::unknown(a);
        }
        let Some(value) = it.next() else {
            eprintln!("error: {flag} takes a value");
            return ExitCode::from(2);
        };
        let parsed = match flag {
            "--timeout" => value
                .parse::<f64>()
                .ok()
                .filter(|s| s.is_finite() && *s > 0.0)
                .map(|s| timeout = Some(Duration::from_secs_f64(s)))
                .ok_or_else(|| format!("--timeout takes seconds, not `{value}`")),
            "--blob-budget" => BlobBudget::parse(value).map(|b| budget = b),
            _ => grants.parse(flag, value).map(|_| ()),
        };
        if let Err(why) = parsed {
            eprintln!("error: {why}");
            return ExitCode::from(2);
        }
    }
    let Some(dir) = dir else { return crate::usage() };
    if let Err(status) = crate::package_dir(&dir) {
        return status;
    }
    let caps = match Caps::open(&grants, |name| std::env::var(name).ok()) {
        Ok(c) => c,
        Err(why) => {
            eprintln!("error: {why}");
            return ExitCode::from(2);
        }
    };
    let built = wrela_driver::build(&dir);
    if crate::report(&built, false) {
        return ExitCode::from(1);
    }
    let Some((_, wasm)) = built.files.iter().find(|(p, _)| p == wrela_driver::build::TOOL_WASM)
    else {
        eprintln!(
            "error: `{}` has no tool build: its main.wrela doesn't export `tool` (§15.5)",
            dir.display()
        );
        return ExitCode::from(2);
    };
    let interrupt = Arc::new(AtomicBool::new(false));
    let flag = interrupt.clone();
    // The first Ctrl-C stops the tool, and the host cleans up after it; a second ends the
    // process at once.
    let _ = ctrlc::set_handler(move || {
        if flag.swap(true, Ordering::AcqRel) {
            std::process::exit(130);
        }
    });
    let mut options = Options::new(caps);
    options.args = tool_args;
    options.timeout = timeout;
    options.blob_budget = budget;
    options.interrupt = Some(interrupt);
    let ended = wrela_host::tool::run(&ToolBuild::from_wasm(wasm.clone()), options);
    if !matches!(ended.end, wrela_host::tool::End::Exit(_)) {
        eprintln!("error: {}", ended.end);
    }
    if ended.workers_left > 0 {
        eprintln!(
            "warning: {} of the host's workers were still busy when the tool ended",
            ended.workers_left
        );
    }
    ExitCode::from(ended.end.status())
}
