# Forest Clearing v1 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build the first playable Wrela experience — a dark gothic anime combat arena where a Nameless Traveller fights escalating wraith waves driven by resonance, culminating in an Ancient boss encounter.

**Architecture:** Extend the existing deterministic game tick (`tick_game()` pure function), procedural animation system, DOM-based HUD, Web Audio synthesis, and GPU particle system. Add multi-enemy support, combo chains, aerial combat, resonance-driven spawning, and Tripo API extensions for rigging/animation. All new game logic is unit-tested via the existing `GameState`/`GameInput` pattern.

**Tech Stack:** Rust (edition 2024), wgpu 24, WebAssembly, Web Audio API, DOM HUD, Tripo3D API (reqwest), rowan parser, axum preview server.

---

## Task 1: Extend GameInput + GameState for New Systems

**Files:**
- Modify: `client/src/game_logic.rs:77-84` (GameInput struct)
- Modify: `client/src/game_logic.rs:92-149` (GameState struct)
- Modify: `client/src/game_logic.rs:7-13` (state constants)
- Modify: `client/src/game_logic.rs:15-74` (combat constants)

**Step 1: Add new state constants after line 13**

```rust
const STATE_JUMP: i32 = 7;
const STATE_AIR_IDLE: i32 = 8;
const STATE_AIR_ATTACK: i32 = 9;
const STATE_AIR_DODGE: i32 = 10;
```

**Step 2: Add combo and aerial constants after the existing combat constants block**

```rust
// Combo chain
const COMBO_1_STARTUP: i32 = 3;
const COMBO_1_ACTIVE: i32 = 4;
const COMBO_1_RECOVERY: i32 = 6;
const COMBO_2_STARTUP: i32 = 3;
const COMBO_2_ACTIVE: i32 = 5;
const COMBO_2_RECOVERY: i32 = 5;
const COMBO_3_STARTUP: i32 = 4;
const COMBO_3_ACTIVE: i32 = 6;
const COMBO_3_RECOVERY: i32 = 12;
const COMBO_WINDOW: i32 = 8;
const COMBO_DAMAGE_SCALE_2: i32 = 1200; // 1.2x milli
const COMBO_DAMAGE_SCALE_3: i32 = 1400; // 1.4x milli

// Aerial
const GRAVITY: i32 = -300;
const JUMP_VELOCITY: i32 = 4500;
const AIR_CONTROL_FACTOR: i32 = 600; // 60% of 1000
const GROUND_Y: i32 = 0;
const LAUNCH_VELOCITY: i32 = 3000;
const AIR_ATTACK_1_STARTUP: i32 = 2;
const AIR_ATTACK_1_ACTIVE: i32 = 3;
const AIR_ATTACK_1_RECOVERY: i32 = 4;
const AIR_ATTACK_2_STARTUP: i32 = 2;
const AIR_ATTACK_2_ACTIVE: i32 = 3;
const AIR_ATTACK_2_RECOVERY: i32 = 4;
const AIR_ATTACK_3_STARTUP: i32 = 3;
const AIR_ATTACK_3_ACTIVE: i32 = 4;
const AIR_ATTACK_3_RECOVERY: i32 = 6;
const AIR_DODGE_STARTUP: i32 = 2;
const AIR_DODGE_ACTIVE: i32 = 3;
const AIR_DODGE_RECOVERY: i32 = 8;

// Multi-enemy
const MAX_ENEMIES: usize = 8;
const WRAITH_HP: i32 = 40000;
const WRAITH_POISE: i32 = 20000;
const WRAITH_SCRATCH_DAMAGE: i32 = 10000;
const WRAITH_LUNGE_DAMAGE: i32 = 15000;
const WRAITH_MOVE_SCRATCH: i32 = 20;
const WRAITH_MOVE_LUNGE: i32 = 21;
const WRAITH_SCRATCH_STARTUP: i32 = 2;
const WRAITH_SCRATCH_ACTIVE: i32 = 3;
const WRAITH_SCRATCH_RECOVERY: i32 = 5;
const WRAITH_LUNGE_STARTUP: i32 = 3;
const WRAITH_LUNGE_ACTIVE: i32 = 4;
const WRAITH_LUNGE_RECOVERY: i32 = 8;

const ANCIENT_HP: i32 = 300000;
const ANCIENT_POISE: i32 = 200000;
const ANCIENT_STAGGER_DURATION: i32 = 45;
const ANCIENT_ROOT_SLAM_DAMAGE: i32 = 30000;
const ANCIENT_SWEEP_DAMAGE: i32 = 25000;
const ANCIENT_PULSE_DAMAGE: i32 = 20000;
const ANCIENT_MOVE_ROOT_SLAM: i32 = 30;
const ANCIENT_MOVE_SWEEP: i32 = 31;
const ANCIENT_MOVE_PULSE: i32 = 32;
const ANCIENT_ROOT_SLAM_STARTUP: i32 = 6;
const ANCIENT_ROOT_SLAM_ACTIVE: i32 = 8;
const ANCIENT_ROOT_SLAM_RECOVERY: i32 = 16;
const ANCIENT_SWEEP_STARTUP: i32 = 4;
const ANCIENT_SWEEP_ACTIVE: i32 = 10;
const ANCIENT_SWEEP_RECOVERY: i32 = 12;
const ANCIENT_PULSE_STARTUP: i32 = 8;
const ANCIENT_PULSE_ACTIVE: i32 = 0;
const ANCIENT_PULSE_RECOVERY: i32 = 20;

// Resonance spawn thresholds
const SPAWN_INTERVAL_TIER_0: i32 = 60;
const SPAWN_INTERVAL_TIER_1: i32 = 60;
const SPAWN_INTERVAL_TIER_2: i32 = 45;
const SPAWN_INTERVAL_TIER_3: i32 = 30;
const ANCIENT_SPAWN_TIER: i32 = 4;
```

**Step 3: Add new fields to GameInput (lines 77-84)**

```rust
pub struct GameInput {
    pub move_x: i32,
    pub move_z: i32,
    pub attack_light: bool,
    pub attack_heavy: bool,
    pub dodge: bool,
    pub parry: bool,
    // New
    pub jump: bool,
    pub lock_on_toggle: bool,
}
```

