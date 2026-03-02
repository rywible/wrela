/// Rust-native game tick implementing all 14 combat systems from the Wrela domain.
/// This is a direct port of apps/wrela-forest/src/application/game_tick.wr.
/// All values use milli-scaling: 1000 = 1.0 in world space.
/// When the WASM pipeline is ready, this module is swapped for the compiled Wrela output.

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

// Enemy (Rot Stalker)
const ENEMY_AGGRO_RANGE: i32 = 3000;
const ENEMY_ATTACK_RANGE: i32 = 800;
const ENEMY_CLAW_DAMAGE: i32 = 15000;
const ENEMY_LUNGE_DAMAGE: i32 = 20000;
const ENEMY_MOVE_CLAW: i32 = 10;
const ENEMY_MOVE_LUNGE: i32 = 11;

// Camera
const CAMERA_DISTANCE: i32 = 5000; // milli
const CAMERA_HEIGHT: i32 = 3000;
const CAMERA_SPRING_FACTOR: i32 = 100; // milli (0.1 = 10% per tick)

#[derive(Clone)]
pub struct GameInput {
    pub move_x: i32,
    pub move_z: i32,
    pub attack_light: bool,
    pub attack_heavy: bool,
    pub dodge: bool,
    pub parry: bool,
}

impl Default for GameInput {
    fn default() -> Self {
        Self { move_x: 0, move_z: 0, attack_light: false, attack_heavy: false, dodge: false, parry: false }
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

    // Enemy
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

    // Combat
    pub hit_stop_remaining: i32,
    pub parry_tick: i32,
    pub parry_success_this_tick: bool,

    // Player hit tracking (prevent multi-hit per attack)
    pub player_attack_hit: bool,
    pub enemy_attack_hit: bool,

    // Resonance
    pub resonance: i32,

    // Camera
    pub camera_x: i32,
    pub camera_y: i32,
    pub camera_z: i32,
    pub camera_jolt_x: i32,
    pub camera_jolt_y: i32,
    pub camera_shake: i32,

    // Soul Blade
    pub kills: i32,

    pub tick_count: u64,
    pub prev_input: GameInput,
}

impl GameState {
    pub fn new() -> Self {
        Self {
            player_x: 0, player_y: 0, player_z: 0,
            player_facing_x: 0, player_facing_z: 1000,
            player_state: STATE_IDLE, player_state_tick: 0,
            player_attack_heavy: false,
            player_health: 100000, player_max_health: 100000,
            player_stamina: MAX_STAMINA, player_stamina_cooldown: 0,
            player_poise: MAX_POISE, player_poise_broken_tick: 0,

            enemy_x: 3000, enemy_y: 0, enemy_z: 3000,
            enemy_facing_x: -707, enemy_facing_z: -707,
            enemy_health: 150000, enemy_max_health: 150000,
            enemy_poise: MAX_POISE, enemy_poise_broken_tick: 0,
            enemy_move: 0, enemy_move_tick: 0, enemy_cooldown: 0,

            hit_stop_remaining: 0,
            parry_tick: 0, parry_success_this_tick: false,
            player_attack_hit: false, enemy_attack_hit: false,

            resonance: 0,

            camera_x: 0, camera_y: CAMERA_HEIGHT, camera_z: -CAMERA_DISTANCE,
            camera_jolt_x: 0, camera_jolt_y: 0, camera_shake: 0,

            kills: 0,
            tick_count: 0,
            prev_input: GameInput::default(),
        }
    }

