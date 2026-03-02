use std::sync::Once;

pub const RUNTIME_ABI_VERSION: u32 = 5;

static INIT: Once = Once::new();

pub fn runtime_init() {
    INIT.call_once(|| {});
}

pub fn dump_diagnostics() {
    // Intentionally minimal after runtime module cleanup.
}

#[allow(dead_code)]
pub fn log_event(_event: &str) {
    // Intentionally minimal after runtime module cleanup.
}
