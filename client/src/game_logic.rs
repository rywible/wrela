/// Rust-native deterministic combat tick with lock-on camera metadata.
///
/// Hard-cut upgrade notes:
/// - Runtime combat state is multi-target (`enemies`) instead of single-enemy only.
/// - Lock-on intent and target cycling are first-class input/state.
/// - RenderData now carries lock-on/camera readability telemetry.
///
/// The existing single-enemy mirror fields remain as a cache of the current
/// primary combat target used by some rendering systems while the rest of the
/// runtime migrates to pure multi-target consumption.

// Character states
const STATE_IDLE: i32 = 0;
const STATE_WALK: i32 = 1;
const STATE_RUN: i32 = 2;
const STATE_DODGE: i32 = 3;
const STATE_ATTACK: i32 = 4;
const STATE_STAGGER: i32 = 5;
const STATE_PARRY_ACTIVE: i32 = 6;

// Attack frame data (startup, active, recovery ticks)
const LIGHT_STARTUP: i32 = 3;
const LIGHT_ACTIVE: i32 = 4;
const LIGHT_RECOVERY: i32 = 8;
const HEAVY_STARTUP: i32 = 5;
const HEAVY_ACTIVE: i32 = 6;
const HEAVY_RECOVERY: i32 = 12;
const PARRY_STARTUP: i32 = 2;
const PARRY_ACTIVE: i32 = 4;
const PARRY_RECOVERY: i32 = 16;
const DODGE_STARTUP: i32 = 2;
const DODGE_ACTIVE: i32 = 3;
const DODGE_RECOVERY: i32 = 10;

// Combat values (milli-scaled)
const LIGHT_DAMAGE: i32 = 12000;
const HEAVY_DAMAGE: i32 = 25000;
const LIGHT_STAMINA_COST: i32 = 8000;
const HEAVY_STAMINA_COST: i32 = 18000;
const PARRY_STAMINA_COST: i32 = 5000;
const DODGE_STAMINA_COST: i32 = 10000;
const LIGHT_POISE_DAMAGE: i32 = 15000;
const HEAVY_POISE_DAMAGE: i32 = 40000;
const PARRY_COUNTER_WINDOW: i32 = 10;

// Hit stop ticks
const HIT_STOP_LIGHT: i32 = 3;
const HIT_STOP_HEAVY: i32 = 6;
const HIT_STOP_PARRY: i32 = 8;

// Movement
const MOVE_SPEED: i32 = 80; // milli per tick
const DODGE_DISTANCE: i32 = 200;

// Stamina
const MAX_STAMINA: i32 = 100000;
const STAMINA_REGEN_RATE: i32 = 600; // per tick
const STAMINA_REGEN_DELAY: i32 = 30; // ticks before regen starts

// Poise
const MAX_POISE: i32 = 100000;
const POISE_REGEN_RATE: i32 = 500;
const POISE_BROKEN_DURATION: i32 = 30;

// Resonance
const RESONANCE_DECAY: i32 = 3;
const RESONANCE_PARRY_GAIN: i32 = 200;
const RESONANCE_KILL_GAIN: i32 = 300;

// Enemy (Rot Stalker profile)
const ENEMY_AGGRO_RANGE: i32 = 3000;
const ENEMY_ATTACK_RANGE: i32 = 800;
const ENEMY_CLAW_DAMAGE: i32 = 15000;
const ENEMY_LUNGE_DAMAGE: i32 = 20000;
const ENEMY_MOVE_CLAW: i32 = 10;
const ENEMY_MOVE_LUNGE: i32 = 11;

// Lock-on and camera grammar
const MAX_ENEMIES: usize = 8;
const LOCK_ON_CONE_DOT_MIN: i32 = -150; // allow nearly full sphere for keyboard-only demo
const LOCK_ON_BREAK_RANGE: i32 = 12000;
const CAMERA_DISTANCE_EXPLORATION: i32 = 5000;
const CAMERA_DISTANCE_COMBAT: i32 = 4300;
const CAMERA_HEIGHT: i32 = 3000;
const CAMERA_SPRING_FACTOR: i32 = 100; // milli (0.1 = 10% per tick)

#[derive(Clone, Copy, Debug)]
pub struct EnemyState {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub facing_x: i32,
    pub facing_z: i32,
    pub health: i32,
    pub max_health: i32,
    pub poise: i32,
    pub poise_broken_tick: i32,
    pub move_id: i32,
    pub move_tick: i32,
    pub cooldown: i32,
    pub attack_hit: bool,
    pub alive: bool,
}

impl Default for EnemyState {
    fn default() -> Self {
        Self {
            x: 0,
            y: 0,
            z: 0,
            facing_x: 0,
            facing_z: 1000,
            health: 0,
            max_health: 150000,
            poise: MAX_POISE,
            poise_broken_tick: 0,
            move_id: 0,
            move_tick: 0,
            cooldown: 0,
            attack_hit: false,
            alive: false,
        }
    }
}

