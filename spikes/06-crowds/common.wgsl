// Shared by every pass: the frame uniform, layout constants, the terrain's height, noise, counters.

struct Frame {
  eye: vec4f,   // xyz: camera; w: a pixel's angular size (radians per pixel at distance 1)
  cf: vec4f,    // camera forward
  cr: vec4f,    // camera right × tan(half fov x)
  cu: vec4f,    // camera up × tan(half fov y)
  sun: vec4f,   // xyz: toward the sun; w: time (s)
  lc: vec4f,    // light grid: xyz centre, w half-extent (m)
  lr: vec4f,    // light grid: right axis
  lu: vec4f,    // light grid: up axis
  dims: vec4u,  // width, height, fine tiles across, instance count
  grid: vec4u,  // light cells per side, coarse tiles across, coarse tiles down, edit count
  misc: vec4u,  // x: first row, z: first column (tiled rendering of heavy variants), y: fine tiles down
}

@group(0) @binding(0) var<uniform> F: Frame;

const INF: f32 = 1e30;
const BIG: f32 = 1e6;
const PI: f32 = 3.14159265;

// ---- binning ----------------------------------------------------------------------------------
const TILE: u32 = 8u;           // fine tile: 8×8 px
const CT: u32 = 8u;             // fine tiles per coarse tile side (coarse tile: 64×64 px)
const MAXE: u32 = 24u;          // entries per fine screen tile
const BIN_WORDS: u32 = 49u;     // count + (instance, part mask) × MAXE
const LMAXE: u32 = 16u;         // entries per fine light cell
const LBIN_WORDS: u32 = 33u;
const CMAX: u32 = 512u;         // entries per coarse list
const LGRID: u32 = 128u;        // fine light cells per side
const LCOARSE: u32 = 16u;       // coarse light cells per side
const PEN: f32 = 0.1;           // creature shadows: the penumbra reaches this far from the surface

// ---- posed instances (pose.wgsl writes, crowd.js mirrors) --------------------------------------
const STRIDE: u32 = 36u;        // vec4s per posed instance
const I_HDR: u32 = 0u;          // blend k, type, enabled-part bits, secondary-colour bits (16-bit masks stored as exact floats)
const I_COLA: u32 = 1u;         // primary colour (linear rgb)
const I_COLB: u32 = 2u;         // secondary colour
const I_BOUND: u32 = 3u;        // bound sphere (world)
const I_PARTS: u32 = 4u;        // 16 round cones, world space: (a.xyz, r1), (b.xyz, r2)
const NPARTS: u32 = 16u;
const S_STRIDE: u32 = 2u;       // root state per instance, vec4s (CPU → GPU every frame)
const P_STRIDE: u32 = 4u;       // static parameters per instance, vec4s (uploaded once)

// ---- the brick cache ----------------------------------------------------------------------------
const RMIN = vec3f(-48.0, -8.0, -48.0);
const RMAX = vec3f(48.0, 32.0, 48.0);
const RX: i32 = 96;
const RY: i32 = 40;
const RZ: i32 = 96;
const BRICK: u32 = 0x80000000u; // map entry: brick flag | slot; otherwise f16 centre distance in the low half
const SLOT_MASK: u32 = 0x00FFFFFFu;
const BAND: f32 = 1.0;          // a cell gets a brick when |distance at its centre| < BAND (≥ half its diagonal)
const MAXSKIP: f32 = 4.0;       // empty-cell distances are clamped to this (so additions only touch a bounded box)
const VPC: f32 = 8.0;           // voxels per cell: 12.5 cm
const BS: u32 = 9u;             // samples per brick side (corners; shared borders duplicated)
const AS: u32 = 32u;            // atlas slots per side
const ATLAS: f32 = 288.0;       // atlas texels per side
const CAP: u32 = 32768u;        // brick slots

fn cell_index(c: vec3i) -> u32 { return u32((c.y * RZ + c.z) * RX + c.x); }
fn cell_of_index(i: u32) -> vec3i {
  let x = i32(i % u32(RX));
  let z = i32((i / u32(RX)) % u32(RZ));
  let y = i32(i / u32(RX * RZ));
  return vec3i(x, y, z);
}
fn slot_origin(slot: u32) -> vec3u { return vec3u(slot % AS, (slot / AS) % AS, slot / (AS * AS)) * BS; }