    pub fn player_health_ratio(&self) -> f32 { self.player_health as f32 / self.player_max_health as f32 }
    pub fn player_stamina_ratio(&self) -> f32 { self.player_stamina as f32 / MAX_STAMINA as f32 }
    pub fn enemy_health_ratio(&self) -> f32 { self.enemy_health as f32 / self.enemy_max_health as f32 }
    pub fn resonance_tier(&self) -> i32 {
        if self.resonance >= 1000 { 4 }
        else if self.resonance >= 800 { 3 }
        else if self.resonance >= 500 { 2 }
        else if self.resonance >= 200 { 1 }
        else { 0 }
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

/// Run one deterministic game tick. Pure function: (state, input) -> state.
pub fn tick_game(state: &GameState, input: &GameInput) -> GameState {
    let mut s = state.clone();
    s.tick_count += 1;
    s.parry_success_this_tick = false;

    // Step 0: If hit-stopped, just decrement and return
    if s.hit_stop_remaining > 0 {
        s.hit_stop_remaining -= 1;
        s.camera_jolt_x = s.camera_jolt_x * 900 / 1000;
        s.camera_jolt_y = s.camera_jolt_y * 900 / 1000;
        s.camera_shake = s.camera_shake * 850 / 1000;
        s.prev_input = input.clone();
        return s;
    }

    // Step 1: Process character state machine
    let total_frames = match s.player_state {
        STATE_ATTACK => {
            if s.player_attack_heavy { HEAVY_STARTUP + HEAVY_ACTIVE + HEAVY_RECOVERY }
            else { LIGHT_STARTUP + LIGHT_ACTIVE + LIGHT_RECOVERY }
        }
        STATE_PARRY_ACTIVE => PARRY_STARTUP + PARRY_ACTIVE + PARRY_RECOVERY,
        STATE_DODGE => DODGE_STARTUP + DODGE_ACTIVE + DODGE_RECOVERY,
        STATE_STAGGER => POISE_BROKEN_DURATION,
        _ => 0,
    };

    if total_frames > 0 && s.player_state_tick >= total_frames {
        // Action complete, return to idle
        s.player_state = STATE_IDLE;
        s.player_state_tick = 0;
        s.player_attack_hit = false;
    }

    // Transition from idle/walk based on input (rising edge)
    if s.player_state == STATE_IDLE || s.player_state == STATE_WALK || s.player_state == STATE_RUN {
        if is_rising_edge(input.attack_light, s.prev_input.attack_light) && s.player_stamina >= LIGHT_STAMINA_COST {
            s.player_state = STATE_ATTACK;
            s.player_state_tick = 0;
            s.player_attack_heavy = false;
            s.player_stamina -= LIGHT_STAMINA_COST;
            s.player_stamina_cooldown = STAMINA_REGEN_DELAY;
            s.player_attack_hit = false;
        } else if is_rising_edge(input.attack_heavy, s.prev_input.attack_heavy) && s.player_stamina >= HEAVY_STAMINA_COST {
            s.player_state = STATE_ATTACK;
            s.player_state_tick = 0;
            s.player_attack_heavy = true;
            s.player_stamina -= HEAVY_STAMINA_COST;
            s.player_stamina_cooldown = STAMINA_REGEN_DELAY;
            s.player_attack_hit = false;
        } else if is_rising_edge(input.parry, s.prev_input.parry) && s.player_stamina >= PARRY_STAMINA_COST {
            s.player_state = STATE_PARRY_ACTIVE;
            s.player_state_tick = 0;
            s.player_stamina -= PARRY_STAMINA_COST;
            s.player_stamina_cooldown = STAMINA_REGEN_DELAY;
        } else if is_rising_edge(input.dodge, s.prev_input.dodge) && s.player_stamina >= DODGE_STAMINA_COST {
            s.player_state = STATE_DODGE;
            s.player_state_tick = 0;
            s.player_stamina -= DODGE_STAMINA_COST;
            s.player_stamina_cooldown = STAMINA_REGEN_DELAY;
        }
    }

    // Step 2: Movement (only in idle/walk/run)
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

    // Step 3: Check if player attack is in active frames
    let player_in_active = s.player_state == STATE_ATTACK && {
        let startup = if s.player_attack_heavy { HEAVY_STARTUP } else { LIGHT_STARTUP };
        let active = if s.player_attack_heavy { HEAVY_ACTIVE } else { LIGHT_ACTIVE };
        s.player_state_tick >= startup && s.player_state_tick < startup + active
    };

    // Step 4: Check if parry is in active window
    let parry_window_active = s.player_state == STATE_PARRY_ACTIVE
        && s.player_state_tick >= PARRY_STARTUP
        && s.player_state_tick < PARRY_STARTUP + PARRY_ACTIVE;

    // Step 5: Enemy AI tick
    let dist = milli_distance(s.player_x, s.player_z, s.enemy_x, s.enemy_z);

    if s.enemy_health > 0 && s.enemy_poise_broken_tick <= 0 {
        if s.enemy_cooldown > 0 {
            s.enemy_cooldown -= 1;
        } else if s.enemy_move == 0 {
            // Not attacking — consider starting one
            if dist < ENEMY_ATTACK_RANGE {
                // Close range: claw swipe — face the player first
                let dx = s.player_x - s.enemy_x;
                let dz = s.player_z - s.enemy_z;
                let mag = milli_distance(0, 0, dx, dz).max(1);
                s.enemy_facing_x = dx * 1000 / mag;
                s.enemy_facing_z = dz * 1000 / mag;
                s.enemy_move = ENEMY_MOVE_CLAW;
                s.enemy_move_tick = 0;
                s.enemy_attack_hit = false;
            } else if dist < ENEMY_AGGRO_RANGE {
                // Medium range: approach or lunge
                if dist < ENEMY_ATTACK_RANGE + 500 {
                    let dx = s.player_x - s.enemy_x;
                    let dz = s.player_z - s.enemy_z;
                    let mag = milli_distance(0, 0, dx, dz).max(1);
                    s.enemy_facing_x = dx * 1000 / mag;
                    s.enemy_facing_z = dz * 1000 / mag;
                    s.enemy_move = ENEMY_MOVE_LUNGE;
                    s.enemy_move_tick = 0;
                    s.enemy_attack_hit = false;
                } else {
                    // Walk toward player
                    let dx = s.player_x - s.enemy_x;
                    let dz = s.player_z - s.enemy_z;
                    let mag = milli_distance(0, 0, dx, dz).max(1);
                    s.enemy_x += dx * 40 / mag;
                    s.enemy_z += dz * 40 / mag;
                    s.enemy_facing_x = dx * 1000 / mag;
                    s.enemy_facing_z = dz * 1000 / mag;
                }
            }
        }
    }

    // Enemy poise broken stagger
    if s.enemy_poise_broken_tick > 0 {
        s.enemy_poise_broken_tick -= 1;
        s.enemy_move = 0;
        s.enemy_move_tick = 0;
        if s.enemy_poise_broken_tick == 0 {
            s.enemy_poise = MAX_POISE;
        }
    }

    // Advance enemy move
    let enemy_in_active = if s.enemy_move > 0 {
        s.enemy_move_tick += 1;
        let (startup, active, recovery) = match s.enemy_move {
            ENEMY_MOVE_CLAW => (3, 4, 8),
            ENEMY_MOVE_LUNGE => (4, 5, 10),
            _ => (3, 4, 8),
        };
        let total = startup + active + recovery;
        if s.enemy_move_tick >= total {
            s.enemy_move = 0;
            s.enemy_move_tick = 0;
            s.enemy_cooldown = 20;
            false
        } else {
            s.enemy_move_tick >= startup && s.enemy_move_tick < startup + active
        }
    } else {
        false
    };

    // Step 6: Hit detection — player hits enemy
    if player_in_active && !s.player_attack_hit && dist < ENEMY_ATTACK_RANGE {
        s.player_attack_hit = true;
        let damage = if s.player_attack_heavy { HEAVY_DAMAGE } else { LIGHT_DAMAGE };
        let poise_dmg = if s.player_attack_heavy { HEAVY_POISE_DAMAGE } else { LIGHT_POISE_DAMAGE };
        s.enemy_health = (s.enemy_health - damage).max(0);
        s.enemy_poise = (s.enemy_poise - poise_dmg).max(0);

        // Hit stop
        let stop = if s.player_attack_heavy { HIT_STOP_HEAVY } else { HIT_STOP_LIGHT };
        s.hit_stop_remaining = stop;

        // Camera jolt
        let jolt = if s.player_attack_heavy { 250 } else { 100 };
        s.camera_jolt_x = s.player_facing_x * jolt / 1000;
        s.camera_jolt_y = jolt;
        s.camera_shake = if s.player_attack_heavy { 150 } else { 50 };

        // Poise break check
        if s.enemy_poise <= 0 && s.enemy_poise_broken_tick <= 0 {
            s.enemy_poise_broken_tick = POISE_BROKEN_DURATION;
            s.camera_shake = 300;
        }

        // Kill check
        if s.enemy_health <= 0 {
            s.kills += 1;
            s.resonance = (s.resonance + RESONANCE_KILL_GAIN).min(1000);
        }
    }

    // Step 7: Hit detection — enemy hits player
    if enemy_in_active && !s.enemy_attack_hit && dist < ENEMY_ATTACK_RANGE + 200 {
        // Check parry first
        if parry_window_active {
            // Successful parry!
            s.parry_success_this_tick = true;
            s.enemy_attack_hit = true;
            s.hit_stop_remaining = HIT_STOP_PARRY;
            s.camera_jolt_y = 400;
            s.camera_shake = 200;
            s.resonance = (s.resonance + RESONANCE_PARRY_GAIN).min(1000);
            // Stagger the enemy
            s.enemy_poise = (s.enemy_poise - HEAVY_POISE_DAMAGE).max(0);
            if s.enemy_poise <= 0 {
                s.enemy_poise_broken_tick = POISE_BROKEN_DURATION;
            }
            // Parry counter window
            s.parry_tick = PARRY_COUNTER_WINDOW;
        } else if s.player_state != STATE_DODGE
            || s.player_state_tick < DODGE_STARTUP
            || s.player_state_tick >= DODGE_STARTUP + DODGE_ACTIVE
        {
            // Player takes damage (not in dodge i-frames)
            s.enemy_attack_hit = true;
            let damage = match s.enemy_move {
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
    }

    // Step 8: Stamina regen
    if s.player_stamina_cooldown > 0 {
        s.player_stamina_cooldown -= 1;
    } else {
        s.player_stamina = (s.player_stamina + STAMINA_REGEN_RATE).min(MAX_STAMINA);
    }

    // Step 9: Poise regen (when not broken)
    if s.player_poise_broken_tick <= 0 {
        s.player_poise = (s.player_poise + POISE_REGEN_RATE).min(MAX_POISE);
    }
    if s.enemy_poise_broken_tick <= 0 && s.enemy_health > 0 {
        s.enemy_poise = (s.enemy_poise + POISE_REGEN_RATE).min(MAX_POISE);
    }

    // Step 10: Resonance decay
    s.resonance = (s.resonance - RESONANCE_DECAY).max(0);

    // Step 11: Advance state tick
    s.player_state_tick += 1;

    // Step 12: Camera follow (spring arm)
    let target_cam_x = s.player_x;
    let target_cam_y = s.player_y + CAMERA_HEIGHT;
    let target_cam_z = s.player_z - CAMERA_DISTANCE;
    s.camera_x += (target_cam_x - s.camera_x) * CAMERA_SPRING_FACTOR / 1000;
    s.camera_y += (target_cam_y - s.camera_y) * CAMERA_SPRING_FACTOR / 1000;
    s.camera_z += (target_cam_z - s.camera_z) * CAMERA_SPRING_FACTOR / 1000;

    // Decay jolt/shake
    s.camera_jolt_x = s.camera_jolt_x * 900 / 1000;
    s.camera_jolt_y = s.camera_jolt_y * 900 / 1000;
    s.camera_shake = s.camera_shake * 850 / 1000;

    // Step 13: Enemy respawn if dead and enough time passed
    if s.enemy_health <= 0 && s.tick_count % 300 == 0 {
        s.enemy_health = s.enemy_max_health;
        s.enemy_poise = MAX_POISE;
        s.enemy_x = 3000;
        s.enemy_z = 3000;
        s.enemy_move = 0;
        s.enemy_move_tick = 0;
        s.enemy_cooldown = 60;
        s.enemy_poise_broken_tick = 0;
    }

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
        let m = 1000.0_f32; // milli to float
        RenderData {
            player_pos: [self.player_x as f32 / m, self.player_y as f32 / m, self.player_z as f32 / m],
            player_facing: [self.player_facing_x as f32 / m, self.player_facing_z as f32 / m],
            player_state: self.player_state,
            enemy_pos: [self.enemy_x as f32 / m, self.enemy_y as f32 / m, self.enemy_z as f32 / m],
            enemy_facing: [self.enemy_facing_x as f32 / m, self.enemy_facing_z as f32 / m],
            enemy_health_ratio: self.enemy_health_ratio(),
            enemy_alive: self.enemy_health > 0,
            camera_eye: [
                (self.camera_x + self.camera_jolt_x) as f32 / m,
                (self.camera_y + self.camera_jolt_y) as f32 / m,
                self.camera_z as f32 / m,
            ],
            camera_target: [self.player_x as f32 / m, (self.player_y + 1000) as f32 / m, self.player_z as f32 / m],
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
    fn test_initial_state() {
        let s = GameState::new();
        assert_eq!(s.player_state, STATE_IDLE);
        assert_eq!(s.player_health, 100000);
        assert_eq!(s.enemy_health, 150000);
        assert_eq!(s.resonance, 0);
    }

    #[test]
    fn test_movement() {
        let s = GameState::new();
        let input = GameInput { move_x: 1000, move_z: 0, ..Default::default() };
        let s2 = tick_game(&s, &input);
        assert!(s2.player_x > s.player_x);
        assert_eq!(s2.player_state, STATE_WALK);
    }

    #[test]
    fn test_light_attack() {
        let s = GameState::new();
        let input = GameInput { attack_light: true, ..Default::default() };
        let s2 = tick_game(&s, &input);
        assert_eq!(s2.player_state, STATE_ATTACK);
        assert!(!s2.player_attack_heavy);
        assert!(s2.player_stamina < MAX_STAMINA);
    }

    #[test]
    fn test_heavy_attack() {
        let s = GameState::new();
        let input = GameInput { attack_heavy: true, ..Default::default() };
        let s2 = tick_game(&s, &input);
        assert_eq!(s2.player_state, STATE_ATTACK);
        assert!(s2.player_attack_heavy);
    }

    #[test]
    fn test_parry() {
        let s = GameState::new();
        let input = GameInput { parry: true, ..Default::default() };
        let s2 = tick_game(&s, &input);
        assert_eq!(s2.player_state, STATE_PARRY_ACTIVE);
    }

    #[test]
    fn test_dodge() {
        let s = GameState::new();
        let input = GameInput { dodge: true, ..Default::default() };
        let s2 = tick_game(&s, &input);
        assert_eq!(s2.player_state, STATE_DODGE);
    }

    #[test]
    fn test_stamina_regen() {
        let mut s = GameState::new();
        s.player_stamina = 50000;
        s.player_stamina_cooldown = 0;
        let input = GameInput::default();
        let s2 = tick_game(&s, &input);
        assert!(s2.player_stamina > 50000);
    }

    #[test]
    fn test_resonance_decay() {
        let mut s = GameState::new();
        s.resonance = 500;
        let input = GameInput::default();
        let s2 = tick_game(&s, &input);
        assert_eq!(s2.resonance, 500 - RESONANCE_DECAY);
    }

    #[test]
    fn test_hit_stop_freezes_action() {
        let mut s = GameState::new();
        s.hit_stop_remaining = 5;
        let old_x = s.player_x;
        let input = GameInput { move_x: 1000, ..Default::default() };
        let s2 = tick_game(&s, &input);
        assert_eq!(s2.player_x, old_x); // No movement during hit stop
        assert_eq!(s2.hit_stop_remaining, 4);
    }

    #[test]
    fn test_player_hits_nearby_enemy() {
        let mut s = GameState::new();
        s.enemy_x = 500;
        s.enemy_z = 0;
        s.player_state = STATE_ATTACK;
        s.player_state_tick = LIGHT_STARTUP; // In active frames
        s.player_attack_heavy = false;
        let input = GameInput::default();
        let s2 = tick_game(&s, &input);
        assert!(s2.enemy_health < s.enemy_health);
        assert!(s2.hit_stop_remaining > 0);
    }
}