#[derive(Clone)]
pub struct GameInput {
    pub move_x: i32,
    pub move_z: i32,
    pub attack_light: bool,
    pub attack_heavy: bool,
    pub dodge: bool,
    pub parry: bool,
    pub lock_on_toggle: bool,
    pub target_cycle_left: bool,
    pub target_cycle_right: bool,
}

impl Default for GameInput {
    fn default() -> Self {
        Self {
            move_x: 0,
            move_z: 0,
            attack_light: false,
            attack_heavy: false,
            dodge: false,
            parry: false,
            lock_on_toggle: false,
            target_cycle_left: false,
            target_cycle_right: false,
        }
    }
}

#[derive(Clone)]
pub struct GameState {
    // Player
    pub player_x: i32,
    pub player_y: i32,
    pub player_z: i32,
    pub player_facing_x: i32,
    pub player_facing_z: i32,
    pub player_state: i32,
    pub player_state_tick: i32,
    pub player_attack_heavy: bool,
    pub player_health: i32,
    pub player_max_health: i32,
    pub player_stamina: i32,
    pub player_stamina_cooldown: i32,
    pub player_poise: i32,
    pub player_poise_broken_tick: i32,

    // Multi-target combat state (authoritative)
    pub enemies: [EnemyState; MAX_ENEMIES],
    pub enemy_count: usize,
    pub lock_on_target: i32, // -1 = none

    // Readability/combat metadata for HUD/camera
    pub boss_phase: i32,
    pub readability_state: i32,

    // Combat
    pub hit_stop_remaining: i32,
    pub parry_tick: i32,
    pub parry_success_this_tick: bool,

    // Player hit tracking (prevent multi-hit per attack)
    pub player_attack_hit: bool,

    // Resonance
    pub resonance: i32,

    // Camera
    pub camera_x: i32,
    pub camera_y: i32,
    pub camera_z: i32,
    pub camera_jolt_x: i32,
    pub camera_jolt_y: i32,
    pub camera_shake: i32,
    pub camera_desired_distance: i32,

    // Soul Blade
    pub kills: i32,

    // Primary-target cache for current runtime consumers
    pub enemy_x: i32,
    pub enemy_y: i32,
    pub enemy_z: i32,
    pub enemy_facing_x: i32,
    pub enemy_facing_z: i32,
    pub enemy_health: i32,
    pub enemy_max_health: i32,
    pub enemy_poise: i32,
    pub enemy_poise_broken_tick: i32,
    pub enemy_move: i32,
    pub enemy_move_tick: i32,
    pub enemy_cooldown: i32,
    pub enemy_attack_hit: bool,

    pub tick_count: u64,
    pub prev_input: GameInput,
}

impl GameState {
    pub fn new() -> Self {
        let mut enemies = [EnemyState::default(); MAX_ENEMIES];
        enemies[0] = EnemyState {
            x: 3000,
            y: 0,
            z: 3000,
            facing_x: -707,
            facing_z: -707,
            health: 150000,
            max_health: 150000,
            poise: MAX_POISE,
            alive: true,
            ..EnemyState::default()
        };
        enemies[1] = EnemyState {
            x: -2800,
            y: 0,
            z: 2600,
            facing_x: 707,
            facing_z: -707,
            health: 110000,
            max_health: 110000,
            poise: MAX_POISE,
            alive: true,
            cooldown: 24,
            ..EnemyState::default()
        };

        Self {
            player_x: 0,
            player_y: 0,
            player_z: 0,
            player_facing_x: 0,
            player_facing_z: 1000,
            player_state: STATE_IDLE,
            player_state_tick: 0,
            player_attack_heavy: false,
            player_health: 100000,
            player_max_health: 100000,
            player_stamina: MAX_STAMINA,
            player_stamina_cooldown: 0,
            player_poise: MAX_POISE,
            player_poise_broken_tick: 0,

            enemies,
            enemy_count: 2,
            lock_on_target: -1,

            boss_phase: 0,
            readability_state: 0,

            hit_stop_remaining: 0,
            parry_tick: 0,
            parry_success_this_tick: false,
            player_attack_hit: false,

            resonance: 0,

            camera_x: 0,
            camera_y: CAMERA_HEIGHT,
            camera_z: -CAMERA_DISTANCE_EXPLORATION,
            camera_jolt_x: 0,
            camera_jolt_y: 0,
            camera_shake: 0,
            camera_desired_distance: CAMERA_DISTANCE_EXPLORATION,

            kills: 0,

            enemy_x: enemies[0].x,
            enemy_y: enemies[0].y,
            enemy_z: enemies[0].z,
            enemy_facing_x: enemies[0].facing_x,
            enemy_facing_z: enemies[0].facing_z,
            enemy_health: enemies[0].health,
            enemy_max_health: enemies[0].max_health,
            enemy_poise: enemies[0].poise,
            enemy_poise_broken_tick: enemies[0].poise_broken_tick,
            enemy_move: enemies[0].move_id,
            enemy_move_tick: enemies[0].move_tick,
            enemy_cooldown: enemies[0].cooldown,
            enemy_attack_hit: enemies[0].attack_hit,

            tick_count: 0,
            prev_input: GameInput::default(),
        }
    }