**Step 4: Add EnemyState struct and extend GameState**

Add `EnemyState` struct before `GameState`:
```rust
#[derive(Debug, Clone, Copy, Default)]
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
    pub enemy_type: i32,     // 0 = wraith, 1 = ancient
    pub current_move: i32,
    pub move_tick: i32,
    pub cooldown: i32,
    pub stagger_tick: i32,
    pub y_velocity: i32,
    pub airborne: bool,
    pub alive: bool,
    pub attack_hit: bool,
    pub spawn_tick: i32,     // fade-in timer
}
```

Add new fields to `GameState` (after existing fields, before tick_count):
```rust
    // Aerial
    pub player_y_velocity: i32,
    pub player_airborne: bool,
    pub player_air_combo_count: i32,
    pub player_air_dodge_used: bool,

    // Combo chain
    pub player_combo_step: i32,
    pub player_combo_window: i32,

    // Lock-on
    pub lock_on_target: i32, // -1 = none, else index into enemies

    // Multi-enemy
    pub enemies: [EnemyState; MAX_ENEMIES],
    pub enemy_count: usize,
    pub spawn_timer: i32,
    pub ancient_spawned: bool,

    // Game flow
    pub game_won: bool,
    pub forest_name: i32, // 0 = no name yet, >0 = name index
```

**Step 5: Update `GameState::new()` to initialize new fields**

Initialize enemies array with first 3 wraiths at staggered positions. Set `lock_on_target = -1`. Set `player_combo_step = 0`, etc.

**Step 6: Remove old single-enemy fields**

Delete `enemy_x` through `enemy_cooldown` (lines 111-122), `enemy_attack_hit` (line 131). Replace all references in `tick_game` to use the enemies array instead. This is a hard cutover per CLAUDE.md.

**Step 7: Run tests, verify compilation**

Run: `cargo test -p wrela_client --lib game_logic`
Expected: Some tests will fail because they reference old enemy fields. Fix them in the next task.

**Step 8: Commit**

```bash
git add client/src/game_logic.rs
git commit -m "feat: extend GameState with multi-enemy, combo, aerial, lock-on fields"
```

---

## Task 2: Combo Chain + Aerial Combat Logic

**Files:**
- Modify: `client/src/game_logic.rs` (tick_game function, ~lines 206-491)

**Step 1: Write failing tests for ground combo**

Add to test module:
```rust
#[test]
fn test_ground_combo_3_hit() {
    let mut s = GameState::new();
    s.enemies[0] = EnemyState { x: 500, z: 0, health: WRAITH_HP, max_health: WRAITH_HP, poise: WRAITH_POISE, alive: true, ..Default::default() };
    s.enemy_count = 1;
    let light = GameInput { attack_light: true, ..Default::default() };
    let idle = GameInput::default();

    // Combo 1
    let s = tick_game(&s, &light);
    assert_eq!(s.player_state, STATE_ATTACK);
    assert_eq!(s.player_combo_step, 1);

    // Advance through startup + active + into recovery
    let mut s = s;
    for _ in 0..(COMBO_1_STARTUP + COMBO_1_ACTIVE) {
        s = tick_game(&s, &idle);
    }
    // Now in recovery — chain to combo 2
    let s = tick_game(&s, &light);
    assert_eq!(s.player_combo_step, 2);

    // Advance through combo 2
    for _ in 0..(COMBO_2_STARTUP + COMBO_2_ACTIVE) {
        s = tick_game(&s, &idle);
    }
    // Chain to combo 3 (finisher)
    let s = tick_game(&s, &light);
    assert_eq!(s.player_combo_step, 3);
}

#[test]
fn test_combo_window_expiry() {
    let mut s = GameState::new();
    let light = GameInput { attack_light: true, ..Default::default() };
    let idle = GameInput::default();

    let s = tick_game(&s, &light);
    assert_eq!(s.player_combo_step, 1);

    // Wait through full combo 1 + combo window
    let mut s = s;
    for _ in 0..(COMBO_1_STARTUP + COMBO_1_ACTIVE + COMBO_1_RECOVERY + COMBO_WINDOW + 1) {
        s = tick_game(&s, &idle);
    }
    assert_eq!(s.player_state, STATE_IDLE);
    assert_eq!(s.player_combo_step, 0);
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p wrela_client --lib game_logic::tests::test_ground_combo`
Expected: FAIL

**Step 3: Implement combo chain in tick_game**

In the state machine processing step (Step 1, ~lines 221-267), modify the attack state handling:

When `player_state == STATE_ATTACK`:
- Calculate total frames based on `player_combo_step` (1, 2, or 3) using the corresponding constants
- During recovery phase: if `attack_light` rising edge and `combo_step < 3`, advance to next combo step, reset `state_tick`
- Track `player_combo_window` countdown during recovery
- If window expires without chain input: transition to idle, reset `combo_step = 0`

When starting a new attack from idle:
- Set `player_combo_step = 1`
- Use COMBO_1 frame data

**Step 4: Write failing tests for aerial combat**

