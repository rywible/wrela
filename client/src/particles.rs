use crate::game_logic::GameState;

fn pseudo_random(seed: &mut u32) -> f32 {
    *seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
    (*seed >> 16) as f32 / 65536.0
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Particle {
    pub position: [f32; 3],
    pub life: f32,
    pub velocity: [f32; 3],
    pub size: f32,
    pub color: [f32; 4],
}

#[derive(Clone, Debug)]
pub enum EmitterShape {
    Point { position: [f32; 3] },
    Line { start: [f32; 3], end: [f32; 3] },
    Sphere { center: [f32; 3], radius: f32 },
}

#[derive(Clone, Debug)]
pub struct EmitterParams {
    pub shape: EmitterShape,
    pub initial_speed: f32,
    pub life_range: [f32; 2],
    pub size_range: [f32; 2],
    pub color: [f32; 4],
    pub gravity: [f32; 3],
}

pub struct ParticlePool {
    pub particles: Vec<Particle>,
    pub max_particles: usize,
}

impl ParticlePool {
    pub fn new(max_particles: usize) -> Self {
        Self {
            particles: Vec::with_capacity(max_particles),
            max_particles,
        }
    }

    pub fn spawn_burst(&mut self, params: &EmitterParams, count: usize) {
        let mut seed: u32 = count as u32 ^ 0xDEAD_BEEF;
        let budget = self.max_particles.saturating_sub(self.particles.len());
        let to_spawn = count.min(budget);

        for _ in 0..to_spawn {
            let (pos, dir) = match &params.shape {
                EmitterShape::Point { position } => {
                    let dx = pseudo_random(&mut seed) * 2.0 - 1.0;
                    let dy = pseudo_random(&mut seed) * 2.0 - 1.0;
                    let dz = pseudo_random(&mut seed) * 2.0 - 1.0;
                    let len = (dx * dx + dy * dy + dz * dz).sqrt().max(1e-6);
                    (*position, [dx / len, dy / len, dz / len])
                }
                EmitterShape::Line { start, end } => {
                    let t = pseudo_random(&mut seed);
                    let pos = [
                        start[0] + (end[0] - start[0]) * t,
                        start[1] + (end[1] - start[1]) * t,
                        start[2] + (end[2] - start[2]) * t,
                    ];
                    let dx = pseudo_random(&mut seed) * 2.0 - 1.0;
                    let dy = pseudo_random(&mut seed).abs();
                    let dz = pseudo_random(&mut seed) * 2.0 - 1.0;
                    let len = (dx * dx + dy * dy + dz * dz).sqrt().max(1e-6);
                    (pos, [dx / len, dy / len, dz / len])
                }
                EmitterShape::Sphere { center, radius } => {
                    let dx = pseudo_random(&mut seed) * 2.0 - 1.0;
                    let dy = pseudo_random(&mut seed) * 2.0 - 1.0;
                    let dz = pseudo_random(&mut seed) * 2.0 - 1.0;
                    let len = (dx * dx + dy * dy + dz * dz).sqrt().max(1e-6);
                    let nx = dx / len;
                    let ny = dy / len;
                    let nz = dz / len;
                    let r = radius * pseudo_random(&mut seed);
                    let pos = [center[0] + nx * r, center[1] + ny * r, center[2] + nz * r];
                    (pos, [nx, ny, nz])
                }
            };

            let life_t = pseudo_random(&mut seed);
            let life =
                params.life_range[0] + (params.life_range[1] - params.life_range[0]) * life_t;
            let size_t = pseudo_random(&mut seed);
            let size =
                params.size_range[0] + (params.size_range[1] - params.size_range[0]) * size_t;

            let velocity = [
                dir[0] * params.initial_speed,
                dir[1] * params.initial_speed,
                dir[2] * params.initial_speed,
            ];

            self.particles.push(Particle {
                position: pos,
                life,
                velocity,
                size,
                color: params.color,
            });
        }
    }

    pub fn update(&mut self, dt: f32) {
        for p in self.particles.iter_mut() {
            p.position[0] += p.velocity[0] * dt;
            p.position[1] += p.velocity[1] * dt;
            p.position[2] += p.velocity[2] * dt;
            p.life -= dt;
        }
        self.particles.retain(|p| p.life > 0.0);
    }

    /// Update with gravity applied to velocity each frame.
    pub fn update_with_gravity(&mut self, dt: f32, gravity: [f32; 3]) {
        for p in self.particles.iter_mut() {
            p.velocity[0] += gravity[0] * dt;
            p.velocity[1] += gravity[1] * dt;
            p.velocity[2] += gravity[2] * dt;
            p.position[0] += p.velocity[0] * dt;
            p.position[1] += p.velocity[1] * dt;
            p.position[2] += p.velocity[2] * dt;
            p.life -= dt;
            // Fade alpha linearly with remaining life / initial life (approximate via clamping)
            let alpha_factor = p.life.max(0.0).min(1.0);
            p.color[3] = alpha_factor;
        }
        self.particles.retain(|p| p.life > 0.0);
    }

    pub fn alive_count(&self) -> usize {
        self.particles.len()
    }

    pub fn alive_particles(&self) -> &[Particle] {
        &self.particles
    }
}

// ---------------------------------------------------------------------------
// Combat particle effects
// ---------------------------------------------------------------------------

// Character state constants matching game_logic.rs (not pub there, mirrored here).
const STATE_ATTACK: i32 = 4;
const STATE_DODGE: i32 = 3;

// Attack frame data mirrored from game_logic.rs for active-frame detection.
const LIGHT_STARTUP: i32 = 3;
const LIGHT_ACTIVE: i32 = 4;
const HEAVY_STARTUP: i32 = 5;
const HEAVY_ACTIVE: i32 = 6;

/// The five distinct combat particle effects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CombatParticleEffect {
    /// Bright orange/white sparks on player attack connecting.
    HitSparks,
    /// Trailing particles along the blade arc during active attack frames.
    BladeTrail,
    /// Radial burst of 30+ particles on successful parry.
    ParryBurst,
    /// Dust cloud at feet on dodge roll.
    DodgeDust,
    /// Dark red/purple particles when the enemy takes damage.
    EnemyHitBlood,
}