// ---- stats (atomic counters; only the STATS variants write most of them) -----------------------------
const S_CREATURE_PX: u32 = 0u;
const S_WORLD_PX: u32 = 1u;
const S_SKY_PX: u32 = 2u;
const S_MARCHES: u32 = 3u;
const S_STEPS: u32 = 4u;
const S_MISSES: u32 = 5u;
const S_CAPS: u32 = 6u;
const S_SHADOW_PX: u32 = 7u;
const S_SH_MARCHES: u32 = 8u;
const S_SH_STEPS: u32 = 9u;
const S_SH_CAPS: u32 = 10u;
const S_W_STEPS: u32 = 11u;
const S_W_EMPTY: u32 = 12u;
const S_W_CAPS: u32 = 13u;
const S_WS_STEPS: u32 = 14u;
const S_WS_CAPS: u32 = 15u;
const S_T_STEPS: u32 = 16u;
const S_ENTRIES: u32 = 17u;
const S_PART_EVALS: u32 = 18u;
const S_HIT_STEPS: u32 = 19u;
const S_SCREEN_OVF: u32 = 20u;
const S_LIGHT_OVF: u32 = 21u;
const S_COARSE_OVF: u32 = 22u;
const S_COARSE_LIGHT_OVF: u32 = 23u;
const S_SPHERE_TESTS: u32 = 24u;
const S_COOK_CELLS: u32 = 25u;
const S_COOK_BRICKS: u32 = 26u;
const S_ALLOC_FAIL: u32 = 27u;
const S_FREED: u32 = 28u;
const S_FAR_PX: u32 = 29u;
const S_WS_MARCHES: u32 = 30u;
const S_RELEVANT_OVF: u32 = 31u;

struct Counters {
  steps: u32, parts: u32, caps: u32, misses: u32, marches: u32,
  sh_marches: u32, sh_steps: u32, sh_caps: u32,
  w_steps: u32, w_empty: u32, w_caps: u32,
  ws_steps: u32, ws_caps: u32, ws_marches: u32, t_steps: u32, entries: u32, spheres: u32,
}

// ---- terrain height (crowd.js has the same function, for placing walkers) --------------------------

const PLAZA = vec4f(-18.0, -14.0, 18.0, 18.0);   // xmin, zmin, xmax, zmax

fn rect_sd(x: f32, z: f32, r: vec4f) -> f32 {
  let c = (r.xy + r.zw) * 0.5;
  let h = (r.zw - r.xy) * 0.5;
  let q = abs(vec2f(x, z) - c) - h;
  return length(max(q, vec2f(0.0))) + min(max(q.x, q.y), 0.0);
}

/** Rolling ground, flattened under the square, rising into hills far away. */
fn terrain_h(x: f32, z: f32) -> f32 {
  var h = 0.9 * sin(0.061 * x + 0.4) * cos(0.047 * z + 1.1)
        + 0.45 * sin(0.13 * x + 1.7) * sin(0.11 * z + 0.3)
        + 0.12 * sin(0.37 * x) * cos(0.29 * z + 0.7);
  let r = sqrt(x * x + z * z);
  if (r > 60.0) {
    let a = atan2(z, x);
    h += 30.0 * smoothstep(60.0, 300.0, r) * (0.62 + 0.38 * sin(3.0 * a + 0.5) * cos(0.013 * r));
  }
  let m = 1.0 - smoothstep(0.0, 8.0, rect_sd(x, z, PLAZA));
  return mix(h, 0.05 + 0.004 * z, m);
}

/** Height and its gradient (x, z). The far hills' gradient is left out: they're zero inside the
 *  brick region, which is the only place this is used (to normalize the terrain's distance). */
fn terrain_hg(x: f32, z: f32) -> vec3f {
  let s1 = sin(0.061 * x + 0.4); let c1 = cos(0.061 * x + 0.4);
  let s2 = sin(0.047 * z + 1.1); let c2 = cos(0.047 * z + 1.1);
  let s3 = sin(0.13 * x + 1.7);  let c3 = cos(0.13 * x + 1.7);
  let s4 = sin(0.11 * z + 0.3);  let c4 = cos(0.11 * z + 0.3);
  let s5 = sin(0.37 * x);        let c5 = cos(0.37 * x);
  let s6 = sin(0.29 * z + 0.7);  let c6 = cos(0.29 * z + 0.7);
  var h = 0.9 * s1 * c2 + 0.45 * s3 * s4 + 0.12 * s5 * c6;
  var g = vec2f(0.9 * 0.061 * c1 * c2 + 0.45 * 0.13 * c3 * s4 + 0.12 * 0.37 * c5 * c6,
               -0.9 * 0.047 * s1 * s2 + 0.45 * 0.11 * s3 * c4 - 0.12 * 0.29 * s5 * s6);
  let r = sqrt(x * x + z * z);
  if (r > 60.0) {
    let a = atan2(z, x);
    h += 30.0 * smoothstep(60.0, 300.0, r) * (0.62 + 0.38 * sin(3.0 * a + 0.5) * cos(0.013 * r));
  }
  // The plaza blend: h' = mix(h, plaza, m), with m a smoothstep of the plaza rectangle's distance.
  let c = (PLAZA.xy + PLAZA.zw) * 0.5;
  let hh = (PLAZA.zw - PLAZA.xy) * 0.5;
  let rel = vec2f(x, z) - c;
  let q = abs(rel) - hh;
  let sd = length(max(q, vec2f(0.0))) + min(max(q.x, q.y), 0.0);
  var gsd: vec2f;
  if (max(q.x, q.y) > 0.0) {
    gsd = normalize(max(q, vec2f(1e-6))) * sign(rel);
  } else if (q.x > q.y) {
    gsd = vec2f(sign(rel.x), 0.0);
  } else {
    gsd = vec2f(0.0, sign(rel.y));
  }
  let t = clamp(sd / 8.0, 0.0, 1.0);
  let m = 1.0 - t * t * (3.0 - 2.0 * t);
  let dm = -6.0 * t * (1.0 - t) / 8.0;
  let plaza = 0.05 + 0.004 * z;
  let hb = mix(h, plaza, m);
  let gb = g * (1.0 - m) + vec2f(0.0, 0.004) * m + (plaza - h) * dm * gsd;
  return vec3f(hb, gb);
}