    pub fn player_health_ratio(&self) -> f32 {
        self.player_health as f32 / self.player_max_health as f32
    }

    pub fn player_stamina_ratio(&self) -> f32 {
        self.player_stamina as f32 / MAX_STAMINA as f32
    }

    pub fn enemy_health_ratio(&self) -> f32 {
        let idx = self.primary_enemy_index().unwrap_or(0);
        let enemy = self.enemies[idx];
        if enemy.max_health <= 0 {
            0.0
        } else {
            enemy.health.max(0) as f32 / enemy.max_health as f32
        }
    }

    pub fn resonance_tier(&self) -> i32 {
        if self.resonance >= 1000 {
            4
        } else if self.resonance >= 800 {
            3
        } else if self.resonance >= 500 {
            2
        } else if self.resonance >= 200 {
            1
        } else {
            0
        }
    }

    pub fn lock_on_active(&self) -> bool {
        self.lock_on_target >= 0
    }

    pub fn primary_enemy_index(&self) -> Option<usize> {
        if self.lock_on_target >= 0 {
            let idx = self.lock_on_target as usize;
            if idx < self.enemy_count && self.enemies[idx].alive {
                return Some(idx);
            }
        }
        let mut best = None;
        let mut best_dist = i32::MAX;
        for idx in 0..self.enemy_count {
            let e = self.enemies[idx];
            if !e.alive {
                continue;
            }
            let dist = milli_distance(self.player_x, self.player_z, e.x, e.z);
            if dist < best_dist {
                best_dist = dist;
                best = Some(idx);
            }
        }
        best
    }
}

fn milli_distance(ax: i32, az: i32, bx: i32, bz: i32) -> i32 {
    let dx = (ax - bx) as i64;
    let dz = (az - bz) as i64;
    ((dx * dx + dz * dz) as f64).sqrt() as i32
}

fn is_rising_edge(current: bool, previous: bool) -> bool {
    current && !previous
}

fn lock_on_candidate_score(player: (i32, i32), facing: (i32, i32), enemy: EnemyState) -> Option<i32> {
    let dx = enemy.x - player.0;
    let dz = enemy.z - player.1;
    let dist = milli_distance(player.0, player.1, enemy.x, enemy.z).max(1);
    let dir_x = dx * 1000 / dist;
    let dir_z = dz * 1000 / dist;
    let dot = (dir_x * facing.0 + dir_z * facing.1) / 1000;
    if dot < LOCK_ON_CONE_DOT_MIN {
        return None;
    }
    // lower score = better: prioritize high dot and near distance
    Some(dist - dot * 2)
}

fn find_best_lock_on_target(state: &GameState) -> Option<usize> {
    let mut best_idx = None;
    let mut best_score = i32::MAX;
    for idx in 0..state.enemy_count {
        let e = state.enemies[idx];
        if !e.alive {
            continue;
        }
        if let Some(score) = lock_on_candidate_score(
            (state.player_x, state.player_z),
            (state.player_facing_x, state.player_facing_z),
            e,
        ) {
            if score < best_score {
                best_score = score;
                best_idx = Some(idx);
            }
        }
    }
    best_idx
}

fn cycle_lock_on_target(state: &GameState, direction: i32) -> Option<usize> {
    let mut alive = Vec::new();
    for idx in 0..state.enemy_count {
        if state.enemies[idx].alive {
            alive.push(idx);
        }
    }
    if alive.is_empty() {
        return None;
    }

    let current = if state.lock_on_target >= 0 {
        state.lock_on_target as usize
    } else {
        return Some(alive[0]);
    };

    let pos = alive.iter().position(|idx| *idx == current).unwrap_or(0);
    let next = if direction < 0 {
        if pos == 0 { alive.len() - 1 } else { pos - 1 }
    } else {
        (pos + 1) % alive.len()
    };
    Some(alive[next])
}

fn update_primary_enemy_cache(state: &mut GameState) {
    let primary_idx = state.primary_enemy_index().unwrap_or(0);
    let enemy = state.enemies[primary_idx];
    state.enemy_x = enemy.x;
    state.enemy_y = enemy.y;
    state.enemy_z = enemy.z;
    state.enemy_facing_x = enemy.facing_x;
    state.enemy_facing_z = enemy.facing_z;
    state.enemy_health = enemy.health;
    state.enemy_max_health = enemy.max_health;
    state.enemy_poise = enemy.poise;
    state.enemy_poise_broken_tick = enemy.poise_broken_tick;
    state.enemy_move = enemy.move_id;
    state.enemy_move_tick = enemy.move_tick;
    state.enemy_cooldown = enemy.cooldown;
    state.enemy_attack_hit = enemy.attack_hit;
}

