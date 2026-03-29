use std::sync::{Mutex, OnceLock};

use crate::class::class_get;
use crate::value::{Value, int_value};
use crate::wr_rc_dec;
#[cfg(test)]
use std::cell::RefCell;

#[derive(Clone, Copy)]
pub struct ActorConfig {
    pub mailbox_cap: usize,
    pub batch_limit: usize,
}

#[derive(Clone)]
pub struct RuntimeConfig {
    pub actor_mailbox_cap: usize,
    pub actor_enqueue_timeout_ms: u64,
    pub actor_batch_limit: usize,
    pub pause_queue_cap: usize,
    pub deterministic: bool,
    pub debug_actor: bool,
    pub reactor_disable_io_uring: bool,
    pub pool_queue_cap: usize,
    pub pool_auto_min: i64,
    pub pool_auto_max: i64,
    pub diagnostics_enabled: bool,
    pub tokio_worker_threads: Option<usize>,
    pub tokio_blocking_threads: usize,
    pub allow_fs: bool,
    pub allow_net: bool,
    pub allow_env_get: bool,
    pub allow_env_set: bool,
    pub allow_time: bool,
    pub allow_actor: bool,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        let cores = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        Self {
            actor_mailbox_cap: 256,
            actor_enqueue_timeout_ms: 10,
            actor_batch_limit: 64,
            pause_queue_cap: 128,
            deterministic: false,
            debug_actor: false,
            reactor_disable_io_uring: false,
            pool_queue_cap: 256,
            pool_auto_min: 1,
            pool_auto_max: cores as i64,
            diagnostics_enabled: false,
            tokio_worker_threads: None,
            tokio_blocking_threads: (4 * cores).max(64).min(512),
            allow_fs: true,
            allow_net: true,
            allow_env_get: true,
            allow_env_set: true,
            allow_time: true,
            allow_actor: true,
        }
    }
}

static RUNTIME_CONFIG: OnceLock<Mutex<RuntimeConfig>> = OnceLock::new();
#[cfg(test)]
thread_local! {
    static TEST_RUNTIME_CONFIG_OVERRIDE: RefCell<Option<RuntimeConfig>> = const { RefCell::new(None) };
}
#[cfg(test)]
static TEST_RUNTIME_CONFIG_OVERRIDE_GLOBAL: OnceLock<Mutex<Option<RuntimeConfig>>> =
    OnceLock::new();

fn runtime_config() -> RuntimeConfig {
    #[cfg(test)]
    if let Some(override_config) = TEST_RUNTIME_CONFIG_OVERRIDE.with(|slot| slot.borrow().clone()) {
        return override_config;
    }
    #[cfg(test)]
    if let Some(override_config) = TEST_RUNTIME_CONFIG_OVERRIDE_GLOBAL
        .get_or_init(|| Mutex::new(None))
        .lock()
        .expect("runtime global test override lock")
        .clone()
    {
        return override_config;
    }
    RUNTIME_CONFIG
        .get_or_init(|| Mutex::new(RuntimeConfig::default()))
        .lock()
        .expect("runtime config lock")
        .clone()
}

fn set_runtime_config(config: RuntimeConfig) {
    *RUNTIME_CONFIG
        .get_or_init(|| Mutex::new(RuntimeConfig::default()))
        .lock()
        .expect("runtime config lock") = config;
}

pub fn runtime_configure(config: Value) -> Value {
    let runtime_config = runtime_config_from_value(config);
    validate_runtime_config(&runtime_config);
    set_runtime_config(runtime_config);
    Value::nil()
}