```rust
#[test]
fn test_jump_and_gravity() {
    let s = GameState::new();
    let jump = GameInput { jump: true, ..Default::default() };
    let idle = GameInput::default();

    let s = tick_game(&s, &jump);
    assert_eq!(s.player_state, STATE_JUMP);
    assert!(s.player_airborne);
    assert_eq!(s.player_y_velocity, JUMP_VELOCITY);

    // Advance a few ticks — should rise then fall
    let mut s = s;
    for _ in 0..5 {
        s = tick_game(&s, &idle);
    }
    assert!(s.player_y > 0);

    // Eventually lands
    for _ in 0..60 {
        s = tick_game(&s, &idle);
    }
    assert!(!s.player_airborne);
    assert_eq!(s.player_y, GROUND_Y);
}

#[test]
fn test_air_combo_after_launch() {
    let mut s = GameState::new();
    s.enemies[0] = EnemyState { x: 500, z: 0, health: WRAITH_HP, max_health: WRAITH_HP, poise: WRAITH_POISE, alive: true, ..Default::default() };
    s.enemy_count = 1;

    // Get to combo 3 finisher to launch enemy
    // ... (set up state directly for test brevity)
    s.player_state = STATE_ATTACK;
    s.player_combo_step = 3;
    s.player_state_tick = COMBO_3_STARTUP; // in active frames
    s.player_attack_hit = false;

    // Tick to trigger hit — enemy should launch
    let s = tick_game(&s, &GameInput::default());
    assert!(s.enemies[0].airborne);
    assert!(s.enemies[0].y_velocity > 0);
}

#[test]
fn test_air_dodge_once_per_jump() {
    let mut s = GameState::new();
    s.player_airborne = true;
    s.player_y = 2000;
    s.player_y_velocity = 0;
    s.player_state = STATE_AIR_IDLE;

    let dodge = GameInput { dodge: true, ..Default::default() };
    let s = tick_game(&s, &dodge);
    assert_eq!(s.player_state, STATE_AIR_DODGE);
    assert!(s.player_air_dodge_used);

    // Second dodge attempt should fail
    let mut s2 = s;
    s2.player_state = STATE_AIR_IDLE;
    s2.player_state_tick = 0;
    let s3 = tick_game(&s2, &dodge);
    assert_ne!(s3.player_state, STATE_AIR_DODGE);
}
```

**Step 5: Implement jump physics and aerial combat in tick_game**

Add jump initiation: from idle/walk, if `jump` rising edge and grounded, set `STATE_JUMP`, `player_y_velocity = JUMP_VELOCITY`, `player_airborne = true`.

Add gravity step (new step in tick_game before camera): if airborne, `player_y_velocity += GRAVITY`, `player_y += player_y_velocity / 1000`. If `player_y <= GROUND_Y`, land: `player_y = GROUND_Y`, `airborne = false`, `air_combo_count = 0`, `air_dodge_used = false`, state → IDLE.

Add air attack: if airborne and `attack_light` rising edge and `air_combo_count < 3`, enter `STATE_AIR_ATTACK`, increment `air_combo_count`.

Add air dodge: if airborne and `dodge` rising edge and `!air_dodge_used`, enter `STATE_AIR_DODGE`, set `air_dodge_used = true`.

Add combo 3 launch: when combo step 3 hits an enemy, set `enemy.y_velocity = LAUNCH_VELOCITY`, `enemy.airborne = true`.

**Step 6: Run all tests**

Run: `cargo test -p wrela_client --lib game_logic`
Expected: All new + existing tests pass

**Step 7: Commit**

```bash
git add client/src/game_logic.rs
git commit -m "feat: combo chains and aerial combat system"
```

---

## Task 3: Multi-Enemy System + Resonance-Driven Spawning

**Files:**
- Modify: `client/src/game_logic.rs`

**Step 1: Write failing tests for multi-enemy**

```rust
#[test]
fn test_multiple_enemies_take_independent_damage() {
    let mut s = GameState::new();
    s.enemies[0] = EnemyState { x: 500, z: 0, health: WRAITH_HP, max_health: WRAITH_HP, poise: WRAITH_POISE, alive: true, ..Default::default() };
    s.enemies[1] = EnemyState { x: -500, z: 0, health: WRAITH_HP, max_health: WRAITH_HP, poise: WRAITH_POISE, alive: true, ..Default::default() };
    s.enemy_count = 2;

    // Attack facing enemy 0
    s.player_facing_x = 1000;
    s.player_state = STATE_ATTACK;
    s.player_combo_step = 1;
    s.player_state_tick = COMBO_1_STARTUP; // in active frames
    let s = tick_game(&s, &GameInput::default());

    // Only enemy 0 should be hit
    assert!(s.enemies[0].health < WRAITH_HP);
    assert_eq!(s.enemies[1].health, WRAITH_HP);
}

#[test]
fn test_resonance_driven_spawn() {
    let mut s = GameState::new();
    s.enemy_count = 0;
    s.resonance = 500; // Tier 2 = EDGE
    s.spawn_timer = 0;

    let s = tick_game(&s, &GameInput::default());
    // Should spawn wraiths at tier 2 interval
    assert!(s.enemy_count > 0 || s.spawn_timer > 0);
}

#[test]
fn test_ancient_spawns_at_transcendent() {
    let mut s = GameState::new();
    s.enemy_count = 0;
    s.resonance = 1000; // Tier 4 = TRANSCENDENT
    s.ancient_spawned = false;
    s.spawn_timer = 0;

    let s = tick_game(&s, &GameInput::default());
    assert!(s.ancient_spawned);
    // Find the ancient in enemies array
    let ancient = s.enemies.iter().find(|e| e.enemy_type == 1 && e.alive);
    assert!(ancient.is_some());
}
```

**Step 2: Run tests to verify failure**

Run: `cargo test -p wrela_client --lib game_logic::tests::test_multiple`
Expected: FAIL

**Step 3: Implement multi-enemy tick**

Refactor `tick_game` Steps 5-7 (enemy AI + hit detection):

**Enemy AI tick (replacing Step 5):** Iterate `enemies[0..enemy_count]`, for each alive enemy:
- Wraith AI (type 0): approach player, attack when in range (same logic as old Rot Stalker but with wraith constants)
- Ancient AI (type 1): slower movement, cycle between root_slam/sweep/pulse based on cooldown and distance

**Player→Enemy hit detection (replacing Step 6):** Iterate enemies, check range for each, apply damage to first hit (or all in arc for combo finisher).

**Enemy→Player hit detection (replacing Step 7):** Each alive enemy independently checks if its attack connects.

**Resonance-driven spawning (new step):**
- Decrement `spawn_timer`. When it hits 0:
  - If tier < 4: spawn a wraith at random forest edge position. Reset timer to `SPAWN_INTERVAL_TIER_X`.
  - If tier == 4 and `!ancient_spawned`: spawn the Ancient. Set `ancient_spawned = true`. Stop wraith spawning.

**Step 4: Implement wraith and ancient AI**

Wraith AI:
- If distance to player > 800: move toward player at speed 60
- If distance <= 800 and cooldown == 0: choose scratch (70%) or lunge (30%) based on `(tick_count % 10)`
- Apply frame data for chosen move

