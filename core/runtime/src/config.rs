use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use crate::class::class_get;
use crate::value::{Value, int_value};
use crate::wr_rc_dec;

fn env_i64(key: &str) -> Option<i64> {
    std::env::var(key)
        .ok()
        .and_then(|val| val.trim().parse::<i64>().ok())
}

fn env_bool(key: &str) -> Option<bool> {
    let val = std::env::var(key).ok()?;
    match val.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "on" => Some(true),
        "0" | "false" | "off" => Some(false),
        _ => None,
    }
}

fn env_usize(key: &str) -> Option<usize> {
    env_i64(key).and_then(|val| if val >= 0 { Some(val as usize) } else { None })
}

fn env_u64(key: &str) -> Option<u64> {
    env_i64(key).and_then(|val| if val >= 0 { Some(val as u64) } else { None })
}

fn env_u32(key: &str) -> Option<u32> {
    env_i64(key).and_then(|val| if val >= 0 { Some(val as u32) } else { None })
}

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
        let mut config = Self {
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
            pool_min_share: 1,
            pool_max_share: (cores * 2).max(1) as u32,
            pool_queue_cap: 256,
            pool_auto_min: 1,
            pool_auto_max: cores as i64,
            diagnostics_enabled: false,
        };
        if let Some(val) = env_usize("WRELA_ACTOR_MAILBOX_CAP") {
            config.actor_mailbox_cap = val;
        }
        if let Some(val) = env_u64("WRELA_ACTOR_ENQUEUE_TIMEOUT_MS") {
            config.actor_enqueue_timeout_ms = val;
        }
        if let Some(val) = env_usize("WRELA_ACTOR_BATCH_LIMIT") {
            config.actor_batch_limit = val;
        }
        if let Some(val) = env_usize("WRELA_PAUSE_QUEUE_CAP") {
            config.pause_queue_cap = val;
        }
        if let Some(val) = env_bool("WRELA_DETERMINISTIC") {
            config.deterministic = val;
        }
        if let Some(val) = env_bool("WRELA_DEBUG_ACTOR") {
            config.debug_actor = val;
        }
        if let Some(val) = env_u64("WRELA_ACTOR_WATCHDOG_MS") {
            config.actor_watchdog_ms = val;
        }
        if let Some(val) = env_u64("WRELA_SCHED_WATCHDOG_MS") {
            config.sched_watchdog_ms = val;
        }
        if let Some(val) = env_usize("WRELA_SCHED_SHARDS") {
            if val > 0 {
                config.sched_shards = val;
            }
        }
        if let Some(val) = env_u64("WRELA_SCHED_TICK_MS") {
            config.sched_tick_ms = val;
        }
        if let Some(val) = env_usize("WRELA_SCHED_READY_CAP") {
            config.sched_ready_cap = val;
        }
        if let Some(val) = env_i64("WRELA_SCHED_BATCH_LIMIT") {
            config.sched_batch_limit = val;
        }
        if let Some(val) = env_u32("WRELA_POOL_MIN_SHARE") {
            config.pool_min_share = val;
        }
        if let Some(val) = env_u32("WRELA_POOL_MAX_SHARE") {
            if val > 0 {
                config.pool_max_share = val;
            }
        }
        if let Some(val) = env_usize("WRELA_POOL_QUEUE_CAP") {
            config.pool_queue_cap = val;
        }
        if let Some(val) = env_i64("WRELA_POOL_AUTO_MIN") {
            if val > 0 {
                config.pool_auto_min = val;
            }
        }
        if let Some(val) = env_i64("WRELA_POOL_AUTO_MAX") {
            if val > 0 {
                config.pool_auto_max = val;
            }
        }
        if let Some(val) = env_bool("WRELA_DIAGNOSTICS_ENABLED") {
            config.diagnostics_enabled = val;
        }
        config
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
    let new_config = runtime_config_from_value(config);
    set_runtime_config(new_config);
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
        if val > 0 {
            out.sched_shards = val;
        }
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
    if let Some(val) = config_field_u32(config, "pool_min_share") {
        out.pool_min_share = val;
    }
    if let Some(val) = config_field_u32(config, "pool_max_share") {
        if val > 0 {
            out.pool_max_share = val;
        }
    }
    if let Some(val) = config_field_usize(config, "pool_queue_cap") {
        out.pool_queue_cap = val;
    }
    if let Some(val) = config_field_i64(config, "pool_auto_min") {
        if val > 0 {
            out.pool_auto_min = val;
        }
    }
    if let Some(val) = config_field_i64(config, "pool_auto_max") {
        if val > 0 {
            out.pool_auto_max = val;
        }
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
    let out = if val.is_bool() { Some(val.as_bool()) } else { None };
    unsafe { wr_rc_dec(val) };
    out
}

fn config_field_usize(config: Value, field: &str) -> Option<usize> {
    config_field_i64(config, field).and_then(|val| if val >= 0 { Some(val as usize) } else { None })
}

fn config_field_u64(config: Value, field: &str) -> Option<u64> {
    config_field_i64(config, field).and_then(|val| if val >= 0 { Some(val as u64) } else { None })
}

fn config_field_u32(config: Value, field: &str) -> Option<u32> {
    config_field_i64(config, field).and_then(|val| if val >= 0 { Some(val as u32) } else { None })
}

