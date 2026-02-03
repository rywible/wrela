use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::scheduler;

pub const RUNTIME_ABI_VERSION: u32 = 1;
const EVENT_CAP: usize = 256;

static DIAG_INIT: OnceLock<()> = OnceLock::new();
static EVENTS: OnceLock<Mutex<VecDeque<String>>> = OnceLock::new();

fn enabled() -> bool {
    crate::config::diagnostics_enabled()
}

pub fn runtime_init() {
    if DIAG_INIT.get().is_some() {
        return;
    }
    let _ = DIAG_INIT.set(());
    if enabled() {
        init_events();
        std::panic::set_hook(Box::new(|info| {
            eprintln!("panic: {info}");
            let bt = std::backtrace::Backtrace::force_capture();
            eprintln!("{bt}");
            dump_diagnostics();
        }));
        log_event("runtime_init");
    }
}

fn init_events() -> &'static Mutex<VecDeque<String>> {
    EVENTS.get_or_init(|| Mutex::new(VecDeque::with_capacity(EVENT_CAP)))
}

pub fn log_event(msg: &str) {
    if !enabled() {
        return;
    }
    let events = init_events();
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let mut guard = events.lock().expect("diagnostics lock");
    if guard.len() == EVENT_CAP {
        guard.pop_front();
    }
    guard.push_back(format!("{ts} {msg}"));
}

pub fn dump_diagnostics() {
    if !enabled() {
        return;
    }
    eprintln!("--- runtime diagnostics ---");
    eprintln!("{}", scheduler::snapshot());
    if let Some(events) = EVENTS.get() {
        let guard = events.lock().expect("diagnostics lock");
        if !guard.is_empty() {
            eprintln!("recent events:");
            for ev in guard.iter() {
                eprintln!("  {ev}");
            }
        }
    }
}