Ancient AI:
- If distance to player > 2000: move toward player at speed 40
- If distance <= 2000 and cooldown == 0: cycle through root_slam → sweep → pulse
- Root slam: check all positions within radius 1500
- Sweep: check 180-degree arc within radius 1200
- Pulse: check all positions within radius 2000

**Step 5: Implement game end conditions**

Victory: when `ancient_spawned` and the Ancient's health <= 0 (it recedes), set `game_won = true`, assign `forest_name` randomly.
Defeat: existing player_health <= 0 check.

**Step 6: Run all tests**

Run: `cargo test -p wrela_client --lib game_logic`
Expected: PASS

**Step 7: Commit**

```bash
git add client/src/game_logic.rs
git commit -m "feat: multi-enemy system with wraith swarm, ancient boss, resonance spawning"
```

---

## Task 4: Lock-On Camera

**Files:**
- Modify: `client/src/game_logic.rs` (lock-on target selection in tick_game)
- Modify: `client/src/camera_math.rs:191-241` (add lock-on camera mode)

**Step 1: Write failing test for lock-on targeting**

```rust
#[test]
fn test_lock_on_selects_nearest_enemy() {
    let mut s = GameState::new();
    s.enemies[0] = EnemyState { x: 5000, z: 0, alive: true, health: WRAITH_HP, ..Default::default() };
    s.enemies[1] = EnemyState { x: 2000, z: 0, alive: true, health: WRAITH_HP, ..Default::default() };
    s.enemy_count = 2;
    s.lock_on_target = -1;

    let toggle = GameInput { lock_on_toggle: true, ..Default::default() };
    let s = tick_game(&s, &toggle);
    assert_eq!(s.lock_on_target, 1); // enemy[1] is closer
}

#[test]
fn test_lock_on_auto_unlock_on_death() {
    let mut s = GameState::new();
    s.enemies[0] = EnemyState { x: 500, z: 0, alive: true, health: 1, max_health: WRAITH_HP, ..Default::default() };
    s.enemy_count = 1;
    s.lock_on_target = 0;

    // Kill enemy
    s.enemies[0].health = 0;
    s.enemies[0].alive = false;
    let s = tick_game(&s, &GameInput::default());
    assert_eq!(s.lock_on_target, -1);
}
```

**Step 2: Implement lock-on in tick_game**

New step in tick_game (before camera step):
- On `lock_on_toggle` rising edge: if `lock_on_target == -1`, find nearest alive enemy within 8000 range. Else, unlock.
- If locked target is dead or out of range: auto-unlock.
- Movement reorientation: when locked, forward/backward is toward/away from target.

**Step 3: Add lock-on camera to camera_math.rs**

Add to `OrbitCamera`:
```rust
pub fn lock_on_update(&mut self, player_pos: [f32; 3], target_pos: [f32; 3], dt: f32) {
    // Point camera behind player, looking toward target
    let dx = target_pos[0] - player_pos[0];
    let dz = target_pos[2] - player_pos[2];
    let desired_azimuth = dz.atan2(dx) + std::f32::consts::PI;
    // Smooth interpolation
    let spring = 0.1;
    self.azimuth += (desired_azimuth - self.azimuth) * spring;
    self.target = [
        (player_pos[0] + target_pos[0]) * 0.5,
        player_pos[1] + 1.5,
        (player_pos[2] + target_pos[2]) * 0.5,
    ];
}
```

**Step 4: Run tests, commit**

Run: `cargo test -p wrela_client --lib game_logic`

```bash
git add client/src/game_logic.rs client/src/camera_math.rs
git commit -m "feat: lock-on camera with target selection and auto-unlock"
```

---

## Task 5: Procedural Animation Expansion

**Files:**
- Modify: `client/src/procedural_animation.rs`

**Step 1: Write failing test for new clips**

```rust
#[test]
fn test_all_clips_includes_new_combat_clips() {
    let clips = generate_all_clips(15);
    // Should have original 7 + new clips
    assert!(clips.len() >= 18);
    let names: Vec<&str> = clips.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"jump_up"));
    assert!(names.contains(&"air_idle"));
    assert!(names.contains(&"air_attack_1"));
    assert!(names.contains(&"combo_1"));
    assert!(names.contains(&"combo_2"));
    assert!(names.contains(&"combo_3_finisher"));
    assert!(names.contains(&"guard")); // kept as parry stance visual
    assert!(names.contains(&"death"));
    assert!(names.contains(&"run"));
}
```

**Step 2: Run test to verify failure**

Run: `cargo test -p wrela_client --lib procedural_animation::tests::test_all_clips_includes`
Expected: FAIL

**Step 3: Implement new player clips**

Add to `procedural_animation.rs`, following the existing `generate_idle()` pattern:

- `generate_jump_up()` — 0.4s: root translates up, legs tuck (hip/knee rotation)
- `generate_air_idle()` — 0.8s loop: arms spread, legs slightly bent
- `generate_air_attack_1/2/3()` — 0.25-0.4s: R_hand slash variants
- `generate_land()` — 0.3s: legs compress then extend
- `generate_combo_1()` — 0.22s: R_hand right-to-left slash (reuse light pattern, modify direction)
- `generate_combo_2()` — 0.22s: R_hand left-to-right slash (reverse combo_1)
- `generate_combo_3_finisher()` — 0.37s: overhead slam with full body commitment
- `generate_run()` — 0.6s loop: faster walk with more arm swing
- `generate_death()` — 1.0s: collapse backward

**Step 4: Implement wraith clips**

- `generate_wraith_idle()` — 0.5s: fidgety, twitching (head/spine rapid small rotations)
- `generate_wraith_scratch()` — 0.17s: fast forward swipe
- `generate_wraith_lunge()` — 0.25s: leap forward
- `generate_wraith_death()` — 0.3s: collapse inward (all joints contract toward center)

**Step 5: Implement ancient clips**