fn config_field_i64(config: Value, field: &str) -> Option<i64> {
    let val = class_get(config, field.as_ptr(), field.len());
    if val.is_nil() {
        unsafe { wr_rc_dec(val) };
        return None;
    }
    let out = int_value(val);
    unsafe { wr_rc_dec(val) };
    out
}

pub fn actor_config() -> ActorConfig {
    let config = runtime_config();
    ActorConfig {
        mailbox_cap: config.actor_mailbox_cap.max(1),
        enqueue_timeout: Duration::from_millis(config.actor_enqueue_timeout_ms.max(1)),
        batch_limit: config.actor_batch_limit.max(1),
    }
}

pub fn pause_queue_cap() -> usize {
    runtime_config().pause_queue_cap.max(1)
}

pub fn deterministic_runtime() -> bool {
    runtime_config().deterministic
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

pub fn diagnostics_enabled() -> bool {
    runtime_config().diagnostics_enabled
}

pub fn actor_config_for_objective(objective: u8) -> ActorConfig {
    let base = actor_config();
    match objective {
        // latency
        0 => ActorConfig {
            mailbox_cap: scale_usize(base.mailbox_cap, 1, 2, 1),
            enqueue_timeout: scale_duration(base.enqueue_timeout, 1, 2),
            batch_limit: scale_usize(base.batch_limit, 1, 2, 1),
        },
        // throughput
        1 => ActorConfig {
            mailbox_cap: scale_usize(base.mailbox_cap, 2, 1, 1),
            enqueue_timeout: scale_duration(base.enqueue_timeout, 2, 1),
            batch_limit: scale_usize(base.batch_limit, 2, 1, 1),
        },
        // conservation
        2 => ActorConfig {
            mailbox_cap: scale_usize(base.mailbox_cap, 1, 2, 1),
            enqueue_timeout: base.enqueue_timeout,
            batch_limit: base.batch_limit,
        },
        // balance / default
        _ => base,
    }
}

pub fn sched_shards() -> usize {
    let config = runtime_config();
    if config.deterministic {
        return 1;
    }
    config.sched_shards.max(1)
}

pub fn sched_tick_ms() -> u64 {
    let config = runtime_config();
    if config.deterministic {
        return 1;
    }
    config.sched_tick_ms.max(1)
}

pub fn sched_ready_cap() -> usize {
    runtime_config().sched_ready_cap.max(2)
}

pub fn sched_batch_limit() -> i64 {
    runtime_config().sched_batch_limit.max(1)
}

pub fn pool_min_share_default() -> u32 {
    runtime_config().pool_min_share.max(1)
}

pub fn pool_max_share_default() -> u32 {
    let config = runtime_config();
    if config.pool_max_share == 0 {
        let cores = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        return (cores * 2).max(1) as u32;
    }
    config.pool_max_share.max(1)
}

pub fn pool_queue_cap_default() -> usize {
    runtime_config().pool_queue_cap.max(1)
}

pub fn pool_queue_cap_for_objective(objective: u8) -> usize {
    let base = pool_queue_cap_default();
    match objective {
        // latency
        0 => base.max(32),
        // throughput
        1 => base.saturating_mul(2).max(64),
        // conservation
        2 => (base / 2).max(16),
        // balance
        _ => base.max(32),
    }
}

pub fn sched_batch_limit_for_objective(objective: u8) -> i64 {
    let base = sched_batch_limit();
    match objective {
        0 => (base / 2).max(1),
        1 => base.saturating_mul(2).max(1),
        2 => base.max(1),
        _ => base.max(1),
    }
}

pub fn pool_queue_cap_for_policy(objective: u8, queue_cap: isize) -> usize {
    if queue_cap == 0 {
        1
    } else if queue_cap > 0 {
        queue_cap as usize
    } else {
        pool_queue_cap_for_objective(objective)
    }
}

pub fn normalize_pool_size(size: i64, objective: u8) -> i64 {
    if size >= 1 {
        size
    } else {
        auto_pool_size(objective, 0, 0, 0)
    }
}

pub fn pool_auto_size(objective: u8, min: i64, max: i64, weight: i64) -> i64 {
    auto_pool_size(objective, min, max, weight)
}

pub fn normalize_objective(objective: i64) -> u8 {
    match objective {
        0 => 0,
        1 => 1,
        2 => 2,
        3 => 3,
        _ => 3,
    }
}

fn scale_usize(value: usize, num: usize, den: usize, min: usize) -> usize {
    let scaled = value.saturating_mul(num).saturating_div(den.max(1));
    scaled.max(min)
}

fn scale_duration(value: Duration, num: u64, den: u64) -> Duration {
    let millis = value.as_millis() as u64;
    let scaled = millis.saturating_mul(num).saturating_div(den.max(1));
    Duration::from_millis(scaled.max(1))
}

fn auto_pool_size(objective: u8, min: i64, max: i64, weight: i64) -> i64 {
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1) as i64;
    let base = match objective {
        // latency
        0 => cores,
        // throughput
        1 => cores.saturating_mul(2),
        // conservation
        2 => (cores / 2).max(1),
        // balance
        _ => cores,
    };
    let config = runtime_config();
    let min_default = config.pool_auto_min.max(1);
    let max_default = if config.pool_auto_max > 0 {
        config.pool_auto_max
    } else {
        cores.max(1)
    };
    let min = if min > 0 { min } else { min_default };
    let max = if max > 0 { max } else { max_default };
    let weight = if weight > 0 { weight } else { 1 };
    let weighted = base.saturating_mul(weight);
    weighted.clamp(min, max.max(min))
}