/// Run one deterministic game tick. Pure function: (state, input) -> state.
pub fn tick_game(state: &GameState, input: &GameInput) -> GameState {
    let mut s = state.clone();
    s.tick_count += 1;
    s.parry_success_this_tick = false;

    // Lock-on: toggle and target cycle are edge-triggered.
    if is_rising_edge(input.lock_on_toggle, s.prev_input.lock_on_toggle) {
        if s.lock_on_target >= 0 {
            s.lock_on_target = -1;
        } else {
            s.lock_on_target = find_best_lock_on_target(&s).map_or(-1, |idx| idx as i32);
        }
    }
    if is_rising_edge(input.target_cycle_left, s.prev_input.target_cycle_left) {
        s.lock_on_target = cycle_lock_on_target(&s, -1).map_or(-1, |idx| idx as i32);
    }
    if is_rising_edge(input.target_cycle_right, s.prev_input.target_cycle_right) {
        s.lock_on_target = cycle_lock_on_target(&s, 1).map_or(-1, |idx| idx as i32);
    }

    // If hit-stopped, only decay camera and update input history.
    if s.hit_stop_remaining > 0 {
        s.hit_stop_remaining -= 1;
        s.camera_jolt_x = s.camera_jolt_x * 900 / 1000;
        s.camera_jolt_y = s.camera_jolt_y * 900 / 1000;
        s.camera_shake = s.camera_shake * 850 / 1000;
        // break stale lock-ons
        if let Some(idx) = s.primary_enemy_index() {
            let e = s.enemies[idx];
            let dist = milli_distance(s.player_x, s.player_z, e.x, e.z);
            if dist > LOCK_ON_BREAK_RANGE || !e.alive {
                s.lock_on_target = -1;
            }
        } else {
            s.lock_on_target = -1;
        }
        update_primary_enemy_cache(&mut s);
        s.prev_input = input.clone();
        return s;
    }

    // Step 1: player state lifetime
    let total_frames = match s.player_state {
        STATE_ATTACK => {
            if s.player_attack_heavy {
                HEAVY_STARTUP + HEAVY_ACTIVE + HEAVY_RECOVERY
            } else {
                LIGHT_STARTUP + LIGHT_ACTIVE + LIGHT_RECOVERY
            }
        }
        STATE_PARRY_ACTIVE => PARRY_STARTUP + PARRY_ACTIVE + PARRY_RECOVERY,
        STATE_DODGE => DODGE_STARTUP + DODGE_ACTIVE + DODGE_RECOVERY,
        STATE_STAGGER => POISE_BROKEN_DURATION,
        _ => 0,
    };
    if total_frames > 0 && s.player_state_tick >= total_frames {
        s.player_state = STATE_IDLE;
        s.player_state_tick = 0;
        s.player_attack_hit = false;
    }

    // Step 2: transitions from locomotion
    if s.player_state == STATE_IDLE || s.player_state == STATE_WALK || s.player_state == STATE_RUN {
        if is_rising_edge(input.attack_light, s.prev_input.attack_light)
            && s.player_stamina >= LIGHT_STAMINA_COST
        {
            s.player_state = STATE_ATTACK;
            s.player_state_tick = 0;
            s.player_attack_heavy = false;
            s.player_stamina -= LIGHT_STAMINA_COST;
            s.player_stamina_cooldown = STAMINA_REGEN_DELAY;
            s.player_attack_hit = false;
        } else if is_rising_edge(input.attack_heavy, s.prev_input.attack_heavy)
            && s.player_stamina >= HEAVY_STAMINA_COST
        {
            s.player_state = STATE_ATTACK;
            s.player_state_tick = 0;
            s.player_attack_heavy = true;
            s.player_stamina -= HEAVY_STAMINA_COST;
            s.player_stamina_cooldown = STAMINA_REGEN_DELAY;
            s.player_attack_hit = false;
        } else if is_rising_edge(input.parry, s.prev_input.parry)
            && s.player_stamina >= PARRY_STAMINA_COST
        {
            s.player_state = STATE_PARRY_ACTIVE;
            s.player_state_tick = 0;
            s.player_stamina -= PARRY_STAMINA_COST;
            s.player_stamina_cooldown = STAMINA_REGEN_DELAY;
        } else if is_rising_edge(input.dodge, s.prev_input.dodge)
            && s.player_stamina >= DODGE_STAMINA_COST
        {
            s.player_state = STATE_DODGE;
            s.player_state_tick = 0;
            s.player_stamina -= DODGE_STAMINA_COST;
            s.player_stamina_cooldown = STAMINA_REGEN_DELAY;
        }
    }

    // Step 3: movement and facing
    if s.player_state == STATE_IDLE || s.player_state == STATE_WALK || s.player_state == STATE_RUN {
        let mx = input.move_x.clamp(-1000, 1000);
        let mz = input.move_z.clamp(-1000, 1000);
        if mx != 0 || mz != 0 {
            s.player_x += mx * MOVE_SPEED / 1000;
            s.player_z += mz * MOVE_SPEED / 1000;
            s.player_facing_x = mx;
            s.player_facing_z = mz;
            s.player_state = STATE_WALK;
        } else if s.player_state == STATE_WALK {
            s.player_state = STATE_IDLE;
        }
    }

    // Dodge movement
    if s.player_state == STATE_DODGE && s.player_state_tick < DODGE_ACTIVE {
        s.player_x += s.player_facing_x * DODGE_DISTANCE / 1000 / DODGE_ACTIVE;
        s.player_z += s.player_facing_z * DODGE_DISTANCE / 1000 / DODGE_ACTIVE;
    }

    // Clamp lock-on break and facing assist
    if let Some(idx) = s.primary_enemy_index() {
        let e = s.enemies[idx];
        let dist = milli_distance(s.player_x, s.player_z, e.x, e.z);
        if s.lock_on_target >= 0 {
            if dist > LOCK_ON_BREAK_RANGE || !e.alive {
                s.lock_on_target = -1;
            } else {
                let dx = e.x - s.player_x;
                let dz = e.z - s.player_z;
                let mag = milli_distance(0, 0, dx, dz).max(1);
                s.player_facing_x = dx * 1000 / mag;
                s.player_facing_z = dz * 1000 / mag;
            }
        }
    } else {
        s.lock_on_target = -1;
    }

    // Step 4: player active windows
    let player_in_active = s.player_state == STATE_ATTACK && {
        let startup = if s.player_attack_heavy {
            HEAVY_STARTUP
        } else {
            LIGHT_STARTUP
        };
        let active = if s.player_attack_heavy {
            HEAVY_ACTIVE
        } else {
            LIGHT_ACTIVE
        };
        s.player_state_tick >= startup && s.player_state_tick < startup + active
    };

    let parry_window_active = s.player_state == STATE_PARRY_ACTIVE
        && s.player_state_tick >= PARRY_STARTUP
        && s.player_state_tick < PARRY_STARTUP + PARRY_ACTIVE;

    // Step 5: enemy AI tick for all enemies
    let mut any_enemy_active_hit = None::<usize>;
    for idx in 0..s.enemy_count {
        let mut enemy = s.enemies[idx];
        if !enemy.alive {
            continue;
        }

        let dist = milli_distance(s.player_x, s.player_z, enemy.x, enemy.z);

        if enemy.poise_broken_tick > 0 {
            enemy.poise_broken_tick -= 1;
            enemy.move_id = 0;
            enemy.move_tick = 0;
            if enemy.poise_broken_tick == 0 {
                enemy.poise = MAX_POISE;
            }
            s.enemies[idx] = enemy;
            continue;
        }

        if enemy.cooldown > 0 {
            enemy.cooldown -= 1;
        } else if enemy.move_id == 0 {
            if dist < ENEMY_ATTACK_RANGE {
                let dx = s.player_x - enemy.x;
                let dz = s.player_z - enemy.z;
                let mag = milli_distance(0, 0, dx, dz).max(1);
                enemy.facing_x = dx * 1000 / mag;
                enemy.facing_z = dz * 1000 / mag;
                enemy.move_id = ENEMY_MOVE_CLAW;
                enemy.move_tick = 0;
                enemy.attack_hit = false;
            } else if dist < ENEMY_AGGRO_RANGE {
                if dist < ENEMY_ATTACK_RANGE + 500 {
                    let dx = s.player_x - enemy.x;
                    let dz = s.player_z - enemy.z;
                    let mag = milli_distance(0, 0, dx, dz).max(1);
                    enemy.facing_x = dx * 1000 / mag;
                    enemy.facing_z = dz * 1000 / mag;
                    enemy.move_id = ENEMY_MOVE_LUNGE;
                    enemy.move_tick = 0;
                    enemy.attack_hit = false;
                } else {
                    let dx = s.player_x - enemy.x;
                    let dz = s.player_z - enemy.z;
                    let mag = milli_distance(0, 0, dx, dz).max(1);
                    enemy.x += dx * 40 / mag;
                    enemy.z += dz * 40 / mag;
                    enemy.facing_x = dx * 1000 / mag;
                    enemy.facing_z = dz * 1000 / mag;
                }
            }
        }

        let in_active = if enemy.move_id > 0 {
            enemy.move_tick += 1;
            let (startup, active, recovery) = match enemy.move_id {
                ENEMY_MOVE_CLAW => (3, 4, 8),
                ENEMY_MOVE_LUNGE => (4, 5, 10),
                _ => (3, 4, 8),
            };
            let total = startup + active + recovery;
            if enemy.move_tick >= total {
                enemy.move_id = 0;
                enemy.move_tick = 0;
                enemy.cooldown = 20;
                false
            } else {
                enemy.move_tick >= startup && enemy.move_tick < startup + active
            }
        } else {
            false
        };

        if in_active && !enemy.attack_hit {
            any_enemy_active_hit = Some(idx);
        }

        s.enemies[idx] = enemy;
    }

    // Step 6: player hits currently selected primary target
    if player_in_active && !s.player_attack_hit {
        if let Some(target_idx) = s.primary_enemy_index() {
            let enemy = s.enemies[target_idx];
            let dist = milli_distance(s.player_x, s.player_z, enemy.x, enemy.z);
            if dist < ENEMY_ATTACK_RANGE {
                s.player_attack_hit = true;
                let damage = if s.player_attack_heavy { HEAVY_DAMAGE } else { LIGHT_DAMAGE };
                let poise_dmg = if s.player_attack_heavy {
                    HEAVY_POISE_DAMAGE
                } else {
                    LIGHT_POISE_DAMAGE
                };

                let mut target = s.enemies[target_idx];
                target.health = (target.health - damage).max(0);
                target.poise = (target.poise - poise_dmg).max(0);
                if target.poise <= 0 && target.poise_broken_tick <= 0 {
                    target.poise_broken_tick = POISE_BROKEN_DURATION;
                    s.camera_shake = 300;
                }
                if target.health <= 0 {
                    target.alive = false;
                    s.kills += 1;
                    s.resonance = (s.resonance + RESONANCE_KILL_GAIN).min(1000);
                    if s.lock_on_target == target_idx as i32 {
                        s.lock_on_target = -1;
                    }
                }
                s.enemies[target_idx] = target;

                s.hit_stop_remaining = if s.player_attack_heavy {
                    HIT_STOP_HEAVY
                } else {
                    HIT_STOP_LIGHT
                };
                let jolt = if s.player_attack_heavy { 250 } else { 100 };
                s.camera_jolt_x = s.player_facing_x * jolt / 1000;
                s.camera_jolt_y = jolt;
                s.camera_shake = s.camera_shake.max(if s.player_attack_heavy { 150 } else { 50 });
            }
        }
    }

    // Step 7: enemy hit against player (nearest active)
    if let Some(attacker_idx) = any_enemy_active_hit {
        let mut attacker = s.enemies[attacker_idx];
        let dist = milli_distance(s.player_x, s.player_z, attacker.x, attacker.z);
        if !attacker.attack_hit && dist < ENEMY_ATTACK_RANGE + 200 {
            if parry_window_active {
                s.parry_success_this_tick = true;
                attacker.attack_hit = true;
                s.hit_stop_remaining = HIT_STOP_PARRY;
                s.camera_jolt_y = 400;
                s.camera_shake = 200;
                s.resonance = (s.resonance + RESONANCE_PARRY_GAIN).min(1000);
                attacker.poise = (attacker.poise - HEAVY_POISE_DAMAGE).max(0);
                if attacker.poise <= 0 {
                    attacker.poise_broken_tick = POISE_BROKEN_DURATION;
                }
                s.parry_tick = PARRY_COUNTER_WINDOW;
            } else if s.player_state != STATE_DODGE
                || s.player_state_tick < DODGE_STARTUP
                || s.player_state_tick >= DODGE_STARTUP + DODGE_ACTIVE
            {
                attacker.attack_hit = true;
                let damage = match attacker.move_id {
                    ENEMY_MOVE_LUNGE => ENEMY_LUNGE_DAMAGE,
                    _ => ENEMY_CLAW_DAMAGE,
                };
                s.player_health = (s.player_health - damage).max(0);
                s.player_poise -= LIGHT_POISE_DAMAGE;
                if s.player_poise <= 0 {
                    s.player_state = STATE_STAGGER;
                    s.player_state_tick = 0;
                    s.player_poise = MAX_POISE;
                }
                s.hit_stop_remaining = HIT_STOP_LIGHT;
                s.camera_shake = 100;
            }
            s.enemies[attacker_idx] = attacker;
        }
    }

    // Step 8: regen and decay
    if s.player_stamina_cooldown > 0 {
        s.player_stamina_cooldown -= 1;
    } else {
        s.player_stamina = (s.player_stamina + STAMINA_REGEN_RATE).min(MAX_STAMINA);
    }
    if s.player_poise_broken_tick <= 0 {
        s.player_poise = (s.player_poise + POISE_REGEN_RATE).min(MAX_POISE);
    }
    for idx in 0..s.enemy_count {
        let mut enemy = s.enemies[idx];
        if enemy.alive && enemy.poise_broken_tick <= 0 {
            enemy.poise = (enemy.poise + POISE_REGEN_RATE).min(MAX_POISE);
        }
        s.enemies[idx] = enemy;
    }

    s.resonance = (s.resonance - RESONANCE_DECAY).max(0);
    s.player_state_tick += 1;

    // Step 9: camera follow; lock-on uses midpoint framing.
    let mut target_cam_x = s.player_x;
    let target_cam_y = s.player_y + CAMERA_HEIGHT;
    let mut target_cam_z = s.player_z;

    if let Some(idx) = s.primary_enemy_index() {
        let e = s.enemies[idx];
        if s.lock_on_target >= 0 {
            target_cam_x = (s.player_x + e.x) / 2;
            target_cam_z = (s.player_z + e.z) / 2;
            s.camera_desired_distance = CAMERA_DISTANCE_COMBAT;
            s.readability_state = 2;
        } else {
            s.camera_desired_distance = CAMERA_DISTANCE_EXPLORATION;
            s.readability_state = if milli_distance(s.player_x, s.player_z, e.x, e.z) < 6000 {
                1
            } else {
                0
            };
        }
    } else {
        s.camera_desired_distance = CAMERA_DISTANCE_EXPLORATION;
        s.readability_state = 0;
    }

    let target_cam_z = target_cam_z - s.camera_desired_distance;
    s.camera_x += (target_cam_x - s.camera_x) * CAMERA_SPRING_FACTOR / 1000;
    s.camera_y += (target_cam_y - s.camera_y) * CAMERA_SPRING_FACTOR / 1000;
    s.camera_z += (target_cam_z - s.camera_z) * CAMERA_SPRING_FACTOR / 1000;

    // decay jolt/shake
    s.camera_jolt_x = s.camera_jolt_x * 900 / 1000;
    s.camera_jolt_y = s.camera_jolt_y * 900 / 1000;
    s.camera_shake = s.camera_shake * 850 / 1000;

    // Step 10: simple respawn lane for dead enemies.
    if s.tick_count % 300 == 0 {
        for idx in 0..s.enemy_count {
            let mut enemy = s.enemies[idx];
            if !enemy.alive {
                enemy.health = enemy.max_health;
                enemy.poise = MAX_POISE;
                enemy.move_id = 0;
                enemy.move_tick = 0;
                enemy.cooldown = 60 + idx as i32 * 15;
                enemy.poise_broken_tick = 0;
                enemy.attack_hit = false;
                enemy.alive = true;
                s.enemies[idx] = enemy;
            }
        }
    }

    // Boss phase approximation for readability HUD state.
    let alive_count = s
        .enemies
        .iter()
        .take(s.enemy_count)
        .filter(|enemy| enemy.alive)
        .count();
    s.boss_phase = if alive_count >= 2 {
        1
    } else if alive_count == 1 {
        2
    } else {
        3
    };

    update_primary_enemy_cache(&mut s);
    s.prev_input = input.clone();
    s
}