- `generate_ancient_idle()` — 2.0s: slow sway, heavy breathing
- `generate_ancient_slam()` — 0.5s: arms raise then slam down
- `generate_ancient_sweep()` — 0.43s: horizontal arm sweep
- `generate_ancient_pulse()` — 0.47s: crouch then expand upward
- `generate_ancient_stagger()` — 0.75s: buckle forward
- `generate_ancient_death()` — 1.5s: slow collapse, arms reaching

**Step 6: Update `generate_all_clips()` to include new clips**

Add all new clips to the Vec in `generate_all_clips()`. Also add `generate_wraith_clips()` and `generate_ancient_clips()` convenience functions.

**Step 7: Run tests, commit**

Run: `cargo test -p wrela_client --lib procedural_animation`

```bash
git add client/src/procedural_animation.rs
git commit -m "feat: 25 new procedural animation clips for player, wraith, ancient"
```

---

## Task 6: Audio — Music System + New SFX

**Files:**
- Modify: `client/src/audio.rs`

**Step 1: Add music playback fields to AudioEngine (line 30)**

```rust
pub struct AudioEngine {
    ctx: AudioContext,
    master_gain: GainNode,
    ambient_layers: Vec<AmbientLayer>,
    ambient_running: bool,
    resumed: bool,
    // New
    music_gain: Option<GainNode>,
    music_sources: Vec<OscillatorNode>,
    music_playing: bool,
    music_tier: i32,
}
```

**Step 2: Implement procedural battle theme**

```rust
pub fn update_music_tier(&mut self, tier: i32) {
    if tier == self.music_tier { return; }
    self.music_tier = tier;
    self.stop_music();
    match tier {
        0 | 1 => { /* silence — ambient only */ }
        2 => { self.start_drone(); }
        3 => { self.start_drone(); self.start_rhythm(80.0); }
        4 => { self.start_full_battle_theme(); }
        _ => {}
    }
}
```

- `start_drone()`: sustained low saw wave (C2 = 65Hz), low-pass filtered, volume 0.15
- `start_rhythm(bpm)`: noise bursts via gain envelope at beat intervals
- `start_full_battle_theme()`: layered — bass saw (65Hz cycling C2/D2/E2/G2), dissonant saw pad (detuned +-5Hz), rhythm at 120 BPM, high sine tension (2kHz, tremolo at 4Hz)

**Step 3: Implement new SFX methods**

```rust
pub fn play_jump(&self) { /* sine 200->400Hz, 0.15s */ }
pub fn play_land(&self) { /* filtered noise, 80Hz, 0.1s */ }
pub fn play_combo_hit(&self, step: i32) { /* existing hit impact, pitch *= 1.0 + step * 0.15 */ }
pub fn play_wraith_death(&self) { /* descending filtered noise, 0.5s */ }
pub fn play_ancient_footstep(&self) { /* deep sine 30Hz + noise, 0.3s, heavy reverb */ }
pub fn play_ancient_attack(&self) { /* creaking saw + low sine, 0.5s */ }
pub fn play_resonance_tier_up(&self) { /* ascending sine harmonics 523->659->784Hz, 0.3s */ }
pub fn play_wraith_spawn(&self) { /* low whisper noise, 0.4s */ }
pub fn play_forest_reclaims(&self) { /* deep descending drone, 2s */ }
pub fn play_victory_chime(&self) { /* ascending major triad, 1s */ }
```

**Step 4: Add non-wasm stubs for all new methods**

In the `#[cfg(not(target_arch = "wasm32"))]` block, add empty stubs.

**Step 5: Run tests, commit**

Run: `cargo test -p wrela_client --lib audio`

```bash
git add client/src/audio.rs
git commit -m "feat: procedural battle theme and 10 new SFX"
```

---

## Task 7: VFX — New Particle Effects

**Files:**
- Modify: `client/src/particles.rs:164-175` (CombatParticleEffect enum)
- Modify: `client/src/particles.rs:179+` (emitter_at method)
- Modify: `client/src/vfx.rs`

**Step 1: Add new particle effect variants**

Extend `CombatParticleEffect` enum:
```rust
pub enum CombatParticleEffect {
    HitSparks,
    BladeTrail,
    ParryBurst,
    DodgeDust,
    EnemyHitBlood,
    // New
    WraithSpawn,
    WraithDeath,
    AncientRootSlam,
    AncientPulse,
    ResonanceTierUp { tier: i32 },
    ComboHitSparks { step: i32 },
}
```

**Step 2: Implement emitter params for each new variant**

In `emitter_at()`:
- `WraithSpawn`: Sphere emitter (radius 0.4), black/purple [0.15, 0.05, 0.2, 0.8], 20 particles, speed 1.5, life [0.3, 0.5], gravity [0, 2, 0] (rising smoke)
- `WraithDeath`: Sphere emitter (radius 0.3), dark purple [0.1, 0.02, 0.15, 0.7], 25 particles, speed 4.0, life [0.2, 0.4], gravity [0, -1, 0] (scatter)
- `AncientRootSlam`: Line emitter (ground crack), earth-tone [0.4, 0.3, 0.15, 0.9], 30 particles, speed 6.0, life [0.2, 0.5], gravity [0, -8, 0]
- `AncientPulse`: Sphere emitter (radius 2.0), green-amber [0.3, 0.6, 0.1, 0.7], 40 particles, speed 8.0, life [0.3, 0.6], no gravity
- `ResonanceTierUp { tier }`: Sphere emitter (radius 1.0), tier-colored, 30 particles, speed 3.0, life [0.2, 0.5], gravity [0, 1, 0] (rising)
- `ComboHitSparks { step }`: Same as HitSparks but count = 15 + step * 10

**Step 3: Add resonance tier-up detection to CombatParticleSystem**

Add `prev_resonance_tier: i32` field. In `emit()`, detect tier transitions and spawn `ResonanceTierUp`.

**Step 4: Run tests, commit**

Run: `cargo test -p wrela_client --lib particles`

```bash
git add client/src/particles.rs client/src/vfx.rs
git commit -m "feat: wraith spawn/death, ancient attack, resonance tier-up VFX"
```

---

## Task 8: HUD Overhaul

**Files:**
- Modify: `client/src/hud.rs`

**Step 1: Update HudState struct (line 10)**