fn runtime_config_from_value(config: Value) -> RuntimeConfig {
    let mut out = RuntimeConfig::default();
    if let Some(val) = config_field_usize(config, "actor_mailbox_cap") {
        out.actor_mailbox_cap = val;
    }
    if let Some(val) = config_field_u64(config, "actor_enqueue_timeout_ms") {
        out.actor_enqueue_timeout_ms = val;
    }
    if let Some(val) = config_field_usize(config, "actor_batch_limit") {
        out.actor_batch_limit = val;
    }
    if let Some(val) = config_field_usize(config, "paused_queue_cap")
        .or_else(|| config_field_usize(config, "pause_queue_cap"))
    {
        out.pause_queue_cap = val;
    }
    if let Some(val) = config_field_bool(config, "deterministic") {
        out.deterministic = val;
    }
    if let Some(val) = config_field_bool(config, "actor_debug")
        .or_else(|| config_field_bool(config, "debug_actor"))
    {
        out.debug_actor = val;
    }
    if let Some(val) = config_field_bool(config, "reactor_disable_io_uring") {
        out.reactor_disable_io_uring = val;
    }
    if let Some(val) = config_field_usize(config, "pool_queue_cap") {
        out.pool_queue_cap = val;
    }
    if let Some(val) = config_field_i64(config, "pool_auto_min") {
        out.pool_auto_min = val;
    }
    if let Some(val) = config_field_i64(config, "pool_auto_max") {
        out.pool_auto_max = val;
    }
    if let Some(val) = config_field_bool(config, "diagnostics_enabled") {
        out.diagnostics_enabled = val;
    }
    if let Some(val) = config_field_usize(config, "tokio_worker_threads") {
        out.tokio_worker_threads = Some(val);
    }
    if let Some(val) = config_field_usize(config, "tokio_blocking_threads") {
        out.tokio_blocking_threads = val;
    }
    if let Some(val) = config_field_bool(config, "allow_fs")
        .or_else(|| config_field_bool(config, "sandbox_allow_fs"))
    {
        out.allow_fs = val;
    }
    if let Some(val) = config_field_bool(config, "allow_net")
        .or_else(|| config_field_bool(config, "sandbox_allow_net"))
    {
        out.allow_net = val;
    }
    if let Some(val) = config_field_bool(config, "allow_env_get")
        .or_else(|| config_field_bool(config, "sandbox_allow_env_get"))
    {
        out.allow_env_get = val;
    }
    if let Some(val) = config_field_bool(config, "allow_env_set")
        .or_else(|| config_field_bool(config, "sandbox_allow_env_set"))
    {
        out.allow_env_set = val;
    }
    if let Some(val) = config_field_bool(config, "allow_time")
        .or_else(|| config_field_bool(config, "sandbox_allow_time"))
    {
        out.allow_time = val;
    }
    if let Some(val) = config_field_bool(config, "allow_actor")
        .or_else(|| config_field_bool(config, "sandbox_allow_actor"))
    {
        out.allow_actor = val;
    }
    out
}

fn config_field_bool(config: Value, field: &str) -> Option<bool> {
    let val = class_get(config, field.as_ptr(), field.len());
    if val.is_nil() {
        unsafe { wr_rc_dec(val) };
        return None;
    }
    let out = if val.is_bool() {
        Some(val.as_bool())
    } else {
        panic!("runtime_configure: field `{field}` must be a Boolean");
    };
    unsafe { wr_rc_dec(val) };
    out
}

fn config_field_usize(config: Value, field: &str) -> Option<usize> {
    config_field_i64(config, field).map(|val| {
        usize::try_from(val).unwrap_or_else(|_| {
            panic!("runtime_configure: field `{field}` must be a non-negative integer")
        })
    })
}

fn config_field_u64(config: Value, field: &str) -> Option<u64> {
    config_field_i64(config, field).map(|val| {
        u64::try_from(val).unwrap_or_else(|_| {
            panic!("runtime_configure: field `{field}` must be a non-negative integer")
        })
    })
}

#[allow(dead_code)]
fn config_field_u32(config: Value, field: &str) -> Option<u32> {
    config_field_i64(config, field).map(|val| {
        u32::try_from(val)
            .unwrap_or_else(|_| panic!("runtime_configure: field `{field}` must be in u32 range"))
    })
}

fn config_field_i64(config: Value, field: &str) -> Option<i64> {
    let val = class_get(config, field.as_ptr(), field.len());
    if val.is_nil() {
        unsafe { wr_rc_dec(val) };
        return None;
    }
    let out =
        int_value(val).or_else(|| panic!("runtime_configure: field `{field}` must be an Integer"));
    unsafe { wr_rc_dec(val) };
    out
}

pub fn actor_config() -> ActorConfig {
    let config = runtime_config();
    ActorConfig {
        mailbox_cap: config.actor_mailbox_cap,
        batch_limit: config.actor_batch_limit,
    }
}