/// Extract rendering data from game state.
pub struct RenderData {
    pub player_pos: [f32; 3],
    pub player_facing: [f32; 2],
    pub player_state: i32,
    pub enemy_pos: [f32; 3],
    pub enemy_facing: [f32; 2],
    pub enemy_health_ratio: f32,
    pub enemy_alive: bool,
    pub enemy_count: u32,
    pub lock_on_active: bool,
    pub lock_on_target_index: i32,
    pub lock_on_target_pos: [f32; 3],
    pub boss_phase: i32,
    pub readability_state: i32,
    pub camera_eye: [f32; 3],
    pub camera_target: [f32; 3],
    pub player_health_ratio: f32,
    pub player_stamina_ratio: f32,
    pub resonance_tier: i32,
    pub resonance_ratio: f32,
    pub hit_stop_active: bool,
    pub camera_shake: f32,
    pub parry_flash: bool,
    pub player_dead: bool,
}

impl GameState {
    pub fn render_data(&self) -> RenderData {
        let m = 1000.0_f32;
        let lock_target = self
            .primary_enemy_index()
            .map(|idx| self.enemies[idx])
            .unwrap_or_default();
        RenderData {
            player_pos: [
                self.player_x as f32 / m,
                self.player_y as f32 / m,
                self.player_z as f32 / m,
            ],
            player_facing: [
                self.player_facing_x as f32 / m,
                self.player_facing_z as f32 / m,
            ],
            player_state: self.player_state,
            enemy_pos: [
                self.enemy_x as f32 / m,
                self.enemy_y as f32 / m,
                self.enemy_z as f32 / m,
            ],
            enemy_facing: [
                self.enemy_facing_x as f32 / m,
                self.enemy_facing_z as f32 / m,
            ],
            enemy_health_ratio: self.enemy_health_ratio(),
            enemy_alive: self.primary_enemy_index().is_some(),
            enemy_count: self
                .enemies
                .iter()
                .take(self.enemy_count)
                .filter(|enemy| enemy.alive)
                .count() as u32,
            lock_on_active: self.lock_on_active(),
            lock_on_target_index: self.lock_on_target,
            lock_on_target_pos: [
                lock_target.x as f32 / m,
                lock_target.y as f32 / m,
                lock_target.z as f32 / m,
            ],
            boss_phase: self.boss_phase,
            readability_state: self.readability_state,
            camera_eye: [
                (self.camera_x + self.camera_jolt_x) as f32 / m,
                (self.camera_y + self.camera_jolt_y) as f32 / m,
                self.camera_z as f32 / m,
            ],
            camera_target: [
                self.player_x as f32 / m,
                (self.player_y + 1000) as f32 / m,
                self.player_z as f32 / m,
            ],
            player_health_ratio: self.player_health_ratio(),
            player_stamina_ratio: self.player_stamina_ratio(),
            resonance_tier: self.resonance_tier(),
            resonance_ratio: self.resonance as f32 / 1000.0,
            hit_stop_active: self.hit_stop_remaining > 0,
            camera_shake: self.camera_shake as f32 / 1000.0,
            parry_flash: self.parry_success_this_tick,
            player_dead: self.player_health <= 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_state_has_multi_target_lane() {
        let s = GameState::new();
        assert_eq!(s.player_state, STATE_IDLE);
        assert_eq!(s.player_health, 100000);
        assert!(s.enemy_count >= 2);
        assert!(s.enemies[0].alive);
        assert_eq!(s.lock_on_target, -1);
    }

    #[test]
    fn movement_updates_player() {
        let s = GameState::new();
        let input = GameInput {
            move_x: 1000,
            ..Default::default()
        };
        let s2 = tick_game(&s, &input);
        assert!(s2.player_x > s.player_x);
        assert_eq!(s2.player_state, STATE_WALK);
    }

    #[test]
    fn lock_on_toggle_acquires_target() {
        let s = GameState::new();
        let s1 = tick_game(
            &s,
            &GameInput {
                lock_on_toggle: true,
                ..Default::default()
            },
        );
        assert!(s1.lock_on_target >= 0);
        let s2 = tick_game(
            &s1,
            &GameInput {
                lock_on_toggle: false,
                ..Default::default()
            },
        );
        assert_eq!(s2.lock_on_target, s1.lock_on_target);
        let s3 = tick_game(
            &s2,
            &GameInput {
                lock_on_toggle: true,
                ..Default::default()
            },
        );
        assert_eq!(s3.lock_on_target, -1);
    }

    #[test]
    fn target_cycle_moves_between_alive_targets() {
        let mut s = GameState::new();
        s.lock_on_target = 0;
        let s2 = tick_game(
            &s,
            &GameInput {
                target_cycle_right: true,
                ..Default::default()
            },
        );
        assert_ne!(s2.lock_on_target, 0);
    }

    #[test]
    fn enemy_iteration_updates_multiple_enemies_in_same_tick() {
        let mut s = GameState::new();
        s.enemy_count = 2;
        s.enemies[0].alive = true;
        s.enemies[0].x = 1200;
        s.enemies[0].z = 0;
        s.enemies[0].cooldown = 0;
        s.enemies[0].move_id = 0;
        s.enemies[1].alive = true;
        s.enemies[1].x = -1200;
        s.enemies[1].z = 0;
        s.enemies[1].cooldown = 0;
        s.enemies[1].move_id = 0;

        let s2 = tick_game(&s, &GameInput::default());
        assert_ne!(s2.enemies[0].move_id, 0, "enemy 0 should pick a move");
        assert_ne!(s2.enemies[1].move_id, 0, "enemy 1 should pick a move");
    }

    #[test]
    fn light_attack_starts_and_spends_stamina() {
        let s = GameState::new();
        let s2 = tick_game(
            &s,
            &GameInput {
                attack_light: true,
                ..Default::default()
            },
        );
        assert_eq!(s2.player_state, STATE_ATTACK);
        assert!(!s2.player_attack_heavy);
        assert!(s2.player_stamina < MAX_STAMINA);
    }

    #[test]
    fn heavy_attack_starts() {
        let s = GameState::new();
        let s2 = tick_game(
            &s,
            &GameInput {
                attack_heavy: true,
                ..Default::default()
            },
        );
        assert_eq!(s2.player_state, STATE_ATTACK);
        assert!(s2.player_attack_heavy);
    }

    #[test]
    fn parry_and_dodge_states_enter() {
        let s = GameState::new();
        let p = tick_game(
            &s,
            &GameInput {
                parry: true,
                ..Default::default()
            },
        );
        assert_eq!(p.player_state, STATE_PARRY_ACTIVE);

        let d = tick_game(
            &s,
            &GameInput {
                dodge: true,
                ..Default::default()
            },
        );
        assert_eq!(d.player_state, STATE_DODGE);
    }

    #[test]
    fn hit_stop_freezes_movement() {
        let mut s = GameState::new();
        s.hit_stop_remaining = 5;
        let old_x = s.player_x;
        let s2 = tick_game(
            &s,
            &GameInput {
                move_x: 1000,
                ..Default::default()
            },
        );
        assert_eq!(s2.player_x, old_x);
        assert_eq!(s2.hit_stop_remaining, 4);
    }

    #[test]
    fn player_hits_primary_target() {
        let mut s = GameState::new();
        s.enemies[0].x = 500;
        s.enemies[0].z = 0;
        s.enemies[0].alive = true;
        s.lock_on_target = 0;
        s.player_state = STATE_ATTACK;
        s.player_state_tick = LIGHT_STARTUP;
        s.player_attack_heavy = false;
        let before = s.enemies[0].health;
        let s2 = tick_game(&s, &GameInput::default());
        assert!(s2.enemies[0].health < before);
        assert!(s2.hit_stop_remaining > 0);
    }

    #[test]
    fn render_data_exposes_lock_on_telemetry() {
        let mut s = GameState::new();
        s.lock_on_target = 0;
        let rd = s.render_data();
        assert!(rd.lock_on_active);
        assert_eq!(rd.lock_on_target_index, 0);
        assert!(rd.enemy_count >= 1);
    }
}