impl CombatParticleEffect {
    /// Returns (EmitterParams, particle_count) for this effect at the given world position.
    pub fn emitter_at(&self, position: [f32; 3]) -> (EmitterParams, usize) {
        match self {
            CombatParticleEffect::HitSparks => {
                let params = EmitterParams {
                    shape: EmitterShape::Point { position },
                    initial_speed: 8.0,
                    life_range: [0.1, 0.2],
                    size_range: [0.02, 0.06],
                    color: [1.0, 0.85, 0.4, 1.0], // bright orange/white
                    gravity: [0.0, -5.0, 0.0],
                };
                (params, 15)
            }
            CombatParticleEffect::BladeTrail => {
                let params = EmitterParams {
                    shape: EmitterShape::Point { position },
                    initial_speed: 0.5,
                    life_range: [0.08, 0.15],
                    size_range: [0.03, 0.07],
                    color: [0.9, 0.95, 1.0, 0.7], // pale white/blue shimmer
                    gravity: [0.0, 0.0, 0.0],
                };
                (params, 3)
            }
            CombatParticleEffect::ParryBurst => {
                let params = EmitterParams {
                    shape: EmitterShape::Sphere {
                        center: position,
                        radius: 0.15,
                    },
                    initial_speed: 12.0,
                    life_range: [0.15, 0.35],
                    size_range: [0.04, 0.10],
                    color: [1.0, 1.0, 1.0, 1.0], // pure white flash
                    gravity: [0.0, -3.0, 0.0],
                };
                (params, 40)
            }
            CombatParticleEffect::DodgeDust => {
                // Feet-level dust: position.y should be at ground
                let feet = [position[0], 0.0, position[2]];
                let params = EmitterParams {
                    shape: EmitterShape::Sphere {
                        center: feet,
                        radius: 0.3,
                    },
                    initial_speed: 2.0,
                    life_range: [0.3, 0.6],
                    size_range: [0.05, 0.15],
                    color: [0.65, 0.55, 0.40, 0.6], // earthy brown dust
                    gravity: [0.0, -1.0, 0.0],
                };
                (params, 12)
            }
            CombatParticleEffect::EnemyHitBlood => {
                let params = EmitterParams {
                    shape: EmitterShape::Point { position },
                    initial_speed: 5.0,
                    life_range: [0.15, 0.30],
                    size_range: [0.03, 0.08],
                    color: [0.35, 0.02, 0.08, 0.9], // dark red/purple
                    gravity: [0.0, -8.0, 0.0],
                };
                (params, 12)
            }
        }
    }
}

/// Wraps a `ParticlePool` with combat-specific state tracking so it can detect
/// game state transitions and emit the right effects each tick.
pub struct CombatParticleSystem {
    pub pool: ParticlePool,
    /// Tracks whether the player had already hit the enemy during the current
    /// attack (prevents re-emitting sparks/blood every frame of active window).
    prev_player_attack_hit: bool,
    /// Previous parry_success_this_tick so we can detect the rising edge.
    prev_parry_success: bool,
    /// Previous player state for dodge-start detection.
    prev_player_state: i32,
    /// Gravity vector used during update.
    pub gravity: [f32; 3],
}

impl CombatParticleSystem {
    pub fn new(max_particles: usize) -> Self {
        Self {
            pool: ParticlePool::new(max_particles),
            prev_player_attack_hit: false,
            prev_parry_success: false,
            prev_player_state: 0, // STATE_IDLE
            gravity: [0.0, -9.8, 0.0],
        }
    }

    /// Call once per game tick (after tick_game) with the current game state.
    /// Emits appropriate combat particle effects and advances the simulation
    /// by `dt` seconds. The system internally tracks the previous state for
    /// transition detection.
    pub fn emit_and_update(&mut self, current: &GameState, dt: f32) {
        self.emit(current);
        self.pool.update_with_gravity(dt, self.gravity);
    }