```rust
pub struct HudState {
    pub player_health_ratio: f32,
    pub player_stamina_ratio: f32,
    pub enemy_health_ratio: f32, // Ancient only
    pub enemy_alive: bool,       // Ancient alive
    pub resonance_tier: i32,
    pub resonance: i32,
    pub kills: i32,
    pub player_dead: bool,
    // New
    pub game_won: bool,
    pub forest_name: &'static str,
    pub ancient_active: bool,
}
```

**Step 2: Update death overlay text**

In `Hud::create()` (~line 315), change death text from "YOU DIED" to "THE FOREST RECLAIMS".

**Step 3: Add victory overlay**

Add new DOM elements in `create()`:
- `victory_overlay: HtmlElement` — full screen, hidden by default
- Shows Forest Name text (fades in after 3s delay)
- Subtle, no fanfare — just the name appearing

**Step 4: Update enemy HP bar**

Change label from "ROT STALKER" to "THE ANCIENT" when ancient is active. Only show when `ancient_active`.

**Step 5: Remove kill counter**

Remove `kill_text` element and related updates. The forest doesn't count kills.

**Step 6: Update `update()` method to handle new states**

- Show victory overlay when `game_won`
- Show "THE FOREST RECLAIMS" when `player_dead`
- Show enemy bar only when `ancient_active`
- Resonance indicator stays (already perfect for the design)

**Step 7: Commit**

```bash
git add client/src/hud.rs
git commit -m "feat: HUD overhaul — THE FOREST RECLAIMS, victory name, remove kill counter"
```

---

## Task 9: Tripo Pipeline — Rig + Animate Adapters

**Files:**
- Modify: `compiler/asset_factory/tripo.rs`
- Modify: `compiler/asset_factory/mod.rs:19-25` (AssetGenerationRequest)
- Modify: `compiler/scene_ir/mod.rs:25-35` (AssetRef)
- Modify: `compiler/resolve/mod.rs`

**Step 1: Extend AssetGenerationRequest**

Add to struct:
```rust
pub rig_type: Option<String>,      // "biped", "quadruped"
pub rig_spec: Option<String>,      // "mixamo", "tripo"
pub animations: Vec<String>,       // ["idle", "walk", "run", "slash", ...]
pub face_limit: Option<u32>,
pub pbr: bool,
pub model_version: Option<String>,
```

**Step 2: Extend AssetRef in scene_ir**

Add matching fields to `AssetRef`:
```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub rig_type: Option<String>,
#[serde(default, skip_serializing_if = "Option::is_none")]
pub rig_spec: Option<String>,
#[serde(default, skip_serializing_if = "Vec::is_empty")]
pub animations: Vec<String>,
#[serde(default, skip_serializing_if = "Option::is_none")]
pub face_limit: Option<i64>,
#[serde(default, skip_serializing_if = "Option::is_none")]
pub model_version: Option<String>,
```

**Step 3: Add rig_model to TripoMeshAdapter**

New method:
```rust
pub async fn rig_model(&self, original_task_id: &str, spec: &str, rig_type: &str) -> Result<String, String>
```
- POST `/v2/openapi/task` with `{ "type": "animate_rig", "original_model_task_id": id, "spec": spec }`
- Poll until complete
- Return rigged task ID

**Step 4: Add retarget_animation to TripoMeshAdapter**

```rust
pub async fn retarget_animation(&self, rig_task_id: &str, animations: &[String]) -> Result<String, String>
```
- POST `/v2/openapi/task` with `{ "type": "animate_retarget", "original_model_task_id": id, "animations": animations }`
- Poll until complete
- Return animated task ID

**Step 5: Add convert_model to TripoMeshAdapter**

```rust
pub async fn convert_model(&self, task_id: &str, face_limit: u32) -> Result<String, String>
```
- POST with `{ "type": "convert_model", "original_model_task_id": id, "format": "glb", "face_limit": face_limit }`

**Step 6: Create full pipeline method**

```rust
pub async fn generate_full_pipeline(&self, request: &AssetGenerationRequest) -> Result<AssetGenerationResult, String> {
    // 1. text_to_model
    let mesh_result = self.generate(request).await?;
    let mesh_task_id = /* extract from result */;

    // 2. rig if requested
    if let (Some(rig_type), Some(rig_spec)) = (&request.rig_type, &request.rig_spec) {
        let rig_task_id = self.rig_model(&mesh_task_id, rig_spec, rig_type).await?;

        // 3. animate if requested
        if !request.animations.is_empty() {
            let anim_task_id = self.retarget_animation(&rig_task_id, &request.animations).await?;
        }
    }

    // 4. convert/optimize if face_limit set
    if let Some(limit) = request.face_limit {
        self.convert_model(&final_task_id, limit).await?;
    }

    // 5. Download final GLB
    // ... (existing download logic)
}
```

**Step 7: Update resolve pipeline to use full pipeline**

In `compiler/resolve/mod.rs`, change the "mesh" adapter arm to call `generate_full_pipeline` when rig_type/animations are present.

**Step 8: Write tests with mock HTTP**

Mock the rig/animate/convert endpoints same pattern as existing Tripo tests.

**Step 9: Run tests, commit**

Run: `cargo test -p wrela --lib asset_factory`
Run: `cargo test -p wrela --lib resolve`

```bash
git add compiler/asset_factory/tripo.rs compiler/asset_factory/mod.rs compiler/scene_ir/mod.rs compiler/resolve/mod.rs
git commit -m "feat: Tripo full pipeline — rig, animate, convert adapters"
```

---

## Task 10: Wrela Parser — Extended Asset Declaration

**Files:**
- Modify: `compiler/parser/grammar/class.rs` (asset_decl parser)
- Modify: `compiler/parser/kind.rs` (new SyntaxKind variants)
- Modify: `compiler/parser/ast.rs` (new clause accessors)
- Modify: `compiler/hir/lower.rs` (lower new fields)
- Modify: `compiler/hir/def.rs` (HirAssetDecl fields)
- Modify: `compiler/scene_ir/mod.rs` (SceneIR::from_hir carries new fields)

**Step 1: Add new SyntaxKind variants for asset clauses**

