# Wrela v1: Forest Clearing — First Contact

## Overview

The first playable Wrela experience. A single forest clearing where a Nameless Traveller faces escalating wraith pressure, culminating in an Ancient encounter. Proves the core thesis: mastery is the only progression axis, and resonance drives the world's response to you.

No cutscenes. No dialogue. No tutorial popups. The world teaches through pressure, sound, motion, and consequence.

## Narrative Frame

You wake in a clearing. No memory. No name. A dark forest presses in on all sides. A blade that shouldn't exist — your Soul Blade — is the only thing that's yours.

The wraiths come. They're drawn to the blade's resonance. Small shadow-forms at first — fast, darting, testing. Each act of precision feeds the resonance.

The resonance builds. SURVIVOR to FLOW to EDGE. The world responds — bloom intensifies, chromatic aberration creeps in, particles multiply.

At OVERDRIVE, the forest notices. The ground trembles. An Ancient stirs from the treeline — massive, arboreal, indifferent. It doesn't hate you. It corrects you.

Survive the Ancient, and the clearing remembers you. You are no longer Nameless.

Die, and the forest reclaims.

## Combat System

### Soul Blade (Existing + Extensions)

**Existing (no changes):**
- Light attack: 3/4/8 ticks (startup/active/recovery). 12,000 damage. 8,000 stamina.
- Heavy attack: 5/6/12 ticks. 25,000 damage. 18,000 stamina.
- Dodge: 2/3/10 ticks. I-frames during active. 10,000 stamina.
- Parry: 2/4/16 ticks. Success opens punish window + resonance +200. 5,000 stamina.
- No block. Commit or move.

**New — Ground Combo Chain (3-hit):**
- Combo 1: 3/4/6 ticks (shorter recovery than standalone light). Damage: 12,000.
- Combo 2: 3/5/5 ticks. Damage: 14,400 (1.2x escalation).
- Combo 3 (Finisher): 4/6/12 ticks. Damage: 16,800 (1.4x). Launches small enemies upward.
- Chain window: 8 ticks after recovery start. Miss it = back to idle.
- Reset on: window expiry, getting hit, dodging, returning to idle.

**New — Aerial Combat:**
- Jump: initial velocity 4,500, gravity -300/tick. Air control = 60% ground speed.
- Combo finisher launches wraiths upward (launch velocity 3,000).
- Player can jump to auto-pursue launched enemies.
- Air Attack 1: 2/3/4 ticks. Air Attack 2: 2/3/4 ticks. Air Attack 3: 3/4/6 ticks.
- Max 3 air hits per jump. Keeps enemy aloft.
- Air dodge: once per jump, i-frames, resets momentum.
- Both fall after air combo ends.

**New — Resonance-Driven Spawning:**
- Tier 0-1 (SURVIVOR/FLOW): 1 wraith every 60 ticks.
- Tier 2 (EDGE): wraith pairs every 45 ticks.
- Tier 3 (OVERDRIVE): 3-4 wraiths every 30 ticks.
- Tier 4 (TRANSCENDENT): The Ancient awakens. Wraith spawning stops.
- Resonance decay: 3/tick (existing). Aggression sustains tier; passivity drops it.

## Enemies

### Wraith (Small Enemy)

Predatory shadow-forms drawn to Soul Blade resonance.

- Visual: smoky humanoid silhouette, glowing amber eyes, dark purple-black, 0.6x player scale
- HP: 40,000. Poise: 20,000 (breaks in 2 lights or 1 heavy).
- Scratch: 2/3/5 ticks. 10,000 damage. Close range.
- Lunge: 3/4/8 ticks. 15,000 damage. Medium range leap.
- AI: approach player, attack when in range. No coordination — independent swarm.
- Spawn from forest edge (fade in from shadow particles).

Wraiths are pressure, not individual threats. One is trivial. Five is dangerous.

### The Ancient (Boss)

The forest's corrective agency made manifest.

- Visual: massive tree-creature, gnarled bark armor, glowing green-amber sap veins, moss and roots, 3x player scale
- HP: 300,000. Poise: 200,000.
- Root Slam: 6/8/16 ticks. Ground AoE — roots erupt in wide radius. 30,000 damage. Heavily telegraphed (2-second windup, visible ground cracks).
- Sweep: 4/10/12 ticks. 180-degree horizontal arm sweep. 25,000 damage. Fast for its size.
- Reclamation Pulse: 8/0/20 ticks. Arena compression — the clearing shrinks briefly. 20,000 damage to anything in the compression zone.
- Stagger: poise break = 45-tick buckle (DPS window).
- Defeat: doesn't die. Recedes into treeline. Clearing calms.

The Ancient fight should feel like fighting a natural disaster.

## Art Direction

Dark gothic anime. Desaturated base palette with selective vivid accents (blade glow, wraith eyes, Ancient sap veins). Moonlit clearing — pale blue-white ambient, warm directional light through canopy. Stylized proportions — tall, angular, elegant. Think FFVII Advent Children meets Bleach meets Elden Ring's Erdtree.

### Tripo Asset Manifest

