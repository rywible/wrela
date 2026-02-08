use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use crate::class::class_get;
use crate::value::{Value, int_value};
use crate::wr_rc_dec;

#[derive(Clone, Copy)]
pub struct ActorConfig {
    pub mailbox_cap: usize,
    pub enqueue_timeout: Duration,
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
    pub actor_watchdog_ms: u64,
    pub sched_watchdog_ms: u64,
    pub sched_shards: usize,
    pub sched_tick_ms: u64,
    pub sched_ready_cap: usize,
    pub sched_batch_limit: i64,
    pub reactor_disable_io_uring: bool,
    pub pool_min_share: u32,
    pub pool_max_share: u32,
    pub pool_queue_cap: usize,
    pub pool_auto_min: i64,
    pub pool_auto_max: i64,
    pub diagnostics_enabled: bool,
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
            actor_watchdog_ms: 0,
            sched_watchdog_ms: 0,
            sched_shards: cores,
            sched_tick_ms: 1,
            sched_ready_cap: 1024,
            sched_batch_limit: 64,
            reactor_disable_io_uring: false,
            pool_min_share: 1,
            pool_max_share: (cores * 2).max(1) as u32,
            pool_queue_cap: 256,
            pool_auto_min: 1,
            pool_auto_max: cores as i64,
            diagnostics_enabled: false,
        }
    }
}

static RUNTIME_CONFIG: OnceLock<Mutex<RuntimeConfig>> = OnceLock::new();

fn runtime_config() -> RuntimeConfig {
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
    if let Some(val) = config_field_usize(config, "pause_queue_cap") {
        out.pause_queue_cap = val;
    }
    if let Some(val) = config_field_bool(config, "deterministic") {
        out.deterministic = val;
    }
    if let Some(val) = config_field_bool(config, "debug_actor") {
        out.debug_actor = val;
    }
    if let Some(val) = config_field_u64(config, "actor_watchdog_ms") {
        out.actor_watchdog_ms = val;
    }
    if let Some(val) = config_field_u64(config, "sched_watchdog_ms") {
        out.sched_watchdog_ms = val;
    }
    if let Some(val) = config_field_usize(config, "sched_shards") {
        out.sched_shards = val;
    }
    if let Some(val) = config_field_u64(config, "sched_tick_ms") {
        out.sched_tick_ms = val;
    }
    if let Some(val) = config_field_usize(config, "sched_ready_cap") {
        out.sched_ready_cap = val;
    }
    if let Some(val) = config_field_i64(config, "sched_batch_limit") {
        out.sched_batch_limit = val;
    }
    if let Some(val) = config_field_bool(config, "reactor_disable_io_uring") {
        out.reactor_disable_io_uring = val;
    }
    if let Some(val) = config_field_u32(config, "pool_min_share") {
        out.pool_min_share = val;
    }
    if let Some(val) = config_field_u32(config, "pool_max_share") {
        out.pool_max_share = val;
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
        enqueue_timeout: Duration::from_millis(config.actor_enqueue_timeout_ms),
        batch_limit: config.actor_batch_limit,
    }
}

pub fn pause_queue_cap() -> usize {
    runtime_config().pause_queue_cap
}

pub fn debug_actor_enabled() -> bool {
    runtime_config().debug_actor
}

pub fn actor_watchdog_ms() -> u64 {
    runtime_config().actor_watchdog_ms
}

pub fn sched_watchdog_ms() -> u64 {
    runtime_config().sched_watchdog_ms
}

pub fn sched_shards() -> usize {
    runtime_config().sched_shards
}

pub fn sched_tick_ms() -> u64 {
    runtime_config().sched_tick_ms
}

pub fn sched_ready_cap() -> usize {
    runtime_config().sched_ready_cap
}

pub fn sched_batch_limit() -> i64 {
    runtime_config().sched_batch_limit
}

pub fn reactor_disable_io_uring() -> bool {
    runtime_config().reactor_disable_io_uring
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
        config.sched_shards > 0,
        "runtime config `sched_shards` must be > 0"
    );
    assert!(
        config.sched_tick_ms > 0,
        "runtime config `sched_tick_ms` must be > 0"
    );
    assert!(
        config.sched_ready_cap >= 2,
        "runtime config `sched_ready_cap` must be >= 2"
    );
    assert!(
        config.sched_batch_limit > 0,
        "runtime config `sched_batch_limit` must be > 0"
    );
    assert!(
        config.pool_min_share > 0,
        "runtime config `pool_min_share` must be > 0"
    );
    assert!(
        config.pool_max_share > 0,
        "runtime config `pool_max_share` must be > 0"
    );
    assert!(
        config.pool_queue_cap > 0,
        "runtime config `pool_queue_cap` must be > 0"
    );
}