// ---- noise (spike 02's) ---------------------------------------------------------------------------

fn hash_u(x: u32) -> u32 {
  var h = x * 747796405u + 2891336453u;
  h = ((h >> ((h >> 28u) + 4u)) ^ h) * 277803737u;
  return (h >> 22u) ^ h;
}

fn hash3(c: vec3i) -> f32 {
  let u = bitcast<vec3u>(c);
  return f32(hash_u(u.x + hash_u(u.y + hash_u(u.z)))) * (1.0 / 4294967295.0);
}

fn hash2(c: vec2i) -> f32 {
  let u = bitcast<vec2u>(c);
  return f32(hash_u(u.x + hash_u(u.y + 0x9e3779b9u))) * (1.0 / 4294967295.0);
}

fn noise3(x: vec3f) -> f32 {
  let fl = floor(x);
  let i = vec3i(fl);
  let f = x - fl;
  let u = f * f * (3.0 - 2.0 * f);
  let a = hash3(i);
  let b = hash3(i + vec3i(1, 0, 0));
  let c = hash3(i + vec3i(0, 1, 0));
  let d = hash3(i + vec3i(1, 1, 0));
  let e = hash3(i + vec3i(0, 0, 1));
  let f1 = hash3(i + vec3i(1, 0, 1));
  let g = hash3(i + vec3i(0, 1, 1));
  let h = hash3(i + vec3i(1, 1, 1));
  return mix(mix(mix(a, b, u.x), mix(c, d, u.x), u.y), mix(mix(e, f1, u.x), mix(g, h, u.x), u.y), u.z);
}

fn noise2(x: vec2f) -> f32 {
  let fl = floor(x);
  let i = vec2i(fl);
  let f = x - fl;
  let u = f * f * (3.0 - 2.0 * f);
  return mix(mix(hash2(i), hash2(i + vec2i(1, 0)), u.x), mix(hash2(i + vec2i(0, 1)), hash2(i + vec2i(1, 1)), u.x), u.y);
}

fn fbm2(x: vec2f) -> f32 {
  return 0.5 * noise2(x) + 0.25 * noise2(x * 2.03 + 17.0) + 0.125 * noise2(x * 4.01 + 31.0) + 0.0625 * noise2(x * 8.05 + 5.0);
}

// ---- ray helpers ---------------------------------------------------------------------------------------

fn safe_inv(x: f32) -> f32 { return 1.0 / select(x, select(-1e-9, 1e-9, x >= 0.0), abs(x) < 1e-9); }

/** Ray vs axis-aligned box: (t_enter, t_exit); t_exit < t_enter when it misses. */
fn ray_box(o: vec3f, d: vec3f, lo: vec3f, hi: vec3f) -> vec2f {
  let inv = vec3f(safe_inv(d.x), safe_inv(d.y), safe_inv(d.z));
  let a = (lo - o) * inv;
  let b = (hi - o) * inv;
  let t0 = max(max(min(a.x, b.x), min(a.y, b.y)), min(a.z, b.z));
  let t1 = min(min(max(a.x, b.x), max(a.y, b.y)), max(a.z, b.z));
  return vec2f(t0, t1);
}

fn smin(a: f32, b: f32, k: f32) -> f32 {
  let h = clamp(0.5 + 0.5 * (b - a) / k, 0.0, 1.0);
  return mix(b, a, h) - k * h * (1.0 - h);
}

fn smax(a: f32, b: f32, k: f32) -> f32 { return -smin(-a, -b, k); }