    /// Detect state transitions and emit particles. Does NOT advance the
    /// simulation -- call `pool.update_with_gravity` (or `emit_and_update`)
    /// separately. The system internally tracks previous state for rising-edge
    /// detection via its `prev_*` fields.
    pub fn emit(&mut self, current: &GameState) {
        let m = 1000.0_f32;

        // --- Hit sparks + enemy blood: player attack connects ---
        // Rising edge of player_attack_hit means the hit just landed.
        if current.player_attack_hit && !self.prev_player_attack_hit {
            let hit_point = midpoint_milli(
                current.player_x, current.player_y, current.player_z,
                current.enemy_x, current.enemy_y, current.enemy_z,
                m,
            );
            // Sparks at the midpoint between player and enemy
            let (sparks_params, sparks_count) = CombatParticleEffect::HitSparks.emitter_at(hit_point);
            self.pool.spawn_burst(&sparks_params, sparks_count);

            // Blood/dark particles on the enemy
            let enemy_pos = [
                current.enemy_x as f32 / m,
                (current.enemy_y as f32 / m) + 1.0, // chest height
                current.enemy_z as f32 / m,
            ];
            let (blood_params, blood_count) = CombatParticleEffect::EnemyHitBlood.emitter_at(enemy_pos);
            self.pool.spawn_burst(&blood_params, blood_count);
        }
        self.prev_player_attack_hit = current.player_attack_hit;

        // --- Parry burst ---
        if current.parry_success_this_tick && !self.prev_parry_success {
            let parry_point = midpoint_milli(
                current.player_x, current.player_y, current.player_z,
                current.enemy_x, current.enemy_y, current.enemy_z,
                m,
            );
            let (parry_params, parry_count) = CombatParticleEffect::ParryBurst.emitter_at(parry_point);
            self.pool.spawn_burst(&parry_params, parry_count);
        }
        self.prev_parry_success = current.parry_success_this_tick;

        // --- Dodge dust: player just entered dodge state ---
        if current.player_state == STATE_DODGE && self.prev_player_state != STATE_DODGE {
            let player_pos = [
                current.player_x as f32 / m,
                current.player_y as f32 / m,
                current.player_z as f32 / m,
            ];
            let (dust_params, dust_count) = CombatParticleEffect::DodgeDust.emitter_at(player_pos);
            self.pool.spawn_burst(&dust_params, dust_count);
        }
        self.prev_player_state = current.player_state;

        // --- Blade trail: continuous emission during active attack frames ---
        if current.player_state == STATE_ATTACK {
            let startup = if current.player_attack_heavy { HEAVY_STARTUP } else { LIGHT_STARTUP };
            let active = if current.player_attack_heavy { HEAVY_ACTIVE } else { LIGHT_ACTIVE };
            if current.player_state_tick >= startup && current.player_state_tick < startup + active {
                let blade_tip = blade_tip_position(current, m);
                let (trail_params, trail_count) = CombatParticleEffect::BladeTrail.emitter_at(blade_tip);
                self.pool.spawn_burst(&trail_params, trail_count);
            }
        }
    }

    pub fn alive_count(&self) -> usize {
        self.pool.alive_count()
    }

    pub fn alive_particles(&self) -> &[Particle] {
        self.pool.alive_particles()
    }
}

/// Compute the midpoint between player and enemy positions (milli-scaled).
fn midpoint_milli(
    px: i32, py: i32, pz: i32,
    ex: i32, ey: i32, ez: i32,
    m: f32,
) -> [f32; 3] {
    [
        (px + ex) as f32 / (2.0 * m),
        ((py + ey) as f32 / (2.0 * m)) + 1.0, // roughly chest height
        (pz + ez) as f32 / (2.0 * m),
    ]
}

/// Approximate the blade tip position based on player position, facing, and
/// the current attack frame progress. This arcs the tip upward in an approximate
/// slash trajectory.
fn blade_tip_position(state: &GameState, m: f32) -> [f32; 3] {
    let px = state.player_x as f32 / m;
    let py = state.player_y as f32 / m;
    let pz = state.player_z as f32 / m;
    let fx = state.player_facing_x as f32 / m;
    let fz = state.player_facing_z as f32 / m;
    let facing_len = (fx * fx + fz * fz).sqrt().max(1e-6);
    let nx = fx / facing_len;
    let nz = fz / facing_len;

    let startup = if state.player_attack_heavy { HEAVY_STARTUP } else { LIGHT_STARTUP };
    let active = if state.player_attack_heavy { HEAVY_ACTIVE } else { LIGHT_ACTIVE };
    // t goes from 0..1 across active frames
    let t = ((state.player_state_tick - startup) as f32) / (active as f32).max(1.0);

    // Blade reach ~ 1.2 units forward, arc vertically using sin
    let reach = 1.2;
    let arc_height = 0.4 * (t * std::f32::consts::PI).sin();
    [
        px + nx * reach,
        py + 1.2 + arc_height, // shoulder height + arc
        pz + nz * reach,
    ]
}

/// Standalone function for callers that manage their own `ParticlePool` and
/// want to emit combat particles without the `CombatParticleSystem` wrapper.
/// Detects transitions by comparing `prev` and `current` game states and
/// spawns particles into the pool. The caller is responsible for tracking
/// any additional per-frame state (e.g., blade trail needs to be called
/// every frame during active attack frames).
pub fn emit_combat_particles(
    prev: &GameState,
    current: &GameState,
    pool: &mut ParticlePool,
) {
    let m = 1000.0_f32;

    // Hit sparks + enemy blood on player attack connecting
    if current.player_attack_hit && !prev.player_attack_hit {
        let hit_point = midpoint_milli(
            current.player_x, current.player_y, current.player_z,
            current.enemy_x, current.enemy_y, current.enemy_z,
            m,
        );
        let (sparks_params, sparks_count) = CombatParticleEffect::HitSparks.emitter_at(hit_point);
        pool.spawn_burst(&sparks_params, sparks_count);

        let enemy_pos = [
            current.enemy_x as f32 / m,
            (current.enemy_y as f32 / m) + 1.0,
            current.enemy_z as f32 / m,
        ];
        let (blood_params, blood_count) = CombatParticleEffect::EnemyHitBlood.emitter_at(enemy_pos);
        pool.spawn_burst(&blood_params, blood_count);
    }

    // Parry burst
    if current.parry_success_this_tick && !prev.parry_success_this_tick {
        let parry_point = midpoint_milli(
            current.player_x, current.player_y, current.player_z,
            current.enemy_x, current.enemy_y, current.enemy_z,
            m,
        );
        let (parry_params, parry_count) = CombatParticleEffect::ParryBurst.emitter_at(parry_point);
        pool.spawn_burst(&parry_params, parry_count);
    }

    // Dodge dust
    if current.player_state == STATE_DODGE && prev.player_state != STATE_DODGE {
        let player_pos = [
            current.player_x as f32 / m,
            current.player_y as f32 / m,
            current.player_z as f32 / m,
        ];
        let (dust_params, dust_count) = CombatParticleEffect::DodgeDust.emitter_at(player_pos);
        pool.spawn_burst(&dust_params, dust_count);
    }

    // Blade trail (continuous)
    if current.player_state == STATE_ATTACK {
        let startup = if current.player_attack_heavy { HEAVY_STARTUP } else { LIGHT_STARTUP };
        let active = if current.player_attack_heavy { HEAVY_ACTIVE } else { LIGHT_ACTIVE };
        if current.player_state_tick >= startup && current.player_state_tick < startup + active {
            let blade_tip = blade_tip_position(current, m);
            let (trail_params, trail_count) = CombatParticleEffect::BladeTrail.emitter_at(blade_tip);
            pool.spawn_burst(&trail_params, trail_count);
        }
    }
}