| Asset | Prompt | Rig | Animations | Face Limit |
|---|---|---|---|---|
| traveller | "dark anime swordsman, tall, angular build, hooded cloak, minimal armor, holding a katana-like blade, gothic fantasy style, dark colors with subtle blue accents" | BIPED/MIXAMO | idle, walk, run, slash, jump, hurt, fall | 10,000 |
| soul_blade | "elegant dark katana-sword hybrid, slim blade, ethereal blue glow along the edge, minimal guard, wrapped hilt, gothic fantasy weapon" | none | none | 3,000 |
| wraith | "small dark shadow creature, smoky humanoid silhouette, glowing amber eyes, wispy form, gothic anime style, semi-transparent dark purple-black" | BIPED/MIXAMO | idle, walk, slash, hurt, fall | 4,000 |
| ancient | "massive ancient tree creature, gnarled bark armor, glowing green-amber sap veins, moss and hanging roots, twisted face in bark, gothic dark fantasy, imposing towering figure" | BIPED/MIXAMO | idle, walk, slash, hurt, fall | 15,000 |
| clearing_ground | "dark forest clearing floor, mossy stone and packed earth, scattered dead leaves, faint moonlight, circular shape, gothic fantasy environment" | none | none | 6,000 |
| stone_pillar | "ancient crumbling stone pillar, roots growing through cracks, moss-covered, gothic fantasy ruins, moonlit" | none | none | 3,000 |
| tree_wall | "dense dark forest treeline, twisted ancient trees, fog between trunks, gothic fairy tale style, imposing and claustrophobic" | none | none | 5,000 |

7 assets. ~$20-30 Tripo credits. Each character auto-rigged + animated with PBR textures.

## HUD

Minimal. The world speaks through consequence, not UI.

- **HP**: thin bar, bottom-left. Desaturated green to red. Fades to 30% opacity when full.
- **Stamina**: thinner bar below HP. Dim when full, brightens on use.
- **Resonance**: bottom-right. Tier name in small stylized text with tier-colored glow pulse.
- **Enemy HP**: Ancient only — thin bar at top. Wraiths get no HP bar.
- **Death**: "THE FOREST RECLAIMS" fade to black. "Press R to try again" after 2s.
- **Victory**: Ancient recedes. Silence. After 3s, a Forest Name fades in center screen. Then fade to black.
- No combo counter. No command menu. No items.

## Audio

### Ambient
- Wind through trees (existing) + distant wood creaking + occasional owl
- Forest breathing: low sine pulse matching resonance tier (slower at low, faster at high)

### SFX (extend existing procedural synthesis)
- Combo hits: existing swing + impact, pitch escalates per combo step
- Wraith death: descending filtered noise dissolve
- Ancient footsteps: deep thuds, heavy reverb
- Ancient attacks: creaking wood + ground crack
- Resonance tier-up: subtle crystalline ascending chime

### Music (procedural, resonance-driven)
- **Tier 0-1**: Ambient only. No music. The forest is quiet.
- **Tier 2 (EDGE)**: Low ominous drone fades in. Single sustained bass.
- **Tier 3 (OVERDRIVE)**: Rhythm enters. Slow percussion (~80 BPM). Bass pulse.
- **Tier 4 (TRANSCENDENT) / Ancient**: Full intensity. Dissonant layered saws, driving rhythm (~120 BPM), high-frequency tension. The music IS the forest fighting back.

## VFX

### Existing (keep as-is)
- Blade trail (white/blue during attacks)
- Hit sparks, parry burst
- Screen shake, hit-stop, chromatic aberration
- Resonance visual escalation (bloom, vignette, color grade, particle multiplier)

### New
- **Wraith spawn**: dark smoke particles rising from ground (sphere emitter, black/purple, 0.5s)
- **Wraith death**: dissolve into shadow particles (burst scatter, fade to transparent)
- **Ancient Root Slam**: ground-crack line emitter (earth-tone particles along attack path)
- **Ancient Reclamation Pulse**: ring of green-amber particles expanding outward
- **Resonance tier-up**: brief screen-wide pulse of tier color (0.2s fade)
- **Combo escalation**: hit sparks increase per combo step (15, 25, 40 particles)

## Technical Requirements

### Tripo Pipeline (extend existing adapter)
1. `text_to_model` (pbr=true) — mesh + PBR textures
2. `rig_model` (BIPED, MIXAMO) — skeleton for characters
3. `retarget_animation` — IDLE, WALK, RUN, SLASH, JUMP, HURT, FALL per character
4. `convert_model` (face_limit) — optimize for game use
5. Download GLB, cache with SHA256

### Wrela Language Extensions
- `asset` declaration: add `rig_type`, `rig_spec`, `animations`, `face_limit`, `model_version` fields
- `scene` declaration: already supports entities, lighting, camera

### Client Extensions
- Multi-enemy support (enemy array in GameState, max 8)
- Combo chain state machine
- Jump physics + aerial combat
- Resonance-driven spawn logic
- Ancient boss AI (3 attacks + stagger)
- Lock-on camera (Tab toggle, nearest enemy, directional cycling)
- Extended procedural animations (25+ clips as fallback for Tripo animations)
- Music playback system + procedural battle theme
- 12+ new SFX
- 5 new particle effects

### Setup
- Sign up at tripo3d.ai/api
- Set TRIPO_API_KEY environment variable
- Estimated cost: ~$20-30