In `kind.rs`, add:
```rust
AssetRigTypeClause,
AssetRigSpecClause,
AssetAnimationsClause,
AssetFaceLimitClause,    // if not already exists
AssetModelVersionClause,
```

**Step 2: Extend asset_decl parser to handle new fields**

In `grammar/class.rs`, in the `asset_decl()` function's field-value parsing loop, add cases for:
- `"rig_type"` → parse identifier value, wrap in `AssetRigTypeClause`
- `"rig_spec"` → parse identifier value, wrap in `AssetRigSpecClause`
- `"animations"` → parse comma-separated identifier list, wrap in `AssetAnimationsClause`
- `"face_limit"` → parse integer, wrap in `AssetFaceLimitClause`
- `"model_version"` → parse identifier, wrap in `AssetModelVersionClause`

**Step 3: Add AST accessor methods**

In `ast.rs`, add clause structs and accessor methods following the existing pattern (AssetKindClause, AssetPromptClause, etc.).

**Step 4: Extend HIR lowering**

In `hir/def.rs`, add to `HirAssetDecl`:
```rust
pub rig_type: Option<String>,
pub rig_spec: Option<String>,
pub animations: Vec<String>,
pub face_limit: Option<i64>,
pub model_version: Option<String>,
```

In `hir/lower.rs`, in `lower_asset_decl()`, extract new fields from AST clauses.

**Step 5: Extend SceneIR::from_hir to carry new fields**

In `scene_ir/mod.rs`, `SceneIR::from_hir()` should populate `AssetRef.rig_type`, `rig_spec`, `animations`, `face_limit`, `model_version` from the `HirAssetDecl`.

**Step 6: Write parser test**

```rust
#[test]
fn test_parse_asset_with_rig_and_animations() {
    let src = "asset hero {\n  kind: mesh\n  prompt: \"anime hero\"\n  rig_type: biped\n  rig_spec: mixamo\n  animations: idle, walk, run, slash\n  face_limit: 10000\n}\n";
    let (root, errors) = parse_with_errors(src);
    assert!(errors.is_empty());
    // Verify AST has all clauses
}
```

**Step 7: Run tests, commit**

Run: `cargo test -p wrela --lib parser`
Run: `cargo test -p wrela --lib hir`

```bash
git add compiler/parser/ compiler/hir/ compiler/scene_ir/
git commit -m "feat: extended asset declaration with rig_type, animations, face_limit"
```

---

## Task 11: .wr Scene Declaration for Forest Clearing

**Files:**
- Create: `apps/wrela-forest/src/main.wr`

**Step 1: Write the Wrela source**

```wrela
asset traveller {
    kind: mesh
    prompt: "dark anime swordsman, tall angular build, hooded cloak, minimal armor, holding a katana-like blade, gothic fantasy, dark colors with subtle blue accents"
    rig_type: biped
    rig_spec: mixamo
    animations: idle, walk, run, slash, jump, hurt, fall
    face_limit: 10000
}

asset soul_blade {
    kind: mesh
    prompt: "elegant dark katana-sword hybrid, slim blade, ethereal blue glow along edge, minimal guard, wrapped hilt, gothic fantasy weapon"
    face_limit: 3000
}

asset wraith {
    kind: mesh
    prompt: "small dark shadow creature, smoky humanoid silhouette, glowing amber eyes, wispy form, gothic anime, semi-transparent dark purple-black"
    rig_type: biped
    rig_spec: mixamo
    animations: idle, walk, slash, hurt, fall
    face_limit: 4000
}

asset ancient {
    kind: mesh
    prompt: "massive ancient tree creature, gnarled bark armor, glowing green-amber sap veins, moss and hanging roots, twisted face in bark, gothic dark fantasy, imposing towering figure"
    rig_type: biped
    rig_spec: mixamo
    animations: idle, walk, slash, hurt, fall
    face_limit: 15000
}

asset clearing_ground {
    kind: mesh
    prompt: "dark forest clearing floor, mossy stone and packed earth, scattered dead leaves, faint moonlight, circular shape, gothic fantasy environment"
    face_limit: 6000
}

asset stone_pillar {
    kind: mesh
    prompt: "ancient crumbling stone pillar, roots growing through cracks, moss-covered, gothic fantasy ruins, moonlit"
    face_limit: 3000
}

asset tree_wall {
    kind: mesh
    prompt: "dense dark forest treeline, twisted ancient trees, fog between trunks, gothic fairy tale style, imposing and claustrophobic"
    face_limit: 5000
}

scene forest_clearing {
    entity player {
        mesh: traveller
        position: 0, 0, 0
        rotation: 0, 0, 0
        scale: 1000, 1000, 1000
    }

    entity floor {
        mesh: clearing_ground
        position: 0, -100, 0
        scale: 15000, 1000, 15000
    }

    entity pillar_1 {
        mesh: stone_pillar
        position: 12000, 0, 0
        scale: 1000, 1500, 1000
    }

    entity pillar_2 {
        mesh: stone_pillar
        position: -12000, 0, 0
        scale: 1000, 1500, 1000
    }

    entity pillar_3 {
        mesh: stone_pillar
        position: 0, 0, 12000
        scale: 1000, 1500, 1000
    }

    entity pillar_4 {
        mesh: stone_pillar
        position: 0, 0, -12000
        scale: 1000, 1500, 1000
    }

    entity treeline {
        mesh: tree_wall
        position: 0, 0, 0
        scale: 16000, 3000, 16000
    }

    lighting {
        sun_direction: 300, -800, 400
        sun_color: 180, 190, 220
        sun_intensity: 1500
        ambient_color: 30, 25, 45
        ambient_intensity: 600
    }

    camera {
        mode: orbit
        target: player
        distance: 5000
        pitch: -300
    }
}
```

**Step 2: Verify it compiles**

Run: `cargo run -p wrela -- check apps/wrela-forest/src/main.wr`
Expected: No errors

**Step 3: Commit**

```bash
git add apps/wrela-forest/
git commit -m "feat: forest clearing scene declaration with 7 assets"
```

---

## Task 12: Integration — Wire Everything Into web.rs

**Files:**
- Modify: `client/src/web.rs`

This is the largest integration task. It wires all the new systems together.