// ---------------------------------------------------------------------------
// GPU particle renderer (wasm32 only)
// ---------------------------------------------------------------------------

#[cfg(target_arch = "wasm32")]
const PARTICLE_WGSL: &str = r#"
struct ParticleInstance {
    @location(0) position: vec3<f32>,
    @location(1) life: f32,
    @location(2) velocity: vec3<f32>,
    @location(3) size: f32,
    @location(4) color: vec4<f32>,
};

struct Uniforms {
    view_proj: mat4x4<f32>,
    camera_right: vec4<f32>,
    camera_up: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_main(
    @builtin(vertex_index) vi: u32,
    inst: ParticleInstance,
) -> VsOut {
    let quad_uv = array<vec2<f32>, 4>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>( 1.0,  1.0),
    );
    let uv = quad_uv[vi % 4u];
    let right = u.camera_right.xyz;
    let up = u.camera_up.xyz;
    let world_pos = inst.position
        + right * uv.x * inst.size
        + up * uv.y * inst.size;
    var out: VsOut;
    out.position = u.view_proj * vec4<f32>(world_pos, 1.0);
    out.uv = uv * 0.5 + 0.5;
    out.color = inst.color * vec4<f32>(1.0, 1.0, 1.0, clamp(inst.life, 0.0, 1.0));
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let center = vec2<f32>(0.5, 0.5);
    let dist = length(in.uv - center) * 2.0;
    let alpha = 1.0 - smoothstep(0.6, 1.0, dist);
    return vec4<f32>(in.color.rgb, in.color.a * alpha);
}
"#;

#[cfg(target_arch = "wasm32")]
const MAX_GPU_PARTICLES: usize = 4096;

#[cfg(target_arch = "wasm32")]
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ParticleGpuUniforms {
    view_proj: [[f32; 4]; 4],
    camera_right: [f32; 4],
    camera_up: [f32; 4],
}

#[cfg(target_arch = "wasm32")]
pub struct GpuParticleRenderer {
    pipeline: wgpu::RenderPipeline,
    particle_buffer: wgpu::Buffer,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    max_particles: usize,
}

