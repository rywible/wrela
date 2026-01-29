use std::sync::OnceLock;
use std::time::Duration;

#[derive(Clone, Copy)]
pub struct ActorConfig {
    pub mailbox_cap: usize,
    pub enqueue_timeout: Duration,
    pub batch_limit: usize,
}

static ACTOR_CONFIG: OnceLock<ActorConfig> = OnceLock::new();

pub fn actor_config() -> &'static ActorConfig {
    ACTOR_CONFIG.get_or_init(|| ActorConfig {
        mailbox_cap: read_env_usize("WRELA_MAILBOX_CAP", 256).max(1),
        enqueue_timeout: Duration::from_millis(read_env_u64("WRELA_ENQUEUE_TIMEOUT_MS", 10)),
        batch_limit: read_env_usize("WRELA_BATCH_LIMIT", 64).max(1),
    })
}

pub fn pause_queue_cap() -> usize {
    read_env_usize("WRELA_PAUSE_QUEUE_CAP", 128).max(1)
}

pub fn actor_config_for_objective(objective: u8) -> ActorConfig {
    let base = *actor_config();
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
    if std::env::var("WRELA_DETERMINISTIC")
        .ok()
        .map(|v| !matches!(v.as_str(), "0" | "false" | "off"))
        .unwrap_or(false)
    {
        return 1;
    }
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    read_env_usize("WRELA_SCHED_SHARDS", cores).max(1)
}

pub fn sched_tick_ms() -> u64 {
    if std::env::var("WRELA_DETERMINISTIC")
        .ok()
        .map(|v| !matches!(v.as_str(), "0" | "false" | "off"))
        .unwrap_or(false)
    {
        return 1;
    }
    read_env_u64("WRELA_SCHED_TICK_MS", 1).max(1)
}

pub fn sched_ready_cap() -> usize {
    read_env_usize("WRELA_SCHED_READY_CAP", 1024).max(2)
}

pub fn sched_batch_limit() -> i64 {
    read_env_u64("WRELA_SCHED_BATCH", 64).max(1) as i64
}

pub fn pool_min_share_default() -> u32 {
    read_env_usize("WRELA_POOL_MIN_SHARE", 1).max(1) as u32
}

pub fn pool_max_share_default() -> u32 {
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    read_env_usize("WRELA_POOL_MAX_SHARE", cores * 2).max(1) as u32
}

pub fn pool_queue_cap_default() -> usize {
    read_env_usize("WRELA_POOL_QUEUE_CAP", 256).max(1)
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

fn read_env_usize(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|val| val.parse::<usize>().ok())
        .unwrap_or(default)
}

fn read_env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|val| val.parse::<u64>().ok())
        .unwrap_or(default)
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
    let min_default = read_env_usize("WRELA_POOL_AUTO_MIN", 1).max(1) as i64;
    let max_default = read_env_usize("WRELA_POOL_AUTO_MAX", cores as usize).max(1) as i64;
    let min = if min > 0 { min } else { min_default };
    let max = if max > 0 { max } else { max_default };
    let weight = if weight > 0 { weight } else { 1 };
    let weighted = base.saturating_mul(weight);
    weighted.clamp(min, max.max(min))
}