pub fn actor_catch_panic_enabled() -> bool {
    true
}

#[allow(dead_code)]
pub fn pause_queue_cap() -> usize {
    runtime_config().pause_queue_cap
}

pub fn debug_actor_enabled() -> bool {
    runtime_config().debug_actor
}

pub fn tokio_worker_threads_opt() -> Option<usize> {
    std::env::var("WRELA_TOKIO_WORKERS")
        .ok()
        .and_then(|s| s.parse().ok())
        .or(runtime_config().tokio_worker_threads)
}

pub fn tokio_blocking_threads() -> usize {
    std::env::var("WRELA_TOKIO_BLOCKING_THREADS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| runtime_config().tokio_blocking_threads)
}

pub fn capability_fs_enabled() -> bool {
    runtime_config().allow_fs
}

pub fn capability_net_enabled() -> bool {
    runtime_config().allow_net
}

pub fn capability_env_get_enabled() -> bool {
    runtime_config().allow_env_get
}

pub fn capability_env_set_enabled() -> bool {
    runtime_config().allow_env_set
}

pub fn capability_time_enabled() -> bool {
    runtime_config().allow_time
}

pub fn capability_actor_enabled() -> bool {
    runtime_config().allow_actor
}

pub fn deterministic_runtime_enabled() -> bool {
    if let Some(value) = std::env::var("WRELA_RUNTIME_DETERMINISTIC")
        .ok()
        .as_deref()
        .map(str::trim)
        .and_then(parse_bool_env)
    {
        return value;
    }
    runtime_config().deterministic
}

#[cfg(target_os = "linux")]
pub fn reactor_disable_io_uring() -> bool {
    runtime_config().reactor_disable_io_uring
}

fn parse_bool_env(value: &str) -> Option<bool> {
    match value.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
pub fn set_test_runtime_config_override(config: Option<RuntimeConfig>) {
    TEST_RUNTIME_CONFIG_OVERRIDE.with(|slot| *slot.borrow_mut() = config);
}

fn validate_runtime_config(config: &RuntimeConfig) {
    assert!(
        config.actor_mailbox_cap > 0,
        "runtime config `actor_mailbox_cap` must be > 0"
    );
    assert!(
        config.actor_enqueue_timeout_ms > 0,
        "runtime config `actor_enqueue_timeout_ms` must be > 0"
    );
    assert!(
        config.actor_batch_limit > 0,
        "runtime config `actor_batch_limit` must be > 0"
    );
    assert!(
        config.pause_queue_cap > 0,
        "runtime config `pause_queue_cap` must be > 0"
    );
    assert!(
        config.pool_queue_cap > 0,
        "runtime config `pool_queue_cap` must be > 0"
    );
    assert!(
        config.tokio_blocking_threads > 0,
        "runtime config `tokio_blocking_threads` must be > 0"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn validate_runtime_config_rejects_invalid_actor_mailbox_cap() {
        let mut config = RuntimeConfig::default();
        config.actor_mailbox_cap = 0;
        let result = std::panic::catch_unwind(|| validate_runtime_config(&config));
        assert!(
            result.is_err(),
            "expected validation panic for invalid actor_mailbox_cap"
        );
    }

    #[test]
    fn runtime_config_from_value_reads_sandbox_capabilities() {
        let names = [
            b"allow_fs".as_ptr(),
            b"allow_net".as_ptr(),
            b"allow_env_set".as_ptr(),
            b"allow_actor".as_ptr(),
        ];
        let lens = [8usize, 9usize, 13usize, 11usize];
        let cfg = crate::wr_class_new(2001, names.as_ptr(), lens.as_ptr(), names.len());
        crate::wr_class_set(cfg, b"allow_fs".as_ptr(), 8, Value::from_bool(false));
        crate::wr_class_set(cfg, b"allow_net".as_ptr(), 9, Value::from_bool(false));
        crate::wr_class_set(cfg, b"allow_env_set".as_ptr(), 13, Value::from_bool(false));
        crate::wr_class_set(cfg, b"allow_actor".as_ptr(), 11, Value::from_bool(false));
        let parsed = runtime_config_from_value(cfg);
        assert!(!parsed.allow_fs);
        assert!(!parsed.allow_net);
        assert!(!parsed.allow_env_set);
        assert!(!parsed.allow_actor);
        unsafe {
            crate::wr_rc_dec(cfg);
        }
    }

    #[test]
    fn runtime_config_from_value_reads_sandbox_prefixed_capabilities() {
        let names = [
            b"sandbox_allow_fs".as_ptr(),
            b"sandbox_allow_net".as_ptr(),
            b"sandbox_allow_env_get".as_ptr(),
            b"sandbox_allow_env_set".as_ptr(),
            b"sandbox_allow_time".as_ptr(),
            b"sandbox_allow_actor".as_ptr(),
        ];
        let lens = [16usize, 17usize, 21usize, 21usize, 18usize, 19usize];
        let cfg = crate::wr_class_new(2002, names.as_ptr(), lens.as_ptr(), names.len());
        crate::wr_class_set(
            cfg,
            b"sandbox_allow_fs".as_ptr(),
            16,
            Value::from_bool(false),
        );
        crate::wr_class_set(
            cfg,
            b"sandbox_allow_net".as_ptr(),
            17,
            Value::from_bool(false),
        );
        crate::wr_class_set(
            cfg,
            b"sandbox_allow_env_get".as_ptr(),
            21,
            Value::from_bool(false),
        );
        crate::wr_class_set(
            cfg,
            b"sandbox_allow_env_set".as_ptr(),
            21,
            Value::from_bool(false),
        );
        crate::wr_class_set(
            cfg,
            b"sandbox_allow_time".as_ptr(),
            18,
            Value::from_bool(false),
        );
        crate::wr_class_set(
            cfg,
            b"sandbox_allow_actor".as_ptr(),
            19,
            Value::from_bool(false),
        );
        let parsed = runtime_config_from_value(cfg);
        assert!(!parsed.allow_fs);
        assert!(!parsed.allow_net);
        assert!(!parsed.allow_env_get);
        assert!(!parsed.allow_env_set);
        assert!(!parsed.allow_time);
        assert!(!parsed.allow_actor);
        unsafe {
            crate::wr_rc_dec(cfg);
        }
    }

    #[test]
    fn runtime_config_prefers_allow_over_sandbox_allow_when_both_are_present() {
        let names = [
            b"allow_fs".as_ptr(),
            b"sandbox_allow_fs".as_ptr(),
            b"allow_net".as_ptr(),
            b"sandbox_allow_net".as_ptr(),
            b"allow_env_get".as_ptr(),
            b"sandbox_allow_env_get".as_ptr(),
            b"allow_env_set".as_ptr(),
            b"sandbox_allow_env_set".as_ptr(),
            b"allow_time".as_ptr(),
            b"sandbox_allow_time".as_ptr(),
            b"allow_actor".as_ptr(),
            b"sandbox_allow_actor".as_ptr(),
        ];
        let lens = [
            8usize, 16usize, 9usize, 17usize, 13usize, 21usize, 13usize, 21usize, 10usize, 18usize,
            11usize, 19usize,
        ];
        let cfg = crate::wr_class_new(2003, names.as_ptr(), lens.as_ptr(), names.len());
        crate::wr_class_set(cfg, b"allow_fs".as_ptr(), 8, Value::from_bool(true));
        crate::wr_class_set(
            cfg,
            b"sandbox_allow_fs".as_ptr(),
            16,
            Value::from_bool(false),
        );
        crate::wr_class_set(cfg, b"allow_net".as_ptr(), 9, Value::from_bool(false));
        crate::wr_class_set(
            cfg,
            b"sandbox_allow_net".as_ptr(),
            17,
            Value::from_bool(true),
        );
        crate::wr_class_set(cfg, b"allow_env_get".as_ptr(), 13, Value::from_bool(false));
        crate::wr_class_set(
            cfg,
            b"sandbox_allow_env_get".as_ptr(),
            21,
            Value::from_bool(true),
        );
        crate::wr_class_set(cfg, b"allow_env_set".as_ptr(), 13, Value::from_bool(true));
        crate::wr_class_set(
            cfg,
            b"sandbox_allow_env_set".as_ptr(),
            21,
            Value::from_bool(false),
        );
        crate::wr_class_set(cfg, b"allow_time".as_ptr(), 10, Value::from_bool(true));
        crate::wr_class_set(
            cfg,
            b"sandbox_allow_time".as_ptr(),
            18,
            Value::from_bool(false),
        );
        crate::wr_class_set(cfg, b"allow_actor".as_ptr(), 11, Value::from_bool(false));
        crate::wr_class_set(
            cfg,
            b"sandbox_allow_actor".as_ptr(),
            19,
            Value::from_bool(true),
        );
        let parsed = runtime_config_from_value(cfg);
        assert!(parsed.allow_fs, "allow_fs should win over sandbox_allow_fs");
        assert!(
            !parsed.allow_net,
            "allow_net should win over sandbox_allow_net"
        );
        assert!(
            !parsed.allow_env_get,
            "allow_env_get should win over sandbox_allow_env_get"
        );
        assert!(
            parsed.allow_env_set,
            "allow_env_set should win over sandbox_allow_env_set"
        );
        assert!(
            parsed.allow_time,
            "allow_time should win over sandbox_allow_time"
        );
        assert!(
            !parsed.allow_actor,
            "allow_actor should win over sandbox_allow_actor"
        );
        unsafe {
            crate::wr_rc_dec(cfg);
        }
    }

    #[test]
    fn deterministic_runtime_enabled_respects_runtime_config_flag() {
        let mut cfg = RuntimeConfig::default();
        cfg.deterministic = true;
        set_test_runtime_config_override(Some(cfg));
        assert!(deterministic_runtime_enabled());
        set_test_runtime_config_override(None);
    }

    #[test]
    fn deterministic_runtime_enabled_env_true_overrides_runtime_config_false() {
        let _guard = env_lock().lock().expect("env lock");
        let original = std::env::var("WRELA_RUNTIME_DETERMINISTIC").ok();
        unsafe {
            std::env::set_var("WRELA_RUNTIME_DETERMINISTIC", "1");
        }

        let mut cfg = RuntimeConfig::default();
        cfg.deterministic = false;
        set_test_runtime_config_override(Some(cfg));
        assert!(deterministic_runtime_enabled());
        set_test_runtime_config_override(None);

        if let Some(value) = original {
            unsafe {
                std::env::set_var("WRELA_RUNTIME_DETERMINISTIC", value);
            }
        } else {
            unsafe {
                std::env::remove_var("WRELA_RUNTIME_DETERMINISTIC");
            }
        }
    }

    #[test]
    fn deterministic_runtime_enabled_env_false_overrides_runtime_config_true() {
        let _guard = env_lock().lock().expect("env lock");
        let original = std::env::var("WRELA_RUNTIME_DETERMINISTIC").ok();
        unsafe {
            std::env::set_var("WRELA_RUNTIME_DETERMINISTIC", "0");
        }

        let mut cfg = RuntimeConfig::default();
        cfg.deterministic = true;
        set_test_runtime_config_override(Some(cfg));
        assert!(!deterministic_runtime_enabled());
        set_test_runtime_config_override(None);

        if let Some(value) = original {
            unsafe {
                std::env::set_var("WRELA_RUNTIME_DETERMINISTIC", value);
            }
        } else {
            unsafe {
                std::env::remove_var("WRELA_RUNTIME_DETERMINISTIC");
            }
        }
    }

    #[test]
    fn deterministic_runtime_enabled_invalid_env_falls_back_to_runtime_config() {
        let _guard = env_lock().lock().expect("env lock");
        let original = std::env::var("WRELA_RUNTIME_DETERMINISTIC").ok();
        unsafe {
            std::env::set_var("WRELA_RUNTIME_DETERMINISTIC", "not-a-bool");
        }

        let mut cfg = RuntimeConfig::default();
        cfg.deterministic = true;
        set_test_runtime_config_override(Some(cfg));
        assert!(deterministic_runtime_enabled());
        set_test_runtime_config_override(None);

        if let Some(value) = original {
            unsafe {
                std::env::set_var("WRELA_RUNTIME_DETERMINISTIC", value);
            }
        } else {
            unsafe {
                std::env::remove_var("WRELA_RUNTIME_DETERMINISTIC");
            }
        }
    }
}