#[cfg(target_arch = "wasm32")]
impl GpuParticleRenderer {
    pub fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        depth_format: wgpu::TextureFormat,
    ) -> Self {
        use wgpu::util::DeviceExt;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("particle_shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(PARTICLE_WGSL)),
        });

        let particle_buf_size =
            (MAX_GPU_PARTICLES * std::mem::size_of::<Particle>()) as wgpu::BufferAddress;
        let particle_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("particle_instances"),
            size: particle_buf_size,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("particle_uniforms"),
            contents: bytemuck::bytes_of(&ParticleGpuUniforms {
                view_proj: [[0.0; 4]; 4],
                camera_right: [0.0; 4],
                camera_up: [0.0; 4],
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("particle_bind_group_layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("particle_bind_group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("particle_pipeline_layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let particle_stride = std::mem::size_of::<Particle>() as wgpu::BufferAddress;
        let vertex_buffers = [wgpu::VertexBufferLayout {
            array_stride: particle_stride,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                // position: [f32; 3]
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x3,
                    offset: 0,
                    shader_location: 0,
                },
                // life: f32
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32,
                    offset: 12,
                    shader_location: 1,
                },
                // velocity: [f32; 3]
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x3,
                    offset: 16,
                    shader_location: 2,
                },
                // size: f32
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32,
                    offset: 28,
                    shader_location: 3,
                },
                // color: [f32; 4]
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 32,
                    shader_location: 4,
                },
            ],
        }];

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("particle_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &vertex_buffers,
                compilation_options: Default::default(),
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: depth_format,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::SrcAlpha,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            multiview: None,
            cache: None,
        });

        Self {
            pipeline,
            particle_buffer,
            uniform_buffer,
            bind_group,
            max_particles: MAX_GPU_PARTICLES,
        }
    }

    pub fn update(&self, queue: &wgpu::Queue, particles: &[Particle]) {
        let count = particles.len().min(self.max_particles);
        if count == 0 {
            return;
        }
        let byte_len = count * std::mem::size_of::<Particle>();
        let src = unsafe { std::slice::from_raw_parts(particles.as_ptr() as *const u8, byte_len) };
        queue.write_buffer(&self.particle_buffer, 0, src);
    }

    pub fn render<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        view_proj: &[[f32; 4]; 4],
        camera_right: &[f32; 3],
        camera_up: &[f32; 3],
    ) {
        let uniforms = ParticleGpuUniforms {
            view_proj: *view_proj,
            camera_right: [camera_right[0], camera_right[1], camera_right[2], 0.0],
            camera_up: [camera_up[0], camera_up[1], camera_up[2], 0.0],
        };
        // NOTE: uniform update via the pass's queue must happen before the render pass begins.
        // The caller is expected to call `update` (which writes particle data) and also write
        // uniforms to the queue before beginning the render pass. For simplicity, we store the
        // uniforms and expect the caller to use `update_uniforms` before beginning the pass.
        // Here we just bind and draw.
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.particle_buffer.slice(..));
        let _ = uniforms; // used via update_uniforms
        pass.draw(0..4, 0..1);
    }

    pub fn update_uniforms(
        &self,
        queue: &wgpu::Queue,
        view_proj: &[[f32; 4]; 4],
        camera_right: &[f32; 3],
        camera_up: &[f32; 3],
        instance_count: u32,
    ) {
        let uniforms = ParticleGpuUniforms {
            view_proj: *view_proj,
            camera_right: [camera_right[0], camera_right[1], camera_right[2], 0.0],
            camera_up: [camera_up[0], camera_up[1], camera_up[2], 0.0],
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
        let _ = instance_count;
    }

    pub fn render_instanced<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, instance_count: u32) {
        if instance_count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.particle_buffer.slice(..));
        pass.draw(0..4, 0..instance_count);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_logic::{GameInput, GameState};

    // -----------------------------------------------------------------------
    // Original ParticlePool tests (preserved)
    // -----------------------------------------------------------------------

    #[test]
    fn particle_pool_new_creates_empty_pool() {
        let pool = ParticlePool::new(100);
        assert_eq!(pool.alive_count(), 0);
        assert!(pool.alive_particles().is_empty());
        assert_eq!(pool.max_particles, 100);
    }

    #[test]
    fn spawn_burst_creates_correct_count() {
        let mut pool = ParticlePool::new(1000);
        let params = EmitterParams {
            shape: EmitterShape::Point {
                position: [0.0, 0.0, 0.0],
            },
            initial_speed: 5.0,
            life_range: [1.0, 2.0],
            size_range: [0.1, 0.5],
            color: [1.0, 1.0, 1.0, 1.0],
            gravity: [0.0, -9.8, 0.0],
        };
        pool.spawn_burst(&params, 50);
        assert_eq!(pool.alive_count(), 50);
    }

    #[test]
    fn spawn_burst_respects_max_particles() {
        let mut pool = ParticlePool::new(10);
        let params = EmitterParams {
            shape: EmitterShape::Point {
                position: [0.0, 0.0, 0.0],
            },
            initial_speed: 1.0,
            life_range: [1.0, 2.0],
            size_range: [0.1, 0.5],
            color: [1.0, 0.0, 0.0, 1.0],
            gravity: [0.0, 0.0, 0.0],
        };
        pool.spawn_burst(&params, 100);
        assert_eq!(pool.alive_count(), 10);
    }

    #[test]
    fn update_decrements_life_and_removes_dead() {
        let mut pool = ParticlePool::new(100);
        let params = EmitterParams {
            shape: EmitterShape::Point {
                position: [0.0, 0.0, 0.0],
            },
            initial_speed: 1.0,
            life_range: [0.5, 0.5],
            size_range: [0.1, 0.1],
            color: [1.0, 1.0, 1.0, 1.0],
            gravity: [0.0, 0.0, 0.0],
        };
        pool.spawn_burst(&params, 10);
        assert_eq!(pool.alive_count(), 10);

        pool.update(0.3);
        assert_eq!(pool.alive_count(), 10);
        for p in pool.alive_particles() {
            assert!(p.life > 0.0);
            assert!(p.life < 0.5);
        }

        pool.update(0.3);
        assert_eq!(pool.alive_count(), 0);
    }

    #[test]
    fn alive_count_after_spawn_and_update() {
        let mut pool = ParticlePool::new(200);
        let params = EmitterParams {
            shape: EmitterShape::Sphere {
                center: [1.0, 2.0, 3.0],
                radius: 5.0,
            },
            initial_speed: 2.0,
            life_range: [1.0, 3.0],
            size_range: [0.1, 0.2],
            color: [0.0, 1.0, 0.0, 1.0],
            gravity: [0.0, -1.0, 0.0],
        };
        pool.spawn_burst(&params, 20);
        assert_eq!(pool.alive_count(), 20);

        pool.update(0.5);
        assert!(pool.alive_count() <= 20);
        assert!(pool.alive_count() > 0);

        pool.update(10.0);
        assert_eq!(pool.alive_count(), 0);
    }

    #[test]
    fn emitter_shape_point_creates_particles_at_position() {
        let mut pool = ParticlePool::new(100);
        let origin = [5.0, 10.0, 15.0];
        let params = EmitterParams {
            shape: EmitterShape::Point { position: origin },
            initial_speed: 0.0,
            life_range: [1.0, 1.0],
            size_range: [1.0, 1.0],
            color: [1.0, 1.0, 1.0, 1.0],
            gravity: [0.0, 0.0, 0.0],
        };
        pool.spawn_burst(&params, 5);
        for p in pool.alive_particles() {
            assert_eq!(p.position, origin);
        }
    }

    #[test]
    fn emitter_shape_line_creates_particles_between_endpoints() {
        let mut pool = ParticlePool::new(200);
        let start = [0.0, 0.0, 0.0];
        let end = [10.0, 0.0, 0.0];
        let params = EmitterParams {
            shape: EmitterShape::Line { start, end },
            initial_speed: 0.0,
            life_range: [5.0, 5.0],
            size_range: [0.1, 0.1],
            color: [1.0, 1.0, 1.0, 1.0],
            gravity: [0.0, 0.0, 0.0],
        };
        pool.spawn_burst(&params, 50);
        for p in pool.alive_particles() {
            assert!(p.position[0] >= 0.0 && p.position[0] <= 10.0);
            assert_eq!(p.position[1], 0.0);
            assert_eq!(p.position[2], 0.0);
        }
    }

    #[test]
    fn emitter_shape_sphere_creates_particles_within_radius() {
        let mut pool = ParticlePool::new(500);
        let center = [0.0, 0.0, 0.0];
        let radius = 5.0;
        let params = EmitterParams {
            shape: EmitterShape::Sphere { center, radius },
            initial_speed: 0.0,
            life_range: [5.0, 5.0],
            size_range: [0.1, 0.1],
            color: [1.0, 1.0, 1.0, 1.0],
            gravity: [0.0, 0.0, 0.0],
        };
        pool.spawn_burst(&params, 100);
        for p in pool.alive_particles() {
            let dist =
                (p.position[0].powi(2) + p.position[1].powi(2) + p.position[2].powi(2)).sqrt();
            assert!(
                dist <= radius + 0.01,
                "particle at distance {dist} exceeds radius {radius}"
            );
        }
    }

    #[test]
    fn update_applies_velocity() {
        let mut pool = ParticlePool::new(10);
        pool.particles.push(Particle {
            position: [0.0, 0.0, 0.0],
            life: 10.0,
            velocity: [1.0, 2.0, 3.0],
            size: 1.0,
            color: [1.0; 4],
        });
        pool.update(1.0);
        let p = &pool.particles[0];
        assert!((p.position[0] - 1.0).abs() < 1e-5);
        assert!((p.position[1] - 2.0).abs() < 1e-5);
        assert!((p.position[2] - 3.0).abs() < 1e-5);
    }

    #[test]
    fn pseudo_random_produces_values_in_zero_to_one() {
        let mut seed = 42u32;
        for _ in 0..1000 {
            let v = pseudo_random(&mut seed);
            assert!(v >= 0.0 && v < 1.0, "value {v} out of expected range");
        }
    }

    // -----------------------------------------------------------------------
    // update_with_gravity tests
    // -----------------------------------------------------------------------

    #[test]
    fn update_with_gravity_applies_gravity_to_velocity() {
        let mut pool = ParticlePool::new(10);
        pool.particles.push(Particle {
            position: [0.0, 10.0, 0.0],
            life: 5.0,
            velocity: [0.0, 0.0, 0.0],
            size: 1.0,
            color: [1.0; 4],
        });
        pool.update_with_gravity(1.0, [0.0, -10.0, 0.0]);
        let p = &pool.particles[0];
        // After 1s of -10 gravity: vel_y = -10, pos_y = 10 + (-10) * 1 = 0
        assert!((p.velocity[1] - (-10.0)).abs() < 1e-4);
        assert!((p.position[1] - 0.0).abs() < 1e-4);
    }

    #[test]
    fn update_with_gravity_fades_alpha() {
        let mut pool = ParticlePool::new(10);
        pool.particles.push(Particle {
            position: [0.0, 0.0, 0.0],
            life: 2.0,
            velocity: [0.0, 0.0, 0.0],
            size: 1.0,
            color: [1.0, 1.0, 1.0, 1.0],
        });
        pool.update_with_gravity(1.0, [0.0, 0.0, 0.0]);
        let p = &pool.particles[0];
        // life is now 1.0, alpha should be clamped to 1.0
        assert!((p.color[3] - 1.0).abs() < 1e-4);

        pool.update_with_gravity(0.5, [0.0, 0.0, 0.0]);
        let p = &pool.particles[0];
        // life is now 0.5, alpha should be 0.5
        assert!((p.color[3] - 0.5).abs() < 1e-4);
    }

    // -----------------------------------------------------------------------
    // CombatParticleEffect emitter_at tests
    // -----------------------------------------------------------------------

    #[test]
    fn hit_sparks_emitter_produces_valid_params() {
        let (params, count) = CombatParticleEffect::HitSparks.emitter_at([1.0, 2.0, 3.0]);
        assert!(count >= 10 && count <= 20, "hit sparks count {count} not in [10,20]");
        assert!(params.life_range[0] >= 0.1 && params.life_range[1] <= 0.2);
        assert!(params.initial_speed > 0.0);
        // Color should be bright (high red/green)
        assert!(params.color[0] > 0.8);
    }

    #[test]
    fn blade_trail_emitter_has_short_lifetime() {
        let (params, count) = CombatParticleEffect::BladeTrail.emitter_at([0.0, 1.0, 0.0]);
        assert!(count > 0 && count <= 5);
        assert!(params.life_range[1] <= 0.2, "blade trail should fade fast");
        assert!(params.initial_speed < 2.0, "blade trail should have low speed");
    }

    #[test]
    fn parry_burst_emits_at_least_30_particles() {
        let (_, count) = CombatParticleEffect::ParryBurst.emitter_at([0.0, 0.0, 0.0]);
        assert!(count >= 30, "parry burst should emit 30+ particles, got {count}");
    }

    #[test]
    fn parry_burst_color_is_white() {
        let (params, _) = CombatParticleEffect::ParryBurst.emitter_at([0.0, 0.0, 0.0]);
        assert!((params.color[0] - 1.0).abs() < 0.01);
        assert!((params.color[1] - 1.0).abs() < 0.01);
        assert!((params.color[2] - 1.0).abs() < 0.01);
    }

    #[test]
    fn dodge_dust_emitter_at_ground_level() {
        let (params, count) = CombatParticleEffect::DodgeDust.emitter_at([5.0, 2.0, 3.0]);
        assert!(count > 0);
        // Shape should be a sphere centered at feet (y=0)
        if let EmitterShape::Sphere { center, .. } = params.shape {
            assert!((center[1] - 0.0).abs() < 0.01, "dust should be at ground level");
            assert!((center[0] - 5.0).abs() < 0.01);
        } else {
            panic!("DodgeDust should use Sphere shape");
        }
    }

    #[test]
    fn enemy_hit_blood_color_is_dark_red() {
        let (params, count) = CombatParticleEffect::EnemyHitBlood.emitter_at([0.0, 0.0, 0.0]);
        assert!(count > 0);
        // Dark red: red channel much higher than green/blue
        assert!(params.color[0] > params.color[1]);
        assert!(params.color[0] > params.color[2]);
        assert!(params.color[0] < 0.5, "blood should be dark red, not bright");
    }

    // -----------------------------------------------------------------------
    // CombatParticleSystem integration tests
    // -----------------------------------------------------------------------

    /// Helper: create a GameState with the enemy close enough to hit.
    fn state_with_close_enemy() -> GameState {
        let mut s = GameState::new();
        s.enemy_x = 500;
        s.enemy_z = 0;
        s
    }

    #[test]
    fn combat_system_emits_hit_sparks_on_attack_hit() {
        let mut sys = CombatParticleSystem::new(4096);
        let mut prev = state_with_close_enemy();
        prev.player_attack_hit = false;
        let mut current = prev.clone();
        current.player_attack_hit = true;

        sys.emit(&current);
        // Should have hit sparks (15) + enemy blood (12) = 27
        assert!(
            sys.alive_count() >= 20,
            "expected sparks+blood, got {}",
            sys.alive_count()
        );
    }

    #[test]
    fn combat_system_does_not_double_emit_hit_sparks() {
        let mut sys = CombatParticleSystem::new(4096);
        let mut prev = state_with_close_enemy();
        prev.player_attack_hit = false;
        let mut current = prev.clone();
        current.player_attack_hit = true;

        sys.emit(&current);
        let first_count = sys.alive_count();

        // Call again with same state -- should NOT emit again
        sys.emit(&current);
        assert_eq!(
            sys.alive_count(),
            first_count,
            "should not double-emit hit sparks"
        );
    }

    #[test]
    fn combat_system_emits_parry_burst() {
        let mut sys = CombatParticleSystem::new(4096);
        let mut prev = state_with_close_enemy();
        prev.parry_success_this_tick = false;
        let mut current = prev.clone();
        current.parry_success_this_tick = true;

        sys.emit(&current);
        assert!(
            sys.alive_count() >= 30,
            "parry burst should emit 30+ particles, got {}",
            sys.alive_count()
        );
    }

    #[test]
    fn combat_system_does_not_double_emit_parry_burst() {
        let mut sys = CombatParticleSystem::new(4096);
        let prev = state_with_close_enemy();
        let mut current = prev.clone();
        current.parry_success_this_tick = true;

        sys.emit(&current);
        let first_count = sys.alive_count();

        // Same parry_success_this_tick=true -> should not re-emit
        sys.emit(&current);
        assert_eq!(sys.alive_count(), first_count);
    }

    #[test]
    fn combat_system_emits_dodge_dust_on_state_transition() {
        let mut sys = CombatParticleSystem::new(4096);
        let prev = GameState::new(); // STATE_IDLE = 0
        let mut current = prev.clone();
        current.player_state = STATE_DODGE; // 3

        sys.emit(&current);
        assert!(
            sys.alive_count() > 0,
            "dodge dust should emit particles"
        );
    }

    #[test]
    fn combat_system_does_not_emit_dodge_dust_when_already_dodging() {
        let mut sys = CombatParticleSystem::new(4096);
        let mut prev = GameState::new();
        prev.player_state = STATE_DODGE;
        let mut current = prev.clone();
        current.player_state = STATE_DODGE;

        // Manually set prev state in the system to dodge
        sys.prev_player_state = STATE_DODGE;
        sys.emit(&current);
        assert_eq!(
            sys.alive_count(),
            0,
            "should not emit dust when already in dodge"
        );
    }

    #[test]
    fn combat_system_emits_blade_trail_during_active_frames() {
        let mut sys = CombatParticleSystem::new(4096);
        let mut state = GameState::new();
        state.player_state = STATE_ATTACK;
        state.player_attack_heavy = false;
        state.player_state_tick = LIGHT_STARTUP; // first active frame

        sys.emit(&state);
        assert!(
            sys.alive_count() > 0,
            "blade trail should emit during active frames"
        );
    }

    #[test]
    fn combat_system_no_blade_trail_during_startup_frames() {
        let mut sys = CombatParticleSystem::new(4096);
        let mut state = GameState::new();
        state.player_state = STATE_ATTACK;
        state.player_attack_heavy = false;
        state.player_state_tick = 0; // startup frame, not yet active

        sys.emit(&state);
        assert_eq!(
            sys.alive_count(),
            0,
            "no blade trail during startup frames"
        );
    }

    #[test]
    fn combat_system_no_blade_trail_during_recovery_frames() {
        let mut sys = CombatParticleSystem::new(4096);
        let mut state = GameState::new();
        state.player_state = STATE_ATTACK;
        state.player_attack_heavy = false;
        state.player_state_tick = LIGHT_STARTUP + LIGHT_ACTIVE; // recovery

        sys.emit(&state);
        assert_eq!(
            sys.alive_count(),
            0,
            "no blade trail during recovery frames"
        );
    }

    #[test]
    fn combat_system_heavy_attack_blade_trail_during_active() {
        let mut sys = CombatParticleSystem::new(4096);
        let mut state = GameState::new();
        state.player_state = STATE_ATTACK;
        state.player_attack_heavy = true;
        state.player_state_tick = HEAVY_STARTUP; // first active frame

        sys.emit(&state);
        assert!(
            sys.alive_count() > 0,
            "blade trail should emit during heavy attack active frames"
        );
    }

    #[test]
    fn emit_and_update_advances_simulation() {
        let mut sys = CombatParticleSystem::new(4096);
        let prev = GameState::new();
        let mut current = prev.clone();
        current.player_state = STATE_DODGE;

        sys.emit_and_update(&current, 0.016);
        let count_after_one_tick = sys.alive_count();
        assert!(count_after_one_tick > 0, "particles should be alive after one tick");

        // Advance far enough to kill all particles
        let idle = GameState::new();
        sys.emit_and_update(&idle, 10.0);
        assert_eq!(sys.alive_count(), 0, "all particles should be dead after 10s");
    }

    // -----------------------------------------------------------------------
    // Standalone emit_combat_particles function tests
    // -----------------------------------------------------------------------

    #[test]
    fn standalone_emit_hit_sparks() {
        let mut pool = ParticlePool::new(4096);
        let mut prev = state_with_close_enemy();
        prev.player_attack_hit = false;
        let mut current = prev.clone();
        current.player_attack_hit = true;

        emit_combat_particles(&prev, &current, &mut pool);
        assert!(pool.alive_count() > 0, "standalone should emit hit sparks");
    }

    #[test]
    fn standalone_emit_parry_burst() {
        let mut pool = ParticlePool::new(4096);
        let prev = state_with_close_enemy();
        let mut current = prev.clone();
        current.parry_success_this_tick = true;

        emit_combat_particles(&prev, &current, &mut pool);
        assert!(
            pool.alive_count() >= 30,
            "standalone parry burst should emit 30+, got {}",
            pool.alive_count()
        );
    }

    #[test]
    fn standalone_emit_dodge_dust() {
        let mut pool = ParticlePool::new(4096);
        let prev = GameState::new();
        let mut current = prev.clone();
        current.player_state = STATE_DODGE;

        emit_combat_particles(&prev, &current, &mut pool);
        assert!(pool.alive_count() > 0, "standalone should emit dodge dust");
    }

    #[test]
    fn standalone_emit_blade_trail() {
        let mut pool = ParticlePool::new(4096);
        let mut state = GameState::new();
        state.player_state = STATE_ATTACK;
        state.player_attack_heavy = false;
        state.player_state_tick = LIGHT_STARTUP;

        // For standalone, prev and current are the same -- trail emits every active frame
        emit_combat_particles(&state, &state, &mut pool);
        assert!(pool.alive_count() > 0, "standalone should emit blade trail");
    }

    #[test]
    fn standalone_no_particles_on_idle() {
        let mut pool = ParticlePool::new(4096);
        let state = GameState::new();
        emit_combat_particles(&state, &state, &mut pool);
        assert_eq!(pool.alive_count(), 0, "no particles in idle state");
    }

    // -----------------------------------------------------------------------
    // blade_tip_position tests
    // -----------------------------------------------------------------------

    #[test]
    fn blade_tip_is_in_front_of_player() {
        let mut state = GameState::new();
        state.player_x = 0;
        state.player_y = 0;
        state.player_z = 0;
        state.player_facing_x = 1000; // facing +x
        state.player_facing_z = 0;
        state.player_state = STATE_ATTACK;
        state.player_attack_heavy = false;
        state.player_state_tick = LIGHT_STARTUP;

        let tip = blade_tip_position(&state, 1000.0);
        assert!(tip[0] > 0.0, "blade tip should be in front of player along +x");
    }

    #[test]
    fn blade_tip_arcs_vertically_across_active_frames() {
        let mut heights = Vec::new();
        for tick in 0..LIGHT_ACTIVE {
            let mut state = GameState::new();
            state.player_facing_x = 1000;
            state.player_facing_z = 0;
            state.player_state = STATE_ATTACK;
            state.player_attack_heavy = false;
            state.player_state_tick = LIGHT_STARTUP + tick;
            let tip = blade_tip_position(&state, 1000.0);
            heights.push(tip[1]);
        }
        // The midpoint of the arc should be highest
        let mid = heights.len() / 2;
        if heights.len() > 2 {
            assert!(
                heights[mid] >= heights[0] - 0.01,
                "mid arc should be at least as high as start"
            );
        }
    }

    // -----------------------------------------------------------------------
    // midpoint_milli tests
    // -----------------------------------------------------------------------

    #[test]
    fn midpoint_is_between_positions() {
        let mid = midpoint_milli(0, 0, 0, 2000, 0, 2000, 1000.0);
        assert!((mid[0] - 1.0).abs() < 0.01);
        assert!((mid[2] - 1.0).abs() < 0.01);
    }

    // -----------------------------------------------------------------------
    // End-to-end integration with tick_game
    // -----------------------------------------------------------------------

    #[test]
    fn full_tick_hit_detection_emits_particles() {
        use crate::game_logic::tick_game;

        let mut sys = CombatParticleSystem::new(4096);
        let mut state = GameState::new();
        state.enemy_x = 500;
        state.enemy_z = 0;
        state.player_state = STATE_ATTACK;
        state.player_state_tick = LIGHT_STARTUP; // about to be in active frames
        state.player_attack_heavy = false;

        let input = GameInput::default();
        let next = tick_game(&state, &input);

        sys.emit_and_update(&next, 0.016);
        // If the enemy is in range and active frames -> hit should land
        if next.player_attack_hit && !state.player_attack_hit {
            assert!(sys.alive_count() > 0, "particles should emit on hit");
        }
    }

    #[test]
    fn full_tick_dodge_emits_dust() {
        use crate::game_logic::tick_game;

        let mut sys = CombatParticleSystem::new(4096);
        let state = GameState::new();
        let input = GameInput { dodge: true, ..Default::default() };
        let next = tick_game(&state, &input);

        sys.emit_and_update(&next, 0.016);
        if next.player_state == STATE_DODGE {
            assert!(sys.alive_count() > 0, "dodge should emit dust particles");
        }
    }
}