**Step 1: Add new keyboard mappings (~line 6099)**

In the keydown handler:
- `Space` → `jump = true` (repurpose from dodge; dodge moves to `Shift`)
- `Shift` → `dodge = true`
- `Tab` → `lock_on_toggle = true`
- Keep existing: `j`=attack_light, `k`=attack_heavy, `l`=parry

In keyup handler: clear the same fields.

**Step 2: Wire multi-enemy rendering (~line 5411)**

Replace single-enemy render with loop over `game_state.enemies[0..enemy_count]`:
- For each alive enemy: update scene node position/rotation
- For dead enemies: move off-screen
- Handle wraith vs ancient scale difference (0.6x vs 3.0x)

**Step 3: Wire new animation clips (~line 2407)**

Extend the state→clip mapping:
```rust
match player_state {
    STATE_IDLE => 0,    // idle
    STATE_WALK => 1,    // walk
    STATE_RUN => clip_index("run"),
    STATE_DODGE => 4,   // dodge
    STATE_ATTACK => match combo_step { 1 => combo_1, 2 => combo_2, 3 => combo_3, _ => light },
    STATE_STAGGER => 6, // hit_stagger
    STATE_PARRY_ACTIVE => 5, // parry
    STATE_JUMP => clip_index("jump_up"),
    STATE_AIR_IDLE => clip_index("air_idle"),
    STATE_AIR_ATTACK => clip_index("air_attack_1"), // + offset by air_combo_count
    STATE_AIR_DODGE => clip_index("air_dodge_uses_dodge"),
    _ => 0,
}
```

**Step 4: Wire audio engine (~line 5463)**

Initialize `AudioEngine` in bootstrap. In `render_frame`:
- Call `audio.update_music_tier(game_state.resonance_tier())`
- On attack hit: `audio.play_combo_hit(combo_step)`
- On jump: `audio.play_jump()`
- On land: `audio.play_land()`
- On wraith death: `audio.play_wraith_death()`
- On resonance tier change: `audio.play_resonance_tier_up()`
- On player death: `audio.play_forest_reclaims()`
- On victory: `audio.play_victory_chime()`

**Step 5: Wire lock-on camera (~line 5429)**

In camera update step:
```rust
if game_state.lock_on_target >= 0 {
    let target_enemy = &game_state.enemies[game_state.lock_on_target as usize];
    orbit_camera.lock_on_update(player_pos, enemy_pos, dt);
} else {
    // existing spring arm follow
}
```

**Step 6: Wire new particle effects**

In particle system update, add spawning for:
- Wraith spawn/death events
- Ancient attack events
- Resonance tier-up events

**Step 7: Update HUD creation and update**

Pass new `HudState` fields: `game_won`, `forest_name`, `ancient_active`.

**Step 8: Run WASM build**

Run: `cargo build -p wrela_client --target wasm32-unknown-unknown`
Expected: Compiles without errors

**Step 9: Commit**

```bash
git add client/src/web.rs
git commit -m "feat: integrate all forest clearing systems into web.rs"
```

---

## Task 13: Playwright Visual Verification

**Files:**
- Test via browser

**Step 1: Build WASM and start preview server**

Run: `cargo build -p wrela_client --target wasm32-unknown-unknown && cargo run -p wrela -- preview apps/wrela-forest/`

**Step 2: Playwright snapshot**

Navigate to `http://127.0.0.1:8080`, verify:
- Canvas renders (WebGPU context active)
- HUD elements exist (HP bar, stamina bar, resonance indicator)
- No "THE FOREST RECLAIMS" showing (player alive)
- Keyboard inputs work (send key events, verify state changes)

**Step 3: Commit any Playwright test files**

---

## Task 14: Asset Generation Run

**Files:** None (runtime task)
**Prerequisite:** `TRIPO_API_KEY` environment variable set

**Step 1: Run resolve**

Run: `cargo run -p wrela -- resolve apps/wrela-forest/scene-ir.json --parallel 2`

This calls Tripo API for all 7 assets:
- 4 characters get: text_to_model → rig_model → retarget_animation → convert_model
- 3 environment meshes get: text_to_model → convert_model

**Step 2: Verify cached GLBs**

Check `.wrela/asset-factory-cache-v1/` for 7 GLB files.

**Step 3: Preview with real assets**

Run: `cargo run -p wrela -- preview apps/wrela-forest/`
Verify Tripo-generated meshes appear in the browser.

---

## Task 15: Final Review

Per CLAUDE.md: launch a review subagent that checks:
1. Correctness — all combat mechanics match design doc
2. Architecture — EnemyState array, combo system, resonance spawning are clean
3. Maintainability — follows milli-scaling, deterministic ticks, procedural animation patterns
4. Performance — O(n) enemy iteration with n<=8, particle budget respected
5. Completeness — all design doc features implemented and tested
6. Test coverage — unit tests for all new game logic, animation clips, particle effects

---

## Parallel Execution Map

```
PHASE 1 (all independent, run 6 subagents):
  Task 1-3: Game Logic (combo + aerial + multi-enemy)  [SEQUENTIAL - shared file]
  Task 4: Lock-On Camera                               [after Task 3]
  Task 5: Procedural Animations                        [INDEPENDENT]
  Task 6: Audio                                        [INDEPENDENT]
  Task 7: VFX/Particles                                [INDEPENDENT]
  Task 9-10: Tripo Pipeline + Parser                   [INDEPENDENT]

Recommended parallel grouping:
  Subagent A: Tasks 1 → 2 → 3 → 4 (game logic, sequential dependency)
  Subagent B: Task 5 (animations)
  Subagent C: Task 6 (audio)
  Subagent D: Task 7 (VFX)
  Subagent E: Tasks 9 → 10 (pipeline + parser)
  Subagent F: Task 8 (HUD)

PHASE 2 (depends on Phase 1):
  Task 11: .wr Scene (after Task 10)
  Task 12: Integration (after ALL Phase 1)

PHASE 3 (sequential):
  Task 13: Playwright (after Task 12)
  Task 14: Asset Generation (after Tasks 11 + 12)
  Task 15: Final Review (after ALL)
```
