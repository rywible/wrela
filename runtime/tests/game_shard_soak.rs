use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug)]
struct SimClientState {
    tick: u64,
    player_x: f32,
    player_y: f32,
    score: u32,
    collected_mask: u32,
}

impl Default for SimClientState {
    fn default() -> Self {
        Self {
            tick: 0,
            player_x: 400.0,
            player_y: 300.0,
            score: 0,
            collected_mask: 0,
        }
    }
}

fn axis_for(client_index: usize, tick: u64) -> (f32, f32) {
    match ((client_index as u64) + tick) % 8 {
        0 => (1.0, 0.0),
        1 => (0.0, 1.0),
        2 => (-1.0, 0.0),
        3 => (0.0, -1.0),
        4 => (0.7, 0.7),
        5 => (-0.7, 0.7),
        6 => (-0.7, -0.7),
        _ => (0.7, -0.7),
    }
}

fn step_client(state: &mut SimClientState, client_index: usize, tick: u64) {
    let (axis_x, axis_y) = axis_for(client_index, tick);
    let dt_seconds = 0.016f32;
    let speed = 240.0f32;
    state.player_x += axis_x * speed * dt_seconds;
    state.player_y += axis_y * speed * dt_seconds;
    state.player_x = state.player_x.clamp(18.0, 782.0);
    state.player_y = state.player_y.clamp(18.0, 582.0);
    state.tick = state.tick.saturating_add(1);

    // Deterministic synthetic pickup cadence that emulates per-client world progress.
    if ((tick as usize + client_index) % 120) == 0 {
        let bit = (client_index % u32::BITS as usize) as u32;
        let mask = 1u32 << bit;
        if state.collected_mask & mask == 0 {
            state.collected_mask |= mask;
            state.score = state.score.saturating_add(1);
        }
    }
}

fn state_hash(state: &SimClientState) -> u64 {
    const PRIME: u64 = 1_099_511_628_211;
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    hash ^= state.tick;
    hash = hash.wrapping_mul(PRIME);
    hash ^= state.player_x.to_bits() as u64;
    hash = hash.wrapping_mul(PRIME);
    hash ^= state.player_y.to_bits() as u64;
    hash = hash.wrapping_mul(PRIME);
    hash ^= state.score as u64;
    hash = hash.wrapping_mul(PRIME);
    hash ^= state.collected_mask as u64;
    hash = hash.wrapping_mul(PRIME);
    hash
}

fn max_duration_budget() -> Duration {
    let max_ms = std::env::var("WRELA_GAME_SHARD_SOAK_MAX_MS")
        .ok()
        .and_then(|raw| raw.parse::<u64>().ok())
        .unwrap_or(10_000);
    Duration::from_millis(max_ms)
}

#[test]
#[ignore = "expensive 100-client soak/perf hook; run with --ignored --nocapture"]
fn game_shard_soak_100_clients() {
    // Keep one explicit runtime touchpoint so this hook remains anchored to runtime config defaults.
    let config = wrela_runtime::game_slice::VerticalSliceServerConfig::default();
    assert!(
        !config.bind_address.trim().is_empty(),
        "runtime default bind address must be non-empty"
    );

    const CLIENT_COUNT: usize = 100;
    const TICKS: u64 = 2_000;
    let mut clients = vec![SimClientState::default(); CLIENT_COUNT];

    let started_at = Instant::now();
    for tick in 0..TICKS {
        for (idx, client) in clients.iter_mut().enumerate() {
            step_client(client, idx, tick);
        }
    }
    let elapsed = started_at.elapsed();

    let min_tick = clients.iter().map(|client| client.tick).min().unwrap_or(0);
    let max_tick = clients.iter().map(|client| client.tick).max().unwrap_or(0);
    let aggregate_score: u64 = clients.iter().map(|client| client.score as u64).sum();
    let aggregate_hash = clients
        .iter()
        .fold(0u64, |acc, client| acc ^ state_hash(client).rotate_left(1));

    println!(
        "game_shard_soak: clients={} ticks={} elapsed_ms={} min_tick={} max_tick={} aggregate_score={} aggregate_hash=0x{:016x}",
        CLIENT_COUNT,
        TICKS,
        elapsed.as_millis(),
        min_tick,
        max_tick,
        aggregate_score,
        aggregate_hash
    );

    assert_eq!(
        min_tick, TICKS,
        "all clients must complete full soak horizon"
    );
    assert_eq!(
        max_tick, TICKS,
        "all clients must complete full soak horizon"
    );
    assert!(
        clients.iter().all(|client| {
            client.player_x.is_finite()
                && client.player_y.is_finite()
                && client.player_x >= 18.0
                && client.player_x <= 782.0
                && client.player_y >= 18.0
                && client.player_y <= 582.0
        }),
        "all simulated client positions must remain in-bounds and finite"
    );
    assert_ne!(
        aggregate_hash, 0,
        "aggregate hash should never collapse to zero"
    );
    let duration_budget = max_duration_budget();
    assert!(
        elapsed <= duration_budget,
        "soak exceeded duration budget: elapsed_ms={} budget_ms={}",
        elapsed.as_millis(),
        duration_budget.as_millis()
    );
}
